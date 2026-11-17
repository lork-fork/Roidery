//! The bridge to StitchFoundry's stitch engine (`sf-core`, vendored in `crates/sf-core`; MIT OR
//! Apache-2.0, see its `VENDORED.md`).
//!
//! **Satin Fill** ([`satin_fill`], stitch type `satinFill`) sews through it: satin across a
//! closed shape that turns with it, worked out from the outline alone every time the shape
//! changes. The shape is cut into simple columns (each hole slit where the shape is thinnest, so
//! an "O" becomes two columns; sharp inside corners where strokes meet cut too, so a "T" becomes
//! three), each column's two sides are paired, and the stitches turn from pair to pair. No rails
//! are drawn. [`art`] sews any StitchFoundry [`Embroidery`] record the same way (Satin Fill is
//! one such record).
//!
//! The conversion from art to StitchFoundry objects follows StitchFoundry's own
//! (`crates/embroidery/src/convert.rs` there), reading Roidery's flattened paths (millimetres, y
//! down) instead of document nodes.

use std::collections::HashSet;

use kurbo::Point;
use sf_core::attr::{Embroidery, StitchType as SfType};
use sf_core::direction::{Guide, auto_fill_angle, auto_guides};
use sf_core::engine::object_stitches;
use sf_core::geom::{Column, Contour, Point as SfPoint, Polygon, Rung, Shape as SfShape, poly, polyline};
use sf_core::model::{
    AppliqueObject, DesignSettings, EmbObject, FillKind, FillObject, FillParams, FillUnderlay, ObjectKind, RedworkObject, RunObject, SatinObject,
    SatinParams, SatinUnderlay, StitchBlock,
};
use sf_core::pattern::{Cmd, Stitch as SfStitch};
use sf_core::satin_line::satin_line;

use crate::settings::{SatinSettings, Underlay};
use crate::{MAX_STITCHES, Polyline, Stitch, StitchKind};

/// Millimetres per document point: positions a StitchFoundry record stores (inclination lines,
/// start and stop points) are in document points, as StitchFoundry keeps them.
const MM_PER_PT: f64 = 25.4 / 72.0;

/// The art one object sews, in millimetres.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Art {
    /// Every subpath, flattened.
    pub lines: Vec<Polyline>,
    /// Satin sews along the paths rather than across their area: open paths, and closed ones
    /// painted with a stroke and no fill (a satin border round a shape).
    pub line_art: bool,
    /// The stroke weight, mm: the width of a satin line that has none of its own.
    pub stroke_width: f64,
}

impl Art {
    /// Art read as an area when every path is closed, as lines otherwise.
    pub fn of(lines: &[Polyline]) -> Art {
        Art { lines: lines.to_vec(), line_art: lines.iter().any(|l| !l.closed), stroke_width: 0.0 }
    }
}

fn sf(p: Point) -> SfPoint {
    SfPoint::new(p.x, p.y)
}

fn points(l: &Polyline) -> Vec<SfPoint> {
    let pts: Vec<SfPoint> = l.points.iter().copied().filter(|p| p.x.is_finite() && p.y.is_finite()).map(sf).collect();
    if l.closed { polyline::dedup_ring(&pts, 1e-6) } else { polyline::dedup(&pts, 1e-6) }
}

/// The filled regions (with their holes) of closed paths, by the even-odd rule, as Roidery fills.
fn regions(lines: &[Polyline]) -> Vec<Polygon> {
    let rings: Vec<Polygon> = lines.iter().map(points).filter(|p| p.len() >= 3).map(|outer| Polygon { outer, holes: Vec::new() }).collect();
    if rings.is_empty() { Vec::new() } else { poly::clean(&rings) }
}

/// Roidery's satin settings as StitchFoundry's.
pub fn satin_params(s: &SatinSettings) -> SatinParams {
    let mut u = SatinUnderlay { auto: s.auto_underlay, center_run: false, edge_run: false, zigzag: false, ..SatinUnderlay::default() };
    for pass in &s.underlays {
        match *pass {
            Underlay::CenterWalk { length } => {
                u.center_run = true;
                u.stitch_length = length;
            }
            Underlay::Contour { length, inset } => {
                u.edge_run = true;
                u.stitch_length = length;
                u.inset = inset;
            }
            Underlay::ZigZag { spacing, inset } => {
                u.zigzag = true;
                u.zigzag_spacing = spacing;
                u.inset = inset;
            }
        }
    }
    SatinParams {
        spacing: s.density,
        pull_comp: s.pull_compensation,
        auto_spacing: s.auto_spacing,
        split_length: s.split_length,
        short_stitches: s.short_stitch_inset > 0.0,
        underlay: u,
        ..SatinParams::default()
    }
}

