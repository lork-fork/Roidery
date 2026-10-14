//! Symbol Options: the name of the symbol New Symbol makes from the selection (`symbol.new`), asked
//! before the symbol is made, as the Symbols panel's New Symbol button and menu item do.
//!
//! Fields: `name`.

use serde_json::{Value, json};

use super::swatch_options::{grid, label};
use super::{DialogSpec, form};
use crate::VectorcraftApp;
use crate::state::Dialog;

/// The dialog kind of Symbol Options.
pub const KIND: &str = "symbolOptions";

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |_| tl!("Symbol Options").into(), body, confirm, min_width: 320.0, ..DialogSpec::FORM };

/// Open Symbol Options to name a new symbol made from the selection.
pub fn open(app: &mut VectorcraftApp) -> Result<Value, String> {
    let st = app.session.active().ok_or("no document open")?;
    if st.selection.is_empty() {
        return Err("nothing selected".into());
    }
    let name = st.doc.unique_symbol_name("New Symbol");
    app.ui.dialog = Some(Dialog::new(KIND, json!({ "name": name })));
    Ok(Value::Null)
}

fn body(_: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    grid(ui, |ui| {
        label(ui, tl!("Name:"));
        form::text(ui, d, "name", 190.0);
        ui.end_row();
    });
    false
}

/// OK: make the symbol from the selection with the name typed.
fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    let r = app.run("symbol.new", json!({ "name": d.str("name") }));
    if r.is_ok() {
        app.ui.dialog = None;
    }
    r
}
