//! Flow fills: tatami rows that follow drawn lines instead of running straight. With one line
//! every row is a copy of it, moved across the shape (a Florentine effect); with two, the rows
//! change shape evenly from the first line to the second (a liquid effect).
//!
//! The lines make a frame: stations along them, each pairing a point on the first line with one
//! on the second, `u` measured along the rows and `v` across them from the first line. The
//! shape is taken into the frame, where its rows are straight, filled there like any tatami,
//! and the stitches brought back.

use super::fill::{self, ScanConfig};
use super::out::{Out, mapped};
use crate::geom::{BBox, Point, Polygon, polyline};

/// Most points kept of each drawn line.
const MAX_LINE_POINTS: usize = 10_000;
/// Stations along the lines, and along each of their straight extensions.
const STATIONS: usize = 128;
const EXTENSION: usize = 8;
/// Most points of a shape's outline taken into the frame.
const MAX_OUTLINE_POINTS: f64 = 20_000.0;
/// Most a frame may stretch a shape, either way, and farthest out in it the shape may lie (mm):
/// lines thousands of kilometres long make frames too big to fill in.
const MAX_STRETCH: f64 = 20.0;
const MAX_FRAME: f64 = 1e6;

/// The frame one or two lines make (mm).
struct Frame {
    /// Stations: a point on the first line, one on the second, and how far along the rows the
    /// station is (the line halfway between them, measured from the first station).
    a: Vec<Point>,
    b: Vec<Point>,
    u: Vec<f64>,
    /// mm across for a whole step from the first line to the second: the widest gap between
    /// them, so rows are never further apart than asked.
    gap: f64,
}

/// A drawn line fit to build a frame on: finite, without repeated points, thinned to at most
/// [`MAX_LINE_POINTS`].
fn clean(line: &[Point]) -> Vec<Point> {
    let pts: Vec<Point> = line.iter().copied().filter(|p| p.is_finite()).collect();
    let pts = polyline::dedup(&pts, 1e-6);
    if pts.len() <= MAX_LINE_POINTS {
        return pts;
    }
    let k = pts.len().div_ceil(MAX_LINE_POINTS);
    let mut thin: Vec<Point> = pts.iter().step_by(k).copied().collect();
    if let Some(&last) = pts.last()
        && thin.last() != Some(&last)
    {
        thin.push(last);
    }
    thin
}

/// `n + 1` points evenly along a line by length.
fn stations(line: &[Point], n: usize) -> Vec<Point> {
    let total = polyline::length(line);
    (0..=n).map(|i| polyline::point_at(line, total * i as f64 / n as f64)).collect()
}

/// The unit direction from `from` to `to`, or `or` when they meet.
fn unit(from: Point, to: Point, or: Point) -> Point {
    let d = to - from;
    if d.length() > 1e-12 { d * (1.0 / d.length()) } else { or }
}

/// The real roots of `a s² + b s + c = 0` (one when it is linear).
fn roots(a: f64, b: f64, c: f64) -> [Option<f64>; 2] {
    let scale = a.abs().max(b.abs()).max(c.abs());
    if !(scale.is_finite() && scale > 0.0) {
        return [None, None];
    }
    let (a, b, c) = (a / scale, b / scale, c / scale);
    if a.abs() < 1e-12 {
        return [(b.abs() > 1e-15).then(|| -c / b), None];
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return [None, None];
    }
    let q = -0.5 * (b + b.signum() * disc.sqrt());
    [Some(q / a), (q.abs() > 1e-300).then(|| c / q)]
}

