//! Flare, Reshape, Shaper and Graph tools.
//!
//! Flare: drag sets the centre and its size (click without dragging opens the Flare Tool Options),
//! then a click sets the end point of the rings. Reshape: drag a point of a selected path; the
//! engine's `path.reshape` adds an anchor there if needed and moves the neighbourhood smoothly.

use serde_json::{Value, json};
use vectorcraft_geom::{Point, Vec2};

use crate::distort::{BLUE, ellipse_path};
use crate::{Action, Cursor, Mods, Overlay, PointerEvent, PointerKind, Tool, ToolContext, ToolKey};

pub fn create(id: &str) -> Option<Box<dyn Tool>> {
    Some(match id {
        "flare" => Box::new(FlareTool::default()),
        "reshape" => Box::new(ReshapeTool::default()),
        "shaper" => Box::new(ShaperTool::default()),
        id if vectorcraft_doc::GraphKind::parse(id).is_some() && id.ends_with("Graph") => Box::new(GraphTool::new(id)),
        _ => return None,
    })
}

/// Flare Tool: centre drag, then a click for the rings' end point.
#[derive(Default)]
pub struct FlareTool {
    /// Pressed at (centre) and the current drag point.
    drag: Option<(Point, Point)>,
    /// Centre placed, waiting for the end-point click: (centre, diameter).
    placed: Option<(Point, f64)>,
    hover: Point,
    began: bool,
    /// Ray count changed with ↑/↓ during the gesture (None = the command's default, 15).
    rays: Option<u64>,
}

impl FlareTool {
    fn params(&self, c: Point, diameter: f64, end: Option<Point>) -> Value {
        let mut v = json!({ "cx": c.x, "cy": c.y, "diameter": diameter.max(1.0) });
        if let Some(r) = self.rays {
            v["rays"] = json!(r);
        }
        if let Some(e) = end {
            v["x2"] = json!(e.x);
            v["y2"] = json!(e.y);
        }
        v
    }
}

impl Tool for FlareTool {
    fn id(&self) -> &'static str {
        "flare"
    }
    fn busy(&self) -> bool {
        self.drag.is_some() || self.placed.is_some()
    }
    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        self.hover = ev.pos;
        match ev.kind {
            PointerKind::Down => {
                if self.placed.is_none() {
                    self.drag = Some((ev.pos, ev.pos));
                }
                vec![]
            }
            PointerKind::Drag => {
                if let Some((c, _)) = self.drag {
                    self.drag = Some((c, ev.pos));
                    if !self.began && ev.pos.distance(c) >= cx.tol(3.0) {
                        self.began = true;
                        return vec![
                            Action::Begin("Flare".into()),
                            Action::Preview("shape.flare".into(), self.params(c, 2.0 * ev.pos.distance(c), None)),
                        ];
                    }
                    if self.began {
                        return vec![Action::Preview("shape.flare".into(), self.params(c, 2.0 * ev.pos.distance(c), None))];
                    }
                } else if let Some((c, d)) = self.placed {
                    return vec![Action::Preview("shape.flare".into(), self.params(c, d, Some(ev.pos)))];
                }
                vec![]
            }
            PointerKind::Up => {
                if let Some((c, d)) = self.placed.take() {
                    self.began = false;
                    return vec![Action::Preview("shape.flare".into(), self.params(c, d, Some(ev.pos))), Action::Commit];
                }
                let Some((c, p)) = self.drag.take() else { return vec![] };
                if self.began {
                    // Keep the interaction open: the next click places the end point.
                    self.placed = Some((c, 2.0 * p.distance(c)));
                    vec![]
                } else {
                    vec![Action::Dialog("flare".into(), json!({ "x": c.x, "y": c.y }))]
                }
            }
            PointerKind::Move => {
                if let Some((c, d)) = self.placed {
                    return vec![Action::Preview("shape.flare".into(), self.params(c, d, Some(ev.pos)))];
                }
                vec![]
            }
            _ => vec![],
        }
    }
    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        // ↑/↓ while drawing add/remove rays.
        if matches!(key, ToolKey::Up | ToolKey::Down) && self.began {
            let r = self.rays.unwrap_or(15);
            self.rays = Some(if key == ToolKey::Up { (r + 1).min(50) } else { r.saturating_sub(1) });
            let preview = match (self.drag, self.placed) {
                (Some((c, p)), _) => self.params(c, 2.0 * p.distance(c), None),
                (_, Some((c, d))) => self.params(c, d, Some(self.hover)),
                _ => return vec![],
            };
            return vec![Action::Preview("shape.flare".into(), preview)];
        }
        if key == ToolKey::Escape && self.busy() {
            self.rays = None;
            self.drag = None;
            self.placed = None;
            let began = std::mem::take(&mut self.began);
            return if began { vec![Action::Cancel] } else { vec![] };
        }
        vec![]
    }
    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        // Switching away after the centre drag keeps the flare with its default rings.
        self.drag = None;
        if self.placed.take().is_some() && std::mem::take(&mut self.began) { vec![Action::Commit] } else { vec![] }
    }
    fn overlays(&self, _cx: &ToolContext) -> Vec<Overlay> {
        match (self.drag, self.placed) {
            (Some((c, p)), _) => {
                vec![Overlay::Path { path: ellipse_path(c, p.distance(c), p.distance(c), 0.0), color: BLUE, width: 1.0, dashed: false }]
            }
            (_, Some((c, _))) => vec![Overlay::Line { a: c, b: self.hover, color: BLUE, dashed: true }],
            _ => vec![],
        }
    }
    fn cursor(&self, _cx: &ToolContext, _p: Point, _mods: Mods) -> Cursor {
        Cursor::Crosshair
    }
}

