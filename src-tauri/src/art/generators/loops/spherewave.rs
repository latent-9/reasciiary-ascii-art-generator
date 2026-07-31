//! A front crossing a sphere that is made of loose pieces rather than skin.
//!
//! After the *sphere wave* in [Bleuje's animations][ref], written here from the
//! idea rather than from that sketch.
//!
//! The sphere is a scattering, not a surface: a Fibonacci lattice of points, one
//! small flat element lying on the sphere at each of them, facing out. Nothing
//! joins them, so a wave passing through does not stretch a skin — it takes hold
//! of each element in turn, lifts it off, throws it about and sets it back down.
//! That is the whole difference between this and the sphere in `scene`, and it
//! is why the elements have to be loose.
//!
//! What travels is a band. An element knows only how near the band's middle is
//! to its own height, and outside the band it knows nothing at all — see
//! [`response`], which is exactly zero past its reach rather than merely small.
//! That is what closes the loop. The band starts and finishes *off* the sphere:
//! at the top of the period its middle is past the near pole and at the bottom
//! it is past the far one, so both ends of the period are the same sphere, at
//! rest, to the last bit. Nothing here relies on the noise happening to come
//! back round — although it does, which is what [`blended_noise`] is for, and is
//! what keeps the throw smooth while the band is over an element.
//!
//! Elements do not all set off together. Each lags its neighbours by a little,
//! read off its own index, or the front sweeps whole rings at once and the
//! sphere reads as a set of hoops lighting up rather than as a wave. The lag is
//! counted into how far past the poles the band has to start and finish.
//!
//! And the frame is cut for the largest the wave ever gets, once — see
//! [`reach`]. A fit that measured each frame would have the sphere breathe in
//! and out under the camera as the wave travelled over it, which reads as the
//! camera lurching rather than as the sphere being disturbed.
//!
//! [ref]: https://github.com/Bleuje/processing-animations-code

use std::f64::consts::{PI, SQRT_2, TAU};

use noise::Perlin;

use crate::art::generators::ascii::{Face, Vector3};
use crate::art::motion::{blended_noise, ease, scatter};

/// How far the front reaches either side of itself, in the sphere's own radius.
///
/// Narrow enough that most of the sphere is at rest at any moment — a wave you
/// can see the front of — and wide enough that the band holds a good few rings
/// of elements, so it arrives as a swell rather than as a line.
const BAND: f64 = 0.44;

/// How far out of the sphere the crest lifts an element.
const LIFT: f64 = 0.30;

/// And how far across the sphere it throws one.
const DRIFT: f64 = 0.22;

/// How much larger an element is at the crest. The lift alone reads as a bulge;
/// growing with it is what makes a disturbed element read as a separate thing
/// that has come away from the surface.
const BLOOM: f64 = 1.0;

/// How sharply the front takes hold and lets go. Above one, so an element leaves
/// and returns to rest with no speed at all, and the edge of the band is not a
/// crease travelling over the sphere.
const HARDNESS: f64 = 2.2;

/// How much of the band an element may lag its neighbours by.
const LAG: f64 = 0.5;

/// How coarse the field the throw is read from is. About one hump to a quarter
/// of the sphere, so neighbours mostly agree and the throw reads as the wave
/// tearing at the surface rather than as every element going its own way.
const GRAIN: f64 = 1.35;

/// How many elements the sphere is made of.
///
/// Below the floor there is no surface left to read, only a constellation; above
/// the ceiling an element is smaller than one sample of the raster and the whole
/// thing greys over.
pub fn count(given: usize) -> usize {
    given.clamp(120, 4000)
}

/// The furthest anything ever gets from the middle.
///
/// Declared rather than measured, because the piece knows and the quads in hand
/// do not — see [`crate::art::generators::ascii::Solid::from_quads_reaching`].
pub fn reach(count: usize) -> f64 {
    // Lifted, thrown across, and half a grown element's diagonal past its own
    // middle. The lift and the throw cannot both be full at once, so this is
    // over-generous by a little, which is the right way to be wrong about it.
    1.0 + LIFT + DRIFT + element(count) * (1.0 + BLOOM) * SQRT_2
}

