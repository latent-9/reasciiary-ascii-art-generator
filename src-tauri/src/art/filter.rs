//! Post-processing, in the two domains a frame passes through.
//!
//! Nothing like this exists in the Swift original — it is a new axis, not a
//! port. The split matters: a filter that reorders glyphs has to run while the
//! frame is still a character grid, and a filter that bends scanlines has to
//! run after the grid has been painted. `crt` and `vhs` are the second kind.

use image::RgbaImage;

use super::canvas::AsciiCanvas;
use super::params::Params;

/// Runs while the frame is still a character grid.
pub trait GlyphFilter: Send + Sync {
    fn apply(&self, canvas: &mut AsciiCanvas, time: f64);
}

/// Runs after the grid has been painted to pixels.
pub trait PixelFilter: Send + Sync {
    fn apply(&self, image: &mut RgbaImage, time: f64);
}

pub enum Filter {
    Glyph(Box<dyn GlyphFilter>),
    Pixel(Box<dyn PixelFilter>),
}

pub type FilterFactory = fn(&Params) -> Result<Filter, String>;

pub fn registry() -> &'static [(&'static str, FilterFactory)] {
    &[
        // ("crt", filters::crt::build),
        // ("vhs", filters::vhs::build),
        // ("glitch", filters::glitch::build),
    ]
}

pub fn lookup(name: &str) -> Option<FilterFactory> {
    registry()
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, factory)| *factory)
}
