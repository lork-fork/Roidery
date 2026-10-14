//! VectorCraft desktop app.
//!
//! Usage: `vectorcraft [--control <port>] [files…]`
//!
//! `--control <port>` (or `VECTORCRAFT_CONTROL_PORT`) starts a localhost JSON-lines control server:
//! `{"id":1,"method":"ui.inspect","params":{}}` → `{"id":1,"ok":true,"result":…}`.
//! See `vectorcraft_ui_egui::control` for the methods.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(all(feature = "windows7", any(feature = "wgpu", feature = "accessibility")))]
compile_error!("windows7 requires --no-default-features (wgpu and accessibility must be disabled)");
#[cfg(all(windows, feature = "windows7", not(target_vendor = "win7")))]
compile_error!("windows7 requires --target x86_64-win7-windows-msvc; the ordinary Windows target still imports newer APIs");
#[cfg(all(target_vendor = "win7", not(feature = "windows7")))]
compile_error!("the win7 target requires --no-default-features --features windows7");

mod clipboard;
mod control_server;
#[cfg(target_os = "macos")]
mod native_menu;
#[cfg(target_os = "macos")]
mod open_documents;
mod printing;
mod update_install;
mod window;

use vectorcraft_engine::Session;
use vectorcraft_engine::cmd::fileio;
use vectorcraft_ui_egui::graphics::GraphicsLoss;
use vectorcraft_ui_egui::{FilePick, Services, VectorcraftApp};

struct App {
    app: VectorcraftApp,
    /// Reported by wgpu when the window's graphics device is lost (a driver reset).
    graphics_loss: GraphicsLoss,
    /// The graphics device was lost and the unsaved changes are kept for Data Recovery.
    graphics_lost: bool,
    #[cfg(target_os = "macos")]
    menu: Option<native_menu::NativeMenu>,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(why) = self.graphics_loss.take() {
            // eframe can't give a window a new device: the user saves and starts again.
            self.graphics_lost = self.app.graphics_lost(&why);
            self.app.status(if self.graphics_lost {
                "The graphics device was lost: unsaved changes are kept for Data Recovery. Save your documents and restart Roidery"
            } else {
                "The graphics device was lost: save your documents and restart Roidery"
            });
        }
        if self.graphics_lost && ctx.input(|i| i.viewport().close_requested()) {
            // The window can't show the Save Changes question: it closes, and the next launch
            // offers the changes back.
            vectorcraft_ui_egui::background::wait_all(&mut self.app);
            return;
        }
        #[cfg(target_os = "macos")]
        {
            if self.menu.is_none() && std::env::var_os("VECTORCRAFT_NO_NATIVE_MENU").is_none() {
                self.menu = Some(native_menu::NativeMenu::install(&mut self.app));
            }
            if let Some(m) = &mut self.menu {
                m.poll(&mut self.app, ctx);
            }
            open_files(&mut self.app, open_documents::take());
        }
        self.app.logic(ctx);
        window::track(ctx, &mut self.app.ui.window);
        if self.app.ui.status == "quit" {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.app.raw_input_hook(raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
        #[cfg(target_os = "macos")]
        if self.app.take_ime_discard() {
            discard_marked_text();
        }
    }
    #[cfg(not(feature = "windows7"))]
    fn on_exit(&mut self) {
        save_prefs(&self.app);
    }
    #[cfg(feature = "windows7")]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        save_prefs(&self.app);
    }
}

/// Open files handed to the app (command line, macOS Finder and Dock) as documents. A file that
/// can't be opened is reported in the status bar and on stderr; the others still open.
fn open_files(app: &mut VectorcraftApp, files: Vec<String>) {
    for f in files {
        if let Err(e) = vectorcraft_ui_egui::io::open_path(app, &f) {
            eprintln!("vectorcraft: {f}: {e}");
            app.status(format!("Couldn't open {}: {e}", fileio::file_name(&f)));
        }
    }
}

/// Tell the macOS input method to drop its composition (the Type tool kept the marked text as
/// typed). winit's IME toggle only clears its own copy, so the IME would type it again.
#[cfg(target_os = "macos")]
fn discard_marked_text() {
    if let Some(mtm) = objc2::MainThreadMarker::new()
        && let Some(ic) = objc2_app_kit::NSTextInputContext::currentInputContext(mtm)
    {
        ic.discardMarkedText();
    }
}

