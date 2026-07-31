//! A rope lying on a torus, with swells travelling down it.
//!
//! After the travelling-wave pieces in [Bleuje's animations][ref], written here
//! from the idea rather than from those sketches.
//!
//! The figure is a curve that winds twice the long way round a torus while it
//! winds an odd number of times the short way, tubed into a rope. Nothing about
//! the curve moves — it is the same knot in every frame — and the whole of the
//! animation is two things running along it: the rope's thickness swells and
//! falls away, and the rope's own middle is wound in a slow coil whose winding
//! travels. One is a wave you watch arrive; the other is what makes the surface
//! between two waves look like it is flowing rather than sitting still.
//!
//! Both are functions of a whole number of repeats along the curve minus the
//! phase, and both are built out of things that repeat once over that phase, so
//! the frame at the end of the period is the frame at the start of it. There is
//! nothing to fade in or out: the loop is closed by arithmetic.
//!
//! The rope is framed by [`crate::art::surface::tube`], which builds its ring
//! against an upright and would jump if the curve ever leaned past it. This one
//! never does — its span is cut to its twists so that going round always
//! outruns going up — and a test holds it to that, because the constants here
//! are the only reason it is true.
//!
//! [ref]: https://github.com/Bleuje/processing-animations-code

use std::f64::consts::TAU;

use crate::art::generators::ascii::{Face, Vector3};
use crate::art::motion::swell;
use crate::art::surface::{surface, tube};

/// How far the curve's own ring is from the axis it goes round.
const RING: f64 = 1.0;

/// How far the curve strays either side of that ring, once its twists are
/// counted — see [`span`].
const SWEEP: f64 = 1.0;

/// How many turns the curve makes the long way. Two, so the rope crosses in
/// front of itself: a figure that can hide behind its own body is what says it
/// is a solid and not a diagram.
const WINDS: f64 = 2.0;

/// The rope's radius where no swell is on it, and how much fatter a swell makes
/// it, both as a part of the span.
///
/// A part of the span rather than a length, because the span is what says how
/// far apart the strands run: the curve passes any given point of the ring
/// twice, half a trip apart, and those two passes sit on opposite sides of the
/// small circle — two spans between them and nothing else. A rope measured in
/// anything but spans is one that grows through itself as soon as the twists go
/// up.
const THICK: f64 = 0.20;
const BULGE: f64 = 1.0;

/// How many swells are on the rope at once.
///
/// Whole, so the wave meets itself where the curve closes, and odd for the same
/// reason the twists are: half a trip along the curve is then half a swell, so
/// the two strands sharing a place on the ring are always in opposite states.
/// One is thinnest exactly where the other is fattest, and the pair takes up
/// the same room wherever you cut it.
const SWELLS: f64 = 3.0;

/// How far the rope's middle is wound off the curve, and how many times round
/// it over the whole length. Whole again, and small — a coil this size reads as
/// the surface flowing, and a larger one reads as a second knot.
const COIL: f64 = 0.30;
const COILS: f64 = 3.0;

/// How finely the rope is cut: along its length, and round it.
///
/// Along is where the detail is — the curve turns hard and the swells have to
/// grade — and round it is a circle a fifth of a unit across, which a dozen
/// quads already draw more finely than the raster can read.
const ALONG: usize = 300;
const AROUND: usize = 12;

/// How many turns the curve makes the short way, kept odd.
///
/// Two the long way against an even number the short way is one strand drawn
/// twice — the parameter comes back round halfway through — so a count that
/// would do that is nudged up to the next odd one rather than refused.
pub fn twists(given: usize) -> usize {
    given.clamp(3, 7) | 1
}

/// The furthest anything ever gets from the middle: the outside of the ring,
/// plus the coil, plus a rope at its fattest.
pub fn reach(twists: usize) -> f64 {
    RING + span(twists as f64) * (1.0 + COIL + THICK * (1.0 + BULGE))
}

pub fn model(twists: usize, phase: f64) -> Vec<Face> {
    let twists = twists as f64;
    surface(ALONG, AROUND, |along, round| {
        tube(
            |at| middle(twists, at, phase),
            along,
            round,
            thickness(twists, along, phase),
        )
    })
}

/// How far the curve strays either side of its ring.
///
/// Cut to the twists rather than fixed, so that the more times the curve goes
/// up and down the less far it goes each time. Left fixed, a curve with many
/// twists would spend its time climbing rather than going round, and the frame
/// the rope is wrapped in is built on the assumption that it never does.
fn span(twists: f64) -> f64 {
    SWEEP / twists
}

/// The curve, with `along` running a whole trip round it over nought to one.
fn curve(twists: f64, along: f64) -> Vector3 {
    let (round, through) = (TAU * WINDS * along, TAU * twists * along);
    let span = span(twists);
    let reach = RING + span * through.cos();
    Vector3::new(reach * round.cos(), reach * round.sin(), span * through.sin())
}

/// The rope's own middle: the curve with a coil wound round it.
///
/// The coil is a point on a tube of its own, which is what a tube is for — the
/// ring it draws is the coil, and the rope is then wrapped round that. Its
/// winding turns by one over the period, so the coil travels along the rope
/// rather than the rope turning.
fn middle(twists: f64, along: f64, phase: f64) -> Vector3 {
    let round = COILS * along - phase;
    tube(|at| curve(twists, at), along, round, span(twists) * COIL).point
}

