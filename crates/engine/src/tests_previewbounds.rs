//! Use Preview Bounds: the Transform panel values, the bounding box, Align and Distribute measure
//! visual bounds (strokes included) when it is on.

use serde_json::{Value, json};
use vectorcraft_doc::NodeId;
use vectorcraft_geom::Rect;
use vectorcraft_tools::{PointerEvent, PointerKind};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 400})).unwrap();
    s
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A rectangle with a `weight` pt stroke (0: no stroke).
fn rect(s: &mut Session, x: f64, w: f64, weight: f64) -> NodeId {
    let id = NodeId(run(s, "shape.rectangle", json!({"x": x, "y": 100, "width": w, "height": 50}))["id"].as_u64().unwrap());
    if weight > 0.0 {
        run(s, "stroke.set", json!({"weight": weight}));
    } else {
        run(s, "paint.setStroke", json!({"none": true}));
    }
    id
}

fn geometric(s: &Session, id: NodeId) -> Rect {
    s.doc().unwrap().doc.node(id).unwrap().geometric_bounds().unwrap()
}

fn preview(s: &mut Session, on: bool) {
    run(s, "prefs.set", json!({"key": "usePreviewBounds", "value": on}));
}

/// Two rectangles: one with a 10 pt stroke at x = 100, an unstroked one at x = 20; align left.
fn aligned_left(on: bool, p: Value) -> f64 {
    let mut s = session();
    let a = rect(&mut s, 100.0, 50.0, 10.0);
    rect(&mut s, 20.0, 50.0, 0.0);
    preview(&mut s, on);
    run(&mut s, "select.all", json!({}));
    let mut p = p;
    p["horizontal"] = json!("left");
    run(&mut s, "object.align", p);
    geometric(&s, a).x0
}

#[test]
fn align_left_differs_by_half_the_stroke() {
    assert_eq!(aligned_left(false, json!({})), 20.0);
    // The 10 pt stroke's outer edge lines up with the other object: the path sits 5 pt in.
    assert_eq!(aligned_left(true, json!({})), 25.0);
    // `bounds` overrides the preference either way.
    assert_eq!(aligned_left(true, json!({"bounds": "geometric"})), 20.0);
    assert_eq!(aligned_left(false, json!({"bounds": "preview"})), 25.0);
    let mut s = session();
    rect(&mut s, 0.0, 10.0, 0.0);
    assert!(s.execute("object.align", &json!({"horizontal": "left", "bounds": "visual"})).is_err());
}

#[test]
fn distribute_spacing_measures_preview_bounds() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 50.0, 10.0);
    let b = rect(&mut s, 200.0, 50.0, 10.0);
    preview(&mut s, true);
    run(&mut s, "select.set", json!({"ids": [a.0, b.0]}));
    run(&mut s, "object.distributeSpacing", json!({"axis": "horizontal", "spacing": 0}));
    // The strokes touch: the paths are a stroke weight apart.
    assert_eq!(geometric(&s, b).x0 - geometric(&s, a).x1, 10.0);
}

#[test]
fn the_w_field_sets_the_visual_width() {
    let mut s = session();
    let id = rect(&mut s, 100.0, 100.0, 10.0);
    preview(&mut s, true);
    assert_eq!(s.transform_bounds(&[id]).unwrap().width(), 110.0);
    // Strokes scale: the whole visual box doubles, from its left edge.
    run(&mut s, "object.setBounds", json!({"width": 220, "reference": 0, "proportional": true, "strokes": true}));
    let v = s.transform_bounds(&[id]).unwrap();
    assert!((v.width() - 220.0).abs() < 1e-9 && (v.x0 - 95.0).abs() < 1e-9, "{v:?}");
    assert!((geometric(&s, id).width() - 200.0).abs() < 1e-9);
    run(&mut s, "edit.undo", json!({}));
    // Strokes kept: the path takes the difference.
    run(&mut s, "object.setBounds", json!({"width": 220, "reference": 0, "strokes": false}));
    let v = s.transform_bounds(&[id]).unwrap();
    assert!((v.width() - 220.0).abs() < 1e-9 && (v.x0 - 95.0).abs() < 1e-9, "{v:?}");
    assert!((geometric(&s, id).width() - 210.0).abs() < 1e-9);
    // X places the visual box's reference point.
    run(&mut s, "object.setBounds", json!({"x": 0, "reference": 0}));
    assert!((s.transform_bounds(&[id]).unwrap().x0).abs() < 1e-9);
    assert!((geometric(&s, id).x0 - 5.0).abs() < 1e-9);
}

