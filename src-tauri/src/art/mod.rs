pub mod canvas;
pub mod export;
pub mod filter;
pub mod generator;
pub mod glyphs;
pub mod motion;
pub mod paint;
pub mod params;
pub mod raster;
pub mod read;
pub mod surface;

pub mod filters;
pub mod generators;

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command as Process, Stdio};

use image::RgbaImage;
use rayon::prelude::*;

use export::{Format, Settings};
use filter::Filter;
use generator::Generator;
use paint::Painter;

pub const FONT: &[u8] = include_bytes!("../../assets/JetBrainsMono-Regular.ttf");

/// The size the cell grid is measured at before the export scale is applied.
pub const BASE_FONT_SIZE: f32 = 14.0;

/// One parsed command line: a source, the filters it flows through, and where
/// the result lands.
pub struct Pipeline {
    pub generator: Generator,
    pub filters: Vec<Filter>,
    pub output: PathBuf,
}

/// Refuses an output whose file name begins with `-`.
///
/// The animated formats hand this path to ffmpeg as its final argument, and
/// ffmpeg reads its arguments positionally: a name like `-y.mp4` is taken for
/// the overwrite flag, and a carefully chosen one could set encoder options
/// rather than name a file. Nothing legitimate starts a file name with a dash,
/// so the whole class is turned away before a renderer or a process is started —
/// the still formats are held to the same rule so the message is the same
/// wherever the name came from.
fn reject_option_like_output(output: &std::path::Path) -> Result<(), String> {
    let named_like_option = matches!(
        output.file_name().and_then(|name| name.to_str()),
        Some(name) if name.starts_with('-')
    );
    if named_like_option {
        return Err(format!(
            "output `{}` starts its name with `-`, which an encoder reads as an \
             option rather than a file; rename it",
            output.display()
        ));
    }
    Ok(())
}

