//! Tatami fill.
//!
//! The area is cut into parallel rows at the fill angle, `row_spacing` apart. Each row is the
//! inside of the area along one scanline (even-odd rule, so holes stay empty), lengthened by the
//! pull compensation at both ends. Rows are sewn back and forth; a row only follows the one
//! before it when the two overlap, so a shape with holes or several lobes splits into sections,
//! each started with a [`StitchKind::Jump`].
//!
//! Penetrations along a row sit on a grid shared by every row, `stitch_length` apart and shifted
//! by `1/staggers` of a stitch each row, which spreads the needle holes into the diagonal texture
//! tatami is known for. Underlay is a sparse fill at right angles, kept inside the edges.

use kurbo::{Point, Vec2};

use crate::geometry::Polyline;
use crate::settings::FillSettings;
use crate::{MAX_STITCHES, Stitch, StitchKind};

/// Rows per fill are capped so a huge shape at a tiny spacing can't run away.
const MAX_ROWS: usize = 200_000;
/// A grid penetration closer than this fraction of a stitch to a row's end is skipped, so rows
/// don't end in tiny stitches.
const MIN_END_FRACTION: f64 = 0.3;
/// Underlay stitches are at least this long, mm.
const UNDERLAY_STITCH: f64 = 3.0;
/// A row only follows the previous one when its start is within this many stitches of the
/// previous row's end; otherwise the section ends and the next starts with a jump.
const MAX_STEP_STITCHES: f64 = 2.0;
/// Running stitch length for moves inside the shape, mm.
const TRAVEL_STITCH: f64 = 2.5;
/// How far outside the outline a move inside the shape may stray, mm.
const EDGE_SLACK: f64 = 0.6;
/// Rows shorter than this are not sewn, mm.
const MIN_ROW: f64 = 0.2;

/// An outline edge in row space, `(u, v)` at each end.
type Edge = ((f64, f64), (f64, f64));

/// One set of rows: the top fill or its underlay.
struct Rows {
    angle: f64,
    spacing: f64,
    length: f64,
    staggers: u32,
    /// mm added to (negative: taken from) each end of a row.
    extend: f64,
    /// mm kept clear of the edges across the rows (the first and last rows start this far in).
    margin: f64,
    /// Fill thin shapes with at least one row.
    always_one: bool,
    /// Gradient fill: the row spacing on the far side, growing evenly from `spacing` across.
    end_spacing: Option<f64>,
    kind: StitchKind,
}

/// Tatami fill of the area inside `rings` (outer rings and holes, even-odd).
pub fn tatami(rings: &[Polyline], s: &FillSettings) -> Vec<Stitch> {
    let mut out = vec![];
    underlay(rings, s, &mut out);
    let top = Rows {
        angle: s.angle,
        spacing: s.row_spacing,
        length: s.stitch_length,
        staggers: s.staggers.max(1),
        extend: s.pull_compensation,
        margin: 0.0,
        always_one: true,
        end_spacing: None,
        kind: StitchKind::Top,
    };
    fill(rings, &top, &mut out);
    out
}

/// Gradient fill: tatami whose rows thin out across the shape, from `row_spacing` on the near
/// side (the side the rows' normal points away from) to `end_spacing` on the far side, so the
/// colour fades. No underlay: it would show through the open rows.
pub fn gradient(rings: &[Polyline], s: &FillSettings) -> Vec<Stitch> {
    let mut out = vec![];
    let top = Rows {
        angle: s.angle,
        spacing: s.row_spacing,
        length: s.stitch_length,
        staggers: s.staggers.max(1),
        extend: s.pull_compensation,
        margin: 0.0,
        always_one: true,
        end_spacing: Some(s.end_spacing),
        kind: StitchKind::Top,
    };
    fill(rings, &top, &mut out);
    out
}

/// The fill underlay (when `s.underlay`): a sparse tatami at right angles to the fill, inside
/// the edges. Other fills (contour, circular) lay the same underlay.
pub(crate) fn underlay(rings: &[Polyline], s: &FillSettings, out: &mut Vec<Stitch>) {
    if s.underlay {
        let ul = Rows {
            angle: s.angle + 90.0,
            spacing: s.underlay_spacing,
            length: UNDERLAY_STITCH.max(s.stitch_length),
            staggers: 1,
            extend: -s.underlay_inset,
            margin: s.underlay_inset,
            always_one: false,
            end_spacing: None,
            kind: StitchKind::Underlay,
        };
        fill(rings, &ul, out);
    }
}

