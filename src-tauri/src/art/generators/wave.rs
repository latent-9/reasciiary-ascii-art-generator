//! A relief nobody drew: a heightfield a formula fills, lifted and lit by the
//! same machinery a text file is.
//!
//! The lift only ever asked for heights — ink was how a *drawing* answered that
//! question, not the only way to answer it. A formula can answer it per frame,
//! and then the whole rig behind [`super::ascii`] applies to something that
//! moves: caps tilted by the slope under them, walls rolled off the face they
//! drop from, troughs darkened by the sky their own rims take away.
//!
//! What it draws is a travelling wave on a disc, in the shape those loops take
//! in [Bleuje's Processing sketches][ref]: one phase per cell, offset by where
//! the cell is, eased by a power so the crests arrive with an edge on them. The
//! offset is what makes it travel and the loop is exact — after a period every
//! crest stands where the one behind it did, so the animation is periodic
//! without ever appearing to reset.
//!
//! [ref]: https://github.com/Bleuje/processing-animations-code

use std::f64::consts::TAU;

use crate::art::canvas::{AsciiCanvas, CELL_ASPECT};
use crate::art::generator::{Generator, GlyphGenerator};
use crate::art::params::Params;

use super::ascii::{Renderer, Solid};

/// How much of the radius the sheet takes to taper away to nothing.
///
/// Squared off, the disc ends in a cliff as tall as the relief and the eye
/// reads the cliff before it reads the wave. Tapered, the rim is the one part
/// of the sheet that never moves, which is what gives the crests something to
/// travel against.
const RIM: f64 = 0.18;

/// How much of the depth a trough keeps.
///
/// At nothing the troughs are holes clean through and the disc comes apart into
/// free-standing ridges — accurate to the formula and unreadable as a surface.
const FLOOR: f64 = 0.24;

/// How sharply a crest turns over into the trough behind it.
const EDGE: f64 = 2.6;

/// The same curve either side of the middle, steepened by `hardness`.
///
/// Bleuje's easing, and it is doing something here that a cosine cannot. A
/// cosine spends most of a cycle on the way somewhere; this one spends it
/// arrived, and turns over quickly in between. On a heightfield that difference
/// is walls: broad flats separated by short steep sides, which is what the
/// shading has to work with. A wave made of cosines has no sides worth lighting
/// and renders as a wash.
fn ease(progress: f64, hardness: f64) -> f64 {
    if progress < 0.5 {
        0.5 * (2.0 * progress).powf(hardness)
    } else {
        1.0 - 0.5 * (2.0 * (1.0 - progress)).powf(hardness)
    }
}

/// The wave, as heights over a grid of cells.
struct Field {
    columns: usize,
    rows: usize,
    /// How far a crest stands above the plane the disc is cut from.
    depth: f64,
    /// Crests between the middle and the rim. Also the speed: over one period
    /// every crest travels out to where the next one stood, so the more of them
    /// there are the less far each has to go.
    rings: f64,
    /// How many times the crest line winds round on its way out. Zero draws
    /// rings; anything else draws a spiral with that many arms, and a whole
    /// number is what keeps the two sides of the seam at the same height.
    arms: f64,
}

impl Field {
    /// Cells across, from cells down: enough rows that the disc comes out round
    /// rather than as an ellipse, since a character cell is taller than it is
    /// wide.
    fn new(columns: usize, depth: f64, rings: f64, arms: f64) -> Self {
        let rows = ((columns as f64 / CELL_ASPECT).round() as usize).max(1);
        Self { columns, rows, depth, rings, arms }
    }

    /// The field at `phase`, which runs 0 to 1 over a period.
    fn heights(&self, phase: f64) -> Vec<f64> {
        // The middle of the grid, in the same units [`Solid::from_heights`]
        // lays the cells out in, so the disc is centred on the axis the frame
        // turns about rather than half a cell off it.
        let middle_column = (self.columns as f64 - 1.0) / 2.0;
        let middle_row = (self.rows as f64 - 1.0) / 2.0;
        let span = self.columns as f64 / 2.0;

        let mut heights = vec![0.0; self.rows * self.columns];
        for row in 0..self.rows {
            let y = (middle_row - row as f64) * CELL_ASPECT / span;
            for column in 0..self.columns {
                let x = (column as f64 - middle_column) / span;

                let radius = (x * x + y * y).sqrt();
                // Smoothly, so the rim reads as the sheet turning over and not
                // as one more crest.
                let inside = ((1.0 - radius) / RIM).clamp(0.0, 1.0);
                let taper = inside * inside * (3.0 - 2.0 * inside);
                if taper <= 0.0 {
                    continue;
                }

                // Where the cell sits in the wave: how far out it is, how far
                // round, and how far through the period. Taking the fraction
                // is what closes the loop — the same argument a period later
                // is the same argument.
                let winding = self.arms * y.atan2(x) / TAU;
                let travel = radius * self.rings - winding - phase;
                let swell = 0.5 - 0.5 * (TAU * travel).cos();

                let crest = FLOOR + (1.0 - FLOOR) * ease(swell, EDGE);
                heights[row * self.columns + column] = self.depth * taper * crest;
            }
        }
        heights
    }

