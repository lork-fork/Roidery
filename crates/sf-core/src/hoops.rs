//! Embroidery hoops (sewing fields). Hoops are centred on the design origin.
//!
//! The built-in library is data: `hoops.json` is embedded at build time ([`library`]), holding
//! parametric outline [`HoopTemplate`]s and the hoops that use them. Add hoops there, not in code.

use crate::geom::{BBox, Point};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HoopShape {
    /// Saved files write `Rect` / `Oval`; the library JSON writes them in lower case.
    #[default]
    #[serde(alias = "rect")]
    Rect,
    #[serde(alias = "oval")]
    Oval,
    /// Circular field (width = height); drawn and tested like an oval.
    #[serde(alias = "round")]
    Round,
    /// Square field (width = height); drawn and tested like a rectangle.
    #[serde(alias = "square")]
    Square,
}

/// What kind of hoop or frame this is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoopCategory {
    #[default]
    Machine,
    Magnetic,
    Clamp,
    Cap,
    TubularSleeve,
    BorderMulti,
    Specialty,
}

/// The side of the frame where the bracket or clamp is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachSide {
    Left,
    Right,
    Top,
    Bottom,
}

/// Bracket or clamp block of a template (mm): `width` along the frame edge, `depth` outward.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BracketSpec {
    pub width: f64,
    pub depth: f64,
}

/// Screw knob of a template (mm), drawn as a circle on the bracket's outer end.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct KnobSpec {
    pub radius: f64,
}

