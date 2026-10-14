//! A font's `font.json`: its name, description, licence and metrics (Ink/Stitch's format). Every
//! field is optional and read leniently, so a font made by a newer or older Ink/Stitch still loads.

use std::collections::HashMap;

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct FontInfo {
    pub name: String,
    pub description: String,
    pub keywords: Vec<String>,
    /// The licence's name as the font states it.
    pub license: String,
    pub original_font: String,
    pub original_font_url: String,
    /// `ltr`, `rtl`, `ttb` or `btt`.
    pub text_direction: String,
    /// `upper` or `lower` to force the letter case, else empty.
    pub letter_case: String,
    /// The glyph sewn for characters the font doesn't have (`" "`: a space).
    pub default_glyph: String,
    /// Baseline to baseline, font units.
    pub leading: f64,
    /// Extra advance between two glyphs (`"A V"` or the legacy `"AV"`), font units.
    pub kerning: HashMap<String, f64>,
    /// The scales it sews well at (1 is the size it was digitized at).
    pub min_scale: f64,
    pub max_scale: f64,
    /// Ink/Stitch's nominal size, mm (0 when unknown).
    pub size: f64,
    pub advance: HashMap<String, f64>,
    /// The advance of a glyph without its own (`None`: its width).
    pub advance_default: Option<f64>,
    /// A space's advance, font units.
    pub word_spacing: f64,
}

impl Default for FontInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            keywords: vec![],
            license: String::new(),
            original_font: String::new(),
            original_font_url: String::new(),
            text_direction: "ltr".into(),
            letter_case: String::new(),
            default_glyph: "\u{fffd}".into(),
            leading: 100.0,
            kerning: HashMap::new(),
            min_scale: 1.0,
            max_scale: 1.0,
            size: 0.0,
            advance: HashMap::new(),
            advance_default: None,
            word_spacing: 20.0,
        }
    }
}

fn number(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    v.as_f64().or_else(|| v.as_str()?.trim().parse().ok()).filter(|x: &f64| x.is_finite())
}

fn text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        // A translated description: English if there is one, else any.
        Value::Object(m) => m.get("en").or_else(|| m.values().next()).and_then(Value::as_str).map(str::to_string),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn numbers(v: Option<&Value>) -> HashMap<String, f64> {
    v.and_then(Value::as_object)
        .map(|m| m.iter().take(100_000).filter_map(|(k, v)| Some((k.clone(), number(Some(v))?))).collect())
        .unwrap_or_default()
}

impl FontInfo {
    /// Read `font.json`'s text; anything unreadable takes the default.
    pub fn parse(json: &str) -> Result<FontInfo, String> {
        let v: Value = serde_json::from_str(json).map_err(|e| format!("font.json: {e}"))?;
        let d = FontInfo::default();
        let g = |k: &str| v.get(k);
        let scale = |k: &str, def: f64| number(g(k)).filter(|s| *s > 0.0 && *s < 100.0).unwrap_or(def);
        Ok(FontInfo {
            name: text(g("name")).unwrap_or_default(),
            description: text(g("description")).unwrap_or_default(),
            keywords: g("keywords")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|k| k.as_str().map(str::to_string)).collect())
                .unwrap_or_default(),
            license: text(g("font_license")).unwrap_or_else(|| "SIL Open Font License v1.1".into()),
            original_font: text(g("original_font")).unwrap_or_default(),
            original_font_url: text(g("original_font_url")).unwrap_or_default(),
            text_direction: text(g("text_direction")).filter(|t| ["ltr", "rtl", "ttb", "btt"].contains(&t.as_str())).unwrap_or(d.text_direction),
            letter_case: text(g("letter_case")).unwrap_or_default(),
            default_glyph: text(g("default_glyph")).unwrap_or(d.default_glyph),
            leading: number(g("leading")).filter(|l| *l > 0.0).unwrap_or(d.leading),
            kerning: numbers(g("kerning_pairs")),
            min_scale: scale("min_scale", 1.0),
            max_scale: scale("max_scale", 1.0),
            size: number(g("size")).filter(|s| *s > 0.0).unwrap_or(0.0),
            advance: numbers(g("horiz_adv_x")),
            advance_default: number(g("horiz_adv_x_default")),
            word_spacing: number(g("horiz_adv_x_space")).unwrap_or(d.word_spacing),
        })
    }

    /// The kerning between `prev` and `next` (font units, subtracted from the gap).
    pub fn kern(&self, prev: &str, next: &str) -> f64 {
        self.kerning.get(&format!("{prev} {next}")).or_else(|| self.kerning.get(&format!("{prev}{next}"))).copied().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_metrics_leniently() {
        let i = FontInfo::parse(
            r#"{"name":"Amitaclo","description":{"fr":"Une","en":"A script satin font"},"keywords":["satin"],
               "leading":"113","min_scale":0.8,"max_scale":1.6,"horiz_adv_x":{"A":77,"B":"73"},"horiz_adv_x_space":24,
               "kerning_pairs":{"A V":5,"To":-3}, "text_direction":"sideways"}"#,
        )
        .unwrap();
        assert_eq!(i.name, "Amitaclo");
        assert_eq!(i.description, "A script satin font");
        assert_eq!((i.leading, i.min_scale, i.max_scale, i.word_spacing), (113.0, 0.8, 1.6, 24.0));
        assert_eq!(i.advance.get("B"), Some(&73.0));
        assert_eq!(i.text_direction, "ltr");
        assert_eq!((i.kern("A", "V"), i.kern("T", "o"), i.kern("x", "y")), (5.0, -3.0, 0.0));
        assert!(FontInfo::parse("not json").is_err());
        assert_eq!(FontInfo::parse("{}").unwrap(), FontInfo { license: "SIL Open Font License v1.1".into(), ..FontInfo::default() });
    }
}
