//! Live Corners: the widgets inside the corners of a selected live rectangle. Dragging one rounds
//! (or sharpens) the corners whose widgets show together: all four when the whole shape is
//! selected, those with a selected anchor when Direct Selection picked some. Alt-clicking a widget
//! cycles their corner kind (round, inverted round, chamfer); double-clicking one opens the
//! Corners dialog. The Selection and Direct Selection tools share this, and the canvas draws the
//! widgets from the same geometry.

use serde_json::{Map, Value, json};
use vectorcraft_doc::{Document, LiveShape, NodeId, NodeKind, Selection};
use vectorcraft_geom::shapes::CornerKind;
use vectorcraft_geom::{Affine, Point, Vec2};

use crate::{Action, Overlay, PointerEvent, ToolContext};

/// The dialog a double-click on a corner widget opens: `{id, corners}`.
pub const DIALOG: &str = "corners";

/// Widgets sit at least this far inside their corner (screen px), further in once the radius is.
const MIN_INSET_PX: f64 = 10.0;
/// Shapes whose shorter side is smaller than this on screen (px) hide their widgets.
const MIN_SIDE_PX: f64 = 3.0 * MIN_INSET_PX;
/// Corners in radii order (top-left, top-right, bottom-right, bottom-left): position as factors
/// of (w, h) and the inward diagonal.
const CORNERS: [((f64, f64), (f64, f64)); 4] =
    [((0.0, 0.0), (1.0, 1.0)), ((1.0, 0.0), (-1.0, 1.0)), ((1.0, 1.0), (-1.0, -1.0)), ((0.0, 1.0), (1.0, -1.0))];

/// The corner widgets of a selection that is exactly one editable live rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerWidgets {
    pub id: NodeId,
    w: f64,
    h: f64,
    radii: [f64; 4],
    kinds: [CornerKind; 4],
    xf: Affine,
    /// Widget centres in document coordinates, in radii order.
    pub points: [Point; 4],
    /// The corners whose widgets show (and which a drag edits).
    pub shown: [bool; 4],
}

impl CornerWidgets {
    /// The widgets at `zoom` (screen px per document point), if the selection has them.
    pub fn of(doc: &Document, selection: &Selection, zoom: f64) -> Option<Self> {
        let [id] = selection.objects[..] else { return None };
        if !doc.is_editable(id) {
            return None;
        }
        let Some(NodeKind::Path { live: Some(live), .. }) = doc.node(id).map(|n| &n.kind) else { return None };
        let LiveShape::Rectangle { w, h, radii, kinds, xf } = live else { return None };
        let (w, h, radii, kinds, xf) = (*w, *h, *radii, *kinds, *xf);
        // Screen pixels per shape unit along each side (the shape may be scaled or skewed).
        let c = xf.as_coeffs();
        let (px, py) = (Vec2::new(c[0], c[1]).hypot() * zoom, Vec2::new(c[2], c[3]).hypot() * zoom);
        if xf.determinant().abs() < 1e-12 || (w * px).min(h * py) < MIN_SIDE_PX {
            return None;
        }
        let points = std::array::from_fn(|k| {
            let ((fx, fy), (sx, sy)) = CORNERS[k];
            let ix = radii[k].max(MIN_INSET_PX / px).min(w / 2.0);
            let iy = radii[k].max(MIN_INSET_PX / py).min(h / 2.0);
            xf * Point::new(fx * w + sx * ix, fy * h + sy * iy)
        });
        Some(Self { id, w, h, radii, kinds, xf, points, shown: live.picked_corners(selection.partial(id)) })
    }

    /// Selection & Anchor Display → Hide Corner Widget for angles greater than: the widgets of
    /// corners wider than `max` degrees (a skewed rectangle's obtuse ones) hide; none if all do.
    pub fn within_angle(mut self, max: f64) -> Option<Self> {
        // The angle between the shape's sides where they meet at the top-left corner; the
        // top-right and bottom-left corners are its supplement.
        let c = self.xf.as_coeffs();
        let (u, v) = (Vec2::new(c[0], c[1]), Vec2::new(c[2], c[3]));
        let a = u.cross(v).abs().atan2(u.dot(v)).to_degrees();
        for (k, shown) in self.shown.iter_mut().enumerate() {
            let angle = if k % 2 == 0 { a } else { 180.0 - a };
            *shown &= angle <= max + 1e-9;
        }
        self.shown.contains(&true).then_some(self)
    }

    /// The widgets the active tool can drag (View → Show Corner Widget on).
    pub fn for_tool(cx: &ToolContext) -> Option<Self> {
        if !cx.corner_widgets {
            return None;
        }
        Self::of(cx.doc, cx.selection, cx.zoom)?.within_angle(cx.corner_widget_max_angle)
    }

