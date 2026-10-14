//! Roidery: the embroidery bar, across the top of the Embroidery workspace in place of the
//! Control bar (Window › Embroidery Bar). Its left side follows the selection: what is selected
//! and the likeliest next steps for it (a fill's stitch type, angle and density and Convert to
//! Satin; a line's stitch and width; lettering's text, font and height; for several objects their
//! stitch type, alignment and sewing order). Its right side is always there: the stitch views, the
//! view toggles (snapping, grid, guides, rulers, outline and the stitch overlays), the hoop and
//! how the design fits it, the design's stitch count and sew time, the sew-out player and Export.
//!
//! Like the panels, it reads `stitch.info` and acts through commands.

use egui::{Align, CornerRadius, Layout, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::{Value, json};
use vectorcraft_doc::{NodeId, NodeKind};

use crate::VectorcraftApp;
use crate::panels::stitch_settings as ss;
use crate::theme::{self, Tokens};
use crate::widgets;

/// The bar's height, points.
pub const HEIGHT: f32 = 40.0;
const BUTTON_H: f32 = 26.0;
const ICON: f32 = 26.0;
const FIELD_W: f32 = 62.0;
/// Satin Blocks wider than this many mm get the warning chip (as Convert to Satin warns).
const WIDE_SATIN_MM: f64 = 10.0;

/// What the bar shows on its left, by what is selected.
#[derive(Clone, Debug, PartialEq)]
pub enum Context {
    /// Nothing selected: the design.
    Design,
    /// A closed shape: its fill (and border).
    Fill,
    /// An open path: its line stitch.
    Line,
    /// Satin Blocks.
    Satin,
    /// Lettering (with an open path selected too: `path`).
    Lettering { path: Option<u64> },
    /// Text in a computer font.
    Text,
    /// Stitches opened from a machine file (or set to Manual).
    Manual,
    /// A group (not lettering).
    Group,
    /// Several objects.
    Multi,
    /// Something that doesn't sew (an image, a guide).
    Other,
}

/// The bar's context for the selection, from `stitch.info` (`info`).
pub fn context(app: &VectorcraftApp, info: &Value) -> Context {
    let Some(st) = app.session.active() else { return Context::Design };
    let sel = &st.selection.objects;
    if sel.is_empty() {
        return Context::Design;
    }
    let letterings: Vec<u64> = info["lettering"].as_array().into_iter().flatten().filter_map(|l| l["id"].as_u64()).collect();
    let open_path =
        |id: &NodeId| st.doc.node(*id).is_some_and(|n| matches!(n.kind, NodeKind::Path { .. }) && n.path_data().is_some_and(|p| !p.is_closed()));
    if !letterings.is_empty() {
        let others: Vec<&NodeId> = sel.iter().filter(|id| !letterings.contains(&id.0)).collect();
        match others.as_slice() {
            [] => return Context::Lettering { path: None },
            [p] if open_path(p) => return Context::Lettering { path: Some(p.0) },
            _ => {}
        }
        // A letter or piece picked inside lettering: how it sews.
        if sel.len() == 1 && st.doc.node(sel[0]).is_some_and(|n| !matches!(n.kind, NodeKind::Group { .. })) {
            return single(app, info, sel[0]);
        }
        return Context::Multi;
    }
    if sel.len() > 1 {
        return Context::Multi;
    }
    single(app, info, sel[0])
}

fn single(app: &VectorcraftApp, info: &Value, id: NodeId) -> Context {
    let Some(st) = app.session.active() else { return Context::Design };
    let Some(n) = st.doc.node(id) else { return Context::Other };
    let row = info["objects"].as_array().and_then(|o| o.iter().find(|r| r["id"].as_u64() == Some(id.0)));
    match &n.kind {
        NodeKind::Group { .. } => return Context::Group,
        NodeKind::Text(_) => return Context::Text,
        _ => {}
    }
    let Some(row) = row else { return Context::Other };
    match row["type"].as_str() {
        Some("satinBlocks") => Context::Satin,
        Some("manual") => Context::Manual,
        _ if n.path_data().is_some_and(|p| !p.is_closed()) => Context::Line,
        _ => Context::Fill,
    }
}

/// The bar: the always-there right side first (it keeps its room), then the selection's side
/// in what is left, clipped when the window is narrow.
pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("embroidery_bar")
        .exact_size(HEIGHT)
        .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(10, 0)).stroke(Stroke::new(1.0, t.border)))
        .show(ui, |ui| {
            let full = ui.max_rect();
            let mut rui = ui.new_child(egui::UiBuilder::new().max_rect(full).layout(Layout::right_to_left(Align::Center)));
            rui.spacing_mut().item_spacing.x = 4.0;
            right_side(app, &mut rui);
            let right_start = rui.min_rect().left();
            let left = Rect::from_min_max(full.min, pos2((right_start - 14.0).max(full.left()), full.bottom()));
            let mut lui = ui.new_child(egui::UiBuilder::new().max_rect(left).layout(Layout::left_to_right(Align::Center)));
            lui.set_clip_rect(left.intersect(ui.clip_rect()));
            lui.spacing_mut().item_spacing.x = 6.0;
            left_side(app, &mut lui);
        });
}

// ---------------------------------------------------------------------------------------------
// The right side.

/// `stitch.info` for the whole design (whatever is selected), recomputed when the document
/// changes.
fn design_totals(app: &mut VectorcraftApp, ctx: &egui::Context) -> Option<Value> {
    type Key = (u64, u64);
    let st = app.session.active()?;
    let key: Key = (st.uid, st.revision);
    let id = egui::Id::new("embroidery-bar-totals");
    if let Some((k, v)) = ctx.data(|d| d.get_temp::<(Key, Value)>(id))
        && k == key
    {
        return Some(v);
    }
    let v = app.session.execute("stitch.info", &json!({"ids": []})).ok()?;
    ctx.data_mut(|d| d.insert_temp(id, (key, v.clone())));
    Some(v)
}

/// A count with thousands separated, as `12,345`.
fn grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A thin vertical divider.
fn divider(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(9.0, 22.0), Sense::hover());
    ui.painter().line_segment([r.center_top(), r.center_bottom()], Stroke::new(1.0, t.divider));
}

/// A toggle for a UI command: lit while on, runs `cmd` when clicked.
fn toggle(app: &mut VectorcraftApp, ui: &mut Ui, icon: &str, tip: &str, cmd: &str, on: bool) {
    if widgets::icon_button(ui, icon, tip, on, ICON).clicked() {
        crate::menus::invoke(app, cmd, json!({}));
    }
}

