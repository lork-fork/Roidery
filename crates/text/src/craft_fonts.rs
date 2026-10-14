//! Fonts from the optional craft-fonts build input (<https://github.com/storytold/craft-fonts>).
//!
//! Built with `CRAFT_FONTS_DIR=<craft-fonts checkout>`, [`CRAFT_FONTS`] holds every font in its
//! manifest (today the Japanese UI and document fonts); built without it, it is empty and Japanese
//! text falls back to the installed system fonts. Release builds always set it. See `build.rs`
//! and craftrules `standards/fonts.md`.

/// A font from the optional craft-fonts build input (empty unless built with `CRAFT_FONTS_DIR`).
pub struct CraftFont {
    pub family: &'static str,
    pub style: &'static str,
    /// ISO 15924 scripts the font is for, e.g. `"Jpan"`.
    pub scripts: &'static [&'static str],
    pub bytes: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/craft_fonts.rs"));

impl CraftFont {
    /// Is the font for Japanese text?
    pub fn is_japanese(&self) -> bool {
        self.scripts.contains(&"Jpan")
    }
    /// A serif (Mincho) family, for document text.
    pub fn is_mincho(&self) -> bool {
        self.family.contains("Mincho")
    }
    /// A sans (Gothic) family, for UI text.
    pub fn is_gothic(&self) -> bool {
        self.family.contains("Gothic")
    }
    /// A bold face.
    pub fn is_bold(&self) -> bool {
        crate::style_weight(self.style) >= 600.0
    }
}

/// The craft-fonts faces for Japanese text, best for document text first: Mincho (serif) families,
/// then the others, regular weights before bold. Empty when built without craft-fonts.
pub fn japanese_document_fonts() -> Vec<&'static CraftFont> {
    let mut v: Vec<&'static CraftFont> = CRAFT_FONTS.iter().filter(|f| f.is_japanese()).collect();
    v.sort_by_key(|f| (!f.is_mincho(), f.is_bold()));
    v
}

/// The craft-fonts faces for Japanese text, best for UI text first: Gothic (sans) families, then
/// the others; `bold` puts bold faces before regular ones. Empty when built without craft-fonts.
pub fn japanese_ui_fonts(bold: bool) -> Vec<&'static CraftFont> {
    let mut v: Vec<&'static CraftFont> = CRAFT_FONTS.iter().filter(|f| f.is_japanese()).collect();
    v.sort_by_key(|f| (!f.is_gothic(), f.is_bold() != bold));
    v
}
