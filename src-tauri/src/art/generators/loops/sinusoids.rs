//! Circles packed into the frame, each with a wave running through it.
//!
//! After the *sinusoids packing* piece in [Bleuje's animations][ref], written
//! here from the idea rather than from that sketch.
//!
//! Two ideas held together. The packing is the composition: circles dropped one
//! at a time, each grown until it meets something, which fills a frame with
//! sizes that were never chosen and never repeat. The wave is what the circle
//! is for — a sinusoid cut off by the edge of its own disc, so a disc is read
//! as a window onto a wave rather than as a shape with something drawn in it.
//!
//! The packing does not move. It is settled once and then held, because a
//! packing resolved again each frame is not a composition, it is a flicker; all
//! the movement is inside the discs, where every wave travels a whole number of
//! its own wavelengths over the period and so arrives back exactly.
//!
//! [ref]: https://github.com/Bleuje/processing-animations-code

use std::f64::consts::TAU;

use crate::art::generators::paper::{hue, Paper};
use crate::art::motion::{scatter, swell};

/// The square the circles are packed into. Nothing here swings outside the
/// frame the way a pivot does, so this is close to the whole of it.
const SIDE: f64 = 0.94;

/// The smallest circle worth packing. A disc under about four rows tall has no
/// room to show a wave — the crest and the trough land in the same cell — so
/// the packing stops rather than filling the gaps with specks.
const SMALLEST: f64 = 0.058;

/// And the largest, so one lucky first circle cannot take a quarter of the
/// frame and leave the rest as trim.
const LARGEST: f64 = 0.19;

/// The gap held between two circles, so a packing reads as circles that touch
/// rather than as one shape with seams.
const CLEARANCE: f64 = 0.008;

/// How many places are tried for each circle asked for. A packing runs out of
/// room long before it runs out of tries, which is the point: the count is a
/// wish and the frame is the answer.
const TRIES: usize = 120;

/// Points along a wave, across a disc.
const STEPS: usize = 96;

/// Points around a disc's own edge.
const EDGE: usize = 72;

/// The outline, against the wave. Present, because it is what says the wave is
/// cut off rather than fading out; quiet, because a frame of hard circles reads
/// as the packing and not as the waves.
const OUTLINE: f64 = 0.45;

/// One circle, and the wave that belongs to it. Settled at build time.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Disc {
    at: (f64, f64),
    radius: f64,
    /// Which way the wave runs across the disc.
    angle: f64,
    /// Wavelengths across the diameter.
    waves: f64,
    /// Whole wavelengths it travels over the period. Whole, or it would not
    /// arrive back.
    travel: f64,
    /// Where in the period this disc is, so no frame is the one where every
    /// wave is doing the same thing.
    offset: f64,
}

/// Circles dropped one at a time, each grown until it meets something.
///
/// Dropping and growing rather than solving: a circle takes whatever room is
/// left where it landed, so the sizes come out of the order they arrived in.
/// That is what gives a packing its look, and it is also why it has to be
/// settled once — the same list, every frame, or the composition twitches.
pub fn pack(seed: u64, count: usize) -> Vec<Disc> {
    let count = count.clamp(4, 200);
    let mut discs: Vec<Disc> = Vec::with_capacity(count);

    for attempt in 0..count * TRIES {
        if discs.len() >= count {
            break;
        }
        let index = attempt as u64 * 8;
        let at = (
            (scatter(seed, index) - 0.5) * SIDE,
            (scatter(seed, index + 1) - 0.5) * SIDE,
        );

        let mut radius = LARGEST.min(SIDE / 2.0 - at.0.abs()).min(SIDE / 2.0 - at.1.abs());
        for other in &discs {
            radius = radius.min(gap(at, other.at) - other.radius - CLEARANCE);
        }
        if radius < SMALLEST {
            continue;
        }

        discs.push(Disc {
            at,
            radius,
            angle: scatter(seed, index + 2) * TAU,
            // A wave and a half is one crest and one trough across the disc,
            // which is the fewest that reads as a wave at all.
            waves: 1.2 + scatter(seed, index + 3) * 1.6,
            travel: (1.0 + (scatter(seed, index + 4) * 3.0).floor())
                * if scatter(seed, index + 5) < 0.5 { 1.0 } else { -1.0 },
            offset: scatter(seed, index + 6),
        });
    }
    discs
}

/// One frame.
pub fn draw(paper: &mut Paper, discs: &[Disc], phase: f64, colored: bool) {
    for disc in discs {
        let tint = if colored { hue(disc.offset + phase) } else { [1.0; 3] };
        let weight = (disc.radius * 0.075).max(0.003);

        let edge: Vec<(f64, f64)> = (0..=EDGE)
            .map(|step| {
                let (sin, cos) = (TAU * step as f64 / EDGE as f64).sin_cos();
                (disc.at.0 + cos * disc.radius, disc.at.1 + sin * disc.radius)
            })
            .collect();
        paper.stroke(&edge, weight * 0.8, tint, OUTLINE);

        for run in wave(disc, phase) {
            paper.stroke(&run, weight, tint, 1.0);
        }
    }
}

