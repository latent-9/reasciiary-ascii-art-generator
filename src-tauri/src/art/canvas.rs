//! The grid of characters every glyph-domain tool produces.
//!
//! Ported from `asciiary/AsciiCanvas.swift` and the `AsciiInk` table in
//! `asciiary/Ascii3D.swift` of the Swift original.

use std::sync::LazyLock;

/// A cell colour. Eight bits a channel is what both a terminal's true-colour
/// escape and an exported frame can carry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AsciiColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl AsciiColor {
    pub const WHITE: Self = Self { red: 255, green: 255, blue: 255 };

    /// Clamps and quantizes unit-range components.
    pub fn from_unit(r: f64, g: f64, b: f64) -> Self {
        fn channel(value: f64) -> u8 {
            (value.clamp(0.0, 1.0) * 255.0) as u8
        }
        Self { red: channel(r), green: channel(g), blue: channel(b) }
    }

    /// Reads `#rrggbb`, or `rrggbb` — the window sends the CSS form it already
    /// holds, and a typed line should not need the `#` escaped from the shell.
    pub fn from_hex(text: &str) -> Result<Self, String> {
        let digits = text.strip_prefix('#').unwrap_or(text);
        if digits.len() != 6 {
            return Err(format!("`{text}` is not a colour — write one as #rrggbb"));
        }
        let channel = |at: usize| {
            u8::from_str_radix(&digits[at..at + 2], 16)
                .map_err(|_| format!("`{text}` is not a colour — write one as #rrggbb"))
        };
        Ok(Self { red: channel(0)?, green: channel(2)?, blue: channel(4)? })
    }
}

pub const SPACE: u8 = b' ';

/// How much taller a character cell is than it is wide.
///
/// Not a stylistic choice and not a round number by accident: it is the shape
/// [`super::paint::Painter`] arrives at when it measures the bundled JetBrains
/// Mono, whose line box is 1.32em over a 0.6em advance. A tool that models the
/// grid in world space — the 3D lift does — divides this back out again, so its
/// output only comes back undistorted while every surface the grid lands on
/// agrees on it. The exporter and the window's preview both have to hold to it,
/// and `paint`'s test is the guard on the first of those.
pub const CELL_ASPECT: f64 = 2.2;

/// A grid of characters, optionally coloured.
///
/// Cells are ASCII bytes rather than `char`: every ramp is ASCII by
/// construction, and bytes keep the per-cell loops clear of UTF-8 decoding.
#[derive(Clone, Debug)]
pub struct AsciiCanvas {
    pub columns: usize,
    pub rows: usize,
    /// Row-major cell glyphs.
    pub glyphs: Vec<u8>,
    /// Row-major cell colours. Empty when the canvas is monochrome, in which
    /// case whatever draws it supplies one colour for the whole grid.
    pub colors: Vec<AsciiColor>,
}

impl AsciiCanvas {
    pub fn new(columns: usize, rows: usize, colored: bool) -> Self {
        let count = columns * rows;
        Self {
            columns,
            rows,
            glyphs: vec![SPACE; count],
            colors: if colored { vec![AsciiColor::WHITE; count] } else { Vec::new() },
        }
    }

    pub fn is_colored(&self) -> bool {
        !self.colors.is_empty()
    }

    pub fn get(&self, column: usize, row: usize) -> u8 {
        self.glyphs[row * self.columns + column]
    }

    pub fn set(&mut self, column: usize, row: usize, glyph: u8, color: Option<AsciiColor>) {
        if column >= self.columns || row >= self.rows {
            return;
        }
        let index = row * self.columns + column;
        self.glyphs[index] = glyph;
        if let Some(color) = color {
            if !self.colors.is_empty() {
                self.colors[index] = color;
            }
        }
    }

    pub fn color_at(&self, column: usize, row: usize) -> Option<AsciiColor> {
        if self.colors.is_empty() {
            return None;
        }
        Some(self.colors[row * self.columns + column])
    }

    /// The canvas as plain text, rows separated by newlines. Colour is dropped,
    /// which is the honest answer: a text file has none.
    pub fn text(&self) -> String {
        let mut buffer = Vec::with_capacity(self.rows * (self.columns + 1));
        for row in 0..self.rows {
            if row > 0 {
                buffer.push(b'\n');
            }
            buffer.extend_from_slice(&self.glyphs[row * self.columns..(row + 1) * self.columns]);
        }
        String::from_utf8_lossy(&buffer).into_owned()
    }

    /// A drawing somebody wrote, on the grid it was written at.
    ///
    /// The inverse of [`AsciiCanvas::text`], and the one place a file of
    /// characters becomes a grid of them. Two tools want it for opposite
    /// reasons — the lift reads the ink as heights, the flat read draws the
    /// characters back out — and a file has to be tidied the same way for both
    /// or the same drawing arrives at two sizes.
    ///
    /// Tidying is three things. Tabs become four spaces, because a tab is a
    /// width the file does not carry and every other column would shift by
    /// whatever this guessed later. Trailing blank lines go, being an artifact
    /// of how the file was saved rather than part of the drawing. And short
    /// lines are padded to the longest, so the grid is a rectangle — a cell
    /// past the end of a line is paper the same way a space is.
    pub fn from_text(text: &str) -> Self {
        let normalized = text.replace("\r\n", "\n").replace('\t', "    ");
        let mut lines: Vec<&str> = normalized.split('\n').collect();
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }

