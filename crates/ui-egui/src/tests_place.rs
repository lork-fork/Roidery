//! File → Place in the app: the Place dialog, files dropped on the window, the loaded place
//! cursor and the Control bar's image details.

use std::sync::Arc;

use egui::{Pos2, Shape, vec2};
use serde_json::{Value, json};
use vectorcraft_doc::NodeKind;
use vectorcraft_engine::Session;

use crate::VectorcraftApp;
use crate::canvas::Xf;

/// A `w`×`h` red PNG declaring `ppi`.
fn png(w: u32, h: u32, ppi: f64) -> Vec<u8> {
    let mut out = vec![];
    image::RgbaImage::from_pixel(w, h, image::Rgba([255, 0, 0, 255])).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    vectorcraft_engine::cmd::fileio::ppi::with_png_resolution(&out, (ppi, ppi))
}

/// `bytes` written to a fresh temporary file named `name` → its path.
fn temp_file(name: &str, bytes: &[u8]) -> String {
    let dir = std::env::temp_dir().join(format!("vectorcraft-ui-place-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path.to_string_lossy().to_string()
}

/// An app reading files from disk, with a 400×300 document.
fn app() -> VectorcraftApp {
    let services = crate::Services { read: Some(Box::new(|p: &str| std::fs::read(p).map_err(|e| e.to_string()))), ..Default::default() };
    let mut app = VectorcraftApp::new(Session::new(), services);
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app
}

/// File → Place… with `path` chosen in the file picker.
fn place_picked(app: &mut VectorcraftApp, path: &str) {
    let path = path.to_string();
    app.services.pick_open = Some(Box::new(move |_: &crate::FilePick| Some(path.clone())));
    app.run("file.place", json!({})).unwrap();
}

#[derive(Debug)]
struct Dropped(std::path::PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

/// One headless frame of the whole window (800×600) with `events` and `dropped` files.
fn frame(app: &mut VectorcraftApp, ctx: &egui::Context, mut events: Vec<egui::Event>, dropped: &[&str], shift: bool) {
    events.push(egui::Event::ModifiersChanged(egui::Modifiers { shift, ..Default::default() }));
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
        events,
        dropped_files: dropped.iter().map(|p| Arc::new(Dropped(p.into())) as egui::DroppedFileHandle).collect(),
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| {
        app.logic(ui.ctx());
        app.ui(ui);
    });
    out.textures_delta.clear();
}

fn selected_image(app: &VectorcraftApp) -> vectorcraft_doc::Node {
    let st = app.session.active().unwrap();
    let n = st.doc.node(st.selection.objects[0]).unwrap().clone();
    assert!(matches!(n.kind, NodeKind::Image(_)), "{:?}", n.kind);
    n
}

#[test]
fn a_dropped_png_is_centred_at_the_pointer() {
    let mut app = app();
    let ctx = egui::Context::default();
    // Two frames: fonts, then the canvas lays out.
    frame(&mut app, &ctx, vec![], &[], false);
    frame(&mut app, &ctx, vec![], &[], false);
    let rect = app.canvas_rect.expect("the canvas is laid out");
    let pos = rect.center() + vec2(60.0, -40.0);
    let path = temp_file("drop.png", &png(20, 10, 72.0));
    frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)], &[&path], false);
    let n = selected_image(&app);
    let want = Xf::new(rect, app.view().unwrap()).to_doc(pos);
    let c = n.geometric_bounds().unwrap().center();
    assert!((c.x - want.x).abs() < 1e-6 && (c.y - want.y).abs() < 1e-6, "{c:?} vs {want:?}");
    assert!(matches!(&n.kind, NodeKind::Image(im) if im.link.as_ref().map(|l| l.path.as_str()) == Some(path.as_str())), "linked by default");
    assert_eq!(app.session.documents().len(), 1, "placed, not opened");
    // Shift embeds.
    frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)], &[&path], true);
    assert!(matches!(&selected_image(&app).kind, NodeKind::Image(im) if im.link.is_none()));
}

