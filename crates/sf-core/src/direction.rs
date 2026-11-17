//! Stitch direction: inclination lines ("guides"), turning strips and automatic
//! direction.
//!
//! A [`Guide`] is a line drawn across a shape; stitches run parallel to it. One
//! guide simply sets a fill's angle. Two or more guides turn the shape into a
//! [`Strip`] – a column whose sides are the outline between the guides – so the
//! stitch direction can turn smoothly along the shape, the way digitizers bend
//! satin and fill stitches around curves.
//!
//! [`auto_guides`] places guides automatically for column-like shapes, and
//! [`auto_fill_angle`] picks the straight-fill angle that sews with the fewest
//! breaks. Both produce ordinary editable data: the user can drag, add or delete
//! guides afterwards.

use crate::geom::{Affine, Point, Polygon, polyline, screen_angle};
use crate::model::normalize_line_angle;
use serde::{Deserialize, Serialize};

/// An inclination line: stitches run parallel to it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub a: Point,
    pub b: Point,
}

impl Guide {
    pub fn new(a: Point, b: Point) -> Self {
        Guide { a, b }
    }

    /// Stitch angle in degrees counter-clockwise on screen, in `[0, 180)`.
    pub fn angle(&self) -> f64 {
        normalize_line_angle(screen_angle(self.b - self.a))
    }

    pub fn midpoint(&self) -> Point {
        self.a.midpoint(self.b)
    }

    pub fn length(&self) -> f64 {
        self.a.distance(self.b)
    }

    pub fn transform(&mut self, m: &Affine) {
        self.a = m.apply(self.a);
        self.b = m.apply(self.b);
    }
}

/// A shape seen as a turning column.
#[derive(Clone, Debug, PartialEq)]
pub struct Strip {
    /// Chords across the shape (side A point, side B point), in order along the strip.
    pub stations: Vec<(Point, Point)>,
    /// Outline between consecutive stations on side A (both ends included).
    pub rails_a: Vec<Vec<Point>>,
    /// Outline between consecutive stations on side B (both ends included).
    pub rails_b: Vec<Vec<Point>>,
    /// Outline around the start end, from the first station's A point to its B point.
    pub cap_start: Vec<Point>,
    /// Outline around the far end, from the last station's A point to its B point.
    pub cap_end: Vec<Point>,
}

/// Arc-length helper for a closed ring.
struct Ring<'a> {
    pts: &'a [Point],
    cum: Vec<f64>,
    total: f64,
}

impl<'a> Ring<'a> {
    fn new(pts: &'a [Point]) -> Self {
        let mut cum = Vec::with_capacity(pts.len() + 1);
        let mut acc = 0.0;
        for i in 0..pts.len() {
            cum.push(acc);
            acc += pts[i].distance(pts[(i + 1) % pts.len()]);
        }
        cum.push(acc);
        Ring { pts, cum, total: acc }
    }

    fn at(&self, pos: f64) -> Point {
        let n = self.pts.len();
        if n == 0 || self.total <= 0.0 {
            return self.pts.first().copied().unwrap_or(Point::ZERO);
        }
        let d = pos.rem_euclid(self.total);
        let i = self.cum.partition_point(|&c| c <= d).clamp(1, n) - 1;
        let seg = self.cum[i + 1] - self.cum[i];
        let a = self.pts[i];
        let b = self.pts[(i + 1) % n];
        if seg < 1e-12 { a } else { a.lerp(b, (d - self.cum[i]) / seg) }
    }

    /// Forward (increasing position) distance from `a` to `b`.
    fn fwd(&self, a: f64, b: f64) -> f64 {
        (b - a).rem_euclid(self.total)
    }

