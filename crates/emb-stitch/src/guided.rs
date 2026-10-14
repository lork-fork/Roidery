//! Guided fill: tatami rows that follow a curve drawn across the shape, for fills that bend with
//! the design (a petal, a ribbon, hair) instead of running straight.
//!
//! Each row is a copy of the guide moved sideways (across the guide's start-to-end direction)
//! by the row spacing, its ends carried on straight past the shape; the copies are cut to the
//! area (even-odd, so holes stay empty) and sewn back and forth like the motif fill's rows, with
//! staggered penetrations. The underlay runs across the guide's overall direction.

use kurbo::{Point, Vec2};

use crate::fill::{self, Region};
use crate::geometry::Polyline;
use crate::motif::{RowSewing, clip_along, edges, sew_rows};
use crate::settings::{FillSettings, GuidedSettings};
use crate::{Stitch, geometry};

/// Rows are capped so a tiny spacing in a huge shape can't run away.
const MAX_ROWS: usize = 5_000;
/// Row pieces shorter than this are not sewn, mm.
const MIN_PIECE: f64 = 0.4;

/// The guide in millimetres: its points as fractions of the bounding box of `rings`.
pub fn guide_points(rings: &[Polyline], g: &GuidedSettings) -> Vec<Point> {
    let pts: Vec<Point> = rings.iter().flat_map(|r| r.points.iter()).filter(|p| p.x.is_finite() && p.y.is_finite()).copied().collect();
    let Some(first) = pts.first() else { return vec![] };
    let (mut min, mut max) = (*first, *first);
    for p in &pts {
        (min, max) = (Point::new(min.x.min(p.x), min.y.min(p.y)), Point::new(max.x.max(p.x), max.y.max(p.y)));
    }
    let mut out: Vec<Point> = g
        .guide
        .iter()
        .filter(|q| q[0].is_finite() && q[1].is_finite())
        .map(|q| Point::new(min.x + q[0] * (max.x - min.x), min.y + q[1] * (max.y - min.y)))
        .collect();
    out.dedup_by(|a, b| a.distance(*b) < 1e-6);
    out
}

/// Guided fill of the area inside `rings` along `g`'s guide; straight tatami without one.
pub fn guided(rings: &[Polyline], s: &FillSettings, g: &GuidedSettings) -> Vec<Stitch> {
    let guide = guide_points(rings, g);
    let (Some(&a), Some(&b)) = (guide.first(), guide.last()) else { return fill::tatami(rings, s) };
    let Some(dir) = unit(b - a) else { return fill::tatami(rings, s) };
    // Underlay across the guide's overall direction (the underlay runs at the fill angle + 90).
    let mut out = vec![];
    let chord_deg = (-dir.y).atan2(dir.x).to_degrees();
    fill::underlay(rings, &FillSettings { angle: chord_deg, ..s.clone() }, &mut out);

    let pts: Vec<Point> = rings.iter().flat_map(|r| r.points.iter()).filter(|p| p.x.is_finite() && p.y.is_finite()).copied().collect();
    if pts.len() < 3 {
        return out;
    }
    // The guide carried on straight past the shape at both ends.
    let (x0, x1, y0, y1) =
        pts.iter().fold((f64::MAX, f64::MIN, f64::MAX, f64::MIN), |(a, b, c, d), p| (a.min(p.x), b.max(p.x), c.min(p.y), d.max(p.y)));
    let reach = (x1 - x0).hypot(y1 - y0) + 1.0;
    let mut long = guide.clone();
    if let [p, q, ..] = guide.as_slice()
        && let Some(t) = unit(*p - *q)
    {
        long.insert(0, *p + t * reach);
    }
    if let [.., p, q] = guide.as_slice()
        && let Some(t) = unit(*q - *p)
    {
        long.push(*q + t * reach);
    }
    // Rows are the guide offset sideways (parallel curves), so they stay `spacing` apart however
    // the guide bends. Offsets reach every point of the shape on both sides.
    let normals = geometry::miter_normals(&long);
    let spacing = s.row_spacing.max(0.1);
    let region = Region::new(rings);
    // The furthest the shape reaches either side of the guide: over its outline and a grid
    // inside it (the furthest point can be inside, deep in a bend).
    let step = ((x1 - x0).hypot(y1 - y0) / 150.0).max(0.25);
    let (nx, ny) = (((x1 - x0) / step).ceil() as usize, ((y1 - y0) / step).ceil() as usize);
    let inside = (0..=ny.min(400))
        .flat_map(|j| (0..=nx.min(400)).map(move |i| Point::new(x0 + i as f64 * step, y0 + j as f64 * step)))
        .filter(|p| region.contains(*p));
    let (lo, hi) =
        pts.iter().copied().chain(inside).map(|p| side_distance(&long, p)).fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let count = (((hi - lo) / spacing).floor() as usize + 1).min(MAX_ROWS);
    let edges = edges(rings);
    let (rows, starts): (Vec<Vec<Vec<Point>>>, Vec<Vec<f64>>) = (0..count)
        .map(|k| {
            let curve = offset(&long, &normals, lo + (k as f64 + 0.5) * spacing);
            clip_along(&curve, &edges, &region).into_iter().filter(|(p, _)| geometry::path_length(p) >= MIN_PIECE).unzip()
        })
        .unzip();
    let length = s.stitch_length.max(0.5);
    let o = RowSewing { length, reach: 2.0 * length + spacing, triple: false, staggers: s.staggers.max(1) as usize, starts: &starts };
    sew_rows(&rows, &region, &o, &mut out);
    out
}

