//! Machine-level stitch patterns: the flat list of needle penetrations and
//! machine commands that embroidery machines (and their file formats) consume.

use crate::geom::{Affine, BBox, Point};
use crate::threads::{Thread, generic_chart};
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// Command attached to a stitch record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Cmd {
    /// Move to the position and penetrate the fabric.
    Stitch,
    /// Move the frame to the position without penetrating.
    Jump,
    /// Cut the thread(s). The position repeats the current needle position.
    Trim,
    /// Stop and switch to the next thread in [`Pattern::threads`].
    /// The position repeats the current needle position.
    ColorChange,
    /// Pause the machine without changing thread (e.g. to place appliqué fabric).
    Stop,
    /// End of design. The position repeats the current needle position.
    End,
}

/// One record: absolute position in mm (design space, +y down) and a command.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stitch {
    pub x: f64,
    pub y: f64,
    pub cmd: Cmd,
}

impl Stitch {
    #[inline]
    pub const fn new(x: f64, y: f64, cmd: Cmd) -> Self {
        Stitch { x, y, cmd }
    }

    #[inline]
    pub fn at(p: Point, cmd: Cmd) -> Self {
        Stitch { x: p.x, y: p.y, cmd }
    }

    #[inline]
    pub fn pos(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// True for records that move the needle/frame (stitches and jumps).
    #[inline]
    pub fn is_move(&self) -> bool {
        matches!(self.cmd, Cmd::Stitch | Cmd::Jump)
    }
}

/// A contiguous run of records sewn with one thread.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorBlock {
    /// Index into [`Pattern::threads`].
    pub thread: usize,
    /// Record range (excluding the ColorChange record that ends the block).
    pub range: Range<usize>,
}

/// A flat machine pattern.
///
/// Thread semantics: block 0 starts with `threads[0]`; every [`Cmd::ColorChange`]
/// advances to the next thread. `threads` therefore usually has
/// `color_changes + 1` entries (missing entries fall back to a default palette).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub stitches: Vec<Stitch>,
    pub threads: Vec<Thread>,
    /// Design name/label (stored by formats that support it).
    #[serde(default)]
    pub name: String,
}

impl Pattern {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn add(&mut self, p: Point, cmd: Cmd) {
        self.stitches.push(Stitch::at(p, cmd));
    }

    pub fn stitch(&mut self, p: Point) {
        self.add(p, Cmd::Stitch);
    }

    pub fn jump(&mut self, p: Point) {
        self.add(p, Cmd::Jump);
    }

    /// Current needle position (last record), or the origin.
    pub fn current(&self) -> Point {
        self.stitches.last().map(Stitch::pos).unwrap_or(Point::ZERO)
    }

    pub fn trim(&mut self) {
        let p = self.current();
        self.add(p, Cmd::Trim);
    }

    pub fn stop(&mut self) {
        let p = self.current();
        self.add(p, Cmd::Stop);
    }

    pub fn color_change(&mut self) {
        let p = self.current();
        self.add(p, Cmd::ColorChange);
    }

    /// Appends an [`Cmd::End`] record unless the pattern already ends with one.
    pub fn ensure_end(&mut self) {
        if self.stitches.last().is_none_or(|s| s.cmd != Cmd::End) {
            let p = self.current();
            self.add(p, Cmd::End);
        }
    }

    /// Bounds of actual needle penetrations.
    pub fn bounds(&self) -> BBox {
        BBox::from_points(self.stitches.iter().filter(|s| s.cmd == Cmd::Stitch).map(Stitch::pos))
    }

    /// Bounds of all moves including jumps.
    pub fn extents(&self) -> BBox {
        BBox::from_points(self.stitches.iter().filter(|s| s.is_move()).map(Stitch::pos))
    }

    pub fn count(&self, cmd: Cmd) -> usize {
        self.stitches.iter().filter(|s| s.cmd == cmd).count()
    }

    pub fn stitch_count(&self) -> usize {
        self.count(Cmd::Stitch)
    }

    /// Splits the records into colour blocks.
    pub fn color_blocks(&self) -> Vec<ColorBlock> {
        let mut blocks = Vec::new();
        let mut start = 0;
        let mut thread = 0;
        for (i, s) in self.stitches.iter().enumerate() {
            if s.cmd == Cmd::ColorChange {
                blocks.push(ColorBlock { thread, range: start..i });
                thread += 1;
                start = i + 1;
            }
        }
        blocks.push(ColorBlock { thread, range: start..self.stitches.len() });
        blocks
    }

    /// Thread for colour block `i`, falling back to a generic palette.
    pub fn thread(&self, i: usize) -> Thread {
        if let Some(t) = self.threads.get(i) {
            return t.clone();
        }
        let generic = generic_chart();
        generic.threads[i % generic.threads.len()].clone()
    }

    /// Thread index active at each record.
    pub fn thread_indices(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.stitches.len());
        let mut t = 0;
        for s in &self.stitches {
            out.push(t);
            if s.cmd == Cmd::ColorChange {
                t += 1;
            }
        }
        out
    }

    pub fn translate(&mut self, d: Point) {
        for s in &mut self.stitches {
            s.x += d.x;
            s.y += d.y;
        }
    }

    pub fn transform(&mut self, m: &Affine) {
        for s in &mut self.stitches {
            let p = m.apply(s.pos());
            s.x = p.x;
            s.y = p.y;
        }
    }

    /// Moves the design so that the centre of its bounds is at the origin.
    pub fn center_at_origin(&mut self) {
        let c = self.bounds().center();
        self.translate(-c);
    }

    /// Ensures there are at least as many threads as colour blocks
    /// (filling from the generic palette).
    pub fn fill_missing_threads(&mut self) {
        let need = self.count(Cmd::ColorChange) + 1;
        while self.threads.len() < need {
            let t = self.thread(self.threads.len());
            self.threads.push(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_blocks_split_on_color_change() {
        let mut p = Pattern::new();
        p.stitch(Point::new(0.0, 0.0));
        p.stitch(Point::new(1.0, 0.0));
        p.color_change();
        p.stitch(Point::new(2.0, 0.0));
        p.ensure_end();
        let blocks = p.color_blocks();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].range, 0..2);
        assert_eq!(blocks[1].thread, 1);
        assert_eq!(blocks[1].range, 3..5);
        p.fill_missing_threads();
        assert_eq!(p.threads.len(), 2);
    }
}
