//! Travel under cover: the shortest way between two points that stays inside an area,
//! for thread carried from one object to the next under stitching sewn later
//! (branching).

use crate::geom::{Point, Polygon, poly, polyline};

/// How far inside the covering area a routed way keeps (mm), clear of its edge.
const INSET: f64 = 0.4;
/// The covering area's outline is simplified this much before routing (mm).
const SIMPLIFY: f64 = 0.2;
/// Areas with more corners than this are not routed through: the thread is trimmed.
const MAX_CORNERS: usize = 1500;
/// Parameter tolerance along a segment.
const EPS: f64 = 1e-9;

/// A way from `a` to `b` that stays under `cover`: the points to sew through after
/// `a`, the last one `b`. Each end may lie up to `slack` outside the cover (they sit
/// on the edges of the objects the thread leaves and enters). The straight line when
/// it is covered, else the shortest way round the cover's corners; `None` when there
/// is none (the cover does not join the two ends).
pub fn route(a: Point, b: Point, cover: &[Polygon], slack: f64) -> Option<Vec<Point>> {
    if straight(a, b, cover, slack) {
        return Some(vec![b]);
    }
    let inner: Vec<Polygon> = poly::offset(cover, -INSET).into_iter().map(simplified).filter(|p| !p.is_empty()).collect();
    let corners: usize = inner.iter().flat_map(|p| p.rings()).map(Vec::len).sum();
    if inner.is_empty() || corners > MAX_CORNERS {
        return None;
    }
    let (a1, pa) = enter(a, &inner, slack + INSET)?;
    let (b1, pb) = enter(b, &inner, slack + INSET)?;
    if pa != pb {
        return None;
    }
    let path = shortest(a1, b1, &inner[pa])?;
    let mut out: Vec<Point> = Vec::with_capacity(path.len() + 1);
    for p in path.into_iter().chain([b]) {
        if out.last().copied().unwrap_or(a).distance(p) > 1e-9 {
            out.push(p);
        }
    }
    Some(out)
}

/// Whether the straight way from `a` to `b` lies under `cover`, but for `slack` at
/// each end.
pub fn straight(a: Point, b: Point, cover: &[Polygon], slack: f64) -> bool {
    let len = a.distance(b);
    let k = ((len / 0.5).ceil() as usize).max(1);
    (0..=k)
        .map(|m| a.lerp(b, m as f64 / k as f64))
        .filter(|p| p.distance(a) > slack && p.distance(b) > slack)
        .all(|p| cover.iter().any(|c| c.contains(p)))
}

fn simplified(p: Polygon) -> Polygon {
    let outer = polyline::simplify_ring(&p.outer, SIMPLIFY);
    let holes = p.holes.iter().map(|h| polyline::simplify_ring(h, SIMPLIFY)).collect();
    Polygon::new(outer, holes)
}

fn inside(piece: &Polygon, p: Point) -> bool {
    piece.contains(p) || piece.distance_to_boundary(p) < 1e-7
}

/// Where `p` enters the area: itself when inside a piece, else the nearest point on a
/// piece's edge within `reach`; with the index of that piece.
fn enter(p: Point, pieces: &[Polygon], reach: f64) -> Option<(Point, usize)> {
    if let Some(i) = pieces.iter().position(|q| q.contains(p)) {
        return Some((p, i));
    }
    let mut best: Option<(f64, Point, usize)> = None;
    for (i, piece) in pieces.iter().enumerate() {
        for ring in piece.rings() {
            for k in 0..ring.len() {
                let (q, _) = polyline::closest_on_segment(p, ring[k], ring[(k + 1) % ring.len()]);
                let d = q.distance(p);
                if d <= reach && best.is_none_or(|b| d < b.0) {
                    best = Some((d, q, i));
                }
            }
        }
    }
    best.map(|(_, q, i)| (q, i))
}

