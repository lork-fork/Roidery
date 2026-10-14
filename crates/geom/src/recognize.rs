//! Shape recognition for the Shaper tool: a rough freehand stroke becomes a clean shape.
//!
//! The stroke is classified by how closed it is, how many corners survive a coarse polyline
//! simplification (Douglas–Peucker at a fraction of the stroke's size) and how well it fills its
//! bounding box. A zig-zag stroke with several reversals is a scribble (the Shaper deletes what it
//! covers).

use crate::{Point, Rect, Vec2};

/// What the Shaper recognised.
#[derive(Clone, Debug, PartialEq)]
pub enum Recognized {
    Line {
        a: Point,
        b: Point,
    },
    /// Axis-aligned rectangle (squares snap when the sides are within 10%).
    Rectangle(Rect),
    /// Axis-aligned ellipse in `rect` (circles snap when the axes are within 10%).
    Ellipse(Rect),
    /// Regular polygon (triangle, pentagon, hexagon…): centre, radius, side count and the rotation
    /// (degrees) of the first vertex from straight up.
    Polygon {
        center: Point,
        radius: f64,
        sides: u32,
        rotation: f64,
    },
    /// A zig-zag over existing art: delete what it covers.
    Scribble(Rect),
}

fn bbox(pts: &[Point]) -> Rect {
    pts.iter().fold(Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN), |r, p| r.union_pt(*p))
}

fn path_len(pts: &[Point]) -> f64 {
    pts.windows(2).map(|w| w[0].distance(w[1])).sum()
}

fn dist_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let l2 = ab.hypot2();
    if l2 < 1e-12 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Douglas–Peucker simplification.
fn simplify(pts: &[Point], tol: f64) -> Vec<Point> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let (i, d) = pts[1..pts.len() - 1]
        .iter()
        .enumerate()
        .map(|(i, p)| (i + 1, dist_to_segment(*p, a, b)))
        .fold((0, 0.0), |m, x| if x.1 > m.1 { x } else { m });
    if d <= tol {
        return vec![a, b];
    }
    let mut left = simplify(&pts[..=i], tol);
    let right = simplify(&pts[i..], tol);
    left.pop();
    left.extend(right);
    left
}

fn area(poly: &[Point]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].to_vec2().cross(poly[(i + 1) % n].to_vec2())).sum::<f64>().abs() / 2.0
}

/// Number of sharp direction reversals (turns of more than 120°) along the stroke.
fn reversals(pts: &[Point], min_seg: f64) -> usize {
    let s = simplify(pts, min_seg);
    s.windows(3)
        .filter(|w| {
            let (u, v): (Vec2, Vec2) = (w[1] - w[0], w[2] - w[1]);
            u.hypot() > min_seg && v.hypot() > min_seg && u.dot(v) / (u.hypot() * v.hypot()) < -0.5
        })
        .count()
}

/// Recognise a freehand stroke (document points, in drawing order).
pub fn recognize(pts: &[Point]) -> Option<Recognized> {
    if pts.len() < 2 {
        return None;
    }
    let b = bbox(pts);
    let size = b.width().max(b.height());
    if size < 2.0 {
        return None;
    }
    let len = path_len(pts);
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    // Scribble: several sharp reversals packed into a small area relative to the stroke length.
    if reversals(pts, size * 0.08) >= 3 && len > 3.0 * size {
        return Some(Recognized::Scribble(b));
    }
    // Line: every point near the chord.
    let chord = first.distance(last);
    let max_dev = pts.iter().map(|p| dist_to_segment(*p, first, last)).fold(0.0, f64::max);
    if chord > 0.5 * size && max_dev < 0.08 * chord.max(1.0) && len < 1.3 * chord {
        return Some(Recognized::Line { a: first, b: last });
    }
    // Closed shapes: the ends meet (within a fifth of the size).
    if chord > 0.25 * size {
        return None;
    }
    let mut closed: Vec<Point> = pts.to_vec();
    closed.push(first);
    let mut corners = simplify(&closed, size * 0.12);
    if corners.len() > 1 && corners[0].distance(corners[corners.len() - 1]) < size * 0.15 {
        corners.pop();
    }
    let fill = area(pts) / (b.width() * b.height()).max(1e-9);
    let center = b.center();
    let square = |r: Rect| {
        let (w, h) = (r.width(), r.height());
        if (w - h).abs() < 0.1 * w.max(h) {
            let s = (w + h) / 2.0;
            Rect::from_center_size(r.center(), (s, s))
        } else {
            r
        }
    };
    match corners.len() {
        3 => {
            let radius = corners.iter().map(|p| p.distance(center)).sum::<f64>() / 3.0;
            // Rotation of the vertex nearest straight up (y grows downward).
            let top = corners.iter().copied().min_by(|a, b| a.y.total_cmp(&b.y)).unwrap_or(center);
            let rot = (top - center).atan2().to_degrees() + 90.0;
            Some(Recognized::Polygon { center, radius, sides: 3, rotation: rot })
        }
        4 if fill > 0.86 => Some(Recognized::Rectangle(square(b))),
        // A diamond: a square standing on a corner.
        4 if (0.4..0.62).contains(&fill) => {
            let radius = corners.iter().map(|p| p.distance(center)).sum::<f64>() / 4.0;
            Some(Recognized::Polygon { center, radius, sides: 4, rotation: 0.0 })
        }
        n @ 5..=8 if fill > 0.6 && fill < 0.85 && is_polygonal(pts, &corners, size) => {
            let radius = corners.iter().map(|p| p.distance(center)).sum::<f64>() / n as f64;
            Some(Recognized::Polygon { center, radius, sides: n as u32, rotation: 0.0 })
        }
        _ if fill > 0.6 => Some(Recognized::Ellipse(square(b))),
        _ => None,
    }
}

