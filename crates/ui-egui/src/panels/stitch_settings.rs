//! Roidery: the Stitch Settings panel (Window → Stitch Settings), how the selected objects sew,
//! like Ember's object properties: the stitch type (Automatic follows the fill-or-stroke rule),
//! the type's settings in mm, the underlay and the stitch count. With nothing selected it shows
//! the design's totals. Everything reads `stitch.info` and acts through `stitch.set`.
//!
//! Basic shows what most designs need (type, width, density, angle, stitch length, underlay);
//! Advanced adds pull compensation, fractional spacing, staggers and each underlay pass's own
//! settings (`stitch.settingsMode`).
//!
//! Under each part's stitch type its settings fold into sections: Stitching, Compensation, Curves
//! and Splits (satin) and Underlay, each open until clicked shut (`group_header`).

use egui::Ui;
use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::widgets;

pub const ID: &str = "stitchSettings";
const LABEL_W: f32 = 116.0;
const FIELD_W: f32 = 80.0;

/// The stitch types the menu offers, as (`stitch.set` type, label); "auto" resets to the rule.
/// Satin Rails (`satinBlocks`) are listed for objects that already are them: shapes become Satin
/// Rails through Object → Embroidery → Convert to Satin Rails.
pub(crate) fn types() -> [(&'static str, &'static str); 16] {
    [
        ("auto", tl!("Automatic")),
        ("tatami", tl!("Tatami")),
        ("contour", tl!("Contour Fill")),
        ("circular", tl!("Circular Fill")),
        ("gradient", tl!("Gradient Fill")),
        ("motif", tl!("Motif Fill")),
        ("guided", tl!("Guided Fill")),
        ("satinFill", tl!("Satin Fill")),
        ("satinBlocks", tl!("Satin Rails")),
        ("run", tl!("Single Run")),
        ("triple", tl!("Triple Run")),
        ("satin", tl!("Satin Stroke")),
        ("eStitch", tl!("E-Stitch")),
        ("rope", tl!("Rope")),
        ("manual", tl!("Manual")),
        ("none", tl!("Not Stitched")),
    ]
}

/// A number setting: (label, settings section, key, suffix, decimals, Advanced only).
pub(crate) type Field = (&'static str, &'static str, &'static str, &'static str, usize, bool);

/// The number settings a stitch type has, in panel order.
pub(crate) fn fields(kind: &str) -> Vec<Field> {
    let length = |section| (tl!("Stitch length"), section, if section == "fill" { "stitchLength" } else { "length" }, " mm", 2, false);
    match kind {
        "run" | "triple" => vec![length("run"), (tl!("Curve tolerance"), "run", "tolerance", " mm", 2, true)],
        "eStitch" => vec![(tl!("Width"), "eStitch", "width", " mm", 2, false), (tl!("Spacing"), "eStitch", "spacing", " mm", 2, false)],
        "rope" => vec![
            (tl!("Width"), "rope", "width", " mm", 2, false),
            (tl!("Density"), "rope", "spacing", " mm", 2, false),
            (tl!("Passes"), "rope", "passes", "", 0, false),
            (tl!("Slant"), "rope", "slant", " mm", 2, true),
        ],
        // Satin Fill: StitchFoundry's engine spaces and underlays each column itself unless told
        // not to (`satinFill_toggles`), and sews short stitches on curves or not.
        "satinFill" => vec![
            (tl!("Density"), "satin", "density", " mm", 2, false),
            (tl!("Pull compensation"), "satin", "pullCompensation", " mm", 2, false),
            (tl!("Split satin over"), "satin", "splitLength", " mm", 2, false),
        ],
        "satin" | "satinBlocks" => {
            let mut v = vec![];
            if kind == "satin" {
                v.push((tl!("Width"), "satin", "width", " mm", 2, false));
            }
            v.push((tl!("Density"), "satin", "density", " mm", 2, false));
            v.push((tl!("Pull compensation"), "satin", "pullCompensation", " mm", 2, true));
            v.push((tl!("Fractional spacing"), "satin", "fractionalSpacing", "", 2, true));
            v.push((tl!("Short stitches"), "satin", "shortStitchInset", "%", 0, false));
            v.push((tl!("Short stitch distance"), "satin", "shortStitchDistance", " mm", 2, true));
            v.push((tl!("Split satin over"), "satin", "splitLength", " mm", 2, false));
            v
        }
        "contour" => vec![
            (tl!("Density"), "fill", "rowSpacing", " mm", 2, false),
            length("fill"),
            (tl!("Pull compensation"), "fill", "pullCompensation", " mm", 2, true),
        ],
        "circular" => vec![
            (tl!("Density"), "fill", "rowSpacing", " mm", 2, false),
            length("fill"),
            (tl!("Center X"), "circular", "centerX", " mm", 2, false),
            (tl!("Center Y"), "circular", "centerY", " mm", 2, false),
            (tl!("Pull compensation"), "fill", "pullCompensation", " mm", 2, true),
            (tl!("Staggers"), "fill", "staggers", "", 0, true),
        ],
        "gradient" => vec![
            (tl!("Angle"), "fill", "angle", "°", 1, false),
            (tl!("Density"), "fill", "rowSpacing", " mm", 2, false),
            (tl!("End density"), "fill", "endSpacing", " mm", 2, false),
            length("fill"),
            (tl!("Pull compensation"), "fill", "pullCompensation", " mm", 2, true),
            (tl!("Staggers"), "fill", "staggers", "", 0, true),
        ],
        "motif" => vec![
            (tl!("Width"), "motif", "width", " mm", 2, false),
            (tl!("Height"), "motif", "height", " mm", 2, false),
            (tl!("Row spacing"), "motif", "spacing", " mm", 2, false),
            (tl!("Angle"), "motif", "angle", "°", 1, false),
            (tl!("Stitch length"), "motif", "length", " mm", 2, true),
        ],
        "guided" => vec![(tl!("Density"), "fill", "rowSpacing", " mm", 2, false), length("fill"), (tl!("Staggers"), "fill", "staggers", "", 0, true)],
        "tatami" => vec![
            (tl!("Angle"), "fill", "angle", "°", 1, false),
            (tl!("Density"), "fill", "rowSpacing", " mm", 2, false),
            length("fill"),
            (tl!("Pull compensation"), "fill", "pullCompensation", " mm", 2, true),
            (tl!("Staggers"), "fill", "staggers", "", 0, true),
        ],
        _ => vec![],
    }
}

