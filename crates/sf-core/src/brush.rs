//! Freehand digitizing helpers behind the brush widget: fitting pointer strokes to
//! editable curves, variable-width satin columns (tapered, calligraphy and
//! pressure brushes) and preset shapes ("stamps").

use crate::geom::{Column, Contour, Node, NodeKind, Point, polyline};
use serde::{Deserialize, Serialize};

/// Turn angle (radians) above which a fitted stroke vertex becomes a corner.
const FIT_CORNER_ANGLE: f64 = 0.95;

/// Simplifies a freehand pointer stroke into an editable contour: Ramer–Douglas–
/// Peucker at `tolerance` mm, then smooth vertices become curve nodes and sharp
/// turns stay corners.
pub fn fit_stroke(points: &[Point], tolerance: f64, closed: bool) -> Contour {
    let pts = polyline::dedup(points, 0.02);
    if pts.len() < 2 {
        return Contour::from_points(&pts, false);
    }
    let simplified = if closed { polyline::simplify_ring(&polyline::dedup_ring(&pts, 0.02), tolerance) } else { polyline::simplify(&pts, tolerance) };
    let n = simplified.len();
    let nodes = simplified
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let (prev, next) = if closed {
                (simplified.get((i + n - 1) % n).copied(), simplified.get((i + 1) % n).copied())
            } else {
                (i.checked_sub(1).and_then(|k| simplified.get(k).copied()), simplified.get(i + 1).copied())
            };
            let kind = match (prev, next) {
                (Some(a), Some(c)) if polyline::turn_angle(a, p, c) < FIT_CORNER_ANGLE => NodeKind::Curve,
                _ => NodeKind::Corner,
            };
            Node { p, kind }
        })
        .collect();
    Contour::new(nodes, closed)
}

/// How a brush varies the column width along the stroke.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum WidthProfile {
    /// Same width everywhere ("steil" / satin line).
    #[default]
    Constant,
    /// Full width in the middle, tapering to points at both ends.
    Tapered,
    /// A broad-nib pen held at `nib_angle` degrees (counter-clockwise on screen):
    /// stitches always run along the nib, so the stroke is thick or thin depending
    /// on its direction.
    Calligraphy { nib_angle: f64 },
    /// Width scaled by pen pressure (0–1) sampled along the stroke.
    Pressure(Vec<f32>),
}

/// A satin column of `width` mm along a centreline, shaped by `profile`, with
/// rungs about every `step` mm and sharp corners joined automatically (see
/// [`satin_line`](crate::satin_line::satin_line)).
pub fn column_along(centerline: &[Point], width: f64, profile: &WidthProfile, step: f64) -> Option<Column> {
    crate::satin_line::satin_line(centerline, false, width, profile, step, crate::model::Corners::Auto)
}

/// Preset shapes for the brush widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stamp {
    /// Free drawing (no preset).
    Free,
    /// Straight line from press to release.
    Line,
    Ellipse,
    Rectangle,
    RoundedRectangle,
    Triangle,
    Diamond,
    Pentagon,
    Hexagon,
    Star,
    Heart,
    Arrow,
}

impl Stamp {
    pub const ALL: [Stamp; 12] = [
        Stamp::Free,
        Stamp::Line,
        Stamp::Ellipse,
        Stamp::Rectangle,
        Stamp::RoundedRectangle,
        Stamp::Triangle,
        Stamp::Diamond,
        Stamp::Pentagon,
        Stamp::Hexagon,
        Stamp::Star,
        Stamp::Heart,
        Stamp::Arrow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Stamp::Free => "Free draw",
            Stamp::Line => "Line",
            Stamp::Ellipse => "Circle / ellipse",
            Stamp::Rectangle => "Rectangle",
            Stamp::RoundedRectangle => "Rounded rectangle",
            Stamp::Triangle => "Triangle",
            Stamp::Diamond => "Diamond",
            Stamp::Pentagon => "Pentagon",
            Stamp::Hexagon => "Hexagon",
            Stamp::Star => "Star",
            Stamp::Heart => "Heart",
            Stamp::Arrow => "Arrow",
        }
    }

    /// Whether the stamp makes a closed outline.
    pub fn is_closed(self) -> bool {
        !matches!(self, Stamp::Free | Stamp::Line)
    }
}

