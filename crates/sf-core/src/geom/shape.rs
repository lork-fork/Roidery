//! Editable outlines: contours made of corner/curve nodes (the classic digitizing
//! model – click for a corner point, right-click for a curve point), closed shapes
//! with holes, and satin columns described by pairs of rail points ("rungs").

use super::{Affine, BBox, Point, Polygon, polyline};
use serde::{Deserialize, Serialize};

/// Default flattening tolerance (mm) for turning curves into polylines.
pub const FLATTEN_TOLERANCE: f64 = 0.05;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    /// Sharp corner: the outline may change direction abruptly here.
    #[default]
    Corner,
    /// Smooth point: a spline passes through it with a continuous tangent.
    Curve,
}

fn is_corner(k: &NodeKind) -> bool {
    *k == NodeKind::Corner
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub p: Point,
    #[serde(default, skip_serializing_if = "is_corner")]
    pub kind: NodeKind,
}

impl Node {
    pub const fn corner(p: Point) -> Node {
        Node { p, kind: NodeKind::Corner }
    }
    pub const fn curve(p: Point) -> Node {
        Node { p, kind: NodeKind::Curve }
    }
}

/// An open or closed outline through a list of nodes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Contour {
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub closed: bool,
}

impl Contour {
    pub fn new(nodes: Vec<Node>, closed: bool) -> Self {
        Contour { nodes, closed }
    }

    /// A contour of corner nodes through the given points.
    pub fn from_points(pts: &[Point], closed: bool) -> Self {
        Contour { nodes: pts.iter().map(|&p| Node::corner(p)).collect(), closed }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn points(&self) -> impl Iterator<Item = Point> + '_ {
        self.nodes.iter().map(|n| n.p)
    }

    /// Flattened polyline. For closed contours the result is a ring
    /// (the first point is not repeated).
    pub fn polyline(&self, tolerance: f64) -> Vec<Point> {
        let segs = flatten_segments(&self.nodes, self.closed, tolerance);
        let mut out: Vec<Point> = Vec::new();
        for s in segs {
            for p in s {
                if out.last().is_none_or(|q: &Point| !q.approx_eq(p, 1e-12)) {
                    out.push(p);
                }
            }
        }
        if out.is_empty() && self.nodes.len() == 1 {
            out.push(self.nodes[0].p);
        }
        if self.closed { polyline::dedup_ring(&out, 1e-9) } else { out }
    }

    /// Flattened polyline split per node-to-node segment (each part includes both end nodes).
    pub fn segments(&self, tolerance: f64) -> Vec<Vec<Point>> {
        flatten_segments(&self.nodes, self.closed, tolerance)
    }

    pub fn transform(&mut self, m: &Affine) {
        for n in &mut self.nodes {
            n.p = m.apply(n.p);
        }
    }

    pub fn bbox(&self) -> BBox {
        BBox::from_points(self.polyline(FLATTEN_TOLERANCE * 4.0))
    }

    pub fn reverse(&mut self) {
        self.nodes.reverse();
    }

    pub fn length(&self, tolerance: f64) -> f64 {
        let p = self.polyline(tolerance);
        if self.closed { polyline::ring_length(&p) } else { polyline::length(&p) }
    }
}

/// A closed region with optional holes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub outer: Contour,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Contour>,
}

impl Shape {
    pub fn new(outer: Contour, holes: Vec<Contour>) -> Self {
        let mut s = Shape { outer, holes };
        s.outer.closed = true;
        for h in &mut s.holes {
            h.closed = true;
        }
        s
    }

    /// A shape from polygon rings, all nodes being corners.
    pub fn from_polygon(p: &Polygon) -> Self {
        Shape { outer: Contour::from_points(&p.outer, true), holes: p.holes.iter().map(|h| Contour::from_points(h, true)).collect() }
    }

    pub fn rect(min: Point, max: Point) -> Self {
        Shape::from_polygon(&Polygon::from_ring(vec![min, Point::new(max.x, min.y), max, Point::new(min.x, max.y)]))
    }

    /// An ellipse made of curve nodes.
    pub fn ellipse(center: Point, rx: f64, ry: f64) -> Self {
        let nodes = (0..8)
            .map(|i| {
                let a = i as f64 * std::f64::consts::TAU / 8.0;
                Node::curve(Point::new(center.x + rx * a.cos(), center.y + ry * a.sin()))
            })
            .collect();
        Shape { outer: Contour::new(nodes, true), holes: Vec::new() }
    }

    /// Flattened polygon with normalised orientation.
    pub fn polygon(&self, tolerance: f64) -> Polygon {
        let mut p = Polygon {
            outer: self.outer.polyline(tolerance),
            holes: self.holes.iter().map(|h| h.polyline(tolerance)).filter(|h| h.len() >= 3).collect(),
        };
        p.normalize();
        p
    }

