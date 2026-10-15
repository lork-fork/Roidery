//! Stitch settings, as stored on an object. Every field has a default, so a project saved by an
//! older or newer Roidery still opens: missing values take the defaults and unknown ones are
//! ignored.

use serde::{Deserialize, Serialize};

/// The stitch types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StitchType {
    /// Not stitched (artwork only, such as a traced image's guide).
    None,
    /// Single running stitch.
    #[default]
    Run,
    /// Triple (bean) run: each stitch sewn forward, back and forward again.
    Triple,
    /// Satin of a fixed width along a path.
    Satin,
    /// Satin Blocks: two rails with rungs between them.
    SatinBlocks,
    /// Tatami fill.
    Tatami,
    /// Contour fill: rows following the outline inwards.
    Contour,
    /// Circular fill: rows that are circles round a centre.
    Circular,
    /// Gradient fill: tatami whose rows spread out across the shape, so the colour fades.
    Gradient,
    /// Motif fill: rows of a small repeating pattern in running stitch.
    Motif,
    /// Guided fill: tatami rows that follow a curve drawn across the shape.
    Guided,
    /// The path's points are the needle penetrations.
    Manual,
    /// E-stitch (blanket stitch): a run with prongs to one side, for appliqué edges.
    EStitch,
    /// Rope: a narrow column of overlapping slanted stitches, sewn once, twice or three times.
    Rope,
}

impl StitchType {
    pub const ALL: [StitchType; 14] = [
        Self::None,
        Self::Run,
        Self::Triple,
        Self::Satin,
        Self::SatinBlocks,
        Self::Tatami,
        Self::Contour,
        Self::Circular,
        Self::Gradient,
        Self::Motif,
        Self::Guided,
        Self::Manual,
        Self::EStitch,
        Self::Rope,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Run => "run",
            Self::Triple => "triple",
            Self::Satin => "satin",
            Self::SatinBlocks => "satinBlocks",
            Self::Tatami => "tatami",
            Self::Contour => "contour",
            Self::Circular => "circular",
            Self::Gradient => "gradient",
            Self::Motif => "motif",
            Self::Guided => "guided",
            Self::Manual => "manual",
            Self::EStitch => "eStitch",
            Self::Rope => "rope",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Run => "Single Run",
            Self::Triple => "Triple Run",
            Self::Satin => "Satin",
            Self::SatinBlocks => "Satin Blocks",
            Self::Tatami => "Tatami Fill",
            Self::Contour => "Contour Fill",
            Self::Circular => "Circular Fill",
            Self::Gradient => "Gradient Fill",
            Self::Motif => "Motif Fill",
            Self::Guided => "Guided Fill",
            Self::Manual => "Manual",
            Self::EStitch => "E-Stitch",
            Self::Rope => "Rope",
        }
    }

    /// Parse an id or label, ignoring case, spaces, dashes and underscores.
    pub fn parse(s: &str) -> Option<Self> {
        let norm = |t: &str| t.to_ascii_lowercase().replace([' ', '-', '_'], "");
        let s = norm(s);
        Self::ALL.into_iter().find(|t| norm(t.id()) == s || norm(t.label()) == s).or(match s.as_str() {
            "fill" => Some(Self::Tatami),
            "bean" => Some(Self::Triple),
            "single" | "running" => Some(Self::Run),
            "blanket" | "blanketstitch" => Some(Self::EStitch),
            _ => None,
        })
    }

    /// Whether this type fills an area (as opposed to following a line).
    pub fn is_fill(self) -> bool {
        matches!(self, Self::Tatami | Self::Contour | Self::Circular | Self::Gradient | Self::Motif | Self::Guided)
    }
}

/// Running stitch settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RunSettings {
    /// Target stitch length, mm.
    pub length: f64,
    /// Corners sharper than this keep a penetration on them; mm of allowed deviation when
    /// simplifying curves before stitching.
    pub tolerance: f64,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self { length: 2.5, tolerance: 0.2 }
    }
}

/// One underlay pass.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Underlay {
    /// A run down the middle of a satin.
    #[serde(rename_all = "camelCase")]
    CenterWalk {
        #[serde(default = "d_underlay_len")]
        length: f64,
    },
    /// Runs just inside both edges of a satin.
    #[serde(rename_all = "camelCase")]
    Contour {
        #[serde(default = "d_underlay_len")]
        length: f64,
        /// mm inside each edge.
        #[serde(default = "d_inset")]
        inset: f64,
    },
    /// A sparse zig-zag inside a satin.
    #[serde(rename_all = "camelCase")]
    ZigZag {
        /// mm between same-side penetrations.
        #[serde(default = "d_zz_spacing")]
        spacing: f64,
        #[serde(default = "d_inset")]
        inset: f64,
    },
}

