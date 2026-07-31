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
use crate::art::motion::Movement;
use crate::art::params::Params;
use crate::art::surface::{surface, tube, Sample};

use super::ascii::{Face, Renderer, Solid, Turning, Vector3};

/// How far a movement at full strength slides the surface, in the units the
/// shapes are built in — where a body's own reach is about one.
///
/// A fifth of it. A drawing can afford more, because a movement runs out over
/// hundreds of cells there and arrives as a slow swell; here it runs out over
/// the width of the body, and much past this the rings are steeper than the
/// surface they are on and a sphere stops being a sphere.
const SWELL: f64 = 0.2;

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
    ///
    /// Every sample goes out through [`slid`] on its way, so a shape says what
    /// it is and nothing about what is being done to it.
    fn quads(self, steps: usize, thickness: f64, movement: &Movement, phase: f64) -> Vec<Face> {
        let moved = |sample| slid(sample, movement, phase);
        match self {
            Self::Sphere => surface(steps, steps / 2, |u, v| {
                let lift = TAU / 2.0 * v;
                let angle = TAU * u;
                moved(Sample::about_the_middle(Vector3::new(
                    lift.sin() * angle.cos(),
                    lift.sin() * angle.sin(),
                    lift.cos(),
                )))
            }),
            Self::Torus => surface(steps, steps / 2, |u, v| {
                let (round, through) = (TAU * u, TAU * v);
                let reach = 1.0 + thickness * through.cos();
                moved(Sample {
                    point: Vector3::new(
                        reach * round.cos(),
                        reach * round.sin(),
                        thickness * through.sin(),
                    ),
                    inside: Vector3::new(round.cos(), round.sin(), 0.0),
                })
            }),
            Self::Cube => cube(steps / 4, &moved),
            // Two turns the short way for every three the long way, which is
            // the knot that reads most clearly at this size.
            Self::Knot => {
                surface(steps * 3, steps / 3, |u, v| moved(tube(knot, u, v, thickness)))
            }
        }
    }

    /// The furthest the surface gets from the middle before anything moves it.
    ///
    /// Declared rather than measured. A shape that is rebuilt every frame has to
    /// be framed for all of them at once, or the body breathes in and out of the
    /// frame as the movement travels over it — see [`Solid::from_quads_reaching`].
    fn reach(self, thickness: f64) -> f64 {
        match self {
            Self::Sphere => 1.0,
            Self::Torus => 1.0 + thickness,
            // A corner, which is the furthest a cube gets from its middle.
            Self::Cube => 3.0_f64.sqrt(),
            // The curve swings out to three times its scale where its two turns
            // agree, and the tube stands off it by its thickness everywhere.
            Self::Knot => KNOT_SCALE * 3.0 + thickness,
        }
    }

    fn solid(self, steps: usize, thickness: f64, movement: &Movement, phase: f64) -> Solid {
        let faces = self.quads(steps, thickness, movement, phase);
        if movement.moves() {
            Solid::from_quads_reaching(faces, self.reach(thickness) + SWELL * movement.amount())
        } else {
            // Measured, which is tighter: a shape that is only turned is the same
            // shape every frame, so there is nothing to leave room for.
            Solid::from_quads(faces)
        }
    }
}

/// A sample slid out along its own outward direction, as far as `movement` says
/// at that place.
///
/// Out is away from the point on the shape's own middle that the surface hangs
/// off, which is the one thing every sample here already knows how to say. So a
/// sphere swells, a tube fattens and a flat side lifts, all from this one line,
/// and no shape has to be told about movement at all.
fn slid(sample: Sample, movement: &Movement, phase: f64) -> Sample {
    let point = sample.point;
    let out = point.minus(sample.inside);
    let reach = out.length();
    // Where the surface meets its own middle there is no out to slide along —
    // the pole of a sphere cut as a fan is the whole ring at once — and leaving
    // it be is the only answer that does not tear the surface open.
    if reach < 1e-9 {
        return sample;
    }
    let push = SWELL * movement.at([point.x, point.y, point.z], phase) / reach;
    Sample {
        point: Vector3::new(
            point.x + out.x * push,
            point.y + out.y * push,
            point.z + out.z * push,
        ),
        inside: sample.inside,
    }
}

