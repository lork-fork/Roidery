//! Satin stitches.
//!
//! One generator serves both satin objects. Satin Blocks come straight from the user's clicks:
//! point `i` of the left rail and point `i` of the right rail form rung `i`, as in Ember's tool
//! and stitchjs's `ClassicSatin` (alternating point and counterpoint). A satin along a path
//! builds its rails by offsetting the path half the width each way.
//!
//! Between two rungs the column is a quad. Stitches zig-zag across it, from one edge to the
//! other, spaced `density` mm apart on the same edge, measured at `fractional_spacing` across
//! the column (0: the longer edge, 1: the shorter). The stitch angle therefore turns smoothly
//! from one rung's angle to the next, which is what lets a user steer satins on curves and in
//! lettering.
//!
//! Roidery: Satin Blocks rails can curve between rungs ([`blocks_curved`]). Both rails of a
//! curved block are walked together, paired by the fraction of each rail's length, into rungs
//! about [`SUB_STEP`] mm apart, so the stitches fan round the bend instead of cutting its chord.

use kurbo::{Point, Vec2};

use crate::geometry::{Polyline, miter_normals, path_length, point_at, resample};
use crate::run;
use crate::settings::{SatinSettings, Underlay};
use crate::{Stitch, StitchKind};

/// A column narrower than this at a rung keeps its underlay on the centre line.
const MIN_UNDERLAY_WIDTH: f64 = 0.6;
/// Rungs per mm are capped so a pathological density can't explode the stitch count.
const MAX_STEPS_PER_SEGMENT: usize = 20_000;
/// Spacing of the rungs a curved block is walked into, mm.
pub const SUB_STEP: f64 = 0.4;
/// Rungs one curved block can be walked into.
const MAX_SUB_RUNGS: usize = 5_000;

/// Satin Blocks from two rails. Rails of different lengths use the shorter's rung count.
pub fn blocks(left: &[Point], right: &[Point], s: &SatinSettings) -> Vec<Stitch> {
    let rungs: Vec<(Point, Point)> = left.iter().zip(right).map(|(l, r)| (*l, *r)).filter(|(l, r)| finite(*l) && finite(*r)).collect();
    let keys: Vec<usize> = (0..rungs.len()).collect();
    column(&rungs, &keys, s)
}

/// Satin Blocks whose rails curve: `left[k]` and `right[k]` are the rails from rung `k` to rung
/// `k + 1`, flattened (first point on rung `k`, last on rung `k + 1`). Rails with different
/// block counts use the smaller.
pub fn blocks_curved(left: &[Vec<Point>], right: &[Vec<Point>], s: &SatinSettings) -> Vec<Stitch> {
    let (rungs, keys) = walk(left, right);
    column(&rungs, &keys, s)
}

/// The rungs of curved blocks, and the indices of the user's rungs among them.
fn walk(left: &[Vec<Point>], right: &[Vec<Point>]) -> (Vec<(Point, Point)>, Vec<usize>) {
    let clean = |v: &[Point]| v.iter().copied().filter(|p| finite(*p)).collect::<Vec<_>>();
    let (mut rungs, mut keys) = (vec![], vec![]);
    for (l, r) in left.iter().zip(right) {
        let (l, r) = (clean(l), clean(r));
        let (Some(&l0), Some(&r0)) = (l.first(), r.first()) else { continue };
        let (ll, rl) = (path_length(&l), path_length(&r));
        // A straight block needs no rungs between its own two.
        let m = if l.len() <= 2 && r.len() <= 2 { 1 } else { ((ll.max(rl) / SUB_STEP).ceil() as usize).clamp(1, MAX_SUB_RUNGS) };
        // A block continues from the rung the last one ended on.
        let continues = rungs.last().is_some_and(|&(pl, pr): &(Point, Point)| pl.distance(l0) < 1e-9 && pr.distance(r0) < 1e-9);
        if !continues {
            keys.push(rungs.len());
            rungs.push((l0, r0));
        }
        for j in 1..=m {
            let f = j as f64 / m as f64;
            rungs.push((point_at(&l, ll * f), point_at(&r, rl * f)));
        }
        keys.push(rungs.len() - 1);
    }
    (rungs, keys)
}

