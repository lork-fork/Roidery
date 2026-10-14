//! The document canvas: rendering, rulers, navigation, pointer routing to tools and on-canvas
//! selection visuals (bounding box, anchors, handles, smart-guide style labels).

use egui::{Color32, CornerRadius, Pos2, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;
use vectorcraft_doc::{Node, NodeKind, Unit};
use vectorcraft_geom::{Affine, BezPath, PathEl, Point, Rect};
use vectorcraft_tools::{Cursor, Mods, Overlay, PointerEvent, PointerKind};

use crate::state::View;
use crate::theme::{self, Tokens};
use crate::{CacheKey, VectorcraftApp, now_ms, widgets};

const RULER: f32 = 16.0;

/// Screen ↔ document mapping for one frame.
#[derive(Clone, Copy, Debug)]
pub struct Xf {
    pub rect: egui::Rect,
    pub zoom: f64,
    pub center: Point,
    /// View rotation in radians (Rotate View), clockwise on screen.
    pub rot: f64,
}

impl Xf {
    pub fn new(rect: egui::Rect, v: &View) -> Self {
        Self { rect, zoom: v.zoom, center: v.center, rot: v.rotation.to_radians() }
    }
    pub fn to_screen(&self, p: Point) -> Pos2 {
        let c = self.rect.center();
        let (dx, dy) = ((p.x - self.center.x) * self.zoom, (p.y - self.center.y) * self.zoom);
        let (sn, cs) = self.rot.sin_cos();
        pos2(c.x + (dx * cs - dy * sn) as f32, c.y + (dx * sn + dy * cs) as f32)
    }
    pub fn to_doc(&self, p: Pos2) -> Point {
        let c = self.rect.center();
        let (dx, dy) = ((p.x - c.x) as f64, (p.y - c.y) as f64);
        let (sn, cs) = self.rot.sin_cos();
        let (ux, uy) = (dx * cs + dy * sn, -dx * sn + dy * cs);
        Point::new(self.center.x + ux / self.zoom, self.center.y + uy / self.zoom)
    }
    /// Screen-space corners of a document rect (a rotated quad when the view is rotated).
    pub fn quad(&self, r: Rect) -> Vec<Pos2> {
        [Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)].iter().map(|p| self.to_screen(*p)).collect()
    }
    /// Convert a screen-space delta to a document delta.
    pub fn delta_to_doc(&self, d: egui::Vec2) -> vectorcraft_geom::Vec2 {
        let (sn, cs) = self.rot.sin_cos();
        let (dx, dy) = (d.x as f64, d.y as f64);
        vectorcraft_geom::Vec2::new((dx * cs + dy * sn) / self.zoom, (-dx * sn + dy * cs) / self.zoom)
    }
    pub fn rect_to_screen(&self, r: Rect) -> egui::Rect {
        egui::Rect::from_two_pos(self.to_screen(Point::new(r.x0, r.y0)), self.to_screen(Point::new(r.x1, r.y1)))
    }
    /// Affine mapping document points to screen points.
    pub fn affine(&self) -> Affine {
        let c = self.rect.center();
        Affine::translate((c.x as f64, c.y as f64)) * Affine::rotate(self.rot) * Affine::scale(self.zoom) * Affine::translate(-self.center.to_vec2())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Tool,
    /// Hand tool / Space drag, or a middle-button drag with any tool (`middle`).
    Pan {
        start: Pos2,
        center: Point,
        middle: bool,
    },
    ZoomBox {
        start: Pos2,
    },
    /// Roidery: scrubby zoom (Performance › Animated Zoom): drag right to zoom in, left to zoom
    /// out, about `anchor` (the document point pressed), from `zoom`.
    Scrub {
        start: Pos2,
        zoom: f64,
        anchor: Point,
    },
    RotateView {
        start_angle: f64,
        start_rot: f64,
    },
    /// Cmd held: temporary selection tool; restore this tool on release.
    TempSelect,
    /// A Selection tool move dragged off the canvas: the panels get the art
    /// ([`widgets::PanelDrag::Art`]); `temp` restores the tool Cmd switched from on release.
    Art {
        temp: bool,
    },
}

/// Scrubby zoom: points of horizontal drag that double the zoom.
const SCRUB_DOUBLING: f64 = 150.0;
/// A zoom press that moves less than this many points sideways is a click.
const SCRUB_CLICK: f32 = 4.0;

/// The zoom a scrubby-zoom drag of `dx` points (right: in) gives from `zoom`: twice as close every
/// [`SCRUB_DOUBLING`] points, within the view's zoom range.
fn scrub_zoom(zoom: f64, dx: f64) -> f64 {
    let dx = if dx.is_finite() { dx } else { 0.0 };
    (zoom * (dx / SCRUB_DOUBLING).exp2()).clamp(0.0313, 640.0)
}

fn drag_id() -> egui::Id {
    egui::Id::new("canvas-drag")
}
fn temp_tool_id() -> egui::Id {
    egui::Id::new("canvas-temp-tool")
}

/// Selection, Direct Selection and Group Selection: the tools Cmd switches to for a drag.
pub(crate) fn is_selection_tool(id: &str) -> bool {
    matches!(id, "selection" | "directSelection" | "groupSelection")
}

/// The pen pressure (0..1) of the press in progress: the force of this frame's pen or touch
/// input, else the last one seen since the press (`pressed`: a new press, whose mouse has none
/// until a pen reports it). A mouse presses fully (1).
fn pen_pressure(ui: &Ui, pressed: bool) -> f32 {
    let id = egui::Id::new("canvas-pressure");
    let force = ui.input(|i| {
        i.events.iter().rev().find_map(|e| match e {
            egui::Event::Touch { force: Some(f), .. } if f.is_finite() => Some(f.clamp(0.0, 1.0)),
            _ => None,
        })
    });
    let p = force.or_else(|| if pressed { None } else { ui.data(|d| d.get_temp::<f32>(id)) }).unwrap_or(1.0);
    ui.data_mut(|d| d.insert_temp(id, p));
    p
}

pub fn mods(m: egui::Modifiers, space: bool) -> Mods {
    Mods { shift: m.shift, alt: m.alt, cmd: m.command, ctrl: m.ctrl, space }
}

/// Fit the view (View → Fit Artboard / Fit All / Actual Size).
pub fn fit(app: &mut VectorcraftApp, how: &str) {
    let Some(rect) = app.canvas_rect else {
        if let Some(v) = app.view_mut() {
            v.fitted = false;
        }
        return;
    };
    let current = app.view().map_or(0, |v| v.artboard);
    let Some(st) = app.session.active() else { return };
    let target = match how {
        "view.fitAll" => {
            st.doc.art_bounds().map(|a| st.doc.artboards.iter().fold(a, |r, ab| r.union(ab.rect))).or(st.doc.artboards.first().map(|a| a.rect))
        }
        // The navigator's artboard (the first if it has gone).
        _ => st.doc.artboards.get(current).or(st.doc.artboards.first()).map(|a| a.rect),
    };
    let Some(target) = target else { return };
    let Some(v) = app.view_mut() else { return };
    v.center = target.center();
    v.fitted = true;
    if how == "view.actualSize" {
        v.zoom = 1.0;
    } else {
        let zx = (rect.width() as f64 - 60.0) / target.width().max(1.0);
        let zy = (rect.height() as f64 - 60.0) / target.height().max(1.0);
        v.zoom = zx.min(zy).clamp(0.0313, 640.0);
    }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let full = ui.available_rect_before_wrap();
    // A document that opened, closed or became active since Home was chosen replaces it.
    if app.ui.home.is_some_and(|k| k != crate::menus::home_key(app)) {
        app.ui.home = None;
    }
    if app.session.active().is_none() || app.ui.home.is_some() {
        home(app, ui, full);
        return;
    }
    let rect = if app.ui.view.rulers && app.ui.screen_mode < 3 { egui::Rect::from_min_max(full.min + vec2(RULER, RULER), full.max) } else { full };
    app.canvas_rect = Some(rect);
    let fitted = app.view().is_some_and(|v| v.fitted);
    if !fitted {
        fit(app, "view.fitArtboard");
    }
    let resp = ui.interact(rect, egui::Id::new("canvas"), Sense::click_and_drag());
    // A press on the canvas while the context menu is open only closes it (a drag too, which egui
    // alone would leave open); the tool doesn't get it.
    if resp.context_menu_opened() && resp.hovered() && ui.input(|i| i.pointer.any_pressed()) {
        egui::Popup::close_all(ui.ctx());
    } else {
        handle_input(app, ui, &resp, rect);
    }
    let v = *app.view().unwrap_or(&View::default());
    let xf = Xf::new(rect, &v);
    panel_drop(app, ui, &resp, &xf);
    context_menu(app, &resp, &xf);
    let painter = ui.painter_at(rect);
    let Some(st) = app.session.active() else { return };
    let doc = st.doc.clone();
    let mask_view = st.shown_mask();

    // Pasteboard, artboard shadows and paper (Document Setup: the transparency grid's look, the
    // simulated paper colour; a white Background Contents hides the grid).
    painter.rect_filled(rect, 0.0, t.pasteboard);
    if app.ui.view.artboards && !app.ui.view.outline {
        let setup = &doc.setup;
        let grid = (st.transparency_grid && setup.background == vectorcraft_doc::Background::Transparent).then(|| checker_texture(ui.ctx(), setup));
        let paper = if setup.simulate_paper { crate::panels::c32(&setup.paper()) } else { Color32::WHITE };
        for ab in &doc.artboards {
            let q = xf.quad(ab.rect);
            // Hard 2 pt drop shadow, right and bottom (measured: #4d4d4d then #565656 on #606060).
            let shift = |d: f32| q.iter().map(|p| *p + vec2(d, d)).collect::<Vec<_>>();
            painter.add(Shape::convex_polygon(shift(2.0), Color32::from_black_alpha(26), Stroke::NONE));
            painter.add(Shape::convex_polygon(shift(1.0), Color32::from_black_alpha(52), Stroke::NONE));
            match &grid {
                Some((tex, cell)) => checker(&painter, &q, ab.rect, xf.zoom, tex.id(), *cell),
                None => {
                    painter.add(Shape::convex_polygon(q, paper, Stroke::NONE));
                }
            }
        }
    } else if app.ui.view.outline {
        for ab in &doc.artboards {
            painter.add(Shape::convex_polygon(xf.quad(ab.rect), Color32::WHITE, Stroke::NONE));
        }
    }
    // The grid behind the art (Guides & Grid › Grids In Back), else over it below.
    let grid_look = app.ui.view.grid.then(|| LineLook::grid(&app.session.prefs));
    let grids_in_back = app.session.prefs.grids_in_back;
    if let Some(look) = grid_look.filter(|_| grids_in_back) {
        grid(&painter, &xf, doc.grid.spacing, doc.grid.subdivisions, look);
    }

    // Artwork raster.
    let ppp = ui.ctx().pixels_per_point();
    let (w, h) = ((rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32);
    let key = CacheKey {
        doc: st.uid as usize,
        revision: st.revision,
        zoom: v.zoom,
        cx: v.center.x,
        cy: v.center.y,
        w,
        h,
        outline: app.ui.view.outline,
        trim: app.ui.view.trim_view,
        ppp,
        hidden: vec![],
        rot: v.rotation,
    };
    if !app.canvas.worker_started {
        app.canvas.worker_started = true;
        if std::env::var_os("VECTORCRAFT_SYNC_RENDER").is_none() {
            app.canvas.worker = crate::render_worker::Worker::spawn(ui.ctx().clone());
        }
    }
    // Upload finished background renders.
    if let Some(done) = app.canvas.worker.as_mut().and_then(|w| w.poll())
        && done.key.doc == key.doc
    {
        upload(app, ui.ctx(), &done.img);
        app.canvas.key = Some(done.key);
        app.perf.render_ms = done.ms;
        app.canvas.last_ms = done.ms;
    }
    if app.canvas.key.as_ref() != Some(&key) || app.canvas.texture.is_none() {
        let view = Affine::translate((w as f64 / 2.0, h as f64 / 2.0))
            * Affine::rotate(v.rotation.to_radians())
            * Affine::scale(v.zoom * ppp as f64)
            * Affine::translate(-v.center.to_vec2());
        let opts = vectorcraft_render::RenderOptions { outline: app.ui.view.outline, background: None, artboards: false, ..Default::default() };
        let opts = vectorcraft_render::RenderOptions {
            proof: vectorcraft_render::proof::active_proof(),
            overprint_preview: vectorcraft_render::proof::overprint_preview_on(),
            trim: app.ui.view.trim_view,
            tile_edge: vectorcraft_color::Color::from_hex(&app.session.prefs.pattern_tile_edge_color).map_or(opts.tile_edge, |c| {
                let [r, g, b, _] = c.to_rgba8(1.0);
                [r, g, b]
            }),
            mask_view,
            highlight_substitutions: true,
            ..opts
        };
        // Light documents render synchronously (no lag vs overlays); heavy ones go to the worker.
        // A document switch renders synchronously so another document's frame is never shown.
        let same_doc = app.canvas.key.as_ref().is_some_and(|k| k.doc == key.doc);
        let heavy = app.canvas.last_ms > 8.0 && app.canvas.texture.is_some() && same_doc;
        match (&mut app.canvas.worker, heavy) {
            (Some(worker), true) => worker.submit(crate::render_worker::Job { key: key.clone(), doc: doc.clone(), w, h, view, opts }),
            _ => {
                let t0 = now_ms();
                let img = app.canvas.renderer.render(&doc, w, h, view, &opts);
                upload(app, ui.ctx(), &img);
                app.canvas.key = Some(key.clone());
                app.perf.render_ms = now_ms() - t0;
                app.canvas.last_ms = app.perf.render_ms;
            }
        }
    }
    if let (Some(tex), Some(k)) = (&app.canvas.texture, &app.canvas.key)
        && k.doc == key.doc
        && (k.rot - v.rotation).abs() < 1e-9
    {
        // Reproject the last frame if it was rendered for a different view.
        let old = Xf { rect, zoom: k.zoom, center: Point::new(k.cx, k.cy), rot: xf.rot };
        let _ = k.rot;
        let a = xf.to_screen(old.to_doc(rect.min));
        let b = xf.to_screen(old.to_doc(rect.max));
        painter.image(tex.id(), egui::Rect::from_min_max(a, b), egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    // Roidery: the stitch and realistic views cover the art with fabric and thread.
    crate::stitchview::paint(app, &painter, &xf, rect);
    if let Some(look) = grid_look.filter(|_| !grids_in_back) {
        grid(&painter, &xf, doc.grid.spacing, doc.grid.subdivisions, look);
    }
    // Artboard edges and names.
    let active_ab = 0;
    for (i, ab) in doc.artboards.iter().enumerate() {
        let r = xf.rect_to_screen(ab.rect);
        let c = if i == active_ab { Color32::from_gray(0) } else { Color32::from_gray(120) };
        painter.add(Shape::closed_line(xf.quad(ab.rect), Stroke::new(if i == active_ab { 1.0 } else { 0.6 }, c)));
        // The bleed (Document Setup) as a red outline around the artboard.
        if doc.setup.has_bleed() {
            painter.add(Shape::closed_line(xf.quad(doc.setup.bleed_rect(ab.rect)), Stroke::new(1.0, t.bleed)));
        }
        if app.session.tool_id() == "artboard" || doc.artboards.len() > 1 {
            painter.text(
                r.left_top() - vec2(0.0, 4.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{:02} - {}", i + 1, ab.name),
                egui::FontId::proportional(11.0),
                t.text_dim,
            );
        }
    }
    if app.ui.view.guides {
        let look = LineLook::guides(&app.session.prefs, &t);
        // Selected guides in the selection colour.
        let picked = LineLook { color: t.selection, ..look };
        let selected: &[usize] = app.session.active().map_or(&[], |st| &st.selection.guides);
        for (i, g) in doc.guides.iter().enumerate() {
            let (a, b) = if g.vertical {
                let x = xf.to_screen(Point::new(g.pos, 0.0)).x;
                (pos2(x, rect.top()), pos2(x, rect.bottom()))
            } else {
                let y = xf.to_screen(Point::new(0.0, g.pos)).y;
                (pos2(rect.left(), y), pos2(rect.right(), y))
            };
            if selected.contains(&i) { picked } else { look }.guide(&painter, a, b);
        }
    }

    if !app.session.slices_hidden() {
        slice_overlay(app, &painter, &xf);
    }
    if app.session.active().is_some_and(|d| d.print_tiling) || app.session.tool_id() == "printTiling" {
        print_tiling_overlay(app, &painter, &xf);
    }

    // Selection visuals and tool overlays.
    if app.ui.view.edges {
        hover_highlight(app, &painter, &xf);
        selection_overlay(app, &painter, &xf);
        if app.ui.view.text_threads {
            thread_overlay(app, &painter, &xf);
        }
        if app.ui.view.hidden_chars {
            hidden_chars_overlay(app, &painter, &xf);
        }
    }
    let view_info = app.view_info();
    // View → Hide Gradient Annotator hides the Gradient tool's annotator.
    if app.session.tool_id() != "gradient" || app.ui.view.gradient_annotator {
        let overlays = app.session.overlays(view_info);
        draw_overlays(&painter, &xf, &overlays, &t, HandleLook::of(&app.session.prefs));
    }
    ime_output(app, ui.ctx(), &xf);
    crate::place::paint_drop_highlight(app, ui.ctx(), &painter, rect);

    if app.ui.view.rulers && app.ui.screen_mode < 3 {
        rulers(ui, full, &xf, app.hover_doc, app.session.general_unit(), &t);
        ruler_guides(app, ui, full, rect, &xf, &t);
    }
    // Roidery: the Embroidery workspace has no floating task bar.
    if app.ui.task_bar && !crate::embroidery_workspace::hides_task_bar(app) && !app.session.tool_busy() && app.ui.screen_mode < 3 {
        task_bar(app, ui, &xf);
    }
    // Roidery: View → Sew-Out Player.
    crate::stitchview::player(app, ui, rect);
    // Cursor.
    if resp.hovered() {
        let m = ui.input(|i| i.modifiers);
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        let panning = matches!(ui.data(|d| d.get_temp::<Drag>(drag_id())), Some(Drag::Pan { .. }));
        let zooming = if space { m.command } else { app.session.tool_id() == "zoom" };
        let cur = if panning {
            egui::CursorIcon::Grabbing
        } else if zooming {
            if m.alt { egui::CursorIcon::ZoomOut } else { egui::CursorIcon::ZoomIn }
        } else if space || app.session.tool_id() == "hand" {
            egui::CursorIcon::Grab
        } else if let Some(p) = app.hover_doc {
            let c = app.session.cursor(p, mods(m, space), view_info);
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("tool-cursor")));
            // The loaded place cursor carries the file's thumbnail.
            if app.session.tool_id() == "place"
                && let Some(hp) = ui.input(|i| i.pointer.hover_pos())
            {
                crate::place::paint_cursor(app, ui.ctx(), &painter, hp);
            }
            // Caps Lock gives precise (crosshair) cursors, like Illustrator; env opt-out for system cursors.
            let custom = std::env::var_os("VECTORCRAFT_SYSTEM_CURSORS").is_none();
            match ui.input(|i| i.pointer.hover_pos()) {
                Some(hp) if custom && crate::cursors::paint(&painter, c, hp) => egui::CursorIcon::None,
                _ => cursor_icon(c),
            }
        } else {
            egui::CursorIcon::Default
        };
        ui.ctx().set_cursor_icon(cur);
    }
}

fn cursor_icon(c: Cursor) -> egui::CursorIcon {
    use egui::CursorIcon as C;
    match c {
        Cursor::Arrow | Cursor::ArrowHollow | Cursor::CornerRadius | Cursor::PathBracket => C::Default,
        Cursor::Move => C::Move,
        Cursor::Crosshair => C::Crosshair,
        Cursor::ResizeH => C::ResizeHorizontal,
        Cursor::ResizeV => C::ResizeVertical,
        Cursor::ResizeNwSe => C::ResizeNwSe,
        Cursor::ResizeNeSw => C::ResizeNeSw,
        Cursor::Rotate => C::Alias,
        Cursor::Pen | Cursor::PenAdd | Cursor::PenDelete | Cursor::PenClose | Cursor::PenContinue | Cursor::PenConvert => C::Crosshair,
        Cursor::Text => C::Text,
        Cursor::Hand => C::Grab,
        Cursor::HandGrab => C::Grabbing,
        Cursor::ZoomIn => C::ZoomIn,
        Cursor::ZoomOut => C::ZoomOut,
        Cursor::Eyedropper => C::Crosshair,
        Cursor::NotAllowed => C::NotAllowed,
        Cursor::AddStop => C::Copy,
        Cursor::RemoveStop => C::NotAllowed,
        Cursor::Slice => C::Crosshair,
        Cursor::SliceSelect => C::Default,
        Cursor::Width | Cursor::WidthAdd => C::Crosshair,
        Cursor::WidthPoint => C::Move,
        Cursor::Blend | Cursor::BlendObject | Cursor::BlendAnchor | Cursor::ShapeBuilder | Cursor::ShapeBuilderErase => C::Crosshair,
    }
}

fn handle_input(app: &mut VectorcraftApp, ui: &Ui, resp: &egui::Response, rect: egui::Rect) {
    let line = ui.ctx().options(|o| o.input_options.line_scroll_speed);
    let wheel_zooms = app.session.prefs.zoom_with_mouse_wheel;
    let (pointer, m, space, (factor, scroll)) =
        ui.input(|i| (i.pointer.clone(), i.modifiers, i.key_down(egui::Key::Space), wheel(i, wheel_zooms, line, rect.height())));
    let v = *app.view().unwrap_or(&View::default());
    let xf = Xf::new(rect, &v);
    let hover = pointer.hover_pos().filter(|p| rect.contains(*p));
    app.hover_doc = hover.map(|p| xf.to_doc(p));
    let view = app.view_info();
    let drag: Option<Drag> = ui.data(|d| d.get_temp(drag_id()));
    // A modifier pressed or released over the canvas re-hovers the tool, so what it changes shows
    // without moving the mouse (Alt switches the Shape Builder to erase mode).
    let mods_changed = ui.data_mut(|d| {
        let id = egui::Id::new("canvas-mods");
        let prev = d.get_temp::<egui::Modifiers>(id);
        d.insert_temp(id, m);
        prev.is_some_and(|prev| prev != m)
    });

    // Zoom around the pointer, or scroll ([`wheel`]).
    if resp.hovered() {
        if (factor - 1.0).abs() > 1e-6 {
            if let (Some(p), Some(vm)) = (hover, app.view_mut()) {
                let before = xf.to_doc(p);
                vm.zoom = (vm.zoom * factor).clamp(0.0313, 640.0);
                let nx = Xf { rect, zoom: vm.zoom, center: vm.center, rot: vm.rotation.to_radians() };
                let after = nx.to_doc(p);
                vm.center += before - after;
            }
        } else if scroll != egui::Vec2::ZERO
            && let Some(vm) = app.view_mut()
        {
            let d = Xf { rect, zoom: vm.zoom, center: vm.center, rot: vm.rotation.to_radians() }.delta_to_doc(scroll);
            vm.center -= d;
        }
    }

    let tool = app.session.tool_id();
    // Held, Space is the Hand tool for the moment, and Cmd+Space the Zoom tool (with Alt, zooming
    // out), whatever the tool.
    let zoom_mode = if space { m.command } else { tool == "zoom" };
    let pan_mode = if space { !m.command } else { tool == "hand" };
    let middle_pan = matches!(drag, Some(Drag::Pan { middle: true, .. }));
    // egui counts a press a few pixels outside the canvas as on it (its interaction radius): only a
    // press on the canvas itself reaches the tools, else it would land at the canvas's centre.
    if pointer.primary_pressed()
        && resp.hovered()
        && !middle_pan
        && let Some(p) = hover
    {
        ui.ctx().memory_mut(|mem| mem.stop_text_input());
        app.ui.flyout = None;
        let d = if zoom_mode && app.session.prefs.animated_zoom {
            Drag::Scrub { start: p, zoom: v.zoom, anchor: xf.to_doc(p) }
        } else if zoom_mode {
            Drag::ZoomBox { start: p }
        } else if pan_mode {
            Drag::Pan { start: p, center: v.center, middle: false }
        } else if tool == "rotateView" {
            let c = rect.center();
            Drag::RotateView { start_angle: (p.y - c.y).atan2(p.x - c.x) as f64, start_rot: v.rotation }
        } else {
            let mut kind = Drag::Tool;
            // Cmd with another tool: drag with the selection tool used last.
            if m.command && !is_selection_tool(tool) {
                ui.data_mut(|d| d.insert_temp(temp_tool_id(), tool.to_string()));
                let last = app.ui.last_selection_tool.clone();
                app.select_tool(if is_selection_tool(&last) { &last } else { "selection" });
                kind = Drag::TempSelect;
            }
            // Cmd only brought the selection tool: its click is a plain one, not Select Behind.
            let mods = Mods { cmd: m.command && kind != Drag::TempSelect, ..mods(m, space) };
            let ev = PointerEvent { kind: PointerKind::Down, pos: xf.to_doc(p), mods, pressure: pen_pressure(ui, true) };
            dispatch(app, &ev, view);
            kind
        };
        ui.data_mut(|dd| dd.insert_temp(drag_id(), d));
    } else if drag.is_none()
        && resp.hovered()
        && pointer.button_pressed(egui::PointerButton::Middle)
        && let Some(start) = hover
    {
        // Middle-button drag pans the view whatever the tool.
        ui.data_mut(|dd| dd.insert_temp(drag_id(), Drag::Pan { start, center: v.center, middle: true }));
    } else if let Some(d) = drag {
        let p = pointer.interact_pos().unwrap_or(rect.center());
        let held = if middle_pan { pointer.button_down(egui::PointerButton::Middle) } else { pointer.primary_down() };
        if held {
            match d {
                Drag::Pan { start, center, .. } => {
                    let d = xf.delta_to_doc(p - start);
                    if let Some(vm) = app.view_mut() {
                        vm.center = center - d;
                    }
                }
                Drag::RotateView { start_angle, start_rot } => {
                    let c = rect.center();
                    let a = (p.y - c.y).atan2(p.x - c.x) as f64;
                    let mut deg = start_rot + (a - start_angle).to_degrees();
                    if m.shift {
                        deg = (deg / 15.0).round() * 15.0;
                    }
                    if let Some(vm) = app.view_mut() {
                        vm.rotation = vectorcraft_geom::normalize_deg(deg);
                    }
                }
                Drag::Scrub { start, zoom, anchor } => {
                    if let Some(vm) = app.view_mut() {
                        vm.zoom = scrub_zoom(zoom, f64::from(p.x - start.x));
                        // The point pressed stays under the pointer's starting place.
                        let nx = Xf { rect, zoom: vm.zoom, center: vm.center, rot: vm.rotation.to_radians() };
                        vm.center += anchor - nx.to_doc(start);
                    }
                }
                Drag::ZoomBox { .. } | Drag::Art { .. } => {}
                Drag::Tool | Drag::TempSelect if drag_art_out(app, ui, resp, p, view) => {
                    ui.data_mut(|dd| dd.insert_temp(drag_id(), Drag::Art { temp: d == Drag::TempSelect }));
                }
                Drag::Tool | Drag::TempSelect => {
                    if pointer.delta() != egui::Vec2::ZERO {
                        let ev = PointerEvent { kind: PointerKind::Drag, pos: xf.to_doc(p), mods: mods(m, space), pressure: pen_pressure(ui, false) };
                        dispatch(app, &ev, view);
                    }
                    // Time held (Twirl, Pucker and Bloat keep applying); a stalled frame counts
                    // a quarter second at most.
                    if app.session.tool_wants_ticks() {
                        let dt = f64::from(ui.input(|i| i.unstable_dt).min(0.25));
                        let r = app.session.tool_tick(dt, view);
                        apply_requests(app, r);
                    }
                }
            }
        } else {
            ui.data_mut(|dd| dd.remove::<Drag>(drag_id()));
            match d {
                // A click without a drag zooms one step about the point clicked.
                Drag::Scrub { start, zoom, anchor } => {
                    if (p.x - start.x).abs() < SCRUB_CLICK
                        && let Some(vm) = app.view_mut()
                    {
                        vm.zoom = crate::state::next_zoom(zoom, !m.alt);
                        let nx = Xf { rect, zoom: vm.zoom, center: vm.center, rot: vm.rotation.to_radians() };
                        vm.center += anchor - nx.to_doc(start);
                    }
                }
                Drag::ZoomBox { start } => {
                    let r = egui::Rect::from_two_pos(start, p);
                    if let Some(vm) = app.view_mut() {
                        if r.width() > 8.0 && r.height() > 8.0 {
                            let a = xf.to_doc(r.min);
                            let b = xf.to_doc(r.max);
                            vm.center = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                            vm.zoom = (rect.width() as f64 / (b.x - a.x)).min(rect.height() as f64 / (b.y - a.y)).clamp(0.0313, 640.0);
                        } else {
                            let before = xf.to_doc(p);
                            vm.zoom = crate::state::next_zoom(vm.zoom, !m.alt);
                            let nx = Xf { rect, zoom: vm.zoom, center: vm.center, rot: vm.rotation.to_radians() };
                            vm.center += before - nx.to_doc(p);
                        }
                    }
                }
                Drag::Tool | Drag::TempSelect | Drag::Art { .. } => {
                    if !matches!(d, Drag::Art { .. }) {
                        let ev = PointerEvent { kind: PointerKind::Up, pos: xf.to_doc(p), mods: mods(m, space), pressure: pen_pressure(ui, false) };
                        dispatch(app, &ev, view);
                    }
                    if matches!(d, Drag::TempSelect | Drag::Art { temp: true })
                        && let Some(prev) = ui.data(|dd| dd.get_temp::<String>(temp_tool_id()))
                    {
                        app.restore_tool(&prev);
                    }
                }
                Drag::Pan { .. } | Drag::RotateView { .. } => {}
            }
        }
    } else if let Some(p) = hover
        && (pointer.is_moving() || mods_changed)
    {
        let ev = PointerEvent { kind: PointerKind::Move, pos: xf.to_doc(p), mods: mods(m, space), pressure: 1.0 };
        dispatch(app, &ev, view);
    }
    if resp.double_clicked()
        && let Some(p) = hover
    {
        let ev = PointerEvent { kind: PointerKind::DoubleClick, pos: xf.to_doc(p), mods: mods(m, space), pressure: 1.0 };
        let before = app.session.tool_id().to_string();
        dispatch(app, &ev, view);
        // A double-click on type switched a selection tool to the Type tool: the caret goes where
        // the type was clicked, so typing edits it at once.
        if before != "type" && app.session.tool_id() == "type" {
            for kind in [PointerKind::Down, PointerKind::Up] {
                dispatch(app, &PointerEvent { kind, ..ev }, view);
            }
        }
    }
    if drag.is_some() || pointer.is_moving() || mods_changed {
        ui.ctx().request_repaint();
    }
}

/// How much one point of wheel motion zooms (as `exp(points × WHEEL_ZOOM)`).
const WHEEL_ZOOM: f64 = 0.01;

/// What the wheel and a pinch did over the canvas this frame: a zoom factor (about the pointer)
/// and a scroll (screen points the content moves). The wheel scrolls, Cmd- and Alt-wheel zoom.
/// With General › Zoom with Mouse Wheel (`wheel_zooms`) the wheel zooms, Shift-wheel scrolls up
/// and down and Cmd/Ctrl-wheel sideways. `line` and `page`: points per wheel line and page.
fn wheel(i: &egui::InputState, wheel_zooms: bool, line: f32, page: f32) -> (f64, egui::Vec2) {
    if !wheel_zooms {
        // egui makes Cmd-wheel (and a pinch) its zoom and the rest a smoothed scroll.
        let (zoom, scroll) = (f64::from(i.zoom_delta()), i.smooth_scroll_delta);
        return if i.modifiers.alt && scroll.y != 0.0 { (zoom * (f64::from(scroll.y) * WHEEL_ZOOM).exp(), egui::Vec2::ZERO) } else { (zoom, scroll) };
    }
    // The wheel events themselves: egui's own handling turns Cmd-wheel into a zoom.
    let mut zoom = i.multi_touch().map_or(1.0, |t| f64::from(t.zoom_delta));
    let mut scroll = egui::Vec2::ZERO;
    for e in &i.events {
        match e {
            egui::Event::MouseWheel { unit, delta, modifiers, .. } => {
                let d = match unit {
                    egui::MouseWheelUnit::Point => *delta,
                    egui::MouseWheelUnit::Line => *delta * line,
                    egui::MouseWheelUnit::Page => *delta * page,
                };
                if modifiers.command {
                    scroll.x += d.x + d.y;
                } else if modifiers.shift {
                    scroll.y += d.x + d.y;
                } else {
                    zoom *= (f64::from(d.y) * WHEEL_ZOOM).exp();
                    scroll.x += d.x;
                }
            }
            egui::Event::Zoom(f) => zoom *= f64::from(*f),
            _ => {}
        }
    }
    (zoom, scroll)
}

/// A Selection tool move dragged off the canvas (to `p`, over a panel) turns into a panel drag of
/// the selected art ([`widgets::PanelDrag::Art`]): the move is dropped, so the art stays where it
/// was, and the panel it is released on takes it (the Graphic Styles panel makes a style of it).
fn drag_art_out(app: &mut VectorcraftApp, ui: &Ui, resp: &egui::Response, p: Pos2, view: vectorcraft_engine::ViewInfo) -> bool {
    let off = !ui.clip_rect().contains(p) || ui.ctx().layer_id_at(p).is_some_and(|l| l != resp.layer_id);
    let Some(st) = app.session.active().filter(|_| off && app.session.tool_id() == "selection") else { return false };
    // The Selection tool's interaction while it moves (or Alt-copies) the selection.
    if !st.interaction.as_ref().is_some_and(|i| i.label == "Move" || i.label == "Copy") {
        return false;
    }
    let ids = st.selection.objects.clone();
    if app.session.cancel_interaction().is_err() {
        return false;
    }
    // With nothing left to commit, the tool's mouse-up just returns it to rest.
    dispatch(app, &PointerEvent { kind: PointerKind::Up, pos: Point::ZERO, mods: Mods::default(), pressure: 1.0 }, view);
    egui::DragAndDrop::set_payload(ui.ctx(), widgets::PanelDrag::Art(ids));
    true
}

/// Send a pointer event to the active tool and act on UI requests (dialogs, tool switches).
pub fn dispatch(app: &mut VectorcraftApp, ev: &PointerEvent, view: vectorcraft_engine::ViewInfo) {
    let r = app.session.pointer(ev, view);
    // The artboard the Artboard tool makes active is the active one (the navigator's, the
    // Artboards panel's).
    if app.session.tool_id() == "artboard"
        && let Some(i) = app.session.tool_options()["active"].as_u64().and_then(|i| usize::try_from(i).ok())
        && let Some(v) = app.view_mut()
    {
        v.artboard = i;
    }
    apply_requests(app, r);
}

/// Act on what a tool event asked the UI for (dialogs, tool switches), or show its error.
pub fn apply_requests(app: &mut VectorcraftApp, r: vectorcraft_engine::Result<Vec<vectorcraft_engine::UiRequest>>) {
    match r {
        Ok(reqs) => {
            for r in reqs {
                match r {
                    vectorcraft_engine::UiRequest::Dialog(kind, p) => crate::dialogs::open_tool_dialog(app, &kind, p),
                    vectorcraft_engine::UiRequest::SwitchTool(t) => app.select_tool(&t),
                    vectorcraft_engine::UiRequest::Status(msg) => app.status(msg),
                }
            }
        }
        Err(e) => app.status(e.to_string()),
    }
}

/// The transparency grid's 2 × 2-cell tile in the document's colours (Document Setup), cached per
/// look, and its cell size in screen points.
pub(crate) fn checker_texture(ctx: &egui::Context, setup: &vectorcraft_doc::DocSetup) -> (egui::TextureHandle, f32) {
    let colors = setup.grid_colors.map(|c| crate::panels::c32(&c));
    let id = egui::Id::new("transparency-grid-tile");
    let cached = ctx.data(|d| d.get_temp::<([Color32; 2], egui::TextureHandle)>(id)).filter(|(c, _)| *c == colors);
    let tex = cached.map(|(_, t)| t).unwrap_or_else(|| {
        let [a, b] = colors;
        let img = egui::ColorImage::new([2, 2], vec![a, b, b, a]);
        let opts = egui::TextureOptions { wrap_mode: egui::TextureWrapMode::Repeat, ..egui::TextureOptions::NEAREST };
        let t = ctx.load_texture(TRANSPARENCY_GRID, img, opts);
        ctx.data_mut(|d| d.insert_temp(id, (colors, t.clone())));
        t
    });
    (tex, setup.grid_size.cell())
}

/// Name of the transparency grid's texture.
pub(crate) const TRANSPARENCY_GRID: &str = "transparency grid";

/// The transparency grid over the artboard `ab` (screen corners `quad`): one textured quad whose
/// cells stay `cell` screen points at every zoom and turn with a rotated view.
fn checker(p: &egui::Painter, quad: &[Pos2], ab: Rect, zoom: f64, tex: egui::TextureId, cell: f32) {
    let mut mesh = egui::Mesh::with_texture(tex);
    // Two cells per texture repeat.
    let per = (2.0 * cell as f64 / zoom).max(1e-9);
    let (u, v) = ((ab.width() / per) as f32, (ab.height() / per) as f32);
    for (pos, uv) in quad.iter().zip([pos2(0.0, 0.0), pos2(u, 0.0), pos2(u, v), pos2(0.0, v)]) {
        mesh.vertices.push(egui::epaint::Vertex { pos: *pos, uv, color: Color32::WHITE });
    }
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    p.add(Shape::mesh(mesh));
}

/// The look of the grid or of the guides (Preferences › Guides & Grid › Color and Style).
#[derive(Clone, Copy)]
struct LineLook {
    color: Color32,
    dots: bool,
}

impl LineLook {
    fn grid(prefs: &vectorcraft_engine::Prefs) -> Self {
        Self { color: pref_color(&prefs.grid_color, Color32::from_gray(200)), dots: prefs.grid_style == "dots" }
    }

    fn guides(prefs: &vectorcraft_engine::Prefs, t: &Tokens) -> Self {
        Self { color: pref_color(&prefs.guide_color, t.guide), dots: prefs.guide_style == "dots" }
    }

    /// A guide from `a` to `b`: a line, or dots.
    fn guide(self, p: &egui::Painter, a: Pos2, b: Pos2) {
        if self.dots {
            p.extend(Shape::dotted_line(&[a, b], self.color, 4.0, 0.75));
        } else {
            p.line_segment([a, b], Stroke::new(1.0, self.color));
        }
    }
}

/// A `#rrggbb` colour preference, `fallback` when it doesn't parse.
fn pref_color(hex: &str, fallback: Color32) -> Color32 {
    vectorcraft_color::Color::from_hex(hex).map_or(fallback, |c| crate::panels::c32(&c))
}

/// The grid (View → Show Grid): a gridline every `spacing` and `subdiv` lighter lines between
/// (left out when they would crowd). In the Dots style, a dot where gridlines cross the
/// subdivisions instead.
fn grid(p: &egui::Painter, xf: &Xf, spacing: f64, subdiv: u32, look: LineLook) {
    let r = xf.rect;
    let a = xf.to_doc(r.min);
    let b = xf.to_doc(r.max);
    let sub = spacing / subdiv.max(1) as f64;
    let step = if sub * xf.zoom >= 6.0 { sub } else { spacing };
    // Dots crowd sooner than lines.
    if step * xf.zoom < 4.0 || (look.dots && spacing * xf.zoom < 8.0) {
        return;
    }
    let major = |v: f64| (v / spacing - (v / spacing).round()).abs() < 1e-6;
    // The lines across the view, on screen, each with whether it is a gridline.
    let lines = |from: f64, to: f64, screen: &dyn Fn(f64) -> f32| {
        let mut v = (from / step).floor() * step;
        let mut out = vec![];
        while v <= to {
            out.push((screen(v), major(v)));
            v += step;
        }
        out
    };
    let xs = lines(a.x, b.x, &|x| xf.to_screen(Point::new(x, 0.0)).x);
    let ys = lines(a.y, b.y, &|y| xf.to_screen(Point::new(0.0, y)).y);
    if look.dots {
        let mut mesh = egui::Mesh::default();
        for &(x, mx) in &xs {
            for &(y, my) in &ys {
                if mx || my {
                    mesh.add_colored_rect(egui::Rect::from_center_size(pos2(x, y), vec2(1.5, 1.5)), look.color);
                }
            }
        }
        p.add(Shape::mesh(mesh));
        return;
    }
    let minor = look.color.gamma_multiply(0.4);
    let stroke = |major: bool| Stroke::new(1.0, if major { look.color } else { minor });
    for (x, m) in xs {
        p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], stroke(m));
    }
    for (y, m) in ys {
        p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], stroke(m));
    }
}

