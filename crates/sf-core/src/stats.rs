//! Production statistics: stitch counts, size, thread usage and run time.

use crate::geom::BBox;
use crate::model::DesignSettings;
use crate::pattern::{Cmd, Pattern};
use crate::threads::Thread;

/// Extra thread consumed per penetration (loop formation), in mm.
const THREAD_PER_PENETRATION: f64 = 0.85;
/// Bobbin thread is roughly a third of top thread for typical satin/fill work.
const BOBBIN_RATIO: f64 = 0.33;

#[derive(Clone, Debug, PartialEq)]
pub struct ColorUsage {
    pub thread: Thread,
    pub stitches: usize,
    /// Top thread length in metres.
    pub thread_m: f64,
    /// Jumps sewn in this colour's block (a jump before its first stitch counts here).
    pub jumps: usize,
    /// Trims in this colour's block.
    pub trims: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PatternStats {
    pub stitches: usize,
    pub jumps: usize,
    pub trims: usize,
    pub color_changes: usize,
    pub stops: usize,
    pub bounds: BBox,
    pub width_mm: f64,
    pub height_mm: f64,
    /// Total top thread (m).
    pub thread_m: f64,
    /// Estimated bobbin thread (m).
    pub bobbin_m: f64,
    /// Estimated sewing time in seconds.
    pub seconds: f64,
    pub colors: Vec<ColorUsage>,
    /// Longest stitch (mm).
    pub max_stitch_mm: f64,
}

impl PatternStats {
    pub fn time_string(&self) -> String {
        let s = self.seconds.round() as u64;
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    }
}

pub fn pattern_stats(p: &Pattern, settings: &DesignSettings) -> PatternStats {
    let mut stitches = 0;
    let mut jumps = 0;
    let mut trims = 0;
    let mut color_changes = 0;
    let mut stops = 0;
    let mut thread_mm = 0.0;
    let mut max_stitch: f64 = 0.0;
    let mut colors: Vec<ColorUsage> = vec![ColorUsage { thread: p.thread(0), stitches: 0, thread_m: 0.0, jumps: 0, trims: 0 }];
    let mut prev = None;
    for s in &p.stitches {
        let pos = s.pos();
        match s.cmd {
            Cmd::Stitch => {
                stitches += 1;
                let used = prev.map(|q: crate::geom::Point| {
                    let d = pos.distance(q);
                    max_stitch = max_stitch.max(d);
                    d + THREAD_PER_PENETRATION
                });
                thread_mm += used.unwrap_or(0.0);
                if let Some(cu) = colors.last_mut() {
                    cu.stitches += 1;
                    cu.thread_m += used.unwrap_or(0.0) / 1000.0;
                }
            }
            Cmd::Jump => {
                jumps += 1;
                if let Some(cu) = colors.last_mut() {
                    cu.jumps += 1;
                }
            }
            Cmd::Trim => {
                trims += 1;
                if let Some(cu) = colors.last_mut() {
                    cu.trims += 1;
                }
            }
            Cmd::ColorChange => {
                color_changes += 1;
                colors.push(ColorUsage { thread: p.thread(colors.len()), stitches: 0, thread_m: 0.0, jumps: 0, trims: 0 });
            }
            Cmd::Stop => stops += 1,
            Cmd::End => {}
        }
        if s.is_move() {
            prev = Some(pos);
        }
    }
    colors.retain(|c| c.stitches > 0);
    let bounds = p.bounds();
    let speed = settings.machine_speed.max(1.0);
    let seconds =
        stitches as f64 / speed * 60.0 + trims as f64 * settings.trim_seconds + (color_changes + stops) as f64 * settings.color_change_seconds;
    PatternStats {
        stitches,
        jumps,
        trims,
        color_changes,
        stops,
        bounds,
        width_mm: bounds.width(),
        height_mm: bounds.height(),
        thread_m: thread_mm / 1000.0,
        bobbin_m: thread_mm / 1000.0 * BOBBIN_RATIO,
        seconds,
        colors,
        max_stitch_mm: max_stitch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point;

    #[test]
    fn jumps_and_trims_are_counted_per_colour() {
        let mut p = Pattern::new();
        p.stitch(Point::new(0.0, 0.0));
        p.stitch(Point::new(1.0, 0.0));
        p.trim();
        p.jump(Point::new(5.0, 0.0));
        p.stitch(Point::new(5.0, 1.0));
        p.color_change();
        p.stitch(Point::new(6.0, 1.0));
        p.stitch(Point::new(7.0, 1.0));
        let st = pattern_stats(&p, &DesignSettings::default());
        assert_eq!((st.jumps, st.trims), (st.colors.iter().map(|c| c.jumps).sum(), st.colors.iter().map(|c| c.trims).sum()));
        assert_eq!(st.colors.len(), 2);
        assert_eq!((st.colors[0].jumps, st.colors[0].trims), (1, 1));
        assert_eq!((st.colors[1].jumps, st.colors[1].trims), (0, 0));
    }
}
