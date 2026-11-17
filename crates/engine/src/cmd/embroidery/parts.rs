//! Roidery: an object's two stitch parts. The fill sews with a fill stitch (tatami, contour,
//! circular, gradient, motif, guided, Satin Fill) in the fill colour; the stroke sews with a line
//! stitch (run, triple, Satin Stroke, E-stitch, rope) in the stroke colour, after the fill, as its
//! border. Satin Fill (stitch type `satinFill`) is a fill like the others: satin across the
//! shape, turning with it. Satin Rails (stitch type `satinBlocks`), manual stitches, lettering
//! are the whole object: Satin Rails sit in the fill part, the others in the stroke part, and the other part doesn't sew. Satin Rails sew in their
//! fill colour, which fills the area between the rails. Documents from before Satin Rails were a
//! fill keep them in the stroke part: they are read into the fill, and written back in the same
//! one-setting form as before.
//!
//! `ObjectAttributes::stitch` stores them as `{fill?, stroke?}`, each a `StitchSettings`; a part
//! left out follows the automatic rule (tatami where there is a fill, a running stitch where there
//! is a stroke). Older documents store one `StitchSettings` (it has a `type`): that is the part
//! its type belongs to, and the other part doesn't sew. One part set and the other off is still
//! written that way, so those documents and scripts read as before.
//!
//! The fill sews first, so the stroke borders it; `strokeFirst` turns that round (the
//! Sequencer's Fill and Stroke steps reorder it).
//!
//! The stroke weight is the sewn width of a satin, E-stitch or rope stroke: the width is read
//! from the stroke, and setting it sets the stroke ([`write`]).

use roidery_stitch::{StitchSettings, StitchType};
use serde_json::{Value, json};
use vectorcraft_doc::Node;

use super::design::DesignSettings;
use super::{MM_PER_PT, fill_rgb, stroke_rgb};

/// Thread thickness, mm: the stroke weight a running stitch shows at.
pub const THREAD_MM: f64 = 0.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Fill,
    Stroke,
}

impl Part {
    pub const BOTH: [Part; 2] = [Part::Fill, Part::Stroke];

    pub fn id(self) -> &'static str {
        match self {
            Part::Fill => "fill",
            Part::Stroke => "stroke",
        }
    }

    pub fn parse(s: &str) -> Option<Part> {
        match s {
            "fill" => Some(Part::Fill),
            "stroke" => Some(Part::Stroke),
            _ => None,
        }
    }

    /// The part a stitch type belongs to.
    pub fn of(kind: StitchType) -> Part {
        if kind.sews_fill() { Part::Fill } else { Part::Stroke }
    }

    /// Whether `kind` can sew this part (not stitching always can).
    pub fn allows(self, kind: StitchType) -> bool {
        kind == StitchType::None || Part::of(kind) == self
    }
}

/// What a part is set to.
#[derive(Clone, Debug, PartialEq)]
// An object has two of these, read and written one object at a time: boxing would buy nothing.
#[allow(clippy::large_enum_variant)]
pub enum Slot {
    /// The automatic rule.
    Auto,
    Set(StitchSettings),
}

impl Slot {
    fn off() -> Slot {
        Slot::Set(StitchSettings::of(StitchType::None))
    }

    fn is_off(&self) -> bool {
        matches!(self, Slot::Set(s) if s.kind == StitchType::None)
    }
}

/// Both parts of an object, as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct Slots {
    pub fill: Slot,
    pub stroke: Slot,
    /// The stroke sews before the fill.
    pub stroke_first: bool,
}

impl Slots {
    pub const AUTO: Slots = Slots { fill: Slot::Auto, stroke: Slot::Auto, stroke_first: false };

    /// One part set, the other off (order doesn't matter with one part).
    pub fn only(fill: Slot, stroke: Slot) -> Slots {
        Slots { fill, stroke, stroke_first: false }
    }

    /// Read `ObjectAttributes::stitch`.
    pub fn read(v: Option<&Value>) -> Slots {
        let Some(v) = v else { return Slots::AUTO };
        let parse = |v: &Value| serde_json::from_value::<StitchSettings>(v.clone()).ok();
        if v.get("type").is_some() {
            // One part (older documents and single-part settings).
            return match parse(v) {
                Some(s) if s.kind == StitchType::None => Slots::only(Slot::off(), Slot::off()),
                Some(s) if s.kind.sews_fill() => Slots::only(Slot::Set(s), Slot::off()),
                Some(s) => Slots::only(Slot::off(), Slot::Set(s)),
                None => Slots::AUTO,
            };
        }
        let slot = |k: &str| v.get(k).and_then(parse).map_or(Slot::Auto, Slot::Set);
        let (fill, stroke) = match slot("stroke") {
            // Satin Rails kept in the stroke part (before they were a fill): they are the whole object.
            Slot::Set(s) if s.kind == StitchType::SatinBlocks => (Slot::Set(s), Slot::off()),
            stroke => (slot("fill"), stroke),
        };
        Slots { fill, stroke, stroke_first: v.get("strokeFirst").and_then(Value::as_bool).unwrap_or(false) }
    }

