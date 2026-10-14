//! Motif fill: rows of a small repeating pattern (zigzag, wave, diamonds or scales) sewn as
//! running stitch across the area, at the motif angle, clipped to the area (even-odd, so holes
//! stay empty). The patterns are drawn here in code.
//!
//! Each row is the pattern's curve repeated along the row, `spacing` below the last. Diamonds
//! mirror every other row so neighbouring rows meet at their points; scales shift every other row
//! by half a motif. The pieces of the rows inside the area are sewn one after another, each next
//! piece the nearest unsewn one in this row or the next whose start can be reached with a short
//! stitch inside the shape; anything further starts a new section with a jump.

use kurbo::{Point, Vec2};

use crate::fill::Region;
use crate::geometry::Polyline;
use crate::settings::{MotifPattern, MotifSettings};
use crate::{MAX_STITCHES, Stitch, StitchKind, geometry};

/// Rows are capped so a tiny spacing in a huge shape can't run away.
const MAX_ROWS: usize = 5_000;
/// Points per motif on curved patterns.
const CURVE_STEPS: usize = 8;
/// Turns sharper than this keep a penetration on them, so curved motifs keep their shape.
const CORNER_DEG: f64 = 10.0;
/// Pieces shorter than this are not sewn, mm.
const MIN_PIECE: f64 = 0.5;

/// Motif fill of the area inside `rings` (outer rings and holes).
pub fn motif(rings: &[Polyline], s: &MotifSettings) -> Vec<Stitch> {
    let mut out = vec![];
    let ok = |v: f64, lo: f64, def: f64| if v.is_finite() && v >= lo { v } else { def };
    let d = MotifSettings::default();
    let (w, h) = (ok(s.width, 0.5, d.width), ok(s.height, 0.0, d.height));
    let spacing = ok(s.spacing, 0.3, d.spacing);
    let length = ok(s.length, 0.3, d.length);
    let a = if s.angle.is_finite() { s.angle.to_radians() } else { 0.0 };
    // Row space: u along the rows, v across them (rows at `angle` counter-clockwise on screen).
    let (du, dv) = (Vec2::new(a.cos(), -a.sin()), Vec2::new(a.sin(), a.cos()));
    let to_world = |u: f64, v: f64| (du * u + dv * v).to_point();

    let pts: Vec<Point> = rings.iter().flat_map(|r| r.points.iter()).filter(|p| p.x.is_finite() && p.y.is_finite()).copied().collect();
    if pts.is_empty() {
        return out;
    }
    let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in &pts {
        let (u, v) = (p.to_vec2().dot(du), p.to_vec2().dot(dv));
        (u0, u1, v0, v1) = (u0.min(u), u1.max(u), v0.min(v), v1.max(v));
    }
    let region = Region::new(rings);
    let edges = edges(rings);
    // Rows centred across the shape, reaching past both sides by the pattern's height.
    let count = (((v1 - v0 + h) / spacing).floor() as usize + 1).min(MAX_ROWS);
    let first = (v0 + v1) / 2.0 - (count - 1) as f64 * spacing / 2.0;
    // Motifs line up on a grid fixed to the shape, starting a motif before its left edge.
    let start = (u0 / w).floor() * w - w;
    let cells = (((u1 - start) / w).ceil() as usize + 1).min(100_000);
    let rows: Vec<Vec<Vec<Point>>> = (0..count)
        .map(|k| {
            let v = first + k as f64 * spacing;
            let curve: Vec<Point> = row_curve(s.pattern, k, w, h, cells).into_iter().map(|(u, dv)| to_world(start + u, v + dv)).collect();
            clip(&curve, &edges, &region).into_iter().filter(|p| geometry::path_length(p) >= MIN_PIECE).collect()
        })
        .collect();

    let reach = 2.0 * length + spacing + h;
    sew_rows(&rows, &region, &RowSewing { length, reach, triple: s.triple, staggers: 1, starts: &[] }, &mut out);
    out
}

