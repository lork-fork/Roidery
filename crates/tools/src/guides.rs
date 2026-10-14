//! Smart Guides: snapping to anchors, path segments (for dragged anchors), object bounds
//! (edges/centres), artboards, with the magenta construction lines and labels Illustrator users
//! expect.

use vectorcraft_doc::hit::{HitOptions, hit_test};
use vectorcraft_doc::{Document, NodeId, NodeKind, OrientedBox, Selection};
use vectorcraft_geom::kurbo::ParamCurveNearest;
use vectorcraft_geom::{Affine, Line, ParamCurve, PathSeg, Point, Rect, SubPath, Vec2};

use crate::bbox::Handle;
use crate::{Overlay, ToolContext};

pub const MAGENTA: [u8; 3] = [0xff, 0x3d, 0xfc];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Anchor,
    Center,
    Edge,
    Artboard,
    Bleed,
    Guide,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Anchor => "anchor",
            Kind::Center => "center",
            Kind::Edge => "path",
            Kind::Artboard => "artboard",
            Kind::Bleed => "bleed",
            Kind::Guide => "guide",
        }
    }
}

/// Snap targets gathered from the document (excluding the objects being edited).
#[derive(Default)]
pub struct Targets {
    points: Vec<(Point, Kind)>,
    xs: Vec<(f64, Point, Kind)>,
    ys: Vec<(f64, Point, Kind)>,
    /// Path segments and their control bounds, which a point lands on ("path"): gathered only for
    /// dragged anchors ([`Self::for_anchor_drag`]).
    segments: Vec<(Rect, PathSeg)>,
}

/// How many anchors, and how many segments, the targets gather at most.
const BUDGET: usize = 20_000;

impl Targets {
    pub fn collect(doc: &Document, exclude: &[NodeId], visible: Option<Rect>) -> Self {
        Self::collect_inner(doc, exclude, None, visible, false)
    }

    /// Targets for dragging artboard `index`: everything except that artboard and `exclude` (the
    /// art that moves along with it).
    pub fn for_artboard(doc: &Document, index: usize, exclude: &[NodeId]) -> Self {
        Self::collect_inner(doc, exclude, Some(index), None, false)
    }

    /// Targets for dragging the direct-selected anchors of `sel` (and its objects selected as a
    /// whole): the other art, its path segments too, and the anchors and segments that stay put on
    /// the paths being reshaped (their bounds move along, so they are no targets).
    pub fn for_anchor_drag(doc: &Document, sel: &Selection) -> Self {
        let mut t = Self::collect_inner(doc, &sel.objects, None, None, true);
        let (mut anchors, mut segments) = (BUDGET, BUDGET);
        for (id, set) in &sel.anchors {
            let Some(path) = doc.node(*id).and_then(|n| n.path_data()) else { continue };
            for (si, sp) in path.subpaths.iter().enumerate() {
                let moving = |ai: usize| set.contains(&(si, ai));
                for a in sp.anchors.iter().enumerate().filter(|(ai, _)| !moving(*ai)).map(|(_, a)| a.p).take(anchors) {
                    t.points.push((a, Kind::Anchor));
                    t.xs.push((a.x, a, Kind::Anchor));
                    t.ys.push((a.y, a, Kind::Anchor));
                    anchors -= 1;
                }
                t.push_segments(sp, moving, &mut segments);
            }
        }
        t
    }

    /// The segments of `sp` that stay put while the anchors `moving` accepts move, as targets, at
    /// most `budget` of them (counted down).
    fn push_segments(&mut self, sp: &SubPath, moving: impl Fn(usize) -> bool, budget: &mut usize) {
        let n = sp.anchors.len();
        for seg in 0..sp.segment_count() {
            if *budget == 0 {
                return;
            }
            if moving(seg) || moving((seg + 1) % n) {
                continue;
            }
            let c = sp.segment(seg);
            let bounds = Rect::from_points(c.p0, c.p1).union_pt(c.p2).union_pt(c.p3);
            // A straight one as a line, so the point lands on it exactly.
            self.segments.push((bounds, if sp.segment_is_line(seg) { PathSeg::Line(Line::new(c.p0, c.p3)) } else { PathSeg::Cubic(c) }));
            *budget -= 1;
        }
    }

