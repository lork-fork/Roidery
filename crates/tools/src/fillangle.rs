//! Roidery: the Fill Angle tool (2).
//!
//! Drag across the selected fills to set the direction their rows run: the rows follow the line
//! you draw, and the stitches update as you drag. Shift keeps the angle to steps of 15°. Escape
//! while dragging puts the angle back. Works on tatami, gradient and motif fills (contour and
//! circular fills turn their underlay).

use serde_json::json;
use vectorcraft_geom::Point;

use crate::{Action, Cursor, Mods, Overlay, PointerEvent, PointerKind, Tool, ToolContext, ToolKey};

/// The drawn line's colour.
const LINE: [u8; 3] = [255, 64, 160];
/// Drags shorter than this, in screen pixels, don't set an angle.
const MIN_DRAG_PX: f64 = 4.0;

#[derive(Default)]
pub struct FillAngleTool {
    /// Where the drag started, and where the pointer is now.
    drag: Option<(Point, Point)>,
    /// An interaction (the previewed angle) is open.
    begun: bool,
}

/// The rows' angle for a line from `a` to `b`: degrees counter-clockwise from the x axis as seen
/// on screen (the document's y runs down), 0 to 180 (a row runs both ways).
pub fn angle_of(a: Point, b: Point, snap: bool) -> f64 {
    let deg = (-(b.y - a.y)).atan2(b.x - a.x).to_degrees().rem_euclid(180.0);
    let deg = if snap { (deg / 15.0).round() * 15.0 } else { (deg * 10.0).round() / 10.0 };
    deg.rem_euclid(180.0)
}

impl Tool for FillAngleTool {
    fn id(&self) -> &'static str {
        "fillAngle"
    }

    fn busy(&self) -> bool {
        self.drag.is_some()
    }

    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        match ev.kind {
            PointerKind::Down if !cx.selection.objects.is_empty() => {
                self.drag = Some((ev.pos, ev.pos));
                vec![]
            }
            PointerKind::Drag => {
                let Some((a, _)) = self.drag else { return vec![] };
                self.drag = Some((a, ev.pos));
                if a.distance(ev.pos) * cx.zoom < MIN_DRAG_PX {
                    return vec![];
                }
                let mut acts = vec![];
                if !self.begun {
                    self.begun = true;
                    acts.push(Action::Begin("Fill Angle".into()));
                }
                acts.push(Action::Preview("stitch.setAngle".into(), json!({"angle": angle_of(a, ev.pos, ev.mods.shift)})));
                acts
            }
            PointerKind::Up => {
                self.drag = None;
                if std::mem::take(&mut self.begun) { vec![Action::Commit] } else { vec![] }
            }
            _ => vec![],
        }
    }

    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        if key == ToolKey::Escape && self.drag.take().is_some() && std::mem::take(&mut self.begun) {
            return vec![Action::Cancel];
        }
        vec![]
    }

    fn claims_key(&self, _cx: &ToolContext, key: ToolKey) -> bool {
        self.drag.is_some() && key == ToolKey::Escape
    }

    fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        let Some((a, b)) = self.drag else { return vec![] };
        if a.distance(b) * cx.zoom < MIN_DRAG_PX {
            return vec![];
        }
        vec![
            Overlay::Line { a, b, color: LINE, dashed: false },
            Overlay::Anchor { p: a, color: LINE, filled: true, size: 5.0 },
            Overlay::Measure { p: b, text: format!("{}°", angle_of(a, b, false)) },
        ]
    }

    fn cursor(&self, cx: &ToolContext, _p: Point, _mods: Mods) -> Cursor {
        if cx.selection.objects.is_empty() && self.drag.is_none() { Cursor::NotAllowed } else { Cursor::Crosshair }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_read_as_on_screen_and_snap() {
        let o = Point::new(0.0, 0.0);
        assert_eq!(angle_of(o, Point::new(10.0, 0.0), false), 0.0);
        // Up the screen is y going down in the document's numbers: 90°.
        assert_eq!(angle_of(o, Point::new(0.0, -10.0), false), 90.0);
        assert_eq!(angle_of(o, Point::new(10.0, -10.0), false), 45.0);
        // A row runs both ways: dragging the other way gives the same angle.
        assert_eq!(angle_of(Point::new(10.0, -10.0), o, false), 45.0);
        assert_eq!(angle_of(o, Point::new(10.0, -3.0), true), 15.0);
    }
}
