//! File › Export Worksheet (`stitch.worksheet`): a one-page production sheet to print and keep
//! with the job. It shows the design as it sews (at actual size when it fits the page) in its
//! hoop (when the hoop fits the page too), its size, stitches, colour changes, trims, sew time
//! and thread, the fabric, and each colour in sewing order with its thread and stitch count.
//!
//! The page is drawn as a document in a scratch session and written with `document.export`, so
//! it comes out as PDF, SVG or an image like any other export.

use roidery_formats::{Command, Design, hoops};
use roidery_plan::{PlanSettings, review};
use serde_json::{Value, json};

use super::design::DesignSettings;
use super::{MM_PER_PT, all_objects, sew};
use crate::Session;
use crate::cmd::{Result, bad, str_param};

const C: &str = "stitch.worksheet";
/// Page margin, pt.
const MARGIN: f64 = 36.0;
/// Colours listed before "and N more".
const MAX_COLOR_ROWS: usize = 16;
const INK: &str = "#1b2230";
const MUTED: &str = "#5a6577";
const RULE: &str = "#c9ced8";

/// A sheet being drawn: a scratch session holding one page (the mockup's backdrop is drawn the
/// same way).
pub(super) struct Sheet<'a> {
    pub s: Session,
    pub labels: Option<&'a Value>,
}

impl Sheet<'_> {
    fn t(&self, en: &'static str) -> String {
        self.labels.and_then(|l| l.get(en)).and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or(en).to_string()
    }

    pub fn run(&mut self, id: &str, p: Value) -> Result<Value> {
        self.s.execute(id, &p)
    }

    /// Style the object `id`: fill and stroke colours (`None`: no paint) and stroke weight.
    pub fn paint(&mut self, id: &Value, fill: Option<&str>, stroke: Option<(&str, f64)>, dash: Option<[f64; 2]>) -> Result<()> {
        self.run("select.set", json!({ "ids": [id] }))?;
        self.run("paint.setFill", fill.map_or(json!({ "none": true }), |c| json!({ "color": c })))?;
        match stroke {
            Some((c, w)) => {
                self.run("paint.setStroke", json!({ "color": c }))?;
                self.run("stroke.set", json!({ "weight": w, "cap": "round", "join": "round", "dash": dash.map(|d| d.to_vec()) }))?;
            }
            None => {
                self.run("paint.setStroke", json!({ "none": true }))?;
            }
        }
        self.run("select.none", json!({}))?;
        Ok(())
    }

    /// A rectangle `[x, y, width, height]`.
    fn rect(&mut self, [x, y, w, h]: [f64; 4], fill: Option<&str>, stroke: Option<(&str, f64)>, dash: Option<[f64; 2]>) -> Result<()> {
        let id = self.run("shape.rectangle", json!({ "x": x, "y": y, "width": w, "height": h }))?["id"].clone();
        self.paint(&id, fill, stroke, dash)
    }

    pub fn path(&mut self, d: &str, color: &str, weight: f64) -> Result<()> {
        let id = self.run("path.create", json!({ "d": d }))?["id"].clone();
        self.paint(&id, None, Some((color, weight)), None)
    }

    /// Text with its baseline at `y`.
    fn text(&mut self, x: f64, y: f64, text: &str, size: f64, color: &str) -> Result<()> {
        self.run("text.create", json!({ "x": x, "y": y, "text": text, "size": size, "color": color }))?;
        Ok(())
    }
}

/// The thread colour as `#rrggbb`.
pub(super) fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// SVG path data for each colour block's stitches, in page points: `at` maps millimetres.
fn block_paths(design: &Design, at: impl Fn(f64, f64) -> (f64, f64)) -> Vec<(String, usize)> {
    let mut blocks = vec![(String::new(), 0usize)];
    // Where the needle is, and whether the thread is down there.
    let mut last: Option<(f64, f64)> = None;
    let mut down = false;
    for s in &design.stitches {
        match s.command {
            Command::Stitch => {
                let (x, y) = at(s.x, s.y);
                let Some((d, n)) = blocks.last_mut() else { break };
                if !down {
                    let (lx, ly) = last.unwrap_or((x, y));
                    d.push_str(&format!("M{lx:.2} {ly:.2}"));
                    down = true;
                }
                d.push_str(&format!("L{x:.2} {y:.2}"));
                *n += 1;
                last = Some((x, y));
            }
            Command::Jump | Command::Trim => {
                last = Some(at(s.x, s.y));
                down = false;
            }
            Command::ColorChange => {
                blocks.push((String::new(), 0));
                last = Some(at(s.x, s.y));
                down = false;
            }
            Command::End => break,
        }
    }
    blocks
}