fn right_side(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let has_doc = app.session.active().is_some();
    // Right to left: Export, the sew-out player, the totals, the hoop, the toggles, the views.
    if text_button(ui, Some("save"), tl!("Export"), tl!("Export Machine File…")).clicked() && has_doc {
        crate::menus::invoke(app, "file.exportMachine", json!({}));
    }
    let sewing = app.ui.sew_out.is_some();
    if widgets::icon_button(ui, if sewing { "pause" } else { "play" }, tl!("Sew-Out Player"), sewing, ICON).clicked() && has_doc {
        crate::menus::invoke(app, "view.sewOut", json!({}));
    }
    let totals = if has_doc { design_totals(app, ui.ctx()) } else { None };
    if let Some(info) = &totals {
        let stitches = info["stitches"].as_u64().unwrap_or(0);
        let text = if stitches == 0 {
            tl!("No stitches yet").to_string()
        } else {
            format!(
                "{} · {}",
                crate::i18n::fmt(tl!("{n} st"), &[("n", &grouped(stitches))]),
                ss::sew_time(info["sewMinutes"].as_f64().unwrap_or(0.0))
            )
        };
        let colors = info["colorChanges"].as_u64().unwrap_or(0) + u64::from(stitches > 0);
        let tip = crate::i18n::fmt(
            tl!("{n} stitches, {c} colors, {t} trims"),
            &[("n", &grouped(stitches)), ("c", &colors.to_string()), ("t", &info["trims"].as_u64().unwrap_or(0).to_string())],
        );
        ui.label(egui::RichText::new(text).size(12.0).color(t.text)).on_hover_text(tip);
    }
    if has_doc {
        hoop_chip(app, ui, totals.as_ref());
    }
    divider(ui);
    // The toggles, rightmost first: the stitch overlays, then the drawing aids.
    let v = app.ui.view.clone();
    toggle(app, ui, "rd-hoop", tl!("Show Hoop"), "view.hoop", v.hoop);
    solo_toggle(app, ui);
    toggle(app, ui, "rd-underlay", tl!("Show Underlay"), "view.underlay", !v.hide_underlay);
    toggle(app, ui, "rd-needle-points", tl!("Show Needle Points"), "view.needlePoints", v.needle_points);
    toggle(app, ui, "rd-jumps", tl!("Show Jumps and Trims"), "view.jumps", v.jumps);
    toggle(app, ui, "rd-density", tl!("Density Map"), "view.densityMap", v.density);
    divider(ui);
    toggle(app, ui, "square", tl!("Outline"), "view.outline", v.outline);
    toggle(app, ui, "ruler", tl!("Rulers"), "view.rulers", v.rulers);
    toggle(app, ui, "rd-guides", tl!("Guides"), "view.guides", v.guides);
    toggle(app, ui, "grid-3x3", tl!("Grid"), "view.grid", v.grid);
    snap_toggle(app, ui);
    divider(ui);
    // The views, as a segmented control (right to left: Realistic, Stitches, Vector).
    for (i, icon, tip) in [(2u8, "spool", tl!("Realistic View")), (1, "spline", tl!("Stitch View")), (0, "pen-tool", tl!("Vector View"))] {
        if widgets::icon_button(ui, icon, tip, v.stitches == i, ICON).clicked() {
            let view = crate::stitchview::VIEWS.get(usize::from(i)).copied().unwrap_or("vector");
            crate::menus::invoke(app, "view.stitches", json!({ "view": view }));
        }
    }
}

/// Snapping: one button that turns snapping off and back on (to what it snapped to before), and
/// a chevron for what it snaps to.
fn snap_toggle(app: &mut VectorcraftApp, ui: &mut Ui) {
    let v = &app.ui.view;
    let on = v.snap_to_grid || v.snap_to_point || v.smart_guides;
    let (r, resp) = ui.allocate_exact_size(vec2(14.0, ICON), Sense::click());
    let t = Tokens::get(ui.ctx());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(3), t.hover);
    }
    crate::icons::paint(ui, "chevron-down", Rect::from_center_size(r.center(), vec2(10.0, 10.0)), t.text_dim);
    let resp = resp.on_hover_text(tl!("Snap to…"));
    egui::Popup::menu(&resp).show(|ui| {
        for (label, cmd, on) in [
            (tl!("Snap to Grid"), "view.snapToGrid", app.ui.view.snap_to_grid),
            (tl!("Snap to Point"), "view.snapToPoint", app.ui.view.snap_to_point),
            (tl!("Smart Guides"), "view.smartGuides", app.ui.view.smart_guides),
        ] {
            if widgets::menu_item_name(ui, label, true, on) {
                crate::menus::invoke(app, cmd, json!({}));
            }
        }
    });
    if widgets::icon_button(ui, "rd-snap", tl!("Snapping"), on, ICON).clicked() {
        let id = egui::Id::new("embroidery-bar-snap");
        let v = &mut app.ui.view;
        if on {
            ui.ctx().data_mut(|d| d.insert_temp(id, (v.snap_to_grid, v.snap_to_point, v.smart_guides)));
            (v.snap_to_grid, v.snap_to_point, v.smart_guides) = (false, false, false);
        } else {
            // Back to what it snapped to, or to points and smart guides.
            (v.snap_to_grid, v.snap_to_point, v.smart_guides) =
                ui.ctx().data(|d| d.get_temp::<(bool, bool, bool)>(id)).filter(|s| s.0 || s.1 || s.2).unwrap_or((false, true, true));
        }
    }
}

