//! Roidery: embroidery. Every visible path and compound path is an embroidery object, sewn in
//! paint order (bottom to top). Each object has two parts (`parts`): its fill sews with a fill
//! stitch in the fill colour, then its stroke with a line stitch in the stroke colour. Each part
//! has stitch settings of its own (`stitch.set`) or, without them, the automatic rule: a fill is
//! tatami, a stroke a running stitch. `stitch.export` writes the result as a machine file.
//!
//! Text objects (the Type tools, in any installed font) are embroidery objects too: their glyph
//! outlines sew as one fill in the text's colour, and the text stays editable.
//!
//! Satin Blocks objects are paths with two subpaths, the left and right rails: anchor `i` of
//! each forms rung `i`, as the Satin Blocks tool draws them.

use std::collections::BTreeSet;

use roidery_formats::{Design, Format, Thread};
use roidery_plan::{PlanObject, PlanSettings, plan, review};
use roidery_stitch::{Point as SPoint, Polyline, Shape, StitchSettings, StitchType, generate_from, polylines_from_bezpath};
use serde_json::{Value, json};
use vectorcraft_color::Paint;
use vectorcraft_doc::{Document, Node, NodeKind};
use vectorcraft_geom::BezPath;

use super::overprint::edit_counted;

mod check;
mod convert;
pub(crate) mod design;
mod guide;
mod lettering;
pub(crate) mod machine;
mod mockup;
mod overlaps;
pub(crate) mod parts;
mod satin_edit;
mod sequence;
pub mod trace;
mod worksheet;
use design::{DesignSettings, HoopField};
use parts::{Part, Slot};

use super::*;

