//! Transform & utility tools driven through `Session::pointer`, and their commands.

use serde_json::json;
use vectorcraft_color::{Color, Paint};
use vectorcraft_geom::{Point, Rect};
use vectorcraft_tools::{Mods, PointerEvent, PointerKind, ToolKey};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
    s
}

fn rect(s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> NodeId {
    let r = s.execute("shape.rectangle", &json!({"x": x, "y": y, "width": w, "height": h})).unwrap();
    NodeId(r["id"].as_u64().unwrap())
}

fn bounds(s: &Session, id: NodeId) -> Rect {
    s.doc().unwrap().doc.node(id).unwrap().geometric_bounds().unwrap()
}

fn gesture(s: &mut Session, tool: &str, pts: &[(f64, f64)], mods: Mods) -> Vec<UiRequest> {
    let v = ViewInfo::default();
    s.select_tool(tool, v).unwrap();
    let mut out = vec![];
    for (i, (x, y)) in pts.iter().enumerate() {
        let kind = if i == 0 {
            PointerKind::Down
        } else if i == pts.len() - 1 {
            PointerKind::Up
        } else {
            PointerKind::Drag
        };
        out.extend(s.pointer(&PointerEvent::new(kind, *x, *y).with_mods(mods), v).unwrap());
    }
    out
}

fn close(a: Rect, b: Rect) -> bool {
    (a.x0 - b.x0).abs() < 1e-6 && (a.y0 - b.y0).abs() < 1e-6 && (a.x1 - b.x1).abs() < 1e-6 && (a.y1 - b.y1).abs() < 1e-6
}

fn undo_len(s: &Session) -> usize {
    s.doc().unwrap().history.undo.len()
}

#[test]
fn rotate_tool_drag_rotates_about_center_one_undo() {
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 100.0, 50.0);
    let n = undo_len(&s);
    gesture(&mut s, "rotate", &[(250.0, 125.0), (200.0, 175.0), (150.0, 225.0), (150.0, 225.0)], Mods::default());
    // 90° about (150,125): 100×50 becomes 50×100.
    assert!(close(bounds(&s, a), Rect::new(125.0, 75.0, 175.0, 175.0)), "{:?}", bounds(&s, a));
    assert_eq!(undo_len(&s), n + 1);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(close(bounds(&s, a), Rect::new(100.0, 100.0, 200.0, 150.0)));
}

#[test]
fn rotate_tool_alt_release_copies() {
    let mut s = session();
    rect(&mut s, 100.0, 100.0, 100.0, 50.0);
    let alt = Mods { alt: true, ..Default::default() };
    let v = ViewInfo::default();
    s.select_tool("rotate", v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Down, 250.0, 125.0), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Drag, 150.0, 225.0), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, 150.0, 225.0).with_mods(alt), v).unwrap();
    assert_eq!(s.doc().unwrap().doc.layers[0].children().unwrap().len(), 2);
}

#[test]
fn rotate_tool_alt_click_requests_dialog() {
    let mut s = session();
    rect(&mut s, 100.0, 100.0, 100.0, 50.0);
    let alt = Mods { alt: true, ..Default::default() };
    let ui = gesture(&mut s, "rotate", &[(10.0, 20.0), (10.0, 20.0)], alt);
    assert_eq!(ui, vec![UiRequest::Dialog("rotate".into(), json!({"angle": 0, "origin": [10.0, 20.0]}))]);
}

#[test]
fn scale_tool_click_origin_then_drag() {
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 100.0, 100.0);
    gesture(&mut s, "scale", &[(100.0, 100.0), (100.0, 100.0)], Mods::default());
    gesture(&mut s, "scale", &[(200.0, 200.0), (250.0, 250.0), (300.0, 300.0), (300.0, 300.0)], Mods::default());
    assert!(close(bounds(&s, a), Rect::new(100.0, 100.0, 300.0, 300.0)), "{:?}", bounds(&s, a));
}

