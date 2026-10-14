//! Read every font in a library folder and report what it sews with, its size range, and any
//! satin column that couldn't be paired, to check a library before shipping it:
//! `cargo run -p roidery-fonts --example check_fonts -- path/to/fonts`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use roidery_fonts::{convert, font, svg};

fn main() {
    let dir = std::env::args().nth(1).expect("usage: check_fonts FOLDER");
    let mut dirs: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    for d in dirs {
        let id = d.file_name().unwrap().to_string_lossy().to_string();
        let t = std::time::Instant::now();
        let f = match font::Font::load(&d) {
            Ok(f) => f,
            Err(e) => {
                println!("{id}\tERROR {e}");
                continue;
            }
        };
        let ms = t.elapsed().as_millis();
        // Satin columns that didn't pair, and how far the capitals' bottoms sit off the baseline.
        let mut bad_satins = 0;
        let mut satins = 0;
        for file in font::variant_files(&d) {
            for g in svg::glyphs(&font::read_text(&file).unwrap()).unwrap() {
                for e in &g.elements {
                    if e.flag("satin_column") == Some(true) {
                        satins += 1;
                        if convert::convert(e).map(|p| p.is_empty()).unwrap_or(false) {
                            bad_satins += 1;
                        }
                    }
                }
            }
        }
        let bottom = f
            .glyphs
            .get("H")
            .or_else(|| f.glyphs.get("E"))
            .and_then(|g| g.pieces.iter().map(|p| kurbo::Shape::bounding_box(&p.path)).reduce(|a, b| a.union(b)).map(|b| b.y1 / f.cap));
        let (lo, hi) = f.size_range();
        let s = &f.summary;
        println!(
            "{id}\t{}\tglyphs {}\tcap {:.1} mm\trange {:.1}-{:.1} mm\t{}{}{}{}\tunsupported {}\tsatins {satins} unpaired {bad_satins}\tbaseline {}\t{ms} ms{}",
            f.info.name,
            s.glyphs,
            s.cap_mm,
            lo,
            hi,
            if s.satin { "satin " } else { "" },
            if s.fill { "fill " } else { "" },
            if s.run { "run " } else { "" },
            if s.multicolor { "multicolor" } else { "" },
            s.unsupported,
            bottom.map_or("-".into(), |b| format!("{b:+.3}")),
            if s.shaped { "\tSHAPED" } else { "" }
        );
    }
}
