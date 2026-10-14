//! Roidery: software updates (Help › Check for Updates…).
//!
//! The app reads Roidery's update feed ([`roidery_update`]) off the UI thread, tells you when a
//! newer version is out, downloads the installer for this computer, checks its SHA-256 and hands
//! it to the host to install ([`Services::install_update`]): the Windows installer runs and
//! Roidery quits, macOS opens the disk image, an AppImage is replaced in place.
//!
//! It checks by itself once a day (Preferences › General › Check for updates automatically) when
//! the build has a feed: `ROIDERY_UPDATE_URL` when it was built, or at run time, or the
//! `updates.feed` setting. Without one, Check for Updates says updates aren't set up. The browser
//! version is always current and has no updates.
//!
//! Commands: `help.checkForUpdates {quiet?}`, `help.updateStatus`, `help.downloadUpdate`,
//! `help.installUpdate`, `help.skipUpdate`.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use roidery_update::{Download, MAX_DOWNLOAD, MAX_FEED, Manifest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::state::Dialog;
use crate::{VectorcraftApp, widgets};

/// Fetch a URL (at most the given bytes), reporting (bytes so far, total when known). Called on a
/// background thread.
pub type FetchFn = Arc<dyn Fn(&str, u64, &mut dyn FnMut(u64, Option<u64>)) -> Result<Vec<u8>, String> + Send + Sync>;

/// Install a checked download: (file name, kind, bytes) → whether Roidery should now quit, and
/// what to tell the user.
pub type InstallFn = Box<dyn FnMut(&str, &str, &[u8]) -> Result<Installed, String>>;

/// What installing did.
pub struct Installed {
    /// The installer takes over (Windows): quit so it can replace the app.
    pub quit: bool,
    pub message: String,
}

/// The dialog kind of Software Update.
pub const KIND: &str = "softwareUpdate";

/// Seconds between automatic checks.
const DAY: u64 = 24 * 60 * 60;
/// App seconds after start before the automatic check (the first frames stay quick).
const STARTUP_DELAY: f64 = 5.0;

/// The update settings kept with the UI state.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UpdatePrefs {
    /// Check once a day by itself.
    pub auto: bool,
    /// When it last checked (Unix seconds).
    pub last_check: u64,
    /// A version the user chose to skip: automatic checks don't offer it.
    pub skip: String,
    /// The update feed's URL (empty: the build's).
    pub feed: String,
}

impl Default for UpdatePrefs {
    fn default() -> Self {
        Self { auto: true, last_check: 0, skip: String::new(), feed: String::new() }
    }
}

/// Where the check is.
#[derive(Clone, Debug, Default)]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    /// A newer version, and the download for this computer (none: no installer for it).
    Available(Box<Manifest>, Option<Download>),
    Downloading(Box<Manifest>, Download, u64, Option<u64>),
    /// Downloaded and checked, ready to install.
    Ready(Box<Manifest>, Download, Arc<Vec<u8>>),
    Failed(String),
}

enum Event {
    Checked(Result<Manifest, String>),
    Progress(u64, Option<u64>),
    Downloaded(Result<Vec<u8>, String>),
}

/// The updater's state while the app runs.
#[derive(Default)]
pub struct Updater {
    pub phase: Phase,
    /// The running check started by itself: only a newer version is worth showing.
    quiet: bool,
    /// The automatic check of this run has been considered.
    started: bool,
    rx: Option<Receiver<Event>>,
}

