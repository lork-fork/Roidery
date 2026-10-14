//! Transform panel: reference point, X/Y/W/H with the constrain link, rotate and shear, flips,
//! live-shape properties and the Scale Corners / Scale Strokes & Effects options. While the
//! Artboard tool is in use it shows and edits the active artboard's X/Y/W/H instead.

use egui::Ui;
use serde_json::json;
use vectorcraft_doc::NodeKind;
use vectorcraft_geom::Rect;

use super::{corner_radius, first_selected, pstate, set_pstate};
use crate::theme::Tokens;
use crate::widgets::{self, menu_item};
use crate::{VectorcraftApp, icons};

pub const ANGLE_PRESETS: [f64; 9] = [-180.0, -135.0, -90.0, -45.0, 0.0, 45.0, 90.0, 135.0, 180.0];

/// Proportional size: the other dimension when one changes with the link on.
pub fn constrained(w: f64, h: f64, new_w: Option<f64>, new_h: Option<f64>) -> (f64, f64) {
    match (new_w, new_h) {
        (Some(nw), _) if w.abs() > 1e-9 => (nw, h * nw / w),
        (_, Some(nh)) if h.abs() > 1e-9 => (w * nh / h, nh),
        (a, b) => (a.unwrap_or(w), b.unwrap_or(h)),
    }
}

/// The artboard the Artboard tool has active, while that tool is in use.
fn tool_artboard(app: &VectorcraftApp) -> Option<(usize, Rect)> {
    if app.session.tool_id() != "artboard" {
        return None;
    }
    let i = usize::try_from(app.session.tool_options().get("active")?.as_u64()?).ok()?;
    app.session.active()?.doc.artboards.get(i).map(|a| (i, a.rect))
}

/// Where an artboard goes when the panel sets its X, Y, W or H: the reference point `refi` moves to
/// the new X/Y or stays put while the size changes (both sides together when `link` is on).
pub fn artboard_rect(r: Rect, refi: usize, x: Option<f64>, y: Option<f64>, w: Option<f64>, h: Option<f64>, link: bool) -> Rect {
    let rp = vectorcraft_geom::reference_point(r, refi);
    let (w0, h0) = (r.width(), r.height());
    let (nw, nh) = if link { constrained(w0, h0, w, h) } else { (w.unwrap_or(w0), h.unwrap_or(h0)) };
    // Where the reference point sits across the box (0, ½ or 1 of each side).
    let fx = if w0 > 1e-9 { (rp.x - r.x0) / w0 } else { 0.0 };
    let fy = if h0 > 1e-9 { (rp.y - r.y0) / h0 } else { 0.0 };
    let (x0, y0) = (x.unwrap_or(rp.x) - fx * nw, y.unwrap_or(rp.y) - fy * nh);
    Rect::new(x0, y0, x0 + nw, y0 + nh)
}