/// A satin column across `rungs`; `keys` are the user's rungs among them (in order, the first
/// and last included), between which the density is measured.
fn column(rungs: &[(Point, Point)], keys: &[usize], s: &SatinSettings) -> Vec<Stitch> {
    if rungs.len() < 2 || keys.len() < 2 {
        return vec![];
    }
    let rev_keys: Vec<usize> = keys.iter().rev().map(|k| rungs.len() - 1 - k).collect();
    let keys_for = |reverse: bool| if reverse { rev_keys.as_slice() } else { keys };
    let top = compensate(rungs, s.pull_compensation);
    let mut out = vec![];
    // Which end of the column the needle is at: false = the first rung.
    let mut at_end = false;
    for u in &s.underlays {
        match u {
            Underlay::CenterWalk { length } => {
                let c = oriented(rungs.iter().map(|(l, r)| l.midpoint(*r)).collect(), at_end);
                out.extend(run::along(&c, *length, StitchKind::Underlay));
                at_end = !at_end;
            }
            Underlay::Contour { length, inset } => {
                let inner = inset_rungs(rungs, *inset);
                let l = oriented(inner.iter().map(|(l, _)| *l).collect(), at_end);
                let r = oriented(inner.iter().map(|(_, r)| *r).collect(), !at_end);
                out.extend(run::along(&l, *length, StitchKind::Underlay));
                out.extend(run::along(&r, *length, StitchKind::Underlay));
            }
            Underlay::ZigZag { spacing, inset } => {
                let inner = inset_rungs(rungs, *inset);
                out.extend(zigzag(&oriented(inner, at_end), keys_for(at_end), *spacing, s.fractional_spacing, StitchKind::Underlay));
                at_end = !at_end;
            }
        }
    }
    let pairs = zigzag_pairs(&oriented(top, at_end), keys_for(at_end), s.density, s.fractional_spacing);
    let shortened = short_stitches(&pairs, s.short_stitch_inset, s.short_stitch_distance);
    out.extend(split(&shortened, s.split_length).into_iter().map(|p| Stitch::new(p, StitchKind::Top)));
    out
}

/// Short stitches: where penetrations on one edge crowd closer than `distance` (the inside of a
/// curve), every other one stops `inset` % of the way across toward the other edge, so the
/// thread doesn't pile up there. The first and last crossings stay whole, so the ends are square.
fn short_stitches(pairs: &[(Point, Point)], inset: f64, distance: f64) -> Vec<Point> {
    let mut out: Vec<Point> = pairs.iter().map(|(p, _)| *p).collect();
    if !(inset.is_finite() && inset > 0.0 && distance.is_finite() && distance > 0.0) {
        return out;
    }
    let f = (inset / 100.0).clamp(0.0, 0.5);
    // Penetrations alternate edges, so the same edge's last one is two back. Per edge: whether
    // the last crowded penetration was shortened. The whole first rung is followed half a
    // step later by the first zig, and the last zag by the whole last rung: those gaps are
    // half the spacing on every column, so they are left out.
    let mut short = [false; 2];
    for i in 3..pairs.len().saturating_sub(2) {
        let (Some(&(p, other)), Some(&(prev, _)), Some(flag)) = (pairs.get(i), pairs.get(i - 2), short.get_mut(i % 2)) else { continue };
        if p.distance(prev) < distance {
            *flag = !*flag;
            if *flag && let Some(o) = out.get_mut(i) {
                *o = p.lerp(other, f);
            }
        } else {
            *flag = false;
        }
    }
    out
}

