//! Cutting a surface into quads, and wrapping one round a curve.
//!
//! A surface given by two parameters — go round one way, go round the other,
//! and the patch between four neighbouring samples is a face — is how every
//! solid in this app that is not a heightfield gets made. The cutting is the
//! same work whoever is asking for it, so it is written here once, next to
//! [`crate::art::motion`], and the shapes are left to say only what they are.
//!
//! Nothing here knows what is being cut, and nothing here moves: a piece that
//! animates hands in a different formula each frame and gets a different set of
//! quads back.

use std::f64::consts::TAU;

use super::generators::ascii::{Face, Vector3};

/// How far along a curve to look to see which way it is going, in turns.
///
/// Small enough that the chord is the tangent for any purpose here, large
/// enough that it is not the difference between two nearly equal numbers.
const NUDGE: f64 = 1e-5;

/// A point on a surface, and the point on the shape's own middle it hangs off:
/// nothing for a sphere, the ring for a torus, the curve the tube follows for a
/// knot.
///
/// The second is there to say which way is out, which the surface itself cannot
/// — see [`surface`].
#[derive(Clone, Copy)]
pub struct Sample {
    pub point: Vector3,
    pub inside: Vector3,
}

impl Sample {
    /// A sample on a surface wrapped around a single point.
    pub fn about_the_middle(point: Vector3) -> Self {
        Self { point, inside: Vector3::new(0.0, 0.0, 0.0) }
    }
}

/// Cuts a surface given by two parameters into quads.
///
/// The normal is taken from the quad itself — the cross product of its two
/// sides — rather than from the formula. It costs the same and it is the one
/// the shading wants: a quad is drawn flat, so lighting it by a normal its own
/// corners do not agree with leaves a seam along every edge where the two
/// answers part company.
///
/// Which of the two directions that product comes out in is another matter. It
/// follows the order the parameters run in, and each formula picks that order
/// for its own reasons — a sphere and a torus disagree about it — so the sample
/// says which side is the outside and the quad is turned to match.
pub fn surface(around: usize, through: usize, at: impl Fn(f64, f64) -> Sample) -> Vec<Face> {
    let around = around.max(3);
    let through = through.max(2);
    let mut faces = Vec::with_capacity(around * through);
    for step in 0..around {
        let (u0, u1) = (step as f64 / around as f64, (step + 1) as f64 / around as f64);
        for ring in 0..through {
            let (v0, v1) = (ring as f64 / through as f64, (ring + 1) as f64 / through as f64);
            let samples = [at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)];
            let corners = samples.map(|sample| sample.point);
            let across = corners[1].minus(corners[0]);
            let down = corners[3].minus(corners[0]);
            let turn = across.cross(down).normalized();
            // The first corner against its own middle, so the two are a matched
            // pair however coarsely the surface is cut. Where the quad closes to
            // a triangle and the cross product has nothing to say — at a pole,
            // where a whole ring of samples is one point — this is the entire
            // answer.
            let out = corners[0].minus(samples[0].inside);
            let normal = if turn.dot(out) < 0.0 { turn.negated() } else { turn };
            faces.push(Face::new(corners, normal));
        }
    }
    faces
}

/// A point on a tube of radius `thickness` wrapped round `spine`, `along` turns
/// down the curve and `round` turns about it.
///
/// The curve is asked where it is and where it is a moment later, which is the
/// direction the tube runs in; the ring is drawn at right angles to that. The
/// two directions across the ring have to come from somewhere, and there is no
/// choice that works for every curve — this one crosses the tangent with
/// whichever upright the curve is leaning on least, which is stable as long as
/// the curve does not cross between the two. Neither curve in this app goes
/// anywhere near it: both lie on a torus, where the way round always outruns the
/// way up.
pub fn tube(spine: impl Fn(f64) -> Vector3, along: f64, round: f64, thickness: f64) -> Sample {
    let here = spine(along);
    let forward = spine(along + NUDGE).minus(here).normalized();
    let upright = if forward.z.abs() < 0.9 {
        Vector3::new(0.0, 0.0, 1.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };
    let side = forward.cross(upright).normalized();
    let over = side.cross(forward).normalized();

    let turn = TAU * round;
    let (out, up) = (turn.cos() * thickness, turn.sin() * thickness);
    Sample {
        point: Vector3::new(
            here.x + side.x * out + over.x * up,
            here.y + side.y * out + over.y * up,
            here.z + side.z * out + over.z * up,
        ),
        inside: here,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cylinder along the x axis: a curve nothing can be ambiguous about, so
    /// what the tube does to it is the tube's own doing.
    fn straight(along: f64) -> Vector3 {
        Vector3::new(along, 0.0, 0.0)
    }

    #[test]
    fn a_ring_sits_at_the_thickness_it_was_asked_for() {
        for step in 0..12 {
            let round = step as f64 / 12.0;
            let sample = tube(straight, 0.3, round, 0.25);
            let out = sample.point.minus(sample.inside);
            assert!((out.length() - 0.25).abs() < 1e-9, "{}", out.length());
            // And square to the curve, or the tube leans as it runs.
            assert!(out.x.abs() < 1e-9, "{}", out.x);
        }
    }

    /// The quad count is the grid, and the coarsest grid a surface can be cut
    /// on still has to be one.
    #[test]
    fn a_surface_is_cut_into_a_quad_for_every_patch() {
        let cut = surface(8, 5, |u, v| Sample::about_the_middle(Vector3::new(u, v, 0.0)));
        assert_eq!(cut.len(), 40);
        let flat = surface(1, 1, |u, v| Sample::about_the_middle(Vector3::new(u, v, 0.0)));
        assert_eq!(flat.len(), 6);
    }
}