/// Do the stroke's points hug the straight edges between `corners` (rather than bulging like an
/// ellipse)?
fn is_polygonal(pts: &[Point], corners: &[Point], size: f64) -> bool {
    let n = corners.len();
    let dev: f64 = pts.iter().map(|p| (0..n).map(|i| dist_to_segment(*p, corners[i], corners[(i + 1) % n])).fold(f64::MAX, f64::min)).sum::<f64>()
        / pts.len() as f64;
    dev < 0.02 * size
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wobbly stroke through `corners` (closed), `k` samples per edge.
    fn wobbly(corners: &[Point], k: usize, wobble: f64) -> Vec<Point> {
        let mut out = vec![];
        let n = corners.len();
        for i in 0..n {
            let (a, b) = (corners[i], corners[(i + 1) % n]);
            for j in 0..k {
                let t = j as f64 / k as f64;
                let w = ((i * k + j) as f64 * 1.7).sin() * wobble;
                out.push(a + (b - a) * t + Vec2::new(w, -w));
            }
        }
        out.push(corners[0] + Vec2::new(3.0, 2.0));
        out
    }

    fn circle(c: Point, rx: f64, ry: f64, k: usize) -> Vec<Point> {
        (0..=k)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / k as f64;
                c + Vec2::new(rx * a.cos() + (a * 5.0).sin(), ry * a.sin())
            })
            .collect()
    }

    #[test]
    fn rough_rectangle_and_square() {
        let r = wobbly(&[Point::new(0.0, 0.0), Point::new(200.0, 0.0), Point::new(200.0, 100.0), Point::new(0.0, 100.0)], 20, 2.0);
        let Some(Recognized::Rectangle(b)) = recognize(&r) else { panic!("{:?}", recognize(&r)) };
        assert!((b.width() - 200.0).abs() < 8.0 && (b.height() - 100.0).abs() < 8.0);
        let s = wobbly(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 95.0), Point::new(0.0, 95.0)], 20, 1.0);
        let Some(Recognized::Rectangle(b)) = recognize(&s) else { panic!() };
        assert!((b.width() - b.height()).abs() < 1e-9, "near-squares snap to squares");
    }

    #[test]
    fn ellipse_circle_triangle_hexagon_line() {
        assert!(
            matches!(recognize(&circle(Point::new(100.0, 100.0), 80.0, 40.0, 72)), Some(Recognized::Ellipse(r)) if r.width() > 150.0 && r.height() < 100.0)
        );
        assert!(
            matches!(recognize(&circle(Point::new(100.0, 100.0), 50.0, 48.0, 72)), Some(Recognized::Ellipse(r)) if (r.width() - r.height()).abs() < 1e-9)
        );
        let tri = wobbly(&[Point::new(100.0, 0.0), Point::new(200.0, 170.0), Point::new(0.0, 170.0)], 25, 1.5);
        assert!(matches!(recognize(&tri), Some(Recognized::Polygon { sides: 3, rotation, .. }) if rotation.abs() < 5.0), "{:?}", recognize(&tri));
        let hex: Vec<Point> = (0..6)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 6.0;
                Point::new(100.0 + 80.0 * a.cos(), 100.0 + 80.0 * a.sin())
            })
            .collect();
        assert!(matches!(recognize(&wobbly(&hex, 15, 0.5)), Some(Recognized::Polygon { sides: 6, .. })), "{:?}", recognize(&wobbly(&hex, 15, 0.5)));
        let line: Vec<Point> = (0..30).map(|i| Point::new(i as f64 * 10.0, i as f64 * 3.0 + (i as f64).sin())).collect();
        assert!(matches!(recognize(&line), Some(Recognized::Line { .. })));
    }

    #[test]
    fn scribble_and_noise() {
        let z: Vec<Point> = (0..40).map(|i| Point::new(100.0 + (i % 2) as f64 * 60.0, 100.0 + i as f64 * 2.0)).collect();
        assert!(matches!(recognize(&z), Some(Recognized::Scribble(_))));
        assert_eq!(recognize(&[Point::new(0.0, 0.0)]), None);
        // An open curl is not a shape.
        let curl: Vec<Point> = (0..60)
            .map(|i| {
                let a = i as f64 * 0.08;
                Point::new(100.0 + 50.0 * a.cos(), 100.0 + 50.0 * a.sin())
            })
            .collect();
        assert_eq!(recognize(&curl), None);
    }
}