/// The wave inside one disc, in the pieces of it the disc does not cut off.
///
/// A sinusoid drawn across a circle leaves at the top and comes back, and the
/// part outside is not drawn faintly or clamped to the edge — it is simply not
/// there, which is what makes the disc read as a window rather than as a shape
/// the wave has been squeezed into.
fn wave(disc: &Disc, phase: f64) -> Vec<Vec<(f64, f64)>> {
    // How tall the wave stands, breathing over the period on its own offset:
    // nearly flat at one end of it and filling the disc at the other, so a
    // frame has waves at every height in it.
    let height = disc.radius * (0.15 + 0.6 * swell((phase + disc.offset).rem_euclid(1.0)));
    let (sin, cos) = disc.angle.sin_cos();

    let mut runs = Vec::new();
    let mut run: Vec<(f64, f64)> = Vec::new();
    for step in 0..=STEPS {
        let along = (step as f64 / STEPS as f64 * 2.0 - 1.0) * disc.radius;
        let turn = along / disc.radius * disc.waves + disc.travel * phase + disc.offset;
        let across = height * (TAU * turn).sin();

        if along * along + across * across > disc.radius * disc.radius {
            if run.len() > 1 {
                runs.push(std::mem::take(&mut run));
            }
            run.clear();
            continue;
        }
        run.push((
            disc.at.0 + along * cos - across * sin,
            disc.at.1 + along * sin + across * cos,
        ));
    }
    if run.len() > 1 {
        runs.push(run);
    }
    runs
}

fn gap(one: (f64, f64), other: (f64, f64)) -> f64 {
    ((one.0 - other.0).powi(2) + (one.1 - other.1).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a packing is: circles inside the frame that do not run into each
    /// other, and enough of them to be a composition.
    #[test]
    fn the_circles_fit_the_frame_and_keep_off_each_other() {
        let discs = pack(7, 40);
        assert!(discs.len() > 12, "only {} circles were placed", discs.len());

        for (index, disc) in discs.iter().enumerate() {
            assert!(disc.radius >= SMALLEST, "a circle of {}", disc.radius);
            assert!(disc.radius <= LARGEST, "a circle of {}", disc.radius);
            assert!(
                disc.at.0.abs() + disc.radius <= SIDE / 2.0 + 1e-9,
                "a circle runs off the side"
            );
            assert!(
                disc.at.1.abs() + disc.radius <= SIDE / 2.0 + 1e-9,
                "a circle runs off the top"
            );
            for other in &discs[index + 1..] {
                let clear = gap(disc.at, other.at) - disc.radius - other.radius;
                assert!(clear >= CLEARANCE - 1e-9, "two circles overlap by {clear}");
            }
        }
    }

    /// A small count is met exactly and a large one is not met at all: the
    /// frame fills up and stops. That is what the dial does — it thins a
    /// packing out, and past the point where the frame is full it does
    /// nothing, which is the frame answering rather than the flag failing.
    #[test]
    fn a_small_count_is_met_and_a_large_one_is_the_frame_full() {
        assert_eq!(pack(7, 6).len(), 6);
        assert_eq!(pack(7, 200).len(), pack(7, 60).len());
    }

    /// The packing is asked for once and is the same answer every time, which
    /// is what lets it be settled at build time and held.
    #[test]
    fn the_same_seed_packs_the_same_circles() {
        assert_eq!(pack(7, 40), pack(7, 40));
        assert_ne!(pack(7, 40), pack(8, 40));
    }

    /// Whole wavelengths over the period, so a wave is where it started.
    #[test]
    fn every_wave_arrives_back_at_the_end_of_the_period() {
        for disc in pack(7, 40) {
            assert_eq!(disc.travel, disc.travel.round());
            assert!(disc.travel != 0.0);

            let start = wave(&disc, 0.0);
            let round = wave(&disc, 1.0);
            assert_eq!(start.len(), round.len(), "the wave breaks up differently");
            for (one, other) in start.iter().flatten().zip(round.iter().flatten()) {
                assert!((one.0 - other.0).abs() < 1e-9, "{one:?} != {other:?}");
                assert!((one.1 - other.1).abs() < 1e-9, "{one:?} != {other:?}");
            }
        }
    }

    /// And it has been somewhere else in between.
    #[test]
    fn the_waves_are_not_where_they_were_in_the_middle_of_the_period() {
        let discs = pack(7, 40);
        let moved = discs
            .iter()
            .filter(|disc| {
                let start = wave(disc, 0.0);
                let middle = wave(disc, 0.41);
                start
                    .iter()
                    .flatten()
                    .zip(middle.iter().flatten())
                    .any(|(one, other)| gap(*one, *other) > disc.radius / 8.0)
            })
            .count();
        assert!(moved > discs.len() / 2, "only {moved} waves moved");
    }

    /// Nothing may be drawn outside the disc it belongs to — the cut is the
    /// whole reason the wave reads as seen through a circle.
    #[test]
    fn no_wave_leaves_its_own_circle() {
        for disc in pack(7, 40) {
            for tick in 0..12 {
                for point in wave(&disc, tick as f64 / 12.0).iter().flatten() {
                    let out = gap(*point, disc.at);
                    assert!(out <= disc.radius + 1e-9, "{out} past a radius {}", disc.radius);
                }
            }
        }
    }
}
