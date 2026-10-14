//! Roidery: the Sequencer, the Embroidery workspace's one list of objects (it stands in for
//! Layers there). Objects lists every embroidery object in sewing order, first sewn at the top,
//! laid out as Layers lays out its rows: the eye and lock, a thumbnail of the object's stitches
//! in its threads, its name (double-click to rename) and stitch type, and its stitch count. An
//! object with both a fill and a stroke shows them as two steps under it; dragging one above the
//! other changes which sews first (`stitch.partOrder`). Layers and groups head their objects and
//! fold open and closed; a thin divider with the thread's swatch marks each colour change.
//! Colors lists the colour blocks. Click to select, drag a row (or use Sew Earlier / Sew Later)
//! to change the order, and Sort by Color sews each thread in one go where the stacking allows.
//! Everything reads `stitch.sequence` and acts through `stitch.move`, `stitch.partOrder`,
//! `stitch.sortColors` and `layer.setProps`.

use std::collections::{HashMap, HashSet};

use egui::{CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::theme::Tokens;
use crate::{VectorcraftApp, icons, widgets};

pub const ID: &str = "sequencer";
const ROW_H: f32 = 26.0;
/// An object's row (thumbnail and two lines of text).
const OBJECT_H: f32 = 40.0;
/// A fill or stroke step under an object, a layer or group heading, a colour divider.
const STEP_H: f32 = 22.0;
const HEAD_H: f32 = 24.0;
const DIVIDER_H: f32 = 16.0;
/// The eye and lock columns.
const COLUMN: f32 = 22.0;
const THUMB: f32 = 32.0;

/// `stitch.sequence`, recomputed only when the document changes (it sews every object).
pub(crate) fn sequence(app: &mut VectorcraftApp, ctx: &egui::Context) -> Option<Value> {
    type Key = (u64, u64);
    let st = app.session.active()?;
    let key: Key = (st.uid, st.revision);
    let id = egui::Id::new("sequencer-sequence");
    if let Some((k, v)) = ctx.data(|d| d.get_temp::<(Key, Value)>(id))
        && k == key
    {
        return Some(v);
    }
    let v = app.session.execute("stitch.sequence", &json!({"thumbs": true})).ok()?;
    ctx.data_mut(|d| d.insert_temp(id, (key, v.clone())));
    Some(v)
}

/// A stitch type's label, as the Stitch Settings panel names it.
fn type_label(kind: &str) -> &'static str {
    match kind {
        "tatami" => tl!("Tatami"),
        "contour" => tl!("Contour Fill"),
        "circular" => tl!("Circular Fill"),
        "gradient" => tl!("Gradient Fill"),
        "motif" => tl!("Motif Fill"),
        "guided" => tl!("Guided Fill"),
        "satin" => tl!("Satin"),
        "satinBlocks" => tl!("Satin Blocks"),
        "run" => tl!("Single Run"),
        "triple" => tl!("Triple Run"),
        "manual" => tl!("Manual"),
        "eStitch" => tl!("E-Stitch"),
        "rope" => tl!("Rope"),
        _ => tl!("Not Stitched"),
    }
}

fn ids(v: &Value) -> Vec<u64> {
    v.as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default()
}

pub(crate) fn rgb(v: &Value) -> egui::Color32 {
    let c = |i: usize| v.get(i).and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
    egui::Color32::from_rgb(c(0), c(1), c(2))
}

/// One row of either tab: the objects it stands for, its thread, and its text.
struct Row {
    ids: Vec<u64>,
    thread: egui::Color32,
    number: usize,
    label: String,
    detail: String,
}

/// One fill or stroke step of an object.
struct Step {
    /// `fill`, `stroke`, or `None` for an object that sews nothing.
    part: Option<String>,
    kind: String,
    stitches: u64,
    thread: egui::Color32,
    block: Option<usize>,
    thumb: Vec<[f32; 2]>,
}

/// An object in the Objects tab, with its steps in sewing order.
struct Entry {
    id: u64,
    name: String,
    visible: bool,
    locked: bool,
    /// Layers and groups, outermost first.
    parents: Vec<u64>,
    steps: Vec<Step>,
}

impl Entry {
    fn kind(&self) -> &'static str {
        type_label(self.steps.first().map_or("", |s| s.kind.as_str()))
    }

    fn label(&self) -> String {
        if self.name.is_empty() { self.kind().to_string() } else { self.name.clone() }
    }

    fn stitches(&self) -> u64 {
        self.steps.iter().map(|s| s.stitches).sum()
    }
}

