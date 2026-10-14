//! Barudan U01 (also FDR and U00 machines).
//!
//! A 256-byte header, then one 3-byte record per needle movement relative to the previous
//! position, in 0.1 mm, ending with `F8 00 00`.
//!
//! Header (little-endian): 0x80 ASCII `0` bytes, `i16` min x, -max y, max x, -min y (y up),
//! `u32` 0, `u32` record count, `i16` end x and -end y, then zeros to 0x100.
//!
//! A record is `ctrl |dy| |dx|`. `ctrl` has bit 0x80 set, 0x40 when the move goes down (or not
//! at all), 0x20 when it goes left (or not at all), and a command in its low five bits:
//!
//! | Low bits      | Meaning                                                  |
//! |---------------|----------------------------------------------------------|
//! | `00`          | stitch                                                   |
//! | `01`          | jump                                                     |
//! | `02`–`05`     | speed change (fast, slow), then stitch or jump           |
//! | `06`, `07`    | trim, then move                                          |
//! | `08`          | stop                                                     |
//! | `09`–`17`     | needle 1–15                                              |
//! | `18`          | end                                                      |
//!
//! Colours are needles: the writer sets needle 1 at the start and the next needle at each colour
//! change (after 15 it starts again at 1). U01 holds no thread colours.

use crate::{Command, Design, FormatError, Reader, steps, to_units};

const FORMAT: &str = "U01";
const HEADER_LEN: usize = 0x100;
/// Largest move one record can make, per axis, in 0.1 mm.
pub const MAX_STEP: i32 = 127;
/// Needles on the machine.
const NEEDLES: usize = 15;

fn record(out: &mut Vec<u8>, dx: i32, dy: i32, low: u8) {
    let mut ctrl = 0x80 | low;
    if dy >= 0 {
        ctrl |= 0x40;
    }
    if dx <= 0 {
        ctrl |= 0x20;
    }
    let mag = |v: i32| v.unsigned_abs().min(MAX_STEP as u32) as u8;
    out.extend([ctrl, mag(dy), mag(dx)]);
}

fn put16(out: &mut Vec<u8>, v: i32) {
    out.extend((v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16).to_le_bytes());
}

pub fn write(design: &Design) -> Result<Vec<u8>, FormatError> {
    let units = to_units(design, FORMAT)?;
    let mut body = Vec::with_capacity(units.len() * 3 + 16);
    let (mut x, mut y) = (0, 0);
    let (mut lo, mut hi) = ((i32::MAX, i32::MAX), (i32::MIN, i32::MIN));
    let mut needle = 0;
    record(&mut body, 0, 0, 0x09);
    for (tx, ty, command) in units {
        let (dx, dy) = (tx - x, ty - y);
        match command {
            Command::Stitch => {
                for (sx, sy) in steps(dx, dy, MAX_STEP) {
                    record(&mut body, sx, sy, 0x00);
                }
            }
            Command::End => break,
            _ => {
                if dx != 0 || dy != 0 {
                    for (sx, sy) in steps(dx, dy, MAX_STEP) {
                        record(&mut body, sx, sy, 0x01);
                    }
                }
                match command {
                    Command::Trim => record(&mut body, 0, 0, 0x07),
                    Command::ColorChange => {
                        needle = (needle + 1) % NEEDLES;
                        record(&mut body, 0, 0, 0x09 + needle as u8);
                    }
                    _ => {}
                }
            }
        }
        (x, y) = (tx, ty);
        (lo, hi) = ((lo.0.min(x), lo.1.min(y)), (hi.0.max(x), hi.1.max(y)));
    }
    body.extend([0xF8, 0x00, 0x00]);
    if lo.0 > hi.0 {
        (lo, hi) = ((0, 0), (0, 0));
    }
    let mut out = vec![b'0'; 0x80];
    for v in [lo.0, -hi.1, hi.0, -lo.1] {
        put16(&mut out, v);
    }
    out.extend(0u32.to_le_bytes());
    out.extend(u32::try_from(body.len() / 3).unwrap_or(u32::MAX).to_le_bytes());
    put16(&mut out, x);
    put16(&mut out, -y);
    out.resize(HEADER_LEN, 0);
    out.extend(body);
    Ok(out)
}

/// Read a U01 file. Needle changes after the first stitch become colour changes.
pub fn read(bytes: &[u8]) -> Result<Design, FormatError> {
    if bytes.len() < HEADER_LEN {
        return Err(FormatError::Invalid { format: FORMAT, why: "shorter than the 256-byte header".into() });
    }
    let mut r = Reader::new();
    for rec in bytes.get(HEADER_LEN..).unwrap_or_default().chunks_exact(3) {
        let &[ctrl, by, bx] = rec else { break };
        let dy = if ctrl & 0x40 != 0 { i32::from(by) } else { -i32::from(by) };
        let dx = if ctrl & 0x20 != 0 { -i32::from(bx) } else { i32::from(bx) };
        let moved = dx != 0 || dy != 0;
        match ctrl & 0x1F {
            0x00 | 0x02 | 0x04 => r.add(dx, dy, Command::Stitch, FORMAT)?,
            0x01 | 0x03 | 0x05 => r.add(dx, dy, Command::Jump, FORMAT)?,
            0x06 | 0x07 => {
                r.add(0, 0, Command::Trim, FORMAT)?;
                if moved {
                    r.add(dx, dy, Command::Jump, FORMAT)?;
                }
            }
            0x08..=0x17 => {
                if moved {
                    r.add(dx, dy, Command::Jump, FORMAT)?;
                }
                r.needle(FORMAT)?;
            }
            // 0x18 ends the design; anything else is data we don't understand.
            _ => break,
        }
    }
    Ok(r.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MachineStitch;
    use crate::testutil::{sample, sewn};

    #[test]
    fn round_trips_the_sample() {
        let d = sample();
        let back = read(&write(&d).unwrap()).unwrap();
        assert_eq!(sewn(&back), sewn(&d));
        assert_eq!((back.color_changes(), back.trims()), (1, 1));
    }

    #[test]
    fn records_are_as_specified() {
        let d = Design {
            stitches: vec![
                MachineStitch::new(0.0, 0.0, Command::Stitch),
                MachineStitch::new(1.0, 0.5, Command::Stitch),
                MachineStitch::new(1.0, 0.5, Command::Trim),
                MachineStitch::new(1.0, 0.5, Command::ColorChange),
                MachineStitch::new(2.0, 0.5, Command::Stitch),
                MachineStitch::new(2.0, 0.5, Command::End),
            ],
            ..Default::default()
        };
        let bytes = write(&d).unwrap();
        // The bytes pyembroidery writes for the same design.
        assert_eq!(&bytes[0x100..], &[0xE9, 0, 0, 0xE0, 0, 0, 0xC0, 5, 10, 0xE7, 0, 0, 0xEA, 0, 0, 0xC0, 0, 10, 0xF8, 0, 0]);
        assert_eq!(&bytes[0x80..0x88], &[0, 0, 0xFB, 0xFF, 20, 0, 0, 0]);
    }

    #[test]
    fn damaged_input_does_not_panic() {
        assert!(read(&[0u8; 10]).is_err());
        let mut v = vec![0u8; 0x100];
        v.extend([0x8A, 1, 2, 0xFF]);
        assert!(read(&v).is_ok());
    }
}