/// A reusable parametric outline: many hoops share one drawing, sized per model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HoopTemplate {
    pub id: String,
    #[serde(default)]
    pub description: String,
    /// Width of the frame ring between the sewing field and the outer edge (mm).
    pub field_inset: f64,
    /// Corner radius of the outer frame (mm).
    #[serde(default)]
    pub outer_corner_radius: f64,
    #[serde(default)]
    pub bracket: Option<BracketSpec>,
    #[serde(default)]
    pub knob: Option<KnobSpec>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hoop {
    pub name: String,
    /// Sewing field width (mm).
    pub width: f64,
    /// Sewing field height (mm).
    pub height: f64,
    #[serde(default)]
    pub shape: HoopShape,
    /// Stable id of a library hoop; empty for hoops of one's own and old files.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// Maker the hoop is compatible with (names are for compatibility only).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub brand: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub category: HoopCategory,
    /// A multi-position hoop's positions: where each sewing field's centre is (mm from the
    /// frame's centre); the fabric is hooped once and the machine sews one position at a time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub positions: Vec<[f64; 2]>,
    /// Corner radius of a rectangular sewing field (mm); 0 for square corners.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub corner_radius: f64,
    /// Outer frame size (width, height in mm), when known; display only. Also read as `outer`.
    #[serde(default, alias = "outer", skip_serializing_if = "Option::is_none")]
    pub frame: Option<[f64; 2]>,
    /// Where the bracket or clamp is, when the outline should show one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attach_side: Option<AttachSide>,
    /// Offset of the needle centre from the field centre (mm).
    #[serde(default, skip_serializing_if = "is_zero_pair")]
    pub needle_center_offset: [f64; 2],
    /// Id of the outline [`HoopTemplate`] drawn for this hoop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// Names of bracket variants the hoop is sold with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bracket_variants: Vec<String>,
    /// Whether the size was checked against a public source; false until then.
    #[serde(default, skip_serializing_if = "is_default")]
    pub verified: bool,
    /// Where the size came from (a public URL or note).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// Machines (by name) a hoop of one's own is attached to; empty for every standard hoop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub machines: Vec<String>,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_zero_pair(v: &[f64; 2]) -> bool {
    v[0] == 0.0 && v[1] == 0.0
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// A rounded rectangle about the origin as a polygon: four corner arcs of 12 steps each, or
/// the four corners when `r` is 0.
fn rounded_rect(hw: f64, hh: f64, r: f64) -> Vec<Point> {
    let r = if r.is_finite() { r.clamp(0.0, hw.min(hh).max(0.0)) } else { 0.0 };
    if r <= 0.0 {
        return vec![Point::new(-hw, -hh), Point::new(hw, -hh), Point::new(hw, hh), Point::new(-hw, hh)];
    }
    let mut v = Vec::with_capacity(52);
    for (cx, cy, a0) in [(hw - r, hh - r, 0.0), (-(hw - r), hh - r, 0.25), (-(hw - r), -(hh - r), 0.5), (hw - r, -(hh - r), 0.75)] {
        for i in 0..=12 {
            let a = (a0 + 0.25 * f64::from(i) / 12.0) * std::f64::consts::TAU;
            v.push(Point::new(cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    v
}

fn ellipse(hw: f64, hh: f64, n: usize) -> Vec<Point> {
    (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            Point::new(hw * a.cos(), hh * a.sin())
        })
        .collect()
}

impl Hoop {
    pub fn new(name: &str, width: f64, height: f64, shape: HoopShape) -> Self {
        Hoop { name: name.into(), width, height, shape, ..Hoop::default() }
    }

    /// A multi-position hoop: a `width` × `height` field at each of `positions`.
    pub fn multi(name: &str, width: f64, height: f64, positions: &[[f64; 2]]) -> Self {
        Hoop { positions: positions.to_vec(), ..Hoop::new(name, width, height, HoopShape::Rect) }
    }

    /// Whether the field is an ellipse (oval or round) rather than a rectangle.
    pub fn is_elliptical(&self) -> bool {
        matches!(self.shape, HoopShape::Oval | HoopShape::Round)
    }

    /// Sewing field bounds (centred on the origin).
    pub fn bounds(&self) -> BBox {
        BBox::new(Point::new(-self.width / 2.0, -self.height / 2.0), Point::new(self.width / 2.0, self.height / 2.0))
    }

    /// Whether a point lies inside the sewing field.
    pub fn contains(&self, p: Point) -> bool {
        let (hw, hh) = (self.width / 2.0, self.height / 2.0);
        if self.is_elliptical() {
            return (p.x / hw).powi(2) + (p.y / hh).powi(2) <= 1.0;
        }
        if p.x.abs() > hw || p.y.abs() > hh {
            return false;
        }
        let r = self.corner_radius.clamp(0.0, hw.min(hh));
        // distance past the inner box into the corner arc
        let (dx, dy) = ((p.x.abs() - (hw - r)).max(0.0), (p.y.abs() - (hh - r)).max(0.0));
        dx * dx + dy * dy <= r * r + 1e-9
    }

    /// Whether a bounding box fits entirely in the sewing field.
    pub fn fits(&self, b: &BBox) -> bool {
        b.is_empty() || b.corners().iter().all(|&c| self.contains(c))
    }

    /// Outline polygon of the sewing field for display.
    pub fn outline(&self) -> Vec<Point> {
        let (hw, hh) = (self.width / 2.0, self.height / 2.0);
        if self.is_elliptical() { ellipse(hw, hh, 96) } else { rounded_rect(hw, hh, self.corner_radius) }
    }

    /// The sewing field's outline (same as [`Hoop::outline`]; pairs with [`Hoop::outline_outer`]).
    pub fn outline_field(&self) -> Vec<Point> {
        self.outline()
    }

    fn template_in<'a>(&self, templates: &'a [HoopTemplate]) -> Option<&'a HoopTemplate> {
        let id = self.template.as_deref()?;
        templates.iter().find(|t| t.id == id)
    }

    /// The library template drawn for this hoop, if it names a known one.
    pub fn template_spec(&self) -> Option<&'static HoopTemplate> {
        self.template_in(&library().templates)
    }

    fn outer_size_in(&self, templates: &[HoopTemplate]) -> [f64; 2] {
        if let Some(f) = self.frame {
            return f;
        }
        let inset = self.template_in(templates).map_or(0.0, |t| t.field_inset);
        [self.width + 2.0 * inset, self.height + 2.0 * inset]
    }

    /// Outer frame size (mm): the stated `frame`, else the field plus the template's inset on
    /// every side, else the field itself.
    pub fn outer_size(&self) -> [f64; 2] {
        self.outer_size_in(&library().templates)
    }

    /// Outlines of the frame for display, as closed polygons: the outer ring first, then (when
    /// `attach_side` is set and the template has them) the bracket block and the screw knob.
    pub fn outline_outer(&self) -> Vec<Vec<Point>> {
        let [ow, oh] = self.outer_size();
        let (hw, hh) = (ow / 2.0, oh / 2.0);
        let t = self.template_spec();
        let ring = if self.is_elliptical() { ellipse(hw, hh, 96) } else { rounded_rect(hw, hh, t.map_or(0.0, |t| t.outer_corner_radius)) };
        let mut out = vec![ring];
        let (Some(t), Some(side)) = (t, self.attach_side) else { return out };
        let Some(b) = t.bracket else { return out };
        // a point `along` the frame edge on the attach side, `outward` past the edge
        let place = |along: f64, outward: f64| match side {
            AttachSide::Right => Point::new(hw + outward, along),
            AttachSide::Left => Point::new(-(hw + outward), along),
            AttachSide::Top => Point::new(along, -(hh + outward)),
            AttachSide::Bottom => Point::new(along, hh + outward),
        };
        let h = b.width / 2.0;
        out.push(vec![place(-h, 0.0), place(h, 0.0), place(h, b.depth), place(-h, b.depth)]);
        if let Some(k) = t.knob {
            let c = place(0.0, b.depth);
            out.push(
                (0..16)
                    .map(|i| {
                        let a = f64::from(i) / 16.0 * std::f64::consts::TAU;
                        Point::new(c.x + k.radius * a.cos(), c.y + k.radius * a.sin())
                    })
                    .collect(),
            );
        }
        out
    }

    /// Checks the hoop against the library's templates.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_with(&library().templates)
    }

    /// Checks a hoop's numbers: positive finite sizes, field ≤ outer frame, positions (with
    /// their fields) inside the outer frame, a known template.
    pub fn validate_with(&self, templates: &[HoopTemplate]) -> Result<(), String> {
        let n = &self.name;
        if n.trim().is_empty() {
            return Err("hoop without a name".into());
        }
        let ok = |v: f64| v.is_finite() && v > 0.0;
        if !ok(self.width) || !ok(self.height) {
            return Err(format!("{n}: field size must be positive and finite"));
        }
        if !self.corner_radius.is_finite() || self.corner_radius < 0.0 {
            return Err(format!("{n}: bad corner radius"));
        }
        if matches!(self.shape, HoopShape::Round | HoopShape::Square) && (self.width - self.height).abs() > 1e-9 {
            return Err(format!("{n}: a round or square field must have width = height"));
        }
        if let Some(id) = &self.template
            && self.template_in(templates).is_none()
        {
            return Err(format!("{n}: unknown template `{id}`"));
        }
        let [ow, oh] = self.outer_size_in(templates);
        if !ok(ow) || !ok(oh) || ow + 1e-9 < self.width || oh + 1e-9 < self.height {
            return Err(format!("{n}: field {}×{} does not fit the outer frame {ow}×{oh}", self.width, self.height));
        }
        for p in &self.positions {
            let (px, py) = (p[0], p[1]);
            if !px.is_finite() || !py.is_finite() || px.abs() + self.width / 2.0 > ow / 2.0 + 1e-9 || py.abs() + self.height / 2.0 > oh / 2.0 + 1e-9 {
                return Err(format!("{n}: position [{px}, {py}] is outside the outer frame"));
            }
        }
        if !self.needle_center_offset.iter().all(|v| v.is_finite()) {
            return Err(format!("{n}: bad needle centre offset"));
        }
        Ok(())
    }
}

