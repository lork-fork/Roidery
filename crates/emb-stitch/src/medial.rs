//! Roidery: Convert to Satin. Any closed shape becomes satin columns (Satin Blocks rails) whose
//! stitches cross the shape square to its spine.
//!
//! 1. The shape's medial axis, the points as far from one side as from the other, is drawn into
//!    a grid one cell wide: worked out exactly from the outline (a Voronoi diagram of its
//!    segments, by the `centerline` crate), or, for an outline it can't take (one that crosses
//!    itself) and in the web build, by thinning the shape drawn into the grid.
//! 2. The short spurs every corner sprouts are pruned: a branch only stays when it reaches well
//!    beyond the widest circle that fits where it joins.
//! 3. The longest path through the skeleton is the spine (a loop, for a ring like the letter O).
//!    It is smoothed, and an open spine is carried on to the outline at both ends, so the
//!    column reaches the shape's ends.
//! 4. Along the spine, rungs are cast square to it out to the outline on both sides. Where the
//!    spine bends tighter than the column is wide, the inner ends would cross over; they stay
//!    put instead, so the rungs fan round the bend.
//! 5. Where a rail would cut a corner off the outline, a rung goes to the corner; an open
//!    column's ends close in on the corners beyond its last rungs, so ends are sewn to the tip.
//! 6. Rungs that a straight block between their neighbours already gives are dropped, leaving
//!    rungs that are easy to edit by hand.
//!
//! What the spine leaves of the skeleton (the arms of a Y or a star) becomes more columns, each
//! from just inside the column it leaves to its tip, longest first.
//!
//! Written from the idea (Blum's medial axis, Zhang–Suen thinning); no code is copied. The Voronoi
//! diagram comes from the `centerline` and `boostvoronoi` crates (MIT/Apache-2.0, BSL-1.0).

use kurbo::{Point, Vec2};

use crate::geometry::Polyline;

/// Spacing of the rungs cast along the spine, mm.
const STEP: f64 = 0.5;
/// How far a dropped rung may be from the straight block between its neighbours, mm.
const TOLERANCE: f64 = 0.15;
/// Grid cells along the longer side of the shape, at most.
const GRID: f64 = 450.0;
/// Grid cells at most (a 4000 × 1000 shape and the like).
const MAX_CELLS: usize = 1_500_000;
/// Columns one shape can make.
const MAX_COLUMNS: usize = 32;
/// Outline segments the Voronoi diagram is worked out for, at most; above it the grid is thinned.
#[cfg(not(target_arch = "wasm32"))]
const MAX_VORONOI_SEGMENTS: usize = 20_000;
/// Width above which a column is too wide to satin well, mm.
pub const WIDE_MM: f64 = 10.0;

/// One satin column: rung `i` joins `left[i]` and `right[i]` (mm).
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub left: Vec<Point>,
    pub right: Vec<Point>,
    /// A ring: the last rung is the first again.
    pub closed: bool,
}

impl Column {
    /// The longest rung, mm.
    pub fn widest(&self) -> f64 {
        self.left.iter().zip(&self.right).map(|(l, r)| l.distance(*r)).fold(0.0, f64::max)
    }
}

/// How Convert to Satin finds the spine and casts the rungs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// The exact medial axis (Voronoi, the `centerline` crate), with arms bisected into the
    /// column they leave and rungs sent to the outline's corners and an open end's tip.
    Centerline,
    /// Roidery's first way, kept to compare: the grid thinned to its skeleton, and rungs only
    /// where they're cast along the spine.
    Classic,
}

/// The satin columns of the shape bounded by `rings` (mm, even-odd), main column first; empty
/// when the shape is too small or not closed.
pub fn to_satin(rings: &[Polyline]) -> Vec<Column> {
    to_satin_by(rings, Method::Centerline)
}

/// [`to_satin`], by `method`.
pub fn to_satin_by(rings: &[Polyline], method: Method) -> Vec<Column> {
    let classic = method == Method::Classic;
    let outline = Outline::new(rings);
    let Some(mut grid) = Grid::of(&outline) else { return vec![] };
    if classic || !grid.trace_medial_axis(&outline) {
        grid.thin();
    }
    let mut out = vec![];
    let mut first_pass = true;
    while out.len() < MAX_COLUMNS {
        let mut found = false;
        for comp in grid.components() {
            if out.len() >= MAX_COLUMNS {
                break;
            }
            grid.prune(&comp, &outline);
            let comp: Vec<usize> = comp.into_iter().filter(|&i| grid.on(i)).collect();
            let Some((path, closed)) = grid.longest_path(&comp) else { continue };
            let mut spine: Vec<Point> = path.iter().map(|&i| grid.center(i)).collect();
            if let [p] = spine[..] {
                // A round blob thins to one cell: sew it across, side to side.
                spine = vec![p - Vec2::new(grid.h, 0.0), p + Vec2::new(grid.h, 0.0)];
            }
            let len = spine.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>();
            let r_end = spine.first().map_or(0.0, |p| outline.distance(*p)).max(spine.last().map_or(0.0, |p| outline.distance(*p)));
            // After the main columns, an arm must reach well beyond the column it leaves.
            if !first_pass && (len < 1.5 || len < r_end) {
                grid.clear(&comp);
                continue;
            }
            let spine = smooth(&spine, (1.0 / grid.h).round().clamp(1.0, 40.0) as usize, closed);
            let mut spine = resample(&spine, STEP, closed);
            // Spine points carried on beyond the skeleton, at the start and at the end.
            let mut carried = (0, 0);
            if !closed {
                // The first pass's spines run to the outline at both ends; an arm only at the end
                // away from the column it leaves.
                let (first, last) = if first_pass { (None, None) } else { arm_gaps(&spine, &out) };
                (spine, carried) = extend(&spine, &outline, &out, first, last, classic);
            }
            if let Some(c) = cast(&spine, carried, closed, &outline, classic) {
                out.push(c);
                found = true;
            }
            grid.stamp_out(&path, &outline);
        }
        first_pass = false;
        if !found {
            break;
        }
    }
    // Main column first, then the arms longest first.
    if out.len() > 2 {
        let len = |c: &Column| c.left.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>();
        out[1..].sort_by(|a, b| len(b).total_cmp(&len(a)));
    }
    out
}

/// The shape's outline, for distances and ray casts.
struct Outline {
    /// Segments, with the ring each belongs to and where along it each starts.
    segs: Vec<(Point, Point, usize, f64)>,
    rings: Vec<Vec<Point>>,
    perimeters: Vec<f64>,
}

