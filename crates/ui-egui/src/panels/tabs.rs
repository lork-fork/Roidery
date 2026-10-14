//! Tabs panel: tab-stop alignment, X position, leader and "align on", and a ruler on which a click
//! adds a stop, dragging moves one and dragging it off the ruler deletes it.

use egui::{Sense, Stroke, Ui, vec2};
use serde_json::{Value, json};
use vectorcraft_doc::{NodeKind, TabAlign, TabStop};

use super::{first_selected, pstate, set_pstate};
use crate::VectorcraftApp;
use crate::theme::Tokens;
use crate::widgets::{self, menu_item};

const ALIGNS: [(TabAlign, &str, &str); 4] = [
    (TabAlign::Left, "⇥", "Left-Justified Tab"),
    (TabAlign::Center, "⇹", "Center-Justified Tab"),
    (TabAlign::Right, "⇤", "Right-Justified Tab"),
    (TabAlign::Decimal, ".⇥", "Decimal-Justified Tab"),
];

fn align_id(a: TabAlign) -> &'static str {
    match a {
        TabAlign::Left => "left",
        TabAlign::Center => "center",
        TabAlign::Right => "right",
        TabAlign::Decimal => "decimal",
    }
}

/// The first selected text object's stops and the ruler span in points (the frame width for area type).
fn current(app: &VectorcraftApp) -> Option<(Vec<TabStop>, f64)> {
    let n = first_selected(app)?;
    let NodeKind::Text(t) = &n.kind else { return None };
    let span = match &t.kind {
        vectorcraft_doc::TextKind::Area { frame } => frame.bounds().map_or(360.0, |b| b.width()),
        _ => 360.0,
    };
    Some((t.para.tabs.clone(), span.max(72.0)))
}

fn stops_json(stops: &[TabStop]) -> Value {
    Value::Array(
        stops
            .iter()
            .map(|s| json!({"position": s.position, "align": align_id(s.align), "leader": s.leader, "alignOn": s.align_on.to_string()}))
            .collect(),
    )
}

fn apply(app: &mut VectorcraftApp, stops: &[TabStop]) {
    if let Some((id, _, _)) = super::character::text_editing(app) {
        super::character::end_typing(app);
        app.run("text.tabs.set", json!({"stops": stops_json(stops), "ids": [id.0]})).ok();
    } else {
        app.run("text.tabs.set", json!({"stops": stops_json(stops)})).ok();
    }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some((mut stops, span)) = current(app) else {
        super::empty_state(ui, "pilcrow", tl!("No text selected"), tl!("Select a text object to set its tab stops."));
        return;
    };
    let mut sel: usize = pstate(ui.ctx(), "tabs-sel");
    if sel >= stops.len() {
        sel = stops.len().saturating_sub(1);
    }
    let mut align_idx: usize = pstate(ui.ctx(), "tabs-align");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for (i, (a, label, tip)) in ALIGNS.iter().enumerate() {
            let on = stops.get(sel).map_or(align_idx == i, |s| s.align == *a);
            if ui
                .add(egui::Button::new(egui::RichText::new(*label).size(13.0)).selected(on).min_size(vec2(28.0, 24.0)))
                .on_hover_text(tl!(*tip))
                .clicked()
            {
                align_idx = i;
                set_pstate(ui.ctx(), "tabs-align", i);
                if let Some(s) = stops.get_mut(sel) {
                    s.align = *a;
                    apply(app, &stops);
                }
            }
        }
    });
    ui.add_space(4.0);
    egui::Grid::new("tabs-grid").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        ui.label(egui::RichText::new("X:").color(t.text));
        let x = stops.get(sel).map(|s| s.position);
        if let Some(v) = widgets::spin_field(ui, "tabs-x", x, app.session.general_unit(), 90.0, 1.0, 0.0, &[])
            && let Some(s) = stops.get_mut(sel)
        {
            s.position = v.max(0.0);
            stops.sort_by(|a, b| a.position.total_cmp(&b.position));
            apply(app, &stops);
        }
        ui.end_row();
        ui.label(egui::RichText::new(tl!("Leader:")).color(t.text));
        let mut leader = stops.get(sel).map(|s| s.leader.clone()).unwrap_or_default();
        if ui.add_enabled(stops.get(sel).is_some(), egui::TextEdit::singleline(&mut leader).desired_width(90.0)).lost_focus()
            && let Some(s) = stops.get_mut(sel)
            && s.leader != leader
        {
            s.leader = leader.chars().take(8).collect();
            apply(app, &stops);
        }
        ui.end_row();
        ui.label(egui::RichText::new(tl!("Align On:")).color(t.text));
        let decimal = stops.get(sel).is_some_and(|s| s.align == TabAlign::Decimal);
        let mut on = stops.get(sel).map(|s| s.align_on.to_string()).unwrap_or_else(|| ".".into());
        if ui.add_enabled(decimal, egui::TextEdit::singleline(&mut on).desired_width(30.0)).lost_focus()
            && let (Some(s), Some(c)) = (stops.get_mut(sel), on.chars().next())
            && s.align_on != c
        {
            s.align_on = c;
            apply(app, &stops);
        }
        ui.end_row();
    });
    ui.add_space(6.0);
    // The ruler.
    let w = ui.available_width().max(120.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::click_and_drag());
    let p = ui.painter_at(rect.expand(2.0));
    p.rect_filled(rect, 2.0, t.input);
    p.rect_stroke(rect, 2.0, Stroke::new(1.0, t.input_border), egui::StrokeKind::Inside);
    let scale = (w as f64 - 8.0) / span;
    let to_x = |pt: f64| rect.left() + 4.0 + (pt * scale) as f32;
    let to_pt = |x: f32| (((x - rect.left() - 4.0) as f64) / scale).max(0.0);
    let mut tick = 0.0;
    while tick <= span + 1e-6 {
        let x = to_x(tick);
        let major = (tick / 36.0).round() * 36.0 == tick && ((tick / 36.0) as i64) % 2 == 0;
        p.line_segment([egui::pos2(x, rect.bottom() - if major { 10.0 } else { 5.0 }), egui::pos2(x, rect.bottom())], Stroke::new(1.0, t.text_dim));
        if major {
            p.text(
                egui::pos2(x + 2.0, rect.top() + 1.0),
                egui::Align2::LEFT_TOP,
                format!("{}", (tick / 72.0 * 100.0).round() / 100.0),
                egui::FontId::proportional(9.0),
                t.text_dim,
            );
        }
        tick += 18.0;
    }
    for (i, s) in stops.iter().enumerate() {
        let x = to_x(s.position);
        let c = if i == sel { t.accent } else { t.text };
        let y = rect.bottom() - 2.0;
        // An original marker: a stem with a foot showing which way the text runs.
        p.line_segment([egui::pos2(x, y - 12.0), egui::pos2(x, y)], Stroke::new(1.5, c));
        let foot = match s.align {
            TabAlign::Left => [egui::pos2(x, y), egui::pos2(x + 6.0, y)],
            TabAlign::Right => [egui::pos2(x - 6.0, y), egui::pos2(x, y)],
            TabAlign::Center | TabAlign::Decimal => [egui::pos2(x - 4.0, y), egui::pos2(x + 4.0, y)],
        };
        p.line_segment(foot, Stroke::new(1.5, c));
        if s.align == TabAlign::Decimal {
            p.circle_filled(egui::pos2(x + 3.0, y - 6.0), 1.5, c);
        }
    }
    let drag: Option<usize> = pstate(ui.ctx(), "tabs-drag");
    if resp.drag_started()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let hit = stops.iter().position(|s| (to_x(s.position) - pos.x).abs() <= 5.0);
        set_pstate(ui.ctx(), "tabs-drag", hit);
        if let Some(i) = hit {
            set_pstate(ui.ctx(), "tabs-sel", i);
        }
    }
    if resp.drag_stopped()
        && let (Some(i), Some(pos)) = (drag, resp.interact_pointer_pos())
    {
        set_pstate::<Option<usize>>(ui.ctx(), "tabs-drag", None);
        if i < stops.len() {
            if pos.y > rect.bottom() + 20.0 {
                // Dragged off the ruler: delete.
                stops.remove(i);
            } else {
                stops[i].position = to_pt(pos.x);
                stops.sort_by(|a, b| a.position.total_cmp(&b.position));
            }
            apply(app, &stops);
        }
    } else if resp.clicked()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        match stops.iter().position(|s| (to_x(s.position) - pos.x).abs() <= 5.0) {
            Some(i) => set_pstate(ui.ctx(), "tabs-sel", i),
            None => {
                let position = to_pt(pos.x);
                stops.push(TabStop { position, align: ALIGNS[align_idx.min(3)].0, leader: String::new(), align_on: '.' });
                stops.sort_by(|a, b| a.position.total_cmp(&b.position));
                let i = stops.iter().position(|s| s.position == position).unwrap_or(0);
                set_pstate(ui.ctx(), "tabs-sel", i);
                apply(app, &stops);
            }
        }
    }
    resp.on_hover_text(tl!("Click to add a tab stop, drag to move it, drag it off the ruler to delete it"));
}

pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Some((mut stops, _)) = current(app) else {
        ui.add_enabled(false, egui::Button::new(tl!("Select text")).frame(false));
        return;
    };
    let sel: usize = pstate(ui.ctx(), "tabs-sel");
    if menu_item(ui, tl!("Clear All Tabs"), false, !stops.is_empty()) {
        app.run("text.tabs.clear", json!({})).ok();
    }
    if menu_item(ui, tl!("Delete Tab"), false, sel < stops.len()) {
        stops.remove(sel);
        apply(app, &stops);
    }
    // Repeat Tab: copies of the selected stop at its distance from the previous one, across the ruler.
    if menu_item(ui, tl!("Repeat Tab"), false, sel < stops.len()) {
        let prev = if sel == 0 { 0.0 } else { stops[sel - 1].position };
        let step = (stops[sel].position - prev).max(1.0);
        let base = stops[sel].clone();
        stops.truncate(sel + 1);
        let mut pos = base.position + step;
        while pos <= base.position + step * 20.0 && pos < 2000.0 {
            stops.push(TabStop { position: pos, ..base.clone() });
            pos += step;
        }
        apply(app, &stops);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

    fn frame(app: &mut VectorcraftApp) {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(app, ui);
            menu(app, ui);
        });
        out.textures_delta.clear();
    }

    #[test]
    fn panel_draws_empty_with_text_and_with_stops() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({})).unwrap();
        frame(&mut app);
        app.session.execute("text.create", &json!({"x": 10, "y": 20, "text": "a\tb", "area": {"width": 300, "height": 100}})).unwrap();
        frame(&mut app);
        app.session.execute("text.tabs.set", &json!({"stops": [{"position": 50}, {"position": 120, "align": "decimal"}]})).unwrap();
        frame(&mut app);
        let (stops, span) = current(&app).unwrap();
        assert_eq!(stops.len(), 2);
        assert_eq!(span, 300.0);
        // The stops survive a JSON round trip through the panel's encoding.
        let back: Vec<serde_json::Value> = stops_json(&stops).as_array().unwrap().clone();
        assert_eq!(back[1]["align"], json!("decimal"));
    }
}
