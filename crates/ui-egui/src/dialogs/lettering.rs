//! Roidery: Add Lettering (Type › Add Lettering…, Embroidery › Add Lettering…). Two kinds of
//! font, on two tabs, each saying what it is:
//!
//! - **Embroidery Fonts**: fonts digitized for stitching (Roidery Sans and the embroidery font
//!   library, `stitch.fonts`), each shown with its sample, the stitches it uses and the sizes it
//!   sews well at. Adds a lettering object (`stitch.createLettering`) that stays editable text.
//! - **Computer Fonts**: any font installed on the computer, as a text object whose letters are
//!   filled with stitches (`text.create`).
//!
//! The Stitch Settings panel's Change… opens it on the Embroidery Fonts tab to change selected
//! lettering's font (`stitch.setLettering`).
//!
//! Fields: `tab` (`embroidery`|`computer`), `text`, `font` (embroidery font id), `family`
//! (computer font), `height` (mm cap height), `change` (with `ids`: change those letterings'
//! font).

use egui::{Color32, CornerRadius, Sense, Stroke, Ui, vec2};
use serde_json::{Value, json};

use super::{DialogResult, DialogSpec};
use crate::state::Dialog;
use crate::theme::Tokens;
use crate::{VectorcraftApp, widgets};

pub const KIND: &str = "lettering";

pub(super) const SPEC: DialogSpec = DialogSpec {
    heading: |d| if d.bool("change") { tl!("Change Lettering Font").into() } else { tl!("Add Lettering").into() },
    body,
    confirm,
    ok: Some("Add"),
    ok_label: Some(|app| if app.ui.dialog.as_ref().is_some_and(|d| d.bool("change")) { tl!("Change Font") } else { tl!("Add") }),
    min_width: 520.0,
    max_width: Some(560.0),
    ..DialogSpec::FORM
};

/// Points per mm.
const PT_PER_MM: f64 = 72.0 / 25.4;
/// A computer font's cap height as a share of its size (most Latin fonts: 0.66–0.72).
const CAP_SHARE: f64 = 0.7;
const ROW_H: f32 = 72.0;

/// Open Add Lettering (`type.addLettering`); `{tab?, text?, font?, height?}` start it so.
pub fn open(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    app.session.active().ok_or("no document open")?;
    let mut fields = json!({"tab": "embroidery", "text": tl!("Lettering"), "font": "sans", "family": "Source Sans 3", "height": 10.0});
    for key in ["tab", "text", "font", "family", "height"] {
        if let Some(v) = p.get(key) {
            fields[key] = v.clone();
        }
    }
    app.ui.dialog = Some(Dialog::new(KIND, fields));
    Ok(Value::Null)
}

/// Open it to change the font of lettering objects `ids` (now in `font`, `height` mm tall).
pub fn open_change(app: &mut VectorcraftApp, ids: &[u64], font: &str, height: f64) {
    app.ui.dialog = Some(Dialog::new(KIND, json!({"tab": "embroidery", "change": true, "ids": ids, "font": font, "height": height})));
}

/// The lettering fonts (`stitch.fonts`), kept until Look Again.
fn fonts(app: &mut VectorcraftApp, ui: &Ui, rescan: bool) -> Vec<Value> {
    let id = egui::Id::new("lettering-fonts");
    if !rescan && let Some(v) = ui.ctx().data(|m| m.get_temp::<std::sync::Arc<Vec<Value>>>(id)) {
        return (*v).clone();
    }
    let r = app.session.execute("stitch.fonts", &json!({"rescan": rescan})).unwrap_or_default();
    let list = r["fonts"].as_array().cloned().unwrap_or_default();
    ui.ctx().data_mut(|m| {
        m.insert_temp(id, std::sync::Arc::new(list.clone()));
        m.insert_temp(id.with("folder"), r["userFolder"].as_str().unwrap_or_default().to_string());
    });
    list
}

/// What a font sews with, as short labels.
fn badges(f: &Value) -> Vec<&'static str> {
    let mut v = vec![];
    if f["digitized"] == json!(false) {
        v.push(tl!("Built-in"));
    }
    if f["user"] == json!(true) {
        v.push(tl!("Yours"));
    }
    for (key, label) in [("satin", tl!("Satin")), ("fill", tl!("Fill")), ("run", tl!("Run")), ("multicolor", tl!("Multicolor"))] {
        if f[key] == json!(true) {
            v.push(label);
        }
    }
    v
}

