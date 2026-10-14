//! The Pen tool (P).
//!
//! Click adds a corner anchor; click-drag adds a smooth anchor with symmetric handles (Alt breaks
//! them: the incoming handle stays where it was when Alt went down and only the outgoing one
//! follows the pointer; Space held moves the anchor, handles and all); Shift constrains to 45°.
//! Clicking the first anchor closes the path. Clicking the last one retracts its outgoing handle, so
//! the next segment leaves it as a corner; dragging from it pulls a new one out on its own.
//! Enter/Esc (or switching tools) ends the path. Clicking the end of a selected open path continues
//! it. The rubber-band preview shows the next segment (Enable Rubber Band for Pen Tool). Auto Add/Delete: between paths, a click on a
//! segment of a selected path adds an anchor there and a click on one of its anchors deletes it
//! (Shift held or General → Disable Auto Add/Delete starts a new path instead). On a selected
//! blend's spine a click adds a point (on a point no key object sits on: deletes it).

use serde_json::json;
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_geom::{BezPath, Point};

use crate::{Action, Cursor, Mods, Overlay, PointerEvent, PointerKind, Tool, ToolContext, ToolKey};

#[derive(Default)]
pub struct PenTool {
    /// The path being drawn (set once the first anchor exists and the engine selected it).
    drawing: bool,
    drag: Option<(Point, bool)>,
    hover: Option<Point>,
    /// The incoming handle of the anchor being dragged out, as last previewed.
    in_h: Point,
    /// The pointer at the last button-down or drag: Space held moves the anchor as far as it moves.
    last: Point,
    /// The last anchor of the path being drawn, while a click retracts its outgoing handle or a
    /// drag pulls a new one out: (path, subpath, anchor, its position).
    handle: Option<(NodeId, usize, usize, Point)>,
}

/// The open path the pen is extending: the single selected open path.
fn active_path(cx: &ToolContext) -> Option<(NodeId, Point, Point, Point)> {
    if cx.selection.objects.len() != 1 {
        return None;
    }
    let id = cx.selection.objects[0];
    let n = cx.doc.node(id)?;
    let NodeKind::Path { path, .. } = &n.kind else { return None };
    let sp = path.subpaths.last()?;
    if sp.closed {
        return None;
    }
    let first = sp.anchors.first()?.p;
    let last = sp.anchors.last()?;
    Some((id, first, last.p, last.h_out))
}

/// The last anchor of `id`'s last subpath: (subpath, anchor).
fn last_anchor(cx: &ToolContext, id: NodeId) -> Option<(usize, usize)> {
    let pd = cx.doc.node(id)?.path_data()?;
    let si = pd.subpaths.len().checked_sub(1)?;
    let ai = pd.subpaths.get(si)?.anchors.len().checked_sub(1)?;
    Some((si, ai))
}

/// Preview the outgoing handle of anchor `ai` of subpath `si` at `h`, the incoming one left alone.
fn set_out_handle(id: NodeId, si: usize, ai: usize, h: Point) -> Action {
    Action::Preview(
        "path.setHandle".into(),
        json!({"id": id.0, "subpath": si, "anchor": ai, "which": "out", "x": h.x, "y": h.y, "independent": true}),
    )
}

