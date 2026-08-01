//! A spiral wave running out under a drift of particles.
//!
//! After the spiral surface in [Bleuje's animations][ref], written here from the
//! idea rather than from that sketch.
//!
//! The figure is one plane and one crowd. The plane's height is a plain sine
//! wave, delayed by how far out a point is *and* by which way round it lies —
//! and a delay that reads the angle is a spiral, so the crest winds outward
//! instead of ringing. The crowd is particles, each on its own fixed ray,
//! crawling out from the middle a little above the surface and rising and
//! falling with it.
//!
//! The plane is drawn in the paper's own colour, so nothing of it is visible.
//! That is deliberate, and it is the whole reason this tool draws pixels: what
//! the plane is there for is to stand in the way. A particle over the far slope
//! of a swell is hidden by the near one, and that occlusion is the only thing
//! saying the crowd lies on a surface rather than swimming in a fog. Read back
//! as characters the plane would have to take a shade of its own, and the piece
//! would be a lit relief with some dust on it — a different picture.
//!
//! A drawing or a photograph can be laid on the disc the drift covers, and then
//! the crowd is what shows it: a particle takes the light it is standing over,
//! and one standing over the picture's paper is not drawn at all. The picture
//! holds still while the crowd walks out through it, so what arrives is not the
//! picture but the picture being carried — thinning where it is dark, riding the
//! swell where the wave is under it. See [`Subject`].
//!
//! Every length here is a fraction of the frame's height, so the same numbers
//! compose the preview and the poster. The eye stands at the distance where
//! [`FIELD`] takes in exactly one of those heights, which is what makes that
//! true — see [`eye`].
//!
//! Both halves close their own loop by construction. The wave depends on the
//! phase only through a sine of it. A particle is drawn as [`COPIES`] copies
//! evenly spaced along its own run, so a full period walks each copy onto where
//! the next one stood — and the one that falls off the end is at the end of the
//! run, where its size is nought, as is the one arriving at the start. Nothing
//! appears or disappears at the seam.
//!
//! [ref]: https://github.com/Bleuje/processing-animations-code

use std::f64::consts::{FRAC_PI_3, PI, TAU};

use image::imageops::{resize, FilterType};
use image::RgbaImage;

use crate::art::canvas::{AsciiCanvas, AsciiColor, CELL_ASPECT};
use crate::art::export;
use crate::art::generator::{Generator, PixelGenerator};
use crate::art::motion::scatter;
use crate::art::params::Params;
use crate::art::raster::{Raster, Seen};
use crate::art::read::{open, raster_of, Source, Tones};

use super::ascii::Vector3;

/// How much the eye takes in from the top of the picture to the bottom.
const FIELD: f64 = FRAC_PI_3;

/// How far the plane runs from edge to edge, in frames.
///
/// Wider than the picture, so it fills it from the horizon down and the eye is
/// never shown where it stops.
const REACH: f64 = 1.05;

/// How far up the picture the middle of the plane is set, in frames. Without it
/// the horizon lies across the centre and half the frame is empty sky.
const LIFT: f64 = 0.09;

/// How tall the wave stands where the plane is one unit out, in frames.
const SWELL: f64 = 1.0 / 20.0;

/// How many times the wave repeats between the middle and the rim, and how far
/// round it is dragged over a whole turn.
///
/// The second is one, and has to be a whole number: it is read off an angle, and
/// an angle is a quantity that comes back to where it started. Anything else
/// leaves a step down the ray where the angle wraps — a crease running out of
/// the middle of the piece, in every frame.
const RINGS: f64 = 8.0;
const TWIST: f64 = 1.0;

/// How far a particle rides above the surface, so it is not fighting the plane
/// it lies on for the same pixel.
const RIDE: f64 = 1.0 / 400.0;

/// How far out a particle may start, and how far it travels from there.
///
/// They add to a half, which is where the plane ends. A particle that ran past
/// that would be the one thing in the piece with nothing behind it — a speck
/// hanging in the dark, still rising and falling with a surface that is no
/// longer under it.
const START: f64 = 0.15;
const TRAVEL: f64 = 0.35;

/// The largest a particle is drawn, as a radius in frames.
const GRAIN: f64 = 1.0 / 320.0;

