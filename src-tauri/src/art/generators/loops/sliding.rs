//! Squares that slide inside one another while the whole grid doubles.
//!
//! After the *2D fractal sliding squares* in [Bleuje's animations][ref], written
//! here from the idea rather than from that sketch.
//!
//! The loop is a zoom of exactly two. Over a period every square grows to twice
//! its size, which is the size the squares one step coarser were, and the figure
//! at the end of the period is the figure at the start of it with every scale
//! having taken over the job of the scale above. Nothing is where it was and the
//! picture has not changed.
//!
//! What makes that exact is the grid the squares are cut from. Halve the plane
//! into squares, halve those, and go on: the whole family of squares, all scales
//! at once, is unchanged by doubling about the origin — a square of a given
//! scale lands exactly on a square of the scale above. Nothing else does. In
//! particular a tree grown from one root does not: double it and the root's four
//! children are each the size the root was, so a period on there are four
//! figures where there was one, and the seam is a jump rather than a seam.
//!
//! So there is no root here. A square is picked out by which cell of the grid it
//! is and which scale it belongs to, and everything about it is a function of
//!
//! ```text
//! over = phase - scale
//! ```
//!
//! and of the cell's number, both of which the doubling carries across
//! unchanged: the cell a scale down inherits the number along with the job. Its
//! size, its tone, where it sets off from, how far it has slid, whether it is
//! drawn at all. Anything that reads the scale on its own — a tone per scale, a
//! slide that starts at the top — has nothing to be carried across, and the seam
//! shows.
//!
//! Only two scales are ever on the frame. A square grows into the size the scale
//! above it is leaving and shrinks away again as it outgrows it, so a pair of
//! neighbouring scales is always crossing and — see [`grown`] — the two of them
//! cover the frame between them however far through the crossing they are. That
//! is what a zoom looks like from inside, and it is also why nothing has to be
//! said about where the grid starts or stops: the scales that would give it away
//! are not drawn.
//!
//! Four things had to be true before it read as a picture rather than as graph
//! paper, and all four are about the grid it is built on.
//!
//! A square is an *area*, not an outline. Nested outlines are ruled lines by
//! construction, and no amount of moving them changes that. Filled and held a
//! little in from its own edges, a square reads as a tile with a gutter round
//! it, and tiles that have slid apart read as tiles that have slid apart.
//!
//! A move takes three periods rather than one. One period puts every scale at
//! the same point of the same move at the same moment, which is a lattice
//! shearing in place; two is no better, because a scale is then half a move
//! behind and half a move is the middle of an edge, which is on the grid too. A
//! third of a move is nowhere in particular.
//!
//! A square is carried by the [`INHERITED`] cells above it and no further, so it
//! is not moving about a fixed grid but about a cell that is itself moving about
//! a cell. That is a truncation the doubling survives: the cells that carry it
//! move up a scale exactly as it does, so a period later it is carried by the
//! same cells.
//!
//! And no two cells set off at the same moment — see [`slip`], without which the
//! grid's own repeat comes through and the frame is one motif tiled.
//!
//! [ref]: https://github.com/Bleuje/processing-animations-code

use crate::art::generators::paper::{hue, Paper};
use crate::art::motion::{ease, scatter};

/// How sharply a square leaves its corner and arrives at the next. Gentle,
/// because a hard ease parks them on the corners, and the corners are exactly
/// where the figure is a grid.
const HARDNESS: f64 = 1.5;

/// Periods a square takes to reach the next corner. Three, so a scale runs a
/// third of a move behind the one above it — neither on the corners nor on the
/// middles of the edges, which are the two ways of being on the grid.
const UNHURRIED: f64 = 3.0;

/// How many cells above it carry a square along.
///
/// This is what makes the figure nest rather than shimmer: with nothing above it
/// a square only wanders about its own cell of a fixed grid, and a fixed grid is
/// what the piece is trying not to be. Each one of these is another scale of
/// movement underneath the movement, and four times the work.
const INHERITED: u32 = 3;

/// How much of a cell is left as a margin, against the cell. Against the cell
/// rather than the frame because that is what makes it survive the zoom: a
/// gutter a quarter of its own tile is a quarter of it when the tile doubles.
const GUTTER: f64 = 0.26;