pub fn model(count: usize, phase: f64, seed: u64) -> Vec<Face> {
    let one = Perlin::new(seed as u32);
    let other = Perlin::new((seed >> 32) as u32 ^ 0x9e37_79b9);
    let front = front(phase);
    let half = element(count);

    (0..count)
        .map(|index| {
            let out = lattice(index, count);
            let taken = response(out.z + lag(seed, index), front);
            let (side, up) = across(out);

            // One field, sampled at the element's own place, read a quarter turn
            // apart for the two directions across the surface. A quarter turn of
            // a circle is the direction at right angles to it, so the two throws
            // are as unalike as one field can make them and both come back round
            // together.
            let at = [out.x * GRAIN, out.y * GRAIN, out.z * GRAIN];
            let thrown = |quarter: f64| {
                taken * DRIFT * blended_noise(&one, &other, at, phase + quarter)
            };
            let (along, across) = (thrown(0.0), thrown(0.25));

            let lifted = 1.0 + taken * LIFT;
            let place = Vector3::new(
                out.x * lifted + side.x * along + up.x * across,
                out.y * lifted + side.y * along + up.y * across,
                out.z * lifted + side.z * along + up.z * across,
            );

            let size = half * (1.0 + taken * BLOOM);
            let corner = |wide: f64, tall: f64| {
                Vector3::new(
                    place.x + (side.x * wide + up.x * tall) * size,
                    place.y + (side.y * wide + up.y * tall) * size,
                    place.z + (side.z * wide + up.z * tall) * size,
                )
            };
            Face::new(
                [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)],
                out,
            )
        })
        .collect()
}

/// Where the middle of the band is, as a height on the sphere.
///
/// It runs from past one pole to past the other over the period, and how far
/// past is the whole of what makes the loop exact: far enough that no element,
/// however much it lags, is inside the band at either end.
fn front(phase: f64) -> f64 {
    (1.0 + BAND * (1.0 + LAG)) * (1.0 - 2.0 * phase)
}

/// How hard the front is riding an element sitting at height `height`.
///
/// Zero outside the band, and it reaches zero rather than approaching it: an
/// element the front has not got to yet, or has finished with, is exactly where
/// it started. That is what a period is allowed to be an identity by.
fn response(height: f64, front: f64) -> f64 {
    let near = (front - height).abs() / BAND;
    if near >= 1.0 {
        return 0.0;
    }
    ease(1.0 - near, HARDNESS)
}

/// How far behind its neighbours an element sets off, as a height.
fn lag(seed: u64, index: usize) -> f64 {
    (scatter(seed, index as u64) - 0.5) * LAG * BAND
}

/// Half the width of one element, from how many are sharing the sphere.
fn element(count: usize) -> f64 {
    // Each has 4π/count of the surface to itself, so they sit about that far
    // apart across it; a little over a third of that leaves gaps a reader can
    // still see when the wave has crowded them together.
    (2.0 * TAU / count.max(1) as f64).sqrt() * 0.38
}

/// The `index`th of `count` points spread evenly over the sphere.
///
/// Heights stepped evenly from pole to pole, turned by the golden angle each
/// time. Every other lattice on a sphere has a seam or a crowded pole; this one
/// has neither, and a wave crossing it is not secretly tracing a spiral.
fn lattice(index: usize, count: usize) -> Vector3 {
    let count = count.max(1) as f64;
    // Off the ends by half a step, so no element sits exactly on a pole where
    // there is no way across the surface to lay it.
    let height = 1.0 - 2.0 * (index as f64 + 0.5) / count;
    let radius = (1.0 - height * height).max(0.0).sqrt();
    // The golden angle: a whole turn cut in the most irrational place there is,
    // so no number of steps ever comes back round to where it began.
    let angle = PI * (3.0 - 5.0_f64.sqrt()) * index as f64;
    Vector3::new(radius * angle.cos(), radius * angle.sin(), height)
}

