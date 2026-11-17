//! Scanline fills (tatami, satin fill, gradient density).
//!
//! The shape is rotated so stitch rows are horizontal, cut into rows, and the row
//! spans are grouped into *sections* (boustrophedon cells: runs of rows with one
//! span each, joined end to end). Sections are sewn back and forth without jumps.
//! Moving between sections uses running stitches routed over a travel graph made
//! of the outline and the rows themselves; routes that run *under* areas still to
//! be sewn are cheap, routes on top of finished stitching are expensive, so
//! connecting stitches end up hidden — the same trick human digitizers use.

use super::out::Out;
use crate::geom::{Affine, BBox, Point, Polygon, poly, polyline};
use crate::model::{Texture, TexturePattern};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::f64::consts::{PI, TAU};

/// Cost multiplier for travelling along an outline stretch whose neighbouring
/// rows are already sewn (the travel would sit visibly on top).
const COVERED_RING_COST: f64 = 6.0;
/// Cost multiplier for travelling along a row that is already sewn (nearly
/// invisible because it follows the existing stitch line).
const COVERED_SPAN_COST: f64 = 1.6;
/// Penalty for a straight hop when the graph offers no route.
const NO_ROUTE_COST: f64 = 25.0;
/// Cost multiplier for travelling along a row with trapunto on: travel keeps to the outline,
/// crossing the fill only where nothing else reaches.
const TRAPUNTO_SPAN_COST: f64 = 50.0;

#[derive(Clone, Debug)]
pub(crate) struct ScanConfig {
    /// Stitch direction, degrees counter-clockwise on screen.
    pub angle: f64,
    /// Distance between rows (mm).
    pub row_spacing: f64,
    /// Row spacing at the far side for gradients; 0 = constant.
    pub row_spacing_end: f64,
    /// Needle step inside rows; `f64::INFINITY` = satin (one stitch per row).
    pub stitch_length: f64,
    /// Stagger of the needle grid per row, as a fraction of `stitch_length`.
    pub offset: f64,
    /// Satin rows longer than this are split; 0 = never.
    pub split_length: f64,
    /// Row end extension (mm).
    pub pull_comp: f64,
    /// Stitch length for travel runs.
    pub travel_length: f64,
    /// Needle-point jitter (0–1, fraction of a quarter stitch).
    pub random: f64,
    /// Programmed splits (tatami): the needle points follow the lines of a pattern tiled over
    /// the fill.
    pub texture: Option<Texture>,
    /// Split lines (tatami; design space, mm): each row has a needle point where it crosses one.
    pub splits: Vec<Vec<Point>>,
    /// Trapunto: travel keeps to the outline instead of running along the rows.
    pub trapunto: bool,
    /// A textured edge: the most a row's end stops short of the outline (mm, at random).
    pub ragged: f64,
    /// Row spacing along a profile across the shape (see `FillParams::profile`), already in
    /// row spacing (halved for satin); empty = `row_spacing`.
    pub profile: Vec<[f64; 2]>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        ScanConfig {
            angle: 0.0,
            row_spacing: 0.4,
            row_spacing_end: 0.0,
            stitch_length: 4.0,
            offset: 0.25,
            split_length: 0.0,
            pull_comp: 0.0,
            travel_length: 2.5,
            random: 0.0,
            texture: None,
            splits: Vec::new(),
            trapunto: false,
            ragged: 0.0,
            profile: Vec::new(),
        }
    }
}

/// Most rows one fill sews along a spacing profile.
const MAX_PROFILE_ROWS: usize = 200_000;

/// The most stitches one fill piece sews (a 1 m square of plain tatami is about 770,000).
/// Document coordinates reach kilometres, so a mistyped or hostile size would otherwise ask
/// for billions of stitches and exhaust memory; sewing stops here instead (issue #74).
pub const MAX_FILL_STITCHES: usize = 1_000_000;

/// A spacing profile ready to read: its points that are numbers as `(t, rows per mm)`, in
/// order across the shape.
struct Profile(Vec<(f64, f64)>);

impl Profile {
    /// `None` when the profile has no points that are numbers.
    fn new(points: &[[f64; 2]]) -> Option<Profile> {
        let density = |s: f64| if s > 1e-9 { 1.0 / s.max(0.05) } else { 0.0 };
        let mut pts: Vec<(f64, f64)> =
            points.iter().filter(|[t, s]| t.is_finite() && s.is_finite()).map(|&[t, s]| (t.clamp(0.0, 1.0), density(s))).collect();
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        (!pts.is_empty()).then_some(Profile(pts))
    }

    /// The rows per mm at `t` (0 to 1 across the shape): the density (one over the spacing, 0
    /// for no rows) changes evenly between the points.
    fn density(&self, t: f64) -> f64 {
        let pts = &self.0;
        let i = pts.partition_point(|p| p.0 <= t);
        match (i.checked_sub(1).and_then(|k| pts.get(k)), pts.get(i)) {
            (Some(a), Some(b)) if b.0 - a.0 > 1e-12 => a.1 + (b.1 - a.1) * (t - a.0) / (b.0 - a.0),
            (Some(a), _) => a.1,
            (None, Some(b)) => b.1,
            (None, None) => 0.0,
        }
    }
}

/// Most segments a programmed split pattern is tiled into over one fill: a larger fill gets
/// larger tiles.
const MAX_PATTERN_SEGMENTS: f64 = 400_000.0;
/// Most split-line segments drawn on one fill that are used.
const MAX_SPLIT_SEGMENTS: usize = 100_000;
/// Most bands of rows the split lines are sorted into.
const MAX_BANDS: f64 = 4096.0;

/// Split lines in row space (rows run along x), sorted into bands of rows so a row only looks
/// at the lines near it.
pub(crate) struct Splits {
    /// Segments, each from its lower to its higher y.
    segs: Vec<(Point, Point)>,
    /// The segments reaching into each band.
    bands: Vec<Vec<usize>>,
    y0: f64,
    band: f64,
    /// Programmed splits: the needle points are where the rows cross the lines, with the tatami
    /// grid only filling gaps too long for one stitch. Otherwise the grid stays and the
    /// crossings join it.
    pattern: bool,
}

impl Splits {
    /// The split lines of `cfg` over a fill whose rings in row space are `rings` (`fwd` turns
    /// design space into row space); `None` without any (or for satin).
    fn new(cfg: &ScanConfig, rings: &[Vec<Point>], fwd: &Affine) -> Option<Splits> {
        if !cfg.stitch_length.is_finite() || (cfg.texture.is_none() && cfg.splits.is_empty()) {
            return None;
        }
        let bb = rings.iter().map(|r| polyline::bbox(r)).reduce(BBox::union)?;
        if bb.is_empty() || !(bb.width().is_finite() && bb.height().is_finite()) {
            return None;
        }
        let mut segs: Vec<(Point, Point)> = Vec::new();
        for line in &cfg.splits {
            for w in line.windows(2) {
                if let [a, b] = w {
                    push_segment(&mut segs, fwd.apply(*a), fwd.apply(*b));
                }
            }
        }
        segs.truncate(MAX_SPLIT_SEGMENTS);
        if let Some(t) = cfg.texture {
            pattern_segments(t, &bb, &mut segs);
        }
        Self::banded(segs, &bb, cfg.texture.is_some())
    }

    /// `segs` sorted into bands of rows over `bb`; `None` without any.
    fn banded(segs: Vec<(Point, Point)>, bb: &BBox, pattern: bool) -> Option<Splits> {
        if segs.is_empty() {
            return None;
        }
        let (y0, y1) = (bb.min.y, bb.max.y);
        let band = ((y1 - y0) / MAX_BANDS).max(2.0);
        let n = ((y1 - y0) / band).floor() as usize + 1;
        let mut bands: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, (a, b)) in segs.iter().enumerate() {
            if b.y < y0 || a.y > y1 {
                continue;
            }
            let first = ((a.y - y0) / band).floor().max(0.0) as usize;
            let last = (((b.y - y0) / band).floor().max(0.0) as usize).min(n - 1);
            for list in bands.iter_mut().take(last + 1).skip(first) {
                list.push(i);
            }
        }
        Some(Splits { segs, bands, y0, band, pattern })
    }

    /// The lines of texture `t` tiled over `bb` (row space: rows along x): an embossed satin's
    /// pattern laid along its column (NEW-28). `None` when nothing crosses the rows.
    pub(crate) fn of_texture(t: Texture, bb: &BBox) -> Option<Splits> {
        if bb.is_empty() || !(bb.width().is_finite() && bb.height().is_finite()) {
            return None;
        }
        let mut segs = Vec::new();
        pattern_segments(t, bb, &mut segs);
        Self::banded(segs, bb, true)
    }

    /// Where the row at `y` crosses the lines, in increasing x.
    pub(crate) fn crossings(&self, y: f64, out: &mut Vec<f64>) {
        out.clear();
        let k = ((y - self.y0) / self.band).floor();
        if k.is_nan() || k < 0.0 {
            return;
        }
        let Some(list) = self.bands.get(k as usize) else { return };
        for &i in list {
            let Some(&(a, b)) = self.segs.get(i) else { continue };
            // Half-open, so a row through a vertex counts it once.
            if a.y <= y && y < b.y {
                let x = a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x);
                if x.is_finite() {
                    out.push(x);
                }
            }
        }
        out.sort_by(f64::total_cmp);
    }
}