/// Satin Fill: satin across the closed shapes in `rings`, turning with them, started as near
/// `from` as it can be.
pub fn satin_fill(rings: &[Polyline], s: &SatinSettings, from: Option<Point>) -> Vec<Stitch> {
    let e = Embroidery::new(SfType::Satin(satin_params(s)));
    let closed: Vec<Polyline> = rings.iter().map(|r| Polyline { points: r.points.clone(), closed: true }).collect();
    art(&Art { lines: closed, line_art: false, stroke_width: 0.0 }, &e, from)
}

/// One column of a Satin Fill: its outline, and the inclination lines its stitches turn along.
pub type SatinColumn = (Vec<Point>, Vec<(Point, Point)>);

/// The columns a Satin Fill makes of `rings`: each one's outline and the inclination lines its
/// stitches turn along (none where the piece sews straight across). For previews and checks.
pub fn satin_fill_columns(rings: &[Polyline]) -> Vec<SatinColumn> {
    let closed: Vec<Polyline> = rings.iter().map(|r| Polyline { points: r.points.clone(), closed: true }).collect();
    let back = |p: SfPoint| Point::new(p.x, p.y);
    regions(&closed)
        .iter()
        .flat_map(|r| poly::split_holes(r).iter().flat_map(poly::split_branches).collect::<Vec<_>>())
        .map(|piece| {
            let guides = if piece.holes.is_empty() { auto_guides(&piece.outer).unwrap_or_default() } else { Vec::new() };
            (piece.outer.iter().copied().map(back).collect(), guides.iter().map(|g| (back(g.a), back(g.b))).collect())
        })
        .collect()
}

/// The stitches of `art` sewn as the StitchFoundry record `e` says, started as near `from` (where
/// the needle is) as the object allows.
pub fn art(art: &Art, e: &Embroidery, from: Option<Point>) -> Vec<Stitch> {
    let settings = DesignSettings::default();
    let entry = e.entry.map(|p| p * MM_PER_PT);
    let exit = e.exit.map(|p| p * MM_PER_PT);
    let kinds = convert(art, e);
    let last = kinds.len().saturating_sub(1);
    let mut out: Vec<Stitch> = Vec::new();
    for (i, kind) in kinds.into_iter().enumerate() {
        // Each piece starts near where the one before ended.
        let hint = out.last().map(|s| sf(s.point())).or(from.filter(|p| p.x.is_finite() && p.y.is_finite()).map(sf));
        let bare = without_underlay(&kind);
        let mut obj = EmbObject::new(1, kind, 0);
        obj.entry = entry.filter(|_| i == 0);
        obj.exit = exit.filter(|_| i == last);
        let sewn = object_stitches(&obj, &settings, hint);
        // The top stitches are the ones the object also sews without its underlay.
        let top: Option<HashSet<(i64, i64)>> = bare.map(|k| {
            obj.kind = k;
            object_stitches(&obj, &settings, hint).iter().filter(|s| s.cmd == Cmd::Stitch).map(key).collect()
        });
        append(&mut out, &sewn, top.as_ref());
        if out.len() >= MAX_STITCHES {
            break;
        }
    }
    out.retain(|s| s.x.is_finite() && s.y.is_finite());
    out.truncate(MAX_STITCHES);
    out
}

fn key(s: &SfStitch) -> (i64, i64) {
    // A thousandth of a millimetre: the same needle point, however it was reached.
    ((s.x * 1000.0).round() as i64, (s.y * 1000.0).round() as i64)
}

/// StitchFoundry's records as Roidery's stitches. A trim or a jump starts a separate part: its
/// first stitch is a [`StitchKind::Jump`], as the parts of Roidery's own objects start (the plan
/// trims when the move is long). A stop (to place appliqué fabric) has no place in Roidery's plan
/// yet and is left out.
fn append(out: &mut Vec<Stitch>, sewn: &[SfStitch], top: Option<&HashSet<(i64, i64)>>) {
    // A piece after the first is a separate part too.
    let mut jump = !out.is_empty();
    for s in sewn {
        match s.cmd {
            Cmd::Stitch => {
                let kind = if jump && !out.is_empty() {
                    StitchKind::Jump
                } else if top.is_some_and(|t| !t.contains(&key(s))) {
                    StitchKind::Underlay
                } else {
                    StitchKind::Top
                };
                out.push(Stitch::new(Point::new(s.x, s.y), kind));
                jump = false;
            }
            Cmd::Jump | Cmd::Trim => jump = true,
            Cmd::Stop | Cmd::ColorChange | Cmd::End => {}
        }
    }
}

