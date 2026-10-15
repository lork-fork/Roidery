//! Janome JEF.
//!
//! A little-endian header, then moves relative to the previous position in 0.1 mm with y
//! pointing up. All header fields are 32-bit integers except the date:
//!
//! | Offset | Field                                                                  |
//! |--------|------------------------------------------------------------------------|
//! | 0      | offset of the stitch data: `0x74 + 8 × colours`                        |
//! | 4      | flags, `0x14`                                                          |
//! | 8      | date and time as 14 ASCII digits (`YYYYMMDDHHMMSS`), then two zeros    |
//! | 24     | colour count                                                           |
//! | 28     | record count: stitches count 1, commands 2, plus 1 for the end         |
//! | 32     | hoop: 0 = 110 × 110, 1 = 50 × 50, 2 = 140 × 200, 3 = 126 × 110, 4 = 200 × 200 mm |
//! | 36     | the design's half width, half height, half width, half height          |
//! | 52     | margins in the 110 × 110 hoop (left, top, right, bottom; −1 if too big)|
//! | 68     | margins in the 50 × 50 hoop                                            |
//! | 84     | margins in the 140 × 200 hoop                                          |
//! | 100    | margins in the 126 × 110 hoop                                          |
//! | 116    | one Janome thread number per colour, then `0x0D` per colour            |
//!
//! Records: `dx dy` stitches (±127), `80 02 dx dy` a jump (several in place make the machine
//! trim), `80 01 dx dy` a colour change, `80 10` the end.

use crate::{Bytes, Command, Design, FormatError, Reader, Thread, block_threads, nearest, steps, to_units};

const FORMAT: &str = "JEF";
pub const MAX_STEP: i32 = 127;
const HEADER: usize = 0x74;
/// The date written in the header. Machines show it but don't depend on it; a fixed value keeps
/// the output identical for identical designs (and needs no clock, which the web build lacks).
const DATE: &[u8; 14] = b"20260101000000";

/// Janome's thread chart, by thread number (0 is unused).
pub const PALETTE: [([u8; 3], &str); 79] = [
    ([0, 0, 0], "Placeholder"),
    ([0, 0, 0], "Black"),
    ([255, 255, 255], "White"),
    ([255, 255, 23], "Yellow"),
    ([255, 102, 0], "Orange"),
    ([47, 89, 51], "Olive Green"),
    ([35, 115, 54], "Green"),
    ([101, 194, 200], "Sky"),
    ([171, 90, 150], "Purple"),
    ([246, 105, 160], "Pink"),
    ([255, 0, 0], "Red"),
    ([177, 112, 78], "Brown"),
    ([11, 47, 132], "Blue"),
    ([228, 195, 93], "Gold"),
    ([72, 26, 5], "Dark Brown"),
    ([172, 156, 199], "Pale Violet"),
    ([252, 242, 148], "Pale Yellow"),
    ([249, 153, 183], "Pale Pink"),
    ([250, 179, 129], "Peach"),
    ([201, 164, 128], "Beige"),
    ([151, 5, 51], "Wine Red"),
    ([160, 184, 204], "Pale Sky"),
    ([127, 194, 28], "Yellow Green"),
    ([229, 229, 229], "Silver Gray"),
    ([136, 155, 155], "Gray"),
    ([152, 214, 189], "Pale Aqua"),
    ([178, 225, 227], "Baby Blue"),
    ([54, 139, 160], "Powder Blue"),
    ([79, 131, 171], "Bright Blue"),
    ([56, 106, 145], "Slate Blue"),
    ([7, 22, 80], "Navy Blue"),
    ([249, 153, 162], "Salmon Pink"),
    ([249, 103, 107], "Coral"),
    ([227, 49, 31], "Burnt Orange"),
    ([226, 161, 136], "Cinnamon"),
    ([181, 148, 116], "Umber"),
    ([228, 207, 153], "Blond"),
    ([255, 203, 0], "Sunflower"),
    ([225, 173, 212], "Orchid Pink"),
    ([195, 0, 126], "Peony Purple"),
    ([128, 0, 75], "Burgundy"),
    ([84, 5, 113], "Royal Purple"),
    ([177, 5, 37], "Cardinal Red"),
    ([202, 224, 192], "Opal Green"),
    ([137, 152, 86], "Moss Green"),
    ([92, 148, 26], "Meadow Green"),
    ([0, 49, 20], "Dark Green"),
    ([93, 174, 148], "Aquamarine"),
    ([76, 191, 143], "Emerald Green"),
    ([0, 119, 114], "Peacock Green"),
    ([89, 91, 97], "Dark Gray"),
    ([255, 255, 242], "Ivory White"),
    ([177, 88, 24], "Hazel"),
    ([203, 138, 7], "Toast"),
    ([152, 108, 128], "Salmon"),
    ([152, 105, 45], "Cocoa Brown"),
    ([77, 52, 25], "Sienna"),
    ([76, 51, 11], "Sepia"),
    ([51, 32, 10], "Dark Sepia"),
    ([82, 58, 151], "Violet Blue"),
    ([13, 33, 126], "Blue Ink"),
    ([30, 119, 172], "Sola Blue"),
    ([178, 221, 83], "Green Dust"),
    ([243, 54, 137], "Crimson"),
    ([222, 100, 158], "Floral Pink"),
    ([152, 65, 97], "Wine"),
    ([76, 86, 18], "Olive Drab"),
    ([76, 136, 31], "Meadow"),
    ([228, 222, 121], "Mustard"),
    ([203, 138, 26], "Yellow Ocher"),
    ([203, 162, 28], "Old Gold"),
    ([255, 152, 5], "Honey Dew"),
    ([252, 178, 87], "Tangerine"),
    ([255, 229, 5], "Canary Yellow"),
    ([240, 51, 31], "Vermilion"),
    ([26, 132, 45], "Bright Green"),
    ([56, 108, 174], "Ocean Blue"),
    ([227, 196, 180], "Beige Gray"),
    ([227, 172, 129], "Bamboo"),
];