/// Where UI preferences live: ~/Library/Application Support/Roidery (macOS),
/// %APPDATA%\Roidery (Windows), $XDG_CONFIG_HOME or ~/.config/roidery (Linux). Roidery keeps
/// its own, apart from a VectorCraft installed on the same machine.
fn prefs_path() -> Option<std::path::PathBuf> {
    prefs_path_for("Roidery", "roidery")
}

/// The same place under the project's former name (DrawCraft): read once if there are no
/// VectorCraft preferences yet, so settings survive the rename.
fn legacy_prefs_path() -> Option<std::path::PathBuf> {
    prefs_path_for("DrawCraft", "drawcraft")
}

fn prefs_path_for(name: &str, lower: &str) -> Option<std::path::PathBuf> {
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/Application Support").join(name))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join(name))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
            .map(|c| c.join(lower))
    };
    base.map(|b| b.join("ui.json"))
}

/// Runs without preferences (`VECTORCRAFT_NO_PREFS`, agents' test runs) neither read nor write them.
fn prefs_enabled() -> bool {
    std::env::var_os("VECTORCRAFT_NO_PREFS").is_none()
}

/// The saved UI preferences, read before the window opens (they hold its size and position).
fn read_prefs() -> Option<vectorcraft_ui_egui::UiState> {
    if !prefs_enabled() {
        return None;
    }
    let bytes = prefs_path().and_then(|p| std::fs::read(p).ok()).or_else(|| legacy_prefs_path().and_then(|p| std::fs::read(p).ok()))?;
    serde_json::from_slice(&bytes).ok()
}

fn load_prefs(app: &mut VectorcraftApp, saved: Option<vectorcraft_ui_egui::UiState>) {
    match saved {
        Some(ui) if prefs_enabled() => app.ui = ui.sanitized(),
        // Roidery: a first run opens on the Embroidery workspace.
        _ => vectorcraft_ui_egui::embroidery_workspace::first_run(app),
    }
    if !prefs_enabled() {
        return;
    }
    vectorcraft_ui_egui::prefs_dialog::restore(app);
}

fn save_prefs(app: &VectorcraftApp) {
    if !prefs_enabled() {
        return;
    }
    if let Some(p) = prefs_path() {
        let _ = std::fs::create_dir_all(p.parent().unwrap_or(std::path::Path::new(".")));
        let mut ui = app.ui.clone();
        ui.engine_prefs = app.session.prefs.to_json();
        if let Ok(bytes) = serde_json::to_vec_pretty(&ui) {
            // Preferences are best effort: a failed write keeps the previous file.
            let _ = fileio::write_atomic(&p, &bytes);
        }
    }
}

/// A native file dialog showing `pick`'s file types, folder and suggested name.
fn file_dialog(pick: &FilePick) -> rfd::FileDialog {
    let d = pick.filters.iter().fold(rfd::FileDialog::new(), |d, (name, exts)| d.add_filter(*name, exts));
    let d = match &pick.folder {
        Some(folder) => d.set_directory(folder),
        None => d,
    };
    if pick.name.is_empty() { d } else { d.set_file_name(&pick.name) }
}

/// File → Show in Folder: select `path` in Finder / Explorer, or open its folder elsewhere.
fn reveal(path: &str) -> Result<(), String> {
    reveal_command(path).spawn().map(|_| ()).map_err(|e| format!("can't show {path}: {e}"))
}

#[cfg(target_os = "macos")]
fn reveal_command(path: &str) -> std::process::Command {
    let mut c = std::process::Command::new("open");
    c.args(["-R", path]);
    c
}

#[cfg(windows)]
fn reveal_command(path: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt as _;
    // Explorer reads `/select,"path"` itself (the usual argument quoting breaks paths with spaces)
    // and needs backslashes.
    let mut c = std::process::Command::new("explorer");
    c.raw_arg(format!("/select,\"{}\"", path.replace('/', "\\")));
    c
}

#[cfg(not(any(target_os = "macos", windows)))]
fn reveal_command(path: &str) -> std::process::Command {
    let folder = std::path::Path::new(path).parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
    let mut c = std::process::Command::new("xdg-open");
    c.arg(folder);
    c
}

/// Write a file the safe way: a failed write keeps the old file ([`fileio::write_atomic`]).
fn write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    fileio::write_atomic(std::path::Path::new(path), bytes).map_err(|e| e.to_string())
}

