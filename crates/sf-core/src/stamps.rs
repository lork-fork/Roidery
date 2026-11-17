//! Carving stamps: shapes pressed into tatami fills as split lines, so they show embossed in the
//! stitching (a star, a leaf with its veins, a paw print). Each is drawn here in code, original
//! art for StitchFoundry, as lines in a box one unit across centred on the origin, y down.

use std::f64::consts::{PI, TAU};

use serde::{Deserialize, Serialize};

use crate::geom::Point;

/// A carving stamp from the library.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stamp {
    Star,
    Heart,
    Flower,
    Leaf,
    Spiral,
    Sun,
    Moon,
    Snowflake,
    Paw,
    Swirl,
    Shell,
    Butterfly,
    Clover,
    Feather,
    Rings,
    Diamond,
}

/// Points round an ellipse centred on `c`, its axes `rx` and `ry` turned by `turn` radians,
/// closed (the last point back at the first).
fn ellipse(c: (f64, f64), rx: f64, ry: f64, turn: f64) -> Vec<Point> {
    let (s, k) = turn.sin_cos();
    (0..=48)
        .map(|i| f64::from(i) * TAU / 48.0)
        .map(|a| {
            let (x, y) = (rx * a.cos(), ry * a.sin());
            Point::new(c.0 + x * k - y * s, c.1 + x * s + y * k)
        })
        .collect()
}

/// Points along `f` for `n + 1` evenly spaced values from `a` to `b`.
fn curve(a: f64, b: f64, n: u32, f: impl Fn(f64) -> (f64, f64)) -> Vec<Point> {
    (0..=n)
        .map(|i| {
            let (x, y) = f(a + (b - a) * f64::from(i) / f64::from(n));
            Point::new(x, y)
        })
        .collect()
}

fn line(a: (f64, f64), b: (f64, f64)) -> Vec<Point> {
    vec![Point::new(a.0, a.1), Point::new(b.0, b.1)]
}

/// A polygon through `pts`, closed.
fn closed(pts: &[(f64, f64)]) -> Vec<Point> {
    pts.iter().chain(pts.first()).map(|&(x, y)| Point::new(x, y)).collect()
}

impl Stamp {
    pub const ALL: [Stamp; 16] = [
        Stamp::Star,
        Stamp::Heart,
        Stamp::Flower,
        Stamp::Leaf,
        Stamp::Spiral,
        Stamp::Sun,
        Stamp::Moon,
        Stamp::Snowflake,
        Stamp::Paw,
        Stamp::Swirl,
        Stamp::Shell,
        Stamp::Butterfly,
        Stamp::Clover,
        Stamp::Feather,
        Stamp::Rings,
        Stamp::Diamond,
    ];

