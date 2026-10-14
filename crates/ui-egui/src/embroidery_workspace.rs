//! Roidery: the Embroidery workspace (Window › Workspace › Embroidery), Roidery's default. It
//! keeps VectorCraft's features but hides what embroidery doesn't use: a Basic toolbar of the
//! tools a digitizer needs (Advanced still shows every tool), a dock icon column without the
//! illustration-only panels, and menus without 3D, graphs, perspective grid, symbols, slices and
//! print tiling. `UiState::embroidery` turns it on; other workspaces turn it off.

use serde_json::Value;

use crate::VectorcraftApp;
use crate::menus::Item;
use crate::state::DockTab;
use crate::workspaces::Workspace;

pub const NAME: &str = "Embroidery";

/// The Basic toolbar in the Embroidery workspace: (category, slots), as `toolbar::BASIC`.
pub const TOOLBAR: &[(&str, &[&[&str]])] = &[
    ("Select", &[&["selection"], &["directSelection", "groupSelection"], &["lasso"]]),
    ("Embroidery", &[&["satinBlocks"], &["fillAngle"]]),
    (
        "Draw",
        &[
            &["pen", "addAnchor", "deleteAnchor", "anchorPoint"],
            &["curvature"],
            &["pencil", "smooth", "pathEraser", "join"],
            &["scissors", "knife", "eraser"],
        ],
    ),
    ("Shapes", &[&["rectangle", "roundedRectangle", "star", "polygon", "lineSegment", "arc", "spiral"], &["ellipse"]]),
    ("Modify", &[&["rotate", "reflect", "scale", "shear", "reshape", "freeTransform"], &["shapeBuilder"], &["width"]]),
    ("Type", &[&["type", "typeOnPath", "areaType"]]),
    ("Navigate", &[&["zoom"], &["hand"], &["rotateView"]]),
    ("Color", &[&["eyedropper", "measure"]]),
];

/// Panels the Embroidery workspace trims: left out of the dock, Window and command search (they
/// still open by id, `window.panel`).
const TRIMMED_PANELS: &[&str] = &[
    // Colour and appearance.
    "colorGuide",
    "colorThemes",
    "gradient",
    "transparency",
    "appearance",
    "graphicStyles",
    // Brushes and symbols.
    "brushes",
    "symbols",
    // Type beyond font and size.
    "paragraph",
    "charStyles",
    "paraStyles",
    "openType",
    "glyphs",
    "tabs",
    // Output for screens and print.
    crate::panels::asset_export::ID,
    crate::panels::css_properties::ID,
    "separations",
    crate::panels::flattener_preview::ID,
];

/// Dock icons the Embroidery workspace also leaves out, though Window still opens them: the
/// Sequencer lists the layers, so Layers has no tab of its own.
const DOCK_ONLY: &[&str] = &["libraries", "layers", "patternOptions"];

/// Submenus the Embroidery workspace leaves out.
const HIDDEN_MENUS: &[&str] = &[
    "3D and Materials",
    "Graph",
    "Perspective",
    "Perspective Grid",
    "Slice",
    "Symbol Libraries",
    "Blend",
    "Threaded Text",
    "Text Wrap",
    "Brush Libraries",
    "Graphic Style Libraries",
    "Gradients",
    "Collect for Export",
    "SVG Filters",
];

/// Effects the Embroidery workspace leaves out of Effect: blurs, colour adjustments, shadows,
/// glows and feathering (raster looks thread can't sew), and plug-in effects.
fn hidden_effect(id: &str) -> bool {
    ["blur.", "adjust.", vectorcraft_plugins::effect::PREFIX].iter().any(|x| id.starts_with(x))
        || matches!(id, "stylize.dropShadow" | "stylize.innerGlow" | "stylize.outerGlow" | "stylize.feather")
}

/// Whether the fx menu leaves out effect `id`.
pub fn hides_effect(app: &VectorcraftApp, id: &str) -> bool {
    app.ui.embroidery && hidden_effect(id)
}

