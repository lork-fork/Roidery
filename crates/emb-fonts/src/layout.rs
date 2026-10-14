//! Laying out text in a digitized font, as Ink/Stitch does: each glyph is placed at its advance
//! with the font's kerning, ligatures are used where the text spells them, and a missing
//! character sews the font's default glyph (or leaves a space).
//!
//! Sizes follow Roidery's lettering: the height is the cap height in mm, letter spacing is added
//! in mm, and lines are `line_spacing` cap heights apart.

use kurbo::{Affine, BezPath, Point};
use roidery_stitch::{Lettering, StitchSettings, StitchType};

use crate::font::{Font, FontGlyph};

/// A piece placed on the page, in mm.
#[derive(Clone, Debug)]
pub struct PlacedPiece {
    pub path: BezPath,
    pub fill: Option<StitchSettings>,
    pub stroke: Option<StitchSettings>,
    pub fill_rgb: Option<[u8; 3]>,
    pub stroke_rgb: Option<[u8; 3]>,
}

/// One glyph's pieces on the page.
#[derive(Clone, Debug)]
pub struct Placed {
    pub name: String,
    pub pieces: Vec<PlacedPiece>,
}

/// Mm per font unit when the cap height is `height` mm.
pub fn scale(font: &Font, height: f64) -> f64 {
    if font.cap > 1e-9 { height / font.cap } else { height / 100.0 }
}

/// The glyphs a word sews as: the longest glyph name the text continues with at each point (a
/// ligature before its letters), `None` for a space.
fn word_glyphs<'f>(font: &'f Font, word: &[char], missing: &mut Vec<char>) -> Vec<Option<&'f FontGlyph>> {
    let mut out = vec![];
    let mut i = 0;
    while let Some(&c) = word.get(i) {
        let rest: String = word.get(i..).unwrap_or_default().iter().collect();
        let found = font.by_first.get(&c).and_then(|names| names.iter().find(|n| rest.starts_with(n.as_str())));
        match found.and_then(|n| font.glyphs.get(n).map(|g| (g, n.chars().count()))) {
            Some((g, len)) => {
                out.push(Some(g));
                i += len.max(1);
            }
            None => {
                if !missing.contains(&c) {
                    missing.push(c);
                }
                let fallback = font.glyphs.get(&font.info.default_glyph).filter(|_| font.info.default_glyph != " ");
                out.push(fallback);
                i += 1;
            }
        }
    }
    out
}

fn case(font: &Font, line: &str) -> String {
    match font.info.letter_case.as_str() {
        "upper" => line.to_uppercase(),
        "lower" => line.to_lowercase(),
        _ => line.to_string(),
    }
}

/// `l`'s text in `font`, in mm with the first baseline at y = 0 starting at x = 0 (or along the
/// lettering's baseline path), and the characters the font doesn't have.
pub fn layout(font: &Font, l: &Lettering) -> (Vec<Placed>, Vec<char>) {
    let s = scale(font, l.height);
    let mut out = vec![];
    let mut missing = vec![];
    if !(s.is_finite() && s > 0.0) {
        return (out, missing);
    }
    let spacing = l.spacing / s;
    let baseline: Option<Vec<Point>> = l.path.as_ref().map(|p| p.iter().map(|q| Point::new(q[0], q[1])).collect());
    for (row, line) in l.text.lines().enumerate() {
        let y = row as f64 * l.line_spacing * l.height;
        let mut x = 0.0;
        let line = case(font, line);
        // A right-to-left line is set from its last character, each word looked up as written.
        let rtl = font.info.text_direction == "rtl";
        let mut chars: Vec<char> = line.chars().collect();
        if rtl {
            chars.reverse();
        }
        let line_start = out.len();
        for word in chars.split(|c| *c == ' ') {
            let mut last: Option<&str> = None;
            let glyphs = if rtl {
                let logical: Vec<char> = word.iter().rev().copied().collect();
                let mut g = word_glyphs(font, &logical, &mut missing);
                g.reverse();
                g
            } else {
                word_glyphs(font, word, &mut missing)
            };
            for g in glyphs {
                let Some(g) = g else {
                    x += font.info.word_spacing;
                    last = None;
                    continue;
                };
                if let Some(prev) = last {
                    let kern = if rtl { font.info.kern(&g.name, prev) } else { font.info.kern(prev, &g.name) };
                    x += g.min_x - kern + spacing;
                }
                let place = match &baseline {
                    Some(path) => {
                        // The letter's middle goes where it would be along a straight line.
                        let half = g.width * s / 2.0;
                        let (at, dir) = roidery_stitch::lettering::along(path, x * s + half);
                        Affine::new([dir.x, dir.y, -dir.y, dir.x, at.x, at.y]) * Affine::translate((-half, y))
                    }
                    None => Affine::translate((x * s, y)),
                } * Affine::scale(s);
                out.push(Placed { name: g.name.clone(), pieces: g.pieces.iter().map(|p| place_piece(p, place, s)).collect() });
                let advance = font.info.advance.get(&g.name).copied().or(font.info.advance_default).unwrap_or(g.width + g.min_x);
                x += advance - g.min_x;
                last = Some(&g.name);
            }
            x += font.info.word_spacing;
        }
        // Right to left sews from the right.
        if rtl && let Some(line) = out.get_mut(line_start..) {
            line.reverse();
        }
    }
    (out, missing)
}

