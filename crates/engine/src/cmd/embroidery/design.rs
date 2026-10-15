//! Roidery: design-wide embroidery settings, kept in [`Document::embroidery`]: the fabric (the
//! automatic rule's stitch settings come from its preset), the thread chart the design's
//! colours are matched against (machine files then name the threads), and the hoop it is sewn in.
//!
//! The page is the hoop: a design with a hoop has its first artboard as the hoop's sewing field
//! ([`HoopField`]). The needle starts at the page's centre, so the design sews where it sits on
//! the page, and a page resized by hand is a custom hoop of the page's size.

use roidery_formats::charts::{self, Chart};
use roidery_formats::hoops::{self, Fit};
use roidery_formats::{Command, Design, Thread};
use roidery_stitch::{FABRICS, Fabric};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Document, NodeId};
use vectorcraft_geom::Rect;

use super::{MM_PER_PT, Part, all_objects, objects_in, part_rgb, parts};
use crate::Session;
use crate::cmd::overprint::edit_counted;
use crate::cmd::{Result, bad, str_param, targets};

/// What [`Document::embroidery`] holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DesignSettings {
    /// [`Fabric::id`]; empty or unknown: the default fabric.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub fabric: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart: Option<Chart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoop: Option<DesignHoop>,
}

/// The hoop a design is sewn in: one from the library (by id) or a custom size.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DesignHoop {
    /// [`hoops::Hoop::id`]; empty for a custom hoop.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub brand: String,
    pub name: String,
    /// Sewing field, mm.
    pub width: f64,
    pub height: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub round: bool,
}

impl DesignHoop {
    /// How a design `w` × `h` mm sits in this hoop, as JSON for the panel and `stitch.export`:
    /// `{fit: "fits"|"rotate"|"tooLarge", overWidthMm?, overHeightMm?}`.
    pub fn fit_json(&self, w: f64, h: f64) -> Value {
        match hoops::fit(w, h, self.width, self.height, self.round) {
            Fit::Fits => json!({ "fit": "fits" }),
            Fit::Rotated => json!({ "fit": "rotate" }),
            Fit::TooLarge { over_w, over_h } => json!({ "fit": "tooLarge", "overWidthMm": over_w, "overHeightMm": over_h }),
        }
    }

    pub fn json(&self) -> Value {
        json!({ "id": self.id, "brand": self.brand, "name": self.name, "widthMm": self.width, "heightMm": self.height, "round": self.round })
    }
}

/// The hoop `v` asks for: a library id, `"none"`/null (no hoop), or `{width, height, name?,
/// round?}` in mm.
fn hoop_param(v: &Value) -> Result<Option<DesignHoop>> {
    const C: &str = "stitch.setDesign";
    if v.is_null() {
        return Ok(None);
    }
    if let Some(id) = v.as_str() {
        if id.eq_ignore_ascii_case("none") || id.is_empty() {
            return Ok(None);
        }
        return library_hoop(id).map(Some).ok_or_else(|| bad(C, format!("unknown hoop `{id}`")));
    }
    let mm = |k: &str| v.get(k).and_then(Value::as_f64).filter(|x| x.is_finite() && (10.0..=2000.0).contains(x));
    let (Some(width), Some(height)) = (mm("width"), mm("height")) else {
        return Err(bad(C, "`hoop` must be a hoop id, \"none\", or {width, height} in mm (10 to 2000)"));
    };
    let name = str_param(v, "name").map_or_else(|| format!("{width} × {height} mm"), str::to_string);
    let round = v.get("round").and_then(Value::as_bool).unwrap_or(false);
    Ok(Some(DesignHoop { id: String::new(), brand: "Custom".into(), name, width, height: if round { width } else { height }, round }))
}

impl DesignSettings {
    /// `d`'s settings (the defaults when it has none, or they don't parse).
    pub fn of(d: &Document) -> Self {
        d.embroidery.as_ref().and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
    }

    pub fn fabric(&self) -> &'static Fabric {
        Fabric::find(&self.fabric).unwrap_or_else(Fabric::default_fabric)
    }
}

/// A custom hoop as big as `w` × `h` mm.
fn custom_hoop(w: f64, h: f64, name: Option<&str>) -> DesignHoop {
    let r = |v: f64| (v * 10.0).round() / 10.0;
    let name = name.map_or_else(|| format!("{} × {} mm", r(w), r(h)), str::to_string);
    DesignHoop { id: String::new(), brand: "Custom".into(), name, width: w, height: h, round: false }
}

