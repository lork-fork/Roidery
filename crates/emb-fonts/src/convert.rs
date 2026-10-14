//! How an Ink/Stitch element sews in Roidery: satin columns become Satin Blocks (two rails whose
//! anchors pair up as rungs), fills tatami, strokes running stitches, with the element's own
//! parameters (density, underlay, angle, stitch length) carried over.
//!
//! An Ink/Stitch satin column is a path whose subpaths are two rails and any number of rungs
//! crossing them; without rungs, rails with the same number of nodes pair node by node. A Satin
//! Blocks object pairs anchor `i` of each rail, so the rails are split where each rung crosses
//! them, and between rungs each rail gets an anchor wherever the other has a node (at the same
//! fraction of the section's length), which pairs the rails in proportion as Ink/Stitch does.

use kurbo::{BezPath, CubicBez, ParamCurve, ParamCurveArclen, PathEl, Point, Shape};
use roidery_stitch::{StitchSettings, StitchType, Underlay};

use crate::svg::Element;

/// A piece of a glyph: one Roidery object, in font units.
#[derive(Clone, Debug)]
pub struct Piece {
    pub path: BezPath,
    /// The fill part's settings, `None` when the fill doesn't sew.
    pub fill: Option<StitchSettings>,
    /// The stroke part's settings (Satin Blocks and manual stitches are the whole object).
    pub stroke: Option<StitchSettings>,
    pub fill_rgb: Option<[u8; 3]>,
    pub stroke_rgb: Option<[u8; 3]>,
    /// The width a satin along a path sews, in font units (the element's stroke width).
    pub satin_width: Option<f64>,
}

/// What an element's stitches need that Roidery doesn't have yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    CrossStitch,
    Tartan,
}

/// Mm per Ink/Stitch pixel.
pub const MM_PER_PX: f64 = 25.4 / 96.0;

const ACCURACY: f64 = 1e-3;

/// The pieces `e` sews as, or what it needs that Roidery lacks.
pub fn convert(e: &Element) -> Result<Vec<Piece>, Unsupported> {
    let fill_method = e.param("fill_method").unwrap_or("auto_fill");
    match fill_method {
        "cross_stitch" => return Err(Unsupported::CrossStitch),
        "tartan_fill" => return Err(Unsupported::Tartan),
        _ => {}
    }
    if e.flag("satin_column") == Some(true) {
        return Ok(satin(e).into_iter().collect());
    }
    let stroke_method = e.param("stroke_method").unwrap_or("running_stitch");
    let fills = e.fill.is_some() && e.flag("fill") != Some(false) && has_area(&e.path);
    let fill = fills.then(|| fill_settings(e));
    let stroke = e.stroke.is_some().then(|| match stroke_method {
        "manual_stitch" => StitchSettings::of(StitchType::Manual),
        "zigzag_stitch" => {
            let mut s = StitchSettings::of(StitchType::Satin);
            s.satin.width = e.number("zigzag_width_mm").unwrap_or(3.0);
            s.satin.density = e.number("zigzag_spacing_mm").filter(|v| *v > 0.0).unwrap_or(0.4);
            s.satin.pull_compensation = 0.0;
            s.satin.underlays.clear();
            s
        }
        // Ripple stitches sew as a running stitch along the shape until Roidery has them.
        _ => run_settings(e),
    });
    // A manual stitch is the whole object: its fill doesn't sew.
    let fill = if stroke.as_ref().is_some_and(|s| s.kind == StitchType::Manual) { None } else { fill };
    if fill.is_none() && stroke.is_none() {
        return Ok(vec![]);
    }
    let satin_width = stroke.as_ref().filter(|s| s.kind == StitchType::Satin).map(|s| s.satin.width / MM_PER_PX);
    Ok(vec![Piece { path: e.path.clone(), fill, stroke, fill_rgb: e.fill, stroke_rgb: e.stroke, satin_width }])
}

fn has_area(p: &BezPath) -> bool {
    p.area().abs() > 1e-6
}