/// Split satin: a crossing longer than `max` mm gets penetrations along it so no stitch is longer
/// than `max`. Where they fall shifts from one crossing to the next (a third of a piece either
/// way), so the splits make a brick pattern rather than a groove down the column. 0 never splits.
fn split(points: &[Point], max: f64) -> Vec<Point> {
    if !(max.is_finite() && max > 0.0) {
        return points.to_vec();
    }
    let max = max.max(1.0);
    let mut out = Vec::with_capacity(points.len());
    let mut crossing = 0usize;
    for (i, &b) in points.iter().enumerate() {
        if let Some(&a) = i.checked_sub(1).and_then(|j| points.get(j)) {
            let len = a.distance(b);
            if len > max {
                let mut n = (len / max).ceil();
                // How far the splits can shift and keep every piece within `max`.
                let mut room = (max * n / len - 1.0).min(1.0 / 3.0);
                if room < 0.15 {
                    n += 1.0;
                    room = (max * n / len - 1.0).min(1.0 / 3.0);
                }
                let shift = [-room, 0.0, room].get(crossing % 3).copied().unwrap_or(0.0);
                let n = (n as usize).min(64);
                for k in 1..n {
                    out.push(a.lerp(b, (k as f64 + shift) / n as f64));
                }
            }
            crossing += 1;
        }
        out.push(b);
    }
    out
}

/// A satin of fixed width along a path.
pub fn along_path(path: &Polyline, s: &SatinSettings) -> Vec<Stitch> {
    along_path_profiled(path, s, &[])
}

/// A satin along a path whose width varies as a width profile says: (t, left, right) points in
/// order, t the fraction of the path's length, left and right factors of half the width on each
/// side (left of the direction of travel, y down). An empty profile is the full width all along.
/// This is the Satin Column a path's stroke profile (the Width tool's) shapes.
pub fn along_path_profiled(path: &Polyline, s: &SatinSettings, profile: &[(f64, f64, f64)]) -> Vec<Stitch> {
    let pts = path.vertices();
    if pts.len() < 2 {
        return vec![];
    }
    let widest = profile.iter().map(|&(_, l, r)| l.max(r)).filter(|w| w.is_finite()).fold(1.0_f64, f64::max).min(MAX_PROFILE);
    // Rungs close enough together that the rails follow curves and the profile.
    let step = (s.width * widest / 2.0).clamp(0.3, 1.5).min(if profile.is_empty() { 1.5 } else { 0.8 });
    let centre = resample(&pts, step, 20.0);
    let normals = if path.closed && centre.len() > 2 {
        // Wrap so the seam gets a proper miter too.
        let mut ext = vec![centre[centre.len() - 2]];
        ext.extend(&centre);
        ext.push(centre[1]);
        let n = miter_normals(&ext);
        n[1..n.len() - 1].to_vec()
    } else {
        miter_normals(&centre)
    };
    // Each rung's fraction of the way along.
    let mut along = Vec::with_capacity(centre.len());
    let mut d = 0.0;
    for (i, p) in centre.iter().enumerate() {
        if let Some(prev) = i.checked_sub(1).and_then(|j| centre.get(j)) {
            d += prev.distance(*p);
        }
        along.push(d);
    }
    let total = d.max(1e-12);
    let half = s.width / 2.0;
    let (left, right): (Vec<Point>, Vec<Point>) = centre
        .iter()
        .zip(&normals)
        .zip(&along)
        .map(|((p, n), a)| {
            let (l, r) = width_at(profile, a / total);
            (*p + *n * (half * l), *p - *n * (half * r))
        })
        .unzip();
    blocks(&left, &right, s)
}

/// A profile's width factor can't exceed this (a runaway Width-tool drag can't make a column
/// metres wide).
const MAX_PROFILE: f64 = 20.0;

