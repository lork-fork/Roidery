//! Effect → Document Raster Effects Settings, and raster effects for PDF export.
//!
//! PDF has no live shadows, glows or blurs, so PDF export receives a copy of the document in which
//! every object with a raster effect (any kind of object, or one of its fills or strokes) is
//! accompanied by an image rendered with the document's raster effects settings (resolution, colour
//! model, background, anti-aliasing, room around the art). Shadows and outer glows only add the
//! effect: the image (with the object's own area knocked out) goes under the untouched vector
//! object. Effects that change the object itself (inner glow, feather, Gaussian blur) replace it
//! with the image. Expand Appearance makes its images with the same helpers.

use std::sync::Arc;

use serde_json::{Map, Value, json};
use vectorcraft_color::Paint;
use vectorcraft_doc::rastersettings::MAX_ADD_AROUND;
use vectorcraft_doc::{
    Appearance, AppearanceItem, ColorMode, Document, Effect, ImageBlob, ImageObject, Node, NodeId, NodeKind, RasterColorModel, StrokeLayer,
};
use vectorcraft_geom::{Affine, FillRule, PathData, Rect};
use vectorcraft_render::effects::{self, RasterFx};
use vectorcraft_render::{AntiAlias, RenderOptions};

use super::*;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "document.rasterEffectsSettings",
        "Document Raster Effects Settings…",
        ["Effect"],
        None,
        "{resolution?: ppi (1–2400) | \"screen\" (72) | \"medium\" (150) | \"high\" (300), colorModel?: \"rgb\"|\"cmyk\" (the document's mode)|\"grayscale\"|\"bitmap\", background?: \"transparent\"|\"white\", antiAlias?: bool (off: hard edges), clippingMask?: bool (white background only under the art; Rasterize clips to the art), addAround?: pt (0–1000, room around the art), preserveSpotColors?: bool (stored)} how raster effects (shadows, glows, blurs, feathers) become images in PDF export and Expand Appearance, and the defaults of object.rasterize; one undo step; always → the settings",
        has_doc,
        settings
    )]
}

const C: &str = "document.rasterEffectsSettings";

/// What `document.rasterEffectsSettings` reports (and takes back).
fn settings_json(d: &Document) -> Value {
    let r = &d.raster_effects;
    let model = match (r.color_model, d.color_mode) {
        (RasterColorModel::Document, ColorMode::Rgb) => "rgb",
        (RasterColorModel::Document, ColorMode::Cmyk) => "cmyk",
        (m, _) => m.id(),
    };
    json!({
        "resolution": d.raster_effects_ppi,
        "colorModel": model,
        "background": r.background.id(),
        "antiAlias": r.anti_alias,
        "clippingMask": r.clipping_mask,
        "addAround": r.add_around,
        "preserveSpotColors": r.preserve_spot_colors,
    })
}

fn resolution(v: &Value) -> Result<f64> {
    let ppi = match v {
        Value::String(v) => match v.to_ascii_lowercase().as_str() {
            "screen" => 72.0,
            "medium" => 150.0,
            "high" => 300.0,
            other => other.trim_end_matches("ppi").trim().parse::<f64>().map_err(|_| bad(C, format!("unknown resolution `{v}`")))?,
        },
        v => v.as_f64().ok_or_else(|| bad(C, "resolution must be a number or screen|medium|high"))?,
    };
    if !(1.0..=2400.0).contains(&ppi) {
        return Err(bad(C, "resolution must be between 1 and 2400 ppi"));
    }
    Ok(ppi)
}

/// `colorModel` for a document in `mode`: its own mode's name means the document's model.
pub(crate) fn color_model(v: &Value, mode: ColorMode, cmd: &str) -> Result<RasterColorModel> {
    let s = v.as_str().unwrap_or_default().to_ascii_lowercase();
    match (s.as_str(), mode) {
        ("document", _) | ("rgb", ColorMode::Rgb) | ("cmyk", ColorMode::Cmyk) => Ok(RasterColorModel::Document),
        ("grayscale" | "gray", _) => Ok(RasterColorModel::Grayscale),
        ("bitmap", _) => Ok(RasterColorModel::Bitmap),
        _ => {
            let own = if mode == ColorMode::Rgb { "rgb" } else { "cmyk" };
            Err(bad(cmd, format!("colorModel must be {own} (the document's mode), grayscale or bitmap")))
        }
    }
}

