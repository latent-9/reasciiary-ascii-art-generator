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
            std::fs::read_to_string(path)
                .map_err(|error| format!("cannot read `{path}`: {error}"))?
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