/// The motif fill's patterns, as (`stitch.set` pattern, label).
pub(crate) fn motif_patterns() -> [(&'static str, &'static str); 4] {
    [("zigzag", tl!("Zigzag")), ("wave", tl!("Wave")), ("diamonds", tl!("Diamonds")), ("scales", tl!("Scales"))]
}

/// A satin underlay pass's own settings: (label, key, default) for `stitch.set`'s underlays.
fn underlay_fields(ty: &str) -> Vec<(&'static str, &'static str, f64)> {
    match ty {
        "centerWalk" => vec![(tl!("Length"), "length", 2.5)],
        "contour" => vec![(tl!("Length"), "length", 2.5), (tl!("Inset"), "inset", 0.4)],
        "zigZag" => vec![(tl!("Spacing"), "spacing", 2.0), (tl!("Inset"), "inset", 0.4)],
        _ => vec![],
    }
}

/// Satin underlay passes: (label, underlay type), sewn in this order.
pub(crate) fn satin_underlays() -> [(&'static str, &'static str); 3] {
    [(tl!("Center walk"), "centerWalk"), (tl!("Edge walk"), "contour"), (tl!("Zigzag"), "zigZag")]
}

/// The sections a part's settings fold into, in panel order, as (id, title). The underlay has
/// its own section after these.
fn groups() -> [(&'static str, &'static str); 3] {
    [("stitches", tl!("Stitching")), ("compensation", tl!("Compensation")), ("curves", tl!("Curves and Splits"))]
}

/// The section a setting's key belongs to.
fn group_of(key: &str) -> &'static str {
    match key {
        "pullCompensation" | "fractionalSpacing" => "compensation",
        "shortStitchInset" | "shortStitchDistance" | "splitLength" => "curves",
        _ => "stitches",
    }
}

/// A section's folding header, open until it is clicked shut (remembered per part and section).
/// Whether it is open. Smaller and dimmer than the part's header above it, with a rule out to the
/// panel's edge, so the part, its sections and their rows read as three levels.
fn group_header(ui: &mut Ui, part: &str, group: &str, title: &str) -> bool {
    let id = egui::Id::new(("stitch-settings-group", part, group));
    let mut open = ui.ctx().data_mut(|d| *d.get_persisted_mut_or(id, true));
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.add_space(2.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, t.hover);
    }
    let strong = if resp.hovered() { t.text_strong } else { t.text_dim };
    let chevron = egui::Rect::from_center_size(egui::pos2(rect.left() + 7.0, rect.center().y), egui::vec2(10.0, 10.0));
    crate::icons::paint(ui, if open { "chevron-down" } else { "chevron-right" }, chevron, strong);
    let text =
        ui.painter().text(egui::pos2(rect.left() + 16.0, rect.center().y), egui::Align2::LEFT_CENTER, title, crate::theme::semibold(10.5), strong);
    if text.right() + 8.0 < rect.right() - 2.0 {
        let y = rect.center().y.round() + 0.5;
        ui.painter().line_segment([egui::pos2(text.right() + 8.0, y), egui::pos2(rect.right() - 2.0, y)], egui::Stroke::new(1.0, t.divider));
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, title));
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        open = !open;
        ui.ctx().data_mut(|d| d.insert_persisted(id, open));
    }
    open
}

/// A section's rows, set in under its header with a guide line down their left, as a tree.
fn nested(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let r = ui
        .horizontal(|ui| {
            ui.add_space(14.0);
            ui.vertical(add).response.rect
        })
        .inner;
    let x = r.left() - 7.5;
    if r.height() > 4.0 {
        ui.painter().line_segment([egui::pos2(x, r.top() + 2.0), egui::pos2(x, r.bottom() - 2.0)], egui::Stroke::new(1.0, t.input_border));
    }
    ui.add_space(2.0);
}

/// A part's header: a band across the panel with the part's thread colour, its name and its
/// stitch count, heading the stitch type and the sections under it.
fn part_header(ui: &mut Ui, title: &str, thread: Option<egui::Color32>, stitches: u64) {
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, t.input);
    let mut x = rect.left() + 9.0;
    if let Some(c) = thread {
        let center = egui::pos2(x + 6.0, rect.center().y);
        ui.painter().circle_filled(center, 6.0, c);
        ui.painter().circle_stroke(center, 6.0, egui::Stroke::new(1.0, t.input_border));
        x += 18.0;
    }
    ui.painter().text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, title, crate::theme::semibold(13.5), t.text_strong);
    let count = crate::i18n::fmt(tl!("{n} stitches"), &[("n", &stitches.to_string())]);
    ui.painter().text(
        egui::pos2(rect.right() - 9.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        count,
        egui::FontId::proportional(11.0),
        t.text_dim,
    );
    ui.add_space(4.0);
}

/// `stitch.info` for the selection, recomputed only when the document or the selection changes
/// (it sews the objects to count their stitches).
pub(crate) fn info(app: &mut VectorcraftApp, ctx: &egui::Context) -> Option<Value> {
    type Key = (u64, u64, Vec<vectorcraft_doc::NodeId>);
    let st = app.session.active()?;
    let key: Key = (st.uid, st.revision, st.selection.objects.clone());
    let id = egui::Id::new("stitch-settings-info");
    if let Some((k, v)) = ctx.data(|d| d.get_temp::<(Key, Value)>(id))
        && k == key
    {
        return Some(v);
    }
    let v = app.session.execute("stitch.info", &json!({})).ok()?;
    ctx.data_mut(|d| d.insert_temp(id, (key, v.clone())));
    Some(v)
}

