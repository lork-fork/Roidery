//! File → New (`file.new`, `file.newPresets*`): artboard layouts, orientation, bleed, colour mode,
//! raster effects resolution, background, presets, Recent and Saved.

use serde_json::{Value, json};
use vectorcraft_doc::{Background, ColorMode, Unit};
use vectorcraft_geom::Rect;

use super::*;

fn new_doc(p: Value) -> Session {
    let mut s = Session::new();
    s.execute("file.new", &p).unwrap();
    s
}

fn boards(s: &Session) -> Vec<Rect> {
    s.doc().unwrap().doc.artboards.iter().map(|a| a.rect).collect()
}

fn presets(s: &mut Session, category: &str) -> Vec<Value> {
    let r = s.execute("file.newPresets", &json!({ "category": category })).unwrap();
    r["categories"][0]["presets"].as_array().unwrap().clone()
}

#[test]
fn four_artboards_in_two_columns_make_a_two_by_two_grid() {
    let s = new_doc(json!({"width": 100, "height": 50, "artboards": 4, "artboardLayout": {"columns": 2, "spacing": 10}}));
    let origins: Vec<(f64, f64)> = boards(&s).iter().map(|r| (r.x0, r.y0)).collect();
    assert_eq!(origins, [(0.0, 0.0), (110.0, 0.0), (0.0, 60.0), (110.0, 60.0)]);
    assert!(boards(&s).iter().all(|r| r.width() == 100.0 && r.height() == 50.0));
    let names: Vec<String> = s.doc().unwrap().doc.artboards.iter().map(|a| a.name.clone()).collect();
    assert_eq!(names, ["Artboard 1", "Artboard 2", "Artboard 3", "Artboard 4"]);
    // Grid by Column fills down first; Right-to-Left starts at the right.
    let s = new_doc(json!({"width": 100, "height": 50, "artboards": 4, "artboardLayout": {"layout": "gridByColumn", "columns": 2, "spacing": 10}}));
    assert_eq!(boards(&s).iter().map(|r| (r.x0, r.y0)).collect::<Vec<_>>(), [(0.0, 0.0), (0.0, 60.0), (110.0, 0.0), (110.0, 60.0)]);
    let s = new_doc(json!({"width": 100, "height": 50, "artboards": 3, "artboardLayout": {"layout": "row", "rightToLeft": true}}));
    assert_eq!(boards(&s).iter().map(|r| r.x0).collect::<Vec<_>>(), [240.0, 120.0, 0.0]);
    let s = new_doc(json!({"width": 100, "height": 50, "artboards": 3, "artboardLayout": "column"}));
    assert_eq!(boards(&s).iter().map(|r| r.y0).collect::<Vec<_>>(), [0.0, 70.0, 140.0]);
    // Without a layout: one row, 20 pt apart (as before).
    let s = new_doc(json!({"width": 100, "height": 50, "artboards": 2}));
    assert_eq!(boards(&s)[1].x0, 120.0);
}

#[test]
fn orientation_swaps_width_and_height() {
    let s = new_doc(json!({"width": 612, "height": 792, "orientation": "landscape"}));
    assert_eq!((boards(&s)[0].width(), boards(&s)[0].height()), (792.0, 612.0));
    let s = new_doc(json!({"width": 792, "height": 612, "orientation": "Portrait"}));
    assert_eq!((boards(&s)[0].width(), boards(&s)[0].height()), (612.0, 792.0));
    let s = new_doc(json!({"preset": "HDTV 1080p", "orientation": "portrait"}));
    assert_eq!((boards(&s)[0].width(), boards(&s)[0].height()), (1080.0, 1920.0));
}

#[test]
fn every_setting_reaches_the_document() {
    let mut s = Session::new();
    let r = s
        .execute(
            "file.new",
            &json!({"name": "Poster", "width": "210 mm", "height": "297mm", "units": "Millimeters", "bleed": [9, 9, 4.5, 4.5],
                "backgroundContents": "white", "colorMode": "cmyk", "rasterEffectsPpi": 150}),
        )
        .unwrap();
    assert_eq!(r["previewMode"], "default");
    let d = &s.doc().unwrap().doc;
    assert_eq!(d.title, "Poster");
    assert_eq!(d.units, Unit::Millimeters);
    assert!((d.artboards[0].rect.width() - 595.2756).abs() < 1e-3);
    assert_eq!(d.setup.bleed, [9.0, 9.0, 4.5, 4.5]);
    assert_eq!(d.setup.background, Background::White);
    assert_eq!(d.color_mode, ColorMode::Cmyk);
    assert_eq!(d.raster_effects_ppi, 150.0);
    // Saved and reopened, the settings stay.
    let back = vectorcraft_format::load(&vectorcraft_format::save_file(d)).unwrap();
    assert_eq!((back.raster_effects_ppi, back.setup.bleed, back.setup.background), (150.0, d.setup.bleed, Background::White));
}