/// The app's version.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The feed to read: the setting, else `ROIDERY_UPDATE_URL` now, else when built.
pub fn feed_url(app: &VectorcraftApp) -> Option<String> {
    let set = app.ui.updates.feed.trim();
    if !set.is_empty() {
        return Some(set.to_string());
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(v) = std::env::var("ROIDERY_UPDATE_URL")
        && !v.trim().is_empty()
    {
        return Some(v.trim().to_string());
    }
    option_env!("ROIDERY_UPDATE_URL").map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

fn now_unix() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

/// Run `job` off the UI thread, its events arriving at [`poll`].
fn spawn(app: &mut VectorcraftApp, job: impl FnOnce(&Sender<Event>) + Send + 'static) -> Result<(), String> {
    let (tx, rx) = channel();
    app.updates.rx = Some(rx);
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::Builder::new().name("roidery-update".into()).spawn(move || job(&tx)).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (job, tx);
        Err(tl!("The browser version is always up to date.").into())
    }
}

/// `help.checkForUpdates {quiet?}`: read the feed. Not quiet: the Software Update dialog opens and
/// shows the answer.
pub fn check(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    let quiet = p.get("quiet").and_then(Value::as_bool).unwrap_or(false);
    let fetch = app.services.fetch.clone();
    let (Some(fetch), Some(url)) = (fetch, feed_url(app)) else {
        let why = if app.services.fetch.is_none() {
            tl!("The browser version is always up to date.")
        } else {
            tl!("Updates aren't set up for this copy of Roidery.")
        };
        if !quiet {
            app.updates.phase = Phase::Failed(why.into());
            open_dialog(app);
        }
        return Err(why.into());
    };
    if matches!(app.updates.phase, Phase::Checking | Phase::Downloading(..)) {
        if !quiet {
            open_dialog(app);
        }
        return Ok(json!({"checking": true}));
    }
    app.updates.quiet = quiet;
    app.updates.phase = Phase::Checking;
    app.ui.updates.last_check = now_unix();
    if !quiet {
        open_dialog(app);
    }
    spawn(app, move |tx| {
        let r = fetch(&url, MAX_FEED, &mut |_, _| {}).and_then(|b| Manifest::parse(&b));
        let _ = tx.send(Event::Checked(r));
    })?;
    Ok(json!({"checking": true}))
}

/// `help.downloadUpdate`: fetch the installer the check found.
pub fn download(app: &mut VectorcraftApp) -> Result<Value, String> {
    let Phase::Available(m, Some(d)) = app.updates.phase.clone() else { return Err("no update to download".into()) };
    let fetch = app.services.fetch.clone().ok_or("no network")?;
    app.updates.phase = Phase::Downloading(m, d.clone(), 0, Some(d.size).filter(|s| *s > 0));
    let limit = if d.size > 0 { d.size } else { MAX_DOWNLOAD };
    spawn(app, move |tx| {
        let progress_tx = tx.clone();
        let r = fetch(&d.url, limit, &mut |n, total| {
            let _ = progress_tx.send(Event::Progress(n, total));
        })
        .and_then(|b| roidery_update::verify(&b, &d.sha256).map(|_| b));
        let _ = tx.send(Event::Downloaded(r));
    })?;
    Ok(json!({"downloading": true}))
}

/// `help.installUpdate`: hand the checked download to the system; quit when its installer takes
/// over.
pub fn install(app: &mut VectorcraftApp) -> Result<Value, String> {
    let Phase::Ready(m, d, bytes) = app.updates.phase.clone() else { return Err("no update downloaded".into()) };
    let installer = app.services.install_update.as_mut().ok_or("this copy of Roidery can't install updates")?;
    let done = installer(&d.file_name(), &d.kind, &bytes)?;
    app.status(done.message.clone());
    app.ui.dialog = None;
    app.updates.phase = Phase::Idle;
    if done.quit {
        crate::menus::invoke(app, "app.quit", json!({}));
    }
    Ok(json!({"version": m.version.to_string(), "quit": done.quit, "message": done.message}))
}

/// `help.skipUpdate`: don't offer this version again by itself.
pub fn skip(app: &mut VectorcraftApp) -> Result<Value, String> {
    if let Phase::Available(m, _) = &app.updates.phase {
        app.ui.updates.skip = m.version.to_string();
    }
    app.updates.phase = Phase::Idle;
    app.ui.dialog = None;
    Ok(Value::Null)
}

/// `help.updateStatus`: where the check is.
pub fn status(app: &VectorcraftApp) -> Value {
    let mut v = json!({"current": current_version(), "feed": feed_url(app), "auto": app.ui.updates.auto});
    let (phase, extra) = match &app.updates.phase {
        Phase::Idle => ("idle", json!({})),
        Phase::Checking => ("checking", json!({})),
        Phase::UpToDate => ("upToDate", json!({})),
        Phase::Available(m, d) => {
            ("available", json!({"version": m.version.to_string(), "notes": m.notes, "download": d.as_ref().map(|d| d.file_name())}))
        }
        Phase::Downloading(m, _, n, total) => ("downloading", json!({"version": m.version.to_string(), "bytes": n, "total": total})),
        Phase::Ready(m, d, _) => ("ready", json!({"version": m.version.to_string(), "download": d.file_name()})),
        Phase::Failed(e) => ("failed", json!({"error": e})),
    };
    v["phase"] = json!(phase);
    if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    v
}

fn open_dialog(app: &mut VectorcraftApp) {
    if app.ui.dialog.as_ref().is_none_or(|d| d.kind == KIND) {
        app.ui.dialog = Some(Dialog::new(KIND, json!({})));
    }
}

/// Each frame: take the background work's news, and start the daily check when it's due.
pub fn poll(app: &mut VectorcraftApp, now: f64) {
    let events: Vec<Event> = app.updates.rx.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
    for e in events {
        match e {
            Event::Checked(Ok(m)) => {
                if m.is_newer_than(current_version()) {
                    let d = m.pick(&roidery_update::platforms(), running_from_appimage()).cloned();
                    let offer = !app.updates.quiet || m.version.to_string() != app.ui.updates.skip;
                    app.updates.phase = Phase::Available(Box::new(m), d);
                    if app.updates.quiet && offer {
                        open_dialog(app);
                    }
                    if !offer {
                        app.updates.phase = Phase::Idle;
                    }
                } else {
                    app.updates.phase = Phase::UpToDate;
                }
            }
            Event::Checked(Err(e)) => {
                // A quiet check that fails (offline) says nothing.
                app.updates.phase = if app.updates.quiet { Phase::Idle } else { Phase::Failed(e) };
            }
            Event::Progress(n, total) => {
                if let Phase::Downloading(_, _, bytes, t) = &mut app.updates.phase {
                    *bytes = n;
                    *t = total.or(*t);
                }
            }
            Event::Downloaded(r) => {
                if let Phase::Downloading(m, d, ..) = app.updates.phase.clone() {
                    app.updates.phase = match r {
                        Ok(bytes) => Phase::Ready(m, d, Arc::new(bytes)),
                        Err(e) => Phase::Failed(e),
                    };
                }
            }
        }
    }
    if !app.updates.started && now >= STARTUP_DELAY {
        app.updates.started = true;
        let due = now_unix().saturating_sub(app.ui.updates.last_check) >= DAY;
        if app.ui.updates.auto && due && app.services.fetch.is_some() && feed_url(app).is_some() {
            let _ = check(app, &json!({"quiet": true}));
        }
    }
}

/// Is work running that needs the screen refreshed?
pub fn busy(app: &VectorcraftApp) -> bool {
    matches!(app.updates.phase, Phase::Checking | Phase::Downloading(..)) || (!app.updates.started && app.ui.updates.auto)
}

/// The app runs from an AppImage (Linux), which an update replaces.
fn running_from_appimage() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("APPIMAGE").is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

fn megabytes(n: u64) -> String {
    format!("{:.1}", n as f64 / 1_048_576.0)
}

/// The Software Update dialog's body: the state, and the buttons that move it on.
pub(crate) fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, _d: &mut Dialog) -> bool {
    let mut run: Option<&str> = None;
    let current = current_version();
    match app.updates.phase.clone() {
        Phase::Idle | Phase::Checking => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tl!("Checking for updates…"));
            });
        }
        Phase::UpToDate => {
            ui.label(tl!("Roidery {version} is up to date.").replace("{version}", current));
        }
        Phase::Failed(e) => {
            ui.label(tl!("Couldn't check for updates."));
            widgets::dim_label(ui, &e);
        }
        Phase::Available(m, d) => {
            ui.heading(tl!("Roidery {version} is available").replace("{version}", &m.version.to_string()));
            widgets::dim_label(ui, &tl!("You have {version}.").replace("{version}", current));
            notes(ui, &m.notes);
            ui.add_space(8.0);
            match &d {
                Some(d) => {
                    ui.horizontal(|ui| {
                        if ui.button(tl!("Skip This Version")).clicked() {
                            run = Some("help.skipUpdate");
                        }
                        let size = if d.size > 0 { format!(" ({} MB)", megabytes(d.size)) } else { String::new() };
                        if ui.add(egui::Button::new(format!("{}{size}", tl!("Download and Install"))).fill(ui.visuals().selection.bg_fill)).clicked()
                        {
                            run = Some("help.downloadUpdate");
                        }
                    });
                }
                None => {
                    widgets::dim_label(ui, tl!("There's no installer for this computer in this release."));
                }
            }
        }
        Phase::Downloading(m, _, n, total) => {
            ui.label(tl!("Downloading Roidery {version}…").replace("{version}", &m.version.to_string()));
            let frac = total.filter(|t| *t > 0).map_or(0.0, |t| n as f32 / t as f32);
            ui.add(egui::ProgressBar::new(frac.clamp(0.0, 1.0)).desired_width(320.0).text(format!("{} MB", megabytes(n))));
        }
        Phase::Ready(m, d, _) => {
            ui.label(tl!("Roidery {version} is ready to install.").replace("{version}", &m.version.to_string()));
            let what = match d.kind.as_str() {
                "msi" => tl!("Roidery quits and the installer opens. Save your work first."),
                "dmg" => tl!("The disk image opens: drag Roidery to Applications."),
                "AppImage" if running_from_appimage() => tl!("Roidery is replaced; open it again to use the new version."),
                _ => tl!("The download opens in its folder."),
            };
            widgets::dim_label(ui, what);
            ui.add_space(8.0);
            if ui.add(egui::Button::new(tl!("Install")).fill(ui.visuals().selection.bg_fill)).clicked() {
                run = Some("help.installUpdate");
            }
        }
    }
    if let Some(id) = run {
        crate::menus::invoke(app, id, json!({}));
    }
    false
}