/// `addAround`/`padding` in points.
pub(crate) fn add_around(v: &Value, cmd: &str, key: &str) -> Result<f64> {
    v.as_f64().filter(|x| (0.0..=MAX_ADD_AROUND).contains(x)).ok_or_else(|| bad(cmd, format!("{key} must be 0–{MAX_ADD_AROUND} pt")))
}

fn flag(k: &str, v: &Value) -> Result<bool> {
    v.as_bool().ok_or_else(|| bad(C, format!("{k} must be true or false")))
}

fn settings(s: &mut Session, p: &Value) -> Result<Value> {
    let empty = Map::new();
    let o = match p {
        Value::Null => &empty,
        Value::Object(o) => o,
        _ => return Err(bad(C, "params must be an object")),
    };
    let d = &s.doc()?.doc;
    let (mut ppi, mut r) = (d.raster_effects_ppi, d.raster_effects.clone());
    // A null value leaves its setting as it is.
    for (k, v) in o.iter().filter(|(_, v)| !v.is_null()) {
        match k.as_str() {
            "resolution" => ppi = resolution(v)?,
            "colorModel" => r.color_model = color_model(v, d.color_mode, C)?,
            "background" => r.background = super::docsetup::background_param(v, C)?,
            "antiAlias" => r.anti_alias = flag(k, v)?,
            "clippingMask" => r.clipping_mask = flag(k, v)?,
            "addAround" => r.add_around = add_around(v, C, k)?,
            "preserveSpotColors" => r.preserve_spot_colors = flag(k, v)?,
            _ => return Err(bad(C, format!("unknown setting `{k}`"))),
        }
    }
    if ppi != d.raster_effects_ppi || r != d.raster_effects {
        s.edit("Document Raster Effects Settings", |d, _| {
            d.raster_effects_ppi = ppi;
            d.raster_effects = r;
            Ok(())
        })?;
    }
    Ok(settings_json(&s.doc()?.doc))
}

/// `art` in a clip group clipped by `path` filled by `rule` (a clipping path that paints nothing).
pub(crate) fn clip_group(d: &mut Document, path: PathData, rule: FillRule, art: Node) -> Node {
    let mut clip = super::pathops::shape_node(d, path, None);
    clip.appearance = Appearance::basic(Paint::None, Paint::None, 0.0);
    match &mut clip.kind {
        NodeKind::Path { clipping, rule: r, .. } => (*clipping, *r) = (true, rule),
        NodeKind::Compound { rule: r, .. } => *r = rule,
        _ => {}
    }
    Node::new(d.alloc_id(), NodeKind::Group { children: vec![Arc::new(clip), Arc::new(art)], clip: true })
}

/// Render `nodes` alone over transparency: (premultiplied pixels, width, height).
fn render(doc: &Document, nodes: Vec<Node>, region: Rect, scale: f64, anti_alias: AntiAlias) -> vectorcraft_render::Rendered {
    let mut tmp = doc.clone();
    let mut layer = Node::layer(NodeId(u64::MAX), "raster", vectorcraft_doc::LayerColor::Preset(0));
    if let Some(ch) = layer.children_mut() {
        *ch = nodes.into_iter().map(Arc::new).collect();
    }
    tmp.layers = vec![Arc::new(layer)];
    let mut r = vectorcraft_render::Renderer::new();
    r.threads = 0;
    r.render_region_with(&tmp, region, scale, &RenderOptions { skip_templates: true, anti_alias, ..Default::default() })
}

/// Pixels per point of raster effects rendered as images (the document's raster effects
/// resolution).
pub(crate) fn effects_scale(doc: &Document) -> f64 {
    (doc.raster_effects_ppi / 72.0).clamp(1.0 / 72.0, 2400.0 / 72.0)
}

