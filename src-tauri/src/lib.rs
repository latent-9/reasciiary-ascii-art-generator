pub mod art;
pub mod repl;

use std::collections::HashMap;

use rayon::prelude::*;
use tauri::Manager;

use art::canvas::AsciiColor;
use art::export::{self, Format, Settings};
use art::generator::Generator;
use art::params::Params;
use art::{filter, generator, Pipeline};

/// What the window sends when a control moves.
///
/// Deliberately shaped like a parsed command line rather than like the ascii
/// tool's own settings: the registry stays generic, so the next tool needs no
/// change here.
#[derive(serde::Deserialize)]
pub struct Request {
    tool: String,
    #[serde(default)]
    positional: Vec<String>,
    /// An empty value means a flag that carries none, like `--still`.
    #[serde(default)]
    flags: HashMap<String, String>,
    #[serde(default)]
    output: Option<String>,
}

impl Request {
    fn params(&self) -> Params {
        Params {
            positional: self.positional.clone(),
            flags: self
                .flags
                .iter()
                .map(|(key, value)| {
                    let value = if value.is_empty() { None } else { Some(value.clone()) };
                    (key.clone(), value)
                })
                .collect(),
        }
    }

    fn build(&self) -> Result<(Generator, Vec<filter::Filter>, Params), String> {
        let params = self.params();
        let factory = generator::lookup(&self.tool)
            .ok_or_else(|| format!("unknown tool `{}`", self.tool))?;
        Ok((factory(&params)?, Vec::new(), params))
    }
}

fn settings_from(params: &Params, format: Format) -> Result<Settings, String> {
    let colour = |name, fallback| match params.string(name) {
        Some(text) => AsciiColor::from_hex(text),
        None => Ok(fallback),
    };

    Ok(Settings {
        format,
        columns: params.usize("columns", 160)?,
        rows: params.usize("rows", 48)?,
        frames_per_second: params.usize("fps", 20)? as u32,
        duration: params.f64("duration", 4.0)?,
        scale: params.f64("scale", 2.0)?,
        ink: colour("ink", export::INK)?,
        paper: colour("paper", export::PAPER)?,
    })
}

/// Runs `work` somewhere other than the thread the window is drawn on.
///
/// A `#[tauri::command]` that is not `async` runs on the main thread, and none
/// of the three below are cheap: each one lifts the whole drawing into a solid,
/// and an export then sits on ffmpeg until it finishes. Left on the main thread
/// a thirty-second GIF freezes the window solid — the elapsed-time counter the
/// window puts up while rendering could not even repaint itself, so the app
/// looked hung for exactly as long as it was working.
async fn off_the_ui_thread<T, F>(work: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| format!("the render thread stopped: {error}"))?
}

/// One frame as text, for the live preview. No rasterizing and no ffmpeg, so
/// this is the cheap one — but only by comparison.
#[tauri::command]
async fn preview(request: Request, time: f64) -> Result<String, String> {
    off_the_ui_thread(move || {
        let (generator, _, params) = request.build()?;
        let columns = params.usize("columns", 160)?;
        let rows = params.usize("rows", 48)?;
        match generator {
            Generator::Glyph(generator) => Ok(generator.canvas(columns, rows, time).text()),
            Generator::Pixel(_) => Err("this tool draws pixels, not text".into()),
        }
    })
    .await
}

/// Everything the window needs to mirror what an export will do, so the preview
/// is the frame that gets written rather than something near it.
///
/// The rate is not simply the loop the sliders carry: an export is rounded to a
/// whole number of loops so it can end where it began, and the preview has to
/// be rounded the same way or the window shows a motion nothing ever writes.
#[derive(serde::Serialize)]
pub struct Plan {
    /// Seconds of generator time in one full loop. `null` for a still.
    period: Option<f64>,
    /// Whole loops an export covers.
    loops: usize,
    /// How long that export runs, in seconds.
    seconds: f64,
    /// Columns per row for a grid the drawing fills. `null` from a tool with no
    /// shape of its own, which the window reads as "any grid will do".
    frame: Option<f64>,
    /// Columns and rows a subject was already made at, when it was. `null` from
    /// anything that draws at the size it is asked for, which is the usual case;
    /// set, it settles the grid and the detail slider has nothing to say.
    grid: Option<(usize, usize)>,
}

#[tauri::command]
async fn plan(request: Request) -> Result<Plan, String> {
    off_the_ui_thread(move || {
        let (generator, _, params) = request.build()?;
        let seconds = params.f64("duration", 4.0)?;
        let period = generator.loop_duration();
        Ok(Plan {
            period,
            loops: period.map_or(1, |period| export::whole_loops(period, seconds)),
            seconds,
            frame: match &generator {
                Generator::Glyph(generator) => generator.frame_aspect(),
                Generator::Pixel(_) => None,
            },
            grid: match &generator {
                Generator::Glyph(generator) => generator.natural_grid(),
                Generator::Pixel(_) => None,
            },
        })
    })
    .await
}