/// The objects of `stitch.sequence`, each with its steps.
fn entries(seq: &Value) -> Vec<Entry> {
    let mut out: Vec<Entry> = vec![];
    for o in seq["objects"].as_array().into_iter().flatten() {
        let Some(id) = o["id"].as_u64() else { continue };
        let thumb = o["thumb"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| Some([p.get(0)?.as_f64()? as f32, p.get(1)?.as_f64()? as f32]))
                    .filter(|p| p[0].is_finite() && p[1].is_finite())
                    .collect()
            })
            .unwrap_or_default();
        let step = Step {
            part: o["part"].as_str().map(str::to_string),
            kind: o["type"].as_str().unwrap_or("").to_string(),
            stitches: o["stitches"].as_u64().unwrap_or(0),
            thread: rgb(&o["thread"]),
            block: o["block"].as_u64().and_then(|b| usize::try_from(b).ok()),
            thumb,
        };
        match out.last_mut() {
            Some(e) if e.id == id => e.steps.push(step),
            _ => out.push(Entry {
                id,
                name: o["name"].as_str().unwrap_or("").to_string(),
                visible: o["visible"].as_bool().unwrap_or(true),
                locked: o["locked"].as_bool().unwrap_or(false),
                parents: ids(&o["parents"]),
                steps: vec![step],
            }),
        }
    }
    out
}

/// A layer or group heading its objects.
struct Group {
    name: String,
    layer: bool,
    visible: bool,
    locked: bool,
}

fn groups(seq: &Value) -> HashMap<u64, Group> {
    seq["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| {
            Some((
                g["id"].as_u64()?,
                Group {
                    name: g["name"].as_str().unwrap_or("").to_string(),
                    layer: g["kind"] == "layer",
                    visible: g["visible"].as_bool().unwrap_or(true),
                    locked: g["locked"].as_bool().unwrap_or(false),
                },
            ))
        })
        .collect()
}

/// A line of the Objects tab, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Line {
    /// A layer or group, `depth` in.
    Head { id: u64, depth: usize },
    /// A colour change: the colour block that starts.
    Divider { block: usize },
    /// Entry `entry`.
    Object { entry: usize, depth: usize },
    /// Step `step` of entry `entry`, when it has more than one.
    Step { entry: usize, step: usize, depth: usize },
}

/// The Objects tab's lines: layers (when there are several) and groups heading their objects,
/// folded ones without what's inside, and a divider at each colour change.
fn lines(entries: &[Entry], layers: usize, folded: &HashSet<u64>) -> Vec<Line> {
    let mut out = vec![];
    let mut open: Vec<u64> = vec![];
    let mut colour: Option<usize> = None;
    for (i, e) in entries.iter().enumerate() {
        let chain: &[u64] = if layers > 1 { &e.parents } else { e.parents.get(1..).unwrap_or_default() };
        let same = open.iter().zip(chain).take_while(|(a, b)| a == b).count();
        open.truncate(same);
        // Inside a folded layer or group: its heading, and nothing deeper.
        for g in &chain[same..] {
            if !open.iter().any(|o| folded.contains(o)) {
                out.push(Line::Head { id: *g, depth: open.len() });
            }
            open.push(*g);
        }
        if chain.iter().any(|g| folded.contains(g)) {
            continue;
        }
        let depth = chain.len();
        let mut divide = |b: Option<usize>, out: &mut Vec<Line>| {
            if let Some(b) = b
                && colour != Some(b)
            {
                out.push(Line::Divider { block: b });
                colour = Some(b);
            }
        };
        divide(e.steps.first().and_then(|s| s.block), &mut out);
        out.push(Line::Object { entry: i, depth });
        if e.steps.len() > 1 {
            for (k, st) in e.steps.iter().enumerate() {
                if k > 0 {
                    divide(st.block, &mut out);
                }
                out.push(Line::Step { entry: i, step: k, depth });
            }
        }
    }
    out
}

/// A thread's chart name and number (`Black 002`), else its colour as hex.
pub(crate) fn thread_label(v: &Value, t: egui::Color32) -> String {
    let name = v["threadName"].as_str().or_else(|| v["name"].as_str()).unwrap_or("");
    let code = v["threadCode"].as_str().or_else(|| v["code"].as_str()).unwrap_or("");
    match (name, code) {
        ("", "") => format!("#{:02X}{:02X}{:02X}", t.r(), t.g(), t.b()),
        (n, "") => n.to_string(),
        ("", c) => c.to_string(),
        (n, c) => format!("{n} {c}"),
    }
}

fn color_rows(seq: &Value) -> Vec<Row> {
    let colors = seq["colors"].as_array().cloned().unwrap_or_default();
    colors
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let t = rgb(&c["thread"]);
            let objects = ids(&c["objects"]);
            Row {
                label: format!(
                    "{} · {}",
                    thread_label(c, t),
                    if objects.len() == 1 { tl!("1 object").to_string() } else { tl!("{n} objects").replace("{n}", &objects.len().to_string()) }
                ),
                ids: objects,
                thread: t,
                number: i + 1,
                detail: c["stitches"].as_u64().unwrap_or(0).to_string(),
            }
        })
        .collect()
}

