//! `document.exportPdf {createLayers}` writes PDF layers that reopen as layers, and
//! `advanced.overprint` keeps overprinting in the file or drops it.

use serde_json::{Value, json};

use super::*;

fn b64(v: &Value) -> Vec<u8> {
    vectorcraft_format::base64_decode(v["dataBase64"].as_str().expect("dataBase64")).unwrap()
}

fn warnings(v: &Value) -> Vec<String> {
    v["warnings"].as_array().expect("warnings").iter().map(|w| w.as_str().unwrap().to_string()).collect()
}

/// A document with a rectangle on "Layer 1" and one on a hidden, non-printing layer "Notes".
fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 100, "height": 80})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
    let notes = s.execute("layer.new", &json!({"name": "Notes"})).unwrap()["id"].as_u64().unwrap();
    s.execute("shape.rectangle", &json!({"x": 50, "y": 10, "width": 20, "height": 20})).unwrap();
    s.execute("layer.setProps", &json!({"id": notes, "visible": false, "printable": false})).unwrap();
    s
}

#[test]
fn export_pdf_create_layers_writes_layers_that_reopen_as_layers() {
    let mut s = session();
    let v = s.execute("document.exportPdf", &json!({"createLayers": true, "preserveEditing": false})).unwrap();
    assert!(warnings(&v).is_empty(), "{:?}", warnings(&v));
    let back = vectorcraft_pdf::import(&b64(&v)).unwrap();
    let layers: Vec<(String, bool, bool)> = back
        .layers
        .iter()
        .map(|l| (l.name.clone().unwrap_or_default(), l.visible, matches!(l.kind, vectorcraft_doc::NodeKind::Layer { printable: true, .. })))
        .collect();
    assert_eq!(layers, [("Layer 1".to_string(), true, true), ("Notes".to_string(), false, false)]);
    // The default preset carries the editing data too.
    let v = s.execute("document.exportPdf", &json!({"createLayers": true})).unwrap();
    assert!(warnings(&v).is_empty(), "{:?}", warnings(&v));
    assert!(vectorcraft_pdf::editing(&b64(&v)).is_some_and(|e| e.intact));
    // At PDF 1.4 there are no PDF layers.
    let v = s.execute("document.exportPdf", &json!({"createLayers": true, "compatibility": "1.4"})).unwrap();
    assert!(warnings(&v).iter().any(|w| w.contains("PDF 1.5")), "{:?}", warnings(&v));
}

#[test]
fn export_pdf_overprint_is_preserved_or_discarded() {
    let mut s = session();
    s.execute("select.all", &json!({})).unwrap();
    // The rectangles' strokes are black (their white fills would knock out: discardWhiteOverprint).
    s.execute("object.setOverprint", &json!({"stroke": true})).unwrap();
    let overprints = |s: &mut Session, p: Value| {
        let v = s.execute("document.exportPdf", &p).unwrap();
        assert!(warnings(&v).iter().all(|w| !w.contains("overprint")), "{:?}", warnings(&v));
        String::from_utf8_lossy(&b64(&v)).matches("/OP true/op true/OPM 1").count()
    };
    assert_eq!(overprints(&mut s, json!({"compression": {"compressText": false}})), 1);
    assert_eq!(overprints(&mut s, json!({"advanced": {"overprint": "discard"}})), 0);
}
