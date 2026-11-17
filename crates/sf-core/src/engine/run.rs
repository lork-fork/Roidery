//! Line stitches along a path: single / triple run, backstitch, stem stitch,
//! blanket, zigzag and motifs, sewn once or in several passes.

use super::out::Out;
use crate::geom::{Contour, FLATTEN_TOLERANCE, Point, polyline};
use crate::model::{CustomMotif, MAX_MOTIF_POINTS, MAX_PASSES, Motif, RunKind, RunParams};
use crate::pattern::Stitch;

/// Turn angle (radians) above which a path vertex is treated as a corner that
/// must receive a needle penetration.
pub(crate) const CORNER_ANGLE: f64 = 0.35;

/// Prepares the path polyline and whether to sew it backwards: closed paths start
/// nearest to `hint`; open paths keep their drawn direction and are sewn backwards
/// if their far end is closer to it. Without a hint, an open path ends near `exit`,
/// or starts there when the stitching comes back to its start (`back_to_start`: an
/// even number of passes).
fn oriented_path(path: &Contour, hint: Option<Point>, exit: Option<Point>, back_to_start: bool) -> (Vec<Point>, bool) {
    let pts = path.polyline(FLATTEN_TOLERANCE);
    if pts.len() < 2 {
        return (pts, false);
    }
    if path.closed {
        let ring = match hint {
            Some(h) => polyline::ring_starting_near(&pts, h),
            None => pts,
        };
        (polyline::closed(&ring), false)
    } else {
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        let reverse = match (hint, exit) {
            (Some(h), _) => h.distance(last) < h.distance(first),
            (None, Some(e)) if back_to_start => e.distance(last) < e.distance(first),
            (None, Some(e)) => e.distance(first) < e.distance(last),
            _ => false,
        };
        (pts, reverse)
    }
}

pub(crate) fn generate(path: &Contour, params: &RunParams, hint: Option<Point>, exit: Option<Point>) -> Vec<Stitch> {
    let passes = params.passes.clamp(1, MAX_PASSES);
    // Stitches are laid out along the path as drawn (so prongs, motifs and their grading stay
    // on their side and in their order), then sewn backwards when it starts at the far end.
    let (pts, backwards) = oriented_path(path, hint, exit, passes.is_multiple_of(2));
    if pts.is_empty() {
        return Vec::new();
    }
    if pts.len() == 1 {
        return super::out::as_stitches([pts[0]]);
    }
    // A very long path gets longer stitches (and wider zigzag/blanket spacing) so the whole
    // object, all passes together, stays within `MAX_RUN_STITCHES` (#79). Below the cap the
    // floor is far under the smallest stitch, so nothing changes. Up to about four needle
    // points are made per step (triple run, backstitch, blanket prongs).
    let total = polyline::length(&pts);
    let per_pass = MAX_RUN_STITCHES as f64 / (f64::from(passes) * 4.0);
    let floor = if total.is_finite() { total / per_pass } else { 0.0 };
    let step = params.stitch_length.max(0.3).max(floor);
    let spacing = params.spacing.max(floor);
    let once = match params.kind {
        RunKind::Single => polyline::resample(&pts, step, CORNER_ANGLE),
        RunKind::Triple => triple(&polyline::resample(&pts, step, CORNER_ANGLE)),
        RunKind::Blanket => blanket(&pts, step, params.width, spacing, params.flip),
        RunKind::Zigzag => zigzag(&pts, params.width, spacing),
        RunKind::Motif => {
            let shape = motif_points(params.motif, params.custom_motif.as_ref());
            motif(&pts, &shape, (params.width, params.width_end), params.spacing, step, params.flip)
        }
        RunKind::Back => backstitch(&pts, step, 0.0),
        RunKind::Stem => backstitch(&pts, step, (params.width * 0.5).clamp(0.2, 3.0) * if params.flip { -1.0 } else { 1.0 }),
        RunKind::Hand => hand(&pts, step, params.spacing),
        RunKind::Sculpture => sculpture(&pts, (step, params.spacing), params.groups),
        RunKind::String => string(&pts, params.width, params.spacing, path.closed),
    };
    let mut once = once;
    if backwards {
        once.reverse();
    }
    let mut out = Out::new();
    out.stitches(repeat_passes(&once, passes));
    out.finish()
}

/// Most needle points one line stitch object makes, all passes together (#79): a 1 km
/// single run at 1 mm is sewn whole; a longer one gets longer stitches.
pub(crate) const MAX_RUN_STITCHES: usize = 1_000_000;

/// The stitching sewn `passes` times, every other pass backwards over the same
/// needle points (2 passes = a backtrack to the start), stopping at
/// [`MAX_RUN_STITCHES`] points.
fn repeat_passes(once: &[Point], passes: u32) -> Vec<Point> {
    let mut out: Vec<Point> = once.iter().take(MAX_RUN_STITCHES).copied().collect();
    for k in 1..passes {
        let room = MAX_RUN_STITCHES.saturating_sub(out.len());
        if room == 0 {
            break;
        }
        if k % 2 == 1 {
            out.extend(once.iter().rev().skip(1).take(room));
        } else {
            out.extend(once.iter().skip(1).take(room));
        }
    }
    out
}

/// Most stitches a hand stitch run is cut into.
const MAX_HAND_STITCHES: usize = 100_000;

/// Hand stitch: stitches about `step` long, each sewn three times (forward, back and forward)
/// so it lies thick, joined by single stitches about `gap` long that hardly show, every length
/// varied a little at random (the same way each time), like a running stitch sewn by hand.
/// The line starts and ends with a thick stitch.
pub(crate) fn hand(pts: &[Point], step: f64, gap: f64) -> Vec<Point> {
    let total = polyline::length(pts);
    let Some(&first) = pts.first() else { return Vec::new() };
    if !(total.is_finite() && total > 1e-6) {
        return pts.to_vec();
    }
    let step = step.clamp(0.5, 12.0);
    let gap = if gap.is_finite() { gap.clamp(0.3, 2.0 * step) } else { step * 0.5 };
    // n thick stitches and n − 1 gaps, scaled to the line's length.
    let n = ((total + gap) / (step + gap)).round().clamp(1.0, MAX_HAND_STITCHES as f64) as usize;
    let mut rng = super::special::Rng::new(total.to_bits() ^ first.x.to_bits().rotate_left(17) ^ first.y.to_bits().rotate_left(41));
    let lens: Vec<f64> = (0..2 * n - 1)
        .map(|i| {
            let base = if i % 2 == 0 { step } else { gap };
            base * (0.8 + 0.4 * rng.unit())
        })
        .collect();
    let k = total / lens.iter().sum::<f64>();
    let mut out = Vec::with_capacity(4 * n);
    out.push(first);
    let mut s = 0.0;
    for (i, len) in lens.iter().enumerate() {
        let from = s;
        s = (s + len * k).min(total);
        let to = polyline::point_at(pts, s);
        if i % 2 == 0 {
            out.extend([to, polyline::point_at(pts, from), to]);
        } else {
            out.push(to);
        }
    }
    out
}

