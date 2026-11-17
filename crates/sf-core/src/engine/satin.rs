//! Satin columns: zigzag stitches swinging between two rails, with underlay,
//! pull compensation, auto spacing, short stitches and auto split.

use super::out::Out;
use crate::geom::{Column, FLATTEN_TOLERANCE, Point, Rung, polyline};
use crate::model::{JaggedSides, SatinParams, TATAMI_LENGTH, effective_split_length};
use crate::pattern::Stitch;

/// Most rail pairs one sampling of a column makes (#79): a column about 25 m long at the
/// finest sampling (50 m at the usual one). A longer column is sampled more coarsely.
const MAX_SATIN_PAIRS: usize = 1_000_000;

/// Most pieces auto split cuts one stitch into (#84).
const MAX_SPLIT_PIECES: usize = 10_000;

/// Fine samples along the column: pairs of corresponding rail points spaced
/// `half_step(width)` apart along the longer rail, at most about [`MAX_SATIN_PAIRS`].
fn sample_pairs(col: &Column, half_step: impl Fn(f64) -> f64) -> Vec<(Point, Point)> {
    let sections = col.sections(FLATTEN_TOLERANCE);
    let mut out = Vec::new();
    let last = sections.len().saturating_sub(1);
    // The column's length along its longer rails: no step is shorter than its share of the cap.
    let total: f64 = sections.iter().map(|(a, b)| polyline::length(a).max(polyline::length(b))).sum();
    let floor = if total.is_finite() { total / MAX_SATIN_PAIRS as f64 } else { 0.0 };
    for (si, (a, b)) in sections.iter().enumerate() {
        let la = polyline::length(a);
        let lb = polyline::length(b);
        let w0 = a.first().zip(b.first()).map(|(p, q)| p.distance(*q)).unwrap_or(0.0);
        let w1 = a.last().zip(b.last()).map(|(p, q)| p.distance(*q)).unwrap_or(0.0);
        let hs = half_step((w0 + w1) * 0.5).max(0.05).max(floor);
        let n = ((la.max(lb) / hs).ceil() as usize).clamp(1, MAX_SATIN_PAIRS);
        let end = if si == last { n } else { n - 1 };
        let on_a = polyline::points_at(a, (0..=end).map(|m| m as f64 / n as f64 * la));
        let on_b = polyline::points_at(b, (0..=end).map(|m| m as f64 / n as f64 * lb));
        out.extend(on_a.into_iter().zip(on_b));
    }
    out
}

/// The spacing `profile` gives at `t` (0–1 along the column), straight between its points; the
/// first or last point's before or after them.
fn spacing_at(profile: &[[f64; 2]], t: f64, nominal: f64) -> f64 {
    // at most 64 points: the profile comes from the design file or a command's params
    let mut pts: Vec<[f64; 2]> = profile.iter().take(64).copied().filter(|[a, b]| a.is_finite() && b.is_finite()).collect();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let s = match (pts.first(), pts.last()) {
        (None, _) | (_, None) => nominal,
        (Some(f), _) if t <= f[0] => f[1],
        (_, Some(l)) if t >= l[0] => l[1],
        _ => pts.windows(2).find(|w| t >= w[0][0] && t <= w[1][0]).map_or(nominal, |w| {
            let span = (w[1][0] - w[0][0]).max(1e-9);
            w[0][1] + (w[1][1] - w[0][1]) * (t - w[0][0]) / span
        }),
    };
    s.clamp(0.1, 5.0)
}

/// Rail pairs for variable or random density (PU-04, PU-05): sampled finely, then kept every half
/// row along the column's middle, each row's spacing from `spacing_along` at that point (else
/// `spacing`), changed by up to `random_density` of itself.
fn varied_pairs(col: &Column, params: &SatinParams) -> Vec<(Point, Point)> {
    const FINE: f64 = 0.025;
    let dense = sample_pairs(col, |_| FINE);
    let mids: Vec<Point> = dense.iter().map(|(a, b)| a.lerp(*b, 0.5)).collect();
    let mut along = Vec::with_capacity(mids.len());
    let mut d = 0.0;
    for (i, m) in mids.iter().enumerate() {
        if let Some(prev) = i.checked_sub(1).and_then(|j| mids.get(j)) {
            d += prev.distance(*m);
        }
        along.push(d);
    }
    let total = d.max(1e-9);
    let random = params.random_density.clamp(0.0, 0.9);
    let mut rng = super::special::Rng::new((total * 1000.0) as u64 ^ 0x5a71);
    let mut out = Vec::new();
    let mut next = 0.0;
    let last = dense.len().saturating_sub(1);
    for (i, (pair, at)) in dense.iter().zip(&along).enumerate() {
        if *at + 1e-9 >= next || i == last {
            out.push(*pair);
            let w = pair.0.distance(pair.1);
            let base = if params.spacing_along.is_empty() {
                effective_spacing(params, w)
            } else {
                spacing_at(&params.spacing_along, at / total, params.spacing)
            };
            let jitter = if random > 0.0 { 1.0 + random * (rng.unit() * 2.0 - 1.0) } else { 1.0 };
            next = at + (base * jitter * 0.5).max(FINE);
        }
    }
    out
}

/// Effective satin spacing for a column width (auto spacing opens up narrow
/// columns so small lettering does not become bullet-proof).
pub(crate) fn effective_spacing(params: &SatinParams, width: f64) -> f64 {
    let s = params.spacing.max(0.1);
    if !params.auto_spacing {
        return s;
    }
    // 1 mm → +25 %, 3 mm and wider → nominal.
    let f = 1.0 + 0.25 * ((3.0 - width) / 2.0).clamp(0.0, 1.0);
    s * f
}

/// Splits a long stitch `p → q` into shorter staggered pieces.
fn split_stitch(p: Point, q: Point, max_len: f64, index: usize, out: &mut Vec<Point>) {
    let len = p.distance(q);
    let max_len = effective_split_length(max_len);
    if max_len <= 0.0 || len.is_nan() || len <= max_len {
        out.push(q);
        return;
    }
    // Leave 25 % slack so the staggered split points never exceed `max_len`; at most
    // MAX_SPLIT_PIECES pieces however long the stitch (#84).
    let k = ((len / (max_len * 0.75)).ceil() as usize).clamp(1, MAX_SPLIT_PIECES);
    // Stagger the split points so they don't line up into a visible groove.
    const PHASES: [f64; 5] = [0.0, 0.4, -0.3, 0.2, -0.4];
    let shift = PHASES[index % PHASES.len()] / k as f64;
    for m in 1..k {
        let f = (m as f64 / k as f64 + shift * 0.8).clamp(0.05, 0.95);
        out.push(p.lerp(q, f));
    }
    out.push(q);
}