    /// The outline from `from` to `to`, walking forwards or backwards.
    fn chain(&self, from: f64, to: f64, forward: bool) -> Vec<Point> {
        let n = self.pts.len();
        let mut out = vec![self.at(from)];
        if forward {
            let len = self.fwd(from, to);
            // vertices strictly between, in forward order
            let mut verts: Vec<(f64, Point)> =
                (0..n).map(|i| (self.fwd(from, self.cum[i]), self.pts[i])).filter(|(d, _)| *d > 1e-9 && *d < len - 1e-9).collect();
            verts.sort_by(|a, b| a.0.total_cmp(&b.0));
            out.extend(verts.into_iter().map(|v| v.1));
        } else {
            let len = self.fwd(to, from);
            let mut verts: Vec<(f64, Point)> =
                (0..n).map(|i| (self.fwd(self.cum[i], from), self.pts[i])).filter(|(d, _)| *d > 1e-9 && *d < len - 1e-9).collect();
            verts.sort_by(|a, b| a.0.total_cmp(&b.0));
            out.extend(verts.into_iter().map(|v| v.1));
        }
        out.push(self.at(to));
        polyline::dedup(&out, 1e-9)
    }

    /// Intersections of the infinite line `p + t·d` with the ring: `(t, ring position, point)`,
    /// sorted by `t`.
    fn line_hits(&self, p: Point, d: Point) -> Vec<(f64, f64, Point)> {
        let n = self.pts.len();
        // Nudge the line off exact vertices so every crossing is counted once.
        let p = p + d.perp().normalized() * 1e-7;
        let mut hits = Vec::new();
        for i in 0..n {
            let a = self.pts[i];
            let b = self.pts[(i + 1) % n];
            let e = b - a;
            let denom = d.cross(e);
            if denom.abs() < 1e-14 {
                continue;
            }
            let ap = a - p;
            let t = ap.cross(e) / denom;
            let u = ap.cross(d) / denom;
            if (0.0..1.0).contains(&u) {
                let q = a + e * u;
                hits.push((t, self.cum[i] + u * e.length(), q));
            }
        }
        hits.sort_by(|x, y| x.0.total_cmp(&y.0));
        hits
    }
}

/// The chord of `ring` that a guide marks: the inside interval of the guide's line
/// that contains (or is nearest to) the guide's midpoint. Returns two ring positions.
fn guide_chord(ring: &Ring, g: &Guide) -> Option<(f64, f64)> {
    let d = g.b - g.a;
    if d.length() < 1e-9 {
        return None;
    }
    let hits = ring.line_hits(g.a, d);
    if hits.len() < 2 {
        return None;
    }
    // the guide midpoint is at t = 0.5
    let mut best: Option<(f64, (f64, f64))> = None;
    for pair in hits.as_chunks::<2>().0.iter() {
        let (t0, p0, _) = pair[0];
        let (t1, p1, _) = pair[1];
        let dist = if (t0..=t1).contains(&0.5) { 0.0 } else { (t0 - 0.5).abs().min((t1 - 0.5).abs()) };
        if best.is_none_or(|b| dist < b.0) {
            best = Some((dist, (p0, p1)));
        }
    }
    best.map(|b| b.1)
}