/// A ruler label for `v` (in the ruler's unit) on a ruler whose labels are `step` apart: whole
/// numbers, or as many decimals as the step has.
pub(crate) fn ruler_label(v: f64, step: f64) -> String {
    let decimals = if step >= 1.0 { 0 } else { (-step.log10()).ceil() as usize };
    let s = format!("{v:.decimals$}");
    if s.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') { "0".into() } else { s }
}

/// The rulers, numbered in `unit` (the General unit).
/// The top ruler, the left ruler and the box where they meet.
fn ruler_rects(full: egui::Rect) -> [egui::Rect; 3] {
    let top = egui::Rect::from_min_max(pos2(full.left() + RULER, full.top()), pos2(full.right(), full.top() + RULER));
    let left = egui::Rect::from_min_max(pos2(full.left(), full.top() + RULER), pos2(full.left() + RULER, full.bottom()));
    [top, left, egui::Rect::from_min_size(full.min, vec2(RULER, RULER))]
}

/// A drag from a ruler onto the canvas makes a guide where the button is released: a horizontal
/// one from the top ruler, a vertical one from the left ruler; with Shift on the nearest ruler
/// tick. Released anywhere else, it makes none.
fn ruler_guides(app: &mut VectorcraftApp, ui: &Ui, full: egui::Rect, canvas: egui::Rect, xf: &Xf, t: &Tokens) {
    let [top, left, _] = ruler_rects(full);
    let shift = ui.input(|i| i.modifiers.shift);
    for (r, vertical, id) in [(top, false, "ruler-top"), (left, true, "ruler-left")] {
        let resp = ui.interact(r, egui::Id::new(id), Sense::drag());
        let Some(p) = resp.interact_pointer_pos().filter(|p| canvas.contains(*p)) else { continue };
        let d = xf.to_doc(p);
        let mut pos = if vertical { d.x } else { d.y };
        if shift {
            pos = app.session.general_unit().snap_to_ruler_tick(pos, xf.zoom);
        }
        if resp.drag_stopped() {
            // The new guide shows even if guides were hidden.
            app.ui.view.guides = true;
            if let Err(e) = app.run("guide.add", json!({ "vertical": vertical, "pos": pos })) {
                app.status(e);
            }
        } else if resp.dragged() {
            let at = xf.to_screen(if vertical { Point::new(pos, d.y) } else { Point::new(d.x, pos) });
            let (a, b) = if vertical {
                (pos2(at.x, canvas.top()), pos2(at.x, canvas.bottom()))
            } else {
                (pos2(canvas.left(), at.y), pos2(canvas.right(), at.y))
            };
            LineLook::guides(&app.session.prefs, t).guide(&ui.painter_at(canvas), a, b);
        }
    }
}