/// (left, right) width factors at `t`, linear between the profile's points; (1, 1) without any.
fn width_at(profile: &[(f64, f64, f64)], t: f64) -> (f64, f64) {
    let clean = |(l, r): (f64, f64)| {
        let c = |v: f64| if v.is_finite() { v.clamp(0.0, MAX_PROFILE) } else { 1.0 };
        (c(l), c(r))
    };
    let (Some(first), Some(last)) = (profile.first(), profile.last()) else { return (1.0, 1.0) };
    if t <= first.0 {
        return clean((first.1, first.2));
    }
    for w in profile.windows(2) {
        let [a, b] = w else { continue };
        if t <= b.0 {
            let u = ((t - a.0) / (b.0 - a.0).max(1e-9)).clamp(0.0, 1.0);
            return clean((a.1 + (b.1 - a.1) * u, a.2 + (b.2 - a.2) * u));
        }
    }
    clean((last.1, last.2))
}

fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn oriented<T>(mut v: Vec<T>, reverse: bool) -> Vec<T> {
    if reverse {
        v.reverse();
    }
    v
}

/// Unit vector from the left to the right end of a rung (zero for a collapsed rung).
fn across(l: Point, r: Point) -> Vec2 {
    let d = r - l;
    let len = d.hypot();
    if len < 1e-12 { Vec2::ZERO } else { d / len }
}

/// Each rung widened by `pc` mm at both ends.
fn compensate(rungs: &[(Point, Point)], pc: f64) -> Vec<(Point, Point)> {
    rungs
        .iter()
        .map(|(l, r)| {
            let a = across(*l, *r);
            // Negative compensation never crosses the rung over.
            let pc = pc.max(-(l.distance(*r) / 2.0 - 0.05).max(0.0));
            (*l - a * pc, *r + a * pc)
        })
        .collect()
}

/// Each rung shortened by `inset` at both ends, collapsing to its midpoint when too narrow.
fn inset_rungs(rungs: &[(Point, Point)], inset: f64) -> Vec<(Point, Point)> {
    rungs
        .iter()
        .map(|(l, r)| {
            let w = l.distance(*r);
            if w - 2.0 * inset < MIN_UNDERLAY_WIDTH {
                let m = l.midpoint(*r);
                let a = across(*l, *r) * (MIN_UNDERLAY_WIDTH.min(w) / 2.0);
                (m - a, m + a)
            } else {
                let a = across(*l, *r);
                (*l + a * inset, *r - a * inset)
            }
        })
        .collect()
}

/// Zig-zag across a run of rungs: left, right, left… with `spacing` mm between penetrations on
/// the same edge, measured `frac` of the way from the longer edge to the shorter. The spacing is
/// fitted to each stretch between two `keys` (the user's rungs), so it never piles up at a rung.
fn zigzag(rungs: &[(Point, Point)], keys: &[usize], spacing: f64, frac: f64, kind: StitchKind) -> Vec<Stitch> {
    zigzag_pairs(rungs, keys, spacing, frac).into_iter().map(|(p, _)| Stitch::new(p, kind)).collect()
}

