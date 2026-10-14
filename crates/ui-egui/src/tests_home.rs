//! The Home icon shows the Home screen over open documents; Cmd+N opens the New Document dialog.

use serde_json::json;

use crate::{VectorcraftApp, canvas};

fn app_with_doc() -> VectorcraftApp {
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({})).unwrap();
    app
}

/// Draw one canvas frame; whether it drew the document (else the Home screen).
fn shows_document(app: &mut VectorcraftApp) -> bool {
    app.canvas_rect = None;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| canvas::show(app, ui));
    out.textures_delta.clear();
    app.canvas_rect.is_some()
}

#[test]
fn home_shows_over_open_documents_until_a_document_is_chosen() {
    let mut app = app_with_doc();
    assert!(shows_document(&mut app));
    app.run("app.home", json!({})).unwrap();
    assert!(!shows_document(&mut app), "Home replaces the canvas");
    assert_eq!(app.session.documents().len(), 1, "the document stays open");
    assert!(app.ui.dialog.is_none(), "Home is not the New Document dialog");
    // Choosing the document's tab returns to it.
    app.ui.home = None;
    assert!(shows_document(&mut app));
    // A new document (from Home's presets, New… or Open) replaces Home too.
    app.run("app.home", json!({})).unwrap();
    app.run("file.new", json!({})).unwrap();
    assert!(shows_document(&mut app));
    assert!(app.ui.home.is_none());
}

#[test]
fn cmd_n_opens_the_new_document_dialog() {
    assert_eq!(crate::shortcut_editor::command_for_key("Cmd+N"), Some("file.newDialog"));
    let mut app = app_with_doc();
    app.run("file.newDialog", json!({})).unwrap();
    assert_eq!(app.ui.dialog.as_ref().map(|d| d.kind.as_str()), Some("newDocument"));
    assert_eq!(app.session.documents().len(), 1, "no document is made until the dialog's OK");
}
