//! Lifts flat ASCII art into a solid and renders it back out as ASCII from any
//! angle.
//!
//! Port of `asciiary/Ascii3D.swift`.
//!
//! The renderers this follows all start from a mesh somebody modelled. This
//! starts from a text file, so it needs one step they do not: a rule for what
//! the third dimension of a drawing actually *is*. The rule is ink. A glyph
//! that fills more of its cell stands taller, so `@` rises, `.` barely lifts,
//! and a space is a hole.

use std::f64::consts::TAU;
use std::sync::Mutex;
use std::time::SystemTime;

use crate::art::canvas::{ink_coverage, AsciiCanvas, AsciiRamp, CELL_ASPECT};
use crate::art::generator::{Generator, GlyphGenerator};
use crate::art::params::Params;

#[derive(Clone, Copy, Debug)]
pub struct Vector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vector3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn normalized(self) -> Self {
        let length = (self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if length <= 1e-12 {
            return Self::new(0.0, 0.0, 1.0);
        }
        Self::new(self.x / length, self.y / length, self.z / length)
    }
}

/// One frame's rotation, resolved once.
///
/// Every vertex and every normal in a frame turns by the same two angles.
/// Computing the four trigonometric functions per vertex instead costs more
/// than the projection they feed.
struct Rotation {
    cos_yaw: f64,
    sin_yaw: f64,
    cos_pitch: f64,
    sin_pitch: f64,
}

impl Rotation {
    fn new(yaw: f64, pitch: f64) -> Self {
        Self {
            cos_yaw: yaw.cos(),
            sin_yaw: yaw.sin(),
            cos_pitch: pitch.cos(),
            sin_pitch: pitch.sin(),
        }
    }

    fn apply(&self, point: Vector3) -> Vector3 {
        let spun_x = point.x * self.cos_yaw + point.z * self.sin_yaw;
        let spun_z = -point.x * self.sin_yaw + point.z * self.cos_yaw;
        Vector3::new(
            spun_x,
            point.y * self.cos_pitch - spun_z * self.sin_pitch,
            point.y * self.sin_pitch + spun_z * self.cos_pitch,
        )
    }
}

/// One flat face of the solid, carrying the normal it should be lit by.
#[derive(Clone, Copy)]
struct Face {
    a: Vector3,
    b: Vector3,
    c: Vector3,
    d: Vector3,
    normal: Vector3,
}

/// The shape the frame is fitted to.
///
/// A drawing is a wide, shallow slab: the ball around it has to reach its far
/// corners, which would leave the drawing at roughly a third of the size the
/// pane could show. Fitting the slab instead uses the pane, and both extents
/// stay constant as the model turns, so a spin does not make it breathe.
enum Bound {
    Slab { half_width: f64, half_height: f64, half_depth: f64 },
}

impl Bound {
    fn extents(&self, pitch: f64) -> (f64, f64) {
        match *self {
            Self::Slab { half_width, half_height, half_depth } => {
                // Turning sweeps the slab's near corner out to here and no further.
                let horizontal = (half_width * half_width + half_depth * half_depth).sqrt();
                // Tipping trades the drawing's own height for that same sweep.
                let vertical =
                    pitch.cos().abs() * half_height + pitch.sin().abs() * horizontal;
                (horizontal.max(0.001), vertical.max(0.001))
            }
        }
    }
}

pub struct Solid {
    faces: Vec<Face>,
    radius: f64,
    bound: Bound,
}