/// A hoop library: outline templates and the hoops that use them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HoopLibrary {
    #[serde(default)]
    pub templates: Vec<HoopTemplate>,
    #[serde(default)]
    pub hoops: Vec<Hoop>,
}

/// Parses and validates a library from JSON: unique template ids and hoop names, sane template
/// sizes, every hoop valid against the library's own templates.
pub fn parse_library(json: &str) -> Result<HoopLibrary, String> {
    let lib: HoopLibrary = serde_json::from_str(json).map_err(|e| format!("hoop library: {e}"))?;
    let size_ok = |v: f64| v.is_finite() && v >= 0.0;
    for (i, t) in lib.templates.iter().enumerate() {
        if t.id.is_empty() || lib.templates[..i].iter().any(|u| u.id == t.id) {
            return Err(format!("template `{}`: empty or repeated id", t.id));
        }
        let bracket_ok = t.bracket.is_none_or(|b| size_ok(b.width) && size_ok(b.depth));
        let knob_ok = t.knob.is_none_or(|k| size_ok(k.radius));
        if !(size_ok(t.field_inset) && size_ok(t.outer_corner_radius) && bracket_ok && knob_ok) {
            return Err(format!("template `{}`: sizes must be finite and not negative", t.id));
        }
    }
    for (i, h) in lib.hoops.iter().enumerate() {
        if lib.hoops[..i].iter().any(|u| u.name == h.name) {
            return Err(format!("{}: repeated hoop name", h.name));
        }
        h.validate_with(&lib.templates)?;
    }
    Ok(lib)
}

static LIBRARY: OnceLock<HoopLibrary> = OnceLock::new();

/// The embedded library (`hoops.json`). If the embedded data were ever invalid this is empty
/// (a test guards against that) rather than a crash.
pub fn library() -> &'static HoopLibrary {
    LIBRARY.get_or_init(|| parse_library(include_str!("hoops.json")).unwrap_or_default())
}