/// Splits a stitch across a tatami border into pieces about `len` long, the needle
/// points shifting a quarter of a stitch from row to row: the tatami pattern. They are
/// measured from side A whichever way a row runs (rows alternate), so the pattern
/// lines up.
fn split_tatami(p: Point, q: Point, len: f64, row: usize, out: &mut Vec<Point>) {
    let d = p.distance(q);
    let len = effective_split_length(len);
    let min = 0.3 * len;
    if len <= 0.0 || d.is_nan() || d <= len + min {
        out.push(q);
        return;
    }
    // A stitch longer than MAX_SPLIT_PIECES pieces is cut into longer pieces (#84).
    let len = len.max(d / MAX_SPLIT_PIECES as f64);
    let min = 0.3 * len;
    let phase = (row % 4) as f64 * 0.25 * len;
    let mut cuts: Vec<f64> = (0usize..).map(|k| phase + k as f64 * len).take_while(|x| *x < d - min).filter(|x| *x > min).collect();
    // even rows run from side A, odd ones back from side B
    let from_a = row.is_multiple_of(2);
    if !from_a {
        cuts.reverse();
    }
    for x in cuts {
        out.push(p.lerp(q, if from_a { x / d } else { 1.0 - x / d }));
    }
    out.push(q);
}

/// The narrowest a satin stitch is made (mm). Narrower ones, at a column's pointed tips,
/// are shorter than many machines' (and designs') shortest stitch and get dropped, leaving
/// the tip ragged; they don't sit up on the fabric either.
pub(crate) const MIN_WIDTH: f64 = 0.8;

/// Satin zigzag over a list of rail pairs. Even samples land on rail A, odd ones
/// on rail B. `pull` is the pull compensation on (rail A, rail B).
pub(crate) fn zigzag_points(pairs: &[(Point, Point)], params: &SatinParams, pull: (f64, f64), short: bool) -> Vec<Point> {
    if pairs.is_empty() {
        return Vec::new();
    }
    // Pull compensation widens each pair, side by side; a pair still narrower than
    // [`MIN_WIDTH`] (a pointed tip) is widened to it about its middle.
    let comp: Vec<(Point, Point)> = pairs
        .iter()
        .map(|&(a, b)| {
            let d = (a - b).normalized();
            let (a, b) = (a + d * pull.0, b - d * pull.1);
            let short = (MIN_WIDTH - a.distance(b)) * 0.5;
            if short > 0.0 { (a + d * short, b - d * short) } else { (a, b) }
        })
        .collect();

    let nominal = params.spacing.max(0.1);
    let mut pts: Vec<Point> = Vec::with_capacity(comp.len());
    for (j, &(a, b)) in comp.iter().enumerate() {
        // side 1 first, or side 2 with the sides swapped
        let on_a = (j % 2 == 0) != params.swap_sides;
        let mut p = if on_a { a } else { b };
        if short {
            // Same-side neighbour penetrations: if they crowd together (inside of a
            // curve), pull every other one in towards the centre.
            let other = if on_a { b } else { a };
            let prev = j.checked_sub(2).map(|k| if on_a { comp[k].0 } else { comp[k].1 });
            let next = comp.get(j + 2).map(|&(pa, pb)| if on_a { pa } else { pb });
            let local = match (prev, next) {
                (Some(x), Some(y)) => x.distance(y) * 0.5,
                (Some(x), None) => x.distance(p),
                (None, Some(y)) => y.distance(p),
                (None, None) => nominal,
            };
            if local < nominal * 0.55 && (j / 2) % 2 == 1 {
                let width = p.distance(other);
                p = p.lerp(other, (0.35 * width).min(width * 0.45) / width.max(1e-9));
            }
        }
        // Jagged edges: the stitch ends short of the edge by a random amount, always leaving a
        // fifth of the column's width between the two ends of a stitch.
        if let Some(jag) = params.jagged.filter(|g| g.sides.covers(on_a)) {
            let other = if on_a { b } else { a };
            let width = p.distance(other);
            let depth = if jag.depth.is_finite() { jag.depth.clamp(0.0, 20.0) } else { 0.0 };
            let most = if jag.sides == JaggedSides::Both { 0.4 } else { 0.8 };
            if width > 1e-9 {
                p = p.lerp(other, (unit(j as u64) * depth).min(width * most) / width);
            }
        }
        pts.push(p);
    }
    // Auto split long stitches.
    let mut out = Vec::with_capacity(pts.len());
    out.push(pts[0]);
    for (i, w) in pts.windows(2).enumerate() {
        if params.tatami {
            let len = if params.split_length > 0.0 { params.split_length } else { TATAMI_LENGTH };
            split_tatami(w[0], w[1], len, i, &mut out);
        } else {
            split_stitch(w[0], w[1], params.split_length, i, &mut out);
        }
    }
    out
}