/// Millimetres per document unit (points).
pub const MM_PER_PT: f64 = 25.4 / 72.0;
/// How closely curves are followed when flattened for stitching, mm.
const FLATTEN_MM: f64 = 0.05;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "stitch.set",
            "Stitch Settings",
            [],
            None,
            "{part?: \"fill\"|\"stroke\" (which part to set: the fill sews fill types in the fill colour, then the stroke sews line types in the stroke colour; without part, the type's own part is set and the other part stops sewing, and settings without a type go to every part that sews), type?: \"none\"|\"run\"|\"triple\"|\"satin\"|\"satinBlocks\"|\"tatami\"|\"contour\"|\"circular\"|\"gradient\"|\"motif\"|\"manual\"|\"eStitch\"|\"rope\", settings?: {run?: {length}, satin?: {width, density, pullCompensation, fractionalSpacing, underlays: [{type: \"centerWalk\"|\"contour\"|\"zigZag\", length?, spacing?, inset?}]}, fill?: {angle, rowSpacing, stitchLength, staggers, pullCompensation, underlay, underlaySpacing, underlayInset, endSpacing (gradient: row spacing on the far side)}, circular?: {centerX, centerY (mm from the middle of the shape's bounds)}, motif?: {pattern: \"zigzag\"|\"wave\"|\"diamonds\"|\"scales\", width, height, spacing, angle, length, triple}, eStitch?: {width, spacing}, rope?: {width, spacing, slant, passes: 1-3}} (mm and degrees; merged into what the object has), reset?: bool (back to the automatic rule), ids?} set the embroidery stitch settings of the paths and compound paths in ids or the selection, as one undo step; a satin, E-stitch or rope stroke's width is its stroke weight (setting one sets the other), a running stitch set here shows at thread thickness (0.4 mm), and a part set on an object without its paint gets that paint in the object's other colour → {changed}",
            has_doc,
            set
        ),
        cmd!(
            query "stitch.info",
            "Stitch Info",
            [],
            None,
            "{ids?} the embroidery objects in ids or the selection (every one when nothing is selected) in sewing order, with their stitch settings (auto: from the automatic rule) and stitch counts, and the design's totals; type, auto, settings and thread are the first part that sews → {objects: [{id, type, auto, settings, stitches, thread: [r, g, b], whole (Satin Blocks, manual or lettering: no fill part), fill: {type, auto, sews, settings, stitches, thread}, stroke: {…}}], stitches, trims, colorChanges, widthMm, heightMm, sewMinutes (on a home machine: 650 stitches a minute, 5 s a trim, 30 s a colour change), threadMeters: [top thread for each colour block, m], bobbinMeters (estimates: stitch lengths, 0.8 mm take-up a stitch, 15 mm tails, bobbin a third of the top), hoop?: {id, brand, name, widthMm, heightMm, round, fit: \"fits\"|\"outside\"|\"rotate\"|\"tooLarge\", overWidthMm?, overHeightMm?, outsideMm?} when the design has a hoop (the page is the hoop: the fit is checked where the design sits on the first artboard; outside: small enough but partly off the page, by outsideMm), lettering: [{id, lettering: {text, font, height, spacing, lineSpacing, path?}, digitized, fontName, sizeMin?, sizeMax?, multicolor?, error?}] for the lettering objects among ids or the selection (a letter stands for its lettering)}",
            has_doc,
            info
        ),
        cmd!(
            query "stitch.density",
            "Stitch Density",
            [],
            None,
            "{ids?, cellMm?: 0.5-50 (default 2), cells?: bool} layers of thread across the design (1: covered once, as a fill at its usual density), cell by cell, for ids or the selection (every object when nothing is selected) → {max, dense: cells over 1.8 layers, tooDense: cells over 2.6, cellMm, x0, y0 (mm, the plan's uncentred coordinates), cols, rows, layers?: [per cell, row by row] when cells}",
            has_doc,
            density
        ),
        cmd!(
            query "stitch.export",
            "Export Machine File…",
            [],
            None,
            "{path?: string, format?: \"dst\"|\"pes\"|\"jef\"|\"exp\"|\"vp3\"|\"xxx\"|\"u01\"|\"tap\" (default: from the path's extension, else dst), name?: string (the design name in the file), trimAt?: mm, tieIn?: bool, tieOff?: bool, travelUnder?: bool (default true: travel where fills sewn later hide it is sewn as running stitches instead of a jump and trim)} sew every visible object in paint order and write the machine file to path (with a hoop, the needle starts at the first artboard's centre, so the design keeps its place in the hoop; without one, the design is centred) (without a path: return it as dataBase64) → {path?, dataBase64?, format, stitches, trims, colorChanges, widthMm, heightMm}",
            has_doc,
            export
        ),
        cmd!(
            query "stitch.mockup",
            "Export Mockup…",
            [],
            None,
            "{path?: string, garment?: \"tshirt\"|\"cap\"|\"tote\"|\"swatch\" (default tshirt; swatch: a piece of fabric round the design), placement?: \"center\"|\"leftChest\" (T-shirt; default center), color?: \"#rrggbb\" (the garment's colour), fabric?: a fabric id (its weave; default the design's fabric), ppm?: pixels per mm (0.5 to 40; default 3 for a T-shirt, 8 a cap, 4 a tote, 20 a swatch; at most 4000 pixels a side), format?: \"png\"|\"jpg\"} a picture of the design sewn on the garment: every stitch drawn as realistic thread on the fabric's weave, at its real size on the garment; without a path: dataBase64 → {path?, dataBase64?, format, width, height, ppm, garment, bytes}",
            has_doc,
            mockup::mockup
        ),
        cmd!(
            query "stitch.worksheet",
            "Export Worksheet…",
            [],
            None,
            "{path?: string, format?: \"pdf\"|\"svg\"|\"png\" (default pdf), paper?: \"a4\"|\"letter\" (default a4), ppi?: png pixels per inch (default 72), name?: string (default: the document's title), labels?: {English text: translation} (the sheet's words in another language; keys: Production Worksheet, Actual size, Scale {n}%, Size, Stitches, Colors, Trims, Sew time, Thread, Hoop, Fabric, {h} h {m} min, {m} min, {top} m + {bobbin} m bobbin, Fits the hoop, Fits turned 90°, Too large for the hoop, Color sequence, {n} stitches, and {n} more, and the fabric's label)} a one-page production worksheet: the design as it sews in its hoop (actual size when it fits the page), its size, stitches, colours, trims, sew time, thread and fabric, and each colour in sewing order with its thread and stitch count; written as document.export writes (without a path: dataBase64) → {path?, dataBase64?, format, bytes, scale (1: actual size), colors}",
            has_doc,
            worksheet::worksheet
        ),
        cmd!(
            "stitch.createSatinBlocks",
            "Satin Blocks",
            [],
            None,
            "{left: [[x, y]…], right: [[x, y]…] (pt; point i of each rail forms rung i; a point may be {p: [x, y], in?: [x, y], out?: [x, y]}, whose handles curve the rail and the stitches with it), settings?: as stitch.set's} create a Satin Blocks object: a path whose two subpaths are the rails, stroked in the current stroke colour (else the fill colour) → {id}",
            has_doc,
            create_satin_blocks
        ),
        cmd!(
            "stitch.sequence",
            "Sequencer",
            [],
            None,
            "{thumbs?: bool} every embroidery object in sewing order, one row per sewn part (fill, stroke) and one for an object that sews nothing (hidden objects too, unless their layer or group is hidden), and the colour blocks they sew in (runs of one thread; objects without stitches don't break a run) → {objects: [{id, name, part: \"fill\"|\"stroke\"|null, type, auto, stitches, thread: [r, g, b], block: index into colors | null, visible, locked, parents: [layer and group ids, outermost first], thumb?: [[x, y]…] (with thumbs: up to 240 of its stitches, mm)}], groups: [{id, name, kind: \"layer\"|\"group\", visible, locked}], colors: [{thread, threadName, threadCode (from the design's thread chart; empty when the colour isn't in it), objects: [id…], stitches}], stitches, colorChanges}",
            has_doc,
            sequence::sequence
        ),
        cmd!(
            "stitch.move",
            "Reorder Sequence",
            [],
            None,
            "{ids: [id…], before?: id, after?: id} sew the objects (kept in their order) just before or just after another one, by moving them next to it in its layer or group; one undo step → {moved}",
            has_doc,
            sequence::move_objects
        ),
        cmd!(
            "stitch.partOrder",
            "Sewing Order of Fill and Stroke",
            [],
            None,
            "{ids?, first: \"fill\"|\"stroke\"} sew the fill or the stroke of the objects in ids or the selection first (default: the fill, so the stroke borders it); one undo step → {changed}",
            has_doc,
            sequence::part_order
        ),
        cmd!(
            "stitch.sortColors",
            "Sort by Color",
            [],
            None,
            "{} sew each thread colour in one go where the stacking allows: within each layer or group, objects are reordered by colour (colours in the order they first sew, each colour's objects in their old order); one undo step → {colorChanges, changed}",
            has_doc,
            sequence::sort_colors
        ),
        cmd!(
            query "stitch.design",
            "Embroidery Design",
            [],
            None,
            "{} the design's fabric and thread chart, the choices for each, and the design's thread colours (first sewn first) with the chart thread each is, or is nearest to → {fabric, fabricLabel, fabrics: [{id, label}], chart: {name, threads: count} | null, charts: [{id, name}], threads: [{rgb, name, code, inChart, nearest: {rgb, name, code, deltaE} | null, objects: [id…]}], hoop: {id, brand, name, widthMm, heightMm, round} | null (the page is the hoop: a first artboard resized by hand makes it a custom hoop of the page's size), field: {x, y, width, height} | null (the sewing field, the first artboard, in document units), hoops: [{id, brand, name, widthMm, heightMm, round}] (sewing fields by machine brand)}",
            has_doc,
            design::design
        ),
        cmd!(
            "stitch.setDesign",
            "Embroidery Design",
            [],
            None,
            "{fabric?: \"woven\"|\"knit\"|\"pique\"|\"terry\"|\"fleece\"|\"denim\"|\"twill\"|\"leather\" (sets the density, pull compensation and underlay of automatic objects, and, unless applyToObjects is false, of objects with settings of their own), chart?: \"brother\"|\"janome\"|\"none\"|{name?, threads: [{rgb: [r, g, b], name, code}]}|{name?, text: GIMP palette or CSV}|{name?, path}, applyToObjects?: bool, hoop?: hoop id (see stitch.design)|\"none\"|{width, height, name?, round?} (mm)} (the first artboard is the hoop: it takes the hoop's sewing field, about its centre, with its centre mark) set the design's fabric, thread chart (machine files name threads whose colour is in the chart) and hoop (Stitch Settings and stitch.export say whether the design fits it); one undo step → {fabric, chart, hoop, changed}",
            has_doc,
            design::set_design
        ),
        cmd!(
            "stitch.matchThreads",
            "Match to Thread Chart",
            [],
            None,
            "{ids?} change the sewing colour (a fill's fill, a line's stroke) of the objects in ids or the selection (every object when nothing is selected) to the nearest thread in the design's chart; one undo step → {changed, chart}",
            has_doc,
            design::match_threads
        ),
        cmd!(
            "stitch.createLettering",
            "Lettering",
            [],
            None,
            "{text: string (lines split on \\n), font?: id or name from stitch.fonts (default \"sans\"), height?: mm cap height (default 10; a digitized font: the size it was digitized at), spacing?: mm between letters, lineSpacing?: × height (default: the font's), x?, y?: pt where the first baseline starts (default: centred on the first artboard)} create a lettering object. In a built-in font: the text's strokes as one path, sewn as satin columns as wide as suits the height, stroked that wide in the current stroke colour. In a digitized font: a group with a group per letter holding the font's pieces, each sewn as the font says (satin, fill, running stitch), in the current stroke colour unless the font has its own colours → {id, missing: characters the font doesn't have}",
            has_doc,
            lettering::create
        ),
        cmd!(
            query "stitch.fonts",
            "Lettering Fonts",
            [],
            None,
            "{rescan?: bool (look for fonts added since)} the lettering fonts: the built-in satin fonts (digitized: false) and the digitized embroidery fonts in the font library (digitized: true; Ink/Stitch's font format, found beside Roidery and in userFolder) → {fonts: [{id, name, digitized, description, keywords?, license?, satin, fill, run, multicolor (sews in its own several colours), glyphs?, size? (mm cap height new lettering starts at: the size it was digitized at, brought into its range), sizeMin?, sizeMax? (mm cap heights it sews well at), preview? (image file), user? (in the user's folder), folder?}], userFolder}",
            has_doc,
            lettering::fonts
        ),
        cmd!(
            "stitch.setLettering",
            "Lettering",
            [],
            None,
            "{text?, height?, spacing?, lineSpacing?, font?, path?: the id of a path to set the lettering along (null straightens it), ids?} change the lettering objects in ids or the selection (a letter stands for its lettering; every one when nothing is selected) and lay them out again where they are; a new font sets its own line spacing unless lineSpacing is given, and switching between a built-in and a digitized font replaces the object (same id); a column width that followed the height follows the new one; one undo step → {changed, missing}",
            has_doc,
            lettering::set
        ),
        cmd!(
            "stitch.mapLettering",
            "Map Lettering to Path",
            [],
            None,
            "{ids?, path?: id of an open path (default: the one open path in ids or the selection), keep?: bool} lettering in ids or the selection follows the path: each letter stands on it, turned with it, where it would be along a straight baseline; the path is removed unless keep; setLettering {path: null} straightens it again; one undo step → {changed, missing}",
            has_doc,
            lettering::map
        ),
        cmd!(
            "stitch.moveRung",
            "Move Rung",
            [],
            None,
            "{id: a Satin Blocks object, index: rung (0 is where sewing starts), left?: [x, y], right?: [x, y] pt} move the ends of one rung (their curve handles move with them); one undo step → {changed}",
            has_doc,
            satin_edit::move_rung
        ),
        cmd!(
            "stitch.insertRung",
            "Insert Rung",
            [],
            None,
            "{id: a Satin Blocks object, index: the rung to insert after, t?: 0 to 1 along the way to the next rung by length (default 0.5)} add a rung between two others, on both rails (a curved rail keeps its shape); one undo step → {changed, index: the new rung}",
            has_doc,
            satin_edit::insert_rung
        ),
        cmd!(
            "stitch.deleteRung",
            "Delete Rung",
            [],
            None,
            "{id: a Satin Blocks object, index: rung} remove one rung from both rails (two always stay); one undo step → {changed}",
            has_doc,
            satin_edit::delete_rung
        ),
        cmd!(
            "stitch.traceImage",
            "Image Trace",
            [],
            None,
            "{preset?: \"2 Color\"|\"3 Color\"|\"6 Color\"|\"Detailed\" (default 3 Color: VectorCraft's Default, 3 Colors, 6 Colors and Flat Logo presets), id?, ids?: the images (default: the selection)} trace each image with the preset, white left out so the fabric shows (3 and 6 Color then trace one colour more, so they sew that many threads), expand it into paths and set them to sew as tatami fills; one undo step → {id: the traced paths' group, paths, colors, preset}",
            has_doc,
            trace::trace_image
        ),
        cmd!(
            "stitch.convertToSatin",
            "Convert to Satin (Centerline)",
            [],
            None,
            "{ids?} turn each closed shape in ids or the selection into Satin Blocks along its spine (its exact medial axis from a Voronoi diagram of the outline: the longest path through the shape's skeleton, a loop for a ring), rungs square to the spine out to the outline and fanned round tight bends; each arm the spine leaves (a Y, a star) becomes another column; the columns sew in the shape's colour just after it and are selected, and the shape is hidden (show it to go back); one undo step → {ids: the new objects, skipped: [ids that aren't closed shapes], widestMm, warning?: when a column is over 10 mm wide}",
            has_doc,
            convert::convert
        ),
        cmd!(
            "stitch.convertToSatinClassic",
            "Convert to Satin (Classic)",
            [],
            None,
            "{ids?} as stitch.convertToSatin, the way Roidery first did it, kept to compare: the shape is drawn into a grid and thinned (Zhang–Suen) to its skeleton, and rungs are only those cast along the spine (no extra rungs to the outline's corners or an end's tip) → {ids, skipped, widestMm, warning?}",
            has_doc,
            convert::convert_classic
        ),
        cmd!(
            "stitch.swapSides",
            "Swap Satin Sides",
            [],
            None,
            "{ids?} swap the left and right rails of the Satin Blocks objects in ids or the selection, so each stitch starts from the other side; one undo step → {changed}",
            has_doc,
            satin_edit::swap_sides
        ),
        cmd!(
            "stitch.setAngle",
            "Fill Angle",
            [],
            None,
            "{angle: degrees counter-clockwise from the x axis as seen on screen, ids?} set the row angle of the fills in ids or the selection (tatami, gradient and motif fills; contour and circular fills turn their underlay), as one undo step; the Fill Angle tool sets it by dragging → {changed}",
            has_doc,
            set_angle
        ),
        cmd!(
            "stitch.setGuide",
            "Use as Fill Guide",
            [],
            None,
            "{ids?, guide?: id of an open path (default: the one open path in ids or the selection), keep?: bool} make the fills in ids or the selection guided fills whose rows follow the guide line; the guide is stored with each fill, scaled to its bounds, and removed from the artwork unless keep; one undo step → {changed, guideRemoved}",
            has_doc,
            guide::set_guide
        ),
        cmd!(
            "stitch.centerInHoop",
            "Center in Hoop",
            [],
            None,
            "{ids?} move the objects in ids or the selection (everything unlocked when nothing is selected) together so their middle sits in the middle of the artboard they are on, which in Roidery is the hoop's sewing field; one undo step → {dx, dy} (document units)",
            has_doc,
            center_in_hoop
        ),
        cmd!(
            query "stitch.check",
            "Check Design",
            [],
            None,
            "{ids?} what will sew badly in ids or the selection (every object when nothing is selected or ids is empty), errors first → {problems: [{kind: \"outsideHoop\" (value: mm past the edge)|\"tooLarge\" (value: mm too big)|\"tooDense\" (value: layers of thread, over 2.6)|\"thinFill\" (value: mean width mm, under 1.5)|\"wideSatin\" (value: mm, over 10)|\"narrowSatin\" (value: mm, under 1)|\"tiny\" (value: mm, under 1.5), level: \"error\"|\"warning\", ids: the objects involved, value, x?, y?, radius? (where, document units), fix?: {command, params} (the command that fixes it)}], errors, warnings}",
            has_doc,
            check::check
        ),
        cmd!(
            "stitch.removeOverlaps",
            "Remove Overlaps",
            [],
            None,
            "{ids?, overlap?: mm from 0 to 20 (default 1)} cut the fills in ids or the selection (every fill when nothing is selected) where fills sewn after them cover them, keeping `overlap` mm under the covering shapes' edges so no fabric shows; changes the paths, one undo step → {changed}",
            has_doc,
            overlaps::remove
        ),
    ]
}

