//! Drawing / path-editing tools driven through the session (tools → actions → `path.*` commands).

use serde_json::json;
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_geom::{FillRule, PathData, Point};
use vectorcraft_tools::{Mods, PointerEvent, PointerKind, ToolKey};

use super::*;
use crate::tooling::{UiRequest, ViewInfo};

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
    s
}

fn view() -> ViewInfo {
    ViewInfo { smart_guides: false, ..Default::default() }
}

fn rect(s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> NodeId {
    let r = s.execute("shape.rectangle", &json!({"x": x, "y": y, "width": w, "height": h})).unwrap();
    NodeId(r["id"].as_u64().unwrap())
}

fn line(s: &mut Session, x1: f64, y1: f64, x2: f64, y2: f64) -> NodeId {
    let r = s.execute("shape.line", &json!({"x1": x1, "y1": y1, "x2": x2, "y2": y2})).unwrap();
    NodeId(r["id"].as_u64().unwrap())
}

fn paths(s: &Session) -> Vec<(NodeId, PathData)> {
    let mut v = vec![];
    s.doc().unwrap().doc.walk(|n| {
        if let NodeKind::Path { path, .. } = &n.kind {
            v.push((n.id, path.clone()));
        }
    });
    v
}

fn path(s: &Session, id: NodeId) -> PathData {
    s.doc().unwrap().doc.node(id).unwrap().path_data().unwrap().clone()
}

fn drag(s: &mut Session, tool: &str, pts: &[(f64, f64)], up_mods: Mods) -> Vec<UiRequest> {
    let v = view();
    s.select_tool(tool, v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Down, pts[0].0, pts[0].1), v).unwrap();
    for &(x, y) in &pts[1..] {
        s.pointer(&PointerEvent::new(PointerKind::Drag, x, y), v).unwrap();
    }
    let l = pts[pts.len() - 1];
    s.pointer(&PointerEvent::new(PointerKind::Up, l.0, l.1).with_mods(up_mods), v).unwrap()
}

fn click(s: &mut Session, tool: &str, x: f64, y: f64) -> Vec<UiRequest> {
    drag(s, tool, &[(x, y)], Mods::default())
}

/// A wavy stroke sampled every 2 pt.
fn wave(x0: f64, x1: f64, y: f64) -> Vec<(f64, f64)> {
    let n = ((x1 - x0) / 2.0) as usize;
    (0..=n).map(|i| x0 + i as f64 * 2.0).map(|x| (x, y + 20.0 * ((x - x0) / 40.0).sin())).collect()
}

fn near(a: Point, b: Point) -> bool {
    a.distance(b) < 1e-6
}

fn area(pd: &PathData) -> f64 {
    vectorcraft_pathops::area(pd, FillRule::NonZero)
}

#[test]
fn pencil_fits_freehand_stroke_as_one_undo_step() {
    let mut s = session();
    let pts = wave(100.0, 400.0, 200.0);
    let undo_before = s.doc().unwrap().history.undo.len();
    drag(&mut s, "pencil", &pts, Mods::default());
    let ps = paths(&s);
    assert_eq!(ps.len(), 1);
    let p = &ps[0].1;
    assert!(!p.is_closed());
    assert!(p.anchor_count() >= 2 && p.anchor_count() < pts.len() / 4, "{} anchors", p.anchor_count());
    let first = p.subpaths[0].anchors[0].p;
    assert!(first.distance(Point::new(100.0, 200.0)) < 1e-6);
    assert_eq!(s.doc().unwrap().history.undo.len(), undo_before + 1);
    // Unfilled, stroked.
    let n = s.doc().unwrap().doc.node(ps[0].0).unwrap().clone();
    assert!(n.appearance.fill_paint().is_none());
    assert!(!n.appearance.stroke_paint().is_none());
}

