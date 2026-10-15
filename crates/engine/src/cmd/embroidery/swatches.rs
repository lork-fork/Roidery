//! Roidery: Test Swatches (`stitch.testSwatches`). A labelled grid of the same small shape sewn at
//! several densities (rows, `D…`) and pull compensations (columns, `P…`), to sew out once on the
//! machine and fabric at hand and read off which settings suit them. Satin swatches are rings
//! (straight runs and curves in one), fill swatches tatami squares. Labels are Roidery Sans
//! lettering sewn in triple run, which stays legible this small where satin letters clog.

use serde_json::{Value, json};
use vectorcraft_doc::NodeId;

use super::MM_PER_PT;
use crate::Session;
use crate::cmd::{Result, bad, str_param};

const C: &str = "stitch.testSwatches";
/// A swatch's size, the space between swatches, the label column's width and the header row's
/// height, mm.
const SIZE: f64 = 14.0;
const GAP: f64 = 6.0;
const LABEL_W: f64 = 18.0;
const HEADER_H: f64 = 10.0;
/// The satin ring's column width, mm.
const RING_WIDTH: f64 = 4.0;
/// The labels' cap height, mm.
const LABEL_H: f64 = 5.0;
/// Values per axis at most.
const MAX_VALUES: usize = 6;

/// `stitch.testSwatches`.
pub(super) fn test_swatches(s: &mut Session, p: &Value) -> Result<Value> {
    let fill = match str_param(p, "type").unwrap_or("satin") {
        "satin" => false,
        "fill" | "tatami" => true,
        t => return Err(bad(C, format!("unknown type `{t}`: use satin or fill"))),
    };
    let densities = values(p, "densities", &[0.3, 0.4, 0.5], 0.1, 5.0)?;
    let pulls = values(p, "pulls", &[0.0, 0.2, 0.4], -2.0, 5.0)?;
    let color = s.paint.stroke.color().or_else(|| s.paint.fill.color()).unwrap_or(vectorcraft_color::Color::BLACK).to_hex();
    let centre = s.doc()?.doc.artboards.first().map_or(kurbo::Point::ZERO, |a| a.rect.center());
    let (w, h) = (LABEL_W + pulls.len() as f64 * (SIZE + GAP), HEADER_H + densities.len() as f64 * (SIZE + GAP));
    // Top left of the grid, pt, so it sits in the middle of the first artboard.
    let (x0, y0) = (centre.x - w / 2.0 / MM_PER_PT, centre.y - h / 2.0 / MM_PER_PT);
    let at = |x: f64, y: f64| (x0 + x / MM_PER_PT, y0 + y / MM_PER_PT);
    in_one_step(s, |s| {
        let mut ids = vec![];
        for (c, pull) in pulls.iter().enumerate() {
            let cx = LABEL_W + c as f64 * (SIZE + GAP) + SIZE / 2.0;
            ids.push(label(s, &format!("P{}", short(*pull)), at(cx, HEADER_H / 2.0), &color)?);
        }
        for (r, density) in densities.iter().enumerate() {
            let cy = HEADER_H + r as f64 * (SIZE + GAP) + SIZE / 2.0;
            ids.push(label(s, &format!("D{}", short(*density)), at(LABEL_W / 2.0, cy), &color)?);
            for (c, pull) in pulls.iter().enumerate() {
                let cx = LABEL_W + c as f64 * (SIZE + GAP) + SIZE / 2.0;
                ids.push(swatch(s, fill, at(cx, cy), *density, *pull, &color)?);
            }
        }
        s.execute("select.set", &json!({ "ids": ids }))?;
        let group = s.execute("object.group", &json!({}))?;
        Ok(json!({ "id": group["id"], "swatches": densities.len() * pulls.len() }))
    })
}

