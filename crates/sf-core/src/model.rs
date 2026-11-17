//! The object-based design model.
//!
//! Like professional digitizing software, a [`Design`] is a sequence of
//! *embroidery objects*. Each object stores an editable outline (contour, shape
//! or satin column) plus stitch parameters; the actual needle penetrations are
//! generated from them by [`crate::engine`]. This keeps designs scalable and
//! re-editable – changing size, angle or density regenerates the stitches.
//!
//! Angles are degrees measured **counter-clockwise on screen** from the +x axis
//! (see [`crate::geom::screen_dir`]).

use crate::direction::Guide;
use crate::geom::{Affine, BBox, Column, Contour, FLATTEN_TOLERANCE, Point, Shape, screen_angle, screen_dir};
use crate::hoops::Hoop;
use crate::pattern::Stitch;
use crate::threads::{Rgb, Thread, default_palette};
use serde::{Deserialize, Serialize};

pub type ObjectId = u64;

/// Version of the native `.sfd` file format.
pub const FORMAT_VERSION: u32 = 1;

fn yes() -> bool {
    true
}

fn default_version() -> u32 {
    FORMAT_VERSION
}

// ---------------------------------------------------------------------------
// Design
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Design {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub name: String,
    /// Thread palette; objects refer to it by index.
    pub palette: Vec<Thread>,
    /// Objects in sewing order.
    pub objects: Vec<EmbObject>,
    #[serde(default)]
    pub hoop: Option<Hoop>,
    /// Name of the fabric preset the design was prepared for.
    #[serde(default)]
    pub fabric: String,
    #[serde(default)]
    pub settings: DesignSettings,
    /// Artwork shown behind the design for manual tracing.
    #[serde(default)]
    pub backdrop: Option<Backdrop>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub next_id: ObjectId,
}

impl Default for Design {
    fn default() -> Self {
        Design::new()
    }
}

impl Design {
    pub fn new() -> Self {
        Design {
            version: FORMAT_VERSION,
            name: "Untitled".into(),
            palette: default_palette(),
            objects: Vec::new(),
            hoop: None,
            fabric: String::new(),
            settings: DesignSettings::default(),
            backdrop: None,
            notes: String::new(),
            next_id: 1,
        }
    }

    /// Allocates a fresh object id.
    pub fn fresh_id(&mut self) -> ObjectId {
        let max_existing = self.objects.iter().map(|o| o.id).max().unwrap_or(0);
        self.next_id = self.next_id.max(max_existing + 1);
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Appends an object and returns its id.
    pub fn add(&mut self, kind: ObjectKind, color: usize) -> ObjectId {
        let id = self.fresh_id();
        self.objects.push(EmbObject::new(id, kind, color));
        id
    }

    /// Appends a fully built object, assigning it a fresh id. Returns the id.
    pub fn push(&mut self, mut obj: EmbObject) -> ObjectId {
        obj.id = self.fresh_id();
        let id = obj.id;
        self.objects.push(obj);
        id
    }

    pub fn index_of(&self, id: ObjectId) -> Option<usize> {
        self.objects.iter().position(|o| o.id == id)
    }

    pub fn get(&self, id: ObjectId) -> Option<&EmbObject> {
        self.objects.iter().find(|o| o.id == id)
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut EmbObject> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    pub fn remove(&mut self, id: ObjectId) -> Option<EmbObject> {
        let i = self.index_of(id)?;
        Some(self.objects.remove(i))
    }

    /// Palette index for a colour: an existing thread with exactly this colour,
    /// otherwise a new palette entry.
    pub fn color_index(&mut self, c: Rgb) -> usize {
        if let Some(i) = self.palette.iter().position(|t| t.color == c) {
            return i;
        }
        self.palette.push(Thread::new(c, c.to_hex()));
        self.palette.len() - 1
    }

    /// Palette index for a thread (matched by colour, otherwise appended).
    pub fn thread_index(&mut self, t: &Thread) -> usize {
        if let Some(i) = self.palette.iter().position(|p| p.color == t.color) {
            return i;
        }
        self.palette.push(t.clone());
        self.palette.len() - 1
    }

    pub fn thread(&self, i: usize) -> Thread {
        self.palette.get(i).cloned().unwrap_or_else(|| Thread::new(Rgb::BLACK, "Black"))
    }

    /// Outline bounds of all visible objects.
    pub fn bbox(&self) -> BBox {
        self.objects.iter().filter(|o| o.visible).fold(BBox::EMPTY, |b, o| b.union(o.bbox()))
    }
}

/// Machine and sequencing settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesignSettings {
    /// Connectors longer than this (mm) are trimmed instead of jumped.
    pub trim_threshold: f64,
    /// Between two objects of one thread, carry the thread under the object about to be sewn
    /// (running stitches it covers) where it can reach it that way, instead of trimming.
    pub hide_travel: bool,
    /// How far (mm) a fill reaches in under a shape sewn after it that it touches, so no fabric
    /// shows between them when the stitches pull the fabric in (0: as drawn).
    pub overlap_touching: f64,
    /// Sew small locking stitches at the start of each object.
    pub tie_in: bool,
    /// Sew small locking stitches before each trim / colour change.
    pub tie_off: bool,
    /// Size of tie-in / tie-off stitches (mm).
    pub tie_length: f64,
    /// How tie-ins and tie-offs lock the thread.
    #[serde(skip_serializing_if = "TieStyle::is_default")]
    pub tie_style: TieStyle,
    /// Stitches shorter than this are merged away (mm).
    pub min_stitch: f64,
    /// Longest stitch the machine can sew; longer stitches are split (mm).
    pub max_stitch: f64,
    /// Longest single frame move; longer jumps are split (mm).
    pub max_jump: f64,
    /// Machine speed used for time estimates (stitches per minute).
    pub machine_speed: f64,
    /// Seconds per trim, for time estimates.
    pub trim_seconds: f64,
    /// Seconds per colour change, for time estimates.
    pub color_change_seconds: f64,
    /// A basting run sewn before everything else (none: not sewn).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basting: Option<Basting>,
    /// Where on the design sewing starts: the first object entered at its point closest to it
    /// (none: where the first object starts).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<DesignPoint>,
    /// Where the frame goes when the design is done (none: where its last stitch is).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<DesignPoint>,
    /// The registration marks multi-hooping sews in the overlaps.
    pub registration: RegistrationMarks,
    /// Where the design's middle sits in the hoop (mm from the hoop's centre, x right, y down);
    /// none: centred, as machines centre designs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoop_offset: Option<[f64; 2]>,
}

/// What a registration mark looks like.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkShape {
    /// Two short runs crossing.
    #[default]
    Cross,
    /// A ring.
    Circle,
    /// A ring with a cross through it.
    Target,
}

impl MarkShape {
    pub fn id(self) -> &'static str {
        match self {
            MarkShape::Cross => "cross",
            MarkShape::Circle => "circle",
            MarkShape::Target => "target",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "cross" => MarkShape::Cross,
            "circle" => MarkShape::Circle,
            "target" => MarkShape::Target,
            _ => return None,
        })
    }
}

/// The registration marks multi-hooping sews first in each hooping, in the overlaps it shares
/// with its neighbours, to line the fabric up by.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegistrationMarks {
    pub shape: MarkShape,
    /// How far a mark reaches from its middle (mm).
    pub size: f64,
    /// Marks in each overlap with a neighbour (1–4).
    pub per_overlap: u8,
    /// Their thread (a colour of their own, sewn first).
    pub color: Rgb,
}

impl Default for RegistrationMarks {
    fn default() -> Self {
        RegistrationMarks { shape: MarkShape::Cross, size: 3.0, per_overlap: 2, color: Rgb::new(0xc0, 0xc0, 0xc0) }
    }
}

impl RegistrationMarks {
    /// A mark at `at` as runs of running stitches about 1 mm long, each entered by a jump.
    pub fn runs(&self, at: Point) -> Vec<Vec<Point>> {
        let size = if self.size.is_finite() { self.size.clamp(1.0, 20.0) } else { 3.0 };
        let steps = (2.0 * size).ceil().max(2.0) as usize;
        let line = |dx: f64, dy: f64| -> Vec<Point> {
            (0..=steps)
                .map(|k| {
                    let t = -1.0 + 2.0 * k as f64 / steps as f64;
                    Point::new(at.x + dx * size * t, at.y + dy * size * t)
                })
                .collect()
        };
        let ring = || -> Vec<Point> {
            let n = ((std::f64::consts::TAU * size).ceil() as usize).clamp(12, 120);
            (0..=n)
                .map(|k| {
                    let a = std::f64::consts::TAU * k as f64 / n as f64;
                    Point::new(at.x + size * a.cos(), at.y + size * a.sin())
                })
                .collect()
        };
        match self.shape {
            MarkShape::Cross => vec![line(1.0, 0.0), line(0.0, 1.0)],
            MarkShape::Circle => vec![ring()],
            MarkShape::Target => vec![ring(), line(1.0, 0.0), line(0.0, 1.0)],
        }
    }
}