fn rulers(ui: &Ui, full: egui::Rect, xf: &Xf, hover: Option<Point>, unit: Unit, t: &Tokens) {
    let p = ui.painter();
    let [top, left, corner] = ruler_rects(full);
    for r in [top, left, corner] {
        p.rect_filled(r, 0.0, t.ruler);
    }
    p.line_segment([top.left_bottom(), top.right_bottom()], Stroke::new(1.0, t.border));
    p.line_segment([left.right_top(), left.right_bottom()], Stroke::new(1.0, t.border));
    // Crosshair in the origin box.
    p.line_segment([corner.center() - vec2(4.0, 0.0), corner.center() + vec2(4.0, 0.0)], Stroke::new(1.0, t.ruler_tick));
    p.line_segment([corner.center() - vec2(0.0, 4.0), corner.center() + vec2(0.0, 4.0)], Stroke::new(1.0, t.ruler_tick));
    // Pick a label step (in `unit`) that gives ≥ 50 px between labels; positions below are in `unit`
    // (`per` points each).
    let per = unit.points();
    let step = unit.ruler_step(xf.zoom);
    let minor = step / 10.0;
    let font = egui::FontId::proportional(9.5);
    let a = xf.to_doc(top.left_top());
    let b = xf.to_doc(top.right_top());
    let clip_top = p.with_clip_rect(top);
    let mut x = (a.x / per / minor).floor() * minor;
    while x * per <= b.x {
        let sx = xf.to_screen(Point::new(x * per, 0.0)).x;
        let is_major = ((x / step).round() * step - x).abs() < minor * 0.01;
        let is_mid = ((x / (step / 2.0)).round() * (step / 2.0) - x).abs() < minor * 0.01;
        let len = if is_major {
            RULER
        } else if is_mid {
            7.0
        } else {
            4.0
        };
        clip_top.line_segment([pos2(sx, top.bottom() - len), pos2(sx, top.bottom())], Stroke::new(1.0, t.ruler_tick));
        if is_major {
            clip_top.text(pos2(sx + 2.0, top.top() + 1.0), egui::Align2::LEFT_TOP, ruler_label(x, step), font.clone(), t.ruler_tick);
        }
        x += minor;
    }
    let a = xf.to_doc(left.left_top());
    let b = xf.to_doc(left.left_bottom());
    let clip_left = p.with_clip_rect(left);
    let mut y = (a.y / per / minor).floor() * minor;
    while y * per <= b.y {
        let sy = xf.to_screen(Point::new(0.0, y * per)).y;
        let is_major = ((y / step).round() * step - y).abs() < minor * 0.01;
        let is_mid = ((y / (step / 2.0)).round() * (step / 2.0) - y).abs() < minor * 0.01;
        let len = if is_major {
            RULER
        } else if is_mid {
            7.0
        } else {
            4.0
        };
        clip_left.line_segment([pos2(left.right() - len, sy), pos2(left.right(), sy)], Stroke::new(1.0, t.ruler_tick));
        if is_major {
            // Vertical labels read top-to-bottom, one digit per line like Illustrator.
            let s = ruler_label(y, step);
            for (k, ch) in s.chars().enumerate() {
                clip_left.text(
                    pos2(left.left() + 4.0, sy + 2.0 + k as f32 * 8.5),
                    egui::Align2::LEFT_TOP,
                    ch.to_string(),
                    font.clone(),
                    t.ruler_tick,
                );
            }
        }
        y += minor;
    }
    if let Some(h) = hover {
        let s = xf.to_screen(h);
        clip_top.line_segment([pos2(s.x, top.top()), pos2(s.x, top.bottom())], Stroke::new(1.0, t.text));
        clip_left.line_segment([pos2(left.left(), s.y), pos2(left.right(), s.y)], Stroke::new(1.0, t.text));
    }
}