/// A library hoop by id.
fn library_hoop(id: &str) -> Option<DesignHoop> {
    let h = hoops::find(id)?;
    Some(DesignHoop { id: h.id.into(), brand: h.brand.into(), name: h.name.into(), width: h.width, height: h.height, round: h.round })
}

/// The embroidery settings of a new document whose page is a hoop: the library hoop `id`, or
/// (`"custom"`) a custom hoop as big as the page, named `name` (else by its size).
pub(crate) fn new_document_hoop(id: &str, w_mm: f64, h_mm: f64, name: Option<&str>) -> Option<Value> {
    let hoop = if id.eq_ignore_ascii_case("custom") { Some(custom_hoop(w_mm, h_mm, name)) } else { library_hoop(id) }?;
    serde_json::to_value(DesignSettings { hoop: Some(hoop), ..DesignSettings::default() }).ok()
}

/// Rounding in a machine file can add a tenth of a millimetre.
const FIT_SLACK_MM: f64 = 0.15;
/// A page this close to its hoop's size (mm) is still that hoop.
const SAME_SIZE_MM: f64 = 0.5;

/// The hoop's sewing field, which is the design's first artboard: the page is the hoop.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HoopField {
    /// The page, document units.
    pub rect: Rect,
    /// The hoop as the page is now: the chosen hoop (turned when the page is), or a custom
    /// hoop as big as a page resized by hand.
    pub hoop: DesignHoop,
}

impl HoopField {
    /// `d`'s sewing field; None when the design has no hoop (or no artboard).
    pub fn of(d: &Document) -> Option<Self> {
        let hoop = DesignSettings::of(d).hoop?;
        let rect = d.artboards.first()?.rect;
        let (w, h) = (rect.width() * MM_PER_PT, rect.height() * MM_PER_PT);
        let same = |a: f64, b: f64| (a - b).abs() <= SAME_SIZE_MM;
        let hoop = if same(w, hoop.width) && same(h, hoop.height) {
            hoop
        } else if !hoop.round && same(w, hoop.height) && same(h, hoop.width) {
            DesignHoop { width: hoop.height, height: hoop.width, ..hoop }
        } else {
            custom_hoop(w, h, None)
        };
        Some(Self { rect, hoop })
    }

    /// Where the needle starts (the page's centre), mm: the stitch plan's origin.
    pub fn origin_mm(&self) -> (f64, f64) {
        let c = self.rect.center();
        (c.x * MM_PER_PT, c.y * MM_PER_PT)
    }

    /// How far (mm) the stitches of `design`, planned from [`Self::origin_mm`], go past the
    /// field's edge (0 when they are all inside).
    pub fn outside_mm(&self, design: &Design) -> f64 {
        let (hw, hh) = (self.hoop.width / 2.0, self.hoop.height / 2.0);
        design
            .stitches
            .iter()
            .filter(|s| s.command == Command::Stitch)
            .map(|s| if self.hoop.round { s.x.hypot(s.y) - hw } else { (s.x.abs() - hw).max(s.y.abs() - hh) })
            .fold(0.0f64, f64::max)
    }

    /// How `design` (planned from [`Self::origin_mm`], `w` × `h` mm) sits in the hoop where it
    /// is on the page: `{fit: "fits"}`, `{fit: "outside", outsideMm}` (small enough, but partly
    /// off the page), or the hoop's size check (`tooLarge`, `rotate`) with `outsideMm`.
    pub fn fit_json(&self, design: &Design, w: f64, h: f64) -> Value {
        let out = self.outside_mm(design);
        if out <= FIT_SLACK_MM {
            return json!({ "fit": "fits" });
        }
        let mut v = if self.hoop.round {
            // A round hoop holds the design if the smallest circle round its stitches fits: its
            // bounding box's diagonal would turn a star away.
            let pts: Vec<(f64, f64)> = design.stitches.iter().filter(|s| s.command == Command::Stitch).map(|s| (s.x, s.y)).collect();
            let over = (hoops::enclosing_circle(&pts).map_or(0.0, |c| c.2) - self.hoop.width).max(0.0);
            if over <= FIT_SLACK_MM { json!({ "fit": "fits" }) } else { json!({ "fit": "tooLarge", "overWidthMm": over, "overHeightMm": over }) }
        } else {
            self.hoop.fit_json(w, h)
        };
        if v["fit"] == "fits" {
            v["fit"] = json!("outside");
        }
        v["outsideMm"] = json!((out * 10.0).round() / 10.0);
        v
    }

