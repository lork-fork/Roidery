//! Commands the panels need beyond the core set: artboard reorder/duplicate and the advanced
//! Character/Paragraph attributes.

use serde_json::{Value, json};
use vectorcraft_doc::NodeKind;

use super::*;
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("artboard.reorder", "Move Artboard Up/Down", [], None, "{index, to} change an artboard's number", has_doc, artboard_reorder),
        cmd!(
            "artboard.duplicate",
            "Duplicate Artboard",
            ["Window", "Artboards"],
            None,
            "{index} copy placed right of the last artboard",
            has_doc,
            artboard_duplicate
        ),
        cmd!(
            "text.setFormat",
            "Character / Paragraph",
            [],
            None,
            "{ids?|id?, kerning?: 1/1000 em|\"auto\", baselineShift?: pt, hScale?: %, vScale?: %, rotation?: deg, underline?, strikethrough?, allCaps?, smallCaps?: bool, position?: \"normal\"|\"superscript\"|\"subscript\" (sizes from Document Setup), leftIndent?, rightIndent?, firstLineIndent?, spaceBefore?, spaceAfter?: pt, hyphenate?: bool, mojikumi?: \"none\"|\"lineEndHalf\" (Japanese punctuation spacing), direction?: \"auto\"|\"leftToRight\"|\"rightToLeft\" (paragraph direction; auto: from each paragraph's first strong character), leadingModel?: \"romanBaseline\"|\"emBoxTop\" (leading measured baseline to baseline, or em box top to top), charAlign?: \"romanBaseline\"|\"emBoxTop\"|\"emBoxCenter\"|\"emBoxBottom\" (where characters smaller than the largest on their line line up with it)}",
            has_doc,
            set_format
        ),
    ]
}

// ---------- artboards ----------

fn artboard_reorder(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "artboard.reorder";
    let i = p.get("index").and_then(Value::as_u64).ok_or_else(|| bad(C, "missing index"))? as usize;
    let to = p.get("to").and_then(Value::as_u64).ok_or_else(|| bad(C, "missing to"))? as usize;
    s.edit("Reorder Artboards", |d, _| {
        if i >= d.artboards.len() {
            return Err(EngineError::Other("no such artboard".into()));
        }
        let a = d.artboards.remove(i);
        let to = to.min(d.artboards.len());
        d.artboards.insert(to, a);
        Ok(())
    })?;
    ok()
}

fn artboard_duplicate(s: &mut Session, p: &Value) -> Result<Value> {
    let i = p.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
    let index = s.edit("Duplicate Artboard", |d, _| {
        let src = d.artboards.get(i).cloned().ok_or_else(|| EngineError::Other("no such artboard".into()))?;
        let right = d.artboards.iter().map(|a| a.rect.x1).fold(f64::MIN, f64::max);
        let dx = right + 20.0 - src.rect.x0;
        Ok(push_artboard_copy(d, &src, vectorcraft_geom::Rect::new(src.rect.x0 + dx, src.rect.y0, src.rect.x1 + dx, src.rect.y1)))
    })?;
    Ok(json!({"index": index}))
}

/// Add a copy of artboard `src` at `rect`, named `<name> copy` (`<name> copy 2`… when taken),
/// with an id of its own → its index.
pub(crate) fn push_artboard_copy(d: &mut vectorcraft_doc::Document, src: &vectorcraft_doc::Artboard, rect: vectorcraft_geom::Rect) -> usize {
    let mut a = src.clone();
    a.id = d.artboards.iter().map(|a| a.id).max().unwrap_or(0).saturating_add(1);
    let taken = |name: &str| d.artboards.iter().any(|a| a.name == name);
    a.name = std::iter::once(format!("{} copy", src.name))
        .chain((2u64..).map(|i| format!("{} copy {i}", src.name)))
        .find(|name| !taken(name))
        .unwrap_or_default();
    a.rect = rect;
    d.artboards.push(a);
    d.artboards.len() - 1
}

// ---------- text ----------