fn run_settings(e: &Element) -> StitchSettings {
    let repeats = e.number("repeats").unwrap_or(1.0);
    let bean = e.param("bean_stitch_repeats").and_then(|v| v.split([' ', ',']).find_map(|s| s.parse::<f64>().ok())).unwrap_or(0.0);
    let mut s = StitchSettings::of(if repeats >= 3.0 || bean >= 1.0 { StitchType::Triple } else { StitchType::Run });
    s.run.length = e.number("running_stitch_length_mm").unwrap_or(2.5);
    s.run.tolerance = e.number("running_stitch_tolerance_mm").unwrap_or(0.1);
    s.sanitized()
}

fn fill_settings(e: &Element) -> StitchSettings {
    let mut s = StitchSettings::of(StitchType::Tatami);
    let f = &mut s.fill;
    f.angle = e.number("angle").unwrap_or(0.0);
    f.row_spacing = e.number("row_spacing_mm").unwrap_or(0.25);
    f.stitch_length = e.number("max_stitch_length_mm").unwrap_or(4.0);
    f.staggers = e.number("staggers").map_or(4, |v| v.round().clamp(1.0, 16.0) as u32);
    f.pull_compensation = e.number("pull_compensation_mm").unwrap_or(0.0);
    f.underlay = e.flag("fill_underlay").unwrap_or(true);
    f.underlay_spacing = e.number("fill_underlay_row_spacing_mm").unwrap_or(f.row_spacing * 3.0);
    f.underlay_inset = e.number("fill_underlay_inset_mm").unwrap_or(0.0).max(0.0);
    s.sanitized()
}

fn satin_settings(e: &Element, kind: StitchType) -> StitchSettings {
    let mut s = StitchSettings::of(kind);
    s.satin.density = e.number("zigzag_spacing_mm").unwrap_or(0.4);
    s.satin.pull_compensation = e.number("pull_compensation_mm").unwrap_or(0.0);
    s.satin.underlays.clear();
    if e.flag("center_walk_underlay") == Some(true) {
        s.satin.underlays.push(Underlay::CenterWalk { length: e.number("center_walk_underlay_stitch_length_mm").unwrap_or(3.0) });
    }
    if e.flag("contour_underlay") == Some(true) {
        s.satin.underlays.push(Underlay::Contour {
            length: e.number("contour_underlay_stitch_length_mm").unwrap_or(3.0),
            inset: e.number("contour_underlay_inset_mm").unwrap_or(0.4),
        });
    }
    if e.flag("zigzag_underlay") == Some(true) {
        s.satin.underlays.push(Underlay::ZigZag {
            spacing: e.number("zigzag_underlay_spacing_mm").unwrap_or(3.0),
            inset: e.number("zigzag_underlay_inset_mm").unwrap_or(0.4),
        });
    }
    s.sanitized()
}

/// A satin column as Satin Blocks (two subpaths), or as a satin along the path when it has one
/// subpath (Ink/Stitch sews a simple stroke at its stroke width).
fn satin(e: &Element) -> Option<Piece> {
    let subs = rails::subpaths(&e.path);
    let color = e.stroke.or(e.fill);
    if subs.len() == 1 {
        let mut s = satin_settings(e, StitchType::Satin);
        let width = e.stroke_width.max(0.3 / MM_PER_PX);
        s.satin.width = width * MM_PER_PX;
        return Some(Piece { path: e.path.clone(), fill: None, stroke: Some(s), fill_rgb: None, stroke_rgb: color, satin_width: Some(width) });
    }
    let reverse = e.param("reverse_rails").unwrap_or("automatic");
    let (mut left, mut right) = rails::blocks(&subs, reverse)?;
    if e.flag("swap_satin_rails") == Some(true) {
        std::mem::swap(&mut left, &mut right);
    }
    let mut path = left;
    path.extend(right);
    Some(Piece { path, fill: None, stroke: Some(satin_settings(e, StitchType::SatinBlocks)), fill_rgb: None, stroke_rgb: color, satin_width: None })
}