/// The longest side a subject is read down to.
///
/// The crowd is the only thing that draws it, and tens of thousands spread over
/// the disc stand a couple of hundred apart at best — so anything finer is
/// detail no particle will ever be standing on. Read down rather than sampled at
/// a point, which is the difference between a photograph and its own noise.
const SAMPLE: u32 = 256;

/// Below this a particle is standing on the picture's own paper and is not
/// drawn.
///
/// A dot carries how much light it found in how strongly it is drawn, so a faint
/// one is honest and this does not have to cut high. It is here so the dark half
/// of a picture comes out empty rather than dusted over: a haze reads as a grey
/// wash, and paper showing through is most of what makes a picture drawn in dots
/// legible at all.
const FAINT: f32 = 1.0 / 24.0;

/// How many places along its run a particle is drawn at once.
///
/// One particle drawn six times, not six particles: they share a ray and a size,
/// and they are spread evenly along the run, so the ray stays evenly populated
/// as the phase turns rather than thinning out behind the crowd and refilling in
/// a rush. It is also what closes the loop — see the note at the top.
const COPIES: usize = 6;

/// How many particles the drift is made of.
///
/// Below the floor it is a handful of specks rather than a crowd; above the
/// ceiling a frame costs more to draw than it gains, and the surface is
/// carpeted anyway.
fn count(given: usize) -> usize {
    given.clamp(200, 60_000)
}

/// How finely the plane is cut, in quads along each side.
///
/// The plane is invisible, so this buys nothing but the accuracy of the edge it
/// hides things behind. Too coarse and a swell occludes in steps.
fn mesh(given: usize) -> usize {
    given.clamp(16, 400)
}

/// How far over the disc a subject is spread, as a share of the whole of it.
///
/// Below the floor the picture is a speck in the middle of a crowd that is
/// nearly all standing off it. Past the ceiling it is a crop that is being asked
/// for rather than a fit — which is a fair thing to want, so the ceiling is set
/// where a quarter of the picture still reaches the disc rather than where the
/// whole of it does.
fn spread(given: f64) -> f64 {
    given.clamp(0.1, 4.0)
}

/// How far the eye stands off the plane's own middle, in frames.
///
/// The distance at which [`FIELD`] takes in exactly one frame from top to
/// bottom. That is what lets every length here be a fraction of the picture: a
/// figure half a frame tall standing at the middle fills half the picture,
/// whatever size the picture was asked for.
fn eye() -> f64 {
    0.5 / (FIELD / 2.0).tan()
}

/// How near the eye the picture is cut off, in frames.
///
/// A fiftieth of where the eye stands. The plane runs well past the lens at this
/// pitch, so something has to be cut; near enough that only what was going to
/// pass through the lens is.
fn near() -> f64 {
    eye() / 50.0
}

/// One particle: which way out it lies, where along that it starts, how far
/// behind the others it sets off, and how large it is drawn.
///
/// Settled once when the tool is built rather than per frame — the drift is a
/// composition, and one redrawn every frame is a fog.
#[derive(Clone, Copy)]
struct Particle {
    angle: f64,
    start: f64,
    offset: f64,
    size: f64,
}

impl Particle {
    fn new(seed: u64, index: usize) -> Self {
        let at = |which: u64| scatter(seed, index as u64 * 4 + which);
        Self {
            angle: at(0) * TAU,
            // Weighted toward the middle, so the crowd sets off from a knot
            // rather than from an even wash over the whole disc.
            start: at(1).powf(1.4) * START,
            offset: at(2),
            // Squared, so most are fine and a few are large. An even spread of
            // sizes reads as one size drawn badly.
            size: at(3).powi(2) * GRAIN,
        }
    }

    /// Where over the plane it stands when it is `along` of the way through its
    /// run, and how large it is drawn there.
    ///
    /// Flat — where it stands on the disc, before the wave is asked how high the
    /// plane is there. A subject is laid on the disc rather than draped over the
    /// swell, so this is the one point both of them are asked about.
    fn at(&self, along: f64) -> ((f64, f64), f64) {
        let out = self.start + along * TRAVEL;
        (
            (out * self.angle.cos(), out * self.angle.sin()),
            // Nought at both ends of the run and largest in the middle, so a
            // particle arrives and leaves rather than blinking on and off.
            self.size * (PI * along).sin().max(0.0).sqrt(),
        )
    }
}