impl Tool for PenTool {
    fn id(&self) -> &'static str {
        "pen"
    }
    fn busy(&self) -> bool {
        self.drag.is_some() || self.handle.is_some()
    }
    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        let exclude: Vec<vectorcraft_doc::NodeId> = if self.drawing { cx.selection.objects.clone() } else { vec![] };
        let (mut p, _) = if matches!(ev.kind, PointerKind::Down) { crate::guides::snap_draw(cx, ev.pos, &exclude) } else { (ev.pos, vec![]) };
        let tol = cx.tol(5.0);
        let active = if self.drawing { active_path(cx) } else { None };
        if self.drawing && active.is_none() && ev.kind == PointerKind::Down {
            self.drawing = false;
        }
        match ev.kind {
            PointerKind::Move => {
                self.hover = Some(p);
                vec![]
            }
            PointerKind::Down => {
                self.last = ev.pos;
                if let Some((id, first, last, _)) = active {
                    if ev.mods.shift {
                        p = last + vectorcraft_geom::constrain_angle(p - last, 45.0);
                    }
                    let end = last_anchor(cx, id);
                    // A one-anchor path doesn't close on itself: its anchor is the last one too.
                    if p.distance(first) <= tol && end.is_some_and(|(_, ai)| ai > 0) {
                        self.drag = Some((first, true));
                        return vec![Action::Begin("Close Path".into()), Action::Preview("path.close".into(), json!({"id": id.0}))];
                    }
                    if p.distance(last) <= tol
                        && let Some((si, ai)) = end
                    {
                        self.handle = Some((id, si, ai, last));
                        return vec![Action::Begin("Convert Anchor Point".into()), set_out_handle(id, si, ai, last)];
                    }
                    self.drag = Some((p, false));
                    self.in_h = p;
                    return vec![Action::Begin("Pen".into()), Action::Preview("path.appendAnchor".into(), json!({"id": id.0, "x": p.x, "y": p.y}))];
                }
                if let Some(acts) = spine_click(cx, p, tol) {
                    return acts;
                }
                // Continue a selected open path when clicking on one of its ends.
                if let Some((_, first, last, _)) = active_path(cx)
                    && (p.distance(last) <= tol || p.distance(first) <= tol)
                {
                    self.drawing = true;
                    if p.distance(first) <= tol && p.distance(last) > tol {
                        return vec![Action::Exec("path.reverse".into(), json!({}))];
                    }
                    return vec![];
                }
                if let Some(act) = auto_add_delete(cx, ev.pos, ev.mods, tol) {
                    return vec![act];
                }
                self.drawing = true;
                self.drag = Some((p, false));
                vec![Action::Begin("Pen".into()), Action::Preview("path.create".into(), json!({"anchors": [{"x": p.x, "y": p.y}]}))]
            }
            PointerKind::Drag => {
                if let Some((id, si, ai, a)) = self.handle {
                    let mut h = ev.pos;
                    if ev.mods.shift {
                        h = a + vectorcraft_geom::constrain_angle(ev.pos - a, 45.0);
                    }
                    return vec![set_out_handle(id, si, ai, h)];
                }
                let Some((mut a, closing)) = self.drag else { return vec![] };
                // Space held moves the anchor being placed, handles and all.
                if ev.mods.space && !closing {
                    let d = ev.pos - self.last;
                    a += d;
                    self.in_h += d;
                    self.drag = Some((a, closing));
                }
                self.last = ev.pos;
                let mut out_h = ev.pos;
                if ev.mods.shift {
                    out_h = a + vectorcraft_geom::constrain_angle(ev.pos - a, 45.0);
                }
                let alt = ev.mods.alt;
                let Some((id, ..)) = active_path(cx).or(active) else {
                    // First anchor of a new path: re-issue create with handles.
                    let in_h = a - (out_h - a);
                    return vec![Action::Preview(
                        "path.create".into(),
                        json!({"anchors": [{"x": a.x, "y": a.y, "out": [out_h.x, out_h.y], "in": [in_h.x, in_h.y]}]}),
                    )];
                };
                if closing {
                    return vec![Action::Preview(
                        "path.close".into(),
                        json!({"id": id.0, "in": [2.0 * a.x - out_h.x, 2.0 * a.y - out_h.y], "independent": alt}),
                    )];
                }
                // Alt pressed mid-drag keeps the curve already shaped into the anchor: only the
                // outgoing handle follows. Held from the start, the incoming handle stays retracted.
                let in_h = if alt { self.in_h } else { a - (out_h - a) };
                self.in_h = in_h;
                vec![Action::Preview(
                    "path.appendAnchor".into(),
                    json!({"id": id.0, "x": a.x, "y": a.y, "in": [in_h.x, in_h.y], "out": [out_h.x, out_h.y]}),
                )]
            }
            PointerKind::Up => {
                if self.handle.take().is_some() {
                    return vec![Action::Commit];
                }
                let Some((_, closing)) = self.drag.take() else { return vec![] };
                if closing {
                    self.drawing = false;
                }
                vec![Action::Commit]
            }
            PointerKind::DoubleClick => vec![],
        }
    }
    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _m: Mods) -> Vec<Action> {
        match key {
            ToolKey::Enter | ToolKey::Escape => {
                self.drawing = false;
                self.drag = None;
                self.handle = None;
                vec![]
            }
            _ => vec![],
        }
    }
    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        self.drawing = false;
        vec![]
    }
    fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        if !cx.pen_rubber_band || !self.drawing || self.drag.is_some() || self.handle.is_some() {
            return vec![];
        }
        let (Some((id, _, last, out)), Some(h)) = (active_path(cx), self.hover) else { return vec![] };
        let mut bp = BezPath::new();
        bp.move_to(last);
        if out.distance(last) > 1e-9 {
            bp.quad_to(out, h);
        } else {
            bp.line_to(h);
        }
        let c = cx.doc.layer_color(id);
        vec![Overlay::Path { path: bp, color: c, width: 1.0, dashed: false }]
    }
    fn cursor(&self, cx: &ToolContext, p: Point, m: Mods) -> Cursor {
        if !self.drawing {
            match spine_click(cx, p, cx.tol(5.0)).as_deref() {
                Some([Action::Exec(c, _)]) if c == "object.blend.spine.removeAnchor" => return Cursor::PenDelete,
                Some([_]) => return Cursor::PenAdd,
                _ => {}
            }
        }
        if let Some((id, first, last, _)) = active_path(cx) {
            let one = last_anchor(cx, id).is_some_and(|(_, ai)| ai == 0);
            if self.drawing && p.distance(first) <= cx.tol(5.0) && !one {
                return Cursor::PenClose;
            }
            if self.drawing && p.distance(last) <= cx.tol(5.0) {
                return Cursor::PenConvert;
            }
            if !self.drawing && (p.distance(last) <= cx.tol(5.0) || p.distance(first) <= cx.tol(5.0)) {
                return Cursor::PenContinue;
            }
        }
        if !self.drawing {
            match auto_add_delete(cx, p, m, cx.tol(5.0)) {
                Some(Action::Exec(c, _)) if c == "path.removeAnchor" => return Cursor::PenDelete,
                Some(_) => return Cursor::PenAdd,
                None => {}
            }
        }
        Cursor::Pen
    }
}