/// The objects to stitch under `n`, in paint order: visible paths (not guides or clipping
/// paths), compound paths (whose members are sewn as one) and text.
fn collect(n: &Node, out: &mut Vec<NodeId>) {
    if !n.visible {
        return;
    }
    match &n.kind {
        NodeKind::Path { guide: false, clipping: false, .. } | NodeKind::Compound { .. } | NodeKind::Text(_) => out.push(n.id),
        NodeKind::Path { .. } => {}
        _ => n.children().into_iter().flatten().for_each(|c| collect(c, out)),
    }
}

/// Every object to stitch in the document, in sewing order.
fn all_objects(d: &Document) -> Vec<NodeId> {
    let mut out = vec![];
    d.layers.iter().for_each(|l| collect(l, &mut out));
    out
}

/// The stitch objects in `ids` (groups and layers: those inside), in sewing order.
fn objects_in(d: &Document, ids: &[NodeId]) -> Vec<NodeId> {
    let want: BTreeSet<NodeId> = ids
        .iter()
        .filter_map(|id| d.node(*id))
        .flat_map(|n| {
            let mut v = vec![];
            collect(n, &mut v);
            v
        })
        .collect();
    all_objects(d).into_iter().filter(|id| want.contains(id)).collect()
}

fn solid(p: &Paint) -> Option<[u8; 3]> {
    let [r, g, b, _] = p.color()?.to_rgba8(1.0);
    Some([r, g, b])
}