/// Builds a strip from a closed outline (ring) and two or more guides, given in
/// order along the shape. Returns `None` when the guides don't cut the outline
/// into a clean column (crossing guides, guides outside the shape, ...).
pub fn strip_from_guides(ring: &[Point], guides: &[Guide]) -> Option<Strip> {
    if guides.len() < 2 || ring.len() < 3 {
        return None;
    }
    let r = Ring::new(ring);
    if r.total <= 1e-9 {
        return None;
    }
    let mut chords: Vec<(f64, f64)> = Vec::with_capacity(guides.len());
    for g in guides {
        chords.push(guide_chord(&r, g)?);
    }
    // Orient consecutive chords consistently (A sides together).
    for i in 1..chords.len() {
        let (pa, pb) = (r.at(chords[i - 1].0), r.at(chords[i - 1].1));
        let (ca, cb) = (r.at(chords[i].0), r.at(chords[i].1));
        if pa.distance(cb) + pb.distance(ca) < pa.distance(ca) + pb.distance(cb) {
            chords[i] = (chords[i].1, chords[i].0);
        }
    }
    let n = chords.len();
    // Walking from A1 in one direction we must meet A2..An, then Bn..B1.
    for forward in [true, false] {
        let dist = |p: f64| if forward { r.fwd(chords[0].0, p) } else { r.fwd(p, chords[0].0) };
        let mut seq: Vec<(f64, usize, bool)> = Vec::with_capacity(2 * n);
        for (i, c) in chords.iter().enumerate() {
            seq.push((dist(c.0), i, true));
            seq.push((dist(c.1), i, false));
        }
        seq.sort_by(|a, b| a.0.total_cmp(&b.0));
        let expected: Vec<(usize, bool)> = (0..n).map(|i| (i, true)).chain((0..n).rev().map(|i| (i, false))).collect();
        let got: Vec<(usize, bool)> = seq.iter().map(|s| (s.1, s.2)).collect();
        if got != expected {
            continue;
        }
        let stations: Vec<(Point, Point)> = chords.iter().map(|c| (r.at(c.0), r.at(c.1))).collect();
        let rails_a = (0..n - 1).map(|i| r.chain(chords[i].0, chords[i + 1].0, forward)).collect();
        let rails_b = (0..n - 1).map(|i| r.chain(chords[i].1, chords[i + 1].1, !forward)).collect();
        let cap_end = r.chain(chords[n - 1].0, chords[n - 1].1, forward);
        let cap_start = r.chain(chords[0].0, chords[0].1, !forward);
        return Some(Strip { stations, rails_a, rails_b, cap_start, cap_end });
    }
    None
}

/// Rows across an end cap, parallel to the end station's chord `(a, b)` and `step`
/// apart, from the station outwards until they leave the cap: the region closed by
/// `cap` (the outline from `a` around the end to `b`) and the chord. `inner` is a
/// point inside the strip, on the other side of the station. Each row keeps the
/// chord's direction (`a` side first), so a square end stays square and a round
/// end rounds off, without stitches converging on one point.
fn cap_rows(cap: &[Point], station: (Point, Point), inner: Point, step: f64) -> Vec<(Point, Point)> {
    let (a0, b0) = station;
    let u = (b0 - a0).normalized();
    if u == Point::ZERO || cap.len() < 2 || step <= 0.0 {
        return Vec::new();
    }
    let mid = a0.midpoint(b0);
    let mut n = u.perp();
    if n.dot(mid - inner) < 0.0 {
        n = -n;
    }
    let ring = cap;
    let m = ring.len();
    let mut rows = Vec::new();
    for k in 1..=4000 {
        let o = mid + n * (step * k as f64);
        let mut ts: Vec<f64> = Vec::new();
        for i in 0..m {
            let (p, q) = (ring[i], ring[(i + 1) % m]);
            let e = q - p;
            let denom = u.cross(e);
            if denom.abs() < 1e-12 {
                continue;
            }
            let ap = p - o;
            let s = ap.cross(u) / denom;
            if (0.0..1.0).contains(&s) {
                ts.push(ap.cross(e) / denom);
            }
        }
        if ts.len() < 2 {
            break;
        }
        ts.sort_by(f64::total_cmp);
        let Some((t0, t1)) = ts.as_chunks::<2>().0.iter().map(|c| (c[0], c[1])).max_by(|x, y| (x.1 - x.0).total_cmp(&(y.1 - y.0))) else { break };
        if t1 - t0 < 1e-6 {
            break;
        }
        rows.push((o + u * t0, o + u * t1));
    }
    rows
}