pub mod rails {
    //! Turning an Ink/Stitch satin column's rails and rungs into two rails with paired anchors.

    use super::*;

    /// The subpaths of `path` that have at least one segment, as cubic segments.
    pub fn subpaths(path: &BezPath) -> Vec<Vec<CubicBez>> {
        let mut out: Vec<Vec<CubicBez>> = vec![];
        let (mut start, mut at) = (Point::ZERO, Point::ZERO);
        for el in path.elements() {
            match *el {
                PathEl::MoveTo(p) => {
                    out.push(vec![]);
                    start = p;
                    at = p;
                }
                PathEl::LineTo(p) => {
                    push(&mut out, line(at, p));
                    at = p;
                }
                PathEl::QuadTo(c, p) => {
                    push(&mut out, kurbo::QuadBez::new(at, c, p).raise());
                    at = p;
                }
                PathEl::CurveTo(c1, c2, p) => {
                    push(&mut out, CubicBez::new(at, c1, c2, p));
                    at = p;
                }
                PathEl::ClosePath => {
                    if at.distance(start) > 1e-9 {
                        push(&mut out, line(at, start));
                    }
                    at = start;
                }
            }
        }
        out.retain(|s| !s.is_empty());
        out
    }

    fn push(out: &mut [Vec<CubicBez>], c: CubicBez) {
        if let Some(last) = out.last_mut() {
            last.push(c);
        }
    }

    fn line(a: Point, b: Point) -> CubicBez {
        CubicBez::new(a, a.lerp(b, 1.0 / 3.0), a.lerp(b, 2.0 / 3.0), b)
    }

    /// A rail with its segments' lengths, for going between lengths and points.
    struct Rail {
        segs: Vec<CubicBez>,
        /// Length at the start of each segment, then the total.
        starts: Vec<f64>,
    }

    impl Rail {
        fn new(segs: Vec<CubicBez>) -> Rail {
            let mut starts = vec![0.0];
            let mut acc = 0.0;
            for s in &segs {
                acc += s.arclen(ACCURACY);
                starts.push(acc);
            }
            Rail { segs, starts }
        }

        fn len(&self) -> f64 {
            self.starts.last().copied().unwrap_or(0.0)
        }

        /// The lengths at which the rail's own nodes are.
        fn nodes(&self) -> &[f64] {
            &self.starts
        }

        fn closed(&self) -> bool {
            match (self.segs.first(), self.segs.last()) {
                (Some(a), Some(b)) => a.p0.distance(b.p3) < 1e-6,
                _ => false,
            }
        }

        /// (segment, t) at length `s`.
        fn at(&self, s: f64) -> (usize, f64) {
            let n = self.segs.len();
            let s = s.clamp(0.0, self.len());
            let i = self.starts.partition_point(|x| *x <= s).saturating_sub(1).min(n.saturating_sub(1));
            let (Some(seg), Some(s0), Some(s1)) = (self.segs.get(i), self.starts.get(i), self.starts.get(i + 1)) else { return (0, 0.0) };
            let l = s1 - s0;
            if l <= 1e-12 {
                return (i, 0.0);
            }
            (i, seg.inv_arclen((s - s0).clamp(0.0, l), ACCURACY).clamp(0.0, 1.0))
        }

        fn point(&self, s: f64) -> Point {
            let (i, t) = self.at(s);
            self.segs.get(i).map_or(Point::ZERO, |c| c.eval(t))
        }

        /// Flattened, as (point, length along the rail).
        fn samples(&self) -> Vec<(Point, f64)> {
            let mut out = vec![];
            for (i, seg) in self.segs.iter().enumerate() {
                let s0 = self.starts.get(i).copied().unwrap_or(0.0);
                let l = self.starts.get(i + 1).copied().unwrap_or(s0) - s0;
                let n = 16;
                for k in 0..=n {
                    if k == 0 && i > 0 {
                        continue;
                    }
                    let t = k as f64 / n as f64;
                    out.push((seg.eval(t), s0 + l * t));
                }
            }
            out
        }

