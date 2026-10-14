//! Hit testing over the document tree.

use vectorcraft_geom::hit::{fill_contains, stroke_contains};
use vectorcraft_geom::{Point, Rect};

use crate::{Document, Node, NodeId, NodeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    Fill,
    Stroke,
    /// Hit a path outline in outline mode, or an unfilled path's edge.
    Outline,
    Bounds,
}

/// The result of a hit test: the leaf that was hit and its ancestry (layer first).
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub leaf: NodeId,
    pub ancestry: Vec<NodeId>,
    pub kind: HitKind,
    /// The innermost envelope in the ancestry whose contents are being edited (Edit Contents):
    /// its content, not the envelope, is the object clicked.
    pub contents_of: Option<NodeId>,
    /// How many entries at the start of `ancestry` are layers (the layer and its sublayers).
    pub layers: usize,
}

impl Hit {
    /// The object the Selection tool selects: the child of the layer (the outermost group).
    /// Inside isolation mode (`scope`), the child of the isolated container instead.
    pub fn top_object(&self, scope: Option<NodeId>) -> NodeId {
        // The innermost of the isolated container and an envelope whose contents are edited.
        let at = |s: Option<NodeId>| s.and_then(|s| self.ancestry.iter().position(|x| *x == s));
        if let Some(i) = at(scope).max(at(self.contents_of)) {
            return self.ancestry.get(i + 1).copied().unwrap_or(self.leaf);
        }
        // Skip layers and sublayers: they are not objects.
        self.ancestry.get(self.layers.max(1)).copied().unwrap_or(self.leaf)
    }
}

/// Options for hit testing.
#[derive(Clone, Copy, Debug)]
pub struct HitOptions {
    /// Tolerance in document units (screen tolerance / zoom).
    pub tol: f64,
    /// Outline mode: only outlines are hittable.
    pub outline: bool,
    /// "Object Selection by Path Only" preference.
    pub path_only: bool,
    /// Roidery: a click inside an unfilled closed path picks it too, as does one between the two
    /// rails of a Satin Blocks object, and open paths take a click twice the tolerance away. These
    /// count only where no edge or fill is hit ("Select Inside Unfilled Shapes").
    pub inside: bool,
}

impl Default for HitOptions {
    fn default() -> Self {
        Self { tol: 3.0, outline: false, path_only: false, inside: false }
    }
}

/// Topmost editable object under `p`.
pub fn hit_test(doc: &Document, p: Point, opt: HitOptions) -> Option<Hit> {
    hit_test_skipping(doc, p, opt, &|_| false)
}

/// [`hit_test_skipping`] with Roidery's inside hits: an edge or fill anywhere wins, then the
/// inside of an unfilled shape.
fn hit_test_inside(doc: &Document, p: Point, opt: HitOptions, skip: &dyn Fn(NodeId) -> bool) -> Option<Hit> {
    let plain = HitOptions { inside: false, ..opt };
    hit_test_layers(doc, p, plain, skip).or_else(|| if opt.inside { hit_test_layers(doc, p, opt, skip) } else { None })
}

/// The objects under `p` that [`Hit::top_object`] picks in `scope` (isolation mode), topmost
/// first: what clicks there select, one below the other (Cmd/Ctrl-click selects behind).
pub fn objects_at(doc: &Document, p: Point, opt: HitOptions, scope: Option<NodeId>) -> Vec<NodeId> {
    let mut tops: Vec<NodeId> = vec![];
    // Each round leaves out the objects found so far (and everything inside them).
    while let Some(top) = hit_test_skipping(doc, p, opt, &|id| tops.contains(&id)).map(|h| h.top_object(scope)) {
        if tops.contains(&top) {
            break;
        }
        tops.push(top);
    }
    tops
}

/// [`hit_test`] passing over the nodes `skip` names (and their contents).
fn hit_test_skipping(doc: &Document, p: Point, opt: HitOptions, skip: &dyn Fn(NodeId) -> bool) -> Option<Hit> {
    hit_test_inside(doc, p, opt, skip)
}

