//! The characters a shaded surface can be drawn with, and how one is picked.
//!
//! The obvious way to choose is a ramp: order some glyphs by weight, take the
//! surface's brightness, index in. That is what `emilwidlund/ASCII` does on the
//! GPU with `" .:,'-^=*+?!|0#X%WM@"`, and it is what this renderer used to do.
//! It cannot do better than one value a cell, so a silhouette comes out as a
//! staircase of whichever glyph happened to be nearest the average.
//!
//! `alecjacobson/ascii3d` does something better, and this follows it. The
//! surface is rasterised *finer* than the grid — 5×10 there, [`CELL_PIXELS`]
//! here — and each cell then takes the character whose own rasterised bitmap
//! looks most like that patch, by least squares over the pixels. Nothing is
//! ordered by weight and nothing is ordered at all: `/` wins a cell because the
//! ink in `/` lies where the light in that cell lies. That is where the sloped
//! edges and the traced silhouettes come from.

use std::sync::LazyLock;

use fontdue::Font;

use super::canvas::SPACE;
use super::FONT;

/// How finely a cell is sampled before a character is chosen for it.
///
/// `ascii3d` matches against 5×10 bitmaps of Courier New Bold. This is the same
/// width, against the shape a cell on this grid actually has — the two differ
/// only because JetBrains Mono's line box is slightly taller than Courier's.
pub const CELL_PIXELS_WIDE: usize = 5;
pub const CELL_PIXELS_TALL: usize = 11;
pub const CELL_PIXELS: usize = CELL_PIXELS_WIDE * CELL_PIXELS_TALL;

/// One patch of surface, sampled at [`CELL_PIXELS`] and ready to be matched.
pub type Cell = [f32; CELL_PIXELS];

/// How much finer than [`CELL_PIXELS`] a glyph is rasterised before being
/// averaged down into one.
///
/// Rasterising straight to a 5×11 bitmap asks the font for a 5-pixel em, and
/// what comes back is mush: the stems of `d`, `b` and `h` all land on the same
/// two pixels. Rasterising large and box-filtering keeps the *proportion* of
/// each cell each stroke covers, which is the quantity being matched.
const SUPERSAMPLE: usize = 8;

/// The printable ASCII range, which is the candidate set `ascii3d` uses too.
/// The space at the head of it is not a candidate: see [`Alphabet`].
const FIRST: u8 = b' ';
const LAST: u8 = b'~';

/// Which of them a cell may actually be drawn with.
///
/// `ascii3d` matches against all ninety-five and is right to: it renders smooth
/// meshes, where a cell that is not flat is nearly always a real edge. A drawing
/// lifted by ink coverage is not smooth — it is terraced, so small faces at
/// different angles meet inside a single cell all over the interior, and the
/// matcher gets asked about far more cells than a mesh would ever produce.
///
/// Answering those with the full range put `g`, `j`, `m`, `p`, `q` and `w` in
/// the middle of the picture, and what the eye does with a row of lowercase
/// letters is read it. They win on ink statistics honestly — a bowl and a
/// descender really do cover the lower half of a cell the way a lit ledge does —
/// but a mark that is busy being a word cannot also be a piece of a surface. So
/// the candidates are the marks that carry a direction and nothing else: every
/// punctuation character, `0` for a round blob, and the upper-case letters drawn
/// as bare strokes. Nothing here spells anything, and every slope, corner and
/// weight the matcher needs is still on offer.
const MARKS: &[u8] = b"!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~0AHIJKLMNTVWXYZ";

/// How much the amount of light in a cell counts against where in the cell it
/// falls, when the two disagree.
///
/// They are measured separately below, so this is the only place their relative
/// worth is decided. Raise it and a lit face grades smoothly but its edges blur
/// into the ramp; lower it and every edge is traced but the faces behind them
/// lose their shading.
const LEVEL_WEIGHT: f32 = 4.0;