/// Adds the segment from `a` to `b`, lower end first, unless it runs along the rows (it would
/// never cross them).
fn push_segment(segs: &mut Vec<(Point, Point)>, a: Point, b: Point) {
    if a.is_finite() && b.is_finite() && (a.y - b.y).abs() > 1e-9 {
        segs.push(if a.y <= b.y { (a, b) } else { (b, a) });
    }
}

/// The lines of one tile of `pattern`, in tile widths with y down, and the tile's height. Every
/// line crosses the rows: one along them would leave no needle points.
fn tile(pattern: TexturePattern) -> (f64, Vec<Vec<Point>>) {
    let p = Point::new;
    let curve = |n: usize, f: &dyn Fn(f64) -> Point| (0..=n).map(|k| f(k as f64 / n as f64)).collect::<Vec<_>>();
    let ring = |cx: f64, cy: f64, r: f64| curve(24, &|t: f64| p(cx + r * (TAU * t).cos(), cy + r * (TAU * t).sin()));
    match pattern {
        TexturePattern::Diamonds => (1.0, vec![vec![p(0.0, 0.0), p(1.0, 1.0)], vec![p(1.0, 0.0), p(0.0, 1.0)]]),
        TexturePattern::Waves => (1.0, vec![curve(16, &|t| p(0.5 + 0.2 * (TAU * t).sin(), t))]),
        TexturePattern::Herringbone => (1.0, vec![vec![p(0.0, 0.0), p(0.5, 0.5), p(0.0, 1.0)], vec![p(0.5, 0.0), p(1.0, 0.5), p(0.5, 1.0)]]),
        // Courses half a brick high; the joints of one course fall mid-brick in the next.
        TexturePattern::Bricks => (1.0, vec![vec![p(0.0, 0.0), p(0.0, 0.5)], vec![p(0.5, 0.5), p(0.5, 1.0)]]),
        TexturePattern::Circles => (1.0, vec![curve(20, &|t| p(0.5 + 0.36 * (TAU * t).cos(), 0.5 + 0.36 * (TAU * t).sin()))]),
        // Rows of half circles hanging from the row above, each row half a scale along.
        TexturePattern::Scales => {
            let arc = |cx: f64, cy: f64| curve(12, &|t: f64| p(cx + 0.5 * (PI * t).cos(), cy + 0.5 * (PI * t).sin()));
            (1.0, vec![arc(0.5, 0.0), arc(0.0, 0.5)])
        }
        // A honeycomb of hexagons standing on a point, every other row half a cell along: the
        // edges of two cells, each edge once.
        TexturePattern::Hexagons => {
            let s = 1.0 / 3.0_f64.sqrt();
            (
                3.0 * s,
                vec![
                    vec![p(0.0, 0.5 * s), p(0.5, 0.0), p(1.0, 0.5 * s), p(1.0, 1.5 * s), p(0.5, 2.0 * s), p(0.0, 1.5 * s)],
                    vec![p(0.5, 2.0 * s), p(0.5, 3.0 * s)],
                ],
            )
        }
        // Rows of Vs, one inside the next.
        TexturePattern::Chevrons => (0.5, vec![vec![p(0.0, 0.0), p(0.5, 0.5), p(1.0, 0.0)]]),
        TexturePattern::Zigzag => (1.0, vec![vec![p(0.5, 0.0), p(0.8, 0.25), p(0.5, 0.5), p(0.8, 0.75), p(0.5, 1.0)]]),
        // Two waves in opposite phase meeting, the onion shapes of an ogee lattice.
        TexturePattern::Ogee => (1.0, vec![curve(16, &|t| p(0.25 + 0.25 * (TAU * t).sin(), t)), curve(16, &|t| p(0.75 - 0.25 * (TAU * t).sin(), t))]),
        // Blocks of three short lines, every other block a row down: a woven look.
        TexturePattern::BasketWeave => (
            1.0,
            [0.125, 0.25, 0.375]
                .iter()
                .map(|&x| vec![p(x, 0.0), p(x, 0.5)])
                .chain([0.625, 0.75, 0.875].iter().map(|&x| vec![p(x, 0.5), p(x, 1.0)]))
                .collect(),
        ),
        // A five-pointed star, one point up.
        TexturePattern::Stars => (
            1.0,
            vec![
                (0..=10)
                    .map(|k| {
                        let a = -PI / 2.0 + f64::from(k) * PI / 5.0;
                        let r = if k % 2 == 0 { 0.42 } else { 0.17 };
                        p(0.5 + r * a.cos(), 0.52 + r * a.sin())
                    })
                    .collect(),
            ],
        ),
        // A heart standing on its point.
        TexturePattern::Hearts => (
            1.0,
            vec![curve(32, &|t| {
                let a = TAU * t;
                let (x, y) = (16.0 * a.sin().powi(3), 13.0 * a.cos() - 5.0 * (2.0 * a).cos() - 2.0 * (3.0 * a).cos() - (4.0 * a).cos());
                p(0.5 + x * 0.022, 0.46 - y * 0.022)
            })],
        ),
        // A leaf lying corner to corner, with its midrib.
        TexturePattern::Leaves => {
            let edge =
                |side: f64| curve(12, &move |t| p(0.15 + 0.7 * t + side * 0.16 * (PI * t).sin(), 0.85 - 0.7 * t + side * 0.16 * (PI * t).sin()));
            (1.0, vec![edge(1.0), edge(-1.0), vec![p(0.15, 0.85), p(0.85, 0.15)]])
        }
        // Lines across the rows: straight, slanting, in close pairs, and a slant through them.
        TexturePattern::Stripes => (1.0, vec![vec![p(0.5, 0.0), p(0.5, 1.0)]]),
        TexturePattern::Slants => (1.0, vec![vec![p(0.0, 0.0), p(1.0, 1.0)]]),
        TexturePattern::Pinstripes => (1.0, vec![vec![p(0.42, 0.0), p(0.42, 1.0)], vec![p(0.58, 0.0), p(0.58, 1.0)]]),
        TexturePattern::Lattice => (1.0, vec![vec![p(0.0, 0.0), p(0.0, 1.0)], vec![p(0.0, 0.0), p(1.0, 1.0)]]),
        // Rows of round arches, each standing on the tops of the row below.
        TexturePattern::Arches => (0.5, vec![curve(16, &|t| p(0.5 - 0.5 * (PI * t).cos(), 0.5 - 0.5 * (PI * t).sin()))]),
        TexturePattern::Rings => (1.0, vec![ring(0.5, 0.5, 0.4), ring(0.5, 0.5, 0.2)]),
        TexturePattern::Dots => (1.0, vec![ring(0.5, 0.5, 0.14)]),
        TexturePattern::Spirals => (
            1.0,
            vec![curve(48, &|t| {
                let (a, r) = (2.0 * TAU * t, 0.04 + 0.38 * t);
                p(0.5 + r * a.cos(), 0.5 + r * a.sin())
            })],
        ),
        // Peaks one inside the next.
        TexturePattern::Pyramids => (1.0, vec![vec![p(0.0, 1.0), p(0.5, 0.0), p(1.0, 1.0)], vec![p(0.25, 1.0), p(0.5, 0.5), p(0.75, 1.0)]]),
        // Two waves side by side running across the rows.
        TexturePattern::Ripples => {
            (1.0, vec![curve(16, &|t| p(0.3 + 0.12 * (TAU * t).sin(), t)), curve(16, &|t| p(0.7 + 0.12 * (TAU * t).sin(), t))])
        }
        // A key winding in on itself (its runs along the rows leave no needle points, its
        // uprights show).
        TexturePattern::GreekKey => {
            (1.0, vec![vec![p(0.1, 1.0), p(0.1, 0.1), p(0.9, 0.1), p(0.9, 0.9), p(0.3, 0.9), p(0.3, 0.3), p(0.7, 0.3), p(0.7, 0.7), p(0.5, 0.7)]])
        }
        TexturePattern::Crosses => (1.0, vec![vec![p(0.3, 0.3), p(0.7, 0.7)], vec![p(0.7, 0.3), p(0.3, 0.7)]]),
        TexturePattern::Quatrefoils => (1.0, vec![ring(0.3, 0.5, 0.2), ring(0.7, 0.5, 0.2), ring(0.5, 0.3, 0.2), ring(0.5, 0.7, 0.2)]),
        // Five round petals.
        TexturePattern::Flowers => (
            1.0,
            vec![curve(60, &|t| {
                let a = TAU * t - PI / 2.0;
                let r = 0.38 * (0.6 + 0.4 * (5.0 * (a + PI / 2.0)).cos());
                p(0.5 + r * a.cos(), 0.5 + r * a.sin())
            })],
        ),
        TexturePattern::Trefoils => {
            (1.0, [-90.0f64, 30.0, 150.0].iter().map(|d| d.to_radians()).map(|a| ring(0.5 + 0.18 * a.cos(), 0.52 + 0.18 * a.sin(), 0.17)).collect())
        }
        // A crescent: the outer circle's left side, then back round a circle bitten out of it.
        TexturePattern::Moons => {
            let mut moon = curve(24, &|t| {
                let a = PI / 3.0 + t * 4.0 * PI / 3.0;
                p(0.5 + 0.38 * a.cos(), 0.5 + 0.38 * a.sin())
            });
            let (cx, r) = (0.152, (0.038f64 * 0.038 + 0.329 * 0.329).sqrt());
            let end = (-0.329f64).atan2(0.038);
            moon.extend(curve(24, &|t| {
                let a = end + t * (-2.0 * end - TAU);
                p(0.5 + cx + r * a.cos(), 0.5 + r * a.sin())
            }));
            (1.0, vec![moon])
        }
        TexturePattern::Arrows => (1.0, vec![vec![p(0.5, 0.9), p(0.5, 0.15)], vec![p(0.3, 0.35), p(0.5, 0.15), p(0.7, 0.35)]]),
        TexturePattern::NestedDiamonds => (
            1.0,
            vec![
                vec![p(0.5, 0.0), p(1.0, 0.5), p(0.5, 1.0), p(0.0, 0.5), p(0.5, 0.0)],
                vec![p(0.5, 0.25), p(0.75, 0.5), p(0.5, 0.75), p(0.25, 0.5), p(0.5, 0.25)],
            ],
        ),
        // Diamonds with lines running across them corner to corner.
        TexturePattern::Argyle => (
            1.0,
            vec![
                vec![p(0.5, 0.0), p(1.0, 0.5), p(0.5, 1.0), p(0.0, 0.5), p(0.5, 0.0)],
                vec![p(0.0, 0.0), p(1.0, 1.0)],
                vec![p(1.0, 0.0), p(0.0, 1.0)],
            ],
        ),
        // Short slanting strands, the slant turning from block to block.
        TexturePattern::Wicker => (
            1.0,
            [0.05, 0.15, 0.25]
                .iter()
                .map(|&x| vec![p(x, 0.0), p(x + 0.2, 0.5)])
                .chain([0.55, 0.65, 0.75].iter().map(|&x| vec![p(x + 0.2, 0.5), p(x, 1.0)]))
                .collect(),
        ),
        // A quill with barbs either side.
        TexturePattern::Feathers => (
            1.0,
            std::iter::once(vec![p(0.5, 1.0), p(0.5, 0.0)])
                .chain([0.35, 0.6, 0.85].iter().flat_map(|&y| [vec![p(0.5, y), p(0.2, y - 0.2)], vec![p(0.5, y), p(0.8, y - 0.2)]]))
                .collect(),
        ),
        // A teardrop leaning over at its tip, with an eye.
        TexturePattern::Paisley => (
            1.0,
            vec![
                curve(40, &|t| {
                    let a = TAU * t;
                    p(0.5 + 0.28 * a.sin() * (a / 2.0).sin() + 0.12 * (a.cos() + 1.0) * 0.5, 0.5 + 0.4 * a.cos())
                }),
                ring(0.5, 0.36, 0.08),
            ],
        ),
        // Two curls joined in an S.
        TexturePattern::Swirls => {
            let left: Vec<Point> = curve(36, &|t| {
                let a = 3.0 * PI * t;
                let r = 0.03 + 0.17 * t;
                p(-0.2 + r * (a + PI).cos(), r * (a + PI).sin())
            });
            let mut s: Vec<Point> = left.iter().map(|q| p(0.5 + q.x, 0.5 + q.y)).collect();
            s.extend(left.iter().rev().skip(1).map(|q| p(0.5 - q.x, 0.5 - q.y)));
            (1.0, vec![s])
        }
        TexturePattern::Snowflakes => (
            1.0,
            [90.0f64, 30.0, 150.0]
                .iter()
                .map(|d| d.to_radians())
                .map(|a| vec![p(0.5 - 0.4 * a.cos(), 0.5 - 0.4 * a.sin()), p(0.5 + 0.4 * a.cos(), 0.5 + 0.4 * a.sin())])
                .collect(),
        ),
        TexturePattern::Bubbles => (1.0, vec![ring(0.3, 0.3, 0.18), ring(0.72, 0.45, 0.12), ring(0.4, 0.76, 0.1)]),
        // Rays out from the middle, none along the rows.
        TexturePattern::Sunbursts => (
            1.0,
            (0..12)
                .map(|k| (15.0 + 30.0 * f64::from(k)).to_radians())
                .map(|a| vec![p(0.5 + 0.1 * a.cos(), 0.5 + 0.1 * a.sin()), p(0.5 + 0.42 * a.cos(), 0.5 + 0.42 * a.sin())])
                .collect(),
        ),
    }
}

