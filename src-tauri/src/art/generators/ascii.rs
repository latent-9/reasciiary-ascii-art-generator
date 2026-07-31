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

use rayon::prelude::*;

use crate::art::canvas::{ink_coverage, AsciiCanvas, CELL_ASPECT};
use crate::art::generator::{Generator, GlyphGenerator};
use crate::art::glyphs::{ALPHABET, CELL_PIXELS, CELL_PIXELS_TALL, CELL_PIXELS_WIDE};
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

    fn negated(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
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

/// One flat face of the solid, carrying the normal it should be lit by and how
/// much of the light around it ever reaches it.
#[derive(Clone, Copy)]
struct Face {
    a: Vector3,
    b: Vector3,
    c: Vector3,
    d: Vector3,
    normal: Vector3,
    openness: f64,
}

/// Yaws the fit is measured at. Fine enough that no corner slips between two of
/// them by more than the rounding a character cell already imposes.
const TURN_STEPS: usize = 180;

/// Points on the rim of a round bound. Its extremes all lie on a rim, so this
/// is the whole of what a fit has to look at.
const RIM_STEPS: usize = 64;

/// The shape the frame is fitted to.
///
/// A drawing is a wide, shallow slab: the ball around it has to reach its far
/// corners, which would leave the drawing at roughly a third of the size the
/// pane could show. Fitting the slab instead uses the pane, and both extents
/// stay constant as the model turns, so a spin does not make it breathe.
enum Bound {
    Slab { half_width: f64, half_height: f64, half_depth: f64 },
    /// A relief cut on a disc rather than on a rectangle.
    Puck { radius: f64, half_depth: f64 },
}

impl Bound {
    /// The points every question about room is answered by.
    ///
    /// Each of the four below used to carry its own closed form, derived off
    /// the slab's eight corners. They were right, and they were four chances to
    /// be wrong about the next shape — and the slab's hide an assumption a
    /// round solid does not keep: that whatever reaches furthest across also
    /// reaches furthest back. A disc is thin exactly where it is widest, so a
    /// fit that assumes a corner out there holds a third of the frame back for
    /// something the disc has never had.
    fn hull(&self) -> Vec<Vector3> {
        match *self {
            Self::Slab { half_width, half_height, half_depth } => {
                let mut points = Vec::with_capacity(8);
                for x in [-half_width, half_width] {
                    for y in [-half_height, half_height] {
                        for z in [-half_depth, half_depth] {
                            points.push(Vector3::new(x, y, z));
                        }
                    }
                }
                points
            }
            Self::Puck { radius, half_depth } => (0..RIM_STEPS)
                .flat_map(|step| {
                    let angle = TAU * step as f64 / RIM_STEPS as f64;
                    let (x, y) = (radius * angle.cos(), radius * angle.sin());
                    [-half_depth, half_depth].map(|z| Vector3::new(x, y, z))
                })
                .collect(),
        }
    }

    /// Shows `look` every hull point at every yaw the fit has to hold for.
    fn sweep(&self, pitch: f64, yaws: &[f64], mut look: impl FnMut(Vector3)) {
        let hull = self.hull();
        for &yaw in yaws {
            let rotation = Rotation::new(yaw, pitch);
            for point in &hull {
                look(rotation.apply(*point));
            }
        }
    }

    fn extents(&self, pitch: f64, yaws: &[f64]) -> (f64, f64) {
        let (mut horizontal, mut vertical) = (0.001_f64, 0.001_f64);
        self.sweep(pitch, yaws, |spun| {
            horizontal = horizontal.max(spun.x.abs());
            vertical = vertical.max(spun.y.abs());
        });
        (horizontal, vertical)
    }

    /// The furthest in front of its middle the solid ever reaches —
    /// [`Self::depth_extent`] at its worst yaw.
    fn depth_reach(&self, pitch: f64, yaws: &[f64]) -> f64 {
        let mut reach = 0.001_f64;
        self.sweep(pitch, yaws, |spun| reach = reach.max(spun.z.abs()));
        reach
    }

    /// Half the width and half the height the solid covers on screen at unit
    /// scale, seen from `eye` away, swept over every yaw the solid will be seen
    /// at so the fit holds still rather than breathing as it spins.
    ///
    /// The hull goes through the same projection the faces do, which starts to
    /// matter once the camera converges: a near point is drawn larger than the
    /// box it came from says. Allowing for that the cheap way — taking the
    /// parallel extents and scaling them by the most any point can possibly
    /// gain — hands a fifth of the frame to a margin nothing reaches into, and
    /// on a grid this coarse a fifth of the frame is a lot of detail to lose.
    fn screen_extents(&self, pitch: f64, yaws: &[f64], eye: f64) -> (f64, f64) {
        let (mut horizontal, mut vertical) = (0.001_f64, 0.001_f64);
        self.sweep(pitch, yaws, |spun| {
            let converge = eye / (eye - spun.z);
            horizontal = horizontal.max((spun.x * converge).abs());
            vertical = vertical.max((spun.y * converge).abs());
        });
        (horizontal, vertical)
    }

    /// How far the nearest point of the turned solid stands in front of its
    /// middle — the half-extent along the axis the camera looks down.
    ///
    /// The bounding sphere's radius answers a nearby question and was what the
    /// shading used, but it is the wrong number: a drawing is wide and shallow,
    /// so its sphere is far larger than its depth and every point in it landed
    /// within a few percent of the middle. Depth cueing then had almost no range
    /// to work with and the relief it exists to bring out barely showed.
    fn depth_extent(&self, yaw: f64, pitch: f64) -> f64 {
        let rotation = Rotation::new(yaw, pitch);
        self.hull()
            .into_iter()
            .map(|point| rotation.apply(point).z.abs())
            .fold(0.001, f64::max)
    }
}

pub struct Solid {
    faces: Vec<Face>,
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

        // The heaviest column reaches half this far either side of the origin
        // the frame is rotated about — see [`build_faces`]. Measuring the
        // drawing rather than `depth` keeps the fit tight when nothing in it
        // reaches full ink.
        let tallest = heights.iter().copied().fold(0.0, f64::max);
        let half_depth = tallest / 2.0;

        let bound = Bound::Slab { half_width, half_height, half_depth };
        if rows == 0 || columns == 0 {
            return Self { faces: Vec::new(), bound };
        }

        Self { faces: build_faces(&heights, rows, columns), bound }
    }
}

