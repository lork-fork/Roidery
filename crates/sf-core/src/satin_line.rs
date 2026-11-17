//! Satin lines: a satin column of some width along a centre line, open or closed,
//! with smart corners.
//!
//! Round a sharp corner the inside of a column is far shorter than the outside, so
//! rows that simply turned with the line would bunch up in the inner corner, or
//! fold back over each other. Instead the line is cut into legs at its corners and
//! each corner is joined by one of the [`Corners`] methods: rows fan round the
//! inner corner point of gentle corners, keep square to each leg and meet on the
//! miter line at medium ones, and sharp ones are mitered with the outer tip capped.

use crate::brush::WidthProfile;
use crate::geom::{Column, NodeKind, Point, Rung, polyline, screen_dir};
use crate::model::Corners;

const DEG: f64 = std::f64::consts::PI / 180.0;
/// A turn sharper than this is a corner; gentler ones belong to curves.
const CORNER: f64 = 30.0 * DEG;
/// Automatic corners fan their rows round turns up to this...
const AUTO_TURN: f64 = 45.0 * DEG;
/// ...meet on a miter up to this, and are capped beyond it.
const AUTO_MITER: f64 = 110.0 * DEG;
/// Miters are capped anyway beyond this turn: the tip would run far out.
const MITER_LIMIT: f64 = 150.0 * DEG;
/// Nearly reversing turns get no corner treatment.
const REVERSAL: f64 = 175.0 * DEG;
/// Rungs shorter than this (mm) are left out.
const MIN_RUNG: f64 = 0.02;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Method {
    Turn,
    Miter,
    Cap,
}

/// A corner of the centre line and how its two legs meet there.
#[derive(Clone, Copy, Debug)]
struct Join {
    v: Point,
    /// Unit direction into the corner.
    t1: Point,
    /// Unit direction out of the corner.
    t2: Point,
    /// +1 when the line turns towards the rungs' `a` side, −1 otherwise.
    sigma: f64,
    /// Half the column width at the corner.
    h: f64,
    /// Unit vector from the corner towards the outer corner point.
    u: Point,
    /// cos(turn / 2): the inner and outer corner points are `h / cos_half` from `v`.
    cos_half: f64,
    method: Method,
}

impl Join {
    fn new(prev: Point, v: Point, next: Point, h: f64, corners: Corners) -> Option<Join> {
        let t1 = (v - prev).normalized();
        let t2 = (next - v).normalized();
        if t1 == Point::ZERO || t2 == Point::ZERO {
            return None;
        }
        let turn = t1.cross(t2).atan2(t1.dot(t2));
        let angle = turn.abs();
        if !(CORNER..=REVERSAL).contains(&angle) {
            return None;
        }
        let sigma = turn.signum();
        let u = ((t1.perp() + t2.perp()) * -sigma).normalized();
        if u == Point::ZERO {
            return None;
        }
        let method = match corners {
            Corners::Auto if angle <= AUTO_TURN => Method::Turn,
            Corners::Auto if angle <= AUTO_MITER => Method::Miter,
            Corners::Miter if angle <= MITER_LIMIT => Method::Miter,
            Corners::Turn => Method::Turn,
            Corners::Auto | Corners::Miter | Corners::Cap => Method::Cap,
        };
        Some(Join { v, t1, t2, sigma, h, u, cos_half: (angle * 0.5).cos(), method })
    }

    /// Where the inner edges of the two legs meet.
    fn inner(&self) -> Point {
        self.v - self.u * (self.h / self.cos_half)
    }

    /// Where the outer edges of the two legs meet.
    fn outer(&self) -> Point {
        self.v + self.u * (self.h / self.cos_half)
    }

    /// The point the two legs' rows shrink to: the outer corner point of a miter,
    /// the middle of the cap of a capped corner.
    fn tip(&self) -> Point {
        match self.method {
            Method::Cap => self.v + self.u * self.h,
            _ => self.outer(),
        }
    }

    /// The point where the incoming leg ends and the outgoing one starts.
    fn meet(&self) -> Point {
        if self.method == Method::Turn { self.inner() } else { self.tip() }
    }

    /// How far past the corner the incoming leg runs (negative: it stops short).
    fn end_in(&self) -> f64 {
        (self.meet() - self.v).dot(self.t1)
    }

