//! Tests for Shape Builder, Live Paint and Image Trace commands and tools.

use std::sync::Arc;

use serde_json::{Value, json};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{ImageBlob, ImageObject, Node, NodeKind};
use vectorcraft_geom::{Affine, FillRule, PathData, Rect};
use vectorcraft_tools::{Mods, PointerEvent, PointerKind};

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

fn set_fill(s: &mut Session, id: NodeId, c: Color) {
    s.edit("t", |d, _| {
        d.node_mut(id).unwrap().appearance.set_fill(Paint::solid(c));
        Ok(())
    })
    .unwrap();
}

fn ids(v: &Value) -> Vec<NodeId> {
    v["ids"].as_array().unwrap().iter().map(|x| NodeId(x.as_u64().unwrap())).collect()
}

fn node(s: &Session, id: NodeId) -> Node {
    s.doc().unwrap().doc.node(id).cloned().unwrap()
}

fn outline(n: &Node) -> (PathData, FillRule) {
    vectorcraft_tools::builder::node_outline(n).expect("path")
}

fn area(s: &Session, id: NodeId) -> f64 {
    let (p, r) = outline(&node(s, id));
    vectorcraft_pathops::area(&p, r)
}

/// Rect A (0..100) and rect B (50..150), both 100 tall, selected. A red, B blue.
fn two(s: &mut Session) -> (NodeId, NodeId) {
    let a = rect(s, 0.0, 0.0, 100.0, 100.0);
    let b = rect(s, 50.0, 0.0, 100.0, 100.0);
    set_fill(s, a, Color::rgb(1.0, 0.0, 0.0));
    set_fill(s, b, Color::rgb(0.0, 0.0, 1.0));
    s.execute("select.set", &json!({"ids": [a.0, b.0]})).unwrap();
    (a, b)
}

fn layer_children(s: &Session) -> usize {
    let d = &s.doc().unwrap().doc;
    d.children(d.default_layer()).map(|c| c.len()).unwrap_or(0)
}

// ---------- Shape Builder ----------

#[test]
fn shape_builder_drag_merges_touched_regions() {
    let mut s = session();
    two(&mut s);
    let r = s.execute("shapeBuilder.merge", &json!({"points": [[25, 50], [75, 50]]})).unwrap();
    assert_eq!(r["regions"], 2);
    let out = ids(&r);
    assert_eq!(out.len(), 2, "merged shape + remaining part of B");
    let merged = NodeId(r["merged"].as_u64().unwrap());
    assert!((area(&s, merged) - 10000.0).abs() < 1.0, "{}", area(&s, merged));
    let rest = out.iter().copied().find(|i| *i != merged).unwrap();
    assert!((area(&s, rest) - 5000.0).abs() < 1.0);
    // Merged shape takes the fill of the object under the drag start (A, red).
    assert_eq!(node(&s, merged).appearance.fill_paint(), Paint::solid(Color::rgb(1.0, 0.0, 0.0)));
    assert_eq!(layer_children(&s), 2);
}

#[test]
fn shape_builder_click_splits_out_one_region() {
    let mut s = session();
    two(&mut s);
    let r = s.execute("shapeBuilder.merge", &json!({"points": [[75, 50]]})).unwrap();
    let out = ids(&r);
    assert_eq!(out.len(), 3);
    let total: f64 = out.iter().map(|i| area(&s, *i)).sum();
    assert!((total - 15000.0).abs() < 1.0, "{total}");
    let merged = NodeId(r["merged"].as_u64().unwrap());
    // Overlap is covered by B on top → blue.
    assert_eq!(node(&s, merged).appearance.fill_paint(), Paint::solid(Color::rgb(0.0, 0.0, 1.0)));
    let bb = node(&s, merged).geometric_bounds().unwrap();
    assert!((bb.x0 - 50.0).abs() < 1e-6 && (bb.x1 - 100.0).abs() < 1e-6);
}

#[test]
fn shape_builder_erase_deletes_regions() {
    let mut s = session();
    two(&mut s);
    let r = s.execute("shapeBuilder.merge", &json!({"points": [[75, 50]], "erase": true})).unwrap();
    assert!(r["merged"].is_null());
    let out = ids(&r);
    assert_eq!(out.len(), 2);
    for i in out {
        assert!((area(&s, i) - 5000.0).abs() < 1.0);
    }
}

