//! Embroidery settings attached to the objects of a host vector document.
//!
//! In a vector editor every path, compound path or text object can carry an
//! [`Embroidery`] record saying how it is stitched; the stitch plan is generated
//! from the artwork itself (the way Ink/Stitch works inside an SVG editor).
//!
//! **Units:** positions stored here (inclination lines, start/stop points) are in
//! the *host document's* coordinates (VectorCraft: points, y down), so they move
//! with the object when the host transforms it. Stitch parameters (spacing,
//! lengths, widths) are physical millimetres.

use std::collections::BTreeMap;

use crate::brush::WidthProfile;
use crate::direction::Guide;
use crate::fabrics::Fabric;
use crate::geom::{Affine, Point};
use crate::hoops::Hoop;
use crate::model::{AppliqueParams, CustomMotif, DesignSettings, FillKind, FillParams, RunKind, RunParams, SatinParams, transform_angle};
use crate::pattern::Stitch;
use crate::threads::Thread;
use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_constant(p: &WidthProfile) -> bool {
    *p == WidthProfile::Constant
}

/// How an object is stitched.
// One per stitched object, cloned with it; boxing the fill parameters buys nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StitchType {
    /// Running stitch along the outline or stroke (single, triple, blanket, ...).
    Run(RunParams),
    /// Satin: along an open path (with `width`/`profile`), between two rails of a
    /// compound path, or across a closed shape following its inclination lines.
    Satin(SatinParams),
    /// Area fill of a closed shape (tatami, satin fill, contour, meander, ...).
    Fill(FillParams),
    /// Appliqué placement, tack-down and cover around a closed shape.
    Applique(AppliqueParams),
    /// Every path vertex is a needle penetration (imported or hand-placed stitches).
    Manual,
    /// A group whose stitched members are sewn as one path (branching): ordered for
    /// the shortest travel, with the thread carried between them as running stitches
    /// where later members cover it. The group itself sews nothing.
    Branch,
    /// Redwork: the lines of a path, compound path, type or group (all its art) sewn as one
    /// line that starts and ends at the same point, every line once in the run style, the way
    /// between them retraced along lines already sewn. In a group, the art's own stitching is
    /// left out.
    Redwork(RunParams),
    /// Lettering in an embroidery font: a group of glyphs, each a group of stitched art, that
    /// remembers what it says, in what font and how big, so that can change where the lettering
    /// is now. The group itself sews nothing.
    Lettering(Box<Lettering>),
    /// Multi-decoration: sequins, beads, rhinestones, cord or boring holes placed along the
    /// line, inside the shape, round its edge or at its points (phase 4).
    Decoration(crate::decor::DecorParams),
}

impl StitchType {
    /// The stitch type a stitch type menu entry (an `embroidery.set` type) makes, with
    /// default parameters; aliases (`single`, `triple`, `back`, `stem`, `steil`,
    /// `tatami`) too. `None` for an unknown name (and for `none`, no stitches).
    pub fn from_name(name: &str) -> Option<StitchType> {
        let fill = |kind: FillKind| StitchType::Fill(FillParams { kind, ..FillParams::default() });
        let run = |kind: RunKind| StitchType::Run(RunParams { kind, ..RunParams::default() });
        Some(match name {
            "run" | "single" => run(RunKind::Single),
            "bean" | "triple" => run(RunKind::Triple),
            "blanket" => run(RunKind::Blanket),
            "backstitch" | "back" => run(RunKind::Back),
            "stemstitch" | "stem" => run(RunKind::Stem),
            "zigzag" => StitchType::Run(RunParams { kind: RunKind::Zigzag, spacing: 0.8, ..RunParams::default() }),
            "motif" => StitchType::Run(RunParams { kind: RunKind::Motif, width: 3.0, spacing: 4.0, ..RunParams::default() }),
            "satin" | "steil" => StitchType::Satin(SatinParams::default()),
            "fill" | "tatami" => fill(FillKind::Tatami),
            "satinfill" => StitchType::Fill(FillParams::satin()),
            "contour" => StitchType::Fill(FillParams { kind: FillKind::Contour, spacing: 0.6, ..FillParams::default() }),
            "spiral" => StitchType::Fill(FillParams { kind: FillKind::Spiral, spacing: 0.6, ..FillParams::default() }),
            "radial" => StitchType::Fill(FillParams { kind: FillKind::Radial, spacing: 0.8, ..FillParams::default() }),
            "cross" => StitchType::Fill(FillParams { kind: FillKind::Cross, stitch_length: 2.5, ..FillParams::default() }),
            "meander" => StitchType::Fill(FillParams { kind: FillKind::Meander, spacing: 2.0, stitch_length: 2.5, ..FillParams::default() }),
            "motiffill" => StitchType::Fill(FillParams { kind: FillKind::Motif, spacing: 5.0, stitch_length: 2.0, ..FillParams::default() }),
            "applique" => StitchType::Applique(AppliqueParams::default()),
            "manual" => StitchType::Manual,
            "redwork" => StitchType::Redwork(RunParams::default()),
            "sequin" | "sequins" => StitchType::Decoration(crate::decor::DecorParams::of(crate::decor::DecorKind::Sequin)),
            "bead" | "beads" => StitchType::Decoration(crate::decor::DecorParams::of(crate::decor::DecorKind::Bead)),
            "stone" | "stones" | "rhinestone" | "rhinestones" | "bling" => {
                StitchType::Decoration(crate::decor::DecorParams::of(crate::decor::DecorKind::Stone))
            }
            "cord" | "couching" => StitchType::Decoration(crate::decor::DecorParams::of(crate::decor::DecorKind::Cord)),
            "boring" => StitchType::Decoration(crate::decor::DecorParams::of(crate::decor::DecorKind::Boring)),
            "print" => StitchType::Decoration(crate::decor::DecorParams::of(crate::decor::DecorKind::Print)),
            "foil" => StitchType::Decoration(crate::decor::DecorParams::of(crate::decor::DecorKind::Foil)),
            _ => return None,
        })
    }

