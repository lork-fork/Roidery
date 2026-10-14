//! Roidery's software updates.
//!
//! A release publishes an update feed next to its installers: a small JSON file naming the
//! version, what's new, and each download with its SHA-256 and size.
//!
//! ```json
//! {
//!   "app": "Roidery",
//!   "version": "0.5.0",
//!   "date": "2026-10-20",
//!   "notes": "What's new in this version.",
//!   "downloads": [
//!     {"platform": "windows-x64", "kind": "msi", "url": "https://…/roidery-0.5.0-windows-x64.msi",
//!      "sha256": "…", "size": 31457280}
//!   ]
//! }
//! ```
//!
//! The app reads the feed ([`Manifest::parse`]), compares versions ([`Manifest::is_newer_than`]),
//! picks the download for this computer ([`Manifest::pick`]), fetches it ([`fetch`], desktop only)
//! and checks its SHA-256 ([`verify`]) before anything runs it. `cargo xtask update-feed` writes
//! the feed from a release's files ([`describe_file`], [`feed_json`]).

use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The largest feed accepted.
pub const MAX_FEED: u64 = 1 << 20;
/// The largest download accepted.
pub const MAX_DOWNLOAD: u64 = 1 << 30;

/// One installer or archive in the feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Download {
    /// `<os>-<arch>`: `windows-x64`, `windows-arm64`, `windows-x86`, `macos-universal`,
    /// `macos-aarch64`, `linux-x86_64`, `freebsd-x86_64`…
    pub platform: String,
    /// The file type: `msi`, `zip`, `dmg`, `AppImage`, `deb`, `rpm` or `tar.gz`.
    pub kind: String,
    pub url: String,
    /// Lowercase hex.
    pub sha256: String,
    pub size: u64,
}

impl Download {
    /// The file name to save it as: the URL's last segment, kept to safe characters.
    pub fn file_name(&self) -> String {
        let last = self.url.split(['?', '#']).next().unwrap_or("").rsplit('/').next().unwrap_or("");
        let clean: String = last.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
        let clean = clean.trim_start_matches('.').to_string();
        if clean.is_empty() { format!("roidery-update.{}", self.kind) } else { clean }
    }
}

/// A parsed update feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub version: semver::Version,
    pub date: Option<String>,
    pub notes: String,
    pub downloads: Vec<Download>,
}

#[derive(Deserialize)]
struct RawDownload {
    platform: String,
    kind: String,
    url: String,
    sha256: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct RawManifest {
    #[serde(default)]
    app: Option<String>,
    version: String,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    downloads: Vec<RawDownload>,
}

/// Is `url` one the app may fetch: HTTPS, or plain HTTP to this computer (for testing a feed).
pub fn allowed_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://127.0.0.1") || lower.starts_with("http://localhost")
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The version as semver, without a leading `v`.
pub fn parse_version(v: &str) -> Result<semver::Version, String> {
    let v = v.trim();
    semver::Version::parse(v.strip_prefix('v').unwrap_or(v)).map_err(|e| format!("version `{v}`: {e}"))
}

impl Manifest {
    /// Read a feed. Downloads that aren't HTTPS, lack a SHA-256 or are too large are refused.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_FEED {
            return Err("the update feed is too large".into());
        }
        let raw: RawManifest = serde_json::from_slice(bytes).map_err(|e| format!("the update feed can't be read: {e}"))?;
        if let Some(app) = &raw.app
            && !app.eq_ignore_ascii_case("roidery")
        {
            return Err(format!("the update feed is for {app}, not Roidery"));
        }
        let mut downloads = Vec::new();
        for d in raw.downloads {
            if !allowed_url(&d.url) {
                return Err(format!("download `{}` isn't HTTPS", d.url));
            }
            if !is_sha256(&d.sha256) {
                return Err(format!("download `{}` has no SHA-256", d.url));
            }
            if d.size > MAX_DOWNLOAD {
                return Err(format!("download `{}` is too large", d.url));
            }
            downloads.push(Download { platform: d.platform, kind: d.kind, url: d.url, sha256: d.sha256.to_ascii_lowercase(), size: d.size });
        }
        Ok(Self { version: parse_version(&raw.version)?, date: raw.date, notes: raw.notes, downloads })
    }

    /// Is the feed's version later than `current`? (A version that can't be read is never older.)
    pub fn is_newer_than(&self, current: &str) -> bool {
        parse_version(current).is_ok_and(|c| self.version > c)
    }

    /// The download for this computer: the first of `platforms` (best first) that has one, and of
    /// its files the kind that installs best there. `appimage`: the app runs from an AppImage, which
    /// the update replaces.
    pub fn pick(&self, platforms: &[String], appimage: bool) -> Option<&Download> {
        platforms.iter().find_map(|p| {
            let os = p.split('-').next().unwrap_or("");
            kinds(os, appimage).iter().find_map(|k| self.downloads.iter().find(|d| &d.platform == p && d.kind.eq_ignore_ascii_case(k)))
        })
    }
}

