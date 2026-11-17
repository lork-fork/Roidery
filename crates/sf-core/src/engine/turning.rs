//! Turning fills: tatami rows or satin stitches that follow a [`Strip`], so the
//! stitch direction bends from inclination line to inclination line.

use super::fill::Splits;
use super::out::Out;
use super::run::CORNER_ANGLE;
use super::satin;
use crate::direction::{Strip, strip_pairs};
use crate::geom::{BBox, Point, Polygon, poly, polyline};
use crate::model::{FillKind, FillParams, SatinParams, SatinUnderlay, Texture};

/// Needle positions along a row from `a` to `b` with a staggered grid (tatami).
fn tatami_row(a: Point, b: Point, row: usize, params: &FillParams, out: &mut Vec<Point>) {
    out.clear();
    let len = a.distance(b);
    out.push(a);
    let step = params.stitch_length.max(0.5);
    if len > 1e-9 {
        let phase = (row as f64 * params.offset).rem_euclid(1.0) * step;
        let min_end = (step * 0.35).clamp(0.4, 1.5).min(len * 0.5);
        let mut s = phase;
        while s < len - min_end {
            if s > min_end {
                out.push(a.lerp(b, s / len));
            }
            s += step;
        }
    }
    out.push(b);
}

/// Fills `piece` along `strip`. Underlay: edge run plus a centre walk along the strip,
/// or for satin from a satin object, that satin's own column underlay.
/// A satin object's column (`satin_column`) ends nearest `exit` when given.
pub(crate) fn turning_fill(
    piece: &Polygon,
    strip: &Strip,
    params: &FillParams,
    out: &mut Out,
    ends: (Option<Point>, Option<Point>),
    trim: f64,
    max_stitch: f64,
) {
    let (hint, exit) = ends;
    let satin = params.kind == FillKind::Satin;
    if satin && let Some(sp) = &params.satin_column {
        satin_column(strip, sp, out, (hint, exit), trim, max_stitch);
        return;
    }
    // Satin spacing is between penetrations on the same side → rows at half spacing.
    let step = if satin { params.spacing.max(0.1) * 0.5 } else { params.spacing.max(0.1) };
    let mut pairs = strip_pairs(strip, step);
    if pairs.len() < 2 {
        return;
    }
    // Start at the end nearest to where the needle is.
    let here = out.pos().or(hint);
    if let (Some(h), Some(first), Some(last)) = (here, pairs.first(), pairs.last())
        && h.distance(last.0.midpoint(last.1)) < h.distance(first.0.midpoint(first.1))
    {
        pairs.reverse();
    }

    // Underlay.
    let u = &params.underlay;
    if u.edge_run {
        for p in poly::offset(std::slice::from_ref(piece), -u.inset.max(0.0)) {
            for ring in p.rings() {
                let start = out.pos().or(hint).unwrap_or(ring[0]);
                let r = polyline::closed(&polyline::ring_starting_near(ring, start));
                out.connect(r[0], trim, 2.5);
                out.stitches(polyline::resample(&r, u.stitch_length.max(0.5), CORNER_ANGLE));
            }
        }
    }
    if u.tatami {
        // A walk down the middle of the strip, perpendicular to the top stitches.
        let center: Vec<Point> = pairs.iter().map(|(a, b)| a.midpoint(*b)).collect();
        let center = polyline::simplify(&center, 0.05);
        if let Some(&c0) = center.first() {
            out.connect(c0, trim, 2.5);
            out.stitches(polyline::resample(&center, u.stitch_length.max(0.5), CORNER_ANGLE));
        }
        // ... and back along the strip so the top stitching starts where we began.
        let mut back = center.clone();
        back.reverse();
        let ins: Vec<(Point, Point)> = pairs
            .iter()
            .map(|&(a, b)| {
                let w = a.distance(b);
                let d = u.inset.min(w * 0.5 - 0.05).max(0.0);
                let dir = (b - a).normalized();
                (a + dir * d, b - dir * d)
            })
            .rev()
            .collect();
        // sparse zigzag back over the inset strip
        let zz_step = (u.spacing.max(1.0) * 0.5 / step).round().max(1.0) as usize;
        let zz: Vec<Point> = ins.iter().step_by(zz_step).enumerate().map(|(k, (a, b))| if k % 2 == 0 { *a } else { *b }).collect();
        if let Some(&z0) = zz.first() {
            out.run_to(z0, 2.5);
            // each leg crosses the whole strip: split, so a wide strip makes no long stitches
            out.stitches(polyline::resample(&zz, u.stitch_length.max(0.5), CORNER_ANGLE));
        }
        let _ = back;
    }

    // Top stitching.
    if let Some(&(a0, _)) = pairs.first() {
        out.connect(a0, trim, 2.5);
    }
    if satin {
        let sp = SatinParams {
            spacing: params.spacing,
            pull_comp: params.pull_comp,
            auto_spacing: false,
            split_length: params.split_length,
            short_stitches: true,
            underlay: SatinUnderlay { auto: false, center_run: false, ..SatinUnderlay::default() },
            ..SatinParams::default()
        };
        let pts = satin::zigzag_points(&pairs, &sp, (params.pull_comp, params.pull_comp), true);
        out.stitches(match params.texture {
            Some(t) => emboss(pts, &pairs, t),
            None => pts,
        });
    } else {
        let pc = params.pull_comp;
        let mut row = Vec::new();
        for (k, &(a, b)) in pairs.iter().enumerate() {
            let d = (b - a).normalized();
            let (a, b) = (a - d * pc, b + d * pc);
            tatami_row(a, b, k, params, &mut row);
            if k % 2 == 1 {
                row.reverse();
            }
            if let Some(&r0) = row.first() {
                out.run_to(r0, params.stitch_length.max(1.0));
            }
            out.stitches(row.iter().copied());
        }
    }
}