/// The value every object shares at `path`, else `None`.
pub(crate) fn shared<'a>(objects: &'a [Value], path: &[&str]) -> Option<&'a Value> {
    let at = |o: &'a Value| path.iter().try_fold(o, |v, k| v.get(*k));
    let first = at(objects.first()?)?;
    objects.iter().all(|o| at(o) == Some(first)).then_some(first)
}

fn set(app: &mut VectorcraftApp, p: Value) {
    app.run("stitch.set", p).ok();
}

fn row(ui: &mut Ui, label: &str, value: &str) {
    widgets::label_row(ui, label, LABEL_W, |ui| {
        ui.label(egui::RichText::new(value).size(12.5));
    });
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Some(info) = info(app, ui.ctx()) else {
        widgets::dim_label(ui, tl!("No document"));
        return;
    };
    body(app, ui, &info);
}

/// The Properties panel's Embroidery section: the same as the panel, left out when the selection
/// has nothing to sew.
pub fn properties_section(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Some(info) = info(app, ui.ctx()) else { return };
    let selected = app.session.active().is_some_and(|st| !st.selection.is_empty());
    if selected && info["objects"].as_array().is_none_or(Vec::is_empty) {
        return;
    }
    widgets::section_header(ui, tl!("Embroidery"));
    body(app, ui, &info);
    widgets::divider(ui);
}

/// Lettering rows: the text, letter height and spacing of the selected lettering objects.
/// The Lettering section, for the lettering among the selection (`stitch.info`'s `lettering`
/// rows): its text, its font (an embroidery font or a built-in satin font, Change… picks
/// another), its height with the sizes the font sews well at, and its spacing.
fn lettering(app: &mut VectorcraftApp, ui: &mut Ui, rows: &[Value]) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let ls: Vec<Value> = rows.iter().map(|r| r["lettering"].clone()).collect();
    widgets::subheader(ui, tl!("Lettering"));
    let text = shared(&ls, &["text"]).and_then(Value::as_str).map(str::to_string);
    let mut edit = None;
    widgets::label_row(ui, tl!("Text"), LABEL_W, |ui| {
        edit = widgets::text_field(ui, "lettering-text", text.as_deref(), ui.available_width().min(170.0), 2);
    });
    if let Some(t) = edit.filter(|t| !t.is_empty()) {
        lettering_set(app, json!({ "text": t }));
    }
    let font = shared(rows, &["fontName"]).and_then(Value::as_str).unwrap_or("");
    let digitized = shared(rows, &["digitized"]).and_then(Value::as_bool);
    let mut change = false;
    widgets::label_row(ui, tl!("Font"), LABEL_W, |ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(font).size(12.5).color(t.text_strong));
            let kind = match digitized {
                Some(true) => tl!("Embroidery font"),
                Some(false) => tl!("Built-in satin font"),
                None => "",
            };
            if !kind.is_empty() {
                ui.label(egui::RichText::new(kind).size(11.0).color(t.text_dim));
            }
        });
        change = widgets::secondary_button(ui, tl!("Change…")).clicked();
    });
    if change {
        let ids: Vec<u64> = rows.iter().filter_map(|r| r["id"].as_u64()).collect();
        let font = shared(&ls, &["font"]).and_then(Value::as_str).unwrap_or("sans").to_string();
        let height = shared(&ls, &["height"]).and_then(Value::as_f64).unwrap_or(10.0);
        crate::dialogs::lettering::open_change(app, &ids, &font, height);
    }
    for e in rows.iter().filter_map(|r| r["error"].as_str()) {
        ui.label(egui::RichText::new(e).size(11.5).color(t.error));
    }
    let height = shared(&ls, &["height"]).and_then(Value::as_f64);
    let range = (shared(rows, &["sizeMin"]).and_then(Value::as_f64), shared(rows, &["sizeMax"]).and_then(Value::as_f64));
    for (label, key, suffix) in
        [(tl!("Letter height"), "height", " mm"), (tl!("Letter spacing"), "spacing", " mm"), (tl!("Line spacing"), "lineSpacing", "×")]
    {
        let value = shared(&ls, &[key]).and_then(Value::as_f64);
        let mut edit = None;
        widgets::label_row(ui, label, LABEL_W, |ui| {
            edit = widgets::mixed_field(ui, ("lettering", key), value, suffix, if key == "lineSpacing" { 2 } else { 1 }, FIELD_W);
        });
        if let Some(v) = edit.filter(|v| v.is_finite()) {
            lettering_set(app, json!({ key: v }));
        }
        // The sizes the font sews well at, under the height (amber when this one isn't).
        if key == "height"
            && let (Some(lo), Some(hi)) = range
        {
            let off = height.is_some_and(|h| h < lo * 0.95 - 0.05 || h > hi * 1.05 + 0.05);
            ui.horizontal(|ui| {
                ui.add_space(LABEL_W + 8.0);
                let hint = crate::dialogs::lettering::size_hint(lo, hi);
                ui.label(egui::RichText::new(hint).size(11.0).color(if off { t.warning } else { t.text_dim }));
            });
        }
    }
}

pub(crate) fn lettering_set(app: &mut VectorcraftApp, p: Value) {
    match app.run("stitch.setLettering", p) {
        Ok(r) if r["missing"].as_str().is_some_and(|m| !m.is_empty()) => {
            let m = r["missing"].as_str().unwrap_or_default().to_string();
            app.status(tl!("The lettering font has no {chars}").replace("{chars}", &m));
        }
        Ok(_) => {}
        Err(e) => app.status(e),
    }
}