#[test]
fn reflect_and_shear_tools() {
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 100.0, 50.0);
    // Reflect across a vertical axis through the centre (150,125): bounds unchanged, geometry mirrored.
    gesture(&mut s, "reflect", &[(150.0, 50.0), (150.0, 40.0), (150.0, 20.0), (150.0, 20.0)], Mods::default());
    assert!(close(bounds(&s, a), Rect::new(100.0, 100.0, 200.0, 150.0)));
    // Shear horizontally: top edge shifts right by 25 relative to the centre row.
    gesture(&mut s, "shear", &[(150.0, 100.0), (160.0, 100.0), (175.0, 100.0), (175.0, 100.0)], Mods::default());
    let b = bounds(&s, a);
    assert!((b.width() - 150.0).abs() < 1e-6, "{b:?}");
}

#[test]
fn distort_command_maps_corners() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 100.0, 100.0);
    s.execute("object.distort", &json!({"corners": [[10, 0], [90, 0], [100, 100], [0, 100]]})).unwrap();
    let d = &s.doc().unwrap().doc;
    let pts: Vec<Point> = d.node(a).unwrap().path_data().unwrap().anchors().map(|(_, _, an)| an.p).collect();
    for want in [Point::new(10.0, 0.0), Point::new(90.0, 0.0), Point::new(100.0, 100.0), Point::new(0.0, 100.0)] {
        assert!(pts.iter().any(|p| p.distance(want) < 1e-6), "{pts:?} missing {want:?}");
    }
    assert!(s.execute("object.distort", &json!({"corners": [[0, 0]]})).is_err());
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(close(bounds(&s, a), Rect::new(0.0, 0.0, 100.0, 100.0)));
}

#[test]
fn distort_perspective_midpoint_is_projective() {
    // Distort a tiny probe at the centre of a 100×100 source box into a trapezoid: with a real
    // perspective warp the centre lands where the quad's diagonals cross (y = 100/6), not at y = 50.
    let mut s = session();
    let probe = rect(&mut s, 49.999, 49.999, 0.002, 0.002);
    s.execute("object.distort", &json!({"corners": [[40, 0], [60, 0], [100, 100], [0, 100]], "from": [0, 0, 100, 100], "ids": [probe.0]})).unwrap();
    let c = bounds(&s, probe).center();
    assert!((c.x - 50.0).abs() < 1e-6);
    assert!((c.y - 100.0 / 6.0).abs() < 1e-3, "{c:?}");
}

#[test]
fn free_transform_distort_mode_via_pointer() {
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 100.0, 100.0);
    let v = ViewInfo::default();
    s.select_tool("freeTransform", v).unwrap();
    s.set_tool_option("mode", &json!("distort"));
    s.pointer(&PointerEvent::new(PointerKind::Down, 200.0, 200.0), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Drag, 250.0, 260.0), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, 250.0, 260.0), v).unwrap();
    let d = &s.doc().unwrap().doc;
    let pts: Vec<Point> = d.node(a).unwrap().path_data().unwrap().anchors().map(|(_, _, an)| an.p).collect();
    assert!(pts.iter().any(|p| p.distance(Point::new(250.0, 260.0)) < 1e-6), "{pts:?}");
    assert!(pts.iter().any(|p| p.distance(Point::new(100.0, 100.0)) < 1e-6));
    assert_eq!(s.journal.last().map(|j| j.0.as_str()), Some("object.distort"));
}

#[test]
fn free_transform_cmd_held_once_a_corner_drag_started_distorts_in_one_step() {
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 100.0, 100.0);
    let v = ViewInfo::default();
    s.select_tool("freeTransform", v).unwrap();
    let before = undo_len(&s);
    let cmd = Mods { cmd: true, ..Default::default() };
    s.pointer(&PointerEvent::new(PointerKind::Down, 200.0, 200.0), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Drag, 210.0, 210.0), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Drag, 250.0, 260.0).with_mods(cmd), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, 250.0, 260.0).with_mods(cmd), v).unwrap();
    let d = &s.doc().unwrap().doc;
    let pts: Vec<Point> = d.node(a).unwrap().path_data().unwrap().anchors().map(|(_, _, an)| an.p).collect();
    for p in [Point::new(250.0, 260.0), Point::new(100.0, 100.0), Point::new(200.0, 100.0), Point::new(100.0, 200.0)] {
        assert!(pts.iter().any(|q| q.distance(p) < 1e-6), "{p:?} not in {pts:?}");
    }
    assert_eq!(undo_len(&s), before + 1);
    assert_eq!(s.doc().unwrap().history.undo.last().map(|e| e.label.as_str()), Some("Distort"));
}

