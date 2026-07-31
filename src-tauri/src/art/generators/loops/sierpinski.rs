//! Three fractals that trade places and are the same fractal again.
//!
//! After the *Sierpinski triangle loop* in [Bleuje's animations][ref], written
//! here from the idea rather than from that sketch.
//!
//! A gasket is three half-size copies of itself, one at each corner of its
//! triangle. Slide every copy along an edge into the next corner and the union
//! is the gasket again — the movement is one of the figure's own symmetries,
//! taken slowly instead of jumped. Three such moves put every copy back where
//! it started, so a loop is three moves long and the seam is exact rather than
//! nearly.
//!
//! What makes it worth watching is the middle of a move. A copy travelling from
//! one corner to the next passes over the midpoint of the edge between them, so
//! half way through the three copies straddle the middles of the edges and the
//! figure is briefly not a gasket at all. Then it resolves, and it has not
//! turned into anything else.
//!
//! The levels do not move together: each runs a little further round its own
//! clock than its parent and each copy a little further than its siblings, so
//! the shuffle runs down the tree rather than the whole figure sliding at every
//! scale at once. An offset does not change how long a clock takes, so every
//! level still arrives.
//!
//! A copy dims at the middle of its travel and comes back, which is what makes
//! a swap read as a dissolve rather than a collision. It is not a ramp that only
//! falls — the figure would have to gain a level over the loop for that to close,
//! and gaining a level is the sliding squares' business, not this piece's.
//!
//! [ref]: https://github.com/Bleuje/processing-animations-code

use std::f64::consts::{PI, TAU};

use crate::art::generators::paper::{hue, Paper};
use crate::art::motion::{ease, scatter};

/// The circle the outer corners sit on, against the frame. The figure never
/// leaves its own outer triangle — a copy slides along an edge and every point
/// of it stays inside — so this can be fitted to the still figure.
const SIDE: f64 = 0.96;

/// How sharply a copy leaves and arrives. High enough that most of a move is
/// spent arrived, which is what makes three moves read as three moves and not
/// as a drift.
const HARDNESS: f64 = 2.6;

/// The least a level runs ahead of its parent, in moves.
const SLIP: f64 = 0.17;

/// And how much further the seed is allowed to send it, which is what makes one
/// seed's shuffle a different shuffle: the three thirds of a loop are not one
/// third drawn three times, and two seeds are not one figure drawn twice.
const TWIST: f64 = 0.71;

/// The outer triangle's line, against the frame.
const WEIGHT: f64 = 0.016;

/// What each level down takes off it, so depth reads as depth.
const FALL: f64 = 0.72;

/// How far a copy fades at the middle of its travel.
const DIP: f64 = 0.5;

/// Moves in a loop. Three, because three of them are the identity.
const MOVES: f64 = 3.0;

type Point = (f64, f64);
type Triangle = [Point; 3];

/// How deep the recursion is allowed to go.
///
/// Past six the leaves are smaller than a cell and the picture is a grey
/// triangle; below one there is nothing to move.
pub fn depth(given: usize) -> u32 {
    given.clamp(1, 6) as u32
}

pub fn draw(paper: &mut Paper, depth: u32, phase: f64, seed: u64, colored: bool) {
    // An equilateral triangle stands one and a half radii tall and √3 wide, and
    // sits a quarter of a radius below its own corner circle's middle.
    let radius = (SIDE / 1.5).min(SIDE * paper.across() / 3.0_f64.sqrt());
    let outer = [0, 1, 2].map(|corner| {
        let (sin, cos) = (TAU * (corner as f64 / 3.0 - 0.25)).sin_cos();
        (cos * radius, sin * radius + radius / 4.0)
    });

    let fractal = Fractal { depth, phase, seed, colored };
    fractal.spray(paper, outer, 0, 0.0, 1.0);
}

/// What every level of the recursion has in common, so the level itself carries
/// only what makes it that level.
struct Fractal {
    depth: u32,
    phase: f64,
    seed: u64,
    colored: bool,
}

impl Fractal {
    /// One triangle, and the three that are inside it.
    ///
    /// `offset` is how far round its own clock this branch runs, and it is
    /// handed down rather than worked out, so a branch keeps the same clock for
    /// the whole loop.
    fn spray(&self, paper: &mut Paper, triangle: Triangle, level: u32, offset: f64, alpha: f64) {
        let tint = if self.colored {
            hue(offset / MOVES + self.phase)
        } else {
            [1.0; 3]
        };
        let outline = [triangle[0], triangle[1], triangle[2], triangle[0]];
        paper.stroke(&outline, WEIGHT * FALL.powi(level as i32), tint, alpha);

        if level >= self.depth {
            return;
        }

        // This branch's own clock: three moves over the period, however far
        // round it starts.
        let clock = (MOVES * self.phase + offset).rem_euclid(MOVES);
        let turn = clock.floor() as usize;
        let progress = clock.fract();
        // Quietest where it is furthest from anywhere it belongs.
        let faded = alpha * (1.0 - DIP * (PI * progress).sin());

        for copy in 0..3 {
            let moved = placed(triangle, copy, turn, ease(progress, HARDNESS));
            let ahead = offset + self.spread(level, copy);
            self.spray(paper, moved, level + 1, ahead, faded);
        }
    }