    /// The centres of the widgets that show.
    pub fn visible(&self) -> impl Iterator<Item = Point> + '_ {
        self.points.iter().zip(self.shown).filter(|(_, on)| *on).map(|(p, _)| *p)
    }

    /// The indices of the shown corners.
    fn shown_indices(&self) -> Value {
        (0..4).filter(|k| self.shown[*k]).collect()
    }

    /// Index of the shown widget nearest to `p` within `tol` (document units).
    pub fn hit(&self, p: Point, tol: f64) -> Option<usize> {
        let d = |k: &usize| self.points[*k].distance(p);
        (0..4).filter(|k| self.shown[*k] && d(k) <= tol).min_by(|a, b| d(a).total_cmp(&d(b)))
    }

    /// `object.setLiveShape` params setting `key` on the shown corners (all four unless some are
    /// hidden, which `corners` then leaves out).
    fn command(&self, key: &str, value: Value) -> Value {
        let mut p = Map::new();
        p.insert("id".into(), json!(self.id.0));
        p.insert(key.into(), value);
        if self.shown != [true; 4] {
            p.insert("corners".into(), self.shown_indices());
        }
        Value::Object(p)
    }
}

/// Is `p` over a corner widget the active tool would drag?
pub fn over_widget(cx: &ToolContext, p: Point) -> bool {
    CornerWidgets::for_tool(cx).and_then(|w| w.hit(p, cx.tol(5.0))).is_some()
}

/// A double-click on a corner widget opens the Corners dialog for the shown corners.
pub fn double_click(cx: &ToolContext, p: Point) -> Option<Action> {
    let w = CornerWidgets::for_tool(cx)?;
    w.hit(p, cx.tol(5.0))?;
    Some(Action::Dialog(DIALOG.into(), json!({ "id": w.id.0, "corners": w.shown_indices() })))
}

/// Dragging a corner widget: the radius follows the pointer along the corner's diagonal. An
/// Alt-click (no drag) cycles the corner kind instead.
#[derive(Clone, Copy, Debug)]
pub struct CornerDrag {
    widgets: CornerWidgets,
    corner: usize,
    start: Point,
    began: bool,
    alt: bool,
    radius: f64,
    at: Point,
}

impl CornerDrag {
    /// Start a drag when the press `ev` is on a widget of the selection.
    pub fn hit(cx: &ToolContext, ev: &PointerEvent) -> Option<Self> {
        let (widgets, p) = (CornerWidgets::for_tool(cx)?, ev.pos);
        let corner = widgets.hit(p, cx.tol(5.0))?;
        Some(Self { widgets, corner, start: p, began: false, alt: ev.mods.alt, radius: widgets.radii[corner], at: p })
    }

    /// The start radius (as drawn: no larger than fits) plus the pointer's travel along the
    /// corner's inward diagonal (in the shape's own units), from square to fully round.
    fn radius_at(&self, p: Point) -> f64 {
        let w = &self.widgets;
        let inv = w.xf.inverse();
        let d = inv * p - inv * self.start;
        let (_, (sx, sy)) = CORNERS[self.corner];
        let max = (w.w.abs().min(w.h.abs()) / 2.0).max(0.0);
        (w.radii[self.corner].min(max) + (d.x * sx + d.y * sy) / 2.0).clamp(0.0, max)
    }

    pub fn drag(&mut self, cx: &ToolContext, p: Point) -> Vec<Action> {
        let mut out = vec![];
        if !self.began {
            if p.distance(self.start) < cx.tol(3.0) {
                return out;
            }
            self.began = true;
            out.push(Action::Begin("Corner Radius".into()));
        }
        self.radius = self.radius_at(p);
        self.at = p;
        out.push(Action::Preview("object.setLiveShape".into(), self.widgets.command("radius", json!(self.radius))));
        out
    }

    pub fn finish(self) -> Vec<Action> {
        if self.began {
            vec![Action::Commit]
        } else if self.alt {
            let kind = self.widgets.kinds[self.corner].next();
            vec![Action::Exec("object.setLiveShape".into(), self.widgets.command("kind", json!(kind)))]
        } else {
            vec![]
        }
    }