#[test]
fn free_transform_side_handle_scales() {
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 100.0, 100.0);
    gesture(&mut s, "freeTransform", &[(200.0, 150.0), (250.0, 150.0), (300.0, 150.0), (300.0, 150.0)], Mods::default());
    assert!(close(bounds(&s, a), Rect::new(100.0, 100.0, 300.0, 200.0)));
}

#[test]
fn eyedropper_copies_appearance_to_selection_and_defaults() {
    let mut s = session();
    let src = rect(&mut s, 300.0, 300.0, 50.0, 50.0);
    s.execute("paint.setFill", &json!({"color": "#ff0000"})).unwrap();
    s.execute("stroke.set", &json!({"weight": 4})).unwrap();
    s.execute("object.setProps", &json!({"opacity": 50})).unwrap();
    let dst = rect(&mut s, 100.0, 100.0, 50.0, 50.0);
    s.execute("paint.default", &json!({})).unwrap();
    s.execute("select.set", &json!({"ids": [dst.0]})).unwrap();
    gesture(&mut s, "eyedropper", &[(325.0, 325.0), (325.0, 325.0)], Mods::default());
    let d = &s.doc().unwrap().doc;
    let n = d.node(dst).unwrap();
    assert_eq!(n.appearance.fill_paint(), Paint::solid(Color::from_hex("#ff0000").unwrap()));
    assert_eq!(n.appearance.stroke_width(), 4.0);
    assert!((n.opacity - 0.5).abs() < 1e-6);
    assert_eq!(s.paint.fill, Paint::solid(Color::from_hex("#ff0000").unwrap()));
    assert_eq!(s.paint.stroke_width, 4.0);
    let _ = src;
}

#[test]
fn eyedropper_shift_samples_color_into_active_proxy() {
    let mut s = session();
    rect(&mut s, 300.0, 300.0, 50.0, 50.0);
    s.execute("paint.setFill", &json!({"color": "#00ff00"})).unwrap();
    let dst = rect(&mut s, 100.0, 100.0, 50.0, 50.0);
    s.execute("paint.default", &json!({})).unwrap();
    s.execute("select.set", &json!({"ids": [dst.0]})).unwrap();
    s.execute("paint.toggleActive", &json!({})).unwrap(); // stroke active
    gesture(&mut s, "eyedropper", &[(325.0, 325.0), (325.0, 325.0)], Mods { shift: true, ..Default::default() });
    let n = s.doc().unwrap().doc.node(dst).unwrap().clone();
    assert_eq!(n.appearance.stroke_paint(), Paint::solid(Color::from_hex("#00ff00").unwrap()));
    assert_eq!(n.appearance.fill_paint(), Paint::solid(Color::WHITE));
}

#[test]
fn gradient_tool_sets_vector_on_solid_fill() {
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 100.0, 100.0);
    gesture(&mut s, "gradient", &[(100.0, 150.0), (150.0, 150.0), (200.0, 100.0), (200.0, 100.0)], Mods::default());
    let n = s.doc().unwrap().doc.node(a).unwrap().clone();
    let Paint::Gradient(g) = n.appearance.fill_paint() else { panic!("expected gradient") };
    let geom = g.geom.unwrap();
    assert_eq!(geom.start, Point::new(100.0, 150.0));
    assert_eq!(geom.end, Point::new(200.0, 100.0));
    assert_eq!(undo_len(&s), 2);
    // Changing an existing gradient keeps its stops.
    s.execute("paint.setGradientGeom", &json!({"start": [0, 0], "end": [10, 0]})).unwrap();
    let Paint::Gradient(g2) = s.doc().unwrap().doc.node(a).unwrap().appearance.fill_paint() else { panic!() };
    assert_eq!(g2.gradient, g.gradient);
    assert!(s.execute("paint.setGradientGeom", &json!({"start": [0, 0]})).is_err());
}