        /// `t` on segment `i` at length `s` (clamped to the segment).
        fn t_on(&self, i: usize, s: f64) -> f64 {
            let (Some(seg), Some(s0), Some(s1)) = (self.segs.get(i), self.starts.get(i), self.starts.get(i + 1)) else { return 0.0 };
            let l = s1 - s0;
            if l <= 1e-12 {
                return 0.0;
            }
            let d = s - s0;
            if d <= 1e-9 * l.max(1.0) {
                return 0.0;
            }
            if d >= l - 1e-9 * l.max(1.0) {
                return 1.0;
            }
            seg.inv_arclen(d, ACCURACY).clamp(0.0, 1.0)
        }

        /// The path through anchors at the lengths `at` (sorted, from 0 to the end), one curve
        /// per span: each span lies within one segment (the segment its middle is in).
        fn path(&self, at: &[f64]) -> BezPath {
            let mut bp = BezPath::new();
            let Some(first) = at.first() else { return bp };
            bp.move_to(self.point(*first));
            for w in at.windows(2) {
                let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
                let (seg, _) = self.at((a + b) / 2.0);
                if let Some(c) = self.segs.get(seg) {
                    let c = c.subsegment(self.t_on(seg, *a)..self.t_on(seg, *b));
                    bp.curve_to(c.p1, c.p2, c.p3);
                }
            }
            bp
        }

        fn reversed(&self) -> Rail {
            Rail::new(self.segs.iter().rev().map(|c| CubicBez::new(c.p3, c.p2, c.p1, c.p0)).collect())
        }

        /// A closed rail started at length `s` instead.
        fn rotated(&self, s: f64) -> Rail {
            let (i, t) = self.at(s);
            let Some(seg) = self.segs.get(i) else { return Rail::new(self.segs.clone()) };
            let mut segs = vec![];
            if t < 1.0 - 1e-9 {
                segs.push(seg.subsegment(t..1.0));
            }
            segs.extend(self.segs.iter().skip(i + 1).copied());
            segs.extend(self.segs.iter().take(i).copied());
            if t > 1e-9 {
                segs.push(seg.subsegment(0.0..t));
            }
            Rail::new(segs)
        }

        /// The length along the rail nearest `p`.
        fn nearest(&self, p: Point) -> f64 {
            self.samples().into_iter().min_by(|a, b| a.0.distance(p).total_cmp(&b.0.distance(p))).map_or(0.0, |s| s.1)
        }

        /// Signed area enclosed (for a closed rail's winding).
        fn area(&self) -> f64 {
            let pts = self.samples();
            pts.windows(2).filter_map(|w| Some((w.first()?.0, w.get(1)?.0))).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f64>() / 2.0
        }
    }

    fn polyline(segs: &[CubicBez]) -> Vec<Point> {
        let mut out = vec![];
        for (i, c) in segs.iter().enumerate() {
            for k in 0..=16 {
                if k == 0 && i > 0 {
                    continue;
                }
                out.push(c.eval(k as f64 / 16.0));
            }
        }
        out
    }

    fn cross(a0: Point, a1: Point, b0: Point, b1: Point) -> Option<(f64, f64)> {
        let (r, s) = (a1 - a0, b1 - b0);
        let d = r.cross(s);
        if d.abs() < 1e-12 {
            return None;
        }
        let t = (b0 - a0).cross(s) / d;
        let u = (b0 - a0).cross(r) / d;
        ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some((t, u))
    }

    fn polylines_cross(a: &[Point], b: &[Point]) -> bool {
        a.windows(2).any(|x| b.windows(2).any(|y| matches!((x, y), ([a0, a1], [b0, b1]) if cross(*a0, *a1, *b0, *b1).is_some())))
    }

