//! Singer XXX.
//!
//! A 256-byte header, then moves relative to the previous position in 0.1 mm with y pointing
//! up, then the thread colours.
//!
//! Header (little-endian): 0x17 zero bytes, `u32` record count, 0x0C zero bytes, `u32` colour
//! count (at 0x27), `u16` 0, then `i16` width, height, end x, end y, -min x and max y (y up), 0x42
//! zero bytes, two `u16` 0, 0x73 zero bytes, `u16` 0x20, 8 zero bytes, and the `u32` offset of
//! the end marker. Stitch data starts at 0x100.
//!
//! | Bytes               | Meaning                                       |
//! |---------------------|-----------------------------------------------|
//! | `dx dy`             | stitch (signed bytes, under ±124)             |
//! | `7D x16 y16`        | long move without stitching (`i16` each)      |
//! | `7F 01 dx dy`       | jump                                          |
//! | `7F 03 dx dy`       | trim, then move                               |
//! | `7F 08 dx dy`       | colour change (`7F 0A`–`7F 17` also read so)  |
//! | `7F 7F 02 14`       | end                                           |
//!
//! After the end marker: two zero bytes, then each colour as `00 r g b`, padded with zeros to 21
//! colours, then `00 FF FF FF` and `00 01`.

use crate::{Bytes, Command, Design, FormatError, Reader, Thread, block_threads, steps, to_units};

const FORMAT: &str = "XXX";
const HEADER_LEN: usize = 0x100;
/// Largest move one two-byte record can make, per axis, in 0.1 mm (bytes 0x7D–0x7F are codes).
pub const MAX_STEP: i32 = 123;
/// Colours a reader takes from one file.
const MAX_COLORS: usize = 1000;

fn byte(v: i32) -> u8 {
    (v.clamp(-MAX_STEP, MAX_STEP) as i8) as u8
}

fn put16(out: &mut Vec<u8>, v: i32) {
    out.extend((v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16).to_le_bytes());
}