#[test]
fn pencil_alt_closes_path() {
    let mut s = session();
    let mut pts: Vec<(f64, f64)> =
        (0..=36).map(|i| (i as f64 * 10.0f64).to_radians()).map(|a| (300.0 + 80.0 * a.cos(), 300.0 + 80.0 * a.sin())).collect();
    pts.pop();
    drag(&mut s, "pencil", &pts, Mods { alt: true, ..Default::default() });
    let ps = paths(&s);
    assert!(ps[0].1.is_closed());
}

#[test]
fn pencil_continues_selected_open_path() {
    let mut s = session();
    let id = line(&mut s, 100.0, 100.0, 200.0, 100.0);
    drag(&mut s, "pencil", &[(202.0, 101.0), (230.0, 120.0), (260.0, 150.0), (300.0, 160.0)], Mods::default());
    let ps = paths(&s);
    assert_eq!(ps.len(), 1, "no new object");
    let p = path(&s, id);
    assert!(p.anchor_count() >= 3);
    let last = p.subpaths[0].anchors.last().unwrap().p;
    assert!(last.distance(Point::new(300.0, 160.0)) < 1e-6);
    assert_eq!(p.subpaths[0].anchors[0].p, Point::new(100.0, 100.0));
}

#[test]
fn paintbrush_uses_stroke_blob_brush_fills() {
    let mut s = session();
    drag(&mut s, "paintbrush", &wave(50.0, 200.0, 100.0), Mods::default());
    let n = s.doc().unwrap().doc.node(paths(&s)[0].0).unwrap().clone();
    assert!(n.appearance.fill_paint().is_none());
    s.execute("select.none", &json!({})).unwrap();
    drag(&mut s, "blobBrush", &[(100.0, 400.0), (200.0, 400.0)], Mods::default());
    let ps = paths(&s);
    assert_eq!(ps.len(), 2);
    let blob = &ps[1].1;
    assert!(blob.is_closed());
    // 100 × 10 capsule ≈ 1000 + π·25.
    assert!((area(blob) - (1000.0 + std::f64::consts::PI * 25.0)).abs() < 10.0, "{}", area(blob));
    let bn = s.doc().unwrap().doc.node(ps[1].0).unwrap().clone();
    assert!(bn.appearance.stroke_paint().is_none() && !bn.appearance.fill_paint().is_none());
    // A second overlapping stroke of the same colour merges.
    drag(&mut s, "blobBrush", &[(150.0, 380.0), (150.0, 450.0)], Mods::default());
    let ps = paths(&s);
    assert_eq!(ps.len(), 2, "merged into the first blob");
    assert!(area(&ps[1].1) > 1500.0);
}

#[test]
fn curvature_clicks_build_smooth_path() {
    let mut s = session();
    let v = view();
    s.select_tool("curvature", v).unwrap();
    for (x, y) in [(100.0, 300.0), (200.0, 200.0), (300.0, 300.0)] {
        s.pointer(&PointerEvent::new(PointerKind::Down, x, y), v).unwrap();
        s.pointer(&PointerEvent::new(PointerKind::Up, x, y), v).unwrap();
    }
    let ps = paths(&s);
    assert_eq!(ps.len(), 1);
    let sp = &ps[0].1.subpaths[0];
    assert_eq!(sp.anchors.len(), 3);
    assert_eq!(sp.anchors[1].kind, vectorcraft_geom::AnchorKind::Smooth);
    // Alt-click the middle point → corner.
    s.pointer(&PointerEvent::new(PointerKind::Down, 200.0, 200.0).with_mods(Mods { alt: true, ..Default::default() }), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, 200.0, 200.0), v).unwrap();
    assert!(!path(&s, ps[0].0).subpaths[0].anchors[1].has_out());
    // Drag the middle point.
    s.pointer(&PointerEvent::new(PointerKind::Down, 200.0, 200.0), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Drag, 200.0, 150.0), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, 200.0, 150.0), v).unwrap();
    assert_eq!(path(&s, ps[0].0).subpaths[0].anchors[1].p, Point::new(200.0, 150.0));
    // Esc ends; the next click starts a new path.
    s.tool_key(ToolKey::Escape, Mods::default(), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Down, 500.0, 500.0), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, 500.0, 500.0), v).unwrap();
    assert_eq!(paths(&s).len(), 2);
}

