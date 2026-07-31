//! A picture read back as glyphs.
//!
//! Port of `asciiary/AsciiMedia.swift`.
//!
//! The original sampled one pixel a cell and indexed a ramp with it, which is
//! what almost every image-to-ASCII converter does and it throws away the thing
//! that makes a picture legible at this size. A cell is not one value: it is a
//! patch, and whether the light in it lies along a diagonal or across the top is
//! the whole of what tells an eye where the edges are.
//!
//! So the picture is sampled at a whole patch a cell and handed to
//! [`super::super::read`], which matches it the way the 3D lift's own cells are
//! matched. An edge in the photograph comes back as a glyph running the same way
//! the edge does. What is left here is everything that is particular to a file:
//! decoding it, keeping its shape on a grid that is not its shape, and holding
//! to the timing it was authored with.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Mutex;

use image::imageops::{crop_imm, resize, FilterType};
use image::{AnimationDecoder, ImageReader, RgbaImage};

use crate::art::canvas::{AsciiCanvas, CELL_ASPECT};
use crate::art::generator::{Generator, GlyphGenerator};
use crate::art::params::Params;
use crate::art::read::{fine_size, Reader};

/// How long a frame holds when the file does not say.
const FRAME_SECONDS: f64 = 0.1;

/// How a picture that is not the shape of the grid is made to fit it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fit {
    /// All of the picture, with the grid left blank around it.
    Contain,
    /// All of the grid, with the ends of the picture cut off.
    Cover,
}

impl Fit {
    fn named(name: &str) -> Result<Self, String> {
        match name {
            "contain" => Ok(Self::Contain),
            "cover" => Ok(Self::Cover),
            other => Err(format!("`{other}` is not a fit — try contain or cover")),
        }
    }
}

/// One frame of the source and how long it holds the screen.
struct Still {
    image: RgbaImage,
    seconds: f64,
}

/// The last frame sampled onto the last grid it was asked for.
///
/// A frame is resampled to the grid's own sub-cell raster, which is real work,
/// and the window asks for the same frame on the same grid repeatedly — every
/// redraw while nothing moves, and every frame twice at the ends of a scrub. One
/// entry catches all of that. It deliberately does not hold more: a long
/// animation at a usable grid size is tens of megabytes a second of footage, and
/// the sequence is walked in order anyway, so a second entry would never be the
/// one asked for.
struct Sampled {
    frame: usize,
    columns: usize,
    rows: usize,
    fine: RgbaImage,
    /// Where on the grid the picture starts.
    at: (usize, usize),
}

pub struct Media {
    frames: Vec<Still>,
    /// Seconds the whole sequence takes.
    span: f64,
    fit: Fit,
    reader: Reader,
    sampled: Mutex<Option<Sampled>>,
}

impl Media {
    /// Which frame is on screen at `time`, and the picture's own timing decides
    /// it: a GIF carries a delay per frame and they are not all the same.
    fn frame_at(&self, time: f64) -> usize {
        if self.frames.len() < 2 || self.span <= 0.0 {
            return 0;
        }
        let mut left = time.rem_euclid(self.span);
        for (index, still) in self.frames.iter().enumerate() {
            left -= still.seconds;
            if left < 0.0 {
                return index;
            }
        }
        self.frames.len() - 1
    }