/// `kind` with its underlay off, when it has any (to tell underlay from top stitches).
fn without_underlay(kind: &ObjectKind) -> Option<ObjectKind> {
    let bare_satin = |p: &SatinParams| SatinParams {
        underlay: SatinUnderlay { auto: false, center_run: false, edge_run: false, zigzag: false, ..p.underlay.clone() },
        ..p.clone()
    };
    match kind {
        ObjectKind::Satin(s) => Some(ObjectKind::Satin(SatinObject { column: s.column.clone(), params: bare_satin(&s.params) })),
        ObjectKind::Fill(f) => {
            let params = FillParams {
                underlay: FillUnderlay { edge_run: false, tatami: false, ..f.params.underlay.clone() },
                satin_column: f.params.satin_column.as_ref().map(bare_satin),
                ..f.params.clone()
            };
            Some(ObjectKind::Fill(FillObject { params, ..f.clone() }))
        }
        _ => None,
    }
}

fn guides_mm(e: &Embroidery) -> Vec<Guide> {
    e.guides.iter().map(|g| Guide::new(g.a * MM_PER_PT, g.b * MM_PER_PT)).collect()
}

fn lines_mm(lines: &[Vec<SfPoint>]) -> Vec<Vec<SfPoint>> {
    lines.iter().map(|line| line.iter().map(|&p| p * MM_PER_PT).collect()).collect()
}

/// Whether a closed outline reads as a column (long and thin), so tatami should turn along it
/// rather than run straight across.
fn column_like(ring: &[SfPoint], guides: &[Guide]) -> bool {
    let poly = Polygon::from_ring(ring.to_vec());
    let area = poly.area();
    let half = poly.perimeter() * 0.5;
    // elongation (half perimeter)² / area: 4 for a square, about 7 for a 5:1 bar
    !guides.is_empty() && area > 1e-9 && half * half / area > 6.5
}

