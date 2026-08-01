//! A spiral wave running out under a drift of particles.
//!
//! After the spiral surface in [Bleuje's animations][ref], written here from the
//! idea rather than from that sketch.
//!
//! The figure is one plane and one crowd. The plane's height is a plain sine
//! wave, delayed by how far out a point is *and* by which way round it lies —
//! and a delay that reads the angle is a spiral, so the crest winds outward
//! instead of ringing. How much of each delay there is settles what the piece
//! is: rings growing out of the middle where no angle is read, one arm or six
//! where it is, winding either way round. See [`Wave`]. The disc the whole of
//! that runs over is flat as the piece was composed and need not be — lifted, it
//! is a dome with the wave still running over it, and the crowd stacks against
//! its edge rather than spreading over its face. The crowd is particles, each on
//! its own fixed ray, crawling out from the middle a little above the surface
//! and rising and falling with it.
//!
//! The plane is drawn in the paper's own colour, so nothing of it is visible.
//! That is deliberate, and it is the whole reason this tool draws pixels: what
//! the plane is there for is to stand in the way. A particle over the far slope
//! of a swell is hidden by the near one, and that occlusion is the only thing
//! saying the crowd lies on a surface rather than swimming in a fog. Read back
//! as characters the plane would have to take a shade of its own, and the piece
//! would be a lit relief with some dust on it — a different picture.
//!
//! A drawing or a photograph can be laid on the disc, and then the piece is
//! drawn a second way. The crowd goes, and in its place the marks stand along
//! one line wound out from the middle — an engraver's line, thickening where the
//! light is and thinning to nothing where it is not, so what arrives is the
//! picture rather than a haze in roughly its shape. Each mark takes the light it
//! is standing over as its size, the way a halftone takes a tone, and one
//! standing over the picture's paper is not drawn at all. The picture holds
//! still while the line turns through it, riding the swell where the wave is
//! under it. See [`Winding`] and [`Subject`].
//!
//! Every length here is a fraction of the frame's height, so the same numbers
//! compose the preview and the poster. The eye stands at the distance where
//! [`FIELD`] takes in exactly one of those heights, which is what makes that
//! true — see [`eye`].
//!
//! Every part of it closes its own loop by construction. The wave depends on the
//! phase only through a sine of it, and the wandering laid over that only through
//! a circle walked in a field that has no ends to walk off — see
//! [`Wave::wandered`]. A particle is drawn as [`COPIES`] copies
//! evenly spaced along its own run, so a full period walks each copy onto where
//! the next one stood — and the one that falls off the end is at the end of the
//! run, where its size is nought, as is the one arriving at the start. The
//! winding turns a whole number of revolutions over the period, which puts every
//! mark back where it started, and none of them is a whole number too. Nothing
//! appears or disappears at the seam, at any setting the tool will take — which
//! is why the two that are read off an angle are held to whole numbers rather
//! than offered as they come.
//!
//! [ref]: https://github.com/Bleuje/processing-animations-code

use std::f64::consts::{FRAC_PI_3, PI, TAU};

use image::imageops::{resize, FilterType};
use image::RgbaImage;
use noise::Perlin;

use crate::art::canvas::{AsciiCanvas, AsciiColor, CELL_ASPECT};
use crate::art::export;
use crate::art::generator::{Generator, PixelGenerator};
use crate::art::motion::{circle_noise, scatter};
use crate::art::params::Params;
use crate::art::raster::{Raster, Seen};
use crate::art::read::{open, plain_light, raster_of, Fit, Source, Tones};

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

/// How tall the wave stands where the plane is one unit out, in frames, as it
/// was composed.
///
/// Where it opens rather than the whole of what it can be. See [`swell`].
const SWELL: f64 = 1.0 / 20.0;

/// How much of that is left standing once a picture is laid on the disc.
///
/// A swell at its full height carries the plane about a quarter of a frame
/// toward the eye and away again, eight times over between the middle and the
/// rim. On a scatter that reads as depth, because a scatter has no shape to
/// lose. On a drawing it reads as damage: the lens magnifies what is near it and
/// shrinks what is far, so each swell drags the part of the picture standing on
/// it outward and the next one hauls it back, and what arrives is the subject
/// torn into eight rings of itself.
///
/// It is also bought for nothing at the angle a picture is looked at from. The
/// tool opens nearly overhead when it is handed a file, and from overhead a
/// swell hides nothing behind it and casts no profile — all it does there is the
/// dragging. So the disc settles under a picture: enough wave left to see the
/// line rise and fall over it, not enough to pull the drawing apart.
const SETTLED: f64 = 0.25;

/// The wave the piece was composed on: how many times it repeats between the
/// middle and the rim, and how many arms it winds those repeats into.
///
/// Where it opens rather than the whole of what it can be — both are asked for
/// now. See [`Wave`], and [`rings`] and [`arms`] for what either is held to.
const RINGS: f64 = 8.0;
const ARMS: f64 = 1.0;

/// How far the surface wanders off the wave, in frames, as it was composed.
///
/// The one number in this file that is not the piece as it was first drawn. It
/// was nought, and what nought gives is a run of crests all the same height, the
/// same distance apart, each a perfect circle or a perfect spiral — and the eye
/// reads that as a diagram of a wave rather than as a wave. Every other flag
/// here changes which diagram it is. This one is what stops it being one.
///
/// Set low: the wave is still what the piece is, and this is the amount that
/// leaves the crests wandering and varying without burying them. See [`churn`],
/// which is where the whole of the argument is.
const CHURN: f64 = 0.035;

/// How fine the wandering is, in lobes across the plane, and how far round its
/// own circle the field turns over a loop.
///
/// Not offered. How coarse the wander is, is a fact about the plane carrying it
/// rather than a thing to want: cut into a hundred and thirty quads a side, the
/// plane draws the finer of the two scales in [`Wave::wandered`] with some ten
/// quads to a lobe, and halving that again is where a lobe starts arriving as a
/// handful of facets rather than as a curve. A slider for it would be a slider
/// that spoils the piece at one end and does nothing at the other.
///
/// How far the field turns is the same kind of fact, read off the other end: it
/// is how much of the wandering happens within one loop rather than sitting
/// still under it. Far enough that a crest is somewhere else by the end and near
/// enough that the surface is not boiling.
const SCALE: f64 = 6.0;
const DRIFTING: f64 = 0.6;

/// How far the disc it runs over is lifted into a dome, as it was composed.
///
/// Flat, which is the one amount that leaves the piece its horizon: a plane seen
/// nearly edge-on runs off the top of the frame, and everything the tool does
/// with [`LIFT`] and [`REACH`] is about that. See [`dome`].
const DOME: f64 = 0.0;

/// How many turns the line makes over a period, as it was composed.
///
/// One, which is the amount that leaves every mark of it where it started.
const SPIN: f64 = 1.0;

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
/// Set by how close together the marks that draw it stand. A line wound two
/// hundred times over the disc lays them four hundred across it, so a picture
/// read any coarser than that is one whose detail the line reaches for and does
/// not find. Read down rather than sampled at a point, which is the difference
/// between a photograph and its own noise.
const SAMPLE: u32 = 512;

/// Below this a particle is standing on the picture's own paper and is not
/// drawn, unless a line says otherwise.
///
/// A dot carries how much light it found in how large it is drawn, so a faint
/// one is honest and this does not have to cut high. It is set at all so the
/// dark half of a picture comes out empty rather than dusted over: a haze reads
/// as a grey wash, and paper showing through is most of what makes a picture
/// drawn in dots legible at all.
///
/// Where exactly it falls is a judgement about the picture rather than about the
/// piece, though — a photograph with its subject in shadow wants it low, and a
/// drawing meant to read as a stencil wants it high — so it is only where the
/// cut starts. See [`floor`].
const FAINT: f64 = 0.04;

