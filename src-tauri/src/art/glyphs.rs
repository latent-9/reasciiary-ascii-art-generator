//! The characters a shaded surface can be drawn with, and how one is picked.
//!
//! The obvious way to choose is a ramp: order some glyphs by weight, take the
//! surface's brightness, index in. That is what `emilwidlund/ASCII` does on the
//! GPU with `" .:,'-^=*+?!|0#X%WM@"`, and it is what this renderer used to do.
//! It cannot do better than one value a cell, so a silhouette comes out as a
//! staircase of whichever glyph happened to be nearest the average.
//!
//! `alecjacobson/ascii3d` does something better, and this follows it. The
//! surface is rasterised *finer* than the grid — 5×10 there, [`CELL_PIXELS`]
//! here — and each cell then takes the character whose own rasterised bitmap
//! looks most like that patch, by least squares over the pixels. Nothing is
//! ordered by weight and nothing is ordered at all: `/` wins a cell because the
//! ink in `/` lies where the light in that cell lies. That is where the sloped
//! edges and the traced silhouettes come from.

use std::sync::LazyLock;

use fontdue::Font;

use super::canvas::SPACE;
use super::FONT;

/// How finely a cell is sampled before a character is chosen for it.
///
/// `ascii3d` matches against 5×10 bitmaps of Courier New Bold. This is the same
/// width, against the shape a cell on this grid actually has — the two differ
/// only because JetBrains Mono's line box is slightly taller than Courier's.
pub const CELL_PIXELS_WIDE: usize = 5;
pub const CELL_PIXELS_TALL: usize = 11;
pub const CELL_PIXELS: usize = CELL_PIXELS_WIDE * CELL_PIXELS_TALL;

/// One patch of surface, sampled at [`CELL_PIXELS`] and ready to be matched.
pub type Cell = [f32; CELL_PIXELS];

/// How much finer than [`CELL_PIXELS`] a glyph is rasterised before being
/// averaged down into one.
///
/// Rasterising straight to a 5×11 bitmap asks the font for a 5-pixel em, and
/// what comes back is mush: the stems of `d`, `b` and `h` all land on the same
/// two pixels. Rasterising large and box-filtering keeps the *proportion* of
/// each cell each stroke covers, which is the quantity being matched.
const SUPERSAMPLE: usize = 8;

/// The printable ASCII range, which is the candidate set `ascii3d` uses too.
const FIRST: u8 = b' ';
const LAST: u8 = b'~';

/// How much the amount of light in a cell counts against where in the cell it
/// falls, when the two disagree.
///
/// They are measured separately below, so this is the only place their relative
/// worth is decided. Raise it and a lit face grades smoothly but its edges blur
/// into the ramp; lower it and every edge is traced but the faces behind them
/// lose their shading.
const LEVEL_WEIGHT: f32 = 4.0;

struct Template {
    byte: u8,
    /// How much ink the glyph lays down, as a fraction of the most any glyph
    /// can — so it is directly comparable with a cell's brightness.
    weight: f32,
    /// The coverage with its own mean taken out and scaled to unit length:
    /// where the ink sits, with how much of it there is divided back out.
    ///
    /// That division is the point. `/` and `X` are drawn along the same
    /// diagonal and differ only in weight, and a comparison that keeps the
    /// weight in cannot say they are the same shape — it just prefers whichever
    /// one happens to be nearer the cell's brightness, which is the level
    /// question being asked twice.
    shape: Cell,
}

pub struct Alphabet {
    templates: Vec<Template>,
}

/// Rasterised once. Ninety-five glyphs at eight times resolution is real work,
/// and every frame of an export would otherwise repeat it.
pub static ALPHABET: LazyLock<Alphabet> = LazyLock::new(Alphabet::rasterize);