/// Reshape Tool: drag a point on a selected path.
#[derive(Default)]
pub struct ReshapeTool {
    /// (path, grab point) while dragging.
    grab: Option<(vectorcraft_doc::NodeId, Point)>,
    began: bool,
}

impl ReshapeTool {
    /// The selected path under `p` (Reshape works on selected paths only, like Illustrator).
    fn hit(cx: &ToolContext, p: Point) -> Option<(vectorcraft_doc::NodeId, Point)> {
        let tol = cx.tol(4.0);
        cx.selection.objects.iter().copied().filter(|id| cx.doc.is_editable(*id)).find_map(|id| {
            let pd = cx.doc.node(id)?.path_data()?;
            let (_, _, _, q, d) = pd.nearest(p)?;
            (d <= tol).then_some((id, q))
        })
    }
}

impl Tool for ReshapeTool {
    fn id(&self) -> &'static str {
        "reshape"
    }
    fn busy(&self) -> bool {
        self.grab.is_some()
    }
    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        match ev.kind {
            PointerKind::Down => {
                self.grab = Self::hit(cx, ev.pos);
                self.began = false;
                vec![]
            }
            PointerKind::Drag => {
                let Some((id, at)) = self.grab else { return vec![] };
                let mut d: Vec2 = ev.pos - at;
                if ev.mods.shift {
                    d = vectorcraft_geom::constrain_angle(d, 45.0);
                }
                let mut out = vec![];
                if !self.began {
                    if d.hypot() < cx.tol(2.0) {
                        return out;
                    }
                    self.began = true;
                    out.push(Action::Begin("Reshape".into()));
                }
                out.push(Action::Preview(
                    "path.reshape".into(),
                    json!({ "id": id.0, "x": at.x, "y": at.y, "dx": d.x, "dy": d.y, "tol": cx.tol(4.0) }),
                ));
                out
            }
            PointerKind::Up => {
                self.grab = None;
                if std::mem::take(&mut self.began) { vec![Action::Commit] } else { vec![] }
            }
            _ => vec![],
        }
    }
    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        if key == ToolKey::Escape && self.grab.take().is_some() && std::mem::take(&mut self.began) {
            return vec![Action::Cancel];
        }
        vec![]
    }
    fn cursor(&self, cx: &ToolContext, p: Point, _mods: Mods) -> Cursor {
        if self.grab.is_some() || Self::hit(cx, p).is_some() { Cursor::Move } else { Cursor::Arrow }
    }
}

/// Shaper Tool: draw a rough shape and it becomes a clean live shape (rectangle, ellipse,
/// triangle/polygon, line); scribble over art to delete it.
#[derive(Default)]
pub struct ShaperTool {
    points: Vec<Point>,
}

