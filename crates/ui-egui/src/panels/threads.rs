//! Roidery: the Threads panel. Choose the design's thread chart (Brother, Janome, or one imported
//! from a GIMP palette or CSV) and fabric, see each thread colour the design sews with the chart
//! thread it is (or is nearest to), click one to select what sews in it, and Match to Chart to
//! change every colour to its nearest chart thread. Reads `stitch.design`; acts through
//! `stitch.setDesign`, `stitch.matchThreads` and `stitch.importChart`.

use egui::{CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::panels::sequencer::{rgb, thread_label};
use crate::theme::Tokens;
use crate::{FilePick, VectorcraftApp, widgets};

pub const ID: &str = "threads";
const ROW_H: f32 = 26.0;
const LABEL_W: f32 = 84.0;

/// Chart files the import reads.
pub const CHART_EXTS: &[&str] = &["gpl", "csv", "txt"];

/// `stitch.design`, recomputed only when the document changes.
pub(crate) fn design(app: &mut VectorcraftApp, ctx: &egui::Context) -> Option<Value> {
    type Key = (u64, u64);
    let st = app.session.active()?;
    let key: Key = (st.uid, st.revision);
    let id = egui::Id::new("threads-design");
    if let Some((k, v)) = ctx.data(|d| d.get_temp::<(Key, Value)>(id))
        && k == key
    {
        return Some(v);
    }
    let v = app.session.execute("stitch.design", &json!({})).ok()?;
    ctx.data_mut(|d| d.insert_temp(id, (key, v.clone())));
    Some(v)
}

/// A fabric's label in the interface language.
pub(crate) fn fabric_label(id: &str) -> &'static str {
    match id {
        "knit" => tl!("Knit (T-Shirt, Jersey)"),
        "pique" => tl!("Piqué (Polo Shirt)"),
        "terry" => tl!("Terry (Towel)"),
        "fleece" => tl!("Fleece"),
        "denim" => tl!("Denim, Canvas"),
        "twill" => tl!("Twill (Cap)"),
        "leather" => tl!("Leather, Vinyl"),
        _ => tl!("Woven (Cotton, Poplin)"),
    }
}

fn report(app: &mut VectorcraftApp, r: Result<Value, String>) {
    if let Err(e) = r {
        app.status(e);
    }
}

/// The Fabric row: a dropdown of the fabric presets. Shared with the Stitch Settings panel.
pub(crate) fn fabric_row(app: &mut VectorcraftApp, ui: &mut Ui, q: &Value, label_w: f32) {
    let ids: Vec<String> = q["fabrics"].as_array().into_iter().flatten().filter_map(|f| f["id"].as_str().map(str::to_string)).collect();
    let labels: Vec<&str> = ids.iter().map(|i| fabric_label(i)).collect();
    let current = fabric_label(q["fabric"].as_str().unwrap_or("woven"));
    let mut chosen = None;
    widgets::label_row(ui, tl!("Fabric"), label_w, |ui| {
        chosen = widgets::dropdown(ui, "embroidery-fabric", current, &labels, ui.available_width().min(170.0));
    });
    if let Some(id) = chosen.and_then(|i| ids.get(i)) {
        let r = app.run("stitch.setDesign", json!({ "fabric": id }));
        report(app, r);
    }
}

/// A hoop brand or name in the interface language (brands and the makers' own hoop names stay
/// as they are).
fn hoop_text(s: &str) -> String {
    match s {
        "Commercial" => tl!("Commercial").to_string(),
        "Custom" => tl!("Custom").to_string(),
        "Cap frame" => tl!("Cap frame").to_string(),
        _ => match s.strip_suffix(" cm round") {
            Some(n) => tl!("{n} cm round").replace("{n}", n),
            None => s.to_string(),
        },
    }
}