/// Move `rows[from]` (each row: the objects it stands for) to sew at gap `to` (0 = first,
/// `rows.len()` = last) → the `stitch.move` params, or `None` when it stays where it is.
fn move_params(rows: &[Vec<u64>], from: usize, to: usize) -> Option<Value> {
    let moving = rows.get(from)?;
    if to == from || to == from + 1 {
        return None;
    }
    match rows.get(to) {
        Some(target) => Some(json!({ "ids": moving, "before": target.first()? })),
        None => Some(json!({ "ids": moving, "after": rows.last()?.last()? })),
    }
}

pub(crate) fn run_move(app: &mut VectorcraftApp, p: Value) {
    if let Err(e) = app.run("stitch.move", p) {
        app.status(e);
    }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(seq) = sequence(app, ui.ctx()) else {
        widgets::dim_label(ui, tl!("No document"));
        return;
    };
    let tab = usize::from(app.ui.sequencer_colors);
    if let Some(i) = widgets::tab_bar(ui, &["Objects", "Colors"], tab) {
        app.ui.sequencer_colors = i == 1;
    }
    ui.add_space(4.0);
    if seq["objects"].as_array().is_none_or(Vec::is_empty) {
        widgets::dim_label(ui, tl!("Nothing sews yet: draw paths, or use the Satin Blocks tool."));
        return;
    }
    let selected: Vec<u64> = app.session.active().map(|st| st.selection.objects.iter().map(|i| i.0).collect()).unwrap_or_default();
    // The selected rows, in order, for Sew Earlier / Later: each row's objects.
    let rows: Vec<Vec<u64>> = if app.ui.sequencer_colors {
        let rows = color_rows(&seq);
        show_colors(app, ui, &t, &rows, &selected);
        rows.into_iter().map(|r| r.ids).collect()
    } else {
        let entries = entries(&seq);
        show_objects(app, ui, &t, &seq, &entries, &selected);
        entries.iter().map(|e| vec![e.id]).collect()
    };
    let is_selected = |r: &Vec<u64>| !r.is_empty() && r.iter().all(|id| selected.contains(id));
    let first = rows.iter().position(is_selected);
    let last = rows.iter().rposition(is_selected);
    let picked: Vec<u64> = rows.iter().filter(|r| is_selected(r)).flatten().copied().collect();
    widgets::bottom_bar(ui, |ui| {
        let earlier = first.filter(|f| *f > 0).and_then(|f| rows.get(f - 1)).and_then(|r| r.first().copied());
        if widgets::icon_button_enabled(ui, "chevron-up", tl!("Sew Earlier"), false, earlier.is_some(), 24.0).clicked()
            && let Some(before) = earlier
        {
            run_move(app, json!({ "ids": picked, "before": before }));
        }
        let later = last.and_then(|l| rows.get(l + 1)).and_then(|r| r.last().copied());
        if widgets::icon_button_enabled(ui, "chevron-down", tl!("Sew Later"), false, later.is_some(), 24.0).clicked()
            && let Some(after) = later
        {
            run_move(app, json!({ "ids": picked, "after": after }));
        }
        if widgets::icon_button(ui, "arrow-down-wide-narrow", tl!("Sort by Color"), false, 24.0).clicked() {
            sort(app);
        }
        let total = format!(
            "{} · {}",
            tl!("{n} stitches").replace("{n}", &seq["stitches"].as_u64().unwrap_or(0).to_string()),
            tl!("{n} colors").replace("{n}", &seq["colors"].as_array().map_or(0, Vec::len).to_string())
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(total).size(11.5).color(t.text_dim));
        });
    });
}

/// Room left above the bottom bar.
const FOOTER: f32 = 40.0;

/// Select `ids`, or add them to the selection.
fn select(app: &mut VectorcraftApp, ids: &[u64], add: bool, selected: &[u64]) {
    let mut ids = ids.to_vec();
    if add {
        let more: Vec<u64> = selected.iter().filter(|id| !ids.contains(id)).copied().collect();
        ids.extend(more);
    }
    app.run("select.set", json!({ "ids": ids })).ok();
}