/// "Sews well at 8–30 mm".
pub(crate) fn size_hint(lo: f64, hi: f64) -> String {
    let mm = |v: f64| format!("{:.0}", v.max(0.0));
    if (hi - lo).abs() < 0.5 {
        tl!("Sews well at about {size} mm").replace("{size}", &mm(lo))
    } else {
        tl!("Sews well at {min}–{max} mm").replace("{min}", &mm(lo)).replace("{max}", &mm(hi))
    }
}

/// Whether `h` mm is in the size range `f` sews well at (true when unknown), with a little slack.
fn fits(f: &Value, h: f64) -> bool {
    match (f["sizeMin"].as_f64(), f["sizeMax"].as_f64()) {
        (Some(lo), Some(hi)) => h >= lo * 0.95 - 0.05 && h <= hi * 1.05 + 0.05,
        _ => true,
    }
}

fn badge(ui: &mut Ui, text: &str, t: &Tokens) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(10.5), t.text);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(10.0, 4.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(3), t.panel_darker, Stroke::new(1.0, t.input_border), egui::StrokeKind::Inside);
    ui.painter().galley(rect.min + vec2(5.0, 2.0), galley, t.text);
}

/// The font's sample picture (`preview.png`), loaded once.
fn preview(ui: &Ui, path: &str) -> Option<egui::TextureHandle> {
    let id = egui::Id::new(("lettering-preview", path));
    if let Some(t) = ui.ctx().data(|m| m.get_temp::<Option<egui::TextureHandle>>(id)) {
        return t;
    }
    let tex = std::fs::read(path).ok().and_then(|b| image::load_from_memory(&b).ok()).map(|img| {
        // Big samples are scaled down: the rows show them a few dozen pixels tall.
        let img =
            if img.height() > 160 { img.resize(img.width() * 160 / img.height().max(1), 160, image::imageops::FilterType::Triangle) } else { img };
        let img = img.to_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        ui.ctx().load_texture(format!("lettering-{path}"), egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), egui::TextureOptions::LINEAR)
    });
    ui.ctx().data_mut(|m| m.insert_temp(id, tex.clone()));
    tex
}

/// One font's row: its sample (or its name, large), its name, what it sews with and its sizes.
/// Returns clicked.
fn font_row(ui: &mut Ui, f: &Value, selected: bool, t: &Tokens) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
    let fill = if selected {
        t.row_selected
    } else if resp.hovered() {
        t.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
    let name = f["name"].as_str().unwrap_or_default();
    let sample = egui::Rect::from_min_size(rect.min + vec2(8.0, 6.0), vec2(w * 0.5 - 12.0, ROW_H - 12.0));
    match f["preview"].as_str().and_then(|p| preview(ui, p)) {
        Some(tex) => {
            let s = tex.size_vec2();
            let k = (sample.width() / s.x).min(sample.height() / s.y);
            let r = egui::Rect::from_center_size(sample.left_center() + vec2(s.x * k / 2.0, 0.0), s * k);
            // Samples are dark letters on white: a light card keeps them readable in dark themes.
            ui.painter().rect_filled(r.expand(2.0), CornerRadius::same(2), Color32::from_gray(250));
            ui.painter().image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            let galley = ui.painter().layout(name.to_string(), egui::FontId::proportional(22.0), t.text_strong, sample.width());
            ui.painter().galley(egui::pos2(sample.left(), sample.center().y - galley.size().y / 2.0), galley, t.text_strong);
        }
    }
    let right = egui::Rect::from_min_max(egui::pos2(rect.left() + w * 0.5 + 4.0, rect.top() + 6.0), rect.right_bottom() - vec2(8.0, 4.0));
    let mut col = ui.new_child(egui::UiBuilder::new().max_rect(right).layout(egui::Layout::top_down(egui::Align::Min)));
    col.spacing_mut().item_spacing.y = 3.0;
    col.label(egui::RichText::new(name).size(13.0).color(t.text_strong));
    col.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 2.0);
        for b in badges(f) {
            badge(ui, b, t);
        }
    });
    if let (Some(lo), Some(hi)) = (f["sizeMin"].as_f64(), f["sizeMax"].as_f64()) {
        col.label(egui::RichText::new(size_hint(lo, hi)).size(11.0).color(t.text_dim));
    }
    let r = resp.on_hover_text(f["description"].as_str().unwrap_or_default());
    r.clicked()
}

