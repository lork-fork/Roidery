//! Husqvarna Viking HUS (read only; Roidery writes Husqvarna designs as VP3).
//!
//! Header (little-endian): `u32` magic, `u32` stitch count, `u32` colour count, `i16` extents
//! (+x, +y, -x, -y), `u32` offsets of the command, x and y streams, an 8-byte string, a `u16`,
//! then one `u16` per colour: an index into the HUS palette ([`PALETTE`]).
//!
//! The three streams are compressed separately (see [`expand`]). Stitch `i` is command byte
//! `i`, with signed bytes `x[i]` and `y[i]` (y up) as its move in 0.1 mm:
//!
//! | Command | Meaning                       |
//! |---------|-------------------------------|
//! | `80`    | stitch                        |
//! | `81`    | jump                          |
//! | `84`    | move, then colour change      |
//! | `88`    | move, then trim               |
//! | `90`    | end                           |

use crate::{Bytes, Command, Design, FormatError, MAX_READ, Reader, Thread};

const FORMAT: &str = "HUS";
const HEADER_LEN: usize = 0x2A;

/// The HUS thread palette: colour, name and Husqvarna's number.
pub const PALETTE: [([u8; 3], &str, &str); 29] = [
    ([0x00, 0x00, 0x00], "Black", "026"),
    ([0x00, 0x00, 0xE7], "Blue", "005"),
    ([0x00, 0xC6, 0x00], "Green", "002"),
    ([0xFF, 0x00, 0x00], "Red", "014"),
    ([0x84, 0x00, 0x84], "Purple", "008"),
    ([0xFF, 0xFF, 0x00], "Yellow", "020"),
    ([0x84, 0x84, 0x84], "Grey", "024"),
    ([0x84, 0x84, 0xE7], "Light Blue", "006"),
    ([0x00, 0xFF, 0x84], "Light Green", "003"),
    ([0xFF, 0x7B, 0x31], "Orange", "017"),
    ([0xFF, 0x8C, 0xA5], "Pink", "011"),
    ([0x84, 0x52, 0x00], "Brown", "028"),
    ([0xFF, 0xFF, 0xFF], "White", "022"),
    ([0x00, 0x00, 0x84], "Dark Blue", "004"),
    ([0x00, 0x84, 0x00], "Dark Green", "001"),
    ([0x7B, 0x00, 0x00], "Dark Red", "013"),
    ([0xFF, 0x63, 0x84], "Light Red", "015"),
    ([0x52, 0x29, 0x52], "Dark Purple", "007"),
    ([0xFF, 0x00, 0xFF], "Light Purple", "009"),
    ([0xFF, 0xDE, 0x00], "Dark Yellow", "019"),
    ([0xFF, 0xFF, 0x9C], "Light Yellow", "021"),
    ([0x52, 0x52, 0x52], "Dark Grey", "025"),
    ([0xD6, 0xD6, 0xD6], "Light Grey", "023"),
    ([0xFF, 0x52, 0x08], "Dark Orange", "016"),
    ([0xFF, 0x9C, 0x5A], "Light Orange", "018"),
    ([0xFF, 0x52, 0xB5], "Dark Pink", "010"),
    ([0xFF, 0xC6, 0xDE], "Light Pink", "012"),
    ([0x52, 0x31, 0x00], "Dark Brown", "027"),
    ([0xB5, 0xA5, 0x84], "Light Brown", "029"),
];

