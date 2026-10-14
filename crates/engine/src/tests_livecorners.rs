//! Live Corners on live rectangles: `object.setLiveShape` rounds the given corners, the
//! Direct-Selected ones or all four, sets corner kinds, and keeps selected corners selected.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use vectorcraft_doc::{AnchorRef, LiveShape, NodeKind};
use vectorcraft_geom::shapes::CornerKind;

use super::*;

/// A 100 × 60 pt live rectangle at (10, 10), selected.
fn session() -> (Session, NodeId) {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 400})).unwrap();
    let id = s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 100, "height": 60})).unwrap()["id"].as_u64().unwrap();
    (s, NodeId(id))
}

fn run(s: &mut Session, p: Value) {
    s.execute("object.setLiveShape", &p).unwrap_or_else(|e| panic!("{p}: {e}"));
}

fn corners(s: &Session, id: NodeId) -> ([f64; 4], [CornerKind; 4], usize) {
    let n = s.doc().unwrap().doc.node(id).unwrap();
    let NodeKind::Path { live: Some(LiveShape::Rectangle { radii, kinds, .. }), path, .. } = &n.kind else { panic!("not a live rectangle") };
    (*radii, *kinds, path.anchor_count())
}

fn selected_anchors(s: &Session, id: NodeId) -> Option<BTreeSet<AnchorRef>> {
    s.doc().unwrap().selection.partial(id).cloned()
}

fn anchors(v: &[usize]) -> BTreeSet<AnchorRef> {
    v.iter().map(|ai| (0, *ai)).collect()
}

#[test]
fn a_whole_rectangle_rounds_all_corners_and_given_corners_alone() {
    let (mut s, id) = session();
    run(&mut s, json!({"radius": 8}));
    assert_eq!(corners(&s, id), ([8.0; 4], [CornerKind::Round; 4], 8));
    run(&mut s, json!({"id": id.0, "radius": 0}));
    // One corner: only it rounds, and the path gains one anchor (one undo step).
    let undo = s.doc().unwrap().history.undo.len();
    run(&mut s, json!({"ids": [id.0], "corners": [1], "radius": 12}));
    assert_eq!(corners(&s, id), ([0.0, 12.0, 0.0, 0.0], [CornerKind::Round; 4], 5));
    assert_eq!(s.doc().unwrap().history.undo.len(), undo + 1);
    let b = s.doc().unwrap().doc.node(id).unwrap().geometric_bounds().unwrap();
    assert!((b.x1 - 110.0).abs() < 1e-9 && (b.y0 - 10.0).abs() < 1e-9, "the shape keeps its bounds: {b:?}");
    // Kinds alone, then both; the others stay as they were.
    run(&mut s, json!({"corners": [1, 3], "kind": "chamfer"}));
    run(&mut s, json!({"corners": [3], "radius": 5, "kind": "invertedRound"}));
    assert_eq!(corners(&s, id), ([0.0, 12.0, 0.0, 5.0], [CornerKind::Round, CornerKind::Chamfer, CornerKind::Round, CornerKind::InvertedRound], 6));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(corners(&s, id).1[3], CornerKind::Chamfer);
}

#[test]
fn bad_corners_and_kinds_are_errors() {
    let (mut s, id) = session();
    for p in
        [json!({"corners": [4], "radius": 5}), json!({"corners": ["a"]}), json!({"corners": 1}), json!({"corners": [-1]}), json!({"kind": "wavy"})]
    {
        assert!(s.execute("object.setLiveShape", &p).is_err(), "{p}");
    }
    assert_eq!(corners(&s, id), ([0.0; 4], [CornerKind::Round; 4], 4), "nothing changed");
}

#[test]
fn direct_selected_corners_round_alone_and_stay_selected() {
    let (mut s, id) = session();
    // Direct Selection picks the top-right and bottom-left corners.
    s.execute("select.anchors", &json!({"id": id.0, "anchors": [[0, 1], [0, 3]], "mode": "set"})).unwrap();
    run(&mut s, json!({"radius": 10}));
    assert_eq!(corners(&s, id), ([0.0, 10.0, 0.0, 10.0], [CornerKind::Round; 4], 6));
    // Anchors: top-left, top-right's two, bottom-right, bottom-left's two.
    assert_eq!(selected_anchors(&s, id), Some(anchors(&[1, 2, 4, 5])));
    // The same corners again (a field in the Properties panel), now with their two anchors each.
    run(&mut s, json!({"kind": "chamfer"}));
    assert_eq!(corners(&s, id).1, [CornerKind::Round, CornerKind::Chamfer, CornerKind::Round, CornerKind::Chamfer]);
    run(&mut s, json!({"radius": 0}));
    assert_eq!(corners(&s, id).2, 4);
    assert_eq!(selected_anchors(&s, id), Some(anchors(&[1, 3])));
    // Undo brings back the selection with the shape.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(selected_anchors(&s, id), Some(anchors(&[1, 2, 4, 5])));
    // Explicit corners win over the selection, which follows its own corners.
    run(&mut s, json!({"corners": [0], "radius": 4}));
    assert_eq!(corners(&s, id).0, [4.0, 10.0, 0.0, 10.0]);
    assert_eq!(selected_anchors(&s, id), Some(anchors(&[1, 2, 4, 5])), "the top-left corner comes first and last");
}

#[test]
fn a_drag_previews_one_corner_and_commits_one_step() {
    let (mut s, id) = session();
    s.execute("select.anchors", &json!({"id": id.0, "anchors": [[0, 2]], "mode": "set"})).unwrap();
    let undo = s.doc().unwrap().history.undo.len();
    s.begin_interaction("Corner Radius").unwrap();
    for r in [5.0, 9.0, 14.0] {
        s.preview("object.setLiveShape", &json!({"id": id.0, "radius": r, "corners": [2]})).unwrap();
    }
    s.commit_interaction().unwrap();
    assert_eq!(corners(&s, id), ([0.0, 0.0, 14.0, 0.0], [CornerKind::Round; 4], 5));
    assert_eq!(selected_anchors(&s, id), Some(anchors(&[2, 3])));
    assert_eq!(s.doc().unwrap().history.undo.len(), undo + 1);
}
