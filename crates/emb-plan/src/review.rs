//! Checking a design before it sews: how long it takes, and where the thread piles up.

use roidery_formats::{Command, Design};
use serde::{Deserialize, Serialize};

/// Thread width on the fabric, mm (a 40 wt thread lies about this wide).
pub const THREAD_MM: f64 = 0.4;
/// Size of a density map's cells, mm: large enough that rows a fill's spacing apart average out.
pub const DENSITY_CELL_MM: f64 = 2.0;
/// Layers of thread above which an area is dense: stiff, and the needle starts to struggle.
pub const DENSE_LAYERS: f64 = 1.8;
/// Layers above which an area is too dense: it puckers, and needles and thread break.
pub const TOO_DENSE_LAYERS: f64 = 2.6;
/// Cells per map are capped so a huge design can't run away.
const MAX_CELLS: usize = 1_000_000;

/// How fast a machine sews, for the sew-time estimate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MachineSpeed {
    /// Stitches per minute.
    pub stitches_per_minute: f64,
    /// Seconds to trim the thread.
    pub trim_seconds: f64,
    /// Seconds to change thread colour (by hand on a single-needle machine).
    pub color_change_seconds: f64,
}

impl Default for MachineSpeed {
    /// A home single-needle machine.
    fn default() -> Self {
        Self { stitches_per_minute: 650.0, trim_seconds: 5.0, color_change_seconds: 30.0 }
    }
}

/// Minutes the design takes to sew.
pub fn sew_minutes(d: &Design, m: &MachineSpeed) -> f64 {
    let ok = |v: f64, def: f64| if v.is_finite() && v > 0.0 { v } else { def };
    let spm = ok(m.stitches_per_minute, 650.0);
    let secs = d.trims() as f64 * ok(m.trim_seconds, 0.0) + d.color_changes() as f64 * ok(m.color_change_seconds, 0.0);
    d.stitch_count() as f64 / spm + secs / 60.0
}

/// Top thread each penetration takes beyond the stitch's length, mm: down through the fabric to
/// the bobbin thread and back up.
pub const TAKE_UP_MM: f64 = 0.8;
/// Thread left as tails at each trim and colour change, mm.
pub const TAIL_MM: f64 = 15.0;
/// Bobbin thread as a share of the top thread, with balanced tension (a third of a satin's width
/// shows underneath).
pub const BOBBIN_SHARE: f64 = 1.0 / 3.0;

/// How much thread a design uses, metres: an estimate to buy and wind bobbins by.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadUse {
    /// Top thread for each colour block, in sewing order.
    pub per_block: Vec<f64>,
    pub bobbin: f64,
}

impl ThreadUse {
    /// Top thread, all colours.
    pub fn top(&self) -> f64 {
        self.per_block.iter().sum()
    }
}

/// The thread `d` uses: each stitch's length plus the take-up, jumps that aren't trimmed (the
/// thread runs along them), and tails at trims and colour changes.
pub fn thread_use(d: &Design) -> ThreadUse {
    let mut per_block = vec![0.0];
    let mut at: Option<(f64, f64)> = None;
    for s in &d.stitches {
        if !(s.x.is_finite() && s.y.is_finite()) {
            continue;
        }
        let len = at.map_or(0.0, |(x, y)| (s.x - x).hypot(s.y - y));
        let add = match s.command {
            Command::Stitch => len + TAKE_UP_MM,
            Command::Jump => len,
            Command::Trim => TAIL_MM,
            Command::ColorChange => TAIL_MM,
            Command::End => 0.0,
        };
        if let Some(b) = per_block.last_mut() {
            *b += add;
        }
        if s.command == Command::ColorChange {
            per_block.push(0.0);
        }
        if s.command != Command::End {
            at = Some((s.x, s.y));
        }
    }
    // Before anything is sewn, a jump carries no thread yet; drop blocks with nothing sewn.
    let per_block: Vec<f64> = per_block.into_iter().filter(|m| *m > TAIL_MM).map(|mm| mm / 1000.0).collect();
    let bobbin = per_block.iter().sum::<f64>() * BOBBIN_SHARE;
    ThreadUse { per_block, bobbin }
}

