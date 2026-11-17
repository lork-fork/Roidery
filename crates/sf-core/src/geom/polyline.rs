//! Polyline and ring utilities.
//!
//! A *polyline* is an open sequence of points. A *ring* is a closed polygon boundary
//! stored without repeating the first point at the end.

use super::{BBox, Point};

/// Total length of an open polyline.
pub fn length(pts: &[Point]) -> f64 {
    pts.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Perimeter of a ring (includes the closing segment).
pub fn ring_length(ring: &[Point]) -> f64 {
    if ring.len() < 2 {
        return 0.0;
    }
    length(ring) + ring[ring.len() - 1].distance(ring[0])
}

/// Returns the ring as an explicitly closed polyline (first point repeated at the end).
pub fn closed(ring: &[Point]) -> Vec<Point> {
    let mut v = ring.to_vec();
    if let (Some(&f), Some(&l)) = (ring.first(), ring.last())
        && !f.approx_eq(l, 1e-12)
    {
        v.push(f);
    }
    v
}

/// Removes consecutive duplicate points (closer than `eps`).
pub fn dedup(pts: &[Point], eps: f64) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(pts.len());
    for &p in pts {
        if out.last().is_none_or(|q| q.distance(p) > eps) {
            out.push(p);
        }
    }
    out
}

/// Same as [`dedup`] but also drops a final point that duplicates the first.
pub fn dedup_ring(ring: &[Point], eps: f64) -> Vec<Point> {
    let mut out = dedup(ring, eps);
    while out.len() > 1 && out[0].distance(out[out.len() - 1]) <= eps {
        out.pop();
    }
    out
}

/// Point at arc length `dist` along the polyline (clamped to the ends).
pub fn point_at(pts: &[Point], dist: f64) -> Point {
    if pts.is_empty() {
        return Point::ZERO;
    }
    if dist <= 0.0 {
        return pts[0];
    }
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let l = w[0].distance(w[1]);
        if acc + l >= dist && l > 0.0 {
            return w[0].lerp(w[1], (dist - acc) / l);
        }
        acc += l;
    }
    pts[pts.len() - 1]
}

/// The points [`point_at`] gives at each of `dists` (ascending), in one walk along the
/// polyline instead of one from its start per distance: the same points, in linear time.
pub fn points_at(pts: &[Point], dists: impl IntoIterator<Item = f64>) -> Vec<Point> {
    let dists = dists.into_iter();
    let mut out = Vec::with_capacity(dists.size_hint().0);
    let (Some(&first), Some(&last)) = (pts.first(), pts.last()) else {
        out.extend(dists.map(|_| Point::ZERO));
        return out;
    };
    let mut i = 0;
    let mut acc = 0.0;
    for dist in dists {
        if dist <= 0.0 {
            out.push(first);
            continue;
        }
        let mut found = None;
        while let (Some(&a), Some(&b)) = (pts.get(i), pts.get(i + 1)) {
            let l = a.distance(b);
            if acc + l >= dist && l > 0.0 {
                found = Some(a.lerp(b, (dist - acc) / l));
                break;
            }
            acc += l;
            i += 1;
        }
        out.push(found.unwrap_or(last));
    }
    out
}

/// Unit tangent at arc length `dist`.
pub fn tangent_at(pts: &[Point], dist: f64) -> Point {
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let l = w[0].distance(w[1]);
        if acc + l >= dist && l > 0.0 {
            return (w[1] - w[0]) / l;
        }
        acc += l;
    }
    if pts.len() >= 2 { (pts[pts.len() - 1] - pts[pts.len() - 2]).normalized() } else { Point::new(1.0, 0.0) }
}

/// Cumulative arc lengths: `out[i]` is the distance from `pts[0]` to `pts[i]`.
pub fn cumulative_lengths(pts: &[Point]) -> Vec<f64> {
    let mut out = Vec::with_capacity(pts.len());
    let mut acc = 0.0;
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            acc += pts[i - 1].distance(*p);
        }
        out.push(acc);
    }
    out
}

/// Portion of a polyline between two arc lengths (`from <= to`).
pub fn sub_polyline(pts: &[Point], from: f64, to: f64) -> Vec<Point> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let cum = cumulative_lengths(pts);
    let mut out = vec![point_at(pts, from)];
    for (i, &d) in cum.iter().enumerate() {
        if d > from && d < to {
            out.push(pts[i]);
        }
    }
    out.push(point_at(pts, to));
    dedup(&out, 1e-9)
}

