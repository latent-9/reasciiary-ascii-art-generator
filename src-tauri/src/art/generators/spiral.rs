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

use image::RgbaImage;

use crate::art::canvas::{AsciiColor, CELL_ASPECT};
use crate::art::export;
use crate::art::generator::{Generator, PixelGenerator};
use crate::art::motion::scatter;
use crate::art::params::Params;
use crate::art::raster::{Raster, Seen};

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

    /// Where it stands when it is `along` of the way through its run, and how
    /// large it is drawn there.
    fn at(&self, along: f64, phase: f64) -> (Point, f64) {
        let out = self.start + along * TRAVEL;
        let mut point = surface(out * self.angle.cos(), out * self.angle.sin(), phase);
        point.z += RIDE;
        // Nought at both ends of the run and largest in the middle, so a
        // particle arrives and leaves rather than blinking on and off.
        (point, self.size * (PI * along).sin().max(0.0).sqrt())
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
                let (point, size) = particle.at(along, phase);
                raster.dot(self.view.sees(point), size, self.ink, 1.0);
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

    Ok(Spiral {
        particles: (0..count(params.usize("count", 17_000)?))
            .map(|index| Particle::new(seed, index))
            .collect(),
        mesh: mesh(params.usize("mesh", 130)?),
        // The angles it was composed at. The plane is turned about its own
        // upright before it is tipped, so half a right angle of yaw puts a
        // corner of it toward the eye rather than an edge.
        view: View::new(
            params.f64("yaw", 45.0)?.to_radians(),
            params.f64("pitch", 52.2)?.to_radians(),
        ),
        field: FIELD / zoom,
        ink: tint(colour("ink", export::INK)?),
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

    /// Small, because each of these renders whole pictures.
    const WIDE: u32 = 96;
    const TALL: u32 = 96;

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
    }

    #[test]
    fn a_lens_that_magnifies_by_nothing_is_refused() {
        let mut params = Params::default();
        params.flags.insert("zoom".into(), Some("0".into()));
        assert!(build(&params).is_err());
    }
}