/// An embedded image (a new node of `out`, its pixels in `out.images`) of `whole` rendered alone
/// with `out`'s resources at `scale` over all it paints (the reach of its own, its members' and its
/// fills' and strokes' raster effects included). With `knockout`, the coverage of that art is knocked out
/// of the image, which then only holds what the effects add around it (a shadow to go under the
/// vector object). The document's raster effects settings add room around it and finish its pixels.
pub(crate) fn effect_image(out: &mut Document, whole: &Node, knockout: Option<&Node>, scale: f64) -> Option<Node> {
    // What it paints (its members' effects included), and how far its fills' and strokes' raster
    // effects reach beyond that.
    let b = vectorcraft_render::painted_bounds(whole)?;
    let look = out.raster_effects.clone();
    let reach = whole.appearance.items.iter().map(|i| effects::outset(i.effects())).fold(0.0, f64::max) + 2.0 + look.add_around;
    let b = b.inflate(reach, reach);
    // Whole pixels, and no larger than 64 Mpx.
    let scale = scale.min((64.0e6 / (b.width() * b.height()).max(1.0)).sqrt());
    let region = Rect::new(b.x0, b.y0, b.x0 + (b.width() * scale).ceil().max(1.0) / scale, b.y0 + (b.height() * scale).ceil().max(1.0) / scale);
    // Anti-alias off: hard edges (the effects themselves stay smooth).
    let anti_alias = if look.anti_alias { AntiAlias::Art } else { AntiAlias::None };
    let mut img = render(out, vec![whole.clone()], region, scale, anti_alias);
    if let Some(bare) = knockout {
        let obj = render(out, vec![bare.clone()], region, scale, anti_alias);
        for (px, o) in img.pixels.as_chunks_mut::<4>().0.iter_mut().zip(obj.pixels.as_chunks::<4>().0) {
            let keep = 1.0 - o[3] as f32 / 255.0;
            for c in px.iter_mut() {
                *c = (*c as f32 * keep).round() as u8;
            }
        }
    }
    // Document Raster Effects Settings: colour model, background.
    look.finish_pixels(&mut img.pixels);
    // Can't encode: keep the object as it is (vector, effects ignored) rather than fail the export.
    let png = img.to_png().ok()?;
    let id = out.alloc_id();
    let mut key = format!("raster-effect-{}", id.0);
    while out.images.contains_key(&key) {
        key.push('+');
    }
    out.images.insert(key.clone(), ImageBlob::new("image/png", png));
    let xf = Affine::translate(region.origin().to_vec2()) * Affine::scale(1.0 / scale);
    let mut image =
        Node::new(id, NodeKind::Image(ImageObject { key, width: img.width, height: img.height, xf, link: None, placement: Default::default() }));
    image.name = Some("Raster effect".into());
    Some(image)
}

/// The visible raster effects of `n`: its own and its fills' and strokes' (none on guides and
/// clipping paths, which paint nothing).
pub(crate) fn raster_fx(n: &Node) -> Vec<RasterFx> {
    if matches!(n.kind, NodeKind::Path { guide: true, .. } | NodeKind::Path { clipping: true, .. }) {
        return vec![];
    }
    std::iter::once(&n.appearance.effects).chain(n.appearance.items.iter().map(|i| i.effects())).flat_map(|e| effects::raster_effects(e)).collect()
}

/// Remove the raster effects of `n` (its own and its fills' and strokes') and, with `members`, of
/// everything inside it.
pub(crate) fn strip_raster(n: &mut Node, members: bool) {
    let keep = |e: &Effect| !effects::is_raster(&e.id);
    n.appearance.effects.retain(keep);
    for it in &mut n.appearance.items {
        it.effects_mut().retain(keep);
    }
    if members {
        for c in n.children_mut().into_iter().flatten() {
            strip_raster(Arc::make_mut(c), true);
        }
    }
}

