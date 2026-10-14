//! Tajima DST.
//!
//! Layout: a 512-byte ASCII header, then one 3-byte record per needle movement, relative to the
//! previous position, in 0.1 mm units with y pointing up.
//!
//! Header lines end in CR (0x0D): `LA:` name (16 characters), `ST:` record count, `CO:` colour
//! changes, `+X:` `-X:` `+Y:` `-Y:` extents in 0.1 mm, `AX:` `AY:` end position relative to the
//! start (sign + 5 digits), `MX:` `MY:` (multi-volume, zero here), `PD:******`, then 0x1A and
//! spaces up to 512 bytes.
//!
//! Each record moves by at most ±121 units per axis, encoded in balanced ternary:
//!
//! | Bit    | 7    | 6     | 5    | 4    | 3    | 2    | 1   | 0   |
//! |--------|------|-------|------|------|------|------|-----|-----|
//! | Byte 0 | y+1  | y-1   | y+9  | y-9  | x-9  | x+9  | x-1 | x+1 |
//! | Byte 1 | y+3  | y-3   | y+27 | y-27 | x-27 | x+27 | x-3 | x+3 |
//! | Byte 2 | jump | stop  | y+81 | y-81 | x-81 | x+81 | 1   | 1   |
//!
//! Byte 2's low bits are 0x03 on every record; 0x83 marks a jump, 0xC3 a colour change (stop),
//! and `00 00 F3` ends the design. DST has no trim record: machines cut the thread on a run of
//! jumps, so a trim is written as three jumps that end where they started, and a reader treats
//! three or more jumps in a row as a trim.

use crate::{Command, Design, FormatError, MachineStitch};

const FORMAT: &str = "DST";
pub(crate) const HEADER_LEN: usize = 512;
/// Largest move one record can make, per axis, in 0.1 mm.
pub const MAX_STEP: i32 = 121;
/// Jumps in a row that a reader takes as a trim.
pub const TRIM_JUMPS: usize = 3;
/// Bound on records read, so a damaged file can't make us allocate without limit.
const MAX_RECORDS: usize = 5_000_000;

/// The flag bits of a record's third byte.
const JUMP: u8 = 0x83;
const STOP: u8 = 0xC3;
const END: [u8; 3] = [0x00, 0x00, 0xF3];

/// Encode a move of (`dx`, `dy`) units (y up) into the three record bytes, without flags.
/// `None` when either value is outside ±121.
pub fn encode_ternary(dx: i32, dy: i32) -> Option<[u8; 3]> {
    if !(-MAX_STEP..=MAX_STEP).contains(&dx) || !(-MAX_STEP..=MAX_STEP).contains(&dy) {
        return None;
    }
    let mut b = [0u8; 3];
    // (weight, byte index, bit for +, bit for -) from the largest weight down.
    const X: [(i32, usize, u8, u8); 5] = [(81, 2, 0x04, 0x08), (27, 1, 0x04, 0x08), (9, 0, 0x04, 0x08), (3, 1, 0x01, 0x02), (1, 0, 0x01, 0x02)];
    const Y: [(i32, usize, u8, u8); 5] = [(81, 2, 0x20, 0x10), (27, 1, 0x20, 0x10), (9, 0, 0x20, 0x10), (3, 1, 0x80, 0x40), (1, 0, 0x80, 0x40)];
    for (mut v, table) in [(dx, X), (dy, Y)] {
        for (w, i, plus, minus) in table {
            // A digit of the balanced ternary is +1 when what is left is over half the weight.
            let half = w / 2;
            let slot = b.get_mut(i)?;
            if v > half {
                *slot |= plus;
                v -= w;
            } else if v < -half {
                *slot |= minus;
                v += w;
            }
        }
        if v != 0 {
            return None;
        }
    }
    Some(b)
}