    /// The field for the UI, document units.
    pub fn rect_json(&self) -> Value {
        json!({ "x": self.rect.x0, "y": self.rect.y0, "width": self.rect.width(), "height": self.rect.height() })
    }
}

fn thread_json(t: &Thread) -> Value {
    json!({ "rgb": t.rgb, "name": t.name, "code": t.code })
}

/// The design's sewing colours, first-sewn first: (rgb, the objects sewn in it).
fn design_colors(d: &Document, ds: &DesignSettings) -> Vec<([u8; 3], Vec<NodeId>)> {
    let mut out: Vec<([u8; 3], Vec<NodeId>)> = vec![];
    for id in all_objects(d) {
        let Some(n) = d.node(id) else { continue };
        for (part, ..) in parts::sewn(n, ds) {
            let Some((rgb, _)) = part_rgb(n, part) else { continue };
            match out.iter_mut().find(|c| c.0 == rgb) {
                Some(c) if c.1.last() != Some(&id) => c.1.push(id),
                Some(_) => {}
                None => out.push((rgb, vec![id])),
            }
        }
    }
    out
}

/// `stitch.design`.
pub(super) fn design(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = &s.doc()?.doc;
    let ds = DesignSettings::of(d);
    let field = HoopField::of(d);
    let fabric = ds.fabric();
    let threads: Vec<Value> = design_colors(d, &ds)
        .into_iter()
        .map(|(rgb, objects)| {
            let chart = ds.chart.as_ref();
            let exact = chart.and_then(|c| c.exact(rgb));
            let nearest = chart.and_then(|c| c.nearest(rgb)).map(|(t, de)| {
                let mut v = thread_json(t);
                v["deltaE"] = json!((de * 10.0).round() / 10.0);
                v
            });
            json!({
                "rgb": rgb,
                "name": exact.map(|t| t.name.as_str()).unwrap_or_default(),
                "code": exact.map(|t| t.code.as_str()).unwrap_or_default(),
                "inChart": exact.is_some(),
                "nearest": nearest,
                "objects": objects,
            })
        })
        .collect();
    Ok(json!({
        "fabric": fabric.id,
        "fabricLabel": fabric.label,
        "fabrics": FABRICS.iter().map(|f| json!({ "id": f.id, "label": f.label })).collect::<Vec<_>>(),
        "chart": ds.chart.as_ref().map(|c| json!({ "name": c.name, "threads": c.threads.len() })),
        "charts": charts::BUILT_IN.iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>(),
        "threads": threads,
        "hoop": field.as_ref().map(|f| f.hoop.json()).or_else(|| ds.hoop.as_ref().map(DesignHoop::json)),
        "field": field.as_ref().map(HoopField::rect_json),
        "hoops": hoops::HOOPS.iter().map(|h| json!({ "id": h.id, "brand": h.brand, "name": h.name, "widthMm": h.width, "heightMm": h.height, "round": h.round })).collect::<Vec<_>>(),
    }))
}

/// The chart `v` asks for: a built-in id, `"none"`, or an object with `threads`, `text` (a GIMP
/// palette or CSV) or `path` (such a file), and an optional `name`.
fn chart_param(v: &Value) -> Result<Option<Chart>> {
    const C: &str = "stitch.setDesign";
    if let Some(id) = v.as_str() {
        if id.eq_ignore_ascii_case("none") || id.is_empty() {
            return Ok(None);
        }
        return charts::built_in(id).map(Some).ok_or_else(|| bad(C, format!("unknown thread chart `{id}`")));
    }
    if v.is_null() {
        return Ok(None);
    }
    let name = str_param(v, "name").unwrap_or("Imported Chart");
    if let Some(threads) = v.get("threads") {
        let threads: Vec<Thread> = serde_json::from_value(threads.clone()).map_err(|e| bad(C, format!("bad threads: {e}")))?;
        if threads.is_empty() || threads.len() > charts::MAX_THREADS {
            return Err(bad(C, format!("a chart holds 1 to {} threads", charts::MAX_THREADS)));
        }
        return Ok(Some(Chart { name: name.into(), threads }));
    }
    let (text, fallback) = match (str_param(v, "text"), str_param(v, "path")) {
        (Some(t), _) => (t.to_string(), name.to_string()),
        (None, Some(path)) => {
            let bytes = super::super::fileio::read_file(path)?;
            if bytes.len() > charts::MAX_CHART_BYTES {
                return Err(bad(C, "the chart file is too large"));
            }
            let stem = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned());
            (String::from_utf8_lossy(&bytes).into_owned(), str_param(v, "name").map(str::to_string).or(stem).unwrap_or_else(|| name.into()))
        }
        (None, None) => return Err(bad(C, "`chart` must be a chart id, \"none\", or {name?, threads | text | path}")),
    };
    charts::parse_chart(&text, &fallback).map(Some).map_err(|e| bad(C, e))
}