/// Basic and Advanced, as text tabs with the current one underlined (in the panel's own font, so
/// it draws wherever the panel does). Returns the tab clicked: true for Advanced.
fn mode_tabs(ui: &mut Ui, advanced: bool) -> Option<bool> {
    let t = crate::theme::Tokens::get(ui.ctx());
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        for (adv, label) in [(false, tl!("Basic")), (true, tl!("Advanced"))] {
            let galley = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.5), t.text);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(galley.size().x, 24.0), egui::Sense::click());
            let sel = adv == advanced;
            let color = if sel || resp.hovered() { t.text_strong } else { t.text_dim };
            ui.painter().galley(egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0 - 1.0), galley, color);
            if sel {
                let line = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 2.0), rect.right_bottom());
                ui.painter().rect_filled(line, egui::CornerRadius::same(1), t.accent);
            }
            if resp.clicked() {
                clicked = Some(adv);
            }
        }
    });
    clicked
}

/// A sew time, as minutes or hours and minutes.
pub(crate) fn sew_time(minutes: f64) -> String {
    let m = if minutes.is_finite() { minutes.max(0.0).ceil() as u64 } else { 0 };
    if m >= 60 {
        tl!("{h} h {m} min").replace("{h}", &(m / 60).to_string()).replace("{m}", &(m % 60).to_string())
    } else {
        tl!("{m} min").replace("{m}", &m.max(1).to_string())
    }
}

/// Thread to have on hand: top thread (all colours) and bobbin thread, metres.
fn thread_amount(top: f64, bobbin: f64) -> String {
    let m = |v: f64| format!("{:.1}", if v.is_finite() { v.max(0.0) } else { 0.0 });
    tl!("{top} m + {bobbin} m bobbin").replace("{top}", &m(top)).replace("{bobbin}", &m(bobbin))
}

fn body(app: &mut VectorcraftApp, ui: &mut Ui, info: &Value) {
    let empty = app.session.active().is_none_or(|st| st.selection.is_empty());
    let objects: Vec<Value> = info["objects"].as_array().cloned().unwrap_or_default();
    let mm = |k: &str| format!("{:.1} mm", info[k].as_f64().unwrap_or(0.0));
    if empty {
        widgets::subheader(ui, tl!("Design"));
        row(ui, tl!("Stitches"), &info["stitches"].as_u64().unwrap_or(0).to_string());
        row(ui, tl!("Colors"), &(info["colorChanges"].as_u64().unwrap_or(0) + u64::from(!objects.is_empty())).to_string());
        row(ui, tl!("Trims"), &info["trims"].as_u64().unwrap_or(0).to_string());
        row(ui, tl!("Size"), &format!("{} × {}", mm("widthMm"), mm("heightMm")));
        row(ui, tl!("Sew time"), &sew_time(info["sewMinutes"].as_f64().unwrap_or(0.0)));
        let top: f64 = info["threadMeters"].as_array().map_or(0.0, |a| a.iter().filter_map(Value::as_f64).sum());
        row(ui, tl!("Thread"), &thread_amount(top, info["bobbinMeters"].as_f64().unwrap_or(0.0)));
        if let Some(q) = crate::panels::threads::design(app, ui.ctx()) {
            crate::panels::threads::fabric_row(app, ui, &q, LABEL_W);
            crate::panels::threads::hoop_rows(app, ui, &q, info.get("hoop"), LABEL_W);
            row(ui, tl!("Thread chart"), q["chart"]["name"].as_str().unwrap_or(tl!("None")));
        }
        widgets::divider(ui);
        widgets::dim_label(ui, tl!("Select objects to set how they sew."));
        return;
    }
    if objects.is_empty() {
        widgets::dim_label(ui, tl!("Nothing selected sews: paths, compound paths and text do."));
        return;
    }
    let advanced = app.ui.stitch_advanced;
    if let Some(adv) = mode_tabs(ui, advanced) {
        app.run("stitch.settingsMode", json!({"advanced": adv})).ok();
    }
    let letterings: Vec<Value> = info["lettering"].as_array().cloned().unwrap_or_default();
    if !letterings.is_empty() {
        lettering(app, ui, &letterings);
    }
    // Lettering in an embroidery font picked as a whole: each letter sews as its digitizer made
    // it, so the stitch rows stay with the pieces (selected inside the group).
    let picked: Vec<u64> = app.session.active().map(|st| st.selection.objects.iter().map(|id| id.0).collect()).unwrap_or_default();
    let whole_lettering = !letterings.is_empty()
        && letterings.iter().all(|l| l["digitized"] == json!(true))
        && picked.iter().all(|id| letterings.iter().any(|l| l["id"].as_u64() == Some(*id)));
    if whole_lettering {
        widgets::divider(ui);
        widgets::dim_label(
            ui,
            tl!("Each letter sews as the font's digitizer made it. To change how one piece sews, select it inside the lettering."),
        );
        let stitches: u64 = objects.iter().filter_map(|o| o["stitches"].as_u64()).sum();
        widgets::dim_label(ui, &crate::i18n::fmt(tl!("{n} stitches"), &[("n", &stitches.to_string())]));
        return;
    }
    // A Satin Fill is all fill, manual stitches and lettering all stroke: just that part shows.
    let whole = shared(&objects, &["wholePart"]).and_then(Value::as_str).map(str::to_string);
    for part in ["fill", "stroke"] {
        if whole.as_deref().is_some_and(|w| w != part) {
            continue;
        }
        let values: Vec<Value> = objects.iter().map(|o| o[part].clone()).collect();
        if part == "stroke" && whole.is_none() {
            widgets::divider(ui);
        }
        // The part heads its sections: a band with its thread, larger than their headers.
        let sews = values.iter().any(|o| o["sews"] == json!(true));
        let thread = shared(&values, &["thread"]).filter(|_| sews).map(crate::panels::sequencer::rgb);
        let count = values.iter().filter_map(|o| o["stitches"].as_u64()).sum();
        part_header(ui, if part == "fill" { tl!("Fill") } else { tl!("Stroke") }, thread, count);
        ui.push_id(part, |ui| part_section(app, ui, part, &values, advanced));
    }
    widgets::divider(ui);
    let stitches: u64 = objects.iter().filter_map(|o| o["stitches"].as_u64()).sum();
    widgets::dim_label(ui, &crate::i18n::fmt(tl!("{n} stitches"), &[("n", &stitches.to_string())]));
}

