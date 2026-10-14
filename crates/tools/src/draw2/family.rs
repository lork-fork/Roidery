//! Line Segment family drag tools: Arc, Spiral, Rectangular Grid and Polar Grid.
//!
//! Drag draws (Shift = equal axes / square, Alt = from the centre for arc and grids, Space held
//! moves the shape being drawn); a click without dragging asks the UI for the options dialog.
//! While dragging, ↑/↓ change the spiral's segments, the grid rows or the concentric dividers; ←/→
//! change grid columns / radial dividers.

use serde_json::{Value, json};
use vectorcraft_geom::{Point, Rect, Vec2};

use crate::{Action, Cursor, Mods, Overlay, PointerEvent, PointerKind, Tool, ToolContext, ToolKey};

pub struct FamilyTool {
    id: &'static str,
    start: Option<Point>,
    last: Point,
    mods: Mods,
    began: bool,
    pub closed: bool,
    pub decay: f64,
    pub segments: u32,
    pub clockwise: bool,
    /// Rows / concentric dividers.
    pub rows: u32,
    /// Columns / radial dividers.
    pub columns: u32,
}

impl FamilyTool {
    pub fn new(id: &str) -> Self {
        let id: &'static str = match id {
            "spiral" => "spiral",
            "rectangularGrid" => "rectangularGrid",
            "polarGrid" => "polarGrid",
            _ => "arc",
        };
        Self {
            id,
            start: None,
            last: Point::ZERO,
            mods: Mods::default(),
            began: false,
            closed: false,
            decay: 80.0,
            segments: 10,
            clockwise: true,
            rows: 5,
            columns: 5,
        }
    }

    fn label(&self) -> &'static str {
        match self.id {
            "spiral" => "Spiral",
            "rectangularGrid" => "Rectangular Grid",
            "polarGrid" => "Polar Grid",
            _ => "Arc",
        }
    }

    fn rect(start: Point, p: Point, m: Mods) -> Rect {
        let mut d = p - start;
        if m.shift {
            let s = d.x.abs().max(d.y.abs());
            d = Vec2::new(s * d.x.signum(), s * d.y.signum());
        }
        if m.alt { Rect::from_points(start - d, start + d) } else { Rect::from_points(start, start + d) }
    }

    /// The command for a drag from `start` to `p`.
    pub fn command(&self, start: Point, p: Point, m: Mods) -> (String, Value) {
        match self.id {
            "spiral" => {
                let r = p.distance(start).max(0.01);
                (
                    "shape.spiral".into(),
                    json!({"cx": start.x, "cy": start.y, "radius": r, "decay": self.decay, "segments": self.segments, "clockwise": self.clockwise}),
                )
            }
            "rectangularGrid" => {
                let r = Self::rect(start, p, m);
                (
                    "shape.rectangularGrid".into(),
                    json!({"x": r.x0, "y": r.y0, "width": r.width(), "height": r.height(), "rows": self.rows, "columns": self.columns}),
                )
            }
            "polarGrid" => {
                let r = Self::rect(start, p, m);
                (
                    "shape.polarGrid".into(),
                    json!({"x": r.x0, "y": r.y0, "width": r.width(), "height": r.height(), "concentric": self.rows, "radial": self.columns}),
                )
            }
            _ => {
                let mut d = p - start;
                if m.shift {
                    let s = d.x.abs().max(d.y.abs());
                    d = Vec2::new(s * d.x.signum(), s * d.y.signum());
                }
                let (a, b) = if m.alt { (start - d, start + d) } else { (start, start + d) };
                ("shape.arc".into(), json!({"x1": a.x, "y1": a.y, "x2": b.x, "y2": b.y, "closed": self.closed}))
            }
        }
    }
}

