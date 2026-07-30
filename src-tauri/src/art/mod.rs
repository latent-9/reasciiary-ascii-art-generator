pub mod canvas;
pub mod export;
pub mod filter;
pub mod generator;
pub mod glyphs;
pub mod paint;
pub mod params;

pub mod filters;
pub mod generators;

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command as Process, Stdio};

use image::RgbaImage;
use rayon::prelude::*;

use canvas::AsciiColor;
use export::{Format, Settings};
use filter::Filter;
use generator::Generator;
use paint::Painter;

pub const FONT: &[u8] = include_bytes!("../../assets/JetBrainsMono-Regular.ttf");

/// The size the cell grid is measured at before the export scale is applied.
pub const BASE_FONT_SIZE: f32 = 14.0;

const FOREGROUND: AsciiColor = AsciiColor { red: 231, green: 231, blue: 231 };
const BACKGROUND: AsciiColor = AsciiColor { red: 12, green: 12, blue: 14 };

/// One parsed command line: a source, the filters it flows through, and where
/// the result lands.
pub struct Pipeline {
    pub generator: Generator,
    pub filters: Vec<Filter>,
    pub output: PathBuf,
}

pub fn render(pipeline: &Pipeline, settings: &Settings) -> Result<PathBuf, String> {
    if settings.format == Format::Text {
        return write_text(pipeline, settings);
    }

    let font = fontdue::Font::from_bytes(FONT, fontdue::FontSettings::default())
        .map_err(|error| format!("bundled font failed to load: {error}"))?;

    // The scale is resolved against the unscaled cell width, then the painter is
    // rebuilt at the size that scale implies — so a GIF's width cap lands on the
    // font size rather than on a resampled bitmap.
    let base = Painter::new(&font, BASE_FONT_SIZE);
    let scale = settings.resolved_scale(base.cell_width as f64);
    let painter = Painter::new(&font, BASE_FONT_SIZE * scale as f32);

    let size = painter.size_of(settings.columns, settings.rows);
    let size = if settings.format == Format::Mp4 {
        export::even_dimensions(size.0, size.1)
    } else {
        size
    };

    match settings.format {
        Format::Text => unreachable!("handled above"),
        Format::Png => {
            let frame = draw_frame(pipeline, &painter, settings, size, 0.0);
            frame
                .save(&pipeline.output)
                .map_err(|error| format!("cannot write {}: {error}", pipeline.output.display()))?;
        }
        Format::Gif | Format::Mp4 => stream(pipeline, &painter, settings, size)?,
    }

    Ok(pipeline.output.clone())
}

/// Renders a batch at a time and hands each straight to ffmpeg.
///
/// Collecting every frame first meant a thirty-second export held several
/// gigabytes of bitmaps at once, and writing them out as PNGs first cost more
/// than the render did. Peak memory here is one batch.
fn stream(
    pipeline: &Pipeline,
    painter: &Painter,
    settings: &Settings,
    size: (u32, u32),
) -> Result<(), String> {
    let times = frame_times(pipeline, settings);
    let output = pipeline.output.to_string_lossy().into_owned();
    let fps = settings.frames_per_second;

    let args = match settings.format {
        Format::Mp4 => export::mp4_args(size.0, size.1, fps, &output),
        Format::Gif => export::gif_args(size.0, size.1, fps, &output),
        _ => unreachable!("only the animated formats stream"),
    };

    let mut child = Process::new(export::encoder()?)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("ffmpeg could not be run: {error}"))?;

    let mut sink = child.stdin.take().expect("stdin was piped");
    let batch = rayon::current_num_threads().max(1) * 2;
    let mut broken = false;

    for chunk in times.chunks(batch) {
        let frames: Vec<RgbaImage> = chunk
            .par_iter()
            .map(|time| draw_frame(pipeline, painter, settings, size, *time))
            .collect();

        for frame in frames {
            // A write failing means ffmpeg already gave up; its stderr says why,
            // so stop feeding it and let the exit status carry the message.
            if sink.write_all(frame.as_raw()).is_err() {
                broken = true;
                break;
            }
        }
        if broken {
            break;
        }
    }
    drop(sink);

    let finished = child
        .wait_with_output()
        .map_err(|error| format!("ffmpeg could not be waited on: {error}"))?;
    if finished.status.success() {
        return Ok(());
    }

    // ffmpeg puts the actual complaint in the last few lines of a long banner.
    let stderr = String::from_utf8_lossy(&finished.stderr);
    let tail: Vec<&str> = stderr.lines().rev().take(4).collect();
    Err(format!(
        "ffmpeg failed: {}",
        tail.into_iter().rev().collect::<Vec<_>>().join(" / ")
    ))
}

/// Sampling spans a whole number of `loop_duration`s and excludes the endpoint,
/// which is what makes a periodic generator loop seamlessly without knowing it
/// is being exported. [`export::whole_loops`] is why it is a whole number rather
/// than exactly one.
fn frame_times(pipeline: &Pipeline, settings: &Settings) -> Vec<f64> {
    let count = settings.frame_count();
    match pipeline.generator.loop_duration() {
        Some(period) if settings.format.is_animated() => {
            let span = period * export::whole_loops(period, settings.duration) as f64;
            (0..count)
                .map(|index| index as f64 / count as f64 * span)
                .collect()
        }
        _ => (0..count)
            .map(|index| index as f64 / settings.frames_per_second as f64)
            .collect(),
    }
}

fn draw_frame(
    pipeline: &Pipeline,
    painter: &Painter,
    settings: &Settings,
    size: (u32, u32),
    time: f64,
) -> RgbaImage {
    let mut image = match &pipeline.generator {
        Generator::Glyph(generator) => {
            let mut canvas = generator.canvas(settings.columns, settings.rows, time);
            for filter in &pipeline.filters {
                if let Filter::Glyph(filter) = filter {
                    filter.apply(&mut canvas, time);
                }
            }
            painter.draw(&canvas, FOREGROUND, BACKGROUND, size)
        }
        Generator::Pixel(generator) => generator.frame(size.0, size.1, time),
    };

    for filter in &pipeline.filters {
        if let Filter::Pixel(filter) = filter {
            filter.apply(&mut image, time);
        }
    }
    image
}

fn write_text(pipeline: &Pipeline, settings: &Settings) -> Result<PathBuf, String> {
    let Generator::Glyph(generator) = &pipeline.generator else {
        return Err("a pixel tool has no text form".into());
    };
    let canvas = generator.canvas(settings.columns, settings.rows, 0.0);
    std::fs::write(&pipeline.output, canvas.text() + "\n")
        .map_err(|error| format!("cannot write {}: {error}", pipeline.output.display()))?;
    Ok(pipeline.output.clone())
}
