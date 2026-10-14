//! Lettering: text sewn as satin columns along each letter's centre line, as embroidery fonts
//! do (a TrueType outline filled with tatami sews poorly at lettering sizes).
//!
//! The built-in font, Roidery Sans, is original work drawn for Roidery as centre-line strokes:
//! cap height 100 units, x-height 70, descenders to 30 below the baseline, y down. Each stroke
//! (one SVG subpath) becomes one satin column, so strokes break wherever a letter has a sharp
//! corner and the columns overlap at joins, as hand-digitized lettering does.

use kurbo::{Affine, BezPath};
use serde::{Deserialize, Serialize};

/// Lettering settings, stored with the object's stitch settings so the text stays editable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Lettering {
    pub text: String,
    /// Font id: a built-in font ([`FONTS`]) or a digitized font's folder name in the embroidery
    /// font library.
    pub font: String,
    /// Cap height, mm.
    pub height: f64,
    /// Extra space between letters, mm (may be negative).
    pub spacing: f64,
    /// Baseline to baseline, as a multiple of the cap height.
    pub line_spacing: f64,
    /// Map to Path: the baseline the letters follow, in mm from where the first baseline starts
    /// (each letter sits upright on it, turned with the curve). None: straight lines.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<[f64; 2]>>,
}

impl Default for Lettering {
    fn default() -> Self {
        Self { text: "Roidery".into(), font: FONTS[0].0.into(), height: 10.0, spacing: 0.0, line_spacing: 1.6, path: None }
    }
}

/// Longest text laid out.
pub const MAX_CHARS: usize = 2_000;
/// Most points a baseline path keeps.
pub const MAX_PATH_POINTS: usize = 5_000;

impl Lettering {
    /// Values in a range that lays out: heights 2–300 mm, text capped at [`MAX_CHARS`].
    pub fn sanitized(&self) -> Self {
        let f = |v: f64, def: f64, lo: f64, hi: f64| if v.is_finite() { v.clamp(lo, hi) } else { def };
        let d = Self::default();
        Self {
            text: self.text.chars().take(MAX_CHARS).collect(),
            font: if font(&self.font).is_some() || library_id(&self.font) { self.font.clone() } else { d.font },
            height: f(self.height, d.height, 2.0, 300.0),
            spacing: f(self.spacing, d.spacing, -50.0, 100.0),
            line_spacing: f(self.line_spacing, d.line_spacing, 0.8, 5.0),
            path: self
                .path
                .as_ref()
                .map(|p| {
                    p.iter()
                        .filter(|q| q[0].is_finite() && q[1].is_finite() && q[0].abs() < 1e6 && q[1].abs() < 1e6)
                        .take(MAX_PATH_POINTS)
                        .copied()
                        .collect::<Vec<_>>()
                })
                .filter(|p| p.len() >= 2 && kurbo_len(p) > 1e-6),
        }
    }

    /// The satin column width that suits this height, mm.
    pub fn column_width(&self) -> f64 {
        (self.height * 0.12).clamp(0.8, 6.0)
    }
}

/// A font: (id, name, glyphs).
pub type Font = (&'static str, &'static str, &'static [Glyph]);
/// A glyph: (character, advance in font units, centre-line strokes as SVG path data).
pub type Glyph = (char, f64, &'static str);

/// Cap height in font units.
const UNITS: f64 = 100.0;
/// Space between glyphs in font units (besides the strokes' own reach).
const GAP: f64 = 14.0;

/// The built-in lettering fonts.
pub const FONTS: [Font; 1] = [("sans", "Roidery Sans", SANS)];

/// Whether `id` can name a font in the embroidery font library (a folder name: letters, digits,
/// `_`, `-`, `.` and spaces, not starting with a dot).
pub fn library_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.starts_with('.') && id.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
}

pub fn font(id: &str) -> Option<&'static Font> {
    FONTS.iter().find(|f| f.0.eq_ignore_ascii_case(id) || f.1.eq_ignore_ascii_case(id))
}