/// Pairs of corresponding points interpolated between two chains (each from the
/// same "station" to the next), about `step` apart along the longer chain. The
/// last pair is omitted (it starts the next section).
fn interpolate(a: &[Point], b: &[Point], step: f64, out: &mut Vec<(Point, Point)>) {
    let la = polyline::length(a);
    let lb = polyline::length(b);
    let n = ((la.max(lb) / step.max(0.02)).ceil() as usize).max(1);
    for k in 0..n {
        let t = k as f64 / n as f64;
        out.push((polyline::point_at(a, t * la), polyline::point_at(b, t * lb)));
    }
}

/// Corresponding point pairs along the whole strip, from end to end, about `step`
/// apart on the longer side. Each pair is a stitch row / rung direction. Beyond
/// the first and last stations, the ends are covered by rows parallel to them.
pub fn strip_pairs(strip: &Strip, step: f64) -> Vec<(Point, Point)> {
    let mut out = Vec::new();
    let n = strip.stations.len();
    let inner = |i: usize| strip.stations.get(i).map(|(a, b)| a.midpoint(*b));
    // start cap: from the far end in to the first station
    if let (Some(&first), Some(towards)) = (strip.stations.first(), inner(1)) {
        let mut rows = cap_rows(&strip.cap_start, first, towards, step);
        rows.reverse();
        out.extend(rows);
    }
    for (ra, rb) in strip.rails_a.iter().zip(&strip.rails_b) {
        interpolate(ra, rb, step, &mut out);
    }
    // the last station, then the far cap outwards
    if let Some(&last) = strip.stations.last() {
        out.push(last);
        if let Some(towards) = n.checked_sub(2).and_then(inner) {
            out.extend(cap_rows(&strip.cap_end, last, towards, step));
        }
    }
    out
}

/// Automatically finds a turning strip for a column-like outline: chooses the two
/// ends ("tips") so that the rungs across the shape are as short as possible and
/// stay inside it, then pairs the two sides proportionally.
pub fn auto_strip(ring: &[Point]) -> Option<Strip> {
    let guides = auto_guides(ring)?;
    strip_from_guides(ring, &guides)
}

/// Places guides automatically along a column-like outline. The guides are plain
/// editable data: callers store them on the object so users can adjust them.
/// How far (degrees) a column turns between two automatic guides at most.
const BEND_DEG: f64 = 20.0;