/// Row space: `u` along the rows, `v` across them.
struct Frame {
    d: Vec2,
    n: Vec2,
}

impl Frame {
    /// Rows run at `deg` counter-clockwise from the x axis as seen on screen (y down).
    fn new(deg: f64) -> Self {
        let a = deg.to_radians();
        Self { d: Vec2::new(a.cos(), -a.sin()), n: Vec2::new(a.sin(), a.cos()) }
    }
    fn to_rows(&self, p: Point) -> (f64, f64) {
        (p.to_vec2().dot(self.d), p.to_vec2().dot(self.n))
    }
    fn to_world(&self, u: f64, v: f64) -> Point {
        (self.d * u + self.n * v).to_point()
    }
}

fn fill(rings: &[Polyline], r: &Rows, out: &mut Vec<Stitch>) {
    let spacing = if r.spacing.is_finite() && r.spacing > 0.05 { r.spacing } else { 0.4 };
    let length = if r.length.is_finite() && r.length > 0.2 { r.length } else { 3.0 };
    let f = Frame::new(if r.angle.is_finite() { r.angle } else { 0.0 });

    // Every edge of every ring (closed implicitly), in row space, ordered by its lower v.
    let mut edges: Vec<Edge> = vec![];
    // Every edge, including those along the rows (which scanlines skip).
    let mut outline: Vec<Edge> = vec![];
    for ring in rings {
        let pts: Vec<(f64, f64)> = ring.points.iter().filter(|p| p.x.is_finite() && p.y.is_finite()).map(|p| f.to_rows(*p)).collect();
        if pts.len() < 3 {
            continue;
        }
        for (i, a) in pts.iter().enumerate() {
            let b = pts[(i + 1) % pts.len()];
            outline.push((*a, b));
            if (a.1 - b.1).abs() > 1e-12 {
                edges.push(if a.1 < b.1 { (*a, b) } else { (b, *a) });
            }
        }
    }
    if edges.is_empty() {
        return;
    }
    edges.sort_by(|a, b| a.0.1.total_cmp(&b.0.1));
    let vmin = edges.iter().map(|e| e.0.1).fold(f64::INFINITY, f64::min);
    let vmax = edges.iter().map(|e| e.1.1).fold(f64::NEG_INFINITY, f64::max);
    let height = vmax - vmin;
    if !(height.is_finite() && height > 1e-9) {
        return;
    }

    // Rows centred in the band that keeps `margin` clear of the edges.
    let band = height - 2.0 * r.margin.max(spacing / 2.0);
    let vs: Vec<f64> = match r.end_spacing.filter(|e| e.is_finite() && *e > 0.05) {
        // Gradient: the density (rows per mm) changes evenly with how far across a row is, which
        // reads as an even fade.
        Some(end) => {
            let mut vs = vec![];
            let mut v = vmin + spacing / 2.0;
            while v <= vmax - spacing.min(end) / 2.0 && vs.len() < MAX_ROWS {
                vs.push(v);
                let t = ((v - vmin) / height).clamp(0.0, 1.0);
                v += 1.0 / (1.0 / spacing + (1.0 / end - 1.0 / spacing) * t);
            }
            if vs.is_empty() {
                vs.push((vmin + vmax) / 2.0);
            }
            vs
        }
        None => {
            let count = if band >= 0.0 {
                ((band / spacing).floor() as usize + 1).min(MAX_ROWS)
            } else if r.always_one {
                1
            } else {
                return;
            };
            let first = vmin + (height - (count - 1) as f64 * spacing) / 2.0;
            (0..count).map(|k| first + k as f64 * spacing).collect()
        }
    };
    let count = vs.len();

    // Scanline each row; `active` holds the edges spanning it.
    let mut rows: Vec<(f64, Vec<(f64, f64)>)> = Vec::with_capacity(count);
    let mut next = 0;
    let mut active: Vec<Edge> = vec![];
    for &v in &vs {
        while let Some(e) = edges.get(next)
            && e.0.1 <= v
        {
            active.push(*e);
            next += 1;
        }
        active.retain(|e| e.1.1 > v);
        // Half-open in v, so a vertex on the scanline counts once.
        let mut xs: Vec<f64> = active.iter().filter(|(a, b)| a.1 <= v && v < b.1).map(|(a, b)| a.0 + (v - a.1) * (b.0 - a.0) / (b.1 - a.1)).collect();
        xs.sort_by(f64::total_cmp);
        let segs = xs
            .chunks_exact(2)
            .filter_map(|c| {
                let (a, b) = (c[0], c[1]);
                if b - a < 1e-9 {
                    // The scanline only touches the outline (a zero-area sliver).
                    return None;
                }
                // Negative extension never turns a row inside out.
                let e = r.extend.max(-(b - a) / 2.0 + MIN_ROW / 2.0);
                let (a, b) = (a - e, b + e);
                (b - a >= MIN_ROW || (r.always_one && b > a)).then_some((a, b))
            })
            .collect();
        rows.push((v, segs));
    }

    // Sew sections: start at the first unsewn row segment, then keep stepping to an overlapping
    // segment in the next row, alternating direction.
    // Sew from the end of the area nearer the needle (after underlay, the far side), so the
    // fill doesn't open with a long move. Staggering follows the rows' own order in space.
    let reversed = out.last().is_some_and(|last| {
        let (_, lv) = f.to_rows(last.point());
        let (lo, hi) = (rows.first().map_or(0.0, |r| r.0), rows.last().map_or(0.0, |r| r.0));
        (lv - hi).abs() < (lv - lo).abs()
    });
    if reversed {
        rows.reverse();
    }
    let n_rows = rows.len();
    let stagger_row = |k: usize| if reversed { n_rows.saturating_sub(k + 1) } else { k };
    let mut used: Vec<Vec<bool>> = rows.iter().map(|(_, s)| vec![false; s.len()]).collect();
    let mut start_row = 0;
    let mut first_section = true;
    loop {
        if out.len() >= MAX_STITCHES {
            return;
        }
        while start_row < rows.len() && used.get(start_row).is_some_and(|u| u.iter().all(|x| *x)) {
            start_row += 1;
        }
        let Some(j) = used.get(start_row).and_then(|u| u.iter().position(|x| !*x)) else { return };
        let (mut k, mut seg) = (start_row, j);
        let Some(&(a, b)) = rows.get(k).and_then(|r| r.1.get(seg)) else { return };
        // Start at the end nearer the needle.
        let mut forward = match out.last() {
            Some(last) => {
                let (lu, lv) = f.to_rows(last.point());
                let v = rows.get(k).map_or(0.0, |r| r.0);
                (a - lu).hypot(v - lv) <= (b - lu).hypot(v - lv)
            }
            None => true,
        };
        mark(&mut used, k, seg);
        let v = rows.get(k).map_or(0.0, |r| r.0);
        let (s0, _) = ends(a, b, forward);
        // Underlay is covered by the top fill, and so is anything sewn before the top fill
        // starts: moves there can run inside the shape instead of jumping. A move between
        // sections of the top fill would show, so it jumps.
        let covered = r.kind == StitchKind::Underlay || first_section;
        first_section = false;
        let from = out.last().map(|l| f.to_rows(l.point()));
        match from {
            Some((fu, fv)) if covered && runs_inside(&edges, &outline, (fu, fv), (s0, v)) => {
                let (p, q) = (Point::new(fu, fv), Point::new(s0, v));
                let n = ((p.distance(q) / TRAVEL_STITCH).ceil() as usize).clamp(1, 10_000);
                for m in 1..=n {
                    let t = p.lerp(q, m as f64 / n as f64);
                    out.push(Stitch::new(f.to_world(t.x, t.y), StitchKind::Travel));
                }
            }
            _ => out.push(Stitch::new(f.to_world(s0, v), StitchKind::Jump)),
        }
        sew_row(&f, a, b, v, forward, stagger_row(k), length, r, out);
        loop {
            let Some((cu, cv)) = out.last().map(|s| f.to_rows(s.point())) else { return };
            let Some(&(ca, cb)) = rows.get(k).and_then(|r| r.1.get(seg)) else { return };
            let Some((nv, nsegs)) = rows.get(k + 1) else { break };
            // Next row's unsewn segments overlapping this one and starting near where this one
            // ends (a far start would mean sewing across a gap); take the nearest.
            let nf = !forward;
            let best = nsegs
                .iter()
                .enumerate()
                .filter(|(i, (a, b))| !used.get(k + 1).and_then(|u| u.get(*i)).copied().unwrap_or(true) && *a < cb && *b > ca)
                .filter(|(_, (a, b))| (ends(*a, *b, nf).0 - cu).abs() <= MAX_STEP_STITCHES * length)
                .min_by(|x, y| (ends(x.1.0, x.1.1, nf).0 - cu).abs().total_cmp(&(ends(y.1.0, y.1.1, nf).0 - cu).abs()));
            let Some((i, &(na, nb))) = best else { break };
            let nv = *nv;
            k += 1;
            seg = i;
            forward = nf;
            mark(&mut used, k, seg);
            // Step to the next row's start, split so no stitch is longer than a fill stitch.
            let (s0, _) = ends(na, nb, forward);
            let to = Point::new(s0, nv);
            let from = Point::new(cu, cv);
            let n = ((from.distance(to) / length).ceil() as usize).clamp(1, 10_000);
            for m in 1..=n {
                let p = from.lerp(to, m as f64 / n as f64);
                out.push(Stitch::new(f.to_world(p.x, p.y), r.kind));
            }
            sew_row(&f, na, nb, nv, forward, stagger_row(k), length, r, out);
            if out.len() >= MAX_STITCHES {
                return;
            }
        }
    }
}

