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
//!
//! A drawing is a file this can read too, and the one it has least to do to. It
//! arrives as characters already, so at the grid it was written at it is laid
//! down as it stands — the mark that best fits a cell is the one somebody typed
//! there, and a round trip through the matcher would only answer with the marks
//! it happens to know. Everything the tool is for is still there: the drawing
//! goes out as a PNG, a GIF or an MP4 in the scheme on screen. It is when the
//! grid is not the drawing's own that there is a question worth asking, and
//! then it goes the same way a picture does.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Mutex;

use image::imageops::{crop_imm, resize, FilterType};
use image::{AnimationDecoder, ImageReader, RgbaImage};

use crate::art::canvas::{AsciiCanvas, CELL_ASPECT};
use crate::art::generator::{Generator, GlyphGenerator};
use crate::art::params::Params;
use crate::art::read::{fine_size, is_drawing, raster_of, Marks, Reader};

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
    /// The characters the source was written with, when it was written in
    /// characters at all. Kept beside the raster taken off it rather than
    /// instead of it: one of the two is right at the drawing's own size and the
    /// other at every size but that.
    drawing: Option<AsciiCanvas>,
    /// Seconds the whole sequence takes.
    span: f64,
    fit: Fit,
    reader: Reader,
    sampled: Mutex<Option<Sampled>>,
}

impl Media {
    /// The drawing to lay down as written, if that is what this grid asks for.
    ///
    /// A drawing is already what this tool makes, so at the size it was written
    /// there is nothing to work out. It is only when something else is asked
    /// that it has to be drawn out as light and read back: a grid too small to
    /// hold it, a fit that says fill the grid rather than show all of the
    /// drawing, another set of marks, or a reading that swaps its ends or opens
    /// up its middle. Each of those is a request to redraw it, and the answer to
    /// all of them is the picture path below.
    ///
    /// A grid larger than the drawing is not one of them. Blowing a drawing up
    /// is not the same act as shrinking one — nothing is lost by leaving it at
    /// the size it was written and centring it, and enlarging it would only
    /// restate every character as a coarser guess at itself.
    fn verbatim(&self, columns: usize, rows: usize) -> Option<&AsciiCanvas> {
        let drawing = self.drawing.as_ref()?;
        let asked = self.reader.marks != Marks::Matched || !self.reader.tones.is_plain();
        let fits = self.fit == Fit::Contain && drawing.columns <= columns && drawing.rows <= rows;
        (!asked && fits).then_some(drawing)
    }

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
        if let Some(drawing) = self.verbatim(columns, rows) {
            // Monochrome whatever `--color` says: a file of characters carries
            // none, so the honest answer is the ink the frame is drawn in.
            let mut canvas = AsciiCanvas::new(columns, rows, false);
            let (left, top) = ((columns - drawing.columns) / 2, (rows - drawing.rows) / 2);
            for row in 0..drawing.rows {
                for column in 0..drawing.columns {
                    canvas.set(left + column, top + row, drawing.get(column, row), None);
                }
            }
            return canvas;
        }

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

