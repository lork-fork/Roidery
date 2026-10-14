//! Roidery machine embroidery formats.
//!
//! A [`Design`] is what a machine sews: needle positions in millimetres (y down, as on the canvas)
//! with the command at each one, and the thread colours in sewing order. Readers and writers
//! translate it to and from the machine file formats.
//!
//! Formats read and written:
//! - [`dst`]: Tajima DST, and [`tap`]: Tajima TAP.
//! - [`pes`]: Brother PES (version 1, with the PEC block Brother machines sew from).
//! - [`jef`]: Janome JEF.
//! - [`exp`]: Melco EXP.
//! - [`vp3`]: Husqvarna Viking / Pfaff VP3.
//! - [`xxx`]: Singer XXX.
//! - [`u01`]: Barudan U01.
//!
//! Read only: [`dsb`] (Barudan DSB), [`dsz`] (ZSK DSZ) and [`hus`] (Husqvarna Viking HUS).
//!
//! [`charts`] holds thread charts: the built-in Brother and Janome charts and chart import.
//! [`hoops`] holds the hoops of each machine brand and checks a design fits one.
//!
//! The format notes in each module come from the public format descriptions (libembroidery's
//! documentation and sources, zlib licence, and pyembroidery's, MIT licence, both used as
//! references and test oracles). Thread
//! palettes are the manufacturers' published colour charts.
#![forbid(unsafe_code)]

pub mod charts;
pub mod dsb;
pub mod dst;
pub mod dsz;
pub mod exp;
pub mod hoops;
pub mod hus;
pub mod jef;
pub mod pes;
pub mod tap;
pub mod u01;
pub mod vp3;
pub mod xxx;

use serde::{Deserialize, Serialize};

/// What the machine does at one needle position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    /// Move here with the needle going through the fabric.
    Stitch,
    /// Move here without stitching (the thread is carried over the fabric).
    Jump,
    /// Cut the thread here. The next stitch starts a new thread.
    Trim,
    /// Stop and change to the next thread colour.
    ColorChange,
    /// End of the design.
    End,
}

/// One needle position.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MachineStitch {
    /// Millimetres, x right.
    pub x: f64,
    /// Millimetres, y down.
    pub y: f64,
    pub command: Command,
}

impl MachineStitch {
    pub fn new(x: f64, y: f64, command: Command) -> Self {
        Self { x, y, command }
    }
}

/// A thread colour.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Thread {
    /// sRGB, 0–255.
    pub rgb: [u8; 3],
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Brand catalogue number, e.g. Madeira Polyneon "1800".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code: String,
}

/// A design ready to sew: stitches in order, and one thread per colour block.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Design {
    pub stitches: Vec<MachineStitch>,
    /// One thread per colour block: the design starts with `threads[0]` and each
    /// [`Command::ColorChange`] moves to the next.
    pub threads: Vec<Thread>,
    /// Design name, written where the format has room for one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

/// Axis-aligned bounds in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }
    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }
}

impl Design {
    /// Needle-down stitches (what the stitch counter on a machine counts).
    pub fn stitch_count(&self) -> usize {
        self.stitches.iter().filter(|s| s.command == Command::Stitch).count()
    }

    /// Colour changes in the stitch list.
    pub fn color_changes(&self) -> usize {
        self.stitches.iter().filter(|s| s.command == Command::ColorChange).count()
    }

    /// Trims in the stitch list.
    pub fn trims(&self) -> usize {
        self.stitches.iter().filter(|s| s.command == Command::Trim).count()
    }

    /// Bounds of every position except the end marker; `None` for an empty design.
    pub fn bounds(&self) -> Option<Bounds> {
        let mut it = self.stitches.iter().filter(|s| s.command != Command::End && s.x.is_finite() && s.y.is_finite());
        let first = it.next()?;
        let mut b = Bounds { min_x: first.x, min_y: first.y, max_x: first.x, max_y: first.y };
        for s in it {
            b.min_x = b.min_x.min(s.x);
            b.min_y = b.min_y.min(s.y);
            b.max_x = b.max_x.max(s.x);
            b.max_y = b.max_y.max(s.y);
        }
        Some(b)
    }
}