    /// Where the outgoing leg starts, measured along it from the corner (negative:
    /// before the corner).
    fn start_out(&self) -> f64 {
        (self.meet() - self.v).dot(self.t2)
    }

    /// Clips a rung of the incoming (or outgoing) leg to its side of the miter
    /// line, and to the cap.
    fn clip(&self, r: (Point, Point), incoming: bool) -> Option<(Point, Point)> {
        if self.method == Method::Turn {
            return Some(r);
        }
        // the miter line runs through the corner along `u`; `m` points across it
        // from the incoming leg's side to the outgoing leg's
        let mut m = self.u.perp();
        if m.dot(self.t1) < 0.0 {
            m = -m;
        }
        let r = clip_half(r, self.v, if incoming { m } else { -m })?;
        if self.method == Method::Cap { clip_half(r, self.v + self.u * self.h, self.u) } else { Some(r) }
    }

    /// For a turned corner: rungs fanning round the inner corner point from the
    /// incoming leg's last rung to the outgoing leg's first (both left out).
    fn fan(&self, step: f64) -> Vec<(Point, Point)> {
        if self.method != Method::Turn {
            return Vec::new();
        }
        let inner = self.inner();
        let first = self.v + self.t1 * self.end_in() - self.t1.perp() * (self.sigma * self.h);
        let last = self.v + self.t2 * self.start_out() - self.t2.perp() * (self.sigma * self.h);
        let path = [first, self.outer(), last];
        let len = polyline::length(&path);
        let k = (len / step).ceil() as usize;
        (1..k)
            .map(|j| {
                let o = polyline::point_at(&path, len * j as f64 / k as f64);
                if self.sigma > 0.0 { (inner, o) } else { (o, inner) }
            })
            .collect()
    }
}

/// The part of segment `a → b` on the near side of a line: where `(p − q)·m ≤ 0`.
fn clip_half((a, b): (Point, Point), q: Point, m: Point) -> Option<(Point, Point)> {
    const EPS: f64 = 1e-9;
    let (da, db) = ((a - q).dot(m), (b - q).dot(m));
    match (da <= EPS, db <= EPS) {
        (true, true) => Some((a, b)),
        (false, false) => None,
        _ => {
            let p = a.lerp(b, da / (da - db));
            Some(if da > EPS { (p, b) } else { (a, p) })
        }
    }
}

/// Pen pressure (0.08–1) at fraction `t` of the stroke.
fn pressure(samples: &[f32], t: f64) -> f64 {
    let (Some(&first), Some(_)) = (samples.first(), samples.last()) else { return 1.0 };
    let x = t.clamp(0.0, 1.0) * (samples.len() - 1) as f64;
    let k = (x.floor() as usize).min(samples.len() - 1);
    let a = f64::from(samples.get(k).copied().unwrap_or(first));
    let b = f64::from(samples.get(k + 1).or(samples.get(k)).copied().unwrap_or(first));
    (a + (b - a) * (x - k as f64)).clamp(0.08, 1.0)
}

/// Keeps both sides of the column moving forwards. Round a curve tighter than
/// the column is wide, the inner side would run backwards and the rows fold over
/// each other; there it waits on the spot while the outer side goes round.
fn unfold(rungs: &mut [(Point, Point)], centre: &[Point]) {
    for i in 1..rungs.len().min(centre.len()) {
        let t = (centre[i] - centre[i - 1]).normalized();
        let (pa, pb) = rungs[i - 1];
        let (a, b) = &mut rungs[i];
        if (*a - pa).dot(t) < 0.0 {
            *a = pa;
        }
        if (*b - pb).dot(t) < 0.0 {
            *b = pb;
        }
    }
}