/// Interior turning angle magnitude at `b` for the path a→b→c, in radians
/// (0 = straight on, π = full reversal).
pub fn turn_angle(a: Point, b: Point, c: Point) -> f64 {
    let u = (b - a).normalized();
    let v = (c - b).normalized();
    if u == Point::ZERO || v == Point::ZERO {
        return 0.0;
    }
    u.cross(v).atan2(u.dot(v)).abs()
}

/// Distributes running-stitch needle points along an open polyline.
///
/// Vertices where the path turns by more than `corner_angle` radians are kept
/// exactly; each stretch between kept vertices is divided into equal stitches no
/// longer than `step` (so stitch lengths are as even as possible and corners are
/// always hit).
pub fn resample(pts: &[Point], step: f64, corner_angle: f64) -> Vec<Point> {
    let pts = dedup(pts, 1e-9);
    if pts.len() < 2 || step <= 0.0 {
        return pts;
    }
    // Indices of anchor vertices.
    let mut anchors = vec![0usize];
    for i in 1..pts.len() - 1 {
        if turn_angle(pts[i - 1], pts[i], pts[i + 1]) > corner_angle {
            anchors.push(i);
        }
    }
    anchors.push(pts.len() - 1);

    let mut out = vec![pts[0]];
    for w in anchors.windows(2) {
        let piece = &pts[w[0]..=w[1]];
        let len = length(piece);
        let n = ((len / step).ceil() as usize).max(1);
        let seg = len / n as f64;
        out.extend(points_at(piece, (1..n).map(|k| seg * k as f64)));
        out.push(pts[w[1]]);
    }
    out
}

/// Like [`resample`] for a ring; the result starts and ends at `ring[0]`
/// (i.e. it is an explicitly closed polyline).
pub fn resample_ring(ring: &[Point], step: f64, corner_angle: f64) -> Vec<Point> {
    let n = ring.len();
    if n < 3 {
        return closed(ring);
    }
    // Rotate so that we start at a corner if there is one; this keeps the
    // seam on a corner instead of the middle of a smooth stretch.
    let start = (0..n).find(|&i| turn_angle(ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]) > corner_angle).unwrap_or(0);
    // Walking the ring from a vertex back to the same vertex: `resample` always
    // keeps both ends, so the seam vertex is hit exactly.
    let path: Vec<Point> = (0..=n).map(|k| ring[(start + k) % n]).collect();
    resample(&path, step, corner_angle)
}

/// Ramer–Douglas–Peucker simplification of an open polyline.
pub fn simplify(pts: &[Point], tolerance: f64) -> Vec<Point> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let mut best = (0.0, 0usize);
        for i in a + 1..b {
            let d = distance_to_segment(pts[i], pts[a], pts[b]);
            if d > best.0 {
                best = (d, i);
            }
        }
        if best.0 > tolerance {
            keep[best.1] = true;
            stack.push((a, best.1));
            stack.push((best.1, b));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// RDP simplification for rings (keeps at least 3 points when possible).
pub fn simplify_ring(ring: &[Point], tolerance: f64) -> Vec<Point> {
    let n = ring.len();
    if n < 4 {
        return ring.to_vec();
    }
    // Split at the vertex farthest from ring[0] so both halves are well conditioned.
    let far = (1..n).max_by(|&i, &j| ring[0].distance_sq(ring[i]).total_cmp(&ring[0].distance_sq(ring[j]))).unwrap_or(n / 2);
    let first: Vec<Point> = ring[..=far].to_vec();
    let mut second: Vec<Point> = ring[far..].to_vec();
    second.push(ring[0]);
    let mut out = simplify(&first, tolerance);
    let s2 = simplify(&second, tolerance);
    out.extend_from_slice(&s2[1..s2.len() - 1]);
    if out.len() < 3 { ring.to_vec() } else { out }
}

/// Distance from `p` to segment `ab`.
pub fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    p.distance(closest_on_segment(p, a, b).0)
}

/// Closest point on segment `ab` to `p`, and its parameter `t` in `[0, 1]`.
pub fn closest_on_segment(p: Point, a: Point, b: Point) -> (Point, f64) {
    let ab = b - a;
    let l2 = ab.length_sq();
    if l2 < 1e-24 {
        return (a, 0.0);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    (a + ab * t, t)
}

/// Result of projecting a point onto a polyline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection {
    /// Index of the segment (`pts[segment]..pts[segment + 1]`).
    pub segment: usize,
    /// Parameter along that segment.
    pub t: f64,
    pub point: Point,
    pub distance: f64,
    /// Arc length from the start of the polyline.
    pub along: f64,
}