    pub fn transform(&mut self, m: &Affine) {
        self.outer.transform(m);
        for h in &mut self.holes {
            h.transform(m);
        }
    }

    pub fn bbox(&self) -> BBox {
        self.outer.bbox()
    }

    pub fn contains(&self, p: Point) -> bool {
        self.polygon(FLATTEN_TOLERANCE * 4.0).contains(p)
    }
}

/// One pair of corresponding points on the two rails of a satin column.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rung {
    pub a: Point,
    pub b: Point,
    #[serde(default, skip_serializing_if = "is_corner")]
    pub kind: NodeKind,
}

impl Rung {
    pub fn new(a: Point, b: Point, kind: NodeKind) -> Self {
        Rung { a, b, kind }
    }
    pub fn width(&self) -> f64 {
        self.a.distance(self.b)
    }
    pub fn center(&self) -> Point {
        self.a.midpoint(self.b)
    }
}

/// A satin column: two rails (through the `a` and `b` points of the rungs).
/// Stitches swing between the rails; the rungs also define how stitch direction
/// turns along the column.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub rungs: Vec<Rung>,
}

impl Column {
    pub fn new(rungs: Vec<Rung>) -> Self {
        Column { rungs }
    }

    pub fn rail_a(&self) -> Contour {
        Contour::new(self.rungs.iter().map(|r| Node { p: r.a, kind: r.kind }).collect(), false)
    }

    pub fn rail_b(&self) -> Contour {
        Contour::new(self.rungs.iter().map(|r| Node { p: r.b, kind: r.kind }).collect(), false)
    }

    /// Flattened rails per rung interval: `(rail_a_part, rail_b_part)` for each pair of
    /// consecutive rungs.
    pub fn sections(&self, tolerance: f64) -> Vec<(Vec<Point>, Vec<Point>)> {
        let a = self.rail_a().segments(tolerance);
        let b = self.rail_b().segments(tolerance);
        a.into_iter().zip(b).collect()
    }

    /// Closed outline: rail A forward then rail B backward.
    pub fn outline(&self, tolerance: f64) -> Vec<Point> {
        let mut ring = self.rail_a().polyline(tolerance);
        let mut b = self.rail_b().polyline(tolerance);
        b.reverse();
        ring.extend(b);
        polyline::dedup_ring(&ring, 1e-9)
    }

    pub fn centerline(&self, tolerance: f64) -> Vec<Point> {
        let c = Contour::new(self.rungs.iter().map(|r| Node { p: r.center(), kind: r.kind }).collect(), false);
        c.polyline(tolerance)
    }

    /// Builds a column of constant `width` around a centreline (digitizing "input C").
    pub fn from_centerline(nodes: &[Node], width: f64) -> Column {
        let pts: Vec<Point> = nodes.iter().map(|n| n.p).collect();
        let normals = polyline::vertex_normals(&pts, false);
        let h = width * 0.5;
        let rungs = nodes.iter().zip(normals).map(|(n, nv)| Rung { a: n.p + nv * h, b: n.p - nv * h, kind: n.kind }).collect();
        Column { rungs }
    }

    pub fn average_width(&self) -> f64 {
        if self.rungs.is_empty() {
            return 0.0;
        }
        self.rungs.iter().map(Rung::width).sum::<f64>() / self.rungs.len() as f64
    }

    pub fn transform(&mut self, m: &Affine) {
        for r in &mut self.rungs {
            r.a = m.apply(r.a);
            r.b = m.apply(r.b);
        }
    }

    pub fn reverse(&mut self) {
        self.rungs.reverse();
    }

    /// Swaps the two rails.
    pub fn swap_rails(&mut self) {
        for r in &mut self.rungs {
            std::mem::swap(&mut r.a, &mut r.b);
        }
    }

    pub fn bbox(&self) -> BBox {
        BBox::from_points(self.outline(FLATTEN_TOLERANCE * 4.0))
    }
}

// ---------------------------------------------------------------------------
// Spline flattening
// ---------------------------------------------------------------------------