/// Rungs along one leg of the line, from one corner (or end) to the next: the leg
/// is shortened or lengthened at each end for the corner there, and its rungs
/// clipped to its side of it. `g0` is where the leg starts on the whole line, for
/// the width profile.
fn leg(pts: &[Point], g0: f64, start: Option<Join>, end: Option<Join>, step: f64, half_at: &dyn Fn(f64) -> f64, out: &mut Vec<(Point, Point)>) {
    let len = polyline::length(pts);
    let s0 = start.map_or(0.0, |j| j.start_out());
    let e1 = end.map_or(0.0, |j| j.end_in());
    let (from, to) = (s0.max(0.0), len + e1.min(0.0));
    if to - from < 1e-6 {
        return;
    }
    let mut line = polyline::sub_polyline(pts, from, to);
    if let Some(j) = start
        && s0 < 0.0
    {
        line.insert(0, j.v + j.t2 * s0);
    }
    if let Some(j) = end
        && e1 > 0.0
    {
        line.push(j.v + j.t1 * e1);
    }
    let fine = polyline::resample(&polyline::dedup(&line, 1e-9), step, 0.6);
    let normals = polyline::vertex_normals(&fine, false);
    let arc = polyline::cumulative_lengths(&fine);
    let mut rungs: Vec<(Point, Point)> = fine
        .iter()
        .zip(&normals)
        .zip(&arc)
        .map(|((&p, &n), &l)| {
            let h = half_at(g0 + s0 + l);
            (p + n * h, p - n * h)
        })
        .collect();
    unfold(&mut rungs, &fine);
    for r in rungs {
        let r = match start {
            Some(j) => j.clip(r, false),
            None => Some(r),
        };
        let r = match (r, end) {
            (Some(r), Some(j)) => j.clip(r, true),
            (r, _) => r,
        };
        if let Some(r) = r
            && r.0.distance(r.1) > MIN_RUNG
        {
            out.push(r);
        }
    }
}