/// An area for fills that don't sew straight rows (circular): which points are inside it, and
/// which moves can run inside it.
pub(crate) struct Region {
    edges: Vec<Edge>,
    outline: Vec<Edge>,
}

impl Region {
    pub(crate) fn new(rings: &[Polyline]) -> Self {
        let (mut edges, mut outline) = (vec![], vec![]);
        for ring in rings {
            let pts: Vec<(f64, f64)> = ring.points.iter().filter(|p| p.x.is_finite() && p.y.is_finite()).map(|p| (p.x, p.y)).collect();
            if pts.len() < 3 {
                continue;
            }
            for (i, a) in pts.iter().enumerate() {
                let b = pts[(i + 1) % pts.len()];
                outline.push((*a, b));
                if (a.1 - b.1).abs() > 1e-12 {
                    edges.push(if a.1 < b.1 { (*a, b) } else { (b, *a) });
                }
            }
        }
        Self { edges, outline }
    }

    pub(crate) fn contains(&self, p: Point) -> bool {
        inside(&self.edges, (p.x, p.y))
    }

    /// Whether a straight move stays inside, or within [`EDGE_SLACK`] of the outline.
    pub(crate) fn runs_inside(&self, a: Point, b: Point) -> bool {
        runs_inside(&self.edges, &self.outline, (a.x, a.y), (b.x, b.y))
    }
}