fn hit_test_layers(doc: &Document, p: Point, opt: HitOptions, skip: &dyn Fn(NodeId) -> bool) -> Option<Hit> {
    let mut chain = Vec::new();
    for layer in doc.layers.iter().rev() {
        if !layer.visible || layer.locked {
            continue;
        }
        if layer.is_template() {
            continue;
        }
        // Pattern editing mode: only the tile art is editable.
        if doc.pattern_edit.as_ref().is_some_and(|e| e.layer != layer.id) {
            continue;
        }
        chain.push(layer.id);
        let opt = if matches!(layer.kind, NodeKind::Layer { preview: false, .. }) { HitOptions { outline: true, ..opt } } else { opt };
        if let Some(mut h) = hit_children(layer, p, opt, skip, &mut chain) {
            h.contents_of = h.ancestry.iter().rev().copied().find(|a| doc.node(*a).is_some_and(edits_contents));
            h.layers = h.ancestry.iter().take_while(|a| doc.node(**a).is_some_and(Node::is_layer)).count();
            return Some(h);
        }
        chain.pop();
    }
    None
}

/// Is `n` an envelope whose contents are being edited (they hit, not the envelope)?
fn edits_contents(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Envelope { editing: true, .. })
}

/// Is `p` inside the region `clip` clips to ([`Node::clip_shapes`])? Text counts by its frame:
/// glyph outlines need the font engine, above this crate.
fn clip_contains(clip: &Node, p: Point) -> bool {
    let frame = |n: &Node| {
        let b = n.geometric_bounds()?;
        Some(Node::path(n.id, vectorcraft_geom::shapes::rectangle(b), Default::default()))
    };
    clip.clip_shapes(Some(&frame)).iter().any(|(bp, rule)| fill_contains(bp, *rule, p))
}

fn hit_children(parent: &Node, p: Point, opt: HitOptions, skip: &dyn Fn(NodeId) -> bool, chain: &mut Vec<NodeId>) -> Option<Hit> {
    let children = parent.children()?;
    // A clip group (or a layer with a clipping mask) only hits inside its clipping path.
    if parent.clips()
        && let Some(clip) = children.first()
        && !clip_contains(clip, p)
    {
        return None;
    }
    for c in children.iter().rev() {
        // Hidden, locked and template sublayers block clicks like top-level layers do.
        if !c.visible || c.locked || c.is_template() || skip(c.id) {
            continue;
        }
        // (An envelope's content sits where it was, not where the envelope draws it.)
        if !edits_contents(c)
            && let Some(b) = c.reach_bounds()
            && !b.inflate(opt.tol, opt.tol).contains(p)
        {
            continue;
        }
        chain.push(c.id);
        let hit = match &c.kind {
            // A sublayer whose Preview is off is clicked in outline, like an outline view.
            NodeKind::Layer { preview: false, .. } => hit_children(c, p, HitOptions { outline: true, ..opt }, skip, chain),
            NodeKind::Layer { .. } | NodeKind::Group { .. } => hit_children(c, p, opt, skip, chain),
            // Edit Contents: the envelope's content hits, undistorted.
            NodeKind::Envelope { editing: true, .. } => hit_children(c, p, opt, skip, chain),
            // A blend's key objects first (Direct and Group Selection pick them); its steps hit
            // as the blend.
            NodeKind::Blend { .. } => hit_children(c, p, opt, skip, chain)
                .or_else(|| hit_leaf(c, p, opt).map(|kind| Hit { leaf: c.id, ancestry: chain.clone(), kind, contents_of: None, layers: 1 })),
            _ => hit_leaf(c, p, opt).map(|kind| Hit { leaf: c.id, ancestry: chain.clone(), kind, contents_of: None, layers: 1 }),
        };
        if hit.is_some() {
            return hit;
        }
        chain.pop();
    }
    None
}

/// A click on the strokes of a path or compound path `n` along `bp`: what any visible stroke
/// paints there (arrowheads, alignment, width profile, projecting caps, miter spikes), else its
/// outline within the tolerance. Outline mode hits the outline only.
fn hit_stroke(n: &Node, bp: &vectorcraft_geom::BezPath, rule: vectorcraft_geom::FillRule, p: Point, opt: HitOptions) -> Option<HitKind> {
    if !opt.outline && n.appearance.stroke_hit(bp, rule, p, opt.tol) {
        return Some(HitKind::Stroke);
    }
    stroke_contains(bp, 0.0, opt.tol, p).then_some(HitKind::Outline)
}

