//! Brother PES, version 1 (`#PES0001`), with its PEC block.
//!
//! A PES file holds two copies of the design: the PES section, which design software reads, and
//! the PEC block, which is what Brother (and Babylock, Bernina deco) machines sew from. All
//! numbers are little-endian unless noted.
//!
//! **PES section:** `#PES0001`, the PEC block's offset (u32), then u16 1 (scale to fit), the hoop
//! (u16: 0 = 100 × 100, 1 = 130 × 180 mm), the object count (u16), `FFFF 0000`, and one
//! `CEmbOne` object: its name (u16 length, ASCII), eight zero u16s, an affine transform (six
//! f32s placing the design in a 130 × 180 hoop space whose centre, (1000, 1000), is the design's
//! origin),
//! u16 1, two zero u16s, the width and height (u16, 0.1 mm), eight zero bytes and the segment
//! count (u16). Then `FFFF 0000`, the `CSewSeg` name and the segments: each is a type (u16: 0
//! stitches, 1 a jump), a PEC colour number, a point count and the points (i16 pairs, relative
//! to the design's bottom left), with `8003` between segments. A colour log follows (count, then
//! (segment, colour) pairs) and four zero bytes.
//!
//! **PEC block:** `LA:` and the name in 16 characters, CR, 12 spaces, `FF 00`, the thumbnail
//! width in bytes (6) and height (38), 12 spaces, the colour count less one (u8), one PEC colour
//! number per colour block, spaces up to 463 bytes after the count, `00 00`, then the stitch
//! block: its length (u24, counted from those two zeros), `31 FF F0`, the width and height (u16,
//! 0.1 mm), `E0 01 B0 01`, the start's distance from the left and top (u16 big-endian, ORed with
//! `0x9000`), and the stitches, in 0.1 mm with y down:
//!
//! - a short move: two bytes of 7-bit two's complement (−63 to 62);
//! - a long move: two big-endian u16s, `0x8000 | (move & 0x0FFF)` with `0x1000` for a jump or
//!   `0x2000` for a jump that trims first;
//! - `FE B0 nn`: a colour change (`nn` alternates 2 and 1);
//! - `FF`: the end.
//!
//! After the stitches come the thumbnails the machine shows: 48 × 38 pixels at one bit each
//! (6 bytes a row, leftmost pixel in the low bit), one for the whole design and one per colour.

use crate::{Bytes, Command, Design, FormatError, Reader, Thread, block_threads, nearest, steps, to_units};

const FORMAT: &str = "PES";
/// Longest PEC move per axis, 0.1 mm (12-bit two's complement).
const MAX_LONG: i32 = 2047;
const THUMB_W: usize = 48;
const THUMB_H: usize = 38;

/// Brother's PEC thread chart, by colour number (0 is unused).
pub const PALETTE: [([u8; 3], &str); 65] = [
    ([0, 0, 0], "Unknown"),
    ([14, 31, 124], "Prussian Blue"),
    ([10, 85, 163], "Blue"),
    ([0, 135, 119], "Teal Green"),
    ([75, 107, 175], "Cornflower Blue"),
    ([237, 23, 31], "Red"),
    ([209, 92, 0], "Reddish Brown"),
    ([145, 54, 151], "Magenta"),
    ([228, 154, 203], "Light Lilac"),
    ([145, 95, 172], "Lilac"),
    ([158, 214, 125], "Mint Green"),
    ([232, 169, 0], "Deep Gold"),
    ([254, 186, 53], "Orange"),
    ([255, 255, 0], "Yellow"),
    ([112, 188, 31], "Lime Green"),
    ([186, 152, 0], "Brass"),
    ([168, 168, 168], "Silver"),
    ([125, 111, 0], "Russet Brown"),
    ([255, 255, 179], "Cream Brown"),
    ([79, 85, 86], "Pewter"),
    ([0, 0, 0], "Black"),
    ([11, 61, 145], "Ultramarine"),
    ([119, 1, 118], "Royal Purple"),
    ([41, 49, 51], "Dark Gray"),
    ([42, 19, 1], "Dark Brown"),
    ([246, 74, 138], "Deep Rose"),
    ([178, 118, 36], "Light Brown"),
    ([252, 187, 197], "Salmon Pink"),
    ([254, 55, 15], "Vermilion"),
    ([240, 240, 240], "White"),
    ([106, 28, 138], "Violet"),
    ([168, 221, 196], "Seacrest"),
    ([37, 132, 187], "Sky Blue"),
    ([254, 179, 67], "Pumpkin"),
    ([255, 243, 107], "Cream Yellow"),
    ([208, 166, 96], "Khaki"),
    ([209, 84, 0], "Clay Brown"),
    ([102, 186, 73], "Leaf Green"),
    ([19, 74, 70], "Peacock Blue"),
    ([135, 135, 135], "Gray"),
    ([216, 204, 198], "Warm Gray"),
    ([67, 86, 7], "Dark Olive"),
    ([253, 217, 222], "Flesh Pink"),
    ([249, 147, 188], "Pink"),
    ([0, 56, 34], "Deep Green"),
    ([178, 175, 212], "Lavender"),
    ([104, 106, 176], "Wisteria Violet"),
    ([239, 227, 185], "Beige"),
    ([247, 56, 102], "Carmine"),
    ([181, 75, 100], "Amber Red"),
    ([19, 43, 26], "Olive Green"),
    ([199, 1, 86], "Dark Fuchsia"),
    ([254, 158, 50], "Tangerine"),
    ([168, 222, 235], "Light Blue"),
    ([0, 103, 62], "Emerald Green"),
    ([78, 41, 144], "Purple"),
    ([47, 126, 32], "Moss Green"),
    ([255, 204, 204], "Flesh Pink"),
    ([255, 217, 17], "Harvest Gold"),
    ([9, 91, 166], "Electric Blue"),
    ([240, 249, 112], "Lemon Yellow"),
    ([227, 243, 91], "Fresh Green"),
    ([255, 153, 0], "Orange"),
    ([255, 240, 141], "Cream Yellow"),
    ([255, 200, 200], "Applique"),
];