/// The Colors tab: one row per colour block.
fn show_colors(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens, rows: &[Row], selected: &[u64]) {
    let is_selected = |r: &Row| !r.ids.is_empty() && r.ids.iter().all(|id| selected.contains(id));
    let drag_id = ui.id().with("sequencer-drag");
    let dragging: Option<usize> = ui.ctx().data(|d| d.get_temp(drag_id));
    let mut rects = Vec::with_capacity(rows.len());
    let mut clicked: Option<(usize, bool)> = None;
    egui::ScrollArea::vertical()
        .id_salt("sequencer-rows")
        .max_height((ui.available_height() - FOOTER).max(ROW_H * 3.0))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, row) in rows.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
                let p = ui.painter();
                if is_selected(row) {
                    p.rect_filled(rect, 0.0, t.selection);
                } else if resp.hovered() {
                    p.rect_filled(rect, 0.0, t.hover);
                }
                let num = p.layout_no_wrap(row.number.to_string(), egui::FontId::proportional(11.0), t.text_dim);
                p.galley(pos2(rect.left() + 26.0 - num.size().x, rect.center().y - num.size().y / 2.0), num, t.text_dim);
                chip(ui, pos2(rect.left() + 40.0, rect.center().y), row.thread, t);
                let p = ui.painter();
                let detail = p.layout_no_wrap(row.detail.clone(), egui::FontId::proportional(11.5), t.text_dim);
                let detail_x = rect.right() - 8.0 - detail.size().x;
                p.galley(pos2(detail_x, rect.center().y - detail.size().y / 2.0), detail, t.text_dim);
                let label = p.layout(row.label.clone(), egui::FontId::proportional(12.5), t.text, (detail_x - rect.left() - 60.0).max(20.0));
                p.galley_with_override_text_color(pos2(rect.left() + 54.0, rect.center().y - label.size().y.min(ROW_H) / 2.0), label, t.text);
                if resp.drag_started() {
                    ui.ctx().data_mut(|d| d.insert_temp(drag_id, i));
                }
                if resp.clicked() {
                    clicked = Some((i, ui.input(|i| i.modifiers.shift || i.modifiers.command)));
                }
                rects.push(rect);
            }
        });
    let ids: Vec<Vec<u64>> = rows.iter().map(|r| r.ids.clone()).collect();
    drop_rows(app, ui, t, drag_id, dragging, &rects, &ids);
    if let Some((i, add)) = clicked
        && let Some(row) = rows.get(i)
    {
        select(app, &row.ids, add, selected);
    }
}