    fn collect_inner(doc: &Document, exclude: &[NodeId], skip_artboard: Option<usize>, visible: Option<Rect>, segments: bool) -> Self {
        let mut t = Targets::default();
        let excluded = |id: NodeId| exclude.iter().any(|e| doc.ancestry(id).is_some_and(|a| a.contains(e)));
        let add_rect = |t: &mut Targets, r: Rect, kind: Kind| {
            let c = r.center();
            for p in [Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)] {
                t.xs.push((p.x, p, kind));
                t.ys.push((p.y, p, kind));
            }
            // A bleed shares its artboard's centre.
            if kind != Kind::Bleed {
                t.points.push((c, Kind::Center));
                t.xs.push((c.x, c, Kind::Center));
                t.ys.push((c.y, c, Kind::Center));
            }
        };
        for (_, ab) in doc.artboards.iter().enumerate().filter(|(i, _)| Some(*i) != skip_artboard) {
            add_rect(&mut t, ab.rect, Kind::Artboard);
            if doc.setup.has_bleed() {
                add_rect(&mut t, doc.setup.bleed_rect(ab.rect), Kind::Bleed);
            }
        }
        let (mut budget, mut segment_budget) = (BUDGET, BUDGET);
        doc.walk(|n| {
            if budget == 0 || n.is_container() || !n.visible || excluded(n.id) {
                return;
            }
            let Some(b) = n.geometric_bounds() else { return };
            if let Some(v) = visible
                && b.intersect(v).area() <= 0.0
                && !v.contains(b.center())
            {
                return;
            }
            if let NodeKind::Path { path, .. } = &n.kind {
                for (_, _, a) in path.anchors() {
                    t.points.push((a.p, Kind::Anchor));
                    budget = budget.saturating_sub(1);
                }
                if segments {
                    for sp in &path.subpaths {
                        t.push_segments(sp, |_| false, &mut segment_budget);
                    }
                }
            }
            add_rect(&mut t, b, Kind::Edge);
        });
        t
    }

    /// View → Snap to Point: only these targets' anchors pull, and the ruler guides (shown and
    /// unlocked, as the selection tools pick them) pull the pointer into line.
    fn for_snap_to_point(mut self, cx: &ToolContext) -> Self {
        self.points.retain(|(_, k)| *k == Kind::Anchor);
        self.xs.clear();
        self.ys.clear();
        self.segments.clear();
        if cx.guides {
            for g in &cx.doc.guides {
                let (lines, at) = if g.vertical { (&mut self.xs, Point::new(g.pos, 0.0)) } else { (&mut self.ys, Point::new(0.0, g.pos)) };
                lines.push((g.pos, at, Kind::Guide));
            }
        }
        self
    }

    /// What a dragged selection's grabbed point snaps to with View → Snap to Point and Smart
    /// Guides off: the anchors of the art but `exclude` and the ruler guides.
    pub fn snap_to_point(cx: &ToolContext, exclude: &[NodeId]) -> Option<Self> {
        (cx.snap_to_point && !cx.smart_guides).then(|| Self::collect(cx.doc, exclude, None).for_snap_to_point(cx))
    }

    /// Anchors and centres of the visible leaves under `roots` (the roots included) whose bounds
    /// reach into `near`.
    fn points_near(doc: &Document, roots: &[NodeId], near: Rect) -> Self {
        let mut t = Targets::default();
        for n in roots.iter().filter_map(|id| doc.node(*id)) {
            n.walk(&mut |c| {
                if c.is_container() || !c.visible {
                    return;
                }
                let Some(b) = c.geometric_bounds() else { return };
                if b.x0 > near.x1 || b.x1 < near.x0 || b.y0 > near.y1 || b.y1 < near.y0 {
                    return;
                }
                if let NodeKind::Path { path, .. } = &c.kind {
                    t.points.extend(path.anchors().map(|(_, _, a)| (a.p, Kind::Anchor)));
                }
                t.points.push((b.center(), Kind::Center));
            });
        }
        t
    }

    /// Snap a single point: onto the nearest anchor or centre, else onto the nearest segment, else
    /// into line with targets. Returns the snapped point and guide overlays.
    pub fn snap_point(&self, p: Point, tol: f64) -> (Point, Vec<Overlay>) {
        if let Some((q, k)) = self.points.iter().filter(|(q, _)| q.distance(p) <= tol).min_by(|a, b| a.0.distance(p).total_cmp(&b.0.distance(p))) {
            return (*q, vec![Overlay::Label { p: *q, text: k.label().into(), color: MAGENTA }]);
        }
        if let Some(q) = self.on_segment(p, tol) {
            return (q, vec![Overlay::Label { p: q, text: Kind::Edge.label().into(), color: MAGENTA }]);
        }
        let mut out = p;
        let mut ov = vec![];
        if let Some((x, from, _)) =
            self.xs.iter().filter(|(x, _, _)| (x - p.x).abs() <= tol).min_by(|a, b| (a.0 - p.x).abs().total_cmp(&(b.0 - p.x).abs()))
        {
            out.x = *x;
            ov.push(Overlay::Line { a: *from, b: Point::new(*x, p.y), color: MAGENTA, dashed: false });
        }
        if let Some((y, from, _)) =
            self.ys.iter().filter(|(y, _, _)| (y - p.y).abs() <= tol).min_by(|a, b| (a.0 - p.y).abs().total_cmp(&(b.0 - p.y).abs()))
        {
            out.y = *y;
            ov.push(Overlay::Line { a: *from, b: Point::new(p.x, *y), color: MAGENTA, dashed: false });
        }
        if !ov.is_empty() {
            ov.push(Overlay::Label { p: out, text: "align".into(), color: MAGENTA });
        }
        (out, ov)
    }

    /// The nearest point within `tol` of `p` on a target segment.
    fn on_segment(&self, p: Point, tol: f64) -> Option<Point> {
        self.segments
            .iter()
            .filter(|(b, _)| b.inflate(tol, tol).contains(p))
            .map(|(_, c)| (c, c.nearest(p, 1e-6)))
            .filter(|(_, n)| n.distance_sq <= tol * tol)
            .min_by(|a, b| a.1.distance_sq.total_cmp(&b.1.distance_sq))
            .map(|(c, n)| c.eval(n.t))
    }

    /// Snap a ruler guide at `v` (the x of a vertical one, the y of a horizontal one) into line
    /// with the nearest edge, centre or anchor within `tol`: where it goes, and a label there.
    pub fn snap_guide(&self, vertical: bool, v: f64, tol: f64) -> (f64, Vec<Overlay>) {
        let along = |q: &Point| if vertical { q.x } else { q.y };
        let lines = if vertical { &self.xs } else { &self.ys };
        let best = lines
            .iter()
            .map(|(t, from, k)| (*t, *from, *k))
            .chain(self.points.iter().map(|(q, k)| (along(q), *q, *k)))
            .filter(|(t, ..)| (t - v).abs() <= tol)
            .min_by(|a, b| (a.0 - v).abs().total_cmp(&(b.0 - v).abs()));
        match best {
            Some((t, from, k)) => (t, vec![Overlay::Label { p: from, text: k.label().into(), color: MAGENTA }]),
            None => (v, vec![]),
        }
    }

    /// Snap a dragged point that carries others along at `offsets` (a resize handle and the
    /// bleed edge beyond it): onto an anchor or centre near the point itself, otherwise into line
    /// with targets, each axis on whichever of them comes nearest.
    pub fn snap_point_with(&self, p: Point, offsets: &[Vec2], tol: f64) -> (Point, Vec<Overlay>) {
        if self.points.iter().any(|(q, _)| q.distance(p) <= tol) {
            return self.snap_point(p, tol);
        }
        let rects: Vec<Rect> = std::iter::once(Vec2::ZERO).chain(offsets.iter().copied()).map(|o| Rect::from_points(p + o, p + o)).collect();
        let (adj, mut ov) = self.snap_rects(&rects, tol);
        let out = p + adj;
        if !ov.is_empty() {
            ov.push(Overlay::Label { p: out, text: "align".into(), color: MAGENTA });
        }
        (out, ov)
    }

    /// Snap a moving rectangle (selection bounds after a move by `d`): tries its corners/edges/centre.
    pub fn snap_rect(&self, r: Rect, tol: f64) -> (Vec2, Vec<Overlay>) {
        self.snap_rects(&[r], tol)
    }

    /// Snap rectangles that move together (an artboard and its bleed): the edge or centre of any
    /// of them nearest a target, per axis. Returns the shift and the guides.
    pub fn snap_rects(&self, rects: &[Rect], tol: f64) -> (Vec2, Vec<Overlay>) {
        let nearest = |targets: &[(f64, Point, Kind)], along: fn(&Rect) -> [f64; 3]| {
            rects
                .iter()
                .flat_map(|r| along(r).into_iter().map(move |v| (v, *r)))
                .flat_map(|(v, r)| targets.iter().map(move |(t, from, _)| (t - v, *from, v, r)))
                .filter(|(d, ..)| d.abs() <= tol)
                .min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()))
        };
        let best_x = nearest(&self.xs, |r| [r.x0, r.center().x, r.x1]);
        let best_y = nearest(&self.ys, |r| [r.y0, r.center().y, r.y1]);
        let mut d = Vec2::ZERO;
        let mut ov = vec![];
        if let Some((dx, from, x, r)) = best_x {
            d.x = dx;
            let x = x + dx;
            let (y0, y1) = (from.y.min(r.y0), from.y.max(r.y1));
            ov.push(Overlay::Line { a: Point::new(x, y0), b: Point::new(x, y1), color: MAGENTA, dashed: false });
        }
        if let Some((dy, from, y, r)) = best_y {
            d.y = dy;
            let y = y + dy;
            let (x0, x1) = (from.x.min(r.x0), from.x.max(r.x1));
            ov.push(Overlay::Line { a: Point::new(x0, y), b: Point::new(x1, y), color: MAGENTA, dashed: false });
        }
        (d, ov)
    }

    /// Snap a bounding-box resize. `a` is the scale [`crate::bbox::scale_for_drag`] gave for
    /// dragging `handle` of the box `bx` (about the centre when `from_center`), in the box's own
    /// frame, and so is the result. The corners and the side that handle moves land on the nearest
    /// target within `tol` on the page, whatever the box's angle. A handle with one way to go (a
    /// side, or a proportional corner) slides along it until one of them is on a target.
    pub fn snap_scale(&self, bx: &OrientedBox, handle: Handle, a: Affine, proportional: bool, from_center: bool, tol: f64) -> (Affine, Vec<Overlay>) {
        type Hit = Option<(f64, Point)>;
        let r = bx.rect;
        let origin = if from_center { r.center() } else { handle.opposite().pos(r) };
        let (d0, [sx, _, _, sy, _, _]) = (handle.pos(r) - origin, a.as_coeffs());
        let (to_doc, to_local) = (bx.to_doc(), bx.to_doc().inverse());
        // A scale that collapses or flips the box is no snap.
        let valid = |n: f64, s: f64| n.is_finite() && n * s > 0.0;
        let nearest = |v: f64, ts: &[(f64, Point, Kind)]| -> Hit {
            ts.iter().map(|(t, from, _)| (*t, *from)).filter(|(t, _)| (t - v).abs() <= tol).min_by(|p, q| (p.0 - v).abs().total_cmp(&(q.0 - v).abs()))
        };
        let (mut nx, mut ny) = (sx, sy);
        let (mut hit_x, mut hit_y): (Hit, Hit) = (None, None);
        if handle.is_corner() && !proportional {
            // The corner goes anywhere: it takes the target's x, its y, or both.
            let p = to_doc * (a * handle.pos(r));
            let (hx, hy) = (nearest(p.x, &self.xs), nearest(p.y, &self.ys));
            for (tx, ty) in [(hx, hy), (hx, None), (None, hy)] {
                if tx.is_none() && ty.is_none() {
                    continue;
                }
                let q = to_local * Point::new(tx.map_or(p.x, |h| h.0), ty.map_or(p.y, |h| h.0));
                let (mut cx, mut cy) = ((q.x - origin.x) / d0.x, (q.y - origin.y) / d0.y);
                // Square to the page, the axis without a target stays exactly where it was.
                if bx.angle == 0.0 {
                    (cx, cy) = (if tx.is_some() { cx } else { sx }, if ty.is_some() { cy } else { sy });
                }
                if valid(cx, sx) && valid(cy, sy) {
                    (nx, ny, hit_x, hit_y) = (cx, cy, tx, ty);
                    break;
                }
            }
        } else {
            // One parameter `s`: the scale is `c + s·e` on each axis.
            let (ax, _) = handle.axes();
            let (e, c, s0) = match (handle.is_corner(), ax, proportional) {
                (true, _, _) => ((sx.signum(), sy.signum()), (0.0, 0.0), sx.abs()),
                (false, true, true) => ((1.0, sx.signum()), (0.0, 0.0), sx),
                (false, true, false) => ((1.0, 0.0), (0.0, 1.0), sx),
                (false, false, true) => ((sy.signum(), 1.0), (0.0, 0.0), sy),
                (false, false, false) => ((0.0, 1.0), (1.0, 0.0), sy),
            };
            // The points that may land on a target: the handle, and the two ends of its side.
            let i = handle as usize;
            let ends = if handle.is_corner() { [None, None] } else { [Handle::ALL.get((i + 1) % 8), Handle::ALL.get((i + 7) % 8)] };
            // How far the handle travels per unit of `s`: a snap may not drag it far from the
            // pointer, as it would for a target the side runs almost along.
            let travel = (e.0 * d0.x).hypot(e.1 * d0.y);
            let mut best: Option<(f64, f64, bool, (f64, Point))> = None;
            for h in std::iter::once(handle).chain(ends.into_iter().flatten().copied()) {
                let v = h.pos(r) - origin;
                let base = to_doc * (origin + Vec2::new(c.0 * v.x, c.1 * v.y));
                let dir = (to_doc * Point::new(e.0 * v.x, e.1 * v.y)).to_vec2();
                let p = base + dir * s0;
                for (is_x, at, b, d, ts) in [(true, p.x, base.x, dir.x, &self.xs), (false, p.y, base.y, dir.y, &self.ys)] {
                    if d.abs() <= 1e-9 {
                        continue;
                    }
                    let Some(hit) = nearest(at, ts) else { continue };
                    let s = (hit.0 - b) / d;
                    let moved = (s - s0).abs() * travel;
                    if valid(s, s0) && moved <= 2.0 * tol && best.is_none_or(|k| moved < k.1) {
                        best = Some((s, moved, is_x, hit));
                    }
                }
            }
            if let Some((s, _, is_x, hit)) = best {
                (nx, ny) = (c.0 + s * e.0, c.1 + s * e.1);
                if is_x {
                    hit_x = Some(hit);
                } else {
                    hit_y = Some(hit);
                }
            }
        }
        let out = Affine::translate(origin.to_vec2()) * Affine::scale_non_uniform(nx, ny) * Affine::translate(-origin.to_vec2());
        // The lines run from the target's own point across the resized box, on the page.
        let nr = (to_doc * out).transform_rect_bbox(r);
        let mut ov = vec![];
        if let Some((x, from)) = hit_x {
            ov.push(Overlay::Line { a: Point::new(x, from.y.min(nr.y0)), b: Point::new(x, from.y.max(nr.y1)), color: MAGENTA, dashed: false });
        }
        if let Some((y, from)) = hit_y {
            ov.push(Overlay::Line { a: Point::new(from.x.min(nr.x0), y), b: Point::new(from.x.max(nr.x1), y), color: MAGENTA, dashed: false });
        }
        (out, ov)
    }
}