/// A point of the design's box: its centre, a corner or the middle of a side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DesignPoint {
    Centre,
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl DesignPoint {
    pub const ALL: [DesignPoint; 9] = [
        DesignPoint::Centre,
        DesignPoint::TopLeft,
        DesignPoint::Top,
        DesignPoint::TopRight,
        DesignPoint::Left,
        DesignPoint::Right,
        DesignPoint::BottomLeft,
        DesignPoint::Bottom,
        DesignPoint::BottomRight,
    ];

    pub fn id(self) -> &'static str {
        match self {
            DesignPoint::Centre => "centre",
            DesignPoint::TopLeft => "top-left",
            DesignPoint::Top => "top",
            DesignPoint::TopRight => "top-right",
            DesignPoint::Left => "left",
            DesignPoint::Right => "right",
            DesignPoint::BottomLeft => "bottom-left",
            DesignPoint::Bottom => "bottom",
            DesignPoint::BottomRight => "bottom-right",
        }
    }

    pub fn from_id(id: &str) -> Option<DesignPoint> {
        let id = if id == "center" { "centre" } else { id };
        DesignPoint::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The point on `b` (y down: the top is its least y).
    pub fn on(self, b: &BBox) -> Point {
        let (x0, x1, y0, y1) = (b.min.x, b.max.x, b.min.y, b.max.y);
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (x, y) = match self {
            DesignPoint::Centre => (cx, cy),
            DesignPoint::TopLeft => (x0, y0),
            DesignPoint::Top => (cx, y0),
            DesignPoint::TopRight => (x1, y0),
            DesignPoint::Left => (x0, cy),
            DesignPoint::Right => (x1, cy),
            DesignPoint::BottomLeft => (x0, y1),
            DesignPoint::Bottom => (cx, y1),
            DesignPoint::BottomRight => (x1, y1),
        };
        Point::new(x, y)
    }
}

/// The overlap where shapes touch (mm). With the pull compensation the stitches add, about 1 mm
/// in all, as digitizers leave it; more piles up layers where three shapes meet (measured on the
/// first sew-out's design: 0.5 mm closed the gap and added no dense spots, 0.7 mm and 1 mm added
/// nine to twelve).
pub const OVERLAP_TOUCHING: f64 = 0.5;

impl Default for DesignSettings {
    fn default() -> Self {
        DesignSettings {
            trim_threshold: 2.0,
            hide_travel: true,
            overlap_touching: OVERLAP_TOUCHING,
            tie_in: true,
            tie_off: true,
            tie_length: 0.8,
            tie_style: TieStyle::Double,
            min_stitch: 0.3,
            max_stitch: 12.0,
            max_jump: 12.0,
            machine_speed: 800.0,
            trim_seconds: 6.0,
            color_change_seconds: 20.0,
            basting: None,
            start: None,
            end: None,
            registration: RegistrationMarks::default(),
            hoop_offset: None,
        }
    }
}

/// A basting run: long running stitches sewn first, in the first thread and without ties, to
/// hold the fabric to the stabiliser; picked out after. It goes round the design `margin` out
/// from it, or round the hoop's field `margin` in from its edge (the hoop centred on the design).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Basting {
    /// Round the hoop rather than the design (the design when there is no hoop).
    pub hoop: bool,
    /// mm out from the design, or in from the hoop's edge.
    pub margin: f64,
    /// Stitch length (mm).
    pub length: f64,
}

impl Default for Basting {
    fn default() -> Self {
        Basting { hoop: false, margin: 5.0, length: 6.0 }
    }
}

impl Basting {
    /// The run round a design whose outlines span `bbox` (mm): its needle points, closed (the
    /// last on the first). None when there is nothing to go round or the hoop leaves no room.
    pub fn stitches(&self, bbox: BBox, hoop: Option<&Hoop>) -> Option<Vec<Stitch>> {
        if bbox.is_empty() || ![self.margin, self.length].iter().all(|v| v.is_finite()) {
            return None;
        }
        let c = Point::new((bbox.min.x + bbox.max.x) / 2.0, (bbox.min.y + bbox.max.y) / 2.0);
        let ring: Vec<Point> = match hoop.filter(|_| self.hoop) {
            Some(h) => {
                let (hw, hh) = (h.width / 2.0 - self.margin, h.height / 2.0 - self.margin);
                if !(hw > 1.0 && hh > 1.0) {
                    return None;
                }
                if h.is_elliptical() {
                    (0..=48)
                        .map(|k| {
                            let a = std::f64::consts::TAU * f64::from(k) / 48.0;
                            Point::new(c.x + hw * a.cos(), c.y + hh * a.sin())
                        })
                        .collect()
                } else {
                    corners(c, hw, hh)
                }
            }
            None => corners(c, bbox.width() / 2.0 + self.margin.max(0.0), bbox.height() / 2.0 + self.margin.max(0.0)),
        };
        let step = self.length.clamp(1.0, 12.0);
        let mut out = Vec::new();
        for w in ring.windows(2) {
            let (a, b) = (w[0], w[1]);
            if out.is_empty() {
                out.push(Stitch::at(a, crate::pattern::Cmd::Stitch));
            }
            let n = (a.distance(b) / step).ceil().clamp(1.0, 10_000.0) as usize;
            for k in 1..=n {
                out.push(Stitch::at(a.lerp(b, k as f64 / n as f64), crate::pattern::Cmd::Stitch));
            }
        }
        (out.len() > 1).then_some(out)
    }
}

/// A rectangle's corners round `c`, closed.
fn corners(c: Point, hw: f64, hh: f64) -> Vec<Point> {
    vec![
        Point::new(c.x - hw, c.y - hh),
        Point::new(c.x + hw, c.y - hh),
        Point::new(c.x + hw, c.y + hh),
        Point::new(c.x - hw, c.y + hh),
        Point::new(c.x - hw, c.y - hh),
    ]
}

/// How tie-ins and tie-offs lock the thread: small stitches sewn in place where a
/// thread starts and before it is cut.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TieStyle {
    /// One stitch out and back: the lightest lock.
    Single,
    /// Out and back twice.
    #[default]
    Double,
    /// A small triangle: out along the line, across it and back, locking on fabric
    /// that frays or stretches.
    Triangle,
}

impl TieStyle {
    pub const ALL: [TieStyle; 3] = [TieStyle::Single, TieStyle::Double, TieStyle::Triangle];

    pub fn label(self) -> &'static str {
        match self {
            TieStyle::Single => "Single",
            TieStyle::Double => "Double",
            TieStyle::Triangle => "Triangle",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            TieStyle::Single => "single",
            TieStyle::Double => "double",
            TieStyle::Triangle => "triangle",
        }
    }

    pub fn is_default(&self) -> bool {
        *self == TieStyle::default()
    }

    /// The needle points of a tie at `at`, heading along the unit vector `u`, `t` long
    /// (it ends back at `at`).
    pub fn points(self, at: Point, u: Point, t: f64) -> Vec<Point> {
        let q = at + u * t;
        match self {
            TieStyle::Single => vec![q, at],
            TieStyle::Double => vec![q, at, q, at],
            TieStyle::Triangle => vec![q, at + u * (t * 0.5) + Point::new(-u.y, u.x) * (t * 0.6), at],
        }
    }
}

/// Bitmap artwork shown behind the design.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Backdrop {
    pub name: String,
    /// The encoded image file (PNG, JPEG, ...); base64 in JSON.
    #[serde(with = "b64")]
    pub data: Vec<u8>,
    /// Top-left corner in design space.
    pub min: Point,
    /// Displayed size in mm.
    pub width: f64,
    pub height: f64,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
}

fn default_opacity() -> f32 {
    0.6
}

mod b64 {
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD.decode(s.as_bytes()).map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Objects
// ---------------------------------------------------------------------------

/// A chenille machine's two stitches (CH-01): chain, link-like loops on top for outlines,
/// borders and light fills, and moss, a dense looped pile for fills (coil and square-loop
/// patterns). Public description:
/// https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Chenille/chenille_basics/chenille_basics-3.htm
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChenilleStitch {
    Chain,
    Moss,
}

impl ChenilleStitch {
    pub fn id(self) -> &'static str {
        match self {
            ChenilleStitch::Chain => "chain",
            ChenilleStitch::Moss => "moss",
        }
    }
}

/// How an object sews on a chenille machine: chain or moss, at a needle height (the machine's
/// own levels, 1 to 9: lower for outlines, higher for fills).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Chenille {
    pub stitch: ChenilleStitch,
    #[serde(default = "chenille_height")]
    pub height: u8,
}

/// Needle heights a chenille object can take.
pub const CHENILLE_HEIGHTS: std::ops::RangeInclusive<u8> = 1..=9;

fn chenille_height() -> u8 {
    5
}

