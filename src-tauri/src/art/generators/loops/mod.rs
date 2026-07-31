//! Pieces, rather than a machine with settings.
//!
//! Every other tool here is a way of looking at something the user brings — a
//! drawing, a photograph, a formula, a field of angles. This one brings the
//! subject too. A piece is a finished animation with a handful of dials on it,
//! the way [Bleuje's Processing animations][ref] are, and what it is for is
//! being exported: a loop that meets itself, at a size that can be posted.
//!
//! The pieces are written here from the ideas in that collection rather than
//! from its source, which reserves its rights. What is borrowed is the craft in
//! [`crate::art::motion`] — phase instead of a clock, noise walked round a
//! circle, easing that arrives — and the taste for a figure that rearranges
//! itself and comes back.
//!
//! Two paths run out of here. A drawn piece paints on a [`Paper`] and is read
//! back as glyphs, the way `gen2d` is; a modelled one hands quads to the lit
//! renderer, the way `scene` does. Which one a piece takes is the piece's own
//! business and nothing outside this module needs to know.
//!
//! [ref]: https://github.com/Bleuje/processing-animations-code

mod hilbert;
mod sierpinski;
mod sinusoids;

use crate::art::canvas::{AsciiCanvas, CELL_ASPECT};
use crate::art::generator::{Generator, GlyphGenerator};
use crate::art::params::Params;
use crate::art::read::{fine_size, Reader};

use super::paper::Paper;

/// Every piece the tool draws, in the order they were written.
///
/// One list, read by the error message and walked by the tests, so a piece that
/// is not on it is neither offered to anyone who mistypes nor answerable for
/// its loop.
const PIECES: [&str; 3] = ["hilbert", "sinusoids", "sierpinski"];

/// What the tool can draw, and everything that piece settled before it drew a
/// frame.
///
/// A piece's own dials belong to the piece — a curve has a depth, a packing has
/// a count — so they are read here once, at the piece they were named for,
/// rather than becoming a row of settings that mean nothing to five of the six.
/// Anything a piece has to work out before it can draw at all lands here too: a
/// packing settled once and then held still is a composition, and one settled
/// again every frame is a flicker.
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    /// A Hilbert curve whose blocks pivot about their own middles.
    Hilbert { order: u32, seed: u64 },
    /// Circles packed into the frame, each with a wave running through it.
    Sinusoids { discs: Vec<sinusoids::Disc> },
    /// A gasket whose three copies slide round its corners.
    Sierpinski { depth: u32, seed: u64 },
}

impl Piece {
    fn named(name: &str, params: &Params, seed: u64) -> Result<Self, String> {
        match name {
            "hilbert" => Ok(Self::Hilbert {
                order: hilbert::order(params.usize("order", 4)?),
                seed,
            }),
            "sinusoids" => Ok(Self::Sinusoids {
                discs: sinusoids::pack(seed, params.usize("count", 28)?),
            }),
            "sierpinski" => Ok(Self::Sierpinski {
                depth: sierpinski::depth(params.usize("depth", 4)?),
                seed,
            }),
            other => Err(format!(
                "`{other}` is not a piece — try one of {}",
                PIECES.join(", ")
            )),
        }
    }

    fn draw(&self, paper: &mut Paper, phase: f64, colored: bool) {
        match self {
            Self::Hilbert { order, seed } => hilbert::draw(paper, *order, phase, *seed, colored),
            Self::Sinusoids { discs } => sinusoids::draw(paper, discs, phase, colored),
            Self::Sierpinski { depth, seed } => {
                sierpinski::draw(paper, *depth, phase, *seed, colored)
            }
        }
    }

    /// Columns per row for a grid this piece fills. A figure composed in a
    /// square wants a square frame; a wide grid would give it two empty
    /// margins and nothing else.
    fn frame_aspect(&self) -> Option<f64> {
        Some(CELL_ASPECT)
    }
}

/// A piece that paints and is read back.
struct Drawn {
    piece: Piece,
    period: f64,
    still: bool,
    reader: Reader,
}

impl GlyphGenerator for Drawn {
    fn canvas(&self, columns: usize, rows: usize, time: f64) -> AsciiCanvas {
        let blank = || AsciiCanvas::new(columns, rows, self.reader.colored);
        if columns == 0 || rows == 0 {
            return blank();
        }
        let phase = if self.still { 0.0 } else { (time / self.period).rem_euclid(1.0) };
        let (wide, tall) = fine_size(columns, rows);
        let Some(mut paper) = Paper::new(wide, tall) else {
            return blank();
        };
        self.piece.draw(&mut paper, phase, self.reader.colored);
        let Some(picture) = paper.picture() else {
            return blank();
        };
        self.reader.canvas(&picture)
    }