#[test]
fn curvature_click_first_point_closes() {
    let mut s = session();
    let v = view();
    s.select_tool("curvature", v).unwrap();
    for (x, y) in [(100.0, 100.0), (200.0, 100.0), (150.0, 200.0), (100.0, 100.0)] {
        s.pointer(&PointerEvent::new(PointerKind::Down, x, y), v).unwrap();
        s.pointer(&PointerEvent::new(PointerKind::Up, x, y), v).unwrap();
    }
    let p = &paths(&s)[0].1;
    assert!(p.is_closed());
    assert_eq!(p.anchor_count(), 3);
}

#[test]
fn add_and_delete_anchor_tools() {
    let mut s = session();
    let id = rect(&mut s, 100.0, 100.0, 100.0, 100.0);
    click(&mut s, "addAnchor", 150.0, 100.0);
    let p = path(&s, id);
    assert_eq!(p.anchor_count(), 5);
    assert!(near(p.subpaths[0].anchors[1].p, Point::new(150.0, 100.0)));
    click(&mut s, "deleteAnchor", 150.0, 100.0);
    click(&mut s, "deleteAnchor", 200.0, 200.0);
    let p = path(&s, id);
    assert_eq!(p.anchor_count(), 3);
    assert!(p.is_closed());
}

#[test]
fn delete_anchor_on_circle_keeps_curve() {
    let mut s = session();
    let r = s.execute("shape.ellipse", &json!({"x": 100, "y": 100, "width": 200, "height": 200})).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    let before = path(&s, id);
    let a = before.subpaths[0].anchors[1].p;
    click(&mut s, "deleteAnchor", a.x, a.y);
    let p = path(&s, id);
    assert_eq!(p.anchor_count(), before.anchor_count() - 1);
    // The remaining curve still bulges out towards the removed anchor.
    let b = p.bounds().unwrap();
    assert!(b.width() > 150.0 && b.height() > 150.0, "{b:?}");
}

#[test]
fn anchor_point_tool_converts() {
    let mut s = session();
    let id = rect(&mut s, 100.0, 100.0, 100.0, 100.0);
    drag(&mut s, "anchorPoint", &[(100.0, 100.0), (130.0, 80.0)], Mods::default());
    let a = path(&s, id).subpaths[0].anchors[0];
    assert_eq!(a.kind, vectorcraft_geom::AnchorKind::Smooth);
    assert_eq!(a.h_out, Point::new(130.0, 80.0));
    assert_eq!(a.h_in, Point::new(70.0, 120.0));
    // Click the smooth anchor → corner again.
    click(&mut s, "anchorPoint", 100.0, 100.0);
    let a = path(&s, id).subpaths[0].anchors[0];
    assert!(!a.has_in() && !a.has_out());
    // Drag a segment → it bends so the grabbed point follows.
    drag(&mut s, "anchorPoint", &[(150.0, 200.0), (150.0, 240.0)], Mods::default());
    let p = path(&s, id);
    let (_, _, _, q, d) = p.nearest(Point::new(150.0, 240.0)).unwrap();
    assert!(d < 0.5, "{q:?}");
}

#[test]
fn scissors_opens_closed_path() {
    let mut s = session();
    let id = rect(&mut s, 100.0, 100.0, 100.0, 100.0);
    click(&mut s, "scissors", 200.0, 150.0);
    let p = path(&s, id);
    assert!(!p.is_closed());
    assert_eq!(p.anchor_count(), 6);
    assert!(near(p.subpaths[0].anchors[0].p, Point::new(200.0, 150.0)));
    assert!(near(p.subpaths[0].anchors[5].p, Point::new(200.0, 150.0)));
}