/// How far apart in the colour wheel two scales are.
const SHADE: f64 = 0.16;

type Point = (f64, f64);

/// How fine the squares are.
///
/// Two scales are drawn whatever this is, so it does not set how much is on the
/// frame so much as how small it is: each one of these halves the tiles and puts
/// four times as many of them on the frame.
pub fn depth(given: usize) -> u32 {
    given.clamp(3, 6) as u32
}

pub fn draw(paper: &mut Paper, depth: u32, phase: f64, colored: bool) {
    for tile in tiles(paper.across(), depth, phase) {
        let tint = if colored { hue(tile.over * SHADE) } else { [1.0; 3] };
        let half = tile.size * (1.0 - GUTTER) * tile.showing / 2.0;
        paper.fill(
            &[
                (tile.centre.0 - half, tile.centre.1 - half),
                (tile.centre.0 + half, tile.centre.1 - half),
                (tile.centre.0 + half, tile.centre.1 + half),
                (tile.centre.0 - half, tile.centre.1 + half),
            ],
            tint,
            1.0,
        );
    }
}

/// A square of the grid, on its way down to the ones that get drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Tile {
    centre: Point,
    /// The whole cell it stands for, gutter and all.
    size: f64,
    /// Which cell of the grid it is, at its own scale. The only thing that
    /// tells it from the cell beside it, and the doubling hands it on.
    cell: (i64, i64),
    /// How long it has been on the frame, which is the only thing it knows
    /// about the zoom and the only thing the doubling carries across.
    over: f64,
    /// How much of its cell it has grown into.
    showing: f64,
}

/// Every square on the frame at one moment.
///
/// The two scales that are crossing, each taken from the cells that carry it,
/// each of those walked down to the squares themselves. Nothing else is worked
/// out: a scale that is not being drawn is not being drawn.
fn tiles(across: f64, depth: u32, phase: f64) -> Vec<Tile> {
    let mut all = Vec::new();
    for scale in [depth as i64 - 1, depth as i64] {
        let over = phase - scale as f64;
        let showing = grown(over, depth);
        if showing <= 0.0 {
            continue;
        }

        let carried = carrying(over);
        // Whole cells of the carrying scale, enough of them to cover the frame.
        // A square never leaves the cell that carries it, so a cell that misses
        // the frame has nothing on it and none is needed beyond the edges.
        let reach = |extent: f64| {
            let last = (extent / carried).floor() as i64;
            -last - 1..=last
        };

        for down in reach(0.5) {
            for along in reach(across / 2.0) {
                let centre = ((along as f64 + 0.5) * carried, (down as f64 + 0.5) * carried);
                let cell = Tile {
                    centre,
                    size: carried,
                    cell: (along, down),
                    over: over + INHERITED as f64,
                    showing,
                };
                carry(&mut all, across, cell, INHERITED);
            }
        }
    }
    all
}

/// One cell, and the squares it carries, `left` scales further down.
fn carry(all: &mut Vec<Tile>, across: f64, cell: Tile, left: u32) {
    if offstage(across, cell.centre, cell.size) {
        return;
    }
    if left == 0 {
        all.push(cell);
        return;
    }
    let started = cell.over + slip(cell.cell);
    for (quarter, centre) in quarters(cell.centre, cell.size, started).into_iter().enumerate() {
        let under = Tile {
            centre,
            size: cell.size / 2.0,
            cell: halves(cell.cell, quarter),
            over: cell.over - 1.0,
            ..cell
        };
        carry(all, across, under, left - 1);
    }
}

/// How wide the cell that carries a square is, given how long the square has
/// been on the frame. [`INHERITED`] doublings larger than the square itself.
fn carrying(over: f64) -> f64 {
    2.0_f64.powf(over + INHERITED as f64)
}