    /// The grid a drawing was written at. A picture has none — every size of it
    /// is as much the picture as any other — so the answer is only ever the
    /// drawing's, and it is given whether or not the reading will be redrawn:
    /// asked for other marks, a drawing is still that drawing at that size.
    fn natural_grid(&self) -> Option<(usize, usize)> {
        let drawing = self.drawing.as_ref()?;
        Some((drawing.columns, drawing.rows))
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

/// A drawing, and the light it stands for.
///
/// Both are kept. The characters answer the grid the drawing was written at and
/// the raster answers every other one, and which of the two is wanted is not
/// known until a grid is asked for — the window resizes, and an export is
/// written at a size of its own.
fn drawn(text: &str) -> (AsciiCanvas, Vec<Still>) {
    let drawing = AsciiCanvas::from_text(text);
    let frames = vec![Still { image: raster_of(&drawing), seconds: FRAME_SECONDS }];
    (drawing, frames)
}

fn assemble(params: &Params) -> Result<Media, String> {
    // `--text` carries a drawing inline, which is how the window offers a sample
    // without shipping a file whose path differs between dev and a bundle.
    let (drawing, frames) = match params.string("text") {
        Some(inline) => {
            let (drawing, frames) = drawn(inline);
            (Some(drawing), frames)
        }
        None => {
            let path = params
                .first_positional()
                .ok_or("media needs a drawing or a picture to read")?;
            if is_drawing(path) {
                let text = std::fs::read_to_string(path)
                    .map_err(|error| format!("cannot read `{path}`: {error}"))?;
                let (drawing, frames) = drawn(&text);
                (Some(drawing), frames)
            } else {
                (None, read(path)?)
            }
        }
    };
    let span = frames.iter().map(|still| still.seconds).sum();

    Ok(Media {
        frames,
        drawing,
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

    /// The same tool pointed at a drawing carried inline, which is the path the
    /// window's sample takes.
    fn drawing(text: &str, flags: &[(&str, &str)]) -> Media {
        let mut params = Params::default();
        params.flags.insert("text".into(), Some(text.to_string()));
        for (name, value) in flags {
            params
                .flags
                .insert(name.to_string(), Some(value.to_string()));
        }
        assemble(&params).expect("the tool builds")
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
        for path in ["/no/such/picture.png", "/no/such/drawing.txt"] {
            let params = Params { positional: vec![path.into()], ..Params::default() };
            let message = assemble(&params).err().expect("nothing was read");
            assert!(message.contains("cannot read"), "{message}");
        }
    }

    /// A drawing is already the thing this tool makes, so on a grid that can
    /// hold it there is nothing to decide: what somebody typed is what the frame
    /// shows, centred on whatever it was given.
    #[test]
    fn a_drawing_is_laid_down_as_it_was_written() {
        const ART: &str = " /\\_/\\\n( o.o )\n > ^ <";
        let written = AsciiCanvas::from_text(ART);
        let canvas = drawing(ART, &[]).canvas(21, 9, 0.0);

        let (left, top) = (
            (canvas.columns - written.columns) / 2,
            (canvas.rows - written.rows) / 2,
        );
        for row in 0..written.rows {
            for column in 0..written.columns {
                assert_eq!(
                    canvas.get(left + column, top + row) as char,
                    written.get(column, row) as char,
                    "cell {column},{row} of the drawing"
                );
            }
        }
    }

    /// And a drawing the grid cannot hold is not cut down to it. Characters do
    /// not shrink, so it is drawn out as light and read back — all of it, at the
    /// size that fits.
    #[test]
    fn a_drawing_too_wide_for_the_grid_arrives_whole() {
        let art = vec!["@".repeat(60); 6].join("\n");
        let canvas = drawing(&art, &[]).canvas(24, 12, 0.0);

        let drawn = |column: usize| (0..canvas.rows).any(|row| canvas.get(column, row) != SPACE);
        assert!(drawn(0), "the left of the drawing was cut off");
        assert!(drawn(canvas.columns - 1), "the right of the drawing was cut off");
    }

    /// Asking for a set of marks is asking for the drawing to be drawn again in
    /// them, and that is a question the characters it arrived with cannot
    /// answer.
    #[test]
    fn a_drawing_asked_for_other_marks_is_drawn_again_in_them() {
        const ART: &str = "AAAA\nAAAA";
        assert!(
            drawing(ART, &[]).canvas(20, 8, 0.0).glyphs.contains(&b'A'),
            "the drawing was not laid down as written"
        );

        let graded = drawing(ART, &[("marks", "shades")]).canvas(20, 8, 0.0);
        assert!(!graded.glyphs.contains(&b'A'), "`A` is not one of the shades");
        assert!(
            graded.glyphs.iter().any(|&glyph| glyph != SPACE),
            "nothing was drawn in them either"
        );
    }
}
