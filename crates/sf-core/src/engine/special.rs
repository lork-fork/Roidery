//! Special fills: contour, spiral, radial, cross stitch, meander and motif fills.

use super::out::Out;
use super::run::CORNER_ANGLE;
use crate::geom::{Affine, Point, Polygon, poly, polyline};
use crate::model::{CrossStyle, FillParams, MotifWarp, WarpKind};

/// Rings of the polygon offset inwards by `spacing/2 + k·spacing`, grouped by level
/// (outermost first).
fn inset_levels(polygon: &Polygon, spacing: f64, max_levels: usize) -> Vec<Vec<Vec<Point>>> {
    let mut levels = Vec::new();
    let mut d = spacing * 0.5;
    for _ in 0..max_levels {
        let ins = if d < 1e-9 { vec![polygon.clone()] } else { poly::offset(std::slice::from_ref(polygon), -d) };
        if ins.is_empty() {
            break;
        }
        let rings: Vec<Vec<Point>> = ins
            .iter()
            .flat_map(|p| p.rings().cloned().collect::<Vec<_>>())
            .filter(|r| r.len() >= 3 && polyline::ring_length(r) > spacing * 1.5)
            .collect();
        if rings.is_empty() {
            break;
        }
        levels.push(rings);
        d += spacing;
    }
    levels
}

/// Contour fill: concentric rows following the outline, outside in; only the first `rings`
/// when that isn't 0 (an offset fill, a band along the outline).
pub(crate) fn contour(polygon: &Polygon, (spacing, rings): (f64, u32), step: f64, out: &mut Out, hint: Option<Point>, trim: f64) {
    let levels = inset_levels(polygon, spacing.max(0.2), if rings > 0 { (rings as usize).min(2000) } else { 2000 });
    for rings in levels {
        let mut left = rings;
        while !left.is_empty() {
            let here = out.pos().or(hint).unwrap_or(left[0][0]);
            let i = (0..left.len())
                .min_by(|&a, &b| polyline::distance_to_ring(&left[a], here).total_cmp(&polyline::distance_to_ring(&left[b], here)))
                .unwrap_or(0);
            let ring = left.remove(i);
            let start = polyline::ring_starting_near(&ring, here);
            let pts = polyline::resample_ring(&start, step, CORNER_ANGLE);
            if let Some(&p0) = pts.first() {
                out.connect(p0, trim.max(spacing * 3.0), step);
                out.stitches(pts);
            }
        }
    }
}

/// Re-parameterises a closed ring (explicitly closed) by normalised arc length.
fn ring_at(ring: &[Point], cum: &[f64], t: f64) -> Point {
    let total = *cum.last().unwrap_or(&0.0);
    let d = t.rem_euclid(1.0) * total;
    let i = cum.partition_point(|&c| c <= d).clamp(1, ring.len() - 1);
    let seg = cum[i] - cum[i - 1];
    if seg < 1e-12 { ring[i] } else { ring[i - 1].lerp(ring[i], (d - cum[i - 1]) / seg) }
}

/// Spiral fill: one continuous spiral from the outline inwards. Falls back to
/// contour fill for shapes whose insets split or have holes.
pub(crate) fn spiral(polygon: &Polygon, spacing: f64, step: f64, out: &mut Out, hint: Option<Point>, trim: f64) {
    let spacing = spacing.max(0.2);
    let levels = inset_levels(polygon, spacing, 2000);
    if polygon.holes.iter().any(|h| h.len() >= 3) || levels.iter().any(|l| l.len() != 1) || levels.len() < 2 {
        contour(polygon, (spacing, 0), step, out, hint, trim);
        return;
    }
    let here = out.pos().or(hint).unwrap_or(levels[0][0][0]);
    let mut rings: Vec<Vec<Point>> = Vec::new();
    let mut anchor = here;
    for l in &levels {
        let r = polyline::closed(&polyline::ring_starting_near(&l[0], anchor));
        anchor = r[0];
        rings.push(r);
    }
    let cums: Vec<Vec<f64>> = rings.iter().map(|r| polyline::cumulative_lengths(r)).collect();
    let mut pts = Vec::new();
    for k in 0..rings.len() {
        let len = *cums[k].last().unwrap_or(&0.0);
        let n = ((len / step).ceil() as usize).max(8);
        let last = k + 1 == rings.len();
        for i in 0..n {
            let t = i as f64 / n as f64;
            let a = ring_at(&rings[k], &cums[k], t);
            let p = if last { a } else { a.lerp(ring_at(&rings[k + 1], &cums[k + 1], t), t) };
            pts.push(p);
        }
        if last {
            pts.push(ring_at(&rings[k], &cums[k], 0.0));
        }
    }
    if let Some(&p0) = pts.first() {
        out.connect(p0, trim, step);
        out.stitches(pts);
    }
}

/// A point inside the polygon near its centroid.
fn interior_point(polygon: &Polygon) -> Point {
    let c = polygon.centroid();
    if polygon.contains(c) {
        return c;
    }
    // Middle of the widest horizontal span through the centroid's height.
    let y = c.y;
    let mut xs: Vec<f64> = Vec::new();
    for ring in polygon.rings() {
        let n = ring.len();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
            }
        }
    }
    xs.sort_by(f64::total_cmp);
    xs.as_chunks::<2>().0.iter().max_by(|a, b| (a[1] - a[0]).total_cmp(&(b[1] - b[0]))).map(|s| Point::new((s[0] + s[1]) * 0.5, y)).unwrap_or(c)
}

