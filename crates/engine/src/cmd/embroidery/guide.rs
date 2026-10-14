//! Object › Embroidery › Use as Fill Guide (`stitch.setGuide`): a line drawn across fills
//! becomes the curve their rows follow (guided fill). The line is stored with each fill, scaled
//! to the fill's bounds so it moves and scales with it, and taken out of the artwork.

use roidery_stitch::guided::guide_points;
use roidery_stitch::{GuidedSettings, StitchType};

use super::*;

/// The selected open path to use as a guide, when there is exactly one (lettering aside).
pub(super) fn open_path(d: &Document, ids: &[NodeId]) -> Option<NodeId> {
    let open: Vec<NodeId> = ids
        .iter()
        .copied()
        .filter(|id| {
            d.node(*id).is_some_and(|n| {
                // Lettering is drawn as open strokes, but isn't a line to follow.
                let lettering = matches!(&super::parts::slots(n).stroke, super::parts::Slot::Set(st) if st.lettering.is_some());
                !lettering
                    && matches!(&n.kind, NodeKind::Path { path, guide: false, clipping: false, .. }
                    if !path.subpaths.is_empty() && path.subpaths.iter().all(|sp| !sp.closed && sp.anchors.len() >= 2))
            })
        })
        .collect();
    match open.as_slice() {
        [one] => Some(*one),
        _ => None,
    }
}

pub(super) fn set_guide(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.setGuide";
    let sel = targets(s, p)?;
    let keep = p.get("keep").and_then(Value::as_bool).unwrap_or(false);
    let d = &s.doc()?.doc;
    let guide_id = match p.get("guide").and_then(Value::as_u64) {
        Some(g) => NodeId(g),
        None => open_path(d, &sel).ok_or_else(|| bad(C, "select the fills and one open path to guide their rows"))?,
    };
    let guide_node = d.node(guide_id).ok_or(EngineError::NoNode(guide_id))?;
    let line: Vec<SPoint> = mm_polylines(guide_node).unwrap_or_default().into_iter().next().map(|p| p.points).unwrap_or_default();
    if line.len() < 2 {
        return Err(bad(C, "the guide must be a line with at least two points"));
    }
    let ds = DesignSettings::of(d);
    let mut todo = vec![];
    for id in objects_in(d, &sel).into_iter().filter(|id| *id != guide_id) {
        let Some(n) = d.node(id) else { continue };
        if !super::parts::sewn(n, &ds).iter().any(|(part, ..)| *part == super::Part::Fill) {
            continue;
        }
        let (mut st, _) = super::parts::part_settings(n, &ds, super::Part::Fill);
        let Some(rings) = mm_polylines(n) else { continue };
        // The guide as fractions of this fill's bounds.
        let pts: Vec<SPoint> = rings.iter().flat_map(|r| r.points.iter()).copied().filter(|q| q.x.is_finite() && q.y.is_finite()).collect();
        let Some(first) = pts.first() else { continue };
        let (mut lo, mut hi) = (*first, *first);
        for q in &pts {
            (lo, hi) = (SPoint::new(lo.x.min(q.x), lo.y.min(q.y)), SPoint::new(hi.x.max(q.x), hi.y.max(q.y)));
        }
        let frac = |v: f64, a: f64, b: f64| if b - a > 1e-9 { (v - a) / (b - a) } else { 0.5 };
        let guide = GuidedSettings { guide: line.iter().map(|q| [frac(q.x, lo.x, hi.x), frac(q.y, lo.y, hi.y)]).collect() };
        if guide_points(&rings, &guide).len() < 2 {
            continue;
        }
        st.kind = StitchType::Guided;
        st.guided = guide;
        let mut slots = super::parts::slots(n);
        slots.fill = super::parts::Slot::Set(st);
        todo.push((id, slots));
    }
    if todo.is_empty() {
        return Err(bad(C, "select the fills the guide should shape, with the guide"));
    }
    let fills: Vec<NodeId> = todo.iter().map(|(id, _)| *id).collect();
    let changed = s.edit("Use as Fill Guide", |d, sel| {
        let mut count = 0;
        for (id, slots) in &todo {
            if let Some(n) = d.node_mut(*id) {
                super::parts::write(n, slots, false);
                count += 1;
            }
        }
        if !keep {
            d.remove(guide_id)?;
            sel.set(fills.clone());
        }
        Ok(count)
    })?;
    Ok(json!({ "changed": changed, "guideRemoved": !keep }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn an_open_path_guides_the_selected_fills_and_goes() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
        let sq = s.execute("shape.rectangle", &json!({"x": 50, "y": 50, "width": 100, "height": 100})).unwrap()["id"].as_u64().unwrap();
        let line = s.execute("path.create", &json!({"d": "M 40 140 Q 100 40 160 140"})).unwrap()["id"].as_u64().unwrap();
        // Nothing open selected: an error, nothing changes.
        s.execute("select.set", &json!({"ids": [sq]})).unwrap();
        assert!(s.execute("stitch.setGuide", &json!({})).is_err());
        s.execute("select.set", &json!({"ids": [sq, line]})).unwrap();
        let r = s.execute("stitch.setGuide", &json!({})).unwrap();
        assert_eq!(r["changed"], 1);
        assert!(s.doc().unwrap().doc.node(NodeId(line)).is_none());
        let info = s.execute("stitch.info", &json!({"ids": [sq]})).unwrap();
        assert_eq!(info["objects"][0]["type"], "guided");
        let guide = info["objects"][0]["settings"]["guided"]["guide"].as_array().unwrap().clone();
        assert!(guide.len() > 4);
        // The line ran from just left of the square's bottom-left to just right of its
        // bottom-right: x a little outside 0..1, y at the bottom.
        assert!(guide[0][0].as_f64().unwrap() < 0.0 && guide[0][1].as_f64().unwrap() > 0.85);
        assert!(info["objects"][0]["stitches"].as_u64().unwrap() > 1000);
        // One undo step brings the line back.
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(s.doc().unwrap().doc.node(NodeId(line)).is_some());
    }
}
