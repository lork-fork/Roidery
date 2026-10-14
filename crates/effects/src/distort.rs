//! Distort & Transform effects.

use serde_json::Value;
use vectorcraft_geom::{Affine, Anchor, ParamCurve, PathData, Point, Rect, SubPath, Vec2};

use crate::util::*;

fn diag(b: Rect) -> f64 {
    b.width().hypot(b.height()).max(1e-6)
}

/// Free Distort: bilinear map of the bounding box onto four new corners (unit box coordinates).
pub fn free_distort(path: &PathData, b: Rect, p: &Value) -> PathData {
    let mut c = [Point::new(0.0, 0.0), Point::new(1.0, 0.0), Point::new(1.0, 1.0), Point::new(0.0, 1.0)];
    if let Some(arr) = p.get("corners").and_then(Value::as_array) {
        for (i, v) in arr.iter().take(4).enumerate() {
            if let Some(a) = v.as_array()
                && let (Some(x), Some(y)) = (a.first().and_then(Value::as_f64), a.get(1).and_then(Value::as_f64))
                && x.is_finite()
                && y.is_finite()
            {
                c[i] = Point::new(x, y);
            }
        }
    }
    let (w, h) = (b.width().max(1e-9), b.height().max(1e-9));
    let to_doc = |q: Point| Point::new(b.x0 + q.x * w, b.y0 + q.y * h);
    let c = c.map(to_doc);
    let f = |q: Point| {
        let u = (q.x - b.x0) / w;
        let v = (q.y - b.y0) / h;
        let top = c[0].lerp(c[1], u);
        let bot = c[3].lerp(c[2], u);
        top.lerp(bot, v)
    };
    map_nonlinear(path, diag(b) / 16.0, f)
}

/// Pucker & Bloat: anchors move toward (bloat) or away from (pucker) the centre, handles the
/// opposite way.
pub fn pucker_bloat(path: &PathData, b: Rect, amount: f64) -> PathData {
    let a = amount.clamp(-200.0, 200.0) / 100.0;
    if a == 0.0 {
        return path.clone();
    }
    let c = b.center();
    // Lines have retracted handles: give them handles at 1/3 before distorting so they bulge.
    {
        let mut res = path.clone();
        for (si, sp) in res.subpaths.iter_mut().enumerate() {
            let n = sp.anchors.len();
            let segs = sp.segment_count();
            let orig = &path.subpaths[si];
            for i in 0..segs {
                let cub = seg_cubic(orig, i);
                let j = (i + 1) % n;
                sp.anchors[i].h_out = cub.p1;
                sp.anchors[j].h_in = cub.p2;
            }
            for an in &mut sp.anchors {
                let p = an.p;
                *an = Anchor::with_handles(p.lerp(c, a * 0.5), an.h_in.lerp(c, -a * 0.5), an.h_out.lerp(c, -a * 0.5));
            }
        }
        res
    }
}

/// Roughen: resample at `detail` points per inch and jitter each point by up to `size`.
pub fn roughen(path: &PathData, b: Rect, p: &Value) -> PathData {
    let size = num(p, "size", 5.0).clamp(0.0, 1e4);
    let amp = if flag(p, "relative", true) { size / 100.0 * mean_size(b) } else { size };
    let detail = num(p, "detail", 10.0).clamp(0.0, 100.0);
    let spacing = if detail > 0.0 { 72.0 / detail } else { f64::INFINITY };
    let smooth = smooth_points(p);
    let seed = seed(p);
    let mut subs = vec![];
    for (si, sp) in path.subpaths.iter().enumerate() {
        let mut pts = vec![];
        for i in 0..sp.segment_count() {
            let c = sp.segment(i);
            let len = vectorcraft_geom::kurbo::ParamCurveArclen::arclen(&c, 0.1);
            let k = if spacing.is_finite() { ((len / spacing).ceil() as usize).clamp(1, 2000) } else { 1 };
            for j in 0..k {
                pts.push(c.eval(j as f64 / k as f64));
            }
        }
        if !sp.closed
            && let Some(l) = sp.anchors.last()
        {
            pts.push(l.p);
        }
        if pts.is_empty() {
            pts.extend(sp.anchors.iter().map(|a| a.p));
        }
        for (k, q) in pts.iter_mut().enumerate() {
            q.x += amp * noise(seed, si as u64, k as u64, 1);
            q.y += amp * noise(seed, si as u64, k as u64, 2);
        }
        subs.push(if smooth { catmull_rom(&pts, sp.closed, 1.0) } else { SubPath::polyline(&pts, sp.closed) });
    }
    PathData::new(subs)
}

