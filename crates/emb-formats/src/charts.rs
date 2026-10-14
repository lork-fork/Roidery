//! Thread charts: a brand's threads (colour, name, catalogue number) to match artwork colours
//! against, so the design sews in threads that exist and machine files name them.
//!
//! Built in:
//! - Brother Embroidery thread: the colours and names of the PEC palette Brother machines use
//!   ([`pes::PALETTE`]). Brother's catalogue numbers aren't part of it, so the chart has none.
//! - Janome: the JEF palette ([`jef::PALETTE`]) with Janome's catalogue numbers (as pyembroidery,
//!   MIT licence, lists them).
//!
//! Other brands (Madeira, Isacord, Robison-Anton…) come in as files the user has: GIMP palettes
//! (`.gpl`) or CSV ([`parse_chart`]).

use serde::{Deserialize, Serialize};

use crate::{Thread, jef, pes};

/// A thread chart.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Chart {
    pub name: String,
    pub threads: Vec<Thread>,
}

/// The built-in charts: (id, name).
pub const BUILT_IN: [(&str, &str); 2] = [("brother", "Brother Embroidery"), ("janome", "Janome Embroidery")];

/// Janome's catalogue number for each [`jef::PALETTE`] entry.
const JANOME_CODES: [&str; 79] = [
    "000", "002", "001", "204", "203", "219", "226", "217", "208", "201", "225", "214", "207", "003", "205", "209", "210", "211", "212", "213",
    "215", "216", "218", "220", "221", "227", "228", "229", "230", "231", "232", "233", "234", "235", "236", "237", "238", "239", "240", "241",
    "242", "243", "244", "245", "246", "247", "248", "249", "250", "251", "252", "253", "254", "255", "256", "257", "258", "259", "260", "261",
    "262", "263", "264", "265", "266", "267", "268", "269", "270", "271", "272", "273", "274", "275", "202", "206", "222", "223", "224",
];

/// A built-in chart by id (or name, ignoring case).
pub fn built_in(id: &str) -> Option<Chart> {
    let (key, name) = BUILT_IN.iter().find(|(k, n)| k.eq_ignore_ascii_case(id) || n.eq_ignore_ascii_case(id))?;
    let threads = match *key {
        "brother" => pes::PALETTE.iter().skip(1).map(|(rgb, name)| Thread { rgb: *rgb, name: (*name).into(), code: String::new() }).collect(),
        _ => jef::PALETTE
            .iter()
            .zip(JANOME_CODES)
            .skip(1)
            .map(|((rgb, name), code)| Thread { rgb: *rgb, name: (*name).into(), code: code.into() })
            .collect(),
    };
    Some(Chart { name: (*name).into(), threads })
}

