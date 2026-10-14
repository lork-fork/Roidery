//! Ruler guides on the canvas, for the selection tools. A press on a guide selects it (Shift adds
//! it to the selected guides or, released without a drag, takes it out); dragging moves the
//! selected guides (Alt copies them). The dragged guide snaps: with Shift to the ruler's ticks,
//! else as a drawn point does (whole pixels, the grid, or with Smart Guides the art's edges,
//! centres and anchors). Dropped off the canvas onto its ruler, the guide is deleted. Hidden or
//! locked guides (View › Guides) can't be picked.

use serde_json::json;
use vectorcraft_geom::Point;

use crate::guides::Targets;
use crate::{Action, Cursor, Overlay, PointerEvent, PointerKind, ToolContext};

/// The ruler guide under `p`: the nearest within the selection tolerance (the newest of equals).
pub fn guide_at(cx: &ToolContext, p: Point) -> Option<usize> {
    if !cx.guides {
        return None;
    }
    let tol = cx.pick_tol();
    let off = |pos: f64, vertical: bool| (if vertical { p.x } else { p.y } - pos).abs();
    cx.doc
        .guides
        .iter()
        .enumerate()
        .map(|(i, g)| (i, off(g.pos, g.vertical)))
        .filter(|(_, d)| *d <= tol)
        .min_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(i, _)| i)
}

/// The pointer for a guide: it moves sideways if vertical, up and down if horizontal.
fn guide_cursor(vertical: bool) -> Cursor {
    if vertical { Cursor::ResizeH } else { Cursor::ResizeV }
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    index: usize,
    vertical: bool,
    /// Where the guide was, and where it is now (x of a vertical guide, y of a horizontal one).
    from: f64,
    at: f64,
    start: Point,
    pointer: Point,
    began: bool,
    /// Shift-pressed on a selected guide: released without a drag, it leaves the selection.
    deselect: bool,
}

/// A selection tool's ruler-guide handling: [`Self::press`] on a press, [`Self::pointer`] for
/// the rest of the gesture.
#[derive(Default)]
pub struct GuideEdit {
    drag: Option<Drag>,
    /// Smart Guides targets, gathered when the drag starts.
    targets: Option<Targets>,
    /// The smart guide the dragged guide snapped to.
    snapped: Vec<Overlay>,
}

impl GuideEdit {
    /// Is a guide pressed or being dragged?
    pub fn busy(&self) -> bool {
        self.drag.is_some()
    }