    fn loop_duration(&self) -> Option<f64> {
        (!self.still).then_some(self.period)
    }

    fn frame_aspect(&self) -> Option<f64> {
        self.piece.frame_aspect()
    }
}

pub fn build(params: &Params) -> Result<Generator, String> {
    let name = params.string("piece").unwrap_or("hilbert");
    let piece = Piece::named(name, params, params.seed(7)?)?;
    // A piece is one loop by default, so the clip is the piece rather than the
    // piece repeated a few times inside a clip. Asking for a period shorter
    // than the export is how it gets repeated on purpose.
    let period = params.f64("period", params.f64("duration", 4.0)?)?;
    if period <= 0.0 {
        return Err("--period is how many seconds a loop takes, so it has to be positive".into());
    }

    Ok(Generator::Glyph(Box::new(Drawn {
        piece,
        period,
        still: params.is_set("still"),
        reader: Reader::from_params(params)?,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::canvas::SPACE;

    fn piece(name: &str, seed: u64) -> Piece {
        Piece::named(name, &Params::default(), seed).expect("a piece by that name")
    }

    fn drawn(piece: Piece) -> Drawn {
        Drawn {
            piece,
            period: 8.0,
            still: false,
            reader: Reader::from_params(&Params::default()).expect("the defaults read"),
        }
    }

    fn inked(canvas: &AsciiCanvas) -> usize {
        canvas.glyphs.iter().filter(|&&glyph| glyph != SPACE).count()
    }

    /// A piece has to arrive as a picture: one that marks nothing and one that
    /// fills every cell are equally not one.
    #[test]
    fn every_piece_draws_a_part_of_the_frame() {
        for name in PIECES {
            let canvas = drawn(piece(name, 7)).canvas(64, 30, 2.0);
            let marks = inked(&canvas);
            assert!(marks > 150, "{name} drew only {marks} cells");
            assert!(marks < canvas.glyphs.len(), "{name} filled the whole grid");
        }
    }

    /// The point of the tool: a period later the frame is the frame it started
    /// as, so an export meets itself.
    #[test]
    fn a_period_brings_every_piece_back_to_itself() {
        for name in PIECES {
            let drawn = drawn(piece(name, 7));
            let start = drawn.canvas(64, 30, 0.0);
            let round = drawn.canvas(64, 30, drawn.period);
            assert_eq!(start.glyphs, round.glyphs, "{name} does not close its loop");
        }
    }

    /// And the middle of the loop is not the start of it.
    #[test]
    fn every_piece_moves_inside_its_period() {
        for name in PIECES {
            let drawn = drawn(piece(name, 7));
            let start = drawn.canvas(64, 30, 0.0);
            let middle = drawn.canvas(64, 30, drawn.period * 0.37);
            let moved = start
                .glyphs
                .iter()
                .zip(&middle.glyphs)
                .filter(|(one, other)| one != other)
                .count();
            assert!(moved > 40, "{name} moved only {moved} cells");
        }
    }

    /// A seed is a promise that the same line can be typed twice, and that
    /// another one is worth typing.
    #[test]
    fn the_same_seed_draws_the_same_frame() {
        for name in PIECES {
            let one = drawn(piece(name, 7)).canvas(64, 30, 1.5);
            let same = drawn(piece(name, 7)).canvas(64, 30, 1.5);
            assert_eq!(one.glyphs, same.glyphs, "{name} is not repeatable");

            let other = drawn(piece(name, 8)).canvas(64, 30, 1.5);
            assert_ne!(one.glyphs, other.glyphs, "{name} ignores its seed");
        }
    }

    /// The loop is the clip unless something says otherwise, so a piece is seen
    /// once through rather than hurried.
    #[test]
    fn a_piece_lasts_the_whole_clip_by_default() {
        let mut params = Params::default();
        params.flags.insert("duration".into(), Some("11".into()));
        let built = build(&params).expect("the defaults build");
        assert_eq!(built.loop_duration(), Some(11.0));
    }

    #[test]
    fn an_unknown_piece_says_what_there_is() {
        let message = Piece::named("mandelbrot", &Params::default(), 7).unwrap_err();
        assert!(message.contains("hilbert"), "{message}");
    }
}