/// Projects `p` onto the polyline. Returns `None` for fewer than 2 points.
pub fn project(pts: &[Point], p: Point) -> Option<Projection> {
    if pts.len() < 2 {
        return None;
    }
    let mut best: Option<Projection> = None;
    let mut acc = 0.0;
    for (i, w) in pts.windows(2).enumerate() {
        let (q, t) = closest_on_segment(p, w[0], w[1]);
        let d = q.distance(p);
        let seg_len = w[0].distance(w[1]);
        if best.is_none_or(|b| d < b.distance) {
            best = Some(Projection { segment: i, t, point: q, distance: d, along: acc + seg_len * t });
        }
        acc += seg_len;
    }
    best
}

/// Intersection of segments `a1a2` and `b1b2`: returns `(t, u)` parameters when they cross.
pub fn segment_intersection(a1: Point, a2: Point, b1: Point, b2: Point) -> Option<(f64, f64)> {
    let r = a2 - a1;
    let s = b2 - b1;
    let denom = r.cross(s);
    if denom.abs() < 1e-14 {
        return None;
    }
    let qp = b1 - a1;
    let t = qp.cross(s) / denom;
    let u = qp.cross(r) / denom;
    if (-1e-12..=1.0 + 1e-12).contains(&t) && (-1e-12..=1.0 + 1e-12).contains(&u) { Some((t, u)) } else { None }
}

/// Signed area of a ring via the shoelace formula. Positive means counter-clockwise
/// in a y-up system (clockwise on screen in y-down design space).
pub fn signed_area(ring: &[Point]) -> f64 {
    let n = ring.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..n {
        let p = ring[i];
        let q = ring[(i + 1) % n];
        a += p.x * q.y - q.x * p.y;
    }
    a * 0.5
}

/// Even-odd point-in-ring test.
pub fn ring_contains(ring: &[Point], p: Point) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (ring[i], ring[j]);
        if ((a.y > p.y) != (b.y > p.y)) && (p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Area centroid of a ring (falls back to the vertex mean for degenerate rings).
pub fn ring_centroid(ring: &[Point]) -> Point {
    let n = ring.len();
    if n == 0 {
        return Point::ZERO;
    }
    let a = signed_area(ring);
    if a.abs() < 1e-12 {
        let s = ring.iter().fold(Point::ZERO, |acc, p| acc + *p);
        return s / n as f64;
    }
    let (mut cx, mut cy) = (0.0, 0.0);
    for i in 0..n {
        let p = ring[i];
        let q = ring[(i + 1) % n];
        let f = p.x * q.y - q.x * p.y;
        cx += (p.x + q.x) * f;
        cy += (p.y + q.y) * f;
    }
    Point::new(cx / (6.0 * a), cy / (6.0 * a))
}

/// Distance from a point to a ring's boundary.
pub fn distance_to_ring(ring: &[Point], p: Point) -> f64 {
    let n = ring.len();
    let mut best = f64::INFINITY;
    for i in 0..n {
        best = best.min(distance_to_segment(p, ring[i], ring[(i + 1) % n]));
    }
    best
}

/// Distance from a point to an open polyline.
pub fn distance_to_polyline(pts: &[Point], p: Point) -> f64 {
    match pts.len() {
        0 => f64::INFINITY,
        1 => pts[0].distance(p),
        _ => pts.windows(2).map(|w| distance_to_segment(p, w[0], w[1])).fold(f64::INFINITY, f64::min),
    }
}

/// Averaged vertex normals (unit, pointing to the left of travel in the
/// mathematical sense, i.e. `tangent.perp()`).
pub fn vertex_normals(pts: &[Point], closed: bool) -> Vec<Point> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = if i > 0 {
            Some(pts[i - 1])
        } else if closed {
            Some(pts[n - 1])
        } else {
            None
        };
        let next = if i + 1 < n {
            Some(pts[i + 1])
        } else if closed {
            Some(pts[0])
        } else {
            None
        };
        let t_in = prev.map(|q| (pts[i] - q).normalized());
        let t_out = next.map(|q| (q - pts[i]).normalized());
        let t = match (t_in, t_out) {
            (Some(a), Some(b)) => {
                let s = a + b;
                if s.length() < 1e-9 { a } else { s.normalized() }
            }
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => Point::new(1.0, 0.0),
        };
        out.push(t.perp());
    }
    out
}