#[test]
fn shape_builder_fill_option_and_untouched_shapes_kept() {
    let mut s = session();
    let (a, b) = two(&mut s);
    let c = rect(&mut s, 300.0, 300.0, 50.0, 50.0);
    let r = s.execute("shapeBuilder.merge", &json!({"ids": [a.0, b.0, c.0], "points": [[25, 50], [125, 50]], "fill": "#00ff00"})).unwrap();
    let out = ids(&r);
    assert_eq!(out.len(), 2, "one merged + the untouched square");
    let merged = NodeId(r["merged"].as_u64().unwrap());
    assert!((area(&s, merged) - 15000.0).abs() < 1.0);
    assert_eq!(node(&s, merged).appearance.fill_paint(), Paint::solid(Color::from_hex("#00ff00").unwrap()));
    let sq = out.iter().copied().find(|i| *i != merged).unwrap();
    assert_eq!(node(&s, sq).geometric_bounds().unwrap(), Rect::new(300.0, 300.0, 350.0, 350.0));
    assert_eq!(outline(&node(&s, sq)).0.anchor_count(), 4);
}

#[test]
fn shape_builder_miss_is_an_error_and_undo_restores() {
    let mut s = session();
    let (a, b) = two(&mut s);
    let undo_before = s.doc().unwrap().history.undo.len();
    assert!(s.execute("shapeBuilder.merge", &json!({"points": [[500, 500]]})).is_err());
    assert_eq!(s.doc().unwrap().history.undo.len(), undo_before);
    s.execute("shapeBuilder.merge", &json!({"points": [[25, 50], [75, 50]]})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(a).is_some() && s.doc().unwrap().doc.node(b).is_some());
    assert!(s.execute("shapeBuilder.merge", &json!({"points": []})).is_err());
    assert!(s.execute("shapeBuilder.merge", &json!({})).is_err());
}

#[test]
fn shape_builder_regions_query() {
    let mut s = session();
    two(&mut s);
    let r = s.execute("shapeBuilder.regions", &json!({})).unwrap();
    let regs = r["regions"].as_array().unwrap();
    assert_eq!(regs.len(), 3);
    let total: f64 = regs.iter().map(|x| x["area"].as_f64().unwrap()).sum();
    assert!((total - 15000.0).abs() < 1.0);
    assert!(regs.iter().any(|x| x["sources"].as_array().unwrap().len() == 2));
}

#[test]
fn shape_builder_tool_drives_the_command() {
    let mut s = session();
    two(&mut s);
    let v = ViewInfo::default();
    s.select_tool("shapeBuilder", v).unwrap();
    assert_eq!(s.tool_id(), "shapeBuilder");
    s.pointer(&PointerEvent::new(PointerKind::Move, 75.0, 50.0), v).unwrap();
    assert!(!s.overlays(v).is_empty(), "hover highlight");
    s.pointer(&PointerEvent::new(PointerKind::Down, 25.0, 50.0), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Drag, 75.0, 50.0), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, 75.0, 50.0), v).unwrap();
    assert_eq!(layer_children(&s), 2);
    assert_eq!(s.journal.last().unwrap().0, "shapeBuilder.merge");
    // Alt-click deletes a region.
    let alt = Mods { alt: true, ..Default::default() };
    s.pointer(&PointerEvent::new(PointerKind::Down, 125.0, 50.0).with_mods(alt), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, 125.0, 50.0).with_mods(alt), v).unwrap();
    assert_eq!(layer_children(&s), 1);
}

/// A red 100 × 100 square crossed by a stroke-only vertical line at x = 50, both selected.
fn square_and_line(s: &mut Session) -> (NodeId, NodeId) {
    let a = rect(s, 0.0, 0.0, 100.0, 100.0);
    set_fill(s, a, Color::rgb(1.0, 0.0, 0.0));
    let l = line(s, (50.0, -20.0), (50.0, 120.0));
    s.execute("select.set", &json!({"ids": [a.0, l.0]})).unwrap();
    (a, l)
}

/// The open paths among `ids`, as (anchors, bounds).
fn open_paths(s: &Session, ids: &[NodeId]) -> Vec<(usize, Rect)> {
    ids.iter()
        .map(|i| node(s, *i))
        .filter(|n| !outline(n).0.is_closed())
        .map(|n| (outline(&n).0.anchor_count(), n.geometric_bounds().unwrap()))
        .collect()
}