fn d_underlay_len() -> f64 {
    2.5
}
fn d_inset() -> f64 {
    0.4
}
fn d_zz_spacing() -> f64 {
    2.0
}

/// Satin settings (Satin Blocks and satin along a path).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SatinSettings {
    /// Column width for a satin along a path, mm (Satin Blocks take theirs from the rails).
    pub width: f64,
    /// mm between penetrations on the same edge.
    pub density: f64,
    /// mm added beyond each edge so the column keeps its width after the thread pulls in.
    pub pull_compensation: f64,
    /// Where across the column the density is measured: 0 the longer (outside) edge, 0.5 the
    /// centre, 1 the shorter (inside) edge.
    pub fractional_spacing: f64,
    /// Underlay passes, sewn in order before the top stitching.
    pub underlays: Vec<Underlay>,
    /// Short stitches: where a curve crowds penetrations on one edge (closer than
    /// `short_stitch_distance`), every other one stops this % of the way short of that edge, so
    /// the inside of the curve doesn't pile up. 0 turns them off.
    pub short_stitch_inset: f64,
    /// mm: penetrations on the same edge closer than this get short stitches.
    pub short_stitch_distance: f64,
    /// Split satin: a top stitch longer than this (mm) is split into shorter ones, staggered so
    /// the splits don't line up into a groove. 0 never splits.
    pub split_length: f64,
}

impl Default for SatinSettings {
    fn default() -> Self {
        Self {
            width: 3.0,
            density: 0.4,
            pull_compensation: 0.2,
            fractional_spacing: 0.5,
            underlays: vec![Underlay::CenterWalk { length: 2.5 }],
            short_stitch_inset: 15.0,
            short_stitch_distance: 0.25,
            split_length: 7.0,
        }
    }
}

/// Tatami fill settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FillSettings {
    /// Row direction, degrees counter-clockwise from the x axis (as seen on screen).
    pub angle: f64,
    /// mm between rows.
    pub row_spacing: f64,
    /// Target stitch length along a row, mm.
    pub stitch_length: f64,
    /// Rows after which the stagger pattern repeats (penetrations shift by 1/n of a stitch
    /// each row, which hides the needle holes in a diagonal texture).
    pub staggers: u32,
    /// mm added to each end of a row, against the thread pulling the fill in.
    pub pull_compensation: f64,
    /// Lay a sparse fill at right angles underneath first.
    pub underlay: bool,
    /// Underlay row spacing, mm.
    pub underlay_spacing: f64,
    /// mm the underlay stays inside the edges.
    pub underlay_inset: f64,
    /// Gradient fill: mm between rows on the far side (`row_spacing` is the near side's).
    pub end_spacing: f64,
}

impl Default for FillSettings {
    fn default() -> Self {
        Self {
            angle: 45.0,
            row_spacing: 0.4,
            stitch_length: 3.0,
            staggers: 4,
            pull_compensation: 0.2,
            underlay: true,
            underlay_spacing: 2.5,
            underlay_inset: 0.8,
            end_spacing: 2.0,
        }
    }
}

/// Circular fill settings (the rows, density and underlay come from the fill settings).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CircularSettings {
    /// The circles' centre, mm right of the middle of the shape's bounds.
    pub center_x: f64,
    /// The circles' centre, mm below the middle of the shape's bounds.
    pub center_y: f64,
}

/// The motif fill's patterns (drawn in code).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MotifPattern {
    #[default]
    Zigzag,
    Wave,
    /// Zigzags mirrored row to row, meeting point to point.
    Diamonds,
    /// Arches, every other row shifted half a motif.
    Scales,
}

impl MotifPattern {
    pub const ALL: [MotifPattern; 4] = [Self::Zigzag, Self::Wave, Self::Diamonds, Self::Scales];
}

/// Motif fill settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MotifSettings {
    pub pattern: MotifPattern,
    /// One motif's length along the row, mm.
    pub width: f64,
    /// The pattern's height across the row, mm.
    pub height: f64,
    /// mm between rows.
    pub spacing: f64,
    /// Rows' direction, degrees counter-clockwise from the x axis (as seen on screen).
    pub angle: f64,
    /// Running stitch length, mm.
    pub length: f64,
    /// Sew each stitch three times (bean stitch) for a bolder line.
    pub triple: bool,
}

