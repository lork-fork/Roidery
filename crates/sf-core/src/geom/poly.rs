//! Flattened polygons with holes and robust boolean / offset operations
//! (backed by `i_overlay`).

use super::{Affine, BBox, Point, polyline};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::clip::FloatClip;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::stroke::offset::StrokeOffset;
use i_overlay::mesh::float::style::{LineCap, LineJoin, OutlineStyle, StrokeStyle};
use i_overlay::string::clip::ClipRule;

/// A polygon with holes. Rings are stored without repeating the first point.
///
/// After [`Polygon::normalize`] the outer ring has positive [`polyline::signed_area`]
/// and holes have negative area.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polygon {
    pub outer: Vec<Point>,
    pub holes: Vec<Vec<Point>>,
}

impl Polygon {
    pub fn new(outer: Vec<Point>, holes: Vec<Vec<Point>>) -> Self {
        let mut p = Polygon { outer, holes };
        p.normalize();
        p
    }

    pub fn from_ring(ring: Vec<Point>) -> Self {
        Polygon::new(ring, Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.outer.len() < 3
    }

    pub fn rings(&self) -> impl Iterator<Item = &Vec<Point>> {
        std::iter::once(&self.outer).chain(self.holes.iter())
    }

    /// Orients rings (outer positive, holes negative) and drops degenerate rings.
    pub fn normalize(&mut self) {
        self.outer = polyline::dedup_ring(&self.outer, 1e-9);
        if polyline::signed_area(&self.outer) < 0.0 {
            self.outer.reverse();
        }
        self.holes = std::mem::take(&mut self.holes)
            .into_iter()
            .map(|h| polyline::dedup_ring(&h, 1e-9))
            .filter(|h| h.len() >= 3 && polyline::signed_area(h).abs() > 1e-9)
            .map(|mut h| {
                if polyline::signed_area(&h) > 0.0 {
                    h.reverse();
                }
                h
            })
            .collect();
    }

    /// Area of the region (outer minus holes).
    pub fn area(&self) -> f64 {
        polyline::signed_area(&self.outer).abs() - self.holes.iter().map(|h| polyline::signed_area(h).abs()).sum::<f64>()
    }

    pub fn contains(&self, p: Point) -> bool {
        polyline::ring_contains(&self.outer, p) && !self.holes.iter().any(|h| polyline::ring_contains(h, p))
    }

    pub fn bbox(&self) -> BBox {
        polyline::bbox(&self.outer)
    }

    pub fn transform(&mut self, m: &Affine) {
        for p in &mut self.outer {
            *p = m.apply(*p);
        }
        for h in &mut self.holes {
            for p in h.iter_mut() {
                *p = m.apply(*p);
            }
        }
        if m.is_mirroring() {
            self.normalize();
        }
    }

    pub fn distance_to_boundary(&self, p: Point) -> f64 {
        self.rings().map(|r| polyline::distance_to_ring(r, p)).fold(f64::INFINITY, f64::min)
    }

    /// Area-weighted centroid.
    pub fn centroid(&self) -> Point {
        let ao = polyline::signed_area(&self.outer).abs();
        let mut acc = polyline::ring_centroid(&self.outer) * ao;
        let mut total = ao;
        for h in &self.holes {
            let ah = polyline::signed_area(h).abs();
            acc -= polyline::ring_centroid(h) * ah;
            total -= ah;
        }
        if total.abs() < 1e-12 { polyline::ring_centroid(&self.outer) } else { acc / total }
    }

    /// Total length of all rings.
    pub fn perimeter(&self) -> f64 {
        self.rings().map(|r| polyline::ring_length(r)).sum()
    }

    /// Approximate diameter of the largest circle inside the region: how thick the
    /// shape is at its thickest (a 4 mm bar is 4 thick however long it is). A grid
    /// search refined by hill climbing; within about 2 % of the true value.
    pub fn thickness(&self) -> f64 {
        let area = self.area();
        if area.is_nan() || area <= 1e-9 {
            return 0.0;
        }
        // Distances to a lightly simplified outline: far fewer edges, the same answer to a
        // hundredth of the size.
        let tol = (area.sqrt() * 0.004).clamp(0.002, 0.03);
        let simple = Polygon {
            outer: polyline::simplify_ring(&self.outer, tol),
            holes: self.holes.iter().map(|h| polyline::simplify_ring(h, tol)).filter(|h| h.len() >= 3).collect(),
        };
        let this = if simple.outer.len() >= 3 { &simple } else { self };
        this.thickness_sampled(area)
    }

    fn thickness_sampled(&self, area: f64) -> f64 {
        let b = self.bbox();
        let step = (area / 500.0).sqrt().max(0.01);
        let (mut best, mut at) = (0.0f64, None);
        let mut y = b.min.y + step * 0.5;
        while y < b.max.y {
            let mut x = b.min.x + step * 0.5;
            while x < b.max.x {
                let p = Point::new(x, y);
                if self.contains(p) {
                    let d = self.distance_to_boundary(p);
                    if d > best {
                        (best, at) = (d, Some(p));
                    }
                }
                x += step;
            }
            y += step;
        }
        let Some(mut p) = at else { return 0.0 };
        let mut s = step * 0.5;
        const DIRS: [(f64, f64); 8] = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (0.7, 0.7), (-0.7, 0.7), (0.7, -0.7), (-0.7, -0.7)];
        while s > step * 0.01 {
            let mut moved = false;
            for (dx, dy) in DIRS {
                let q = Point::new(p.x + dx * s, p.y + dy * s);
                if self.contains(q) {
                    let d = self.distance_to_boundary(q);
                    if d > best {
                        (best, p, moved) = (d, q, true);
                    }
                }
            }
            if !moved {
                s *= 0.5;
            }
        }
        2.0 * best
    }