#[test]
fn shape_builder_line_splits_a_shape_into_regions() {
    let mut s = session();
    let (a, l) = square_and_line(&mut s);
    let r = s.execute("shapeBuilder.regions", &json!({})).unwrap();
    let regs = r["regions"].as_array().unwrap();
    assert_eq!(regs.len(), 2, "{r}");
    for x in regs {
        assert!((x["area"].as_f64().unwrap() - 5000.0).abs() < 1e-6);
        assert_eq!(x["sources"], json!([a.0]));
    }
    let lines = r["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 3, "the line cut at both sides of the square");
    assert!(lines.iter().all(|x| x["source"] == l.0));
}

#[test]
fn shape_builder_click_on_a_side_of_a_line_splits_the_shape() {
    let mut s = session();
    square_and_line(&mut s);
    let r = s.execute("shapeBuilder.merge", &json!({"points": [[25, 50]]})).unwrap();
    assert_eq!(r["regions"], 1);
    let out = ids(&r);
    let merged = NodeId(r["merged"].as_u64().unwrap());
    assert_eq!(node(&s, merged).geometric_bounds().unwrap(), Rect::new(0.0, 0.0, 50.0, 100.0));
    assert_eq!(node(&s, merged).appearance.fill_paint(), Paint::solid(Color::rgb(1.0, 0.0, 0.0)));
    let closed: Vec<f64> = out.iter().filter(|i| outline(&node(&s, **i)).0.is_closed()).map(|i| area(&s, *i)).collect();
    assert_eq!(closed.len(), 2, "both halves");
    assert!(closed.iter().all(|a| (a - 5000.0).abs() < 1.0), "{closed:?}");
    // The line only bounds the region: it stays whole.
    assert_eq!(open_paths(&s, &out), vec![(2, Rect::new(50.0, -20.0, 50.0, 120.0))]);
}

#[test]
fn shape_builder_merge_across_a_line_removes_the_piece_between() {
    let mut s = session();
    square_and_line(&mut s);
    let r = s.execute("shapeBuilder.merge", &json!({"points": [[25, 50], [75, 50]]})).unwrap();
    assert_eq!(r["regions"], 2);
    let out = ids(&r);
    let merged = NodeId(r["merged"].as_u64().unwrap());
    assert!((area(&s, merged) - 10000.0).abs() < 1.0);
    assert_eq!(out.len(), 3, "merged square + the line's two loose ends");
    let mut ends = open_paths(&s, &out);
    ends.sort_by(|a, b| a.1.y0.total_cmp(&b.1.y0));
    assert_eq!(ends, vec![(2, Rect::new(50.0, -20.0, 50.0, 0.0)), (2, Rect::new(50.0, 100.0, 50.0, 120.0))]);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(layer_children(&s), 2);
}

#[test]
fn shape_builder_alt_deletes_a_region_or_a_line_piece() {
    let mut s = session();
    square_and_line(&mut s);
    let r = s.execute("shapeBuilder.merge", &json!({"points": [[75, 50]], "erase": true})).unwrap();
    assert!(r["merged"].is_null());
    let out = ids(&r);
    let left: Vec<NodeId> = out.iter().copied().filter(|i| outline(&node(&s, *i)).0.is_closed()).collect();
    assert_eq!(left.len(), 1);
    assert_eq!(node(&s, left[0]).geometric_bounds().unwrap(), Rect::new(0.0, 0.0, 50.0, 100.0));
    assert_eq!(open_paths(&s, &out).len(), 1, "the line stays whole");

    // Alt on the line inside the square deletes that piece only.
    let mut s = session();
    square_and_line(&mut s);
    let r = s.execute("shapeBuilder.merge", &json!({"points": [[51, 50]], "erase": true, "tolerance": 2})).unwrap();
    assert_eq!((r["regions"].as_u64(), r["lines"].as_u64()), (Some(0), Some(1)));
    let out = ids(&r);
    assert_eq!(out.len(), 3);
    let sq: Vec<NodeId> = out.iter().copied().filter(|i| outline(&node(&s, *i)).0.is_closed()).collect();
    assert_eq!(sq.len(), 1);
    assert!((area(&s, sq[0]) - 10000.0).abs() < 1.0, "the square is untouched");
    assert_eq!(open_paths(&s, &out).len(), 2);
}

#[test]
fn shape_builder_fills_an_area_lines_enclose() {
    let mut s = session();
    let l1 = line(&mut s, (0.0, 0.0), (100.0, 100.0));
    let l2 = line(&mut s, (100.0, 0.0), (0.0, 100.0));
    let l3 = line(&mut s, (-10.0, 80.0), (110.0, 80.0));
    s.execute("select.set", &json!({"ids": [l1.0, l2.0, l3.0]})).unwrap();
    let r = s.execute("shapeBuilder.regions", &json!({})).unwrap();
    assert_eq!(r["regions"].as_array().unwrap().len(), 1);
    assert_eq!(r["regions"][0]["sources"], json!([]));
    let r = s.execute("shapeBuilder.merge", &json!({"points": [[50, 70]]})).unwrap();
    let merged = NodeId(r["merged"].as_u64().unwrap());
    assert!((area(&s, merged) - 900.0).abs() < 1e-6, "{}", area(&s, merged));
    assert_eq!(node(&s, merged).appearance.fill_paint(), s.paint.fill, "the current fill");
    assert!(!node(&s, merged).appearance.stroke_paint().is_none(), "the lines' stroke");
    assert_eq!(ids(&r).len(), 4, "the triangle + the three lines, which only bound it");
}

#[test]
fn shape_builder_closes_filled_open_paths() {
    let mut s = session();
    let r = s.execute("path.create", &json!({"anchors": [{"x": 0, "y": 0}, {"x": 100, "y": 0}, {"x": 100, "y": 100}]})).unwrap();
    let p = NodeId(r["id"].as_u64().unwrap());
    set_fill(&mut s, p, Color::rgb(0.0, 1.0, 0.0));
    s.execute("select.set", &json!({"ids": [p.0]})).unwrap();
    let r = s.execute("shapeBuilder.regions", &json!({})).unwrap();
    assert_eq!(r["regions"].as_array().unwrap().len(), 1, "the fill's area, closed by an invisible edge");
    assert!((r["regions"][0]["area"].as_f64().unwrap() - 5000.0).abs() < 1e-6);
    assert_eq!(r["lines"], json!([]));
    // Unfilled, it is only an edge.
    s.edit("t", |d, _| {
        d.node_mut(p).unwrap().appearance.set_fill(Paint::None);
        Ok(())
    })
    .unwrap();
    let r = s.execute("shapeBuilder.regions", &json!({})).unwrap();
    assert_eq!(r["regions"], json!([]));
    assert_eq!(r["lines"].as_array().unwrap().len(), 1);
}

#[test]
fn shape_builder_tool_alt_deletes_line_pieces() {
    let mut s = session();
    square_and_line(&mut s);
    let v = ViewInfo::default();
    s.select_tool("shapeBuilder", v).unwrap();
    let alt = Mods { alt: true, ..Default::default() };
    s.pointer(&PointerEvent::new(PointerKind::Move, 51.0, 50.0).with_mods(alt), v).unwrap();
    assert_eq!(s.overlays(v).len(), 1, "the line piece is highlighted");
    s.pointer(&PointerEvent::new(PointerKind::Down, 51.0, 50.0).with_mods(alt), v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, 51.0, 50.0).with_mods(alt), v).unwrap();
    assert_eq!(s.journal.last().unwrap().0, "shapeBuilder.merge");
    assert_eq!(layer_children(&s), 3, "square + the line's two loose ends");
}

