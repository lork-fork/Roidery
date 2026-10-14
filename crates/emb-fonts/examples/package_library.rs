//! Package Ink/Stitch's font library for Roidery: the fonts Roidery may ship and sews well, each
//! with its licence, its sample picture, and its metrics and glyphs compressed, plus `index.json` (each
//! font's stitch summary, for the font list) and `FONTS.md` (the credits).
//!
//! ```text
//! cargo run --release -p roidery-fonts --example package_library -- <inkstitch-fonts>/src <out>
//! ```
//!
//! `<inkstitch-fonts>` is a checkout of https://github.com/inkstitch/inkstitch-fonts (or the
//! `fonts` folder of Ink/Stitch). The fonts are never committed to Roidery's repository: the
//! folder this writes is installed beside the program (`embroidery-fonts`) or used from beside
//! the checkout (`roidery-fonts`).
//!
//! A font is left out when its licence doesn't let Roidery ship it to everyone (non-commercial,
//! no-derivatives, GPL or unclear), when it needs contextual shaping (Arabic), or when more than
//! one element in twenty uses a stitch Roidery doesn't have (cross stitch, tartan) or sews
//! differently (ripple, guided and meander fills). The fonts themselves are copied unchanged.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use roidery_fonts::font::{Font, read_text, variant_files};
use roidery_fonts::info::FontInfo;

/// Licences Roidery can ship to everyone, by how a font's licence files read.
fn licence(dir: &Path) -> &'static str {
    let mut text = String::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        if name.starts_with("license") {
            text.push_str(&std::fs::read_to_string(e.path()).unwrap_or_default());
        }
    }
    let l = text.to_lowercase();
    let id = dir.file_name().unwrap_or_default().to_string_lossy();
    // Read by hand: Bluenesia and Dinomouse are shared with permission for personal use only;
    // Nick Ainley's licence doesn't say what may be done with it.
    match id.as_ref() {
        "bluenesia_satin" | "dinomouse72" => return "NC",
        "nick_ainley" => return "?",
        _ => {}
    }
    if l.contains("by-nc") || l.replace('-', "").contains("noncommercial") || (l.contains("personal use") && !l.contains("commercial use")) {
        "NC"
    } else if l.contains("by-nd") {
        "ND"
    } else if l.contains("open font license") || l.contains("ofl") {
        "OFL-1.1"
    } else if l.contains("by-sa") {
        "CC-BY-SA-4.0"
    } else if l.contains("creativecommons.org/licenses/by/") {
        "CC-BY-4.0"
    } else if l.contains("public domain") || l.contains("cc0") {
        "Public domain"
    } else if l.contains("general public license") || l.contains("gpl") {
        "GPL"
    } else {
        "?"
    }
}

fn shippable(lic: &str) -> bool {
    matches!(lic, "OFL-1.1" | "CC-BY-SA-4.0" | "CC-BY-4.0" | "Public domain")
}

/// Elements in all, and those whose stitches Roidery doesn't have or sews differently.
fn stitch_fit(svgs: &[String]) -> (usize, usize) {
    let mut all = 0;
    let mut off = 0;
    for text in svgs {
        for g in roidery_fonts::svg::glyphs(text).unwrap_or_default() {
            for e in &g.elements {
                all += 1;
                let fill = e.param("fill_method").unwrap_or_default();
                let stroke = e.param("stroke_method").unwrap_or_default();
                if matches!(fill, "cross_stitch" | "tartan_fill" | "guided_fill" | "meander_fill") || stroke == "ripple_stitch" {
                    off += 1;
                }
            }
        }
    }
    (all, off)
}

fn gzip_to(text: &str, to: &Path) {
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(text.as_bytes()).unwrap();
    std::fs::write(to, gz.finish().unwrap()).unwrap();
}