/// `line` moved `d` mm sideways (to the left of travel, y down, for d > 0). Where it would
/// fold back on itself (past the centre of a bend), the folded stretch is left out.
fn offset(line: &[Point], normals: &[Vec2], d: f64) -> Vec<Point> {
    let mut out: Vec<Point> = vec![];
    let mut prev: Option<Point> = None;
    for (p, n) in line.iter().zip(normals) {
        let q = *p + *n * d;
        if let (Some(a), Some(last)) = (prev, out.last()) {
            // Keep only stretches that run the way the guide does.
            if (q - *last).dot(*p - a) <= 0.0 {
                prev = Some(*p);
                continue;
            }
        }
        out.push(q);
        prev = Some(*p);
    }
    out
}

/// How far `p` is from `line`, signed like [`offset`]'s distance.
fn side_distance(line: &[Point], p: Point) -> f64 {
    let mut best = (f64::INFINITY, 0.0);
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let ab = b - a;
        let l2 = ab.hypot2();
        if l2 < 1e-18 {
            continue;
        }
        let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
        let q = a + ab * t;
        let d = q.distance(p);
        if d < best.0 {
            // Left of travel (y down) is the normal (dy, -dx).
            let side = (p - q).dot(Vec2::new(ab.y, -ab.x));
            best = (d, if side >= 0.0 { d } else { -d });
        }
    }
    best.1
}