/// Current Color Only: the toggle, and while it is on, the colour shown, which picks another.
fn solo_toggle(app: &mut VectorcraftApp, ui: &mut Ui) {
    if let Some(c) = app.ui.view.solo {
        let resp = widgets::chip_button(ui, 20.0, false, |ui, r| {
            ui.painter().rect_filled(r, 0.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
        })
        .on_hover_text(tl!("Color shown"));
        let threads: Vec<[u8; 3]> = crate::panels::threads::design(app, ui.ctx())
            .and_then(|q| q["threads"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|th| crate::stitchview::thread_rgb(&th["rgb"]))
            .collect();
        egui::Popup::menu(&resp).show(|ui| {
            for rgb in threads {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    ui.painter().rect_filled(r, CornerRadius::same(2), egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
                    if widgets::menu_item_name(ui, &crate::stitchview::hex(rgb), true, rgb == c) {
                        crate::menus::invoke(app, "view.soloColor", json!({ "color": crate::stitchview::hex(rgb) }));
                    }
                });
            }
        });
    }
    let on = app.ui.view.solo.is_some();
    if widgets::icon_button(ui, "rd-solo-color", tl!("Current Color Only"), on, ICON).clicked() {
        crate::menus::invoke(app, "view.soloColor", json!({}));
    }
}

/// The hoop: its name with how the design fits (green fits, amber turned, red too large), and a
/// popover to choose the machine, hoop, fabric and thread chart.
fn hoop_chip(app: &mut VectorcraftApp, ui: &mut Ui, totals: Option<&Value>) {
    let t = Tokens::get(ui.ctx());
    let Some(q) = crate::panels::threads::design(app, ui.ctx()) else { return };
    let fit = totals.and_then(|i| i["hoop"]["fit"].as_str()).unwrap_or("");
    let name = q["hoop"]["name"].as_str().map(str::to_string);
    let (text, color, mark) = match (&name, fit) {
        (None, _) => (tl!("No hoop").to_string(), t.text_dim, ""),
        (Some(n), "fits") => (n.clone(), t.text, "✓"),
        (Some(n), "rotate") => (n.clone(), t.warning, "↻"),
        (Some(n), "tooLarge") => (n.clone(), t.error, "!"),
        (Some(n), _) => (n.clone(), t.text, ""),
    };
    let label = if mark.is_empty() { text } else { format!("{mark} {text}") };
    let galley = ui.painter().layout_no_wrap(label, egui::FontId::proportional(12.0), color);
    let w = (galley.size().x + 34.0).min(200.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, BUTTON_H), Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(4), if resp.hovered() { t.hover } else { t.input });
    ui.painter().rect_stroke(r, CornerRadius::same(4), Stroke::new(1.0, t.button_border), StrokeKind::Inside);
    crate::icons::paint(ui, "rd-hoop", Rect::from_center_size(r.left_center() + vec2(12.0, 0.0), vec2(14.0, 14.0)), t.icon);
    ui.painter().with_clip_rect(r.shrink(2.0)).galley(pos2(r.left() + 24.0, r.center().y - galley.size().y / 2.0), galley, color);
    let tip = match fit {
        "fits" => tl!("The design fits the hoop"),
        "rotate" => tl!("The design fits the hoop turned 90°"),
        "tooLarge" => tl!("The design is too large for the hoop"),
        _ => tl!("Hoop, fabric and thread chart"),
    };
    let resp = resp.on_hover_text(tip);
    widgets::popover(&resp, resp.clicked(), |ui| {
        ui.set_min_width(300.0);
        widgets::subheader(ui, tl!("Design"));
        crate::panels::threads::hoop_rows(app, ui, &q, totals.and_then(|i| i.get("hoop")), 96.0);
        crate::panels::threads::fabric_row(app, ui, &q, 96.0);
        crate::panels::threads::chart_row(app, ui, &q);
        ui.add_space(4.0);
        if widgets::flat_button(ui, tl!("Center in Hoop"), 120.0).clicked() {
            run(app, "stitch.centerInHoop", json!({}));
        }
    });
}

// ---------------------------------------------------------------------------------------------
// The left side.

/// A bordered button with an optional icon and a label, sized to it.
fn text_button(ui: &mut Ui, icon: Option<&str>, label: &str, tip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let galley = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.5), t.text_strong);
    let pad = if icon.is_some() { 30.0 } else { 18.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + pad, BUTTON_H), Sense::click());
    if resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(r, CornerRadius::same(3), t.tool_active);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(3), t.hover);
    }
    ui.painter().rect_stroke(r, CornerRadius::same(3), Stroke::new(1.0, t.button_border), StrokeKind::Inside);
    let x = match icon {
        Some(i) => {
            crate::icons::paint(ui, i, Rect::from_min_size(r.min + vec2(7.0, 5.0), vec2(16.0, 16.0)), t.icon);
            r.left() + 26.0
        }
        None => r.left() + 9.0,
    };
    ui.painter().galley(pos2(x, r.center().y - galley.size().y / 2.0), galley, t.text_strong);
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

/// A dim label before a field.
fn small_label(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    widgets::field_label(ui, egui::RichText::new(text).size(11.5).color(t.text_dim));
}

/// Run a command, reporting a failure in the status bar.
fn run(app: &mut VectorcraftApp, cmd: &str, p: Value) -> Option<Value> {
    match app.run(cmd, p) {
        Ok(v) => Some(v),
        Err(e) => {
            app.status(e);
            None
        }
    }
}

/// Run a menu or UI command (tools, dialogs, panels).
fn invoke(app: &mut VectorcraftApp, cmd: &str, p: Value) {
    crate::menus::invoke(app, cmd, p);
}

fn left_side(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    if app.session.active().is_none() {
        ui.label(egui::RichText::new(tl!("No Document")).color(t.text_dim));
        return;
    }
    let Some(info) = ss::info(app, ui.ctx()) else { return };
    let cx = context(app, &info);
    let objects: Vec<Value> = info["objects"].as_array().cloned().unwrap_or_default();
    let n = app.session.active().map_or(0, |st| st.selection.objects.len());
    let title = match &cx {
        Context::Design => tl!("Design").to_string(),
        Context::Fill => fill_title(&objects),
        Context::Line => line_title(&objects),
        Context::Satin => tl!("Satin Blocks").to_string(),
        Context::Lettering { .. } => tl!("Lettering").to_string(),
        Context::Text => tl!("Text").to_string(),
        Context::Manual => tl!("Stitches").to_string(),
        Context::Group => tl!("Group").to_string(),
        Context::Multi => crate::i18n::fmt(tl!("{n} objects"), &[("n", &n.to_string())]),
        Context::Other => tl!("Not stitched").to_string(),
    };
    ui.label(egui::RichText::new(title).font(theme::semibold(12.5)).color(t.text));
    ui.add_space(4.0);
    match cx {
        Context::Design => design(app, ui),
        Context::Fill => fill(app, ui, &objects),
        Context::Line => line(app, ui, &objects),
        Context::Satin => satin(app, ui, &objects),
        Context::Lettering { path } => lettering(app, ui, &info, path),
        Context::Text => text(app, ui),
        Context::Manual => manual(app, ui, &objects),
        Context::Group => group(app, ui, &objects),
        Context::Multi => multi(app, ui, &info, &objects),
        Context::Other => {
            widgets::dim_label(ui, tl!("Nothing selected sews: paths, compound paths and text do."));
        }
    }
}

/// A stitch type's label, as the Stitch Settings panel names it.
fn type_label(id: &str) -> &'static str {
    ss::types().into_iter().find(|t| t.0 == id).map_or("", |t| t.1)
}

