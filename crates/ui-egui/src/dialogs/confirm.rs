//! A question answered with OK or Cancel; OK runs a command ([`ask`]). Used for confirmations such
//! as deleting swatches.
//!
//! Fields: `message` (the question, shown as the heading), `detail` (an optional line under it),
//! `__command` and `__params` (the command OK runs). Both texts are shown as given: callers
//! translate their template before filling in a name, so the name is never translated.

use serde_json::{Value, json};

use super::DialogSpec;
use crate::VectorcraftApp;
use crate::state::Dialog;
use crate::theme::Tokens;

/// The dialog kind of a confirmation.
pub const KIND: &str = "confirm";

pub(super) const SPEC: DialogSpec = DialogSpec {
    heading: |d| d.str("message"),
    body: |_, ui, d| {
        let detail = d.str("detail");
        if !detail.is_empty() {
            ui.label(egui::RichText::new(detail).color(Tokens::get(ui.ctx()).text_dim));
        }
        false
    },
    confirm,
    max_width: Some(420.0),
    ..DialogSpec::FORM
};

/// Ask `message` (with an optional `detail` line), both in the UI language; OK runs `command`
/// with `params`.
pub fn ask(app: &mut VectorcraftApp, message: &str, detail: &str, command: &str, params: Value) {
    let fields = json!({"message": message, "detail": detail, "__command": command, "__params": params});
    app.ui.dialog = Some(Dialog::new(KIND, fields));
}

/// The dialog kind of a message with a Close button only ([`tell`]).
pub const MESSAGE: &str = "message";

pub(super) const MESSAGE_SPEC: DialogSpec = DialogSpec { ok: None, ..SPEC };

/// Most messages kept for later while a dialog is open ([`tell`]).
const MAX_WAITING: usize = 16;

/// Tell the user `message` (with an optional `detail` line), both in the UI language, in a dialog
/// they close. While another dialog is open (the message comes from work in the background) it
/// waits its turn instead of replacing it: neither what the user was doing there nor the message
/// is lost.
pub fn tell(app: &mut VectorcraftApp, message: &str, detail: &str) {
    let d = Dialog::new(MESSAGE, json!({"message": message, "detail": detail}));
    if app.ui.dialog.is_none() {
        app.ui.dialog = Some(d);
    } else if app.ui.told.len() < MAX_WAITING && !app.ui.dialog.iter().chain(&app.ui.told).any(|t| t.kind == MESSAGE && t.fields == d.fields) {
        app.ui.told.push(d);
    }
}

/// Show the next message that waited for a dialog to close (each frame).
pub(super) fn show_waiting(app: &mut VectorcraftApp) {
    if app.ui.dialog.is_none() && !app.ui.told.is_empty() {
        app.ui.dialog = Some(app.ui.told.remove(0));
    }
}

/// Closes before running, so a dialog the command opens stays open.
fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    let params = d.fields.get("__params").cloned().unwrap_or_else(|| json!({}));
    app.ui.dialog = None;
    app.run(&d.str("__command"), params)
}
