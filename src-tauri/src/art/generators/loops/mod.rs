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
mod sliding;
mod spherewave;
mod toruscurve;

use crate::art::canvas::{AsciiCanvas, AsciiRamp, CELL_ASPECT};
use crate::art::generator::{Generator, GlyphGenerator};
use crate::art::params::Params;
use crate::art::read::{fine_size, Reader};

use super::ascii::{Face, Renderer, Solid};
use super::paper::Paper;

/// Every piece the tool draws, in the order they were written.
///
/// One list, read by the error message and walked by the tests, so a piece that
/// is not on it is neither offered to anyone who mistypes nor answerable for
/// its loop.
const PIECES: [&str; 6] = [
    "hilbert",
    "sinusoids",
    "sierpinski",
    "sliding",
    "spherewave",
    "toruscurve",
];

/// What the tool can make, and which of the app's two ways of arriving at
/// glyphs that piece takes.
///
/// The split is the piece's own business and is settled here, once, so that
/// neither half has to carry an arm it can never reach: a painting is never
/// asked for quads and a model is never handed a sheet.
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    Drawn(Drawing),
    Modelled(Model),
}

/// A piece that paints on a sheet and is read back, the way `gen2d` is.
///
/// A piece's own dials belong to the piece — a curve has an order, a packing has
/// a count — so they are read here once, at the piece they were named for,
/// rather than becoming a row of settings that mean nothing to five of the six.
/// Anything a piece has to work out before it can draw at all lands here too: a
/// packing settled once and then held still is a composition, and one settled
/// again every frame is a flicker.
#[derive(Clone, PartialEq, Debug)]
enum Drawing {
    /// A Hilbert curve whose blocks pivot about their own middles.
    Hilbert { order: u32, seed: u64 },
    /// Circles packed into the frame, each with a wave running through it.
    Sinusoids { discs: Vec<sinusoids::Disc> },
    /// A gasket whose three copies slide round its corners.
    Sierpinski { depth: u32, seed: u64 },
    /// A quadtree whose quarters slide while the whole of it doubles.
    Sliding { depth: u32 },
}

/// A piece that hands quads to the lit renderer, the way `scene` does.
#[derive(Clone, PartialEq, Debug)]
enum Model {
    /// A sphere of loose elements with a front travelling over it.
    SphereWave { count: usize, seed: u64 },
    /// A rope knotted round a torus with swells running along it.
    TorusCurve { twists: usize },
}

impl Piece {
    fn named(name: &str, params: &Params, seed: u64) -> Result<Self, String> {
        match name {
            "hilbert" => Ok(Self::Drawn(Drawing::Hilbert {
                order: hilbert::order(params.usize("order", 4)?),
                seed,
            })),
            "sinusoids" => Ok(Self::Drawn(Drawing::Sinusoids {
                discs: sinusoids::pack(seed, params.usize("count", 28)?),
            })),
            "sierpinski" => Ok(Self::Drawn(Drawing::Sierpinski {
                depth: sierpinski::depth(params.usize("depth", 4)?),
                seed,
            })),
            "sliding" => Ok(Self::Drawn(Drawing::Sliding {
                depth: sliding::depth(params.usize("depth", 4)?),
            })),
            "spherewave" => Ok(Self::Modelled(Model::SphereWave {
                count: spherewave::count(params.usize("count", 700)?),
                seed,
            })),
            "toruscurve" => Ok(Self::Modelled(Model::TorusCurve {
                twists: toruscurve::twists(params.usize("twists", 3)?),
            })),
            other => Err(format!(
                "`{other}` is not a piece — try one of {}",
                PIECES.join(", ")
            )),
        }
    }
}

impl Drawing {
    fn draw(&self, paper: &mut Paper, phase: f64, colored: bool) {
        match self {
            Self::Hilbert { order, seed } => hilbert::draw(paper, *order, phase, *seed, colored),
            Self::Sinusoids { discs } => sinusoids::draw(paper, discs, phase, colored),
            Self::Sierpinski { depth, seed } => {
                sierpinski::draw(paper, *depth, phase, *seed, colored)
            }
            Self::Sliding { depth } => sliding::draw(paper, *depth, phase, colored),
        }
    }
}