/// Errors reading or writing a machine file.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum FormatError {
    #[error("not a {format} file: {why}")]
    Invalid { format: &'static str, why: String },
    #[error("the design is too large for {format}: {why}")]
    TooLarge { format: &'static str, why: String },
    #[error("a stitch position is not a finite number")]
    NotFinite,
}

/// The machine formats Roidery reads (and, where [`Format::can_write`], writes), by file
/// extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Dst,
    Pes,
    Jef,
    Exp,
    Vp3,
    Xxx,
    U01,
    Tap,
    Dsb,
    Dsz,
    Hus,
}

impl Format {
    pub const ALL: [Format; 11] = [
        Format::Dst,
        Format::Pes,
        Format::Jef,
        Format::Exp,
        Format::Vp3,
        Format::Xxx,
        Format::U01,
        Format::Tap,
        Format::Dsb,
        Format::Dsz,
        Format::Hus,
    ];

    pub fn extension(self) -> &'static str {
        match self {
            Format::Dst => "dst",
            Format::Pes => "pes",
            Format::Jef => "jef",
            Format::Exp => "exp",
            Format::Vp3 => "vp3",
            Format::Xxx => "xxx",
            Format::U01 => "u01",
            Format::Tap => "tap",
            Format::Dsb => "dsb",
            Format::Dsz => "dsz",
            Format::Hus => "hus",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Dst => "Tajima DST",
            Format::Pes => "Brother PES",
            Format::Jef => "Janome JEF",
            Format::Exp => "Melco EXP",
            Format::Vp3 => "Husqvarna Viking VP3",
            Format::Xxx => "Singer XXX",
            Format::U01 => "Barudan U01",
            Format::Tap => "Tajima TAP",
            Format::Dsb => "Barudan DSB",
            Format::Dsz => "ZSK DSZ",
            Format::Hus => "Husqvarna Viking HUS",
        }
    }

    /// Whether Roidery writes this format (every format is read).
    pub fn can_write(self) -> bool {
        !matches!(self, Format::Dsb | Format::Dsz | Format::Hus)
    }

    /// The formats Roidery writes.
    pub fn writable() -> impl Iterator<Item = Format> {
        Self::ALL.into_iter().filter(|f| f.can_write())
    }

    /// The format for a file extension (case-insensitive, with or without the dot).
    pub fn from_extension(ext: &str) -> Option<Self> {
        let ext = ext.trim_start_matches('.');
        Self::ALL.into_iter().find(|f| f.extension().eq_ignore_ascii_case(ext))
    }

    pub fn write(self, design: &Design) -> Result<Vec<u8>, FormatError> {
        match self {
            Format::Dst => dst::write(design),
            Format::Pes => pes::write(design),
            Format::Jef => jef::write(design),
            Format::Exp => exp::write(design),
            Format::Vp3 => vp3::write(design),
            Format::Xxx => xxx::write(design),
            Format::U01 => u01::write(design),
            Format::Tap => tap::write(design),
            Format::Dsb | Format::Dsz | Format::Hus => {
                Err(FormatError::Invalid { format: self.label(), why: "Roidery reads this format but doesn't write it".into() })
            }
        }
    }

    pub fn read(self, bytes: &[u8]) -> Result<Design, FormatError> {
        match self {
            Format::Dst => dst::read(bytes),
            Format::Pes => pes::read(bytes),
            Format::Jef => jef::read(bytes),
            Format::Exp => exp::read(bytes),
            Format::Vp3 => vp3::read(bytes),
            Format::Xxx => xxx::read(bytes),
            Format::U01 => u01::read(bytes),
            Format::Tap => tap::read(bytes),
            Format::Dsb => dsb::read(bytes),
            Format::Dsz => dsz::read(bytes),
            Format::Hus => hus::read(bytes),
        }
    }
}

