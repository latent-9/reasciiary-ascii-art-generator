//! A picture turned back into characters.
//!
//! The exact inverse of [`super::paint`], which turns characters into a picture,
//! and the two are the ends of everything this app does: a tool either lays
//! marks out on a grid or it draws, and one that draws needs this to join the
//! rest of the pipeline.
//!
//! A cell is not one value. It is a patch, and whether the light in it lies
//! along a diagonal or across the top is most of what tells an eye where the
//! edges are — so the picture arrives already sampled at [`CELL_PIXELS`] a cell
//! and every cell is matched by its whole patch, the way the 3D lift's cells
//! are. That is the difference between a picture read back as glyphs and one
//! averaged into a ramp, and at this size it is not a small one.

use image::RgbaImage;

use super::canvas::{AsciiCanvas, AsciiColor, AsciiRamp};
use super::glyphs::{ALPHABET, CELL_PIXELS, CELL_PIXELS_TALL, CELL_PIXELS_WIDE};
use super::params::Params;

/// Below this a cell is background rather than a very faint mark.
///
/// The matcher has no space in it — whether a cell is background is not a
/// question about which mark fits best, and asking it that way is how the dark
/// half of a photograph turns into a field of commas.
///
/// It sits this high because a mark is painted at full strength whatever tone
/// it stands for: the only grey in the output is how much of a cell the glyph
/// covers. A near-black that is not quite black is honestly a faint mark, and a
/// faint mark on paper is brighter than the tone it came from — so the
/// near-blacks come back as a haze over everything the picture meant to leave
/// dark.
const BACKGROUND: f32 = 0.09;

/// How a cell's patch of light becomes a character.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Marks {
    /// By what the patch looks like, which traces edges.
    Matched,
    /// By how much light it holds and nothing else, against one of the ordered
    /// ramps. Coarser, and the right answer for a picture that is already flat
    /// artwork rather than a photograph.
    Graded(AsciiRamp),
}

impl Marks {
    pub fn named(name: &str) -> Result<Self, String> {
        if name == "match" {
            return Ok(Self::Matched);
        }
        AsciiRamp::named(name).map(Self::Graded).map_err(|_| {
            format!("`{name}` is not a set of marks — try match, shades, detailed or ink")
        })
    }

    fn byte(self, cell: &[f32; CELL_PIXELS], light: f32) -> u8 {
        match self {
            // Which ramp the matcher falls back to on a cell with no shape in
            // it at all. A picture read this way is mostly edges, so it is
            // asked rarely and the short one is enough.
            Self::Matched => ALPHABET.nearest(cell, false, AsciiRamp::Shades),
            Self::Graded(ramp) => AsciiRamp::byte_for_intensity(light as f64, ramp.bytes()),
        }
    }
}

/// How much light a pixel is taken to carry: which end of the picture is the
/// subject, and how far the rest are pushed apart.
///
/// Separate from [`Reader`] because it is asked twice for different reasons. A
/// tool reading a picture back as marks wants it, and so does the lift, where
/// the same number is a height rather than a shade — but "which end is the ink"
/// and "how hard" are the same two questions either way, so `--invert` and
/// `--contrast` mean one thing across the app rather than one thing each.
#[derive(Clone, Copy)]
pub struct Tones {
    pub inverted: bool,
    /// How far the tones are pushed apart around the middle. One leaves them
    /// alone.
    pub contrast: f32,
}

impl Tones {
    /// The source as it stands, which is what one nobody has said anything
    /// about gets.
    pub const PLAIN: Self = Self { inverted: false, contrast: 1.0 };

    pub fn from_params(params: &Params) -> Result<Self, String> {
        Ok(Self {
            inverted: params.is_set("invert"),
            contrast: params.f64("contrast", 1.0)?.clamp(0.1, 6.0) as f32,
        })
    }

    /// Both adjustments applied to a level that is already a level, which is
    /// what a drawing hands over — its ink has been counted before anything
    /// here is asked.
    pub fn level(self, level: f32) -> f32 {
        let level = if self.inverted { 1.0 - level } else { level };
        // Around the middle, so raising it opens the picture up rather than
        // washing it out.
        (0.5 + (level - 0.5) * self.contrast).clamp(0.0, 1.0)
    }

    /// How much light a pixel carries, with those two applied.
    pub fn light(self, pixel: &[u8; 4]) -> f32 {
        let [red, green, blue, alpha] = pixel.map(|channel| channel as f32 / 255.0);
        // What the eye weighs each channel at, which is not what a mean does.
        self.level(0.2126 * red + 0.7152 * green + 0.0722 * blue) * alpha
    }
}