pub fn write(design: &Design) -> Result<Vec<u8>, FormatError> {
    let units = to_units(design, FORMAT)?;
    let mut body = Vec::with_capacity(units.len() * 2 + 16);
    let (mut x, mut y) = (0, 0);
    let (mut lo, mut hi) = ((0, 0), (0, 0));
    for (tx, ty, command) in units {
        let (dx, dy) = (tx - x, ty - y);
        match command {
            Command::Stitch => {
                for (sx, sy) in steps(dx, dy, MAX_STEP) {
                    body.extend([byte(sx), byte(-sy)]);
                }
            }
            Command::End => break,
            _ => {
                // Trims and colour changes happen where the needle is: jump there first.
                if dx != 0 || dy != 0 {
                    for (sx, sy) in steps(dx, dy, MAX_STEP) {
                        body.extend([0x7F, 0x01, byte(sx), byte(-sy)]);
                    }
                }
                match command {
                    Command::Trim => body.extend([0x7F, 0x03, 0, 0]),
                    Command::ColorChange => body.extend([0x7F, 0x08, 0, 0]),
                    _ => {}
                }
            }
        }
        (x, y) = (tx, ty);
        (lo, hi) = ((lo.0.min(x), lo.1.min(y)), (hi.0.max(x), hi.1.max(y)));
    }
    let records = body.len() / 2;
    let threads = block_threads(design);
    let end_at = HEADER_LEN + body.len();
    let end_at = u32::try_from(end_at).map_err(|_| FormatError::TooLarge { format: FORMAT, why: "more stitch data than XXX can address".into() })?;
    let mut out = vec![0u8; 0x17];
    out.extend(u32::try_from(records).unwrap_or(u32::MAX).to_le_bytes());
    out.extend([0u8; 0x0C]);
    out.extend(u32::try_from(threads.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend([0u8; 2]);
    // y up from here on.
    for v in [hi.0 - lo.0, hi.1 - lo.1, x, -y, -lo.0, hi.1] {
        put16(&mut out, v);
    }
    out.extend([0u8; 0x42 + 4 + 0x73]);
    out.extend(0x20u16.to_le_bytes());
    out.extend([0u8; 8]);
    out.extend(end_at.to_le_bytes());
    out.extend(body);
    out.extend([0x7F, 0x7F, 0x02, 0x14, 0, 0]);
    for t in &threads {
        out.extend([0, t.rgb[0], t.rgb[1], t.rgb[2]]);
    }
    for _ in threads.len()..21 {
        out.extend([0u8; 4]);
    }
    out.extend([0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x01]);
    Ok(out)
}

pub fn read(bytes: &[u8]) -> Result<Design, FormatError> {
    if bytes.len() < HEADER_LEN {
        return Err(FormatError::Invalid { format: FORMAT, why: "shorter than the 256-byte header".into() });
    }
    let mut b = Bytes::new(bytes, FORMAT);
    b.seek(0x27)?;
    let colors = usize::from(u16::from_le_bytes([b.u8()?, b.u8()?])).min(MAX_COLORS);
    b.seek(HEADER_LEN)?;
    let mut r = Reader::new();
    let s8 = |v: u8| i32::from(v as i8);
    let mut ended = false;
    while let Ok(b1) = b.u8() {
        if b1 == 0x7D || b1 == 0x7E {
            let Ok(&[x0, x1, y0, y1]) = b.take(4) else { break };
            let (dx, dy) = (i16::from_le_bytes([x0, x1]), i16::from_le_bytes([y0, y1]));
            r.add(i32::from(dx), -i32::from(dy), Command::Jump, FORMAT)?;
            continue;
        }
        let Ok(b2) = b.u8() else { break };
        if b1 != 0x7F {
            r.add(s8(b1), -s8(b2), Command::Stitch, FORMAT)?;
            continue;
        }
        let (Ok(b3), Ok(b4)) = (b.u8(), b.u8()) else { break };
        let (dx, dy) = (s8(b3), -s8(b4));
        match b2 {
            0x01 => r.add(dx, dy, Command::Jump, FORMAT)?,
            0x03 => {
                r.add(0, 0, Command::Trim, FORMAT)?;
                if dx != 0 || dy != 0 {
                    r.add(dx, dy, Command::Jump, FORMAT)?;
                }
            }
            0x08 | 0x0A..=0x17 => {
                if dx != 0 || dy != 0 {
                    r.add(dx, dy, Command::Jump, FORMAT)?;
                }
                r.add(0, 0, Command::ColorChange, FORMAT)?;
            }
            0x7F | 0x18 => {
                ended = true;
                break;
            }
            // Unknown codes carry no move we understand.
            _ => {}
        }
    }
    let mut design = r.finish();
    if ended {
        // Two bytes after the end marker, then one big-endian `00 r g b` per colour.
        let _ = b.take(2);
        for _ in 0..colors {
            let Ok(&[_, r, g, bl]) = b.take(4) else { break };
            design.threads.push(Thread { rgb: [r, g, bl], ..Default::default() });
        }
    }
    Ok(design)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MachineStitch;
    use crate::testutil::{sample, sewn};

    #[test]
    fn round_trips_the_sample_with_its_colours() {
        let d = sample();
        let bytes = write(&d).unwrap();
        let back = read(&bytes).unwrap();
        assert_eq!(sewn(&back), sewn(&d));
        assert_eq!((back.color_changes(), back.trims()), (1, 1));
        assert_eq!(back.threads.iter().map(|t| t.rgb).collect::<Vec<_>>(), vec![[200, 20, 30], [10, 40, 160]]);
        // The end marker's offset is in the header.
        let at = u32::from_le_bytes(bytes[0xFC..0x100].try_into().unwrap()) as usize;
        assert_eq!(&bytes[at..at + 4], &[0x7F, 0x7F, 0x02, 0x14]);
    }

    #[test]
    fn records_are_as_specified() {
        let d = Design {
            stitches: vec![
                MachineStitch::new(0.0, 0.0, Command::Stitch),
                MachineStitch::new(1.0, 0.5, Command::Stitch),
                MachineStitch::new(1.0, 0.5, Command::Trim),
                MachineStitch::new(3.0, 0.5, Command::Jump),
                MachineStitch::new(3.0, 0.5, Command::ColorChange),
                MachineStitch::new(3.0, -1.0, Command::Stitch),
                MachineStitch::new(3.0, -1.0, Command::End),
            ],
            ..Default::default()
        };
        let bytes = write(&d).unwrap();
        assert_eq!(&bytes[0x100..0x100 + 22], &[0, 0, 10, 0xFB, 0x7F, 3, 0, 0, 0x7F, 1, 20, 0, 0x7F, 8, 0, 0, 0, 15, 0x7F, 0x7F, 2, 0x14]);
        // Two colours, padded to 21, then the closing words.
        assert_eq!(bytes.len(), 0x100 + 18 + 4 + 2 + 21 * 4 + 6);
    }

    #[test]
    fn damaged_input_does_not_panic() {
        assert!(read(&[0u8; 10]).is_err());
        let mut v = vec![0u8; 0x100];
        v.extend([0x7D, 1, 2, 0x7F, 0x7F]);
        assert!(read(&v).is_ok());
        v[0x27] = 0xFF;
        v.extend([0x7F, 0x7F, 0x02, 0x14, 0, 0, 0, 1]);
        assert!(read(&v).is_ok());
    }
}