/// Snap `p` for a drawing tool when smart guides (or grid snapping) are on.
pub fn snap_draw(cx: &ToolContext, p: Point, exclude: &[NodeId]) -> (Point, Vec<Overlay>) {
    snap_with(cx, p, || Targets::collect(cx.doc, exclude, None))
}

/// Snap `p` to pixels or the grid when they are on, else to the smart guide `targets`, else (Snap
/// to Point) to their anchors and the ruler guides within the Snap to Point distance.
fn snap_with(cx: &ToolContext, p: Point, targets: impl FnOnce() -> Targets) -> (Point, Vec<Overlay>) {
    PointSnap::new(cx, targets).snap(cx, p)
}

/// Snapping for a dragged point, its targets gathered once (when the drag begins): what
/// [`snap_draw`] does for a point at a time.
pub struct PointSnap {
    /// The smart guide targets, or with Smart Guides off those of Snap to Point, and how near (in
    /// screen pixels) they pull; none with pixels or the grid snapping instead, or nothing on.
    targets: Option<(Targets, f64)>,
}

impl PointSnap {
    pub fn new(cx: &ToolContext, targets: impl FnOnce() -> Targets) -> Self {
        let targets = if cx.snap_to_pixel || cx.snap_to_grid {
            None
        } else if cx.smart_guides {
            Some((targets(), 5.0))
        } else if cx.snap_to_point {
            Some((targets().for_snap_to_point(cx), cx.snap_tolerance))
        } else {
            None
        };
        Self { targets }
    }