pub fn auto_guides(ring: &[Point]) -> Option<Vec<Guide>> {
    const N: usize = 96;
    const SAMPLES: usize = 24;
    if ring.len() < 3 {
        return None;
    }
    let r = Ring::new(ring);
    if r.total <= 1e-6 {
        return None;
    }
    let pts: Vec<Point> = (0..N).map(|i| r.at(r.total * i as f64 / N as f64)).collect();
    let poly = Polygon::from_ring(ring.to_vec());
    // A rung lying along a straight stretch of the outline (across a square end)
    // stays in the shape: its midpoint is on the outline, not outside it.
    let inside = |p: Point| poly.contains(p) || poly.distance_to_boundary(p) < 1e-6;
    // whether the rung between sample points a and b has its middle in the shape, worked out
    // once per pair (the search below asks about the same pairs many times)
    let mut memo: Vec<Option<bool>> = vec![None; N * N];
    let mut rung_inside = |a: usize, b: usize, pa: Point, pb: Point| -> bool {
        let key = a.min(b) * N + a.max(b);
        match memo.get_mut(key) {
            Some(Some(v)) => *v,
            Some(slot) => {
                let v = inside(pa.midpoint(pb));
                *slot = Some(v);
                v
            }
            None => inside(pa.midpoint(pb)),
        }
    };
    let min_gap = N / 6;
    let mut best: Option<(f64, usize, usize)> = None;
    for i in 0..N {
        for j in (i + min_gap)..(i + N - min_gap + 1).min(i + N) {
            let jj = j % N;
            // rail A: i → j forward, rail B: i → j backward
            let len_a = (j - i) as f64;
            let len_b = (N - (j - i)) as f64;
            let mut score = 0.0;
            for k in 1..SAMPLES {
                let t = k as f64 / SAMPLES as f64;
                let ia = (i as f64 + t * len_a).round() as usize % N;
                let ib = ((i + N) as f64 - t * len_b).round() as usize % N;
                let (pa, pb) = (pts[ia], pts[ib]);
                let mut rung = pa.distance(pb);
                if !rung_inside(ia, ib, pa, pb) {
                    rung *= 8.0;
                }
                score += rung;
            }
            let _ = jj;
            if best.is_none_or(|b| score < b.0) {
                best = Some((score, i, j % N));
            }
        }
    }
    let (_, i, j) = best?;
    let (pi, pj) = (r.total * i as f64 / N as f64, r.total * j as f64 / N as f64);
    let rail_a = r.chain(pi, pj, true);
    let rail_b = r.chain(pi, pj, false);
    let (la, lb) = (polyline::length(&rail_a), polyline::length(&rail_b));
    // Pair the two sides by dynamic time warping: each point of one side with points
    // of the other, in order, so the rungs are as short as possible (proportional
    // pairing skews them where one side is longer, round a bend or a bulge).
    const M: usize = 48;
    let sa: Vec<Point> = (0..=M).map(|k| polyline::point_at(&rail_a, la * k as f64 / M as f64)).collect();
    let sb: Vec<Point> = (0..=M).map(|k| polyline::point_at(&rail_b, lb * k as f64 / M as f64)).collect();
    let rungs: Vec<(Point, Point)> =
        warp(&sa, &sb).into_iter().filter(|&(a, b)| (2..M - 1).contains(&a) && (2..M - 1).contains(&b)).map(|(a, b)| (sa[a], sb[b])).collect();
    // A column's rungs are about as long as it is thick. Much longer ones run along
    // the shape rather than across it (a branched shape such as an "E"), and rungs
    // leaving the shape cross a gap: such shapes get no automatic turning.
    let thick = poly.thickness().max(0.3);
    let bad = rungs.iter().filter(|(a, b)| a.distance(*b) > 1.6 * thick + 0.3 || !inside(a.midpoint(*b))).count();
    if rungs.len() < 4 || bad * 100 > rungs.len() * 15 {
        return None;
    }
    // Guides about every 1.5 column widths along the centre line.
    let mut widths: Vec<f64> = rungs.iter().map(|(a, b)| a.distance(*b)).collect();
    widths.sort_by(f64::total_cmp);
    let width = widths.get(widths.len() / 2).copied().unwrap_or(1.0).max(0.5);
    let spacing = (width * 1.5).max(3.0);
    let mut guides = Vec::new();
    let (mut travelled, mut next, mut prev) = (0.0, spacing * 0.5, None::<Point>);
    // and one wherever the column has turned by BEND_DEG since the last guide (at least half a
    // width on), so a tight bend gets the lines it needs to turn smoothly, as a digitizer would
    // place them by hand round a curve
    let angle = |a: Point, b: Point| (b.y - a.y).atan2(b.x - a.x);
    let turned = |x: f64, y: f64| {
        // rungs have no front: a half turn is no turn
        let d = (x - y).rem_euclid(std::f64::consts::PI);
        d.min(std::f64::consts::PI - d)
    };
    let mut last: Option<(f64, f64)> = None;
    for &(a, b) in &rungs {
        let mid = a.midpoint(b);
        travelled += prev.map_or(0.0, |p| p.distance(mid));
        prev = Some(mid);
        let bend = last.is_some_and(|(at, dir)| travelled - at >= width * 0.5 && turned(angle(a, b), dir) >= BEND_DEG.to_radians());
        if travelled >= next || bend {
            guides.push(Guide::new(a, b));
            last = Some((travelled, angle(a, b)));
            next = travelled + spacing;
        } else if last.is_none() {
            last = Some((travelled, angle(a, b)));
        }
    }
    if guides.len() < 2 {
        // a short column: two guides, a third and two thirds of the way along
        guides = [rungs.len() / 3, rungs.len() * 2 / 3].iter().filter_map(|&k| rungs.get(k)).map(|&(a, b)| Guide::new(a, b)).collect();
    }
    guides.retain(|g| g.length() > 1e-6);
    // The sampled rungs lean a little where the two sides were sampled out of step;
    // squared up, the rows cross straight columns squarely and curves radially.
    let square: Vec<Guide> = guides.iter().map(|&g| shortest_chord(&r, g).unwrap_or(g)).collect();
    if strip_from_guides(ring, &square).is_some() {
        guides = square;
    }
    (guides.len() >= 2).then_some(guides)
}