#[test]
fn scissors_splits_open_path_in_two() {
    let mut s = session();
    let id = line(&mut s, 100.0, 100.0, 300.0, 100.0);
    click(&mut s, "scissors", 150.0, 100.0);
    let ps = paths(&s);
    assert_eq!(ps.len(), 2);
    assert!(near(path(&s, id).subpaths[0].anchors[1].p, Point::new(150.0, 100.0)));
    assert!(near(ps[1].1.subpaths[0].anchors[0].p, Point::new(150.0, 100.0)));
    assert_eq!(ps[1].1.subpaths[0].anchors[1].p, Point::new(300.0, 100.0));
    assert_eq!(s.doc().unwrap().selection.objects.len(), 2);
    // Clicking an end point is refused without panicking.
    assert!(s.execute("path.split", &json!({"id": id.0, "subpath": 0, "anchor": 0})).is_err());
}

#[test]
fn knife_cuts_rect_into_two_pieces() {
    let mut s = session();
    rect(&mut s, 100.0, 100.0, 200.0, 100.0);
    s.execute("select.none", &json!({})).unwrap();
    let undo = s.doc().unwrap().history.undo.len();
    drag(&mut s, "knife", &[(200.0, 50.0), (210.0, 150.0), (200.0, 250.0)], Mods::default());
    let ps = paths(&s);
    assert_eq!(ps.len(), 2);
    let total: f64 = ps.iter().map(|(_, p)| area(p)).sum();
    assert!((total - 20000.0).abs() < 50.0, "{total}");
    assert!(ps.iter().all(|(_, p)| p.is_closed() && area(p) > 9000.0));
    assert_eq!(s.doc().unwrap().history.undo.len(), undo + 1);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(paths(&s).len(), 1);
}

#[test]
fn knife_partial_cut_does_nothing() {
    let mut s = session();
    let id = rect(&mut s, 100.0, 100.0, 200.0, 100.0);
    s.execute("select.none", &json!({})).unwrap();
    let r = s.execute("path.knife", &json!({"points": [[200, 50], [200, 150]]})).unwrap();
    assert_eq!(r["ids"], json!([]));
    assert_eq!(path(&s, id).anchor_count(), 4);
}

#[test]
fn eraser_splits_shapes_and_lines() {
    let mut s = session();
    rect(&mut s, 100.0, 100.0, 200.0, 100.0);
    line(&mut s, 100.0, 300.0, 300.0, 300.0);
    s.execute("select.none", &json!({})).unwrap();
    drag(&mut s, "eraser", &[(200.0, 50.0), (200.0, 350.0)], Mods::default());
    let ps = paths(&s);
    assert_eq!(ps.len(), 4, "two rect halves + two line pieces");
    let closed: Vec<&PathData> = ps.iter().map(|p| &p.1).filter(|p| p.is_closed()).collect();
    assert_eq!(closed.len(), 2);
    let total: f64 = closed.iter().map(|p| area(p)).sum();
    assert!((total - (20000.0 - 1000.0)).abs() < 50.0, "{total}");
    let open: Vec<&PathData> = ps.iter().map(|p| &p.1).filter(|p| !p.is_closed()).collect();
    let len: f64 = open.iter().map(|p| p.length()).sum();
    assert!((len - 190.0).abs() < 0.1, "{len}");
}

#[test]
fn eraser_removes_small_shape_entirely() {
    let mut s = session();
    rect(&mut s, 100.0, 100.0, 4.0, 4.0);
    s.execute("select.none", &json!({})).unwrap();
    s.execute("path.eraseRegion", &json!({"points": [[102, 102]], "size": 20})).unwrap();
    assert!(paths(&s).is_empty());
}

#[test]
fn path_eraser_splits_selected_path() {
    let mut s = session();
    let id = line(&mut s, 100.0, 100.0, 300.0, 100.0);
    drag(&mut s, "pathEraser", &[(180.0, 100.0), (220.0, 100.0)], Mods::default());
    let ps = paths(&s);
    assert_eq!(ps.len(), 2);
    assert!(path(&s, id).subpaths[0].anchors[1].p.x < 180.0);
}