/// Auto Add/Delete: what a click at `p` does to a selected path. On one of its anchors it deletes
/// it; else, where one of its segments passes within `tol`, it adds an anchor. None when the
/// preference turns it off, Shift is held, or no selected path is there.
fn auto_add_delete(cx: &ToolContext, p: Point, m: Mods, tol: f64) -> Option<Action> {
    if !cx.auto_add_delete || m.shift {
        return None;
    }
    let paths = || {
        cx.selection
            .objects
            .iter()
            .copied()
            .filter(|&id| cx.doc.is_editable(id) && cx.doc.node(id).is_some_and(|n| matches!(n.kind, NodeKind::Path { guide: false, .. })))
    };
    let anchor = crate::draw2::anchor_in(cx, paths(), p, tol).map(crate::draw2::remove_anchor);
    anchor.or_else(|| crate::draw2::segment_in(cx, paths(), p, tol).map(crate::draw2::insert_anchor))
}

/// A click on a selected blend's spine: delete the point under `p` when no key object sits on it
/// (a key's point does nothing), else add one where the spine passes within `tol`.
fn spine_click(cx: &ToolContext, p: Point, tol: f64) -> Option<Vec<Action>> {
    for (id, path) in crate::direct::spines(cx).into_iter().filter(|(id, _)| cx.selection.contains(*id)) {
        let Some(NodeKind::Blend { children, spec }) = cx.doc.node(id).map(|n| &n.kind) else { continue };
        let keys = vectorcraft_doc::live::blend_spine(children, spec).and_then(|(_, a)| a).unwrap_or_default();
        if let Some((_, ai, _)) = path.anchors().find(|(_, _, a)| a.p.distance(p) <= tol) {
            if keys.contains(&ai) {
                return Some(vec![]);
            }
            return Some(vec![Action::Exec("object.blend.spine.removeAnchor".into(), json!({"id": id.0, "anchor": ai}))]);
        }
        if path.nearest(p).is_some_and(|n| n.4 <= tol) {
            return Some(vec![Action::Exec("object.blend.spine.addAnchor".into(), json!({"id": id.0, "x": p.x, "y": p.y}))]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;
    use vectorcraft_doc::Selection;

    #[test]
    fn first_click_creates_path() {
        let (d, _) = doc_with_rect();
        let s = Selection::default();
        let p = paint();
        let cx = cx(&d, &s, &p);
        let mut t = PenTool::default();
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 10.0, 10.0));
        assert_eq!(a[0], Action::Begin("Pen".into()));
        assert!(matches!(&a[1], Action::Preview(c, _) if c == "path.create"));
        assert_eq!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, 10.0, 10.0)), vec![Action::Commit]);
    }

    #[test]
    fn alt_pressed_mid_drag_keeps_the_incoming_handle() {
        let (mut d, _) = doc_with_rect();
        let l = d.layers[0].id;
        let id = d.alloc_id();
        let line = vectorcraft_geom::shapes::line(Point::new(10.0, 300.0), Point::new(60.0, 300.0));
        d.insert(Some(l), 1, vectorcraft_doc::Node::path(id, line, vectorcraft_doc::Appearance::default_art())).unwrap();
        let mut s = Selection::default();
        s.set([id]);
        let p = paint();
        let cx = cx(&d, &s, &p);
        let mut t = PenTool { drawing: true, ..PenTool::default() };
        let alt = Mods { alt: true, ..Mods::default() };
        let handles = |acts: Vec<Action>| match acts.as_slice() {
            [Action::Preview(c, v)] if c == "path.appendAnchor" => (v["in"].clone(), v["out"].clone()),
            other => panic!("not an anchor preview: {other:?}"),
        };
        t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 400.0, 420.0));
        let drag = PointerEvent::new(PointerKind::Drag, 450.0, 420.0);
        assert_eq!(handles(t.pointer(&cx, &drag)), (json!([350.0, 420.0]), json!([450.0, 420.0])));
        // Alt goes down: the curve shaped so far stays, and only the outgoing handle moves on.
        let drag = PointerEvent::new(PointerKind::Drag, 450.0, 470.0).with_mods(alt);
        assert_eq!(handles(t.pointer(&cx, &drag)), (json!([350.0, 420.0]), json!([450.0, 470.0])));
        let drag = PointerEvent::new(PointerKind::Drag, 400.0, 480.0).with_mods(alt);
        assert_eq!(handles(t.pointer(&cx, &drag)), (json!([350.0, 420.0]), json!([400.0, 480.0])));
        assert_eq!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, 400.0, 480.0).with_mods(alt)), vec![Action::Commit]);
        // Alt held from the start: the new anchor gets an outgoing handle only.
        t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 300.0, 440.0).with_mods(alt));
        let drag = PointerEvent::new(PointerKind::Drag, 340.0, 440.0).with_mods(alt);
        assert_eq!(handles(t.pointer(&cx, &drag)), (json!([300.0, 440.0]), json!([340.0, 440.0])));
    }

    #[test]
    fn space_moves_the_anchor_being_placed() {
        let (mut d, _) = doc_with_rect();
        let space = Mods { space: true, ..Mods::default() };
        // The first anchor of a new path.
        let (s, p) = (Selection::default(), paint());
        let cx0 = cx(&d, &s, &p);
        let mut t = PenTool::default();
        t.pointer(&cx0, &PointerEvent::new(PointerKind::Down, 400.0, 420.0));
        t.pointer(&cx0, &PointerEvent::new(PointerKind::Drag, 450.0, 420.0));
        let created = |acts: Vec<Action>| match acts.as_slice() {
            [Action::Preview(c, v)] if c == "path.create" => v["anchors"][0].clone(),
            other => panic!("not a create preview: {other:?}"),
        };
        let a = created(t.pointer(&cx0, &PointerEvent::new(PointerKind::Drag, 450.0, 380.0).with_mods(space)));
        assert_eq!(a, json!({"x": 400.0, "y": 380.0, "out": [450.0, 380.0], "in": [350.0, 380.0]}), "moved, handles and all");
        // Space released: the drag shapes the handles round the anchor's new place.
        let a = created(t.pointer(&cx0, &PointerEvent::new(PointerKind::Drag, 460.0, 380.0)));
        assert_eq!(a, json!({"x": 400.0, "y": 380.0, "out": [460.0, 380.0], "in": [340.0, 380.0]}));
        // An anchor added to an open path, its handles broken by Alt: the kept incoming handle
        // moves with it.
        let l = d.layers[0].id;
        let id = d.alloc_id();
        let line = vectorcraft_geom::shapes::line(Point::new(10.0, 300.0), Point::new(60.0, 300.0));
        d.insert(Some(l), 1, vectorcraft_doc::Node::path(id, line, vectorcraft_doc::Appearance::default_art())).unwrap();
        let mut s = Selection::default();
        s.set([id]);
        let cx = cx(&d, &s, &p);
        let mut t = PenTool { drawing: true, ..PenTool::default() };
        let alt = Mods { alt: true, ..Mods::default() };
        t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 400.0, 420.0));
        t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 450.0, 420.0));
        t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 450.0, 470.0).with_mods(alt));
        let acts = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 450.0, 450.0).with_mods(Mods { space: true, ..alt }));
        assert_eq!(
            acts,
            vec![Action::Preview(
                "path.appendAnchor".into(),
                json!({"id": id.0, "x": 400.0, "y": 400.0, "in": [350.0, 400.0], "out": [450.0, 450.0]})
            )]
        );
    }

    /// A document with an open path of `anchors`, selected, being drawn by a Pen.
    fn drawing(anchors: vectorcraft_geom::SubPath) -> (vectorcraft_doc::Document, NodeId, Selection) {
        let (mut d, _) = doc_with_rect();
        let l = d.layers[0].id;
        let id = d.alloc_id();
        let path = vectorcraft_geom::PathData::single(anchors);
        d.insert(Some(l), 1, vectorcraft_doc::Node::path(id, path, vectorcraft_doc::Appearance::default_art())).unwrap();
        let mut s = Selection::default();
        s.set([id]);
        (d, id, s)
    }

    fn set_out(id: NodeId, ai: usize, x: f64, y: f64) -> Action {
        Action::Preview("path.setHandle".into(), json!({"id": id.0, "subpath": 0, "anchor": ai, "which": "out", "x": x, "y": y, "independent": true}))
    }

    #[test]
    fn clicking_the_last_anchor_retracts_its_handle_and_dragging_pulls_a_new_one() {
        // The last anchor is smooth: handles at 40 and 80 either side of (60, 300).
        let mut sp = vectorcraft_geom::SubPath::polyline(&[Point::new(10.0, 300.0), Point::new(60.0, 300.0)], false);
        sp.anchors[1].h_in = Point::new(40.0, 300.0);
        sp.anchors[1].h_out = Point::new(80.0, 300.0);
        let (d, id, s) = drawing(sp);
        let p = paint();
        let cx = cx(&d, &s, &p);
        let mut t = PenTool { drawing: true, ..PenTool::default() };
        assert_eq!(t.cursor(&cx, Point::new(61.0, 301.0), Mods::default()), Cursor::PenConvert);
        // A click retracts the outgoing handle and adds no anchor.
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 61.0, 301.0));
        assert_eq!(a, vec![Action::Begin("Convert Anchor Point".into()), set_out(id, 1, 60.0, 300.0)]);
        assert_eq!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, 61.0, 301.0)), vec![Action::Commit]);
        // A drag pulls a new outgoing handle out.
        t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 60.0, 300.0));
        assert_eq!(t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 70.0, 280.0)), vec![set_out(id, 1, 70.0, 280.0)]);
        assert_eq!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, 70.0, 280.0)), vec![Action::Commit]);
        // Elsewhere a click still adds an anchor, and the first anchor still closes the path.
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 200.0, 400.0));
        assert!(matches!(&a[..], [Action::Begin(_), Action::Preview(c, _)] if c == "path.appendAnchor"), "{a:?}");
        t.pointer(&cx, &PointerEvent::new(PointerKind::Up, 200.0, 400.0));
        assert_eq!(t.cursor(&cx, Point::new(10.0, 300.0), Mods::default()), Cursor::PenClose);
    }

    #[test]
    fn a_one_anchor_path_does_not_close_on_itself() {
        let (d, id, s) = drawing(vectorcraft_geom::SubPath::polyline(&[Point::new(10.0, 300.0)], false));
        let p = paint();
        let cx = cx(&d, &s, &p);
        let mut t = PenTool { drawing: true, ..PenTool::default() };
        assert_eq!(t.cursor(&cx, Point::new(10.0, 300.0), Mods::default()), Cursor::PenConvert);
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 10.0, 300.0));
        assert_eq!(a, vec![Action::Begin("Convert Anchor Point".into()), set_out(id, 0, 10.0, 300.0)]);
    }

    #[test]
    fn clicking_a_selected_blend_spine_adds_a_point() {
        let (mut d, _) = doc_with_rect();
        let l = d.layers[0].id;
        let key = |id: NodeId, x: f64| {
            let r = vectorcraft_geom::shapes::rectangle(vectorcraft_geom::Rect::new(x, 300.0, x + 20.0, 320.0));
            std::sync::Arc::new(vectorcraft_doc::Node::path(id, r, vectorcraft_doc::Appearance::default_art()))
        };
        let (g, k1, k2) = (d.alloc_id(), d.alloc_id(), d.alloc_id());
        d.insert(
            Some(l),
            1,
            vectorcraft_doc::Node::new(g, NodeKind::Blend { children: vec![key(k1, 100.0), key(k2, 200.0)], spec: Default::default() }),
        )
        .unwrap();
        let mut s = Selection::default();
        s.set([g]);
        let p = paint();
        let cx = cx(&d, &s, &p);
        let mut t = PenTool::default();
        assert_eq!(t.cursor(&cx, Point::new(160.0, 311.0), Mods::default()), Cursor::PenAdd);
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 160.0, 311.0));
        assert_eq!(a, vec![Action::Exec("object.blend.spine.addAnchor".into(), json!({"id": g.0, "x": 160.0, "y": 310.0}))]);
        // A key's own point is left alone (the click snaps to the spine line).
        assert_eq!(t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 110.0, 310.0)), vec![]);
    }

    fn click(t: &mut PenTool, cx: &ToolContext, x: f64, y: f64, m: Mods) -> Vec<Action> {
        let acts = t.pointer(cx, &PointerEvent::new(PointerKind::Down, x, y).with_mods(m));
        t.pointer(cx, &PointerEvent::new(PointerKind::Up, x, y).with_mods(m));
        acts
    }

    #[test]
    fn clicking_a_selected_path_adds_or_deletes_an_anchor() {
        let (mut d, _) = doc_with_rect();
        let l = d.layers[0].id;
        let id = d.alloc_id();
        let pts = [Point::new(300.0, 400.0), Point::new(350.0, 300.0), Point::new(400.0, 400.0)];
        let path = vectorcraft_geom::PathData::single(vectorcraft_geom::SubPath::polyline(&pts, false));
        d.insert(Some(l), 1, vectorcraft_doc::Node::path(id, path, vectorcraft_doc::Appearance::default_art())).unwrap();
        let mut s = Selection::default();
        s.set([id]);
        let p = paint();
        let off = ToolContext { auto_add_delete: false, ..cx(&d, &s, &p) };
        let cx = cx(&d, &s, &p);
        let none = Mods::default();
        let mut t = PenTool::default();
        // On a segment: add an anchor there.
        assert_eq!(t.cursor(&cx, Point::new(325.0, 351.0), none), Cursor::PenAdd);
        let a = click(&mut t, &cx, 325.0, 351.0, none);
        assert!(matches!(&a[..], [Action::Exec(c, v)] if c == "path.insertAnchor" && v["id"] == id.0 && v["segment"] == 0), "{a:?}");
        // On an anchor: delete it.
        assert_eq!(t.cursor(&cx, Point::new(350.0, 302.0), none), Cursor::PenDelete);
        let a = click(&mut t, &cx, 350.0, 302.0, none);
        assert_eq!(a, vec![Action::Exec("path.removeAnchor".into(), json!({"id": id.0, "subpath": 0, "anchor": 1}))]);
        // An end still continues the path.
        assert_eq!(t.cursor(&cx, Point::new(400.0, 400.0), none), Cursor::PenContinue);
        // Shift, or the preference turned off, starts a new path instead.
        let shift = Mods { shift: true, ..Mods::default() };
        assert_eq!(t.cursor(&cx, Point::new(325.0, 351.0), shift), Cursor::Pen);
        assert_eq!(t.cursor(&off, Point::new(350.0, 302.0), none), Cursor::Pen);
        let a = click(&mut t, &off, 350.0, 302.0, none);
        assert!(matches!(&a[..], [Action::Begin(_), Action::Preview(c, _)] if c == "path.create"), "{a:?}");
        // While a path is being drawn, a click goes on drawing it.
        let a = click(&mut t, &cx, 325.0, 351.0, none);
        assert!(matches!(&a[..], [Action::Begin(_), Action::Preview(c, _)] if c == "path.appendAnchor"), "{a:?}");
        let mut t = PenTool::default();
        let a = click(&mut t, &cx, 325.0, 351.0, shift);
        assert!(matches!(&a[..], [Action::Begin(_), Action::Preview(c, _)] if c == "path.create"), "{a:?}");
    }
}