    /// How far ahead of its parent one branch runs. Only the level and which
    /// copy it is go into it, so a branch is not told apart by its whole path
    /// down the tree — the offsets add up on the way down, and two branches
    /// that agree at every step are the same branch anyway.
    fn spread(&self, level: u32, copy: usize) -> f64 {
        SLIP + TWIST * scatter(self.seed, level as u64 * 3 + copy as u64)
    }
}

/// The half-size triangle that sits in one corner of a triangle.
fn corner(triangle: Triangle, at: usize) -> Triangle {
    let held = triangle[at];
    [
        held,
        between(held, triangle[(at + 1) % 3], 0.5),
        between(held, triangle[(at + 2) % 3], 0.5),
    ]
}

/// Where one of the three copies is, part way through the move that carries it
/// to the next corner.
///
/// The move is a translation and is written as one: a corner triangle shifted
/// by half the edge it travels along is exactly the corner triangle at the far
/// end of that edge. Sliding rather than interpolating corner to corner is what
/// keeps a copy's own corners in the order they started in, so the branch below
/// it does not swap around underneath it half way through.
fn placed(triangle: Triangle, copy: usize, turn: usize, progress: f64) -> Triangle {
    let from = (copy + turn) % 3;
    let to = (from + 1) % 3;
    let step = (
        (triangle[to].0 - triangle[from].0) / 2.0 * progress,
        (triangle[to].1 - triangle[from].1) / 2.0 * progress,
    );
    corner(triangle, from).map(|(x, y)| (x + step.0, y + step.1))
}

fn between(one: Point, other: Point, along: f64) -> Point {
    (
        one.0 + (other.0 - one.0) * along,
        one.1 + (other.1 - one.1) * along,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTER: Triangle = [(0.0, -0.5), (0.45, 0.3), (-0.45, 0.3)];

    fn gap(one: Point, other: Point) -> f64 {
        ((one.0 - other.0).powi(2) + (one.1 - other.1).powi(2)).sqrt()
    }

    /// Whether a point is inside a triangle, by the sign of the turn it makes
    /// with each edge. Every sign the same means every edge left it on the same
    /// side, which is what inside means.
    fn holds(triangle: Triangle, point: Point) -> bool {
        (0..3)
            .map(|edge| {
                let one = triangle[edge];
                let other = triangle[(edge + 1) % 3];
                (other.0 - one.0) * (point.1 - one.1) - (other.1 - one.1) * (point.0 - one.0)
            })
            .all(|turn| turn >= -1e-9)
    }

    /// A packed set of triangles, so two placements can be compared without
    /// caring which corner either of them starts at. Rounded, because arriving
    /// at a corner by two half steps is the same place as being handed it and
    /// is not the same last bit.
    fn sorted(triangles: [Triangle; 3]) -> Vec<[i64; 6]> {
        let mut all: Vec<[i64; 6]> = triangles
            .iter()
            .map(|triangle| {
                let mut points: Vec<[i64; 2]> = triangle
                    .iter()
                    .map(|(x, y)| [(x * 1e6).round() as i64, (y * 1e6).round() as i64])
                    .collect();
                points.sort();
                [
                    points[0][0], points[0][1], points[1][0], points[1][1], points[2][0],
                    points[2][1],
                ]
            })
            .collect();
        all.sort();
        all
    }

    fn resting(turn: usize, progress: f64) -> [Triangle; 3] {
        [0, 1, 2].map(|copy| placed(OUTER, copy, turn, progress))
    }

    /// The whole claim of the piece: a move ends where a move begins, so the
    /// figure at rest is the same figure whichever move has just finished.
    #[test]
    fn every_move_arrives_at_the_figure_it_left() {
        let still = sorted(resting(0, 0.0));
        for turn in 0..3 {
            assert_eq!(sorted(resting(turn, 0.0)), still, "at rest after {turn}");
            assert_eq!(sorted(resting(turn, 1.0)), still, "arriving on {turn}");
        }
    }

    /// And at rest it is the gasket — the three copies are the three corners,
    /// not some other three places that happen to repeat.
    #[test]
    fn at_rest_the_copies_are_the_corners() {
        let corners = sorted([corner(OUTER, 0), corner(OUTER, 1), corner(OUTER, 2)]);
        assert_eq!(sorted(resting(0, 0.0)), corners);
    }

    /// Half way, a copy is over the middle of the edge it is travelling along —
    /// the moment the figure is not a gasket, which is the moment worth having.
    #[test]
    fn a_copy_passes_over_the_middle_of_its_edge() {
        for turn in 0..3 {
            for copy in 0..3 {
                let from = (copy + turn) % 3;
                let middle = between(OUTER[from], OUTER[(from + 1) % 3], 0.5);
                let moved = placed(OUTER, copy, turn, 0.5);
                let straddle = gap(moved[0], middle) + gap(middle, moved[1]);
                let across = gap(moved[0], moved[1]);
                assert!((straddle - across).abs() < 1e-9, "{straddle} against {across}");
            }
        }
    }

    /// Nothing ever leaves the triangle it belongs to, at any point of any
    /// move, which is why the outer triangle can be fitted to the frame.
    #[test]
    fn a_copy_never_leaves_its_own_triangle() {
        for turn in 0..3 {
            for step in 0..=20 {
                for copy in 0..3 {
                    let moved = placed(OUTER, copy, turn, step as f64 / 20.0);
                    for point in moved {
                        assert!(holds(OUTER, point), "{point:?} is outside on move {turn}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_depth_stays_worth_drawing() {
        assert_eq!(depth(0), 1);
        assert_eq!(depth(4), 4);
        assert_eq!(depth(99), 6);
    }
}
