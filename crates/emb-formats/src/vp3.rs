//! Husqvarna Viking / Pfaff VP3.
//!
//! Big-endian, built from nested blocks, each a three-byte tag and a `u32` length of what follows
//! it. Positions in the headers are micrometres with y pointing up; stitch moves are 0.1 mm with
//! y pointing down. Strings are a `u16` byte length and the text (UTF-16BE in the headers, UTF-8
//! in thread blocks).
//!
//! ```text
//! "%vsm%" 00, "Produced by     Software Ltd" (UTF-16)
//! 00 02 00 len                        file block
//!   "" (settings), right, -top, left, -bottom, record count, 00 colours 0C 00, 01 (designs)
//!   00 03 00 len                      design block
//!     centre x, -centre y, 00 00 00, -w/2, w/2, -h/2, h/2, w, h, "" (notes)
//!     64 64, 4096, 0, 0, 4096, "xxPP" 01 00, "Produced by     Software Ltd", u16 colours
//!     00 05 00 len                    one per colour
//!       start x, start y (from the centre), thread, block shift x, y
//!       00 01 00 len 0A F6 00 records, then 00
//! ```
//!
//! Records: `dx dy` (±127) is a stitch, `80 01 dx dy 80 02` (`i16`s) a longer one, `80 03` a
//! trim. VP3 has no jumps: a move is the next stitch, and the machine travels without sewing
//! when it is long. Colour changes start a new colour block, and the design ends with a trim.
//! The layout follows pyembroidery's VP3 reader and writer (MIT), which is also the test oracle.

use crate::{Command, Design, FormatError, MachineStitch, Thread, block_threads, to_units};

const FORMAT: &str = "VP3";
/// The producer string every VP3 file carries (the spaces are part of it).
const PRODUCER: &str = "Produced by     Software Ltd";
/// Largest move a short record makes, per axis, in 0.1 mm.
pub const MAX_STEP: i32 = 127;

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_be_bytes());
}

fn put_i32(out: &mut Vec<u8>, v: i32) {
    out.extend(v.to_be_bytes());
}

fn put_utf16(out: &mut Vec<u8>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().take(u16::MAX as usize / 2).collect();
    put_u16(out, (units.len() * 2) as u16);
    for u in units {
        put_u16(out, u);
    }
}

fn put_utf8(out: &mut Vec<u8>, s: &str) {
    // Cut at a character boundary so the length fits a u16.
    let mut end = s.len().min(u16::MAX as usize);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let b = s.as_bytes().get(..end).unwrap_or_default();
    put_u16(out, b.len() as u16);
    out.extend(b);
}

/// Start a block with `tag`; returns where its length goes, for [`close`].
fn open(out: &mut Vec<u8>, tag: [u8; 3]) -> usize {
    out.extend(tag);
    out.extend([0; 4]);
    out.len() - 4
}

/// Write the length of the block whose length field is at `at`: the bytes after that field.
fn close(out: &mut [u8], at: usize) -> Result<(), FormatError> {
    let len = u32::try_from(out.len() - at - 4).map_err(|_| FormatError::TooLarge { format: FORMAT, why: "a block over 4 GB".into() })?;
    if let Some(field) = out.get_mut(at..at + 4) {
        field.copy_from_slice(&len.to_be_bytes());
    }
    Ok(())
}

/// 0.1 mm → micrometres.
fn um(v: i32) -> i32 {
    v.saturating_mul(100)
}

fn i16_move(v: i32) -> Result<[u8; 2], FormatError> {
    i16::try_from(v)
        .map(i16::to_be_bytes)
        .map_err(|_| FormatError::TooLarge { format: FORMAT, why: format!("a move of {:.1} mm in one stitch", f64::from(v) / 10.0) })
}