impl Outline {
    fn new(rings: &[Polyline]) -> Self {
        let mut segs = vec![];
        let mut kept = vec![];
        let mut perimeters = vec![];
        for r in rings {
            let pts: Vec<Point> = r.points.iter().copied().filter(|p| p.x.is_finite() && p.y.is_finite()).collect();
            if pts.len() < 3 {
                continue;
            }
            let ring = kept.len();
            let mut along = 0.0;
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                segs.push((a, b, ring, along));
                along += a.distance(b);
            }
            perimeters.push(along);
            kept.push(pts);
        }
        Self { segs, rings: kept, perimeters }
    }

    fn bounds(&self) -> Option<(Point, Point)> {
        let mut it = self.rings.iter().flatten();
        let first = *it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| (Point::new(lo.x.min(p.x), lo.y.min(p.y)), Point::new(hi.x.max(p.x), hi.y.max(p.y)))))
    }

    /// The ring nearest `p` and how far along it the nearest point is.
    fn locate(&self, p: Point) -> Option<(usize, f64)> {
        let mut best: Option<(f64, usize, f64)> = None;
        for &(a, b, ring, along) in &self.segs {
            let ab = b - a;
            let len2 = ab.hypot2();
            let t = if len2 < 1e-18 { 0.0 } else { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) };
            let dist = a.lerp(b, t).distance(p);
            if best.is_none_or(|x| dist < x.0) {
                best = Some((dist, ring, along + t * len2.sqrt()));
            }
        }
        best.filter(|b| b.0 < 0.05).map(|b| (b.1, b.2))
    }

    /// Whether `p` is inside (even-odd).
    fn contains(&self, p: Point) -> bool {
        let mut inside = false;
        for (a, b, ..) in &self.segs {
            if (a.y <= p.y) != (b.y <= p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                inside = !inside;
            }
        }
        inside
    }

    /// Distance from `p` to the nearest point of the outline.
    fn distance(&self, p: Point) -> f64 {
        self.segs.iter().map(|(a, b, ..)| seg_distance(*a, *b, p)).fold(f64::INFINITY, f64::min)
    }

    /// The first crossing of the ray from `p` along unit `d`, within `max`: (distance, ring,
    /// where along the ring).
    fn cast(&self, p: Point, d: Vec2, max: f64) -> Option<(f64, usize, f64)> {
        let mut best: Option<(f64, usize, f64)> = None;
        for &(a, b, ring, along) in &self.segs {
            let e = b - a;
            let den = d.cross(e);
            if den.abs() < 1e-12 {
                continue;
            }
            let w = a - p;
            let t = w.cross(e) / den;
            let u = w.cross(d) / den;
            if t > 1e-9 && t <= max && (-1e-9..=1.0 + 1e-9).contains(&u) && best.is_none_or(|b| t < b.0) {
                best = Some((t, ring, along + e.hypot() * u.clamp(0.0, 1.0)));
            }
        }
        best
    }

    /// The point `s` along ring `ring` (wrapping round).
    fn point_on(&self, ring: usize, s: f64) -> Option<Point> {
        let pts = self.rings.get(ring)?;
        let per = *self.perimeters.get(ring)?;
        if per <= 0.0 {
            return pts.first().copied();
        }
        let mut left = s.rem_euclid(per);
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            let l = a.distance(b);
            if left <= l {
                return Some(if l < 1e-12 { a } else { a.lerp(b, left / l) });
            }
            left -= l;
        }
        pts.first().copied()
    }
}

/// The web build has no Voronoi diagram: it thins.
#[cfg(target_arch = "wasm32")]
fn voronoi_axis(_: &Outline, _: f64, _: Point) -> Option<Vec<Vec<Point>>> {
    None
}

/// The medial axis of `outline` as lines (mm), worked out exactly from its segments rounded to
/// `quantum` mm; `None` for an outline the diagram can't be built for (one that crosses or
/// touches itself, or one too big).
#[cfg(not(target_arch = "wasm32"))]
fn voronoi_axis(outline: &Outline, quantum: f64, origin: Point) -> Option<Vec<Vec<Point>>> {
    use boostvoronoi::prelude::Line;
    if !(quantum.is_finite() && quantum > 0.0) || outline.segs.len() > MAX_VORONOI_SEGMENTS {
        return None;
    }
    let to = |p: Point| -> Option<[i32; 2]> {
        let (x, y) = ((p.x - origin.x) / quantum, (p.y - origin.y) / quantum);
        (x.is_finite() && y.is_finite() && x.abs() < 1e8 && y.abs() < 1e8).then(|| [x.round() as i32, y.round() as i32])
    };
    let mut segs: Vec<[i32; 4]> = vec![];
    for ring in &outline.rings {
        let mut pts: Vec<[i32; 2]> = vec![];
        for p in ring {
            let q = to(*p)?;
            if pts.last() != Some(&q) {
                pts.push(q);
            }
        }
        while pts.len() > 1 && pts.first() == pts.last() {
            pts.pop();
        }
        if pts.len() < 3 {
            continue;
        }
        for (i, a) in pts.iter().enumerate() {
            let b = pts.get(i + 1).or(pts.first())?;
            segs.push([a[0], a[1], b[0], b[1]]);
        }
    }
    if segs.len() < 3 || crosses_itself(&segs) {
        return None;
    }
    let lines: Vec<Line<i32>> = segs.iter().map(|s| (*s).into()).collect();
    // The diagram is someone else's code: a panic in it falls back to thinning, not a crash.
    let axis = std::panic::catch_unwind(move || {
        let mut c = centerline::Centerline::<i32, vector_traits::glam::DVec3>::with_segments(lines);
        c.build_voronoi().ok()?;
        // Keep the edges that run between two sides, not those that run into a corner of one.
        c.calculate_centerline(0.38, 0.25, None).ok()?;
        let mut out: Vec<Vec<[f64; 2]>> = c.line_strings.unwrap_or_default().iter().map(|l| l.iter().map(|v| [v.x, v.y]).collect()).collect();
        out.extend(c.lines.unwrap_or_default().iter().map(|l| vec![[l.start.x, l.start.y], [l.end.x, l.end.y]]));
        Some(out)
    })
    .ok()??;
    let back = |[x, y]: [f64; 2]| Point::new(origin.x + x * quantum, origin.y + y * quantum);
    Some(axis.into_iter().map(|l| l.into_iter().map(back).collect()).collect())
}

/// Whether any two of `segs` (closed rings, in order) cross or touch, other than neighbours
/// meeting end to end.
#[cfg(not(target_arch = "wasm32"))]
fn crosses_itself(segs: &[[i32; 4]]) -> bool {
    let bbox = |s: &[i32; 4]| (s[0].min(s[2]), s[1].min(s[3]), s[0].max(s[2]), s[1].max(s[3]));
    let mut order: Vec<usize> = (0..segs.len()).collect();
    order.sort_by_key(|&i| segs.get(i).map_or(0, |s| bbox(s).0));
    for (k, &i) in order.iter().enumerate() {
        let Some(a) = segs.get(i) else { continue };
        let ba = bbox(a);
        for &j in order.get(k + 1..).unwrap_or_default() {
            let Some(b) = segs.get(j) else { continue };
            let bb = bbox(b);
            if bb.0 > ba.2 {
                break;
            }
            if bb.1 > ba.3 || bb.3 < ba.1 {
                continue;
            }
            let shared = [(a[0], a[1]), (a[2], a[3])].iter().filter(|p| **p == (b[0], b[1]) || **p == (b[2], b[3])).count();
            if shared == 1 {
                // Neighbours meet at one end; they must not also overlap along a line.
                if collinear_overlap(a, b) {
                    return true;
                }
                continue;
            }
            if shared > 1 || segments_touch(a, b) {
                return true;
            }
        }
    }
    false
}