/// The raster effects of `n` (its own, its fills' and strokes') as an embedded image of `d`
/// rendered at the document's raster effects resolution, without `n`'s transparency (that stays
/// on the object). With `below` (they all paint below it: shadows and outer glows) the image holds
/// just them; with `members` too, a group's or layer's image leaves out its members' raster
/// effects (they get images of their own).
pub(crate) fn raster_image(d: &mut Document, n: &Node, below: bool, members: bool) -> Option<Node> {
    let mut whole = n.clone();
    (whole.opacity, whole.blend, whole.mask) = (1.0, Default::default(), None);
    if below && members && is_container(&whole) {
        for c in whole.children_mut().into_iter().flatten() {
            strip_raster(Arc::make_mut(c), true);
        }
    }
    let bare = below.then(|| {
        let mut b = whole.clone();
        strip_raster(&mut b, false);
        b
    });
    let scale = effects_scale(d);
    effect_image(d, &whole, bare.as_ref(), scale)
}

/// `v` (an object without its raster effects) replaced by `image` of the object with them: the
/// image takes its place with its id, name and transparency (a layer keeps the image as its only
/// member).
pub(crate) fn replace_with_image(mut v: Node, image: Node) -> Node {
    if !v.is_layer() {
        return Node { kind: image.kind, appearance: Default::default(), ..v };
    }
    v.appearance = Default::default();
    v.set_clips(false);
    if let Some(ch) = v.children_mut() {
        *ch = vec![Arc::new(image)];
    }
    v
}

/// `m` with `image` of its shadows and outer glows painted below its art: the image goes first in
/// a group or layer (after a layer's clipping path), anything else is grouped with it under its
/// id, name and transparency.
pub(crate) fn put_below(d: &mut Document, m: &mut Node, image: Node) {
    match &mut m.kind {
        NodeKind::Group { children, clip: false } => children.insert(0, Arc::new(image)),
        NodeKind::Layer { children, clip, .. } => children.insert(usize::from(*clip).min(children.len()), Arc::new(image)),
        _ => {
            let mut inner = m.clone();
            inner.id = d.alloc_id();
            (inner.name, inner.opacity, inner.blend, inner.isolate, inner.mask) = (None, 1.0, Default::default(), false, None);
            // Knockout is between the object's own parts (a blend's steps): it stays with them,
            // so the image doesn't knock out the art over it.
            m.knockout = Default::default();
            inner.knockout_shape = false;
            m.appearance = Default::default();
            m.kind = NodeKind::Group { children: vec![Arc::new(image), Arc::new(inner)], clip: false };
        }
    }
}

fn is_container(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Group { .. } | NodeKind::Layer { .. })
}

/// Can the raster effects of `n` go in an image under its untouched vector art? They must all
/// paint below it (shadows, outer glows) and belong to the object or to fills and strokes with
/// nothing painted under them (a stroke's shadow falls on the fills below it, and type's
/// characters or a group's members paint below its upper fills and strokes).
fn below_only(n: &Node) -> bool {
    let contents = (is_container(n) || matches!(n.kind, NodeKind::Text(_))).then(|| n.appearance.contents_at());
    let mut covered = false;
    for (i, item) in n.appearance.items.iter().enumerate() {
        covered |= contents == Some(i);
        let fx = effects::raster_effects(item.effects());
        if !fx.iter().all(RasterFx::is_below) || (covered && !fx.is_empty()) {
            return false;
        }
        covered |= effects::paints(item);
    }
    effects::raster_effects(&n.appearance.effects).iter().all(RasterFx::is_below)
}

/// A path or compound path with several fills and strokes, some of them with raster effects, as a
/// group of one path per fill and stroke (each keeping its own effects, so each gets an image of
/// its own) under the object's transparency and effects. `None` for other objects.
fn split_items(out: &mut Document, n: &Node) -> Option<Node> {
    let items = &n.appearance.items;
    if !matches!(n.kind, NodeKind::Path { .. } | NodeKind::Compound { .. })
        || items.iter().filter(|i| effects::paints(i)).count() < 2
        || items.iter().all(|i| effects::raster_effects(i.effects()).is_empty())
    {
        return None;
    }
    let mut stroke_piece = |d: &mut Document, path: &PathData, rule: FillRule, st: &StrokeLayer| {
        let mut p = super::pathops::shape_node(d, path.clone(), None);
        p.appearance = Appearance { items: vec![AppearanceItem::Stroke(st.clone())], ..Default::default() };
        if let NodeKind::Path { rule: r, .. } | NodeKind::Compound { rule: r, .. } = &mut p.kind {
            *r = rule;
        }
        Some(p)
    };
    effects::expand_leaf(out, n, &mut stroke_piece)
}