    fn solid(&self, phase: f64) -> Solid {
        // The reach is what the formula can raise, not what it happens to have
        // raised in this frame. They agree wherever a whole crest fits inside
        // the disc, and where one does not the frame is a little loose — which
        // is the right way round, since the alternative is a model that grows
        // and shrinks as the wave runs under it.
        Solid::from_heights(&self.heights(phase), self.rows, self.columns, self.depth).rounded()
    }
}

/// The animated tool: the wave travels out and the camera goes round, both a
/// whole number of times over one period.
pub struct Wave {
    field: Field,
    yaw: f64,
    pitch: f64,
    zoom: f64,
    /// Seconds the whole thing takes to come back to where it started.
    period: f64,
    /// Turns the camera makes in that time. A whole number, or the camera
    /// arrives somewhere the wave does not.
    ///
    /// None by default. The drawing tool spins because a drawing seen from one
    /// angle is a picture of a drawing, and the turn is what says it is a
    /// solid. This one is already moving, and a disc is flat enough that a
    /// quarter turn puts it edge-on and takes the whole picture away with it.
    turns: f64,
    still: bool,
}

impl Wave {
    fn renderer(&self, phase: f64) -> Renderer {
        let mut renderer = Renderer::new(self.field.solid(phase));
        renderer.yaw = self.yaw;
        renderer.pitch = self.pitch;
        renderer.zoom = self.zoom;
        renderer.spins = self.turns != 0.0;
        renderer
    }
}

impl GlyphGenerator for Wave {
    fn canvas(&self, columns: usize, rows: usize, time: f64) -> AsciiCanvas {
        let phase = if self.still { 0.0 } else { (time / self.period).rem_euclid(1.0) };
        self.renderer(phase)
            .canvas_at(columns, rows, self.yaw + TAU * self.turns * phase)
    }

    fn loop_duration(&self) -> Option<f64> {
        if self.still {
            None
        } else {
            Some(self.period)
        }
    }

    fn frame_aspect(&self) -> Option<f64> {
        // The disc and its reach are the same in every frame, so any one of
        // them answers this and none of them disagrees.
        Some(self.renderer(0.0).frame_aspect())
    }
}

pub fn build(params: &Params) -> Result<Generator, String> {
    let period = params.f64("period", 6.0)?;
    if period <= 0.0 {
        return Err("--period is how many seconds a loop takes, so it has to be positive".into());
    }

    let field = Field::new(
        params.usize("cells", 96)?.clamp(8, 512),
        params.f64("depth", 9.0)?,
        params.f64("rings", 3.0)?,
        params.f64("arms", 2.0)?,
    );

    Ok(Generator::Glyph(Box::new(Wave {
        field,
        yaw: params.f64("yaw", 0.0)?.to_radians(),
        pitch: params.f64("pitch", 52.0)?.to_radians(),
        zoom: params.f64("zoom", 0.92)?,
        period,
        turns: params.f64("turns", 0.0)?,
        still: params.is_set("still"),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::canvas::SPACE;

    fn field() -> Field {
        Field::new(64, 9.0, 3.0, 2.0)
    }

    /// The whole point of the offset: a period later every cell is where it
    /// was, so an export can be cut anywhere and still meet itself.
    #[test]
    fn a_period_brings_the_field_back_to_itself() {
        let field = field();
        let start = field.heights(0.0);
        let round = field.heights(1.0);
        for (one, other) in start.iter().zip(&round) {
            assert!((one - other).abs() < 1e-9, "{one} != {other}");
        }
    }

    /// And the point of taking the fraction: nothing in the middle of the loop
    /// is where it started either, or the wave is standing still.
    #[test]
    fn the_wave_travels_between_one_period_and_the_next() {
        let field = field();
        let start = field.heights(0.0);
        let middle = field.heights(0.5);
        let moved = start
            .iter()
            .zip(&middle)
            .filter(|(one, other)| (*one - *other).abs() > 0.5)
            .count();
        assert!(moved > start.len() / 8, "only {moved} of {} cells moved", start.len());
    }

    /// The rim tapers to nothing, so the corners of the grid are outside the
    /// disc and the silhouette is round.
    #[test]
    fn the_disc_does_not_reach_the_corners() {
        let field = field();
        let heights = field.heights(0.3);
        let corners = [
            0,
            field.columns - 1,
            (field.rows - 1) * field.columns,
            field.rows * field.columns - 1,
        ];
        for corner in corners {
            assert_eq!(heights[corner], 0.0);
        }
        assert!(heights.iter().copied().fold(0.0, f64::max) > 0.0);
    }

    /// A crest reaches the depth it was promised whatever the phase, which is
    /// what lets the frame be fitted once to a field that moves.
    #[test]
    fn a_crest_stands_the_full_depth_at_every_phase() {
        let field = field();
        for step in 0..16 {
            let tallest = field
                .heights(step as f64 / 16.0)
                .iter()
                .copied()
                .fold(0.0, f64::max);
            assert!((tallest - field.depth).abs() < 0.05, "{tallest} at step {step}");
        }
    }

    #[test]
    fn a_frame_draws_something() {
        let generator = Wave {
            field: field(),
            yaw: 0.0,
            pitch: 0.9,
            zoom: 0.92,
            period: 6.0,
            turns: 0.0,
            still: false,
        };
        let canvas = generator.canvas(80, 24, 1.5);
        let inked = canvas.glyphs.iter().filter(|&&glyph| glyph != SPACE).count();
        assert!(inked > 200, "only {inked} cells drawn");
    }
}