/// Roidery Sans: original centre-line strokes, drawn for Roidery.
const SANS: &[Glyph] = &[
    (' ', 34.0, ""),
    ('A', 70.0, "M0,0 L35,-100 M35,-100 L70,0 M14,-36 L56,-36"),
    ('B', 62.0, "M6,0 L6,-100 M6,-100 L34,-100 C60,-100 60,-52 34,-52 L6,-52 M6,-52 L38,-52 C66,-52 66,0 38,0 L6,0"),
    ('C', 66.0, "M62,-82 C50,-104 4,-108 4,-50 C4,8 50,4 62,-18"),
    ('D', 66.0, "M6,0 L6,-100 M6,-100 L28,-100 C70,-100 70,0 28,0 L6,0"),
    ('E', 56.0, "M6,0 L6,-100 M6,-100 L54,-100 M6,-52 L46,-52 M6,0 L54,0"),
    ('F', 54.0, "M6,0 L6,-100 M6,-100 L54,-100 M6,-52 L46,-52"),
    ('G', 70.0, "M64,-82 C52,-104 4,-108 4,-50 C4,6 60,6 64,-42 M64,-42 L38,-42"),
    ('H', 66.0, "M6,0 L6,-100 M60,0 L60,-100 M6,-52 L60,-52"),
    ('I', 20.0, "M10,0 L10,-100"),
    ('J', 50.0, "M44,-100 L44,-28 C44,6 6,6 4,-20"),
    ('K', 62.0, "M6,0 L6,-100 M58,-100 L6,-40 M24,-60 L60,0"),
    ('L', 52.0, "M6,-100 L6,0 M6,0 L52,0"),
    ('M', 80.0, "M6,0 L6,-100 M6,-100 L40,-30 M40,-30 L74,-100 M74,-100 L74,0"),
    ('N', 66.0, "M6,0 L6,-100 M6,-100 L60,0 M60,0 L60,-100"),
    ('O', 74.0, "M37,-100 C-4,-100 -4,0 37,0 C78,0 78,-100 37,-100 Z"),
    ('P', 60.0, "M6,0 L6,-100 M6,-100 L34,-100 C62,-100 62,-48 34,-48 L6,-48"),
    ('Q', 74.0, "M37,-100 C-4,-100 -4,0 37,0 C78,0 78,-100 37,-100 Z M44,-24 L70,4"),
    ('R', 62.0, "M6,0 L6,-100 M6,-100 L34,-100 C62,-100 62,-48 34,-48 L6,-48 M30,-48 L60,0"),
    ('S', 60.0, "M54,-84 C46,-102 6,-104 6,-76 C6,-50 56,-56 56,-24 C56,6 10,4 4,-16"),
    ('T', 64.0, "M2,-100 L62,-100 M32,-100 L32,0"),
    ('U', 66.0, "M6,-100 L6,-36 C6,8 60,8 60,-36 L60,-100"),
    ('V', 70.0, "M0,-100 L35,0 M35,0 L70,-100"),
    ('W', 96.0, "M0,-100 L24,0 M24,0 L48,-80 M48,-80 L72,0 M72,0 L96,-100"),
    ('X', 66.0, "M2,-100 L64,0 M64,-100 L2,0"),
    ('Y', 66.0, "M2,-100 L33,-48 M64,-100 L33,-48 M33,-48 L33,0"),
    ('Z', 60.0, "M4,-100 L56,-100 M56,-100 L4,0 M4,0 L58,0"),
    ('a', 56.0, "M8,-60 C20,-76 50,-74 50,-46 L50,0 M50,-30 C50,-46 4,-48 4,-18 C4,6 44,4 50,-14"),
    ('b', 58.0, "M6,-100 L6,0 M6,-36 C6,-80 54,-80 54,-36 C54,8 6,8 6,-36 Z"),
    ('c', 52.0, "M48,-58 C36,-78 4,-72 4,-36 C4,2 38,4 48,-14"),
    ('d', 58.0, "M52,-100 L52,0 M52,-36 C52,-80 4,-80 4,-36 C4,8 52,8 52,-36 Z"),
    ('e', 54.0, "M6,-36 L50,-36 M50,-36 C50,-80 4,-80 4,-36 C4,2 38,4 48,-12"),
    ('f', 36.0, "M34,-96 C24,-104 12,-98 12,-80 L12,0 M0,-66 L32,-66"),
    ('g', 58.0, "M52,-36 C52,-80 4,-80 4,-36 C4,8 52,8 52,-36 Z M52,-70 L52,8 C52,40 12,38 6,22"),
    ('h', 56.0, "M6,-100 L6,0 M6,-44 C10,-78 50,-80 50,-44 L50,0"),
    ('i', 16.0, "M8,-70 L8,0 M8,-94 L8,-86"),
    ('j', 24.0, "M16,-70 L16,10 C16,34 2,32 -2,26 M16,-94 L16,-86"),
    ('k', 52.0, "M6,-100 L6,0 M48,-70 L6,-26 M20,-40 L50,0"),
    ('l', 16.0, "M8,-100 L8,0"),
    ('m', 84.0, "M6,-70 L6,0 M6,-46 C8,-78 42,-80 42,-46 L42,0 M42,-46 C44,-78 78,-80 78,-46 L78,0"),
    ('n', 56.0, "M6,-70 L6,0 M6,-44 C10,-78 50,-80 50,-44 L50,0"),
    ('o', 56.0, "M28,-70 C2,-70 2,0 28,0 C54,0 54,-70 28,-70 Z"),
    ('p', 58.0, "M6,-70 L6,30 M6,-36 C6,-80 54,-80 54,-36 C54,8 6,8 6,-36 Z"),
    ('q', 58.0, "M52,-70 L52,30 M52,-36 C52,-80 4,-80 4,-36 C4,8 52,8 52,-36 Z"),
    ('r', 38.0, "M6,-70 L6,0 M6,-40 C10,-68 24,-74 38,-68"),
    ('s', 48.0, "M42,-60 C34,-76 6,-74 6,-54 C6,-34 44,-40 44,-18 C44,4 8,4 4,-10"),
    ('t', 36.0, "M14,-92 L14,-14 C14,2 28,2 34,-2 M0,-66 L32,-66"),
    ('u', 56.0, "M6,-70 L6,-26 C6,8 50,6 50,-26 M50,-70 L50,0"),
    ('v', 54.0, "M0,-70 L27,0 M27,0 L54,-70"),
    ('w', 80.0, "M0,-70 L20,0 M20,0 L40,-56 M40,-56 L60,0 M60,0 L80,-70"),
    ('x', 52.0, "M2,-70 L50,0 M50,-70 L2,0"),
    ('y', 54.0, "M0,-70 L27,0 M54,-70 L22,14 C16,30 8,32 0,30"),
    ('z', 50.0, "M4,-70 L46,-70 M46,-70 L4,0 M4,0 L48,0"),
    ('0', 58.0, "M29,-100 C-1,-100 -1,0 29,0 C59,0 59,-100 29,-100 Z"),
    ('1', 40.0, "M10,-80 L28,-100 M28,-100 L28,0"),
    ('2', 58.0, "M6,-78 C10,-104 52,-106 52,-74 C52,-50 20,-36 4,0 M4,0 L54,0"),
    ('3', 58.0, "M6,-88 C18,-106 54,-102 50,-76 C48,-56 30,-54 22,-54 M22,-54 C58,-54 58,4 28,0 C14,0 6,-6 4,-16"),
    ('4', 58.0, "M40,0 L40,-100 M40,-100 L2,-30 M2,-30 L56,-30"),
    ('5', 58.0, "M50,-100 L12,-100 M12,-100 L8,-56 M8,-56 C30,-68 54,-58 54,-32 C54,6 10,6 4,-16"),
    ('6', 58.0, "M48,-96 C24,-108 4,-80 4,-44 C4,6 54,6 54,-32 C54,-66 10,-66 4,-40"),
    ('7', 58.0, "M4,-100 L54,-100 M54,-100 L20,0"),
    ('8', 58.0, "M29,-54 C2,-56 4,-100 29,-100 C54,-100 56,-56 29,-54 C0,-52 0,0 29,0 C58,0 58,-52 29,-54 Z"),
    ('9', 58.0, "M10,-4 C34,8 54,-20 54,-56 C54,-106 4,-106 4,-68 C4,-34 48,-34 54,-60"),
    ('.', 18.0, "M9,-7 L9,0"),
    (',', 18.0, "M10,-7 L6,12"),
    ('!', 18.0, "M9,-100 L9,-28 M9,-8 L9,0"),
    ('?', 50.0, "M6,-82 C12,-106 46,-104 46,-78 C46,-56 24,-54 24,-30 M24,-8 L24,0"),
    ('-', 40.0, "M6,-40 L34,-40"),
    ('\'', 14.0, "M7,-100 L7,-76"),
    (':', 18.0, "M9,-66 L9,-59 M9,-7 L9,0"),
    ('/', 40.0, "M36,-100 L4,0"),
    ('&', 70.0, "M66,0 L20,-62 C6,-80 18,-102 34,-100 C52,-98 54,-76 34,-62 L18,-50 C-2,-34 6,0 30,0 C46,0 56,-12 62,-30"),
    ('+', 52.0, "M26,-66 L26,-14 M4,-40 L48,-40"),
];

