//! Line stitches that decorate an edge: the E-stitch (blanket stitch) and the rope.
//!
//! An E-stitch runs along the path and, every `spacing` mm, sews a prong `width` mm out to the
//! left of the direction of travel and back, like the letter E. Appliqué edges use it to hold
//! the fabric down, so on a closed shape the prongs point inside it; on an open line they point
//! left, and reversing the path puts them on the other side.
//!
//! A rope is a narrow column of slanted stitches: each one crosses from the left edge to a
//! point `slant` mm further along the right edge, then the needle comes back to the left edge
//! `spacing` mm on, so the stitches lie over each other like the strands of a cord. Double and
//! triple ropes sew the column again in the other direction, each pass shifted so its stitches
//! fall between the earlier ones.

use kurbo::{Point, Vec2};

use crate::geometry::Polyline;
use crate::settings::{EStitchSettings, RopeSettings};
use crate::{Stitch, StitchKind};

/// Positions along a path are capped so a tiny spacing on a long path can't run away.
const MAX_STEPS: usize = 200_000;

/// A chain of points measured along its length.
struct Chain {
    pts: Vec<Point>,
    /// Distance from the start to each point.
    at: Vec<f64>,
}

impl Chain {
    fn new(path: &Polyline) -> Option<Self> {
        let pts: Vec<Point> = path.vertices().into_iter().filter(|p| p.x.is_finite() && p.y.is_finite()).collect();
        let mut at = Vec::with_capacity(pts.len());
        let mut d = 0.0;
        for (i, p) in pts.iter().enumerate() {
            if let Some(prev) = i.checked_sub(1).and_then(|j| pts.get(j)) {
                d += prev.distance(*p);
            }
            at.push(d);
        }
        (pts.len() >= 2 && d > 1e-9).then_some(Self { pts, at })
    }

    fn length(&self) -> f64 {
        self.at.last().copied().unwrap_or(0.0)
    }

    /// The point `d` along, and the unit normal to the left of travel (y down) there.
    fn sample(&self, d: f64) -> (Point, Vec2) {
        let d = d.clamp(0.0, self.length());
        // The segment holding `d` (skipping zero-length ones).
        let i = self.at.partition_point(|&a| a < d).clamp(1, self.pts.len() - 1);
        let mut j = i;
        while j + 1 < self.pts.len() && self.pts[j].distance(self.pts[j - 1]) < 1e-12 {
            j += 1;
        }
        let (a, b) = (self.pts[j - 1], self.pts[j]);
        let len = a.distance(b).max(1e-12);
        let u = ((d - self.at[j - 1]) / len).clamp(0.0, 1.0);
        let t = (b - a) / len;
        (a.lerp(b, u), Vec2::new(t.y, -t.x))
    }

    /// Positions about every `step` from the start to the end, evenly spread (the step is
    /// stretched or squeezed to fit a whole number into the length), each moved on by `offset`
    /// up to the end.
    fn steps(&self, step: f64, offset: f64) -> Vec<f64> {
        let len = self.length();
        let n = ((len / step).round() as usize).clamp(1, MAX_STEPS);
        let mut v: Vec<f64> = (0..=n).map(|k| (len * k as f64 / n as f64 + offset).min(len)).collect();
        v.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        v
    }
}

/// E-stitch (blanket stitch) along `path`.
pub fn e_stitch(path: &Polyline, s: &EStitchSettings) -> Vec<Stitch> {
    let Some(chain) = Chain::new(path) else { return vec![] };
    // On a closed shape, which side is inside: left of travel is outside when the ring runs
    // clockwise on screen (a positive shoelace area with y down).
    let side = if path.closed && signed_area(&chain.pts) > 0.0 { -1.0 } else { 1.0 };
    let mut at = chain.steps(s.spacing, 0.0);
    // A ring ends where it starts: one prong there, then the run closes the ring.
    if path.closed && at.len() > 2 {
        at.pop();
    }
    let mut out = vec![];
    for d in at {
        let (p, n) = chain.sample(d);
        out.push(Stitch::new(p, StitchKind::Top));
        out.push(Stitch::new(p + n * (s.width * side), StitchKind::Top));
        out.push(Stitch::new(p, StitchKind::Top));
    }
    if path.closed
        && let Some(first) = out.first().copied()
    {
        out.push(first);
    }
    out
}