fn place_piece(p: &crate::convert::Piece, place: Affine, s: f64) -> PlacedPiece {
    let mut path = p.path.clone();
    path.apply_affine(place);
    let mut stroke = p.stroke.clone();
    if let (Some(st), Some(w)) = (stroke.as_mut(), p.satin_width)
        && st.kind == StitchType::Satin
    {
        st.satin.width = (w * s).clamp(0.3, 30.0);
    }
    PlacedPiece { path, fill: p.fill.clone(), stroke, fill_rgb: p.fill_rgb, stroke_rgb: p.stroke_rgb }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::info::FontInfo;

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
        xmlns:inkstitch="http://inkstitch.org/namespace">
      <g inkscape:label="GlyphLayer-H"><path d="M 0,0 L 0,-100 M 10,0 L 10,-100" style="fill:none;stroke:#000" inkstitch:satin_column="True"/></g>
      <g inkscape:label="GlyphLayer-I"><path d="M 5,0 L 5,-100 M 15,0 L 15,-100" style="fill:none;stroke:#000" inkstitch:satin_column="True"/></g>
      <g inkscape:label="GlyphLayer-HI"><path d="M 0,0 L 40,-100" style="fill:none;stroke:#f00"/></g>
    </svg>"##;

    fn font() -> Font {
        let info = FontInfo { advance: [("H".to_string(), 30.0)].into_iter().collect(), word_spacing: 25.0, ..FontInfo::default() };
        Font::from_parts("test", info, &[SVG.to_string()]).unwrap()
    }

    #[test]
    fn places_glyphs_at_their_advance_and_the_cap_height() {
        let f = font();
        assert_eq!(f.cap, 100.0);
        let l = Lettering { text: "H H".into(), font: "test".into(), height: 10.0, ..Lettering::default() };
        let (placed, missing) = layout(&f, &l);
        assert!(missing.is_empty());
        assert_eq!(placed.len(), 2);
        let b = |p: &Placed| kurbo::Shape::bounding_box(&p.pieces[0].path);
        // 10 mm cap height: 0.1 mm per unit. The second H is one advance (30) and a space (25)
        // further on.
        assert!((b(&placed[0]).height() - 10.0).abs() < 1e-6);
        assert!((b(&placed[1]).x0 - 5.5).abs() < 1e-6, "{:?}", b(&placed[1]));
    }

    #[test]
    fn ligatures_win_and_missing_letters_are_reported() {
        let f = font();
        let l = Lettering { text: "HIx".into(), font: "test".into(), ..Lettering::default() };
        let (placed, missing) = layout(&f, &l);
        assert_eq!(placed.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["HI"]);
        assert_eq!(missing, ['x']);
        assert_eq!(f.summary.glyphs, 3);
        assert!(f.summary.satin && f.summary.run && f.summary.multicolor);
    }

    #[test]
    fn follows_a_baseline_path() {
        let f = font();
        let l = Lettering { text: "I".into(), font: "test".into(), height: 10.0, path: Some(vec![[0.0, 0.0], [0.0, 100.0]]), ..Lettering::default() };
        let (placed, _) = layout(&f, &l);
        let b = kurbo::Shape::bounding_box(&placed[0].pieces[0].path);
        // Turned to run down the page: the letter lies along x.
        assert!((b.width() - 10.0).abs() < 1e-6, "{b:?}");
    }
}