/// Backstitch: needle points every half stitch along the line; each stitch goes
/// forward two of them and back one, so consecutive stitches overlap by half.
/// With `slant` ≠ 0 (stem stitch), the back points sit that far to one side of
/// the line, so the overlapping stitches lie slanted like a twisted rope.
pub(crate) fn backstitch(pts: &[Point], step: f64, slant: f64) -> Vec<Point> {
    let q = polyline::resample(pts, step * 0.5, CORNER_ANGLE);
    let n = q.len();
    if n < 3 {
        return q;
    }
    let normals = polyline::vertex_normals(&q, false);
    let mut out = Vec::with_capacity(n * 2);
    out.push(q[0]);
    let mut i = 0;
    while i + 2 < n {
        out.push(q[i + 2]);
        if i + 3 < n {
            let back = q[i + 1] + normals.get(i + 1).copied().unwrap_or(Point::ZERO) * slant;
            out.push(back);
        }
        i += 1;
    }
    out
}

/// Most stitches a sculpture run is cut into.
const MAX_SCULPTURE_STITCHES: usize = 200_000;

/// Sculpture run: a thick-thin line. `groups[0]` triple stitches `lengths.0` long (each sewn
/// forward, back and forward again) in turn with `groups[1]` single stitches `lengths.1` long,
/// all scaled a little so the line ends on a whole stitch.
pub(crate) fn sculpture(pts: &[Point], lengths: (f64, f64), groups: [u32; 2]) -> Vec<Point> {
    let total = polyline::length(pts);
    let Some(&first) = pts.first() else { return Vec::new() };
    if !(total.is_finite() && total > 1e-6) {
        return pts.to_vec();
    }
    let len = |l: f64, default: f64| if l.is_finite() { l.clamp(0.5, 12.0) } else { default };
    let (a, b) = (len(lengths.0, 2.5), len(lengths.1, 2.0));
    let [na, nb] = groups.map(|n| n.min(20));
    let [na, nb] = if na + nb == 0 { crate::model::SCULPTURE_GROUPS } else { [na, nb] };
    // One turn of the pattern: (length, triple).
    let unit: Vec<(f64, bool)> = std::iter::repeat_n((a, true), na as usize).chain(std::iter::repeat_n((b, false), nb as usize)).collect();
    let mut seq: Vec<(f64, bool)> = Vec::new();
    let mut s = 0.0;
    'line: while seq.len() < MAX_SCULPTURE_STITCHES {
        for &(l, triple) in &unit {
            if s + l * 0.5 > total {
                break 'line;
            }
            seq.push((l, triple));
            s += l;
        }
    }
    if seq.is_empty() {
        seq.push((total, na > 0));
    }
    let k = total / seq.iter().map(|u| u.0).sum::<f64>();
    let mut out = Vec::with_capacity(seq.len() * 3 + 1);
    out.push(first);
    let (mut at, mut from) = (0.0, first);
    for (l, triple) in seq {
        at = (at + l * k).min(total);
        let to = polyline::point_at(pts, at);
        if triple {
            out.extend([to, from, to]);
        } else {
            out.push(to);
        }
        from = to;
    }
    out
}

/// Most long stitches one string run sews.
const MAX_STRINGS: usize = 100_000;

/// String stitching (string art, as in Paghadi work): straight stitches each spanning `chord` mm
/// of the line, one starting every `spacing` mm, joined by shorter stitches back along it. The
/// long stitches overlap, and on curves they draw a curve inside the line. Along an open line
/// the last one ends at its end; round a closed line (`closed`, the polyline back at its start)
/// they go all the way round.
pub(crate) fn string(pts: &[Point], chord: f64, spacing: f64, closed: bool) -> Vec<Point> {
    let total = polyline::length(pts);
    if pts.is_empty() || !(total.is_finite() && total > 1e-6) {
        return pts.to_vec();
    }
    let spacing = if spacing.is_finite() { spacing.clamp(0.3, 50.0) } else { 1.0 };
    let chord = if chord.is_finite() { chord.clamp(spacing, 200.0) } else { 6.0 };
    let chord = chord.min(if closed { total * 0.5 } else { total });
    let at = |s: f64| polyline::point_at(pts, if closed { s.rem_euclid(total) } else { s.clamp(0.0, total) });
    // Where the long stitches start: round a closed line all the way, along an open one up to a
    // chord before its end.
    let reach = if closed { total } else { (total - chord).max(0.0) };
    let n = ((reach / spacing).round() as usize).clamp(1, MAX_STRINGS);
    let starts: Vec<f64> =
        if closed { (0..n).map(|k| k as f64 * total / n as f64).collect() } else { (0..=n).map(|k| k as f64 * reach / n as f64).collect() };
    let mut out = Vec::with_capacity(starts.len() * 2);
    for s in starts {
        out.extend([at(s), at(s + chord)]);
    }
    polyline::dedup(&out, 1e-9)
}

/// Bean stitch: every stitch is sewn forward, back and forward again.
pub(crate) fn triple(pts: &[Point]) -> Vec<Point> {
    let mut out = Vec::with_capacity(pts.len() * 3);
    if let Some(&first) = pts.first() {
        out.push(first);
    }
    for w in pts.windows(2) {
        out.push(w[1]);
        out.push(w[0]);
        out.push(w[1]);
    }
    out
}

/// Frame (point, unit normal) at arc length `s` along the polyline.
fn frame(pts: &[Point], s: f64) -> (Point, Point) {
    (polyline::point_at(pts, s), polyline::tangent_at(pts, s).perp())
}

/// Blanket / E-stitch: running stitch with prongs every `spacing` mm.
pub(crate) fn blanket(pts: &[Point], step: f64, prong: f64, spacing: f64, flip: bool) -> Vec<Point> {
    let side = if flip { -1.0 } else { 1.0 };
    let anchors = polyline::resample(pts, spacing.max(0.5), CORNER_ANGLE);
    let normals = polyline::vertex_normals(&anchors, false);
    let mut out = Vec::new();
    for (i, (&a, &n)) in anchors.iter().zip(&normals).enumerate() {
        if i > 0 {
            // base line from the previous anchor, subdivided if long
            let prev = anchors[i - 1];
            let k = ((prev.distance(a) / step).ceil() as usize).max(1);
            for j in 1..=k {
                out.push(prev.lerp(a, j as f64 / k as f64));
            }
        } else {
            out.push(a);
        }
        out.push(a + n * (prong * side));
        out.push(a);
    }
    out
}

