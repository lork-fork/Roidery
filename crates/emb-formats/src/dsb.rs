//! Barudan DSB (read only).
//!
//! A 512-byte header laid out like DST's, then one 3-byte record per needle movement relative
//! to the previous position, in 0.1 mm: `ctrl |dy| |dx|`. `ctrl` has 0x40 set when the move
//! goes down and 0x20 when it goes left; its low five bits are 0 for a stitch and 1 for a jump.
//! Whole control bytes: `F8` end, `E7` trim, `E8` stop, `E9`–`F7` needle change.

use crate::{Command, Design, FormatError, Reader, dst};

const FORMAT: &str = "DSB";

pub fn read(bytes: &[u8]) -> Result<Design, FormatError> {
    let head = bytes.get(..dst::HEADER_LEN).ok_or_else(|| FormatError::Invalid { format: FORMAT, why: "shorter than the 512-byte header".into() })?;
    let mut r = Reader::new();
    for rec in bytes.get(dst::HEADER_LEN..).unwrap_or_default().chunks_exact(3) {
        let &[ctrl, by, bx] = rec else { break };
        let dy = if ctrl & 0x40 != 0 { i32::from(by) } else { -i32::from(by) };
        let dx = if ctrl & 0x20 != 0 { -i32::from(bx) } else { i32::from(bx) };
        match (ctrl, ctrl & 0x1F) {
            (_, 0) => r.add(dx, dy, Command::Stitch, FORMAT)?,
            (_, 1) => r.add(dx, dy, Command::Jump, FORMAT)?,
            (0xE7, _) => r.add(0, 0, Command::Trim, FORMAT)?,
            (0xE8..=0xF7, _) => r.needle(FORMAT)?,
            // 0xF8 ends the design; anything else is data we don't understand.
            _ => break,
        }
    }
    let mut design = r.finish();
    design.name = dst::header_name(head);
    Ok(design)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_moves_trims_and_needles() {
        let mut v = b"LA:Badge".to_vec();
        v.resize(512, b' ');
        // Needle 1, stitch 1 mm right and 0.5 mm down, trim, needle 2, stitch 2 mm left, end.
        v.extend([0xE9, 0, 0, 0x40, 5, 10, 0xE7, 0, 0, 0xEA, 0, 0, 0x20, 0, 20, 0xF8, 0, 0]);
        let d = read(&v).unwrap();
        let got: Vec<(f64, f64, Command)> = d.stitches.iter().map(|s| (s.x, s.y, s.command)).collect();
        assert_eq!(
            got,
            vec![
                (1.0, 0.5, Command::Stitch),
                (1.0, 0.5, Command::Trim),
                (1.0, 0.5, Command::ColorChange),
                (-1.0, 0.5, Command::Stitch),
                (-1.0, 0.5, Command::End)
            ]
        );
        assert_eq!(d.name, "Badge");
        assert!(read(&v[..100]).is_err());
    }
}