    pub fn label(&self) -> &'static str {
        match self {
            StitchType::Run(r) => r.kind.label(),
            StitchType::Satin(_) => "Satin",
            StitchType::Fill(f) => f.kind.label(),
            StitchType::Applique(_) => "Appliqué",
            StitchType::Manual => "Manual stitches",
            StitchType::Branch => "Branch",
            StitchType::Redwork(_) => "Redwork",
            StitchType::Lettering(_) => "Lettering",
            StitchType::Decoration(d) => d.kind.label(),
        }
    }

    /// The stitch type menu entry (the `embroidery.set` type) this is: `run`, `bean`,
    /// `blanket`, `backstitch`, `stemstitch`, `satin`, `fill`, `satinfill`, `contour`,
    /// `spiral`, `radial`, `cross`, `meander`, `motiffill`, `applique`, `manual`, `branch` or
    /// `redwork`. Zigzag and motif runs are runs (styles of the Run entry).
    pub fn type_name(&self) -> &'static str {
        match self {
            StitchType::Run(r) => match r.kind {
                RunKind::Triple => "bean",
                RunKind::Blanket => "blanket",
                RunKind::Back => "backstitch",
                RunKind::Stem => "stemstitch",
                RunKind::Single | RunKind::Zigzag | RunKind::Motif | RunKind::Hand | RunKind::Sculpture | RunKind::String => "run",
            },
            StitchType::Satin(_) => "satin",
            StitchType::Fill(f) => match f.kind {
                FillKind::Tatami => "fill",
                FillKind::Satin => "satinfill",
                FillKind::Contour => "contour",
                FillKind::Spiral => "spiral",
                FillKind::Radial => "radial",
                FillKind::Cross => "cross",
                FillKind::Meander => "meander",
                FillKind::Motif => "motiffill",
            },
            StitchType::Applique(_) => "applique",
            StitchType::Manual => "manual",
            StitchType::Branch => "branch",
            StitchType::Redwork(_) => "redwork",
            StitchType::Lettering(_) => "lettering",
            StitchType::Decoration(d) => d.kind.id(),
        }
    }

    /// Short machine-readable name (`run`, `satin`, `fill`, `applique`, `manual`,
    /// `branch`, `redwork`).
    pub fn id(&self) -> &'static str {
        match self {
            StitchType::Run(_) => "run",
            StitchType::Satin(_) => "satin",
            StitchType::Fill(_) => "fill",
            StitchType::Applique(_) => "applique",
            StitchType::Manual => "manual",
            StitchType::Branch => "branch",
            StitchType::Redwork(_) => "redwork",
            StitchType::Lettering(_) => "lettering",
            StitchType::Decoration(_) => "decoration",
        }
    }
}

/// Lettering set in an embroidery font ([`StitchType::Lettering`]): what it says, in what and how
/// big, and where, so it can be set again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lettering {
    pub text: String,
    /// The font, by name.
    pub font: String,
    /// The capitals' height (mm).
    pub size: f64,
    /// Each line's own capitals' height (mm), by line; a line past the end (or with 0) has `size`.
    #[serde(default, rename = "lineSizes", skip_serializing_if = "Vec::is_empty")]
    pub line_sizes: Vec<f64>,
    /// Lines apart, × the font's own line spacing.
    #[serde(default = "one")]
    pub leading: f64,
    /// The baseline bent round a circle this big (radius, mm): positive, arched over its top;
    /// negative, along its bottom; 0, straight.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub arc: f64,
    /// With an arc, the circle's radius shrinks this much a turn along the line (mm; negative,
    /// grows): the baseline an Archimedean spiral winding in (out).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub spiral: f64,
    /// Where the lines sit on the origin: their start, middle or end.
    #[serde(default, skip_serializing_if = "is_left")]
    pub align: LetterAlign,
    /// The order its letters are sewn in, how they join and where the thread is cut.
    #[serde(default, skip_serializing_if = "LetterSewing::is_plain")]
    pub sew: LetterSewing,
    /// Letters moved, scaled or turned from their places, kept when it is set again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub letters: Vec<LetterAdjust>,
    /// 3D puff: its glyphs' satin sewn over foam.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub puff: Option<crate::model::Puff>,
    /// Appliqué letters in fabric of this colour: each glyph one patch the shape of what it sews
    /// (placement line, tack-down and cover), instead of its stitches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applique: Option<crate::threads::Rgb>,
    /// How its letters are stitched: as the font has them, or all satin or all fill.
    #[serde(default, skip_serializing_if = "LetterStitch::is_font")]
    pub stitch: LetterStitch,
    /// Small-text settings: shorter stitches, lighter pull compensation, a light underlay.
    #[serde(default, skip_serializing_if = "SmallText::is_auto")]
    pub small: SmallText,
    /// Letters one under another, each centred; lines side by side (straight lettering only).
    #[serde(default, skip_serializing_if = "is_false")]
    pub vertical: bool,
    /// A baseline drawn as a line (points, the lettering's own space: from its origin): the
    /// letters along it, upright to it. Empty: straight or round `arc`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<crate::geom::Point>,
    /// From the lettering's own space (points: the first line's baseline runs from (0, 0) along
    /// x, the lines follow down y) to the document. It follows the lettering as it moves, turns
    /// and slants; a scale goes into `size`.
    pub frame: Affine,
}

