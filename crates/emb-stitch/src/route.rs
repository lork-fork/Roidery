//! Automatic start and end points: an object starts where the needle already is, so the move to
//! it is as short as it can be (fewer trims, less thread to cut). Lines sew their parts nearest
//! first, an open line from whichever end is nearer, and a closed one from its nearest point;
//! a tatami fill starts from whichever side is nearer.
//!
//! Only changes that keep the object looking the same are made: an E-stitch or rope keeps its
//! direction (its prongs and slant depend on it), Satin Blocks keep the start drawn as rung 0,
//! manual stitches stay as they are, and a column's width profile keeps its direction.

use kurbo::Point;

use crate::{Polyline, Shape, Stitch, StitchSettings, StitchType, generate};

/// [`generate`], starting as near `from` (where the needle is, mm) as the object allows.
pub fn generate_from(shape: &Shape, settings: &StitchSettings, from: Option<Point>) -> Vec<Stitch> {
    let Some(from) = from.filter(|p| p.x.is_finite() && p.y.is_finite()) else { return generate(shape, settings) };
    match (settings.kind, shape) {
        (StitchType::Run | StitchType::Triple | StitchType::Satin, Shape::Paths(ps)) => generate(&Shape::Paths(order(ps, from, true)), settings),
        (StitchType::EStitch | StitchType::Rope, Shape::Paths(ps)) => generate(&Shape::Paths(order(ps, from, false)), settings),
        (StitchType::Tatami, Shape::Area(_) | Shape::Paths(_)) => {
            // The same rows sewn from the other side: whichever starts nearer.
            let a = generate(shape, settings);
            let mut turned = settings.clone();
            turned.fill.angle = (settings.fill.angle + 180.0).rem_euclid(360.0);
            let b = generate(shape, &turned);
            let dist = |v: &[Stitch]| v.first().map_or(f64::INFINITY, |s| s.point().distance(from));
            if dist(&b) + 1e-6 < dist(&a) { b } else { a }
        }
        _ => generate(shape, settings),
    }
}

