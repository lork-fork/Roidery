//! Image Trace panel: preset, mode, threshold or colour count, and the advanced fidelity options.
//! With an Image Trace object selected, changing a setting re-traces it (one undo step per change;
//! sliders apply when released). With an image selected, Trace makes a new Image Trace object.

use egui::Ui;
use serde_json::{Value, json};

use super::{first_selected, pstate, set_pstate};
use crate::VectorcraftApp;
use crate::widgets::{self, menu_item};

const MODES: [(&str, &str); 3] = [("blackAndWhite", "Black and White"), ("grayscale", "Grayscale"), ("color", "Color")];

/// Panel state: the preset name, the current parameters and the last result's counts.
#[derive(Clone, Default)]
struct TraceUi {
    preset: String,
    params: Value,
    info: Option<(u64, u64, u64)>,
    /// The selected Image Trace object's stored settings last adopted (so edits in progress
    /// aren't overwritten until the object changes).
    synced: Value,
}

fn presets(app: &mut VectorcraftApp) -> Vec<(String, Value)> {
    let v = app.session.execute("imageTrace.presets", &json!({})).unwrap_or_default();
    v["presets"]
        .as_array()
        .map(|a| a.iter().map(|p| (p["name"].as_str().unwrap_or("").to_string(), p["params"].clone())).collect())
        .unwrap_or_default()
}

/// What the selection is: an Image Trace object (with its stored settings), a plain image, or neither.
fn target(app: &VectorcraftApp) -> (bool, bool, Option<Value>) {
    let Some(n) = first_selected(app) else { return (false, false, None) };
    let trace = n.name.as_deref() == Some("Image Trace")
        && n.children().is_some_and(|c| c.first().is_some_and(|i| matches!(i.kind, vectorcraft_doc::NodeKind::Image(_))));
    (trace, matches!(n.kind, vectorcraft_doc::NodeKind::Image(_)), n.trace.map(|t| *t))
}

fn trace(app: &mut VectorcraftApp, st: &mut TraceUi) {
    let preset = if st.preset == "Custom" { "Default" } else { st.preset.as_str() };
    match app.run("imageTrace.make", json!({ "preset": preset, "params": st.params })) {
        Ok(r) => st.info = Some((r["paths"].as_u64().unwrap_or(0), r["anchors"].as_u64().unwrap_or(0), r["colors"].as_u64().unwrap_or(0))),
        Err(e) => app.ui.status = e,
    }
}