impl Solid {
    /// `depth` is how far the heaviest glyph stands out, in cell widths.
    pub fn from_text(text: &str, depth: f64) -> Self {
        let normalized = text.replace("\r\n", "\n").replace('\t', "    ");
        let mut lines: Vec<Vec<char>> =
            normalized.split('\n').map(|line| line.chars().collect()).collect();

        // Trailing blank lines are an artifact of how the file was saved, not
        // part of the drawing, and would otherwise offset it in the frame.
        while lines
            .last()
            .is_some_and(|line| line.iter().all(|c| c.is_whitespace()))
        {
            lines.pop();
        }

        let rows = lines.len();
        let columns = lines.iter().map(Vec::len).max().unwrap_or(0);

        let half_width = columns as f64 / 2.0;
        let half_height = rows as f64 * CELL_ASPECT / 2.0;

        let mut heights = vec![0.0f64; rows * columns];
        for (row, line) in lines.iter().enumerate() {
            for (column, character) in line.iter().enumerate() {
                heights[row * columns + column] = ink_coverage(*character) * depth;
            }
        }

        // The drawing is lifted from a floor at zero, so the solid occupies
        // `0..tallest` and nothing sits at the origin the frame is rotated
        // about. Left that way the model orbits the frame instead of spinning
        // in place, and a pitched render walks off the bottom edge. Halving the
        // tallest column and hanging the solid around that puts its own middle
        // on the axis. Measuring the drawing rather than `depth` also keeps the
        // fit tight when nothing in it reaches full ink.
        let tallest = heights.iter().copied().fold(0.0, f64::max);
        let half_depth = tallest / 2.0;

        let bound = Bound::Slab { half_width, half_height, half_depth };
        let radius = (half_width * half_width
            + half_height * half_height
            + half_depth * half_depth)
            .sqrt();

        if rows == 0 || columns == 0 {
            return Self { faces: Vec::new(), radius: radius.max(0.001), bound };
        }

        Self {
            faces: build_faces(&heights, rows, columns, half_depth),
            radius: radius.max(0.001),
            bound,
        }
    }
}

/// Emits only the faces that are actually exposed. An interior wall between two
/// equally tall cells can never be seen, and skipping it keeps the face count
/// proportional to the drawing's silhouette rather than its area.
///
/// `sink` is how far every vertex drops so the solid straddles the origin;
/// heights themselves stay measured from the drawing's own floor, which is what
/// the neighbour comparisons below are about.
fn build_faces(heights: &[f64], rows: usize, columns: usize, sink: f64) -> Vec<Face> {
    let mut faces = Vec::with_capacity(rows * columns * 2);

    let half_width = (columns as f64 - 1.0) / 2.0;
    let half_height = (rows as f64 - 1.0) / 2.0;
    let half_cell = CELL_ASPECT / 2.0;

    let height = |row: i64, column: i64| -> f64 {
        if row < 0 || row >= rows as i64 || column < 0 || column >= columns as i64 {
            return 0.0;
        }
        heights[row as usize * columns + column as usize]
    };

    for row in 0..rows {
        for column in 0..columns {
            let top = height(row as i64, column as i64);
            if top <= 0.0 {
                continue;
            }

            let x0 = column as f64 - half_width - 0.5;
            let x1 = x0 + 1.0;
            let center_y = (half_height - row as f64) * CELL_ASPECT;
            let y0 = center_y - half_cell;
            let y1 = center_y + half_cell;

            // Cap.
            faces.push(Face {
                a: Vector3::new(x0, y0, top - sink),
                b: Vector3::new(x1, y0, top - sink),
                c: Vector3::new(x1, y1, top - sink),
                d: Vector3::new(x0, y1, top - sink),
                normal: Vector3::new(0.0, 0.0, 1.0),
            });

            // Base, so the solid still reads as one when tipped past edge-on.
            faces.push(Face {
                a: Vector3::new(x0, y0, -sink),
                b: Vector3::new(x1, y0, -sink),
                c: Vector3::new(x1, y1, -sink),
                d: Vector3::new(x0, y1, -sink),
                normal: Vector3::new(0.0, 0.0, -1.0),
            });

            // Walls, one per neighbour this cell stands above. Starting the wall
            // at the neighbour's height rather than at zero is what makes
            // terraced art show its steps.
            let walls = [
                (height(row as i64, column as i64 - 1), Vector3::new(-1.0, 0.0, 0.0), (x0, y0), (x0, y1)),
                (height(row as i64, column as i64 + 1), Vector3::new(1.0, 0.0, 0.0), (x1, y0), (x1, y1)),
                (height(row as i64 - 1, column as i64), Vector3::new(0.0, 1.0, 0.0), (x0, y1), (x1, y1)),
                (height(row as i64 + 1, column as i64), Vector3::new(0.0, -1.0, 0.0), (x0, y0), (x1, y0)),
            ];

            for (neighbour, normal, p, q) in walls {
                if top <= neighbour {
                    continue;
                }
                faces.push(Face {
                    a: Vector3::new(p.0, p.1, neighbour - sink),
                    b: Vector3::new(q.0, q.1, neighbour - sink),
                    c: Vector3::new(q.0, q.1, top - sink),
                    d: Vector3::new(p.0, p.1, top - sink),
                    normal,
                });
            }
        }
    }

    faces
}

