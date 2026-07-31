//! How a thing moves over a loop that closes.
//!
//! Everything this app exports is a loop — a GIF that meets itself, an MP4 that
//! can be posted and watched twice without a seam — so a tool never animates
//! against a clock, it animates against a *phase* running 0 to 1 and arriving
//! back where it started. That constraint is the whole craft, and it is not
//! obvious how to satisfy it with the usual materials: noise walked along a line
//! never returns, and a cosine returns but says the same thing every time.
//!
//! These are the answers, from [Bleuje's Processing animations][ref], written out
//! once here so that no tool has to work them out again. Nothing in this module
//! knows what is being moved.
//!
//! [ref]: https://github.com/Bleuje/processing-animations-code

use std::f64::consts::TAU;

use noise::{NoiseFn, Perlin};

use super::params::Params;

/// The same curve either side of the middle, steepened by `hardness`.
///
/// Bleuje's easing, and it does something a cosine cannot. A cosine spends most
/// of a cycle on the way somewhere; this one spends it arrived, and turns over
/// quickly in between. On a heightfield that difference is walls — broad flats
/// separated by short steep sides, which is what shading has to work with — and
/// on a movement it is the difference between drifting and having gone somewhere.
pub fn ease(progress: f64, hardness: f64) -> f64 {
    if progress < 0.5 {
        0.5 * (2.0 * progress).powf(hardness)
    } else {
        1.0 - 0.5 * (2.0 * (1.0 - progress)).powf(hardness)
    }
}

/// Nothing at the ends of a loop and everything in the middle of it.
///
/// Where a thing has to travel and then be back at its start, this is what it
/// travels by: it leaves and returns with zero speed, so the moment the loop is
/// cut at is the moment nothing is happening.
pub fn swell(phase: f64) -> f64 {
    0.5 - 0.5 * (TAU * phase).cos()
}

/// Deterministic scatter: the same seed and index always give the same number,
/// in `0..1`.
///
/// A generator that cannot be drawn twice is not much use — a piece worth keeping
/// has to be reachable again — and a field's worth of positions is only
/// reproducible if nothing ever changes the order they are drawn in. A hash of
/// the index has no order to get wrong. This is splitmix64, which is the usual
/// answer to exactly this.
pub fn scatter(seed: u64, index: u64) -> f64 {
    let mut state = seed
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(index.wrapping_mul(0xbf58_476d_1ce4_e5b9));
    state ^= state >> 30;
    state = state.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    state ^= state >> 27;
    state = state.wrapping_mul(0x94d0_49bb_1331_11eb);
    state ^= state >> 31;
    // The top 53 bits, which is every bit an f64 can hold without rounding.
    (state >> 11) as f64 / (1_u64 << 53) as f64
}

/// Noise over a plane, animated by walking a *circle* through the field rather
/// than a line.
///
/// This is the trick and it is worth being plain about: noise sampled along a
/// line never comes back, so a field animated that way cannot loop. Sampled
/// round a circle it returns exactly, at no cost — the same noise, at two more
/// coordinates. `radius` is how far the circle reaches: small and the loop barely
/// moves, large and it churns.
pub fn circle_noise(field: &Perlin, x: f64, y: f64, phase: f64, radius: f64) -> f64 {
    let (around, through) = (TAU * phase).sin_cos();
    field.get([x, y, radius * through, radius * around])
}

/// The same guarantee where the subject has three dimensions of its own.
///
/// Perlin noise here has four coordinates and a solid has already spent three of
/// them, so there is no room left to draw the circle *in the field*. It can be
/// drawn between two fields instead: `a·cos + b·sin` traces a circle in the plane
/// the two of them span, is smooth because both of them are, and is periodic in
/// the phase because the sine and the cosine are. Two independent fields is the
/// whole cost.
pub fn blended_noise(one: &Perlin, other: &Perlin, at: [f64; 3], phase: f64) -> f64 {
    let (around, through) = (TAU * phase).sin_cos();
    one.get(at) * through + other.get(at) * around
}

/// How many waves a ripple fits between the middle of a surface and its edge.
///
/// Few enough that each ring is wide enough to shade across, which is what makes
/// it read as a surface bending rather than as a texture laid over one.
const RINGS: f64 = 2.0;

/// What a drift's two fields are opened up to before they are cut off.
///
/// Perlin noise spends almost all of its time well inside its own range, so a
/// field taken as it comes uses about half the swing it was given. Opened up
/// this far it uses the whole of it, and the little that overshoots is cut off
/// at the ends, where a surface is at its furthest and standing still anyway.
const LOUDNESS: f64 = 1.6;

/// What a surface does over a loop, on top of whatever else it is doing.
///
/// Three of them and one for none, because a surface that moves has only so many
/// things it can do that still read as one surface: something can travel across
/// it, it can rise and fall as a whole, or it can wander. Each says how far to
/// push at a place, and what pushing means is left to whoever asked — a
/// heightfield scales its columns by it, a solid slides its corners out along
/// their own normals.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Motion {
    /// The surface as it was given.
    #[default]
    None,
    /// Rings travelling out from the middle.
    Ripple,
    /// The whole surface swelling and settling.
    Breathe,
    /// Noise wandering over it.
    Drift,
}