/// Decode a record's move (`dx`, `dy`) in units, y up.
pub fn decode_ternary(b: [u8; 3]) -> (i32, i32) {
    let bit = |byte: u8, mask: u8, w: i32| if byte & mask != 0 { w } else { 0 };
    let [b0, b1, b2] = b;
    let x = bit(b0, 0x01, 1) - bit(b0, 0x02, 1) + bit(b0, 0x04, 9) - bit(b0, 0x08, 9) + bit(b1, 0x01, 3) - bit(b1, 0x02, 3) + bit(b1, 0x04, 27)
        - bit(b1, 0x08, 27)
        + bit(b2, 0x04, 81)
        - bit(b2, 0x08, 81);
    let y = bit(b0, 0x80, 1) - bit(b0, 0x40, 1) + bit(b0, 0x20, 9) - bit(b0, 0x10, 9) + bit(b1, 0x80, 3) - bit(b1, 0x40, 3) + bit(b1, 0x20, 27)
        - bit(b1, 0x10, 27)
        + bit(b2, 0x20, 81)
        - bit(b2, 0x10, 81);
    (x, y)
}

/// Collects records while tracking the position in units (y up) and the extents.
struct Writer {
    records: Vec<[u8; 3]>,
    x: i32,
    y: i32,
    min: (i32, i32),
    max: (i32, i32),
    color_changes: usize,
}

impl Writer {
    fn record(&mut self, dx: i32, dy: i32, flags: u8) -> Result<(), FormatError> {
        let mut b = encode_ternary(dx, dy).ok_or_else(|| too_large(format!("a move of ({dx}, {dy}) units in one record")))?;
        b[2] |= flags;
        self.records.push(b);
        self.x += dx;
        self.y += dy;
        self.min = (self.min.0.min(self.x), self.min.1.min(self.y));
        self.max = (self.max.0.max(self.x), self.max.1.max(self.y));
        Ok(())
    }

    /// Move to (`tx`, `ty`) in as few records as fit, all with `flags` (a long stitch becomes
    /// several shorter stitches on the same line, a long jump several jumps).
    fn move_to(&mut self, tx: i32, ty: i32, flags: u8) -> Result<(), FormatError> {
        let (dx, dy) = (tx - self.x, ty - self.y);
        if dx == 0 && dy == 0 && flags != 0x03 {
            // Nothing to travel; a jump in place would read back as part of a trim.
            return Ok(());
        }
        let steps = (dx.abs().max(dy.abs()) + MAX_STEP - 1) / MAX_STEP;
        if steps <= 1 {
            return self.record(dx, dy, flags);
        }
        let (x0, y0) = (self.x, self.y);
        for i in 1..=steps {
            // Rounded points along the line, so the pieces add up exactly.
            let px = x0 + ((dx as i64 * i as i64) as f64 / steps as f64).round() as i32;
            let py = y0 + ((dy as i64 * i as i64) as f64 / steps as f64).round() as i32;
            self.record(px - self.x, py - self.y, flags)?;
        }
        Ok(())
    }
}

fn too_large(why: String) -> FormatError {
    FormatError::TooLarge { format: FORMAT, why }
}

/// Millimetres (y down) to DST units (y up).
fn units(s: &MachineStitch) -> Result<(i32, i32), FormatError> {
    if !s.x.is_finite() || !s.y.is_finite() {
        return Err(FormatError::NotFinite);
    }
    let (x, y) = ((s.x * 10.0).round(), (-s.y * 10.0).round());
    // ±10 m is far beyond any hoop; it keeps the integer maths safe.
    if x.abs() > 100_000.0 || y.abs() > 100_000.0 {
        return Err(too_large(format!("a stitch at ({:.1} mm, {:.1} mm)", s.x, s.y)));
    }
    Ok((x as i32, y as i32))
}

/// Write `design` as DST. Positions are relative to the design's origin, which is where the
/// machine's needle starts (usually the hoop centre).
pub fn write(design: &Design) -> Result<Vec<u8>, FormatError> {
    let w = encode(design)?;
    let mut out = header(design, &w)?;
    out.reserve(w.records.len() * 3);
    for r in &w.records {
        out.extend_from_slice(r);
    }
    Ok(out)
}

/// The stitch records alone, with no header (Tajima TAP).
pub(crate) fn write_records(design: &Design) -> Result<Vec<u8>, FormatError> {
    Ok(encode(design)?.records.concat())
}

