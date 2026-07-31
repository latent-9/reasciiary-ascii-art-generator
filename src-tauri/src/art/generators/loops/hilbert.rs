//! A Hilbert curve pivoting about the middles of its own blocks.
//!
//! After the *hilbert curve transforms* piece in [Bleuje's animations][ref],
//! written here from the idea rather than from that sketch.
//!
//! The curve is built the same way it is defined — the square halved, and
//! halved again, each block holding a smaller copy of the figure — so the
//! obvious thing to animate is the construction itself. Every block is a square
//! and a square can be turned a quarter and still be the same square, so a
//! block can pivot about its own middle and leave the figure's outline exactly
//! where it was while rearranging everything inside it. Four quarters and the
//! block is back, which is the loop.
//!
//! Blocks do not pivot together. Each takes its turn on its own offset into the
//! period, so at any moment most of the figure is square and a few of its parts
//! are going round — which is the difference between a drawing that transforms
//! and a drawing that spins.
//!
//! [ref]: https://github.com/Bleuje/processing-animations-code

use std::f64::consts::TAU;

use crate::art::generators::paper::{hue, Paper};
use crate::art::motion::{ease, scatter};

/// How deep the square may be halved. Four is a 16 by 16 grid and 256 steps,
/// which is about what a character grid can hold: the curve doubles back on
/// itself every step, so a run of it needs a few cells to read as a run rather
/// than as texture, and six is already past what any grid this app exports will
/// resolve.
const ORDERS: std::ops::RangeInclusive<usize> = 2..=6;

/// How many levels of blocks pivot, counting in from the whole square. The
/// square itself is not one of them: turning that is turning the picture. Two
/// in is where a block is large enough to be watched going round and small
/// enough that several are going at once.
const LEVELS: u32 = 2;

/// The square's side. It is fitted to the widest the motion ever gets rather
/// than to the figure standing still — a quadrant halfway through its quarter
/// reaches a fifth again past the corner it started from, and a frame fitted to
/// the still figure would cut it off there.
const SIDE: f64 = 0.76;

/// How much longer than the grid's own spacing a step can be and still count as
/// the curve rather than as a connection stretched across a pivot.
const SNAP: f64 = 1.7;

/// How sharply a block turns over. High spends most of the period square and
/// all of the movement in a quick quarter.
const HARDNESS: f64 = 4.0;

/// The line, against the spacing of the grid it is drawn on: a quarter of the
/// gap between neighbouring steps. Held to the figure rather than to the frame
/// so that asking for one order finer thins the line to match, instead of
/// filling the gaps in with ink.
const WEIGHT: f64 = 0.26;

/// What the piece is asked for.
pub fn order(given: usize) -> u32 {
    given.clamp(*ORDERS.start(), *ORDERS.end()) as u32
}

/// One frame.
pub fn draw(paper: &mut Paper, order: u32, phase: f64, seed: u64, colored: bool) {
    let cells = 1_u64 << order;
    let steps = cells * cells;
    let spacing = SIDE / cells as f64;
    let weight = spacing * WEIGHT;

    let points: Vec<(f64, f64)> = (0..steps)
        .map(|step| place(along(order, step), cells, seed, phase))
        .collect();

    // Round the palette once over the period, so a colour is where it started
    // when the figure is.
    let tint = |step: usize| {
        if colored {
            hue(step as f64 / steps as f64 + phase)
        } else {
            [1.0; 3]
        }
    };

    let mut run = vec![points[0]];
    let mut began = 0;
    for (step, pair) in points.windows(2).enumerate() {
        let stretch = gap(pair[0], pair[1]) / spacing;
        if stretch <= SNAP {
            run.push(pair[1]);
            continue;
        }
        paper.stroke(&run, weight, tint(began), 1.0);
        // Two blocks that no longer sit as they were cut, and the curve still
        // running between them. Drawn, because it is still one curve, but
        // faintly and more faintly the further it has been pulled — so a pivot
        // reads as the figure coming apart and closing again rather than as a
        // line thrown across the frame.
        paper.stroke(pair, weight, tint(step), (SNAP / stretch).powi(2) * 0.5);
        run.clear();
        run.push(pair[1]);
        began = step + 1;
    }
    paper.stroke(&run, weight, tint(began), 1.0);
}

