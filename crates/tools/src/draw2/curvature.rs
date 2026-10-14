//! The Curvature tool (Shift+~).
//!
//! Each click adds a point and the path curves smoothly through all points. Alt-click or
//! double-click a point toggles it between smooth and corner; drag a point to move it; click the
//! first point to close; Backspace/Delete removes the last touched point; Esc/Enter ends the path.
//! A rubber band shows the curve to the cursor (Enable Rubber Band for Curvature Tool).

use serde_json::{Value, json};
use vectorcraft_doc::NodeId;
use vectorcraft_geom::Point;

use super::{FEEDBACK, catmull_rom};
use crate::{Action, Cursor, Mods, Overlay, PointerEvent, PointerKind, Tool, ToolContext, ToolKey};

#[derive(Default)]
pub struct CurvatureTool {
    pts: Vec<(Point, bool)>,
    closed: bool,
    id: Option<NodeId>,
    /// Point being dragged: (index, began).
    drag: Option<(usize, Point, bool)>,
    /// Last touched point (Backspace removes it).
    current: Option<usize>,
    hover: Option<Point>,
}

impl CurvatureTool {
    /// The path being drawn, if it is still the single selected object and matches our points.
    fn active(&self, cx: &ToolContext) -> Option<NodeId> {
        let id = self.id?;
        if cx.selection.objects != [id] {
            return None;
        }
        let pd = cx.doc.node(id)?.path_data()?;
        (pd.subpaths.len() == 1 && pd.anchor_count() == self.pts.len()).then_some(id)
    }

    fn params(&self) -> Value {
        let pts: Vec<Value> = self.pts.iter().map(|(p, c)| json!({"x": p.x, "y": p.y, "corner": c})).collect();
        let mut v = json!({"points": pts, "closed": self.closed});
        if let Some(id) = self.id {
            v["id"] = json!(id.0);
        }
        v
    }

    fn hit_point(&self, cx: &ToolContext, p: Point) -> Option<usize> {
        let tol = cx.tol(5.0);
        self.pts.iter().position(|(q, _)| q.distance(p) <= tol)
    }

    fn reset(&mut self) {
        self.pts.clear();
        self.closed = false;
        self.id = None;
        self.drag = None;
        self.current = None;
    }
}

