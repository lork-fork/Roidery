//! File › Export Mockup (`stitch.mockup`): a picture of the design sewn on a garment, to show a
//! customer before anything is stitched. The garment (T-shirt, cap, tote bag, or a plain swatch
//! of fabric) is drawn in a scratch session and rendered; `roidery_mockup` gives it the weave of
//! the design's fabric and draws every stitch as realistic thread. The garments are simple
//! outlines drawn for Roidery; no photos are used.

use roidery_formats::{Command, Design};
use roidery_mockup::{Canvas, Segment, Texture, draw_threads, light, texture};
use roidery_plan::PlanSettings;
use serde_json::{Value, json};
use vectorcraft_geom::Rect;

use super::design::DesignSettings;
use super::worksheet::{Sheet, hex};
use super::{all_objects, sew};
use crate::Session;
use crate::cmd::{Result, bad, str_param};

const C: &str = "stitch.mockup";
/// Thread thickness, mm.
const THREAD_MM: f64 = 0.4;
/// Around the garment.
const BACKDROP: [u8; 3] = [0xDD, 0xE0, 0xE5];
/// Largest side of the picture, pixels.
const MAX_SIDE: f64 = 4000.0;

/// A garment: its picture's size (mm), its outline and details (SVG path data in mm: filled
/// shapes, each darkened by a factor, then lines), where the design's centre goes, and the
/// default pixels per mm and colour.
struct Garment {
    id: &'static str,
    size: (f64, f64),
    /// (path, darken: 1 is the garment's colour).
    shapes: &'static [(&'static str, f64)],
    /// (path, darken, width mm).
    lines: &'static [(&'static str, f64, f64)],
    /// Design centre for each placement: (id, x, y).
    places: &'static [(&'static str, f64, f64)],
    ppm: f64,
    color: [u8; 3],
}

const GARMENTS: &[Garment] = &[
    Garment {
        id: "tshirt",
        size: (640.0, 760.0),
        shapes: &[
            (
                "M250 40C225 45 170 58 140 70L40 180L100 262L150 222L152 722Q320 734 488 722L490 222L540 262L600 180L500 70C470 58 415 45 390 40C370 92 270 92 250 40Z",
                1.0,
            ),
            // The inside of the back, seen through the neckline.
            ("M250 40C290 28 350 28 390 40C370 92 270 92 250 40Z", 0.78),
        ],
        lines: &[
            ("M250 40C270 92 370 92 390 40", 0.9, 9.0),
            ("M152 706Q320 718 488 706", 0.93, 1.0),
            ("M55 200L112 250", 0.93, 1.0),
            ("M585 200L528 250", 0.93, 1.0),
        ],
        places: &[("center", 320.0, 240.0), ("leftChest", 405.0, 195.0)],
        ppm: 3.0,
        color: [0xF4, 0xF3, 0xEF],
    },
    Garment {
        id: "cap",
        size: (300.0, 210.0),
        shapes: &[
            ("M25 150C25 62 88 30 150 30C212 30 275 62 275 150Z", 1.0),
            // The brim, seen from the front.
            ("M14 150C70 138 230 138 286 150C262 196 38 196 14 150Z", 0.86),
            ("M144 30a6 6 0 1 0 12 0a6 6 0 1 0 -12 0Z", 0.9),
        ],
        lines: &[("M150 36L150 146", 0.9, 0.8), ("M150 36C110 60 82 100 74 146", 0.9, 0.8), ("M150 36C190 60 218 100 226 146", 0.9, 0.8)],
        places: &[("center", 150.0, 100.0)],
        ppm: 8.0,
        color: [0x23, 0x2E, 0x4A],
    },
    Garment {
        id: "tote",
        size: (420.0, 540.0),
        shapes: &[("M40 150H380V505Q380 515 370 515H50Q40 515 40 505Z", 1.0)],
        lines: &[("M120 152C118 20 300 20 300 152", 0.92, 24.0), ("M42 170H378", 0.93, 1.0)],
        places: &[("center", 210.0, 330.0)],
        ppm: 4.0,
        color: [0xEA, 0xE1, 0xCB],
    },
];

