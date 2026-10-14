//! Loop → Bézier curves, and the pieces of that which centre lines use too.

use vectorcraft_geom::{Anchor, AnchorKind, Point, SubPath};

use crate::contour::Loop;
use crate::logo::{Cubic, Stats, Style, all_within, fit_open, fit_outline, resample_closed, resample_open, sample_cubics};
use crate::subpixel::{self, Crack};

fn seg_dist(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let l2 = ab.hypot2();
    if l2 < 1e-18 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Douglas–Peucker on `pts[a..=b]`, marking kept indices.
fn rdp(pts: &[Point], a: usize, b: usize, tol: f64, keep: &mut [bool]) {
    let mut stack = vec![(a, b)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (pa, pb) = (pts[a], pts[b % pts.len()]);
        let mut best = (0.0, a);
        for (i, &q) in pts.iter().enumerate().take(b).skip(a + 1) {
            let d = seg_dist(q, pa, pb);
            if d > best.0 {
                best = (d, i);
            }
        }
        if best.0 > tol {
            keep[best.1] = true;
            stack.push((a, best.1));
            stack.push((best.1, b));
        }
    }
}

fn reversed(mut segs: Vec<Cubic>) -> Vec<Cubic> {
    segs.reverse();
    for c in &mut segs {
        c.reverse();
    }
    segs
}

fn before(a: Point, b: Point) -> std::cmp::Ordering {
    a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y))
}

/// Lines within this of level or upright are made exactly so (the tangent of 2.5°).
const NEARLY_UPRIGHT: f64 = 0.0437;

/// The farthest (px) levelling a line may move either of its ends.
const LEVEL_REACH: f64 = 1.6;

/// Snap Curves To Lines: the straight pieces of `segs` (a chain; `closed` when its end joins its
/// start) that are within 2.5° of level or upright and at least 2 px long are made exactly level
/// or upright (unless that moves an end by more than [`LEVEL_REACH`]), by moving their ends
/// across them: both to their mean, or, in an open chain, the
/// free end to the end that is fixed (with `fixed_ends`, a chain's own ends stay where they are:
/// they are junctions other regions share). The pieces next to a moved end follow it.
fn snap_upright(segs: &mut [Cubic], closed: bool, fixed_ends: bool) {
    let n = segs.len();
    let straight = |c: &Cubic| seg_dist(c[1], c[0], c[3]) < 1e-6 && seg_dist(c[2], c[0], c[3]) < 1e-6;
    let lines: Vec<bool> = segs.iter().map(straight).collect();
    // Anchor `i` starts piece `i`; an open chain has one more, at its end.
    let fixed = |i: usize| !closed && fixed_ends && (i == 0 || i == n);
    let at = |segs: &[Cubic], i: usize| if i < n { segs[i][0] } else { segs[n - 1][3] };
    let put = |segs: &mut [Cubic], i: usize, p: Point| {
        let i = if closed { i % n } else { i };
        let d = p - at(segs, i);
        if i < n {
            segs[i][0] = p;
            segs[i][1] += d;
        }
        if let Some(before) = if i > 0 { Some(i - 1) } else { closed.then_some(n - 1) } {
            segs[before][3] = p;
            segs[before][2] += d;
        }
    };
    for (i, &line) in lines.iter().enumerate() {
        let (ia, ib) = (i, if closed { (i + 1) % n } else { i + 1 });
        let (a, b) = (at(segs, ia), at(segs, ib));
        let d = b - a;
        if !line || d.hypot() < 2.0 || ia == ib {
            continue;
        }
        let level = d.y.abs() <= NEARLY_UPRIGHT * d.x.abs();
        if !level && d.x.abs() > NEARLY_UPRIGHT * d.y.abs() {
            continue;
        }
        let pick = |p: Point| if level { p.y } else { p.x };
        let to = match (fixed(ia), fixed(ib)) {
            (true, true) => continue,
            (true, false) => pick(a),
            (false, true) => pick(b),
            (false, false) => (pick(a) + pick(b)) / 2.0,
        };
        // Not when an end would go farther than a pixel and a half or so from where the image has it: a long line a
        // degree or two off is that line, not a level one drawn badly.
        if (pick(a) - to).abs().max((pick(b) - to).abs()) > LEVEL_REACH {
            continue;
        }
        let moved = |p: Point| if level { Point::new(p.x, to) } else { Point::new(to, p.y) };
        if !fixed(ia) {
            put(segs, ia, moved(a));
        }
        if !fixed(ib) {
            put(segs, ib, moved(b));
        }
    }
    // A straight piece has its handles on it.
    for (c, _) in segs.iter_mut().zip(&lines).filter(|(_, line)| **line) {
        *c = [c[0], c[0] + (c[3] - c[0]) / 3.0, c[0] + (c[3] - c[0]) * (2.0 / 3.0), c[3]];
    }
}