fn services() -> Services {
    Services {
        pick_open: Some(Box::new(|pick: &FilePick| file_dialog(pick).pick_file().map(|p| p.to_string_lossy().to_string()))),
        pick_open_multi: Some(Box::new(|| {
            fileio::place_filters()
                .fold(rfd::FileDialog::new().set_title("Place"), |d, (name, exts)| d.add_filter(name, exts))
                .pick_files()
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect()
        })),
        pick_save: Some(Box::new(|pick: &FilePick| {
            // The Templates folder may not exist yet.
            if let Some(folder) = &pick.folder {
                let _ = std::fs::create_dir_all(folder);
            }
            file_dialog(pick).save_file().map(|p| p.to_string_lossy().to_string())
        })),
        read: Some(Box::new(|p: &str| std::fs::read(p).map_err(|e| e.to_string()))),
        write: Some(Box::new(write_file)),
        // Background Save and Export write from a worker thread.
        write_shared: Some(std::sync::Arc::new(write_file)),
        // Every format Copy offers and Paste reads (menu-bar Paste never sees egui's Paste event).
        system_clipboard: Some(clipboard::system_clipboard()),
        // Help → Discord / website / GitHub, the Discord button, About and Home links.
        open_url: Some(Box::new(|url: &str| {
            let _ = webbrowser::open(url);
        })),
        reveal: Some(Box::new(reveal)),
        // Links panel: Edit Original; Package: Show Package. Relink to Folder and Package pick folders.
        open_file: Some(Box::new(open_file)),
        pick_folder: Some(Box::new(|| rfd::FileDialog::new().pick_folder().map(|p| p.to_string_lossy().to_string()))),
        // File → Print: the system's printers and print queue.
        print: Some(Box::new(printing::SystemPrint)),
        // Roidery: Help › Check for Updates… reads the update feed and fetches installers.
        fetch: Some(std::sync::Arc::new(|url: &str, limit: u64, progress: &mut dyn FnMut(u64, Option<u64>)| {
            roidery_update::fetch(url, limit, progress)
        })),
        install_update: Some(Box::new(update_install::install)),
        ..Default::default()
    }
}

/// Edit Original, Show Package: open `path` (a file or a folder) in the system's default app for it.
fn open_file(path: &str) -> Result<(), String> {
    #[cfg(windows)]
    let mut c = {
        use std::os::windows::process::CommandExt as _;
        let mut c = std::process::Command::new("explorer");
        c.raw_arg(format!("\"{}\"", path.replace('/', "\\")));
        c
    };
    #[cfg(target_os = "macos")]
    let mut c = std::process::Command::new("open");
    #[cfg(not(any(target_os = "macos", windows)))]
    let mut c = std::process::Command::new("xdg-open");
    #[cfg(not(windows))]
    c.arg(path);
    c.spawn().map(|_| ()).map_err(|e| format!("can't open {path}: {e}"))
}

/// The window, Dock, taskbar and app-switcher icon (`assets/roidery-icon/`, see its README). macOS gets
/// the version with Apple's transparent margin; elsewhere the full-bleed tile. The app ID matches
/// `packaging/linux/app.roidery.Roidery.desktop` so Wayland docks find the launcher icon.
fn app_icon() -> egui::IconData {
    #[cfg(target_os = "macos")]
    let png: &[u8] = include_bytes!("../../../assets/roidery-icon/roidery-macos-512.png");
    #[cfg(not(target_os = "macos"))]
    let png: &[u8] = include_bytes!("../../../assets/roidery-icon/hicolor/256x256/apps/app.roidery.Roidery.png");
    eframe::icon_data::from_png_bytes(png).unwrap_or_default()
}

/// The power preference wgpu picks the window's graphics adapter with (#306): the
/// `WGPU_POWER_PREF` environment variable (`low`, `high`, `none`) when set, otherwise Preferences ›
/// Performance › Graphics Processor (`gpuPreference`). Power saving unless the preference asks for
/// high performance: the canvas is rasterized on the CPU and the GPU only composites it, which the
/// integrated GPU of a hybrid-graphics laptop does easily, while presenting frames rendered on the
/// discrete GPU through the integrated one made the window flicker on some laptops. With a single
/// GPU both preferences pick it.
#[cfg(feature = "wgpu")]
fn power_preference(pref: Option<&str>, env: Option<eframe::wgpu::PowerPreference>) -> eframe::wgpu::PowerPreference {
    use eframe::wgpu::PowerPreference;
    match (env, pref) {
        (Some(p), _) => p,
        (None, Some("highPerformance")) => PowerPreference::HighPerformance,
        (None, _) => PowerPreference::LowPower,
    }
}

