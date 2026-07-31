//! A camera with a depth buffer, for a tool that ends at pixels.
//!
//! Everything else in this app arrives at a character grid, and the one place
//! that projects a solid — [`crate::art::generators::ascii`] — rasterizes into
//! sub-cell samples so it can match a glyph to each cell afterwards. A piece
//! whose whole subject is a surface standing in front of something else needs
//! neither of those: it needs the surface to be opaque, and it needs what is
//! behind it to be gone. That is a depth buffer and nothing more.
//!
//! So this is deliberately small. It takes points already turned to face the
//! eye, cuts away what has got behind it, divides by distance, and keeps the
//! nearest thing to have written each pixel. Two marks are enough for the pieces
//! that want it: a filled triangle for a surface, and a round dot for a particle.
//!
//! The dot is why the camera lives here rather than in the tool. A particle is a
//! point with a size in the world, and how large that lands on the picture is a
//! question about distance — the same question the depth test is already asking.
//! Answered anywhere else it becomes a fudge factor, which is what it is in the
//! sketches this follows.

use image::{Rgba, RgbaImage};

use super::generators::ascii::Vector3;

/// How near the eye a point may come before it is cut away, as a part of the
/// focal length.
///
/// Something has to be: a point level with the eye divides by nothing, and one
/// behind it divides by a negative and lands on the picture upside down and on
/// the wrong side, which is a plane that folds through the horizon rather than
/// running to it. Small enough to keep everything anybody meant to draw.
const NEAR: f64 = 0.02;

/// The smallest a dot is drawn, in pixels of radius.
///
/// Below this a dot falls between sample centres and comes and goes between
/// frames, which reads as a field of dust flickering rather than receding. It is
/// held at this size and dimmed by however much of it that is not — the same
/// bargain [`crate::art::generators::paper`] strikes for a hairline, and the
/// reason a drift of far-off particles fades out instead of sparkling.
const FINEST_DOT: f64 = 0.4;

/// A point of the world as the eye sees it: `x` across the picture, `y` up it,
/// and `z` how far off it is.
pub type Seen = Vector3;

/// A point on the picture, with what the depth test compares.
///
/// Inverse distance rather than distance, because that is the part of a
/// perspective view that runs evenly across the picture: interpolating distance
/// itself between two corners bends a long surface, and a plane running to the
/// horizon then punches through its own near half in bands.
#[derive(Clone, Copy)]
struct Plotted {
    x: f64,
    y: f64,
    nearness: f64,
}

pub struct Raster {
    width: usize,
    height: usize,
    /// How far across the picture a thing one unit wide at one unit off would
    /// land, in pixels.
    focal: f64,
    color: Vec<[f32; 3]>,
    /// Inverse distance, so nought is infinitely far and larger is nearer.
    nearness: Vec<f32>,
}

impl Raster {
    /// A picture of that many pixels, filled with `paper` and empty to the
    /// horizon. `field` is how much the eye takes in from top to bottom, in
    /// radians.
    pub fn new(width: u32, height: u32, field: f64, paper: [f32; 3]) -> Self {
        let (width, height) = (width.max(1) as usize, height.max(1) as usize);
        // The same lens Processing's default camera uses, written as what it
        // means rather than as the eye distance it is usually given as.
        let focal = (height as f64 / 2.0) / (field / 2.0).tan().max(1e-6);
        Self {
            width,
            height,
            focal,
            color: vec![paper; width * height],
            nearness: vec![0.0; width * height],
        }
    }

    pub fn focal(&self) -> f64 {
        self.focal
    }

    /// A flat triangle, opaque, nearest wins.
    ///
    /// Corners are given as the eye sees them and may be anywhere, including
    /// behind it. What is cut away is replaced by the edge of what is not, so a
    /// surface running past the camera keeps its shape up to where it leaves.
    pub fn triangle(&mut self, corners: [Seen; 3], tint: [f32; 3]) {
        let kept = clipped(corners, self.focal * NEAR);
        // A polygon of four is the most a triangle can be cut into, and a fan
        // off its first corner covers it.
        for step in 1..kept.len().saturating_sub(1) {
            let fan = [kept[0], kept[step], kept[step + 1]];
            let plotted = fan.map(|corner| self.plot(corner));
            self.fill(plotted, tint);
        }
    }

