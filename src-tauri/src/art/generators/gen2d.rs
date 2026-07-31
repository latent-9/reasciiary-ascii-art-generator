//! Flow fields and noise: a tool that draws rather than lifts.
//!
//! New — nothing in the Swift original worked this way. Every other tool here
//! starts from something that already exists, a drawing or a solid or a file,
//! and asks what it looks like. This one starts from a field of angles and
//! follows it, which is the oldest generative sketch there is and still the one
//! that gives the most back for the least.
//!
//! It draws with `tiny-skia` into a raster the size of the grid's own sub-cell
//! sampling, and then hands that to [`crate::art::read`] — so a stroke a fifth
//! of a cell wide is not lost, it comes back as the mark whose ink runs the same
//! way the stroke does. Drawing coarsely straight into cells cannot do that: a
//! curve crossing a cell would be one character chosen by how much of the cell
//! it covered, and every diagonal would read as the same grey smudge.
//!
//! The loop is exact and it is Bleuje's method for it: the field is sampled on a
//! *circle* through the noise rather than along a line, so a period brings it
//! back to itself exactly, and every stroke fades in and out on its own offset
//! phase so nothing has to jump back to a start.
//!
//! [ref]: https://github.com/Bleuje/processing-animations-code

use std::f64::consts::TAU;

use image::RgbaImage;
use noise::Perlin;
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::art::canvas::AsciiCanvas;
use crate::art::generator::{Generator, GlyphGenerator};
use crate::art::motion::{circle_noise, scatter, swell};
use crate::art::params::Params;
use crate::art::read::{fine_size, Reader};

use super::paper::hue;

/// How wide a circle through the noise one period travels.
///
/// This is the whole trick and it is worth being plain about: a field animated
/// by walking a straight line through noise never returns, so it cannot loop. A
/// field animated by walking a circle returns exactly, at no cost — the same
/// noise, sampled at two more coordinates. Small and the loop barely moves;
/// large and it churns.
const TIME_REACH: f64 = 0.38;

/// Octaves of noise in the field.
///
/// Two is enough to be interesting and cheap enough that a frame is not worth
/// caching. The flow style is not sensitive to it — a streamline integrates the
/// field, and integration is a low pass — so this is really a setting for the
/// noise style.
const OCTAVES: usize = 3;

/// How much of the frame a whole streamline crosses. The drawn part is a
/// fraction of that; see [`Flow::tail`].
const REACH: f64 = 0.62;

/// Which of the two things this tool does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Style {
    /// Strokes that follow the field, each travelling its length on its own
    /// phase.
    Flow,
    /// The field itself, as tone.
    Noise,
}

impl Style {
    fn named(name: &str) -> Result<Self, String> {
        match name {
            "flow" => Ok(Self::Flow),
            "noise" => Ok(Self::Noise),
            other => Err(format!("`{other}` is not a style — try flow or noise")),
        }
    }
}

pub struct Flow {
    style: Style,
    field: Perlin,
    /// Strokes in the field.
    lines: usize,
    /// Points each one is traced through.
    steps: usize,
    /// How fine the field is. Large is a churn, small is a slow curl.
    grain: f64,
    /// How far the field's angle can swing. One is a whole turn.
    swirl: f64,
    seed: u64,
    period: f64,
    still: bool,
    reader: Reader,
}

impl Flow {
    /// How much of a streamline is drawn at once.
    fn tail(&self) -> usize {
        (self.steps / 4).max(2)
    }

    /// The field's angle at a point, on the circle through the noise that
    /// `phase` runs round.
    fn angle(&self, x: f64, y: f64, phase: f64) -> f64 {
        let mut level = 0.0;
        let mut reach = 1.0;
        let mut weight = 1.0;
        let mut total = 0.0;
        for _ in 0..OCTAVES {
            level += weight
                * circle_noise(
                    &self.field,
                    x * self.grain * reach,
                    y * self.grain * reach,
                    phase,
                    TIME_REACH * reach,
                );
            total += weight;
            reach *= 2.0;
            weight *= 0.5;
        }
        level / total * TAU * self.swirl
    }