/// How [`sew_rows`] sews.
pub(crate) struct RowSewing<'a> {
    /// Running stitch length, mm.
    pub length: f64,
    /// Furthest the needle steps from one piece to the next inside the shape, mm.
    pub reach: f64,
    /// Each stitch three times.
    pub triple: bool,
    /// Penetrations step along by 1/staggers of a stitch from row to row, so they don't line
    /// up (1: each piece's stitches start where the piece does).
    pub staggers: usize,
    /// Where each piece starts along its row's curve (`starts[row][piece]`, mm), when rows are
    /// copies of one curve: penetrations then sit on a grid along the curve, staggered row by
    /// row, as tatami's do. Empty: the stagger counts from each piece's start.
    pub starts: &'a [Vec<f64>],
}

/// Sew rows of pieces (each row's pieces inside the area): each next piece the nearest unsewn
/// one in this row or the next whose start a short stitch inside the shape reaches, anything
/// further starting a new section with a jump.
pub(crate) fn sew_rows(rows: &[Vec<Vec<Point>>], region: &Region, o: &RowSewing, out: &mut Vec<Stitch>) {
    let (length, reach) = (o.length, o.reach);
    let mut used: Vec<Vec<bool>> = rows.iter().map(|r| vec![false; r.len()]).collect();
    let mut row = 0;
    let stitch = |piece: &[Point], k: usize, i: usize, rev: bool, out: &mut Vec<Stitch>| {
        let phase = if o.staggers > 1 { length * (k % o.staggers) as f64 / o.staggers as f64 } else { 0.0 };
        // Distance from the piece's start to its first penetration.
        let offset = match o.starts.get(k).and_then(|r| r.get(i)) {
            Some(s0) if rev => (s0 + geometry::path_length(piece) - phase).rem_euclid(length),
            Some(s0) => (phase - s0).rem_euclid(length),
            None => phase,
        };
        let st = if !o.starts.is_empty() {
            // Exactly on the grid: penetrations every `length` from the first, the last
            // stitch taking what is left.
            let total = geometry::path_length(piece);
            let mut st: Vec<Point> = piece.first().copied().into_iter().collect();
            let mut at = if offset < 0.05 { length } else { offset };
            while at < total - 0.05 && st.len() < MAX_STITCHES {
                st.push(geometry::point_at(piece, at));
                at += length;
            }
            st.extend(piece.last().copied());
            st
        } else if offset > 0.05 && geometry::path_length(piece) > offset + 0.05 {
            let mut st = vec![piece.first().copied().unwrap_or_default()];
            st.extend(geometry::resample(&tail(piece, offset), length, CORNER_DEG));
            st
        } else {
            geometry::resample(piece, length, CORNER_DEG)
        };
        if o.triple {
            for w in st.windows(2) {
                out.extend([w[1], w[0], w[1]].map(|p| Stitch::new(p, StitchKind::Top)));
            }
        } else {
            out.extend(st.into_iter().skip(1).map(|p| Stitch::new(p, StitchKind::Top)));
        }
    };
    loop {
        if out.len() >= MAX_STITCHES {
            return;
        }
        let needle = out.last().map(Stitch::point);
        // The nearest piece in this row or the next that a short stitch inside the shape reaches.
        let near = needle.and_then(|n| {
            (row..(row + 2).min(rows.len()))
                .flat_map(|k| (0..rows[k].len()).map(move |i| (k, i)))
                .filter(|&(k, i)| !used[k][i])
                .flat_map(|(k, i)| [(k, i, false), (k, i, true)])
                .filter_map(|(k, i, rev)| {
                    let p = &rows[k][i];
                    let s = if rev { p.last() } else { p.first() }.copied()?;
                    (s.distance(n) <= reach && region.runs_inside(n, s)).then_some((k, i, rev, s.distance(n)))
                })
                .min_by(|a, b| a.3.total_cmp(&b.3))
        });
        let (k, i, rev, jump) = match near {
            Some((k, i, rev, _)) => (k, i, rev, false),
            None => {
                // A new section: the first unsewn piece, from its end nearer the needle.
                let Some((k, i)) = (0..rows.len()).find_map(|k| used[k].iter().position(|u| !*u).map(|i| (k, i))) else { return };
                let p = &rows[k][i];
                let rev = match (needle, p.first(), p.last()) {
                    (Some(n), Some(a), Some(b)) => b.distance(n) < a.distance(n),
                    _ => false,
                };
                (k, i, rev, true)
            }
        };
        used[k][i] = true;
        row = k;
        let mut piece = rows[k][i].clone();
        if rev {
            piece.reverse();
        }
        let Some(&s0) = piece.first() else { continue };
        match needle {
            Some(n) if !jump => {
                // The short stitch over, split so none is longer than a stitch.
                let m = ((n.distance(s0) / length).ceil() as usize).clamp(1, 1_000);
                out.extend((1..=m).map(|j| Stitch::new(n.lerp(s0, j as f64 / m as f64), StitchKind::Top)));
            }
            _ => out.push(Stitch::new(s0, StitchKind::Jump)),
        }
        stitch(&piece, k, i, rev, out);
    }
}

