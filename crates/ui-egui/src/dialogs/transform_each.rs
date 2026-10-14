//! Object → Transform → Transform Each: scale, move and rotate every selected object about its own
//! reference point, with reflection, randomness, Scale Corners and Scale Strokes & Effects
//! (`object.transformEach`), previewed live on the canvas.
//!
//! Fields: `scaleH`, `scaleV` (%), `moveH`, `moveV` (pt), `rotate` (°), `reflectX`, `reflectY`,
//! `random`, `reference` (0..8, the 9-point grid), `copy`, `corners`, `strokes` (default: the
//! preferences, which OK updates) and `preview`.

use serde_json::{Value, json};

use super::transform::{add_scale_options, scale_options, with_scale_options};
use super::{DialogSpec, form};
use crate::VectorcraftApp;
use crate::state::Dialog;
use crate::widgets;

/// The dialog kind of Transform Each.
pub const KIND: &str = "transformEach";

const CMD: &str = "object.transformEach";

pub(super) const SPEC: DialogSpec =
    DialogSpec { heading: |_| tl!("Transform Each").into(), body, confirm, preview: true, min_width: 360.0, ..DialogSpec::FORM };

/// The fields a fresh dialog starts with.
pub fn fields() -> Value {
    json!({"scaleH": 100, "scaleV": 100, "moveH": 0, "moveV": 0, "rotate": 0, "reflectX": false, "reflectY": false, "random": false, "reference": 4, "copy": false})
}

/// The command's parameters without the scale options.
fn params(d: &Dialog) -> Value {
    let f = |k: &str, v: f64| d.f64(k, v);
    json!({
        "scaleH": f("scaleH", 100.0), "scaleV": f("scaleV", 100.0), "moveH": f("moveH", 0.0), "moveV": f("moveV", 0.0),
        "rotate": f("rotate", 0.0), "reflectX": d.bool("reflectX"), "reflectY": d.bool("reflectY"), "random": d.bool("random"),
        "reference": f("reference", 4.0).clamp(0.0, 8.0) as u64, "copy": d.bool("copy"),
    })
}

/// A labelled number row of the field grid: `key` shown with `suffix` (a distance in `units` for
/// "pt").
fn number(ui: &mut egui::Ui, d: &mut Dialog, key: &str, label: &str, suffix: &str, units: vectorcraft_doc::Unit) {
    widgets::dim_label(ui, label);
    if suffix == "pt" {
        form::length(ui, d, key, units, 90.0);
    } else if let Some(n) = widgets::plain_field(ui, ("te", key), d.f64(key, 0.0), suffix, 2, 90.0) {
        d.fields.insert(key.into(), json!(n));
    }
    ui.end_row();
}

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    let units = app.session.general_unit();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            egui::Grid::new("te-grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                for (head, rows) in [
                    (tl!("Scale"), [("scaleH", tl!("Horizontal:"), "%"), ("scaleV", tl!("Vertical:"), "%")].as_slice()),
                    (tl!("Move"), &[("moveH", tl!("Horizontal:"), "pt"), ("moveV", tl!("Vertical:"), "pt")]),
                    (tl!("Rotate"), &[("rotate", tl!("Angle:"), "°")]),
                ] {
                    widgets::subheader(ui, head);
                    ui.end_row();
                    for (key, label, suffix) in rows {
                        number(ui, d, key, label, suffix, units);
                    }
                }
            });
        });
        ui.add_space(16.0);
        ui.vertical(|ui| {
            widgets::subheader(ui, tl!("Options"));
            ui.add_space(4.0);
            scale_options(app, ui, d);
            form::check(ui, d, "reflectX", tl!("Reflect X"));
            form::check(ui, d, "reflectY", tl!("Reflect Y"));
            ui.horizontal(|ui| {
                let cur = d.f64("reference", 4.0).clamp(0.0, 8.0) as usize;
                if let Some(i) = widgets::reference_point(ui, cur) {
                    d.fields.insert("reference".into(), json!(i));
                }
                widgets::dim_label(ui, tl!("Reference Point"));
            });
            form::check(ui, d, "random", tl!("Random"));
            form::check(ui, d, "copy", tl!("Copy"));
        });
    });
    let mut p = params(d);
    add_scale_options(app, d, &mut p);
    form::preview(app, ui, d, "Transform Each", CMD, p);
    false
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    let p = with_scale_options(app, d, params(d))?;
    form::commit_preview(app, CMD, p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_doc::NodeId;
    use vectorcraft_engine::Session;

    #[test]
    fn ok_transforms_each_and_keeps_the_scale_options() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
        let id = app.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 50, "height": 50})).unwrap()["id"].as_u64().unwrap();
        app.run("select.all", json!({})).unwrap();
        app.run("prefs.set", json!({"key": "scaleStrokes", "value": true})).unwrap();
        app.run("ui.menuDialog", json!({ "command": CMD })).unwrap();
        assert_eq!(app.ui.dialog.as_ref().map(|d| d.kind.as_str()), Some(KIND));
        // One drawn frame: the checkboxes start from the preferences.
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| super::super::show(&mut app, ui.ctx()));
        out.textures_delta.clear();
        let d = app.ui.dialog.as_mut().unwrap();
        assert_eq!(d.fields["strokes"], json!(true));
        d.fields.insert("scaleH".into(), json!(200));
        d.fields.insert("scaleV".into(), json!(200));
        d.fields.insert("strokes".into(), json!(false));
        super::super::confirm(&mut app).unwrap();
        assert!(app.ui.dialog.is_none());
        assert!(!app.session.prefs.scale_strokes, "OK keeps the option as the preference");
        let n = app.session.doc().unwrap().doc.node(NodeId(id)).unwrap().clone();
        assert!((n.geometric_bounds().unwrap().width() - 100.0).abs() < 1e-6);
        assert_eq!(n.appearance.stroke_width(), 1.0, "strokes kept their weight");
        let (cmd, p) = app.session.journal.last().unwrap();
        assert_eq!((cmd.as_str(), &p["strokes"]), (CMD, &json!(false)));
    }
}
