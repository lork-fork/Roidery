//! Type menu: Create Outlines, and the Character/Paragraph panel setters (`text.setText`,
//! `text.setStyle`).

use std::sync::Arc;

use serde_json::{Value, json};
use vectorcraft_doc::{Document, Justify, Node, NodeId, NodeKind, TextObject};
use vectorcraft_geom::{Affine, PathData, Vec2};

use super::edit::selected_roots;
use super::pathops::{len_param, num_param, shape_node};
use super::*;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("type.orientation.vertical", "Vertical", ["Type", "Orientation"], None, "{ids?|id?} set vertical writing", has_selection, |s, p| {
            orientation(s, p, true)
        }),
        cmd!(
            "type.orientation.horizontal",
            "Horizontal",
            ["Type", "Orientation"],
            None,
            "{ids?|id?} set horizontal writing",
            has_selection,
            |s, p| orientation(s, p, false)
        ),
        cmd!(
            "type.createOutlines",
            "Create Outlines",
            ["Type"],
            Some("Cmd+Shift+O"),
            "{} convert the selected text to groups of compound paths (one per glyph) → {ids}",
            has_selection,
            create_outlines
        ),
        cmd!(
            "text.setText",
            "Set Text",
            [],
            None,
            "{id?|ids?, text} replace the contents of text objects (keeps the first run's style)",
            has_doc,
            set_text
        ),
        cmd!(
            "text.areaOptions",
            "Area Type Options…",
            ["Type"],
            None,
            "{ids?, width?: pt, height?: pt, rows?, columns?, gutter?: pt, inset?: pt, firstBaseline?: ascent|capHeight|xHeight|leading|fixed, firstBaselineMin?: pt} set the selected area type's options; width and height size the type area from its top-left corner, along the type's own axes, and the text reflows at its size (none given: query) → the first object's options",
            has_selection,
            area_options
        ),
        cmd!(
            "text.reshapeArea",
            "Reshape Type Area",
            [],
            None,
            "{id, anchors: [[subpath, anchor]…], dx, dy} move anchors of area type's frame (the type area) by dx, dy points, with their handles; the text reflows at its size, through its thread too (what Direct Selection does dragging a frame corner or edge)",
            has_doc,
            reshape_area
        ),
        cmd!(
            "text.fitHeadline",
            "Fit Headline",
            ["Type"],
            None,
            "{ids?} track the first line of area type so it fills the frame width → {tracking}",
            has_selection,
            fit_headline
        ),
        cmd!(
            "text.setStyle",
            "Character",
            [],
            None,
            "{ids?|id?, font?, style?, size?: pt, leading?: pt|\"auto\", tracking?: 1/1000 em, justify?: \"auto\" (the start of each paragraph's direction)|\"left\"|\"center\"|\"right\"|\"justifyAll\", fill?: colour, features?: [\"dlig\", \"-liga\", …] OpenType}",
            has_doc,
            set_style
        ),
    ]
}

fn orientation(s: &mut Session, p: &Value, vertical: bool) -> Result<Value> {
    let ids = text_targets(s, p, "Text Orientation")?;
    s.edit("Text Orientation", |d, _| {
        for id in &ids {
            let Some(NodeKind::Text(t)) = d.node_mut(*id).map(|n| &mut n.kind) else {
                return Err(EngineError::NoNode(*id));
            };
            t.vertical = vertical;
            refresh_bounds(t);
        }
        Ok(())
    })?;
    Ok(json!({"vertical": vertical, "ids": ids.iter().map(|id| id.0).collect::<Vec<_>>()}))
}

/// Recompute the layout bounds cache after a text edit.
pub(crate) fn refresh_bounds(t: &mut TextObject) {
    let lay = vectorcraft_text::layout(vectorcraft_text::FontDb::global(), t);
    t.cached_bounds = Some(lay.bounds);
}

/// Text objects among `ids` and their descendants.
fn text_ids(d: &Document, ids: &[NodeId]) -> Vec<NodeId> {
    let mut out = vec![];
    for id in ids {
        if let Some(n) = d.node(*id) {
            n.walk(&mut |c| {
                if matches!(c.kind, NodeKind::Text(_)) && !out.contains(&c.id) {
                    out.push(c.id);
                }
            });
        }
    }
    out
}