/// The file kinds an OS installs from, best first.
fn kinds(os: &str, appimage: bool) -> &'static [&'static str] {
    match os {
        "windows" => &["msi", "zip"],
        "macos" => &["dmg"],
        "linux" if appimage => &["AppImage"],
        "linux" => &["AppImage", "deb", "rpm", "tar.gz"],
        _ => &["tar.gz"],
    }
}

/// The feed's platform names for an OS and CPU (`std::env::consts` spelling), best first.
pub fn platforms_for(os: &str, arch: &str) -> Vec<String> {
    let arch_name = |win: bool| match arch {
        "x86_64" if win => "x64",
        "x86" if win => "x86",
        "aarch64" if win => "arm64",
        other => other,
    };
    match os {
        "windows" => vec![format!("windows-{}", arch_name(true))],
        "macos" => vec!["macos-universal".into(), format!("macos-{arch}")],
        other => vec![format!("{other}-{}", arch_name(false))],
    }
}

/// This computer's platform names, best first.
pub fn platforms() -> Vec<String> {
    platforms_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Check a download against the SHA-256 the feed gave.
pub fn verify(bytes: &[u8], sha256: &str) -> Result<(), String> {
    if sha256_hex(bytes).eq_ignore_ascii_case(sha256) {
        Ok(())
    } else {
        Err("the download is damaged (its SHA-256 doesn't match the update feed)".into())
    }
}

/// A release file's platform and kind from its name, `roidery-<version>-<os>-<arch>[-portable].<ext>`
/// (`None` for other files: the CLI archive, the web zip, checksums).
pub fn describe_file(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix("roidery-")?;
    // The version is everything before `-<os>-`.
    let (os, version, after) =
        ["windows", "macos", "linux", "freebsd"].iter().find_map(|os| rest.split_once(&format!("-{os}-")).map(|(v, a)| (*os, v, a)))?;
    parse_version(version).ok()?;
    let kind = ["tar.gz", "AppImage", "msi", "zip", "dmg", "deb", "rpm"].into_iter().find(|k| after.ends_with(&format!(".{k}")))?;
    let stem = after.strip_suffix(&format!(".{kind}"))?;
    let arch = stem.strip_suffix("-portable").unwrap_or(stem);
    if arch.is_empty() || arch.contains('-') {
        return None;
    }
    Some((format!("{os}-{arch}"), kind.to_string()))
}

/// The feed for a release: `files` are (name, bytes) of its files; each one [`describe_file`]
/// knows becomes a download at `base_url` + name.
pub fn feed_json(version: &str, date: &str, notes: &str, base_url: &str, files: &[(String, Vec<u8>)]) -> Result<Value, String> {
    parse_version(version)?;
    let base = base_url.trim_end_matches('/');
    let downloads: Vec<Value> = files
        .iter()
        .filter_map(|(name, bytes)| {
            let (platform, kind) = describe_file(name)?;
            Some(json!({"platform": platform, "kind": kind, "url": format!("{base}/{name}"), "sha256": sha256_hex(bytes), "size": bytes.len()}))
        })
        .collect();
    if downloads.is_empty() {
        return Err("no installers among the files".into());
    }
    Ok(json!({"app": "Roidery", "version": version.trim_start_matches('v'), "date": date, "notes": notes, "downloads": downloads}))
}

/// Does `no_proxy` (NO_PROXY: comma-separated hosts or domain suffixes, `*` for all) say to reach
/// `url` directly? This computer always is.
pub fn no_proxy(url: &str, no_proxy: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host =
        if host.starts_with('[') { host.split(']').next().unwrap_or(host).trim_start_matches('[') } else { host.split(':').next().unwrap_or(host) };
    let host = host.to_ascii_lowercase();
    if matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1") {
        return true;
    }
    no_proxy
        .split(',')
        .map(|e| e.trim().trim_start_matches('*').trim_start_matches('.').to_ascii_lowercase())
        .any(|e| e.is_empty() && no_proxy.trim() == "*" || !e.is_empty() && (host == e || host.ends_with(&format!(".{e}"))))
}

