//! Roidery: the canvas's embroidery views (View → Stitch View, `view.stitches`).
//!
//! - **Stitches**: every stitch as a thin line in its thread colour on plain fabric, as a
//!   digitizer reads a design.
//! - **Realistic**: each stitch drawn as a strand of thread, its true width at the current zoom,
//!   with a shadow and a sheen across it, so densities and angles look as they will sew out.
//!
//! The vector art is covered by the fabric in both views. The preview comes from the engine's
//! stitch plan and is cached per document revision.
//!
//! View → Sew-Out Player sews the design stitch by stitch in either view, with the needle marked,
//! as the machine will: play, pause, step, scrub and change speed from the bar under the canvas.
//!
//! View → Density Map shades where thread piles up over either view: yellow where the fabric is
//! covered more than once and a half, orange where it is dense, red where it is too dense to sew
//! well, with a key in the corner.

use std::sync::Arc;

use egui::{Color32, CornerRadius, Painter, Shape, Stroke, pos2, vec2};
use serde_json::{Value, json};
use vectorcraft_engine::StitchPreview;
use vectorcraft_engine::stitch_review::{DENSE_LAYERS, DensityMap, TOO_DENSE_LAYERS};
use vectorcraft_geom::Point;

use crate::VectorcraftApp;
use crate::canvas::Xf;
use crate::theme::Tokens;
use crate::widgets::icon_button;

/// The views, in the order `view.stitches` cycles through them.
pub const VIEWS: [&str; 3] = ["vector", "stitches", "realistic"];

/// [`crate::CanvasCache::stitches`]: the stitch preview of (document uid, revision).
pub type StitchCache = ((u64, u64), Arc<StitchPreview>);

/// Thread thickness on the fabric, mm (a 40 wt thread lies about this wide).
const THREAD_MM: f64 = 0.4;
const PT_PER_MM: f64 = 72.0 / 25.4;
/// More stitches than this are drawn as plain lines even in the realistic view.
const MAX_REALISTIC: usize = 400_000;

/// The player's speeds, stitches per second.
pub const SPEEDS: [f64; 6] = [10.0, 50.0, 200.0, 1_000.0, 5_000.0, 25_000.0];
const MAX_SPEED: f64 = 1_000_000.0;

/// View → Sew-Out Player, while it is open ([`crate::state::UiState::sew_out`]).
#[derive(Clone, Debug, PartialEq)]
pub struct SewOut {
    /// Stitch records sewn so far, jumps included (fractional while playing).
    pub pos: f64,
    pub playing: bool,
    /// Stitches per second.
    pub speed: f64,
}

impl Default for SewOut {
    /// Nothing sewn yet, sewing at 200 stitches a second.
    fn default() -> Self {
        Self { pos: 0.0, playing: true, speed: 200.0 }
    }
}

impl SewOut {
    /// Records sewn, as a count.
    fn sewn(&self) -> usize {
        // A saturating cast: negative and NaN give 0.
        self.pos as usize
    }
}

/// The view `name` names, as an index into [`VIEWS`].
pub fn parse_view(name: &str) -> Option<u8> {
    VIEWS.iter().position(|v| v.eq_ignore_ascii_case(name)).and_then(|i| u8::try_from(i).ok())
}

/// The label of the menu item, naming the view it switches to next.
pub fn next_label(current: u8) -> &'static str {
    match current {
        0 => "Stitch View",
        1 => "Realistic View",
        _ => "Vector View",
    }
}

fn preview(app: &mut VectorcraftApp) -> Option<Arc<StitchPreview>> {
    let st = app.session.active()?;
    let key = (st.uid, st.revision);
    if let Some((k, p)) = &app.canvas.stitches
        && *k == key
    {
        return Some(p.clone());
    }
    let p = Arc::new(vectorcraft_engine::stitch_preview(&st.doc));
    app.canvas.stitches = Some((key, p.clone()));
    Some(p)
}

fn fabric(realistic: bool) -> Color32 {
    if realistic { Color32::from_rgb(236, 233, 226) } else { Color32::from_rgb(250, 250, 248) }
}