/// Emits only the faces that are actually exposed. An interior wall between two
/// equally tall cells can never be seen, and skipping it keeps the face count
/// proportional to the drawing's silhouette rather than its area.
fn build_faces(heights: &[f64], rows: usize, columns: usize) -> Vec<Face> {
    let mut faces = Vec::with_capacity(rows * columns * 2);

    let half_width = (columns as f64 - 1.0) / 2.0;
    let half_height = (rows as f64 - 1.0) / 2.0;
    let half_cell = CELL_ASPECT / 2.0;

    let height = |row: i64, column: i64| -> f64 { surface(heights, rows, columns, row, column) };
    let openness = openness(heights, rows, columns);

    // Which way the drawing is sloping under this cell, as the normal of that
    // slope. A cap used to be handed a flat `(0, 0, 1)` — true of the box it
    // sits on, but not of the drawing, and it meant every cap in the model took
    // exactly the same light however the ink around it ran. The relief only
    // survived as a step at the silhouette, which is why a third of the shade
    // had to be bought back from distance instead.
    //
    // The gradient is a Sobel rather than a difference of the two neighbours
    // either side: art is often dithered, and a stencil one cell wide reads
    // `#@#@` as a cliff a cell, so the surface breaks up into noise. Weighting
    // the diagonals in asks the same question of a three by three patch, which
    // alternating ink answers the way the eye does — as flat.
    let slope = |row: i64, column: i64| -> Vector3 {
        let at = |dr: i64, dc: i64| height(row + dr, column + dc);
        let run = |dc: i64| at(-1, dc) + 2.0 * at(0, dc) + at(1, dc);
        let rise = |dr: i64| at(dr, -1) + 2.0 * at(dr, 0) + at(dr, 1);

        // Columns are one apart, rows `CELL_ASPECT`, and a row's y falls as its
        // index climbs — so `row - 1` is the one further up the picture.
        let per_x = (run(1) - run(-1)) / 8.0;
        let per_y = (rise(-1) - rise(1)) / (8.0 * CELL_ASPECT);
        Vector3::new(-per_x, -per_y, 1.0).normalized()
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

            // Both sides, each tilted into the light by the slope it sits on.
            // The far one is the near one's mirror, so it leans the same way
            // across the picture and the opposite way through it.
            let near = slope(row as i64, column as i64);
            let far = Vector3::new(near.x, near.y, -near.z);
            let sky = openness[row * columns + column];
            for (z, normal) in [(top, near), (-top, far)] {
                faces.push(Face {
                    a: Vector3::new(x0, y0, z),
                    b: Vector3::new(x1, y0, z),
                    c: Vector3::new(x1, y1, z),
                    d: Vector3::new(x0, y1, z),
                    normal,
                    openness: sky,
                });
            }

            // Walls, one per neighbour this cell stands proud of. A neighbour
            // standing `n` tall leaves this column's side open above it and,
            // struck through, an equal band below — with nothing between them,
            // which is where the neighbour's own body is. Starting a wall at the
            // neighbour rather than at nothing is what makes terraced art show
            // its steps.
            let walls = [
                ((0, -1), Vector3::new(-1.0, 0.0, 0.0), (x0, y0), (x0, y1)),
                ((0, 1), Vector3::new(1.0, 0.0, 0.0), (x1, y0), (x1, y1)),
                ((-1, 0), Vector3::new(0.0, 1.0, 0.0), (x0, y1), (x1, y1)),
                ((1, 0), Vector3::new(0.0, -1.0, 0.0), (x0, y0), (x1, y0)),
            ];

            for ((down, across), axis, p, q) in walls {
                let (row, column) = (row as i64, column as i64);
                let neighbour = height(row + down, column + across);
                if top <= neighbour {
                    continue;
                }
                // A wall faces the notch it drops into rather than the sky its
                // own cell sees, so it takes what the two of them share.
                let sky =
                    (sky + sky_at(&openness, rows, columns, row + down, column + across)) / 2.0;
                for (low, high, side) in [(neighbour, top, near), (-top, -neighbour, far)] {
                    faces.push(Face {
                        a: Vector3::new(p.0, p.1, low),
                        b: Vector3::new(q.0, q.1, low),
                        c: Vector3::new(q.0, q.1, high),
                        d: Vector3::new(p.0, p.1, high),
                        normal: bevelled(axis, side),
                        openness: sky,
                    });
                }
            }
        }
    }

    faces
}

/// How far in front of the middle the cell's surface stands, and zero off the
/// edge of the drawing.
///
/// The drawing is struck *through* the slab rather than raised off a backing
/// plate: a column of ink height `h` runs from `-h/2` to `+h/2`, so the solid
/// straddles the origin by construction and both of its sides carry the relief.
///
/// The plate is what this had, and a plate is one flat quad with one normal
/// spanning the whole drawing. Head-on it hides behind the ink and costs
/// nothing. Turned past a quarter it *is* the picture — so half of every spin
/// arrived as a featureless slab, one character repeated across the frame,
/// however carefully the front had been lit. There is no angle a struck relief
/// has nothing to show at.
fn surface(heights: &[f64], rows: usize, columns: usize, row: i64, column: i64) -> f64 {
    if row < 0 || row >= rows as i64 || column < 0 || column >= columns as i64 {
        return 0.0;
    }
    heights[row as usize * columns + column as usize] / 2.0
}

