//! Roidery: a new object joins its thread colour's block. Objects sew bottom to top, so a shape
//! drawn in a colour the design already sews earlier would start that thread a second time (one
//! more colour change). Instead it is put straight after the last object of that colour, as long
//! as nothing sewn in between overlaps it (it would then end up under what it was drawn over, so
//! it stays on top and the colour sews again).
//!
//! It applies to embroidery designs (documents with a hoop) unless the design turns it off
//! (`stitch.setDesign {joinColors: false}`, the Sequencer's Keep Colors Together), and only to
//! one new object put on top of its layer or group, which is what drawing, pasting and
//! duplicating make: edits that replace or add several objects keep the order they give.

use std::collections::HashSet;

use vectorcraft_doc::{Document, Node, NodeId, NodeKind};

use super::design::DesignSettings;
use super::{all_objects, part_rgb, parts};

/// The thread colours `n` starts and ends sewing in.
fn colours(n: &Node, ds: &DesignSettings) -> Option<([u8; 3], [u8; 3])> {
    let sewn = parts::sewn(n, ds);
    let rgb = |part: Option<&(parts::Part, roidery_stitch::StitchSettings, bool)>| part.and_then(|p| part_rgb(n, p.0)).map(|c| c.0);
    Some((rgb(sewn.first())?, rgb(sewn.last())?))
}

fn object(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Path { guide: false, clipping: false, .. } | NodeKind::Compound { .. } | NodeKind::Text(_))
}

/// Called after every edit ([`crate::Session::edit`]): `before` is the document before it.
pub(crate) fn join_new(before: &Document, d: &mut Document) {
    // Cheap first: most edits add nothing.
    if d.embroidery.is_none() {
        return;
    }
    let ds = DesignSettings::of(d);
    if ds.hoop.is_none() || ds.separate_colors {
        return;
    }
    let now = all_objects(d);
    let old: HashSet<NodeId> = all_objects(before).into_iter().collect();
    if now.len() != old.len() + 1 {
        return;
    }
    let mut added = now.iter().filter(|id| !old.contains(id));
    let (Some(&id), None) = (added.next(), added.next()) else { return };
    let parent = d.parent_of(id);
    let Some(siblings) = d.children(parent) else { return };
    let Some((new, below)) = siblings.split_last() else { return };
    // On top of its layer or group, as new art is.
    if new.id != id {
        return;
    }
    let Some((start, _)) = colours(new, &ds) else { return };
    let Some(bounds) = new.visual_bounds() else { return };
    let mut target = None;
    for (i, sib) in below.iter().enumerate().rev() {
        if !sib.visible {
            continue;
        }
        if object(sib) && colours(sib, &ds).is_some_and(|(_, end)| end == start) {
            target = Some(i);
            break;
        }
        // It was drawn over this one: under it, it would be hidden and sewn over.
        if sib.visual_bounds().is_some_and(|b| b.x0 < bounds.x1 && bounds.x0 < b.x1 && b.y0 < bounds.y1 && bounds.y0 < b.y1) {
            return;
        }
    }
    let Some(i) = target else { return };
    // Already next to its colour.
    if i + 2 == siblings.len() {
        return;
    }
    let Ok(children) = d.children_mut(parent) else { return };
    if let Some(n) = children.pop() {
        let at = (i + 1).min(children.len());
        children.insert(at, n);
    }
}