fn shade(c: [u8; 3], f: f32) -> Color32 {
    let m = |v: u8| -> u8 {
        let v = f32::from(v);
        let out = if f >= 0.0 { v + (255.0 - v) * f } else { v * (1.0 + f) };
        out.clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(m(c[0]), m(c[1]), m(c[2]))
}

/// Paint the current embroidery view over the canvas `rect` (nothing in the vector view), and
/// the hoop over any view when View › Stitch Overlays › Hoop is on.
pub fn paint(app: &mut VectorcraftApp, painter: &Painter, xf: &Xf, rect: egui::Rect) {
    if app.ui.view.stitches != 0 {
        paint_stitches(app, painter, xf, rect);
    }
    if app.ui.view.hoop {
        hoop(app, painter, xf);
    }
}

/// Turn on a stitch view (the stitch lines) when `on` and the vector art is showing: the
/// overlays that draw over stitches need one.
pub fn show_stitches(app: &mut VectorcraftApp, on: bool) {
    if on && app.ui.view.stitches == 0 {
        app.ui.view.stitches = 1;
    }
}

/// A faded copy of a thread colour, for Current Color Only: most of the way to the fabric.
fn faded(c: [u8; 3]) -> [u8; 3] {
    let f = |v: u8| -> u8 { (f32::from(v) * 0.22 + 248.0 * 0.78).round().clamp(0.0, 255.0) as u8 };
    [f(c[0]), f(c[1]), f(c[2])]
}

/// The preview as the views show it: without underlay when View › Stitch Overlays › Underlay is
/// off, and the threads other than the current colour faded. A move that stood for hidden
/// underlay is marked `underlay` and doesn't sew, so neither stitches nor jumps show for it.
fn shown(pv: &StitchPreview, hide_underlay: bool, solo: Option<[u8; 3]>) -> StitchPreview {
    let n = pv.threads.len();
    let mut threads = pv.threads.clone();
    if solo.is_some() {
        threads.extend(pv.threads.iter().map(|t| faded(*t)));
    }
    let mut stitches = Vec::with_capacity(pv.stitches.len());
    let mut skipped = false;
    for s in &pv.stitches {
        if hide_underlay && s.underlay {
            skipped = true;
            continue;
        }
        let mut s = *s;
        if std::mem::take(&mut skipped) && !s.trim {
            s.sew = false;
            s.underlay = true;
        }
        if let Some(c) = solo
            && pv.threads.get(s.thread) != Some(&c)
        {
            s.thread += n;
        }
        stitches.push(s);
    }
    StitchPreview { stitches, threads, density: pv.density.clone() }
}

/// A preview and the key the realistic view caches it by.
type Keyed = (Arc<StitchPreview>, Option<(u64, u64)>);

/// [`preview`], as the views show it ([`shown`]), with the key the realistic view caches it by.
fn shown_preview(app: &mut VectorcraftApp, ctx: &egui::Context) -> Option<Keyed> {
    let pv = preview(app)?;
    let key = app.canvas.stitches.as_ref().map(|c| c.0);
    // The sew-out player counts every stitch, so it shows the underlay it sews.
    let (hide, solo) = (app.ui.view.hide_underlay && app.ui.sew_out.is_none(), app.ui.view.solo);
    if !hide && solo.is_none() {
        return Some((pv, key));
    }
    type Cached = ((Option<(u64, u64)>, bool, Option<[u8; 3]>), Arc<StitchPreview>);
    let id = egui::Id::new("stitch-preview-shown");
    let want = (key, hide, solo);
    let cached = ctx.data(|d| d.get_temp::<Cached>(id)).filter(|(k, _)| *k == want).map(|(_, p)| p);
    let p = cached.unwrap_or_else(|| {
        let p = Arc::new(shown(&pv, hide, solo));
        ctx.data_mut(|d| d.insert_temp(id, (want, p.clone())));
        p
    });
    // A key of its own, so the realistic view uploads this one rather than the full preview.
    let tag = 1 + u64::from(hide) + solo.map_or(0, |c| (u64::from(c[0]) << 16 | u64::from(c[1]) << 8 | u64::from(c[2])) << 2);
    Some((p, key.map(|(uid, rev)| (uid, rev ^ (tag << 40)))))
}

/// `view.soloColor`: show one thread colour (see its params in [`crate::menus`]).
pub fn solo_command(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    let color = match p.get("color") {
        Some(Value::Null) => None,
        Some(Value::String(h)) => Some(parse_hex(h).ok_or_else(|| format!("`{h}` is not a #rrggbb colour"))?),
        Some(_) => return Err("color must be \"#rrggbb\" or null".into()),
        None if app.ui.view.solo.is_some() => None,
        None => {
            let selected = app.session.active().is_some_and(|st| !st.selection.is_empty());
            let picked = if selected {
                app.session.execute("stitch.info", &json!({})).ok().and_then(|i| thread_rgb(&i["objects"][0]["thread"]))
            } else {
                None
            };
            let first = picked.or_else(|| preview(app).and_then(|pv| pv.threads.first().copied()));
            Some(first.ok_or("nothing to stitch yet")?)
        }
    };
    app.ui.view.solo = color;
    show_stitches(app, color.is_some());
    Ok(color.map_or(Value::Null, |c| json!(hex(c))))
}

/// `[r, g, b]` from JSON.
pub fn thread_rgb(v: &Value) -> Option<[u8; 3]> {
    let a = v.as_array()?;
    let c = |i: usize| a.get(i).and_then(Value::as_u64).and_then(|x| u8::try_from(x).ok());
    Some([c(0)?, c(1)?, c(2)?])
}

/// `#rrggbb`.
pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let c = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok());
    Some([c(0)?, c(2)?, c(4)?])
}

