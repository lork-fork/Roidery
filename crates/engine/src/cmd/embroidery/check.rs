//! Roidery: Check Design (`stitch.check`). Finds what will sew badly before it is sewn, with
//! where it is and, when one command fixes it, that command:
//! - the design past the hoop's edge, or too large for the hoop;
//! - spots where objects stack and thread piles up (the density map), which break needles and
//!   pucker fabric;
//! - fills too thin to fill (a sliver sews as a few ragged rows);
//! - satins too wide (long loose stitches snag) or too narrow (the stitches bunch up);
//! - objects too small to sew cleanly.
//!
//! Locations are document points (the UI marks and zooms to them); sizes are mm.

use std::collections::{BTreeSet, HashMap};

use roidery_formats::Command;
use roidery_stitch::{Polyline, SatinSettings, StitchKind, StitchType};
use serde_json::{Value, json};
use vectorcraft_doc::{Document, NodeId};

use super::{HoopField, MM_PER_PT, Part, PlanSettings, Sewn, all_objects, mm_polylines, objects_in, plan, plan_settings, review, sewn_parts};
use crate::cmd::*;

/// A fill narrower than this on average (mm) can't lay rows across it.
pub const THIN_FILL_MM: f64 = 1.5;
/// Satin stitches longer than this (mm) lie loose and snag.
pub const WIDE_SATIN_MM: f64 = 10.0;
/// Satin narrower than this (mm) bunches into a knot.
pub const NARROW_SATIN_MM: f64 = 1.0;
/// Objects whose stitches fit in a square this size (mm) are too small to sew cleanly.
pub const TINY_MM: f64 = 1.5;
/// Stacked thread past this many layers is an error (needles break), below it a warning.
pub const HEAVY_LAYERS: f64 = 4.0;
/// At most this many dense spots are listed, the worst first.
const MAX_SPOTS: usize = 20;

/// One problem found.
struct Problem {
    kind: &'static str,
    error: bool,
    ids: Vec<NodeId>,
    /// Where, mm (document coordinates).
    at: Option<(f64, f64)>,
    /// How far round `at` it reaches, mm.
    radius: f64,
    /// The measurement the message names (mm or layers).
    value: f64,
    fix: Option<Value>,
}

impl Problem {
    fn json(&self) -> Value {
        let mut v = json!({
            "kind": self.kind, "level": if self.error { "error" } else { "warning" }, "ids": self.ids,
            "value": (self.value * 10.0).round() / 10.0,
        });
        if let Some((x, y)) = self.at {
            v["x"] = json!(x / MM_PER_PT);
            v["y"] = json!(y / MM_PER_PT);
            v["radius"] = json!(self.radius.max(1.0) / MM_PER_PT);
        }
        if let Some(f) = &self.fix {
            v["fix"] = f.clone();
        }
        v
    }
}

/// `stitch.check`.
pub(super) fn check(s: &mut Session, p: &Value) -> Result<Value> {
    let d = &s.doc()?.doc;
    let sel = targets(s, p)?;
    let ids = if sel.is_empty() { all_objects(d) } else { objects_in(d, &sel) };
    let parts = sewn_parts(d, &ids);
    let mut out = hoop_problems(d, &parts);
    out.extend(dense_spots(&parts));
    for part in &parts {
        out.extend(part_problems(d, part));
    }
    // Errors first, then in sewing order.
    out.sort_by_key(|p| !p.error);
    let errors = out.iter().filter(|p| p.error).count();
    Ok(json!({ "problems": out.iter().map(Problem::json).collect::<Vec<_>>(), "errors": errors, "warnings": out.len() - errors }))
}