#[cfg(not(target_arch = "wasm32"))]
fn orient(a: (i64, i64), b: (i64, i64), c: (i64, i64)) -> i64 {
    ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).signum()
}

#[cfg(not(target_arch = "wasm32"))]
fn on_segment(a: (i64, i64), b: (i64, i64), p: (i64, i64)) -> bool {
    p.0 >= a.0.min(b.0) && p.0 <= a.0.max(b.0) && p.1 >= a.1.min(b.1) && p.1 <= a.1.max(b.1)
}

#[cfg(not(target_arch = "wasm32"))]
fn ends(s: &[i32; 4]) -> ((i64, i64), (i64, i64)) {
    ((i64::from(s[0]), i64::from(s[1])), (i64::from(s[2]), i64::from(s[3])))
}

#[cfg(not(target_arch = "wasm32"))]
fn segments_touch(a: &[i32; 4], b: &[i32; 4]) -> bool {
    let ((p1, p2), (q1, q2)) = (ends(a), ends(b));
    let (o1, o2, o3, o4) = (orient(p1, p2, q1), orient(p1, p2, q2), orient(q1, q2, p1), orient(q1, q2, p2));
    (o1 != o2 && o3 != o4)
        || (o1 == 0 && on_segment(p1, p2, q1))
        || (o2 == 0 && on_segment(p1, p2, q2))
        || (o3 == 0 && on_segment(q1, q2, p1))
        || (o4 == 0 && on_segment(q1, q2, p2))
}

#[cfg(not(target_arch = "wasm32"))]
fn collinear_overlap(a: &[i32; 4], b: &[i32; 4]) -> bool {
    let ((p1, p2), (q1, q2)) = (ends(a), ends(b));
    if orient(p1, p2, q1) != 0 || orient(p1, p2, q2) != 0 {
        return false;
    }
    // The far end of either lies on the other: they fold back over each other.
    let far_b = if q1 == p1 || q1 == p2 { q2 } else { q1 };
    let far_a = if p1 == q1 || p1 == q2 { p2 } else { p1 };
    on_segment(p1, p2, far_b) || on_segment(q1, q2, far_a)
}