/// Commands the Embroidery workspace leaves out of the menus and command search (they still run
/// by id).
fn hidden_command(id: &str, p: &Value) -> bool {
    const PREFIXES: &[&str] = &[
        "object.slice.",
        "perspective.",
        "graph.",
        "view.slices.",
        "ui.perspective",
        "ui.savePerspective",
        "object.blend.",
        "object.mesh.",
        "object.textWrap.",
        "text.thread.",
        "window.graphicStyleLibrary",
        "assets.",
    ];
    let param = |k: &str| p.get(k).and_then(Value::as_str);
    PREFIXES.iter().any(|x| id.starts_with(x))
        || matches!(
            id,
            "view.printTiling"
                | "select.object.slices"
                | "select.same.symbolInstance"
                | "select.same.appearance"
                | "select.same.appearanceAttribute"
                | "select.same.blendingMode"
                | "select.same.opacity"
                | "select.same.graphicStyle"
                | "select.object.brushStrokes"
                | "select.object.bristleBrushStrokes"
                | "document.rasterEffectsSettings"
                | "file.exportForScreens"
                | "ui.flattenTransparencyDialog"
                | "ui.flattenerPresetsDialog"
                | "ui.saveGraphicStyleLibrary"
                | "view.gradientAnnotator"
                | "paint.lastGradient"
                | "window.taskBar"
                | "window.control"
        )
        || (id == "window.panel" && param("panel").is_some_and(|x| TRIMMED_PANELS.contains(&x)))
        || (id == "window.swatchLibrary" && param("library").is_some_and(|x| x.contains("gradient")))
        || (id.starts_with("effect.") && param("effect").is_some_and(hidden_effect))
}

/// Tools the Embroidery workspace leaves out of the Advanced toolbar and command search (their
/// keys still pick them).
const HIDDEN_TOOLS: &[&str] = &[
    "flare",
    "mesh",
    "gradient",
    "blend",
    "symbolSprayer",
    "symbolShifter",
    "symbolScruncher",
    "symbolSizer",
    "symbolSpinner",
    "symbolStainer",
    "symbolScreener",
    "symbolStyler",
    "columnGraph",
    "stackedColumnGraph",
    "barGraph",
    "stackedBarGraph",
    "lineGraph",
    "areaGraph",
    "scatterGraph",
    "pieGraph",
    "radarGraph",
    "perspectiveGrid",
    "perspectiveSelection",
    "slice",
    "sliceSelection",
    "printTiling",
];

/// Whether the toolbar leaves out tool `id`.
pub fn hides_tool(app: &VectorcraftApp, id: &str) -> bool {
    app.ui.embroidery && HIDDEN_TOOLS.contains(&id)
}

/// Whether command search leaves out `id` (a command id, or `tool:<id>`).
pub fn hides_in_search(app: &VectorcraftApp, id: &str) -> bool {
    app.ui.embroidery
        && match id.strip_prefix("tool:") {
            Some(tool) => HIDDEN_TOOLS.contains(&tool),
            None => hidden_command(id, &Value::Null),
        }
}

/// Whether the canvas leaves out the Contextual Task Bar (the floating bar under a selection).
pub fn hides_task_bar(app: &VectorcraftApp) -> bool {
    app.ui.embroidery
}

/// Whether the toolbar leaves out the Gradient paint button.
pub fn hides_gradient_paint(app: &VectorcraftApp) -> bool {
    app.ui.embroidery
}

/// The built-in Embroidery workspace: the embroidery toolbar, the Sequencer in the dock (no
/// Libraries tab), and the Stitch Settings panel open.
pub fn workspace(base: &Workspace) -> Workspace {
    Workspace {
        name: NAME.into(),
        embroidery: true,
        dock_tab: DockTab::Sequencer,
        task_bar: false,
        open_panel: Some(crate::panels::stitch_settings::ID.into()),
        ..base.clone()
    }
}

/// Roidery's first run: the Embroidery workspace, and millimetres as the General unit (what
/// hoops and stitch settings are measured in). Saved preferences replace both afterwards.
pub fn first_run(app: &mut VectorcraftApp) {
    if let Some(w) = crate::workspaces::find(&app.ui, NAME) {
        crate::workspaces::apply(&mut app.ui, &w);
    }
    // Best effort: points stay the unit if the preference is refused.
    let _ = app.run("prefs.set", serde_json::json!({"key": "unitsGeneral", "value": "Millimeters"}));
}