/// A whole loop fitted. The same loop gives the same curves whichever way it runs and wherever it
/// starts (it is fitted clockwise from its leftmost point): the outline of a shape and the hole it
/// leaves in the shape around it are one curve.
pub(crate) fn fit_closed(pts: &[Point], style: &Style) -> Option<Vec<Cubic>> {
    let n = pts.len();
    let area2: f64 = (0..n).map(|i| pts[i].x * pts[(i + 1) % n].y - pts[(i + 1) % n].x * pts[i].y).sum();
    let flip = area2 < 0.0;
    let mut pts = pts.to_vec();
    if flip {
        pts.reverse();
    }
    let start = (0..n).min_by(|&i, &j| before(pts[i], pts[j])).unwrap_or(0);
    pts.rotate_left(start);
    let mut stats = Stats::default();
    let mut segs = fit_outline(&pts, style, &mut stats)?;
    // A fit that strayed was replaced by a plain one that nothing has checked: one that strays too
    // (a wild handle on a tiny or ragged loop) gives way to the outline's own points.
    if stats.reverted > 0 && !all_within(&sample_cubics(&segs, 0.25), &resample_closed(&pts, 0.5).map_or_else(|| pts.clone(), |(p, _)| p), 1.5) {
        segs = polygon(&pts, style.tol);
    }
    if style.upright {
        snap_upright(&mut segs, true, false);
    }
    Some(if flip { reversed(segs) } else { segs })
}

/// A piece of a loop between two junctions fitted, from its first point to its last. The same
/// piece gives the same curve whichever way it runs (it is fitted from its end further left): the
/// edge two regions share is one curve. `fixed_ends`: its ends are junctions and stay put (a centre
/// line's ends are its own).
pub(crate) fn fit_piece(pts: &[Point], style: &Style, fixed_ends: bool) -> Vec<Cubic> {
    let n = pts.len();
    let (Some(&first), Some(&last)) = (pts.first(), pts.last()) else { return vec![] };
    let flip = match before(first, last) {
        std::cmp::Ordering::Equal => n > 3 && before(pts[1], pts[n - 2]).is_gt(),
        order => order.is_gt(),
    };
    let mut pts = pts.to_vec();
    if flip {
        pts.reverse();
    }
    let mut segs = fit_open(&pts, style, &mut Stats::default());
    // Nothing else checks this fit: one that strays gives way to the piece's own points.
    if !all_within(&sample_cubics(&segs, 0.25), &resample_open(&pts, 0.5).map_or_else(|| pts.clone(), |(p, _)| p), 1.5) {
        let mut keep = vec![false; n];
        rdp(&pts, 0, n - 1, style.tol, &mut keep);
        keep[0] = true;
        keep[n - 1] = true;
        let kept: Vec<Point> = (0..n).filter(|&i| keep[i]).map(|i| pts[i]).collect();
        segs = kept.windows(2).filter(|p| p[0] != p[1]).map(|p| [p[0], p[0], p[1], p[1]]).collect();
    }
    if style.upright {
        snap_upright(&mut segs, false, fixed_ends);
    }
    if flip { reversed(segs) } else { segs }
}

