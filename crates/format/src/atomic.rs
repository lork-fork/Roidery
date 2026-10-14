//! Safe saving: a file is written to a temporary file beside it, then renamed over it, so a failed
//! or interrupted write never leaves a damaged file behind (the old one stays as it was).

use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Write `bytes` to `path` atomically (see the module docs).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with(path, |f| f.write_all(bytes))
}

/// Write `path` atomically with `fill`, which writes the content into the temporary file. When
/// anything fails (`fill`, flushing, the rename) the temporary file is removed and `path` is left
/// untouched.
pub fn write_atomic_with(path: &Path, fill: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    let target = resolve_link(path)?;
    let tmp = temp_path(&target)?;
    let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    let filled = fill(&mut f).and_then(|()| f.sync_all());
    drop(f);
    filled.and_then(|()| keep_permissions(&target, &tmp)).and_then(|()| std::fs::rename(&tmp, &target)).inspect_err(|_| {
        // Best effort: the temporary file is all there is to clean up.
        let _ = std::fs::remove_file(&tmp);
    })
}

/// A symbolic link keeps pointing at its file: write the file it names.
fn resolve_link(path: &Path) -> io::Result<PathBuf> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => std::fs::canonicalize(path),
        _ => Ok(path.to_path_buf()),
    }
}

/// A fresh hidden name beside `target` (the same folder, so the rename stays on one volume).
fn temp_path(target: &Path) -> io::Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = target.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{}: not a file path", target.display())))?;
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    Ok(target.with_file_name(format!(".{}.{}-{n}.tmp", name.to_string_lossy(), std::process::id())))
}

/// The replacement keeps the replaced file's permissions (Unix; Windows files have none to keep
/// beyond read-only, which makes the save fail as writing in place would).
#[cfg(unix)]
fn keep_permissions(target: &Path, tmp: &Path) -> io::Result<()> {
    match std::fs::metadata(target) {
        Ok(m) => std::fs::set_permissions(tmp, m.permissions()),
        // A new file: the default permissions.
        Err(_) => Ok(()),
    }
}

#[cfg(not(unix))]
fn keep_permissions(_: &Path, _: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vc-atomic-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The other files in `d` besides `keep`.
    fn leftovers(d: &Path, keep: &str) -> Vec<String> {
        std::fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).filter(|n| n != keep).collect()
    }

    #[test]
    fn the_original_file_survives_a_failed_write() {
        let d = dir("fail");
        let path = d.join("doc.vectorcraft");
        std::fs::write(&path, b"original").unwrap();
        let r = write_atomic_with(&path, |f| {
            f.write_all(b"half a fi")?;
            Err(io::Error::other("disk full"))
        });
        assert!(r.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        assert!(leftovers(&d, "doc.vectorcraft").is_empty(), "the temporary file is removed");
        write_atomic(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert!(leftovers(&d, "doc.vectorcraft").is_empty());
        // A folder that doesn't exist: an error, nothing written.
        assert!(write_atomic(&d.join("missing").join("x.vectorcraft"), b"x").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