/// `stitch.setDesign`.
pub(super) fn set_design(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.setDesign";
    let fabric = str_param(p, "fabric").map(|f| Fabric::find(f).ok_or_else(|| bad(C, format!("unknown fabric `{f}`")))).transpose()?;
    let chart = p.get("chart").map(chart_param).transpose()?;
    let hoop = p.get("hoop").map(hoop_param).transpose()?;
    if fabric.is_none() && chart.is_none() && hoop.is_none() {
        return Err(bad(C, "give fabric, chart or hoop"));
    }
    let old = DesignSettings::of(&s.doc()?.doc);
    let mut new = old.clone();
    if let Some(f) = fabric {
        new.fabric = f.id.into();
    }
    if let Some(c) = chart {
        new.chart = c;
    }
    if let Some(h) = hoop.clone() {
        new.hoop = h;
    }
    // The page is the hoop: a hoop chosen from here sizes the first artboard to its sewing field, about its centre.
    let field = hoop.flatten();
    let refit = fabric.is_some() && p.get("applyToObjects").and_then(Value::as_bool).unwrap_or(true);
    let value = serde_json::to_value(&new).map_err(|e| bad(C, e.to_string()))?;
    let changed = edit_counted(s, "Embroidery Design", |d| {
        let mut count = usize::from(new != old);
        d.embroidery = Some(value);
        if let Some(h) = &field
            && let Some(a) = d.artboards.first_mut()
        {
            let (w, ht) = (h.width / super::MM_PER_PT, h.height / super::MM_PER_PT);
            let c = a.rect.center();
            let rect = vectorcraft_geom::Rect::new(c.x - w / 2.0, c.y - ht / 2.0, c.x + w / 2.0, c.y + ht / 2.0);
            if a.rect != rect || !a.show_center_mark {
                a.rect = rect;
                // The needle starts at the hoop's centre.
                a.show_center_mark = true;
                count += 1;
            }
        }
        // Objects with stitch settings of their own take the new fabric's values too.
        if let (true, Some(f)) = (refit, fabric) {
            for id in all_objects(d) {
                let Some(n) = d.node_mut(id) else { continue };
                let old = parts::slots(n);
                let mut new = old.clone();
                for part in Part::BOTH {
                    if let parts::Slot::Set(st) = new.get_mut(part) {
                        f.apply(st);
                    }
                }
                if new != old {
                    parts::write(n, &new, false);
                    count += 1;
                }
            }
        }
        Ok(count)
    })?;
    Ok(
        json!({ "fabric": new.fabric().id, "chart": new.chart.as_ref().map(|c| c.name.clone()), "hoop": new.hoop.as_ref().map(DesignHoop::json), "changed": changed }),
    )
}

/// `stitch.matchThreads`.
pub(super) fn match_threads(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "stitch.matchThreads";
    let d = &s.doc()?.doc;
    let ds = DesignSettings::of(d);
    let chart = ds.chart.clone().ok_or_else(|| bad(C, "choose a thread chart first (stitch.setDesign {chart})"))?;
    let sel = targets(s, p)?;
    let ids = if sel.is_empty() { all_objects(d) } else { objects_in(d, &sel) };
    // (object, fill?, thread colour) for every sewn colour that isn't a chart thread yet.
    let chart_ref = &chart;
    let todo: Vec<(NodeId, bool, [u8; 3])> = ids
        .iter()
        .filter_map(|id| d.node(*id))
        .flat_map(|n| {
            parts::sewn(n, &ds).into_iter().filter_map(move |(part, ..)| {
                let (rgb, fill) = part_rgb(n, part)?;
                let (t, _) = chart_ref.nearest(rgb)?;
                (t.rgb != rgb).then_some((n.id, fill, t.rgb))
            })
        })
        .collect();
    let changed = edit_counted(s, "Match to Thread Chart", |d| {
        let mut count = 0;
        for (id, fill, [r, g, b]) in &todo {
            let Some(n) = d.node_mut(*id) else { continue };
            let paint = Paint::solid(Color::rgb8(*r, *g, *b));
            if *fill {
                n.appearance.set_fill(paint);
            } else {
                n.appearance.set_stroke(paint);
            }
            count += 1;
        }
        Ok(count)
    })?;
    Ok(json!({ "changed": changed, "chart": chart.name }))
}
