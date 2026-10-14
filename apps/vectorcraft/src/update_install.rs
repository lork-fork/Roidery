//! Roidery: install a downloaded, checked update (Help › Check for Updates…).
//!
//! - Windows: the `.msi` is saved in the temp folder and Windows Installer runs it with a
//!   progress window; Roidery quits so the installer can replace it.
//! - macOS: the `.dmg` is saved in Downloads and opened; the user drags Roidery to Applications.
//! - Linux AppImage: the running AppImage (`$APPIMAGE`) is replaced; the next start is the new
//!   version.
//! - Anything else (a `.deb`, a `.zip`…): saved in Downloads and shown in its folder.

use std::path::{Path, PathBuf};

use vectorcraft_ui_egui::updates::Installed;

/// The user's Downloads folder, else the temp folder.
fn downloads() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    match home.map(|h| h.join("Downloads")) {
        Some(d) if d.is_dir() => d,
        _ => std::env::temp_dir(),
    }
}

fn save(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| format!("can't save {}: {e}", path.display()))?;
    Ok(path)
}

pub fn install(name: &str, kind: &str, bytes: &[u8]) -> Result<Installed, String> {
    match kind {
        #[cfg(windows)]
        "msi" => {
            let dir = std::env::temp_dir().join("Roidery Update");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let path = save(&dir, name, bytes)?;
            std::process::Command::new("msiexec")
                .arg("/i")
                .arg(&path)
                .arg("/passive")
                .spawn()
                .map_err(|e| format!("can't start the installer: {e}"))?;
            Ok(Installed { quit: true, message: format!("Installing {name}…") })
        }
        #[cfg(target_os = "linux")]
        "AppImage" if std::env::var_os("APPIMAGE").is_some() => {
            let current = std::env::var_os("APPIMAGE").map(PathBuf::from).ok_or("not running from an AppImage")?;
            replace_appimage(&current, bytes)?;
            Ok(Installed { quit: false, message: format!("Updated {}: open Roidery again to use the new version", current.display()) })
        }
        _ => {
            let path = save(&downloads(), name, bytes)?;
            let shown = path.to_string_lossy().to_string();
            #[cfg(target_os = "macos")]
            let opened = std::process::Command::new("open").arg(&path).spawn().map(|_| ());
            #[cfg(not(target_os = "macos"))]
            let opened = crate::reveal(&shown);
            opened.map_err(|e| format!("saved {shown}, but can't open it: {e}"))?;
            Ok(Installed { quit: false, message: format!("Saved {shown}") })
        }
    }
}

/// Write the new AppImage beside the running one, make it executable, then rename it over the
/// old one (a running AppImage can be replaced: the old file stays open until Roidery quits).
#[cfg(target_os = "linux")]
fn replace_appimage(current: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = current.parent().ok_or("the AppImage has no folder")?;
    let tmp = dir.join(".roidery-update.AppImage");
    std::fs::write(&tmp, bytes).map_err(|e| format!("can't write {}: {e}", tmp.display()))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, current).map_err(|e| format!("can't replace {}: {e}", current.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_kinds_are_saved() {
        let dir = std::env::temp_dir().join(format!("roidery-update-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = save(&dir, "roidery-9.0.0-linux-x86_64.deb", b"deb").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"deb");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_appimage_is_replaced_in_place() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = std::env::temp_dir().join(format!("roidery-appimage-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = dir.join("Roidery.AppImage");
        std::fs::write(&app, b"old").unwrap();
        replace_appimage(&app, b"new").unwrap();
        assert_eq!(std::fs::read(&app).unwrap(), b"new");
        assert_eq!(std::fs::metadata(&app).unwrap().permissions().mode() & 0o777, 0o755);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