/// Bezier handles for each node: `(in_handle, out_handle)` offsets from the node.
pub fn bezier_handles(nodes: &[Node], closed: bool) -> Vec<(Point, Point)> {
    let n = nodes.len();
    (0..n)
        .map(|i| {
            let node = nodes[i];
            if node.kind == NodeKind::Corner {
                return (Point::ZERO, Point::ZERO);
            }
            let prev = if i > 0 {
                Some(nodes[i - 1].p)
            } else if closed && n > 2 {
                Some(nodes[n - 1].p)
            } else {
                None
            };
            let next = if i + 1 < n {
                Some(nodes[i + 1].p)
            } else if closed && n > 2 {
                Some(nodes[0].p)
            } else {
                None
            };
            match (prev, next) {
                (Some(a), Some(b)) => {
                    let t = (b - a).normalized();
                    let lin = node.p.distance(a) / 3.0;
                    let lout = node.p.distance(b) / 3.0;
                    (-t * lin, t * lout)
                }
                // Open-path end points behave as corners.
                _ => (Point::ZERO, Point::ZERO),
            }
        })
        .collect()
}

/// Flattens node segments into polylines; each returned part starts at node `i` and
/// ends at node `i+1` (wrapping for closed contours).
pub fn flatten_segments(nodes: &[Node], closed: bool, tolerance: f64) -> Vec<Vec<Point>> {
    let n = nodes.len();
    if n < 2 {
        return Vec::new();
    }
    let h = bezier_handles(nodes, closed);
    let seg_count = if closed { n } else { n - 1 };
    let tol = tolerance.max(1e-4);
    (0..seg_count)
        .map(|i| {
            let j = (i + 1) % n;
            let p0 = nodes[i].p;
            let p3 = nodes[j].p;
            let c1 = p0 + h[i].1;
            let c2 = p3 + h[j].0;
            if h[i].1 == Point::ZERO && h[j].0 == Point::ZERO {
                vec![p0, p3]
            } else {
                let mut out = vec![p0];
                flatten_cubic(p0, c1, c2, p3, tol, 0, &mut out);
                out
            }
        })
        .collect()
}

/// Flattens nodes into a single polyline (ring for closed input).
pub fn flatten_nodes(nodes: &[Node], closed: bool, tolerance: f64) -> Vec<Point> {
    Contour { nodes: nodes.to_vec(), closed }.polyline(tolerance)
}

/// Recursive subdivision of a cubic Bezier; pushes points after `p0` up to and including `p3`.
pub fn flatten_cubic(p0: Point, p1: Point, p2: Point, p3: Point, tol: f64, depth: u32, out: &mut Vec<Point>) {
    let flat = polyline::distance_to_segment(p1, p0, p3).max(polyline::distance_to_segment(p2, p0, p3));
    if flat <= tol || depth >= 16 {
        out.push(p3);
        return;
    }
    let p01 = p0.midpoint(p1);
    let p12 = p1.midpoint(p2);
    let p23 = p2.midpoint(p3);
    let p012 = p01.midpoint(p12);
    let p123 = p12.midpoint(p23);
    let mid = p012.midpoint(p123);
    flatten_cubic(p0, p01, p012, mid, tol, depth + 1, out);
    flatten_cubic(mid, p123, p23, p3, tol, depth + 1, out);
}

/// Flattens a quadratic Bezier (as used by TrueType outlines).
pub fn flatten_quad(p0: Point, p1: Point, p2: Point, tol: f64, out: &mut Vec<Point>) {
    let c1 = p0 + (p1 - p0) * (2.0 / 3.0);
    let c2 = p2 + (p1 - p2) * (2.0 / 3.0);
    flatten_cubic(p0, c1, c2, p2, tol, 0, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_stay_straight() {
        let c = Contour::from_points(&[Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 5.0)], false);
        assert_eq!(c.polyline(0.01).len(), 3);
    }

    #[test]
    fn curves_pass_through_nodes() {
        let shape = Shape::ellipse(Point::new(0.0, 0.0), 10.0, 5.0);
        let poly = shape.outer.polyline(0.01);
        assert!(poly.len() > 32);
        for n in &shape.outer.nodes {
            assert!(poly.iter().any(|p| p.approx_eq(n.p, 1e-9)));
        }
        // The flattened ellipse should be close to the true area (π·a·b).
        let area = polyline::signed_area(&poly).abs();
        let expect = std::f64::consts::PI * 50.0;
        assert!((area - expect).abs() / expect < 0.02, "area {area} vs {expect}");
    }

    #[test]
    fn column_from_centerline() {
        let nodes = [Node::corner(Point::new(0.0, 0.0)), Node::corner(Point::new(20.0, 0.0))];
        let col = Column::from_centerline(&nodes, 4.0);
        assert_eq!(col.rungs.len(), 2);
        assert!((col.average_width() - 4.0).abs() < 1e-9);
        let outline = col.outline(0.05);
        assert!((polyline::signed_area(&outline).abs() - 80.0).abs() < 1e-6);
    }

    #[test]
    fn shape_serde_round_trip() {
        let s = Shape::ellipse(Point::new(1.0, 2.0), 3.0, 4.0);
        let json = serde_json::to_string(&s).unwrap();
        let back: Shape = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}