fn pec_color(t: &Thread) -> u8 {
    let palette: Vec<[u8; 3]> = PALETTE.iter().map(|p| p.0).collect();
    u8::try_from(nearest(t.rgb, &palette, 1)).unwrap_or(20)
}

fn u16le(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_le_bytes());
}

fn i16le(out: &mut Vec<u8>, v: i32) {
    out.extend((v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16).to_le_bytes());
}

/// ASCII for the file's name fields: other characters become `_`.
fn ascii(name: &str, max: usize) -> Vec<u8> {
    name.chars().take(max).map(|c| if c.is_ascii_graphic() || c == ' ' { c as u8 } else { b'_' }).collect()
}

/// One PES segment: 0 stitches or 1 a jump, its colour, its points.
struct Segment {
    kind: u16,
    color: u8,
    points: Vec<(i32, i32)>,
}

pub fn write(design: &Design) -> Result<Vec<u8>, FormatError> {
    let units = to_units(design, FORMAT)?;
    let threads = block_threads(design);
    let colors: Vec<u8> = threads.iter().map(pec_color).collect();
    let sewn: Vec<(i32, i32)> = units.iter().filter(|u| u.2 != Command::End).map(|u| (u.0, u.1)).collect();
    let (left, top, right, bottom) =
        sewn.iter().fold((i32::MAX, i32::MAX, i32::MIN, i32::MIN), |(l, t, r, b), &(x, y)| (l.min(x), t.min(y), r.max(x), b.max(y)));
    let (left, top, right, bottom) = if sewn.is_empty() { (0, 0, 0, 0) } else { (left, top, right, bottom) };
    let (w, h) = (right - left, bottom - top);

    let mut out = Vec::with_capacity(units.len() * 6 + 2048);
    out.extend(b"#PES0001");
    out.extend([0u8; 4]); // the PEC offset, filled in below
    u16le(&mut out, 1);
    u16le(&mut out, u16::from(w > 1000 || h > 1000));
    u16le(&mut out, u16::from(!sewn.is_empty()));
    u16le(&mut out, 0xFFFF);
    u16le(&mut out, 0);
    if !sewn.is_empty() {
        pes_object(&mut out, &units, &colors, (left, bottom), (w, h));
    }
    let pec_at = u32::try_from(out.len()).map_err(|_| FormatError::TooLarge { format: FORMAT, why: "the file passes 4 GB".into() })?;
    if let Some(slot) = out.get_mut(8..12) {
        slot.copy_from_slice(&pec_at.to_le_bytes());
    }
    pec(&mut out, design, &units, &colors, (left, top, w, h))?;
    Ok(out)
}

