//! Roidery: digitized embroidery fonts.
//!
//! An embroidery font is lettering a person digitized stitch by stitch: each letter is satin
//! columns, fills and runs with their own direction, density and underlay, so it sews cleanly
//! at small sizes where a computer font's outline filled with tatami would not. Roidery reads
//! fonts in Ink/Stitch's format (a folder with `font.json` and an SVG with one layer per glyph),
//! so the Ink/Stitch font library and any font made for Ink/Stitch work as they are.
//!
//! - [`svg`] reads a font's glyph layers and their `inkstitch:*` parameters;
//! - [`convert`] turns each element into what Roidery sews (Satin Blocks, tatami, runs);
//! - [`font`] loads a font folder; [`library`] finds the folders and keeps what it loaded;
//! - [`layout`] sets text in a font at a size, as objects in mm.
//!
//! The format is read from its published files; no Ink/Stitch code is used (Ink/Stitch is GPL).

pub mod convert;
pub mod font;
pub mod info;
pub mod layout;
pub mod library;
pub mod svg;

pub use font::{Font, Summary};
pub use info::FontInfo;
pub use layout::{Placed, PlacedPiece, layout};