/// The Machine and Hoop rows (the hoop library, by brand) and how the design fits the hoop.
/// `fit` is `stitch.info`'s `hoop` (with `fit`), when the panel has it. Shared with the Stitch
/// Settings panel.
pub(crate) fn hoop_rows(app: &mut VectorcraftApp, ui: &mut Ui, q: &Value, fit: Option<&Value>, label_w: f32) {
    let hoops: Vec<&Value> = q["hoops"].as_array().into_iter().flatten().collect();
    let current = &q["hoop"];
    let brand = current["brand"].as_str().unwrap_or("");
    // Brands in library order, then Custom when the design has a custom hoop.
    let mut brands: Vec<&str> = vec![];
    for h in &hoops {
        if let Some(b) = h["brand"].as_str()
            && !brands.contains(&b)
        {
            brands.push(b);
        }
    }
    if !brand.is_empty() && !brands.contains(&brand) {
        brands.push(brand);
    }
    let mut labels: Vec<String> = vec![tl!("None").to_string()];
    labels.extend(brands.iter().map(|b| hoop_text(b)));
    let shown = if brand.is_empty() { tl!("None").to_string() } else { hoop_text(brand) };
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut chosen = None;
    widgets::label_row(ui, tl!("Machine"), label_w, |ui| {
        chosen = widgets::dropdown_names(ui, "embroidery-hoop-brand", &shown, &refs, ui.available_width().min(170.0));
    });
    match chosen {
        Some(0) => {
            let r = app.run("stitch.setDesign", json!({ "hoop": "none" }));
            report(app, r);
            return;
        }
        // A brand picks its smallest hoop.
        Some(i) => {
            if let Some(id) = brands.get(i - 1).and_then(|b| hoops.iter().find(|h| h["brand"] == *b)).and_then(|h| h["id"].as_str()) {
                let r = app.run("stitch.setDesign", json!({ "hoop": id }));
                report(app, r);
                return;
            }
        }
        None => {}
    }
    if brand.is_empty() {
        return;
    }
    let mm = |h: &Value| format!("{} × {} mm", h["widthMm"].as_f64().unwrap_or(0.0), h["heightMm"].as_f64().unwrap_or(0.0));
    let label = |h: &Value| {
        let name = hoop_text(h["name"].as_str().unwrap_or(""));
        if name.ends_with(" mm") { name } else { format!("{name} ({})", mm(h)) }
    };
    let mine: Vec<&&Value> = hoops.iter().filter(|h| h["brand"] == brand).collect();
    let names: Vec<String> = mine.iter().map(|h| label(h)).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut chosen = None;
    widgets::label_row(ui, tl!("Hoop"), label_w, |ui| {
        chosen = widgets::dropdown_names(ui, "embroidery-hoop", &label(current), &refs, ui.available_width().min(170.0));
    });
    if let Some(id) = chosen.and_then(|i| mine.get(i)).and_then(|h| h["id"].as_str()) {
        let r = app.run("stitch.setDesign", json!({ "hoop": id }));
        report(app, r);
    }
    let Some(fit) = fit else { return };
    let t = Tokens::get(ui.ctx());
    let (text, color) = match fit["fit"].as_str() {
        Some("fits") => (tl!("Fits the hoop").to_string(), t.text_dim),
        Some("rotate") => (tl!("Fits turned 90°").to_string(), t.warning),
        Some("tooLarge") => {
            let over = |k: &str| fit[k].as_f64().filter(|v| *v > 0.05);
            let text = match (over("overWidthMm"), over("overHeightMm")) {
                (Some(w), Some(h)) if fit["round"] != json!(true) => {
                    tl!("{w} mm too wide, {h} mm too tall").replace("{w}", &format!("{w:.1}")).replace("{h}", &format!("{h:.1}"))
                }
                (Some(w), None) => tl!("{w} mm too wide").replace("{w}", &format!("{w:.1}")),
                (None, Some(h)) => tl!("{h} mm too tall").replace("{h}", &format!("{h:.1}")),
                _ => tl!("Too large for the hoop").to_string(),
            };
            (text, t.error)
        }
        _ => return,
    };
    widgets::label_row(ui, "", label_w, |ui| {
        ui.label(egui::RichText::new(text).size(12.0).color(color));
    });
}

/// The Thread Chart row: None, the built-in charts, the design's own imported chart, Import….
pub(crate) fn chart_row(app: &mut VectorcraftApp, ui: &mut Ui, q: &Value) {
    let current = q["chart"]["name"].as_str().unwrap_or(tl!("None")).to_string();
    // (shown name, stitch.setDesign chart value; Null = import)
    let mut options: Vec<(String, Value)> = vec![(tl!("None").to_string(), json!("none"))];
    for c in q["charts"].as_array().into_iter().flatten() {
        if let (Some(id), Some(name)) = (c["id"].as_str(), c["name"].as_str()) {
            options.push((name.to_string(), json!(id)));
        }
    }
    let imported = !options.iter().any(|o| o.0 == current);
    if imported {
        options.push((current.clone(), Value::Bool(false)));
    }
    options.push((tl!("Import Chart…").to_string(), Value::Null));
    let names: Vec<&str> = options.iter().map(|o| o.0.as_str()).collect();
    let mut chosen = None;
    widgets::label_row(ui, tl!("Thread chart"), LABEL_W, |ui| {
        chosen = widgets::dropdown_names(ui, "thread-chart", &current, &names, ui.available_width().min(170.0));
    });
    match chosen.and_then(|i| options.get(i)).map(|o| o.1.clone()) {
        Some(Value::Null) => {
            let r = import_chart(app, &json!({}));
            if r.as_ref().err().is_none_or(|e| e != "cancelled") {
                report(app, r);
            }
        }
        Some(Value::Bool(_)) | None => {}
        Some(v) => {
            let r = app.run("stitch.setDesign", json!({ "chart": v }));
            report(app, r);
        }
    }
}

