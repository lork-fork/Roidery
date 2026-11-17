//! A small stitch-list builder shared by the generators.

use crate::geom::{Point, Polygon};
use crate::pattern::{Cmd, Stitch};

/// Accumulates the stitches of one object.
#[derive(Default)]
pub(crate) struct Out {
    pub v: Vec<Stitch>,
    /// Area that will be covered by stitching; connections that stay inside it
    /// are sewn as running stitches (hidden later) instead of trimmed jumps.
    pub region: Vec<Polygon>,
}

impl Out {
    pub fn new() -> Self {
        Out { v: Vec::new(), region: Vec::new() }
    }

    /// Whether the straight move `a → b` stays inside the covered region.
    fn inside_region(&self, a: Point, b: Point) -> bool {
        if self.region.is_empty() {
            return false;
        }
        let n = ((a.distance(b) / 0.5).ceil() as usize).clamp(1, 400);
        (0..=n).all(|k| {
            let q = a.lerp(b, k as f64 / n as f64);
            self.region.iter().any(|p| p.contains(q) || p.distance_to_boundary(q) < 0.25)
        })
    }

    /// Current needle position.
    pub fn pos(&self) -> Option<Point> {
        self.v.iter().rev().find(|s| s.is_move()).map(Stitch::pos)
    }

    /// Penetration at `p` (skipped if it would repeat the current position).
    pub fn stitch(&mut self, p: Point) {
        if let Some(last) = self.v.last()
            && last.cmd == Cmd::Stitch
            && last.pos().distance_sq(p) < 1e-10
        {
            return;
        }
        self.v.push(Stitch::at(p, Cmd::Stitch));
    }

    pub fn stitches(&mut self, pts: impl IntoIterator<Item = Point>) {
        for p in pts {
            self.stitch(p);
        }
    }

    pub fn jump(&mut self, p: Point) {
        self.v.push(Stitch::at(p, Cmd::Jump));
    }

    pub fn trim(&mut self) {
        if let Some(p) = self.pos() {
            self.v.push(Stitch::at(p, Cmd::Trim));
        }
    }

    pub fn stop(&mut self) {
        if let Some(p) = self.pos() {
            self.v.push(Stitch::at(p, Cmd::Stop));
        }
    }

    /// A stitch to `p` kept whole even when longer than `max`: the frame moves there
    /// in jumps no longer than `max` and the needle drops only at `p`.
    pub fn long_stitch(&mut self, p: Point, max: f64) {
        let max = max.max(1.0);
        if let Some(from) = self.pos() {
            let d = from.distance(p);
            if d > max {
                let k = (d / max).ceil() as usize;
                for m in 1..k {
                    self.jump(from.lerp(p, m as f64 / k as f64));
                }
            }
        }
        self.v.push(Stitch::at(p, Cmd::Stitch));
    }

    /// Straight running stitches from the current position to `p`, no stitch
    /// longer than `step`.
    pub fn run_to(&mut self, p: Point, step: f64) {
        let Some(from) = self.pos() else {
            self.stitch(p);
            return;
        };
        let d = from.distance(p);
        let n = ((d / step.max(0.1)).ceil() as usize).max(1);
        for k in 1..=n {
            self.stitch(from.lerp(p, k as f64 / n as f64));
        }
    }

    /// Running stitches along a polyline (starting with a run to its first point).
    pub fn run_along(&mut self, pts: &[Point], step: f64) {
        if pts.is_empty() {
            return;
        }
        if self.pos().is_some() {
            self.run_to(pts[0], step);
        }
        let r = crate::geom::polyline::resample(pts, step, 0.35);
        self.stitches(r);
    }

    /// Moves to `p` for a new part of the object: running stitches when close or
    /// when the move stays inside [`Out::region`], otherwise a trimmed jump (the
    /// pattern builder adds tie stitches).
    pub fn connect(&mut self, p: Point, trim_threshold: f64, step: f64) {
        match self.pos() {
            None => self.stitch(p),
            Some(q) if q.distance(p) <= trim_threshold || self.inside_region(q, p) => self.run_to(p, step),
            Some(_) => {
                self.trim();
                self.jump(p);
                self.stitch(p);
            }
        }
    }

    pub fn append(&mut self, other: Vec<Stitch>) {
        for s in other {
            match s.cmd {
                Cmd::Stitch => self.stitch(s.pos()),
                _ => self.v.push(s),
            }
        }
    }

    pub fn finish(self) -> Vec<Stitch> {
        self.v
    }
}

/// A line moved point by point by `f` (a warp of the plane), the stitches it stretches split
/// along the line before moving so none is longer than `step`.
pub(crate) fn mapped(pts: &[Point], step: f64, f: impl Fn(Point) -> Point) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(pts.len());
    let mut prev: Option<Point> = None;
    for &q in pts {
        let p = f(q);
        if let (Some(a), Some(&last)) = (prev, out.last()) {
            let parts = ((last.distance(p) / step).ceil() as usize).clamp(1, 64);
            out.extend((1..parts).map(|j| f(a.lerp(q, j as f64 / parts as f64))));
        }
        out.push(p);
        prev = Some(q);
    }
    out
}

/// Converts a list of points to stitch records.
pub(crate) fn as_stitches(pts: impl IntoIterator<Item = Point>) -> Vec<Stitch> {
    let mut o = Out::new();
    o.stitches(pts);
    o.finish()
}