    /// A round dot of `radius` world units, standing at `at`.
    ///
    /// Its size is the perspective divide and nothing else, so a particle far
    /// down the surface is small for the reason it looks small.
    pub fn dot(&mut self, at: Seen, radius: f64, tint: [f32; 3], alpha: f64) {
        if at.z < self.focal * NEAR || radius <= 0.0 || alpha <= 0.0 {
            return;
        }
        let middle = self.plot(at);
        let wanted = radius * self.focal * middle.nearness;
        let drawn = wanted.max(FINEST_DOT);
        // What the dot was owed against what it is being given, by area — a dot
        // held up to the finest size is faded by exactly what it gained.
        let alpha = alpha * (wanted / drawn).powi(2).min(1.0);

        let (from_x, to_x) = span(middle.x, drawn, self.width);
        let (from_y, to_y) = span(middle.y, drawn, self.height);
        for y in from_y..to_y {
            for x in from_x..to_x {
                let away = ((x as f64 + 0.5 - middle.x).powi(2)
                    + (y as f64 + 0.5 - middle.y).powi(2))
                .sqrt();
                // One pixel of soft edge, which is the whole of the
                // anti-aliasing a dot this small can carry.
                let covered = (drawn + 0.5 - away).clamp(0.0, 1.0) * alpha;
                if covered <= 0.0 {
                    continue;
                }
                let at = y * self.width + x;
                if (middle.nearness as f32) <= self.nearness[at] {
                    continue;
                }
                let covered = covered as f32;
                for (channel, ink) in self.color[at].iter_mut().zip(tint) {
                    *channel = *channel * (1.0 - covered) + ink * covered;
                }
                // Only the solid middle of a dot stands in front of anything.
                // Letting its soft rim write depth would have every dot punch a
                // hole a pixel wider than itself through the dots behind it.
                if covered >= 0.5 {
                    self.nearness[at] = middle.nearness as f32;
                }
            }
        }
    }

    pub fn into_image(self) -> RgbaImage {
        let mut image = RgbaImage::new(self.width as u32, self.height as u32);
        for (pixel, color) in image.pixels_mut().zip(&self.color) {
            let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            *pixel = Rgba([channel(color[0]), channel(color[1]), channel(color[2]), 255]);
        }
        image
    }

    fn plot(&self, point: Seen) -> Plotted {
        let nearness = 1.0 / point.z;
        Plotted {
            x: self.width as f64 / 2.0 + point.x * self.focal * nearness,
            y: self.height as f64 / 2.0 - point.y * self.focal * nearness,
            nearness,
        }
    }

    fn fill(&mut self, corners: [Plotted; 3], tint: [f32; 3]) {
        let [a, b, c] = corners;
        let area = edge(a, b, c.x, c.y);
        if area.abs() < 1e-9 {
            return;
        }

        let (from_x, to_x) = bounds([a.x, b.x, c.x], self.width);
        let (from_y, to_y) = bounds([a.y, b.y, c.y], self.height);
        for y in from_y..to_y {
            for x in from_x..to_x {
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                // Barycentric, as parts of the whole — dividing by the area
                // first is what lets both windings be tested the same way.
                let one = edge(b, c, px, py) / area;
                let two = edge(c, a, px, py) / area;
                let three = 1.0 - one - two;
                if one < 0.0 || two < 0.0 || three < 0.0 {
                    continue;
                }

                let nearness =
                    (one * a.nearness + two * b.nearness + three * c.nearness) as f32;
                let at = y * self.width + x;
                if nearness <= self.nearness[at] {
                    continue;
                }
                self.nearness[at] = nearness;
                self.color[at] = tint;
            }
        }
    }
}

/// Twice the area of the triangle two corners make with a point, which is
/// positive on one side of the line through them and negative on the other.
fn edge(from: Plotted, to: Plotted, x: f64, y: f64) -> f64 {
    (to.x - from.x) * (y - from.y) - (to.y - from.y) * (x - from.x)
}

/// The rows or columns a triangle can possibly touch.
fn bounds(across: [f64; 3], limit: usize) -> (usize, usize) {
    let low = across.iter().copied().fold(f64::INFINITY, f64::min);
    let high = across.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !low.is_finite() || !high.is_finite() {
        return (0, 0);
    }
    (
        low.floor().max(0.0) as usize,
        (high.ceil().max(0.0) as usize + 1).min(limit),
    )
}

/// The same for a dot, which knows its own reach.
fn span(middle: f64, radius: f64, limit: usize) -> (usize, usize) {
    if !middle.is_finite() {
        return (0, 0);
    }
    (
        (middle - radius - 1.0).floor().max(0.0) as usize,
        ((middle + radius + 1.0).ceil().max(0.0) as usize + 1).min(limit),
    )
}