/// Shortest piece an embossed satin stitch is cut into (mm).
const EMBOSS_EDGE: f64 = 0.8;

/// An embossed turning satin (NEW-28): texture `t` laid along the column, its length along the
/// column's middle and its width across it (mm from the middle), so the pattern bends with the
/// turning; each stitch of `pts` gets a needle point where it crosses the pattern's lines.
fn emboss(pts: Vec<Point>, pairs: &[(Point, Point)], t: Texture) -> Vec<Point> {
    let mids: Vec<Point> = pairs.iter().map(|(a, b)| a.midpoint(*b)).collect();
    let mut along = Vec::with_capacity(mids.len());
    let mut acc = 0.0;
    along.push(0.0);
    for w in mids.windows(2) {
        if let [a, b] = w {
            acc += a.distance(*b);
            along.push(acc);
        }
    }
    let half = pairs.iter().map(|(a, b)| a.distance(*b)).fold(0.0, f64::max) * 0.5 + 2.0;
    if !(acc.is_finite() && half.is_finite()) {
        return pts;
    }
    // rows of the pattern run across the column (x), the column runs down (y)
    let Some(lines) = Splits::of_texture(t, &BBox::new(Point::new(-half, -1.0), Point::new(half, acc + 1.0))) else {
        return pts;
    };
    let mut out = Vec::with_capacity(pts.len() * 2);
    let (mut k, mut cuts) = (0usize, Vec::new());
    for (i, &p) in pts.iter().enumerate() {
        out.push(p);
        let Some(&q) = pts.get(i + 1) else { break };
        // the pair the stitch belongs to: the stitches go along the pairs in order
        let m = p.midpoint(q);
        while let (Some(here), Some(next)) = (mids.get(k), mids.get(k + 1)) {
            if next.distance(m) > here.distance(m) {
                break;
            }
            k += 1;
        }
        let (Some(&(a, b)), Some(&c), Some(&y)) = (pairs.get(k), mids.get(k), along.get(k)) else { continue };
        let d = (b - a).normalized();
        let (vp, vq) = ((p - c).dot(d), (q - c).dot(d));
        let span = (vq - vp).abs();
        if span.is_nan() || span <= 2.0 * EMBOSS_EDGE {
            continue;
        }
        lines.crossings(y, &mut cuts);
        let (lo, hi) = (vp.min(vq), vp.max(vq));
        let mut kept: Vec<f64> = Vec::with_capacity(cuts.len());
        for &v in &cuts {
            if v >= lo + EMBOSS_EDGE && v <= hi - EMBOSS_EDGE && kept.last().is_none_or(|&l| v - l >= EMBOSS_EDGE) {
                kept.push(v);
            }
        }
        if vq < vp {
            kept.reverse();
        }
        out.extend(kept.into_iter().map(|v| p.lerp(q, (v - vp) / (vq - vp))));
    }
    out
}