/// `stitch.importChart`: read a thread chart file (`path`, else a picked one) into the design.
pub fn import_chart(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    let path = match p.get("path").and_then(Value::as_str) {
        Some(path) => path.to_string(),
        None if crate::io::is_web(app) => return Err(tl!("Importing a thread chart needs the desktop app for now.").into()),
        None => crate::io::pick_open(app, &FilePick { filters: vec![(tl!("Thread Charts"), CHART_EXTS)], ..Default::default() })?,
    };
    let r = app.run("stitch.setDesign", json!({ "chart": { "path": path } }))?;
    let name = r["chart"].as_str().unwrap_or_default().to_string();
    app.status(tl!("Thread chart: {name}").replace("{name}", &name));
    Ok(r)
}

/// Match to Chart: every object the design sews, whatever is selected.
pub(crate) fn match_threads(app: &mut VectorcraftApp) {
    let ids: Vec<u64> = app.session.execute("stitch.design", &json!({})).ok().map_or_else(Vec::new, |q| {
        q["threads"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|t| t["objects"].as_array().cloned().unwrap_or_default())
            .filter_map(|v| v.as_u64())
            .collect()
    });
    match app.run("stitch.matchThreads", json!({ "ids": ids })) {
        Ok(r) => {
            let n = r["changed"].as_u64().unwrap_or(0);
            app.status(tl!("Matched {n} objects to the thread chart").replace("{n}", &n.to_string()));
        }
        Err(e) => app.status(e),
    }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(q) = design(app, ui.ctx()) else {
        widgets::dim_label(ui, tl!("No document"));
        return;
    };
    chart_row(app, ui, &q);
    fabric_row(app, ui, &q, LABEL_W);
    widgets::divider(ui);
    let threads = q["threads"].as_array().cloned().unwrap_or_default();
    let has_chart = !q["chart"].is_null();
    if threads.is_empty() {
        widgets::dim_label(ui, tl!("Nothing sews yet: draw paths, or use the Satin Blocks tool."));
        return;
    }
    let selected: Vec<u64> = app.session.active().map(|st| st.selection.objects.iter().map(|i| i.0).collect()).unwrap_or_default();
    let mut clicked: Option<(Vec<u64>, bool)> = None;
    egui::ScrollArea::vertical().id_salt("threads-rows").max_height((ui.available_height() - 40.0).max(ROW_H * 3.0)).auto_shrink([false, true]).show(
        ui,
        |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for th in &threads {
                let ids: Vec<u64> = th["objects"].as_array().into_iter().flatten().filter_map(Value::as_u64).collect();
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                let p = ui.painter();
                if !ids.is_empty() && ids.iter().all(|i| selected.contains(i)) {
                    p.rect_filled(rect, 0.0, t.selection);
                } else if resp.hovered() {
                    p.rect_filled(rect, 0.0, t.hover);
                }
                let color = rgb(&th["rgb"]);
                let chip = |p: &egui::Painter, x: f32, c: egui::Color32| {
                    let r = Rect::from_center_size(pos2(x, rect.center().y), vec2(14.0, 14.0));
                    p.rect_filled(r, CornerRadius::same(3), c);
                    p.rect_stroke(r, CornerRadius::same(3), Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
                };
                chip(p, rect.left() + 14.0, color);
                // Right: the nearest chart thread when this colour isn't one.
                let mut right = rect.right() - 8.0;
                let nearest = &th["nearest"];
                if has_chart && th["inChart"] != json!(true) && !nearest.is_null() {
                    let text = format!("→ {}", thread_label(nearest, rgb(&nearest["rgb"])));
                    let g = p.layout_no_wrap(text, egui::FontId::proportional(11.5), t.text_dim);
                    let x = right - g.size().x;
                    p.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, t.text_dim);
                    chip(p, x - 12.0, rgb(&nearest["rgb"]));
                    right = x - 24.0;
                }
                let label = thread_label(th, color);
                let mut job = egui::text::LayoutJob::simple_singleline(label, egui::FontId::proportional(12.5), t.text);
                job.wrap = egui::text::TextWrapping::truncate_at_width((right - rect.left() - 30.0).max(20.0));
                let g = p.layout_job(job);
                p.galley_with_override_text_color(pos2(rect.left() + 28.0, rect.center().y - g.size().y / 2.0), g, t.text);
                let resp = if has_chart && th["inChart"] != json!(true) && !nearest.is_null() {
                    let tip = tl!("Not in chart. Nearest: {thread} (difference {d})")
                        .replace("{thread}", &thread_label(nearest, rgb(&nearest["rgb"])))
                        .replace("{d}", &format!("{:.1}", nearest["deltaE"].as_f64().unwrap_or(0.0)));
                    resp.on_hover_text(tip)
                } else {
                    resp
                };
                if resp.clicked() {
                    clicked = Some((ids, ui.input(|i| i.modifiers.shift || i.modifiers.command)));
                }
            }
        },
    );
    if let Some((mut ids, add)) = clicked {
        if add {
            ids.extend(selected.iter().filter(|id| !ids.contains(id)).copied().collect::<Vec<_>>());
        }
        app.run("select.set", json!({ "ids": ids })).ok();
    }
    let off = threads.iter().filter(|th| th["inChart"] != json!(true)).count();
    widgets::bottom_bar(ui, |ui| {
        if widgets::icon_button_enabled(ui, "pipette", tl!("Match to Chart"), false, has_chart && off > 0, 24.0).clicked() {
            match_threads(app);
        }
        if widgets::icon_button(ui, "import", tl!("Import Chart…"), false, 24.0).clicked() {
            let r = import_chart(app, &json!({}));
            if r.as_ref().err().is_none_or(|e| e != "cancelled") {
                report(app, r);
            }
        }
        let mut total = tl!("{n} threads").replace("{n}", &threads.len().to_string());
        if has_chart && off > 0 {
            total = format!("{total} · {}", tl!("{n} not in chart").replace("{n}", &off.to_string()));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(total).size(11.5).color(t.text_dim));
        });
    });
}