#[test]
fn smooth_tool_reduces_jagged_path() {
    let mut s = session();
    let anchors: Vec<_> = (0..=40).map(|i| json!({"x": 100.0 + i as f64 * 5.0, "y": 200.0 + if i % 2 == 0 { 0.0 } else { 1.5 }})).collect();
    let r = s.execute("path.create", &json!({"anchors": anchors})).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    drag(&mut s, "smooth", &[(90.0, 200.0), (200.0, 200.0), (310.0, 200.0)], Mods::default());
    let p = path(&s, id);
    assert!(p.anchor_count() < 20, "{}", p.anchor_count());
    assert_eq!(p.subpaths[0].anchors[0].p, Point::new(100.0, 200.0));
}

#[test]
fn join_tool_joins_two_lines() {
    let mut s = session();
    let a = line(&mut s, 100.0, 100.0, 200.0, 100.0);
    line(&mut s, 205.0, 100.0, 300.0, 150.0);
    s.execute("select.none", &json!({})).unwrap();
    drag(&mut s, "join", &[(202.0, 90.0), (202.0, 110.0)], Mods::default());
    let ps = paths(&s);
    assert_eq!(ps.len(), 1);
    let p = path(&s, a);
    assert_eq!(p.anchor_count(), 3);
    assert_eq!(p.subpaths[0].anchors[1].p, Point::new(202.5, 100.0));
    assert_eq!(p.subpaths[0].anchors[2].p, Point::new(300.0, 150.0));
}

#[test]
fn line_family_tools_draw_and_click_asks_dialog() {
    let mut s = session();
    drag(&mut s, "arc", &[(100.0, 100.0), (200.0, 150.0)], Mods::default());
    drag(&mut s, "spiral", &[(400.0, 300.0), (450.0, 300.0)], Mods::default());
    drag(&mut s, "rectangularGrid", &[(100.0, 300.0), (200.0, 400.0)], Mods::default());
    drag(&mut s, "polarGrid", &[(500.0, 100.0), (600.0, 200.0)], Mods::default());
    let top = s.doc().unwrap().doc.layers[0].children().unwrap().len();
    assert_eq!(top, 4);
    let b = paths(&s)[0].1.bounds().unwrap();
    assert!((b.width() - 100.0).abs() < 1e-6 && (b.height() - 50.0).abs() < 1e-6, "{b:?}");
    let ui = click(&mut s, "polarGrid", 10.0, 10.0);
    assert_eq!(ui, vec![UiRequest::Dialog("polarGrid".into(), json!({"x": 10.0, "y": 10.0}))]);
}

#[test]
fn draw2_commands_reject_bad_params() {
    let mut s = session();
    let id = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    for (cmd, p) in [
        ("path.freehand", json!({})),
        ("path.freehand", json!({"points": [[1, 2]]})),
        ("path.curvature", json!({"points": []})),
        ("path.removeAnchor", json!({"id": id.0, "anchor": 99})),
        ("path.removeAnchors", json!({})),
        ("path.convertAnchor", json!({"id": 9999, "anchor": 0, "to": "corner"})),
        ("path.reshapeSegment", json!({"id": id.0, "segment": 42})),
        ("path.split", json!({"id": id.0})),
        ("path.split", json!({"id": id.0, "segment": 0, "t": "x"})),
        ("path.knife", json!({"points": [[0, 0]]})),
        ("path.eraseRegion", json!({"points": [[0, 0]], "size": -1})),
        ("path.blob", json!({"points": "nope"})),
        ("path.joinScrub", json!({"points": []})),
    ] {
        let before = s.doc().unwrap().history.undo.len();
        assert!(s.execute(cmd, &p).is_err(), "{cmd} {p}");
        assert_eq!(s.doc().unwrap().history.undo.len(), before, "{cmd} left an undo step");
    }
}

/// A curve from (100, 100) to (300, 100) whose first anchor has an out handle at (150, 100).
fn curve(s: &mut Session) -> NodeId {
    let anchors = json!([{"x": 100, "y": 100, "out": [150, 100]}, {"x": 300, "y": 100, "in": [250, 100]}]);
    NodeId(s.execute("path.create", &json!({"anchors": anchors})).unwrap()["id"].as_u64().unwrap())
}