/// How open a cell is, and a clear sky off the edge of the drawing — there is
/// nothing out there to stand in the light's way.
fn sky_at(openness: &[f64], rows: usize, columns: usize, row: i64, column: i64) -> f64 {
    if row < 0 || row >= rows as i64 || column < 0 || column >= columns as i64 {
        return 1.0;
    }
    openness[row as usize * columns + column as usize]
}

/// How much of the sky each cell of the drawing can still see, in reading order.
///
/// The light so far has only asked which way a surface faces. That is the whole
/// of direct lighting and it is blind to the one thing relief is made of: a cell
/// down a pit and a cell out on a plateau can face the same way, take the same
/// shade, and come out the same character. So the dish in the middle of a
/// drawing renders no darker than its rim, terraces show only where their slope
/// happens to change, and the picture flattens into a map of its own gradients.
///
/// A heightfield answers this directly, with no rays and no second pass over the
/// geometry. A neighbour standing `rise` above a cell `run` away walls off
/// everything below `rise / run`, so the steepest ratio along a direction is how
/// far the sky is closed off that way. Averaged over the directions and taken
/// off one, that is how open the cell is.
///
/// It belongs to the drawing rather than to the angle it is seen from, which is
/// the point: it is still there at the yaws where the direct light has nothing
/// left to separate.
fn openness(heights: &[f64], rows: usize, columns: usize) -> Vec<f64> {
    /// Compass directions, and how far along each one a blocker is still worth
    /// looking for. Past a few cells a rise has to be enormous to shade anything.
    const STEPS: [(i64, i64); 8] =
        [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (-1, 1), (1, -1), (1, 1)];
    const HORIZON: i64 = 3;
    /// How dark a fully walled-in cell goes. All the way to nothing is a hole
    /// punched in the drawing rather than a shadow in it.
    const DEPTH: f64 = 0.55;

    (0..rows)
        .flat_map(|row| (0..columns).map(move |column| (row as i64, column as i64)))
        .map(|(row, column)| {
            let here = surface(heights, rows, columns, row, column);
            let closed = STEPS
                .iter()
                .map(|(down, across)| {
                    // Columns sit one apart and rows `CELL_ASPECT`, so a step
                    // up the picture is a longer walk than a step across it.
                    let run = ((*across as f64).powi(2)
                        + (*down as f64 * CELL_ASPECT).powi(2))
                    .sqrt();
                    (1..=HORIZON)
                        .map(|step| {
                            let there = surface(
                                heights,
                                rows,
                                columns,
                                row + down * step,
                                column + across * step,
                            );
                            ((there - here) / (run * step as f64)).clamp(0.0, 1.0)
                        })
                        .fold(0.0, f64::max)
                })
                .sum::<f64>()
                / STEPS.len() as f64;
            1.0 - DEPTH * closed
        })
        .collect()
}

/// A wall's normal, rolled off the face it descends from.
///
/// A rim on a cut relief is not a knife edge — the surface turns over into its
/// own side, and how it turns is decided by the slope it turns off. The axis
/// alone is the knife edge, and every wall in a heightfield that runs the same
/// way shares it exactly: seen from near edge-on, where walls are the whole
/// picture, that is one normal, one tone and one character across the frame, no
/// matter what the ink underneath is doing. Rolling the face in gives the rim
/// the drawing back, and it is the same argument the caps take their tilt from.
///
/// A third, so the wall still reads as a side rather than as more of the front.
fn bevelled(axis: Vector3, face: Vector3) -> Vector3 {
    const ROLL: f64 = 0.34;
    Vector3::new(
        axis.x + ROLL * face.x,
        axis.y + ROLL * face.y,
        axis.z + ROLL * face.z,
    )
    .normalized()
}

/// How far the eye stands from the middle of the solid, counted in whichever way
/// the solid reaches furthest.
///
/// A parallel camera — no distance at all — is what this had, and under one a
/// face keeps its size however far off it is. The two ends of a turning slab are
/// then drawn identically, so nothing in the picture says which end is nearer:
/// a spin reads as a shape shearing about on the page rather than as a body
/// turning in space, and every bit of the depth has to be inferred from the
/// shading alone. Convergence is the one cue that does not have to be inferred.
///
/// Far enough that the drawing is not bent into a fisheye, near enough that the
/// far end visibly gives way.
const EYE_REACH: f64 = 3.6;

/// One directional light, resolved so the per-face work is two dot products.
#[derive(Clone, Copy)]
struct Light {
    direction: Vector3,
    /// Halfway between the light and the eye, with the eye taken as looking
    /// straight down the z axis from infinitely far. The camera converges, so
    /// that is an approximation — but over a lens this long it is worth a degree
    /// or two of highlight placement, and it is what lets the Blinn term be
    /// resolved once here instead of per sample.
    halfway: Vector3,
    strength: f64,
}

impl Light {
    fn new(direction: Vector3, strength: f64) -> Self {
        let direction = direction.normalized();
        let eye = Vector3::new(0.0, 0.0, 1.0);
        Self {
            direction,
            halfway: Vector3::new(direction.x, direction.y, direction.z + eye.z).normalized(),
            strength,
        }
    }
}