fn byte(v: i32) -> u8 {
    (v.clamp(-MAX_STEP, MAX_STEP) as i8) as u8
}

/// The smallest of Janome's hoops the design fits, by its size in 0.1 mm.
fn hoop(w: i32, h: i32) -> i32 {
    if w <= 500 && h <= 500 {
        1
    } else if w <= 1100 && h <= 1100 {
        0
    } else if w <= 1260 && h <= 1100 {
        3
    } else if w <= 1400 && h <= 2000 {
        2
    } else {
        4
    }
}

pub fn write(design: &Design) -> Result<Vec<u8>, FormatError> {
    let units = to_units(design, FORMAT)?;
    let threads = block_threads(design);
    // Records, and the header's record count.
    let mut body = Vec::with_capacity(units.len() * 2 + 16);
    let mut count: i32 = 1;
    let (mut x, mut y) = (0, 0);
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for &(tx, ty, command) in &units {
        if command == Command::End {
            break;
        }
        min_x = min_x.min(tx);
        min_y = min_y.min(ty);
        max_x = max_x.max(tx);
        max_y = max_y.max(ty);
        let (dx, dy) = (tx - x, -(ty - y));
        match command {
            Command::Stitch => {
                for (sx, sy) in steps(dx, dy, MAX_STEP) {
                    body.extend([byte(sx), byte(sy)]);
                    count += 1;
                }
            }
            Command::Jump => {
                if dx != 0 || dy != 0 {
                    for (sx, sy) in steps(dx, dy, MAX_STEP) {
                        body.extend([0x80, 0x02, byte(sx), byte(sy)]);
                        count += 2;
                    }
                }
            }
            Command::Trim | Command::ColorChange => {
                if dx != 0 || dy != 0 {
                    for (sx, sy) in steps(dx, dy, MAX_STEP) {
                        body.extend([0x80, 0x02, byte(sx), byte(sy)]);
                        count += 2;
                    }
                }
                if command == Command::Trim {
                    // Janome machines cut on jumps in place.
                    for _ in 0..3 {
                        body.extend([0x80, 0x02, 0, 0]);
                        count += 2;
                    }
                } else {
                    body.extend([0x80, 0x01, 0, 0]);
                    count += 2;
                }
            }
            Command::End => {}
        }
        (x, y) = (tx, ty);
    }
    body.extend([0x80, 0x10]);
    let (w, h) = if min_x <= max_x { (max_x - min_x, max_y - min_y) } else { (0, 0) };
    // The header's extents are symmetric about the origin (where the needle starts): half the
    // size for a design centred on it, else far enough each way to hold a design that keeps its
    // place in the hoop.
    let half = |lo: i32, hi: i32, size: i32| if (lo + hi).abs() <= 2 { size / 2 } else { lo.saturating_abs().max(hi.saturating_abs()) };
    let (hw, hh) = if min_x <= max_x { (half(min_x, max_x, w), half(min_y, max_y, h)) } else { (0, 0) };
    let colors = threads.len();
    let colors_i32 = i32::try_from(colors).map_err(|_| FormatError::TooLarge { format: FORMAT, why: "too many colours".into() })?;
    let mut out = Vec::with_capacity(HEADER + colors * 8 + body.len());
    let i32le = |v: i32, out: &mut Vec<u8>| out.extend(v.to_le_bytes());
    i32le(i32::try_from(HEADER + colors * 8).unwrap_or(i32::MAX), &mut out);
    i32le(0x14, &mut out);
    out.extend(DATE);
    out.extend([0, 0]);
    i32le(colors_i32, &mut out);
    i32le(count, &mut out);
    i32le(hoop(w.max(2 * hw), h.max(2 * hh)), &mut out);
    for v in [hw, hh, hw, hh] {
        i32le(v, &mut out);
    }
    // Margins left in each hoop (half its size less half the design's), -1 where it doesn't fit.
    for (half_w, half_h) in [(550, 550), (250, 250), (700, 1000), (630, 550)] {
        let fits = half_w >= hw && half_h >= hh;
        for v in [half_w - hw, half_h - hh, half_w - hw, half_h - hh] {
            i32le(if fits { v } else { -1 }, &mut out);
        }
    }
    let palette: Vec<[u8; 3]> = PALETTE.iter().map(|p| p.0).collect();
    for t in &threads {
        i32le(i32::try_from(nearest(t.rgb, &palette, 1)).unwrap_or(1), &mut out);
    }
    for _ in &threads {
        i32le(0x0D, &mut out);
    }
    out.extend(body);
    Ok(out)
}