fn hit_leaf(n: &Node, p: Point, opt: HitOptions) -> Option<HitKind> {
    match &n.kind {
        NodeKind::Path { path, rule, .. } => {
            let bp = path.to_bezpath();
            if let Some(k) = hit_stroke(n, &bp, *rule, p, opt) {
                return Some(k);
            }
            let filled = !n.appearance.fill_paint().is_none();
            if !opt.outline && !opt.path_only && filled && fill_contains(&bp, *rule, p) {
                return Some(HitKind::Fill);
            }
            if opt.inside && !opt.outline && !filled {
                return hit_inside(path, &bp, p, opt.tol);
            }
            None
        }
        NodeKind::Compound { rule, .. } => {
            let bp = n.stroke_path()?;
            if let Some(k) = hit_stroke(n, &bp, *rule, p, opt) {
                return Some(k);
            }
            (!opt.outline && !opt.path_only && fill_contains(&bp, *rule, p)).then_some(HitKind::Fill)
        }
        NodeKind::Text(_) | NodeKind::Image(_) | NodeKind::SymbolInstance { .. } | NodeKind::Blend { .. } | NodeKind::Envelope { .. } => {
            n.geometric_bounds().filter(|b| b.inflate(opt.tol, opt.tol).contains(p)).map(|_| HitKind::Bounds)
        }
        NodeKind::Repeat(r) => r.expand().iter().find_map(|g| hit_any(g, p, opt)),
        NodeKind::Mesh(m) => {
            let bp = m.outline().to_bezpath();
            if stroke_contains(&bp, 0.0, opt.tol, p) {
                return Some(HitKind::Outline);
            }
            (!opt.path_only && fill_contains(&bp, vectorcraft_geom::FillRule::NonZero, p)).then_some(HitKind::Fill)
        }
        _ => None,
    }
}

/// Roidery's inside hit on an unfilled path: inside it when every subpath is closed, between the
/// rails of a two-rail path (Satin Blocks), else near it at twice the tolerance.
fn hit_inside(path: &vectorcraft_geom::PathData, bp: &vectorcraft_geom::BezPath, p: Point, tol: f64) -> Option<HitKind> {
    let subs = &path.subpaths;
    if !subs.is_empty() && subs.iter().all(|sp| sp.closed) {
        return fill_contains(bp, vectorcraft_geom::FillRule::EvenOdd, p).then_some(HitKind::Fill);
    }
    if let [l, r] = subs.as_slice()
        && !l.closed
        && !r.closed
        && l.anchors.len() >= 2
        && r.anchors.len() >= 2
    {
        let mut rails = vectorcraft_geom::BezPath::new();
        let mut pts = l.anchors.iter().map(|a| a.p).chain(r.anchors.iter().rev().map(|a| a.p));
        if let Some(first) = pts.next() {
            rails.move_to(first);
            pts.for_each(|q| rails.line_to(q));
            rails.close_path();
            if fill_contains(&rails, vectorcraft_geom::FillRule::NonZero, p) {
                return Some(HitKind::Fill);
            }
        }
    }
    stroke_contains(bp, 0.0, tol * 2.0, p).then_some(HitKind::Outline)
}

/// Hit anywhere in an evaluated subtree (groups recurse; leaves use [`hit_leaf`]).
fn hit_any(n: &Node, p: Point, opt: HitOptions) -> Option<HitKind> {
    match &n.kind {
        NodeKind::Group { children, .. } => children.iter().rev().find_map(|c| hit_any(c, p, opt)),
        _ => hit_leaf(n, p, opt),
    }
}