/// The paint an object sews in: a text object's is its characters'.
fn paint_of(n: &Node) -> std::borrow::Cow<'_, vectorcraft_doc::Appearance> {
    match &n.kind {
        NodeKind::Text(t) => std::borrow::Cow::Owned(t.first_style().appearance()),
        _ => std::borrow::Cow::Borrowed(&n.appearance),
    }
}

fn fill_rgb(n: &Node) -> Option<[u8; 3]> {
    paint_of(n).fill().filter(|f| f.visible).and_then(|f| solid(&f.paint))
}

fn stroke_rgb(n: &Node) -> Option<[u8; 3]> {
    paint_of(n).stroke().and_then(|s| solid(&s.paint))
}

/// A text object's glyph outlines in document units, overlaps merged (fonts may draw a letter
/// from overlapping contours, which would otherwise sew twice or leave holes).
pub(crate) fn text_outline(t: &vectorcraft_doc::TextObject) -> vectorcraft_geom::PathData {
    let lay = vectorcraft_text::layout(vectorcraft_text::FontDb::global(), t);
    let mut all = vectorcraft_geom::PathData::default();
    for g in &lay.glyphs {
        all.subpaths.extend(vectorcraft_geom::PathData::from_bezpath(&g.outline).transformed(t.xf).subpaths);
    }
    if all.is_empty() {
        return all;
    }
    vectorcraft_pathops::unite_all(&[(&all, vectorcraft_geom::FillRule::NonZero)])
}

/// The path of a path or compound path, in document units.
fn bezpath(n: &Node) -> Option<BezPath> {
    match &n.kind {
        NodeKind::Path { path, .. } => Some(path.to_bezpath()),
        NodeKind::Compound { children, .. } => {
            let mut bp = BezPath::new();
            children.iter().filter_map(|c| c.path_data()).for_each(|pd| bp.extend(pd.to_bezpath()));
            Some(bp)
        }
        NodeKind::Text(t) => Some(text_outline(t).to_bezpath()),
        _ => None,
    }
}

/// The anchors of each subpath, in millimetres (Satin Blocks rails and manual stitches).
fn anchors_mm(n: &Node) -> Vec<Polyline> {
    let mut subs = vec![];
    let mut add = |pd: &vectorcraft_geom::PathData| {
        for sp in &pd.subpaths {
            subs.push(Polyline::new(sp.anchors.iter().map(|a| SPoint::new(a.p.x * MM_PER_PT, a.p.y * MM_PER_PT)).collect(), sp.closed));
        }
    };
    match &n.kind {
        NodeKind::Path { path, .. } => add(path),
        NodeKind::Compound { children, .. } => children.iter().filter_map(|c| c.path_data()).for_each(&mut add),
        _ => {}
    }
    subs
}

