//! Roidery: File → Export Mockup… (`file.exportMockup`). Without a garment it opens the Export
//! Mockup dialog; with one it picks a file (PNG or JPEG), pictures the design sewn on the garment
//! with `stitch.mockup` and writes it there (the web downloads it).

use serde_json::{Value, json};

use vectorcraft_engine::cmd::fileio;

use crate::{FilePick, VectorcraftApp, io};

/// What the mockup can be saved as: (format, save-panel label, extensions).
const FORMATS: [(&str, &str, &[&str]); 2] = [("png", "PNG", &["png"]), ("jpg", "JPEG", &["jpg", "jpeg"])];

/// The format a file name asks for.
fn format_of(path: &str) -> Option<&'static str> {
    let ext = fileio::extension(path).to_ascii_lowercase();
    FORMATS.iter().find(|f| f.2.contains(&ext.as_str())).map(|f| f.0)
}

/// `{garment?, placement?, color?, fabric?, ppm?, path?, format?}`. No garment: the dialog opens.
/// Without a path a save panel opens, PNG first → `stitch.mockup`'s result with `path`.
pub fn export(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    if p.get("garment").is_none() {
        return crate::dialogs::export_mockup::open(app);
    }
    let mut params = p.as_object().cloned().unwrap_or_default();
    let given = params.remove("path").and_then(|v| v.as_str().map(str::to_string));
    let path = match given {
        Some(path) => path,
        None => {
            let (name, folder) = io::suggested(app, "png");
            let name = format!("{} {}.png", fileio::file_stem(&name), tl!("mockup"));
            let pick = FilePick { name, folder, filters: FORMATS.iter().map(|f| (f.1, f.2)).collect() };
            if io::is_web(app) {
                pick.name
            } else {
                let picked = app.services.pick_save.as_mut().and_then(|f| f(&pick)).ok_or("cancelled")?;
                if format_of(&picked).is_some() { picked } else { format!("{picked}.png") }
            }
        }
    };
    if !params.contains_key("format") {
        params.insert("format".into(), json!(format_of(&path).unwrap_or("png")));
    }
    let mut v = app.session.execute("stitch.mockup", &Value::Object(params)).map_err(|e| e.to_string())?;
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

    fn app(written: &Written) -> VectorcraftApp {
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
        app
    }

    #[test]
    fn no_garment_opens_the_dialog() {
        let mut app = app(&Rc::default());
        app.run("file.exportMockup", json!({})).unwrap();
        assert_eq!(app.ui.dialog.as_ref().map(|d| d.kind.as_str()), Some(crate::dialogs::export_mockup::KIND));
    }

    #[test]
    fn writes_a_png_named_after_the_document() {
        let written: Written = Rc::default();
        let mut app = app(&written);
        let names = Rc::new(RefCell::new(Vec::new()));
        let n = names.clone();
        app.services.pick_save = Some(Box::new(move |f: &FilePick| {
            n.borrow_mut().push(f.name.clone());
            Some("/tmp/badge on a cap".into())
        }));
        let r = app.run("file.exportMockup", json!({"garment": "cap", "ppm": 1})).unwrap();
        assert_eq!(names.borrow().as_slice(), ["Badge mockup.png"]);
        assert_eq!((r["path"].as_str(), r["format"].as_str()), (Some("/tmp/badge on a cap.png"), Some("png")));
        assert!(written.borrow()[0].1.starts_with(b"\x89PNG"));
        // A JPEG when the name says so.
        let r = app.run("file.exportMockup", json!({"garment": "tote", "ppm": 1, "path": "/tmp/b.jpeg"})).unwrap();
        assert_eq!(r["format"], "jpg");
        assert!(written.borrow()[1].1.starts_with(&[0xFF, 0xD8]));
    }
}
