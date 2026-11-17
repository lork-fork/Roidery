//! Thread colours and thread charts.

use serde::{Deserialize, Serialize};

/// 8-bit sRGB colour. Serialised as `"#rrggbb"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);
    pub const WHITE: Rgb = Rgb::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }

    pub const fn from_u32(v: u32) -> Self {
        Rgb::new((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    pub fn to_u32(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Parses `#rgb`, `#rrggbb` (with or without `#`).
    pub fn from_hex(s: &str) -> Option<Rgb> {
        let s = s.trim().trim_start_matches('#');
        let v = u32::from_str_radix(s, 16).ok()?;
        match s.len() {
            6 => Some(Rgb::from_u32(v)),
            3 => {
                let (r, g, b) = ((v >> 8) & 0xf, (v >> 4) & 0xf, v & 0xf);
                Some(Rgb::new((r * 17) as u8, (g * 17) as u8, (b * 17) as u8))
            }
            _ => None,
        }
    }

    /// Relative luminance in `[0, 1]` (sRGB weights, gamma ignored).
    pub fn luma(self) -> f64 {
        (0.2126 * self.r as f64 + 0.7152 * self.g as f64 + 0.0722 * self.b as f64) / 255.0
    }

    /// CIE L*a*b* (D65).
    pub fn to_lab(self) -> [f64; 3] {
        fn lin(c: u8) -> f64 {
            let c = c as f64 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }
        let (r, g, b) = (lin(self.r), lin(self.g), lin(self.b));
        let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
        fn f(t: f64) -> f64 {
            if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 }
        }
        let (fx, fy, fz) = (f(x), f(y), f(z));
        [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
    }

    /// Perceptual colour difference (CIE76 ΔE).
    pub fn delta_e(self, o: Rgb) -> f64 {
        let a = self.to_lab();
        let b = o.to_lab();
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    /// The CIEDE2000 colour difference to `o`: how far apart two colours look, closer to what
    /// people see than [`delta_e`](Self::delta_e) for saturated blues, purples and dark reds.
    /// From G. Sharma, W. Wu and E. N. Dalal, "The CIEDE2000 Color-Difference Formula:
    /// Implementation Notes, Supplementary Test Data, and Mathematical Observations", Color
    /// Research & Application 30(1), 2005 (<https://hajim.rochester.edu/ece/sites/gsharma/ciede2000/>).
    pub fn delta_e2000(self, o: Rgb) -> f64 {
        de2000(self.to_lab(), o.to_lab())
    }

    /// Multiplies each channel by `f` (clamped), for shading.
    pub fn scaled(self, f: f64) -> Rgb {
        let s = |c: u8| (c as f64 * f).round().clamp(0.0, 255.0) as u8;
        Rgb::new(s(self.r), s(self.g), s(self.b))
    }

    /// Linear blend towards `o` by `t` in `[0, 1]`.
    pub fn mix(self, o: Rgb, t: f64) -> Rgb {
        let m = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round().clamp(0.0, 255.0) as u8;
        Rgb::new(m(self.r, o.r), m(self.g, o.g), m(self.b, o.b))
    }
}

impl From<Rgb> for String {
    fn from(c: Rgb) -> String {
        c.to_hex()
    }
}

impl TryFrom<String> for Rgb {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Rgb::from_hex(&s).ok_or_else(|| format!("invalid colour '{s}'"))
    }
}

/// A thread: colour plus catalogue information.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Thread {
    pub color: Rgb,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Catalogue number within the chart (e.g. "1147").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code: String,
    /// Thread chart / brand name (e.g. "Brother", "Janome").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chart: String,
}

impl Thread {
    pub fn new(color: Rgb, name: impl Into<String>) -> Self {
        Thread { color, name: name.into(), code: String::new(), chart: String::new() }
    }

    pub fn with_code(color: Rgb, name: impl Into<String>, code: impl Into<String>, chart: impl Into<String>) -> Self {
        Thread { color, name: name.into(), code: code.into(), chart: chart.into() }
    }

    /// Short human label like `"1147 Royal Blue"`.
    pub fn label(&self) -> String {
        match (self.code.is_empty(), self.name.is_empty()) {
            (false, false) => format!("{} {}", self.code, self.name),
            (false, true) => self.code.clone(),
            (true, false) => self.name.clone(),
            (true, true) => self.color.to_hex(),
        }
    }
}

/// A named collection of threads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThreadChart {
    pub name: String,
    pub threads: Vec<Thread>,
}

/// Threads a chart read from a file holds at most.
pub const MAX_CHART_THREADS: usize = 5000;

/// The fields of one CSV line (quotes and doubled quotes as RFC 4180 has them).
fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
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
            ',' | ';' | '\t' if !quoted => out.push(std::mem::take(&mut cur).trim().to_string()),
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', ';']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

impl ThreadChart {
    /// Index of the thread closest in colour to `c`.
    pub fn nearest(&self, c: Rgb) -> usize {
        nearest_index(self.threads.iter().map(|t| t.color), c).unwrap_or(0)
    }

    /// The chart as CSV: `code,name,color` a thread (`#rrggbb`).
    pub fn to_csv(&self) -> String {
        let mut out = String::from("code,name,color\n");
        for t in &self.threads {
            out.push_str(&format!("{},{},{}\n", csv_field(&t.code), csv_field(&t.name), t.color.to_hex()));
        }
        out
    }

    /// A chart named `name` from CSV: a header naming the columns (`color`/`colour`/`hex`, or `r`,
    /// `g`, `b`; `code`/`number`; `name`/`description`), or rows of a code, a name and a `#rrggbb`
    /// colour found by their look. Commas, semicolons or tabs between fields.
    pub fn from_csv(name: &str, text: &str) -> Result<ThreadChart, String> {
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
        let first = lines.next().ok_or("the file is empty")?;
        let head: Vec<String> = csv_fields(first).iter().map(|f| f.to_ascii_lowercase()).collect();
        let col = |names: &[&str]| head.iter().position(|h| names.contains(&h.as_str()));
        let hex_col = col(&["color", "colour", "hex", "rgb hex", "html"]);
        let rgb_cols = match (col(&["r", "red"]), col(&["g", "green"]), col(&["b", "blue"])) {
            (Some(r), Some(g), Some(b)) => Some((r, g, b)),
            _ => None,
        };
        let has_header = hex_col.is_some() || rgb_cols.is_some();
        let code_col = col(&["code", "number", "no", "no.", "id", "catalog", "catalogue"]);
        let name_col = col(&["name", "description", "colour name", "color name"]);
        let hex = |s: &str| Rgb::from_hex(s.trim()).or_else(|| (s.trim().len() == 6).then(|| Rgb::from_hex(&format!("#{}", s.trim()))).flatten());
        let mut threads = Vec::new();
        let rows = if has_header { None } else { Some(first) }.into_iter().chain(lines);
        for line in rows {
            if threads.len() >= MAX_CHART_THREADS {
                break;
            }
            let f = csv_fields(line);
            let get = |i: Option<usize>| i.and_then(|i| f.get(i)).cloned().unwrap_or_default();
            let color = match (hex_col, rgb_cols) {
                (Some(c), _) => f.get(c).and_then(|v| hex(v)),
                (None, Some((r, g, b))) => {
                    let n = |i: usize| f.get(i).and_then(|v| v.trim().parse::<u8>().ok());
                    match (n(r), n(g), n(b)) {
                        (Some(r), Some(g), Some(b)) => Some(Rgb::new(r, g, b)),
                        _ => None,
                    }
                }
                // no header: the field that looks like a colour, the others code then name
                _ => f.iter().find_map(|v| v.starts_with('#').then(|| hex(v)).flatten()),
            };
            let Some(color) = color else { continue };
            let (code, tname) = if has_header {
                (get(code_col), get(name_col))
            } else {
                let rest: Vec<&String> = f.iter().filter(|v| !v.starts_with('#')).collect();
                (rest.first().map(|s| s.to_string()).unwrap_or_default(), rest.get(1).map(|s| s.to_string()).unwrap_or_default())
            };
            let cut = |s: String| s.chars().take(80).collect::<String>();
            threads.push(Thread::with_code(color, cut(tname), cut(code), name));
        }
        if threads.is_empty() {
            return Err("no threads with a colour found: the CSV needs a colour column (#rrggbb, or r, g and b)".into());
        }
        Ok(ThreadChart { name: name.to_string(), threads })
    }

    /// A chart named `name` from a GIMP palette (`.gpl`): after the `GIMP Palette` line and the
    /// `Name:` and `Columns:` lines, one colour a line as red, green and blue (0–255) and its
    /// name. A number at the start or end of the name (`1800 Black`, `Black 1800`) is taken as
    /// the thread's code.
    pub fn from_gpl(name: &str, text: &str) -> Result<ThreadChart, String> {
        let mut lines = text.lines();
        if !lines.next().is_some_and(|l| l.trim_start_matches('\u{feff}').trim().eq_ignore_ascii_case("GIMP Palette")) {
            return Err("not a GIMP palette: the first line isn't `GIMP Palette`".into());
        }
        let mut threads = Vec::new();
        for line in lines.map(str::trim) {
            if threads.len() >= MAX_CHART_THREADS {
                break;
            }
            if line.is_empty()
                || line.starts_with('#')
                || (line.contains(':') && line.split(':').next().is_some_and(|k| k.chars().all(char::is_alphabetic)))
            {
                continue;
            }
            let mut parts = line.split_whitespace();
            let mut channel = || parts.next().and_then(|v| v.parse::<u8>().ok());
            let (Some(r), Some(g), Some(b)) = (channel(), channel(), channel()) else { continue };
            let label: Vec<&str> = parts.collect();
            let is_code = |w: &&str| {
                w.len() <= 12 && w.chars().any(|c| c.is_ascii_digit()) && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
            };
            let (code, words) = match (label.first(), label.last()) {
                (Some(w), _) if label.len() > 1 && is_code(w) => (w.to_string(), label.get(1..).unwrap_or_default()),
                (_, Some(w)) if label.len() > 1 && is_code(w) => (w.to_string(), label.get(..label.len() - 1).unwrap_or_default()),
                _ => (String::new(), label.as_slice()),
            };
            let cut = |s: String| s.chars().take(80).collect::<String>();
            threads.push(Thread::with_code(Rgb::new(r, g, b), cut(words.join(" ")), cut(code), name));
        }
        if threads.is_empty() {
            return Err("no colours found in the palette".into());
        }
        Ok(ThreadChart { name: name.to_string(), threads })
    }

    /// A chart from a GIMP palette when the text is one, else from CSV.
    pub fn from_text(name: &str, text: &str) -> Result<ThreadChart, String> {
        if text.trim_start_matches('\u{feff}').trim_start().starts_with("GIMP Palette") {
            ThreadChart::from_gpl(name, text)
        } else {
            ThreadChart::from_csv(name, text)
        }
    }
}

/// Index of the colour closest (ΔE) to `c`.
pub fn nearest_index(colors: impl Iterator<Item = Rgb>, c: Rgb) -> Option<usize> {
    let lab = c.to_lab();
    colors.enumerate().map(|(i, k)| (i, de2000(lab, k.to_lab()))).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
}

/// CIEDE2000 between two CIELAB colours (Sharma, Wu and Dalal 2005, equations 1–22; kL = kC =
/// kH = 1).
pub fn de2000(a: [f64; 3], b: [f64; 3]) -> f64 {
    let ([l1, a1, b1], [l2, a2, b2]) = (a, b);
    let pow7 = |v: f64| v.powi(7);
    let c_bar = (a1.hypot(b1) + a2.hypot(b2)) / 2.0;
    let g = 0.5 * (1.0 - (pow7(c_bar) / (pow7(c_bar) + pow7(25.0))).sqrt());
    let (a1p, a2p) = ((1.0 + g) * a1, (1.0 + g) * a2);
    let (c1p, c2p) = (a1p.hypot(b1), a2p.hypot(b2));
    let hue = |b: f64, a: f64| if b == 0.0 && a == 0.0 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) };
    let (h1p, h2p) = (hue(b1, a1p), hue(b2, a2p));
    let dlp = l2 - l1;
    let dcp = c2p - c1p;
    let dh = if c1p * c2p == 0.0 {
        0.0
    } else if (h2p - h1p).abs() <= 180.0 {
        h2p - h1p
    } else if h2p - h1p > 180.0 {
        h2p - h1p - 360.0
    } else {
        h2p - h1p + 360.0
    };
    let dhp = 2.0 * (c1p * c2p).sqrt() * (dh / 2.0).to_radians().sin();
    let lbp = (l1 + l2) / 2.0;
    let cbp = (c1p + c2p) / 2.0;
    let hbp = if c1p * c2p == 0.0 {
        h1p + h2p
    } else if (h1p - h2p).abs() <= 180.0 {
        (h1p + h2p) / 2.0
    } else if h1p + h2p < 360.0 {
        (h1p + h2p + 360.0) / 2.0
    } else {
        (h1p + h2p - 360.0) / 2.0
    };
    let cos = |d: f64| d.to_radians().cos();
    let t = 1.0 - 0.17 * cos(hbp - 30.0) + 0.24 * cos(2.0 * hbp) + 0.32 * cos(3.0 * hbp + 6.0) - 0.20 * cos(4.0 * hbp - 63.0);
    let d_theta = 30.0 * (-((hbp - 275.0) / 25.0).powi(2)).exp();
    let rc = 2.0 * (pow7(cbp) / (pow7(cbp) + pow7(25.0))).sqrt();
    let sl = 1.0 + 0.015 * (lbp - 50.0).powi(2) / (20.0 + (lbp - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cbp;
    let sh = 1.0 + 0.015 * cbp * t;
    let rt = -(2.0 * d_theta).to_radians().sin() * rc;
    let (l, c, h) = (dlp / sl, dcp / sc, dhp / sh);
    (l * l + c * c + h * h + rt * c * h).max(0.0).sqrt()
}

fn chart(name: &'static str, entries: &[(u32, &str, &str)]) -> ThreadChart {
    ThreadChart { name: name.to_string(), threads: entries.iter().map(|&(c, n, code)| Thread::with_code(Rgb::from_u32(c), n, code, name)).collect() }
}

/// Default palette for new designs.
pub fn default_palette() -> Vec<Thread> {
    generic_chart().threads.into_iter().take(12).collect()
}

/// Generic polyester colour set, used for default palettes and for colouring
/// colour-less formats (DST, EXP, ...).
pub fn generic_chart() -> ThreadChart {
    chart("Generic", &GENERIC)
}

/// The generic polyester colours: colour, name, code.
const GENERIC: [(u32, &str, &str); 24] = [
    (0x1a3e8c, "Royal Blue", "G01"),
    (0xd62828, "Red", "G02"),
    (0x111111, "Black", "G03"),
    (0xf6c90e, "Yellow", "G04"),
    (0x2a9d4b, "Kelly Green", "G05"),
    (0xfafafa, "White", "G06"),
    (0xf77f00, "Orange", "G07"),
    (0x6a2c91, "Purple", "G08"),
    (0x0b1f44, "Navy", "G09"),
    (0xc9a227, "Old Gold", "G10"),
    (0x8d99ae, "Silver Grey", "G11"),
    (0x6b3e26, "Brown", "G12"),
    (0xf4a6c6, "Pink", "G13"),
    (0x5fa8d3, "Sky Blue", "G14"),
    (0x7a1f2b, "Maroon", "G15"),
    (0x1b4d2e, "Forest Green", "G16"),
    (0xe9d8a6, "Cream", "G17"),
    (0x3d3d3d, "Charcoal", "G18"),
    (0x00a6a6, "Teal", "G19"),
    (0xb5e48c, "Mint", "G20"),
    (0xe85d75, "Coral", "G21"),
    (0xd4a373, "Tan", "G22"),
    (0x9d4edd, "Lavender", "G23"),
    (0xffd166, "Light Gold", "G24"),
];

/// The name of the generic thread colour nearest `c` ("Sky Blue"): what to call a thread that
/// has no name of its own.
pub fn generic_name(c: Rgb) -> &'static str {
    nearest_index(GENERIC.iter().map(|&(v, _, _)| Rgb::from_u32(v)), c).and_then(|i| GENERIC.get(i)).map_or("Black", |&(_, n, _)| n)
}

