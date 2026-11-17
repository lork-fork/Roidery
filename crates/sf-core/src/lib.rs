//! # StitchFoundry core
//!
//! Vendored into Roidery from <https://github.com/StitchFoundry/StitchFoundry> (see `VENDORED.md`).
//!
//! Geometry, the object-based embroidery design model, the stitch generation
//! engine and machine-level stitch patterns.
//!
//! * [`geom`] – points, transforms, splines, polygons with boolean/offset ops.
//! * [`model`] – [`Design`] made of editable [`EmbObject`]s (run, satin, fill,
//!   appliqué, lettering, raw stitches).
//! * [`engine`] – object → stitches, design → machine [`Pattern`].
//! * [`direction`] – inclination lines, turning strips, automatic stitch direction.
//! * [`brush`] – freehand stroke fitting, variable-width brush columns, preset shapes.
//! * [`attr`] – embroidery settings attached to objects of a host vector document.
//! * [`pattern`] – flat stitch/jump/trim/colour-change records.
//! * [`threads`], [`hoops`], [`fabrics`] – reference data.
//! * [`stats`] – stitch counts, thread usage, sewing time.
//! * [`io`] – native `.sfd` design files.

#![forbid(unsafe_code)]
pub mod attr;
pub mod barcode;
pub mod brush;
pub mod decor;
pub mod density;
pub mod direction;
pub mod engine;
pub mod fabrics;
pub mod geom;
pub mod hoops;
pub mod io;
pub mod machines;
pub mod model;
pub mod pattern;
pub mod quilt;
pub mod satin_line;
pub mod sequence;
pub mod stamps;
pub mod stats;
pub mod threads;
pub mod travel;

pub use geom::{Affine, BBox, Point};
pub use model::*;
pub use pattern::{Cmd, ColorBlock, Pattern, Stitch};
pub use threads::{Rgb, Thread};