/// A Satin Blocks path's rails as curves between rungs, in millimetres, when either rail curves
/// anywhere (`None` when both are straight from rung to rung).
fn rail_curves_mm(n: &Node) -> Option<Shape> {
    let NodeKind::Path { path, .. } = &n.kind else { return None };
    let [l, r, ..] = path.subpaths.as_slice() else { return None };
    let rungs = l.anchors.len().min(r.anchors.len());
    let blocks = rungs.checked_sub(1)?;
    if (0..blocks).all(|i| l.segment_is_line(i) && r.segment_is_line(i)) {
        return None;
    }
    let rail = |sp: &vectorcraft_geom::SubPath| -> Vec<Vec<SPoint>> {
        (0..blocks)
            .map(|i| {
                let c = sp.segment(i);
                let mm = |p: kurbo::Point| SPoint::new(p.x * MM_PER_PT, p.y * MM_PER_PT);
                if sp.segment_is_line(i) {
                    return vec![mm(c.p0), mm(c.p3)];
                }
                let mut pts = vec![];
                kurbo::flatten([kurbo::PathEl::MoveTo(c.p0), kurbo::PathEl::CurveTo(c.p1, c.p2, c.p3)], FLATTEN_MM / MM_PER_PT, |el| match el {
                    kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => pts.push(mm(p)),
                    _ => {}
                });
                pts
            })
            .collect()
    };
    Some(Shape::RailCurves { left: rail(l), right: rail(r) })
}

/// The settings that describe an object in one line, and whether they come from the automatic
/// rule (which uses the design's fabric): its first part that sews ([`parts::primary`]).
fn settings_of(n: &Node, ds: &DesignSettings) -> (StitchSettings, bool) {
    parts::primary(n, ds)
}

/// The colour `part` of an object sews in, and whether it is the fill's: fills sew in the fill
/// colour, lines in the stroke colour (each in the other when the object has only that one).
fn part_rgb(n: &Node, part: Part) -> Option<([u8; 3], bool)> {
    let fill = fill_rgb(n).map(|c| (c, true));
    let stroke = stroke_rgb(n).map(|c| (c, false));
    if part == Part::Fill { fill.or(stroke) } else { stroke.or(fill) }
}

/// The thread `part` of an object sews with, named from the design's thread chart when its colour
/// is one of the chart's threads.
fn thread_of(n: &Node, part: Part, ds: &DesignSettings) -> Thread {
    let rgb = part_rgb(n, part).map_or([0, 0, 0], |c| c.0);
    ds.chart.as_ref().and_then(|c| c.exact(rgb)).cloned().unwrap_or(Thread { rgb, ..Default::default() })
}

/// One part of an object, sewn: its stitches in millimetres, started as near `from` (where the
/// needle is) as the part allows.
fn stitch_part(n: &Node, ds: &DesignSettings, part: Part, s: &StitchSettings, from: Option<SPoint>) -> PlanObject {
    let thread = thread_of(n, part, ds);
    let shape = match s.kind {
        StitchType::None => None,
        StitchType::SatinBlocks | StitchType::Manual => {
            let subs = anchors_mm(n);
            match (s.kind, subs.as_slice()) {
                (StitchType::SatinBlocks, [l, r, ..]) => {
                    Some(rail_curves_mm(n).unwrap_or_else(|| Shape::Rails { left: l.points.clone(), right: r.points.clone() }))
                }
                (StitchType::Manual, _) => Some(Shape::Paths(subs)),
                _ => mm_polylines(n).map(Shape::Paths),
            }
        }
        k if k.is_fill() => mm_polylines(n).map(Shape::Area),
        // A stroke's width profile (the Width tool's) shapes a satin into a Satin Column.
        StitchType::Satin => {
            let profile = n.appearance.stroke().and_then(|st| st.profile.as_ref()).map(|p| p.points.clone()).filter(|p| !p.is_empty());
            mm_polylines(n).map(|paths| match profile {
                Some(profile) => Shape::Column { paths, profile },
                None => Shape::Paths(paths),
            })
        }
        _ => mm_polylines(n).map(Shape::Paths),
    };
    // A fill hides travel sewn before it.
    let cover = match (&shape, s.kind.is_fill()) {
        (Some(Shape::Area(rings)), true) => rings.clone(),
        _ => vec![],
    };
    // Lettering keeps the stroke order its font gives.
    let from = from.filter(|_| s.lettering.is_none());
    let stitches = shape.map(|sh| generate_from(&sh, s, from)).unwrap_or_default();
    PlanObject { stitches, thread, cover }
}

pub(crate) fn mm_polylines(n: &Node) -> Option<Vec<Polyline>> {
    let mut bp = bezpath(n)?;
    bp.apply_affine(kurbo::Affine::scale(MM_PER_PT));
    Some(polylines_from_bezpath(&bp, FLATTEN_MM))
}

