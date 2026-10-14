//! Contour fill: rows that follow the outline of the area inwards, ring inside ring, like the
//! growth lines of a shell. Each ring is the area inset by a whole number of row spacings
//! (VectorCraft's Offset Path), so holes grow and narrow parts split off as the rings shrink.
//!
//! Rings are sewn from the outside in, one lineage at a time: after a ring, the next is one a
//! level in that it shares ground with, so the band around a hole and the band along the outer
//! edge are each sewn through before the ring where they meet. A ring waits until every ring
//! one level out that touches it is sewn. Each starts at the point nearest the needle. Past the
//! outermost ring, which keeps the edge true, a ring with a single ring inside it spirals into
//! that ring as it goes round, so nested rings sew as one
//! spiral with no step lines; elsewhere the step in is short (a row spacing) and sewn as a stitch.
//! A longer move is sewn as a run when all of it lies under rings still to come, and otherwise
//! jumps.

use kurbo::{BezPath, Point};
use vectorcraft_geom::{FillRule, PathData};
use vectorcraft_pathops::{Join, normalize, offset_path};

use crate::geometry::{Polyline, polylines_from_bezpath};
use crate::settings::FillSettings;
use crate::{Stitch, StitchKind, fill, run};

/// Rings are capped so a tiny spacing in a huge shape can't run away.
const MAX_RINGS: usize = 2_000;
/// How closely rings follow curves, mm.
const FLATTEN: f64 = 0.05;

/// Contour fill of the area inside `rings` (outer rings and holes).
pub fn contour(rings: &[Polyline], s: &FillSettings) -> Vec<Stitch> {
    let mut out = vec![];
    fill::underlay(rings, s, &mut out);
    let area = to_path(rings);
    if area.is_empty() {
        return out;
    }
    let mut needle: Option<Point> = out.last().map(|st| st.point());
    let mut levels: Vec<Vec<Ring>> = vec![];
    // The first row sits half a spacing in, pushed out again by the pull compensation.
    for k in 0..MAX_RINGS {
        let delta = s.pull_compensation - s.row_spacing * (0.5 + k as f64);
        let level = offset_path(&area, delta, Join::Round, 4.0);
        let rings: Vec<Ring> = polylines_from_bezpath(&level.to_bezpath(), FLATTEN)
            .into_iter()
            .map(|p| p.vertices())
            .filter(|v| v.len() >= 3)
            .map(|pts| Ring::new(pts, k))
            .collect();
        if rings.is_empty() {
            break;
        }
        levels.push(rings);
    }
    let rings: Vec<Ring> = levels.into_iter().flatten().collect();
    let near = s.row_spacing * 1.5;
    // The rings one level out that each ring shares ground with (overlapping boxes).
    let outer: Vec<Vec<usize>> =
        rings.iter().map(|r| (0..rings.len()).filter(|&j| rings[j].level + 1 == r.level && rings[j].touches(r, near)).collect()).collect();
    // The ring each ring spirals into: its only inner ring, when that ring has no other outer one.
    // The outermost rings go round true, so the edge is crisp.
    let inner: Vec<Option<usize>> = (0..rings.len())
        .map(|i| {
            if rings[i].level == 0 {
                return None;
            }
            let mut kids = (0..rings.len()).filter(|&j| outer[j].contains(&i));
            match (kids.next(), kids.next()) {
                (Some(c), None) if outer[c].len() == 1 => Some(c),
                _ => None,
            }
        })
        .collect();
    let mut sewn = vec![false; rings.len()];
    let mut last: Option<usize> = None;
    for _ in 0..rings.len() {
        let at = needle.unwrap_or(Point::ZERO);
        let ready = |i: &usize| !sewn[*i] && outer[*i].iter().all(|&j| sewn[j]);
        // Prefer a ring one level in from the last; then the nearest ready ring.
        let follows = |i: &usize| last.is_some_and(|l| outer[*i].contains(&l));
        let pick = (0..rings.len()).filter(ready).min_by(|a, b| {
            (!follows(a), rings[*a].nearest(at).1).partial_cmp(&(!follows(b), rings[*b].nearest(at).1)).unwrap_or(std::cmp::Ordering::Equal)
        });
        let Some(i) = pick else { break };
        sewn[i] = true;
        last = Some(i);
        let mut ring = rings[i].from(rings[i].nearest(at).0);
        if let Some(c) = inner[i] {
            ring = spiral(&ring, &rings[c]);
        }
        let mut st = run::along(&ring, s.stitch_length, StitchKind::Top);
        if let (Some(first), Some(from)) = (st.first().copied(), needle)
            && first.point().distance(from) > 2.0 * s.row_spacing + s.stitch_length
        {
            let to_come: Vec<&Ring> = rings.iter().zip(&sewn).filter(|(_, d)| !**d).map(|(r, _)| r).collect();
            if covered(&to_come, from, first.point(), near, s.row_spacing) {
                // Walk over, under the rings still to come.
                let walk = run::along(&[from, first.point()], s.stitch_length, StitchKind::Top);
                out.extend(walk.into_iter().skip(1));
                st.remove(0);
            } else if let Some(f) = st.first_mut() {
                f.kind = StitchKind::Jump;
            }
        }
        needle = st.last().map(|p| p.point()).or(needle);
        out.extend(st);
    }
    out
}