impl Model {
    /// The surface at one moment. A new set of quads every frame, which is what
    /// makes these pieces different from `scene`: there the model is cut once
    /// and only turned.
    fn faces(&self, phase: f64) -> Vec<Face> {
        match self {
            Self::SphereWave { count, seed } => spherewave::model(*count, phase, *seed),
            Self::TorusCurve { twists } => toruscurve::model(*twists, phase),
        }
    }

    /// The furthest the surface ever gets from the middle, over the whole
    /// period — see [`Solid::from_quads_reaching`].
    fn reach(&self) -> f64 {
        match self {
            Self::SphereWave { count, .. } => spherewave::reach(*count),
            Self::TorusCurve { twists } => toruscurve::reach(*twists),
        }
    }

    /// Where the camera sits to look at this piece, in degrees above its
    /// middle. A scattering reads from anywhere; a ring has to be looked down
    /// on, or the near half of it lies over the far half and the whole figure
    /// is one band.
    fn pitch(&self) -> f64 {
        match self {
            Self::SphereWave { .. } => 18.0,
            Self::TorusCurve { .. } => 52.0,
        }
    }

    fn solid(&self, phase: f64) -> Solid {
        Solid::from_quads_reaching(self.faces(phase), self.reach())
    }
}

/// A piece that paints and is read back.
struct Drawn {
    drawing: Drawing,
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
        self.drawing.draw(&mut paper, phase, self.reader.colored);
        let Some(picture) = paper.picture() else {
            return blank();
        };
        self.reader.canvas(&picture)
    }

    fn loop_duration(&self) -> Option<f64> {
        (!self.still).then_some(self.period)
    }

    /// Columns per row for a grid this piece fills. A figure composed in a
    /// square wants a square frame; a wide grid would give it two empty margins
    /// and nothing else.
    fn frame_aspect(&self) -> Option<f64> {
        Some(CELL_ASPECT)
    }
}

/// A piece that is lit and projected.
///
/// The renderer is built once and handed a fresh surface every frame. Building
/// one per frame would sweep the probe sphere again for a light rig that has not
/// changed, and the rig is the expensive half — see [`Renderer::canvas_of`].
struct Modelled {
    model: Model,
    period: f64,
    still: bool,
    renderer: Renderer,
}

impl GlyphGenerator for Modelled {
    fn canvas(&self, columns: usize, rows: usize, time: f64) -> AsciiCanvas {
        let phase = if self.still { 0.0 } else { (time / self.period).rem_euclid(1.0) };
        self.renderer
            .canvas_of(&self.model.solid(phase), columns, rows, self.renderer.yaw)
    }

    fn loop_duration(&self) -> Option<f64> {
        (!self.still).then_some(self.period)
    }

    fn frame_aspect(&self) -> Option<f64> {
        Some(self.renderer.frame_aspect())
    }
}