/// A point of the piece in the frame it is composed in: `x` across, `y` down it,
/// `z` toward the eye — the plane's own upright, before anything is turned.
#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
    z: f64,
}

/// The plane at `(x, y)` out from its middle, at this phase.
///
/// The delay is what makes it a spiral: so many turns of the wave for the
/// distance out, and one more for the way round. Both feed the same sine, so the
/// crest is a curve that winds rather than a ring that grows.
fn surface(x: f64, y: f64, phase: f64) -> Point {
    let out = (x * x + y * y).sqrt();
    let delay = out * RINGS + TWIST * y.atan2(x) / TAU;
    // Taller further out, and by a root rather than in step, so the middle is
    // not a flat plate and the rim is not a wall.
    let height = SWELL * out.sqrt();
    Point {
        x: x * REACH,
        y: y * REACH,
        // Mostly under the plane and a little over it, which is what leaves the
        // swells reading as troughs with crests drawn between them.
        z: height * (4.0 * (TAU * (phase - delay)).sin() - 2.0),
    }
}

/// A picture for the crowd to carry, laid flat on the disc.
///
/// Read where a particle stands rather than where it set off, so the picture
/// holds still while the crowd walks out through it. Read the other way it would
/// be smeared along every ray at once — a picture being dragged apart rather
/// than one being shown.
///
/// Settled once at the size it will be read at, tones and all. What a frame then
/// costs is an index and a comparison a particle, whatever was opened.
struct Subject {
    /// How much light stands at each pixel, and what colour to draw it in — the
    /// picture's own where that was asked for, and the crowd's ink where it was
    /// not, so a particle never has to ask which.
    light: Vec<f32>,
    tint: Vec<[f32; 3]>,
    wide: usize,
    tall: usize,
    /// How far the picture reaches over the plane, across and away.
    half: (f64, f64),
}

impl Subject {
    /// `spread` is how far over the disc the picture is laid, and `ink` what the
    /// crowd draws in when the picture's own colours are not wanted.
    fn new(picture: &RgbaImage, spread: f64, tones: Tones, colored: bool, ink: [f32; 3]) -> Self {
        let (wide, tall) = (picture.width().max(1), picture.height().max(1));
        // In proportion, and only ever smaller: a picture already coarser than
        // the crowd is at the size the crowd can show, and blowing it up would
        // spread its own pixels out into squares nothing is asking for.
        let scale = (SAMPLE as f64 / wide.max(tall) as f64).min(1.0);
        let read = |side: u32| ((side as f64 * scale).round() as u32).max(1);
        let (wide, tall) = (read(wide), read(tall));
        let small = resize(picture, wide, tall, FilterType::Triangle);

        let light = small.pixels().map(|pixel| tones.light(&pixel.0)).collect();
        let tint = small
            .pixels()
            .map(|pixel| {
                if !colored {
                    return ink;
                }
                // The colour as it was written. Which end of the picture is the
                // subject is a question about its light, and asking it again of
                // the colours would leave a photograph in its own negative.
                [pixel.0[0], pixel.0[1], pixel.0[2]].map(|channel| channel as f32 / 255.0)
            })
            .collect();

        // The disc the drift covers is what the picture is laid over, since the
        // crowd is what has to carry it: any wider and the corners of it are out
        // where there is nobody standing.
        let disc = (START + TRAVEL) * spread;
        let long = wide.max(tall) as f64;
        Self {
            light,
            tint,
            wide: wide as usize,
            tall: tall as usize,
            half: (disc * wide as f64 / long, disc * tall as f64 / long),
        }
    }

    /// What the picture shows at this point of the plane: what a particle
    /// standing there is drawn in, and how strongly. Nothing off the picture,
    /// and nothing on its paper.
    fn at(&self, x: f64, y: f64) -> Option<([f32; 3], f64)> {
        // A picture counts its rows down from the top, and so does the plane:
        // its y runs away from the eye, and away from the eye is down the
        // picture at every pitch there is anything to see from.
        let across = (1.0 + x / self.half.0) / 2.0 * self.wide as f64;
        let down = (1.0 + y / self.half.1) / 2.0 * self.tall as f64;
        if across < 0.0 || down < 0.0 {
            return None;
        }
        let (across, down) = (across as usize, down as usize);
        if across >= self.wide || down >= self.tall {
            return None;
        }

        let at = down * self.wide + across;
        let light = self.light[at];
        (light > FAINT).then(|| (self.tint[at], light as f64))
    }
}