/// How much of its cell a square has grown into: up from nothing as it takes
/// over the size the scale above it is leaving, and back down as it outgrows it.
///
/// It grows rather than fades because the reader has nothing to make of a fade.
/// It matches a cell by how much light is in it, so a half-lit square and a
/// half-lit square laid over it are one grey, and the frame in the middle of a
/// crossing came out as texture with no figure in it. Two squares, one small and
/// one large and both solid, are two squares.
///
/// It is a quarter circle because that is the shape that keeps the frame's
/// covering constant. A scale's squares cover the square of this, four of them
/// stand where one of the scale above did, and the two scales that are crossing
/// are a quarter turn apart — so the covering goes as `sin² + cos²`, which is
/// one, and nothing on the frame swells or thins as the zoom goes by.
fn grown(over: f64, depth: u32) -> f64 {
    let shown = over + depth as f64;
    // Both ends by hand: a sine of half a turn is a hair off zero rather than
    // zero, and a hair is a square the recursion still walks down to and the
    // seam still has to account for.
    if shown <= 0.0 || shown >= 2.0 {
        return 0.0;
    }
    (std::f64::consts::FRAC_PI_2 * shown).sin()
}

/// Whether a cell, and so everything it carries, has left the frame.
fn offstage(across: f64, centre: Point, size: f64) -> bool {
    centre.0.abs() - size / 2.0 > across / 2.0 || centre.1.abs() - size / 2.0 > 0.5
}

/// The four corners of a cell, in the order they are travelled.
const CORNERS: [Point; 4] = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];

/// Which cell of the next scale down a quarter of this one is.
///
/// Halving the plane again numbers the new squares two for one, so the quarter
/// that starts in a corner takes the number of the cell in that corner, and
/// keeps it as it travels: the number is what tells a square from its
/// neighbours, and one that changed under a square while it was on the frame
/// would change what the square was doing halfway through doing it.
fn halves(cell: (i64, i64), quarter: usize) -> (i64, i64) {
    let corner = CORNERS[quarter % 4];
    (
        cell.0 * 2 + (corner.0 > 0.0) as i64,
        cell.1 * 2 + (corner.1 > 0.0) as i64,
    )
}

/// How long after the cell beside it a cell sets off, in the units its clock is
/// kept in.
///
/// Without this every cell of the grid does the same thing at the same moment,
/// and the frame comes out as one motif tiled — the failure the sliding was
/// there to avoid, arrived at from the other side. A cell is told apart by which
/// cell it is and nothing else, because that is the one thing about it the
/// doubling hands down: a scale later the same number belongs to the cell that
/// has taken over the job, and it sets off just as late.
///
/// It has to be a *part* of a move. A whole one, or a corner's worth of lead,
/// leaves the four squares in the same four places at the same moment — the four
/// of them are the four corners whichever of them set off first — so the frame
/// would tile just the same and nothing would look any different.
fn slip(cell: (i64, i64)) -> f64 {
    let quarters = (scatter(cell.0 as u64, cell.1 as u64) * 4.0).floor();
    quarters * UNHURRIED / 4.0
}

