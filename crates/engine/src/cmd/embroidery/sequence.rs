//! Roidery: the sewing sequence behind the Sequencer panel. Objects sew in paint order (bottom to
//! top), so changing the sequence moves objects in the layer stack.

use std::collections::HashMap;

use roidery_plan::PlanSettings;
use serde_json::{Value, json};
use vectorcraft_doc::{Document, NodeId, NodeKind};

use super::{all_objects, sew};
use crate::Session;
use crate::cmd::{EngineError, Result, bad, id_param};

/// One row of the sequence: an object's sewn part (no part: it sews nothing), its settings,
/// whether they're automatic, and its stitches.
type Step<'a> = (NodeId, Option<super::Part>, roidery_stitch::StitchSettings, bool, Option<&'a roidery_plan::PlanObject>);

/// Points of a row's thumbnail at most.
const THUMB_POINTS: usize = 240;

/// Every embroidery object in sewing order, the hidden ones too (but not those inside a hidden
/// layer or group, which the Sequencer shows as that container hidden).
fn listed_objects(d: &Document) -> Vec<NodeId> {
    fn walk(n: &vectorcraft_doc::Node, out: &mut Vec<NodeId>) {
        match &n.kind {
            NodeKind::Path { guide: false, clipping: false, .. } | NodeKind::Compound { .. } | NodeKind::Text(_) => out.push(n.id),
            NodeKind::Path { .. } => {}
            _ if !n.visible => {}
            _ => n.children().into_iter().flatten().for_each(|c| walk(c, out)),
        }
    }
    let mut out = vec![];
    d.layers.iter().for_each(|l| walk(l, &mut out));
    out
}

/// A row's stitches for its thumbnail: every k-th, mm.
fn thumb(o: &roidery_plan::PlanObject) -> Value {
    let n = o.stitches.len();
    let k = n.div_ceil(THUMB_POINTS).max(1);
    let pts: Vec<[f64; 2]> = o.stitches.iter().step_by(k).map(|p| [(p.x * 10.0).round() / 10.0, (p.y * 10.0).round() / 10.0]).collect();
    json!(pts)
}

/// `stitch.sequence`: every object in sewing order, and the colour blocks they make.
pub(super) fn sequence(s: &mut Session, p: &Value) -> Result<Value> {
    let thumbs = p.get("thumbs").and_then(Value::as_bool).unwrap_or(false);
    let d = &s.doc()?.doc;
    let ids = all_objects(d);
    let listed = listed_objects(d);
    let (sewn, design) = sew(d, &ids, &PlanSettings::default(), "");
    let ds = super::DesignSettings::of(d);
    // Each object's sewn parts, or one row saying it sews nothing.
    let mut steps: Vec<Step> = vec![];
    for id in &listed {
        let mine: Vec<&super::Sewn> = sewn.iter().filter(|o| o.id == *id).collect();
        if mine.is_empty() {
            let Some(n) = d.node(*id) else { continue };
            let (st, auto) = super::settings_of(n, &ds);
            steps.push((*id, None, st, auto, None));
        }
        steps.extend(mine.into_iter().map(|o| (o.id, Some(o.part), o.settings.clone(), o.auto, Some(&o.obj))));
    }
    let mut rows = vec![];
    // Colour blocks: runs of objects that sew in the same thread (objects without stitches
    // don't break a run).
    let mut colors: Vec<(roidery_formats::Thread, Vec<u64>, usize)> = vec![];
    for (id, part, st, auto, o) in &steps {
        // Unnamed text goes by its words.
        let name = d
            .node(*id)
            .and_then(|n| match (&n.name, &n.kind) {
                (Some(name), _) => Some(name.clone()),
                (None, NodeKind::Text(t)) => Some(t.plain_text().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(40).collect()),
                _ => None,
            })
            .unwrap_or_default();
        let n = o.map_or(0, |o| o.stitches.len());
        let thread = o.map(|o| o.thread.clone()).unwrap_or_default();
        let rgb = thread.rgb;
        let block = if n == 0 {
            None
        } else {
            match colors.last_mut() {
                Some((c, ids, count)) if c.rgb == rgb => {
                    if ids.last() != Some(&id.0) {
                        ids.push(id.0);
                    }
                    *count += n;
                }
                _ => colors.push((thread, vec![id.0], n)),
            }
            Some(colors.len() - 1)
        };
        let part = part.map(super::Part::id);
        let node = d.node(*id);
        // The layers and groups it is in, outermost first.
        let parents: Vec<u64> = d.ancestry(*id).map(|a| a[..a.len().saturating_sub(1)].iter().map(|p| p.0).collect()).unwrap_or_default();
        let mut row = json!({
            "id": id, "name": name, "part": part, "type": st.kind.id(), "auto": auto, "stitches": n, "thread": rgb, "block": block,
            "visible": node.is_some_and(|n| n.visible), "locked": node.is_some_and(|n| n.locked), "parents": parents,
        });
        if thumbs && let Some(o) = o {
            row["thumb"] = thumb(o);
        }
        rows.push(row);
    }
    // The layers and groups the rows are in.
    let mut groups = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for id in &listed {
        let Some(chain) = d.ancestry(*id) else { continue };
        for g in &chain[..chain.len().saturating_sub(1)] {
            if !seen.insert(*g) {
                continue;
            }
            let Some(n) = d.node(*g) else { continue };
            let kind = if matches!(n.kind, NodeKind::Layer { .. }) { "layer" } else { "group" };
            groups.push(json!({ "id": g, "name": n.name.clone().unwrap_or_default(), "kind": kind, "visible": n.visible, "locked": n.locked }));
        }
    }
    let colors: Vec<Value> = colors
        .into_iter()
        .map(|(t, ids, n)| json!({ "thread": t.rgb, "threadName": t.name, "threadCode": t.code, "objects": ids, "stitches": n }))
        .collect();
    Ok(json!({ "objects": rows, "groups": groups, "colors": colors, "stitches": design.stitch_count(), "colorChanges": design.color_changes() }))
}