/// One ring of a contour fill: a closed chain of points (first not repeated) at an inset level.
struct Ring {
    pts: Vec<Point>,
    level: usize,
    bbox: kurbo::Rect,
}

impl Ring {
    fn new(mut pts: Vec<Point>, level: usize) -> Self {
        if pts.len() > 1 && pts.first().zip(pts.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-9) {
            pts.pop();
        }
        let bbox = pts.iter().fold(kurbo::Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN), |r, p| r.union_pt(*p));
        Self { pts, level, bbox }
    }

    fn touches(&self, other: &Ring, margin: f64) -> bool {
        let a = self.bbox.inflate(margin, margin);
        a.x0 <= other.bbox.x1 && other.bbox.x0 <= a.x1 && a.y0 <= other.bbox.y1 && other.bbox.y0 <= a.y1
    }

    /// The index of the point nearest `p`, and its distance.
    fn nearest(&self, p: Point) -> (usize, f64) {
        self.pts.iter().enumerate().map(|(i, q)| (i, q.distance(p))).min_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or((0, f64::MAX))
    }

    /// The ring starting at point `i`, closed back to it.
    fn from(&self, i: usize) -> Vec<Point> {
        let n = self.pts.len();
        let mut v: Vec<Point> = (0..n).map(|k| self.pts[(i + k) % n]).collect();
        if let Some(f) = v.first().copied() {
            v.push(f);
        }
        v
    }

    /// Distance from `p` to the ring's outline.
    fn distance(&self, p: Point) -> f64 {
        self.project(p).distance(p)
    }

    /// The point on the ring's outline nearest `p`.
    fn project(&self, p: Point) -> Point {
        let n = self.pts.len();
        (0..n)
            .map(|k| {
                let (a, b) = (self.pts[k], self.pts[(k + 1) % n]);
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.hypot2().max(1e-18)).clamp(0.0, 1.0);
                a.lerp(b, t)
            })
            .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
            .unwrap_or(p)
    }
}

/// `ring` (closed, starting where it ends) bent so it drifts onto `into` as it goes round: a point
/// a fraction f of the way along moves f of the way to the nearest point of `into`. It ends on
/// `into` beside where it began, ready to go round that ring.
fn spiral(ring: &[Point], into: &Ring) -> Vec<Point> {
    // Points every half mm or closer, so the drift is smooth along straight sides.
    let mut dense: Vec<Point> = vec![];
    for w in ring.windows(2) {
        let (a, b) = (w[0], w[1]);
        let n = ((a.distance(b) / 0.5).ceil() as usize).clamp(1, 10_000);
        dense.extend((0..n).map(|k| a.lerp(b, k as f64 / n as f64)));
    }
    dense.extend(ring.last().copied());
    let total: f64 = dense.windows(2).map(|w| w[0].distance(w[1])).sum();
    if total <= 0.0 {
        return ring.to_vec();
    }
    let mut d = 0.0;
    let mut prev = dense.first().copied().unwrap_or(Point::ZERO);
    dense
        .iter()
        .map(|p| {
            d += p.distance(prev);
            prev = *p;
            p.lerp(into.project(*p), d / total)
        })
        .collect()
}