impl Alphabet {
    fn rasterize() -> Self {
        let font = Font::from_bytes(FONT, fontdue::FontSettings::default())
            .expect("the bundled font loads");

        let wide = CELL_PIXELS_WIDE * SUPERSAMPLE;
        let tall = CELL_PIXELS_TALL * SUPERSAMPLE;

        // Advance width scales linearly with size, so one measurement gives the
        // size whose cell is exactly `wide` pixels across.
        let probe = font.metrics('0', 100.0).advance_width.max(1.0);
        let size = wide as f32 * 100.0 / probe;
        let ascent = font
            .horizontal_line_metrics(size)
            .expect("monospace font without horizontal line metrics")
            .ascent;

        let coverages: Vec<(u8, Cell)> = (FIRST..=LAST)
            .map(|byte| {
                let mut large = vec![0f32; wide * tall];
                if byte != SPACE {
                    stamp(&font, byte as char, size, ascent, &mut large, wide, tall);
                }
                (byte, reduce(&large, wide))
            })
            .collect();

        // The most ink any one character can put on a cell, averaged over it —
        // nowhere near 1, because the heaviest glyph in ASCII still leaves the
        // gaps inside its own strokes and the space between one cell and the
        // next. Weight is measured against this rather than against a full cell,
        // so that a fully lit patch asks for something a candidate can actually
        // supply. Against 1.0 the whole top of the range collapses onto one
        // character and the brightest part of a render goes flat.
        //
        // It is relative, so every glyph has to be rasterised before any of them
        // can be measured.
        let range = coverages
            .iter()
            .map(|(_, coverage)| mean_of(coverage))
            .fold(0.0, f32::max)
            .max(f32::EPSILON);

        let templates = coverages
            .into_iter()
            .map(|(byte, coverage)| Template::describe(byte, coverage, range))
            .collect();

        Self { templates }
    }

    /// The character to draw `cell` with, whose samples run from 0 for unlit to
    /// 1 for fully lit.
    ///
    /// Which of the two references applies depends on what is in the cell, and
    /// neither does the other's job well. Least squares over bitmaps is the only
    /// thing that can find `/` for a sloped edge, but on the flat inside of a
    /// face it has nothing to lock onto: every candidate at roughly the right
    /// weight is wrong by roughly the same amount, so the winner is whichever
    /// glyph happens to be most evenly spread, and a whole face comes out as one
    /// character repeated. A ramp is the opposite — it grades a face properly
    /// and cannot represent an edge at all.
    ///
    /// So: a cell that varies inside itself is an edge, and gets matched; a cell
    /// that does not is interior, and gets the ramp.
    pub fn nearest(&self, cell: &Cell) -> u8 {
        let mut lightest = f32::INFINITY;
        let mut heaviest = f32::NEG_INFINITY;
        let mut total = 0.0;
        for &sample in cell.iter() {
            lightest = lightest.min(sample);
            heaviest = heaviest.max(sample);
            total += sample;
        }
        let mean = total / CELL_PIXELS as f32;

        if heaviest - lightest < STRUCTURE {
            return graded(mean);
        }
        self.matched(cell, mean)
    }

    /// Picks the glyph closest to the cell in two respects at once: how much
    /// light the cell holds, and where in the cell it sits.
    ///
    /// `ascii3d` asks both at once, as one least squares over the raw bitmaps,
    /// and that does not survive the move to a shaded surface. A sum of squares
    /// splits into a level part and a shape part — `Σ (t - c)² = N (t̄ - c̄)² +
    /// Σ ((t - t̄) - (c - c̄))²` — and the two parts are not on the same scale.
    /// A glyph's strokes are almost opaque while a lit patch of surface is
    /// rarely more than half bright, so the shape part is dominated by the
    /// glyph's own contrast and reads every candidate as far too dark; the
    /// cheapest answer becomes whichever glyph is faintest, whatever the cell
    /// looks like. Taking each side's own level out first and comparing what is
    /// left as directions puts shape on a scale of its own, and leaves the level
    /// question to be asked once, plainly, against [`Alphabet::range`].
    fn matched(&self, cell: &Cell, mean: f32) -> u8 {
        let Some(shape) = direction_of(cell, mean) else {
            return graded(mean);
        };

        let mut best = SPACE;
        let mut best_score = f32::INFINITY;
        for template in &self.templates {
            let mut alignment = 0.0;
            for (sample, ink) in shape.iter().zip(&template.shape) {
                alignment += sample * ink;
            }
            let level = template.weight - mean;
            let score = 1.0 - alignment + LEVEL_WEIGHT * level * level;
            if score < best_score {
                best_score = score;
                best = template.byte;
            }
        }
        best
    }
}

