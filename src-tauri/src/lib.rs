pub mod art;
pub mod repl;

use std::collections::HashMap;

use art::export::{Format, Settings};
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
    Ok(Settings {
        format,
        columns: params.usize("columns", 160)?,
        rows: params.usize("rows", 48)?,
        frames_per_second: params.usize("fps", 20)? as u32,
        duration: params.f64("duration", 4.0)?,
        scale: params.f64("scale", 2.0)?,
    })
}

/// One frame as text, for the live preview. Cheap enough to run on every slider
/// move — no rasterizing, no ffmpeg.
#[tauri::command]
fn preview(request: Request, time: f64) -> Result<String, String> {
    let (generator, _, params) = request.build()?;
    let columns = params.usize("columns", 160)?;
    let rows = params.usize("rows", 48)?;
    match generator {
        Generator::Glyph(generator) => Ok(generator.canvas(columns, rows, time).text()),
        Generator::Pixel(_) => Err("this tool draws pixels, not text".into()),
    }
}

/// How long one loop of the current settings runs, so the preview can spin at
/// the rate the export will.
#[tauri::command]
fn loop_duration(request: Request) -> Result<Option<f64>, String> {
    let (generator, _, _) = request.build()?;
    Ok(generator.loop_duration())
}

#[tauri::command]
fn render_art(request: Request) -> Result<String, String> {
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
        .invoke_handler(tauri::generate_handler![preview, loop_duration, render_art])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