/// `stitch.partOrder {ids?, first: "fill" | "stroke"}`: which of each object's fill and stroke
/// sews first.
pub(super) fn part_order(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.partOrder";
    let stroke_first = match p.get("first").and_then(Value::as_str) {
        Some("fill") => false,
        Some("stroke") => true,
        _ => return Err(bad(C, "`first` must be \"fill\" or \"stroke\"")),
    };
    let d = &s.doc()?.doc;
    let ids: Vec<NodeId> = super::objects_in(d, &crate::cmd::targets(s, p)?)
        .into_iter()
        .filter(|id| d.node(*id).is_some_and(|n| super::parts::slots(n).stroke_first != stroke_first))
        .collect();
    let changed = crate::cmd::overprint::edit_counted(s, "Sewing Order", |d| {
        let mut count = 0;
        for id in &ids {
            let Some(n) = d.node_mut(*id) else { continue };
            let mut slots = super::parts::slots(n);
            slots.stroke_first = stroke_first;
            let v = slots.to_value();
            n.edit_attrs(|a| a.stitch.clone_from(&v));
            count += 1;
        }
        Ok(count)
    })?;
    Ok(json!({ "changed": changed }))
}

/// The ids in `p[key]` that are embroidery objects of `d`, in sewing order.
fn objects_param(d: &Document, p: &Value, key: &str, c: &'static str) -> Result<Vec<NodeId>> {
    let want: Vec<NodeId> = p
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| bad(c, format!("missing `{key}`: an array of object ids")))?
        .iter()
        .map(|v| v.as_u64().map(NodeId).ok_or_else(|| bad(c, format!("`{key}` holds a non-id"))))
        .collect::<Result<_>>()?;
    let all = listed_objects(d);
    if let Some(bad_id) = want.iter().find(|id| !all.contains(id)) {
        return Err(bad(c, format!("{} is not an embroidery object", bad_id.0)));
    }
    Ok(all.into_iter().filter(|id| want.contains(id)).collect())
}

/// Take `id` out of the tree and put it at `index` of `parent`'s children (clamped).
fn reinsert(d: &mut Document, id: NodeId, parent: Option<NodeId>, place: impl Fn(&Document) -> Option<usize>) -> Result<()> {
    let n = d.remove(id)?;
    let i = place(d).ok_or_else(|| EngineError::Other("the sequence changed while moving".into()))?;
    let ch = d.children_mut(parent)?;
    let i = i.min(ch.len());
    ch.insert(i, n);
    Ok(())
}

fn index_in(d: &Document, parent: Option<NodeId>, id: NodeId) -> Option<usize> {
    d.children(parent)?.iter().position(|c| c.id == id)
}