/// How faint the light gets before the picture is taken to be paper there.
///
/// The ceiling stops short of the whole: past it the light left over is so
/// narrow that all a picture has is its brightest handful of pixels, and what
/// arrives is not a stencil of the subject but a scatter of specks off it.
fn floor(given: f64) -> f64 {
    given.clamp(0.0, 0.9)
}

/// Below this a picture has no range worth opening and is left alone.
///
/// A picture all one tone has nothing to stretch — the two ends are the same
/// number, and pulling them apart would turn whatever noise lies between them
/// into the subject.
const FLAT: f32 = 1.0 / 64.0;

/// Above this much light on average, a picture is mostly its own paper and it is
/// the ink that gets carried.
///
/// Light is drawn as the size of a mark, so a picture that is bright nearly
/// everywhere is a line that runs solid nearly everywhere — a coil, with nothing
/// in it to say a file was ever opened. That is not a rare kind of picture to be
/// handed. A signature, a logo, a diagram, a screenshot of a page: every one of
/// them is a little ink on a great deal of white, and read with the bright end
/// for the subject every one of them comes back as a wound thread and no
/// subject.
///
/// Well above a half, because a picture with tones either side of the middle is
/// a photograph and turning a photograph over is never what was meant. It takes
/// a picture that is mostly one bright thing before the tool says so.
const MOSTLY: f32 = 0.6;

/// How far a picture is opened out to its own two ends before it is read.
///
/// Nought reads the light as it stands. A whole puts the picture's own darkest
/// pixel at paper and its brightest at a full mark, which is the difference
/// between a low-key source arriving and not: light is carried as the size of a
/// mark, so a picture whose light never rises far is a picture drawn entirely in
/// marks too fine to see. A screenshot of pale text on a dark field is the worst
/// case and a common one — downsampled, every stroke in it averages away to a
/// few hundredths, and the whole picture comes out under the paper [`floor`].
fn opened(given: f64) -> f64 {
    given.clamp(0.0, 1.0)
}

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

/// How many times the line goes round on its way out to the rim.
///
/// Below the floor the windings stand so far apart that what is between them is
/// paper rather than picture, and the piece is a coil with a subject somewhere
/// behind it. The ceiling is where a mark is finer than the pixel it is drawn on
/// in anything but a poster, and where the count of them — which grows as the
/// square of this — starts costing a frame more than the detail is worth.
fn windings(given: usize) -> usize {
    given.clamp(8, 200)
}

/// How finely the plane is cut, in quads along each side.
///
/// The plane is invisible, so this buys nothing but the accuracy of the edge it
/// hides things behind. Too coarse and a swell occludes in steps.
fn mesh(given: usize) -> usize {
    given.clamp(16, 400)
}

/// How many times the wave repeats between the middle and the rim.
///
/// None of them is a real answer and not a broken one: with no repeat out the
/// ray the delay is read off the angle alone, and the disc is one swell sweeping
/// round instead of a run of them travelling out. The ceiling is where the
/// crests stand closer together than the plane is cut, and a wave finer than the
/// surface carrying it is drawn as a shudder rather than as a wave.
fn rings(given: f64) -> f64 {
    given.clamp(0.0, 20.0)
}

/// How tall the wave stands, in frames at one unit out.
///
/// None of it is a plane with no wave in it, which leaves the crowd crawling out
/// over a flat disc and is the one setting where nothing at all is hidden behind
/// anything. The ceiling is four times what the piece was composed at, which
/// carries the plane about a frame's width toward the eye and away again: past
/// that the crests stand taller than the disc is wide and what is drawn is the
/// inside of a swell rather than a piece with swells in it.
///
/// A picture laid on the disc still settles it — see [`SETTLED`]. This is the
/// height before that is taken off, not after, so the flag means the same thing
/// whether or not a file is open.
fn swell(given: f64) -> f64 {
    given.clamp(0.0, 4.0 * SWELL)
}

/// How far the surface wanders off the wave, in frames.
///
/// The wave is one sine, and a sine says the same thing everywhere. Every crest
/// it draws stands the same height as its neighbours, the same distance out from
/// them, and holds the same shape all the way round — and a crowd lying over
/// that bunches into a run of hard even bands, because the plane hides the far
/// slope of each crest and the near edge of every one of them is in the same
/// place. What arrives is a machined thing. It is the piece's oldest complaint
/// and it is a fair one: the wave is not being drawn, it is being asserted.
///
/// So a field with no account of itself is added to the height. It is not a
/// second wave — a second wave is a second assertion, and two of them beating
/// against each other is still a pattern anybody can hear. It is noise, which is
/// the one thing here that cannot be read off a formula by looking at it. What
/// it does to the piece is small and is the whole point: the crests still travel
/// out and still wind into arms, but no two of them are alike, none is a circle,
/// and the bands come apart into ridges.
///
/// None of it is the wave alone, exactly as the piece was first drawn, which is
/// worth keeping reachable and is not worth opening on. The ceiling is the same
/// number the swell's is, and it is where the wandering stands about as tall as
/// the wave the piece opens on stands at its rim — see [`SWELL`] — past which the
/// piece is a rough surface that a wave is running under rather than a wave with
/// a roughness on it, and the arms cannot be found in it at all. Asked for at its
/// own ceiling the swell is several times that, which is the right way round: the
/// taller the wave stands, the less a fixed wandering takes off it.
///
/// A picture laid on the disc settles this the way it settles the swell, and for
/// the same reason: the lens drags whatever stands on a slope, and a picture
/// dragged is a picture torn. See [`SETTLED`].
fn churn(given: f64) -> f64 {
    given.clamp(0.0, 4.0 * SWELL)
}

/// How far the disc is lifted into a dome, as a share of its own radius.
///
/// None of it is the flat disc the piece was composed on, and one of it is a
/// half-sphere — the middle standing a full radius above the rim. Not further:
/// past a half-sphere the shape has to overhang itself to keep going, and a
/// height read off how far out a point lies cannot say that. What it would do
/// instead is climb faster than the mesh is cut and come apart at the rim.
///
/// Below none it is a bowl, which is the same shape looked into rather than at.
/// Worth having and not merely allowed: the crowd crawls out over the inside of
/// it, so the near rim stands between the eye and the middle instead of behind
/// it, and the piece is read through its own edge.
fn dome(given: f64) -> f64 {
    given.clamp(-1.0, 1.0)
}

/// How many arms the crest winds into, and which way round it winds.
///
/// Whole, and not as a matter of taste: the delay is read off an angle, and an
/// angle is a quantity that comes back to where it started. Anything else leaves
/// a step down the ray where the angle wraps — a crease running out of the
/// middle of the piece, in every frame.
///
/// None of them is the wave with no twist in it at all, which is rings growing
/// out of the middle rather than a spiral. Below none it winds the other way.
/// The ceiling is where the arms are packed so closely at the middle that what
/// is there is a knot rather than a figure.
fn arms(given: f64) -> f64 {
    given.round().clamp(-6.0, 6.0)
}