impl Default for MotifSettings {
    fn default() -> Self {
        Self { pattern: MotifPattern::Zigzag, width: 4.0, height: 2.0, spacing: 3.0, angle: 0.0, length: 2.0, triple: false }
    }
}

/// Guided fill settings (rows, stitch length and underlay come from [`FillSettings`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GuidedSettings {
    /// The curve the rows follow, as fractions of the shape's bounding box (0,0 its top-left,
    /// 1,1 its bottom-right), so it moves and scales with the shape. Fewer than two points: the
    /// rows run straight at the fill angle.
    pub guide: Vec<[f64; 2]>,
}

/// Most points a guide keeps.
pub const MAX_GUIDE_POINTS: usize = 2_000;

/// E-stitch settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EStitchSettings {
    /// How far each prong reaches from the line, mm.
    pub width: f64,
    /// mm between prongs.
    pub spacing: f64,
}

impl Default for EStitchSettings {
    fn default() -> Self {
        Self { width: 2.5, spacing: 2.5 }
    }
}

/// Rope settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RopeSettings {
    /// Column width, mm.
    pub width: f64,
    /// mm between stitches along the left edge.
    pub spacing: f64,
    /// How much further along each stitch lands on the right edge, mm.
    pub slant: f64,
    /// 1, 2 (double rope) or 3 (triple rope).
    pub passes: u32,
}

impl Default for RopeSettings {
    fn default() -> Self {
        Self { width: 2.0, spacing: 0.6, slant: 1.5, passes: 2 }
    }
}

/// Everything an object needs to be stitched.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StitchSettings {
    #[serde(rename = "type")]
    pub kind: StitchType,
    pub run: RunSettings,
    pub satin: SatinSettings,
    pub fill: FillSettings,
    pub circular: CircularSettings,
    pub motif: MotifSettings,
    pub guided: GuidedSettings,
    #[serde(rename = "eStitch")]
    pub e_stitch: EStitchSettings,
    pub rope: RopeSettings,
    /// Lettering: the object's path is this text, laid out in a lettering font (and is
    /// regenerated when the text changes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lettering: Option<crate::lettering::Lettering>,
}

impl StitchSettings {
    pub fn of(kind: StitchType) -> Self {
        Self { kind, ..Default::default() }
    }

