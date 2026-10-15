//! Roidery embroidery commands (`stitch.set`, `stitch.info`, `stitch.export`).

use roidery_formats::{Command, Format};
use serde_json::{Value, json};

use super::*;
/// Millimetres per point.
const MM_PER_PT: f64 = 25.4 / 72.0;

/// A new document whose shapes are filled and not stroked, so each sews one part.
fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
    s.execute("paint.setStroke", &json!({"none": true})).unwrap();
    s
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap()
}

fn rect(s: &mut Session, x: f64) -> NodeId {
    NodeId(run(s, "shape.rectangle", json!({"x": x, "y": 10, "width": 72, "height": 72}))["id"].as_u64().unwrap())
}

fn exported(s: &mut Session) -> roidery_formats::Design {
    let out = run(s, "stitch.export", json!({}));
    let bytes = vectorcraft_format::base64_decode(out["dataBase64"].as_str().unwrap()).unwrap();
    Format::Dst.read(&bytes).unwrap()
}

#[test]
fn filled_shapes_are_tatami_by_default_and_export_to_scale() {
    let mut s = session();
    rect(&mut s, 10.0);
    let i = run(&mut s, "stitch.info", json!({}));
    assert_eq!(i["objects"][0]["type"], "tatami");
    assert_eq!(i["objects"][0]["auto"], true);
    let d = exported(&mut s);
    assert!(d.stitch_count() > 500);
    // 72 pt is 25.4 mm; the fill adds 0.2 mm pull compensation each side.
    let b = d.bounds().unwrap();
    assert!((b.width() - 25.8).abs() < 0.25, "{}", b.width());
    assert!((b.height() - 25.4).abs() < 0.25, "{}", b.height());
    assert_eq!(d.stitches.last().map(|s| s.command), Some(Command::End));
}

#[test]
fn set_changes_type_and_settings_in_one_undo_step() {
    let mut s = session();
    let a = rect(&mut s, 10.0);
    let r = run(&mut s, "stitch.set", json!({"type": "Satin", "settings": {"satin": {"width": 4, "underlays": []}}}));
    assert_eq!(r["changed"], 1);
    let i = run(&mut s, "stitch.info", json!({}));
    let o = &i["objects"][0];
    assert_eq!((o["type"].clone(), o["auto"].clone(), o["settings"]["satin"]["width"].clone()), (json!("satin"), json!(false), json!(4.0)));
    // Merged: an untouched value keeps its default.
    assert_eq!(o["settings"]["satin"]["density"], 0.4);
    // A satin on a shape with no stroke sews in its fill colour.
    assert_eq!(o["thread"], json!([255, 255, 255]));
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc().unwrap().doc.node(a).unwrap().extras().attrs.is_none());
    run(&mut s, "edit.redo", json!({}));
    run(&mut s, "stitch.set", json!({"reset": true}));
    assert_eq!(run(&mut s, "stitch.info", json!({}))["objects"][0]["auto"], true);
}

#[test]
fn bad_input_is_an_error_and_changes_nothing() {
    let mut s = session();
    let a = rect(&mut s, 10.0);
    assert!(s.execute("stitch.set", &json!({"type": "lace"})).is_err());
    assert!(s.execute("stitch.set", &json!({"settings": {"fill": {"rowSpacing": "wide"}}})).is_err());
    assert!(s.execute("stitch.set", &json!({"settings": 3})).is_err());
    assert!(s.execute("stitch.set", &json!({})).is_err());
    assert!(s.doc().unwrap().doc.node(a).unwrap().extras().attrs.is_none());
    assert!(s.execute("stitch.export", &json!({"format": "vp9"})).is_err());
    // Absurd values are clamped when stitching, never hang or crash.
    run(&mut s, "stitch.set", json!({"settings": {"fill": {"rowSpacing": 0, "stitchLength": -5, "angle": 1e300}}}));
    assert!(exported(&mut s).stitch_count() > 0);
}

#[test]
fn satin_blocks_take_rails_from_two_subpaths() {
    let mut s = session();
    let id = NodeId(run(&mut s, "path.create", json!({"d": "M 10 10 L 110 10 M 10 30 L 110 30"}))["id"].as_u64().unwrap());
    run(&mut s, "stitch.set", json!({"ids": [id.0], "type": "satinBlocks", "settings": {"satin": {"pullCompensation": 0}}}));
    let d = exported(&mut s);
    let b = d.bounds().unwrap();
    // Rails 20 pt (7.06 mm) apart, 100 pt (35.3 mm) long.
    assert!((b.height() - 20.0 * 25.4 / 72.0).abs() < 0.15, "{}", b.height());
    assert!((b.width() - 100.0 * 25.4 / 72.0).abs() < 0.15, "{}", b.width());
}