struct Template {
    byte: u8,
    /// How much ink the glyph lays down, as a fraction of the most any glyph
    /// can — so it is directly comparable with a cell's brightness.
    weight: f32,
    /// The coverage with its own mean taken out and scaled to unit length:
    /// where the ink sits, with how much of it there is divided back out.
    ///
    /// That division is the point. `/` and `X` are drawn along the same
    /// diagonal and differ only in weight, and a comparison that keeps the
    /// weight in cannot say they are the same shape — it just prefers whichever
    /// one happens to be nearer the cell's brightness, which is the level
    /// question being asked twice.
    shape: Cell,
}

/// Every mark a covered cell can be drawn with.
///
/// A space is deliberately not among them. Whether a cell is background is a
/// question about whether the solid reaches it, and the rasteriser already
/// knows the answer exactly; letting it be decided a second time by whether the
/// cell came out dark is what forced the shading up into the heavy end of the
/// alphabet. The unlit side of a solid had to stay bright enough not to be
/// mistaken for a hole, so a render spanned `+` to `@` — eleven characters that
/// all read as the same grey mass, which is most of why a lit solid did not
/// look like one. With the two questions separated the shading is free to run
/// all the way down to `.` and the form comes back.
pub struct Alphabet {
    /// Ordered lightest to heaviest, which is what lets [`Alphabet::matched`]
    /// stop early.
    templates: Vec<Template>,
    /// Brightness to the ramp character carrying that much ink.
    ramp: Vec<u8>,
}

/// Rasterised once. Ninety-five glyphs at eight times resolution is real work,
/// and every frame of an export would otherwise repeat it.
pub static ALPHABET: LazyLock<Alphabet> = LazyLock::new(Alphabet::rasterize);

impl Alphabet {
    fn rasterize() -> Self {
        let font = Font::from_bytes(FONT, fontdue::FontSettings::default())
            .expect("the bundled font loads");

        let wide = CELL_PIXELS_WIDE * SUPERSAMPLE;
        let tall = CELL_PIXELS_TALL * SUPERSAMPLE;

        // Advance width scales linearly with size, so one measurement gives the
        // size whose cell is exactly `wide` pixels across.
        let probe = font.metrics('0', 100.0).advance_width.max(1.0);
        let size = wide as f32 * 100.0 / probe;
        let ascent = font
            .horizontal_line_metrics(size)
            .expect("monospace font without horizontal line metrics")
            .ascent;

        let coverages: Vec<(u8, Cell)> = (FIRST..=LAST)
            .map(|byte| {
                let mut large = vec![0f32; wide * tall];
                if byte != SPACE {
                    stamp(&font, byte as char, size, ascent, &mut large, wide, tall);
                }
                (byte, reduce(&large, wide))
            })
            .collect();

        // The most ink any one character can put on a cell, averaged over it —
        // nowhere near 1, because the heaviest glyph in ASCII still leaves the
        // gaps inside its own strokes and the space between one cell and the
        // next. Weight is measured against this rather than against a full cell,
        // so that a fully lit patch asks for something a candidate can actually
        // supply. Against 1.0 the whole top of the range collapses onto one
        // character and the brightest part of a render goes flat.
        //
        // It is relative, so every glyph has to be rasterised before any of them
        // can be measured.
        let range = coverages
            .iter()
            .map(|(_, coverage)| mean_of(coverage))
            .fold(0.0, f32::max)
            .max(f32::EPSILON);

        let ramp = grade(&coverages, range);

        let mut templates: Vec<Template> = coverages
            .into_iter()
            .filter(|(byte, _)| MARKS.contains(byte))
            .map(|(byte, coverage)| Template::describe(byte, coverage, range))
            .collect();
        templates.sort_by(|one, other| one.weight.total_cmp(&other.weight));

        Self { templates, ramp }
    }

