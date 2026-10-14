//! Edit → Color Settings and Assign Profile with non-default profiles. The Color Settings are
//! process-wide and every colour shown or rendered goes through them, so these tests run in their
//! own test binary (process) instead of next to the engine's unit tests, which would see the
//! switched working spaces mid-test; here they take turns.

use std::sync::{Mutex, MutexGuard};

use serde_json::json;
use vectorcraft_color::cms;
use vectorcraft_engine::Session;
use vectorcraft_engine::cmd::colormgmt::{PROFILES_KEY, doc_profiles};
use vectorcraft_geom::Affine;
use vectorcraft_render::{RenderOptions, Renderer};
use vectorcraft_testkit::fixtures::{exec, id_of, rect, select, session};

/// The active Color Settings: one test at a time.
fn settings() -> MutexGuard<'static, ()> {
    static SETTINGS: Mutex<()> = Mutex::new(());
    SETTINGS.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn color_settings_lists_profiles_and_validates() {
    let _g = settings();
    let mut s = session();
    let r = s.execute("edit.colorSettings", &json!({})).unwrap();
    let names: Vec<&str> = r["profiles"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert!(names.contains(&cms::GENERIC_CMYK) && names.contains(&cms::SRGB) && names.contains(&cms::DEVICE_CMYK));
    assert!(s.execute("edit.colorSettings", &json!({"cmyk": "Missing.icc"})).is_err());
    assert_eq!(cms::active_settings().cmyk, r["cmyk"].as_str().unwrap(), "failed change leaves settings alone");
    let before = r["bpc"].as_bool().unwrap();
    let r2 = s.execute("edit.colorSettings", &json!({"bpc": !before})).unwrap();
    assert_eq!(r2["bpc"], !before);
    s.execute("edit.colorSettings", &json!({"bpc": before})).unwrap();
    assert!(s.execute("color.loadProfile", &json!({"path": "/nonexistent/x.icc"})).is_err());
}

#[test]
fn legacy_profile_name_resolves_in_commands_and_files() {
    let _g = settings();
    let (old, _) = cms::LEGACY_NAMES[0];
    let mut s = session();
    let before = cms::active_settings();
    let r = s.execute("edit.colorSettings", &json!({"rgb": old})).unwrap();
    assert_eq!(r["rgb"], cms::WIDE_GAMUT_RGB);
    assert_eq!(cms::active_settings().rgb, cms::WIDE_GAMUT_RGB);
    let names: Vec<&str> = r["profiles"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert!(names.contains(&cms::WIDE_GAMUT_RGB) && !names.contains(&old));
    let r = s.execute("edit.assignProfile", &json!({"rgb": old})).unwrap();
    assert_eq!(r["rgb"], cms::WIDE_GAMUT_RGB, "stored under the current name");
    // A document saved by an older version names the profile the old way.
    let mut d = (*s.doc().unwrap().doc).clone();
    d.unknown.insert(PROFILES_KEY.into(), json!({"rgb": old, "cmyk": null}));
    let back = vectorcraft_format::load(&vectorcraft_format::save(&d, false)).unwrap();
    assert_eq!(doc_profiles(&back), (Some(cms::WIDE_GAMUT_RGB.to_string()), None));
    cms::set_active(&before).unwrap();
}

/// Separations Preview follows the Color Settings and the profiles loaded: a renderer draws a gray
/// square on the Black plate through the new working CMYK profile, and the four process plates
/// through a proof profile loaded again under the same name, as a new renderer draws them.
#[test]
fn separations_preview_draws_through_new_color_settings() {
    let _g = settings();
    let mut s = session();
    let gray = rect(&mut s, 20.0, 20.0, 160.0, 120.0);
    exec(&mut s, "paint.setFill", json!({"ids": [gray.0], "color": "#808080"}));
    // The proof and Overprint Preview options the canvas takes from the view state the
    // commands set.
    let canvas = || RenderOptions {
        proof: vectorcraft_render::proof::active_proof(),
        overprint_preview: vectorcraft_render::proof::overprint_preview_on(),
        background: Some([255, 255, 255, 255]),
        ..Default::default()
    };
    let frame = |r: &mut Renderer, s: &Session| r.render(&s.doc().unwrap().doc, 400, 300, Affine::IDENTITY, &canvas()).pixels;
    exec(&mut s, "view.separationsPreview", json!({"only": "Black"}));
    let mut r = Renderer::new();
    let before = frame(&mut r, &s);
    let old = cms::active_settings();
    s.execute("edit.colorSettings", &json!({"cmyk": cms::DEVICE_CMYK})).unwrap();
    let fresh = frame(&mut Renderer::new(), &s);
    let warm = frame(&mut r, &s);
    cms::set_active(&old).unwrap();
    assert!(fresh != before, "the new profile changes the plate");
    assert!(warm == fresh);
    let name = "Separations Proof CMYK";
    cms::register_icc(&cms::icc_bytes(cms::GENERIC_CMYK).unwrap(), Some(name.into())).unwrap();
    exec(&mut s, "view.proofSetup", json!({"target": format!("cmyk:{name}")}));
    exec(&mut s, "view.separationsPreview", json!({"plates": ["Cyan", "Magenta", "Yellow", "Black"]}));
    let before = frame(&mut r, &s);
    cms::register_icc(&cms::icc_bytes(cms::DEVICE_CMYK).unwrap(), Some(name.into())).unwrap();
    let fresh = frame(&mut Renderer::new(), &s);
    let warm = frame(&mut r, &s);
    exec(&mut s, "view.proofSetup", json!({"target": "workingCmyk"}));
    exec(&mut s, "view.separationsPreview", json!({"on": false}));
    assert!(fresh != before, "the profile loaded again changes the plates");
    assert!(warm == fresh);
}

/// Raster effects follow the Color Settings: a blurred CMYK fill, and a white square's drop shadow
/// in a CMYK color, are drawn again through the new working CMYK profile, in a frame drawn while
/// interacting (as during a dialog's preview) and after it.
#[test]
fn raster_effects_draw_through_new_color_settings() {
    let _g = settings();
    let mut s = session();
    let blurred = rect(&mut s, 20.0, 20.0, 160.0, 120.0);
    exec(&mut s, "paint.setFill", json!({"ids": [blurred.0], "color": {"c": 0, "m": 60, "y": 100, "k": 0}}));
    exec(&mut s, "effect.apply", json!({"ids": [blurred.0], "effect": "blur.gaussian", "params": {"radius": 2}}));
    let lit = rect(&mut s, 220.0, 20.0, 120.0, 120.0);
    exec(&mut s, "paint.setFill", json!({"ids": [lit.0], "color": "#ffffff"}));
    let cyan = json!({"model": "cmyk", "c": 1.0, "m": 0.0, "y": 0.0, "k": 0.0});
    let shadow = json!({"x": 8, "y": 8, "blur": 1, "opacity": 100, "mode": "normal", "color": cyan});
    exec(&mut s, "effect.apply", json!({"ids": [lit.0], "effect": "stylize.dropShadow", "params": shadow}));
    let opts = RenderOptions { background: Some([255, 255, 255, 255]), ..Default::default() };
    let live = RenderOptions { interacting: true, ..opts.clone() };
    let frame = |r: &mut Renderer, s: &Session, o: &RenderOptions| r.render(&s.doc().unwrap().doc, 400, 300, Affine::IDENTITY, o).pixels;
    let mut r = Renderer::new();
    let before = frame(&mut r, &s, &opts);
    let old = cms::active_settings();
    s.execute("edit.colorSettings", &json!({"cmyk": cms::DEVICE_CMYK})).unwrap();
    let fresh = frame(&mut Renderer::new(), &s, &opts);
    assert!(fresh != before, "the new profile changes the frame");
    assert!(frame(&mut r, &s, &live) == fresh, "while interacting");
    assert!(frame(&mut r, &s, &opts) == fresh, "after it");
    cms::set_active(&old).unwrap();
}

/// Pattern tiles follow the Color Settings: a fill of a pattern in a CMYK color is drawn through
/// the new working CMYK profile in the next frame, as a renderer on a new thread (with no tiles of
/// its own yet) draws it.
#[test]
fn pattern_tiles_draw_through_new_color_settings() {
    use std::sync::Arc;
    use vectorcraft_color::{Color, Paint};
    use vectorcraft_doc::pattern::{PatternDef, pattern_paint};
    use vectorcraft_doc::{Appearance, Document, Node};
    use vectorcraft_geom::Rect;
    use vectorcraft_geom::shapes::rectangle;
    let _g = settings();
    let old = cms::active_settings();
    let mut d = Document::new(200.0, 100.0);
    let cyan = Appearance::basic(Paint::solid(Color::cmyk(1.0, 0.0, 0.0, 0.0)), Paint::None, 0.0);
    let square = Node::path(d.alloc_id(), rectangle(Rect::new(0.0, 0.0, 10.0, 10.0)), cyan);
    let mut def = PatternDef::new("Checks", vec![Arc::new(square)]);
    def.tile = Rect::new(0.0, 0.0, 20.0, 20.0);
    d.patterns.push(def);
    let checks = Appearance::basic(pattern_paint("Checks"), Paint::None, 0.0);
    let filled = Node::path(d.alloc_id(), rectangle(Rect::new(10.0, 10.0, 190.0, 90.0)), checks);
    let layer = d.layers[0].id;
    d.insert(Some(layer), 0, filled).unwrap();
    let opts = RenderOptions { background: Some([255, 255, 255, 255]), ..Default::default() };
    let frame = |r: &mut Renderer, d: &Document| r.render(d, 200, 100, Affine::IDENTITY, &opts).pixels;
    let mut r = Renderer::new();
    let before = frame(&mut r, &d);
    session().execute("edit.colorSettings", &json!({"cmyk": cms::DEVICE_CMYK})).unwrap();
    let warm = frame(&mut r, &d);
    let fresh = std::thread::scope(|t| t.spawn(|| frame(&mut Renderer::new(), &d)).join().unwrap());
    cms::set_active(&old).unwrap();
    assert!(fresh != before, "the new profile changes the frame");
    assert!(warm == fresh, "drawn as a renderer with no tiles draws it");
}

/// Live objects follow the Color Settings: in the next frame, a blend of an RGB red and a CMYK cyan,
/// the same blend in an envelope, an envelope that distorts the appearance of a CMYK cyan rectangle
/// with Revolve, and a gradient mesh in CMYK colors draw through the new working CMYK profile, and
/// so do the blend and the mesh with a drop shadow.
#[test]
fn live_objects_draw_through_new_color_settings() {
    let _g = settings();
    let old = cms::active_settings();
    let cyan = json!({"c": 100, "m": 0, "y": 0, "k": 0});
    let blend = |s: &mut Session| {
        let a = rect(s, 20.0, 40.0, 60.0, 120.0);
        exec(s, "paint.setFill", json!({"ids": [a.0], "color": "#ff0000"}));
        let b = rect(s, 300.0, 40.0, 60.0, 120.0);
        exec(s, "paint.setFill", json!({"ids": [b.0], "color": cyan}));
        id_of(&exec(s, "object.blend.make", json!({"ids": [a.0, b.0], "steps": 5})))
    };
    let mut blended = session();
    blend(&mut blended);
    let mut enveloped = session();
    let b = blend(&mut enveloped);
    exec(&mut enveloped, "object.envelope.makeWithWarp", json!({"ids": [b.0], "style": "arc", "bend": 30}));
    let mut revolved = session();
    let profile = rect(&mut revolved, 150.0, 60.0, 60.0, 120.0);
    exec(&mut revolved, "paint.setFill", json!({"ids": [profile.0], "color": cyan}));
    exec(&mut revolved, "effect.apply", json!({"ids": [profile.0], "effect": "threeD.revolve"}));
    let e = id_of(&exec(&mut revolved, "object.envelope.makeWithWarp", json!({"ids": [profile.0], "style": "arc", "bend": 30})));
    select(&mut revolved, &[e]);
    exec(&mut revolved, "object.envelope.options", json!({"distortAppearance": true}));
    let mesh = |s: &mut Session| {
        let m = rect(s, 20.0, 20.0, 360.0, 260.0);
        exec(s, "paint.setFill", json!({"ids": [m.0], "color": cyan}));
        exec(s, "object.mesh.create", json!({"ids": [m.0], "rows": 2, "cols": 2}));
        exec(s, "object.mesh.setPointColor", json!({"id": m.0, "index": 4, "color": {"c": 0, "m": 100, "y": 0, "k": 0}}));
        m
    };
    let mut meshed = session();
    mesh(&mut meshed);
    let shadow = json!({"x": 4, "y": 4, "blur": 1, "opacity": 100});
    let mut shadowed_blend = session();
    let b = blend(&mut shadowed_blend);
    exec(&mut shadowed_blend, "effect.apply", json!({"ids": [b.0], "effect": "stylize.dropShadow", "params": shadow}));
    let mut shadowed_mesh = session();
    let m = mesh(&mut shadowed_mesh);
    exec(&mut shadowed_mesh, "effect.apply", json!({"ids": [m.0], "effect": "stylize.dropShadow", "params": shadow}));
    let opts = RenderOptions { background: Some([255, 255, 255, 255]), ..Default::default() };
    let frame = |r: &mut Renderer, s: &Session| r.render(&s.doc().unwrap().doc, 400, 300, Affine::IDENTITY, &opts).pixels;
    for (what, mut s) in [
        ("the blend", blended),
        ("the envelope", enveloped),
        ("the revolved rectangle", revolved),
        ("the mesh", meshed),
        ("the blend with a drop shadow", shadowed_blend),
        ("the mesh with a drop shadow", shadowed_mesh),
    ] {
        let mut r = Renderer::new();
        let before = frame(&mut r, &s);
        s.execute("edit.colorSettings", &json!({"cmyk": cms::DEVICE_CMYK})).unwrap();
        let fresh = frame(&mut Renderer::new(), &s);
        let warm = frame(&mut r, &s);
        cms::set_active(&old).unwrap();
        assert!(fresh != before, "{what}: the new profile changes the frame");
        assert!(warm == fresh, "{what}: drawn as a new renderer draws it");
    }
}
