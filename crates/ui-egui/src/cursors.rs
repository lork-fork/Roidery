//! Tool cursors drawn as vector glyphs (egui only offers system cursors). Black shapes with a white
//! halo, hotspot at `p`, Illustrator's visual grammar: solid arrow (Selection), hollow arrow (Direct
//! Selection), pen nib with state badges, crosshair for drawing tools, curved arrows for rotate.

use egui::{Color32, Painter, Pos2, Shape, Stroke, pos2, vec2};
use vectorcraft_tools::Cursor;

const INK: Color32 = Color32::BLACK;
const HALO: Color32 = Color32::WHITE;

fn poly(p: &Painter, pts: Vec<Pos2>, fill: Color32, stroke: Color32) {
    // Halo first (thicker white outline), then the glyph.
    p.add(Shape::closed_line(pts.clone(), Stroke::new(3.0, HALO)));
    p.add(Shape::convex_polygon(pts.clone(), fill, Stroke::NONE));
    p.add(Shape::closed_line(pts, Stroke::new(1.0, stroke)));
}

fn line(p: &Painter, a: Pos2, b: Pos2) {
    p.line_segment([a, b], Stroke::new(3.0, HALO));
    p.line_segment([a, b], Stroke::new(1.2, INK));
}

fn arrow_points(o: Pos2) -> Vec<Pos2> {
    // Classic pointer, tip at o.
    [(0.0, 0.0), (0.0, 15.0), (3.8, 11.4), (6.4, 17.0), (8.6, 16.0), (6.1, 10.6), (11.0, 10.6)].iter().map(|(x, y)| o + vec2(*x, *y)).collect()
}

fn arrow(p: &Painter, o: Pos2, hollow: bool) {
    let pts = arrow_points(o);
    // The arrow is concave: draw as a filled mesh of two convex parts, then outline.
    p.add(Shape::closed_line(pts.clone(), Stroke::new(3.0, HALO)));
    let fill = if hollow { HALO } else { INK };
    p.add(Shape::convex_polygon(vec![pts[0], pts[1], pts[2], pts[5], pts[6]], fill, Stroke::NONE));
    p.add(Shape::convex_polygon(vec![pts[2], pts[3], pts[4], pts[5]], fill, Stroke::NONE));
    p.add(Shape::closed_line(pts, Stroke::new(1.0, INK)));
}

fn crosshair(p: &Painter, o: Pos2) {
    for (a, b) in
        [(vec2(-9.0, 0.0), vec2(-2.0, 0.0)), (vec2(2.0, 0.0), vec2(9.0, 0.0)), (vec2(0.0, -9.0), vec2(0.0, -2.0)), (vec2(0.0, 2.0), vec2(0.0, 9.0))]
    {
        line(p, o + a, o + b);
    }
}

fn pen(p: &Painter, o: Pos2, badge: &str) {
    // Nib pointing to the top-left hotspot.
    let pts = vec![o, o + vec2(4.0, 12.0), o + vec2(8.0, 16.0), o + vec2(16.0, 8.0), o + vec2(12.0, 4.0)];
    poly(p, pts, INK, INK);
    p.circle_filled(o + vec2(6.0, 6.0), 1.4, HALO);
    line(p, o + vec2(10.0, 14.0), o + vec2(14.0, 18.0));
    let b = o + vec2(16.0, 14.0);
    match badge {
        "o" => {
            p.circle_stroke(b + vec2(3.0, 3.0), 3.0, Stroke::new(3.0, HALO));
            p.circle_stroke(b + vec2(3.0, 3.0), 3.0, Stroke::new(1.2, INK));
        }
        "+" => {
            line(p, b + vec2(0.0, 3.0), b + vec2(6.0, 3.0));
            line(p, b + vec2(3.0, 0.0), b + vec2(3.0, 6.0));
        }
        "-" => line(p, b + vec2(0.0, 3.0), b + vec2(6.0, 3.0)),
        "/" => line(p, b + vec2(0.0, 6.0), b + vec2(5.0, 0.0)),
        "^" => {
            line(p, b + vec2(0.0, 6.0), b + vec2(3.0, 0.0));
            line(p, b + vec2(3.0, 0.0), b + vec2(6.0, 6.0));
        }
        "*" => {
            line(p, b + vec2(0.0, 0.0), b + vec2(6.0, 6.0));
            line(p, b + vec2(6.0, 0.0), b + vec2(0.0, 6.0));
            line(p, b + vec2(3.0, -1.0), b + vec2(3.0, 7.0));
        }
        _ => {}
    }
}

fn double_arrow(p: &Painter, o: Pos2, dir: egui::Vec2) {
    let d = dir.normalized() * 8.0;
    let n = vec2(-d.y, d.x) * 0.45;
    line(p, o - d, o + d);
    for (tip, back) in [(o + d, o + d * 0.45), (o - d, o - d * 0.45)] {
        poly(p, vec![tip, back + n, back - n], INK, INK);
    }
}

