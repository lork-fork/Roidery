//! Object › Path › Remove Anchor Points: the menu, the Control bar, the contextual task bar, the
//! Properties panel and the context menu run it on direct-selected anchors.

use egui::{Event, PointerButton, Pos2, Rect, Shape, vec2};
use serde_json::json;
use vectorcraft_doc::NodeId;
use vectorcraft_engine::Session;
use vectorcraft_geom::Point;

use crate::canvas::Xf;
use crate::menus::{self, Item};
use crate::{VectorcraftApp, canvas, chrome};

fn app() -> VectorcraftApp {
    let mut app = VectorcraftApp::new(Session::new(), Default::default());
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app
}

fn rect(app: &mut VectorcraftApp) -> NodeId {
    NodeId(app.run("shape.rectangle", json!({"x": 50, "y": 50, "width": 100, "height": 80})).unwrap()["id"].as_u64().unwrap())
}

fn anchor_count(app: &VectorcraftApp, id: NodeId) -> usize {
    app.session.active().and_then(|st| st.doc.node(id)).and_then(|n| n.path_data()).map(|p| p.anchor_count()).unwrap_or(0)
}

fn labels(items: &[Item]) -> Vec<&'static str> {
    items
        .iter()
        .flat_map(|it| match it {
            Item::Cmd(l, ..) => vec![*l],
            Item::Sub(l, ch) => std::iter::once(*l).chain(labels(ch)).collect(),
            _ => vec![],
        })
        .collect()
}

fn shapes_text(shapes: &[Shape]) -> Vec<(String, Rect)> {
    fn walk(s: &Shape, v: &mut Vec<(String, Rect)>) {
        match s {
            Shape::Text(t) => v.push((t.galley.text().to_string(), Rect::from_min_size(t.pos, t.galley.size()))),
            Shape::Vec(s) => s.iter().for_each(|s| walk(s, v)),
            _ => {}
        }
    }
    let mut v = vec![];
    shapes.iter().for_each(|s| walk(s, &mut v));
    v
}

fn control_frame(app: &mut VectorcraftApp, ctx: &egui::Context, events: Vec<Event>) -> Vec<(String, Rect)> {
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0));
    let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| chrome::control_bar(app, ui));
    out.textures_delta.clear();
    shapes_text(&out.shapes.iter().map(|c| c.shape.clone()).collect::<Vec<_>>())
}

fn click_control(app: &mut VectorcraftApp, ctx: &egui::Context, at: Pos2) {
    let press = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() };
    control_frame(app, ctx, vec![Event::PointerMoved(at), press(true)]);
    control_frame(app, ctx, vec![press(false)]);
    control_frame(app, ctx, vec![]);
}

fn canvas_frame(app: &mut VectorcraftApp, ctx: &egui::Context, events: Vec<Event>) -> Vec<(String, Rect)> {
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
    let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| canvas::show(app, ui));
    out.textures_delta.clear();
    shapes_text(&out.shapes.iter().map(|c| c.shape.clone()).collect::<Vec<_>>())
}

fn click_canvas(app: &mut VectorcraftApp, ctx: &egui::Context, at: Pos2, button: PointerButton) -> Vec<(String, Rect)> {
    let press = |pressed| Event::PointerButton { pos: at, button, pressed, modifiers: Default::default() };
    canvas_frame(app, ctx, vec![Event::PointerMoved(at)]);
    canvas_frame(app, ctx, vec![press(true)]);
    canvas_frame(app, ctx, vec![press(false)]);
    canvas_frame(app, ctx, vec![])
}

fn properties_frame(app: &mut VectorcraftApp, width: f32) -> Vec<(String, Rect)> {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0));
    let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
        crate::panels::properties::show(app, ui);
    });
    out.textures_delta.clear();
    shapes_text(&out.shapes.iter().map(|c| c.shape.clone()).collect::<Vec<_>>())
}

fn has(texts: &[(String, Rect)], label: &str) -> bool {
    texts.iter().any(|(t, _)| t == label)
}

fn at(texts: &[(String, Rect)], label: &str) -> Pos2 {
    texts.iter().find(|(t, _)| t == label).map(|(_, r)| r.center()).unwrap_or_else(|| panic!("no `{label}`"))
}

#[test]
fn the_path_menu_and_bars_run_it_for_direct_selected_anchors() {
    const LABEL: &str = "Remove Anchor Points";
    let mut app = app();
    let id = rect(&mut app);
    assert_eq!(anchor_count(&app, id), 4);
    let entries: Vec<_> = menus::menu_entries(&app).into_iter().filter(|e| e.label == LABEL).collect();
    assert_eq!(entries.len(), 1, "one Remove Anchor Points item");
    assert_eq!(entries[0].command.as_deref(), Some("path.removeAnchors"));
    assert_eq!(entries[0].path, ["Object", "Path"]);
    assert!(!entries[0].enabled, "an object selection is not enough");
    assert!(!labels(&menus::context_items(&app)).contains(&LABEL));
    assert!(crate::palette::items().iter().any(|(_, cmd, _)| cmd == "path.removeAnchors"));

    app.select_tool("directSelection");
    app.run("select.anchors", json!({"id": id.0, "anchors": [[0, 1]]})).unwrap();
    assert!(menus::enabled(&app, "path.removeAnchors"));
    assert!(labels(&menus::context_items(&app)).contains(&LABEL));

    // The Control bar's icon follows its "Anchor Point" label.
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let bar = control_frame(&mut app, &ctx, vec![]);
    let label = bar.iter().find(|(t, _)| t == "Anchor Point").map(|(_, r)| *r).expect("anchor label");
    click_control(&mut app, &ctx, Pos2::new(label.max.x + ctx.global_style().spacing.item_spacing.x + 12.0, label.center().y));
    assert_eq!(anchor_count(&app, id), 3, "the Control bar button removes the anchor");
    let st = app.session.active().unwrap();
    assert_eq!(st.history.undo.last().unwrap().label, LABEL);
    let sp = &st.doc.node(id).unwrap().path_data().unwrap().subpaths[0];
    assert!(sp.closed, "the path stays closed");
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(anchor_count(&app, id), 4);

    app.run("select.anchors", json!({"id": id.0, "anchors": [[0, 1]]})).unwrap();
    // 230 pt is the dock's minimum width. The label has to sit inside that row.
    let props = properties_frame(&mut app, 230.0);
    let row = props.iter().find(|(t, _)| t == LABEL).expect("properties");
    assert!(row.1.min.x >= -0.5 && row.1.max.x <= 230.5, "properties clips the label: {:?}", row.1);

    // The task bar's area settles on the second frame.
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    canvas_frame(&mut app, &ctx, vec![]);
    let texts = canvas_frame(&mut app, &ctx, vec![]);
    assert!(has(&texts, LABEL), "task bar: {texts:?}");

    // Right-click keeps the anchor selection (the object is already selected) and runs the item.
    let p = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap()).to_screen(Point::new(100.0, 90.0));
    let texts = click_canvas(&mut app, &ctx, p, PointerButton::Secondary);
    assert!(has(&texts, LABEL), "context menu: {texts:?}");
    let texts = click_canvas(&mut app, &ctx, at(&texts, LABEL), PointerButton::Primary);
    assert_eq!(anchor_count(&app, id), 3);
    assert!(!has(&texts, LABEL), "the menu closes after the command");
}

#[test]
fn the_control_bar_and_properties_hide_it_without_anchors() {
    let mut app = app();
    rect(&mut app);
    let props = crate::tests_labels::painted_text(&mut app, crate::panels::properties::show);
    assert!(!props.contains("Remove Anchor Points"), "{props}");
}