fn create_outlines(s: &mut Session, _: &Value) -> Result<Value> {
    let roots = selected_roots(s)?;
    let ids = s.edit("Create Outlines", |d, sel| {
        let texts = text_ids(d, &roots);
        if texts.is_empty() {
            return Err(EngineError::Other("Create Outlines: select text objects".into()));
        }
        let mut new_sel: Vec<NodeId> = roots.iter().copied().filter(|r| !texts.contains(r)).collect();
        let mut out = vec![];
        for tid in texts {
            let Some(n) = d.node(tid).cloned() else { continue };
            let NodeKind::Text(t) = &n.kind else { continue };
            let lay = vectorcraft_text::layout(vectorcraft_text::FontDb::global(), t);
            let mut children = vec![];
            for g in &lay.glyphs {
                let path = PathData::from_bezpath(&g.outline).transformed(t.xf);
                if path.is_empty() {
                    continue;
                }
                let st = t.runs.get(g.run).map(|r| r.style.clone()).unwrap_or_else(|| t.first_style());
                let mut node = shape_node(d, path, None);
                node.appearance = st.appearance();
                children.push(Arc::new(node));
            }
            let (par, idx, _) = d.position(tid).ok_or(EngineError::NoNode(tid))?;
            d.remove(tid)?;
            if children.is_empty() {
                continue;
            }
            let gid = d.alloc_id();
            let mut g = Node::group(gid, children);
            g.opacity = n.opacity;
            g.blend = n.blend;
            g.name = n.name.clone();
            d.insert(par, idx, g)?;
            out.push(gid);
            if roots.contains(&tid) {
                new_sel.push(gid);
            }
        }
        sel.set(new_sel);
        Ok(out)
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

pub(crate) fn text_targets(s: &Session, p: &Value, cmd: &str) -> Result<Vec<NodeId>> {
    let ids = targets(s, p)?;
    let t = text_ids(&s.doc()?.doc, &ids);
    if t.is_empty() {
        return Err(bad(cmd, "no text objects selected"));
    }
    Ok(t)
}

fn set_text(s: &mut Session, p: &Value) -> Result<Value> {
    let text = str_param(p, "text").ok_or_else(|| bad("text.setText", "missing `text`"))?.to_string();
    let ids = text_targets(s, p, "text.setText")?;
    s.edit("Typing", |d, _| {
        for id in &ids {
            let Some(NodeKind::Text(t)) = d.node_mut(*id).map(|n| &mut n.kind) else { continue };
            let style = t.first_style();
            t.runs = vec![vectorcraft_doc::TextRun { text: text.clone(), style }];
            refresh_bounds(t);
        }
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

fn justify_param(v: &str) -> Option<Justify> {
    Some(match v.to_ascii_lowercase().as_str() {
        "auto" => Justify::Auto,
        "left" => Justify::Left,
        "center" => Justify::Center,
        "right" => Justify::Right,
        "justifyleft" => Justify::JustifyLeft,
        "justifycenter" => Justify::JustifyCenter,
        "justifyright" => Justify::JustifyRight,
        "justifyall" | "justify" => Justify::JustifyAll,
        _ => return None,
    })
}

fn set_style(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "text.setStyle";
    let font = str_param(p, "font").map(str::to_string);
    let style = str_param(p, "style").map(str::to_string);
    let size = len_param(p, "size");
    if size.is_some_and(|v| v <= 0.0) {
        return Err(bad(C, "size must be positive"));
    }
    let leading = match p.get("leading") {
        None | Some(Value::Null) => None,
        Some(Value::String(a)) if a.eq_ignore_ascii_case("auto") => Some(None),
        Some(_) => Some(Some(len_param(p, "leading").ok_or_else(|| bad(C, "leading must be a number or \"auto\""))?.clamp(0.1, 5000.0))),
    };
    let tracking = num_param(p, "tracking");
    let justify = match str_param(p, "justify") {
        Some(j) => Some(justify_param(j).ok_or_else(|| bad(C, "justify must be auto|left|center|right|justifyAll"))?),
        None => None,
    };
    let fill = match p.get("fill") {
        None | Some(Value::Null) => None,
        Some(Value::String(n)) if n.eq_ignore_ascii_case("none") => Some(vectorcraft_color::Paint::None),
        Some(v) => Some(vectorcraft_color::Paint::solid(color_value(v).ok_or_else(|| bad(C, "bad fill colour"))?)),
    };
    let features = super::textedit::features_param(p, C)?;
    if font.is_none()
        && style.is_none()
        && size.is_none()
        && leading.is_none()
        && tracking.is_none()
        && justify.is_none()
        && fill.is_none()
        && features.is_none()
    {
        return Err(bad(C, "nothing to change"));
    }
    let ids = text_targets(s, p, C)?;
    let protect = s.prefs.missing_glyph_protection && (font.is_some() || style.is_some());
    s.edit("Character", |d, _| {
        for id in &ids {
            let Some(NodeKind::Text(t)) = d.node_mut(*id).map(|n| &mut n.kind) else { continue };
            let before = protect.then(|| t.runs.clone());
            for r in &mut t.runs {
                let st = &mut r.style;
                if let Some(f) = &font {
                    st.font_family = f.clone();
                }
                if let Some(f) = &style {
                    st.font_style = f.clone();
                }
                if let Some(v) = size {
                    st.size = v.clamp(0.1, 1296.0);
                }
                if let Some(l) = leading {
                    st.leading = l;
                }
                if let Some(v) = tracking {
                    st.tracking = v.clamp(-1000.0, 10000.0);
                }
                if let Some(f) = &fill {
                    st.fill = f.clone();
                }
                if let Some(f) = &features {
                    st.features = f.clone();
                }
            }
            if let Some(before) = before {
                super::textedit::protect_missing_glyphs(&before, &mut t.runs);
            }
            if let Some(j) = justify {
                t.para.justify = j;
            }
            refresh_bounds(t);
        }
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

/// Reshape area type's frame with `f` ([`TextObject::transform_area`],
/// [`TextObject::move_area_anchors`]) and lay its text out again. False, changing nothing, when
/// `f` fails or the frame would reach past [`crate::MAX_COORD`] (the text flows over its height).
pub(crate) fn reshape_area_with(t: &mut TextObject, f: impl FnOnce(&mut TextObject) -> bool) -> bool {
    let before = t.kind.clone();
    let within = |t: &TextObject| match &t.kind {
        vectorcraft_doc::TextKind::Area { frame } => {
            frame.bounds().is_some_and(|b| [b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.abs() <= crate::MAX_COORD))
        }
        _ => false,
    };
    if !(f(t) && within(t)) {
        t.kind = before;
        return false;
    }
    refresh_bounds(t);
    true
}

/// Area type's text, if `n` is area type.
fn area_text(n: Option<&Node>) -> Option<&TextObject> {
    match n.map(|n| &n.kind) {
        Some(NodeKind::Text(t)) if matches!(t.kind, vectorcraft_doc::TextKind::Area { .. }) => Some(t),
        _ => None,
    }
}

/// The width and height of area type's frame in points, along the type's own axes, and the frame's
/// bounds in text space.
fn area_size(t: &TextObject) -> Option<(f64, f64, vectorcraft_geom::Rect)> {
    let vectorcraft_doc::TextKind::Area { frame } = &t.kind else { return None };
    let b = frame.bounds()?;
    let [a, bb, c, d, _, _] = t.xf.as_coeffs();
    Some((b.width() * a.hypot(bb), b.height() * c.hypot(d), b))
}

/// Size area type's frame to `w` × `h` points (None: keep that side) from its top-left corner,
/// and lay its text out again. False when the frame doesn't change.
fn size_area(t: &mut TextObject, w: Option<f64>, h: Option<f64>) -> bool {
    let Some((cw, ch, b)) = area_size(t) else { return false };
    let factor = |to: Option<f64>, cur: f64| match to {
        Some(to) if cur > 1e-9 && (to - cur).abs() > 1e-9 => to / cur,
        _ => 1.0,
    };
    let (sx, sy) = (factor(w, cw), factor(h, ch));
    if sx == 1.0 && sy == 1.0 {
        return false;
    }
    let o = Affine::translate(b.origin().to_vec2());
    let a = t.xf * o * Affine::scale_non_uniform(sx, sy) * o.inverse() * t.xf.inverse();
    reshape_area_with(t, |t| t.transform_area(a))
}

fn area_options(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "text.areaOptions";
    let ids: Vec<NodeId> = {
        let d = &s.doc()?.doc;
        text_targets(s, p, C)?.into_iter().filter(|i| area_text(d.node(*i)).is_some()).collect()
    };
    let first = *ids.first().ok_or_else(|| bad(C, "select area type (text in a frame)"))?;
    let options = |s: &Session| -> Result<Value> {
        let t = area_text(s.doc()?.doc.node(first)).ok_or_else(|| bad(C, "select area type (text in a frame)"))?;
        let mut v = serde_json::to_value(&t.area).map_err(|e| EngineError::Other(e.to_string()))?;
        if let (Some(o), Some((w, h, _))) = (v.as_object_mut(), area_size(t)) {
            o.insert("width".into(), json!(w));
            o.insert("height".into(), json!(h));
        }
        Ok(v)
    };
    let mut v = options(s)?;
    let size = |k: &str| p.get(k).and_then(Value::as_f64).map(|x| x.clamp(1.0, 100_000.0));
    let (w, h) = (size("width"), size("height"));
    let mut changed = w.is_some() || h.is_some();
    if let (Some(o), Some(src)) = (v.as_object_mut(), p.as_object()) {
        for (k, val) in src {
            if o.contains_key(k) && !matches!(k.as_str(), "ids" | "width" | "height") {
                o.insert(k.clone(), val.clone());
                changed = true;
            }
        }
    }
    if !changed {
        return Ok(v);
    }
    let mut opts: vectorcraft_doc::AreaOptions = serde_json::from_value(v).map_err(|e| bad(C, e.to_string()))?;
    opts.rows = opts.rows.clamp(1, 100);
    opts.columns = opts.columns.clamp(1, 100);
    opts.gutter = opts.gutter.clamp(0.0, 10_000.0);
    opts.inset = opts.inset.clamp(0.0, 10_000.0);
    opts.first_baseline_min = opts.first_baseline_min.clamp(0.0, 10_000.0);
    s.edit("Area Type Options", |d, _| {
        for id in &ids {
            if let Some(NodeKind::Text(t)) = d.node_mut(*id).map(|n| &mut n.kind) {
                t.area = opts.clone();
                // A resized frame has laid its text out again; otherwise the new options do here.
                if !size_area(t, w, h) {
                    refresh_bounds(t);
                }
            }
        }
        Ok(())
    })?;
    options(s)
}

fn reshape_area(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "text.reshapeArea";
    let id = id_param(p, "id").ok_or_else(|| bad(C, "missing id"))?;
    let mut refs = super::select::parse_refs(p.get("anchors"));
    refs.sort_unstable();
    refs.dedup();
    if refs.is_empty() {
        return Err(bad(C, "missing anchors [[subpath, anchor]…]"));
    }
    let d = Vec2::new(f64_req(p, "dx", C)?, f64_req(p, "dy", C)?);
    let node = s.doc()?.doc.node(id);
    if area_text(node).is_none() || node.is_some_and(|n| n.perspective.is_some()) {
        return Err(bad(C, "not area type (text in a frame) outside perspective"));
    }
    s.edit("Reshape Type Area", |doc, _| {
        let Some(NodeKind::Text(t)) = doc.node_mut(id).map(|n| &mut n.kind) else { return Err(EngineError::NoNode(id)) };
        if !reshape_area_with(t, |t| t.move_area_anchors(&refs, d)) {
            return Err(bad(C, "no such frame anchor, or the frame would reach past the canvas"));
        }
        Ok(())
    })?;
    ok()
}

#[cfg(test)]
mod area_tests {
    use super::*;

    #[test]
    fn leading_model_is_set_per_paragraph_and_saved_only_when_top_to_top() {
        use vectorcraft_doc::LeadingModel;
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 400, "height": 400})).unwrap();
        let id = s.execute("text.create", &json!({"x": 10, "y": 10, "text": "一\n二", "area": {"width": 200, "height": 100}})).unwrap()["id"]
            .as_u64()
            .unwrap();
        let text = |s: &Session| match &s.doc().unwrap().doc.node(NodeId(id)).unwrap().kind {
            NodeKind::Text(t) => (**t).clone(),
            _ => panic!("text"),
        };
        assert_eq!(text(&s).para.leading_model, LeadingModel::RomanBaseline);
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        assert!(s.execute("text.setFormat", &json!({"leadingModel": "middle"})).is_err());
        let before = text(&s).cached_bounds;
        s.execute("text.setFormat", &json!({"leadingModel": "emBoxTop"})).unwrap();
        let t = text(&s);
        assert_eq!(t.para.leading_model, LeadingModel::EmBoxTop);
        assert_ne!(t.cached_bounds, before, "the first line moves up to the frame's top");
        assert_eq!(serde_json::to_value(&t.para).unwrap()["leading_model"], "emBoxTop");
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(serde_json::to_value(&text(&s).para).unwrap().get("leading_model").is_none());
    }

    #[test]
    fn character_alignment_is_a_character_attribute_saved_only_when_set() {
        use vectorcraft_doc::CharAlign;
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 400, "height": 400})).unwrap();
        let id = s.execute("text.create", &json!({"x": 10, "y": 50, "text": "雅楽"})).unwrap()["id"].as_u64().unwrap();
        let runs = |s: &Session| match &s.doc().unwrap().doc.node(NodeId(id)).unwrap().kind {
            NodeKind::Text(t) => t.runs.clone(),
            _ => panic!("text"),
        };
        assert_eq!(runs(&s)[0].style.char_align, CharAlign::RomanBaseline);
        assert!(serde_json::to_value(&runs(&s)[0].style).unwrap().get("charAlign").is_none());
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        assert!(s.execute("text.setFormat", &json!({"charAlign": "middle"})).is_err());
        s.execute("text.setFormat", &json!({"charAlign": "emBoxCenter"})).unwrap();
        assert!(runs(&s).iter().all(|r| r.style.char_align == CharAlign::EmBoxCenter));
        assert_eq!(serde_json::to_value(&runs(&s)[0].style).unwrap()["charAlign"], "emBoxCenter");
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(runs(&s)[0].style.char_align, CharAlign::RomanBaseline);
        // Characters selected with the Type tool: only the range takes it (雅 is 3 bytes).
        assert!(s.execute("text.setRangeStyle", &json!({"id": id, "start": 3, "end": 6, "charAlign": "top"})).is_err());
        s.execute("text.setRangeStyle", &json!({"id": id, "start": 3, "end": 6, "charAlign": "emBoxTop"})).unwrap();
        let aligns: Vec<_> = runs(&s).iter().map(|r| (r.text.clone(), r.style.char_align)).collect();
        assert_eq!(aligns, [("雅".to_string(), CharAlign::RomanBaseline), ("楽".to_string(), CharAlign::EmBoxTop)]);
    }

    #[test]
    fn new_type_is_composed_with_line_end_half_width_punctuation() {
        use vectorcraft_doc::Mojikumi;
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 400, "height": 400})).unwrap();
        let id = s.execute("text.create", &json!({"x": 10, "y": 50, "text": "雅楽。"})).unwrap()["id"].as_u64().unwrap();
        let para = |s: &Session| match &s.doc().unwrap().doc.node(NodeId(id)).unwrap().kind {
            NodeKind::Text(t) => t.para.clone(),
            _ => panic!("text"),
        };
        assert_eq!(para(&s).mojikumi, Mojikumi::LineEndHalf);
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        assert!(s.execute("text.setFormat", &json!({"mojikumi": "everything"})).is_err());
        s.execute("text.setFormat", &json!({"mojikumi": "none"})).unwrap();
        assert_eq!(para(&s).mojikumi, Mojikumi::None);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(para(&s).mojikumi, Mojikumi::LineEndHalf);
        // Saved only when set; documents from before it read as None.
        let json = serde_json::to_value(para(&s)).unwrap();
        assert_eq!(json["mojikumi"], "lineEndHalf");
        let old: vectorcraft_doc::ParaStyle = serde_json::from_value(json!({"justify": "Left"})).unwrap();
        assert_eq!(old.mojikumi, Mojikumi::None);
        assert!(serde_json::to_value(&old).unwrap().get("mojikumi").is_none());
    }

    #[test]
    fn vertical_point_type_keeps_its_anchor_on_the_column_centre_line() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 400, "height": 400})).unwrap();
        let id = s.execute("text.create", &json!({"x": 100, "y": 50, "text": "§§§§ abc"})).unwrap()["id"].as_u64().unwrap();
        let bounds = |s: &Session| s.doc().unwrap().doc.node(NodeId(id)).unwrap().geometric_bounds().unwrap();
        let wide = bounds(&s);
        assert!(wide.width() > wide.height());
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        s.execute("type.orientation.vertical", &json!({})).unwrap();
        let tall = bounds(&s);
        assert!(tall.height() > tall.width(), "{tall:?}");
        assert!((tall.center().x - 100.0).abs() < 6.0, "the anchor stays on the column's centre line: {tall:?}");
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(bounds(&s), wide);
    }

    #[test]
    fn area_options_columns_change_layout() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 400, "height": 400})).unwrap();
        let text = "word ".repeat(80);
        let id = s.execute("text.create", &json!({"x": 10, "y": 10, "text": text, "area": {"width": 300, "height": 120}})).unwrap()["id"]
            .as_u64()
            .unwrap();
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        let q = s.execute("text.areaOptions", &json!({})).unwrap();
        assert_eq!((q["rows"].as_u64(), q["columns"].as_u64()), (Some(1), Some(1)));
        let lay = |s: &Session| match &s.doc().unwrap().doc.node(NodeId(id)).unwrap().kind {
            NodeKind::Text(t) => vectorcraft_text::layout(vectorcraft_text::FontDb::global(), t),
            _ => panic!(),
        };
        assert_eq!(lay(&s).frames.len(), 1);
        let r = s.execute("text.areaOptions", &json!({"columns": 3, "gutter": 12, "inset": 4, "firstBaseline": "capHeight"})).unwrap();
        assert_eq!(r["firstBaseline"], "capHeight");
        let l = lay(&s);
        assert_eq!(l.frames.len(), 3, "three column cells");
        // Glyphs land in more than one column.
        let xs: Vec<f64> = l.glyphs.iter().map(|g| g.origin.x).collect();
        assert!(xs.iter().any(|x| *x > 110.0 + 10.0) && xs.iter().any(|x| *x < 100.0));
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(lay(&s).frames.len(), 1);
        assert!(s.execute("text.areaOptions", &json!({"firstBaseline": "nope"})).is_err());
    }
}