impl Tool for CurvatureTool {
    fn id(&self) -> &'static str {
        "curvature"
    }
    fn busy(&self) -> bool {
        self.drag.is_some()
    }
    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        let p = ev.pos;
        let active = self.active(cx);
        if active.is_none() && self.id.is_some() && matches!(ev.kind, PointerKind::Down) {
            self.reset();
        }
        match ev.kind {
            PointerKind::Move => {
                self.hover = Some(p);
                vec![]
            }
            PointerKind::Down => {
                if active.is_some() {
                    if let Some(i) = self.hit_point(cx, p) {
                        self.current = Some(i);
                        if ev.mods.alt {
                            self.pts[i].1 = !self.pts[i].1;
                            return vec![Action::Exec("path.curvature".into(), self.params())];
                        }
                        if i == 0 && !self.closed && self.pts.len() >= 3 {
                            self.closed = true;
                            return vec![Action::Exec("path.curvature".into(), self.params())];
                        }
                        self.drag = Some((i, p, false));
                        return vec![];
                    }
                    if !self.closed {
                        self.pts.push((p, ev.mods.alt));
                        self.current = Some(self.pts.len() - 1);
                        return vec![Action::Exec("path.curvature".into(), self.params())];
                    }
                }
                // Start a new path.
                self.reset();
                self.pts.push((p, false));
                self.current = Some(0);
                vec![Action::Exec("path.curvature".into(), self.params()), Action::Notify("created".into())]
            }
            PointerKind::Drag => {
                let Some((i, start, began)) = self.drag else { return vec![] };
                let mut out = vec![];
                if !began {
                    if p.distance(start) < cx.tol(3.0) {
                        return out;
                    }
                    out.push(Action::Begin("Curvature".into()));
                    self.drag = Some((i, start, true));
                }
                if let Some(pt) = self.pts.get_mut(i) {
                    pt.0 = p;
                }
                out.push(Action::Preview("path.curvature".into(), self.params()));
                out
            }
            PointerKind::Up => match self.drag.take() {
                Some((_, _, true)) => vec![Action::Commit],
                _ => vec![],
            },
            PointerKind::DoubleClick => {
                if active.is_none() {
                    return vec![];
                }
                let Some(i) = self.hit_point(cx, p) else { return vec![] };
                self.pts[i].1 = !self.pts[i].1;
                vec![Action::Exec("path.curvature".into(), self.params())]
            }
        }
    }
    fn notify(&mut self, cx: &ToolContext, what: &str) {
        if what == "created" && cx.selection.objects.len() == 1 {
            self.id = Some(cx.selection.objects[0]);
        }
    }
    fn key(&mut self, cx: &ToolContext, key: ToolKey, _m: Mods) -> Vec<Action> {
        match key {
            ToolKey::Escape | ToolKey::Enter => {
                let dragging = self.drag.is_some_and(|d| d.2);
                self.reset();
                if dragging { vec![Action::Cancel] } else { vec![] }
            }
            ToolKey::Backspace | ToolKey::Delete if self.active(cx).is_some() && self.pts.len() > 1 && self.drag.is_none() => {
                let i = self.current.unwrap_or(self.pts.len() - 1).min(self.pts.len() - 1);
                self.pts.remove(i);
                if self.pts.len() < 3 {
                    self.closed = false;
                }
                self.current = None;
                vec![Action::Exec("path.curvature".into(), self.params())]
            }
            _ => vec![],
        }
    }
    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        let dragging = self.drag.is_some_and(|d| d.2);
        self.reset();
        if dragging { vec![Action::Commit] } else { vec![] }
    }
    fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        if self.active(cx).is_none() {
            return vec![];
        }
        let mut o = vec![];
        if cx.curvature_rubber_band
            && !self.closed
            && self.drag.is_none()
            && let Some(h) = self.hover
        {
            let mut pts = self.pts.clone();
            pts.push((h, false));
            let sp = catmull_rom(&pts, false);
            let mut bp = vectorcraft_geom::BezPath::new();
            sp.to_bezpath_into(&mut bp);
            o.push(Overlay::Path { path: bp, color: FEEDBACK, width: 1.0, dashed: false });
        }
        for (i, (p, _)) in self.pts.iter().enumerate() {
            o.push(Overlay::Anchor { p: *p, color: FEEDBACK, filled: Some(i) == self.current, size: 6.0 });
        }
        o
    }
    fn cursor(&self, _cx: &ToolContext, _p: Point, _m: Mods) -> Cursor {
        Cursor::Pen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;
    use vectorcraft_doc::Selection;

    #[test]
    fn first_click_creates_then_notify_tracks_path() {
        let (d, id) = doc_with_rect();
        let s = Selection::default();
        let p = paint();
        let cx = cx(&d, &s, &p);
        let mut t = CurvatureTool::default();
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 10.0, 10.0));
        assert!(matches!(&a[0], Action::Exec(c, v) if c == "path.curvature" && v.get("id").is_none() && v["points"].as_array().unwrap().len() == 1));
        assert_eq!(a[1], Action::Notify("created".into()));
        // Pretend the engine created `id` with one anchor: it is a rect (4 anchors) so it doesn't match.
        let mut s2 = Selection::default();
        s2.set([id]);
        let cx2 = crate::testutil::cx(&d, &s2, &p);
        t.notify(&cx2, "created");
        assert_eq!(t.id, Some(id));
        assert!(t.active(&cx2).is_none());
        // With four points it matches, so a click appends a fifth.
        t.pts = vec![(Point::new(100.0, 100.0), false); 4];
        let a = t.pointer(&cx2, &PointerEvent::new(PointerKind::Down, 400.0, 400.0));
        assert!(matches!(&a[0], Action::Exec(_, v) if v["id"] == id.0 && v["points"].as_array().unwrap().len() == 5));
    }
}