pub fn write(design: &Design) -> Result<Vec<u8>, FormatError> {
    let mut units = to_units(design, FORMAT)?;
    if let Some(end) = units.iter().position(|u| u.2 == Command::End) {
        units.truncate(end);
    }
    let threads = block_threads(design);
    let colors =
        u8::try_from(threads.len()).map_err(|_| FormatError::TooLarge { format: FORMAT, why: format!("{} colours (255 at most)", threads.len()) })?;
    let (mut left, mut top, mut right, mut bottom) = (0, 0, 0, 0);
    if let Some(&(x, y, _)) = units.first() {
        (left, top, right, bottom) = (x, y, x, y);
    }
    for &(x, y, _) in &units {
        (left, top, right, bottom) = (left.min(x), top.min(y), right.max(x), bottom.max(y));
    }
    let (w, h) = (right - left, bottom - top);
    // The centre, in micrometres so a half unit isn't lost.
    let (cx, cy) = (um(left) + um(w) / 2, um(top) + um(h) / 2);

    let mut out = b"%vsm%\0".to_vec();
    put_utf16(&mut out, PRODUCER);
    let file = open(&mut out, [0, 2, 0]);
    put_utf16(&mut out, "");
    for v in [um(right), -um(top), um(left), -um(bottom)] {
        put_i32(&mut out, v);
    }
    put_i32(&mut out, i32::try_from(units.len()).unwrap_or(i32::MAX));
    out.extend([0, colors, 12, 0, 1]);

    let design_block = open(&mut out, [0, 3, 0]);
    put_i32(&mut out, cx);
    put_i32(&mut out, -cy);
    out.extend([0, 0, 0]);
    for v in [-um(w) / 2, um(w) / 2, -um(h) / 2, um(h) / 2, um(w), um(h)] {
        put_i32(&mut out, v);
    }
    put_utf16(&mut out, "");
    out.extend([0x64, 0x64]);
    for v in [4096, 0, 0, 4096] {
        put_i32(&mut out, v);
    }
    out.extend(b"xxPP\x01\x00");
    put_utf16(&mut out, PRODUCER);
    put_u16(&mut out, u16::from(colors));

    // Colour blocks: the first starts at the origin, each later one where its colour change is.
    let mut blocks: Vec<&[(i32, i32, Command)]> = Vec::with_capacity(threads.len());
    let mut from = 0;
    for (i, u) in units.iter().enumerate() {
        if u.2 == Command::ColorChange {
            blocks.push(units.get(from..i).unwrap_or_default());
            from = i;
        }
    }
    blocks.push(units.get(from..).unwrap_or_default());
    for (i, (stitches, thread)) in blocks.iter().zip(&threads).enumerate() {
        let (sx, sy) = match stitches.first() {
            Some(&(x, y, _)) if i > 0 => (x, y),
            _ => (0, 0),
        };
        let (ex, ey) = stitches.last().map_or((0, 0), |&(x, y, _)| (x, y));
        let block = open(&mut out, [0, 5, 0]);
        put_i32(&mut out, um(sx) - cx);
        put_i32(&mut out, -(um(sy) - cy));
        write_thread(&mut out, thread);
        put_i32(&mut out, um(ex - sx));
        put_i32(&mut out, -um(ey - sy));
        let records = open(&mut out, [0, 1, 0]);
        out.extend([0x0A, 0xF6, 0x00]);
        let (mut x, mut y) = (sx, sy);
        for &(tx, ty, command) in stitches.iter() {
            match command {
                Command::Stitch => {
                    let (dx, dy) = (tx - x, ty - y);
                    if dx.abs() <= MAX_STEP && dy.abs() <= MAX_STEP {
                        out.extend([dx as i8 as u8, dy as i8 as u8]);
                    } else {
                        out.extend([0x80, 0x01]);
                        out.extend(i16_move(dx)?);
                        out.extend(i16_move(dy)?);
                        out.extend([0x80, 0x02]);
                    }
                    (x, y) = (tx, ty);
                }
                Command::Trim => out.extend([0x80, 0x03]),
                // No jumps: the next stitch makes the move. Colour changes start blocks.
                Command::Jump | Command::ColorChange | Command::End => {}
            }
        }
        if i + 1 == blocks.len() {
            // The machine doesn't cut at the end by itself.
            out.extend([0x80, 0x03]);
        }
        close(&mut out, records)?;
        out.push(0);
        close(&mut out, block)?;
    }
    close(&mut out, design_block)?;
    close(&mut out, file)?;
    Ok(out)
}