    /// One stroke, from where it starts to where it ends.
    ///
    /// The whole line is traced even though a quarter of it is drawn: where the
    /// drawn part is depends on the phase, and a streamline has to be followed
    /// from its start to know where its middle is.
    fn trace(&self, start: (f64, f64), phase: f64) -> Vec<(f64, f64)> {
        let stride = REACH / self.steps as f64;
        let mut at = start;
        let mut points = Vec::with_capacity(self.steps);
        for _ in 0..self.steps {
            points.push(at);
            let (sin, cos) = self.angle(at.0, at.1, phase).sin_cos();
            at = (at.0 + cos * stride, at.1 + sin * stride);
        }
        points
    }

    /// The frame, as a picture, before it is read back as characters.
    fn draw(&self, wide: u32, tall: u32, phase: f64) -> Option<Pixmap> {
        let mut pixmap = Pixmap::new(wide.max(1), tall.max(1))?;
        pixmap.fill(Color::BLACK);
        match self.style {
            Style::Flow => self.draw_flow(&mut pixmap, phase),
            Style::Noise => self.draw_noise(&mut pixmap, phase),
        }
        Some(pixmap)
    }

    fn draw_flow(&self, pixmap: &mut Pixmap, phase: f64) {
        let (wide, tall) = (pixmap.width() as f64, pixmap.height() as f64);
        // World units: a unit tall, and as wide as the frame is.
        let across = wide / tall;
        let onto = |point: (f64, f64)| {
            (
                ((point.0 + across / 2.0) / across * wide) as f32,
                ((point.1 + 0.5) * tall) as f32,
            )
        };

        let tail = self.tail();
        let travel = self.steps.saturating_sub(tail) as f64;
        let mut stroke = Stroke { width: 1.4, ..Stroke::default() };
        stroke.line_cap = tiny_skia::LineCap::Round;

        for line in 0..self.lines {
            // Its own place in the loop, so the field is never all in the same
            // part of its cycle at once — which is what a loop looks like when
            // it is one thing, and what a *field* looks like when it is not.
            let offset = scatter(self.seed, line as u64 * 3 + 2);
            let progress = (phase + offset).rem_euclid(1.0);

            let start = (
                (scatter(self.seed, line as u64 * 3) - 0.5) * across,
                scatter(self.seed, line as u64 * 3 + 1) - 0.5,
            );
            let points = self.trace(start, phase);
            let head = (progress * travel) as usize;

            let mut path = PathBuilder::new();
            for (step, point) in points[head..(head + tail).min(points.len())].iter().enumerate() {
                let (x, y) = onto(*point);
                if step == 0 {
                    path.move_to(x, y);
                } else {
                    path.line_to(x, y);
                }
            }
            let Some(path) = path.finish() else {
                continue;
            };

            // In and out over the loop, so the jump back to the start of the
            // line happens while there is nothing on screen to jump.
            let alpha = swell(progress) as f32;
            let tint = if self.reader.colored { hue(offset) } else { [1.0; 3] };
            let mut paint = Paint { anti_alias: true, ..Paint::default() };
            paint.set_color(
                Color::from_rgba(tint[0], tint[1], tint[2], alpha).unwrap_or(Color::WHITE),
            );
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }

    fn draw_noise(&self, pixmap: &mut Pixmap, phase: f64) {
        let (wide, tall) = (pixmap.width(), pixmap.height());
        let across = wide as f64 / tall as f64;
        let data = pixmap.pixels_mut();
        for row in 0..tall {
            let y = (row as f64 + 0.5) / tall as f64 - 0.5;
            for column in 0..wide {
                let x = ((column as f64 + 0.5) / wide as f64 - 0.5) * across;
                // The same field the strokes follow, shown rather than followed:
                // its angle is a turn, so a turn's worth of it is a tone.
                let level = self.angle(x, y, phase) / (TAU * self.swirl.max(1e-9));
                let tone = (0.5 + level).clamp(0.0, 1.0) as f32;
                data[(row * wide + column) as usize] =
                    Color::from_rgba(tone, tone, tone, 1.0)
                        .unwrap_or(Color::BLACK)
                        .premultiply()
                        .to_color_u8();
            }
        }
    }
}

impl GlyphGenerator for Flow {
    fn canvas(&self, columns: usize, rows: usize, time: f64) -> AsciiCanvas {
        if columns == 0 || rows == 0 {
            return AsciiCanvas::new(columns, rows, self.reader.colored);
        }
        let phase = if self.still { 0.0 } else { (time / self.period).rem_euclid(1.0) };
        let (wide, tall) = fine_size(columns, rows);
        let Some(pixmap) = self.draw(wide, tall, phase) else {
            return AsciiCanvas::new(columns, rows, self.reader.colored);
        };
        let Some(picture) = RgbaImage::from_raw(wide, tall, pixmap.data().to_vec()) else {
            return AsciiCanvas::new(columns, rows, self.reader.colored);
        };
        self.reader.canvas(&picture)
    }

