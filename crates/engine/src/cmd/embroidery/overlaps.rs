//! Object › Embroidery › Remove Overlaps (`stitch.removeOverlaps`): cut each fill where fills
//! sewn after it cover it, so thread isn't sewn where nobody will see it. The fill still reaches
//! `overlap` mm under the covering shapes' edges, so no fabric shows between them when the thread
//! pulls in. A fill covered completely is left as it is. Text covers what is under it but isn't
//! cut itself, so it stays editable.

use vectorcraft_geom::{FillRule, PathData};
use vectorcraft_pathops::{BoolOp, Join, area, boolean, offset_path, unite_all};

use super::*;

/// Default mm a fill keeps running under the shapes that cover it.
pub const DEFAULT_OVERLAP: f64 = 1.0;
/// Cuts that take away less than this fraction of a fill's area are skipped.
const MIN_CUT: f64 = 1e-4;

/// The area an object fills, as one path and its fill rule.
fn filled_area(n: &Node) -> Option<(PathData, FillRule)> {
    match &n.kind {
        NodeKind::Path { path, rule, .. } => Some((path.clone(), *rule)),
        NodeKind::Compound { children, rule } => {
            // The members painted as one path, under the compound path's rule.
            let mut all = PathData::default();
            for p in children.iter().filter_map(|c| c.path_data()) {
                all.subpaths.extend(p.subpaths.iter().cloned());
            }
            (!all.is_empty()).then_some((all, *rule))
        }
        NodeKind::Text(t) => Some((super::text_outline(t), FillRule::NonZero)),
        _ => None,
    }
}