impl Tool for FamilyTool {
    fn id(&self) -> &'static str {
        self.id
    }
    fn busy(&self) -> bool {
        self.start.is_some()
    }
    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        match ev.kind {
            PointerKind::Down => {
                let (p, _) = crate::guides::snap_draw(cx, ev.pos, &[]);
                self.start = Some(p);
                self.last = p;
                self.began = false;
                vec![]
            }
            PointerKind::Drag => {
                let Some(mut s) = self.start else { return vec![] };
                crate::shape::space_moves(&mut s, &mut self.last, ev, self.began);
                self.start = Some(s);
                self.mods = ev.mods;
                let mut out = vec![];
                if !self.began {
                    if ev.pos.distance(s) < cx.tol(2.0) {
                        return out;
                    }
                    self.began = true;
                    out.push(Action::Begin(self.label().into()));
                }
                let (c, v) = self.command(s, ev.pos, ev.mods);
                out.push(Action::Preview(c, v));
                out
            }
            PointerKind::Up => {
                let Some(s) = self.start.take() else { return vec![] };
                if std::mem::take(&mut self.began) { vec![Action::Commit] } else { vec![Action::Dialog(self.id.into(), json!({"x": s.x, "y": s.y}))] }
            }
            _ => vec![],
        }
    }
    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _m: Mods) -> Vec<Action> {
        let Some(s) = self.start.filter(|_| self.began) else { return vec![] };
        let changed = match (self.id, key) {
            ("spiral", ToolKey::Up) => {
                self.segments = (self.segments + 1).min(1000);
                true
            }
            ("spiral", ToolKey::Down) => {
                self.segments = self.segments.saturating_sub(1).max(2);
                true
            }
            ("rectangularGrid" | "polarGrid", ToolKey::Up) => {
                self.rows = (self.rows + 1).min(999);
                true
            }
            ("rectangularGrid" | "polarGrid", ToolKey::Down) => {
                self.rows = self.rows.saturating_sub(1);
                true
            }
            ("rectangularGrid" | "polarGrid", ToolKey::Right) => {
                self.columns = (self.columns + 1).min(999);
                true
            }
            ("rectangularGrid" | "polarGrid", ToolKey::Left) => {
                self.columns = self.columns.saturating_sub(1);
                true
            }
            _ => false,
        };
        if changed {
            let (c, v) = self.command(s, self.last, self.mods);
            vec![Action::Preview(c, v)]
        } else if key == ToolKey::Escape {
            self.start = None;
            self.began = false;
            vec![Action::Cancel]
        } else {
            vec![]
        }
    }
    fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        match self.start {
            Some(s) if self.began => {
                let d = self.last - s;
                vec![Overlay::Measure { p: self.last, text: cx.size_label(d.x.abs(), d.y.abs()) }]
            }
            _ => vec![],
        }
    }
    fn cursor(&self, _cx: &ToolContext, _p: Point, _m: Mods) -> Cursor {
        Cursor::Crosshair
    }
    fn options(&self) -> Value {
        match self.id {
            "spiral" => json!({"decay": self.decay, "segments": self.segments, "clockwise": self.clockwise}),
            "rectangularGrid" => json!({"rows": self.rows, "columns": self.columns}),
            "polarGrid" => json!({"concentric": self.rows, "radial": self.columns}),
            _ => json!({"closed": self.closed}),
        }
    }
    fn set_option(&mut self, key: &str, v: &Value) {
        match key {
            "closed" => self.closed = v.as_bool().unwrap_or(self.closed),
            "decay" => self.decay = v.as_f64().unwrap_or(self.decay).clamp(5.0, 150.0),
            "segments" => self.segments = v.as_u64().unwrap_or(10).clamp(2, 1000) as u32,
            "clockwise" => self.clockwise = v.as_bool().unwrap_or(self.clockwise),
            "rows" | "concentric" => self.rows = v.as_u64().unwrap_or(5).min(999) as u32,
            "columns" | "radial" => self.columns = v.as_u64().unwrap_or(5).min(999) as u32,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;
    use vectorcraft_doc::Selection;

    #[test]
    fn grid_drag_and_arrows() {
        let (d, _) = doc_with_rect();
        let s = Selection::default();
        let p = paint();
        let mut cx = cx(&d, &s, &p);
        cx.smart_guides = false;
        let mut t = FamilyTool::new("rectangularGrid");
        t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 10.0, 10.0));
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 60.0, 40.0));
        assert_eq!(a[0], Action::Begin("Rectangular Grid".into()));
        assert_eq!(
            a[1],
            Action::Preview("shape.rectangularGrid".into(), json!({"x": 10.0, "y": 10.0, "width": 50.0, "height": 30.0, "rows": 5, "columns": 5}))
        );
        let a = t.key(&cx, ToolKey::Right, Mods::default());
        assert!(matches!(&a[0], Action::Preview(_, v) if v["columns"] == 6));
        // Space held moves the grid at its size; let go, it grows again from there.
        let space = Mods { space: true, ..Mods::default() };
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 80.0, 50.0).with_mods(space));
        assert!(matches!(&a[..], [Action::Preview(_, v)] if v["x"] == 30.0 && v["y"] == 20.0 && v["width"] == 50.0 && v["height"] == 30.0), "{a:?}");
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, 90.0, 60.0));
        assert!(matches!(&a[..], [Action::Preview(_, v)] if v["x"] == 30.0 && v["width"] == 60.0 && v["height"] == 40.0), "{a:?}");
        assert_eq!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, 90.0, 60.0)), vec![Action::Commit]);
    }

    #[test]
    fn click_opens_dialog_and_arc_shift() {
        let (d, _) = doc_with_rect();
        let s = Selection::default();
        let p = paint();
        let mut cx = cx(&d, &s, &p);
        cx.smart_guides = false;
        let mut t = FamilyTool::new("spiral");
        t.pointer(&cx, &PointerEvent::new(PointerKind::Down, 10.0, 10.0));
        assert_eq!(
            t.pointer(&cx, &PointerEvent::new(PointerKind::Up, 10.0, 10.0)),
            vec![Action::Dialog("spiral".into(), json!({"x": 10.0, "y": 10.0}))]
        );
        let t = FamilyTool::new("arc");
        let (c, v) = t.command(Point::new(0.0, 0.0), Point::new(10.0, 4.0), Mods { shift: true, ..Default::default() });
        assert_eq!(c, "shape.arc");
        assert_eq!(v, json!({"x1": 0.0, "y1": 0.0, "x2": 10.0, "y2": 10.0, "closed": false}));
    }
}