fn rotate(p: &Painter, o: Pos2) {
    let pts: Vec<Pos2> = (0..=10)
        .map(|i| {
            let a = std::f32::consts::PI * (0.15 + 0.7 * i as f32 / 10.0);
            o + vec2(a.cos() * 9.0, -a.sin() * 9.0)
        })
        .collect();
    p.add(Shape::line(pts.clone(), Stroke::new(3.0, HALO)));
    p.add(Shape::line(pts.clone(), Stroke::new(1.2, INK)));
    for end in [pts[0], pts[pts.len() - 1]] {
        poly(p, vec![end + vec2(-3.0, -1.0), end + vec2(3.0, -1.0), end + vec2(0.0, 4.0)], INK, INK);
    }
}

/// Live Corners: the hollow arrow with a rounded-corner badge.
fn corner_radius(p: &Painter, o: Pos2) {
    arrow(p, o, true);
    let b = o + vec2(12.0, 13.0);
    let mut pts = vec![b + vec2(0.0, 10.0)];
    pts.extend((0..=8).map(|i| {
        let a = std::f32::consts::PI * (1.0 + 0.5 * i as f32 / 8.0);
        b + vec2(4.0 + 4.0 * a.cos(), 4.0 + 4.0 * a.sin())
    }));
    pts.push(b + vec2(10.0, 0.0));
    p.add(Shape::line(pts.clone(), Stroke::new(3.0, HALO)));
    p.add(Shape::line(pts, Stroke::new(1.2, INK)));
}

/// Over a bracket of type on a path: the arrow with a bracket (a stem standing on a baseline,
/// with a foot) below right.
fn path_bracket(p: &Painter, o: Pos2) {
    arrow(p, o, false);
    let b = o + vec2(13.0, 13.0);
    let stem = [b + vec2(3.0, 0.0), b + vec2(3.0, 10.0)];
    let foot = [b + vec2(3.0, 0.0), b + vec2(7.0, 0.0)];
    let base = [b + vec2(0.0, 8.0), b + vec2(10.0, 8.0)];
    for (w, c) in [(3.0, HALO), (1.2, INK)] {
        for l in [stem, foot, base] {
            p.line_segment(l, Stroke::new(w, c));
        }
    }
}

/// The gradient annotator's stop cursors: the arrow with a plus (add a stop) or minus (delete it)
/// badge.
fn stop_badge(p: &Painter, o: Pos2, add: bool) {
    arrow(p, o, false);
    let b = o + vec2(13.0, 13.0);
    line(p, b, b + vec2(6.0, 0.0));
    if add {
        line(p, b + vec2(3.0, -3.0), b + vec2(3.0, 3.0));
    }
}

/// A slice badge: a small rectangle cut by a line, at `b` (its top left).
fn slice_badge(p: &Painter, b: Pos2) {
    let pts = vec![b, b + vec2(8.0, 0.0), b + vec2(8.0, 6.0), b + vec2(0.0, 6.0)];
    poly(p, pts, HALO, INK);
    line(p, b + vec2(4.0, 0.0), b + vec2(4.0, 6.0));
}

/// The Slice tool: a crosshair with a blade below right of the hotspot.
fn slice(p: &Painter, o: Pos2) {
    crosshair(p, o);
    let b = o + vec2(7.0, 7.0);
    poly(p, vec![b, b + vec2(9.0, 4.0), b + vec2(10.0, 7.0), b + vec2(3.0, 6.0)], HALO, INK);
    line(p, b + vec2(8.0, 6.0), b + vec2(12.0, 12.0));
}

/// The Width tool: the hollow arrow with a stroke that swells in the middle (a width point),
/// plus a badge: `+` over a stroke (a drag adds a point), a bar across the swell over a width point
/// (a drag moves or widens it).
fn width(p: &Painter, o: Pos2, badge: &str) {
    arrow(p, o, true);
    let b = o + vec2(11.0, 18.0);
    let top: Vec<Pos2> = (0..=8)
        .map(|i| {
            let t = i as f32 / 8.0;
            b + vec2(12.0 * t, -3.5 * (std::f32::consts::PI * t).sin())
        })
        .collect();
    let mut lens = top.clone();
    lens.extend(top.iter().rev().map(|q| pos2(q.x, 2.0 * b.y - q.y)));
    p.add(Shape::closed_line(lens.clone(), Stroke::new(3.0, HALO)));
    p.add(Shape::closed_line(lens, Stroke::new(1.2, INK)));
    match badge {
        "+" => {
            let c = b + vec2(16.0, -6.0);
            line(p, c - vec2(3.0, 0.0), c + vec2(3.0, 0.0));
            line(p, c - vec2(0.0, 3.0), c + vec2(0.0, 3.0));
        }
        "point" => line(p, b + vec2(6.0, -6.0), b + vec2(6.0, 6.0)),
        _ => {}
    }
}