/// Brother PEC palette (index 0 is a placeholder; indices 1–64 are the machine colours).
/// Used by PES/PEC writers, which store palette indices rather than RGB values. The table is
/// the one pyembroidery publishes (MIT License, by Tatarize; see `NOTICE`).
pub fn pec_chart() -> ThreadChart {
    chart(
        "Brother",
        &[
            (0x000000, "Unknown", "0"),
            (0x0e1f7c, "Prussian Blue", "1"),
            (0x0a55a3, "Blue", "2"),
            (0x008777, "Teal Green", "3"),
            (0x4b6baf, "Cornflower Blue", "4"),
            (0xed171f, "Red", "5"),
            (0xd15c00, "Reddish Brown", "6"),
            (0x913697, "Magenta", "7"),
            (0xe49acb, "Light Lilac", "8"),
            (0x915fac, "Lilac", "9"),
            (0x9ed67d, "Mint Green", "10"),
            (0xe8a900, "Deep Gold", "11"),
            (0xfeba35, "Orange", "12"),
            (0xffff00, "Yellow", "13"),
            (0x70bc1f, "Lime Green", "14"),
            (0xba9800, "Brass", "15"),
            (0xa8a8a8, "Silver", "16"),
            (0x7d6f00, "Russet Brown", "17"),
            (0xffffb3, "Cream Brown", "18"),
            (0x4f5556, "Pewter", "19"),
            (0x000000, "Black", "20"),
            (0x0b3d91, "Ultramarine", "21"),
            (0x770176, "Royal Purple", "22"),
            (0x293133, "Dark Gray", "23"),
            (0x2a1301, "Dark Brown", "24"),
            (0xf64a8a, "Deep Rose", "25"),
            (0xb27624, "Light Brown", "26"),
            (0xfcbbc5, "Salmon Pink", "27"),
            (0xfe370f, "Vermilion", "28"),
            (0xf0f0f0, "White", "29"),
            (0x6a1c8a, "Violet", "30"),
            (0xa8ddc4, "Seacrest", "31"),
            (0x2584bb, "Sky Blue", "32"),
            (0xfeb343, "Pumpkin", "33"),
            (0xfff36b, "Cream Yellow", "34"),
            (0xd0a660, "Khaki", "35"),
            (0xd15400, "Clay Brown", "36"),
            (0x66ba49, "Leaf Green", "37"),
            (0x134a46, "Peacock Blue", "38"),
            (0x878787, "Gray", "39"),
            (0xd8ccc6, "Warm Gray", "40"),
            (0x435607, "Dark Olive", "41"),
            (0xfdd9de, "Flesh Pink", "42"),
            (0xf993bc, "Pink", "43"),
            (0x003822, "Deep Green", "44"),
            (0xb2afd4, "Lavender", "45"),
            (0x686ab0, "Wisteria Violet", "46"),
            (0xefe3b9, "Beige", "47"),
            (0xf73866, "Carmine", "48"),
            (0xb54b64, "Amber Red", "49"),
            (0x132b1a, "Olive Green", "50"),
            (0xc70156, "Dark Fuchsia", "51"),
            (0xfe9e32, "Tangerine", "52"),
            (0xa8deeb, "Light Blue", "53"),
            (0x00673e, "Emerald Green", "54"),
            (0x4e2990, "Purple", "55"),
            (0x2f7e20, "Moss Green", "56"),
            (0xffcccc, "Flesh Pink", "57"),
            (0xffd911, "Harvest Gold", "58"),
            (0x095ba6, "Electric Blue", "59"),
            (0xf0f970, "Lemon Yellow", "60"),
            (0xe3f35b, "Fresh Green", "61"),
            (0xff9900, "Orange", "62"),
            (0xfff08d, "Cream Yellow", "63"),
            (0xffc8c8, "Applique", "64"),
        ],
    )
}