/// Red square A (0..100) and blue square B (50..150 both ways), overlapping at a corner, selected.
fn corner(s: &mut Session) -> (NodeId, NodeId) {
    let a = rect(s, 0.0, 0.0, 100.0, 100.0);
    let b = rect(s, 50.0, 50.0, 100.0, 100.0);
    set_fill(s, a, Color::rgb(1.0, 0.0, 0.0));
    set_fill(s, b, Color::rgb(0.0, 0.0, 1.0));
    s.execute("select.set", &json!({"ids": [a.0, b.0]})).unwrap();
    (a, b)
}

/// (closed, anchors, length) of each of `ids`.
fn shapes_of(s: &Session, ids: &[NodeId]) -> Vec<(bool, usize, f64)> {
    ids.iter()
        .map(|i| {
            let (p, _) = outline(&node(s, *i));
            (p.is_closed(), p.anchor_count(), (p.length() * 1e6).round() / 1e6)
        })
        .collect()
}

#[test]
fn shape_builder_erase_click_on_an_edge_opens_its_path() {
    let mut s = session();
    corner(&mut s);
    let r = s.execute("shapeBuilder.regions", &json!({})).unwrap();
    assert_eq!(r["edges"].as_array().unwrap().len(), 4, "each outline cut in two where the other crosses it: {r}");
    // A's side inside B (1 pt off it).
    let r = s.execute("shapeBuilder.merge", &json!({"points": [[101, 75]], "erase": true})).unwrap();
    assert_eq!((r["regions"].as_u64(), r["lines"].as_u64(), r["edges"].as_u64()), (Some(0), Some(0), Some(1)));
    let out = ids(&r);
    // A opens where B's outline crosses it and keeps its look; B is untouched.
    assert_eq!(shapes_of(&s, &out), vec![(false, 5, 300.0), (true, 4, 400.0)]);
    assert_eq!(node(&s, out[0]).appearance.fill_paint(), Paint::solid(Color::rgb(1.0, 0.0, 0.0)));
    assert_eq!(s.journal.last().unwrap().0, "shapeBuilder.merge");
    s.execute("edit.undo", &json!({})).unwrap();
    let back: Vec<NodeId> = s.doc().unwrap().doc.children(s.doc().unwrap().doc.default_layer()).unwrap().iter().map(|n| n.id).collect();
    assert_eq!(shapes_of(&s, &back), vec![(true, 4, 400.0); 2], "one undo step");
}

#[test]
fn shape_builder_erase_drag_along_edges_deletes_each() {
    let mut s = session();
    corner(&mut s);
    // Down A's side inside B, round the corner and along B's side inside A.
    let r = s.execute("shapeBuilder.merge", &json!({"points": [[100, 75], [100, 50], [75, 50]], "erase": true})).unwrap();
    assert_eq!((r["regions"].as_u64(), r["edges"].as_u64()), (Some(0), Some(2)));
    assert_eq!(shapes_of(&s, &ids(&r)), vec![(false, 5, 300.0); 2]);
}