fn matches(f: &Value, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let q = query.to_lowercase();
    let has = |k: &str| f[k].as_str().is_some_and(|s| s.to_lowercase().contains(&q));
    has("name")
        || has("description")
        || f["keywords"].as_array().is_some_and(|a| a.iter().any(|k| k.as_str().is_some_and(|s| s.to_lowercase().contains(&q))))
}

fn text_box(ui: &mut Ui, d: &mut Dialog, t: &Tokens) {
    let mut text = d.str("text");
    egui::Frame::NONE
        .fill(t.input)
        .stroke(Stroke::new(1.0, t.input_border))
        .corner_radius(CornerRadius::same(2))
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            let resp = ui.add(
                egui::TextEdit::multiline(&mut text)
                    .id(egui::Id::new("lettering-text"))
                    .frame(egui::Frame::NONE)
                    .desired_rows(2)
                    .desired_width(ui.available_width())
                    .font(egui::FontId::proportional(14.0))
                    .text_color(t.text_strong),
            );
            widgets::take_dialog_focus(ui, resp.id, &text);
        });
    d.fields.insert("text".into(), json!(text));
}

fn height_row(ui: &mut Ui, d: &mut Dialog, warn: Option<String>, t: &Tokens) {
    let h = d.f64("height", 10.0);
    let mut edit = None;
    widgets::label_row(ui, tl!("Letter height"), 110.0, |ui| {
        edit = widgets::plain_field(ui, "lettering-height", h, " mm", 1, 80.0);
        if let Some(w) = &warn {
            ui.label(egui::RichText::new(w).size(11.5).color(t.warning));
        }
    });
    if let Some(v) = edit.filter(|v| v.is_finite() && *v > 0.0) {
        d.fields.insert("height".into(), json!(v.clamp(2.0, 300.0)));
    }
}

fn body(app: &mut VectorcraftApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    let t = Tokens::get(ui.ctx());
    let change = d.bool("change");
    if !change {
        let tabs = [tl!("Embroidery Fonts"), tl!("Computer Fonts")];
        let current = usize::from(d.str("tab") == "computer");
        if let Some(i) = widgets::tab_bar(ui, &tabs, current) {
            d.fields.insert("tab".into(), json!(if i == 1 { "computer" } else { "embroidery" }));
        }
        ui.add_space(4.0);
    }
    let computer = !change && d.str("tab") == "computer";
    widgets::dim_label(
        ui,
        if computer {
            tl!("Any font installed on this computer. Roidery fills each letter's outline with stitches, which suits large, bold letters best.")
        } else {
            tl!("Fonts digitized for stitching, letter by letter. They sew cleanly at the sizes shown and stay editable as text.")
        },
    );
    ui.add_space(8.0);
    if !change {
        text_box(ui, d, &t);
        ui.add_space(8.0);
    }
    if computer {
        let family = d.str("family");
        let mut pick = None;
        widgets::label_row(ui, tl!("Font"), 110.0, |ui| {
            pick = crate::font_menu::font_menu(ui, "lettering-family", &family, 260.0, None, crate::font_menu::MenuLook::of(app));
        });
        if let Some((f, _)) = crate::font_menu::picked(app, pick) {
            d.fields.insert("family".into(), json!(f));
        }
        height_row(ui, d, None, &t);
        return false;
    }
    let mut rescan = false;
    let list = fonts(app, ui, false);
    let query = widgets::search_field(ui, egui::Id::new("lettering-search"), tl!("Search fonts"));
    ui.add_space(4.0);
    let shown: Vec<&Value> = list.iter().filter(|f| matches(f, &query)).collect();
    let current = d.str("font");
    let mut chosen: Option<&Value> = None;
    // When the list first shows, the chosen font is in view.
    let mut area = egui::ScrollArea::vertical().max_height(300.0).auto_shrink([false, false]);
    if !d.bool("scrolled") {
        d.fields.insert("scrolled".into(), json!(true));
        if let Some(i) = shown.iter().position(|f| f["id"].as_str() == Some(current.as_str())) {
            let row = ROW_H + ui.spacing().item_spacing.y;
            area = area.vertical_scroll_offset((i as f32 - 1.0).max(0.0) * row);
        }
    }
    widgets::list_box(ui, |ui| {
        area.show_rows(ui, ROW_H, shown.len(), |ui, range| {
            for f in shown.get(range).unwrap_or_default() {
                if font_row(ui, f, f["id"].as_str() == Some(current.as_str()), &t) {
                    chosen = Some(f);
                }
            }
        });
    });
    if let Some(f) = chosen {
        d.fields.insert("font".into(), f["id"].clone());
        // A size the new font doesn't sew well at: the size it was digitized at.
        if !fits(f, d.f64("height", 10.0))
            && let Some(size) = f["size"].as_f64().or_else(|| f["sizeMin"].as_f64())
        {
            d.fields.insert("height".into(), json!((size * 10.0).round() / 10.0));
        }
    }
    ui.add_space(6.0);
    let font = list.iter().find(|f| f["id"].as_str() == Some(d.str("font").as_str()));
    let warn = font.filter(|f| !fits(f, d.f64("height", 10.0))).and_then(|f| {
        Some(
            tl!("This font sews best at {min}–{max} mm")
                .replace("{min}", &format!("{:.0}", f["sizeMin"].as_f64()?))
                .replace("{max}", &format!("{:.0}", f["sizeMax"].as_f64()?)),
        )
    });
    height_row(ui, d, warn, &t);
    ui.add_space(6.0);
    let folder: String = ui.ctx().data(|m| m.get_temp(egui::Id::new("lettering-fonts").with("folder"))).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        if !folder.is_empty() {
            widgets::dim_label(ui, &tl!("Add Ink/Stitch fonts of your own to {folder}").replace("{folder}", &folder));
        }
        if widgets::link_label(ui, tl!("Look Again")).clicked() {
            rescan = true;
        }
    });
    if rescan {
        fonts(app, ui, true);
    }
    false
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> DialogResult {
    let height = d.f64("height", 10.0);
    let r = if d.bool("change") {
        app.run("stitch.setLettering", json!({"font": d.str("font"), "height": height, "ids": d.fields.get("ids").cloned().unwrap_or(json!([]))}))
    } else if d.str("text").trim().is_empty() {
        return Err(tl!("Type the text to add.").into());
    } else if d.str("tab") == "computer" {
        add_text(app, d, height)
    } else {
        app.run("stitch.createLettering", json!({"text": d.str("text"), "font": d.str("font"), "height": height}))
    };
    let r = r?;
    if let Some(m) = r["missing"].as_str().filter(|m| !m.is_empty()) {
        app.status(tl!("The lettering font has no {chars}").replace("{chars}", m));
    }
    app.ui.dialog = None;
    Ok(r)
}