/// A key light up and to the left, a fill to the right, and a low back light.
///
/// One light was what this had, and a solid built out of boxes has very few
/// distinct normals: with a single source, every wall facing the same way took
/// exactly one value and whole flanks of the model went flat. Three separates
/// them. Following `ascii3d`, which lights its scene with three directionals
/// for the same reason.
const KEY: Vector3 = Vector3::new(-0.45, 0.65, 1.0);
const FILL: Vector3 = Vector3::new(0.85, 0.15, 0.45);
const BACK: Vector3 = Vector3::new(0.15, -0.70, 0.25);

/// Directions the rig's range is measured over: this many rings from the equator
/// up to the eye, each of this many steps around. How brightly a face can be lit
/// depends on where it points and nothing else, so the range belongs to the rig
/// rather than to any drawing and is worth finding once, properly.
const PROBE_RINGS: usize = 32;
const PROBE_SPOKES: usize = 96;

/// How much light a face pointing along `normal` collects from the rig.
fn diffuse(lights: &[Light], normal: Vector3) -> f64 {
    lights
        .iter()
        .map(|light| light.strength * normal.dot(light.direction).max(0.0))
        .sum()
}

/// Projects a solid onto a character grid with a depth buffer.
pub struct Renderer {
    pub solid: Solid,
    pub yaw: f64,
    pub pitch: f64,
    pub zoom: f64,
    /// Whether the yaw is going to move. Only the fit cares — see
    /// [`Self::yaws`] — and only a tool that holds still should clear it.
    pub spins: bool,
    lights: [Light; 3],
    /// The brightest any surface in this rig can be lit, so the diffuse sum can
    /// be read as a fraction of what is on offer rather than of what three
    /// lights would add up to if they all struck the same face square on — which
    /// they cannot, since they point in different directions. Without it the
    /// brightest part of a render never asks for the heaviest character and the
    /// top of the alphabet goes unused.
    peak: f64,
    /// And the dimmest, over the same normals [`Self::tone`] is ever handed.
    ///
    /// Only the peak was measured once, and the bottom of the range was left
    /// wherever the rig happened to drop it. That turned out to be a quarter of
    /// the way up: the darkest face in a frame arrived already lit, every shade
    /// in a render fell between 0.39 and 0.90, and the five lightest characters
    /// were never once asked for. A form graded through the top half of an
    /// alphabet reads as a bright mass with some modulation over it — the
    /// shading is doing its work and the ramp is throwing most of it away.
    floor: f64,
    /// How dark the darkest part of the solid is allowed to get.
    ///
    /// This used to be a floor holding the whole model up in the heavy end of
    /// the alphabet, because a face turned away from every light came out as a
    /// space and tore a hole through the silhouette. Coverage decides what is
    /// background now (see [`super::super::glyphs::Alphabet`]), so a dark face
    /// is just dark and this can be what it says it is: a little light bouncing
    /// around the scene. The shading gets the whole ramp to move through, which
    /// is the difference between a solid you can read the shape of and a slab.
    pub ambient: f64,
    /// How hard the specular highlight is, and how tight.
    ///
    /// `ascii3d` uses an exponent of 1000, which on a pixel raster puts a pin
    /// of light on a curved surface. Here a highlight has to survive being
    /// averaged over a whole cell before it can pick a heavier character, so it
    /// is spread far wider — at 1000 it would land between samples and never
    /// appear at all.
    pub specular: f64,
    pub shininess: f64,
    /// How much of the shade comes from distance rather than from the light.
    ///
    /// This carried the relief on its own once, because every cap took the same
    /// flat normal and lighting could not tell one from another; without it the
    /// model collapsed into a slab. A cap is tilted by the ink around it now (see
    /// [`build_faces`]), so the light does that work and this is left to do only
    /// what it is actually good for: staying faithful to the source. A taller
    /// column is a nearer one, so head-on the render still reproduces the
    /// drawing's own ink rather than only the shape the lights find in it.
    pub depth_cueing: f64,
}

impl Renderer {
    pub fn new(solid: Solid) -> Self {
        let lights = [Light::new(KEY, 1.0), Light::new(FILL, 0.38), Light::new(BACK, 0.22)];

        // A sum of cosines is largest at the direction the sources add up to, so
        // the best-lit normal this rig can produce is the sum of its directions
        // and the peak is what that normal collects.
        let brightest = lights
            .iter()
            .fold(Vector3::new(0.0, 0.0, 0.0), |total, light| {
                Vector3::new(
                    total.x + light.direction.x * light.strength,
                    total.y + light.direction.y * light.strength,
                    total.z + light.direction.z * light.strength,
                )
            })
            .normalized();
        let peak = diffuse(&lights, brightest).max(1e-6);

        // The darkest normal is not the mirror of the brightest. Every face is
        // turned to look at the eye before it is lit, so the normals this rig is
        // ever asked about are the hemisphere in front of the picture, and three
        // lights all aimed into that hemisphere leave even its worst-placed
        // member some light. Sweeping it is the only honest way to find the
        // bottom, and it costs one loop at load.
        let floor = (0..PROBE_RINGS)
            .flat_map(|ring| (0..PROBE_SPOKES).map(move |spoke| (ring, spoke)))
            .map(|(ring, spoke)| {
                let height = ring as f64 / PROBE_RINGS as f64;
                let radius = (1.0 - height * height).sqrt();
                let angle = TAU * spoke as f64 / PROBE_SPOKES as f64;
                let normal =
                    Vector3::new(radius * angle.cos(), radius * angle.sin(), height);
                diffuse(&lights, normal)
            })
            .fold(peak, f64::min);

        Self {
            solid,
            yaw: 0.0,
            pitch: 0.0,
            zoom: 1.0,
            spins: true,
            peak,
            floor: floor.min(peak - 1e-6),
            lights,
            ambient: 0.08,
            specular: 0.30,
            shininess: 24.0,
            depth_cueing: 0.32,
        }
    }