/// Where the eye stands, worked out once and used for every point of a frame.
struct View {
    cos_yaw: f64,
    sin_yaw: f64,
    cos_pitch: f64,
    sin_pitch: f64,
}

impl View {
    fn new(yaw: f64, pitch: f64) -> Self {
        Self {
            cos_yaw: yaw.cos(),
            sin_yaw: yaw.sin(),
            cos_pitch: pitch.cos(),
            sin_pitch: pitch.sin(),
        }
    }

    /// A point of the piece as the eye sees it: turned about the plane's own
    /// upright, tipped away, lifted up the picture, and counted from the eye
    /// outward.
    ///
    /// No pitch leaves the plane facing the eye square on; a right angle of it
    /// lays the plane flat as a floor.
    fn sees(&self, point: Point) -> Seen {
        let across = point.x * self.cos_yaw - point.y * self.sin_yaw;
        let along = point.x * self.sin_yaw + point.y * self.cos_yaw;
        let down = along * self.cos_pitch - point.z * self.sin_pitch;
        let toward = along * self.sin_pitch + point.z * self.cos_pitch;
        // The picture counts up where the piece counts down, and away where the
        // piece counts toward.
        Vector3::new(across, LIFT - down, eye() - toward)
    }
}

pub struct Spiral {
    particles: Vec<Particle>,
    /// The picture the crowd is carrying, or nothing and it carries its own ink.
    subject: Option<Subject>,
    mesh: usize,
    view: View,
    /// The lens, narrowed to magnify.
    field: f64,
    ink: [f32; 3],
    paper: [f32; 3],
    period: f64,
    still: bool,
}

impl Spiral {
    /// The frame at a phase rather than at a time, which is what the tests and
    /// [`PixelGenerator::frame`] both want and only one of them can say.
    fn picture(&self, width: u32, height: u32, phase: f64) -> RgbaImage {
        let mut raster = Raster::new(width, height, self.field, near(), self.paper);

        // Four quads meet at every corner of the grid, so a corner worked out
        // where it is used is worked out four times — and working one out is a
        // root, an arc and a sine. Settled once each into the grid, and the
        // quads then read off it.
        let side = self.mesh + 1;
        let step = 1.0 / self.mesh.max(1) as f64;
        let out = |count: usize| count as f64 * step - 0.5;
        let grid: Vec<Seen> = (0..side * side)
            .map(|at| self.view.sees(surface(out(at % side), out(at / side), phase)))
            .collect();

        for across in 0..self.mesh {
            for down in 0..self.mesh {
                let at = down * side + across;
                let patch = [grid[at], grid[at + 1], grid[at + side + 1], grid[at + side]];
                raster.triangle([patch[0], patch[1], patch[2]], self.paper);
                raster.triangle([patch[0], patch[2], patch[3]], self.paper);
            }
        }

        for particle in &self.particles {
            let walked = (phase + particle.offset).rem_euclid(1.0);
            for copy in 0..COPIES {
                let along = (copy as f64 + walked) / COPIES as f64;
                let ((x, y), size) = particle.at(along);
                // Where the picture is paper the crowd is paper: a particle
                // standing off it, or on the dark of it, is not drawn at all.
                let (tint, alpha) = match &self.subject {
                    Some(subject) => match subject.at(x, y) {
                        Some(carried) => carried,
                        None => continue,
                    },
                    None => (self.ink, 1.0),
                };
                let mut point = surface(x, y, phase);
                point.z += RIDE;
                raster.dot(self.view.sees(point), size, tint, alpha);
            }
        }

        raster.into_image()
    }
}

impl PixelGenerator for Spiral {
    fn frame(&self, width: u32, height: u32, time: f64) -> RgbaImage {
        let phase = if self.still { 0.0 } else { time / self.period };
        self.picture(width, height, phase.rem_euclid(1.0))
    }

    fn loop_duration(&self) -> Option<f64> {
        (!self.still).then_some(self.period)
    }

    /// Square, which is the shape it was composed in. A cell is that many times
    /// taller than it is wide, so that many columns to a row lands square.
    fn frame_aspect(&self) -> Option<f64> {
        Some(CELL_ASPECT)
    }
}