    /// What `ObjectAttributes::stitch` stores (`None`: both automatic).
    pub fn to_value(&self) -> Option<Value> {
        let v = |s: &StitchSettings| serde_json::to_value(s).unwrap_or(Value::Null);
        match (&self.fill, &self.stroke) {
            (Slot::Auto, Slot::Auto) if !self.stroke_first => None,
            // Both off: one setting that says so.
            (f, s) if f.is_off() && s.is_off() => Some(v(&StitchSettings::of(StitchType::None))),
            // One part, the other off: one setting, as older documents have.
            (Slot::Set(f), s) if s.is_off() && f.kind.sews_fill() => Some(v(f)),
            (f, Slot::Set(s)) if f.is_off() && !s.kind.sews_fill() => Some(v(s)),
            (f, s) => {
                let mut o = serde_json::Map::new();
                for (k, slot) in [("fill", f), ("stroke", s)] {
                    if let Slot::Set(st) = slot {
                        o.insert(k.into(), v(st));
                    }
                }
                if self.stroke_first {
                    o.insert("strokeFirst".into(), Value::Bool(true));
                }
                Some(Value::Object(o))
            }
        }
    }

    pub fn get(&self, part: Part) -> &Slot {
        match part {
            Part::Fill => &self.fill,
            Part::Stroke => &self.stroke,
        }
    }

    pub fn get_mut(&mut self, part: Part) -> &mut Slot {
        match part {
            Part::Fill => &mut self.fill,
            Part::Stroke => &mut self.stroke,
        }
    }

    /// Whether one part is the whole object (Satin Rails, manual stitches, lettering).
    pub fn whole(&self) -> bool {
        self.whole_part().is_some()
    }

    /// The part that is the whole object: the fill for Satin Rails, the stroke for manual
    /// stitches and lettering.
    pub fn whole_part(&self) -> Option<Part> {
        Part::BOTH.into_iter().find(|p| matches!(self.get(*p), Slot::Set(s) if whole(s)))
    }
}

/// Satin Rails, manual stitches and lettering are the whole object.
pub fn whole(s: &StitchSettings) -> bool {
    matches!(s.kind, StitchType::SatinBlocks | StitchType::Manual) || s.lettering.is_some()
}

/// The stored parts of `n`.
pub fn slots(n: &Node) -> Slots {
    Slots::read(n.extras().attrs.as_ref().and_then(|a| a.stitch.as_ref()))
}

/// Whether a stitch type's width is the stroke weight.
fn has_width(kind: StitchType) -> bool {
    matches!(kind, StitchType::Satin | StitchType::EStitch | StitchType::Rope)
}

/// The sewn width a stroke stitch keeps in its settings, mm.
fn width_of(s: &StitchSettings) -> Option<f64> {
    match s.kind {
        StitchType::Satin => Some(s.satin.width),
        StitchType::EStitch => Some(s.e_stitch.width),
        StitchType::Rope => Some(s.rope.width),
        _ => None,
    }
}

fn set_width(s: &mut StitchSettings, w: f64) {
    match s.kind {
        StitchType::Satin => s.satin.width = w,
        StitchType::EStitch => s.e_stitch.width = w,
        StitchType::Rope => s.rope.width = w,
        _ => {}
    }
}

/// The settings `part` of `n` has, sewn or not, and whether they come from the automatic rule.
/// A stroke's width is its stroke weight.
pub fn part_settings(n: &Node, ds: &DesignSettings, part: Part) -> (StitchSettings, bool) {
    let (mut s, auto) = match slots(n).get(part) {
        Slot::Set(s) => (s.clone(), false),
        Slot::Auto => (ds.fabric().settings(if part == Part::Fill { StitchType::Tatami } else { StitchType::Run }), true),
    };
    if part == Part::Stroke && has_width(s.kind) && s.lettering.is_none() {
        // Lettering keeps the column width that suits its letters; its stroke shows that width.
        if let Some(st) = n.appearance.stroke().filter(|st| st.width.is_finite() && st.width > 0.0) {
            set_width(&mut s, st.width * MM_PER_PT);
        }
    }
    (s, auto)
}