fn encode(design: &Design) -> Result<Writer, FormatError> {
    let mut w = Writer { records: vec![], x: 0, y: 0, min: (0, 0), max: (0, 0), color_changes: 0 };
    for s in &design.stitches {
        let (tx, ty) = units(s)?;
        match s.command {
            Command::Stitch => w.move_to(tx, ty, 0x03)?,
            Command::Jump => w.move_to(tx, ty, JUMP)?,
            Command::Trim => {
                w.move_to(tx, ty, JUMP)?;
                // Three jumps that come back to the same point: (2, 2), (-4, -4), (2, 2).
                w.record(2, 2, JUMP)?;
                w.record(-4, -4, JUMP)?;
                w.record(2, 2, JUMP)?;
            }
            Command::ColorChange => {
                w.move_to(tx, ty, JUMP)?;
                w.record(0, 0, STOP)?;
                w.color_changes += 1;
            }
            Command::End => break,
        }
    }
    w.records.push(END);
    Ok(w)
}

fn header(design: &Design, w: &Writer) -> Result<Vec<u8>, FormatError> {
    let field = |v: usize, digits: u32, what: &str| -> Result<usize, FormatError> {
        if v >= 10usize.pow(digits) { Err(too_large(format!("{v} {what} (DST holds at most {} digits)", digits))) } else { Ok(v) }
    };
    let st = field(w.records.len(), 7, "records")?;
    let co = field(w.color_changes, 3, "colour changes")?;
    let ext = |v: i32| field(v.unsigned_abs() as usize, 5, "units of extent");
    let sign = |v: i32| if v < 0 { '-' } else { '+' };
    let name: String = design.name.chars().map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '_' }).take(16).collect::<String>();
    let name = if name.trim().is_empty() { "Roidery".to_string() } else { name };
    let mut h = String::with_capacity(HEADER_LEN);
    h.push_str(&format!("LA:{name:<16}\r"));
    h.push_str(&format!("ST:{st:>7}\r"));
    h.push_str(&format!("CO:{co:>3}\r"));
    h.push_str(&format!("+X:{:>5}\r", ext(w.max.0)?));
    h.push_str(&format!("-X:{:>5}\r", ext(w.min.0)?));
    h.push_str(&format!("+Y:{:>5}\r", ext(w.max.1)?));
    h.push_str(&format!("-Y:{:>5}\r", ext(w.min.1)?));
    h.push_str(&format!("AX:{}{:>5}\r", sign(w.x), ext(w.x)?));
    h.push_str(&format!("AY:{}{:>5}\r", sign(w.y), ext(w.y)?));
    h.push_str("MX:+    0\rMY:+    0\rPD:******\r");
    let mut bytes = h.into_bytes();
    bytes.push(0x1A);
    if bytes.len() > HEADER_LEN {
        return Err(too_large("the header".into()));
    }
    bytes.resize(HEADER_LEN, b' ');
    Ok(bytes)
}

/// Read a DST file. Runs of [`TRIM_JUMPS`] or more jumps become a trim where the run starts and
/// one jump to where it ends.
pub fn read(bytes: &[u8]) -> Result<Design, FormatError> {
    let invalid = |why: &str| FormatError::Invalid { format: FORMAT, why: why.into() };
    if bytes.len() < HEADER_LEN {
        return Err(invalid("shorter than the 512-byte header"));
    }
    let head = bytes.get(..HEADER_LEN).ok_or_else(|| invalid("no header"))?;
    if !head.starts_with(b"LA:") {
        return Err(invalid("the header does not start with LA:"));
    }
    read_records(bytes.get(HEADER_LEN..).unwrap_or_default(), header_name(head))
}

/// The design name in a DST-style header's `LA:` field (empty when there is none).
pub(crate) fn header_name(head: &[u8]) -> String {
    if !head.starts_with(b"LA:") {
        return String::new();
    }
    head.get(3..19).map(|n| String::from_utf8_lossy(n).split(['\r', '\u{1a}']).next().unwrap_or_default().trim().to_string()).unwrap_or_default()
}

