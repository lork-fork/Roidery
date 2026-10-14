//! Roidery: the Satin Blocks tool (1).
//!
//! Click on alternate sides of a column: the first click is on the left edge, the second on the
//! right edge across from it, and so on, each pair making a rung that sets the stitch angle there
//! (as in Ember's Satin Blocks tool). The satin appears as soon as there are two rungs and follows
//! every click. Enter or a double-click finishes the object, Backspace takes the last point back
//! and Escape abandons it. Shift keeps a rung square to the column's last direction.
//!
//! Click and drag pulls curve handles out of the point, as the Pen tool does, so the rail curves
//! into and out of it and the stitches fan round the curve; Alt while dragging moves only the
//! outgoing handle, for a corner.
//!
//! With one Satin Blocks object selected and nothing being drawn, the tool edits it, as Ember's
//! does: drag a rung's end to move it, click on a rail between two rungs to add one there, and
//! Alt-click a rung (or its end) to delete it. Rung 0, where sewing starts, is marked "Start".
//! Moving a rung end keeps its curve handles; Direct Selection edits the handles themselves.

use kurbo::{ParamCurve, ParamCurveArclen, ParamCurveNearest};
use serde_json::{Value, json};
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_geom::{Anchor, BezPath, Point, SubPath};

use crate::{Action, Cursor, Mods, Overlay, PointerEvent, PointerKind, Tool, ToolContext, ToolKey};

/// Rail and rung colour on the canvas.
const RAIL: [u8; 3] = [255, 64, 160];
const RUNG: [u8; 3] = [120, 120, 120];

/// Pointer distance, in screen pixels, that hits a rung end or a rail.
const HIT_PX: f64 = 6.0;
/// A press that moves less than this, in screen pixels, places a corner without handles.
const DRAG_PX: f64 = 3.0;

#[derive(Default)]
pub struct SatinBlocksTool {
    /// Points in click order: even indices on the left rail, odd ones on the right.
    points: Vec<Anchor>,
    hover: Option<Point>,
    /// The point just placed is still pressed: dragging pulls out its handles.
    pressing: bool,
    /// An interaction (the previewed object) is open.
    begun: bool,
    /// A rung end being dragged: (object, rung, right rail?).
    drag: Option<(NodeId, usize, bool)>,
}

/// What the pointer is over on the Satin Blocks object being edited.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    /// The end of rung `.0` on the left (false) or right (true) rail.
    End(usize, bool),
    /// Rail segment from rung `.0` to the next, `.1` of the way along.
    Rail(usize, f64),
    /// The rung line itself.
    Rung(usize),
}

/// The one selected Satin Blocks object and its two rails, cut to the rungs both have.
fn edited(cx: &ToolContext) -> Option<(NodeId, SubPath, SubPath)> {
    let [id] = cx.selection.objects.as_slice() else { return None };
    let n = cx.doc.node(*id)?;
    let kind = n.extras().attrs.as_ref()?.stitch.as_ref()?.get("type")?.as_str()?;
    if kind != "satinBlocks" {
        return None;
    }
    let NodeKind::Path { path, .. } = &n.kind else { return None };
    let [l, r, ..] = path.subpaths.as_slice() else { return None };
    let rungs = l.anchors.len().min(r.anchors.len());
    let take = |sp: &SubPath| SubPath::new(sp.anchors.iter().take(rungs).copied().collect(), false);
    Some((*id, take(l), take(r)))
}

/// What `p` hits on rails `left` and `right`, within `tol` document points; ends first. A rail
/// hit says how far along the segment it is, by length.
fn hit(left: &SubPath, right: &SubPath, p: Point, tol: f64) -> Option<Hit> {
    let near = |q: Point| q.distance(p) <= tol;
    for (i, (l, r)) in left.anchors.iter().zip(&right.anchors).enumerate() {
        if near(l.p) {
            return Some(Hit::End(i, false));
        }
        if near(r.p) {
            return Some(Hit::End(i, true));
        }
    }
    for rail in [left, right] {
        for i in 0..rail.segment_count() {
            let (t, d) = if rail.segment_is_line(i) {
                let (Some(a), Some(b)) = (rail.anchors.get(i), rail.anchors.get(i + 1)) else { continue };
                project(a.p, b.p, p)
            } else {
                let c = rail.segment(i);
                let near = c.nearest(p, 1e-6);
                let total = c.arclen(1e-3);
                let t = if total > 1e-9 { c.subsegment(0.0..near.t).arclen(1e-3) / total } else { 0.0 };
                (t, near.distance_sq.max(0.0).sqrt())
            };
            if d <= tol && t > 0.0 && t < 1.0 {
                return Some(Hit::Rail(i, t));
            }
        }
    }
    left.anchors.iter().zip(&right.anchors).position(|(l, r)| project(l.p, r.p, p).1 <= tol).map(Hit::Rung)
}