/// Where the design sits in the hoop: past its edge (Center in Hoop fixes it when the design is
/// small enough) or too large for it.
fn hoop_problems(d: &Document, parts: &[Sewn]) -> Vec<Problem> {
    let Some(field) = HoopField::of(d) else { return vec![] };
    let objs: Vec<_> = parts.iter().map(|o| o.obj.clone()).collect();
    let design = plan(&objs, &plan_settings(d), "");
    let b = design.bounds();
    let (w, h) = b.map_or((0.0, 0.0), |b| (b.width(), b.height()));
    let fit = field.fit_json(&design, w, h);
    let (ox, oy) = field.origin_mm();
    let (hw, hh) = (field.hoop.width / 2.0, field.hoop.height / 2.0);
    // The stitch furthest past the edge.
    let worst = design
        .stitches
        .iter()
        .filter(|s| s.command == Command::Stitch)
        .map(|s| (if field.hoop.round { s.x.hypot(s.y) - hw } else { (s.x.abs() - hw).max(s.y.abs() - hh) }, s.x, s.y))
        .fold(None, |m: Option<(f64, f64, f64)>, v| if m.is_none_or(|m| v.0 > m.0) { Some(v) } else { m });
    let at = worst.map(|(_, x, y)| (x + ox, y + oy));
    let all: Vec<NodeId> = parts.iter().map(|o| o.id).collect::<BTreeSet<_>>().into_iter().collect();
    match fit["fit"].as_str() {
        Some("outside") => vec![Problem {
            kind: "outsideHoop",
            error: true,
            ids: vec![],
            at,
            radius: 4.0,
            value: fit["outsideMm"].as_f64().unwrap_or(0.0),
            fix: Some(json!({ "command": "stitch.centerInHoop", "params": { "ids": all } })),
        }],
        Some("tooLarge") => vec![Problem {
            kind: "tooLarge",
            error: true,
            ids: vec![],
            at: None,
            radius: 0.0,
            value: fit["overWidthMm"].as_f64().unwrap_or(0.0).max(fit["overHeightMm"].as_f64().unwrap_or(0.0)),
            fix: None,
        }],
        _ => vec![],
    }
}