/// Positions in 0.1 mm units (y down), rounded once each so moves between them add up exactly.
/// ±10 m keeps the integer maths safe and is far beyond any hoop.
pub(crate) fn to_units(design: &Design, format: &'static str) -> Result<Vec<(i32, i32, Command)>, FormatError> {
    const LIMIT: f64 = 100_000.0;
    design
        .stitches
        .iter()
        .map(|s| {
            if !s.x.is_finite() || !s.y.is_finite() {
                return Err(FormatError::NotFinite);
            }
            let (x, y) = ((s.x * 10.0).round(), (s.y * 10.0).round());
            if x.abs() > LIMIT || y.abs() > LIMIT {
                return Err(FormatError::TooLarge { format, why: format!("a stitch at ({:.1}, {:.1}) mm", s.x, s.y) });
            }
            Ok((x as i32, y as i32, s.command))
        })
        .collect()
}

/// A move of (`dx`, `dy`) split into the fewest steps of at most `max` per axis, each step a
/// rounded point along the line so the steps add up to the move exactly. A zero move is one
/// zero step.
pub(crate) fn steps(dx: i32, dy: i32, max: i32) -> Vec<(i32, i32)> {
    let max = max.max(1);
    let n = ((dx.abs().max(dy.abs()) + max - 1) / max).max(1);
    let mut out = Vec::with_capacity(n as usize);
    let (mut px, mut py) = (0, 0);
    for i in 1..=n {
        let x = (f64::from(dx) * f64::from(i) / f64::from(n)).round() as i32;
        let y = (f64::from(dy) * f64::from(i) / f64::from(n)).round() as i32;
        out.push((x - px, y - py));
        (px, py) = (x, y);
    }
    out
}

/// The index of the colour in `palette` nearest to `rgb` (by weighted RGB distance), skipping
/// the first `skip` entries.
pub(crate) fn nearest(rgb: [u8; 3], palette: &[[u8; 3]], skip: usize) -> usize {
    let d = |p: &[u8; 3]| {
        let c = |i: usize| f64::from(rgb[i]) - f64::from(p[i]);
        let rmean = (f64::from(rgb[0]) + f64::from(p[0])) / 2.0;
        (2.0 + rmean / 256.0) * c(0) * c(0) + 4.0 * c(1) * c(1) + (2.0 + (255.0 - rmean) / 256.0) * c(2) * c(2)
    };
    palette.iter().enumerate().skip(skip).min_by(|a, b| d(a.1).total_cmp(&d(b.1))).map_or(skip, |(i, _)| i)
}

/// The thread for each colour block: `threads`, cut or padded (with black) to `blocks`.
pub(crate) fn block_threads(design: &Design) -> Vec<Thread> {
    let blocks = design.color_changes() + 1;
    let mut t: Vec<Thread> = design.threads.iter().take(blocks).cloned().collect();
    t.resize(blocks, Thread::default());
    t
}

/// Little-endian readers that fail instead of panicking on short input.
pub(crate) struct Bytes<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    pub format: &'static str,
}