/// Width of the first line and of the space it can fill, with `tracking` on the first paragraph.
fn headline_fit(t: &TextObject, tracking: f64) -> Option<(f64, f64, usize)> {
    let mut probe = t.clone();
    let para_end = probe.plain_text().find('\n').unwrap_or(usize::MAX);
    vectorcraft_text::edit::style_range(&mut probe.runs, 0, para_end.min(vectorcraft_text::edit::runs_len(&t.runs)), |st| st.tracking = tracking);
    let lay = vectorcraft_text::layout(vectorcraft_text::FontDb::global(), &probe);
    let cell = lay.frames.first()?;
    let line = lay.lines.first()?;
    let avail = cell.width() - 2.0 * t.area.inset - t.para.left_indent - t.para.right_indent;
    Some((line.x1 - line.x0, avail, lay.lines.iter().filter(|l| l.start < para_end).count()))
}

fn fit_headline(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "text.fitHeadline";
    let ids = text_targets(s, p, C)?;
    let d = &s.doc()?.doc;
    let mut plans = vec![];
    for id in &ids {
        let Some(NodeKind::Text(t)) = d.node(*id).map(|n| &n.kind) else { continue };
        if !matches!(t.kind, vectorcraft_doc::TextKind::Area { .. }) {
            continue;
        }
        // Largest tracking that keeps the first paragraph on one line (bisection).
        let one_line = |tr: f64| headline_fit(t, tr).is_some_and(|(_, _, lines)| lines == 1);
        let (mut lo, mut hi) = (-200.0, 2000.0);
        if !one_line(lo) {
            continue;
        }
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if one_line(mid) { lo = mid } else { hi = mid }
        }
        plans.push((*id, (lo * 10.0).floor() / 10.0));
    }
    let first = plans.first().map(|p| p.1).ok_or_else(|| bad(C, "select area type whose first line can fit its frame"))?;
    s.edit("Fit Headline", |d, _| {
        for (id, tr) in &plans {
            if let Some(NodeKind::Text(t)) = d.node_mut(*id).map(|n| &mut n.kind) {
                let end = t.plain_text().find('\n').unwrap_or(usize::MAX).min(vectorcraft_text::edit::runs_len(&t.runs));
                vectorcraft_text::edit::style_range(&mut t.runs, 0, end, |st| st.tracking = *tr);
                refresh_bounds(t);
            }
        }
        Ok(())
    })?;
    Ok(json!({ "tracking": first }))
}

#[cfg(test)]
mod headline_tests {
    use super::*;

    #[test]
    fn fit_headline_fills_the_first_line() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 400, "height": 400})).unwrap();
        let id = s
            .execute("text.create", &json!({"x": 10, "y": 10, "text": "HEADLINE\nbody text", "size": 24, "area": {"width": 300, "height": 120}}))
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        let tr = s.execute("text.fitHeadline", &json!({})).unwrap()["tracking"].as_f64().unwrap();
        assert!(tr > 100.0, "a short word spreads out: {tr}");
        let NodeKind::Text(t) = &s.doc().unwrap().doc.node(NodeId(id)).unwrap().kind else { panic!() };
        let (w, avail, lines) = headline_fit(t, tr).unwrap();
        assert_eq!(lines, 1);
        assert!(avail - w < 30.0, "{w} of {avail}");
        assert_eq!(t.runs.last().unwrap().style.tracking, 0.0, "the body keeps its tracking");
    }
}