#[test]
fn a_white_background_exports_opaque() {
    let alpha = |bg: &str| {
        let mut s = new_doc(json!({"width": 20, "height": 20, "backgroundContents": bg}));
        let r = s.execute("document.export", &json!({"format": "png"})).unwrap();
        let png = vectorcraft_format::base64_decode(r["dataBase64"].as_str().unwrap()).unwrap();
        image::load_from_memory(&png).unwrap().to_rgba8().get_pixel(10, 10).0
    };
    assert_eq!(alpha("white"), [255, 255, 255, 255]);
    assert_eq!(alpha("transparent")[3], 0);
}

#[test]
fn a_preset_starts_the_document_and_params_change_it() {
    let s = new_doc(json!({"preset": "a4"}));
    let d = &s.doc().unwrap().doc;
    assert_eq!((d.units, d.color_mode, d.raster_effects_ppi), (Unit::Points, ColorMode::Cmyk, 300.0), "print sizes in prefs unitsGeneral");
    assert!((d.artboards[0].rect.height() - 841.89).abs() < 0.01);
    let s = new_doc(json!({"preset": "Web 1920×1080", "colorMode": "cmyk", "artboards": 2}));
    let d = &s.doc().unwrap().doc;
    assert_eq!((d.units, d.color_mode, d.artboards.len()), (Unit::Pixels, ColorMode::Cmyk, 2));
    assert!(Session::new().execute("file.new", &json!({"preset": "Napkin"})).is_err());
    // Print presets and plain sizes start in Preferences ▸ Units ▸ General; screen presets in pixels.
    let mut s = Session::new();
    s.execute("prefs.set", &json!({"key": "unitsGeneral", "value": "millimeters"})).unwrap();
    s.execute("file.new", &json!({"preset": "A4"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.units, Unit::Millimeters);
    assert_eq!(presets(&mut s, "Print")[4]["size"], "210 × 297 mm");
    s.execute("file.new", &json!({"width": 100})).unwrap();
    assert_eq!(s.doc().unwrap().doc.units, Unit::Millimeters);
    s.execute("file.new", &json!({"preset": "HDTV 720p"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.units, Unit::Pixels);
}

#[test]
fn ppi_overprint_preview_and_bad_params() {
    // Overprint Preview is process-wide.
    let _view = crate::tests_colormgmt::GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = new_doc(json!({"rasterEffectsPpi": 300, "previewMode": "overprint"}));
    assert_eq!(s.doc().unwrap().doc.raster_effects_ppi, 300.0);
    let overprint = |s: &mut Session| s.execute("view.proofSetup", &json!({})).unwrap()["overprintPreview"].clone();
    assert_eq!(overprint(&mut s), true);
    s.execute("view.overprintPreview", &json!({"on": false})).unwrap();
    let r = s.execute("file.new", &json!({"previewMode": "pixel"})).unwrap();
    assert_eq!(r["previewMode"], "pixel", "for the app's Pixel Preview");
    assert_eq!(overprint(&mut s), false);
    for p in [
        json!({"orientation": "sideways"}),
        json!({"width": -5}),
        json!({"width": "wide"}),
        json!({"bleed": 100}),
        json!({"backgroundContents": "plaid"}),
        json!({"colorMode": "lab"}),
        json!({"rasterEffectsPpi": 0}),
        json!({"previewMode": "xray"}),
        json!({"artboardLayout": {"layout": "spiral"}}),
        json!({"artboardLayout": {"columns": 0}}),
        json!({"artboardLayout": {"rows": 2}}),
        json!({"units": "furlongs"}),
        json!({"width": 160000, "artboards": 1000}),
    ] {
        let mut s = Session::new();
        assert!(s.execute("file.new", &p).is_err(), "{p}");
        assert!(s.active().is_none(), "{p}: no document on an error");
    }
}

#[test]
fn categories_list_generated_presets_with_neutral_names() {
    let mut s = Session::new();
    let r = s.execute("file.newPresets", &json!({})).unwrap();
    let names: Vec<&str> = r["categories"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Recent", "Saved", "Embroidery", "Mobile", "Web", "Print", "Film & Video", "Art & Illustration", "Branding", "Social"]);
    // Roidery's hoops: sewing fields, shown in the General unit (points by default).
    let hoop = presets(&mut s, "Embroidery").into_iter().find(|p| p["name"] == "Hoop 130 × 180 mm").unwrap();
    assert_eq!((hoop["size"].as_str(), hoop["colorMode"].as_str()), (Some("368.504 × 510.236 pt"), Some("rgb")));
    let print = presets(&mut s, "print");
    let letter = print.iter().find(|p| p["name"] == "Letter").unwrap();
    assert_eq!(
        (letter["size"].as_str(), letter["colorMode"].as_str(), letter["orientation"].as_str()),
        (Some("612 × 792 pt"), Some("cmyk"), Some("portrait"))
    );
    assert_eq!(presets(&mut s, "Film & Video").iter().find(|p| p["name"] == "4K UHD").unwrap()["size"], "3840 × 2160 px");
    for c in ["Mobile", "Web", "Print", "Film & Video", "Art & Illustration", "Branding", "Social"] {
        assert!(!presets(&mut s, c).is_empty(), "{c}");
    }
    // A listed preset can be passed straight back to file.new.
    let story = presets(&mut s, "Social").into_iter().find(|p| p["name"] == "Social Story 1080×1920").unwrap();
    s.execute("file.new", &story).unwrap();
    assert_eq!(boards(&s)[0], Rect::new(0.0, 0.0, 1080.0, 1920.0));
    assert!(s.execute("file.newPresets", &json!({"category": "Furniture"})).is_err());
}

#[test]
fn recent_remembers_the_last_sizes() {
    let mut s = Session::new();
    assert!(presets(&mut s, "Recent").is_empty());
    s.execute("file.new", &json!({"preset": "A4"})).unwrap();
    s.execute("file.new", &json!({"width": 300, "height": 200})).unwrap();
    s.execute("file.new", &json!({"preset": "A4", "orientation": "landscape"})).unwrap();
    let recent = presets(&mut s, "Recent");
    let names: Vec<&str> = recent.iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["A4", "Custom", "A4"], "newest first; a turned preset keeps its name");
    // The same size again moves to the front instead of repeating.
    s.execute("file.new", &json!({"width": 300, "height": 200})).unwrap();
    assert_eq!(presets(&mut s, "Recent").len(), 3);
    assert_eq!(presets(&mut s, "Recent")[0]["size"], "300 × 200 pt");
    for i in 0..20 {
        s.execute("file.new", &json!({"width": 100 + i, "height": 100})).unwrap();
    }
    assert_eq!(presets(&mut s, "Recent").len(), 12);
}

#[test]
fn saved_presets_live_in_the_preferences() {
    let mut s = Session::new();
    assert!(s.execute("file.newPresets.delete", &json!({"name": "Mine"})).is_err());
    let r = s
        .execute(
            "file.newPresets.save",
            &json!({"name": "Mine", "width": 400, "height": 300, "units": "Pixels", "artboards": 4, "artboardLayout": {"columns": 2}, "bleed": 9}),
        )
        .unwrap();
    assert_eq!(r, json!({"name": "Mine", "count": 1}));
    let saved = presets(&mut s, "Saved");
    assert_eq!(
        (saved[0]["name"].as_str(), saved[0]["size"].as_str(), saved[0]["artboardLayout"]["columns"].as_u64()),
        (Some("Mine"), Some("400 × 300 px"), Some(2))
    );
    // The same name replaces it.
    s.execute("file.newPresets.save", &json!({"name": "mine", "width": 500, "height": 300})).unwrap();
    assert_eq!(presets(&mut s, "Saved").len(), 1);
    s.execute("file.new", &json!({"preset": "Mine"})).unwrap();
    assert_eq!(boards(&s)[0].width(), 500.0);
    // Kept with the preferences, and by a reset.
    let prefs: crate::Prefs = serde_json::from_value(s.prefs.to_json()).unwrap();
    assert_eq!(prefs.new_doc_presets, s.prefs.new_doc_presets);
    s.execute("prefs.reset", &json!({})).unwrap();
    assert_eq!(s.prefs.new_doc_presets.len(), 1);
    assert_eq!(s.execute("file.newPresets.delete", &json!({"name": "MINE"})).unwrap(), json!({"deleted": "mine"}));
    assert!(presets(&mut s, "Saved").is_empty());
    assert!(s.execute("file.newPresets.save", &json!({"name": " "})).is_err());
}

#[test]
fn rearrange_keeps_its_grid() {
    let mut s = new_doc(json!({"width": 100, "height": 50, "artboards": 3}));
    s.execute("artboard.rearrange", &json!({"columns": 2, "spacing": 5})).unwrap();
    assert_eq!(boards(&s).iter().map(|r| (r.x0, r.y0)).collect::<Vec<_>>(), [(0.0, 0.0), (105.0, 0.0), (0.0, 55.0)]);
}