/// The `CEmbOne` object and its `CSewSeg` segments.
fn pes_object(out: &mut Vec<u8>, units: &[(i32, i32, Command)], colors: &[u8], (left, bottom): (i32, i32), (w, h): (i32, i32)) {
    // Segments, by colour block.
    let mut segs: Vec<Segment> = vec![];
    let mut block = 0usize;
    let mut at = (0, 0);
    let mut open: Option<Segment> = None;
    for &(x, y, command) in units {
        let color = colors.get(block).copied().unwrap_or(20);
        let kind = match command {
            Command::Stitch => Some(0),
            Command::Jump => Some(1),
            _ => None,
        };
        match kind {
            Some(k) => {
                if open.as_ref().is_none_or(|s| s.kind != k || s.color != color) {
                    segs.extend(open.take());
                    open = Some(Segment { kind: k, color, points: vec![at] });
                }
                if let Some(s) = open.as_mut() {
                    s.points.push((x, y));
                }
            }
            None => {
                if command == Command::ColorChange {
                    segs.extend(open.take());
                    block += 1;
                }
            }
        }
        if command != Command::End {
            at = (x, y);
        }
    }
    segs.extend(open);
    // Single-point segments say nothing.
    segs.retain(|s| s.points.len() > 1);

    let name = b"CEmbOne";
    u16le(out, name.len() as u16);
    out.extend(name);
    out.extend([0u8; 16]);
    // Place the design in the hoop space, whose centre (1000, 1000) is the design's origin
    // (where the needle starts): a design centred on its origin lands in the middle.
    let tx = 1000.0 + f64::from(left);
    let ty = 1000.0 + f64::from(bottom);
    for v in [1.0f32, 0.0, 0.0, 1.0, tx as f32, ty as f32] {
        out.extend(v.to_le_bytes());
    }
    u16le(out, 1);
    u16le(out, 0);
    u16le(out, 0);
    i16le(out, w);
    i16le(out, h);
    out.extend([0u8; 8]);
    i16le(out, i32::try_from(segs.len()).unwrap_or(i32::MAX));
    u16le(out, 0xFFFF);
    u16le(out, 0);
    let name = b"CSewSeg";
    u16le(out, name.len() as u16);
    out.extend(name);
    let mut log: Vec<(usize, u8)> = vec![];
    for (i, s) in segs.iter().enumerate() {
        if i > 0 {
            u16le(out, 0x8003);
        }
        if log.last().is_none_or(|l| l.1 != s.color) {
            log.push((i, s.color));
        }
        u16le(out, s.kind);
        u16le(out, u16::from(s.color));
        i16le(out, i32::try_from(s.points.len()).unwrap_or(i32::MAX));
        for &(x, y) in &s.points {
            i16le(out, x - left);
            i16le(out, y - bottom);
        }
    }
    i16le(out, i32::try_from(log.len()).unwrap_or(i32::MAX));
    for (seg, color) in log {
        i16le(out, i32::try_from(seg).unwrap_or(i32::MAX));
        u16le(out, u16::from(color));
    }
    out.extend([0u8; 4]);
}

/// A PEC long-form move with its flags (`0x10` jump, `0x20` trim).
fn long(out: &mut Vec<u8>, v: i32, flags: u8) {
    let v = (v.clamp(-MAX_LONG, MAX_LONG) & 0x0FFF) as u16 | 0x8000 | (u16::from(flags) << 8);
    out.extend(v.to_be_bytes());
}