#[test]
fn artboard_tool_moves_with_art_creates_and_deletes() {
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 50.0, 50.0);
    let outside = rect(&mut s, 900.0, 100.0, 50.0, 50.0);
    s.execute("select.none", &json!({})).unwrap();
    gesture(&mut s, "artboard", &[(400.0, 300.0), (420.0, 310.0), (450.0, 330.0), (450.0, 330.0)], Mods::default());
    let d = &s.doc().unwrap().doc;
    assert_eq!(d.artboards[0].rect, Rect::new(50.0, 30.0, 850.0, 630.0));
    assert!(close(bounds(&s, a), Rect::new(150.0, 130.0, 200.0, 180.0)));
    assert!(close(bounds(&s, outside), Rect::new(900.0, 100.0, 950.0, 150.0)));
    // Draw a new artboard on the pasteboard.
    gesture(&mut s, "artboard", &[(1000.0, 0.0), (1100.0, 50.0), (1200.0, 100.0), (1200.0, 100.0)], Mods::default());
    assert_eq!(s.doc().unwrap().doc.artboards.len(), 2);
    assert_eq!(s.tool_options()["active"], 1);
    assert_eq!(s.doc().unwrap().doc.artboards[1].rect, Rect::new(1000.0, 0.0, 1200.0, 100.0));
    // Delete it.
    s.tool_key(ToolKey::Delete, Mods::default(), ViewInfo::default()).unwrap();
    assert_eq!(s.doc().unwrap().doc.artboards.len(), 1);
    // The last artboard can't be deleted.
    s.tool_key(ToolKey::Delete, Mods::default(), ViewInfo::default()).unwrap();
    assert_eq!(s.doc().unwrap().doc.artboards.len(), 1);
}

#[test]
fn artboard_resize_via_handle_and_move_command() {
    let mut s = session();
    gesture(&mut s, "artboard", &[(800.0, 300.0), (850.0, 300.0), (900.0, 300.0), (900.0, 300.0)], Mods::default());
    assert_eq!(s.doc().unwrap().doc.artboards[0].rect, Rect::new(0.0, 0.0, 900.0, 600.0));
    s.execute("artboard.move", &json!({"index": 0, "dx": -10, "dy": 5})).unwrap();
    assert_eq!(s.doc().unwrap().doc.artboards[0].rect, Rect::new(-10.0, 5.0, 890.0, 605.0));
    assert!(s.execute("artboard.move", &json!({"index": 7, "dx": 1})).is_err());
}

/// Alt-dragging an artboard with the Artboard tool leaves it and its art in place and moves
/// copies of both.
#[test]
fn artboard_alt_drag_duplicates_it_with_its_art() {
    let mut s = session();
    let r = rect(&mut s, 100.0, 100.0, 50.0, 50.0);
    s.execute("select.none", &json!({})).unwrap();
    let alt = Mods { alt: true, ..Mods::default() };
    gesture(&mut s, "artboard", &[(400.0, 300.0), (600.0, 300.0), (1300.0, 300.0), (1300.0, 300.0)], alt);
    let st = s.doc().unwrap();
    let d = &st.doc;
    assert_eq!(d.artboards.len(), 2);
    assert_eq!(d.artboards[0].rect, Rect::new(0.0, 0.0, 800.0, 600.0), "the original stays");
    assert_eq!((d.artboards[1].rect, d.artboards[1].name.as_str()), (Rect::new(900.0, 0.0, 1700.0, 600.0), "Artboard 1 copy"));
    assert_ne!(d.artboards[1].id, d.artboards[0].id);
    let kids = d.layers[0].children().unwrap();
    assert_eq!(kids.len(), 2, "the rectangle and its copy");
    assert_eq!(d.node(r).unwrap().geometric_bounds().unwrap().x0, 100.0);
    let copy = kids.iter().find(|n| n.id != r).unwrap();
    assert_eq!(copy.geometric_bounds().unwrap().x0, 1000.0);
    assert_eq!(st.history.undo.last().unwrap().label, "Duplicate Artboard");
    // One undo takes the copies away.
    s.execute("edit.undo", &json!({})).unwrap();
    let d = &s.doc().unwrap().doc;
    assert_eq!((d.artboards.len(), d.layers[0].children().unwrap().len()), (1, 1));
}