    fn to_shape(&self) -> Vec<Vec<[f64; 2]>> {
        self.rings().filter(|r| r.len() >= 3).map(|r| r.iter().map(|p| [p.x, p.y]).collect()).collect()
    }
}

fn to_shapes(polys: &[Polygon]) -> Vec<Vec<Vec<[f64; 2]>>> {
    polys.iter().filter(|p| !p.is_empty()).map(Polygon::to_shape).collect()
}

fn from_shapes(shapes: Vec<Vec<Vec<[f64; 2]>>>) -> Vec<Polygon> {
    shapes
        .into_iter()
        .filter_map(|shape| {
            let mut rings = shape.into_iter().map(|r| r.into_iter().map(Point::from).collect::<Vec<Point>>());
            let outer = rings.next()?;
            let p = Polygon::new(outer, rings.collect());
            (!p.is_empty() && p.area() > 1e-9).then_some(p)
        })
        .collect()
}

/// Grows (`delta > 0`) or shrinks (`delta < 0`) polygons by `delta` mm.
/// Shrinking can split a polygon into several pieces or make it vanish.
pub fn offset(polys: &[Polygon], delta: f64) -> Vec<Polygon> {
    if delta.abs() < 1e-9 {
        return polys.to_vec();
    }
    let shapes = to_shapes(polys);
    if shapes.is_empty() {
        return Vec::new();
    }
    let style = OutlineStyle::new(delta).line_join(LineJoin::Miter(0.4));
    from_shapes(shapes.outline(&style))
}

/// [`offset`] with round corners: growing never makes a spike from a sharp tip (a sliver
/// between pieces of a letter), so outlines round art stay smooth.
pub fn offset_round(polys: &[Polygon], delta: f64) -> Vec<Polygon> {
    if delta.abs() < 1e-9 {
        return polys.to_vec();
    }
    let shapes = to_shapes(polys);
    if shapes.is_empty() {
        return Vec::new();
    }
    let style = OutlineStyle::new(delta).line_join(LineJoin::Round(0.25));
    from_shapes(shapes.outline(&style))
}

/// The area lines `width` wide cover, with round ends and joins (closed lines go round), as
/// polygons.
pub fn stroke(lines: &[(Vec<Point>, bool)], width: f64) -> Vec<Polygon> {
    if !(width.is_finite() && width > 1e-6) {
        return Vec::new();
    }
    let style = StrokeStyle::new(width).line_join(LineJoin::Round(0.25)).start_cap(LineCap::Round(0.25)).end_cap(LineCap::Round(0.25));
    let mut out = Vec::new();
    for (pts, closed) in lines {
        let path: Vec<[f64; 2]> = pts.iter().filter(|p| p.x.is_finite() && p.y.is_finite()).map(|p| [p.x, p.y]).collect();
        if path.len() >= 2 {
            out.extend(from_shapes(path.stroke(style.clone(), *closed)));
        }
    }
    union(&out)
}