/// Layers of thread across a design, cell by cell: 1 is fabric covered once (a fill at its usual
/// density), 2 twice over.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DensityMap {
    /// The first cell's top-left corner, mm.
    pub x0: f64,
    pub y0: f64,
    /// Cell size, mm.
    pub cell: f64,
    pub cols: usize,
    pub rows: usize,
    /// Layers in each cell, row by row from the top.
    pub layers: Vec<f32>,
}

impl DensityMap {
    pub fn max(&self) -> f64 {
        self.layers.iter().copied().fold(0.0, f32::max).into()
    }

    /// Cells above `layers`.
    pub fn count_above(&self, layers: f64) -> usize {
        self.layers.iter().filter(|l| f64::from(**l) > layers).count()
    }

    /// The layers in the cell at (`x`, `y`) mm.
    pub fn at(&self, x: f64, y: f64) -> f64 {
        let (c, r) = ((x - self.x0) / self.cell, (y - self.y0) / self.cell);
        if !(c >= 0.0 && r >= 0.0) {
            return 0.0;
        }
        let (c, r) = (c as usize, r as usize);
        if c >= self.cols || r >= self.rows {
            return 0.0;
        }
        self.layers.get(r * self.cols + c).copied().map_or(0.0, f64::from)
    }
}

/// The density map of a design, `cell` mm cells.
pub fn density(d: &Design, cell: f64) -> DensityMap {
    let cell = if cell.is_finite() && cell >= 0.5 { cell } else { DENSITY_CELL_MM };
    let Some(b) = d.bounds() else { return DensityMap { cell, ..Default::default() } };
    let (mut cols, mut rows) = (((b.width() / cell).floor() as usize + 1), ((b.height() / cell).floor() as usize + 1));
    let mut cell = cell;
    while cols.saturating_mul(rows) > MAX_CELLS {
        cell *= 2.0;
        (cols, rows) = ((b.width() / cell).floor() as usize + 1, (b.height() / cell).floor() as usize + 1);
    }
    let mut area = vec![0.0f64; cols * rows];
    // Each piece of thread is shared between the four cells whose centres surround it, so rows
    // that happen to fall either side of a cell edge don't alias into stripes.
    let mut add = |x: f64, y: f64, a: f64| {
        let (fx, fy) = ((x - b.min_x) / cell - 0.5, (y - b.min_y) / cell - 0.5);
        let (c0, r0) = (fx.floor(), fy.floor());
        let (wx, wy) = (fx - c0, fy - r0);
        for (dc, dr, w) in [(0.0, 0.0, (1.0 - wx) * (1.0 - wy)), (1.0, 0.0, wx * (1.0 - wy)), (0.0, 1.0, (1.0 - wx) * wy), (1.0, 1.0, wx * wy)] {
            let (c, r) = (c0 + dc, r0 + dr);
            // At the design's edge the share past it goes to the edge cell.
            let (c, r) = (c.clamp(0.0, (cols - 1) as f64) as usize, r.clamp(0.0, (rows - 1) as f64) as usize);
            if let Some(v) = area.get_mut(r * cols + c) {
                *v += a * w;
            }
        }
    };
    let mut at: Option<(f64, f64)> = None;
    for s in &d.stitches {
        if !(s.x.is_finite() && s.y.is_finite()) {
            continue;
        }
        if s.command == Command::Stitch
            && let Some((x, y)) = at
        {
            // The thread laid by this stitch, shared out along it.
            let len = (s.x - x).hypot(s.y - y);
            let n = ((len / (cell / 4.0)).ceil() as usize).clamp(1, 10_000);
            let piece = len / n as f64;
            for k in 0..n {
                let t = (k as f64 + 0.5) / n as f64;
                add(x + (s.x - x) * t, y + (s.y - y) * t, piece * THREAD_MM);
            }
        }
        if s.command != Command::End {
            at = Some((s.x, s.y));
        }
    }
    let cell_area = cell * cell;
    DensityMap { x0: b.min_x, y0: b.min_y, cell, cols, rows, layers: area.into_iter().map(|a| (a / cell_area) as f32).collect() }
}

