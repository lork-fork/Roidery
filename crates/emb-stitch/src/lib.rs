//! Roidery stitch generators.
//!
//! Everything here works in millimetres with y down (the canvas's orientation) and knows nothing
//! about the document or the interface: shapes and [`StitchSettings`] go in, an ordered list of
//! [`Stitch`]es comes out. The engine converts document paths to millimetres, calls these, and
//! hands the result to the stitch plan.
//!
//! Stitch types (the names follow Ember's stitch library):
//! - [`run`]: single and triple (bean) running stitch along a path.
//! - [`satin`]: Satin Blocks (pairs of points across a column, as Ember's tool draws them) and a
//!   satin of fixed width along a path, both through one generator, with centre-walk, contour
//!   and zig-zag underlay and pull compensation.
//! - [`contour`]: contour fill, rows that follow the outline inwards.
//! - [`fill::gradient`]: gradient fill, tatami rows spreading out so the colour fades.
//! - [`circular`]: circular fill, rows that are circles round a centre.
//! - [`guided`]: guided fill, tatami rows that follow a curve drawn across the shape.
//! - [`motif`]: motif fill, rows of a small repeating pattern in running stitch.
//! - [`edge`]: the E-stitch (blanket stitch) for appliqué edges, and single, double and triple
//!   rope.
//! - [`fill`]: tatami fill with angle, row spacing, staggered penetrations and underlay.
//! - [`lettering`]: text as satin columns in the built-in centre-line font, Roidery Sans.
//! - [`route`]: automatic start points, so each object starts near where the needle is.
//! - [`fabric`]: fabric presets (density, pull compensation and underlay for each fabric).
//!
//! Algorithms follow the published behaviour of stitchjs (MIT, Matt Jacobson; the library behind
//! Ember) and common digitizing practice. No GPL code is used.
#![forbid(unsafe_code)]

pub mod circular;
pub mod contour;
pub mod edge;
pub mod fabric;
pub mod fill;
pub mod geometry;
pub mod guided;
pub mod lettering;
pub mod medial;
pub mod motif;
pub mod route;
pub mod run;
pub mod satin;
pub mod settings;

pub use fabric::{FABRICS, Fabric};
pub use geometry::{Polyline, polylines_from_bezpath};
pub use kurbo::Point;
pub use lettering::Lettering;
pub use route::generate_from;
pub use settings::{EStitchSettings, FillSettings, GuidedSettings, RopeSettings, RunSettings, SatinSettings, StitchSettings, StitchType, Underlay};

use serde::{Deserialize, Serialize};

/// What a stitch is for. Only the plan and the preview care: every kind is a needle penetration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StitchKind {
    /// The visible top stitching.
    Top,
    /// Underlay laid down first to stabilise the fabric.
    Underlay,
    /// A running stitch that moves the needle inside the object (covered later).
    Travel,
    /// The needle moves here without sewing, to start a separate part of the object. The plan
    /// turns it into a jump, or a trim and a jump when it is long.
    Jump,
}

/// One needle penetration, in millimetres, y down.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stitch {
    pub x: f64,
    pub y: f64,
    pub kind: StitchKind,
}

impl Stitch {
    pub fn new(p: Point, kind: StitchKind) -> Self {
        Self { x: p.x, y: p.y, kind }
    }
    pub fn point(&self) -> Point {
        Point::new(self.x, self.y)
    }
}

/// The geometry a stitch type works on.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// One or more paths (runs, satins along a path, manual stitches).
    Paths(Vec<Polyline>),
    /// Closed areas (fills): outer rings and holes together, filled by the even-odd rule.
    Area(Vec<Polyline>),
    /// Satin Blocks: the left and right rails, with point `i` of each forming rung `i`.
    Rails { left: Vec<Point>, right: Vec<Point> },
    /// Satin Blocks whose rails curve: `left[k]` and `right[k]` are each rail from rung `k` to
    /// rung `k + 1`, flattened.
    RailCurves { left: Vec<Vec<Point>>, right: Vec<Vec<Point>> },
    /// Paths with a width profile, (t, left, right) points along each path's length: a satin
    /// sews as a Satin Column that widens and narrows as the profile does; other types sew the
    /// paths as they would without it.
    Column { paths: Vec<Polyline>, profile: Vec<(f64, f64, f64)> },
}

/// Separate parts of one object (the subpaths of a path) in sewing order: the needle jumps to
/// the start of each part after the first rather than sewing across to it.
fn parts(parts: impl Iterator<Item = Vec<Stitch>>) -> Vec<Stitch> {
    let mut out: Vec<Stitch> = vec![];
    for mut part in parts.filter(|p| !p.is_empty()) {
        if !out.is_empty()
            && let Some(first) = part.first_mut()
        {
            first.kind = StitchKind::Jump;
        }
        out.extend(part);
    }
    out
}

/// Curved rail pieces as one rail (each piece starts where the last ended).
fn joined(pieces: &[Vec<Point>]) -> Vec<Point> {
    let mut out: Vec<Point> = vec![];
    for piece in pieces {
        let skip = usize::from(out.last().zip(piece.first()).is_some_and(|(a, b)| a.distance(*b) < 1e-9));
        out.extend(piece.iter().skip(skip));
    }
    out
}

/// Hard cap on stitches per object, so a huge shape or a tiny spacing can't run away.
pub const MAX_STITCHES: usize = 2_000_000;