fn seg_distance(a: Point, b: Point, p: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.hypot2();
    if len2 < 1e-18 {
        return a.distance(p);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    a.lerp(b, t).distance(p)
}

/// The shape drawn into square cells `h` mm across; `true` inside, then on the skeleton.
struct Grid {
    cells: Vec<bool>,
    cols: usize,
    rows: usize,
    origin: Point,
    h: f64,
}

/// The eight neighbours, clockwise from north (y down): N, NE, E, SE, S, SW, W, NW.
const RING8: [(i64, i64); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];

impl Grid {
    fn of(outline: &Outline) -> Option<Self> {
        let (lo, hi) = outline.bounds()?;
        let size = (hi.x - lo.x).max(hi.y - lo.y);
        if !(size.is_finite() && size > 0.2) {
            return None;
        }
        let mut h = (size / GRID).clamp(0.02, 2.0);
        let dims = |h: f64| (((hi.x - lo.x) / h).ceil() as usize + 4, ((hi.y - lo.y) / h).ceil() as usize + 4);
        while dims(h).0 * dims(h).1 > MAX_CELLS {
            h *= 1.25;
        }
        let (cols, rows) = dims(h);
        let origin = Point::new(lo.x - 2.0 * h, lo.y - 2.0 * h);
        let mut cells = vec![false; cols * rows];
        // Even-odd scanlines through the cell centres.
        let mut xs = vec![];
        for row in 0..rows {
            let y = origin.y + (row as f64 + 0.5) * h;
            xs.clear();
            for &(a, b, ..) in &outline.segs {
                if (a.y <= y) != (b.y <= y) {
                    xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.chunks_exact(2) {
                let c0 = ((pair[0] - origin.x) / h - 0.5).ceil().max(0.0) as usize;
                let c1 = ((pair[1] - origin.x) / h - 0.5).floor();
                if c1 < 0.0 {
                    continue;
                }
                for c in c0..=(c1 as usize).min(cols - 1) {
                    cells[row * cols + c] = true;
                }
            }
        }
        Some(Self { cells, cols, rows, origin, h })
    }

    fn on(&self, i: usize) -> bool {
        self.cells.get(i).copied().unwrap_or(false)
    }

    fn center(&self, i: usize) -> Point {
        let (c, r) = (i % self.cols, i / self.cols);
        Point::new(self.origin.x + (c as f64 + 0.5) * self.h, self.origin.y + (r as f64 + 0.5) * self.h)
    }

    fn neighbour(&self, i: usize, k: usize) -> Option<usize> {
        let (dx, dy) = *RING8.get(k)?;
        let (c, r) = ((i % self.cols) as i64 + dx, (i / self.cols) as i64 + dy);
        (c >= 0 && r >= 0 && (c as usize) < self.cols && (r as usize) < self.rows).then(|| r as usize * self.cols + c as usize)
    }

    fn ring(&self, i: usize) -> [bool; 8] {
        std::array::from_fn(|k| self.neighbour(i, k).is_some_and(|j| self.on(j)))
    }

    fn neighbours(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        (0..8).filter_map(move |k| self.neighbour(i, k)).filter(|&j| self.on(j))
    }

    fn degree(&self, i: usize) -> usize {
        self.neighbours(i).count()
    }

    /// Replace the shape in the grid with its exact medial axis, one cell wide; `false` (the
    /// grid untouched) when the outline can't be worked out that way.
    fn trace_medial_axis(&mut self, outline: &Outline) -> bool {
        let Some(lines) = voronoi_axis(outline, self.h / 16.0, self.origin) else { return false };
        let inside = std::mem::replace(&mut self.cells, vec![false; self.cols * self.rows]);
        let cell = |p: Point| {
            let (c, r) = ((p.x - self.origin.x) / self.h, (p.y - self.origin.y) / self.h);
            (c.is_finite() && r.is_finite() && c >= 0.0 && r >= 0.0 && (c as usize) < self.cols && (r as usize) < self.rows)
                .then_some((c as i64, r as i64))
        };
        let mut drawn = vec![];
        for line in &lines {
            for w in line.windows(2) {
                let (Some(&a), Some(&b)) = (w.first(), w.get(1)) else { continue };
                // Only the axis inside the shape: the diagram also has the holes' axes.
                if !outline.contains(a.midpoint(b)) {
                    continue;
                }
                let (Some((c0, r0)), Some((c1, r1))) = (cell(a), cell(b)) else { continue };
                // One cell wide: a step to the next cell, square or diagonal, at a time.
                let n = (c1 - c0).abs().max((r1 - r0).abs());
                for k in 0..=n {
                    let t = if n == 0 { 0.0 } else { k as f64 / n as f64 };
                    let (c, r) = ((c0 as f64 + t * (c1 - c0) as f64).round() as usize, (r0 as f64 + t * (r1 - r0) as f64).round() as usize);
                    let i = r * self.cols + c;
                    if inside.get(i).copied().unwrap_or(false)
                        && let Some(x) = self.cells.get_mut(i)
                    {
                        *x = true;
                        drawn.push(i);
                    }
                }
            }
        }
        if drawn.is_empty() {
            self.cells = inside;
            return false;
        }
        drawn.sort_unstable();
        drawn.dedup();
        self.unstair(drawn);
        true
    }

    /// Zhang–Suen thinning, then the staircase corners it leaves, so the skeleton is one cell
    /// wide and a cell on it has two neighbours except at ends and junctions.
    fn thin(&mut self) {
        let mut live: Vec<usize> = (0..self.cells.len()).filter(|&i| self.cells[i]).collect();
        loop {
            let mut changed = false;
            for step in 0..2 {
                let gone: Vec<usize> = live
                    .iter()
                    .copied()
                    .filter(|&i| self.cells[i])
                    .filter(|&i| {
                        let n = self.ring(i);
                        let b = n.iter().filter(|x| **x).count();
                        let a = (0..8).filter(|&k| !n[k] && n[(k + 1) % 8]).count();
                        let (p2, p4, p6, p8) = (n[0], n[2], n[4], n[6]);
                        let first = if step == 0 { !(p2 && p4 && p6) && !(p4 && p6 && p8) } else { !(p2 && p4 && p8) && !(p2 && p6 && p8) };
                        (2..=6).contains(&b) && a == 1 && first
                    })
                    .collect();
                changed |= !gone.is_empty();
                for i in gone {
                    self.cells[i] = false;
                }
            }
            live.retain(|&i| self.cells[i]);
            if !changed {
                break;
            }
        }
        self.unstair(live);
    }

    /// A cell whose two square neighbours touch each other diagonally carries nothing.
    fn unstair(&mut self, live: Vec<usize>) {
        for i in live {
            if !self.cells[i] {
                continue;
            }
            let n = self.ring(i);
            let corner = |a: usize, b: usize, opp: [usize; 3]| n[a] && n[b] && opp.iter().all(|&k| !n[k]);
            if corner(0, 2, [4, 5, 6]) || corner(2, 4, [6, 7, 0]) || corner(4, 6, [0, 1, 2]) || corner(6, 0, [2, 3, 4]) {
                self.cells[i] = false;
            }
        }
    }

    /// The skeleton's connected parts, largest first.
    fn components(&self) -> Vec<Vec<usize>> {
        let mut seen = vec![false; self.cells.len()];
        let mut out = vec![];
        for start in 0..self.cells.len() {
            if !self.cells[start] || seen[start] {
                continue;
            }
            let mut comp = vec![];
            let mut stack = vec![start];
            seen[start] = true;
            while let Some(i) = stack.pop() {
                comp.push(i);
                for j in self.neighbours(i).collect::<Vec<_>>() {
                    if !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
            out.push(comp);
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.len()));
        out
    }

    /// Cut the spurs off `comp`: chains from an end to a junction no longer than the junction's
    /// widest circle, give or take; shortest first, and never down to fewer than two ends.
    fn prune(&mut self, comp: &[usize], outline: &Outline) {
        for _ in 0..24 {
            let mut spurs: Vec<(f64, Vec<usize>, usize)> = comp
                .iter()
                .copied()
                .filter(|&i| self.on(i) && self.degree(i) == 1)
                .filter_map(|e| self.spur(e, comp.len()))
                .filter(|(len, _, j)| *len < 1.6 * outline.distance(self.center(*j)) + 2.0 * self.h)
                .collect();
            spurs.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut cut = false;
            for (_, chain, j) in spurs {
                // Still a spur (an earlier cut can leave its junction an end).
                if chain.iter().any(|&i| !self.on(i)) || self.degree(j) < 2 {
                    continue;
                }
                let ends = comp.iter().filter(|&&i| self.on(i) && self.degree(i) == 1).count();
                // The junction is left an end itself when only one other branch meets it.
                let after = ends - 1 + usize::from(self.degree(j) == 2);
                if after < 2 {
                    continue;
                }
                self.clear(&chain);
                cut = true;
            }
            if !cut {
                break;
            }
        }
    }

    /// The chain from end `e` to the junction it leads to: its length, its cells (the junction
    /// not among them) and the junction.
    fn spur(&self, e: usize, limit: usize) -> Option<(f64, Vec<usize>, usize)> {
        let mut chain = vec![e];
        let mut prev = e;
        let mut cur = self.neighbours(e).next()?;
        loop {
            match self.degree(cur) {
                2 => {
                    let next = self.neighbours(cur).find(|&j| j != prev)?;
                    chain.push(cur);
                    prev = cur;
                    cur = next;
                }
                d if d >= 3 => return Some((chain.len() as f64 * self.h, chain, cur)),
                _ => return None,
            }
            if chain.len() > limit {
                return None;
            }
        }
    }

    /// The longest path through `comp`, as cells, and whether it is a loop.
    fn longest_path(&self, comp: &[usize]) -> Option<(Vec<usize>, bool)> {
        let start = *comp.first()?;
        if comp.len() == 1 {
            return Some((vec![start], false));
        }
        if comp.len() >= 8 && comp.iter().all(|&i| self.degree(i) == 2) {
            // A loop: walk round it.
            let mut path = vec![start];
            let mut prev = start;
            let mut cur = self.neighbours(start).next()?;
            while cur != start && path.len() <= comp.len() {
                path.push(cur);
                let next = self.neighbours(cur).find(|&j| j != prev)?;
                prev = cur;
                cur = next;
            }
            return Some((path, true));
        }
        // Between two ends, the path that goes far and stays straight: its length plus half the
        // distance between its ends (so a plus sign's bar wins over turning into an arm).
        let leaves: Vec<usize> = comp.iter().copied().filter(|&i| self.degree(i) == 1).collect();
        let (a, b, prev) = if (2..=48).contains(&leaves.len()) {
            let mut best: Option<(f64, usize, usize)> = None;
            for &a in &leaves {
                let (dist, _) = self.distances(a, comp);
                for &b in &leaves {
                    let Some(d) = dist.get(&b).filter(|_| b != a) else { continue };
                    let score = d * self.h + 0.5 * self.center(a).distance(self.center(b));
                    if best.is_none_or(|x| score > x.0) {
                        best = Some((score, a, b));
                    }
                }
            }
            let (_, a, b) = best?;
            (a, b, self.distances(a, comp).1)
        } else {
            let from = leaves.first().copied().unwrap_or(start);
            let (a, _) = self.farthest(from, comp);
            let (b, prev) = self.farthest(a, comp);
            (a, b, prev)
        };
        let mut path = vec![b];
        let mut cur = b;
        while cur != a {
            cur = *prev.get(&cur)?;
            path.push(cur);
            if path.len() > comp.len() {
                return None;
            }
        }
        (path.len() >= 2).then_some((path, false))
    }

    /// The cell of `comp` farthest from `from` along the skeleton, with each cell's predecessor.
    fn farthest(&self, from: usize, comp: &[usize]) -> (usize, std::collections::HashMap<usize, usize>) {
        let (dist, prev) = self.distances(from, comp);
        let far = dist.iter().max_by(|a, b| a.1.total_cmp(b.1)).map_or(from, |(&i, _)| i);
        (far, prev)
    }

    /// Distances (in cells) from `from` along the skeleton, and each cell's predecessor.
    #[allow(clippy::type_complexity)]
    fn distances(&self, from: usize, comp: &[usize]) -> (std::collections::HashMap<usize, f64>, std::collections::HashMap<usize, usize>) {
        use std::collections::{BinaryHeap, HashMap};
        let mut dist: HashMap<usize, f64> = HashMap::with_capacity(comp.len());
        let mut prev = HashMap::with_capacity(comp.len());
        // Distances in hundredths of a cell, so the heap orders integers.
        let mut heap = BinaryHeap::new();
        dist.insert(from, 0.0);
        heap.push(std::cmp::Reverse((0_u64, from)));
        while let Some(std::cmp::Reverse((d, i))) = heap.pop() {
            let d = d as f64 / 100.0;
            if dist.get(&i).is_some_and(|&best| d > best + 1e-2) {
                continue;
            }
            for k in 0..8 {
                let Some(j) = self.neighbour(i, k).filter(|&j| self.on(j)) else { continue };
                let nd = d + if k % 2 == 0 { 1.0 } else { std::f64::consts::SQRT_2 };
                if dist.get(&j).is_none_or(|&best| nd < best - 1e-9) {
                    dist.insert(j, nd);
                    prev.insert(j, i);
                    heap.push(std::cmp::Reverse(((nd * 100.0) as u64, j)));
                }
            }
        }
        (dist, prev)
    }

    fn clear(&mut self, cells: &[usize]) {
        for &i in cells {
            if let Some(c) = self.cells.get_mut(i) {
                *c = false;
            }
        }
    }

    /// Take a column's spine off the skeleton, with everything inside the column.
    fn stamp_out(&mut self, path: &[usize], outline: &Outline) {
        let every = ((STEP / self.h).round() as usize).max(1);
        for (n, &i) in path.iter().enumerate() {
            if n % every != 0 && n + 1 != path.len() {
                if let Some(c) = self.cells.get_mut(i) {
                    *c = false;
                }
                continue;
            }
            let c = self.center(i);
            let r = outline.distance(c) + 1.5 * self.h;
            let cells = (r / self.h).ceil() as i64;
            let (ci, ri) = ((i % self.cols) as i64, (i / self.cols) as i64);
            for dy in -cells..=cells {
                for dx in -cells..=cells {
                    let (x, y) = (ci + dx, ri + dy);
                    if x < 0 || y < 0 || x as usize >= self.cols || y as usize >= self.rows {
                        continue;
                    }
                    let j = y as usize * self.cols + x as usize;
                    if self.cells[j] && self.center(j).distance(c) <= r {
                        self.cells[j] = false;
                    }
                }
            }
        }
    }
}

/// A moving average `k` points either side (ends held, a loop wraps).
fn smooth(pts: &[Point], k: usize, closed: bool) -> Vec<Point> {
    let n = pts.len();
    if n < 3 || k == 0 {
        return pts.to_vec();
    }
    (0..n)
        .map(|i| {
            if !closed && (i == 0 || i + 1 == n) {
                return pts[i];
            }
            // Open: a window that shrinks near the ends, so the ends stay put.
            let k = if closed { k.min(n / 3) } else { k.min(i).min(n - 1 - i) };
            let mut sum = Vec2::ZERO;
            for j in 0..=2 * k {
                let idx = (i + n + j - k) % n;
                sum += pts[idx].to_vec2();
            }
            (sum / (2 * k + 1) as f64).to_point()
        })
        .collect()
}

/// Points every `step` along `pts` (closed: round the loop, not repeating the start).
fn resample(pts: &[Point], step: f64, closed: bool) -> Vec<Point> {
    let mut chain = pts.to_vec();
    if closed && let Some(&f) = pts.first() {
        chain.push(f);
    }
    let total: f64 = chain.windows(2).map(|w| w[0].distance(w[1])).sum();
    if total < 1e-9 {
        return pts.to_vec();
    }
    let n = ((total / step).round() as usize).clamp(2, 200_000);
    let mut out = Vec::with_capacity(n + 1);
    let count = if closed { n } else { n + 1 };
    let mut seg = 0;
    let mut seg_start = 0.0;
    for k in 0..count {
        let target = total * k as f64 / n as f64;
        while seg + 2 < chain.len() && seg_start + chain[seg].distance(chain[seg + 1]) < target {
            seg_start += chain[seg].distance(chain[seg + 1]);
            seg += 1;
        }
        let (a, b) = (chain[seg], chain[seg + 1]);
        let l = a.distance(b);
        out.push(if l < 1e-12 { a } else { a.lerp(b, ((target - seg_start) / l).clamp(0.0, 1.0)) });
    }
    out
}

/// How far each end of an arm's spine is from the columns already made (their rails), or
/// `None` for a tip: the end that is farther away.
fn arm_gaps(spine: &[Point], columns: &[Column]) -> (Option<f64>, Option<f64>) {
    let gap = |p: Point| rail_gap(columns, p);
    match (spine.first(), spine.last()) {
        (Some(&a), Some(&b)) => {
            let (ga, gb) = (gap(a), gap(b));
            if ga <= gb { (Some(ga), None) } else { (None, Some(gb)) }
        }
        _ => (None, None),
    }
}

/// Distance from `p` to the nearest rail of `columns`.
fn rail_gap(columns: &[Column], p: Point) -> f64 {
    columns
        .iter()
        .flat_map(|c| [&c.left, &c.right])
        .flat_map(|rail| rail.windows(2).map(|w| seg_distance(w[0], w[1], p)))
        .fold(f64::INFINITY, f64::min)
}

/// Carry an open spine on along its ends: a tip (`None`) out to the outline, and an arm's
/// other end (`Some(gap)`) across the gap to the column it leaves, a little into it. Returns
/// how many points each end gained.
fn extend(
    spine: &[Point],
    outline: &Outline,
    columns: &[Column],
    first: Option<f64>,
    last: Option<f64>,
    classic: bool,
) -> (Vec<Point>, (usize, usize)) {
    let mut gained = (0, 0);
    let mut out = spine.to_vec();
    let back = (1.5 / STEP).round() as usize;
    let reach = |end: Point, inner: Point, gap: Option<f64>| -> Vec<Point> {
        let d = end - inner;
        let len = d.hypot();
        if len < 1e-9 {
            return vec![];
        }
        let d = d / len;
        let max = 3.0 * outline.distance(end) + 1.0;
        let hit = outline.cast(end, d, max).map(|(t, ..)| (t - 0.02).max(0.0));
        let t = match (gap, hit) {
            (None, Some(t)) => t,
            (Some(g), hit) if g.is_finite() && classic => hit.map_or(g + 0.3, |t| t.min(g + 0.3)),
            (Some(g), hit) if g.is_finite() => {
                // As far as the nearest rail along this way (the arm may meet the column
                // slantwise, so further than the gap), and a little into the column.
                let mut reach = g;
                let mut best = g;
                for k in 1..=((3.0 * g + 1.0) / 0.05).ceil().min(2000.0) as usize {
                    let t = k as f64 * 0.05;
                    let here = rail_gap(columns, end + d * t);
                    if here > best + 0.05 {
                        break;
                    }
                    if here < best {
                        (best, reach) = (here, t);
                    }
                }
                let t = reach + 0.3;
                hit.map_or(t, |h| h.min(t))
            }
            _ => return vec![],
        };
        let n = (t / STEP).ceil() as usize;
        (1..=n).map(|k| end + d * (t * k as f64 / n as f64)).collect()
    };
    if let (Some(&end), Some(&inner)) = (out.last(), out.get(out.len().saturating_sub(1 + back))) {
        let more = reach(end, inner, last);
        gained.1 = more.len();
        out.extend(more);
    }
    if let (Some(&end), Some(&inner)) = (out.first(), out.get(back.min(out.len().saturating_sub(1)))) {
        let mut more = reach(end, inner, first);
        gained.0 = more.len();
        more.reverse();
        more.extend(out);
        out = more;
    }
    (out, gained)
}

/// Rungs square to the spine out to the outline, fanned where they'd cross, then thinned out.
fn cast(spine: &[Point], carried: (usize, usize), closed: bool, outline: &Outline, classic: bool) -> Option<Column> {
    let n = spine.len();
    if n < 2 {
        return None;
    }
    // A hit on the outline, or where the rung stops without one.
    type End = (Point, Option<(usize, f64)>);
    let mut ends: Vec<(End, End)> = Vec::with_capacity(n);
    // How far a rung may reach: well past the widest circle nearby, so the ends carried on to
    // the outline still find the sides, and a rung never runs off across the whole shape.
    let radius: Vec<f64> = spine.iter().map(|p| outline.distance(*p)).collect();
    let window = (3.0 / STEP).round() as usize;
    // The rung at `c` square to `t`, for spine point `i`: `None` where it can't be cast.
    let rung = |i: usize, c: Point, t: Vec2| -> Option<(End, End)> {
        let len = t.hypot();
        if len < 1e-12 {
            return None;
        }
        let normal = Vec2::new(-t.y, t.x) / len;
        let near = radius.get(i.saturating_sub(window)..(i + window + 1).min(n)).unwrap_or_default();
        let max = 2.5 * near.iter().copied().fold(0.0, f64::max) + 0.2;
        let side = |d: Vec2| -> Option<End> { outline.cast(c, d, max).map(|(t, ring, s)| (c + d * t, Some((ring, s)))) };
        // Where a side is open (an arm leaves there), the rungs either side bridge the gap.
        let (Some(l), Some(r)) = (side(normal), side(-normal)) else { return None };
        // On the skeleton, the sides are about the widest circle's radius away; a side much
        // farther off has run out through an opening (into an arm, across a junction), and the
        // rungs either side bridge it instead.
        // Carried on past the skeleton, the nearest skeleton point's circle says.
        let k = i.clamp(carried.0, n.saturating_sub(carried.1 + 1).max(carried.0));
        let reach = 1.6 * radius.get(k).copied().unwrap_or(radius[i]) + 0.3;
        (l.0.distance(c) <= reach && r.0.distance(c) <= reach).then_some((l, r))
    };
    let tangent = |i: usize| {
        let (a, b) = if closed { (spine[(i + n - 1) % n], spine[(i + 1) % n]) } else { (spine[i.saturating_sub(1)], spine[(i + 1).min(n - 1)]) };
        b - a
    };
    let mut cast_at = vec![];
    for (i, &c) in spine.iter().enumerate() {
        if let Some(e) = rung(i, c, tangent(i)) {
            ends.push(e);
            cast_at.push(i);
        }
    }
    // An open spine whose last points (carried on into the column an arm leaves) cast no rung:
    // the furthest rung that can be cast before the first that can't, so the arm reaches.
    if !closed && !classic {
        let edge = |from: usize, to: usize| -> Option<(End, End)> {
            let (a, b) = (spine[from], spine[to]);
            let t = if to > from { b - a } else { a - b };
            let mut good = None;
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..12 {
                let mid = (lo + hi) / 2.0;
                match rung(from, a.lerp(b, mid), t) {
                    Some(e) => {
                        good = Some(e);
                        lo = mid;
                    }
                    None => hi = mid,
                }
            }
            good
        };
        if let Some(&last) = cast_at.last()
            && last + 1 < n
            && let Some(e) = edge(last, last + 1)
        {
            ends.push(e);
        }
        if let Some(&first) = cast_at.first()
            && first > 0
            && let Some(e) = edge(first, first - 1)
        {
            ends.insert(0, e);
        }
    }
    if ends.len() < 2 {
        return None;
    }
    let mut left: Vec<Point> = fan(ends.iter().map(|e| e.0).collect(), outline, closed);
    let mut right: Vec<Point> = fan(ends.iter().map(|e| e.1).collect(), outline, closed);
    if closed && let (Some(&l), Some(&r)) = (left.first(), right.first()) {
        left.push(l);
        right.push(r);
    }
    if !classic {
        (left, right) = fill_corners(left, right, outline);
        (right, left) = fill_corners(right, left, outline);
        if !closed {
            (left, right) = cap_ends(left, right, outline);
        }
    }
    let keep = thin_rungs(&left, &right);
    left = keep.iter().map(|&i| left[i]).collect();
    right = keep.iter().map(|&i| right[i]).collect();
    (left.len() >= 2).then_some(Column { left, right, closed })
}

/// Rungs added where a rail would cut a corner off the outline: one to each corner the
/// outline turns between two of the rail's ends, the other end placed proportionally between
/// the other rail's.
fn fill_corners(rail: Vec<Point>, other: Vec<Point>, outline: &Outline) -> (Vec<Point>, Vec<Point>) {
    if rail.len() != other.len() || rail.len() < 2 {
        return (rail, other);
    }
    let mut out = (Vec::with_capacity(rail.len()), Vec::with_capacity(rail.len()));
    for i in 0..rail.len() {
        let (Some(&a), Some(&oa)) = (rail.get(i), other.get(i)) else { break };
        out.0.push(a);
        out.1.push(oa);
        let (Some(&b), Some(&ob)) = (rail.get(i + 1), other.get(i + 1)) else { continue };
        let (Some((ring, sa)), Some((ring_b, sb))) = (outline.locate(a), outline.locate(b)) else { continue };
        let (Some(pts), Some(&per)) = (outline.rings.get(ring), outline.perimeters.get(ring)) else { continue };
        if ring != ring_b || per <= 0.0 {
            continue;
        }
        let d = (sb - sa + per / 2.0).rem_euclid(per) - per / 2.0;
        if d.abs() < 1e-9 || d.abs() > per / 4.0 {
            continue;
        }
        // The ring's corners on the way from a to b, in order.
        let mut along = 0.0;
        let mut corners = vec![];
        for (k, v) in pts.iter().enumerate() {
            let off = ((along - sa) * d.signum()).rem_euclid(per);
            if off > 1e-6 && off < d.abs() - 1e-6 && seg_distance(a, b, *v) > TOLERANCE {
                corners.push((off, *v));
            }
            along += pts.get(k + 1).or(pts.first()).map_or(0.0, |n| v.distance(*n));
        }
        corners.sort_by(|x, y| x.0.total_cmp(&y.0));
        // A rung no longer than those either side: never one across into another part.
        let longest = 1.25 * a.distance(oa).max(b.distance(ob)) + 0.2;
        for (off, v) in corners {
            let o = oa.lerp(ob, off / d.abs());
            if v.distance(o) <= longest {
                out.0.push(v);
                out.1.push(o);
            }
        }
    }
    out
}

/// The outline's corners beyond an open column's end rungs, closed in by rungs from either side
/// meeting at the tip, so a pointed or square end is sewn right to its corners.
fn cap_ends(mut left: Vec<Point>, mut right: Vec<Point>, outline: &Outline) -> (Vec<Point>, Vec<Point>) {
    let cap = |l: Point, r: Point, inward: Point| -> Vec<(Point, Point)> {
        let (Some((ring, sl)), Some((ring_r, sr))) = (outline.locate(l), outline.locate(r)) else { return vec![] };
        let (Some(pts), Some(&per)) = (outline.rings.get(ring), outline.perimeters.get(ring)) else { return vec![] };
        if ring != ring_r || per <= 0.0 {
            return vec![];
        }
        let width = l.distance(r);
        let across = r - l;
        // The side of the rung away from the column.
        let away = |p: Point| across.cross(p - l).signum() != across.cross(inward - l).signum();
        for dir in [1.0, -1.0] {
            let span = ((sr - sl) * dir).rem_euclid(per);
            if span > 3.0 * width + 1.0 {
                continue;
            }
            let mut along = 0.0;
            let mut corners = vec![];
            for (k, v) in pts.iter().enumerate() {
                let off = ((along - sl) * dir).rem_euclid(per);
                if off > 1e-6 && off < span - 1e-6 && seg_distance(l, r, *v) > TOLERANCE {
                    corners.push((off, *v));
                }
                along += pts.get(k + 1).or(pts.first()).map_or(0.0, |n| v.distance(*n));
            }
            if corners.is_empty() || !corners.iter().all(|c| away(c.1)) {
                continue;
            }
            corners.sort_by(|x, y| x.0.total_cmp(&y.0));
            let k = corners.len();
            // Innermost pair first, out to the tip.
            return (0..k.div_ceil(2)).filter_map(|i| Some((corners.get(i)?.1, corners.get(k - 1 - i)?.1))).collect();
        }
        vec![]
    };
    if let (Some(&l), Some(&r), Some(&l1), Some(&r1)) =
        (left.last(), right.last(), left.get(left.len().wrapping_sub(2)), right.get(right.len().wrapping_sub(2)))
    {
        for (a, b) in cap(l, r, l1.midpoint(r1)) {
            left.push(a);
            right.push(b);
        }
    }
    if let (Some(&l), Some(&r), Some(&l1), Some(&r1)) = (left.first(), right.first(), left.get(1), right.get(1)) {
        let mut head: Vec<(Point, Point)> = cap(l, r, l1.midpoint(r1));
        head.reverse();
        left.splice(0..0, head.iter().map(|h| h.0));
        right.splice(0..0, head.iter().map(|h| h.1));
    }
    (left, right)
}

/// One rail's ends, kept moving one way round the outline: an end that would go back (the
/// inside of a bend tighter than the column) stays where the rail had got to.
fn fan(ends: Vec<(Point, Option<(usize, f64)>)>, outline: &Outline, closed: bool) -> Vec<Point> {
    let pts: Vec<Point> = ends.iter().map(|e| e.0).collect();
    let Some(ring) = ends.first().and_then(|e| e.1.map(|h| h.0)) else { return pts };
    let Some(&per) = outline.perimeters.get(ring) else { return pts };
    let Some(along): Option<Vec<f64>> = ends.iter().map(|e| e.1.filter(|h| h.0 == ring).map(|h| h.1)).collect() else { return pts };
    if per <= 0.0 || along.len() < 3 {
        return pts;
    }
    let wrap = |d: f64| (d + per / 2.0).rem_euclid(per) - per / 2.0;
    let steps: Vec<f64> = along.windows(2).map(|w| wrap(w[1] - w[0])).collect();
    let dir = steps.iter().sum::<f64>().signum();
    if dir == 0.0 || (closed && steps.iter().map(|s| s.abs()).sum::<f64>() > 2.0 * per) {
        return pts;
    }
    let mut out = Vec::with_capacity(pts.len());
    let mut at = 0.0; // unwrapped position, from the first end
    let mut reached = 0.0;
    out.push(pts[0]);
    for (i, s) in steps.iter().enumerate() {
        at += s;
        if (at - reached) * dir >= 0.0 {
            reached = at;
            out.push(pts[i + 1]);
        } else {
            out.push(outline.point_on(ring, along[0] + reached).unwrap_or(pts[i + 1]));
        }
    }
    out
}

/// The rungs to keep: the first and last, and each that a straight block between the kept
/// rungs either side would miss by more than [`TOLERANCE`].
fn thin_rungs(left: &[Point], right: &[Point]) -> Vec<usize> {
    let n = left.len().min(right.len());
    if n <= 2 {
        return (0..n).collect();
    }
    let fits = |i: usize, j: usize| {
        (i + 1..j).all(|k| {
            let f = (k - i) as f64 / (j - i) as f64;
            left[i].lerp(left[j], f).distance(left[k]) <= TOLERANCE && right[i].lerp(right[j], f).distance(right[k]) <= TOLERANCE
        })
    };
    let mut keep = vec![0];
    let mut i = 0;
    while i + 1 < n {
        let mut j = i + 1;
        while j + 1 < n && fits(i, j + 1) {
            j += 1;
        }
        keep.push(j);
        i = j;
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poly(pts: &[(f64, f64)]) -> Polyline {
        Polyline::new(pts.iter().map(|&(x, y)| Point::new(x, y)).collect(), true)
    }

    fn circle(cx: f64, cy: f64, r: f64, n: usize) -> Polyline {
        poly(
            &(0..n)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / n as f64;
                    (cx + r * a.cos(), cy + r * a.sin())
                })
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn a_bar_becomes_one_column_across_its_width() {
        let cols = to_satin(&[poly(&[(0.0, 0.0), (40.0, 0.0), (40.0, 4.0), (0.0, 4.0)])]);
        assert_eq!(cols.len(), 1, "{cols:?}");
        let c = &cols[0];
        // Rungs cross the bar (4 mm), from end to end, with few rungs on a straight bar.
        for (l, r) in c.left.iter().zip(&c.right) {
            assert!((l.distance(*r) - 4.0).abs() < 0.05, "{l:?} {r:?}");
            assert!((l.x - r.x).abs() < 0.05);
        }
        let xs: Vec<f64> = c.left.iter().map(|p| p.x).collect();
        let (lo, hi) = (xs.iter().copied().fold(f64::MAX, f64::min), xs.iter().copied().fold(f64::MIN, f64::max));
        assert!(lo < 0.1 && hi > 39.9, "{lo} {hi}");
        assert!(c.left.len() <= 6, "{}", c.left.len());
        assert!(!c.closed);
    }

    #[test]
    fn a_ring_becomes_one_closed_column() {
        let cols = to_satin(&[circle(0.0, 0.0, 12.0, 180), circle(0.0, 0.0, 8.0, 180)]);
        assert_eq!(cols.len(), 1, "{:?}", cols.iter().map(|c| c.left.len()).collect::<Vec<_>>());
        let c = &cols[0];
        assert!(c.closed);
        assert_eq!(c.left.first(), c.left.last());
        for (l, r) in c.left.iter().zip(&c.right) {
            assert!((l.distance(*r) - 4.0).abs() < 0.3, "{}", l.distance(*r));
        }
        assert!(c.left.len() > 20);
    }

    #[test]
    fn an_arc_fans_round_its_bend_without_crossing() {
        // A thick C: outer radius 10, inner 4, so the column (6 wide) bends tighter than it is wide.
        let mut pts = vec![];
        for i in 0..=90 {
            let a = std::f64::consts::PI * (0.25 + 1.5 * i as f64 / 90.0);
            pts.push((10.0 * a.cos(), 10.0 * a.sin()));
        }
        for i in (0..=90).rev() {
            let a = std::f64::consts::PI * (0.25 + 1.5 * i as f64 / 90.0);
            pts.push((4.0 * a.cos(), 4.0 * a.sin()));
        }
        let outline = poly(&pts);
        let cols = to_satin(std::slice::from_ref(&outline));
        assert_eq!(cols.len(), 1);
        let c = &cols[0];
        let o = Outline::new(&[outline]);
        // Every rung goes from the outline to the outline, round the whole C.
        for (l, r) in c.left.iter().zip(&c.right) {
            assert!(o.distance(*l) < 0.05 && o.distance(*r) < 0.05, "{l:?} {r:?}");
        }
        assert!(c.left.iter().zip(&c.right).any(|(l, r)| l.y > 6.0 && r.y > 2.5));
        assert!(c.left.iter().zip(&c.right).any(|(l, r)| l.y < -6.0 && r.y < -2.5));
        // No two rungs cross.
        let crosses = |a: (Point, Point), b: (Point, Point)| {
            let s = |p: Point, q: Point, r: Point| (q - p).cross(r - p);
            let (d1, d2) = (s(a.0, a.1, b.0), s(a.0, a.1, b.1));
            let (d3, d4) = (s(b.0, b.1, a.0), s(b.0, b.1, a.1));
            d1 * d2 < -1e-9 && d3 * d4 < -1e-9
        };
        let rungs: Vec<(Point, Point)> = c.left.iter().copied().zip(c.right.iter().copied()).collect();
        for (i, a) in rungs.iter().enumerate() {
            for b in &rungs[i + 1..] {
                assert!(!crosses(*a, *b), "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn a_plus_sign_makes_a_column_for_each_arm() {
        let plus = poly(&[
            (8.0, 0.0),
            (12.0, 0.0),
            (12.0, 8.0),
            (20.0, 8.0),
            (20.0, 12.0),
            (12.0, 12.0),
            (12.0, 20.0),
            (8.0, 20.0),
            (8.0, 12.0),
            (0.0, 12.0),
            (0.0, 8.0),
            (8.0, 8.0),
        ]);
        let cols = to_satin(&[plus]);
        // One bar through, then the two arms either side of it.
        assert_eq!(cols.len(), 3, "{cols:?}");
        for c in &cols {
            assert!(c.widest() < 4.6, "{}", c.widest());
        }
    }

    #[test]
    fn a_star_makes_an_arm_each_without_rungs_across_its_middle() {
        // Five points, 17.6 mm out and 4.9 mm in.
        let pts: Vec<(f64, f64)> = (0..10)
            .map(|i| {
                let r = if i % 2 == 0 { 17.6 } else { 4.9 };
                let a = std::f64::consts::PI * (i as f64 / 5.0 - 0.5);
                (r * a.cos(), r * a.sin())
            })
            .collect();
        let cols = to_satin(&[poly(&pts)]);
        // One column through two points, then one for each of the other three.
        assert_eq!(cols.len(), 4, "{:?}", cols.iter().map(Column::widest).collect::<Vec<_>>());
        // No rung reaches across the middle into another point.
        for c in &cols {
            assert!(c.widest() < 2.0 * 4.9 + 0.5, "{}", c.widest());
        }
        // Each arm reaches the first column, so no fabric shows between them.
        let main = &cols[0];
        for arm in &cols[1..] {
            let gap = arm
                .left
                .iter()
                .chain(&arm.right)
                .flat_map(|p| [&main.left, &main.right].into_iter().flat_map(move |rail| rail.windows(2).map(move |w| seg_distance(w[0], w[1], *p))))
                .fold(f64::INFINITY, f64::min);
            assert!(gap < 0.3, "{gap}");
        }
    }

    #[test]
    fn the_medial_axis_is_exact_and_a_crossed_outline_falls_back_to_thinning() {
        let bar = poly(&[(0.0, 0.0), (30.0, 0.0), (30.0, 4.0), (0.0, 4.0)]);
        let outline = Outline::new(std::slice::from_ref(&bar));
        let mut grid = Grid::of(&outline).unwrap();
        assert!(grid.trace_medial_axis(&outline));
        // Along the middle, exactly: every skeleton cell between the end forks is on y = 2.
        let on: Vec<Point> = (0..grid.cells.len()).filter(|&i| grid.on(i)).map(|i| grid.center(i)).filter(|p| (3.0..27.0).contains(&p.x)).collect();
        assert!(!on.is_empty());
        assert!(on.iter().all(|p| (p.y - 2.0).abs() <= grid.h), "{on:?}");
        // A bow tie crosses itself: no diagram, but still a satin by thinning.
        let bow = poly(&[(0.0, 0.0), (20.0, 10.0), (20.0, 0.0), (0.0, 10.0)]);
        let outline = Outline::new(std::slice::from_ref(&bow));
        assert!(!Grid::of(&outline).unwrap().trace_medial_axis(&outline));
        assert!(!to_satin(&[bow]).is_empty());
        // Corners: a square end is sewn right out to both its corners.
        let cols = to_satin(&[bar]);
        let c = &cols[0];
        for corner in [Point::new(0.0, 0.0), Point::new(0.0, 4.0), Point::new(30.0, 0.0), Point::new(30.0, 4.0)] {
            let near = c.left.iter().chain(&c.right).map(|p| p.distance(corner)).fold(f64::INFINITY, f64::min);
            assert!(near < 0.1, "{corner:?}: {near}");
        }
    }

    #[test]
    fn junk_and_tiny_shapes_make_nothing() {
        assert!(to_satin(&[]).is_empty());
        // Shapes without a long axis still make a column, edge to edge.
        for shape in [circle(0.0, 0.0, 5.0, 90), poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)])] {
            let cols = to_satin(std::slice::from_ref(&shape));
            assert_eq!(cols.len(), 1, "{cols:?}");
            let o = Outline::new(&[shape]);
            assert!(cols[0].left.iter().chain(&cols[0].right).all(|p| o.distance(*p) < 0.05));
            assert!(cols[0].widest() > 9.0);
        }
        assert!(to_satin(&[poly(&[(0.0, 0.0), (0.1, 0.0), (0.0, 0.1)])]).is_empty());
        assert!(to_satin(&[poly(&[(f64::NAN, 0.0), (1.0, f64::INFINITY), (0.0, 1.0)])]).is_empty());
        // A huge shape still finishes (the grid is capped).
        let big = to_satin(&[poly(&[(0.0, 0.0), (5000.0, 0.0), (5000.0, 3.0), (0.0, 3.0)])]);
        assert!(big.len() <= 1);
    }
}