    /// The character to draw `cell` with, whose samples run from 0 for unlit to
    /// 1 for fully lit.
    ///
    /// `whole` says the solid covers every sample in the cell.
    ///
    /// Which of the two references applies depends on where the cell is, and
    /// neither does the other's job well. Least squares over bitmaps is the only
    /// thing that can find `/` for a sloped edge, but on the flat inside of a
    /// face it has nothing to lock onto: every candidate at roughly the right
    /// weight is wrong by roughly the same amount, so the winner is whichever
    /// glyph happens to be most evenly spread. A ramp is the opposite — it
    /// grades a face properly and cannot represent an edge at all.
    ///
    /// The two were told apart by how much the cell varied inside itself, on the
    /// theory that a cell that varies is an edge. That holds for a smooth mesh.
    /// It does not hold for a drawing lifted by ink coverage, because the solid
    /// is terraced: a cap meets a wall inside cells all over the interior, and
    /// most of them cleared the threshold. The middle of a face came out as a
    /// scattering of `[`, `J`, `V` and `T` — the matcher answering honestly, but
    /// about a staircase rather than about a silhouette, and what the eye does
    /// with a row of letters is read it.
    ///
    /// Coverage settles it exactly and for nothing: the rasteriser already knows
    /// which samples the solid reached. A cell it fills completely is interior,
    /// whatever is happening inside it, and a cell it fills partly is on the
    /// silhouette — which is the only place a traced edge belongs.
    pub fn nearest(&self, cell: &Cell, whole: bool) -> u8 {
        let mean = mean_of(cell);
        if whole {
            return self.graded(mean);
        }
        self.matched(cell, mean)
    }

    /// The ramp character holding as near as possible to `brightness` worth of
    /// ink.
    fn graded(&self, brightness: f32) -> u8 {
        let step = brightness.clamp(0.0, 1.0) * (self.ramp.len() - 1) as f32;
        self.ramp[step.round() as usize]
    }

    /// Picks the glyph closest to the cell in two respects at once: how much
    /// light the cell holds, and where in the cell it sits.
    ///
    /// `ascii3d` asks both at once, as one least squares over the raw bitmaps,
    /// and that does not survive the move to a shaded surface. A sum of squares
    /// splits into a level part and a shape part — `Σ (t - c)² = N (t̄ - c̄)² +
    /// Σ ((t - t̄) - (c - c̄))²` — and the two parts are not on the same scale.
    /// A glyph's strokes are almost opaque while a lit patch of surface is
    /// rarely more than half bright, so the shape part is dominated by the
    /// glyph's own contrast and reads every candidate as far too dark; the
    /// cheapest answer becomes whichever glyph is faintest, whatever the cell
    /// looks like. Taking each side's own level out first and comparing what is
    /// left as directions puts shape on a scale of its own, and leaves the level
    /// question to be asked once, plainly, against a [`Template::weight`].
    /// Scoring all ninety-four is most of what a frame costs, and almost all of
    /// it is wasted: `alignment` is a dot product of two unit vectors, so
    /// `1 - alignment` is never negative and the level term alone is already a
    /// lower bound on a candidate's score. Templates run lightest to heaviest,
    /// so walking outward from the cell's own brightness takes them in order of
    /// that bound — and the first one whose bound alone exceeds the best score
    /// so far ends the search, because everything still unvisited lies further
    /// out and scores at least as badly. The answer is the same one the full
    /// sweep gives; only the work is smaller.
    fn matched(&self, cell: &Cell, mean: f32) -> u8 {
        let Some(shape) = direction_of(cell, mean) else {
            return self.graded(mean);
        };

        let mut best = self.graded(mean);
        let mut best_score = f32::INFINITY;

        let start = self.templates.partition_point(|template| template.weight < mean);
        let (mut lighter, mut heavier) = (start, start);
        while lighter > 0 || heavier < self.templates.len() {
            // Whichever of the two frontiers is nearer the cell's brightness,
            // so candidates arrive in order of their lower bound.
            let take_heavier = match (lighter > 0, heavier < self.templates.len()) {
                (true, true) => {
                    self.templates[heavier].weight - mean
                        <= mean - self.templates[lighter - 1].weight
                }
                (_, only_heavier) => only_heavier,
            };
            let index = if take_heavier { heavier } else { lighter - 1 };
            if take_heavier {
                heavier += 1;
            } else {
                lighter -= 1;
            }

            let template = &self.templates[index];
            let level = template.weight - mean;
            let floor = LEVEL_WEIGHT * level * level;
            if floor >= best_score {
                break;
            }

            let mut alignment = 0.0;
            for (sample, ink) in shape.iter().zip(&template.shape) {
                alignment += sample * ink;
            }
            let score = 1.0 - alignment + floor;
            if score < best_score {
                best_score = score;
                best = template.byte;
            }
        }
        best
    }
}