    /// The radius readout next to the pointer.
    pub fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        if !self.began {
            return vec![];
        }
        vec![Overlay::Measure { p: self.at, text: format!("Radius: {}", cx.len(self.radius)) }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;
    use crate::{Mods, PointerKind};
    use vectorcraft_doc::{Appearance, Node};

    /// A live 100 × 100 rectangle at (100, 100) with corner radius `r`.
    fn live_rect(r: f64, xf: Affine) -> (Document, NodeId) {
        let mut d = Document::new(500.0, 500.0);
        let l = d.layers[0].id;
        let id = d.alloc_id();
        let live = LiveShape::Rectangle { w: 100.0, h: 100.0, radii: [r; 4], kinds: Default::default(), xf };
        let mut n = Node::path(id, live.to_path(), Appearance::default_art());
        if let NodeKind::Path { live: slot, .. } = &mut n.kind {
            *slot = Some(live);
        }
        d.insert(Some(l), 0, n).unwrap();
        (d, id)
    }

    fn selected(id: NodeId) -> Selection {
        let mut s = Selection::default();
        s.add(id);
        s
    }

    fn radius(a: &Action) -> f64 {
        let Action::Preview(c, v) = a else { panic!("not a preview: {a:?}") };
        assert_eq!(c, "object.setLiveShape");
        v["radius"].as_f64().unwrap()
    }

    #[test]
    fn widgets_sit_inside_each_corner() {
        let (d, id) = live_rect(0.0, Affine::translate((100.0, 100.0)));
        let w = CornerWidgets::of(&d, &selected(id), 1.0).unwrap();
        assert_eq!(w.points, [Point::new(110.0, 110.0), Point::new(190.0, 110.0), Point::new(190.0, 190.0), Point::new(110.0, 190.0)]);
        // A larger radius moves them to the arc centres; zooming in keeps a 10 px minimum.
        let (d, id) = live_rect(20.0, Affine::translate((100.0, 100.0)));
        assert_eq!(CornerWidgets::of(&d, &selected(id), 1.0).unwrap().points[0], Point::new(120.0, 120.0));
        assert_eq!(CornerWidgets::of(&d, &selected(id), 4.0).unwrap().points[2], Point::new(180.0, 180.0));
        // Hidden when the shape is tiny on screen, for plain paths and without a single selection.
        assert!(CornerWidgets::of(&d, &selected(id), 0.2).is_none());
        let (d, id) = doc_with_rect();
        assert!(CornerWidgets::of(&d, &selected(id), 1.0).is_none());
        assert!(CornerWidgets::of(&d, &Selection::default(), 1.0).is_none());
    }

    #[test]
    fn dragging_a_widget_inward_rounds_all_corners_in_one_step() {
        let (d, id) = live_rect(0.0, Affine::translate((100.0, 100.0)));
        let s = selected(id);
        let p = paint();
        let cx = cx(&d, &s, &p);
        for mut t in [crate::create("selection"), crate::create("directSelection")] {
            assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 190.0, 191.0)).is_empty());
            let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 175.0, 176.0));
            assert_eq!(a[0], Action::Begin("Corner Radius".into()));
            assert!((radius(&a[1]) - 15.0).abs() < 1e-9);
            assert!(matches!(&a[1], Action::Preview(_, v) if v["id"] == id.0));
            assert!(t.overlays(&cx).iter().any(|o| matches!(o, Overlay::Measure { text, .. } if text == "Radius: 15.00 pt")));
            // Past the middle the corner is fully round; back outside it is square.
            assert_eq!(radius(&t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 100.0, 100.0))[0]), 50.0);
            assert_eq!(radius(&t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 260.0, 260.0))[0]), 0.0);
            assert_eq!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, 260.0, 260.0)), vec![Action::Commit]);
        }
    }

    #[test]
    fn drag_follows_a_rotated_shape_and_a_click_does_nothing() {
        // Rotated 90°: the shape's top-left corner is at document (200, 100).
        let xf = Affine::translate((200.0, 100.0)) * Affine::rotate(std::f64::consts::FRAC_PI_2);
        let (d, id) = live_rect(10.0, xf);
        let s = selected(id);
        let p = paint();
        let cx = cx(&d, &s, &p);
        let w = CornerWidgets::of(&d, &s, 1.0).unwrap();
        assert!(w.points[0].distance(Point::new(190.0, 110.0)) < 1e-9);
        let mut t = crate::create("directSelection");
        t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 190.0, 110.0));
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 180.0, 120.0));
        assert!((radius(&a[1]) - 20.0).abs() < 1e-9);
        // A click without a drag leaves the shape (and the undo history) alone.
        let mut t = crate::create("selection");
        assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 190.0, 110.0)).is_empty());
        assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, 190.0, 110.0)).is_empty());
    }

    #[test]
    fn direct_selected_corners_show_their_widgets_alone_and_round_alone() {
        let (d, id) = live_rect(0.0, Affine::translate((100.0, 100.0)));
        // Direct Selection picked the bottom-right anchor.
        let mut s = selected(id);
        s.anchors.insert(id, [(0, 2)].into());
        let p = paint();
        let cx = cx(&d, &s, &p);
        let w = CornerWidgets::of(&d, &s, 1.0).unwrap();
        assert_eq!(w.shown, [false, false, true, false]);
        assert_eq!(w.visible().collect::<Vec<_>>(), [Point::new(190.0, 190.0)]);
        assert!(!over_widget(&cx, Point::new(110.0, 110.0)), "the other corners hide theirs");
        let mut t = crate::create("directSelection");
        assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 190.0, 191.0)).is_empty());
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 175.0, 176.0));
        assert_eq!(a[0], Action::Begin("Corner Radius".into()));
        assert_eq!(a[1], Action::Preview("object.setLiveShape".into(), json!({"id": id.0, "radius": 15.0, "corners": [2]})));
        assert_eq!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, 175.0, 176.0)), vec![Action::Commit]);
    }

    #[test]
    fn alt_click_cycles_the_kind_and_double_click_opens_the_dialog() {
        let (d, id) = live_rect(10.0, Affine::translate((100.0, 100.0)));
        let p = paint();
        let alt = Mods { alt: true, ..Mods::default() };
        let whole = selected(id);
        let mut part = selected(id);
        part.anchors.insert(id, [(0, 0), (0, 7)].into());
        for (s, extra) in [(&whole, json!({})), (&part, json!({"corners": [0]}))] {
            let cx = cx(&d, s, &p);
            let mut want = json!({"id": id.0, "kind": "invertedRound"});
            want.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            for tool in ["selection", "directSelection"] {
                let mut t = crate::create(tool);
                assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 110.0, 110.0).with_mods(alt)).is_empty());
                let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Up, 110.0, 110.0).with_mods(alt));
                assert_eq!(a, vec![Action::Exec("object.setLiveShape".into(), want.clone())], "{tool}");
            }
        }
        // A double-click on a widget opens Corners for the shown corners; elsewhere it doesn't.
        let cx1 = cx(&d, &part, &p);
        let a = crate::create("directSelection").pointer(&cx1, &PointerEvent::new(PointerKind::DoubleClick, 110.0, 110.0));
        assert_eq!(a, vec![Action::Dialog(DIALOG.into(), json!({"id": id.0, "corners": [0]}))]);
        let cx2 = cx(&d, &whole, &p);
        let a = crate::create("selection").pointer(&cx2, &PointerEvent::new(PointerKind::DoubleClick, 190.0, 110.0));
        assert_eq!(a, vec![Action::Dialog(DIALOG.into(), json!({"id": id.0, "corners": [0, 1, 2, 3]}))]);
        assert!(crate::create("selection").pointer(&cx2, &PointerEvent::new(PointerKind::DoubleClick, 150.0, 150.0)).is_empty());
    }

    /// Hide Corner Widget for angles greater than (#394): a rectangle's right angles hide below
    /// 90°; sheared, only its acute corners keep their widgets.
    #[test]
    fn corners_wider_than_the_preference_hide_their_widgets() {
        let (d, id) = live_rect(0.0, Affine::translate((100.0, 100.0)));
        let w = CornerWidgets::of(&d, &selected(id), 1.0).unwrap();
        assert_eq!(w.within_angle(177.0).map(|w| w.shown), Some([true; 4]));
        assert_eq!(w.within_angle(90.0).map(|w| w.shown), Some([true; 4]));
        assert!(w.within_angle(89.0).is_none());
        // Sheared by 30°: 60° at the top-left and bottom-right corners, 120° at the others.
        let shear = Affine::translate((100.0, 100.0)) * Affine::new([1.0, 0.0, 30f64.to_radians().tan(), 1.0, 0.0, 0.0]);
        let (d, id) = live_rect(0.0, shear);
        let w = CornerWidgets::of(&d, &selected(id), 1.0).unwrap();
        assert_eq!(w.within_angle(100.0).map(|w| w.shown), Some([true, false, true, false]));
        let s = selected(id);
        let p = paint();
        let c = ToolContext { corner_widget_max_angle: 100.0, ..cx(&d, &s, &p) };
        assert!(over_widget(&c, w.points[0]));
        assert!(!over_widget(&c, w.points[1]), "the 120° corner's widget is hidden");
    }

    #[test]
    fn widgets_off_or_elsewhere_keep_the_tools_behaviour() {
        let (d, id) = live_rect(0.0, Affine::translate((100.0, 100.0)));
        let s = selected(id);
        let p = paint();
        let mut c = cx(&d, &s, &p);
        assert!(over_widget(&c, Point::new(110.0, 110.0)));
        assert!(!over_widget(&c, Point::new(150.0, 150.0)));
        c.corner_widgets = false;
        assert!(!over_widget(&c, Point::new(110.0, 110.0)));
        // With the widgets hidden, a drag from the same spot moves the object.
        let mut t = crate::create("selection");
        t.pointer(&c, &PointerEvent::new(PointerKind::Down, 110.0, 110.0));
        let a = t.pointer(&c, &PointerEvent::new(PointerKind::Drag, 130.0, 110.0));
        assert_eq!(a[0], Action::Begin("Move".into()));
    }
}