pub(super) fn worksheet(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let d = &st.doc;
    let ds = DesignSettings::of(d);
    // The document's title is its file name: without the extension.
    let name = str_param(p, "name").map(str::to_string).unwrap_or_else(|| {
        let t = std::path::Path::new(&d.title);
        if t.extension().is_some() { t.file_stem().map_or_else(|| d.title.clone(), |s| s.to_string_lossy().into_owned()) } else { d.title.clone() }
    });
    let (_, design) = sew(d, &all_objects(d), &PlanSettings::default(), &name);
    if design.stitch_count() == 0 {
        return Err(bad(C, "nothing sews: give some paths a stitch type first"));
    }
    let (pw, ph) = match str_param(p, "paper").unwrap_or("a4") {
        "a4" => (595.28, 841.89),
        "letter" => (612.0, 792.0),
        other => return Err(bad(C, format!("unknown paper `{other}`: a4 or letter"))),
    };
    let format = str_param(p, "format").unwrap_or("pdf");
    if !matches!(format, "pdf" | "svg" | "png") {
        return Err(bad(C, format!("unknown format `{format}`: pdf, svg or png")));
    }
    let bounds = design.bounds().ok_or_else(|| bad(C, "nothing sews"))?;
    let (wmm, hmm) = (bounds.width(), bounds.height());
    let minutes = review::sew_minutes(&design, &review::MachineSpeed::default());
    let thread = review::thread_use(&design);

    let mut sh = Sheet { s: Session::new(), labels: p.get("labels") };
    sh.run("file.new", json!({ "width": pw, "height": ph, "title": name, "units": "Millimeters" }))?;

    // Title.
    let title = if name.trim().is_empty() { "Roidery".to_string() } else { name.clone() };
    sh.text(MARGIN, MARGIN + 18.0, &title, 20.0, INK)?;
    let sub = sh.t("Production Worksheet");
    sh.text(MARGIN, MARGIN + 34.0, &sub, 9.0, MUTED)?;

    // The picture: the design as it sews, in its hoop, at actual size when it fits.
    let (bx, by, bw, bh) = (MARGIN, MARGIN + 46.0, pw - 2.0 * MARGIN, ph * 0.47);
    sh.rect([bx, by, bw, bh], Some("#eef0f4"), None, None)?;
    let hoop = ds.hoop.clone();
    // Scaled down only when the design itself doesn't fit; the hoop is drawn when it fits too.
    let room = |v: f64, box_pt: f64| (box_pt - 24.0) / (v.max(1e-6) / MM_PER_PT);
    let scale = room(wmm, bw).min(room(hmm, bh)).min(1.0);
    let (cx, cy) = (bx + bw / 2.0, by + bh / 2.0);
    // The design is centred on the hoop's centre (where the needle starts), as exported.
    let at = |x: f64, y: f64| (cx + x / MM_PER_PT * scale, cy + y / MM_PER_PT * scale);
    if let Some(h) = &hoop {
        let (w, ht) = (h.width / MM_PER_PT * scale, h.height / MM_PER_PT * scale);
        if w > bw - 8.0 || ht > bh - 8.0 {
            // Larger than the picture: left out.
        } else if h.round {
            let id = sh.run("shape.ellipse", json!({ "x": cx - w / 2.0, "y": cy - ht / 2.0, "width": w, "height": ht }))?["id"].clone();
            sh.paint(&id, Some("#ffffff"), Some((MUTED, 0.75)), Some([4.0, 3.0]))?;
        } else {
            sh.rect([cx - w / 2.0, cy - ht / 2.0, w, ht], Some("#ffffff"), Some((MUTED, 0.75)), Some([4.0, 3.0]))?;
        }
    }
    // The hoop's centre.
    sh.path(&format!("M{} {cy}H{}M{cx} {}V{}", cx - 8.0, cx + 8.0, cy - 8.0, cy + 8.0), MUTED, 0.5)?;
    let weight = (0.4 / MM_PER_PT * scale).max(0.2);
    let threads = &design.threads;
    let blocks = block_paths(&design, at);
    for (i, (path, _)) in blocks.iter().enumerate() {
        if path.is_empty() {
            continue;
        }
        let rgb = threads.get(i).map_or([0, 0, 0], |t| t.rgb);
        sh.path(path, &hex(rgb), weight)?;
    }
    let scale_text = if scale >= 0.999 { sh.t("Actual size") } else { sh.t("Scale {n}%").replace("{n}", &format!("{:.0}", scale * 100.0)) };
    sh.text(bx, by + bh + 12.0, &scale_text, 8.0, MUTED)?;

    // Details, left; colours, right.
    let top = by + bh + 34.0;
    let col2 = pw / 2.0 + 6.0;
    let row_h = 16.0;
    let m = if minutes.is_finite() { minutes.max(0.0).ceil() as u64 } else { 0 };
    let sew_time = if m >= 60 {
        sh.t("{h} h {m} min").replace("{h}", &(m / 60).to_string()).replace("{m}", &(m % 60).to_string())
    } else {
        sh.t("{m} min").replace("{m}", &m.max(1).to_string())
    };
    let top_m: f64 = thread.per_block.iter().sum();
    let mut details = vec![
        (sh.t("Size"), format!("{wmm:.1} × {hmm:.1} mm")),
        (sh.t("Stitches"), design.stitch_count().to_string()),
        (sh.t("Colors"), blocks.len().to_string()),
        (sh.t("Trims"), design.trims().to_string()),
        (sh.t("Sew time"), sew_time),
        (
            sh.t("Thread"),
            sh.t("{top} m + {bobbin} m bobbin").replace("{top}", &format!("{top_m:.1}")).replace("{bobbin}", &format!("{:.1}", thread.bobbin)),
        ),
    ];
    if let Some(h) = &hoop {
        let fit = match hoops::fit(wmm, hmm, h.width, h.height, h.round) {
            hoops::Fit::Fits => sh.t("Fits the hoop"),
            hoops::Fit::Rotated => sh.t("Fits turned 90°"),
            hoops::Fit::TooLarge { .. } => sh.t("Too large for the hoop"),
        };
        details.push((sh.t("Hoop"), format!("{} {} ({} × {} mm)", h.brand, h.name, h.width, h.height)));
        details.push((String::new(), fit));
    }
    let fabric = ds.fabric().label;
    let fabric = sh.labels.and_then(|l| l.get(fabric)).and_then(Value::as_str).unwrap_or(fabric).to_string();
    details.push((sh.t("Fabric"), fabric));
    for (i, (label, value)) in details.iter().enumerate() {
        let y = top + i as f64 * row_h;
        if !label.is_empty() {
            sh.text(MARGIN, y, label, 9.0, MUTED)?;
        }
        sh.text(MARGIN + 70.0, y, value, 9.0, INK)?;
    }
    let heading = sh.t("Color sequence");
    sh.text(col2, top, &heading, 9.0, MUTED)?;
    sh.path(&format!("M{col2} {}H{}", top + 5.0, pw - MARGIN), RULE, 0.5)?;
    for (i, (_, stitches)) in blocks.iter().enumerate().take(MAX_COLOR_ROWS) {
        let y = top + (i as f64 + 1.0) * row_h + 2.0;
        let t = threads.get(i);
        let rgb = t.map_or([0, 0, 0], |t| t.rgb);
        sh.rect([col2, y - 8.0, 10.0, 10.0], Some(&hex(rgb)), Some((RULE, 0.5)), None)?;
        let name = match t {
            Some(t) if !t.name.is_empty() => format!("{} {}", t.name, t.code).trim().to_string(),
            _ => hex(rgb),
        };
        sh.text(col2 + 16.0, y, &format!("{}. {name}", i + 1), 9.0, INK)?;
        let count = sh.t("{n} stitches").replace("{n}", &stitches.to_string());
        sh.text(pw - MARGIN - 70.0, y, &count, 9.0, MUTED)?;
    }
    if blocks.len() > MAX_COLOR_ROWS {
        let more = sh.t("and {n} more").replace("{n}", &(blocks.len() - MAX_COLOR_ROWS).to_string());
        sh.text(col2 + 16.0, top + (MAX_COLOR_ROWS as f64 + 1.0) * row_h + 2.0, &more, 9.0, MUTED)?;
    }

    let mut q = json!({ "format": format });
    if let Some(path) = str_param(p, "path") {
        q["path"] = json!(path);
    }
    if let Some(ppi) = p.get("ppi").filter(|_| format == "png") {
        q["ppi"] = ppi.clone();
    }
    let mut out = sh.run("document.export", q)?;
    out["scale"] = json!(scale);
    out["colors"] = json!(blocks.len());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    fn design(s: &mut Session, wide: f64) {
        s.execute("file.new", &json!({"width": 400, "height": 400, "title": "Badge"})).unwrap();
        s.execute("paint.setStroke", &json!({"none": true})).unwrap();
        s.execute("shape.rectangle", &json!({"x": 20, "y": 20, "width": wide, "height": 60})).unwrap();
        s.execute("select.none", &json!({})).unwrap();
        s.execute("paint.setFill", &json!({"color": "#1060c0"})).unwrap();
        s.execute("shape.ellipse", &json!({"x": 40, "y": 100, "width": 80, "height": 80})).unwrap();
    }

    fn svg(r: &serde_json::Value) -> String {
        String::from_utf8(vectorcraft_format::base64_decode(r["dataBase64"].as_str().unwrap()).unwrap()).unwrap()
    }

    #[test]
    fn a_small_design_is_drawn_at_actual_size_with_its_colours() {
        let mut s = Session::new();
        design(&mut s, 100.0);
        s.execute("stitch.setDesign", &json!({"hoop": "brother-5x7", "fitArtboard": false})).unwrap();
        let r = s.execute("stitch.worksheet", &json!({"format": "svg", "labels": {"Colors": "Farben"}})).unwrap();
        assert_eq!(r["scale"], 1.0);
        assert_eq!(r["colors"], 2);
        let text = svg(&r);
        for want in ["Badge", "Production Worksheet", "Actual size", "Farben", "Brother 5 × 7 in", "#1060c0"] {
            assert!(text.contains(want), "{want} missing");
        }
        // The document itself is unchanged.
        assert_eq!(s.doc().unwrap().doc.title, "Badge");
    }

    #[test]
    fn a_large_design_is_scaled_and_bad_params_are_errors() {
        let mut s = Session::new();
        // About 350 mm wide: more than an A4 page.
        design(&mut s, 1000.0);
        let r = s.execute("stitch.worksheet", &json!({"format": "svg", "paper": "letter"})).unwrap();
        let scale = r["scale"].as_f64().unwrap();
        assert!(scale < 0.6, "{scale}");
        assert!(svg(&r).contains(&format!("Scale {:.0}%", scale * 100.0)));
        let pdf = s.execute("stitch.worksheet", &json!({})).unwrap();
        assert!(vectorcraft_format::base64_decode(pdf["dataBase64"].as_str().unwrap()).unwrap().starts_with(b"%PDF"));
        assert!(s.execute("stitch.worksheet", &json!({"paper": "a0"})).is_err());
        assert!(s.execute("stitch.worksheet", &json!({"format": "dst"})).is_err());
        let mut empty = Session::new();
        empty.execute("file.new", &json!({})).unwrap();
        assert!(empty.execute("stitch.worksheet", &json!({})).is_err());
    }
}
