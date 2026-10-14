//! Path dialogs: Average, and Offset Path, Simplify and Split Into Grid with Preview.

use serde_json::{Value, json};
use vectorcraft_doc::Unit;

use super::{DialogSpec, form, run_and_close};
use crate::VectorcraftApp;
use crate::state::Dialog;

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |d| title(&d.kind).into(), body, confirm, preview: true, ..DialogSpec::FORM };

fn title(kind: &str) -> &'static str {
    match kind {
        "average" => tl!("Average"),
        "offsetPath" => tl!("Offset Path"),
        "simplify" => tl!("Simplify"),
        _ => tl!("Split Into Grid"),
    }
}

/// The command the dialog runs and its parameters.
fn command(d: &Dialog) -> (&'static str, Value) {
    match d.kind.as_str() {
        "average" => ("path.average", json!({"axis": d.str("axis")})),
        "offsetPath" => {
            ("object.path.offsetPath", json!({"offset": d.f64("offset", 10.0), "joins": d.str("joins"), "miterLimit": d.f64("miterLimit", 4.0)}))
        }
        "simplify" => ("object.path.simplify", json!({"tolerance": d.f64("tolerance", 1.0)})),
        _ => ("object.path.splitIntoGrid", form::params(d)),
    }
}

/// The undo label of the dialogs that preview on the canvas (all but Average).
fn previewed(kind: &str) -> Option<&'static str> {
    match kind {
        "offsetPath" => Some("Offset Path"),
        "simplify" => Some("Simplify"),
        "splitIntoGrid" => Some("Split Into Grid"),
        _ => None,
    }
}

/// The fields Split Into Grid opens with for a selection `width` × `height` points.
pub(crate) fn split_fields(width: f64, height: f64) -> Value {
    json!({"rows": 2, "columns": 2, "rowGutter": 12.0, "columnGutter": 12.0, "totalHeight": height, "totalWidth": width, "addGuides": false})
}

/// Split Into Grid's fields: the rows on the left and the columns on the right, each with its
/// number, size, gutter and total, and Add Guides. The size of a row or column is what the total
/// leaves each of them after the gutters, so it follows the other three; typing it sets the total.
fn split_grid(ui: &mut egui::Ui, d: &mut Dialog, unit: Unit) {
    const WIDTH: f32 = 84.0;
    let axes = [
        ("rows", tl!("Rows:"), tl!("Height:"), "rowGutter", "totalHeight"),
        ("columns", tl!("Columns:"), tl!("Width:"), "columnGutter", "totalWidth"),
    ];
    let gutter_label = form::humanized("gutter");
    egui::Grid::new("dlg").num_columns(4).spacing([10.0, 8.0]).show(ui, |ui| {
        for (number, label, ..) in axes {
            form::row_label(ui, label);
            if let Some(v) = crate::widgets::plain_field(ui, ("dlg-num", number), d.f64(number, 2.0), "", 0, WIDTH) {
                d.fields.insert(number.into(), json!(v.round().clamp(1.0, 500.0)));
            }
        }
        ui.end_row();
        for (number, _, label, gutter, total) in axes {
            form::row_label(ui, label);
            let n = d.f64(number, 2.0).max(1.0);
            let gaps = d.f64(gutter, 0.0) * (n - 1.0);
            if let Some(v) = crate::widgets::num_field(ui, ("dlg-size", number), Some((d.f64(total, 0.0) - gaps) / n), unit, WIDTH) {
                d.fields.insert(total.into(), json!(n * v + gaps));
            }
        }
        ui.end_row();
        for (.., gutter, _) in axes {
            form::row_label(ui, &gutter_label);
            form::length(ui, d, gutter, unit, WIDTH);
        }
        ui.end_row();
        for (.., total) in axes {
            form::row_label(ui, tl!("Total:"));
            form::length(ui, d, total, unit, WIDTH);
        }
        ui.end_row();
    });
    ui.add_space(8.0);
    form::check(ui, d, "addGuides", tl!("Add Guides"));
}

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    let unit = app.session.general_unit();
    if d.kind == "splitIntoGrid" {
        split_grid(ui, d, unit);
    } else {
        form::grid(ui, d, unit);
    }
    if let Some(label) = previewed(&d.kind) {
        let (id, params) = command(d);
        if d.fields.get(form::PREVIEWED) != Some(&params) || !d.bool("preview") {
            d.fields.remove("__counts");
        }
        if let Some(r) = form::preview_result(app, ui, d, label, id, params)
            && d.kind == "simplify"
        {
            d.fields.insert("__counts".into(), json!({"before": r["before"], "after": r["after"]}));
        }
        if d.kind == "simplify"
            && d.bool("preview")
            && let Some(counts) = d.fields.get("__counts")
            && let (Some(before), Some(after)) = (counts["before"].as_u64(), counts["after"].as_u64())
        {
            ui.label(format!("{}: {before} → {after}", tl!("Anchor Points")));
        }
    }
    false
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    let (id, params) = command(d);
    if previewed(&d.kind).is_none() {
        return run_and_close(app, id, params);
    }
    let r = form::commit_preview(app, id, params)?;
    // The fields as the dialog held them, so they show as typed when it opens again.
    if d.kind == "offsetPath" {
        let kept = ["offset", "joins", "miterLimit"].into_iter().filter_map(|k| Some((k.to_string(), d.fields.get(k)?.clone())));
        app.ui.offset_path = Value::Object(kept.collect());
    }
    Ok(r)
}