/// The parts of `n` that sew, in sewing order (fill, then stroke, unless the stroke goes first), with their settings and
/// whether they're automatic. An automatic part sews only where the object has that paint; a part
/// set without its paint sews in the other paint's colour when it is the only part.
pub fn sewn(n: &Node, ds: &DesignSettings) -> Vec<(Part, StitchSettings, bool)> {
    let slots = slots(n);
    let has = |part: Part| match part {
        Part::Fill => fill_rgb(n).is_some(),
        Part::Stroke => stroke_rgb(n).is_some(),
    };
    let wanted = |part: Part| match slots.get(part) {
        Slot::Auto => has(part),
        Slot::Set(s) => s.kind != StitchType::None,
    };
    let (fill, stroke) = match slots.whole_part() {
        Some(part) => (part == Part::Fill, part == Part::Stroke),
        None => {
            let stroke = wanted(Part::Stroke);
            let fill = wanted(Part::Fill) && (has(Part::Fill) || !stroke);
            (fill, stroke && (has(Part::Stroke) || !fill))
        }
    };
    let mut out = vec![];
    let order = if slots.stroke_first { [(Part::Stroke, stroke), (Part::Fill, fill)] } else { [(Part::Fill, fill), (Part::Stroke, stroke)] };
    for (part, on) in order {
        if on {
            let (s, auto) = part_settings(n, ds, part);
            out.push((part, s, auto));
        }
    }
    out
}

/// The settings that describe `n` in one line: its first sewn part's, else the stroke's.
pub fn primary(n: &Node, ds: &DesignSettings) -> (StitchSettings, bool) {
    match sewn(n, ds).into_iter().next() {
        Some((_, s, auto)) => (s, auto),
        None if slots(n) == Slots::AUTO => (StitchSettings::of(StitchType::None), true),
        None => (StitchSettings::of(StitchType::None), false),
    }
}

/// Store `slots` on `n`. A satin, E-stitch or rope stroke shows as wide as it sews: the stroke
/// weight takes its width; a running stitch set this time (`thread`) shows at thread thickness.
pub fn write(n: &mut Node, slots: &Slots, thread: bool) {
    let v = slots.to_value();
    n.edit_attrs(|a| a.stitch.clone_from(&v));
    if let Slot::Set(s) = &slots.stroke
        && let Some(st) = n.appearance.stroke_mut()
    {
        let s = s.sanitized();
        if let Some(w) = width_of(&s).filter(|w| w.is_finite() && *w > 0.0) {
            st.width = w / MM_PER_PT;
        } else if thread && matches!(s.kind, StitchType::Run | StitchType::Triple) {
            st.width = THREAD_MM / MM_PER_PT;
        }
    }
}

/// `stitch.info`'s view of one part: {type, auto, sews, settings}.
pub fn part_json(n: &Node, ds: &DesignSettings, part: Part, sews: bool) -> Value {
    let (s, auto) = part_settings(n, ds, part);
    let kind = if !sews && auto { StitchType::None } else { s.kind };
    json!({ "type": kind.id(), "auto": auto, "sews": sews, "settings": s })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn satin_fill_is_the_fill_part_in_old_and_new_documents() {
        let satin = |s: &Slot| matches!(s, Slot::Set(st) if st.kind == StitchType::SatinBlocks);
        // One setting, as Convert to Satin has always stored it.
        let one = Slots::read(Some(&json!({"type": "satinBlocks"})));
        assert!(satin(&one.fill) && one.stroke.is_off());
        assert_eq!(one.whole_part(), Some(Part::Fill));
        assert_eq!(one.to_value().unwrap()["type"], "satinBlocks");
        // In the stroke part, as digitized lettering stored it before Satin Rails were a fill: read
        // into the fill, and written back as one setting.
        let old = Slots::read(Some(&json!({"fill": {"type": "none"}, "stroke": {"type": "satinBlocks", "satin": {"density": 0.3}}})));
        assert!(satin(&old.fill) && old.stroke.is_off());
        let v = old.to_value().unwrap();
        assert_eq!((v["type"].clone(), v["satin"]["density"].clone()), (json!("satinBlocks"), json!(0.3)));
        // Manual stitches stay the stroke's.
        assert_eq!(Slots::read(Some(&json!({"type": "manual"}))).whole_part(), Some(Part::Stroke));
        assert_eq!(Part::of(StitchType::SatinBlocks), Part::Fill);
        assert_eq!(Part::of(StitchType::Satin), Part::Stroke);
    }
}