/// Twice the signed area of a ring (shoelace), positive when it runs clockwise on screen.
fn signed_area(pts: &[Point]) -> f64 {
    pts.iter().zip(pts.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum()
}

/// Rope along `path`: `passes` passes of slanted stitches, alternating direction.
pub fn rope(path: &Polyline, s: &RopeSettings) -> Vec<Stitch> {
    let Some(chain) = Chain::new(path) else { return vec![] };
    let half = s.width / 2.0;
    let len = chain.length();
    let passes = s.passes.clamp(1, 3);
    let mut out = vec![];
    for k in 0..passes {
        let shift = s.spacing * f64::from(k) / f64::from(passes);
        let mut pass = vec![];
        for d in chain.steps(s.spacing, shift) {
            let (l, nl) = chain.sample(d);
            let (r, nr) = chain.sample((d + s.slant).min(len));
            pass.push(Stitch::new(l + nl * half, StitchKind::Top));
            pass.push(Stitch::new(r - nr * half, StitchKind::Top));
        }
        // Every other pass comes back the other way, so the needle ends where the next starts.
        if k % 2 == 1 {
            pass.reverse();
        }
        out.extend(pass);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(len: f64) -> Polyline {
        Polyline::new(vec![Point::new(0.0, 0.0), Point::new(len, 0.0)], false)
    }

    #[test]
    fn e_stitch_prongs_stick_out_to_the_left_every_spacing() {
        let st = e_stitch(&line(20.0), &EStitchSettings { width: 3.0, spacing: 2.0 });
        // 11 positions (0, 2, … 20), three penetrations each.
        assert_eq!(st.len(), 33);
        let prongs: Vec<&Stitch> = st.iter().filter(|p| p.y.abs() > 1e-9).collect();
        assert_eq!(prongs.len(), 11);
        // Travelling +x, the left is up (negative y).
        assert!(prongs.iter().all(|p| (p.y + 3.0).abs() < 1e-9));
        assert!((prongs[1].x - 2.0).abs() < 1e-9);
    }

    #[test]
    fn rope_slants_across_and_double_passes_come_back() {
        let s = RopeSettings { width: 2.0, spacing: 0.5, slant: 1.5, passes: 1 };
        let one = rope(&line(10.0), &s);
        // Left edge at y = -1, right edge at y = +1, each right end 1.5 mm further on.
        assert!((one[0].y + 1.0).abs() < 1e-9 && (one[1].y - 1.0).abs() < 1e-9);
        assert!((one[1].x - one[0].x - 1.5).abs() < 1e-9);
        let two = rope(&line(10.0), &RopeSettings { passes: 2, ..s });
        assert!(two.len() > one.len() * 2 - 4);
        // The second pass ends back near the start.
        assert!(two.last().unwrap().x < 2.0);
        // Degenerate paths sew nothing rather than failing.
        assert!(rope(&Polyline::new(vec![Point::new(1.0, 1.0)], false), &s).is_empty());
        assert!(e_stitch(&Polyline::new(vec![Point::new(1.0, 1.0), Point::new(1.0, 1.0)], false), &EStitchSettings::default()).is_empty());
    }

    #[test]
    fn e_stitch_prongs_point_into_closed_shapes_either_way_round() {
        let sq = [Point::new(0.0, 0.0), Point::new(20.0, 0.0), Point::new(20.0, 20.0), Point::new(0.0, 20.0)];
        let inside = |st: &[Stitch]| st.iter().all(|p| p.x > -1e-9 && p.x < 20.0 + 1e-9 && p.y > -1e-9 && p.y < 20.0 + 1e-9);
        let s = EStitchSettings::default();
        assert!(inside(&e_stitch(&Polyline::new(sq.to_vec(), true), &s)));
        let st = e_stitch(&Polyline::new(sq.iter().rev().copied().collect(), true), &s);
        assert!(inside(&st));
        // One prong at the seam, and the ring closes where it started.
        let start = st[0];
        assert_eq!(st.iter().filter(|p| p.x == start.x && p.y == start.y).count(), 3);
        assert_eq!(st.last(), Some(&start));
    }
}