pub fn render(pipeline: &Pipeline, settings: &Settings) -> Result<PathBuf, String> {
    reject_option_like_output(&pipeline.output)?;

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
            let step = frame_step(pipeline, settings);
            let frame = draw_frame(pipeline, &painter, settings, size, 0.0, step);
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
    let step = frame_step(pipeline, settings);
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
            .map(|time| draw_frame(pipeline, painter, settings, size, *time, step))
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

/// How much generator time one written frame stands for.
///
/// A loop is not sampled at the rate that was asked for: it is stretched or
/// squeezed so that a whole number of loops lands exactly on the frames there
/// are — see [`frame_times`] — and the shutter has to be told the same, or a
/// smear covers something other than the gap it is filling.
fn frame_step(pipeline: &Pipeline, settings: &Settings) -> f64 {
    match pipeline.generator.loop_duration() {
        Some(period) if settings.format.is_animated() => {
            let span = period * export::whole_loops(period, settings.duration) as f64;
            span / settings.frame_count() as f64
        }
        _ => 1.0 / settings.frames_per_second.max(1) as f64,
    }
}

/// Sampling spans a whole number of `loop_duration`s and excludes the endpoint,
/// which is what makes a periodic generator loop seamlessly without knowing it
/// is being exported. [`export::whole_loops`] is why it is a whole number rather
/// than exactly one.
fn frame_times(pipeline: &Pipeline, settings: &Settings) -> Vec<f64> {
    let step = frame_step(pipeline, settings);
    (0..settings.frame_count())
        .map(|index| index as f64 * step)
        .collect()
}

/// One written frame, exposed over `step` seconds rather than caught at `time`.
///
/// The shutter opens at the frame's own time and runs forward, which is the
/// convention the sketches this follows use and the one that keeps an
/// unblurred export frame-for-frame identical to a blurred one's first sample.
///
/// The samples are averaged as they are stored rather than in light. Averaging
/// in light is the truer answer for a photograph, but this is ink laid on paper:
/// a trail averaged in light comes out brighter than either the ink or the paper
/// it lies between, and a drift of fine particles turns into a glow.
fn draw_frame(
    pipeline: &Pipeline,
    painter: &Painter,
    settings: &Settings,
    size: (u32, u32),
    time: f64,
    step: f64,
) -> RgbaImage {
    let samples = settings.samples.max(1);
    let mut exposed = one_sample(pipeline, painter, settings, size, time);
    if samples == 1 {
        return exposed;
    }

    let open = step * settings.shutter;
    let mut total: Vec<u32> = exposed.as_raw().iter().map(|value| *value as u32).collect();
    for index in 1..samples {
        let at = time + open * index as f64 / samples as f64;
        let sample = one_sample(pipeline, painter, settings, size, at);
        for (sum, value) in total.iter_mut().zip(sample.as_raw()) {
            *sum += *value as u32;
        }
    }

    let half = samples as u32 / 2;
    for (channel, sum) in exposed.iter_mut().zip(&total) {
        *channel = ((sum + half) / samples as u32) as u8;
    }
    exposed
}

fn one_sample(
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
            painter.draw(&canvas, settings.ink, settings.paper, size)
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

#[cfg(test)]
mod tests {
    use super::*;

    use generator::PixelGenerator;
    use image::Rgba;

    /// A frame that is nothing but its own time, so what an exposure did to a
    /// span of them can be read straight off a pixel.
    struct Clock;

    impl PixelGenerator for Clock {
        fn frame(&self, width: u32, height: u32, time: f64) -> RgbaImage {
            let shade = (time * 255.0).round().clamp(0.0, 255.0) as u8;
            RgbaImage::from_pixel(width, height, Rgba([shade, shade, shade, 255]))
        }

        fn loop_duration(&self) -> Option<f64> {
            Some(1.0)
        }
    }

    fn clock() -> Pipeline {
        Pipeline {
            generator: Generator::Pixel(Box::new(Clock)),
            filters: Vec::new(),
            output: PathBuf::from("nowhere.png"),
        }
    }

    /// The exposure never touches the painter, but the signature carries one.
    fn painter() -> Painter {
        let font = fontdue::Font::from_bytes(FONT, fontdue::FontSettings::default())
            .expect("the bundled font loads");
        Painter::new(&font, BASE_FONT_SIZE)
    }

    fn shade(settings: &Settings, time: f64, step: f64) -> u8 {
        draw_frame(&clock(), &painter(), settings, (2, 2), time, step)
            .get_pixel(0, 0)
            .0[0]
    }

    /// One sample is the frame at that moment and nothing either side of it,
    /// which is what every export wrote before there was a shutter at all.
    #[test]
    fn a_single_sample_catches_the_instant() {
        let settings = Settings { samples: 1, shutter: 1.0, ..Settings::default() };
        assert_eq!(shade(&settings, 0.5, 1.0), 128);
    }

    /// And more than one is their average, spread forward over the gap.
    #[test]
    fn an_open_shutter_averages_what_passed_through_it() {
        let settings = Settings { samples: 4, shutter: 1.0, ..Settings::default() };
        // Nought, a quarter, a half and three quarters of a second: 0, 64, 128
        // and 191 as eight-bit shades, which come to 96 rounded.
        assert_eq!(shade(&settings, 0.0, 1.0), 96);
    }

    /// A shutter closed the instant it opens is a still camera again, however
    /// many samples it was asked for.
    #[test]
    fn a_shutter_that_never_opens_leaves_the_frame_sharp() {
        let settings = Settings { samples: 8, shutter: 0.0, ..Settings::default() };
        assert_eq!(shade(&settings, 0.25, 1.0), 64);
    }

    /// The smear covers the gap to the next frame, so it has to be measured the
    /// same way the frames are: a loop squeezed to land on a whole number of
    /// them steps by something other than one over the rate.
    #[test]
    fn the_shutter_is_told_the_step_the_frames_actually_take() {
        let settings = Settings {
            format: Format::Gif,
            frames_per_second: 8,
            duration: 3.0,
            ..Settings::default()
        };
        // Three seconds of a one second loop is three loops over 24 frames.
        let times = frame_times(&clock(), &settings);
        assert_eq!(times.len(), 24);
        let step = frame_step(&clock(), &settings);
        assert!((step - 0.125).abs() < 1e-12, "{step}");
        assert!((times[1] - step).abs() < 1e-12);
    }

    /// A still is one frame, and the gap it stands for is the one the rate
    /// names — there being no next frame to reach.
    #[test]
    fn a_frame_of_something_that_never_loops_steps_by_the_rate() {
        let settings = Settings { format: Format::Png, frames_per_second: 25, ..Settings::default() };
        assert!((frame_step(&clock(), &settings) - 0.04).abs() < 1e-12);
    }

    /// A name whose first character is `-` is refused: ffmpeg takes its output
    /// positionally, so `-y.mp4` would land as its overwrite flag and write no
    /// file. A dash deeper in the path is part of a directory, not the name, so
    /// the whole path still reads as one.
    #[test]
    fn an_output_named_like_an_option_is_refused() {
        assert!(reject_option_like_output(std::path::Path::new("out.mp4")).is_ok());
        assert!(reject_option_like_output(std::path::Path::new("/tmp/-odd/out.gif")).is_ok());

        let error = reject_option_like_output(std::path::Path::new("-y.mp4")).unwrap_err();
        assert!(error.contains("starts its name with `-`"), "{error}");
    }

    /// And the guard sits on the one path every format runs through, so it stops
    /// an export before a frame is drawn or an encoder is spawned.
    #[test]
    fn render_turns_away_an_option_like_output() {
        let pipeline = Pipeline {
            generator: Generator::Pixel(Box::new(Clock)),
            filters: Vec::new(),
            output: PathBuf::from("-y.gif"),
        };
        let settings = Settings { format: Format::Gif, ..Settings::default() };
        let error = render(&pipeline, &settings).unwrap_err();
        assert!(error.contains("starts its name with `-`"), "{error}");
    }
}