/// Which cell of a `2^order` grid the curve's `step`th point lands on.
///
/// The usual construction, read from the bottom up: two bits of the step say
/// which quadrant, and the frame those bits are read in is reflected on the way
/// down — which is the whole of what makes the curve join up where quadrants
/// meet rather than jump between them.
fn along(order: u32, step: u64) -> (u64, u64) {
    let cells = 1_u64 << order;
    let (mut x, mut y) = (0, 0);
    let mut rest = step;
    let mut side = 1;
    while side < cells {
        let across = 1 & (rest / 2);
        let down = 1 & (rest ^ across);
        if down == 0 {
            if across == 1 {
                x = side - 1 - x;
                y = side - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        x += side * across;
        y += side * down;
        rest /= 4;
        side *= 2;
    }
    (x, y)
}

/// Where a cell lands once every block holding it has turned about its middle.
fn place(cell: (u64, u64), cells: u64, seed: u64, phase: f64) -> (f64, f64) {
    let world = |along: f64| SIDE * (along / cells as f64 - 0.5);
    let mut point = (world(cell.0 as f64 + 0.5), world(cell.1 as f64 + 0.5));

    // Deepest first. An outer block carries whatever its inner ones have
    // already done round with it, which is what makes the figure fold instead
    // of shear — and it is safe to take every middle from the cell the point
    // started in, because a quarter turn leaves a block sitting on itself.
    for level in (1..=LEVELS).rev() {
        let side = cells >> level;
        let (across, down) = (cell.0 / side, cell.1 / side);
        let middle = (
            world((across * side) as f64 + side as f64 / 2.0),
            world((down * side) as f64 + side as f64 / 2.0),
        );
        let block = down * (1 << level) + across;
        point = turn(point, middle, pivot(level, block, seed, phase));
    }
    point
}

/// How far round a block is, at this point in the period.
///
/// Four quarters over the period, so it arrives back as the block it was. The
/// easing is inside the quarter rather than across the whole turn: without it
/// the block would rotate steadily and never be anything but rotating.
fn pivot(level: u32, block: u64, seed: u64, phase: f64) -> f64 {
    let offset = scatter(seed, u64::from(level) * 4096 + block);
    let quarters = (phase + offset).rem_euclid(1.0) * 4.0;
    let done = quarters.floor();
    (done + ease(quarters - done, HARDNESS)) * TAU / 4.0
}

fn turn(point: (f64, f64), about: (f64, f64), angle: f64) -> (f64, f64) {
    let (sin, cos) = angle.sin_cos();
    let (x, y) = (point.0 - about.0, point.1 - about.1);
    (about.0 + x * cos - y * sin, about.1 + x * sin + y * cos)
}

fn gap(one: (f64, f64), other: (f64, f64)) -> f64 {
    ((one.0 - other.0).powi(2) + (one.1 - other.1).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// What a Hilbert curve is: every cell once, and never a jump.
    #[test]
    fn the_curve_walks_every_cell_one_step_at_a_time() {
        for order in 1..=5 {
            let cells = 1_u64 << order;
            let steps = cells * cells;
            let walk: Vec<(u64, u64)> = (0..steps).map(|step| along(order, step)).collect();

            assert_eq!(walk.iter().collect::<HashSet<_>>().len(), steps as usize);
            for pair in walk.windows(2) {
                let stride = pair[0].0.abs_diff(pair[1].0) + pair[0].1.abs_diff(pair[1].1);
                assert_eq!(stride, 1, "order {order} jumps");
            }
        }
    }

    /// However deep it is asked to go, the answer is a depth it can draw.
    #[test]
    fn the_order_is_held_to_what_a_grid_can_show() {
        assert_eq!(order(4), 4);
        assert_eq!(order(0), *ORDERS.start() as u32);
        assert_eq!(order(40), *ORDERS.end() as u32);
    }

    /// The loop, at the level everything else here rests on: a period of pivots
    /// leaves every point exactly where it was found.
    #[test]
    fn a_period_puts_every_point_back() {
        let cells = 1_u64 << 4;
        for step in (0..cells * cells).step_by(7) {
            let cell = along(4, step);
            let start = place(cell, cells, 7, 0.0);
            let round = place(cell, cells, 7, 1.0);
            assert!((start.0 - round.0).abs() < 1e-9, "{start:?} != {round:?}");
            assert!((start.1 - round.1).abs() < 1e-9, "{start:?} != {round:?}");
        }
    }

    /// And it has been somewhere in between, or the pivots are not turning.
    #[test]
    fn the_figure_is_not_where_it_was_in_the_middle_of_the_period() {
        let cells = 1_u64 << 4;
        let moved = (0..cells * cells)
            .step_by(3)
            .filter(|&step| {
                let cell = along(4, step);
                gap(place(cell, cells, 7, 0.0), place(cell, cells, 7, 0.37)) > SIDE / 40.0
            })
            .count();
        assert!(moved > 40, "only {moved} points moved");
    }

    /// Nothing may leave the sheet, at any point in the period — the whole
    /// reason the square is fitted smaller than the frame.
    #[test]
    fn the_figure_stays_inside_the_frame() {
        let cells = 1_u64 << 5;
        for tick in 0..24 {
            let phase = tick as f64 / 24.0;
            for step in (0..cells * cells).step_by(7) {
                let (x, y) = place(along(5, step), cells, 7, phase);
                assert!(y.abs() < 0.5, "a point sat {y} down the frame");
                assert!(x.abs() < 0.5, "a point sat {x} across a square frame");
            }
        }
    }
}