fn one() -> f64 {
    1.0
}

/// How lettering's letters are stitched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LetterStitch {
    /// As the font digitizes them.
    #[default]
    Font,
    /// Satin: filled parts sewn as satin across them.
    Satin,
    /// Each letter one tatami fill, the shape of what it sews.
    Fill,
}

impl LetterStitch {
    pub fn is_font(&self) -> bool {
        *self == LetterStitch::Font
    }

    pub fn from_name(name: &str) -> Option<LetterStitch> {
        match name {
            "font" => Some(LetterStitch::Font),
            "satin" => Some(LetterStitch::Satin),
            "fill" => Some(LetterStitch::Fill),
            _ => None,
        }
    }
}

/// Small-text settings for lettering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmallText {
    /// On below [`SmallText::BELOW`].
    #[default]
    Auto,
    On,
    Off,
}

impl SmallText {
    /// Capitals lower than this (mm) are small text.
    pub const BELOW: f64 = 6.0;

    pub fn is_auto(&self) -> bool {
        *self == SmallText::Auto
    }

    pub fn from_name(name: &str) -> Option<SmallText> {
        match name {
            "auto" => Some(SmallText::Auto),
            "on" => Some(SmallText::On),
            "off" => Some(SmallText::Off),
            _ => None,
        }
    }

    /// Whether lettering `size` mm high is sewn as small text.
    pub fn applies(self, size: f64) -> bool {
        match self {
            SmallText::Auto => size < SmallText::BELOW,
            SmallText::On => true,
            SmallText::Off => false,
        }
    }
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_left(a: &LetterAlign) -> bool {
    *a == LetterAlign::Left
}

/// Where lettering's lines sit on its origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LetterAlign {
    #[default]
    Left,
    #[serde(alias = "center")]
    Centre,
    Right,
}

impl LetterAlign {
    /// The share of a line's width before the origin.
    pub fn share(self) -> f64 {
        match self {
            LetterAlign::Left => 0.0,
            LetterAlign::Centre => 0.5,
            LetterAlign::Right => 1.0,
        }
    }

    /// `left`, `centre` (or `center`) or `right`.
    pub fn from_name(name: &str) -> Option<LetterAlign> {
        match name.to_ascii_lowercase().as_str() {
            "left" => Some(LetterAlign::Left),
            "centre" | "center" => Some(LetterAlign::Centre),
            "right" => Some(LetterAlign::Right),
            _ => None,
        }
    }
}

/// The order lettering's letters are sewn in along each line (public documentation of the
/// practice: https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Lettering/lettering_advanced/lettering_advanced-15.htm).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LetterOrder {
    /// Left to right.
    #[default]
    Left,
    /// Right to left.
    Right,
    /// From the middle letter out: back to the first, then on from the middle to the last (on
    /// caps, the middle held best).
    #[serde(alias = "center")]
    Centre,
}

impl LetterOrder {
    /// `left`, `right` or `centre` (or `center`).
    pub fn from_name(name: &str) -> Option<LetterOrder> {
        match name.to_ascii_lowercase().as_str() {
            "left" => Some(LetterOrder::Left),
            "right" => Some(LetterOrder::Right),
            "centre" | "center" => Some(LetterOrder::Centre),
            _ => None,
        }
    }
}

/// How one letter joins the next (public documentation of the practice:
/// https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Lettering/lettering_advanced/lettering_advanced-16.htm).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LetterJoin {
    /// Each letter starts where it is closest to where the last one stopped.
    #[default]
    Closest,
    /// Each letter starts at its baseline's start and stops at its end, so the thread runs on
    /// along the baseline (hidden in a towel's pile).
    Baseline,
}

impl LetterJoin {
    /// `closest` or `baseline`.
    pub fn from_name(name: &str) -> Option<LetterJoin> {
        match name.to_ascii_lowercase().as_str() {
            "closest" => Some(LetterJoin::Closest),
            "baseline" => Some(LetterJoin::Baseline),
            _ => None,
        }
    }
}

/// Where lettering's thread is cut.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LetterTrim {
    /// Where the machine's jumps need it.
    #[default]
    Auto,
    /// After each letter.
    Letter,
    /// After each word.
    Word,
    /// After each line.
    Line,
}

impl LetterTrim {
    /// `auto`, `letter`, `word` or `line`.
    pub fn from_name(name: &str) -> Option<LetterTrim> {
        match name.to_ascii_lowercase().as_str() {
            "auto" => Some(LetterTrim::Auto),
            "letter" => Some(LetterTrim::Letter),
            "word" => Some(LetterTrim::Word),
            "line" => Some(LetterTrim::Line),
            _ => None,
        }
    }
}

/// One letter of lettering moved, scaled or turned from its place.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LetterAdjust {
    /// The letter: its place among the lettering's letters as the text sets them (from 0).
    pub index: usize,
    /// Moved along its line (mm).
    #[serde(default)]
    pub dx: f64,
    /// Moved down from its line (mm).
    #[serde(default)]
    pub dy: f64,
    /// Scaled about the middle of its baseline.
    #[serde(default = "one")]
    pub scale: f64,
    /// Turned about the middle of its baseline (degrees, clockwise).
    #[serde(default)]
    pub angle: f64,
}

impl LetterAdjust {
    /// Letter `index` where it is set.
    pub fn none(index: usize) -> LetterAdjust {
        LetterAdjust { index, dx: 0.0, dy: 0.0, scale: 1.0, angle: 0.0 }
    }