#[test]
fn a_size_below_what_a_kept_stroke_adds_gives_the_path_no_width_or_height() {
    // The 10 pt stroke adds 10 pt to the width and the height. With strokes kept, a width or height
    // of 0 or more below 10 pt gives the path no width or height, and the visual box is the
    // stroke's 10 pt: the path does not turn over. A size of 10 pt or more scales the path to it,
    // and a negative size turns the path over.
    let mut wrong = vec![];
    for (key, size, path, visual) in [
        ("width", 5.0, 0.0, 10.0),
        ("width", 0.0, 0.0, 10.0),
        ("width", 10.0, 0.0, 10.0),
        ("width", 30.0, 20.0, 30.0),
        ("width", -10.0, 20.0, 30.0),
        ("height", 5.0, 0.0, 10.0),
        ("height", 0.0, 0.0, 10.0),
        ("height", 10.0, 0.0, 10.0),
        ("height", 30.0, 20.0, 30.0),
        ("height", -10.0, 20.0, 30.0),
    ] {
        let mut s = session();
        let id = rect(&mut s, 100.0, 100.0, 10.0);
        preview(&mut s, true);
        run(&mut s, "object.setBounds", json!({key: size, "reference": 0, "strokes": false}));
        let (g, v) = (geometric(&s, id), s.transform_bounds(&[id]).unwrap());
        let (g, v) = if key == "width" { (g.width(), v.width()) } else { (g.height(), v.height()) };
        if (g - path).abs() > 1e-9 || (v - visual).abs() > 1e-9 {
            wrong.push(format!("{key} {size}: path {g}, visual box {v}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:?}");
}

#[test]
fn set_bounds_gives_one_path_sizes_out_of_proportion_when_strokes_scale() {
    // The 100 × 50 rectangle with a 10 pt stroke: its visual box runs from (95, 95) to (205, 155).
    // The stroke scales by the square root of the product of the two scales, so the width it adds
    // depends on the height too. Set from the top-left corner, the visual box is 220 × 60, and
    // 5 × 1000 with the path the same way round.
    let mut wrong = vec![];
    for (width, height) in [(220.0, 60.0), (5.0, 1000.0)] {
        let mut s = session();
        let id = rect(&mut s, 100.0, 100.0, 10.0);
        preview(&mut s, true);
        run(&mut s, "object.setBounds", json!({"width": width, "height": height, "reference": 0, "strokes": true}));
        let v = s.transform_bounds(&[id]).unwrap();
        let Some(vectorcraft_doc::LiveShape::Rectangle { xf, .. }) = s.doc().unwrap().doc.node(id).unwrap().live_shape() else {
            panic!("the rectangle")
        };
        if !close(v, Rect::new(95.0, 95.0, 95.0 + width, 95.0 + height)) || xf.as_coeffs()[0] <= 0.0 {
            wrong.push(format!("{width} × {height}: {v:?}, {xf:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:?}");
}

fn close(a: Rect, b: Rect) -> bool {
    [(a.x0, b.x0), (a.y0, b.y0), (a.x1, b.x1), (a.y1, b.y1)].iter().all(|(x, y)| (x - y).abs() < 1e-9)
}

/// Two rectangles 50 pt high at y 100, both selected with Use Preview Bounds on: one from x 0 to
/// 10 with a 2 pt stroke, which sets the left edge of the paths, and one from x 3 to 10 with a
/// 10 pt stroke, which sets every edge of the visual box, (-2, 95) to (15, 155).
fn thin_and_thick() -> (Session, [NodeId; 2]) {
    let mut s = session();
    let ids = [rect(&mut s, 0.0, 10.0, 2.0), rect(&mut s, 3.0, 7.0, 10.0)];
    preview(&mut s, true);
    run(&mut s, "select.set", json!({"ids": [ids[0].0, ids[1].0]}));
    assert!(close(s.transform_bounds(&ids).unwrap(), Rect::new(-2.0, 95.0, 15.0, 155.0)));
    (s, ids)
}

#[test]
fn set_bounds_measures_the_margins_on_each_side_object_by_object() {
    // Set to 37 wide from the left edge with strokes kept, the paths scale by 3.1, and the left edge
    // of the visual box stays at -2.
    let (mut s, ids) = thin_and_thick();
    run(&mut s, "object.setBounds", json!({"width": 37, "reference": 0, "strokes": false}));
    let v = s.transform_bounds(&ids).unwrap();
    assert!(close(v, Rect::new(-2.0, 95.0, 35.0, 155.0)), "{v:?}");
    assert!(close(geometric(&s, ids[0]), Rect::new(-1.0, 100.0, 30.0, 150.0)), "{:?}", geometric(&s, ids[0]));
    assert!(close(geometric(&s, ids[1]), Rect::new(8.3, 100.0, 30.0, 150.0)), "{:?}", geometric(&s, ids[1]));
}

#[test]
fn set_bounds_scales_both_ways_at_once_when_strokes_scale_and_the_sizes_are_out_of_proportion() {
    // Each stroke scales by the square root of the product of the two scales, so the width it
    // adds depends on the height too. The visual box is 40 × 100.
    let (mut s, ids) = thin_and_thick();
    run(&mut s, "object.setBounds", json!({"width": 40, "height": 100, "reference": 0, "strokes": true}));
    let v = s.transform_bounds(&ids).unwrap();
    assert!(close(v, Rect::new(-2.0, 95.0, 38.0, 195.0)), "{v:?}");
}

#[test]
fn set_bounds_measures_paths_turned_together_square_to_their_box() {
    // Turned 30° together, the paths are measured square to their turned box: it is 37 wide, and
    // its top-left corner stays where it was.
    let (mut s, ids) = thin_and_thick();
    run(&mut s, "object.rotate", json!({"angle": 30}));
    let before = s.transform_box(&ids).unwrap();
    run(&mut s, "object.setBounds", json!({"width": 37, "reference": 0, "strokes": false}));
    let after = s.transform_box(&ids).unwrap();
    assert!((after.angle - 30.0).abs() < 1e-9 && (after.rect.width() - 37.0).abs() < 1e-9, "{after:?}");
    assert!((after.rect.height() - 60.0).abs() < 1e-9, "{after:?}");
    assert!(after.reference_point(0).distance(before.reference_point(0)) < 1e-9, "{before:?} {after:?}");
}

#[test]
fn a_size_below_what_the_kept_strokes_of_several_paths_add_gives_them_no_width() {
    // With no width, the paths have a visual box 10 pt wide, the width of the thick stroke. A width
    // of 0 or more below 10 gives both paths no width, and the left edge of the visual box stays at
    // -2. A negative width turns the paths over, as it does one path.
    let mut wrong = vec![];
    for (width, paths, visual) in [
        (8.0, [Rect::new(3.0, 100.0, 3.0, 150.0); 2], Rect::new(-2.0, 95.0, 8.0, 155.0)),
        (0.0, [Rect::new(3.0, 100.0, 3.0, 150.0); 2], Rect::new(-2.0, 95.0, 8.0, 155.0)),
        (-37.0, [Rect::new(-44.0, 100.0, 0.0, 150.0), Rect::new(-44.0, 100.0, -13.2, 150.0)], Rect::new(-49.0, 95.0, 1.0, 155.0)),
    ] {
        let (mut s, ids) = thin_and_thick();
        run(&mut s, "object.setBounds", json!({"width": width, "reference": 0, "strokes": false}));
        let (g, v) = (ids.map(|id| geometric(&s, id)), s.transform_bounds(&ids).unwrap());
        if !close(g[0], paths[0]) || !close(g[1], paths[1]) || !close(v, visual) {
            wrong.push(format!("width {width}: paths {g:?}, visual box {v:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:?}");
}

#[test]
fn set_bounds_leaves_the_height_of_lines_with_no_height() {
    // Horizontal lines at y 100 with square caps, from x 0 to 10 with a 2 pt stroke and from x 3 to
    // 10 with a 10 pt stroke: the visual box is (-2, 95) to (15, 105). They have no height to
    // scale, so the height stays 10; the width becomes 37 from the left edge.
    let mut s = session();
    let mut ids = vec![];
    for (x, weight) in [(0.0, 2.0), (3.0, 10.0)] {
        ids.push(NodeId(run(&mut s, "shape.line", json!({"x1": x, "y1": 100, "x2": 10, "y2": 100}))["id"].as_u64().unwrap()));
        run(&mut s, "stroke.set", json!({"weight": weight, "cap": "square"}));
    }
    preview(&mut s, true);
    run(&mut s, "select.set", json!({"ids": [ids[0].0, ids[1].0]}));
    assert!(close(s.transform_bounds(&ids).unwrap(), Rect::new(-2.0, 95.0, 15.0, 105.0)), "{:?}", s.transform_bounds(&ids));
    run(&mut s, "object.setBounds", json!({"width": 37, "height": 30, "reference": 0, "strokes": false}));
    let v = s.transform_bounds(&ids).unwrap();
    assert!(close(v, Rect::new(-2.0, 95.0, 35.0, 105.0)), "{v:?}");
    assert!(close(geometric(&s, ids[1]), Rect::new(8.3, 100.0, 30.0, 100.0)), "{:?}", geometric(&s, ids[1]));
}

/// The rectangles of [`thin_and_thick`] in a group, the thick one in a group of its own inside it
/// when `nested`, selected with an unstroked rectangle from x 100 to 110 when `plain`. Returns the
/// selected objects.
fn grouped(nested: bool, plain: bool) -> (Session, Vec<NodeId>) {
    let (mut s, [thin, thick]) = thin_and_thick();
    let mut members = vec![thin.0, thick.0];
    if nested {
        run(&mut s, "select.set", json!({"ids": [thick.0]}));
        members[1] = run(&mut s, "object.group", json!({}))["id"].as_u64().unwrap();
    }
    run(&mut s, "select.set", json!({"ids": members}));
    let mut ids = vec![NodeId(run(&mut s, "object.group", json!({}))["id"].as_u64().unwrap())];
    if plain {
        ids.push(rect(&mut s, 100.0, 10.0, 0.0));
    }
    run(&mut s, "select.set", json!({"ids": ids.iter().map(|id| id.0).collect::<Vec<_>>()}));
    (s, ids)
}

#[test]
fn set_bounds_measures_the_members_of_groups_one_by_one() {
    // With the unstroked rectangle, the visual box runs from (-2, 95) to (110, 155). Set to 224 wide
    // from its left edge, it is 224 wide and its left edge stays at -2, with strokes kept or
    // scaling, and with the thick rectangle in a group of its own. The group alone, (-2, 95) to
    // (15, 155), set to 37 wide is 37 wide.
    let mut wrong = vec![];
    for (nested, plain, width, strokes) in
        [(false, true, 224.0, false), (false, true, 224.0, true), (true, true, 224.0, false), (false, false, 37.0, false)]
    {
        let (mut s, ids) = grouped(nested, plain);
        run(&mut s, "object.setBounds", json!({"width": width, "reference": 0, "strokes": strokes}));
        let v = s.transform_bounds(&ids).unwrap();
        if !close(v, Rect::new(-2.0, 95.0, width - 2.0, 155.0)) {
            wrong.push(format!("nested {nested}, plain {plain}, strokes {strokes}: {v:?}"));
        }
    }
    // The group with a 1 pt stroke of its own, which reaches 5 pt at its miter limit of 10, around
    // each member: the visual box runs from (-7, 90) to (110, 160). Set to 224 wide from its left
    // edge, it is 224 wide and its left edge stays at -7, with strokes kept or scaling.
    for strokes in [false, true] {
        let (mut s, _) = thin_and_thick();
        let group = NodeId(run(&mut s, "object.group", json!({}))["id"].as_u64().unwrap());
        run(&mut s, "appearance.addStroke", json!({"ids": [group.0]}));
        let ids = [group, rect(&mut s, 100.0, 10.0, 0.0)];
        run(&mut s, "select.set", json!({"ids": [ids[0].0, ids[1].0]}));
        assert!(close(s.transform_bounds(&ids).unwrap(), Rect::new(-7.0, 90.0, 110.0, 160.0)), "{:?}", s.transform_bounds(&ids));
        run(&mut s, "object.setBounds", json!({"width": 224, "reference": 0, "strokes": strokes}));
        let v = s.transform_bounds(&ids).unwrap();
        if !close(v, Rect::new(-7.0, 90.0, 217.0, 160.0)) {
            wrong.push(format!("the group with a stroke, strokes {strokes}: {v:?}"));
        }
    }
    // A group of a rectangle from x 0 to 100 with a 2 pt stroke and one from x 40 to 60 with a 10 pt
    // stroke: its visual box, (-1, 95) to (101, 155), set to 11 wide is 11 wide.
    let mut s = session();
    let ids = [rect(&mut s, 0.0, 100.0, 2.0), rect(&mut s, 40.0, 20.0, 10.0)];
    preview(&mut s, true);
    run(&mut s, "select.set", json!({"ids": [ids[0].0, ids[1].0]}));
    let group = NodeId(run(&mut s, "object.group", json!({}))["id"].as_u64().unwrap());
    run(&mut s, "object.setBounds", json!({"width": 11, "reference": 0, "strokes": false}));
    let v = s.transform_bounds(&[group]).unwrap();
    if !close(v, Rect::new(-1.0, 95.0, 10.0, 155.0)) {
        wrong.push(format!("the 0 to 100 and 40 to 60 group: {v:?}"));
    }
    assert!(wrong.is_empty(), "{wrong:?}");
}

#[test]
fn set_bounds_measures_clip_groups_and_groups_with_effects_as_one_object() {
    // A clip group is measured by its clipping path, a rectangle from x 0 to 10, though its content
    // with a 10 pt stroke reaches past it: with the unstroked rectangle from x 100 to 110, set to
    // 224 wide from the left edge, the visual box runs from x 0 to 224.
    let mut s = session();
    let content = rect(&mut s, 3.0, 17.0, 10.0);
    let path = rect(&mut s, 0.0, 10.0, 0.0);
    run(&mut s, "select.set", json!({"ids": [content.0, path.0]}));
    let clip = NodeId(run(&mut s, "object.clippingMask.make", json!({}))["id"].as_u64().unwrap());
    let plain = rect(&mut s, 100.0, 10.0, 0.0);
    preview(&mut s, true);
    run(&mut s, "select.set", json!({"ids": [clip.0, plain.0]}));
    run(&mut s, "object.setBounds", json!({"width": 224, "reference": 0, "strokes": false}));
    let v = s.transform_bounds(&[clip, plain]).unwrap();
    assert!(close(v, Rect::new(0.0, 100.0, 224.0, 150.0)), "the clip group: {v:?}");
    // A group with a drop shadow of its own is measured as one object, as before. Its 2 pt margin on
    // the left is measured from the group's left edge, though the stroke of the thick rectangle at
    // x 3 paints it, so the visual box is 223 wide and its left edge moves to -1.
    let (mut s, ids) = grouped(false, true);
    run(&mut s, "effect.apply", json!({"effect": "stylize.dropShadow", "ids": [ids[0].0]}));
    run(&mut s, "select.set", json!({"ids": [ids[0].0, ids[1].0]}));
    run(&mut s, "object.setBounds", json!({"width": 224, "reference": 0, "strokes": false}));
    let v = s.transform_bounds(&ids).unwrap();
    assert!(close(v, Rect::new(-1.0, 95.0, 222.0, 155.0)), "the group with a shadow: {v:?}");
}

#[test]
fn the_bounding_box_handles_sit_on_the_preview_bounds() {
    // Drag from the visual right edge (10 pt outside the path) 100 pt to the right.
    let drag = |on: bool| {
        let mut s = session();
        let id = rect(&mut s, 100.0, 100.0, 20.0);
        preview(&mut s, on);
        let v = ViewInfo::default();
        s.select_tool("selection", v).unwrap();
        for (k, x) in [(PointerKind::Down, 210.0), (PointerKind::Drag, 310.0), (PointerKind::Up, 310.0)] {
            s.pointer(&PointerEvent::new(k, x, 125.0), v).unwrap();
        }
        geometric(&s, id)
    };
    assert!(drag(true).width() > 150.0, "a scale from the visual edge");
    assert_eq!(drag(false).width(), 100.0, "off, the point is no handle");
}