/// `stitch.move {ids, before? | after?}`: sew `ids` (kept in their order) just before or after
/// another object, by moving them next to it in the layer stack.
pub(super) fn move_objects(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.move";
    let d = &s.doc()?.doc;
    let ids = objects_param(d, p, "ids", C)?;
    let (target, after) = match (id_param(p, "before"), id_param(p, "after")) {
        (Some(t), None) => (t, false),
        (None, Some(t)) => (t, true),
        _ => return Err(bad(C, "give exactly one of `before` and `after`")),
    };
    if !listed_objects(d).contains(&target) {
        return Err(bad(C, format!("{} is not an embroidery object", target.0)));
    }
    let ids: Vec<NodeId> = ids.into_iter().filter(|id| *id != target).collect();
    if ids.is_empty() {
        return Ok(json!({ "moved": 0 }));
    }
    let parent = d.parent_of(target);
    let moved = ids.len();
    s.edit("Reorder Sequence", |d, _| {
        let mut anchor = target;
        for id in &ids {
            if after {
                let a = anchor;
                reinsert(d, *id, parent, |d| index_in(d, parent, a).map(|i| i + 1))?;
                anchor = *id;
            } else {
                reinsert(d, *id, parent, |d| index_in(d, parent, target))?;
            }
        }
        Ok(())
    })?;
    Ok(json!({ "moved": moved }))
}

/// `stitch.sortColors`: sew each thread colour in one go where the layer stack allows it. Within
/// each layer or group, its objects are reordered by colour (colours in the order they first
/// sew, objects of one colour in their old order); objects in different layers or groups stay
/// where they are.
pub(super) fn sort_colors(s: &mut Session, _p: &Value) -> Result<Value> {
    let before = sequence(s, &json!({}))?["colorChanges"].clone();
    let d = &s.doc()?.doc;
    let (objs, _) = sew(d, &all_objects(d), &PlanSettings::default(), "");
    let mut rank: HashMap<[u8; 3], usize> = HashMap::new();
    for o in &objs {
        let next = rank.len();
        rank.entry(o.obj.thread.rgb).or_insert(next);
    }
    // An object sorts by the colour it sews first.
    let mut thread: HashMap<NodeId, [u8; 3]> = HashMap::new();
    for o in &objs {
        thread.entry(o.id).or_insert(o.obj.thread.rgb);
    }
    // Each parent's object slots, and the objects sorted into them.
    let mut by_parent: Vec<(Option<NodeId>, Vec<NodeId>)> = vec![];
    let mut seen = std::collections::HashSet::new();
    for id in objs.iter().map(|o| o.id).filter(|id| seen.insert(*id)) {
        let id = &id;
        let parent = d.parent_of(*id);
        match by_parent.iter_mut().find(|(p, _)| *p == parent) {
            Some((_, v)) => v.push(*id),
            None => by_parent.push((parent, vec![*id])),
        }
    }
    let mut plan = vec![];
    for (parent, ids) in by_parent {
        let mut sorted = ids.clone();
        sorted.sort_by_key(|id| thread.get(id).and_then(|t| rank.get(t)).copied().unwrap_or(usize::MAX));
        if sorted != ids {
            plan.push((parent, ids, sorted));
        }
    }
    if plan.is_empty() {
        return Ok(json!({ "colorChanges": before, "changed": false }));
    }
    s.edit("Sort by Color", |d, _| {
        for (parent, old, sorted) in &plan {
            let ch = d.children_mut(*parent)?;
            // The slots the objects held, lowest first, refilled in sorted order.
            let mut slots: Vec<usize> = old.iter().filter_map(|id| ch.iter().position(|c| c.id == *id)).collect();
            slots.sort_unstable();
            let nodes: Vec<_> = sorted.iter().filter_map(|id| ch.iter().find(|c| c.id == *id).cloned()).collect();
            if nodes.len() != slots.len() {
                return Err(EngineError::Other("the sequence changed while sorting".into()));
            }
            for (slot, n) in slots.into_iter().zip(nodes) {
                if let Some(c) = ch.get_mut(slot) {
                    *c = n;
                }
            }
        }
        Ok(())
    })?;
    let after = sequence(s, &json!({}))?;
    Ok(json!({ "colorChanges": after["colorChanges"], "changed": true }))
}