/// The Artboard tool's view of the panel: the active artboard's position and size.
fn artboard_fields(app: &mut VectorcraftApp, ui: &mut Ui, index: usize, r: Rect) {
    let units = app.session.general_unit();
    let refi: usize = ui.data(|d| d.get_temp(egui::Id::new("refpt"))).unwrap_or(4);
    let link = app.session.prefs.constrain_proportions;
    let rp = vectorcraft_geom::reference_point(r, refi);
    let set = |app: &mut VectorcraftApp, x, y, w, h| {
        let n = artboard_rect(r, refi, x, y, w, h, link);
        let p = json!({"index": index, "x": n.x0, "y": n.y0, "width": n.width(), "height": n.height()});
        if let Err(e) = app.run("artboard.setProps", p) {
            app.status(e);
        }
    };
    ui.horizontal(|ui| {
        if let Some(i) = widgets::reference_point(ui, refi) {
            ui.data_mut(|d| d.insert_temp(egui::Id::new("refpt"), i));
        }
        ui.add_space(4.0);
        egui::Grid::new("xfp-ab-grid").num_columns(4).spacing([4.0, 6.0]).min_col_width(0.0).show(ui, |ui| {
            widgets::dim_label(ui, "X:");
            if let Some(v) = widgets::num_field(ui, "xfp-ab-x", Some(rp.x), units, 80.0) {
                set(app, Some(v), None, None, None);
            }
            widgets::dim_label(ui, "W:");
            if let Some(v) = widgets::num_field(ui, "xfp-ab-w", Some(r.width()), units, 80.0) {
                set(app, None, None, Some(v), None);
            }
            ui.end_row();
            widgets::dim_label(ui, "Y:");
            if let Some(v) = widgets::num_field(ui, "xfp-ab-y", Some(rp.y), units, 80.0) {
                set(app, None, Some(v), None, None);
            }
            widgets::dim_label(ui, "H:");
            if let Some(v) = widgets::num_field(ui, "xfp-ab-h", Some(r.height()), units, 80.0) {
                set(app, None, None, None, Some(v));
            }
            ui.end_row();
        });
        constrain_link(app, ui);
    });
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    if app.session.active().is_none() {
        widgets::dim_label(ui, tl!("No document"));
        return;
    }
    if let Some((i, r)) = tool_artboard(app) {
        artboard_fields(app, ui, i, r);
        return;
    }
    let units = app.session.general_unit();
    // The bounding box, rotated with rotated objects: W/H are its own sides, X/Y its reference
    // point on the page.
    let bx = app.selection_box();
    let bounds = bx.map(|b| b.rect);
    let refi: usize = ui.data(|d| d.get_temp(egui::Id::new("refpt"))).unwrap_or(4);
    let link = app.session.prefs.constrain_proportions;
    let has = bounds.is_some();
    let rp = bx.map(|b| b.reference_point(refi));
    ui.horizontal(|ui| {
        if let Some(i) = widgets::reference_point(ui, refi) {
            ui.data_mut(|d| d.insert_temp(egui::Id::new("refpt"), i));
        }
        ui.add_space(4.0);
        ui.add_enabled_ui(has, |ui| {
            egui::Grid::new("xfp-grid").num_columns(4).spacing([4.0, 6.0]).min_col_width(0.0).show(ui, |ui| {
                widgets::dim_label(ui, "X:");
                if let Some(v) = widgets::num_field(ui, "xfp-x", rp.map(|p| p.x), units, 80.0) {
                    app.run("object.setBounds", json!({"x": v, "reference": refi})).ok();
                }
                widgets::dim_label(ui, "W:");
                if let Some(v) = widgets::num_field(ui, "xfp-w", bounds.map(|b| b.width()), units, 80.0) {
                    app.run("object.setBounds", json!({"width": v, "reference": refi, "proportional": link})).ok();
                }
                ui.end_row();
                widgets::dim_label(ui, "Y:");
                if let Some(v) = widgets::num_field(ui, "xfp-y", rp.map(|p| p.y), units, 80.0) {
                    app.run("object.setBounds", json!({"y": v, "reference": refi})).ok();
                }
                widgets::dim_label(ui, "H:");
                if let Some(v) = widgets::num_field(ui, "xfp-h", bounds.map(|b| b.height()), units, 80.0) {
                    app.run("object.setBounds", json!({"height": v, "reference": refi, "proportional": link})).ok();
                }
                ui.end_row();
            });
        });
        constrain_link(app, ui);
    });
    ui.add_space(4.0);
    let origin = rp.map(|p| json!([p.x, p.y]));
    ui.horizontal(|ui| {
        ui.add_enabled_ui(has, |ui| {
            let icon = icons::icon(ui, "rotate-ccw", 16.0, t.icon).on_hover_text(tl!("Rotate"));
            crate::scrub::note_label(ui, icon.rect);
            // The bounding box's angle: a new value turns the selection to it.
            let angle = bx.map_or(0.0, |b| b.angle);
            if let Some(a) = widgets::spin_plain(ui, "xfp-rot", angle, "°", 2, 96.0, 15.0, -360.0, &ANGLE_PRESETS)
                && a != angle
            {
                app.run("object.rotate", json!({"angle": a, "absolute": true, "origin": origin})).ok();
            }
            ui.add_space(4.0);
            let icon = icons::icon(ui, "dc-shear", 16.0, t.icon).on_hover_text(tl!("Shear"));
            crate::scrub::note_label(ui, icon.rect);
            if let Some(a) = widgets::plain_field(ui, "xfp-shear", 0.0, "°", 1, 56.0)
                && a != 0.0
            {
                app.run("object.shear", json!({"angle": a, "axis": "horizontal", "origin": origin})).ok();
            }
        });
    });
    ui.horizontal(|ui| {
        if widgets::icon_button_enabled(ui, "flip-horizontal-2", tl!("Flip Horizontal"), false, has, 24.0).clicked() {
            app.run("object.reflect", json!({"axis": "vertical", "origin": origin})).ok();
        }
        if widgets::icon_button_enabled(ui, "flip-vertical-2", tl!("Flip Vertical"), false, has, 24.0).clicked() {
            app.run("object.reflect", json!({"axis": "horizontal", "origin": origin})).ok();
        }
    });
    // Live shape properties.
    if let Some(n) = first_selected(app)
        && let NodeKind::Path { live: Some(live), .. } = &n.kind
    {
        widgets::divider(ui);
        match live {
            vectorcraft_doc::LiveShape::Rectangle { .. } => {
                widgets::subheader(ui, tl!("Rectangle Properties:"));
                ui.horizontal(|ui| {
                    widgets::dim_label(ui, tl!("Corner Radius:"));
                    if let Some(r) = widgets::num_field(ui, "xfp-radius", corner_radius(app, &n, live), units, 80.0) {
                        app.run("object.setLiveShape", json!({"radius": r})).ok();
                    }
                });
            }
            vectorcraft_doc::LiveShape::Polygon { sides, .. } => {
                widgets::subheader(ui, tl!("Polygon Properties:"));
                ui.horizontal(|ui| {
                    widgets::dim_label(ui, tl!("Sides:"));
                    if let Some(s) = widgets::plain_field(ui, "xfp-sides", *sides as f64, "", 0, 60.0) {
                        app.run("object.setLiveShape", json!({"sides": s.clamp(3.0, 20.0) as u32})).ok();
                    }
                });
            }
            _ => {
                widgets::subheader(ui, tl!("Shape Properties:"));
                widgets::dim_label(ui, tl!(n.kind_label()));
            }
        }
    }
    if pstate::<bool>(ui.ctx(), "xf-hide-options") {
        return;
    }
    widgets::divider(ui);
    let (sc, ss) = (app.session.prefs.scale_corners, app.session.prefs.scale_strokes);
    if widgets::check(ui, tl!("Scale Corners"), sc, true) {
        set_pref(app, "scaleCorners", !sc);
    }
    if widgets::check(ui, tl!("Scale Strokes & Effects"), ss, true) {
        set_pref(app, "scaleStrokes", !ss);
    }
}