    /// A press `ev` on a ruler guide: it gets selected and the drag starts. None (and nothing
    /// done) when no guide is there.
    pub fn press(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Option<Vec<Action>> {
        let index = guide_at(cx, ev.pos)?;
        let g = cx.doc.guides.get(index)?;
        let (shift, selected) = (ev.mods.shift, cx.selection.guides.contains(&index));
        self.drag = Some(Drag {
            index,
            vertical: g.vertical,
            from: g.pos,
            at: g.pos,
            start: ev.pos,
            pointer: ev.pos,
            began: false,
            deselect: shift && selected,
        });
        self.targets = None;
        self.snapped.clear();
        Some(match (selected, shift) {
            (true, _) => vec![],
            (false, true) => vec![Action::Exec("guide.select".into(), json!({ "indexes": [index], "toggle": true }))],
            (false, false) => vec![Action::Exec("guide.select".into(), json!({ "indexes": [index] }))],
        })
    }

    /// The drag and release of a guide [`Self::press`] picked; None when no guide is pressed.
    pub fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Option<Vec<Action>> {
        let mut d = self.drag?;
        Some(match ev.kind {
            PointerKind::Drag => {
                let out = self.drag_to(cx, &mut d, ev);
                self.drag = Some(d);
                out
            }
            PointerKind::Up => {
                self.drag = None;
                self.targets = None;
                self.snapped.clear();
                if d.began && on_ruler(cx, d.vertical, ev.pos) {
                    // Dropped on its ruler: the guide goes (a copy is just not made).
                    let mut out = vec![Action::Cancel];
                    if !ev.mods.alt {
                        out.push(Action::Exec("guide.remove".into(), json!({ "index": d.index })));
                    }
                    out
                } else if d.began {
                    vec![Action::Commit]
                } else if d.deselect {
                    vec![Action::Exec("guide.select".into(), json!({ "indexes": [d.index], "toggle": true }))]
                } else {
                    vec![]
                }
            }
            // Another press ends the gesture.
            _ => {
                self.drag = None;
                return None;
            }
        })
    }

    fn drag_to(&mut self, cx: &ToolContext, d: &mut Drag, ev: &PointerEvent) -> Vec<Action> {
        let p = ev.pos;
        let mut out = vec![];
        if !d.began {
            if p.distance(d.start) < cx.tol(3.0) {
                return out;
            }
            d.began = true;
            out.push(Action::Begin(if ev.mods.alt { "Copy Guide" } else { "Move Guide" }.into()));
            self.targets = cx.smart_guides.then(|| Targets::collect(cx.doc, &[], None));
        }
        let moved = p - d.start;
        let (at, snapped) = self.snap(cx, d.vertical, d.from + if d.vertical { moved.x } else { moved.y }, ev.mods.shift);
        (d.at, d.pointer, self.snapped) = (at, p, snapped);
        // The other selected guides follow the pointer: vertical ones across, horizontal ones down.
        let (dx, dy) = if d.vertical { (at - d.from, moved.y) } else { (moved.x, at - d.from) };
        out.push(Action::Preview("guide.move".into(), json!({ "dx": dx, "dy": dy, "copy": ev.mods.alt })));
        out
    }

    /// Where a guide dragged to `v` goes: with Shift on the nearest ruler tick, else on whole
    /// pixels, the grid or a smart guide (as a drawn point snaps).
    fn snap(&self, cx: &ToolContext, vertical: bool, v: f64, shift: bool) -> (f64, Vec<Overlay>) {
        if shift {
            return (cx.unit.snap_to_ruler_tick(v, cx.zoom), vec![]);
        }
        if cx.snap_to_pixel {
            return (v.round(), vec![]);
        }
        if cx.snap_to_grid {
            return (vectorcraft_geom::snap::snap_to_grid(v, cx.grid_step(), 0.0), vec![]);
        }
        match &self.targets {
            Some(t) => t.snap_guide(vertical, v, cx.tol(5.0)),
            None => (v, vec![]),
        }
    }

    /// The pointer over a guide, or while one is dragged.
    pub fn cursor(&self, cx: &ToolContext, p: Point) -> Option<Cursor> {
        if let Some(d) = &self.drag {
            return Some(guide_cursor(d.vertical));
        }
        guide_at(cx, p).and_then(|i| cx.doc.guides.get(i)).map(|g| guide_cursor(g.vertical))
    }

    /// While a guide is dragged: the smart guide it snapped to and its position by the pointer.
    pub fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        let Some(d) = self.drag.filter(|d| d.began) else { return vec![] };
        let mut o = self.snapped.clone();
        let axis = if d.vertical { "X" } else { "Y" };
        o.push(Overlay::Measure { p: d.pointer, text: format!("{axis}: {}", cx.len(d.at)) });
        o
    }
}