/// Projects a solid onto a character grid with a depth buffer.
pub struct Renderer {
    pub solid: Solid,
    pub yaw: f64,
    pub pitch: f64,
    pub zoom: f64,
    /// Glyphs the surface is shaded with. A drawing lifted into a solid keeps
    /// the ink ramp it was read with, so head-on it reproduces itself.
    pub ramp: &'static [u8],
    /// Fixed key light, up and to the left of the camera.
    pub light: Vector3,
    /// Floor under the shading so faces turned away stay legible instead of
    /// dropping to blank cells and tearing holes in the silhouette.
    pub ambient: f64,
    /// How much of the shade comes from distance rather than from the light.
    ///
    /// Flat-topped columns all share one normal, so lighting alone paints every
    /// cap the same glyph and the relief disappears into a slab. Mixing in
    /// nearness restores it, and is faithful to the source: a taller column is
    /// a nearer one, so head-on the render reproduces the drawing's own ink.
    pub depth_cueing: f64,
}

impl Renderer {
    pub fn new(solid: Solid) -> Self {
        Self {
            solid,
            yaw: 0.0,
            pitch: 0.0,
            zoom: 1.0,
            ramp: AsciiRamp::Ink.bytes(),
            light: Vector3::new(-0.45, 0.65, 1.0).normalized(),
            ambient: 0.18,
            depth_cueing: 0.45,
        }
    }

    /// `yaw` is passed rather than read from the field so one prepared renderer
    /// can serve every frame of a spin without being rebuilt.
    pub fn canvas_at(&self, columns: usize, rows: usize, yaw: f64) -> AsciiCanvas {
        let mut canvas = AsciiCanvas::new(columns, rows, false);
        if columns == 0 || rows == 0 || self.solid.faces.is_empty() {
            return canvas;
        }

        let mut depths = vec![f64::NEG_INFINITY; columns * rows];
        let rotation = Rotation::new(yaw, self.pitch);
        let radius = self.solid.radius;
        let (horizontal, vertical) = self.solid.bound.extents(self.pitch);
        let scale = (columns as f64 / (2.0 * horizontal))
            .min(rows as f64 * CELL_ASPECT / (2.0 * vertical))
            * self.zoom;
        let center_x = (columns as f64 - 1.0) / 2.0;
        let center_y = (rows as f64 - 1.0) / 2.0;
        let vertical_scale = scale / CELL_ASPECT;

        let project = |point: Vector3| -> Vector3 {
            let spun = rotation.apply(point);
            Vector3::new(
                center_x + spun.x * scale,
                center_y - spun.y * vertical_scale,
                spun.z,
            )
        };

        for face in &self.solid.faces {
            let normal = rotation.apply(face.normal);
            // Back faces are deliberately *not* culled. The solid is closed
            // enough that culling them looks safe, but at these grid sizes a
            // face lands on a cell centre rather than on a pixel, and dropping
            // the back half measurably changed which glyph won several cells.
            // Faces are lit by how squarely they meet the light regardless of
            // which way they point.
            let lambert = normal.dot(self.light).abs();

            let a = project(face.a);
            let b = project(face.b);
            let c = project(face.c);
            let d = project(face.d);

            let nearness =
                (((a.z + b.z + c.z + d.z) / 4.0 / radius + 1.0) / 2.0).clamp(0.0, 1.0);
            let shade = (1.0 - self.depth_cueing) * lambert + self.depth_cueing * nearness;
            let glyph = AsciiRamp::byte_for_intensity(
                self.ambient + (1.0 - self.ambient) * shade,
                self.ramp,
            );

            fill(a, b, c, glyph, &mut canvas, &mut depths);
            fill(a, c, d, glyph, &mut canvas, &mut depths);

            // A face smaller than one cell can fall between cell centres and
            // vanish. Its centre always lands inside it, so plotting that too
            // guarantees every face contributes at least one character.
            plot(
                Vector3::new(
                    (a.x + b.x + c.x + d.x) / 4.0,
                    (a.y + b.y + c.y + d.y) / 4.0,
                    (a.z + b.z + c.z + d.z) / 4.0,
                ),
                glyph,
                &mut canvas,
                &mut depths,
            );
        }

        canvas
    }
}

fn plot(point: Vector3, glyph: u8, canvas: &mut AsciiCanvas, depths: &mut [f64]) {
    let column = point.x.round();
    let row = point.y.round();
    if column < 0.0 || column >= canvas.columns as f64 || row < 0.0 || row >= canvas.rows as f64 {
        return;
    }
    let index = row as usize * canvas.columns + column as usize;
    if point.z <= depths[index] {
        return;
    }
    depths[index] = point.z;
    canvas.glyphs[index] = glyph;
}