fn mean_of(cell: &Cell) -> f32 {
    cell.iter().sum::<f32>() / CELL_PIXELS as f32
}

/// The cell with its level taken out and what is left scaled to unit length, or
/// `None` if there was nothing left — a patch of one flat value has no shape to
/// point at, and normalising it would be a division by zero.
fn direction_of(cell: &Cell, mean: f32) -> Option<Cell> {
    let mut shape = *cell;
    let mut length = 0.0;
    for sample in shape.iter_mut() {
        *sample -= mean;
        length += *sample * *sample;
    }

    let length = length.sqrt();
    if length < f32::EPSILON {
        return None;
    }
    for sample in shape.iter_mut() {
        *sample /= length;
    }
    Some(shape)
}

/// How much a cell has to vary inside itself before it counts as an edge.
///
/// Low enough that a silhouette is caught, high enough that the gentle slope
/// across a lit face is not — a face graded by the ramp and a face picked out
/// glyph by glyph look completely different, and the boundary between them
/// should fall where the picture has one.
const STRUCTURE: f32 = 0.16;

/// `emilwidlund/ASCII`'s default set, which is its whole technique: twenty
/// characters "in brightness order dark -> light", indexed by brightness. Short
/// enough that consecutive steps are visibly different, which the ninety-five
/// ordered by coverage are not — that ordering puts `%`, `&`, `M` and `W` beside
/// each other, and a face graded through them reads as noise.
const RAMP: &[u8] = b" .:,'-^=*+?!|0#X%WM@";

fn graded(brightness: f32) -> u8 {
    let step = brightness.clamp(0.0, 1.0) * (RAMP.len() - 1) as f32;
    RAMP[step.round() as usize]
}

/// Draws one glyph into `target` where [`super::paint::Painter`] would put it,
/// so a matched character lands on the exported frame in the position it was
/// matched at.
fn stamp(
    font: &Font,
    character: char,
    size: f32,
    ascent: f32,
    target: &mut [f32],
    wide: usize,
    tall: usize,
) {
    let (metrics, alpha) = font.rasterize(character, size);
    let left = metrics.xmin as i64;
    let top = (ascent - metrics.ymin as f32 - metrics.height as f32).round() as i64;

    for y in 0..metrics.height {
        let row = top + y as i64;
        if row < 0 || row >= tall as i64 {
            continue;
        }
        for x in 0..metrics.width {
            let column = left + x as i64;
            if column < 0 || column >= wide as i64 {
                continue;
            }
            target[row as usize * wide + column as usize] =
                alpha[y * metrics.width + x] as f32 / 255.0;
        }
    }
}

/// Box-filters a supersampled glyph down to one cell's worth of coverage.
fn reduce(large: &[f32], wide: usize) -> Cell {
    let mut coverage = [0f32; CELL_PIXELS];
    let area = (SUPERSAMPLE * SUPERSAMPLE) as f32;

    for (index, sample) in coverage.iter_mut().enumerate() {
        let left = (index % CELL_PIXELS_WIDE) * SUPERSAMPLE;
        let top = (index / CELL_PIXELS_WIDE) * SUPERSAMPLE;
        let mut total = 0.0;
        for y in 0..SUPERSAMPLE {
            for x in 0..SUPERSAMPLE {
                total += large[(top + y) * wide + left + x];
            }
        }
        *sample = total / area;
    }

    coverage
}