fn fill_title(objects: &[Value]) -> String {
    match objects.first().map(|o| o["fill"]["type"].as_str().unwrap_or("none")) {
        Some("none") | None => tl!("Shape").to_string(),
        Some(k) => type_label(k).to_string(),
    }
}

fn line_title(objects: &[Value]) -> String {
    match objects.first().map(|o| o["stroke"]["type"].as_str().unwrap_or("none")) {
        Some("none") | None => tl!("Path").to_string(),
        Some(k) => type_label(k).to_string(),
    }
}

/// A stitch type dropdown for `part` (`None`: the whole object, every type), showing the type the
/// objects share; returns the `stitch.set` params chosen.
fn type_dropdown(ui: &mut Ui, id: &str, part: Option<&str>, objects: &[Value], width: f32) -> Option<Value> {
    let parts: Vec<Value> = objects.iter().map(|o| part.map_or_else(|| o.clone(), |p| o[p].clone())).collect();
    let all_auto = parts.iter().all(|o| o["auto"] == json!(true));
    let kind = ss::shared(&parts, &["type"]).and_then(Value::as_str).unwrap_or("");
    let current = if part.is_some() && all_auto {
        "auto"
    } else if part.is_some() && parts.iter().any(|o| o["auto"] == json!(true)) {
        ""
    } else {
        kind
    };
    let types: Vec<(&str, &str)> = ss::types().into_iter().filter(|t| part.is_none_or(|p| ss::allowed(p, t.0))).collect();
    let labels: Vec<&str> = types.iter().map(|t| t.1).collect();
    let shown = types.iter().find(|t| t.0 == current).map_or(tl!("Mixed"), |t| t.1);
    let i = widgets::dropdown(ui, id, shown, &labels, width)?;
    let (ty, _) = types.get(i)?;
    let mut p = if *ty == "auto" { json!({"reset": true}) } else { json!({ "type": ty }) };
    if let Some(part) = part {
        p["part"] = json!(part);
    }
    Some(p)
}

/// A number field for one setting of `part` (`section.key`, as Stitch Settings names it).
fn setting_field(app: &mut VectorcraftApp, ui: &mut Ui, part: &str, parts: &[Value], field: ss::Field) {
    let (label, section, key, suffix, decimals, _) = field;
    let settings: Vec<Value> = parts.iter().map(|o| o["settings"].clone()).collect();
    let value = ss::shared(&settings, &[section, key]).and_then(Value::as_f64);
    small_label(ui, label);
    if let Some(v) = widgets::mixed_field(ui, ("bar", part, section, key), value, suffix, decimals, FIELD_W).filter(|v| v.is_finite()) {
        let v = if decimals == 0 { json!(v.round().clamp(1.0, 64.0) as u32) } else { json!(v) };
        run(app, "stitch.set", json!({ "part": part, "settings": { section: { key: v } } }));
    }
}

/// The settings of a part's stitch type the bar shows (`fields` of the Stitch Settings panel that
/// most designs change), in order.
fn bar_fields(kind: &str) -> Vec<ss::Field> {
    let keep = |f: &ss::Field| !f.5 && !matches!(f.2, "stitchLength" | "endSpacing" | "height" | "tolerance");
    let mut v: Vec<ss::Field> = ss::fields(kind).into_iter().filter(keep).collect();
    // The angle has its own field with the Fill Angle tool.
    v.retain(|f| f.2 != "angle");
    v
}

fn design(app: &mut VectorcraftApp, ui: &mut Ui) {
    if text_button(ui, Some("type"), tl!("Add Lettering"), tl!("Add Lettering…")).clicked() {
        invoke(app, "type.addLettering", json!({}));
    }
    if text_button(ui, Some("image"), tl!("Place Artwork"), tl!("Place an image or SVG to trace or digitize")).clicked() {
        invoke(app, "file.place", json!({}));
    }
    if let Some(q) = crate::panels::threads::design(app, ui.ctx()) {
        let ids: Vec<String> = q["fabrics"].as_array().into_iter().flatten().filter_map(|f| f["id"].as_str().map(str::to_string)).collect();
        let labels: Vec<&str> = ids.iter().map(|i| crate::panels::threads::fabric_label(i)).collect();
        let current = crate::panels::threads::fabric_label(q["fabric"].as_str().unwrap_or("woven"));
        small_label(ui, tl!("Fabric"));
        if let Some(id) = widgets::dropdown(ui, "bar-fabric", current, &labels, 170.0).and_then(|i| ids.get(i)) {
            run(app, "stitch.setDesign", json!({ "fabric": id }));
        }
    }
    if widgets::icon_button(ui, "arrow-down-wide-narrow", tl!("Sort by Color"), false, ICON).clicked() {
        crate::panels::sequencer::sort(app);
    }
    if widgets::icon_button(ui, "swatch-book", tl!("Match to Thread Chart"), false, ICON).clicked() {
        crate::panels::threads::match_threads(app);
    }
    if text_button(ui, Some("move"), tl!("Center in Hoop"), "").clicked() {
        run(app, "stitch.centerInHoop", json!({}));
    }
}

/// Convert to Satin, with a chevron for the classic way (kept side by side while they are
/// compared).
fn convert_to_satin(app: &mut VectorcraftApp, ui: &mut Ui) {
    if text_button(ui, Some("rd-satin-blocks"), tl!("Convert to Satin"), tl!("Convert to Satin (Centerline)")).clicked() {
        convert(app, "stitch.convertToSatin");
    }
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(16.0, BUTTON_H), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(3), t.hover);
    }
    crate::icons::paint(ui, "chevron-down", Rect::from_center_size(r.center(), vec2(10.0, 10.0)), t.text_dim);
    egui::Popup::menu(&resp).show(|ui| {
        if widgets::menu_item(ui, "Convert to Satin (Centerline)", true, false) {
            convert(app, "stitch.convertToSatin");
        }
        if widgets::menu_item(ui, "Convert to Satin (Classic)", true, false) {
            convert(app, "stitch.convertToSatinClassic");
        }
    });
}

fn convert(app: &mut VectorcraftApp, cmd: &str) {
    if let Some(r) = run(app, cmd, json!({}))
        && let Some(w) = r["warning"].as_str()
    {
        app.status(w.to_string());
    }
}

