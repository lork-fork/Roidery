//! Melco EXP.
//!
//! No header: the file is a list of moves relative to the previous position, in 0.1 mm with y
//! pointing up. A stitch is two signed bytes, `dx dy` (±127; `0x80` is never a move). A command
//! is `0x80` and a code, then a two-byte move:
//!
//! | Bytes         | Meaning                                  |
//! |---------------|------------------------------------------|
//! | `dx dy`       | stitch                                   |
//! | `80 04 dx dy` | jump                                     |
//! | `80 01 00 00` | colour change (stop)                     |
//! | `80 80 07 00` | trim                                     |
//!
//! EXP has no colours or end marker; the file just ends.

use crate::{Command, Design, FormatError, Reader, steps, to_units};

const FORMAT: &str = "EXP";
/// Largest move one record can make, per axis, in 0.1 mm.
pub const MAX_STEP: i32 = 127;

fn byte(v: i32) -> u8 {
    // Moves are split to ±127 first, so this is a plain two's complement byte.
    (v.clamp(-MAX_STEP, MAX_STEP) as i8) as u8
}

pub fn write(design: &Design) -> Result<Vec<u8>, FormatError> {
    let mut out = Vec::with_capacity(design.stitches.len() * 2 + 16);
    let (mut x, mut y) = (0, 0);
    for (tx, ty, command) in to_units(design, FORMAT)? {
        // EXP's y points up.
        let (dx, dy) = (tx - x, -(ty - y));
        match command {
            Command::Stitch => {
                for (sx, sy) in steps(dx, dy, MAX_STEP) {
                    out.extend([byte(sx), byte(sy)]);
                }
            }
            Command::End => break,
            _ => {
                // A trim or colour change happens where the needle is: reach its position
                // with jumps first.
                if dx != 0 || dy != 0 {
                    for (sx, sy) in steps(dx, dy, MAX_STEP) {
                        out.extend([0x80, 0x04, byte(sx), byte(sy)]);
                    }
                }
                match command {
                    Command::Trim => out.extend([0x80, 0x80, 0x07, 0x00]),
                    Command::ColorChange => out.extend([0x80, 0x01, 0x00, 0x00]),
                    _ => {}
                }
            }
        }
        (x, y) = (tx, ty);
    }
    Ok(out)
}

pub fn read(bytes: &[u8]) -> Result<Design, FormatError> {
    let mut r = Reader::new();
    let signed = |b: u8| i32::from(b as i8);
    let mut i = 0;
    while let (Some(&a), Some(&b)) = (bytes.get(i), bytes.get(i + 1)) {
        i += 2;
        if a == 0x80 {
            let (Some(&c), Some(&d)) = (bytes.get(i), bytes.get(i + 1)) else { break };
            i += 2;
            match b {
                0x01 => r.add(0, 0, Command::ColorChange, FORMAT)?,
                0x80 => r.add(0, 0, Command::Trim, FORMAT)?,
                // 0x04 and any other code: a move without stitching.
                _ => r.add(signed(c), -signed(d), Command::Jump, FORMAT)?,
            }
        } else {
            r.add(signed(a), -signed(b), Command::Stitch, FORMAT)?;
        }
    }
    Ok(r.finish())
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
        // The 40 mm jump needs four records of at most 12.7 mm.
        assert_eq!(bytes.chunks(2).filter(|w| w == &[0x80, 0x04]).count(), 4);
    }

    #[test]
    fn records_are_as_specified() {
        use crate::MachineStitch;
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
        // y up: 0.5 mm down is -5.
        assert_eq!(write(&d).unwrap(), [0, 0, 10, 0xFB, 0x80, 0x80, 7, 0, 0x80, 4, 20, 0, 0x80, 1, 0, 0, 0, 15]);
    }

    #[test]
    fn damaged_input_does_not_panic() {
        for bytes in [&[][..], &[0x80], &[0x80, 0x01], &[0x80, 0x04, 0x7F], &[1, 2, 3]] {
            assert!(read(bytes).is_ok());
        }
    }
}
