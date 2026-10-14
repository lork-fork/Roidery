//! Auto Grouping: which traced path lies inside which, so that a result can be grouped the way it
//! is nested (a ring with what fills its hole, a face with its eyes).

use crate::TracedPath;
use kurbo::{PathEl, Point, Rect};

/// How far (px) outside an outline a point may lie and still count as inside it: two paths that
/// share an edge are each fitted on their own.
const NEAR: f64 = 0.5;

/// A path's outer contour as a polygon, and its box.
struct Outer {
    pts: Vec<Point>,
    bbox: Rect,
    /// A closed, filled contour: something can lie inside it.
    holds: bool,
    /// Where something can: for each pixel row from `bbox.y0 - NEAR` down, the edges (each from
    /// the point of that index to the next) that pass through it or within [`NEAR`] of it. A
    /// point is tested against the edges of its row, not all of a long outline's.
    rows: Vec<Vec<u32>>,
}

fn outer(tp: &TracedPath) -> Option<Outer> {
    let (mut pts, mut subpaths, mut closed) = (vec![], 0, false);
    kurbo::flatten(tp.path.to_bezpath(), 0.2, |el| match el {
        PathEl::MoveTo(p) => {
            subpaths += 1;
            if subpaths == 1 {
                pts.push(p);
            }
        }
        PathEl::LineTo(p) if subpaths == 1 => pts.push(p),
        PathEl::ClosePath if subpaths == 1 => closed = true,
        _ => {}
    });
    let first = *pts.first()?;
    let bbox = pts.iter().fold(Rect::from_points(first, first), |b, p| b.union_pt(*p));
    if !bbox.is_finite() {
        return None;
    }
    let holds = closed && tp.stroke.is_none();
    // (Traced paths lie in the image: a box taller than any image is nothing to hold things.)
    let holds = holds && bbox.height() < 1e6 && u32::try_from(pts.len()).is_ok();
    let mut rows = vec![];
    if holds {
        rows = vec![vec![]; (bbox.height() + 2.0 * NEAR) as usize + 1];
        for (i, (a, b)) in pts.iter().zip(pts.iter().cycle().skip(1)).enumerate() {
            let (top, bottom) = (a.y.min(b.y) - bbox.y0, a.y.max(b.y) - bbox.y0 + 2.0 * NEAR);
            for row in rows.iter_mut().take(bottom as usize + 1).skip(top.max(0.0) as usize) {
                row.push(i as u32);
            }
        }
    }
    Some(Outer { pts, bbox, holds, rows })
}

/// Whether `p` lies inside the closed outline of `o`, or within [`NEAR`] of its edge.
fn inside_or_near(p: Point, o: &Outer) -> bool {
    let row = p.y - (o.bbox.y0 - NEAR);
    let Some(edges) = (row >= 0.0).then(|| o.rows.get(row as usize)).flatten() else { return false };
    let mut inside = false;
    for &i in edges {
        let (Some(a), Some(b)) = (o.pts.get(i as usize), o.pts.get((i as usize + 1) % o.pts.len().max(1))) else { continue };
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
        let d = *b - *a;
        let t = if d.hypot2() > 0.0 { ((p - *a).dot(d) / d.hypot2()).clamp(0.0, 1.0) } else { 0.0 };
        if (p - (*a + d * t)).hypot2() <= NEAR * NEAR {
            return true;
        }
    }
    inside
}