pub fn read(bytes: &[u8]) -> Result<Design, FormatError> {
    let invalid = |why: String| FormatError::Invalid { format: FORMAT, why };
    let mut b = Bytes::new(bytes, FORMAT);
    let _magic = b.i32()?;
    let count = usize::try_from(b.i32()?).map_err(|_| invalid("a negative stitch count".into()))?;
    if count > MAX_READ {
        return Err(FormatError::TooLarge { format: FORMAT, why: "more than 5 million stitches".into() });
    }
    let colors = usize::try_from(b.i32()?).map_err(|_| invalid("a negative colour count".into()))?.min(1000);
    b.take(8)?;
    let mut offset = || -> Result<usize, FormatError> { usize::try_from(b.i32()?).map_err(|_| invalid("a negative offset".into())) };
    let (cmd_at, x_at, y_at) = (offset()?, offset()?, offset()?);
    b.seek(HEADER_LEN)?;
    let mut threads = Vec::with_capacity(colors);
    for _ in 0..colors {
        let Ok(&[lo, hi]) = b.take(2) else { break };
        let t = PALETTE.get(usize::from(u16::from_le_bytes([lo, hi]))).map(|(rgb, name, code)| Thread {
            rgb: *rgb,
            name: (*name).into(),
            code: (*code).into(),
        });
        threads.push(t.unwrap_or_default());
    }
    let stream = |from: usize, to: usize| -> Result<Vec<u8>, FormatError> {
        let data =
            bytes.get(from..to.max(from).min(bytes.len())).ok_or_else(|| invalid(format!("a stream offset ({from}) past the end of the file")))?;
        expand(data, count)
    };
    let cmds = stream(cmd_at, x_at)?;
    let xs = stream(x_at, y_at)?;
    let ys = stream(y_at, bytes.len())?;
    let mut r = Reader::new();
    for ((c, x), y) in cmds.iter().zip(&xs).zip(&ys) {
        let (dx, dy) = (i32::from(*x as i8), -i32::from(*y as i8));
        let moved = dx != 0 || dy != 0;
        match c {
            0x80 => r.add(dx, dy, Command::Stitch, FORMAT)?,
            0x81 => r.add(dx, dy, Command::Jump, FORMAT)?,
            0x84 | 0x88 => {
                if moved {
                    r.add(dx, dy, Command::Jump, FORMAT)?;
                }
                let command = if *c == 0x84 { Command::ColorChange } else { Command::Trim };
                r.add(0, 0, command, FORMAT)?;
            }
            // 0x90 ends the design; anything else is data we don't understand.
            _ => break,
        }
    }
    let mut design = r.finish();
    design.threads = threads;
    Ok(design)
}

/// Reads bits most significant first; past the end, bits read as zero.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn peek(&self, n: u32) -> u32 {
        let mut v: u64 = 0;
        for i in 0..n as usize {
            let at = self.pos + i;
            let bit = self.data.get(at / 8).map_or(0, |byte| (byte >> (7 - at % 8)) & 1);
            v = (v << 1) | u64::from(bit);
        }
        v as u32
    }
    fn pop(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.pos += n as usize;
        v
    }
    fn done(&self) -> bool {
        self.pos >= self.data.len() * 8
    }
    /// 3 bits; 7 continues in unary (each 1 adds one, up to 13 more).
    fn variable(&mut self) -> u32 {
        let mut m = self.pop(3);
        if m == 7 {
            for _ in 0..13 {
                if self.pop(1) == 1 {
                    m += 1;
                } else {
                    break;
                }
            }
        }
        m
    }
}

/// A Huffman code given by its code lengths (shorter codes first; lower values first among equal
/// lengths), or a single value that takes no bits.
enum Huffman {
    Fixed(u32),
    Table { lengths: Vec<u32>, table: Vec<u32>, width: u32 },
}

impl Huffman {
    fn new(lengths: Vec<u32>) -> Result<Self, FormatError> {
        let width = lengths.iter().copied().max().unwrap_or(0);
        if width == 0 || width > 16 {
            return Err(FormatError::Invalid { format: FORMAT, why: format!("a Huffman code {width} bits long") });
        }
        let mut table = Vec::with_capacity(1 << width);
        for len in 1..=width {
            for (v, l) in lengths.iter().enumerate() {
                if *l == len {
                    table.extend(std::iter::repeat_n(v as u32, 1 << (width - len)));
                }
            }
        }
        Ok(Huffman::Table { lengths, table, width })
    }

    /// The next value, consuming its code.
    fn read(&self, bits: &mut Bits) -> Result<u32, FormatError> {
        match self {
            Huffman::Fixed(v) => Ok(*v),
            Huffman::Table { lengths, table, width } => {
                let bad = || FormatError::Invalid { format: FORMAT, why: "an incomplete Huffman code".into() };
                let v = *table.get((bits.peek(16) >> (16 - width)) as usize).ok_or_else(bad)?;
                bits.pos += *lengths.get(v as usize).ok_or_else(bad)? as usize;
                Ok(v)
            }
        }
    }
}