/// Symbol instance `n` as a group of its symbol's art (placed, and tinted as the instance tints
/// it) with the instance's appearance and transparency, which PDF export draws the same: the
/// instance alone has no size to render its effects' image over. `None` for other objects.
fn placed_symbol(d: &Document, n: &Node) -> Option<Node> {
    let NodeKind::SymbolInstance { symbol, xf } = &n.kind else { return None };
    let mut art = vectorcraft_render::instance_art(&d.symbols.iter().find(|s| s.name == *symbol)?.art, n);
    art.transform(*xf, false);
    Some(Node { kind: NodeKind::Group { children: vec![Arc::new(art)], clip: false }, ..n.clone() })
}

/// Does `n` hold raster effects PDF export draws: its own, its fills' and strokes', its opacity
/// mask's or (groups and layers) its members'? Hidden objects and template layers aren't drawn.
fn needs(n: &Node) -> bool {
    n.visible
        && !matches!(n.kind, NodeKind::Layer { template: true, .. })
        && (!raster_fx(n).is_empty()
            || n.mask.as_ref().is_some_and(|m| needs(&m.art))
            || (is_container(n) && n.children().is_some_and(|ch| ch.iter().any(|c| needs(c)))))
}

/// The art of `n`'s opacity mask with its raster effects as images.
fn walk_mask(out: &mut Document, n: &mut Node) {
    if let Some(m) = n.mask.as_mut()
        && let Some(art) = walk(out, &m.art)
    {
        m.art = Arc::new(art);
    }
}

/// `n` with its raster effects (and its members' and its opacity mask's) as images, `None` when it
/// has none. An object whose effects all paint below it keeps its vector art over an image of
/// them; one whose effects change it (blur, feather, inner glow) becomes the image.
fn walk(out: &mut Document, n: &Node) -> Option<Node> {
    if !needs(n) {
        return None;
    }
    if raster_fx(n).is_empty() {
        let mut m = n.clone();
        walk_mask(out, &mut m);
        if is_container(&m)
            && let Some(ch) = walk_all(out, n.children()?)
            && let Some(slot) = m.children_mut()
        {
            *slot = ch;
        }
        return Some(m);
    }
    if let Some(g) = placed_symbol(out, n) {
        return Some(walk(out, &g).unwrap_or(g));
    }
    let below = below_only(n);
    if !below && let Some(g) = split_items(out, n) {
        return Some(walk(out, &g).unwrap_or(g));
    }
    // An image that can't be made leaves the object as it is (the writer reports its effects).
    let image = raster_image(out, n, below, false)?;
    let mut v = n.clone();
    strip_raster(&mut v, false);
    let mut m = if below {
        let mut m = walk(out, &v).unwrap_or(v);
        put_below(out, &mut m, image);
        m
    } else {
        replace_with_image(v, image)
    };
    walk_mask(out, &mut m);
    Some(m)
}

/// `nodes` with their raster effects as images, `None` when none has any.
fn walk_all(out: &mut Document, nodes: &[Arc<Node>]) -> Option<Vec<Arc<Node>>> {
    nodes.iter().any(|n| needs(n)).then(|| nodes.iter().map(|n| walk(out, n).map(Arc::new).unwrap_or_else(|| n.clone())).collect())
}

