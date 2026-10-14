//! The right dock: Properties | Layers | Libraries tabs, plus the collapsed icon-panel column
//! whose panels pop out to the left. The double arrow at the top of the dock collapses the tabbed
//! group to icons at the top of the icon column (`window.collapseDock`); the arrow above the column
//! then expands it again.

use egui::{CornerRadius, Rect, Sense, Stroke, Ui, vec2};
use serde_json::json;

use crate::state::{DockTab, ICON_PANEL_GROUPS, ICON_PANELS};
use crate::theme::{self, Tokens};
use crate::{VectorcraftApp, icons, panels, widgets};

const ICON_COL: f32 = 38.0;
/// Height of the strip along the top of a dock column that carries its double arrow.
const HEADER: f32 = 14.0;
/// The tabbed group's width until the dock has been laid out (its default size).
const DOCK_WIDTH: f32 = 300.0;
/// Top of a popped-out panel.
const FLYOUT_TOP: f32 = 110.0;

/// Where the icon column's left edge was drawn last (egui temp memory, one frame old).
fn column_left_id() -> egui::Id {
    egui::Id::new("dock-icon-column-left")
}

/// Collapse the tabbed group to icons or expand it again (`window.collapseDock`). Expanding while
/// one of its panels is popped out shows that panel's tab instead.
pub fn set_collapsed(app: &mut VectorcraftApp, collapsed: bool) {
    app.ui.dock_collapsed = collapsed;
    app.ui.dock = true;
    if !collapsed && let Some(tab) = app.ui.open_panel.as_deref().and_then(DockTab::from_id) {
        app.ui.dock_tab = tab;
        app.ui.open_panel = None;
    }
}

/// The strip along the top of a dock column with its double arrow: » collapses the dock to icons,
/// « expands it. The whole strip is the click target (as on the toolbar). True when clicked.
fn collapse_header(ui: &mut Ui, collapsed: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (hdr, resp) = ui.allocate_exact_size(vec2(ui.available_width(), HEADER), Sense::click());
    ui.painter().rect_filled(hdr, 0.0, t.tab_strip);
    let arrow = Rect::from_min_size(hdr.right_top() + vec2(-16.0, 1.0), vec2(12.0, 12.0));
    if resp.hovered() {
        ui.painter().rect_filled(arrow, CornerRadius::same(2), t.hover);
    }
    let icon = if collapsed { "chevrons-left" } else { "chevrons-right" };
    icons::paint(ui, icon, arrow.shrink(1.0), if resp.hovered() { t.text_strong } else { t.text });
    resp.on_hover_text(if collapsed { tl!("Expand Panels") } else { tl!("Collapse to Icons") }).clicked()
}

/// Run `window.collapseDock` for a click on a double arrow.
fn toggle(app: &mut VectorcraftApp, collapsed: bool) {
    // It only fails on bad params, which this never sends; show it all the same.
    if let Err(e) = app.run("window.collapseDock", json!({ "collapsed": collapsed })) {
        app.ui.status = e;
    }
}

