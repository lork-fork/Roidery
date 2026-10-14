//! OpenType feature selection (the OpenType panel), resolved per character style.

use harfrust::{Feature, Tag};
use vectorcraft_doc::CharStyle;

/// OpenType features applied during shaping. Kerning, `case` and ligature suppression are driven
/// by the character style (`kerning`, `all_caps`, `tracking`); the rest are layout-wide options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OtFeatures {
    /// Vertical glyph alternates (`vert`), set by the writing direction.
    pub vertical: bool,
    /// Standard ligatures (`liga`, `clig`). Suppressed automatically when tracking is non-zero.
    pub ligatures: bool,
    /// Contextual alternates (`calt`).
    pub contextual: bool,
    /// Discretionary ligatures (`dlig`).
    pub discretionary_ligatures: bool,
    /// Small capitals (`smcp`).
    pub small_caps: bool,
    /// Diagonal fractions (`frac`).
    pub fractions: bool,
    /// Oldstyle figures (`onum`).
    pub oldstyle_figures: bool,
    /// Tabular figures (`tnum`).
    pub tabular_figures: bool,
    /// Ordinals (`ordn`).
    pub ordinals: bool,
    /// Swashes (`swsh`).
    pub swash: bool,
}

impl Default for OtFeatures {
    fn default() -> Self {
        Self {
            vertical: false,
            ligatures: true,
            contextual: true,
            discretionary_ligatures: false,
            small_caps: false,
            fractions: false,
            oldstyle_figures: false,
            tabular_figures: false,
            ordinals: false,
            swash: false,
        }
    }
}

fn f(tag: &[u8; 4], on: bool) -> Feature {
    Feature::new(Tag::new(tag), on as u32, ..)
}

impl OtFeatures {
    /// Parse a feature list like `["dlig", "smcp", "-liga"]` (unknown tags are ignored).
    pub fn from_tags<'a>(tags: impl IntoIterator<Item = &'a str>) -> Self {
        Self::default().with_tags(tags)
    }

    /// The tags this set differs from the defaults by (what `CharStyle::features` stores).
    pub fn to_tags(&self) -> Vec<String> {
        let d = Self::default();
        let mut v = vec![];
        for (on, def, tag) in [
            (self.ligatures, d.ligatures, "liga"),
            (self.contextual, d.contextual, "calt"),
            (self.discretionary_ligatures, d.discretionary_ligatures, "dlig"),
            (self.small_caps, d.small_caps, "smcp"),
            (self.fractions, d.fractions, "frac"),
            (self.oldstyle_figures, d.oldstyle_figures, "onum"),
            (self.tabular_figures, d.tabular_figures, "tnum"),
            (self.ordinals, d.ordinals, "ordn"),
            (self.swash, d.swash, "swsh"),
        ] {
            if on != def {
                v.push(if on { tag.to_string() } else { format!("-{tag}") });
            }
        }
        v
    }

    /// Is `tag` (optionally prefixed with `-` or `+`) one this set understands?
    pub fn known_tag(tag: &str) -> bool {
        let t = tag.trim_start_matches(['-', '+']);
        matches!(t, "liga" | "calt" | "dlig" | "smcp" | "frac" | "onum" | "tnum" | "ordn" | "swsh")
    }

    /// This set with `tags` applied on top.
    pub fn with_tags<'a>(mut self, tags: impl IntoIterator<Item = &'a str>) -> Self {
        let o = &mut self;
        for t in tags {
            let (on, t) = match t.strip_prefix('-') {
                Some(r) => (false, r),
                None => (true, t.strip_prefix('+').unwrap_or(t)),
            };
            match t {
                "liga" => o.ligatures = on,
                "calt" => o.contextual = on,
                "dlig" => o.discretionary_ligatures = on,
                "smcp" => o.small_caps = on,
                "frac" => o.fractions = on,
                "onum" => o.oldstyle_figures = on,
                "tnum" => o.tabular_figures = on,
                "ordn" => o.ordinals = on,
                "swsh" => o.swash = on,
                _ => {}
            }
        }
        self
    }

    /// The harfrust features for text in style `st`.
    pub(crate) fn resolve(&self, st: &CharStyle) -> Vec<Feature> {
        // The character's own OpenType settings override the layout-wide ones.
        let own = self.with_tags(st.features.iter().map(String::as_str));
        let s = &own;
        let mut v = Vec::with_capacity(8);
        if st.kerning.is_some() {
            v.push(f(b"kern", false));
        }
        let liga = s.ligatures && st.tracking.abs() < 1e-9;
        if !liga {
            v.push(f(b"liga", false));
            v.push(f(b"clig", false));
        }
        if !s.contextual {
            v.push(f(b"calt", false));
        }
        if st.all_caps {
            v.push(f(b"case", true));
        }
        for (on, tag) in [
            (s.discretionary_ligatures, b"dlig"),
            (s.small_caps, b"smcp"),
            (s.fractions, b"frac"),
            (s.oldstyle_figures, b"onum"),
            (s.tabular_figures, b"tnum"),
            (s.ordinals, b"ordn"),
            (s.swash, b"swsh"),
        ] {
            if on {
                v.push(f(tag, true));
            }
        }
        if self.vertical {
            // Not `vrt2`: its glyphs are already turned on their side, and the layout turns Latin
            // and digits itself (they would lie upside down, and tate-chu-yoko break).
            v.push(f(b"vert", true));
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FontDb, layout};
    use vectorcraft_doc::TextObject;
    use vectorcraft_geom::Point;

    fn glyphs(features: &[&str]) -> usize {
        let st = CharStyle { font_family: "Source Serif 4".into(), features: features.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        layout(FontDb::global(), &TextObject::point(Point::ZERO, "fi", st)).glyphs.len()
    }

    #[test]
    fn per_character_features_drive_shaping() {
        let with = glyphs(&[]);
        let without = glyphs(&["-liga"]);
        assert!(with < without, "the default ligature merges f+i ({with} vs {without})");
        assert_eq!(OtFeatures::from_tags(["-liga", "dlig"]).to_tags(), ["-liga", "dlig"]);
        assert!(OtFeatures::default().to_tags().is_empty());
        assert!(OtFeatures::known_tag("-onum") && !OtFeatures::known_tag("zzzz"));
    }
}