/// The shortest chord of the outline through a guide's midpoint, turned at most 30°
/// from the guide: the way across the shape there, as a guide with the same sides.
fn shortest_chord(ring: &Ring, g: Guide) -> Option<Guide> {
    let m = g.midpoint();
    let d0 = (g.b - g.a).normalized();
    if d0 == Point::ZERO {
        return None;
    }
    let chord = |deg: f64| -> Option<(f64, Guide)> {
        let (s, c) = deg.to_radians().sin_cos();
        let d = Point::new(d0.x * c - d0.y * s, d0.x * s + d0.y * c);
        let hits = ring.line_hits(m, d);
        // the inside stretch of the line around the midpoint (t = 0)
        hits.as_chunks::<2>().0.iter().find(|h| h[0].0 <= 0.0 && 0.0 <= h[1].0).map(|h| (h[1].0 - h[0].0, Guide::new(m + d * h[0].0, m + d * h[1].0)))
    };
    let shortest =
        |degs: &mut dyn Iterator<Item = f64>| degs.filter_map(|deg| chord(deg).map(|(len, g)| (len, deg, g))).min_by(|x, y| x.0.total_cmp(&y.0));
    let (_, coarse, _) = shortest(&mut (-12..=12).map(|k| f64::from(k) * 2.5))?;
    shortest(&mut (-10..=10).map(|k| (coarse + f64::from(k) * 0.25).clamp(-30.0, 30.0))).map(|b| b.2)
}

/// Dynamic time warping of two point sequences: the monotone pairing of their
/// points (both ends paired) with the least total distance.
fn warp(a: &[Point], b: &[Point]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return Vec::new();
    }
    let mut cost = vec![f64::INFINITY; n * m];
    for i in 0..n {
        for j in 0..m {
            let prev = match (i, j) {
                (0, 0) => 0.0,
                (0, _) => cost[j - 1],
                (_, 0) => cost[(i - 1) * m],
                _ => cost[(i - 1) * m + j - 1].min(cost[i * m + j - 1]).min(cost[(i - 1) * m + j]),
            };
            cost[i * m + j] = prev + a[i].distance(b[j]);
        }
    }
    let (mut i, mut j) = (n - 1, m - 1);
    let mut path = vec![(i, j)];
    while i > 0 || j > 0 {
        (i, j) = match (i, j) {
            (0, _) => (0, j - 1),
            (_, 0) => (i - 1, 0),
            _ => {
                let (d, l, u) = (cost[(i - 1) * m + j - 1], cost[i * m + j - 1], cost[(i - 1) * m + j]);
                if d <= l && d <= u {
                    (i - 1, j - 1)
                } else if l <= u {
                    (i, j - 1)
                } else {
                    (i - 1, j)
                }
            }
        };
        path.push((i, j));
    }
    path.reverse();
    path
}