/// One object's art as StitchFoundry objects (several for runs and fills of several parts).
fn convert(art: &Art, e: &Embroidery) -> Vec<ObjectKind> {
    let mut out = Vec::new();
    // Stitches edited by hand are sewn as they are.
    if let Some(frozen) = &e.frozen {
        let stitches: Vec<SfStitch> = frozen.iter().map(|s| SfStitch::new(s.x * MM_PER_PT, s.y * MM_PER_PT, s.cmd)).collect();
        if stitches.iter().any(|s| s.cmd == Cmd::Stitch) {
            out.push(ObjectKind::Stitches(StitchBlock { stitches }));
        }
        return out;
    }
    let lines: Vec<(Vec<SfPoint>, bool)> = art.lines.iter().map(|l| (points(l), l.closed)).filter(|(p, _)| p.len() >= 2).collect();
    match &e.stitch {
        // A branch group says how its members are sewn, lettering what they say, and decorations
        // (sequins, beads) need a device Roidery's machine files don't drive: none sews here.
        SfType::Branch | SfType::Lettering(_) | SfType::Decoration(_) => {}
        SfType::Redwork(params) => {
            let lines: Vec<Contour> = lines.iter().map(|(p, closed)| Contour::from_points(p, *closed)).collect();
            if !lines.is_empty() {
                out.push(ObjectKind::Redwork(RedworkObject { lines, params: params.clone() }));
            }
        }
        SfType::Manual => {
            let mut st: Vec<SfStitch> = Vec::new();
            for (pts, closed) in &lines {
                let mut pts = pts.clone();
                if *closed && let Some(&f) = pts.first() {
                    pts.push(f);
                }
                for (i, p) in pts.into_iter().enumerate() {
                    let cmd = if i == 0 && !st.is_empty() { Cmd::Jump } else { Cmd::Stitch };
                    if cmd == Cmd::Jump {
                        st.push(SfStitch::at(p, Cmd::Trim));
                    }
                    st.push(SfStitch::at(p, cmd));
                    if cmd == Cmd::Jump {
                        st.push(SfStitch::at(p, Cmd::Stitch));
                    }
                }
            }
            if st.len() >= 2 {
                out.push(ObjectKind::Stitches(StitchBlock { stitches: st }));
            }
        }
        SfType::Run(params) => {
            for (pts, closed) in &lines {
                out.push(ObjectKind::Run(RunObject { path: Contour::from_points(pts, *closed), params: params.clone() }));
            }
        }
        SfType::Satin(params) => {
            let open: Vec<&Vec<SfPoint>> = lines.iter().filter(|l| !l.1).map(|l| &l.0).collect();
            if let (2, [a, b]) = (lines.len(), open.as_slice()) {
                // A column between two rails.
                if let Some(col) = rails_column(a, b, &guides_mm(e)) {
                    out.push(ObjectKind::Satin(SatinObject { column: col, params: params.clone() }));
                }
            } else if !lines.is_empty() && (open.len() == lines.len() || art.line_art) {
                // A satin line along each path, round and round a closed one (a satin border).
                let width = e.width.unwrap_or(art.stroke_width.max(0.0));
                let width = if width.is_finite() && width > 0.05 { width } else { 3.0 };
                for (pts, closed) in &lines {
                    if let Some(col) = satin_line(pts, *closed, width, &e.profile, 0.6, params.corners) {
                        out.push(ObjectKind::Satin(SatinObject { column: col, params: params.clone() }));
                    }
                }
            } else {
                // A closed shape: satin across it, turning with the inclination lines.
                let fill = FillParams {
                    kind: FillKind::Satin,
                    spacing: params.spacing,
                    pull_comp: params.pull_comp,
                    split_length: params.split_length,
                    angle: 0.0,
                    satin_column: Some(params.clone()),
                    ..FillParams::satin()
                };
                for region in regions(&art.lines) {
                    fill_objects(region, fill.clone(), e, &mut out);
                }
            }
        }
        SfType::Fill(params) => {
            for region in regions(&art.lines) {
                fill_objects(region, params.clone(), e, &mut out);
            }
        }
        SfType::Applique(params) => {
            let shapes: Vec<SfShape> = regions(&art.lines).iter().map(SfShape::from_polygon).collect();
            if params.combine {
                if let Some((first, rest)) = shapes.split_first() {
                    out.push(ObjectKind::Applique(AppliqueObject {
                        shape: first.clone(),
                        params: params.clone(),
                        covered: Vec::new(),
                        extra: rest.to_vec(),
                    }));
                }
            } else {
                for shape in shapes {
                    out.push(ObjectKind::Applique(AppliqueObject { shape, params: params.clone(), covered: Vec::new(), extra: Vec::new() }));
                }
            }
        }
    }
    out
}

/// Fill objects for one region. Satin turns along simple columns, so without inclination lines
/// of its own the region is first cut open at its holes and at the sharp inside corners where
/// strokes join.
fn fill_objects(region: Polygon, params: FillParams, e: &Embroidery, out: &mut Vec<ObjectKind>) {
    if params.kind == FillKind::Satin && e.guides.is_empty() && e.auto_direction {
        for piece in poly::split_holes(&region).iter().flat_map(poly::split_branches) {
            out.push(fill_object(piece, params.clone(), e));
        }
    } else {
        out.push(fill_object(region, params, e));
    }
}

/// A fill object for one region, its stitch direction chosen automatically when the record has no
/// inclination lines.
fn fill_object(region: Polygon, mut params: FillParams, e: &Embroidery) -> ObjectKind {
    if params.kind == FillKind::Cross {
        params.center = params.center.map(|c| c * MM_PER_PT);
    }
    let mut guides = guides_mm(e);
    let splits = lines_mm(&e.splits);
    let flow = if params.kind == FillKind::Tatami { lines_mm(&e.flow) } else { Vec::new() };
    if guides.is_empty() && e.auto_direction && flow.is_empty() {
        let satin = params.kind == FillKind::Satin;
        // Textures and split lines show on straight rows: tatami with them doesn't turn.
        let textured = params.texture.is_some() || !splits.is_empty();
        let auto = if region.holes.is_empty() { auto_guides(&region.outer) } else { None };
        match auto {
            // satin (and tatami on long thin shapes) turns along the shape
            Some(g) if satin || (!textured && column_like(&region.outer, &g)) => guides = g,
            _ if matches!(params.kind, FillKind::Tatami | FillKind::Satin) => {
                params.angle = auto_fill_angle(&region, params.spacing.max(0.8));
            }
            _ => {}
        }
    }
    ObjectKind::Fill(FillObject { shape: SfShape::from_polygon(&region), params, guides, splits, flow })
}

