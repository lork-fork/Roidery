//! Barcodes for worksheets, as bar and space widths in modules (the narrowest element), bar
//! first: Code 128 in its code set B (ISO/IEC 15417) and Code 39 (ISO/IEC 16388). Written from
//! the standards' public descriptions (https://en.wikipedia.org/wiki/Code_128,
//! https://en.wikipedia.org/wiki/Code_39); quiet zones (10 modules either side) are the
//! caller's.

/// A barcode symbology.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbology {
    Code128,
    Code39,
}

impl Symbology {
    pub fn from_name(name: &str) -> Option<Symbology> {
        match name.to_ascii_lowercase().as_str() {
            "code128" | "128" => Some(Symbology::Code128),
            "code39" | "39" => Some(Symbology::Code39),
            _ => None,
        }
    }
}

/// The longest text encoded.
pub const MAX_LEN: usize = 48;

/// `text` as alternating bar and space widths (modules), starting and ending with a bar. None
/// when it is empty, longer than [`MAX_LEN`] or has a character the symbology lacks (Code 39:
/// digits, capitals, space and `-.$/+%`, letters taken as capitals; Code 128: printable ASCII).
pub fn encode(kind: Symbology, text: &str) -> Option<Vec<u8>> {
    if text.is_empty() || text.chars().count() > MAX_LEN {
        return None;
    }
    match kind {
        Symbology::Code128 => code128(text),
        Symbology::Code39 => code39(text),
    }
}

/// Code 128's symbols 0–105 (105: Start C), six widths each (bar, space, …), then Stop.
const C128: [&[u8; 6]; 106] = [
    b"212222", b"222122", b"222221", b"121223", b"121322", b"131222", b"122213", b"122312", b"132212", b"221213", //
    b"221312", b"231212", b"112232", b"122132", b"122231", b"113222", b"123122", b"123221", b"223211", b"221132", //
    b"221231", b"213212", b"223112", b"312131", b"311222", b"321122", b"321221", b"312212", b"322112", b"322211", //
    b"212123", b"212321", b"232121", b"111323", b"131123", b"131321", b"112313", b"132113", b"132311", b"211313", //
    b"231113", b"231311", b"112133", b"112331", b"132131", b"113123", b"113321", b"133121", b"313121", b"211331", //
    b"231131", b"213113", b"213311", b"213131", b"311123", b"311321", b"331121", b"312113", b"312311", b"332111", //
    b"314111", b"221411", b"431111", b"111224", b"111422", b"121124", b"121421", b"141122", b"141221", b"112214", //
    b"112412", b"122114", b"122411", b"142112", b"142211", b"241211", b"221114", b"413111", b"241112", b"134111", //
    b"111242", b"121142", b"121241", b"114212", b"124112", b"124211", b"411212", b"421112", b"421211", b"212141", //
    b"214121", b"412121", b"111143", b"111341", b"131141", b"114113", b"114311", b"411113", b"411311", b"113141", //
    b"114131", b"311141", b"411131", b"211412", b"211214", b"211232",
];
const C128_START_B: usize = 104;
const C128_STOP: &[u8; 7] = b"2331112";

fn code128(text: &str) -> Option<Vec<u8>> {
    // code set B: a character's value is its code less 32
    let values: Vec<usize> = text.chars().map(|c| u32::from(c).checked_sub(32).filter(|v| *v < 95).map(|v| v as usize)).collect::<Option<_>>()?;
    let check = values.iter().enumerate().fold(C128_START_B, |sum, (i, v)| sum + (i + 1) * v) % 103;
    let mut out = Vec::with_capacity((values.len() + 3) * 6 + 1);
    for v in std::iter::once(C128_START_B).chain(values).chain(std::iter::once(check)) {
        out.extend(C128.get(v)?.iter().map(|d| d - b'0'));
    }
    out.extend(C128_STOP.iter().map(|d| d - b'0'));
    Some(out)
}

/// Code 39's characters in table order: four groups of ten share their bars (two of five wide,
/// in the order of [`C39_BARS`]); a group's wide space is its second, third, fourth or first.
const C39_GROUPS: [&str; 4] = ["1234567890", "ABCDEFGHIJ", "KLMNOPQRST", "UVWXYZ-. *"];
/// The wide bars of each place in a group.
const C39_BARS: [[usize; 2]; 10] = [[0, 4], [1, 4], [0, 1], [2, 4], [0, 2], [1, 2], [3, 4], [0, 3], [1, 3], [2, 3]];
/// The wide space of each group.
const C39_SPACE: [usize; 4] = [1, 2, 3, 0];
/// The four with narrow bars and three wide spaces: the narrow space of each.
const C39_SPECIAL: [(char, usize); 4] = [('$', 3), ('/', 2), ('+', 1), ('%', 0)];
/// A wide element, in modules.
const C39_WIDE: u8 = 3;