/// For each of `paths` (bottom-most first), the path it lies inside, if any: the smallest filled
/// path whose outer contour holds all of its own. The path that covers the whole `w` × `h` image
/// (the paper) is nobody's parent: everything would be inside it.
///
/// Drawing each path together with those inside it (in their own order) paints the picture
/// `paths` paints in its own order: where the paths can overlap (`stacked`), a path only gets a parent below it, and only
/// if no path that would then come after it instead of before it shares any of its box.
pub(crate) fn parents(paths: &[TracedPath], w: usize, h: usize, stacked: bool) -> Vec<Option<usize>> {
    let outers: Vec<Option<Outer>> = paths.iter().map(outer).collect();
    let paper = Rect::new(1.0, 1.0, w as f64 - 1.0, h as f64 - 1.0);
    let mut parent: Vec<Option<usize>> = vec![None; paths.len()];
    // How many paths each is inside of.
    let mut depth = vec![0usize; paths.len()];
    // The paths something can lie inside, by the area of their boxes.
    let mut by_area: Vec<usize> = (0..paths.len()).filter(|&p| outers.get(p).and_then(Option::as_ref).is_some_and(|o| o.holds)).collect();
    let area = |p: &usize| outers.get(*p).and_then(Option::as_ref).map_or(0.0, |o| o.bbox.area());
    by_area.sort_by(|a, b| area(a).total_cmp(&area(b)));
    for (x, ox) in outers.iter().enumerate() {
        let Some(ox) = ox else { continue };
        // The paths whose box holds its own, smallest first.
        let mut holders = by_area.iter().filter_map(|&p| Some((p, outers.get(p)?.as_ref()?))).filter(|&(p, op)| {
            let (o, i) = (op.bbox, ox.bbox);
            o.x0 - NEAR <= i.x0
                && o.y0 - NEAR <= i.y0
                && o.x1 + NEAR >= i.x1
                && o.y1 + NEAR >= i.y1
                && o.area() > i.area()
                && !(stacked && p > x)
                && !(o.x0 <= paper.x0 && o.y0 <= paper.y0 && o.x1 >= paper.x1 && o.y1 >= paper.y1)
        });
        // The paths under `x` that share some of its box: it may not come to lie under any of them.
        // (Found only once some path holds its first point: most paths are inside none.)
        let under = std::cell::OnceCell::new();
        let under = || {
            under.get_or_init(|| {
                (0..x)
                    .filter(|&y| outers.get(y).and_then(Option::as_ref).is_some_and(|oy| oy.bbox.intersect(ox.bbox).area() > 0.0))
                    .collect::<Vec<_>>()
            })
        };
        let found = holders.find(|&(p, op)| {
            ox.pts.first().is_some_and(|pt| inside_or_near(*pt, op))
                && (!stacked || under().iter().all(|&y| !comes_after(&parent, &depth, y, p)))
                && ox.pts.iter().all(|pt| inside_or_near(*pt, op))
        });
        if let (Some((p, _)), Some(slot)) = (found, parent.get_mut(x)) {
            *slot = Some(p);
            let deep = depth.get(p).copied().unwrap_or(0) + 1;
            if let Some(d) = depth.get_mut(x) {
                *d = deep;
            }
        }
    }
    parent
}

/// Whether `y` is drawn after `p` and all that is inside `p`: it is in another branch of the
/// nesting, and that branch starts above the one `p` is in.
fn comes_after(parent: &[Option<usize>], depth: &[usize], y: usize, p: usize) -> bool {
    let up = |i: usize| parent.get(i).copied().flatten();
    let deep = |i: usize| depth.get(i).copied().unwrap_or(0);
    let (mut a, mut b) = (y, p);
    while deep(a) > deep(b) {
        let Some(q) = up(a) else { break };
        a = q;
    }
    while deep(b) > deep(a) {
        let Some(q) = up(b) else { break };
        b = q;
    }
    // (Equal: one is the other, or inside it.)
    while a != b && up(a) != up(b) {
        let (Some(qa), Some(qb)) = (up(a), up(b)) else { break };
        (a, b) = (qa, qb);
    }
    a > b
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_geom::PathData;

    fn disc(c: (f64, f64), r: f64, color: u8) -> TracedPath {
        let path = PathData::from_bezpath(&kurbo::Shape::to_path(&kurbo::Circle::new(c, r), 0.01));
        TracedPath { path, color: [color; 3], pixels: 1, stroke: None, shape: None, gradient: None, square_caps: false }
    }

    #[test]
    fn a_path_is_inside_the_smallest_path_that_holds_it() {
        // The paper, a disc, a disc inside it, a dot inside that, and a disc beside them.
        let paths = [
            disc((50.0, 50.0), 80.0, 255),
            disc((30.0, 50.0), 20.0, 0),
            disc((30.0, 50.0), 10.0, 100),
            disc((30.0, 50.0), 3.0, 0),
            disc((75.0, 50.0), 10.0, 0),
        ];
        for stacked in [false, true] {
            assert_eq!(parents(&paths, 100, 100, stacked), [None, None, Some(1), Some(2), None]);
        }
    }

    #[test]
    fn stacked_paths_are_not_reordered_where_they_overlap() {
        // A dot over a disc, with a wider disc between them in the stack that covers the dot's
        // place without holding the first disc: grouped with the first disc, the dot would go under it.
        let paths = [disc((30.0, 50.0), 20.0, 0), disc((60.0, 50.0), 24.0, 100), disc((35.0, 50.0), 3.0, 200)];
        assert_eq!(parents(&paths, 100, 100, true), [None, None, None]);
        // Where paths cannot overlap, either order paints the same.
        assert_eq!(parents(&paths, 100, 100, false), [None, None, Some(0)]);
    }
}
