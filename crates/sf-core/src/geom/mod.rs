//! 2D geometry in design space.
//!
//! Units are **millimetres**, +x to the right and **+y downwards** (the same
//! orientation as SVG and screen space). Machine file formats convert to their own
//! conventions (usually 0.1 mm units, often y-up) when reading/writing.

mod affine;
mod bbox;
mod point;
pub mod poly;
pub mod polyline;
pub mod shape;

pub use affine::Affine;
pub use bbox::BBox;
pub use point::{Point, screen_angle, screen_dir};
pub use poly::Polygon;
pub use shape::{Column, Contour, FLATTEN_TOLERANCE, Node, NodeKind, Rung, Shape};
