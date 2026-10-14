//! Roidery: File → Export Machine File… (`file.exportMachine`). Picks a file, sews the document
//! with `stitch.export` and writes the machine file there (the web downloads it).

use serde_json::{Value, json};

use vectorcraft_engine::cmd::fileio;

use crate::{FilePick, VectorcraftApp, io};

/// The machine formats `stitch.export` writes: (extension, save-panel label, extensions).
pub const FORMATS: [(&str, &str, &[&str]); 8] = [
    ("dst", "Tajima DST", &["dst"]),
    ("pes", "Brother PES", &["pes"]),
    ("jef", "Janome JEF", &["jef"]),
    ("exp", "Melco EXP", &["exp"]),
    ("vp3", "Husqvarna Viking VP3", &["vp3"]),
    ("xxx", "Singer XXX", &["xxx"]),
    ("u01", "Barudan U01", &["u01"]),
    ("tap", "Tajima TAP", &["tap"]),
];

/// `ext` when it names a machine format, ignoring case.
fn known(ext: &str) -> Option<&'static str> {
    FORMATS.iter().map(|f| f.0).find(|f| f.eq_ignore_ascii_case(ext))
}

/// The save panel's choice (the web: the suggested name), given the suggested extension when it
/// names no machine format.
fn pick(app: &mut VectorcraftApp, pick: FilePick) -> Result<String, String> {
    if io::is_web(app) {
        return Ok(pick.name);
    }
    let picked = crate::picks::save(app, &pick).ok_or("cancelled")?;
    if known(&fileio::extension(&picked)).is_some() {
        return Ok(picked);
    }
    Ok(format!("{picked}.{}", fileio::extension(&pick.name)))
}

/// `{path?, format?, …stitch.export options}`. Without a path, a save panel opens on the format
/// used last (DST at first), its filter list holding every machine format. The format comes from
/// `format`, else the path's extension, else the one used last → `{path, format, stitches, …}`.
pub fn export(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    let mut params = p.as_object().cloned().unwrap_or_default();
    let last = known(&app.ui.machine_format).unwrap_or("dst");
    let given = params.remove("path").and_then(|v| v.as_str().map(str::to_string));
    let asked = params.get("format").and_then(Value::as_str).map(str::to_string);
    let path = match given {
        Some(path) => path,
        None => {
            let first = asked.as_deref().and_then(known).unwrap_or(last);
            let (name, folder) = io::suggested(app, first);
            let mut filters: Vec<_> = FORMATS.iter().map(|f| (f.1, f.2)).collect();
            if let Some(i) = FORMATS.iter().position(|f| f.0 == first) {
                filters.swap(0, i);
            }
            pick(app, FilePick { name, folder, filters })?
        }
    };
    let format = match asked {
        Some(f) => f,
        None => known(&fileio::extension(&path)).unwrap_or(last).to_string(),
    };
    params.insert("format".into(), json!(format));
    if !params.contains_key("name") {
        params.insert("name".into(), json!(fileio::file_stem(&path)));
    }
    let mut v = app.session.execute("stitch.export", &Value::Object(params)).map_err(|e| e.to_string())?;
    let o = v.as_object_mut().ok_or("no data")?;
    let bytes = o.remove("dataBase64").and_then(|b| b.as_str().and_then(vectorcraft_format::base64_decode)).ok_or("no data")?;
    io::write_to(&mut app.services, &path, &bytes)?;
    if let Some(f) = known(&format) {
        app.ui.machine_format = f.to_string();
    }
    let stitches = o.get("stitches").and_then(Value::as_u64).unwrap_or_default();
    let done = tl!("Exported {path}: {n} stitches").replace("{path}", &path).replace("{n}", &stitches.to_string());
    // Warn when the design won't go in the design's hoop as it is.
    let done = match o.get("hoop").and_then(|h| h["fit"].as_str()) {
        Some("tooLarge") => format!("{done}. {}", tl!("The design is larger than the hoop.")),
        Some("rotate") => format!("{done}. {}", tl!("Turn the design 90° to fit the hoop.")),
        _ => done,
    };
    app.status(done);
    o.insert("path".into(), json!(path));
    Ok(v)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use vectorcraft_engine::Session;

    use super::*;
    use crate::Services;

    type Written = Rc<RefCell<Vec<(String, Vec<u8>)>>>;

    fn app() -> (VectorcraftApp, Written) {
        let written = Written::default();
        let w = written.clone();
        let services = Services {
            write: Some(Box::new(move |p: &str, b: &[u8]| {
                w.borrow_mut().push((p.to_string(), b.to_vec()));
                Ok(())
            })),
            ..Default::default()
        };
        let mut app = VectorcraftApp::new(Session::new(), services);
        app.session.execute("file.new", &json!({"width": 60, "height": 40})).unwrap();
        app.session.execute("shape.rectangle", &json!({"x": 5, "y": 5, "width": 20, "height": 10})).unwrap();
        (app, written)
    }

    #[test]
    fn writes_the_format_of_the_picked_name_and_remembers_it() {
        let (mut app, written) = app();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s = seen.clone();
        app.services.pick_save = Some(Box::new(move |f: &FilePick| {
            s.borrow_mut().push((f.name.clone(), f.filters.first().map(|x| x.0)));
            Some("/tmp/badge.pes".into())
        }));
        let r = app.run("file.exportMachine", json!({})).unwrap();
        assert_eq!((r["path"].as_str(), r["format"].as_str()), (Some("/tmp/badge.pes"), Some("pes")), "{r}");
        assert!(r["stitches"].as_u64().unwrap() > 0 && r.get("dataBase64").is_none());
        assert!(written.borrow()[0].1.starts_with(b"#PES"));
        assert_eq!(app.ui.machine_format, "pes");
        // The panel suggested DST first; next time it suggests PES.
        app.run("file.exportMachine", json!({})).unwrap();
        let seen = seen.borrow();
        assert_eq!(seen[0].1, Some("Tajima DST"));
        assert!(seen[1].0.ends_with(".pes") && seen[1].1 == Some("Brother PES"), "{:?}", seen[1]);
    }

    #[test]
    fn a_given_path_and_format_need_no_panel() {
        let (mut app, written) = app();
        app.run("file.exportMachine", json!({"path": "/tmp/a.jef"})).unwrap();
        app.run("file.exportMachine", json!({"path": "/tmp/b.bin", "format": "exp"})).unwrap();
        let w = written.borrow();
        assert_eq!(w[0].0, "/tmp/a.jef");
        let offset = u32::from_le_bytes(w[0].1[..4].try_into().unwrap());
        assert!(offset > 0x74 && (offset - 0x74) % 8 == 0, "JEF header: 0x74 + 8 per colour, not {offset}");
        assert_eq!(w[1].0, "/tmp/b.bin");
        assert!(app.run("file.exportMachine", json!({"path": "/tmp/c.dst", "format": "xyz"})).is_err());
    }

    #[test]
    fn cancelling_the_panel_writes_nothing() {
        let (mut app, written) = app();
        app.services.pick_save = Some(Box::new(|_: &FilePick| None));
        assert!(app.run("file.exportMachine", json!({})).is_err());
        assert!(written.borrow().is_empty());
    }
}
