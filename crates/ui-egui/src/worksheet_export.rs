//! Roidery: File → Export Worksheet… (`file.exportWorksheet`). Picks a file (PDF, PNG or SVG),
//! draws the production worksheet with `stitch.worksheet` in the interface language and writes it
//! there (the web downloads it).

use serde_json::{Map, Value, json};

use vectorcraft_engine::cmd::fileio;

use crate::{FilePick, VectorcraftApp, io};

/// What the worksheet can be saved as: (extension, save-panel label, extensions).
const FORMATS: [(&str, &str, &[&str]); 3] = [("pdf", "PDF", &["pdf"]), ("png", "PNG", &["png"]), ("svg", "SVG", &["svg"])];

/// The sheet's words in the interface language, keyed by their English (see `stitch.worksheet`).
fn labels(app: &mut VectorcraftApp) -> Value {
    let mut m = Map::new();
    let words = [
        ("Production Worksheet", tl!("Production Worksheet")),
        ("Actual size", tl!("Actual size")),
        ("Scale {n}%", tl!("Scale {n}%")),
        ("Size", tl!("Size")),
        ("Stitches", tl!("Stitches")),
        ("Colors", tl!("Colors")),
        ("Trims", tl!("Trims")),
        ("Sew time", tl!("Sew time")),
        ("Thread", tl!("Thread")),
        ("Hoop", tl!("Hoop")),
        ("Fabric", tl!("Fabric")),
        ("{h} h {m} min", tl!("{h} h {m} min")),
        ("{m} min", tl!("{m} min")),
        ("{top} m + {bobbin} m bobbin", tl!("{top} m + {bobbin} m bobbin")),
        ("Fits the hoop", tl!("Fits the hoop")),
        ("Fits turned 90°", tl!("Fits turned 90°")),
        ("Too large for the hoop", tl!("Too large for the hoop")),
        ("Color sequence", tl!("Color sequence")),
        ("{n} stitches", tl!("{n} stitches")),
        ("and {n} more", tl!("and {n} more")),
    ];
    for (en, local) in words {
        m.insert(en.into(), json!(local));
    }
    // The design's fabric, as the Stitch Settings panel names it.
    if let Ok(q) = app.session.execute("stitch.design", &json!({}))
        && let (Some(en), Some(id)) = (q["fabricLabel"].as_str(), q["fabric"].as_str())
    {
        m.insert(en.into(), json!(crate::panels::threads::fabric_label(id)));
    }
    Value::Object(m)
}

/// `{path?, format?, paper?}`. Without a path a save panel opens, PDF first. The format comes from
/// `format`, else the path's extension, else PDF → `stitch.worksheet`'s result with `path`.
pub fn export(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    let mut params = p.as_object().cloned().unwrap_or_default();
    let given = params.remove("path").and_then(|v| v.as_str().map(str::to_string));
    let path = match given {
        Some(path) => path,
        None => {
            let (name, folder) = io::suggested(app, "pdf");
            let name = format!("{} {}.pdf", fileio::file_stem(&name), tl!("worksheet"));
            let pick = FilePick { name, folder, filters: FORMATS.iter().map(|f| (f.1, f.2)).collect() };
            if io::is_web(app) {
                pick.name
            } else {
                let picked = crate::picks::save(app, &pick).ok_or("cancelled")?;
                if FORMATS.iter().any(|f| f.0.eq_ignore_ascii_case(&fileio::extension(&picked))) { picked } else { format!("{picked}.pdf") }
            }
        }
    };
    if !params.contains_key("format") {
        let ext = fileio::extension(&path).to_ascii_lowercase();
        let format = FORMATS.iter().map(|f| f.0).find(|f| *f == ext).unwrap_or("pdf");
        params.insert("format".into(), json!(format));
    }
    if params.get("format").and_then(Value::as_str) == Some("png") && !params.contains_key("ppi") {
        // Sharp enough to print.
        params.insert("ppi".into(), json!(200));
    }
    if !params.contains_key("labels") {
        params.insert("labels".into(), labels(app));
    }
    let mut v = app.session.execute("stitch.worksheet", &Value::Object(params)).map_err(|e| e.to_string())?;
    let o = v.as_object_mut().ok_or("no data")?;
    let bytes = o.remove("dataBase64").and_then(|b| b.as_str().and_then(vectorcraft_format::base64_decode)).ok_or("no data")?;
    io::write_to(&mut app.services, &path, &bytes)?;
    app.status(tl!("Exported {path}").replace("{path}", &path));
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

    #[test]
    fn writes_a_pdf_named_after_the_document() {
        let written: Written = Rc::default();
        let w = written.clone();
        let services = Services {
            write: Some(Box::new(move |p: &str, b: &[u8]| {
                w.borrow_mut().push((p.to_string(), b.to_vec()));
                Ok(())
            })),
            ..Default::default()
        };
        let mut app = VectorcraftApp::new(Session::new(), services);
        app.session.execute("file.new", &json!({"width": 200, "height": 200, "title": "Badge"})).unwrap();
        app.session.execute("shape.rectangle", &json!({"x": 20, "y": 20, "width": 60, "height": 40})).unwrap();
        let names = Rc::new(RefCell::new(Vec::new()));
        let n = names.clone();
        app.services.pick_save = Some(Box::new(move |f: &FilePick| {
            n.borrow_mut().push(f.name.clone());
            Some("/tmp/badge sheet".into())
        }));
        let r = app.run("file.exportWorksheet", json!({})).unwrap();
        assert_eq!(names.borrow().as_slice(), ["Badge worksheet.pdf"]);
        assert_eq!((r["path"].as_str(), r["format"].as_str()), (Some("/tmp/badge sheet.pdf"), Some("pdf")));
        assert!(written.borrow()[0].1.starts_with(b"%PDF"));
        // A PNG when the name says so.
        let r = app.run("file.exportWorksheet", json!({"path": "/tmp/b.png"})).unwrap();
        assert_eq!(r["format"], "png");
    }
}