/// The palette of Janome SEW files (older Janome, Elna and Kenmore machines; index 0 is a
/// placeholder). It is not JEF's: 42 of its 79 colours differ. The table is the one pyembroidery
/// publishes (MIT License, by Tatarize; see `NOTICE`).
pub fn sew_chart() -> ThreadChart {
    chart(
        "Janome SEW",
        &[
            (0x000000, "Placeholder", "0"),
            (0x000000, "Black", "1"),
            (0xffffff, "White", "2"),
            (0xffff17, "Sunflower", "3"),
            (0xfaa060, "Hazel", "4"),
            (0x5c7649, "Green Dust", "5"),
            (0x40c030, "Green", "6"),
            (0x65c2c8, "Sky", "7"),
            (0xac80be, "Purple", "8"),
            (0xf5bccb, "Pink", "9"),
            (0xff0000, "Red", "10"),
            (0xc08000, "Brown", "11"),
            (0x0000f0, "Blue", "12"),
            (0xe4c35d, "Gold", "13"),
            (0xa52a2a, "Dark Brown", "14"),
            (0xd5b0d4, "Pale Violet", "15"),
            (0xfcf294, "Pale Yellow", "16"),
            (0xf0d0c0, "Pale Pink", "17"),
            (0xffc000, "Peach", "18"),
            (0xc9a480, "Beige", "19"),
            (0x9b3d4b, "Wine Red", "20"),
            (0xa0b8cc, "Pale Sky", "21"),
            (0x7fc21c, "Yellow Green", "22"),
            (0xb9b9b9, "Silver Grey", "23"),
            (0xa0a0a0, "Grey", "24"),
            (0x98d6bd, "Pale Aqua", "25"),
            (0xb8f0f0, "Baby Blue", "26"),
            (0x368ba0, "Powder Blue", "27"),
            (0x4f83ab, "Bright Blue", "28"),
            (0x386a91, "Slate Blue", "29"),
            (0x00206b, "Nave Blue", "30"),
            (0xe5c5ca, "Salmon Pink", "31"),
            (0xf9676b, "Coral", "32"),
            (0xe3311f, "Burnt Orange", "33"),
            (0xe2a188, "Cinnamon", "34"),
            (0xb59474, "Umber", "35"),
            (0xe4cf99, "Blonde", "36"),
            (0xe1cb00, "Sunflower", "37"),
            (0xe1add4, "Orchid Pink", "38"),
            (0xc3007e, "Peony Purple", "39"),
            (0x80004b, "Burgundy", "40"),
            (0xa060b0, "Royal Purple", "41"),
            (0xc04020, "Cardinal Red", "42"),
            (0xcae0c0, "Opal Green", "43"),
            (0x899856, "Moss Green", "44"),
            (0x00aa00, "Meadow Green", "45"),
            (0x218a21, "Dark Green", "46"),
            (0x5dae94, "Aquamarine", "47"),
            (0x4cbf8f, "Emerald Green", "48"),
            (0x007772, "Peacock Green", "49"),
            (0x707070, "Dark Grey", "50"),
            (0xf2ffff, "Ivory White", "51"),
            (0xb15818, "Hazel", "52"),
            (0xcb8a07, "Toast", "53"),
            (0xf7927b, "Salmon", "54"),
            (0x98692d, "Cocoa Brown", "55"),
            (0xa27148, "Sienna", "56"),
            (0x7b554a, "Sepia", "57"),
            (0x4f3946, "Dark Sepia", "58"),
            (0x523a97, "Violet Blue", "59"),
            (0x0000a0, "Blue Ink", "60"),
            (0x0096de, "Solar Blue", "61"),
            (0xb2dd53, "Green Dust", "62"),
            (0xfa8fbb, "Crimson", "63"),
            (0xde649e, "Floral Pink", "64"),
            (0xb55066, "Wine", "65"),
            (0x5e5747, "Olive Drab", "66"),
            (0x4c881f, "Meadow", "67"),
            (0xe4dc79, "Canary Yellow", "68"),
            (0xcb8a1a, "Toast", "69"),
            (0xc6aa42, "Beige", "70"),
            (0xecb02c, "Honey Dew", "71"),
            (0xf88040, "Tangerine", "72"),
            (0xffe505, "Ocean Blue", "73"),
            (0xfa7a7a, "Sepia", "74"),
            (0x6be000, "Royal Purple", "75"),
            (0x386cae, "Yellow Ocher", "76"),
            (0xd0bab0, "Beige Grey", "77"),
            (0xe3be81, "Bamboo", "78"),
        ],
    )
}