/// `pts` from `dist` along it to its end.
fn tail(pts: &[Point], dist: f64) -> Vec<Point> {
    let mut walked = 0.0;
    for (i, w) in pts.windows(2).enumerate() {
        let l = w[0].distance(w[1]);
        if walked + l >= dist {
            let mut out = vec![w[0].lerp(w[1], if l > 0.0 { (dist - walked) / l } else { 0.0 })];
            out.extend(pts.iter().skip(i + 1).copied());
            return out;
        }
        walked += l;
    }
    pts.last().map(|p| vec![*p]).unwrap_or_default()
}

/// One row of the pattern in row space: (u from the row's start, offset across the row).
fn row_curve(pattern: MotifPattern, row: usize, w: f64, h: f64, cells: usize) -> Vec<(f64, f64)> {
    let odd = row % 2 == 1;
    let mut pts = vec![];
    for c in 0..cells {
        let x = c as f64 * w;
        match pattern {
            MotifPattern::Zigzag | MotifPattern::Diamonds => {
                // Diamonds: every other row mirrored, so rows meet point to point.
                let peak = if pattern == MotifPattern::Diamonds && odd { h / 2.0 } else { -h / 2.0 };
                pts.extend([(x, 0.0), (x + w / 4.0, peak), (x + w * 0.75, -peak)]);
            }
            MotifPattern::Wave => {
                pts.extend((0..CURVE_STEPS).map(|j| {
                    let t = j as f64 / CURVE_STEPS as f64;
                    (x + t * w, -h / 2.0 * (std::f64::consts::TAU * t).sin())
                }));
            }
            MotifPattern::Scales => {
                // Arches, every other row shifted half a motif.
                let shift = if odd { w / 2.0 } else { 0.0 };
                pts.extend((0..CURVE_STEPS).map(|j| {
                    let t = j as f64 / CURVE_STEPS as f64;
                    (x + shift + t * w, -h * (std::f64::consts::PI * t).sin())
                }));
            }
        }
    }
    let end = cells as f64 * w + if pattern == MotifPattern::Scales && odd { w / 2.0 } else { 0.0 };
    pts.push((end, 0.0));
    pts
}

/// The outline's edges, closed.
pub(crate) fn edges(rings: &[Polyline]) -> Vec<(Point, Point)> {
    let mut out = vec![];
    for r in rings {
        let p: Vec<Point> = r.points.iter().filter(|p| p.x.is_finite() && p.y.is_finite()).copied().collect();
        if p.len() >= 3 {
            out.extend((0..p.len()).map(|i| (p[i], p[(i + 1) % p.len()])));
        }
    }
    out
}

/// The pieces of `curve` inside the area: each segment is cut where it crosses the outline.
pub(crate) fn clip(curve: &[Point], edges: &[(Point, Point)], region: &Region) -> Vec<Vec<Point>> {
    clip_along(curve, edges, region).into_iter().map(|(p, _)| p).collect()
}

/// [`clip`], with how far along `curve` each piece starts (mm).
pub(crate) fn clip_along(curve: &[Point], edges: &[(Point, Point)], region: &Region) -> Vec<(Vec<Point>, f64)> {
    let mut pieces: Vec<(Vec<Point>, f64)> = vec![];
    let mut cur: Vec<Point> = vec![];
    let (mut base, mut cur_start) = (0.0, 0.0);
    for seg in curve.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let seg_len = a.distance(b);
        let mut ts: Vec<f64> = edges.iter().filter_map(|(c, d)| cross(a, b, *c, *d)).collect();
        ts.push(0.0);
        ts.push(1.0);
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-12);
        for t in ts.windows(2) {
            let (p, q) = (a.lerp(b, t[0]), a.lerp(b, t[1]));
            if region.contains(p.midpoint(q)) {
                if cur.last().is_none_or(|l| l.distance(p) > 1e-9) {
                    if !cur.is_empty() {
                        pieces.push((std::mem::take(&mut cur), cur_start));
                    }
                    cur.push(p);
                    cur_start = base + t[0] * seg_len;
                }
                cur.push(q);
            } else if !cur.is_empty() {
                pieces.push((std::mem::take(&mut cur), cur_start));
            }
        }
        base += seg_len;
    }
    if cur.len() >= 2 {
        pieces.push((cur, cur_start));
    }
    pieces.retain(|(p, _)| p.len() >= 2);
    pieces
}

