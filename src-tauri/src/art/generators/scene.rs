//! A primitive turned from a formula.
//!
//! Port of `asciiary/AsciiScene.swift`.
//!
//! Every surface here is a function of two parameters — go round one way, go
//! round the other, and the quad between four neighbouring samples is a face.
//! That is the whole tool: the camera, the light rig, the raster and the
//! alphabet all belong to [`super::ascii`] and are handed a list of quads
//! without being told where the quads came from.

use std::f64::consts::TAU;

use crate::art::generator::Generator;
#[cfg(test)]
use crate::art::generator::GlyphGenerator;
use crate::art::params::Params;
use crate::art::surface::{surface, tube, Sample};

use super::ascii::{Face, Renderer, Solid, Spinning, Vector3};

/// What the tool can draw.
///
/// Round things, mostly, and on purpose: a turn is what says a picture is of a
/// solid, and a shape with a silhouette that changes as it turns says it
/// loudest. A cube is here because the opposite is worth having too — flat
/// faces, hard edges, and three tones that step rather than grade.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    Sphere,
    Torus,
    Cube,
    /// A tube following a knot that lies on a torus, which crosses over itself
    /// and so is the one shape here that can hide behind itself.
    Knot,
}

impl Shape {
    fn named(name: &str) -> Result<Self, String> {
        match name {
            "sphere" => Ok(Self::Sphere),
            "torus" => Ok(Self::Torus),
            "cube" => Ok(Self::Cube),
            "knot" => Ok(Self::Knot),
            other => Err(format!(
                "`{other}` is not a shape — try sphere, torus, cube or knot"
            )),
        }
    }

    /// How finely the surface is cut, given a request of `steps` around.
    ///
    /// A quad is shaded flat, so tessellation is what tone the surface has to
    /// grade through, and it is cheap: the raster is a sub-cell grid a couple
    /// of hundred wide, and a quad smaller than one of its samples is drawn by
    /// its centre alone.
    fn quads(self, steps: usize, thickness: f64) -> Vec<Face> {
        match self {
            Self::Sphere => surface(steps, steps / 2, |u, v| {
                let lift = TAU / 2.0 * v;
                let angle = TAU * u;
                Sample::about_the_middle(Vector3::new(
                    lift.sin() * angle.cos(),
                    lift.sin() * angle.sin(),
                    lift.cos(),
                ))
            }),
            Self::Torus => surface(steps, steps / 2, |u, v| {
                let (round, through) = (TAU * u, TAU * v);
                let reach = 1.0 + thickness * through.cos();
                Sample {
                    point: Vector3::new(
                        reach * round.cos(),
                        reach * round.sin(),
                        thickness * through.sin(),
                    ),
                    inside: Vector3::new(round.cos(), round.sin(), 0.0),
                }
            }),
            Self::Cube => cube(steps / 4),
            // Two turns the short way for every three the long way, which is
            // the knot that reads most clearly at this size.
            Self::Knot => surface(steps * 3, steps / 3, |u, v| tube(knot, u, v, thickness)),
        }
    }
}

/// The curve the knot's tube is wrapped around: three turns one way while it
/// makes two the other, kept at about the reach of the other shapes. `along` is
/// a whole turn of the curve over nought to one.
fn knot(along: f64) -> Vector3 {
    let angle = TAU * along;
    let reach = 0.62 * (2.0 + (3.0 * angle).cos());
    Vector3::new(
        reach * (2.0 * angle).cos(),
        reach * (2.0 * angle).sin(),
        0.62 * (3.0 * angle).sin(),
    )
}

/// Six flat sides, each cut into `steps` squares either way.
///
/// The cut buys nothing in shading — a side is one plane and one tone — but the
/// camera converges, so a whole side drawn as one quad has its perspective
/// worked out at four corners and interpolated flat in between, and its
/// diagonal bows the wrong way.
fn cube(steps: usize) -> Vec<Face> {
    let steps = steps.max(1);
    let mut faces = Vec::with_capacity(6 * steps * steps);
    // Each side as the axis it faces along, and the two directions across it.
    let sides = [
        (Vector3::new(0.0, 0.0, 1.0), Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 1.0, 0.0)),
        (Vector3::new(0.0, 0.0, -1.0), Vector3::new(0.0, 1.0, 0.0), Vector3::new(1.0, 0.0, 0.0)),
        (Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 1.0, 0.0), Vector3::new(0.0, 0.0, 1.0)),
        (Vector3::new(-1.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 1.0), Vector3::new(0.0, 1.0, 0.0)),
        (Vector3::new(0.0, 1.0, 0.0), Vector3::new(0.0, 0.0, 1.0), Vector3::new(1.0, 0.0, 0.0)),
        (Vector3::new(0.0, -1.0, 0.0), Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 1.0)),
    ];

    for (out, across, down) in sides {
        for column in 0..steps {
            for row in 0..steps {
                let corner = |dc: usize, dr: usize| {
                    let s = 2.0 * (column + dc) as f64 / steps as f64 - 1.0;
                    let t = 2.0 * (row + dr) as f64 / steps as f64 - 1.0;
                    Vector3::new(
                        out.x + across.x * s + down.x * t,
                        out.y + across.y * s + down.y * t,
                        out.z + across.z * s + down.z * t,
                    )
                };
                faces.push(Face::new(
                    [corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1)],
                    out,
                ));
            }
        }
    }
    faces
}

