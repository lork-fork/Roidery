//! Circular fill: rows are circles around a centre (the middle of the shape's bounds, moved by
//! the circular settings), `row_spacing` apart, clipped to the area (even-odd, so holes stay
//! empty).
//!
//! Rows are sewn from the centre out. Where a row is a whole circle and so is the next, the row
//! spirals out into the next as it goes round, so rings sew as one spiral with no step lines.
//! Arcs (rows the outline cuts) go back and forth like tatami rows, lengthened by the pull
//! compensation at both ends; a row follows the one before when the two overlap and its start is
//! close, and otherwise a new section starts with a jump. Penetrations sit on a grid of angles a
//! stitch apart, shifted by `1/staggers` of a stitch each row.

use std::f64::consts::TAU;

use kurbo::{Point, Vec2};

use crate::fill::{self, Region};
use crate::geometry::Polyline;
use crate::settings::{CircularSettings, FillSettings};
use crate::{MAX_STITCHES, Stitch, StitchKind};

/// Rows are capped so a tiny spacing in a huge shape can't run away.
const MAX_ROWS: usize = 20_000;
/// A grid penetration closer than this fraction of a stitch to an arc's end is skipped.
const MIN_END_FRACTION: f64 = 0.3;
/// Arcs shorter than this are not sewn, mm.
const MIN_ARC: f64 = 0.2;
/// Running stitch length for moves inside the shape, mm.
const TRAVEL_STITCH: f64 = 2.5;
/// A row follows the one before when its start is within this many stitches of the needle.
const MAX_STEP_STITCHES: f64 = 2.0;

/// One row's piece inside the area: angles `a` to `b` (radians, `b > a`), or the whole circle.
#[derive(Clone, Copy, Debug)]
struct Arc {
    a: f64,
    b: f64,
    full: bool,
}

