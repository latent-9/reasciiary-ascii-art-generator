//! A sheet to draw on, in units that owe nothing to the grid.
//!
//! A tool that draws rather than lifts has to answer the same two questions
//! every time: how large is the raster, and where on it does a point go. Both
//! have one answer here. The raster is whatever [`crate::art::read`] wants to
//! read back, and the frame is one unit tall, as many wide as its shape, with
//! nothing in the middle. So a piece is written in the units of the picture —
//! a stroke a two-hundredth of the frame, a figure three quarters of its height
//! — and the same code draws a preview and a poster.

use image::RgbaImage;
use tiny_skia::{Color, FillRule, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Thinner than this and anti-aliasing spends the whole stroke on coverage: the
/// line survives as a grey that comes and goes between frames, which reads as
/// flicker rather than as a fine line.
const HAIRLINE: f32 = 0.9;

pub struct Paper {
    pixmap: Pixmap,
    /// How many world units wide the frame is. It is always one tall.
    across: f64,
}

impl Paper {
    /// A sheet of that many pixels, dark, with the origin in the middle.
    pub fn new(wide: u32, tall: u32) -> Option<Self> {
        let mut pixmap = Pixmap::new(wide.max(1), tall.max(1))?;
        pixmap.fill(Color::BLACK);
        let across = pixmap.width() as f64 / pixmap.height() as f64;
        Some(Self { pixmap, across })
    }

    /// How far the frame runs either side of the middle, twice over.
    pub fn across(&self) -> f64 {
        self.across
    }

    /// A point of the world, in pixels.
    fn onto(&self, point: (f64, f64)) -> (f32, f32) {
        let (wide, tall) = (self.pixmap.width() as f64, self.pixmap.height() as f64);
        (
            ((point.0 / self.across + 0.5) * wide) as f32,
            ((point.1 + 0.5) * tall) as f32,
        )
    }

    /// A run of points, joined. `weight` is a fraction of the frame's height
    /// like everything else here, so a line keeps its weight against the
    /// picture however large the picture was asked for.
    pub fn stroke(&mut self, points: &[(f64, f64)], weight: f64, tint: [f32; 3], alpha: f64) {
        if points.len() < 2 || alpha <= 0.0 {
            return;
        }
        let mut path = PathBuilder::new();
        for (step, point) in points.iter().enumerate() {
            let (x, y) = self.onto(*point);
            if step == 0 {
                path.move_to(x, y);
            } else {
                path.line_to(x, y);
            }
        }
        let Some(path) = path.finish() else {
            return;
        };

        let width = (weight * self.pixmap.height() as f64) as f32;
        let stroke = Stroke {
            width: width.max(HAIRLINE),
            line_cap: LineCap::Round,
            ..Stroke::default()
        };
        let mut paint = Paint { anti_alias: true, ..Paint::default() };
        paint.set_color(
            Color::from_rgba(tint[0], tint[1], tint[2], alpha.clamp(0.0, 1.0) as f32)
                .unwrap_or(Color::WHITE),
        );
        self.pixmap
            .stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    /// The area a run of points encloses, closed and filled.
    ///
    /// A tool that wants a shape read rather than an edge traced needs this
    /// rather than a heavy stroke: the reader matches a cell by the patch of
    /// light in it, so an area comes back as a solid glyph and an outline comes
    /// back as a rule. Three outlines inside one another are a ruled grid; three
    /// areas inside one another are three tones.
    pub fn fill(&mut self, points: &[(f64, f64)], tint: [f32; 3], alpha: f64) {
        if points.len() < 3 || alpha <= 0.0 {
            return;
        }
        let mut path = PathBuilder::new();
        for (step, point) in points.iter().enumerate() {
            let (x, y) = self.onto(*point);
            if step == 0 {
                path.move_to(x, y);
            } else {
                path.line_to(x, y);
            }
        }
        path.close();
        let Some(path) = path.finish() else {
            return;
        };

        let mut paint = Paint { anti_alias: true, ..Paint::default() };
        paint.set_color(
            Color::from_rgba(tint[0], tint[1], tint[2], alpha.clamp(0.0, 1.0) as f32)
                .unwrap_or(Color::WHITE),
        );
        self.pixmap
            .fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }

    /// What the reader reads.
    pub fn picture(&self) -> Option<RgbaImage> {
        RgbaImage::from_raw(
            self.pixmap.width(),
            self.pixmap.height(),
            self.pixmap.data().to_vec(),
        )
    }
}

/// A colour that goes round rather than from one end to another, so nothing is
/// at the end of the palette and nothing is left grey.
pub fn hue(turn: f64) -> [f32; 3] {
    [0.0, 0.33, 0.67].map(|offset| (0.5 + 0.5 * (std::f64::consts::TAU * (turn + offset)).cos()) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The middle of the world is the middle of the sheet, and a point a half
    /// unit down is the bottom of it, whatever shape the sheet is.
    #[test]
    fn the_world_is_a_unit_tall_and_centred() {
        let paper = Paper::new(300, 100).expect("a sheet that size");
        assert!((paper.across() - 3.0).abs() < 1e-9);
        assert_eq!(paper.onto((0.0, 0.0)), (150.0, 50.0));
        assert_eq!(paper.onto((0.0, 0.5)), (150.0, 100.0));
        assert_eq!(paper.onto((1.5, 0.0)), (300.0, 50.0));
    }

    /// A stroke has to land ink, and an invisible one has to cost nothing.
    #[test]
    fn a_stroke_marks_the_sheet_and_a_clear_one_does_not() {
        let lit = |paper: &Paper| {
            paper
                .picture()
                .expect("a picture")
                .pixels()
                .filter(|pixel| pixel.0[0] > 0)
                .count()
        };

        let mut paper = Paper::new(120, 40).expect("a sheet");
        assert_eq!(lit(&paper), 0);
        // A third of the sheet across, two pixels of it thick.
        paper.stroke(&[(-0.5, 0.0), (0.5, 0.0)], 0.05, [1.0; 3], 1.0);
        assert!(lit(&paper) > 60, "{} pixels", lit(&paper));

        let mut clear = Paper::new(120, 40).expect("a sheet");
        clear.stroke(&[(-0.5, 0.0), (0.5, 0.0)], 0.05, [1.0; 3], 0.0);
        assert_eq!(lit(&clear), 0);
    }

    /// A fill covers its area rather than tracing it, which is the whole reason
    /// it is here: the same square as an outline leaves the middle dark.
    #[test]
    fn a_fill_covers_the_inside_and_a_stroke_does_not() {
        let middle = |paper: &Paper| paper.picture().expect("a picture").get_pixel(60, 20).0[0];
        let square = [(-0.2, -0.2), (0.2, -0.2), (0.2, 0.2), (-0.2, 0.2)];

        let mut filled = Paper::new(120, 40).expect("a sheet");
        filled.fill(&square, [1.0; 3], 1.0);
        assert_eq!(middle(&filled), 255);

        let mut traced = Paper::new(120, 40).expect("a sheet");
        traced.stroke(&square, 0.02, [1.0; 3], 1.0);
        assert_eq!(middle(&traced), 0);
    }

    /// The hue is a loop: a whole turn of it is where it started.
    #[test]
    fn the_hue_comes_back_round() {
        for step in 0..8 {
            let turn = step as f64 / 8.0;
            let one = hue(turn);
            let round = hue(turn + 1.0);
            for channel in 0..3 {
                assert!((one[channel] - round[channel]).abs() < 1e-6);
            }
        }
        assert_ne!(hue(0.0), hue(0.4));
    }
}