fn tint(color: AsciiColor) -> [f32; 3] {
    [color.red, color.green, color.blue].map(|channel| channel as f32 / 255.0)
}

fn assemble(params: &Params) -> Result<Spiral, String> {
    let colour = |name, fallback| match params.string(name) {
        Some(text) => AsciiColor::from_hex(text),
        None => Ok(fallback),
    };
    let seed = params.seed(7)?;
    let zoom = params.f64("zoom", 1.0)?;
    if zoom <= 0.0 {
        return Err("--zoom is how much the lens magnifies, so it has to be positive".into());
    }

    let ink = tint(colour("ink", export::INK)?);
    // Read whether or not there turns out to be anything to read, so a line that
    // asks for a contrast the app cannot give is refused rather than quietly
    // drawn without it.
    let over = spread(params.f64("spread", 1.0)?);
    let tones = Tones::from_params(params)?;
    let colored = params.is_set("color");

    // Anything the app can open is a subject here, and none is a subject too:
    // without one the drift is drawn in its own ink, as it always was.
    let laid = |picture: &RgbaImage| Subject::new(picture, over, tones, colored, ink);
    let written = |text: &str| laid(&raster_of(&AsciiCanvas::from_text(text)));
    let subject = match params.string("text") {
        // `--text` carries a drawing inline, which is how the window offers a
        // sample without a file whose path differs between dev and a bundle.
        Some(inline) => Some(written(inline)),
        None => match params.first_positional() {
            Some(path) => Some(match &*open(path)? {
                Source::Drawing(text) => written(text),
                Source::Picture(picture) => laid(picture),
            }),
            None => None,
        },
    };

    Ok(Spiral {
        particles: (0..count(params.usize("count", 17_000)?))
            .map(|index| Particle::new(seed, index))
            .collect(),
        subject,
        mesh: mesh(params.usize("mesh", 130)?),
        // The angles it was composed at. The plane is turned about its own
        // upright before it is tipped, so half a right angle of yaw puts a
        // corner of it toward the eye rather than an edge.
        view: View::new(
            params.f64("yaw", 45.0)?.to_radians(),
            params.f64("pitch", 52.2)?.to_radians(),
        ),
        field: FIELD / zoom,
        ink,
        paper: tint(colour("paper", export::PAPER)?),
        period: params.period()?,
        still: params.is_set("still"),
    })
}