/// Drag the out handle of path `id`'s first anchor with `tool` to `to`, holding `mods`. Direct
/// Selection clicks the anchor first, so its handles show.
fn drag_handle(s: &mut Session, tool: &str, id: NodeId, to: (f64, f64), mods: Mods, v: ViewInfo) {
    let from = path(s, id).subpaths[0].anchors[0].h_out;
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    s.select_tool(tool, v).unwrap();
    if tool == "directSelection" {
        s.execute("select.none", &json!({})).unwrap();
        s.pointer(&PointerEvent::new(PointerKind::Down, 100.0, 100.0), v).unwrap();
        s.pointer(&PointerEvent::new(PointerKind::Up, 100.0, 100.0), v).unwrap();
    }
    s.pointer(&PointerEvent::new(PointerKind::Down, from.x, from.y).with_mods(mods), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Drag, to.0, to.1).with_mods(mods), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, to.0, to.1).with_mods(mods), v).unwrap();
    assert!(!s.in_interaction());
}

/// The out handle of path `id`'s first anchor.
fn out_handle(s: &Session, id: NodeId) -> Point {
    path(s, id).subpaths[0].anchors[0].h_out
}

#[test]
fn shift_keeps_a_dragged_handle_at_45_degree_steps() {
    // #322.
    let shift = Mods { shift: true, ..Mods::default() };
    for tool in ["directSelection", "anchorPoint"] {
        let mut s = session();
        let id = curve(&mut s);
        drag_handle(&mut s, tool, id, (190.0, 130.0), shift, view());
        let h = out_handle(&s, id);
        assert!(near(h, Point::new(100.0 + 90.0f64.hypot(30.0), 100.0)), "{tool}: {h:?}");
        drag_handle(&mut s, tool, id, (160.0, 155.0), shift, view());
        let h = out_handle(&s, id);
        assert!((h.x - h.y).abs() < 1e-6 && h.x > 100.0, "{tool}: 45°: {h:?}");
        // Without Shift the handle goes where it is dragged.
        drag_handle(&mut s, tool, id, (190.0, 130.0), Mods::default(), view());
        assert_eq!(out_handle(&s, id), Point::new(190.0, 130.0), "{tool}");
    }
}

#[test]
fn a_dragged_handle_snaps_to_smart_guides() {
    // #322: in line with the other anchor, with smart guides on.
    for tool in ["directSelection", "anchorPoint"] {
        let mut s = session();
        let id = curve(&mut s);
        drag_handle(&mut s, tool, id, (298.0, 160.0), Mods::default(), ViewInfo::default());
        assert_eq!(out_handle(&s, id), Point::new(300.0, 160.0), "{tool}");
        drag_handle(&mut s, tool, id, (298.0, 170.0), Mods::default(), view());
        assert_eq!(out_handle(&s, id), Point::new(298.0, 170.0), "{tool}: smart guides off");
    }
}

#[test]
fn shift_drag_keeps_a_smooth_anchor_smooth_and_alt_breaks_it() {
    // #322: the opposite handle of a smooth anchor turns with the constrained one; Alt still
    // moves the dragged handle alone.
    let shift = Mods { shift: true, ..Mods::default() };
    let shift_alt = Mods { shift: true, alt: true, ..Mods::default() };
    for (mods, opposite_follows) in [(shift, true), (shift_alt, false)] {
        let mut s = session();
        let anchors = json!([{"x": 100, "y": 200}, {"x": 200, "y": 100, "in": [150, 100], "out": [250, 100]}, {"x": 300, "y": 200}]);
        let id = NodeId(s.execute("path.create", &json!({"anchors": anchors})).unwrap()["id"].as_u64().unwrap());
        s.select_tool("directSelection", view()).unwrap();
        for (kind, x, y, m) in [
            (PointerKind::Down, 200.0, 100.0, Mods::default()),
            (PointerKind::Up, 200.0, 100.0, Mods::default()),
            (PointerKind::Down, 250.0, 100.0, mods),
            (PointerKind::Drag, 275.0, 165.0, mods),
            (PointerKind::Up, 275.0, 165.0, mods),
        ] {
            s.pointer(&PointerEvent::new(kind, x, y).with_mods(m), view()).unwrap();
        }
        let a = path(&s, id).subpaths[0].anchors[1];
        let (o, i) = (a.h_out - a.p, a.h_in - a.p);
        assert!((o.x - o.y).abs() < 1e-6 && o.x > 0.0, "{mods:?}: 45°: {o:?}");
        if opposite_follows {
            assert!(near(a.h_in, a.p - o.normalize() * 50.0), "{mods:?}: {i:?}");
        } else {
            assert_eq!(a.h_in, Point::new(150.0, 100.0), "{mods:?}");
        }
    }
}