    /// Nothing changed.
    pub fn is_plain(&self) -> bool {
        *self == LetterAdjust::none(self.index)
    }
}

/// How lettering is sewn: its letters' order, their joins, its trims.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LetterSewing {
    #[serde(default)]
    pub order: LetterOrder,
    /// The lines from the last up (else from the first down).
    #[serde(default)]
    pub up: bool,
    #[serde(default)]
    pub join: LetterJoin,
    #[serde(default)]
    pub trim: LetterTrim,
}

impl LetterSewing {
    /// As lettering is sewn when nothing is said.
    pub fn is_plain(&self) -> bool {
        *self == LetterSewing::default()
    }
}

impl Lettering {
    /// Follows the lettering through `m`: the frame takes it, but for its scale, which `size`
    /// takes (the glyphs' places grow with the size, so that is the same).
    pub fn transform(&mut self, m: &Affine) {
        let f = self.frame.then(m);
        let k = f.determinant().abs().sqrt();
        if !(k.is_finite() && k > 1e-9 && f.0.iter().all(|v| v.is_finite())) {
            return;
        }
        let [a, b, c, d, e, g] = f.0;
        self.frame = Affine([a / k, b / k, c / k, d / k, e, g]);
        self.size *= k;
        // a line's own size grows with the rest (0 stays "the block's size")
        for s in &mut self.line_sizes {
            *s *= k;
        }
        self.arc *= k;
        self.spiral *= k;
        for q in &mut self.path {
            (q.x, q.y) = (q.x * k, q.y * k);
        }
        for l in &mut self.letters {
            l.dx *= k;
            l.dy *= k;
        }
    }
}

/// Embroidery settings of one object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Embroidery {
    pub stitch: StitchType,
    /// Thread; `None` uses the object's fill colour (strokes: stroke colour).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<Thread>,
    /// Satin width along an open path (mm); `None` uses the stroke width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// Width profile of satin along an open path (tapered, calligraphy, pressure).
    #[serde(default, skip_serializing_if = "is_constant")]
    pub profile: WidthProfile,
    /// Inclination lines (host coordinates): one sets the stitch angle, two or
    /// more make the stitches turn along the shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<Guide>,
    /// Split lines (host coordinates; tatami fills): each stitch row has a needle point where
    /// it crosses one, so the lines show in the fill (drawn splits, carved stamps, flexi
    /// splits).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub splits: Vec<Vec<Point>>,
    /// Flow lines (host coordinates; tatami fills): the rows follow them, copying one line or
    /// changing shape from the first line to the second.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flow: Vec<Vec<Point>>,
    /// Pick the stitch direction automatically while there are no guides.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub auto_direction: bool,
    /// Start bead (host coordinates); `None` = closest join.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<Point>,
    /// Stop bead (host coordinates).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<Point>,
    /// Always trim after this object.
    #[serde(default, skip_serializing_if = "is_false")]
    pub trim_after: bool,
    /// Lock stitches at the start: `None` follows the design setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tie_in: Option<bool>,
    /// Lock stitches before the thread is cut after this object: `None` follows
    /// the design setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tie_off: Option<bool>,
    /// Remove underlying stitching: fills sewn before this object leave out what
    /// it covers, except a margin under its edge (see [`Embroidery::cut_overlap`]).
    #[serde(default, skip_serializing_if = "is_false")]
    pub remove_underlying: bool,
    /// How far (mm) the fills below still run in under this object's edge when
    /// they are cut; `None` = [`UNDERLYING_OVERLAP`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underlying_overlap: Option<f64>,
    /// Stop the machine after this object (to place appliqué fabric, or change
    /// thread by hand on a single-needle machine).
    #[serde(default, skip_serializing_if = "is_false")]
    pub stop_after: bool,
    /// Start a colour block of its own, a colour change before it even in the colour sewn just
    /// before (split colour blocks: the same colour sewn as two, a stop between).
    #[serde(default, skip_serializing_if = "is_false")]
    pub own_block: bool,
    /// Leave out the last needle point (Omit last stitch): one stitch less where the
    /// object's last stitch would sew back over its edge or into the next object.
    #[serde(default, skip_serializing_if = "is_false")]
    pub omit_last: bool,
    /// Stitches edited by hand: the object's own records (host coordinates, in
    /// sewing order, with their trims, stops and jumps), sewn instead of stitches
    /// generated from its shape. `None` = generated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen: Option<Vec<Stitch>>,
    /// The style (a named set of stitch properties) the object follows: redefining
    /// the style changes the object too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// Sewn on a chenille machine as chain or moss, at a needle height (CH-01); `None`: a
    /// lockstitch object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chenille: Option<crate::model::Chenille>,
}

/// How far (mm) fills cut by an object above still run in under its edge, so no
/// gap opens when the fabric pulls.
pub const UNDERLYING_OVERLAP: f64 = 1.0;

impl Embroidery {
    pub fn new(stitch: StitchType) -> Self {
        Embroidery {
            stitch,
            thread: None,
            width: None,
            profile: WidthProfile::Constant,
            guides: Vec::new(),
            splits: Vec::new(),
            flow: Vec::new(),
            auto_direction: true,
            entry: None,
            exit: None,
            trim_after: false,
            tie_in: None,
            tie_off: None,
            remove_underlying: false,
            underlying_overlap: None,
            stop_after: false,
            own_block: false,
            omit_last: false,
            frozen: None,
            style: None,
            chenille: None,
        }
    }