fn write_thread(out: &mut Vec<u8>, t: &Thread) {
    // One colour, no transition; then no parts, no length, rayon, 40 weight.
    out.extend([1, 0, t.rgb[0], t.rgb[1], t.rgb[2], 0, 0, 0, 5, 0x28]);
    put_utf8(out, &t.code);
    let hex = format!("#{:02x}{:02x}{:02x}", t.rgb[0], t.rgb[1], t.rgb[2]);
    put_utf8(out, if t.name.is_empty() { &hex } else { &t.name });
    put_utf8(out, "");
}

/// Big-endian reads over [`crate::Bytes`].
struct Be<'a>(crate::Bytes<'a>);

impl<'a> Be<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        self.0.take(n)
    }
    fn u8(&mut self) -> Result<u8, FormatError> {
        self.0.u8()
    }
    fn u16(&mut self) -> Result<u16, FormatError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b.first().copied().unwrap_or(0), b.get(1).copied().unwrap_or(0)]))
    }
    fn i32(&mut self) -> Result<i32, FormatError> {
        let mut a = [0u8; 4];
        a.copy_from_slice(self.take(4)?);
        Ok(i32::from_be_bytes(a))
    }
    fn skip(&mut self, n: usize) -> Result<(), FormatError> {
        self.take(n).map(|_| ())
    }
    fn string(&mut self) -> Result<&'a [u8], FormatError> {
        let n = self.u16()?;
        self.take(usize::from(n))
    }
    fn utf8(&mut self) -> Result<String, FormatError> {
        Ok(String::from_utf8_lossy(self.string()?).into_owned())
    }
    /// A block's tag and length → where the block ends.
    fn block(&mut self) -> Result<usize, FormatError> {
        self.skip(3)?;
        let len = self.i32()?;
        let len = usize::try_from(len).map_err(|_| FormatError::Invalid { format: FORMAT, why: "a negative block length".into() })?;
        Ok(self.0.pos.saturating_add(len).min(self.0.data.len()))
    }
}

/// Micrometres → 0.1 mm, rounded.
fn tenth(v: i32) -> i32 {
    (f64::from(v) / 100.0).round() as i32
}

