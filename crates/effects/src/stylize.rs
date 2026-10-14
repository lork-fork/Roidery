//! Convert to Shape, Round Corners and Scribble.

use serde_json::Value;
use vectorcraft_geom::kurbo;
use vectorcraft_geom::shapes::{self, KAPPA};
use vectorcraft_geom::{Affine, Anchor, PathData, Point, Rect, SubPath};

use crate::util::*;

/// Convert to Shape (rectangle / rounded rectangle / ellipse) around the bounds.
pub fn convert_to_shape(id: &str, b: Rect, p: &Value) -> PathData {
    let r = if flag(p, "relative", true) {
        let (ew, eh) = (num(p, "extraW", 18.0), num(p, "extraH", 18.0));
        let r = b.inflate(ew / 2.0, eh / 2.0);
        Rect::new(r.x0.min(b.center().x), r.y0.min(b.center().y), r.x1.max(b.center().x), r.y1.max(b.center().y))
    } else {
        let (w, h) = (num(p, "width", 100.0).abs(), num(p, "height", 100.0).abs());
        Rect::from_center_size(b.center(), (w, h))
    };
    match id {
        "convertToShape.ellipse" => shapes::ellipse(r),
        "convertToShape.roundedRectangle" => shapes::rounded_rectangle(r, num(p, "radius", 9.0)),
        _ => shapes::rectangle(r),
    }
}

/// Round Corners: every sharp, handle-less corner becomes a circular-ish arc of `radius`.
pub fn round_corners(path: &PathData, radius: f64) -> PathData {
    if radius <= 0.0 {
        return path.clone();
    }
    let mut subs = vec![];
    for sp in &path.subpaths {
        let n = sp.anchors.len();
        if n < 3 && (n != 2 || sp.closed) {
            subs.push(sp.clone());
            continue;
        }
        let mut out: Vec<Anchor> = vec![];
        for i in 0..n {
            let a = sp.anchors[i];
            let interior = sp.closed || (i > 0 && i + 1 < n);
            if !interior || a.has_in() || a.has_out() {
                out.push(a);
                continue;
            }
            let prev = &sp.anchors[(i + n - 1) % n];
            let next = &sp.anchors[(i + 1) % n];
            // Tangent directions toward the neighbours (their handles if the segments curve).
            let tp = if prev.has_out() { prev.h_out } else { prev.p };
            let tn = if next.has_in() { next.h_in } else { next.p };
            let (vp, vn) = (tp - a.p, tn - a.p);
            let (lp, ln) = (vp.hypot(), vn.hypot());
            if lp < 1e-9 || ln < 1e-9 {
                out.push(a);
                continue;
            }
            let (up, un) = (vp / lp, vn / ln);
            // Nearly straight: nothing to round.
            if up.dot(un) < -0.9999 {
                out.push(a);
                continue;
            }
            let d = radius.min(prev.p.distance(a.p) / 2.0).min(next.p.distance(a.p) / 2.0);
            let pa = a.p + up * d;
            let pb = a.p + un * d;
            let k = d * KAPPA;
            out.push(Anchor::with_handles(pa, pa, pa - up * k));
            out.push(Anchor::with_handles(pb, pb - un * k, pb));
        }
        subs.push(SubPath::new(out, sp.closed));
    }
    PathData::new(subs)
}

fn flatten(path: &PathData) -> Vec<Vec<Point>> {
    let mut polys: Vec<Vec<Point>> = vec![];
    kurbo::flatten(path.to_bezpath(), 0.25, |el| match el {
        kurbo::PathEl::MoveTo(p) => polys.push(vec![p]),
        kurbo::PathEl::LineTo(p) => {
            if let Some(l) = polys.last_mut() {
                l.push(p)
            }
        }
        kurbo::PathEl::ClosePath => {
            if let Some(l) = polys.last_mut()
                && let Some(&f) = l.first()
            {
                l.push(f)
            }
        }
        _ => {}
    });
    polys
}

/// Whether segment `p`–`q` properly crosses an edge of `polys`: its own ends, which lie on the
/// outline, don't count, nor does running along an edge.
fn crosses_outline(p: Point, q: Point, polys: &[Vec<Point>]) -> bool {
    let (p, q) = (p.lerp(q, 1e-3), q.lerp(p, 1e-3));
    let side = |a: Point, b: Point, c: Point| (b - a).cross(c - a);
    // Strictly on opposite sides, beyond rounding noise (the sides are areas: length × length).
    let apart = |s1: f64, s2: f64, eps: f64| (s1 > eps && s2 < -eps) || (s1 < -eps && s2 > eps);
    let pq = (q - p).hypot();
    polys.iter().any(|pl| {
        pl.windows(2).any(|w| {
            let (a, b) = (w[0], w[1]);
            let eps = 1e-9 * pq * (b - a).hypot();
            apart(side(p, q, a), side(p, q, b), eps) && apart(side(a, b, p), side(a, b, q), eps)
        })
    })
}