#[test]
fn shape_builder_erase_drag_into_regions_deletes_them_not_the_edges_it_crosses() {
    let mut s = session();
    corner(&mut s);
    let r = s.execute("shapeBuilder.merge", &json!({"points": [[25, 25], [75, 75]], "erase": true})).unwrap();
    assert_eq!((r["regions"].as_u64(), r["edges"].as_u64()), (Some(2), Some(0)));
    // A is gone; what is left of B stays closed and blue.
    let out = ids(&r);
    assert_eq!(out.len(), 1);
    assert!((area(&s, out[0]) - 7500.0).abs() < 1e-6);
    assert!(outline(&node(&s, out[0])).0.is_closed());
    assert_eq!(node(&s, out[0]).appearance.fill_paint(), Paint::solid(Color::rgb(0.0, 0.0, 1.0)));
}

// ---------- Live Paint ----------

fn live(s: &mut Session) -> NodeId {
    two(s);
    let r = s.execute("livePaint.make", &json!({})).unwrap();
    assert_eq!(r["faces"], 3);
    assert!(r["edges"].as_u64().unwrap() >= 4);
    NodeId(r["id"].as_u64().unwrap())
}

#[test]
fn live_paint_make_builds_group_with_hidden_sources() {
    let mut s = session();
    let g = live(&mut s);
    let n = node(&s, g);
    assert!(vectorcraft_tools::builder::is_live_paint(&n));
    let src = vectorcraft_tools::builder::sources(&n).unwrap();
    assert!(!src.visible);
    assert_eq!(src.children().unwrap().len(), 2);
    assert_eq!(layer_children(&s), 1);
    assert_eq!(s.doc().unwrap().selection.objects, vec![g]);
    // Faces start with the fill of the top-most covering object.
    let info = s.execute("livePaint.info", &json!({"group": g.0})).unwrap();
    assert_eq!(info["faces"].as_array().unwrap().len(), 3);
    assert_eq!(info["sources"], 2);
}

#[test]
fn live_paint_fill_assigns_the_face_under_the_point() {
    let mut s = session();
    let g = live(&mut s);
    let r = s.execute("livePaint.fill", &json!({"group": g.0, "point": [75, 50], "color": "#00ff00"})).unwrap();
    let face = node(&s, NodeId(r["face"].as_u64().unwrap()));
    let bb = face.geometric_bounds().unwrap();
    assert!((bb.x0 - 50.0).abs() < 1e-6 && (bb.x1 - 100.0).abs() < 1e-6, "{bb:?}");
    assert_eq!(face.appearance.fill_paint(), Paint::solid(Color::from_hex("#00ff00").unwrap()));
    // The other faces are untouched.
    let gn = node(&s, g);
    let green = vectorcraft_tools::builder::faces(&gn).iter().filter(|f| f.appearance.fill_paint() == face.appearance.fill_paint()).count();
    assert_eq!(green, 1);
    // Default paint = current fill; a miss is an error.
    s.paint.fill = Paint::solid(Color::rgb(1.0, 1.0, 0.0));
    let r = s.execute("livePaint.fill", &json!({"group": g.0, "point": [10, 10]})).unwrap();
    assert_eq!(node(&s, NodeId(r["face"].as_u64().unwrap())).appearance.fill_paint(), s.paint.fill);
    assert!(s.execute("livePaint.fill", &json!({"group": g.0, "point": [400, 400]})).is_err());
}

#[test]
fn live_paint_fill_on_plain_paths_makes_the_group() {
    let mut s = session();
    let (a, b) = two(&mut s);
    let r = s.execute("livePaint.fill", &json!({"ids": [a.0, b.0], "point": [125, 50], "color": "#123456"})).unwrap();
    let g = NodeId(r["group"].as_u64().unwrap());
    assert!(vectorcraft_tools::builder::is_live_paint(&node(&s, g)));
    assert_eq!(layer_children(&s), 1);
    // One undo step removes both the make and the fill.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(layer_children(&s), 2);
}

#[test]
fn live_paint_stroke_edge() {
    let mut s = session();
    let g = live(&mut s);
    let r = s.execute("livePaint.strokeEdge", &json!({"group": g.0, "point": [100, 50], "color": "#ff0000", "width": 3})).unwrap();
    let e = node(&s, NodeId(r["edge"].as_u64().unwrap()));
    assert_eq!(e.appearance.stroke_width(), 3.0);
    assert!(e.appearance.fill_paint().is_none());
    let bb = e.geometric_bounds().unwrap();
    assert!((bb.x0 - 100.0).abs() < 1e-6 && (bb.x1 - 100.0).abs() < 1e-6, "the vertical edge at x=100: {bb:?}");
    assert!(s.execute("livePaint.strokeEdge", &json!({"group": g.0, "point": [300, 300]})).is_err());
}