/// Circular fill of the area inside `rings` (outer rings and holes).
pub fn circular(rings: &[Polyline], s: &FillSettings, c: &CircularSettings) -> Vec<Stitch> {
    let mut out = vec![];
    fill::underlay(rings, s, &mut out);
    let pts = || rings.iter().flat_map(|r| r.points.iter()).filter(|p| p.x.is_finite() && p.y.is_finite());
    let Some(first) = pts().next() else { return out };
    let bounds = pts().fold(kurbo::Rect::from_points(*first, *first), |r, p| r.union_pt(*p));
    let off = |v: f64| if v.is_finite() { v } else { 0.0 };
    let centre = bounds.center() + Vec2::new(off(c.center_x), off(c.center_y));
    let region = Region::new(rings);
    let spacing = if s.row_spacing.is_finite() && s.row_spacing > 0.05 { s.row_spacing } else { 0.4 };
    let length = if s.stitch_length.is_finite() && s.stitch_length > 0.2 { s.stitch_length } else { 3.0 };
    let pc = if s.pull_compensation.is_finite() { s.pull_compensation } else { 0.0 };
    let staggers = s.staggers.max(1) as usize;

    let reach = pts().map(|p| p.distance(centre)).fold(0.0, f64::max);
    let count = ((reach / spacing).floor() as usize + 1).min(MAX_ROWS);
    let radius = |k: usize| spacing * (k as f64 + 0.5);
    let rows: Vec<Vec<Arc>> = (0..count)
        .map(|k| {
            let r = radius(k);
            clip(rings, &region, centre, r)
                .into_iter()
                .filter_map(|arc| {
                    if arc.full {
                        return Some(arc);
                    }
                    // Negative compensation never turns an arc inside out.
                    let e = (pc / r).max(-(arc.b - arc.a) / 2.0 + MIN_ARC / r / 2.0);
                    let (a, b) = (arc.a - e, arc.b + e);
                    (r * (b - a) >= MIN_ARC).then_some(Arc { a, b, full: false })
                })
                .collect()
        })
        .collect();

    let polar = |r: f64, t: f64| centre + Vec2::new(r * t.cos(), r * t.sin());
    let angle_of = |p: Point| (p.y - centre.y).atan2(p.x - centre.x);
    let mut used: Vec<Vec<bool>> = rows.iter().map(|r| vec![false; r.len()]).collect();
    let mut first_section = true;
    let mut start_row = 0;
    loop {
        if out.len() >= MAX_STITCHES {
            return out;
        }
        while used.get(start_row).is_some_and(|u| u.iter().all(|x| *x)) {
            start_row += 1;
        }
        let Some(j) = used.get(start_row).and_then(|u| u.iter().position(|x| !*x)) else { return out };
        let (mut k, mut arc) = (start_row, rows[start_row][j]);
        used[k][j] = true;
        // Start at the end nearer the needle (a whole circle: at the needle's angle).
        let needle = out.last().map(Stitch::point);
        let r = radius(k);
        let (mut from, mut forward) = match (arc.full, needle) {
            (true, Some(p)) => (angle_of(p), true),
            (true, None) => (0.0, true),
            (false, Some(p)) => {
                let fwd = polar(r, arc.a).distance(p) <= polar(r, arc.b).distance(p);
                (if fwd { arc.a } else { arc.b }, fwd)
            }
            (false, None) => (arc.a, true),
        };
        let start = polar(r, from);
        // Moves before the top fill starts are covered by it, so they can run inside.
        match needle {
            Some(p) if first_section && region.runs_inside(p, start) => {
                let n = ((p.distance(start) / TRAVEL_STITCH).ceil() as usize).clamp(1, 10_000);
                out.extend((1..=n).map(|m| Stitch::new(p.lerp(start, m as f64 / n as f64), StitchKind::Travel)));
            }
            _ => out.push(Stitch::new(start, StitchKind::Jump)),
        }
        first_section = false;
        loop {
            let r = radius(k);
            // A whole circle followed by a whole circle spirals out into it.
            let spiral = arc.full && rows.get(k + 1).is_some_and(|n| n.len() == 1 && n[0].full) && !used[k + 1][0];
            let to = if arc.full {
                from + if forward { TAU } else { -TAU }
            } else if forward {
                arc.b
            } else {
                arc.a
            };
            let r_end = if spiral { radius(k + 1) } else { r };
            sew_arc(&polar, (r, r_end), (from, to), k % staggers, staggers, length, &mut out);
            if out.len() >= MAX_STITCHES {
                return out;
            }
            if spiral {
                k += 1;
                used[k][0] = true;
                arc = rows[k][0];
                from = to;
                continue;
            }
            // The next row: an unsewn piece overlapping this one that starts near the needle.
            let Some(next) = rows.get(k + 1) else { break };
            let at = polar(r, to);
            let nr = radius(k + 1);
            let best = next
                .iter()
                .enumerate()
                .filter(|(i, n)| !used[k + 1][*i] && overlaps(&arc, n))
                .flat_map(|(i, n)| if n.full { vec![(i, to, forward)] } else { vec![(i, n.a, true), (i, n.b, false)] })
                .map(|(i, t, fwd)| (i, t, fwd, polar(nr, t).distance(at)))
                .filter(|c| c.3 <= MAX_STEP_STITCHES * length + spacing)
                .min_by(|x, y| x.3.total_cmp(&y.3));
            let Some((i, t, fwd, _)) = best else { break };
            k += 1;
            used[k][i] = true;
            arc = next[i];
            from = t;
            forward = fwd;
            // Step out to the next row, split so no stitch is longer than a fill stitch.
            let target = polar(nr, t);
            let n = ((at.distance(target) / length).ceil() as usize).clamp(1, 10_000);
            out.extend((1..=n).map(|m| Stitch::new(at.lerp(target, m as f64 / n as f64), StitchKind::Top)));
        }
    }
}

