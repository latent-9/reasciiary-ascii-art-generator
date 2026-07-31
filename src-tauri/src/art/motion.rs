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
}