/// The PEC block, appended to `out`.
fn pec(
    out: &mut Vec<u8>,
    design: &Design,
    units: &[(i32, i32, Command)],
    colors: &[u8],
    (left, top, w, h): (i32, i32, i32, i32),
) -> Result<(), FormatError> {
    out.extend(b"LA:");
    let mut name = ascii(&design.name, 16);
    name.resize(16, b' ');
    out.extend(name);
    out.push(0x0D);
    out.extend([b' '; 12]);
    out.extend([0xFF, 0x00, (THUMB_W / 8) as u8, THUMB_H as u8]);
    out.extend([b' '; 12]);
    let n = colors.len().clamp(1, 255);
    out.push((n - 1) as u8);
    out.extend(colors.iter().take(255));
    out.extend(std::iter::repeat_n(b' ', 463usize.saturating_sub(n)));
    let block = out.len();
    out.extend([0, 0, 0, 0, 0]);
    out.extend([0x31, 0xFF, 0xF0]);
    i16le(out, w);
    i16le(out, h);
    out.extend([0xE0, 0x01, 0xB0, 0x01]);
    out.extend((0x9000u16 | ((-left).clamp(0, 0x0FFF) as u16)).to_be_bytes());
    out.extend((0x9000u16 | ((-top).clamp(0, 0x0FFF) as u16)).to_be_bytes());

    let (mut x, mut y) = (0, 0);
    let mut trim = false;
    let mut stop = 2u8;
    for &(tx, ty, command) in units {
        let (dx, dy) = (tx - x, ty - y);
        match command {
            Command::Stitch => {
                if trim {
                    // A trim with no jump after it: cut in place.
                    long(out, 0, 0x20);
                    long(out, 0, 0x20);
                    trim = false;
                }
                for (sx, sy) in steps(dx, dy, MAX_LONG) {
                    if (-63..=62).contains(&sx) && (-63..=62).contains(&sy) {
                        out.extend([(sx & 0x7F) as u8, (sy & 0x7F) as u8]);
                    } else {
                        long(out, sx, 0);
                        long(out, sy, 0);
                    }
                }
            }
            Command::Jump | Command::Trim | Command::ColorChange => {
                if dx != 0 || dy != 0 {
                    let flags = if trim { 0x20 } else { 0x10 };
                    for (sx, sy) in steps(dx, dy, MAX_LONG) {
                        long(out, sx, flags);
                        long(out, sy, flags);
                    }
                    trim = false;
                }
                match command {
                    Command::Trim => trim = true,
                    Command::ColorChange => {
                        out.extend([0xFE, 0xB0, stop]);
                        stop = if stop == 2 { 1 } else { 2 };
                    }
                    _ => {}
                }
            }
            Command::End => break,
        }
        (x, y) = (tx, ty);
    }
    out.extend([0xFF, 0x00]);
    let len = out.len() - block;
    if len > 0x00FF_FFFF {
        return Err(FormatError::TooLarge { format: FORMAT, why: "the PEC stitch block passes 16 MB".into() });
    }
    if let Some(slot) = out.get_mut(block + 2..block + 5) {
        slot.copy_from_slice(&(len as u32).to_le_bytes()[..3]);
    }
    thumbnails(out, units, (left, top, w, h));
    Ok(())
}

/// The design's thumbnail, then one per colour block.
fn thumbnails(out: &mut Vec<u8>, units: &[(i32, i32, Command)], (left, top, w, h): (i32, i32, i32, i32)) {
    // The picture area inside the frame, keeping the design's proportions.
    let (aw, ah) = (40.0, 30.0);
    let scale = (aw / f64::from(w.max(1))).min(ah / f64::from(h.max(1)));
    let (ox, oy) = (4.0 + (aw - f64::from(w) * scale) / 2.0, 4.0 + (ah - f64::from(h) * scale) / 2.0);
    let plot = |img: &mut [[bool; THUMB_W]; THUMB_H], x: i32, y: i32| {
        let px = (ox + f64::from(x - left) * scale).round() as usize;
        let py = (oy + f64::from(y - top) * scale).round() as usize;
        if let Some(p) = img.get_mut(py).and_then(|r| r.get_mut(px)) {
            *p = true;
        }
    };
    let mut all = frame();
    let mut blocks = vec![frame()];
    for &(x, y, command) in units {
        match command {
            Command::Stitch => {
                plot(&mut all, x, y);
                if let Some(b) = blocks.last_mut() {
                    plot(b, x, y);
                }
            }
            Command::ColorChange => blocks.push(frame()),
            _ => {}
        }
    }
    for img in std::iter::once(&all).chain(blocks.iter()) {
        for row in img {
            for byte in row.chunks(8) {
                out.push(byte.iter().enumerate().fold(0u8, |acc, (i, &on)| acc | (u8::from(on) << i)));
            }
        }
    }
}

/// An empty thumbnail with a rounded frame.
fn frame() -> [[bool; THUMB_W]; THUMB_H] {
    let mut img = [[false; THUMB_W]; THUMB_H];
    for y in [1, 36] {
        img[y][4..44].fill(true);
    }
    for row in img.iter_mut().take(34).skip(4) {
        row[1] = true;
        row[46] = true;
    }
    for (x, y) in [(2, 3), (3, 2), (2, 34), (3, 35), (45, 3), (44, 2), (45, 34), (44, 35)] {
        img[y][x] = true;
    }
    img
}

pub fn read(bytes: &[u8]) -> Result<Design, FormatError> {
    let mut b = Bytes::new(bytes, FORMAT);
    let magic = b.take(8)?;
    if !magic.starts_with(b"#PES") {
        return Err(FormatError::Invalid { format: FORMAT, why: "it doesn't start with #PES".into() });
    }
    let pec = usize::try_from(b.i32()?).map_err(|_| b.short())?;
    read_pec(bytes, pec)
}