/// Where along `a`→`b` (0 to 1) it crosses `c`→`d`, if it does.
fn cross(a: Point, b: Point, c: Point, d: Point) -> Option<f64> {
    let (r, s) = (b - a, d - c);
    let den = r.cross(s);
    if den.abs() < 1e-15 {
        return None;
    }
    let t = (c - a).cross(s) / den;
    let u = (c - a).cross(r) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, size: f64) -> Polyline {
        Polyline::new(vec![Point::new(x, y), Point::new(x + size, y), Point::new(x + size, y + size), Point::new(x, y + size)], true)
    }

    fn top(st: &[Stitch]) -> Vec<Point> {
        st.iter().filter(|p| p.kind == StitchKind::Top).map(Stitch::point).collect()
    }

    #[test]
    fn every_pattern_fills_the_square_and_stays_inside() {
        for pattern in MotifPattern::ALL {
            let s = MotifSettings { pattern, ..Default::default() };
            let st = motif(&[square(0.0, 0.0, 30.0)], &s);
            let pts = top(&st);
            assert!(pts.len() > 100, "{pattern:?}");
            assert!(pts.iter().all(|p| p.x > -1e-6 && p.x < 30.0 + 1e-6 && p.y > -1e-6 && p.y < 30.0 + 1e-6), "{pattern:?}");
            // Rows reach the whole square, and connect along the edge (one section).
            assert!(pts.iter().any(|p| p.y < 2.0) && pts.iter().any(|p| p.y > 28.0), "{pattern:?}");
            assert_eq!(st.iter().filter(|p| p.kind == StitchKind::Jump).count(), 1, "{pattern:?}");
            for w in st.windows(2).filter(|w| w[1].kind == StitchKind::Top) {
                let d = w[0].point().distance(w[1].point());
                assert!(d <= s.length * 1.5 + 1e-6, "{pattern:?} {d}");
            }
        }
    }

    #[test]
    fn zigzag_keeps_its_points() {
        let s = MotifSettings { pattern: MotifPattern::Zigzag, width: 4.0, height: 2.0, length: 3.0, ..Default::default() };
        let pts = top(&motif(&[square(0.0, 0.0, 30.0)], &s));
        // The sides (2.8 mm) are shorter than a stitch, so the needle goes in on the points,
        // which sit a quarter and three quarters of the way along each 4 mm motif.
        let on_point = pts.iter().filter(|p| ((p.x - 1.0).rem_euclid(2.0)).min(2.0 - (p.x - 1.0).rem_euclid(2.0)) < 1e-6).count();
        // (The rest are where rows meet the edge and the stitches between rows.)
        assert!(on_point * 10 >= pts.len() * 7, "{on_point} of {}", pts.len());
    }

    #[test]
    fn holes_stay_empty_and_bad_input_sews_nothing() {
        let s = MotifSettings::default();
        let st = motif(&[square(0.0, 0.0, 30.0), square(10.0, 10.0, 10.0)], &s);
        let inside = |p: Point, m: f64| p.x > 10.0 + m && p.x < 20.0 - m && p.y > 10.0 + m && p.y < 20.0 - m;
        assert!(top(&st).iter().all(|p| !inside(*p, 1e-6)));
        for w in st.windows(2).filter(|w| w[1].kind != StitchKind::Jump) {
            assert!(!inside(w[0].point().midpoint(w[1].point()), 0.3));
        }
        assert!(motif(&[], &s).is_empty());
        let nan = Polyline::new(vec![Point::new(f64::NAN, 0.0), Point::new(1.0, 1.0)], true);
        assert!(motif(&[nan], &s).iter().all(|p| p.x.is_finite()));
    }
}