fn ibeam(p: &Painter, o: Pos2) {
    line(p, o + vec2(0.0, -8.0), o + vec2(0.0, 8.0));
    line(p, o + vec2(-3.0, -8.0), o + vec2(3.0, -8.0));
    line(p, o + vec2(-3.0, 8.0), o + vec2(3.0, 8.0));
    line(p, o + vec2(-2.0, 3.0), o + vec2(2.0, 3.0));
}

/// The Blend tool: a crosshair with a square below right of the hotspot, hollow away from art,
/// filled over an object; over an anchor point a ringed dot (the blend starts there).
fn blend(p: &Painter, o: Pos2, badge: Cursor) {
    crosshair(p, o);
    let b = o + vec2(9.0, 9.0);
    if badge == Cursor::BlendAnchor {
        p.circle_stroke(b + vec2(3.5, 3.5), 3.5, Stroke::new(3.0, HALO));
        p.circle_stroke(b + vec2(3.5, 3.5), 3.5, Stroke::new(1.2, INK));
        p.circle_filled(b + vec2(3.5, 3.5), 1.4, INK);
        return;
    }
    let fill = if badge == Cursor::BlendObject { INK } else { HALO };
    poly(p, vec![b, b + vec2(7.0, 0.0), b + vec2(7.0, 7.0), b + vec2(0.0, 7.0)], fill, INK);
}

/// The Shape Builder: a crosshair with a plus badge below right of the hotspot (merge mode), or a
/// minus (erase mode).
fn shape_builder(p: &Painter, o: Pos2, erase: bool) {
    crosshair(p, o);
    let c = o + vec2(12.0, 12.0);
    line(p, c - vec2(3.5, 0.0), c + vec2(3.5, 0.0));
    if !erase {
        line(p, c - vec2(0.0, 3.5), c + vec2(0.0, 3.5));
    }
}

/// Paint cursor `c` at `p` on the given (foreground) painter. Returns false for cursors that should
/// stay system cursors (hand, zoom, busy states).
pub fn paint(painter: &Painter, c: Cursor, p: Pos2) -> bool {
    match c {
        Cursor::Arrow => arrow(painter, p, false),
        Cursor::ArrowHollow => arrow(painter, p, true),
        Cursor::Move => {
            arrow(painter, p, false);
            double_arrow(painter, p + vec2(15.0, 18.0), vec2(1.0, 0.0));
        }
        Cursor::Crosshair | Cursor::Eyedropper => crosshair(painter, p),
        Cursor::ResizeH => double_arrow(painter, p, vec2(1.0, 0.0)),
        Cursor::ResizeV => double_arrow(painter, p, vec2(0.0, 1.0)),
        Cursor::ResizeNwSe => double_arrow(painter, p, vec2(1.0, 1.0)),
        Cursor::ResizeNeSw => double_arrow(painter, p, vec2(1.0, -1.0)),
        Cursor::Rotate => rotate(painter, p),
        Cursor::CornerRadius => corner_radius(painter, p),
        Cursor::Pen => pen(painter, p, "*"),
        Cursor::PenAdd => pen(painter, p, "+"),
        Cursor::PenDelete => pen(painter, p, "-"),
        Cursor::PenClose => pen(painter, p, "o"),
        Cursor::PenContinue => pen(painter, p, "/"),
        Cursor::PenConvert => pen(painter, p, "^"),
        Cursor::Text => ibeam(painter, p),
        Cursor::AddStop => stop_badge(painter, p, true),
        Cursor::RemoveStop => stop_badge(painter, p, false),
        Cursor::Slice => slice(painter, p),
        Cursor::SliceSelect => {
            arrow(painter, p, false);
            slice_badge(painter, p + vec2(11.0, 14.0));
        }
        Cursor::Width => width(painter, p, ""),
        Cursor::WidthAdd => width(painter, p, "+"),
        Cursor::WidthPoint => width(painter, p, "point"),
        Cursor::Blend | Cursor::BlendObject | Cursor::BlendAnchor => blend(painter, p, c),
        Cursor::PathBracket => path_bracket(painter, p),
        Cursor::ShapeBuilder => shape_builder(painter, p, false),
        Cursor::ShapeBuilderErase => shape_builder(painter, p, true),
        _ => return false,
    }
    let _ = pos2;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_hotspot_is_tip() {
        let pts = arrow_points(pos2(10.0, 20.0));
        assert_eq!(pts[0], pos2(10.0, 20.0));
        assert!(pts.iter().all(|q| q.x >= 10.0 && q.y >= 20.0));
    }
}