/// Whether the dock's icon column leaves out panel `id`.
pub fn hides_panel(app: &VectorcraftApp, id: &str) -> bool {
    app.ui.embroidery && (TRIMMED_PANELS.contains(&id) || DOCK_ONLY.contains(&id))
}

/// The icon column's groups in the order drawn: the Embroidery workspace puts Roidery's
/// Stitch Settings and Threads group (the last of [`crate::state::ICON_PANEL_GROUPS`]) first.
pub fn icon_groups(ui: &crate::UiState) -> Vec<&'static [&'static str]> {
    let groups = crate::state::ICON_PANEL_GROUPS;
    match groups.split_last() {
        Some((ours, rest)) if ui.embroidery => std::iter::once(*ours).chain(rest.iter().copied()).collect(),
        _ => groups.to_vec(),
    }
}

/// The dock's tabs, without those the Embroidery workspace hides (unless Window opened one).
pub fn dock_tabs(app: &VectorcraftApp) -> Vec<DockTab> {
    DockTab::ALL.into_iter().filter(|t| !hides_dock_tab(&app.ui, *t)).collect()
}

/// Whether the dock leaves `tab` out (never the one shown): the Embroidery workspace the panels
/// it hides, the other workspaces the Sequencer.
pub fn hides_dock_tab(ui: &crate::UiState, tab: DockTab) -> bool {
    let id = tab.info().0;
    if ui.dock_tab == tab {
        return false;
    }
    if ui.embroidery { TRIMMED_PANELS.contains(&id) || DOCK_ONLY.contains(&id) } else { tab == DockTab::Sequencer }
}

/// `items` without what the Embroidery workspace hides (unchanged in other workspaces).
pub fn menus(app: &VectorcraftApp, items: Vec<Item>) -> Vec<Item> {
    if !app.ui.embroidery {
        return without_embroidery_bar(items);
    }
    strip(items)
}

/// `items` without Window › Embroidery Bar, which only the Embroidery workspace has.
fn without_embroidery_bar(items: Vec<Item>) -> Vec<Item> {
    items
        .into_iter()
        .filter(|it| !matches!(it, Item::Cmd(_, "window.embroideryBar", _)))
        .map(|it| match it {
            Item::Sub(label, children) => Item::Sub(label, without_embroidery_bar(children)),
            it => it,
        })
        .collect()
}

/// Whether the window shows the embroidery bar ([`crate::embroidery_bar`]) in place of the
/// Control bar.
pub fn shows_embroidery_bar(app: &VectorcraftApp) -> bool {
    app.ui.embroidery && app.ui.embroidery_bar
}