        let rows = lines.len();
        let columns = lines.iter().map(|line| line.chars().count()).max().unwrap_or(0);
        let mut canvas = Self::new(columns, rows, false);
        for (row, line) in lines.iter().enumerate() {
            for (column, character) in line.chars().enumerate() {
                canvas.glyphs[row * columns + column] = mark(character);
            }
        }
        canvas
    }
}

/// The byte a character of somebody's drawing is held as.
///
/// The grid is ASCII by construction and the font is only asked for ASCII, so a
/// drawing that arrives with box-drawing or block characters in it has to come
/// down to one. It comes down by ink, which is the measure everything else here
/// reads a drawing with: those characters are solid, and the solid ASCII mark is
/// `@`. Dropping them instead would punch holes through exactly the strokes the
/// artist drew heaviest.
fn mark(character: char) -> u8 {
    if character.is_whitespace() {
        SPACE
    } else if character.is_ascii_graphic() {
        character as u8
    } else {
        b'@'
    }
}

/// Every printable ASCII glyph ordered by roughly how much ink it puts on the
/// cell, lightest first. One ordering serves both directions: reading a
/// drawing's heights, and — as [`AsciiRamp::Ink`] — writing back the shade of
/// a lit surface.
pub const INK_RAMP: &str =
    r#" .'`,:;^"~-_!|ilIjrt()[]{}/\<>?+=*cvxzsnuoea17LTJCYfywkh325469FPVXZESGAKHUDOQ0bdpqgmRNWM&8%$#B@"#;

/// Coverage per ASCII byte, resolved once. A drawing is read cell by cell, so a
/// linear scan of the ramp here showed up on drawings of any size.
static COVERAGE: LazyLock<[f64; 128]> = LazyLock::new(|| {
    let ramp = INK_RAMP.as_bytes();
    let last = (ramp.len() - 1) as f64;
    let mut table = [1.0; 128];
    for byte in 0..128u8 {
        if (byte as char).is_whitespace() {
            table[byte as usize] = 0.0;
        }
    }
    for (index, &byte) in ramp.iter().enumerate() {
        table[byte as usize] = index as f64 / last;
    }
    table
});

/// Ink coverage of `character`, 0 for an empty cell and 1 for a solid one.
///
/// Anything outside printable ASCII counts as solid rather than empty:
/// box-drawing and block glyphs are common in ASCII art, and dropping them
/// would punch holes through exactly the parts the artist drew heaviest.
pub fn ink_coverage(character: char) -> f64 {
    if (character as u32) < 128 {
        return COVERAGE[character as usize];
    }
    if character.is_whitespace() {
        0.0
    } else {
        1.0
    }
}

/// The set of glyphs a tool shades with, lightest first.
///
/// Which one to use is a real choice rather than a preference. The ink ramp is
/// ordered by coverage, which is exactly right for reading a drawing's heights
/// back out — but it puts `%`, `&`, `M` and `W` next to each other, and on a
/// smooth surface that reads as noise however correct the coverage is. The
/// short ramps trade steps for glyphs that are visibly ordered.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AsciiRamp {
    /// The ten-step ramp most ASCII renderers use.
    Shades,
    /// Fifteen steps, still visibly ordered.
    Detailed,
    /// Every printable glyph, ordered by coverage.
    Ink,
}

impl AsciiRamp {
    /// Every one of them, which is the order they are offered in and the order
    /// anything holding one table per ramp holds them in.
    pub const ALL: [Self; 3] = [Self::Shades, Self::Detailed, Self::Ink];

    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Shades => b" .:-=+*#%@",
            Self::Detailed => b" .,:;i1tfLCG08@",
            Self::Ink => INK_RAMP.as_bytes(),
        }
    }

    /// What a flag calls it, which is also what the window's control sends.
    pub fn name(self) -> &'static str {
        match self {
            Self::Shades => "shades",
            Self::Detailed => "detailed",
            Self::Ink => "ink",
        }
    }

    pub fn named(name: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|ramp| ramp.name() == name)
            .ok_or_else(|| {
                format!("`{name}` is not a set of characters — try shades, detailed or ink")
            })
    }

    /// The glyph standing in for a surface lit to `intensity`, 0 to 1.
    pub fn byte_for_intensity(intensity: f64, ramp: &[u8]) -> u8 {
        if ramp.is_empty() {
            return SPACE;
        }
        let clamped = intensity.clamp(0.0, 1.0);
        ramp[(clamped * (ramp.len() - 1) as f64).round() as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `#` has to be optional both ways: the window sends the CSS colour it
    /// already holds, which carries one, and a typed line would rather not
    /// quote it past the shell.
    #[test]
    fn a_colour_reads_with_or_without_its_hash() {
        let cheese = AsciiColor { red: 245, green: 232, blue: 199 };
        assert_eq!(AsciiColor::from_hex("#f5e8c7"), Ok(cheese));
        assert_eq!(AsciiColor::from_hex("F5E8C7"), Ok(cheese));
    }

    /// A colour that does not parse has to say so rather than silently falling
    /// back, or an export quietly comes out in the wrong scheme.
    #[test]
    fn anything_that_is_not_six_hex_digits_is_refused() {
        for text in ["#fff", "", "#gggggg", "#f5e8c7f"] {
            assert!(AsciiColor::from_hex(text).is_err(), "`{text}` was accepted");
        }
    }
}