#[test]
fn live_paint_release_and_expand() {
    let mut s = session();
    let g = live(&mut s);
    s.execute("livePaint.fill", &json!({"group": g.0, "point": [75, 50], "none": true})).unwrap();
    let r = s.execute("livePaint.expand", &json!({})).unwrap();
    assert_eq!(ids(&r), vec![g]);
    let n = node(&s, g);
    assert!(!vectorcraft_tools::builder::is_live_paint(&n));
    // Unpainted face and the hidden sources are gone: 2 faces + stroked edges.
    let ch = n.children().unwrap();
    assert!(ch.iter().all(|c| c.visible));
    assert_eq!(ch.iter().filter(|c| c.name.as_deref() == Some("Face")).count(), 2);

    let mut s = session();
    let (a, b) = two(&mut s);
    live_group_only(&mut s);
    let r = s.execute("livePaint.release", &json!({})).unwrap();
    assert_eq!(ids(&r), vec![a, b]);
    assert!((area(&s, a) - 10000.0).abs() < 1.0);
}

fn live_group_only(s: &mut Session) -> NodeId {
    let r = s.execute("livePaint.make", &json!({})).unwrap();
    NodeId(r["id"].as_u64().unwrap())
}

#[test]
fn live_paint_merge_keeps_paint_and_adds_faces() {
    let mut s = session();
    let g = live(&mut s);
    s.execute("livePaint.fill", &json!({"group": g.0, "point": [25, 50], "color": "#00ff00"})).unwrap();
    let c = rect(&mut s, 120.0, 20.0, 100.0, 20.0);
    s.execute("select.set", &json!({"ids": [g.0, c.0]})).unwrap();
    let r = s.execute("livePaint.merge", &json!({})).unwrap();
    assert_eq!(NodeId(r["id"].as_u64().unwrap()), g);
    assert!(r["faces"].as_u64().unwrap() > 3);
    let gn = node(&s, g);
    let f = vectorcraft_tools::builder::face_at(&gn, vectorcraft_geom::Point::new(25.0, 50.0)).unwrap();
    assert_eq!(f.appearance.fill_paint(), Paint::solid(Color::from_hex("#00ff00").unwrap()));
    let src = vectorcraft_tools::builder::sources(&gn).unwrap();
    assert_eq!(src.children().unwrap().len(), 3, "the rectangle joined the sources");
    assert_eq!(s.doc().unwrap().doc.parent_of(c), Some(src.id));
    assert_eq!(layer_children(&s), 1);
}

#[test]
fn live_paint_bucket_tool_fills_face() {
    let mut s = session();
    let g = live(&mut s);
    let v = ViewInfo::default();
    s.select_tool("livePaintBucket", v).unwrap();
    s.paint.fill = Paint::solid(Color::rgb(0.0, 1.0, 1.0));
    s.pointer(&PointerEvent::new(PointerKind::Move, 125.0, 50.0), v).unwrap();
    assert!(!s.overlays(v).is_empty(), "red face highlight");
    s.pointer(&PointerEvent::new(PointerKind::Down, 125.0, 50.0), v).unwrap();
    let gn = node(&s, g);
    let f = vectorcraft_tools::builder::face_at(&gn, vectorcraft_geom::Point::new(125.0, 50.0)).unwrap();
    assert_eq!(f.appearance.fill_paint(), s.paint.fill);
    // Live Paint Selection selects the face.
    s.select_tool("livePaintSelection", v).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Down, 125.0, 50.0), v).unwrap();
    assert_eq!(s.doc().unwrap().selection.objects, vec![f.id]);
}

fn line(s: &mut Session, a: (f64, f64), b: (f64, f64)) -> NodeId {
    let r = s.execute("shape.line", &json!({"x1": a.0, "y1": a.1, "x2": b.0, "y2": b.1})).unwrap();
    NodeId(r["id"].as_u64().unwrap())
}

