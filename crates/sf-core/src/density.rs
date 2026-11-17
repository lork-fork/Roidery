//! How deep the thread lies: the layers of thread on each small square of a design, for Check
//! Design's dense spots and the density map. A layer is the fabric covered once: a tatami fill
//! is about 1.2 with its underlay, a satin column 2.5 to 3.5.

use crate::geom::Point;
use crate::pattern::{Cmd, Pattern};

/// Width of a thread lying on the fabric (mm).
pub const THREAD: f64 = 0.4;
/// Most cells across or down a grid, so a huge design keeps a bounded grid (the cells grow).
pub const MAX_CELLS: usize = 2000;

/// Layers of thread on a grid of square cells over a design's stitches.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerGrid {
    /// The top left corner of the first cell (mm, +y down).
    pub origin: Point,
    /// A cell's side (mm).
    pub cell: f64,
    pub columns: usize,
    pub rows: usize,
    /// Layers on each cell, row by row from the top.
    pub layers: Vec<f32>,
}

impl LayerGrid {
    /// The layers of `pattern`'s stitches on cells `cell` mm square (at least 0.1 mm; larger when
    /// the design would need more than [`MAX_CELLS`] across or down). The thread of each stitch
    /// is laid along it, sampled every 0.2 mm; jumps and the travel after a trim or colour
    /// change carry no thread.
    pub fn of(pattern: &Pattern, cell: f64) -> LayerGrid {
        let b = pattern.bounds();
        let (w, h) = (b.width(), b.height());
        let has_stitches = pattern.stitches.iter().any(|s| s.cmd == Cmd::Stitch);
        let sized = w.is_finite() && h.is_finite() && w >= 0.0 && h >= 0.0;
        if !(has_stitches && sized) {
            return LayerGrid { cell: cell.max(0.1), ..LayerGrid::default() };
        }
        let cell = if cell.is_finite() { cell.max(0.1) } else { 1.0 }.max(w.max(h) / (MAX_CELLS - 1) as f64);
        let (columns, rows) = (((w / cell).ceil() as usize + 1).min(MAX_CELLS), ((h / cell).ceil() as usize + 1).min(MAX_CELLS));
        let mut layers = vec![0.0f32; columns * rows];
        let at = |p: Point| -> Option<usize> {
            let (x, y) = ((p.x - b.min.x) / cell, (p.y - b.min.y) / cell);
            (x >= 0.0 && y >= 0.0 && (x as usize) < columns && (y as usize) < rows).then(|| y as usize * columns + x as usize)
        };
        let mut needle: Option<Point> = None;
        for s in &pattern.stitches {
            match s.cmd {
                Cmd::Stitch => {
                    let q = s.pos();
                    if let Some(p) = needle {
                        let len = p.distance(q);
                        if len.is_finite() {
                            let k = ((len / 0.2).ceil() as usize).clamp(1, 1000);
                            let add = (len / k as f64 * THREAD / (cell * cell)) as f32;
                            for m in 0..k {
                                if let Some(v) = at(p.lerp(q, (m as f64 + 0.5) / k as f64)).and_then(|c| layers.get_mut(c)) {
                                    *v += add;
                                }
                            }
                        }
                    }
                    needle = Some(q);
                }
                Cmd::Trim | Cmd::ColorChange | Cmd::End => needle = None,
                Cmd::Jump | Cmd::Stop => {}
            }
        }
        LayerGrid { origin: b.min, cell, columns, rows, layers }
    }

    /// The layers on the cell at column `x`, row `y` (0 outside the grid).
    pub fn get(&self, x: usize, y: usize) -> f32 {
        if x >= self.columns {
            return 0.0;
        }
        self.layers.get(y * self.columns + x).copied().unwrap_or(0.0)
    }

    /// The most layers on any cell.
    pub fn max(&self) -> f32 {
        self.layers.iter().copied().fold(0.0, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::Stitch;

    fn rows(n: usize, spacing: f64) -> Pattern {
        // back-and-forth rows 10 mm long, `spacing` apart: about THREAD / spacing layers
        let mut p = Pattern::new();
        for i in 0..n {
            let y = i as f64 * spacing;
            let (a, b) = if i % 2 == 0 { (0.0, 10.0) } else { (10.0, 0.0) };
            p.stitches.push(Stitch::new(a, y, Cmd::Stitch));
            p.stitches.push(Stitch::new(b, y, Cmd::Stitch));
        }
        p
    }

    #[test]
    fn layers_follow_how_close_the_rows_are() {
        let open = LayerGrid::of(&rows(21, 0.4), 1.0);
        let dense = LayerGrid::of(&rows(81, 0.1), 1.0);
        let mid = |g: &LayerGrid| g.get(5, 4);
        // rows 0.4 mm apart cover the fabric about once; 0.1 mm apart about four times
        assert!((mid(&open) - 1.0).abs() < 0.35, "{}", mid(&open));
        assert!((mid(&dense) - 4.0).abs() < 1.0, "{}", mid(&dense));
        assert!(dense.max() >= mid(&dense));
    }

    #[test]
    fn jumps_and_trims_lay_no_thread_and_huge_designs_keep_a_bounded_grid() {
        let mut p = Pattern::new();
        p.stitches.push(Stitch::new(0.0, 0.0, Cmd::Stitch));
        p.stitches.push(Stitch::new(0.0, 0.0, Cmd::Trim));
        p.stitches.push(Stitch::new(5000.0, 3000.0, Cmd::Jump));
        p.stitches.push(Stitch::new(5000.0, 3000.0, Cmd::Stitch));
        let g = LayerGrid::of(&p, 1.0);
        assert!(g.columns <= MAX_CELLS && g.rows <= MAX_CELLS);
        assert_eq!(g.max(), 0.0);
        assert_eq!(LayerGrid::of(&Pattern::new(), 1.0).layers.len(), 0);
    }
}