/// Open zigzag of total `width` with peaks every `spacing` mm.
pub(crate) fn zigzag(pts: &[Point], width: f64, spacing: f64) -> Vec<Point> {
    let len = polyline::length(pts);
    let half = (spacing.max(0.2)) / 2.0;
    let n = ((len / half).ceil() as usize).max(1);
    let mut out = Vec::with_capacity(n + 1);
    for k in 0..=n {
        let s = len * k as f64 / n as f64;
        let (p, nrm) = frame(pts, s);
        let side = if k % 2 == 0 { 0.5 } else { -0.5 };
        out.push(p + nrm * (width * side));
    }
    out
}

/// A motif's line in a repeat one long (along the run) and one wide (across it), from `(0, 0)`
/// to `(1, 0)` so repeats join. Across is positive to the right of the run: down on screen for
/// a left-to-right line, so a motif that stands upright reaches up with negative values.
pub(crate) fn motif_shape(m: Motif) -> Vec<(f64, f64)> {
    match m {
        Motif::Wave => (0..=8)
            .map(|i| {
                let t = i as f64 / 8.0;
                (t, 0.5 * (t * std::f64::consts::TAU).sin())
            })
            .collect(),
        Motif::Scallop => (0..=8)
            .map(|i| {
                let t = i as f64 / 8.0;
                (t, (t * std::f64::consts::PI).sin())
            })
            .collect(),
        Motif::Diamond => vec![(0.0, 0.0), (0.5, 0.5), (1.0, 0.0), (0.5, -0.5), (0.0, 0.0), (1.0, 0.0)],
        Motif::Cross => vec![(0.0, 0.0), (0.0, -0.5), (1.0, 0.5), (1.0, -0.5), (0.0, 0.5), (0.0, 0.0), (1.0, 0.0)],
        Motif::Feather => vec![(0.0, 0.0), (0.3, 0.5), (0.5, 0.0), (0.8, -0.5), (1.0, 0.0)],
        // A heart standing on its point, reached from the line and left back to it.
        Motif::Heart => vec![
            (0.0, 0.0),
            (0.5, 0.45),
            (0.15, -0.05),
            (0.13, -0.25),
            (0.22, -0.4),
            (0.36, -0.43),
            (0.5, -0.3),
            (0.64, -0.43),
            (0.78, -0.4),
            (0.87, -0.25),
            (0.85, -0.05),
            (0.5, 0.45),
            (1.0, 0.0),
        ],
        // A five-pointed star round the middle of the repeat, one point up.
        Motif::Star => {
            let mut v = vec![(0.0, 0.0)];
            v.extend((0..=10).map(|k| {
                let r = if k % 2 == 0 { 0.48 } else { 0.2 };
                let a = std::f64::consts::FRAC_PI_2 + f64::from(k) * std::f64::consts::PI / 5.0;
                (0.5 + r * a.cos(), -r * a.sin())
            }));
            v.push((1.0, 0.0));
            v
        }
        // A leaf: its upper edge, its lower edge back, then the vein.
        Motif::Leaf => {
            let edge = |side: f64| (0..=8).map(move |i| f64::from(i) / 8.0).map(move |t| (t, side * 0.4 * (t * std::f64::consts::PI).sin()));
            let mut v: Vec<(f64, f64)> = edge(1.0).collect();
            v.extend(edge(-1.0).collect::<Vec<_>>().into_iter().rev().skip(1));
            v.push((1.0, 0.0));
            v
        }
        Motif::Zigzag => vec![(0.0, 0.0), (0.25, 0.5), (0.75, -0.5), (1.0, 0.0)],
        // A prong out to one side and back, then along: blanket stitch.
        Motif::EStitch => vec![(0.0, 0.0), (0.0, 1.0), (0.0, 0.0), (1.0, 0.0)],
        // A coil: the line runs on while turning a full loop to one side and back across
        // itself (a prolate cycloid), so the loops overlap like chenille's.
        // Ten stitches a loop, so they stay long enough to sew.
        Motif::Coil => (0..=10)
            .map(|i| {
                let a = f64::from(i) / 10.0 * std::f64::consts::TAU;
                (a / std::f64::consts::TAU + 0.45 * a.sin(), -0.5 * (1.0 - a.cos()))
            })
            .collect(),
        // Blackwork: up, along, down across the line, along, back up to it: a stepped key, every
        // repeat the same, so a row of them reads as a counted-thread border.
        Motif::Blackwork => vec![(0.0, 0.0), (0.0, 0.4), (0.4, 0.4), (0.4, -0.4), (0.8, -0.4), (0.8, 0.0), (1.0, 0.0)],
        // Candlewicking: along to the middle, a knot of four short crossing stitches round it,
        // back to the middle, on.
        Motif::Candlewick => {
            let k = 0.14;
            vec![(0.0, 0.0), (0.5, 0.0), (0.5 + k, k), (0.5 - k, -k), (0.5 + k, -k), (0.5 - k, k), (0.5, 0.0), (1.0, 0.0)]
        }
        // A square loop: on, down, back across, up past where it went down, on.
        Motif::SquareLoop => vec![(0.0, 0.0), (0.75, 0.0), (0.75, -1.0), (0.25, -1.0), (0.25, 0.0), (1.0, 0.0)],
        // A loop like a handwritten l.
        Motif::Loop => {
            let mut v = vec![(0.0, 0.0)];
            v.extend((0..=12).map(|i| {
                let a = f64::from(i) / 12.0 * std::f64::consts::TAU;
                (0.55 + 0.25 * a.sin(), 0.25 * a.cos() - 0.25)
            }));
            v.push((1.0, 0.0));
            v
        }
    }
}

/// The line a run or a motif fill sews for its motif: its custom motif's, else the built-in one's.
pub(crate) fn motif_points(m: Motif, custom: Option<&CustomMotif>) -> Vec<(f64, f64)> {
    custom.and_then(custom_shape).unwrap_or_else(|| motif_shape(m))
}

/// A custom motif's points fit to sew: the finite ones, kept near their repeat, at most
/// [`MAX_MOTIF_POINTS`]; none when fewer than two are left.
fn custom_shape(m: &CustomMotif) -> Option<Vec<(f64, f64)>> {
    let pts: Vec<(f64, f64)> = m
        .points
        .iter()
        .take(MAX_MOTIF_POINTS)
        .filter(|[a, c]| a.is_finite() && c.is_finite())
        .map(|&[a, c]| (a.clamp(-2.0, 3.0), c.clamp(-2.0, 2.0)))
        .collect();
    (pts.len() >= 2).then_some(pts)
}

