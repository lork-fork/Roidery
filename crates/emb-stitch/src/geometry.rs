//! Polylines and the small geometry the generators share.

use kurbo::{BezPath, PathEl, Point, Vec2};

/// A flattened path, in millimetres.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polyline {
    pub points: Vec<Point>,
    /// The last point joins back to the first.
    pub closed: bool,
}

impl Polyline {
    pub fn new(points: Vec<Point>, closed: bool) -> Self {
        Self { points, closed }
    }

    /// The points in sewing order, repeating the first at the end when closed.
    pub fn vertices(&self) -> Vec<Point> {
        let mut v = self.points.clone();
        if self.closed
            && let (Some(f), Some(l)) = (v.first().copied(), v.last().copied())
            && f.distance(l) > 1e-9
        {
            v.push(f);
        }
        v
    }

    pub fn length(&self) -> f64 {
        path_length(&self.vertices())
    }
}

/// Total length of an open chain of points.
pub fn path_length(pts: &[Point]) -> f64 {
    pts.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Flatten a Bézier path into polylines, one per subpath, within `tolerance` (same units as the
/// path). Degenerate subpaths (fewer than two distinct points) are dropped.
pub fn polylines_from_bezpath(path: &BezPath, tolerance: f64) -> Vec<Polyline> {
    let tol = if tolerance.is_finite() && tolerance > 0.0 { tolerance } else { 0.05 };
    let mut out: Vec<Polyline> = vec![];
    let mut cur = Polyline::default();
    let finish = |cur: &mut Polyline, out: &mut Vec<Polyline>| {
        let mut p = std::mem::take(cur);
        p.points.dedup_by(|a, b| a.distance(*b) < 1e-9);
        if p.closed
            && p.points.len() > 2
            && let (Some(f), Some(l)) = (p.points.first().copied(), p.points.last().copied())
            && f.distance(l) < 1e-9
        {
            p.points.pop();
        }
        if p.points.len() >= 2 && p.points.iter().all(|q| q.x.is_finite() && q.y.is_finite()) {
            out.push(p);
        }
    };
    kurbo::flatten(path.elements().iter().copied(), tol, |el| match el {
        PathEl::MoveTo(p) => {
            finish(&mut cur, &mut out);
            cur.points.push(p);
        }
        PathEl::LineTo(p) => cur.points.push(p),
        PathEl::ClosePath => {
            cur.closed = true;
            finish(&mut cur, &mut out);
        }
        // flatten emits only the three above.
        PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => cur.points.push(p),
    });
    finish(&mut cur, &mut out);
    out
}

/// Points along `pts` every `step` (the last segment absorbs the remainder), keeping every
/// vertex whose turn is sharper than `corner_deg` so corners stay crisp.
pub fn resample(pts: &[Point], step: f64, corner_deg: f64) -> Vec<Point> {
    let Some(&first) = pts.first() else { return vec![] };
    if pts.len() < 2 || !(step.is_finite() && step > 0.0) {
        return pts.to_vec();
    }
    // Split at the corners, then divide each stretch evenly.
    let mut cuts = vec![0];
    for i in 1..pts.len() - 1 {
        let (a, b, c) = (pts[i - 1], pts[i], pts[i + 1]);
        if turn_deg(b - a, c - b) > corner_deg {
            cuts.push(i);
        }
    }
    cuts.push(pts.len() - 1);
    let mut out = vec![first];
    for w in cuts.windows(2) {
        let stretch = &pts[w[0]..=w[1]];
        let len = path_length(stretch);
        if len < 1e-9 {
            continue;
        }
        let n = ((len / step).round() as usize).clamp(1, 1_000_000);
        for k in 1..=n {
            out.push(point_at(stretch, len * k as f64 / n as f64));
        }
    }
    out
}

/// The point `dist` along the chain `pts` (clamped to its ends).
pub fn point_at(pts: &[Point], dist: f64) -> Point {
    let mut left = dist.max(0.0);
    for w in pts.windows(2) {
        let seg = w[0].distance(w[1]);
        if left <= seg {
            return if seg < 1e-12 { w[1] } else { w[0].lerp(w[1], left / seg) };
        }
        left -= seg;
    }
    pts.last().copied().unwrap_or_default()
}

/// Unsigned turn between two directions, degrees (0 straight on, 180 a reversal).
pub fn turn_deg(a: Vec2, b: Vec2) -> f64 {
    let (la, lb) = (a.hypot(), b.hypot());
    if la < 1e-12 || lb < 1e-12 {
        return 0.0;
    }
    (a.dot(b) / (la * lb)).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Unit normal (to the left of travel, y down) at each point of an open chain: the average of
/// the neighbouring segments' normals, scaled so the offset keeps its width at corners (miter,
/// limited to 3×).
pub fn miter_normals(pts: &[Point]) -> Vec<Vec2> {
    let n = pts.len();
    let seg_normal = |i: usize| -> Option<Vec2> {
        let d = *pts.get(i + 1)? - *pts.get(i)?;
        let l = d.hypot();
        (l > 1e-12).then(|| Vec2::new(d.y / l, -d.x / l))
    };
    (0..n)
        .map(|i| {
            let before = if i > 0 { seg_normal(i - 1) } else { None };
            let after = seg_normal(i);
            match (before, after) {
                (Some(a), Some(b)) => {
                    let m = a + b;
                    let ml = m.hypot();
                    if ml < 1e-9 {
                        return b;
                    }
                    let m = m / ml;
                    let cos = m.dot(b).max(1.0 / 3.0);
                    m / cos
                }
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => Vec2::new(0.0, 0.0),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_keeps_subpaths_and_closure() {
        let mut bp = BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((10.0, 0.0));
        bp.line_to((10.0, 10.0));
        bp.close_path();
        bp.move_to((20.0, 0.0));
        bp.curve_to((25.0, 10.0), (30.0, 10.0), (35.0, 0.0));
        let ps = polylines_from_bezpath(&bp, 0.05);
        assert_eq!(ps.len(), 2);
        assert!(ps[0].closed && ps[0].points.len() == 3);
        assert!(!ps[1].closed && ps[1].points.len() > 5);
    }

    #[test]
    fn resample_keeps_corners_and_spacing() {
        let pts = [Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0)];
        let r = resample(&pts, 2.5, 30.0);
        assert!(r.contains(&Point::new(10.0, 0.0)));
        assert_eq!(r.len(), 1 + 4 + 4);
        for w in r.windows(2) {
            assert!((w[0].distance(w[1]) - 2.5).abs() < 1e-9);
        }
    }

    #[test]
    fn miter_keeps_width_on_a_right_angle() {
        let pts = [Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0)];
        let n = miter_normals(&pts);
        assert!((n[1].hypot() - 2f64.sqrt()).abs() < 1e-9);
    }
}
