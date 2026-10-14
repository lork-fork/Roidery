//! macOS: VectorCraft's menu tree as the native system menu bar (like Illustrator on the Mac).
//! Items dispatch through the same command path as the in-window menus; enablement, check marks
//! and dynamic labels ("Undo Move") are refreshed a few times per second, and the menu is rebuilt
//! when saved views or recent files come and go.

use std::collections::HashMap;
use std::str::FromStr;

use muda::accelerator::Accelerator;
use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use serde_json::Value;
use vectorcraft_ui_egui::VectorcraftApp;
use vectorcraft_ui_egui::i18n::{self, Lang, t};
use vectorcraft_ui_egui::menus::{self, Item};

enum Handle {
    Plain(MenuItem),
    Check(CheckMenuItem),
}

pub struct NativeMenu {
    _menu: Menu,
    items: HashMap<String, (String, Value, Handle, String)>,
    last_refresh: f64,
    /// Shortcut-override / workspace-list generation the menu was built for.
    generation: u64,
    /// Saved views and recent files listed (empty slots are left out, so a change rebuilds).
    slots: usize,
    /// Plug-in registry revision the Plug-ins submenus were built for.
    plugins: u64,
    /// The language the labels were built in (a change rebuilds the menu).
    language: Lang,
    /// Roidery: whether the Embroidery workspace's menus were built (a switch rebuilds).
    embroidery: bool,
}

/// Accelerator for a shortcut like "Cmd+Shift+]" (modifier-less shortcuts stay in the app so they
/// don't steal keys from text fields; a focused field still gets Cmd+A/X/C/V, see [`NativeMenu::poll`]).
fn accel(sc: &str) -> Option<Accelerator> {
    if !(sc.contains("Cmd") || sc.contains("Ctrl") || sc.contains("Alt") || sc.starts_with('F')) {
        return None;
    }
    Accelerator::from_str(&sc.replace("Cmd", "CMD").replace("Alt", "ALT").replace("Shift", "SHIFT").replace("Ctrl", "CTRL")).ok()
}

impl NativeMenu {
    pub fn install(app: &mut VectorcraftApp) -> Self {
        // Accelerators come from `shortcut_of`, which honours the user's overrides.
        vectorcraft_ui_egui::shortcut_editor::sync(&app.ui);
        let generation = vectorcraft_ui_egui::shortcut_editor::GENERATION.load(std::sync::atomic::Ordering::Relaxed);
        let menu = Menu::new();
        let mut items = HashMap::new();
        let mut counter = 0usize;
        for (title, entries) in menus::visible_tree(app) {
            let sub = Submenu::new(t(title), true);
            build(app, &sub, &entries, &mut items, &mut counter);
            let _ = menu.append(&sub);
        }
        menu.init_for_nsapp();
        app.native_menu = true;
        app.native_shortcuts = items
            .values()
            .filter(|(_, p, _, c)| (p.is_null() || p.as_object().is_some_and(|o| o.is_empty())) && menus::shortcut_of(c).and_then(accel).is_some())
            .map(|(_, _, _, c)| c.clone())
            .collect();
        Self {
            _menu: menu,
            items,
            last_refresh: 0.0,
            generation,
            slots: menus::listed_slots(app),
            plugins: menus::plugin_revision(),
            language: i18n::current(),
            embroidery: app.ui.embroidery,
        }
    }

    /// Dispatch chosen items (a focused text field takes Select All and the clipboard) and refresh
    /// state.
    pub fn poll(&mut self, app: &mut VectorcraftApp, ctx: &egui::Context) {
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if let Some((cmd, params, _, _)) = self.items.get(ev.id.as_ref()) {
                let p = if params.is_null() { serde_json::json!({}) } else { params.clone() };
                menus::invoke_from_system_menu(app, ctx, cmd, p);
            }
        }
        let now = vectorcraft_ui_egui::now_ms();
        if now - self.last_refresh < 250.0 {
            return;
        }
        self.last_refresh = now;
        // Shortcuts edited, workspaces added, saved views, recent files, plug-ins or the UI language
        // changed: rebuild so accelerators, lists and labels are current.
        if vectorcraft_ui_egui::shortcut_editor::GENERATION.load(std::sync::atomic::Ordering::Relaxed) != self.generation
            || menus::listed_slots(app) != self.slots
            || menus::plugin_revision() != self.plugins
            || i18n::current() != self.language
            || app.ui.embroidery != self.embroidery
        {
            *self = NativeMenu::install(app);
            return;
        }
        for (run, rp, h, cmd) in self.items.values() {
            let null = Value::Null;
            let params = if run == cmd { rp } else { &null };
            let en = menus::enabled(app, cmd);
            match h {
                Handle::Plain(i) => {
                    i.set_enabled(en);
                    if matches!(
                        cmd.as_str(),
                        "edit.undo"
                            | "edit.redo"
                            | "view.outline"
                            | "view.rulers"
                            | "view.guides"
                            | "view.grid"
                            | "view.edges"
                            | "view.artboards"
                            | "view.boundingBox"
                            | "view.transparencyGrid"
                            | "window.workspace.reset"
                    ) || cmd.starts_with("file.openRecent")
                        || cmd.starts_with("type.recentFont")
                        || cmd.starts_with("view.goto")
                        || matches!(
                            cmd.as_str(),
                            "view.cornerWidget" | "view.textThreads" | "view.gradientAnnotator" | "effect.last" | "type.hiddenCharacters"
                        )
                    {
                        i.set_text(menus::display_label(app, cmd, ""));
                    }
                }
                Handle::Check(c) => {
                    c.set_enabled(en);
                    c.set_checked(menus::checked(app, cmd, params).unwrap_or(false));
                }
            }
        }
    }
}

fn build(
    app: &VectorcraftApp,
    parent: &Submenu,
    entries: &[Item],
    items: &mut HashMap<String, (String, Value, Handle, String)>,
    counter: &mut usize,
) {
    for e in entries {
        match e {
            Item::Sep => {
                let _ = parent.append(&PredefinedMenuItem::separator());
            }
            Item::Header(h) => {
                let _ = parent.append(&MenuItem::new(t(h), false, None));
            }
            Item::Todo(label, sc) => {
                let _ = parent.append(&MenuItem::new(t(label), false, accel(sc)));
            }
            Item::Sub(label, children) => {
                let sub = Submenu::new(t(label), true);
                build(app, &sub, children, items, counter);
                let _ = parent.append(&sub);
            }
            // Unused saved-view and recent-file slots are left out, as in the in-window menus.
            Item::Cmd(_, cmd, _) if menus::hidden_slot(app, cmd) => {}
            Item::Cmd(label, cmd, params) => {
                *counter += 1;
                let id = format!("dc{counter}");
                let sc = if params.is_null() { menus::shortcut_of(cmd).and_then(accel) } else { None };
                let handle = if menus::checked(app, cmd, params).is_some() {
                    let c = CheckMenuItem::with_id(id.clone(), menus::display_label(app, cmd, label), true, false, sc);
                    let _ = parent.append(&c);
                    Handle::Check(c)
                } else {
                    let i = MenuItem::with_id(id.clone(), menus::display_label(app, cmd, label), true, sc);
                    let _ = parent.append(&i);
                    Handle::Plain(i)
                };
                let (run, run_params) = menus::click_target(label, cmd, params);
                let _ = &run_params;
                items.insert(id, (run, run_params, handle, cmd.to_string()));
            }
        }
    }
}
