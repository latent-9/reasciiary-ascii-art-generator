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
            // Deliberately not rounded to whole pixels. A cell is 18.48 pixels
            // tall at the default size, and rounding it down cost a frame 2.6%
            // of its height over 48 rows — a squash the 3D lift cannot know
            // about, since it models the grid at a fixed cell shape. Baselines
            // land on fractional rows instead and `blit` rounds each one, which
            // is the same trade a terminal makes.
            cell_height: (line.ascent - line.descent + line.line_gap).max(1.0),
            ascent: line.ascent,
            glyphs,
        }
    }

    /// The shape of one cell, tall over wide.
    pub fn cell_aspect(&self) -> f64 {
        self.cell_height as f64 / self.cell_width as f64
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::canvas::CELL_ASPECT;
    use crate::art::{BASE_FONT_SIZE, FONT};

    /// Swapping the bundled font for one with different metrics would silently
    /// stretch every 3D render, because the lift models the grid at
    /// [`CELL_ASPECT`] and nothing downstream re-measures. Failing here instead
    /// says which number to move.
    #[test]
    fn painted_cells_have_the_shape_the_grid_is_modelled_at() {
        let font = Font::from_bytes(FONT, fontdue::FontSettings::default())
            .expect("bundled font loads");

        for scale in [1.0, 2.0, 3.0] {
            let painter = Painter::new(&font, BASE_FONT_SIZE * scale);
            let aspect = painter.cell_aspect();
            assert!(
                (aspect - CELL_ASPECT).abs() < 0.01,
                "cells are {aspect:.3} tall per unit wide at scale {scale}, \
                 but the grid is modelled at {CELL_ASPECT}"
            );
        }
    }
}