/// Windows: the graphics backends to render with. Direct3D 12 when this PC has a DX12 adapter,
/// since some Intel Vulkan drivers crash as soon as the window is created (an access violation in
/// igvk64.dll on a hybrid Intel + NVIDIA laptop); otherwise wgpu's default (Vulkan, GL). The
/// `WGPU_BACKEND` environment variable still wins.
#[cfg(all(feature = "wgpu", windows))]
fn windows_backends() -> Option<eframe::wgpu::Backends> {
    use eframe::wgpu;
    if wgpu::Backends::from_env().is_some() {
        return None;
    }
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::DX12, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    // Native enumeration is synchronous; the future is ready on its first poll.
    let mut adapters = std::pin::pin!(instance.enumerate_adapters(wgpu::Backends::DX12));
    match adapters.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())) {
        std::task::Poll::Ready(found) if !found.is_empty() => Some(wgpu::Backends::DX12),
        _ => None,
    }
}

/// "name (backend, kind)" of the adapter the window renders with, for Help › About and bug reports.
#[cfg(feature = "wgpu")]
fn adapter_summary(info: &eframe::wgpu::AdapterInfo) -> String {
    format!("{} ({:?}, {:?})", info.name.trim(), info.backend, info.device_type)
}

/// Windows and Linux: no OS title bar; the app bar is the title bar (`vectorcraft_ui_egui::titlebar`).
/// macOS keeps its traffic lights over the integrated title strip.
const CUSTOM_TITLEBAR: bool = !cfg!(target_os = "macos");

fn main() -> eframe::Result {
    vectorcraft_ui_egui::i18n::detect_system_lang_in_background();
    let mut control_port: Option<u16> = std::env::var("VECTORCRAFT_CONTROL_PORT").ok().and_then(|p| p.parse().ok());
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--control" => control_port = args.next().and_then(|p| p.parse().ok()),
            "--version" => {
                println!("vectorcraft {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => files.push(a),
        }
    }
    let saved = read_prefs();
    let saved_window = saved.as_ref().and_then(|ui| ui.window);
    #[cfg(feature = "wgpu")]
    let gpu_pref = saved.as_ref().and_then(|ui| ui.engine_prefs.get("gpuPreference")).and_then(serde_json::Value::as_str);
    #[cfg(feature = "wgpu")]
    let power = power_preference(gpu_pref, eframe::wgpu::PowerPreference::from_env());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Roidery")
            .with_inner_size(window::DEFAULT_SIZE)
            .with_min_inner_size(window::MIN_SIZE)
            .with_drag_and_drop(true)
            .with_decorations(!CUSTOM_TITLEBAR)
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_icon(app_icon())
            .with_app_id("app.roidery.Roidery"),
        #[cfg(feature = "windows7")]
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    #[cfg(feature = "wgpu")]
    let options = {
        let mut options = options;
        if let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
            create.power_preference = power;
            #[cfg(windows)]
            if let Some(backends) = windows_backends() {
                create.instance_descriptor.backends = backends;
            }
        }
        options
    };
    // Files opened from Finder and the Dock arrive as events, not arguments.
    #[cfg(target_os = "macos")]
    open_documents::install();
    eframe::run_native(
        "Roidery",
        options,
        Box::new(move |cc| {
            let mut app = VectorcraftApp::new(Session::new(), services());
            load_prefs(&mut app, saved);
            // Fit the window to its monitor, or put it back where it was (still hidden).
            if let Some(w) = cc.winit_window() {
                app.ui.window = Some(window::restore(w, saved_window));
            }
            // User Defined swatch and graphic style libraries live next to the preferences.
            let swatches = prefs_path().and_then(|p| Some(p.parent()?.join("Swatches").to_string_lossy().to_string()));
            app.session.swatch_libraries.set_user_dir(swatches);
            let styles = prefs_path().and_then(|p| Some(p.parent()?.join("Graphic Styles").to_string_lossy().to_string()));
            app.session.style_libraries.set_user_dir(styles);
            // Data Recovery copies live next to the preferences too (none for runs without
            // preferences, such as agents' test runs, unless the recoveryFolder preference is set).
            if std::env::var_os("VECTORCRAFT_NO_PREFS").is_none() {
                let recovery = prefs_path().and_then(|p| Some(p.parent()?.join("Data Recovery").to_string_lossy().to_string()));
                app.session.recovery.set_default_folder(recovery);
            }
            let graphics_loss = GraphicsLoss::default();
            #[cfg(feature = "wgpu")]
            if let Some(rs) = &cc.wgpu_render_state {
                let summary = adapter_summary(&rs.adapter.get_info());
                log::info!("vectorcraft: rendering with {summary} (power preference {power:?})");
                app.graphics_adapter = Some(summary);
                // Roidery: the realistic view draws thread on the GPU.
                vectorcraft_ui_egui::thread_gpu::install(rs);
                let (loss, ctx) = (graphics_loss.clone(), cc.egui_ctx.clone());
                rs.device.set_device_lost_callback(move |reason, msg| loss.report(&ctx, format!("{reason:?}: {msg}")));
            }
            #[cfg(feature = "windows7")]
            {
                app.graphics_adapter = Some("OpenGL (Windows 7 compatibility)".into());
            }
            app.integrated_titlebar = cfg!(target_os = "macos");
            app.custom_titlebar = CUSTOM_TITLEBAR;
            if let Some(port) = control_port {
                let rx = control_server::start(port, cc.egui_ctx.clone());
                app = app.with_control(rx);
            }
            #[cfg(target_os = "macos")]
            open_documents::set_ui(&cc.egui_ctx);
            open_files(&mut app, files);
            Ok(Box::new(App {
                app,
                graphics_loss,
                graphics_lost: false,
                #[cfg(target_os = "macos")]
                menu: None,
            }))
        }),
    )
}

