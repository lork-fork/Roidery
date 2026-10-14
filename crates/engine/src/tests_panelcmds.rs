//! Panel-support commands (`cmd/{panelcmds,gradient,appearance,stroke,swatch,style}.rs`).

use serde_json::json;
use vectorcraft_color::{GradientKind, Paint};
use vectorcraft_doc::{AppearanceItem, NodeKind};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
    s
}

fn rect(s: &mut Session) -> NodeId {
    let r = s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 100, "height": 50})).unwrap();
    NodeId(r["id"].as_u64().unwrap())
}

fn node(s: &Session, id: NodeId) -> vectorcraft_doc::Node {
    s.doc().unwrap().doc.node(id).unwrap().clone()
}

#[test]
fn edit_gradient_converts_and_keeps_stops() {
    let mut s = session();
    let id = rect(&mut s);
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    s.execute(
        "paint.editGradient",
        &json!({"stroke": false, "kind": "radial", "stops": [
            {"offset": 1.0, "color": "#0000ff"},
            {"offset": 0.0, "color": "#ff0000", "opacity": 50, "midpoint": 0.3},
            {"offset": 0.5, "color": "#00ff00"}]}),
    )
    .unwrap();
    let Paint::Gradient(g) = node(&s, id).appearance.fill_paint() else { panic!("not a gradient") };
    assert_eq!(g.gradient.kind, GradientKind::Radial);
    assert_eq!(g.gradient.stops.len(), 3);
    assert_eq!(g.gradient.stops[0].color.to_hex(), "#ff0000");
    assert!((g.gradient.stops[0].opacity - 0.5).abs() < 1e-6);
    assert!((g.gradient.stops[0].midpoint - 0.3).abs() < 1e-6);
    // One undo step.
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(node(&s, id).appearance.fill_paint().color().is_some());
}

#[test]
fn edit_gradient_angle_reverse_and_errors() {
    let mut s = session();
    let id = rect(&mut s);
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    s.execute("paint.editGradient", &json!({"stroke": false, "angle": 450})).unwrap();
    let Paint::Gradient(g) = node(&s, id).appearance.fill_paint() else { panic!() };
    assert!((g.angle - 90.0).abs() < 1e-9);
    s.execute("paint.editGradient", &json!({"stroke": false, "reverse": true})).unwrap();
    let Paint::Gradient(g) = node(&s, id).appearance.fill_paint() else { panic!() };
    assert_eq!(g.gradient.stops[0].color.to_hex(), "#000000");
    assert!(s.execute("paint.editGradient", &json!({"stops": [{"offset": 0, "color": "#fff"}]})).is_err());
    assert!(s.execute("paint.editGradient", &json!({"kind": "conic"})).is_err());
}

#[test]
fn appearance_duplicate_and_move() {
    let mut s = session();
    let id = rect(&mut s);
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    let n0 = node(&s, id).appearance.items.len();
    s.execute("appearance.duplicateItem", &json!({"index": 0})).unwrap();
    assert_eq!(node(&s, id).appearance.items.len(), n0 + 1);
    let last = node(&s, id).appearance.items.len() - 1;
    s.execute("appearance.moveItem", &json!({"from": last, "to": 0})).unwrap();
    assert!(matches!(node(&s, id).appearance.items[0], AppearanceItem::Stroke(_)));
    assert!(s.execute("appearance.duplicateItem", &json!({"index": 99})).is_err());
}

#[test]
fn artboards_reorder_and_duplicate() {
    let mut s = session();
    s.execute("artboard.new", &json!({"name": "Second"})).unwrap();
    let r = s.execute("artboard.duplicate", &json!({"index": 0})).unwrap();
    assert_eq!(r["index"], 2);
    let d = &s.doc().unwrap().doc;
    assert_eq!(d.artboards.len(), 3);
    assert!(d.artboards[2].rect.x0 >= d.artboards[1].rect.x1);
    s.execute("artboard.reorder", &json!({"index": 1, "to": 0})).unwrap();
    assert_eq!(s.doc().unwrap().doc.artboards[0].name, "Second");
}