/// Crossing lines, as in the bug report: the Live Paint Bucket fills the area they enclose.
#[test]
fn live_paint_bucket_fills_an_area_enclosed_by_lines() {
    let mut s = session();
    let ids = [line(&mut s, (0.0, 0.0), (100.0, 100.0)), line(&mut s, (100.0, 0.0), (0.0, 100.0)), line(&mut s, (-10.0, 80.0), (110.0, 80.0))];
    s.execute("select.set", &json!({"ids": ids.map(|i| i.0)})).unwrap();
    let v = ViewInfo::default();
    s.select_tool("livePaintBucket", v).unwrap();
    s.paint.fill = Paint::solid(Color::rgb(1.0, 0.0, 1.0));
    // Inside the triangle (20,80) (50,50) (80,80): highlighted, then filled.
    s.pointer(&PointerEvent::new(PointerKind::Move, 50.0, 70.0), v).unwrap();
    assert!(!s.overlays(v).is_empty(), "red face highlight");
    s.pointer(&PointerEvent::new(PointerKind::Down, 50.0, 70.0), v).unwrap();
    let g = s.doc().unwrap().selection.objects[0];
    let gn = node(&s, g);
    assert!(vectorcraft_tools::builder::is_live_paint(&gn));
    assert_eq!(vectorcraft_tools::builder::faces(&gn).len(), 1);
    let f = vectorcraft_tools::builder::face_at(&gn, vectorcraft_geom::Point::new(50.0, 70.0)).unwrap();
    assert_eq!(f.appearance.fill_paint(), s.paint.fill);
    assert!((vectorcraft_pathops::area(f.path_data().unwrap(), FillRule::NonZero).abs() - 900.0).abs() < 1e-6);
    // The lines are edges, cut where they cross, and keep their stroke.
    let edges = vectorcraft_tools::builder::edges(&gn);
    assert_eq!(edges.len(), 9);
    assert!(edges.iter().all(|e| !e.appearance.stroke_paint().is_none()));
    // Outside the triangle there's nothing to fill.
    assert!(s.execute("livePaint.fill", &json!({"group": g.0, "point": [50, 20]})).is_err());
}

#[test]
fn a_line_across_a_live_paint_shape_splits_its_face() {
    let mut s = session();
    let r = rect(&mut s, 0.0, 0.0, 100.0, 50.0);
    set_fill(&mut s, r, Color::rgb(1.0, 0.0, 0.0));
    let l = line(&mut s, (40.0, -20.0), (60.0, 70.0));
    s.execute("select.set", &json!({"ids": [r.0, l.0]})).unwrap();
    let made = s.execute("livePaint.make", &json!({})).unwrap();
    assert_eq!(made["faces"], 2, "{made}");
    let g = NodeId(made["id"].as_u64().unwrap());
    s.execute("livePaint.fill", &json!({"group": g.0, "point": [80, 25], "color": "#0000ff"})).unwrap();
    let gn = node(&s, g);
    let face = |x: f64| vectorcraft_tools::builder::face_at(&gn, vectorcraft_geom::Point::new(x, 25.0)).unwrap().appearance.fill_paint();
    assert_eq!(face(10.0), Paint::solid(Color::rgb(1.0, 0.0, 0.0)), "the left half keeps the rectangle's fill");
    assert_eq!(face(80.0), Paint::solid(Color::from_hex("#0000ff").unwrap()));
}

// ---------- Image Trace ----------

/// A 100×100 image with a black disc (r = 30 px), placed at (50, 50) scaled ×2.
fn image_doc(s: &mut Session) -> NodeId {
    let r = vectorcraft_trace::Raster::from_fn(100, 100, |x, y| {
        let (dx, dy) = (x as f64 + 0.5 - 50.0, y as f64 + 0.5 - 50.0);
        if dx * dx + dy * dy <= 900.0 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }
    });
    add_image(s, &r, Affine::translate((50.0, 50.0)) * Affine::scale(2.0))
}

fn add_image(s: &mut Session, r: &vectorcraft_trace::Raster, xf: Affine) -> NodeId {
    let png = r.encode_png();
    let (w, h) = (r.width, r.height);
    s.edit("img", |d, sel| {
        d.images.insert("img1".into(), ImageBlob::new("image/png", png));
        let id = d.alloc_id();
        let l = d.default_layer();
        d.insert(
            l,
            0,
            Node::new(id, NodeKind::Image(ImageObject { key: "img1".into(), width: w, height: h, xf, link: None, placement: Default::default() })),
        )?;
        sel.set([id]);
        Ok(id)
    })
    .unwrap()
}

fn traced_paths(n: &Node) -> Vec<Arc<Node>> {
    n.children().unwrap().iter().filter(|c| !matches!(c.kind, NodeKind::Image(_))).cloned().collect()
}

#[test]
fn image_trace_make_places_paths_over_the_image() {
    let mut s = session();
    let img = image_doc(&mut s);
    let r = s.execute("imageTrace.make", &json!({"params": {"ignoreWhite": true}})).unwrap();
    assert_eq!(r["paths"], 1);
    let g = NodeId(r["id"].as_u64().unwrap());
    let n = node(&s, g);
    assert_eq!(n.name.as_deref(), Some("Image Trace"));
    let first = &n.children().unwrap()[0];
    assert!(matches!(first.kind, NodeKind::Image(_)) && !first.visible && first.id == img);
    let paths = traced_paths(&n);
    assert_eq!(paths.len(), 1);
    let (p, rule) = outline(&paths[0]);
    let want = std::f64::consts::PI * 60.0 * 60.0; // r = 30 px × 2
    let got = vectorcraft_pathops::area(&p, rule);
    assert!((got - want).abs() / want < 0.03, "{got} vs {want}");
    let bb = p.bounds().unwrap();
    assert!((bb.center().x - 150.0).abs() < 1.5 && (bb.center().y - 150.0).abs() < 1.5, "{bb:?}");
    assert_eq!(paths[0].appearance.fill_paint(), Paint::solid(Color::rgb8(0, 0, 0)));
}