/// A thread swatch centred at `c`.
fn chip(ui: &Ui, c: egui::Pos2, thread: egui::Color32, t: &Tokens) {
    let r = Rect::from_center_size(c, vec2(14.0, 14.0));
    ui.painter().rect_filled(r, CornerRadius::same(3), thread);
    ui.painter().rect_stroke(r, CornerRadius::same(3), Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
}

/// While a row is dragged: a line where it would go; dropping it there moves it.
fn drop_rows(app: &mut VectorcraftApp, ui: &Ui, t: &Tokens, drag_id: egui::Id, dragging: Option<usize>, rects: &[Rect], rows: &[Vec<u64>]) {
    let Some(from) = dragging else { return };
    let gap = ui.input(|i| i.pointer.interact_pos()).map(|p| rects.iter().take_while(|r| p.y > r.center().y).count());
    if let (Some(gap), Some(first)) = (gap, rects.first()) {
        let y = rects.get(gap).map_or_else(|| rects.last().map_or(first.top(), |r| r.bottom()), |r| r.top());
        ui.painter().line_segment([pos2(first.left(), y), pos2(first.right(), y)], Stroke::new(2.0, t.accent));
    }
    if ui.input(|i| !i.pointer.any_down()) {
        ui.ctx().data_mut(|d| d.remove::<usize>(drag_id));
        if let Some(p) = gap.and_then(|g| move_params(rows, from, g)) {
            run_move(app, p);
        }
    }
}

/// What a click or drag in the Objects tab asks for, done after drawing.
#[derive(Default)]
struct Asks {
    select: Option<(Vec<u64>, bool)>,
    props: Option<Value>,
    rename: Option<(u64, String)>,
    fold: Option<u64>,
}

/// The eye and lock columns at the left of `r`, for object or group `id`.
fn eye_and_lock(ui: &mut Ui, t: &Tokens, r: Rect, id: u64, visible: bool, locked: bool, asks: &mut Asks) {
    let eye = Rect::from_min_size(r.min, vec2(COLUMN, r.height()));
    let lock = Rect::from_min_size(r.min + vec2(COLUMN, 0.0), vec2(COLUMN, r.height()));
    let er = ui.interact(eye, ui.id().with(("seq-eye", id)), Sense::click());
    let lr = ui.interact(lock, ui.id().with(("seq-lock", id)), Sense::click());
    if visible {
        icons::paint(ui, "eye", Rect::from_center_size(eye.center(), vec2(14.0, 14.0)), t.icon);
    } else if er.hovered() {
        icons::paint(ui, "eye", Rect::from_center_size(eye.center(), vec2(14.0, 14.0)), t.text_disabled);
    }
    if locked {
        icons::paint(ui, "lock", Rect::from_center_size(lock.center(), vec2(12.0, 12.0)), t.icon);
    } else if lr.hovered() {
        icons::paint(ui, "lock", Rect::from_center_size(lock.center(), vec2(12.0, 12.0)), t.text_disabled);
    }
    if er.on_hover_text(tl!("Show or hide")).clicked() {
        asks.props = Some(json!({ "ids": [id], "visible": !visible }));
    }
    if lr.on_hover_text(tl!("Lock or unlock")).clicked() {
        asks.props = Some(json!({ "ids": [id], "locked": !locked }));
    }
}

/// An object's stitches drawn small in their threads, on fabric; its outline when it sews
/// nothing (hidden, or not stitched).
fn thumbnail(ui: &Ui, app: &VectorcraftApp, e: &Entry, r: Rect, t: &Tokens) {
    let fabric = egui::Color32::from_rgb(236, 233, 226);
    ui.painter().rect_filled(r, CornerRadius::same(3), fabric);
    ui.painter().rect_stroke(r, CornerRadius::same(3), Stroke::new(1.0, t.input_border), egui::StrokeKind::Inside);
    let pts = || e.steps.iter().flat_map(|s| s.thumb.iter());
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in pts() {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let inner = r.shrink(3.0);
    if lo[0] > hi[0] {
        if let Some(n) = app.session.active().and_then(|st| st.doc.node(vectorcraft_doc::NodeId(e.id))) {
            super::layers::thumb(ui, n, inner);
        }
        return;
    }
    let size = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(0.1);
    let k = inner.width() / size;
    let off = inner.center() - vec2((lo[0] + hi[0]) / 2.0 * k, (lo[1] + hi[1]) / 2.0 * k);
    let painter = ui.painter().with_clip_rect(r);
    for s in &e.steps {
        if s.thumb.len() >= 2 {
            let line: Vec<egui::Pos2> = s.thumb.iter().map(|p| off + vec2(p[0] * k, p[1] * k)).collect();
            painter.add(egui::Shape::line(line, Stroke::new(1.0, s.thread)));
        }
    }
}

/// An object's name, or what Layers calls it (`<Ellipse>`).
fn shown_name(app: &VectorcraftApp, e: &Entry) -> String {
    if !e.name.is_empty() {
        return e.name.clone();
    }
    match app.session.active().and_then(|st| st.doc.node(vectorcraft_doc::NodeId(e.id))) {
        Some(n) => super::layers::painted_name(n, &n.display_name(), crate::i18n::current()),
        None => e.label(),
    }
}

/// The Objects tab: layers and groups, objects with thumbnails, fill and stroke steps, colour
/// dividers.
fn show_objects(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens, seq: &Value, entries: &[Entry], selected: &[u64]) {
    let groups = groups(seq);
    let layers = groups.values().filter(|g| g.layer).count();
    let fold_id = egui::Id::new("sequencer-folded");
    let folded: HashSet<u64> = ui.ctx().data(|d| d.get_temp(fold_id)).unwrap_or_default();
    let rename_id = egui::Id::new("sequencer-rename");
    let mut renaming: Option<(u64, String)> = ui.ctx().data(|d| d.get_temp(rename_id));
    let drag_id = ui.id().with("sequencer-drag");
    let dragging: Option<usize> = ui.ctx().data(|d| d.get_temp(drag_id));
    let step_drag_id = ui.id().with("sequencer-step-drag");
    let step_drag: Option<(usize, usize)> = ui.ctx().data(|d| d.get_temp(step_drag_id));
    let lines = lines(entries, layers, &folded);
    let colors = seq["colors"].as_array().cloned().unwrap_or_default();
    // Drawn object rows: (entry, rect), for dropping dragged rows; step rows likewise.
    let mut objects: Vec<(usize, Rect)> = vec![];
    let mut steps: Vec<(usize, usize, Rect)> = vec![];
    let mut asks = Asks::default();
    egui::ScrollArea::vertical()
        .id_salt("sequencer-objects")
        .max_height((ui.available_height() - FOOTER).max(OBJECT_H * 3.0))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for line in &lines {
                match *line {
                    Line::Divider { block } => {
                        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), DIVIDER_H), Sense::hover());
                        let c = colors.get(block).cloned().unwrap_or_default();
                        let thread = rgb(&c["thread"]);
                        let y = r.center().y;
                        ui.painter().line_segment([pos2(r.left() + 2.0 * COLUMN, y), pos2(r.right() - 4.0, y)], Stroke::new(1.0, t.divider));
                        let label = thread_label(&c, thread);
                        let g = ui.painter().layout_no_wrap(label, egui::FontId::proportional(10.5), t.text_dim);
                        let w = g.size().x + 26.0;
                        let bg = Rect::from_min_size(pos2(r.left() + 2.0 * COLUMN, r.top()), vec2(w, r.height()));
                        ui.painter().rect_filled(bg, 0.0, ui.visuals().panel_fill);
                        let sw = Rect::from_center_size(pos2(bg.left() + 8.0, y), vec2(10.0, 10.0));
                        ui.painter().rect_filled(sw, CornerRadius::same(2), thread);
                        ui.painter().rect_stroke(sw, CornerRadius::same(2), Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
                        ui.painter().galley(pos2(sw.right() + 5.0, y - g.size().y / 2.0), g, t.text_dim);
                    }
                    Line::Head { id, depth } => {
                        let Some(g) = groups.get(&id) else { continue };
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), HEAD_H), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, 0.0, t.hover);
                        }
                        eye_and_lock(ui, t, r, id, g.visible, g.locked, &mut asks);
                        let x = r.left() + 2.0 * COLUMN + depth as f32 * 12.0;
                        let tri = Rect::from_min_size(pos2(x, r.top()), vec2(16.0, r.height()));
                        let open = !folded.contains(&id);
                        icons::paint(
                            ui,
                            if open { "chevron-down" } else { "chevron-right" },
                            Rect::from_center_size(tri.center(), vec2(12.0, 12.0)),
                            t.icon,
                        );
                        let name = if g.name.is_empty() { if g.layer { tl!("Layer") } else { tl!("Group") }.to_string() } else { g.name.clone() };
                        let color = if g.visible { t.text } else { t.text_disabled };
                        let galley = ui.painter().layout_no_wrap(name, egui::FontId::proportional(12.0), color);
                        ui.painter().galley(pos2(tri.right() + 4.0, r.center().y - galley.size().y / 2.0), galley, color);
                        if resp.clicked() {
                            if resp.interact_pointer_pos().is_some_and(|p| tri.contains(p)) {
                                asks.fold = Some(id);
                            } else {
                                asks.select = Some((vec![id], ui.input(|i| i.modifiers.shift || i.modifiers.command)));
                            }
                        }
                    }
                    Line::Object { entry, depth } => {
                        let Some(e) = entries.get(entry) else { continue };
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), OBJECT_H), Sense::click_and_drag());
                        if selected.contains(&e.id) {
                            ui.painter().rect_filled(r, 0.0, t.selection);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(r, 0.0, t.hover);
                        }
                        eye_and_lock(ui, t, r, e.id, e.visible, e.locked, &mut asks);
                        let x = r.left() + 2.0 * COLUMN + depth as f32 * 12.0 + 4.0;
                        let tr = Rect::from_min_size(pos2(x, r.center().y - THUMB / 2.0), vec2(THUMB, THUMB));
                        thumbnail(ui, app, e, tr, t);
                        if !e.visible {
                            ui.painter().rect_filled(tr, CornerRadius::same(3), ui.visuals().panel_fill.gamma_multiply(0.6));
                        }
                        let count = e.stitches().to_string();
                        let cg = ui.painter().layout_no_wrap(count, egui::FontId::proportional(11.5), t.text_dim);
                        let count_x = r.right() - 8.0 - cg.size().x;
                        ui.painter().galley(pos2(count_x, r.center().y - cg.size().y / 2.0), cg, t.text_dim);
                        let text_x = tr.right() + 8.0;
                        let width = (count_x - text_x - 6.0).max(20.0);
                        let name_rect = Rect::from_min_size(pos2(text_x, r.top() + 4.0), vec2(width, 17.0));
                        let color = if e.visible { t.text } else { t.text_disabled };
                        match renaming.as_mut().filter(|(id, _)| *id == e.id) {
                            Some((_, buf)) => {
                                let te = ui.put(name_rect, egui::TextEdit::singleline(buf).font(egui::FontId::proportional(12.5)));
                                te.request_focus();
                                if te.lost_focus() {
                                    asks.rename = Some((e.id, buf.clone()));
                                }
                            }
                            None => {
                                let g = ui.painter().layout(shown_name(app, e), egui::FontId::proportional(12.5), color, width);
                                ui.painter().with_clip_rect(name_rect.expand(2.0)).galley(name_rect.min, g, color);
                            }
                        }
                        let kind = if e.steps.len() > 1 {
                            e.steps.iter().map(|s| type_label(&s.kind)).collect::<Vec<_>>().join(" + ")
                        } else {
                            e.kind().to_string()
                        };
                        let g = ui.painter().layout_no_wrap(kind, egui::FontId::proportional(11.0), t.text_dim);
                        ui.painter().with_clip_rect(r).galley(pos2(text_x, r.bottom() - 6.0 - g.size().y), g, t.text_dim);
                        if resp.double_clicked() && resp.interact_pointer_pos().is_some_and(|p| p.x >= text_x) {
                            renaming = Some((e.id, e.name.clone()));
                        } else if resp.clicked() {
                            asks.select = Some((vec![e.id], ui.input(|i| i.modifiers.shift || i.modifiers.command)));
                        }
                        if resp.drag_started() {
                            ui.ctx().data_mut(|d| d.insert_temp(drag_id, objects.len()));
                        }
                        objects.push((entry, r));
                    }
                    Line::Step { entry, step, depth } => {
                        let (Some(e), Some(st)) = (entries.get(entry), entries.get(entry).and_then(|e| e.steps.get(step))) else { continue };
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), STEP_H), Sense::click_and_drag());
                        if selected.contains(&e.id) {
                            ui.painter().rect_filled(r, 0.0, t.selection.gamma_multiply(0.6));
                        } else if resp.hovered() {
                            ui.painter().rect_filled(r, 0.0, t.hover);
                        }
                        let x = r.left() + 2.0 * COLUMN + depth as f32 * 12.0 + 4.0 + THUMB / 2.0;
                        chip(ui, pos2(x + 7.0, r.center().y), st.thread, t);
                        let part = match st.part.as_deref() {
                            Some("fill") => tl!("Fill"),
                            Some("stroke") => tl!("Stroke"),
                            _ => "",
                        };
                        let text = format!("{part} · {}", type_label(&st.kind));
                        let color = if e.visible { t.text } else { t.text_disabled };
                        let g = ui.painter().layout_no_wrap(text, egui::FontId::proportional(11.5), color);
                        ui.painter().galley(pos2(x + 20.0, r.center().y - g.size().y / 2.0), g, color);
                        let cg = ui.painter().layout_no_wrap(st.stitches.to_string(), egui::FontId::proportional(11.0), t.text_dim);
                        ui.painter().galley(pos2(r.right() - 8.0 - cg.size().x, r.center().y - cg.size().y / 2.0), cg, t.text_dim);
                        if resp.clicked() {
                            asks.select = Some((vec![e.id], ui.input(|i| i.modifiers.shift || i.modifiers.command)));
                        }
                        if resp.drag_started() {
                            ui.ctx().data_mut(|d| d.insert_temp(step_drag_id, (entry, step)));
                        }
                        steps.push((entry, step, r));
                    }
                }
            }
        });
    // Dropping a dragged object row.
    let rects: Vec<Rect> = objects.iter().map(|o| o.1).collect();
    let rows: Vec<Vec<u64>> = objects.iter().filter_map(|(i, _)| entries.get(*i).map(|e| vec![e.id])).collect();
    drop_rows(app, ui, t, drag_id, dragging, &rects, &rows);
    // Dropping a dragged fill or stroke step above or below its sibling.
    if let Some((entry, step)) = step_drag {
        let mine: Vec<&(usize, usize, Rect)> = steps.iter().filter(|s| s.0 == entry).collect();
        let pos = ui.input(|i| i.pointer.interact_pos());
        // The step it would go before (or after the last).
        let gap = pos.map(|p| mine.iter().take_while(|s| p.y > s.2.center().y).count());
        if let (Some(g), Some(first)) = (gap, mine.first()) {
            let y = mine.get(g).map_or_else(|| mine.last().map_or(first.2.top(), |s| s.2.bottom()), |s| s.2.top());
            ui.painter().line_segment([pos2(first.2.left() + 2.0 * COLUMN, y), pos2(first.2.right(), y)], Stroke::new(2.0, t.accent));
        }
        if ui.input(|i| !i.pointer.any_down()) {
            ui.ctx().data_mut(|d| d.remove::<(usize, usize)>(step_drag_id));
            let e = entries.get(entry);
            let moved_first = gap == Some(0) && step > 0;
            let moved_last = gap.is_some_and(|g| g >= mine.len()) && step == 0;
            let part = e.and_then(|e| e.steps.get(step)).and_then(|s| s.part.clone());
            let other = e.and_then(|e| e.steps.get(1 - step.min(1))).and_then(|s| s.part.clone());
            let first = if moved_first {
                part
            } else if moved_last {
                other
            } else {
                None
            };
            if let (Some(first), Some(e)) = (first, e)
                && let Err(err) = app.run("stitch.partOrder", json!({ "ids": [e.id], "first": first }))
            {
                app.status(err);
            }
        }
    }
    // Renaming: Enter or clicking away keeps the name; Escape keeps the old one.
    if let Some((id, name)) = asks.rename.take() {
        renaming = None;
        if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.run("layer.setProps", json!({ "ids": [id], "name": name })).ok();
        }
    }
    if let Some(r) = renaming {
        ui.ctx().data_mut(|d| d.insert_temp(rename_id, r));
    } else {
        ui.ctx().data_mut(|d| d.remove::<(u64, String)>(rename_id));
    }
    if let Some(id) = asks.fold {
        let mut f = folded;
        if !f.remove(&id) {
            f.insert(id);
        }
        ui.ctx().data_mut(|d| d.insert_temp(fold_id, f));
    }
    if let Some(p) = asks.props
        && let Err(e) = app.run("layer.setProps", p)
    {
        app.status(e);
    }
    if let Some((ids, add)) = asks.select {
        select(app, &ids, add, selected);
    }
}