/// Whether a straight move from `a` to `b` (row space) stays inside the area, or within
/// [`EDGE_SLACK`] of its outline (row ends sit on the outline, or just past it with pull
/// compensation).
fn runs_inside(edges: &[Edge], outline: &[Edge], a: (f64, f64), b: (f64, f64)) -> bool {
    let len = (b.0 - a.0).hypot(b.1 - a.1);
    let n = ((len / 0.25).ceil() as usize).clamp(1, 4000);
    (0..=n).all(|i| {
        let t = i as f64 / n as f64;
        let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        inside(edges, p) || near_edge(outline, p, EDGE_SLACK)
    })
}

/// Even-odd point in area (edges are stored lower end first).
fn inside(edges: &[Edge], p: (f64, f64)) -> bool {
    edges.iter().filter(|(a, b)| a.1 <= p.1 && p.1 < b.1 && a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1) > p.0).count() % 2 == 1
}

fn near_edge(edges: &[Edge], p: (f64, f64), d: f64) -> bool {
    edges.iter().any(|(a, b)| {
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let l2 = ex * ex + ey * ey;
        let t = if l2 > 0.0 { (((p.0 - a.0) * ex + (p.1 - a.1) * ey) / l2).clamp(0.0, 1.0) } else { 0.0 };
        (a.0 + ex * t - p.0).hypot(a.1 + ey * t - p.1) <= d
    })
}

