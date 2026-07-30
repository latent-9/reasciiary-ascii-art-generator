//! What a tool produces.
//!
//! The glyph contract is a direct port of the `AsciiFrameSource` protocol in
//! `asciiary/AsciiCanvasView.swift` — the abstraction the Swift stage, the
//! exporter and the copy command all already speak to. The pixel contract is
//! new: it exists so a tool whose output was never a character grid (flow
//! fields, noise) can join the same pipeline.

use image::RgbaImage;

use super::canvas::AsciiCanvas;
use super::params::Params;

/// A tool that fills a character grid.
///
/// `time` is absolute seconds, not a normalized position. A generator that is
/// periodic over [`loop_duration`](Self::loop_duration) loops seamlessly,
/// because the exporter samples exactly that span.
pub trait GlyphGenerator: Send + Sync {
    fn canvas(&self, columns: usize, rows: usize, time: f64) -> AsciiCanvas;

    /// `None` for a still.
    fn loop_duration(&self) -> Option<f64> {
        None
    }

    /// How many columns the drawing wants for every row, so the grid it lands
    /// on can be the shape it is rather than a shape chosen for it. `None` from
    /// a tool that fills whatever grid it is handed.
    fn frame_aspect(&self) -> Option<f64> {
        None
    }
}

/// A tool that draws pixels directly, skipping the character grid.
pub trait PixelGenerator: Send + Sync {
    fn frame(&self, width: u32, height: u32, time: f64) -> RgbaImage;

    fn loop_duration(&self) -> Option<f64> {
        None
    }
}

pub enum Generator {
    Glyph(Box<dyn GlyphGenerator>),
    Pixel(Box<dyn PixelGenerator>),
}

impl Generator {
    pub fn loop_duration(&self) -> Option<f64> {
        match self {
            Self::Glyph(g) => g.loop_duration(),
            Self::Pixel(g) => g.loop_duration(),
        }
    }
}

/// Builds a generator from the flags a REPL command carried.
pub type GeneratorFactory = fn(&Params) -> Result<Generator, String>;

/// Name to factory. The Swift original switched on a `PaneContent` enum in
/// five places; a table keeps adding the next tool to one line.
pub fn registry() -> &'static [(&'static str, GeneratorFactory)] {
    &[
        ("ascii", super::generators::ascii::build),
        // ("scene", generators::scene::build),
        // ("media", generators::media::build),
        // ("gen2d", generators::gen2d::build),
    ]
}

pub fn lookup(name: &str) -> Option<GeneratorFactory> {
    registry()
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, factory)| *factory)
}