/// Satin along `strip` with a satin object's column settings: spacing opened up on
/// narrow strips (auto spacing), underlay chosen by width, then the top stitches.
/// It starts at the end nearest the needle and, when `exit` lies nearer that end than the far
/// one, comes back to it: the thread is carried to the far end under the column first, so the
/// next piece (a letter's next stroke) is reached without a trim.
fn satin_column(strip: &Strip, sp: &SatinParams, out: &mut Out, ends: (Option<Point>, Option<Point>), trim: f64, max_stitch: f64) {
    let (hint, exit) = ends;
    let probe = strip_pairs(strip, sp.spacing.max(0.1));
    if probe.len() < 2 {
        return;
    }
    let width = probe.iter().map(|(a, b)| a.distance(*b)).sum::<f64>() / probe.len() as f64;
    let mut pairs = strip_pairs(strip, satin::effective_spacing(sp, width) * 0.5);
    let here = out.pos().or(hint);
    if let (Some(h), Some(first), Some(last)) = (here, pairs.first(), pairs.last())
        && h.distance(last.0.midpoint(last.1)) < h.distance(first.0.midpoint(first.1))
    {
        pairs.reverse();
    }
    satin::stitch_pairs(&pairs, sp, width, max_stitch, trim, exit, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::direction::{Guide, strip_from_guides};
    use crate::pattern::Cmd;

    fn band() -> (Polygon, Vec<Guide>) {
        let mut ring = Vec::new();
        for i in 0..=24 {
            let a = std::f64::consts::PI * i as f64 / 24.0;
            ring.push(Point::new(20.0 * a.cos(), -20.0 * a.sin()));
        }
        for i in (0..=24).rev() {
            let a = std::f64::consts::PI * i as f64 / 24.0;
            ring.push(Point::new(14.0 * a.cos(), -14.0 * a.sin()));
        }
        let g = |deg: f64| {
            let a: f64 = deg.to_radians();
            Guide::new(Point::new(12.0 * a.cos(), -12.0 * a.sin()), Point::new(22.0 * a.cos(), -22.0 * a.sin()))
        };
        (Polygon::from_ring(ring), vec![g(30.0), g(90.0), g(150.0)])
    }

    #[test]
    fn satin_from_a_satin_object_uses_its_column_settings() {
        let (poly, guides) = band();
        let strip = strip_from_guides(&poly.outer, &guides).expect("strip");
        let run = |sp: SatinParams| {
            let params = FillParams { kind: FillKind::Satin, satin_column: Some(sp), ..FillParams::satin() };
            let mut out = Out::new();
            turning_fill(&poly, &strip, &params, &mut out, (None, None), 2.0, 12.0);
            out.finish()
        };
        let bare = run(SatinParams {
            pull_comp: 0.0,
            underlay: SatinUnderlay { auto: false, center_run: false, ..Default::default() },
            ..Default::default()
        });
        // a 6 mm band: automatic underlay sews edge runs and a zigzag first
        let auto = run(SatinParams { pull_comp: 0.0, ..Default::default() });
        assert!(auto.len() > bare.len() + 50, "{} vs {}", auto.len(), bare.len());
        for st in [&bare, &auto] {
            for s in st.iter() {
                let r = s.pos().length();
                assert!(r > 13.9 && r < 20.1, "{s:?} r={r}");
            }
        }
        // twin-sided pull compensation widens one side only (side 2 = the inner rail here or the outer)
        let twin = run(SatinParams {
            pull_comp: 0.0,
            pull_comp_2: Some(0.5),
            underlay: SatinUnderlay { auto: false, center_run: false, ..Default::default() },
            ..Default::default()
        });
        let radii: Vec<f64> = twin.iter().map(|s| s.pos().length()).collect();
        let (min, max) = radii.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), r| (lo.min(*r), hi.max(*r)));
        let inner_moved = min < 13.8;
        let outer_moved = max > 20.2;
        assert!(inner_moved != outer_moved, "exactly one side widened: {min} {max}");
    }

    /// NEW-28: a texture on a turning satin is pressed into it: the stitches are cut where they
    /// cross the pattern, which is laid along the column, and they stay in the band.
    #[test]
    fn a_texture_embosses_a_turning_satin_along_the_column() {
        let (poly, guides) = band();
        let strip = strip_from_guides(&poly.outer, &guides).expect("strip");
        let run = |texture: Option<Texture>| {
            let params = FillParams { kind: FillKind::Satin, pull_comp: 0.0, texture, ..FillParams::default() };
            let mut out = Out::new();
            turning_fill(&poly, &strip, &params, &mut out, (None, None), 2.0, 12.0);
            out.finish()
        };
        let plain = run(None);
        let t = Texture { pattern: crate::model::TexturePattern::Diamonds, size: 4.0, angle: 0.0 };
        let embossed = run(Some(t));
        assert!(embossed.len() > plain.len() + 100, "{} vs {}", embossed.len(), plain.len());
        for s in &embossed {
            let r = s.pos().length();
            assert!(r > 13.9 && r < 20.1, "{s:?} r={r}");
        }
        // no piece shorter than the shortest cut, apart from the ends of rows the satin makes short
        let pieces: Vec<f64> = embossed.windows(2).map(|w| w[0].pos().distance(w[1].pos())).collect();
        let tiny = pieces.iter().filter(|&&d| d > 1e-6 && d < EMBOSS_EDGE * 0.9).count();
        assert!(tiny * 20 < pieces.len(), "{tiny} of {}", pieces.len());
        // the cuts follow the column: on both halves of the arch a cut sits at each width
        let mid = |r: f64| (r - 17.0).abs() < 1.6;
        let left = embossed.iter().filter(|s| s.pos().x < -5.0 && mid(s.pos().length())).count();
        let right = embossed.iter().filter(|s| s.pos().x > 5.0 && mid(s.pos().length())).count();
        assert!(left > 50 && right > 50, "{left} {right}");
    }

    /// A wide turning tatami's sparse zigzag underlay crosses the strip from edge to edge: on a
    /// 12 mm band each leg was one 12 mm stitch, which snags. Its legs are split like the rest of
    /// the underlay.
    #[test]
    fn a_wide_turning_fills_underlay_has_no_long_stitches() {
        let mut ring = Vec::new();
        for i in 0..=24 {
            let a = std::f64::consts::PI * i as f64 / 24.0;
            ring.push(Point::new(26.0 * a.cos(), -26.0 * a.sin()));
        }
        for i in (0..=24).rev() {
            let a = std::f64::consts::PI * i as f64 / 24.0;
            ring.push(Point::new(14.0 * a.cos(), -14.0 * a.sin()));
        }
        let g = |deg: f64| {
            let a: f64 = deg.to_radians();
            Guide::new(Point::new(12.0 * a.cos(), -12.0 * a.sin()), Point::new(28.0 * a.cos(), -28.0 * a.sin()))
        };
        let poly = Polygon::from_ring(ring);
        let strip = strip_from_guides(&poly.outer, &[g(30.0), g(90.0), g(150.0)]).expect("strip");
        let params = FillParams { kind: FillKind::Tatami, pull_comp: 0.0, ..FillParams::default() };
        assert!(params.underlay.tatami, "the default underlay");
        let mut out = Out::new();
        turning_fill(&poly, &strip, &params, &mut out, (None, None), 2.0, 12.0);
        let st = out.finish();
        let longest =
            st.windows(2).filter(|w| w[0].cmd == Cmd::Stitch && w[1].cmd == Cmd::Stitch).map(|w| w[0].pos().distance(w[1].pos())).fold(0.0, f64::max);
        // under the 7 mm Check Design warns of on clothing
        assert!(longest < 7.0, "a {longest:.1} mm stitch");
    }

    #[test]
    fn turning_tatami_and_satin_stay_in_band() {
        let (poly, guides) = band();
        let strip = strip_from_guides(&poly.outer, &guides).expect("strip");
        for kind in [FillKind::Tatami, FillKind::Satin] {
            let params = FillParams { kind, pull_comp: 0.0, ..FillParams::default() };
            let mut out = Out::new();
            turning_fill(&poly, &strip, &params, &mut out, (None, None), 2.0, 12.0);
            let st = out.finish();
            assert!(st.len() > 200, "{kind:?}: {}", st.len());
            for s in &st {
                let r = s.pos().length();
                assert!(r > 13.9 && r < 20.1, "{kind:?}: {s:?} r={r}");
            }
        }
    }

    /// A letter's stroke ends back where it started when the next stroke is there: the thread
    /// goes out under the column first, then the satin comes back over it.
    #[test]
    fn a_satin_column_comes_back_to_an_exit_by_its_start() {
        let (poly, guides) = band();
        let strip = strip_from_guides(&poly.outer, &guides).expect("strip");
        let bare = SatinUnderlay { auto: false, center_run: false, ..Default::default() };
        let params = FillParams {
            kind: FillKind::Satin,
            satin_column: Some(SatinParams { pull_comp: 0.0, underlay: bare, ..Default::default() }),
            ..FillParams::satin()
        };
        let (start, far) = (Point::new(17.0, 0.0), Point::new(-17.0, 0.0));
        let end = |exit: Option<Point>| {
            let mut out = Out::new();
            turning_fill(&poly, &strip, &params, &mut out, (Some(start), exit), 2.0, 12.0);
            let st = out.finish();
            st.iter().rev().find(|s| s.cmd == crate::pattern::Cmd::Stitch).map(|s| s.pos()).expect("stitches")
        };
        let plain = end(None);
        assert!(plain.distance(far) < plain.distance(start), "no exit: it ends at the far end, {plain:?}");
        let back = end(Some(start));
        assert!(back.distance(start) < 4.0, "{back:?}");
        assert!(end(Some(far)).distance(far) < 4.0, "an exit at the far end changes nothing");
    }
}