/// Transform effect (scale, move, rotate, reflect about the centre, plus copies).
pub fn transform(path: &PathData, b: Rect, p: &Value) -> PathData {
    let sx = num(p, "scaleH", 100.0) / 100.0;
    let sy = num(p, "scaleV", 100.0) / 100.0;
    let (rx, ry) = (if flag(p, "reflectX", false) { -1.0 } else { 1.0 }, if flag(p, "reflectY", false) { -1.0 } else { 1.0 });
    let c = b.center().to_vec2();
    // Reflect X mirrors across the horizontal axis (flips x coordinates in Illustrator's dialog).
    let m = Affine::translate(Vec2::new(num(p, "moveH", 0.0), num(p, "moveV", 0.0)) + c)
        * Affine::rotate(-num(p, "rotate", 0.0).to_radians())
        * Affine::scale_non_uniform(sx * rx, sy * ry)
        * Affine::translate(-c);
    let copies = num(p, "copies", 0.0).clamp(0.0, 1000.0) as usize;
    if copies == 0 {
        return path.transformed(m);
    }
    let mut out = path.clone();
    let mut cur = path.clone();
    for _ in 0..copies {
        cur = cur.transformed(m);
        out.subpaths.extend(cur.subpaths.iter().cloned());
    }
    out
}

/// Tweak: random displacement of anchors and/or control points.
pub fn tweak(path: &PathData, b: Rect, p: &Value) -> PathData {
    let rel = flag(p, "relative", true);
    let h = num(p, "h", 10.0);
    let v = num(p, "v", 10.0);
    let (ah, av) = if rel { (h / 100.0 * b.width().abs(), v / 100.0 * b.height().abs()) } else { (h, v) };
    let (anchors, ins, outs) = (flag(p, "anchors", true), flag(p, "in", true), flag(p, "out", true));
    let seed = seed(p);
    let mut out = path.clone();
    for (si, sp) in out.subpaths.iter_mut().enumerate() {
        for (ai, an) in sp.anchors.iter_mut().enumerate() {
            let key = ((si as u64) << 32) | ai as u64;
            let d = |c: u64| Vec2::new(ah * noise(seed, key, c, 1), av * noise(seed, key, c, 2));
            let (mut pp, mut hi, mut ho) = (an.p, an.h_in, an.h_out);
            if anchors {
                let dp = d(0);
                pp += dp;
                hi += dp;
                ho += dp;
            }
            if ins {
                hi += d(1);
            }
            if outs {
                ho += d(2);
            }
            *an = Anchor::with_handles(pp, hi, ho);
        }
    }
    out
}

/// Twist: rotation that is strongest at the centre and fades to zero at the bounding circle.
pub fn twist(path: &PathData, b: Rect, angle: f64) -> PathData {
    let ang = angle.clamp(-3600.0, 3600.0).to_radians();
    if ang == 0.0 {
        return path.clone();
    }
    let c = b.center();
    let r = diag(b) / 2.0;
    let f = move |q: Point| {
        let d = q - c;
        let t = (1.0 - d.hypot() / r).max(0.0);
        let a = -ang * t;
        let (s, co) = a.sin_cos();
        c + Vec2::new(d.x * co - d.y * s, d.x * s + d.y * co)
    };
    let pieces = (r / 8.0).min(r / (1.0 + ang.abs() * 2.0));
    map_nonlinear(path, pieces, f)
}

/// Zig Zag: `ridges` extra points per segment, alternately displaced along the normal.
pub fn zig_zag(path: &PathData, b: Rect, p: &Value) -> PathData {
    let size = num(p, "size", 10.0).clamp(-1e4, 1e4);
    let amp = if flag(p, "relative", false) { size / 100.0 * mean_size(b) } else { size };
    let ridges = num(p, "ridges", 4.0).clamp(0.0, 100.0) as usize;
    let smooth = smooth_points(p);
    let mut subs = vec![];
    for sp in &path.subpaths {
        let segs = sp.segment_count();
        let mut pts = vec![];
        let mut k = 0usize;
        for i in 0..segs {
            let c = sp.segment(i);
            for j in 0..=ridges {
                let t = j as f64 / (ridges + 1) as f64;
                let nrm = if j == 0 {
                    // Average the normals of the segments meeting at the anchor.
                    let prev = if i > 0 {
                        Some(sp.segment(i - 1))
                    } else if sp.closed {
                        Some(sp.segment(segs - 1))
                    } else {
                        None
                    };
                    let n1 = normal_at(&c, 0.0);
                    match prev {
                        Some(pc) => {
                            let s = n1 + normal_at(&pc, 1.0);
                            if s.hypot() > 1e-9 { s / s.hypot() } else { n1 }
                        }
                        None => n1,
                    }
                } else {
                    normal_at(&c, t)
                };
                let sign = if k.is_multiple_of(2) { 1.0 } else { -1.0 };
                pts.push(c.eval(t) + nrm * (amp * sign));
                k += 1;
            }
        }
        if !sp.closed {
            if segs > 0 {
                let c = sp.segment(segs - 1);
                let sign = if k.is_multiple_of(2) { 1.0 } else { -1.0 };
                pts.push(c.p3 + normal_at(&c, 1.0) * (amp * sign));
            } else {
                pts.extend(sp.anchors.iter().map(|a| a.p));
            }
        }
        subs.push(if smooth { catmull_rom(&pts, sp.closed, 1.0) } else { SubPath::polyline(&pts, sp.closed) });
    }
    PathData::new(subs)
}