/// The penetrations after an arc's start, up to and including its end: from angle `t.0` to `t.1`
/// with the radius going from `r.0` to `r.1` (a spiral when they differ).
fn sew_arc(polar: &impl Fn(f64, f64) -> Point, r: (f64, f64), t: (f64, f64), row: usize, staggers: usize, length: f64, out: &mut Vec<Stitch>) {
    let (t0, t1) = t;
    let span = t1 - t0;
    if span.abs() < 1e-12 {
        return;
    }
    // Small circles near the centre still get six stitches round.
    let step = (length / ((r.0 + r.1) / 2.0).max(1e-6)).min(TAU / 6.0);
    let phase = (row % staggers) as f64 / staggers as f64;
    let margin = step * MIN_END_FRACTION;
    let (lo_t, hi_t) = (t0.min(t1), t0.max(t1));
    let lo = ((lo_t + margin) / step - phase).ceil();
    let hi = ((hi_t - margin) / step - phase).floor();
    let mut grid: Vec<f64> = vec![];
    if lo.is_finite() && hi.is_finite() && hi >= lo && hi - lo < 1e6 {
        grid = (lo as i64..=hi as i64).map(|m| (m as f64 + phase) * step).collect();
    }
    if span < 0.0 {
        grid.reverse();
    }
    for a in grid.into_iter().chain([t1]) {
        let f = (a - t0) / span;
        out.push(Stitch::new(polar(r.0 + (r.1 - r.0) * f, a), StitchKind::Top));
    }
}

/// Whether two pieces of neighbouring rows share angles.
fn overlaps(x: &Arc, y: &Arc) -> bool {
    x.full || y.full || [-TAU, 0.0, TAU].iter().any(|s| y.a + s < x.b && y.b + s > x.a)
}