    /// The object's stitch properties without what belongs to its shape (inclination
    /// lines, split lines, beads, hand-edited stitches, a fill's centre), its thread or its
    /// style: what current properties and styles hold.
    pub fn properties(&self) -> Embroidery {
        let mut e = self.clone();
        e.thread = None;
        e.guides.clear();
        e.splits.clear();
        e.flow.clear();
        e.entry = None;
        e.exit = None;
        e.frozen = None;
        e.style = None;
        if let StitchType::Fill(f) = &mut e.stitch {
            f.center = None;
        }
        e
    }

    /// Takes the stitch properties of `props` (see [`Embroidery::properties`]),
    /// keeping this object's thread, inclination and split lines, beads, hand-edited
    /// stitches, fill centre and style.
    pub fn take_properties(&mut self, props: &Embroidery) {
        let center = match &self.stitch {
            StitchType::Fill(f) => f.center,
            _ => None,
        };
        let mut e = props.properties();
        e.thread = self.thread.take();
        e.guides = std::mem::take(&mut self.guides);
        e.splits = std::mem::take(&mut self.splits);
        e.flow = std::mem::take(&mut self.flow);
        e.entry = self.entry;
        e.exit = self.exit;
        e.frozen = self.frozen.take();
        e.style = self.style.take();
        if let StitchType::Fill(f) = &mut e.stitch
            && f.center.is_none()
        {
            f.center = center;
        }
        *self = e;
    }

    /// The overlap (mm) kept under this object's edge when it removes the
    /// stitching below it.
    pub fn cut_overlap(&self) -> f64 {
        self.underlying_overlap.unwrap_or(UNDERLYING_OVERLAP).clamp(0.0, 10.0)
    }

    pub fn run() -> Self {
        Self::new(StitchType::Run(RunParams::default()))
    }

    pub fn satin() -> Self {
        Self::new(StitchType::Satin(SatinParams::default()))
    }

    pub fn fill() -> Self {
        Self::new(StitchType::Fill(FillParams::default()))
    }

    /// A closed shape stitched with satin that follows its inclination lines.
    pub fn satin_fill() -> Self {
        Self::new(StitchType::Fill(FillParams { kind: FillKind::Satin, ..FillParams::satin() }))
    }

    pub fn applique() -> Self {
        Self::new(StitchType::Applique(AppliqueParams::default()))
    }

    pub fn manual() -> Self {
        Self::new(StitchType::Manual)
    }

    /// Follows a host transform: positions move, the fill angle turns with the
    /// shape and satin widths scale with it.
    pub fn transform(&mut self, m: &Affine) {
        if let StitchType::Lettering(l) = &mut self.stitch {
            l.transform(m);
        }
        for g in &mut self.guides {
            g.transform(m);
        }
        for p in self.splits.iter_mut().chain(self.flow.iter_mut()).flatten() {
            *p = m.apply(*p);
        }
        for s in self.frozen.iter_mut().flatten() {
            let p = m.apply(s.pos());
            (s.x, s.y) = (p.x, p.y);
        }
        if let Some(p) = &mut self.entry {
            *p = m.apply(*p);
        }
        if let Some(p) = &mut self.exit {
            *p = m.apply(*p);
        }
        if let StitchType::Fill(f) = &mut self.stitch {
            f.angle = transform_angle(f.angle, m);
            if let Some(c) = &mut f.center {
                *c = m.apply(*c);
            }
        }
        let k = m.mean_scale();
        if k.is_finite()
            && k > 1e-9
            && (k - 1.0).abs() > 1e-12
            && let Some(w) = &mut self.width
        {
            *w *= k;
        }
    }
}

/// Document-wide embroidery settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocEmbroidery {
    pub hoop: Option<Hoop>,
    /// Fabric preset name.
    pub fabric: String,
    pub settings: DesignSettings,
    /// Thread chart used to name colours ("Generic", "Brother", "Janome", ...).
    pub chart: String,
    /// User-defined threads (in addition to the chart).
    pub palette: Vec<Thread>,
    /// The machine profile chosen for the design: a standard profile's file format
    /// (`"pes"`) or the name of one of the user's; empty when none is chosen.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub machine: String,
    /// The file format of the design's machine when it is one of the user's profiles,
    /// so the design keeps it where that profile isn't saved.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub machine_format: String,
    /// Properties new objects take, by stitch type ([`StitchType::type_name`]): the
    /// digitizing presets, or "current properties".
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub current: BTreeMap<String, Embroidery>,
    /// Named sets of stitch properties.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<EmbStyle>,
    /// Fabrics made for this design, next to the built-in ones.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fabrics: Vec<Fabric>,
    /// What Check Design flags.
    #[serde(skip_serializing_if = "CheckLimits::is_default")]
    pub checks: CheckLimits,
    /// Motifs made from lines drawn in the design (Make Motif).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub motifs: Vec<CustomMotif>,
    /// A font document: the embroidery font the design is being made into.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<FontDesign>,
    /// Notes kept with the design (placement, garment, customer wishes), printed on worksheets.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// What sewing the design costs, shown with its figures and on worksheets (none: not shown).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<CostModel>,
    /// Other thread schemes for the design.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub colorways: Vec<Colorway>,
    /// The scheme in use: 0 the design's own threads, else `colorways[colorway - 1]`.
    #[serde(skip_serializing_if = "is_zero_index")]
    pub colorway: usize,
    /// The machine stitch file the design was opened from (its format, `dst`, `pes`…), kept when
    /// it is saved: what its [`DesignGrade`] goes by. Empty for a design made here.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub opened_from: String,
}