    /// Where `p` goes, and the guides showing why.
    pub fn snap(&self, cx: &ToolContext, p: Point) -> (Point, Vec<Overlay>) {
        if cx.snap_to_pixel {
            return (Point::new(p.x.round(), p.y.round()), vec![]);
        }
        if cx.snap_to_grid {
            return (vectorcraft_geom::snap::snap_point_to_grid(p, cx.grid_step()), vec![]);
        }
        match &self.targets {
            Some((t, tol)) => t.snap_point(p, cx.tol(*tol)),
            None => (p, vec![]),
        }
    }
}

/// Where a dragged direction handle of anchor `ai` of subpath `si` of path `id` goes for the
/// pointer at `p`: with Shift at a multiple of 45° (from the Constrain Angle) round its anchor,
/// else snapped as a drawn point is (smart guides, the grid, pixels).
pub fn snap_handle(cx: &ToolContext, (id, si, ai): (NodeId, usize, usize), p: Point, shift: bool) -> (Point, Vec<Overlay>) {
    let path = cx.doc.node(id).and_then(|n| n.path_data());
    if shift && let Some(a) = path.and_then(|pd| pd.subpaths.get(si)?.anchors.get(ai)) {
        return (a.p + vectorcraft_geom::constrain_angle_from(p - a.p, 45.0, cx.constrain_angle), vec![]);
    }
    snap_with(cx, p, || {
        // The path's bounds move with the handle, so they would chase it: its anchors (which stay
        // put) are the targets instead, the handle lining up with them too.
        let mut t = Targets::collect(cx.doc, &[id], None);
        for (_, _, a) in path.into_iter().flat_map(|pd| pd.anchors()).take(20_000) {
            t.points.push((a.p, Kind::Anchor));
            t.xs.push((a.p.x, a.p, Kind::Anchor));
            t.ys.push((a.p.y, a.p, Kind::Anchor));
        }
        t
    })
}