/// A satin column `width` mm wide along `centerline` (when `closed`, round and back
/// to the start), shaped by `profile`, with rungs about every `step` mm and sharp
/// corners joined by `corners`. `None` when the line is too short.
pub fn satin_line(centerline: &[Point], closed: bool, width: f64, profile: &WidthProfile, step: f64, corners: Corners) -> Option<Column> {
    let step = step.max(0.2);
    let mut pts = polyline::dedup(centerline, 1e-6);
    if closed {
        pts = polyline::dedup_ring(&pts, 1e-6);
    }
    let closed = closed && pts.len() >= 3;
    if pts.len() < 2 {
        return None;
    }
    let path: Vec<Point> = if closed { pts.iter().chain(pts.first()).copied().collect() } else { pts.clone() };
    let cum = polyline::cumulative_lengths(&path);
    let total = cum.last().copied().unwrap_or(0.0);
    if total < 1e-6 {
        return None;
    }
    let half = width.max(0.1) * 0.5;
    let taper = (total * 0.3).min(width.max(0.5) * 2.5).max(1e-6);
    let half_at = |s: f64| -> f64 {
        let s = if closed { s.rem_euclid(total) } else { s.clamp(0.0, total) };
        match profile {
            WidthProfile::Constant | WidthProfile::Calligraphy { .. } => half,
            WidthProfile::Tapered => {
                let t = (s.min(total - s) / taper).clamp(0.0, 1.0);
                // smoothstep keeps the tips pointed but the shoulders round
                half * (t * t * (3.0 - 2.0 * t)).max(0.04)
            }
            WidthProfile::Pressure(samples) => half * pressure(samples, s / total),
        }
    };
    let to_column = |rungs: Vec<(Point, Point)>| {
        let rungs: Vec<Rung> = rungs.into_iter().map(|(a, b)| Rung::new(a, b, NodeKind::Corner)).collect();
        (rungs.len() >= 2).then(|| Column::new(rungs))
    };
    if let WidthProfile::Calligraphy { nib_angle } = profile {
        // the nib sets the stitch direction everywhere: nothing turns at corners
        let dir = screen_dir(*nib_angle);
        return to_column(polyline::resample(&path, step, 0.6).into_iter().map(|p| (p + dir * half, p - dir * half)).collect());
    }

    let n = pts.len();
    let joins: Vec<Option<Join>> = (0..n)
        .map(|i| {
            let (prev, next) = match (closed, i) {
                (true, _) => (pts[(i + n - 1) % n], pts[(i + 1) % n]),
                (false, 0) => return None,
                (false, _) if i + 1 >= n => return None,
                (false, _) => (pts[i - 1], pts[i + 1]),
            };
            Join::new(prev, pts[i], next, half_at(cum[i]), corners)
        })
        .collect();
    let corners_at: Vec<usize> = (0..n).filter(|&i| joins[i].is_some()).collect();

    let mut rungs: Vec<(Point, Point)> = Vec::new();
    match corners_at.first() {
        None if closed => {
            // a smooth loop: one leg all the way round, turning smoothly at the seam
            let ring = polyline::resample_ring(&pts, step, 0.6);
            let open = &ring[..ring.len().saturating_sub(1)];
            let normals = polyline::vertex_normals(open, true);
            let arc = polyline::cumulative_lengths(&ring);
            rungs = open
                .iter()
                .zip(&normals)
                .zip(&arc)
                .map(|((&p, &nv), &s)| {
                    let h = half_at(s);
                    (p + nv * h, p - nv * h)
                })
                .collect();
            unfold(&mut rungs, open);
            if let Some(&first) = rungs.first() {
                rungs.push(first);
            }
        }
        _ => {
            // legs from corner to corner (and from the ends of an open line)
            let bounds: Vec<usize> = if closed {
                corners_at.iter().copied().chain(corners_at.first().map(|&c| c + n)).collect()
            } else {
                std::iter::once(0).chain(corners_at.iter().copied()).chain(std::iter::once(n - 1)).collect()
            };
            for w in bounds.windows(2) {
                let (i0, i1) = (w[0], w[1]);
                let (start, end) = (joins[i0 % n], joins[i1 % n]);
                let leg_pts: Vec<Point> = (i0..=i1).map(|i| pts[i % n]).collect();
                leg(&leg_pts, cum[i0 % n], start, end, step, &half_at, &mut rungs);
                if let Some(j) = end {
                    rungs.extend(j.fan(step));
                }
            }
            if closed && let Some(&first) = rungs.first() {
                rungs.push(first);
            }
        }
    }
    to_column(rungs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    /// The densest spot: most rung lengths of thread (each `0.4` wide) per 1 mm cell,
    /// sampling every rung at `spacing` along the column like the satin stitches do.
    fn crowding(col: &Column) -> usize {
        let mut cells = std::collections::HashMap::new();
        for r in &col.rungs {
            let n = (r.width() / 0.1).ceil().max(1.0) as usize;
            for k in 0..=n {
                let q = r.a.lerp(r.b, k as f64 / n as f64);
                *cells.entry(((q.x * 2.0).floor() as i64, (q.y * 2.0).floor() as i64)).or_insert(0usize) += 1;
            }
        }
        cells.values().copied().max().unwrap_or(0)
    }

    fn l_shape() -> Vec<Point> {
        vec![p(0.0, 0.0), p(30.0, 0.0), p(30.0, 30.0)]
    }

    #[test]
    fn straight_lines_are_plain_columns() {
        let col = satin_line(&[p(0.0, 0.0), p(20.0, 0.0)], false, 4.0, &WidthProfile::Constant, 1.0, Corners::Auto).expect("column");
        assert_eq!(col.rungs.len(), 21);
        for r in &col.rungs {
            assert!((r.width() - 4.0).abs() < 1e-9 && (r.a.x - r.b.x).abs() < 1e-9, "{r:?}");
        }
    }

    #[test]
    fn mitered_corners_meet_on_the_miter_line() {
        let col = satin_line(&l_shape(), false, 4.0, &WidthProfile::Constant, 0.5, Corners::Miter).expect("column");
        // rows stay square to each leg: vertical on the first, horizontal on the second
        for r in &col.rungs {
            let d = r.b - r.a;
            assert!(d.x.abs() < 1e-6 || d.y.abs() < 1e-6, "{r:?}");
        }
        // and end on the miter line from the inner (28, 2) to the outer corner (32, -2)
        let on_miter = |q: Point| (q.x + q.y - 30.0).abs() < 1e-6;
        let ends = col.rungs.iter().filter(|r| on_miter(r.a) || on_miter(r.b)).count();
        assert!(ends >= 12, "{ends}");
        // every point of the column outline is inside the two legs' strips
        for r in &col.rungs {
            for q in [r.a, r.b] {
                assert!((q.y.abs() <= 2.0 + 1e-6 && q.x <= 32.0 + 1e-6) || ((q.x - 30.0).abs() <= 2.0 + 1e-6 && q.y >= -2.0 - 1e-6), "{q:?}");
            }
        }
    }

    #[test]
    fn corners_do_not_pile_up() {
        let turned = satin_line(&l_shape(), false, 4.0, &WidthProfile::Constant, 0.2, Corners::Turn).expect("column");
        let straight = satin_line(&[p(0.0, 0.0), p(60.0, 0.0)], false, 4.0, &WidthProfile::Constant, 0.2, Corners::Auto).expect("column");
        let base = crowding(&straight);
        assert!(crowding(&turned) > base * 3, "a turned right angle fans into its inner corner");
        for c in [Corners::Auto, Corners::Miter, Corners::Cap] {
            let col = satin_line(&l_shape(), false, 4.0, &WidthProfile::Constant, 0.2, c).expect("column");
            assert!(crowding(&col) <= base * 2, "{c:?}: {} vs {base}", crowding(&col));
        }
    }

    #[test]
    fn sharp_corners_are_capped() {
        // a spike: the line turns back 160°
        let spike = vec![p(0.0, 0.0), p(30.0, 0.0), p(30.0 - 30.0 * (20.0 * DEG).cos(), 30.0 * (20.0 * DEG).sin())];
        let col = satin_line(&spike, false, 4.0, &WidthProfile::Constant, 0.5, Corners::Auto).expect("column");
        // the tip is cut off 2 mm (half the width) past the corner instead of running ~11 mm out
        let reach = col.rungs.iter().flat_map(|r| [r.a, r.b]).map(|q| q.x).fold(f64::NEG_INFINITY, f64::max);
        assert!(reach < 32.1, "{reach}");
        let miter = satin_line(&spike, false, 4.0, &WidthProfile::Constant, 0.5, Corners::Miter).expect("column");
        let reach = miter.rungs.iter().flat_map(|r| [r.a, r.b]).map(|q| q.x).fold(f64::NEG_INFINITY, f64::max);
        assert!(reach < 32.1, "a forced miter this sharp is capped too: {reach}");
    }

    #[test]
    fn closed_lines_loop_back_to_the_start() {
        let square = vec![p(0.0, 0.0), p(20.0, 0.0), p(20.0, 20.0), p(0.0, 20.0)];
        let col = satin_line(&square, true, 3.0, &WidthProfile::Constant, 0.5, Corners::Auto).expect("column");
        let (first, last) = (col.rungs[0], col.rungs[col.rungs.len() - 1]);
        assert!(first.a.approx_eq(last.a, 1e-9) && first.b.approx_eq(last.b, 1e-9));
        // all four corners mitered: rows are horizontal or vertical everywhere
        assert!(col.rungs.iter().all(|r| (r.b - r.a).x.abs() < 1e-6 || (r.b - r.a).y.abs() < 1e-6));
        // a circle has no corners: one smooth loop, every rung full width
        let circle: Vec<Point> = (0..48).map(|i| Point::from_angle(i as f64 * 7.5 * DEG) * 10.0).collect();
        let ring = satin_line(&circle, true, 3.0, &WidthProfile::Constant, 0.5, Corners::Auto).expect("column");
        assert!(ring.rungs.iter().all(|r| (r.width() - 3.0).abs() < 1e-6));
        assert!(ring.rungs[0].a.approx_eq(ring.rungs[ring.rungs.len() - 1].a, 1e-9));
    }

    #[test]
    fn tight_curves_do_not_fold_back() {
        // a U turn of radius 1 mm with a 6 mm column: the inner side must not run backwards
        let mut line = vec![p(-10.0, 1.0)];
        line.extend((0..=12).map(|i| Point::from_angle((90.0 - i as f64 * 15.0) * DEG)));
        line.push(p(-10.0, -1.0));
        let col = satin_line(&line, false, 6.0, &WidthProfile::Constant, 0.3, Corners::Turn).expect("column");
        let rail: Vec<Point> = col.rungs.iter().map(|r| r.b).collect();
        let len = polyline::length(&rail);
        // the inner rail is no longer than its straight parts plus a little: it waits round the bend
        assert!(len < 26.0, "{len}");
    }
}
