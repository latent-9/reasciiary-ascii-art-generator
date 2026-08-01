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
//!
//! A mark is written down where it is made and drawn later, in bands. The
//! exporter already spreads frames across the cores, so a written file was never
//! the problem; a preview is one frame, and one frame drawn on one thread is
//! what the window waits on between a drag and a picture. Split by rows there is
//! nothing to share and nothing to lock — a band owns its own pixels outright —
//! and because a band draws the marks that reach it in the order they were made,
//! the picture is the picture that would have been drawn in one pass. The cost
//! of that is holding a frame's marks in memory until it is asked for.

use image::RgbaImage;
use rayon::prelude::*;

use super::generators::ascii::Vector3;

/// The smallest a dot is drawn, in pixels of radius.
///
/// Below this a dot falls between sample centres and comes and goes between
/// frames, which reads as a field of dust flickering rather than receding. It is
/// held at this size and dimmed by however much of it that is not — the same
/// bargain [`crate::art::generators::paper`] strikes for a hairline, and the
/// reason a drift of far-off particles fades out instead of sparkling.
const FINEST_DOT: f64 = 0.4;

/// How many rows of the picture one band holds.
///
/// Small enough that a picture is cut into more bands than the machine has cores
/// to draw them with. The work is nowhere near even — a band with the horizon
/// across it is a hundred times the band above it, which is empty sky — so the
/// way to keep every core busy to the end is to leave spare bands for whichever
/// core finishes first to take.
const BAND: usize = 32;

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

/// A mark as it will be drawn: on the picture, sized, and waiting for a band.
#[derive(Clone, Copy)]
enum Mark {
    Face { corners: [Plotted; 3], tint: [f32; 3] },
    Dot { middle: Plotted, drawn: f64, tint: [f32; 3], alpha: f64 },
}

pub struct Raster {
    width: usize,
    height: usize,
    /// How far across the picture a thing one unit wide at one unit off would
    /// land, in pixels.
    focal: f64,
    /// How near the eye a point may come before it is cut away, in world units.
    near: f64,
    paper: [f32; 3],
    /// Every mark of the frame, in the order it was made.
    marks: Vec<Mark>,
    /// Which of those reach each band, as places in `marks`. A mark lying across
    /// a seam is in both bands, and an index is only ever added after the ones
    /// before it, so a band draws in the order the frame was drawn in.
    bands: Vec<Vec<u32>>,
}