#[test]
fn removing_an_anchor_refits_a_split_curve_and_undo_restores_it() {
    let mut s = session();
    let made = s
        .execute(
            "path.create",
            &json!({"anchors": [
                {"x": 0, "y": 0, "out": [30, 80]},
                {"x": 100, "y": 0, "in": [70, 80]}
            ]}),
        )
        .unwrap();
    let id = NodeId(made["id"].as_u64().unwrap());
    let before = path(&s, id);
    let inserted = s.execute("path.insertAnchor", &json!({"id": id.0, "segment": 0, "t": 0.4})).unwrap();
    let ai = inserted["anchor"].as_u64().unwrap();
    s.execute("path.removeAnchor", &json!({"id": id.0, "anchor": ai})).unwrap();
    let after = path(&s, id);
    assert_eq!(after.subpaths[0].anchors.len(), 2);
    let (got, orig) = (&after.subpaths[0].anchors, &before.subpaths[0].anchors);
    assert!(got[0].h_out.distance(orig[0].h_out) < 0.5, "{:?}", got[0].h_out);
    assert!(got[1].h_in.distance(orig[1].h_in) < 0.5, "{:?}", got[1].h_in);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(path(&s, id).subpaths[0].anchors.len(), 3);
}

#[test]
fn remove_anchor_points_keeps_the_paths_closed() {
    // Object › Path › Remove Anchor Points, unlike the Delete key: a rectangle loses a corner and
    // stays closed, a straight-sided triangle; a curve keeps its shape round a removed point.
    let mut s = session();
    let r = rect(&mut s, 0.0, 0.0, 100.0, 80.0);
    let made = s.execute("path.create", &json!({"anchors": [{"x": 300, "y": 0, "out": [330, 80]}, {"x": 400, "y": 0, "in": [370, 80]}]})).unwrap();
    let c = NodeId(made["id"].as_u64().unwrap());
    let before = path(&s, c).subpaths[0].segment(0);
    s.execute("path.insertAnchor", &json!({"id": c.0, "segment": 0, "t": 0.5})).unwrap();
    s.execute("select.anchors", &json!({"id": r.0, "anchors": [[0, 0]]})).unwrap();
    s.execute("select.anchors", &json!({"id": c.0, "anchors": [[0, 1]], "mode": "add"})).unwrap();
    assert_eq!(s.execute("path.removeAnchors", &json!({})).unwrap()["removedObjects"], 0);
    let sp = &path(&s, r).subpaths[0];
    assert!(sp.closed && sp.anchors.len() == 3, "{sp:?}");
    assert!(!sp.anchors.iter().any(|a| a.has_in() || a.has_out()), "straight sides stay straight: {sp:?}");
    let after = path(&s, c).subpaths[0].segment(0);
    assert_eq!(path(&s, c).subpaths[0].anchors.len(), 2);
    assert!(after.p1.distance(before.p1) < 0.5 && after.p2.distance(before.p2) < 0.5, "{after:?}");
    assert_eq!(s.doc().unwrap().history.undo.last().unwrap().label, "Remove Anchor Points");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(path(&s, r).subpaths[0].anchors.len(), 4);
    assert_eq!(path(&s, c).subpaths[0].anchors.len(), 3);
    let spec = find_command("path.removeAnchors").unwrap();
    assert_eq!(spec.menu, &["Object", "Path"][..]);
}