/// `paths` in sewing order from `from`: each next part the one nearest where the last ended, an
/// open part reversed when its end is nearer (if `reverse`), a closed one started at its point
/// nearest the needle.
pub fn order(paths: &[Polyline], from: Point, reverse: bool) -> Vec<Polyline> {
    let mut left: Vec<&Polyline> = paths.iter().filter(|p| !p.points.is_empty()).collect();
    let mut out = Vec::with_capacity(left.len());
    let mut at = from;
    while !left.is_empty() {
        // The part that can start nearest, and how.
        let mut best: Option<(usize, f64)> = None;
        for (i, p) in left.iter().enumerate() {
            let d = entry_distance(p, at, reverse);
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        let Some((i, _)) = best else { break };
        let p = left.remove(i);
        let routed = enter(p, at, reverse);
        if let Some(end) = exit(&routed) {
            at = end;
        }
        out.push(routed);
    }
    out
}

/// How near `at` the part can start.
fn entry_distance(p: &Polyline, at: Point, reverse: bool) -> f64 {
    if p.closed {
        return nearest_on_ring(p, at).map_or(f64::INFINITY, |(_, q)| q.distance(at));
    }
    let first = p.points.first().map_or(f64::INFINITY, |q| q.distance(at));
    let last = p.points.last().map_or(f64::INFINITY, |q| q.distance(at));
    if reverse { first.min(last) } else { first }
}

/// The part started as near `at` as it can be.
fn enter(p: &Polyline, at: Point, reverse: bool) -> Polyline {
    if p.closed {
        let Some((i, q)) = nearest_on_ring(p, at) else { return p.clone() };
        // Start at q on the edge from point i, round to point i, and back to q.
        let n = p.points.len();
        let mut pts = vec![q];
        pts.extend((1..=n).filter_map(|k| p.points.get((i + k) % n).copied()));
        // Drop q's twin when it is the point itself.
        pts.dedup_by(|a, b| a.distance(*b) < 1e-9);
        if pts.len() > 1 && pts.first().zip(pts.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-9) {
            pts.pop();
        }
        return Polyline::new(pts, true);
    }
    let (first, last) = (p.points.first().copied(), p.points.last().copied());
    match (first, last) {
        (Some(f), Some(l)) if reverse && l.distance(at) < f.distance(at) => Polyline::new(p.points.iter().rev().copied().collect(), false),
        _ => p.clone(),
    }
}

/// Where sewing the part ends.
fn exit(p: &Polyline) -> Option<Point> {
    if p.closed { p.points.first().copied() } else { p.points.last().copied() }
}

/// The point of closed `p` nearest `at`, and the index of the point starting its edge.
fn nearest_on_ring(p: &Polyline, at: Point) -> Option<(usize, Point)> {
    let n = p.points.len();
    let mut best: Option<(usize, Point, f64)> = None;
    for (i, (a, b)) in p.points.iter().zip(p.points.iter().cycle().skip(1)).take(n).enumerate() {
        let ab = *b - *a;
        let len2 = ab.hypot2();
        let t = if len2 > 1e-18 { ((at - *a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let q = *a + ab * t;
        let d = q.distance(at);
        if best.is_none_or(|(_, _, bd)| d < bd) {
            best = Some((i, q, d));
        }
    }
    best.map(|(i, q, _)| (i, q))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StitchKind;

    fn line(a: (f64, f64), b: (f64, f64)) -> Polyline {
        Polyline::new(vec![Point::new(a.0, a.1), Point::new(b.0, b.1)], false)
    }

    fn square(x: f64, y: f64, size: f64) -> Polyline {
        Polyline::new(vec![Point::new(x, y), Point::new(x + size, y), Point::new(x + size, y + size), Point::new(x, y + size)], true)
    }

    #[test]
    fn lines_start_at_the_nearer_end_and_parts_go_nearest_first() {
        let ps = [line((0.0, 0.0), (10.0, 0.0)), line((50.0, 0.0), (40.0, 0.0)), line((20.0, 0.0), (30.0, 0.0))];
        let o = order(&ps, Point::new(60.0, 0.0), true);
        let starts: Vec<f64> = o.iter().map(|p| p.points[0].x).collect();
        assert_eq!(starts, [50.0, 30.0, 10.0]);
        // Not reversible: each still starts at its own start.
        let o = order(&ps, Point::new(60.0, 0.0), false);
        assert!(o.iter().all(|p| ps.iter().any(|q| q.points == p.points)));
    }

    #[test]
    fn closed_paths_start_at_the_point_nearest_the_needle() {
        let o = order(&[square(0.0, 0.0, 10.0)], Point::new(5.0, -3.0), true);
        assert_eq!(o[0].points[0], Point::new(5.0, 0.0));
        assert_eq!(o[0].points.len(), 5);
        assert!(o[0].closed);
        // On a corner: no doubled point.
        let o = order(&[square(0.0, 0.0, 10.0)], Point::new(12.0, 12.0), true);
        assert_eq!(o[0].points[0], Point::new(10.0, 10.0));
        assert_eq!(o[0].points.len(), 4);
    }

    #[test]
    fn a_run_and_a_fill_start_near_the_needle() {
        let run = StitchSettings::of(StitchType::Run);
        let st = generate_from(&Shape::Paths(vec![line((0.0, 0.0), (30.0, 0.0))]), &run, Some(Point::new(31.0, 0.0)));
        assert_eq!(st[0].point(), Point::new(30.0, 0.0));
        // Without a needle position: as generate.
        let shape = Shape::Area(vec![square(0.0, 0.0, 20.0)]);
        let fill = StitchSettings::of(StitchType::Tatami);
        assert_eq!(generate_from(&shape, &fill, None), generate(&shape, &fill));
        let plain = generate(&shape, &fill);
        let p0 = plain[0].point();
        // From the opposite corner, the fill starts on that side instead.
        let far = Point::new(20.0 - p0.x, 20.0 - p0.y);
        let st = generate_from(&shape, &fill, Some(far));
        assert!(st[0].point().distance(far) < p0.distance(far) - 5.0, "{:?} {:?}", st[0].point(), p0);
        assert_eq!(st.iter().filter(|s| s.kind == StitchKind::Top).count(), plain.iter().filter(|s| s.kind == StitchKind::Top).count());
    }
}