fn slider(ui: &mut Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>, suffix: &str) -> (bool, bool) {
    let mut out = (false, false);
    ui.horizontal(|ui| {
        ui.add_sized([74.0, 22.0], egui::Label::new(egui::RichText::new(tl!(label)).size(12.0)));
        // The theme's widget fill matches the panel, which would hide the rail.
        let t = crate::theme::Tokens::get(ui.ctx());
        ui.visuals_mut().widgets.inactive.bg_fill = t.input_border;
        ui.visuals_mut().selection.bg_fill = t.accent;
        let r = ui.add(egui::Slider::new(v, range).suffix(suffix).integer().trailing_fill(true));
        out = (r.changed(), r.drag_stopped() || (r.changed() && !r.dragged()));
    });
    out
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let all = presets(app);
    let mut st: TraceUi = pstate(ui.ctx(), "image-trace");
    if st.params.is_null() {
        st.preset = "Default".into();
        st.params = all.first().map(|p| p.1.clone()).unwrap_or_default();
    }
    let (is_trace, is_image, stored) = target(app);
    if let Some(t) = stored.filter(|t| *t != st.synced) {
        st.preset = t["preset"].as_str().unwrap_or("Custom").to_string();
        st.params = t["params"].clone();
        st.synced = t;
    }
    let mut retrace = false;

    ui.horizontal(|ui| {
        widgets::dim_label(ui, tl!("Preset:"));
        let names: Vec<&str> = all.iter().map(|p| p.0.as_str()).collect();
        if let Some(i) = widgets::dropdown(ui, "it-preset", &st.preset, &names, 170.0) {
            st.preset = all[i].0.clone();
            st.params = all[i].1.clone();
            retrace = true;
        }
    });
    ui.horizontal(|ui| {
        widgets::dim_label(ui, tl!("Mode:"));
        let mode = st.params["mode"].as_str().unwrap_or("blackAndWhite").to_string();
        let label = MODES.iter().find(|m| m.0 == mode).map_or("Black and White", |m| m.1);
        let labels: Vec<&str> = MODES.iter().map(|m| m.1).collect();
        if let Some(i) = widgets::dropdown(ui, "it-mode", label, &labels, 170.0) {
            st.params["mode"] = json!(MODES[i].0);
            st.preset = "Custom".into();
            retrace = true;
        }
    });
    let bw = st.params["mode"].as_str() == Some("blackAndWhite");
    let (key, label, range) = if bw {
        ("threshold", tl!("Threshold"), 0.0..=255.0)
    } else {
        ("colors", if st.params["mode"] == "grayscale" { tl!("Grays") } else { tl!("Colors") }, 2.0..=256.0)
    };
    let mut v = st.params[key].as_f64().unwrap_or(0.0);
    let (changed, release) = slider(ui, label, &mut v, range, "");
    if changed {
        st.params[key] = json!(v.round() as u64);
        st.preset = "Custom".into();
    }
    retrace |= release;

    let open: bool = !pstate::<bool>(ui.ctx(), "it-advanced-closed");
    if ui.add(egui::Button::new(format!("{} {}", if open { "▾" } else { "▸" }, tl!("Advanced"))).frame(false)).clicked() {
        set_pstate(ui.ctx(), "it-advanced-closed", open);
    }
    if open {
        for (key, label, range, suffix) in
            [("paths", tl!("Paths"), 0.0..=100.0, "%"), ("corners", tl!("Corners"), 0.0..=100.0, "%"), ("noise", tl!("Noise"), 1.0..=100.0, " px")]
        {
            let mut v = st.params[key].as_f64().unwrap_or(0.0);
            let (changed, release) = slider(ui, label, &mut v, range, suffix);
            if changed {
                st.params[key] = if key == "noise" { json!(v.round() as u64) } else { json!(v.round()) };
                st.preset = "Custom".into();
            }
            retrace |= release;
        }
        ui.horizontal(|ui| {
            widgets::dim_label(ui, tl!("Method:"));
            for (m, l) in [("abutting", tl!("Abutting")), ("overlapping", tl!("Overlapping"))] {
                if ui.selectable_label(st.params["method"] == m, l).clicked() && st.params["method"] != m {
                    st.params["method"] = json!(m);
                    st.preset = "Custom".into();
                    retrace = true;
                }
            }
        });
        for (key, label) in [("snapCurvesToLines", tl!("Snap Curves To Lines")), ("ignoreWhite", tl!("Ignore White"))] {
            let on = st.params[key].as_bool().unwrap_or(false);
            if widgets::check(ui, label, on, true) {
                st.params[key] = json!(!on);
                st.preset = "Custom".into();
                retrace = true;
            }
        }
    }
    widgets::divider(ui);
    if let Some((p, a, c)) = st.info {
        widgets::dim_label(
            ui,
            &crate::i18n::fmt(
                tl!("Paths: {paths}    Anchors: {anchors}    Colors: {colors}"),
                &[("paths", &p.to_string()), ("anchors", &a.to_string()), ("colors", &c.to_string())],
            ),
        );
    }
    ui.horizontal(|ui| {
        let r = ui.add_enabled_ui(is_trace || is_image, |ui| widgets::flat_button(ui, tl!("Trace"), 80.0)).inner;
        if r.on_disabled_hover_text(tl!("Select an image to trace")).clicked() {
            trace(app, &mut st);
        }
        if ui.add_enabled_ui(is_trace, |ui| widgets::flat_button(ui, tl!("Expand"), 80.0)).inner.clicked() {
            app.run("imageTrace.expand", json!({})).ok();
        }
    });
    if retrace && is_trace {
        trace(app, &mut st);
    }
    set_pstate(ui.ctx(), "image-trace", st);
}

pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    let (is_trace, _, _) = target(app);
    if menu_item(ui, tl!("Release"), is_trace, false) {
        app.run("imageTrace.release", json!({})).ok();
    }
    if menu_item(ui, tl!("Expand"), is_trace, false) {
        app.run("imageTrace.expand", json!({})).ok();
    }
    ui.separator();
    if menu_item(ui, tl!("Reset to Default"), true, false) {
        set_pstate(ui.ctx(), "image-trace", TraceUi::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_and_menu_draw_headless() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 100, "height": 100})).unwrap();
        app.session.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
        for _ in 0..2 {
            let ctx = egui::Context::default();
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                show(&mut app, ui);
                menu(&mut app, ui);
            });
            out.textures_delta.clear();
        }
    }
}