    /// Columns per row for a grid the model fills, which is what the window
    /// shapes the frame from.
    ///
    /// Measured at no pitch rather than at the current one: pitching the model
    /// changes how tall it stands, and a frame that followed would resize under
    /// a drag. The horizontal extent already covers everything the yaw will do,
    /// so this holds steady through a spin — which is the part that has to.
    pub fn frame_aspect(&self) -> f64 {
        let (_, horizontal, vertical) = self.camera(0.0, self.yaw);
        // A cell is `CELL_ASPECT` times taller than it is wide, so a world box
        // that square needs that many fewer rows than columns to hold it.
        horizontal * CELL_ASPECT / vertical
    }

    /// The yaws the frame has to be big enough for.
    ///
    /// A whole turn for a solid that turns: fitting each frame to itself would
    /// have the model swell and shrink as it went round, which reads as the
    /// camera lurching rather than as the solid turning. For one that does not
    /// turn there is nothing to hold still, and sweeping anyway costs the frame
    /// everything the worst yaw would have needed — on a disc, half of it, to a
    /// side-on view that is never drawn.
    fn yaws(&self, yaw: f64) -> Vec<f64> {
        if !self.spins {
            return vec![yaw];
        }
        (0..TURN_STEPS)
            .map(|step| TAU * step as f64 / TURN_STEPS as f64)
            .collect()
    }

    /// The camera the frame is fitted to at this pitch: how far off the eye
    /// stands, and half the width and height the solid covers from there.
    ///
    /// The standoff is counted off the solid's own reach, so a big drawing and
    /// a small one are seen through the same lens rather than the small one
    /// being held up against the glass. Reaching along the view axis counts too,
    /// and has to: a tall drawing turned on its side puts its length in front of
    /// the eye, and a standoff blind to that would set the eye down inside it.
    fn camera(&self, pitch: f64, yaw: f64) -> (f64, f64, f64) {
        let yaws = self.yaws(yaw);
        let bound = &self.solid.bound;
        let (horizontal, vertical) = bound.extents(pitch, &yaws);
        let eye = EYE_REACH * horizontal.max(vertical).max(bound.depth_reach(pitch, &yaws));
        let (horizontal, vertical) = bound.screen_extents(pitch, &yaws, eye);
        (eye, horizontal, vertical)
    }

    /// `yaw` is passed rather than read from the field so one prepared renderer
    /// can serve every frame of a spin without being rebuilt.
    pub fn canvas_at(&self, columns: usize, rows: usize, yaw: f64) -> AsciiCanvas {
        let mut canvas = AsciiCanvas::new(columns, rows, false);
        if columns == 0 || rows == 0 || self.solid.faces.is_empty() {
            return canvas;
        }

        // The solid is rasterised finer than the grid it lands on and only then
        // read back as characters, so a cell is decided by CELL_PIXELS samples
        // instead of by one. See `art::glyphs`.
        let mut surface = Surface::new(columns, rows);

        let rotation = Rotation::new(yaw, self.pitch);
        let (eye, horizontal, vertical) = self.camera(self.pitch, yaw);

        // A cell is CELL_PIXELS_TALL / CELL_PIXELS_WIDE = CELL_ASPECT times
        // taller than it is wide, which is exactly how much taller than wide the
        // world models it. So the sample grid is square in world terms and one
        // scale serves both axes — the correction the cell grid needed is gone.
        let scale = (surface.width as f64 / (2.0 * horizontal))
            .min(surface.height as f64 / (2.0 * vertical))
            * self.zoom;
        let center_x = (surface.width as f64 - 1.0) / 2.0;
        let center_y = (surface.height as f64 - 1.0) / 2.0;

        let project = |point: Vector3| -> Vector3 {
            let spun = rotation.apply(point);
            // What is near is drawn large. `eye` is far enough past the solid's
            // own reach that the divisor cannot approach zero.
            let converge = scale * eye / (eye - spun.z);
            Vector3::new(center_x + spun.x * converge, center_y - spun.y * converge, spun.z)
        };

        let depth_extent = self.solid.bound.depth_extent(yaw, self.pitch);
        for face in &self.solid.faces {
            let normal = rotation.apply(face.normal);
            let tone = self.tone(normal, face.openness, depth_extent);

            let a = project(face.a);
            let b = project(face.b);
            let c = project(face.c);
            let d = project(face.d);

            surface.fill(a, b, c, tone);
            surface.fill(a, c, d, tone);

            // A face smaller than one sample can fall between sample centres and
            // vanish. Its own centre always lands inside it, so plotting that
            // too guarantees every face contributes something.
            surface.plot(
                Vector3::new(
                    (a.x + b.x + c.x + d.x) / 4.0,
                    (a.y + b.y + c.y + d.y) / 4.0,
                    (a.z + b.z + c.z + d.z) / 4.0,
                ),
                tone,
            );
        }

        surface.read_into(&mut canvas);
        canvas
    }