/// Is `p` off the canvas on the side of the ruler a guide comes from (the top one for a
/// horizontal guide, the left one for a vertical guide)? Never without a window.
fn on_ruler(cx: &ToolContext, vertical: bool, p: Point) -> bool {
    cx.screen.and_then(|s| s.to_screen(p)).is_some_and(|(x, y)| if vertical { x < 0.0 } else { y < 0.0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;
    use crate::{Mods, ScreenFrame};
    use vectorcraft_doc::{Guide, Selection};
    use vectorcraft_geom::Vec2;

    fn ev(kind: PointerKind, x: f64, y: f64, mods: Mods) -> PointerEvent {
        PointerEvent::new(kind, x, y).with_mods(mods)
    }

    fn guides_doc() -> vectorcraft_doc::Document {
        let (mut d, _) = doc_with_rect();
        d.guides = vec![Guide { vertical: true, pos: 300.0 }, Guide { vertical: false, pos: 400.0 }];
        d
    }

    #[test]
    fn picks_the_guide_within_the_tolerance_unless_hidden_or_locked() {
        let (d, s, p) = (guides_doc(), Selection::default(), paint());
        let c = cx(&d, &s, &p);
        assert_eq!(guide_at(&c, Point::new(302.0, 50.0)), Some(0));
        assert_eq!(guide_at(&c, Point::new(50.0, 398.0)), Some(1));
        // Where they cross, the nearer one.
        assert_eq!(guide_at(&c, Point::new(301.0, 398.0)), Some(0));
        assert_eq!(guide_at(&c, Point::new(305.0, 50.0)), None, "beyond the 3 px tolerance");
        let hidden = ToolContext { guides: false, ..cx(&d, &s, &p) };
        assert_eq!(guide_at(&hidden, Point::new(300.0, 50.0)), None);
    }

    #[test]
    fn a_drag_moves_the_selected_guides_in_one_step() {
        let d = guides_doc();
        let (s, p) = (Selection::default(), paint());
        let c = cx(&d, &s, &p);
        let mut g = GuideEdit::default();
        let m = Mods::default();
        assert_eq!(g.press(&c, &ev(PointerKind::Down, 300.0, 50.0, m)), Some(vec![Action::Exec("guide.select".into(), json!({"indexes": [0]}))]));
        // The tool sees the guide selected from now on.
        let mut s = s;
        s.set_guides([0]);
        let c = cx(&d, &s, &p);
        let a = g.pointer(&c, &ev(PointerKind::Drag, 340.0, 60.0, m)).unwrap();
        assert_eq!(a[0], Action::Begin("Move Guide".into()));
        assert_eq!(a[1], Action::Preview("guide.move".into(), json!({"dx": 40.0, "dy": 10.0, "copy": false})));
        assert!(matches!(g.overlays(&c).last(), Some(Overlay::Measure { text, .. }) if text == "X: 340.00 pt"));
        assert_eq!(g.cursor(&c, Point::ZERO), Some(Cursor::ResizeH));
        assert_eq!(g.pointer(&c, &ev(PointerKind::Up, 340.0, 60.0, m)), Some(vec![Action::Commit]));
        assert!(!g.busy() && g.pointer(&c, &ev(PointerKind::Drag, 0.0, 0.0, m)).is_none());
    }

    #[test]
    fn the_dragged_guide_snaps_to_art_ruler_ticks_and_the_grid() {
        let d = guides_doc();
        let mut s = Selection::default();
        s.set_guides([0]);
        let p = paint();
        let drag_to = |c: &ToolContext, x: f64, mods: Mods| {
            let mut g = GuideEdit::default();
            g.press(c, &ev(PointerKind::Down, 300.0, 50.0, Mods::default()));
            let a = g.pointer(c, &ev(PointerKind::Drag, x, 50.0, mods)).unwrap();
            let Some(Action::Preview(_, v)) = a.last() else { panic!("{a:?}") };
            (300.0 + v["dx"].as_f64().unwrap(), g.overlays(c))
        };
        // Smart Guides: onto the rect's right edge (x = 200), labelled.
        let (x, ov) = drag_to(&cx(&d, &s, &p), 203.0, Mods::default());
        assert_eq!(x, 200.0);
        assert!(matches!(&ov[0], Overlay::Label { .. }));
        let off = ToolContext { smart_guides: false, ..cx(&d, &s, &p) };
        assert_eq!(drag_to(&off, 203.0, Mods::default()).0, 203.0);
        // Shift: the ruler's ticks (labels every 50 pt at 100 %, ticks every 5 pt).
        assert_eq!(drag_to(&off, 223.0, Mods { shift: true, ..Default::default() }).0, 225.0);
        let zoomed = ToolContext { zoom: 5.0, ..cx(&d, &s, &p) };
        assert_eq!(drag_to(&zoomed, 223.4, Mods { shift: true, ..Default::default() }).0, 223.0, "every point at 500 %");
        let grid = ToolContext { snap_to_grid: true, ..cx(&d, &s, &p) };
        assert_eq!(drag_to(&grid, 226.0, Mods::default()).0, 225.0, "the grid's 9 pt subdivisions");
    }

    #[test]
    fn shift_click_toggles_and_alt_drag_copies() {
        let d = guides_doc();
        let mut s = Selection::default();
        s.set_guides([0, 1]);
        let p = paint();
        let c = cx(&d, &s, &p);
        let (shift, alt) = (Mods { shift: true, ..Default::default() }, Mods { alt: true, ..Default::default() });
        let mut g = GuideEdit::default();
        assert_eq!(g.press(&c, &ev(PointerKind::Down, 50.0, 400.0, shift)), Some(vec![]));
        let deselect = Action::Exec("guide.select".into(), json!({"indexes": [1], "toggle": true}));
        assert_eq!(g.pointer(&c, &ev(PointerKind::Up, 50.0, 400.0, shift)), Some(vec![deselect]));
        g.press(&c, &ev(PointerKind::Down, 50.0, 400.0, alt));
        let a = g.pointer(&c, &ev(PointerKind::Drag, 50.0, 420.0, alt)).unwrap();
        assert_eq!(a[0], Action::Begin("Copy Guide".into()));
        assert_eq!(a[1], Action::Preview("guide.move".into(), json!({"dx": 0.0, "dy": 20.0, "copy": true})));
    }

    #[test]
    fn dropped_on_its_ruler_the_guide_is_deleted() {
        let d = guides_doc();
        let p = paint();
        let mut s = Selection::default();
        s.set_guides([1]);
        // The window shows the document from (0, 0) at 100 %: above y = 0 is the top ruler.
        let screen = ScreenFrame { origin: Point::ZERO, right: Vec2::new(1.0, 0.0), down: Vec2::new(0.0, 1.0), size: (800.0, 600.0) };
        let c = ToolContext { screen: Some(screen), ..cx(&d, &s, &p) };
        let m = Mods::default();
        let gesture = |to: Point, mods: Mods| {
            let mut g = GuideEdit::default();
            g.press(&c, &ev(PointerKind::Down, 50.0, 400.0, m));
            g.pointer(&c, &ev(PointerKind::Drag, to.x, to.y, mods));
            g.pointer(&c, &ev(PointerKind::Up, to.x, to.y, mods)).unwrap()
        };
        assert_eq!(gesture(Point::new(50.0, -6.0), m), vec![Action::Cancel, Action::Exec("guide.remove".into(), json!({"index": 1}))]);
        assert_eq!(gesture(Point::new(50.0, -6.0), Mods { alt: true, ..m }), vec![Action::Cancel], "a copy is just not made");
        // Off the left side: not its ruler.
        assert_eq!(gesture(Point::new(-6.0, 300.0), m), vec![Action::Commit]);
        // Headless (no window): no ruler to drop it on.
        let mut g = GuideEdit::default();
        let c = cx(&d, &s, &p);
        g.press(&c, &ev(PointerKind::Down, 50.0, 400.0, m));
        g.pointer(&c, &ev(PointerKind::Drag, 50.0, -6.0, m));
        assert_eq!(g.pointer(&c, &ev(PointerKind::Up, 50.0, -6.0, m)), Some(vec![Action::Commit]));
    }
}