#[test]
fn files_dropped_off_the_canvas_or_with_no_document_open_and_are_recent() {
    let mut app = VectorcraftApp::new(Session::new(), Default::default());
    let ctx = egui::Context::default();
    let path = temp_file("open-me.png", &png(8, 8, 72.0));
    frame(&mut app, &ctx, vec![], &[], false);
    frame(&mut app, &ctx, vec![egui::Event::PointerMoved(Pos2::new(400.0, 300.0))], &[&path], false);
    assert_eq!(app.session.documents().len(), 1, "no document: the drop opens");
    assert_eq!(app.ui.recent_files.first(), Some(&path));
    // Over the tab bar (above the canvas): opened too.
    frame(&mut app, &ctx, vec![], &[], false);
    let top = app.canvas_rect.unwrap().top();
    frame(&mut app, &ctx, vec![egui::Event::PointerMoved(Pos2::new(300.0, top - 10.0))], &[&path], false);
    assert_eq!(app.session.documents().len(), 2);
}

/// The images in open document `i`.
fn images(app: &VectorcraftApp, i: usize) -> usize {
    let mut n = 0;
    app.session.documents()[i].doc.walk(|node| n += usize::from(matches!(node.kind, NodeKind::Image(_))));
    n
}

/// The web reads dropped files asynchronously (#359): a file that arrives after another document
/// became active still lands where it was dropped, and is dropped itself when that document closed.
#[test]
fn a_file_read_after_another_document_became_active_lands_where_it_was_dropped() {
    let mut app = app();
    let inbox = crate::place::PlaceInbox::default();
    app.services.place_inbox = Some(inbox.clone());
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![], &[], false);
    frame(&mut app, &ctx, vec![], &[], false);
    let rect = app.canvas_rect.expect("the canvas is laid out");
    let pos = rect.center() + vec2(60.0, -40.0);
    let drop_on_active = |app: &VectorcraftApp| match app.drop_target(Some(pos), false) {
        crate::place::DropTarget::Place(d) => d,
        t => panic!("{t:?}"),
    };
    let arrive =
        |name: &str, drop| inbox.lock().unwrap().push(crate::place::PlaceArrival { name: name.into(), bytes: png(20, 10, 72.0), drop: Some(drop) });
    // Dropped on A; another document opens while the file is read.
    let on_a = drop_on_active(&app);
    let want = Xf::new(rect, app.view().unwrap()).to_doc(pos);
    app.run("file.new", json!({"width": 600, "height": 500})).unwrap();
    assert_eq!(app.session.active_index(), Some(1));
    arrive("late.png", on_a);
    frame(&mut app, &ctx, vec![], &[], false);
    assert_eq!(app.session.active_index(), Some(0), "the document it was dropped on is active again");
    assert_eq!((images(&app, 0), images(&app, 1)), (1, 0));
    let c = selected_image(&app).geometric_bounds().unwrap().center();
    assert!((c.x - want.x).abs() < 1e-6 && (c.y - want.y).abs() < 1e-6, "where it was dropped: {c:?} vs {want:?}");
    // Dropped on B, which closes before the file arrives: nothing is placed.
    app.run("document.activate", json!({"index": 1})).unwrap();
    let on_b = drop_on_active(&app);
    app.run("file.close", json!({})).unwrap();
    assert_eq!(app.session.documents().len(), 1);
    arrive("orphan.png", on_b);
    frame(&mut app, &ctx, vec![], &[], false);
    assert_eq!(images(&app, 0), 1, "not placed in the document left open");
    assert!(app.ui.status.contains("orphan.png") && app.ui.status.contains("closed"), "{}", app.ui.status);
}