    /// The picture at the size it is drawn, in sub-cell samples, and where on
    /// the grid it lands.
    ///
    /// Both of those follow from the picture keeping its shape. A cell is
    /// [`CELL_ASPECT`] times taller than it is wide, so a square photograph on a
    /// square grid comes out stretched unless something divides that back out,
    /// and the something is here.
    fn sample(&self, frame: usize, columns: usize, rows: usize) -> (RgbaImage, (usize, usize)) {
        let image = &self.frames[frame].image;
        let (wide, tall) = (image.width().max(1) as f64, image.height().max(1) as f64);

        let (source, across, down) = match self.fit {
            // Every column, every row, and whichever part of the picture has
            // the grid's own shape.
            Fit::Cover => {
                let wanted = columns as f64 / (rows as f64 * CELL_ASPECT);
                let (cut_wide, cut_tall) = if wide / tall > wanted {
                    (tall * wanted, tall)
                } else {
                    (wide, wide / wanted)
                };
                let crop = crop_imm(
                    image,
                    ((wide - cut_wide) / 2.0) as u32,
                    ((tall - cut_tall) / 2.0) as u32,
                    cut_wide.max(1.0) as u32,
                    cut_tall.max(1.0) as u32,
                )
                .to_image();
                (crop, columns, rows)
            }
            // Every pixel, on as much of the grid as keeps its shape.
            Fit::Contain => {
                let across = (rows as f64 * CELL_ASPECT * wide / tall).round() as usize;
                if across <= columns {
                    (image.clone(), across.clamp(1, columns), rows)
                } else {
                    let down = (columns as f64 * tall / (CELL_ASPECT * wide)).round() as usize;
                    (image.clone(), columns, down.clamp(1, rows))
                }
            }
        };

        let (wide, tall) = fine_size(across, down);
        let fine = resize(
            &source,
            wide,
            tall,
            // Averaging, not nearest: a cell's patch is meant to be what that
            // part of the picture looks like, and a picture sampled at a point
            // is a picture of its own noise.
            FilterType::Triangle,
        );
        (fine, ((columns - across) / 2, (rows - down) / 2))
    }
}

impl GlyphGenerator for Media {
    fn canvas(&self, columns: usize, rows: usize, time: f64) -> AsciiCanvas {
        let mut canvas = AsciiCanvas::new(columns, rows, self.reader.colored);
        if columns == 0 || rows == 0 || self.frames.is_empty() {
            return canvas;
        }

        let frame = self.frame_at(time);
        let mut held = self.sampled.lock().unwrap_or_else(|error| error.into_inner());
        let stale = match held.as_ref() {
            Some(last) => last.frame != frame || last.columns != columns || last.rows != rows,
            None => true,
        };
        if stale {
            let (fine, at) = self.sample(frame, columns, rows);
            *held = Some(Sampled { frame, columns, rows, fine, at });
        }
        let sampled = held.as_ref().expect("a sample was just taken");
        self.reader.draw_into(&mut canvas, &sampled.fine, sampled.at);
        canvas
    }

    fn loop_duration(&self) -> Option<f64> {
        (self.frames.len() > 1 && self.span > 0.0).then_some(self.span)
    }

    fn frame_aspect(&self) -> Option<f64> {
        let image = &self.frames.first()?.image;
        Some(image.width() as f64 * CELL_ASPECT / image.height().max(1) as f64)
    }
}