/// Spots where objects stack and thread piles up past [`review::TOO_DENSE_LAYERS`], each with
/// the objects sewn there; Remove Overlaps fixes those where fills overlap.
fn dense_spots(parts: &[Sewn]) -> Vec<Problem> {
    let objs: Vec<_> = parts.iter().map(|o| o.obj.clone()).collect();
    let design = plan(&objs, &PlanSettings { center: false, ..PlanSettings::default() }, "");
    let m = review::density(&design, review::DENSITY_CELL_MM);
    let dense = |c: usize| m.layers.get(c).is_some_and(|l| f64::from(*l) > review::TOO_DENSE_LAYERS);
    // Join touching dense cells into spots.
    let mut spot_of: HashMap<usize, usize> = HashMap::new();
    let mut spots: Vec<Vec<usize>> = vec![];
    for start in 0..m.layers.len() {
        if !dense(start) || spot_of.contains_key(&start) {
            continue;
        }
        let n = spots.len();
        let mut cells = vec![];
        let mut todo = vec![start];
        spot_of.insert(start, n);
        while let Some(c) = todo.pop() {
            cells.push(c);
            let (r, col) = (c / m.cols.max(1), c % m.cols.max(1));
            let mut near = vec![];
            if col > 0 {
                near.push(c - 1);
            }
            if col + 1 < m.cols {
                near.push(c + 1);
            }
            if r > 0 {
                near.push(c - m.cols);
            }
            if r + 1 < m.rows {
                near.push(c + m.cols);
            }
            for nb in near {
                if dense(nb) && !spot_of.contains_key(&nb) {
                    spot_of.insert(nb, n);
                    todo.push(nb);
                }
            }
        }
        spots.push(cells);
    }
    let cell_of = |x: f64, y: f64| -> Option<usize> {
        let (c, r) = ((x - m.x0) / m.cell, (y - m.y0) / m.cell);
        if !(c >= 0.0 && r >= 0.0) || c >= m.cols as f64 || r >= m.rows as f64 {
            return None;
        }
        Some(r as usize * m.cols + c as usize)
    };
    // The objects with stitches in each spot.
    let mut ids: Vec<BTreeSet<NodeId>> = vec![BTreeSet::new(); spots.len()];
    let mut fills: Vec<BTreeSet<NodeId>> = vec![BTreeSet::new(); spots.len()];
    for o in parts {
        for st in &o.obj.stitches {
            if let Some(n) = cell_of(st.x, st.y).and_then(|c| spot_of.get(&c)) {
                if let Some(set) = ids.get_mut(*n) {
                    set.insert(o.id);
                }
                if o.settings.kind.is_fill()
                    && let Some(set) = fills.get_mut(*n)
                {
                    set.insert(o.id);
                }
            }
        }
    }
    let mut out: Vec<Problem> = spots
        .iter()
        .enumerate()
        .map(|(n, cells)| {
            let centre =
                |c: usize| (m.x0 + (c % m.cols.max(1)) as f64 * m.cell + m.cell / 2.0, m.y0 + (c / m.cols.max(1)) as f64 * m.cell + m.cell / 2.0);
            let k = cells.len().max(1) as f64;
            let (sx, sy) = cells.iter().map(|c| centre(*c)).fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
            let (cx, cy) = (sx / k, sy / k);
            let radius = cells.iter().map(|c| centre(*c)).map(|p| (p.0 - cx).hypot(p.1 - cy)).fold(0.0f64, f64::max) + m.cell;
            let worst = cells.iter().filter_map(|c| m.layers.get(*c)).copied().fold(0.0f32, f32::max);
            let here: Vec<NodeId> = ids.get(n).map(|s| s.iter().copied().collect()).unwrap_or_default();
            let overlapping: Vec<NodeId> = fills.get(n).map(|s| s.iter().copied().collect()).unwrap_or_default();
            Problem {
                kind: "tooDense",
                error: f64::from(worst) > HEAVY_LAYERS,
                ids: here,
                at: Some((cx, cy)),
                radius,
                value: f64::from(worst),
                fix: (overlapping.len() > 1).then(|| json!({ "command": "stitch.removeOverlaps", "params": { "ids": overlapping } })),
            }
        })
        .collect();
    // Only where objects stack: one object's own dense spots (a satin's corners, small
    // lettering) are how it sews, and nothing here would change them.
    out.retain(|p| p.ids.len() > 1);
    out.sort_by(|a, b| b.value.total_cmp(&a.value));
    out.truncate(MAX_SPOTS);
    out
}

