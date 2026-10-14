//! The system clipboard's formats: Copy and Cut publish every flavour the engine makes
//! ([`Session::clipboard_flavours`](vectorcraft_engine::Session::clipboard_flavours): text, SVG,
//! PDF, PNG), and a Paste first turns what another app copied (SVG, PDF, an EMF, text, a bitmap,
//! or a copied file that is one of them) into the internal clipboard with the `clipboard.import*`
//! commands. The host installs the platform side as
//! [`Services::system_clipboard`](crate::Services::system_clipboard); without it (the web) Copy
//! publishes SVG text through egui and Paste takes SVG text only.

use serde_json::{Value, json};
use vectorcraft_engine::cmd::clipboard::{EMF, Flavour, PASTE_ORDER, PDF, SVG, TEXT, looks_like_svg};

use crate::VectorcraftApp;

/// The platform clipboard, with every format it carries.
pub trait SystemClipboard {
    /// Replace the clipboard's contents with `flavours` (best first; none empties it).
    fn write(&mut self, flavours: &[Flavour]) -> Result<(), String>;
    /// Does the clipboard still hold what [`Self::write`] last put there (nothing copied since)?
    fn holds_ours(&mut self) -> bool;
    /// The first of `mimes` the clipboard holds; `image/*` is any bitmap (returned as PNG, BMP…).
    fn read(&mut self, mimes: &[&'static str]) -> Option<Flavour>;
    /// Does the clipboard hold one of `mimes`? Cheap enough to ask a few times a second.
    fn has(&mut self, mimes: &[&'static str]) -> bool;
}

/// The command (and its params) that loads `f` into the internal clipboard, centred on `center`.
pub(crate) fn import_command(f: &Flavour, center: Option<[f64; 2]>) -> (&'static str, Value) {
    let b64 = || vectorcraft_format::base64_encode(&f.data);
    match f.mime {
        TEXT => {
            let text = String::from_utf8_lossy(&f.data);
            if looks_like_svg(&text) {
                ("clipboard.importSvg", json!({ "svg": text, "center": center }))
            } else {
                ("clipboard.importText", json!({ "text": text, "center": center }))
            }
        }
        SVG => ("clipboard.importSvg", json!({ "dataBase64": b64(), "center": center })),
        PDF => ("clipboard.importPdf", json!({ "dataBase64": b64(), "center": center })),
        EMF => ("clipboard.importEmf", json!({ "dataBase64": b64(), "center": center })),
        mime => ("clipboard.importImage", json!({ "dataBase64": b64(), "mime": mime, "center": center })),
    }
}

impl VectorcraftApp {
    /// After Copy or Cut: offer the copied objects to other apps.
    pub(crate) fn publish_clipboard(&mut self) {
        match self.services.system_clipboard.as_mut() {
            Some(cb) => {
                if let Err(e) = cb.write(&self.session.clipboard_flavours()) {
                    self.ui.status = format!("Couldn't copy to the system clipboard: {e}");
                }
            }
            None if self.session.prefs.copy_as_svg => {
                self.clipboard_out = self.session.clipboard_svg();
                self.clipboard_published = self.clipboard_out.clone();
            }
            None => {}
        }
    }

    /// Before a paste: what another app put on the system clipboard replaces the internal
    /// clipboard (centred in the view). What we published ourselves keeps the lossless internal
    /// copy. An error means nothing should be pasted.
    pub(crate) fn adopt_system_clipboard(&mut self) -> Result<(), String> {
        let event_text = self.clipboard_in.take();
        let center = self.view().map(|v| [v.center.x, v.center.y]);
        let (cmd, params) = match self.services.system_clipboard.as_mut() {
            Some(cb) => {
                if cb.holds_ours() {
                    return Ok(());
                }
                let Some(f) = cb.read(&PASTE_ORDER) else { return Ok(()) };
                import_command(&f, center)
            }
            // Text only: SVG markup is art, other text is left alone.
            None => {
                let text = event_text.or_else(|| self.services.clipboard_read.as_mut().and_then(|f| f()));
                let Some(text) = text.filter(|t| looks_like_svg(t)) else { return Ok(()) };
                if self.clipboard_published.as_deref() == Some(text.as_str()) {
                    return Ok(());
                }
                self.clipboard_published = Some(text.clone());
                ("clipboard.importSvg", json!({ "svg": text, "center": center }))
            }
        };
        self.session.execute(cmd, &params).map(drop).map_err(|e| {
            self.clipboard_published = None;
            format!("Couldn't paste from the clipboard: {e}")
        })
    }

    /// The text on the system clipboard, if any.
    pub(crate) fn system_clipboard_text(&mut self) -> Option<String> {
        match self.services.system_clipboard.as_mut() {
            Some(cb) => cb.read(&[TEXT]).map(|f| String::from_utf8_lossy(&f.data).into_owned()),
            None => self.services.clipboard_read.as_mut().and_then(|read| read()),
        }
    }

    /// Does the system clipboard hold something Paste can take (with nothing copied in the app)?
    pub(crate) fn system_clipboard_pasteable(&mut self) -> bool {
        match self.services.system_clipboard.as_mut() {
            Some(cb) => cb.has(&PASTE_ORDER),
            None => self.services.clipboard_read.as_mut().and_then(|read| read()).is_some_and(|t| looks_like_svg(&t)),
        }
    }
}