impl Template {
    fn describe(byte: u8, coverage: Cell, range: f32) -> Self {
        let mean = mean_of(&coverage);
        Self {
            byte,
            weight: mean / range,
            // A space, and only a space, has no direction. Zero aligns with
            // nothing, which is what leaves it competing on level alone.
            shape: direction_of(&coverage, mean).unwrap_or([0.0; CELL_PIXELS]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::canvas::CELL_ASPECT;

    /// The sub-cell grid stands in for a cell, so it has to be that cell's
    /// shape. Were it not, every render would come out stretched by the
    /// difference without anything else noticing.
    #[test]
    fn a_sampled_cell_has_the_shape_of_a_cell() {
        let aspect = CELL_PIXELS_TALL as f64 / CELL_PIXELS_WIDE as f64;
        assert!(
            (aspect - CELL_ASPECT).abs() < 0.01,
            "cells are sampled {CELL_PIXELS_WIDE}x{CELL_PIXELS_TALL}, an aspect \
             of {aspect}, but the grid is modelled at {CELL_ASPECT}"
        );
    }

    /// Nothing lit is nothing drawn. A blank patch has to come back blank or a
    /// render fogs over with whichever glyph is faintest.
    #[test]
    fn an_unlit_cell_is_a_space() {
        assert_eq!(ALPHABET.nearest(&[0.0; CELL_PIXELS]), SPACE);
    }

    /// A patch that is lit everywhere wants a glyph that covers everywhere.
    #[test]
    fn a_fully_lit_cell_is_a_heavy_glyph() {
        let glyph = ALPHABET.nearest(&[1.0; CELL_PIXELS]);
        assert!(
            b"@#%$&8BMW".contains(&glyph),
            "a solid patch came back as `{}`",
            glyph as char
        );
    }

    /// The point of matching bitmaps rather than reading a ramp: a cell lit only
    /// along one diagonal should come back as a glyph drawn along that diagonal,
    /// which no ordering by weight could ever produce.
    #[test]
    fn a_diagonal_patch_picks_a_diagonal_glyph() {
        let mut rising = [0f32; CELL_PIXELS];
        let mut falling = [0f32; CELL_PIXELS];
        for y in 0..CELL_PIXELS_TALL {
            // Bottom-left to top-right, and its mirror.
            let across = y as f32 / (CELL_PIXELS_TALL - 1) as f32;
            let x = ((1.0 - across) * (CELL_PIXELS_WIDE - 1) as f32).round() as usize;
            rising[y * CELL_PIXELS_WIDE + x] = 1.0;
            falling[y * CELL_PIXELS_WIDE + (CELL_PIXELS_WIDE - 1 - x)] = 1.0;
        }

        assert_eq!(ALPHABET.nearest(&rising), b'/');
        assert_eq!(ALPHABET.nearest(&falling), b'\\');
    }

    /// Where in the cell the light is has to matter, not just how much of it
    /// there is. Both of these are one row lit out of eleven.
    #[test]
    fn where_the_light_sits_in_a_cell_decides_the_glyph() {
        let row = |index: usize| {
            let mut cell = [0f32; CELL_PIXELS];
            for x in 0..CELL_PIXELS_WIDE {
                cell[index * CELL_PIXELS_WIDE + x] = 1.0;
            }
            ALPHABET.nearest(&cell)
        };
        let low = row(CELL_PIXELS_TALL - 3);
        let high = row(1);
        assert_ne!(
            low, high,
            "a bar at the bottom and a bar at the top both came back as `{}`",
            low as char
        );
        assert_eq!(low, b'_');
    }

    /// Every candidate is a character the exporter can actually draw.
    #[test]
    fn the_alphabet_is_printable_ascii() {
        assert_eq!(ALPHABET.templates.len(), 95);
        assert!(ALPHABET
            .templates
            .iter()
            .all(|template| template.byte.is_ascii_graphic() || template.byte == SPACE));
    }
}