/// How editable a design is, by where it came from: made here (every object keeps its stitch
/// settings), or a machine stitch file, with its stitches made into objects or not. The grades
/// BERNINA's software shows on its status bar
/// (<https://softwarehelp.bernina.com/en/MainHelp/PART_Basics/assets/Design_files.htm>); grade B,
/// a design read from another program's outline file, has no counterpart here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesignGrade {
    /// Made in StitchFoundry: objects with their stitch settings.
    A,
    /// A machine file, part or all of its stitches made into objects.
    C,
    /// A machine file, stitches only.
    D,
}

impl DesignGrade {
    pub fn letter(self) -> &'static str {
        match self {
            DesignGrade::A => "A",
            DesignGrade::C => "C",
            DesignGrade::D => "D",
        }
    }

    /// The grade of a design with `settings` whose stitched objects have `stitches` types.
    pub fn of<'a>(settings: Option<&DocEmbroidery>, mut stitches: impl Iterator<Item = &'a StitchType>) -> DesignGrade {
        if settings.is_none_or(|s| s.opened_from.is_empty()) {
            DesignGrade::A
        } else if stitches.any(|t| !matches!(t, StitchType::Manual)) {
            DesignGrade::C
        } else {
            DesignGrade::D
        }
    }
}

fn is_zero_index(v: &usize) -> bool {
    *v == 0
}

/// Colorways a design keeps at most.
pub const MAX_COLORWAYS: usize = 50;

/// A thread scheme for the design: a thread for each of its colours ("slots": its own threads
/// in the order they are first used).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Colorway {
    pub name: String,
    pub threads: Vec<Thread>,
}

impl DocEmbroidery {
    /// The colorway in use, if not the design's own threads.
    pub fn active_colorway(&self) -> Option<&Colorway> {
        self.colorway.checked_sub(1).and_then(|i| self.colorways.get(i))
    }

    /// The threads of scheme `index` (0: the design's own, `own`) for each slot: a colorway's
    /// own where it has one, the design's for colours it gained since.
    pub fn threads_of(&self, index: usize, own: &[Thread]) -> Vec<Thread> {
        let cw = index.checked_sub(1).and_then(|i| self.colorways.get(i));
        own.iter().enumerate().map(|(k, t)| cw.and_then(|c| c.threads.get(k)).cloned().unwrap_or_else(|| t.clone())).collect()
    }
}

/// What sewing a design costs: a price per thousand stitches, the machine's time (shared by
/// the pieces its heads sew at once), the thread (top and bobbin) and a set-up charge spread
/// over the order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CostModel {
    pub per_thousand: f64,
    /// The machine's time, an hour.
    pub per_hour: f64,
    /// Thread, per 1000 m.
    pub thread_per_km: f64,
    /// Once an order.
    pub setup: f64,
    /// Pieces sewn at once.
    pub heads: u32,
    /// Written before amounts ("$", "€", "kr ").
    pub currency: String,
}

impl Default for CostModel {
    fn default() -> Self {
        CostModel { per_thousand: 1.0, per_hour: 0.0, thread_per_km: 3.0, setup: 0.0, heads: 1, currency: "$".into() }
    }
}

/// A cost estimate, a piece but for `total`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CostEstimate {
    pub stitches: f64,
    pub time: f64,
    pub thread: f64,
    pub setup: f64,
    pub per_piece: f64,
    pub total: f64,
}

impl CostModel {
    /// The cost of `quantity` pieces of a design with these figures.
    pub fn estimate(&self, stats: &crate::stats::PatternStats, quantity: u32) -> CostEstimate {
        let q = f64::from(quantity.max(1));
        let fin = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
        let stitches = fin(stats.stitches as f64 / 1000.0 * self.per_thousand);
        let time = fin(stats.seconds / 3600.0 * self.per_hour / f64::from(self.heads.max(1)));
        let thread = fin((stats.thread_m + stats.bobbin_m) / 1000.0 * self.thread_per_km);
        let setup = fin(self.setup / q);
        let per_piece = stitches + time + thread + setup;
        CostEstimate { stitches, time, thread, setup, per_piece, total: per_piece * q }
    }

    /// `v` as money.
    pub fn money(&self, v: f64) -> String {
        format!("{}{v:.2}", self.currency)
    }
}

/// An embroidery font being made: a document whose artboards, each named after its glyph, hold
/// the glyphs' art. An artboard's width is its glyph's advance; its baseline sits `descent` up
/// from its bottom and its capitals reach `size` above that.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FontDesign {
    pub name: String,
    /// The capitals' height as digitized (mm).
    pub size: f64,
    /// Room below the baseline on each glyph's artboard (mm).
    pub descent: f64,
    /// Lines apart, × `size`.
    pub leading: f64,
    /// A space's width (mm).
    pub space: f64,
    /// "upper" or "lower": text set in that case; empty: as typed.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub letter_case: String,
    /// The glyph characters the font lacks show (empty: none).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub default_glyph: String,
    /// How far the font scales and still sews well, × `size` (0: not said).
    pub min_scale: f64,
    pub max_scale: f64,
    /// How much closer a pair of glyphs ("A V") sits (mm).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub kerning: BTreeMap<String, f64>,
    /// Kerning classes: named sets of glyphs ("round": "COQ"), kerned against a glyph or another
    /// class in one step.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub classes: BTreeMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub license: String,
}

