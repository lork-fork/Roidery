//! Reading a font variant's SVG: one layer per glyph (`GlyphLayer-A`), each holding the paths
//! that sew the letter with their Ink/Stitch parameters (`inkstitch:*` attributes).
//!
//! Coordinates are the font's user units (Ink/Stitch treats them as pixels, 96 to the inch,
//! whatever the file's own width says), y down. Each glyph is moved so its leftmost point is at
//! x = 0 and the baseline at y = 0, as Ink/Stitch lays glyphs out.

use std::collections::BTreeMap;
use std::str::FromStr;

use kurbo::{Affine, BezPath, Shape};

/// The Ink/Stitch attribute namespace.
const INKSTITCH_NS: &str = "http://inkstitch.org/namespace";
const INKSCAPE_NS: &str = "http://www.inkscape.org/namespaces/inkscape";
const SODIPODI_NS: &str = "http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd";

/// Largest SVG a variant may be (the biggest Ink/Stitch font is about 15 MB).
pub const MAX_SVG_BYTES: usize = 64 * 1024 * 1024;
/// Most XML nodes a variant may have.
const MAX_NODES: u32 = 4_000_000;
/// Most glyphs a font keeps.
const MAX_GLYPHS: usize = 20_000;
/// Most elements a glyph keeps.
const MAX_ELEMENTS: usize = 2_000;

/// A drawn element of a glyph: its outline in font units and how it is painted and sewn.
#[derive(Clone, Debug)]
pub struct Element {
    pub path: BezPath,
    /// Fill colour, `None` when the element has no fill.
    pub fill: Option<[u8; 3]>,
    /// Stroke colour, `None` when the element has no stroke.
    pub stroke: Option<[u8; 3]>,
    /// Stroke width in font units.
    pub stroke_width: f64,
    /// The element's `inkstitch:*` parameters, by name without the prefix.
    pub params: BTreeMap<String, String>,
}

impl Element {
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(String::as_str).map(str::trim).filter(|v| !v.is_empty())
    }

    /// A number parameter (the first of several, as `pull_compensation_mm="0.2 0.3"` has).
    pub fn number(&self, key: &str) -> Option<f64> {
        self.param(key)?.split([' ', ',']).find(|s| !s.is_empty())?.parse::<f64>().ok().filter(|v| v.is_finite())
    }

    /// A yes/no parameter.
    pub fn flag(&self, key: &str) -> Option<bool> {
        match self.param(key)?.to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        }
    }
}

/// A glyph in font units, its leftmost point at x = 0 and the baseline at y = 0.
#[derive(Clone, Debug, Default)]
pub struct Glyph {
    /// The glyph's name: its character, a ligature (`fi`) or a named form (`a.init`).
    pub name: String,
    /// Where the leftmost point was before the glyph was moved to x = 0 (Ink/Stitch adds it
    /// before a letter and takes it off the advance after).
    pub min_x: f64,
    pub width: f64,
    pub elements: Vec<Element>,
}

impl Glyph {
    pub fn bounds(&self) -> Option<kurbo::Rect> {
        self.elements.iter().map(|e| e.path.bounding_box()).filter(|r| r.is_finite()).reduce(|a, b| a.union(b))
    }
}

/// Inherited presentation: fill, stroke and stroke width.
#[derive(Clone, Copy)]
struct Style {
    fill: Option<[u8; 3]>,
    stroke: Option<[u8; 3]>,
    stroke_width: f64,
    hidden: bool,
}

impl Default for Style {
    fn default() -> Self {
        // SVG's initial values: a black fill, no stroke, 1 unit wide.
        Self { fill: Some([0, 0, 0]), stroke: None, stroke_width: 1.0, hidden: false }
    }
}

fn paint(v: &str, current: Option<[u8; 3]>) -> Option<Option<[u8; 3]>> {
    match svgtypes::Paint::from_str(v.trim()).ok()? {
        svgtypes::Paint::None => Some(None),
        svgtypes::Paint::Inherit => None,
        svgtypes::Paint::CurrentColor | svgtypes::Paint::ContextFill | svgtypes::Paint::ContextStroke => Some(current.or(Some([0, 0, 0]))),
        svgtypes::Paint::Color(c) => Some(Some([c.red, c.green, c.blue])),
        // A gradient or pattern sews in its fallback colour, else black.
        svgtypes::Paint::FuncIRI(_, fallback) => Some(Some(match fallback {
            Some(svgtypes::PaintFallback::Color(c)) => [c.red, c.green, c.blue],
            _ => [0, 0, 0],
        })),
    }
}