#[test]
fn the_place_dialog_lists_the_files_and_loads_the_cursor_with_several() {
    let mut app = app();
    let a = temp_file("a.png", &png(300, 150, 300.0));
    let b = temp_file("b.png", &png(10, 10, 72.0));
    app.run("file.place", json!({"paths": [a, b]})).unwrap();
    let d = app.ui.dialog.as_ref().expect("the Place dialog");
    assert_eq!(d.kind, crate::dialogs::place::KIND);
    assert!(d.bool("link") && !d.bool("__replace"), "Link on; Replace needs one file and one selected object");
    assert_eq!(d.fields["__info"][0], "300 × 150 px, 300 ppi, RGB (72 pt × 36 pt)");
    let text = crate::tests_labels::painted_text(&mut app, |app, ui| crate::dialogs::show(app, ui.ctx()));
    for label in ["Place", "a.png", "b.png", "Link", "Template", "Replace", "Cancel"] {
        assert!(text.contains(label), "{label} in {text}");
    }
    crate::dialogs::confirm(&mut app).unwrap();
    assert!(app.ui.dialog.is_none());
    assert_eq!(app.session.tool_id(), "place");
    assert_eq!(app.session.tool_options()["count"], 2);
    // The cursor carries the current file's thumbnail and the number of files.
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut shapes = vec![];
    for _ in 0..2 {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| crate::place::paint_cursor(&mut app, ui.ctx(), ui.painter(), Pos2::new(50.0, 50.0)));
        out.textures_delta.clear();
        shapes = out.shapes;
    }
    let mut text = String::new();
    let mut textured = false;
    for s in shapes {
        match s.shape {
            Shape::Text(t) => text.push_str(t.galley.text()),
            Shape::Mesh(m) => textured |= m.texture_id != egui::TextureId::default(),
            _ => {}
        }
    }
    assert!(textured, "the thumbnail");
    assert_eq!(text, "2", "the badge");
}

#[test]
fn one_file_places_centred_in_the_view_and_replace_swaps_the_selection() {
    let mut app = app();
    app.view_mut().unwrap().center = vectorcraft_geom::Point::new(120.0, 80.0);
    place_picked(&mut app, &temp_file("one.png", &png(300, 150, 300.0)));
    let d = app.ui.dialog.as_ref().expect("the Place dialog");
    assert_eq!(d.kind, crate::dialogs::place::KIND);
    assert!(d.bool("link") && !d.bool("__replace"), "Link on; Replace needs one selected object");
    assert_eq!(d.fields["__info"][0], "300 × 150 px, 300 ppi, RGB (72 pt × 36 pt)");
    let text = crate::tests_labels::painted_text(&mut app, |app, ui| crate::dialogs::show(app, ui.ctx()));
    for label in ["Place", "one.png", "Link", "Template", "Replace", "Cancel"] {
        assert!(text.contains(label), "{label} in {text}");
    }
    crate::dialogs::confirm(&mut app).unwrap();
    assert!(app.ui.dialog.is_none());
    let first = selected_image(&app);
    assert_eq!(first.geometric_bounds().unwrap().center(), vectorcraft_geom::Point::new(120.0, 80.0), "centred in the view");
    // With one object selected, Replace applies.
    place_picked(&mut app, &temp_file("two.png", &png(40, 40, 144.0)));
    assert!(app.ui.dialog.as_ref().unwrap().bool("__replace"));
    app.ui.dialog.as_mut().unwrap().fields.insert("replace".into(), json!(true));
    app.ui.dialog.as_mut().unwrap().fields.insert("link".into(), json!(false));
    crate::dialogs::confirm(&mut app).unwrap();
    let second = selected_image(&app);
    assert_eq!(second.name.as_deref(), Some("two.png"));
    assert_eq!(app.session.active().unwrap().doc.layers[0].children().unwrap().len(), 1, "replaced");
    assert!(!app.ui.place_link, "Link is remembered");
}

#[test]
fn the_control_bar_shows_the_image_file_link_colour_mode_and_ppi() {
    let mut app = app();
    let path = temp_file("photo.png", &png(30, 30, 300.0));
    app.run("file.place", json!({"path": path})).unwrap();
    let text = crate::tests_labels::painted_text(&mut app, crate::chrome::control_bar);
    for s in ["Linked File", "photo.png", "RGB   PPI: 300"] {
        assert!(text.contains(s), "{s} in {text}");
    }
    let v: Value = app.run("file.place", json!({"path": path, "link": false})).unwrap();
    assert_eq!(v["linked"], false);
    let text = crate::tests_labels::painted_text(&mut app, crate::chrome::control_bar);
    assert!(text.contains("Embedded"), "{text}");
}