impl Default for FontDesign {
    fn default() -> Self {
        FontDesign {
            name: "My Font".into(),
            size: 25.0,
            descent: 10.0,
            leading: 1.4,
            space: 8.0,
            letter_case: String::new(),
            default_glyph: String::new(),
            min_scale: 0.5,
            max_scale: 2.0,
            kerning: BTreeMap::new(),
            classes: BTreeMap::new(),
            description: String::new(),
            license: String::new(),
        }
    }
}

/// The limits Check Design measures the design against.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CheckLimits {
    /// Layers of thread over a 2 × 2 mm square flagged as too dense. A tatami fill is
    /// about 1.2 with its underlay, a satin column 2.5 to 3.5.
    pub dense_layers: f64,
    /// Stitches longer than this (mm) can snag.
    pub long_stitch: f64,
    /// Lettering lower than this (mm) is hard to sew legibly.
    pub small_lettering: f64,
    /// Objects smaller than this (mm) both ways are tiny.
    pub tiny_object: f64,
    /// Sewn on clothing (worn and washed): long stitches catch from 7 mm, not 12.1 mm.
    pub garment: bool,
    /// Stitches shorter than this (mm) make the needle sew in one spot and can break the thread.
    pub short_stitch: f64,
    /// Satin narrower than this (mm) doesn't sit up on the fabric.
    pub narrow_satin: f64,
    /// Tatami stitches shorter than this (mm) ripple after washing.
    pub short_fill: f64,
}

/// The long-stitch limit for clothing and for everything else (mm).
pub const LONG_STITCH_GARMENT: f64 = 7.0;
pub const LONG_STITCH_OTHER: f64 = 12.1;

impl Default for CheckLimits {
    fn default() -> Self {
        CheckLimits {
            dense_layers: 4.5,
            long_stitch: LONG_STITCH_GARMENT,
            small_lettering: 5.0,
            tiny_object: 1.0,
            garment: true,
            short_stitch: 0.5,
            narrow_satin: 1.0,
            short_fill: 4.0,
        }
    }
}

impl CheckLimits {
    pub fn is_default(&self) -> bool {
        *self == CheckLimits::default()
    }
}

/// A named set of stitch properties ([`Embroidery::properties`]) objects can follow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmbStyle {
    pub name: String,
    pub properties: Embroidery,
}

impl DocEmbroidery {
    /// The properties a new object of stitch type `name` ([`StitchType::type_name`])
    /// takes, when they were made current.
    pub fn current_for(&self, name: &str) -> Option<&Embroidery> {
        self.current.get(name).filter(|e| e.stitch.type_name() == name)
    }

    pub fn style(&self, name: &str) -> Option<&EmbStyle> {
        self.styles.iter().find(|s| s.name == name)
    }

    /// One of the design's motifs, by name.
    pub fn motif(&self, name: &str) -> Option<&CustomMotif> {
        self.motifs.iter().find(|m| m.name == name)
    }

    /// A fabric by name: one made for the design, else a built-in one.
    pub fn fabric(&self, name: &str) -> Option<Fabric> {
        self.fabrics.iter().find(|f| f.name == name).cloned().or_else(|| crate::fabrics::find(name))
    }

    /// The file format the design's machine reads (`"vp3"`): the format of the profile
    /// it names among the standard ones and the user's `saved` ones, else the one the
    /// design kept; none when no machine is chosen.
    pub fn machine_file_format(&self, saved: &[crate::machines::MachineProfile]) -> Option<String> {
        crate::machines::find(saved, &self.machine).map(|m| m.format).or_else(|| Some(self.machine_format.clone()).filter(|f| !f.is_empty()))
    }
}