/// A text object in a computer font, its letters `height` mm tall, centred on the first artboard
/// and painted in the current stroke colour (else the fill colour) so it sews in that thread.
fn add_text(app: &mut VectorcraftApp, d: &Dialog, height: f64) -> DialogResult {
    let st = app.session.active().ok_or("no document open")?;
    let centre = st.doc.artboards.first().map_or(vectorcraft_geom::kurbo::Point::ZERO, |a| a.rect.center());
    let paint = &app.session.paint;
    let color = paint.stroke.color().or_else(|| paint.fill.color()).unwrap_or(vectorcraft_color::Color::BLACK);
    let size = height * PT_PER_MM / CAP_SHARE;
    let r = app.run(
        "text.create",
        json!({"x": centre.x, "y": centre.y, "text": d.str("text"), "font": d.str("family"), "size": size, "color": color.to_hex()}),
    )?;
    let Some(id) = r["id"].as_u64() else { return Ok(r) };
    let bounds = app.session.active().and_then(|st| st.doc.node(vectorcraft_doc::NodeId(id))?.geometric_bounds());
    if let Some(b) = bounds {
        let (dx, dy) = (centre.x - b.center().x, centre.y - b.center().y);
        app.run("object.move", json!({"dx": dx, "dy": dy, "ids": [id]}))?;
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

    /// A two-letter digitized font (a satin I, a filled O), sized 10 mm and sewn well at 5–20 mm.
    fn add_font() {
        const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
            xmlns:inkstitch="http://inkstitch.org/namespace">
          <g inkscape:label="GlyphLayer-I"><path d="M 0,0 L 0,-37.8 M 8,0 L 8,-37.8" style="fill:none;stroke:#000" inkstitch:satin_column="True"/></g>
          <g inkscape:label="GlyphLayer-O"><path d="M 0,-19 A 15,19 0 1 0 30,-19 A 15,19 0 1 0 0,-19 Z" style="fill:#000"/></g>
        </svg>"##;
        let info = roidery_fonts::FontInfo { name: "Dialog Test".into(), min_scale: 0.5, max_scale: 2.0, ..Default::default() };
        roidery_fonts::library::insert(roidery_fonts::Font::from_parts("dialog_test", info, &[SVG.to_string()]).unwrap());
    }

    fn app() -> VectorcraftApp {
        add_font();
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 200, "height": 200})).unwrap();
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
        let mut v = vec![];
        // Twice: the window measures itself on the first pass.
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| crate::dialogs::show(app, ui.ctx()));
            out.textures_delta.clear();
            v.clear();
            out.shapes.iter().for_each(|c| texts(&c.shape, &mut v));
        }
        v
    }

    #[test]
    fn add_lettering_tells_embroidery_fonts_from_computer_fonts() {
        let mut app = app();
        app.run("type.addLettering", json!({})).unwrap();
        let texts = frame(&mut app);
        for want in ["Add Lettering", "Embroidery Fonts", "Computer Fonts", "Roidery Sans", "Built-in", "Satin"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        assert!(texts.iter().any(|t| t.starts_with("Fonts digitized for stitching")), "{texts:?}");
        // A digitized font, scrolled into view: lettering that sews as the font says.
        let d = app.ui.dialog.as_mut().unwrap();
        d.fields.insert("text".into(), json!("IOx"));
        d.fields.insert("font".into(), json!("dialog_test"));
        d.fields.remove("scrolled");
        let texts = frame(&mut app);
        for want in ["Dialog Test", "Fill", "Sews well at 5–20 mm"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        let r = crate::dialogs::confirm(&mut app).unwrap();
        assert!(app.ui.dialog.is_none());
        assert_eq!(app.ui.status, "The lettering font has no x");
        let info = app.run("stitch.info", json!({"ids": [r["id"]]})).unwrap();
        assert_eq!(info["lettering"][0]["fontName"], "Dialog Test");
        let types: Vec<&str> = info["objects"].as_array().unwrap().iter().filter_map(|o| o["type"].as_str()).collect();
        assert_eq!(types, ["satinBlocks", "tatami"]);

        // The Computer Fonts tab says what it does and adds a text object.
        app.run("type.addLettering", json!({"tab": "computer", "text": "Hi", "height": 20})).unwrap();
        let texts = frame(&mut app);
        assert!(texts.iter().any(|t| t.starts_with("Any font installed on this computer")), "{texts:?}");
        assert!(!texts.iter().any(|t| t == "Dialog Test"));
        let r = crate::dialogs::confirm(&mut app).unwrap();
        let id = vectorcraft_doc::NodeId(r["id"].as_u64().unwrap());
        let doc = &app.session.active().unwrap().doc;
        assert!(matches!(doc.node(id).unwrap().kind, vectorcraft_doc::NodeKind::Text(_)));
        let b = doc.node(id).unwrap().geometric_bounds().unwrap();
        assert!((b.center().x - 100.0).abs() < 1.0, "centred on the artboard: {b:?}");
        // Nothing to add: it says so and stays open.
        app.run("type.addLettering", json!({"text": "  "})).unwrap();
        assert!(crate::dialogs::confirm(&mut app).is_err());
    }

    #[test]
    fn stitch_settings_change_the_lettering_font() {
        let mut app = app();
        let id = app.run("stitch.createLettering", json!({"text": "IO", "font": "dialog_test", "height": 30})).unwrap()["id"].as_u64().unwrap();
        // Too big for the font: the panel says what it sews well at, and keeps the stitch rows to
        // the letters.
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut texts = vec![];
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| crate::panels::stitch_settings::show(&mut app, ui));
            out.textures_delta.clear();
            texts = out
                .shapes
                .iter()
                .filter_map(|c| if let egui::Shape::Text(t) = &c.shape { Some(t.galley.text().to_string()) } else { None })
                .collect();
        }
        for want in ["Lettering", "Dialog Test", "Embroidery font", "Change…", "Sews well at 5–20 mm"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        assert!(!texts.iter().any(|t| t == "Stitch type"), "{texts:?}");
        open_change(&mut app, &[id], "dialog_test", 30.0);
        assert!(frame(&mut app).iter().any(|t| t == "Change Lettering Font"));
        let d = app.ui.dialog.as_mut().unwrap();
        d.fields.insert("font".into(), json!("sans"));
        crate::dialogs::confirm(&mut app).unwrap();
        let info = app.run("stitch.info", json!({"ids": [id]})).unwrap();
        assert_eq!((info["lettering"][0]["fontName"].clone(), info["objects"][0]["type"].clone()), (json!("Roidery Sans"), json!("satin")));
    }
}