    fn loop_duration(&self) -> Option<f64> {
        (!self.still).then_some(self.period)
    }
}

pub fn build(params: &Params) -> Result<Generator, String> {
    let period = params.f64("period", 8.0)?;
    if period <= 0.0 {
        return Err("--period is how many seconds a loop takes, so it has to be positive".into());
    }
    let seed = params.seed(7)?;

    Ok(Generator::Glyph(Box::new(Flow {
        style: Style::named(params.string("style").unwrap_or("flow"))?,
        field: Perlin::new(seed as u32),
        // Enough that the field reads as a field. Each stroke is only on screen
        // for part of the loop, so the count on the grid at any moment is about
        // half of this.
        lines: params.usize("lines", 640)?.clamp(1, 4000),
        steps: params.usize("steps", 120)?.clamp(4, 1000),
        grain: params.f64("grain", 1.3)?.clamp(0.05, 40.0),
        swirl: params.f64("swirl", 1.0)?.clamp(0.02, 8.0),
        seed,
        period,
        still: params.is_set("still"),
        reader: Reader::from_params(params)?,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::canvas::SPACE;

    fn flow(style: Style) -> Flow {
        Flow {
            style,
            field: Perlin::new(7),
            lines: 200,
            steps: 90,
            grain: 1.7,
            swirl: 1.0,
            seed: 7,
            period: 8.0,
            still: false,
            reader: Reader::from_params(&Params::default()).expect("the defaults read"),
        }
    }

    fn inked(canvas: &AsciiCanvas) -> usize {
        canvas.glyphs.iter().filter(|&&glyph| glyph != SPACE).count()
    }

    /// Both styles have to put marks on the grid without filling it: one that
    /// covers everything is as empty a picture as one that covers nothing.
    #[test]
    fn each_style_draws_a_part_of_the_frame() {
        for style in [Style::Flow, Style::Noise] {
            let canvas = flow(style).canvas(80, 24, 2.0);
            let drawn = inked(&canvas);
            assert!(drawn > 120, "{style:?} drew only {drawn} cells");
            assert!(drawn < canvas.glyphs.len(), "{style:?} filled the whole grid");
        }
    }

    /// The point of sampling the noise on a circle: a period later the frame is
    /// the frame it started as, so an export meets itself.
    #[test]
    fn a_period_brings_the_frame_back_to_itself() {
        for style in [Style::Flow, Style::Noise] {
            let flow = flow(style);
            let start = flow.canvas(60, 20, 0.0);
            let round = flow.canvas(60, 20, flow.period);
            assert_eq!(start.glyphs, round.glyphs, "{style:?} does not close its loop");
        }
    }

    /// And nothing in the middle of the loop is where it started, or the field
    /// is standing still.
    #[test]
    fn the_field_moves_between_one_period_and_the_next() {
        for style in [Style::Flow, Style::Noise] {
            let flow = flow(style);
            let start = flow.canvas(60, 20, 0.0);
            let middle = flow.canvas(60, 20, flow.period / 2.0);
            let moved = start
                .glyphs
                .iter()
                .zip(&middle.glyphs)
                .filter(|(one, other)| one != other)
                .count();
            assert!(moved > 60, "{style:?} moved only {moved} cells");
        }
    }

    /// A seed is a promise that the same line can be typed twice.
    #[test]
    fn the_same_seed_draws_the_same_frame() {
        let one = flow(Style::Flow).canvas(60, 20, 1.5);
        let same = flow(Style::Flow).canvas(60, 20, 1.5);
        assert_eq!(one.glyphs, same.glyphs);

        let mut other = flow(Style::Flow);
        other.seed = 8;
        assert_ne!(one.glyphs, other.canvas(60, 20, 1.5).glyphs);
    }

    #[test]
    fn an_unknown_style_says_what_there_is() {
        let message = Style::named("swirls").unwrap_err();
        assert!(message.contains("flow"), "{message}");
    }
}
