//! Appliqué: placement line → stop → tack-down → stop → cover stitch; combined pieces sew every
//! placement line, one stop, every tack-down, one stop, then every cover.

use super::out::Out;
use super::run::{CORNER_ANGLE, blanket};
use super::satin;
use crate::geom::{FLATTEN_TOLERANCE, Point, Shape, poly, polyline};
use crate::model::{AppliqueParams, CoverKind, TackdownKind};
use crate::pattern::Stitch;

fn rings_from(shape_polys: &[crate::geom::Polygon], here: Option<Point>) -> Vec<Vec<Point>> {
    let mut rings: Vec<Vec<Point>> = shape_polys.iter().flat_map(|p| p.rings().cloned().collect::<Vec<_>>()).collect();
    // Order rings nearest-first from `here`, each starting at its closest point.
    let mut ordered = Vec::new();
    let mut cur = here;
    while !rings.is_empty() {
        let i = match cur {
            Some(h) => (0..rings.len())
                .min_by(|&a, &b| polyline::distance_to_ring(&rings[a], h).total_cmp(&polyline::distance_to_ring(&rings[b], h)))
                .unwrap_or(0),
            None => 0,
        };
        let r = rings.remove(i);
        let r = match cur {
            Some(h) => polyline::ring_starting_near(&r, h),
            None => r,
        };
        cur = r.first().copied();
        ordered.push(polyline::closed(&r));
    }
    ordered
}

/// The parts of closed ring `r` outside `covered`, as open lines (the whole ring, closed, when
/// none of it is covered).
fn uncovered(r: &[Point], covered: &[crate::geom::Polygon]) -> Vec<Vec<Point>> {
    if covered.is_empty() || r.len() < 3 {
        return vec![r.to_vec()];
    }
    let pts = polyline::resample(r, 0.5, CORNER_ANGLE);
    let inside: Vec<bool> = pts.iter().map(|p| covered.iter().any(|c| c.contains(*p))).collect();
    if !inside.iter().any(|i| *i) {
        return vec![r.to_vec()];
    }
    // start at a covered point, so every open stretch is whole
    let start = inside.iter().position(|i| *i).unwrap_or(0);
    let n = pts.len();
    let mut out: Vec<Vec<Point>> = Vec::new();
    let mut run: Vec<Point> = Vec::new();
    for k in 0..=n {
        let i = (start + k) % n.max(1);
        let (Some(p), Some(cov)) = (pts.get(i), inside.get(i)) else { continue };
        if *cov {
            if run.len() >= 2 {
                out.push(std::mem::take(&mut run));
            }
            run.clear();
        } else {
            run.push(*p);
        }
    }
    if run.len() >= 2 {
        out.push(run);
    }
    out
}