fn length(v: &str) -> Option<f64> {
    let l = svgtypes::Length::from_str(v.trim()).ok()?;
    let k = match l.unit {
        svgtypes::LengthUnit::None | svgtypes::LengthUnit::Px => 1.0,
        svgtypes::LengthUnit::Mm => 96.0 / 25.4,
        svgtypes::LengthUnit::Cm => 96.0 / 2.54,
        svgtypes::LengthUnit::In => 96.0,
        svgtypes::LengthUnit::Pt => 96.0 / 72.0,
        svgtypes::LengthUnit::Pc => 16.0,
        _ => 1.0,
    };
    Some(l.number * k).filter(|v| v.is_finite())
}

/// `style` declarations and presentation attributes of `n` over the inherited `st`.
fn style_of(n: roxmltree::Node, mut st: Style) -> Style {
    let mut decls: Vec<(String, String)> = vec![];
    for key in ["fill", "stroke", "stroke-width", "display", "visibility"] {
        if let Some(v) = n.attribute(key) {
            decls.push((key.to_string(), v.to_string()));
        }
    }
    if let Some(s) = n.attribute("style") {
        for d in s.split(';') {
            if let Some((k, v)) = d.split_once(':') {
                decls.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    for (k, v) in decls {
        match k.as_str() {
            "fill" => {
                if let Some(p) = paint(&v, st.fill) {
                    st.fill = p;
                }
            }
            "stroke" => {
                if let Some(p) = paint(&v, st.stroke) {
                    st.stroke = p;
                }
            }
            "stroke-width" => {
                if let Some(w) = length(&v).filter(|w| *w >= 0.0) {
                    st.stroke_width = w;
                }
            }
            "display" if v == "none" => st.hidden = true,
            "visibility" if v == "hidden" || v == "collapse" => st.hidden = true,
            _ => {}
        }
    }
    st
}

fn transform_of(n: roxmltree::Node) -> Affine {
    n.attribute("transform")
        .and_then(|t| svgtypes::Transform::from_str(t).ok())
        .map(|t| Affine::new([t.a, t.b, t.c, t.d, t.e, t.f]))
        .filter(|a| a.is_finite())
        .unwrap_or(Affine::IDENTITY)
}

fn num(n: roxmltree::Node, key: &str) -> f64 {
    n.attribute(key).and_then(length).unwrap_or(0.0)
}

/// The outline of a path or basic shape, untransformed.
fn outline(n: roxmltree::Node) -> Option<BezPath> {
    let mut bp = BezPath::new();
    match n.tag_name().name() {
        "path" => {
            let d = n.attribute("d")?;
            let mut open = false;
            for seg in svgtypes::SimplifyingPathParser::from(d) {
                let Ok(seg) = seg else { break };
                match seg {
                    svgtypes::SimplePathSegment::MoveTo { x, y } => {
                        bp.move_to((x, y));
                        open = true;
                    }
                    svgtypes::SimplePathSegment::LineTo { x, y } if open => bp.line_to((x, y)),
                    svgtypes::SimplePathSegment::CurveTo { x1, y1, x2, y2, x, y } if open => bp.curve_to((x1, y1), (x2, y2), (x, y)),
                    svgtypes::SimplePathSegment::Quadratic { x1, y1, x, y } if open => bp.quad_to((x1, y1), (x, y)),
                    svgtypes::SimplePathSegment::ClosePath if open => bp.close_path(),
                    _ => {}
                }
            }
        }
        "rect" => {
            let (x, y, w, h) = (num(n, "x"), num(n, "y"), num(n, "width"), num(n, "height"));
            if w <= 0.0 || h <= 0.0 {
                return None;
            }
            bp = kurbo::Rect::new(x, y, x + w, y + h).to_path(0.01);
        }
        "circle" | "ellipse" => {
            let (cx, cy) = (num(n, "cx"), num(n, "cy"));
            let (rx, ry) = if n.tag_name().name() == "circle" { (num(n, "r"), num(n, "r")) } else { (num(n, "rx"), num(n, "ry")) };
            if rx <= 0.0 || ry <= 0.0 {
                return None;
            }
            bp = kurbo::Ellipse::new((cx, cy), (rx, ry), 0.0).to_path(0.01);
        }
        "line" => {
            bp.move_to((num(n, "x1"), num(n, "y1")));
            bp.line_to((num(n, "x2"), num(n, "y2")));
        }
        "polyline" | "polygon" => {
            let pts: Vec<f64> = n.attribute("points")?.split([' ', ',', '\n', '\t', '\r']).filter_map(|s| s.parse().ok()).collect();
            for (i, p) in pts.chunks_exact(2).enumerate() {
                let (Some(x), Some(y)) = (p.first(), p.get(1)) else { continue };
                if i == 0 {
                    bp.move_to((*x, *y));
                } else {
                    bp.line_to((*x, *y));
                }
            }
            if n.tag_name().name() == "polygon" {
                bp.close_path();
            }
        }
        _ => return None,
    }
    (!bp.elements().is_empty() && bp.elements().iter().all(|e| e.end_point().is_none_or(|p| p.is_finite()))).then_some(bp)
}

/// Elements that hold artwork that isn't sewn where it stands.
fn is_container_skipped(name: &str) -> bool {
    matches!(
        name,
        "defs" | "clipPath" | "mask" | "symbol" | "marker" | "pattern" | "metadata" | "title" | "desc" | "namedview" | "text" | "image" | "style"
    )
}

fn collect(n: roxmltree::Node, xf: Affine, st: Style, out: &mut Vec<Element>) {
    if out.len() >= MAX_ELEMENTS {
        return;
    }
    let name = n.tag_name().name();
    if is_container_skipped(name) {
        return;
    }
    let st = style_of(n, st);
    if st.hidden {
        return;
    }
    let xf = xf * transform_of(n);
    match name {
        "g" | "a" | "switch" => {
            for c in n.children().filter(roxmltree::Node::is_element) {
                collect(c, xf, st, out);
            }
        }
        // Ink/Stitch commands (trim, start and end points) and their connectors.
        "use" => {}
        _ if n.attribute((INKSCAPE_NS, "connection-end")).is_some() || n.attribute((INKSCAPE_NS, "connector-type")).is_some() => {}
        _ => {
            let Some(mut path) = outline(n) else { return };
            path.apply_affine(xf);
            let mut params = BTreeMap::new();
            for a in n.attributes() {
                if a.namespace() == Some(INKSTITCH_NS) {
                    params.insert(a.name().to_string(), a.value().to_string());
                }
            }
            // Stroke widths scale with the transform (by its mean scale).
            let k = xf.determinant().abs().sqrt();
            let stroke_width = if k.is_finite() && k > 0.0 { st.stroke_width * k } else { st.stroke_width };
            out.push(Element { path, fill: st.fill, stroke: st.stroke, stroke_width, params });
        }
    }
}

/// The page height in user units: the viewBox's, else the `height` attribute's.
fn page_height(root: roxmltree::Node) -> Option<f64> {
    if let Some(vb) = root.attribute("viewBox") {
        let v: Vec<f64> = vb.split([' ', ',']).filter_map(|s| s.parse().ok()).collect();
        if let Some(h) = v.get(3).filter(|h| h.is_finite() && **h > 0.0) {
            return Some(*h);
        }
    }
    root.attribute("height").and_then(length).filter(|h| *h > 0.0)
}

/// The baseline guides' readings, y down: Inkscape stores guide positions from the bottom of
/// the page (y up) in most files, but some keep them y down, and some files have two baselines.
fn baseline_candidates(doc: &roxmltree::Document) -> Vec<f64> {
    let root = doc.root_element();
    let mut v = vec![];
    for n in doc.descendants().filter(|n| n.tag_name().name() == "guide" && n.tag_name().namespace() == Some(SODIPODI_NS)) {
        let label = n.attribute((INKSCAPE_NS, "label")).unwrap_or_default();
        if !label.eq_ignore_ascii_case("baseline") {
            continue;
        }
        let Some(pos) = n.attribute("position") else { continue };
        let Some(y) = pos.split(',').nth(1).and_then(|y| y.trim().parse::<f64>().ok()).filter(|y| y.is_finite()) else { continue };
        if let Some(h) = page_height(root) {
            v.push(h - y);
        }
        v.push(y);
    }
    v
}

fn layer_label<'a>(n: roxmltree::Node<'a, '_>) -> Option<&'a str> {
    n.attribute((INKSCAPE_NS, "label"))?.strip_prefix("GlyphLayer-")
}

/// Every glyph in one variant file, in font units with each glyph's leftmost point at x = 0 and
/// the baseline at y = 0.
pub fn glyphs(text: &str) -> Result<Vec<Glyph>, String> {
    if text.len() > MAX_SVG_BYTES {
        return Err("the font file is too big".into());
    }
    let opts = roxmltree::ParsingOptions { allow_dtd: true, nodes_limit: MAX_NODES, ..Default::default() };
    let doc = roxmltree::Document::parse_with_options(text, opts).map_err(|e| format!("not a readable SVG: {e}"))?;
    let mut raw: Vec<(String, Vec<Element>)> = vec![];
    for layer in doc.descendants().filter(|n| n.is_element() && n.tag_name().name() == "g") {
        let Some(name) = layer_label(layer) else { continue };
        if raw.len() >= MAX_GLYPHS {
            break;
        }
        // The layer's own transform and its ancestors' apply; a hidden layer still holds its glyph.
        let mut xf = Affine::IDENTITY;
        for a in layer.ancestors().skip(1).filter(roxmltree::Node::is_element).collect::<Vec<_>>().into_iter().rev() {
            xf *= transform_of(a);
        }
        let mut st = Style::default();
        for a in layer.ancestors().skip(1).filter(roxmltree::Node::is_element).collect::<Vec<_>>().into_iter().rev() {
            st = style_of(a, st);
        }
        st = style_of(layer, st);
        st.hidden = false;
        let xf = xf * transform_of(layer);
        let mut els = vec![];
        for c in layer.children().filter(roxmltree::Node::is_element) {
            collect(c, xf, st, &mut els);
        }
        if !els.is_empty() {
            raw.push((name.to_string(), els));
        }
    }
    let baseline = choose_baseline(&baseline_candidates(&doc), &raw);
    let mut out = vec![];
    for (name, mut elements) in raw {
        let Some(b) = elements.iter().map(|e| e.path.bounding_box()).filter(|r| r.is_finite()).reduce(|a, b| a.union(b)) else { continue };
        let shift = Affine::translate((-b.x0, -baseline));
        for e in &mut elements {
            e.path.apply_affine(shift);
        }
        out.push(Glyph { name, min_x: b.x0, width: b.width(), elements });
    }
    Ok(out)
}

/// The baseline: the guide reading the capitals stand on. Without a guide, or when no reading is
/// near the capitals' bottoms (a quarter of their height), the capitals' median bottom.
fn choose_baseline(candidates: &[f64], raw: &[(String, Vec<Element>)]) -> f64 {
    let mut caps: Vec<kurbo::Rect> = raw
        .iter()
        .filter(|(n, _)| n.chars().count() == 1 && n.chars().all(|c| "ABDEFHIKLMNPRTUVWXZ".contains(c)))
        .filter_map(|(_, els)| els.iter().map(|e| e.path.bounding_box()).filter(|r| r.is_finite()).reduce(|a, b| a.union(b)))
        .collect();
    caps.sort_by(|a, b| a.y1.total_cmp(&b.y1));
    let Some(median) = caps.get(caps.len() / 2) else { return candidates.first().copied().unwrap_or(0.0) };
    let best = candidates.iter().copied().min_by(|a, b| (a - median.y1).abs().total_cmp(&(b - median.y1).abs()));
    match best {
        Some(b) if (b - median.y1).abs() <= median.height() * 0.25 => b,
        _ => median.y1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
        xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd" xmlns:inkstitch="http://inkstitch.org/namespace"
        width="100" height="100" viewBox="0 0 100 100">
      <sodipodi:namedview><sodipodi:guide position="0,20" inkscape:label="baseline"/></sodipodi:namedview>
      <g inkscape:groupmode="layer" inkscape:label="GlyphLayer-H" style="display:none" transform="translate(5,0)">
        <path d="M 10,80 L 10,20 M 14,80 L 14,20" style="fill:none;stroke:#ff0000;stroke-width:2" inkstitch:satin_column="True" inkstitch:zigzag_spacing_mm="0.35"/>
        <g transform="translate(20,0)"><path d="m 0,50 h 10 v 30 h -10 z" fill="#00ff00"/></g>
        <use xlink:href="#inkstitch_trim" xmlns:xlink="http://www.w3.org/1999/xlink"/>
      </g>
      <g inkscape:label="GlyphLayer-fi"><rect x="0" y="40" width="10" height="40" style="fill:none;stroke:black"/></g>
      <defs><path d="M0,0 L1000,1000" style="stroke:black"/></defs>
    </svg>"##;

    #[test]
    fn reads_glyph_layers_with_their_paths_moved_to_the_origin() {
        let g = glyphs(SVG).unwrap();
        assert_eq!(g.len(), 2);
        let h = &g[0];
        assert_eq!(h.name, "H");
        assert_eq!(h.elements.len(), 2, "the command and the defs are left out");
        // Leftmost point was x = 15 (10 + the layer's 5); the baseline y = 100 - 20 = 80.
        assert!((h.min_x - 15.0).abs() < 1e-9, "{}", h.min_x);
        let b = h.bounds().unwrap();
        assert!(b.x0.abs() < 1e-9 && b.y1.abs() < 1e-9 && (b.y0 + 60.0).abs() < 1e-9, "{b:?}");
        let satin = &h.elements[0];
        assert_eq!(satin.stroke, Some([255, 0, 0]));
        assert_eq!(satin.fill, None);
        assert_eq!(satin.flag("satin_column"), Some(true));
        assert_eq!(satin.number("zigzag_spacing_mm"), Some(0.35));
        assert_eq!(h.elements[1].fill, Some([0, 255, 0]));
        assert_eq!(g[1].name, "fi");
    }

    #[test]
    fn bad_input_is_an_error_not_a_crash() {
        assert!(glyphs("<svg").is_err());
        assert!(glyphs("").is_err());
        let none = glyphs(r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0,0 L nan"/></svg>"#).unwrap();
        assert!(none.is_empty());
    }
}