impl Chenille {
    pub fn new(stitch: ChenilleStitch, height: u8) -> Self {
        Chenille { stitch, height: height.clamp(*CHENILLE_HEIGHTS.start(), *CHENILLE_HEIGHTS.end()) }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmbObject {
    pub id: ObjectId,
    #[serde(default)]
    pub name: String,
    /// Palette index.
    pub color: usize,
    pub kind: ObjectKind,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    /// Objects sharing a group id are selected and transformed together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u64>,
    /// Preferred start point (the closest valid point is used). `None` = automatic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<Point>,
    /// Preferred end point. `None` = automatic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<Point>,
    /// Always trim after this object, even when the next one is close.
    #[serde(default)]
    pub trim_after: bool,
    /// Lock stitches at the start of this object: `None` follows the design setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tie_in: Option<bool>,
    /// Lock stitches before the thread is cut after this object: `None` follows the
    /// design setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tie_off: Option<bool>,
    /// Stop the machine after this object.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stop_after: bool,
    /// Start a colour block of its own, even in the colour sewn just before.
    #[serde(default, skip_serializing_if = "is_false")]
    pub own_block: bool,
    /// Leave out the object's last needle point.
    #[serde(default, skip_serializing_if = "is_false")]
    pub omit_last: bool,
    /// Objects sharing a branch id are sewn as one path: the thread travels from one
    /// to the next as running stitches where later ones of the branch cover it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<u64>,
    /// Sewn on a chenille machine as chain or moss (`None`: a lockstitch object).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chenille: Option<Chenille>,
}

impl EmbObject {
    pub fn new(id: ObjectId, kind: ObjectKind, color: usize) -> Self {
        let name = kind.type_name().to_string();
        EmbObject {
            id,
            name,
            color,
            kind,
            visible: true,
            locked: false,
            group: None,
            entry: None,
            exit: None,
            trim_after: false,
            tie_in: None,
            tie_off: None,
            stop_after: false,
            own_block: false,
            omit_last: false,
            branch: None,
            chenille: None,
        }
    }

    /// Outline bounds.
    pub fn bbox(&self) -> BBox {
        self.kind.bbox()
    }

    /// Applies an affine transform to the geometry, keeping stitch angles attached
    /// to the shape (a rotated fill keeps its stitch direction relative to the shape).
    pub fn transform(&mut self, m: &Affine) {
        self.kind.transform(m);
        if let Some(p) = &mut self.entry {
            *p = m.apply(*p);
        }
        if let Some(p) = &mut self.exit {
            *p = m.apply(*p);
        }
    }

    pub fn type_name(&self) -> &'static str {
        self.kind.type_name()
    }
}

/// The geometry + stitch type of an object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ObjectKind {
    /// Running-stitch style line along an open or closed contour.
    Run(RunObject),
    /// Satin column between two rails.
    Satin(SatinObject),
    /// Closed shape (with holes) filled with tatami, satin, contour, ... stitches.
    Fill(FillObject),
    /// Appliqué: placement line, tack-down and cover stitch around a fabric piece.
    Applique(AppliqueObject),
    /// Text converted to stitches.
    Lettering(LetteringObject),
    /// Raw stitches (imported machine files or manual stitching).
    Stitches(StitchBlock),
    /// Line art sewn as one line that starts and ends at the same point (redwork).
    Redwork(RedworkObject),
    /// Sequins, beads, rhinestones, cord or boring holes.
    Decor(crate::decor::DecorObject),
}

impl ObjectKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            ObjectKind::Run(r) => match r.params.kind {
                RunKind::Single => "Run",
                RunKind::Triple => "Triple Run",
                RunKind::Blanket => "Blanket",
                RunKind::Zigzag => "Zigzag",
                RunKind::Motif => "Motif Run",
                RunKind::Back => "Backstitch",
                RunKind::Stem => "Stem Stitch",
                RunKind::Hand => "Hand Stitch",
                RunKind::Sculpture => "Sculpture Run",
                RunKind::String => "String Stitch",
            },
            ObjectKind::Satin(_) => "Satin Column",
            ObjectKind::Fill(f) => f.params.kind.label(),
            ObjectKind::Applique(_) => "Appliqué",
            ObjectKind::Lettering(_) => "Lettering",
            ObjectKind::Stitches(_) => "Stitches",
            ObjectKind::Redwork(_) => "Redwork",
            ObjectKind::Decor(d) => d.params.kind.label(),
        }
    }

    pub fn bbox(&self) -> BBox {
        match self {
            ObjectKind::Run(r) => r.path.bbox(),
            ObjectKind::Satin(s) => s.column.bbox(),
            ObjectKind::Fill(f) => f.shape.bbox(),
            ObjectKind::Applique(a) => a.pieces().fold(BBox::EMPTY, |b, s| b.union(s.bbox())),
            ObjectKind::Lettering(l) => l.glyphs.iter().fold(BBox::EMPTY, |b, g| b.union(g.shape.bbox())),
            ObjectKind::Stitches(s) => BBox::from_points(s.stitches.iter().filter(|s| s.is_move()).map(Stitch::pos)),
            ObjectKind::Redwork(r) => r.lines.iter().fold(BBox::EMPTY, |b, c| b.union(c.bbox())),
            ObjectKind::Decor(d) => BBox::from_points(crate::decor::outlines(d).into_iter().flatten()),
        }
    }

    pub fn transform(&mut self, m: &Affine) {
        match self {
            ObjectKind::Run(r) => r.path.transform(m),
            ObjectKind::Satin(s) => s.column.transform(m),
            ObjectKind::Fill(f) => {
                f.shape.transform(m);
                f.params.angle = transform_angle(f.params.angle, m);
                if let Some(c) = &mut f.params.center {
                    *c = m.apply(*c);
                }
                for g in &mut f.guides {
                    g.transform(m);
                }
                for p in f.splits.iter_mut().chain(f.flow.iter_mut()).flatten() {
                    *p = m.apply(*p);
                }
            }
            ObjectKind::Applique(a) => {
                a.shape.transform(m);
                for s in &mut a.extra {
                    s.transform(m);
                }
            }
            ObjectKind::Lettering(l) => {
                l.transform = l.transform.then(m);
                for g in &mut l.glyphs {
                    g.shape.transform(m);
                    g.angle = transform_angle(g.angle, m);
                }
            }
            ObjectKind::Stitches(s) => {
                for st in &mut s.stitches {
                    let p = m.apply(st.pos());
                    st.x = p.x;
                    st.y = p.y;
                }
            }
            ObjectKind::Redwork(r) => {
                for c in &mut r.lines {
                    c.transform(m);
                }
            }
            ObjectKind::Decor(d) => {
                let a = transform_angle(0.0, m).to_radians();
                for it in &mut d.items {
                    it.at = m.apply(it.at);
                    it.angle += a;
                }
                for (l, _) in &mut d.lines {
                    for p in l.iter_mut() {
                        *p = m.apply(*p);
                    }
                }
            }
        }
    }

    /// Outline polylines for display: closed rings are returned explicitly closed.
    pub fn outlines(&self) -> Vec<Vec<Point>> {
        let tol = FLATTEN_TOLERANCE;
        let ring = |c: &Contour| crate::geom::polyline::closed(&c.polyline(tol));
        match self {
            ObjectKind::Run(r) => {
                if r.path.closed {
                    vec![ring(&r.path)]
                } else {
                    vec![r.path.polyline(tol)]
                }
            }
            ObjectKind::Satin(s) => vec![crate::geom::polyline::closed(&s.column.outline(tol))],
            ObjectKind::Fill(f) => std::iter::once(&f.shape.outer).chain(&f.shape.holes).map(ring).collect(),
            ObjectKind::Applique(a) => a.pieces().flat_map(|s| std::iter::once(&s.outer).chain(&s.holes).map(ring).collect::<Vec<_>>()).collect(),
            ObjectKind::Lettering(l) => {
                l.glyphs.iter().flat_map(|g| std::iter::once(&g.shape.outer).chain(&g.shape.holes).map(ring).collect::<Vec<_>>()).collect()
            }
            ObjectKind::Stitches(s) => vec![s.stitches.iter().filter(|s| s.is_move()).map(Stitch::pos).collect()],
            ObjectKind::Redwork(r) => r.lines.iter().map(|c| if c.closed { ring(c) } else { c.polyline(tol) }).collect(),
            ObjectKind::Decor(d) => crate::decor::outlines(d),
        }
    }
}

/// Maps a stitch angle through a transform (result normalised to `[0, 180)`).
pub fn transform_angle(degrees: f64, m: &Affine) -> f64 {
    let v = m.apply_vector(screen_dir(degrees));
    if v.length() < 1e-12 {
        return degrees;
    }
    normalize_line_angle(screen_angle(v))
}