/// Radial fill: satin-like rays between the outline and a centre. Ray lengths
/// alternate so the centre does not get perforated.
pub(crate) fn radial(polygon: &Polygon, center: Option<Point>, spacing: f64, split: f64, out: &mut Out, trim: f64) {
    let c = center.filter(|c| polygon.contains(*c)).unwrap_or_else(|| interior_point(polygon));
    let outer = polyline::closed(&polygon.outer);
    let perim = polyline::length(&outer);
    // `spacing` is measured between penetrations on the outline (same side).
    let n = ((perim / (spacing.max(0.1))).ceil() as usize).max(8);
    const REACH: [f64; 4] = [0.04, 0.55, 0.3, 0.7];
    let mut pts = Vec::with_capacity(n * 2 + 2);
    // Start at the outline point nearest the current position.
    let start = out.pos().and_then(|p| polyline::project(&outer, p)).map(|pr| pr.along).unwrap_or(0.0);
    for i in 0..=n {
        let s = (start + perim * i as f64 / n as f64) % perim;
        let b = polyline::point_at(&outer, s);
        let inner = c.lerp(b, REACH[i % REACH.len()]);
        if i % 2 == 0 {
            pts.push(inner);
            pts.push(b);
        } else {
            pts.push(b);
            pts.push(inner);
        }
    }
    if let Some(&p0) = pts.first() {
        out.connect(p0, trim, 2.5);
    }
    // A tiny split length is raised to the minimum (#84).
    let split = crate::model::effective_split_length(split);
    let mut prev = pts[0];
    for (i, &p) in pts.iter().enumerate().skip(1) {
        let d = prev.distance(p);
        if split > 0.0 && d > split {
            let k = (d / (split * 0.75)).ceil() as usize;
            let shift = [0.0, 0.3, -0.25][i % 3] / k as f64;
            for m in 1..k {
                out.stitch(prev.lerp(p, (m as f64 / k as f64 + shift).clamp(0.05, 0.95)));
            }
        }
        out.stitch(p);
        prev = p;
    }
}

/// Cross stitch on a square grid (rotated by `angle`), each cell sewn in `style`. Each run of
/// cells along a row is sewn there and back: an X's "/" diagonals going and its "\" diagonals
/// coming back, an upright cross's bars going and its middle line again coming back, a double
/// cross both; a half cross's "/" diagonals are sewn once, rows in turn right to left.
///
/// The grid is centred on the shape, or, given `anchor`, has a cell corner there (a
/// cross-stitch pattern's grid, so every X lands in its cell).
pub(crate) fn cross(polygon: &Polygon, cell: f64, style: CrossStyle, angle: f64, anchor: Option<Point>, out: &mut Out, trim: f64) {
    let cell = cell.clamp(1.0, 10.0);
    let fwd = Affine::rotate(angle.to_radians());
    let inv = Affine::rotate(-angle.to_radians());
    let mut p = polygon.clone();
    p.transform(&fwd);
    let bb = p.bbox();
    let (x0, y0, cols, rows) = match anchor.map(|a| fwd.apply(a)).filter(|a| a.x.is_finite() && a.y.is_finite()) {
        Some(a) => {
            let x0 = a.x + ((bb.min.x - a.x) / cell).floor() * cell;
            let y0 = a.y + ((bb.min.y - a.y) / cell).floor() * cell;
            (x0, y0, (((bb.max.x - x0) / cell).ceil() as i64).max(1), (((bb.max.y - y0) / cell).ceil() as i64).max(1))
        }
        None => {
            let cols = ((bb.width() / cell).ceil() as i64).max(1);
            let rows = ((bb.height() / cell).ceil() as i64).max(1);
            (bb.min.x - (cols as f64 * cell - bb.width()) * 0.5, bb.min.y - (rows as f64 * cell - bb.height()) * 0.5, cols, rows)
        }
    };
    let corner = |c: i64, r: i64| inv.apply(Point::new(x0 + c as f64 * cell, y0 + r as f64 * cell));
    for r in 0..rows {
        // contiguous runs of cells whose centres are inside
        let mut runs: Vec<(i64, i64)> = Vec::new();
        let mut c = 0;
        while c < cols {
            let inside = |c: i64| p.contains(Point::new(x0 + (c as f64 + 0.5) * cell, y0 + (r as f64 + 0.5) * cell));
            if inside(c) {
                let s = c;
                while c < cols && inside(c) {
                    c += 1;
                }
                runs.push((s, c - 1));
            } else {
                c += 1;
            }
        }
        if r % 2 == 1 {
            runs.reverse();
        }
        for (a, b) in runs {
            let bl = |c: i64| corner(c, r + 1);
            let tl = |c: i64| corner(c, r);
            // the middle of a cell's left side, of the cell, of its top and of its bottom
            let ml = |c: i64| tl(c).lerp(bl(c), 0.5);
            let mid = |c: i64| ml(c).lerp(ml(c + 1), 0.5);
            if matches!(style, CrossStyle::Full | CrossStyle::Double) {
                // going: "/" diagonals; coming back: "\" diagonals
                out.connect(bl(a), trim, 2.5);
                for c in a..=b {
                    out.stitch(tl(c + 1));
                    out.stitch(bl(c + 1));
                }
                for c in (a..=b).rev() {
                    out.stitch(tl(c));
                    out.stitch(bl(c));
                }
            } else if style == CrossStyle::Half {
                // "/" diagonals, every other row right to left, so each row starts where the last
                // one ended
                if r % 2 == 0 {
                    out.connect(bl(a), trim, 2.5);
                    for c in a..=b {
                        out.stitch(tl(c + 1));
                        out.stitch(bl(c + 1));
                    }
                } else {
                    out.connect(bl(b + 1), trim, 2.5);
                    for c in (a..=b).rev() {
                        out.stitch(tl(c + 1));
                        out.stitch(bl(c));
                    }
                }
            }
            if matches!(style, CrossStyle::Upright | CrossStyle::Double) {
                // going: the middle line, with a bar up and down each cell's middle
                out.connect(ml(a), trim, 2.5);
                for c in a..=b {
                    let m = mid(c);
                    out.stitch(m);
                    out.stitch(tl(c).lerp(tl(c + 1), 0.5));
                    out.stitch(bl(c).lerp(bl(c + 1), 0.5));
                    out.stitch(m);
                    out.stitch(ml(c + 1));
                }
                // coming back along the middle line
                for c in (a..=b).rev() {
                    out.stitch(ml(c));
                }
            }
        }
    }
}