/// The first paragraph of the font's licence: who made it and from what.
fn credit(dir: &Path) -> String {
    for name in ["LICENSE", "license"] {
        if let Ok(t) = std::fs::read_to_string(dir.join(name)) {
            let para = t.split("\n\n").map(str::trim).find(|p| !p.is_empty()).unwrap_or_default();
            return para.split_whitespace().collect::<Vec<_>>().join(" ");
        }
    }
    String::new()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(src), Some(out)) = (args.get(1).map(PathBuf::from), args.get(2).map(PathBuf::from)) else {
        eprintln!("usage: package_library <inkstitch-fonts>/src <out>");
        std::process::exit(2);
    };
    std::fs::create_dir_all(&out).unwrap();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&src).unwrap().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    let mut index = BTreeMap::new();
    let mut credits = vec![];
    let mut left_out = vec![];
    for dir in dirs {
        let id = dir.file_name().unwrap().to_string_lossy().to_string();
        let lic = licence(&dir);
        if !shippable(lic) {
            left_out.push(format!("{id}: licence {lic}"));
            continue;
        }
        let files = variant_files(&dir);
        let svgs: Vec<String> = files.iter().filter_map(|f| read_text(f).ok()).collect();
        let (all, off) = stitch_fit(&svgs);
        if all == 0 || off * 20 > all {
            left_out.push(format!("{id}: {off} of {all} elements need stitches Roidery doesn't have"));
            continue;
        }
        let font = match Font::load(&dir) {
            Ok(f) => f,
            Err(e) => {
                left_out.push(format!("{id}: {e}"));
                continue;
            }
        };
        if font.summary.shaped {
            left_out.push(format!("{id}: needs contextual shaping"));
            continue;
        }
        let to = out.join(&id);
        let _ = std::fs::remove_dir_all(&to);
        std::fs::create_dir_all(&to).unwrap();
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if lower == "font.json" {
                gzip_to(&std::fs::read_to_string(e.path()).unwrap(), &to.join("font.json.gz"));
            } else if lower == "preview.png" || lower.starts_with("license") || lower.starts_with("readme") {
                std::fs::copy(e.path(), to.join(&name)).unwrap();
            }
        }
        for (f, text) in files.iter().zip(&svgs) {
            let rel = f.strip_prefix(&dir).unwrap();
            let dest = to.join(format!("{}.gz", rel.to_string_lossy().trim_end_matches(".gz")));
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            gzip_to(text, &dest);
        }
        let info: &FontInfo = &font.info;
        let (lo, hi) = font.size_range();
        println!("{id}\t{}\t{lic}\t{} glyphs\t{lo:.0}–{hi:.0} mm", info.name, font.summary.glyphs);
        index.insert(id.clone(), serde_json::to_value(&font.summary).unwrap());
        credits.push((info.name.trim().to_string(), id, lic, info.original_font.clone(), info.original_font_url.clone(), credit(&dir)));
    }
    let index = serde_json::json!({ "source": "https://github.com/inkstitch/inkstitch-fonts", "fonts": index });
    std::fs::write(out.join("index.json"), serde_json::to_string_pretty(&index).unwrap()).unwrap();
    credits.sort_by_key(|c| c.0.to_lowercase());
    let mut md = String::from(
        "# Embroidery fonts\n\n\
         These embroidery fonts come from the Ink/Stitch font library \
         (<https://github.com/inkstitch/inkstitch-fonts>, <https://inkstitch.org/fonts/font-library/>), \
         digitized by the Ink/Stitch community. Roidery ships them unchanged (their glyph files are \
         compressed). Each font keeps its own licence: the `LICENSE` file in its folder, named below. \
         They are not covered by Roidery's own licence.\n\n\
         | Font | Licence | Based on | Credits |\n|---|---|---|---|\n",
    );
    let cell = |s: &str| s.replace('|', "\\|");
    for (name, id, lic, orig, url, credit) in &credits {
        let based = match (orig.is_empty(), url.is_empty()) {
            (false, false) => format!("[{}]({url})", cell(orig)),
            (false, true) => cell(orig),
            _ => String::new(),
        };
        md.push_str(&format!("| {} (`{id}`) | {lic} | {based} | {} |\n", cell(name), cell(credit)));
    }
    std::fs::write(out.join("FONTS.md"), md).unwrap();
    println!("\n{} fonts packaged in {}", credits.len(), out.display());
    println!("left out:\n  {}", left_out.join("\n  "));
}