/// Janome JEF palette (index 0 is a placeholder). The table is the one pyembroidery publishes
/// (MIT License, by Tatarize; see `NOTICE`).
pub fn jef_chart() -> ThreadChart {
    chart(
        "Janome",
        &[
            (0x000000, "Placeholder", "000"),
            (0x000000, "Black", "002"),
            (0xffffff, "White", "001"),
            (0xffff17, "Yellow", "204"),
            (0xff6600, "Orange", "203"),
            (0x2f5933, "Olive Green", "219"),
            (0x237336, "Green", "226"),
            (0x65c2c8, "Sky", "217"),
            (0xab5a96, "Purple", "208"),
            (0xf669a0, "Pink", "201"),
            (0xff0000, "Red", "225"),
            (0xb1704e, "Brown", "214"),
            (0x0b2f84, "Blue", "207"),
            (0xe4c35d, "Gold", "003"),
            (0x481a05, "Dark Brown", "205"),
            (0xac9cc7, "Pale Violet", "209"),
            (0xfcf294, "Pale Yellow", "210"),
            (0xf999b7, "Pale Pink", "211"),
            (0xfab381, "Peach", "212"),
            (0xc9a480, "Beige", "213"),
            (0x970533, "Wine Red", "215"),
            (0xa0b8cc, "Pale Sky", "216"),
            (0x7fc21c, "Yellow Green", "218"),
            (0xe5e5e5, "Silver Gray", "220"),
            (0x889b9b, "Gray", "221"),
            (0x98d6bd, "Pale Aqua", "227"),
            (0xb2e1e3, "Baby Blue", "228"),
            (0x368ba0, "Powder Blue", "229"),
            (0x4f83ab, "Bright Blue", "230"),
            (0x386a91, "Slate Blue", "231"),
            (0x071650, "Navy Blue", "232"),
            (0xf999a2, "Salmon Pink", "233"),
            (0xf9676b, "Coral", "234"),
            (0xe3311f, "Burnt Orange", "235"),
            (0xe2a188, "Cinnamon", "236"),
            (0xb59474, "Umber", "237"),
            (0xe4cf99, "Blond", "238"),
            (0xffcb00, "Sunflower", "239"),
            (0xe1add4, "Orchid Pink", "240"),
            (0xc3007e, "Peony Purple", "241"),
            (0x80004b, "Burgundy", "242"),
            (0x540571, "Royal Purple", "243"),
            (0xb10525, "Cardinal Red", "244"),
            (0xcae0c0, "Opal Green", "245"),
            (0x899856, "Moss Green", "246"),
            (0x5c941a, "Meadow Green", "247"),
            (0x003114, "Dark Green", "248"),
            (0x5dae94, "Aquamarine", "249"),
            (0x4cbf8f, "Emerald Green", "250"),
            (0x007772, "Peacock Green", "251"),
            (0x595b61, "Dark Gray", "252"),
            (0xfffff2, "Ivory White", "253"),
            (0xb15818, "Hazel", "254"),
            (0xcb8a07, "Toast", "255"),
            (0x986c80, "Salmon", "256"),
            (0x98692d, "Cocoa Brown", "257"),
            (0x4d3419, "Sienna", "258"),
            (0x4c330b, "Sepia", "259"),
            (0x33200a, "Dark Sepia", "260"),
            (0x523a97, "Violet Blue", "261"),
            (0x0d217e, "Blue Ink", "262"),
            (0x1e77ac, "Sola Blue", "263"),
            (0xb2dd53, "Green Dust", "264"),
            (0xf33689, "Crimson", "265"),
            (0xde649e, "Floral Pink", "266"),
            (0x984161, "Wine", "267"),
            (0x4c5612, "Olive Drab", "268"),
            (0x4c881f, "Meadow", "269"),
            (0xe4de79, "Mustard", "270"),
            (0xcb8a1a, "Yellow Ocher", "271"),
            (0xcba21c, "Old Gold", "272"),
            (0xff9805, "Honey Dew", "273"),
            (0xfcb257, "Tangerine", "274"),
            (0xffe505, "Canary Yellow", "275"),
            (0xf0331f, "Vermilion", "202"),
            (0x1a842d, "Bright Green", "206"),
            (0x386cae, "Ocean Blue", "222"),
            (0xe3c4b4, "Beige Gray", "223"),
            (0xe3ac81, "Bamboo", "224"),
        ],
    )
}

