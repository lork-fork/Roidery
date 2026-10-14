//! Character panel: font family / style, size, leading, kerning, tracking, vertical and horizontal
//! scale, baseline shift, character rotation and the caps / underline / strikethrough toggles.

use std::sync::{Mutex, OnceLock};

use egui::{Key, Ui, vec2};
use serde_json::{Value, json};
use vectorcraft_doc::{CharStyle, NodeId, NodeKind};
use vectorcraft_tools::{Mods, ToolKey};

use super::{first_selected, pstate, set_pstate};
use crate::VectorcraftApp;
use crate::theme::Tokens;
use crate::widgets::{self, menu_item};

pub const SIZE_PRESETS: [f64; 16] = [6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 18.0, 21.0, 24.0, 36.0, 48.0, 60.0, 72.0, 96.0];
pub const TRACKING_PRESETS: [f64; 13] = [-100.0, -75.0, -50.0, -25.0, -10.0, -5.0, 0.0, 5.0, 10.0, 25.0, 50.0, 75.0, 100.0];
pub const SCALE_PRESETS: [f64; 8] = [25.0, 50.0, 75.0, 90.0, 100.0, 110.0, 125.0, 150.0];

/// The character style shown by the panel: the selected range's (or the caret's) while the Type
/// tool edits text, else the first selected text object's first run.
pub(crate) fn text_style(app: &VectorcraftApp) -> Option<(CharStyle, vectorcraft_doc::ParaStyle)> {
    if let Some((id, a, b)) = text_editing(app)
        && let Some(NodeKind::Text(t)) = app.session.active().and_then(|d| d.doc.node(id)).map(|n| &n.kind)
    {
        return Some((vectorcraft_text::edit::insertion_style(&t.runs, a, b), t.para.clone()));
    }
    let n = first_selected(app)?;
    match &n.kind {
        NodeKind::Text(t) => Some((t.first_style(), t.para.clone())),
        _ => None,
    }
}

fn style(app: &mut VectorcraftApp, p: Value) {
    char_cmd(app, "text.setStyle", p);
}
fn format(app: &mut VectorcraftApp, p: Value) {
    char_cmd(app, "text.setFormat", p);
}

/// Apply character attributes: to the selected range while the Type tool edits text (the whole
/// object when nothing is selected), else to the selected text objects.
fn char_cmd(app: &mut VectorcraftApp, cmd: &str, p: Value) {
    if text_editing(app).is_none() {
        app.run(cmd, p).ok();
        return;
    }
    range_style(app, p);
}

/// `text.setRangeStyle` with `p` on the range the Type tool has selected (the whole text when
/// nothing is selected). Does nothing unless the Type tool edits text.
pub(crate) fn range_style(app: &mut VectorcraftApp, p: Value) {
    let Some((id, a, b)) = text_editing(app) else { return };
    end_typing(app);
    let mut q = p;
    q["id"] = json!(id.0);
    if b > a {
        q["start"] = json!(a);
        q["end"] = json!(b);
    }
    app.run("text.setRangeStyle", q).ok();
}

// ---------- Type tool editing glue (selection, clipboard, keys) ----------

/// The text object the Type tool is editing and its selection (clamped to the text).
pub(crate) fn text_editing(app: &VectorcraftApp) -> Option<(NodeId, usize, usize)> {
    let o = app.session.tool_options();
    let id = NodeId(o.get("editing")?.as_u64()?);
    let len = match &app.session.active()?.doc.node(id)?.kind {
        NodeKind::Text(t) => t.plain_text().len(),
        _ => return None,
    };
    let g = |k: &str| o.get(k).and_then(Value::as_u64).map_or(0, |v| (v as usize).min(len));
    Some((id, g("start"), g("end")))
}

/// Close the Type tool's typing session (one undo step) before another command edits the text.
pub(crate) fn end_typing(app: &mut VectorcraftApp) {
    if app.session.tool_options().get("typing").and_then(Value::as_bool) == Some(true) {
        app.session.set_tool_option("commitTyping", &json!(true));
        app.session.commit_interaction().ok();
        app.sync_views();
    }
}