/// A motif made from a line drawn in the design (mm, y down), named `name`. An open line runs
/// from `(0, 0)` at its first point to `(1, 0)` at its last, so it sews along the line between
/// them as drawn along a left-to-right one. A line that ends where it starts (or close to it)
/// has no such line: it lies on the middle of its height, from its left side to its right, and
/// is sewn once round from its point nearest the left side, then on to its point nearest the
/// right side. Across, the points are scaled to a range of one; `aspect` keeps the drawn
/// proportions. None when the line is too small, too detailed even simplified, or not finite.
pub(crate) fn motif_from_line(name: &str, line: &[Point]) -> Option<CustomMotif> {
    // A very long line is thinned out first, so simplifying it stays quick.
    let stride = line.len().div_ceil(MAX_MOTIF_LINE).max(1);
    let mut thinned: Vec<Point> = line.iter().step_by(stride).copied().collect();
    if !line.len().saturating_sub(1).is_multiple_of(stride)
        && let Some(&end) = line.last()
    {
        thinned.push(end);
    }
    let pts = polyline::dedup(&thinned.into_iter().filter(|p| p.x.is_finite() && p.y.is_finite()).collect::<Vec<_>>(), 1e-6);
    let (&first, &last) = (pts.first()?, pts.last()?);
    let (lo, hi) =
        pts.iter().fold((first, first), |(lo, hi), p| (Point::new(lo.x.min(p.x), lo.y.min(p.y)), Point::new(hi.x.max(p.x), hi.y.max(p.y))));
    let extent = lo.distance(hi);
    if pts.len() < 2 || !(extent.is_finite() && extent > 0.1) {
        return None;
    }
    let chord = last - first;
    let (origin, along, line): (Point, Point, Vec<Point>) = if chord.length() >= 0.25 * extent {
        (first, chord, pts)
    } else {
        // Round once from the point nearest the left side, then on to the one nearest the right.
        let mid = (lo.y + hi.y) * 0.5;
        let (left, right) = (Point::new(lo.x, mid), Point::new(hi.x, mid));
        if hi.x - lo.x < 0.1 {
            return None;
        }
        let ring: Vec<Point> = if first.distance(last) < 1e-6 { pts[..pts.len() - 1].to_vec() } else { pts };
        let nearest = |q: Point| (0..ring.len()).min_by(|&i, &j| ring[i].distance(q).total_cmp(&ring[j].distance(q))).unwrap_or(0);
        let (i0, j0) = (nearest(left), nearest(right));
        let n = ring.len();
        let mut v = vec![left];
        v.extend((0..=n).map(|k| ring[(i0 + k) % n]));
        let ahead = (j0 + n - i0) % n;
        if 2 * ahead <= n {
            v.extend((1..=ahead).map(|k| ring[(i0 + k) % n]));
        } else {
            v.extend((1..=n - ahead).map(|k| ring[(i0 + n - k) % n]));
        }
        v.push(right);
        (left, right - left, v)
    };
    let len = along.length();
    let (u, w) = (along * (1.0 / len), (along * (1.0 / len)).perp());
    let mut raw: Vec<(f64, f64)> = line.iter().map(|&p| ((p - origin).dot(u) / len, (p - origin).dot(w) / len)).collect();
    // The ends exactly on the line, so repeats join.
    if let Some(a) = raw.first_mut() {
        *a = (0.0, 0.0);
    }
    if let Some(z) = raw.last_mut() {
        *z = (1.0, 0.0);
    }
    let (c0, c1) = raw.iter().fold((0.0f64, 0.0f64), |(lo, hi), &(_, c)| (lo.min(c), hi.max(c)));
    let height = c1 - c0;
    let k = if height > 1e-6 { 1.0 / height } else { 1.0 };
    let mut points: Vec<[f64; 2]> = raw.iter().map(|&(a, c)| [a, c * k]).collect();
    // Too detailed: keep fewer points, a little less exactly each time.
    let mut tol = 0.002;
    while points.len() > MAX_MOTIF_POINTS && tol < 0.1 {
        let simple = polyline::simplify(&points.iter().map(|&[a, c]| Point::new(a, c)).collect::<Vec<_>>(), tol);
        points = simple.iter().map(|p| [p.x, p.y]).collect();
        tol *= 2.0;
    }
    if points.len() > MAX_MOTIF_POINTS || points.len() < 2 {
        return None;
    }
    Some(CustomMotif { name: name.to_string(), points, aspect: height.max(0.05) })
}

/// Most points of a drawn line Make Motif looks at.
const MAX_MOTIF_LINE: usize = 20_000;
/// Motifs along a satin column (a graded motif column) given as pairs of rail points along it:
/// each as wide as the column where it starts (a tenth less, so it stays inside) and `length`
/// times as long, its across direction following the column's rungs, upright along a column
/// running left to right as on a run; no stitch longer than `step`.
pub(crate) fn motif_column(pairs: &[(Point, Point)], shape: &[(f64, f64)], length: f64, step: f64) -> Vec<Point> {
    let mids: Vec<Point> = pairs.iter().map(|(a, b)| a.midpoint(*b)).collect();
    let cum = polyline::cumulative_lengths(&mids);
    let total = cum.last().copied().unwrap_or(0.0);
    if mids.len() < 2 || !(total.is_finite() && total > 1e-6) {
        return Vec::new();
    }
    let length = if length.is_finite() { length.clamp(0.2, 10.0) } else { 1.5 };
    let step = if step.is_finite() { step.clamp(0.3, 12.0) } else { 1.5 };
    // The motif centred across, a range of one.
    let (lo, hi) = shape.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &(_, c)| (lo.min(c), hi.max(c)));
    let (mid, range) = ((lo + hi) * 0.5, (hi - lo).max(1e-6));
    // The column at arc length `s` along its middle: the middle point, the width, and the unit
    // across it along the rung, turned to the right of the way along.
    let at = |s: f64| -> (Point, f64, Point) {
        let s = s.clamp(0.0, total);
        let i = cum.partition_point(|&c| c < s).clamp(1, cum.len() - 1);
        let (Some(&(a0, b0)), Some(&(a1, b1)), Some(&s0), Some(&s1)) = (pairs.get(i - 1), pairs.get(i), cum.get(i - 1), cum.get(i)) else {
            return (Point::ZERO, 0.0, Point::ZERO);
        };
        let t = if s1 > s0 { (s - s0) / (s1 - s0) } else { 0.0 };
        let (a, b) = (a0.lerp(a1, t), b0.lerp(b1, t));
        let w = a.distance(b);
        let d = a1.midpoint(b1) - a0.midpoint(b0);
        let along = if d.length() > 1e-12 { d * (1.0 / d.length()) } else { Point::new(1.0, 0.0) };
        let rung = if w > 1e-9 { (b - a) * (1.0 / w) } else { along.perp() };
        let u = if rung.dot(along.perp()) < 0.0 { rung * -1.0 } else { rung };
        (a.midpoint(b), w, u)
    };
    // Each repeat as long as `length` times the width where it starts, then all scaled to fit.
    let mut sizes = Vec::new();
    let mut s = 0.0;
    while s < total && sizes.len() < MAX_MOTIFS {
        let r = (at(s).1 * length).max(0.5);
        sizes.push(r);
        s += r;
    }
    let k = total / sizes.iter().sum::<f64>().max(1e-9);
    let mut out: Vec<Point> = Vec::new();
    let mut s0 = 0.0;
    'repeats: for r in sizes {
        let r = r * k;
        for &(a, c) in shape {
            let (p, w, u) = at(s0 + a * r);
            let q = p + u * ((c - mid) / range * w * 0.9);
            if let Some(&last) = out.last() {
                let parts = ((last.distance(q) / step).ceil() as usize).clamp(1, 10_000);
                out.extend((1..=parts).map(|j| last.lerp(q, j as f64 / parts as f64)));
            } else {
                out.push(q);
            }
            if out.len() > MAX_MOTIF_STITCHES {
                break 'repeats;
            }
        }
        s0 += r;
    }
    polyline::dedup(&out, 1e-6)
}