/// Fetch `url` (at most `limit` bytes), reporting (bytes so far, total when known) as it goes.
#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
pub fn fetch(url: &str, limit: u64, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<Vec<u8>, String> {
    use std::io::Read;
    if !allowed_url(url) {
        return Err(format!("`{url}` isn't HTTPS"));
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(60))
        .user_agent(concat!("Roidery/", env!("CARGO_PKG_VERSION")))
        // HTTPS_PROXY / ALL_PROXY, as offices often need, except for hosts NO_PROXY names.
        .try_proxy_from_env(!no_proxy(url, &std::env::var("NO_PROXY").or_else(|_| std::env::var("no_proxy")).unwrap_or_default()))
        .build();
    let resp = agent.get(url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => format!("the server answered {code}"),
        other => format!("can't reach the update server: {other}"),
    })?;
    let total = resp.header("Content-Length").and_then(|v| v.trim().parse::<u64>().ok());
    if total.is_some_and(|t| t > limit) {
        return Err("the file is too large".into());
    }
    let mut reader = resp.into_reader().take(limit.saturating_add(1));
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("the download stopped: {e}"))?;
        let Some(chunk) = buf.get(..n) else { break };
        if n == 0 {
            break;
        }
        out.extend_from_slice(chunk);
        if out.len() as u64 > limit {
            return Err("the file is too large".into());
        }
        progress(out.len() as u64, total);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(version: &str) -> Vec<u8> {
        let sha = "ab".repeat(32);
        serde_json::to_vec(&json!({
            "app": "Roidery", "version": version, "notes": "Faster fills.",
            "downloads": [
                {"platform": "windows-x64", "kind": "zip", "url": "https://example.com/r/roidery-0.5.0-windows-x64-portable.zip", "sha256": sha, "size": 10},
                {"platform": "windows-x64", "kind": "msi", "url": "https://example.com/r/roidery-0.5.0-windows-x64.msi", "sha256": sha, "size": 10},
                {"platform": "macos-universal", "kind": "dmg", "url": "https://example.com/r/roidery-0.5.0-macos-universal.dmg", "sha256": sha, "size": 10},
                {"platform": "linux-x86_64", "kind": "deb", "url": "https://example.com/r/roidery-0.5.0-linux-x86_64.deb", "sha256": sha, "size": 10},
                {"platform": "linux-x86_64", "kind": "AppImage", "url": "https://example.com/r/roidery-0.5.0-linux-x86_64.AppImage", "sha256": sha, "size": 10}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn reads_a_feed_and_compares_versions() {
        let m = Manifest::parse(&feed("v0.5.0")).unwrap();
        assert_eq!(m.version.to_string(), "0.5.0");
        assert!(m.is_newer_than("0.4.0"));
        assert!(m.is_newer_than("0.5.0-rc.1"));
        assert!(!m.is_newer_than("0.5.0"));
        assert!(!m.is_newer_than("0.6.0"));
        assert!(!m.is_newer_than("nonsense"));
        assert_eq!(m.notes, "Faster fills.");
    }

    #[test]
    fn picks_the_installer_for_each_computer() {
        let m = Manifest::parse(&feed("0.5.0")).unwrap();
        let kind = |os, arch, appimage| m.pick(&platforms_for(os, arch), appimage).map(|d| d.kind.clone());
        assert_eq!(kind("windows", "x86_64", false).as_deref(), Some("msi"));
        assert_eq!(kind("macos", "aarch64", false).as_deref(), Some("dmg"));
        assert_eq!(kind("linux", "x86_64", true).as_deref(), Some("AppImage"));
        assert_eq!(kind("linux", "x86_64", false).as_deref(), Some("AppImage"));
        assert_eq!(kind("windows", "aarch64", false), None);
        assert_eq!(m.pick(&platforms_for("windows", "x86_64"), false).unwrap().file_name(), "roidery-0.5.0-windows-x64.msi");
    }

    #[test]
    fn refuses_unsafe_feeds() {
        let bad = |f: Value| Manifest::parse(&serde_json::to_vec(&f).unwrap());
        let sha = "ab".repeat(32);
        assert!(bad(json!({"version": "1.0.0", "downloads": [{"platform": "windows-x64", "kind": "msi", "url": "http://example.com/a.msi", "sha256": sha}]})).is_err());
        assert!(bad(json!({"version": "1.0.0", "downloads": [{"platform": "windows-x64", "kind": "msi", "url": "https://example.com/a.msi", "sha256": "abc"}]})).is_err());
        assert!(bad(json!({"app": "Other", "version": "1.0.0"})).is_err());
        assert!(bad(json!({"version": "one"})).is_err());
        assert!(Manifest::parse(b"{").is_err());
        assert!(Manifest::parse(&vec![b' '; (MAX_FEED + 1) as usize]).is_err());
    }

    #[test]
    fn file_names_stay_safe() {
        let d = |url: &str| Download { platform: "windows-x64".into(), kind: "msi".into(), url: url.into(), sha256: String::new(), size: 0 };
        assert_eq!(d("https://e.com/a/../..%2fevil.msi?x=1").file_name(), "2fevil.msi");
        assert_eq!(d("https://e.com/").file_name(), "roidery-update.msi");
        assert_eq!(d("https://e.com/..").file_name(), "roidery-update.msi");
    }

    #[test]
    fn no_proxy_hosts_are_reached_directly() {
        assert!(no_proxy("http://127.0.0.1:8899/feed.json", ""));
        assert!(no_proxy("https://updates.example.com/feed.json", "localhost, .example.com"));
        assert!(no_proxy("https://example.com/feed.json", "example.com"));
        assert!(no_proxy("https://a.b/feed.json", "*"));
        assert!(!no_proxy("https://github.com/feed.json", "example.com,localhost"));
        assert!(!no_proxy("https://notexample.com/feed.json", "example.com"));
    }

    #[test]
    fn checks_the_sha256() {
        let sha = sha256_hex(b"abc");
        assert_eq!(sha, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(verify(b"abc", &sha.to_uppercase()).is_ok());
        assert!(verify(b"abd", &sha).is_err());
    }

    #[test]
    fn describes_release_files_and_writes_the_feed() {
        let d = |n: &str| describe_file(n).map(|(p, k)| format!("{p} {k}"));
        assert_eq!(d("roidery-0.5.0-windows-x64.msi").as_deref(), Some("windows-x64 msi"));
        assert_eq!(d("roidery-0.5.0-rc.1-windows-arm64-portable.zip").as_deref(), Some("windows-arm64 zip"));
        assert_eq!(d("roidery-0.5.0-macos-universal.dmg").as_deref(), Some("macos-universal dmg"));
        assert_eq!(d("roidery-0.5.0-linux-aarch64.AppImage").as_deref(), Some("linux-aarch64 AppImage"));
        assert_eq!(d("roidery-0.5.0-freebsd-x86_64.tar.gz").as_deref(), Some("freebsd-x86_64 tar.gz"));
        assert_eq!(d("roidery-web-0.5.0.zip"), None);
        assert_eq!(d("roidery-cli-0.5.0-macos-universal.zip"), None);
        assert_eq!(d("SHA256SUMS"), None);
        let files = vec![("roidery-0.5.0-windows-x64.msi".to_string(), b"msi".to_vec()), ("notes.txt".to_string(), b"x".to_vec())];
        let v = feed_json("v0.5.0", "2026-10-20", "New.", "https://example.com/r/", &files).unwrap();
        let m = Manifest::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(m.downloads.len(), 1);
        assert_eq!(m.downloads[0].url, "https://example.com/r/roidery-0.5.0-windows-x64.msi");
        assert!(verify(b"msi", &m.downloads[0].sha256).is_ok());
        assert!(feed_json("0.5.0", "", "", "https://e.com", &[]).is_err());
    }
}