/// One compressed block's codes.
struct Block {
    left: u32,
    chars: Huffman,
    distances: Huffman,
}

fn load_block(bits: &mut Bits) -> Result<Block, FormatError> {
    let bad = |why: &str| FormatError::Invalid { format: FORMAT, why: why.into() };
    let left = bits.pop(16);
    // The code for the character code lengths.
    let count = bits.pop(5) as usize;
    let length_code = if count == 0 {
        Huffman::Fixed(bits.pop(5))
    } else {
        let mut lengths = vec![0; count];
        let mut i = 0;
        while i < count {
            if i == 3 {
                // Index 3 may skip up to three lengths.
                i += bits.pop(2) as usize;
            }
            *lengths.get_mut(i).ok_or_else(|| bad("a code length past the table"))? = bits.variable();
            i += 1;
        }
        Huffman::new(lengths)?
    };
    // The character code: literals 0–255, back references 256–509, end 510.
    let count = bits.pop(9) as usize;
    let chars = if count == 0 {
        Huffman::Fixed(bits.pop(9))
    } else {
        let mut lengths = vec![0; count];
        let mut i = 0;
        while i < count {
            match length_code.read(bits)? {
                0 => i += 1,
                1 => i += 3 + bits.pop(4) as usize,
                2 => i += 20 + bits.pop(9) as usize,
                c => {
                    if let Some(l) = lengths.get_mut(i) {
                        *l = c - 2;
                    }
                    i += 1;
                }
            }
        }
        Huffman::new(lengths)?
    };
    let count = bits.pop(5) as usize;
    let distances = if count == 0 { Huffman::Fixed(bits.pop(5)) } else { Huffman::new((0..count).map(|_| bits.variable()).collect())? };
    Ok(Block { left, chars, distances })
}