fn mean_of(cell: &Cell) -> f32 {
    cell.iter().sum::<f32>() / CELL_PIXELS as f32
}

/// The cell with its level taken out and what is left scaled to unit length, or
/// `None` if there was nothing left — a patch of one flat value has no shape to
/// point at, and normalising it would be a division by zero.
fn direction_of(cell: &Cell, mean: f32) -> Option<Cell> {
    let mut shape = *cell;
    let mut length = 0.0;
    for sample in shape.iter_mut() {
        *sample -= mean;
        length += *sample * *sample;
    }

    let length = length.sqrt();
    if length < f32::EPSILON {
        return None;
    }
    for sample in shape.iter_mut() {
        *sample /= length;
    }
    Some(shape)
}

/// `emilwidlund/ASCII`'s default set, which is its whole technique: characters
/// "in brightness order dark -> light", indexed by brightness. Short enough that
/// consecutive steps are visibly different, which the ninety-four ordered by
/// coverage are not — that ordering puts `%`, `&`, `M` and `W` beside each
/// other, and a face graded through them reads as noise.
///
/// Its leading space is missing here because background is settled by coverage;
/// see [`Alphabet`].
const RAMP: &[u8] = b".:,'-^=*+?!|0#X%WM@";

/// How finely brightness is resolved before it is turned into a character.
/// Well past what nineteen characters can distinguish, so the table is exact
/// wherever two of them are close together.
const RAMP_STEPS: usize = 256;

/// Brightness to ramp character, by how much ink each one actually lays down in
/// this font.
///
/// [`RAMP`] is ordered by eye against whatever face its author was looking at,
/// and its steps are not evenly spaced in any case — `'` and `-` are nearly the
/// same weight while `|` to `0` is a jump. Indexing it by position therefore
/// spends the same range of brightness on each, which is a ramp that lies about
/// its own middle: a face at half brightness comes out at whatever the tenth
/// character happens to weigh. Every glyph has just been measured, so ask for
/// the one whose ink is nearest instead and a shade means what it says. Any
/// character the ordering had out of place is fixed by the same stroke.
fn grade(coverages: &[(u8, Cell)], range: f32) -> Vec<u8> {
    let ink: Vec<(u8, f32)> = RAMP
        .iter()
        .map(|&byte| {
            let coverage = coverages
                .iter()
                .find(|(candidate, _)| *candidate == byte)
                .map_or(0.0, |(_, coverage)| mean_of(coverage));
            (byte, coverage / range)
        })
        .collect();

    (0..RAMP_STEPS)
        .map(|step| {
            let brightness = step as f32 / (RAMP_STEPS - 1) as f32;
            ink.iter()
                .min_by(|one, other| {
                    (one.1 - brightness).abs().total_cmp(&(other.1 - brightness).abs())
                })
                .map_or(SPACE, |(byte, _)| *byte)
        })
        .collect()
}

/// Draws one glyph into `target` where [`super::paint::Painter`] would put it,
/// so a matched character lands on the exported frame in the position it was
/// matched at.
fn stamp(
    font: &Font,
    character: char,
    size: f32,
    ascent: f32,
    target: &mut [f32],
    wide: usize,
    tall: usize,
) {
    let (metrics, alpha) = font.rasterize(character, size);
    let left = metrics.xmin as i64;
    let top = (ascent - metrics.ymin as f32 - metrics.height as f32).round() as i64;

    for y in 0..metrics.height {
        let row = top + y as i64;
        if row < 0 || row >= tall as i64 {
            continue;
        }
        for x in 0..metrics.width {
            let column = left + x as i64;
            if column < 0 || column >= wide as i64 {
                continue;
            }
            target[row as usize * wide + column as usize] =
                alpha[y * metrics.width + x] as f32 / 255.0;
        }
    }
}