/// How thick the rope is at a point along it, with the swells where they have
/// got to by this phase.
fn thickness(twists: f64, along: f64, phase: f64) -> f64 {
    span(twists) * THICK * (1.0 + BULGE * swell(SWELLS * along - phase))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The point of the rope's middle that the face at `index` was wrapped
    /// round.
    ///
    /// Read off where the face sits in the list rather than searched for:
    /// [`surface`] cuts along the length first and round the rope inside that,
    /// so a face's place in the list is its place on the curve. Searching would
    /// be worse than slow — where the rope passes close to itself the nearest
    /// middle is the *other* strand's, and a test that measured that would be
    /// asking a question about the wrong piece of rope.
    fn owner(twists: f64, phase: f64, index: usize) -> Vector3 {
        let along = ((index / AROUND) as f64 + 0.5) / ALONG as f64;
        middle(twists, along, phase)
    }

    fn apart(one: &[Face], other: &[Face]) -> f64 {
        one.iter()
            .zip(other)
            .map(|(one, other)| one.middle().minus(other.middle()).length())
            .fold(0.0, f64::max)
    }

    /// The whole claim of the piece: the end of the period is the start of it.
    #[test]
    fn a_period_leaves_the_rope_where_it_found_it() {
        for count in [3, 5, 7] {
            let start = model(count, 0.0);
            let round = model(count, 1.0);
            assert!(apart(&start, &round) < 1e-9, "{count} twists do not come back");
        }
    }

    /// And the middle of it is somewhere else.
    #[test]
    fn the_rope_is_not_the_same_halfway_through() {
        let start = model(5, 0.0);
        let middle = model(5, 0.5);
        assert!(apart(&start, &middle) > 0.05, "the rope barely moves");
    }

    /// A swell is a shape travelling along the rope rather than the rope
    /// growing all over: the thickness a third of the way through the period is
    /// the thickness at the start, moved a third of the way along one swell.
    #[test]
    fn the_swells_travel_rather_than_the_whole_rope_breathing() {
        for step in 0..8 {
            let along = step as f64 / 8.0;
            let moved = thickness(5.0, along + 0.3 / SWELLS, 0.3);
            assert!((moved - thickness(5.0, along, 0.0)).abs() < 1e-12, "{moved}");
        }
        assert!(
            thickness(5.0, 0.2, 0.0) != thickness(5.0, 0.2, 0.3),
            "nothing travelled"
        );
    }

    /// The rope is a rope: never pinched to nothing where the curve turns
    /// hardest, never fatter than the swell it was promised.
    #[test]
    fn the_rope_keeps_between_its_thinnest_and_its_fattest() {
        for count in [3.0, 5.0, 7.0] {
            let thinnest = span(count) * THICK;
            let fattest = thinnest * (1.0 + BULGE);
            for phase in [0.0, 0.37] {
                for (index, face) in model(count as usize, phase).iter().enumerate() {
                    let out = face.middle().minus(owner(count, phase, index)).length();
                    assert!(
                        (thinnest * 0.9..=fattest * 1.02).contains(&out),
                        "a rope of {count} twists runs {out} from its middle"
                    );
                }
            }
        }
    }

    /// A quad's normal has to point out of the rope rather than into it.
    #[test]
    fn every_quad_faces_out_of_the_rope() {
        for (index, face) in model(5, 0.2).iter().enumerate() {
            let out = face.middle().minus(owner(5.0, 0.2, index)).normalized();
            let agreement = face.normal().dot(out);
            assert!(agreement > 0.8, "a quad points {agreement} of the way out");
        }
    }

    /// The curve passes every place on its ring twice, half a trip apart, and
    /// those two passes have two spans between them and nothing else. Between
    /// them the pair must not need all of it, or the rope grows through itself.
    ///
    /// Odd swells are what makes this hold everywhere rather than on average:
    /// half a trip is half a swell, so one strand is thinnest exactly where the
    /// other is fattest and the pair takes up the same room wherever it is cut.
    #[test]
    fn the_two_strands_that_share_a_place_never_meet() {
        for count in [3.0, 5.0, 7.0] {
            let room = 2.0 * span(count);
            for step in 0..16 {
                let along = step as f64 / 16.0;
                let taken = thickness(count, along, 0.3)
                    + thickness(count, along + 0.5, 0.3)
                    + 2.0 * span(count) * COIL;
                assert!(taken < room, "{taken} of {room} taken at {along}");
            }
        }
    }

    /// The frame the rope is wrapped in is built against the upright, and would
    /// swing round to another one if the curve ever ran near vertical. It must
    /// not: a swing is a ring of sheared quads and a seam across the rope.
    #[test]
    fn the_curve_never_leans_far_enough_to_swing_its_frame() {
        let mut worst: f64 = 0.0;
        for count in [3.0, 5.0, 7.0] {
            for phase in [0.0, 0.31, 0.68] {
                for step in 0..2048 {
                    let along = step as f64 / 2048.0;
                    let forward = middle(count, along + 1e-5, phase)
                        .minus(middle(count, along, phase))
                        .normalized();
                    worst = worst.max(forward.z.abs());
                }
            }
        }
        assert!(worst < 0.8, "the curve leans {worst} of the way up");
    }

    /// Nothing gets past the fit the frame was cut for.
    #[test]
    fn nothing_ever_reaches_past_what_the_frame_was_cut_for() {
        for count in [3, 5, 7] {
            let allowed = reach(count);
            for step in 0..12 {
                let phase = step as f64 / 12.0;
                for face in model(count, phase) {
                    for corner in face.corners() {
                        let out = corner.length();
                        assert!(out <= allowed, "{out} past {allowed} at {phase}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_twists_stay_odd_and_worth_drawing() {
        for given in 0..16 {
            let count = twists(given);
            assert!((3..=7).contains(&count), "{given} became {count}");
            assert_eq!(count % 2, 1, "{count} twists draw the same strand twice");
        }
    }
}