    /// How bright a face pointing along `normal` is, as the two coefficients of
    /// `shade = base + gain * z` — depth cueing is the only part that varies
    /// across a flat face, and it varies linearly, so the per-sample work in the
    /// rasteriser stays one multiply and one add.
    fn tone(&self, normal: Vector3, openness: f64, depth_extent: f64) -> Tone {
        // Back faces are deliberately not culled. A struck relief is open work:
        // every space in the drawing is a hole clean through it, so the far side
        // of the solid is visible through the near one and culling would tear
        // gaps in the silhouette. Turning the normal to face the eye is what
        // makes keeping them safe — a surface is lit as the side of it we can
        // see. Taking the absolute value of the dot products instead — which is
        // what this did — lights a wall pointing directly away from the key as
        // brightly as one facing it, and that is most of why a lit solid used to
        // read as one flat mass.
        let normal = if normal.z < 0.0 { normal.negated() } else { normal };

        let mut highlight = 0.0;
        for light in &self.lights {
            highlight +=
                light.strength * normal.dot(light.halfway).max(0.0).powf(self.shininess);
        }
        // Stretched over the range the rig can actually reach a visible face
        // through, so the darkest one in a frame lands at the bottom of the
        // alphabet rather than a quarter of the way up it.
        let diffuse = ((diffuse(&self.lights, normal) - self.floor)
            / (self.peak - self.floor))
            .clamp(0.0, 1.0);
        let highlight = (highlight / self.peak).min(1.0);

        // Light and nearness together, both running 0 to 1. `nearness` is
        // `(z / depth_extent + 1) / 2`, which is affine in z — so the whole
        // shade stays affine in z and the rasteriser's per-sample work is one
        // multiply and one add.
        //
        // Only the light is shut out by what stands around the face. Nearness is
        // the camera's business and a face in a pit is no further away for being
        // in one.
        let cue = self.depth_cueing;
        let lit =
            openness * ((1.0 - cue) * diffuse + self.specular * highlight) + cue * 0.5;

        // Everything the solid covers starts at `ambient` and climbs from there,
        // over very nearly the whole range the alphabet can draw.
        Tone {
            base: (openness * self.ambient + (1.0 - self.ambient) * lit) as f32,
            gain: ((1.0 - self.ambient) * cue / (2.0 * depth_extent)) as f32,
        }
    }
}

/// A flat face's brightness, as a function of how near to the eye a sample on
/// it is.
#[derive(Clone, Copy)]
struct Tone {
    base: f32,
    gain: f32,
}

impl Tone {
    fn at(self, z: f32) -> f32 {
        (self.base + self.gain * z).clamp(0.0, 1.0)
    }
}

/// The sub-cell raster the solid is drawn into before it is read back as
/// characters.
struct Surface {
    columns: usize,
    width: usize,
    height: usize,
    shade: Vec<f32>,
    depths: Vec<f32>,
}

impl Surface {
    fn new(columns: usize, rows: usize) -> Self {
        let width = columns * CELL_PIXELS_WIDE;
        let height = rows * CELL_PIXELS_TALL;
        Self {
            columns,
            width,
            height,
            shade: vec![0.0; width * height],
            depths: vec![f32::NEG_INFINITY; width * height],
        }
    }

    fn plot(&mut self, point: Vector3, tone: Tone) {
        let x = point.x.round();
        let y = point.y.round();
        if x < 0.0 || x >= self.width as f64 || y < 0.0 || y >= self.height as f64 {
            return;
        }
        self.sample(y as usize * self.width + x as usize, point.z as f32, tone);
    }

    fn sample(&mut self, index: usize, z: f32, tone: Tone) {
        if z <= self.depths[index] {
            return;
        }
        self.depths[index] = z;
        self.shade[index] = tone.at(z);
    }

    fn fill(&mut self, a: Vector3, b: Vector3, c: Vector3, tone: Tone) {
        fn edge(p: Vector3, q: Vector3, x: f64, y: f64) -> f64 {
            (q.x - p.x) * (y - p.y) - (q.y - p.y) * (x - p.x)
        }

        let area = edge(a, b, c.x, c.y);
        if area.abs() <= 1e-9 {
            return;
        }

        let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
        let max_x = (a.x.max(b.x).max(c.x).ceil()).min(self.width as f64 - 1.0);
        let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
        let max_y = (a.y.max(b.y).max(c.y).ceil()).min(self.height as f64 - 1.0);
        if max_x < 0.0 || max_y < 0.0 {
            return;
        }
        let max_x = max_x as usize;
        let max_y = max_y as usize;
        if min_x > max_x || min_y > max_y {
            return;
        }

        // Each weight is an edge function divided by the signed area, which
        // normalises the winding: a point is inside whenever all three are
        // non-negative. Every one of them is affine in x and y, so its value at
        // the next sample is its value at this one plus a constant — and the
        // inner loop becomes three adds where it was three edge functions and
        // three divisions. On a grid this fine that is the difference between
        // most of a frame and a small part of one.
        let scale = 1.0 / area;
        let (step_x, step_y) = (
            [
                -(c.y - b.y) * scale,
                -(a.y - c.y) * scale,
                -(b.y - a.y) * scale,
            ],
            [
                (c.x - b.x) * scale,
                (a.x - c.x) * scale,
                (b.x - a.x) * scale,
            ],
        );

        let (x, y) = (min_x as f64, min_y as f64);
        let mut down = [
            edge(b, c, x, y) * scale,
            edge(c, a, x, y) * scale,
            edge(a, b, x, y) * scale,
        ];

        for row in min_y..=max_y {
            let mut weight = down;
            for column in min_x..=max_x {
                if weight[0] >= -1e-9 && weight[1] >= -1e-9 && weight[2] >= -1e-9 {
                    let z = weight[0] * a.z + weight[1] * b.z + weight[2] * c.z;
                    self.sample(row * self.width + column, z as f32, tone);
                }
                for (value, step) in weight.iter_mut().zip(&step_x) {
                    *value += step;
                }
            }
            for (value, step) in down.iter_mut().zip(&step_y) {
                *value += step;
            }
        }
    }

