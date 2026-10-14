//! A loaded font: its metadata and its glyphs, each converted to the pieces it sews as.

use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::convert::{MM_PER_PX, Piece, Unsupported, convert};
use crate::info::FontInfo;
use crate::svg::{self, MAX_SVG_BYTES};

/// A glyph, ready to lay out: font units, leftmost point at x = 0, baseline at y = 0.
#[derive(Clone, Debug, Default)]
pub struct FontGlyph {
    pub name: String,
    pub min_x: f64,
    pub width: f64,
    pub pieces: Vec<Piece>,
    /// Top and bottom of the glyph (negative: above the baseline).
    pub top: f64,
    pub bottom: f64,
}

/// What a font sews with, for the font list.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Summary {
    pub satin: bool,
    pub fill: bool,
    pub run: bool,
    /// Its letters use more than one thread colour.
    pub multicolor: bool,
    /// Glyphs it has.
    pub glyphs: usize,
    /// Cap height at the size it was digitized at, mm.
    pub cap_mm: f64,
    /// Elements that need stitches Roidery doesn't have (cross stitch, tartan), which are left out.
    pub unsupported: usize,
    /// It needs contextual shaping (Arabic initial, medial and final forms).
    pub shaped: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Font {
    pub id: String,
    pub info: FontInfo,
    pub glyphs: HashMap<String, FontGlyph>,
    /// Glyph names by first character, longest first (ligatures before single letters).
    pub by_first: HashMap<char, Vec<String>>,
    /// Cap height, font units.
    pub cap: f64,
    pub summary: Summary,
}

impl Font {
    /// Build a font from its metadata and the text of its variant SVG files.
    pub fn from_parts(id: &str, info: FontInfo, svgs: &[String]) -> Result<Font, String> {
        let mut glyphs: HashMap<String, FontGlyph> = HashMap::new();
        let mut summary = Summary::default();
        let mut colors: BTreeSet<[u8; 3]> = BTreeSet::new();
        for text in svgs {
            for g in svg::glyphs(text)? {
                if glyphs.contains_key(&g.name) {
                    continue;
                }
                let mut pieces = vec![];
                for e in &g.elements {
                    match convert(e) {
                        Ok(p) => pieces.extend(p),
                        Err(Unsupported::CrossStitch | Unsupported::Tartan) => summary.unsupported += 1,
                    }
                }
                for p in &pieces {
                    match p.stroke.as_ref().map(|s| s.kind) {
                        Some(roidery_stitch::StitchType::SatinBlocks | roidery_stitch::StitchType::Satin) => summary.satin = true,
                        Some(_) => summary.run = true,
                        None => {}
                    }
                    if p.fill.is_some() {
                        summary.fill = true;
                        colors.extend(p.fill_rgb);
                    }
                    if p.stroke.is_some() {
                        colors.extend(p.stroke_rgb);
                    }
                }
                let (top, bottom) = g.bounds().map_or((0.0, 0.0), |b| (b.y0, b.y1));
                if g.name.contains(".init") || g.name.contains(".medi") || g.name.contains(".fina") {
                    summary.shaped = true;
                }
                glyphs.insert(g.name.clone(), FontGlyph { name: g.name, min_x: g.min_x, width: g.width, pieces, top, bottom });
            }
        }
        if glyphs.is_empty() {
            return Err("the font has no glyphs".into());
        }
        let mut by_first: HashMap<char, Vec<String>> = HashMap::new();
        for name in glyphs.keys() {
            // Named forms (`a.alt`) aren't typed.
            if name.contains('.') && name.chars().count() > 1 {
                continue;
            }
            if let Some(c) = name.chars().next() {
                by_first.entry(c).or_default().push(name.clone());
            }
        }
        for v in by_first.values_mut() {
            v.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()).then(a.cmp(b)));
        }
        let cap = cap_height(&glyphs, &info);
        summary.glyphs = glyphs.len();
        summary.multicolor = colors.len() > 1;
        summary.cap_mm = cap * MM_PER_PX;
        Ok(Font { id: id.to_string(), info, glyphs, by_first, cap, summary })
    }

    /// Load the font in folder `dir` (`font.json` and its left-to-right variant).
    pub fn load(dir: &Path) -> Result<Font, String> {
        let id = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        let info = match read_text(&info_file(dir)) {
            Ok(t) => FontInfo::parse(&t)?,
            Err(_) => FontInfo::default(),
        };
        let files = variant_files(dir);
        if files.is_empty() {
            return Err(format!("{id}: no glyph file (ltr.svg)"));
        }
        let mut svgs = vec![];
        for f in files {
            svgs.push(read_text(&f)?);
        }
        let mut font = Font::from_parts(&id, info, &svgs)?;
        if font.info.name.is_empty() {
            font.info.name.clone_from(&id);
        }
        Ok(font)
    }

    /// The sizes (cap height, mm) it sews well at.
    pub fn size_range(&self) -> (f64, f64) {
        (self.summary.cap_mm * self.info.min_scale, self.summary.cap_mm * self.info.max_scale)
    }
}