/// The stitches in doc units, as a box (`None` with no stitches).
fn stitch_bounds(pv: &StitchPreview) -> Option<(Point, Point)> {
    let mut it = pv.stitches.iter().filter(|s| s.x.is_finite() && s.y.is_finite());
    let first = it.next()?;
    let (mut lo, mut hi) = (Point::new(first.x, first.y), Point::new(first.x, first.y));
    for s in it {
        lo = Point::new(lo.x.min(s.x), lo.y.min(s.y));
        hi = Point::new(hi.x.max(s.x), hi.y.max(s.y));
    }
    Some((lo, hi))
}

/// View › Stitch Overlays › Hoop: the design's hoop round the stitches (the machine centres the
/// design in it), else round the first artboard; nothing without a hoop.
fn hoop(app: &mut VectorcraftApp, painter: &Painter, xf: &Xf) {
    let Some(q) = crate::panels::threads::design(app, painter.ctx()) else { return };
    let h = &q["hoop"];
    let (Some(w), Some(hh)) = (h["widthMm"].as_f64().filter(|v| *v > 0.0), h["heightMm"].as_f64().filter(|v| *v > 0.0)) else { return };
    let round = h["round"].as_bool().unwrap_or(false);
    let center = match preview(app).as_deref().and_then(stitch_bounds) {
        Some((lo, hi)) => Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0),
        None => match app.session.active().and_then(|st| st.doc.artboards.first().map(|a| a.rect.center())) {
            Some(c) => c,
            None => return,
        },
    };
    let (w, hh) = (w * PT_PER_MM, hh * PT_PER_MM);
    let field = egui::Rect::from_two_pos(
        xf.to_screen(Point::new(center.x - w / 2.0, center.y - hh / 2.0)),
        xf.to_screen(Point::new(center.x + w / 2.0, center.y + hh / 2.0)),
    );
    let frame_pad = (12.0 * PT_PER_MM * xf.zoom) as f32;
    let frame = field.expand(frame_pad);
    let ring = Color32::from_rgba_unmultiplied(120, 120, 128, 150);
    let edge = Color32::from_rgb(40, 140, 220);
    let dash = |pts: &[egui::Pos2]| Shape::dashed_line(pts, Stroke::new(1.5, edge), 6.0, 4.0);
    if round {
        painter.circle_stroke(frame.center(), frame.width() / 2.0, Stroke::new((frame_pad * 0.5).max(3.0), ring));
        let r = field.width() / 2.0;
        let pts: Vec<egui::Pos2> = (0..=96).map(|i| field.center() + egui::Vec2::angled(i as f32 / 96.0 * std::f32::consts::TAU) * r).collect();
        painter.extend(dash(&pts));
    } else {
        let corner = CornerRadius::same((frame_pad * 1.4).clamp(4.0, 120.0) as u8);
        painter.rect_stroke(frame, corner, Stroke::new((frame_pad * 0.5).max(3.0), ring), egui::StrokeKind::Middle);
        let pts = [field.left_top(), field.right_top(), field.right_bottom(), field.left_bottom(), field.left_top()];
        painter.extend(dash(&pts));
    }
    let name = h["name"].as_str().unwrap_or_default();
    painter.text(field.left_top() + vec2(4.0, 4.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(11.0), edge);
}

