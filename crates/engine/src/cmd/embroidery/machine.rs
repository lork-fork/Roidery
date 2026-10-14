//! Opening machine files (DST, PES, JEF, EXP, VP3, XXX, U01, TAP, DSB, DSZ, HUS) for editing. Each colour block becomes a
//! group named after its thread, and each run of stitches between jumps and trims a path whose
//! anchors are the needle penetrations, sewn as Manual stitches in the thread's colour. Saved
//! again, the design sews as it did.

use std::fmt::Write as _;

use roidery_formats::{Command, Format};
use roidery_stitch::{StitchSettings, StitchType};
use vectorcraft_doc::{Document, NodeKind};

use super::*;

/// mm of fabric left round the design on the artboard.
const MARGIN: f64 = 10.0;
/// Thread colours for formats that don't store any (DST, EXP): one per colour block, in turn.
const DEFAULT_THREADS: [[u8; 3]; 6] =
    [[0x1d, 0x35, 0x57], [0xe6, 0x39, 0x46], [0x2a, 0x9d, 0x8f], [0xf4, 0xa2, 0x61], [0x6a, 0x4c, 0x93], [0x26, 0x46, 0x53]];

/// The document a machine file opens as, and warnings.
pub fn import(ext: &str, bytes: &[u8]) -> Result<(Document, Vec<String>)> {
    let format = Format::from_extension(ext).ok_or_else(|| EngineError::Other(format!("`.{ext}` is not a machine embroidery format")))?;
    let design = format.read(bytes).map_err(|e| EngineError::Other(format!("can't open the {} file: {e}", format.label())))?;
    let Some(b) = design.bounds() else {
        return Err(EngineError::Other(format!("the {} file has no stitches", format.label())));
    };
    // Runs of stitches per colour block, in millimetres from the artboard's corner.
    let (dx, dy) = (MARGIN - b.min_x, MARGIN - b.min_y);
    let mut blocks: Vec<Vec<Vec<(f64, f64)>>> = vec![vec![]];
    let mut run: Vec<(f64, f64)> = vec![];
    let mut at: Option<(f64, f64)> = None;
    let flush = |run: &mut Vec<(f64, f64)>, blocks: &mut Vec<Vec<Vec<(f64, f64)>>>| {
        if run.len() >= 2
            && let Some(block) = blocks.last_mut()
        {
            block.push(std::mem::take(run));
        }
        run.clear();
    };
    for st in &design.stitches {
        let p = (st.x + dx, st.y + dy);
        match st.command {
            Command::Stitch => {
                // A run starts where the needle was: after a jump, that is where it landed.
                if run.is_empty()
                    && let Some(a) = at
                {
                    run.push(a);
                }
                run.push(p);
            }
            Command::Jump | Command::Trim => flush(&mut run, &mut blocks),
            Command::ColorChange => {
                flush(&mut run, &mut blocks);
                blocks.push(vec![]);
            }
            Command::End => flush(&mut run, &mut blocks),
        }
        at = Some(p);
    }
    flush(&mut run, &mut blocks);

    let (w, h) = (b.width() + 2.0 * MARGIN, b.height() + 2.0 * MARGIN);
    let mut svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}mm" height="{h}mm" viewBox="0 0 {w} {h}">"#);
    let mut warnings = vec![];
    for (i, block) in blocks.iter().enumerate().filter(|(_, b)| !b.is_empty()) {
        let thread = design.threads.get(i);
        let rgb = thread.map_or(DEFAULT_THREADS[i % DEFAULT_THREADS.len()], |t| t.rgb);
        let name = match thread {
            Some(t) if !t.name.is_empty() => t.name.clone(),
            _ => format!("Thread {}", i + 1),
        };
        let _ = write!(svg, r#"<g id="{}">"#, xml_escape(&name));
        for run in block {
            let mut d = String::new();
            for (k, (x, y)) in run.iter().enumerate() {
                let _ = write!(d, "{}{x:.2} {y:.2} ", if k == 0 { "M" } else { "L" });
            }
            let _ = write!(
                svg,
                r##"<path d="{}" fill="none" stroke="#{:02x}{:02x}{:02x}" stroke-width="0.35" stroke-linejoin="round"/>"##,
                d.trim_end(),
                rgb[0],
                rgb[1],
                rgb[2]
            );
        }
        svg.push_str("</g>");
    }
    svg.push_str("</svg>");
    if design.threads.is_empty() && blocks.iter().filter(|b| !b.is_empty()).count() > 0 {
        warnings.push(format!("{} files don't store thread colours: the colour blocks got stand-in colours", format.label()));
    }
    let (mut doc, w2) = vectorcraft_svg::import_with_report(&svg).map_err(|e| EngineError::Other(format!("can't open the design: {e}")))?;
    warnings.extend(w2);

    // Every run sews its own points.
    let manual = serde_json::to_value(StitchSettings::of(StitchType::Manual)).map_err(|e| EngineError::Other(e.to_string()))?;
    let mut paths = vec![];
    doc.walk(|n| {
        if matches!(n.kind, NodeKind::Path { .. }) {
            paths.push(n.id);
        }
    });
    for id in paths {
        if let Some(n) = doc.node_mut(id) {
            n.edit_attrs(|a| a.stitch = Some(manual.clone()));
        }
    }
    Ok((doc, warnings))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use roidery_formats::{Design, MachineStitch, Thread};
    use serde_json::json;

    use super::*;

    fn design() -> Design {
        let s = |x: f64, y: f64, c: Command| MachineStitch::new(x, y, c);
        Design {
            stitches: vec![
                s(0.0, 0.0, Command::Jump),
                s(10.0, 0.0, Command::Stitch),
                s(10.0, 10.0, Command::Stitch),
                s(30.0, 10.0, Command::Jump),
                s(30.0, 20.0, Command::Stitch),
                s(40.0, 20.0, Command::Stitch),
                s(40.0, 20.0, Command::ColorChange),
                s(40.0, 30.0, Command::Stitch),
                s(50.0, 30.0, Command::Stitch),
                s(50.0, 30.0, Command::End),
            ],
            threads: vec![
                Thread { rgb: [200, 30, 40], name: "Red".into(), code: String::new() },
                Thread { rgb: [20, 40, 200], name: String::new(), code: String::new() },
            ],
            name: String::new(),
        }
    }

    #[test]
    fn runs_become_manual_paths_in_thread_colours_and_sew_the_same() {
        let bytes = Format::Pes.write(&design()).unwrap();
        let mut s = Session::new();
        let r = s.execute("document.open", &json!({"name": "flower.pes", "dataBase64": vectorcraft_format::base64_encode(&bytes)})).unwrap();
        assert!(r.is_object());
        let info = s.execute("stitch.info", &json!({})).unwrap();
        let objs = info["objects"].as_array().unwrap();
        // Two runs in the first block, one in the second.
        assert_eq!(objs.len(), 3, "{info}");
        assert!(objs.iter().all(|o| o["type"] == "manual"));
        // Saved again, the penetrations are where they were.
        let out = s.execute("stitch.export", &json!({"format": "pes"})).unwrap();
        let again = Format::Pes.read(&vectorcraft_format::base64_decode(out["dataBase64"].as_str().unwrap()).unwrap()).unwrap();
        let sewn = |d: &Design| -> Vec<(i64, i64)> {
            let b = d.bounds().unwrap();
            d.stitches
                .iter()
                .filter(|p| p.command == Command::Stitch)
                .map(|p| (((p.x - b.min_x) * 10.0).round() as i64, ((p.y - b.min_y) * 10.0).round() as i64))
                .collect()
        };
        let (before, after) = (sewn(&Format::Pes.read(&bytes).unwrap()), sewn(&again));
        for p in &before {
            assert!(after.contains(p), "{p:?} missing from {after:?}");
        }
        // PES keeps colours as Brother's palette: the first block keeps the colour it read as.
        let read = Format::Pes.read(&bytes).unwrap();
        assert_eq!(again.threads.first().map(|t| t.rgb), read.threads.first().map(|t| t.rgb));
    }

    #[test]
    fn every_writable_format_opens_with_its_runs() {
        for f in Format::writable() {
            let bytes = f.write(&design()).unwrap();
            let mut s = Session::new();
            let name = format!("flower.{}", f.extension());
            s.execute("document.open", &json!({"name": name, "dataBase64": vectorcraft_format::base64_encode(&bytes)})).unwrap();
            let info = s.execute("stitch.info", &json!({})).unwrap();
            // VP3 has no jump: the move between the first block's runs reads back as a stitch.
            let runs = if f == Format::Vp3 { 2 } else { 3 };
            assert_eq!(info["objects"].as_array().unwrap().len(), runs, "{}: {info}", f.label());
        }
    }

    #[test]
    fn bad_files_are_errors() {
        let mut s = Session::new();
        let open = |s: &mut Session, name: &str, b: &[u8]| {
            s.execute("document.open", &json!({"name": name, "dataBase64": vectorcraft_format::base64_encode(b)}))
        };
        assert!(open(&mut s, "x.dst", b"not a dst").is_err());
        assert!(open(&mut s, "x.pes", &[0u8; 3]).is_err());
        assert!(open(&mut s, "x.hus", &[0u8; 3]).is_err());
        assert!(open(&mut s, "x.dsz", &[0u8; 30]).is_err());
        assert!(import("txt", b"").is_err());
    }
}