/// Two directions across the surface at `out`, for laying a flat element on it.
///
/// Taken against whichever axis `out` leans on least. A frame built against a
/// fixed axis collapses where the surface faces along it, which on a sphere is
/// the poles — and a lattice with a point near each pole would put a degenerate
/// element there every frame.
fn across(out: Vector3) -> (Vector3, Vector3) {
    let axis = if out.z.abs() < 0.9 {
        Vector3::new(0.0, 0.0, 1.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };
    let side = out.cross(axis).normalized();
    (side, side.cross(out).normalized())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame's worth of corners, rounded so two of them can be compared
    /// without asking the last bit of a float to agree.
    fn placed(phase: f64) -> Vec<[i64; 3]> {
        model(240, phase, 7)
            .iter()
            .flat_map(|face| {
                let middle = face.middle();
                [[middle.x, middle.y, middle.z].map(|held| (held * 1e9).round() as i64)]
            })
            .collect()
    }

    /// The whole claim of the piece: the band starts and finishes off the
    /// sphere, so a period is an identity rather than nearly one.
    #[test]
    fn a_period_leaves_the_sphere_exactly_as_it_found_it() {
        assert_eq!(placed(1.0), placed(0.0));
        assert_eq!(placed(0.0).len(), 240);
    }

    /// And at both ends it is the plain lattice — at rest means back on the
    /// sphere, not merely back somewhere it has been.
    #[test]
    fn at_the_ends_of_the_period_nothing_has_left_the_sphere() {
        for phase in [0.0, 1.0] {
            for (index, face) in model(240, phase, 7).iter().enumerate() {
                let middle = face.middle();
                let home = lattice(index, 240);
                let apart = middle.minus(home).length();
                assert!(apart < 1e-12, "element {index} is {apart} off the sphere");
            }
        }
    }

    /// The wave has to be somewhere in between, and it has to have been
    /// everywhere by the end: a front that only ever touched the middle of the
    /// sphere would pass all of the above.
    #[test]
    fn the_front_crosses_the_whole_sphere_inside_the_period() {
        let mut touched = vec![false; 240];
        for step in 1..40 {
            let phase = step as f64 / 40.0;
            for (index, face) in model(240, phase, 7).iter().enumerate() {
                let home = lattice(index, 240);
                if face.middle().minus(home).length() > 1e-6 {
                    touched[index] = true;
                }
            }
        }
        let missed = touched.iter().filter(|&&moved| !moved).count();
        assert_eq!(missed, 0, "{missed} of 240 elements were never disturbed");
    }

    /// Nothing ever gets further out than the frame was cut for, at any point
    /// of the period — the fit is declared once, so it has to be right for all
    /// of it rather than for the frame in hand.
    #[test]
    fn nothing_ever_reaches_past_what_the_frame_was_cut_for() {
        for count in [120, 240, 4000] {
            let allowed = reach(count);
            for step in 0..24 {
                let phase = step as f64 / 24.0;
                for face in model(count, phase, 7) {
                    for corner in face.corners() {
                        let out = corner.length();
                        assert!(out <= allowed, "{out} past {allowed} at {phase}");
                    }
                }
            }
        }
    }

    /// An element lies flat on the sphere facing out, which is what lets the
    /// lighting tell the near side from the far one.
    #[test]
    fn every_element_faces_away_from_the_middle() {
        for face in model(400, 0.3, 7) {
            let out = face.middle().normalized();
            let agreement = face.normal().dot(out);
            assert!(agreement > 0.8, "an element points {agreement} of the way out");
        }
    }

    /// The band is narrow, so most of the sphere is at rest at any one moment.
    /// A wave that had hold of everything is a pulse, and a pulse has no front
    /// to watch.
    #[test]
    fn the_front_only_ever_has_hold_of_a_part_of_the_sphere() {
        let front = front(0.5);
        let held = (0..240)
            .filter(|&index| response(lattice(index, 240).z + lag(7, index), front) > 0.0)
            .count();
        assert!((30..170).contains(&held), "{held} of 240 elements are in the band");
    }

    /// Neighbours do not set off together, or the wave arrives as a stack of
    /// hoops rather than as a front.
    #[test]
    fn elements_do_not_all_set_off_together() {
        let front = front(0.42);
        let held: Vec<f64> = (0..240)
            .map(|index| response(lattice(index, 240).z + lag(7, index), front))
            .filter(|&taken| taken > 0.0)
            .collect();
        let steps = held.windows(2).filter(|pair| pair[0] != pair[1]).count();
        assert!(steps > held.len() / 2, "only {steps} of {} differ", held.len());
    }

    #[test]
    fn the_count_stays_worth_drawing() {
        assert_eq!(count(0), 120);
        assert_eq!(count(700), 700);
        assert_eq!(count(99_999), 4000);
    }
}