static CTX: OnceLock<egui::Context> = OnceLock::new();
/// Plain text of the last text copied inside the Type tool (paste from the native menu, which
/// can't read the system clipboard).
static CLIP: Mutex<String> = Mutex::new(String::new());

fn text_copy(app: &mut VectorcraftApp) -> bool {
    let Some((id, a, b)) = text_editing(app) else { return false };
    if a == b {
        return false;
    }
    let Ok(r) = app.session.execute("text.getRange", &json!({"id": id.0, "start": a, "end": b})) else { return false };
    app.session.set_tool_option("copy", &r["runs"]);
    let text = r["text"].as_str().unwrap_or("").to_string();
    if let Some(ctx) = CTX.get() {
        ctx.copy_text(text.clone());
    }
    *CLIP.lock().unwrap_or_else(|e| e.into_inner()) = text;
    true
}

fn text_key(app: &mut VectorcraftApp, k: ToolKey, m: Mods) {
    let v = app.view_info();
    if let Err(e) = app.session.tool_key(k, m, v) {
        app.status(e.to_string());
    }
}

fn text_paste(app: &mut VectorcraftApp, s: Option<String>) {
    let s = s.unwrap_or_else(|| CLIP.lock().unwrap_or_else(|e| e.into_inner()).clone());
    if !s.is_empty() {
        let v = app.view_info();
        if let Err(e) = app.session.tool_text(&s, v) {
            app.status(e.to_string());
        }
    }
}

/// Edit-menu commands while the Type tool edits text act on the text (Cut/Copy/Paste/Clear;
/// Select All is the engine's `select.all`). `None` = not intercepted.
pub(crate) fn intercept_text_command(app: &mut VectorcraftApp, id: &str) -> Option<Result<Value, String>> {
    if !app.session.tool_wants_text() {
        return None;
    }
    match id {
        "edit.copy" => {
            text_copy(app);
        }
        "edit.cut" => {
            if text_copy(app) {
                text_key(app, ToolKey::Delete, Mods::default());
            }
        }
        "edit.paste" | "edit.pasteWithoutFormatting" => text_paste(app, None),
        "edit.clear" => text_key(app, ToolKey::Delete, Mods::default()),
        _ => return None,
    }
    app.sync_views();
    Some(Ok(Value::Null))
}

enum TextInput {
    Key(ToolKey, Mods),
    Copy,
    Cut,
    Paste(Option<String>),
}

/// Route editing keys (with modifiers), clipboard events and Cmd+C/X/V to the Type tool (Cmd+A is
/// left to the Select All shortcut, which selects the text being edited).
pub(crate) fn route_type_input(app: &mut VectorcraftApp, ctx: &egui::Context) {
    CTX.get_or_init(|| ctx.clone());
    let mut todo = vec![];
    ctx.input_mut(|i| {
        i.events.retain(|e| match e {
            egui::Event::Copy => {
                todo.push(TextInput::Copy);
                false
            }
            egui::Event::Cut => {
                todo.push(TextInput::Cut);
                false
            }
            egui::Event::Paste(s) => {
                todo.push(TextInput::Paste(Some(s.clone())));
                false
            }
            egui::Event::Key { key, pressed, modifiers: m, .. } => {
                let mods = Mods { shift: m.shift, alt: m.alt, cmd: m.command, ctrl: m.ctrl && !m.command, space: false };
                let tk = match key {
                    Key::ArrowLeft => Some(ToolKey::Left),
                    Key::ArrowRight => Some(ToolKey::Right),
                    Key::ArrowUp => Some(ToolKey::Up),
                    Key::ArrowDown => Some(ToolKey::Down),
                    Key::Home => Some(ToolKey::Home),
                    Key::End => Some(ToolKey::End),
                    Key::Backspace => Some(ToolKey::Backspace),
                    Key::Delete => Some(ToolKey::Delete),
                    Key::Tab if !m.command => Some(ToolKey::Tab),
                    _ => None,
                };
                if let Some(tk) = tk {
                    if *pressed {
                        todo.push(TextInput::Key(tk, mods));
                    }
                    return false;
                }
                if m.command && !m.shift && !m.alt && matches!(key, Key::C | Key::X | Key::V) {
                    if *pressed {
                        todo.push(match key {
                            Key::C => TextInput::Copy,
                            Key::X => TextInput::Cut,
                            _ => TextInput::Paste(None),
                        });
                    }
                    return false;
                }
                true
            }
            _ => true,
        })
    });
    for t in todo {
        match t {
            TextInput::Key(k, m) => text_key(app, k, m),
            TextInput::Copy => {
                text_copy(app);
            }
            TextInput::Cut => {
                if text_copy(app) {
                    text_key(app, ToolKey::Delete, Mods::default());
                }
            }
            TextInput::Paste(s) => text_paste(app, s),
        }
    }
}

