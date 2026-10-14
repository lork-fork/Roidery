//! Roidery: lettering objects, in one of two kinds of font.
//!
//! - A built-in lettering font (Roidery Sans) draws each letter as centre-line strokes: the
//!   lettering is one path whose subpaths are the strokes, sewn as satin columns.
//! - A digitized embroidery font from the library ([`roidery_fonts`], Ink/Stitch's font format)
//!   was digitized by hand, letter by letter: the lettering is a group with a group (or path) per
//!   letter, holding the pieces its digitizer made, each sewn with the stitches the font gives it
//!   (satin columns, fills, running stitches, in the font's colours when it has several).
//!
//! Either way the text and its settings ([`roidery_stitch::Lettering`]) are kept on the object (in
//! the path's stitch settings, or as `{"lettering": …}` in the group's), so `stitch.setLettering`
//! can lay it out again, in the same font or another.

use std::sync::Arc;

use kurbo::{Affine, BezPath, Point, Rect};
use roidery_fonts::{Font, Placed, library};
use roidery_stitch::{Lettering, StitchSettings, StitchType, lettering};
use serde_json::{Value, json};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Document, LineCap, LineJoin, Node, NodeId, NodeKind, ObjectAttributes};
use vectorcraft_geom::{FillRule, PathData};

use super::parts::{self, Part, Slot, Slots, THREAD_MM};
use super::{MM_PER_PT, fill_rgb, merge, stroke_rgb};
use crate::Session;
use crate::cmd::{Result, bad, targets};

/// Whether lettering in `font` is digitized (a library font, not a built-in one).
fn digitized(font: &str) -> bool {
    lettering::font(font).is_none()
}

fn load(font: &str, c: &'static str) -> Result<Arc<Font>> {
    library::load(font).map_err(|e| bad(c, e))
}

/// A library font's id from its id or its name.
fn library_font(id: &str) -> Option<String> {
    if library::has(id) {
        return Some(id.to_string());
    }
    library::entries().into_iter().find(|e| e.info.name.eq_ignore_ascii_case(id)).map(|e| e.id)
}

/// Baseline to baseline as a multiple of the cap height, as the font sets its lines.
fn default_line_spacing(f: &Font) -> f64 {
    if f.cap > 1e-9 { (f.info.leading / f.cap).clamp(0.8, 5.0) } else { Lettering::default().line_spacing }
}

/// The size new lettering in a digitized font starts at (cap height, mm): the size it was
/// digitized at, or the nearest it sews well at (a font's "small" variant shares the large one's
/// letters and only sews well smaller).
fn natural_height(f: &Font) -> f64 {
    let h = usual_size(f.summary.cap_mm, f.size_range());
    if h.is_finite() && h > 0.0 { h.clamp(2.0, 300.0) } else { Lettering::default().height }
}

/// `cap_mm` brought into `(lo, hi)`.
fn usual_size(cap_mm: f64, (lo, hi): (f64, f64)) -> f64 {
    if lo.is_finite() && hi.is_finite() && lo <= hi { cap_mm.clamp(lo, hi) } else { cap_mm }
}

/// Apply the lettering keys of `p` (`text`, `height`, `spacing`, `lineSpacing`, `font`, `path`) to
/// `l`. A new font sets its own lines' spacing unless `lineSpacing` is given; new lettering in a
/// digitized font is the size the font was made at unless `height` is given.
fn patch(l: &Lettering, p: &Value, c: &'static str, new: bool) -> Result<Lettering> {
    let mut v = serde_json::to_value(l).map_err(|e| bad(c, e.to_string()))?;
    for key in ["text", "height", "spacing", "lineSpacing", "font", "path"] {
        if let Some(x) = p.get(key) {
            merge(&mut v[key], x);
        }
    }
    let mut out: Lettering = serde_json::from_value(v).map_err(|e| bad(c, format!("bad lettering: {e}")))?;
    if let Some(f) = lettering::font(&out.font) {
        out.font = f.0.to_string();
    } else {
        out.font = library_font(&out.font).ok_or_else(|| bad(c, format!("unknown lettering font `{}`", out.font)))?;
    }
    if out.font != l.font || new {
        if digitized(&out.font) {
            let f = load(&out.font, c)?;
            if p.get("lineSpacing").is_none() {
                out.line_spacing = default_line_spacing(&f);
            }
            if new && p.get("height").is_none() {
                out.height = natural_height(&f);
            }
        } else if p.get("lineSpacing").is_none() {
            out.line_spacing = Lettering::default().line_spacing;
        }
    }
    Ok(out.sanitized())
}