/// The fill angle: a field, and the Fill Angle tool to drag it.
fn angle_field(app: &mut VectorcraftApp, ui: &mut Ui, settings: &[Value], kind: &str) {
    let (section, key) = if kind == "motif" { ("motif", "angle") } else { ("fill", "angle") };
    let value = ss::shared(settings, &[section, key]).and_then(Value::as_f64);
    small_label(ui, tl!("Angle"));
    if let Some(a) = widgets::mixed_field(ui, ("bar", "angle"), value, "°", 1, 54.0).filter(|v| v.is_finite()) {
        run(app, "stitch.setAngle", json!({ "angle": a }));
    }
    let active = app.session.tool_id() == "fillAngle";
    if widgets::icon_button(ui, "rd-fill-angle", tl!("Fill Angle tool: drag across the shape"), active, ICON).clicked() {
        app.select_tool("fillAngle");
    }
}

/// The fill's underlay, as a checkbox.
fn fill_underlay(app: &mut VectorcraftApp, ui: &mut Ui, settings: &[Value]) {
    let on = ss::shared(settings, &["fill", "underlay"]).and_then(Value::as_bool);
    if widgets::check3(ui, tl!("Underlay"), on, true) {
        run(app, "stitch.set", json!({"part": "fill", "settings": {"fill": {"underlay": on != Some(true)}}}));
    }
}

/// A thread colour chip for the fill or the stroke (opens Swatches, as the Control bar's does).
fn color_chip(app: &mut VectorcraftApp, ui: &mut Ui, stroke: bool) {
    crate::panels::paint_chip(app, ui, stroke, 22.0, false);
}

/// The ⋯ menu: Stitch Settings and the less common actions for the selection.
fn more_menu(app: &mut VectorcraftApp, ui: &mut Ui, items: &[(&str, &str)]) {
    let resp = widgets::icon_button(ui, "ellipsis", tl!("More"), false, ICON);
    egui::Popup::menu(&resp).show(|ui| {
        if widgets::menu_item(ui, "Stitch Settings…", true, false) {
            invoke(app, "window.panel", json!({"panel": ss::ID}));
        }
        for (label, cmd) in items {
            if widgets::menu_item(ui, label, true, false) {
                invoke(app, cmd, json!({}));
            }
        }
    });
}

fn fill(app: &mut VectorcraftApp, ui: &mut Ui, objects: &[Value]) {
    let fills: Vec<Value> = objects.iter().map(|o| o["fill"].clone()).collect();
    small_label(ui, tl!("Fill"));
    if let Some(p) = type_dropdown(ui, "bar-fill-type", Some("fill"), objects, 118.0) {
        run(app, "stitch.set", p);
        return;
    }
    convert_to_satin(app, ui);
    let kind = ss::shared(&fills, &["type"]).and_then(Value::as_str).unwrap_or("");
    let sews = fills.iter().any(|o| o["sews"] == json!(true));
    let settings: Vec<Value> = fills.iter().map(|o| o["settings"].clone()).collect();
    if sews && !kind.is_empty() {
        if matches!(kind, "tatami" | "gradient" | "motif") {
            angle_field(app, ui, &settings, kind);
        }
        if kind == "motif" {
            let patterns = ss::motif_patterns();
            let current = ss::shared(&settings, &["motif", "pattern"]).and_then(Value::as_str).unwrap_or("");
            let labels: Vec<&str> = patterns.iter().map(|p| p.1).collect();
            let shown = patterns.iter().find(|p| p.0 == current).map_or("", |p| p.1);
            if let Some((id, _)) = widgets::dropdown(ui, "bar-motif", shown, &labels, 96.0).and_then(|i| patterns.get(i)) {
                run(app, "stitch.set", json!({"part": "fill", "settings": {"motif": {"pattern": id}}}));
            }
        }
        for field in bar_fields(kind) {
            setting_field(app, ui, "fill", &fills, field);
        }
        if matches!(kind, "tatami" | "contour" | "circular" | "guided") {
            fill_underlay(app, ui, &settings);
        }
    }
    if sews {
        color_chip(app, ui, false);
    }
    // The border: the stroke part's line stitch.
    small_label(ui, tl!("Border"));
    if let Some(p) = type_dropdown(ui, "bar-border-type", Some("stroke"), objects, 104.0) {
        run(app, "stitch.set", p);
    }
    if objects.iter().any(|o| o["stroke"]["sews"] == json!(true)) {
        color_chip(app, ui, true);
    }
    if widgets::icon_button(ui, "squares-subtract", tl!("Remove Overlaps: cut fills where later ones cover them"), false, ICON).clicked() {
        run(app, "stitch.removeOverlaps", json!({}));
    }
    more_menu(app, ui, &[("Reverse Sewing Direction", "path.reverse"), ("Match to Thread Chart", "stitch.matchThreads")]);
}

fn line(app: &mut VectorcraftApp, ui: &mut Ui, objects: &[Value]) {
    let strokes: Vec<Value> = objects.iter().map(|o| o["stroke"].clone()).collect();
    small_label(ui, tl!("Stitch"));
    if let Some(p) = type_dropdown(ui, "bar-line-type", Some("stroke"), objects, 118.0) {
        run(app, "stitch.set", p);
        return;
    }
    let kind = ss::shared(&strokes, &["type"]).and_then(Value::as_str).unwrap_or("");
    if strokes.iter().any(|o| o["sews"] == json!(true)) && !kind.is_empty() {
        for field in bar_fields(kind) {
            setting_field(app, ui, "stroke", &strokes, field);
        }
        color_chip(app, ui, true);
    }
    if widgets::icon_button(ui, "dc-reverse", tl!("Reverse Sewing Direction"), false, ICON).clicked() {
        run(app, "path.reverse", json!({}));
    }
    more_menu(app, ui, &[("Match to Thread Chart", "stitch.matchThreads")]);
}

/// The widest rung of the selected Satin Blocks, mm.
fn widest_rung_mm(app: &VectorcraftApp) -> Option<f64> {
    let st = app.session.active()?;
    let mut widest: Option<f64> = None;
    for id in &st.selection.objects {
        let Some(path) = st.doc.node(*id).and_then(|n| n.path_data()) else { continue };
        let (Some(l), Some(r)) = (path.subpaths.first(), path.subpaths.get(1)) else { continue };
        for (a, b) in l.anchors.iter().zip(&r.anchors) {
            let w = a.p.distance(b.p) * 25.4 / 72.0;
            widest = Some(widest.map_or(w, |x: f64| x.max(w)));
        }
    }
    widest
}