    /// Where `rung` (a polyline) crosses `rail`, as a length along the rail; a rung that falls
    /// just short takes the rail point nearest its end.
    fn hit(rail: &Rail, rung: &[Point]) -> Option<f64> {
        let samples = rail.samples();
        for w in samples.windows(2) {
            let ([(p0, s0), (p1, s1)], true) = (w, true) else { continue };
            for r in rung.windows(2) {
                let [r0, r1] = r else { continue };
                if let Some((t, _)) = cross(*p0, *p1, *r0, *r1) {
                    return Some(s0 + (s1 - s0) * t);
                }
            }
        }
        let (Some(a), Some(b)) = (rung.first(), rung.last()) else { return None };
        let reach = a.distance(*b).max(1e-6);
        let best = samples.iter().map(|(p, s)| (p.distance(*a).min(p.distance(*b)), *s)).min_by(|x, y| x.0.total_cmp(&y.0))?;
        (best.0 < reach * 0.5).then_some(best.1)
    }

    /// Which subpaths are the rails, as Ink/Stitch decides: with three or more subpaths the rails
    /// are the ones crossing more than two others (exactly one when there are three), else the
    /// two longest.
    fn rail_indices(subs: &[Vec<CubicBez>]) -> (usize, usize) {
        if subs.len() <= 2 {
            return (0, 1);
        }
        let polys: Vec<Vec<Point>> = subs.iter().map(|s| polyline(s)).collect();
        let lens: Vec<f64> = subs.iter().map(|s| s.iter().map(|c| c.arclen(ACCURACY)).sum()).collect();
        let counts: Vec<usize> =
            polys.iter().enumerate().map(|(i, a)| polys.iter().enumerate().filter(|(j, b)| *j != i && polylines_cross(a, b)).count()).collect();
        let want = |c: usize| if subs.len() == 3 { c == 1 } else { c > 2 };
        let possible: Vec<usize> =
            (0..subs.len()).filter(|i| counts.get(*i).is_some_and(|c| want(*c)) && lens.get(*i).is_some_and(|l| *l > 0.1)).collect();
        if let [a, b] = possible[..] {
            return (a, b);
        }
        let mut by_len: Vec<usize> = (0..subs.len()).collect();
        by_len.sort_by(|a, b| lens.get(*b).copied().unwrap_or(0.0).total_cmp(&lens.get(*a).copied().unwrap_or(0.0)));
        (by_len.first().copied().unwrap_or(0), by_len.get(1).copied().unwrap_or(1))
    }

