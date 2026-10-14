//! Roidery: Export Mockup. Choose the garment (T-shirt, cap, tote bag or a swatch of fabric),
//! where the design goes on a T-shirt, the garment's colour and its fabric, with a preview of the
//! result; Export… picks a file and writes the picture (`file.exportMockup` → `stitch.mockup`).
//!
//! Fields: `garment`, `placement`, `color` (`#rrggbb`), `fabric` (empty: the design's).

use serde_json::{Value, json};

use super::swatch_options::{grid, label};
use super::{DialogSpec, run_and_close};
use crate::state::Dialog;
use crate::{VectorcraftApp, widgets};

/// The dialog kind of Export Mockup.
pub const KIND: &str = "exportMockup";

pub(super) const SPEC: DialogSpec = DialogSpec {
    heading: |_| tl!("Export Mockup").into(),
    body,
    confirm,
    ok: Some("Export…"),
    min_width: 420.0,
    max_width: Some(440.0),
    ..DialogSpec::FORM
};

/// (id, label, the garment's usual colour, preview pixels per mm).
fn garments() -> [(&'static str, &'static str, &'static str, f64); 4] {
    [
        ("tshirt", tl!("T-Shirt"), "#f4f3ef", 0.5),
        ("cap", tl!("Baseball Cap"), "#232e4a", 1.2),
        ("tote", tl!("Tote Bag"), "#eae1cb", 0.7),
        ("swatch", tl!("Fabric Swatch"), "#f4f3ef", 3.0),
    ]
}

fn placements() -> [(&'static str, &'static str); 2] {
    [("center", tl!("Center Chest")), ("leftChest", tl!("Left Chest"))]
}

/// Garment colours to choose from.
fn colors() -> [(&'static str, &'static str); 9] {
    [
        ("#f4f3ef", tl!("White")),
        ("#eae1cb", tl!("Natural")),
        ("#b9bcbf", tl!("Heather Grey")),
        ("#1d1d1f", tl!("Black")),
        ("#232e4a", tl!("Navy")),
        ("#b3122a", tl!("Red")),
        ("#2347a8", tl!("Royal Blue")),
        ("#24553a", tl!("Forest Green")),
        ("#f2b8c6", tl!("Pink")),
    ]
}

/// Open Export Mockup on a white T-shirt with the design centred on the chest.
pub fn open(app: &mut VectorcraftApp) -> Result<Value, String> {
    app.session.active().ok_or("no document open")?;
    app.ui.dialog = Some(Dialog::new(KIND, json!({"garment": "tshirt", "placement": "center", "color": "#f4f3ef", "fabric": ""})));
    Ok(Value::Null)
}

/// The `stitch.mockup` params the fields ask for.
fn params(d: &Dialog) -> Value {
    let mut p = json!({"garment": d.str("garment"), "color": d.str("color")});
    if d.str("garment") == "tshirt" {
        p["placement"] = json!(d.str("placement"));
    }
    if !d.str("fabric").is_empty() {
        p["fabric"] = json!(d.str("fabric"));
    }
    p
}

fn dropdown(ui: &mut egui::Ui, d: &mut Dialog, key: &str, label_text: &str, options: &[(&str, &str)]) -> bool {
    label(ui, label_text);
    let current = d.str(key);
    let shown = options.iter().find(|o| o.0 == current).map_or(current.as_str(), |o| o.1);
    let names: Vec<&str> = options.iter().map(|o| o.1).collect();
    let picked = widgets::dropdown_names(ui, ("mockup", key), shown, &names, 220.0).and_then(|i| options.get(i));
    ui.end_row();
    if let Some((id, _)) = picked {
        d.fields.insert(key.into(), json!(id));
        return true;
    }
    false
}

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    let garments = garments();
    let gopts: Vec<(&str, &str)> = garments.iter().map(|g| (g.0, g.1)).collect();
    let fabrics: Vec<(String, &'static str)> = app
        .session
        .execute("stitch.design", &json!({}))
        .ok()
        .and_then(|q| q["fabrics"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|f| f["id"].as_str().map(|id| (id.to_string(), crate::panels::threads::fabric_label(id))))
        .collect();
    let mut fopts: Vec<(&str, &str)> = vec![("", tl!("As the Design"))];
    fopts.extend(fabrics.iter().map(|(id, l)| (id.as_str(), *l)));
    grid(ui, |ui| {
        if dropdown(ui, d, "garment", tl!("Garment:"), &gopts)
            && let Some(g) = garments.iter().find(|g| g.0 == d.str("garment"))
        {
            d.fields.insert("color".into(), json!(g.2));
        }
        if d.str("garment") == "tshirt" {
            dropdown(ui, d, "placement", tl!("Placement:"), &placements());
        }
        dropdown(ui, d, "color", tl!("Color:"), &colors());
        dropdown(ui, d, "fabric", tl!("Fabric:"), &fopts);
    });
    ui.add_space(8.0);
    preview(app, ui, d);
    false
}

/// A small picture of the mockup, made again when the document or a field changes.
fn preview(app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &Dialog) {
    let Some(st) = app.session.active() else { return };
    let mut p = params(d);
    let ppm = garments().iter().find(|g| g.0 == d.str("garment")).map_or(1.0, |g| g.3);
    p["ppm"] = json!(ppm);
    let key = format!("{}-{}-{p}", st.uid, st.revision);
    let id = egui::Id::new("export-mockup-preview");
    type Cached = (String, Result<egui::TextureHandle, String>);
    let cached = ui.ctx().data(|m| m.get_temp::<Cached>(id)).filter(|c| c.0 == key);
    let made = match cached {
        Some((_, r)) => r,
        None => {
            let r = app.session.execute("stitch.mockup", &p).map_err(|e| e.to_string()).and_then(|r| {
                let bytes = r["dataBase64"].as_str().and_then(vectorcraft_format::base64_decode).ok_or("no picture")?;
                let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?.to_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                Ok(ui.ctx().load_texture("export-mockup", egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), egui::TextureOptions::LINEAR))
            });
            ui.ctx().data_mut(|m| m.insert_temp(id, (key, r.clone())));
            r
        }
    };
    match made {
        Ok(tex) => {
            let s = tex.size_vec2();
            let k = (380.0 / s.x).min(320.0 / s.y);
            ui.vertical_centered(|ui| ui.image((tex.id(), s * k)));
        }
        Err(e) => {
            widgets::dim_label(ui, &e);
        }
    }
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> super::DialogResult {
    run_and_close(app, "file.exportMockup", params(d))
}