#[cfg(test)]
mod tests {
    use roidery_formats::{MachineStitch, Thread};

    use super::*;

    /// Back-and-forth rows `spacing` apart across a `size` mm square, `passes` times over.
    fn rows(size: f64, spacing: f64, passes: usize) -> Design {
        let mut st = vec![MachineStitch::new(0.0, 0.0, Command::Jump)];
        for _ in 0..passes {
            let mut y = 0.0;
            let mut left = true;
            while y <= size {
                st.push(MachineStitch::new(if left { 0.0 } else { size }, y, Command::Stitch));
                st.push(MachineStitch::new(if left { size } else { 0.0 }, y, Command::Stitch));
                left = !left;
                y += spacing;
            }
        }
        st.push(MachineStitch::new(0.0, 0.0, Command::End));
        Design { stitches: st, threads: vec![Thread::default()], name: String::new() }
    }

    #[test]
    fn a_fill_at_its_usual_density_is_one_layer_and_doubling_it_is_two() {
        let one = density(&rows(20.0, 0.4, 1), DENSITY_CELL_MM);
        // Inner cells (the rows' turns run up the sides).
        let inner =
            |m: &DensityMap| (2..8).flat_map(|r| (2..8).map(move |c| (r, c))).map(|(r, c)| f64::from(m.layers[r * m.cols + c])).collect::<Vec<_>>();
        assert!(inner(&one).iter().all(|l| (l - 1.0).abs() < 0.15), "{:?}", inner(&one));
        let two = density(&rows(20.0, 0.4, 2), DENSITY_CELL_MM);
        assert!(inner(&two).iter().all(|l| (l - 2.0).abs() < 0.3));
        assert!(two.max() > TOO_DENSE_LAYERS - 0.7 && one.count_above(DENSE_LAYERS) < one.layers.len() / 4);
        assert!((one.at(10.0, 10.0) - 1.0).abs() < 0.15 && one.at(-5.0, 0.0) == 0.0);
        assert_eq!(density(&Design::default(), 2.0).layers.len(), 0);
    }

    #[test]
    fn thread_use_is_the_sewn_length_with_take_up_per_colour() {
        // 20 mm square, rows 1 mm apart: 21 rows of 20 mm, 20 steps of 1 mm across.
        let mut d = rows(20.0, 1.0, 1);
        let one = thread_use(&d);
        assert_eq!(one.per_block.len(), 1);
        let sewn = 21.0 * 20.0 + 20.0 * 1.0 + 42.0 * TAKE_UP_MM;
        assert!((one.top() * 1000.0 - sewn).abs() < 1.0, "{}", one.top() * 1000.0);
        assert!((one.bobbin - one.top() / 3.0).abs() < 1e-9);
        // A second colour adds its own block.
        d.stitches.pop();
        d.stitches.push(MachineStitch::new(0.0, 0.0, Command::ColorChange));
        d.stitches.extend(rows(20.0, 1.0, 1).stitches);
        let two = thread_use(&d);
        assert_eq!(two.per_block.len(), 2);
        assert!((two.per_block[1] - one.per_block[0]).abs() < 0.05);
        assert_eq!(thread_use(&Design::default()).per_block.len(), 0);
    }

    #[test]
    fn sew_time_counts_stitches_trims_and_colour_changes() {
        let mut d = rows(10.0, 1.0, 1);
        let base = sew_minutes(&d, &MachineSpeed::default());
        assert!((base - d.stitch_count() as f64 / 650.0).abs() < 1e-9);
        d.stitches.insert(3, MachineStitch::new(0.0, 0.0, Command::Trim));
        d.stitches.insert(4, MachineStitch::new(0.0, 0.0, Command::ColorChange));
        let more = sew_minutes(&d, &MachineSpeed::default());
        assert!((more - base - 35.0 / 60.0).abs() < 1e-9);
        let bad = MachineSpeed { stitches_per_minute: f64::NAN, ..Default::default() };
        assert!(sew_minutes(&d, &bad).is_finite());
    }
}