/// Built-in hoop library (common domestic and commercial sewing fields), in library order.
pub fn standard_hoops() -> Vec<Hoop> {
    library().hoops.clone()
}

/// Smallest standard hoop that fits the bounds (by area).
pub fn smallest_fitting(b: &BBox) -> Option<Hoop> {
    let centered = BBox::new(Point::new(-b.width() / 2.0, -b.height() / 2.0), Point::new(b.width() / 2.0, b.height() / 2.0));
    standard_hoops().into_iter().filter(|h| h.fits(&centered)).min_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 60 hoops the library had before it became data, in order.
    const OLD_NAMES: [&str; 5] = [
        "Brother 100 × 100 (4×4)",
        "Janome SQ14 140 × 140",
        "Tajima Round 12 cm",
        "Magnetic pocket frame 100 × 70",
        "Multi-position 200 × 360 (2 × 200 × 200)",
    ];

    #[test]
    fn embedded_library_parses_and_validates() {
        let lib = parse_library(include_str!("hoops.json")).unwrap();
        assert_eq!(lib.hoops.len(), 60);
        assert_eq!(library().hoops.len(), 60);
        for h in &lib.hoops {
            h.validate().unwrap();
        }
    }

    #[test]
    fn old_names_resolve_and_keep_their_sizes() {
        let all = standard_hoops();
        for n in OLD_NAMES {
            assert!(all.iter().any(|h| h.name == n), "{n}");
        }
        assert_eq!(all[0].name, "Brother 100 × 100 (4×4)");
        assert_eq!(all[58].positions.len(), 3);
        let tajima = all.iter().find(|h| h.name == "Tajima Round 15 cm").unwrap();
        assert_eq!((tajima.width, tajima.height, tajima.shape), (150.0, 150.0, HoopShape::Oval));
        assert!(all.iter().all(|h| !h.verified));
    }

    #[test]
    fn smallest_fitting_is_unchanged() {
        let bb = |w: f64, h: f64| BBox::new(Point::new(0.0, 0.0), Point::new(w, h));
        assert_eq!(smallest_fitting(&bb(90.0, 90.0)).unwrap().name, "Brother 100 × 100 (4×4)");
        // 125 × 175 needs the 130 × 180 field (first of its area)
        assert_eq!(smallest_fitting(&bb(125.0, 175.0)).unwrap().name, "Brother 130 × 180 (5×7)");
        assert!(smallest_fitting(&bb(400.0, 400.0)).is_none());
    }

    #[test]
    fn old_files_load_and_round_trip_without_new_fields() {
        let old = r#"{"name":"X","width":100.0,"height":80.0,"frame":[120.0,100.0]}"#;
        let h: Hoop = serde_json::from_str(old).unwrap();
        assert_eq!((h.shape, h.category, h.verified, h.template.clone()), (HoopShape::Rect, HoopCategory::Machine, false, None));
        // saved files keep the old spelling and gain nothing
        assert_eq!(serde_json::to_string(&h).unwrap(), r#"{"name":"X","width":100.0,"height":80.0,"shape":"Rect","frame":[120.0,100.0]}"#);
        assert_eq!(serde_json::from_str::<Hoop>(&serde_json::to_string(&h).unwrap()).unwrap(), h);
        let outer: Hoop = serde_json::from_str(r#"{"name":"Y","width":1.0,"height":1.0,"outer":[2.0,2.0]}"#).unwrap();
        assert_eq!(outer.frame, Some([2.0, 2.0]));
    }

    #[test]
    fn library_rejects_bad_data() {
        let bad = |hoop: &str| parse_library(&format!(r#"{{"templates":[{{"id":"t","field_inset":5.0}}],"hoops":[{hoop}]}}"#)).is_err();
        assert!(bad(r#"{"name":"A","width":0.0,"height":10.0}"#));
        assert!(bad(r#"{"name":"A","width":10.0,"height":10.0,"frame":[8.0,20.0]}"#));
        assert!(bad(r#"{"name":"A","width":10.0,"height":10.0,"template":"nope"}"#));
        assert!(bad(r#"{"name":"A","width":10.0,"height":10.0,"frame":[12.0,12.0],"positions":[[5.0,0.0]]}"#));
        assert!(bad(r#"{"name":"A","width":10.0,"height":12.0,"shape":"round"}"#));
        assert!(!bad(r#"{"name":"A","width":10.0,"height":10.0,"template":"t","shape":"square"}"#));
        assert!(parse_library("not json").is_err());
    }

    fn lib_template(id: &str) -> HoopTemplate {
        library().templates.iter().find(|t| t.id == id).unwrap().clone()
    }

    fn bounds_of(p: &[Point]) -> (f64, f64, f64, f64) {
        let f = |g: fn(f64, f64) -> f64, v: fn(&Point) -> f64, s: f64| p.iter().map(v).fold(s, g);
        (f(f64::min, |p| p.x, f64::MAX), f(f64::min, |p| p.y, f64::MAX), f(f64::max, |p| p.x, f64::MIN), f(f64::max, |p| p.y, f64::MIN))
    }

    #[test]
    fn golden_frame_rect_outline() {
        let h = Hoop { template: Some("frame-rect".into()), ..Hoop::new("T", 100.0, 100.0, HoopShape::Rect) };
        assert_eq!(h.outer_size(), [124.0, 124.0]);
        let rings = h.outline_outer();
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].len(), 52);
        let (x0, y0, x1, y1) = bounds_of(&rings[0]);
        assert!((x0 + 62.0).abs() < 1e-9 && (y0 + 62.0).abs() < 1e-9 && (x1 - 62.0).abs() < 1e-9 && (y1 - 62.0).abs() < 1e-9);
        // first vertex: the start of the lower-right corner arc
        assert!((rings[0][0].x - 62.0).abs() < 1e-9 && (rings[0][0].y - 52.0).abs() < 1e-9);
        assert_eq!(h.outline_field().len(), 4);
    }

    #[test]
    fn golden_frame_round_outline() {
        let h = Hoop { template: Some("frame-round".into()), ..Hoop::new("T", 120.0, 120.0, HoopShape::Round) };
        assert_eq!(h.outer_size(), [144.0, 144.0]);
        let rings = h.outline_outer();
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].len(), 96);
        assert!((rings[0][0].x - 72.0).abs() < 1e-9 && rings[0][0].y.abs() < 1e-9);
        assert!((rings[0][24].y - 72.0).abs() < 1e-9);
        // the field is inside the frame
        assert!(h.outline_field().iter().all(|p| p.x.hypot(p.y) <= 60.0 + 1e-9));
    }

    #[test]
    fn golden_magnetic_outline() {
        let h = Hoop { template: Some("magnetic".into()), ..Hoop::new("T", 100.0, 70.0, HoopShape::Rect) };
        assert_eq!(h.outer_size(), [116.0, 86.0]);
        let rings = h.outline_outer();
        assert_eq!(rings.len(), 1);
        let (x0, y0, x1, y1) = bounds_of(&rings[0]);
        assert!((x1 - x0 - 116.0).abs() < 1e-9 && (y1 - y0 - 86.0).abs() < 1e-9);
    }

    #[test]
    fn golden_clamp_bracket_outline() {
        let t = lib_template("clamp-bracket");
        assert_eq!((t.field_inset, t.outer_corner_radius), (15.0, 8.0));
        let h = Hoop {
            template: Some("clamp-bracket".into()),
            attach_side: Some(AttachSide::Right),
            category: HoopCategory::Clamp,
            ..Hoop::new("T", 100.0, 100.0, HoopShape::Square)
        };
        assert_eq!(h.outer_size(), [130.0, 130.0]);
        let rings = h.outline_outer();
        assert_eq!(rings.len(), 3);
        // bracket block: 60 wide along the right edge, 20 deep outward
        let (x0, y0, x1, y1) = bounds_of(&rings[1]);
        assert_eq!((x0, y0, x1, y1), (65.0, -30.0, 85.0, 30.0));
        // knob: a 7 mm circle centred on the bracket's outer end
        assert_eq!(rings[2].len(), 16);
        let (kx0, _, kx1, _) = bounds_of(&rings[2]);
        assert!((kx0 - 78.0).abs() < 1e-9 && (kx1 - 92.0).abs() < 1e-9);
        // other sides mirror it
        let top = Hoop { attach_side: Some(AttachSide::Top), ..h.clone() }.outline_outer();
        assert_eq!(bounds_of(&top[1]), (-30.0, -85.0, 30.0, -65.0));
    }

    #[test]
    fn explicit_outer_frame_wins_and_contains_positions() {
        let all = standard_hoops();
        let m = all.iter().find(|h| h.name.starts_with("Multi-position 130")).unwrap();
        assert_eq!(m.outer_size(), [130.0, 300.0]);
        let (x0, y0, x1, y1) = bounds_of(&m.outline_outer()[0]);
        assert!((x1 - x0 - 130.0).abs() < 1e-9 && (y1 - y0 - 300.0).abs() < 1e-9);
    }
}
