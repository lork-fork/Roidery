//! Save Swatch Library: a name, a file format and where it goes: the user library folder (listed
//! under Window → Swatch Libraries → User Defined) or a file picked in a save dialog (a download
//! on the web). OK runs `swatch.library.save`.
//!
//! Fields: `name`, `format` (`vcswatches`, `gpl` or `css`), `user` (save to the user library
//! folder), `selectedOnly` and `names` (the swatches selected in the Swatches panel), `__user`
//! (there is a user library folder).
//!
//! The name and Save To rows and the saving are shared with Save Graphic Style Library
//! ([`super::save_style_library`]).

use serde_json::{Value, json};
use vectorcraft_color::palette_io::PaletteFormat;

use super::swatch_options::{grid, label};
use super::{DialogSpec, form, run_and_close};
use crate::state::Dialog;
use crate::{VectorcraftApp, widgets};

/// The dialog kind of Save Swatch Library.
pub const KIND: &str = "saveSwatchLibrary";

pub(super) const SPEC: DialogSpec =
    DialogSpec { heading: |_| tl!("Save Swatch Library").into(), body, confirm, min_width: 360.0, ..DialogSpec::FORM };

/// Open Save Swatch Library for the document's swatches (`names`: the ones selected in the panel).
pub fn open(app: &mut VectorcraftApp, names: Vec<String>) -> Result<Value, String> {
    let mut fields = fields(app, app.session.swatch_libraries.user_dir().is_some(), names)?;
    fields["format"] = json!(PaletteFormat::Native.id());
    app.ui.dialog = Some(Dialog::new(KIND, fields));
    Ok(Value::Null)
}

/// The fields a Save … Library dialog opens with: the document's name, Save To the user library
/// folder when there is one (`user`), and the panel's selected `names`.
pub(super) fn fields(app: &VectorcraftApp, user: bool, names: Vec<String>) -> Result<Value, String> {
    let st = app.session.active().ok_or("no document open")?;
    let title = st.title();
    let name = title.rsplit_once('.').map_or(title.as_str(), |(s, _)| s).to_string();
    Ok(json!({"name": name, "user": user, "__user": user, "selectedOnly": false, "names": names}))
}

fn format_of(d: &Dialog) -> PaletteFormat {
    PaletteFormat::parse(&d.str("format")).unwrap_or(PaletteFormat::Native)
}

fn set(d: &mut Dialog, key: &str, v: Value) {
    d.fields.insert(key.into(), v);
}

fn body(_: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    let labels = PaletteFormat::ALL.map(PaletteFormat::label);
    grid(ui, |ui| {
        name_row(ui, d);
        label(ui, tl!("Format:"));
        if let Some(i) = widgets::dropdown(ui, "library-format", format_of(d).label(), &labels, 230.0) {
            set(d, "format", json!(PaletteFormat::ALL[i].id()));
        }
        ui.end_row();
        destination(ui, d, tl!("Selected Swatches Only"));
    });
    if format_of(d) == PaletteFormat::Gpl {
        widgets::dim_label(ui, tl!("GPL palettes keep solid colours only, as RGB."));
    }
    false
}

/// The Name row of a Save … Library dialog's [`grid`].
pub(super) fn name_row(ui: &mut egui::Ui, d: &mut Dialog) {
    label(ui, tl!("Name:"));
    form::text(ui, d, "name", 220.0);
    ui.end_row();
}

/// The Save To rows of a Save … Library dialog's [`grid`]: the user library folder (when there is
/// one) or a file, and `only` (Selected Swatches Only) with the number of selected items.
pub(super) fn destination(ui: &mut egui::Ui, d: &mut Dialog, only: &str) {
    let names = d.fields.get("names").and_then(Value::as_array).map_or(0, Vec::len);
    label(ui, tl!("Save To:"));
    let (user, has_user) = (d.bool("user"), d.bool("__user"));
    if widgets::radio(ui, tl!("User Defined Libraries"), user, has_user) {
        set(d, "user", json!(true));
    }
    ui.end_row();
    ui.label("");
    if widgets::radio(ui, tl!("A File…"), !user, true) {
        set(d, "user", json!(false));
    }
    ui.end_row();
    ui.label("");
    let selected = d.bool("selectedOnly");
    if widgets::check(ui, &format!("{only} ({names})"), selected, names > 0) {
        set(d, "selectedOnly", json!(!selected));
    }
    ui.end_row();
}

/// The save command's parameters from the shared fields: `name`, `user` and the selected `names`.
pub(super) fn params(d: &Dialog) -> Value {
    let mut p = json!({"name": d.str("name"), "user": d.bool("user") && d.bool("__user")});
    if d.bool("selectedOnly") {
        p["names"] = d.fields.get("names").cloned().unwrap_or_else(|| json!([]));
    }
    p
}

/// Run save command `cmd` with `p`: into the user library folder, else to a file the user picks
/// (extension `ext`).
pub(super) fn save(app: &mut VectorcraftApp, d: &Dialog, cmd: &str, ext: &str, p: Value) -> Result<Value, String> {
    if p["user"] == json!(true) {
        let r = run_and_close(app, cmd, p)?;
        app.status(format!("Saved to User Defined: {}", d.str("name")));
        return Ok(r);
    }
    app.ui.dialog = None;
    crate::io::save_command_output(app, cmd, ext, p).map(|path| json!({ "path": path }))
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    let format = format_of(d).id();
    let mut p = params(d);
    p["format"] = json!(format);
    save(app, d, "swatch.library.save", format, p)
}