/// Fit one loop of a `w × h` image into a closed Bézier subpath (pixel coordinates): its points
/// are placed to a fraction of a pixel ([`subpixel::outline`], which explains `across`) and fitted
/// as Flat Logo mode fits its outlines (corners restored, straight runs as lines, the rest as few
/// curves as `style.tol` allows), piece by piece between its junctions, so that the regions on
/// either side of an edge get the same curve for it.
pub(crate) fn fit_loop(
    l: &Loop,
    w: usize,
    h: usize,
    across: &dyn Fn(Crack) -> (f64, u32),
    junction_at: &dyn Fn(i32, i32) -> Option<Point>,
    style: &Style,
) -> Option<(SubPath, bool)> {
    let pieces = subpixel::outline(l, w, h, across, junction_at);
    let mut segs: Vec<Cubic> = Vec::new();
    for piece in &pieces {
        let mut style = *style;
        if piece.hard {
            // No anti-aliasing: the edge is only known to about half a pixel (as in Flat Logo mode).
            style.tol *= 2.5;
            style.line_tol *= 1.8;
            style.circle_tol *= 1.8;
            style.sigma = 0.9;
        }
        if piece.closed {
            segs = fit_closed(&piece.pts, &style)?;
        } else {
            segs.extend(fit_piece(&piece.pts, &style, true));
        }
    }
    Some((subpath(&segs, true)?, pieces.iter().any(|p| p.hard)))
}

/// A chain of cubics as a subpath: closed (its end joins its start) or open. A straight piece has
/// no handles; an anchor whose two handles line up is smooth.
pub(crate) fn subpath(segs: &[Cubic], closed: bool) -> Option<SubPath> {
    let n = segs.len();
    let straight = |c: &Cubic| seg_dist(c[1], c[0], c[3]) < 1e-6 && seg_dist(c[2], c[0], c[3]) < 1e-6;
    let mut anchors = Vec::with_capacity(n + 1);
    for i in 0..if closed { n } else { n + 1 } {
        // The piece that arrives at this anchor and the one that leaves it.
        let before = if i > 0 { segs.get(i - 1) } else { closed.then(|| segs.last()).flatten() };
        let after = segs.get(i);
        let p = after.map(|c| c[0]).or(before.map(|c| c[3]))?;
        let h_in = before.filter(|c| !straight(c)).map_or(p, |c| c[2]);
        let h_out = after.filter(|c| !straight(c)).map_or(p, |c| c[1]);
        let (vi, vo) = (p - h_in, h_out - p);
        let smooth = vi.hypot() > 1e-9 && vo.hypot() > 1e-9 && vi.cross(vo).abs() < 0.07 * vi.hypot() * vo.hypot() && vi.dot(vo) > 0.0;
        anchors.push(Anchor { p, h_in, h_out, kind: if smooth { AnchorKind::Smooth } else { AnchorKind::Corner } });
    }
    (n >= if closed { 2 } else { 1 }).then(|| SubPath::new(anchors, closed))
}

/// The closed outline `pts` as straight pieces within `tol` of it (Douglas–Peucker).
fn polygon(pts: &[Point], tol: f64) -> Vec<Cubic> {
    let n = pts.len();
    let Some(&first) = pts.first() else { return vec![] };
    let far = (1..n).max_by(|&i, &j| pts[i].distance_squared(first).total_cmp(&pts[j].distance_squared(first))).unwrap_or(0);
    let mut ext = pts.to_vec();
    ext.push(first);
    let mut keep = vec![false; n + 1];
    keep[0] = true;
    keep[far] = true;
    rdp(&ext, 0, far, tol, &mut keep);
    rdp(&ext, far, n, tol, &mut keep);
    let kept: Vec<Point> = (0..n).filter(|&i| keep[i]).map(|i| pts[i]).collect();
    (0..kept.len()).map(|i| [kept[i], kept[i], kept[(i + 1) % kept.len()], kept[(i + 1) % kept.len()]]).collect()
}