/// How far the knot's curve is scaled, so that it sits at about the reach of the
/// other shapes.
const KNOT_SCALE: f64 = 0.62;

/// The curve the knot's tube is wrapped around: three turns one way while it
/// makes two the other. `along` is a whole turn of the curve over nought to one.
fn knot(along: f64) -> Vector3 {
    let angle = TAU * along;
    let reach = KNOT_SCALE * (2.0 + (3.0 * angle).cos());
    Vector3::new(
        reach * (2.0 * angle).cos(),
        reach * (2.0 * angle).sin(),
        KNOT_SCALE * (3.0 * angle).sin(),
    )
}

/// Six flat sides, each cut into `steps` squares either way.
///
/// The cut buys nothing in shading — a side is one plane and one tone — but the
/// camera converges, so a whole side drawn as one quad has its perspective
/// worked out at four corners and interpolated flat in between, and its
/// diagonal bows the wrong way.
///
/// A side hangs off the plane through the middle it runs parallel to, rather
/// than off the middle point: out of a cube is straight out of the side you are
/// standing on, and a square in the corner of one is well round from the middle
/// without facing any differently for it.
fn cube(steps: usize, moved: &impl Fn(Sample) -> Sample) -> Vec<Face> {
    let steps = steps.max(1);
    // Each side as the axis it faces along, and the two directions across it.
    let sides = [
        (Vector3::new(0.0, 0.0, 1.0), Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 1.0, 0.0)),
        (Vector3::new(0.0, 0.0, -1.0), Vector3::new(0.0, 1.0, 0.0), Vector3::new(1.0, 0.0, 0.0)),
        (Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 1.0, 0.0), Vector3::new(0.0, 0.0, 1.0)),
        (Vector3::new(-1.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 1.0), Vector3::new(0.0, 1.0, 0.0)),
        (Vector3::new(0.0, 1.0, 0.0), Vector3::new(0.0, 0.0, 1.0), Vector3::new(1.0, 0.0, 0.0)),
        (Vector3::new(0.0, -1.0, 0.0), Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 1.0)),
    ];

    sides
        .into_iter()
        .flat_map(|(out, across, down)| {
            surface(steps, steps, |u, v| {
                let (s, t) = (2.0 * u - 1.0, 2.0 * v - 1.0);
                let inside = Vector3::new(
                    across.x * s + down.x * t,
                    across.y * s + down.y * t,
                    across.z * s + down.z * t,
                );
                moved(Sample {
                    point: Vector3::new(inside.x + out.x, inside.y + out.y, inside.z + out.z),
                    inside,
                })
            })
        })
        .collect()
}