#[test]
fn image_trace_release_and_expand() {
    let mut s = session();
    let img = image_doc(&mut s);
    s.execute("imageTrace.make", &json!({"preset": "Black and White Logo"})).unwrap();
    let r = s.execute("imageTrace.release", &json!({})).unwrap();
    assert_eq!(ids(&r), vec![img]);
    assert!(node(&s, img).visible);
    assert_eq!(layer_children(&s), 1);

    let r = s.execute("imageTrace.make", &json!({"id": img.0})).unwrap();
    let g = NodeId(r["id"].as_u64().unwrap());
    // Re-trace an existing Image Trace group with other parameters.
    let r2 = s.execute("imageTrace.make", &json!({"id": g.0, "params": {"ignoreWhite": true}})).unwrap();
    assert_eq!(r2["paths"], 1);
    let g = NodeId(r2["id"].as_u64().unwrap());
    s.execute("imageTrace.expand", &json!({})).unwrap();
    let n = node(&s, g);
    assert!(n.name.is_none());
    assert!(n.children().unwrap().iter().all(|c| !matches!(c.kind, NodeKind::Image(_))));
    assert!(s.execute("imageTrace.release", &json!({})).is_err());
}

#[test]
fn image_trace_make_and_expand_and_color_presets() {
    let mut s = session();
    let r = vectorcraft_trace::Raster::from_fn(60, 60, |x, y| match (x < 30, y < 30) {
        (true, true) => [230, 20, 20, 255],
        (false, true) => [20, 200, 30, 255],
        (true, false) => [20, 30, 220, 255],
        (false, false) => [250, 230, 10, 255],
    });
    add_image(&mut s, &r, Affine::IDENTITY);
    let res = s.execute("imageTrace.makeAndExpand", &json!({"preset": "16 Colors", "params": {"colors": 4}})).unwrap();
    assert_eq!(res["colors"], 4);
    assert_eq!(res["paths"], 4);
    let g = node(&s, NodeId(res["id"].as_u64().unwrap()));
    assert!(g.name.is_none());
    assert_eq!(g.children().unwrap().len(), 4);
    assert!(s.execute("imageTrace.make", &json!({"preset": "No Such Preset"})).is_err());
    assert!(s.execute("imageTrace.make", &json!({"params": {"mode": "rainbow"}})).is_err());
}

#[test]
fn image_trace_presets_query_and_errors() {
    let mut s = session();
    let r = s.execute("imageTrace.presets", &json!({})).unwrap();
    let names: Vec<&str> = r["presets"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), 12);
    assert!(names.contains(&"Technical Drawing") && names.contains(&"Shades of Gray"));
    let a = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    assert!(s.execute("imageTrace.make", &json!({"id": a.0})).is_err(), "not an image");
    assert!(find_command("imageTrace.make").unwrap().menu == ["Object", "Image Trace"]);
    assert!(find_command("livePaint.make").unwrap().menu == ["Object", "Live Paint"]);
}

#[test]
fn image_trace_object_remembers_its_settings() {
    let mut s = session();
    image_doc(&mut s);
    let r = s.execute("imageTrace.make", &json!({"preset": "6 Colors"})).unwrap();
    let g = NodeId(r["id"].as_u64().unwrap());
    let t = node(&s, g).trace.clone().expect("settings stored");
    assert_eq!(t["preset"], "6 Colors");
    assert_eq!(t["params"]["colors"], 6);
    // Changing a parameter makes it Custom; the settings survive save/open.
    let r = s.execute("imageTrace.make", &json!({"id": g.0, "preset": "6 Colors", "params": {"colors": 9}})).unwrap();
    let g = NodeId(r["id"].as_u64().unwrap());
    let d = s.doc().unwrap().doc.clone();
    let back = vectorcraft_format::load(&vectorcraft_format::save(&d, false)).unwrap();
    let t = back.node(g).unwrap().trace.clone().unwrap();
    assert_eq!((t["preset"].as_str(), t["params"]["colors"].as_u64()), (Some("Custom"), Some(9)));
    // Expanded traces are plain groups.
    s.execute("imageTrace.expand", &json!({})).unwrap();
    let st = s.doc().unwrap();
    let id = st.selection.in_paint_order(&st.doc)[0];
    assert!(st.doc.node(id).unwrap().trace.is_none());
}