/// Where a cell's four squares are, part way through the turn that carries each
/// of them into the corner the next one is leaving.
///
/// `over` is the cell's own clock and `size` its whole width. A square arrives
/// at a corner exactly as the next departure begins, so the four of them
/// together are the same four squares at the end of every move — which is what
/// lets a scale hand the figure to the scale below it.
fn quarters(centre: Point, size: f64, over: f64) -> [Point; 4] {
    let moves = over / UNHURRIED;
    let turn = moves.div_euclid(1.0).rem_euclid(4.0) as usize;
    let progress = ease(moves.rem_euclid(1.0), HARDNESS);

    [0, 1, 2, 3].map(|quarter| {
        let at = quarter + turn;
        let from = CORNERS[at % 4];
        let to = CORNERS[(at + 1) % 4];
        (
            centre.0 + (from.0 + (to.0 - from.0) * progress) * size / 4.0,
            centre.1 + (from.1 + (to.1 - from.1) * progress) * size / 4.0,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// A frame's worth of squares, rounded so two of them can be compared
    /// without asking the last bit of a float to agree.
    fn placed(phase: f64) -> Vec<[i64; 4]> {
        let mut all: Vec<[i64; 4]> = tiles(1.0, 4, phase)
            .iter()
            .map(|tile| {
                [tile.centre.0, tile.centre.1, tile.size, tile.showing]
                    .map(|held| (held * 1e6).round() as i64)
            })
            .collect();
        all.sort();
        all
    }

    /// The whole piece in one assertion, and the one an earlier draft of it
    /// failed: a period on, the squares are the squares it started with. They
    /// are not the same squares — every one of them has taken over from the one
    /// a scale above — and the picture cannot tell.
    #[test]
    fn a_period_on_the_squares_are_the_squares_it_started_with() {
        assert_eq!(placed(1.0), placed(0.0));
        assert!(placed(0.0).len() > 40, "only {} squares", placed(0.0).len());
    }

    /// And the way round: what is on the frame in between is not what it
    /// started with, so the loop is a loop rather than a still.
    #[test]
    fn the_squares_are_elsewhere_inside_the_period() {
        for step in 1..8 {
            let phase = step as f64 / 8.0;
            assert_ne!(placed(phase), placed(0.0), "nothing has moved by {phase}");
        }
    }

    /// Two scales are on the frame at most, they cover it between them whatever
    /// point of the crossing they are at, and neither of them is a scale that is
    /// not drawn — which is what lets the grid be cut off at both ends without
    /// anything going missing.
    #[test]
    fn the_two_scales_on_the_frame_cover_it_between_them() {
        let depth = 4;
        for step in 0..=8 {
            let phase = step as f64 / 8.0;
            let band: Vec<f64> = (0..=depth + 2)
                .map(|scale| grown(phase - scale as f64, depth))
                .collect();
            // A scale's squares cover the square of how far they have grown, and
            // there are four of them to one of the scale above.
            let covered: f64 = band.iter().map(|grown| grown * grown).sum();
            assert!((covered - 1.0).abs() < 1e-12, "the frame is {covered} at {phase}");
            assert_eq!(band[0], 0.0, "a scale that is not drawn is on: {band:?}");
            let on = band.iter().filter(|&&grown| grown > 0.0).count();
            assert!(on <= 2, "{on} scales are on: {band:?}");
        }
    }

    /// Where a cell's four squares are, as a set — the order they come back in
    /// says which square is which, and two cells with the same four squares in
    /// a different order draw the same cell.
    fn arranged(over: f64) -> Vec<[i64; 2]> {
        let mut all: Vec<[i64; 2]> = quarters((0.0, 0.0), 0.8, over)
            .iter()
            .map(|(x, y)| [(x * 1e6).round() as i64, (y * 1e6).round() as i64])
            .collect();
        all.sort();
        all
    }

    /// At rest the four of them are the four quarters of the cell, whichever
    /// turn has just finished — so the figure a scale hands over is the figure
    /// the next one starts from.
    #[test]
    fn at_rest_the_squares_are_the_four_quarters() {
        let corners = vec![
            [-200000, -200000],
            [-200000, 200000],
            [200000, -200000],
            [200000, 200000],
        ];
        for turn in -4..8 {
            assert_eq!(arranged(turn as f64 * UNHURRIED), corners, "at rest on {turn}");
        }
    }

    /// No two scales are ever at rest together, which is the difference between
    /// a picture and a sheet of squared paper. A move that took one period, or
    /// two, would fail this at every whole phase.
    #[test]
    fn no_two_scales_are_ever_square_with_each_other() {
        let travelled = |over: f64| ease((over / UNHURRIED).rem_euclid(1.0), HARDNESS);
        for step in 0..12 {
            let phase = step as f64 / 12.0;
            // On the grid means arrived, half way along an edge, or about to go.
            let gridlike = (0..6)
                .map(|scale| travelled(phase - scale as f64))
                .filter(|along| [0.0, 0.5, 1.0].iter().any(|at| (along - at).abs() < 0.05))
                .count();
            assert!(gridlike <= 2, "{gridlike} scales are on the grid at {phase}");
        }
    }

    /// And no two cells beside each other are doing the same thing, which is the
    /// other way of ending up with wallpaper: the grid repeats by construction,
    /// so something on it has to not.
    ///
    /// The draft that failed this gave each cell a corner to set off from
    /// instead of a moment, and passed a test that compared the four squares in
    /// the order they were worked out. Four squares starting a corner apart are
    /// the same four squares in a different order, and the frame tiled.
    #[test]
    fn cells_beside_each_other_do_not_set_off_together() {
        let mut taken = HashSet::new();
        for along in -8..8 {
            for down in -8..8 {
                taken.insert((slip((along, down)) * 1e6).round() as i64);
            }
        }
        assert_eq!(taken.len(), 4, "the moments taken are {taken:?}");

        let moments: HashSet<Vec<[i64; 2]>> = taken
            .iter()
            .map(|&late| arranged(0.7 + late as f64 / 1e6))
            .collect();
        assert_eq!(moments.len(), 4, "two cells draw the same four squares");
        // Which is what a whole move late would have come to.
        assert_eq!(arranged(0.7), arranged(0.7 + UNHURRIED));
    }

    /// Halving the plane numbers the quarters, and it gives the four of them
    /// four different numbers — a cell whose squares shared a number would have
    /// them all doing the same thing again, a scale further down.
    #[test]
    fn the_quarters_are_four_cells_of_the_scale_below() {
        let quartered: HashSet<(i64, i64)> = (0..4).map(|at| halves((3, -2), at)).collect();
        assert_eq!(quartered, HashSet::from([(6, -4), (7, -4), (6, -3), (7, -3)]));
        assert_eq!(halves((0, 0), 2), (0, 0), "the far corner is the low cell");
        assert_eq!(halves((0, 0), 0), (1, 1), "the near corner is the high one");
    }

    /// A square stays inside the cell it belongs to at every point of every
    /// move, which is what lets a cell that has left the frame be dropped along
    /// with everything it carries.
    #[test]
    fn a_square_never_leaves_the_cell_it_belongs_to() {
        let size = 0.8;
        for step in -40..40 {
            let over = step as f64 / 6.0;
            for (x, y) in quarters((0.0, 0.0), size, over) {
                // A square is half the cell, so its middle may reach a quarter
                // of the way out and no further.
                assert!(x.abs() <= size / 4.0 + 1e-9, "{x} out at {over}");
                assert!(y.abs() <= size / 4.0 + 1e-9, "{y} out at {over}");
            }
        }
    }

    /// And it has gone somewhere in between.
    #[test]
    fn the_squares_are_elsewhere_in_the_middle_of_a_turn() {
        let still = quarters((0.0, 0.0), 0.8, 0.0);
        let moving = quarters((0.0, 0.0), 0.8, UNHURRIED / 2.0);
        let moved = still
            .iter()
            .zip(&moving)
            .filter(|(one, other)| (one.0 - other.0).abs() + (one.1 - other.1).abs() > 0.05)
            .count();
        assert_eq!(moved, 4, "only {moved} squares moved");
    }

    /// A cell carries what it carries however far down it is asked for.
    #[test]
    fn the_carrying_cell_doubles_with_the_scale_it_carries() {
        for step in 0..6 {
            let over = -(step as f64) - 0.3;
            assert!((carrying(over) / carrying(over - 1.0) - 2.0).abs() < 1e-12);
        }
    }

    // ---- TEMPORARY probes, to be deleted ----

    #[test]
    fn probe_png() {
        for (name, phase) in [("a0", 0.0), ("b25", 0.25), ("c50", 0.5), ("d75", 0.75), ("e99", 0.99)] {
            let mut paper = Paper::new(700, 700).expect("sheet");
            draw(&mut paper, 4, phase, false);
            paper.picture().expect("picture").save(format!("/tmp/slide_{name}.png")).expect("saved");
        }
        for depth in [3u32, 5, 6] {
            let mut paper = Paper::new(700, 700).expect("sheet");
            draw(&mut paper, depth, 0.35, false);
            paper.picture().expect("picture").save(format!("/tmp/slide_d{depth}.png")).expect("saved");
        }
    }

    #[test]
    fn probe_slip_distribution() {
        let mut counts = [0usize; 4];
        for a in -16..16i64 {
            for d in -16..16i64 {
                let q = (scatter(a as u64, d as u64) * 4.0).floor() as usize;
                counts[q.min(3)] += 1;
            }
        }
        println!("slip counts over 32x32 = {counts:?}");
        println!("slip(0,0) = {}, scatter(0,0) = {}", slip((0, 0)), scatter(0, 0));

        // How often do the four quarters of a cell get four different slips?
        let mut all_four = 0;
        let mut all_same = 0;
        for a in -16..16i64 {
            for d in -16..16i64 {
                let s: HashSet<i64> = (0..4)
                    .map(|q| (slip(halves((a, d), q)) * 1e6).round() as i64)
                    .collect();
                if s.len() == 4 { all_four += 1; }
                if s.len() == 1 { all_same += 1; }
            }
        }
        println!("of 1024 cells: {all_four} have four different child slips, {all_same} have one");
    }

    /// Is the leaf figure of one top cell the same as another's?
    #[test]
    fn probe_spatial_repeat() {
        // Build one top cell's subtree by hand, and compare the offsets.
        let motif = |a: i64, d: i64, over: f64| -> Vec<[i64; 2]> {
            let mut all = Vec::new();
            let cell = Tile {
                centre: (0.0, 0.0),
                size: 1.0,
                cell: (a, d),
                over: over + INHERITED as f64,
                showing: 1.0,
            };
            carry(&mut all, 1e9, cell, INHERITED);
            let mut out: Vec<[i64; 2]> = all
                .iter()
                .map(|t| [(t.centre.0 * 1e9).round() as i64, (t.centre.1 * 1e9).round() as i64])
                .collect();
            out.sort();
            out
        };

        for over in [-3.0, -2.6, -3.4] {
            let mut seen: HashMap<Vec<[i64; 2]>, Vec<(i64, i64)>> = HashMap::new();
            for a in -8..8i64 {
                for d in -8..8i64 {
                    seen.entry(motif(a, d, over)).or_default().push((a, d));
                }
            }
            let biggest = seen.values().map(|v| v.len()).max().unwrap();
            println!(
                "over {over}: {} distinct motifs over 256 top cells, biggest class {biggest}",
                seen.len()
            );
            // Neighbour repeat: how many cells draw the same motif as the cell to their right?
            let mut same_right = 0;
            for a in -8..7i64 {
                for d in -8..8i64 {
                    if motif(a, d, over) == motif(a + 1, d, over) { same_right += 1; }
                }
            }
            println!("  {same_right} of 240 cells match the cell to their right");
        }
    }

    /// The actual frame: does the set of drawn squares repeat under a shift of
    /// one carrying cell?
    #[test]
    fn probe_frame_repeat() {
        for &phase in &[0.0, 0.25, 0.5, 0.73] {
            for scale in [3i64, 4] {
                let over = phase - scale as f64;
                if grown(over, 4) <= 0.0 { continue; }
                let carried = carrying(over);
                let leaf = 2.0_f64.powf(over);
                // A wide swathe, so that the shift has plenty to compare.
                let all = tiles(9.0, 4, phase);
                let mine: HashSet<[i64; 3]> = all
                    .iter()
                    .filter(|t| (t.size - leaf).abs() < leaf * 1e-9)
                    .map(|t| {
                        [
                            (t.centre.0 * 1e7).round() as i64,
                            (t.centre.1 * 1e7).round() as i64,
                            (t.size * 1e7).round() as i64,
                        ]
                    })
                    .collect();
                let shift = (carried * 1e7).round() as i64;
                let inner: Vec<&[i64; 3]> = mine
                    .iter()
                    .filter(|t| t[0] < (2.0 * 1e7) as i64 && t[0] > (-2.0 * 1e7) as i64)
                    .collect();
                let matched = inner
                    .iter()
                    .filter(|t| mine.contains(&[t[0] + shift, t[1], t[2]]))
                    .count();
                println!(
                    "phase {phase} scale {scale}: {matched} of {} squares repeat one carrier ({carried}) to the right",
                    inner.len()
                );
            }
        }
    }

    #[test]
    fn probe_seam_step() {
        let step = 1.0 / 240.0;
        let apart = |one: f64, other: f64| {
            let a: HashSet<[i64; 4]> = placed(one).into_iter().collect();
            let b: HashSet<[i64; 4]> = placed(other).into_iter().collect();
            a.symmetric_difference(&b).count()
        };
        let seam = apart(1.0 - step, 1.0);
        let inside: Vec<usize> = (0..12).map(|k| apart(k as f64 / 12.0, k as f64 / 12.0 + step)).collect();
        println!("seam step {seam}, inside steps {inside:?}");
    }

    #[test]
    fn the_depth_stays_worth_drawing() {
        assert_eq!(depth(0), 3);
        assert_eq!(depth(4), 4);
        assert_eq!(depth(99), 6);
    }
}