/// Most rows a motif fill sews (a larger shape gets rows further apart).
const MAX_MOTIF_ROWS: f64 = 4000.0;
/// Most motifs on one row.
const MAX_ROW_MOTIFS: f64 = 20_000.0;

/// Motif fill: rows `spacing` apart across the shape (turned by `angle`), each a motif run of
/// motifs `motif_size` long and as wide (a custom motif: as wide as its drawn proportions make
/// it), kept half a motif inside the outline and lined up from row to row, each row shifted by
/// the row offset (a fraction of a motif). Rows are sewn back and forth, the motifs the same way
/// up on every row; parts of a row are joined by travel or trims.
pub(crate) fn motif_fill(polygon: &Polygon, params: &FillParams, angle: f64, out: &mut Out, trim: f64) {
    let size = if params.motif_size.is_finite() { params.motif_size.clamp(1.0, 50.0) } else { 4.0 };
    let across = size * params.custom_motif.as_ref().map_or(1.0, |m| m.aspect());
    let step = params.stitch_length.max(0.5);
    let offset = if params.offset.is_finite() { params.offset } else { 0.0 };
    let (fwd, inv) = (Affine::rotate(angle.to_radians()), Affine::rotate(-angle.to_radians()));
    // A 3D motif fill: motifs laid out plainly over the shape as it is before the warp, then
    // moved by it.
    let warp = params.warp.as_ref().and_then(|w| Warp::new(w, polygon));
    let mut p = warp.as_ref().map_or_else(|| polygon.clone(), |w| w.invert_polygon(polygon));
    p.transform(&fwd);
    // The motifs (a paired fill: two in turn) each centred across its row, and rows far enough
    // inside for them to stay in.
    let mut raws = vec![super::run::motif_points(params.motif, params.custom_motif.as_ref())];
    raws.extend(params.motif_pair.map(super::run::motif_shape));
    let mut reach: f64 = 0.05;
    let shapes: Vec<Vec<(f64, f64)>> = raws
        .iter()
        .map(|raw| {
            let (lo, hi) = raw.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &(_, c)| (lo.min(c), hi.max(c)));
            let mid = (lo + hi) * 0.5;
            reach = reach.max((hi - lo) * 0.5);
            raw.iter().map(|&(a, c)| (a, (c - mid) * across / size)).collect()
        })
        .collect();
    let inner = poly::offset(std::slice::from_ref(&p), -across * reach);
    let bb = p.bbox();
    let motifs = bb.width() / size;
    if inner.is_empty() || bb.is_empty() || motifs.is_nan() || motifs >= MAX_ROW_MOTIFS {
        return;
    }
    let spacing = params.spacing.max(0.5).max(bb.height() / MAX_MOTIF_ROWS);
    let rows = (bb.height() / spacing).floor() as usize + 1;
    let start = bb.min.y + (bb.height() - (rows - 1) as f64 * spacing) * 0.5;
    for r in 0..rows {
        let y = start + r as f64 * spacing;
        let mut xs: Vec<f64> = Vec::new();
        for ring in inner.iter().flat_map(|q| q.rings()) {
            for (a, b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
                if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                    xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        let mut spans: Vec<(f64, f64)> = xs.as_chunks::<2>().0.iter().filter_map(|w| (w[1] - w[0] >= size).then_some((w[0], w[1]))).collect();
        let back = r % 2 == 1;
        if back {
            spans.reverse();
        }
        let shift = (r as f64 * offset).rem_euclid(1.0) * size;
        for (xa, xb) in spans {
            let mut row = motif_row((xa, xb), y, size, shift, &shapes, step);
            if back {
                row.reverse();
            }
            let row: Vec<Point> = row.into_iter().map(|q| inv.apply(q)).collect();
            let row = match &warp {
                Some(w) => super::out::mapped(&row, step, |q| w.apply(q)),
                None => row,
            };
            if let Some(&first) = row.first() {
                out.connect(first, trim, 2.5);
                out.stitches(row);
            }
        }
    }
}

/// Strongest 3D motif fill warp.
const MAX_WARP: f64 = 0.8;
/// Most points the outline of a 3D motif fill is followed by through its warp.
const MAX_WARP_POINTS: f64 = 50_000.0;

/// A 3D motif fill's warp in the shape's own frame (mm): where a point of the motif fill laid
/// out plainly lands (`apply`), and where a point of the shape comes from (`invert`), so the
/// warped fill still fills the shape.
struct Warp {
    kind: WarpKind,
    /// Globes: the strength; perspective: how fast the size falls off with depth (per mm).
    k: f64,
    /// Globes: the middle and the radius out to the farthest corner; perspective: the middle.
    c: Point,
    r: f64,
    /// Perspective: the way the fill recedes, and the depth of its near edge from `c`.
    dir: Point,
    near: f64,
}

impl Warp {
    fn new(w: &MotifWarp, polygon: &Polygon) -> Option<Warp> {
        let k = if w.amount.is_finite() { w.amount.clamp(0.0, MAX_WARP) } else { 0.0 };
        let bb = polygon.bbox();
        if k < 1e-3 || polygon.is_empty() || bb.is_empty() {
            return None;
        }
        let c = bb.center();
        let r = polygon.outer.iter().map(|q| q.distance(c)).fold(0.0, f64::max);
        let a = if w.angle.is_finite() { w.angle.rem_euclid(360.0).to_radians() } else { std::f64::consts::FRAC_PI_2 };
        let dir = Point::new(a.cos(), -a.sin());
        let (near, far) =
            polygon.outer.iter().map(|q| (*q - c).dot(dir)).fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
        if !(r.is_finite() && r > 1e-6 && near.is_finite() && far - near > 1e-6) {
            return None;
        }
        // perspective: the far side's motifs `1 - k` the size of the near side's
        let k = if w.kind == WarpKind::Perspective { -(1.0 - k).ln() / (far - near) } else { k };
        Some(Warp { kind: w.kind, k, c, r, dir, near })
    }

    /// A globe's profile: how far out (a fraction of the radius) a point `rho` out lands. It
    /// rises from 0 to 1, faster in the middle for a dome and toward the edge for a bowl.
    fn globe(&self, rho: f64) -> f64 {
        let q = rho * std::f64::consts::FRAC_PI_2;
        match self.kind {
            WarpKind::GlobeIn => (1.0 - self.k) * rho + self.k * (1.0 - q.cos()),
            _ => (1.0 - self.k) * rho + self.k * q.sin(),
        }
    }

    /// Where a point of the plain layout lands.
    fn apply(&self, p: Point) -> Point {
        let d = p - self.c;
        if self.kind == WarpKind::Perspective {
            // a motif `depth` past the near edge is scaled by 1 / s
            let (depth, side) = (d.dot(self.dir) - self.near, d.dot(self.dir.perp()));
            let s = (1.0 + self.k * depth).max(1e-9);
            return self.c + self.dir * (s.ln() / self.k + self.near) + self.dir.perp() * (side / s);
        }
        let rho = d.length() / self.r;
        if !(rho > 1e-12 && rho < 1.0) {
            return p;
        }
        self.c + d * (self.globe(rho) / rho)
    }

    /// Where a point of the shape comes from in the plain layout.
    fn invert(&self, p: Point) -> Point {
        let d = p - self.c;
        if self.kind == WarpKind::Perspective {
            let (depth, side) = (d.dot(self.dir) - self.near, d.dot(self.dir.perp()));
            let e = (self.k * depth).min(50.0).exp();
            return self.c + self.dir * ((e - 1.0) / self.k + self.near) + self.dir.perp() * (side * e);
        }
        let target = d.length() / self.r;
        if !(target > 1e-12 && target < 1.0) {
            return p;
        }
        // the profile rises steadily, so halving finds where it reaches `target`
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..60 {
            let m = (lo + hi) * 0.5;
            if self.globe(m) < target {
                lo = m;
            } else {
                hi = m;
            }
        }
        self.c + d * ((lo + hi) * 0.5 / target)
    }

    /// The shape as it is before the warp, its outline followed closely enough to bend with it.
    fn invert_polygon(&self, polygon: &Polygon) -> Polygon {
        let perimeter: f64 = polygon.rings().map(|r| polyline::ring_length(r)).sum();
        let seg = (perimeter / MAX_WARP_POINTS).max(0.25);
        let ring = |r: &Vec<Point>| -> Vec<Point> {
            let mut out = Vec::new();
            for (&a, &b) in r.iter().zip(r.iter().cycle().skip(1)) {
                let parts = ((a.distance(b) / seg).ceil() as usize).clamp(1, 10_000);
                out.extend((0..parts).map(|j| self.invert(a.lerp(b, j as f64 / parts as f64))));
            }
            out
        };
        Polygon::new(ring(&polygon.outer), polygon.holes.iter().map(ring).collect())
    }
}

/// One row of a motif fill from `xa` to `xb` at height `y`: motifs `size` long on a grid from
/// `shift` (the `shapes` in turn, the same one at the same place on every row), with a straight
/// run before the first whole motif and after the last; no stitch longer than `step`.
fn motif_row((xa, xb): (f64, f64), y: f64, size: f64, shift: f64, shapes: &[Vec<(f64, f64)>], step: f64) -> Vec<Point> {
    let (k0, k1) = (((xa - shift) / size).ceil(), ((xb - shift) / size).floor() - 1.0);
    let mut pts = vec![Point::new(xa, y)];
    if k1 >= k0 {
        for k in (k0 as i64)..=(k1 as i64) {
            let x0 = shift + k as f64 * size;
            let shape = usize::try_from(k.rem_euclid(shapes.len().max(1) as i64)).ok().and_then(|i| shapes.get(i));
            pts.extend(shape.into_iter().flatten().map(|&(a, c)| Point::new(x0 + a * size, y + c * size)));
        }
    }
    pts.push(Point::new(xb, y));
    let mut out: Vec<Point> = Vec::with_capacity(pts.len() * 2);
    for q in polyline::dedup(&pts, 1e-6) {
        if let Some(&last) = out.last() {
            let parts = ((last.distance(q) / step).ceil() as usize).clamp(1, 10_000);
            out.extend((1..=parts).map(|j| last.lerp(q, j as f64 / parts as f64)));
        } else {
            out.push(q);
        }
    }
    out
}

/// Deterministic xorshift generator (no external RNG, same result everywhere).
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform in `[0, 1)`.
    pub(crate) fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Meander fill: a random maze is grown through the shape on a grid of
/// `2·spacing` cells, and the stitch path runs around it, giving one continuous,
/// wandering line with corridors `spacing` mm apart (quilting "stipple" look).
pub(crate) fn meander(polygon: &Polygon, spacing: f64, step: f64, random: f64, out: &mut Out, hint: Option<Point>, trim: f64) {
    let c = spacing.max(0.8);
    let cell = 2.0 * c;
    let inset = poly::offset(std::slice::from_ref(polygon), -c * 0.5);
    let bb = polygon.bbox();
    let cols = ((bb.width() / cell).ceil() as usize).clamp(1, 600);
    let rows = ((bb.height() / cell).ceil() as usize).clamp(1, 600);
    let origin = bb.min;
    let center = |i: usize, j: usize| Point::new(origin.x + (i as f64 + 0.5) * cell, origin.y + (j as f64 + 0.5) * cell);
    let inside: Vec<bool> = (0..cols * rows)
        .map(|k| {
            let p = center(k % cols, k / cols);
            inset.iter().any(|q| q.contains(p))
        })
        .collect();
    if !inside.iter().any(|&b| b) {
        contour(polygon, (c, 0), step, out, hint, trim);
        return;
    }
    // Random spanning forest by depth-first search.
    let seed = (bb.min.x * 1000.0) as i64 as u64 ^ ((bb.min.y * 1000.0) as i64 as u64).rotate_left(21) ^ (cols * 7919 + rows) as u64;
    let mut rng = Rng::new(seed);
    let mut visited = vec![false; cols * rows];
    // fine grid: node (i,j) -> (2i, 2j); edge cells in between
    let fw = 2 * cols + 1;
    let fh = 2 * rows + 1;
    let mut fine = vec![false; fw * fh];
    for start in 0..cols * rows {
        if !inside[start] || visited[start] {
            continue;
        }
        visited[start] = true;
        fine[(2 * (start / cols)) * fw + 2 * (start % cols)] = true;
        let mut stack = vec![start];
        while let Some(&cur) = stack.last() {
            let (i, j) = (cur % cols, cur / cols);
            let mut nb: Vec<usize> = Vec::with_capacity(4);
            if i > 0 {
                nb.push(cur - 1);
            }
            if i + 1 < cols {
                nb.push(cur + 1);
            }
            if j > 0 {
                nb.push(cur - cols);
            }
            if j + 1 < rows {
                nb.push(cur + cols);
            }
            nb.retain(|&n| inside[n] && !visited[n]);
            if nb.is_empty() {
                stack.pop();
                continue;
            }
            let next = nb[(rng.next_u64() % nb.len() as u64) as usize];
            visited[next] = true;
            let (ni, nj) = (next % cols, next / cols);
            fine[(2 * nj) * fw + 2 * ni] = true;
            fine[(j + nj) * fw + (i + ni)] = true;
            stack.push(next);
        }
    }
    // Boundary of the occupied fine cells: one closed loop per tree.
    let occ = |u: i64, v: i64| u >= 0 && v >= 0 && (u as usize) < fw && (v as usize) < fh && fine[v as usize * fw + u as usize];
    let mut next_of: std::collections::HashMap<(i64, i64), (i64, i64)> = std::collections::HashMap::new();
    for v in 0..fh as i64 {
        for u in 0..fw as i64 {
            if !occ(u, v) {
                continue;
            }
            // edges walk around the cell keeping it on the right (screen coordinates)
            if !occ(u, v - 1) {
                next_of.insert((u, v), (u + 1, v));
            }
            if !occ(u + 1, v) {
                next_of.insert((u + 1, v), (u + 1, v + 1));
            }
            if !occ(u, v + 1) {
                next_of.insert((u + 1, v + 1), (u, v + 1));
            }
            if !occ(u - 1, v) {
                next_of.insert((u, v + 1), (u, v));
            }
        }
    }
    let to_design = |(u, v): (i64, i64), rng: &mut Rng| {
        let j = random.clamp(0.0, 1.0) * c * 0.3;
        let (dx, dy) = ((rng.unit() - 0.5) * j, (rng.unit() - 0.5) * j);
        Point::new(origin.x + (u as f64) * c + c * 0.5 + dx, origin.y + (v as f64) * c + c * 0.5 + dy)
    };
    let mut loops: Vec<Vec<Point>> = Vec::new();
    let mut keys: Vec<(i64, i64)> = next_of.keys().copied().collect();
    keys.sort();
    let mut used = std::collections::HashSet::new();
    for k in keys {
        if used.contains(&k) {
            continue;
        }
        let mut ring = Vec::new();
        let mut cur = k;
        while used.insert(cur) {
            ring.push(to_design(cur, &mut rng));
            match next_of.get(&cur) {
                Some(&n) => cur = n,
                None => break,
            }
        }
        if ring.len() >= 4 {
            loops.push(ring);
        }
    }
    // Round the maze corners (Chaikin) for a flowing line.
    let smooth = |ring: &[Point]| {
        let mut r = ring.to_vec();
        for _ in 0..2 {
            let n = r.len();
            let mut s = Vec::with_capacity(n * 2);
            for i in 0..n {
                let (a, b) = (r[i], r[(i + 1) % n]);
                s.push(a.lerp(b, 0.25));
                s.push(a.lerp(b, 0.75));
            }
            r = s;
        }
        r
    };
    let mut loops: Vec<Vec<Point>> = loops.iter().map(|l| smooth(l)).collect();
    while !loops.is_empty() {
        let here = out.pos().or(hint).unwrap_or(loops[0][0]);
        let i = (0..loops.len())
            .min_by(|&a, &b| polyline::distance_to_ring(&loops[a], here).total_cmp(&polyline::distance_to_ring(&loops[b], here)))
            .unwrap_or(0);
        let ring = loops.remove(i);
        let start = polyline::ring_starting_near(&ring, here);
        let pts = polyline::resample_ring(&start, step.max(0.5), CORNER_ANGLE);
        if let Some(&p0) = pts.first() {
            out.connect(p0, trim, step);
            out.stitches(pts);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::Cmd;

    fn square(s: f64) -> Polygon {
        Polygon::from_ring(vec![Point::new(0.0, 0.0), Point::new(s, 0.0), Point::new(s, s), Point::new(0.0, s)])
    }

    #[test]
    fn motif_fills_lay_rows_of_motifs_inside_the_shape() {
        use crate::model::{FillKind, Motif};
        let poly = Polygon::from_ring(vec![Point::new(0.0, 0.0), Point::new(40.0, 0.0), Point::new(40.0, 30.0), Point::new(0.0, 30.0)]);
        for m in Motif::ALL {
            for angle in [0.0, 30.0] {
                let params = FillParams { kind: FillKind::Motif, motif: m, motif_size: 4.0, spacing: 5.0, stitch_length: 2.0, ..Default::default() };
                let mut out = Out::new();
                motif_fill(&poly, &params, angle, &mut out, 1e9);
                let pts: Vec<Point> = out.finish().iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect();
                assert!(pts.len() > 150, "{m:?} at {angle}°: {}", pts.len());
                assert!(pts.iter().all(|p| poly.contains(*p) || poly.distance_to_boundary(*p) < 0.05), "{m:?} at {angle}° leaves the shape");
                let longest = pts.windows(2).map(|w| w[0].distance(w[1])).fold(0.0, f64::max);
                assert!(longest <= 2.5 + 1e-6, "{m:?} at {angle}°: {longest}");
            }
        }
        // Nothing fits, or absurd values: nothing sewn, no panic.
        for (size, spacing) in [(50.0, 5.0), (f64::NAN, f64::NAN), (1e308, -1.0), (4.0, 1e-308)] {
            let params = FillParams { kind: FillKind::Motif, motif_size: size, spacing, ..Default::default() };
            let mut out = Out::new();
            motif_fill(&square(10.0), &params, 0.0, &mut out, 1e9);
            assert!(out.finish().iter().all(|s| s.pos().is_finite()));
        }
    }

    #[test]
    fn paired_motif_fills_sew_two_motifs_in_turn() {
        use crate::model::{FillKind, Motif};
        // On a row, a peak up and a dip down in turn, each in its place on the grid.
        let (up, down) = (vec![(0.0, 0.0), (0.5, -1.0), (1.0, 0.0)], vec![(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)]);
        let row = motif_row((0.0, 16.0), 10.0, 4.0, 0.0, &[up, down], 1.0);
        let at = |x: f64| row.iter().find(|p| (p.x - x).abs() < 1e-9).map(|p| p.y);
        assert_eq!((at(2.0), at(6.0), at(10.0)), (Some(6.0), Some(14.0), Some(6.0)), "{row:?}");
        // A fill paired with a second motif sews differently from one of the first alone, and
        // stays inside its shape.
        let poly = Polygon::from_ring(vec![Point::new(0.0, 0.0), Point::new(40.0, 0.0), Point::new(40.0, 30.0), Point::new(0.0, 30.0)]);
        let sew = |pair: Option<Motif>| {
            let params = FillParams {
                kind: FillKind::Motif,
                motif: Motif::Wave,
                motif_pair: pair,
                motif_size: 4.0,
                spacing: 5.0,
                stitch_length: 2.0,
                ..Default::default()
            };
            let mut out = Out::new();
            motif_fill(&poly, &params, 0.0, &mut out, 1e9);
            out.finish().iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect::<Vec<Point>>()
        };
        let (single, paired) = (sew(None), sew(Some(Motif::Star)));
        assert_ne!(single, paired);
        assert!(paired.len() > 150 && paired.iter().all(|p| poly.contains(*p) || poly.distance_to_boundary(*p) < 0.05));
    }

    #[test]
    fn three_d_motif_fills_grow_and_shrink_their_motifs() {
        use crate::model::{FillKind, Motif};
        // A 60 mm square: stitches per area go with how many motifs fit there (long stitches, so
        // only the motifs' own points count).
        let poly = square(60.0);
        let sew = |warp: Option<MotifWarp>| {
            let params = FillParams {
                kind: FillKind::Motif,
                motif: Motif::Diamond,
                motif_size: 4.0,
                spacing: 4.0,
                stitch_length: 20.0,
                warp,
                ..Default::default()
            };
            let mut out = Out::new();
            motif_fill(&poly, &params, 0.0, &mut out, 1e9);
            out.finish().iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect::<Vec<Point>>()
        };
        let count =
            |pts: &[Point], x: f64, y: f64| pts.iter().filter(|p| (x..x + 16.0).contains(&p.x) && (y..y + 16.0).contains(&p.y)).count() as f64;
        // the middle against a box out toward a corner
        let ratio = |pts: &[Point]| count(pts, 22.0, 22.0) / count(pts, 40.0, 40.0).max(1.0);
        let plain = sew(None);
        let flat = ratio(&plain);
        assert!((0.7..1.4).contains(&flat), "{flat}");
        let warp = |kind, amount| Some(MotifWarp { kind, amount, angle: 90.0 });
        let (dome, bowl) = (sew(warp(WarpKind::GlobeOut, 0.8)), sew(warp(WarpKind::GlobeIn, 0.8)));
        // a dome: big motifs in the middle, so fewer stitches there; a bowl: the other way round
        assert!(ratio(&dome) < flat * 0.75 && ratio(&bowl) > flat * 1.5, "{} {} {flat}", ratio(&dome), ratio(&bowl));
        // receding upward: smaller motifs at the top (low y) than at the bottom
        let far = sew(warp(WarpKind::Perspective, 0.6));
        let (top, bottom) = (count(&far, 22.0, 4.0), count(&far, 22.0, 40.0));
        assert!(top > bottom * 1.5, "{top} {bottom}");
        for pts in [&dome, &bowl, &far] {
            assert!(pts.len() > 100 && pts.iter().all(|p| poly.contains(*p) || poly.distance_to_boundary(*p) < 0.05));
        }
        // stitches the warp stretches are split
        let params = FillParams {
            kind: FillKind::Motif,
            motif_size: 4.0,
            spacing: 4.0,
            stitch_length: 2.0,
            warp: warp(WarpKind::GlobeOut, 0.8),
            ..Default::default()
        };
        let mut out = Out::new();
        motif_fill(&poly, &params, 30.0, &mut out, 1e9);
        let pts: Vec<Point> = out.finish().iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect();
        let longest = pts.windows(2).map(|w| w[0].distance(w[1])).fold(0.0, f64::max);
        assert!(longest <= 2.5 + 1e-6, "{longest}");
        // absurd values: bounded, finite
        for (amount, angle) in [(f64::NAN, f64::NAN), (1e308, -1e308), (-1.0, 0.0), (0.8, 1e300)] {
            for kind in WarpKind::ALL {
                let pts = sew(Some(MotifWarp { kind, amount, angle }));
                assert!(pts.len() < 400_000 && pts.iter().all(|p| p.is_finite()), "{kind:?} {amount} {angle}");
            }
        }
    }

    #[test]
    fn motif_fills_sew_custom_motifs_the_way_up_and_as_high_as_they_were_drawn() {
        use crate::model::{CustomMotif, FillKind};
        // On a row, a peak drawn upward points up (−y), as it does along a left-to-right run.
        let peak = [(0.0, 0.0), (0.5, -1.0), (1.0, 0.0)];
        let row = motif_row((0.0, 20.0), 10.0, 4.0, 0.0, &[peak.to_vec()], 10.0);
        assert!(row.iter().all(|p| p.y <= 10.0 + 1e-9) && row.iter().any(|p| p.y < 6.0 + 1e-9), "{row:?}");
        // A custom motif half as high as it is long: rows of it 4 mm long and 2 mm high.
        let poly = Polygon::from_ring(vec![Point::new(0.0, 0.0), Point::new(40.0, 0.0), Point::new(40.0, 30.0), Point::new(0.0, 30.0)]);
        let custom = CustomMotif { name: "Peak".into(), points: vec![[0.0, 0.0], [0.5, -1.0], [1.0, 0.0]], aspect: 0.5 };
        let params =
            FillParams { kind: FillKind::Motif, motif_size: 4.0, spacing: 5.0, stitch_length: 2.0, custom_motif: Some(custom), ..Default::default() };
        let mut out = Out::new();
        motif_fill(&poly, &params, 0.0, &mut out, 1e9);
        let pts: Vec<Point> = out.finish().iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect();
        assert!(pts.len() > 100 && pts.iter().all(|p| poly.contains(*p) || poly.distance_to_boundary(*p) < 0.05), "{}", pts.len());
        // Each row is 2 mm high: rows 5 mm apart leave 3 mm bands with no stitches (away from the
        // row ends, where travel joins the rows).
        let mut ys: Vec<f64> = pts.iter().filter(|p| (8.0..=32.0).contains(&p.x)).map(|p| p.y).collect();
        ys.sort_by(f64::total_cmp);
        let widest_gap = ys.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max);
        assert!((2.9..=3.1).contains(&widest_gap), "{widest_gap}");
    }

    #[test]
    fn contour_rings_inside() {
        let mut out = Out::new();
        contour(&square(20.0), (1.0, 0), 2.5, &mut out, None, 2.0);
        let st = out.finish();
        assert!(st.len() > 100);
        for s in st.iter().filter(|s| s.cmd == Cmd::Stitch) {
            assert!(s.x >= 0.0 && s.x <= 20.0 && s.y >= 0.0 && s.y <= 20.0);
        }
        // An offset fill: three rings 1 mm apart along the outline, the middle left open.
        let mut out = Out::new();
        contour(&square(20.0), (1.0, 3), 2.5, &mut out, None, 2.0);
        let band: Vec<Point> = out.finish().iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect();
        let depth = |p: &Point| p.x.min(p.y).min(20.0 - p.x).min(20.0 - p.y);
        assert!(band.len() > 30 && band.iter().all(|p| depth(p) <= 2.5 + 1e-6), "{:?}", band.iter().map(depth).fold(0.0, f64::max));
        assert!(band.len() < st.len() / 2);
    }

    #[test]
    fn spiral_is_continuous() {
        let mut out = Out::new();
        spiral(&square(20.0), 1.0, 2.5, &mut out, None, 2.0);
        let st = out.finish();
        assert!(st.iter().all(|s| s.cmd == Cmd::Stitch));
        for w in st.windows(2) {
            assert!(w[0].pos().distance(w[1].pos()) < 3.5);
        }
    }

    #[test]
    fn meander_is_one_continuous_line_inside() {
        let poly = square(30.0);
        let mut out = Out::new();
        meander(&poly, 2.0, 2.5, 0.5, &mut out, None, 2.0);
        let st = out.finish();
        assert!(st.len() > 150, "{}", st.len());
        assert!(st.iter().all(|s| s.cmd == Cmd::Stitch), "a square is one tree: no trims");
        for s in &st {
            assert!(s.x > -0.5 && s.x < 30.5 && s.y > -0.5 && s.y < 30.5, "{s:?}");
        }
        for w in st.windows(2) {
            assert!(w[0].pos().distance(w[1].pos()) < 3.2);
        }
    }

    #[test]
    fn a_cross_grid_with_an_anchor_puts_its_corners_there() {
        // a 6 mm square from (1, 1): with the grid's corner at (0, 0) the cells are whole mm
        // multiples of 3, so the square's three middle cells (centres 1.5, 4.5) sew
        let poly = Polygon::from_ring(vec![Point::new(1.0, 1.0), Point::new(7.0, 1.0), Point::new(7.0, 7.0), Point::new(1.0, 7.0)]);
        let mut out = Out::new();
        cross(&poly, 3.0, CrossStyle::Full, 0.0, Some(Point::new(0.0, 0.0)), &mut out, 1e9);
        let on_grid = |v: f64, o: f64| ((v - o) / 3.0 - ((v - o) / 3.0).round()).abs() < 1e-9;
        let pts: Vec<Point> = out.finish().iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect();
        assert!(!pts.is_empty());
        for p in &pts {
            // corners, or travel along a grid line between rows
            assert!(on_grid(p.x, 0.0) || on_grid(p.y, 0.0), "{p:?} off the grid");
        }
        // without the anchor the grid is centred on the shape: corners at 1, 4 and 7
        let mut free = Out::new();
        cross(&poly, 3.0, CrossStyle::Full, 0.0, None, &mut free, 1e9);
        assert!(free.finish().iter().filter(|s| s.cmd == Cmd::Stitch).all(|s| on_grid(s.pos().x, 1.0) || on_grid(s.pos().y, 1.0)));
        // the anchored grid has its corners at (0, 0), (3, 3) and (6, 6)
        assert!(pts.iter().any(|p| p.distance(Point::new(3.0, 3.0)) < 1e-9) && pts.iter().any(|p| p.distance(Point::new(6.0, 6.0)) < 1e-9));
    }

    #[test]
    fn radial_split_length_has_a_minimum() {
        // #84: a tiny split length is raised to MIN_SPLIT_LENGTH instead of making millions of stitches.
        let count = |split: f64| {
            let mut out = Out::new();
            radial(&square(20.0), None, 0.8, split, &mut out, 2.0);
            out.finish().len()
        };
        let tiny = count(1e-6);
        assert_eq!(tiny, count(crate::model::MIN_SPLIT_LENGTH));
        assert!(tiny < 100_000, "{tiny}");
    }

    #[test]
    fn radial_and_cross() {
        let mut out = Out::new();
        radial(&square(20.0), None, 0.8, 7.0, &mut out, 2.0);
        let st = out.finish();
        assert!(st.len() > 100);
        for w in st.windows(2) {
            assert!(w[0].pos().distance(w[1].pos()) <= 7.0 + 1e-6);
        }
        let mut out = Out::new();
        cross(&square(12.0), 3.0, CrossStyle::Full, 0.0, None, &mut out, 2.0);
        let st = out.finish();
        // 4×4 cells, each 4 diagonal stitches → 64 + connections
        assert!(st.len() >= 64, "{}", st.len());
    }

    #[test]
    fn cross_styles_sew_their_bars() {
        // One row of three 3 mm cells.
        let poly = Polygon::from_ring(vec![Point::new(0.0, 0.0), Point::new(9.0, 0.0), Point::new(9.0, 3.0), Point::new(0.0, 3.0)]);
        let stitches = |style| -> Vec<(Point, Point)> {
            let mut out = Out::new();
            cross(&poly, 3.0, style, 0.0, None, &mut out, 1e9);
            let pts: Vec<Point> = out.finish().iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect();
            pts.windows(2).map(|w| (w[0], w[1])).collect()
        };
        let has = |sts: &[(Point, Point)], a: (f64, f64), b: (f64, f64)| {
            let (a, b) = (Point::new(a.0, a.1), Point::new(b.0, b.1));
            sts.iter().any(|&(p, q)| (p.distance(a) < 1e-9 && q.distance(b) < 1e-9) || (p.distance(b) < 1e-9 && q.distance(a) < 1e-9))
        };
        for style in CrossStyle::ALL {
            let sts = stitches(style);
            for x in [0.0, 3.0, 6.0] {
                let rising = has(&sts, (x, 3.0), (x + 3.0, 0.0));
                let falling = has(&sts, (x, 0.0), (x + 3.0, 3.0));
                let bar = has(&sts, (x + 1.5, 0.0), (x + 1.5, 3.0));
                let across = has(&sts, (x, 1.5), (x + 1.5, 1.5)) && has(&sts, (x + 1.5, 1.5), (x + 3.0, 1.5));
                let want = match style {
                    CrossStyle::Full => (true, true, false, false),
                    CrossStyle::Half => (true, false, false, false),
                    CrossStyle::Upright => (false, false, true, true),
                    CrossStyle::Double => (true, true, true, true),
                };
                assert_eq!((rising, falling, bar, across), want, "{style:?} at {x}");
            }
            assert!(sts.iter().all(|(p, q)| p.distance(*q) <= 3.0 * std::f64::consts::SQRT_2 + 1e-9), "{style:?}");
        }
        // Half crosses on three rows: each row starts where the last one ended, one cell down.
        let mut out = Out::new();
        cross(&square(9.0), 3.0, CrossStyle::Half, 0.0, None, &mut out, 1e9);
        let pts: Vec<Point> = out.finish().iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect();
        let sewn: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        let rows = 3.0 * (3.0 * 3.0 * std::f64::consts::SQRT_2 + 3.0 * 3.0);
        assert!(sewn <= rows + 2.0 * 3.0 + 1e-6, "{sewn} > {}", rows + 6.0);
    }
}
