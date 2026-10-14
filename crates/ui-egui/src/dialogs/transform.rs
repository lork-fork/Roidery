//! Transform dialogs: Move, Rotate, Scale, Reflect and Shear (with Transform Objects, Transform
//! Patterns, Copy and Preview; Scale also has Uniform, Scale Corners and Scale Strokes & Effects;
//! Reflect's axis is Horizontal, Vertical or an angle). With Preview on
//! (`preview`, off when the dialog opens), the transform shows on the canvas as the values change.

use serde_json::{Value, json};

use super::{DialogSpec, form, run_and_close};
use crate::VectorcraftApp;
use crate::state::Dialog;

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |d| title(&d.kind).into(), body, confirm, preview: true, ..DialogSpec::FORM };

fn title(kind: &str) -> &'static str {
    match kind {
        "move" => tl!("Move"),
        "rotate" => tl!("Rotate"),
        "scale" => tl!("Scale"),
        "reflect" => tl!("Reflect"),
        _ => tl!("Shear"),
    }
}

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    form::grid(ui, d, app.session.general_unit());
    ui.add_space(6.0);
    if d.kind == "scale" {
        form::check(ui, d, "uniform", tl!("Uniform"));
        scale_options(app, ui, d);
    }
    object_options(app, ui, d);
    form::check(ui, d, "copy", tl!("Copy (make a transformed copy)"));
    let (cmd, mut params) = command(app, d);
    if d.kind == "scale" {
        add_scale_options(app, d, &mut params);
    }
    form::preview(app, ui, d, title(&d.kind), cmd, params);
    false
}

/// Transform Patterns: the dialog's field, else General › Transform Pattern Tiles.
fn transform_patterns(app: &VectorcraftApp, d: &Dialog) -> bool {
    d.fields.get("patterns").and_then(Value::as_bool).unwrap_or(app.session.prefs.transform_pattern_tiles)
}

/// Transform Objects: the dialog's field, on until it is switched off (the pattern tiles alone
/// transform then, with Transform Patterns).
fn transform_objects(d: &Dialog) -> bool {
    d.fields.get("objects").and_then(Value::as_bool).unwrap_or(true)
}

/// The Transform Objects and Transform Patterns checkboxes (fields `objects` and `patterns`).
pub(super) fn object_options(app: &VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) {
    d.fields.insert("objects".into(), Value::Bool(transform_objects(d)));
    form::check(ui, d, "objects", tl!("Transform Objects"));
    d.fields.insert("patterns".into(), Value::Bool(transform_patterns(app, d)));
    form::check(ui, d, "patterns", tl!("Transform Patterns"));
}

/// Add the dialog's Transform Objects and Transform Patterns options to command `params`.
pub(super) fn add_object_options(app: &VectorcraftApp, d: &Dialog, params: &mut Value) {
    params["objects"] = json!(transform_objects(d));
    params["patterns"] = json!(transform_patterns(app, d));
}

/// The scale options: (dialog field, preference, label).
const SCALE_OPTIONS: [(&str, &str, &str); 2] = [("corners", "scaleCorners", "Scale Corners"), ("strokes", "scaleStrokes", "Scale Strokes & Effects")];

/// A scale option's value: the dialog's field, else the preference.
fn scale_option(app: &VectorcraftApp, d: &Dialog, field: &str) -> bool {
    let prefs = &app.session.prefs;
    d.fields.get(field).and_then(Value::as_bool).unwrap_or(if field == "corners" { prefs.scale_corners } else { prefs.scale_strokes })
}

/// The Scale Corners and Scale Strokes & Effects checkboxes (fields `corners` and `strokes`),
/// starting from the preferences.
pub(super) fn scale_options(app: &VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) {
    for (field, _, label) in SCALE_OPTIONS {
        let on = scale_option(app, d, field);
        d.fields.insert(field.into(), Value::Bool(on));
        form::check(ui, d, field, tl!(label));
    }
}

/// Add the dialog's scale options to command `params`.
pub(super) fn add_scale_options(app: &VectorcraftApp, d: &Dialog, params: &mut Value) {
    for (field, _, _) in SCALE_OPTIONS {
        params[field] = json!(scale_option(app, d, field));
    }
}

/// `params` with the dialog's scale options, which also become the preferences (they are shared
/// with the Transform panel and Preferences, as in the reference app).
pub(super) fn with_scale_options(app: &mut VectorcraftApp, d: &Dialog, mut params: Value) -> Result<Value, String> {
    add_scale_options(app, d, &mut params);
    let prefs: serde_json::Map<String, Value> = SCALE_OPTIONS.iter().map(|(field, pref, _)| (pref.to_string(), params[*field].clone())).collect();
    app.run("prefs.set", json!({ "values": prefs }))?;
    Ok(params)
}

