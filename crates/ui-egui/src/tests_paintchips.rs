//! The Fill/Stroke proxies and chips (toolbar, Control bar, Properties), their swatch and mixer
//! popovers, the Recolor quick action and the panel shortcuts (M3.51).

use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, Shape, vec2};
use serde_json::json;
use vectorcraft_engine::Session;

use crate::{VectorcraftApp, chrome, panels, shortcut_editor, shortcuts, toolbar};

fn app() -> VectorcraftApp {
    let mut app = VectorcraftApp::new(Session::new(), Default::default());
    app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
    app
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    ctx
}

/// One headless frame of `draw` with `events` (and `modifiers` held); returns the painted shapes.
fn frame(
    app: &mut VectorcraftApp,
    ctx: &egui::Context,
    events: Vec<Event>,
    modifiers: Modifiers,
    draw: fn(&mut VectorcraftApp, &mut egui::Ui),
) -> Vec<Shape> {
    let events = std::iter::once(Event::ModifiersChanged(modifiers)).chain(events).collect();
    let screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 1000.0)));
    let mut out = ctx.run_ui(egui::RawInput { events, screen_rect, ..Default::default() }, |ui| draw(app, ui));
    out.textures_delta.clear();
    out.shapes.into_iter().map(|c| c.shape).collect()
}

/// Click at `at` with `modifiers` held (press and release in two frames), then settle a frame.
fn click(app: &mut VectorcraftApp, ctx: &egui::Context, at: Pos2, modifiers: Modifiers, draw: fn(&mut VectorcraftApp, &mut egui::Ui)) {
    let button = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers };
    frame(app, ctx, vec![Event::PointerMoved(at), button(true)], modifiers, draw);
    frame(app, ctx, vec![button(false)], modifiers, draw);
    frame(app, ctx, vec![], Modifiers::NONE, draw);
}

/// Whether `shapes` paint a rectangle filled with `color`.
fn paints_rect(shapes: &[Shape], color: Color32) -> bool {
    shapes.iter().any(|s| match s {
        Shape::Rect(r) => r.fill == color,
        Shape::Vec(v) => paints_rect(v, color),
        _ => false,
    })
}

/// The Control bar's Fill and Stroke chips (22 pt squares with a chevron), left to right.
fn chips(ctx: &egui::Context) -> (Rect, Rect) {
    let mut r: Vec<Rect> = ctx.viewport(|vp| {
        vp.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter()).filter(|w| w.rect.size() == vec2(38.0, 22.0)).map(|w| w.rect).collect()
    });
    r.sort_by(|a, b| a.left().total_cmp(&b.left()));
    (r[0], r[1])
}

fn control_bar(app: &mut VectorcraftApp, ui: &mut egui::Ui) {
    chrome::control_bar(app, ui);
}

fn fill_hex(app: &VectorcraftApp) -> String {
    panels::current_paints(app).0.color().map(|c| c.to_hex()).unwrap_or_default()
}

#[test]
fn red_text_shows_a_red_toolbar_fill() {
    let mut app = app();
    app.run("text.create", json!({"x": 20, "y": 60, "text": "Red"})).unwrap();
    app.run("paint.setFill", json!({"color": "#ff0000"})).unwrap();
    let ctx = context();
    frame(&mut app, &ctx, vec![], Modifiers::NONE, toolbar::show);
    let shapes = frame(&mut app, &ctx, vec![], Modifiers::NONE, toolbar::show);
    assert!(paints_rect(&shapes, Color32::from_rgb(255, 0, 0)), "the toolbar's Fill proxy shows the type's red");
}

#[test]
fn the_chips_show_a_question_mark_for_mixed_fills() {
    let mut app = app();
    let a = app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 50, "height": 50})).unwrap()["id"].clone();
    app.run("paint.setFill", json!({"color": "#ff0000"})).unwrap();
    let b = app.run("shape.rectangle", json!({"x": 100, "y": 10, "width": 50, "height": 50})).unwrap()["id"].clone();
    app.run("paint.setFill", json!({"color": "#00ff00"})).unwrap();
    let text = |app: &mut VectorcraftApp| crate::tests_labels::painted_text(app, control_bar);
    assert!(!text(&mut app).lines().any(|l| l == "?"));
    app.run("select.set", json!({"ids": [a, b]})).unwrap();
    assert!(text(&mut app).lines().any(|l| l == "?"), "the Fill chip shows ?");
    let props = crate::tests_labels::painted_text(&mut app, panels::properties::show);
    assert!(props.lines().any(|l| l == "?"), "so does the Properties panel's");
}

