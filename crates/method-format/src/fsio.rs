//! Files on disk: atomic replacement and capped reads, shared by the app, the
//! command line and the change log (src/log.rs) so they all write the same
//! careful way.
//!
//! Plain `std::fs`, which still builds for WebAssembly (where every call
//! fails as unsupported); the browser build never calls any of it.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The most any whole-file read takes in: a budget, or one collaborator's log
/// file. Far beyond any real budget; a larger file is refused before it is
/// read into memory.
pub const MAX_READ_BYTES: u64 = 256 * 1024 * 1024;

/// Who may read a file written by [`write_atomically`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Only this user (0600 on Unix): recovery files, backups, the recent list.
    Private,
    /// A document: keeps the permissions of the file it replaces (a new file
    /// gets the usual ones).
    Document,
}

/// Creates `path`, which must not exist yet (not even as a link: a link is
/// never followed), for writing.
pub fn create_new(path: &Path, access: Access) -> io::Result<File> {
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    if access == Access::Private {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = access;
    o.open(path)
}

/// This process's id, in hex: temporary file names start with it, so a copy
/// of the app can tell its own from strays (`Recovery::tidy` in the app).
fn pid() -> String {
    // WebAssembly has no processes (and `std::process::id` would panic).
    #[cfg(target_arch = "wasm32")]
    let id = 0;
    #[cfg(not(target_arch = "wasm32"))]
    let id = std::process::id();
    format!("{id:x}")
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to `path` so that `path` is always either the old file or
/// the complete new one: write a temporary file in the same folder, flush it
/// to disk, then rename it over `path` (and flush the folder, on Unix). A
/// failed write leaves the old file alone and no temporary file behind.
pub fn write_atomically(path: &Path, bytes: &[u8], access: Access) -> io::Result<()> {
    write_atomically_with_prefix(path, bytes, access, ".")
}

/// [`write_atomically`], with the temporary file's name starting with
/// `prefix` (the change log uses `.~`, which sync services skip).
pub fn write_atomically_with_prefix(
    path: &Path,
    bytes: &[u8],
    access: Access,
    prefix: &str,
) -> io::Result<()> {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file name"))?;
    // A fresh name each time; one left over from an earlier run (the same
    // process id after a crash) is stepped over, never written through.
    let (tmp, mut f) = loop {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = dir.join(format!("{prefix}{name}.{}-{n:x}.tmp", pid()));
        match create_new(&tmp, access) {
            Ok(f) => break (tmp, f),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && n % 1000 != 999 => {}
            Err(e) => return Err(e),
        }
    };
    let result = (|| {
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        if access == Access::Document {
            if let Ok(meta) = fs::metadata(path) {
                // Best effort: a file on a share may refuse.
                let _ = fs::set_permissions(&tmp, meta.permissions());
            }
        }
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
        return result;
    }
    #[cfg(unix)]
    if let Ok(d) = File::open(&dir) {
        // The rename itself reaches the disk. Best effort: some file
        // systems can't sync a folder.
        let _ = d.sync_all();
    }
    Ok(())
}

/// Reads the whole file at `path`, refusing one larger than `limit` bytes
/// (the error's kind is `InvalidData` then) without reading it all in.
pub fn read_capped(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let f = File::open(path)?;
    let mut bytes = Vec::new();
    f.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "the file is larger than {} MB, too large to open",
                limit / (1024 * 1024)
            ),
        ));
    }
    Ok(bytes)
}

/// [`read_capped`], as text.
pub fn read_to_string_capped(path: &Path, limit: u64) -> io::Result<String> {
    String::from_utf8(read_capped(path, limit)?).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "stream did not contain valid UTF-8",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("method-fsio-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn replaces_and_leaves_nothing_behind() {
        let dir = temp_dir("replace");
        let p = dir.join("x.mthd");
        write_atomically(&p, b"one", Access::Document).unwrap();
        write_atomically_with_prefix(&p, b"two", Access::Document, ".~").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        // A failed rename (onto a folder) cleans up its temporary file.
        let folder = dir.join("folder");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("inside"), b"").unwrap();
        assert!(write_atomically(&folder, b"three", Access::Document).is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_planted_temporary_link_is_never_followed() {
        let dir = temp_dir("link");
        let victim = dir.join("victim");
        fs::write(&victim, b"keep").unwrap();
        // Links at the next few names this process could pick.
        let start = COUNTER.load(Ordering::Relaxed);
        for n in start..start + 5 {
            let name = format!(".x.{}-{n:x}.tmp", pid());
            std::os::unix::fs::symlink(&victim, dir.join(name)).unwrap();
        }
        write_atomically(&dir.join("x"), b"new", Access::Private).unwrap();
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
        assert_eq!(fs::read(dir.join("x")).unwrap(), b"new");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_are_capped() {
        let dir = temp_dir("cap");
        let p = dir.join("big");
        fs::write(&p, vec![b'a'; 100]).unwrap();
        assert_eq!(read_capped(&p, 100).unwrap().len(), 100);
        let e = read_capped(&p, 99).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        assert!(e.to_string().contains("too large"), "{e}");
        assert_eq!(read_to_string_capped(&p, 100).unwrap().len(), 100);
        let _ = fs::remove_dir_all(&dir);
    }
}