fn missing_json(missing: &[char]) -> Value {
    json!(missing.iter().map(char::to_string).collect::<String>())
}

fn add_missing(all: &mut Vec<char>, more: Vec<char>) {
    for c in more {
        if !all.contains(&c) {
            all.push(c);
        }
    }
}

fn name_of(l: &Lettering) -> String {
    l.text.lines().next().unwrap_or_default().chars().take(40).collect()
}

/// Built-in font: `l` laid out in document units with its first baseline starting at `origin`
/// (pt), and the characters the font doesn't have.
fn path_pt(l: &Lettering, origin: Point) -> (BezPath, Vec<char>) {
    let (mut bp, missing) = lettering::layout(l);
    bp.apply_affine(Affine::translate(origin.to_vec2()) * Affine::scale(1.0 / MM_PER_PT));
    (bp, missing)
}

/// Digitized font: `l`'s letters in document units with the first baseline starting at `origin`.
fn pieces_pt(f: &Font, l: &Lettering, origin: Point) -> (Vec<Placed>, Vec<char>) {
    let (mut placed, missing) = roidery_fonts::layout(f, l);
    let to_pt = Affine::translate(origin.to_vec2()) * Affine::scale(1.0 / MM_PER_PT);
    for p in placed.iter_mut().flat_map(|g| g.pieces.iter_mut()) {
        p.path.apply_affine(to_pt);
    }
    placed.retain(|g| !g.pieces.is_empty());
    (placed, missing)
}

/// The bounds of paths as the document measures them.
fn bounds_of<'a>(paths: impl IntoIterator<Item = &'a BezPath>) -> Option<Rect> {
    paths.into_iter().fold(None, |acc, p| vectorcraft_geom::union_opt(acc, PathData::from_bezpath(p).bounds()))
}

/// Where `l` reaches when its first baseline starts at the origin, pt.
fn laid_bounds(l: &Lettering) -> Option<Rect> {
    if digitized(&l.font) {
        let f = library::load(&l.font).ok()?;
        let (placed, _) = pieces_pt(&f, l, Point::ZERO);
        bounds_of(placed.iter().flat_map(|g| g.pieces.iter().map(|p| &p.path)))
    } else {
        let (bp, _) = path_pt(l, Point::ZERO);
        bounds_of([&bp])
    }
}

/// The lettering object `n` is, if it is one.
fn lettering_of(n: &Node) -> Option<Lettering> {
    match &n.kind {
        NodeKind::Group { .. } => {
            let v = n.attrs.as_ref()?.stitch.as_ref()?.get("lettering")?;
            serde_json::from_value::<Lettering>(v.clone()).ok().map(|l| l.sanitized())
        }
        NodeKind::Path { .. } => match parts::slots(n).get(Part::Stroke) {
            Slot::Set(s) => s.lettering.clone(),
            Slot::Auto => None,
        },
        _ => None,
    }
}

fn find(n: &Node, out: &mut Vec<NodeId>) {
    if lettering_of(n).is_some() {
        out.push(n.id);
        return;
    }
    n.children().into_iter().flatten().for_each(|c| find(c, out));
}

/// The lettering objects among `ids`: each one, the lettering a letter or piece belongs to, and
/// those inside groups and layers.
fn lettering_targets(d: &Document, ids: &[NodeId]) -> Vec<NodeId> {
    let mut out: Vec<NodeId> = vec![];
    for id in ids {
        let mut at = Some(*id);
        let mut found = None;
        while let Some(a) = at {
            if d.node(a).is_some_and(|n| lettering_of(n).is_some()) {
                found = Some(a);
                break;
            }
            at = d.parent_of(a);
        }
        let mut v = vec![];
        match found {
            Some(l) => v.push(l),
            None => {
                if let Some(n) = d.node(*id) {
                    find(n, &mut v);
                }
            }
        }
        for l in v {
            if !out.contains(&l) {
                out.push(l);
            }
        }
    }
    out
}