/// Generate the stitches for one object.
pub fn generate(shape: &Shape, settings: &StitchSettings) -> Vec<Stitch> {
    let settings = &settings.sanitized();
    let mut out = match (settings.kind, shape) {
        (StitchType::None, _) => vec![],
        (StitchType::Satin | StitchType::SatinBlocks, Shape::Column { paths, profile }) => {
            parts(paths.iter().map(|p| satin::along_path_profiled(p, &settings.satin, profile)))
        }
        // Without a satin to shape, the profile changes nothing.
        (_, Shape::Column { paths, .. }) if settings.kind.is_fill() => generate(&Shape::Area(paths.clone()), settings),
        (_, Shape::Column { paths, .. }) => generate(&Shape::Paths(paths.clone()), settings),
        (StitchType::Run, Shape::Paths(ps) | Shape::Area(ps)) => parts(ps.iter().map(|p| run::single(p, &settings.run))),
        (StitchType::Triple, Shape::Paths(ps) | Shape::Area(ps)) => parts(ps.iter().map(|p| run::triple(p, &settings.run))),
        (StitchType::Manual, Shape::Paths(ps) | Shape::Area(ps)) => {
            parts(ps.iter().map(|p| p.points.iter().map(|q| Stitch::new(*q, StitchKind::Top)).collect()))
        }
        (StitchType::Satin, Shape::Paths(ps) | Shape::Area(ps)) => parts(ps.iter().map(|p| satin::along_path(p, &settings.satin))),
        (StitchType::Satin | StitchType::SatinBlocks, Shape::Rails { left, right }) => satin::blocks(left, right, &settings.satin),
        (StitchType::Satin | StitchType::SatinBlocks, Shape::RailCurves { left, right }) => satin::blocks_curved(left, right, &settings.satin),
        (StitchType::SatinBlocks, Shape::Paths(ps) | Shape::Area(ps)) => match ps.as_slice() {
            [l, r, ..] => satin::blocks(&l.points, &r.points, &settings.satin),
            [p] => satin::along_path(p, &settings.satin),
            [] => vec![],
        },
        (StitchType::Tatami, Shape::Area(ps) | Shape::Paths(ps)) => fill::tatami(ps, &settings.fill),
        (StitchType::Gradient, Shape::Area(ps) | Shape::Paths(ps)) => fill::gradient(ps, &settings.fill),
        (StitchType::Contour, Shape::Area(ps) | Shape::Paths(ps)) => contour::contour(ps, &settings.fill),
        (StitchType::Circular, Shape::Area(ps) | Shape::Paths(ps)) => circular::circular(ps, &settings.fill, &settings.circular),
        (StitchType::Motif, Shape::Area(ps) | Shape::Paths(ps)) => motif::motif(ps, &settings.motif),
        (StitchType::Guided, Shape::Area(ps) | Shape::Paths(ps)) => guided::guided(ps, &settings.fill, &settings.guided),
        (StitchType::EStitch, Shape::Paths(ps) | Shape::Area(ps)) => parts(ps.iter().map(|p| edge::e_stitch(p, &settings.e_stitch))),
        (StitchType::Rope, Shape::Paths(ps) | Shape::Area(ps)) => parts(ps.iter().map(|p| edge::rope(p, &settings.rope))),
        (
            StitchType::Run
            | StitchType::Triple
            | StitchType::Manual
            | StitchType::Tatami
            | StitchType::Contour
            | StitchType::Circular
            | StitchType::Gradient
            | StitchType::Motif
            | StitchType::Guided
            | StitchType::EStitch
            | StitchType::Rope,
            Shape::Rails { .. } | Shape::RailCurves { .. },
        ) => {
            // A rails object asked for a run or fill: treat the outline as a closed path.
            let (left, right) = match shape {
                Shape::RailCurves { left, right } => (joined(left), joined(right)),
                Shape::Rails { left, right } => (left.clone(), right.clone()),
                _ => (vec![], vec![]),
            };
            let mut ring = left;
            ring.extend(right.iter().rev());
            let ring = Polyline { points: ring, closed: true };
            generate(&Shape::Paths(vec![ring]), settings)
        }
    };
    out.retain(|s| s.x.is_finite() && s.y.is_finite());
    out.truncate(MAX_STITCHES);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(size: f64) -> Polyline {
        Polyline { points: vec![Point::new(0.0, 0.0), Point::new(size, 0.0), Point::new(size, size), Point::new(0.0, size)], closed: true }
    }

    #[test]
    fn separate_subpaths_are_joined_by_jumps() {
        let a = Polyline { points: vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)], closed: false };
        let b = Polyline { points: vec![Point::new(0.0, 20.0), Point::new(10.0, 20.0)], closed: false };
        for kind in [StitchType::Run, StitchType::Triple, StitchType::Satin, StitchType::Manual] {
            let st = generate(&Shape::Paths(vec![a.clone(), b.clone()]), &StitchSettings::of(kind));
            let jumps: Vec<_> = st.iter().filter(|s| s.kind == StitchKind::Jump).collect();
            assert_eq!(jumps.len(), 1, "{kind:?}");
            assert!(jumps[0].y > 15.0, "{kind:?}: the jump goes to the second path");
        }
    }

    #[test]
    fn every_type_produces_finite_stitches() {
        let shapes = [
            Shape::Paths(vec![square(20.0)]),
            Shape::Area(vec![square(20.0)]),
            Shape::Rails { left: vec![Point::new(0.0, 0.0), Point::new(20.0, 0.0)], right: vec![Point::new(0.0, 4.0), Point::new(20.0, 4.0)] },
        ];
        for kind in StitchType::ALL {
            for shape in &shapes {
                let s = StitchSettings { kind, ..Default::default() };
                let st = generate(shape, &s);
                assert!(st.iter().all(|s| s.x.is_finite() && s.y.is_finite()), "{kind:?}");
                if kind != StitchType::None {
                    assert!(!st.is_empty(), "{kind:?} on {shape:?}");
                }
            }
        }
    }
}