fn kurbo_len(p: &[[f64; 2]]) -> f64 {
    p.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum()
}

/// The point `s` mm along the baseline and the direction there; past either end the baseline
/// carries on straight.
pub fn along(path: &[kurbo::Point], s: f64) -> (kurbo::Point, kurbo::Vec2) {
    let dir = |a: kurbo::Point, b: kurbo::Point| {
        let v = b - a;
        let l = v.hypot();
        if l > 1e-12 { v / l } else { kurbo::Vec2::new(1.0, 0.0) }
    };
    let mut walked = 0.0;
    let segs: Vec<(kurbo::Point, kurbo::Point)> = path.windows(2).map(|w| (w[0], w[1])).filter(|(a, b)| a.distance(*b) > 1e-12).collect();
    let Some(&(a0, b0)) = segs.first() else { return (path.first().copied().unwrap_or_default(), kurbo::Vec2::new(1.0, 0.0)) };
    if s <= 0.0 {
        let d = dir(a0, b0);
        return (a0 + d * s, d);
    }
    for &(a, b) in &segs {
        let l = a.distance(b);
        if walked + l >= s {
            return (a.lerp(b, (s - walked) / l), dir(a, b));
        }
        walked += l;
    }
    let (a, b) = segs.last().copied().unwrap_or((a0, b0));
    let d = dir(a, b);
    (b + d * (s - walked), d)
}