/// Every frame the file holds, in order.
///
/// A still is one frame; an animation is all of them, each with the delay it was
/// authored with. Nothing here re-times anything — a GIF that was made to run
/// slowly runs slowly, and the exporter samples the span it reports.
fn read(path: &str) -> Result<Vec<Still>, String> {
    let animated = Path::new(path)
        .extension()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("gif"));

    if animated {
        let file = File::open(path).map_err(|error| format!("cannot read `{path}`: {error}"))?;
        let decoder = image::codecs::gif::GifDecoder::new(BufReader::new(file))
            .map_err(|error| format!("cannot read `{path}`: {error}"))?;
        let frames = decoder
            .into_frames()
            .collect_frames()
            .map_err(|error| format!("cannot read `{path}`: {error}"))?;
        if !frames.is_empty() {
            return Ok(frames
                .into_iter()
                .map(|frame| {
                    let (numerator, denominator) = frame.delay().numer_denom_ms();
                    let seconds = if denominator == 0 {
                        FRAME_SECONDS
                    } else {
                        numerator as f64 / denominator as f64 / 1000.0
                    };
                    // A GIF written with no delay means "as fast as you can",
                    // which at export is a frame that lasts no time at all.
                    Still { image: frame.into_buffer(), seconds: seconds.max(0.01) }
                })
                .collect());
        }
    }

    let image = ImageReader::open(path)
        .map_err(|error| format!("cannot read `{path}`: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("cannot read `{path}`: {error}"))?
        .decode()
        .map_err(|error| format!("`{path}` is not a picture this can read: {error}"))?;
    Ok(vec![Still { image: image.to_rgba8(), seconds: FRAME_SECONDS }])
}

fn assemble(params: &Params) -> Result<Media, String> {
    let path = params
        .first_positional()
        .ok_or("media needs a picture to read")?;
    let frames = read(path)?;
    let span = frames.iter().map(|still| still.seconds).sum();

    Ok(Media {
        frames,
        span,
        fit: Fit::named(params.string("fit").unwrap_or("contain"))?,
        reader: Reader::from_params(params)?,
        sampled: Mutex::new(None),
    })
}

pub fn build(params: &Params) -> Result<Generator, String> {
    Ok(Generator::Glyph(Box::new(assemble(params)?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    use crate::art::canvas::{ink_coverage, SPACE};

    /// A picture written where a test can point the tool at it. The tool takes a
    /// path, which is the whole of what it does that a formula does not, so a
    /// test that skipped the file would not be testing it.
    fn written(name: &str, image: &RgbaImage) -> String {
        let path = std::env::temp_dir().join(name);
        image.save(&path).expect("the picture writes");
        path.to_string_lossy().into_owned()
    }

    /// Light on the left, dark on the right.
    fn gradient(wide: u32, tall: u32) -> RgbaImage {
        RgbaImage::from_fn(wide, tall, |x, _| {
            let level = (255 - (x * 255 / wide.max(1)) as u8).max(1);
            Rgba([level, level, level, 255])
        })
    }

    fn media(path: &str, flags: &[(&str, &str)]) -> Media {
        let mut params = Params { positional: vec![path.to_string()], ..Params::default() };
        for (name, value) in flags {
            params
                .flags
                .insert(name.to_string(), Some(value.to_string()));
        }
        assemble(&params).expect("the tool builds")
    }

    #[test]
    fn a_picture_arrives_as_marks() {
        let path = written("asciiary-gradient.png", &gradient(320, 160));
        let canvas = media(&path, &[]).canvas(60, 20, 0.0);
        let inked = canvas.glyphs.iter().filter(|&&glyph| glyph != SPACE).count();
        assert!(inked > 300, "only {inked} cells drawn");
    }

    /// Which way round the picture goes has to survive the trip: a converter
    /// that draws it mirrored or upside down passes every count-based test.
    #[test]
    fn the_light_end_of_a_picture_is_the_heavy_end_of_the_drawing() {
        let path = written("asciiary-gradient.png", &gradient(320, 160));
        let canvas = media(&path, &[("marks", "shades")]).canvas(60, 20, 0.0);
        let row = canvas.rows / 2;
        let ink = |column: usize| ink_coverage(canvas.get(column, row) as char);
        assert!(
            ink(4) > ink(canvas.columns - 5),
            "{} against {}",
            canvas.get(4, row) as char,
            canvas.get(canvas.columns - 5, row) as char
        );
    }

    /// A tall picture on a wide grid keeps its shape rather than being pulled
    /// out to the edges, and the cells it does not reach stay empty.
    #[test]
    fn a_picture_that_does_not_fit_the_grid_keeps_its_shape() {
        let path = written("asciiary-tall.png", &gradient(80, 320));
        let canvas = media(&path, &[]).canvas(80, 24, 0.0);
        let drawn = |column: usize| {
            (0..canvas.rows).any(|row| canvas.get(column, row) != SPACE)
        };
        assert!(drawn(canvas.columns / 2), "nothing down the middle");
        assert!(!drawn(0) && !drawn(canvas.columns - 1), "the picture reached the edges");
    }

    #[test]
    fn a_still_has_no_loop() {
        let path = written("asciiary-gradient.png", &gradient(320, 160));
        assert_eq!(media(&path, &[]).loop_duration(), None);
    }

    #[test]
    fn a_file_that_is_not_there_says_so() {
        let params = Params {
            positional: vec!["/no/such/picture.png".into()],
            ..Params::default()
        };
        let message = assemble(&params).err().expect("nothing was read");
        assert!(message.contains("cannot read"), "{message}");
    }
}
