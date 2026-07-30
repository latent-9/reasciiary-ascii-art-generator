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
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Shades => b" .:-=+*#%@",
            Self::Detailed => b" .,:;i1tfLCG08@",
            Self::Ink => INK_RAMP.as_bytes(),
        }
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