fn to_screen_path(bp: &BezPath, xf: &Xf) -> Vec<Vec<Pos2>> {
    // Flatten for drawing: a polyline per subpath (tolerance ~0.25 screen px).
    let mut out: Vec<Vec<Pos2>> = vec![];
    let a = xf.affine();
    let mut t = bp.clone();
    t.apply_affine(a);
    let mut cur: Vec<Pos2> = vec![];
    kurbo_flatten(&t, 0.25, &mut |el| match el {
        PathEl::MoveTo(p) => {
            if cur.len() > 1 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
            cur.push(pos2(p.x as f32, p.y as f32));
        }
        PathEl::LineTo(p) => cur.push(pos2(p.x as f32, p.y as f32)),
        PathEl::ClosePath => {
            if let Some(f) = cur.first().copied() {
                cur.push(f);
            }
        }
        _ => {}
    });
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

fn stroke_path(p: &egui::Painter, bp: &BezPath, xf: &Xf, s: Stroke) {
    for line in to_screen_path(bp, xf) {
        p.add(Shape::line(line, s));
    }
}

fn c32(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Outline of a node for highlighting (paths, compound children, area type frames, other
/// text/image bounds).
fn node_outline(n: &Node) -> BezPath {
    let mut bp = BezPath::new();
    walk_drawn(n, &mut |c| match &c.kind {
        NodeKind::Path { path, .. } => bp.extend(path.to_bezpath()),
        // Area type shows its frame: the type area Direct Selection reshapes; type on a path its
        // path, which the selection tools' brackets sit on.
        NodeKind::Text(t) if c.perspective.is_none() && !matches!(t.kind, vectorcraft_doc::TextKind::Point) => {
            if let Some(path) = t.area_frame().or_else(|| t.type_path()) {
                bp.extend(path.to_bezpath());
            }
        }
        NodeKind::Text(_) | NodeKind::Image(_) | NodeKind::SymbolInstance { .. } => {
            if let Some(b) = c.geometric_bounds() {
                bp.extend(vectorcraft_geom::shapes::rectangle(b).to_bezpath());
            }
        }
        // An envelope shows its mesh (or top object), not its content.
        NodeKind::Envelope { .. } => {
            if let Some((lines, _)) = vectorcraft_doc::live::envelope_overlay(c) {
                bp.extend(lines.to_bezpath());
            }
        }
        _ => {}
    });
    bp
}

/// [`Node::walk`] over what a selection highlight shows: an envelope's content is left out (the
/// envelope shows its mesh instead).
fn walk_drawn<'a>(n: &'a Node, f: &mut impl FnMut(&'a Node)) {
    f(n);
    if matches!(n.kind, NodeKind::Envelope { .. }) {
        return;
    }
    for c in n.children().into_iter().flatten() {
        walk_drawn(c, f);
    }
}

/// The topmost editable object under document point `p` at `zoom`, as the selection tools pick it
/// (Selection & Anchor Display › Tolerance and Object Selection by Path Only).
fn hit_at(app: &VectorcraftApp, p: Point, zoom: f64) -> Option<vectorcraft_doc::hit::Hit> {
    let prefs = &app.session.prefs;
    let opt = vectorcraft_doc::hit::HitOptions {
        tol: prefs.selection_tolerance / zoom,
        outline: app.ui.view.outline,
        path_only: prefs.object_selection_by_path_only,
        inside: prefs.select_inside_shapes,
    };
    vectorcraft_doc::hit::hit_test(&app.session.active()?.doc, p, opt)
}

/// Right-click: the object under the pointer is selected first unless it already is, then the
/// context menu lists what applies to the selection ([`crate::menus::context_items`]).
fn context_menu(app: &mut VectorcraftApp, resp: &egui::Response, xf: &Xf) {
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
        && let Some(st) = app.session.active()
        && let Some(top) = hit_at(app, xf.to_doc(p), xf.zoom).map(|h| h.top_object(st.isolation))
        && !st.selection.contains(top)
    {
        // A locked or hidden object can't be selected; the menu is then for the selection as is.
        let _ = app.run("select.set", json!({ "ids": [top.0] }));
    }
    let mut clicked = None;
    resp.context_menu(|ui| crate::menus::context_menu_body(app, ui, &mut clicked));
    if let Some((id, p)) = clicked {
        crate::menus::invoke(app, &id, p);
    }
}

/// A panel drag ([`widgets::PanelDrag`]) dropped on art acts on the object under the pointer,
/// selected or not: a paint (swatches, a Fill/Stroke proxy, the Gradient panel's thumbnail) goes
/// to its active proxy (`paint.setFill`/`paint.setStroke` with its `ids`; a gradient fits it), the
/// Appearance panel's thumbnail gives the object (the topmost one hit) the appearance it carries
/// (`appearance.copyFrom`), a graphic style is applied to it (`graphicStyle.apply`; with Alt, on
/// top of its appearance). A chip follows the pointer meanwhile.
fn panel_drop(app: &mut VectorcraftApp, ui: &Ui, resp: &egui::Response, xf: &Xf) {
    crate::panels::swatches::drag_preview(app, ui.ctx());
    let Some(pos) = ui.input(|i| i.pointer.interact_pos()) else { return };
    // Not through a floating panel over the canvas.
    if ui.ctx().layer_id_at(pos).is_some_and(|l| l != resp.layer_id) {
        return;
    }
    let Some(d) = resp.dnd_release_payload::<widgets::PanelDrag>() else { return };
    // A symbol from the Symbols panel: an instance centred where it is dropped, on art or not.
    if let widgets::PanelDrag::Symbol(name) = &*d {
        let at = xf.to_doc(pos);
        if let Err(e) = app.run("symbol.place", json!({"name": name, "x": at.x, "y": at.y})) {
            app.status(e);
        }
        return;
    }
    let Some(hit) = hit_at(app, xf.to_doc(pos), xf.zoom) else { return };
    let Some(st) = app.session.active() else { return };
    let (cmd, params) = match &*d {
        widgets::PanelDrag::Paint { params, .. } => {
            // Colour groups paint nothing.
            if params.is_null() {
                return;
            }
            let mut params = params.clone();
            params["ids"] = json!([vectorcraft_tools::xform::paint_owner(&st.doc, hit.leaf).0]);
            params["focus"] = json!(false);
            (crate::panels::proxy_cmd(app, false), params)
        }
        widgets::PanelDrag::Appearance(source) => {
            let target = hit.top_object(st.isolation);
            if target == *source {
                return;
            }
            ("appearance.copyFrom", json!({"source": source.0, "ids": [target.0]}))
        }
        widgets::PanelDrag::GraphicStyle(name) => {
            let add = ui.input(|i| i.modifiers.alt);
            ("graphicStyle.apply", json!({"name": name, "ids": [hit.top_object(st.isolation).0], "add": add}))
        }
        // A brush from the Brushes panel: the path it lands on takes it (a compound path as a whole).
        widgets::PanelDrag::Brush { name, .. } => {
            ("brush.apply", json!({"name": name, "ids": [vectorcraft_tools::xform::paint_owner(&st.doc, hit.leaf).0]}))
        }
        // Art dragged back onto the canvas: its move was already dropped. (A symbol was placed
        // above.)
        widgets::PanelDrag::Art(_) | widgets::PanelDrag::Symbol(_) => return,
    };
    if let Err(e) = app.run(cmd, params) {
        app.status(e);
    }
}

fn hover_highlight(app: &VectorcraftApp, p: &egui::Painter, xf: &Xf) {
    let Some(h) = app.hover_doc else { return };
    if app.session.tool_busy() || !matches!(app.session.tool_id(), "selection" | "directSelection" | "groupSelection") {
        return;
    }
    let Some(st) = app.session.active() else { return };
    let Some(hit) = hit_at(app, h, xf.zoom) else { return };
    let id = if app.session.tool_id() == "selection" { hit.top_object(st.isolation) } else { hit.leaf };
    if st.selection.contains(id) {
        return;
    }
    if let Some(n) = st.doc.node(id) {
        let color = c32(st.doc.layer_color(id));
        stroke_path(p, &node_outline(n), xf, Stroke::new(1.5, color));
    }
}

/// Selected anchors are drawn slightly deeper than the layer colour (#4f80ff → #3d82ff for Layer 1).
fn selected_anchor(c: Color32) -> Color32 {
    if c == Color32::from_rgb(0x4f, 0x80, 0xff) { Color32::from_rgb(0x3d, 0x82, 0xff) } else { c }
}

/// How anchors and handles look: Selection & Anchor Display › Size and Handles.
#[derive(Clone, Copy)]
struct HandleLook<'a> {
    /// Points bigger (or smaller) than the default size 3, per step of Size (1–7).
    grow: f32,
    /// `solid` (the default), `hollow` or `large`.
    style: &'a str,
}

impl<'a> HandleLook<'a> {
    fn of(prefs: &'a vectorcraft_engine::Prefs) -> Self {
        Self { grow: prefs.anchor_size.clamp(1, 7) as f32 - 3.0, style: &prefs.handle_style }
    }

    /// A direction handle's end at `c`: a solid dot, a hollow one or a larger solid one.
    fn draw(self, p: &egui::Painter, c: Pos2, color: Color32) {
        let r = 2.75 + self.grow / 2.0;
        let hollow = self.style == "hollow";
        p.circle_filled(c, if self.style == "large" { r + 1.5 } else { r }, if hollow { Color32::WHITE } else { color });
        if hollow {
            p.circle_stroke(c, r, Stroke::new(1.0, color));
        }
    }
}

fn anchor_square(p: &egui::Painter, c: Pos2, color: Color32, filled: bool, size: f32) {
    let r = egui::Rect::from_center_size(c, vec2(size, size));
    if filled {
        p.rect_filled(r, 0.0, color);
    } else {
        p.rect_filled(r, 0.0, Color32::WHITE);
        p.rect_stroke(r, 0.0, Stroke::new(1.0, color), StrokeKind::Inside);
    }
}

/// Type → Show Hidden Characters: spaces as dots, ¶ at paragraph ends, # at the end of a story.
fn hidden_chars_overlay(app: &VectorcraftApp, p: &egui::Painter, xf: &Xf) {
    let Some(st) = app.session.active() else { return };
    let clip = p.clip_rect();
    let color = Color32::from_rgb(0x4f, 0x9d, 0xff);
    let font = egui::FontId::proportional(((10.0 * xf.zoom) as f32).clamp(7.0, 18.0));
    st.doc.walk(|n| {
        let NodeKind::Text(tx) = &n.kind else { return };
        if !n.visible || n.geometric_bounds().is_none_or(|b| !xf.rect_to_screen(b).intersects(clip)) {
            return;
        }
        let lay = vectorcraft_text::layout(vectorcraft_text::FontDb::global(), tx);
        if lay.on_path {
            return;
        }
        let text = tx.plain_text();
        let to = |q: Point| xf.to_screen(tx.xf * q);
        for g in &lay.glyphs {
            if text.get(g.byte..).is_some_and(|s| s.starts_with(' ')) {
                p.circle_filled(to(Point::new(g.origin.x + g.advance / 2.0, g.origin.y - 3.0)), 1.2, color);
            }
        }
        for (i, line) in lay.lines.iter().enumerate() {
            let last = i + 1 == lay.lines.len();
            let breaks = text.get(line.end..).is_some_and(|s| s.starts_with('\n'));
            let (glyph, at) = if last && line.end >= text.len() {
                ("#", true)
            } else if breaks {
                ("¶", true)
            } else {
                ("", false)
            };
            if at {
                p.text(to(Point::new(line.x1 + 1.0, line.baseline)), egui::Align2::LEFT_BOTTOM, glyph, font.clone(), color);
            }
        }
    });
}

/// Text threads of the selected frames: a line from each frame's out port (bottom right) to the
/// next frame's in port (top left), like Illustrator's thread indicators.
fn thread_overlay(app: &VectorcraftApp, p: &egui::Painter, xf: &Xf) {
    let Some(st) = app.session.active() else { return };
    for thread in &st.doc.text_threads {
        if !thread.iter().any(|id| st.selection.objects.contains(id)) {
            continue;
        }
        let color = c32(st.doc.layer_color(thread[0]));
        let frames: Vec<vectorcraft_geom::Rect> = thread.iter().filter_map(|id| st.doc.node(*id).and_then(|n| n.geometric_bounds())).collect();
        for w in frames.windows(2) {
            let (out, inp) = (xf.to_screen(Point::new(w[0].x1, w[0].y1)), xf.to_screen(Point::new(w[1].x0, w[1].y0)));
            p.line_segment([out, inp], Stroke::new(1.0, color));
            for port in [out, inp] {
                p.rect_filled(egui::Rect::from_center_size(port, egui::vec2(7.0, 7.0)), 0.0, Color32::WHITE);
                p.rect_stroke(egui::Rect::from_center_size(port, egui::vec2(7.0, 7.0)), 0.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
                p.line_segment([port - egui::vec2(2.0, 0.0), port + egui::vec2(2.0, 0.0)], Stroke::new(1.0, color));
            }
        }
    }
}

/// The slices (unless View → Hide Slices): user and object slices outlined in the Slices
/// preferences' line colour, auto slices dashed and dimmer, each numbered at its top left (Show
/// Slice Numbers); selected slices have a bolder outline. The layout is cached per revision.
fn slice_overlay(app: &mut VectorcraftApp, p: &egui::Painter, xf: &Xf) {
    let Some(st) = app.session.active() else { return };
    let key = (st.uid, st.revision);
    let slices = match &app.canvas.slices {
        Some((k, s)) if *k == key => s.clone(),
        _ => {
            let s = std::sync::Arc::new(st.doc.slice_layout());
            app.canvas.slices = Some((key, s.clone()));
            s
        }
    };
    if slices.is_empty() {
        return;
    }
    let prefs = &app.session.prefs;
    let line = pref_color(&prefs.slice_line_color, Color32::from_rgb(0xff, 0x3f, 0x3f));
    let dim = line.gamma_multiply(0.55);
    let selected = &st.selection.slices;
    let objects = &st.selection.objects;
    for a in slices.iter() {
        let auto = a.source == vectorcraft_doc::SliceSource::Auto;
        let on = a.id.is_some_and(|id| selected.contains(&id) || objects.contains(&id));
        let q = xf.quad(a.rect);
        let ring: Vec<Pos2> = q.iter().chain(q.first()).copied().collect();
        if auto {
            p.extend(Shape::dashed_line(&ring, Stroke::new(1.0, dim), 3.0, 3.0));
        } else {
            p.add(Shape::line(ring, Stroke::new(if on { 2.0 } else { 1.0 }, line)));
        }
        if prefs.show_slice_numbers
            && let Some(tl) = q.first()
        {
            let font = egui::FontId::proportional(9.0);
            let galley = p.layout_no_wrap(format!("{:02}", a.number), font, Color32::WHITE);
            let badge = egui::Rect::from_min_size(*tl + vec2(1.0, 1.0), galley.size() + vec2(6.0, 2.0));
            p.rect_filled(badge, CornerRadius::ZERO, if auto { dim } else { line });
            p.galley(badge.min + vec2(3.0, 1.0), galley, Color32::WHITE);
        }
    }
}

/// The print tiling (View → Show Print Tiling, or the Print Tiling tool): each page of the
/// document's print settings as `print.preview` lays it out, its paper edge and its imageable
/// area dashed, numbered at its top left; tiles outside the tile range dimmer. Cached per revision.
fn print_tiling_overlay(app: &mut VectorcraftApp, p: &egui::Painter, xf: &Xf) {
    let Some(st) = app.session.active() else { return };
    let key = (st.uid, st.revision);
    let pages = match &app.canvas.print_tiling {
        Some((k, pages)) if *k == key => pages.clone(),
        _ => {
            // Settings that can't print have no pages to show.
            let pages = std::sync::Arc::new(vectorcraft_engine::cmd::printtiling::pages(&st.doc).unwrap_or_default());
            app.canvas.print_tiling = Some((key, pages.clone()));
            pages
        }
    };
    let font = egui::FontId::proportional(10.0);
    let ring = |r: [f64; 4]| {
        let q = xf.quad(Rect::new(r[0], r[1], r[2], r[3]));
        q.iter().chain(q.first()).copied().collect::<Vec<Pos2>>()
    };
    for page in pages.iter() {
        let color = if page.printed { PRINT_TILING } else { PRINT_TILING.gamma_multiply(0.45) };
        p.add(Shape::line(ring(page.page), Stroke::new(0.75, color)));
        let inner = ring(page.imageable);
        p.extend(Shape::dashed_line(&inner, Stroke::new(1.0, color), 4.0, 3.0));
        if let Some(corner) = inner.iter().copied().reduce(|a, b| pos2(a.x.min(b.x), a.y.min(b.y))) {
            p.text(corner + vec2(3.0, 2.0), egui::Align2::LEFT_TOP, page.number.to_string(), font.clone(), color);
        }
    }
}

/// The print tiling's lines: a neutral grey that reads on the paper and on the pasteboard.
const PRINT_TILING: Color32 = Color32::from_gray(96);

fn selection_overlay(app: &mut VectorcraftApp, p: &egui::Painter, xf: &Xf) {
    // The Selection tool's bounding box (rotated with the objects after a rotation).
    let show_box = app.session.tool_id() == "selection"
        && app.ui.view.bounding_box
        && app.session.active().is_some_and(|st| !st.selection.is_empty() && st.selection.anchors.is_empty());
    let bbox = if show_box { app.selection_box() } else { None };
    let app = &*app;
    let Some(st) = app.session.active() else { return };
    let tool = app.session.tool_id();
    let direct = matches!(tool, "directSelection" | "pen" | "addAnchor" | "deleteAnchor" | "anchorPoint" | "curvature");
    // Selection & Anchor Display › Size (1–7, 3 the default): anchors, handles and the bounding
    // box's handles a point bigger or smaller per step.
    let look = HandleLook::of(&app.session.prefs);
    let grow = look.grow;
    let anchor = |direct: bool| grow + if direct { 5.0 } else { 4.0 };
    // The selected anchors' handles (drawn once they are counted: Show handles when multiple
    // anchors are selected off shows them for a single one only).
    let (mut handles, mut with_handles) = (vec![], 0);
    for id in &st.selection.objects {
        let Some(n) = st.doc.node(*id) else { continue };
        let color = c32(st.doc.layer_color(*id));
        let partial = st.selection.partial(*id);
        // Path outlines.
        stroke_path(p, &node_outline(n), xf, Stroke::new(1.0, color));
        // Anchors (and handles for selected anchors in direct mode); a mesh envelope's points.
        walk_drawn(n, &mut |c| {
            if let Some((_, Some(grid))) = vectorcraft_doc::live::envelope_overlay(c) {
                for q in &grid.points {
                    anchor_square(p, xf.to_screen(q.p), color, false, anchor(direct));
                }
                return;
            }
            let NodeKind::Path { path, .. } = &c.kind else { return };
            for (si, ai, a) in path.anchors() {
                let sel = match partial {
                    Some(set) => set.contains(&(si, ai)),
                    None => !direct || c.id == *id,
                };
                let sp = xf.to_screen(a.p);
                if sel && (direct || partial.is_some()) {
                    with_handles += 1;
                    handles.extend([a.h_in, a.h_out].into_iter().filter(|h| h.distance(a.p) > 1e-6).map(|h| (sp, xf.to_screen(h), color)));
                }
                anchor_square(p, sp, if sel && partial.is_some() { selected_anchor(color) } else { color }, sel, anchor(partial.is_some() || direct));
            }
        });
        // Centre point (Attributes panel → Show Center).
        if n.shows_center()
            && let Some(b) = n.geometric_bounds()
        {
            anchor_square(p, xf.to_screen(b.center()), color, true, anchor(false));
        }
        // Point type: baseline marker (area type shows its frame instead, which may not be a
        // rectangle, and type on a path its path).
        if let NodeKind::Text(tx) = &n.kind
            && matches!(tx.kind, vectorcraft_doc::TextKind::Point)
        {
            let o = xf.to_screen(tx.xf * Point::ZERO);
            let b = n.geometric_bounds().unwrap_or_default();
            let e = xf.to_screen(Point::new(b.x1, (tx.xf * Point::ZERO).y));
            p.line_segment([o, e], Stroke::new(1.0, color));
            p.circle_filled(o, 2.5, color);
        }
    }
    if app.session.prefs.show_handles_multiple_anchors || with_handles <= 1 {
        for (sp, hp, color) in handles {
            p.line_segment([sp, hp], Stroke::new(1.0, color));
            look.draw(p, hp, color);
        }
    }
    // The spine of each selected blend (or of the blend a selected key object belongs to).
    let mut spines = vec![];
    for id in &st.selection.objects {
        let is_blend = |b: &vectorcraft_doc::NodeId| st.doc.node(*b).is_some_and(|n| matches!(n.kind, NodeKind::Blend { .. }));
        let Some(b) = [Some(*id), st.doc.parent_of(*id)].into_iter().flatten().find(is_blend).filter(|b| !spines.contains(b)) else { continue };
        spines.push(b);
        let Some(NodeKind::Blend { children, spec }) = st.doc.node(b).map(|n| &n.kind) else { continue };
        let Some((path, _)) = vectorcraft_doc::live::blend_spine(children, spec) else { continue };
        let color = c32(st.doc.layer_color(b));
        stroke_path(p, &path.to_bezpath(), xf, Stroke::new(1.0, color));
        for (_, _, a) in path.anchors() {
            anchor_square(p, xf.to_screen(a.p), color, false, anchor(direct));
        }
    }
    // Live Corners widgets (Selection / Direct Selection on a single live rectangle).
    if matches!(tool, "selection" | "directSelection")
        && app.ui.view.corner_widgets
        && let Some(w) = vectorcraft_tools::corners::CornerWidgets::of(&st.doc, &st.selection, xf.zoom)
            .and_then(|w| w.within_angle(app.session.prefs.hide_corner_widget_above))
    {
        let color = c32(st.doc.layer_color(w.id));
        for sp in w.visible().map(|q| xf.to_screen(q)) {
            p.circle_filled(sp, 3.0, Color32::WHITE);
            p.circle_stroke(sp, 3.0, Stroke::new(1.0, color));
            p.circle_filled(sp, 1.0, color);
        }
    }
    // Bounding box with handles (Selection tool).
    if let Some(b) = bbox {
        let color = c32(st.doc.layer_color(st.selection.objects[0]));
        p.add(Shape::closed_line(b.corners().iter().map(|q| xf.to_screen(*q)).collect(), Stroke::new(1.0, color)));
        for h in vectorcraft_tools::bbox::Handle::ALL {
            let c = xf.to_screen(b.to_doc() * h.pos(b.rect));
            let hr = egui::Rect::from_center_size(c, vec2(6.0 + grow, 6.0 + grow));
            p.rect_filled(hr, 0.0, Color32::WHITE);
            p.rect_stroke(hr, 0.0, Stroke::new(1.0, color), StrokeKind::Inside);
        }
    }
}

/// While the Type tool edits, let the system IME compose (egui-winit allows it only in frames that
/// set `ime`) and keep its candidate window under the caret.
fn ime_output(app: &mut VectorcraftApp, ctx: &egui::Context, xf: &Xf) {
    let ours = app.session.tool_wants_text() && !ctx.egui_wants_keyboard_input() && app.ui.dialog.is_none() && !app.ui.palette_open;
    let caret = if ours { app.session.tool_ime_caret(app.view_info()) } else { None };
    let Some((a, b)) = caret else {
        // The IME goes away with its marked text: keep that text as typed, like a click away.
        crate::shortcuts::keep_marked_text(app);
        return;
    };
    let r = egui::Rect::from_two_pos(xf.to_screen(a), xf.to_screen(b)).expand2(vec2(1.0, 0.0));
    // The tool ended the composition itself: the IME must drop what it still has marked.
    let interrupt = app.ime_marked.is_some() && !app.session.tool_composing();
    if interrupt {
        app.ime_marked = None;
        app.ime_discard = true;
    }
    ctx.output_mut(|o| {
        o.ime = Some(egui::output::IMEOutput { purpose: egui::IMEPurpose::Normal, rect: r, cursor_rect: r, should_interrupt_composition: interrupt });
    });
}

/// Is overlay label `text` the Artboard tool's "01 - <artboard name>"? It holds a name, so it is
/// shown as it is; the tools' other labels ("anchor", "path", Puppet Warp's warning) are ours.
fn names_an_artboard(text: &str) -> bool {
    text.split_once(" - ").is_some_and(|(n, _)| n.len() >= 2 && n.bytes().all(|b| b.is_ascii_digit()))
}

fn draw_overlays(p: &egui::Painter, xf: &Xf, overlays: &[Overlay], t: &Tokens, look: HandleLook) {
    for o in overlays {
        match o {
            Overlay::Marquee(r) => {
                let sr = xf.rect_to_screen(*r);
                let pts = [sr.left_top(), sr.right_top(), sr.right_bottom(), sr.left_bottom(), sr.left_top()];
                for w in pts.windows(2) {
                    p.extend(Shape::dashed_line(&[w[0], w[1]], Stroke::new(1.0, Color32::from_gray(90)), 3.0, 3.0));
                }
            }
            Overlay::Path { path, color, width, dashed } => {
                let s = Stroke::new(*width, c32(*color));
                if *dashed {
                    for line in to_screen_path(path, xf) {
                        p.extend(Shape::dashed_line(&line, s, 4.0, 3.0));
                    }
                } else {
                    stroke_path(p, path, xf, s);
                }
            }
            Overlay::Line { a, b, color, dashed } => {
                let s = Stroke::new(1.0, c32(*color));
                let (a, b) = (xf.to_screen(*a), xf.to_screen(*b));
                if *dashed {
                    p.extend(Shape::dashed_line(&[a, b], s, 4.0, 3.0));
                } else {
                    p.line_segment([a, b], s);
                }
            }
            Overlay::Anchor { p: pt, color, filled, size } => anchor_square(p, xf.to_screen(*pt), c32(*color), *filled, *size),
            Overlay::Handle { p: pt, color } => look.draw(p, xf.to_screen(*pt), c32(*color)),
            Overlay::Label { p: pt, text, color } => {
                let sp = xf.to_screen(*pt) + vec2(8.0, -14.0);
                p.text(
                    sp,
                    egui::Align2::LEFT_TOP,
                    crate::panels::label_or_name(text, !names_an_artboard(text)),
                    egui::FontId::proportional(11.0),
                    c32(*color),
                );
            }
            Overlay::Highlight { quad, color } => {
                let c = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                p.add(Shape::convex_polygon(quad.iter().map(|q| xf.to_screen(*q)).collect(), c, Stroke::NONE));
            }
            Overlay::Measure { p: pt, text } => {
                let sp = xf.to_screen(*pt) + vec2(14.0, 14.0);
                let galley = p.layout(text.clone(), egui::FontId::proportional(11.0), Color32::WHITE, 200.0);
                let r = egui::Rect::from_min_size(sp, galley.size() + vec2(12.0, 8.0));
                p.rect_filled(r, CornerRadius::same(3), t.measure_bg);
                p.galley(sp + vec2(6.0, 4.0), galley, Color32::WHITE);
            }
            Overlay::GridLine { a, b, color } => {
                let c = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                p.line_segment([xf.to_screen(*a), xf.to_screen(*b)], Stroke::new(1.0, c));
            }
            Overlay::Swatch { p: pt, color, selected } => {
                // A white disc under the colour shows its opacity; a dark rim keeps it readable on
                // any art, and the accent ring marks the selected stop.
                let c = xf.to_screen(*pt);
                p.circle_filled(c, 6.0, Color32::WHITE);
                p.circle_filled(c, 5.0, Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]));
                p.circle_stroke(c, 6.0, Stroke::new(1.0, Color32::from_gray(32)));
                if *selected {
                    p.circle_stroke(c, 8.0, Stroke::new(2.0, t.accent));
                }
            }
        }
    }
}

/// The Home screen shown when no document is open.
fn home(app: &mut VectorcraftApp, ui: &mut Ui, rect: egui::Rect) {
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_filled(rect, 0.0, t.panel_darker);
    let inner = rect.shrink2(vec2((rect.width() - 820.0).max(40.0) / 2.0, 60.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
    let ui = &mut child;
    ui.label(egui::RichText::new(tl!("Welcome to Roidery")).font(theme::semibold(26.0)).color(t.text));
    ui.add_space(4.0);
    ui.label(egui::RichText::new(tl!("Embroidery digitizing — fast, open, scriptable.")).size(14.0).color(t.text_dim));
    ui.add_space(22.0);
    ui.horizontal(|ui| {
        if widgets::primary_button(ui, tl!("New file")).clicked() {
            app.run("file.newDialog", json!({})).ok();
        }
        ui.add_space(8.0);
        if widgets::secondary_button(ui, tl!("Open")).clicked() {
            app.run("file.open", json!({})).ok();
        }
    });
    ui.add_space(28.0);
    ui.label(egui::RichText::new(tl!("Quickly start a new file")).font(theme::semibold(14.0)).color(t.text));
    ui.add_space(10.0);
    // A few of New Document's presets (`file.newPresets`), as its cards: Roidery's hoops.
    let presets = ["Hoop 100 × 100 mm", "Hoop 130 × 180 mm", "Hoop 160 × 260 mm", "Hoop 200 × 200 mm", "Left Chest 95 × 95 mm", "Cap 130 × 60 mm"];
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(14.0, 14.0);
        let cards: Vec<_> = presets.iter().filter_map(|name| vectorcraft_engine::cmd::newdoc::find(&app.session, name)).collect();
        for s in cards {
            if crate::dialogs::preset_card(ui, &s, false).clicked() {
                app.run("file.new", json!({ "preset": s.name })).ok();
            }
        }
    });
}

fn kurbo_flatten(p: &BezPath, tol: f64, f: &mut impl FnMut(PathEl)) {
    vectorcraft_geom::kurbo::flatten(p.elements().iter().copied(), tol, f);
}

fn upload(app: &mut VectorcraftApp, ctx: &egui::Context, img: &vectorcraft_render::Rendered) {
    let color = egui::ColorImage::from_rgba_premultiplied([img.width as usize, img.height as usize], &img.pixels);
    match &mut app.canvas.texture {
        Some(tex) => tex.set(color, egui::TextureOptions::LINEAR),
        None => app.canvas.texture = Some(ctx.load_texture("canvas", color, egui::TextureOptions::LINEAR)),
    }
}

/// The Contextual Task Bar: a floating pill under the selection with the most likely next actions.
fn task_bar(app: &mut VectorcraftApp, ui: &mut Ui, xf: &Xf) {
    let t = Tokens::get(ui.ctx());
    let Some(st) = app.session.active() else { return };
    if st.selection.is_empty() || !matches!(app.session.tool_id(), "selection" | "directSelection" | "groupSelection") {
        return;
    }
    let Some(b) = st.doc.bounds_of(&st.selection.objects, true) else { return };
    let n = st.selection.len();
    let first = st.selection.objects.first().and_then(|id| st.doc.node(*id)).cloned();
    let is_group = first.as_ref().is_some_and(|f| matches!(f.kind, NodeKind::Group { .. }));
    let is_text = first.as_ref().is_some_and(|f| matches!(f.kind, NodeKind::Text(_)));
    let mut items: Vec<(&str, &str, &str)> = vec![]; // (label, icon, command)
    if !st.selection.anchors.is_empty() {
        items.push((tl!("Remove Anchor Points"), "dc-pen-delete", "path.removeAnchors"));
    }
    if n > 1 {
        items.push((tl!("Group"), "group", "object.group"));
        items.push((tl!("Unite"), "squares-unite", "object.pathfinder.unite"));
    } else if is_group {
        items.push((tl!("Ungroup"), "ungroup", "object.ungroup"));
        items.push((tl!("Isolate"), "square-dashed", "object.isolate"));
    } else if is_text {
        items.push((tl!("Create Outlines"), "type", "type.createOutlines"));
    } else {
        items.push((tl!("Offset Path"), "square-dashed", "object.path.offsetPath"));
        items.push((tl!("Simplify"), "spline", "object.path.simplify"));
    }
    items.push((tl!("Duplicate"), "copy", "edit.duplicate"));
    let fill = first.as_ref().map(|f| f.appearance.fill_paint()).unwrap_or_default();
    let anchor = xf.to_screen(Point::new(b.center().x, b.y1));
    let est_w = 118.0 + items.iter().map(|(l, _, _)| l.len() as f32 * 7.2 + 44.0).sum::<f32>();
    let x = (anchor.x - est_w / 2.0).clamp(xf.rect.left() + 8.0, (xf.rect.right() - est_w - 8.0).max(xf.rect.left() + 8.0));
    let y = (anchor.y + 28.0).min(xf.rect.bottom() - 56.0);
    let mut run: Option<String> = None;
    egui::Area::new(egui::Id::new("task-bar")).order(egui::Order::Middle).fixed_pos(pos2(x, y)).show(ui.ctx(), |ui| {
        egui::Frame::NONE
            .fill(t.panel)
            .stroke(Stroke::new(1.0, t.tool_active))
            .corner_radius(CornerRadius::same(5))
            .inner_margin(egui::Margin::symmetric(8, 6))
            .shadow(egui::epaint::Shadow { offset: [0, 3], blur: 10, spread: 0, color: Color32::from_black_alpha(70) })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let (g, _) = ui.allocate_exact_size(vec2(4.0, 28.0), Sense::hover());
                    ui.painter().rect_filled(g.shrink2(vec2(0.5, 4.0)), CornerRadius::same(2), t.button_border);
                    for (label, icon, cmd) in &items {
                        let galley = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(13.0), t.text_strong);
                        let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 38.0, 30.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(3), t.hover);
                        }
                        ui.painter().rect_stroke(r, CornerRadius::same(3), Stroke::new(1.0, t.button_border), StrokeKind::Inside);
                        crate::icons::paint(ui, icon, egui::Rect::from_min_size(r.min + vec2(8.0, 7.0), vec2(16.0, 16.0)), t.icon);
                        ui.painter().galley(pos2(r.left() + 30.0, r.center().y - galley.size().y / 2.0), galley, t.text_strong);
                        if resp.clicked() {
                            run = Some(cmd.to_string());
                        }
                    }
                    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 30.0), Sense::click());
                    widgets::paint_chip(ui, egui::Rect::from_center_size(r.center(), vec2(16.0, 16.0)), &fill);
                    ui.painter().rect_stroke(
                        egui::Rect::from_center_size(r.center(), vec2(16.0, 16.0)),
                        0.0,
                        Stroke::new(1.0, t.button_border),
                        StrokeKind::Outside,
                    );
                    if resp.on_hover_text(tl!("Fill")).clicked() {
                        app.session.fill_active = true;
                        app.ui.open_panel = Some("swatches".into());
                    }
                    if widgets::icon_button(ui, "lock", tl!("Lock (⌘2)"), false, 30.0).clicked() {
                        run = Some("object.lock".into());
                    }
                    if widgets::icon_button(ui, "ellipsis", tl!("Hide Contextual Task Bar"), false, 30.0).clicked() {
                        run = Some("window.taskBar".into());
                    }
                });
            });
    });
    if let Some(c) = run {
        crate::menus::invoke(app, &c, json!({}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

    /// One headless canvas frame on an 800 × 600 window.
    fn frame(app: &mut VectorcraftApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), events, ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| show(app, ui));
        out.textures_delta.clear();
    }

    /// The Artboard tool's label holds the artboard's name (never translated); the tools' own
    /// labels are interface text.
    #[test]
    fn artboard_labels_name_the_artboard() {
        assert!(names_an_artboard("01 - Layers") && names_an_artboard("12 - Artboard 12 - copy"));
        assert!(!names_an_artboard("anchor") && !names_an_artboard("1 - x") && !names_an_artboard("Off the mesh - move closer"));
    }

    fn middle(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Middle, pressed, modifiers: Default::default() }
    }

    /// One headless frame with the canvas under a 40-point bar, as it is under the document tabs.
    fn frame_under_bar(app: &mut VectorcraftApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), events, ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| {
            ui.add_space(40.0);
            show(app, ui);
        });
        out.textures_delta.clear();
    }

    #[test]
    fn a_click_just_outside_the_canvas_does_not_reach_the_tool() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        // A square in the middle of the artboard, where the view is centred.
        let id = app.session.execute("shape.rectangle", &json!({"x": 180, "y": 130, "width": 40, "height": 40})).unwrap()["id"].as_u64().unwrap();
        app.session.execute("select.none", &json!({})).unwrap();
        let ctx = egui::Context::default();
        frame_under_bar(&mut app, &ctx, vec![]);
        let rect = app.canvas_rect.unwrap();
        let click = |app: &mut VectorcraftApp, p: Pos2| {
            let button =
                |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
            frame_under_bar(app, &ctx, vec![egui::Event::PointerMoved(p)]);
            frame_under_bar(app, &ctx, vec![button(true)]);
            frame_under_bar(app, &ctx, vec![button(false)]);
        };
        let selected = |app: &VectorcraftApp| app.session.active().unwrap().selection.objects.clone();
        // Two pixels above the canvas, within egui's interaction radius: nothing happens.
        click(&mut app, pos2(rect.center().x, rect.top() - 2.0));
        assert!(selected(&app).is_empty(), "a click above the canvas selected {:?}", selected(&app));
        // At the canvas's centre: the square.
        click(&mut app, rect.center());
        assert_eq!(selected(&app), vec![vectorcraft_doc::NodeId(id)]);
    }

    #[test]
    fn a_drag_from_a_ruler_onto_the_canvas_makes_a_guide() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        app.ui.view.rulers = true;
        app.ui.view.guides = false;
        // A drawing tool: the drag must not draw.
        app.select_tool("rectangle");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        let rect = app.canvas_rect.unwrap();
        let xf = Xf::new(rect, app.view().unwrap());
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        let drag = |app: &mut VectorcraftApp, from: Pos2, to: Pos2| {
            frame(app, &ctx, vec![egui::Event::PointerMoved(from)]);
            frame(app, &ctx, vec![button(from, true)]);
            frame(app, &ctx, vec![egui::Event::PointerMoved(to)]);
            frame(app, &ctx, vec![button(to, false)]);
        };
        let guides = |app: &VectorcraftApp| app.session.active().unwrap().doc.guides.iter().map(|g| (g.vertical, g.pos)).collect::<Vec<_>>();
        // From the top ruler: a horizontal guide where the button was released.
        let to = pos2(rect.center().x, rect.top() + 120.0);
        drag(&mut app, pos2(rect.center().x, rect.top() - RULER / 2.0), to);
        assert_eq!(guides(&app), [(false, xf.to_doc(to).y)]);
        assert!(app.ui.view.guides, "the new guide shows");
        // From the left ruler: a vertical one.
        let to2 = pos2(rect.left() + 200.0, rect.center().y);
        drag(&mut app, pos2(rect.left() - RULER / 2.0, rect.center().y), to2);
        assert_eq!(guides(&app), [(false, xf.to_doc(to).y), (true, xf.to_doc(to2).x)]);
        // Released back on the ruler: no guide.
        drag(&mut app, pos2(rect.center().x, rect.top() - RULER / 2.0), pos2(rect.center().x + 40.0, rect.top() - 4.0));
        assert_eq!(guides(&app).len(), 2);
        assert_eq!(app.session.active().unwrap().doc.art_bounds(), None, "the tool drew nothing");
    }

    /// #414: guides dragged out of the rulers are picked and dragged with the Selection tool
    /// (highlighted while selected), go with Delete or Backspace, and dragged back onto their
    /// ruler.
    #[test]
    fn the_selection_tool_moves_and_deletes_ruler_guides() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        // A square under the guides: the press takes the guide.
        app.session.execute("shape.rectangle", &json!({"x": 50, "y": 50, "width": 300, "height": 200})).unwrap();
        app.session.execute("select.none", &json!({})).unwrap();
        let art = app.session.active().unwrap().doc.art_bounds();
        app.ui.view.rulers = true;
        app.select_tool("selection");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        let rect = app.canvas_rect.unwrap();
        let xf = Xf::new(rect, app.view().unwrap());
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        let drag = |app: &mut VectorcraftApp, from: Pos2, to: Pos2| {
            frame(app, &ctx, vec![egui::Event::PointerMoved(from)]);
            frame(app, &ctx, vec![button(from, true)]);
            frame(app, &ctx, vec![egui::Event::PointerMoved(to)]);
            frame(app, &ctx, vec![button(to, false)]);
        };
        let guides = |app: &VectorcraftApp| app.session.active().unwrap().doc.guides.iter().map(|g| (g.vertical, g.pos)).collect::<Vec<_>>();
        // Two guides out of the rulers.
        let (h, v) = (pos2(rect.center().x + 30.0, rect.center().y + 20.0), pos2(rect.center().x - 40.0, rect.center().y));
        drag(&mut app, pos2(h.x, rect.top() - RULER / 2.0), h);
        drag(&mut app, pos2(rect.left() - RULER / 2.0, v.y), v);
        assert_eq!(guides(&app), [(false, xf.to_doc(h).y), (true, xf.to_doc(v).x)]);
        // Drag the horizontal one down 30 px (across the vertical one: the nearer is picked).
        let to = pos2(h.x + 10.0, h.y + 30.0);
        drag(&mut app, h, to);
        let st = app.session.active().unwrap();
        assert_eq!(guides(&app), [(false, xf.to_doc(to).y), (true, xf.to_doc(v).x)]);
        assert_eq!((st.selection.guides.clone(), st.selection.objects.clone()), (vec![0], vec![]));
        assert_eq!(st.history.undo.last().unwrap().label, "Move Guide");
        let picked = Tokens::get(&ctx).selection;
        let s = shapes(&mut app, &ctx);
        assert!(s.iter().any(|s| matches!(s, Shape::LineSegment { points, stroke } if stroke.color == picked && points[0].y == to.y)), "highlighted");
        // Backspace deletes the selected guide; Delete too.
        let press = |app: &mut VectorcraftApp, key| {
            let ev = egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
            let mut out = ctx.run_ui(egui::RawInput { events: vec![ev], ..Default::default() }, |ui| crate::shortcuts::handle(app, ui.ctx()));
            out.textures_delta.clear();
        };
        press(&mut app, egui::Key::Backspace);
        assert_eq!(guides(&app), [(true, xf.to_doc(v).x)]);
        app.session.execute("guide.select", &json!({"indexes": [0]})).unwrap();
        press(&mut app, egui::Key::Delete);
        assert!(guides(&app).is_empty());
        app.session.execute("edit.undo", &json!({})).unwrap();
        // Hide Guides deselects them.
        app.session.execute("guide.select", &json!({"indexes": [0]})).unwrap();
        app.run("view.guides", json!({})).unwrap();
        assert!(app.session.active().unwrap().selection.guides.is_empty());
        app.run("view.guides", json!({})).unwrap();
        // Dragged back onto its ruler, the vertical guide goes.
        drag(&mut app, v, pos2(rect.left() - RULER / 2.0, v.y + 20.0));
        assert!(guides(&app).is_empty(), "{:?}", guides(&app));
        assert_eq!(app.session.active().unwrap().doc.art_bounds(), art, "the square stayed");
    }

    #[test]
    fn cmd_with_another_tool_drags_with_the_selection_tool_used_last() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let id = app.session.execute("shape.rectangle", &json!({"x": 100, "y": 100, "width": 100, "height": 50})).unwrap()["id"].as_u64().unwrap();
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        let xf = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap());
        let cmd_frame = |app: &mut VectorcraftApp, events: Vec<egui::Event>| {
            let screen = egui::Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
            let events = std::iter::once(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND)).chain(events).collect();
            let raw = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
            let mut out = ctx.run_ui(raw, |ui| show(app, ui));
            out.textures_delta.clear();
        };
        let button =
            |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::COMMAND };
        // A Cmd drag from `a` to `b` with the Pen tool; → the tool that did it.
        let cmd_drag = |app: &mut VectorcraftApp, a: Pos2, b: Pos2| {
            app.select_tool("pen");
            cmd_frame(app, vec![egui::Event::PointerMoved(a), button(a, true)]);
            let used = app.session.tool_id().to_string();
            cmd_frame(app, vec![egui::Event::PointerMoved(b)]);
            cmd_frame(app, vec![button(b, false)]);
            assert_eq!(app.session.tool_id(), "pen", "back to the Pen tool");
            used
        };
        let bounds = |app: &VectorcraftApp| app.session.active().unwrap().doc.node(vectorcraft_doc::NodeId(id)).unwrap().geometric_bounds().unwrap();
        // Direct Selection used last: the drag moves the rectangle's bottom-right corner only.
        app.select_tool("directSelection");
        let corner = xf.to_screen(Point::new(200.0, 150.0));
        assert_eq!(cmd_drag(&mut app, corner, corner + vec2(20.0, 20.0)), "directSelection");
        let b = bounds(&app);
        assert_eq!((b.x0, b.y0), (100.0, 100.0));
        assert!((b.x1 - xf.to_doc(corner + vec2(20.0, 20.0)).x).abs() < 1e-6, "{b:?}");
        // Group Selection, then Selection: each is the one used.
        app.select_tool("groupSelection");
        let inside = xf.to_screen(Point::new(130.0, 120.0));
        assert_eq!(cmd_drag(&mut app, inside, inside), "groupSelection");
        app.select_tool("selection");
        assert_eq!(cmd_drag(&mut app, inside, inside), "selection");
    }

    /// One headless canvas frame → the shapes drawn, `Shape::Vec`s flattened.
    fn shapes(app: &mut VectorcraftApp, ctx: &egui::Context) -> Vec<Shape> {
        fn flat(s: Shape, out: &mut Vec<Shape>) {
            match s {
                Shape::Vec(v) => v.into_iter().for_each(|s| flat(s, out)),
                s => out.push(s),
            }
        }
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| show(app, ui));
        out.textures_delta.clear();
        let mut v = vec![];
        out.shapes.into_iter().for_each(|c| flat(c.shape, &mut v));
        v
    }

    /// Guides & Grid (#394): the grid in Grid Color, behind the art or, with Grids In Back off,
    /// over it, as dots in the Dots style; the guides in Guides Color and Style.
    #[test]
    fn grid_and_guides_take_their_preferences() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        app.session.execute("guide.add", &json!({"vertical": true, "pos": 100})).unwrap();
        app.ui.view.grid = true;
        app.ui.view.guides = true;
        let (red, green) = (Color32::from_rgb(255, 0, 0), Color32::from_rgb(0, 255, 0));
        app.session.execute("prefs.set", &json!({"values": {"gridColor": "#ff0000", "guideColor": "#00ff00"}})).unwrap();
        let ctx = egui::Context::default();
        let red_line = |s: &Shape| matches!(s, Shape::LineSegment { stroke, .. } if stroke.color == red);
        // → (index of the first red gridline, index of the art's image).
        let order = |app: &mut VectorcraftApp| {
            let s = shapes(app, &ctx);
            let art = app.canvas.texture.as_ref().unwrap().id();
            let image = s.iter().position(|s| matches!(s, Shape::Mesh(m) if m.texture_id == art)).expect("the art");
            (s.iter().position(red_line).expect("a gridline in Grid Color"), image, s)
        };
        let (grid, art, s) = order(&mut app);
        assert!(grid < art, "Grids In Back: under the art");
        assert!(s.iter().any(|s| matches!(s, Shape::LineSegment { stroke, .. } if stroke.color == green)), "the guide in Guides Color");
        app.session.execute("prefs.set", &json!({"key": "gridsInBack", "value": false})).unwrap();
        let (grid, art, _) = order(&mut app);
        assert!(grid > art, "Grids In Back off: over the art");
        app.session.execute("prefs.set", &json!({"values": {"gridStyle": "dots", "guideStyle": "dots"}})).unwrap();
        let s = shapes(&mut app, &ctx);
        assert!(!s.iter().any(red_line), "no gridlines in the Dots style");
        let red_dots = |s: &Shape| matches!(s, Shape::Mesh(m) if m.vertices.len() > 40 && m.vertices.iter().all(|v| v.color == red));
        assert!(s.iter().any(red_dots), "the grid's dots");
        assert!(s.iter().any(|s| matches!(s, Shape::Circle(c) if c.fill == green)), "the guide dotted");
    }

    /// Selection & Anchor Display › Size (#394): anchors and the bounding box's handles grow.
    #[test]
    fn anchor_size_scales_anchors_and_handles() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        app.session.execute("shape.rectangle", &json!({"x": 150, "y": 100, "width": 100, "height": 100})).unwrap();
        let ctx = egui::Context::default();
        // The sizes of the squares drawn (anchors and handles are squares).
        let sizes = |app: &mut VectorcraftApp| {
            let mut v: Vec<f32> = shapes(app, &ctx).iter().filter_map(|s| if let Shape::Rect(r) = s { Some(r.rect.width()) } else { None }).collect();
            v.sort_by(f32::total_cmp);
            v.dedup();
            v
        };
        let small = sizes(&mut app);
        assert!(small.contains(&4.0) && small.contains(&6.0), "anchors 4, handles 6 by default: {small:?}");
        app.session.execute("prefs.set", &json!({"key": "anchorSize", "value": 7})).unwrap();
        let big = sizes(&mut app);
        assert!(big.contains(&8.0) && big.contains(&10.0) && !big.contains(&4.0), "4 points bigger at 7: {big:?}");
    }

    /// Selection & Anchor Display › Handles and Show handles when multiple anchors are selected
    /// (#394): handle ends drawn solid, hollow or large; with the latter off, a second selected
    /// anchor hides the handles.
    #[test]
    fn handle_style_and_handles_of_multiple_anchors() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let id = app.session.execute("shape.ellipse", &json!({"x": 100, "y": 100, "width": 100, "height": 100})).unwrap()["id"].clone();
        app.session.select_tool("directSelection", app.view_info()).unwrap();
        let ctx = egui::Context::default();
        let select = |app: &mut VectorcraftApp, anchors: serde_json::Value| {
            app.session.execute("select.anchors", &json!({"id": id, "anchors": anchors, "mode": "set"})).unwrap();
        };
        // The handle ends: (radius, filled white).
        let dots = |app: &mut VectorcraftApp| -> Vec<(f32, bool)> {
            shapes(app, &ctx)
                .iter()
                .filter_map(|s| match s {
                    Shape::Circle(c) if c.fill != Color32::TRANSPARENT => Some((c.radius, c.fill == Color32::WHITE)),
                    _ => None,
                })
                .collect()
        };
        select(&mut app, json!([[0, 0]]));
        assert_eq!(dots(&mut app), [(2.75, false); 2], "solid by default");
        app.session.execute("prefs.set", &json!({"key": "handleStyle", "value": "hollow"})).unwrap();
        assert_eq!(dots(&mut app), [(2.75, true); 2]);
        app.session.execute("prefs.set", &json!({"key": "handleStyle", "value": "large"})).unwrap();
        assert_eq!(dots(&mut app), [(4.25, false); 2]);
        select(&mut app, json!([[0, 0], [0, 1]]));
        assert_eq!(dots(&mut app).len(), 4, "both anchors' handles");
        app.session.execute("prefs.set", &json!({"key": "showHandlesMultipleAnchors", "value": false})).unwrap();
        assert!(dots(&mut app).is_empty(), "off: none for two anchors");
        select(&mut app, json!([[0, 1]]));
        assert_eq!(dots(&mut app).len(), 2, "one anchor still shows them");
    }

    /// Hide Corner Widget for angles greater than (#394): a rectangle's 90° corners lose their
    /// widgets below 90°.
    #[test]
    fn corner_widgets_hide_above_the_preference_angle() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        app.session.execute("shape.rectangle", &json!({"x": 100, "y": 100, "width": 100, "height": 100})).unwrap();
        let ctx = egui::Context::default();
        let widgets = |app: &mut VectorcraftApp| {
            shapes(app, &ctx).iter().filter(|s| matches!(s, Shape::Circle(c) if c.radius == 3.0 && c.fill == Color32::WHITE)).count()
        };
        assert_eq!(widgets(&mut app), 4);
        app.session.execute("prefs.set", &json!({"key": "hideCornerWidgetAbove", "value": 80})).unwrap();
        assert_eq!(widgets(&mut app), 0);
    }

    /// General › Zoom with Mouse Wheel (#394), through the control channel's `ui.wheel`: off,
    /// the wheel scrolls and Cmd-wheel zooms; on, the wheel zooms about the pointer, Shift-wheel
    /// scrolls up and down and Cmd/Ctrl-wheel sideways.
    #[test]
    fn zoom_with_mouse_wheel_preference() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let ctx = egui::Context::default();
        // A canvas frame through the host's input hook (it feeds the control channel's input).
        let run = |app: &mut VectorcraftApp| {
            let mut raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), ..Default::default() };
            app.raw_input_hook(&mut raw);
            let mut out = ctx.run_ui(raw, |ui| show(app, ui));
            out.textures_delta.clear();
        };
        run(&mut app);
        let rect = app.canvas_rect.unwrap();
        let at = rect.center() + vec2(60.0, 40.0);
        // One notch up at `at`: → (zoom ratio, scroll in points, how far the point under `at` moved).
        let turn = |app: &mut VectorcraftApp, shift: bool, cmd: bool| {
            let before = *app.view().unwrap();
            crate::tests_synthetic::control(app, &ctx, "ui.wheel", json!({"x": at.x, "y": at.y, "dy": 1, "shift": shift, "cmd": cmd}));
            // egui spreads a notch over a few frames.
            for _ in 0..40 {
                run(app);
            }
            let after = *app.view().unwrap();
            let moved = Xf::new(rect, &after).to_doc(at).distance(Xf::new(rect, &before).to_doc(at));
            (after.zoom / before.zoom, (after.center - before.center) * after.zoom, moved)
        };
        let (zoom, scroll, _) = turn(&mut app, false, false);
        assert!((zoom - 1.0).abs() < 1e-9 && scroll.x == 0.0 && scroll.y < -1.0, "off: the wheel scrolls up ({zoom}, {scroll:?})");
        let (zoom, _, moved) = turn(&mut app, false, true);
        assert!(zoom > 1.01 && moved < 1e-6, "off: Cmd-wheel zooms in about the pointer ({zoom}, {moved})");
        app.session.execute("prefs.set", &json!({"key": "zoomWithMouseWheel", "value": true})).unwrap();
        let (zoom, _, moved) = turn(&mut app, false, false);
        assert!(zoom > 1.01 && moved < 1e-6, "on: the wheel zooms in about the pointer ({zoom}, {moved})");
        let (zoom, scroll, _) = turn(&mut app, true, false);
        assert!((zoom - 1.0).abs() < 1e-9 && scroll.x == 0.0 && scroll.y < -1.0, "on: Shift-wheel scrolls up ({zoom}, {scroll:?})");
        let (zoom, scroll, _) = turn(&mut app, false, true);
        assert!((zoom - 1.0).abs() < 1e-9 && scroll.x < -1.0 && scroll.y == 0.0, "on: Cmd-wheel scrolls sideways ({zoom}, {scroll:?})");
    }

    #[test]
    fn cmd_space_zooms_and_space_pans_whatever_the_tool() {
        use egui::{Event, Modifiers};
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        app.select_tool("rectangle");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        let at = app.canvas_rect.unwrap().center();
        let space = |pressed, modifiers| Event::Key { key: egui::Key::Space, physical_key: None, pressed, repeat: false, modifiers };
        let button = |pos, pressed, modifiers| Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers };
        // A drag from `at` to `to` with Space and `held` down.
        let drag = |app: &mut VectorcraftApp, held: Modifiers, to: Pos2| {
            frame(app, &ctx, vec![Event::ModifiersChanged(held), space(true, held), Event::PointerMoved(at)]);
            frame(app, &ctx, vec![button(at, true, held)]);
            frame(app, &ctx, vec![Event::PointerMoved(to)]);
            frame(app, &ctx, vec![button(to, false, held)]);
            frame(app, &ctx, vec![space(false, held), Event::ModifiersChanged(Modifiers::NONE)]);
        };
        let z0 = app.view().unwrap().zoom;
        drag(&mut app, Modifiers::COMMAND, at);
        let z1 = app.view().unwrap().zoom;
        assert_eq!(z1, crate::state::next_zoom(z0, true), "Cmd+Space click zooms in");
        drag(&mut app, Modifiers::COMMAND | Modifiers::ALT, at);
        assert_eq!(app.view().unwrap().zoom, crate::state::next_zoom(z1, false), "Cmd+Alt+Space click zooms out");
        assert_eq!(app.session.active().unwrap().doc.art_bounds(), None, "the Rectangle tool drew nothing");
        // Space alone pans, with the Zoom tool too.
        app.select_tool("zoom");
        let before = *app.view().unwrap();
        drag(&mut app, Modifiers::NONE, at + vec2(30.0, 0.0));
        let after = *app.view().unwrap();
        assert_eq!(after.zoom, before.zoom);
        assert!((after.center.x - (before.center.x - 30.0 / before.zoom)).abs() < 1e-6, "{before:?} → {after:?}");
    }

    #[test]
    fn cmd_space_drag_scrubs_the_zoom_about_the_point_pressed() {
        use egui::{Event, Modifiers};
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        let rect = app.canvas_rect.unwrap();
        let at = rect.center() + vec2(-60.0, 20.0);
        let space = |pressed, modifiers| Event::Key { key: egui::Key::Space, physical_key: None, pressed, repeat: false, modifiers };
        let button = |pos, pressed, modifiers| Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers };
        let held = Modifiers::COMMAND;
        let doc_at = |app: &VectorcraftApp| {
            let v = app.view().unwrap();
            Xf { rect, zoom: v.zoom, center: v.center, rot: v.rotation.to_radians() }.to_doc(at)
        };
        let z0 = app.view().unwrap().zoom;
        let anchor = doc_at(&app);
        // Press, drag right in steps (the zoom follows), and let go.
        frame(&mut app, &ctx, vec![Event::ModifiersChanged(held), space(true, held), Event::PointerMoved(at)]);
        frame(&mut app, &ctx, vec![button(at, true, held)]);
        frame(&mut app, &ctx, vec![Event::PointerMoved(at + vec2(75.0, 0.0))]);
        let half = app.view().unwrap().zoom;
        assert!((half / z0 - 2f64.sqrt()).abs() < 0.01, "{z0} → {half}");
        frame(&mut app, &ctx, vec![Event::PointerMoved(at + vec2(150.0, 5.0))]);
        frame(&mut app, &ctx, vec![button(at + vec2(150.0, 5.0), false, held)]);
        frame(&mut app, &ctx, vec![space(false, held), Event::ModifiersChanged(Modifiers::NONE)]);
        let z1 = app.view().unwrap().zoom;
        assert!((z1 / z0 - 2.0).abs() < 0.01, "150 pt right doubles the zoom: {z0} → {z1}");
        let still = doc_at(&app);
        assert!((still.x - anchor.x).abs() < 1e-6 && (still.y - anchor.y).abs() < 1e-6, "the point pressed stays put");
        // Left zooms out; the zoom box comes back with Animated Zoom off.
        assert_eq!(scrub_zoom(4.0, -300.0), 1.0);
        assert_eq!(scrub_zoom(1.0, f64::NAN), 1.0);
        app.session.execute("prefs.set", &json!({"key": "animatedZoom", "value": false})).unwrap();
        frame(&mut app, &ctx, vec![Event::ModifiersChanged(held), space(true, held), Event::PointerMoved(at)]);
        frame(&mut app, &ctx, vec![button(at, true, held)]);
        frame(&mut app, &ctx, vec![Event::PointerMoved(at + vec2(75.0, 0.0))]);
        assert_eq!(app.view().unwrap().zoom, z1, "a zoom box doesn't zoom until it's let go");
        frame(&mut app, &ctx, vec![button(at + vec2(75.0, 0.0), false, held), space(false, held), Event::ModifiersChanged(Modifiers::NONE)]);
    }

    #[test]
    fn space_pressed_mid_drag_moves_what_the_tool_draws() {
        use egui::{Event, Modifiers};
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        app.ui.view.smart_guides = false;
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        let xf = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap());
        let space = |pressed| Event::Key { key: egui::Key::Space, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
        let button = |pos, pressed| Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        // Press at `a`, drag to `b`, then hold Space while moving on by `by` and let go of both.
        let draw = |app: &mut VectorcraftApp, a: Pos2, b: Pos2, by: egui::Vec2| {
            frame(app, &ctx, vec![Event::PointerMoved(a)]);
            frame(app, &ctx, vec![button(a, true)]);
            frame(app, &ctx, vec![Event::PointerMoved(b)]);
            frame(app, &ctx, vec![space(true)]);
            frame(app, &ctx, vec![Event::PointerMoved(b + by)]);
            frame(app, &ctx, vec![space(false)]);
            frame(app, &ctx, vec![button(b + by, false)]);
        };
        let near = |p: Point, q: Point| p.distance(q) < 1e-3;
        let at = app.canvas_rect.unwrap().center() - vec2(100.0, 60.0);
        let center = app.view().unwrap().center;
        // The rectangle moves at its size instead of growing, and the view doesn't pan.
        app.select_tool("rectangle");
        draw(&mut app, at, at + vec2(80.0, 40.0), vec2(30.0, 20.0));
        assert_eq!(app.view().unwrap().center, center, "Space during a drag doesn't pan");
        let st = app.session.active().unwrap();
        let r = st.doc.node(st.selection.objects[0]).unwrap().path_data().unwrap().bounds().unwrap();
        assert!(
            near(Point::new(r.x0, r.y0), xf.to_doc(at + vec2(30.0, 20.0))) && near(Point::new(r.x1, r.y1), xf.to_doc(at + vec2(110.0, 60.0))),
            "{r:?}"
        );
        // The Pen's anchor being dragged out moves, handles and all.
        app.session.execute("select.none", &json!({})).unwrap();
        app.select_tool("pen");
        let a = at + vec2(0.0, 120.0);
        draw(&mut app, a, a + vec2(40.0, 0.0), vec2(20.0, 10.0));
        let st = app.session.active().unwrap();
        let anchor = st.doc.node(st.selection.objects[0]).unwrap().path_data().unwrap().subpaths[0].anchors[0];
        assert!(near(anchor.p, xf.to_doc(a + vec2(20.0, 10.0))) && near(anchor.h_out, xf.to_doc(a + vec2(60.0, 10.0))), "{anchor:?}");
    }

    #[test]
    fn double_clicking_type_with_the_selection_tool_puts_the_caret_there() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let id = app.session.execute("text.create", &json!({"x": 100, "y": 150, "text": "Hello world", "size": 24})).unwrap()["id"].as_u64().unwrap();
        app.session.execute("select.none", &json!({})).unwrap();
        app.select_tool("selection");
        let ctx = egui::Context::default();
        let timed = |app: &mut VectorcraftApp, time: f64, events: Vec<egui::Event>| {
            let screen = egui::Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
            let mut out =
                ctx.run_ui(egui::RawInput { screen_rect: Some(screen), time: Some(time), events, ..Default::default() }, |ui| show(app, ui));
            out.textures_delta.clear();
        };
        timed(&mut app, 0.0, vec![]);
        let xf = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap());
        let text = |app: &VectorcraftApp| match &app.session.active().unwrap().doc.node(vectorcraft_doc::NodeId(id)).unwrap().kind {
            NodeKind::Text(t) => t.plain_text(),
            _ => panic!("not text"),
        };
        // Double-click the middle of the text.
        let b = app.session.active().unwrap().doc.node(vectorcraft_doc::NodeId(id)).unwrap().geometric_bounds().unwrap();
        let at = xf.to_screen(b.center());
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        timed(&mut app, 1.0, vec![egui::Event::PointerMoved(at), button(true), button(false), button(true), button(false)]);
        timed(&mut app, 1.1, vec![]);
        // The Type tool edits the text, with the caret inside it, where it was clicked: typing goes
        // into the text, not to tool shortcuts (X would swap fill and stroke).
        assert_eq!(app.session.tool_id(), "type");
        assert!(app.session.tool_wants_text());
        let o = app.session.tool_options();
        assert_eq!(o["editing"], json!(id));
        let caret = o["caret"].as_u64().unwrap();
        assert!((1..11).contains(&caret), "caret {caret}");
        app.session.tool_text("X", app.view_info()).unwrap();
        let t = text(&app);
        assert_eq!((t.len(), t.find('X')), (12, Some(caret as usize)), "{t}");
    }

    #[test]
    fn a_symbol_dropped_on_the_canvas_is_placed_there() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        app.session.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 40, "height": 20})).unwrap();
        let name = app.session.execute("symbol.new", &json!({})).unwrap()["name"].as_str().unwrap().to_string();
        app.session.execute("select.none", &json!({})).unwrap();
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        let xf = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap());
        // Released over empty canvas: an instance centred there.
        let at = xf.to_screen(Point::new(250.0, 180.0));
        egui::DragAndDrop::set_payload(&ctx, widgets::PanelDrag::Symbol(name.clone()));
        let up = egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() };
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(at), up]);
        let st = app.session.active().unwrap();
        let placed = st.selection.objects.first().copied().unwrap();
        let n = st.doc.node(placed).unwrap();
        assert!(matches!(&n.kind, NodeKind::SymbolInstance { symbol, .. } if *symbol == name));
        let c = n.geometric_bounds().unwrap().center();
        assert!((c - xf.to_doc(at)).hypot() < 1e-6, "{c:?}");
    }

    #[test]
    fn a_brush_dropped_on_a_path_is_applied_to_it() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let id =
            app.session.execute("path.create", &json!({"anchors": [{"x": 100, "y": 150}, {"x": 300, "y": 150}]})).unwrap()["id"].as_u64().unwrap();
        app.session.execute("select.none", &json!({})).unwrap();
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        let xf = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap());
        let brush = |app: &VectorcraftApp| {
            let st = app.session.active().unwrap();
            st.doc.node(vectorcraft_doc::NodeId(id)).unwrap().appearance.stroke().and_then(|s| s.brush.clone())
        };
        let drop = |app: &mut VectorcraftApp, at: Point| {
            let at = xf.to_screen(at);
            egui::DragAndDrop::set_payload(&ctx, widgets::PanelDrag::Brush { name: "Arrow".into(), def: json!({}) });
            let up = egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() };
            frame(app, &ctx, vec![egui::Event::PointerMoved(at), up]);
        };
        // Off the path: nothing.
        drop(&mut app, Point::new(200.0, 250.0));
        assert_eq!(brush(&app), None);
        // On it: the path's stroke takes the brush.
        drop(&mut app, Point::new(200.0, 150.0));
        assert_eq!(brush(&app).as_deref(), Some("Arrow"));
    }

    #[test]
    fn middle_drag_pans_the_view_with_any_tool() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        // A drawing tool: the middle button must pan, never draw.
        app.select_tool("rectangle");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos2(400.0, 300.0))]);
        let before = *app.view().unwrap();
        frame(&mut app, &ctx, vec![middle(pos2(400.0, 300.0), true)]);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos2(450.0, 330.0))]);
        frame(&mut app, &ctx, vec![middle(pos2(450.0, 330.0), false)]);
        let after = *app.view().unwrap();
        assert_eq!(after.zoom, before.zoom);
        assert!((after.center.x - (before.center.x - 50.0 / before.zoom)).abs() < 1e-6);
        assert!((after.center.y - (before.center.y - 30.0 / before.zoom)).abs() < 1e-6);
        // Released: moving no longer pans, and nothing was drawn.
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos2(300.0, 200.0))]);
        assert_eq!(app.view().unwrap().center, after.center);
        assert_eq!(app.session.active().unwrap().doc.art_bounds(), None);
    }

    #[test]
    fn swatches_and_proxy_paints_dropped_on_art_fill_the_object_hit() {
        use vectorcraft_color::{Color, GradientGeom, GradientPaint, Paint};
        use widgets::{PanelDrag, SwatchRows};
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let id = app.session.execute("shape.rectangle", &json!({"x": 50, "y": 50, "width": 100, "height": 100})).unwrap()["id"].as_u64().unwrap();
        app.session.execute("select.none", &json!({})).unwrap();
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        let xf = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap());
        let drop = |app: &mut VectorcraftApp, d: PanelDrag, at: Point| {
            let at = xf.to_screen(at);
            egui::DragAndDrop::set_payload(&ctx, d);
            let up = egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() };
            frame(app, &ctx, vec![egui::Event::PointerMoved(at), up]);
        };
        let fill = |app: &VectorcraftApp| app.session.active().unwrap().doc.node(vectorcraft_doc::NodeId(id)).unwrap().appearance.fill_paint();
        let red = Paint::solid(Color::from_hex("#ed1c24").unwrap());
        let rows = SwatchRows { grabbed: "Red".into(), names: vec!["Red".into()], groups: false };
        let swatch = PanelDrag::Paint { paint: red.clone(), params: json!({"swatch": "Red"}), rows: Some(rows) };
        // Off the art nothing happens.
        drop(&mut app, swatch.clone(), Point::new(300.0, 250.0));
        assert_eq!(fill(&app), Paint::solid(Color::WHITE));
        drop(&mut app, swatch, Point::new(100.0, 100.0));
        assert_eq!(fill(&app), red, "paint.setFill with the hit id");
        assert!(app.session.active().unwrap().selection.is_empty(), "the selection stays as it was");
        // A proxy's paint works the same.
        drop(&mut app, PanelDrag::paint(Paint::solid(Color::rgb(0.0, 0.0, 1.0))), Point::new(60.0, 60.0));
        assert_eq!(fill(&app), Paint::solid(Color::rgb(0.0, 0.0, 1.0)));
        // A dragged gradient (the Gradient panel's thumbnail, a proxy) fits the object it lands on.
        let mut g = GradientPaint::new(Default::default());
        g.geom = Some(GradientGeom { start: Point::new(0.0, 0.0), end: Point::new(10.0, 0.0), aspect: 1.0, focal: None });
        drop(&mut app, PanelDrag::paint(Paint::Gradient(Box::new(g))), Point::new(100.0, 100.0));
        assert!(matches!(fill(&app), Paint::Gradient(g) if g.geom.is_none()));
    }

    #[test]
    fn dropping_the_appearance_thumbnail_on_art_copies_the_appearance() {
        use vectorcraft_doc::NodeId;
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        let run = |app: &mut VectorcraftApp, id: &str, p: serde_json::Value| app.session.execute(id, &p).unwrap();
        run(&mut app, "file.new", json!({"width": 400, "height": 300}));
        let a = run(&mut app, "shape.rectangle", json!({"x": 20, "y": 20, "width": 100, "height": 100}))["id"].as_u64().unwrap();
        run(&mut app, "effect.apply", json!({"effect": "distort.twist"}));
        run(&mut app, "transparency.set", json!({"opacity": 40}));
        let b = run(&mut app, "shape.rectangle", json!({"x": 200, "y": 20, "width": 100, "height": 100}))["id"].as_u64().unwrap();
        run(&mut app, "select.set", json!({ "ids": [a] }));
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        // The Appearance panel's thumbnail (its drag payload) released over the second rectangle.
        let xf = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap());
        let drop = |app: &mut VectorcraftApp, at: Point| {
            let at = xf.to_screen(at);
            egui::DragAndDrop::set_payload(&ctx, widgets::PanelDrag::Appearance(NodeId(a)));
            frame(app, &ctx, vec![egui::Event::PointerMoved(at)]);
            let up = egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() };
            frame(app, &ctx, vec![up]);
        };
        drop(&mut app, Point::new(250.0, 70.0));
        let doc = &app.session.active().unwrap().doc;
        let (na, nb) = (doc.node(NodeId(a)).unwrap(), doc.node(NodeId(b)).unwrap());
        assert_eq!((&nb.appearance, nb.opacity), (&na.appearance, na.opacity));
        assert_eq!(nb.appearance.effects[0].id, "distort.twist");
        // Dropped on empty canvas or on the source itself: nothing changes.
        let undo_len = app.session.active().unwrap().history.undo.len();
        drop(&mut app, Point::new(350.0, 250.0));
        drop(&mut app, Point::new(70.0, 70.0));
        assert_eq!(app.session.active().unwrap().history.undo.len(), undo_len);
    }

    #[test]
    fn a_graphic_style_dropped_on_art_applies_to_the_object_hit() {
        use vectorcraft_doc::NodeId;
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        let run = |app: &mut VectorcraftApp, id: &str, p: serde_json::Value| app.session.execute(id, &p).unwrap();
        run(&mut app, "file.new", json!({"width": 400, "height": 300}));
        let a = NodeId(run(&mut app, "shape.rectangle", json!({"x": 20, "y": 20, "width": 100, "height": 100}))["id"].as_u64().unwrap());
        let b = NodeId(run(&mut app, "shape.rectangle", json!({"x": 200, "y": 20, "width": 100, "height": 100}))["id"].as_u64().unwrap());
        run(&mut app, "select.set", json!({ "ids": [a.0] }));
        let name = app.session.active().unwrap().doc.graphic_styles[2].name.clone();
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        let xf = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap());
        let at = xf.to_screen(Point::new(250.0, 70.0));
        egui::DragAndDrop::set_payload(&ctx, widgets::PanelDrag::GraphicStyle(name.clone()));
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(at)]);
        let up = egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() };
        frame(&mut app, &ctx, vec![up]);
        let st = app.session.active().unwrap();
        let g = st.doc.graphic_style(&name).unwrap();
        // `graphicStyle.apply` with the hit object's id: linked, the selection left alone.
        assert_eq!(st.doc.node(b).unwrap().graphic_style, Some(g.id));
        assert_eq!(st.doc.node(b).unwrap().appearance, g.appearance);
        assert_eq!(st.doc.node(a).unwrap().graphic_style, None);
        assert_eq!(st.selection.objects, [a]);
    }

    #[test]
    fn art_moved_off_the_canvas_becomes_a_panel_drag_and_stays_put() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let id = app.session.execute("shape.rectangle", &json!({"x": 50, "y": 50, "width": 100, "height": 100})).unwrap()["id"].as_u64().unwrap();
        let id = vectorcraft_doc::NodeId(id);
        app.select_tool("selection");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        let xf = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap());
        let before = app.session.active().unwrap().doc.clone();
        let undo = app.session.active().unwrap().history.undo.len();
        let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let start = xf.to_screen(Point::new(100.0, 100.0));
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(start), button(start, true)]);
        // Moving on the canvas moves the art...
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(start + vec2(40.0, 0.0))]);
        assert!(app.session.active().unwrap().interaction.is_some());
        // ...until the pointer leaves it: the move is dropped and the panels get the art.
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos2(850.0, 100.0))]);
        assert!(app.session.active().unwrap().interaction.is_none());
        assert_eq!(*app.session.active().unwrap().doc, *before, "the art is back where it was");
        assert_eq!(egui::DragAndDrop::payload::<widgets::PanelDrag>(&ctx).as_deref(), Some(&widgets::PanelDrag::Art(vec![id])));
        assert!(!app.session.tool_busy());
        // Released anywhere, nothing moves and nothing is recorded.
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(start), button(start, false)]);
        let st = app.session.active().unwrap();
        assert_eq!(*st.doc, *before);
        assert_eq!(st.history.undo.len(), undo);
    }

    /// One frame of keyboard handling and canvas, as the app runs them; returns the IME output.
    fn typing_frame(app: &mut VectorcraftApp, ctx: &egui::Context, events: Vec<egui::Event>) -> Option<egui::output::IMEOutput> {
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), events, ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| {
            crate::shortcuts::handle(app, ui.ctx());
            show(app, ui);
        });
        out.textures_delta.clear();
        out.platform_output.ime
    }

    fn preedit(t: &str, chars: usize) -> egui::Event {
        egui::Event::Ime(egui::ImeEvent::Preedit { text: t.into(), active_range_chars: Some(chars..chars) })
    }

    fn plain(app: &VectorcraftApp, id: u64) -> String {
        match &app.session.active().unwrap().doc.node(vectorcraft_doc::NodeId(id)).unwrap().kind {
            NodeKind::Text(t) => t.plain_text(),
            _ => panic!("not text"),
        }
    }

    #[test]
    fn japanese_ime_composes_on_the_canvas_with_its_window_at_the_caret() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        // The type placed starts empty (Fill New Type Objects With Placeholder Text off).
        app.session.prefs.placeholder_text = false;
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let ctx = egui::Context::default();
        // Not editing: the IME stays off (single-key tool shortcuts keep working).
        assert!(typing_frame(&mut app, &ctx, vec![]).is_none());
        app.select_tool("type");
        let xf = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap());
        let at = xf.to_screen(Point::new(100.0, 100.0));
        let click =
            |p: Pos2, pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        typing_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(at), click(at, true)]);
        typing_frame(&mut app, &ctx, vec![click(at, false)]);
        assert!(app.session.tool_wants_text());
        let id = app.session.active().unwrap().selection.objects[0].0;
        // Editing: the IME is allowed, its window at the caret (the text's baseline at y = 100).
        let ime = typing_frame(&mut app, &ctx, vec![]).expect("IME allowed while editing");
        assert!(ime.rect.contains(pos2(ime.rect.center().x, at.y)) && (ime.rect.center().x - at.x).abs() < 2.0, "{:?} vs {at:?}", ime.rect);
        assert!(!ime.should_interrupt_composition);
        // gagaku → がが → 雅楽, then commit (the macOS sequence).
        typing_frame(&mut app, &ctx, vec![preedit("g", 1)]);
        typing_frame(&mut app, &ctx, vec![preedit("が", 1), preedit("がg", 2)]);
        typing_frame(&mut app, &ctx, vec![preedit("ががく", 3)]);
        assert_eq!(plain(&app, id), "ががく");
        // While composing: plain text events, the clipboard and Undo (the native menu's ⌘Z
        // reaches `menus::invoke`) all wait.
        typing_frame(&mut app, &ctx, vec![egui::Event::Text("x".into()), egui::Event::Paste("y".into())]);
        crate::menus::invoke(&mut app, "edit.undo", json!({}));
        assert!(!crate::menus::enabled(&app, "edit.undo"));
        assert_eq!(plain(&app, id), "ががく");
        let ime2 =
            typing_frame(&mut app, &ctx, vec![egui::Event::Ime(egui::ImeEvent::Preedit { text: "雅楽".into(), active_range_chars: Some(0..2) })])
                .unwrap();
        assert!(ime2.rect.center().x <= ime.rect.center().x + 1.0, "the window stays at the clause being converted");
        typing_frame(&mut app, &ctx, vec![preedit("", 0), egui::Event::Ime(egui::ImeEvent::Commit("雅楽".into()))]);
        assert_eq!(plain(&app, id), "雅楽");
        assert!(!app.session.tool_composing());
        // After the commit, text and Undo work again.
        typing_frame(&mut app, &ctx, vec![egui::Event::Text("!".into())]);
        assert_eq!(plain(&app, id), "雅楽!");
        crate::menus::invoke(&mut app, "edit.undo", json!({}));
        assert_eq!(plain(&app, id), "");
    }

    #[test]
    fn clicking_away_mid_composition_keeps_the_text_and_interrupts_the_ime() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let ctx = egui::Context::default();
        typing_frame(&mut app, &ctx, vec![]);
        app.select_tool("type");
        let xf = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap());
        let (a, b) = (xf.to_screen(Point::new(100.0, 100.0)), xf.to_screen(Point::new(300.0, 250.0)));
        let click =
            |p: Pos2, pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        typing_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(a), click(a, true)]);
        typing_frame(&mut app, &ctx, vec![click(a, false)]);
        let id = app.session.active().unwrap().selection.objects[0].0;
        typing_frame(&mut app, &ctx, vec![preedit("しょうこ", 4)]);
        // macOS sends nothing for a click away; the tool keeps the marked text and the IME is
        // told to drop its composition.
        typing_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(b), click(b, true)]);
        let ime = typing_frame(&mut app, &ctx, vec![click(b, false)]);
        assert_eq!(plain(&app, id), "しょうこ");
        assert!(!app.session.tool_composing());
        assert!(ime.expect("editing the new text").should_interrupt_composition);
        assert!(app.ime_marked.is_none());
        assert!(app.take_ime_discard(), "the host tells the system IME to drop its marked text");
        // Interrupted once, not every frame.
        assert!(!typing_frame(&mut app, &ctx, vec![]).unwrap().should_interrupt_composition);
        assert!(!app.take_ime_discard());
    }

    #[test]
    fn ime_edge_orders_never_eat_committed_text_or_lose_the_composition() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let ctx = egui::Context::default();
        typing_frame(&mut app, &ctx, vec![]);
        app.select_tool("type");
        let xf = Xf::new(app.canvas_rect.unwrap(), app.view().unwrap());
        let at = xf.to_screen(Point::new(100.0, 100.0));
        let click =
            |p: Pos2, pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        typing_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(at), click(at, true)]);
        typing_frame(&mut app, &ctx, vec![click(at, false)]);
        let id = app.session.active().unwrap().selection.objects[0].0;
        typing_frame(&mut app, &ctx, vec![egui::Event::Text("雅".into())]);
        let backspace =
            |pressed| egui::Event::Key { key: egui::Key::Backspace, physical_key: None, pressed, repeat: false, modifiers: Default::default() };
        // The IME empties its marked text and the Backspace comes in the same frame.
        typing_frame(&mut app, &ctx, vec![preedit("が", 1)]);
        typing_frame(&mut app, &ctx, vec![preedit("", 0), backspace(true), backspace(false)]);
        assert_eq!(plain(&app, id), "雅", "the committed character stays");
        // A bare line-break commit ends the composition with the marked text, no newline.
        typing_frame(&mut app, &ctx, vec![preedit("がく", 2)]);
        typing_frame(&mut app, &ctx, vec![egui::Event::Ime(egui::ImeEvent::Commit("\n".into()))]);
        assert!(!app.session.tool_composing());
        assert_eq!(plain(&app, id), "雅がく");
        assert!(app.take_ime_discard());
        // Switching apps mid-composition: the composition goes on when the window comes back.
        typing_frame(&mut app, &ctx, vec![preedit("らく", 2)]);
        typing_frame(&mut app, &ctx, vec![egui::Event::WindowFocused(false)]);
        typing_frame(&mut app, &ctx, vec![egui::Event::WindowFocused(true), preedit("らくか", 3)]);
        assert!(app.session.tool_composing());
        assert!(!app.take_ime_discard());
        assert_eq!(plain(&app, id), "雅がくらくか");
    }
}