#[cfg(all(test, feature = "wgpu"))]
mod tests {
    use super::*;
    use eframe::wgpu::PowerPreference;

    #[test]
    fn power_saving_unless_the_preference_or_the_environment_says_otherwise() {
        assert_eq!(power_preference(None, None), PowerPreference::LowPower);
        assert_eq!(power_preference(Some("powerSaving"), None), PowerPreference::LowPower);
        assert_eq!(power_preference(Some("highPerformance"), None), PowerPreference::HighPerformance);
        // A value this version doesn't know (a newer or damaged preference file) is the default.
        assert_eq!(power_preference(Some("turbo"), None), PowerPreference::LowPower);
        assert_eq!(power_preference(Some(""), None), PowerPreference::LowPower);
        // WGPU_POWER_PREF wins over the preference, either way.
        assert_eq!(power_preference(Some("powerSaving"), Some(PowerPreference::HighPerformance)), PowerPreference::HighPerformance);
        assert_eq!(power_preference(Some("highPerformance"), Some(PowerPreference::LowPower)), PowerPreference::LowPower);
        assert_eq!(power_preference(None, Some(PowerPreference::None)), PowerPreference::None);
    }

    /// The file extensions the macOS bundle declares: its document types and its own exported type.
    fn plist_extensions(plist: &str) -> Vec<&str> {
        ["<key>CFBundleTypeExtensions</key>", "<key>public.filename-extension</key>"]
            .iter()
            .flat_map(|key| plist.split(key).skip(1))
            .filter_map(|rest| rest.split("</array>").next())
            .flat_map(|array| array.split("<string>").skip(1))
            .filter_map(|s| s.split("</string>").next())
            .collect()
    }

    /// Finder offers the app for every file File › Open reads (#295, #354), takes over no other
    /// app's files, and hands them to the app rather than to AppKit's document machinery.
    #[test]
    fn the_macos_bundle_opens_every_readable_format() {
        let plist = include_str!("../../../packaging/macos/Info.plist.in");
        let declared = plist_extensions(plist);
        for e in fileio::OPEN_EXTS {
            assert!(declared.contains(e), "Info.plist.in doesn't declare .{e}");
        }
        for e in &declared {
            assert!(fileio::OPEN_EXTS.contains(e), "Info.plist.in declares .{e}, which the app doesn't open");
        }
        assert!(!plist.contains("<key>NSDocumentClass</key>"), "not an NSDocument app: AppKit would refuse the files");
        let types = plist.matches("<key>CFBundleTypeName</key>").count();
        assert!(types > 1 && plist.matches("<key>LSHandlerRank</key>").count() == types, "every document type has a rank");
        assert_eq!(plist.matches("<string>Owner</string>").count(), 2, "only VectorCraft documents and templates are owned");
    }

    /// The preference the engine saves is the one the app reads back before the window opens.
    #[test]
    fn the_saved_engine_preference_is_found() {
        let mut prefs = vectorcraft_engine::Prefs::default();
        let saved = prefs.to_json();
        assert_eq!(saved.get("gpuPreference").and_then(serde_json::Value::as_str), Some("powerSaving"));
        prefs.gpu_preference = "highPerformance".into();
        let saved = prefs.to_json();
        let pref = saved.get("gpuPreference").and_then(serde_json::Value::as_str);
        assert_eq!(power_preference(pref, None), PowerPreference::HighPerformance);
    }
}