/// A number in `[0, 1)` that looks random, the same every time for the same `k`.
fn unit(k: u64) -> f64 {
    let mut h = k.wrapping_add(0x9e37_79b9_7f4a_7c15).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 31;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 29;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Pairs inset towards the column centre by `inset` (never past the centre).
fn inset_pairs(pairs: &[(Point, Point)], inset: f64) -> Vec<(Point, Point)> {
    pairs
        .iter()
        .map(|&(a, b)| {
            let w = a.distance(b);
            let d = inset.min(w * 0.5 - 0.05).max(0.0);
            let u = (b - a).normalized();
            (a + u * d, b - u * d)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pass {
    Center,
    Edge,
    Zigzag,
    /// The return of a double zigzag: same peaks, opposite rails, so it crosses
    /// the first pass.
    ZigzagBack,
}

fn underlay_passes(params: &SatinParams, width: f64) -> Vec<Pass> {
    let u = &params.underlay;
    if u.auto {
        if width < 1.0 {
            vec![]
        } else if width < 2.2 {
            vec![Pass::Center]
        } else if width < 4.5 {
            vec![Pass::Edge]
        } else {
            vec![Pass::Edge, Pass::Zigzag]
        }
    } else {
        let mut v = Vec::new();
        if u.center_run {
            v.push(Pass::Center);
        }
        if u.edge_run {
            v.push(Pass::Edge);
        }
        if u.zigzag {
            v.push(Pass::Zigzag);
            if u.double_zigzag {
                v.push(Pass::ZigzagBack);
            }
        }
        v
    }
}

/// Whether a column `width` mm wide sewn with `params` ends at its far end: each underlay
/// pass but the edge run (out and back) turns the way the next one goes.
pub(crate) fn ends_far(params: &SatinParams, width: f64) -> bool {
    underlay_passes(params, width).iter().filter(|p| **p != Pass::Edge).count().is_multiple_of(2)
}

fn reversed_pairs(pairs: &[(Point, Point)]) -> Vec<(Point, Point)> {
    pairs.iter().rev().copied().collect()
}

/// Generates a satin column. `max_stitch` is the machine's longest stitch (mm): with
/// `preserve_long`, longer stitches become frame moves (jumps) and one needle drop.
pub(crate) fn generate(col: &Column, params: &SatinParams, hint: Option<Point>, exit: Option<Point>, max_stitch: f64) -> Vec<Stitch> {
    if col.rungs.len() < 2 {
        return Vec::new();
    }
    let width = col.average_width();
    let passes = underlay_passes(params, width);
    // Each underlay pass traverses the column once, except edge run (out and back).
    let traversals: usize = passes.iter().map(|p| if *p == Pass::Edge { 2 } else { 1 }).sum();

    let mut col = col.clone();
    // Start at the end nearest the entry hint (closest join); otherwise try to
    // finish near the requested exit.
    let first = col.rungs[0].center();
    let last = col.rungs[col.rungs.len() - 1].center();
    let reverse = match (hint, exit) {
        (Some(h), _) => h.distance(last) < h.distance(first),
        (None, Some(e)) => {
            // satin ends at the far end iff the number of traversals is even
            let ends_far = traversals.is_multiple_of(2);
            let far_is_closer = e.distance(last) < e.distance(first);
            ends_far != far_is_closer
        }
        _ => false,
    };
    // A motif column: motifs laid out along the column as drawn, sewn backwards when it starts
    // at the far end.
    if let Some(m) = params.motif_column.as_deref() {
        let pairs = sample_pairs(&col, |_| 0.25);
        let shape = super::run::motif_points(m.motif, m.custom_motif.as_ref());
        let mut pts = super::run::motif_column(&pairs, &shape, m.length, m.stitch_length.max(0.3));
        if reverse {
            pts.reverse();
        }
        return super::out::as_stitches(pts);
    }
    if reverse {
        col.reverse();
    }

    let fine = if params.spacing_along.is_empty() && params.random_density <= 0.0 {
        pushed(sample_pairs(&col, |w| effective_spacing(params, w) * 0.5), params.push_comp)
    } else {
        pushed(varied_pairs(&col, params), params.push_comp)
    };
    if fine.len() < 2 {
        return Vec::new();
    }
    let mut out = Out::new();
    // 3D puff: a cap across each end, the first before the column, the last after it
    let cap = params.puff.map(|p| p.cap.clamp(0.0, 3.0));
    let spacing = params.spacing.max(0.1);
    if let Some(cap) = cap {
        out.stitches(end_cap(&fine, false, cap, spacing));
    }
    // the start cap ends by where the column starts: reached with running stitches
    stitch_pairs(&fine, params, width, max_stitch, if cap.is_some() { 2.0 } else { 0.0 }, None, &mut out);
    if let Some(cap) = cap {
        let pts = end_cap(&fine, true, cap, spacing);
        if let Some(&p0) = pts.first() {
            out.run_to(p0, 2.0);
        }
        out.stitches(pts);
    }
    out.finish()
}

/// The column's rungs less `push` mm at each end (push compensation), measured along its middle;
/// a column too short for it keeps its length.
fn pushed(fine: Vec<(Point, Point)>, push: f64) -> Vec<(Point, Point)> {
    if !(push.is_finite() && push > 0.0) || fine.len() < 3 {
        return fine;
    }
    let mids: Vec<Point> = fine.iter().map(|(a, b)| a.midpoint(*b)).collect();
    let mut along = Vec::with_capacity(mids.len());
    let mut d = 0.0;
    for (i, m) in mids.iter().enumerate() {
        if let Some(prev) = i.checked_sub(1).and_then(|k| mids.get(k)) {
            d += prev.distance(*m);
        }
        along.push(d);
    }
    let push = push.min(5.0);
    // a ring (a satin border round a closed shape) has no open ends
    let closed = mids.first().zip(mids.last()).is_some_and(|(a, b)| a.distance(*b) < 0.1);
    if closed || d < 2.0 * push + 0.5 {
        return fine;
    }
    fine.into_iter().zip(along).filter(|(_, t)| *t >= push && *t <= d - push).map(|(p, _)| p).collect()
}

/// A puff end cap across the column's first (or `last`) rung: satin whose stitches lie along
/// the column, from 1 mm inside it to `past` mm beyond its end, so they cut the foam's edge and
/// cover it. Empty for a column too short to tell which way is out.
fn end_cap(fine: &[(Point, Point)], last: bool, past: f64, spacing: f64) -> Vec<Point> {
    let along: Box<dyn Iterator<Item = &(Point, Point)>> = if last { Box::new(fine.iter().rev()) } else { Box::new(fine.iter()) };
    let mut along = along;
    let Some(&(a, b)) = along.next() else { return Vec::new() };
    // the first cap ends at the first rail, where the column starts
    let (a, b) = if last { (a, b) } else { (b, a) };
    let end = a.midpoint(b);
    // the way out of the column: from a rung inside towards the end, square to the end rung
    let Some(inside) = along.map(|(p, q)| p.midpoint(*q)).find(|m| m.distance(end) > 0.3) else { return Vec::new() };
    let mut out_dir = (b - a).perp().normalized();
    if out_dir.dot(inside - end) > 0.0 {
        out_dir = -out_dir;
    }
    let width = a.distance(b);
    if !(width.is_finite() && out_dir.is_finite()) || width < 1e-6 {
        return Vec::new();
    }
    // a zigzag across the end, `spacing` per cycle
    let n = ((width / spacing).ceil() as usize).clamp(1, 2000) * 2;
    (0..=n)
        .map(|k| {
            let p = a.lerp(b, k as f64 / n as f64);
            if k % 2 == 0 { p - out_dir * 1.0 } else { p + out_dir * past }
        })
        .collect()
}

/// Underlay and top satin over rail pairs already in sewing order, `width` mm wide
/// on average (it picks the automatic underlay). Shared by satin columns and the
/// satin of closed shapes that turns along inclination lines. When `out` already
/// has a needle position, the first stitch is reached by [`Out::connect`] (`trim`
/// is the trim threshold).
/// With an `exit`, the column finishes as near it as it can: when it lies nearer the column's
/// start than the end the stitching would finish at, a run along the middle goes first (the
/// thread carried to the far end under the column) so the top stitching comes back; and the
/// last stitch lands on the side nearer it.
pub(crate) fn stitch_pairs(
    fine: &[(Point, Point)],
    params: &SatinParams,
    width: f64,
    max_stitch: f64,
    trim: f64,
    exit: Option<Point>,
    out: &mut Out,
) {
    if fine.len() < 2 {
        return;
    }
    let mut passes = underlay_passes(params, width);
    if let (Some(e), Some(first), Some(last)) = (exit, fine.first(), fine.last()) {
        let (start, far) = (first.0.midpoint(first.1), last.0.midpoint(last.1));
        let (end, other) = if ends_far(params, width) { (far, start) } else { (start, far) };
        if e.distance(other) + 0.5 < e.distance(end) {
            passes.insert(0, Pass::Center);
        }
    }
    let u = &params.underlay;
    let run_step = u.stitch_length.max(0.5);
    if out.pos().is_some()
        && let Some(&(a, b)) = fine.first()
    {
        let start = match passes.first() {
            Some(Pass::Center) => a.midpoint(b),
            Some(_) => inset_pairs(&[(a, b)], u.inset).first().map_or(a, |p| p.0),
            None => a,
        };
        out.connect(start, trim, 2.5);
    }
    let mut forward = true;
    for pass in &passes {
        let pairs: Vec<(Point, Point)> = if forward { fine.to_vec() } else { reversed_pairs(fine) };
        match pass {
            Pass::Center => {
                let center: Vec<Point> = pairs.iter().map(|(a, b)| a.midpoint(*b)).collect();
                out.run_along(&center, run_step);
                forward = !forward;
            }
            Pass::Edge => {
                let ins = inset_pairs(&pairs, u.inset);
                let rail_a: Vec<Point> = ins.iter().map(|p| p.0).collect();
                let mut rail_b: Vec<Point> = ins.iter().map(|p| p.1).collect();
                rail_b.reverse();
                out.run_along(&rail_a, run_step);
                out.run_along(&rail_b, run_step);
            }
            Pass::Zigzag | Pass::ZigzagBack => {
                let mut ins = inset_pairs(&pairs, u.inset);
                if *pass == Pass::ZigzagBack {
                    ins.iter_mut().for_each(|(a, b)| std::mem::swap(a, b));
                }
                let zz_params = SatinParams { spacing: u.zigzag_spacing.max(0.5), jagged: None, ..params.clone() };
                let coarse = resample_pairs(&ins, zz_params.spacing * 0.5);
                let pts = zigzag_points(&coarse, &zz_params, (0.0, 0.0), false);
                if let Some(&p0) = pts.first() {
                    out.run_to(p0, run_step);
                }
                out.stitches(pts);
                forward = !forward;
            }
        }
    }
    let pairs = if forward { fine.to_vec() } else { reversed_pairs(fine) };
    // the last stitch on the side nearer the exit
    let swapped;
    let params = match (exit, pairs.last()) {
        (Some(e), Some(&(a, b))) => {
            let on_a = (pairs.len() - 1).is_multiple_of(2) != params.swap_sides;
            let (land, other) = if on_a { (a, b) } else { (b, a) };
            if e.distance(other) < e.distance(land) {
                swapped = SatinParams { swap_sides: !params.swap_sides, ..params.clone() };
                &swapped
            } else {
                params
            }
        }
        _ => params,
    };
    let pts = zigzag_points(&pairs, params, params.pull_sides(), params.short_stitches);
    if let Some(&p0) = pts.first()
        && out.pos().is_some()
    {
        out.run_to(p0, run_step);
    }
    if params.preserve_long {
        for p in pts {
            out.long_stitch(p, max_stitch);
        }
    } else {
        out.stitches(pts);
    }
}

/// Re-samples a pair list so consecutive pairs are about `step` apart (measured on
/// the longer side).
fn resample_pairs(pairs: &[(Point, Point)], step: f64) -> Vec<(Point, Point)> {
    if pairs.len() < 2 {
        return pairs.to_vec();
    }
    let a: Vec<Point> = pairs.iter().map(|p| p.0).collect();
    let b: Vec<Point> = pairs.iter().map(|p| p.1).collect();
    let la = polyline::length(&a);
    let lb = polyline::length(&b);
    let n = ((la.max(lb) / step.max(0.05)).ceil() as usize).clamp(1, MAX_SATIN_PAIRS);
    let on_a = polyline::points_at(&a, (0..=n).map(|m| m as f64 / n as f64 * la));
    let on_b = polyline::points_at(&b, (0..=n).map(|m| m as f64 / n as f64 * lb));
    on_a.into_iter().zip(on_b).collect()
}

/// Satin border of `width` centred on a path (used for appliqué cover stitches
/// and satin outlines). For closed paths, `pts` must be explicitly closed.
pub(crate) fn border(pts: &[Point], closed: bool, width: f64, spacing: f64, pull_comp: f64) -> Vec<Point> {
    let pts = polyline::dedup(pts, 1e-6);
    if pts.len() < 2 {
        return pts;
    }
    let path: Vec<Point> = if closed {
        let mut p = pts.clone();
        if p.len() > 2 && p[0].approx_eq(p[p.len() - 1], 1e-9) {
            p.pop();
        }
        p
    } else {
        pts.clone()
    };
    let h = width * 0.5;
    let a = polyline::offset_polyline(&path, h, closed, 2.0);
    let b = polyline::offset_polyline(&path, -h, closed, 2.0);
    let mut rungs: Vec<Rung> = a.iter().zip(&b).map(|(&a, &b)| Rung::new(a, b, Default::default())).collect();
    if closed && !rungs.is_empty() {
        rungs.push(rungs[0]);
    }
    let col = Column::new(rungs);
    let params = SatinParams { spacing, pull_comp, auto_spacing: false, split_length: 0.0, short_stitches: true, ..Default::default() };
    let pairs = sample_pairs(&col, |_| spacing * 0.5);
    zigzag_points(&pairs, &params, (pull_comp, pull_comp), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Node, NodeKind};
    use crate::model::{MIN_SPLIT_LENGTH, SatinUnderlay};
    use crate::pattern::Cmd;

    /// #79: a satin column 10 km long used to be sampled every 0.1 mm or finer (hundreds of
    /// millions of rail pairs). Sampling now stops at `MAX_SATIN_PAIRS`.
    #[test]
    fn a_very_long_satin_column_stays_bounded() {
        let col = Column::new(vec![
            Rung::new(Point::new(0.0, -1.0), Point::new(0.0, 1.0), NodeKind::Corner),
            Rung::new(Point::new(1e7, -1.0), Point::new(1e7, 1.0), NodeKind::Corner),
        ]);
        assert!(sample_pairs(&col, |_| 0.025).len() <= MAX_SATIN_PAIRS + 2);
        let started = std::time::Instant::now();
        let out = generate(&col, &SatinParams::default(), None, None, 12.0);
        assert!(!out.is_empty() && out.len() <= 8 * MAX_SATIN_PAIRS, "{}", out.len());
        assert!(started.elapsed().as_secs() < 30, "took {:?}", started.elapsed());
        // A short column keeps its usual sampling.
        let short = Column::new(vec![
            Rung::new(Point::new(0.0, -1.0), Point::new(0.0, 1.0), NodeKind::Corner),
            Rung::new(Point::new(10.0, -1.0), Point::new(10.0, 1.0), NodeKind::Corner),
        ]);
        assert_eq!(sample_pairs(&short, |_| 0.2).len(), 51);
    }

    /// Sew-out on a T-shirt: serif tips narrower than the shortest stitch were dropped and
    /// came out ragged. No satin stitch is narrower than `MIN_WIDTH`.
    #[test]
    fn pointed_tips_are_sewn_no_narrower_than_the_minimum() {
        let pairs: Vec<(Point, Point)> = (0..20)
            .map(|i| {
                let (x, w) = (i as f64 * 0.2, 2.0 * (1.0 - i as f64 / 20.0));
                (Point::new(x, -w / 2.0), Point::new(x, w / 2.0))
            })
            .collect();
        let params = SatinParams { pull_comp: 0.0, auto_spacing: false, ..Default::default() };
        let pts = zigzag_points(&pairs, &params, (0.0, 0.0), false);
        assert!(pts.windows(2).all(|w| w[0].distance(w[1]) >= MIN_WIDTH - 1e-9), "{pts:?}");
        // wide parts keep their width
        assert!((pts[0].distance(pts[1]) - 2.0).abs() < 0.1);
    }

    #[test]
    fn a_motif_column_sews_motifs_as_wide_as_the_column() {
        use crate::model::{CustomMotif, Motif, MotifColumn};
        // A column 2 mm wide at the left growing to 8 mm at the right, 40 mm long.
        let col = Column::new(vec![
            Rung::new(Point::new(0.0, -1.0), Point::new(0.0, 1.0), NodeKind::Corner),
            Rung::new(Point::new(40.0, -4.0), Point::new(40.0, 4.0), NodeKind::Corner),
        ]);
        let sew = |m: MotifColumn, hint: Option<Point>| -> Vec<Point> {
            let params = SatinParams { motif_column: Some(Box::new(m)), ..Default::default() };
            generate(&col, &params, hint, None, 12.0).iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect()
        };
        let pts = sew(MotifColumn { motif: Motif::Diamond, length: 1.0, ..Default::default() }, None);
        assert!(pts.len() > 40, "{}", pts.len());
        // Inside the column, and graded: wider motifs where it is wider.
        assert!(pts.iter().all(|p| p.y.abs() <= 1.0 + 3.0 * p.x / 40.0 + 1e-6), "a needle point outside the column");
        let reach = |lo: f64, hi: f64| pts.iter().filter(|p| p.x >= lo && p.x <= hi).map(|p| p.y.abs()).fold(0.0, f64::max);
        assert!(reach(0.0, 6.0) < 1.6 && reach(32.0, 40.0) > 2.8, "{} {}", reach(0.0, 6.0), reach(32.0, 40.0));
        // From the far end, the same stitches backwards.
        let back = sew(MotifColumn { motif: Motif::Diamond, length: 1.0, ..Default::default() }, Some(Point::new(45.0, 0.0)));
        assert_eq!(back, pts.iter().rev().copied().collect::<Vec<_>>());
        // A peak drawn upward stands up along a column running left to right, as on a run.
        let peak = CustomMotif { name: "Peak".into(), points: vec![[0.0, 0.0], [0.5, -1.0], [1.0, 0.0]], aspect: 1.0 };
        let pts = sew(MotifColumn { custom_motif: Some(peak), length: 1.0, ..Default::default() }, None);
        let below = pts.iter().filter(|p| p.y > 0.0).count();
        assert!(below * 2 > pts.len(), "the bases below the middle, the tips above");
        assert!(pts.iter().any(|p| p.x > 30.0 && p.y < -2.8));
    }

    fn straight(width: f64, len: f64) -> Column {
        Column::new(vec![
            Rung::new(Point::new(0.0, 0.0), Point::new(0.0, width), NodeKind::Corner),
            Rung::new(Point::new(len, 0.0), Point::new(len, width), NodeKind::Corner),
        ])
    }

    #[test]
    fn variable_and_random_density_along_a_column() {
        // a straight column 40 mm long, 4 mm wide
        let col = straight(4.0, 40.0);
        let count = |params: &SatinParams, from: f64, to: f64| varied_pairs(&col, params).iter().filter(|(a, _)| a.x >= from && a.x < to).count();
        let even = SatinParams { auto_spacing: false, ..Default::default() };
        // dense (0.2) at the start, light (0.8) at the end
        let graded = SatinParams { spacing_along: vec![[0.0, 0.2], [1.0, 0.8]], ..even.clone() };
        assert!(count(&graded, 0.0, 10.0) > 2 * count(&graded, 30.0, 40.0), "denser at the start");
        assert!((spacing_at(&graded.spacing_along, 0.5, 0.4) - 0.5).abs() < 1e-9);
        // random: the same row count give or take, uneven gaps, the same every time
        let rough = SatinParams { random_density: 0.5, ..even.clone() };
        let a = varied_pairs(&col, &rough);
        assert_eq!(a, varied_pairs(&col, &rough), "the same every time");
        let gaps: Vec<f64> = a.windows(2).map(|w| w[1].0.distance(w[0].0)).collect();
        let (lo, hi) = gaps.iter().fold((f64::MAX, 0.0_f64), |(l, h), g| (l.min(*g), h.max(*g)));
        assert!(hi > lo * 1.5, "uneven: {lo} {hi}");
        let n_even = count(&even, 0.0, 40.0) as f64;
        assert!((count(&rough, 0.0, 40.0) as f64 - n_even).abs() < n_even * 0.25);
    }

    #[test]
    fn jagged_edges_end_stitches_short_of_the_edge_at_random() {
        use crate::model::{Jagged, JaggedSides};
        let bare = SatinUnderlay { auto: false, center_run: false, ..Default::default() };
        let base =
            SatinParams { pull_comp: 0.0, auto_spacing: false, split_length: 0.0, short_stitches: false, underlay: bare, ..Default::default() };
        // The rails are y = 0 (side 1) and y = 6 (side 2): how far each needle point lies in from them.
        let depths = |params: &SatinParams| {
            let st = generate(&straight(6.0, 30.0), params, None, None, 12.0);
            let pts: Vec<Point> = st.iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect();
            let side1: Vec<f64> = pts.iter().filter(|p| p.y < 3.0).map(|p| p.y).collect();
            let side2: Vec<f64> = pts.iter().filter(|p| p.y >= 3.0).map(|p| 6.0 - p.y).collect();
            (side1, side2)
        };
        let (a, b) = depths(&base);
        assert!(a.iter().chain(&b).all(|d| d.abs() < 1e-9), "plain satin reaches both edges");
        let jag = |sides| SatinParams { jagged: Some(Jagged { sides, depth: 2.0 }), ..base.clone() };
        let (a, b) = depths(&jag(JaggedSides::First));
        assert!(b.iter().all(|d| d.abs() < 1e-9), "side 2 stays clean");
        assert!(a.iter().all(|d| (-1e-9..=2.0 + 1e-9).contains(d)), "{a:?}");
        let spread = a.iter().fold(0.0f64, |m, d| m.max(*d)) - a.iter().fold(f64::INFINITY, |m, d| m.min(*d));
        assert!(spread > 1.5, "the stitches end at different depths: {a:?}");
        let (a, b) = depths(&jag(JaggedSides::Both));
        assert!(a.iter().any(|d| *d > 0.5) && b.iter().any(|d| *d > 0.5));
        // The same column is jagged the same way every time.
        assert_eq!(depths(&jag(JaggedSides::Both)), (a, b));
        // A narrow column's stitches still span at least a fifth of its width.
        for sides in JaggedSides::ALL {
            let deep = SatinParams { jagged: Some(Jagged { sides, depth: 50.0 }), ..base.clone() };
            let st = generate(&straight(2.0, 20.0), &deep, None, None, 12.0);
            let pts: Vec<Point> = st.iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect();
            assert!(pts.windows(2).all(|w| (w[0].y - w[1].y).abs() >= 0.4 - 1e-9), "{sides:?}: {pts:?}");
        }
    }

    #[test]
    fn tatami_borders_split_every_row_in_a_pattern() {
        let bare = SatinUnderlay { auto: false, center_run: false, ..Default::default() };
        let params = SatinParams { tatami: true, split_length: 4.0, auto_spacing: false, pull_comp: 0.0, underlay: bare, ..Default::default() };
        let st = generate(&straight(12.0, 30.0), &params, None, None, 12.0);
        let moves: Vec<Point> = st.iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect();
        let longest = moves.windows(2).map(|w| w[0].distance(w[1])).fold(0.0, f64::max);
        assert!(longest <= 4.0 * 1.3 + 0.05, "{longest}");
        // needle points across the whole column, not only on its sides
        let inside = moves.iter().filter(|p| p.y > 1.0 && p.y < 11.0).count();
        assert!(inside > moves.len() / 2, "{inside} of {}", moves.len());
        // the rows' split points shift from row to row
        let ys: std::collections::BTreeSet<i64> = moves.iter().filter(|p| p.y > 1.0 && p.y < 11.0).map(|p| (p.y * 10.0).round() as i64).collect();
        assert!(ys.len() >= 8, "{ys:?}");
        // satin without it keeps whole stitches
        let satin = generate(&straight(6.0, 30.0), &SatinParams { tatami: false, split_length: 8.0, ..params.clone() }, None, None, 12.0);
        assert!(satin.iter().filter(|s| s.cmd == Cmd::Stitch).all(|s| s.pos().y < 0.3 || s.pos().y > 5.7 || s.pos().x < 0.5 || s.pos().x > 29.5));
    }

    #[test]
    fn a_tiny_split_length_is_raised_to_the_minimum() {
        // #84: a 10 mm wide column 20 mm long with split_length 1e-6 made billions of stitches.
        let bare = SatinUnderlay { auto: false, center_run: false, ..Default::default() };
        let base = SatinParams { auto_spacing: false, pull_comp: 0.0, underlay: bare, ..Default::default() };
        for tatami in [false, true] {
            for split_length in [1e-6, 1e-300, f64::MIN_POSITIVE] {
                let params = SatinParams { tatami, split_length, ..base.clone() };
                let tiny = generate(&straight(10.0, 20.0), &params, None, None, 12.0);
                let at_min = generate(&straight(10.0, 20.0), &SatinParams { split_length: MIN_SPLIT_LENGTH, ..params.clone() }, None, None, 12.0);
                assert_eq!(tiny.len(), at_min.len(), "tatami {tatami}, split {split_length}");
                assert!(tiny.len() < 20_000, "tatami {tatami}: {} stitches", tiny.len());
            }
        }
        // A stitch is never cut into more than MAX_SPLIT_PIECES pieces.
        let mut out = Vec::new();
        split_stitch(Point::new(0.0, 0.0), Point::new(1e9, 0.0), 1.0, 0, &mut out);
        assert_eq!(out.len(), MAX_SPLIT_PIECES);
        out.clear();
        split_tatami(Point::new(0.0, 0.0), Point::new(1e9, 0.0), 1.0, 0, &mut out);
        assert!(out.len() <= MAX_SPLIT_PIECES + 1, "{}", out.len());
    }

    #[test]
    fn puff_caps_reach_past_the_ends() {
        let mut params = SatinParams::default();
        crate::model::Puff::default().apply(&mut params);
        let st = generate(&straight(5.0, 20.0), &params, None, None, 12.0);
        let xs: Vec<f64> = st.iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.x).collect();
        let (lo, hi) = (xs.iter().cloned().fold(f64::INFINITY, f64::min), xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
        assert!((lo + 0.5).abs() < 1e-9 && (hi - 20.5).abs() < 1e-9, "{lo} {hi}");
        // dense: 0.16 mm a cycle along 20 mm, and a cap at each end
        let plain = generate(&straight(5.0, 20.0), &SatinParams::default(), None, None, 12.0);
        assert!(st.len() > 2 * plain.len(), "{} {}", st.len(), plain.len());
        // pushed out at the sides, no underlay inside the column
        let ys: Vec<f64> = st.iter().filter(|s| s.x > 1.5 && s.x < 18.5).map(|s| s.y).collect();
        assert!(ys.iter().all(|y| (y + 0.35).abs() < 1e-9 || (y - 5.35).abs() < 1e-9), "{ys:?}");
        // back to plain satin: no caps
        crate::model::Puff::remove(&mut params);
        let again = generate(&straight(5.0, 20.0), &params, None, None, 12.0);
        assert!(again.iter().all(|s| s.x > -1e-9 && s.x < 20.0 + 1e-9) && params == SatinParams::default());
    }

    #[test]
    fn density_matches_spacing() {
        let params = SatinParams {
            auto_spacing: false,
            pull_comp: 0.0,
            underlay: SatinUnderlay { auto: false, center_run: false, ..Default::default() },
            ..Default::default()
        };
        let st = generate(&straight(3.0, 20.0), &params, None, None, 12.0);
        // 20 mm / 0.4 mm per A-B cycle → 50 cycles → ~101 penetrations
        assert!((st.len() as i64 - 101).abs() <= 2, "{}", st.len());
        for s in &st {
            assert!(s.y.abs() < 1e-9 || (s.y - 3.0).abs() < 1e-9, "{s:?}");
            assert_eq!(s.cmd, Cmd::Stitch);
        }
    }

    #[test]
    fn push_comp_shortens_open_ends_only() {
        let col = Column::new(vec![
            Rung::new(Point::new(0.0, 0.0), Point::new(0.0, 4.0), Default::default()),
            Rung::new(Point::new(20.0, 0.0), Point::new(20.0, 4.0), Default::default()),
        ]);
        let bare = SatinParams { underlay: SatinUnderlay { auto: false, center_run: false, ..Default::default() }, ..Default::default() };
        let span = |p: &SatinParams| {
            let st = generate(&col, p, None, None, 12.1);
            let xs: Vec<f64> = st.iter().filter(|s| s.cmd == crate::pattern::Cmd::Stitch).map(|s| s.x).collect();
            (xs.iter().copied().fold(f64::INFINITY, f64::min), xs.iter().copied().fold(f64::NEG_INFINITY, f64::max))
        };
        let (a0, b0) = span(&bare);
        let (a1, b1) = span(&SatinParams { push_comp: 0.5, ..bare.clone() });
        assert!(a1 - a0 > 0.4 && b0 - b1 > 0.4, "{a0} {b0} → {a1} {b1}");
    }

    /// D4: swapped sides start the zigzag on side 2 and end the column on the other side; side 1
    /// keeps its own settings.
    #[test]
    fn swapped_sides_start_on_side_two() {
        let col = straight(4.0, 20.0);
        let side = |p: &Point| if p.y < 2.0 { 1 } else { 2 };
        use crate::model::{Jagged, JaggedSides};
        let base = SatinParams::default();
        let plain = zigzag_points(&sample_pairs(&col, |_| 0.2), &base, (0.0, 0.0), false);
        let swapped_params = SatinParams { swap_sides: true, ..base.clone() };
        let swapped = zigzag_points(&sample_pairs(&col, |_| 0.2), &swapped_params, (0.0, 0.0), false);
        assert_eq!(plain.len(), swapped.len());
        assert_eq!((side(&plain[0]), side(&swapped[0])), (1, 2));
        assert_ne!(side(&plain[plain.len() - 1]), side(&swapped[swapped.len() - 1]));
        // a jagged side 1 stays side 1
        let jag = Some(Jagged { sides: JaggedSides::First, depth: 1.5 });
        let pts = zigzag_points(&sample_pairs(&col, |_| 0.2), &SatinParams { jagged: jag, ..swapped_params }, (0.0, 0.0), false);
        assert!(pts.iter().filter(|p| p.y >= 2.0).all(|p| (p.y - 4.0).abs() < 1e-9), "side 2 clean");
    }

    #[test]
    fn pull_comp_widens() {
        let params =
            SatinParams { pull_comp: 0.3, underlay: SatinUnderlay { auto: false, center_run: false, ..Default::default() }, ..Default::default() };
        let st = generate(&straight(3.0, 10.0), &params, None, None, 12.0);
        let ys: Vec<f64> = st.iter().map(|s| s.y).collect();
        let min = ys.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!((min + 0.3).abs() < 1e-9 && (max - 3.3).abs() < 1e-9, "{min} {max}");
    }

    #[test]
    fn twin_sided_pull_comp() {
        let params = SatinParams {
            pull_comp: 0.0,
            pull_comp_2: Some(0.45),
            underlay: SatinUnderlay { auto: false, center_run: false, ..Default::default() },
            ..Default::default()
        };
        let st = generate(&straight(3.0, 10.0), &params, None, None, 12.0);
        let min = st.iter().map(|s| s.y).fold(f64::INFINITY, f64::min);
        let max = st.iter().map(|s| s.y).fold(f64::NEG_INFINITY, f64::max);
        // side 1 (y = 0) stays put, side 2 (y = 3) widens by 0.45
        assert!(min.abs() < 1e-9 && (max - 3.45).abs() < 1e-9, "{min} {max}");
        assert_eq!(SatinParams { pull_comp: 0.2, ..Default::default() }.pull_sides(), (0.2, 0.2));
    }

    #[test]
    fn double_zigzag_crosses_the_first() {
        let zz = |double_zigzag| SatinParams {
            underlay: SatinUnderlay { auto: false, center_run: false, zigzag: true, double_zigzag, ..Default::default() },
            ..Default::default()
        };
        let single = generate(&straight(6.0, 20.0), &zz(false), None, None, 12.0);
        let double = generate(&straight(6.0, 20.0), &zz(true), None, None, 12.0);
        assert!(double.len() > single.len() + 10, "{} {}", double.len(), single.len());
        // two underlay traversals: the top satin starts back at the start
        assert!(double.first().unwrap().x < 1.0);
        let top_start = double.iter().position(|s| s.x > 19.0).unwrap();
        let back = double[top_start..].iter().position(|s| s.x < 1.0).unwrap() + top_start;
        // the return pass sews over x where the first pass had the other rail
        let first: Vec<(i64, bool)> = double[..top_start].iter().map(|s| ((s.x * 10.0).round() as i64, s.y < 3.0)).collect();
        let second: Vec<(i64, bool)> = double[top_start..back].iter().map(|s| ((s.x * 10.0).round() as i64, s.y < 3.0)).collect();
        assert!(second.iter().any(|(x, low)| first.iter().any(|(fx, flow)| fx == x && flow != low)));
    }

    #[test]
    fn preserve_long_stitches_moves_the_frame() {
        let params = SatinParams {
            split_length: 0.0,
            preserve_long: true,
            auto_spacing: false,
            pull_comp: 0.0,
            underlay: SatinUnderlay { auto: false, center_run: false, ..Default::default() },
            ..Default::default()
        };
        // a 20 mm wide column: every satin stitch is longer than a 12 mm machine stitch
        let st = generate(&straight(20.0, 4.0), &params, None, None, 12.0);
        let needle: Vec<&Stitch> = st.iter().filter(|s| s.cmd == Cmd::Stitch).collect();
        assert!(needle.iter().all(|s| s.y.abs() < 1e-9 || (s.y - 20.0).abs() < 1e-9), "needle drops only on the rails");
        assert!(st.iter().filter(|s| s.cmd == Cmd::Jump).count() >= needle.len() - 1);
        for w in st.windows(2) {
            assert!(w[0].pos().distance(w[1].pos()) <= 12.0 + 1e-9);
        }
        // without it, the long stitches are left for the machine limit to split
        let plain = generate(&straight(20.0, 4.0), &SatinParams { preserve_long: false, ..params }, None, None, 12.0);
        assert_eq!(plain.iter().filter(|s| s.cmd == Cmd::Jump).count(), 0);
    }

    #[test]
    fn split_long_stitches() {
        let params = SatinParams { split_length: 7.0, ..Default::default() };
        let st = generate(&straight(16.0, 10.0), &params, None, None, 12.0);
        for w in st.windows(2) {
            assert!(w[0].pos().distance(w[1].pos()) <= 7.5, "stitch too long");
        }
    }

    #[test]
    fn underlay_then_satin_returns() {
        // centre run underlay goes out, satin comes back → ends near the start
        let params = SatinParams { underlay: SatinUnderlay { auto: false, center_run: true, ..Default::default() }, ..Default::default() };
        let st = generate(&straight(2.0, 30.0), &params, None, None, 12.0);
        let first = st.first().unwrap().pos();
        let last = st.last().unwrap().pos();
        assert!(first.x < 1.0 && last.x < 1.0, "{first:?} {last:?}");
    }

    #[test]
    fn hint_picks_nearest_end() {
        let params = SatinParams::default();
        let st = generate(&straight(3.0, 30.0), &params, Some(Point::new(40.0, 0.0)), None, 12.0);
        assert!(st[0].x > 29.0);
    }

    #[test]
    fn curved_column_uses_short_stitches() {
        // A tight arc: inner radius 2, outer radius 6.
        let rungs: Vec<Rung> = (0..=6)
            .map(|i| {
                let a = i as f64 / 6.0 * std::f64::consts::PI;
                let d = Point::new(a.cos(), a.sin());
                Rung::new(d * 6.0, d * 2.0, NodeKind::Curve)
            })
            .collect();
        let col = Column::new(rungs);
        let st = generate(&col, &SatinParams::default(), None, None, 12.0);
        // some penetrations must lie strictly between the rails (shortened stitches)
        let mid = st.iter().filter(|s| {
            let r = s.pos().length();
            r > 2.3 && r < 5.0
        });
        assert!(mid.count() > 3);
        let _ = Node::corner(Point::ZERO);
    }

    #[test]
    fn closed_border() {
        let ring = vec![Point::new(0.0, 0.0), Point::new(20.0, 0.0), Point::new(20.0, 20.0), Point::new(0.0, 20.0), Point::new(0.0, 0.0)];
        let pts = border(&ring, true, 3.0, 0.4, 0.0);
        assert!(pts.len() > 300);
        for p in &pts {
            let inside_outer = p.x > -2.2 && p.x < 22.2 && p.y > -2.2 && p.y < 22.2;
            let outside_inner = !(p.x > 1.55 && p.x < 18.45 && p.y > 1.55 && p.y < 18.45);
            assert!(inside_outer && outside_inner, "{p:?}");
        }
    }
}