/// View › Stitch Overlays › Jumps and Trims and Needle Points, over the stitches sewn so far.
fn overlays(app: &VectorcraftApp, painter: &Painter, xf: &Xf, rect: egui::Rect, pv: &StitchPreview, sewn: Option<usize>) {
    let (jumps, points) = (app.ui.view.jumps, app.ui.view.needle_points);
    if !jumps && !points {
        return;
    }
    let cull = rect.expand(8.0);
    let jump = Color32::from_rgb(214, 38, 160);
    let trim = Color32::from_rgb(220, 38, 38);
    let dot = (THREAD_MM * 0.35 * PT_PER_MM * xf.zoom).clamp(0.9, 2.6) as f32;
    let mut shapes: Vec<Shape> = vec![];
    let mut dots = 0usize;
    let mut prev: Option<egui::Pos2> = None;
    for s in pv.stitches.iter().take(sewn.unwrap_or(usize::MAX)) {
        let p = xf.to_screen(Point::new(s.x, s.y));
        if jumps
            && !s.sew
            && !s.underlay
            && let Some(a) = prev
            && egui::Rect::from_two_pos(a, p).intersects(cull)
            && a.distance(p) > 0.5
        {
            shapes.extend(Shape::dashed_line(&[a, p], Stroke::new(1.3, jump), 5.0, 3.0));
            if s.trim && cull.contains(a) {
                // A cut mark where the thread is trimmed: a small ring with a cross.
                shapes.push(Shape::circle_filled(a, 5.0, Color32::WHITE));
                shapes.push(Shape::circle_stroke(a, 5.0, Stroke::new(1.5, trim)));
                let d = vec2(2.6, 2.6);
                shapes.push(Shape::line_segment([a - d, a + d], Stroke::new(1.5, trim)));
                shapes.push(Shape::line_segment([a + vec2(-d.x, d.y), a + vec2(d.x, -d.y)], Stroke::new(1.5, trim)));
            }
        }
        if points && s.sew && cull.contains(p) && dots < MAX_DOTS {
            shapes.push(Shape::circle_filled(p, dot, Color32::from_black_alpha(190)));
            dots += 1;
        }
        prev = Some(p);
    }
    painter.extend(shapes);
}

/// Needle points drawn at most (the rest of a huge design at low zoom would be a solid blur).
const MAX_DOTS: usize = 250_000;