#[test]
fn swatch_groups_duplicate_sort() {
    let mut s = session();
    s.execute("swatch.new", &json!({"name": "Zed", "color": "#123456"})).unwrap();
    s.execute("swatch.new", &json!({"name": "Alpha", "color": "#654321"})).unwrap();
    let r = s.execute("swatch.newGroup", &json!({"name": "Mine", "swatches": ["Zed"], "colors": ["#ff0000"]})).unwrap();
    assert_eq!(r["name"], "Mine");
    let d = &s.doc().unwrap().doc;
    let g = d.swatch_groups.iter().find(|g| g.name == "Mine").unwrap();
    assert_eq!(g.swatches.len(), 2);
    assert!(!d.swatches.iter().any(|x| x.name == "Zed"));
    let r = s.execute("swatch.duplicate", &json!({"name": "Alpha"})).unwrap();
    assert_eq!(r["name"], "Alpha copy");
    s.execute("swatch.sortByName", &json!({})).unwrap();
    let d = &s.doc().unwrap().doc;
    assert!(d.swatches[0].name.starts_with('['));
    let names: Vec<_> = d.swatches.iter().skip(1).map(|x| x.name.to_lowercase()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn graphic_style_delete_duplicate() {
    let mut s = session();
    let id = rect(&mut s);
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    s.execute("graphicStyle.new", &json!({"name": "S1"})).unwrap();
    assert_eq!(s.execute("graphicStyle.duplicate", &json!({"name": "S1"})).unwrap()["name"], "S1 copy");
    s.execute("graphicStyle.delete", &json!({"name": "S1"})).unwrap();
    let d = &s.doc().unwrap().doc;
    assert!(d.graphic_styles.iter().all(|g| g.name != "S1"));
    assert!(s.execute("graphicStyle.delete", &json!({"name": "nope"})).is_err());
}

#[test]
fn text_set_format() {
    let mut s = session();
    let r = s.execute("text.create", &json!({"x": 10, "y": 100, "text": "Hello"})).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    s.execute(
        "text.setFormat",
        &json!({"ids": [id.0], "kerning": 20, "baselineShift": 3, "hScale": 120, "rotation": 370, "underline": true, "spaceBefore": 6, "hyphenate": true}),
    )
    .unwrap();
    let NodeKind::Text(t) = node(&s, id).kind else { panic!() };
    let st = t.first_style();
    assert_eq!(st.kerning, Some(20.0));
    assert_eq!(st.baseline_shift, 3.0);
    assert_eq!(st.h_scale, 120.0);
    assert!((st.rotation - 10.0).abs() < 1e-9);
    assert!(st.underline);
    assert_eq!(t.para.space_before, 6.0);
    assert!(t.para.hyphenate);
    s.execute("text.setFormat", &json!({"ids": [id.0], "kerning": "auto"})).unwrap();
    let NodeKind::Text(t) = node(&s, id).kind else { panic!() };
    assert_eq!(t.first_style().kerning, None);
    assert!(s.execute("text.setFormat", &json!({"ids": [id.0]})).is_err());
}

#[test]
fn stroke_advanced_scale_swap_flip() {
    let mut s = session();
    let id = rect(&mut s);
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    s.execute("stroke.set", &json!({"startArrow": "Triangle", "profile": "taperEnd"})).unwrap();
    s.execute("stroke.setAdvanced", &json!({"arrowScale": [50, 200]})).unwrap();
    s.execute("stroke.setAdvanced", &json!({"swapArrows": true, "flipProfile": "along"})).unwrap();
    let n = node(&s, id);
    let st = n.appearance.stroke().unwrap();
    assert_eq!(st.start_arrow, None);
    assert_eq!(st.end_arrow, Some(vectorcraft_doc::Arrowhead::Triangle));
    assert_eq!(st.arrow_scale, (200.0, 50.0));
    assert_eq!(st.profile.as_ref().unwrap().points, vectorcraft_doc::WidthProfile::taper_start().points);
    assert!(s.execute("stroke.setAdvanced", &json!({})).is_err());
    assert!(s.execute("stroke.setAdvanced", &json!({"flipProfile": "sideways"})).is_err());
}