/// One icon of the column: a click pops its panel out (or puts it away).
fn panel_icon(app: &mut VectorcraftApp, ui: &mut Ui, id: &str, label: &str, icon: &str) {
    let open = app.ui.open_panel.as_deref() == Some(id);
    if widgets::icon_button(ui, icon, label, open, 30.0).clicked() {
        app.ui.open_panel = if open { None } else { Some(id.to_string()) };
    }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    // Main tabbed group, unless it is collapsed to icons.
    if !app.ui.dock_collapsed {
        egui::Panel::right("dock")
            .resizable(true)
            .default_size(DOCK_WIDTH)
            .size_range(230.0..=520.0)
            .frame(egui::Frame::NONE.fill(t.panel).stroke(Stroke::new(1.5, t.border)))
            .show(ui, |ui| {
                if collapse_header(ui, false) {
                    toggle(app, true);
                }
                // Tab strip.
                let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), 33.0), Sense::hover());
                ui.painter().rect_filled(strip, 0.0, t.tab_strip);
                ui.painter().line_segment([strip.left_bottom(), strip.right_bottom()], Stroke::new(1.0, t.border));
                let mut x = strip.left();
                for tab in crate::embroidery_workspace::dock_tabs(app) {
                    let (_, label, _) = tab.info();
                    let active = app.ui.dock_tab == tab;
                    let galley =
                        ui.painter().layout_no_wrap(tl!(label).to_string(), theme::semibold(12.5), if active { t.text_strong } else { t.text_dim });
                    let r = egui::Rect::from_min_size(egui::pos2(x, strip.top()), vec2(galley.size().x + 24.0, strip.height() - 1.0));
                    let resp = ui.interact(r, ui.id().with(("docktab", label)), Sense::click());
                    if active {
                        ui.painter().rect_filled(r, 0.0, t.panel);
                    }
                    ui.painter().galley(egui::pos2(r.left() + 12.0, r.center().y - galley.size().y / 2.0), galley, t.text);
                    ui.painter().line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0, t.border));
                    if resp.clicked() {
                        app.ui.dock_tab = tab;
                    }
                    x = r.right();
                }
                let menu_id = app.ui.dock_tab.info().0;
                panels::panel_menu(app, ui, menu_id, egui::Rect::from_center_size(strip.right_center() - vec2(14.0, 0.0), vec2(16.0, 16.0)));
                egui::Frame::NONE.inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| match app.ui.dock_tab {
                    DockTab::Properties => {
                        egui::ScrollArea::vertical().id_salt("props").auto_shrink([false, false]).show(ui, |ui| panels::properties::show(app, ui));
                    }
                    DockTab::Layers => panels::layers::show(app, ui),
                    DockTab::Sequencer => panels::sequencer::show(app, ui),
                    DockTab::Libraries => panels::libraries(app, ui),
                });
            });
    }
    // Collapsed icon-panel strip, left of the expanded panel group (like Illustrator's dock). A
    // collapsed group's panels head it, under the « that expands them again.
    let collapsed = app.ui.dock_collapsed;
    let column = egui::Panel::right("icon_column")
        .resizable(false)
        .exact_size(ICON_COL)
        .frame(egui::Frame::NONE.fill(t.panel).stroke(Stroke::new(1.5, t.border)))
        .show(ui, |ui| {
            if collapsed && collapse_header(ui, true) {
                toggle(app, false);
            }
            egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 6)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                // The icons scroll in a window too short for them.
                widgets::strip_scroll(ui, "icon_column", |ui| {
                    if collapsed {
                        for tab in crate::embroidery_workspace::dock_tabs(app) {
                            let (id, label, icon) = tab.info();
                            panel_icon(app, ui, id, label, icon);
                        }
                    }
                    let groups: Vec<Vec<&str>> = ICON_PANEL_GROUPS
                        .iter()
                        .map(|g| g.iter().copied().filter(|id| !crate::embroidery_workspace::hides_panel(app, id)).collect())
                        .filter(|g: &Vec<&str>| !g.is_empty())
                        .collect();
                    for (gi, group) in groups.into_iter().enumerate() {
                        if gi > 0 || collapsed {
                            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 7.0), Sense::hover());
                            ui.painter()
                                .line_segment([r.left_center() + vec2(4.0, 0.0), r.right_center() - vec2(4.0, 0.0)], Stroke::new(1.0, t.divider));
                        }
                        for id in group {
                            let Some((_, label, icon)) = ICON_PANELS.iter().find(|p| p.0 == id) else { continue };
                            panel_icon(app, ui, id, label, icon);
                        }
                    }
                });
            });
        });
    let left = column.response.rect.left();
    ui.ctx().data_mut(|d| d.insert_temp(column_left_id(), left));
}