/// What's new, in a scroll area so long notes don't push the buttons off screen.
fn notes(ui: &mut egui::Ui, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    ui.add_space(6.0);
    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
        ui.label(text);
    });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use vectorcraft_engine::Session;

    use super::*;
    use crate::Services;

    fn feed(version: &str, installer: &[u8]) -> Vec<u8> {
        let platform = roidery_update::platforms().remove(0);
        let kind = match std::env::consts::OS {
            "windows" => "msi",
            "macos" => "dmg",
            "linux" => "AppImage",
            _ => "tar.gz",
        };
        serde_json::to_vec(&json!({
            "app": "Roidery", "version": version, "notes": "Faster fills.",
            "downloads": [{"platform": platform, "kind": kind, "url": format!("https://example.com/roidery-{version}.{kind}"),
                "sha256": roidery_update::sha256_hex(installer), "size": installer.len()}]
        }))
        .unwrap()
    }

    /// An app whose network serves `feed` and `installer`.
    fn app(feed: Vec<u8>, installer: Vec<u8>) -> VectorcraftApp {
        let fetch: FetchFn = Arc::new(move |url: &str, _limit, progress: &mut dyn FnMut(u64, Option<u64>)| {
            if url.ends_with(".json") {
                Ok(feed.clone())
            } else {
                progress(installer.len() as u64, Some(installer.len() as u64));
                Ok(installer.clone())
            }
        });
        let mut app = VectorcraftApp::new(Session::new(), Services { fetch: Some(fetch), ..Default::default() });
        app.ui.updates.feed = "https://example.com/latest.json".into();
        app
    }

    /// Poll until the background work settles.
    fn settle(app: &mut VectorcraftApp) {
        for _ in 0..500 {
            poll(app, 0.0);
            if !matches!(app.updates.phase, Phase::Checking | Phase::Downloading(..)) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    #[test]
    fn finds_downloads_checks_and_installs_a_newer_version() {
        let mut app = app(feed("99.0.0", b"installer"), b"installer".to_vec());
        let installed = Rc::new(RefCell::new(Vec::new()));
        let i = installed.clone();
        app.services.install_update = Some(Box::new(move |name: &str, _kind: &str, bytes: &[u8]| {
            i.borrow_mut().push((name.to_string(), bytes.to_vec()));
            Ok(Installed { quit: false, message: "Installed".into() })
        }));
        app.run("help.checkForUpdates", json!({})).unwrap();
        assert_eq!(app.ui.dialog.as_ref().map(|d| d.kind.as_str()), Some(KIND));
        settle(&mut app);
        let s = app.run("help.updateStatus", json!({})).unwrap();
        assert_eq!((s["phase"].as_str(), s["version"].as_str(), s["notes"].as_str()), (Some("available"), Some("99.0.0"), Some("Faster fills.")));
        app.run("help.downloadUpdate", json!({})).unwrap();
        settle(&mut app);
        assert_eq!(app.run("help.updateStatus", json!({})).unwrap()["phase"], "ready");
        let r = app.run("help.installUpdate", json!({})).unwrap();
        assert_eq!(r["quit"], false);
        assert_eq!(installed.borrow()[0].1, b"installer");
        assert!(app.ui.dialog.is_none());
    }

    #[test]
    fn a_damaged_download_is_refused() {
        let mut app = app(feed("99.0.0", b"installer"), b"tampered".to_vec());
        app.run("help.checkForUpdates", json!({})).unwrap();
        settle(&mut app);
        app.run("help.downloadUpdate", json!({})).unwrap();
        settle(&mut app);
        let s = app.run("help.updateStatus", json!({})).unwrap();
        assert_eq!(s["phase"], "failed");
        assert!(s["error"].as_str().unwrap().contains("SHA-256"));
        assert!(app.run("help.installUpdate", json!({})).is_err());
    }

    #[test]
    fn up_to_date_and_skipped_versions() {
        let mut app = app(feed("0.0.1", b"x"), vec![]);
        app.run("help.checkForUpdates", json!({})).unwrap();
        settle(&mut app);
        assert_eq!(app.run("help.updateStatus", json!({})).unwrap()["phase"], "upToDate");
        // A quiet check doesn't offer a skipped version, and opens nothing.
        let mut app2 = self::app(feed("99.0.0", b"x"), vec![]);
        app2.ui.updates.skip = "99.0.0".into();
        app2.run("help.checkForUpdates", json!({"quiet": true})).unwrap();
        settle(&mut app2);
        assert!(app2.ui.dialog.is_none());
        assert_eq!(app2.run("help.updateStatus", json!({})).unwrap()["phase"], "idle");
        // Asking again offers it.
        app2.run("help.checkForUpdates", json!({})).unwrap();
        settle(&mut app2);
        assert_eq!(app2.run("help.updateStatus", json!({})).unwrap()["phase"], "available");
        app2.run("help.skipUpdate", json!({})).unwrap();
        assert_eq!(app2.ui.updates.skip, "99.0.0");
    }

    #[test]
    fn the_daily_check_runs_once_after_startup() {
        let mut app = app(feed("99.0.0", b"x"), vec![]);
        poll(&mut app, 1.0);
        assert!(matches!(app.updates.phase, Phase::Idle));
        poll(&mut app, STARTUP_DELAY);
        settle(&mut app);
        assert!(matches!(app.updates.phase, Phase::Available(..)));
        assert_eq!(app.ui.dialog.as_ref().map(|d| d.kind.as_str()), Some(KIND));
        assert!(app.ui.updates.last_check > 0);
        // Not again within a day.
        let mut again = self::app(feed("99.0.0", b"x"), vec![]);
        again.ui.updates.last_check = now_unix();
        poll(&mut again, STARTUP_DELAY);
        assert!(matches!(again.updates.phase, Phase::Idle));
    }

    #[test]
    fn without_a_feed_it_says_so() {
        let mut app = VectorcraftApp::new(
            Session::new(),
            Services { fetch: Some(Arc::new(|_: &str, _, _: &mut dyn FnMut(u64, Option<u64>)| Ok(vec![]))), ..Default::default() },
        );
        if feed_url(&app).is_some() {
            return; // built with a feed
        }
        assert!(app.run("help.checkForUpdates", json!({})).is_err());
        assert_eq!(app.run("help.updateStatus", json!({})).unwrap()["phase"], "failed");
        assert_eq!(app.ui.dialog.as_ref().map(|d| d.kind.as_str()), Some(KIND));
    }
}