/// The cap height: the height of H (or another flat-topped capital), else the tallest capital,
/// else most of the leading.
fn cap_height(glyphs: &HashMap<String, FontGlyph>, info: &FontInfo) -> f64 {
    for c in ["H", "I", "E", "T", "L", "F", "N", "M"] {
        if let Some(g) = glyphs.get(c).filter(|g| g.bottom - g.top > 0.0) {
            return g.bottom - g.top;
        }
    }
    let tallest = glyphs
        .values()
        .filter(|g| g.name.chars().count() == 1 && g.name.chars().all(char::is_uppercase))
        .map(|g| g.bottom - g.top)
        .fold(0.0, f64::max);
    if tallest > 0.0 { tallest } else { info.leading * 0.7 }
}

/// A file's text; `.gz` files are decompressed.
pub fn read_text(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > MAX_SVG_BYTES as u64 {
        return Err(format!("{}: too big", path.display()));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gz")) {
        let mut out = String::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .take(MAX_SVG_BYTES as u64 + 1)
            .read_to_string(&mut out)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if out.len() > MAX_SVG_BYTES {
            return Err(format!("{}: too big", path.display()));
        }
        return Ok(out);
    }
    String::from_utf8(bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// The font's `font.json` (or `font.json.gz`, as the packaged library has it: large fonts list
/// thousands of kerning pairs).
pub fn info_file(dir: &Path) -> PathBuf {
    let gz = dir.join("font.json.gz");
    if gz.is_file() && !dir.join("font.json").is_file() { gz } else { dir.join("font.json") }
}

/// The variant's files: `ltr.svg` (or `.svg.gz`), the older `→.svg`, or every SVG in an `ltr`
/// folder; a right-to-left font with only its right-to-left variant has that (`rtl`, `←`).
pub fn variant_files(dir: &Path) -> Vec<PathBuf> {
    for name in ["ltr", "\u{2192}", "rtl", "\u{2190}"] {
        for ext in ["svg", "svg.gz"] {
            let f = dir.join(format!("{name}.{ext}"));
            if f.is_file() {
                return vec![f];
            }
        }
        let sub = dir.join(name);
        if sub.is_dir() {
            let mut v: Vec<PathBuf> =
                std::fs::read_dir(&sub).map(|rd| rd.filter_map(Result::ok).map(|e| e.path()).filter(|p| is_svg(p)).collect()).unwrap_or_default();
            v.sort();
            v.truncate(64);
            if !v.is_empty() {
                return v;
            }
        }
    }
    vec![]
}

fn is_svg(p: &Path) -> bool {
    let n = p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_ascii_lowercase();
    n.ends_with(".svg") || n.ends_with(".svg.gz")
}