impl Default for DocEmbroidery {
    fn default() -> Self {
        DocEmbroidery {
            hoop: None,
            fabric: String::new(),
            settings: DesignSettings::default(),
            chart: "Generic".into(),
            palette: Vec::new(),
            machine: String::new(),
            machine_format: String::new(),
            current: BTreeMap::new(),
            styles: Vec::new(),
            fabrics: Vec::new(),
            checks: CheckLimits::default(),
            motifs: Vec::new(),
            font: None,
            notes: String::new(),
            cost: None,
            colorways: Vec::new(),
            colorway: 0,
            opened_from: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_text_is_below_six_millimetres_unless_said() {
        assert!(SmallText::Auto.applies(4.0) && !SmallText::Auto.applies(8.0));
        assert!(SmallText::On.applies(20.0) && !SmallText::Off.applies(2.0));
        assert_eq!((SmallText::from_name("on"), LetterStitch::from_name("fill")), (Some(SmallText::On), Some(LetterStitch::Fill)));
    }

    #[test]
    fn costs_add_stitches_time_thread_and_a_share_of_the_set_up() {
        let stats = crate::stats::PatternStats {
            stitches: 12_000,
            jumps: 0,
            trims: 0,
            color_changes: 0,
            stops: 0,
            bounds: crate::geom::BBox::EMPTY,
            width_mm: 0.0,
            height_mm: 0.0,
            thread_m: 40.0,
            bobbin_m: 10.0,
            seconds: 1800.0,
            colors: Vec::new(),
            max_stitch_mm: 0.0,
        };
        let m = CostModel { per_thousand: 0.5, per_hour: 20.0, thread_per_km: 4.0, setup: 30.0, heads: 4, currency: "€".into() };
        let e = m.estimate(&stats, 10);
        // 6 for the stitches, half an hour over 4 heads at 20 an hour, 50 m of thread, 30 over 10
        for (got, want) in [(e.stitches, 6.0), (e.time, 2.5), (e.thread, 0.2), (e.setup, 3.0), (e.per_piece, 11.7), (e.total, 117.0)] {
            assert!((got - want).abs() < 1e-9, "{e:?}");
        }
        assert_eq!(m.money(e.per_piece), "€11.70");
        // no pieces is one; no heads is one
        let one = CostModel { heads: 0, ..m.clone() }.estimate(&stats, 0);
        assert!((one.setup - 30.0).abs() < 1e-9 && (one.time - 10.0).abs() < 1e-9, "{one:?}");
    }

    #[test]
    fn lettering_lines_with_a_size_of_their_own_scale_with_it() {
        let mut l = Lettering {
            text: "Hi\nthere".into(),
            font: "F".into(),
            size: 10.0,
            line_sizes: vec![0.0, 20.0],
            leading: 1.0,
            arc: 0.0,
            spiral: 0.0,
            align: LetterAlign::Centre,
            sew: LetterSewing::default(),
            letters: Vec::new(),
            puff: None,
            applique: None,
            stitch: LetterStitch::Font,
            small: SmallText::Auto,
            vertical: false,
            path: Vec::new(),
            frame: Affine::IDENTITY,
        };
        l.transform(&Affine::scale(2.0, 2.0));
        // the block doubled; the line twice the block's size stays twice it (0 stays "the block's")
        assert!((l.size - 20.0).abs() < 1e-9, "{l:?}");
        assert_eq!(l.line_sizes, vec![0.0, 40.0]);
    }

    #[test]
    fn lettering_follows_its_group_its_scale_going_into_its_size() {
        let at = Affine::translate(Point::new(100.0, 50.0));
        let mut l = Lettering {
            text: "Hi".into(),
            font: "F".into(),
            size: 10.0,
            line_sizes: Vec::new(),
            leading: 1.0,
            arc: 30.0,
            spiral: 5.0,
            align: LetterAlign::Centre,
            sew: LetterSewing::default(),
            letters: vec![LetterAdjust { dy: 2.0, ..LetterAdjust::none(1) }],
            puff: None,
            applique: None,
            stitch: LetterStitch::Font,
            small: SmallText::Auto,
            vertical: false,
            path: vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)],
            frame: at,
        };
        // turned a quarter about the origin, then doubled
        l.transform(&Affine::rotate(std::f64::consts::FRAC_PI_2).then(&Affine::scale(2.0, 2.0)));
        assert!((l.size - 20.0).abs() < 1e-9 && (l.arc - 60.0).abs() < 1e-9 && (l.spiral - 10.0).abs() < 1e-9, "{l:?}");
        // a drawn baseline: as far again
        assert!((l.path[1].x - 20.0).abs() < 1e-9, "{l:?}");
        // a letter moved: as far again
        assert!((l.letters[0].dy - 4.0).abs() < 1e-9, "{l:?}");
        let o = l.frame.apply(Point::new(0.0, 0.0));
        let x = l.frame.apply(Point::new(1.0, 0.0));
        assert!((o.x + 100.0).abs() < 1e-9 && (o.y - 200.0).abs() < 1e-9, "{o:?}");
        assert!((x.x - o.x).abs() < 1e-9 && (x.y - o.y - 1.0).abs() < 1e-9, "{x:?}");
        // kept through save and open, and moved with the group
        let mut e = Embroidery::new(StitchType::Lettering(Box::new(l.clone())));
        let back: Embroidery = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(back, e);
        // sewn as nothing says: not written; said: kept
        assert!(!serde_json::to_string(&e).unwrap().contains("sew"));
        let mut c = l.clone();
        c.sew = LetterSewing { order: LetterOrder::Centre, up: true, join: LetterJoin::Baseline, trim: LetterTrim::Word };
        let back: Lettering = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back.sew, c.sew);
        e.transform(&Affine::translate(Point::new(5.0, 0.0)));
        let StitchType::Lettering(m) = &e.stitch else { panic!() };
        assert!((m.frame.apply(Point::new(0.0, 0.0)).x + 95.0).abs() < 1e-9 && m.size == l.size);
        // a transform that flattens it leaves it be
        let mut z = l.clone();
        z.transform(&Affine::scale(0.0, 0.0));
        assert_eq!(z, l);
    }

    #[test]
    fn json_is_compact_and_round_trips() {
        let mut e = Embroidery::fill();
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.starts_with(r#"{"stitch":{"type":"fill""#), "{json}");
        assert!(!json.contains("guides") && !json.contains("auto_direction"));
        e.guides.push(Guide::new(Point::new(0.0, 0.0), Point::new(10.0, 0.0)));
        e.entry = Some(Point::new(1.0, 2.0));
        e.auto_direction = false;
        let back: Embroidery = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(e, back);
        let manual: Embroidery = serde_json::from_str(r#"{"stitch":{"type":"manual"}}"#).unwrap();
        assert_eq!(manual, Embroidery::manual());
    }

    #[test]
    fn transform_moves_guides_and_turns_angle() {
        let mut e = Embroidery::fill();
        e.guides.push(Guide::new(Point::new(0.0, 0.0), Point::new(10.0, 0.0)));
        e.width = Some(2.0);
        e.transform(&Affine::translate(Point::new(5.0, 5.0)).then(&Affine::scale(2.0, 2.0)));
        assert!(e.guides[0].a.approx_eq(Point::new(10.0, 10.0), 1e-9));
        assert!((e.width.unwrap() - 4.0).abs() < 1e-9);
        e.transform(&Affine::rotate(std::f64::consts::FRAC_PI_2));
        let StitchType::Fill(f) = &e.stitch else { panic!() };
        assert!((f.angle - 135.0).abs() < 1e-9, "{}", f.angle);
    }
}
