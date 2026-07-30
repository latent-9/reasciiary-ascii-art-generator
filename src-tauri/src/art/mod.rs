pub mod canvas;
pub mod export;
pub mod filter;
pub mod generator;
pub mod paint;
pub mod params;

pub mod filters;
pub mod generators;

use std::path::{Path, PathBuf};
use std::process::Command as Process;

use image::RgbaImage;
use rayon::prelude::*;

use canvas::AsciiColor;
use export::{Format, Settings};
use filter::Filter;
use generator::Generator;
use paint::Painter;

const FONT: &[u8] = include_bytes!("../../assets/JetBrainsMono-Regular.ttf");

/// The size the cell grid is measured at before the export scale is applied.
const BASE_FONT_SIZE: f32 = 14.0;

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

    if settings.format == Format::Text {
        return write_text(pipeline, settings);
    }

    let times = frame_times(pipeline, settings);
    let frames: Vec<RgbaImage> = times
        .par_iter()
        .map(|time| draw_frame(pipeline, &painter, settings, size, *time))
        .collect();

    match settings.format {
        Format::Text => unreachable!("handled above"),
        Format::Png => {
            frames[0]
                .save(&pipeline.output)
                .map_err(|error| format!("cannot write {}: {error}", pipeline.output.display()))?;
        }
        Format::Gif | Format::Mp4 => encode(&frames, settings, size, &pipeline.output)?,
    }

    Ok(pipeline.output.clone())
}

/// Sampling spans exactly one `loop_duration` and excludes its endpoint, which
/// is what makes a periodic generator loop seamlessly without knowing it is
/// being exported.
fn frame_times(pipeline: &Pipeline, settings: &Settings) -> Vec<f64> {
    let count = settings.frame_count();
    match pipeline.generator.loop_duration() {
        Some(period) if settings.format.is_animated() => (0..count)
            .map(|index| index as f64 / count as f64 * period)
            .collect(),
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

fn encode(
    frames: &[RgbaImage],
    settings: &Settings,
    size: (u32, u32),
    output: &Path,
) -> Result<(), String> {
    let directory = tempfile::tempdir().map_err(|error| format!("no scratch space: {error}"))?;

    frames
        .par_iter()
        .enumerate()
        .try_for_each(|(index, frame)| {
            let path = directory.path().join(format!("frame_{index:05}.png"));
            frame
                .save(&path)
                .map_err(|error| format!("cannot write frame {index}: {error}"))
        })?;

    let pattern = directory.path().join("frame_%05d.png");
    let pattern = pattern.to_string_lossy().into_owned();
    let out = output.to_string_lossy().into_owned();
    let fps = settings.frames_per_second;

    match settings.format {
        Format::Mp4 => ffmpeg(&export::mp4_args(&pattern, fps, size.0, size.1, &out))?,
        Format::Gif => ffmpeg(&export::gif_args(&pattern, fps, &out))?,
        _ => unreachable!("only the animated formats are encoded"),
    }

    Ok(())
}

fn ffmpeg(args: &[String]) -> Result<(), String> {
    let output = Process::new("ffmpeg")
        .args(args)
        .output()
        .map_err(|error| format!("ffmpeg could not be run: {error}"))?;

    if output.status.success() {
        return Ok(());
    }
    // ffmpeg puts the actual complaint in the last few lines of a long banner.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let tail: Vec<&str> = stderr.lines().rev().take(4).collect();
    Err(format!("ffmpeg failed: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" / ")))
}