/// Normalises a line direction angle to `[0, 180)`.
pub fn normalize_line_angle(deg: f64) -> f64 {
    let a = deg.rem_euclid(180.0);
    if (a - 180.0).abs() < 1e-9 { 0.0 } else { a }
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RunKind {
    /// Single running stitch.
    #[default]
    Single,
    /// Bean / triple stitch: every stitch sewn forward, back and forward again.
    Triple,
    /// Blanket (E-stitch): running stitch with prongs to one side.
    Blanket,
    /// Open zigzag along the line.
    Zigzag,
    /// Decorative repeating motif along the line.
    Motif,
    /// Hand-sewn look: each stitch goes forward two steps and back one, so
    /// every stitch overlaps the last.
    Back,
    /// Rope-like hand-embroidery outline: like backstitch, with the stitches
    /// slanted across the line.
    Stem,
    /// Hand-sewn running stitch: thick stitches (each sewn three times) with short thin ones
    /// between them, lengths varying a little, like a running stitch sewn by hand.
    Hand,
    /// Sculpture run: a thick-thin line, groups of triple stitches in turn with groups of
    /// single ones, each with its own stitch length.
    Sculpture,
    /// String stitching (string art, as in Paghadi work): long straight stitches spanning
    /// `width` mm of the line, one starting every `spacing` mm, overlapping; on curves they
    /// draw a curve inside the line.
    String,
}

impl RunKind {
    pub const ALL: [RunKind; 10] = [
        RunKind::Single,
        RunKind::Triple,
        RunKind::Blanket,
        RunKind::Zigzag,
        RunKind::Motif,
        RunKind::Back,
        RunKind::Stem,
        RunKind::Hand,
        RunKind::Sculpture,
        RunKind::String,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RunKind::Single => "Single run",
            RunKind::Triple => "Triple run",
            RunKind::Blanket => "Blanket",
            RunKind::Zigzag => "Zigzag",
            RunKind::Motif => "Motif run",
            RunKind::Back => "Backstitch",
            RunKind::Stem => "Stem stitch",
            RunKind::Hand => "Hand stitch",
            RunKind::Sculpture => "Sculpture run",
            RunKind::String => "String stitch",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Motif {
    #[default]
    Wave,
    Scallop,
    Diamond,
    Cross,
    Feather,
    Heart,
    Star,
    Leaf,
    Loop,
    /// An open zigzag: in a motif fill, a zigzag fill.
    Zigzag,
    /// A blanket prong: in a motif fill, an E-stitch fill.
    EStitch,
    /// Chenille look on a regular machine: overlapping round loops, like a coiled spring.
    Coil,
    /// Chenille look on a regular machine: square loops, each crossing the last.
    SquareLoop,
    /// Blackwork: a stepped geometric repeat, like counted-thread blackwork borders.
    Blackwork,
    /// Candlewicking: a small knot of crossing stitches at each repeat, the line between.
    Candlewick,
}

impl Motif {
    pub const ALL: [Motif; 15] = [
        Motif::Wave,
        Motif::Scallop,
        Motif::Diamond,
        Motif::Cross,
        Motif::Feather,
        Motif::Heart,
        Motif::Star,
        Motif::Leaf,
        Motif::Loop,
        Motif::Zigzag,
        Motif::EStitch,
        Motif::Coil,
        Motif::SquareLoop,
        Motif::Blackwork,
        Motif::Candlewick,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Motif::Wave => "Wave",
            Motif::Scallop => "Scallop",
            Motif::Diamond => "Diamond",
            Motif::Cross => "Cross",
            Motif::Feather => "Feather",
            Motif::Heart => "Heart",
            Motif::Star => "Star",
            Motif::Leaf => "Leaf",
            Motif::Loop => "Loop",
            Motif::Zigzag => "Zigzag",
            Motif::EStitch => "E-stitch",
            Motif::Coil => "Coil",
            Motif::SquareLoop => "Square loop",
            Motif::Blackwork => "Blackwork",
            Motif::Candlewick => "Candlewick",
        }
    }
}

/// Most points a custom motif keeps.
pub const MAX_MOTIF_POINTS: usize = 2000;

/// A motif made from a line drawn in the design (Make Motif). Its points lie in a repeat one long
/// (along the run) and one wide (across it), from `(0, 0)` to `(1, 0)` like the built-in motifs;
/// across is positive to the right of the run, down on screen for a left-to-right line, so the
/// motif sews the way it was drawn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomMotif {
    pub name: String,
    pub points: Vec<[f64; 2]>,
    /// The drawn motif's height over its length, which motif fills and Apply Motif keep.
    pub aspect: f64,
}

impl Default for CustomMotif {
    fn default() -> Self {
        CustomMotif { name: String::new(), points: Vec::new(), aspect: 1.0 }
    }
}

impl CustomMotif {
    /// The height over the length to sew it at: [`CustomMotif::aspect`] when it is sensible.
    pub fn aspect(&self) -> f64 {
        if self.aspect.is_finite() { self.aspect.clamp(0.05, 20.0) } else { 1.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunParams {
    pub kind: RunKind,
    /// Needle step along the line (mm).
    pub stitch_length: f64,
    /// Blanket prong length / zigzag width / motif width (mm).
    pub width: f64,
    /// Distance between blanket prongs / zigzag peaks / motif repeats, the gap between hand
    /// stitches, or a sculpture run's single stitch length (mm).
    pub spacing: f64,
    /// Put blanket prongs / motifs / stem slants on the other side of the line.
    pub flip: bool,
    pub motif: Motif,
    /// Times the line is sewn, alternating direction over the same needle points:
    /// 1 = once, 2 = backtrack (ends back at the start), 3 or more = repeats.
    pub passes: u32,
    /// Graded motif runs: the motifs' width at the end of the line (mm); they grow or shrink
    /// to it from `width`, their repeats with them. `None` = all the same size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_end: Option<f64>,
    /// A motif made from a drawn line, sewn instead of `motif`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_motif: Option<CustomMotif>,
    /// Sculpture runs: how many triple stitches (`stitch_length` long), then how many single
    /// ones (`spacing` long), in turn.
    #[serde(skip_serializing_if = "is_sculpture_default")]
    pub groups: [u32; 2],
}

/// A sculpture run's groups unless set: three triple stitches, then one single.
pub const SCULPTURE_GROUPS: [u32; 2] = [3, 1];

fn is_sculpture_default(g: &[u32; 2]) -> bool {
    *g == SCULPTURE_GROUPS
}

/// Most passes a run can be sewn.
pub const MAX_PASSES: u32 = 9;

impl Default for RunParams {
    fn default() -> Self {
        RunParams {
            kind: RunKind::Single,
            stitch_length: 2.5,
            width: 2.0,
            spacing: 2.0,
            flip: false,
            motif: Motif::Wave,
            passes: 1,
            width_end: None,
            custom_motif: None,
            groups: SCULPTURE_GROUPS,
        }
    }
}

/// Line art sewn as one line (redwork): every line once in the run style, the way between them
/// retraced along lines already sewn, ending where it starts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RedworkObject {
    pub lines: Vec<Contour>,
    #[serde(default)]
    pub params: RunParams,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RunObject {
    pub path: Contour,
    #[serde(default)]
    pub params: RunParams,
}

// ---------------------------------------------------------------------------
// Satin
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SatinUnderlay {
    /// Choose underlay from the column width (ignores the flags below).
    pub auto: bool,
    pub center_run: bool,
    pub edge_run: bool,
    pub zigzag: bool,
    /// Inset of edge-run and zigzag underlay from the rails (mm).
    pub inset: f64,
    /// Zigzag underlay spacing (mm).
    pub zigzag_spacing: f64,
    /// Stitch length of run underlays (mm).
    pub stitch_length: f64,
    /// Sew the zigzag underlay there and back, the second pass crossing the
    /// first (a double zigzag).
    pub double_zigzag: bool,
}

impl Default for SatinUnderlay {
    fn default() -> Self {
        SatinUnderlay {
            auto: true,
            center_run: true,
            edge_run: false,
            zigzag: false,
            inset: 0.4,
            zigzag_spacing: 2.0,
            stitch_length: 2.5,
            double_zigzag: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SatinParams {
    /// Distance between needle penetrations along the rails (mm).
    pub spacing: f64,
    /// Extra width on each side against fabric pull-in (mm); with `pull_comp_2`
    /// set, the extra width on side 1 (the first rail) only.
    pub pull_comp: f64,
    /// Twin-sided pull compensation: the extra width on side 2 (the second rail),
    /// e.g. less where the column lies on other stitching. `None` = `pull_comp`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pull_comp_2: Option<f64>,
    /// Push compensation: the column ends this much short at each open end (mm), where the
    /// stitches push the fabric out along the column.
    pub push_comp: f64,
    /// Start the zigzag on side 2 instead of side 1, so the column ends on the other side
    /// (for a shorter way on to what is sewn next). Each side keeps its own settings.
    pub swap_sides: bool,
    /// Tighten spacing on narrow columns and open it on wide ones.
    pub auto_spacing: bool,
    /// Split stitches longer than this (mm) into staggered shorter ones; 0 = never.
    pub split_length: f64,
    /// Shorten alternate stitches on the inside of tight curves.
    pub short_stitches: bool,
    /// Keep stitches longer than the machine's longest stitch whole: the frame
    /// moves in steps without a needle drop, so the thread lies as one stitch.
    #[serde(skip_serializing_if = "is_false")]
    pub preserve_long: bool,
    /// How satin lines join at sharp corners.
    #[serde(skip_serializing_if = "Corners::is_auto")]
    pub corners: Corners,
    /// Sew the column in tatami rows instead of satin (a tatami border, for columns too
    /// wide for satin): every stitch across is split into pieces `split_length` long
    /// ([`TATAMI_LENGTH`] when that is 0), shifted a quarter from row to row.
    #[serde(skip_serializing_if = "is_false")]
    pub tatami: bool,
    /// Jagged edges: stitches end short of the edge by a random amount (fur, grass, feathers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jagged: Option<Jagged>,
    pub underlay: SatinUnderlay,
    /// Sew the column as motifs instead of satin (a graded motif column): along its middle, each
    /// as wide as the column where it sits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motif_column: Option<Box<MotifColumn>>,
    /// 3D puff: sewn over foam, its ends capped (see [`Puff`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub puff: Option<Puff>,
    /// Variable density: the spacing along the column as `[t, mm]` points, `t` from 0 at its
    /// start to 1 at its end, straight between them (denser where it turns, lighter where it
    /// fades out). Empty: `spacing` all along.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spacing_along: Vec<[f64; 2]>,
    /// Random density: each row's spacing changed by up to this share of it (0–0.9), at random
    /// but the same every time, for a hand-sewn texture. 0: even.
    #[serde(skip_serializing_if = "is_zero_f64")]
    pub random_density: f64,
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

/// 3D puff: satin sewn over foam, dense enough to cut it along the column's sides, with caps
/// across the column's ends that reach past them so the foam is cut and covered there too (public
/// descriptions of the practice: https://wilcom.com/resources/blog/qa-from-3d-puff-embroidery-webinar
/// and https://hatchembroidery.com/resources/blog/how-to-digitize-for-3d-puffy-foam-machine-embroidery).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Puff {
    /// How far the end caps reach past a column's ends (mm).
    pub cap: f64,
    /// Made from a fill (a filled shape sewn as satin across it over the foam): plain again, it
    /// fills again.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub fill: bool,
}

impl Default for Puff {
    fn default() -> Self {
        Puff { cap: 0.5, fill: false }
    }
}

impl Puff {
    /// Satin spacing over foam (mm).
    pub const SPACING: f64 = 0.16;
    /// How far the sides are pushed out (mm): the foam pushes the thread, it doesn't pull in.
    pub const PUSH: f64 = 0.35;
    /// The narrowest and widest columns that puff well (mm).
    pub const WIDTHS: (f64, f64) = (3.0, 7.0);

    /// `params` set for sewing over foam: dense, without underlay (it would flatten the foam),
    /// auto split or short stitches (both cut the foam inside the column), pushed out at the
    /// sides, its ends capped.
    pub fn apply(self, params: &mut SatinParams) {
        params.spacing = Puff::SPACING;
        params.auto_spacing = false;
        params.split_length = 0.0;
        params.short_stitches = false;
        params.pull_comp = Puff::PUSH;
        params.pull_comp_2 = None;
        params.tatami = false;
        params.jagged = None;
        params.motif_column = None;
        params.underlay.auto = false;
        params.underlay.center_run = false;
        params.underlay.edge_run = false;
        params.underlay.zigzag = false;
        params.underlay.double_zigzag = false;
        params.puff = Some(Puff { cap: self.cap.clamp(0.0, 3.0), ..self });
    }

    /// `params` back to plain satin: what [`Puff::apply`] set, as satin has it by default.
    pub fn remove(params: &mut SatinParams) {
        let d = SatinParams::default();
        params.spacing = d.spacing;
        params.auto_spacing = d.auto_spacing;
        params.split_length = d.split_length;
        params.short_stitches = d.short_stitches;
        params.pull_comp = d.pull_comp;
        params.underlay = d.underlay;
        params.puff = None;
    }
}

/// A satin column sewn as motifs: along its middle, each motif as wide as the column where it
/// starts (a tenth less, so it stays inside) and `length` times as long, upright along a column
/// drawn left to right.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MotifColumn {
    pub motif: Motif,
    /// A motif made from a drawn line, sewn instead of `motif`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_motif: Option<CustomMotif>,
    /// A motif's length over its width.
    pub length: f64,
    /// Longest stitch (mm).
    pub stitch_length: f64,
}

impl Default for MotifColumn {
    fn default() -> Self {
        MotifColumn { motif: Motif::Leaf, custom_motif: None, length: 1.5, stitch_length: 1.5 }
    }
}

/// The side of a satin column whose stitches end short at random (jagged edges).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JaggedSides {
    /// Both sides.
    #[default]
    Both,
    /// Side 1 (the first rail).
    First,
    /// Side 2 (the second rail).
    Second,
}

impl JaggedSides {
    pub const ALL: [JaggedSides; 3] = [JaggedSides::Both, JaggedSides::First, JaggedSides::Second];

    /// Whether the side of the first rail (`first`) or the other one is jagged.
    pub fn covers(self, first: bool) -> bool {
        match self {
            JaggedSides::Both => true,
            JaggedSides::First => first,
            JaggedSides::Second => !first,
        }
    }
}

/// Jagged edges of a satin column: each stitch on a jagged side ends short of the edge by a
/// random amount up to `depth`, so the edge looks ragged.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Jagged {
    pub sides: JaggedSides,
    /// Most a stitch ends short of the edge (mm).
    pub depth: f64,
}

impl Default for Jagged {
    fn default() -> Self {
        Jagged { sides: JaggedSides::Both, depth: 1.5 }
    }
}

/// The stitch length of a tatami border when none is set (mm).
pub const TATAMI_LENGTH: f64 = 4.0;

/// The shortest auto-split or tatami piece (mm) (#84). A shorter `split_length` from a file,
/// a paste or a command is raised to it: the stitch count grows as one over the split length,
/// so a tiny value would make billions of stitches.
pub const MIN_SPLIT_LENGTH: f64 = 0.5;

/// The split length actually used for `split_length`: at least [`MIN_SPLIT_LENGTH`] when it is
/// positive, 0 (never split) when it is 0, negative or not a number.
pub fn effective_split_length(split_length: f64) -> f64 {
    if split_length > 0.0 { split_length.max(MIN_SPLIT_LENGTH) } else { 0.0 }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// How a satin line's two sides of a sharp corner meet (smart corners). Rows that
/// simply turned with the line would bunch up in the inside of the corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Corners {
    /// By the angle: rows turn round gentle corners, meet on a miter at medium
    /// ones and are mitered with the tip capped at sharp ones.
    #[default]
    Auto,
    /// Rows fan round the inner corner point.
    Turn,
    /// Each side keeps its rows square to it; they meet on the line from the inner
    /// to the outer corner point.
    Miter,
    /// A miter with the outer tip cut off square, for sharp corners.
    Cap,
}

impl Corners {
    pub const ALL: [Corners; 4] = [Corners::Auto, Corners::Turn, Corners::Miter, Corners::Cap];

    pub fn label(self) -> &'static str {
        match self {
            Corners::Auto => "Automatic",
            Corners::Turn => "Turn",
            Corners::Miter => "Miter",
            Corners::Cap => "Capped",
        }
    }

    fn is_auto(&self) -> bool {
        *self == Corners::Auto
    }
}

impl SatinParams {
    /// Pull compensation on (side 1, side 2).
    pub fn pull_sides(&self) -> (f64, f64) {
        (self.pull_comp, self.pull_comp_2.unwrap_or(self.pull_comp))
    }
}

impl Default for SatinParams {
    fn default() -> Self {
        SatinParams {
            spacing: 0.4,
            pull_comp: 0.15,
            pull_comp_2: None,
            push_comp: 0.0,
            swap_sides: false,
            auto_spacing: true,
            split_length: 8.0,
            short_stitches: true,
            preserve_long: false,
            corners: Corners::Auto,
            tatami: false,
            jagged: None,
            underlay: SatinUnderlay::default(),
            motif_column: None,
            puff: None,
            spacing_along: Vec::new(),
            random_density: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SatinObject {
    pub column: Column,
    #[serde(default)]
    pub params: SatinParams,
}

// ---------------------------------------------------------------------------
// Fill
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FillKind {
    /// Rows of staggered running stitches (a.k.a. "fill" / "step").
    #[default]
    Tatami,
    /// Long satin stitches across the shape (auto-split when too long).
    Satin,
    /// Rows following the outline, shrinking towards the centre.
    Contour,
    /// One continuous spiral from the outside in.
    Spiral,
    /// Rays from a centre point to the outline.
    Radial,
    /// Cross stitches on a grid.
    Cross,
    /// A continuous quilting-style meander that wanders through the shape.
    Meander,
    /// Rows of motifs (a motif run on each row) across the shape.
    Motif,
}

impl FillKind {
    pub const ALL: [FillKind; 8] = [
        FillKind::Tatami,
        FillKind::Satin,
        FillKind::Contour,
        FillKind::Spiral,
        FillKind::Radial,
        FillKind::Cross,
        FillKind::Meander,
        FillKind::Motif,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FillKind::Tatami => "Tatami Fill",
            FillKind::Satin => "Satin Fill",
            FillKind::Contour => "Contour Fill",
            FillKind::Spiral => "Spiral Fill",
            FillKind::Radial => "Radial Fill",
            FillKind::Cross => "Cross Fill",
            FillKind::Meander => "Meander Fill",
            FillKind::Motif => "Motif Fill",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FillUnderlay {
    /// Run around the shape just inside the outline.
    pub edge_run: bool,
    /// Sparse tatami across the shape.
    pub tatami: bool,
    /// Distance of the underlay from the outline (mm).
    pub inset: f64,
    /// Row spacing of tatami underlay (mm).
    pub spacing: f64,
    pub stitch_length: f64,
    /// Underlay direction relative to the fill angle (degrees).
    pub angle_offset: f64,
}

impl Default for FillUnderlay {
    fn default() -> Self {
        FillUnderlay { edge_run: true, tatami: true, inset: 0.5, spacing: 2.5, stitch_length: 3.0, angle_offset: 90.0 }
    }
}

/// A programmed split pattern: the needle points of a tatami fill follow its lines, so the fill
/// shows a texture. Generated in code and tiled over the fill.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TexturePattern {
    #[default]
    Diamonds,
    Waves,
    Herringbone,
    Bricks,
    Circles,
    Scales,
    Hexagons,
    Chevrons,
    Zigzag,
    Ogee,
    BasketWeave,
    Stars,
    Hearts,
    Leaves,
    Stripes,
    Slants,
    Pinstripes,
    Lattice,
    Arches,
    Rings,
    Dots,
    Spirals,
    Pyramids,
    Ripples,
    GreekKey,
    Crosses,
    Quatrefoils,
    Flowers,
    Trefoils,
    Moons,
    Arrows,
    NestedDiamonds,
    Argyle,
    Wicker,
    Feathers,
    Paisley,
    Swirls,
    Snowflakes,
    Bubbles,
    Sunbursts,
}

impl TexturePattern {
    pub const ALL: [TexturePattern; 40] = [
        TexturePattern::Diamonds,
        TexturePattern::Waves,
        TexturePattern::Herringbone,
        TexturePattern::Bricks,
        TexturePattern::Circles,
        TexturePattern::Scales,
        TexturePattern::Hexagons,
        TexturePattern::Chevrons,
        TexturePattern::Zigzag,
        TexturePattern::Ogee,
        TexturePattern::BasketWeave,
        TexturePattern::Stars,
        TexturePattern::Hearts,
        TexturePattern::Leaves,
        TexturePattern::Stripes,
        TexturePattern::Slants,
        TexturePattern::Pinstripes,
        TexturePattern::Lattice,
        TexturePattern::Arches,
        TexturePattern::Rings,
        TexturePattern::Dots,
        TexturePattern::Spirals,
        TexturePattern::Pyramids,
        TexturePattern::Ripples,
        TexturePattern::GreekKey,
        TexturePattern::Crosses,
        TexturePattern::Quatrefoils,
        TexturePattern::Flowers,
        TexturePattern::Trefoils,
        TexturePattern::Moons,
        TexturePattern::Arrows,
        TexturePattern::NestedDiamonds,
        TexturePattern::Argyle,
        TexturePattern::Wicker,
        TexturePattern::Feathers,
        TexturePattern::Paisley,
        TexturePattern::Swirls,
        TexturePattern::Snowflakes,
        TexturePattern::Bubbles,
        TexturePattern::Sunbursts,
    ];

    /// The pattern's id in params and files.
    pub fn id(self) -> &'static str {
        match self {
            TexturePattern::Diamonds => "diamonds",
            TexturePattern::Waves => "waves",
            TexturePattern::Herringbone => "herringbone",
            TexturePattern::Bricks => "bricks",
            TexturePattern::Circles => "circles",
            TexturePattern::Scales => "scales",
            TexturePattern::Hexagons => "hexagons",
            TexturePattern::Chevrons => "chevrons",
            TexturePattern::Zigzag => "zigzag",
            TexturePattern::Ogee => "ogee",
            TexturePattern::BasketWeave => "basketweave",
            TexturePattern::Stars => "stars",
            TexturePattern::Hearts => "hearts",
            TexturePattern::Leaves => "leaves",
            TexturePattern::Stripes => "stripes",
            TexturePattern::Slants => "slants",
            TexturePattern::Pinstripes => "pinstripes",
            TexturePattern::Lattice => "lattice",
            TexturePattern::Arches => "arches",
            TexturePattern::Rings => "rings",
            TexturePattern::Dots => "dots",
            TexturePattern::Spirals => "spirals",
            TexturePattern::Pyramids => "pyramids",
            TexturePattern::Ripples => "ripples",
            TexturePattern::GreekKey => "greekkey",
            TexturePattern::Crosses => "crosses",
            TexturePattern::Quatrefoils => "quatrefoils",
            TexturePattern::Flowers => "flowers",
            TexturePattern::Trefoils => "trefoils",
            TexturePattern::Moons => "moons",
            TexturePattern::Arrows => "arrows",
            TexturePattern::NestedDiamonds => "nesteddiamonds",
            TexturePattern::Argyle => "argyle",
            TexturePattern::Wicker => "wicker",
            TexturePattern::Feathers => "feathers",
            TexturePattern::Paisley => "paisley",
            TexturePattern::Swirls => "swirls",
            TexturePattern::Snowflakes => "snowflakes",
            TexturePattern::Bubbles => "bubbles",
            TexturePattern::Sunbursts => "sunbursts",
        }
    }

    /// The tile width (mm) the pattern reads best at with stitch length `stitch_length`: bricks
    /// with their joints a long stitch apart (no stitches between them), scales large enough for
    /// their flat bottoms to show.
    pub fn default_size(self, stitch_length: f64) -> f64 {
        let l = if stitch_length.is_finite() { stitch_length } else { 4.0 };
        match self {
            TexturePattern::Bricks => (l * 1.3).clamp(3.0, 12.0),
            TexturePattern::Scales | TexturePattern::Hexagons => (l * 2.5).clamp(4.0, 14.0),
            // pictures need a few rows to read
            TexturePattern::Stars | TexturePattern::Hearts | TexturePattern::Leaves => (l * 3.0).clamp(6.0, 18.0),
            TexturePattern::Rings
            | TexturePattern::Dots
            | TexturePattern::Spirals
            | TexturePattern::GreekKey
            | TexturePattern::Crosses
            | TexturePattern::Quatrefoils
            | TexturePattern::Flowers
            | TexturePattern::Trefoils
            | TexturePattern::Moons
            | TexturePattern::Arrows
            | TexturePattern::NestedDiamonds
            | TexturePattern::Argyle
            | TexturePattern::Feathers
            | TexturePattern::Paisley
            | TexturePattern::Swirls
            | TexturePattern::Snowflakes
            | TexturePattern::Bubbles
            | TexturePattern::Sunbursts => (l * 3.0).clamp(6.0, 18.0),
            _ => (l * 1.5).clamp(3.0, 12.0),
        }
    }
}

/// How a cross fill sews each cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrossStyle {
    /// An X: both diagonals.
    #[default]
    Full,
    /// One diagonal (/).
    Half,
    /// A + : across the middle and up and down it.
    Upright,
    /// An X with a + over it (a star).
    Double,
}

impl CrossStyle {
    pub const ALL: [CrossStyle; 4] = [CrossStyle::Full, CrossStyle::Half, CrossStyle::Upright, CrossStyle::Double];

    pub fn is_full(&self) -> bool {
        *self == CrossStyle::Full
    }
}

/// A 3D motif fill: its motifs sculpted to look curved (a dome or a bowl) or to recede.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MotifWarp {
    pub kind: WarpKind,
    /// How strong, 0 to 0.8: for a globe, how much the motifs shrink toward the edge (a dome)
    /// or the middle (a bowl); for perspective, how much smaller they are on the far side.
    pub amount: f64,
    /// Perspective: the way the fill recedes, in degrees counter-clockwise on screen (90: up).
    pub angle: f64,
}

impl Default for MotifWarp {
    fn default() -> Self {
        MotifWarp { kind: WarpKind::GlobeOut, amount: 0.5, angle: 90.0 }
    }
}

/// How a 3D motif fill is sculpted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WarpKind {
    /// A dome (convex): motifs largest in the middle, smaller and closer together toward the
    /// edge.
    #[default]
    GlobeOut,
    /// A bowl (concave): motifs smallest in the middle, growing toward the edge.
    GlobeIn,
    /// Receding into the distance: motifs smaller and rows closer together toward the far side.
    Perspective,
}

impl WarpKind {
    pub const ALL: [WarpKind; 3] = [WarpKind::GlobeOut, WarpKind::GlobeIn, WarpKind::Perspective];
}

/// Programmed splits of a tatami fill (a texture).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Texture {
    pub pattern: TexturePattern,
    /// Width of one pattern tile (mm).
    pub size: f64,
    /// Turn of the pattern against the stitch rows (degrees).
    pub angle: f64,
}

impl Default for Texture {
    fn default() -> Self {
        Texture { pattern: TexturePattern::Diamonds, size: 6.0, angle: 0.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FillParams {
    pub kind: FillKind,
    /// Stitch direction (degrees, counter-clockwise on screen).
    pub angle: f64,
    /// Row spacing (mm). Smaller = denser.
    pub spacing: f64,
    /// Needle step within a row (mm).
    pub stitch_length: f64,
    /// Tatami stagger: shift of each row as a fraction of the stitch length.
    pub offset: f64,
    /// Satin fill: split stitches longer than this (mm); 0 = never.
    pub split_length: f64,
    /// Extension of row ends against fabric pull-in (mm).
    pub pull_comp: f64,
    /// Push compensation: the fill stops this much short of the outline across its rows (mm),
    /// where the rows push the fabric out.
    pub push_comp: f64,
    /// If > 0, row spacing changes linearly from `spacing` to this value across the
    /// shape (gradient / colour-blending effect).
    pub spacing_end: f64,
    /// Centre for radial and spiral fills (`None` = shape centroid).
    pub center: Option<Point>,
    /// Random jitter of tatami needle points (0 = none, 1 = up to a quarter
    /// stitch) for a softer, hand-sewn look. Also randomises meander paths.
    pub random: f64,
    pub underlay: FillUnderlay,
    /// A satin fill made from a satin object: its column settings (underlay by
    /// width, short stitches, twin-sided pull compensation, long stitches…) apply
    /// where the satin turns along inclination lines, instead of `underlay`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub satin_column: Option<SatinParams>,
    /// Programmed splits (tatami): the needle points follow a pattern, a texture in the fill.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture: Option<Texture>,
    /// Trapunto (tatami): travel between parts of the fill keeps to its outline, so an open
    /// fill shows no travel stitches across it.
    #[serde(skip_serializing_if = "is_false")]
    pub trapunto: bool,
    /// A textured edge: each row's ends stop short of the outline by a random amount up to this
    /// (mm; 0, the default, for a clean edge), so the edge looks ragged like fur or grass.
    #[serde(skip_serializing_if = "is_no_depth")]
    pub ragged: f64,
    /// Motif fills: the motif on the rows, and its size (mm, across the row and along it).
    pub motif: Motif,
    pub motif_size: f64,
    /// Motif fills: a motif made from a drawn line, sewn instead of `motif` (as long as
    /// `motif_size`, and as high as its drawn proportions make it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_motif: Option<CustomMotif>,
    /// Cross fills: how each cell is sewn.
    #[serde(skip_serializing_if = "CrossStyle::is_full")]
    pub cross: CrossStyle,
    /// Motif fills: a second motif sewn in turn with the first along every row (a paired motif
    /// fill).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motif_pair: Option<Motif>,
    /// Motif fills: sculpted to look three-dimensional (a 3D motif fill).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warp: Option<MotifWarp>,
    /// Contour fills: how many rings in from the outline (an offset fill); 0 = all the way in.
    #[serde(skip_serializing_if = "is_zero")]
    pub rings: u32,
    /// Tatami and satin fills: the row spacing across the shape (accordion spacing, colour
    /// blends), as points `[t, spacing]`: `t` from 0 to 1 in the direction the rows stack,
    /// spacing in mm, 0 for no rows. Between points the density (one over the spacing) changes
    /// evenly. Empty = `spacing` (graded to `spacing_end` when that is set).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spacing_profile: Vec<[f64; 2]>,
    /// Multi Blend (tatami): the fill sewn as stacked layers in these threads, one after the
    /// other, each with its own spacing profile, so the colours blend across the rows in one
    /// object. Underlay goes under the first layer only. Empty = one layer in the object's thread.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blend: Vec<BlendLayer>,
}

/// One layer of a Multi Blend: its thread and how its rows are spaced across the fill.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlendLayer {
    pub thread: Thread,
    /// As [`FillParams::spacing_profile`]: `[t, spacing]` points, 0 for no rows.
    #[serde(default)]
    pub spacing_profile: Vec<[f64; 2]>,
}

impl FillParams {
    /// The fill as its Multi Blend layers, sewn in order, each with its thread colour: one
    /// fill per layer with that layer's spacing profile, underlay under the first only. A fill
    /// without a blend is one layer.
    pub fn blend_layers(&self) -> Vec<(FillParams, Option<&Thread>)> {
        if self.blend.is_empty() {
            return vec![(self.clone(), None)];
        }
        self.blend
            .iter()
            .enumerate()
            .map(|(k, layer)| {
                let mut f = FillParams { blend: Vec::new(), spacing_profile: layer.spacing_profile.clone(), spacing_end: 0.0, ..self.clone() };
                if k > 0 {
                    f.underlay = FillUnderlay { edge_run: false, tatami: false, ..f.underlay.clone() };
                }
                (f, Some(&layer.thread))
            })
            .collect()
    }
}

fn is_no_depth(d: &f64) -> bool {
    *d == 0.0
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl Default for FillParams {
    fn default() -> Self {
        FillParams {
            kind: FillKind::Tatami,
            angle: 45.0,
            spacing: 0.4,
            stitch_length: 4.0,
            offset: 0.25,
            split_length: 7.0,
            pull_comp: 0.2,
            push_comp: 0.0,
            spacing_end: 0.0,
            center: None,
            random: 0.0,
            underlay: FillUnderlay::default(),
            satin_column: None,
            texture: None,
            trapunto: false,
            ragged: 0.0,
            motif: Motif::Wave,
            motif_size: 4.0,
            custom_motif: None,
            cross: CrossStyle::Full,
            motif_pair: None,
            warp: None,
            rings: 0,
            spacing_profile: Vec::new(),
            blend: Vec::new(),
        }
    }
}

impl FillParams {
    /// Defaults suited to a satin-filled shape.
    pub fn satin() -> Self {
        FillParams { kind: FillKind::Satin, angle: 0.0, spacing: 0.4, ..FillParams::default() }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FillObject {
    pub shape: Shape,
    #[serde(default)]
    pub params: FillParams,
    /// Inclination lines. One guide sets the stitch angle (overriding
    /// `params.angle`); two or more, in order along the shape, make the stitches
    /// turn smoothly from guide to guide ("turning" fill / satin).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<Guide>,
    /// Split lines (tatami): a needle point of each row falls where the row crosses one, so the
    /// lines show in the fill (drawn splits, carved stamps, flexi splits).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub splits: Vec<Vec<Point>>,
    /// Flow lines (tatami): the rows follow them instead of running straight; one line, rows
    /// copying it (a Florentine effect); two, rows changing shape from one to the other (a
    /// liquid effect).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flow: Vec<Vec<Point>>,
}

impl FillObject {
    pub fn new(shape: Shape, params: FillParams) -> Self {
        FillObject { shape, params, guides: Vec::new(), splits: Vec::new(), flow: Vec::new() }
    }
}

// ---------------------------------------------------------------------------
// Appliqué
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TackdownKind {
    None,
    Run,
    #[default]
    Zigzag,
    Blanket,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CoverKind {
    None,
    #[default]
    Satin,
    Blanket,
    /// An open zigzag over the edge.
    Zigzag,
    /// A triple run along the edge.
    Triple,
}

/// The appliqué fabric's look on the canvas (display only): its colour plain or woven in a
/// pattern with a second colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FabricPattern {
    #[default]
    Plain,
    Stripes,
    /// Gingham: stripes both ways, darker where they cross.
    Check,
    Dots,
    /// A twill's diagonal ribs.
    Denim,
    /// Felt's flecked surface.
    Felt,
}

impl FabricPattern {
    pub const ALL: [FabricPattern; 6] =
        [FabricPattern::Plain, FabricPattern::Stripes, FabricPattern::Check, FabricPattern::Dots, FabricPattern::Denim, FabricPattern::Felt];

    /// How much of the second colour shows at `(u, v)` mm on the fabric, 0–1.
    pub fn mix(self, u: f64, v: f64) -> f64 {
        if !(u.is_finite() && v.is_finite()) {
            return 0.0;
        }
        let band = |x: f64, w: f64| (x / w).floor().rem_euclid(2.0);
        match self {
            FabricPattern::Plain => 0.0,
            FabricPattern::Stripes => band(u, 3.0),
            FabricPattern::Check => (band(u, 3.0) + band(v, 3.0)) / 2.0,
            FabricPattern::Dots => {
                // a dot 1.2 mm round each point of a 5 mm grid, every other row shifted
                let row = (v / 5.0).round();
                let shift = if row.rem_euclid(2.0) == 1.0 { 2.5 } else { 0.0 };
                let (du, dv) = ((u - shift) - ((u - shift) / 5.0).round() * 5.0, v - row * 5.0);
                if du * du + dv * dv < 1.44 { 1.0 } else { 0.0 }
            }
            FabricPattern::Denim => {
                if ((u + v) / 0.6).floor().rem_euclid(3.0) == 0.0 {
                    0.35
                } else {
                    0.0
                }
            }
            FabricPattern::Felt => {
                // a fixed hash of the 0.4 mm cell: flecks, the same every frame
                let (a, b) = ((u / 0.4).floor() as i64, (v / 0.4).floor() as i64);
                let h = (a.wrapping_mul(73_856_093) ^ b.wrapping_mul(19_349_663)).rem_euclid(1000);
                if h < 90 { 0.5 } else { 0.0 }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppliqueParams {
    /// Sew a placement line, then stop so the fabric can be positioned.
    pub placement: bool,
    pub tackdown: TackdownKind,
    /// Width of zigzag / blanket tack-down (mm).
    pub tackdown_width: f64,
    /// Inset of the tack-down from the outline (mm).
    pub tackdown_inset: f64,
    /// Stop after tack-down so excess fabric can be cut away.
    pub trim_stop: bool,
    pub cover: CoverKind,
    /// Width of the cover stitch (mm).
    pub cover_width: f64,
    /// Spacing of the satin cover stitch (mm).
    pub cover_spacing: f64,
    pub pull_comp: f64,
    /// Colour of the appliqué fabric (display only).
    pub fabric_color: Rgb,
    /// The fabric's pattern and its second colour (display only).
    pub fabric_pattern: FabricPattern,
    pub fabric_color_2: Rgb,
    /// The pieces of a compound path sewn as one appliqué: every placement line, one stop,
    /// every tack-down, one stop, every cover (else each piece in turn).
    pub combine: bool,
    /// Partial cover: the cover leaves out the edges objects sewn later lie over (they hold
    /// the fabric's edge there).
    pub partial: bool,
}

impl Default for AppliqueParams {
    fn default() -> Self {
        AppliqueParams {
            placement: true,
            tackdown: TackdownKind::Zigzag,
            tackdown_width: 1.2,
            tackdown_inset: 0.6,
            trim_stop: true,
            cover: CoverKind::Satin,
            cover_width: 3.0,
            cover_spacing: 0.45,
            pull_comp: 0.2,
            fabric_color: Rgb::new(0xe9, 0xd8, 0xa6),
            fabric_pattern: FabricPattern::Plain,
            fabric_color_2: Rgb::new(0xff, 0xff, 0xff),
            combine: false,
            partial: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AppliqueObject {
    pub shape: Shape,
    #[serde(default)]
    pub params: AppliqueParams,
    /// With partial cover: the outlines of what is sewn later (mm), whose edges the cover leaves
    /// out; set when the design is planned.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub covered: Vec<Vec<Point>>,
    /// The other pieces sewn with this one (combined appliqué), mm.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<Shape>,
}

impl AppliqueObject {
    /// Every piece: the shape, then the pieces combined with it.
    pub fn pieces(&self) -> impl Iterator<Item = &Shape> {
        std::iter::once(&self.shape).chain(&self.extra)
    }
}

// ---------------------------------------------------------------------------
// Lettering
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Baseline {
    #[default]
    Straight,
    /// Text along a circular arc of the given radius (mm). `clockwise` text runs
    /// over the top of the circle (convex); counter-clockwise text runs along the
    /// bottom (concave), reading left to right in both cases.
    Arc { radius: f64, clockwise: bool },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextParams {
    /// The text; `\n` separates lines.
    pub text: String,
    /// Font name (one of the built-in fonts or a loaded font).
    pub font: String,
    /// Height of capital letters (mm).
    pub height: f64,
    /// Extra space between letters (mm, may be negative).
    pub letter_spacing: f64,
    /// Line pitch as a multiple of `height`.
    pub line_spacing: f64,
    pub align: TextAlign,
    pub baseline: Baseline,
}

impl Default for TextParams {
    fn default() -> Self {
        TextParams {
            text: "Text".into(),
            font: "Sans Bold".into(),
            height: 10.0,
            letter_spacing: 0.0,
            line_spacing: 1.5,
            align: TextAlign::Center,
            baseline: Baseline::Straight,
        }
    }
}

/// A laid-out glyph outline in design space.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Glyph {
    pub ch: char,
    pub shape: Shape,
    /// Direction of the baseline under this glyph (degrees, counter-clockwise on screen).
    #[serde(default)]
    pub angle: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LetteringObject {
    pub text: TextParams,
    /// Placement applied after layout. Layout space has the anchor of the first
    /// baseline at the origin (left end, centre or right end depending on alignment).
    #[serde(default)]
    pub transform: Affine,
    /// Cached glyph outlines in design space (already transformed). Kept in the file
    /// so designs remain usable without the font.
    #[serde(default)]
    pub glyphs: Vec<Glyph>,
    /// Stitch settings for the letters. The fill angle is relative to each glyph's baseline.
    #[serde(default = "FillParams::satin")]
    pub fill: FillParams,
    /// Optional running-stitch outline around each letter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<RunParams>,
}

impl Default for LetteringObject {
    fn default() -> Self {
        LetteringObject { text: TextParams::default(), transform: Affine::IDENTITY, glyphs: Vec::new(), fill: FillParams::satin(), outline: None }
    }
}

// ---------------------------------------------------------------------------
// Raw stitches
// ---------------------------------------------------------------------------

/// Raw stitch records (one colour). May contain jumps, trims and stops, but not
/// colour changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StitchBlock {
    pub stitches: Vec<Stitch>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Node, Polygon};

    fn sample_design() -> Design {
        let mut d = Design::new();
        let fill = ObjectKind::Fill(FillObject::new(Shape::rect(Point::new(0.0, 0.0), Point::new(20.0, 10.0)), FillParams::default()));
        d.add(fill, 0);
        let run = ObjectKind::Run(RunObject {
            path: Contour::new(
                vec![Node::corner(Point::new(0.0, 15.0)), Node::curve(Point::new(10.0, 20.0)), Node::corner(Point::new(20.0, 15.0))],
                false,
            ),
            params: RunParams { kind: RunKind::Triple, ..Default::default() },
        });
        d.add(run, 1);
        d.backdrop = Some(Backdrop {
            name: "logo.png".into(),
            data: vec![1, 2, 3, 250],
            min: Point::new(-5.0, -5.0),
            width: 30.0,
            height: 30.0,
            opacity: 0.5,
            visible: true,
            locked: false,
        });
        d
    }

    #[test]
    fn json_round_trip() {
        let d = sample_design();
        let json = serde_json::to_string_pretty(&d).unwrap();
        let back: Design = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
        assert!(json.contains("\"type\": \"Fill\""));
    }

    #[test]
    fn ids_are_unique() {
        let mut d = sample_design();
        let a = d.objects[0].id;
        let b = d.objects[1].id;
        assert_ne!(a, b);
        let c = d.add(ObjectKind::Stitches(StitchBlock::default()), 0);
        assert!(c > b);
    }

    #[test]
    fn rotating_a_fill_rotates_its_angle() {
        let mut d = sample_design();
        let obj = &mut d.objects[0];
        // +90° in y-down maths = clockwise on screen, so a 45° (screen CCW) angle becomes -45° = 135°.
        obj.transform(&Affine::rotate(std::f64::consts::FRAC_PI_2));
        let ObjectKind::Fill(f) = &obj.kind else { panic!() };
        assert!((f.params.angle - 135.0).abs() < 1e-9, "{}", f.params.angle);
        // mirrored horizontally: 135° -> 45°
        obj.transform(&Affine::scale(-1.0, 1.0));
        let ObjectKind::Fill(f) = &obj.kind else { panic!() };
        assert!((f.params.angle - 45.0).abs() < 1e-9, "{}", f.params.angle);
    }

    #[test]
    fn bbox_and_outlines() {
        let d = sample_design();
        let b = d.objects[0].bbox();
        assert!((b.width() - 20.0).abs() < 1e-9 && (b.height() - 10.0).abs() < 1e-9);
        let outlines = d.objects[0].kind.outlines();
        assert_eq!(outlines.len(), 1);
        assert_eq!(outlines[0].first(), outlines[0].last());
        let _ = Polygon::default();
    }
}