pub(crate) fn sort(app: &mut VectorcraftApp) {
    match app.run("stitch.sortColors", json!({})) {
        Ok(r) if r["changed"] == json!(true) => {
            let n = r["colorChanges"].as_u64().unwrap_or(0);
            app.status(tl!("Sorted by color: {n} color changes").replace("{n}", &n.to_string()));
        }
        Ok(_) => app.status(tl!("Already sorted by color")),
        Err(e) => app.status(e),
    }
}

/// The ≡ menu.
pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    if widgets::menu_item(ui, "Sort by Color", app.session.active().is_some(), false) {
        sort(app);
        ui.close();
    }
    // The selected objects' fill and stroke, either way round.
    let picked = app.session.active().is_some_and(|st| !st.selection.objects.is_empty());
    let fill = widgets::menu_item_name(ui, tl!("Sew Fill First"), picked, false);
    let stroke = widgets::menu_item_name(ui, tl!("Sew Stroke First"), picked, false);
    if fill || stroke {
        if let Err(e) = app.run("stitch.partOrder", json!({ "first": if fill { "fill" } else { "stroke" } })) {
            app.status(e);
        }
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use vectorcraft_engine::Session;

    use super::*;

    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
        // Filled, not stroked: one row each.
        app.session.execute("paint.setStroke", &json!({"none": true})).unwrap();
        for (x, c) in [(0, "#ff0000"), (80, "#0000ff"), (160, "#ff0000")] {
            let id = app.session.execute("shape.rectangle", &json!({"x": x, "y": 10, "width": 72, "height": 72})).unwrap()["id"].as_u64().unwrap();
            app.session.execute("paint.setFill", &json!({"color": c, "ids": [id]})).unwrap();
        }
        app
    }

    fn frame(app: &mut VectorcraftApp) {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| show(app, ui));
        out.textures_delta.clear();
    }

    #[test]
    fn rows_list_objects_and_colour_blocks() {
        let mut app = app();
        let seq = app.session.execute("stitch.sequence", &json!({"thumbs": true})).unwrap();
        let objects = entries(&seq);
        assert_eq!(objects.len(), 3);
        assert_eq!((objects[0].label().as_str(), objects[0].steps[0].thread), ("Tatami", egui::Color32::from_rgb(255, 0, 0)));
        assert!(objects[0].steps[0].thumb.len() >= 2, "a thumbnail of its stitches");
        let colors = color_rows(&seq);
        assert_eq!(colors.len(), 3, "red, blue, red");
        assert!(colors[0].label.starts_with("#FF0000"));
        frame(&mut app);
        app.ui.sequencer_colors = true;
        frame(&mut app);
    }

    #[test]
    fn lines_show_dividers_steps_and_folded_groups() {
        let mut app = app();
        // The first object also gets a stroke: a Fill and a Stroke step under it.
        let seq = app.session.execute("stitch.sequence", &json!({})).unwrap();
        let first = entries(&seq)[0].id;
        app.session.execute("paint.setStroke", &json!({"color": "#00ff00", "ids": [first]})).unwrap();
        let seq = app.session.execute("stitch.sequence", &json!({})).unwrap();
        let es = entries(&seq);
        assert_eq!(es[0].steps.len(), 2, "fill and stroke");
        let ls = lines(&es, 1, &HashSet::new());
        let kinds: Vec<char> = ls
            .iter()
            .map(|l| match l {
                Line::Head { .. } => 'h',
                Line::Divider { .. } => '-',
                Line::Object { .. } => 'o',
                Line::Step { .. } => 's',
            })
            .collect();
        assert_eq!(kinds.iter().collect::<String>(), "-os-s-o-o", "a divider at each colour change, steps under the object");
        // Group the last two: a heading, folding hides them.
        let ids: Vec<u64> = es[1..].iter().map(|e| e.id).collect();
        app.session.execute("select.set", &json!({"ids": ids})).unwrap();
        app.session.execute("object.group", &json!({})).unwrap();
        let seq = app.session.execute("stitch.sequence", &json!({})).unwrap();
        let es = entries(&seq);
        let head = ls_head(&lines(&es, 1, &HashSet::new()));
        let g = head.expect("a group heading");
        let folded = lines(&es, 1, &HashSet::from([g]));
        assert!(folded.iter().all(|l| !matches!(l, Line::Object { entry, .. } if *entry > 0)), "{folded:?}");
        assert!(folded.contains(&Line::Head { id: g, depth: 0 }));
        frame(&mut app);
    }

    fn ls_head(ls: &[Line]) -> Option<u64> {
        ls.iter().find_map(|l| if let Line::Head { id, .. } = l { Some(*id) } else { None })
    }

    #[test]
    fn a_drop_moves_the_row_to_the_gap() {
        let mut app = app();
        let seq = app.session.execute("stitch.sequence", &json!({})).unwrap();
        let rows: Vec<Vec<u64>> = entries(&seq).iter().map(|e| vec![e.id]).collect();
        let id = |i: usize| rows[i][0];
        assert_eq!(move_params(&rows, 0, 0), None);
        assert_eq!(move_params(&rows, 0, 1), None, "the gap right after it");
        assert_eq!(move_params(&rows, 2, 0), Some(json!({"ids": [id(2)], "before": id(0)})));
        assert_eq!(move_params(&rows, 0, 3), Some(json!({"ids": [id(0)], "after": id(2)})));
        run_move(&mut app, move_params(&rows, 0, 3).unwrap());
        let seq = app.session.execute("stitch.sequence", &json!({})).unwrap();
        assert_eq!(entries(&seq).iter().map(|e| e.id).collect::<Vec<_>>(), [id(1), id(2), id(0)]);
    }

    #[test]
    fn sorting_reports_the_colour_changes() {
        let mut app = app();
        sort(&mut app);
        assert!(app.ui.status.contains('1'), "{}", app.ui.status);
        sort(&mut app);
        assert_eq!(app.ui.status, "Already sorted by color");
    }
}