pub fn build(params: &Params) -> Result<Generator, String> {
    let shape = Shape::named(params.string("shape").unwrap_or("torus"))?;
    let steps = params.usize("steps", 64)?.clamp(8, 256);
    let thickness = params.f64("thickness", 0.42)?.clamp(0.02, 1.0);
    let movement = Movement::from_params(params)?;

    let mut renderer = Renderer::new(shape.solid(steps, thickness, &movement, 0.0));
    renderer.yaw = params.f64("yaw", 0.0)?.to_radians();
    renderer.pitch = params.f64("pitch", 26.0)?.to_radians();
    renderer.zoom = params.f64("zoom", 0.92)?;

    let turning = Turning::new(
        renderer,
        params.period()?,
        params.f64("turns", 2.0)?,
        params.is_set("still"),
    );
    // Rebuilding is a whole surface a frame, so it is only taken on where there
    // is something to rebuild for.
    Ok(Generator::Glyph(Box::new(if movement.moves() {
        turning.rebuilding(Box::new(move |phase| {
            shape.solid(steps, thickness, &movement, phase)
        }))
    } else {
        turning
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::canvas::SPACE;
    use crate::art::motion::Motion;

    const SHAPES: [Shape; 4] = [Shape::Sphere, Shape::Torus, Shape::Cube, Shape::Knot];

    /// The shape as it is cut when nobody has asked for anything to move it,
    /// which is what most of these tests are about.
    fn held() -> Movement {
        Movement::new(Motion::None, 0.0, 3)
    }

    fn drawn(shape: Shape) -> usize {
        let renderer = Renderer::new(shape.solid(48, 0.42, &held(), 0.0));
        let canvas = Turning::new(renderer, 5.0, 2.0, false).canvas(90, 30, 0.7);
        canvas.glyphs.iter().filter(|&&glyph| glyph != SPACE).count()
    }

    /// Every shape has to arrive as a body rather than as a scattering, at an
    /// angle no shape is symmetric about.
    #[test]
    fn every_shape_fills_a_good_part_of_the_frame() {
        for shape in SHAPES {
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
        for shape in SHAPES {
            for face in shape.quads(32, 0.3, &held(), 0.0) {
                let middle = face.middle();
                let out = middle.minus(spine(shape, middle)).normalized();
                let agreement = face.normal().dot(out);
                assert!(agreement > 0.8, "a {shape:?} face points {agreement} of the way out");
            }
        }
    }

    /// And still faces out while it is moving, which is the harder half.
    ///
    /// Not to within a fifth of the way, as above: a surface with rings running
    /// over it is genuinely steep, and a quad on the side of one leans a long way
    /// off radial. What must not happen is a quad turning right over, which is
    /// what a normal taken from a formula rather than from the moved corners
    /// would do.
    #[test]
    fn every_shape_still_faces_outwards_while_it_moves() {
        for motion in [Motion::Ripple, Motion::Breathe, Motion::Drift] {
            let movement = Movement::new(motion, 0.5, 3);
            for shape in SHAPES {
                for phase in [0.13, 0.4, 0.77] {
                    for face in shape.quads(32, 0.3, &movement, phase) {
                        let middle = face.middle();
                        let out = middle.minus(spine(shape, middle)).normalized();
                        let agreement = face.normal().dot(out);
                        assert!(agreement > 0.0, "a {shape:?} under {motion:?} points {agreement}");
                    }
                }
            }
        }
    }

    /// The frame is cut once for every shape the movement will ever make, so
    /// nothing may reach past what was declared — a body that outgrew its frame
    /// would be clipped on one side of the loop and adrift in it on the other.
    #[test]
    fn nothing_ever_reaches_past_what_the_frame_was_cut_for() {
        for motion in [Motion::Ripple, Motion::Breathe, Motion::Drift] {
            let movement = Movement::new(motion, 1.0, 3);
            for shape in SHAPES {
                let allowed = shape.reach(0.3) + SWELL * movement.amount();
                for step in 0..8 {
                    let phase = step as f64 / 8.0;
                    for face in shape.quads(24, 0.3, &movement, phase) {
                        for corner in face.corners() {
                            let reach = corner.length();
                            assert!(reach <= allowed + 1e-9, "{shape:?} reached {reach} past {allowed}");
                        }
                    }
                }
            }
        }
    }

    /// A movement has to come back round, and to have gone somewhere on the way.
    ///
    /// A quarter of the way, not half: a ripple reads the distance out from the
    /// middle, and every point of a sphere is the same distance out, so a sphere
    /// halfway round is a sphere back at its own size. That is the ripple doing
    /// its job, not the movement failing to move.
    #[test]
    fn a_period_leaves_every_shape_where_it_found_it() {
        let movement = Movement::new(Motion::Ripple, 0.6, 3);
        for shape in SHAPES {
            let at = |phase| {
                shape
                    .quads(24, 0.3, &movement, phase)
                    .into_iter()
                    .map(|face| face.middle())
                    .collect::<Vec<_>>()
            };
            let (start, round, quarter) = (at(0.0), at(1.0), at(0.25));
            assert!(
                start.iter().zip(&round).all(|(one, other)| gap(*one, *other) < 1e-9),
                "{shape:?} does not close its loop"
            );
            let moved = start.iter().zip(&quarter).filter(|(one, other)| gap(**one, **other) > 1e-3);
            assert!(moved.count() > start.len() / 2, "{shape:?} barely moves");
        }
    }

    /// The tube keeps its thickness all the way round: a frame that collapsed
    /// where the curve turns hardest would pinch the knot flat there.
    #[test]
    fn the_knot_keeps_its_thickness() {
        for face in Shape::Knot.quads(48, 0.3, &held(), 0.0) {
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