impl Chart {
    /// The thread closest in colour to `rgb` (CIE76 distance in Lab, which follows what the eye
    /// sees better than RGB distance), and that distance; `None` for an empty chart.
    pub fn nearest(&self, rgb: [u8; 3]) -> Option<(&Thread, f64)> {
        let lab = to_lab(rgb);
        self.threads.iter().map(|t| (t, delta_e(lab, to_lab(t.rgb)))).min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The thread whose colour is exactly `rgb`, if the chart has one.
    pub fn exact(&self, rgb: [u8; 3]) -> Option<&Thread> {
        self.threads.iter().find(|t| t.rgb == rgb)
    }
}

/// sRGB to CIE Lab (D65).
pub fn to_lab(rgb: [u8; 3]) -> [f64; 3] {
    let lin = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (lin(rgb[0]), lin(rgb[1]), lin(rgb[2]));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f64| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIE76 colour difference.
pub fn delta_e(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Largest chart file read, bytes.
pub const MAX_CHART_BYTES: usize = 4 << 20;
/// Most threads in one chart.
pub const MAX_THREADS: usize = 10_000;

/// Read a thread chart from a GIMP palette (`.gpl`) or CSV.
///
/// GIMP palettes: `R G B<tab>Name`; a trailing number in the name (`Black 1800`), or a second
/// tab-separated field, is the catalogue number. The `Name:` header names the chart.
///
/// CSV: one thread per row, as `code,name,r,g,b`, `code,name,#rrggbb`, `name,#rrggbb` or any
/// order of those: the colour is a `#rrggbb` field or three whole numbers 0–255 in a row, the name
/// the first other field with letters, the code the first other non-empty field. Rows without a
/// colour (a header) are skipped. `fallback_name` names the chart when the file doesn't.
pub fn parse_chart(text: &str, fallback_name: &str) -> Result<Chart, String> {
    if text.len() > MAX_CHART_BYTES {
        return Err(format!("the chart file is larger than {} MB", MAX_CHART_BYTES >> 20));
    }
    let text = text.trim_start_matches('\u{feff}');
    let chart = if text.trim_start().starts_with("GIMP Palette") { parse_gpl(text, fallback_name) } else { parse_csv(text, fallback_name) };
    if chart.threads.is_empty() {
        return Err("no threads found: expected a GIMP palette (.gpl) or CSV of code, name and colour".into());
    }
    Ok(chart)
}

fn parse_gpl(text: &str, fallback_name: &str) -> Chart {
    let mut chart = Chart { name: fallback_name.into(), threads: vec![] };
    for line in text.lines().skip(1) {
        let line = line.trim();
        if let Some(n) = line.strip_prefix("Name:") {
            if !n.trim().is_empty() {
                chart.name = n.trim().into();
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') || line.starts_with("Columns:") {
            continue;
        }
        let Some((rgb, rest)) = three_numbers(line) else { continue };
        let tabbed: Vec<&str> = rest.split('\t').map(str::trim).filter(|s| !s.is_empty()).collect();
        let (name, code) = match tabbed.as_slice() {
            [name, code, ..] => ((*name).to_string(), (*code).to_string()),
            _ => split_code(rest),
        };
        let [r, g, b] = rgb;
        chart.threads.push(Thread { rgb: [r, g, b], name, code });
        if chart.threads.len() >= MAX_THREADS {
            break;
        }
    }
    chart
}

/// Three whole numbers 0–255 at the start of `line`, and the text after them.
fn three_numbers(line: &str) -> Option<([u8; 3], &str)> {
    let mut rgb = [0u8; 3];
    let mut rest = line;
    for c in &mut rgb {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        *c = byte(rest.get(..end)?)?;
        rest = rest.get(end..)?;
    }
    Some((rgb, rest))
}

/// `Black 1800` → ("Black", "1800"); a name without a trailing number keeps no code.
fn split_code(s: &str) -> (String, String) {
    let s = s.trim();
    match s.rsplit_once(' ') {
        Some((name, code)) if code.chars().any(|c| c.is_ascii_digit()) && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') => {
            (name.trim().into(), code.into())
        }
        _ => (s.into(), String::new()),
    }
}

fn byte(s: &str) -> Option<u8> {
    s.trim().parse::<u16>().ok().and_then(|v| u8::try_from(v).ok())
}

fn hex(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let c = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok());
    Some([c(0)?, c(2)?, c(4)?])
}

/// Split one CSV row (quoted fields may hold commas; `""` is a quote).
fn csv_fields(line: &str) -> Vec<String> {
    let sep = if !line.contains(',') && line.contains(';') { ';' } else { ',' };
    let mut out = vec![];
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == sep && !quoted => out.push(std::mem::take(&mut cur).trim().to_string()),
            c => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn parse_csv(text: &str, fallback_name: &str) -> Chart {
    let mut chart = Chart { name: fallback_name.into(), threads: vec![] };
    for line in text.lines() {
        let f = csv_fields(line);
        // Blank lines and `#` comments (a `#rrggbb` first field is a colour, not a comment).
        if line.trim().is_empty() || line.trim_start().starts_with('#') && f.first().and_then(|x| hex(x)).is_none() {
            continue;
        }
        // The colour: a #hex field, else three whole numbers 0–255 in a row.
        let (rgb, used): (Option<[u8; 3]>, Vec<usize>) = match f.iter().position(|x| hex(x).is_some()) {
            Some(i) => (f.get(i).and_then(|x| hex(x)), vec![i]),
            None => (0..f.len().saturating_sub(2))
                .find_map(|i| match (f.get(i), f.get(i + 1), f.get(i + 2)) {
                    (Some(r), Some(g), Some(b)) => Some(([byte(r)?, byte(g)?, byte(b)?], vec![i, i + 1, i + 2])),
                    _ => None,
                })
                .map_or((None, vec![]), |(c, u)| (Some(c), u)),
        };
        let Some(rgb) = rgb else { continue };
        let rest: Vec<&String> = f.iter().enumerate().filter(|(i, x)| !used.contains(i) && !x.is_empty()).map(|(_, x)| x).collect();
        let name = rest.iter().find(|x| x.chars().any(char::is_alphabetic)).map(|s| s.to_string()).unwrap_or_default();
        let code = rest.iter().find(|x| **x != &name).map(|s| s.to_string()).unwrap_or_default();
        chart.threads.push(Thread { rgb, name, code });
        if chart.threads.len() >= MAX_THREADS {
            break;
        }
    }
    chart
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_charts_have_their_threads() {
        let b = built_in("brother").unwrap();
        assert_eq!(b.threads.len(), 64);
        assert_eq!(b.threads[0].name, "Prussian Blue");
        let j = built_in("Janome Embroidery").unwrap();
        assert_eq!(j.threads.len(), 78);
        assert_eq!((j.threads[0].name.as_str(), j.threads[0].code.as_str()), ("Black", "002"));
        assert_eq!((j.threads[1].name.as_str(), j.threads[1].code.as_str()), ("White", "001"));
        assert!(built_in("madeira").is_none());
    }

    #[test]
    fn nearest_matches_by_what_the_eye_sees() {
        let j = built_in("janome").unwrap();
        assert_eq!(j.nearest([3, 2, 1]).unwrap().0.name, "Black");
        assert_eq!(j.nearest([250, 250, 250]).unwrap().0.name, "White");
        assert!(j.nearest([0, 0, 0]).unwrap().1 < 1e-9);
        assert!(Chart::default().nearest([0, 0, 0]).is_none());
        assert!(delta_e(to_lab([255, 0, 0]), to_lab([0, 0, 255])) > 100.0);
    }

    #[test]
    fn reads_gimp_palettes() {
        let gpl =
            "GIMP Palette\nName: Madeira Polyneon\nColumns: 4\n# comment\n  0   0   0\tBlack 1800\n255 255 255\tWhite\n200  16  46\tRed\t1747\n";
        let c = parse_chart(gpl, "file").unwrap();
        assert_eq!(c.name, "Madeira Polyneon");
        assert_eq!(c.threads.len(), 3);
        assert_eq!((c.threads[0].name.as_str(), c.threads[0].code.as_str()), ("Black", "1800"));
        assert_eq!((c.threads[1].name.as_str(), c.threads[1].code.as_str()), ("White", ""));
        assert_eq!((c.threads[2].rgb, c.threads[2].code.as_str()), ([200, 16, 46], "1747"));
    }

    #[test]
    fn reads_csv_in_any_column_order() {
        let csv = "Code,Name,R,G,B\n1800,Black,0,0,0\n\"1001\",\"White, bright\",255,255,255\n";
        let c = parse_chart(csv, "Isacord").unwrap();
        assert_eq!(c.name, "Isacord");
        assert_eq!(c.threads.len(), 2);
        assert_eq!(c.threads[1], Thread { rgb: [255, 255, 255], name: "White, bright".into(), code: "1001".into() });
        let hexes = parse_chart("Red;#C8102E;1747\n#00FF00,Green", "x").unwrap();
        assert_eq!(hexes.threads[0], Thread { rgb: [200, 16, 46], name: "Red".into(), code: "1747".into() });
        assert_eq!(hexes.threads[1].name, "Green");
    }

    #[test]
    fn rejects_files_without_threads_and_never_panics() {
        assert!(parse_chart("", "x").is_err());
        assert!(parse_chart("GIMP Palette\nName: x\n", "x").is_err());
        assert!(parse_chart("just words, here", "x").is_err());
        for junk in ["#", "#ffff", "#ééééé", "1,2", ",,,", "GIMP Palette\n999 0 0 Red", "\u{feff}a,#000000"] {
            let _ = parse_chart(junk, "x");
        }
        assert!(parse_chart(&"a".repeat(MAX_CHART_BYTES + 1), "x").is_err());
    }
}