/// Box-filters a supersampled glyph down to one cell's worth of coverage.
fn reduce(large: &[f32], wide: usize) -> Cell {
    let mut coverage = [0f32; CELL_PIXELS];
    let area = (SUPERSAMPLE * SUPERSAMPLE) as f32;

    for (index, sample) in coverage.iter_mut().enumerate() {
        let left = (index % CELL_PIXELS_WIDE) * SUPERSAMPLE;
        let top = (index / CELL_PIXELS_WIDE) * SUPERSAMPLE;
        let mut total = 0.0;
        for y in 0..SUPERSAMPLE {
            for x in 0..SUPERSAMPLE {
                total += large[(top + y) * wide + left + x];
            }
        }
        *sample = total / area;
    }

    coverage
}

impl Template {
    fn describe(byte: u8, coverage: Cell, range: f32) -> Self {
        let mean = mean_of(&coverage);
        Self {
            byte,
            weight: mean / range,
            // A space, and only a space, has no direction. Zero aligns with
            // nothing, which is what leaves it competing on level alone.
            shape: direction_of(&coverage, mean).unwrap_or([0.0; CELL_PIXELS]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::canvas::CELL_ASPECT;

    /// The sub-cell grid stands in for a cell, so it has to be that cell's
    /// shape. Were it not, every render would come out stretched by the
    /// difference without anything else noticing.
    #[test]
    fn a_sampled_cell_has_the_shape_of_a_cell() {
        let aspect = CELL_PIXELS_TALL as f64 / CELL_PIXELS_WIDE as f64;
        assert!(
            (aspect - CELL_ASPECT).abs() < 0.01,
            "cells are sampled {CELL_PIXELS_WIDE}x{CELL_PIXELS_TALL}, an aspect \
             of {aspect}, but the grid is modelled at {CELL_ASPECT}"
        );
    }

    /// The alphabet is asked only about cells the solid covers, so the darkest
    /// answer it can give is the faintest mark rather than nothing at all. A
    /// space here would punch a hole through the middle of a silhouette.
    #[test]
    fn even_an_unlit_cell_gets_a_mark() {
        assert_eq!(ALPHABET.nearest(&[0.0; CELL_PIXELS], true), b'.');
        assert!(ALPHABET
            .ramp
            .iter()
            .chain(ALPHABET.templates.iter().map(|template| &template.byte))
            .all(|&byte| byte != SPACE));
    }

    /// The whole reason the ramp is measured rather than indexed: brightness has
    /// to buy the character carrying that much ink, so a shade means what it
    /// says and the range is spent where the picture actually varies.
    #[test]
    fn the_ramp_runs_from_faint_to_solid_without_going_back() {
        let weight = |byte: u8| {
            ALPHABET
                .templates
                .iter()
                .find(|template| template.byte == byte)
                .expect("every ramp character is a template")
                .weight
        };

        let mut previous = 0.0;
        for step in 0..RAMP_STEPS {
            let ink = weight(ALPHABET.ramp[step]);
            assert!(ink >= previous, "the ramp goes back on itself at step {step}");
            previous = ink;
        }
        assert_eq!(ALPHABET.graded(0.0), b'.');
        assert_eq!(ALPHABET.graded(1.0), b'@');
    }

    /// A patch that is lit everywhere wants a glyph that covers everywhere.
    #[test]
    fn a_fully_lit_cell_is_a_heavy_glyph() {
        let glyph = ALPHABET.nearest(&[1.0; CELL_PIXELS], true);
        assert!(
            b"@#%$&8BMW".contains(&glyph),
            "a solid patch came back as `{}`",
            glyph as char
        );
    }

    /// The point of matching bitmaps rather than reading a ramp: a cell lit only
    /// along one diagonal should come back as a glyph drawn along that diagonal,
    /// which no ordering by weight could ever produce.
    #[test]
    fn a_diagonal_patch_picks_a_diagonal_glyph() {
        let mut rising = [0f32; CELL_PIXELS];
        let mut falling = [0f32; CELL_PIXELS];
        for y in 0..CELL_PIXELS_TALL {
            // Bottom-left to top-right, and its mirror.
            let across = y as f32 / (CELL_PIXELS_TALL - 1) as f32;
            let x = ((1.0 - across) * (CELL_PIXELS_WIDE - 1) as f32).round() as usize;
            rising[y * CELL_PIXELS_WIDE + x] = 1.0;
            falling[y * CELL_PIXELS_WIDE + (CELL_PIXELS_WIDE - 1 - x)] = 1.0;
        }

        assert_eq!(ALPHABET.nearest(&rising, false), b'/');
        assert_eq!(ALPHABET.nearest(&falling, false), b'\\');
    }

    /// Where in the cell the light is has to matter, not just how much of it
    /// there is. Both of these are one row lit out of eleven.
    #[test]
    fn where_the_light_sits_in_a_cell_decides_the_glyph() {
        let row = |index: usize| {
            let mut cell = [0f32; CELL_PIXELS];
            for x in 0..CELL_PIXELS_WIDE {
                cell[index * CELL_PIXELS_WIDE + x] = 1.0;
            }
            ALPHABET.nearest(&cell, false)
        };
        let low = row(CELL_PIXELS_TALL - 3);
        let high = row(1);
        assert_ne!(
            low, high,
            "a bar at the bottom and a bar at the top both came back as `{}`",
            low as char
        );
        assert_eq!(low, b'_');
    }

    /// Every candidate is a character the exporter can actually draw, and none
    /// of them is a space or anything that would read as a word.
    #[test]
    fn the_alphabet_is_marks_and_only_marks() {
        assert_eq!(ALPHABET.templates.len(), MARKS.len());
        assert!(ALPHABET.templates.iter().all(|template| {
            template.byte.is_ascii_graphic() && !template.byte.is_ascii_lowercase()
        }));
        assert!(RAMP.iter().all(|byte| MARKS.contains(byte)));
    }

    /// Stopping the search early is only worth doing if it stops at the same
    /// answer, so this measures it against the sweep it replaced.
    #[test]
    fn the_shortened_search_finds_what_the_full_one_would() {
        let sweep = |cell: &Cell, mean: f32| {
            let shape = direction_of(cell, mean).expect("these cells all vary");
            ALPHABET
                .templates
                .iter()
                .map(|template| {
                    let alignment: f32 = shape
                        .iter()
                        .zip(&template.shape)
                        .map(|(sample, ink)| sample * ink)
                        .sum();
                    let level = template.weight - mean;
                    (1.0 - alignment + LEVEL_WEIGHT * level * level, template.byte)
                })
                .fold((f32::INFINITY, SPACE), |best, next| {
                    if next.0 < best.0 {
                        next
                    } else {
                        best
                    }
                })
                .1
        };

        // A sloped edge at every brightness, which is the case the matcher
        // exists for and the one where the two halves of the score compete.
        // From one step above blank: an edge at zero brightness is not an edge,
        // and has no direction to match against.
        for step in 1..16 {
            let level = step as f32 / 15.0;
            let mut cell = [0f32; CELL_PIXELS];
            for y in 0..CELL_PIXELS_TALL {
                let across = y as f32 / (CELL_PIXELS_TALL - 1) as f32;
                let x = (across * (CELL_PIXELS_WIDE - 1) as f32).round() as usize;
                cell[y * CELL_PIXELS_WIDE + x] = level;
            }
            let mean = mean_of(&cell);
            assert_eq!(
                ALPHABET.matched(&cell, mean),
                sweep(&cell, mean),
                "the search stopped short at brightness {level}"
            );
        }
    }
}
