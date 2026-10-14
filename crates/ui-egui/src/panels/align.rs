//! Align panel: Align Objects, Distribute Objects, Distribute Spacing (with a spacing value when
//! aligning to a key object) and Align To (selection / key object / artboard).

use egui::Ui;
use serde_json::{Value, json};

use super::{pstate, selection_len, set_pstate};
use crate::VectorcraftApp;
use crate::widgets::{self, menu_item};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlignTo {
    #[default]
    Selection,
    Key,
    Artboard,
}

impl AlignTo {
    pub fn param(self) -> &'static str {
        match self {
            AlignTo::Selection => "selection",
            AlignTo::Key => "key",
            AlignTo::Artboard => "artboard",
        }
    }
}

/// `object.align` params: a single object aligns to the artboard (Illustrator does the same).
pub fn align_params(base: Value, to: AlignTo, n_selected: usize) -> Value {
    let mut p = base;
    let to = if n_selected == 1 && to == AlignTo::Selection { AlignTo::Artboard } else { to };
    p["to"] = json!(to.param());
    p
}

pub const ALIGN: [(&str, &str, &str, &str); 6] = [
    ("dc-al-left", "Horizontal Align Left", "horizontal", "left"),
    ("dc-al-hcenter", "Horizontal Align Center", "horizontal", "center"),
    ("dc-al-right", "Horizontal Align Right", "horizontal", "right"),
    ("dc-al-top", "Vertical Align Top", "vertical", "top"),
    ("dc-al-vcenter", "Vertical Align Center", "vertical", "center"),
    ("dc-al-bottom", "Vertical Align Bottom", "vertical", "bottom"),
];

pub const DISTRIBUTE: [(&str, &str, &str, &str); 6] = [
    ("dc-dist-top", "Vertical Distribute Top", "vertical", "top"),
    ("dc-dist-vcenter", "Vertical Distribute Center", "vertical", "center"),
    ("dc-dist-bottom", "Vertical Distribute Bottom", "vertical", "bottom"),
    ("dc-dist-left", "Horizontal Distribute Left", "horizontal", "left"),
    ("dc-dist-hcenter", "Horizontal Distribute Center", "horizontal", "center"),
    ("dc-dist-right", "Horizontal Distribute Right", "horizontal", "right"),
];

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let n = selection_len(app);
    let to: AlignTo = pstate(ui.ctx(), "align-to");
    widgets::subheader(ui, tl!("Align Objects:"));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        for (i, (icon, tip, axis, v)) in ALIGN.iter().enumerate() {
            if i == 3 {
                ui.add_space(6.0);
            }
            if widgets::icon_button_enabled(ui, icon, tl!(tip), false, n >= 1, 32.0).clicked() {
                app.run("object.align", align_params(json!({*axis: v}), to, n)).ok();
            }
        }
    });
    widgets::divider(ui);
    widgets::subheader(ui, tl!("Distribute Objects:"));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        for (i, (icon, tip, axis, v)) in DISTRIBUTE.iter().enumerate() {
            if i == 3 {
                ui.add_space(6.0);
            }
            if widgets::icon_button_enabled(ui, icon, tl!(tip), false, n >= 2, 32.0).clicked() {
                app.run("object.distribute", json!({*axis: v})).ok();
            }
        }
    });
    if pstate::<bool>(ui.ctx(), "align-hide-options") {
        return;
    }
    widgets::divider(ui);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            widgets::subheader(ui, tl!("Distribute Spacing:"));
            let spacing: f64 = pstate::<Option<f64>>(ui.ctx(), "align-spacing").unwrap_or(0.0);
            let key = to == AlignTo::Key;
            ui.horizontal(|ui| {
                let on = n >= 2;
                let sp = if key { Some(spacing) } else { None };
                if widgets::icon_button_enabled(ui, "dc-dist-vspace", tl!("Vertical Distribute Space"), false, on, 28.0).clicked() {
                    app.run("object.distributeSpacing", json!({"axis": "vertical", "spacing": sp})).ok();
                }
                if widgets::icon_button_enabled(ui, "dc-dist-hspace", tl!("Horizontal Distribute Space"), false, on, 28.0).clicked() {
                    app.run("object.distributeSpacing", json!({"axis": "horizontal", "spacing": sp})).ok();
                }
                ui.add_enabled_ui(key, |ui| {
                    if let Some(v) = widgets::spin_field(ui, "align-spacing", Some(spacing), app.session.general_unit(), 70.0, 1.0, 0.0, &[]) {
                        set_pstate(ui.ctx(), "align-spacing", Some(v));
                    }
                });
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            widgets::subheader(ui, tl!("Align To:"));
            ui.horizontal(|ui| {
                for (v, icon, tip) in [
                    (AlignTo::Selection, "dc-alignto-selection", tl!("Align to Selection")),
                    (AlignTo::Key, "dc-alignto-key", tl!("Align to Key Object")),
                    (AlignTo::Artboard, "dc-alignto-artboard", tl!("Align to Artboard")),
                ] {
                    if widgets::icon_button(ui, icon, tl!(tip), to == v, 28.0).clicked() {
                        set_pstate(ui.ctx(), "align-to", v);
                    }
                }
            });
        });
    });
}

pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    let hidden: bool = pstate(ui.ctx(), "align-hide-options");
    if menu_item(ui, if hidden { tl!("Show Options") } else { tl!("Hide Options") }, true, false) {
        set_pstate(ui.ctx(), "align-hide-options", !hidden);
    }
    let pb = app.session.prefs.use_preview_bounds;
    if menu_item(ui, tl!("Use Preview Bounds"), true, pb) {
        super::transform::set_pref(app, "usePreviewBounds", !pb);
    }
    let to: AlignTo = pstate(ui.ctx(), "align-to");
    if menu_item(ui, tl!("Cancel Key Object"), to == AlignTo::Key, false) {
        set_pstate(ui.ctx(), "align-to", AlignTo::Selection);
        app.run("select.key", json!({})).ok();
    }
    menu_item(ui, tl!("Align to Glyph Bounds"), false, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_object_aligns_to_artboard() {
        let p = align_params(json!({"horizontal": "left"}), AlignTo::Selection, 1);
        assert_eq!(p["to"], "artboard");
        let p = align_params(json!({"vertical": "top"}), AlignTo::Selection, 3);
        assert_eq!(p["to"], "selection");
        assert_eq!(align_params(json!({}), AlignTo::Key, 1)["to"], "key");
    }
}