pub fn build(params: &Params) -> Result<Generator, String> {
    Ok(Generator::Pixel(Box::new(assemble(params)?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// Small, because each of these renders whole pictures.
    const WIDE: u32 = 96;
    const TALL: u32 = 96;

    /// A drawing with nothing in it but ink, so what a subject does to the crowd
    /// is the only thing under test.
    const BLOCK: &str = "@@@@@\n@@@@@\n@@@@@\n@@@@@\n@@@@@";

    fn made(flags: &[(&str, &str)]) -> Spiral {
        let mut params = Params::default();
        // A crowd and a cut fine enough to mean something, coarse enough that a
        // test is not a render.
        params.flags.insert("count".into(), Some("900".into()));
        params.flags.insert("mesh".into(), Some("40".into()));
        for (flag, value) in flags {
            params.flags.insert((*flag).into(), Some((*value).into()));
        }
        assemble(&params).expect("the tool builds")
    }

    fn drawn(picture: &RgbaImage) -> usize {
        picture.pixels().filter(|pixel| pixel.0[0] > 24).count()
    }

    fn apart(one: &RgbaImage, other: &RgbaImage) -> usize {
        one.pixels().zip(other.pixels()).filter(|(one, other)| one != other).count()
    }

    /// The whole claim of the piece: the end of the period is the start of it.
    ///
    /// Exactly, not nearly. The wave is a sine of the phase and a particle's
    /// copies walk round onto each other, so there is nothing here that closes
    /// only to within a rounding.
    #[test]
    fn a_period_leaves_the_picture_where_it_found_it() {
        let spiral = made(&[]);
        let start = spiral.picture(WIDE, TALL, 0.0);
        let round = spiral.picture(WIDE, TALL, 1.0);
        assert_eq!(apart(&start, &round), 0);
    }

    /// And the middle of it is somewhere else.
    #[test]
    fn the_picture_moves_inside_its_period() {
        let spiral = made(&[]);
        let start = spiral.picture(WIDE, TALL, 0.0);
        let middle = spiral.picture(WIDE, TALL, 0.37);
        assert!(apart(&start, &middle) > 40, "{}", apart(&start, &middle));
    }

    /// A picture, rather than an empty frame or a filled one.
    #[test]
    fn the_drift_marks_a_part_of_the_frame() {
        let picture = made(&[]).picture(WIDE, TALL, 0.2);
        let marks = drawn(&picture);
        assert!(marks > 100, "only {marks} pixels were lit");
        assert!(marks < picture.pixels().len() / 2, "{marks} pixels were lit");
    }

    /// The delay reads an angle, and an angle wraps. A twist that was not a
    /// whole turn would leave a step down the ray where it wraps, so the two
    /// sides of that ray have to agree.
    #[test]
    fn the_wave_has_no_step_where_the_angle_comes_round() {
        for phase in [0.0, 0.31, 0.68] {
            for out in [0.1, 0.4, 0.8] {
                let above = surface(-out, 1e-9, phase).z;
                let below = surface(-out, -1e-9, phase).z;
                assert!((above - below).abs() < 1e-6, "{above} against {below} at {out}");
            }
        }
    }

    /// A seed is a promise that the same line can be typed twice, and that
    /// another one is worth typing.
    #[test]
    fn the_same_seed_draws_the_same_frame() {
        let one = made(&[("seed", "7")]).picture(WIDE, TALL, 0.4);
        let same = made(&[("seed", "7")]).picture(WIDE, TALL, 0.4);
        assert_eq!(apart(&one, &same), 0);

        let other = made(&[("seed", "8")]).picture(WIDE, TALL, 0.4);
        assert!(apart(&one, &other) > 0, "the seed changed nothing");
    }

    /// What the plane is for. It is drawn in the paper's own colour, so the only
    /// mark it leaves on the picture is the particles it takes away.
    ///
    /// A cut of nothing is a plane with no quads in it, which is the one way to
    /// ask for the same drift with nothing standing in front of it. [`mesh`]
    /// keeps that off a command line.
    #[test]
    fn the_plane_hides_what_is_behind_it() {
        let mut spiral = made(&[]);
        let hidden = drawn(&spiral.picture(WIDE, TALL, 0.25));
        spiral.mesh = 0;
        let all = drawn(&spiral.picture(WIDE, TALL, 0.25));
        assert!(hidden < all, "{hidden} lit against {all} with nothing in the way");
    }

    /// Held still it is one frame, and it says so rather than reporting a loop
    /// nothing travels over.
    #[test]
    fn holding_it_still_leaves_nothing_to_loop() {
        let mut params = Params::default();
        params.flags.insert("count".into(), Some("200".into()));
        params.flags.insert("still".into(), None);
        assert_eq!(build(&params).expect("the tool builds").loop_duration(), None);
    }

    #[test]
    fn the_crowd_and_the_cut_stay_worth_drawing() {
        assert_eq!(count(0), 200);
        assert_eq!(count(17_000), 17_000);
        assert_eq!(count(1_000_000), 60_000);
        assert_eq!(mesh(0), 16);
        assert_eq!(mesh(130), 130);
        assert_eq!(mesh(9_999), 400);
        assert_eq!(spread(0.0), 0.1);
        assert_eq!(spread(1.0), 1.0);
        assert_eq!(spread(90.0), 4.0);
    }

    const LIT: [u8; 4] = [255, 255, 255, 255];
    const UNLIT: [u8; 4] = [0, 0, 0, 255];

    fn painted(shade: impl Fn(u32, u32) -> [u8; 4]) -> RgbaImage {
        RgbaImage::from_fn(32, 32, |x, y| Rgba(shade(x, y)))
    }

    /// The crowd with a picture on it, seen square on so the picture is where
    /// the plane says it is rather than where the camera has swung it to.
    fn carrying(picture: &RgbaImage, colored: bool) -> Spiral {
        let mut spiral = made(&[("yaw", "0"), ("pitch", "0")]);
        spiral.subject = Some(Subject::new(picture, 1.0, Tones::PLAIN, colored, spiral.ink));
        spiral
    }

    /// What the crowd is for once it has a picture: it is the picture. A
    /// particle over paper draws nothing, so paper all through leaves the frame
    /// with the plane on it and nothing else.
    #[test]
    fn a_subject_of_paper_leaves_the_crowd_nothing_to_show() {
        let dark = carrying(&painted(|_, _| UNLIT), false).picture(WIDE, TALL, 0.2);
        assert_eq!(drawn(&dark), 0);

        let light = carrying(&painted(|_, _| LIT), false).picture(WIDE, TALL, 0.2);
        assert!(drawn(&light) > 100, "only {} pixels were lit", drawn(&light));
    }

    /// Which way up and which way round it lies. A sign read the wrong way here
    /// is a piece printed mirrored or upside down, and neither is something the
    /// piece itself would ever look wrong enough to give away.
    #[test]
    fn a_subject_lies_on_the_plane_the_way_it_was_written() {
        let middle = |picture: &RgbaImage| {
            let lit: Vec<(f64, f64)> = picture
                .enumerate_pixels()
                .filter(|(_, _, pixel)| pixel.0[0] > 24)
                .map(|(x, y, _)| (x as f64, y as f64))
                .collect();
            assert!(!lit.is_empty(), "nothing was drawn to take the middle of");
            let count = lit.len() as f64;
            (
                lit.iter().map(|at| at.0).sum::<f64>() / count,
                lit.iter().map(|at| at.1).sum::<f64>() / count,
            )
        };
        let half = |keep: fn(u32, u32) -> bool| {
            let picture = painted(|x, y| if keep(x, y) { LIT } else { UNLIT });
            middle(&carrying(&picture, false).picture(WIDE, TALL, 0.2))
        };

        let (left, right) = (half(|x, _| x < 16), half(|x, _| x >= 16));
        assert!(left.0 < right.0, "{} against {}", left.0, right.0);

        let (top, bottom) = (half(|_, y| y < 16), half(|_, y| y >= 16));
        assert!(top.1 < bottom.1, "{} against {}", top.1, bottom.1);
    }

    /// Asked for the picture's colours, the crowd takes them. Left alone it
    /// keeps its own ink, whatever the picture was painted in.
    #[test]
    fn the_crowd_takes_its_colour_from_the_picture_when_it_is_asked_to() {
        // Pixels the red end of the picture has plainly reached, which the ink
        // is grey enough that nothing it draws can ever be.
        let reddened = |picture: &RgbaImage| {
            picture.pixels().filter(|pixel| pixel.0[0] > pixel.0[2] + 24).count()
        };
        let red = painted(|_, _| [255, 40, 40, 255]);

        let plain = carrying(&red, false).picture(WIDE, TALL, 0.2);
        assert_eq!(reddened(&plain), 0);

        let colored = carrying(&red, true).picture(WIDE, TALL, 0.2);
        assert!(reddened(&colored) > 20, "only {} pixels came out red", reddened(&colored));
    }

    /// The picture is carried rather than pulled along: it is read where a
    /// particle stands, so it is as periodic as the crowd is and the loop it
    /// closes is the same one.
    #[test]
    fn a_period_leaves_a_carried_picture_where_it_found_it() {
        let checks = painted(|x, y| if (x / 4 + y / 4) % 2 == 0 { LIT } else { UNLIT });
        let spiral = carrying(&checks, false);
        let start = spiral.picture(WIDE, TALL, 0.0);
        let round = spiral.picture(WIDE, TALL, 1.0);
        assert_eq!(apart(&start, &round), 0);
    }

    /// A drawing arrives the same way a picture does, and the spread is how much
    /// of the drift is standing on it.
    #[test]
    fn a_tighter_spread_lays_the_subject_over_less_of_the_drift() {
        let whole = drawn(&made(&[("text", BLOCK)]).picture(WIDE, TALL, 0.2));
        let tight = drawn(&made(&[("text", BLOCK), ("spread", "0.4")]).picture(WIDE, TALL, 0.2));
        assert!(tight > 0, "the drawing was lost altogether");
        assert!(tight < whole, "{tight} lit against {whole} laid over the whole disc");
    }

    #[test]
    fn a_lens_that_magnifies_by_nothing_is_refused() {
        let mut params = Params::default();
        params.flags.insert("zoom".into(), Some("0".into()));
        assert!(build(&params).is_err());
    }
}