fn satin(app: &mut VectorcraftApp, ui: &mut Ui, objects: &[Value]) {
    let t = Tokens::get(ui.ctx());
    let strokes: Vec<Value> = objects.iter().map(|o| o["stroke"].clone()).collect();
    let settings: Vec<Value> = strokes.iter().map(|o| o["settings"].clone()).collect();
    setting_field(app, ui, "stroke", &strokes, (tl!("Density"), "satin", "density", " mm", 2, false));
    setting_field(app, ui, "stroke", &strokes, (tl!("Pull comp."), "satin", "pullCompensation", " mm", 2, false));
    // The underlay passes, in a menu.
    let lists = ss::shared(&settings, &["satin", "underlays"]).and_then(Value::as_array).cloned();
    let count = lists.as_ref().map_or(0, Vec::len);
    let label = if count == 0 { tl!("No underlay").to_string() } else { crate::i18n::fmt(tl!("Underlay: {n}"), &[("n", &count.to_string())]) };
    let resp = text_button(ui, Some("rd-underlay"), &label, tl!("Underlay passes"));
    egui::Popup::menu(&resp).show(|ui| {
        for (label, ty) in ss::satin_underlays() {
            let on = lists.as_ref().is_some_and(|l| l.iter().any(|u| u["type"] == json!(ty)));
            if widgets::menu_item_name(ui, label, true, on) {
                let current = lists.clone().unwrap_or_default();
                let next: Vec<Value> = ss::satin_underlays()
                    .iter()
                    .filter_map(
                        |(_, x)| if *x == ty { (!on).then(|| json!({"type": x})) } else { current.iter().find(|u| u["type"] == json!(x)).cloned() },
                    )
                    .collect();
                run(app, "stitch.set", json!({"part": "stroke", "settings": {"satin": {"underlays": next}}}));
            }
        }
    });
    let editing = app.session.tool_id() == "satinBlocks";
    if text_button(ui, Some("rd-satin-blocks"), tl!("Edit Rungs"), tl!("Drag a rung's end, click a rail to add one, Alt-click one to delete it"))
        .clicked()
    {
        app.select_tool(if editing { "selection" } else { "satinBlocks" });
    }
    if widgets::icon_button(ui, "arrow-left-right", tl!("Swap Satin Sides"), false, ICON).clicked() {
        run(app, "stitch.swapSides", json!({}));
    }
    if widgets::icon_button(ui, "dc-reverse", tl!("Reverse Sewing Direction"), false, ICON).clicked() {
        run(app, "path.reverse", json!({}));
    }
    color_chip(app, ui, true);
    if let Some(w) = widest_rung_mm(app).filter(|w| *w > WIDE_SATIN_MM) {
        let text = crate::i18n::fmt(tl!("{w} mm wide"), &[("w", &format!("{w:.1}"))]);
        ui.label(egui::RichText::new(format!("⚠ {text}")).size(12.0).color(t.warning))
            .on_hover_text(tl!("Satin this wide can snag and pull: split the column or sew it as a fill"));
    }
    more_menu(app, ui, &[("Match to Thread Chart", "stitch.matchThreads")]);
}

fn lettering(app: &mut VectorcraftApp, ui: &mut Ui, info: &Value, path: Option<u64>) {
    let t = Tokens::get(ui.ctx());
    let rows: Vec<Value> = info["lettering"].as_array().cloned().unwrap_or_default();
    let ls: Vec<Value> = rows.iter().map(|r| r["lettering"].clone()).collect();
    let text = ss::shared(&ls, &["text"]).and_then(Value::as_str).map(str::to_string);
    if let Some(s) = widgets::text_field(ui, "bar-lettering-text", text.as_deref(), 170.0, 1).filter(|s| !s.is_empty()) {
        ss::lettering_set(app, json!({ "text": s }));
    }
    // The font: its name, which opens the font window to change it.
    let font = ss::shared(&rows, &["fontName"]).and_then(Value::as_str).unwrap_or(tl!("Mixed")).to_string();
    if text_button(ui, Some("type"), &font, tl!("Change the font…")).clicked() {
        let ids: Vec<u64> = rows.iter().filter_map(|r| r["id"].as_u64()).collect();
        let font = ss::shared(&ls, &["font"]).and_then(Value::as_str).unwrap_or("sans").to_string();
        let height = ss::shared(&ls, &["height"]).and_then(Value::as_f64).unwrap_or(10.0);
        crate::dialogs::lettering::open_change(app, &ids, &font, height);
    }
    // The height, amber outside the sizes the font sews well at.
    let height = ss::shared(&ls, &["height"]).and_then(Value::as_f64);
    let range = (ss::shared(&rows, &["sizeMin"]).and_then(Value::as_f64), ss::shared(&rows, &["sizeMax"]).and_then(Value::as_f64));
    let off = match (height, range) {
        (Some(h), (Some(lo), Some(hi))) => h < lo * 0.95 - 0.05 || h > hi * 1.05 + 0.05,
        _ => false,
    };
    let hint = match range {
        (Some(lo), Some(hi)) => crate::dialogs::lettering::size_hint(lo, hi),
        _ => String::new(),
    };
    let lbl = ui.label(egui::RichText::new(tl!("Height")).size(11.5).color(if off { t.warning } else { t.text_dim }));
    if !hint.is_empty() {
        lbl.on_hover_text(&hint);
    }
    for (key, suffix, decimals, label) in [("height", " mm", 1, ""), ("spacing", " mm", 1, tl!("Spacing")), ("lineSpacing", "×", 2, tl!("Lines"))] {
        if !label.is_empty() {
            small_label(ui, label);
        }
        let value = ss::shared(&ls, &[key]).and_then(Value::as_f64);
        let resp_value = widgets::mixed_field(ui, ("bar-lettering", key), value, suffix, decimals, FIELD_W);
        if let Some(v) = resp_value.filter(|v| v.is_finite()) {
            ss::lettering_set(app, json!({ key: v }));
        }
        if key == "height" && off {
            ui.label(egui::RichText::new("⚠").color(t.warning)).on_hover_text(&hint);
        }
    }
    if let Some(p) = path {
        if text_button(ui, Some("spline"), tl!("Put on Path"), tl!("Map Lettering to Path: the letters follow the selected path")).clicked() {
            let ids: Vec<u64> = rows.iter().filter_map(|r| r["id"].as_u64()).collect();
            run(app, "stitch.mapLettering", json!({ "ids": ids, "path": p }));
        }
    } else {
        widgets::dim_label(ui, tl!("Select a path with it to put it on the path"));
    }
}

