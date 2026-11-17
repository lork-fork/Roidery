//! Sewing order: which objects must stay in order (they overlap), and an order
//! that sews them with few colour changes and short travel.

use crate::geom::{BBox, Point, Polygon, poly};

/// An object to place in the sewing order.
#[derive(Clone, Debug)]
pub struct Item {
    /// Thread colour (palette index or packed RGB; only equality matters).
    pub color: usize,
    /// What the object covers (empty: a run, which covers nothing).
    pub area: Vec<Polygon>,
    pub bbox: BBox,
}

/// Pairs `(a, b)` with `a < b` whose objects overlap, so `a` must stay before `b`
/// (the later one sews on top). Runs and other lines overlap what they cross.
pub fn overlaps(items: &[Item]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for b in 1..items.len() {
        for a in 0..b {
            let (ia, ib) = (&items[a], &items[b]);
            if !ia.bbox.intersects(&ib.bbox) {
                continue;
            }
            let touch = match (ia.area.is_empty(), ib.area.is_empty()) {
                // two lines: their boxes overlapping is enough to keep them in order
                (true, true) => true,
                (true, false) | (false, true) => true,
                (false, false) => poly::intersection(&ia.area, &ib.area).iter().any(|p| p.area() > 0.01),
            };
            if touch {
                out.push((a, b));
            }
        }
    }
    out
}

/// A sewing order for `items` (indices into it) that keeps every overlapping pair in
/// its original order, changes colour as rarely as it can and then travels as little
/// as it can, starting near `start`. Greedy: at each step it takes, among the
/// objects whose overlapping predecessors are all sewn, one in the current colour if
/// there is any, the nearest such; a colour change goes to the colour with the most
/// objects ready to sew.
pub fn sequence(items: &[Item], start: Option<Point>) -> Vec<usize> {
    let n = items.len();
    let pairs = overlaps(items);
    let mut before: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (a, b) in pairs {
        before[b].push(a);
    }
    let mut done = vec![false; n];
    let mut order = Vec::with_capacity(n);
    let mut here = start;
    let mut color: Option<usize> = None;
    while order.len() < n {
        let ready: Vec<usize> = (0..n).filter(|&i| !done[i] && before[i].iter().all(|&a| done[a])).collect();
        if ready.is_empty() {
            // cannot happen (pairs only point forwards), but never loop forever
            break;
        }
        let same: Vec<usize> = ready.iter().copied().filter(|&i| Some(items[i].color) == color).collect();
        let pool = if same.is_empty() {
            // the colour with the most objects ready, first come first served on ties
            let mut best: Option<(usize, usize)> = None;
            for &i in &ready {
                let c = items[i].color;
                let count = ready.iter().filter(|&&j| items[j].color == c).count();
                if best.is_none_or(|b| count > b.1) {
                    best = Some((c, count));
                }
            }
            let c = best.map(|b| b.0);
            ready.iter().copied().filter(|&i| Some(items[i].color) == c).collect()
        } else {
            same
        };
        let dist = |i: usize| here.map_or(0.0, |p| bbox_distance(&items[i].bbox, p));
        let Some(&next) = pool.iter().min_by(|&&x, &&y| dist(x).total_cmp(&dist(y)).then(x.cmp(&y))) else { break };
        done[next] = true;
        order.push(next);
        color = Some(items[next].color);
        here = Some(items[next].bbox.center());
    }
    // anything left (only if the greedy loop stopped early) keeps its place at the end
    order.extend((0..n).filter(|i| !done[*i]));
    order
}

/// Distance from `p` to the nearest point of a box (0 inside it).
fn bbox_distance(b: &BBox, p: Point) -> f64 {
    let dx = (b.min.x - p.x).max(p.x - b.max.x).max(0.0);
    let dy = (b.min.y - p.y).max(p.y - b.max.y).max(0.0);
    dx.hypot(dy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64, color: usize) -> Item {
        let ring = vec![Point::new(x, y), Point::new(x + s, y), Point::new(x + s, y + s), Point::new(x, y + s)];
        let p = Polygon::from_ring(ring);
        Item { color, bbox: p.bbox(), area: vec![p] }
    }

    #[test]
    fn colours_group_and_overlaps_keep_their_order() {
        // red, blue, red, blue far apart: two colour changes would do instead of three
        let items = vec![square(0.0, 0.0, 10.0, 1), square(20.0, 0.0, 10.0, 2), square(40.0, 0.0, 10.0, 1), square(60.0, 0.0, 10.0, 2)];
        let order = sequence(&items, None);
        let changes = order.windows(2).filter(|w| items[w[0]].color != items[w[1]].color).count();
        assert_eq!(changes, 1, "{order:?}");
        // but an object on top of another stays after it, colour or not
        let stacked = vec![square(0.0, 0.0, 10.0, 1), square(5.0, 5.0, 10.0, 2), square(8.0, 8.0, 10.0, 1)];
        let order = sequence(&stacked, None);
        assert_eq!(order, vec![0, 1, 2]);
        // and nearer objects come first
        let row = vec![square(100.0, 0.0, 5.0, 1), square(0.0, 0.0, 5.0, 1), square(50.0, 0.0, 5.0, 1)];
        assert_eq!(sequence(&row, Some(Point::new(0.0, 0.0))), vec![1, 2, 0]);
    }
}