pub fn read(bytes: &[u8]) -> Result<Design, FormatError> {
    if !bytes.starts_with(b"%vsm%") {
        return Err(FormatError::Invalid { format: FORMAT, why: "no %vsm% signature".into() });
    }
    let mut b = Be(crate::Bytes::new(bytes, FORMAT));
    b.skip(6)?;
    b.string()?;
    b.block()?;
    b.string()?;
    // Bounds, record count, colours, designs.
    b.skip(16 + 4 + 4 + 1)?;
    b.block()?;
    let (cx, cy) = (b.i32()?, -b.i32()?);
    b.skip(3 + 16 + 8)?;
    b.string()?;
    b.skip(2 + 16 + 6)?;
    b.string()?;
    let colors = b.u16()?;
    let mut stitches = vec![];
    let mut threads = vec![];
    // Position in 0.1 mm, y down.
    let (mut x, mut y) = (0, 0);
    for i in 0..colors {
        let end = b.block()?;
        let (sx, sy) = (b.i32()?.saturating_add(cx), (-b.i32()?).saturating_add(cy));
        let (sx, sy) = (tenth(sx), tenth(sy));
        if i > 0 {
            stitches.push(MachineStitch::new(f64::from(x) / 10.0, f64::from(y) / 10.0, Command::ColorChange));
        }
        if (sx, sy) != (x, y) {
            (x, y) = (sx, sy);
            stitches.push(MachineStitch::new(f64::from(x) / 10.0, f64::from(y) / 10.0, Command::Jump));
        }
        threads.push(read_thread(&mut b)?);
        b.skip(8)?;
        b.block()?;
        b.skip(3)?;
        let records = b.take(end.saturating_sub(b.0.pos))?;
        let mut it = records.iter().map(|&v| v as i8);
        while let (Some(a), Some(c)) = (it.next(), it.next()) {
            if stitches.len() >= crate::MAX_READ {
                return Err(FormatError::TooLarge { format: FORMAT, why: "more than 5 million stitches".into() });
            }
            if a as u8 != 0x80 {
                (x, y) = (x.saturating_add(a.into()), y.saturating_add(c.into()));
                stitches.push(MachineStitch::new(f64::from(x) / 10.0, f64::from(y) / 10.0, Command::Stitch));
                continue;
            }
            match c {
                0x01 => {
                    let mut word = || Some(i16::from_be_bytes([it.next()? as u8, it.next()? as u8]));
                    let (Some(dx), Some(dy)) = (word(), word()) else { break };
                    // The closing 80 02.
                    it.next();
                    it.next();
                    (x, y) = (x.saturating_add(dx.into()), y.saturating_add(dy.into()));
                    stitches.push(MachineStitch::new(f64::from(x) / 10.0, f64::from(y) / 10.0, Command::Stitch));
                }
                0x03 => stitches.push(MachineStitch::new(f64::from(x) / 10.0, f64::from(y) / 10.0, Command::Trim)),
                _ => {}
            }
        }
        b.0.seek(end)?;
    }
    stitches.push(MachineStitch::new(f64::from(x) / 10.0, f64::from(y) / 10.0, Command::End));
    Ok(Design { stitches, threads, name: String::new() })
}

fn read_thread(b: &mut Be) -> Result<Thread, FormatError> {
    let colors = b.u8()?;
    b.skip(1)?;
    let mut rgb = [0; 3];
    for _ in 0..colors {
        let c = b.take(3)?;
        rgb.copy_from_slice(c);
        b.skip(3)?;
    }
    b.skip(2)?;
    let code = b.utf8()?;
    let name = b.utf8()?;
    b.string()?;
    Ok(Thread { rgb, name, code })
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
        assert_eq!(back.color_changes(), 1);
        // The design's trim, and the one VP3 ends with.
        assert_eq!(back.trims(), 2);
        assert_eq!(back.threads.iter().map(|t| t.rgb).collect::<Vec<_>>(), [[200, 20, 30], [10, 40, 160]]);
        assert_eq!(back.threads[0].name, "#c8141e");
    }

    #[test]
    fn blocks_carry_their_lengths() {
        let bytes = write(&sample()).unwrap();
        assert!(bytes.starts_with(b"%vsm%\0"));
        // The file block's length runs to the end of the file.
        let at = 6 + 2 + PRODUCER.len() * 2 + 3;
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        assert_eq!(at + 4 + len, bytes.len());
        // The 40 mm move after the colour change is a long record.
        assert!(bytes.windows(6).any(|w| w == [0x80, 0x01, 0, 0, 0x01, 0x90]), "80 01 0 400");
    }

    #[test]
    fn damaged_input_does_not_panic() {
        let good = write(&sample()).unwrap();
        for n in [0, 5, 6, 40, 120, good.len() / 2, good.len() - 3] {
            let _ = read(&good[..n]);
        }
        let mut bad = good.clone();
        for i in (0..bad.len()).step_by(7) {
            bad[i] ^= 0xA5;
            let _ = read(&bad);
        }
        assert!(read(b"%vsm%").is_err());
    }
}
