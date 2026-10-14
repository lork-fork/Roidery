//! Roidery: Convert to Satin. Each closed shape becomes Satin Blocks columns along its spine
//! (`roidery_stitch::medial`), sewn in the shape's colour just above it; the shape stays,
//! hidden, so showing it again goes back.

use roidery_stitch::medial::{self, WIDE_MM};
use roidery_stitch::{StitchSettings, StitchType};
use serde_json::{Value, json};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::Node;
use vectorcraft_geom::{PathData, Point, SubPath};

use super::{MM_PER_PT, fill_rgb, mm_polylines, objects_in, stroke_rgb};
use crate::Session;
use crate::cmd::{Result, bad, targets};

/// `stitch.convertToSatin`: the centerline way.
pub(super) fn convert(s: &mut Session, p: &Value) -> Result<Value> {
    convert_by(s, p, "stitch.convertToSatin", medial::Method::Centerline)
}

/// `stitch.convertToSatinClassic`: Roidery's first way, kept to compare.
pub(super) fn convert_classic(s: &mut Session, p: &Value) -> Result<Value> {
    convert_by(s, p, "stitch.convertToSatinClassic", medial::Method::Classic)
}

fn convert_by(s: &mut Session, p: &Value, c: &'static str, method: medial::Method) -> Result<Value> {
    let d = &s.doc()?.doc;
    let ids = objects_in(d, &targets(s, p)?);
    if ids.is_empty() {
        return Err(bad(c, "select the shapes to convert"));
    }
    // Columns for each shape, worked out before the edit.
    let mut plans = vec![];
    let mut skipped = vec![];
    let mut widest: f64 = 0.0;
    for id in ids {
        let Some(n) = d.node(id) else { continue };
        let rings: Vec<_> = mm_polylines(n).unwrap_or_default().into_iter().filter(|r| r.points.len() >= 3).collect();
        let columns = medial::to_satin_by(&rings, method);
        if columns.is_empty() {
            skipped.push(id.0);
            continue;
        }
        widest = columns.iter().map(medial::Column::widest).fold(widest, f64::max);
        let rgb = fill_rgb(n).or_else(|| stroke_rgb(n)).unwrap_or([0, 0, 0]);
        plans.push((id, columns, rgb));
    }
    if plans.is_empty() {
        return Err(bad(c, "nothing to convert: Convert to Satin works on closed shapes"));
    }
    let stitch = serde_json::to_value(StitchSettings::of(StitchType::SatinBlocks)).map_err(|e| bad(c, e.to_string()))?;
    let looks: Vec<_> = plans.iter().map(|(.., rgb)| s.new_art_look(Paint::None, Paint::solid(Color::rgb8(rgb[0], rgb[1], rgb[2])), 1.0)).collect();
    let made = s.edit("Convert to Satin", |d, sel| {
        let mut made = vec![];
        for ((id, columns, _), look) in plans.into_iter().zip(looks) {
            let Some((parent, index, _)) = d.position(id) else { continue };
            for (k, c) in columns.into_iter().enumerate() {
                let pt = |v: &[kurbo::Point]| v.iter().map(|q| Point::new(q.x / MM_PER_PT, q.y / MM_PER_PT)).collect::<Vec<_>>();
                let path = PathData::new(vec![SubPath::polyline(&pt(&c.left), false), SubPath::polyline(&pt(&c.right), false)]);
                let nid = d.alloc_id();
                let mut n = Node::new(nid, super::super::create::path_kind(path, None));
                look.apply(d, &mut n);
                n.name = Some(if k == 0 { "Satin".into() } else { format!("Satin {}", k + 1) });
                n.attrs = Some(Box::new(vectorcraft_doc::ObjectAttributes { stitch: Some(stitch.clone()), ..Default::default() }));
                // Sewn just after the shape, in order.
                d.insert(parent, index + 1 + k, n)?;
                made.push(nid);
            }
            if let Some(n) = d.node_mut(id) {
                n.visible = false;
            }
        }
        sel.set(made.iter().copied());
        Ok(made)
    })?;
    let mut out = json!({ "ids": made.iter().map(|id| id.0).collect::<Vec<_>>(), "skipped": skipped, "widestMm": widest });
    if widest > WIDE_MM {
        out["warning"] = json!(format!(
            "Converted to satin, but part of it is {widest:.1} mm wide: satin over {WIDE_MM:.0} mm sews loose stitches. Split it, or keep a fill there."
        ));
    }
    Ok(out)
}
