//! #1575: commands that take `ids` work on the objects given, with nothing selected; the
//! selection is left as it was.

use serde_json::json;

use super::*;

fn doc_with_rect() -> (Session, u64) {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    let id = s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 50, "height": 40})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.none", &json!({})).unwrap();
    (s, id)
}

#[test]
fn ids_stand_in_for_the_selection() {
    for (cmd, params) in [
        ("object.repeat.radial", json!({})),
        ("edit.colors.invert", json!({})),
        ("object.pattern.make", json!({"name": "P"})),
        ("symbol.new", json!({"name": "S"})),
        ("object.mesh.create", json!({})),
        ("object.makePixelPerfect", json!({})),
    ] {
        let (mut s, id) = doc_with_rect();
        let mut p = params;
        p["ids"] = json!([id]);
        let r = s.execute(cmd, &p);
        assert!(r.is_ok(), "{cmd}: {r:?}");
    }
}

#[test]
fn the_selection_is_put_back_and_the_check_still_applies() {
    let (mut s, id) = doc_with_rect();
    // Disabled even with the object given: Make with Top Object needs two.
    let r = s.execute("object.envelope.makeWithTopObject", &json!({"ids": [id]}));
    assert!(matches!(r, Err(EngineError::Disabled(..))), "{r:?}");
    assert!(s.doc().unwrap().selection.objects.is_empty(), "the stand-in selection is put back");
    // Without ids, nothing selected is still nothing selected.
    let r = s.execute("object.repeat.radial", &json!({}));
    assert!(matches!(r, Err(EngineError::Disabled(..))), "{r:?}");
}

#[test]
fn only_an_ids_key_counts() {
    let p = json!({"ids": [3]});
    assert_eq!(given_ids("{ids?: [id]}", &p), Some(vec![NodeId(3)]));
    assert_eq!(given_ids("{id?|ids?, name}", &p), Some(vec![NodeId(3)]));
    assert_eq!(given_ids("{grids: n, guides: bool}", &p), None);
    // A word in what the description says about another key isn't the key.
    assert_eq!(given_ids("{name: of the swatch the ids of the selection take}", &p), None);
    assert_eq!(given_ids("{ids?: [id]}", &json!({"ids": []})), None);
}

/// Every command that names `ids` and is refused with nothing selected: given the object as `ids`
/// it does what it does with the object selected (the same result or the same error, and the same
/// document), so no command is let through to run on an empty selection.
#[test]
fn ids_do_what_the_selection_does_in_every_command_that_takes_them() {
    let mut checked = vec![];
    for spec in command_specs() {
        let (mut given, id) = doc_with_rect();
        if !cmd::params::takes_key(spec.params, "ids") || (spec.enabled)(&given).is_ok() {
            continue;
        }
        let (mut selected, _) = doc_with_rect();
        selected.execute("select.set", &json!({"ids": [id]})).unwrap();
        let (a, b) = (given.execute(spec.id, &json!({"ids": [id]})), selected.execute(spec.id, &json!({})));
        match (&a, &b) {
            (Ok(_), Ok(_)) => {}
            (Err(x), Err(y)) => assert_eq!(x.to_string(), y.to_string(), "{}", spec.id),
            _ => panic!("{}: with ids {a:?}, with the selection {b:?}", spec.id),
        }
        match (given.doc(), selected.doc()) {
            (Ok(g), Ok(s)) => assert!(g.doc == s.doc, "{}: the documents differ", spec.id),
            (g, s) => assert_eq!(g.is_ok(), s.is_ok(), "{}", spec.id),
        }
        checked.push((spec.id, a.is_ok()));
    }
    assert!(checked.iter().filter(|c| c.1).count() >= 20, "{checked:?}");
}