/// Every lettering object in the document.
fn every_lettering(d: &Document) -> Vec<NodeId> {
    let mut out = vec![];
    d.layers.iter().for_each(|l| find(l, &mut out));
    out
}

fn solid(rgb: [u8; 3]) -> Paint {
    Paint::solid(Color::rgb8(rgb[0], rgb[1], rgb[2]))
}

fn round(a: &mut Appearance) {
    if let Some(st) = a.stroke_mut() {
        st.cap = LineCap::Round;
        st.join = LineJoin::Round;
    }
}

/// One piece of a digitized letter: a path sewing what the font says, painted in its thread
/// (`color` instead of the font's when given).
fn piece_node(d: &mut Document, p: &roidery_fonts::PlacedPiece, color: Option<[u8; 3]>) -> Node {
    let fill = p.fill.is_some();
    let rule = if fill { FillRule::EvenOdd } else { FillRule::NonZero };
    let mut n = Node::new(d.alloc_id(), NodeKind::Path { path: PathData::from_bezpath(&p.path), rule, live: None, clipping: false, guide: false });
    let paint = |set: bool, rgb: Option<[u8; 3]>| if set { solid(color.or(rgb).unwrap_or([0, 0, 0])) } else { Paint::None };
    n.appearance = Appearance::basic(paint(fill, p.fill_rgb), paint(p.stroke.is_some(), p.stroke_rgb), THREAD_MM / MM_PER_PT);
    round(&mut n.appearance);
    let slot = |s: &Option<StitchSettings>| s.clone().map_or_else(|| Slot::Set(StitchSettings::of(StitchType::None)), Slot::Set);
    parts::write(&mut n, &Slots::only(slot(&p.fill), slot(&p.stroke)), true);
    n
}

/// Lettering in a digitized font: a group `id` with a group per letter (a path when the letter
/// is one piece), named by the text.
fn build_group(d: &mut Document, id: NodeId, l: &Lettering, placed: Vec<Placed>, color: Option<[u8; 3]>) -> Node {
    let mut letters = vec![];
    for g in placed {
        let mut kids: Vec<Node> = g.pieces.iter().map(|p| piece_node(d, p, color)).collect();
        let letter = if kids.len() == 1 {
            kids.pop()
        } else {
            let gid = d.alloc_id();
            Some(Node::new(gid, NodeKind::Group { children: kids.into_iter().map(Arc::new).collect(), clip: false }))
        };
        if let Some(mut n) = letter {
            n.name = Some(g.name.clone());
            letters.push(Arc::new(n));
        }
    }
    let mut n = Node::new(id, NodeKind::Group { children: letters, clip: false });
    n.name = Some(name_of(l));
    n.attrs = Some(Box::new(ObjectAttributes { stitch: Some(json!({ "lettering": l })), ..Default::default() }));
    n
}

/// The stitch settings of lettering in the built-in font.
fn path_settings(l: &Lettering, c: &'static str) -> Result<Value> {
    let mut settings = StitchSettings::of(StitchType::Satin);
    settings.satin.width = l.column_width();
    settings.lettering = Some(l.clone());
    serde_json::to_value(&settings).map_err(|e| bad(c, e.to_string()))
}

/// Lettering in the built-in font as path `id`: the columns' strokes in `color`, as wide as they
/// sew, with round ends.
fn build_path(id: NodeId, l: &Lettering, path: &BezPath, color: [u8; 3], c: &'static str) -> Result<Node> {
    let mut n = Node::new(id, super::super::create::path_kind(PathData::from_bezpath(path), None));
    n.appearance = Appearance::basic(Paint::None, solid(color), l.column_width() / MM_PER_PT);
    round(&mut n.appearance);
    n.attrs = Some(Box::new(ObjectAttributes { stitch: Some(path_settings(l, c)?), ..Default::default() }));
    n.name = Some(name_of(l));
    Ok(n)
}

/// The thread colour new lettering sews in: the current stroke colour, else the fill colour.
fn color_of(s: &Session) -> Color {
    s.paint.stroke.color().or_else(|| s.paint.fill.color()).unwrap_or(Color::BLACK)
}

