//! Brush colorization (None / Tints / Tints and Shades / Hue Shift) and symbol staining.

use std::sync::Arc;

use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{AppearanceItem, Node};

use crate::Colorization;

fn luminance(c: &Color) -> f32 {
    let [r, g, b] = c.to_rgb();
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn map_color(c: &Color, mode: Colorization, key: &Color) -> Color {
    let lum = luminance(c);
    match mode {
        Colorization::None => *c,
        Colorization::Tints => key.lerp(&Color::WHITE, lum),
        Colorization::TintsAndShades => {
            if lum < 0.5 {
                Color::BLACK.lerp(key, lum * 2.0)
            } else {
                key.lerp(&Color::WHITE, (lum - 0.5) * 2.0)
            }
        }
        Colorization::HueShift { key: art_key } => {
            let [kh, _, _] = art_key.to_hsb();
            let [th, _, _] = key.to_hsb();
            let [h, s, v] = c.to_hsb();
            if (c.to_rgb().iter().zip(art_key.to_rgb()).map(|(a, b)| (a - b).abs()).sum::<f32>()) < 0.02 {
                return *key;
            }
            Color::from_hsb((h + th - kh).rem_euclid(360.0), s, v)
        }
    }
}

fn map_paint(p: &mut Paint, mode: Colorization, key: &Color) {
    if let Paint::Solid { color, .. } = p {
        *p = Paint::solid(map_color(color, mode, key));
    }
}

/// Recolour `n` (recursively) for a stroke painted `stroke`, and strip brushes from its strokes so
/// brush output never recurses. Non-solid stroke paints leave the art's colours alone.
pub fn colorize(n: &mut Node, mode: Colorization, stroke: &Paint) {
    let key = match (mode, stroke.color()) {
        (Colorization::None, _) | (_, None) => None,
        (_, Some(c)) => Some(c),
    };
    fn walk(n: &mut Node, mode: Colorization, key: Option<Color>) {
        for it in &mut n.appearance.items {
            match it {
                AppearanceItem::Fill(f) => {
                    if let Some(k) = &key {
                        map_paint(&mut f.paint, mode, k);
                    }
                }
                AppearanceItem::Stroke(s) => {
                    s.brush = None;
                    if let Some(k) = &key {
                        map_paint(&mut s.paint, mode, k);
                    }
                }
            }
        }
        if let Some(ch) = n.children_mut() {
            for c in ch.iter_mut() {
                walk(Arc::make_mut(c), mode, key);
            }
        }
    }
    walk(n, mode, key);
}

/// Mix every solid paint under `n` towards `color` by `amount` (0..1) — the Symbol Stainer.
pub fn tint_node(n: &mut Node, color: &Color, amount: f32) {
    let amount = amount.clamp(0.0, 1.0);
    for it in &mut n.appearance.items {
        let p = match it {
            AppearanceItem::Fill(f) => &mut f.paint,
            AppearanceItem::Stroke(s) => &mut s.paint,
        };
        if let Paint::Solid { color: c, .. } = p {
            *p = Paint::solid(c.lerp(color, amount));
        }
    }
    if let vectorcraft_doc::NodeKind::Text(t) = &mut n.kind {
        for r in &mut t.runs {
            if let Paint::Solid { color: c, .. } = &mut r.style.fill {
                *c = c.lerp(color, amount);
            }
        }
    }
    if let Some(ch) = n.children_mut() {
        for c in ch.iter_mut() {
            tint_node(Arc::make_mut(c), color, amount);
        }
    }
}

/// The stain instance `inst` puts on its symbol's art: its visible fill colour and that fill's
/// opacity (how far the art mixes towards it). `None` for an unstained instance.
pub fn stain(inst: &Node) -> Option<(Color, f32)> {
    let f = inst.appearance.fill().filter(|f| f.visible)?;
    Some((f.paint.color()?, f.opacity))
}

/// A symbol's art for instance `inst`, stained by the instance's fill ([`stain`]).
pub fn instance_art(art: &Node, inst: &Node) -> Node {
    let mut a = art.clone();
    if let Some((c, amount)) = stain(inst) {
        tint_node(&mut a, &c, amount);
    }
    a
}