impl Frame {
    /// The frame of `lines` (the first two long enough to use) over a shape inside `area`.
    fn new(lines: &[Vec<Point>], area: &BBox) -> Option<Frame> {
        let mut lines = lines.iter().map(|l| clean(l)).filter(|l| l.len() >= 2 && polyline::length(l) > 0.5);
        let first = lines.next()?;
        let reach = area.width().hypot(area.height());
        if !(reach.is_finite() && reach > 1e-6) {
            return None;
        }
        let a = stations(&first, STATIONS);
        let (&a0, &a1) = (a.first()?, a.last()?);
        let b = match lines.next() {
            Some(second) => {
                let mut b = stations(&second, STATIONS);
                // the second line taken the same way as the first
                let (&b0, &b1) = (b.first()?, b.last()?);
                if a0.distance(b0) + a1.distance(b1) > a0.distance(b1) + a1.distance(b0) {
                    b.reverse();
                }
                b
            }
            // one line: rows copying it, the second line a copy of it beside it
            None => {
                let n = unit(a0, a1, Point::new(1.0, 0.0)).perp();
                a.iter().map(|&p| p + n * reach).collect()
            }
        };
        let gap = a.iter().zip(&b).map(|(p, q)| p.distance(*q)).fold(0.0, f64::max);
        if !(gap.is_finite() && gap > 1e-3) {
            return None;
        }
        // Both ends carried on straight, the same way for both lines, past the shape.
        let (Some(&p2), Some(&q2), Some(&pn), Some(&qn)) = (a.get(2), b.get(2), a.get(STATIONS - 2), b.get(STATIONS - 2)) else {
            return None;
        };
        let (&b0, &b1) = (b.first()?, b.last()?);
        let back = unit(Point::ZERO, unit(p2, a0, Point::ZERO) + unit(q2, b0, Point::ZERO), unit(p2, a0, Point::new(-1.0, 0.0)));
        let ahead = unit(Point::ZERO, unit(pn, a1, Point::ZERO) + unit(qn, b1, Point::ZERO), unit(pn, a1, Point::new(1.0, 0.0)));
        let c = area.center();
        let ext0 = a0.distance(c) + b0.distance(c) + reach;
        let ext1 = a1.distance(c) + b1.distance(c) + reach;
        let before = |p: Point| (0..EXTENSION).map(move |j| p + back * (ext0 * (EXTENSION - j) as f64 / EXTENSION as f64));
        let after = |p: Point| (1..=EXTENSION).map(move |j| p + ahead * (ext1 * j as f64 / EXTENSION as f64));
        let a: Vec<Point> = before(a0).chain(a.iter().copied()).chain(after(a1)).collect();
        let b: Vec<Point> = before(b0).chain(b.iter().copied()).chain(after(b1)).collect();
        // `u` along the line halfway between, always moving on
        let mut u = Vec::with_capacity(a.len());
        let mut s = 0.0;
        let mut last: Option<Point> = None;
        for (p, q) in a.iter().zip(&b) {
            let m = p.midpoint(*q);
            if let Some(l) = last {
                s += l.distance(m).max(1e-6);
            }
            u.push(s);
            last = Some(m);
        }
        u.iter().all(|x| x.is_finite()).then_some(Frame { a, b, u, gap })
    }

    /// The point at `q = (u, v)` in the frame.
    fn at(&self, q: Point) -> Point {
        let n = self.u.len();
        let i = self.u.partition_point(|&x| x <= q.x).clamp(1, n.max(2) - 1) - 1;
        let (Some(&u0), Some(&u1), Some(&a0), Some(&a1), Some(&b0), Some(&b1)) =
            (self.u.get(i), self.u.get(i + 1), self.a.get(i), self.a.get(i + 1), self.b.get(i), self.b.get(i + 1))
        else {
            return q;
        };
        let s = (q.x - u0) / (u1 - u0);
        a0.lerp(a1, s).lerp(b0.lerp(b1, s), q.y / self.gap)
    }

    /// Where `p` sits in the frame, `(u, v)`: none when it lies beyond the lines' ends. Where
    /// rows carried on past the lines cross, the place nearest between the lines wins.
    fn place(&self, p: Point) -> Option<Point> {
        let mut best: Option<(f64, Point)> = None;
        for i in 1..self.u.len() {
            let (Some(&u0), Some(&u1), Some(&a0), Some(&a1), Some(&b0), Some(&b1)) =
                (self.u.get(i - 1), self.u.get(i), self.a.get(i - 1), self.a.get(i), self.b.get(i - 1), self.b.get(i))
            else {
                continue;
            };
            // p = A(s) + t·D(s), A and D straight from station to station: across D, p - A(s)
            // is nothing, a quadratic in s
            let (da, d0) = (a1 - a0, b0 - a0);
            let dd = (b1 - a1) - d0;
            let w = p - a0;
            for s in roots(-da.cross(dd), w.cross(dd) - da.cross(d0), w.cross(d0)).into_iter().flatten() {
                if !(-1e-9..=1.0 + 1e-9).contains(&s) {
                    continue;
                }
                let d = d0 + dd * s;
                let l2 = d.length_sq();
                if l2 < 1e-18 {
                    continue;
                }
                let t = (w - da * s).dot(d) / l2;
                let score = (t - 0.5).abs();
                if t.is_finite() && best.is_none_or(|(b, _)| score < b) {
                    best = Some((score, Point::new(u0 + s * (u1 - u0), t * self.gap)));
                }
            }
        }
        best.map(|(_, q)| q)
    }