fn rgb(c: Color) -> [u8; 3] {
    let [r, g, b, _] = c.to_rgba8(1.0);
    [r, g, b]
}

/// The first thread colour sewn in `n`.
fn color_in(n: &Node) -> Option<[u8; 3]> {
    match &n.kind {
        NodeKind::Path { .. } => stroke_rgb(n).or_else(|| fill_rgb(n)),
        _ => n.children().into_iter().flatten().find_map(|c| color_in(c)),
    }
}

/// `stitch.createLettering`.
pub(super) fn create(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.createLettering";
    let l = patch(&Lettering::default(), p, C, true)?;
    if l.text.trim().is_empty() {
        return Err(bad(C, "`text` is empty"));
    }
    let font = if digitized(&l.font) { Some(load(&l.font, C)?) } else { None };
    let size = laid_bounds(&l).unwrap_or_default();
    // Default: centred on the first artboard.
    let d = &s.doc()?.doc;
    let centre = d.artboards.first().map_or(Point::ZERO, |a| a.rect.center());
    let origin = Point::new(
        p.get("x").and_then(Value::as_f64).unwrap_or(centre.x - size.center().x),
        p.get("y").and_then(Value::as_f64).unwrap_or(centre.y - size.center().y),
    );
    if !origin.x.is_finite() || !origin.y.is_finite() {
        return Err(bad(C, "`x` and `y` must be numbers"));
    }
    let Some(f) = font else { return create_path(s, &l, origin) };
    let (placed, missing) = pieces_pt(&f, &l, origin);
    if placed.is_empty() {
        return Err(bad(C, format!("the font has none of these characters: {}", l.text)));
    }
    // A font digitized in one colour sews in the current colour; a multicolour one in its own.
    let color = (!f.summary.multicolor).then(|| rgb(color_of(s)));
    let parent = s.doc()?.insertion_parent();
    let id = s.edit("Lettering", |d, sel| {
        let id = d.alloc_id();
        let n = build_group(d, id, &l, placed, color);
        d.insert(parent, usize::MAX, n)?;
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0, "missing": missing_json(&missing) }))
}

/// `stitch.createLettering` in the built-in font: one path, drawn with the new-art look.
fn create_path(s: &mut Session, l: &Lettering, origin: Point) -> Result<Value> {
    const C: &str = "stitch.createLettering";
    let (path, missing) = path_pt(l, origin);
    if path.elements().is_empty() {
        return Err(bad(C, format!("the font has none of these characters: {}", l.text)));
    }
    // The art shows the columns: a stroke as wide as the satin, with round ends.
    let mut look = s.new_art_look(Paint::None, Paint::solid(color_of(s)), l.column_width() / MM_PER_PT);
    round(&mut look.appearance);
    let attrs = ObjectAttributes { stitch: Some(path_settings(l, C)?), ..Default::default() };
    let mut r = super::super::create::add_look_with(
        s,
        "Lettering",
        super::super::create::path_kind(PathData::from_bezpath(&path), None),
        look,
        Some(name_of(l)),
        Some(Box::new(attrs)),
    )?;
    r["missing"] = missing_json(&missing);
    Ok(r)
}

/// Where the first baseline of lettering object `id` starts now (it may have been moved since
/// it was laid out), and its lettering.
fn placed(d: &Document, id: NodeId) -> Option<(Point, Lettering)> {
    let n = d.node(id)?;
    let l = lettering_of(n)?;
    let now = n.geometric_bounds()?;
    let then = laid_bounds(&l)?;
    Some((Point::new(now.x0 - then.x0, now.y0 - then.y0), l))
}

/// `stitch.setLettering`.
pub(super) fn set(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.setLettering";
    let sel = targets(s, p)?;
    let d = &s.doc()?.doc;
    let ids = if sel.is_empty() { every_lettering(d) } else { lettering_targets(d, &sel) };
    if ids.is_empty() && !sel.is_empty() {
        return Err(bad(C, "select a lettering object"));
    }
    relayout(s, &ids, C, "Lettering", None, |_, old| patch(old, p, C, false))
}

