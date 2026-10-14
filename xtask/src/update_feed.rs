//! Roidery: `cargo xtask update-feed --base-url URL [--notes FILE] [--date YYYY-MM-DD] [--out FILE] FILE...`
//! writes the update feed (`roidery-update.json`) for a release's files: each installer's
//! platform, kind, URL (base URL + its name), SHA-256 and size. Help › Check for Updates… reads it
//! (`roidery_update`).

use std::path::Path;

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask update-feed --base-url URL [--notes FILE] [--date YYYY-MM-DD] [--out FILE] FILE...";
    let (mut base, mut notes, mut date, mut out) = (None, String::new(), String::new(), "roidery-update.json".to_string());
    let mut files = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().map(|v| v.to_string()).ok_or(USAGE);
        match *a {
            "--base-url" => base = Some(value()?),
            "--notes" => {
                let f = value()?;
                notes = std::fs::read_to_string(&f).map_err(|e| format!("{f}: {e}"))?;
            }
            "--date" => date = value()?,
            "--out" => out = value()?,
            f => files.push(f.to_string()),
        }
    }
    let base = base.ok_or(USAGE)?;
    if !roidery_update::allowed_url(&base) {
        return Err(format!("--base-url must be HTTPS: {base}"));
    }
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).map_err(|e| format!("Cargo.toml: {e}"))?;
    let version = crate::version::read(&manifest)?;
    let mut read = Vec::new();
    for f in &files {
        let name = Path::new(f).file_name().and_then(|n| n.to_str()).ok_or_else(|| format!("{f}: no file name"))?;
        read.push((name.to_string(), std::fs::read(f).map_err(|e| format!("{f}: {e}"))?));
    }
    let feed = roidery_update::feed_json(&version, &date, notes.trim(), &base, &read)?;
    let text = serde_json::to_string_pretty(&feed).map_err(|e| e.to_string())?;
    std::fs::write(&out, text + "\n").map_err(|e| format!("{out}: {e}"))?;
    println!("wrote {out} ({} downloads)", feed["downloads"].as_array().map_or(0, Vec::len));
    Ok(())
}