impl<'a> Bytes<'a> {
    pub fn new(data: &'a [u8], format: &'static str) -> Self {
        Self { data, pos: 0, format }
    }
    pub fn short(&self) -> FormatError {
        FormatError::Invalid { format: self.format, why: format!("the file ends early (at byte {})", self.pos) }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        let end = self.pos.checked_add(n).ok_or_else(|| self.short())?;
        let s = self.data.get(self.pos..end).ok_or_else(|| self.short())?;
        self.pos = end;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8, FormatError> {
        Ok(self.take(1)?.first().copied().unwrap_or(0))
    }
    pub fn i32(&mut self) -> Result<i32, FormatError> {
        let b = self.take(4)?;
        let mut a = [0u8; 4];
        a.copy_from_slice(b);
        Ok(i32::from_le_bytes(a))
    }
    pub fn seek(&mut self, pos: usize) -> Result<(), FormatError> {
        if pos > self.data.len() {
            return Err(FormatError::Invalid { format: self.format, why: format!("an offset ({pos}) past the end of the file") });
        }
        self.pos = pos;
        Ok(())
    }
}

/// Builds a [`Design`] from relative moves in 0.1 mm units.
pub(crate) struct Reader {
    pub design: Design,
    x: i32,
    y: i32,
}

/// Bound on stitches read, so a damaged file can't make a reader allocate without limit.
pub(crate) const MAX_READ: usize = 5_000_000;

impl Reader {
    pub fn new() -> Self {
        Self { design: Design::default(), x: 0, y: 0 }
    }
    /// Move by (`dx`, `dy`) units, y down, and record `command` there.
    pub fn add(&mut self, dx: i32, dy: i32, command: Command, format: &'static str) -> Result<(), FormatError> {
        if self.design.stitches.len() >= MAX_READ {
            return Err(FormatError::TooLarge { format, why: "more than 5 million stitches".into() });
        }
        self.x = self.x.saturating_add(dx);
        self.y = self.y.saturating_add(dy);
        self.design.stitches.push(MachineStitch::new(f64::from(self.x) / 10.0, f64::from(self.y) / 10.0, command));
        Ok(())
    }
    /// A change of needle (thread): a colour change, except before the first stitch, where it
    /// only picks the needle the design starts with.
    pub fn needle(&mut self, format: &'static str) -> Result<(), FormatError> {
        if self.design.stitches.iter().any(|s| s.command == Command::Stitch) {
            self.add(0, 0, Command::ColorChange, format)?;
        }
        Ok(())
    }
    /// Close the design with an end marker at the last position.
    pub fn finish(mut self) -> Design {
        if self.design.stitches.last().is_none_or(|s| s.command != Command::End) {
            self.design.stitches.push(MachineStitch::new(f64::from(self.x) / 10.0, f64::from(self.y) / 10.0, Command::End));
        }
        self.design
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;

    /// A small two-colour design with a trim and a long jump: what every writer must carry.
    pub fn sample() -> Design {
        let mut s = vec![MachineStitch::new(0.0, 0.0, Command::Jump)];
        for i in 0..=20 {
            s.push(MachineStitch::new(f64::from(i) * 1.5, if i % 2 == 0 { 0.0 } else { 4.0 }, Command::Stitch));
        }
        s.push(MachineStitch::new(30.0, 0.0, Command::Trim));
        s.push(MachineStitch::new(30.0, 0.0, Command::ColorChange));
        // A 40 mm jump: longer than one record in every format.
        s.push(MachineStitch::new(30.0, 40.0, Command::Jump));
        for i in 0..=10 {
            s.push(MachineStitch::new(30.0 - f64::from(i) * 2.0, 40.0 + if i % 2 == 0 { 0.0 } else { -3.0 }, Command::Stitch));
        }
        s.push(MachineStitch::new(10.0, 40.0, Command::End));
        Design {
            stitches: s,
            threads: vec![Thread { rgb: [200, 20, 30], ..Default::default() }, Thread { rgb: [10, 40, 160], ..Default::default() }],
            name: "Sample".into(),
        }
    }

    /// Needle-down positions in 0.1 mm, for comparing a design with what a reader gives back.
    pub fn sewn(d: &Design) -> Vec<(i32, i32)> {
        d.stitches.iter().filter(|s| s.command == Command::Stitch).map(|s| ((s.x * 10.0).round() as i32, (s.y * 10.0).round() as i32)).collect()
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

        /// Every reader survives garbage, and damaged copies of what the writers write.
        #[test]
        fn readers_never_panic(garbage in proptest::collection::vec(any::<u8>(), 0..1200), cut in 0usize..2000, edits in proptest::collection::vec((0usize..2000, any::<u8>()), 0..12)) {
            for f in Format::ALL {
                let _ = f.read(&garbage);
                let written = if f.can_write() { f.write(&testutil::sample()).unwrap() } else { garbage.clone() };
                let mut damaged: Vec<u8> = written.iter().copied().take(cut.max(1)).collect();
                let n = damaged.len().max(1);
                for (at, b) in &edits {
                    if let Some(slot) = damaged.get_mut(*at % n) {
                        *slot = *b;
                    }
                }
                let _ = f.read(&damaged);
            }
        }
    }

    #[test]
    fn every_writer_round_trips_the_sample() {
        let d = testutil::sample();
        for f in Format::writable() {
            let back = f.read(&f.write(&d).unwrap()).unwrap();
            assert_eq!(testutil::sewn(&back), testutil::sewn(&d), "{}", f.label());
            assert_eq!(back.color_changes(), 1, "{}", f.label());
        }
        for f in Format::ALL.into_iter().filter(|f| !f.can_write()) {
            assert!(f.write(&d).is_err());
        }
    }
}