fn mark(used: &mut [Vec<bool>], k: usize, seg: usize) {
    if let Some(x) = used.get_mut(k).and_then(|u| u.get_mut(seg)) {
        *x = true;
    }
}

/// (start, end) of a row sewn in the given direction.
fn ends(a: f64, b: f64, forward: bool) -> (f64, f64) {
    if forward { (a, b) } else { (b, a) }
}

/// The penetrations after a row's start, up to and including its end.
#[allow(clippy::too_many_arguments)]
fn sew_row(f: &Frame, a: f64, b: f64, v: f64, forward: bool, row: usize, length: f64, r: &Rows, out: &mut Vec<Stitch>) {
    let phase = (row % r.staggers.max(1) as usize) as f64 / r.staggers.max(1) as f64;
    let margin = length * MIN_END_FRACTION;
    // Grid points strictly inside the row, away from both ends.
    let lo = ((a + margin) / length - phase).ceil();
    let hi = ((b - margin) / length - phase).floor();
    let mut grid: Vec<f64> = vec![];
    if lo.is_finite() && hi.is_finite() && hi >= lo && hi - lo < 1e6 {
        let (lo, hi) = (lo as i64, hi as i64);
        grid = (lo..=hi).map(|m| (m as f64 + phase) * length).collect();
    }
    if !forward {
        grid.reverse();
    }
    let (_, end) = ends(a, b, forward);
    for u in grid.into_iter().chain([end]) {
        out.push(Stitch::new(f.to_world(u, v), r.kind));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Polyline {
        Polyline::new(vec![Point::new(x0, y0), Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)], true)
    }

    fn plain(angle: f64) -> FillSettings {
        FillSettings { angle, underlay: false, pull_compensation: 0.0, ..Default::default() }
    }

    #[test]
    fn square_fill_stays_inside_with_short_stitches() {
        let s = plain(0.0);
        let st = tatami(&[rect(0.0, 0.0, 20.0, 20.0)], &s);
        assert!(st.iter().all(|p| p.x > -1e-9 && p.x < 20.0 + 1e-9 && p.y > 0.0 && p.y < 20.0));
        // 20 mm at 0.4 mm spacing: 50 rows, one section.
        assert_eq!(st.iter().filter(|p| p.kind == StitchKind::Jump).count(), 1);
        let mut rows: Vec<i64> = st.iter().map(|p| (p.y * 1000.0).round() as i64).collect();
        rows.dedup();
        assert_eq!(rows.len(), 50);
        for w in st.windows(2) {
            let d = w[0].point().distance(w[1].point());
            assert!(d <= s.stitch_length * (1.0 + MIN_END_FRACTION) + 1e-9, "{d}");
        }
    }

    #[test]
    fn rows_follow_the_angle() {
        let st = tatami(&[rect(0.0, 0.0, 20.0, 20.0)], &plain(90.0));
        // Vertical rows: the stitches of one row share x.
        let xs: std::collections::BTreeSet<i64> = st.iter().map(|p| (p.x * 1000.0).round() as i64).collect();
        assert_eq!(xs.len(), 50);
    }

    #[test]
    fn holes_stay_empty_and_no_stitch_crosses_them() {
        let st = tatami(&[rect(0.0, 0.0, 30.0, 30.0), rect(10.0, 10.0, 20.0, 20.0)], &plain(0.0));
        let inside = |p: Point, m: f64| p.x > 10.0 + m && p.x < 20.0 - m && p.y > 10.0 + m && p.y < 20.0 - m;
        assert!(st.iter().all(|s| !inside(s.point(), 1e-6)));
        for w in st.windows(2) {
            if w[1].kind != StitchKind::Jump {
                assert!(!inside(w[0].point().midpoint(w[1].point()), 0.5));
            }
        }
        assert!(st.iter().filter(|p| p.kind == StitchKind::Jump).count() >= 2);
    }

    #[test]
    fn rows_stagger() {
        let s = FillSettings { staggers: 4, ..plain(0.0) };
        let st = tatami(&[rect(0.0, 0.0, 30.0, 2.0)], &s);
        // Inner penetrations of the first two rows sit on different grid offsets.
        let row = |y: f64| -> Vec<f64> {
            st.iter().filter(|p| (p.y - y).abs() < 1e-6 && p.x > 1.0 && p.x < 29.0).map(|p| (p.x / s.stitch_length).fract()).collect()
        };
        let ys: Vec<f64> = {
            let mut v: Vec<f64> = st.iter().map(|p| p.y).collect();
            v.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
            v
        };
        let (a, b) = (row(ys[0]), row(ys[1]));
        assert!((a[0] - b[0]).abs() > 0.1);
    }

    #[test]
    fn pull_compensation_lengthens_rows() {
        let s = FillSettings { pull_compensation: 0.3, ..plain(0.0) };
        let st = tatami(&[rect(0.0, 0.0, 20.0, 5.0)], &s);
        let (min, max) = st.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
        assert!((min + 0.3).abs() < 1e-9 && (max - 20.3).abs() < 1e-9);
    }

    #[test]
    fn underlay_first_inside_and_across() {
        let s = FillSettings { angle: 0.0, pull_compensation: 0.0, ..Default::default() };
        let st = tatami(&[rect(0.0, 0.0, 20.0, 20.0)], &s);
        let first_top = st.iter().position(|p| p.kind == StitchKind::Top).unwrap();
        let ul: Vec<_> = st[..first_top].iter().filter(|p| !matches!(p.kind, StitchKind::Jump | StitchKind::Travel)).collect();
        // The move from the underlay to the top fill runs inside the shape: no second jump.
        assert_eq!(st.iter().filter(|p| p.kind == StitchKind::Jump).count(), 1);
        assert!(!ul.is_empty());
        assert!(ul.iter().all(|p| p.kind == StitchKind::Underlay));
        assert!(ul.iter().all(|p| p.x >= 0.8 - 1e-9 && p.x <= 19.2 + 1e-9 && p.y >= 0.8 - 1e-9 && p.y <= 19.2 + 1e-9));
    }

    #[test]
    fn gradient_rows_spread_out_across_the_shape() {
        let s = FillSettings { row_spacing: 0.4, end_spacing: 2.0, ..plain(0.0) };
        let st = gradient(&[rect(0.0, 0.0, 20.0, 20.0)], &s);
        assert!(st.iter().all(|p| p.kind != StitchKind::Underlay));
        let mut ys: Vec<f64> = st.iter().map(|p| p.y).collect();
        ys.sort_by(f64::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        // Rows at 0° step down the page: tight at the top, open at the bottom.
        let gaps: Vec<f64> = ys.windows(2).map(|w| w[1] - w[0]).collect();
        let (first, last) = (gaps[0], gaps[gaps.len() - 1]);
        assert!((first - 0.4).abs() < 0.05 && last > 1.3 && last <= 2.0 + 1e-9, "{gaps:?}");
        assert!(gaps.windows(2).all(|w| w[1] >= w[0] - 1e-9));
        // Fewer rows than a plain fill at the tight spacing, and still inside.
        assert!(gaps.len() < 40 && ys.iter().all(|y| *y > 0.0 && *y < 20.0));
    }

    #[test]
    fn thin_and_degenerate_shapes() {
        assert!(tatami(&[], &FillSettings::default()).is_empty());
        let flat = Polyline::new(vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(20.0, 0.0)], true);
        assert!(tatami(&[flat], &FillSettings::default()).is_empty());
        // Thinner than one row still gets one.
        let thin = tatami(&[rect(0.0, 0.0, 10.0, 0.2)], &plain(0.0));
        assert!(thin.len() > 2);
        let nan = Polyline::new(vec![Point::new(f64::NAN, 0.0), Point::new(1.0, 0.0), Point::new(1.0, 1.0), Point::new(0.0, 1.0)], true);
        assert!(tatami(&[nan], &plain(0.0)).iter().all(|p| p.x.is_finite()));
    }
}