fn set_format(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "text.setFormat";
    let ids: Vec<NodeId> = {
        let ids = targets(s, p)?;
        let d = &s.doc()?.doc;
        ids.into_iter().filter(|id| matches!(d.node(*id).map(|n| &n.kind), Some(NodeKind::Text(_)))).collect()
    };
    if ids.is_empty() {
        return Err(bad(C, "no text objects selected"));
    }
    let num = |k: &str| p.get(k).and_then(Value::as_f64);
    let flag = |k: &str| p.get(k).and_then(Value::as_bool);
    let kerning = match p.get("kerning") {
        None | Some(Value::Null) => None,
        Some(Value::String(a)) if a.eq_ignore_ascii_case("auto") => Some(None),
        Some(v) => Some(Some(v.as_f64().ok_or_else(|| bad(C, "kerning must be a number or \"auto\""))?.clamp(-1000.0, 10000.0))),
    };
    let keys = [
        "kerning",
        "baselineShift",
        "hScale",
        "vScale",
        "rotation",
        "underline",
        "strikethrough",
        "allCaps",
        "smallCaps",
        "position",
        "leftIndent",
        "rightIndent",
        "firstLineIndent",
        "spaceBefore",
        "spaceAfter",
        "hyphenate",
        "mojikumi",
        "direction",
        "leadingModel",
        "charAlign",
    ];
    if !keys.iter().any(|k| p.get(*k).is_some()) {
        return Err(bad(C, "nothing to change"));
    }
    let (position, small_caps) = super::docsetup::script_params(p, &s.doc()?.doc.setup, C)?;
    let char_align = super::textedit::char_align_param(p, C)?;
    let mojikumi = match p.get("mojikumi") {
        None => None,
        Some(v) => Some(match v.as_str() {
            Some("none") => vectorcraft_doc::Mojikumi::None,
            Some("lineEndHalf") => vectorcraft_doc::Mojikumi::LineEndHalf,
            _ => return Err(bad(C, "`mojikumi` must be \"none\" or \"lineEndHalf\"")),
        }),
    };
    let direction = match p.get("direction") {
        None => None,
        Some(v) => Some(match v.as_str() {
            Some("auto") => None,
            Some("leftToRight") => Some(vectorcraft_doc::ParaDirection::LeftToRight),
            Some("rightToLeft") => Some(vectorcraft_doc::ParaDirection::RightToLeft),
            _ => return Err(bad(C, "`direction` must be \"auto\", \"leftToRight\" or \"rightToLeft\"")),
        }),
    };
    let leading_model = match p.get("leadingModel") {
        None => None,
        Some(v) => Some(match v.as_str() {
            Some("romanBaseline") => vectorcraft_doc::LeadingModel::RomanBaseline,
            Some("emBoxTop") => vectorcraft_doc::LeadingModel::EmBoxTop,
            _ => return Err(bad(C, "`leadingModel` must be \"romanBaseline\" or \"emBoxTop\"")),
        }),
    };
    s.edit("Character", |d, _| {
        for id in &ids {
            let Some(NodeKind::Text(t)) = d.node_mut(*id).map(|n| &mut n.kind) else { continue };
            for r in &mut t.runs {
                let st = &mut r.style;
                if let Some(k) = kerning {
                    st.kerning = k;
                }
                if let Some(v) = num("baselineShift") {
                    st.baseline_shift = v.clamp(-1296.0, 1296.0);
                }
                if let Some(v) = num("hScale") {
                    st.h_scale = v.clamp(1.0, 10000.0);
                }
                if let Some(v) = num("vScale") {
                    st.v_scale = v.clamp(1.0, 10000.0);
                }
                if let Some(v) = num("rotation") {
                    st.rotation = ((v + 180.0).rem_euclid(360.0)) - 180.0;
                }
                if let Some(v) = flag("underline") {
                    st.underline = v;
                }
                if let Some(v) = flag("strikethrough") {
                    st.strikethrough = v;
                }
                if let Some(v) = flag("allCaps") {
                    st.all_caps = v;
                }
                if let Some(v) = position {
                    st.position = v;
                }
                if let Some(v) = small_caps {
                    st.small_caps = v;
                }
                if let Some(v) = char_align {
                    st.char_align = v;
                }
            }
            let para = &mut t.para;
            if let Some(v) = num("leftIndent") {
                para.left_indent = v;
            }
            if let Some(v) = num("rightIndent") {
                para.right_indent = v;
            }
            if let Some(v) = num("firstLineIndent") {
                para.first_line_indent = v;
            }
            if let Some(v) = num("spaceBefore") {
                para.space_before = v;
            }
            if let Some(v) = num("spaceAfter") {
                para.space_after = v;
            }
            if let Some(v) = flag("hyphenate") {
                para.hyphenate = v;
            }
            if let Some(v) = mojikumi {
                para.mojikumi = v;
            }
            if let Some(v) = direction {
                para.direction = v;
            }
            if let Some(v) = leading_model {
                para.leading_model = v;
            }
            super::typecmd::refresh_bounds(t);
        }
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}