impl Raster {
    /// A picture of that many pixels, filled with `paper` and empty to the
    /// horizon. `field` is how much the eye takes in from top to bottom, in
    /// radians.
    ///
    /// `near` is how close a point may come before it is cut away, and it is
    /// asked for rather than assumed because it is the one thing here that has
    /// to be said in the tool's own units: the field and the focal length are a
    /// question about the picture, but how near is too near is a question about
    /// the world, and a raster is handed points without being told what a unit of
    /// one is. Something has to answer it — a point level with the eye divides by
    /// nothing, and one behind it divides by a negative and lands upside down on
    /// the wrong side of the picture, which is a plane folding through the
    /// horizon rather than running to it. A small part of however far off the
    /// subject stands is the usual answer.
    pub fn new(width: u32, height: u32, field: f64, near: f64, paper: [f32; 3]) -> Self {
        let (width, height) = (width.max(1) as usize, height.max(1) as usize);
        // The same lens Processing's default camera uses, written as what it
        // means rather than as the eye distance it is usually given as.
        let focal = (height as f64 / 2.0) / (field / 2.0).tan().max(1e-6);
        Self {
            width,
            height,
            focal,
            near: near.max(f64::MIN_POSITIVE),
            paper,
            marks: Vec::new(),
            bands: vec![Vec::new(); height.div_ceil(BAND)],
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
        let kept = clipped(corners, self.near);
        // A polygon of four is the most a triangle can be cut into, and a fan
        // off its first corner covers it.
        for step in 1..kept.len().saturating_sub(1) {
            let fan = [kept[0], kept[step], kept[step + 1]];
            let corners = fan.map(|corner| self.plot(corner));
            let rows = bounds(corners.map(|corner| corner.y), self.height);
            self.record(Mark::Face { corners, tint }, rows);
        }
    }

    /// A round dot of `radius` world units, standing at `at`.
    ///
    /// Its size is the perspective divide and nothing else, so a particle far
    /// down the surface is small for the reason it looks small.
    pub fn dot(&mut self, at: Seen, radius: f64, tint: [f32; 3], alpha: f64) {
        if at.z < self.near || radius <= 0.0 || alpha <= 0.0 {
            return;
        }
        let middle = self.plot(at);
        let wanted = radius * self.focal * middle.nearness;
        let drawn = wanted.max(FINEST_DOT);
        // What the dot was owed against what it is being given, by area — a dot
        // held up to the finest size is faded by exactly what it gained.
        let alpha = alpha * (wanted / drawn).powi(2).min(1.0);

        let rows = span(middle.y, drawn, self.height);
        self.record(Mark::Dot { middle, drawn, tint, alpha }, rows);
    }

    /// Keeps a mark, and tells every band it reaches to expect it.
    fn record(&mut self, mark: Mark, (from, to): (usize, usize)) {
        if from >= to {
            return;
        }
        let at = self.marks.len() as u32;
        self.marks.push(mark);
        for band in &mut self.bands[from / BAND..=(to - 1) / BAND] {
            band.push(at);
        }
    }

    /// Draws every mark that was kept, a band of rows at a time.
    ///
    /// A band draws into paper of its own and lays that down as bytes when it is
    /// finished, so the whole picture is never held twice: what a band works in
    /// is a few hundred kilobytes it has to itself, and what it leaves behind is
    /// the image.
    pub fn into_image(self) -> RgbaImage {
        let mut image = RgbaImage::new(self.width as u32, self.height as u32);
        image
            .par_chunks_mut(BAND * self.width * 4)
            .zip(&self.bands)
            .enumerate()
            .for_each(|(index, (pixels, marks))| {
                let held = pixels.len() / 4;
                let mut band = Band {
                    width: self.width,
                    from: index * BAND,
                    color: vec![self.paper; held],
                    // Inverse distance, so nought is infinitely far and larger
                    // is nearer.
                    nearness: vec![0.0; held],
                };
                for at in marks {
                    match self.marks[*at as usize] {
                        Mark::Face { corners, tint } => band.fill(corners, tint),
                        Mark::Dot { middle, drawn, tint, alpha } => {
                            band.dot(middle, drawn, tint, alpha)
                        }
                    }
                }

                let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
                for (pixel, color) in pixels.chunks_exact_mut(4).zip(&band.color) {
                    pixel.copy_from_slice(&[
                        channel(color[0]),
                        channel(color[1]),
                        channel(color[2]),
                        255,
                    ]);
                }
            });
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

}

/// The rows of a picture one thread draws into, and nothing else — no other
/// band holds a pixel of them, which is what makes the drawing below safe to do
/// on all of them at once.
struct Band {
    width: usize,
    /// The row of the picture this band starts at.
    from: usize,
    color: Vec<[f32; 3]>,
    nearness: Vec<f32>,
}

impl Band {
    /// The row after the last one this band holds.
    fn until(&self) -> usize {
        self.from + self.color.len() / self.width
    }

    fn fill(&mut self, corners: [Plotted; 3], tint: [f32; 3]) {
        let [a, b, c] = corners;
        let area = edge(a, b, c.x, c.y);
        if area.abs() < 1e-9 {
            return;
        }

        // The area is divided by at every pixel of the triangle, so it is turned
        // over once instead. Which side of an edge a pixel falls on is the sign
        // of a product either way, and that is what the test below reads.
        let share = 1.0 / area;
        // The part of each edge that only reads the column, held apart from the
        // part that only reads the row.
        let (across_one, across_two) = ((c.y - b.y) * share, (a.y - c.y) * share);

        let (from_x, to_x) = bounds([a.x, b.x, c.x], self.width);
        let (from_y, to_y) = bounds([a.y, b.y, c.y], self.until());
        for y in from_y.max(self.from)..to_y {
            let py = y as f64 + 0.5;
            let (down_one, down_two) =
                ((c.x - b.x) * (py - b.y) * share, (a.x - c.x) * (py - c.y) * share);
            let row = (y - self.from) * self.width;
            for x in from_x..to_x {
                let px = x as f64 + 0.5;
                // Barycentric, as parts of the whole — taking each edge against
                // the area is what lets both windings be tested the same way.
                let one = down_one - across_one * (px - b.x);
                let two = down_two - across_two * (px - c.x);
                let three = 1.0 - one - two;
                if one < 0.0 || two < 0.0 || three < 0.0 {
                    continue;
                }

                let nearness =
                    (one * a.nearness + two * b.nearness + three * c.nearness) as f32;
                let at = row + x;
                if nearness <= self.nearness[at] {
                    continue;
                }
                self.nearness[at] = nearness;
                self.color[at] = tint;
            }
        }
    }

    fn dot(&mut self, middle: Plotted, drawn: f64, tint: [f32; 3], alpha: f64) {
        // One pixel of soft edge, which is the whole of the anti-aliasing a dot
        // this small can carry. Inside the core it is wholly covered and outside
        // the rim it is not covered at all, so the distance is only worth taking
        // a root of between the two — which for any dot larger than a speck is a
        // ring of pixels around a disc of them that never needed one.
        let (core, rim) = ((drawn - 0.5).max(0.0), drawn + 0.5);
        let (inside, outside) = (core * core, rim * rim);

        let nearness = middle.nearness as f32;
        let (from_x, to_x) = span(middle.x, drawn, self.width);
        let (from_y, to_y) = span(middle.y, drawn, self.until());
        for y in from_y.max(self.from)..to_y {
            let down = (y as f64 + 0.5 - middle.y).powi(2);
            let row = (y - self.from) * self.width;
            for x in from_x..to_x {
                let away = (x as f64 + 0.5 - middle.x).powi(2) + down;
                if away >= outside {
                    continue;
                }
                let at = row + x;
                if nearness <= self.nearness[at] {
                    continue;
                }
                let covered = if away <= inside { alpha } else { (rim - away.sqrt()) * alpha };
                let covered = covered as f32;
                let color = &mut self.color[at];
                for (channel, ink) in color.iter_mut().zip(tint) {
                    *channel = *channel * (1.0 - covered) + ink * covered;
                }
                // Only the solid middle of a dot stands in front of anything.
                // Letting its soft rim write depth would have every dot punch a
                // hole a pixel wider than itself through the dots behind it.
                if covered >= 0.5 {
                    self.nearness[at] = nearness;
                }
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
    reach(low, high, limit)
}

/// The same for a dot, which knows its own reach.
fn span(middle: f64, radius: f64, limit: usize) -> (usize, usize) {
    reach(middle - radius - 1.0, middle + radius + 1.0, limit)
}

/// A stretch of the picture, as whole pixels, or nothing at all.
///
/// Nothing rather than the nearest pixel to it: a mark away off the side of the
/// picture would otherwise be handed the first column and drawn against it, row
/// after row, for a triangle that was never going to land.
fn reach(low: f64, high: f64, limit: usize) -> (usize, usize) {
    let edge = limit as f64;
    if !low.is_finite() || !high.is_finite() || high < 0.0 || low >= edge {
        return (0, 0);
    }
    (
        low.floor().max(0.0) as usize,
        (high.ceil().clamp(0.0, edge) as usize + 1).min(limit),
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

    /// Small in pixels, and looking at a world measured in tens of units.
    const NEAR: f64 = 1.0;

    fn raster() -> Raster {
        Raster::new(64, 64, FIELD, NEAR, BLACK)
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

    /// A mark lying across the seam between two bands is one mark, drawn whole
    /// by the two threads that hold its halves.
    #[test]
    fn a_dot_across_a_seam_comes_out_in_one_piece() {
        let side = BAND as u32 * 2;
        let mut raster = Raster::new(side, side, FIELD, NEAR, BLACK);
        // Level with the eye, so it lands on the middle row — which is where one
        // band ends and the next begins.
        raster.dot(Vector3::new(0.0, 0.0, 20.0), 2.0, WHITE, 1.0);

        let image = raster.into_image();
        let lit = |y: u32| (0..side).filter(|x| image.get_pixel(*x, y).0[0] > 0).count();
        let (above, below) = (lit(BAND as u32 - 1), lit(BAND as u32));
        assert!(above > 0, "nothing was drawn above the seam");
        // The two rows either side of the seam are the same distance from the
        // middle of the dot, so they carry the same width of it.
        assert_eq!(above, below);
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

    /// Asking for more pixels is asking for the same picture drawn larger, not
    /// for a different world.
    ///
    /// The near plane is the one number here that could be mistaken for a length
    /// on the picture, and reading it as one would have the same scene lose more
    /// of itself the larger it was asked for — a preview that worked and an
    /// export that came out empty.
    #[test]
    fn the_near_plane_is_a_distance_in_the_world_not_on_the_picture() {
        let lit = |side: u32| {
            let mut raster = Raster::new(side, side, FIELD, NEAR, BLACK);
            for face in wall(5.0) {
                raster.triangle(face, WHITE);
            }
            let image = raster.into_image();
            let middle = side / 2;
            image.get_pixel(middle, middle).0[0]
        };
        assert_eq!(lit(64), 255);
        assert_eq!(lit(512), 255);
    }

    #[test]
    fn the_paper_is_what_nothing_was_drawn_on() {
        let raster = Raster::new(8, 8, FIELD, NEAR, [0.2, 0.4, 0.6]);
        assert_eq!(raster.into_image().get_pixel(4, 4).0, [51, 102, 153, 255]);
    }
}