/// Snap a picked point (a transform tool's reference point) to the nearest anchor or centre of the
/// selection or of the object under the pointer, when Snap to Point or Smart Guides is on.
pub fn snap_pick(cx: &ToolContext, p: Point) -> (Point, Vec<Overlay>) {
    if !(cx.snap_to_point || cx.smart_guides) {
        return (p, vec![]);
    }
    let tol = cx.tol(if cx.smart_guides { 5.0 } else { cx.snap_tolerance });
    let mut roots = cx.selection.objects.clone();
    roots.extend(hit_test(cx.doc, p, HitOptions { tol, ..cx.hit_options() }).map(|h| h.leaf));
    let near = Rect::new(p.x - tol, p.y - tol, p.x + tol, p.y + tol);
    Targets::points_near(cx.doc, &roots, near).snap_point(p, tol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn artboard_bleed_is_a_target() {
        let (mut d, _) = doc_with_rect();
        let t = Targets::collect(&d, &[], None);
        assert_eq!(t.snap_point(Point::new(512.0, 300.0), 4.0).0.x, 512.0, "no bleed, nothing near");
        d.setup.bleed = [10.0; 4];
        let t = Targets::collect(&d, &[], None);
        assert_eq!(t.snap_point(Point::new(512.0, 300.0), 4.0).0.x, 510.0);
    }

    #[test]
    fn snaps_to_anchor_and_alignment() {
        let (d, _) = doc_with_rect();
        let t = Targets::collect(&d, &[], None);
        let (p, ov) = t.snap_point(Point::new(102.0, 99.0), 4.0);
        assert_eq!(p, Point::new(100.0, 100.0));
        assert!(matches!(&ov[0], Overlay::Label { text, .. } if text == "anchor"));
        let (p, _) = t.snap_point(Point::new(301.0, 199.0), 4.0);
        assert_eq!(p.y, 200.0);
    }

    #[test]
    fn rect_snap_aligns_edges() {
        let (d, id) = doc_with_rect();
        let t = Targets::collect(&d, &[id], None);
        // Artboard is 0..500; a rect at x0=2 snaps to the artboard's left edge.
        let (dv, ov) = t.snap_rect(Rect::new(2.0, 50.0, 52.0, 90.0), 4.0);
        assert_eq!(dv.x, -2.0);
        assert!(!ov.is_empty());
    }

    #[test]
    fn scale_snap_lands_the_moved_edges_on_targets() {
        use crate::bbox::scale_for_drag;
        let (d, _) = doc_with_rect();
        // The document's rect is 100..200: a box beside it is resized against it.
        let t = Targets::collect(&d, &[], None);
        let r = Rect::new(250.0, 100.0, 300.0, 150.0);
        let snap = |h: Handle, p: Point, shift: bool, alt: bool| {
            let (a, ov) = t.snap_scale(&OrientedBox::aligned(r), h, scale_for_drag(r, h, p, shift, alt), shift, alt, 4.0);
            (a.transform_rect_bbox(r), ov)
        };
        // The bottom edge lands on the neighbour's bottom: same height.
        let (nr, ov) = snap(Handle::Bottom, Point::new(275.0, 197.0), false, false);
        assert_eq!(nr, Rect::new(250.0, 100.0, 300.0, 200.0));
        assert_eq!(ov.len(), 1);
        // A corner snaps each axis on its own: y to the neighbour, x stays free.
        let (nr, ov) = snap(Handle::BottomRight, Point::new(330.0, 202.0), false, false);
        assert_eq!(nr, Rect::new(250.0, 100.0, 330.0, 200.0));
        assert_eq!(ov.len(), 1);
        // Proportional: the snapped axis carries the other one.
        let (nr, _) = snap(Handle::BottomRight, Point::new(340.0, 198.0), true, false);
        assert_eq!(nr, Rect::new(250.0, 100.0, 350.0, 200.0));
        // From the centre the dragged edge still lands on the target.
        let (nr, _) = snap(Handle::Bottom, Point::new(275.0, 198.0), false, true);
        assert_eq!(nr, Rect::new(250.0, 50.0, 300.0, 200.0));
        // Out of reach, and the fixed edge's own target: untouched.
        let (nr, ov) = snap(Handle::Bottom, Point::new(275.0, 180.0), false, false);
        assert_eq!(nr, Rect::new(250.0, 100.0, 300.0, 180.0));
        assert!(ov.is_empty());
        let (nr, ov) = snap(Handle::Bottom, Point::new(275.0, 102.0), false, false);
        assert_eq!(nr, Rect::new(250.0, 100.0, 300.0, 102.0));
        assert!(ov.is_empty());
    }

    /// Dragging an anchor of a path: its other anchors and the segments not touching it stay
    /// targets, but neither it nor the path's bounds (which move) are.
    #[test]
    fn anchor_drag_targets_leave_out_what_moves() {
        let (d, id) = doc_with_rect();
        let mut s = Selection::default();
        s.set([id]);
        s.anchors.insert(id, [(0, 0)].into_iter().collect());
        let t = Targets::for_anchor_drag(&d, &s);
        let anchors: Vec<Point> = t.points.iter().filter(|(_, k)| *k == Kind::Anchor).map(|(p, _)| *p).collect();
        assert_eq!(anchors, [Point::new(200.0, 100.0), Point::new(200.0, 200.0), Point::new(100.0, 200.0)]);
        assert_eq!(t.segments.len(), 2);
        assert!(!t.points.iter().any(|(p, k)| *k == Kind::Center && *p == Point::new(150.0, 150.0)), "the rect's centre moves");
        assert_eq!(t.snap_point(Point::new(101.0, 99.0), 4.0).0, Point::new(100.0, 100.0), "in line with its neighbours, not on itself");
    }
}
