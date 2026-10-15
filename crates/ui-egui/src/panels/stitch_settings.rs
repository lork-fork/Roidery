//! Roidery: the Stitch Settings panel (Window → Stitch Settings), how the selected objects sew,
//! like Ember's object properties: the stitch type (Automatic follows the fill-or-stroke rule),
//! the type's settings in mm, the underlay and the stitch count. With nothing selected it shows
//! the design's totals. Everything reads `stitch.info` and acts through `stitch.set`.
//!
//! Basic shows what most designs need (type, width, density, angle, stitch length, underlay);
//! Advanced adds pull compensation, fractional spacing, staggers and each underlay pass's own
//! settings (`stitch.settingsMode`).

use egui::Ui;
use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::widgets;

pub const ID: &str = "stitchSettings";
const LABEL_W: f32 = 116.0;
const FIELD_W: f32 = 80.0;

/// The stitch types the menu offers, as (`stitch.set` type, label); "auto" resets to the rule.
pub(crate) fn types() -> [(&'static str, &'static str); 15] {
    [
        ("auto", tl!("Automatic")),
        ("tatami", tl!("Tatami")),
        ("contour", tl!("Contour Fill")),
        ("circular", tl!("Circular Fill")),
        ("gradient", tl!("Gradient Fill")),
        ("motif", tl!("Motif Fill")),
        ("guided", tl!("Guided Fill")),
        ("satin", tl!("Satin")),
        ("satinBlocks", tl!("Satin Blocks")),
        ("run", tl!("Single Run")),
        ("triple", tl!("Triple Run")),
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
    let whole = objects.iter().all(|o| o["whole"] == json!(true));
    for part in ["fill", "stroke"] {
        if part == "fill" && whole {
            continue;
        }
        let values: Vec<Value> = objects.iter().map(|o| o[part].clone()).collect();
        widgets::subheader(ui, if part == "fill" { tl!("Fill") } else { tl!("Stroke") });
        ui.push_id(part, |ui| part_section(app, ui, part, &values, advanced));
    }
    widgets::divider(ui);
    let stitches: u64 = objects.iter().filter_map(|o| o["stitches"].as_u64()).sum();
    widgets::dim_label(ui, &crate::i18n::fmt(tl!("{n} stitches"), &[("n", &stitches.to_string())]));
}

/// Whether a part's menu offers stitch type `id`: fills offer fill stitches, strokes line
/// stitches (Satin Blocks and manual stitches are the whole object, so they're in the stroke's).
pub(crate) fn allowed(part: &str, id: &str) -> bool {
    const FILLS: [&str; 6] = ["tatami", "contour", "circular", "gradient", "motif", "guided"];
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
    let types: Vec<(&str, &str)> = types().into_iter().filter(|t| allowed(part, t.0)).collect();
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
    if kind == "motif" {
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
    for (label, section, key, suffix, decimals, adv) in fields(kind) {
        if adv && !advanced {
            continue;
        }
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
    if kind == "motif" {
        let on = shared(&settings, &["motif", "triple"]).and_then(Value::as_bool);
        widgets::label_row(ui, tl!("Triple Run"), LABEL_W, |ui| {
            if widgets::check3(ui, "", on, true) {
                set_part(app, part, json!({"settings": {"motif": {"triple": on != Some(true)}}}));
            }
        });
    }
    // Underlay.
    match kind {
        "tatami" | "contour" | "circular" | "guided" => {
            let on = shared(&settings, &["fill", "underlay"]).and_then(Value::as_bool);
            widgets::label_row(ui, tl!("Underlay"), LABEL_W, |ui| {
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
        "satin" | "satinBlocks" => {
            let lists = shared(&settings, &["satin", "underlays"]).and_then(Value::as_array).cloned();
            let has = |ty: &str| lists.as_ref().map(|l| l.iter().any(|u| u["type"] == json!(ty)));
            let mut toggled = None;
            let mut changed: Option<(&str, &str, f64)> = None;
            for (i, (label, ty)) in satin_underlays().into_iter().enumerate() {
                widgets::label_row(ui, if i == 0 { tl!("Underlay") } else { "" }, LABEL_W, |ui| {
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
    }
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
        for want in ["Satin", "Width", "Center walk", "Edge walk", "Zigzag"] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
        assert!(!texts.iter().any(|t| t == "Angle"));
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
        for want in ["Text", "Abc", "Letter height", "10 mm", "Satin"] {
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
        for want in ["Satin", "Width", "Angle"] {
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