pub(crate) fn generate(pieces: &[&Shape], p: &AppliqueParams, covered: &[Vec<Point>], hint: Option<Point>, trim: f64) -> Vec<Stitch> {
    let polys: Vec<crate::geom::Polygon> = pieces.iter().map(|s| s.polygon(FLATTEN_TOLERANCE)).collect();
    let base = poly::clean(&polys);
    if base.is_empty() {
        return Vec::new();
    }
    let mut out = Out::new();

    // 1. Placement line, then stop for the fabric to be laid down.
    let rings = rings_from(&base, hint);
    if p.placement {
        for r in &rings {
            out.connect(r[0], trim, 2.5);
            out.stitches(polyline::resample(r, 2.5, CORNER_ANGLE));
        }
        out.stop();
    }

    // 2. Tack-down slightly inside the outline.
    if p.tackdown != TackdownKind::None {
        let inset = poly::offset(&base, -p.tackdown_inset.max(0.0));
        let tack_polys = if inset.is_empty() { base.clone() } else { inset };
        for r in rings_from(&tack_polys, out.pos()) {
            out.connect(r[0], trim, 2.5);
            match p.tackdown {
                TackdownKind::Run => out.stitches(polyline::resample(&r, 2.0, CORNER_ANGLE)),
                TackdownKind::Zigzag => out.stitches(super::run::zigzag(&r, p.tackdown_width, 1.6)),
                TackdownKind::Blanket => out.stitches(blanket(&r, 2.5, p.tackdown_width, 2.0, false)),
                TackdownKind::None => {}
            }
        }
        if p.trim_stop {
            out.stop();
        }
    }

    // 3. Cover stitch centred on the outline (with partial cover, only where nothing sewn later
    //    lies over the edge).
    let covered: Vec<crate::geom::Polygon> =
        if p.partial { covered.iter().filter(|r| r.len() >= 3).map(|r| crate::geom::Polygon::from_ring(r.clone())).collect() } else { Vec::new() };
    if p.cover != CoverKind::None {
        for ring in rings_from(&base, out.pos()) {
            for (k, r) in uncovered(&ring, &covered).into_iter().enumerate() {
                // the whole ring when nothing covers it, else open stretches of it
                let closed = k == 0 && covered.is_empty();
                let Some(&r0) = r.first() else { continue };
                out.connect(r0, trim, 2.5);
                let pts = match p.cover {
                    CoverKind::Satin => satin::border(&r, closed, p.cover_width, p.cover_spacing, p.pull_comp),
                    CoverKind::Blanket => blanket(&r, 2.5, p.cover_width, 2.2, false),
                    CoverKind::Zigzag => super::run::zigzag(&r, p.cover_width, (p.cover_spacing * 2.0).max(0.8)),
                    CoverKind::Triple => {
                        let line = polyline::resample(&r, 2.5, CORNER_ANGLE);
                        let back: Vec<Point> = line.iter().rev().skip(1).copied().collect();
                        let again: Vec<Point> = line.iter().skip(1).copied().collect();
                        line.iter().copied().chain(back).chain(again).collect()
                    }
                    CoverKind::None => Vec::new(),
                };
                if let Some(&p0) = pts.first() {
                    out.run_to(p0, 2.5);
                }
                out.stitches(pts);
            }
        }
    }
    out.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::Cmd;

    #[test]
    fn covers_of_every_kind_and_partial_cover() {
        let shape = Shape::ellipse(Point::new(0.0, 0.0), 20.0, 15.0);
        let count =
            |p: &AppliqueParams, covered: &[Vec<Point>]| generate(&[&shape], p, covered, None, 2.0).iter().filter(|s| s.cmd == Cmd::Stitch).count();
        let base = AppliqueParams { placement: false, tackdown: TackdownKind::None, ..AppliqueParams::default() };
        for cover in [CoverKind::Satin, CoverKind::Blanket, CoverKind::Zigzag, CoverKind::Triple] {
            assert!(count(&AppliqueParams { cover, ..base.clone() }, &[]) > 20, "{cover:?}");
        }
        // the right half covered by something sewn later: about half the cover, only with partial cover
        let half = vec![vec![Point::new(0.0, -30.0), Point::new(40.0, -30.0), Point::new(40.0, 30.0), Point::new(0.0, 30.0)]];
        let whole = count(&base, &[]);
        assert_eq!(count(&base, &half), whole);
        let part = count(&AppliqueParams { partial: true, ..base.clone() }, &half);
        assert!(part * 10 > whole * 3 && part * 10 < whole * 7, "{part} of {whole}");
        let st = generate(&[&shape], &AppliqueParams { partial: true, ..base.clone() }, &half, None, 2.0);
        assert!(st.iter().filter(|s| s.cmd == Cmd::Stitch).all(|s| s.x < 1.6), "the cover stays on the left");
    }

    #[test]
    fn applique_sequence_has_two_stops() {
        let shape = Shape::ellipse(Point::new(0.0, 0.0), 20.0, 15.0);
        let st = generate(&[&shape], &AppliqueParams::default(), &[], None, 2.0);
        assert_eq!(st.iter().filter(|s| s.cmd == Cmd::Stop).count(), 2);
        assert!(st.iter().filter(|s| s.cmd == Cmd::Stitch).count() > 300);
    }

    #[test]
    fn combined_pieces_share_their_stops() {
        let a = Shape::ellipse(Point::new(0.0, 0.0), 10.0, 8.0);
        let b = Shape::ellipse(Point::new(40.0, 0.0), 10.0, 8.0);
        let st = generate(&[&a, &b], &AppliqueParams::default(), &[], None, 2.0);
        assert_eq!(st.iter().filter(|s| s.cmd == Cmd::Stop).count(), 2, "one stop after the placement lines, one after the tack-downs");
        // both placement lines come before the first stop
        let stop = st.iter().position(|s| s.cmd == Cmd::Stop).unwrap();
        let before = &st[..stop];
        assert!(before.iter().any(|s| s.cmd == Cmd::Stitch && s.x < 15.0) && before.iter().any(|s| s.cmd == Cmd::Stitch && s.x > 25.0));
    }
}