fn strip(items: Vec<Item>) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::with_capacity(items.len());
    for it in items {
        match it {
            Item::Sub(label, _) if HIDDEN_MENUS.contains(&label) => {}
            Item::Sub(label, children) => {
                let children = strip(children);
                if !children.is_empty() {
                    out.push(Item::Sub(label, children));
                }
            }
            Item::Cmd(_, id, ref p) if hidden_command(id, p) => {}
            // No separator first, or twice in a row.
            Item::Sep if out.last().is_none_or(|l| matches!(l, Item::Sep)) => {}
            it => out.push(it),
        }
    }
    // No heading over nothing.
    let mut tidy: Vec<Item> = Vec::with_capacity(out.len());
    let mut rest = out.into_iter().peekable();
    while let Some(it) = rest.next() {
        if !(matches!(it, Item::Header(_)) && rest.peek().is_none_or(|n| matches!(n, Item::Sep | Item::Header(_)))) {
            tidy.push(it);
        }
    }
    let mut out = tidy;
    while matches!(out.last(), Some(Item::Sep)) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use vectorcraft_engine::Session;

    use super::*;
    use crate::menus::{self, MenuEntry};

    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        first_run(&mut app);
        app
    }

    fn entries(app: &VectorcraftApp) -> Vec<MenuEntry> {
        menus::menu_entries(app)
    }

    #[test]
    fn roidery_opens_on_the_embroidery_workspace() {
        let app = app();
        assert!(app.ui.embroidery);
        assert_eq!(app.ui.workspace, NAME);
        assert_eq!(app.ui.open_panel.as_deref(), Some(crate::panels::stitch_settings::ID));
        assert_eq!(crate::workspaces::names(&app.ui)[0], NAME, "first in Window › Workspace");
        assert_eq!(app.session.prefs.units_general, "millimeters");
    }

    #[test]
    fn menus_leave_out_what_embroidery_does_not_use_until_another_workspace() {
        let mut app = app();
        let has = |app: &VectorcraftApp, f: &dyn Fn(&MenuEntry) -> bool| entries(app).iter().any(f);
        let illustration = |e: &MenuEntry| {
            e.path.iter().any(|p| HIDDEN_MENUS.contains(&p.as_str()))
                || e.command.as_deref().is_some_and(|c| c.starts_with("object.slice.") || c == "view.printTiling")
        };
        assert!(!has(&app, &illustration));
        assert!(!has(&app, &|e| e.label == "Symbols" && e.path == ["Window"]));
        assert!(has(&app, &|e| e.command.as_deref() == Some("file.exportMachine")));
        assert!(has(&app, &|e| e.label == "Layers"));
        // Every workspace switch goes through Window › Workspace.
        app.run("window.workspace", serde_json::json!({"name": "Essentials"})).unwrap();
        assert!(!app.ui.embroidery);
        assert!(has(&app, &illustration));
        app.run("window.workspace", serde_json::json!({"name": NAME})).unwrap();
        assert!(app.ui.embroidery && !has(&app, &illustration));
    }

    #[test]
    fn stripping_keeps_separators_tidy() {
        let items = vec![
            Item::Sep,
            Item::Cmd("Show Print Tiling", "view.printTiling", Value::Null),
            Item::Sep,
            Item::Cmd("Layers", "window.panel", serde_json::json!({"panel": "layers"})),
            Item::Sep,
            Item::Sub("Slice", vec![Item::Cmd("Make", "object.slice.make", Value::Null)]),
            Item::Sep,
        ];
        let out = strip(items);
        assert_eq!(out.len(), 1, "{:?}", out.iter().map(|i| matches!(i, Item::Sep)).collect::<Vec<_>>());
    }

    #[test]
    fn the_basic_toolbar_names_real_tools_and_satin_blocks_first_after_select() {
        let tools: Vec<&str> = TOOLBAR.iter().flat_map(|(_, slots)| slots.iter().flat_map(|s| s.iter().copied())).collect();
        for t in &tools {
            assert!(vectorcraft_tools::tool_info(t).is_some(), "{t}");
        }
        assert_eq!(TOOLBAR[1], ("Embroidery", &[&["satinBlocks"][..], &["fillAngle"][..]][..]));
    }

    #[test]
    fn the_embroidery_workspace_trims_what_thread_cannot_sew_and_keeps_the_rest() {
        let mut app = app();
        let find = |app: &VectorcraftApp, path: &str, label: &str| {
            entries(app).iter().any(|e| e.path.join(" > ") == path && e.label.trim_end_matches('…') == label)
        };
        let trimmed = [
            ("Effect > Blur", "Gaussian Blur"),
            ("Effect > Stylize", "Drop Shadow"),
            ("Effect > Color Adjustments", "Levels"),
            ("Effect", "Document Raster Effects Settings"),
            ("Window", "Transparency"),
            ("Window", "Appearance"),
            ("Window", "Graphic Styles"),
            ("Window", "Color Guide"),
            ("Window", "Gradient"),
            ("Window", "Brushes"),
            ("Window", "Asset Export"),
            ("Window", "CSS Properties"),
            ("Window", "Separations Preview"),
            ("Window", "Flattener Preview"),
            ("Window > Type", "Paragraph"),
            ("Window > Type", "OpenType"),
            ("Type", "Glyphs"),
            ("Type > Threaded Text", "Create"),
            ("Object > Blend", "Make"),
            ("Object", "Create Gradient Mesh"),
            ("Object > Text Wrap", "Make"),
            ("File > Export", "Export for Screens"),
            ("Select > Same", "Blending Mode"),
        ];
        let kept = [
            ("Effect > Distort & Transform", "Zig Zag"),
            ("Effect > Warp", "Arc"),
            ("Effect > Path", "Offset Path"),
            ("Effect > Pathfinder", "Add"),
            ("Effect > Convert to Shape", "Ellipse"),
            ("Effect > Stylize", "Round Corners"),
            ("Type > Change Case", "UPPERCASE"),
            ("Type", "Add Lettering"),
            ("Window > Type", "Character"),
            ("Window", "Swatches"),
            ("Window", "Color"),
            ("Window", "Threads"),
            ("Window", "Image Trace"),
            ("Window", "Layers"),
            ("File", "Place"),
            ("File > Export", "Export As SVG"),
            ("File > Export", "Export As PNG"),
            ("File", "Save as PDF"),
            ("File", "Export Machine File"),
            ("File", "Export Worksheet"),
            ("File", "Export Mockup"),
            ("Object > Live Paint", "Make"),
            ("Object > Image Trace", "Release"),
        ];
        for (path, label) in kept {
            assert!(find(&app, path, label), "{path} > {label} stays");
        }
        for (path, label) in trimmed {
            assert!(!find(&app, path, label), "{path} > {label} is trimmed");
        }
        // Hidden, not removed: the commands still run, and other workspaces show them.
        app.run("window.panel", serde_json::json!({"panel": "transparency"})).unwrap();
        assert_eq!(app.ui.open_panel.as_deref(), Some("transparency"));
        app.run("window.workspace", serde_json::json!({"name": "Essentials"})).unwrap();
        for (path, label) in trimmed {
            assert!(find(&app, path, label), "{path} > {label} is back");
        }
    }

    #[test]
    fn the_toolbar_dock_and_search_leave_out_trimmed_tools_and_panels() {
        let mut app = app();
        assert!(hides_tool(&app, "mesh") && hides_tool(&app, "gradient") && hides_tool(&app, "symbolSprayer"));
        assert!(!hides_tool(&app, "pen") && !hides_tool(&app, "width") && !hides_tool(&app, "shapeBuilder") && !hides_tool(&app, "livePaintBucket"));
        assert!(hides_in_search(&app, "tool:blend") && hides_in_search(&app, "object.blend.make") && hides_in_search(&app, "file.exportForScreens"));
        assert!(!hides_in_search(&app, "tool:pen") && !hides_in_search(&app, "stitch.convertToSatin") && !hides_in_search(&app, "file.place"));
        // The Sequencer lays out the layers, so Layers has no tab, until Window › Layers opens it.
        assert!(!dock_tabs(&app).contains(&DockTab::Layers));
        assert!(dock_tabs(&app).contains(&DockTab::Sequencer));
        // No floating task bar under the selection, even with the flag saved on from before.
        app.ui.task_bar = true;
        assert!(hides_task_bar(&app) && !crate::menus::menu_entries(&app).iter().any(|e| e.command.as_deref() == Some("window.taskBar")));
        app.run("window.panel", serde_json::json!({"panel": "layers"})).unwrap();
        assert!(dock_tabs(&app).contains(&DockTab::Layers));
        app.run("window.workspace", serde_json::json!({"name": "Essentials"})).unwrap();
        assert!(!hides_tool(&app, "mesh") && !hides_in_search(&app, "tool:blend") && dock_tabs(&app).contains(&DockTab::Layers));
        assert!(!hides_task_bar(&app) && app.ui.task_bar);
    }

    #[test]
    fn a_heading_over_nothing_goes() {
        let items = vec![
            Item::Header("Vector Effects"),
            Item::Sub("Warp", vec![Item::Cmd("Arc", "effect.dialog", serde_json::json!({"effect": "warp.arc"}))]),
            Item::Sep,
            Item::Header("Raster Effects"),
            Item::Sub("Blur", vec![Item::Cmd("Gaussian Blur", "effect.dialog", serde_json::json!({"effect": "blur.gaussian"}))]),
        ];
        let out = strip(items);
        assert_eq!(out.len(), 2, "Vector Effects and Warp");
        assert!(matches!(out[0], Item::Header("Vector Effects")));
    }
}
