//! Resizing area type (#252): a bounding-box handle drag and Area Type Options size the type area
//! and the text reflows at its size; Direct Selection reshapes the frame by a corner or an edge.
//! Point type still scales.

use serde_json::{Value, json};
use vectorcraft_doc::{NodeKind, TextKind, TextObject};
use vectorcraft_geom::{Affine, Point, Rect};
use vectorcraft_text::TextLayout;
use vectorcraft_tools::bbox::Handle;
use vectorcraft_tools::{Mods, PointerEvent, PointerKind};

use super::*;

const STORY: &str = "The quick brown fox jumps over the lazy dog. A second sentence follows the first one here. Then a third one ends it.";

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
    s
}

fn id_of(v: &Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

/// 12 pt area type in a `w` × `h` frame at (`x`, `y`).
fn area(s: &mut Session, x: f64, y: f64, w: f64, h: f64, text: &str) -> NodeId {
    id_of(&s.execute("text.create", &json!({"x": x, "y": y, "size": 12, "area": {"width": w, "height": h}, "text": text})).unwrap())
}

fn sel(s: &mut Session, ids: &[NodeId]) {
    s.execute("select.set", &json!({"ids": ids.iter().map(|i| i.0).collect::<Vec<_>>()})).unwrap();
}

fn text(s: &Session, id: NodeId) -> TextObject {
    match &s.doc().unwrap().doc.node(id).unwrap().kind {
        NodeKind::Text(t) => (**t).clone(),
        k => panic!("not text: {k:?}"),
    }
}

/// The frame's anchors in text space.
fn corners(s: &Session, id: NodeId) -> Vec<Point> {
    match text(s, id).kind {
        TextKind::Area { frame } => frame.anchors().map(|(_, _, a)| a.p).collect(),
        k => panic!("not area type: {k:?}"),
    }
}

fn frame(s: &Session, id: NodeId) -> Rect {
    match text(s, id).kind {
        TextKind::Area { frame } => frame.bounds().unwrap(),
        k => panic!("not area type: {k:?}"),
    }
}

fn layout(s: &Session, id: NodeId) -> TextLayout {
    vectorcraft_text::layout(vectorcraft_text::FontDb::global(), &text(s, id))
}

fn undo_len(s: &Session) -> usize {
    s.doc().unwrap().history.undo.len()
}

fn last_step(s: &Session) -> String {
    s.doc().unwrap().history.undo.last().unwrap().label.clone()
}

/// Press, drag and release `tool` from `from` to `to`.
fn drag(s: &mut Session, tool: &str, from: Point, to: Point) {
    let v = ViewInfo::default();
    s.select_tool(tool, v).unwrap();
    for (kind, p) in [(PointerKind::Down, from), (PointerKind::Drag, to), (PointerKind::Up, to)] {
        s.pointer(&PointerEvent::new(kind, p.x, p.y).with_mods(Mods::default()), v).unwrap();
    }
    assert!(!s.in_interaction());
}

fn near(a: Point, b: Point) -> bool {
    a.distance(b) < 1e-6
}

#[test]
fn a_handle_drag_resizes_the_type_area_and_the_text_reflows() {
    let mut s = session();
    let id = area(&mut s, 40.0, 40.0, 120.0, 40.0, STORY);
    let before = layout(&s, id);
    assert!(before.overflow, "the story overflows 120 × 40");
    sel(&mut s, &[id]);
    let n = undo_len(&s);
    // The bounding box's bottom-right handle, from (160, 80) to (200, 140): 160 × 100.
    drag(&mut s, "selection", Point::new(160.0, 80.0), Point::new(200.0, 140.0));
    assert_eq!(undo_len(&s), n + 1, "one step");
    assert_eq!(last_step(&s), "Resize Type Area");
    let r = frame(&s, id);
    assert!(near(r.origin(), Point::ZERO) && (r.width() - 160.0).abs() < 1e-6 && (r.height() - 100.0).abs() < 1e-6, "{r:?}");
    let t = text(&s, id);
    assert_eq!(t.xf, Affine::translate((40.0, 40.0)), "the type isn't scaled");
    assert!(t.runs.iter().all(|r| r.style.size == 12.0));
    let after = layout(&s, id);
    assert!(after.lines.len() > before.lines.len(), "more lines fit: {} → {}", before.lines.len(), after.lines.len());
    assert!(after.glyphs.len() > before.glyphs.len());
    assert!(!after.overflow, "the whole story fits 160 × 100");
    // The bounding box hugs the new frame.
    assert_eq!(s.doc().unwrap().doc.node(id).unwrap().geometric_bounds(), Some(Rect::new(40.0, 40.0, 200.0, 140.0)));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(frame(&s, id), Rect::new(0.0, 0.0, 120.0, 40.0));
    assert!(layout(&s, id).overflow);
}

#[test]
fn point_type_still_scales_with_its_handles() {
    let mut s = session();
    let id = id_of(&s.execute("text.create", &json!({"x": 40, "y": 40, "size": 12, "text": "Hello"})).unwrap());
    sel(&mut s, &[id]);
    let b = s.transform_box(&s.doc().unwrap().selection.objects).unwrap();
    let (tl, br) = (b.to_doc() * Handle::TopLeft.pos(b.rect), b.to_doc() * Handle::BottomRight.pos(b.rect));
    drag(&mut s, "selection", br, tl + (br - tl) * 2.0);
    assert_eq!(last_step(&s), "Scale");
    let c = text(&s, id).xf.as_coeffs();
    assert!((c[0] - 2.0).abs() < 1e-6 && (c[3] - 2.0).abs() < 1e-6, "the type doubles: {c:?}");
}

#[test]
fn with_other_objects_the_type_area_resizes_and_the_others_scale() {
    let mut s = session();
    let id = area(&mut s, 40.0, 40.0, 120.0, 40.0, STORY);
    let r = id_of(&s.execute("shape.rectangle", &json!({"x": 40, "y": 100, "width": 120, "height": 20})).unwrap());
    sel(&mut s, &[id, r]);
    // The box spans (40, 40)–(160, 120); its right handle doubles the width.
    drag(&mut s, "selection", Point::new(160.0, 80.0), Point::new(280.0, 80.0));
    assert_eq!(last_step(&s), "Scale");
    assert_eq!(s.doc().unwrap().doc.node(r).unwrap().geometric_bounds(), Some(Rect::new(40.0, 100.0, 280.0, 120.0)));
    assert_eq!(frame(&s, id), Rect::new(0.0, 0.0, 240.0, 40.0));
    assert_eq!(text(&s, id).xf, Affine::translate((40.0, 40.0)));
}

#[test]
fn a_rotated_type_area_resizes_along_its_own_axes() {
    let mut s = session();
    let id = area(&mut s, 40.0, 40.0, 120.0, 40.0, STORY);
    sel(&mut s, &[id]);
    s.execute("object.rotate", &json!({"angle": 30, "absolute": true})).unwrap();
    let xf = text(&s, id).xf;
    let b = s.transform_box(&[id]).unwrap();
    let right = b.to_doc() * Handle::Right.pos(b.rect);
    let along = (b.to_doc() * Handle::Right.pos(b.rect)) - (b.to_doc() * Handle::Left.pos(b.rect));
    drag(&mut s, "selection", right, right + along * 0.5);
    assert_eq!(text(&s, id).xf, xf, "the type keeps its size and angle");
    let r = frame(&s, id);
    assert!((r.width() - 180.0).abs() < 1e-6 && (r.height() - 40.0).abs() < 1e-6, "still a rectangle, 180 × 40: {r:?}");
    assert_eq!(corners(&s, id).len(), 4);
}

#[test]
fn direct_selection_drags_a_frame_corner_or_edge() {
    let mut s = session();
    let id = area(&mut s, 40.0, 40.0, 120.0, 40.0, STORY);
    let glyphs = layout(&s, id).glyphs.len();
    let n = undo_len(&s);
    // The bottom-right corner, not selected yet: the corner moves, the others stay.
    drag(&mut s, "directSelection", Point::new(160.0, 80.0), Point::new(200.0, 140.0));
    assert_eq!(s.doc().unwrap().selection.objects, vec![id]);
    assert_eq!(undo_len(&s), n + 1, "one step");
    assert_eq!(last_step(&s), "Reshape Type Area");
    let c = corners(&s, id);
    assert!(near(c[0], Point::ZERO) && near(c[1], Point::new(120.0, 0.0)) && near(c[3], Point::new(0.0, 40.0)), "{c:?}");
    assert!(near(c[2], Point::new(160.0, 100.0)), "{c:?}");
    assert!(layout(&s, id).glyphs.len() > glyphs, "more text fits the larger area");
    assert_eq!(text(&s, id).xf, Affine::translate((40.0, 40.0)));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(frame(&s, id), Rect::new(0.0, 0.0, 120.0, 40.0));
    // The right edge: both its ends move, and the area stays a rectangle.
    drag(&mut s, "directSelection", Point::new(160.0, 60.0), Point::new(180.0, 60.0));
    assert_eq!(frame(&s, id), Rect::new(0.0, 0.0, 140.0, 40.0));
    assert_eq!(text(&s, id).runs[0].style.size, 12.0);
}

#[test]
fn a_resized_frame_reflows_its_thread() {
    let mut s = session();
    let a = area(&mut s, 40.0, 40.0, 120.0, 40.0, STORY);
    let b = area(&mut s, 300.0, 40.0, 120.0, 200.0, "");
    sel(&mut s, &[a, b]);
    s.execute("text.thread.create", &json!({})).unwrap();
    let rest = text(&s, b).plain_text().len();
    assert!(rest > 0, "the overflow flows on");
    // Bounding-box resize of the first frame to 160 × 100, as the Selection tool sends it.
    let m = Affine::translate((40.0, 40.0)) * Affine::scale_non_uniform(160.0 / 120.0, 100.0 / 40.0) * Affine::translate((-40.0, -40.0));
    let c = m.as_coeffs();
    s.execute("object.transform", &json!({"ids": [a.0], "matrix": c, "typeAreas": true})).unwrap();
    assert!(text(&s, b).plain_text().len() < rest, "the second frame holds less");
    assert_eq!(text(&s, a).plain_text().len() + text(&s, b).plain_text().len(), STORY.len());
}

#[test]
fn area_type_options_size_the_type_area() {
    let mut s = session();
    let id = area(&mut s, 40.0, 40.0, 120.0, 40.0, STORY);
    sel(&mut s, &[id]);
    let q = s.execute("text.areaOptions", &json!({})).unwrap();
    assert_eq!((q["width"].as_f64(), q["height"].as_f64()), (Some(120.0), Some(40.0)));
    let n = undo_len(&s);
    let r = s.execute("text.areaOptions", &json!({"width": 160, "height": 100})).unwrap();
    assert_eq!((r["width"].as_f64(), r["height"].as_f64()), (Some(160.0), Some(100.0)));
    assert_eq!(frame(&s, id), Rect::new(0.0, 0.0, 160.0, 100.0));
    assert_eq!(text(&s, id).xf, Affine::translate((40.0, 40.0)));
    assert!(!layout(&s, id).overflow);
    // The dialog's OK sends the size back unchanged with the other options: only those change.
    s.execute("text.areaOptions", &json!({"width": 160, "height": 100, "inset": 4})).unwrap();
    assert_eq!(frame(&s, id), Rect::new(0.0, 0.0, 160.0, 100.0));
    assert_eq!(undo_len(&s), n + 2);
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(frame(&s, id), Rect::new(0.0, 0.0, 120.0, 40.0));
    // Type scaled 200%: sizes are in points on the page, along the type's axes.
    s.execute("object.scale", &json!({"sx": 200, "origin": [40, 40]})).unwrap();
    assert_eq!(s.execute("text.areaOptions", &json!({})).unwrap()["width"].as_f64(), Some(240.0));
    s.execute("text.areaOptions", &json!({"width": 300})).unwrap();
    assert_eq!(frame(&s, id), Rect::new(0.0, 0.0, 150.0, 40.0));
}

#[test]
fn reshape_area_takes_untrusted_params() {
    let mut s = session();
    let id = area(&mut s, 40.0, 40.0, 120.0, 40.0, STORY);
    let point = id_of(&s.execute("text.create", &json!({"x": 40, "y": 300, "text": "Point"})).unwrap());
    let rect = id_of(&s.execute("shape.rectangle", &json!({"x": 300, "y": 300, "width": 50, "height": 50})).unwrap());
    let n = undo_len(&s);
    for bad in [
        json!({}),
        json!({"anchors": [[0, 2]], "dx": 1, "dy": 1}),
        json!({"id": point.0, "anchors": [[0, 0]], "dx": 1, "dy": 1}),
        json!({"id": rect.0, "anchors": [[0, 0]], "dx": 1, "dy": 1}),
        json!({"id": 99_999, "anchors": [[0, 0]], "dx": 1, "dy": 1}),
        json!({"id": id.0, "dx": 1, "dy": 1}),
        json!({"id": id.0, "anchors": [], "dx": 1, "dy": 1}),
        json!({"id": id.0, "anchors": [["a", 1], [0], "x"], "dx": 1, "dy": 1}),
        json!({"id": id.0, "anchors": [[0, 9]], "dx": 1, "dy": 1}),
        json!({"id": id.0, "anchors": [[0, 1], [7, 0]], "dx": 1, "dy": 1}),
        json!({"id": id.0, "anchors": [[0, 2]], "dy": 1}),
        json!({"id": id.0, "anchors": [[0, 2]], "dx": "far", "dy": 1}),
        json!({"id": id.0, "anchors": [[0, 2]], "dx": 0, "dy": 1e300}),
        json!({"id": id.0, "anchors": [[0, 2]], "dx": -1e7, "dy": 0}),
    ] {
        assert!(s.execute("text.reshapeArea", &bad).is_err(), "{bad}");
    }
    assert_eq!(undo_len(&s), n, "nothing changed");
    assert_eq!(frame(&s, id), Rect::new(0.0, 0.0, 120.0, 40.0));
    // Listed twice, an anchor still moves once.
    s.execute("text.reshapeArea", &json!({"id": id.0, "anchors": [[0, 2], [0, 2]], "dx": 10, "dy": 5})).unwrap();
    assert_eq!(corners(&s, id)[2], Point::new(130.0, 45.0));
    assert_eq!(last_step(&s), "Reshape Type Area");
    // A frame collapsed to nothing lays out (everything overflows) without trouble.
    for m in [[0.0, 0.0, 0.0, 1.0, 40.0, 0.0], [1.0, 0.0, 0.0, 0.0, 0.0, 40.0], [0.0; 6]] {
        s.execute("object.transform", &json!({"ids": [id.0], "matrix": m, "typeAreas": true})).unwrap();
        assert!(layout(&s, id).overflow);
        s.execute("edit.undo", &json!({})).unwrap();
    }
}

#[test]
fn reshape_area_is_a_registered_command() {
    let spec = cmd::find_command("text.reshapeArea").unwrap();
    assert_eq!(spec.label, "Reshape Type Area");
    assert!(spec.params.contains("anchors") && spec.params.contains("dx"));
    assert!(cmd::find_command("object.transform").unwrap().params.contains("typeAreas"));
    assert!(session().commands().iter().any(|c| c.id == "text.reshapeArea" && c.enabled));
}