pub fn build(params: &Params) -> Result<Generator, String> {
    let name = params.string("piece").unwrap_or("hilbert");
    let piece = Piece::named(name, params, params.seed(7)?)?;
    let period = params.period()?;
    let still = params.is_set("still");

    Ok(Generator::Glyph(match piece {
        Piece::Drawn(drawing) => Box::new(Drawn {
            drawing,
            period,
            still,
            reader: Reader::from_params(params)?,
        }),
        Piece::Modelled(model) => {
            // Any phase would do to prepare it — the reach is declared, so every
            // frame's solid is bounded the same — and nought is the one that
            // needs no explaining.
            let mut renderer = Renderer::new(model.solid(0.0));
            // The camera holds still: the movement is the piece, and a turn on
            // top of it would only be one more thing happening.
            renderer.spins = false;
            renderer.yaw = params.f64("yaw", 0.0)?.to_radians();
            renderer.pitch = params.f64("pitch", model.pitch())?.to_radians();
            renderer.zoom = params.f64("zoom", 0.92)?;
            renderer.ramp = AsciiRamp::named(params.string("grade").unwrap_or("detailed"))?;
            Box::new(Modelled { model, period, still, renderer })
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::canvas::SPACE;

    /// A period the tests can reason about, and one no piece is tuned to.
    const PERIOD: f64 = 8.0;

    fn piece(name: &str, seed: u64) -> Piece {
        Piece::named(name, &Params::default(), seed).expect("a piece by that name")
    }

    /// Whichever of the two generators the piece asks for, reached the way the
    /// command line reaches it — so a piece that is offered but not wired up
    /// fails here rather than in front of somebody.
    fn made(name: &str, seed: u64) -> Box<dyn GlyphGenerator> {
        let mut params = Params::default();
        for (flag, value) in [
            ("piece", name.to_string()),
            ("seed", seed.to_string()),
            ("period", PERIOD.to_string()),
        ] {
            params.flags.insert(flag.into(), Some(value));
        }
        let Generator::Glyph(made) = build(&params).expect("the piece builds") else {
            panic!("{name} is not a glyph tool");
        };
        made
    }

    fn inked(canvas: &AsciiCanvas) -> usize {
        canvas.glyphs.iter().filter(|&&glyph| glyph != SPACE).count()
    }

    /// A piece has to arrive as a picture: one that marks nothing and one that
    /// fills every cell are equally not one.
    #[test]
    fn every_piece_draws_a_part_of_the_frame() {
        for name in PIECES {
            let canvas = made(name, 7).canvas(64, 30, 2.0);
            let marks = inked(&canvas);
            assert!(marks > 150, "{name} drew only {marks} cells");
            assert!(marks < canvas.glyphs.len(), "{name} filled the whole grid");
        }
    }

    /// The point of the tool: the last frame of an export meets the first, so
    /// the clip can be played round and round without a cut in it.
    ///
    /// Reading the frame at the period and comparing it with the frame at
    /// nought proves nothing — a phase is the time over the period, and those
    /// are the same phase. What has to be true is that the step across the seam
    /// is a step like the others: a piece that jumps there passes the round
    /// trip and still shows a cut. One did, and drawing it was the only way to
    /// find out.
    #[test]
    fn the_last_frame_of_every_piece_meets_the_first() {
        const STEPS: usize = 16;
        let apart = |one: &AsciiCanvas, other: &AsciiCanvas| {
            one.glyphs.iter().zip(&other.glyphs).filter(|(one, other)| one != other).count()
        };

        for name in PIECES {
            let made = made(name, 7);
            let frames: Vec<AsciiCanvas> = (0..STEPS)
                .map(|step| made.canvas(48, 22, PERIOD * step as f64 / STEPS as f64))
                .collect();

            assert_eq!(
                frames[0].glyphs,
                made.canvas(48, 22, PERIOD).glyphs,
                "{name} does not come back round"
            );
            let inside = (1..STEPS).map(|step| apart(&frames[step - 1], &frames[step]));
            let widest = inside.max().expect("a loop of more than one frame");
            let seam = apart(&frames[STEPS - 1], &frames[0]);
            // A piece that moves evenly has every step the same size, and which
            // of them comes out widest is then a matter of a cell or two. So the
            // seam is allowed the widest step and a twentieth of it: far below
            // anything a piece that actually cuts would show, which is several
            // times the step either side of it.
            let allowed = widest + widest / 20 + 1;
            assert!(
                seam <= allowed,
                "{name} moves {seam} cells across the seam and at most {widest} anywhere inside"
            );
        }
    }

    /// And the middle of the loop is not the start of it.
    #[test]
    fn every_piece_moves_inside_its_period() {
        for name in PIECES {
            let made = made(name, 7);
            let start = made.canvas(64, 30, 0.0);
            let middle = made.canvas(64, 30, PERIOD * 0.37);
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
    ///
    /// Not every piece has room for one — a zoom that is exactly self-similar
    /// is the same figure however it is asked for — so a piece is held to what
    /// it took rather than to a list kept by hand: one that read the seed has
    /// to draw differently, and one that did not has to draw the same.
    #[test]
    fn the_same_seed_draws_the_same_frame() {
        for name in PIECES {
            let one = made(name, 7).canvas(64, 30, 1.5);
            let same = made(name, 7).canvas(64, 30, 1.5);
            assert_eq!(one.glyphs, same.glyphs, "{name} is not repeatable");

            let other = made(name, 8).canvas(64, 30, 1.5);
            if piece(name, 7) == piece(name, 8) {
                assert_eq!(one.glyphs, other.glyphs, "{name} took a seed it has not got");
            } else {
                assert_ne!(one.glyphs, other.glyphs, "{name} ignores its seed");
            }
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
