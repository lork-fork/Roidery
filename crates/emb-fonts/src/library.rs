//! The embroidery font library: the font folders Roidery finds, read once and kept.
//!
//! Fonts are folders in Ink/Stitch's layout (`font.json`, `ltr.svg`, either may be gzipped, `LICENSE`,
//! `preview.png`), found in, first match wins:
//!
//! 1. the folders in `ROIDERY_EMBROIDERY_FONTS` (when Roidery runs, else when it was built);
//! 2. `embroidery-fonts` beside the program (where the installers put the bundled library),
//!    or in the macOS bundle's `Resources` or Linux's `share/roidery`;
//! 3. `roidery-fonts` beside the source checkout (a build from source);
//! 4. the user's own folder, `embroidery-fonts` in Roidery's preferences folder, where any
//!    Ink/Stitch font can be added.
//!
//! A library folder may hold `index.json` (written when the library is packaged) with each
//! font's [`Summary`], so the font list shows sizes and stitch types without reading every font.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::font::{Font, Summary, read_text};
use crate::info::FontInfo;

/// A font the library found.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The font's folder name.
    pub id: String,
    pub dir: PathBuf,
    pub info: FontInfo,
    /// From the folder's index, or once the font has been loaded.
    pub summary: Option<Summary>,
    pub preview: Option<PathBuf>,
    /// It is in the user's own folder.
    pub user: bool,
}

/// Most fonts a library folder lists.
const MAX_FONTS: usize = 2_000;

/// The user's own font folder (it may not exist yet).
pub fn user_folder() -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/Roidery"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Roidery"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|c| c.join("roidery"))
    };
    base.map(|b| b.join("embroidery-fonts"))
}

/// The library folders, in search order, as (folder, the user's own).
pub fn folders() -> Vec<(PathBuf, bool)> {
    let mut out: Vec<(PathBuf, bool)> = vec![];
    if cfg!(target_arch = "wasm32") {
        return out;
    }
    let listed = |v: &str| std::env::split_paths(v).collect::<Vec<_>>();
    if let Ok(v) = std::env::var("ROIDERY_EMBROIDERY_FONTS") {
        out.extend(listed(&v).into_iter().map(|p| (p, false)));
    } else if let Some(v) = option_env!("ROIDERY_EMBROIDERY_FONTS") {
        out.extend(listed(v).into_iter().map(|p| (p, false)));
    }
    if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        out.push((exe.join("embroidery-fonts"), false));
        out.push((exe.join("../Resources/embroidery-fonts"), false));
        out.push((exe.join("../share/roidery/embroidery-fonts"), false));
    }
    out.push((Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../roidery-fonts"), false));
    if let Some(u) = user_folder() {
        out.push((u, true));
    }
    out.retain(|(p, _)| p.is_dir());
    let mut seen: Vec<PathBuf> = vec![];
    out.retain(|(p, _)| {
        let c = p.canonicalize().unwrap_or_else(|_| p.clone());
        let new = !seen.contains(&c);
        seen.push(c);
        new
    });
    out
}

fn read_index(folder: &Path) -> HashMap<String, Summary> {
    let Ok(text) = read_text(&folder.join("index.json")) else { return HashMap::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return HashMap::new() };
    v.get("fonts")
        .and_then(serde_json::Value::as_object)
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), serde_json::from_value::<Summary>(v.clone()).ok()?))).collect())
        .unwrap_or_default()
}

/// The fonts in `folders`, in name order (the first of two with the same folder name wins).
pub fn scan(folders: &[(PathBuf, bool)]) -> Vec<Entry> {
    let mut out: Vec<Entry> = vec![];
    for (folder, user) in folders {
        let index = read_index(folder);
        let Ok(rd) = std::fs::read_dir(folder) else { continue };
        let mut dirs: Vec<PathBuf> = rd.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for dir in dirs.into_iter().take(MAX_FONTS) {
            let Some(id) = dir.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            if out.iter().any(|e| e.id == id) || crate::font::variant_files(&dir).is_empty() {
                continue;
            }
            let mut info = read_text(&crate::font::info_file(&dir)).ok().and_then(|t| FontInfo::parse(&t).ok()).unwrap_or_default();
            if info.name.is_empty() {
                info.name.clone_from(&id);
            }
            let preview = Some(dir.join("preview.png")).filter(|p| p.is_file());
            out.push(Entry { summary: index.get(&id).cloned(), id, dir, info, preview, user: *user });
        }
    }
    out.sort_by_key(|e| e.info.name.to_lowercase());
    out
}

#[derive(Default)]
struct State {
    entries: Option<Vec<Entry>>,
    fonts: HashMap<String, Arc<Font>>,
    failed: HashMap<String, String>,
}

fn state() -> std::sync::MutexGuard<'static, State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    // A panic while the lock was held leaves the cache as it was: still usable.
    S.get_or_init(Default::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The fonts the library has (scanned on first use).
pub fn entries() -> Vec<Entry> {
    let mut st = state();
    if st.entries.is_none() {
        st.entries = Some(scan(&folders()));
    }
    let loaded: HashMap<String, Summary> = st.fonts.iter().map(|(k, f)| (k.clone(), f.summary.clone())).collect();
    st.entries
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|mut e| {
            if let Some(s) = loaded.get(&e.id) {
                e.summary = Some(s.clone());
            }
            e
        })
        .collect()
}

/// Look for fonts again (after the user adds some).
pub fn rescan() {
    let mut st = state();
    st.entries = None;
    st.failed.clear();
}

/// Whether the library has a font with this id.
pub fn has(id: &str) -> bool {
    entry(id).is_some()
}

pub fn entry(id: &str) -> Option<Entry> {
    entries().into_iter().find(|e| e.id == id)
}

/// The font `id`, read on first use and kept.
pub fn load(id: &str) -> Result<Arc<Font>, String> {
    {
        let st = state();
        if let Some(f) = st.fonts.get(id) {
            return Ok(f.clone());
        }
        if let Some(e) = st.failed.get(id) {
            return Err(e.clone());
        }
    }
    let e = entry(id).ok_or_else(|| format!("unknown embroidery font `{id}`"))?;
    let mut font = Font::load(&e.dir);
    if let Ok(f) = &mut font {
        f.info.name.clone_from(&e.info.name);
    }
    let mut st = state();
    match font {
        Ok(f) => {
            let f = Arc::new(f);
            st.fonts.insert(id.to_string(), f.clone());
            Ok(f)
        }
        Err(err) => {
            st.failed.insert(id.to_string(), err.clone());
            Err(err)
        }
    }
}

/// Add a font that isn't in a folder (tests, scripts).
pub fn insert(font: Font) {
    let mut st = state();
    let entry =
        Entry { id: font.id.clone(), dir: PathBuf::new(), info: font.info.clone(), summary: Some(font.summary.clone()), preview: None, user: false };
    let mut entries = st.entries.take().unwrap_or_else(|| scan(&folders()));
    entries.retain(|e| e.id != entry.id);
    entries.push(entry);
    entries.sort_by_key(|e| e.info.name.to_lowercase());
    st.entries = Some(entries);
    st.fonts.insert(font.id.clone(), Arc::new(font));
}