/// `stitch.mapLettering`: lettering follows an open path, which goes.
pub(super) fn map(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.mapLettering";
    let sel = targets(s, p)?;
    let keep = p.get("keep").and_then(Value::as_bool).unwrap_or(false);
    let d = &s.doc()?.doc;
    let guide = match p.get("path").and_then(Value::as_u64) {
        Some(g) => NodeId(g),
        None => super::guide::open_path(d, &sel).ok_or_else(|| bad(C, "select the lettering and one open path for it to follow"))?,
    };
    let line: Vec<roidery_stitch::Point> =
        d.node(guide).and_then(super::mm_polylines).unwrap_or_default().into_iter().next().map(|l| l.points).unwrap_or_default();
    if line.len() < 2 {
        return Err(bad(C, "the path must be a line with at least two points"));
    }
    let picked: Vec<NodeId> = sel.iter().copied().filter(|id| *id != guide).collect();
    let ids = lettering_targets(d, &picked);
    if ids.is_empty() {
        return Err(bad(C, "select a lettering object with the path"));
    }
    relayout(s, &ids, C, "Map to Path", (!keep).then_some(guide), |origin, old| {
        // The path in mm from where the first baseline starts.
        let o = (origin.x * MM_PER_PT, origin.y * MM_PER_PT);
        let path = line.iter().map(|q| [q.x - o.0, q.y - o.1]).collect();
        Ok(Lettering { path: Some(path), ..old.clone() }.sanitized())
    })
}

/// How one lettering object changes.
enum Change {
    /// The built-in font's path, edited in place: its new path, stitch settings, stroke weight
    /// (when the column width follows the height) and name.
    Path(NodeId, PathData, Value, Option<f64>, String),
    /// A new object in its place.
    Replace(NodeId, Replacement),
}

enum Replacement {
    Group(Lettering, Vec<Placed>, Option<[u8; 3]>),
    Path(Lettering, BezPath, [u8; 3]),
}

/// Put `n` where `id` is, keeping its visibility, lock and transparency.
fn replace(d: &mut Document, id: NodeId, mut n: Node) -> Result<()> {
    let (parent, index, _) = d.position(id).ok_or(crate::EngineError::NoNode(id))?;
    let old = d.remove(id)?;
    n.visible = old.visible;
    n.locked = old.locked;
    n.opacity = old.opacity;
    n.blend = old.blend;
    d.insert(parent, index, n)?;
    Ok(())
}

/// Lay the lettering objects `ids` out again with what `make` gives for each (from where its
/// first baseline starts now and its lettering), as one undo step that also removes `remove`.
fn relayout(
    s: &mut Session,
    ids: &[NodeId],
    c: &'static str,
    label: &str,
    remove: Option<NodeId>,
    mut make: impl FnMut(Point, &Lettering) -> Result<Lettering>,
) -> Result<Value> {
    let d = &s.doc()?.doc;
    let mut todo = vec![];
    let mut missing: Vec<char> = vec![];
    for &id in ids {
        let Some((origin, old)) = placed(d, id) else { continue };
        let Some(n) = d.node(id) else { continue };
        let new = make(origin, &old)?;
        if new.text.trim().is_empty() {
            return Err(bad(c, "`text` is empty"));
        }
        if new == old {
            continue;
        }
        let color = color_in(n).unwrap_or([0, 0, 0]);
        if digitized(&new.font) {
            let f = load(&new.font, c)?;
            let (placed, miss) = pieces_pt(&f, &new, origin);
            add_missing(&mut missing, miss);
            // A multicolour font sews in its own colours; one colour keeps the lettering's.
            let color = (!f.summary.multicolor).then_some(color);
            todo.push(Change::Replace(id, Replacement::Group(new, placed, color)));
            continue;
        }
        let (path, miss) = path_pt(&new, origin);
        add_missing(&mut missing, miss);
        if digitized(&old.font) {
            todo.push(Change::Replace(id, Replacement::Path(new, path, color)));
            continue;
        }
        let Slot::Set(mut st) = parts::slots(n).get(Part::Stroke).clone() else { continue };
        // A column width that followed the old height follows the new one.
        let follows = (st.satin.width - old.column_width()).abs() < 1e-6;
        if follows {
            st.satin.width = new.column_width();
        }
        st.lettering = Some(new.clone());
        let v = serde_json::to_value(&st).map_err(|e| bad(c, e.to_string()))?;
        todo.push(Change::Path(id, PathData::from_bezpath(&path), v, follows.then(|| new.column_width() / MM_PER_PT), name_of(&new)));
    }
    if todo.is_empty() {
        return Ok(json!({ "changed": 0, "missing": missing_json(&missing) }));
    }
    let kept: Vec<NodeId> = todo
        .iter()
        .map(|t| match t {
            Change::Path(id, ..) | Change::Replace(id, _) => *id,
        })
        .collect();
    let replaced = todo.iter().any(|t| matches!(t, Change::Replace(..)));
    let changed = s.edit(label, |d, selection| {
        let mut count = 0;
        for t in todo {
            match t {
                Change::Path(id, path, v, width, name) => {
                    let Some(n) = d.node_mut(id) else { continue };
                    if let NodeKind::Path { path: p, .. } = &mut n.kind {
                        *p = path;
                    }
                    n.edit_attrs(|a| a.stitch = Some(v));
                    if let (Some(w), Some(st)) = (width, n.appearance.stroke_mut()) {
                        st.width = w;
                    }
                    n.name = Some(name);
                }
                Change::Replace(id, Replacement::Group(l, placed, color)) => {
                    let n = build_group(d, id, &l, placed, color);
                    replace(d, id, n)?;
                }
                Change::Replace(id, Replacement::Path(l, path, color)) => {
                    let n = build_path(id, &l, &path, color, c)?;
                    replace(d, id, n)?;
                }
            }
            count += 1;
        }
        if let Some(r) = remove
            && count > 0
        {
            d.remove(r)?;
        }
        if remove.is_some() || replaced {
            selection.set(kept.clone());
        }
        Ok(count)
    })?;
    Ok(json!({ "changed": changed, "missing": missing_json(&missing) }))
}