impl Motion {
    pub fn named(name: &str) -> Result<Self, String> {
        match name {
            "none" => Ok(Self::None),
            "ripple" => Ok(Self::Ripple),
            "breathe" => Ok(Self::Breathe),
            "drift" => Ok(Self::Drift),
            other => Err(format!(
                "`{other}` is not a motion — try none, ripple, breathe or drift"
            )),
        }
    }
}

/// A [`Motion`] with the strength it is applied at and the fields it needs.
///
/// The fields are built once and kept: a drift asks two of them a question for
/// every corner of every frame, and building one per question would cost more
/// than the surface does.
pub struct Movement {
    motion: Motion,
    amount: f64,
    one: Perlin,
    other: Perlin,
}

impl Movement {
    /// `amount` is how far the surface is pushed, as a fraction of its own
    /// reach. Held to nought and one because a heightfield scales by it, and
    /// anything past one turns a column inside out rather than moving it.
    pub fn new(motion: Motion, amount: f64, seed: u32) -> Self {
        Self {
            motion,
            amount: amount.clamp(0.0, 1.0),
            one: Perlin::new(seed),
            // A second field of its own, not this one read somewhere else: the
            // circle is drawn between the two, so they have to be independent.
            other: Perlin::new(seed.wrapping_add(1)),
        }
    }

    pub fn from_params(params: &Params) -> Result<Self, String> {
        Ok(Self::new(
            Motion::named(params.string("motion").unwrap_or("none"))?,
            params.f64("amount", 0.35)?,
            params.seed(7)? as u32,
        ))
    }

    /// Whether there is anything to redraw for. A tool asks before it takes on
    /// the cost of rebuilding its surface every frame.
    pub fn moves(&self) -> bool {
        self.motion != Motion::None && self.amount > 0.0
    }

    /// The furthest this can ever push, as a fraction of the surface's reach.
    /// A surface that is rebuilt every frame has to be framed for this rather
    /// than for the frame in hand — see [`crate::art::generators::ascii::Solid`].
    pub fn amount(&self) -> f64 {
        if self.moves() {
            self.amount
        } else {
            0.0
        }
    }