    /// Clamp every value into a range that can be sewn and won't hang the generators: lengths
    /// and spacings get floors, counts get ceilings, non-finite values take the defaults.
    pub fn sanitized(&self) -> Self {
        let d = Self::default();
        let f = |v: f64, def: f64, lo: f64, hi: f64| if v.is_finite() { v.clamp(lo, hi) } else { def };
        let mut s = self.clone();
        s.run.length = f(s.run.length, d.run.length, 0.3, 12.0);
        s.run.tolerance = f(s.run.tolerance, d.run.tolerance, 0.01, 5.0);
        s.satin.width = f(s.satin.width, d.satin.width, 0.3, 30.0);
        s.satin.density = f(s.satin.density, d.satin.density, 0.1, 5.0);
        s.satin.pull_compensation = f(s.satin.pull_compensation, d.satin.pull_compensation, -2.0, 5.0);
        s.satin.fractional_spacing = f(s.satin.fractional_spacing, d.satin.fractional_spacing, 0.0, 1.0);
        s.satin.short_stitch_inset = f(s.satin.short_stitch_inset, d.satin.short_stitch_inset, 0.0, 50.0);
        s.satin.short_stitch_distance = f(s.satin.short_stitch_distance, d.satin.short_stitch_distance, 0.0, 5.0);
        s.satin.split_length = f(s.satin.split_length, d.satin.split_length, 0.0, 12.0);
        if s.satin.split_length > 0.0 {
            s.satin.split_length = s.satin.split_length.max(2.0);
        }
        s.satin.underlays.truncate(8);
        for u in &mut s.satin.underlays {
            match u {
                Underlay::CenterWalk { length } => *length = f(*length, 2.5, 0.5, 12.0),
                Underlay::Contour { length, inset } => {
                    *length = f(*length, 2.5, 0.5, 12.0);
                    *inset = f(*inset, 0.4, 0.0, 10.0);
                }
                Underlay::ZigZag { spacing, inset } => {
                    *spacing = f(*spacing, 2.0, 0.3, 10.0);
                    *inset = f(*inset, 0.4, 0.0, 10.0);
                }
            }
        }
        s.fill.angle = if s.fill.angle.is_finite() { s.fill.angle.rem_euclid(360.0) } else { d.fill.angle };
        s.fill.row_spacing = f(s.fill.row_spacing, d.fill.row_spacing, 0.1, 10.0);
        s.fill.stitch_length = f(s.fill.stitch_length, d.fill.stitch_length, 0.5, 12.0);
        s.fill.staggers = s.fill.staggers.clamp(1, 16);
        s.fill.pull_compensation = f(s.fill.pull_compensation, d.fill.pull_compensation, -2.0, 5.0);
        s.fill.underlay_spacing = f(s.fill.underlay_spacing, d.fill.underlay_spacing, 0.5, 10.0);
        s.fill.underlay_inset = f(s.fill.underlay_inset, d.fill.underlay_inset, 0.0, 10.0);
        s.fill.end_spacing = f(s.fill.end_spacing, d.fill.end_spacing, 0.1, 20.0);
        s.circular.center_x = f(s.circular.center_x, 0.0, -1000.0, 1000.0);
        s.circular.center_y = f(s.circular.center_y, 0.0, -1000.0, 1000.0);
        s.motif.width = f(s.motif.width, d.motif.width, 0.5, 100.0);
        s.motif.height = f(s.motif.height, d.motif.height, 0.0, 100.0);
        s.motif.spacing = f(s.motif.spacing, d.motif.spacing, 0.3, 100.0);
        s.motif.angle = if s.motif.angle.is_finite() { s.motif.angle.rem_euclid(360.0) } else { d.motif.angle };
        s.motif.length = f(s.motif.length, d.motif.length, 0.3, 12.0);
        s.guided.guide.retain(|p| p[0].is_finite() && p[1].is_finite());
        s.guided.guide.truncate(MAX_GUIDE_POINTS);
        s.e_stitch.width = f(s.e_stitch.width, d.e_stitch.width, 0.3, 10.0);
        s.e_stitch.spacing = f(s.e_stitch.spacing, d.e_stitch.spacing, 0.5, 12.0);
        s.rope.width = f(s.rope.width, d.rope.width, 0.5, 10.0);
        s.rope.spacing = f(s.rope.spacing, d.rope.spacing, 0.2, 5.0);
        s.rope.slant = f(s.rope.slant, d.rope.slant, 0.0, 10.0);
        s.rope.passes = s.rope.passes.clamp(1, 3);
        s.lettering = s.lettering.as_ref().map(crate::lettering::Lettering::sanitized);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ids_labels_and_aliases() {
        assert_eq!(StitchType::parse("satinBlocks"), Some(StitchType::SatinBlocks));
        assert_eq!(StitchType::parse("Satin Blocks"), Some(StitchType::SatinBlocks));
        assert_eq!(StitchType::parse("fill"), Some(StitchType::Tatami));
        assert_eq!(StitchType::parse("bean"), Some(StitchType::Triple));
        assert_eq!(StitchType::parse("nope"), None);
    }

    #[test]
    fn json_round_trip_and_missing_fields() {
        let s = StitchSettings::of(StitchType::Tatami);
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<StitchSettings>(&j).unwrap(), s);
        let partial: StitchSettings = serde_json::from_str(r#"{"type":"satin","satin":{"width":5}}"#).unwrap();
        assert_eq!(partial.kind, StitchType::Satin);
        assert_eq!(partial.satin.width, 5.0);
        assert_eq!(partial.satin.density, 0.4);
        let u: StitchSettings = serde_json::from_str(r#"{"satin":{"underlays":[{"type":"zigZag"},{"type":"contour","inset":1}]}}"#).unwrap();
        assert_eq!(u.satin.underlays, vec![Underlay::ZigZag { spacing: 2.0, inset: 0.4 }, Underlay::Contour { length: 2.5, inset: 1.0 }]);
    }

    #[test]
    fn sanitize_floors_spacings_and_fixes_nan() {
        let mut s = StitchSettings::default();
        s.fill.row_spacing = 0.0;
        s.satin.density = f64::NAN;
        s.fill.angle = -30.0;
        let t = s.sanitized();
        assert_eq!(t.fill.row_spacing, 0.1);
        assert_eq!(t.satin.density, 0.4);
        assert_eq!(t.fill.angle, 330.0);
    }
}