/// The choices a picture is read under.
///
/// Both tools that draw take them from the same four flags, so `--marks`,
/// `--color`, `--invert` and `--contrast` mean one thing across the app rather
/// than one thing each.
pub struct Reader {
    pub marks: Marks,
    pub colored: bool,
    pub tones: Tones,
}

impl Reader {
    pub fn from_params(params: &Params) -> Result<Self, String> {
        Ok(Self {
            marks: Marks::named(params.string("marks").unwrap_or("match"))?,
            colored: params.is_set("color"),
            tones: Tones::from_params(params)?,
        })
    }

    /// A picture sampled at [`CELL_PIXELS`] a cell, as its own grid.
    pub fn canvas(&self, fine: &RgbaImage) -> AsciiCanvas {
        let (columns, rows) = cells(fine);
        let mut canvas = AsciiCanvas::new(columns, rows, self.colored);
        self.draw_into(&mut canvas, fine, (0, 0));
        canvas
    }

    /// The same, onto part of a grid that is already there. `at` is the cell the
    /// top left of the picture lands on, which is how a picture that does not
    /// fill the grid is centred on it.
    pub fn draw_into(&self, canvas: &mut AsciiCanvas, fine: &RgbaImage, at: (usize, usize)) {
        let (columns, rows) = cells(fine);
        for row in 0..rows {
            for column in 0..columns {
                let mut cell = [0.0_f32; CELL_PIXELS];
                let mut tint = [0.0_f32; 3];
                for y in 0..CELL_PIXELS_TALL {
                    for x in 0..CELL_PIXELS_WIDE {
                        let pixel = fine.get_pixel(
                            (column * CELL_PIXELS_WIDE + x) as u32,
                            (row * CELL_PIXELS_TALL + y) as u32,
                        );
                        cell[y * CELL_PIXELS_WIDE + x] = self.tones.light(&pixel.0);
                        for (channel, held) in tint.iter_mut().enumerate() {
                            *held += pixel.0[channel] as f32 / 255.0;
                        }
                    }
                }

                let light = cell.iter().sum::<f32>() / CELL_PIXELS as f32;
                if light <= BACKGROUND {
                    continue;
                }
                let color = self.colored.then(|| {
                    let mean = tint.map(|channel| channel as f64 / CELL_PIXELS as f64);
                    AsciiColor::from_unit(mean[0], mean[1], mean[2])
                });
                canvas.set(at.0 + column, at.1 + row, self.marks.byte(&cell, light), color);
            }
        }
    }
}

/// How many whole cells a fine raster covers. A part of a cell is not one, and
/// reading it as one would sample past the edge of the picture.
fn cells(fine: &RgbaImage) -> (usize, usize) {
    (
        fine.width() as usize / CELL_PIXELS_WIDE,
        fine.height() as usize / CELL_PIXELS_TALL,
    )
}

/// The raster size a grid of this many cells is read from.
pub fn fine_size(columns: usize, rows: usize) -> (u32, u32) {
    (
        (columns * CELL_PIXELS_WIDE) as u32,
        (rows * CELL_PIXELS_TALL) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::canvas::{ink_coverage, SPACE};
    use image::Rgba;

    /// Light in, marks out — and dark in, nothing out, which is the part a
    /// matcher with no space in it cannot answer for itself.
    #[test]
    fn a_dark_picture_is_left_blank_and_a_light_one_is_not() {
        let reader = Reader {
            marks: Marks::Matched,
            colored: false,
            tones: Tones::PLAIN,
        };
        let (wide, tall) = fine_size(8, 4);

        let dark = RgbaImage::from_pixel(wide, tall, Rgba([0, 0, 0, 255]));
        let canvas = reader.canvas(&dark);
        assert_eq!(canvas.columns, 8);
        assert_eq!(canvas.rows, 4);
        assert!(canvas.glyphs.iter().all(|&glyph| glyph == SPACE));

        let light = RgbaImage::from_pixel(wide, tall, Rgba([255, 255, 255, 255]));
        assert!(reader
            .canvas(&light)
            .glyphs
            .iter()
            .all(|&glyph| ink_coverage(glyph as char) > 0.5));
    }

    /// Inverting is what makes a drawing on white paper readable, and it has to
    /// take the background with it: the blank half of the page is what becomes
    /// the ink.
    #[test]
    fn inverting_swaps_which_half_of_a_picture_is_background() {
        let reader = Reader {
            marks: Marks::Matched,
            colored: false,
            tones: Tones { inverted: true, contrast: 1.0 },
        };
        let (wide, tall) = fine_size(8, 4);
        let paper = RgbaImage::from_pixel(wide, tall, Rgba([255, 255, 255, 255]));
        assert!(reader
            .canvas(&paper)
            .glyphs
            .iter()
            .all(|&glyph| glyph == SPACE));
    }
}
