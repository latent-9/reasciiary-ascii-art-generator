pub mod art;
pub mod repl;

use std::collections::HashMap;

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
/// The rate is not simply the spin the sliders carry: an export is rounded to a
/// whole number of loops so it can end where it began, and the preview has to
/// be rounded the same way or the window shows a spin nothing ever writes.
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
        })
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
        .invoke_handler(tauri::generate_handler![preview, plan, render_art])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