fn fill(a: Vector3, b: Vector3, c: Vector3, glyph: u8, canvas: &mut AsciiCanvas, depths: &mut [f64]) {
    fn edge(p: Vector3, q: Vector3, x: f64, y: f64) -> f64 {
        (q.x - p.x) * (y - p.y) - (q.y - p.y) * (x - p.x)
    }

    let area = edge(a, b, c.x, c.y);
    if area.abs() <= 1e-9 {
        return;
    }

    let min_column = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
    let max_column = (a.x.max(b.x).max(c.x).ceil()).min(canvas.columns as f64 - 1.0);
    let min_row = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
    let max_row = (a.y.max(b.y).max(c.y).ceil()).min(canvas.rows as f64 - 1.0);
    if max_column < 0.0 || max_row < 0.0 {
        return;
    }
    let max_column = max_column as usize;
    let max_row = max_row as usize;
    if min_column > max_column || min_row > max_row {
        return;
    }

    for row in min_row..=max_row {
        for column in min_column..=max_column {
            let x = column as f64;
            let y = row as f64;
            // Dividing by the signed area normalises the winding, so a point is
            // inside whenever all three weights are non-negative.
            let weight_a = edge(b, c, x, y) / area;
            let weight_b = edge(c, a, x, y) / area;
            let weight_c = edge(a, b, x, y) / area;
            if weight_a < -1e-9 || weight_b < -1e-9 || weight_c < -1e-9 {
                continue;
            }

            let z = weight_a * a.z + weight_b * b.z + weight_c * c.z;
            let index = row * canvas.columns + column;
            if z <= depths[index] {
                continue;
            }
            depths[index] = z;
            canvas.glyphs[index] = glyph;
        }
    }
}

/// The animated tool: the solid spins about its vertical axis.
///
/// A full turn is the loop, so `loop_duration` is exactly the period. That is
/// what lets the exporter sample a seamless GIF without the generator knowing
/// it is being exported.
pub struct SpinningAscii {
    renderer: Renderer,
    spin_rate: f64,
    still: bool,
}

impl GlyphGenerator for SpinningAscii {
    fn canvas(&self, columns: usize, rows: usize, time: f64) -> AsciiCanvas {
        let yaw = if self.still {
            self.renderer.yaw
        } else {
            self.renderer.yaw + time * self.spin_rate
        };
        self.renderer.canvas_at(columns, rows, yaw)
    }

    fn loop_duration(&self) -> Option<f64> {
        if self.still || self.spin_rate.abs() < 1e-9 {
            None
        } else {
            Some(TAU / self.spin_rate.abs())
        }
    }
}

/// What a change to the drawing on disk would move.
type Stamp = (u64, Option<SystemTime>);

struct Cached {
    path: String,
    stamp: Stamp,
    text: String,
}

/// The drawing most recently read.
///
/// The window rebuilds this generator from scratch on every preview frame, so
/// dragging a slider used to re-read the file off disk a dozen times a second
/// for a drawing that had not changed. One entry is all that is wanted: the
/// window shows one drawing at a time. Keyed on the file's length and
/// modification time rather than held forever, so editing the drawing in
/// another window still shows up in the preview.
static LAST_READ: Mutex<Option<Cached>> = Mutex::new(None);

fn read_drawing(path: &str) -> Result<String, String> {
    let stamp: Stamp = std::fs::metadata(path)
        .map(|data| (data.len(), data.modified().ok()))
        .map_err(|error| format!("cannot read `{path}`: {error}"))?;

    // A panic while the lock was held would have left it poisoned; the cache is
    // only ever a copy of a file, so there is nothing to protect by refusing.
    let mut cache = LAST_READ.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(cached) = cache.as_ref() {
        if cached.path == path && cached.stamp == stamp {
            return Ok(cached.text.clone());
        }
    }

    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read `{path}`: {error}"))?;
    *cache = Some(Cached { path: path.to_string(), stamp, text: text.clone() });
    Ok(text)
}