/// Union of all polygons (non-zero fill), also fixing self-intersections.
pub fn union(polys: &[Polygon]) -> Vec<Polygon> {
    let shapes = to_shapes(polys);
    if shapes.is_empty() {
        return Vec::new();
    }
    let contours: Vec<Vec<[f64; 2]>> = shapes.into_iter().flatten().collect();
    // Union semantics: every ring counts positively, so orient all rings the same way
    // and let holes come from the overlay itself is not what we want – instead use
    // the shapes as-is with the non-zero rule, which keeps holes (negative rings).
    from_shapes(contours.simplify_shape(FillRule::NonZero))
}

/// Cleans polygons using the even-odd rule (resolves self-intersections,
/// merges duplicate edges, fixes orientation).
pub fn clean(polys: &[Polygon]) -> Vec<Polygon> {
    let shapes = to_shapes(polys);
    if shapes.is_empty() {
        return Vec::new();
    }
    let contours: Vec<Vec<[f64; 2]>> = shapes.into_iter().flatten().collect();
    from_shapes(contours.simplify_shape(FillRule::EvenOdd))
}

fn overlay(a: &[Polygon], b: &[Polygon], rule: OverlayRule) -> Vec<Polygon> {
    let sa = to_shapes(a);
    let sb = to_shapes(b);
    if sa.is_empty() {
        return match rule {
            OverlayRule::Union | OverlayRule::Xor => union(b),
            _ => Vec::new(),
        };
    }
    if sb.is_empty() {
        return match rule {
            OverlayRule::Intersect => Vec::new(),
            _ => union(a),
        };
    }
    from_shapes(sa.overlay(&sb, rule, FillRule::NonZero))
}

/// `a ∪ b`
pub fn union2(a: &[Polygon], b: &[Polygon]) -> Vec<Polygon> {
    overlay(a, b, OverlayRule::Union)
}

/// `a − b`
pub fn difference(a: &[Polygon], b: &[Polygon]) -> Vec<Polygon> {
    overlay(a, b, OverlayRule::Difference)
}

/// `a ∩ b`
pub fn intersection(a: &[Polygon], b: &[Polygon]) -> Vec<Polygon> {
    overlay(a, b, OverlayRule::Intersect)
}

/// Clips an open polyline against polygons, returning the parts inside
/// (or outside when `inside` is false).
pub fn clip_polyline(line: &[Point], polys: &[Polygon], inside: bool) -> Vec<Vec<Point>> {
    if line.len() < 2 {
        return Vec::new();
    }
    let shapes = to_shapes(polys);
    if shapes.is_empty() {
        return if inside { Vec::new() } else { vec![line.to_vec()] };
    }
    let path: Vec<[f64; 2]> = line.iter().map(|p| [p.x, p.y]).collect();
    let rule = ClipRule { invert: !inside, boundary_included: true };
    path.clip_by(&shapes, FillRule::NonZero, rule).into_iter().map(|p| p.into_iter().map(Point::from).collect()).collect()
}

/// Half the width of the slit [`split_holes`] cuts (mm): too narrow to see in stitches.
const SLIT: f64 = 0.01;

/// A point inside a ring: its centroid when that is inside, else the middle of the
/// widest horizontal span through the centroid.
fn interior_point(ring: &[Point]) -> Option<Point> {
    let c = polyline::ring_centroid(ring);
    if polyline::ring_contains(ring, c) {
        return Some(c);
    }
    let n = ring.len();
    let mut xs: Vec<f64> = (0..n)
        .filter_map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            ((a.y <= c.y) != (b.y <= c.y)).then(|| a.x + (c.y - a.y) / (b.y - a.y) * (b.x - a.x))
        })
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.as_chunks::<2>().0.iter().max_by(|a, b| (a[1] - a[0]).total_cmp(&(b[1] - b[0]))).map(|w| Point::new((w[0] + w[1]) * 0.5, c.y))
}