/// The outline of a stamp dragged from `from` to `to`. With `square`, the box is
/// made square (shift-drag). Returns `None` for [`Stamp::Free`] or a degenerate drag.
pub fn stamp(stamp: Stamp, from: Point, to: Point, square: bool) -> Option<Contour> {
    let mut to = to;
    if square {
        let d = to - from;
        let s = d.x.abs().max(d.y.abs());
        to = Point::new(from.x + s * d.x.signum(), from.y + s * d.y.signum());
    }
    if stamp == Stamp::Line {
        return (from.distance(to) > 1e-6).then(|| Contour::from_points(&[from, to], false));
    }
    let min = from.min(to);
    let max = from.max(to);
    let (w, h) = (max.x - min.x, max.y - min.y);
    if w < 1e-6 || h < 1e-6 {
        return None;
    }
    // unit-box coordinates → design space
    let at = |x: f64, y: f64| Point::new(min.x + x * w, min.y + y * h);
    let corners = |pts: &[(f64, f64)]| pts.iter().map(|&(x, y)| Node::corner(at(x, y))).collect::<Vec<_>>();
    let regular = |n: usize, rot: f64| {
        (0..n)
            .map(|i| {
                let a = rot + i as f64 * std::f64::consts::TAU / n as f64;
                Node::corner(at(0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin()))
            })
            .collect::<Vec<_>>()
    };
    let up = -std::f64::consts::FRAC_PI_2;
    let nodes: Vec<Node> = match stamp {
        Stamp::Free | Stamp::Line => return None,
        Stamp::Ellipse => (0..8)
            .map(|i| {
                let a = i as f64 * std::f64::consts::TAU / 8.0;
                Node::curve(at(0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin()))
            })
            .collect(),
        Stamp::Rectangle => corners(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]),
        Stamp::RoundedRectangle => {
            let r = 0.18;
            vec![
                Node::corner(at(r, 0.0)),
                Node::corner(at(1.0 - r, 0.0)),
                Node::curve(at(1.0 - r * 0.3, r * 0.3)),
                Node::corner(at(1.0, r)),
                Node::corner(at(1.0, 1.0 - r)),
                Node::curve(at(1.0 - r * 0.3, 1.0 - r * 0.3)),
                Node::corner(at(1.0 - r, 1.0)),
                Node::corner(at(r, 1.0)),
                Node::curve(at(r * 0.3, 1.0 - r * 0.3)),
                Node::corner(at(0.0, 1.0 - r)),
                Node::corner(at(0.0, r)),
                Node::curve(at(r * 0.3, r * 0.3)),
            ]
        }
        Stamp::Triangle => corners(&[(0.5, 0.0), (1.0, 1.0), (0.0, 1.0)]),
        Stamp::Diamond => corners(&[(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)]),
        Stamp::Pentagon => regular(5, up),
        Stamp::Hexagon => regular(6, 0.0),
        Stamp::Star => (0..10)
            .map(|i| {
                let a = up + i as f64 * std::f64::consts::TAU / 10.0;
                let r = if i % 2 == 0 { 0.5 } else { 0.2 };
                Node::corner(at(0.5 + r * a.cos(), 0.5 + r * a.sin()))
            })
            .collect(),
        Stamp::Heart => vec![
            Node::corner(at(0.5, 1.0)),
            Node::curve(at(0.14, 0.62)),
            Node::curve(at(0.0, 0.3)),
            Node::curve(at(0.1, 0.07)),
            Node::curve(at(0.3, 0.0)),
            Node::curve(at(0.45, 0.08)),
            Node::corner(at(0.5, 0.2)),
            Node::curve(at(0.55, 0.08)),
            Node::curve(at(0.7, 0.0)),
            Node::curve(at(0.9, 0.07)),
            Node::curve(at(1.0, 0.3)),
            Node::curve(at(0.86, 0.62)),
        ],
        Stamp::Arrow => corners(&[(0.0, 0.32), (0.58, 0.32), (0.58, 0.0), (1.0, 0.5), (0.58, 1.0), (0.58, 0.68), (0.0, 0.68)]),
    };
    Some(Contour::new(nodes, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_keeps_corners_and_smooths_curves() {
        // an L made of many samples plus a gentle arc
        let mut pts: Vec<Point> = (0..=50).map(|i| Point::new(i as f64 * 0.2, 0.0)).collect();
        pts.extend((1..=50).map(|i| Point::new(10.0, i as f64 * 0.2)));
        let c = fit_stroke(&pts, 0.1, false);
        assert_eq!(c.nodes.len(), 3);
        assert_eq!(c.nodes[1].kind, NodeKind::Corner);
        let arc: Vec<Point> = (0..=100)
            .map(|i| {
                let a = i as f64 / 100.0 * std::f64::consts::PI;
                Point::new(10.0 * a.cos(), 10.0 * a.sin())
            })
            .collect();
        let c = fit_stroke(&arc, 0.1, false);
        assert!(c.nodes.len() < 30);
        assert!(c.nodes.iter().skip(1).take(c.nodes.len() - 2).all(|n| n.kind == NodeKind::Curve));
        // the fitted curve stays within tolerance of the stroke
        let poly = c.polyline(0.02);
        for p in &arc {
            assert!(polyline::distance_to_polyline(&poly, *p) < 0.3);
        }
    }

    #[test]
    fn width_profiles() {
        let line: Vec<Point> = vec![Point::new(0.0, 0.0), Point::new(20.0, 0.0)];
        let c = column_along(&line, 4.0, &WidthProfile::Constant, 1.0).unwrap();
        assert!(c.rungs.iter().all(|r| (r.width() - 4.0).abs() < 1e-9));
        let t = column_along(&line, 4.0, &WidthProfile::Tapered, 1.0).unwrap();
        let first = t.rungs.first().unwrap().width();
        let mid = t.rungs[t.rungs.len() / 2].width();
        assert!(first < 0.5 && (mid - 4.0).abs() < 1e-6, "{first} {mid}");
        // a calligraphy nib at 45° on a horizontal stroke: rungs run at 45°
        let k = column_along(&line, 4.0, &WidthProfile::Calligraphy { nib_angle: 45.0 }, 1.0).unwrap();
        let r = k.rungs[3];
        let ang = crate::geom::screen_angle(r.a - r.b);
        assert!((ang - 45.0).abs() < 1e-6, "{ang}");
        let p = column_along(&line, 4.0, &WidthProfile::Pressure(vec![0.2, 1.0]), 1.0).unwrap();
        assert!(p.rungs.first().unwrap().width() < p.rungs.last().unwrap().width());
    }

    #[test]
    fn stamps_are_closed_and_sized() {
        for s in Stamp::ALL {
            let c = stamp(s, Point::new(0.0, 0.0), Point::new(20.0, 10.0), false);
            match s {
                Stamp::Free => assert!(c.is_none()),
                Stamp::Line => assert!(!c.unwrap().closed),
                _ => {
                    let c = c.unwrap();
                    assert!(c.closed);
                    let b = c.bbox();
                    assert!(b.width() > 15.0 && b.width() <= 20.0 + 1e-6, "{s:?} {}", b.width());
                }
            }
        }
        let sq = stamp(Stamp::Rectangle, Point::new(0.0, 0.0), Point::new(20.0, 10.0), true).unwrap();
        let b = sq.bbox();
        assert!((b.width() - b.height()).abs() < 1e-9);
    }
}