/// The straight-fill angle (degrees) that sews the polygon with the fewest
/// breaks (boustrophedon sections), then the fewest rows; ties prefer angles
/// near 45°.
pub fn auto_fill_angle(polygon: &Polygon, spacing: f64) -> f64 {
    let mut best = (f64::INFINITY, 45.0);
    for step in 0..36 {
        let angle = step as f64 * 5.0;
        let Some((sections, rows)) = crate::engine::fill_statistics(polygon, angle, spacing) else { continue };
        let score = sections as f64 * 1000.0 + rows as f64 + ((angle - 45.0).abs() / 180.0);
        if score < best.0 {
            best = (score, angle);
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A curved band (half annulus) from angle 0 to π.
    fn arc_band() -> Vec<Point> {
        let mut ring = Vec::new();
        for i in 0..=24 {
            let a = std::f64::consts::PI * i as f64 / 24.0;
            ring.push(Point::new(20.0 * a.cos(), -20.0 * a.sin()));
        }
        for i in (0..=24).rev() {
            let a = std::f64::consts::PI * i as f64 / 24.0;
            ring.push(Point::new(14.0 * a.cos(), -14.0 * a.sin()));
        }
        ring
    }

    /// The most needle points in any 1 mm square.
    fn densest_cell(pairs: &[(Point, Point)]) -> usize {
        let mut cells = std::collections::HashMap::new();
        for p in pairs.iter().flat_map(|(a, b)| [*a, *b]) {
            *cells.entry((p.x.floor() as i64, p.y.floor() as i64)).or_insert(0usize) += 1;
        }
        cells.into_values().max().unwrap_or(0)
    }

    #[test]
    fn strip_ends_are_covered_without_piling_up() {
        // a 70 × 14 mm bar: square ends
        let bar = vec![Point::new(0.0, 0.0), Point::new(70.0, 0.0), Point::new(70.0, 14.0), Point::new(0.0, 14.0)];
        let strip = auto_strip(&bar).expect("bar strip");
        let pairs = strip_pairs(&strip, 0.2);
        let xs: Vec<f64> = pairs.iter().flat_map(|(a, b)| [a.x, b.x]).collect();
        let (lo, hi) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), x| (l.min(*x), h.max(*x)));
        assert!(lo < 0.3 && hi > 69.7, "covers the whole bar: {lo}..{hi}");
        for (a, b) in &pairs {
            for p in [a, b] {
                assert!(p.x > -1e-6 && p.x < 70.0 + 1e-6 && p.y > -1e-6 && p.y < 14.0 + 1e-6, "{p:?}");
            }
        }
        // rows every 0.2 mm put about 5 needle points per mm on each side; the old fan
        // piled dozens into the corners
        assert!(densest_cell(&pairs) <= 14, "{}", densest_cell(&pairs));
        // a 60 × 12 mm stadium: rows shorten towards the round ends, all inside
        let mut stadium = Vec::new();
        for i in 0..=16 {
            let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / 16.0;
            stadium.push(Point::new(54.0 + 6.0 * a.cos(), 6.0 + 6.0 * a.sin()));
        }
        for i in 0..=16 {
            let a = std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / 16.0;
            stadium.push(Point::new(6.0 + 6.0 * a.cos(), 6.0 + 6.0 * a.sin()));
        }
        let poly = Polygon::from_ring(stadium.clone());
        let strip = auto_strip(&stadium).expect("stadium strip");
        let pairs = strip_pairs(&strip, 0.2);
        for (a, b) in &pairs {
            assert!(poly.contains(a.midpoint(*b)) || poly.distance_to_boundary(a.midpoint(*b)) < 0.05);
        }
        let ends: Vec<f64> = pairs.iter().map(|(a, b)| a.midpoint(*b).x).collect();
        let (lo, hi) = ends.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), x| (l.min(*x), h.max(*x)));
        assert!(lo < 0.5 && hi > 59.5, "reaches the round ends: {lo}..{hi}");
        assert!(densest_cell(&pairs) <= 14, "{}", densest_cell(&pairs));
    }

    #[test]
    fn strip_from_two_guides() {
        let ring = arc_band();
        // radial guides at 45° and 135°
        let g = |deg: f64| {
            let a = deg.to_radians();
            Guide::new(Point::new(12.0 * a.cos(), -12.0 * a.sin()), Point::new(22.0 * a.cos(), -22.0 * a.sin()))
        };
        let strip = strip_from_guides(&ring, &[g(45.0), g(90.0), g(135.0)]).expect("valid strip");
        assert_eq!(strip.stations.len(), 3);
        assert_eq!(strip.rails_a.len(), 2);
        let pairs = strip_pairs(&strip, 0.4);
        assert!(pairs.len() > 50);
        // every pair is roughly radial: both points on the same ray from the origin
        for (a, b) in &pairs {
            let ang = (a.angle() - b.angle()).abs();
            assert!(ang < 0.35 || (ang - std::f64::consts::TAU).abs() < 0.35, "{a:?} {b:?}");
        }
    }

    #[test]
    fn crossing_guides_are_rejected() {
        let ring = arc_band();
        let at = |r: f64, deg: f64| {
            let a: f64 = deg.to_radians();
            Point::new(r * a.cos(), -r * a.sin())
        };
        // two chords forming an X inside the band
        let g1 = Guide::new(at(14.0, 50.0), at(20.0, 70.0));
        let g2 = Guide::new(at(14.0, 70.0), at(20.0, 50.0));
        assert!(strip_from_guides(&ring, &[g1, g2]).is_none());
        // the same chords without crossing are fine
        let g2 = Guide::new(at(14.0, 90.0), at(20.0, 110.0));
        assert!(strip_from_guides(&ring, &[g1, g2]).is_some());
    }

    #[test]
    fn auto_guides_follow_a_band() {
        let ring = arc_band();
        let guides = auto_guides(&ring).expect("guides");
        assert!(guides.len() >= 3, "{}", guides.len());
        for g in &guides {
            // each guide spans the band width (≈ 6 mm) and is roughly radial
            assert!(g.length() > 4.5 && g.length() < 9.0, "{}", g.length());
            let m = g.midpoint();
            let radial = m.normalized();
            let dir = (g.b - g.a).normalized();
            assert!(radial.dot(dir).abs() > 0.8, "not radial: {g:?}");
        }
        assert!(strip_from_guides(&ring, &guides).is_some());
    }

    #[test]
    fn auto_guides_cross_columns_squarely() {
        // A bar's guides are square to it, whichever way round its outline runs and
        // wherever it starts, so its ends are sewn with rows parallel to them.
        let bar = [Point::new(17.6, 17.6), Point::new(88.2, 17.6), Point::new(88.2, 31.8), Point::new(17.6, 31.8)];
        for k in 0..4 {
            for reversed in [false, true] {
                let mut ring: Vec<Point> = bar.iter().cycle().skip(k).take(4).copied().collect();
                if reversed {
                    ring.reverse();
                }
                let guides = auto_guides(&ring).expect("guides");
                for g in &guides {
                    assert!((g.a.x - g.b.x).abs() < 0.1, "{k} {reversed}: {g:?}");
                    assert!((g.length() - 14.2).abs() < 0.05, "{k} {reversed}: {g:?}");
                }
                let strip = strip_from_guides(&ring, &guides).expect("strip");
                let pairs = strip_pairs(&strip, 0.2);
                for (a, b) in pairs.iter().take(5).chain(pairs.iter().rev().take(5)) {
                    assert!((a.x - b.x).abs() < 0.1, "end rows lean: {a:?} {b:?}");
                }
            }
        }
        // a band's guides cross it radially, the shortest way
        let ring = arc_band();
        for g in auto_guides(&ring).expect("guides") {
            assert!((g.length() - 6.0).abs() < 0.2, "{}", g.length());
        }
    }

    #[test]
    fn auto_angle_prefers_unbroken_rows() {
        // An L shape: rows at 0° or 90° need 2 sections; 45° needs more breaks? Use a U.
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
        let a = auto_fill_angle(&poly, 0.4);
        // vertical rows sew the U as a single section
        assert!((a - 90.0).abs() < 1e-9, "{a}");
    }
}
