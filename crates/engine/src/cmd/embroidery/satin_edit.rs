//! Roidery: editing Satin Blocks after they are drawn, as Ember allows: move a rung's ends, insert
//! a rung between two others, delete one, and swap the rails (which side each stitch starts
//! from). Reversing the sewing direction is `path.reverse`, which flips both rails.

use serde_json::{Value, json};
use vectorcraft_doc::{Document, NodeId, NodeKind};
use vectorcraft_geom::{Anchor, PathData, Point};

use super::{objects_in, settings_of};
use crate::Session;
use crate::cmd::overprint::edit_counted;
use crate::cmd::{Result, bad, point_param, targets};

/// Accuracy of arc lengths along a curved rail, pt.
const ARCLEN_ACCURACY: f64 = 1e-3;

/// Whether `id` is a Satin Blocks object with its two rails.
fn is_satin_blocks(d: &Document, id: NodeId) -> bool {
    d.node(id).is_some_and(|n| {
        settings_of(n, &super::DesignSettings::of(d)).0.kind == roidery_stitch::StitchType::SatinBlocks
            && matches!(&n.kind, NodeKind::Path { path, .. } if path.subpaths.len() >= 2)
    })
}

/// The Satin Blocks object `p.id` and its index `p.index` (a rung).
fn rung_target(s: &Session, p: &Value, c: &'static str) -> Result<(NodeId, usize)> {
    let id = p.get("id").and_then(Value::as_u64).map(NodeId).ok_or_else(|| bad(c, "missing `id`: a Satin Blocks object"))?;
    let d = &s.doc()?.doc;
    if !is_satin_blocks(d, id) {
        return Err(bad(c, format!("{} is not a Satin Blocks object", id.0)));
    }
    let index = p.get("index").and_then(Value::as_u64).and_then(|i| usize::try_from(i).ok()).ok_or_else(|| bad(c, "missing `index`: a rung"))?;
    Ok((id, index))
}

/// Edit the rails of Satin Blocks object `id` in one undo step.
fn edit_rails(s: &mut Session, label: &str, id: NodeId, f: impl FnOnce(&mut PathData) -> Result<()>) -> Result<usize> {
    edit_counted(s, label, |d| {
        let Some(n) = d.node_mut(id) else { return Ok(0) };
        let NodeKind::Path { path, live, .. } = &mut n.kind else { return Ok(0) };
        *live = None;
        f(path)?;
        Ok(1)
    })
}

/// Rungs of a Satin Blocks path: the shorter rail's anchor count.
fn rung_count(path: &PathData) -> usize {
    match path.subpaths.as_slice() {
        [l, r, ..] => l.anchors.len().min(r.anchors.len()),
        _ => 0,
    }
}

/// `stitch.moveRung`.
pub(super) fn move_rung(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.moveRung";
    let (id, index) = rung_target(s, p, C)?;
    let ends = [point_param(p, "left"), point_param(p, "right")];
    if ends.iter().all(Option::is_none) {
        return Err(bad(C, "give `left` and/or `right`: [x, y]"));
    }
    if ends.iter().flatten().any(|q| !q.x.is_finite() || !q.y.is_finite()) {
        return Err(bad(C, "rung ends must be finite"));
    }
    let changed = edit_rails(s, "Move Rung", id, |path| {
        if index >= rung_count(path) {
            return Err(bad(C, format!("no rung {index}")));
        }
        for (sp, end) in path.subpaths.iter_mut().zip(ends) {
            // The end's curve handles move with it.
            if let (Some(q), Some(a)) = (end, sp.anchors.get_mut(index)) {
                let d = q - a.p;
                a.translate(d);
            }
        }
        Ok(())
    })?;
    Ok(json!({ "changed": changed }))
}

/// `stitch.insertRung`.
pub(super) fn insert_rung(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.insertRung";
    let (id, index) = rung_target(s, p, C)?;
    let t = p.get("t").and_then(Value::as_f64).unwrap_or(0.5);
    if !(0.0..=1.0).contains(&t) {
        return Err(bad(C, "`t` must be 0 to 1"));
    }
    let changed = edit_rails(s, "Insert Rung", id, |path| {
        if index + 1 >= rung_count(path) {
            return Err(bad(C, format!("no rungs {index} and {} to insert between", index + 1)));
        }
        for sp in path.subpaths.iter_mut().take(2) {
            let (Some(a), Some(b)) = (sp.anchors.get(index), sp.anchors.get(index + 1)) else { continue };
            if sp.segment_is_line(index) {
                let q: Point = a.p.lerp(b.p, t);
                sp.anchors.insert(index + 1, Anchor::corner(q));
            } else {
                // `t` of the way along the curve's length; the curve keeps its shape.
                use kurbo::ParamCurveArclen;
                let c = sp.segment(index);
                let at = c.inv_arclen(t * c.arclen(ARCLEN_ACCURACY), ARCLEN_ACCURACY);
                sp.insert_anchor(index, at);
            }
        }
        Ok(())
    })?;
    Ok(json!({ "changed": changed, "index": index + 1 }))
}

/// `stitch.deleteRung`.
pub(super) fn delete_rung(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.deleteRung";
    let (id, index) = rung_target(s, p, C)?;
    let changed = edit_rails(s, "Delete Rung", id, |path| {
        let n = rung_count(path);
        if index >= n {
            return Err(bad(C, format!("no rung {index}")));
        }
        if n <= 2 {
            return Err(bad(C, "a Satin Blocks object keeps at least two rungs"));
        }
        for sp in path.subpaths.iter_mut().take(2) {
            if index < sp.anchors.len() {
                sp.anchors.remove(index);
            }
        }
        Ok(())
    })?;
    Ok(json!({ "changed": changed }))
}

/// `stitch.swapSides`.
pub(super) fn swap_sides(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.swapSides";
    let d = &s.doc()?.doc;
    let ids: Vec<NodeId> = objects_in(d, &targets(s, p)?).into_iter().filter(|id| is_satin_blocks(d, *id)).collect();
    if ids.is_empty() {
        return Err(bad(C, "select Satin Blocks objects"));
    }
    let changed = edit_counted(s, "Swap Satin Sides", |d| {
        let mut count = 0;
        for id in &ids {
            if let Some(n) = d.node_mut(*id)
                && let NodeKind::Path { path, .. } = &mut n.kind
                && path.subpaths.len() >= 2
            {
                path.subpaths.swap(0, 1);
                count += 1;
            }
        }
        Ok(count)
    })?;
    Ok(json!({ "changed": changed }))
}