/// Pass a tool-chosen reference point (origin) through to transform commands.
fn origin_params(d: &Dialog, mut p: Value) -> Value {
    if let Some(o) = d.fields.get("origin") {
        p["origin"] = o.clone();
    }
    p
}

/// The dialog's command and its params (Scale's without its scale options).
fn command(app: &VectorcraftApp, d: &Dialog) -> (&'static str, Value) {
    let (id, mut params) = match d.kind.as_str() {
        "move" => ("object.move", json!({"dx": d.f64("dx", 0.0), "dy": d.f64("dy", 0.0)})),
        "rotate" => ("object.rotate", json!({"angle": d.f64("angle", 0.0)})),
        "scale" => {
            let sx = d.f64("sx", 100.0);
            let sy = if d.bool("uniform") { sx } else { d.f64("sy", 100.0) };
            ("object.scale", json!({"sx": sx, "sy": sy}))
        }
        // The axis by name, or the angle it is at.
        "reflect" if d.str("axis") == "angle" => ("object.reflect", json!({"axis": d.f64("angle", 90.0)})),
        "reflect" => ("object.reflect", json!({"axis": d.fields.get("axis").cloned().unwrap_or(json!("vertical"))})),
        _ => ("object.shear", json!({"angle": d.f64("angle", 0.0), "axis": d.str("axis")})),
    };
    params["copy"] = json!(d.bool("copy"));
    add_object_options(app, d, &mut params);
    // Move has no reference point.
    if id == "object.move" { (id, params) } else { (id, origin_params(d, params)) }
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    // The preview is rolled back and the command run as without one, so its undo step, its journal
    // entry and Transform Again are the same either way.
    if app.session.in_interaction() {
        app.session.cancel_interaction().map_err(|e| e.to_string())?;
    }
    let (id, mut params) = command(app, d);
    if d.kind == "scale" {
        params = with_scale_options(app, d, params)?;
    }
    run_and_close(app, id, params)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vectorcraft_engine::Session;

    use crate::VectorcraftApp;

    #[test]
    fn scale_options_start_from_and_update_the_preferences() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
        app.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 50, "height": 50, "radius": 5})).unwrap();
        app.run("select.all", json!({})).unwrap();
        app.run("prefs.set", json!({"values": {"scaleStrokes": true, "scaleCorners": false}})).unwrap();
        crate::menus::invoke(&mut app, "object.scale", json!({}));
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| crate::dialogs::show(&mut app, ui.ctx()));
        out.textures_delta.clear();
        let d = app.ui.dialog.as_mut().unwrap();
        assert_eq!((&d.fields["strokes"], &d.fields["corners"]), (&json!(true), &json!(false)));
        d.fields.insert("sx".into(), json!(200));
        d.fields.insert("strokes".into(), json!(false));
        d.fields.insert("corners".into(), json!(true));
        crate::dialogs::confirm(&mut app).unwrap();
        let prefs = &app.session.prefs;
        assert_eq!((prefs.scale_strokes, prefs.scale_corners), (false, true));
        let n = &app.session.doc().unwrap().doc.layers[0].children().unwrap()[0];
        assert_eq!(n.appearance.stroke_width(), 1.0);
        let (cmd, p) = app.session.journal.last().unwrap();
        assert_eq!((cmd.as_str(), &p["strokes"], &p["corners"]), ("object.scale", &json!(false), &json!(true)));
    }

    /// #1412: Preview shows the transform on the canvas as the values change, ↑/↓ in the angle
    /// field step it by 1° (Shift: 10°), Cancel rolls the preview back and OK keeps one undo step
    /// that Transform Again repeats.
    #[test]
    fn preview_follows_the_stepped_angle() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
        app.run("shape.rectangle", json!({"x": 100, "y": 100, "width": 60, "height": 20})).unwrap();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let frame = |app: &mut VectorcraftApp, key: Option<(egui::Key, egui::Modifiers)>| {
            let events = key.map(|(key, modifiers)| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
            let mut out =
                ctx.run_ui(egui::RawInput { events: events.into_iter().collect(), ..Default::default() }, |ui| crate::dialogs::show(app, ui.ctx()));
            out.textures_delta.clear();
        };
        let size = |app: &VectorcraftApp| {
            let b = app.session.doc().unwrap().doc.layers[0].children().unwrap()[0].geometric_bounds().unwrap();
            (b.width().round(), b.height().round())
        };
        let undo_steps = |app: &VectorcraftApp| app.session.doc().unwrap().history.undo.len();
        let steps = undo_steps(&app);
        crate::menus::invoke(&mut app, "object.rotate", json!({}));
        frame(&mut app, None);
        assert!(!app.session.in_interaction(), "Preview is off when the dialog opens");
        // ↑ nine times with Shift: 90°, in the dialog only.
        for _ in 0..9 {
            frame(&mut app, Some((egui::Key::ArrowUp, egui::Modifiers::SHIFT)));
        }
        frame(&mut app, Some((egui::Key::ArrowUp, egui::Modifiers::NONE)));
        frame(&mut app, Some((egui::Key::ArrowDown, egui::Modifiers::NONE)));
        assert_eq!(app.ui.dialog.as_ref().unwrap().fields["angle"], json!(90.0));
        assert_eq!(size(&app), (60.0, 20.0));
        // Preview on: the canvas shows it, and follows the next steps.
        app.ui.dialog.as_mut().unwrap().fields.insert("preview".into(), json!(true));
        frame(&mut app, None);
        assert!(app.session.in_interaction());
        assert_eq!(size(&app), (20.0, 60.0));
        for _ in 0..9 {
            frame(&mut app, Some((egui::Key::ArrowUp, egui::Modifiers::SHIFT)));
        }
        assert_eq!(size(&app), (60.0, 20.0), "180°");
        // Cancel rolls it back.
        for _ in 0..9 {
            frame(&mut app, Some((egui::Key::ArrowDown, egui::Modifiers::SHIFT)));
        }
        crate::dialogs::cancel(&mut app);
        assert!(!app.session.in_interaction());
        assert_eq!((size(&app), undo_steps(&app)), ((60.0, 20.0), steps));
        // OK keeps the previewed transform as one undo step, and Transform Again repeats it.
        crate::menus::invoke(&mut app, "object.rotate", json!({}));
        app.ui.dialog.as_mut().unwrap().fields.extend([("preview".to_string(), json!(true)), ("angle".to_string(), json!(90))]);
        frame(&mut app, None);
        assert_eq!(size(&app), (20.0, 60.0));
        crate::dialogs::confirm(&mut app).unwrap();
        assert!(!app.session.in_interaction() && app.ui.dialog.is_none());
        assert_eq!((size(&app), undo_steps(&app)), ((20.0, 60.0), steps + 1));
        assert_eq!(app.session.journal.last().unwrap().0, "object.rotate");
        app.run("object.transformAgain", json!({})).unwrap();
        assert_eq!(size(&app), (60.0, 20.0));
    }

    /// #1460: Reflect's axis is Horizontal, Vertical or an angle, and Transform Objects goes with
    /// the command (on until switched off).
    #[test]
    fn reflect_takes_an_angle_and_transform_objects() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
        app.run("shape.rectangle", json!({"x": 100, "y": 100, "width": 60, "height": 20})).unwrap();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        for (axis, objects, size) in [("angle", true, (20.0, 60.0)), ("vertical", false, (20.0, 60.0))] {
            crate::menus::invoke(&mut app, "object.reflect", json!({}));
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| crate::dialogs::show(&mut app, ui.ctx()));
            out.textures_delta.clear();
            let d = app.ui.dialog.as_mut().unwrap();
            assert_eq!((&d.fields["objects"], &d.fields["angle"]), (&json!(true), &json!(90)));
            d.fields.extend([("axis".to_string(), json!(axis)), ("angle".to_string(), json!(45)), ("objects".to_string(), json!(objects))]);
            crate::dialogs::confirm(&mut app).unwrap();
            let (cmd, p) = app.session.journal.last().unwrap();
            let want = if axis == "angle" { json!(45.0) } else { json!(axis) };
            assert_eq!((cmd.as_str(), &p["axis"], &p["objects"]), ("object.reflect", &want, &json!(objects)));
            // Across the 45° axis the rectangle's sides swap; without Transform Objects it stays.
            let b = app.session.doc().unwrap().doc.layers[0].children().unwrap()[0].geometric_bounds().unwrap();
            assert_eq!((b.width().round(), b.height().round()), size);
        }
    }

    /// Transform Patterns starts from General › Transform Pattern Tiles (#394) and goes with the
    /// command.
    #[test]
    fn transform_patterns_starts_from_the_preference() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
        app.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 50, "height": 50})).unwrap();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        for on in [false, true] {
            app.run("prefs.set", json!({"key": "transformPatternTiles", "value": on})).unwrap();
            crate::menus::invoke(&mut app, "object.move", json!({}));
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| crate::dialogs::show(&mut app, ui.ctx()));
            out.textures_delta.clear();
            let d = app.ui.dialog.as_mut().unwrap();
            assert_eq!(d.fields["patterns"], json!(on));
            d.fields.insert("dx".into(), json!(10));
            crate::dialogs::confirm(&mut app).unwrap();
            let (cmd, p) = app.session.journal.last().unwrap();
            assert_eq!((cmd.as_str(), &p["patterns"]), ("object.move", &json!(on)));
        }
    }
}