/// All built-in thread charts.
pub fn charts() -> Vec<ThreadChart> {
    vec![generic_chart(), pec_chart(), jef_chart(), hus_chart()]
}

/// Husqvarna Viking's HUS palette (index 0 first), from pyembroidery (MIT).
pub fn hus_chart() -> ThreadChart {
    chart(
        "Husqvarna",
        &[
            (0x000000, "Black", "026"),
            (0x0000e7, "Blue", "005"),
            (0x00c600, "Green", "002"),
            (0xff0000, "Red", "014"),
            (0x840084, "Purple", "008"),
            (0xffff00, "Yellow", "020"),
            (0x848484, "Grey", "024"),
            (0x8484e7, "Light Blue", "006"),
            (0x00ff84, "Light Green", "003"),
            (0xff7b31, "Orange", "017"),
            (0xff8ca5, "Pink", "011"),
            (0x845200, "Brown", "028"),
            (0xffffff, "White", "022"),
            (0x000084, "Dark Blue", "004"),
            (0x008400, "Dark Green", "001"),
            (0x7b0000, "Dark Red", "013"),
            (0xff6384, "Light Red", "015"),
            (0x522952, "Dark Purple", "007"),
            (0xff00ff, "Light Purple", "009"),
            (0xffde00, "Dark Yellow", "019"),
            (0xffff9c, "Light Yellow", "021"),
            (0x525252, "Dark Grey", "025"),
            (0xd6d6d6, "Light Grey", "023"),
            (0xff5208, "Dark Orange", "016"),
            (0xff9c5a, "Light Orange", "018"),
            (0xff52b5, "Dark Pink", "010"),
            (0xffc6de, "Light Pink", "012"),
            (0x523100, "Dark Brown", "027"),
            (0xb5a584, "Light Brown", "029"),
        ],
    )
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_thread_without_a_name_is_called_after_the_nearest_generic_colour() {
        use super::{Rgb, generic_name};
        assert_eq!(generic_name(Rgb::from_u32(0x1a3e8c)), "Royal Blue");
        assert_eq!(generic_name(Rgb::WHITE), "White");
        assert_eq!(generic_name(Rgb::from_u32(0x4a90c8)), "Sky Blue");
        assert_eq!(generic_name(Rgb::from_u32(0xe8a820)), "Old Gold");
    }

    #[test]
    fn ciede2000_matches_the_published_test_pairs() {
        // pairs 1, 7, 13, 17, 19, 25 and 34 of Sharma, Wu and Dalal's Table 1
        for (a, b, want) in [
            ([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485], 2.0425),
            ([50.0, 0.0, 0.0], [50.0, -1.0, 2.0], 2.3669),
            ([50.0, 2.5, 0.0], [56.0, -27.0, -3.0], 31.9030),
            ([50.0, 2.5, 0.0], [73.0, 25.0, -18.0], 27.1492),
            ([50.0, 2.5, 0.0], [50.0, 3.1736, 0.5854], 1.0000),
            ([60.2574, -34.0099, 36.2677], [60.4626, -34.1751, 39.4387], 1.2644),
            ([90.8027, -2.0831, 1.4410], [91.1528, -1.6435, 0.0447], 1.4441),
        ] {
            let got = de2000(a, b);
            assert!((got - want).abs() < 1e-4, "{a:?} {b:?}: {got} not {want}");
            assert!((de2000(b, a) - want).abs() < 1e-4, "symmetric");
        }
        assert_eq!(de2000([40.0, 10.0, -5.0], [40.0, 10.0, -5.0]), 0.0);
        // the thread that looks nearest: a mid blue is Cornflower, not the darker Ultramarine
        let blues = [Rgb::new(75, 107, 175), Rgb::new(11, 61, 145)];
        assert_eq!(nearest_index(blues.into_iter(), Rgb::new(67, 99, 216)), Some(0));
    }

    /// D6: a GIMP palette becomes a thread chart, codes taken from the start or end of a name.
    #[test]
    fn gimp_palettes_become_thread_charts() {
        let gpl = "GIMP Palette\nName: My threads\nColumns: 4\n# a comment\n  0   0   0\t1800 Black\n255 255 255\tWhite 1801\n200  16  46\tPoppy Red\n12 34\tbroken\n";
        let c = ThreadChart::from_text("Mine", gpl).unwrap();
        let got: Vec<(String, String, String)> = c.threads.iter().map(|t| (t.color.to_hex(), t.code.clone(), t.name.clone())).collect();
        assert_eq!(
            got,
            [
                ("#000000".to_string(), "1800".to_string(), "Black".to_string()),
                ("#ffffff".to_string(), "1801".to_string(), "White".to_string()),
                ("#c8102e".to_string(), String::new(), "Poppy Red".to_string()),
            ]
        );
        assert!(c.threads.iter().all(|t| t.chart == "Mine"));
        assert!(ThreadChart::from_gpl("x", "code,name,color\n1,Red,#ff0000").is_err());
        assert!(ThreadChart::from_text("x", "code,name,color\n1,Red,#ff0000").is_ok());
        assert!(ThreadChart::from_gpl("x", "GIMP Palette\nName: empty\n").is_err());
    }

    use super::*;

    #[test]
    fn charts_read_and_write_csv() {
        let c = ThreadChart::from_csv("Shop", "Code;Name;Hex\n1001;\"Red; bright\";#d62828\n1002;Navy;1d3557\nbad;row;zz\n").unwrap();
        assert_eq!(c.threads.len(), 2);
        assert_eq!((c.threads[0].code.as_str(), c.threads[0].name.as_str(), c.threads[0].chart.as_str()), ("1001", "Red; bright", "Shop"));
        assert_eq!(c.threads[1].color, Rgb::new(0x1d, 0x35, 0x57));
        let back = ThreadChart::from_csv("Shop", &c.to_csv()).unwrap();
        assert_eq!(back, c);
        let rgb = ThreadChart::from_csv("RGB", "name,r,g,b,number\nWhite,255,255,255,1\n").unwrap();
        assert_eq!((rgb.threads[0].color, rgb.threads[0].code.as_str()), (Rgb::new(255, 255, 255), "1"));
        let bare = ThreadChart::from_csv("Bare", "5, Gold, #ffcc00\n").unwrap();
        assert_eq!((bare.threads[0].code.as_str(), bare.threads[0].name.as_str()), ("5", "Gold"));
        assert!(ThreadChart::from_csv("None", "a,b\n1,2\n").is_err());
        assert!(ThreadChart::from_csv("Empty", "").is_err());
    }

    #[test]
    fn hex_round_trip() {
        let c = Rgb::from_hex("#1a3e8c").unwrap();
        assert_eq!(c, Rgb::new(0x1a, 0x3e, 0x8c));
        assert_eq!(c.to_hex(), "#1a3e8c");
        assert_eq!(Rgb::from_hex("f00"), Some(Rgb::new(255, 0, 0)));
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, "\"#1a3e8c\"");
    }

    #[test]
    fn nearest_thread() {
        let pec = pec_chart();
        assert_eq!(pec.threads.len(), 65);
        assert_eq!(pec.threads[pec.nearest(Rgb::new(250, 250, 0))].name, "Yellow");
        assert_eq!(jef_chart().threads.len(), 79);
        assert_eq!(hus_chart().threads.len(), 29);
    }
}
