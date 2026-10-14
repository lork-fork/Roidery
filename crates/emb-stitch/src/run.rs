//! Running stitches: single and triple (bean).

use crate::geometry::{Polyline, resample};
use crate::settings::RunSettings;
use crate::{Stitch, StitchKind};

/// Turns sharper than this keep a penetration exactly on the corner.
pub const CORNER_DEG: f64 = 25.0;

/// One pass along the path, a penetration about every `length` mm and on every sharp corner.
pub fn single(path: &Polyline, s: &RunSettings) -> Vec<Stitch> {
    resample(&path.vertices(), s.length, CORNER_DEG).into_iter().map(|p| Stitch::new(p, StitchKind::Top)).collect()
}

/// Bean stitch: every stitch of a single run sewn forward, back and forward again, so the line
/// is three threads thick.
pub fn triple(path: &Polyline, s: &RunSettings) -> Vec<Stitch> {
    let base = single(path, s);
    let Some(first) = base.first().copied() else { return vec![] };
    let mut out = vec![first];
    for w in base.windows(2) {
        out.extend([w[1], w[0], w[1]]);
    }
    out
}

/// A running stitch along a chain of points, kind as given (used for underlay and travel).
pub fn along(pts: &[kurbo::Point], length: f64, kind: StitchKind) -> Vec<Stitch> {
    resample(pts, length, CORNER_DEG).into_iter().map(|p| Stitch::new(p, kind)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Point;

    #[test]
    fn single_spaces_stitches_and_closes_rings() {
        let ring = Polyline::new(vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0), Point::new(0.0, 10.0)], true);
        let st = single(&ring, &RunSettings::default());
        assert_eq!(st.first().map(|s| s.point()), st.last().map(|s| s.point()));
        for w in st.windows(2) {
            let d = w[0].point().distance(w[1].point());
            assert!(d > 2.0 && d < 3.0, "{d}");
        }
    }

    #[test]
    fn triple_has_three_threads_per_stitch() {
        let line = Polyline::new(vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)], false);
        let one = single(&line, &RunSettings::default()).len();
        assert_eq!(triple(&line, &RunSettings::default()).len(), 1 + 3 * (one - 1));
    }
}