pub fn build(params: &Params) -> Result<Generator, String> {
    let shape = Shape::named(params.string("shape").unwrap_or("torus"))?;
    let steps = params.usize("steps", 64)?.clamp(8, 256);
    let thickness = params.f64("thickness", 0.42)?.clamp(0.02, 1.0);

    let mut renderer = Renderer::new(Solid::from_quads(shape.quads(steps, thickness)));
    renderer.yaw = params.f64("yaw", 0.0)?.to_radians();
    renderer.pitch = params.f64("pitch", 26.0)?.to_radians();
    renderer.zoom = params.f64("zoom", 0.92)?;

    Ok(Generator::Glyph(Box::new(Spinning::new(
        renderer,
        params.f64("spin", 1.2)?,
        params.is_set("still"),
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::canvas::SPACE;

    fn drawn(shape: Shape) -> usize {
        let renderer = Renderer::new(Solid::from_quads(shape.quads(48, 0.42)));
        let canvas = Spinning::new(renderer, 1.2, false).canvas(90, 30, 0.7);
        canvas.glyphs.iter().filter(|&&glyph| glyph != SPACE).count()
    }

    /// Every shape has to arrive as a body rather than as a scattering, at an
    /// angle no shape is symmetric about.
    #[test]
    fn every_shape_fills_a_good_part_of_the_frame() {
        for shape in [Shape::Sphere, Shape::Torus, Shape::Cube, Shape::Knot] {
            let inked = drawn(shape);
            assert!(inked > 250, "{shape:?} drew only {inked} cells");
        }
    }

    /// The point on the shape's own middle that the surface is wrapped around:
    /// nothing for a sphere or a cube, a ring for a torus, the knot's curve for
    /// the knot. Whichever it is, out is away from it.
    fn spine(shape: Shape, point: Vector3) -> Vector3 {
        match shape {
            Shape::Sphere => Vector3::new(0.0, 0.0, 0.0),
            // A flat side hangs off the plane through the middle it runs
            // parallel to, rather than off the middle point: a quad in the
            // corner of a side is well round from that point without facing any
            // differently for it.
            Shape::Cube => {
                let (across, up, along) = (point.x.abs(), point.y.abs(), point.z.abs());
                if across >= up && across >= along {
                    Vector3::new(0.0, point.y, point.z)
                } else if up >= along {
                    Vector3::new(point.x, 0.0, point.z)
                } else {
                    Vector3::new(point.x, point.y, 0.0)
                }
            }
            Shape::Torus => {
                let flat = (point.x * point.x + point.y * point.y).sqrt().max(1e-9);
                Vector3::new(point.x / flat, point.y / flat, 0.0)
            }
            Shape::Knot => (0..2048)
                .map(|step| knot(step as f64 / 2048.0))
                .min_by(|one, other| gap(point, *one).total_cmp(&gap(point, *other)))
                .unwrap(),
        }
    }

    fn gap(from: Vector3, to: Vector3) -> f64 {
        from.minus(to).length()
    }

    /// A quad's normal has to point out of the surface rather than into it.
    ///
    /// Nothing in a frame gives this away — a face is lit by whichever side of
    /// it the eye can see, so a surface turned inside out shades identically —
    /// which is exactly why it is worth a test. The next thing to want a normal
    /// will not be so forgiving.
    #[test]
    fn every_shape_faces_outwards() {
        for shape in [Shape::Sphere, Shape::Torus, Shape::Cube, Shape::Knot] {
            for face in shape.quads(32, 0.3) {
                let middle = face.middle();
                let out = middle.minus(spine(shape, middle)).normalized();
                let agreement = face.normal().dot(out);
                assert!(agreement > 0.8, "a {shape:?} face points {agreement} of the way out");
            }
        }
    }

    /// The tube keeps its thickness all the way round: a frame that collapsed
    /// where the curve turns hardest would pinch the knot flat there.
    #[test]
    fn the_knot_keeps_its_thickness() {
        for face in Shape::Knot.quads(48, 0.3) {
            let middle = face.middle();
            let reach = gap(middle, spine(Shape::Knot, middle));
            assert!(
                (0.26..0.31).contains(&reach),
                "the tube runs {reach} from the curve it follows"
            );
        }
    }

    #[test]
    fn an_unknown_shape_says_what_there_is() {
        let message = Shape::named("blob").unwrap_err();
        assert!(message.contains("sphere"), "{message}");
    }
}