    /// The two rails of an Ink/Stitch satin column as paths with paired anchors.
    /// `reverse` is the column's `reverse_rails` choice (`automatic`, `none`, `first`,
    /// `second`, `both`).
    pub fn blocks(subs: &[Vec<CubicBez>], reverse: &str) -> Option<(BezPath, BezPath)> {
        if subs.len() < 2 {
            return None;
        }
        let (ia, ib) = rail_indices(subs);
        let mut a = Rail::new(subs.get(ia)?.clone());
        let mut b = Rail::new(subs.get(ib)?.clone());
        if a.len() <= 1e-9 || b.len() <= 1e-9 {
            return None;
        }
        let rungs: Vec<Vec<Point>> = subs.iter().enumerate().filter(|(i, _)| *i != ia && *i != ib).map(|(_, s)| polyline(s)).collect();
        let closed = a.closed() && b.closed();
        match reverse {
            "first" => a = a.reversed(),
            "second" => b = b.reversed(),
            "both" => {
                a = a.reversed();
                b = b.reversed();
            }
            "none" => {}
            _ if closed => {
                if (a.area() > 0.0) != (b.area() > 0.0) {
                    b = b.reversed();
                }
            }
            _ => {
                let (a0, a1, b0, b1) = (a.point(0.0), a.point(a.len()), b.point(0.0), b.point(b.len()));
                if a0.distance(b1) + a1.distance(b0) < a0.distance(b0) + a1.distance(b1) {
                    b = b.reversed();
                }
            }
        }
        // Closed rails start at the first rung (else at the points nearest each other).
        if closed {
            match rungs.iter().find_map(|r| Some((hit(&a, r)?, hit(&b, r)?))) {
                Some((sa, sb)) => {
                    a = a.rotated(sa);
                    b = b.rotated(sb);
                }
                None => {
                    let sb = b.nearest(a.point(0.0));
                    b = b.rotated(sb);
                }
            }
        }
        // The rungs' crossings, in order along both rails.
        let mut pairs: Vec<(f64, f64)> = rungs.iter().filter_map(|r| Some((hit(&a, r)?, hit(&b, r)?))).collect();
        pairs.sort_by(|x, y| x.0.total_cmp(&y.0));
        let (la, lb) = (a.len(), b.len());
        let eps = 1e-6 * la.max(lb).max(1.0);
        let mut kept: Vec<(f64, f64)> = vec![(0.0, 0.0)];
        for (sa, sb) in pairs {
            let Some(&(pa, pb)) = kept.last() else { continue };
            if sa > pa + eps && sb > pb + eps && sa < la - eps && sb < lb - eps {
                kept.push((sa, sb));
            }
        }
        kept.push((la, lb));
        // Without rungs, rails with as many nodes pair node by node.
        if kept.len() == 2 && a.nodes().len() == b.nodes().len() {
            kept = a.nodes().iter().copied().zip(b.nodes().iter().copied()).collect();
        }
        let (mut at_a, mut at_b) = (vec![], vec![]);
        for w in kept.windows(2) {
            let [(a0, b0), (a1, b1)] = w else { continue };
            at_a.push(*a0);
            at_b.push(*b0);
            // Each rail's nodes inside the section, as fractions of it, from either rail.
            let inner = |nodes: &[f64], s0: f64, s1: f64| -> Vec<f64> {
                let l = s1 - s0;
                if l <= eps {
                    return vec![];
                }
                nodes.iter().filter(|s| **s > s0 + eps && **s < s1 - eps).map(|s| (s - s0) / l).collect()
            };
            let mut marks: Vec<(f64, Option<f64>, Option<f64>)> = inner(a.nodes(), *a0, *a1).into_iter().map(|f| (f, Some(f), None)).collect();
            marks.extend(inner(b.nodes(), *b0, *b1).into_iter().map(|f| (f, None, Some(f))));
            marks.sort_by(|x, y| x.0.total_cmp(&y.0));
            // A node on one rail almost level with a node on the other pairs with it.
            let mut merged: Vec<(f64, Option<f64>, Option<f64>)> = vec![];
            for m in marks {
                if let Some(last) = merged.last_mut()
                    && (m.0 - last.0).abs() < 1e-3
                    && (last.1.is_none() != m.1.is_none())
                {
                    last.1 = last.1.or(m.1);
                    last.2 = last.2.or(m.2);
                    continue;
                }
                merged.push(m);
            }
            for (f, fa, fb) in merged {
                at_a.push(a0 + (a1 - a0) * fa.unwrap_or(f));
                at_b.push(b0 + (b1 - b0) * fb.unwrap_or(f));
            }
        }
        if let Some((a1, b1)) = kept.last() {
            at_a.push(*a1);
            at_b.push(*b1);
        }
        let (pa, pb) = (a.path(&at_a), b.path(&at_b));
        (pa.elements().len() == pb.elements().len() && pa.elements().len() >= 2).then_some((pa, pb))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn el(d: &str, params: &[(&str, &str)]) -> Element {
        Element {
            path: BezPath::from_svg(d).unwrap(),
            fill: None,
            stroke: Some([0, 0, 0]),
            stroke_width: 1.0,
            params: params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>(),
        }
    }

    fn anchors(p: &BezPath) -> Vec<Vec<Point>> {
        let mut out: Vec<Vec<Point>> = vec![];
        for e in p.elements() {
            match e {
                PathEl::MoveTo(q) => out.push(vec![*q]),
                _ => {
                    if let (Some(last), Some(q)) = (out.last_mut(), e.end_point()) {
                        last.push(q);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn rails_with_rungs_pair_at_the_rungs() {
        // Two vertical rails, the right one drawn the other way, two rungs, and an extra node
        // on the left rail.
        let e = el("M 0,0 L 0,50 L 0,100 M 10,100 L 10,0 M -2,30 L 12,30 M -2,70 L 12,70", &[("satin_column", "True")]);
        let p = convert(&e).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].stroke.as_ref().unwrap().kind, StitchType::SatinBlocks);
        let a = anchors(&p[0].path);
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].len(), a[1].len(), "{a:?}");
        // Rungs at y = 30 and 70, the node at 50, and the ends: five anchors a side, level.
        assert_eq!(a[0].len(), 5, "{a:?}");
        for (l, r) in a[0].iter().zip(&a[1]) {
            assert!((l.y - r.y).abs() < 1e-3, "{l:?} {r:?}");
            assert!(l.x.abs() < 1e-9 && (r.x - 10.0).abs() < 1e-9);
        }
    }

    #[test]
    fn rails_without_rungs_pair_in_proportion() {
        // Same node count: node by node.
        let e = el("M 0,0 C 0,30 0,60 0,90 L 0,100 M 10,0 L 10,40 L 10,100", &[("satin_column", "true")]);
        let a = anchors(&convert(&e).unwrap()[0].path);
        assert_eq!(a[0].len(), 3);
        assert_eq!(a[1].len(), 3);
        // Different counts: each rail gets the other's nodes at the same fraction.
        let e = el("M 0,0 L 0,100 M 10,0 L 10,25 L 10,50 L 10,100", &[("satin_column", "true")]);
        let a = anchors(&convert(&e).unwrap()[0].path);
        assert_eq!(a[0].len(), 4);
        assert_eq!(a[1].len(), 4);
        for (l, r) in a[0].iter().zip(&a[1]) {
            assert!((l.y - r.y).abs() < 1e-3);
        }
    }

    #[test]
    fn closed_rails_make_a_ring() {
        let mut bp = kurbo::Circle::new((0.0, 0.0), 5.0).to_path(0.01);
        bp.extend(kurbo::Circle::new((0.0, 0.0), 10.0).to_path(0.01));
        bp.extend(BezPath::from_svg("M 0,3 L 0,12").unwrap());
        let mut e = el("M0,0 L1,1", &[("satin_column", "True")]);
        e.path = bp;
        let a = anchors(&convert(&e).unwrap()[0].path);
        assert_eq!(a[0].len(), a[1].len());
        // Both start on the rung, at the top of the ring.
        assert!(a[0][0].distance(Point::new(0.0, 5.0)) < 0.1, "{:?}", a[0][0]);
        assert!(a[1][0].distance(Point::new(0.0, 10.0)) < 0.1, "{:?}", a[1][0]);
    }

    #[test]
    fn one_subpath_is_a_satin_at_the_stroke_width() {
        let mut e = el("M 0,0 L 100,0", &[("satin_column", "True")]);
        e.stroke_width = 10.0;
        let p = convert(&e).unwrap();
        let s = p[0].stroke.as_ref().unwrap();
        assert_eq!(s.kind, StitchType::Satin);
        assert!((s.satin.width - 10.0 * MM_PER_PX).abs() < 1e-9);
    }

    #[test]
    fn fills_strokes_and_parameters_carry_over() {
        let mut e = el("M 0,0 L 10,0 L 10,10 Z", &[("angle", "30"), ("row_spacing_mm", "0.3"), ("running_stitch_length_mm", "2")]);
        e.fill = Some([1, 2, 3]);
        let p = convert(&e).unwrap();
        let fill = p[0].fill.as_ref().unwrap();
        assert_eq!((fill.kind, fill.fill.angle, fill.fill.row_spacing), (StitchType::Tatami, 30.0, 0.3));
        assert_eq!(p[0].stroke.as_ref().unwrap().run.length, 2.0);
        let cross = el("M 0,0 L 1,1", &[("fill_method", "cross_stitch")]);
        assert_eq!(convert(&cross).unwrap_err(), Unsupported::CrossStitch);
        let bean = el("M 0,0 L 1,1", &[("bean_stitch_repeats", "1")]);
        assert_eq!(convert(&bean).unwrap()[0].stroke.as_ref().unwrap().kind, StitchType::Triple);
    }
}