pub fn read(bytes: &[u8]) -> Result<Design, FormatError> {
    let mut b = Bytes::new(bytes, FORMAT);
    let offset = usize::try_from(b.i32()?).map_err(|_| b.short())?;
    b.seek(24)?;
    let colors = b.i32()?.clamp(0, 1000);
    b.seek(HEADER)?;
    let mut threads = vec![];
    for _ in 0..colors {
        let n = usize::try_from(b.i32()?).unwrap_or(0);
        let (rgb, name) = PALETTE.get(n).copied().unwrap_or(([0, 0, 0], ""));
        threads.push(Thread { rgb, name: name.into(), code: n.to_string() });
    }
    b.seek(offset)?;
    let mut r = Reader::new();
    let signed = |v: u8| i32::from(v as i8);
    // Jumps in place, counted to tell a trim from a move.
    let mut pending_trim = 0;
    while let (Ok(a), Ok(c)) = (b.u8(), b.u8()) {
        if a == 0x80 {
            if c == 0x10 {
                break;
            }
            let (Ok(dx), Ok(dy)) = (b.u8(), b.u8()) else { break };
            let (dx, dy) = (signed(dx), -signed(dy));
            if c & 0x01 != 0 {
                r.add(dx, dy, Command::ColorChange, FORMAT)?;
            } else if dx == 0 && dy == 0 {
                pending_trim += 1;
                if pending_trim == 3 {
                    r.add(0, 0, Command::Trim, FORMAT)?;
                }
            } else {
                r.add(dx, dy, Command::Jump, FORMAT)?;
            }
        } else {
            pending_trim = 0;
            r.add(signed(a), -signed(c), Command::Stitch, FORMAT)?;
        }
    }
    let mut d = r.finish();
    d.threads = threads;
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
        // Red and blue map to the nearest of Janome's reds and blues.
        assert_eq!(back.threads.len(), 2);
        assert!(back.threads[0].name.contains("Red"), "{:?}", back.threads[0]);
        assert!(back.threads[1].name.contains("Blue"), "{:?}", back.threads[1]);
    }

    #[test]
    fn header_fields() {
        let bytes = write(&sample()).unwrap();
        let i32_at = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        assert_eq!(i32_at(0), 0x74 + 16);
        assert_eq!(i32_at(4), 0x14);
        assert_eq!(&bytes[8..22], DATE);
        assert_eq!(i32_at(24), 2);
        // The sample is 30 × 40 mm with its corner at the origin: the needle travels 30 mm one
        // way and 40 mm the other, so it needs the 110 × 110 hoop and its extents say so.
        assert_eq!(i32_at(32), 0);
        assert_eq!((i32_at(36), i32_at(40)), (300, 400));
        assert_eq!(i32_at(0x74 + 8), 0x0D);
        assert_eq!(&bytes[bytes.len() - 2..], &[0x80, 0x10]);
        // Centred on the origin, it fits the 50 × 50 hoop.
        let mut centred = sample();
        for s in &mut centred.stitches {
            (s.x, s.y) = (s.x - 15.0, s.y - 20.0);
        }
        let bytes = write(&centred).unwrap();
        let i32_at = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        assert_eq!(i32_at(32), 1);
        assert_eq!((i32_at(36), i32_at(40)), (150, 200));
    }

    #[test]
    fn damaged_input_does_not_panic() {
        let good = write(&sample()).unwrap();
        for n in [0, 3, 30, 0x74, 0x80, good.len() - 1] {
            let _ = read(&good[..n]);
        }
        let mut bad = good.clone();
        bad[0..4].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(read(&bad).is_err());
    }
}