/// The laid-out text: one subpath per stroke, in mm (y down, the first line's baseline at y = 0,
/// starting at x = 0), and the characters the font doesn't have (left out). With a baseline
/// path, each letter stands on the path where it would be along a straight line, turned with it.
pub fn layout(l: &Lettering) -> (BezPath, Vec<char>) {
    let l = l.sanitized();
    let Some((_, _, glyphs)) = font(&l.font) else { return (BezPath::new(), vec![]) };
    let scale = l.height / UNITS;
    let baseline: Option<Vec<kurbo::Point>> = l.path.as_ref().map(|p| p.iter().map(|q| kurbo::Point::new(q[0], q[1])).collect());
    let mut out = BezPath::new();
    let mut missing = vec![];
    for (row, line) in l.text.lines().enumerate() {
        let mut x = 0.0;
        let y = row as f64 * l.line_spacing * l.height;
        for c in line.chars() {
            let Some((_, advance, d)) = glyphs.iter().find(|g| g.0 == c).or_else(|| glyphs.iter().find(|g| g.0 == c.to_ascii_uppercase())) else {
                if !missing.contains(&c) {
                    missing.push(c);
                }
                continue;
            };
            if let Ok(mut bp) = BezPath::from_svg(d) {
                let place = match &baseline {
                    Some(path) => {
                        // The letter's middle goes where it would be along a straight line.
                        let half = advance * scale / 2.0;
                        let (at, dir) = along(path, x + half);
                        let turn = Affine::new([dir.x, dir.y, -dir.y, dir.x, at.x, at.y]);
                        turn * Affine::translate((-half, y))
                    }
                    None => Affine::translate((x, y)),
                };
                bp.apply_affine(place * Affine::scale(scale));
                out.extend(bp);
            }
            x += (advance + GAP) * scale + l.spacing;
        }
    }
    (out, missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Shape, StitchSettings, StitchType, generate, polylines_from_bezpath};

    #[test]
    fn letters_follow_a_baseline_path_upright_on_it() {
        let straight = Lettering { text: "II".into(), ..Default::default() };
        let (flat, _) = layout(&straight);
        // A straight baseline along x lays out as without one.
        let along_x = Lettering { path: Some(vec![[0.0, 0.0], [100.0, 0.0]]), ..straight.clone() };
        let (same, _) = layout(&along_x);
        let b = |p: &BezPath| kurbo::Shape::bounding_box(p);
        assert!((b(&flat).x0 - b(&same).x0).abs() < 1e-9 && (b(&flat).y1 - b(&same).y1).abs() < 1e-9);
        // Straight down the page: the letters stand sideways, the text running downwards.
        let down = Lettering { path: Some(vec![[0.0, 0.0], [0.0, 100.0]]), ..straight.clone() };
        let (side, _) = layout(&down);
        assert!((b(&side).width() - b(&flat).height()).abs() < 0.5, "{:?}", b(&side));
        assert!((b(&side).height() - b(&flat).width()).abs() < 0.5, "{:?}", b(&side));
        // Turned clockwise with the path: the letters' tops point right, off the path.
        assert!(b(&side).x0 > -0.5);
        // Bad points are dropped; too few leave the text straight.
        let bad = Lettering { path: Some(vec![[f64::NAN, 0.0], [1.0, 1.0]]), ..straight };
        assert_eq!(bad.sanitized().path, None);
    }

    #[test]
    fn every_glyph_parses_and_stays_in_its_box() {
        for (c, advance, d) in SANS {
            let bp = BezPath::from_svg(d).unwrap_or_else(|e| panic!("{c}: {e:?}"));
            if d.is_empty() {
                continue;
            }
            let b = kurbo::Shape::bounding_box(&bp);
            assert!(b.x0 >= -4.0 && b.x1 <= advance + 4.0, "{c} reaches {b:?}");
            assert!(b.y0 >= -110.0 && b.y1 <= 42.0, "{c} reaches {b:?}");
        }
        let have: Vec<char> = SANS.iter().map(|g| g.0).collect();
        for c in ('A'..='Z').chain('a'..='z').chain('0'..='9') {
            assert!(have.contains(&c), "{c}");
        }
    }

    #[test]
    fn text_lays_out_at_its_height_line_by_line() {
        let l = Lettering { text: "Hi\nHi".into(), height: 20.0, ..Default::default() };
        let (bp, missing) = layout(&l);
        assert!(missing.is_empty());
        let b = kurbo::Shape::bounding_box(&bp);
        assert!((b.y0 + 20.0).abs() < 0.5, "cap height 20 mm: {b:?}");
        assert!((b.y1 - 32.0).abs() < 0.5, "second baseline at 1.6 × 20: {b:?}");
        let (_, missing) = layout(&Lettering { text: "a€b😀".into(), ..Default::default() });
        assert_eq!(missing, ['€', '😀']);
    }

    #[test]
    fn lettering_sews_as_satin_columns() {
        let l = Lettering::default();
        let (bp, _) = layout(&l);
        let mut s = StitchSettings::of(StitchType::Satin);
        s.satin.width = l.column_width();
        let stitches = generate(&Shape::Paths(polylines_from_bezpath(&bp, 0.05)), &s);
        assert!(stitches.len() > 300, "{}", stitches.len());
        assert!(stitches.iter().all(|s| s.x > -2.0 && s.y > -12.0 && s.y < 5.0));
    }

    #[test]
    fn sanitize_keeps_layout_bounded() {
        let l = Lettering { text: "x".repeat(MAX_CHARS * 2), height: f64::NAN, font: "../nope".into(), spacing: 1e9, line_spacing: 0.0, path: None };
        let s = l.sanitized();
        assert_eq!((s.text.chars().count(), s.height, s.font.as_str(), s.spacing, s.line_spacing), (MAX_CHARS, 10.0, "sans", 100.0, 0.8));
        // A library font's id is kept (the engine checks the library has it).
        assert_eq!(Lettering { font: "amitaclo_small".into(), ..l }.sanitized().font, "amitaclo_small");
    }
}