/// Read DST stitch records (no header), as in a DST file's body or a Tajima TAP file.
pub(crate) fn read_records(body: &[u8], name: String) -> Result<Design, FormatError> {
    let invalid = |why: &str| FormatError::Invalid { format: FORMAT, why: why.into() };
    let mut design = Design { name, ..Design::default() };
    let (mut x, mut y) = (0i32, 0i32);
    let mm = |x: i32, y: i32| (f64::from(x) / 10.0, -f64::from(y) / 10.0);
    // Jumps not yet emitted: the run's start position and how many there were.
    let mut run: Option<((i32, i32), usize)> = None;
    let flush = |design: &mut Design, run: &mut Option<((i32, i32), usize)>, x: i32, y: i32| {
        if let Some(((sx, sy), n)) = run.take() {
            if n >= TRIM_JUMPS {
                let (ax, ay) = mm(sx, sy);
                design.stitches.push(MachineStitch::new(ax, ay, Command::Trim));
            }
            if (sx, sy) != (x, y) || n < TRIM_JUMPS {
                let (bx, by) = mm(x, y);
                design.stitches.push(MachineStitch::new(bx, by, Command::Jump));
            }
        }
    };
    let mut ended = false;
    for (i, chunk) in body.chunks_exact(3).enumerate() {
        if i >= MAX_RECORDS {
            return Err(invalid("more records than any real design has"));
        }
        let rec = [chunk.first().copied().unwrap_or(0), chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        if rec[2] == END[2] {
            ended = true;
            break;
        }
        let (dx, dy) = decode_ternary(rec);
        let flags = rec[2] & 0xC3;
        if flags == STOP {
            flush(&mut design, &mut run, x, y);
            x += dx;
            y += dy;
            let (ax, ay) = mm(x, y);
            design.stitches.push(MachineStitch::new(ax, ay, Command::ColorChange));
        } else if flags & 0x80 != 0 {
            if run.is_none() {
                run = Some(((x, y), 0));
            }
            if let Some((_, n)) = run.as_mut() {
                *n += 1;
            }
            x += dx;
            y += dy;
        } else {
            flush(&mut design, &mut run, x, y);
            x += dx;
            y += dy;
            let (ax, ay) = mm(x, y);
            design.stitches.push(MachineStitch::new(ax, ay, Command::Stitch));
        }
    }
    flush(&mut design, &mut run, x, y);
    let (ax, ay) = mm(x, y);
    design.stitches.push(MachineStitch::new(ax, ay, Command::End));
    let _ = ended; // A file cut short still yields the stitches it has, as machines do.
    design.threads = vec![Default::default(); design.color_changes() + 1];
    Ok(design)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Thread;
    use proptest::prelude::*;

    fn st(x: f64, y: f64) -> MachineStitch {
        MachineStitch::new(x, y, Command::Stitch)
    }

    #[test]
    fn ternary_round_trips_every_value() {
        for dx in -MAX_STEP..=MAX_STEP {
            for dy in [-121, -60, -1, 0, 1, 13, 14, 40, 41, 121] {
                let b = encode_ternary(dx, dy).unwrap();
                assert_eq!(b[2] & 0xC3, 0, "no flag bits from the move alone");
                assert_eq!(decode_ternary(b), (dx, dy));
            }
        }
        assert!(encode_ternary(122, 0).is_none());
        assert!(encode_ternary(0, -122).is_none());
    }

    #[test]
    fn known_records_match_the_spec_table() {
        // +1 x is bit 0 of byte 0; +1 y is bit 7 of byte 0; +81 x is bit 2 of byte 2.
        assert_eq!(encode_ternary(1, 0), Some([0x01, 0, 0]));
        assert_eq!(encode_ternary(0, 1), Some([0x80, 0, 0]));
        assert_eq!(encode_ternary(81, 0), Some([0, 0, 0x04]));
        assert_eq!(encode_ternary(-81, -81), Some([0, 0, 0x18]));
        assert_eq!(encode_ternary(121, 121), Some([0xA5, 0xA5, 0x24]));
    }

    #[test]
    fn header_is_512_bytes_with_the_standard_fields() {
        let d = Design { stitches: vec![st(0.0, 0.0), st(10.0, -5.0), st(-3.0, 2.0)], threads: vec![Thread::default()], name: "Star".into() };
        let bytes = write(&d).unwrap();
        let head = String::from_utf8_lossy(&bytes[..125]).to_string();
        assert!(head.starts_with("LA:Star            \rST:      5\rCO:  0\r"), "{head:?}");
        assert!(head.contains("+X:  100\r-X:   30\r+Y:   50\r-Y:   20\r"), "{head:?}");
        assert!(head.contains("AX:-   30\rAY:-   20\r"), "{head:?}");
        assert_eq!(bytes[124], 0x1A);
        assert!(bytes[125..512].iter().all(|b| *b == b' '));
        assert_eq!((bytes.len() - 512) % 3, 0);
        assert_eq!(&bytes[bytes.len() - 3..], &END);
    }

    #[test]
    fn long_moves_are_split_and_add_up() {
        let d = Design { stitches: vec![st(0.0, 0.0), MachineStitch::new(50.0, 0.0, Command::Jump), st(50.0, 30.0)], ..Default::default() };
        let back = read(&write(&d).unwrap()).unwrap();
        let last = back.stitches.iter().rev().find(|s| s.command == Command::Stitch).unwrap();
        assert!((last.x - 50.0).abs() < 1e-9 && (last.y - 30.0).abs() < 1e-9);
        // 30 mm stitch = 300 units: three stitches of at most 12.1 mm.
        assert_eq!(back.stitch_count(), 1 + 3);
    }

    #[test]
    fn trims_and_colour_changes_survive_a_round_trip() {
        let d = Design {
            stitches: vec![
                st(0.0, 0.0),
                st(2.0, 0.0),
                MachineStitch::new(2.0, 0.0, Command::Trim),
                MachineStitch::new(8.0, 1.0, Command::Jump),
                MachineStitch::new(8.0, 1.0, Command::ColorChange),
                st(9.0, 1.0),
                MachineStitch::new(9.0, 1.0, Command::End),
            ],
            threads: vec![Thread::default(), Thread::default()],
            name: String::new(),
        };
        let back = read(&write(&d).unwrap()).unwrap();
        let cmds: Vec<Command> = back.stitches.iter().map(|s| s.command).collect();
        assert_eq!(cmds, vec![Command::Stitch, Command::Stitch, Command::Trim, Command::Jump, Command::ColorChange, Command::Stitch, Command::End]);
        assert_eq!(back.threads.len(), 2);
    }

    #[test]
    fn rejects_garbage_and_non_finite() {
        assert!(read(b"hello").is_err());
        assert!(read(&[0u8; 600]).is_err());
        let d = Design { stitches: vec![st(f64::NAN, 0.0)], ..Default::default() };
        assert_eq!(write(&d), Err(FormatError::NotFinite));
    }

    proptest! {
        #[test]
        fn stitches_land_within_rounding(points in proptest::collection::vec((-150.0f64..150.0, -150.0f64..150.0), 1..60)) {
            let d = Design { stitches: points.iter().map(|(x, y)| st(*x, *y)).collect(), ..Default::default() };
            let back = read(&write(&d).unwrap()).unwrap();
            // Every original stitch appears (splitting only adds points), within 0.05 mm.
            let got: Vec<_> = back.stitches.iter().filter(|s| s.command == Command::Stitch).collect();
            let mut j = 0;
            for (x, y) in &points {
                while j < got.len() && ((got[j].x - x).abs() > 0.051 || (got[j].y - y).abs() > 0.051) { j += 1; }
                prop_assert!(j < got.len(), "stitch ({x}, {y}) missing");
                j += 1;
            }
        }

        #[test]
        fn reading_arbitrary_bytes_never_panics(body in proptest::collection::vec(any::<u8>(), 0..900)) {
            let mut bytes = b"LA:x".to_vec();
            bytes.resize(512, b' ');
            bytes.extend(body);
            let _ = read(&bytes);
        }
    }
}