fn text(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let (family, size, plain) = app
        .session
        .active()
        .and_then(|st| {
            let id = *st.selection.objects.first()?;
            match &st.doc.node(id)?.kind {
                NodeKind::Text(tx) => {
                    let style = tx.runs.first().map(|r| r.style.clone())?;
                    Some((style.font_family, style.size, tx.plain_text()))
                }
                _ => None,
            }
        })
        .unwrap_or_default();
    let sample = crate::font_menu::sample_text(app);
    if let Some(pick) = crate::font_menu::font_menu(ui, "bar-text-font", &family, 170.0, sample.as_deref(), crate::font_menu::MenuLook::of(app)) {
        crate::font_menu::apply(app, ui.ctx(), pick);
    }
    small_label(ui, tl!("Size"));
    if let Some(v) = widgets::plain_field(ui, "bar-text-size", size, " pt", 1, FIELD_W).filter(|v| v.is_finite() && *v > 0.0) {
        run(app, "text.setStyle", json!({ "size": v }));
    }
    ui.label(egui::RichText::new(tl!("Sews as fill")).size(11.5).color(t.text_dim)).on_hover_text(tl!(
        "Computer fonts sew their outlines as fill, which suits large, bold letters. Embroidery fonts are digitized to sew small text cleanly."
    ));
    if text_button(ui, Some("type"), tl!("Use an Embroidery Font…"), tl!("Add this text as lettering in an embroidery font")).clicked() {
        let text = if plain.trim().is_empty() { tl!("Lettering").to_string() } else { plain };
        invoke(app, "type.addLettering", json!({ "text": text }));
    }
}

fn manual(app: &mut VectorcraftApp, ui: &mut Ui, objects: &[Value]) {
    let stitches: u64 = objects.iter().filter_map(|o| o["stitches"].as_u64()).sum();
    widgets::dim_label(ui, &crate::i18n::fmt(tl!("{n} stitches, sewn as they are"), &[("n", &grouped(stitches))]));
    color_chip(app, ui, true);
    if widgets::icon_button(ui, "dc-reverse", tl!("Reverse Sewing Direction"), false, ICON).clicked() {
        run(app, "path.reverse", json!({}));
    }
    more_menu(app, ui, &[("Match to Thread Chart", "stitch.matchThreads")]);
}

fn group(app: &mut VectorcraftApp, ui: &mut Ui, objects: &[Value]) {
    let stitches: u64 = objects.iter().filter_map(|o| o["stitches"].as_u64()).sum();
    widgets::dim_label(ui, &crate::i18n::fmt(tl!("{n} objects, {s} stitches"), &[("n", &objects.len().to_string()), ("s", &grouped(stitches))]));
    let id = app.session.active().and_then(|st| st.selection.objects.first().copied());
    if text_button(ui, Some("square-dashed"), tl!("Enter Group"), tl!("Edit the objects inside it (Escape leaves)")).clicked()
        && let Some(id) = id
    {
        run(app, "object.isolate", json!({ "id": id.0 }));
    }
    if text_button(ui, Some("ungroup"), tl!("Ungroup"), "").clicked() {
        run(app, "object.ungroup", json!({}));
    }
    align_buttons(app, ui, true);
}

/// Align left, centre, right, top, middle, bottom (to the artboard for one object).
fn align_buttons(app: &mut VectorcraftApp, ui: &mut Ui, single: bool) {
    for (icon, tip, p) in [
        ("align-start-vertical", "Horizontal Align Left", json!({"horizontal": "left"})),
        ("align-center-vertical", "Horizontal Align Center", json!({"horizontal": "center"})),
        ("align-end-vertical", "Horizontal Align Right", json!({"horizontal": "right"})),
        ("align-start-horizontal", "Vertical Align Top", json!({"vertical": "top"})),
        ("align-center-horizontal", "Vertical Align Center", json!({"vertical": "center"})),
        ("align-end-horizontal", "Vertical Align Bottom", json!({"vertical": "bottom"})),
    ] {
        if widgets::icon_button(ui, icon, tl!(tip), false, 24.0).clicked() {
            let mut p = p;
            if single {
                p["to"] = json!("artboard");
            }
            run(app, "object.align", p);
        }
    }
}

/// Sew Earlier / Sew Later for the selection, as the Sequencer moves it.
fn order_buttons(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Some(seq) = crate::panels::sequencer::sequence(app, ui.ctx()) else { return };
    let mut order: Vec<u64> = vec![];
    for id in seq["objects"].as_array().into_iter().flatten().filter_map(|o| o["id"].as_u64()) {
        if !order.contains(&id) {
            order.push(id);
        }
    }
    let selected: Vec<u64> = app.session.active().map(|st| st.selection.objects.iter().map(|i| i.0).collect()).unwrap_or_default();
    let picked: Vec<u64> = order.iter().copied().filter(|id| selected.contains(id)).collect();
    let first = order.iter().position(|id| selected.contains(id));
    let last = order.iter().rposition(|id| selected.contains(id));
    let earlier = first.filter(|f| *f > 0).and_then(|f| order.get(f - 1)).copied();
    let later = last.and_then(|l| order.get(l + 1)).copied();
    if widgets::icon_button_enabled(ui, "chevron-up", tl!("Sew Earlier"), false, earlier.is_some(), 24.0).clicked()
        && let Some(before) = earlier
    {
        crate::panels::sequencer::run_move(app, json!({ "ids": picked, "before": before }));
    }
    if widgets::icon_button_enabled(ui, "chevron-down", tl!("Sew Later"), false, later.is_some(), 24.0).clicked()
        && let Some(after) = later
    {
        crate::panels::sequencer::run_move(app, json!({ "ids": picked, "after": after }));
    }
}

/// What two or more selected objects can do together that one can't: (label, icon, tip,
/// command, params).
fn pair_actions(app: &VectorcraftApp, info: &Value) -> Vec<(&'static str, &'static str, &'static str, &'static str, Value)> {
    let Some(st) = app.session.active() else { return vec![] };
    let sel = &st.selection.objects;
    let node = |id: &NodeId| st.doc.node(*id);
    // Plain open paths: not lettering or Satin Blocks, whose paths are open too.
    let special: Vec<u64> = info["lettering"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["id"].as_u64())
        .chain(info["objects"].as_array().into_iter().flatten().filter(|o| o["type"] == json!("satinBlocks")).filter_map(|o| o["id"].as_u64()))
        .collect();
    let open: Vec<u64> = sel
        .iter()
        .filter(|id| !special.contains(&id.0))
        .filter(|id| node(id).is_some_and(|n| matches!(n.kind, NodeKind::Path { .. }) && n.path_data().is_some_and(|p| !p.is_closed())))
        .map(|i| i.0)
        .collect();
    let closed = sel
        .iter()
        .filter(|id| node(id).is_some_and(|n| n.path_data().is_some_and(|p| p.is_closed()) || matches!(n.kind, NodeKind::Compound { .. })))
        .count();
    let fills = info["objects"].as_array().into_iter().flatten().filter(|o| o["fill"]["sews"] == json!(true)).count();
    let mut out = vec![];
    if let [path] = open.as_slice()
        && fills > 0
    {
        out.push((tl!("Use as Fill Guide"), "spline", tl!("The fills' rows follow the selected path"), "stitch.setGuide", json!({ "guide": path })));
    }
    if closed >= 2 && open.is_empty() {
        out.push((tl!("Unite"), "squares-unite", tl!("Join the shapes into one, for one fill or one satin"), "object.pathfinder.unite", json!({})));
    }
    out
}