    /// How far to push at `place`, at `phase` of the loop.
    ///
    /// `place` is measured in the surface's own reach, so that the middle is
    /// nought and an edge is about one either way whether the surface is a
    /// drawing two hundred cells across or a ball of radius one. Never further
    /// than [`amount`](Self::amount) in either direction, and exactly the same
    /// at both ends of the loop.
    pub fn at(&self, place: [f64; 3], phase: f64) -> f64 {
        let [x, y, z] = place;
        let push = match self.motion {
            Motion::None => 0.0,
            // Periodic in the phase on its own — a travelling wave arrives back
            // one whole wave along, which is where it started — so it needs
            // nothing holding its ends down.
            Motion::Ripple => {
                let out = (x * x + y * y + z * z).sqrt();
                (TAU * (RINGS * out - phase)).sin()
            }
            // Out and back over the loop, starting from rest rather than from
            // the top of the breath: the first frame is then the drawing as it
            // was written, which is the one somebody recognises.
            Motion::Breathe => (TAU * phase).sin(),
            Motion::Drift => LOUDNESS * blended_noise(&self.one, &self.other, place, phase),
        };
        self.amount() * push.clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_holds_the_ends_and_the_middle_where_they_were() {
        for hardness in [1.0, 2.6, 5.0] {
            assert!(ease(0.0, hardness).abs() < 1e-12);
            assert!((ease(1.0, hardness) - 1.0).abs() < 1e-12);
            assert!((ease(0.5, hardness) - 0.5).abs() < 1e-12);
        }
    }

    /// The point of the shape: it spends the middle of its travel moving and the
    /// ends of it arrived, which a straight line does not.
    #[test]
    fn easing_turns_over_faster_than_it_leaves() {
        let early = ease(0.15, 2.6) - ease(0.05, 2.6);
        let middle = ease(0.55, 2.6) - ease(0.45, 2.6);
        assert!(middle > early * 3.0, "{middle} against {early}");
    }

    #[test]
    fn a_swell_is_nothing_at_both_ends_and_everything_in_the_middle() {
        assert!(swell(0.0).abs() < 1e-12);
        assert!(swell(1.0).abs() < 1e-12);
        assert!((swell(0.5) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_same_seed_and_index_scatter_the_same_way() {
        assert_eq!(scatter(7, 3), scatter(7, 3));
        assert_ne!(scatter(7, 3), scatter(8, 3));
        assert_ne!(scatter(7, 3), scatter(7, 4));
        for index in 0..64 {
            let value = scatter(11, index);
            assert!((0.0..1.0).contains(&value), "{value}");
        }
    }

    /// Both fields have to come back to themselves after a period, and to have
    /// been somewhere else in the middle of it. Everything that loops in this app
    /// rests on the first half of that sentence.
    #[test]
    fn a_period_brings_both_fields_back_to_themselves() {
        let field = Perlin::new(7);
        let other = Perlin::new(8);

        for step in 0..8 {
            let x = step as f64 * 0.37;
            let start = circle_noise(&field, x, 0.4, 0.0, 0.38);
            let round = circle_noise(&field, x, 0.4, 1.0, 0.38);
            assert!((start - round).abs() < 1e-12, "{start} != {round}");

            let at = [x, 0.4, 0.9];
            let start = blended_noise(&field, &other, at, 0.0);
            let round = blended_noise(&field, &other, at, 1.0);
            assert!((start - round).abs() < 1e-12, "{start} != {round}");
        }
    }

    #[test]
    fn neither_field_stands_still_in_the_middle_of_a_period() {
        let field = Perlin::new(7);
        let other = Perlin::new(8);
        let moved = (0..32)
            .map(|step| step as f64 * 0.29)
            .filter(|&x| {
                (circle_noise(&field, x, 0.4, 0.0, 0.38)
                    - circle_noise(&field, x, 0.4, 0.5, 0.38))
                .abs()
                    > 1e-3
            })
            .count();
        assert!(moved > 24, "only {moved} of 32 samples moved");

        let start = blended_noise(&field, &other, [0.3, 0.4, 0.9], 0.0);
        let middle = blended_noise(&field, &other, [0.3, 0.4, 0.9], 0.5);
        assert!((start - middle).abs() > 1e-6, "{start} == {middle}");
    }

    /// A spread of places over a surface of about unit reach, which is the scale
    /// [`Movement::at`] asks to be given.
    fn places() -> Vec<[f64; 3]> {
        (0..12)
            .flat_map(|row| {
                (0..12).map(move |column| {
                    [column as f64 / 6.0 - 1.0, row as f64 / 6.0 - 1.0, 0.0_f64]
                })
            })
            .collect()
    }

    const KINDS: [Motion; 3] = [Motion::Ripple, Motion::Breathe, Motion::Drift];

    /// The one thing every motion here has to do, and the only one that cannot
    /// be seen by looking at a single frame.
    #[test]
    fn every_motion_ends_the_loop_where_it_began() {
        for motion in KINDS {
            let movement = Movement::new(motion, 0.5, 3);
            for place in places() {
                let start = movement.at(place, 0.0);
                let round = movement.at(place, 1.0);
                assert!((start - round).abs() < 1e-9, "{motion:?} at {place:?}: {start} != {round}");
            }
        }
    }

    #[test]
    fn every_motion_has_gone_somewhere_in_between() {
        for motion in KINDS {
            let movement = Movement::new(motion, 0.5, 3);
            let moved = places()
                .into_iter()
                .filter(|place| (movement.at(*place, 0.0) - movement.at(*place, 0.27)).abs() > 1e-3)
                .count();
            assert!(moved > 100, "{motion:?} moved only {moved} of 144 places");
        }
    }

    /// What the strength is worth is that it can be trusted: a surface that is
    /// rebuilt every frame is framed for the largest it will ever be, before any
    /// of those frames exist.
    #[test]
    fn no_motion_pushes_further_than_it_was_allowed() {
        for motion in KINDS {
            let movement = Movement::new(motion, 0.4, 3);
            for place in places() {
                for step in 0..40 {
                    let push = movement.at(place, step as f64 / 40.0);
                    assert!(push.abs() <= 0.4 + 1e-12, "{motion:?} pushed {push}");
                }
            }
        }
    }

    /// And that it is worth having at all: a strength the surface barely uses
    /// would make the slider a lie.
    #[test]
    fn every_motion_uses_most_of_the_strength_it_was_given() {
        for motion in KINDS {
            let movement = Movement::new(motion, 0.4, 3);
            let hardest = places()
                .into_iter()
                .flat_map(|place| (0..40).map(move |step| (place, step)))
                .map(|(place, step)| movement.at(place, step as f64 / 40.0).abs())
                .fold(0.0, f64::max);
            assert!(hardest > 0.4 * 0.9, "{motion:?} reaches only {hardest} of 0.4");
        }
    }

    /// Nothing asked for is nothing done, whatever the strength says, and a
    /// strength of nothing is the same. Both are what a tool checks before it
    /// takes on rebuilding its surface for every frame.
    #[test]
    fn a_surface_nobody_asked_to_move_stays_where_it_is() {
        for movement in [Movement::new(Motion::None, 1.0, 3), Movement::new(Motion::Drift, 0.0, 3)] {
            assert!(!movement.moves());
            assert_eq!(movement.amount(), 0.0);
            for place in places() {
                assert_eq!(movement.at(place, 0.4), 0.0);
            }
        }
    }

    #[test]
    fn an_unknown_motion_says_what_there_is() {
        let message = Motion::named("wobble").unwrap_err();
        assert!(message.contains("ripple"), "{message}");
    }
}