/// Decompress one HUS stream (an LZ77 scheme with Huffman-coded blocks, as in Husqvarna's and
/// Pfaff's older formats) into at most `limit` bytes.
pub fn expand(data: &[u8], limit: usize) -> Result<Vec<u8>, FormatError> {
    let mut bits = Bits { data, pos: 0 };
    let mut out: Vec<u8> = Vec::with_capacity(limit.min(MAX_READ));
    let mut block: Option<Block> = None;
    while !bits.done() && out.len() < limit {
        let blk = match block.as_mut() {
            Some(b) if b.left > 0 => b,
            _ => block.insert(load_block(&mut bits)?),
        };
        blk.left = blk.left.saturating_sub(1);
        let token = blk.chars.read(&mut bits)?;
        match token {
            0..=255 => out.push(token as u8),
            510 => break,
            _ => {
                let length = (token - 253) as usize;
                let back = match blk.distances.read(&mut bits)? {
                    0 => 1,
                    v => (1usize << (v - 1).min(30)) + bits.pop((v - 1).min(30)) as usize + 1,
                };
                let start = out
                    .len()
                    .checked_sub(back)
                    .ok_or_else(|| FormatError::Invalid { format: FORMAT, why: "a back reference before the start".into() })?;
                for i in start..start + length {
                    let Some(&byte) = out.get(i) else { break };
                    out.push(byte);
                }
            }
        }
    }
    out.truncate(limit);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes bits most significant first.
    #[derive(Default)]
    struct BitWriter {
        bytes: Vec<u8>,
        n: usize,
    }

    impl BitWriter {
        fn put(&mut self, v: u32, width: u32) {
            for i in (0..width).rev() {
                if self.n.is_multiple_of(8) {
                    self.bytes.push(0);
                }
                if (v >> i) & 1 == 1 {
                    *self.bytes.last_mut().unwrap() |= 0x80 >> (self.n % 8);
                }
                self.n += 1;
            }
        }
    }

    /// pyembroidery's `compress`: one block of plain 8-bit literals.
    fn stored(data: &[u8]) -> Vec<u8> {
        let n = data.len() as u16;
        let mut v = n.to_le_bytes().to_vec();
        v.extend([0x02, 0xA0, 0x01, 0xFE]);
        v.extend(data);
        v
    }

    /// One block of 9-bit tokens (every character code 9 bits long, the distance a fixed value).
    fn tokens(tokens: &[u32], distance: u32) -> Vec<u8> {
        let mut w = BitWriter::default();
        w.put(tokens.len() as u32, 16);
        // Character code lengths: none listed, all `11 - 2 = 9`.
        w.put(0, 5);
        w.put(11, 5);
        w.put(511, 9);
        w.put(0, 5);
        w.put(distance, 5);
        for t in tokens {
            w.put(*t, 9);
        }
        w.bytes
    }

    #[test]
    fn expands_stored_and_coded_blocks() {
        // A stored stream. Its block count is read most significant bit first (pyembroidery's
        // `compress` writes it the other way round), so swap the two bytes.
        let data: Vec<u8> = (0..=255u8).collect();
        let mut s = stored(&data);
        s.swap(0, 1);
        assert_eq!(expand(&s, 256).unwrap(), data);
        // "ab", then 5 bytes from 1 back ("bbbbb"); 510 ends the stream.
        let t = tokens(&[97, 98, 256 + 2, 510, 99], 0);
        assert_eq!(expand(&t, 100).unwrap(), b"abbbbbb");
        // Distance code 2: 1 << 1, plus one extra bit, plus one.
        let mut w = BitWriter::default();
        for (v, n) in [(5, 16), (0, 5), (11, 5), (511, 9), (0, 5), (2, 5), (97, 9), (98, 9), (99, 9), (256, 9), (0, 1), (510, 9)] {
            w.put(v, n);
        }
        assert_eq!(expand(&w.bytes, 100).unwrap(), b"abcabc");
        // The limit stops it.
        assert_eq!(expand(&t, 3).unwrap(), b"abb");
    }

    #[test]
    fn reads_a_design() {
        let cmds = [0x80, 0x80, 0x88, 0x84, 0x80, 0x90];
        let xs = [0u8, 10, 0, 20, 0, 0];
        let ys = [0u8, 0xFB, 0, 0, 15, 0];
        let (c, x, y) = (stored(&cmds), stored(&xs), stored(&ys));
        let mut f = Vec::new();
        f.extend(0x00C8AF5Bu32.to_le_bytes());
        f.extend(6u32.to_le_bytes());
        f.extend(2u32.to_le_bytes());
        f.extend([0u8; 8]);
        let at = HEADER_LEN + 4;
        for o in [at, at + c.len(), at + c.len() + x.len()] {
            f.extend((o as u32).to_le_bytes());
        }
        f.extend([0u8; 10]);
        f.extend([3, 0, 1, 0]);
        // Block counts most significant byte first.
        for mut s in [c, x, y] {
            s.swap(0, 1);
            f.extend(s);
        }
        let d = read(&f).unwrap();
        let got: Vec<(f64, f64, Command)> = d.stitches.iter().map(|s| (s.x, s.y, s.command)).collect();
        assert_eq!(
            got,
            vec![
                (0.0, 0.0, Command::Stitch),
                (1.0, 0.5, Command::Stitch),
                (1.0, 0.5, Command::Trim),
                (3.0, 0.5, Command::Jump),
                (3.0, 0.5, Command::ColorChange),
                (3.0, -1.0, Command::Stitch),
                (3.0, -1.0, Command::End)
            ]
        );
        assert_eq!(d.threads.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Red", "Blue"]);
    }

    #[test]
    fn damaged_input_does_not_panic() {
        assert!(read(&[0u8; 10]).is_err());
        for seed in 0..200u32 {
            let mut v: Vec<u8> = (0..300u32).map(|i| (i.wrapping_mul(2654435761).wrapping_add(seed * 97) >> 13) as u8).collect();
            v[4..8].copy_from_slice(&50u32.to_le_bytes());
            v[8..12].copy_from_slice(&2u32.to_le_bytes());
            for (k, o) in [44u32, 120, 200].into_iter().enumerate() {
                v[20 + k * 4..24 + k * 4].copy_from_slice(&o.to_le_bytes());
            }
            let _ = read(&v);
            let _ = expand(&v, 1000);
        }
    }
}