/// A satin column between two rails: rungs from the inclination lines when there are two or
/// more, otherwise paired by the fraction of each rail's length.
fn rails_column(a: &[SfPoint], b: &[SfPoint], guides: &[Guide]) -> Option<Column> {
    let mut b = b.to_vec();
    // orient rail B like rail A
    if let (Some(a0), Some(b0), Some(bn)) = (a.first(), b.first(), b.last())
        && a0.distance(*bn) < a0.distance(*b0)
    {
        b.reverse();
    }
    let la = polyline::length(a);
    let lb = polyline::length(&b);
    if !(la.is_finite() && lb.is_finite()) || la < 1e-6 || lb < 1e-6 {
        return None;
    }
    let mut ts: Vec<(f64, f64)> = Vec::new();
    for g in guides {
        if let (Some(ta), Some(tb)) = (crossing(a, g), crossing(&b, g)) {
            ts.push((ta / la, tb / lb));
        }
    }
    ts.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut stations = vec![(0.0, 0.0)];
    stations.extend(ts.into_iter().filter(|t| t.0 > 1e-6 && t.0 < 1.0 - 1e-6));
    stations.push((1.0, 1.0));
    let mut rungs = Vec::new();
    for w in stations.windows(2) {
        let [(ta0, tb0), (ta1, tb1)] = *w else { continue };
        let seg = ((ta1 - ta0) * la).max((tb1 - tb0) * lb);
        let n = (seg.ceil() as usize).clamp(1, 100_000);
        for k in 0..n {
            let f = k as f64 / n as f64;
            let pa = polyline::point_at(a, (ta0 + (ta1 - ta0) * f) * la);
            let pb = polyline::point_at(&b, (tb0 + (tb1 - tb0) * f) * lb);
            rungs.push(Rung::new(pa, pb, Default::default()));
        }
    }
    if let (Some(&pa), Some(&pb)) = (a.last(), b.last()) {
        rungs.push(Rung::new(pa, pb, Default::default()));
    }
    (rungs.len() >= 2).then(|| Column::new(rungs))
}