/// Read the PEC block that starts at `pec`.
fn read_pec(bytes: &[u8], pec: usize) -> Result<Design, FormatError> {
    let mut b = Bytes::new(bytes, FORMAT);
    b.seek(pec)?;
    let label = b.take(20)?;
    let name = String::from_utf8_lossy(label.get(3..19).unwrap_or(&[])).trim().to_string();
    b.seek(pec + 48)?;
    let n = usize::from(b.u8()?) + 1;
    let mut threads = vec![];
    for _ in 0..n {
        let i = usize::from(b.u8()?);
        let (rgb, name) = PALETTE.get(i).copied().unwrap_or(([0, 0, 0], ""));
        threads.push(Thread { rgb, name: name.into(), code: i.to_string() });
    }
    b.seek(pec + 512 + 20)?;
    let mut r = Reader::new();
    let mut last_trim = false;
    // One axis: a short move (one byte) or a long one (two, high bit set), and its flags.
    let axis = |first: u8, b: &mut Bytes| -> Result<(i32, u8), FormatError> {
        if first & 0x80 != 0 {
            let lo = b.u8()?;
            let v = (i32::from(first & 0x0F) << 8) | i32::from(lo);
            Ok((if v & 0x800 != 0 { v - 0x1000 } else { v }, first & 0x30))
        } else {
            let v = i32::from(first);
            Ok((if v >= 0x40 { v - 0x80 } else { v }, 0))
        }
    };
    while let Ok(a) = b.u8() {
        if a == 0xFF {
            break;
        }
        if a == 0xFE {
            // FE B0 nn: a colour change.
            let _ = b.take(2);
            r.add(0, 0, Command::ColorChange, FORMAT)?;
            continue;
        }
        let (dx, fx) = axis(a, &mut b)?;
        let Ok(c) = b.u8() else { break };
        let (dy, fy) = axis(c, &mut b)?;
        let flags = fx | fy;
        let trims = flags & 0x20 != 0;
        if trims && !last_trim {
            r.add(0, 0, Command::Trim, FORMAT)?;
        }
        last_trim = trims;
        r.add(dx, dy, if flags != 0 { Command::Jump } else { Command::Stitch }, FORMAT)?;
    }
    let mut d = r.finish();
    d.threads = threads;
    d.name = name;
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{sample, sewn};

    #[test]
    fn round_trips_the_sample() {
        let d = sample();
        let bytes = write(&d).unwrap();
        let back = read(&bytes).unwrap();
        assert_eq!(sewn(&back), sewn(&d));
        assert_eq!((back.color_changes(), back.trims()), (1, 1));
        assert_eq!(back.name, "Sample");
        assert_eq!((back.threads[0].name.as_str(), back.threads.len()), ("Red", 2));
    }

    #[test]
    fn layout_matches_the_format() {
        let bytes = write(&sample()).unwrap();
        assert_eq!(&bytes[..8], b"#PES0001");
        let pec = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        assert_eq!(&bytes[pec..pec + 3], b"LA:");
        assert_eq!(&bytes[pec + 34..pec + 36], &[6, 38]);
        assert_eq!(bytes[pec + 48], 1, "two colours");
        assert_eq!(&bytes[pec + 517..pec + 520], &[0x31, 0xFF, 0xF0]);
        // The stitch block length points at the thumbnails: three of 228 bytes end the file.
        let len = u32::from_le_bytes([bytes[pec + 514], bytes[pec + 515], bytes[pec + 516], 0]) as usize;
        assert_eq!(bytes.len(), pec + 512 + len + 3 * 228);
        assert_eq!(&bytes[pec + 512 + len - 2..pec + 512 + len], &[0xFF, 0x00]);
        // The PES object comes first.
        assert_eq!(&bytes[22..31], b"\x07\x00CEmbOne");
    }

    #[test]
    fn short_and_long_moves() {
        let mut out = vec![];
        long(&mut out, -1, 0x10);
        long(&mut out, 2047, 0x20);
        assert_eq!(out, [0x9F, 0xFF, 0xA7, 0xFF]);
    }

    #[test]
    fn damaged_input_does_not_panic() {
        let good = write(&sample()).unwrap();
        for n in [0, 8, 12, 40, 600, good.len() / 2] {
            let _ = read(&good[..n]);
        }
        assert!(read(b"#PEC0001").is_err());
        let mut bad = good.clone();
        bad[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read(&bad).is_err());
    }
}