/// A part's own problems: too small, a fill too thin, a satin too wide or narrow.
fn part_problems(d: &Document, o: &Sewn) -> Vec<Problem> {
    let pts: Vec<(f64, f64)> =
        o.obj.stitches.iter().filter(|s| s.kind != StitchKind::Jump && s.x.is_finite() && s.y.is_finite()).map(|s| (s.x, s.y)).collect();
    let Some(&first) = pts.first() else { return vec![] };
    let (lo, hi) = pts.iter().fold((first, first), |(lo, hi), p| ((lo.0.min(p.0), lo.1.min(p.1)), (hi.0.max(p.0), hi.1.max(p.1))));
    let at = Some(((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0));
    let radius = (hi.0 - lo.0).hypot(hi.1 - lo.1) / 2.0;
    let problem = |kind, value, fix| Problem { kind, error: false, ids: vec![o.id], at, radius, value, fix };
    let size = (hi.0 - lo.0).max(hi.1 - lo.1);
    if size < TINY_MM {
        return vec![problem("tiny", size, None)];
    }
    let kind = o.settings.kind;
    if kind.is_fill() {
        let width = d.node(o.id).and_then(mm_polylines).map_or(f64::INFINITY, |rings| mean_width(&rings));
        if width < THIN_FILL_MM {
            return vec![problem("thinFill", width, Some(json!({ "command": "stitch.convertToSatin", "params": { "ids": [o.id] } })))];
        }
    } else if matches!(kind, StitchType::Satin | StitchType::SatinBlocks) {
        // The satin's width is how far each top stitch crosses it.
        let mut spans: Vec<f64> = o
            .obj
            .stitches
            .windows(2)
            .filter_map(|w| match w {
                [a, b] if a.kind == StitchKind::Top && b.kind == StitchKind::Top => Some((a.x - b.x).hypot(a.y - b.y)),
                _ => None,
            })
            .collect();
        spans.sort_by(f64::total_cmp);
        let at_rank = |q: f64| spans.get(((spans.len() as f64 - 1.0) * q).round().max(0.0) as usize).copied();
        if let Some(wide) = at_rank(0.9).filter(|w| *w > WIDE_SATIN_MM) {
            // Split satin turned off, or set longer than this: back to the default.
            let part = if o.part == Part::Stroke { "stroke" } else { "fill" };
            let split = SatinSettings::default().split_length;
            let fix =
                json!({ "command": "stitch.set", "params": { "ids": [o.id], "part": part, "settings": { "satin": { "splitLength": split } } } });
            return vec![problem("wideSatin", wide, Some(fix))];
        }
        if let Some(narrow) = at_rank(0.5).filter(|w| *w < NARROW_SATIN_MM) {
            let fix =
                (o.part == Part::Stroke).then(|| json!({ "command": "stitch.set", "params": { "ids": [o.id], "part": "stroke", "type": "run" } }));
            return vec![problem("narrowSatin", narrow, fix)];
        }
    }
    vec![]
}

/// A shape's average width, mm: twice its area over its outline's length (a long strip `w` wide
/// gives `w`). Rings inside an odd number of others are holes.
pub(crate) fn mean_width(rings: &[Polyline]) -> f64 {
    let ring_area = |r: &Polyline| {
        let n = r.points.len();
        (0..n).filter_map(|i| Some((r.points.get(i)?, r.points.get((i + 1) % n)?))).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f64>().abs() / 2.0
    };
    let inside = |p: roidery_stitch::Point, r: &Polyline| {
        let mut c = false;
        for (a, b) in r.points.iter().zip(r.points.iter().cycle().skip(1)) {
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                c = !c;
            }
        }
        c
    };
    let mut area = 0.0;
    let mut length = 0.0;
    for (i, r) in rings.iter().enumerate() {
        let Some(&p) = r.points.first() else { continue };
        let depth = rings.iter().enumerate().filter(|(j, o)| *j != i && inside(p, o)).count();
        area += if depth % 2 == 0 { ring_area(r) } else { -ring_area(r) };
        // Round the ring, back to its start.
        length += r.points.iter().zip(r.points.iter().cycle().skip(1)).map(|(a, b)| (a.x - b.x).hypot(a.y - b.y)).sum::<f64>();
    }
    if length > 0.0 { 2.0 * area.max(0.0) / length } else { f64::INFINITY }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roidery_stitch::Point;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Polyline {
        Polyline::new(vec![Point::new(x, y), Point::new(x + w, y), Point::new(x + w, y + h), Point::new(x, y + h)], true)
    }

    #[test]
    fn mean_width_of_strips_and_rings() {
        // A 100 × 1 strip: 2 × 100 / 202.
        assert!((mean_width(&[rect(0.0, 0.0, 100.0, 1.0)]) - 200.0 / 202.0).abs() < 1e-9);
        // A square ring 1 wide: its hole is taken away and its outline counted.
        let w = mean_width(&[rect(0.0, 0.0, 20.0, 20.0), rect(1.0, 1.0, 18.0, 18.0)]);
        assert!((w - 2.0 * (400.0 - 324.0) / 152.0).abs() < 1e-9, "{w}");
        assert!(mean_width(&[]).is_infinite());
    }
}