/// Most motif repeats along one line.
const MAX_MOTIFS: usize = 50_000;
/// Most needle points one motif run makes.
const MAX_MOTIF_STITCHES: usize = 2_000_000;
/// Widest motif (mm), beyond any hoop.
const MAX_MOTIF_WIDTH: f64 = 500.0;

/// Repeating decorative motif (the line `shape`, [`motif_shape`]) along the path, `widths.0`
/// wide; with `widths.1`, the motifs grow or shrink to that width at the end (graded), their
/// repeats `spacing` long at the start and keeping their proportions.
pub(crate) fn motif(pts: &[Point], shape: &[(f64, f64)], widths: (f64, Option<f64>), spacing: f64, step: f64, flip: bool) -> Vec<Point> {
    let len = polyline::length(pts);
    let width = if widths.0.is_finite() { widths.0.clamp(0.0, MAX_MOTIF_WIDTH) } else { 2.0 };
    let end = widths.1.filter(|w| w.is_finite() && *w > 0.0 && width > 1e-9).map(|w| w.min(MAX_MOTIF_WIDTH));
    let rep = spacing.max(0.5);
    // Detailed motifs repeat fewer times at most.
    let max_repeats = (MAX_MOTIF_STITCHES / shape.len().max(1)).clamp(1, MAX_MOTIFS);
    // Where each repeat starts along the line, and its width.
    let mut repeats: Vec<(f64, f64, f64)> = Vec::new();
    match end {
        None => {
            let n = ((len / rep).round() as usize).clamp(1, max_repeats);
            let r = len / n as f64;
            repeats.extend((0..n).map(|k| (k as f64 * r, r, width)));
        }
        Some(w1) => {
            // Lay repeats out at their own sizes, then scale them all to fit the line.
            let mut s = 0.0;
            let mut sizes = Vec::new();
            while s < len && sizes.len() < max_repeats {
                let w = width + (w1 - width) * (s / len).clamp(0.0, 1.0);
                let r = (rep * w / width).max(0.5);
                sizes.push((r, w));
                s += r;
            }
            let k = len / sizes.iter().map(|x| x.0).sum::<f64>().max(1e-9);
            let mut at = 0.0;
            for (r, w) in sizes {
                repeats.push((at, r * k, w));
                at += r * k;
            }
        }
    }
    let side = if flip { -1.0 } else { 1.0 };
    let mut out: Vec<Point> = Vec::new();
    'repeats: for (s0, rep, width) in repeats {
        for &(a, c) in shape {
            let (p, nrm) = frame(pts, (s0 + a * rep).clamp(0.0, len));
            let q = p + nrm * (c * width * side);
            if let Some(&last) = out.last() {
                let d: f64 = last.distance(q);
                let parts = ((d / step).ceil() as usize).clamp(1, 10_000);
                for j in 1..=parts {
                    out.push(last.lerp(q, j as f64 / parts as f64));
                }
            } else {
                out.push(q);
            }
            if out.len() > MAX_MOTIF_STITCHES {
                break 'repeats;
            }
        }
    }
    polyline::dedup(&out, 1e-6)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::Cmd;

    fn line() -> Contour {
        Contour::from_points(&[Point::new(0.0, 0.0), Point::new(20.0, 0.0)], false)
    }

    /// #79: a 10 km line sewn as a triple run in nine passes, or as a fine zigzag, used to ask
    /// for hundreds of millions of points and run the app out of memory. It now stops at the cap;
    /// a short line is sewn exactly as before.
    #[test]
    fn a_very_long_run_stops_at_the_stitch_cap() {
        let far = Contour::from_points(&[Point::new(0.0, 0.0), Point::new(1e7, 0.0)], false);
        let kinds = [RunKind::Single, RunKind::Triple, RunKind::Back, RunKind::Stem, RunKind::Blanket, RunKind::Zigzag];
        for kind in kinds {
            let p = RunParams { kind, passes: MAX_PASSES, stitch_length: 0.3, spacing: 0.2, width: 2.0, ..Default::default() };
            let started = std::time::Instant::now();
            let out = generate(&far, &p, None, None);
            assert!(out.len() <= MAX_RUN_STITCHES + 16, "{kind:?}: {}", out.len());
            assert!(out.len() > 1000, "{kind:?}: {}", out.len());
            assert!(out.iter().all(|s| s.x.is_finite() && s.y.is_finite()), "{kind:?}");
            assert!(started.elapsed().as_secs() < 20, "{kind:?} took {:?}", started.elapsed());
        }
        // A short line: 20 mm at 2 mm is 11 points, as always.
        let p = RunParams { kind: RunKind::Single, stitch_length: 2.0, ..Default::default() };
        assert_eq!(generate(&line(), &p, None, None).len(), 11);
    }

    #[test]
    fn every_motif_joins_its_repeats_and_graded_runs_change_size() {
        for m in Motif::ALL {
            let shape = motif_shape(m);
            let near = |p: Option<&(f64, f64)>, q: (f64, f64)| p.is_some_and(|p| (p.0 - q.0).abs() < 1e-9 && (p.1 - q.1).abs() < 1e-9);
            assert!(near(shape.first(), (0.0, 0.0)) && near(shape.last(), (1.0, 0.0)), "{m:?}: {shape:?}");
            assert!(shape.iter().all(|(a, c)| (-0.01..=1.01).contains(a) && c.abs() <= 1.0 + 1e-9), "{m:?}");
        }
        // Scallops 2 mm high at the start of a 100 mm line, 6 mm at its end.
        let line = Contour::from_points(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0)], false);
        let p = RunParams { kind: RunKind::Motif, motif: Motif::Scallop, width: 2.0, width_end: Some(6.0), spacing: 4.0, ..Default::default() };
        let pts: Vec<Point> = generate(&line, &p, None, None).iter().map(|s| s.pos()).collect();
        let height = |lo: f64, hi: f64| pts.iter().filter(|q| q.x >= lo && q.x <= hi).map(|q| q.y.abs()).fold(0.0, f64::max);
        assert!(height(0.0, 15.0) < 2.7 && height(85.0, 100.0) > 5.0, "{} {}", height(0.0, 15.0), height(85.0, 100.0));
        assert!(pts.last().unwrap().distance(Point::new(100.0, 0.0)) < 1e-6, "ends on the line's end");
        // Sewn from the far end (the closest join), the motifs still grow along the line as drawn.
        let back: Vec<Point> = generate(&line, &p, Some(Point::new(101.0, 0.0)), None).iter().map(|s| s.pos()).collect();
        assert!(back.first().unwrap().distance(Point::new(100.0, 0.0)) < 1e-6, "starts at the far end");
        let rev: Vec<Point> = pts.iter().rev().copied().collect();
        assert_eq!(back, rev);
        // Without an end width every motif is the same; absurd ones change nothing.
        let even = RunParams { width_end: None, ..p.clone() };
        for w in [f64::NAN, -1.0, 0.0] {
            let odd = RunParams { width_end: Some(w), ..p.clone() };
            assert_eq!(generate(&line, &odd, None, None), generate(&line, &even, None, None), "{w}");
        }
    }

    #[test]
    fn a_drawn_line_makes_a_motif_that_sews_the_way_it_was_drawn() {
        // A peak drawn upward (−y on screen) from (10, 50) to (30, 50), 5 mm high.
        let drawn = [Point::new(10.0, 50.0), Point::new(20.0, 45.0), Point::new(30.0, 50.0)];
        let m = motif_from_line("Peak", &drawn).unwrap();
        assert_eq!((m.name.as_str(), m.points.clone()), ("Peak", vec![[0.0, 0.0], [0.5, -1.0], [1.0, 0.0]]));
        assert!((m.aspect - 0.25).abs() < 1e-9, "{}", m.aspect);
        // Drawn right to left or slanted, it is the same motif.
        let back: Vec<Point> = drawn.iter().rev().map(|p| Point::new(40.0 - p.x, p.y)).collect();
        assert_eq!(motif_from_line("Peak", &back).unwrap().points, m.points);
        // Along a left-to-right run, 2 mm wide, its peaks rise 2 mm as drawn.
        let p = RunParams { kind: RunKind::Motif, custom_motif: Some(m.clone()), width: 2.0, spacing: 4.0, ..Default::default() };
        let pts: Vec<Point> = generate(&line(), &p, None, None).iter().map(|s| s.pos()).collect();
        let (lo, hi) = pts.iter().fold((0.0f64, 0.0f64), |(lo, hi), q| (lo.min(q.y), hi.max(q.y)));
        assert!((lo + 2.0).abs() < 1e-6 && hi.abs() < 1e-6, "{lo} {hi}");
        // The built-in star and loop stand upright the same way.
        for motif in [Motif::Star, Motif::Loop, Motif::Coil, Motif::SquareLoop] {
            let p = RunParams { kind: RunKind::Motif, motif, width: 4.0, spacing: 4.0, ..Default::default() };
            let pts: Vec<Point> = generate(&line(), &p, None, None).iter().map(|s| s.pos()).collect();
            let (lo, hi) = pts.iter().fold((0.0f64, 0.0f64), |(lo, hi), q| (lo.min(q.y), hi.max(q.y)));
            assert!(-lo > hi + 0.3, "{motif:?}: {lo} {hi}");
        }
    }

    /// Blackwork steps above and below the line in the same repeat; candlewicking stays on the
    /// line but for a knot of short crossing stitches at each repeat's middle.
    #[test]
    fn blackwork_steps_across_the_line_and_candlewicking_knots_on_it() {
        let p = RunParams { kind: RunKind::Motif, motif: Motif::Blackwork, width: 4.0, spacing: 4.0, ..Default::default() };
        let pts: Vec<Point> = generate(&line(), &p, None, None).iter().map(|s| s.pos()).collect();
        let (lo, hi) = pts.iter().fold((0.0f64, 0.0f64), |(lo, hi), q| (lo.min(q.y), hi.max(q.y)));
        assert!(lo < -1.0 && hi > 1.0, "both sides: {lo} {hi}");
        let p = RunParams { kind: RunKind::Motif, motif: Motif::Candlewick, width: 4.0, spacing: 4.0, ..Default::default() };
        let pts: Vec<Point> = generate(&line(), &p, None, None).iter().map(|s| s.pos()).collect();
        let off = pts.iter().map(|q| q.y.abs()).fold(0.0, f64::max);
        assert!(off < 1.0, "the knots stay close to the line: {off}");
        // a knot a repeat: points off the line, several in each 4 mm
        let knots = pts.iter().filter(|q| q.y.abs() > 0.2).count();
        assert!(knots >= 8, "{knots}");
    }

    #[test]
    fn chenille_coils_and_square_loops_turn_back_on_themselves() {
        // each repeat goes back against the line, so the loops cross
        for motif in [Motif::Coil, Motif::SquareLoop] {
            let p = RunParams { kind: RunKind::Motif, motif, width: 3.0, spacing: 2.0, ..Default::default() };
            let pts: Vec<Point> = generate(&line(), &p, None, None).iter().map(|s| s.pos()).collect();
            let mut furthest = f64::NEG_INFINITY;
            let back = pts.iter().any(|q| {
                furthest = furthest.max(q.x);
                q.x < furthest - 0.5
            });
            assert!(back, "{motif:?}");
            assert!(pts.len() > 30, "{motif:?}: {}", pts.len());
        }
    }

    #[test]
    fn closed_and_odd_lines_make_motifs_or_none() {
        // A circle (closed) is sewn once round from its left side, then on round to its right.
        let ring: Vec<Point> =
            (0..=36).map(|i| f64::from(i) * std::f64::consts::TAU / 36.0).map(|a| Point::new(5.0 * a.cos(), 5.0 * a.sin())).collect();
        let m = motif_from_line("Ring", &ring).unwrap();
        let pts = &m.points;
        assert_eq!((pts.first(), pts.last()), (Some(&[0.0, 0.0]), Some(&[1.0, 0.0])));
        assert!(pts.len() > 36 + 18 && pts.len() < 36 + 22, "{}", pts.len());
        let (lo, hi) = pts.iter().fold((0.0f64, 0.0f64), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
        assert!((hi - lo - 1.0).abs() < 1e-9 && (lo + 0.5).abs() < 0.01, "{lo} {hi}");
        assert!((m.aspect - 1.0).abs() < 0.01, "{}", m.aspect);
        // An open line that ends near its start counts as closed.
        assert!(motif_from_line("Almost", &ring[..36]).is_some());
        // Too small, a point, nothing, not finite: no motif.
        for bad in [
            vec![],
            vec![Point::new(1.0, 1.0)],
            vec![Point::new(0.0, 0.0), Point::new(0.05, 0.0)],
            vec![Point::new(f64::NAN, 0.0), Point::new(f64::INFINITY, 1.0)],
        ] {
            assert!(motif_from_line("x", &bad).is_none(), "{bad:?}");
        }
        // A straight line is a flat motif; a very detailed one is simplified to fit.
        let flat = motif_from_line("Flat", &[Point::new(0.0, 0.0), Point::new(10.0, 0.0)]).unwrap();
        assert_eq!(flat.points, vec![[0.0, 0.0], [1.0, 0.0]]);
        let wiggle: Vec<Point> = (0..100_000).map(|i| f64::from(i) * 0.001).map(|x| Point::new(x, (x * 7.0).sin())).collect();
        let m = motif_from_line("Wiggle", &wiggle).unwrap();
        assert!(m.points.len() <= MAX_MOTIF_POINTS && m.points.len() > 50, "{}", m.points.len());
    }

    #[test]
    fn hostile_custom_motifs_sew_safely() {
        let huge = CustomMotif { name: "Big".into(), points: vec![[0.0, 0.0]; 100_000], aspect: f64::NAN };
        let odd = CustomMotif { name: "Odd".into(), points: vec![[0.0, 0.0], [1e300, -1e300], [1.0, 0.0]], aspect: -5.0 };
        let few = CustomMotif { name: "Few".into(), points: vec![[0.5, 0.5]], aspect: 1.0 };
        for m in [huge, odd, few] {
            assert!(m.aspect() > 0.0 && m.aspect().is_finite());
            let p = RunParams { kind: RunKind::Motif, custom_motif: Some(m.clone()), spacing: 0.5, ..Default::default() };
            let long = Contour::from_points(&[Point::new(0.0, 0.0), Point::new(1000.0, 0.0)], false);
            let st = generate(&long, &p, None, None);
            assert!(st.len() <= MAX_MOTIF_STITCHES + 10 && st.iter().all(|s| s.pos().is_finite()), "{}: {}", m.name, st.len());
        }
        // Few points to sew: the built-in motif instead.
        let few = CustomMotif { name: "Few".into(), points: vec![[0.5, 0.5]], aspect: 1.0 };
        assert_eq!(motif_points(Motif::Diamond, Some(&few)), motif_shape(Motif::Diamond));
    }

    /// Triple (T) and single (S) stitches in a run's needle points, in order.
    fn stitch_pattern(pts: &[Point]) -> String {
        let mut out = String::new();
        let mut i = 1;
        while i < pts.len() {
            if i + 2 < pts.len() + 1 && pts.get(i + 1) == pts.get(i - 1) && pts.get(i + 2) == pts.get(i) {
                out.push('T');
                i += 3;
            } else {
                out.push('S');
                i += 1;
            }
        }
        out
    }

    #[test]
    fn sculpture_runs_alternate_groups_of_triple_and_single_stitches() {
        // Three triple stitches 2.5 mm long, then one single 1.5 mm long, along 20 mm.
        let p = RunParams { kind: RunKind::Sculpture, stitch_length: 2.5, spacing: 1.5, ..Default::default() };
        let pts: Vec<Point> = generate(&line(), &p, None, None).iter().map(|s| s.pos()).collect();
        assert_eq!(stitch_pattern(&pts), "TTTSTTTST");
        assert!(pts.last().unwrap().distance(Point::new(20.0, 0.0)) < 1e-9, "ends on the line's end");
        let lens: Vec<f64> = pts.windows(2).map(|w| w[0].distance(w[1])).collect();
        assert!(lens.iter().all(|l| (2.3..=2.5).contains(l) || (1.4..=1.5).contains(l)), "{lens:?}");
        // Other groups; none at all is the default.
        for (groups, want) in [([1, 2], "TSSTSSTSSTS"), ([0, 1], "SSSSSSSSSSSSS"), ([1, 0], "TTTTTTTT"), ([0, 0], "TTTSTTTST")] {
            let p = RunParams { groups, ..p.clone() };
            let pts: Vec<Point> = generate(&line(), &p, None, None).iter().map(|s| s.pos()).collect();
            assert_eq!(stitch_pattern(&pts), want, "{groups:?}");
        }
        // Absurd lengths and counts sew something sensible.
        for (a, b, groups) in [(f64::NAN, f64::INFINITY, [u32::MAX, u32::MAX]), (-1.0, 0.0, [3, 1]), (1e9, 1e-9, [7, 0])] {
            let p = RunParams { stitch_length: a, spacing: b, groups, ..p.clone() };
            let st = generate(&line(), &p, None, None);
            assert!(!st.is_empty() && st.len() < 1000 && st.iter().all(|s| s.pos().is_finite()), "{a} {b} {groups:?}: {}", st.len());
        }
    }

    #[test]
    fn string_stitches_overlap_and_draw_a_curve_inside_a_round_line() {
        // Along a straight line: long stitches 6 mm long starting every 1 mm, back and forth.
        let p = RunParams { kind: RunKind::String, width: 6.0, spacing: 1.0, ..Default::default() };
        let pts: Vec<Point> = generate(&line(), &p, None, None).iter().map(|s| s.pos()).collect();
        assert_eq!(pts.first(), Some(&Point::new(0.0, 0.0)));
        assert!(pts.last().unwrap().distance(Point::new(20.0, 0.0)) < 1e-9, "ends on the line's end");
        let long = pts.windows(2).step_by(2).filter(|w| (w[0].distance(w[1]) - 6.0).abs() < 1e-6).count();
        assert_eq!(long, 15, "a long stitch every mm along the 14 mm before the last chord");
        // Round a circle 10 mm across, chords of a quarter of the way round: every needle point on
        // the circle, and the stitches keep away from its middle (they draw an inner circle).
        let ring: Vec<Point> =
            (0..64).map(|i| f64::from(i) * std::f64::consts::TAU / 64.0).map(|a| Point::new(5.0 * a.cos(), 5.0 * a.sin())).collect();
        let circle = Contour::from_points(&ring, true);
        let p = RunParams { kind: RunKind::String, width: 31.4159 / 4.0, spacing: 0.8, ..Default::default() };
        let pts: Vec<Point> = generate(&circle, &p, None, None).iter().map(|s| s.pos()).collect();
        assert!(pts.len() > 70 && pts.iter().all(|q| (q.length() - 5.0).abs() < 0.05), "{}", pts.len());
        let inner = pts.windows(2).map(|w| polyline::distance_to_segment(Point::ZERO, w[0], w[1])).fold(f64::INFINITY, f64::min);
        assert!((3.3..=3.7).contains(&inner), "the chords stay {inner} mm from the middle");
        // Absurd values sew something bounded.
        for (w, s) in [(f64::NAN, f64::NAN), (1e308, 1e-308), (-1.0, 1e308)] {
            let p = RunParams { kind: RunKind::String, width: w, spacing: s, ..Default::default() };
            let st = generate(&circle, &p, None, None);
            assert!(!st.is_empty() && st.len() < 400_000 && st.iter().all(|s| s.pos().is_finite()), "{w} {s}");
        }
    }

    #[test]
    fn hand_stitches_are_thick_dashes_joined_by_short_single_stitches() {
        let p = RunParams { kind: RunKind::Hand, stitch_length: 3.0, spacing: 1.0, ..Default::default() };
        let pts: Vec<Point> = generate(&line(), &p, None, None).iter().map(|s| s.pos()).collect();
        // first, then (to, back, to) for each thick stitch and one point per gap
        assert_eq!((pts.len() - 1) % 4, 3, "{pts:?}");
        assert_eq!(pts.first(), Some(&Point::new(0.0, 0.0)));
        assert!(pts.last().unwrap().distance(Point::new(20.0, 0.0)) < 1e-9, "ends on the line's end");
        let mut thick = Vec::new();
        let mut gaps = Vec::new();
        let mut i = 0;
        while i + 3 < pts.len() {
            let (a, b) = (pts[i], pts[i + 1]);
            assert!(pts[i + 2].distance(a) < 1e-9 && pts[i + 3].distance(b) < 1e-9, "sewn forward, back and forward");
            thick.push(a.distance(b));
            if let Some(g) = pts.get(i + 4) {
                gaps.push(b.distance(*g));
            }
            i += 4;
        }
        assert_eq!(thick.len(), 5, "20 mm of 3 mm stitches and 1 mm gaps: {thick:?}");
        assert!(thick.iter().all(|l| (2.0..=4.2).contains(l)) && gaps.iter().all(|g| (0.6..=1.4).contains(g)), "{thick:?} {gaps:?}");
        assert!(thick.windows(2).any(|w| (w[0] - w[1]).abs() > 0.05), "the lengths vary: {thick:?}");
        // Short lines are one thick stitch; the same line comes out the same.
        let short = Contour::from_points(&[Point::new(0.0, 0.0), Point::new(1.0, 0.0)], false);
        assert_eq!(generate(&short, &p, None, None).len(), 4);
        assert_eq!(generate(&line(), &p, None, None), generate(&line(), &p, None, None));
        for (len, gap) in [(f64::NAN, f64::NAN), (1e308, -1.0), (0.0, 1e308)] {
            let p = RunParams { kind: RunKind::Hand, stitch_length: len, spacing: gap, ..Default::default() };
            let st = generate(&line(), &p, None, None);
            assert!(st.len() >= 4 && st.iter().all(|s| s.pos().is_finite()), "{len} {gap}");
        }
    }

    #[test]
    fn single_run_lengths() {
        let st = generate(&line(), &RunParams::default(), None, None);
        assert_eq!(st.len(), 9); // 20 / 2.5 = 8 stitches
        assert!(st.iter().all(|s| s.cmd == Cmd::Stitch));
    }

    #[test]
    fn hint_reverses_open_path() {
        let st = generate(&line(), &RunParams::default(), Some(Point::new(25.0, 0.0)), None);
        assert!(st[0].pos().approx_eq(Point::new(20.0, 0.0), 1e-9));
    }

    #[test]
    fn closed_run_starts_near_hint() {
        let sq = Contour::from_points(&[Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0), Point::new(0.0, 10.0)], true);
        let st = generate(&sq, &RunParams::default(), Some(Point::new(5.0, 11.0)), None);
        assert!(st[0].pos().approx_eq(Point::new(5.0, 10.0), 1e-9));
        assert!(st.last().unwrap().pos().approx_eq(Point::new(5.0, 10.0), 1e-9));
    }

    #[test]
    fn backstitch_goes_forward_two_and_back_one() {
        let p = RunParams { kind: RunKind::Back, stitch_length: 2.0, ..Default::default() };
        let st = generate(&line(), &p, None, None);
        let xs: Vec<f64> = st.iter().map(|s| s.x).collect();
        // needle points every 1 mm: 0 → 2 → 1 → 3 → 2 → 4 …, ending at 20
        assert_eq!(&xs[..6], &[0.0, 2.0, 1.0, 3.0, 2.0, 4.0]);
        assert_eq!(*xs.last().unwrap(), 20.0);
        for w in xs.windows(2) {
            assert!((w[1] - w[0]).abs() <= 2.0 + 1e-9);
        }
        // stem stitch: the back points are slanted off the line, all on one side
        let p = RunParams { kind: RunKind::Stem, stitch_length: 2.0, width: 2.0, ..Default::default() };
        let st = generate(&line(), &p, None, None);
        let off: Vec<f64> = st.iter().map(|s| s.y).filter(|y| y.abs() > 1e-9).collect();
        assert!(off.len() > 10 && off.iter().all(|y| (y.abs() - 1.0).abs() < 1e-9 && y.signum() == off[0].signum()), "{off:?}");
        let flipped = generate(&line(), &RunParams { flip: true, ..p }, None, None);
        assert!(flipped.iter().any(|s| s.y.signum() == -off[0].signum() && s.y.abs() > 0.5));
    }

    #[test]
    fn passes_backtrack_and_repeat() {
        let once = generate(&line(), &RunParams::default(), None, None);
        let back = generate(&line(), &RunParams { passes: 2, ..Default::default() }, None, None);
        assert_eq!(back.len(), once.len() * 2 - 1);
        assert!(back.last().unwrap().pos().approx_eq(back[0].pos(), 1e-9), "a backtrack ends at the start");
        // the way back uses the same needle points
        for (a, b) in once.iter().zip(back.iter().rev()) {
            assert!(a.pos().approx_eq(b.pos(), 1e-9));
        }
        let three = generate(&line(), &RunParams { passes: 3, ..Default::default() }, None, None);
        assert_eq!(three.len(), once.len() * 3 - 2);
        assert!(three.last().unwrap().pos().approx_eq(Point::new(20.0, 0.0), 1e-9));
        // too many passes are capped
        let many = generate(&line(), &RunParams { passes: 500, ..Default::default() }, None, None);
        assert_eq!(many.len(), (once.len() - 1) * MAX_PASSES as usize + 1);
        // asked to end near x = 20 with a backtrack: starts there and comes back
        let st = generate(&line(), &RunParams { passes: 2, ..Default::default() }, None, Some(Point::new(21.0, 0.0)));
        assert!(st[0].pos().approx_eq(Point::new(20.0, 0.0), 1e-9));
    }

    #[test]
    fn triple_and_styles() {
        let p = RunParams { kind: RunKind::Triple, ..Default::default() };
        let st = generate(&line(), &p, None, None);
        assert_eq!(st.len(), 1 + 8 * 3);
        for kind in RunKind::ALL {
            for m in Motif::ALL {
                let p = RunParams { kind, motif: m, ..Default::default() };
                let st = generate(&line(), &p, None, None);
                assert!(!st.is_empty());
                for w in st.windows(2) {
                    assert!(w[0].pos().distance(w[1].pos()) < 6.0, "{kind:?} {m:?}");
                }
            }
        }
    }
}