    /// The shape in the frame, its outline followed closely enough to bend with it; none when
    /// part of it lies beyond the lines' ends.
    fn place_polygon(&self, piece: &Polygon) -> Option<Polygon> {
        let perimeter: f64 = piece.rings().map(|r| polyline::ring_length(r)).sum();
        let seg = (perimeter / MAX_OUTLINE_POINTS).max(0.3);
        let ring = |r: &Vec<Point>| -> Option<Vec<Point>> {
            let mut out = Vec::new();
            for (&a, &b) in r.iter().zip(r.iter().cycle().skip(1)) {
                let parts = ((a.distance(b) / seg).ceil() as usize).clamp(1, 10_000);
                for j in 0..parts {
                    out.push(self.place(a.lerp(b, j as f64 / parts as f64))?);
                }
            }
            Some(out)
        };
        let outer = ring(&piece.outer)?;
        let holes = piece.holes.iter().map(ring).collect::<Option<Vec<_>>>()?;
        Some(Polygon::new(outer, holes))
    }
}

/// Fills `piece` (mm) with tatami rows along `lines` (one or two): false, sewing nothing, when
/// the lines make no frame for it (the caller then sews straight rows).
pub(crate) fn flow_fill(
    piece: &Polygon,
    lines: &[Vec<Point>],
    cfg: &ScanConfig,
    out: &mut Out,
    hint: Option<Point>,
    exit: Option<Point>,
    trim: f64,
) -> bool {
    let Some(frame) = Frame::new(lines, &piece.bbox()) else { return false };
    let Some(placed) = frame.place_polygon(piece) else { return false };
    // a frame that stretches the shape out of all proportion (lines coming together, or a
    // tangle) sews straight rows instead
    let (real, flat) = (piece.bbox(), placed.bbox());
    let size = real.width().hypot(real.height());
    let far = [flat.min.x, flat.min.y, flat.max.x, flat.max.y].iter().fold(0.0f64, |m, v| m.max(v.abs()));
    if !(flat.width() <= size * MAX_STRETCH && flat.height() <= size * MAX_STRETCH && far < MAX_FRAME) {
        return false;
    }
    // straight rows in the frame; split lines taken there too
    let splits = cfg.splits.iter().map(|l| l.iter().filter_map(|p| frame.place(*p)).collect::<Vec<Point>>()).filter(|l| l.len() >= 2).collect();
    let cfg = ScanConfig { angle: 0.0, splits, ..cfg.clone() };
    let step = cfg.stitch_length.max(0.5);
    let exit = exit.and_then(|p| frame.place(p));
    let mut parts = fill::prepare(placed);
    while !parts.is_empty() {
        let here = out.pos().or(hint).and_then(|p| frame.place(p));
        let i = match here {
            Some(h) => (0..parts.len()).min_by(|&a, &b| parts[a].distance_to_boundary(h).total_cmp(&parts[b].distance_to_boundary(h))).unwrap_or(0),
            None => 0,
        };
        let p = parts.remove(i);
        let ex = if parts.is_empty() { exit } else { None };
        let pts = mapped(&fill::scan_fill(&p, &cfg, here, ex), step, |q| frame.at(q));
        if let Some(&first) = pts.first() {
            out.connect(first, trim, cfg.travel_length);
            out.stitches(pts);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: f64, h: f64) -> Polygon {
        Polygon::from_ring(vec![Point::new(0.0, 0.0), Point::new(w, 0.0), Point::new(w, h), Point::new(0.0, h)])
    }

    fn sew(piece: &Polygon, lines: &[Vec<Point>], cfg: &ScanConfig) -> Option<Vec<Point>> {
        let mut out = Out::new();
        flow_fill(piece, lines, cfg, &mut out, None, None, 1e9).then(|| out.finish().iter().map(|s| s.pos()).collect())
    }

    fn wave(y: f64, amp: f64) -> Vec<Point> {
        (0..=80).map(|i| f64::from(i) * 0.5).map(|x| Point::new(x, y + amp * (x * 0.2).sin())).collect()
    }

    #[test]
    fn the_frame_takes_points_there_and_back() {
        let area = rect(40.0, 30.0).bbox();
        let frame = Frame::new(&[wave(-5.0, 3.0), wave(35.0, 6.0)], &area).unwrap();
        for p in [Point::new(3.0, 4.0), Point::new(20.0, 15.0), Point::new(39.0, 29.0), Point::new(0.0, 0.0)] {
            let q = frame.place(p).unwrap();
            assert!(frame.at(q).distance(p) < 1e-6, "{p:?} → {q:?} → {:?}", frame.at(q));
        }
        // straight rows in the frame run along the waves: the first line is v = 0
        let q = frame.place(Point::new(10.0, -5.0 + 3.0 * 2.0f64.sin())).unwrap();
        assert!(q.y.abs() < 0.01, "{q:?}");
    }

    #[test]
    fn rows_follow_two_lines_and_change_shape_between_them() {
        let piece = rect(40.0, 30.0);
        let cfg = ScanConfig { row_spacing: 1.0, stitch_length: 2.5, ..ScanConfig::default() };
        // flat lines above and below: rows run straight across, as a plain fill at 0°
        let flat =
            sew(&piece, &[vec![Point::new(-5.0, -2.0), Point::new(45.0, -2.0)], vec![Point::new(-5.0, 32.0), Point::new(45.0, 32.0)]], &cfg).unwrap();
        assert!(flat.len() > 300);
        assert!(flat.windows(2).filter(|w| w[0].distance(w[1]) > 1.2).all(|w| (w[0].y - w[1].y).abs() < 0.05));
        // waves: the stitches lean the way the waves do, flat at their crests
        let wavy = sew(&piece, &[wave(-4.0, 3.0), wave(34.0, 3.0)], &cfg).unwrap();
        let slope = |x0: f64| {
            let near: Vec<f64> = wavy
                .windows(2)
                .filter(|w| (w[0].x - x0).abs() < 1.0 && (w[1].x - x0).abs() < 1.5 && w[0].distance(w[1]) > 1.0)
                .map(|w| (w[1].y - w[0].y) / (w[1].x - w[0].x))
                .collect();
            near.iter().sum::<f64>() / near.len().max(1) as f64
        };
        // y = 3 sin(0.2 x): slope 0.6 cos(0.2 x), 0.6 at x = 0 + 10π…, 0 at the crest x = 7.85
        assert!(slope(7.85).abs() < 0.15 && (slope(15.7) + 0.6).abs() < 0.2, "{} {}", slope(7.85), slope(15.7));
        // every stitch inside, none too long
        for pts in [&flat, &wavy] {
            assert!(pts.iter().all(|p| piece.contains(*p) || piece.distance_to_boundary(*p) < 0.3));
            let longest = pts.windows(2).map(|w| w[0].distance(w[1])).fold(0.0, f64::max);
            assert!(longest <= 2.5 * 1.05 + 1e-6, "{longest}");
        }
    }

    #[test]
    fn one_line_makes_rows_that_copy_it() {
        let piece = rect(40.0, 30.0);
        let cfg = ScanConfig { row_spacing: 1.5, stitch_length: 2.0, ..ScanConfig::default() };
        // a whole wave, its ends level: the copies move straight up and down
        let f = |x: f64| 15.0 + 4.0 * (x * std::f64::consts::PI / 20.0).sin();
        let line: Vec<Point> = (0..=80).map(|i| f64::from(i) * 0.5).map(|x| Point::new(x, f(x))).collect();
        let pts = sew(&piece, &[line], &cfg).unwrap();
        assert!(pts.len() > 300);
        // the needle points sit on copies of the wave a whole row apart
        let rows: Vec<f64> = pts.iter().map(|p| ((p.y - f(p.x)) / 1.5).rem_euclid(1.0)).collect();
        let m = rows[rows.len() / 2];
        let on = rows
            .iter()
            .filter(|r| {
                let d = (*r - m).rem_euclid(1.0);
                d.min(1.0 - d) < 0.05
            })
            .count();
        assert!(on as f64 > pts.len() as f64 * 0.8, "{on} of {}", pts.len());
    }

    #[test]
    fn lines_that_make_no_frame_leave_the_fill_straight() {
        let piece = rect(40.0, 30.0);
        let cfg = ScanConfig::default();
        let p = |x: f64, y: f64| Point::new(x, y);
        for lines in [
            vec![],
            vec![vec![p(0.0, 0.0)]],
            vec![vec![p(0.0, 0.0), p(0.1, 0.0)]],
            vec![vec![p(f64::NAN, 0.0), p(f64::INFINITY, 1.0)]],
            vec![vec![p(0.0, 5.0), p(40.0, 5.0)], vec![p(0.0, 5.0), p(40.0, 5.0)]],
        ] {
            assert!(sew(&piece, &lines, &cfg).is_none(), "{lines:?}");
        }
        // crossing lines, huge ones, a tangle: whatever comes out is finite and bounded
        let tangle: Vec<Point> = (0..20_000).map(|i| p((f64::from(i) * 7.3).sin() * 50.0, (f64::from(i) * 3.1).cos() * 50.0)).collect();
        for lines in [
            vec![vec![p(0.0, 0.0), p(40.0, 30.0)], vec![p(0.0, 30.0), p(40.0, 0.0)]],
            vec![vec![p(-1e300, 0.0), p(1e300, 1.0)]],
            vec![tangle.clone(), tangle],
            vec![vec![p(20.0, 15.0), p(20.0, 15.6)], vec![p(20.0, 14.0), p(20.0, 14.6)]],
        ] {
            if let Some(pts) = sew(&piece, &lines, &cfg) {
                assert!(pts.len() < 2_000_000 && pts.iter().all(|q| q.is_finite()));
            }
        }
    }
}