/// Offsets a polyline sideways by `d` (positive = along `tangent.perp()`), using
/// mitred joins limited to `miter_limit × |d|`.
pub fn offset_polyline(pts: &[Point], d: f64, closed: bool, miter_limit: f64) -> Vec<Point> {
    let n = pts.len();
    if n < 2 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = if i > 0 {
            Some(pts[i - 1])
        } else if closed {
            Some(pts[n - 1])
        } else {
            None
        };
        let next = if i + 1 < n {
            Some(pts[i + 1])
        } else if closed {
            Some(pts[0])
        } else {
            None
        };
        let n_in = prev.map(|q| (pts[i] - q).normalized().perp());
        let n_out = next.map(|q| (q - pts[i]).normalized().perp());
        let off = match (n_in, n_out) {
            (Some(a), Some(b)) => {
                let m = (a + b).normalized();
                let cos_half = m.dot(a);
                if m == Point::ZERO || cos_half < 1e-6 {
                    a * d
                } else {
                    let scale = (1.0 / cos_half).min(miter_limit);
                    m * (d * scale)
                }
            }
            (Some(a), None) => a * d,
            (None, Some(b)) => b * d,
            (None, None) => Point::ZERO,
        };
        out.push(pts[i] + off);
    }
    out
}

/// Bounding box of a set of points.
pub fn bbox(pts: &[Point]) -> BBox {
    BBox::from_points(pts.iter().copied())
}

/// Rotates a ring so that it starts at index `i`.
pub fn rotate_ring(ring: &[Point], i: usize) -> Vec<Point> {
    let n = ring.len();
    if n == 0 {
        return Vec::new();
    }
    (0..n).map(|k| ring[(i + k) % n]).collect()
}

/// Inserts the projection of `p` into a ring (if not already a vertex) and returns
/// the ring rotated to start at that point.
pub fn ring_starting_near(ring: &[Point], p: Point) -> Vec<Point> {
    let c = closed(ring);
    let Some(pr) = project(&c, p) else {
        return ring.to_vec();
    };
    let n = ring.len();
    let mut out = Vec::with_capacity(n + 1);
    out.push(pr.point);
    // vertices after the projection point, wrapping around
    for k in 1..=n {
        let idx = (pr.segment + k) % n;
        let v = ring[idx];
        if !v.approx_eq(pr.point, 1e-9) {
            out.push(v);
        }
    }
    dedup_ring(&out, 1e-9)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<Point> {
        vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0), Point::new(0.0, 10.0)]
    }

    /// `points_at` gives exactly the points `point_at` does (#79), repeated vertices and
    /// distances before, at and past the ends included.
    #[test]
    fn points_at_matches_point_at() {
        let mut line = square();
        line.insert(2, Point::new(10.0, 0.0));
        let dists: Vec<f64> = [-1.0, 0.0, 0.5, 10.0, 10.0, 13.3, 20.0, 29.9, 30.0, 45.0].to_vec();
        let one = points_at(&line, dists.iter().copied());
        let each: Vec<Point> = dists.iter().map(|&d| point_at(&line, d)).collect();
        assert_eq!(one, each);
        assert_eq!(points_at(&[], [1.0, 2.0]), vec![Point::ZERO; 2]);
    }

    #[test]
    fn resample_keeps_corners_and_even_steps() {
        let path = vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 7.0)];
        let r = resample(&path, 2.5, 0.5);
        assert_eq!(r.first().copied(), Some(Point::new(0.0, 0.0)));
        assert!(r.contains(&Point::new(10.0, 0.0)));
        assert_eq!(r.last().copied(), Some(Point::new(10.0, 7.0)));
        for w in r.windows(2) {
            assert!(w[0].distance(w[1]) <= 2.5 + 1e-9);
        }
        // first leg: 10 mm / 2.5 = 4 stitches, second leg: 7 / 2.5 -> 3 stitches
        assert_eq!(r.len(), 1 + 4 + 3);
    }

    #[test]
    fn ring_area_and_contains() {
        let s = square();
        assert!((signed_area(&s) - 100.0).abs() < 1e-12);
        assert!(ring_contains(&s, Point::new(5.0, 5.0)));
        assert!(!ring_contains(&s, Point::new(15.0, 5.0)));
        assert!(ring_centroid(&s).approx_eq(Point::new(5.0, 5.0), 1e-12));
    }

    #[test]
    fn simplify_removes_collinear() {
        let pts: Vec<Point> = (0..=10).map(|i| Point::new(i as f64, 0.0)).collect();
        assert_eq!(simplify(&pts, 0.01).len(), 2);
    }

    #[test]
    fn projection() {
        let pts = vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0)];
        let p = project(&pts, Point::new(12.0, 5.0)).unwrap();
        assert_eq!(p.segment, 1);
        assert!((p.along - 15.0).abs() < 1e-9);
        assert!((p.distance - 2.0).abs() < 1e-9);
    }

    #[test]
    fn ring_start() {
        let r = ring_starting_near(&square(), Point::new(5.0, -1.0));
        assert!(r[0].approx_eq(Point::new(5.0, 0.0), 1e-9));
        assert_eq!(r.len(), 5);
    }
}