#[test]
fn colours_change_between_objects_and_hidden_ones_are_skipped() {
    let mut s = session();
    let a = rect(&mut s, 10.0);
    let b = rect(&mut s, 200.0);
    run(&mut s, "select.set", json!({"ids": [b.0]}));
    run(&mut s, "paint.setFill", json!({"color": "#ff0000"}));
    let i = run(&mut s, "stitch.info", json!({"ids": []}));
    assert_eq!(i["colorChanges"], 1);
    // Underlay ends near where the top fill starts, and a colour change cuts the thread itself.
    assert_eq!(i["trims"], 0);
    // In the file, the long move to the second object reads back as a trim (three or more jumps
    // in a row are how DST marks one).
    assert_eq!(exported(&mut s).color_changes(), 1);
    run(&mut s, "select.set", json!({"ids": [a.0]}));
    run(&mut s, "object.hide", json!({}));
    let i = run(&mut s, "stitch.info", json!({"ids": []}));
    assert_eq!(i["objects"].as_array().unwrap().len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn export_writes_the_file() {
    let mut s = session();
    rect(&mut s, 10.0);
    let dir = std::env::temp_dir().join(format!("roidery-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Badge.dst");
    let out = run(&mut s, "stitch.export", json!({"path": path.to_string_lossy()}));
    assert_eq!(out["format"], "dst");
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"LA:Badge"));
    assert!(Format::Dst.read(&bytes).unwrap().stitch_count() > 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn create_satin_blocks_is_one_undoable_object_with_its_settings() {
    let mut s = session();
    let r = run(
        &mut s,
        "stitch.createSatinBlocks",
        json!({"left": [[10, 10], [110, 10]], "right": [[10, 30], [110, 30]], "settings": {"satin": {"density": 0.5}}}),
    );
    let id = NodeId(r["id"].as_u64().unwrap());
    let i = run(&mut s, "stitch.info", json!({"ids": [id.0]}));
    let o = &i["objects"][0];
    assert_eq!((o["type"].clone(), o["auto"].clone(), o["settings"]["satin"]["density"].clone()), (json!("satinBlocks"), json!(false), json!(0.5)));
    assert!(o["stitches"].as_u64().unwrap() > 50);
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc().unwrap().doc.node(id).is_none());
    assert!(s.execute("stitch.createSatinBlocks", &json!({"left": [], "right": [[0, 0]]})).is_err());
    assert!(s.execute("stitch.createSatinBlocks", &json!({"left": [[0, "x"]], "right": [[0, 0]]})).is_err());
}

#[test]
fn preview_is_in_document_units_with_thread_colours() {
    let mut s = session();
    rect(&mut s, 10.0);
    let pv = crate::stitch_preview(&s.doc().unwrap().doc);
    assert_eq!(pv.threads, vec![[255, 255, 255]]);
    let sewn: Vec<_> = pv.stitches.iter().filter(|p| p.sew).collect();
    assert!(sewn.len() > 500);
    // The rectangle spans 10..82 pt; pull compensation adds about half a point each side.
    assert!(sewn.iter().all(|p| p.x > 9.0 && p.x < 83.0 && p.y > 9.0 && p.y < 83.0));
}

#[test]
fn every_machine_format_exports_the_same_stitches() {
    let mut s = session();
    rect(&mut s, 10.0);
    let dst = exported(&mut s);
    for f in Format::writable() {
        let out = run(&mut s, "stitch.export", json!({"format": f.extension()}));
        assert_eq!(out["format"], f.extension());
        let bytes = vectorcraft_format::base64_decode(out["dataBase64"].as_str().unwrap()).unwrap();
        let d = f.read(&bytes).unwrap();
        assert_eq!(d.stitch_count(), dst.stitch_count(), "{}", f.label());
    }
}

/// A 72 pt square at `x` filled with `color`.
fn square(s: &mut Session, x: f64, color: &str) -> u64 {
    let id = rect(s, x).0;
    run(s, "paint.setFill", json!({"color": color, "ids": [id]}));
    id
}

fn order(s: &mut Session) -> Vec<u64> {
    run(s, "stitch.sequence", json!({}))["objects"].as_array().unwrap().iter().map(|o| o["id"].as_u64().unwrap()).collect()
}

#[test]
fn the_sequence_lists_objects_and_colour_blocks_in_sewing_order() {
    let mut s = session();
    let (a, b, c) = (square(&mut s, 0.0, "#ff0000"), square(&mut s, 80.0, "#ff0000"), square(&mut s, 160.0, "#0000ff"));
    let q = run(&mut s, "stitch.sequence", json!({}));
    assert_eq!(order(&mut s), [a, b, c]);
    let colors = q["colors"].as_array().unwrap();
    assert_eq!(colors.len(), 2);
    assert_eq!((colors[0]["thread"].clone(), colors[0]["objects"].clone()), (json!([255, 0, 0]), json!([a, b])));
    assert_eq!(q["objects"][2]["block"], 1);
    assert_eq!(q["colorChanges"], 1);
    assert_eq!(q["objects"][0]["type"], "tatami");
}

#[test]
fn moving_objects_changes_the_sewing_order_in_one_undo_step() {
    let mut s = session();
    let (a, b, c) = (square(&mut s, 0.0, "#ff0000"), square(&mut s, 80.0, "#00ff00"), square(&mut s, 160.0, "#0000ff"));
    assert_eq!(run(&mut s, "stitch.move", json!({"ids": [c], "before": a}))["moved"], 1);
    assert_eq!(order(&mut s), [c, a, b]);
    run(&mut s, "stitch.move", json!({"ids": [a, c], "after": b}));
    assert_eq!(order(&mut s), [b, c, a], "kept in their sewing order");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(order(&mut s), [c, a, b]);
    // Moving an object next to itself, or nothing, changes nothing.
    assert_eq!(run(&mut s, "stitch.move", json!({"ids": [a], "after": a}))["moved"], 0);
    for bad in
        [json!({"ids": [a]}), json!({"ids": [a], "before": b, "after": c}), json!({"ids": [999], "before": b}), json!({"ids": "x", "before": b})]
    {
        assert!(s.execute("stitch.move", &bad).is_err(), "{bad}");
    }
    assert_eq!(order(&mut s), [c, a, b]);
}

#[test]
fn sort_by_colour_sews_each_thread_once() {
    let mut s = session();
    let (a, b, c, e) =
        (square(&mut s, 0.0, "#ff0000"), square(&mut s, 80.0, "#0000ff"), square(&mut s, 160.0, "#ff0000"), square(&mut s, 240.0, "#0000ff"));
    assert_eq!(run(&mut s, "stitch.sequence", json!({}))["colorChanges"], 3);
    let r = run(&mut s, "stitch.sortColors", json!({}));
    assert_eq!((r["changed"].clone(), r["colorChanges"].clone()), (json!(true), json!(1)));
    assert_eq!(order(&mut s), [a, c, b, e]);
    assert_eq!(run(&mut s, "stitch.sortColors", json!({}))["changed"], false);
}

#[test]
fn the_fabric_sets_automatic_and_own_settings_in_one_undo_step() {
    let mut s = session();
    let (a, b) = (square(&mut s, 0.0, "#ff0000"), square(&mut s, 80.0, "#00ff00"));
    run(&mut s, "stitch.set", json!({"type": "satin", "ids": [b]}));
    assert_eq!(run(&mut s, "stitch.design", json!({}))["fabric"], "woven");
    let r = run(&mut s, "stitch.setDesign", json!({"fabric": "Knit"}));
    assert_eq!((r["fabric"].clone(), r["changed"].clone()), (json!("knit"), json!(2)));
    let i = run(&mut s, "stitch.info", json!({"ids": [a, b]}));
    assert_eq!(i["objects"][0]["id"], a);
    assert_eq!(i["objects"][0]["settings"]["fill"]["pullCompensation"], 0.3);
    assert_eq!(i["objects"][1]["settings"]["satin"]["underlays"].as_array().unwrap().len(), 2);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(run(&mut s, "stitch.design", json!({}))["fabric"], "woven");
    let i = run(&mut s, "stitch.info", json!({"ids": [a, b]}));
    assert_eq!(i["objects"][1]["settings"]["satin"]["pullCompensation"], 0.2);
    // Without applyToObjects, objects with their own settings keep them.
    run(&mut s, "stitch.setDesign", json!({"fabric": "leather", "applyToObjects": false}));
    let i = run(&mut s, "stitch.info", json!({"ids": [a, b]}));
    assert_eq!(
        (i["objects"][0]["settings"]["fill"]["pullCompensation"].clone(), i["objects"][1]["settings"]["satin"]["pullCompensation"].clone()),
        (json!(0.1), json!(0.2))
    );
}

#[test]
fn a_thread_chart_names_threads_and_matching_snaps_colours_to_it() {
    let mut s = session();
    let (a, _) = (square(&mut s, 0.0, "#fe0101"), square(&mut s, 80.0, "#000000"));
    assert!(s.execute("stitch.matchThreads", &json!({})).is_err(), "no chart yet");
    run(&mut s, "stitch.setDesign", json!({"chart": "janome"}));
    let q = run(&mut s, "stitch.design", json!({}));
    assert_eq!(q["chart"]["name"], "Janome Embroidery");
    let t = q["threads"].as_array().unwrap();
    assert_eq!((t[0]["inChart"].clone(), t[1]["inChart"].clone(), t[1]["code"].clone()), (json!(false), json!(true), json!("002")));
    assert!(t[0]["nearest"]["deltaE"].as_f64().unwrap() > 0.0);
    // The sequencer and machine files name chart threads.
    assert_eq!(run(&mut s, "stitch.sequence", json!({}))["colors"][1]["threadName"], "Black");
    let r = run(&mut s, "stitch.matchThreads", json!({"ids": [a]}));
    assert_eq!(r["changed"], 1);
    let q = run(&mut s, "stitch.design", json!({}));
    assert_eq!(q["threads"][0]["inChart"], true);
    assert_eq!(q["threads"][0]["rgb"], q["threads"][0]["nearest"]["rgb"]);
    assert_eq!(run(&mut s, "stitch.matchThreads", json!({}))["changed"], 0, "already matched");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(run(&mut s, "stitch.design", json!({}))["threads"][0]["rgb"], json!([254, 1, 1]));
}

#[test]
fn charts_import_from_text_and_bad_input_changes_nothing() {
    let mut s = session();
    square(&mut s, 0.0, "#c8102e");
    let gpl = "GIMP Palette\nName: My Polyneon\n200 16 46\tRed 1747\n";
    run(&mut s, "stitch.setDesign", json!({"chart": {"text": gpl}}));
    let q = run(&mut s, "stitch.design", json!({}));
    assert_eq!((q["chart"]["name"].clone(), q["threads"][0]["code"].clone()), (json!("My Polyneon"), json!("1747")));
    for bad in
        [json!({"fabric": "silk"}), json!({"chart": "madeira"}), json!({"chart": {"text": "nothing"}}), json!({}), json!({"chart": {"threads": []}})]
    {
        assert!(s.execute("stitch.setDesign", &bad).is_err(), "{bad}");
    }
    run(&mut s, "stitch.setDesign", json!({"chart": "none"}));
    assert!(run(&mut s, "stitch.design", json!({}))["chart"].is_null());
}

#[test]
fn lettering_is_satin_text_that_stays_editable() {
    let mut s = session();
    let r = run(&mut s, "stitch.createLettering", json!({"text": "Ab€", "height": 12, "x": 20, "y": 100}));
    assert_eq!(r["missing"], "€");
    let id = r["id"].as_u64().unwrap();
    let i = run(&mut s, "stitch.info", json!({"ids": [id]}));
    let o = &i["objects"][0];
    assert_eq!((o["type"].clone(), o["settings"]["lettering"]["text"].clone()), (json!("satin"), json!("Ab€")));
    assert!((o["settings"]["satin"]["width"].as_f64().unwrap() - 1.44).abs() < 1e-9);
    assert!(o["stitches"].as_u64().unwrap() > 200);
    let bounds = |s: &mut Session| s.doc().unwrap().doc.node(NodeId(id)).unwrap().path_data().unwrap().bounds().unwrap();
    let b = bounds(&mut s);
    assert!((b.y0 - (100.0 - 12.0 / MM_PER_PT_TEST)).abs() < 1.0, "cap height above the baseline: {b:?}");
    // Moved, then retyped and resized: it stays where it was moved to.
    run(&mut s, "select.set", json!({"ids": [id]}));
    run(&mut s, "object.move", json!({"dx": 30, "dy": 0}));
    let moved = bounds(&mut s);
    let r = run(&mut s, "stitch.setLettering", json!({"text": "Hello", "height": 20}));
    assert_eq!((r["changed"].clone(), r["missing"].clone()), (json!(1), json!("")));
    let b = bounds(&mut s);
    // The first baseline still starts where "A" did ("H" sits 6 units in; 20 mm letters).
    assert!((b.x0 - (moved.x0 + 6.0 * 0.2 / MM_PER_PT_TEST)).abs() < 0.01, "{b:?} vs {moved:?}");
    assert!((b.y1 - 100.0).abs() < 0.01, "same baseline: {b:?}");
    let i = run(&mut s, "stitch.info", json!({"ids": [id]}));
    assert!((i["objects"][0]["settings"]["satin"]["width"].as_f64().unwrap() - 2.4).abs() < 1e-9);
    assert_eq!(s.doc().unwrap().doc.node(NodeId(id)).unwrap().name.as_deref(), Some("Hello"));
    run(&mut s, "edit.undo", json!({}));
    let i = run(&mut s, "stitch.info", json!({"ids": [id]}));
    assert_eq!(i["objects"][0]["settings"]["lettering"]["text"], "Ab€");
    for bad in [json!({"text": "  "}), json!({"text": "x", "font": "comic"}), json!({"height": "big"})] {
        assert!(s.execute("stitch.setLettering", &bad).is_err(), "{bad}");
    }
    assert!(s.execute("stitch.createLettering", &json!({"text": "€€"})).is_err());
}

const MM_PER_PT_TEST: f64 = 25.4 / 72.0;

fn rails(s: &Session, id: NodeId) -> Vec<Vec<(f64, f64)>> {
    let Some(NodeKind::Path { path, .. }) = s.doc().unwrap().doc.node(id).map(|n| &n.kind) else { panic!("not a path") };
    path.subpaths.iter().map(|sp| sp.anchors.iter().map(|a| (a.p.x, a.p.y)).collect()).collect()
}

#[test]
fn satin_block_rungs_move_insert_delete_and_sides_swap() {
    let mut s = session();
    let id = NodeId(run(&mut s, "path.create", json!({"d": "M 10 10 L 110 10 M 10 30 L 110 30"}))["id"].as_u64().unwrap());
    // Not Satin Blocks yet: rung commands refuse it.
    assert!(s.execute("stitch.insertRung", &json!({"id": id.0, "index": 0})).is_err());
    run(&mut s, "stitch.set", json!({"ids": [id.0], "type": "satinBlocks"}));

    let r = run(&mut s, "stitch.insertRung", json!({"id": id.0, "index": 0, "t": 0.25}));
    assert_eq!(r["index"], 1);
    assert_eq!(rails(&s, id), vec![vec![(10.0, 10.0), (35.0, 10.0), (110.0, 10.0)], vec![(10.0, 30.0), (35.0, 30.0), (110.0, 30.0)]]);

    run(&mut s, "stitch.moveRung", json!({"id": id.0, "index": 1, "left": [40, 0]}));
    assert_eq!(rails(&s, id)[0][1], (40.0, 0.0));
    assert_eq!(rails(&s, id)[1][1], (35.0, 30.0));
    // One undo step each.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(rails(&s, id)[0][1], (35.0, 10.0));

    run(&mut s, "stitch.deleteRung", json!({"id": id.0, "index": 1}));
    assert_eq!(rails(&s, id), vec![vec![(10.0, 10.0), (110.0, 10.0)], vec![(10.0, 30.0), (110.0, 30.0)]]);
    // Two rungs always stay; bad indexes and points change nothing.
    assert!(s.execute("stitch.deleteRung", &json!({"id": id.0, "index": 0})).is_err());
    assert!(s.execute("stitch.moveRung", &json!({"id": id.0, "index": 5, "left": [0, 0]})).is_err());
    assert!(s.execute("stitch.moveRung", &json!({"id": id.0, "index": 0})).is_err());
    assert!(s.execute("stitch.insertRung", &json!({"id": id.0, "index": 1})).is_err());
    assert!(s.execute("stitch.insertRung", &json!({"id": id.0, "index": 0, "t": 2})).is_err());
    assert_eq!(rails(&s, id).len(), 2);

    run(&mut s, "select.set", json!({"ids": [id.0]}));
    assert_eq!(run(&mut s, "stitch.swapSides", json!({}))["changed"], 1);
    assert_eq!(rails(&s, id)[0][0], (10.0, 30.0));
    // Still sews the same column.
    assert!(exported(&mut s).stitch_count() > 50);
}

#[test]
fn a_width_profile_makes_a_satin_column_and_satins_show_as_wide_as_they_sew() {
    let mut s = session();
    let id = NodeId(run(&mut s, "path.create", json!({"d": "M 10 100 L 210 100"}))["id"].as_u64().unwrap());
    run(&mut s, "select.set", json!({"ids": [id.0]}));
    run(&mut s, "stitch.set", json!({"type": "satin", "settings": {"satin": {"width": 6, "pullCompensation": 0, "underlays": []}}}));
    // The stroke is the satin's width, 6 mm in points.
    let weight = |s: &Session| s.doc().unwrap().doc.node(id).unwrap().appearance.stroke().unwrap().width;
    assert!((weight(&s) - 6.0 / (25.4 / 72.0)).abs() < 1e-9);
    let height = |s: &mut Session| exported(s).bounds().unwrap().height();
    assert!((height(&mut s) - 6.0).abs() < 0.2, "{}", height(&mut s));
    // Lens: narrow ends, full width in the middle; the column stays 6 mm at its widest.
    run(&mut s, "stroke.set", json!({"profile": "lens"}));
    let d = exported(&mut s);
    assert!((d.bounds().unwrap().height() - 6.0).abs() < 0.3);
    let near_end: Vec<f64> = d.stitches.iter().filter(|p| p.x < d.bounds().unwrap().min_x + 2.0).map(|p| p.y).collect();
    let spread = near_end.iter().copied().fold(f64::MIN, f64::max) - near_end.iter().copied().fold(f64::MAX, f64::min);
    assert!(spread < 1.5, "{spread}");
    // One undo step took the type and the weight together.
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    assert!((weight(&s) - 1.0).abs() < 1e-9);
}

#[test]
fn e_stitch_and_rope_sew_lines_with_their_own_settings() {
    let mut s = session();
    let id = NodeId(run(&mut s, "path.create", json!({"d": "M 10 100 L 152 100"}))["id"].as_u64().unwrap());
    run(&mut s, "select.set", json!({"ids": [id.0]}));
    // 142 pt is 50.1 mm.
    run(&mut s, "stitch.set", json!({"type": "E-Stitch", "settings": {"eStitch": {"width": 3, "spacing": 5}}}));
    let o = run(&mut s, "stitch.info", json!({}))["objects"][0].clone();
    assert_eq!((o["type"].clone(), o["settings"]["eStitch"]["width"].clone()), (json!("eStitch"), json!(3.0)));
    let b = exported(&mut s).bounds().unwrap();
    assert!((b.max_y - b.min_y - 3.0).abs() < 0.2, "{b:?}");
    assert_eq!(o["stitches"], 3 * 11);
    run(&mut s, "stitch.set", json!({"type": "rope", "settings": {"rope": {"width": 2, "passes": 3}}}));
    let o = run(&mut s, "stitch.info", json!({}))["objects"][0].clone();
    assert_eq!(o["settings"]["rope"]["passes"], 3);
    let b = exported(&mut s).bounds().unwrap();
    assert!((b.max_y - b.min_y - 2.0).abs() < 0.2, "{b:?}");
    // Passes are capped at three.
    run(&mut s, "stitch.set", json!({"settings": {"rope": {"passes": 9}}}));
    let more = run(&mut s, "stitch.info", json!({}))["objects"][0]["stitches"].clone();
    assert_eq!(more, o["stitches"]);
    assert!(s.execute("stitch.set", &json!({"settings": {"rope": {"passes": "many"}}})).is_err());
}

#[test]
fn set_angle_turns_fills_and_motifs_and_leaves_lines() {
    let mut s = session();
    let a = rect(&mut s, 10.0);
    let b = rect(&mut s, 100.0);
    let line = rect(&mut s, 200.0);
    run(&mut s, "stitch.set", json!({"ids": [b.0], "type": "motif"}));
    run(&mut s, "stitch.set", json!({"ids": [line.0], "type": "run"}));
    let r = run(&mut s, "stitch.setAngle", json!({"ids": [a.0, b.0, line.0], "angle": -30}));
    assert_eq!(r["changed"], 2);
    let i = run(&mut s, "stitch.info", json!({"ids": [a.0, b.0]}));
    assert_eq!(i["objects"][0]["settings"]["fill"]["angle"], 330.0);
    // The automatic tatami became an explicit one at that angle.
    assert_eq!(i["objects"][0]["auto"], false);
    assert_eq!(i["objects"][1]["settings"]["motif"]["angle"], 330.0);
    assert_eq!(run(&mut s, "stitch.setAngle", json!({"ids": [a.0], "angle": 330})), json!({"changed": 0}));
    assert!(s.execute("stitch.setAngle", &json!({"angle": "steep"})).is_err());
}

#[test]
fn density_shows_stacked_fills_and_info_gives_a_sew_time() {
    let mut s = session();
    rect(&mut s, 10.0);
    let one = run(&mut s, "stitch.density", json!({"cells": true}));
    // A tatami with its underlay covers the fabric about once.
    let max = one["max"].as_f64().unwrap();
    assert!(max > 0.9 && max < 1.8, "{one}");
    assert_eq!(one["tooDense"], 0);
    assert_eq!(one["layers"].as_array().unwrap().len() as u64, one["cols"].as_u64().unwrap() * one["rows"].as_u64().unwrap());
    // A second fill on top of the first doubles it.
    rect(&mut s, 10.0);
    run(&mut s, "select.none", json!({}));
    let two = run(&mut s, "stitch.density", json!({}));
    assert!(two["max"].as_f64().unwrap() > 1.8 && two["dense"].as_u64().unwrap() > 20, "{two}");
    assert!(two.get("layers").is_none());
    assert!(s.execute("stitch.density", &json!({"cellMm": 0})).is_err());
    let i = run(&mut s, "stitch.info", json!({}));
    let minutes = i["sewMinutes"].as_f64().unwrap();
    assert!(minutes >= i["stitches"].as_f64().unwrap() / 650.0 - 1e-9 && minutes < 30.0, "{minutes}");
    // Two 25 mm tatami squares: some metres of thread, a third as much on the bobbin.
    let top: Vec<f64> = i["threadMeters"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert_eq!(top.len(), 1);
    assert!(top[0] > 2.0 && top[0] < 20.0, "{top:?}");
    assert!((i["bobbinMeters"].as_f64().unwrap() - top[0] / 3.0).abs() < 1e-9);
}

#[test]
fn gradient_fill_fades_from_dense_to_open() {
    let mut s = session();
    let id = rect(&mut s, 10.0);
    let tatami = exported(&mut s).stitch_count();
    run(&mut s, "stitch.set", json!({"ids": [id.0], "type": "gradient", "settings": {"fill": {"angle": 0, "endSpacing": 3}}}));
    let i = run(&mut s, "stitch.info", json!({"ids": [id.0]}));
    assert_eq!(i["objects"][0]["type"], "gradient");
    // Open rows and no underlay: far fewer stitches than the tatami it was.
    let d = exported(&mut s);
    assert!(d.stitch_count() * 2 < tatami, "{} vs {tatami}", d.stitch_count());
    let b = d.bounds().unwrap();
    assert!(b.height() > 23.0 && b.height() <= 25.4, "{b:?}");
}

#[test]
fn motif_fill_sews_rows_of_the_chosen_pattern() {
    let mut s = session();
    let id = rect(&mut s, 10.0);
    run(&mut s, "stitch.set", json!({"ids": [id.0], "type": "motif", "settings": {"motif": {"pattern": "diamonds", "spacing": 2}}}));
    let i = run(&mut s, "stitch.info", json!({"ids": [id.0]}));
    assert_eq!(i["objects"][0]["type"], "motif");
    let single = exported(&mut s).stitch_count();
    assert!(single > 200, "{single}");
    run(&mut s, "stitch.set", json!({"ids": [id.0], "settings": {"motif": {"triple": true}}}));
    // Bean stitch sews each stitch three times.
    assert!(exported(&mut s).stitch_count() > single * 5 / 2);
    let b = exported(&mut s).bounds().unwrap();
    assert!(b.width() <= 25.4 + 1e-6 && b.width() > 24.0, "{b:?}");
}

#[test]
fn circular_fill_rings_a_centre_that_can_move() {
    let mut s = session();
    let id = NodeId(run(&mut s, "shape.rectangle", json!({"x": 10, "y": 10, "width": 100, "height": 100}))["id"].as_u64().unwrap());
    run(&mut s, "stitch.set", json!({"ids": [id.0], "type": "Circular Fill", "settings": {"fill": {"underlay": false}}}));
    let i = run(&mut s, "stitch.info", json!({"ids": [id.0]}));
    assert_eq!(i["objects"][0]["type"], "circular");
    // The first stitch, from the middle of the design.
    let first = |s: &mut Session| {
        let d = exported(s);
        let b = d.bounds().unwrap();
        let p = d.stitches.iter().find(|p| p.command == Command::Stitch).unwrap();
        (p.x - (b.min_x + b.max_x) / 2.0, p.y - (b.min_y + b.max_y) / 2.0)
    };
    // The first rows circle the middle of the square.
    let (x, y) = first(&mut s);
    assert!(x.hypot(y) < 1.6, "{x} {y}");
    run(&mut s, "stitch.set", json!({"ids": [id.0], "settings": {"circular": {"centerX": 10, "centerY": -5}}}));
    let (x, y) = first(&mut s);
    assert!((x - 10.0).hypot(y + 5.0) < 1.6, "{x} {y}");
}

#[test]
fn contour_fill_follows_the_outline_inwards() {
    let mut s = session();
    let id = NodeId(run(&mut s, "shape.ellipse", json!({"x": 10, "y": 10, "width": 200, "height": 140}))["id"].as_u64().unwrap());
    run(&mut s, "stitch.set", json!({"ids": [id.0], "type": "Contour Fill", "settings": {"fill": {"pullCompensation": 0}}}));
    let t = std::time::Instant::now();
    let i = run(&mut s, "stitch.info", json!({"ids": [id.0]}));
    // A 70 × 49 mm ellipse at 0.4 mm rows, in reasonable time even in a debug build.
    assert!(t.elapsed().as_secs_f64() < 20.0, "{:?}", t.elapsed());
    assert_eq!(i["objects"][0]["type"], "contour");
    assert!(i["objects"][0]["stitches"].as_u64().unwrap() > 1000);
    let b = exported(&mut s).bounds().unwrap();
    assert!((b.max_x - b.min_x - 200.0 * 25.4 / 72.0).abs() < 0.6, "{b:?}");
}

#[test]
fn text_sews_as_a_fill_in_its_font_and_stays_editable() {
    let mut s = session();
    let id = run(&mut s, "text.create", json!({"x": 20, "y": 120, "text": "Hi", "size": 96}))["id"].as_u64().unwrap();
    let info = run(&mut s, "stitch.info", json!({}));
    let o = &info["objects"][0];
    assert_eq!((o["id"].as_u64(), o["type"].as_str()), (Some(id), Some("tatami")), "{info}");
    let one = o["stitches"].as_u64().unwrap();
    assert!(one > 200, "{info}");
    // The stitches stay inside the letters: about as wide as the text is.
    let pv = crate::stitch_preview(&s.doc().unwrap().doc);
    let xs = pv.stitches.iter().map(|p| p.x).fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
    assert!(xs.0 > 17.0 && xs.1 < 20.0 + 96.0 * 1.5 && xs.1 - xs.0 > 60.0, "{xs:?}");
    // Editing the text sews the new text.
    run(&mut s, "text.setText", json!({"id": id, "text": "Hi Hi"}));
    let more = run(&mut s, "stitch.info", json!({}))["objects"][0]["stitches"].as_u64().unwrap();
    assert!(more > one * 3 / 2, "{one} → {more}");
    assert_eq!(run(&mut s, "stitch.sequence", json!({}))["objects"][0]["name"], "Hi Hi");
    // And it takes other fills.
    run(&mut s, "stitch.set", json!({"ids": [id], "type": "contour"}));
    assert_eq!(run(&mut s, "stitch.info", json!({}))["objects"][0]["type"], "contour");
}

#[test]
fn travel_hides_under_a_fill_sewn_later_instead_of_trimming() {
    let mut s = session();
    // Two red lines 40 mm apart, then a big blue square over both.
    run(&mut s, "paint.setStroke", json!({"color": "#ff0000"}));
    run(&mut s, "shape.line", json!({"x1": 20, "y1": 100, "x2": 50, "y2": 100}));
    run(&mut s, "shape.line", json!({"x1": 160, "y1": 100, "x2": 190, "y2": 100}));
    run(&mut s, "select.none", json!({}));
    run(&mut s, "paint.setStroke", json!({"none": true}));
    run(&mut s, "paint.setFill", json!({"color": "#0000ff"}));
    run(&mut s, "shape.rectangle", json!({"x": 10, "y": 80, "width": 200, "height": 40}));
    run(&mut s, "select.none", json!({}));
    let types: Vec<String> =
        run(&mut s, "stitch.info", json!({}))["objects"].as_array().unwrap().iter().map(|o| o["type"].as_str().unwrap().to_string()).collect();
    assert_eq!(types, ["run", "run", "tatami"]);
    let trims = |s: &mut Session, under: bool| {
        let out = run(s, "stitch.export", json!({"travelUnder": under}));
        out["trims"].as_u64().unwrap()
    };
    assert_eq!(trims(&mut s, false), 1);
    assert_eq!(trims(&mut s, true), 0);
}

#[test]
fn each_object_starts_near_where_the_last_one_ended() {
    let mut s = session();
    // Four lines stacked 30 pt apart, each drawn left to right: sewn as drawn each would jump
    // back across (14.9 mm); routed, each starts at the end nearer the needle, straight below.
    run(&mut s, "paint.setStroke", json!({"color": "#ff0000"}));
    for y in [100.0, 130.0, 160.0, 190.0] {
        run(&mut s, "shape.line", json!({"x1": 20, "y1": y, "x2": 50, "y2": y}));
    }
    run(&mut s, "select.none", json!({}));
    let d = exported(&mut s);
    // Every jump is the 30 pt (10.6 mm) down to the next line.
    let mut at: Option<(f64, f64)> = None;
    for st in &d.stitches {
        if st.command == Command::Jump
            && let Some((x, y)) = at
        {
            assert!(((st.x - x).powi(2) + (st.y - y).powi(2)).sqrt() < 11.0, "jump of {:?}", (st.x - x, st.y - y));
        }
        at = Some((st.x, st.y));
    }
}

#[test]
fn lettering_maps_to_a_path_and_straightens_again() {
    let mut s = session();
    let id = run(&mut s, "stitch.createLettering", json!({"text": "ARC", "height": 10, "x": 40, "y": 150}))["id"].as_u64().unwrap();
    let bounds = |s: &Session| {
        let n = s.doc().unwrap().doc.node(NodeId(id)).unwrap().clone();
        let NodeKind::Path { path, .. } = &n.kind else { panic!() };
        kurbo::Shape::bounding_box(&path.to_bezpath())
    };
    let flat = bounds(&s);
    // A rising diagonal from the lettering's start.
    let line = run(&mut s, "shape.line", json!({"x1": 40, "y1": 150, "x2": 200, "y2": -10}))["id"].as_u64().unwrap();
    run(&mut s, "select.set", json!({"ids": [id, line]}));
    assert_eq!(run(&mut s, "stitch.mapLettering", json!({}))["changed"], 1);
    assert!(s.doc().unwrap().doc.node(NodeId(line)).is_none());
    let mapped = bounds(&s);
    // Turned 45° up: taller than flat, and climbing from where it started.
    assert!(mapped.height() > flat.height() * 1.8, "{mapped:?} vs {flat:?}");
    assert!((mapped.x0 - flat.x0).abs() < 15.0);
    // Still lettering: editing the text keeps it on the path.
    run(&mut s, "stitch.setLettering", json!({"ids": [id], "text": "ARCS"}));
    assert!(bounds(&s).height() > mapped.height());
    // Straight again.
    run(&mut s, "stitch.setLettering", json!({"ids": [id], "path": null}));
    assert!((bounds(&s).height() - flat.height()).abs() < 0.5);
    // Nothing to follow: an error.
    run(&mut s, "select.set", json!({"ids": [id]}));
    assert!(s.execute("stitch.mapLettering", &json!({})).is_err());
}

#[test]
fn a_hoop_sizes_the_artboard_and_says_whether_the_design_fits() {
    let mut s = session();
    // A 72 pt (25.4 mm) square, then a strip about 120 mm wide.
    rect(&mut s, 10.0);
    run(&mut s, "shape.rectangle", json!({"x": 10, "y": 100, "width": 340, "height": 20}));
    let q = run(&mut s, "stitch.design", json!({}));
    assert!(q["hoop"].is_null());
    assert!(q["hoops"].as_array().unwrap().iter().any(|h| h["id"] == "janome-re20"));
    assert!(run(&mut s, "stitch.info", json!({})).get("hoop").is_none());
    let before = s.doc().unwrap().doc.artboards[0].rect.center();
    let r = run(&mut s, "stitch.setDesign", json!({"hoop": "brother-4x4"}));
    assert_eq!(r["hoop"]["name"], "4 × 4 in");
    let a = s.doc().unwrap().doc.artboards[0].clone();
    assert!((a.rect.width() * MM_PER_PT_TEST - 100.0).abs() < 1e-9 && (a.rect.center() - before).hypot() < 1e-9);
    assert!(a.show_center_mark);
    let i = run(&mut s, "stitch.info", json!({}));
    assert_eq!(i["hoop"]["fit"], "tooLarge");
    assert!(i["hoop"]["overWidthMm"].as_f64().unwrap() > 15.0);
    // 5 × 7 in (130 wide, 180 tall) is big enough for the design (~120 × 40 mm), but the page is
    // the hoop and the strip runs past its right edge until it is centred.
    run(&mut s, "stitch.setDesign", json!({"hoop": "brother-5x7"}));
    let i = run(&mut s, "stitch.info", json!({}));
    assert_eq!(i["hoop"]["fit"], "outside");
    assert!(i["hoop"]["outsideMm"].as_f64().unwrap() > 4.0, "{i}");
    run(&mut s, "select.none", json!({}));
    run(&mut s, "stitch.centerInHoop", json!({}));
    assert_eq!(run(&mut s, "stitch.info", json!({}))["hoop"]["fit"], "fits");
    run(&mut s, "stitch.setDesign", json!({"hoop": {"width": 50, "height": 150}}));
    let out = run(&mut s, "stitch.export", json!({}));
    assert_eq!(out["hoop"]["fit"], "rotate");
    assert_eq!(out["hoop"]["brand"], "Custom");
    assert!((s.doc().unwrap().doc.artboards[0].rect.width() * MM_PER_PT_TEST - 50.0).abs() < 1e-9);
    // Undo steps back one hoop at a time; "none" clears it.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(run(&mut s, "stitch.design", json!({}))["hoop"]["id"], "brother-5x7");
    run(&mut s, "stitch.setDesign", json!({"hoop": "none"}));
    assert!(run(&mut s, "stitch.design", json!({}))["hoop"].is_null());
    assert!(s.execute("stitch.setDesign", &json!({"hoop": "nope"})).is_err());
    assert!(s.execute("stitch.setDesign", &json!({"hoop": {"width": 5, "height": 100}})).is_err());
}

fn stroke_weight(s: &Session, id: NodeId) -> f64 {
    s.doc().unwrap().doc.node(id).unwrap().appearance.stroke().unwrap().width
}

#[test]
fn a_fill_and_its_stroke_sew_as_two_parts_fill_first() {
    let mut s = session();
    run(&mut s, "paint.setStroke", json!({"color": "#000000"}));
    let id = rect(&mut s, 10.0);
    let i = run(&mut s, "stitch.info", json!({}));
    let o = &i["objects"][0];
    assert_eq!((o["fill"]["type"].clone(), o["fill"]["sews"].clone()), (json!("tatami"), json!(true)));
    assert_eq!((o["stroke"]["type"].clone(), o["stroke"]["sews"].clone()), (json!("run"), json!(true)));
    assert_eq!((o["fill"]["thread"].clone(), o["stroke"]["thread"].clone()), (json!([255, 255, 255]), json!([0, 0, 0])));
    assert_eq!(i["colorChanges"], 1);
    let q = run(&mut s, "stitch.sequence", json!({}));
    let parts: Vec<_> = q["objects"].as_array().unwrap().iter().map(|r| (r["id"].as_u64().unwrap(), r["part"].clone())).collect();
    assert_eq!(parts, [(id.0, json!("fill")), (id.0, json!("stroke"))]);
    // Each part takes its own types; the other keeps sewing.
    run(&mut s, "stitch.set", json!({"part": "stroke", "type": "satin"}));
    run(&mut s, "stitch.set", json!({"part": "fill", "type": "contour"}));
    let o = &run(&mut s, "stitch.info", json!({}))["objects"][0];
    assert_eq!((o["fill"]["type"].clone(), o["stroke"]["type"].clone()), (json!("contour"), json!("satin")));
    assert!(o["fill"]["stitches"].as_u64().unwrap() > 0 && o["stroke"]["stitches"].as_u64().unwrap() > 0);
    // A fill can't sew a line stitch, nor a stroke a fill.
    assert!(s.execute("stitch.set", &json!({"part": "fill", "type": "satin"})).is_err());
    assert!(s.execute("stitch.set", &json!({"part": "stroke", "type": "tatami"})).is_err());
    assert!(s.execute("stitch.set", &json!({"part": "outline", "type": "run"})).is_err());
    // Turning one part off leaves the other; reset brings both back to the rule.
    run(&mut s, "stitch.set", json!({"part": "fill", "type": "none"}));
    let o = &run(&mut s, "stitch.info", json!({}))["objects"][0];
    assert_eq!((o["fill"]["sews"].clone(), o["stroke"]["sews"].clone()), (json!(false), json!(true)));
    run(&mut s, "stitch.set", json!({"reset": true}));
    assert!(s.doc().unwrap().doc.node(id).unwrap().extras().attrs.as_ref().and_then(|a| a.stitch.as_ref()).is_none());
}

#[test]
fn a_stroke_stitch_width_is_the_stroke_weight_both_ways() {
    let mut s = session();
    let id = rect(&mut s, 10.0);
    // No stroke yet: setting the stroke part gives it one in the fill colour, as wide as it sews.
    run(&mut s, "paint.setFill", json!({"color": "#ff0000"}));
    run(&mut s, "stitch.set", json!({"part": "stroke", "type": "satin", "settings": {"satin": {"width": 3}}}));
    let n = s.doc().unwrap().doc.node(id).unwrap().clone();
    assert!(n.appearance.stroke().is_some());
    assert!((stroke_weight(&s, id) - 3.0 / MM_PER_PT).abs() < 1e-9);
    let o = &run(&mut s, "stitch.info", json!({}))["objects"][0];
    assert_eq!(o["stroke"]["thread"], json!([255, 0, 0]));
    // The Stroke panel's weight is the satin's width.
    run(&mut s, "stroke.set", json!({"weight": 5.0 / MM_PER_PT}));
    let w = run(&mut s, "stitch.info", json!({}))["objects"][0]["stroke"]["settings"]["satin"]["width"].as_f64().unwrap();
    assert!((w - 5.0).abs() < 1e-6, "{w}");
    // Rope and E-stitch too.
    run(&mut s, "stitch.set", json!({"part": "stroke", "type": "rope", "settings": {"rope": {"width": 1.5}}}));
    assert!((stroke_weight(&s, id) - 1.5 / MM_PER_PT).abs() < 1e-9);
    // A running stitch shows at thread thickness.
    run(&mut s, "stitch.set", json!({"part": "stroke", "type": "triple"}));
    assert!((stroke_weight(&s, id) - 0.4 / MM_PER_PT).abs() < 1e-9);
}

#[test]
fn single_part_settings_read_and_write_as_before() {
    let mut s = session();
    let id = rect(&mut s, 10.0);
    run(&mut s, "stitch.set", json!({"type": "contour"}));
    let stored = s.doc().unwrap().doc.node(id).unwrap().extras().attrs.as_ref().unwrap().stitch.clone().unwrap();
    assert_eq!(stored["type"], "contour");
    // Two parts are stored by part.
    run(&mut s, "paint.setStroke", json!({"color": "#000000"}));
    run(&mut s, "stitch.set", json!({"part": "stroke", "type": "run"}));
    let stored = s.doc().unwrap().doc.node(id).unwrap().extras().attrs.as_ref().unwrap().stitch.clone().unwrap();
    assert_eq!((stored["fill"]["type"].clone(), stored["stroke"]["type"].clone()), (json!("contour"), json!("run")));
    // A whole-object type (Satin Blocks) has no fill part.
    run(&mut s, "stitch.set", json!({"type": "satinBlocks"}));
    let o = &run(&mut s, "stitch.info", json!({}))["objects"][0];
    assert_eq!((o["whole"].clone(), o["fill"]["sews"].clone()), (json!(true), json!(false)));
}

#[test]
fn curved_satin_block_rails_sew_along_the_curve() {
    let mut s = session();
    // Two rungs whose rails bulge out to the right: the chords between them are straight down x = 100.
    let r = run(
        &mut s,
        "stitch.createSatinBlocks",
        json!({
            "left": [{"p": [100, 50], "out": [155, 50]}, {"p": [100, 150], "in": [155, 150]}],
            "right": [{"p": [100, 70], "out": [130, 70]}, {"p": [100, 130], "in": [130, 130]}],
        }),
    );
    let id = NodeId(r["id"].as_u64().unwrap());
    let pv = crate::stitch_preview(&s.doc().unwrap().doc);
    let max_x = pv.stitches.iter().filter(|p| p.sew).map(|p| p.x).fold(f64::MIN, f64::max);
    // The outer rail's curve peaks at x = 100 + 0.75 × 55.
    assert!((max_x - 141.25).abs() < 1.5, "{max_x}");

    // A rung inserted half way along stays on the curve, which keeps its shape.
    run(&mut s, "stitch.insertRung", json!({"id": id.0, "index": 0, "t": 0.5}));
    let r = rails(&s, id);
    assert!((r[0][1].0 - 141.25).abs() < 0.01 && (r[0][1].1 - 100.0).abs() < 0.01, "{:?}", r[0][1]);
    let pv2 = crate::stitch_preview(&s.doc().unwrap().doc);
    let max_x2 = pv2.stitches.iter().filter(|p| p.sew).map(|p| p.x).fold(f64::MIN, f64::max);
    assert!((max_x2 - max_x).abs() < 0.5, "{max_x2} {max_x}");

    // Moving a rung end carries its handles (halved by the split above: 100 + 55 / 2).
    run(&mut s, "stitch.moveRung", json!({"id": id.0, "index": 0, "left": [90, 40]}));
    let Some(NodeKind::Path { path, .. }) = s.doc().unwrap().doc.node(id).map(|n| &n.kind) else { panic!() };
    let a = &path.subpaths[0].anchors[0];
    assert_eq!((a.p.x, a.p.y), (90.0, 40.0));
    assert!((a.h_out.x - 117.5).abs() < 1e-3 && (a.h_out.y - 40.0).abs() < 1e-9, "{:?}", a.h_out);
}

#[test]
fn convert_to_satin_makes_satin_blocks_and_hides_the_shape() {
    let mut s = session();
    run(&mut s, "paint.setFill", json!({"color": "#2a7de1"}));
    // A bar 100 × 12 pt (35 × 4.2 mm).
    let bar = NodeId(run(&mut s, "shape.rectangle", json!({"x": 20, "y": 20, "width": 100, "height": 12}))["id"].as_u64().unwrap());
    run(&mut s, "select.set", json!({"ids": [bar.0]}));
    let r = run(&mut s, "stitch.convertToSatin", json!({}));
    let ids: Vec<u64> = r["ids"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(ids.len(), 1, "{r}");
    assert!(r.get("warning").is_none());
    assert!((r["widestMm"].as_f64().unwrap() - 12.0 * MM_PER_PT).abs() < 0.1, "{r}");
    let d = &s.doc().unwrap().doc;
    assert!(!d.node(bar).unwrap().visible);
    // The satin sews in the bar's colour, after it, and is what is selected.
    // (The hidden bar is listed too, where it was, sewing nothing.)
    let seq = run(&mut s, "stitch.sequence", json!({}));
    let objects = seq["objects"].as_array().unwrap();
    assert_eq!(
        objects.iter().map(|o| (o["id"].as_u64().unwrap(), o["visible"].as_bool().unwrap())).collect::<Vec<_>>(),
        [(bar.0, false), (ids[0], true)]
    );
    assert_eq!(objects[1]["type"], "satinBlocks");
    assert_eq!(objects[1]["thread"], json!([42, 125, 225]));
    assert_eq!(seq["colors"].as_array().unwrap().len(), 1);
    assert_eq!(s.doc().unwrap().selection.objects, vec![NodeId(ids[0])]);
    // Its rails run the bar's length, its rungs across it.
    let r0 = &rails(&s, NodeId(ids[0]));
    assert!(r0[0].iter().all(|p| (p.1 - 20.0).abs() < 0.2 || (p.1 - 32.0).abs() < 0.2), "{r0:?}");
    // One undo step puts the bar back.
    run(&mut s, "edit.undo", json!({}));
    let d = &s.doc().unwrap().doc;
    assert!(d.node(bar).unwrap().visible && d.node(NodeId(ids[0])).is_none());

    // A shape far too wide still converts, with a warning; an open path is skipped.
    let sq = NodeId(run(&mut s, "shape.rectangle", json!({"x": 20, "y": 60, "width": 100, "height": 100}))["id"].as_u64().unwrap());
    let line = NodeId(run(&mut s, "path.create", json!({"d": "M 0 200 L 100 200"}))["id"].as_u64().unwrap());
    let r = run(&mut s, "stitch.convertToSatin", json!({"ids": [sq.0, line.0]}));
    assert!(r["warning"].as_str().unwrap().contains("mm wide"), "{r}");
    assert_eq!(r["skipped"], json!([line.0]));
    assert!(s.execute("stitch.convertToSatin", &json!({"ids": [line.0]})).is_err());

    // The classic way, kept to compare, converts the same shapes the same way round.
    let bar2 = NodeId(run(&mut s, "shape.rectangle", json!({"x": 20, "y": 250, "width": 60, "height": 8}))["id"].as_u64().unwrap());
    let r = run(&mut s, "stitch.convertToSatinClassic", json!({"ids": [bar2.0, line.0]}));
    assert_eq!(r["ids"].as_array().unwrap().len(), 1, "{r}");
    assert_eq!(r["skipped"], json!([line.0]));
    assert!(!s.doc().unwrap().doc.node(bar2).unwrap().visible);
    assert!(s.execute("stitch.convertToSatinClassic", &json!({"ids": [line.0]})).is_err());
}

#[test]
fn the_sequence_lists_hidden_objects_and_parts_can_swap_order() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
    run(&mut s, "paint.setFill", json!({"color": "#2a7de1"}));
    run(&mut s, "paint.setStroke", json!({"color": "#f2c230"}));
    let a = rect(&mut s, 10.0);
    let b = rect(&mut s, 100.0);
    run(&mut s, "layer.setProps", json!({"ids": [b.0], "visible": false}));
    let seq = run(&mut s, "stitch.sequence", json!({"thumbs": true}));
    let rows = seq["objects"].as_array().unwrap();
    // a: fill then stroke; b: hidden, one row that sews nothing.
    let parts: Vec<(u64, Value, bool)> =
        rows.iter().map(|r| (r["id"].as_u64().unwrap(), r["part"].clone(), r["visible"].as_bool().unwrap())).collect();
    assert_eq!(parts, vec![(a.0, json!("fill"), true), (a.0, json!("stroke"), true), (b.0, Value::Null, false)]);
    assert!(rows[0]["thumb"].as_array().unwrap().len() > 20);
    assert!(rows[2].get("thumb").is_none() && rows[2]["stitches"] == 0);
    let layer = s.doc().unwrap().doc.layers[0].id.0;
    assert_eq!(rows[0]["parents"], json!([layer]));
    assert_eq!(seq["groups"][0]["kind"], "layer");
    // A hidden object can still be reordered.
    run(&mut s, "stitch.move", json!({"ids": [b.0], "before": a.0}));
    // The stroke sews first, in one undo step; the fill again after undo.
    run(&mut s, "select.set", json!({"ids": [a.0]}));
    assert_eq!(run(&mut s, "stitch.partOrder", json!({"first": "stroke"}))["changed"], 1);
    let order = |s: &mut Session| {
        let seq = run(s, "stitch.sequence", json!({}));
        seq["objects"].as_array().unwrap().iter().filter(|r| r["id"] == a.0).map(|r| r["part"].clone()).collect::<Vec<_>>()
    };
    assert_eq!(order(&mut s), vec![json!("stroke"), json!("fill")]);
    // Settings changes keep the order.
    run(&mut s, "stitch.set", json!({"part": "stroke", "type": "satin"}));
    assert_eq!(order(&mut s), vec![json!("stroke"), json!("fill")]);
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(order(&mut s), vec![json!("fill"), json!("stroke")]);
    assert!(s.execute("stitch.partOrder", &json!({"first": "middle"})).is_err());
}

#[test]
fn the_preview_marks_underlay_and_trims() {
    let mut s = session();
    rect(&mut s, 10.0);
    rect(&mut s, 200.0);
    let pv = crate::stitch_preview(&s.doc().unwrap().doc);
    // Tatami sews underlay by default.
    assert!(pv.stitches.iter().any(|p| p.underlay && p.sew));
    assert!(pv.stitches.iter().any(|p| !p.underlay && p.sew));
    // The far shape is trimmed to.
    assert!(pv.stitches.iter().any(|p| p.trim && !p.sew));
}

#[test]
fn center_in_hoop_moves_everything_to_the_middle_of_the_artboard() {
    let mut s = session();
    let a = rect(&mut s, 10.0);
    s.execute("select.none", &json!({})).unwrap();
    let r = run(&mut s, "stitch.centerInHoop", json!({}));
    assert!(r["dx"].as_f64().unwrap() > 0.0);
    let d = &s.doc().unwrap().doc;
    let b = d.bounds_of(&[a], true).unwrap();
    let c = d.artboards[0].rect.center();
    assert!((b.center().x - c.x).abs() < 1e-6 && (b.center().y - c.y).abs() < 1e-6, "{b:?} {c:?}");
    // Once centred, nothing moves; and it undoes in one step.
    let again = run(&mut s, "stitch.centerInHoop", json!({}));
    assert!(again["dx"].as_f64().unwrap().abs() < 1e-6);
    s.execute("edit.undo", &json!({})).unwrap();
    let b = s.doc().unwrap().doc.bounds_of(&[a], true).unwrap();
    assert!((b.center().x - c.x).abs() > 1.0);
}

/// A 60 × 60 image on white: a red square, a blue one and a green bar, selected.
fn two_squares_on_white(s: &mut Session) -> NodeId {
    use vectorcraft_doc::{ImageBlob, ImageObject, Node, NodeKind};
    use vectorcraft_geom::Affine;
    let r = vectorcraft_trace::Raster::from_fn(60, 60, |x, y| match (x, y) {
        (5..25, 5..25) => [220, 20, 30, 255],
        (35..55, 35..55) => [20, 40, 210, 255],
        (35..55, 5..20) => [20, 160, 60, 255],
        _ => [255, 255, 255, 255],
    });
    let png = r.encode_png();
    s.edit("img", |d, sel| {
        d.images.insert("img1".into(), ImageBlob::new("image/png", png));
        let id = d.alloc_id();
        let l = d.default_layer();
        let image = ImageObject { key: "img1".into(), width: 60, height: 60, xf: Affine::IDENTITY, link: None, placement: Default::default() };
        d.insert(l, 0, Node::new(id, NodeKind::Image(Box::new(image))))?;
        sel.set([id]);
        Ok(id)
    })
    .unwrap()
}

#[test]
fn image_trace_sews_the_colours_as_tatami_and_leaves_the_white_out() {
    let mut s = session();
    let image = two_squares_on_white(&mut s);
    let r = run(&mut s, "stitch.traceImage", json!({}));
    assert_eq!(r["preset"], "3 Color", "the default: {r}");
    // Three threads, though VectorCraft's 3 Colors counts the white it leaves out.
    assert_eq!((r["paths"].clone(), r["colors"].clone()), (json!(3), json!(3)), "three shapes, no white: {r}");
    let i = run(&mut s, "stitch.info", json!({}));
    let types: Vec<&str> = i["objects"].as_array().unwrap().iter().map(|o| o["type"].as_str().unwrap()).collect();
    assert_eq!(types, ["tatami", "tatami", "tatami"], "{i}");
    assert!(i["objects"].as_array().unwrap().iter().all(|o| o["stitches"].as_u64().unwrap() > 20), "{i}");
    // One undo step takes it all back to the image.
    run(&mut s, "edit.undo", json!({}));
    let d = &s.doc().unwrap().doc;
    assert!(d.node(NodeId(r["id"].as_u64().unwrap())).is_none());
    assert!(d.node(image).is_some_and(|n| matches!(n.kind, vectorcraft_doc::NodeKind::Image(_))));

    // The other presets, by Roidery's names (any case); VectorCraft's names aren't offered.
    for (name, paths) in [("2 color", 3), ("6 Color", 3), ("Detailed", 3)] {
        let mut s = session();
        two_squares_on_white(&mut s);
        let r = run(&mut s, "stitch.traceImage", json!({"preset": name}));
        assert_eq!(r["paths"], paths, "{name}: {r}");
    }
    let mut s = session();
    two_squares_on_white(&mut s);
    let e = s.execute("stitch.traceImage", &json!({"preset": "16 Colors"})).unwrap_err().to_string();
    assert!(e.contains("2 Color, 3 Color, 6 Color or Detailed"), "{e}");
}

#[test]
fn the_page_is_the_hoop_so_export_keeps_the_place_on_the_page() {
    let mut s = session();
    // The square's centre sits 30 mm left of the page's centre (300 pt page: 150 pt).
    let x = 150.0 - 30.0 / MM_PER_PT - 36.0;
    rect(&mut s, x);
    // Where the needle sews, leaving out the start at the origin.
    let sewn = |d: roidery_formats::Design| {
        let st = || d.stitches.iter().filter(|p| p.command == Command::Stitch);
        let (x, y) = (st().map(|p| p.x), st().map(|p| p.y));
        let (min_x, max_x) = (x.clone().fold(f64::MAX, f64::min), x.fold(f64::MIN, f64::max));
        let (min_y, max_y) = (y.clone().fold(f64::MAX, f64::min), y.fold(f64::MIN, f64::max));
        roidery_formats::Bounds { min_x, min_y, max_x, max_y }
    };
    // No hoop: the design is centred on the needle, as before.
    let b = sewn(exported(&mut s));
    assert!(((b.min_x + b.max_x) / 2.0).abs() < 0.3, "{b:?}");
    run(&mut s, "stitch.setDesign", json!({"hoop": "brother-4x4"}));
    let b = sewn(exported(&mut s));
    assert!(((b.min_x + b.max_x) / 2.0 + 30.0).abs() < 0.3, "{b:?}");
    // y is down: the square's centre is 46 pt above the page centre.
    let dy = (10.0 + 36.0 - 150.0) * MM_PER_PT;
    assert!(((b.min_y + b.max_y) / 2.0 - dy).abs() < 0.3, "{b:?} {dy}");
    let q = run(&mut s, "stitch.design", json!({}));
    assert_eq!(q["hoop"]["id"], "brother-4x4");
    assert!((q["field"]["width"].as_f64().unwrap() * MM_PER_PT - 100.0).abs() < 1e-6, "{q}");
    assert_eq!(run(&mut s, "stitch.info", json!({}))["hoop"]["fit"], "fits");
}

#[test]
fn stitches_past_the_page_edge_are_outside_the_hoop() {
    let mut s = session();
    run(&mut s, "stitch.setDesign", json!({"hoop": "brother-4x4"}));
    // The 100 mm page runs from about -6 to 277 pt; this square overhangs its right edge by 20 mm.
    let right = s.doc().unwrap().doc.artboards[0].rect.x1;
    rect(&mut s, right + 20.0 / MM_PER_PT - 72.0);
    let i = run(&mut s, "stitch.info", json!({}));
    assert_eq!(i["hoop"]["fit"], "outside");
    let out = i["hoop"]["outsideMm"].as_f64().unwrap();
    assert!((out - 20.0).abs() < 0.5, "{out}");
    assert_eq!(run(&mut s, "stitch.export", json!({}))["hoop"]["fit"], "outside");
    // A round hoop (the 12 cm sews a 100 mm circle): an 80 mm square's diagonal is 113 mm, too
    // wide wherever it sits; a 65 mm one (92 mm across) only needs moving into the middle.
    run(&mut s, "stitch.setDesign", json!({"hoop": "commercial-12"}));
    run(&mut s, "select.all", json!({}));
    run(&mut s, "edit.clear", json!({}));
    let r = s.doc().unwrap().doc.artboards[0].rect;
    run(&mut s, "shape.rectangle", json!({"x": r.x0, "y": r.y0, "width": 80.0 / MM_PER_PT, "height": 80.0 / MM_PER_PT}));
    assert_eq!(run(&mut s, "stitch.info", json!({}))["hoop"]["fit"], "tooLarge");
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "shape.rectangle", json!({"x": r.x0, "y": r.y0, "width": 65.0 / MM_PER_PT, "height": 65.0 / MM_PER_PT}));
    let i = run(&mut s, "stitch.info", json!({}));
    assert_eq!(i["hoop"]["fit"], "outside");
    run(&mut s, "select.none", json!({}));
    run(&mut s, "stitch.centerInHoop", json!({}));
    assert_eq!(run(&mut s, "stitch.info", json!({}))["hoop"]["fit"], "fits");
}

#[test]
fn resizing_the_page_by_hand_makes_a_custom_hoop() {
    let mut s = session();
    run(&mut s, "stitch.setDesign", json!({"hoop": "brother-5x7"}));
    // Turned on its side, it is still the same hoop.
    s.edit("turn", |d, _| {
        let r = d.artboards[0].rect;
        let c = r.center();
        let (w, h) = (r.height(), r.width());
        d.artboards[0].rect = vectorcraft_geom::Rect::new(c.x - w / 2.0, c.y - h / 2.0, c.x + w / 2.0, c.y + h / 2.0);
        Ok(())
    })
    .unwrap();
    let q = run(&mut s, "stitch.design", json!({}));
    assert_eq!((q["hoop"]["id"].clone(), q["hoop"]["widthMm"].clone()), (json!("brother-5x7"), json!(180.0)), "{q}");
    s.edit("resize", |d, _| {
        let r = d.artboards[0].rect;
        d.artboards[0].rect = vectorcraft_geom::Rect::new(r.x0, r.y0, r.x0 + 150.0 / MM_PER_PT, r.y0 + 90.0 / MM_PER_PT);
        Ok(())
    })
    .unwrap();
    let q = run(&mut s, "stitch.design", json!({}));
    assert_eq!(q["hoop"]["brand"], "Custom", "{q}");
    assert_eq!(q["hoop"]["name"], "150 × 90 mm");
}

#[test]
fn new_document_offers_every_hoop_and_sets_it() {
    let mut s = Session::new();
    let presets = run(&mut s, "file.newPresets", json!({"category": "Embroidery"}));
    let names: Vec<String> =
        presets["categories"][0]["presets"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap().to_string()).collect();
    assert!(names.iter().any(|n| n == "Brother 5 × 7 in"), "{names:?}");
    run(&mut s, "file.new", json!({"preset": "Brother 5 × 7 in"}));
    let q = run(&mut s, "stitch.design", json!({}));
    assert_eq!(q["hoop"]["id"], "brother-5x7");
    assert!(s.doc().unwrap().doc.artboards[0].show_center_mark);
    run(&mut s, "file.new", json!({"preset": "Hoop 100 × 100 mm"}));
    let q = run(&mut s, "stitch.design", json!({}));
    assert_eq!((q["hoop"]["brand"].clone(), q["hoop"]["name"].clone()), (json!("Custom"), json!("100 × 100 mm")));
    // A plain document has no hoop.
    run(&mut s, "file.new", json!({"width": 300, "height": 300}));
    assert!(run(&mut s, "stitch.design", json!({}))["hoop"].is_null());
    assert!(s.execute("file.new", &json!({"width": 300, "height": 300, "hoop": "nope"})).is_err());
}

fn problems(s: &mut Session) -> Vec<Value> {
    run(s, "stitch.check", json!({}))["problems"].as_array().unwrap().clone()
}

fn kinds(s: &mut Session) -> Vec<String> {
    problems(s).iter().map(|p| p["kind"].as_str().unwrap().to_string()).collect()
}

/// Run a problem's fix as the Check Design panel does.
fn fix(s: &mut Session, p: &Value) {
    let f = &p["fix"];
    s.execute(f["command"].as_str().unwrap(), &f["params"]).unwrap();
}

#[test]
fn check_design_finds_thin_tiny_and_dense_objects_and_fixes_them() {
    let mut s = session();
    rect(&mut s, 10.0);
    run(&mut s, "select.none", json!({}));
    assert!(problems(&mut s).is_empty(), "{:?}", kinds(&mut s));
    // A sliver 0.7 mm wide, and a dot 1 mm across.
    let thin = NodeId(run(&mut s, "shape.rectangle", json!({"x": 10, "y": 100, "width": 150, "height": 2}))["id"].as_u64().unwrap());
    let dot = NodeId(run(&mut s, "shape.rectangle", json!({"x": 10, "y": 120, "width": 3, "height": 3}))["id"].as_u64().unwrap());
    run(&mut s, "select.none", json!({}));
    let ps = problems(&mut s);
    let thin_p = ps.iter().find(|p| p["kind"] == "thinFill").unwrap();
    assert_eq!((thin_p["ids"].clone(), thin_p["level"].clone()), (json!([thin.0]), json!("warning")));
    assert!(thin_p["value"].as_f64().unwrap() < 1.0 && thin_p["x"].as_f64().unwrap() > 50.0, "{thin_p}");
    assert_eq!(thin_p["fix"]["command"], "stitch.convertToSatin");
    assert_eq!(ps.iter().find(|p| p["kind"] == "tiny").unwrap()["ids"], json!([dot.0]));
    // Converting the sliver to satin leaves it sewing fine.
    let p = thin_p.clone();
    fix(&mut s, &p);
    run(&mut s, "select.none", json!({}));
    assert!(!kinds(&mut s).contains(&"thinFill".to_string()), "{:?}", kinds(&mut s));
    // Three fills stacked: thread piles up where they overlap, and Remove Overlaps clears it.
    let mut s = session();
    for x in [10.0, 30.0, 50.0] {
        rect(&mut s, x);
    }
    run(&mut s, "select.none", json!({}));
    let ps = problems(&mut s);
    let dense = ps.iter().find(|p| p["kind"] == "tooDense").unwrap().clone();
    assert_eq!(dense["level"], "error");
    assert!(dense["value"].as_f64().unwrap() > 4.0 && dense["ids"].as_array().unwrap().len() == 3, "{dense}");
    assert_eq!(dense["fix"]["command"], "stitch.removeOverlaps");
    fix(&mut s, &dense);
    run(&mut s, "select.none", json!({}));
    assert!(!kinds(&mut s).contains(&"tooDense".to_string()), "{:?}", kinds(&mut s));
    assert_eq!(run(&mut s, "stitch.check", json!({}))["errors"], 0);
}

#[test]
fn check_design_measures_satin_width() {
    let mut s = session();
    let a = rect(&mut s, 10.0);
    // Split satin keeps a wide satin's stitches short; turned off, they lie 12 mm long.
    run(&mut s, "stitch.set", json!({"ids": [a.0], "part": "stroke", "type": "satin", "settings": {"satin": {"width": 12}}}));
    run(&mut s, "select.none", json!({}));
    assert!(kinds(&mut s).is_empty(), "{:?}", kinds(&mut s));
    run(&mut s, "stitch.set", json!({"ids": [a.0], "part": "stroke", "settings": {"satin": {"splitLength": 0}}}));
    run(&mut s, "select.none", json!({}));
    let ps = problems(&mut s);
    let wide = ps.iter().find(|p| p["kind"] == "wideSatin").unwrap().clone();
    assert!(wide["value"].as_f64().unwrap() > 10.0, "{wide}");
    fix(&mut s, &wide);
    run(&mut s, "select.none", json!({}));
    assert!(kinds(&mut s).is_empty(), "{:?}", kinds(&mut s));
    run(&mut s, "stitch.set", json!({"ids": [a.0], "part": "stroke", "type": "satin", "settings": {"satin": {"width": 0.4}}}));
    run(&mut s, "select.none", json!({}));
    let ps = problems(&mut s);
    let narrow = ps.iter().find(|p| p["kind"] == "narrowSatin").unwrap().clone();
    assert_eq!(narrow["fix"]["params"]["type"], "run");
    fix(&mut s, &narrow);
    assert!(kinds(&mut s).is_empty(), "{:?}", kinds(&mut s));
}

#[test]
fn check_design_flags_the_hoop_and_center_in_hoop_fixes_it() {
    let mut s = session();
    run(&mut s, "stitch.setDesign", json!({"hoop": "brother-4x4"}));
    let right = s.doc().unwrap().doc.artboards[0].rect.x1;
    rect(&mut s, right - 40.0);
    run(&mut s, "select.none", json!({}));
    let ps = problems(&mut s);
    let out = ps.first().unwrap().clone();
    assert_eq!((out["kind"].clone(), out["level"].clone()), (json!("outsideHoop"), json!("error")));
    assert!(out["x"].as_f64().unwrap() > right, "{out}");
    fix(&mut s, &out);
    run(&mut s, "select.none", json!({}));
    assert!(problems(&mut s).is_empty(), "{:?}", kinds(&mut s));
    // Wider than the hoop: nothing to click, pick a bigger hoop.
    run(&mut s, "shape.rectangle", json!({"x": 0, "y": 200, "width": 340, "height": 20}));
    run(&mut s, "select.none", json!({}));
    let ps = problems(&mut s);
    assert_eq!(ps.first().unwrap()["kind"], "tooLarge");
    assert!(ps.first().unwrap().get("fix").is_none());
}

#[test]
fn check_design_leaves_satin_borders_and_lettering_alone() {
    // One object's own dense spots (a satin's corners, lettering) are how it sews.
    let mut s = session();
    let a = rect(&mut s, 10.0);
    run(&mut s, "stitch.set", json!({"ids": [a.0], "part": "stroke", "type": "satin", "settings": {"satin": {"width": 3}}}));
    run(&mut s, "stitch.createLettering", json!({"text": "Hello", "height": 10, "x": 20, "y": 150}));
    run(&mut s, "select.none", json!({}));
    assert!(problems(&mut s).is_empty(), "{:?}", problems(&mut s));
    // Another fill laid over it is flagged where they stack.
    let b = rect(&mut s, 30.0);
    run(&mut s, "select.none", json!({}));
    let ps = problems(&mut s);
    assert!(!ps.is_empty() && ps.iter().all(|p| p["kind"] == "tooDense" && p["ids"] == json!([a.0, b.0])), "{ps:?}");
    // Two plain fills overlapping are a warning; three stacked, an error (the test above).
    let mut s = session();
    rect(&mut s, 10.0);
    rect(&mut s, 30.0);
    run(&mut s, "select.none", json!({}));
    let ps = problems(&mut s);
    assert!(!ps.is_empty() && ps.iter().all(|p| p["level"] == "warning"), "{ps:?}");
}
