//! Roidery: the Check Design panel. Lists what will sew badly (`stitch.check`), errors first:
//! clicking a problem selects its objects and brings it into view, and Fix runs the command that
//! fixes it. While the panel is open the canvas rings each problem where it is (`paint`).

use egui::{Color32, Painter, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use vectorcraft_geom::Point;

use crate::theme::Tokens;
use crate::{VectorcraftApp, widgets};

pub const ID: &str = "stitchCheck";
/// A problem's row: its name and measurement, and the Fix button under them when it has one.
const ROW_H: f32 = 44.0;
const FIX_H: f32 = 28.0;

/// `stitch.check`, recomputed only when the document changes.
pub(crate) fn problems(app: &mut VectorcraftApp, ctx: &egui::Context) -> Option<Value> {
    type Key = (u64, u64);
    let st = app.session.active()?;
    let key: Key = (st.uid, st.revision);
    let id = egui::Id::new("stitch-check");
    if let Some((k, v)) = ctx.data(|d| d.get_temp::<(Key, Value)>(id))
        && k == key
    {
        return Some(v);
    }
    // Every object, whatever is selected (the selection changes as problems are clicked).
    let v = app.session.execute("stitch.check", &json!({ "ids": [] })).ok()?;
    ctx.data_mut(|d| d.insert_temp(id, (key, v.clone())));
    Some(v)
}

/// A problem's name.
pub(crate) fn title(kind: &str) -> &'static str {
    match kind {
        "outsideHoop" => tl!("Outside the hoop"),
        "tooLarge" => tl!("Too large for the hoop"),
        "tooDense" => tl!("Too dense"),
        "thinFill" => tl!("Too thin to fill"),
        "wideSatin" => tl!("Satin too wide"),
        "narrowSatin" => tl!("Satin too narrow"),
        _ => tl!("Too small to sew"),
    }
}

/// The measurement under a problem's name.
fn detail(p: &Value) -> String {
    let v = format!("{:.1}", p["value"].as_f64().unwrap_or(0.0));
    let s = match p["kind"].as_str().unwrap_or("") {
        "outsideHoop" => tl!("{d} mm outside the hoop"),
        "tooLarge" => tl!("{d} mm too large"),
        "tooDense" => tl!("{d} layers of thread"),
        "thinFill" | "wideSatin" | "narrowSatin" => tl!("{d} mm wide"),
        _ => tl!("{d} mm across"),
    };
    s.replace("{d}", &v)
}

/// Why it is a problem and what to do, for the tooltip.
fn hint(kind: &str) -> &'static str {
    match kind {
        "outsideHoop" => tl!("The machine can't reach past the hoop's edge. Move the design onto the page, or click Center in Hoop."),
        "tooLarge" => tl!("The design is bigger than the hoop. Pick a bigger hoop, or scale the design down."),
        "tooDense" => tl!("Objects stack here, and thread this thick can break the needle. Remove Overlaps cuts away the parts that are covered."),
        "thinFill" => tl!("A fill this thin sews as a few ragged rows. Convert it to satin instead."),
        "wideSatin" => tl!("Satin stitches this long lie loose and snag. Split Satin breaks them into shorter stitches, or sew it as a fill."),
        "narrowSatin" => tl!("A satin this narrow bunches up into a knot. Sew it as a running stitch instead."),
        _ => tl!("This is too small to sew cleanly. Make it bigger, or delete it."),
    }
}

/// The Fix button's label for a fix command.
fn fix_label(fix: &Value) -> &'static str {
    match fix["command"].as_str().unwrap_or("") {
        "stitch.centerInHoop" => tl!("Center in Hoop"),
        "stitch.removeOverlaps" => tl!("Remove Overlaps"),
        "stitch.convertToSatin" => tl!("Convert to Satin"),
        _ if fix["params"]["settings"].is_object() => tl!("Split Satin"),
        _ => tl!("Sew as Running Stitch"),
    }
}