impl Tool for ShaperTool {
    fn id(&self) -> &'static str {
        "shaper"
    }
    fn busy(&self) -> bool {
        !self.points.is_empty()
    }
    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        use vectorcraft_geom::recognize::{Recognized, recognize};
        match ev.kind {
            PointerKind::Down => {
                self.points = vec![ev.pos];
                vec![]
            }
            PointerKind::Drag => {
                if !self.points.is_empty() && self.points.last().is_none_or(|l| l.distance(ev.pos) >= cx.tol(1.5)) {
                    self.points.push(ev.pos);
                }
                vec![]
            }
            PointerKind::Up => {
                let pts = std::mem::take(&mut self.points);
                let r = |x: vectorcraft_geom::Rect| json!({ "x": x.x0, "y": x.y0, "width": x.width(), "height": x.height() });
                match recognize(&pts) {
                    Some(Recognized::Line { a, b }) => vec![Action::Exec("shape.line".into(), json!({ "x1": a.x, "y1": a.y, "x2": b.x, "y2": b.y }))],
                    Some(Recognized::Rectangle(x)) => vec![Action::Exec("shape.rectangle".into(), r(x))],
                    Some(Recognized::Ellipse(x)) => vec![Action::Exec("shape.ellipse".into(), r(x))],
                    Some(Recognized::Polygon { center, radius, sides, rotation }) => {
                        vec![Action::Exec(
                            "shape.polygon".into(),
                            json!({ "cx": center.x, "cy": center.y, "radius": radius, "sides": sides, "rotation": rotation }),
                        )]
                    }
                    Some(Recognized::Scribble(_)) => {
                        // Delete the topmost objects the scribble crosses.
                        let mut ids: Vec<u64> = vec![];
                        for p in &pts {
                            if let Some(h) = vectorcraft_doc::hit::hit_test(cx.doc, *p, cx.hit_options()) {
                                let id = h.top_object(cx.isolation).0;
                                if !ids.contains(&id) {
                                    ids.push(id);
                                }
                            }
                        }
                        if ids.is_empty() {
                            return vec![];
                        }
                        vec![Action::Exec("select.set".into(), json!({ "ids": ids })), Action::Exec("edit.clear".into(), json!({ "ids": ids }))]
                    }
                    None => vec![],
                }
            }
            _ => vec![],
        }
    }
    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        if key == ToolKey::Escape {
            self.points.clear();
        }
        vec![]
    }
    fn overlays(&self, _cx: &ToolContext) -> Vec<Overlay> {
        if self.points.len() < 2 {
            return vec![];
        }
        vec![Overlay::Path { path: crate::draw2::polyline(&self.points), color: BLUE, width: 1.5, dashed: false }]
    }
    fn cursor(&self, _cx: &ToolContext, _p: Point, _mods: Mods) -> Cursor {
        Cursor::Crosshair
    }
}

/// Graph tools: drag the plot rectangle (Shift = square, Alt = from the centre); a click opens the
/// size dialog. A new graph opens the Graph Data dialog.
pub struct GraphTool {
    id: &'static str,
    start: Option<Point>,
    began: bool,
}

impl GraphTool {
    fn new(id: &str) -> Self {
        let id = crate::catalog::tool_info(id).map(|t| t.id).unwrap_or("columnGraph");
        Self { id, start: None, began: false }
    }
    fn kind(&self) -> &'static str {
        vectorcraft_doc::GraphKind::parse(self.id).unwrap_or_default().id()
    }
    fn params(&self, s: Point, p: Point, m: Mods) -> Value {
        let mut d = p - s;
        if m.shift {
            let k = d.x.abs().max(d.y.abs());
            d = Vec2::new(k * d.x.signum(), k * d.y.signum());
        }
        let r = if m.alt { vectorcraft_geom::Rect::from_points(s - d, s + d) } else { vectorcraft_geom::Rect::from_points(s, s + d) };
        json!({ "type": self.kind(), "x": r.x0, "y": r.y0, "width": r.width().max(1.0), "height": r.height().max(1.0) })
    }
}

impl Tool for GraphTool {
    fn id(&self) -> &'static str {
        self.id
    }
    fn busy(&self) -> bool {
        self.start.is_some()
    }
    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        match ev.kind {
            PointerKind::Down => {
                self.start = Some(ev.pos);
                self.began = false;
                vec![]
            }
            PointerKind::Drag => {
                let Some(s) = self.start else { return vec![] };
                let mut out = vec![];
                if !self.began {
                    if ev.pos.distance(s) < cx.tol(3.0) {
                        return out;
                    }
                    self.began = true;
                    out.push(Action::Begin("Graph".into()));
                }
                out.push(Action::Preview("graph.create".into(), self.params(s, ev.pos, ev.mods)));
                out
            }
            PointerKind::Up => {
                let Some(s) = self.start.take() else { return vec![] };
                if std::mem::take(&mut self.began) {
                    vec![
                        Action::Preview("graph.create".into(), self.params(s, ev.pos, ev.mods)),
                        Action::Commit,
                        Action::Dialog("graphData".into(), json!({})),
                    ]
                } else {
                    vec![Action::Dialog("graph".into(), json!({ "x": s.x, "y": s.y, "type": self.kind() }))]
                }
            }
            _ => vec![],
        }
    }
    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        if key == ToolKey::Escape && self.start.take().is_some() && std::mem::take(&mut self.began) {
            return vec![Action::Cancel];
        }
        vec![]
    }
    fn cursor(&self, _cx: &ToolContext, _p: Point, _mods: Mods) -> Cursor {
        Cursor::Crosshair
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_covers_both() {
        assert_eq!(create("flare").unwrap().id(), "flare");
        assert_eq!(create("reshape").unwrap().id(), "reshape");
        assert_eq!(create("shaper").unwrap().id(), "shaper");
        assert!(create("pen").is_none());
        for g in
            ["columnGraph", "stackedColumnGraph", "barGraph", "stackedBarGraph", "lineGraph", "areaGraph", "scatterGraph", "pieGraph", "radarGraph"]
        {
            assert_eq!(create(g).unwrap().id(), g);
        }
    }
}