/// A compact labelled cell: short glyph label + spinner field.
fn cell(ui: &mut Ui, label: &str, tip: &str, add: impl FnOnce(&mut Ui)) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        let l =
            ui.add_sized(vec2(22.0, 24.0), egui::Label::new(egui::RichText::new(label).size(11.5).strong().color(t.text))).on_hover_text(tl!(tip));
        crate::scrub::note_label(ui, l.rect);
        add(ui);
    });
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Some((s, _)) = text_style(app) else {
        super::empty_state(ui, "type", tl!("No text selected"), tl!("Select a text object to edit its character attributes."));
        return;
    };
    let w = ui.available_width();
    let sample = crate::font_menu::sample_text(app);
    if let Some(pick) = crate::font_menu::font_menu(ui, "ch-font", &s.font_family, w - 4.0, sample.as_deref(), crate::font_menu::MenuLook::of(app)) {
        crate::font_menu::apply(app, ui.ctx(), pick);
    }
    let styles = vectorcraft_text::FontDb::global().styles(&s.font_family);
    let snames: Vec<&str> = styles.iter().map(String::as_str).collect();
    if let Some(i) = widgets::dropdown_names(ui, "ch-style", &s.font_style, &snames, w - 4.0) {
        style(app, json!({"style": snames[i]}));
    }
    ui.add_space(2.0);
    let fw = ((w - 66.0) / 2.0).clamp(60.0, 100.0);
    let unit = app.session.type_unit();
    egui::Grid::new("ch-grid").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        cell(ui, "T", tl!("Font Size"), |ui| {
            if let Some(v) = widgets::spin_field(ui, "ch-size", Some(s.size), unit, fw, 1.0, 0.1, &SIZE_PRESETS) {
                style(app, json!({"size": v}));
            }
        });
        cell(ui, "A↕", tl!("Leading"), |ui| {
            let presets: Vec<f64> = SIZE_PRESETS.iter().map(|v| v * 1.2).collect();
            if let Some(v) = widgets::spin_field(ui, "ch-lead", Some(s.effective_leading()), unit, fw, 1.0, 0.1, &presets) {
                style(app, json!({"leading": v}));
            }
        });
        ui.end_row();
        cell(ui, "VA", tl!("Kerning (0 = Auto)"), |ui| {
            if let Some(v) = widgets::spin_plain(ui, "ch-kern", s.kerning.unwrap_or(0.0), "", 0, fw, 10.0, -1000.0, &TRACKING_PRESETS) {
                format(app, if v == 0.0 { json!({"kerning": "auto"}) } else { json!({"kerning": v}) });
            }
        });
        cell(ui, "VA↔", tl!("Tracking"), |ui| {
            if let Some(v) = widgets::spin_plain(ui, "ch-track", s.tracking, "", 0, fw, 10.0, -1000.0, &TRACKING_PRESETS) {
                style(app, json!({"tracking": v}));
            }
        });
        ui.end_row();
        if !pstate::<bool>(ui.ctx(), "ch-hide-options") {
            cell(ui, "IT", tl!("Vertical Scale %"), |ui| {
                if let Some(v) = widgets::spin_plain(ui, "ch-vs", s.v_scale, "%", 1, fw, 1.0, 1.0, &SCALE_PRESETS) {
                    format(app, json!({"vScale": v}));
                }
            });
            cell(ui, "T↔", tl!("Horizontal Scale %"), |ui| {
                if let Some(v) = widgets::spin_plain(ui, "ch-hs", s.h_scale, "%", 1, fw, 1.0, 1.0, &SCALE_PRESETS) {
                    format(app, json!({"hScale": v}));
                }
            });
            ui.end_row();
            cell(ui, "Aª", tl!("Baseline Shift"), |ui| {
                if let Some(v) = widgets::spin_field(ui, "ch-bs", Some(s.baseline_shift), unit, fw, 1.0, -1296.0, &[]) {
                    format(app, json!({"baselineShift": v}));
                }
            });
            cell(ui, "⟲T", tl!("Character Rotation"), |ui| {
                if let Some(v) = widgets::spin_plain(ui, "ch-rot", s.rotation, "°", 1, fw, 15.0, -360.0, &super::transform::ANGLE_PRESETS) {
                    format(app, json!({"rotation": v}));
                }
            });
            ui.end_row();
        }
    });
    if pstate::<bool>(ui.ctx(), "ch-hide-options") {
        return;
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for (kind, tip, key, on, enabled) in [
            (Glyph::AllCaps, tl!("All Caps"), "allCaps", s.all_caps, true),
            (Glyph::SmallCaps, tl!("Small Caps (on the roadmap)"), "", false, false),
            (Glyph::Super, tl!("Superscript (on the roadmap)"), "", false, false),
            (Glyph::Sub, tl!("Subscript (on the roadmap)"), "", false, false),
            (Glyph::Underline, tl!("Underline"), "underline", s.underline, true),
            (Glyph::Strike, tl!("Strikethrough"), "strikethrough", s.strikethrough, true),
        ] {
            if style_toggle(ui, kind, tip, on, enabled) && !key.is_empty() {
                format(app, json!({key: !on}));
            }
        }
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Glyph {
    AllCaps,
    SmallCaps,
    Super,
    Sub,
    Underline,
    Strike,
}

/// A drawn "T" style toggle (All Caps, Small Caps, Superscript, Subscript, Underline, Strikethrough).
fn style_toggle(ui: &mut Ui, g: Glyph, tip: &str, on: bool, enabled: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(30.0, 26.0), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    if on {
        ui.painter().rect_filled(r, 3, t.tool_active);
    } else if resp.hovered() && enabled {
        ui.painter().rect_filled(r, 3, t.hover);
    }
    let col = if enabled { t.text_strong } else { t.text_disabled };
    let big = egui::FontId::proportional(15.0);
    let small = egui::FontId::proportional(10.5);
    let c = r.center();
    let p = ui.painter();
    match g {
        Glyph::AllCaps => {
            p.text(c - vec2(5.0, 0.0), egui::Align2::CENTER_CENTER, "T", big.clone(), col);
            p.text(c + vec2(5.0, 0.0), egui::Align2::CENTER_CENTER, "T", big, col);
        }
        Glyph::SmallCaps => {
            p.text(c - vec2(4.0, 0.0), egui::Align2::CENTER_CENTER, "T", big, col);
            p.text(c + vec2(6.0, 2.0), egui::Align2::CENTER_CENTER, "T", small, col);
        }
        Glyph::Super => {
            p.text(c - vec2(3.0, 0.0), egui::Align2::CENTER_CENTER, "T", big, col);
            p.text(c + vec2(6.0, -5.0), egui::Align2::CENTER_CENTER, "1", small, col);
        }
        Glyph::Sub => {
            p.text(c - vec2(3.0, 0.0), egui::Align2::CENTER_CENTER, "T", big, col);
            p.text(c + vec2(6.0, 5.0), egui::Align2::CENTER_CENTER, "1", small, col);
        }
        Glyph::Underline => {
            p.text(c - vec2(0.0, 1.0), egui::Align2::CENTER_CENTER, "T", big, col);
            p.line_segment([c + vec2(-5.0, 7.0), c + vec2(5.0, 7.0)], egui::Stroke::new(1.2, col));
        }
        Glyph::Strike => {
            p.text(c, egui::Align2::CENTER_CENTER, "T", big, col);
            p.line_segment([c + vec2(-6.0, 1.0), c + vec2(6.0, 1.0)], egui::Stroke::new(1.2, col));
        }
    }
    let resp = resp.on_hover_text(tl!(tip));
    enabled && resp.clicked()
}

pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    let st = text_style(app);
    let has = st.is_some();
    let hidden: bool = pstate(ui.ctx(), "ch-hide-options");
    if menu_item(ui, if hidden { tl!("Show Options") } else { tl!("Hide Options") }, true, false) {
        set_pstate(ui.ctx(), "ch-hide-options", !hidden);
    }
    ui.separator();
    let s = st.map(|x| x.0);
    for (label, key, on) in [
        (tl!("All Caps"), "allCaps", s.as_ref().is_some_and(|s| s.all_caps)),
        (tl!("Underline"), "underline", s.as_ref().is_some_and(|s| s.underline)),
        (tl!("Strikethrough"), "strikethrough", s.as_ref().is_some_and(|s| s.strikethrough)),
    ] {
        if menu_item(ui, label, has, on) {
            format(app, json!({key: !on}));
        }
    }
    for l in ["Small Caps", "Superscript", "Subscript"] {
        menu_item(ui, tl!(l), false, false);
    }
    ui.separator();
    // Character Alignment: where characters smaller than the largest on their line line up, with
    // the East Asian options.
    if app.session.prefs.show_east_asian_options {
        let align = s.as_ref().map(|s| s.char_align);
        ui.add_enabled_ui(has, |ui| {
            // Indented like the items beside it (their check column).
            ui.menu_button(format!("   {}", tl!("Character Alignment")), |ui| {
                use vectorcraft_doc::CharAlign;
                for (label, a, key) in [
                    (tl!("Roman Baseline"), CharAlign::RomanBaseline, "romanBaseline"),
                    (tl!("Em Box Top/Right"), CharAlign::EmBoxTop, "emBoxTop"),
                    (tl!("Em Box Center"), CharAlign::EmBoxCenter, "emBoxCenter"),
                    (tl!("Em Box Bottom/Left"), CharAlign::EmBoxBottom, "emBoxBottom"),
                ] {
                    if menu_item(ui, label, true, align == Some(a)) {
                        format(app, json!({"charAlign": key}));
                    }
                }
            });
        });
        ui.separator();
    }
    for l in ["Standard Vertical Roman Alignment", "Tate-chu-yoko", "Fractional Widths", "System Layout", "No Break"] {
        menu_item(ui, tl!(l), false, l == "Fractional Widths");
    }
    ui.separator();
    // The installed fonts are always listed; this picks up fonts installed since the app started.
    #[cfg(not(target_arch = "wasm32"))]
    if menu_item(ui, tl!("Refresh Font List"), true, false)
        && let Ok(r) = app.run("text.rescanFonts", json!({}))
    {
        app.status(format!("{} font families available", r["families"]));
    }
    if menu_item(ui, tl!("Reset Panel"), has, false) {
        style(app, json!({"tracking": 0, "leading": "auto"}));
        format(
            app,
            json!({"kerning": "auto", "baselineShift": 0, "hScale": 100, "vScale": 100, "rotation": 0, "underline": false, "strikethrough": false, "allCaps": false}),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

    /// Character Alignment is in the panel menu with the East Asian options only (as Mojikumi Set
    /// and Top-to-Top Leading are in the Paragraph panel).
    #[test]
    fn character_alignment_shows_with_the_east_asian_options() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 200})).unwrap();
        let id = app.session.execute("text.create", &json!({"x": 20, "y": 50, "text": "雅楽"})).unwrap()["id"].clone();
        app.session.execute("select.set", &json!({"ids": [id]})).unwrap();
        let shown = |app: &mut VectorcraftApp| crate::tests_labels::painted_text(app, menu).contains("Character Alignment");
        assert!(!shown(&mut app));
        app.session.prefs.show_east_asian_options = true;
        assert!(shown(&mut app));
    }
}