/// Scribble (simplified): a zig-zag hatching across the filled area at `angle`, one per region
/// of the shape, outlined to `strokeWidth` so the fill paints it.
pub fn scribble(path: &PathData, b: Rect, p: &Value) -> PathData {
    let angle = num(p, "angle", 30.0).to_radians();
    let overlap = num(p, "overlap", 0.0);
    let width = num(p, "strokeWidth", 3.0).clamp(0.01, 1000.0);
    let curvy = num(p, "curviness", 5.0).clamp(0.0, 100.0) / 100.0;
    let spacing = num(p, "spacing", 5.0).max(0.1);
    let variation = num(p, "variation", 0.5).clamp(0.0, 100.0);
    let seed = seed(p);
    let c = b.center();
    let to_local = Affine::translate(c.to_vec2()) * Affine::rotate(angle) * Affine::translate(-c.to_vec2());
    let from_local = to_local.inverse();
    let polys: Vec<Vec<Point>> = flatten(path).into_iter().map(|pl| pl.into_iter().map(|q| from_local * q).collect()).collect();
    let (mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY);
    for q in polys.iter().flatten() {
        y0 = y0.min(q.y);
        y1 = y1.max(q.y);
    }
    if !y0.is_finite() || y1 - y0 < 1e-9 {
        return PathData::default();
    }
    let step = spacing.max((y1 - y0) / 2000.0);
    // One zig-zag per region (#1648): a span continues a chain of the previous row as long as the
    // line from that row's end to this one's start stays inside the shape; otherwise it starts a
    // chain of its own. So no line crosses a hole or a notch. In a convex shape that line always
    // stays inside: one chain, as before.
    struct Chain {
        pts: Vec<Point>,
        end: Point,
        row: u64,
    }
    let mut chains: Vec<Chain> = vec![];
    let mut y = y0 + step / 2.0;
    let mut row = 0u64;
    while y < y1 {
        let mut xs: Vec<f64> = vec![];
        for pl in &polys {
            for w in pl.windows(2) {
                let (a, bq) = (w[0], w[1]);
                if (a.y <= y && bq.y > y) || (bq.y <= y && a.y > y) {
                    xs.push(a.x + (y - a.y) / (bq.y - a.y) * (bq.x - a.x));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        let spans: Vec<(f64, f64)> = xs.as_chunks::<2>().0.iter().map(|s| (s[0] - overlap, s[1] + overlap)).collect();
        let open = chains.len();
        for (a, bx) in spans {
            let jitter = |k: u64| variation * noise(seed, row, k, 3);
            let (l, r) = (Point::new(a + jitter(0), y + jitter(1)), Point::new(bx + jitter(2), y + jitter(3)));
            let even = row.is_multiple_of(2);
            let pair = if even { [l, r] } else { [r, l] };
            // Un-jittered ends of the span in drawing order.
            let (start, end) = if even { (Point::new(a, y), Point::new(bx, y)) } else { (Point::new(bx, y), Point::new(a, y)) };
            // The nearest chain of the previous row that this span can join without leaving
            // the shape.
            let prev = row.checked_sub(1);
            let pick = (0..open)
                .filter(|&k| Some(chains[k].row) == prev && !crosses_outline(chains[k].end, start, &polys))
                .min_by(|&i, &j| (chains[i].end - start).hypot().total_cmp(&(chains[j].end - start).hypot()));
            match pick {
                Some(k) => {
                    let c = &mut chains[k];
                    c.pts.extend(pair);
                    (c.end, c.row) = (end, row);
                }
                None => chains.push(Chain { pts: pair.to_vec(), end, row }),
            }
        }
        row += 1;
        y += step;
    }
    let lines: Vec<SubPath> = chains
        .into_iter()
        .filter(|c| c.pts.len() >= 2)
        .map(|c| if curvy > 0.0 { catmull_rom(&c.pts, false, curvy * 2.0) } else { SubPath::polyline(&c.pts, false) })
        .collect();
    if lines.is_empty() {
        return PathData::default();
    }
    let mut center = PathData::new(lines);
    center.transform(to_local);
    let stroke = kurbo::Stroke::new(width).with_join(kurbo::Join::Round).with_caps(kurbo::Cap::Round);
    let outline = kurbo::stroke(center.to_bezpath(), &stroke, &kurbo::StrokeOpts::default(), 0.05);
    PathData::from_bezpath(&outline)
}