/// A copy of `doc` with raster effects turned into images at the document's raster effects
/// resolution, or `None` when there are none: on every kind of object (paths, groups and layers,
/// type, images, symbol instances, live objects) and on single fills and strokes, in the layers,
/// opacity masks, symbol definitions and pattern tiles.
pub fn flatten_raster_effects(doc: &Document) -> Option<Document> {
    let any = |nodes: &[Arc<Node>]| nodes.iter().any(|n| needs(n));
    if !any(&doc.layers) && !doc.symbols.iter().any(|s| needs(&s.art)) && !doc.patterns.iter().any(|p| any(&p.art)) {
        return None;
    }
    // Geometry effects first, so the vector objects kept above shadows are final.
    let baked = effects::bake_document(doc);
    let src = baked.as_ref().unwrap_or(doc);
    let mut out = src.clone();
    if let Some(layers) = walk_all(&mut out, &src.layers) {
        out.layers = layers;
    }
    for i in 0..out.symbols.len() {
        if let Some(art) = out.symbols.get(i).map(|s| s.art.clone()).and_then(|a| walk(&mut out, &a))
            && let Some(s) = out.symbols.get_mut(i)
        {
            s.art = Arc::new(art);
        }
    }
    for i in 0..out.patterns.len() {
        if let Some(art) = out.patterns.get(i).map(|p| p.art.clone()).and_then(|a| walk_all(&mut out, &a))
            && let Some(p) = out.patterns.get_mut(i)
        {
            p.art = art;
        }
    }
    Some(out)
}

/// PDF bytes for `doc`, with raster effects rendered as images.
pub fn export_pdf(doc: &Document, opts: &vectorcraft_pdf::PdfOptions) -> std::result::Result<Vec<u8>, vectorcraft_pdf::PdfError> {
    export_pdf_with_report(doc, opts).map(|r| r.bytes)
}

/// Like [`export_pdf`], also returning the writer's warnings (options not applied yet, features
/// approximated or left out).
pub fn export_pdf_with_report(
    doc: &Document,
    opts: &vectorcraft_pdf::PdfOptions,
) -> std::result::Result<vectorcraft_pdf::ExportReport, vectorcraft_pdf::PdfError> {
    match flatten_raster_effects(doc) {
        Some(d) => vectorcraft_pdf::export_with_report(&d, opts),
        None => vectorcraft_pdf::export_with_report(doc, opts),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vectorcraft_doc::NodeKind;

    use crate::Session;

    #[test]
    fn settings_and_pdf_flattening() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
        assert_eq!(s.execute("document.rasterEffectsSettings", &json!({})).unwrap()["resolution"], json!(72.0));
        s.execute("document.rasterEffectsSettings", &json!({"resolution": "high"})).unwrap();
        assert!(s.execute("document.rasterEffectsSettings", &json!({"resolution": 0})).is_err());
        let r = s.execute("shape.rectangle", &json!({"x": 50, "y": 50, "width": 100, "height": 100})).unwrap()["id"].as_u64().unwrap();
        let b = s.execute("shape.ellipse", &json!({"x": 180, "y": 50, "width": 80, "height": 80})).unwrap()["id"].as_u64().unwrap();
        s.execute("effect.apply", &json!({"effect": "stylize.dropShadow", "ids": [r]})).unwrap();
        s.execute("effect.apply", &json!({"effect": "blur.gaussian", "params": {"radius": 4}, "ids": [b]})).unwrap();
        let doc = s.doc().unwrap().doc.clone();
        let flat = super::flatten_raster_effects(&doc).unwrap();
        let kids = flat.layers[0].children().unwrap();
        // Shadow: an image (300 ppi) under the vector rectangle, which keeps no raster effect.
        let NodeKind::Group { children, .. } = &kids[0].kind else { panic!("shadow becomes a group: {:?}", kids[0].kind_label()) };
        let NodeKind::Image(im) = &children[0].kind else { panic!() };
        assert!(im.width as f64 > 100.0 * 300.0 / 72.0);
        assert!(children[1].path_data().is_some() && children[1].appearance.effects.is_empty());
        // Blur changes the object itself: an image alone.
        assert!(matches!(kids[1].kind, NodeKind::Image(_)));
        let pdf = super::export_pdf(&doc, &Default::default()).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Image"), "the PDF embeds the effect images");
        // The source document is untouched.
        assert!(s.doc().unwrap().doc.node(vectorcraft_doc::NodeId(r)).unwrap().appearance.effects.len() == 1);
    }
}