/// One Code 39 character's nine widths.
fn code39_char(c: char) -> Option<[u8; 9]> {
    let mut w = [1u8; 9];
    if let Some((_, narrow)) = C39_SPECIAL.iter().find(|(s, _)| *s == c) {
        for k in (0..4).filter(|k| k != narrow) {
            w[2 * k + 1] = C39_WIDE;
        }
        return Some(w);
    }
    let (g, place) = C39_GROUPS.iter().enumerate().find_map(|(g, chars)| chars.chars().position(|x| x == c).map(|p| (g, p)))?;
    for b in C39_BARS.get(place)? {
        w[2 * b] = C39_WIDE;
    }
    w[2 * C39_SPACE.get(g)? + 1] = C39_WIDE;
    Some(w)
}

fn code39(text: &str) -> Option<Vec<u8>> {
    let text = text.to_ascii_uppercase();
    if text.contains('*') {
        return None;
    }
    let mut out = Vec::new();
    for c in std::iter::once('*').chain(text.chars()).chain(std::iter::once('*')) {
        if !out.is_empty() {
            // the narrow gap between characters
            out.push(1);
        }
        out.extend(code39_char(c)?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code128_symbols_keep_the_standards_rules() {
        let mut seen = std::collections::HashSet::new();
        for (v, s) in C128.iter().enumerate() {
            let w: Vec<u8> = s.iter().map(|d| d - b'0').collect();
            assert!(w.iter().all(|x| (1..=4).contains(x)), "{v}");
            assert_eq!(w.iter().map(|x| u32::from(*x)).sum::<u32>(), 11, "{v}: eleven modules");
            assert_eq!((w[0] + w[2] + w[4]) % 2, 0, "{v}: an even count of bar modules");
            assert!(seen.insert(*s), "{v}: twice");
        }
        assert_eq!(C128_STOP.iter().map(|d| u32::from(d - b'0')).sum::<u32>(), 13);
    }

    #[test]
    fn code128_starts_checks_and_stops() {
        let w = encode(Symbology::Code128, "AB").unwrap();
        // start B, two characters, the check, stop
        assert_eq!(w.len(), 4 * 6 + 7);
        assert_eq!(w.iter().map(|x| u32::from(*x)).sum::<u32>(), 4 * 11 + 13);
        assert_eq!(&w[..6], &[2, 1, 1, 2, 1, 4]);
        // "A" is 33, "B" 34: (104 + 33 + 2 × 34) mod 103 = 102
        let check: Vec<u8> = C128[102].iter().map(|d| d - b'0').collect();
        assert_eq!(&w[18..24], check.as_slice());
        assert!(encode(Symbology::Code128, "é").is_none() && encode(Symbology::Code128, "").is_none());
        assert!(encode(Symbology::Code128, &"x".repeat(MAX_LEN + 1)).is_none());
    }

    #[test]
    fn code39_characters_have_three_wide_elements_of_nine() {
        let mut seen = std::collections::HashSet::new();
        let all: String = C39_GROUPS.concat() + "$/+%";
        assert_eq!(all.chars().count(), 44);
        for c in all.chars() {
            let w = code39_char(c).unwrap();
            assert_eq!(w.iter().filter(|x| **x == C39_WIDE).count(), 3, "{c}");
            let wide_bars = (0..5).filter(|k| w[2 * k] == C39_WIDE).count();
            assert!(wide_bars == 2 || "$/+%".contains(c), "{c}");
            assert!(seen.insert(w), "{c}: twice");
        }
        // the start and stop character: n W n n W n W n n
        assert_eq!(code39_char('*').unwrap(), [1, 3, 1, 1, 3, 1, 3, 1, 1]);
        // "0": n n n W W n W n n; "A": W n n n n W n n W
        assert_eq!(code39_char('0').unwrap(), [1, 1, 1, 3, 3, 1, 3, 1, 1]);
        assert_eq!(code39_char('A').unwrap(), [3, 1, 1, 1, 1, 3, 1, 1, 3]);
        let w = encode(Symbology::Code39, "ab-1").unwrap();
        assert_eq!(w.len(), 6 * 9 + 5);
        assert!(encode(Symbology::Code39, "a*b").is_none() && encode(Symbology::Code39, "a_b").is_none());
    }
}
