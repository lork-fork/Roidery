//! SVG encodings, profiles and embedded fonts through the engine: the options reach the file,
//! files in each encoding open again, and `document.serialize` reads them as text.

use serde_json::json;

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 100})).unwrap();
    s.execute("text.create", &json!({"x": 10, "y": 40, "text": "Grüße €"})).unwrap();
    s
}

fn serialize(s: &mut Session, svg: serde_json::Value) -> serde_json::Value {
    s.execute("document.serialize", &json!({"format": "svg", "svg": svg})).unwrap()
}

#[test]
fn each_encoding_reopens_and_serializes_as_text() {
    let mut s = session();
    for (encoding, name, bom) in [("utf8", "UTF-8", false), ("utf16", "UTF-16", true), ("latin1", "ISO-8859-1", false)] {
        let r = serialize(&mut s, json!({"encoding": encoding}));
        let text = r["text"].as_str().unwrap();
        assert!(text.contains("Grüße") && text.contains(&format!("encoding=\"{name}\"")), "{text}");
        let bytes = match r.get("dataBase64") {
            Some(b) => vectorcraft_format::base64_decode(b.as_str().unwrap()).unwrap(),
            None => text.as_bytes().to_vec(),
        };
        assert_eq!(bom, bytes.starts_with(&[0xfe, 0xff]), "{encoding}");
        assert_eq!(r.get("dataBase64").is_some(), encoding != "utf8", "{encoding}");
        for (file, data) in [("t.svg", bytes.clone()), ("t.svgz", vectorcraft_svg::compress_bytes(&bytes))] {
            let mut o = Session::new();
            o.execute("document.open", &json!({"name": file, "dataBase64": vectorcraft_format::base64_encode(&data)})).unwrap();
            let plain = o.execute("document.inspect", &json!({})).unwrap().to_string();
            assert!(plain.contains("Grüße €"), "{encoding} {file}: {plain}");
        }
    }
}

#[test]
fn profile_and_embedded_fonts_reach_the_writer() {
    let mut s = session();
    let tiny = serialize(&mut s, json!({"profile": "tiny12", "styling": "css"}));
    let tiny = tiny["text"].as_str().unwrap();
    assert!(tiny.contains("baseProfile=\"tiny\"") && !tiny.contains("<style>"), "{tiny}");
    let fonts = serialize(&mut s, json!({"embedFonts": true}));
    assert!(fonts["text"].as_str().unwrap().contains("@font-face{"));
    assert!(s.execute("document.serialize", &json!({"format": "svg", "svg": {"profile": "svg2"}})).is_err(), "unknown profiles are refused");
    assert!(s.execute("document.serialize", &json!({"format": "svg", "encoding": "ebcdic"})).is_err(), "unknown encodings are refused");
}