/// One whole loop, rendered in advance.
#[derive(serde::Serialize)]
pub struct Film {
    frames: Vec<String>,
    /// The rate that plays them back over exactly one loop.
    fps: f64,
}

/// Renders every frame of one loop at once, so the window can play a spin
/// instead of chasing it.
///
/// Asking for a frame at a time cannot be smooth, whatever the renderer costs.
/// The window ran a timer, requested the frame for the time it had reached, and
/// dropped the request if the last one had not come back — so a frame that took
/// a little too long did not merely arrive late, it deleted the frames behind
/// it, and the spin stuttered in proportion to how busy the machine was. Nothing
/// about a loop needs that: it is the same short cycle every time round, so it
/// can be rendered once and played from memory at an exact rate, with no round
/// trip in the way of a frame at all.
///
/// The span sampled is `[0, period)` — the frame at `period` is the frame at 0,
/// so a full turn is covered exactly once and the loop closes on itself. That is
/// the rule [`export::whole_loops`] follows, which is what keeps the preview
/// showing the frames an export writes.
#[tauri::command]
async fn sequence(request: Request) -> Result<Film, String> {
    off_the_ui_thread(move || {
        let (generator, _, params) = request.build()?;
        let columns = params.usize("columns", 160)?;
        let rows = params.usize("rows", 48)?;
        let requested = params.usize("fps", 20)?.max(1) as f64;

        let Generator::Glyph(generator) = generator else {
            return Err("this tool draws pixels, not text".into());
        };

        let Some(period) = generator.loop_duration().filter(|period| *period > 0.0) else {
            // A still is a one-frame film. Playing it costs the window nothing,
            // so it does not need a second path through any of this.
            return Ok(Film { frames: vec![generator.canvas(columns, rows, 0.0).text()], fps: 1.0 });
        };

        // Enough that a short loop still reads as motion, few enough that a long
        // one is not a thousand renders nobody asked for. Whatever the count
        // ends up being, the rate is set to match it, so one pass is one loop.
        let count = ((period * requested).round() as usize).clamp(24, 240);
        let frames = (0..count)
            .into_par_iter()
            .map(|index| {
                let time = period * index as f64 / count as f64;
                generator.canvas(columns, rows, time).text()
            })
            .collect();

        Ok(Film { frames, fps: count as f64 / period })
    })
    .await
}

#[tauri::command]
async fn render_art(request: Request) -> Result<String, String> {
    off_the_ui_thread(move || {
        let (generator, filters, params) = request.build()?;
        let output = request
            .output
            .clone()
            .ok_or("choose where the file should go first")?;
        let format = Format::from_path(&output)
            .ok_or("output must end in .mp4, .gif, .png or .txt")?;

        let settings = settings_from(&params, format)?;
        let pipeline = Pipeline { generator, filters, output: output.into() };
        let path = art::render(&pipeline, &settings)?;
        Ok(path.display().to_string())
    })
    .await
}

/// The same pipeline driven by a typed line, for the `asciiary` command.
pub fn run_line(line: &str) -> Result<String, String> {
    let command = repl::parse(line)?;
    let (source, rest) = command.stages.split_first().ok_or("nothing to run")?;

    let factory = generator::lookup(&source.name)
        .ok_or_else(|| format!("unknown tool `{}`", source.name))?;
    let generator = factory(&source.params)?;

    let filters = rest
        .iter()
        .map(|stage| {
            let factory = filter::lookup(&stage.name)
                .ok_or_else(|| format!("unknown filter `{}`", stage.name))?;
            factory(&stage.params)
        })
        .collect::<Result<Vec<_>, String>>()?;

    let output = command
        .output
        .ok_or("add `> name.mp4` to say where the result should go")?;
    let format = Format::from_path(&output)
        .ok_or("output must end in .txt, .png, .gif or .mp4")?;

    let settings = settings_from(&source.params, format)?;
    let pipeline = Pipeline { generator, filters, output: output.into() };
    let path = art::render(&pipeline, &settings)?;
    Ok(format!("wrote {}", path.display()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Started from a shell rather than from the Dock, the window can
            // arrive minimised, and nothing else in the app would ever bring it
            // back: what the machine shows is a menu bar and no window, which
            // cannot be told apart from a launch that died. The one thing that
            // is certain here is that somebody has just asked for this, so it is
            // raised rather than hoped for.
            //
            // Failing to raise a window is not a reason to refuse to start, and
            // an error out of `setup` aborts the launch, so these are dropped.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![preview, plan, sequence, render_art])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