fn level_color(t: &Tokens, p: &Value) -> Color32 {
    if p["level"] == "error" { t.error } else { t.warning }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(q) = problems(app, ui.ctx()) else {
        widgets::dim_label(ui, tl!("No document"));
        return;
    };
    let list = q["problems"].as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        ui.add_space(4.0);
        widgets::dim_label(ui, tl!("No problems found. The design is ready to sew."));
        return;
    }
    let count = |k: &str| q[k].as_u64().unwrap_or(0).to_string();
    ui.horizontal(|ui| {
        for (label, n, c) in [(tl!("Errors"), count("errors"), t.error), (tl!("Warnings"), count("warnings"), t.warning)] {
            ui.label(egui::RichText::new(n).color(c).strong());
            ui.label(egui::RichText::new(label).color(t.text_dim));
            ui.add_space(8.0);
        }
    });
    widgets::divider(ui);
    let focused = ui.ctx().data(|d| d.get_temp::<usize>(egui::Id::new("stitch-check-focus")));
    let mut clicked: Option<usize> = None;
    let mut fix: Option<Value> = None;
    egui::ScrollArea::vertical().id_salt("stitch-check-rows").auto_shrink([false, true]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, p) in list.iter().enumerate() {
            let has_fix = p.get("fix").is_some();
            let h = if has_fix { ROW_H + FIX_H } else { ROW_H };
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
            let painter = ui.painter();
            if focused == Some(i) {
                painter.rect_filled(rect, 0.0, t.selection);
            } else if resp.hovered() {
                painter.rect_filled(rect, 0.0, t.hover);
            }
            let c = level_color(&t, p);
            painter.circle_filled(pos2(rect.left() + 12.0, rect.top() + 14.0), 4.5, c);
            let right = rect.right() - 8.0;
            let text = |s: String, size: f32, color: Color32, y: f32| {
                let mut job = egui::text::LayoutJob::simple_singleline(s, egui::FontId::proportional(size), color);
                job.wrap = egui::text::TextWrapping::truncate_at_width((right - rect.left() - 26.0).max(20.0));
                let g = painter.layout_job(job);
                painter.galley(pos2(rect.left() + 24.0, y), g, color);
            };
            text(title(p["kind"].as_str().unwrap_or("")).to_string(), 12.5, t.text, rect.top() + 6.0);
            text(detail(p), 11.5, t.text_dim, rect.top() + 24.0);
            if has_fix {
                let label = fix_label(&p["fix"]);
                let w = (painter.layout_no_wrap(label.to_string(), egui::FontId::proportional(11.5), t.text).size().x + 20.0)
                    .min(right - rect.left() - 24.0);
                let b = egui::Rect::from_min_size(pos2(rect.left() + 24.0, rect.top() + ROW_H - 2.0), vec2(w, 22.0));
                let r = ui
                    .put(
                        b,
                        egui::Button::new(egui::RichText::new(label).size(11.5)).truncate().fill(t.button).stroke(Stroke::new(1.0, t.button_border)),
                    )
                    .on_hover_text(label);
                if r.clicked() {
                    fix = Some(p["fix"].clone());
                }
            }
            if resp.on_hover_text(hint(p["kind"].as_str().unwrap_or(""))).clicked() {
                clicked = Some(i);
            }
        }
    });
    if let Some(f) = fix {
        let r = app.run(f["command"].as_str().unwrap_or(""), f["params"].clone());
        if let Err(e) = r {
            app.status(e);
        }
    } else if let Some(i) = clicked {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("stitch-check-focus"), i));
        if let Some(p) = list.get(i) {
            focus(app, p);
        }
    }
}

/// Select a problem's objects and bring it into view (zooming out only when it doesn't fit).
fn focus(app: &mut VectorcraftApp, p: &Value) {
    let ids: Vec<u64> = p["ids"].as_array().into_iter().flatten().filter_map(Value::as_u64).collect();
    if !ids.is_empty() {
        app.run("select.set", json!({ "ids": ids })).ok();
    }
    let (Some(x), Some(y)) = (p["x"].as_f64(), p["y"].as_f64()) else { return };
    let r = p["radius"].as_f64().unwrap_or(10.0).max(1.0);
    let room = app.canvas_rect.map_or(400.0, |c| f64::from(c.width().min(c.height())));
    if let Some(v) = app.view_mut() {
        v.center = Point::new(x, y);
        v.fitted = false;
        if 2.0 * r * v.zoom > room * 0.7 {
            v.zoom = (room * 0.7 / (2.0 * r)).clamp(0.0313, 640.0);
        }
    }
}

/// While the panel is open, ring each problem on the canvas where it is, the clicked one thicker.
pub fn paint(app: &mut VectorcraftApp, painter: &Painter, xf: &crate::canvas::Xf) {
    if app.ui.open_panel.as_deref() != Some(ID) {
        return;
    }
    let Some(q) = problems(app, painter.ctx()) else { return };
    let t = Tokens::get(painter.ctx());
    let focused = painter.ctx().data(|d| d.get_temp::<usize>(egui::Id::new("stitch-check-focus")));
    for (i, p) in q["problems"].as_array().into_iter().flatten().enumerate() {
        let (Some(x), Some(y)) = (p["x"].as_f64(), p["y"].as_f64()) else { continue };
        let c = level_color(&t, p);
        let at = xf.to_screen(Point::new(x, y));
        let r = ((p["radius"].as_f64().unwrap_or(5.0) * xf.zoom) as f32).max(9.0);
        let w = if focused == Some(i) { 3.0 } else { 1.75 };
        painter.circle_stroke(at, r, Stroke::new(w + 2.0, Color32::from_white_alpha(170)));
        painter.circle_stroke(at, r, Stroke::new(w, c));
    }
}

#[cfg(test)]
mod tests {
    use vectorcraft_engine::Session;

    use super::*;

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
    fn lists_problems_with_their_fix_and_selects_them() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
        app.session.execute("paint.setStroke", &json!({"none": true})).unwrap();
        let shown = frame(&mut app);
        assert!(shown.iter().any(|s| s.starts_with("No problems found")), "{shown:?}");
        // Three fills stacked.
        for x in [10, 30, 50] {
            app.session.execute("shape.rectangle", &json!({"x": x, "y": 10, "width": 72, "height": 72})).unwrap();
        }
        app.session.execute("select.none", &json!({})).unwrap();
        let shown = frame(&mut app);
        for want in ["Too dense", "Errors", "Remove Overlaps"] {
            assert!(shown.iter().any(|s| s == want), "{want} missing: {shown:?}");
        }
        let q = app.session.execute("stitch.check", &json!({"ids": []})).unwrap();
        let p = q["problems"][0].clone();
        focus(&mut app, &p);
        assert_eq!(app.session.active().unwrap().selection.objects.len(), 3);
        // The selection doesn't change what is checked.
        assert_eq!(app.session.execute("stitch.check", &json!({"ids": []})).unwrap(), q);
    }
}