/// The two stretches of `region` a line through `c` (inside `hole`) along `d` crosses
/// next to the hole, and their total length.
fn hole_cut(region: &Polygon, hole: &[Point], c: Point, d: Point, reach: f64) -> Option<(f64, Vec<[Point; 2]>)> {
    let line = [c - d * reach, c + d * reach];
    let near = |p: Point| polyline::distance_to_ring(hole, p) < 0.02;
    let segs: Vec<[Point; 2]> = clip_polyline(&line, std::slice::from_ref(region), true)
        .into_iter()
        .filter_map(|s| {
            let (a, b) = (*s.first()?, *s.last()?);
            (a.distance(b) > 1e-6 && (near(a) || near(b))).then_some([a, b])
        })
        .collect();
    (segs.len() >= 2).then(|| (segs.iter().map(|s| s[0].distance(s[1])).sum(), segs))
}

/// Cuts a region with holes into pieces without holes, so satin can turn along each
/// (an "O" becomes two curved columns). Each hole is slit through on both sides
/// along the direction that crosses the least material, so the seams fall where
/// the shape is thinnest. A region without holes comes back as it is.
pub fn split_holes(region: &Polygon) -> Vec<Polygon> {
    if region.holes.is_empty() {
        return vec![region.clone()];
    }
    let b = region.bbox();
    let reach = (b.width() + b.height()) * 2.0 + 1.0;
    let mut cuts = Vec::new();
    for hole in &region.holes {
        let Some(c) = interior_point(hole) else { continue };
        let best = (0..18)
            .filter_map(|k| {
                let a = std::f64::consts::PI * k as f64 / 18.0;
                hole_cut(region, hole, c, Point::new(a.cos(), a.sin()), reach)
            })
            .min_by(|x, y| x.0.total_cmp(&y.0));
        let Some((_, segs)) = best else { continue };
        for [a, z] in segs {
            let d = (z - a).normalized();
            let (a, z) = (a - d * 0.05, z + d * 0.05);
            let n = Point::new(-d.y, d.x) * SLIT;
            cuts.push(Polygon::from_ring(vec![a - n, z - n, z + n, a + n]));
        }
    }
    if cuts.is_empty() {
        return vec![region.clone()];
    }
    difference(std::slice::from_ref(region), &cuts)
}