/// How many turns the line makes over a period.
///
/// Whole for the reason a period is a period: the line is wound once and turned
/// through the picture, so anything but a whole turn ends the loop with every
/// mark of it somewhere other than where it set off. None of them holds the line
/// still and leaves the swell the only thing moving under it, which is the piece
/// with its grain at rest — a picture rather than a spin. Below none it turns
/// against the wave.
fn spin(given: f64) -> f64 {
    given.round().clamp(-4.0, 4.0)
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

/// The line the marks stand on once a picture is laid on the disc.
///
/// The drift cannot draw a picture and was never asked to. It is a scatter, and
/// a scatter reproduces a picture the way a handful of sand reproduces a
/// stencil: most of it lands on the dark and is swept away, what is left holds
/// no edge, and the frame that arrives is a cloud in roughly the right shape. A
/// tree with thin bright branches came back as a haze, which is exactly the
/// complaint — a file was opened and nothing recognisable came of it.
///
/// So when there is something to draw, the marks stand on one line wound out
/// from the middle instead of scattered over the disc. Every one of them lands
/// somewhere the last one did not, the whole disc is covered once, and the
/// picture is drawn the way an engraving is drawn: by a single line that
/// thickens where the light is and thins to nothing where it is not. It is also
/// more of a spiral than the drift ever was, rather than less.
///
/// Settled once, like the drift, and for the same reason.
struct Winding {
    /// Where each mark stands, as how far out it is and which way round.
    marks: Vec<(f64, f64)>,
    /// The largest a mark is drawn, as a radius in frames.
    ///
    /// Half the gap between one winding and the next, so a mark standing in the
    /// full of the light closes on the windings either side of it and the line
    /// goes solid there, and one standing in half the light leaves paper
    /// showing. That is the whole of how a tone arrives.
    grain: f64,
}

impl Winding {
    fn new(windings: usize) -> Self {
        let rim = START + TRAVEL;
        let ring = rim / windings as f64;
        // Stepped along the line rather than round the middle: a turn near the
        // rim is a long way and a turn near the middle is no distance, and
        // marks laid at an even angle would be a crowd at the middle and a
        // dotted line at the edge. One ring apart is what leaves them square to
        // their neighbours — the same gap along the line as across it.
        let mut marks = Vec::new();
        let (mut out, mut angle) = (0.0, 0.0);
        while out < rim {
            marks.push((out, angle));
            // Which is an angle of the arc over how far out it is, except at
            // the very middle, where that is a division by nothing and the line
            // is a knot however it is walked.
            let step = ring / out.max(ring);
            angle += step;
            out += ring * step / TAU;
        }
        Self { marks, grain: ring / 2.0 }
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

/// What the plane is bent into, under the wave that runs over it.
///
/// The first two numbers are one thing said about two directions, so they are
/// carried together: how far the wave is put off for the distance out, and how
/// far for the way round. A delay that reads only the first is rings growing out
/// of the middle. One that reads both is a spiral, because a crest held back
/// further along the angle has to lean as it travels — and how many arms it
/// leans into is the second number.
///
/// The third is not part of that delay and is not a wave at all: it is the disc
/// itself lifted into a dome, with the wave still running over whatever shape it
/// has been left. Added rather than switched to, so none of it is the flat disc
/// the piece was composed on and every other setting means the same thing at any
/// amount of it. See [`dome`].
///
/// The fourth is the one that is not a shape. A delay and a dome are both things
/// that can be said in a sentence, and a surface with nothing on it but things
/// that can be said in a sentence is read as a diagram of itself. The last term
/// is a wandering field with no account of itself at all — see [`churn`] — and
/// what it is there for is to stop the other three from being the whole story.
#[derive(Clone, Copy)]
struct Wave {
    rings: f64,
    arms: f64,
    dome: f64,
    churn: f64,
    /// Where the wandering is read from. Carried here rather than worked out per
    /// call because a field is a table, and building one per point would cost
    /// more than reading the whole surface does.
    wander: Perlin,
}

impl Wave {
    /// The plane at `(x, y)` out from its middle, at this phase, swelling this
    /// much.
    ///
    /// The height is asked for rather than taken from [`SWELL`] because the
    /// plane stands lower under a picture — see [`SETTLED`] — and the marks and
    /// the plane they lie on have to agree about it. Told two different heights
    /// they would disagree by more than the marks ride above the plane, and half
    /// the piece would sink into a surface drawn in the paper's own colour and
    /// go out.
    fn at(&self, x: f64, y: f64, phase: f64, swell: f64) -> Point {
        let out = (x * x + y * y).sqrt();
        let delay = out * self.rings + self.arms * y.atan2(x) / TAU;
        // Taller further out, and by a root rather than in step, so the middle
        // is not a flat plate and the rim is not a wall.
        let height = swell * out.sqrt();
        Point {
            x: x * REACH,
            y: y * REACH,
            // Mostly under the plane and a little over it, which is what leaves
            // the swells reading as troughs with crests drawn between them.
            z: self.raised(out)
                + self.wandered(x, y, phase)
                + height * (4.0 * (TAU * (phase - delay)).sin() - 2.0),
        }
    }

    /// How far the surface has wandered off the wave at this point, in frames.
    ///
    /// Two scales of it, coarse and half as much of one twice as fine. Two and
    /// not one: a single scale of this noise is lobes of about one size, evenly
    /// spread, and a surface made of evenly spread lobes has traded one regular
    /// thing for another. Two and not three: a third would come to five quads a
    /// lobe where the piece is composed and to under two wherever the surface is
    /// cut loosely, and a wander the surface carrying it cannot hold is drawn as
    /// a shudder rather than as a wander.
    ///
    /// Read round a circle rather than along a line, which is the whole reason
    /// this can be here at all — see [`circle_noise`]. Noise walked in a
    /// straight line never comes back, and a piece that does not come back has
    /// no loop. Walked round a circle it returns exactly, for the price of two
    /// more coordinates.
    fn wandered(&self, x: f64, y: f64, phase: f64) -> f64 {
        if self.churn == 0.0 {
            return 0.0;
        }
        let coarse = circle_noise(&self.wander, x * SCALE, y * SCALE, phase, DRIFTING);
        // Offset, so the fine scale is a different part of the same field
        // rather than the coarse one again at another size.
        let fine = circle_noise(
            &self.wander,
            x * 2.0 * SCALE + 31.0,
            y * 2.0 * SCALE,
            phase,
            2.0 * DRIFTING,
        );
        self.churn * (coarse + 0.5 * fine) / 1.5
    }

    /// How far the disc itself stands above its own rim at this distance out.
    ///
    /// A hemisphere, which is the shape and not one of several that would do.
    /// What is wanted from it is what a crowd spread evenly over a disc does when
    /// that disc is lifted and then looked at from the side: every particle the
    /// same distance out lands at the same height, wherever round it stands, so
    /// they stack across the frame instead of spreading over it — and they stack
    /// most tightly where the shape is steepest. A hemisphere is steepest at its
    /// rim and level at its top, so what arrives is a bright edge around a
    /// thinning middle. Round it off and the edge goes with it.
    ///
    /// Nothing outside the rim, where the root has no answer. The mesh is cut
    /// square and reaches into the corners, which the disc never does.
    fn raised(&self, out: f64) -> f64 {
        let rim = START + TRAVEL;
        self.dome * (rim * rim - out * out).max(0.0).sqrt()
    }
}

/// A picture for the line to draw, laid flat on the disc.
///
/// Read where a mark stands rather than where the line set off, so the picture
/// holds still while the line turns through it. Read the other way it would be
/// smeared round the middle — a picture being dragged apart rather than one being
/// shown.
///
/// Settled once at the size it will be read at, tones and all. What a frame then
/// costs is an index and a comparison a mark, whatever was opened.
struct Subject {
    /// How much light stands at each pixel, and what colour to draw it in — the
    /// picture's own where that was asked for, and the line's ink where it was
    /// not, so a mark never has to ask which.
    light: Vec<f32>,
    tint: Vec<[f32; 3]>,
    wide: usize,
    tall: usize,
    /// How far the picture reaches over the plane, across and away.
    half: (f64, f64),
    /// Below this the light is the picture's paper — see [`FAINT`].
    floor: f32,
}

/// What is asked of a picture as it goes down on the disc.
///
/// One value rather than a handful of arguments because they are one decision:
/// every one of them is settled off the same line, none of them means anything
/// without the rest, and a subject read under half of them is not a subject read.
#[derive(Clone, Copy)]
struct Laying {
    /// How far over the disc the picture is laid.
    spread: f64,
    /// How a picture that is not square meets a disc that is.
    fit: Fit,
    /// How far it is stretched to its own two ends before it is read.
    open: f64,
    /// How faint its light may get before it counts as paper.
    floor: f64,
    tones: Tones,
    colored: bool,
    /// What a mark is drawn in where the picture's own colours are not wanted.
    ink: [f32; 3],
}

impl Subject {
    fn new(picture: &RgbaImage, laying: Laying) -> Self {
        let Laying { spread, fit, open, floor, tones, colored, ink } = laying;
        let (wide, tall) = (picture.width().max(1), picture.height().max(1));
        // In proportion, and only ever smaller: a picture already coarser than
        // this is at the size the line can show, and blowing it up would spread
        // its own pixels out into squares nothing is asking for.
        let scale = (SAMPLE as f64 / wide.max(tall) as f64).min(1.0);
        let read = |side: u32| ((side as f64 * scale).round() as u32).max(1);
        let (wide, tall) = (read(wide), read(tall));
        let small = resize(picture, wide, tall, FilterType::Triangle);

        // The picture's own light, before either end of it has been moved, so
        // there is a range to measure at all.
        let plain: Vec<f32> = small.pixels().map(|pixel| plain_light(&pixel.0)).collect();
        let darkest = plain.iter().copied().fold(f32::INFINITY, f32::min);
        let brightest = plain.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        // Opened to those ends, and only then handed to the tones. The other way
        // round, contrast has already pushed a low-key picture off the bottom —
        // it turns about the middle of the range, and a picture with nothing
        // near the middle is one it can only make darker.
        let range = brightest - darkest;
        // A picture all one tone has no two ends to be opened out to, and the
        // range is what would be divided by.
        let spanned = range > FLAT;
        let open = if spanned { open as f32 } else { 0.0 };
        let opened: Vec<f32> = plain
            .iter()
            .map(|&plain| match open > 0.0 {
                true => plain + ((plain - darkest) / range - plain) * open,
                false => plain,
            })
            .collect();

        // Which end of it the subject is on, found rather than assumed — see
        // [`MOSTLY`]. A picture of one tone is left alone for the same reason it
        // is left unopened: it has one end, and neither of the two things that
        // could be said about it is true. `--invert` still has the last word,
        // since it swaps the ends of whatever was found here.
        let mean = opened.iter().sum::<f32>() / opened.len().max(1) as f32;
        let turned = spanned && mean > MOSTLY;

        let light = small
            .pixels()
            .zip(&opened)
            .map(|(pixel, &level)| {
                let level = if turned { 1.0 - level } else { level };
                tones.level(level) * pixel.0[3] as f32 / 255.0
            })
            .collect();
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

        // The disc the line is wound over is what the picture is laid over, since
        // the line is what has to draw it: any wider and the corners of it are
        // out past where the winding reaches.
        let disc = (START + TRAVEL) * spread;
        // Which side of the picture is made to span the disc. The long one puts
        // the whole picture inside it and leaves the near and far ends of the
        // disc to nobody — a wide photograph on a round disc reaches a band
        // across the middle and no further. The short one fills the disc and
        // walks the ends of the picture out past the rim, where they are simply
        // never stood on: a crop that costs nothing to make, because nothing off
        // the disc was ever going to be read.
        let side = match fit {
            Fit::Contain => wide.max(tall),
            Fit::Cover => wide.min(tall),
        } as f64;
        Self {
            light,
            tint,
            wide: wide as usize,
            tall: tall as usize,
            half: (disc * wide as f64 / side, disc * tall as f64 / side),
            floor: floor as f32,
        }
    }

    /// What the picture shows at this point of the plane: what a particle
    /// standing there is drawn in, and how much light it is standing in.
    /// Nothing off the picture, and nothing on its paper.
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
        (light > self.floor).then(|| (self.tint[at], light as f64))
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
    /// The picture the marks are drawing, and the line they stand on to draw it.
    ///
    /// Both or neither: the line is only ever wound to draw something, and a
    /// picture with no line under it is one nothing would show. Neither, and the
    /// disc is the drift it was composed as.
    subject: Option<Subject>,
    winding: Option<Winding>,
    /// How closely that line is wound, kept so a picture laid after the fact has
    /// something to be drawn at.
    windings: usize,
    /// How tall the wave stands, which a picture settles — see [`SETTLED`].
    swell: f64,
    /// What shape it stands in.
    wave: Wave,
    /// How many turns the line makes over a period. Nothing to the drift, which
    /// has no turn of its own — its motion is the walk out and the swell under
    /// it, the same way the crowd's own settings are nothing to a laid picture.
    spin: f64,
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
    /// Lay a picture on the disc, and wind the line that is going to draw it.
    ///
    /// One call rather than two fields set in a row, because a subject without
    /// its winding is a picture nothing draws and a winding without its subject
    /// is a coil drawn over nothing. Neither is a state the piece has.
    fn lay(&mut self, subject: Subject) {
        self.winding = Some(Winding::new(self.windings));
        self.subject = Some(subject);
        self.swell *= SETTLED;
        // The wandering settles with the swell, and for the reason the swell
        // does: it is another slope for the lens to drag the picture over, and
        // it is the more damaging of the two because it has no shape a viewer
        // can allow for. A wave pulls a drawing into rings. Noise pulls it into
        // nothing that can be named.
        self.wave.churn *= SETTLED;
    }

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
            .map(|at| {
                let point = self.wave.at(out(at % side), out(at / side), phase, self.swell);
                self.view.sees(point)
            })
            .collect();

        for across in 0..self.mesh {
            for down in 0..self.mesh {
                let at = down * side + across;
                let patch = [grid[at], grid[at + 1], grid[at + side + 1], grid[at + side]];
                raster.triangle([patch[0], patch[1], patch[2]], self.paper);
                raster.triangle([patch[0], patch[2], patch[3]], self.paper);
            }
        }

        match (&self.subject, &self.winding) {
            (Some(subject), Some(winding)) => self.draw(&mut raster, subject, winding, phase),
            _ => self.drift(&mut raster, phase),
        }

        raster.into_image()
    }

    /// The picture, drawn by the line wound out through it.
    ///
    /// The line turns over the period while the picture holds still under it, so
    /// what moves is the grain and not the subject — and whole turns are the one
    /// amount that leaves the line where it found it. See [`spin`].
    fn draw(&self, raster: &mut Raster, subject: &Subject, winding: &Winding, phase: f64) {
        let spun = TAU * self.spin * phase;
        for &(out, angle) in &winding.marks {
            let angle = angle + spun;
            let (x, y) = (out * angle.cos(), out * angle.sin());
            // Where the picture is paper the line is paper: a mark standing off
            // it, or on the dark of it, is not drawn at all. That is what leaves
            // a subject reading as a subject rather than as a coil with a
            // shading on it.
            let Some((tint, light)) = subject.at(x, y) else {
                continue;
            };
            // The light is taken as the area of the mark rather than as how
            // strongly it is drawn, which is how every halftone ever printed
            // reads a tone. Drawn faintly instead, a picture that is lit nearly
            // everywhere — which is every photograph — leaves every mark
            // standing and merely greys them, and a greyed line is the line.
            let mut point = self.wave.at(x, y, phase, self.swell);
            point.z += RIDE;
            raster.dot(self.view.sees(point), winding.grain * light.sqrt(), tint, 1.0);
        }
    }

    /// The disc with nothing laid on it: the drift, as it was composed.
    fn drift(&self, raster: &mut Raster, phase: f64) {
        for particle in &self.particles {
            let walked = (phase + particle.offset).rem_euclid(1.0);
            for copy in 0..COPIES {
                let along = (copy as f64 + walked) / COPIES as f64;
                let ((x, y), size) = particle.at(along);
                let mut point = self.wave.at(x, y, phase, self.swell);
                point.z += RIDE;
                raster.dot(self.view.sees(point), size, self.ink, 1.0);
            }
        }
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
    // All of the picture unless the disc being full matters more, which is the
    // same fallback the flat read makes.
    let fit = Fit::from_params(params, Fit::Contain)?;
    // Opened to the picture's own ends unless a line asks for the light as it
    // stands. A whole is the default because the crowd draws light as the size
    // of a mark, and a picture that never gets bright has no marks — see
    // [`opened`].
    let laying = Laying {
        spread: over,
        fit,
        open: opened(params.f64("open", 1.0)?),
        floor: floor(params.f64("floor", FAINT)?),
        tones: Tones::from_params(params)?,
        colored: params.is_set("color"),
        ink,
    };

    // Anything the app can open is a subject here, and none is a subject too:
    // without one the drift is drawn in its own ink, as it always was.
    let laid = |picture: &RgbaImage| Subject::new(picture, laying);
    let written = |text: &str| laid(&raster_of(&AsciiCanvas::from_text(text)));
    // On a command line, no subject is a line with no file on it. The window has
    // no such line — it carries one file between all of its tools and hands it to
    // whichever is showing — so `--bare` is how it says the same thing.
    let subject = if params.is_set("bare") {
        None
    } else {
        match params.string("text") {
            // `--text` carries a drawing inline, which is how the window offers
            // a sample without a file whose path differs between dev and a
            // bundle.
            Some(inline) => Some(written(inline)),
            None => match params.first_positional() {
                Some(path) => Some(match &*open(path)? {
                    Source::Drawing(text) => written(text),
                    Source::Picture(picture) => laid(picture),
                }),
                None => None,
            },
        }
    };

    let mut spiral = Spiral {
        particles: (0..count(params.usize("count", 17_000)?))
            .map(|index| Particle::new(seed, index))
            .collect(),
        subject: None,
        winding: None,
        windings: windings(params.usize("windings", 110)?),
        swell: swell(params.f64("swell", SWELL)?),
        wave: Wave {
            rings: rings(params.f64("rings", RINGS)?),
            arms: arms(params.f64("arms", ARMS)?),
            dome: dome(params.f64("dome", DOME)?),
            churn: churn(params.f64("churn", CHURN)?),
            // The same seed the crowd is settled from, so one number is which
            // draw of the piece this is — where the particles fell and which
            // way the surface wanders under them, rather than one of the two.
            wander: Perlin::new(seed as u32),
        },
        spin: spin(params.f64("spin", SPIN)?),
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
    };
    if let Some(subject) = subject {
        spiral.lay(subject);
    }
    Ok(spiral)
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
        // And a line wound loosely enough to be drawn at this size. These frames
        // are a tenth of the side a real one is, so the wind a piece is composed
        // at would put every mark of it inside a pixel and the whole line under
        // the counts below.
        params.flags.insert("windings".into(), Some("12".into()));
        for (flag, value) in flags {
            params.flags.insert((*flag).into(), Some((*value).into()));
        }
        assemble(&params).expect("the tool builds")
    }

    /// The wave with nothing wandering over it.
    ///
    /// What the tests that ask what a delay does are asking about. A wandering
    /// field has no answer to give about arms or rings and would be sitting in
    /// the middle of every measurement of them, so it is set aside where the
    /// question is about the other three. It has tests of its own.
    fn plain(rings: f64, arms: f64, dome: f64) -> Wave {
        Wave { rings, arms, dome, churn: 0.0, wander: Perlin::new(7) }
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

    /// The delay reads an angle, and an angle wraps. An arm count that was not
    /// whole would leave a step down the ray where it wraps, so the two sides of
    /// that ray have to agree — at every count the tool will take, not only at
    /// the one it opens on. See [`arms`].
    #[test]
    fn the_wave_has_no_step_where_the_angle_comes_round() {
        for asked in [-6.0, -1.0, 0.0, ARMS, 3.0, 6.0, 2.4] {
            let wave = plain(RINGS, arms(asked), DOME);
            for phase in [0.0, 0.31, 0.68] {
                for out in [0.1, 0.4, 0.8] {
                    let above = wave.at(-out, 1e-9, phase, SWELL).z;
                    let below = wave.at(-out, -1e-9, phase, SWELL).z;
                    assert!((above - below).abs() < 1e-6, "{above} against {below} on {asked}");
                }
            }
        }
    }

    /// Every shape the tool will take has to come back round as cleanly as the
    /// one it opens on. That is the whole reason the two counts read off an
    /// angle are held to whole numbers rather than passed on as they arrive.
    #[test]
    fn every_shape_the_tool_offers_closes_its_own_loop() {
        for shape in [
            &[("arms", "0")][..],
            &[("arms", "-3")],
            &[("arms", "6"), ("rings", "0")],
            &[("rings", "20")],
            &[("spin", "0"), ("text", BLOCK)],
            &[("spin", "-2"), ("text", BLOCK)],
            &[("dome", "1")],
            &[("dome", "-1"), ("rings", "0")],
            &[("churn", "0")],
            &[("churn", "0.2")],
            &[("arms", "4"), ("rings", "3"), ("spin", "3"), ("text", BLOCK)],
            &[("dome", "0.7"), ("swell", "0.2"), ("spin", "2"), ("text", BLOCK)],
            &[("dome", "0.6"), ("churn", "0.1"), ("arms", "2"), ("text", BLOCK)],
        ] {
            let spiral = made(shape);
            let start = spiral.picture(WIDE, TALL, 0.0);
            let round = spiral.picture(WIDE, TALL, 1.0);
            assert_eq!(apart(&start, &round), 0, "{shape:?} did not come back to itself");
        }
    }

    /// A count read off an angle is taken to the nearest whole one rather than
    /// refused: a slider hands over whatever it is dragged to, and a typed line
    /// asking for two and a half arms is asking for a number of arms.
    #[test]
    fn what_is_read_off_an_angle_is_held_to_whole_numbers() {
        assert_eq!(arms(2.4), 2.0);
        assert_eq!(arms(-2.6), -3.0);
        assert_eq!(arms(99.0), 6.0);
        assert_eq!(spin(0.4), 0.0);
        assert_eq!(spin(-99.0), -4.0);
        assert_eq!(rings(-1.0), 0.0);
        assert_eq!(rings(99.0), 20.0);
    }

    /// No arms is the delay with no angle left in it, so every point the same
    /// distance out stands at the same height — rings growing out of the middle,
    /// which is the one thing a spiral never does.
    #[test]
    fn a_wave_with_no_arms_stands_level_all_the_way_round() {
        let height = |wave: &Wave, out: f64, angle: f64| {
            wave.at(out * angle.cos(), out * angle.sin(), 0.21, SWELL).z
        };
        let ringed = plain(RINGS, 0.0, DOME);
        let armed = plain(RINGS, ARMS, DOME);
        for out in [0.12, 0.3, 0.48] {
            let round = |wave: &Wave| -> Vec<f64> {
                (1..8)
                    .map(|step| height(wave, out, TAU * step as f64 / 8.0) - height(wave, out, 0.0))
                    .collect()
            };
            let level = round(&ringed);
            let leaning = round(&armed);
            assert!(level.iter().all(|step| step.abs() < 1e-9), "the rings leaned at {out}");
            assert!(leaning.iter().any(|step| step.abs() > 1e-6), "the arm stood level at {out}");
        }
    }

    /// The other delay is how many times the wave repeats on the way out, so
    /// asking for more of them puts more crests along the same ray.
    #[test]
    fn the_wave_repeats_as_often_as_it_is_asked_to() {
        let crests = |asked: f64| {
            let wave = plain(rings(asked), 0.0, DOME);
            let along: Vec<f64> = (1..96)
                .map(|step| {
                    let out = step as f64 / 192.0;
                    // Back to the plain sine the height is read through, with
                    // the lift off the middle and the taper toward it taken out.
                    wave.at(out, 0.0, 0.0, SWELL).z / (SWELL * out.sqrt()) + 2.0
                })
                .collect();
            along.windows(2).filter(|pair| pair[0] * pair[1] < 0.0).count()
        };
        assert!(crests(4.0) > 0, "a wave that repeats four times had no crest in it");
        assert!(crests(16.0) > crests(4.0), "{} against {}", crests(16.0), crests(4.0));
    }

    /// A line told not to turn is the piece with its grain at rest, and one told
    /// to turn four times stands exactly where that line does wherever those
    /// turns have come round — which is what says the number is turns of the
    /// line and not some speed it is being dragged at.
    ///
    /// Counted in frames rather than in pixels moved: a spiral turned through an
    /// angle is another spiral, near enough that comparing two of them counts
    /// the difference between the windings rather than the turn.
    #[test]
    fn a_line_makes_whole_turns_or_stands_where_one_at_rest_would() {
        let resting = made(&[("text", BLOCK), ("spin", "0")]);
        let turning = made(&[("text", BLOCK), ("spin", "4")]);
        let side_by_side = |phase: f64| {
            apart(&resting.picture(WIDE, TALL, phase), &turning.picture(WIDE, TALL, phase))
        };
        // A quarter of the way through, four turns have made exactly one, so the
        // marks are back over the part of the picture they set off from — and
        // the wave under both is at the same phase, so nothing else can differ.
        assert_eq!(side_by_side(0.25), 0, "a whole turn landed somewhere else");
        // An eighth of the way through they have made half of one, and have not.
        assert!(side_by_side(0.125) > 0, "half a turn landed nowhere at all");
    }

    /// A seed is a promise that the same line can be typed twice, and that
    /// another one is worth typing.
    ///
    /// One number for two things: where the crowd stands, and which way the
    /// surface wanders under it. A picture is drawn by the line rather than by
    /// the crowd, so with a file open the wandering is the whole of what the
    /// promise comes to — and the seed has to reach it or the control is a
    /// control over nothing.
    #[test]
    fn the_same_seed_draws_the_same_frame() {
        let one = made(&[("seed", "7")]).picture(WIDE, TALL, 0.4);
        let same = made(&[("seed", "7")]).picture(WIDE, TALL, 0.4);
        assert_eq!(apart(&one, &same), 0);

        let other = made(&[("seed", "8")]).picture(WIDE, TALL, 0.4);
        assert!(apart(&one, &other) > 0, "the seed changed nothing");

        let laid = |seed: &str| made(&[("text", BLOCK), ("seed", seed)]).picture(WIDE, TALL, 0.4);
        assert_eq!(apart(&laid("7"), &laid("7")), 0);
        assert!(apart(&laid("7"), &laid("8")) > 0, "the seed reached nothing under a picture");
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
    fn every_setting_stays_inside_what_is_worth_drawing() {
        assert_eq!(count(0), 200);
        assert_eq!(count(17_000), 17_000);
        assert_eq!(count(1_000_000), 60_000);
        assert_eq!(mesh(0), 16);
        assert_eq!(mesh(130), 130);
        assert_eq!(mesh(9_999), 400);
        assert_eq!(spread(0.0), 0.1);
        assert_eq!(spread(1.0), 1.0);
        assert_eq!(spread(90.0), 4.0);
        assert_eq!(windings(0), 8);
        assert_eq!(windings(110), 110);
        assert_eq!(windings(9_999), 200);
    }

    /// The whole difference between this and the drift it replaced: it is one
    /// line. Every mark stands within a winding's spacing of the one before it,
    /// out from the middle to the rim, so what draws the picture is a stroke
    /// rather than a scatter that happens to have been sorted.
    #[test]
    fn every_mark_stands_a_winding_from_the_one_before_it() {
        let turns = 40;
        let ring = (START + TRAVEL) / turns as f64;
        let winding = Winding::new(turns);
        let at = |(out, angle): (f64, f64)| (out * f64::cos(angle), out * f64::sin(angle));

        let mut last = at(winding.marks[0]);
        for &mark in &winding.marks[1..] {
            let (x, y) = at(mark);
            let step = ((x - last.0).powi(2) + (y - last.1).powi(2)).sqrt();
            assert!(step < 1.5 * ring, "a mark stood {step} from the last, a ring being {ring}");
            last = (x, y);
        }
    }

    /// And it covers the disc: it starts at the middle and it stops at the rim,
    /// which is where the plane stops and where a picture is laid out to.
    #[test]
    fn the_line_runs_from_the_middle_to_the_rim() {
        let rim = START + TRAVEL;
        let winding = Winding::new(40);
        let (first, last) = (winding.marks[0].0, winding.marks[winding.marks.len() - 1].0);
        assert!(first < rim / 40.0, "the line set off at {first} rather than at the middle");
        assert!(last > rim - rim / 40.0, "the line stopped at {last} short of {rim}");
    }

    /// A mark is drawn at half the gap between one winding and the next, so the
    /// full of the light closes on the windings either side of it and anything
    /// less leaves paper showing. Wind the line tighter and every mark of it is
    /// finer, which is the whole of what the setting does.
    #[test]
    fn a_tighter_wind_draws_a_finer_mark() {
        let loose = Winding::new(20);
        let tight = Winding::new(40);
        assert!((loose.grain - (START + TRAVEL) / 40.0).abs() < 1e-12);
        assert!(tight.grain < loose.grain);
        assert!(tight.marks.len() > loose.marks.len() * 3);
    }

    const LIT: [u8; 4] = [255, 255, 255, 255];
    const UNLIT: [u8; 4] = [0, 0, 0, 255];

    fn painted(shade: impl Fn(u32, u32) -> [u8; 4]) -> RgbaImage {
        RgbaImage::from_fn(32, 32, |x, y| Rgba(shade(x, y)))
    }

    /// The disc with a picture on it, seen square on so the picture is where the
    /// plane says it is rather than where the camera has swung it to.
    fn carrying(picture: &RgbaImage, colored: bool) -> Spiral {
        laid(picture, colored, Fit::Contain)
    }

    fn laid(picture: &RgbaImage, colored: bool, fit: Fit) -> Spiral {
        let mut spiral = made(&[("yaw", "0"), ("pitch", "0")]);
        let laying = Laying { fit, colored, ..plainly(&spiral) };
        spiral.lay(Subject::new(picture, laying));
        spiral
    }

    /// A whole picture over the whole disc with nothing asked of its tones,
    /// which is what the tests below move one setting off at a time.
    fn plainly(spiral: &Spiral) -> Laying {
        Laying {
            spread: 1.0,
            fit: Fit::Contain,
            open: 1.0,
            floor: FAINT,
            tones: Tones::PLAIN,
            colored: false,
            ink: spiral.ink,
        }
    }

    /// What the line is for once there is a picture: it is the picture. A mark
    /// over paper draws nothing, so paper all through leaves the frame with the
    /// plane on it and nothing else.
    #[test]
    fn a_subject_of_paper_leaves_the_line_nothing_to_show() {
        let dark = carrying(&painted(|_, _| UNLIT), false).picture(WIDE, TALL, 0.2);
        assert_eq!(drawn(&dark), 0);

        let light = carrying(&painted(|_, _| LIT), false).picture(WIDE, TALL, 0.2);
        assert!(drawn(&light) > 100, "only {} pixels were lit", drawn(&light));
    }

    /// And how it shows anything between the two: with the size of the mark,
    /// the way a halftone does. Drawing the mark faintly instead would leave
    /// the whole line standing over a picture that is lit nearly everywhere —
    /// which is every photograph — and a greyed line is the line.
    #[test]
    fn half_the_light_is_half_the_mark_rather_than_half_the_ink() {
        let full = drawn(&carrying(&painted(|_, _| LIT), false).picture(WIDE, TALL, 0.2));
        let half = drawn(&carrying(&painted(|_, _| [128, 128, 128, 255]), false).picture(WIDE, TALL, 0.2));
        assert!(half > 0, "the line went out altogether");
        assert!(half * 4 < full * 3, "{half} lit against {full} in the full of the light");
    }

    /// Where the picture stops being a subject and starts being its own paper.
    /// Which is a judgement about the picture rather than about the piece, so it
    /// moves: an even half-light is a drawing under a low cut and nothing at all
    /// under a high one.
    #[test]
    fn the_cut_says_how_faint_the_light_may_get_before_it_is_paper() {
        let half = painted(|_, _| [128, 128, 128, 255]);
        let under = |floor| {
            let mut spiral = made(&[("yaw", "0"), ("pitch", "0")]);
            let laying = Laying { floor, ..plainly(&spiral) };
            spiral.lay(Subject::new(&half, laying));
            drawn(&spiral.picture(WIDE, TALL, 0.2))
        };

        assert!(under(0.2) > 0, "the light was taken for paper under a cut it stands over");
        assert_eq!(under(0.7), 0, "a mark stood on light the cut had taken for paper");
    }

    /// A picture whose light never rises far — pale text on a dark field, which
    /// is the commonest thing anybody screenshots — read against its own two
    /// ends rather than against the whole of a range it never reaches.
    ///
    /// Light is carried as the size of a mark, so this is not a matter of a
    /// picture arriving dim. Read as it stands there is nothing here at all:
    /// every pixel of the subject is under the paper cut, and the frame comes
    /// back bare, with no sign a file was ever opened.
    #[test]
    fn a_picture_that_never_gets_bright_is_opened_out_to_its_own_ends() {
        let dim = painted(|across, _| if across < 16 { [8, 8, 8, 255] } else { UNLIT });
        let opened_to = |open| {
            let mut spiral = made(&[("yaw", "0"), ("pitch", "0")]);
            let laying = Laying { open, ..plainly(&spiral) };
            spiral.lay(Subject::new(&dim, laying));
            drawn(&spiral.picture(WIDE, TALL, 0.2))
        };

        assert_eq!(opened_to(0.0), 0, "the picture was read as if it had the range it has not");
        assert!(opened_to(1.0) > 100, "only {} pixels were lit", opened_to(1.0));
    }

    /// A stroke of ink on a page of white — a signature, a logo, a diagram, and
    /// most of what anybody has lying about as a file.
    ///
    /// Read with the bright end for the subject, the line runs solid over all
    /// that paper and thins along the one stroke, which is a frame nobody can
    /// tell from a coil: the complaint such a picture draws is not that it came
    /// out wrong but that nothing happened. So the stroke is what is drawn, and
    /// the page is what the line crosses without marking.
    #[test]
    fn a_picture_that_is_mostly_paper_is_read_from_its_ink() {
        // Off to one side, so which end of the picture was taken for the subject
        // is a question the frame answers by where its marks are. A count could
        // not: the page and the stroke both leave fewer marks standing than a
        // full disc of light, and only one of them leaves them on the stroke.
        let page = painted(|x, _| if (4..12).contains(&x) { UNLIT } else { LIT });
        let frame = carrying(&page, false).picture(WIDE, TALL, 0.2);

        let lit: Vec<f64> = frame
            .enumerate_pixels()
            .filter(|(_, _, pixel)| pixel.0[0] > 24)
            .map(|(x, _, _)| x as f64)
            .collect();
        assert!(!lit.is_empty(), "the picture was taken for paper all through");
        let middle = lit.iter().sum::<f64>() / lit.len() as f64;
        assert!(
            middle < WIDE as f64 / 2.0,
            "the marks stood on the page at {middle} rather than on the ink"
        );
    }

    /// A picture all one tone has no two ends to be opened out to, and opening
    /// it anyway would make its own noise the subject.
    #[test]
    fn a_picture_of_one_tone_is_left_where_it_stands() {
        let flat = |shade| painted(move |_, _| [shade, shade, shade, 255]);
        let over = |picture: &RgbaImage| drawn(&carrying(picture, false).picture(WIDE, TALL, 0.2));

        assert_eq!(over(&flat(8)), 0, "a picture of paper was opened into a subject");
        assert!(over(&flat(255)) > 100, "a picture of light was lost");
    }

    #[test]
    fn a_cut_that_leaves_nothing_but_specks_is_refused() {
        assert_eq!(floor(-1.0), 0.0);
        assert_eq!(floor(0.04), 0.04);
        assert_eq!(floor(4.0), 0.9);
    }

    /// A picture that is not square meeting a disc that is. Contained, the whole
    /// of it stands inside the disc and a wide one reaches a band across the
    /// middle with the near and far ends left to nobody. Covering, the disc is
    /// full and the ends of the picture are what goes.
    #[test]
    fn a_wide_picture_stands_inside_the_disc_or_is_laid_across_it() {
        let wide = RgbaImage::from_fn(32, 8, |_, _| Rgba(LIT));
        let over = |fit| drawn(&laid(&wide, false, fit).picture(WIDE, TALL, 0.2));

        let (inside, across) = (over(Fit::Contain), over(Fit::Cover));
        assert!(inside > 0, "the picture was lost altogether");
        assert!(across > inside * 2, "{across} lit against {inside} stood inside the disc");
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

    /// Asked for the picture's colours, the marks take them. Left alone they
    /// keep the piece's own ink, whatever the picture was painted in.
    #[test]
    fn the_marks_take_their_colour_from_the_picture_when_they_are_asked_to() {
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

    /// The picture holds still rather than being pulled along: it is read where
    /// a mark stands, and the line turns one whole revolution over the period,
    /// so the loop it closes is the one the rest of the piece closes.
    #[test]
    fn a_period_leaves_a_carried_picture_where_it_found_it() {
        let checks = painted(|x, y| if (x / 4 + y / 4) % 2 == 0 { LIT } else { UNLIT });
        let spiral = carrying(&checks, false);
        let start = spiral.picture(WIDE, TALL, 0.0);
        let round = spiral.picture(WIDE, TALL, 1.0);
        assert_eq!(apart(&start, &round), 0);
    }

    /// The drift on its own, with a subject open and to hand. The window always
    /// has one, so without this there would be no way from in there to ask for
    /// the piece as it was composed.
    #[test]
    fn the_drift_can_be_asked_for_on_its_own() {
        let carried = made(&[("text", BLOCK)]).picture(WIDE, TALL, 0.2);
        let bare = made(&[("text", BLOCK), ("bare", "")]).picture(WIDE, TALL, 0.2);
        assert_eq!(apart(&bare, &made(&[]).picture(WIDE, TALL, 0.2)), 0);
        assert!(apart(&bare, &carried) > 0, "the drawing was never laid down");
    }

    /// A drawing arrives the same way a picture does, and the spread is how much
    /// of the disc it is laid over.
    #[test]
    fn a_tighter_spread_lays_the_subject_over_less_of_the_disc() {
        let whole = drawn(&made(&[("text", BLOCK)]).picture(WIDE, TALL, 0.2));
        let tight = drawn(&made(&[("text", BLOCK), ("spread", "0.4")]).picture(WIDE, TALL, 0.2));
        assert!(tight > 0, "the drawing was lost altogether");
        assert!(tight < whole, "{tight} lit against {whole} laid over the whole disc");
    }

    /// A swell at its full height is a lens dragging whatever stands on it out
    /// and hauling it back, eight times over between the middle and the rim. A
    /// scatter has no shape to lose to that and a drawing has, so the disc
    /// settles under one — see [`SETTLED`].
    #[test]
    fn a_picture_stands_on_a_disc_that_swells_less_than_the_bare_one() {
        let swing = |spiral: &Spiral| {
            let over: Vec<f64> = (0..64)
                .map(|step| spiral.wave.at(0.3, 0.0, step as f64 / 64.0, spiral.swell).z)
                .collect();
            over.iter().cloned().fold(f64::MIN, f64::max) - over.iter().cloned().fold(f64::MAX, f64::min)
        };
        let bare = swing(&made(&[]));
        let under = swing(&made(&[("text", BLOCK)]));
        assert!(under > 0.0, "the wave went flat under a picture rather than settling");
        assert!(under * 2.0 < bare, "{under} against {bare} with nothing laid on the disc");
    }

    /// A lifted disc stands highest in the middle and meets its own rim, and
    /// past the rim it is nothing at all — the mesh is cut square and reaches
    /// into corners the disc never had. What is between is a half-sphere, which
    /// is the shape and not one that would merely do: it is the one that is
    /// steepest exactly where the crowd is meant to stack.
    #[test]
    fn a_lifted_disc_is_a_half_sphere_out_to_its_rim_and_nothing_past_it() {
        let rim = START + TRAVEL;
        let raised = |dome: f64, out: f64| plain(0.0, 0.0, dome).raised(out);

        assert_eq!(raised(1.0, 0.0), rim, "the middle of a half-sphere stands a radius up");
        assert_eq!(raised(1.0, rim), 0.0, "the rim of the disc came away from the plane");
        assert_eq!(raised(1.0, 0.9), 0.0, "the corners of the mesh were lifted too");
        assert_eq!(raised(0.0, 0.2), 0.0, "a disc asked for no dome was bent anyway");
        // A bowl is the same shape looked into, so it holds at every distance.
        for out in [0.0, 0.1, 0.3, rim] {
            assert_eq!(raised(-1.0, out), -raised(1.0, out), "the bowl was not the dome at {out}");
        }
        // Steepest at the rim and level at the top, which is what stacks the
        // crowd against the edge. Measured as how far the shape falls over the
        // same step out, taken near the middle and near the rim.
        let step = 0.02;
        let flat = raised(1.0, 0.0) - raised(1.0, step);
        let sheer = raised(1.0, rim - step) - raised(1.0, rim);
        assert!(sheer > flat * 10.0, "{sheer} at the rim against {flat} at the middle");
    }

    /// A wave says the same thing all the way round a ring, and the wandering
    /// says a different thing at every point on it — which is the whole of why it
    /// is there. What a ring of one height draws is a circle, and a run of them a
    /// diagram; what a ring that varies draws is a crest that goes somewhere.
    #[test]
    fn the_wandering_is_what_stops_a_ring_standing_at_one_height() {
        // Two points the same distance out, and no arms, so the wave alone has
        // them at exactly the same height by construction.
        let out = 0.3;
        let (here, there) = ((out, 0.0), (out * 0.6, -out * 0.8));
        let apart = |churn: &str| {
            let spiral = made(&[("churn", churn), ("arms", "0")]);
            let at = |(x, y): (f64, f64)| spiral.wave.at(x, y, 0.3, spiral.swell).z;
            // Taken against how tall the wave itself stands there, so the answer
            // is a share of the wave rather than a length.
            (at(here) - at(there)).abs() / (spiral.swell * out.sqrt())
        };
        assert!(apart("0") < 1e-12, "the wave alone broke its own ring: {}", apart("0"));
        assert!(apart("0.05") > 0.1, "the ring stood at one height throughout: {}", apart("0.05"));
        // And a fair share of the wave rather than a wash over it, at the amount
        // the piece is composed at.
        let composed = apart("0.035");
        assert!(composed > 0.02 && composed < 2.0, "{composed} of the wave's own height");
    }

    /// Whatever the field is doing, it has to be doing it again by the end — the
    /// loop is closed by reading the noise round a circle, and the circle is the
    /// only reason a wandering field can be in a piece that repeats at all.
    ///
    /// [`every_shape_the_tool_offers_closes_its_own_loop`] draws this and every
    /// other setting as whole frames. This says the same thing about the surface
    /// alone, so a failure names the wandering rather than the render.
    #[test]
    fn the_wandering_comes_back_to_where_it_set_off() {
        let spiral = made(&[("churn", "0.2")]);
        for (x, y) in [(0.0, 0.0), (0.31, -0.12), (-0.4, 0.4), (0.05, 0.49)] {
            let start = spiral.wave.wandered(x, y, 0.0);
            assert!(
                (start - spiral.wave.wandered(x, y, 1.0)).abs() < 1e-12,
                "the field was somewhere else at the seam, at ({x}, {y})"
            );
            // And it went somewhere in between, or the loop is closed by the
            // field never having moved.
            let moved = (0..8)
                .any(|step| (spiral.wave.wandered(x, y, step as f64 / 8.0) - start).abs() > 1e-6);
            assert!(moved, "the field stood still all the way round, at ({x}, {y})");
        }
    }

    /// Held to a ceiling, and put away entirely at nought — which is the piece
    /// as it was first drawn and the one setting where the surface is only the
    /// three things that can be said in a sentence.
    #[test]
    fn the_wandering_is_held_and_can_be_put_away() {
        assert_eq!(churn(0.0), 0.0);
        assert_eq!(churn(-1.0), 0.0, "a surface cannot wander backwards");
        assert_eq!(churn(99.0), 4.0 * SWELL, "the wandering was let past the wave's own ceiling");
        let flat = made(&[("churn", "0")]);
        for (x, y) in [(0.0, 0.0), (0.2, 0.37), (-0.45, 0.1)] {
            assert_eq!(flat.wave.wandered(x, y, 0.4), 0.0, "nought still wandered at ({x}, {y})");
        }
    }

    /// A picture is drawn on a settled surface, and the wandering settles with
    /// the swell — the lens drags whatever stands on a slope, and this is the
    /// slope a viewer has no shape to allow for.
    #[test]
    fn a_picture_settles_the_wandering_as_well_as_the_wave() {
        let bare = made(&[("churn", "0.1")]).wave.churn;
        let under = made(&[("churn", "0.1"), ("text", BLOCK)]).wave.churn;
        assert!(under < bare, "{under} against {bare} with nothing laid on the disc");
        assert!(under > 0.0, "a laid picture left the surface with no wander at all");
    }

    /// The wave still runs over whatever the disc has been bent into, so lifting
    /// it adds a shape rather than replacing the piece with one.
    #[test]
    fn a_wave_still_runs_over_a_lifted_disc() {
        let swing = |dome: &str| {
            let spiral = made(&[("dome", dome)]);
            let over: Vec<f64> = (0..64)
                .map(|step| spiral.wave.at(0.3, 0.0, step as f64 / 64.0, spiral.swell).z)
                .collect();
            over.iter().cloned().fold(f64::MIN, f64::max)
                - over.iter().cloned().fold(f64::MAX, f64::min)
        };
        assert!((swing("1") - swing("0")).abs() < 1e-9, "{} against {}", swing("1"), swing("0"));
    }

    /// How tall the wave stands is asked for, and the settling a picture does is
    /// taken off whatever was asked — so the flag means the same thing with a
    /// file open as without one, and none of it leaves the wave standing.
    #[test]
    fn a_wave_stands_as_tall_as_it_is_asked_to() {
        let swing = |spiral: &Spiral| {
            let over: Vec<f64> = (0..64)
                .map(|step| spiral.wave.at(0.3, 0.0, step as f64 / 64.0, spiral.swell).z)
                .collect();
            over.iter().cloned().fold(f64::MIN, f64::max)
                - over.iter().cloned().fold(f64::MAX, f64::min)
        };
        let composed = swing(&made(&[]));
        let taller = swing(&made(&[("swell", "0.2")]));
        assert!(taller > composed * 3.0, "{taller} against {composed}");
        // Nothing left of the wave, which is a flat disc only if the wandering
        // is put away with it — the two are separate heights and either can
        // stand without the other.
        assert_eq!(swing(&made(&[("swell", "0"), ("churn", "0")])), 0.0);
        assert!(swing(&made(&[("swell", "0")])) > 0.0, "the wandering went with the wave");
        // Held to the same ceiling from either side of a picture being laid.
        let settled = swing(&made(&[("swell", "99"), ("text", BLOCK)]));
        let unsettled = swing(&made(&[("swell", "99")]));
        assert!(settled > 0.0 && settled < unsettled, "{settled} against {unsettled}");
    }

    #[test]
    fn a_lens_that_magnifies_by_nothing_is_refused() {
        let mut params = Params::default();
        params.flags.insert("zoom".into(), Some("0".into()));
        assert!(build(&params).is_err());
    }
}