/// The link between W and H (Transform panel, Properties panel, Control bar): one toggle, the
/// `constrainProportions` preference, which the size fields pass on as `proportional`.
pub fn constrain_link(app: &mut VectorcraftApp, ui: &mut Ui) {
    let on = app.session.prefs.constrain_proportions;
    if widgets::icon_button(ui, if on { "link" } else { "link-2-off" }, tl!("Constrain Width and Height Proportions"), on, 22.0).clicked() {
        set_pref(app, "constrainProportions", !on);
    }
}

/// Toggle a boolean preference (through `prefs.set`, as agents do).
pub fn set_pref(app: &mut VectorcraftApp, key: &str, on: bool) {
    if let Err(e) = app.run("prefs.set", json!({ "key": key, "value": on })) {
        app.status(e);
    }
}

pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    let hidden: bool = pstate(ui.ctx(), "xf-hide-options");
    let has = super::selection_len(app) > 0;
    if menu_item(ui, if hidden { tl!("Show Options") } else { tl!("Hide Options") }, true, false) {
        set_pstate(ui.ctx(), "xf-hide-options", !hidden);
    }
    ui.separator();
    if menu_item(ui, tl!("Flip Horizontal"), has, false) {
        app.run("object.reflect", json!({"axis": "vertical"})).ok();
    }
    if menu_item(ui, tl!("Flip Vertical"), has, false) {
        app.run("object.reflect", json!({"axis": "horizontal"})).ok();
    }
    ui.separator();
    let ss = app.session.prefs.scale_strokes;
    if menu_item(ui, tl!("Scale Strokes & Effects"), true, ss) {
        set_pref(app, "scaleStrokes", !ss);
    }
    ui.separator();
    menu_item(ui, tl!("Transform Object Only"), false, true);
    menu_item(ui, tl!("Transform Pattern Only"), false, false);
    menu_item(ui, tl!("Transform Both"), false, false);
    ui.separator();
    menu_item(ui, tl!("Use Registration Point for Symbol"), false, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One headless frame of the panel (or the Align panel's menu): the texts drawn.
    fn texts(app: &mut VectorcraftApp, draw: fn(&mut VectorcraftApp, &mut Ui)) -> Vec<String> {
        fn walk(s: &egui::Shape, out: &mut Vec<String>) {
            match s {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| draw(app, ui));
        out.textures_delta.clear();
        let mut v = vec![];
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut v));
        v
    }

    #[test]
    fn use_preview_bounds_shows_the_visual_size_and_checks_the_align_flyout() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
        app.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 100, "height": 50})).unwrap();
        app.run("stroke.set", json!({"weight": 10})).unwrap();
        assert!(texts(&mut app, show).iter().any(|t| t == "100 pt"));
        assert!(texts(&mut app, crate::panels::align::menu).iter().any(|t| t == "   Use Preview Bounds"));
        set_pref(&mut app, "usePreviewBounds", true);
        let shown = texts(&mut app, show);
        assert!(shown.iter().any(|t| t == "110 pt") && shown.iter().any(|t| t == "60 pt"), "{shown:?}");
        assert!(texts(&mut app, crate::panels::align::menu).iter().any(|t| t == "✓ Use Preview Bounds"));
    }

    #[test]
    fn rotation_field_shows_the_angle_the_box_keeps() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
        app.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 100, "height": 50})).unwrap();
        app.run("object.rotate", json!({"angle": 45, "absolute": true})).unwrap();
        let shown = texts(&mut app, show);
        // The angle stays, and W/H are the rectangle's own sides.
        assert!(shown.iter().any(|t| t == "45°"), "{shown:?}");
        assert!(shown.iter().any(|t| t == "100 pt") && shown.iter().any(|t| t == "50 pt"), "{shown:?}");
        app.run("object.resetBoundingBox", json!({})).unwrap();
        let shown = texts(&mut app, show);
        assert!(shown.iter().any(|t| t == "0°") && !shown.iter().any(|t| t == "100 pt"), "{shown:?}");
    }

    #[test]
    fn the_artboard_tool_shows_the_active_artboard() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 200})).unwrap();
        app.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 40, "height": 30})).unwrap();
        assert!(texts(&mut app, show).iter().any(|t| t == "40 pt"), "the selection first");
        app.run("tool.select", json!({"tool": "artboard"})).unwrap();
        let shown = texts(&mut app, show);
        // Centre reference point: X 150, Y 100, W 300, H 200; no rotate/shear for artboards.
        for v in ["150 pt", "100 pt", "300 pt", "200 pt"] {
            assert!(shown.iter().any(|t| t == v), "{v} in {shown:?}");
        }
        assert!(!shown.iter().any(|t| t == "40 pt" || t == "0°"), "{shown:?}");
    }

    #[test]
    fn artboard_fields_keep_the_reference_point() {
        let r = Rect::new(0.0, 0.0, 300.0, 200.0);
        // Centre: a new X centres it there, a new W grows it both ways.
        assert_eq!(artboard_rect(r, 4, Some(200.0), None, None, None, false), Rect::new(50.0, 0.0, 350.0, 200.0));
        assert_eq!(artboard_rect(r, 4, None, None, Some(400.0), None, false), Rect::new(-50.0, 0.0, 350.0, 200.0));
        // Top-left: a new W keeps the left edge; linked, H follows.
        assert_eq!(artboard_rect(r, 0, None, None, Some(600.0), None, true), Rect::new(0.0, 0.0, 600.0, 400.0));
        // Bottom-right: a new Y puts the bottom edge there.
        assert_eq!(artboard_rect(r, 8, None, Some(500.0), None, None, false), Rect::new(0.0, 300.0, 300.0, 500.0));
    }

    #[test]
    fn constrain_proportions() {
        assert_eq!(constrained(100.0, 50.0, Some(200.0), None), (200.0, 100.0));
        assert_eq!(constrained(100.0, 50.0, None, Some(25.0)), (50.0, 25.0));
        assert_eq!(constrained(0.0, 50.0, Some(10.0), None), (10.0, 50.0));
    }
}
