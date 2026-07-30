//! Turns a character grid into pixels.
//!
//! Port of `AsciiCanvasPainter` in `asciiary/AsciiCanvas.swift`. The preview and
//! the exporter both go through this, which is the point: a GIF somebody posts
//! should be the frame they were looking at, not a second implementation that
//! drifted.

use fontdue::{Font, Metrics};
use image::{Rgba, RgbaImage};

use super::canvas::{AsciiCanvas, AsciiColor, SPACE};

/// Every ramp is ASCII by construction, so one small table covers everything
/// that will ever be asked for.
const GLYPH_COUNT: usize = 128;

pub struct Painter {
    pub cell_width: f32,
    pub cell_height: f32,
    ascent: f32,
    glyphs: Vec<Option<(Metrics, Vec<u8>)>>,
}

impl Painter {
    /// `size` is already in device pixels — the caller multiplies by the export
    /// scale before constructing, so nothing downstream has to think about it.
    ///
    /// Cell size is measured from the digit advance rather than the font's
    /// maximum advance, which on many monospace faces is set by a wide symbol
    /// and would space a render out.
    pub fn new(font: &Font, size: f32) -> Self {
        let advance = font.metrics('0', size).advance_width;
        let line = font
            .horizontal_line_metrics(size)
            .expect("monospace font without horizontal line metrics");

        let glyphs = (0..GLYPH_COUNT)
            .map(|byte| {
                let character = byte as u8 as char;
                if byte == SPACE as usize || !character.is_ascii_graphic() {
                    return None;
                }
                Some(font.rasterize(character, size))
            })
            .collect();

        Self {
            cell_width: advance.max(1.0),
            cell_height: (line.ascent - line.descent + line.line_gap).round().max(1.0),
            ascent: line.ascent,
            glyphs,
        }
    }

    pub fn size_of(&self, columns: usize, rows: usize) -> (u32, u32) {
        (
            (self.cell_width * columns as f32).round().max(2.0) as u32,
            (self.cell_height * rows as f32).round().max(2.0) as u32,
        )
    }

    pub fn draw(
        &self,
        canvas: &AsciiCanvas,
        foreground: AsciiColor,
        background: AsciiColor,
        size: (u32, u32),
    ) -> RgbaImage {
        let mut image = RgbaImage::from_pixel(
            size.0,
            size.1,
            Rgba([background.red, background.green, background.blue, 255]),
        );

        for row in 0..canvas.rows {
            let baseline = row as f32 * self.cell_height + self.ascent;
            for column in 0..canvas.columns {
                let byte = canvas.get(column, row) as usize;
                let Some(Some((metrics, coverage))) = self.glyphs.get(byte) else {
                    continue;
                };

                let color = canvas.color_at(column, row).unwrap_or(foreground);
                let left = column as f32 * self.cell_width + metrics.xmin as f32;
                let top = baseline - metrics.ymin as f32 - metrics.height as f32;

                blit(&mut image, coverage, metrics, left, top, color);
            }
        }

        image
    }
}

/// Alpha-blends one glyph's coverage bitmap over whatever is already there.
fn blit(
    image: &mut RgbaImage,
    coverage: &[u8],
    metrics: &Metrics,
    left: f32,
    top: f32,
    color: AsciiColor,
) {
    let left = left.round() as i64;
    let top = top.round() as i64;

    for y in 0..metrics.height {
        let target_y = top + y as i64;
        if target_y < 0 || target_y >= image.height() as i64 {
            continue;
        }
        for x in 0..metrics.width {
            let target_x = left + x as i64;
            if target_x < 0 || target_x >= image.width() as i64 {
                continue;
            }
            let alpha = coverage[y * metrics.width + x];
            if alpha == 0 {
                continue;
            }

            let pixel = image.get_pixel_mut(target_x as u32, target_y as u32);
            let a = alpha as u16;
            let inverse = 255 - a;
            pixel[0] = ((color.red as u16 * a + pixel[0] as u16 * inverse) / 255) as u8;
            pixel[1] = ((color.green as u16 * a + pixel[1] as u16 * inverse) / 255) as u8;
            pixel[2] = ((color.blue as u16 * a + pixel[2] as u16 * inverse) / 255) as u8;
        }
    }
}