/// `stitch.fonts`: the lettering fonts, built-in and digitized.
pub(super) fn fonts(_s: &mut Session, p: &Value) -> Result<Value> {
    if p.get("rescan").and_then(Value::as_bool).unwrap_or(false) {
        library::rescan();
    }
    let mut out: Vec<Value> = lettering::FONTS
        .iter()
        .map(|(id, name, _)| {
            json!({
                "id": id, "name": name, "digitized": false,
                "description": "Roidery's own satin lettering: every stroke sews as a satin column, at any size.",
                "satin": true, "fill": false, "run": false, "multicolor": false, "sizeMin": 4.0, "sizeMax": 300.0,
            })
        })
        .collect();
    for e in library::entries() {
        let mut v = json!({
            "id": e.id, "name": e.info.name, "digitized": true, "description": e.info.description, "keywords": e.info.keywords,
            "license": e.info.license, "user": e.user, "folder": e.dir.to_string_lossy(),
        });
        if let Some(p) = &e.preview {
            v["preview"] = json!(p.to_string_lossy());
        }
        if let Some(sm) = &e.summary {
            v["satin"] = json!(sm.satin);
            v["fill"] = json!(sm.fill);
            v["run"] = json!(sm.run);
            v["multicolor"] = json!(sm.multicolor);
            v["glyphs"] = json!(sm.glyphs);
            if sm.cap_mm > 0.0 {
                let range = (sm.cap_mm * e.info.min_scale, sm.cap_mm * e.info.max_scale);
                v["size"] = json!(usual_size(sm.cap_mm, range));
                v["sizeMin"] = json!(range.0);
                v["sizeMax"] = json!(range.1);
            }
        }
        out.push(v);
    }
    Ok(json!({
        "fonts": out,
        "userFolder": library::user_folder().map(|f| f.to_string_lossy().into_owned()),
    }))
}