fn paint_stitches(app: &mut VectorcraftApp, painter: &Painter, xf: &Xf, rect: egui::Rect) {
    let mode = app.ui.view.stitches;
    let Some((pv, shown_key)) = shown_preview(app, painter.ctx()) else { return };
    #[cfg(not(feature = "wgpu"))]
    let _ = shown_key;
    // The sew-out player shows only what the machine has sewn so far.
    let sewn = app.ui.sew_out.as_ref().map(SewOut::sewn);
    #[cfg(feature = "wgpu")]
    let gpu = mode >= 2 && crate::thread_gpu::available();
    #[cfg(not(feature = "wgpu"))]
    let gpu = false;
    let realistic = mode >= 2 && (gpu || pv.stitches.len() <= MAX_REALISTIC);
    painter.rect_filled(rect, 0.0, fabric(realistic));
    #[cfg(feature = "wgpu")]
    if gpu
        && !pv.stitches.is_empty()
        && let Some(key) = shown_key
    {
        // doc → screen as an affine map, from where three points land.
        let (o, x, y) = (xf.to_screen(Point::new(0.0, 0.0)), xf.to_screen(Point::new(1.0, 0.0)), xf.to_screen(Point::new(0.0, 1.0)));
        let frame = crate::thread_gpu::Frame {
            m: [x.x - o.x, y.x - o.x, x.y - o.y, y.y - o.y],
            offset: [o.x, o.y],
            rect,
            half_width: ((THREAD_MM * PT_PER_MM * xf.zoom) as f32 / 2.0).max(0.6),
            sewn,
        };
        painter.add(crate::thread_gpu::callback(pv.clone(), key, frame));
        overlays(app, painter, xf, rect, &pv, sewn);
        if app.ui.view.density {
            density_map(painter, xf, rect, &pv.density);
        }
        needle(painter, sewn, pv.stitches.get(sewn.unwrap_or(0).saturating_sub(1)).map(|s| xf.to_screen(Point::new(s.x, s.y))));
        return;
    }
    let cull = rect.expand(8.0);
    let width = (THREAD_MM * PT_PER_MM * xf.zoom) as f32;
    let mut shapes: Vec<Shape> = Vec::with_capacity(pv.stitches.len() * if realistic { 3 } else { 1 });
    let mut prev: Option<egui::Pos2> = None;
    for s in pv.stitches.iter().take(sewn.unwrap_or(usize::MAX)) {
        let p = xf.to_screen(Point::new(s.x, s.y));
        if s.sew
            && let Some(a) = prev
            && (cull.contains(a) || cull.contains(p))
        {
            let rgb = pv.threads.get(s.thread).copied().unwrap_or([0, 0, 0]);
            if realistic {
                let w = width.max(1.0);
                // Shadow a little down and right, the thread body, then a sheen down its middle.
                let off = egui::vec2(w * 0.12, w * 0.18);
                shapes.push(Shape::line_segment([a + off, p + off], Stroke::new(w * 1.1, Color32::from_black_alpha(60))));
                shapes.push(Shape::line_segment([a, p], Stroke::new(w, shade(rgb, -0.18))));
                shapes.push(Shape::line_segment([a, p], Stroke::new(w * 0.4, shade(rgb, 0.22))));
            } else {
                shapes.push(Shape::line_segment([a, p], Stroke::new(1.0, Color32::from_rgb(rgb[0], rgb[1], rgb[2]))));
            }
        }
        prev = Some(p);
    }
    painter.extend(shapes);
    overlays(app, painter, xf, rect, &pv, sewn);
    if app.ui.view.density {
        density_map(painter, xf, rect, &pv.density);
    }
    needle(painter, sewn, prev);
    if pv.stitches.is_empty() {
        painter.text(
            pos2(rect.center().x, rect.top() + 24.0),
            egui::Align2::CENTER_TOP,
            crate::tl!("Nothing to stitch: draw a shape with a fill or a stroke"),
            egui::FontId::proportional(13.0),
            Color32::from_gray(110),
        );
    }
}

/// Layers of thread above which View → Density Map starts shading: covered more than once and a
/// half.
const HEAVY_LAYERS: f64 = 1.5;

/// The shade for a cell with this many layers of thread, if it gets one.
fn density_color(layers: f64) -> Option<Color32> {
    if layers > TOO_DENSE_LAYERS {
        Some(Color32::from_rgba_unmultiplied(220, 38, 38, 150))
    } else if layers > DENSE_LAYERS {
        Some(Color32::from_rgba_unmultiplied(249, 115, 22, 120))
    } else if layers > HEAVY_LAYERS {
        Some(Color32::from_rgba_unmultiplied(250, 204, 21, 90))
    } else {
        None
    }
}