/// The ≡ menu.
pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    let has_chart = design(app, ui.ctx()).is_some_and(|q| !q["chart"].is_null());
    if widgets::menu_item(ui, "Match to Chart", has_chart, false) {
        match_threads(app);
        ui.close();
    }
    if widgets::menu_item(ui, "Import Chart…", app.session.active().is_some(), false) {
        ui.close();
        let r = import_chart(app, &json!({}));
        if r.as_ref().err().is_none_or(|e| e != "cancelled") {
            report(app, r);
        }
    }
}

#[cfg(test)]
mod tests {
    use vectorcraft_engine::Session;

    use super::*;

    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
        for (x, c) in [(0, "#fe0101"), (80, "#000000")] {
            let id = app.session.execute("shape.rectangle", &json!({"x": x, "y": 10, "width": 72, "height": 72})).unwrap()["id"].as_u64().unwrap();
            app.session.execute("paint.setFill", &json!({"color": c, "ids": [id]})).unwrap();
        }
        app
    }

    fn frame(app: &mut VectorcraftApp) -> Vec<String> {
        fn texts(s: &egui::Shape, out: &mut Vec<String>) {
            match s {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| show(app, ui));
        out.textures_delta.clear();
        let mut v = vec![];
        out.shapes.iter().for_each(|c| texts(&c.shape, &mut v));
        v
    }

    #[test]
    fn lists_the_design_threads_against_the_chart() {
        let mut app = app();
        let shown = frame(&mut app);
        assert!(shown.iter().any(|s| s == "#FE0101"), "{shown:?}");
        assert!(shown.iter().any(|s| s == "2 threads"), "{shown:?}");
        app.run("stitch.setDesign", json!({"chart": "janome"})).unwrap();
        let shown = frame(&mut app);
        assert!(shown.iter().any(|s| s == "Black 002"), "{shown:?}");
        assert!(shown.iter().any(|s| s == "#FE0101"), "{shown:?}");
        assert!(shown.iter().any(|s| s.starts_with("→ ")), "{shown:?}");
        match_threads(&mut app);
        assert!(app.ui.status.contains('1'), "{}", app.ui.status);
        let shown = frame(&mut app);
        assert!(!shown.iter().any(|s| s.starts_with("→ ")), "{shown:?}");
    }

    #[test]
    fn importing_a_chart_reads_the_file() {
        let mut app = app();
        let dir = std::env::temp_dir().join(format!("roidery-chart-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Polyneon.csv");
        std::fs::write(&path, "code,name,hex\n1800,Black,#000000\n").unwrap();
        let r = import_chart(&mut app, &json!({"path": path.to_string_lossy()})).unwrap();
        assert_eq!(r["chart"], "Polyneon");
        assert_eq!(app.ui.status, "Thread chart: Polyneon");
        let shown = frame(&mut app);
        assert!(shown.iter().any(|s| s == "Black 1800"), "{shown:?}");
        std::fs::remove_dir_all(dir).ok();
    }
}
