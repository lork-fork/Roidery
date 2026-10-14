//! The generic parameter dialog (`ui.paramDialog`): edits a command's parameters (`__command`,
//! headed `__label`) and runs it on OK.

use serde_json::Value;

use super::{DialogSpec, form};
use crate::VectorcraftApp;
use crate::state::Dialog;

pub(super) const SPEC: DialogSpec = DialogSpec {
    heading: |d| tl!(&d.str("__label")).to_string(),
    body: |app, ui, d| {
        let command = d.str("__command");
        let lengths = lengths(&command);
        form::param_fields(ui, d, &|k| lengths.contains(&k), &|k| choices(&command, k), app.session.general_unit());
        false
    },
    confirm,
    ..DialogSpec::FORM
};

/// The parameters of the commands this dialog edits that are distances.
fn lengths(command: &str) -> &'static [&'static str] {
    match command {
        "graph.create" => &["width", "height"],
        "shape.flare" => &["diameter", "pathLength"],
        "artboard.rearrange" => &["spacing"],
        "perspective.grid.set" => &["cell", "distance"],
        "object.repeat.options" => &["radius", "hSpacing", "vSpacing"],
        "text.areaOptions" => &["width", "height", "gutter", "inset", "firstBaselineMin"],
        "type.pathOptions" => &["spacing"],
        _ => &[],
    }
}

/// The parameters of the commands this dialog edits that pick one of some values.
fn choices(command: &str, key: &str) -> Option<form::Choices> {
    match (command, key) {
        ("type.pathOptions", "effect") => Some(crate::menus::PATH_EFFECTS),
        ("type.pathOptions", "alignToPath") => Some(PATH_ALIGN),
        _ => None,
    }
}

/// Type on a Path Options › Align to Path.
const PATH_ALIGN: form::Choices = &[("Ascender", "ascender"), ("Descender", "descender"), ("Center", "center"), ("Baseline", "baseline")];

/// Closes before running, so a dialog the command opens stays open.
fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    let cmd = d.str("__command");
    let params = form::params(d);
    app.ui.dialog = None;
    // A tool's click-to-size shape (Flare) goes on the active perspective plane while the grid shows.
    let at = |x: &str, y: &str| Some(vectorcraft_geom::Point::new(params.get(x)?.as_f64()?, params.get(y)?.as_f64()?));
    if let Some((c, p)) =
        at("cx", "cy").or_else(|| at("x", "y")).and_then(|pt| vectorcraft_engine::perspective_click(&app.session, &cmd, &params, pt))
    {
        return app.run(&c, p);
    }
    app.run(&cmd, params)
}
