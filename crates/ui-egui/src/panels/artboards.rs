//! Artboards panel: numbered list with inline rename (double-click the name), move up / down, new,
//! delete. The highlighted row is the active artboard, the one the status bar's navigator shows and
//! Fit Artboard in Window fits: clicking a row makes it active, double-clicking its number also
//! fits it in the window.

use egui::{Sense, Ui, pos2, vec2};
use serde_json::json;

use super::{pstate, set_pstate};
use crate::theme::Tokens;
use crate::widgets::{self, menu_item};
use crate::{VectorcraftApp, icons};

/// The active artboard (of `n`).
fn selected(app: &VectorcraftApp, n: usize) -> usize {
    app.view().map_or(0, |v| v.artboard).min(n.saturating_sub(1))
}

/// Make artboard `i` the active one, leaving the view where it is: the navigator's and, while it
/// is the tool, the Artboard tool's.
fn select(app: &mut VectorcraftApp, i: usize) {
    if let Some(v) = app.view_mut() {
        v.artboard = i;
    }
    if app.session.tool_id() == "artboard" {
        app.session.set_tool_option("active", &json!(i));
    }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let abs: Vec<String> = app.session.active().map(|d| d.doc.artboards.iter().map(|a| a.name.clone()).collect()).unwrap_or_default();
    if abs.is_empty() {
        super::empty_state(ui, "dc-artboards", tl!("No document"), tl!("Open a document to see its artboards."));
        return;
    }
    let sel = selected(app, abs.len());
    let editing: Option<usize> = pstate(ui.ctx(), "ab-edit");
    widgets::list_box(ui, |ui| {
        ui.set_min_height(110.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().id_salt("ab-scroll").max_height(220.0).show(ui, |ui| {
            for (i, name) in abs.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
                if i == sel {
                    ui.painter().rect_filled(r, 0.0, t.row_selected);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, t.hover);
                }
                ui.painter().text(
                    pos2(r.left() + 22.0, r.center().y),
                    egui::Align2::RIGHT_CENTER,
                    format!("{}", i + 1),
                    egui::FontId::proportional(12.0),
                    t.text_dim,
                );
                let name_rect = egui::Rect::from_min_max(pos2(r.left() + 32.0, r.top() + 2.0), pos2(r.right() - 28.0, r.bottom() - 2.0));
                if editing == Some(i) {
                    let id = ui.id().with(("ab-name", i));
                    let mut buf: String = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| name.clone());
                    let te = ui.put(name_rect, egui::TextEdit::singleline(&mut buf).id(id).font(egui::FontId::proportional(12.0)));
                    if !te.has_focus() && !te.lost_focus() {
                        te.request_focus();
                    }
                    ui.data_mut(|d| d.insert_temp(id, buf.clone()));
                    if te.lost_focus() {
                        set_pstate::<Option<usize>>(ui.ctx(), "ab-edit", None);
                        ui.data_mut(|d| d.remove::<String>(id));
                        if !ui.input(|i| i.key_pressed(egui::Key::Escape)) && !buf.trim().is_empty() && buf != *name {
                            app.run("artboard.setProps", json!({"index": i, "name": buf.trim()})).ok();
                        }
                    }
                } else {
                    ui.painter().text(name_rect.left_center(), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(12.5), t.text);
                }
                let opt = egui::Rect::from_center_size(r.right_center() - vec2(14.0, 0.0), vec2(14.0, 14.0));
                let oresp = ui.interact(opt, ui.id().with(("ab-opt", i)), Sense::click());
                icons::paint(ui, "dc-artboard-options", opt, if oresp.hovered() { t.text_strong } else { t.icon });
                if oresp.on_hover_text(tl!("Artboard Options: edit with the Artboard tool")).clicked() {
                    app.select_tool("artboard");
                    select(app, i);
                }
                if resp.clicked() {
                    select(app, i);
                }
                // Double-clicking the number goes to the artboard (fits it in the window); the name
                // renames it.
                let on_number = resp.interact_pointer_pos().is_some_and(|p| p.x < name_rect.left());
                if resp.double_clicked() && on_number {
                    app.run("view.goToArtboard", json!({ "index": i })).ok();
                } else if resp.double_clicked() {
                    set_pstate(ui.ctx(), "ab-edit", Some(i));
                }
            }
        });
    });
    let n = abs.len();
    widgets::bottom_bar(ui, |ui| {
        widgets::icon_button_enabled(ui, "dc-rearrange", tl!("Rearrange All Artboards (on the roadmap)"), false, false, 24.0);
        ui.add_space((ui.available_width() - 4.0 * 28.0).max(0.0));
        if widgets::icon_button_enabled(ui, "dc-arrow-up", tl!("Move Up"), false, sel > 0, 24.0).clicked()
            && app.run("artboard.reorder", json!({"index": sel, "to": sel - 1})).is_ok()
        {
            select(app, sel - 1);
        }
        if widgets::icon_button_enabled(ui, "dc-arrow-down", tl!("Move Down"), false, sel + 1 < n, 24.0).clicked()
            && app.run("artboard.reorder", json!({"index": sel, "to": sel + 1})).is_ok()
        {
            select(app, sel + 1);
        }
        if widgets::icon_button(ui, "dc-new-item", tl!("New Artboard"), false, 24.0).clicked() && app.run("artboard.new", json!({})).is_ok() {
            select(app, n);
        }
        if widgets::icon_button_enabled(ui, "trash-2", tl!("Delete Artboard"), false, n > 1, 24.0).clicked() {
            app.run("artboard.delete", json!({"index": sel})).ok();
        }
    });
}

pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    let n = app.session.active().map(|d| d.doc.artboards.len()).unwrap_or(0);
    let sel = selected(app, n);
    if menu_item(ui, tl!("New Artboard"), n > 0, false) {
        app.run("artboard.new", json!({})).ok();
    }
    if menu_item(ui, tl!("Duplicate Artboards"), n > 0, false) {
        app.run("artboard.duplicate", json!({"index": sel})).ok();
    }
    if menu_item(ui, tl!("Delete Artboards"), n > 1, false) {
        app.run("artboard.delete", json!({"index": sel})).ok();
    }
    if menu_item(ui, tl!("Rename"), n > 0, false) {
        set_pstate(ui.ctx(), "ab-edit", Some(sel));
    }
    menu_item(ui, tl!("Delete Empty Artboards"), false, false);
    ui.separator();
    menu_item(ui, tl!("Convert to Artboards"), false, false);
    if menu_item(ui, tl!("Artboard Options…"), n > 0, false) {
        app.select_tool("artboard");
    }
    menu_item(ui, tl!("Rearrange All Artboards…"), false, false);
    ui.separator();
    if menu_item(ui, tl!("Fit to Artwork Bounds"), n > 0, false) {
        app.run("artboard.fitToArt", json!({"index": sel})).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

    /// Clicking a row makes its artboard the active one (the navigator's, Fit Artboard in Window's).
    #[test]
    fn clicking_a_row_makes_its_artboard_active() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 200, "height": 150})).unwrap();
        app.session.execute("artboard.new", &json!({"x": 400, "y": 300, "width": 200, "height": 150})).unwrap();
        app.view_mut().unwrap().artboard = 0;
        let ctx = egui::Context::default();
        let frame = |app: &mut VectorcraftApp, events: Vec<egui::Event>| {
            let mut out = ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| show(app, ui));
            out.textures_delta.clear();
            out.shapes
        };
        // Where the second row's name is painted.
        let at = frame(&mut app, vec![])
            .iter()
            .find_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text() == "Artboard 2" => Some(t.pos + vec2(4.0, 4.0)),
                _ => None,
            })
            .unwrap();
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        for e in [egui::Event::PointerMoved(at), button(true), button(false)] {
            frame(&mut app, vec![e]);
        }
        assert_eq!(app.view().unwrap().artboard, 1, "Artboard 2 is active");
        // The navigator moves it on; the panel follows.
        crate::menus::invoke(&mut app, "view.goToArtboard", json!({"index": "previous"}));
        assert_eq!(selected(&app, 2), 0, "and the panel follows the navigator");
    }
    /// Double-clicking a row's number goes to its artboard and its name renames it; a row's
    /// Options button edits that artboard with the Artboard tool, and the tool's artboard is the
    /// active one.
    #[test]
    fn double_clicks_go_to_or_rename_and_options_edits_the_rows_artboard() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 200, "height": 150})).unwrap();
        app.session.execute("artboard.new", &json!({"x": 400, "y": 300, "width": 200, "height": 150})).unwrap();
        app.view_mut().unwrap().artboard = 0;
        let ctx = egui::Context::default();
        let screen = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(240.0, 400.0)));
        let frame = |app: &mut VectorcraftApp, time: f64, events: Vec<egui::Event>| {
            let mut out = ctx.run_ui(egui::RawInput { events, time: Some(time), screen_rect: screen, ..Default::default() }, |ui| show(app, ui));
            out.textures_delta.clear();
            out.shapes
        };
        let shapes = frame(&mut app, 0.0, vec![]);
        let name = shapes
            .iter()
            .find_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text() == "Artboard 2" => Some(t.pos + vec2(4.0, 4.0)),
                _ => None,
            })
            .unwrap();
        // The highlighted row (Artboard 1's), for its Options button at the right end.
        let row = shapes
            .iter()
            .find_map(|c| match &c.shape {
                egui::Shape::Rect(r) if r.fill == Tokens::get(&ctx).row_selected => Some(r.rect),
                _ => None,
            })
            .unwrap();
        let events = |at: egui::Pos2, clicks: usize| {
            let button =
                |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
            std::iter::once(egui::Event::PointerMoved(at)).chain((0..clicks).flat_map(|_| [button(true), button(false)])).collect::<Vec<_>>()
        };
        // Artboard 2's number: it is active, no rename starts.
        frame(&mut app, 1.0, events(name - vec2(16.0, 0.0), 2));
        assert_eq!(app.view().unwrap().artboard, 1);
        assert_eq!(pstate::<Option<usize>>(&ctx, "ab-edit"), None);
        // Its name: a rename.
        frame(&mut app, 2.0, events(name, 2));
        assert_eq!(pstate::<Option<usize>>(&ctx, "ab-edit"), Some(1));
        set_pstate::<Option<usize>>(&ctx, "ab-edit", None);
        frame(&mut app, 3.0, vec![]);
        // Artboard 1's Options button: the Artboard tool, editing Artboard 1.
        frame(&mut app, 4.0, events(egui::pos2(row.right() - 14.0, row.center().y), 1));
        assert_eq!(app.session.tool_id(), "artboard");
        assert_eq!((app.view().unwrap().artboard, app.session.tool_options()["active"].clone()), (0, json!(0)));
        // While the Artboard tool is chosen, clicking a row moves the tool to that artboard too.
        frame(&mut app, 5.0, events(name, 1));
        assert_eq!(app.session.tool_options()["active"], json!(1));
        // And clicking an artboard with the tool makes it the active one, highlighted in the panel.
        let view = app.view_info();
        for kind in [vectorcraft_tools::PointerKind::Down, vectorcraft_tools::PointerKind::Up] {
            crate::canvas::dispatch(&mut app, &vectorcraft_tools::PointerEvent::new(kind, 100.0, 100.0), view);
        }
        assert_eq!((app.session.tool_options()["active"].clone(), selected(&app, 2)), (json!(0), 0));
    }
}