fn unit(v: Vec2) -> Option<Vec2> {
    let l = v.hypot();
    (l > 1e-9 && l.is_finite()).then(|| v / l)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StitchKind;

    fn square(x: f64, y: f64, size: f64) -> Polyline {
        Polyline::new(vec![Point::new(x, y), Point::new(x + size, y), Point::new(x + size, y + size), Point::new(x, y + size)], true)
    }

    fn top(st: &[Stitch]) -> Vec<Point> {
        st.iter().filter(|p| p.kind == StitchKind::Top).map(Stitch::point).collect()
    }

    #[test]
    fn rows_follow_an_arc_guide_and_fill_the_square() {
        // An arch across the 30 mm square: down at the sides, up in the middle.
        let arc: Vec<[f64; 2]> = (0..=10).map(|i| i as f64 / 10.0).map(|t| [t, 0.5 + 0.4 * (2.0 * t - 1.0).powi(2)]).collect();
        let g = GuidedSettings { guide: arc };
        let s = FillSettings { underlay: false, ..Default::default() };
        let st = guided(&[square(0.0, 0.0, 30.0)], &s, &g);
        let pts = top(&st);
        assert!(pts.len() > 300);
        assert!(pts.iter().all(|p| p.x > -1e-6 && p.x < 30.0 + 1e-6 && p.y > -1e-6 && p.y < 30.0 + 1e-6));
        // The whole square is reached.
        for (x, y) in [(2.0, 2.0), (28.0, 2.0), (15.0, 15.0), (2.0, 28.0), (28.0, 28.0)] {
            assert!(pts.iter().any(|p| p.distance(Point::new(x, y)) < 2.0), "({x}, {y})");
        }
        // Rows bend with the guide: stitches near the sides run steeply, in the middle flat.
        let slope = |x0: f64, x1: f64| {
            let v: Vec<f64> = st
                .windows(2)
                .filter(|w| w[1].kind == StitchKind::Top && w[0].x > x0 && w[0].x < x1 && w[0].point().distance(w[1].point()) > 1.5)
                .map(|w| ((w[1].y - w[0].y) / (w[1].x - w[0].x)).abs())
                .filter(|m| m.is_finite())
                .collect();
            v.iter().sum::<f64>() / v.len().max(1) as f64
        };
        assert!(slope(1.0, 6.0) > 2.0 * slope(13.0, 17.0), "{} {}", slope(1.0, 6.0), slope(13.0, 17.0));
    }

    #[test]
    fn rows_stay_a_row_apart_where_the_guide_turns_steep() {
        // A half circle standing on the square's bottom edge: its legs run straight up.
        let arc: Vec<[f64; 2]> =
            (0..=32).map(|i| std::f64::consts::PI * i as f64 / 32.0).map(|t| [0.5 - 0.45 * t.cos(), 1.0 - 0.9 * t.sin()]).collect();
        let s = FillSettings { underlay: false, ..Default::default() };
        let st = guided(&[square(0.0, 0.0, 30.0)], &s, &GuidedSettings { guide: arc });
        // Thread laid is about the area over the row spacing (rows that bunched up would lay
        // far more).
        let sewn: f64 = st.windows(2).filter(|w| w[1].kind == StitchKind::Top).map(|w| w[0].point().distance(w[1].point())).sum();
        let ideal = 30.0 * 30.0 / s.row_spacing;
        assert!(sewn > ideal * 0.85 && sewn < ideal * 1.25, "{sewn} vs {ideal}");
    }

    #[test]
    fn penetrations_sit_on_a_grid_staggered_row_by_row() {
        let g = GuidedSettings { guide: vec![[0.0, 0.5], [1.0, 0.5]] };
        let s = FillSettings { underlay: false, stitch_length: 3.0, staggers: 4, ..Default::default() };
        let st = guided(&[square(0.0, 0.0, 30.0)], &s, &g);
        // Penetrations away from the edges, by row (rows are horizontal here).
        let mut rows: std::collections::BTreeMap<i64, Vec<f64>> = Default::default();
        for p in top(&st).iter().filter(|p| p.x > 1.0 && p.x < 29.0) {
            rows.entry((p.y * 100.0).round() as i64).or_default().push(p.x);
        }
        let phase = |xs: &[f64]| xs.first().map(|x| x.rem_euclid(3.0)).unwrap();
        let rows: Vec<Vec<f64>> = rows.into_values().filter(|v| v.len() > 4).collect();
        assert!(rows.len() > 50);
        for r in &rows {
            assert!(r.iter().all(|x| ((x - r[0]).rem_euclid(3.0)).min(3.0 - (x - r[0]).rem_euclid(3.0)) < 1e-6), "{r:?}");
        }
        for w in rows.windows(2) {
            let d = (phase(&w[1]) - phase(&w[0])).rem_euclid(3.0);
            assert!((d - 0.75).abs() < 1e-6 || (d - 2.25).abs() < 1e-6, "{d}");
        }
    }

    #[test]
    fn without_a_guide_it_is_tatami_and_holes_stay_empty() {
        let s = FillSettings::default();
        let sq = [square(0.0, 0.0, 30.0)];
        assert_eq!(guided(&sq, &s, &GuidedSettings::default()), fill::tatami(&sq, &s));
        let g = GuidedSettings { guide: vec![[0.0, 0.2], [0.5, 0.5], [1.0, 0.2]] };
        let st = guided(&[square(0.0, 0.0, 30.0), square(10.0, 10.0, 10.0)], &s, &g);
        let inside = |p: Point, m: f64| p.x > 10.0 + m && p.x < 20.0 - m && p.y > 10.0 + m && p.y < 20.0 - m;
        assert!(top(&st).iter().all(|p| !inside(*p, 1e-6)));
        assert!(st.iter().any(|p| p.kind == StitchKind::Underlay));
        let nan = GuidedSettings { guide: vec![[f64::NAN, 0.0], [1.0, 1.0]] };
        assert_eq!(guided(&sq, &s, &nan), fill::tatami(&sq, &s));
    }
}