/// Arc length along `rail` where the guide's line crosses it.
fn crossing(rail: &[SfPoint], g: &Guide) -> Option<f64> {
    let d = g.b - g.a;
    let mut acc = 0.0;
    let mut best: Option<(f64, f64)> = None;
    for w in rail.windows(2) {
        let [p, q] = *w else { continue };
        let e = q - p;
        let denom = d.cross(e);
        let seg = e.length();
        if denom.abs() > 1e-12 {
            let ap = p - g.a;
            let t = ap.cross(e) / denom; // along the guide line
            let u = ap.cross(d) / denom; // along the rail segment
            if (0.0..=1.0).contains(&u) {
                let off = (t - 0.5).abs();
                if best.is_none_or(|b| off < b.0) {
                    best = Some((off, acc + u * seg));
                }
            }
        }
        acc += seg;
    }
    best.map(|b| b.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(pts: &[(f64, f64)]) -> Polyline {
        Polyline { points: pts.iter().map(|&(x, y)| Point::new(x, y)).collect(), closed: true }
    }

    /// A band bent through a quarter turn, 4 mm wide.
    fn bend() -> Polyline {
        let mut pts = vec![];
        for i in 0..=30 {
            let a = std::f64::consts::FRAC_PI_2 * i as f64 / 30.0;
            pts.push((22.0 * a.cos(), -22.0 * a.sin()));
        }
        for i in (0..=30).rev() {
            let a = std::f64::consts::FRAC_PI_2 * i as f64 / 30.0;
            pts.push((18.0 * a.cos(), -18.0 * a.sin()));
        }
        ring(&pts)
    }

    fn top(st: &[Stitch]) -> Vec<Point> {
        st.iter().filter(|s| s.kind == StitchKind::Top).map(Stitch::point).collect()
    }

    #[test]
    fn a_satin_fill_turns_with_a_bent_band_and_stays_in_it() {
        let st = satin_fill(&[bend()], &SatinSettings { pull_compensation: 0.0, ..SatinSettings::default() }, None);
        let top = top(&st);
        assert!(top.len() > 100, "{}", top.len());
        for p in &top {
            let r = p.to_vec2().hypot();
            assert!((17.7..=22.3).contains(&r), "{p:?} r={r}");
        }
        // The stitches cross the band (about 4 mm long) all the way round, rather than running
        // straight across the whole shape: they turn with it.
        let long = top.windows(2).filter(|w| w[0].distance(w[1]) > 6.0).count();
        assert_eq!(long, 0);
        let across = top.windows(2).filter(|w| (3.0..5.0).contains(&w[0].distance(w[1]))).count();
        assert!(across * 10 > top.len() * 8, "{across} of {}", top.len());
        // Both ends of the bend are sewn.
        assert!(top.iter().any(|p| p.y > -1.0) && top.iter().any(|p| p.x < 1.0));
    }

    #[test]
    fn a_satin_fill_follows_the_shape_when_it_changes() {
        // The same band, one end pulled out: the stitches reach the new end with nothing redrawn.
        let mut longer = bend();
        for p in longer.points.iter_mut().filter(|p| p.y > -0.5) {
            p.y += 6.0;
        }
        let st = satin_fill(&[longer], &SatinSettings::default(), None);
        assert!(top(&st).iter().any(|p| p.y > 4.0));
    }

    #[test]
    fn a_ring_is_cut_into_columns_and_sewn_all_round() {
        let circle =
            |r: f64| ring(&(0..72).map(|i| (r * (i as f64 * 5.0).to_radians().cos(), r * (i as f64 * 5.0).to_radians().sin())).collect::<Vec<_>>());
        let rings = [circle(12.0), circle(8.0)];
        assert!(satin_fill_columns(&rings).len() >= 2, "an O becomes two columns");
        let st = satin_fill(&rings, &SatinSettings { pull_compensation: 0.0, ..SatinSettings::default() }, None);
        let top = top(&st);
        // Nothing sews across the hole.
        for p in &top {
            let r = p.to_vec2().hypot();
            assert!((7.8..=12.2).contains(&r), "{p:?} r={r}");
        }
        for quadrant in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
            assert!(top.iter().any(|p| p.x * quadrant.0 > 5.0 && p.y * quadrant.1 > 5.0), "{quadrant:?}");
        }
    }

    #[test]
    fn underlay_is_told_from_top_stitches() {
        let s = SatinSettings::default();
        let st = satin_fill(&[bend()], &s, None);
        let under = st.iter().filter(|s| s.kind == StitchKind::Underlay).count();
        assert!(under > 10 && under * 3 < st.len(), "{under} of {}", st.len());
        let none = satin_fill(&[bend()], &SatinSettings { auto_underlay: false, underlays: vec![], ..s }, None);
        assert_eq!(none.iter().filter(|s| s.kind == StitchKind::Underlay).count(), 0);
    }

    #[test]
    fn a_satin_fill_starts_near_the_needle() {
        let s = SatinSettings::default();
        let near_x = satin_fill(&[bend()], &s, Some(Point::new(20.0, 2.0)));
        let near_y = satin_fill(&[bend()], &s, Some(Point::new(-2.0, -20.0)));
        assert!(near_x[0].point().distance(Point::new(20.0, 0.0)) < 4.0, "{:?}", near_x[0]);
        assert!(near_y[0].point().distance(Point::new(0.0, -20.0)) < 4.0, "{:?}", near_y[0]);
    }

    #[test]
    fn stitchfoundry_types_sew_roiderys_art() {
        let square = ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        for name in
            ["run", "bean", "blanket", "satin", "fill", "satinfill", "contour", "spiral", "radial", "meander", "applique", "manual", "redwork"]
        {
            let e = Embroidery::new(SfType::from_name(name).unwrap_or_else(|| panic!("{name}")));
            let st = art(&Art::of(std::slice::from_ref(&square)), &e, None);
            assert!(!st.is_empty(), "{name}");
            assert!(st.iter().all(|s| s.x.is_finite() && s.y.is_finite() && (-3.0..=23.0).contains(&s.x) && (-3.0..=23.0).contains(&s.y)), "{name}");
        }
        // A stroke with no fill sews satin round the outline, not across the square.
        let e = Embroidery::new(SfType::from_name("satin").unwrap());
        let border = art(&Art { lines: vec![square], line_art: true, stroke_width: 2.0 }, &e, None);
        assert!(border.iter().all(|s| !((3.0..17.0).contains(&s.x) && (3.0..17.0).contains(&s.y))));
    }
}