/// `zigzag`'s penetrations, each with the point straight across from it on the other edge.
fn zigzag_pairs(rungs: &[(Point, Point)], keys: &[usize], spacing: f64, frac: f64) -> Vec<(Point, Point)> {
    let spacing = if spacing.is_finite() && spacing > 0.05 { spacing } else { 0.4 };
    let frac = if frac.is_finite() { frac.clamp(0.0, 1.0) } else { 0.5 };
    let (Some(&(first_l, first_r)), Some(&(last_l, last_r))) = (rungs.first(), rungs.last()) else { return vec![] };
    // The first and last rungs are sewn whole, so the column ends square on both corners.
    let mut out = vec![(first_l, first_r), (first_r, first_l)];
    // Penetrations alternate sides every half spacing, carried across rungs so the rhythm
    // doesn't restart at each one.
    let mut left_next = true;
    for w in keys.windows(2) {
        let [a, b] = *w else { continue };
        let Some(stretch) = rungs.get(a..=b) else { continue };
        // How far along the stretch each of its rungs is, measured as the density is.
        let mut at = Vec::with_capacity(stretch.len());
        let mut total = 0.0;
        at.push(0.0);
        for q in stretch.windows(2) {
            let ((l0, r0), (l1, r1)) = (q[0], q[1]);
            let (dl, dr) = (l0.distance(l1), r0.distance(r1));
            let (long, short) = if dl >= dr { (dl, dr) } else { (dr, dl) };
            total += long + (short - long) * frac;
            at.push(total);
        }
        if stretch.len() < 2 {
            continue;
        }
        let halves = ((2.0 * total / spacing).ceil() as usize).clamp(1, MAX_STEPS_PER_SEGMENT);
        let mut j = 0;
        for k in 1..=halves {
            let target = total * k as f64 / halves as f64;
            while j + 2 < at.len() && at[j + 1] < target {
                j += 1;
            }
            let (Some(&(l0, r0)), Some(&(l1, r1))) = (stretch.get(j), stretch.get(j + 1)) else { break };
            let span = at[j + 1] - at[j];
            let t = if span > 1e-12 { ((target - at[j]) / span).clamp(0.0, 1.0) } else { 1.0 };
            let (l, r) = (l0.lerp(l1, t), r0.lerp(r1, t));
            out.push(if left_next { (l, r) } else { (r, l) });
            left_next = !left_next;
        }
    }
    if rungs.len() > 1 {
        out.push(if left_next { (last_l, last_r) } else { (last_r, last_l) });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight(len: f64, width: f64) -> (Vec<Point>, Vec<Point>) {
        (vec![Point::new(0.0, 0.0), Point::new(len, 0.0)], vec![Point::new(0.0, width), Point::new(len, width)])
    }

    #[test]
    fn density_sets_same_edge_spacing() {
        let (l, r) = straight(10.0, 4.0);
        let s = SatinSettings { underlays: vec![], pull_compensation: 0.0, ..Default::default() };
        let st = blocks(&l, &r, &s);
        let lefts: Vec<_> = st.iter().filter(|s| s.y.abs() < 1e-9).collect();
        assert!(lefts.len() >= 25 && lefts.len() <= 27, "{}", lefts.len());
        // Alternates edges.
        for w in st.windows(2) {
            assert!((w[0].y - w[1].y).abs() > 3.9);
        }
    }

    #[test]
    fn pull_compensation_widens_the_column() {
        let (l, r) = straight(10.0, 4.0);
        let s = SatinSettings { underlays: vec![], pull_compensation: 0.3, ..Default::default() };
        let st = blocks(&l, &r, &s);
        let (min, max) = st.iter().fold((f64::MAX, f64::MIN), |(a, b), s| (a.min(s.y), b.max(s.y)));
        assert!((min + 0.3).abs() < 1e-9 && (max - 4.3).abs() < 1e-9);
    }

    #[test]
    fn underlay_comes_first_and_top_ends_away_from_it() {
        let (l, r) = straight(10.0, 4.0);
        let st = blocks(&l, &r, &SatinSettings::default());
        let first_top = st.iter().position(|s| s.kind == StitchKind::Top).unwrap();
        assert!(st[..first_top].iter().all(|s| s.kind == StitchKind::Underlay));
        // Centre walk goes to the far end, so the top satin comes back.
        assert!(st[first_top - 1].x > 9.0 && st.last().unwrap().x < 1.0);
    }

    #[test]
    fn rung_angles_steer_the_stitches() {
        // A column whose second rung is slanted: the last stitches follow the slant.
        let l = vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)];
        let r = vec![Point::new(0.0, 4.0), Point::new(14.0, 4.0)];
        let s = SatinSettings { underlays: vec![], pull_compensation: 0.0, ..Default::default() };
        let st = blocks(&l, &r, &s);
        let max_x = st.iter().map(|s| s.x).fold(f64::MIN, f64::max);
        assert!((max_x - 14.0).abs() < 1e-9);
    }

    #[test]
    fn along_path_keeps_width() {
        let p = Polyline::new(vec![Point::new(0.0, 0.0), Point::new(20.0, 0.0)], false);
        let s = SatinSettings { width: 5.0, pull_compensation: 0.0, underlays: vec![], ..Default::default() };
        let st = along_path(&p, &s);
        let (min, max) = st.iter().fold((f64::MAX, f64::MIN), |(a, b), s| (a.min(s.y), b.max(s.y)));
        assert!((max - min - 5.0).abs() < 1e-9);
    }

    /// Quarter-circle rails about the origin, radii `inner` and `outer`, flattened finely.
    fn arc(r: f64) -> Vec<Point> {
        (0..=64)
            .map(|i| {
                let a = std::f64::consts::FRAC_PI_2 * i as f64 / 64.0;
                Point::new(r * a.cos(), r * a.sin())
            })
            .collect()
    }

    #[test]
    fn curved_rails_sew_round_the_curve() {
        let s = SatinSettings { underlays: vec![], pull_compensation: 0.0, ..Default::default() };
        let st = blocks_curved(&[arc(10.0)], &[arc(14.0)], &s);
        // Every penetration lies on one of the arcs, not on the chord between the rungs.
        for p in &st {
            let r = p.point().distance(Point::ORIGIN);
            assert!((r - 10.0).abs() < 0.01 || (r - 14.0).abs() < 0.01, "{r}");
        }
        // Stitches cross the column roughly square to it all the way round.
        for w in st.windows(2) {
            let (a, b) = (w[0].point(), w[1].point());
            let radial = (a.to_vec2() + b.to_vec2()).normalize();
            let d = (b - a).normalize();
            assert!(d.dot(radial).abs() > 0.95, "{a:?} {b:?}");
        }
        // The density holds on the inner (shorter) edge's side at the default fractional spacing.
        let outer = st.iter().filter(|p| (p.point().distance(Point::ORIGIN) - 14.0).abs() < 0.01).count();
        let measured = std::f64::consts::FRAC_PI_2 * (14.0 + (10.0 - 14.0) * s.fractional_spacing);
        assert!(((outer as f64) - measured / s.density).abs() < 3.0, "{outer} vs {}", measured / s.density);
    }

    #[test]
    fn straight_blocks_sew_as_before() {
        let (l, r) = (
            vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(20.0, 5.0)],
            vec![Point::new(0.0, 4.0), Point::new(10.0, 4.0), Point::new(18.0, 8.0)],
        );
        let pieces = |v: &[Point]| v.windows(2).map(|w| w.to_vec()).collect::<Vec<_>>();
        let s = SatinSettings::default();
        assert_eq!(blocks_curved(&pieces(&l), &pieces(&r), &s), blocks(&l, &r, &s));
        // Junk never crashes.
        let nan = vec![vec![Point::new(f64::NAN, 0.0)], vec![]];
        assert!(blocks_curved(&nan, &nan, &s).iter().all(|p| p.x.is_finite()));
    }

    #[test]
    fn degenerate_input_is_empty_not_a_crash() {
        assert!(blocks(&[Point::new(0.0, 0.0)], &[Point::new(1.0, 0.0)], &SatinSettings::default()).is_empty());
        let nan = [Point::new(f64::NAN, 0.0), Point::new(1.0, 1.0)];
        assert!(blocks(&nan, &nan, &SatinSettings::default()).is_empty());
    }

    #[test]
    fn a_width_profile_shapes_the_column() {
        let s = SatinSettings { width: 4.0, pull_compensation: 0.0, underlays: vec![], ..Default::default() };
        let line = Polyline { points: vec![Point::new(0.0, 0.0), Point::new(40.0, 0.0)], closed: false };
        // Lens: nothing at the ends, full width in the middle.
        let lens = [(0.0, 0.0, 0.0), (0.5, 1.0, 1.0), (1.0, 0.0, 0.0)];
        let st = along_path_profiled(&line, &s, &lens);
        let spread = |x0: f64, x1: f64| {
            let ys: Vec<f64> = st.iter().filter(|p| p.x >= x0 && p.x <= x1).map(|p| p.y).collect();
            ys.iter().copied().fold(f64::MIN, f64::max) - ys.iter().copied().fold(f64::MAX, f64::min)
        };
        assert!((spread(19.0, 21.0) - 4.0).abs() < 0.2, "{}", spread(19.0, 21.0));
        assert!(spread(0.0, 2.0) < 0.6, "{}", spread(0.0, 2.0));
        // Only the left side (up, y down) widens.
        let st = along_path_profiled(&line, &s, &[(0.0, 2.0, 0.0), (1.0, 2.0, 0.0)]);
        let (lo, hi) = st.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
        assert!((lo + 4.0).abs() < 0.05 && hi.abs() < 0.05, "{lo} {hi}");
        // Absurd factors are clamped, never NaN.
        let st = along_path_profiled(&line, &s, &[(0.0, f64::NAN, 1e9), (1.0, -3.0, f64::INFINITY)]);
        assert!(st.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
        assert_eq!(along_path_profiled(&line, &s, &[]), along_path(&line, &s));
    }

    #[test]
    fn tight_curves_get_short_stitches_inside() {
        let s = SatinSettings { underlays: vec![], pull_compensation: 0.0, ..Default::default() };
        let st = blocks_curved(&[arc(2.0)], &[arc(6.0)], &s);
        let radii: Vec<f64> = st.iter().map(|p| p.point().distance(Point::ORIGIN)).collect();
        let inner = radii.iter().filter(|r| (*r - 2.0).abs() < 0.01).count();
        // Every other crowded inner penetration stops 15% of the way across: at 2.6 mm.
        let short = radii.iter().filter(|r| (*r - 2.6).abs() < 0.01).count();
        assert!(short > 5 && short.abs_diff(inner) <= 3, "{short} short, {inner} on the edge");
        // The outer edge isn't crowded, so it keeps its full stitches, and nothing else moves.
        assert!(radii.iter().all(|r| (r - 2.0).abs() < 0.01 || (r - 2.6).abs() < 0.01 || (r - 6.0).abs() < 0.01), "{radii:?}");
        // Off, every penetration is on an edge.
        let off = blocks_curved(&[arc(2.0)], &[arc(6.0)], &SatinSettings { short_stitch_inset: 0.0, ..s.clone() });
        assert!(off.iter().all(|p| {
            let r = p.point().distance(Point::ORIGIN);
            (r - 2.0).abs() < 0.01 || (r - 6.0).abs() < 0.01
        }));
        // A gentle curve has no short stitches.
        let gentle = blocks_curved(&[arc(10.0)], &[arc(14.0)], &s);
        assert!(gentle.iter().all(|p| {
            let r = p.point().distance(Point::ORIGIN);
            (r - 10.0).abs() < 0.01 || (r - 14.0).abs() < 0.01
        }));
    }

    #[test]
    fn wide_satins_split_into_staggered_stitches() {
        let (l, r) = straight(20.0, 12.0);
        let s = SatinSettings { underlays: vec![], pull_compensation: 0.0, ..Default::default() };
        let longest = |st: &[Stitch]| st.windows(2).map(|w| w[0].point().distance(w[1].point())).fold(0.0, f64::max);
        let st = blocks(&l, &r, &s);
        assert!(longest(&st) <= 7.0 + 1e-9, "{}", longest(&st));
        // The splits shift from one crossing to the next, so they don't line up into a groove.
        let mut inside: Vec<i64> = st.iter().filter(|p| p.y > 0.5 && p.y < 11.5).map(|p| (p.y * 100.0).round() as i64).collect();
        inside.sort_unstable();
        inside.dedup();
        assert!(inside.len() >= 3, "{inside:?}");
        // Off, the stitches cross the whole 12 mm.
        let off = blocks(&l, &r, &SatinSettings { split_length: 0.0, ..s.clone() });
        assert!(longest(&off) >= 12.0, "{}", longest(&off));
        // A column narrower than the split length is untouched.
        let (l, r) = straight(20.0, 4.0);
        assert!(blocks(&l, &r, &s).iter().all(|p| p.y.abs() < 1e-9 || (p.y - 4.0).abs() < 1e-9));
    }
}