/// `artboard.move` with `copy` reports the copies it moved, and copies only the artboard when Move/Copy
/// Artwork with Artboard is off.
#[test]
fn artboard_move_copy_reports_the_copies_and_follows_move_art() {
    let mut s = session();
    let r = rect(&mut s, 100.0, 100.0, 50.0, 50.0);
    let out = s.execute("artboard.move", &json!({"index": 0, "dx": 900, "dy": 0, "copy": true, "moveArt": true})).unwrap();
    assert_eq!(out["index"], 1);
    let copies: Vec<NodeId> = out["moved"].as_array().unwrap().iter().map(|v| NodeId(v.as_u64().unwrap())).collect();
    assert_eq!(copies.len(), 1);
    assert_ne!(copies[0], r);
    assert_eq!(s.doc().unwrap().doc.node(copies[0]).unwrap().geometric_bounds().unwrap().x0, 1000.0);
    let out = s.execute("artboard.move", &json!({"index": 0, "dx": 0, "dy": 900, "copy": true})).unwrap();
    assert_eq!(out["moved"], json!([]));
    let d = &s.doc().unwrap().doc;
    assert_eq!((d.artboards.len(), d.layers[0].children().unwrap().len()), (3, 2), "an artboard alone");
    assert_eq!(d.artboards[2].name, "Artboard 1 copy 2", "a name of its own");
    let ids: std::collections::HashSet<u32> = d.artboards.iter().map(|a| a.id).collect();
    assert_eq!(ids.len(), 3, "every artboard has an id of its own");
}

#[test]
fn magic_wand_selects_same_fill() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 20.0, 20.0);
    let b = rect(&mut s, 50.0, 10.0, 20.0, 20.0);
    let c = rect(&mut s, 90.0, 10.0, 20.0, 20.0);
    s.execute("paint.setFill", &json!({"color": "#ff0000", "ids": [c.0]})).unwrap();
    s.execute("select.none", &json!({})).unwrap();
    gesture(&mut s, "magicWand", &[(20.0, 20.0), (20.0, 20.0)], Mods::default());
    assert_eq!(s.doc().unwrap().selection.objects, vec![a, b]);
    gesture(&mut s, "magicWand", &[(100.0, 20.0), (100.0, 20.0)], Mods { shift: true, ..Default::default() });
    assert_eq!(s.doc().unwrap().selection.len(), 3);
}

#[test]
fn lasso_selects_anchor_subset() {
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 100.0, 100.0);
    s.execute("select.none", &json!({})).unwrap();
    gesture(&mut s, "lasso", &[(90.0, 90.0), (210.0, 90.0), (210.0, 110.0), (90.0, 110.0), (90.0, 110.0)], Mods::default());
    let st = s.doc().unwrap();
    assert_eq!(st.selection.objects, vec![a]);
    assert_eq!(st.selection.partial(a).map(|p| p.len()), Some(2));
}

#[test]
fn measure_tool_leaves_document_untouched() {
    let mut s = session();
    rect(&mut s, 100.0, 100.0, 100.0, 100.0);
    let rev = s.doc().unwrap().revision;
    let n = undo_len(&s);
    gesture(&mut s, "measure", &[(0.0, 0.0), (30.0, 40.0), (30.0, 40.0)], Mods::default());
    assert_eq!(s.doc().unwrap().revision, rev);
    assert_eq!(undo_len(&s), n);
    assert!((s.tool_options()["distance"].as_f64().unwrap() - 50.0).abs() < 1e-9);
    assert!(!s.overlays(ViewInfo::default()).is_empty());
}

#[test]
fn copy_from_requires_source() {
    let mut s = session();
    assert!(s.execute("appearance.copyFrom", &json!({})).is_err());
    assert!(s.execute("appearance.copyFrom", &json!({"source": 9999})).is_err());
}