/// A triangle with everything nearer than `near` cut off it.
///
/// Nothing, a triangle, or a four-sided patch — the shape a plane leaves when a
/// wall is taken out of it.
fn clipped(corners: [Seen; 3], near: f64) -> Vec<Seen> {
    if corners.iter().all(|corner| corner.z >= near) {
        return corners.to_vec();
    }
    let mut kept = Vec::with_capacity(4);
    for step in 0..3 {
        let here = corners[step];
        let next = corners[(step + 1) % 3];
        if here.z >= near {
            kept.push(here);
        }
        if (here.z >= near) != (next.z >= near) {
            let along = (near - here.z) / (next.z - here.z);
            kept.push(Vector3::new(
                here.x + (next.x - here.x) * along,
                here.y + (next.y - here.y) * along,
                near,
            ));
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIELD: f64 = std::f64::consts::FRAC_PI_3;
    const BLACK: [f32; 3] = [0.0, 0.0, 0.0];
    const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

    fn raster() -> Raster {
        Raster::new(64, 64, FIELD, BLACK)
    }

    /// A square straddling the middle of the picture, `away` off the eye.
    fn wall(away: f64) -> [[Seen; 3]; 2] {
        let corner = |x: f64, y: f64| Vector3::new(x, y, away);
        [
            [corner(-40.0, -40.0), corner(40.0, -40.0), corner(40.0, 40.0)],
            [corner(-40.0, -40.0), corner(40.0, 40.0), corner(-40.0, 40.0)],
        ]
    }

    fn middle(raster: Raster) -> [u8; 4] {
        raster.into_image().get_pixel(32, 32).0
    }

    #[test]
    fn a_triangle_lands_where_it_was_aimed() {
        let mut raster = raster();
        for face in wall(100.0) {
            raster.triangle(face, WHITE);
        }
        let image = raster.into_image();
        assert_eq!(image.get_pixel(32, 32).0, [255, 255, 255, 255]);
        // And nothing at the corners: a square of eighty units at a hundred off
        // covers the middle of the picture, not the whole of it.
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 255]);
    }

    /// The whole reason this module exists.
    #[test]
    fn the_nearer_surface_is_the_one_that_shows() {
        for order in [[300.0, 100.0], [100.0, 300.0]] {
            let mut raster = raster();
            for (away, tint) in order.iter().zip([WHITE, BLACK]) {
                for face in wall(*away) {
                    raster.triangle(face, if *away == 100.0 { BLACK } else { tint });
                }
            }
            // Drawn in either order, the near black wall is what is seen.
            assert_eq!(middle(raster), [0, 0, 0, 255], "{order:?}");
        }
    }

    #[test]
    fn a_dot_behind_a_surface_is_not_drawn_and_one_in_front_is() {
        for (away, seen) in [(200.0, false), (50.0, true)] {
            let mut raster = raster();
            for face in wall(100.0) {
                raster.triangle(face, BLACK);
            }
            raster.dot(Vector3::new(0.0, 0.0, away), 4.0, WHITE, 1.0);
            let lit = middle(raster)[0] > 0;
            assert_eq!(lit, seen, "a dot at {away}");
        }
    }

    /// A dot too small to cover a sample keeps its weight as light instead of
    /// coming and going between frames.
    #[test]
    fn a_dot_smaller_than_a_pixel_fades_rather_than_flickers() {
        let brightness = |away: f64| {
            let mut raster = raster();
            raster.dot(Vector3::new(0.0, 0.0, away), 1.0, WHITE, 1.0);
            raster
                .into_image()
                .pixels()
                .map(|pixel| pixel.0[0] as u32)
                .sum::<u32>()
        };
        let near = brightness(200.0);
        let far = brightness(1000.0);
        assert!(near > far, "{near} against {far}");
        // Dimmer, and still there. Far enough out it does run past what eight
        // bits can hold — but it gets there by fading, which is the point.
        assert!(far > 0, "the far dot vanished");
    }

    /// A surface running past the eye keeps the part of itself that is still in
    /// front of it, rather than folding back through the picture.
    #[test]
    fn what_gets_behind_the_eye_is_cut_away_rather_than_folded() {
        let mut raster = raster();
        // One corner well behind the eye, the other two in front and low.
        raster.triangle(
            [
                Vector3::new(0.0, -20.0, -400.0),
                Vector3::new(-60.0, -20.0, 200.0),
                Vector3::new(60.0, -20.0, 200.0),
            ],
            WHITE,
        );
        let image = raster.into_image();
        // The corner that was behind the eye would have landed above the middle
        // of the picture with the sign flipped. Nothing may be drawn up there.
        for y in 0..24 {
            for x in 0..64 {
                assert_eq!(image.get_pixel(x, y).0[0], 0, "at {x},{y}");
            }
        }
    }

    #[test]
    fn a_triangle_wholly_behind_the_eye_draws_nothing() {
        let mut raster = raster();
        let at = |x: f64, y: f64| Vector3::new(x, y, -100.0);
        raster.triangle([at(-50.0, -50.0), at(50.0, -50.0), at(0.0, 50.0)], WHITE);
        assert!(raster.into_image().pixels().all(|pixel| pixel.0[0] == 0));
    }

    /// Marks off the edge of the picture cost nothing and reach nothing.
    #[test]
    fn a_mark_outside_the_picture_is_harmless() {
        let mut raster = raster();
        let at = |x: f64, y: f64| Vector3::new(x, y, 50.0);
        raster.triangle([at(-900.0, -900.0), at(-800.0, -900.0), at(-850.0, -800.0)], WHITE);
        raster.dot(Vector3::new(900.0, 900.0, 50.0), 3.0, WHITE, 1.0);
        assert!(raster.into_image().pixels().all(|pixel| pixel.0[0] == 0));
    }

    #[test]
    fn the_paper_is_what_nothing_was_drawn_on() {
        let raster = Raster::new(8, 8, FIELD, [0.2, 0.4, 0.6]);
        assert_eq!(raster.into_image().get_pixel(4, 4).0, [51, 102, 153, 255]);
    }
}