fn rgb_param(v: &str) -> Option<[u8; 3]> {
    let h = v.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

fn darken(rgb: [u8; 3], f: f64) -> String {
    hex(rgb.map(|v| (f64::from(v) * f).round().clamp(0.0, 255.0) as u8))
}

/// The stitches as thread from needle position to needle position, in mm, with their colours.
fn strands(design: &Design) -> Vec<([f64; 2], [f64; 2], [u8; 3])> {
    let mut out = vec![];
    let mut block = 0usize;
    let mut last: Option<[f64; 2]> = None;
    for s in &design.stitches {
        let p = [s.x, s.y];
        match s.command {
            Command::Stitch => {
                if let Some(a) = last {
                    let rgb = design.threads.get(block).map_or([0, 0, 0], |t| t.rgb);
                    out.push((a, p, rgb));
                }
            }
            Command::ColorChange => block += 1,
            Command::End => break,
            Command::Jump | Command::Trim => {}
        }
        last = Some(p);
    }
    out
}

pub(super) fn mockup(s: &mut Session, p: &Value) -> Result<Value> {
    let d = &s.doc()?.doc;
    let ds = DesignSettings::of(d);
    let (_, design) = sew(d, &all_objects(d), &PlanSettings::default(), "");
    if design.stitch_count() == 0 {
        return Err(bad(C, "nothing sews: give some paths a stitch type first"));
    }
    let bounds = design.bounds().ok_or_else(|| bad(C, "nothing sews"))?;
    let garment_id = str_param(p, "garment").unwrap_or("tshirt");
    let garment = match garment_id {
        "swatch" => None,
        id => Some(GARMENTS.iter().find(|g| g.id == id).ok_or_else(|| bad(C, format!("unknown garment `{id}`: tshirt, cap, tote or swatch")))?),
    };
    let color = match str_param(p, "color") {
        Some(c) => rgb_param(c).ok_or_else(|| bad(C, format!("`color` must be #rrggbb, not `{c}`")))?,
        None => garment.map_or([0xF4, 0xF3, 0xEF], |g| g.color),
    };
    let fabric = str_param(p, "fabric").map(str::to_string).unwrap_or_else(|| ds.fabric().id.to_string());
    // The picture's size (mm) and where the design's centre goes.
    let (size, centre) = match garment {
        Some(g) => {
            let place = str_param(p, "placement").unwrap_or("center");
            let (_, x, y) = g.places.iter().find(|(id, _, _)| *id == place).or_else(|| g.places.first()).copied().unwrap_or(("", 0.0, 0.0));
            (g.size, (x, y))
        }
        None => {
            let margin = 25.0;
            let (w, h) = (bounds.width() + 2.0 * margin, bounds.height() + 2.0 * margin);
            ((w, h), (w / 2.0, h / 2.0))
        }
    };
    let want = match p.get("ppm") {
        None | Some(Value::Null) => garment.map_or(12.0, |g| g.ppm),
        Some(v) => v.as_f64().filter(|v| (0.5..=40.0).contains(v)).ok_or_else(|| bad(C, "`ppm` must be 0.5 to 40 pixels per mm"))?,
    };
    let ppm = want.min(MAX_SIDE / size.0.max(size.1));

    // The garment, drawn and rendered on a transparent ground. Its document's units are
    // millimetres (rendered at `ppm` pixels each), as the garment outlines are.
    let mut sh = Sheet { s: Session::new(), labels: None };
    sh.run("file.new", json!({ "width": size.0, "height": size.1 }))?;
    let shape = |sh: &mut Sheet, d: &str, fill: Option<String>, stroke: Option<(String, f64)>| -> Result<()> {
        let id = sh.run("path.create", json!({ "d": d }))?["id"].clone();
        sh.paint(&id, fill.as_deref(), stroke.as_ref().map(|(c, w)| (c.as_str(), *w)), None)
    };
    match garment {
        Some(g) => {
            for (i, (path, f)) in g.shapes.iter().enumerate() {
                // The outline gets a hairline edge a little darker than the cloth.
                let edge = (i == 0).then(|| (darken(color, 0.8), 0.6));
                shape(&mut sh, path, Some(darken(color, *f)), edge)?;
            }
            for (path, f, w) in g.lines {
                shape(&mut sh, path, None, Some((darken(color, *f), *w)))?;
            }
        }
        None => {
            let id = sh.run("shape.rectangle", json!({ "x": 0, "y": 0, "width": size.0, "height": size.1 }))?["id"].clone();
            sh.paint(&id, Some(&hex(color)), None, None)?;
        }
    }
    let doc = sh.s.doc()?.doc.clone();
    let rendered = vectorcraft_render::Renderer::new().render_region(&doc, Rect::new(0.0, 0.0, size.0, size.1), ppm, false);
    let mut canvas = Canvas::from_rgba(rendered.width, rendered.height, rendered.to_straight()).ok_or_else(|| bad(C, "the mockup is too large"))?;
    texture(&mut canvas, Texture::from_id(&fabric), ppm);
    if garment.is_some() {
        light(&mut canvas, 0.18);
    }
    canvas.flatten(BACKDROP);
    let (ox, oy) = (centre.0 - (bounds.min_x + bounds.max_x) / 2.0, centre.1 - (bounds.min_y + bounds.max_y) / 2.0);
    let px = |q: [f64; 2]| [(q[0] + ox) * ppm, (q[1] + oy) * ppm];
    let segments: Vec<Segment> = strands(&design).into_iter().map(|(a, b, rgb)| Segment { a: px(a), b: px(b), rgb }).collect();
    draw_threads(&mut canvas, &segments, THREAD_MM / 2.0 * ppm);

    let format = str_param(p, "format").unwrap_or("png");
    let out = vectorcraft_render::Rendered { width: canvas.width, height: canvas.height, pixels: canvas.rgba };
    let bytes = match format {
        "png" => out.to_png(),
        "jpg" | "jpeg" => out.to_jpeg(92),
        other => return Err(bad(C, format!("unknown format `{other}`: png or jpg"))),
    }
    .map_err(|e| bad(C, e))?;
    let mut r = json!({ "format": if format == "png" { "png" } else { "jpg" }, "width": out.width, "height": out.height, "ppm": ppm, "garment": garment_id, "bytes": bytes.len() });
    match str_param(p, "path") {
        Some(path) => {
            super::super::fileio::write_file(path, &bytes)?;
            r["path"] = json!(path);
        }
        None => r["dataBase64"] = json!(vectorcraft_format::base64_encode(&bytes)),
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    fn png(r: &serde_json::Value) -> image::RgbaImage {
        let bytes = vectorcraft_format::base64_decode(r["dataBase64"].as_str().unwrap()).unwrap();
        image::load_from_memory(&bytes).unwrap().to_rgba8()
    }

    fn design() -> Session {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
        s.execute("paint.setFill", &json!({"color": "#d01020"})).unwrap();
        // 50 × 30 mm.
        s.execute("shape.rectangle", &json!({"x": 20, "y": 20, "width": 141.7, "height": 85.0})).unwrap();
        s
    }

    #[test]
    fn a_swatch_shows_the_design_in_thread_on_the_fabric() {
        let mut s = design();
        let r = s.execute("stitch.mockup", &json!({"garment": "swatch", "ppm": 6, "color": "#203040"})).unwrap();
        // About 50 + 2 × 25 mm wide (pull compensation adds a little) at 6 px a mm.
        let w = r["width"].as_u64().unwrap();
        assert!((598..=606).contains(&w) && r["ppm"] == 6.0, "{r}");
        let img = png(&r);
        // The middle is red thread; the corner is the dark fabric.
        let mid = img.get_pixel(300, img.height() / 2).0;
        assert!(mid[0] > 120 && mid[1] < 80, "{mid:?}");
        let corner = img.get_pixel(5, 5).0;
        assert!(corner[2] > corner[0] && corner[2] < 90, "{corner:?}");
    }

    #[test]
    fn garments_place_the_design_and_bad_params_are_errors() {
        let mut s = design();
        let shirt = s.execute("stitch.mockup", &json!({"ppm": 2})).unwrap();
        assert_eq!((shirt["width"].as_u64(), shirt["height"].as_u64()), (Some(1280), Some(1520)));
        let img = png(&shirt);
        // Centre chest: red at (320, 240) mm; the shirt's own colour beside it; the backdrop
        // beyond the sleeve.
        let at = |x: f64, y: f64| img.get_pixel((x * 2.0) as u32, (y * 2.0) as u32).0;
        assert!(at(320.0, 240.0)[0] > 120 && at(320.0, 240.0)[1] < 90, "{:?}", at(320.0, 240.0));
        assert!(at(250.0, 400.0)[0] > 200, "{:?}", at(250.0, 400.0));
        assert_eq!(&at(10.0, 700.0)[..3], &super::BACKDROP);
        let left = png(&s.execute("stitch.mockup", &json!({"ppm": 2, "placement": "leftChest"})).unwrap());
        assert!(left.get_pixel(810, 390).0[1] < 90);
        for g in ["cap", "tote"] {
            assert!(s.execute("stitch.mockup", &json!({"garment": g, "ppm": 2, "format": "jpg"})).is_ok(), "{g}");
        }
        assert!(s.execute("stitch.mockup", &json!({"garment": "sock"})).is_err());
        assert!(s.execute("stitch.mockup", &json!({"color": "red"})).is_err());
        assert!(s.execute("stitch.mockup", &json!({"ppm": 1000})).is_err());
        let mut empty = Session::new();
        empty.execute("file.new", &json!({})).unwrap();
        assert!(empty.execute("stitch.mockup", &json!({})).is_err());
    }
}