/// `stitch.info`'s lettering rows: the lettering objects among `sel` (a letter or piece stands
/// for its lettering), with their font's name and kind and the sizes it sews well at.
pub(super) fn info_rows(d: &Document, sel: &[NodeId]) -> Vec<Value> {
    lettering_targets(d, sel)
        .into_iter()
        .filter_map(|id| {
            let l = lettering_of(d.node(id)?)?;
            let mut v = json!({ "id": id, "lettering": l, "digitized": digitized(&l.font) });
            match lettering::font(&l.font) {
                Some((_, name, _)) => v["fontName"] = json!(name),
                None => {
                    let e = library::entry(&l.font);
                    v["fontName"] = json!(e.as_ref().map_or(l.font.as_str(), |e| e.info.name.as_str()));
                    match library::load(&l.font) {
                        Ok(f) => {
                            let (lo, hi) = f.size_range();
                            v["sizeMin"] = json!(lo);
                            v["sizeMax"] = json!(hi);
                            v["multicolor"] = json!(f.summary.multicolor);
                        }
                        Err(e) => v["error"] = json!(e),
                    }
                }
            }
            Some(v)
        })
        .collect()
}

#[cfg(test)]
fn test_font() -> Font {
    // A satin H (two rails and a rung) and a filled O with a hole, in two colours.
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
        xmlns:inkstitch="http://inkstitch.org/namespace">
      <g inkscape:label="GlyphLayer-H">
        <path d="M 0,0 L 0,-100 M 12,0 L 12,-100 M -2,-50 L 14,-50" style="fill:none;stroke:#000000" inkstitch:satin_column="True"/>
        <path d="M 48,0 L 48,-100 M 60,0 L 60,-100" style="fill:none;stroke:#000000" inkstitch:satin_column="True"/>
        <path d="M 6,-56 L 54,-56 L 54,-44 L 6,-44 Z" style="fill:#000000"/>
      </g>
      <g inkscape:label="GlyphLayer-O">
        <path d="M 0,-50 A 40,50 0 1 0 80,-50 A 40,50 0 1 0 0,-50 Z M 15,-50 A 25,35 0 1 1 65,-50 A 25,35 0 1 1 15,-50 Z" style="fill:#cc0000;fill-rule:evenodd"/>
        <path d="M 40,-100 L 40,0" style="fill:none;stroke:#0000cc"/>
      </g>
    </svg>"##;
    let info = roidery_fonts::FontInfo {
        name: "Test Script".into(),
        leading: 150.0,
        min_scale: 0.5,
        max_scale: 2.0,
        word_spacing: 30.0,
        ..Default::default()
    };
    Font::from_parts("test_script", info, &[SVG.to_string()]).expect("the test font loads")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        library::insert(test_font());
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
        s.execute("paint.setStroke", &json!({"color": "#336699"})).unwrap();
        s
    }

    fn run(s: &mut Session, id: &str, p: Value) -> Value {
        s.execute(id, &p).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
    }

    fn node(s: &Session, id: u64) -> Node {
        s.doc().unwrap().doc.node(NodeId(id)).unwrap().clone()
    }

    #[test]
    fn digitized_fonts_are_listed_with_their_sizes() {
        let mut s = session();
        let f = run(&mut s, "stitch.fonts", json!({}));
        let fonts = f["fonts"].as_array().unwrap();
        assert_eq!((fonts[0]["id"].clone(), fonts[0]["digitized"].clone()), (json!("sans"), json!(false)));
        let t = fonts.iter().find(|f| f["id"] == "test_script").unwrap();
        assert_eq!(
            (t["digitized"].clone(), t["satin"].clone(), t["fill"].clone(), t["multicolor"].clone()),
            (json!(true), json!(true), json!(true), json!(true))
        );
        // Cap height 100 px = 26.46 mm, sewn well from half to twice that.
        let size = t["size"].as_f64().unwrap();
        assert!((size - 100.0 * 25.4 / 96.0).abs() < 1e-6, "{size}");
        assert!((t["sizeMin"].as_f64().unwrap() - size / 2.0).abs() < 1e-6);
    }

    #[test]
    fn digitized_lettering_is_a_group_of_letters_sewn_as_the_font_says() {
        let mut s = session();
        let r = run(&mut s, "stitch.createLettering", json!({"text": "HOx", "font": "Test Script", "height": 20, "x": 20, "y": 100}));
        assert_eq!(r["missing"], "x");
        let id = r["id"].as_u64().unwrap();
        let g = node(&s, id);
        let NodeKind::Group { children, .. } = &g.kind else { panic!("not a group") };
        assert_eq!(children.iter().map(|c| c.name.clone().unwrap_or_default()).collect::<Vec<_>>(), ["H", "O"]);
        assert_eq!(g.name.as_deref(), Some("HOx"));
        let b = g.geometric_bounds().unwrap();
        assert!((b.y0 - (100.0 - 20.0 / MM_PER_PT)).abs() < 0.5, "cap height above the baseline: {b:?}");
        assert!((b.y1 - 100.0).abs() < 0.5, "{b:?}");

        // Each piece sews what the font says, in the font's own colours (it has two).
        let i = run(&mut s, "stitch.info", json!({"ids": [id]}));
        let types: Vec<String> = i["objects"].as_array().unwrap().iter().map(|o| o["type"].as_str().unwrap().to_string()).collect();
        assert_eq!(types, ["satinBlocks", "satinBlocks", "tatami", "tatami", "run"], "{types:?}");
        assert!(i["objects"].as_array().unwrap().iter().all(|o| o["stitches"].as_u64().unwrap() > 5), "{i}");
        assert_eq!((i["objects"][3]["thread"].clone(), i["objects"][4]["thread"].clone()), (json!([204, 0, 0]), json!([0, 0, 204])));
        let l = &i["lettering"][0];
        assert_eq!((l["id"].as_u64(), l["digitized"].clone(), l["fontName"].clone()), (Some(id), json!(true), json!("Test Script")));
        assert_eq!(l["lettering"]["lineSpacing"], 1.5, "the font's leading");

        // Moved, then retyped with only a letter picked: the lettering changes where it is.
        run(&mut s, "object.move", json!({"dx": 30, "dy": 0, "ids": [id]}));
        let letter = children[1].id.0;
        let r = run(&mut s, "stitch.setLettering", json!({"text": "OH", "ids": [letter]}));
        assert_eq!(r["changed"], 1);
        let g = node(&s, id);
        let b2 = g.geometric_bounds().unwrap();
        // The first baseline still starts where it was moved to (x 20 + 30; O's leftmost point is on it).
        assert!((b2.x0 - 50.0).abs() < 0.01 && (b2.y1 - 100.0).abs() < 0.5, "{b2:?} vs {b:?}");
        assert_eq!(g.name.as_deref(), Some("OH"));

        // Into the built-in font and back: the same object, one undo step each.
        run(&mut s, "stitch.setLettering", json!({"font": "sans", "ids": [id]}));
        let p = node(&s, id);
        assert!(matches!(p.kind, NodeKind::Path { .. }));
        let i = run(&mut s, "stitch.info", json!({"ids": [id]}));
        assert_eq!((i["objects"][0]["type"].clone(), i["lettering"][0]["lettering"]["lineSpacing"].clone()), (json!("satin"), json!(1.6)));
        assert_eq!(i["objects"][0]["thread"], json!([204, 0, 0]), "the first colour it sewed in");
        run(&mut s, "edit.undo", json!({}));
        assert!(matches!(node(&s, id).kind, NodeKind::Group { .. }));
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(node(&s, id).name.as_deref(), Some("HOx"));
        assert!(s.execute("stitch.setLettering", &json!({"font": "nope", "ids": [id]})).is_err());
    }

    #[test]
    fn built_in_lettering_switches_to_a_digitized_font() {
        let mut s = session();
        let id = run(&mut s, "stitch.createLettering", json!({"text": "HO", "height": 10, "x": 20, "y": 100}))["id"].as_u64().unwrap();
        let r = run(&mut s, "stitch.setLettering", json!({"font": "test_script"}));
        assert_eq!(r["changed"], 1);
        let g = node(&s, id);
        assert!(matches!(g.kind, NodeKind::Group { .. }));
        let b = g.geometric_bounds().unwrap();
        assert!((b.height() - 10.0 / MM_PER_PT).abs() < 0.5 && (b.y1 - 100.0).abs() < 0.5, "same size and baseline: {b:?}");
        // Mapped to a path, a digitized lettering's letters stand on it.
        let line = run(&mut s, "path.create", json!({"d": "M 20 100 L 20 300"}))["id"].as_u64().unwrap();
        let r = run(&mut s, "stitch.mapLettering", json!({"ids": [id, line]}));
        assert_eq!(r["changed"], 1);
        let b = node(&s, id).geometric_bounds().unwrap();
        assert!(b.height() > b.width(), "runs down the page: {b:?}");
    }
}