    /// Gathers each cell's samples and takes the character nearest them.
    ///
    /// Rows are independent — they read one shared buffer and write disjoint
    /// runs of another — and choosing characters is the largest single cost in a
    /// frame, so they are shared out across cores. That is most of what makes a
    /// preview keep up with a spin.
    fn read_into(&self, canvas: &mut AsciiCanvas) {
        let columns = self.columns;
        canvas
            .glyphs
            .par_chunks_mut(columns.max(1))
            .enumerate()
            .for_each(|(row, line)| {
                let mut cell = [0f32; CELL_PIXELS];
                for (column, glyph) in line.iter_mut().enumerate() {
                    // How much of this cell the solid reaches, which the depth
                    // buffer knows exactly. Asking the shading instead — "did
                    // any light land here?" — cannot tell a hole from a face
                    // pointing away from every lamp, and that conflation is what
                    // the ambient floor used to exist to paper over.
                    //
                    // None of it is background. All of it is interior and gets
                    // graded. Anything between is the silhouette and gets its
                    // edge traced — see [`Alphabet::nearest`].
                    let mut covered = 0;
                    for y in 0..CELL_PIXELS_TALL {
                        let from = (row * CELL_PIXELS_TALL + y) * self.width
                            + column * CELL_PIXELS_WIDE;
                        let into = y * CELL_PIXELS_WIDE;
                        covered += self.depths[from..from + CELL_PIXELS_WIDE]
                            .iter()
                            .filter(|depth| depth.is_finite())
                            .count();
                        cell[into..into + CELL_PIXELS_WIDE]
                            .copy_from_slice(&self.shade[from..from + CELL_PIXELS_WIDE]);
                    }
                    // Most of a frame is empty background, and matching it
                    // against the whole alphabet to be told it is a space is the
                    // single biggest cost in here.
                    if covered > 0 {
                        *glyph = ALPHABET.nearest(&cell, covered == CELL_PIXELS);
                    }
                }
            });
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

    fn frame_aspect(&self) -> Option<f64> {
        Some(self.renderer.frame_aspect())
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

    let spin_rate = params.f64("spin", 1.2)?;
    let still = params.is_set("still");
    renderer.spins = !still && spin_rate != 0.0;

    Ok(Generator::Glyph(Box::new(SpinningAscii { renderer, spin_rate, still })))
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

    /// Full ink everywhere, so the solid it lifts to is a plain box and its
    /// silhouette is the camera's doing rather than the drawing's.
    const BLOCK: &str = "\
@@@@@@@@@
@@@@@@@@@
@@@@@@@@@
@@@@@@@@@
@@@@@@@@@";

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

    /// A parallel camera draws the far end of a turning solid exactly as large
    /// as the near end, so nothing in the picture says which end is which and a
    /// spin reads as a shape shearing about on the page. Turn a solid block and
    /// the vertical edge nearest the eye should stand taller than the two that
    /// bound the silhouette — a box drawn as a trapezoid rather than a rectangle.
    #[test]
    fn a_turned_block_is_drawn_as_a_trapezoid() {
        let mut renderer = Renderer::new(Solid::from_text(BLOCK, 8.0));
        renderer.zoom = 0.92;
        let canvas = renderer.canvas_at(80, 24, 40_f64.to_radians());

        let columns: Vec<usize> = (0..canvas.columns)
            .map(|column| {
                (0..canvas.rows).filter(|&row| canvas.get(column, row) != SPACE).count()
            })
            .filter(|inked| *inked > 0)
            .collect();
        let (near, _) = columns
            .iter()
            .enumerate()
            .max_by_key(|(_, inked)| **inked)
            .expect("the block draws something");

        assert!(
            columns[near] > columns[0] && columns[near] > columns[columns.len() - 1],
            "the near edge should overtop both far ones: {columns:?}"
        );
        assert!(
            near > 0 && near < columns.len() - 1,
            "and it stands inside the silhouette, not on it: column {near} of {}",
            columns.len()
        );
    }

    /// Three lights all aimed into the hemisphere the eye can see lift even
    /// their worst-placed face clear of dark, so a shade read against the peak
    /// alone starts a quarter of the way up: the whole solid then grades through
    /// the heavy end of the alphabet and the faint end goes unused. That reads
    /// as a bright mass with some modulation over it rather than as a form,
    /// however carefully the geometry underneath it is built.
    #[test]
    fn a_frame_grades_through_the_whole_alphabet() {
        let mut renderer = Renderer::new(Solid::from_text(DIAMOND, 8.0));
        renderer.pitch = 0.5;
        let yaw = 0.6;
        let rotation = Rotation::new(yaw, renderer.pitch);
        let depth_extent = renderer.solid.bound.depth_extent(yaw, renderer.pitch);

        let shades: Vec<f32> = renderer
            .solid
            .faces
            .iter()
            .map(|face| {
                renderer.tone(rotation.apply(face.normal), face.openness, depth_extent).base
            })
            .collect();
        let faintest = shades.iter().copied().fold(f32::INFINITY, f32::min);
        let heaviest = shades.iter().copied().fold(0.0_f32, f32::max);

        assert!(faintest < 0.35, "the turned-away side stays at {faintest}, nowhere near dark");
        assert!(heaviest > 0.8, "and the lit side only reaches {heaviest}");
    }

    /// Blank lines at the end of a file are how it was saved, not part of the
    /// drawing, and counting them would push the drawing up out of centre.
    #[test]
    fn trailing_blank_lines_are_not_part_of_the_drawing() {
        let plain = Solid::from_text(DIAMOND, 8.0);
        let padded = Solid::from_text(&format!("{DIAMOND}\n\n   \n\n"), 8.0);
        assert_eq!(plain.faces.len(), padded.faces.len());
        assert_eq!(plain.bound.extents(0.5, &[0.6]), padded.bound.extents(0.5, &[0.6]));
        assert_eq!(
            plain.bound.depth_extent(0.6, 0.5),
            padded.bound.depth_extent(0.6, 0.5)
        );
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

    /// Caps carry the light now, so which way they point is worth pinning down.
    /// Ink that does not change has nothing to tilt on.
    #[test]
    fn level_ink_leaves_its_caps_facing_the_viewer() {
        let caps = caps(&[4.0; 25], 5, 5);
        assert!(inner(&caps, 5, 5).iter().all(is_level), "an even block is one flat surface");
        assert_eq!(
            caps.iter().filter(|normal| is_level(normal)).count(),
            9,
            "and the cells around it lean out over the drop to the paper"
        );
    }

    /// Dithering fakes a tone by alternating two characters, and ASCII art is
    /// full of it. Asked about one neighbour at a time this is a cliff at every
    /// cell and the surface shatters into noise; asked over a three by three
    /// patch the alternation cancels, which is what the eye does with it too.
    #[test]
    fn dithered_ink_reads_as_a_tone_rather_than_as_cliffs() {
        let heights: Vec<f64> = (0..25)
            .map(|index| if (index / 5 + index % 5) % 2 == 0 { 8.0 } else { 2.0 })
            .collect();
        let caps = caps(&heights, 5, 5);
        assert!(
            inner(&caps, 5, 5).iter().all(is_level),
            "a checkerboard is a flat tone, not a field of steps"
        );
    }

    /// The other half of that claim: ink that genuinely does run one way has to
    /// tilt, and tilt back against the rise, which is what catches a light hung
    /// over the slope.
    #[test]
    fn a_rise_tilts_its_caps_back_against_it() {
        let heights: Vec<f64> = (0..25).map(|index| (index % 5) as f64 + 1.0).collect();
        let caps = caps(&heights, 5, 5);
        let inner = inner(&caps, 5, 5);

        assert!(
            inner.iter().all(|normal| normal.x < -0.4),
            "ink rising to the right should lean every cap well to the left"
        );
        assert!(
            inner.iter().all(|normal| normal.y.abs() < 1e-9),
            "the ramp does not run up the picture, so nothing should lean that way"
        );
    }

    /// Which way a face points is the whole of direct light, and it cannot tell
    /// the floor of a pit from the same floor out in the open: both look at the
    /// sky, both take the same shade, both come out the same character. What
    /// stands around a cell is the other half of the answer, and it is the half
    /// that keeps working at the angles where the light has nothing left to
    /// separate.
    #[test]
    fn a_cell_walled_in_by_its_neighbours_sees_less_sky() {
        let mut heights = vec![9.0; 25];
        heights[12] = 0.5;
        let sky = openness(&heights, 5, 5);

        assert!(sky[12] < 0.6, "a pit should be shut in, not merely dimmed: {}", sky[12]);
        assert!(
            sky.iter().enumerate().all(|(cell, open)| cell == 12 || *open > 0.99),
            "and nothing on a flat plateau should be shaded at all: {sky:?}"
        );
    }

    /// Turned towards edge-on, walls are most of what there is to see, and a
    /// bare axis makes every wall running the same way one normal — one tone
    /// across the frame, whatever the ink under it is doing. Rolling each one
    /// off the face it drops from is what keeps the drawing in the model's side.
    #[test]
    fn a_wall_rolls_off_the_face_it_drops_from() {
        let heights: Vec<f64> = (0..25).map(|index| (index % 5) as f64 + 1.0).collect();
        let walls = walls(&heights, 5, 5);

        assert!(!walls.is_empty(), "a rise steps down to the paper on every side");
        assert!(
            walls.iter().all(|normal| normal.z > 0.1),
            "a rim turns over towards its own face rather than straight out to the side"
        );
        assert!(
            walls.iter().all(|normal| normal.x.hypot(normal.y) > 0.9),
            "but only turns over: a wall that stops reading as a side has taken too much"
        );
    }

    /// The near face of every cell, in reading order. Every grid tested here
    /// stands clear of zero, so no cell is skipped and an index is a cell.
    ///
    /// A face lies flat at one depth; a wall climbs from one to another. That
    /// tells the two apart whatever their normals are doing, and the sign of the
    /// normal then picks the near one of the pair.
    fn caps(heights: &[f64], rows: usize, columns: usize) -> Vec<Vector3> {
        build_faces(heights, rows, columns)
            .into_iter()
            .filter(|face| face.a.z == face.c.z && face.normal.z > 0.0)
            .map(|face| face.normal)
            .collect()
    }

    /// The near side's walls. A wall climbs through depth where a face lies
    /// flat at one, and the near half of the pair is the half in front of the
    /// origin the drawing is struck through.
    fn walls(heights: &[f64], rows: usize, columns: usize) -> Vec<Vector3> {
        build_faces(heights, rows, columns)
            .into_iter()
            .filter(|face| face.a.z != face.c.z && face.a.z >= 0.0)
            .map(|face| face.normal)
            .collect()
    }

    /// The caps with a neighbour on all sides. A grid ends in a cliff down to
    /// the paper, and that cliff is a real slope — leaning out over it is the
    /// edge cells' job, not evidence about the ink inside.
    fn inner(caps: &[Vector3], rows: usize, columns: usize) -> Vec<Vector3> {
        (1..rows - 1)
            .flat_map(|row| (1..columns - 1).map(move |column| row * columns + column))
            .map(|index| caps[index])
            .collect()
    }

    fn is_level(normal: &Vector3) -> bool {
        normal.x.abs() < 1e-9 && normal.y.abs() < 1e-9
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