/// Satin Fill's on-off settings, as (label, settings section, key, group, on when unset).
fn satin_fill_toggles() -> [(&'static str, &'static str, &'static str, &'static str, bool); 2] {
    [(tl!("Automatic spacing"), "satin", "autoSpacing", "stitches", true), (tl!("Short stitches"), "satin", "shortStitchInset", "curves", true)]
}

/// Whether a part's menu offers stitch type `id`: fills offer fill stitches, Satin Fill and Satin
/// Rails, strokes line stitches and Satin Stroke (manual stitches are the whole object, in the
/// stroke's).
pub(crate) fn allowed(part: &str, id: &str) -> bool {
    const FILLS: [&str; 8] = ["tatami", "contour", "circular", "gradient", "motif", "guided", "satinFill", "satinBlocks"];
    matches!(id, "auto" | "none") || (part == "fill") == FILLS.contains(&id)
}

fn set_part(app: &mut VectorcraftApp, part: &str, mut p: Value) {
    p["part"] = json!(part);
    set(app, p);
}

/// One part's rows (the fill's or the stroke's) for the selected objects: its stitch type, its
/// settings and its underlay. `objects` are each object's `stitch.info` part.
fn part_section(app: &mut VectorcraftApp, ui: &mut Ui, part: &str, objects: &[Value], advanced: bool) {
    // The type: Automatic when every object follows the rule, else the type they share.
    let all_auto = objects.iter().all(|o| o["auto"] == json!(true));
    let kind = shared(objects, &["type"]).and_then(Value::as_str).unwrap_or("");
    let current = if all_auto {
        "auto"
    } else if objects.iter().any(|o| o["auto"] == json!(true)) {
        ""
    } else {
        kind
    };
    // Satin Rails only for objects that are already them (converting is in the Object menu).
    let types: Vec<(&str, &str)> = types().into_iter().filter(|t| allowed(part, t.0) && (t.0 != "satinBlocks" || kind == "satinBlocks")).collect();
    let labels: Vec<&str> = types.iter().map(|t| t.1).collect();
    let current_label = types.iter().find(|t| t.0 == current).map_or("", |t| t.1);
    let mut chosen = None;
    widgets::label_row(ui, tl!("Stitch type"), LABEL_W, |ui| {
        chosen = widgets::dropdown(
            ui,
            if part == "fill" { "stitch-type-fill" } else { "stitch-type-stroke" },
            current_label,
            &labels,
            ui.available_width().min(150.0),
        );
    });
    if let Some((id, _)) = chosen.and_then(|i| types.get(i)) {
        set_part(app, part, if *id == "auto" { json!({"reset": true}) } else { json!({"type": id}) });
        return;
    }
    if kind.is_empty() {
        widgets::dim_label(ui, tl!("The selected objects sew differently: choose a stitch type to set them all."));
        return;
    }
    // Automatic with nothing to sew: no fill (or no stroke) yet.
    if objects.iter().all(|o| o["sews"] == json!(false)) {
        if all_auto {
            widgets::dim_label(
                ui,
                if part == "fill" { tl!("No fill: choose a fill stitch to add one.") } else { tl!("No stroke: choose a stroke stitch to add one.") },
            );
        }
        return;
    }
    let settings: Vec<Value> = objects.iter().map(|o| o["settings"].clone()).collect();
    let visible: Vec<Field> = fields(kind).into_iter().filter(|f| advanced || !f.5).collect();
    for (group, title) in groups() {
        let rows: Vec<&Field> = visible.iter().filter(|f| group_of(f.2) == group).collect();
        let extra = group == "stitches" && kind == "motif";
        let toggles: Vec<_> = satin_fill_toggles().into_iter().filter(|t| kind == "satinFill" && t.3 == group).collect();
        if rows.is_empty() && !extra && toggles.is_empty() {
            continue;
        }
        if !group_header(ui, part, group, title) {
            continue;
        }
        nested(ui, |ui| {
            if extra {
                let patterns = motif_patterns();
                let current = shared(&settings, &["motif", "pattern"]).and_then(Value::as_str).unwrap_or("");
                let labels: Vec<&str> = patterns.iter().map(|p| p.1).collect();
                let current_label = patterns.iter().find(|p| p.0 == current).map_or("", |p| p.1);
                let mut chosen = None;
                widgets::label_row(ui, tl!("Pattern"), LABEL_W, |ui| {
                    chosen = widgets::dropdown(ui, "stitch-motif", current_label, &labels, ui.available_width().min(150.0));
                });
                if let Some((id, _)) = chosen.and_then(|i| patterns.get(i)) {
                    set_part(app, part, json!({"settings": {"motif": {"pattern": id}}}));
                }
            }
            for &&(label, section, key, suffix, decimals, _) in &rows {
                let value = shared(&settings, &[section, key]).and_then(Value::as_f64);
                let mut edit = None;
                widgets::label_row(ui, label, LABEL_W, |ui| {
                    edit = widgets::mixed_field(ui, ("stitch", section, key), value, suffix, decimals, FIELD_W);
                });
                if let Some(v) = edit.filter(|v| v.is_finite()) {
                    // Staggers and passes are whole numbers.
                    let v = if decimals == 0 { json!(v.round().clamp(1.0, 64.0) as u32) } else { json!(v) };
                    set_part(app, part, json!({"settings": {section: {key: v}}}));
                }
            }
            for (label, section, key, _, default) in toggles {
                // Short stitches are on when their inset is above 0.
                let on =
                    shared(&settings, &[section, key]).map(|v| v.as_bool().unwrap_or_else(|| v.as_f64().is_some_and(|x| x > 0.0))).or(Some(default));
                widgets::label_row(ui, label, LABEL_W, |ui| {
                    if widgets::check3(ui, "", on, true) {
                        let next = on != Some(true);
                        let v = if key == "shortStitchInset" { json!(if next { 15.0 } else { 0.0 }) } else { json!(next) };
                        set_part(app, part, json!({"settings": {section: {key: v}}}));
                    }
                });
            }
            if extra {
                let on = shared(&settings, &["motif", "triple"]).and_then(Value::as_bool);
                widgets::label_row(ui, tl!("Triple Run"), LABEL_W, |ui| {
                    if widgets::check3(ui, "", on, true) {
                        set_part(app, part, json!({"settings": {"motif": {"triple": on != Some(true)}}}));
                    }
                });
            }
        });
    }
    if matches!(kind, "tatami" | "contour" | "circular" | "guided" | "satin" | "satinFill" | "satinBlocks")
        && !group_header(ui, part, "underlay", tl!("Underlay"))
    {
        return;
    }
    // Underlay.
    nested(ui, |ui| match kind {
        "tatami" | "contour" | "circular" | "guided" => {
            let on = shared(&settings, &["fill", "underlay"]).and_then(Value::as_bool);
            widgets::label_row(ui, tl!("Sew underlay"), LABEL_W, |ui| {
                if widgets::check3(ui, "", on, true) {
                    set_part(app, part, json!({"settings": {"fill": {"underlay": on != Some(true)}}}));
                }
            });
            if advanced && on != Some(false) {
                for (label, key) in [(tl!("Underlay spacing"), "underlaySpacing"), (tl!("Underlay inset"), "underlayInset")] {
                    let value = shared(&settings, &["fill", key]).and_then(Value::as_f64);
                    let mut edit = None;
                    widgets::label_row(ui, label, LABEL_W, |ui| {
                        edit = widgets::mixed_field(ui, ("stitch", "fill", key), value, " mm", 2, FIELD_W);
                    });
                    if let Some(v) = edit.filter(|v| v.is_finite()) {
                        set_part(app, part, json!({"settings": {"fill": {key: v}}}));
                    }
                }
            }
        }
        "satin" | "satinFill" | "satinBlocks" => {
            // Satin Fill picks each column's underlay from its width unless told not to.
            if kind == "satinFill" {
                let auto = shared(&settings, &["satin", "autoUnderlay"]).map(|v| v.as_bool().unwrap_or(true)).or(Some(true));
                widgets::label_row(ui, tl!("Automatic"), LABEL_W, |ui| {
                    if widgets::check3(ui, "", auto, true) {
                        set_part(app, part, json!({"settings": {"satin": {"autoUnderlay": auto != Some(true)}}}));
                    }
                });
                if auto != Some(false) {
                    return;
                }
            }
            let lists = shared(&settings, &["satin", "underlays"]).and_then(Value::as_array).cloned();
            let has = |ty: &str| lists.as_ref().map(|l| l.iter().any(|u| u["type"] == json!(ty)));
            let mut toggled = None;
            let mut changed: Option<(&str, &str, f64)> = None;
            for (label, ty) in satin_underlays() {
                widgets::label_row(ui, "", LABEL_W, |ui| {
                    if widgets::check3(ui, label, has(ty), true) {
                        toggled = Some(ty);
                    }
                });
                if !advanced || has(ty) != Some(true) {
                    continue;
                }
                // The pass's own settings, indented under it.
                for (flabel, key, default) in underlay_fields(ty) {
                    let value = shared(&settings, &["satin", "underlays"])
                        .and_then(Value::as_array)
                        .and_then(|l| l.iter().find(|u| u["type"] == json!(ty)))
                        .map(|u| u[key].as_f64().unwrap_or(default));
                    let mut edit = None;
                    widgets::label_row(ui, "", LABEL_W, |ui| {
                        ui.add_sized([46.0, 20.0], egui::Label::new(egui::RichText::new(flabel).size(11.5)).truncate());
                        edit = widgets::mixed_field(ui, ("stitch", "underlay", ty, key), value, " mm", 2, FIELD_W - 10.0);
                    });
                    if let Some(v) = edit.filter(|v| v.is_finite()) {
                        changed = Some((ty, key, v));
                    }
                }
            }
            if let Some((ty, key, v)) = changed {
                let next: Vec<Value> = lists
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|mut u| {
                        if u["type"] == json!(ty) {
                            u[key] = json!(v);
                        }
                        u
                    })
                    .collect();
                set_part(app, part, json!({"settings": {"satin": {"underlays": next}}}));
            }
            if let Some(ty) = toggled {
                // Keep the other passes as they are (their own lengths and insets) in sewing order.
                let current = lists.unwrap_or_default();
                let on = !current.iter().any(|u| u["type"] == json!(ty));
                let next: Vec<Value> = satin_underlays()
                    .iter()
                    .filter_map(
                        |(_, t)| {
                            if *t == ty { on.then(|| json!({"type": t})) } else { current.iter().find(|u| u["type"] == json!(t)).cloned() }
                        },
                    )
                    .collect();
                set_part(app, part, json!({"settings": {"satin": {"underlays": next}}}));
            }
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

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

    fn run(app: &mut VectorcraftApp, id: &str, p: Value) -> Value {
        app.session.execute(id, &p).unwrap()
    }

    #[test]
    fn shows_the_design_then_the_selections_settings() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        run(&mut app, "file.new", json!({"width": 200, "height": 200}));
        run(&mut app, "shape.rectangle", json!({"x": 0, "y": 0, "width": 60, "height": 40}));
        run(&mut app, "select.none", json!({}));
        let texts = frame(&mut app);
        for want in ["Design", "Stitches", "Size", "Select objects to set how they sew."] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        run(&mut app, "select.all", json!({}));
        let texts = frame(&mut app);
        // A filled rectangle: Automatic, sewn as tatami.
        for want in ["Stitch type", "Automatic", "Angle", "45°", "Density", "0.4 mm", "Underlay"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        assert!(texts.iter().any(|t| t.ends_with(" stitches")), "{texts:?}");
        // Satin shows its underlay passes.
        run(&mut app, "stitch.set", json!({"type": "satin"}));
        let texts = frame(&mut app);
        for want in ["Satin Stroke", "Width", "Center walk", "Edge walk", "Zigzag"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        assert!(!texts.iter().any(|t| t == "Angle"));
    }

    #[test]
    fn convert_to_satin_rails_shows_as_a_fill_not_a_stroke() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        run(&mut app, "file.new", json!({"width": 200, "height": 200}));
        run(&mut app, "shape.rectangle", json!({"x": 20, "y": 20, "width": 100, "height": 12}));
        run(&mut app, "stitch.convertToSatin", json!({}));
        let texts = frame(&mut app);
        for want in ["Fill", "Satin Rails", "Density", "Center walk"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        assert!(!texts.iter().any(|t| t == "Stroke"), "no stroke section: {texts:?}");
        // The fill menu offers Satin Fill and Satin Rails; the stroke menu Satin Stroke.
        assert!(allowed("fill", "satinBlocks") && !allowed("stroke", "satinBlocks"));
        assert!(allowed("fill", "satinFill") && !allowed("stroke", "satinFill"));
        assert!(allowed("stroke", "satin") && !allowed("fill", "satin"));
    }

    #[test]
    fn satin_fill_is_a_fill_with_automatic_spacing_and_underlay() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        run(&mut app, "file.new", json!({"width": 200, "height": 200}));
        run(&mut app, "shape.rectangle", json!({"x": 20, "y": 20, "width": 100, "height": 12}));
        run(&mut app, "stitch.set", json!({"part": "fill", "type": "satinFill"}));
        let texts = frame(&mut app);
        for want in ["Fill", "Satin Fill", "Density", "Automatic spacing", "Short stitches", "Underlay", "Automatic"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        // Underlay passes show only once automatic underlay is off; Satin Rails aren't offered.
        assert!(!texts.iter().any(|t| t == "Center walk" || t == "Angle"), "{texts:?}");
        run(&mut app, "stitch.set", json!({"part": "fill", "settings": {"satin": {"autoUnderlay": false}}}));
        let texts = frame(&mut app);
        assert!(texts.iter().any(|t| t == "Center walk"), "{texts:?}");
    }

    #[test]
    fn properties_shows_an_embroidery_section_for_sewable_selections() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.ui.embroidery = true;
        run(&mut app, "file.new", json!({"width": 200, "height": 200}));
        let section = |app: &mut VectorcraftApp| {
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            let mut texts = vec![];
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| crate::panels::properties::show(app, ui));
            out.textures_delta.clear();
            for c in &out.shapes {
                if let egui::Shape::Text(t) = &c.shape {
                    texts.push(t.galley.text().to_string());
                }
            }
            texts
        };
        assert!(section(&mut app).iter().any(|t| t == "Embroidery"));
        run(&mut app, "shape.rectangle", json!({"x": 0, "y": 0, "width": 60, "height": 40}));
        let texts = section(&mut app);
        assert!(texts.iter().any(|t| t == "Embroidery") && texts.iter().any(|t| t == "Stitch type"), "{texts:?}");
        // Text sews too.
        run(&mut app, "select.none", json!({}));
        run(&mut app, "text.create", json!({"x": 10, "y": 150, "text": "Hi", "size": 24}));
        assert!(section(&mut app).iter().any(|t| t == "Embroidery"));
        // A guide doesn't.
        run(&mut app, "select.none", json!({}));
        let g = run(&mut app, "shape.rectangle", json!({"x": 0, "y": 100, "width": 60, "height": 40}))["id"].clone();
        run(&mut app, "view.guides.make", json!({}));
        run(&mut app, "select.set", json!({"ids": [g]}));
        assert!(!section(&mut app).iter().any(|t| t == "Embroidery"));
    }

    #[test]
    fn lettering_shows_its_text_and_height() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        run(&mut app, "file.new", json!({"width": 200, "height": 200}));
        run(&mut app, "stitch.createLettering", json!({"text": "Abc"}));
        let texts = frame(&mut app);
        for want in ["Text", "Abc", "Letter height", "10 mm", "Satin Stroke"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        lettering_set(&mut app, json!({"text": "A€"}));
        assert_eq!(app.ui.status, "The lettering font has no €");
        // Not lettering: no lettering rows.
        run(&mut app, "shape.rectangle", json!({"x": 0, "y": 0, "width": 60, "height": 40}));
        assert!(!frame(&mut app).iter().any(|t| t == "Letter height"));
    }

    #[test]
    fn mixed_types_ask_for_one() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        run(&mut app, "file.new", json!({"width": 200, "height": 200}));
        run(&mut app, "shape.rectangle", json!({"x": 0, "y": 0, "width": 60, "height": 40}));
        run(&mut app, "stitch.set", json!({"type": "satin"}));
        run(&mut app, "shape.ellipse", json!({"x": 80, "y": 0, "width": 40, "height": 40}));
        run(&mut app, "select.all", json!({}));
        let texts = frame(&mut app);
        assert!(texts.iter().any(|t| t.starts_with("The selected objects sew differently")), "{texts:?}");
        // Nothing to sew in the selection.
        run(&mut app, "select.none", json!({}));
        let g = run(&mut app, "shape.rectangle", json!({"x": 0, "y": 100, "width": 60, "height": 40}))["id"].clone();
        run(&mut app, "view.guides.make", json!({}));
        run(&mut app, "select.set", json!({"ids": [g]}));
        let texts = frame(&mut app);
        assert!(texts.iter().any(|t| t.starts_with("Nothing selected sews")), "{texts:?}");
    }

    #[test]
    fn fill_and_stroke_each_have_a_section() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        run(&mut app, "file.new", json!({"width": 200, "height": 200}));
        // White fill, black stroke: a tatami fill and a running-stitch border.
        run(&mut app, "shape.rectangle", json!({"x": 0, "y": 0, "width": 60, "height": 40}));
        let texts = frame(&mut app);
        for want in ["Fill", "Stroke", "Angle", "Stitch length"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        assert_eq!(texts.iter().filter(|t| *t == "Automatic").count(), 2, "{texts:?}");
        // The stroke as a satin: the fill keeps its tatami.
        set_part(&mut app, "stroke", json!({"type": "satin"}));
        let texts = frame(&mut app);
        for want in ["Satin Stroke", "Width", "Angle"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        // No fill: the fill section says how to add one.
        run(&mut app, "paint.setFill", json!({"none": true}));
        let texts = frame(&mut app);
        assert!(texts.iter().any(|t| t.starts_with("No fill")), "{texts:?}");
    }

    #[test]
    fn thread_reads_as_metres_of_top_and_bobbin() {
        assert_eq!(thread_amount(12.34, 4.11), "12.3 m + 4.1 m bobbin");
        assert_eq!(thread_amount(f64::NAN, -1.0), "0.0 m + 0.0 m bobbin");
    }

    #[test]
    fn sew_times_read_as_minutes_or_hours() {
        assert_eq!(sew_time(0.2), "1 min");
        assert_eq!(sew_time(12.4), "13 min");
        assert_eq!(sew_time(125.0), "2 h 5 min");
        assert_eq!(sew_time(f64::NAN), "1 min");
    }

    #[test]
    fn basic_hides_what_advanced_shows() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        run(&mut app, "file.new", json!({"width": 200, "height": 200}));
        run(&mut app, "shape.rectangle", json!({"x": 0, "y": 0, "width": 60, "height": 40}));
        let texts = frame(&mut app);
        for want in ["Basic", "Advanced", "Angle", "Density"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        for not in ["Pull compensation", "Staggers", "Underlay spacing"] {
            assert!(!texts.iter().any(|t| t == not), "{not} in {texts:?}");
        }
        assert_eq!(app.run("stitch.settingsMode", json!({})).unwrap(), json!(true));
        let texts = frame(&mut app);
        for want in ["Pull compensation", "Staggers", "Underlay spacing", "Underlay inset"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        // Satin: each underlay pass that is on shows its own settings.
        run(&mut app, "stitch.set", json!({"type": "satin", "settings": {"satin": {"underlays": [{"type": "zigZag", "spacing": 1.5}]}}}));
        let texts = frame(&mut app);
        for want in ["Fractional spacing", "Spacing", "1.5 mm", "Inset"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        assert!(!texts.iter().any(|t| t == "Length"), "{texts:?}");
        assert!(app.run("stitch.settingsMode", json!({"advanced": "yes"})).is_err());
        app.run("stitch.settingsMode", json!({"advanced": false})).unwrap();
        assert!(!frame(&mut app).iter().any(|t| t == "Fractional spacing"));
    }

    #[test]
    fn settings_fold_into_sections() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        run(&mut app, "file.new", json!({"width": 200, "height": 200}));
        run(&mut app, "path.create", json!({"d": "M 10 10 L 150 10"}));
        run(&mut app, "stitch.set", json!({"type": "satin"}));
        let texts = frame(&mut app);
        let at = |t: &str| texts.iter().position(|x| x == t).unwrap_or_else(|| panic!("{t} in {texts:?}"));
        // Each section's header comes before its rows, in panel order.
        assert!(at("Stitching") < at("Density") && at("Density") < at("Curves and Splits"));
        assert!(at("Curves and Splits") < at("Split satin over") && at("Split satin over") < at("Underlay"));
        assert!(at("Underlay") < at("Center walk"));
        // Compensation is all Advanced, so Basic leaves its header out.
        assert!(!texts.iter().any(|t| t == "Compensation"), "{texts:?}");
        app.run("stitch.settingsMode", json!({"advanced": true})).unwrap();
        let texts = frame(&mut app);
        let at = |t: &str| texts.iter().position(|x| x == t).unwrap_or_else(|| panic!("{t} in {texts:?}"));
        assert!(at("Compensation") < at("Pull compensation") && at("Pull compensation") < at("Curves and Splits"));
        assert_eq!((group_of("splitLength"), group_of("fractionalSpacing"), group_of("angle")), ("curves", "compensation", "stitches"));
    }

    #[test]
    fn a_closed_section_hides_its_rows() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        run(&mut app, "file.new", json!({"width": 200, "height": 200}));
        run(&mut app, "shape.rectangle", json!({"x": 0, "y": 0, "width": 60, "height": 40}));
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.data_mut(|d| d.insert_persisted(egui::Id::new(("stitch-settings-group", "fill", "stitches")), false));
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| show(&mut app, ui));
        out.textures_delta.clear();
        fn texts(s: &egui::Shape, out: &mut Vec<String>) {
            match s {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
                _ => {}
            }
        }
        let mut v = vec![];
        out.shapes.iter().for_each(|c| texts(&c.shape, &mut v));
        assert!(v.iter().any(|t| t == "Stitching") && v.iter().any(|t| t == "Sew underlay"), "{v:?}");
        assert!(!v.iter().any(|t| t == "Angle" || t == "Density"), "{v:?}");
    }

    #[test]
    fn e_stitch_and_rope_show_their_settings() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        run(&mut app, "file.new", json!({"width": 200, "height": 200}));
        run(&mut app, "path.create", json!({"d": "M 10 10 L 150 10"}));
        run(&mut app, "stitch.set", json!({"type": "eStitch"}));
        let texts = frame(&mut app);
        for want in ["E-Stitch", "Width", "2.5 mm", "Spacing"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        run(&mut app, "stitch.set", json!({"type": "rope"}));
        let texts = frame(&mut app);
        for want in ["Rope", "Passes", "2", "Density"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        assert!(!texts.iter().any(|t| t == "Slant"));
    }
}