/// Deep-merge `patch` into `base` (objects merge key by key; anything else replaces).
fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                merge(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (b, p) => *b = p.clone(),
    }
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.set";
    let kind = str_param(p, "type").map(|t| StitchType::parse(t).ok_or_else(|| bad(C, format!("unknown stitch type `{t}`")))).transpose()?;
    let part = str_param(p, "part").map(|t| Part::parse(t).ok_or_else(|| bad(C, format!("`part` must be fill or stroke, not `{t}`")))).transpose()?;
    if let (Some(part), Some(k)) = (part, kind)
        && !part.allows(k)
    {
        return Err(bad(C, format!("a {} can't sew as {}", part.id(), k.id())));
    }
    let patch = p.get("settings").cloned();
    if patch.as_ref().is_some_and(|v| !v.is_object()) {
        return Err(bad(C, "`settings` must be an object"));
    }
    let reset = p.get("reset").and_then(Value::as_bool).unwrap_or(false);
    if kind.is_none() && patch.is_none() && !reset {
        return Err(bad(C, "give type, settings or reset"));
    }
    let ids = targets(s, p)?;
    let objs = objects_in(&s.doc()?.doc, &ids);
    let ds = DesignSettings::of(&s.doc()?.doc);
    // The new settings of one part: the current ones with the type and the patch applied.
    let apply = |n: &Node, part: Part| -> Result<StitchSettings> {
        let (cur, _) = parts::part_settings(n, &ds, part);
        let mut v = serde_json::to_value(&cur).map_err(|e| bad(C, e.to_string()))?;
        if let Some(k) = kind {
            v["type"] = json!(k.id());
        }
        if let Some(pa) = &patch {
            merge(&mut v, pa);
        }
        serde_json::from_value(v).map_err(|e| bad(C, format!("bad settings: {e}")))
    };
    // Work out every object's new settings first, so a bad value changes nothing.
    let mut todo = vec![];
    {
        let d = &s.doc()?.doc;
        for id in &objs {
            let Some(n) = d.node(*id) else { continue };
            let old = parts::slots(n);
            let mut new = old.clone();
            match (part, kind) {
                (None, _) if reset => new = parts::Slots { stroke_first: old.stroke_first, ..parts::Slots::AUTO },
                (Some(part), _) if reset => *new.get_mut(part) = Slot::Auto,
                (Some(part), _) => *new.get_mut(part) = Slot::Set(apply(n, part)?),
                // A type alone: the object sews as that type, in the type's part.
                (None, Some(k)) => {
                    let part = Part::of(k);
                    let off = Slot::Set(StitchSettings::of(StitchType::None));
                    new = parts::Slots::only(off.clone(), off);
                    if k != StitchType::None {
                        *new.get_mut(part) = Slot::Set(apply(n, part)?);
                    }
                }
                // Settings alone: every part that sews.
                (None, None) => {
                    for (pt, ..) in parts::sewn(n, &ds) {
                        *new.get_mut(pt) = Slot::Set(apply(n, pt)?);
                    }
                }
            }
            // A part set to sew where the object has no such paint takes the other colour.
            let paint = part.filter(|pt| matches!(new.get(*pt), Slot::Set(st) if st.kind != StitchType::None)).filter(|pt| match pt {
                Part::Fill => fill_rgb(n).is_none(),
                Part::Stroke => stroke_rgb(n).is_none(),
            });
            let thread = kind.is_some_and(|k| matches!(k, StitchType::Run | StitchType::Triple));
            let sync = matches!(&new.stroke, Slot::Set(st) if st != &parts::part_settings(n, &ds, Part::Stroke).0);
            if new != old || paint.is_some() || sync {
                todo.push((*id, new, paint, thread));
            }
        }
    }
    let changed = edit_counted(s, "Stitch Settings", |d| {
        let mut count = 0;
        for (id, new, paint, thread) in &todo {
            if let Some(n) = d.node_mut(*id) {
                if let Some(pt) = paint {
                    let other = match pt {
                        Part::Fill => stroke_rgb(n),
                        Part::Stroke => fill_rgb(n),
                    };
                    let [r, g, b] = other.unwrap_or([0, 0, 0]);
                    let color = Paint::solid(vectorcraft_color::Color::rgb8(r, g, b));
                    match pt {
                        Part::Fill => n.appearance.set_fill(color),
                        Part::Stroke => n.appearance.set_stroke(color),
                    }
                }
                parts::write(n, new, *thread);
                count += 1;
            }
        }
        Ok(count)
    })?;
    Ok(json!({ "changed": changed }))
}

fn set_angle(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.setAngle";
    let angle = p.get("angle").and_then(Value::as_f64).filter(|a| a.is_finite()).ok_or_else(|| bad(C, "`angle` must be a number of degrees"))?;
    let angle = angle.rem_euclid(360.0);
    let ids = targets(s, p)?;
    let d = &s.doc()?.doc;
    let ds = DesignSettings::of(d);
    let mut todo = vec![];
    for id in objects_in(d, &ids) {
        let Some(n) = d.node(id) else { continue };
        if !parts::sewn(n, &ds).iter().any(|(pt, ..)| *pt == Part::Fill) {
            continue;
        }
        let (mut st, _) = parts::part_settings(n, &ds, Part::Fill);
        match st.kind {
            StitchType::Motif => st.motif.angle = angle,
            k if k.is_fill() => st.fill.angle = angle,
            _ => continue,
        }
        let mut slots = parts::slots(n);
        if slots.fill != Slot::Set(st.clone()) {
            slots.fill = Slot::Set(st);
            todo.push((id, slots));
        }
    }
    let _ = C;
    let changed = edit_counted(s, "Fill Angle", |d| {
        let mut count = 0;
        for (id, slots) in &todo {
            if let Some(n) = d.node_mut(*id) {
                parts::write(n, slots, false);
                count += 1;
            }
        }
        Ok(count)
    })?;
    Ok(json!({ "changed": changed }))
}

fn center_in_hoop(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.centerInHoop";
    let sel = targets(s, p)?;
    let d = &s.doc()?.doc;
    let ids: Vec<NodeId> = if sel.is_empty() {
        d.layers
            .iter()
            .filter(|l| l.visible && !l.locked)
            .flat_map(|l| l.children().into_iter().flatten().filter(|c| !c.locked).map(|c| c.id))
            .collect()
    } else {
        sel
    };
    let b = d.bounds_of(&ids, true).ok_or_else(|| bad(C, "nothing to center"))?;
    let i = d.artboard_at(b.center()).unwrap_or(0);
    let a = d.artboards.get(i).map(|a| a.rect).ok_or_else(|| bad(C, "the document has no artboard"))?;
    let dv = a.center() - b.center();
    if dv.hypot() > 1e-6 {
        s.edit("Center in Hoop", |d, _| {
            for id in &ids {
                if let Some(n) = d.node_mut(*id) {
                    n.transform(vectorcraft_geom::Affine::translate(dv), false);
                }
            }
            Ok(())
        })?;
    }
    Ok(json!({ "dx": dv.x, "dy": dv.y }))
}

/// One part of an object, sewn.
#[derive(Clone, Debug)]
pub(crate) struct Sewn {
    pub id: NodeId,
    pub part: Part,
    pub settings: StitchSettings,
    pub auto: bool,
    pub obj: PlanObject,
}

/// The design of `ids` in sewing order, with each sewn part's settings (an object that sews
/// nothing has no parts).
fn sew(d: &Document, ids: &[NodeId], ps: &PlanSettings, name: &str) -> (Vec<Sewn>, Design) {
    let objs = sewn_parts(d, ids);
    let plan_objs: Vec<PlanObject> = objs.iter().map(|o| o.obj.clone()).collect();
    let design = plan(&plan_objs, ps, name);
    (objs, design)
}