/// The lines of texture `t` tiled over `bb` (row space), turned by its angle.
fn pattern_segments(t: Texture, bb: &BBox, segs: &mut Vec<(Point, Point)>) {
    let (h, lines) = tile(t.pattern);
    let per_tile: usize = lines.iter().map(|l| l.len().saturating_sub(1)).sum();
    let angle = if t.angle.is_finite() { t.angle.to_radians() } else { 0.0 };
    let (turn, back) = (Affine::rotate(angle), Affine::rotate(-angle));
    // The fill's box in pattern space.
    let pb = BBox::from_points(bb.corners().map(|c| back.apply(c)));
    let mut w = if t.size.is_finite() { t.size.clamp(1.0, 100.0) } else { 6.0 };
    let tiles = |w: f64| ((pb.width() / w).ceil() + 3.0) * ((pb.height() / (w * h)).ceil() + 3.0);
    let count = tiles(w) * per_tile as f64;
    if !count.is_finite() {
        return;
    }
    if count > MAX_PATTERN_SEGMENTS {
        w *= (count / MAX_PATTERN_SEGMENTS).sqrt();
    }
    // Tiles from a fixed origin, so the texture stays put while the shape changes.
    let (i0, i1) = ((pb.min.x / w).floor() as i64 - 1, (pb.max.x / w).ceil() as i64);
    let (j0, j1) = ((pb.min.y / (w * h)).floor() as i64 - 1, (pb.max.y / (w * h)).ceil() as i64);
    for j in j0..=j1 {
        for i in i0..=i1 {
            let o = Point::new(i as f64 * w, j as f64 * w * h);
            for line in &lines {
                for pair in line.windows(2) {
                    if let [a, b] = pair {
                        push_segment(segs, turn.apply(o + *a * w), turn.apply(o + *b * w));
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Crossing {
    x: f64,
    ring: usize,
    /// Arc-length position along the ring.
    pos: f64,
}

#[derive(Clone, Debug)]
struct Span {
    row: usize,
    y: f64,
    l: Crossing,
    r: Crossing,
    /// Graph node ids of the left / right crossings.
    ln: usize,
    rn: usize,
}

#[derive(Clone, Copy, Debug)]
enum EdgeKind {
    /// Outline stretch; covered when both owning spans are sewn.
    Ring(usize, usize),
    /// Along a row.
    Span(usize),
}

#[derive(Clone, Copy, Debug)]
struct Edge {
    to: usize,
    len: f64,
    kind: EdgeKind,
}

struct Graph {
    pts: Vec<Point>,
    adj: Vec<Vec<Edge>>,
}

impl Graph {
    fn add_node(&mut self, p: Point) -> usize {
        self.pts.push(p);
        self.adj.push(Vec::new());
        self.pts.len() - 1
    }

    fn add_edge(&mut self, a: usize, b: usize, kind: EdgeKind) {
        let len = self.pts[a].distance(self.pts[b]);
        self.adj[a].push(Edge { to: b, len, kind });
        self.adj[b].push(Edge { to: a, len, kind });
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Item(f64, usize);
impl Eq for Item {}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.total_cmp(&self.0).then_with(|| self.1.cmp(&o.1))
    }
}

fn dijkstra(g: &Graph, src: usize, sewn: &[bool], trapunto: bool) -> (Vec<f64>, Vec<usize>) {
    let n = g.pts.len();
    let mut dist = vec![f64::INFINITY; n];
    let mut prev = vec![usize::MAX; n];
    let mut heap = BinaryHeap::new();
    dist[src] = 0.0;
    heap.push(Item(0.0, src));
    let covered = |s: usize| s != usize::MAX && sewn[s];
    while let Some(Item(d, u)) = heap.pop() {
        if d > dist[u] {
            continue;
        }
        for e in &g.adj[u] {
            let w = match e.kind {
                EdgeKind::Ring(a, b) => {
                    if covered(a) && covered(b) && !trapunto {
                        COVERED_RING_COST
                    } else {
                        1.0
                    }
                }
                EdgeKind::Span(s) => {
                    if trapunto {
                        TRAPUNTO_SPAN_COST
                    } else if sewn[s] {
                        COVERED_SPAN_COST
                    } else {
                        1.0
                    }
                }
            };
            let nd = d + e.len * w;
            if nd < dist[e.to] {
                dist[e.to] = nd;
                prev[e.to] = u;
                heap.push(Item(nd, e.to));
            }
        }
    }
    (dist, prev)
}

fn path_to(prev: &[usize], src: usize, dst: usize) -> Option<Vec<usize>> {
    let mut path = vec![dst];
    let mut cur = dst;
    while cur != src {
        let p = prev[cur];
        if p == usize::MAX {
            return None;
        }
        path.push(p);
        cur = p;
    }
    path.reverse();
    Some(path)
}

/// How a section is entered: from its first or last row, at its left or right end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    section: usize,
    from_first: bool,
    left: bool,
}

struct Plan {
    spans: Vec<Span>,
    sections: Vec<Vec<usize>>,
    graph: Graph,
}

impl Plan {
    fn entry_node(&self, e: Entry) -> usize {
        let sec = &self.sections[e.section];
        let s = if e.from_first { sec[0] } else { sec[sec.len() - 1] };
        if e.left { self.spans[s].ln } else { self.spans[s].rn }
    }

    /// Node where the section ends when entered as `e`.
    fn exit_node(&self, e: Entry) -> usize {
        let sec = &self.sections[e.section];
        let last = if e.from_first { sec[sec.len() - 1] } else { sec[0] };
        // Rows alternate direction; with an odd number of rows we end on the opposite side.
        let ends_left = if sec.len() % 2 == 1 { !e.left } else { e.left };
        if ends_left { self.spans[last].ln } else { self.spans[last].rn }
    }

    fn entries(&self, section: usize) -> impl Iterator<Item = Entry> + '_ {
        let single = self.sections[section].len() == 1;
        [(true, true), (true, false), (false, true), (false, false)]
            .into_iter()
            .filter(move |(first, _)| *first || !single)
            .map(move |(from_first, left)| Entry { section, from_first, left })
    }
}

/// The heights of the fill's rows between `y0` and `y1`: along the density profile, evenly
/// spaced and centred, or with the spacing changing from start to end. `None` when a profile asks
/// for no rows anywhere.
fn row_heights(y0: f64, y1: f64, cfg: &ScanConfig) -> Option<Vec<f64>> {
    let s0 = cfg.row_spacing.max(0.05);
    let s1 = if cfg.row_spacing_end > 0.0 { cfg.row_spacing_end.max(0.05) } else { s0 };
    let height = y1 - y0;
    let mut ys = Vec::new();
    let profile = Profile::new(&cfg.profile);
    let profiled = profile.is_some();
    if let Some(profile) = &profile {
        // Along the profile: walk across the shape adding up the rows asked for, and sew one
        // each time they make a whole row (half a row in from the edge to start with).
        let dy = (height / 400_000.0).max(0.02);
        let mut acc = 0.5;
        let mut y = y0;
        while y < y1 && ys.len() < MAX_PROFILE_ROWS {
            acc += profile.density(((y - y0) / height.max(1e-9)).clamp(0.0, 1.0)) * dy;
            if acc >= 1.0 {
                ys.push(y);
                acc = (acc - 1.0).min(1.0);
            }
            y += dy;
        }
    } else if (s1 - s0).abs() < 1e-12 {
        // Centre the rows so both edges get the same margin. A shape too tall for that many
        // rows gets them spread wider instead.
        let mut s = s0;
        let mut n = (height / s0).floor() as usize + 1;
        if n > MAX_PROFILE_ROWS {
            n = MAX_PROFILE_ROWS;
            s = height / (n - 1) as f64;
        }
        let used = (n - 1) as f64 * s;
        let start = y0 + (height - used) * 0.5;
        for k in 0..n {
            ys.push(start + k as f64 * s);
        }
    } else {
        let spacing_at = |y: f64| s0 + (s1 - s0) * ((y - y0) / height.max(1e-9)).clamp(0.0, 1.0);
        let mut y = y0 + spacing_at(y0) * 0.5;
        while y < y1 && ys.len() < MAX_PROFILE_ROWS {
            ys.push(y);
            y += spacing_at(y);
        }
    }
    // A profile can ask for no rows anywhere; otherwise there is always one.
    if ys.is_empty() && !profiled {
        ys.push((y0 + y1) * 0.5);
    }
    (!ys.is_empty()).then_some(ys)
}

/// Builds rows, spans, sections and the travel graph in row space.
fn plan(rings: &[Vec<Point>], cfg: &ScanConfig) -> Option<Plan> {
    let bb = rings.first().map(|r| polyline::bbox(r))?;
    if bb.is_empty() {
        return None;
    }
    let mut ys = row_heights(bb.min.y, bb.max.y, cfg)?;
    // Nudge rows off exact vertex heights to avoid degenerate crossings.
    for y in &mut ys {
        if rings.iter().any(|r| r.iter().any(|p| (p.y - *y).abs() < 1e-9)) {
            *y += 1e-7;
        }
    }

    let cum: Vec<Vec<f64>> = rings
        .iter()
        .map(|r| {
            let mut c = Vec::with_capacity(r.len());
            let mut acc = 0.0;
            for i in 0..r.len() {
                c.push(acc);
                acc += r[i].distance(r[(i + 1) % r.len()]);
            }
            c
        })
        .collect();

    let mut graph = Graph { pts: Vec::new(), adj: Vec::new() };
    let mut spans: Vec<Span> = Vec::new();
    let mut row_spans: Vec<Vec<usize>> = Vec::new();
    for (row, &y) in ys.iter().enumerate() {
        let mut xs: Vec<Crossing> = Vec::new();
        for (ri, ring) in rings.iter().enumerate() {
            let n = ring.len();
            for i in 0..n {
                let a = ring[i];
                let b = ring[(i + 1) % n];
                if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                    let t = (y - a.y) / (b.y - a.y);
                    let x = a.x + t * (b.x - a.x);
                    xs.push(Crossing { x, ring: ri, pos: cum[ri][i] + t * a.distance(b) });
                }
            }
        }
        xs.sort_by(|a, b| a.x.total_cmp(&b.x));
        let mut ids = Vec::new();
        for pair in xs.as_chunks::<2>().0.iter() {
            let (l, r) = (pair[0], pair[1]);
            if r.x - l.x < 1e-6 {
                continue;
            }
            let ln = graph.add_node(Point::new(l.x, y));
            let rn = graph.add_node(Point::new(r.x, y));
            ids.push(spans.len());
            spans.push(Span { row, y, l, r, ln, rn });
        }
        row_spans.push(ids);
    }
    if spans.is_empty() {
        return None;
    }

    // Sections: a span continues the section of the single overlapping span in
    // the previous row, provided that one overlaps nothing else in this row.
    let overlap = |a: &Span, b: &Span| a.l.x < b.r.x && b.l.x < a.r.x;
    let mut section_of = vec![usize::MAX; spans.len()];
    let mut sections: Vec<Vec<usize>> = Vec::new();
    for k in 0..row_spans.len() {
        for &s in &row_spans[k] {
            let mut joined = false;
            if k > 0 {
                let prev: Vec<usize> = row_spans[k - 1].iter().copied().filter(|&p| overlap(&spans[p], &spans[s])).collect();
                if prev.len() == 1 {
                    let p = prev[0];
                    let fanout = row_spans[k].iter().filter(|&&c| overlap(&spans[p], &spans[c])).count();
                    let sec = section_of[p];
                    if fanout == 1 && sections[sec].last() == Some(&p) {
                        sections[sec].push(s);
                        section_of[s] = sec;
                        joined = true;
                    }
                }
            }
            if !joined {
                section_of[s] = sections.len();
                sections.push(vec![s]);
            }
        }
    }

    // Travel graph along the rings.
    for (ri, ring) in rings.iter().enumerate() {
        // (position, node, owning span)
        let mut items: Vec<(f64, usize, usize)> = Vec::new();
        for (i, &p) in ring.iter().enumerate() {
            let id = graph.add_node(p);
            items.push((cum[ri][i], id, usize::MAX));
        }
        for (si, s) in spans.iter().enumerate() {
            if s.l.ring == ri {
                items.push((s.l.pos, s.ln, si));
            }
            if s.r.ring == ri {
                items.push((s.r.pos, s.rn, si));
            }
        }
        items.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
        let n = items.len();
        if n < 2 {
            continue;
        }
        let has_owner = items.iter().any(|it| it.2 != usize::MAX);
        let mut prev_owner = vec![usize::MAX; n];
        let mut next_owner = vec![usize::MAX; n];
        if has_owner {
            let mut last = items.iter().rev().find(|it| it.2 != usize::MAX).map(|it| it.2).unwrap_or(usize::MAX);
            for i in 0..n {
                if items[i].2 != usize::MAX {
                    last = items[i].2;
                }
                prev_owner[i] = last;
            }
            let mut next = items.iter().find(|it| it.2 != usize::MAX).map(|it| it.2).unwrap_or(usize::MAX);
            for i in (0..n).rev() {
                if items[i].2 != usize::MAX {
                    next = items[i].2;
                }
                next_owner[i] = next;
            }
        }
        for i in 0..n {
            let j = (i + 1) % n;
            graph.add_edge(items[i].1, items[j].1, EdgeKind::Ring(prev_owner[i], next_owner[j]));
        }
    }
    for (si, s) in spans.iter().enumerate() {
        graph.add_edge(s.ln, s.rn, EdgeKind::Span(si));
    }
    Some(Plan { spans, sections, graph })
}

/// Appends running stitches along a graph path. The node chain is very dense
/// (every row crossing is a node), so it is simplified and re-sampled first.
fn travel_path(pts: &mut Vec<Point>, g: &Graph, path: &[usize], step: f64) {
    let mut line: Vec<Point> = Vec::with_capacity(path.len() + 1);
    if let Some(&cur) = pts.last() {
        line.push(cur);
    }
    line.extend(path.iter().map(|&n| g.pts[n]));
    let line = polyline::simplify(&polyline::dedup(&line, 1e-9), 0.05);
    if line.len() < 2 {
        return;
    }
    let res = polyline::resample(&line, step, 0.35);
    if pts.is_empty() {
        pts.extend(res);
    } else {
        pts.extend(res.into_iter().skip(1));
    }
}

/// Needle positions along one row (number `row`, at height `y`) from `xa` to `xb` (inclusive),
/// with needle points where it crosses `splits`.
fn row_points(xa: f64, xb: f64, (row, y): (usize, f64), cfg: &ScanConfig, splits: Option<&Splits>, out: &mut Vec<f64>) {
    out.clear();
    // A textured edge: each end stops short at random (the same for the same row each time),
    // always leaving the middle fifth of the row.
    let (xa, xb) = if cfg.ragged > 0.0 {
        let unit = |salt: u64| {
            let mut h = (row as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ salt.wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
            h ^= h >> 31;
            h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
            h ^= h >> 29;
            (h >> 11) as f64 / (1u64 << 53) as f64
        };
        let most = (xb - xa).abs() * 0.4;
        let (da, db) = ((unit(1) * cfg.ragged).min(most), (unit(2) * cfg.ragged).min(most));
        let dir = if xb >= xa { 1.0 } else { -1.0 };
        (xa + dir * da, xb - dir * db)
    } else {
        (xa, xb)
    };
    let (lo, hi) = (xa.min(xb), xa.max(xb));
    let len = hi - lo;
    let (step, offset) = if cfg.stitch_length.is_finite() {
        (cfg.stitch_length.max(0.3), cfg.offset)
    } else if let split = crate::model::effective_split_length(cfg.split_length)
        && split > 0.0
        && len > split
    {
        (split * 0.75, 0.37)
    } else {
        out.push(xa);
        out.push(xb);
        return;
    };
    let phase = (row as f64 * offset).rem_euclid(1.0) * step;
    let min_end = (step * 0.35).clamp(0.4, 1.5).min(len * 0.5);
    let m0 = ((lo + min_end - phase) / step).ceil() as i64;
    let m1 = ((hi - min_end - phase) / step).floor() as i64;
    // Optional jitter, deterministic per needle point (hash of row and index).
    let jitter = |m: i64| {
        if cfg.random <= 0.0 {
            return 0.0;
        }
        let mut h = (row as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (m as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
        h ^= h >> 29;
        h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        h ^= h >> 32;
        let u = (h >> 11) as f64 / (1u64 << 53) as f64;
        (u - 0.5) * 0.5 * step * cfg.random.clamp(0.0, 1.0)
    };
    let x_at = |m: i64| (phase + m as f64 * step + jitter(m)).clamp(lo + min_end * 0.5, hi - min_end * 0.5);
    out.push(xa);
    if let Some(sp) = splits.filter(|_| cfg.stitch_length.is_finite()) {
        let grid: Vec<f64> = if m1 >= m0 { (m0..=m1).map(x_at).collect() } else { Vec::new() };
        let mut inner = Vec::new();
        split_points(sp, y, (lo, hi), &grid, step, min_end, &mut inner);
        if xa <= xb {
            out.extend(inner);
        } else {
            out.extend(inner.into_iter().rev());
        }
    } else if m1 >= m0 {
        if xa <= xb {
            for m in m0..=m1 {
                out.push(x_at(m));
            }
        } else {
            for m in (m0..=m1).rev() {
                out.push(x_at(m));
            }
        }
    }
    out.push(xb);
}

/// Needle points strictly inside the row at `y` from `lo` to `hi`, in increasing x, with split
/// lines `sp`: one where the row crosses each line (none nearer than `edge` to an end or to the
/// one before), the tatami `grid` points at least `edge` from them (programmed splits keep only
/// those that break a gap too long for one stitch), and even steps through any gap still too
/// long.
fn split_points(sp: &Splits, y: f64, (lo, hi): (f64, f64), grid: &[f64], step: f64, edge: f64, out: &mut Vec<f64>) {
    let mut cuts = Vec::new();
    sp.crossings(y, &mut cuts);
    let mut kept: Vec<f64> = Vec::with_capacity(cuts.len());
    for c in cuts {
        if c >= lo + edge && c <= hi - edge && kept.last().is_none_or(|&k| c - k >= edge) {
            kept.push(c);
        }
    }
    let long = step * 1.35;
    let mut grid = grid.iter().copied().peekable();
    let mut gap: Vec<f64> = Vec::new();
    // Each gap from `a` to `b` between the row's ends and the crossings.
    let (mut a, mut a_cut) = (lo, false);
    for (i, b) in kept.iter().copied().chain(std::iter::once(hi)).enumerate() {
        let b_cut = i < kept.len();
        let wide = b - a > long;
        gap.clear();
        gap.push(a);
        while let Some(x) = grid.next_if(|&x| x < b) {
            let clear = x > a && (!a_cut || x - a >= edge) && (!b_cut || b - x >= edge);
            if clear && (!sp.pattern || wide) {
                gap.push(x);
            }
        }
        gap.push(b);
        for w in gap.windows(2) {
            let [p, q] = w else { continue };
            if q - p > long {
                let n = ((q - p) / step).ceil().min(1e4) as usize;
                out.extend((1..n).map(|k| p + (q - p) * k as f64 / n as f64));
            }
            if *q < b || b_cut {
                out.push(*q);
            }
        }
        (a, a_cut) = (b, b_cut);
    }
}

/// Fills one polygon. Returns needle positions in design space.
pub(crate) fn scan_fill(polygon: &Polygon, cfg: &ScanConfig, hint: Option<Point>, exit: Option<Point>) -> Vec<Point> {
    let theta = cfg.angle.to_radians();
    let fwd = Affine::rotate(theta);
    let inv = Affine::rotate(-theta);
    let mut p = polygon.clone();
    p.transform(&fwd);
    let rings: Vec<Vec<Point>> = p.rings().filter(|r| r.len() >= 3).cloned().collect();
    if rings.is_empty() {
        return Vec::new();
    }
    let Some(mut plan) = plan(&rings, cfg) else {
        return Vec::new();
    };
    let splits = Splits::new(cfg, &rings, &fwd);
    let hint_rs = hint.map(|h| fwd.apply(h));
    let exit_rs = exit.map(|e| fwd.apply(e));
    let pc = cfg.pull_comp;
    let travel = cfg.travel_length.max(0.5);

    let mut sewn = vec![false; plan.spans.len()];
    let mut done = vec![false; plan.sections.len()];
    let mut remaining = plan.sections.len();
    let mut pts: Vec<Point> = Vec::new();
    let mut cur: Option<usize> = None;
    let mut xs = Vec::new();
    let node_pt = |n: usize| plan.graph.pts.get(n).copied().unwrap_or_default();
    let to_exit = |e: Entry| exit_rs.map_or(0.0, |ex| node_pt(plan.exit_node(e)).distance(ex));
    // With a stop point, the section that can end nearest it is sewn last, so the fill finishes
    // there instead of travelling back over sewn rows.
    let last_sec: Option<usize> = if exit_rs.is_some() && plan.sections.len() > 1 {
        (0..plan.sections.len())
            .flat_map(|s| plan.entries(s).collect::<Vec<_>>())
            .min_by(|a, b| to_exit(*a).total_cmp(&to_exit(*b)))
            .map(|e| e.section)
    } else {
        None
    };

    let push_run = |pts: &mut Vec<Point>, to: Point, step: f64| {
        if let Some(&from) = pts.last() {
            let d = from.distance(to);
            if d < 1e-9 {
                return;
            }
            let n = ((d / step).ceil() as usize).clamp(1, MAX_FILL_STITCHES);
            for k in 1..=n {
                pts.push(from.lerp(to, k as f64 / n as f64));
            }
        } else {
            pts.push(to);
        }
    };

    'sections: while remaining > 0 {
        if pts.len() >= MAX_FILL_STITCHES {
            break;
        }
        let entry = match cur {
            None => {
                // First section: the entry closest to the hint, or the top-left one.
                let target = hint_rs.unwrap_or_else(|| {
                    let b = polyline::bbox(&rings[0]);
                    Point::new(b.min.x, b.min.y)
                });
                // one section: of the entries near the start, the one ending nearest the stop
                let single = plan.sections.len() == 1;
                let score = |e: Entry| node_pt(plan.entry_node(e)).distance(target) + if single { 0.5 * to_exit(e) } else { 0.0 };
                let first = (0..plan.sections.len())
                    .filter(|&s| Some(s) != last_sec)
                    .flat_map(|s| plan.entries(s).collect::<Vec<_>>())
                    .min_by(|a, b| score(*a).total_cmp(&score(*b)));
                let Some(first) = first else { break };
                first
            }
            Some(node) => {
                let (dist, prev) = dijkstra(&plan.graph, node, &sewn, cfg.trapunto);
                let here = plan.graph.pts[node];
                let cost = |e: Entry, dist: &[f64], from: Point| {
                    let n = plan.entry_node(e);
                    if dist[n].is_finite() { dist[n] } else { from.distance(plan.graph.pts[n]) * NO_ROUTE_COST }
                };
                let mut cands: Vec<(f64, Entry)> = (0..plan.sections.len())
                    .filter(|&s| !done[s] && (remaining <= 1 || Some(s) != last_sec))
                    .flat_map(|s| plan.entries(s).collect::<Vec<_>>())
                    .map(|e| (cost(e, &dist, here), e))
                    .collect();
                cands.sort_by(|a, b| a.0.total_cmp(&b.0));
                // Look one section ahead for the nearest few sections, trying every
                // entry of each: entering a branch from its far end is often what
                // keeps later travel hidden.
                let k_sections = if remaining <= 1 {
                    1
                } else if remaining <= 30 {
                    4
                } else if remaining <= 100 {
                    2
                } else {
                    1
                };
                let mut chosen_sections: Vec<usize> = Vec::new();
                for (_, e) in &cands {
                    if !chosen_sections.contains(&e.section) {
                        chosen_sections.push(e.section);
                        if chosen_sections.len() == k_sections {
                            break;
                        }
                    }
                }
                let Some(&first_cand) = cands.first() else { break };
                let mut best = first_cand;
                if remaining == 1 && exit_rs.is_some() {
                    // the last section: entered where it ends nearest the stop point
                    if let Some(&b) = cands.iter().min_by(|a, b| (a.0 + to_exit(a.1)).total_cmp(&(b.0 + to_exit(b.1)))) {
                        best = b;
                    }
                } else if remaining > 1 && k_sections > 1 {
                    let mut best_score = f64::INFINITY;
                    for &(c1, e) in cands.iter().filter(|(_, e)| chosen_sections.contains(&e.section)) {
                        for &s in &plan.sections[e.section] {
                            sewn[s] = true;
                        }
                        let exit_node = plan.exit_node(e);
                        let (d2, _) = dijkstra(&plan.graph, exit_node, &sewn, cfg.trapunto);
                        let from = plan.graph.pts[exit_node];
                        let c2 = (0..plan.sections.len())
                            .filter(|&s| !done[s] && s != e.section && (remaining <= 2 || Some(s) != last_sec))
                            .flat_map(|s| plan.entries(s).collect::<Vec<_>>())
                            .map(|e2| cost(e2, &d2, from))
                            .fold(f64::INFINITY, f64::min);
                        for &s in &plan.sections[e.section] {
                            sewn[s] = false;
                        }
                        let score = c1 + if c2.is_finite() { c2 } else { 0.0 };
                        if score < best_score {
                            best_score = score;
                            best = (c1, e);
                        }
                    }
                }
                let e = best.1;
                let target = plan.entry_node(e);
                match path_to(&prev, node, target) {
                    Some(path) => travel_path(&mut pts, &plan.graph, &path, travel),
                    None => push_run(&mut pts, plan.graph.pts[target], travel),
                }
                e
            }
        };

        // Sew the section back and forth.
        let sec = plan.sections[entry.section].clone();
        let order: Vec<usize> = if entry.from_first { sec.clone() } else { sec.iter().rev().copied().collect() };
        let mut left_to_right = entry.left;
        for (i, &si) in order.iter().enumerate() {
            let s = &plan.spans[si];
            let (xa, xb) = if left_to_right { (s.l.x - pc, s.r.x + pc) } else { (s.r.x + pc, s.l.x - pc) };
            let start = Point::new(xa, s.y);
            if i == 0 && pts.is_empty() {
                pts.push(start);
            } else {
                push_run(&mut pts, start, travel.min(cfg.stitch_length.max(1.0)));
            }
            row_points(xa, xb, (s.row, s.y), cfg, splits.as_ref(), &mut xs);
            for &x in xs.iter().skip(1) {
                pts.push(Point::new(x, s.y));
            }
            sewn[si] = true;
            left_to_right = !left_to_right;
            if pts.len() >= MAX_FILL_STITCHES {
                break 'sections;
            }
        }
        done[entry.section] = true;
        remaining -= 1;
        cur = Some(plan.exit_node(entry));
    }

    // Finish near the requested exit point (not when sewing stopped at the cap).
    let capped = pts.len() >= MAX_FILL_STITCHES;
    pts.truncate(MAX_FILL_STITCHES);
    if let (false, Some(node), Some(ex)) = (capped, cur, exit) {
        let ex_rs = fwd.apply(ex);
        let (_, prev) = dijkstra(&plan.graph, node, &sewn, cfg.trapunto);
        let target = (0..plan.graph.pts.len()).min_by(|&a, &b| plan.graph.pts[a].distance(ex_rs).total_cmp(&plan.graph.pts[b].distance(ex_rs)));
        if let Some(t) = target
            && let Some(path) = path_to(&prev, node, t)
        {
            travel_path(&mut pts, &plan.graph, &path, travel);
        }
    }
    plan.graph.adj.clear();
    pts.into_iter().map(|p| inv.apply(p)).collect()
}

/// Number of sections and rows a straight fill at `angle` would need.
pub(crate) fn statistics(polygon: &Polygon, angle: f64, spacing: f64) -> Option<(usize, usize)> {
    let mut p = polygon.clone();
    p.transform(&Affine::rotate(angle.to_radians()));
    let rings: Vec<Vec<Point>> = p.rings().filter(|r| r.len() >= 3).cloned().collect();
    let cfg = ScanConfig { row_spacing: spacing.max(0.05), ..ScanConfig::default() };
    let plan = plan(&rings, &cfg)?;
    let rows = plan.spans.iter().map(|s| s.row).max().map_or(0, |m| m + 1);
    Some((plan.sections.len(), rows))
}

/// Fills several polygons (e.g. the pieces of a split shape) in nearest-first
/// order, connecting them with runs or trimmed jumps.
pub(crate) fn scan_fill_all(polys: &[Polygon], cfg: &ScanConfig, out: &mut Out, hint: Option<Point>, exit: Option<Point>, trim_threshold: f64) {
    let mut left: Vec<&Polygon> = polys.iter().filter(|p| !p.is_empty()).collect();
    while !left.is_empty() {
        let here = out.pos().or(hint);
        let i = match here {
            Some(h) => (0..left.len()).min_by(|&a, &b| left[a].distance_to_boundary(h).total_cmp(&left[b].distance_to_boundary(h))).unwrap_or(0),
            None => 0,
        };
        let p = left.remove(i);
        let ex = if left.is_empty() { exit } else { None };
        let pts = scan_fill(p, cfg, here, ex);
        if let Some(&first) = pts.first() {
            out.connect(first, trim_threshold, cfg.travel_length);
            out.stitches(pts);
        }
    }
}

/// Clean polygons for filling: resolves self-intersections and drops slivers.
pub(crate) fn prepare(polygon: Polygon) -> Vec<Polygon> {
    poly::clean(&[polygon]).into_iter().filter(|p| p.area() > 0.01).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![Point::new(x0, y0), Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)]
    }

    #[test]
    fn fills_start_at_the_start_point_and_finish_at_the_stop_point() {
        let cfg = ScanConfig { row_spacing: 0.4, stitch_length: 3.0, ..ScanConfig::default() };
        // a square: start at the bottom, stop at the top: the rows climb from bottom to top
        let sq = Polygon::from_ring(rect(0.0, 0.0, 30.0, 30.0));
        let pts = scan_fill(&sq, &cfg, Some(Point::new(15.0, 31.0)), Some(Point::new(15.0, -1.0)));
        let (first, last) = (pts[0], *pts.last().unwrap());
        assert!(first.y > 29.0 && last.y < 1.0, "{first:?} → {last:?}");
        // no travel back down over the rows at the end
        let tail: Vec<f64> = pts.iter().rev().take(40).map(|p| p.y).collect();
        assert!(tail.iter().all(|y| *y < 2.0), "{tail:?}");
        // a U: start at the top of its right arm, stop at the top of its left arm: the left arm is
        // sewn last, ending at its top, with no long travel to get there
        let u = Polygon::from_ring(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 30.0),
            Point::new(20.0, 30.0),
            Point::new(20.0, 0.0),
            Point::new(30.0, 0.0),
            Point::new(30.0, 40.0),
            Point::new(0.0, 40.0),
        ]);
        let pts = scan_fill(&u, &cfg, Some(Point::new(25.0, -1.0)), Some(Point::new(5.0, -1.0)));
        let last = *pts.last().unwrap();
        assert!(last.distance(Point::new(5.0, 0.0)) < 6.0, "{last:?}");
        assert!(pts[0].distance(Point::new(25.0, 0.0)) < 6.0, "{:?}", pts[0]);
    }

    fn max_stitch(pts: &[Point]) -> f64 {
        pts.windows(2).map(|w| w[0].distance(w[1])).fold(0.0, f64::max)
    }

    /// The needle points of each row (by height), in increasing x.
    fn rows(pts: &[Point]) -> Vec<(f64, Vec<f64>)> {
        let mut by_y: std::collections::BTreeMap<i64, (f64, Vec<f64>)> = std::collections::BTreeMap::new();
        for p in pts {
            by_y.entry((p.y * 1e6).round() as i64).or_insert((p.y, Vec::new())).1.push(p.x);
        }
        by_y.into_values()
            .map(|(y, mut xs)| {
                xs.sort_by(f64::total_cmp);
                (y, xs)
            })
            .collect()
    }

    #[test]
    fn a_ragged_edge_ends_each_row_short_of_the_outline_at_random() {
        let poly = Polygon::from_ring(rect(0.0, 0.0, 30.0, 20.0));
        let ends = |ragged: f64| -> Vec<(f64, f64)> {
            let cfg = ScanConfig { ragged, ..Default::default() };
            rows(&scan_fill(&poly, &cfg, None, None)).into_iter().filter(|(_, xs)| xs.len() > 3).map(|(_, xs)| (xs[0], *xs.last().unwrap())).collect()
        };
        let spread = |v: &[f64]| v.iter().copied().fold(f64::MIN, f64::max) - v.iter().copied().fold(f64::MAX, f64::min);
        let clean = ends(0.0);
        let rough = ends(3.0);
        assert!(rough.len() > 20 && rough.len() == clean.len(), "{} rows, {} clean", rough.len(), clean.len());
        let (lefts, rights): (Vec<f64>, Vec<f64>) = rough.iter().copied().unzip();
        let (cl, cr): (Vec<f64>, Vec<f64>) = clean.iter().copied().unzip();
        assert!(spread(&cl) < 0.5 && spread(&cr) < 0.5, "a clean edge stays clean");
        assert!(spread(&lefts) > 1.5 && spread(&rights) > 1.5, "the ends wander: {lefts:?}");
        // inside the outline, never more than the depth in, and the same each time
        assert!(lefts.iter().all(|x| (-0.01..3.3).contains(x)) && rights.iter().all(|x| (26.7..30.01).contains(x)), "{rough:?}");
        assert_eq!(rough, ends(3.0));
    }

    #[test]
    fn rows_have_a_needle_point_where_they_cross_a_split_line() {
        let poly = Polygon::from_ring(rect(0.0, 0.0, 30.0, 20.0));
        let cfg = ScanConfig { splits: vec![vec![Point::new(10.3, -5.0), Point::new(10.3, 25.0)]], ..Default::default() };
        let pts = scan_fill(&poly, &cfg, None, None);
        let plain = rows(&scan_fill(&poly, &ScanConfig::default(), None, None));
        let split = rows(&pts);
        assert_eq!(split.len(), plain.len());
        for ((y, xs), (_, before)) in split.iter().zip(&plain) {
            assert!(xs.iter().any(|x| (x - 10.3).abs() < 1e-9), "row {y}: {xs:?}");
            // Away from the line the tatami grid stays as it was.
            let far = |v: &Vec<f64>| v.iter().copied().filter(|x| (x - 10.3).abs() > 4.0 * 1.35).collect::<Vec<_>>();
            assert_eq!(far(xs), far(before), "row {y}");
        }
        assert!(max_stitch(&pts) <= 4.0 * 1.35 + 1e-9, "{}", max_stitch(&pts));
    }

    #[test]
    fn a_texture_puts_the_needle_points_on_its_lines() {
        let poly = Polygon::from_ring(rect(0.0, 0.0, 40.0, 30.0));
        let cfg = ScanConfig { texture: Some(Texture { pattern: TexturePattern::Bricks, size: 6.0, angle: 0.0 }), ..Default::default() };
        let pts = scan_fill(&poly, &cfg, None, None);
        let edge = (4.0f64 * 0.35).clamp(0.4, 1.5);
        let mut checked = 0;
        for (y, xs) in rows(&pts) {
            // Courses 3 mm high: joints every 6 mm, half a brick along in every other course.
            if (y / 3.0 - (y / 3.0).round()).abs() < 1e-6 {
                continue;
            }
            let along = if (y / 3.0).floor() as i64 % 2 == 0 { 0.0 } else { 3.0 };
            for joint in (0..8).map(|k| 6.0 * f64::from(k) + along).filter(|x| *x >= edge && *x <= 40.0 - edge) {
                assert!(xs.iter().any(|x| (x - joint).abs() < 1e-9), "row {y}: no needle point at the joint {joint}: {xs:?}");
                checked += 1;
            }
        }
        assert!(checked > 400, "{checked}");
        assert!(max_stitch(&pts) <= 4.0 * 1.35 + 1e-9, "{}", max_stitch(&pts));
    }

    #[test]
    fn every_texture_fills_turned_shapes_soundly() {
        let poly = Polygon::from_ring(vec![Point::new(0.0, 0.0), Point::new(50.0, 10.0), Point::new(30.0, 45.0), Point::new(-5.0, 30.0)]);
        for pattern in TexturePattern::ALL {
            for (angle, turn) in [(0.0, 0.0), (30.0, 45.0), (-75.0, 90.0)] {
                let t = Texture { pattern, size: pattern.default_size(4.0), angle: turn };
                let cfg = ScanConfig { angle, texture: Some(t), ..Default::default() };
                let pts = scan_fill(&poly, &cfg, None, None);
                assert!(pts.len() > 500 && pts.iter().all(|p| p.is_finite()), "{pattern:?}");
                // Nothing longer than a long tatami stitch (or than plain tatami's longest: a short
                // row with no needle point in it).
                let plain = max_stitch(&scan_fill(&poly, &ScanConfig { angle, ..Default::default() }, None, None));
                assert!(max_stitch(&pts) <= plain.max(4.0 * 1.35) + 1e-6, "{pattern:?} at {angle}°: {}", max_stitch(&pts));
            }
        }
    }

    #[test]
    fn a_spacing_profile_sets_the_rows_across_the_shape() {
        let poly = Polygon::from_ring(rect(0.0, 0.0, 20.0, 40.0));
        let ys = |cfg: &ScanConfig| rows(&scan_fill(&poly, cfg, None, None)).iter().map(|r| r.0).collect::<Vec<f64>>();
        // Accordion: dense at the ends and in the middle, sparse in between.
        let acc = ys(&ScanConfig { profile: vec![[0.0, 0.4], [0.25, 2.0], [0.5, 0.4], [0.75, 2.0], [1.0, 0.4]], ..Default::default() });
        let band = |v: &[f64], a: f64, b: f64| v.iter().filter(|y| **y >= a && **y < b).count();
        assert!(band(&acc, 0.0, 4.0) > 2 * band(&acc, 8.0, 12.0) && band(&acc, 18.0, 22.0) > 2 * band(&acc, 8.0, 12.0), "{acc:?}");
        // A fade to nothing halfway across.
        let fade = ys(&ScanConfig { profile: vec![[0.0, 0.4], [0.5, 0.0]], ..Default::default() });
        assert!(fade.len() > 10 && fade.iter().all(|y| *y < 20.0), "{fade:?}");
        // Two layers fading into each other add up to an even fill.
        let a = ys(&ScanConfig { profile: vec![[0.0, 0.4], [1.0, 0.0]], ..Default::default() }).len() as f64;
        let b = ys(&ScanConfig { profile: vec![[0.0, 0.0], [1.0, 0.4]], ..Default::default() }).len() as f64;
        let even = ys(&ScanConfig::default()).len() as f64;
        assert!(((a + b) / even - 1.0).abs() < 0.1, "{a} + {b} vs {even}");
        // No rows anywhere; points that aren't numbers are no profile.
        assert!(scan_fill(&poly, &ScanConfig { profile: vec![[0.0, 0.0], [1.0, 0.0]], ..Default::default() }, None, None).is_empty());
        let junk = ScanConfig { profile: vec![[f64::NAN, 1.0], [0.5, f64::INFINITY]], ..Default::default() };
        assert_eq!(scan_fill(&poly, &junk, None, None), scan_fill(&poly, &ScanConfig::default(), None, None));
        assert!(Profile::new(&[[f64::NAN, 1.0]]).is_none());
        assert!(Profile::new(&[[0.0, 1e-300], [1.0, 1e300]]).is_some_and(|p| p.density(0.5).is_finite()));
    }

    #[test]
    fn trapunto_travels_along_the_outline() {
        // A square ring, every row sewn. From the left end of the band below the hole to its
        // right end, plain travel runs along a row (on top of the stitching, where it hardly
        // shows); trapunto goes round the outline instead.
        let ring = Polygon { outer: rect(0.0, 0.0, 30.0, 30.0), holes: vec![rect(10.0, 10.0, 20.0, 20.0)] };
        let rings: Vec<Vec<Point>> = ring.rings().cloned().collect();
        let plan = plan(&rings, &ScanConfig { row_spacing: 2.0, ..Default::default() }).unwrap();
        let near =
            |p: Point| (0..plan.graph.pts.len()).min_by(|&a, &b| plan.graph.pts[a].distance(p).total_cmp(&plan.graph.pts[b].distance(p))).unwrap();
        let (from, to) = (near(Point::new(0.0, 25.0)), near(Point::new(30.0, 25.0)));
        let sewn = vec![true; plan.spans.len()];
        for (trapunto, along_rows) in [(false, true), (true, false)] {
            let (_, prev) = dijkstra(&plan.graph, from, &sewn, trapunto);
            let path = path_to(&prev, from, to).unwrap();
            let kinds: Vec<EdgeKind> = path.windows(2).filter_map(|w| plan.graph.adj[w[0]].iter().find(|e| e.to == w[1]).map(|e| e.kind)).collect();
            assert_eq!(kinds.len() + 1, path.len());
            let rows = kinds.iter().any(|k| matches!(k, EdgeKind::Span(_)));
            assert_eq!(rows, along_rows, "trapunto {trapunto}: {kinds:?}");
        }
    }

    #[test]
    fn absurd_textures_and_split_lines_stay_sound() {
        let poly = Polygon::from_ring(rect(0.0, 0.0, 30.0, 20.0));
        for (size, turn) in [(1e308, 0.0), (-5.0, 1e308), (f64::NAN, f64::NAN), (0.0, -1e-308), (f64::INFINITY, 45.0)] {
            for pattern in TexturePattern::ALL {
                let cfg = ScanConfig { texture: Some(Texture { pattern, size, angle: turn }), ..Default::default() };
                let pts = scan_fill(&poly, &cfg, None, None);
                assert!(!pts.is_empty() && pts.iter().all(|p| p.is_finite()), "{pattern:?} {size} {turn}");
                assert!(max_stitch(&pts) <= 4.0 * 1.35 + 1e-6);
            }
        }
        let lines = vec![
            vec![Point::new(0.0, 0.0), Point::new(1e308, -1e308)],
            vec![Point::new(-1e308, -1e308), Point::new(1e308, 1e308)],
            vec![Point::new(f64::NAN, 0.0), Point::new(5.0, 5.0)],
            vec![Point::new(5.0, 5.0)],
            vec![],
        ];
        let pts = scan_fill(&poly, &ScanConfig { splits: lines, ..Default::default() }, None, None);
        assert!(!pts.is_empty() && pts.iter().all(|p| p.is_finite()));
        // Satin rows aren't split.
        let satin =
            ScanConfig { stitch_length: f64::INFINITY, splits: vec![vec![Point::new(10.0, -1.0), Point::new(10.0, 21.0)]], ..Default::default() };
        let plain = ScanConfig { stitch_length: f64::INFINITY, ..Default::default() };
        assert_eq!(scan_fill(&poly, &satin, None, None), scan_fill(&poly, &plain, None, None));
    }

    #[test]
    fn a_huge_plain_fill_stops_at_the_stitch_cap() {
        // issue #74: a 1 km square asked for ~770 million stitches
        let poly = Polygon::from_ring(rect(0.0, 0.0, 1_000_000.0, 1_000_000.0));
        for cfg in [ScanConfig::default(), ScanConfig { row_spacing_end: 2.0, ..Default::default() }] {
            let pts = scan_fill(&poly, &cfg, Some(Point::new(0.0, 0.0)), Some(Point::new(1_000_000.0, 1_000_000.0)));
            assert!(!pts.is_empty() && pts.len() <= MAX_FILL_STITCHES, "{}", pts.len());
            assert!(pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
        }
        // a 1 m square is still sewn whole
        let poly = Polygon::from_ring(rect(0.0, 0.0, 1000.0, 1000.0));
        let rows = row_heights(0.0, 1000.0, &ScanConfig::default()).unwrap();
        assert!(rows.len() > 2000 && rows.len() < MAX_PROFILE_ROWS, "{}", rows.len());
        assert!(scan_fill(&poly, &ScanConfig::default(), None, None).len() < MAX_FILL_STITCHES);
    }

    #[test]
    fn square_tatami() {
        let poly = Polygon::from_ring(rect(0.0, 0.0, 20.0, 20.0));
        let cfg = ScanConfig { angle: 0.0, ..Default::default() };
        let pts = scan_fill(&poly, &cfg, None, None);
        assert!(pts.len() > 50 * 5, "{}", pts.len());
        assert!(max_stitch(&pts) <= 4.0 + 1.5 + 1e-6, "{}", max_stitch(&pts));
        for p in &pts {
            assert!(p.x >= -1e-6 && p.x <= 20.0 + 1e-6 && p.y >= 0.0 && p.y <= 20.0, "{p:?}");
        }
        // ~50 rows at 0.4 mm
        let mut ys: Vec<i64> = pts.iter().map(|p| (p.y * 1000.0).round() as i64).collect();
        ys.sort();
        ys.dedup();
        assert!((48..=52).contains(&ys.len()), "{}", ys.len());
    }

    #[test]
    fn angled_fill_stays_inside() {
        let poly = Polygon::from_ring(rect(0.0, 0.0, 30.0, 12.0));
        for angle in [0.0, 30.0, 45.0, 90.0, 135.0] {
            let cfg = ScanConfig { angle, ..Default::default() };
            let pts = scan_fill(&poly, &cfg, None, None);
            assert!(!pts.is_empty());
            for p in &pts {
                assert!(p.x > -0.01 && p.x < 30.01 && p.y > -0.01 && p.y < 12.01, "{angle}: {p:?}");
            }
        }
    }

    #[test]
    fn donut_avoids_hole() {
        let outer: Vec<Point> = (0..64)
            .map(|i| {
                let a = i as f64 / 64.0 * std::f64::consts::TAU;
                Point::new(20.0 * a.cos(), 20.0 * a.sin())
            })
            .collect();
        let hole: Vec<Point> = outer.iter().map(|p| *p * 0.4).collect();
        let poly = Polygon::new(outer, vec![hole]);
        let cfg = ScanConfig { angle: 30.0, ..Default::default() };
        let pts = scan_fill(&poly, &cfg, Some(Point::new(-25.0, 0.0)), None);
        assert!(pts.len() > 600, "{}", pts.len());
        for p in &pts {
            assert!(p.length() > 7.9 - 1e-6, "inside hole: {p:?}");
            assert!(p.length() < 20.0 + 1e-6, "outside: {p:?}");
        }
        assert!(max_stitch(&pts) < 6.0);
        // starts near the hint
        assert!(pts[0].distance(Point::new(-20.0, 0.0)) < 3.0, "{:?}", pts[0]);
    }

    #[test]
    fn u_shape_covers_both_arms() {
        // U: two arms connected at the bottom.
        let ring = vec![
            Point::new(0.0, 0.0),
            Point::new(5.0, 0.0),
            Point::new(5.0, 20.0),
            Point::new(15.0, 20.0),
            Point::new(15.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(20.0, 25.0),
            Point::new(0.0, 25.0),
        ];
        let poly = Polygon::from_ring(ring);
        let cfg = ScanConfig { angle: 0.0, ..Default::default() };
        let pts = scan_fill(&poly, &cfg, None, None);
        let left = pts.iter().filter(|p| p.x < 5.0 && p.y < 19.0).count();
        let right = pts.iter().filter(|p| p.x > 15.0 && p.y < 19.0).count();
        assert!(left > 60 && right > 60, "{left} {right}");
        for p in &pts {
            let in_notch = p.x > 5.05 && p.x < 14.95 && p.y < 19.95;
            assert!(!in_notch, "stitch in the notch: {p:?}");
        }
        assert!(max_stitch(&pts) <= 5.5 + 1e-6, "{}", max_stitch(&pts));
    }

    #[test]
    fn satin_rows_and_split() {
        let poly = Polygon::from_ring(rect(0.0, 0.0, 5.0, 3.0));
        let cfg = ScanConfig { stitch_length: f64::INFINITY, row_spacing: 0.2, ..Default::default() };
        let pts = scan_fill(&poly, &cfg, None, None);
        // one stitch per row (plus connections): every point on an edge
        assert!(pts.iter().all(|p| p.x.abs() < 1e-6 || (p.x - 5.0).abs() < 1e-6));
        let poly = Polygon::from_ring(rect(0.0, 0.0, 20.0, 3.0));
        let cfg = ScanConfig { stitch_length: f64::INFINITY, split_length: 7.0, ..Default::default() };
        let pts = scan_fill(&poly, &cfg, None, None);
        assert!(max_stitch(&pts) <= 7.0 + 1e-6, "{}", max_stitch(&pts));
    }

    #[test]
    fn satin_split_length_has_a_minimum() {
        // #84: a tiny split length is raised to MIN_SPLIT_LENGTH instead of making millions of stitches.
        let poly = Polygon::from_ring(rect(0.0, 0.0, 20.0, 10.0));
        let tiny = ScanConfig { stitch_length: f64::INFINITY, split_length: 1e-6, ..Default::default() };
        let at_min = ScanConfig { split_length: crate::model::MIN_SPLIT_LENGTH, ..tiny.clone() };
        let pts = scan_fill(&poly, &tiny, None, None);
        assert_eq!(pts.len(), scan_fill(&poly, &at_min, None, None).len());
        assert!(pts.len() < 50_000, "{}", pts.len());
    }

    #[test]
    fn gradient_rows_spread_out() {
        let poly = Polygon::from_ring(rect(0.0, 0.0, 10.0, 20.0));
        let cfg = ScanConfig { row_spacing: 0.3, row_spacing_end: 2.0, ..Default::default() };
        let pts = scan_fill(&poly, &cfg, None, None);
        let top = pts.iter().filter(|p| p.y < 5.0).count();
        let bottom = pts.iter().filter(|p| p.y > 15.0).count();
        assert!(top > bottom * 2, "{top} {bottom}");
    }
}