/// Whether the move from `a` to `b` lies under rings still to be sewn (every point, checked every
/// half mm apart from `margin` at each end, within a row spacing of one of them).
fn covered(to_come: &[&Ring], a: Point, b: Point, margin: f64, spacing: f64) -> bool {
    let len = a.distance(b);
    if to_come.is_empty() || len <= 2.0 * margin {
        return false;
    }
    let n = ((len / 0.5).ceil() as usize).clamp(2, 2_000);
    (0..=n).map(|i| len * i as f64 / n as f64).filter(|d| *d >= margin && *d <= len - margin).all(|d| {
        let p = a.lerp(b, d / len);
        to_come.iter().any(|r| r.bbox.inflate(spacing, spacing).contains(p) && r.distance(p) <= spacing)
    })
}

/// The rings as one closed path for Offset Path, holes wound against their outer rings (the
/// rings come in as even-odd; Offset Path reads windings).
fn to_path(rings: &[Polyline]) -> PathData {
    let mut bp = BezPath::new();
    for r in rings {
        let mut pts = r.points.iter().filter(|p| p.x.is_finite() && p.y.is_finite());
        let Some(first) = pts.next() else { continue };
        bp.move_to(*first);
        pts.for_each(|p| bp.line_to(*p));
        bp.close_path();
    }
    normalize(&PathData::from_bezpath(&bp), FillRule::EvenOdd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, size: f64) -> Polyline {
        Polyline::new(vec![Point::new(x, y), Point::new(x + size, y), Point::new(x + size, y + size), Point::new(x, y + size)], true)
    }

    fn top(st: &[Stitch]) -> Vec<Point> {
        st.iter().filter(|p| p.kind == StitchKind::Top).map(Stitch::point).collect()
    }

    #[test]
    fn rings_step_inwards_one_spacing_at_a_time() {
        let s = FillSettings { row_spacing: 1.0, underlay: false, pull_compensation: 0.0, ..Default::default() };
        let st = contour(&[square(0.0, 0.0, 10.0)], &s);
        let pts = top(&st);
        assert!(!pts.is_empty());
        // Everything inside, the outer ring half a spacing in.
        assert!(pts.iter().all(|p| p.x >= 0.49 && p.x <= 9.51 && p.y >= 0.49 && p.y <= 9.51), "{pts:?}");
        assert!(pts.iter().any(|p| (p.x - 0.5).abs() < 1e-6));
        // Five rings fit (at 0.5, 1.5, … 4.5 in); no jumps between nested rings.
        assert!(pts.iter().any(|p| (p.x - 4.5).abs() < 1e-6 && p.y > 4.0 && p.y < 6.0));
        assert!(!st.iter().any(|p| p.kind == StitchKind::Jump));
    }

    #[test]
    fn nested_rings_spiral_in_without_steps() {
        let s = FillSettings { row_spacing: 1.0, underlay: false, pull_compensation: 0.0, ..Default::default() };
        let pts = top(&contour(&[square(0.0, 0.0, 10.0)], &s));
        // How far in each stitch is; it creeps inwards and never steps a whole row at once.
        let depth: Vec<f64> = pts.iter().map(|p| p.x.min(p.y).min(10.0 - p.x).min(10.0 - p.y)).collect();
        // (The outer ring goes round true; after it, one step in.)
        let steps = depth.windows(2).filter(|w| w[1] - w[0] >= 0.5).count();
        assert!(depth.windows(2).all(|w| w[1] >= w[0] - 1e-6) && steps <= 1, "{depth:?}");
        assert!(depth.last().is_some_and(|d| *d > 4.0));
    }

    #[test]
    fn holes_stay_empty_and_bad_input_sews_nothing() {
        let s = FillSettings { row_spacing: 1.0, underlay: false, pull_compensation: 0.0, ..Default::default() };
        let st = contour(&[square(0.0, 0.0, 20.0), square(8.0, 8.0, 4.0)], &s);
        let pts = top(&st);
        assert!(!pts.iter().any(|p| p.x > 8.2 && p.x < 11.8 && p.y > 8.2 && p.y < 11.8));
        // The rings carry on past the grown hole's first rings (5.5 mm in, between edge and hole).
        assert!(pts.iter().any(|p| (p.x - 5.5).abs() < 1e-6 && p.y > 9.0 && p.y < 11.0), "{pts:?}");
        assert!(contour(&[], &s).is_empty());
        let nan = Polyline::new(vec![Point::new(f64::NAN, 0.0), Point::new(1.0, 1.0)], true);
        assert!(contour(&[nan], &s).is_empty());
    }
}