    /// The stamp's id in commands and files.
    pub fn id(self) -> &'static str {
        match self {
            Stamp::Star => "star",
            Stamp::Heart => "heart",
            Stamp::Flower => "flower",
            Stamp::Leaf => "leaf",
            Stamp::Spiral => "spiral",
            Stamp::Sun => "sun",
            Stamp::Moon => "moon",
            Stamp::Snowflake => "snowflake",
            Stamp::Paw => "paw",
            Stamp::Swirl => "swirl",
            Stamp::Shell => "shell",
            Stamp::Butterfly => "butterfly",
            Stamp::Clover => "clover",
            Stamp::Feather => "feather",
            Stamp::Rings => "rings",
            Stamp::Diamond => "diamond",
        }
    }

    /// The stamp's name in menus.
    pub fn label(self) -> &'static str {
        match self {
            Stamp::Star => "Star",
            Stamp::Heart => "Heart",
            Stamp::Flower => "Flower",
            Stamp::Leaf => "Leaf",
            Stamp::Spiral => "Spiral",
            Stamp::Sun => "Sun",
            Stamp::Moon => "Moon",
            Stamp::Snowflake => "Snowflake",
            Stamp::Paw => "Paw Print",
            Stamp::Swirl => "Swirl",
            Stamp::Shell => "Shell",
            Stamp::Butterfly => "Butterfly",
            Stamp::Clover => "Clover",
            Stamp::Feather => "Feather",
            Stamp::Rings => "Rings",
            Stamp::Diamond => "Diamond",
        }
    }

    pub fn from_id(id: &str) -> Option<Stamp> {
        Stamp::ALL.into_iter().find(|s| s.id().eq_ignore_ascii_case(id))
    }

    /// The stamp's lines, in a box one unit across centred on the origin (y down); a closed
    /// line ends where it starts.
    pub fn lines(self) -> Vec<Vec<Point>> {
        match self {
            Stamp::Star => {
                let pts: Vec<(f64, f64)> = (0..10)
                    .map(|k| {
                        let a = -PI / 2.0 + f64::from(k) * PI / 5.0;
                        let r = if k % 2 == 0 { 0.5 } else { 0.2 };
                        (r * a.cos(), r * a.sin() + 0.05)
                    })
                    .collect();
                vec![closed(&pts)]
            }
            Stamp::Heart => {
                // the classic parametric heart, 32 units across and 29 high, its tip down
                vec![curve(0.0, TAU, 96, |t| {
                    let x = 16.0 * t.sin().powi(3);
                    let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                    (x / 32.0, -(y + 2.54) / 32.0)
                })]
            }
            Stamp::Flower => vec![
                // five soft petals round a middle
                curve(0.0, TAU, 160, |a| {
                    let r = 0.5 * (0.72 + 0.28 * (5.0 * a).cos());
                    (r * (a - PI / 2.0).cos(), r * (a - PI / 2.0).sin())
                }),
                ellipse((0.0, 0.0), 0.11, 0.11, 0.0),
            ],
            Stamp::Leaf => {
                // an outline from tip to stalk and back, a midrib and three pairs of veins
                let mut outline = curve(0.0, 1.0, 40, |s| (0.3 * (PI * s).sin(), -0.5 + s));
                outline.extend(curve(1.0, 0.0, 40, |s| (-0.3 * (PI * s).sin(), -0.5 + s)).into_iter().skip(1));
                let mut lines = vec![outline, line((0.0, 0.45), (0.0, -0.42))];
                for y in [0.2, -0.02, -0.22] {
                    // each vein reaching three quarters of the way to the edge
                    let end = y - 0.14;
                    let x = 0.75 * 0.3 * (PI * (end + 0.5)).sin();
                    lines.push(line((0.0, y), (x, end)));
                    lines.push(line((0.0, y), (-x, end)));
                }
                lines
            }
            Stamp::Spiral => vec![curve(0.0, 3.0 * TAU, 180, |a| {
                let r = 0.5 * a / (3.0 * TAU);
                (r * a.cos(), r * a.sin())
            })],
            Stamp::Sun => {
                let mut lines = vec![ellipse((0.0, 0.0), 0.22, 0.22, 0.0)];
                for k in 0..12 {
                    let a = f64::from(k) * TAU / 12.0;
                    lines.push(line((0.29 * a.cos(), 0.29 * a.sin()), (0.5 * a.cos(), 0.5 * a.sin())));
                }
                lines
            }
            Stamp::Moon => {
                // a crescent: the outer circle's left side, then back round a circle bitten
                // out of it, meeting at 60° above and below
                let mut pts = curve(PI / 3.0, 5.0 * PI / 3.0, 48, |a| (0.5 * a.cos(), 0.5 * a.sin()));
                let r = (0.05f64 * 0.05 + 0.75 * 0.25).sqrt();
                let end = (-(0.25 * 3f64.sqrt())).atan2(0.05);
                pts.extend(curve(end, -end - TAU, 48, |a| (0.2 + r * a.cos(), r * a.sin())).into_iter().skip(1));
                vec![pts]
            }
            Stamp::Snowflake => {
                let mut lines = Vec::new();
                for k in 0..6 {
                    let a = -PI / 2.0 + f64::from(k) * TAU / 6.0;
                    let at = |r: f64, turn: f64| (r * (a + turn).cos(), r * (a + turn).sin());
                    lines.push(line((0.0, 0.0), at(0.5, 0.0)));
                    let fork = at(0.3, 0.0);
                    for side in [-1.0, 1.0] {
                        let b = a + side * 0.7;
                        lines.push(line(fork, (fork.0 + 0.15 * b.cos(), fork.1 + 0.15 * b.sin())));
                    }
                }
                lines
            }
            Stamp::Paw => vec![
                ellipse((0.0, 0.17), 0.24, 0.19, 0.0),
                ellipse((-0.33, -0.1), 0.09, 0.12, -0.35),
                ellipse((-0.12, -0.32), 0.1, 0.13, -0.1),
                ellipse((0.12, -0.32), 0.1, 0.13, 0.1),
                ellipse((0.33, -0.1), 0.09, 0.12, 0.35),
            ],
            Stamp::Swirl => {
                // two curls, one turned round the middle from the other, joined in an S
                let left = curve(0.0, 3.0 * PI, 90, |a| {
                    let r = 0.04 + 0.21 * a / (3.0 * PI);
                    (-0.25 + r * (a + PI).cos(), r * (a + PI).sin())
                });
                let mut pts = left.clone();
                pts.extend(left.iter().rev().skip(1).map(|p| Point::new(-p.x, -p.y)));
                vec![pts]
            }
            Stamp::Shell => {
                // a fan: ribs from the hinge to a scalloped edge
                let hinge = (0.0, 0.45);
                let edge = |a: f64| (0.5 * a.cos(), 0.12 + 0.55 * a.sin());
                let (a0, a1) = (PI + 0.35, TAU - 0.35);
                let mut outline = vec![Point::new(hinge.0, hinge.1)];
                outline.extend(curve(a0, a1, 70, |a| {
                    let (x, y) = edge(a);
                    let bump = 0.03 * (((a - a0) / (a1 - a0) * 7.0 * PI).sin()).abs();
                    (x * (1.0 + bump), y - bump * 0.5)
                }));
                outline.push(Point::new(hinge.0, hinge.1));
                let mut lines = vec![outline];
                for k in 1..7 {
                    let a = a0 + (a1 - a0) * f64::from(k) / 7.0;
                    lines.push(line(hinge, edge(a)));
                }
                lines
            }
            Stamp::Butterfly => {
                let mut lines = vec![line((0.0, -0.3), (0.0, 0.35)), line((0.0, -0.3), (-0.12, -0.48)), line((0.0, -0.3), (0.12, -0.48))];
                for side in [-1.0, 1.0] {
                    lines.push(ellipse((side * 0.24, -0.13), 0.22, 0.15, side * -0.5));
                    lines.push(ellipse((side * 0.18, 0.2), 0.15, 0.11, side * 0.5));
                }
                lines
            }
            Stamp::Clover => {
                let mut lines: Vec<Vec<Point>> =
                    [(0.0, -0.2), (0.2, 0.0), (0.0, 0.2), (-0.2, 0.0)].iter().map(|&c| ellipse(c, 0.17, 0.17, 0.0)).collect();
                lines.push(curve(0.0, 1.0, 12, |s| (0.12 * s * s, 0.35 + 0.15 * s)));
                lines
            }
            Stamp::Feather => {
                // a gently curved shaft with barbs either side, longest in the middle
                let shaft = |s: f64| (0.06 * (PI * s).sin(), 0.5 - s);
                let mut lines = vec![curve(0.0, 1.0, 40, shaft)];
                for k in 1..12 {
                    let s = 0.1 + 0.8 * f64::from(k) / 12.0;
                    let (x, y) = shaft(s);
                    let w = 0.28 * (PI * (s - 0.05) / 0.95).sin();
                    lines.push(line((x, y), (x + w, y - w * 0.6)));
                    lines.push(line((x, y), (x - w, y - w * 0.6)));
                }
                lines
            }
            Stamp::Rings => [0.16, 0.33, 0.5].iter().map(|&r| ellipse((0.0, 0.0), r, r, 0.0)).collect(),
            Stamp::Diamond => {
                vec![closed(&[(0.0, -0.5), (0.36, 0.0), (0.0, 0.5), (-0.36, 0.0)]), closed(&[(0.0, -0.25), (0.18, 0.0), (0.0, 0.25), (-0.18, 0.0)])]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stamp_fits_its_box_and_has_lines() {
        let mut seen = std::collections::HashSet::new();
        for s in Stamp::ALL {
            let lines = s.lines();
            assert!(!lines.is_empty() && lines.iter().all(|l| l.len() >= 2), "{s:?}");
            for p in lines.iter().flatten() {
                assert!(p.is_finite() && p.x.abs() <= 0.52 && p.y.abs() <= 0.52, "{s:?}: {p:?}");
            }
            // each reaches most of its box
            let (w, h) = lines.iter().flatten().fold((0.0f64, 0.0f64), |(w, h), p| (w.max(p.x.abs()), h.max(p.y.abs())));
            assert!(w.max(h) > 0.4, "{s:?} is small: {w} {h}");
            assert_eq!(Stamp::from_id(s.id()), Some(s));
            assert!(seen.insert(s.label()));
        }
        assert_eq!(Stamp::from_id("nope"), None);
        assert_eq!(Stamp::from_id("PAW"), Some(Stamp::Paw));
    }

    #[test]
    fn the_crescent_and_the_swirl_are_whole_lines() {
        // the moon's two arcs meet: it closes
        let moon = Stamp::Moon.lines();
        let (first, last) = (moon[0][0], *moon[0].last().unwrap());
        assert!(first.distance(last) < 1e-6, "{first:?} {last:?}");
        // the swirl turns about its middle: each point has its opposite
        let swirl = &Stamp::Swirl.lines()[0];
        for p in swirl {
            assert!(swirl.iter().any(|q| q.distance(Point::new(-p.x, -p.y)) < 1e-9));
        }
    }
}