pub(super) fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.removeOverlaps";
    let overlap = match p.get("overlap") {
        None | Some(Value::Null) => DEFAULT_OVERLAP,
        Some(v) => v.as_f64().filter(|o| (0.0..=20.0).contains(o)).ok_or_else(|| bad(C, "`overlap` must be 0 to 20 mm"))?,
    };
    let sel = targets(s, p)?;
    let d = &s.doc()?.doc;
    let ds = DesignSettings::of(d);
    let order = all_objects(d);
    let cut: BTreeSet<NodeId> = if sel.is_empty() { order.iter().copied().collect() } else { objects_in(d, &sel).into_iter().collect() };
    // Each fill in sewing order, with its area.
    let fills: Vec<(NodeId, PathData, FillRule)> = order
        .iter()
        .filter_map(|id| {
            let n = d.node(*id)?;
            if !super::parts::sewn(n, &ds).iter().any(|(part, ..)| *part == super::Part::Fill) {
                return None;
            }
            let (path, rule) = filled_area(n)?;
            Some((*id, path, rule))
        })
        .collect();
    // Work out every cut first, so nothing changes if one fails.
    let mut todo: Vec<(NodeId, PathData)> = vec![];
    for (i, (id, path, rule)) in fills.iter().enumerate() {
        if !cut.contains(id) || d.node(*id).is_some_and(|n| matches!(n.kind, NodeKind::Text(_))) {
            continue;
        }
        let above: Vec<(&PathData, FillRule)> = fills.iter().skip(i + 1).map(|(_, p, r)| (p, *r)).collect();
        if above.is_empty() {
            continue;
        }
        let cover = unite_all(&above);
        if cover.is_empty() {
            continue;
        }
        let before = area(path, *rule);
        // What still shows, grown by the overlap and kept inside the fill: it reaches under the
        // covering shapes' edges, but not past the fill's own.
        let visible = boolean(path, *rule, &cover, FillRule::NonZero, BoolOp::Difference);
        if visible.is_empty() {
            // Covered completely: left as it is (deleting is the user's call).
            continue;
        }
        let after_path = if overlap > 0.0 {
            let grown = offset_path(&visible, overlap / MM_PER_PT, Join::Round, 4.0);
            boolean(path, *rule, &grown, FillRule::NonZero, BoolOp::Intersect)
        } else {
            visible
        };
        let after = area(&after_path, FillRule::NonZero);
        if before - after > before * MIN_CUT && !after_path.is_empty() {
            todo.push((*id, after_path));
        }
    }
    let changed = edit_counted(s, "Remove Overlaps", |d| {
        let mut count = 0;
        for (id, new) in &todo {
            let Some(n) = d.node_mut(*id) else { continue };
            match &mut n.kind {
                NodeKind::Path { path, rule, live, .. } => {
                    *path = new.clone();
                    *rule = FillRule::NonZero;
                    *live = None;
                }
                NodeKind::Compound { .. } => {
                    // The cut area replaces the compound path's members.
                    let mut one = n.clone();
                    one.kind = NodeKind::Path { path: new.clone(), rule: FillRule::NonZero, live: None, clipping: false, guide: false };
                    *n = one;
                }
                _ => continue,
            }
            count += 1;
        }
        Ok(count)
    })?;
    Ok(json!({ "changed": changed }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
        s
    }

    fn rect(s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        NodeId(s.execute("shape.rectangle", &json!({"x": x, "y": y, "width": w, "height": h})).unwrap()["id"].as_u64().unwrap())
    }

    fn area_of(s: &Session, id: NodeId) -> f64 {
        let n = s.doc().unwrap().doc.node(id).unwrap();
        let (p, r) = filled_area(n).unwrap();
        area(&p, r)
    }

    #[test]
    fn fills_lose_what_later_fills_cover_less_the_overlap() {
        let mut s = session();
        let under = rect(&mut s, 0.0, 0.0, 100.0, 100.0);
        let over = rect(&mut s, 50.0, 0.0, 100.0, 100.0);
        s.execute("select.none", &json!({})).unwrap();
        let r = s.execute("stitch.removeOverlaps", &json!({"overlap": 0})).unwrap();
        assert_eq!(r["changed"], 1);
        // The left half stays; the one on top is untouched.
        assert!((area_of(&s, under) - 5000.0).abs() < 1.0, "{}", area_of(&s, under));
        assert!((area_of(&s, over) - 10000.0).abs() < 1.0);
        s.execute("edit.undo", &json!({})).unwrap();
        assert!((area_of(&s, under) - 10000.0).abs() < 1.0);
        // With 1 mm of overlap (2.83 pt) the fill reaches that far under the shape on top.
        s.execute("stitch.removeOverlaps", &json!({})).unwrap();
        let a = area_of(&s, under);
        assert!((a - (50.0 + 1.0 / MM_PER_PT) * 100.0).abs() < 2.0, "{a}");
        // Again: nothing more to cut.
        assert_eq!(s.execute("stitch.removeOverlaps", &json!({})).unwrap()["changed"], 0);
    }

    #[test]
    fn only_fills_cut_and_only_what_is_asked() {
        let mut s = session();
        let a = rect(&mut s, 0.0, 0.0, 100.0, 100.0);
        let b = rect(&mut s, 20.0, 20.0, 100.0, 100.0);
        let line = rect(&mut s, 0.0, 0.0, 100.0, 100.0);
        s.execute("stitch.set", &json!({"ids": [line.0], "type": "run"})).unwrap();
        // Only b asked: it has nothing filled above it (the run line doesn't cover).
        assert_eq!(s.execute("stitch.removeOverlaps", &json!({"ids": [b.0], "overlap": 0})).unwrap()["changed"], 0);
        assert_eq!(s.execute("stitch.removeOverlaps", &json!({"ids": [a.0], "overlap": 0})).unwrap()["changed"], 1);
        assert!((area_of(&s, a) - (10000.0 - 80.0 * 80.0)).abs() < 1.0);
        assert!(s.execute("stitch.removeOverlaps", &json!({"overlap": -1})).is_err());
    }

    #[test]
    fn text_on_top_cuts_the_fill_under_it_and_stays_text() {
        let mut s = session();
        let under = rect(&mut s, 0.0, 0.0, 200.0, 120.0);
        let t = s.execute("text.create", &json!({"x": 10, "y": 90, "text": "HI", "size": 96})).unwrap()["id"].as_u64().unwrap();
        s.execute("select.none", &json!({})).unwrap();
        assert_eq!(s.execute("stitch.removeOverlaps", &json!({"overlap": 0})).unwrap()["changed"], 1);
        assert!(area_of(&s, under) < 24000.0 - 1000.0, "{}", area_of(&s, under));
        assert!(matches!(s.doc().unwrap().doc.node(NodeId(t)).unwrap().kind, NodeKind::Text(_)));
    }
}