#[test]
fn a_popover_tile_click_sets_the_fill() {
    let mut app = app();
    app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 50, "height": 50})).unwrap();
    app.run("paint.toggleActive", json!({"fill": false})).unwrap();
    let ctx = context();
    frame(&mut app, &ctx, vec![], Modifiers::NONE, control_bar);
    let (fill, stroke) = chips(&ctx);
    click(&mut app, &ctx, fill.center(), Modifiers::NONE, control_bar);
    assert!(app.session.fill_active, "the Fill chip brings its proxy forward");
    let tile = ctx.read_response(egui::Id::new(("swatch-pop-tile", "Red"))).expect("the popover shows the swatches").rect;
    click(&mut app, &ctx, tile.center(), Modifiers::NONE, control_bar);
    assert_eq!(fill_hex(&app), "#ed1c24");
    assert_eq!(app.session.doc().unwrap().history.undo.last().unwrap().label, "Fill Color", "through paint.setFill");
    // The Stroke chip: its proxy comes forward; Shift-click shows the mixer instead of swatches.
    click(&mut app, &ctx, Pos2::new(1300.0, 900.0), Modifiers::NONE, control_bar);
    click(&mut app, &ctx, stroke.center(), Modifiers::SHIFT, control_bar);
    assert!(!app.session.fill_active);
    assert!(ctx.read_response(egui::Id::new(("swatch-pop-tile", "Red"))).is_none(), "no swatches in the mixer");
    let text = {
        let mut t = String::new();
        for s in frame(&mut app, &ctx, vec![], Modifiers::NONE, control_bar) {
            if let Shape::Text(g) = s {
                t.push_str(g.galley.text());
                t.push('\n');
            }
        }
        t
    };
    assert!(text.contains("Recent Colors"), "the Color panel's body: {text}");
}

#[test]
fn recolor_is_a_quick_action_for_multicolour_selections() {
    let mut app = app();
    app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 50, "height": 50})).unwrap();
    app.run("paint.setStroke", json!({"none": true})).unwrap();
    let text = |app: &mut VectorcraftApp| crate::tests_labels::painted_text(app, panels::properties::show);
    assert!(!text(&mut app).contains("Recolor"), "one colour");
    app.run("paint.setStroke", json!({"color": "#ff0000"})).unwrap();
    assert!(text(&mut app).contains("Recolor"), "white and red");
}

#[test]
fn the_shortcut_table_maps_f6_to_color() {
    assert_eq!(shortcut_editor::panel_shortcut("color"), Some("F6"));
    assert_eq!(shortcut_editor::panel_shortcut("transparency"), Some("Cmd+Shift+F10"));
    let all = shortcuts::all_shortcuts();
    let f6 = all.iter().find(|(sc, ..)| sc.logical_key == egui::Key::F6 && sc.modifiers == Modifiers::NONE).expect("F6");
    assert_eq!((f6.1, &f6.2), ("window.panel", &json!({"panel": "color"})));
    // The keys toggle their panels; Shift+F6 isn't taken by F6.
    let mut app = app();
    let ctx = context();
    let key =
        |key, modifiers| vec![Event::ModifiersChanged(modifiers), Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }];
    let press = |app: &mut VectorcraftApp, k, m| {
        let mut out = ctx.run_ui(egui::RawInput { events: key(k, m), ..Default::default() }, |ui| shortcuts::handle(app, ui.ctx()));
        out.textures_delta.clear();
    };
    press(&mut app, egui::Key::F6, Modifiers::NONE);
    assert_eq!(app.ui.open_panel.as_deref(), Some("color"));
    press(&mut app, egui::Key::F6, Modifiers::SHIFT);
    assert_eq!(app.ui.open_panel.as_deref(), Some("appearance"));
    press(&mut app, egui::Key::F9, Modifiers::COMMAND);
    assert_eq!(app.ui.open_panel.as_deref(), Some("gradient"));
    // The Window menu lists them.
    let color = crate::menus::menu_entries(&app).into_iter().find(|e| e.path == ["Window"] && e.label == "Color").unwrap();
    assert_eq!(color.shortcut, "F6");
}