/// Whether the segment `u`–`v` stays inside `piece` (its edge counts as inside).
fn visible(u: Point, v: Point, piece: &Polygon) -> bool {
    if u.distance(v) < 1e-9 {
        return true;
    }
    // corners the segment touches split it; between them it is wholly in or out
    let mut cuts = vec![0.0, 1.0];
    for ring in piece.rings() {
        for k in 0..ring.len() {
            let (p, q) = (ring[k], ring[(k + 1) % ring.len()]);
            if let Some((t, s)) = polyline::segment_intersection(u, v, p, q)
                && t > EPS
                && t < 1.0 - EPS
                && s > EPS
                && s < 1.0 - EPS
            {
                return false;
            }
            let (c, t) = polyline::closest_on_segment(p, u, v);
            if c.distance(p) < 1e-7 && t > EPS && t < 1.0 - EPS {
                cuts.push(t);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.windows(2).filter(|w| w[1] - w[0] > EPS).all(|w| inside(piece, u.lerp(v, (w[0] + w[1]) / 2.0)))
}

/// The shortest way from `a` to `b` inside `piece` (both in it or on its edge): A* over
/// the corners that bend into the free space, which are the only places a shortest
/// way turns.
fn shortest(a: Point, b: Point, piece: &Polygon) -> Option<Vec<Point>> {
    if visible(a, b, piece) {
        return Some(vec![a, b]);
    }
    let mut nodes = vec![a, b];
    for ring in piece.rings() {
        let n = ring.len();
        for k in 0..n {
            let (h, p, j) = (ring[(k + n - 1) % n], ring[k], ring[(k + 1) % n]);
            // the free space lies left of every ring (outer counter-clockwise, holes
            // clockwise): a right turn is a corner the way can bend round
            if (p - h).cross(j - p) < 0.0 {
                nodes.push(p);
            }
        }
    }
    let n = nodes.len();
    let mut g = vec![f64::INFINITY; n];
    let mut prev = vec![usize::MAX; n];
    let mut closed = vec![false; n];
    g[0] = 0.0;
    loop {
        let f = |i: usize| g[i] + nodes[i].distance(b);
        let u = (0..n).filter(|&i| !closed[i] && g[i].is_finite()).min_by(|&i, &j| f(i).total_cmp(&f(j)))?;
        if u == 1 {
            break;
        }
        closed[u] = true;
        for v in 0..n {
            if closed[v] || v == u {
                continue;
            }
            let d = g[u] + nodes[u].distance(nodes[v]);
            if d < g[v] && visible(nodes[u], nodes[v], piece) {
                g[v] = d;
                prev[v] = u;
            }
        }
    }
    let mut path = vec![b];
    let mut k = 1;
    while prev[k] != usize::MAX {
        k = prev[k];
        path.push(nodes[k]);
    }
    path.reverse();
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon {
        Polygon::from_ring(vec![Point::new(x0, y0), Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)])
    }

    fn covered_by(route: &[Point], from: Point, cover: &[Polygon]) -> bool {
        // every sewn leg stays under the cover, but for the ends' slack
        let mut at = from;
        route.iter().all(|&p| {
            let ok = straight(at, p, cover, 1.0 + INSET);
            at = p;
            ok
        })
    }

    #[test]
    fn straight_when_covered_else_round_the_corners() {
        // an L: from the end of one arm to the end of the other
        let l = vec![rect(0.0, 0.0, 10.0, 50.0), rect(0.0, 40.0, 50.0, 50.0)];
        let cover = poly::union(&l);
        let (a, b) = (Point::new(5.0, 2.0), Point::new(48.0, 45.0));
        assert!(!straight(a, b, &cover, 1.0));
        let r = route(a, b, &cover, 1.0).unwrap();
        assert_eq!(*r.last().unwrap(), b);
        assert!(covered_by(&r, a, &cover), "{r:?}");
        // one bend, at the inner corner (10, 40), kept inside by the inset
        assert_eq!(r.len(), 2, "{r:?}");
        assert!(r[0].distance(Point::new(10.0 - INSET, 40.0 + INSET)) < 0.6, "{r:?}");
        let len: f64 = std::iter::once(a).chain(r.iter().copied()).collect::<Vec<_>>().windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!(len < 39.0 + 38.5 + 1.0, "{len}");
        // a straight way needs no corners
        assert_eq!(route(Point::new(5.0, 2.0), Point::new(5.0, 45.0), &cover, 1.0).unwrap(), vec![Point::new(5.0, 45.0)]);
    }

    #[test]
    fn ends_just_outside_and_no_way_across_a_gap() {
        let cover = vec![rect(0.0, 0.0, 30.0, 10.0)];
        // both ends 0.8 mm outside the cover: still routed, through it
        let (a, b) = (Point::new(-0.8, 5.0), Point::new(30.8, 5.0));
        assert_eq!(route(a, b, &cover, 1.0).unwrap(), vec![b]);
        // two pieces: no covered way from one to the other
        let apart = vec![rect(0.0, 0.0, 10.0, 10.0), rect(20.0, 0.0, 30.0, 10.0)];
        assert!(route(Point::new(5.0, 5.0), Point::new(25.0, 5.0), &apart, 1.0).is_none());
        // too far outside to start under it
        assert!(route(Point::new(-5.0, 5.0), Point::new(25.0, 5.0), &cover, 1.0).is_none());
    }

    #[test]
    fn round_a_hole() {
        // a ring: from left of the hole to right of it, round the hole
        let cover = vec![Polygon::new(
            vec![Point::new(0.0, 0.0), Point::new(40.0, 0.0), Point::new(40.0, 40.0), Point::new(0.0, 40.0)],
            vec![vec![Point::new(10.0, 10.0), Point::new(10.0, 30.0), Point::new(30.0, 30.0), Point::new(30.0, 10.0)]],
        )];
        let (a, b) = (Point::new(5.0, 20.0), Point::new(35.0, 20.0));
        let r = route(a, b, &cover, 1.0).unwrap();
        assert!(covered_by(&r, a, &cover), "{r:?}");
        assert_eq!(r.len(), 3, "two corners of the hole, then the end: {r:?}");
        assert!(r.iter().all(|p| !(p.x > 10.0 && p.x < 30.0 && p.y > 10.0 && p.y < 30.0)), "{r:?}");
    }
}