/// Each part of `ids` stitched, in sewing order.
fn sewn_parts(d: &Document, ids: &[NodeId]) -> Vec<Sewn> {
    let ds = DesignSettings::of(d);
    let mut objs = Vec::with_capacity(ids.len());
    // Each part starts near where the last one ended.
    let mut at: Option<SPoint> = None;
    for id in ids {
        let Some(n) = d.node(*id) else { continue };
        for (part, settings, auto) in parts::sewn(n, &ds) {
            let obj = stitch_part(n, &ds, part, &settings, at);
            if let Some(last) = obj.stitches.last() {
                at = Some(last.point());
            }
            objs.push(Sewn { id: *id, part, settings, auto, obj });
        }
    }
    objs
}

fn totals(d: &Design) -> Value {
    let (w, h) = d.bounds().map_or((0.0, 0.0), |b| (b.width(), b.height()));
    let minutes = review::sew_minutes(d, &review::MachineSpeed::default());
    let thread = review::thread_use(d);
    json!({
        "stitches": d.stitch_count(), "trims": d.trims(), "colorChanges": d.color_changes(), "widthMm": w, "heightMm": h, "sewMinutes": minutes,
        "threadMeters": thread.per_block, "bobbinMeters": thread.bobbin,
    })
}

/// The stitch plan's settings for `d`: when the page is the hoop, the needle starts at the
/// page's centre, so the design keeps its place in the hoop; otherwise it is centred.
fn plan_settings(d: &Document) -> PlanSettings {
    PlanSettings { origin: HoopField::of(d).map(|f| f.origin_mm()), ..PlanSettings::default() }
}

/// [`totals`], with how the design sits in the document's hoop when it has one
/// (`hoop: {…, fit, overWidthMm?, overHeightMm?, outsideMm?}`). `d` is planned with
/// [`plan_settings`], so where the page is the hoop the fit is checked where the design sits.
fn totals_in_hoop(doc: &Document, d: &Design) -> Value {
    let mut out = totals(d);
    if let Some(field) = HoopField::of(doc) {
        let (w, h) = (out["widthMm"].as_f64().unwrap_or(0.0), out["heightMm"].as_f64().unwrap_or(0.0));
        let mut v = field.hoop.json();
        if let (Some(o), Value::Object(fit)) = (v.as_object_mut(), field.fit_json(d, w, h)) {
            o.extend(fit);
        }
        out["hoop"] = v;
    } else if let Some(hoop) = DesignSettings::of(doc).hoop {
        let (w, h) = (out["widthMm"].as_f64().unwrap_or(0.0), out["heightMm"].as_f64().unwrap_or(0.0));
        let mut v = hoop.json();
        if let (Some(o), Value::Object(fit)) = (v.as_object_mut(), hoop.fit_json(w, h)) {
            o.extend(fit);
        }
        out["hoop"] = v;
    }
    out
}

fn density(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.density";
    let cell = match p.get("cellMm") {
        None | Some(Value::Null) => review::DENSITY_CELL_MM,
        Some(v) => v.as_f64().filter(|c| (0.5..=50.0).contains(c)).ok_or_else(|| bad(C, "`cellMm` must be 0.5 to 50"))?,
    };
    let d = &s.doc()?.doc;
    let sel = targets(s, p)?;
    let ids = if sel.is_empty() { all_objects(d) } else { objects_in(d, &sel) };
    let (_, design) = sew(d, &ids, &PlanSettings { center: false, ..PlanSettings::default() }, "");
    let m = review::density(&design, cell);
    let mut out = json!({
        "max": m.max(),
        "dense": m.count_above(review::DENSE_LAYERS),
        "tooDense": m.count_above(review::TOO_DENSE_LAYERS),
        "cellMm": m.cell, "x0": m.x0, "y0": m.y0, "cols": m.cols, "rows": m.rows,
    });
    if p.get("cells").and_then(Value::as_bool).unwrap_or(false) {
        out["layers"] = json!(m.layers.iter().map(|l| (f64::from(*l) * 100.0).round() / 100.0).collect::<Vec<_>>());
    }
    Ok(out)
}

fn info(s: &mut Session, p: &Value) -> Result<Value> {
    let ds = s.doc()?;
    let d = &ds.doc;
    let sel = targets(s, p)?;
    let ids = if sel.is_empty() { all_objects(d) } else { objects_in(d, &sel) };
    let (objs, design) = sew(d, &ids, &plan_settings(d), "");
    let dset = DesignSettings::of(d);
    let rows: Vec<Value> = ids
        .iter()
        .filter_map(|id| {
            let n = d.node(*id)?;
            let mine: Vec<&Sewn> = objs.iter().filter(|o| o.id == *id).collect();
            let (st, auto) = settings_of(n, &dset);
            let thread = mine.first().map_or_else(|| thread_of(n, Part::of(st.kind), &dset).rgb, |o| o.obj.thread.rgb);
            let mut row = json!({
                "id": id, "type": st.kind.id(), "auto": auto, "settings": st, "thread": thread,
                "stitches": mine.iter().map(|o| o.obj.stitches.len()).sum::<usize>(),
                "whole": parts::slots(n).whole(),
            });
            for part in Part::BOTH {
                let sewn = mine.iter().find(|o| o.part == part);
                let mut v = parts::part_json(n, &dset, part, sewn.is_some());
                v["stitches"] = json!(sewn.map_or(0, |o| o.obj.stitches.len()));
                v["thread"] = json!(thread_of(n, part, &dset).rgb);
                row[part.id()] = v;
            }
            Some(row)
        })
        .collect();
    let mut out = totals_in_hoop(d, &design);
    out["objects"] = json!(rows);
    out["lettering"] = json!(lettering::info_rows(d, &sel));
    Ok(out)
}