/// A panel popped out next to the icon column: an icon panel, or one of the tabbed group's panels
/// while the dock is collapsed.
pub fn floating_panel(app: &mut VectorcraftApp, ctx: &egui::Context) {
    let Some(id) = app.ui.open_panel.clone() else { return };
    let tab = DockTab::from_id(&id);
    let label = match tab {
        Some(tab) if app.ui.dock_collapsed => tab.info().1,
        // Expanded, the group shows its panels in the dock itself.
        Some(_) => return,
        None => match ICON_PANELS.iter().find(|p| p.0 == id) {
            Some((_, label, _)) => *label,
            None => return,
        },
    };
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    // Pinned by its right edge, 6 points left of the icon column wherever the dock's width (or its
    // collapsed state) put it: content wider than the flyout grows it towards the canvas rather
    // than over the panel icons. Without a dock on screen it keeps to the window's right edge.
    let column = if app.ui.dock && app.ui.screen_mode < 3 {
        ctx.data(|d| d.get_temp::<f32>(column_left_id()))
            .filter(|x| x.is_finite() && *x > screen.left() && *x <= screen.right())
            .unwrap_or_else(|| screen.right() - ICON_COL - if app.ui.dock_collapsed { 0.0 } else { DOCK_WIDTH })
    } else {
        screen.right()
    };
    let right = column - 6.0;
    // The tabbed group's panels fill their height (Layers) or scroll (Properties): give them one
    // that keeps the flyout on screen.
    // The tabbed group's panels keep the width the dock gives them; icon panels are 256 points.
    let width = if tab.is_some() { DOCK_WIDTH } else { 256.0 };
    let tall = (screen.height() - FLYOUT_TOP - 26.0 - 20.0 - 40.0).clamp(160.0, 560.0);
    let mut open = true;
    let area = egui::Area::new(egui::Id::new("icon-panel")).order(egui::Order::Foreground).pivot(egui::Align2::RIGHT_TOP);
    area.fixed_pos(egui::pos2(right, FLYOUT_TOP)).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).fill(t.panel).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin::ZERO).show(ui, |ui| {
            ui.set_width(width);
            let (strip, _) = ui.allocate_exact_size(vec2(width, 26.0), Sense::hover());
            ui.painter().rect_filled(strip, CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, t.panel_darker);
            let title = egui::Rect::from_min_size(
                strip.min,
                vec2(ui.painter().layout_no_wrap(tl!(label).to_string(), theme::semibold(12.0), t.text).size().x + 24.0, 26.0),
            );
            ui.painter().rect_filled(title, CornerRadius { nw: 4, ne: 0, sw: 0, se: 0 }, t.panel);
            ui.painter().text(title.left_center() + vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, tl!(label), theme::semibold(12.0), t.text);
            let close = egui::Rect::from_center_size(strip.right_center() - vec2(13.0, 0.0), vec2(14.0, 14.0));
            let cr = ui.interact(close, ui.id().with("close-panel"), Sense::click());
            icons::paint(ui, "chevrons-right", close, if cr.hovered() { t.text } else { t.text_dim });
            if cr.clicked() {
                open = false;
            }
            let menu_r = egui::Rect::from_center_size(strip.right_center() - vec2(34.0, 0.0), vec2(16.0, 16.0));
            panels::panel_menu(app, ui, &id, menu_r);
            egui::Frame::NONE.inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.set_width(width - 20.0);
                match tab {
                    Some(DockTab::Properties) => {
                        // At least `tall` when the sections are taller: the flyout's area keeps the
                        // size its content needs, and egui sizes a new area at most 400 points high,
                        // so a scroll area that only took the room it was given never grew past it.
                        egui::ScrollArea::vertical()
                            .id_salt("props-flyout")
                            .max_height(tall)
                            .min_scrolled_height(tall)
                            .auto_shrink([false, true])
                            .show(ui, |ui| panels::properties::show(app, ui));
                    }
                    Some(DockTab::Layers) => {
                        ui.set_max_height(tall);
                        panels::layers::show(app, ui);
                    }
                    Some(DockTab::Sequencer) => {
                        ui.set_max_height(tall);
                        panels::sequencer::show(app, ui);
                    }
                    Some(DockTab::Libraries) => panels::libraries(app, ui),
                    None => panels::show_icon_panel(app, ui, &id),
                }
            });
        });
    });
    if !open {
        app.ui.open_panel = None;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vectorcraft_engine::Session;

    use super::*;
    use crate::toolbar::tests::{wheel, widget_rects};

    #[test]
    fn every_flyout_stays_left_of_the_icon_column() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
        let id = app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 50, "height": 40})).unwrap()["id"].clone();
        app.run("select.set", json!({ "ids": [id] })).unwrap();
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1600.0, 1200.0));
        // Where the icon column begins: the flyout must stay left of it.
        let column = screen.right() - ICON_COL - 300.0;
        let mut too_wide = vec![];
        for (panel, _, _) in crate::state::ICON_PANELS {
            app.ui.open_panel = Some(panel.to_string());
            // Two frames: the first lays the panel out, the second places the settled area.
            for _ in 0..2 {
                let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
                ctx.run_ui(input, |ui| floating_panel(&mut app, ui.ctx())).textures_delta.clear();
            }
            let rect = ctx.memory(|m| m.area_rect(egui::Id::new("icon-panel"))).unwrap();
            if rect.right() > column {
                too_wide.push(format!("{panel}: {:.0} past the column", rect.right() - column));
            }
        }
        assert!(too_wide.is_empty(), "flyouts reaching over the icon column: {too_wide:?}");
    }

    #[test]
    fn the_icon_column_scrolls_in_a_short_window() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut frame = |time: f64, events| {
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 400.0));
            let input = egui::RawInput { time: Some(time), events, screen_rect: Some(screen), ..Default::default() };
            ctx.run_ui(input, |ui| show(&mut app, ui)).textures_delta.clear();
            widget_rects(&ctx, vec2(30.0, 30.0))
        };
        let icons = frame(0.0, vec![]);
        assert!(icons.last().unwrap().bottom() > 400.0, "the last panel icon starts below the window");
        frame(0.1, wheel(icons[0].center(), 2000.0));
        let mut icons = vec![];
        for k in 2..40 {
            icons = frame(f64::from(k) * 0.1, vec![]);
        }
        assert!(icons.last().unwrap().bottom() <= 400.0, "scrolled into view: {:?}", icons.last());
    }

    const SCREEN: egui::Vec2 = vec2(1600.0, 1000.0);

    /// A frame of the dock and its flyout on a 1600 × 1000 window.
    struct Harness {
        app: VectorcraftApp,
        ctx: egui::Context,
        time: f64,
    }

    impl Harness {
        fn new() -> Self {
            let mut app = VectorcraftApp::new(Session::new(), Default::default());
            app.run("file.new", json!({"width": 300, "height": 300})).unwrap();
            app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 50, "height": 40})).unwrap();
            let ctx = egui::Context::default();
            theme::install_fonts(&ctx);
            let mut h = Self { app, ctx, time: 0.0 };
            h.settle();
            h
        }

        fn frame(&mut self, events: Vec<egui::Event>) {
            self.time += 0.05;
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN);
            let input = egui::RawInput { time: Some(self.time), events, screen_rect: Some(screen), ..Default::default() };
            let app = &mut self.app;
            self.ctx
                .run_ui(input, |ui| {
                    show(app, ui);
                    floating_panel(app, ui.ctx());
                })
                .textures_delta
                .clear();
        }

        fn settle(&mut self) {
            for _ in 0..3 {
                self.frame(vec![]);
            }
        }

        fn click(&mut self, at: egui::Pos2) {
            let button =
                |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
            self.frame(vec![egui::Event::PointerMoved(at), button(true)]);
            self.frame(vec![button(false)]);
            self.settle();
        }

        /// The icon column's left edge as last drawn.
        fn column_left(&self) -> f32 {
            self.ctx.data(|d| d.get_temp::<f32>(column_left_id())).unwrap()
        }

        /// The panel icons of the column, top to bottom.
        fn icons(&self) -> Vec<Rect> {
            widget_rects(&self.ctx, vec2(30.0, 30.0))
        }

        /// The double arrow at the top right of the window (the dock's, or the collapsed column's).
        fn arrow(&self) -> egui::Pos2 {
            egui::pos2(SCREEN.x - 10.0, HEADER / 2.0)
        }
    }

    fn icon_panel_count() -> usize {
        ICON_PANEL_GROUPS.iter().map(|g| g.len()).sum()
    }

    #[test]
    fn the_double_arrow_collapses_the_dock_to_icons_and_back() {
        let mut h = Harness::new();
        assert!(!h.app.ui.dock_collapsed);
        // Expanded, the group (at least its 230-point minimum) is right of the column.
        let expanded = |left: f32| left <= SCREEN.x - ICON_COL - 230.0;
        assert!(expanded(h.column_left()), "expanded: the group is right of the column: {}", h.column_left());
        assert_eq!(h.icons().len(), icon_panel_count());
        h.click(h.arrow());
        assert!(h.app.ui.dock_collapsed, "the » collapses the dock");
        assert!(h.column_left() > SCREEN.x - ICON_COL - 2.0, "collapsed: only the icon column is left: {}", h.column_left());
        let icons = h.icons();
        assert_eq!(icons.len(), icon_panel_count() + 4, "Properties, Layers, Sequencer and Libraries became icons");
        assert!(icons[0].top() > HEADER, "the icons sit under the « strip");
        h.click(h.arrow());
        assert!(!h.app.ui.dock_collapsed, "the « expands the dock again");
        assert!(expanded(h.column_left()), "the group is back: {}", h.column_left());
        assert_eq!(h.icons().len(), icon_panel_count());
    }

    #[test]
    fn a_collapsed_panel_pops_out_left_of_the_column() {
        let mut h = Harness::new();
        h.app.run("window.collapseDock", json!({"collapsed": true})).unwrap();
        h.settle();
        // Properties, Layers, Libraries head the column.
        let layers = h.icons()[1];
        h.click(layers.center());
        assert_eq!(h.app.ui.open_panel.as_deref(), Some("layers"));
        h.settle();
        let rect = h.ctx.memory(|m| m.area_rect(egui::Id::new("icon-panel"))).unwrap();
        let column = h.column_left();
        assert!(rect.right() <= column, "the flyout {rect:?} stays left of the column at {column}");
        assert!(rect.left() >= 0.0 && rect.bottom() <= SCREEN.y, "the flyout {rect:?} is on screen");
        assert!(rect.height() > 200.0, "Layers gets room for its list: {rect:?}");
        // Its » puts it away.
        h.click(egui::pos2(rect.right() - 13.0, rect.top() + 13.0));
        assert_eq!(h.app.ui.open_panel, None, "the flyout's » closes it");
        // Expanding with a panel popped out shows its tab.
        h.click(layers.center());
        assert_eq!(h.app.ui.open_panel.as_deref(), Some("layers"));
        h.click(h.arrow());
        assert!(!h.app.ui.dock_collapsed);
        assert_eq!((h.app.ui.open_panel.as_deref(), h.app.ui.dock_tab), (None, DockTab::Layers));
        // Properties pops out as wide as the dock and shows its sections, not a sliver, also as
        // the first flyout of a session (a fresh harness: the flyouts share one area, which
        // would otherwise keep the size the Layers flyout gave it).
        let mut h = Harness::new();
        h.app.run("window.collapseDock", json!({"collapsed": true})).unwrap();
        h.app.run("window.panel", json!({"panel": "properties"})).unwrap();
        h.settle();
        let rect = h.ctx.memory(|m| m.area_rect(egui::Id::new("icon-panel"))).unwrap();
        assert!(rect.width() >= DOCK_WIDTH && rect.right() <= h.column_left(), "Properties flyout {rect:?}");
        // Taller than the 400 points egui sizes a new area at: its sections (Align, Quick
        // Actions) aren't cut off below Appearance.
        assert!(rect.height() > 450.0 && rect.bottom() <= SCREEN.y, "Properties flyout {rect:?}");
    }

    #[test]
    fn the_collapse_command_sets_or_toggles_the_dock() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        assert_eq!(app.run("window.collapseDock", json!({})).unwrap(), json!(true));
        assert!(app.ui.dock_collapsed && crate::menus::checked(&app, "window.collapseDock", &json!({})) == Some(true));
        assert_eq!(app.run("window.collapseDock", json!({"collapsed": true})).unwrap(), json!(true));
        assert!(app.run("window.collapseDock", json!({"collapsed": "yes"})).is_err());
        // Window › Layers pops Layers out of its icon while collapsed.
        app.run("window.panel", json!({"panel": "layers"})).unwrap();
        assert_eq!(app.ui.open_panel.as_deref(), Some("layers"));
        assert_eq!(crate::menus::checked(&app, "window.panel", &json!({"panel": "layers"})), Some(true));
        assert_eq!(app.run("window.collapseDock", json!({})).unwrap(), json!(false));
        assert_eq!((app.ui.open_panel.as_deref(), app.ui.dock_tab), (None, DockTab::Layers));
        app.run("window.panel", json!({"panel": "properties"})).unwrap();
        assert_eq!((app.ui.open_panel.as_deref(), app.ui.dock_tab), (None, DockTab::Properties));
        // Saved with the preferences and with a user workspace; the built-in ones expand it.
        app.run("window.collapseDock", json!({"collapsed": true})).unwrap();
        let saved: crate::UiState = serde_json::from_slice(&serde_json::to_vec(&app.ui).unwrap()).unwrap();
        assert!(saved.sanitized().dock_collapsed);
        app.run("window.workspace.new", json!({"name": "Icons"})).unwrap();
        app.run("window.workspace", json!({"name": "Essentials"})).unwrap();
        assert!(!app.ui.dock_collapsed, "Essentials shows the panels");
        app.run("window.workspace", json!({"name": "Icons"})).unwrap();
        assert!(app.ui.dock_collapsed, "the user workspace keeps them collapsed");
    }
}