/// View → Density Map: shade each cell of the map by its layers of thread, and the key.
fn density_map(painter: &Painter, xf: &Xf, rect: egui::Rect, m: &DensityMap) {
    let mm = |v: f64| v * PT_PER_MM;
    let mut shapes = vec![];
    for r in 0..m.rows {
        for c in 0..m.cols {
            let Some(color) = m.layers.get(r * m.cols + c).and_then(|l| density_color(f64::from(*l))) else { continue };
            let (x, y) = (m.x0 + c as f64 * m.cell, m.y0 + r as f64 * m.cell);
            let a = xf.to_screen(Point::new(mm(x), mm(y)));
            let b = xf.to_screen(Point::new(mm(x + m.cell), mm(y + m.cell)));
            let cell = egui::Rect::from_two_pos(a, b);
            if cell.intersects(rect) {
                shapes.push(Shape::rect_filled(cell, 0.0, color));
            }
        }
    }
    painter.extend(shapes);
    // The key, bottom left.
    let keys =
        [(HEAVY_LAYERS + 0.1, crate::tl!("Heavy")), (DENSE_LAYERS + 0.1, crate::tl!("Dense")), (TOO_DENSE_LAYERS + 0.1, crate::tl!("Too dense"))];
    let font = egui::FontId::proportional(12.0);
    let mut x = rect.left() + 12.0;
    let y = rect.bottom() - 26.0;
    let galleys: Vec<_> = keys.iter().map(|(_, label)| painter.layout_no_wrap((*label).to_string(), font.clone(), Color32::from_gray(40))).collect();
    let width: f32 = galleys.iter().map(|g| g.size().x + 28.0).sum::<f32>() + 8.0;
    painter.rect_filled(egui::Rect::from_min_size(pos2(x - 6.0, y - 6.0), vec2(width, 24.0)), CornerRadius::same(4), Color32::from_white_alpha(225));
    for ((layers, _), galley) in keys.iter().zip(galleys) {
        if let Some(c) = density_color(*layers) {
            painter.rect_filled(egui::Rect::from_min_size(pos2(x, y + 1.0), vec2(12.0, 10.0)), CornerRadius::same(2), c);
        }
        let w = galley.size().x;
        painter.galley(pos2(x + 16.0, y - 1.0), galley, Color32::from_gray(40));
        x += w + 28.0;
    }
}

/// The needle, on the last stitch sewn (while the sew-out player is open).
fn needle(painter: &Painter, sewn: Option<usize>, at: Option<egui::Pos2>) {
    if let (Some(_), Some(p)) = (sewn, at) {
        painter.circle_stroke(p, 7.0, Stroke::new(3.0, Color32::from_black_alpha(140)));
        painter.circle_stroke(p, 7.0, Stroke::new(1.5, Color32::WHITE));
        painter.circle_filled(p, 2.0, Color32::from_rgb(230, 40, 60));
    }
}

/// `view.sewOut`: open, close or drive the player (see its params in [`crate::menus`]).
pub fn sew_out_command(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    let total = preview(app).ok_or("no document")?.stitches.len();
    let driven = ["play", "pos", "speed"].iter().any(|k| p.get(*k).is_some());
    let open = match p.get("open") {
        Some(v) => v.as_bool().ok_or("open must be true or false")?,
        None => driven || app.ui.sew_out.is_none(),
    };
    if !open {
        app.ui.sew_out = None;
        return Ok(json!({"open": false, "pos": 0, "total": total, "playing": false}));
    }
    let mut so = app.ui.sew_out.take().unwrap_or_default();
    if app.ui.view.stitches == 0 {
        app.ui.view.stitches = 2;
    }
    let end = total as f64;
    let number = |k: &str| -> Result<Option<f64>, String> {
        match p.get(k) {
            None => Ok(None),
            Some(v) => v.as_f64().filter(|x| x.is_finite()).map(Some).ok_or(format!("{k} must be a number")),
        }
    };
    let pos = number("pos");
    let speed = number("speed");
    let play = match p.get("play") {
        Some(v) => Some(v.as_bool().ok_or("play must be true or false")?),
        None => None,
    };
    // Validate everything before changing anything.
    let (pos, speed) = match (pos, speed) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            app.ui.sew_out = Some(so);
            return Err(e);
        }
    };
    if let Some(x) = pos {
        so.pos = x.clamp(0.0, end);
    }
    if let Some(x) = speed {
        so.speed = x.clamp(1.0, MAX_SPEED);
    }
    if let Some(b) = play {
        // Playing from the end starts over.
        if b && pos.is_none() && so.pos >= end {
            so.pos = 0.0;
        }
        so.playing = b;
    }
    so.pos = so.pos.clamp(0.0, end);
    let out = json!({"open": true, "pos": so.sewn(), "total": total, "playing": so.playing, "speed": so.speed});
    app.ui.sew_out = Some(so);
    Ok(out)
}