/// Objects (children of layers, or of `scope` in isolation mode) touched by a marquee rect.
pub fn marquee(doc: &Document, r: Rect, scope: Option<NodeId>, leaves: bool) -> Vec<NodeId> {
    let mut out = Vec::new();
    let tops: Vec<&std::sync::Arc<Node>> = match scope.and_then(|s| doc.node(s)) {
        Some(s) => s.children().map(|c| c.iter().collect()).unwrap_or_default(),
        // The objects of the layers, looking through sublayers (hidden, locked and template ones
        // are left out).
        None => doc
            .layers
            .iter()
            .filter(|l| l.visible && !l.locked && !l.is_template() && doc.pattern_edit.as_ref().is_none_or(|e| e.layer == l.id))
            .flat_map(|l| layer_objects(l))
            .collect(),
    };
    fn layer_objects(l: &Node) -> Vec<&std::sync::Arc<Node>> {
        let mut out = vec![];
        for c in l.children().into_iter().flatten() {
            if c.is_layer() {
                if c.visible && !c.locked && !c.is_template() {
                    out.extend(layer_objects(c));
                }
            } else {
                out.push(c);
            }
        }
        out
    }
    fn touches(n: &Node, r: Rect) -> bool {
        match &n.kind {
            NodeKind::Path { path, .. } => vectorcraft_geom::hit::intersects_rect(path, r),
            NodeKind::Layer { children, .. } | NodeKind::Group { children, .. } | NodeKind::Compound { children, .. } => {
                children.iter().any(|c| c.visible && touches(c, r))
            }
            _ => n.geometric_bounds().is_some_and(|b| b.intersect(r).area() > 0.0 || r.contains(b.origin())),
        }
    }
    fn collect_leaves(n: &Node, r: Rect, out: &mut Vec<NodeId>) {
        match &n.kind {
            NodeKind::Layer { children, .. } | NodeKind::Group { children, .. } => {
                for c in children {
                    if c.visible && !c.locked {
                        collect_leaves(c, r, out);
                    }
                }
            }
            _ => {
                if touches(n, r) {
                    out.push(n.id);
                }
            }
        }
    }
    for t in tops {
        if !t.visible || t.locked {
            continue;
        }
        if leaves {
            collect_leaves(t, r, &mut out);
        } else if touches(t, r) {
            out.push(t.id);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Appearance, Node};
    use std::sync::Arc;
    use vectorcraft_color::{Color, Paint};
    use vectorcraft_geom::shapes;

    #[test]
    fn hits_topmost() {
        let mut d = Document::new(200.0, 200.0);
        let l = d.layers[0].id;
        let a = d.alloc_id();
        d.insert(Some(l), 9, Node::path(a, shapes::rectangle(Rect::new(0.0, 0.0, 50.0, 50.0)), Appearance::default_art())).unwrap();
        let b = d.alloc_id();
        d.insert(Some(l), 9, Node::path(b, shapes::rectangle(Rect::new(25.0, 25.0, 75.0, 75.0)), Appearance::default_art())).unwrap();
        let h = hit_test(&d, Point::new(30.0, 30.0), HitOptions::default()).unwrap();
        assert_eq!(h.leaf, b);
        assert_eq!(h.kind, HitKind::Fill);
        let h = hit_test(&d, Point::new(10.0, 10.0), HitOptions::default()).unwrap();
        assert_eq!(h.leaf, a);
        assert!(hit_test(&d, Point::new(150.0, 150.0), HitOptions::default()).is_none());
        // Outline mode: interior misses.
        assert!(hit_test(&d, Point::new(10.0, 10.0), HitOptions { outline: true, ..Default::default() }).is_none());
        assert!(hit_test(&d, Point::new(0.5, 10.0), HitOptions { outline: true, ..Default::default() }).is_some());
    }

    #[test]
    fn unfilled_interior_misses() {
        let mut d = Document::new(200.0, 200.0);
        let l = d.layers[0].id;
        let a = d.alloc_id();
        d.insert(
            Some(l),
            9,
            Node::path(a, shapes::rectangle(Rect::new(0.0, 0.0, 50.0, 50.0)), Appearance::basic(Paint::None, Paint::solid(Color::BLACK), 1.0)),
        )
        .unwrap();
        assert!(hit_test(&d, Point::new(25.0, 25.0), HitOptions::default()).is_none());
        assert!(hit_test(&d, Point::new(50.5, 25.0), HitOptions::default()).is_some());
    }

    #[test]
    fn group_top_object() {
        let mut d = Document::new(200.0, 200.0);
        let l = d.layers[0].id;
        let a = d.alloc_id();
        let g = d.alloc_id();
        let p = Node::path(a, shapes::rectangle(Rect::new(0.0, 0.0, 50.0, 50.0)), Appearance::default_art());
        d.insert(Some(l), 0, Node::group(g, vec![Arc::new(p)])).unwrap();
        let h = hit_test(&d, Point::new(10.0, 10.0), HitOptions::default()).unwrap();
        assert_eq!(h.leaf, a);
        assert_eq!(h.top_object(None), g);
        assert_eq!(h.top_object(Some(g)), a);
        assert_eq!(marquee(&d, Rect::new(-5.0, -5.0, 5.0, 5.0), None, false), vec![g]);
        assert_eq!(marquee(&d, Rect::new(-5.0, -5.0, 5.0, 5.0), None, true), vec![a]);
        assert!(marquee(&d, Rect::new(100.0, 100.0, 105.0, 105.0), None, false).is_empty());
    }

    /// Art in a sublayer is an object of its own: a click or a marquee takes it, not the
    /// sublayer; hidden, locked and template sublayers block both.
    #[test]
    fn sublayers_are_not_objects() {
        let mut d = Document::new(200.0, 200.0);
        let l = d.layers[0].id;
        let sub = d.alloc_id();
        d.insert(Some(l), 0, Node::layer(sub, "Sub", crate::LayerColor::Preset(2))).unwrap();
        let inner = d.alloc_id();
        d.insert(Some(sub), 0, Node::layer(inner, "Inner", crate::LayerColor::Preset(3))).unwrap();
        let a = d.alloc_id();
        d.insert(Some(inner), 0, Node::path(a, shapes::rectangle(Rect::new(0.0, 0.0, 50.0, 50.0)), Appearance::default_art())).unwrap();
        let g = d.alloc_id();
        let b = d.alloc_id();
        let pb = Node::path(b, shapes::rectangle(Rect::new(100.0, 0.0, 150.0, 50.0)), Appearance::default_art());
        d.insert(Some(sub), 9, Node::group(g, vec![Arc::new(pb)])).unwrap();
        let h = hit_test(&d, Point::new(10.0, 10.0), HitOptions::default()).unwrap();
        assert_eq!((h.leaf, h.top_object(None), h.layers), (a, a, 3));
        let h = hit_test(&d, Point::new(110.0, 10.0), HitOptions::default()).unwrap();
        assert_eq!((h.top_object(None), h.top_object(Some(g))), (g, b));
        let all = Rect::new(-5.0, -5.0, 200.0, 200.0);
        assert_eq!(marquee(&d, all, None, false), vec![a, g]);
        assert_eq!(d.layer_containing(a), Some(inner));
        assert_eq!(d.layer_color(a), crate::LayerColor::Preset(3).rgb(), "the sublayer's own colour");
        assert_eq!(d.selectable_art(), vec![a, g]);
        for f in [
            |n: &mut Node| n.visible = false,
            |n: &mut Node| n.locked = true,
            |n: &mut Node| {
                if let NodeKind::Layer { template, .. } = &mut n.kind {
                    *template = true;
                }
            },
        ] {
            let mut d2 = d.clone();
            f(d2.node_mut(inner).unwrap());
            assert!(hit_test(&d2, Point::new(10.0, 10.0), HitOptions::default()).is_none());
            assert_eq!(marquee(&d2, all, None, false), vec![g]);
            assert_eq!(d2.selectable_art(), vec![g]);
        }
        // A sublayer whose Preview is off is clicked in outline: its fill no longer hits.
        if let NodeKind::Layer { preview, .. } = &mut d.node_mut(inner).unwrap().kind {
            *preview = false;
        }
        assert!(hit_test(&d, Point::new(25.0, 25.0), HitOptions::default()).is_none());
        assert!(hit_test(&d, Point::new(0.5, 25.0), HitOptions::default()).is_some());
    }

    /// Object Selection by Path Only: a filled path or compound path hits on its outline only.
    #[test]
    fn inside_picks_unfilled_shapes_and_satin_rails_after_edges_and_fills() {
        let mut d = Document::new(300.0, 200.0);
        let l = d.layers[0].id;
        let mut outline = Appearance::default_art();
        outline.set_fill(vectorcraft_color::Paint::None);
        // An unfilled square, a filled square inside it on the layer below, and two open rails.
        let (ring, small, rails) = (d.alloc_id(), d.alloc_id(), d.alloc_id());
        d.insert(Some(l), 0, Node::path(small, shapes::rectangle(Rect::new(40.0, 40.0, 60.0, 60.0)), Appearance::default_art())).unwrap();
        d.insert(Some(l), 9, Node::path(ring, shapes::rectangle(Rect::new(0.0, 0.0, 100.0, 100.0)), outline.clone())).unwrap();
        let two = vectorcraft_geom::PathData::new(vec![
            vectorcraft_geom::SubPath::polyline(&[Point::new(150.0, 10.0), Point::new(250.0, 10.0)], false),
            vectorcraft_geom::SubPath::polyline(&[Point::new(150.0, 40.0), Point::new(250.0, 40.0)], false),
        ]);
        d.insert(Some(l), 9, Node::path(rails, two, outline)).unwrap();
        let inside = HitOptions { inside: true, ..Default::default() };
        let at = |p: Point, o: HitOptions| hit_test(&d, p, o).map(|h| h.leaf);
        assert_eq!(at(Point::new(20.0, 20.0), HitOptions::default()), None, "Illustrator's rule: no fill, no hit");
        assert_eq!(at(Point::new(20.0, 20.0), inside), Some(ring));
        // The filled square below still wins where it is.
        assert_eq!(at(Point::new(50.0, 50.0), inside), Some(small));
        assert_eq!(at(Point::new(200.0, 25.0), inside), Some(rails), "between the rails");
        assert_eq!(at(Point::new(200.0, 60.0), inside), None);
        // An open line is easier to hit: twice the tolerance.
        assert_eq!(at(Point::new(200.0, 45.0), HitOptions::default()), None);
        assert_eq!(at(Point::new(200.0, 45.0), inside), Some(rails));
    }

    #[test]
    fn path_only_leaves_out_fills() {
        let mut d = Document::new(200.0, 200.0);
        let l = d.layers[0].id;
        let (a, c) = (d.alloc_id(), d.alloc_id());
        d.insert(Some(l), 0, Node::path(a, shapes::rectangle(Rect::new(0.0, 0.0, 50.0, 50.0)), Appearance::default_art())).unwrap();
        let inner = Node::path(d.alloc_id(), shapes::rectangle(Rect::new(100.0, 0.0, 150.0, 50.0)), Appearance::default());
        let mut compound = Node::new(c, NodeKind::Compound { children: vec![Arc::new(inner)], rule: vectorcraft_geom::FillRule::NonZero });
        compound.appearance = Appearance::default_art();
        d.insert(Some(l), 9, compound).unwrap();
        let path_only = HitOptions { path_only: true, ..Default::default() };
        for (id, inside, edge) in [(a, Point::new(25.0, 25.0), Point::new(0.5, 25.0)), (c, Point::new(125.0, 25.0), Point::new(100.5, 25.0))] {
            assert_eq!(hit_test(&d, inside, HitOptions::default()).map(|h| h.leaf), Some(id), "the fill hits");
            assert!(hit_test(&d, inside, path_only).is_none(), "path only: the fill of {id:?} doesn't hit");
            assert_eq!(hit_test(&d, edge, path_only).map(|h| h.leaf), Some(id), "path only: the outline does");
        }
    }

    /// `objects_at`: every object under the point, topmost first; a group counts once.
    #[test]
    fn objects_at_lists_the_stack() {
        let mut d = Document::new(200.0, 200.0);
        let l = d.layers[0].id;
        let rect =
            |d: &mut Document, x: f64| Node::path(d.alloc_id(), shapes::rectangle(Rect::new(x, 0.0, x + 50.0, 50.0)), Appearance::default_art());
        let a = rect(&mut d, 0.0);
        let (b, c) = (rect(&mut d, 10.0), rect(&mut d, 20.0));
        let (a_id, b_id, c_id, g) = (a.id, b.id, c.id, d.alloc_id());
        d.insert(Some(l), 0, a).unwrap();
        d.insert(Some(l), 9, Node::group(g, vec![Arc::new(b), Arc::new(c)])).unwrap();
        let far = rect(&mut d, 120.0);
        d.insert(Some(l), 9, far).unwrap();
        let at = |x: f64, scope| objects_at(&d, Point::new(x, 25.0), HitOptions::default(), scope);
        assert_eq!(at(30.0, None), vec![g, a_id]);
        assert_eq!(at(30.0, Some(g))[..2], [c_id, b_id], "isolated: the group's own objects");
        assert_eq!(at(5.0, None), vec![a_id]);
        assert!(at(90.0, None).is_empty());
    }

    #[test]
    fn locked_and_hidden_skip() {
        let mut d = Document::new(200.0, 200.0);
        let l = d.layers[0].id;
        let a = d.alloc_id();
        d.insert(Some(l), 0, Node::path(a, shapes::rectangle(Rect::new(0.0, 0.0, 50.0, 50.0)), Appearance::default_art())).unwrap();
        d.node_mut(a).unwrap().locked = true;
        assert!(hit_test(&d, Point::new(10.0, 10.0), HitOptions::default()).is_none());
        d.node_mut(a).unwrap().locked = false;
        d.node_mut(l).unwrap().visible = false;
        assert!(hit_test(&d, Point::new(10.0, 10.0), HitOptions::default()).is_none());
    }
}