/// A rail through `anchors`, curving where they have handles.
fn rail_path(anchors: &[Anchor]) -> BezPath {
    let mut bp = BezPath::new();
    SubPath::new(anchors.to_vec(), false).to_bezpath_into(&mut bp);
    bp
}

/// A rail point as `stitch.createSatinBlocks` takes it: [x, y], or {p, in?, out?} with handles.
fn anchor_json(a: &Anchor) -> Value {
    if !a.has_in() && !a.has_out() {
        return json!([a.p.x, a.p.y]);
    }
    let mut o = json!({"p": [a.p.x, a.p.y]});
    if a.has_in() {
        o["in"] = json!([a.h_in.x, a.h_in.y]);
    }
    if a.has_out() {
        o["out"] = json!([a.h_out.x, a.h_out.y]);
    }
    o
}

/// Where `p` falls along segment `a`–`b` (0 to 1) and its distance from it.
fn project(a: Point, b: Point, p: Point) -> (f64, f64) {
    let ab = b - a;
    let len2 = ab.dot(ab);
    if len2 < 1e-18 {
        return (0.0, a.distance(p));
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (t, a.lerp(b, t).distance(p))
}

impl SatinBlocksTool {
    fn rails(&self) -> (Vec<Anchor>, Vec<Anchor>) {
        let left = self.points.iter().step_by(2).copied().collect();
        let right = self.points.iter().skip(1).step_by(2).copied().collect();
        (left, right)
    }

    /// Rungs complete so far.
    fn rungs(&self) -> usize {
        self.points.len() / 2
    }

    /// Show the object as it stands (or nothing yet, before the first rung).
    fn preview(&mut self) -> Vec<Action> {
        if self.rungs() == 0 {
            return if std::mem::take(&mut self.begun) { vec![Action::Cancel] } else { vec![] };
        }
        let (left, right) = self.rails();
        let (left, right) = (left.into_iter().take(self.rungs()), right.into_iter().take(self.rungs()));
        let mut acts = vec![];
        if !self.begun {
            self.begun = true;
            acts.push(Action::Begin("Satin Blocks".into()));
        }
        let pts = |v: &mut dyn Iterator<Item = Anchor>| v.map(|a| anchor_json(&a)).collect::<Vec<_>>();
        acts.push(Action::Preview("stitch.createSatinBlocks".into(), json!({"left": pts(&mut { left }), "right": pts(&mut { right })})));
        acts
    }

    /// Keep the object if it has a column to sew (two rungs), else drop it; start over.
    fn finish(&mut self) -> Vec<Action> {
        let keep = self.rungs() >= 2;
        self.points.clear();
        self.pressing = false;
        let begun = std::mem::take(&mut self.begun);
        match (begun, keep) {
            (true, true) => vec![Action::Commit],
            (true, false) => vec![Action::Cancel],
            _ => vec![],
        }
    }

    /// `p` with Shift: on the line through the rung's first point square to the column.
    fn constrain(&self, p: Point, mods: Mods) -> Point {
        if !mods.shift || self.points.len().is_multiple_of(2) {
            return p;
        }
        let Some(start) = self.points.last().map(|a| a.p) else { return p };
        // The column's direction: from the previous left point to this one.
        let Some(prev) = self.points.len().checked_sub(3).and_then(|i| self.points.get(i)).map(|a| a.p) else { return p };
        let dir = start - prev;
        let len = dir.hypot();
        if len < 1e-9 {
            return p;
        }
        let normal = vectorcraft_geom::Vec2::new(-dir.y / len, dir.x / len);
        start + normal * (p - start).dot(normal)
    }
}

impl SatinBlocksTool {
    /// A press with nothing being drawn: edit the selected Satin Blocks object if it hits it.
    fn edit_down(&mut self, cx: &ToolContext, p: Point, mods: Mods) -> Option<Vec<Action>> {
        let (id, left, right) = edited(cx)?;
        let tol = HIT_PX / cx.zoom.max(1e-6);
        Some(match (hit(&left, &right, p, tol)?, mods.alt) {
            (Hit::End(i, _) | Hit::Rung(i), true) => vec![Action::Exec("stitch.deleteRung".into(), json!({"id": id.0, "index": i}))],
            (Hit::End(i, right), false) => {
                self.drag = Some((id, i, right));
                vec![Action::Begin("Move Rung".into())]
            }
            (Hit::Rail(i, t), false) => vec![Action::Exec("stitch.insertRung".into(), json!({"id": id.0, "index": i, "t": t}))],
            _ => return None,
        })
    }

    /// Pull handles out of the point just placed: smooth, or with Alt only the outgoing one.
    fn pull_handles(&mut self, cx: &ToolContext, p: Point, mods: Mods) -> Vec<Action> {
        let Some(a) = self.points.last_mut() else { return vec![] };
        if a.p.distance(p) * cx.zoom.max(1e-6) < DRAG_PX {
            if !a.has_in() && !a.has_out() {
                return vec![];
            }
            a.retract();
        } else if mods.alt {
            a.set_handle(true, p, true);
        } else {
            *a = Anchor::smooth(a.p, p);
        }
        if self.points.len().is_multiple_of(2) { self.preview() } else { vec![] }
    }

    fn drag_to(&self, p: Point) -> Vec<Action> {
        let Some((id, index, right)) = self.drag else { return vec![] };
        let side = if right { "right" } else { "left" };
        vec![Action::Preview("stitch.moveRung".into(), json!({"id": id.0, "index": index, side: [p.x, p.y]}))]
    }
}

impl Tool for SatinBlocksTool {
    fn id(&self) -> &'static str {
        "satinBlocks"
    }

    fn busy(&self) -> bool {
        !self.points.is_empty() || self.drag.is_some()
    }

    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        if self.drag.is_some() {
            return match ev.kind {
                PointerKind::Drag | PointerKind::Move => self.drag_to(ev.pos),
                PointerKind::Up => {
                    self.drag = None;
                    vec![Action::Commit]
                }
                _ => vec![],
            };
        }
        if self.pressing {
            match ev.kind {
                PointerKind::Drag => return self.pull_handles(cx, ev.pos, ev.mods),
                PointerKind::Up => {
                    self.pressing = false;
                    return vec![];
                }
                _ => self.pressing = false,
            }
        }
        let p = self.constrain(ev.pos, ev.mods);
        match ev.kind {
            PointerKind::Move | PointerKind::Drag => {
                self.hover = Some(p);
                vec![]
            }
            PointerKind::Down
                if self.points.is_empty()
                    && let Some(acts) = self.edit_down(cx, p, ev.mods) =>
            {
                acts
            }
            PointerKind::Down => {
                // A click on the point just placed (the second click of a double-click) adds nothing.
                if self.points.last().is_some_and(|q| q.p.distance(p) < 1e-6) {
                    return vec![];
                }
                if self.points.len() >= 200_000 {
                    return vec![];
                }
                self.points.push(Anchor::corner(p));
                self.pressing = true;
                if self.points.len().is_multiple_of(2) { self.preview() } else { vec![] }
            }
            PointerKind::DoubleClick => self.finish(),
            PointerKind::Up => vec![],
        }
    }

    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        match key {
            ToolKey::Enter => self.finish(),
            ToolKey::Escape if self.drag.take().is_some() => vec![Action::Cancel],
            ToolKey::Escape => {
                self.points.clear();
                self.pressing = false;
                if std::mem::take(&mut self.begun) { vec![Action::Cancel] } else { vec![] }
            }
            ToolKey::Backspace | ToolKey::Delete => {
                let had_rung = self.points.len().is_multiple_of(2);
                self.pressing = false;
                if self.points.pop().is_none() {
                    return vec![];
                }
                if had_rung { self.preview() } else { vec![] }
            }
            _ => vec![],
        }
    }

    fn claims_key(&self, _cx: &ToolContext, key: ToolKey) -> bool {
        (self.drag.is_some() && matches!(key, ToolKey::Escape))
            || (!self.points.is_empty() && matches!(key, ToolKey::Enter | ToolKey::Escape | ToolKey::Backspace | ToolKey::Delete))
    }

    fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        let mut out = vec![];
        if self.points.is_empty()
            && let Some((_, left, right)) = edited(cx)
        {
            for (i, (l, r)) in left.anchors.iter().zip(&right.anchors).enumerate() {
                out.push(Overlay::Line { a: l.p, b: r.p, color: RUNG, dashed: false });
                out.push(Overlay::Anchor { p: l.p, color: RAIL, filled: i == 0, size: 6.0 });
                out.push(Overlay::Anchor { p: r.p, color: RUNG, filled: i == 0, size: 6.0 });
            }
            if let (Some(l), Some(r)) = (left.anchors.first(), right.anchors.first()) {
                out.push(Overlay::Label { p: l.p.lerp(r.p, 0.5), text: "Start".into(), color: RAIL });
            }
            return out;
        }
        let (left, right) = self.rails();
        for rail in [&left, &right] {
            if rail.len() >= 2 {
                out.push(Overlay::Path { path: rail_path(rail), color: RAIL, width: 1.0, dashed: false });
            }
        }
        for (l, r) in left.iter().zip(&right) {
            out.push(Overlay::Line { a: l.p, b: r.p, color: RUNG, dashed: false });
        }
        // The next rung or rail segment, following the pointer.
        if let (Some(h), Some(last), false) = (self.hover, self.points.last(), self.pressing) {
            let rail_end = self.points.len().checked_sub(2).and_then(|i| self.points.get(i));
            out.push(Overlay::Line { a: last.p, b: h, color: RUNG, dashed: true });
            if let Some(prev_same_side) = rail_end {
                out.push(Overlay::Path { path: rail_path(&[*prev_same_side, Anchor::corner(h)]), color: RAIL, width: 1.0, dashed: true });
            }
        }
        // The handles of the point being placed.
        if let Some(a) = self.points.last().filter(|a| a.has_in() || a.has_out()) {
            for h in [a.h_in, a.h_out] {
                out.push(Overlay::Line { a: a.p, b: h, color: RUNG, dashed: false });
                out.push(Overlay::Anchor { p: h, color: RUNG, filled: true, size: 4.0 });
            }
        }
        for (i, q) in self.points.iter().enumerate() {
            out.push(Overlay::Anchor { p: q.p, color: if i % 2 == 0 { RAIL } else { RUNG }, filled: i + 1 == self.points.len(), size: 5.0 });
        }
        out
    }

    fn cursor(&self, cx: &ToolContext, p: Point, mods: Mods) -> Cursor {
        if self.drag.is_some() {
            return Cursor::Move;
        }
        let over = || {
            let (_, left, right) = edited(cx)?;
            hit(&left, &right, p, HIT_PX / cx.zoom.max(1e-6))
        };
        match (self.points.is_empty().then(over).flatten(), mods.alt) {
            (Some(Hit::End(..) | Hit::Rung(_)), true) => Cursor::PenDelete,
            (Some(Hit::End(..)), false) => Cursor::Move,
            (Some(Hit::Rail(..)), false) => Cursor::PenAdd,
            _ => Cursor::Crosshair,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(t: &mut SatinBlocksTool, cx: &ToolContext, x: f64, y: f64) -> Vec<Action> {
        t.pointer(cx, &PointerEvent::new(PointerKind::Down, x, y))
    }

    #[test]
    fn rungs_preview_from_the_second_and_commit_on_enter() {
        let doc = vectorcraft_doc::Document::new(200.0, 200.0);
        let sel = vectorcraft_doc::Selection::default();
        let paint = crate::testutil::paint();
        let cx = crate::testutil::cx(&doc, &sel, &paint);
        let mut t = SatinBlocksTool::default();
        assert!(click(&mut t, &cx, 0.0, 0.0).is_empty());
        let a = click(&mut t, &cx, 0.0, 20.0);
        assert_eq!(a.first(), Some(&Action::Begin("Satin Blocks".into())));
        assert!(matches!(a.last(), Some(Action::Preview(id, _)) if id == "stitch.createSatinBlocks"));
        assert!(click(&mut t, &cx, 50.0, 0.0).is_empty());
        let a = click(&mut t, &cx, 50.0, 20.0);
        let Some(Action::Preview(_, p)) = a.last() else { panic!("{a:?}") };
        assert_eq!(p["left"], json!([[0.0, 0.0], [50.0, 0.0]]));
        assert_eq!(p["right"], json!([[0.0, 20.0], [50.0, 20.0]]));
        assert!(t.busy());
        assert_eq!(t.key(&cx, ToolKey::Enter, Mods::default()), vec![Action::Commit]);
        assert!(!t.busy());
    }

    #[test]
    fn dragging_a_point_pulls_out_curve_handles() {
        let doc = vectorcraft_doc::Document::new(200.0, 200.0);
        let sel = vectorcraft_doc::Selection::default();
        let paint = crate::testutil::paint();
        let cx = crate::testutil::cx(&doc, &sel, &paint);
        let mut t = SatinBlocksTool::default();
        let ev = |k, x, y, alt| PointerEvent { mods: Mods { alt, ..Mods::default() }, ..PointerEvent::new(k, x, y) };
        click(&mut t, &cx, 0.0, 0.0);
        click(&mut t, &cx, 0.0, 20.0);
        // The third point is dragged: its handles curve the left rail.
        t.pointer(&cx, &ev(PointerKind::Down, 50.0, 0.0, false));
        assert!(t.pointer(&cx, &ev(PointerKind::Drag, 60.0, -10.0, false)).is_empty());
        t.pointer(&cx, &ev(PointerKind::Up, 60.0, -10.0, false));
        let a = t.pointer(&cx, &ev(PointerKind::Down, 50.0, 20.0, false));
        let Some(Action::Preview(_, p)) = a.last() else { panic!("{a:?}") };
        assert_eq!(p["left"][1], json!({"p": [50.0, 0.0], "in": [40.0, 10.0], "out": [60.0, -10.0]}));
        assert_eq!(p["right"][1], json!([50.0, 20.0]));
        // Dragging the fourth re-previews it at once; Alt moves only the outgoing handle.
        let a = t.pointer(&cx, &ev(PointerKind::Drag, 60.0, 20.0, true));
        let Some(Action::Preview(_, p)) = a.last() else { panic!("{a:?}") };
        assert_eq!(p["right"][1], json!({"p": [50.0, 20.0], "out": [60.0, 20.0]}));
        // Dragged back onto the point, the handles go again.
        let a = t.pointer(&cx, &ev(PointerKind::Drag, 50.0, 20.5, true));
        let Some(Action::Preview(_, p)) = a.last() else { panic!("{a:?}") };
        assert_eq!(p["right"][1], json!([50.0, 20.0]));
    }

    #[test]
    fn one_rung_is_dropped_and_backspace_steps_back() {
        let doc = vectorcraft_doc::Document::new(200.0, 200.0);
        let sel = vectorcraft_doc::Selection::default();
        let paint = crate::testutil::paint();
        let cx = crate::testutil::cx(&doc, &sel, &paint);
        let mut t = SatinBlocksTool::default();
        click(&mut t, &cx, 0.0, 0.0);
        click(&mut t, &cx, 0.0, 20.0);
        // Back to one point: no rung left, so the preview goes.
        assert_eq!(t.key(&cx, ToolKey::Backspace, Mods::default()), vec![Action::Cancel]);
        click(&mut t, &cx, 0.0, 20.0);
        assert_eq!(t.key(&cx, ToolKey::Enter, Mods::default()), vec![Action::Cancel]);
        assert!(t.key(&cx, ToolKey::Escape, Mods::default()).is_empty());
    }

    /// A selected Satin Blocks object with rungs at x = 0, 50 and 100 (rails at y = 0 and 20).
    fn satin_doc() -> (vectorcraft_doc::Document, vectorcraft_doc::Selection, NodeId) {
        let mut d = vectorcraft_doc::Document::new(200.0, 200.0);
        let l = d.layers[0].id;
        let id = d.alloc_id();
        let mut bp = BezPath::new();
        for y in [0.0, 20.0] {
            bp.move_to((0.0, y));
            bp.line_to((50.0, y));
            bp.line_to((100.0, y));
        }
        let mut n = vectorcraft_doc::Node::path(id, vectorcraft_geom::PathData::from_bezpath(&bp), vectorcraft_doc::Appearance::default_art());
        n.extras_mut().attrs =
            Some(Box::new(vectorcraft_doc::ObjectAttributes { stitch: Some(json!({"type": "satinBlocks"})), ..Default::default() }));
        d.insert(Some(l), 0, n).unwrap();
        let mut sel = vectorcraft_doc::Selection::default();
        sel.set([id]);
        (d, sel, id)
    }

    #[test]
    fn rung_ends_drag_rails_insert_and_alt_deletes() {
        let (doc, sel, id) = satin_doc();
        let paint = crate::testutil::paint();
        let cx = crate::testutil::cx(&doc, &sel, &paint);
        let mut t = SatinBlocksTool::default();
        let ev = |k, x, y, alt| PointerEvent { mods: Mods { alt, ..Mods::default() }, ..PointerEvent::new(k, x, y) };
        // Drag the right end of rung 1.
        assert_eq!(t.cursor(&cx, Point::new(50.0, 21.0), Mods::default()), Cursor::Move);
        assert_eq!(t.pointer(&cx, &ev(PointerKind::Down, 50.0, 21.0, false)), vec![Action::Begin("Move Rung".into())]);
        assert!(t.busy());
        assert_eq!(
            t.pointer(&cx, &ev(PointerKind::Drag, 60.0, 30.0, false)),
            vec![Action::Preview("stitch.moveRung".into(), json!({"id": id.0, "index": 1, "right": [60.0, 30.0]}))]
        );
        assert_eq!(t.pointer(&cx, &ev(PointerKind::Up, 60.0, 30.0, false)), vec![Action::Commit]);
        assert!(!t.busy());
        // A click on the left rail between rungs 1 and 2 adds one there.
        assert_eq!(t.cursor(&cx, Point::new(75.0, 0.0), Mods::default()), Cursor::PenAdd);
        let a = t.pointer(&cx, &ev(PointerKind::Down, 75.0, 1.0, false));
        assert_eq!(a, vec![Action::Exec("stitch.insertRung".into(), json!({"id": id.0, "index": 1, "t": 0.5}))]);
        // Alt-click on a rung deletes it.
        assert_eq!(t.cursor(&cx, Point::new(100.0, 10.0), Mods { alt: true, ..Mods::default() }), Cursor::PenDelete);
        let a = t.pointer(&cx, &ev(PointerKind::Down, 100.0, 10.0, true));
        assert_eq!(a, vec![Action::Exec("stitch.deleteRung".into(), json!({"id": id.0, "index": 2}))]);
        // Away from the object, a click starts a new column; rung 0 is marked as the start.
        assert!(t.pointer(&cx, &ev(PointerKind::Down, 150.0, 150.0, false)).is_empty());
        assert!(t.busy());
        t.key(&cx, ToolKey::Escape, Mods::default());
        assert!(t.overlays(&cx).iter().any(|o| matches!(o, Overlay::Label { text, .. } if text == "Start")));
    }

    #[test]
    fn shift_squares_the_rung() {
        let mut t = SatinBlocksTool {
            points: vec![Anchor::corner(Point::new(0.0, 0.0)), Anchor::corner(Point::new(0.0, 10.0)), Anchor::corner(Point::new(30.0, 0.0))],
            ..Default::default()
        };
        let p = t.constrain(Point::new(37.0, 12.0), Mods { shift: true, ..Mods::default() });
        assert!((p.x - 30.0).abs() < 1e-9 && (p.y - 12.0).abs() < 1e-9);
        t.points.pop();
        assert_eq!(t.constrain(Point::new(37.0, 12.0), Mods { shift: true, ..Mods::default() }), Point::new(37.0, 12.0));
    }
}