/// The player's progress bar, `frac` of the way along; a click or drag on it returns where to
/// (0 to 1).
fn scrub_bar(ui: &mut egui::Ui, t: &Tokens, frac: f32) -> Option<f32> {
    let (r, resp) = ui.allocate_exact_size(vec2(200.0, 24.0), egui::Sense::click_and_drag());
    let track = egui::Rect::from_center_size(r.center(), vec2(r.width() - 12.0, 4.0));
    let frac = if frac.is_finite() { frac.clamp(0.0, 1.0) } else { 0.0 };
    let x = track.left() + track.width() * frac;
    let p = ui.painter();
    p.rect_filled(track, CornerRadius::same(2), t.divider);
    p.rect_filled(egui::Rect::from_min_max(track.min, pos2(x, track.max.y)), CornerRadius::same(2), t.accent);
    let knob = if resp.hovered() || resp.dragged() { 7.0 } else { 6.0 };
    p.circle(pos2(x, track.center().y), knob, t.accent, Stroke::new(1.5, t.panel));
    let to = resp.interact_pointer_pos().filter(|_| resp.clicked() || resp.dragged())?;
    Some(((to.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0))
}

fn speed_label(speed: f64) -> String {
    crate::i18n::fmt(tl!("{n} stitches/s"), &[("n", &format!("{speed}"))])
}

/// The sew-out player's bar, at the bottom of the canvas `rect`; it also moves the needle on
/// while playing.
pub fn player(app: &mut VectorcraftApp, ui: &mut egui::Ui, rect: egui::Rect) {
    let Some(mut so) = app.ui.sew_out.clone() else { return };
    let Some(pv) = preview(app) else { return };
    let total = pv.stitches.len();
    let end = total as f64;
    if so.playing {
        let dt = f64::from(ui.input(|i| i.stable_dt).clamp(0.0, 0.1));
        so.pos += so.speed * dt;
        if so.pos >= end {
            so.pos = end;
            so.playing = false;
        }
        ui.ctx().request_repaint();
    }
    so.pos = so.pos.clamp(0.0, end);
    let t = Tokens::get(ui.ctx());
    let width = 600.0_f32;
    let x = (rect.center().x - width / 2.0).max(rect.left() + 8.0);
    let y = rect.bottom() - 56.0;
    let mut close = false;
    egui::Area::new(egui::Id::new("sew-out-player")).order(egui::Order::Middle).fixed_pos(pos2(x, y)).show(ui.ctx(), |ui| {
        egui::Frame::NONE
            .fill(t.panel)
            .stroke(Stroke::new(1.0, t.border))
            .corner_radius(CornerRadius::same(5))
            .inner_margin(egui::Margin::symmetric(8, 6))
            .shadow(egui::epaint::Shadow { offset: [0, 3], blur: 10, spread: 0, color: Color32::from_black_alpha(70) })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let n = so.sewn();
                    if icon_button(ui, "skip-back", tl!("Back to the start"), false, 24.0).clicked() {
                        so.pos = 0.0;
                        so.playing = false;
                    }
                    if icon_button(ui, "chevron-left", tl!("Previous stitch"), false, 24.0).clicked() {
                        so.pos = n.saturating_sub(1) as f64;
                        so.playing = false;
                    }
                    let (icon, tip) = if so.playing { ("pause", tl!("Pause")) } else { ("play", tl!("Play")) };
                    if icon_button(ui, icon, tip, false, 24.0).clicked() {
                        if !so.playing && n >= total {
                            so.pos = 0.0;
                        }
                        so.playing = !so.playing;
                    }
                    if icon_button(ui, "chevron-right", tl!("Next stitch"), false, 24.0).clicked() {
                        so.pos = (n + 1).min(total) as f64;
                        so.playing = false;
                    }
                    if icon_button(ui, "skip-forward", tl!("To the end"), false, 24.0).clicked() {
                        so.pos = end;
                        so.playing = false;
                    }
                    // The thread on the needle.
                    let thread = n.checked_sub(1).and_then(|i| pv.stitches.get(i)).or(pv.stitches.first()).map(|s| s.thread);
                    let rgb = thread.and_then(|i| pv.threads.get(i)).copied().unwrap_or([0, 0, 0]);
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, CornerRadius::same(3), Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
                    ui.painter().rect_stroke(r, CornerRadius::same(3), Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
                    if let Some(f) = scrub_bar(ui, &t, if total == 0 { 0.0 } else { (so.pos / end) as f32 }) {
                        so.pos = (f64::from(f) * end).round();
                        so.playing = false;
                    }
                    let count = crate::i18n::fmt(tl!("Stitch {n} of {total}"), &[("n", &so.sewn().to_string()), ("total", &total.to_string())]);
                    ui.label(egui::RichText::new(count).size(12.0).color(t.text));
                    egui::ComboBox::from_id_salt("sew-out-speed").width(110.0).selected_text(speed_label(so.speed)).show_ui(ui, |ui| {
                        for s in SPEEDS {
                            ui.selectable_value(&mut so.speed, s, speed_label(s));
                        }
                    });
                    if icon_button(ui, "x", tl!("Close"), false, 24.0).clicked() {
                        close = true;
                    }
                });
            });
    });
    app.ui.sew_out = if close { None } else { Some(so) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_parse_and_label_the_next_one() {
        assert_eq!(parse_view("Realistic"), Some(2));
        assert_eq!(parse_view("lace"), None);
        assert_eq!(next_label(0), "Stitch View");
        assert_eq!(next_label(2), "Vector View");
    }

    #[test]
    fn shading_stays_in_range() {
        assert_eq!(shade([255, 0, 128], 0.5), Color32::from_rgb(255, 127, 191));
        assert_eq!(shade([200, 100, 0], -1.0), Color32::from_rgb(0, 0, 0));
    }

    fn app_with_art() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 200})).unwrap();
        app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 60, "height": 40})).unwrap();
        app
    }

    #[test]
    fn sew_out_opens_steps_and_closes() {
        let mut app = app_with_art();
        let r = app.run("view.sewOut", json!({})).unwrap();
        let total = r["total"].as_u64().unwrap();
        assert!(total > 100, "{r}");
        assert_eq!((r["open"].clone(), r["pos"].clone(), r["playing"].clone()), (json!(true), json!(0), json!(true)));
        // Opening it from the vector art shows realistic thread.
        assert_eq!(app.ui.view.stitches, 2);
        let r = app.run("view.sewOut", json!({"pos": 40, "play": false, "speed": 5000})).unwrap();
        assert_eq!((r["pos"].clone(), r["playing"].clone(), r["speed"].clone()), (json!(40), json!(false), json!(5000.0)));
        // Out of range positions clamp; bad values are refused and change nothing.
        assert_eq!(app.run("view.sewOut", json!({"pos": 1e12})).unwrap()["pos"], json!(total));
        assert!(app.run("view.sewOut", json!({"pos": "x", "play": true})).is_err());
        assert!(app.ui.sew_out.as_ref().is_some_and(|s| !s.playing));
        // Play at the end starts over.
        assert_eq!(app.run("view.sewOut", json!({"play": true})).unwrap()["pos"], json!(0));
        // Toggling closes it; so does going back to the vector art.
        assert_eq!(app.run("view.sewOut", json!({})).unwrap()["open"], json!(false));
        app.run("view.sewOut", json!({"open": true})).unwrap();
        app.run("view.stitches", json!({"view": "vector"})).unwrap();
        assert!(app.ui.sew_out.is_none());
    }

    #[test]
    fn the_player_plays_to_the_end_and_its_buttons_work() {
        let mut app = app_with_art();
        app.run("view.sewOut", json!({"speed": 1000000})).unwrap();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let rect = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 700.0));
        let frame = |app: &mut VectorcraftApp, events: Vec<egui::Event>| {
            let input = egui::RawInput { screen_rect: Some(rect), events, ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| player(app, ui, rect));
            out.textures_delta.clear();
        };
        frame(&mut app, vec![]);
        frame(&mut app, vec![]);
        let so = app.ui.sew_out.clone().unwrap();
        let total = preview(&mut app).unwrap().stitches.len();
        assert_eq!((so.sewn(), so.playing), (total, false), "a fast player reaches the end and stops");
        // Paint with the player open: only the sewn part, plus the needle.
        app.ui.sew_out = Some(SewOut { pos: 3.0, playing: false, speed: 200.0 });
        let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(rect), ..Default::default() }, |ui| {
            let v = app.view().copied().unwrap_or_default();
            let xf = Xf::new(rect, &v);
            paint(&mut app, ui.painter(), &xf, rect);
        });
        out.textures_delta.clear();
    }
}