/// A list of mm values from `p[key]`, else the defaults.
fn values(p: &Value, key: &str, default: &[f64], lo: f64, hi: f64) -> Result<Vec<f64>> {
    let Some(v) = p.get(key) else { return Ok(default.to_vec()) };
    let list: Option<Vec<f64>> =
        v.as_array().map(|a| a.iter().filter_map(Value::as_f64).filter(|x| x.is_finite()).map(|x| x.clamp(lo, hi)).collect());
    match list {
        Some(l) if !l.is_empty() && l.len() <= MAX_VALUES && v.as_array().is_some_and(|a| a.len() == l.len()) => Ok(l),
        _ => Err(bad(C, format!("`{key}` must be 1 to {MAX_VALUES} numbers (mm)"))),
    }
}

/// A value as a label writes it: 0.4, 0.25, 0.
fn short(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.into() }
}

/// One swatch centred at `at` (pt).
fn swatch(s: &mut Session, fill: bool, at: (f64, f64), density: f64, pull: f64, color: &str) -> Result<NodeId> {
    let id = if fill {
        let size = SIZE / MM_PER_PT;
        let r = s.execute("shape.rectangle", &json!({ "x": at.0 - size / 2.0, "y": at.1 - size / 2.0, "width": size, "height": size }))?;
        s.execute("paint.setFill", &json!({ "ids": [r["id"]], "color": color }))?;
        s.execute("paint.setStroke", &json!({ "ids": [r["id"]], "none": true }))?;
        let settings = json!({ "fill": { "rowSpacing": density, "pullCompensation": pull } });
        s.execute("stitch.set", &json!({ "ids": [r["id"]], "part": "fill", "type": "tatami", "settings": settings }))?;
        r["id"].clone()
    } else {
        // The ring's centre line, so the satin's outer edge is the swatch's size.
        let d = (SIZE - RING_WIDTH) / MM_PER_PT;
        let r = s.execute("shape.ellipse", &json!({ "x": at.0 - d / 2.0, "y": at.1 - d / 2.0, "width": d, "height": d }))?;
        s.execute("paint.setFill", &json!({ "ids": [r["id"]], "none": true }))?;
        s.execute("paint.setStroke", &json!({ "ids": [r["id"]], "color": color }))?;
        let settings = json!({ "satin": { "width": RING_WIDTH, "density": density, "pullCompensation": pull } });
        s.execute("stitch.set", &json!({ "ids": [r["id"]], "part": "stroke", "type": "satin", "settings": settings }))?;
        r["id"].clone()
    };
    id.as_u64().map(NodeId).ok_or_else(|| bad(C, "no swatch id"))
}

/// A label centred at `at` (pt).
fn label(s: &mut Session, text: &str, at: (f64, f64), color: &str) -> Result<NodeId> {
    let r = s.execute("stitch.createLettering", &json!({ "text": text, "height": LABEL_H, "x": at.0, "y": at.1 }))?;
    let id = r["id"].as_u64().map(NodeId).ok_or_else(|| bad(C, "no label id"))?;
    s.execute("paint.setStroke", &json!({ "ids": [id.0], "color": color }))?;
    s.execute("stitch.set", &json!({ "ids": [id.0], "part": "stroke", "type": "triple" }))?;
    if let Some(b) = s.doc()?.doc.bounds_of(&[id], false) {
        let c = b.center();
        s.execute("select.set", &json!({ "ids": [id.0] }))?;
        s.execute("object.move", &json!({ "dx": at.0 - c.x, "dy": at.1 - c.y }))?;
    }
    Ok(id)
}

/// Run `f`'s commands as one undo step (undone together when it fails), unless an undo group is
/// already open (its own step then holds them).
fn in_one_step<T>(s: &mut Session, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
    let open = s.active().is_some_and(|st| st.undo_group.is_some());
    if !open {
        s.begin_named_undo_group(Some("Test Swatches".into()));
    }
    let r = f(s);
    if !open {
        s.end_undo_group(r.is_err());
    }
    r
}

#[cfg(test)]
mod tests {
    use super::short;

    #[test]
    fn labels_write_values_short() {
        assert_eq!(short(0.4), "0.4");
        assert_eq!(short(0.25), "0.25");
        assert_eq!(short(0.0), "0");
        assert_eq!(short(-0.2), "-0.2");
        assert_eq!(short(-0.001), "0");
    }
}