/// Defaults match the Swift CLI (`asciiary +3d`), not the GUI sliders — the two
/// disagreed in the original and the CLI is the one this command line inherits.
pub fn build(params: &Params) -> Result<Generator, String> {
    // `--text` carries a drawing inline, which is how the window offers a sample
    // without shipping a file whose path differs between dev and a bundle.
    let text = match params.string("text") {
        Some(inline) => inline.to_string(),
        None => {
            let path = params
                .first_positional()
                .ok_or("ascii needs a .txt drawing to lift")?;
            read_drawing(path)?
        }
    };

    let depth = params.f64("depth", 8.0)?;
    let mut renderer = Renderer::new(Solid::from_text(&text, depth));
    renderer.yaw = params.f64("yaw", 0.6_f64.to_degrees())?.to_radians();
    renderer.pitch = params.f64("pitch", 0.5_f64.to_degrees())?.to_radians();
    renderer.zoom = params.f64("zoom", 0.92)?;

    Ok(Generator::Glyph(Box::new(SpinningAscii {
        renderer,
        spin_rate: params.f64("spin", 1.2)?,
        still: params.is_set("still"),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::canvas::SPACE;

    const DIAMOND: &str = "\
   @@@
  @###@
 @#+++#@
@#+...+#@
 @#+++#@
  @###@
   @@@   ";

    /// The window's defaults, so a test measures what somebody actually sees.
    fn render(text: &str, yaw_degrees: f64, columns: usize, rows: usize) -> AsciiCanvas {
        let mut renderer = Renderer::new(Solid::from_text(text, 8.0));
        renderer.pitch = 0.5;
        renderer.zoom = 0.92;
        renderer.canvas_at(columns, rows, yaw_degrees.to_radians())
    }

    /// Leftmost, rightmost, topmost and bottommost cell holding anything.
    fn inked_bounds(canvas: &AsciiCanvas) -> Option<(usize, usize, usize, usize)> {
        let drawn: Vec<(usize, usize)> = (0..canvas.rows)
            .flat_map(|row| (0..canvas.columns).map(move |column| (column, row)))
            .filter(|&(column, row)| canvas.get(column, row) != SPACE)
            .collect();
        let (first, _) = drawn.split_first()?;
        let mut bounds = (first.0, first.0, first.1, first.1);
        for (column, row) in &drawn {
            bounds.0 = bounds.0.min(*column);
            bounds.1 = bounds.1.max(*column);
            bounds.2 = bounds.2.min(*row);
            bounds.3 = bounds.3.max(*row);
        }
        Some(bounds)
    }

    /// A drawing is lifted off a floor at zero, so nothing sits at the point the
    /// frame turns about unless the solid is deliberately hung around it.
    /// Without that it orbits the frame as it spins and a pitched view walks off
    /// the bottom edge — which is what this drifted into once already.
    #[test]
    fn the_solid_turns_about_the_middle_of_the_frame() {
        let (columns, rows) = (60, 20);
        for yaw in [0.0, 45.0, 90.0, 135.0, 180.0, 270.0, 359.0] {
            let canvas = render(DIAMOND, yaw, columns, rows);
            let (left, right, top, bottom) =
                inked_bounds(&canvas).expect("the diamond draws something");

            let x = (left + right) as f64 / 2.0;
            let y = (top + bottom) as f64 / 2.0;
            let (frame_x, frame_y) = ((columns as f64 - 1.0) / 2.0, (rows as f64 - 1.0) / 2.0);

            assert!(
                (x - frame_x).abs() <= 1.0,
                "at yaw {yaw} the drawing sits at column {x}, not {frame_x}"
            );
            assert!(
                (y - frame_y).abs() <= 1.0,
                "at yaw {yaw} the drawing sits at row {y}, not {frame_y}"
            );
        }
    }

    /// The zoom the window opens on leaves a margin. Losing it means the fit is
    /// measuring an extent the solid no longer has.
    #[test]
    fn nothing_runs_off_the_edge_at_the_default_zoom() {
        let (columns, rows) = (60, 20);
        for yaw in [0.0, 45.0, 90.0, 135.0, 180.0] {
            let canvas = render(DIAMOND, yaw, columns, rows);
            let (left, right, top, bottom) = inked_bounds(&canvas).expect("something is drawn");
            assert!(
                left > 0 && right < columns - 1 && top > 0 && bottom < rows - 1,
                "at yaw {yaw} the render touches the frame edge: \
                 columns {left}..{right} of {columns}, rows {top}..{bottom} of {rows}"
            );
        }
    }

    /// Blank lines at the end of a file are how it was saved, not part of the
    /// drawing, and counting them would push the drawing up out of centre.
    #[test]
    fn trailing_blank_lines_are_not_part_of_the_drawing() {
        let plain = Solid::from_text(DIAMOND, 8.0);
        let padded = Solid::from_text(&format!("{DIAMOND}\n\n   \n\n"), 8.0);
        assert_eq!(plain.faces.len(), padded.faces.len());
        assert!((plain.radius - padded.radius).abs() < 1e-9);
    }

    /// A file of spaces is a legitimate thing to open, and every stage below has
    /// to survive being handed a solid with nothing in it.
    #[test]
    fn a_drawing_with_no_ink_draws_nothing() {
        let canvas = render("    \n    \n", 0.0, 20, 8);
        assert!(inked_bounds(&canvas).is_none());
        assert!(canvas.glyphs.iter().all(|&glyph| glyph == SPACE));
    }

    #[test]
    fn a_zero_sized_grid_is_not_a_panic() {
        assert_eq!(render(DIAMOND, 0.0, 0, 0).glyphs.len(), 0);
        assert_eq!(render(DIAMOND, 0.0, 10, 0).glyphs.len(), 0);
        assert_eq!(render(DIAMOND, 0.0, 0, 10).glyphs.len(), 0);
    }

    /// Heavier ink stands taller — the one rule that decides what the third
    /// dimension of a flat drawing even is.
    #[test]
    fn heavier_ink_stands_taller() {
        let solid = |glyph: char| {
            Solid::from_text(&format!("{glyph}"), 8.0)
                .faces
                .iter()
                .map(|face| face.a.z)
                .fold(f64::NEG_INFINITY, f64::max)
        };
        assert!(solid('@') > solid('#'));
        assert!(solid('#') > solid('.'));
        assert!(Solid::from_text(" ", 8.0).faces.is_empty());
    }

    /// A whole turn is the loop, and the exporter samples exactly that span. A
    /// wrong answer here is a GIF with a visible jump at the seam.
    ///
    /// The seam closes to within a few cells rather than exactly. `period *
    /// spin_rate` lands one bit off `TAU`, and a cell where two faces meet at
    /// the same depth then breaks its tie the other way. Five cells in five
    /// hundred is invisible; the wrong period is not, which is what the second
    /// half of this measures.
    #[test]
    fn a_spin_loops_over_one_full_turn() {
        let spinning = SpinningAscii {
            renderer: Renderer::new(Solid::from_text(DIAMOND, 8.0)),
            spin_rate: 1.2,
            still: false,
        };
        let period = spinning.loop_duration().expect("a spin has a period");
        assert!((period - TAU / 1.2).abs() < 1e-9);

        let moved_after = |seconds: f64| {
            let start = spinning.canvas(40, 14, 0.0);
            let later = spinning.canvas(40, 14, seconds);
            start
                .glyphs
                .iter()
                .zip(&later.glyphs)
                .filter(|(before, after)| before != after)
                .count()
        };

        let seam = moved_after(period);
        assert!(seam < 560 / 50, "the loop does not close: {seam} cells moved");
        assert!(
            moved_after(period * 0.9) > seam * 10,
            "the seam tolerance is wide enough to hide a wrong period"
        );
    }

    #[test]
    fn a_still_has_no_loop() {
        let still = SpinningAscii {
            renderer: Renderer::new(Solid::from_text(DIAMOND, 8.0)),
            spin_rate: 1.2,
            still: true,
        };
        assert_eq!(still.loop_duration(), None);
        assert_eq!(still.canvas(40, 14, 0.0).glyphs, still.canvas(40, 14, 9.0).glyphs);
    }

    /// The cache is what stops the preview from going back to disk, so the risk
    /// it carries is the opposite one: a drawing edited in another window still
    /// showing its old shape until the app is restarted.
    #[test]
    fn an_edited_drawing_is_read_again() {
        let path = std::env::temp_dir().join(format!("asciiary-{}.txt", std::process::id()));
        let path = path.to_str().expect("temp path is utf-8");

        std::fs::write(path, "@@@\n").expect("first write");
        assert_eq!(read_drawing(path).expect("first read"), "@@@\n");
        assert_eq!(read_drawing(path).expect("cached read"), "@@@\n");

        std::fs::write(path, "...\n..\n").expect("second write");
        assert_eq!(read_drawing(path).expect("read after edit"), "...\n..\n");

        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn a_missing_drawing_says_so() {
        let error = read_drawing("/nowhere/at/all.txt").expect_err("no such file");
        assert!(error.contains("cannot read"), "unhelpful message: {error}");
    }
}