/// Cuts a region without holes at its sharp inside corners, where strokes join (a
/// "b"'s stem and bowl, the arms of a "T"), into simpler pieces satin can turn
/// along: each such corner is slit to the nearest point across the shape, at most
/// 1.5 times the shape's thickness away. Smooth inside curves are left alone.
pub fn split_branches(region: &Polygon) -> Vec<Polygon> {
    const SHARP: f64 = 50.0;
    let thick = region.thickness();
    if !region.holes.is_empty() || thick <= 0.0 {
        return vec![region.clone()];
    }
    let ring = polyline::simplify_ring(&region.outer, (thick * 0.02).clamp(0.01, 0.05));
    let n = ring.len();
    if n < 5 {
        return vec![region.clone()];
    }
    let orient = polyline::signed_area(&ring).signum();
    // Boundary samples to cut to, with their positions along the ring.
    let total = polyline::ring_length(&ring);
    let step = (thick * 0.05).clamp(0.05, 0.3);
    let mut samples: Vec<(f64, Point)> = Vec::new();
    let mut at = 0.0;
    let mut cum = vec![0.0; n + 1];
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        cum[i + 1] = cum[i] + a.distance(b);
        let len = a.distance(b);
        while at < cum[i + 1] && len > 1e-12 {
            samples.push((at, a.lerp(b, (at - cum[i]) / len)));
            at += step;
        }
    }
    let crosses = |v: Point, w: Point| {
        (0..n).any(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            polyline::segment_intersection(v, w, a, b).is_some_and(|(t, u)| t > 1e-3 && t < 1.0 - 1e-3 && u > 1e-6 && u < 1.0 - 1e-6)
        })
    };
    let mut cuts = Vec::new();
    for i in 0..n {
        let (prev, v, next) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
        let (e1, e2) = (v - prev, next - v);
        let cross = e1.x * e2.y - e1.y * e2.x;
        let turn = cross.atan2(e1.x * e2.x + e1.y * e2.y).to_degrees();
        // An inside corner turns against the ring's own winding.
        if cross * orient >= 0.0 || turn.abs() < SHARP {
            continue;
        }
        let bisector = (e2.normalized() - e1.normalized()).normalized() * -1.0;
        let bisector = if region.contains(v + bisector * 1e-3) { bisector } else { bisector * -1.0 };
        let pos = cum[i];
        let best = samples
            .iter()
            .filter(|(q, _)| {
                let arc = (q - pos).rem_euclid(total);
                arc.min(total - arc) > thick * 0.6
            })
            .filter(|(_, w)| {
                let d = *w - v;
                let len = d.length();
                len > 1e-6 && len <= thick * 1.5 && d.normalized().dot(bisector) > 0.17
            })
            .filter(|(_, w)| region.contains(v.midpoint(*w)) && !crosses(v, *w))
            .min_by(|a, b| a.1.distance(v).total_cmp(&b.1.distance(v)));
        if let Some(&(_, w)) = best {
            let d = (w - v).normalized();
            let (a, z) = (v - d * 0.05, w + d * 0.05);
            let off = Point::new(-d.y, d.x) * SLIT;
            cuts.push(Polygon::from_ring(vec![a - off, z - off, z + off, a + off]));
        }
    }
    if cuts.is_empty() {
        return vec![region.clone()];
    }
    difference(std::slice::from_ref(region), &cuts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stroke_covers_a_band_round_its_line() {
        // A 10 mm line 2 mm wide: a 10 × 2 band with round ends.
        let line = (vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)], false);
        let band = stroke(std::slice::from_ref(&line), 2.0);
        let area: f64 = band.iter().map(Polygon::area).sum();
        assert!((area - (20.0 + std::f64::consts::PI)).abs() < 0.2, "{area}");
        assert!(band.iter().any(|p| p.contains(Point::new(-0.9, 0.0))) && !band.iter().any(|p| p.contains(Point::new(5.0, 1.1))));
        // A closed square line goes round, leaving its middle open.
        let square = (vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0), Point::new(0.0, 10.0), Point::new(0.0, 0.0)], true);
        let ring = stroke(&[square], 1.0);
        assert!(ring.iter().any(|p| p.contains(Point::new(0.0, 5.0))) && !ring.iter().any(|p| p.contains(Point::new(5.0, 5.0))));
        // Nothing to stroke.
        assert!(stroke(std::slice::from_ref(&line), 0.0).is_empty() && stroke(&[line], f64::NAN).is_empty());
        assert!(stroke(&[(vec![Point::new(f64::NAN, 0.0)], false)], 1.0).is_empty());
    }

    #[test]
    fn split_branches_cuts_a_t_into_columns() {
        // A "T": a 20 × 4 bar over a 4 × 16 stem.
        let t = Polygon::from_ring(vec![
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(20.0, 4.0),
            Point::new(12.0, 4.0),
            Point::new(12.0, 20.0),
            Point::new(8.0, 20.0),
            Point::new(8.0, 4.0),
            Point::new(0.0, 4.0),
        ]);
        let pieces = split_branches(&t);
        assert_eq!(pieces.len(), 3, "{pieces:?}");
        let area: f64 = pieces.iter().map(Polygon::area).sum();
        assert!((area - t.area()).abs() < 0.3, "only the slits are lost: {area}");
        // A bar or a smooth ring piece has no sharp inside corners.
        let bar = rect(0.0, 0.0, 30.0, 4.0);
        assert_eq!(split_branches(&bar).len(), 1);
        for p in split_holes(&ring(10.0, 6.0)) {
            assert_eq!(split_branches(&p).len(), 1);
        }
    }

    fn ring(r_out: f64, r_in: f64) -> Polygon {
        let circle = |r: f64| (0..72).map(|i| (i as f64 * 5.0).to_radians()).map(|a| Point::new(r * a.cos(), r * a.sin())).collect::<Vec<_>>();
        let mut p = Polygon::new(circle(r_out), vec![circle(r_in)]);
        p.normalize();
        p
    }

    #[test]
    fn thickness_is_the_widest_inscribed_circle() {
        let bar = rect(0.0, 0.0, 40.0, 4.0);
        assert!((bar.thickness() - 4.0).abs() < 0.1, "{}", bar.thickness());
        let disc = ring(5.0, 0.0);
        let disc = Polygon::from_ring(disc.outer);
        assert!((disc.thickness() - 10.0).abs() < 0.25, "{}", disc.thickness());
        let o = ring(10.0, 6.0);
        assert!((o.thickness() - 4.0).abs() < 0.15, "{}", o.thickness());
    }

    #[test]
    fn split_holes_leaves_hole_free_pieces() {
        let o = ring(10.0, 6.0);
        let pieces = split_holes(&o);
        assert!(pieces.len() >= 2, "{}", pieces.len());
        assert!(pieces.iter().all(|p| p.holes.is_empty()));
        let area: f64 = pieces.iter().map(Polygon::area).sum();
        assert!((area - o.area()).abs() < o.area() * 0.01, "{area} vs {}", o.area());
        // An "O" thin at the top and bottom (2 mm) and thick at the sides (4 mm) is cut
        // where it is thinnest, into left and right halves.
        let ellipse =
            |rx: f64, ry: f64| (0..72).map(|i| (i as f64 * 5.0).to_radians()).map(|a| Point::new(rx * a.cos(), ry * a.sin())).collect::<Vec<_>>();
        let mut oval = Polygon::new(ellipse(10.0, 20.0), vec![ellipse(6.0, 18.0)]);
        oval.normalize();
        let pieces = split_holes(&oval);
        assert_eq!(pieces.len(), 2);
        for p in &pieces {
            let b = p.bbox();
            assert!(b.height() > b.width(), "left and right halves: {b:?}");
        }
        // Nothing to do without holes.
        let bar = rect(0.0, 0.0, 10.0, 2.0);
        assert_eq!(split_holes(&bar), vec![bar]);
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon {
        Polygon::from_ring(vec![Point::new(x0, y0), Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)])
    }

    #[test]
    fn offset_shrinks_and_grows() {
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let shrunk = offset(std::slice::from_ref(&r), -1.0);
        assert_eq!(shrunk.len(), 1);
        assert!((shrunk[0].area() - 64.0).abs() < 0.05, "area {}", shrunk[0].area());
        let grown = offset(std::slice::from_ref(&r), 1.0);
        assert_eq!(grown.len(), 1);
        assert!(grown[0].area() > 140.0 && grown[0].area() < 144.5, "area {}", grown[0].area());
        assert!(offset(std::slice::from_ref(&r), -6.0).is_empty());
    }

    #[test]
    fn booleans() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 0.0, 15.0, 10.0);
        let u = union2(std::slice::from_ref(&a), std::slice::from_ref(&b));
        assert_eq!(u.len(), 1);
        assert!((u[0].area() - 150.0).abs() < 1e-3);
        let d = difference(std::slice::from_ref(&a), std::slice::from_ref(&b));
        assert!((d.iter().map(Polygon::area).sum::<f64>() - 50.0).abs() < 1e-3);
        let i = intersection(std::slice::from_ref(&a), std::slice::from_ref(&b));
        assert!((i.iter().map(Polygon::area).sum::<f64>() - 50.0).abs() < 1e-3);
        // hole punched in the middle
        let h = difference(&[rect(0.0, 0.0, 10.0, 10.0)], &[rect(4.0, 4.0, 6.0, 6.0)]);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].holes.len(), 1);
        assert!((h[0].area() - 96.0).abs() < 1e-3);
        assert!(!h[0].contains(Point::new(5.0, 5.0)));
        assert!(h[0].contains(Point::new(2.0, 2.0)));
    }

    #[test]
    fn clip_line() {
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let parts = clip_polyline(&[Point::new(-5.0, 5.0), Point::new(15.0, 5.0)], &[r], true);
        assert_eq!(parts.len(), 1);
        let len = polyline::length(&parts[0]);
        assert!((len - 10.0).abs() < 1e-3, "len {len}");
    }
}