/// The pieces of the circle of radius `r` around `c` inside the area.
fn clip(rings: &[Polyline], region: &Region, c: Point, r: f64) -> Vec<Arc> {
    let mut ts: Vec<f64> = vec![];
    for ring in rings {
        let pts: Vec<Point> = ring.points.iter().filter(|p| p.x.is_finite() && p.y.is_finite()).copied().collect();
        if pts.len() < 3 {
            continue;
        }
        for (i, p) in pts.iter().enumerate() {
            let q = pts[(i + 1) % pts.len()];
            // |p + u (q - p) - c| = r for u in [0, 1).
            let (d, f) = (q - *p, *p - c);
            let (a, b, cc) = (d.hypot2(), 2.0 * f.dot(d), f.hypot2() - r * r);
            let disc = b * b - 4.0 * a * cc;
            if a < 1e-18 || disc < 0.0 {
                continue;
            }
            for u in [(-b - disc.sqrt()) / (2.0 * a), (-b + disc.sqrt()) / (2.0 * a)] {
                if (0.0..1.0).contains(&u) {
                    let x = p.lerp(q, u);
                    ts.push((x.y - c.y).atan2(x.x - c.x).rem_euclid(TAU));
                }
            }
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    let on = |t: f64| region.contains(c + Vec2::new(r * t.cos(), r * t.sin()));
    if ts.is_empty() {
        return if on(0.0) { vec![Arc { a: 0.0, b: TAU, full: true }] } else { vec![] };
    }
    let n = ts.len();
    let mut arcs: Vec<Arc> = vec![];
    for i in 0..n {
        let a = ts[i];
        let b = if i + 1 < n { ts[i + 1] } else { ts[0] + TAU };
        if b - a > 1e-9 && on((a + b) / 2.0) {
            // A circle that only touches the outline splits there: join the pieces back up.
            match arcs.last_mut() {
                Some(last) if (last.b - a).abs() < 1e-9 => last.b = b,
                _ => arcs.push(Arc { a, b, full: false }),
            }
        }
    }
    if arcs.len() > 1
        && let (Some(f), Some(l)) = (arcs.first().copied(), arcs.last().copied())
        && (l.b - (f.a + TAU)).abs() < 1e-9
    {
        arcs.pop();
        if let Some(f) = arcs.first_mut() {
            f.a = l.a - TAU;
        }
    }
    for arc in &mut arcs {
        if arc.b - arc.a >= TAU - 1e-9 {
            *arc = Arc { a: 0.0, b: TAU, full: true };
        }
    }
    arcs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, size: f64) -> Polyline {
        Polyline::new(vec![Point::new(x, y), Point::new(x + size, y), Point::new(x + size, y + size), Point::new(x, y + size)], true)
    }

    fn plain() -> FillSettings {
        FillSettings { row_spacing: 1.0, underlay: false, pull_compensation: 0.0, ..Default::default() }
    }

    fn top(st: &[Stitch]) -> Vec<Point> {
        st.iter().filter(|p| p.kind == StitchKind::Top).map(Stitch::point).collect()
    }

    #[test]
    fn rows_are_circles_round_the_centre_and_stay_inside() {
        let st = circular(&[square(0.0, 0.0, 20.0)], &plain(), &CircularSettings::default());
        let pts = top(&st);
        assert!(pts.len() > 120);
        assert!(pts.iter().all(|p| p.x > -1e-6 && p.x < 20.0 + 1e-6 && p.y > -1e-6 && p.y < 20.0 + 1e-6), "{pts:?}");
        // Starts at the middle and works out: the first stitches are near the centre.
        let c = Point::new(10.0, 10.0);
        assert!(pts.iter().take(5).all(|p| p.distance(c) < 1.6));
        // The corners are reached.
        assert!(pts.iter().any(|p| p.distance(Point::new(0.0, 0.0)) < 1.5));
        // No stitch is much longer than a fill stitch.
        let s = plain();
        for w in st.windows(2).filter(|w| w[1].kind == StitchKind::Top) {
            let d = w[0].point().distance(w[1].point());
            assert!(d <= s.stitch_length * (1.0 + MIN_END_FRACTION) + s.row_spacing + 1e-6, "{d}");
        }
    }

    #[test]
    fn whole_circles_spiral_out_without_steps() {
        let st = circular(&[square(0.0, 0.0, 20.0)], &plain(), &CircularSettings::default());
        let c = Point::new(10.0, 10.0);
        // Inside the inscribed circle the radius creeps outwards and never jumps a row.
        let r: Vec<f64> = top(&st).iter().map(|p| p.distance(c)).take_while(|r| *r < 9.0).collect();
        assert!(r.len() > 60);
        let bad: Vec<_> = r.windows(2).enumerate().filter(|(_, w)| !(w[1] >= w[0] - 1e-6 && w[1] - w[0] < 0.5)).collect();
        assert!(bad.is_empty(), "{bad:?}");
    }

    #[test]
    fn the_centre_moves_and_holes_stay_empty() {
        let s = plain();
        let moved = circular(&[square(0.0, 0.0, 20.0)], &s, &CircularSettings { center_x: -10.0, center_y: -10.0 });
        // Centred on the corner: rows are quarter circles round (0, 0).
        let pts = top(&moved);
        assert!(pts.iter().take(3).all(|p| p.to_vec2().hypot() < 2.0), "{pts:?}");
        let holed = circular(&[square(0.0, 0.0, 30.0), square(10.0, 10.0, 10.0)], &s, &CircularSettings::default());
        let inside = |p: Point, m: f64| p.x > 10.0 + m && p.x < 20.0 - m && p.y > 10.0 + m && p.y < 20.0 - m;
        assert!(top(&holed).iter().all(|p| !inside(*p, 1e-6)));
        for w in holed.windows(2).filter(|w| w[1].kind != StitchKind::Jump) {
            assert!(!inside(w[0].point().midpoint(w[1].point()), 0.5));
        }
        assert!(circular(&[], &s, &CircularSettings::default()).is_empty());
        let nan = Polyline::new(vec![Point::new(f64::NAN, 0.0), Point::new(1.0, 1.0)], true);
        assert!(circular(&[nan], &s, &CircularSettings::default()).iter().all(|p| p.x.is_finite()));
    }
}