fn export(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.export";
    let path = str_param(p, "path");
    let format = match (str_param(p, "format"), path) {
        (Some(f), _) => Format::from_extension(f).ok_or_else(|| bad(C, format!("unknown machine format `{f}`")))?,
        (None, Some(path)) => std::path::Path::new(path).extension().and_then(|e| e.to_str()).and_then(Format::from_extension).unwrap_or(Format::Dst),
        (None, None) => Format::Dst,
    };
    if !format.can_write() {
        return Err(bad(C, format!("{} files open in Roidery but can't be written; pick another format", format.label())));
    }
    let mut ps = plan_settings(&s.doc()?.doc);
    if let Some(v) = p.get("trimAt").and_then(Value::as_f64) {
        ps.trim_at = v;
    }
    if let Some(v) = p.get("tieIn").and_then(Value::as_bool) {
        ps.tie_in = v;
    }
    if let Some(v) = p.get("tieOff").and_then(Value::as_bool) {
        ps.tie_off = v;
    }
    if let Some(v) = p.get("travelUnder").and_then(Value::as_bool) {
        ps.travel_under = v;
    }
    let name = str_param(p, "name")
        .map(str::to_string)
        .or_else(|| path.and_then(|f| std::path::Path::new(f).file_stem()).map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "Roidery".into());
    let d = &s.doc()?.doc;
    let (_, design) = sew(d, &all_objects(d), &ps, &name);
    let bytes = format.write(&design).map_err(|e| bad(C, e.to_string()))?;
    let mut out = totals_in_hoop(d, &design);
    out["format"] = json!(format.extension());
    match path {
        Some(path) => {
            fileio::write_file(path, &bytes)?;
            out["path"] = json!(path);
        }
        None => out["dataBase64"] = json!(vectorcraft_format::base64_encode(&bytes)),
    }
    Ok(out)
}

/// One needle position of the stitch preview, in document units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewStitch {
    pub x: f64,
    pub y: f64,
    /// The needle goes down here (false: a jump, trim or colour change moves it here).
    pub sew: bool,
    /// Index into [`StitchPreview::threads`].
    pub thread: usize,
    /// Underlay: sewn first to hold the fabric, covered by the top stitches.
    pub underlay: bool,
    /// The thread is cut before the needle moves here.
    pub trim: bool,
}

/// What the machine will sew, for the canvas's stitch and realistic views.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StitchPreview {
    pub stitches: Vec<PreviewStitch>,
    /// sRGB of each colour block, in sewing order.
    pub threads: Vec<[u8; 3]>,
    /// Layers of thread across the design (View → Density Map), in millimetres: divide by
    /// [`MM_PER_PT`] for document units.
    pub density: review::DensityMap,
}

/// The stitch preview of every visible object, in document units (stitch plan applied, not
/// centred).
pub fn stitch_preview(d: &Document) -> StitchPreview {
    use roidery_formats::Command;
    let ps = PlanSettings { center: false, ..PlanSettings::default() };
    let objs: Vec<PlanObject> = sewn_parts(d, &all_objects(d)).into_iter().map(|o| o.obj).collect();
    let (design, kinds) = roidery_plan::plan_kinds(&objs, &ps, "");
    let mut thread = 0;
    let mut trim = false;
    let mut stitches = Vec::with_capacity(design.stitches.len());
    for (i, s) in design.stitches.iter().enumerate() {
        match s.command {
            Command::ColorChange => {
                thread += 1;
                trim = true;
            }
            Command::Trim => trim = true,
            Command::End => {}
            c => {
                let underlay = kinds.get(i) == Some(&roidery_stitch::StitchKind::Underlay);
                let sew = c == Command::Stitch;
                stitches.push(PreviewStitch { x: s.x / MM_PER_PT, y: s.y / MM_PER_PT, sew, thread, underlay, trim: std::mem::take(&mut trim) });
            }
        }
    }
    let density = review::density(&design, review::DENSITY_CELL_MM);
    StitchPreview { stitches, threads: design.threads.iter().map(|t| t.rgb).collect(), density }
}

/// A rail: points as [x, y], or as anchors {p, in?, out?} whose handles curve the rail.
fn rail_param(p: &Value, key: &str) -> Result<Vec<vectorcraft_geom::Anchor>> {
    const C: &str = "stitch.createSatinBlocks";
    let arr = p.get(key).and_then(Value::as_array).ok_or_else(|| bad(C, format!("`{key}` must be a list of [x, y] points")))?;
    let finite = |q: vectorcraft_geom::Point| q.x.is_finite() && q.y.is_finite();
    let pts: Vec<vectorcraft_geom::Anchor> = arr
        .iter()
        .filter_map(|v| match v {
            Value::Object(_) => serde_json::from_value::<vectorcraft_geom::Anchor>(v.clone()).ok(),
            _ => point_param(&json!({ "p": v }), "p").map(vectorcraft_geom::Anchor::corner),
        })
        .filter(|a| finite(a.p) && finite(a.h_in) && finite(a.h_out))
        .collect();
    if pts.len() != arr.len() || pts.is_empty() || pts.len() > 100_000 {
        return Err(bad(C, format!("`{key}` must hold 1 to 100000 points as [x, y] or {{p, in?, out?}}")));
    }
    Ok(pts)
}

fn create_satin_blocks(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.createSatinBlocks";
    let (left, right) = (rail_param(p, "left")?, rail_param(p, "right")?);
    let mut v = serde_json::to_value(StitchSettings::of(StitchType::SatinBlocks)).map_err(|e| bad(C, e.to_string()))?;
    if let Some(patch) = p.get("settings") {
        if !patch.is_object() {
            return Err(bad(C, "`settings` must be an object"));
        }
        merge(&mut v, patch);
        v["type"] = json!(StitchType::SatinBlocks.id());
    }
    let settings: StitchSettings = serde_json::from_value(v).map_err(|e| bad(C, format!("bad settings: {e}")))?;
    let stitch = serde_json::to_value(&settings).map_err(|e| bad(C, e.to_string()))?;
    let path = vectorcraft_geom::PathData::new(vec![vectorcraft_geom::SubPath::new(left, false), vectorcraft_geom::SubPath::new(right, false)]);
    let color = s.paint.stroke.color().or_else(|| s.paint.fill.color()).unwrap_or(vectorcraft_color::Color::BLACK);
    let look = s.new_art_look(Paint::None, Paint::solid(color), 1.0);
    let attrs = vectorcraft_doc::ObjectAttributes { stitch: Some(stitch), ..Default::default() };
    super::create::add_look_with(s, "Satin Blocks", super::create::path_kind(path, None), look, None, Some(Box::new(attrs)))
}