fn multi(app: &mut VectorcraftApp, ui: &mut Ui, info: &Value, objects: &[Value]) {
    small_label(ui, tl!("Stitch"));
    if let Some(p) = type_dropdown(ui, "bar-multi-type", None, objects, 118.0) {
        run(app, "stitch.set", p);
        return;
    }
    for (label, icon, tip, cmd, p) in pair_actions(app, info) {
        if text_button(ui, Some(icon), label, tip).clicked() {
            run(app, cmd, p);
        }
    }
    if widgets::icon_button(ui, "group", tl!("Group"), false, ICON).clicked() {
        run(app, "object.group", json!({}));
    }
    divider(ui);
    align_buttons(app, ui, false);
    divider(ui);
    order_buttons(app, ui);
    if widgets::icon_button(ui, "arrow-down-wide-narrow", tl!("Sort by Color"), false, ICON).clicked() {
        crate::panels::sequencer::sort(app);
    }
    if widgets::icon_button(ui, "squares-subtract", tl!("Remove Overlaps: cut fills where later ones cover them"), false, ICON).clicked() {
        run(app, "stitch.removeOverlaps", json!({}));
    }
    if widgets::icon_button(ui, "swatch-book", tl!("Match to Thread Chart"), false, ICON).clicked() {
        run(app, "stitch.matchThreads", json!({}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        crate::embroidery_workspace::first_run(&mut app);
        app.session.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
        app
    }

    fn rect(app: &mut VectorcraftApp, x: f64) -> u64 {
        app.session.execute("shape.rectangle", &json!({"x": x, "y": 20, "width": 60, "height": 60})).unwrap()["id"].as_u64().unwrap()
    }

    fn line_path(app: &mut VectorcraftApp) -> u64 {
        app.session.execute("shape.line", &json!({"x1": 10, "y1": 200, "x2": 200, "y2": 220})).unwrap()["id"].as_u64().unwrap()
    }

    fn select(app: &mut VectorcraftApp, ids: &[u64]) {
        app.session.execute("select.set", &json!({ "ids": ids })).unwrap();
    }

    fn context_now(app: &mut VectorcraftApp) -> Context {
        let info = app.session.execute("stitch.info", &json!({})).unwrap();
        context(app, &info)
    }

    /// The bar's painted text, one frame wide enough for everything.
    fn painted(app: &mut VectorcraftApp) -> String {
        fn texts(s: &egui::Shape, out: &mut Vec<String>) {
            match s {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(2400.0, 200.0))), ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| show(app, ui));
        out.textures_delta.clear();
        let mut v = vec![];
        out.shapes.iter().for_each(|c| texts(&c.shape, &mut v));
        v.join(" | ")
    }

    #[test]
    fn the_context_follows_the_selection() {
        let mut app = app();
        assert_eq!(context_now(&mut app), Context::Design);
        let a = rect(&mut app, 20.0);
        let b = rect(&mut app, 120.0);
        let l = line_path(&mut app);
        select(&mut app, &[a]);
        assert_eq!(context_now(&mut app), Context::Fill);
        select(&mut app, &[l]);
        assert_eq!(context_now(&mut app), Context::Line);
        select(&mut app, &[a, b]);
        assert_eq!(context_now(&mut app), Context::Multi);
        select(&mut app, &[a]);
        app.session.execute("stitch.convertToSatin", &json!({})).unwrap();
        assert_eq!(context_now(&mut app), Context::Satin, "the new Satin Blocks are selected");
        let lt = app.session.execute("stitch.createLettering", &json!({"text": "Hi", "height": 12})).unwrap();
        let lid = lt["id"].as_u64().or_else(|| lt["ids"][0].as_u64()).unwrap();
        select(&mut app, &[lid]);
        assert_eq!(context_now(&mut app), Context::Lettering { path: None });
        select(&mut app, &[lid, l]);
        assert_eq!(context_now(&mut app), Context::Lettering { path: Some(l) });
    }

    #[test]
    fn the_bar_shows_the_design_with_nothing_selected_and_the_fill_with_a_shape() {
        let mut app = app();
        let a = rect(&mut app, 20.0);
        select(&mut app, &[]);
        let text = painted(&mut app);
        for want in ["Design", "Add Lettering", "Center in Hoop", "Export", " st · "] {
            assert!(text.contains(want), "{want:?} in {text}");
        }
        select(&mut app, &[a]);
        let text = painted(&mut app);
        for want in ["Tatami", "Convert to Satin", "Angle", "Border", "Export"] {
            assert!(text.contains(want), "{want:?} in {text}");
        }
        assert!(!text.contains("Add Lettering"), "{text}");
    }

    #[test]
    fn the_bar_replaces_the_control_bar_in_the_embroidery_workspace_only() {
        let mut app = app();
        assert!(crate::embroidery_workspace::shows_embroidery_bar(&app));
        crate::menus::invoke(&mut app, "window.embroideryBar", json!({}));
        assert!(!crate::embroidery_workspace::shows_embroidery_bar(&app));
        crate::menus::invoke(&mut app, "window.embroideryBar", json!({}));
        app.ui.embroidery = false;
        assert!(!crate::embroidery_workspace::shows_embroidery_bar(&app));
    }

    #[test]
    fn a_fill_with_a_path_offers_the_guide_but_not_with_lettering() {
        let mut app = app();
        let a = rect(&mut app, 20.0);
        let l = line_path(&mut app);
        select(&mut app, &[a, l]);
        let info = app.session.execute("stitch.info", &json!({})).unwrap();
        assert!(pair_actions(&app, &info).iter().any(|p| p.3 == "stitch.setGuide"));
        let lt = app.session.execute("stitch.createLettering", &json!({"text": "Hi", "height": 12})).unwrap();
        select(&mut app, &[a, lt["id"].as_u64().unwrap()]);
        let info = app.session.execute("stitch.info", &json!({})).unwrap();
        assert!(pair_actions(&app, &info).is_empty());
    }

    #[test]
    fn counts_group_their_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1,000");
        assert_eq!(grouped(1234567), "1,234,567");
    }
}
