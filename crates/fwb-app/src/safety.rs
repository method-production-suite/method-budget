//! Keeping work safe: atomic saves, autosave (into the file, or into a
//! recovery copy), backups of earlier versions, and the recent files list.
//!
//! Everything here is plain files in a folder handed in by the caller (the
//! app's local data folder in the app, a temporary folder in tests), so it is
//! testable without a window. The Tauri side is `src/safety_app.rs`; the
//! behavior is described in docs/APP.md ("Saving and recovery").
//!
//! Layout of the app's local data folder:
//!
//! ```text
//! Recovery/<key>.fwb          the budget as it was at the last autosave
//!                             (internal, never shown: keeps its pre-rename
//!                             extension so migrated copies read the same way)
//! Recovery/<key>.json         what it is: original path, title, time, owner
//! Recovery/owner-<id>.lock    held (locked) by each running copy of the app
//! Backups/<path hash>/        the last few versions of one document, as
//!                             they were on disk before each save replaced them
//! recent.json                 File > Open Recent
//! settings.json               the Autosave switch
//! tabs.json                   the budgets open as tabs, to reopen next time
//! ```

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Autosave waits for this long without a change...
pub const AUTOSAVE_QUIET: Duration = Duration::from_secs(3);
/// ...but never longer than this after the first change it hasn't saved.
pub const AUTOSAVE_MAX_WAIT: Duration = Duration::from_secs(30);
/// File > Open Recent shows this many files.
pub const RECENT_LIMIT: usize = 10;
/// Backups kept per document.
pub const BACKUPS_KEPT: usize = 10;
/// Autosaves into a file back up its previous version at most this often
/// (an explicit Save always does, and so does the first write in a session).
pub const BACKUP_GAP: Duration = Duration::from_secs(10 * 60);

/// Whether an autosave is due: the budget has been quiet for
/// [`AUTOSAVE_QUIET`] since `last_change`, or [`AUTOSAVE_MAX_WAIT`] has passed
/// since `pending_since`, the first change not yet in the recovery file.
pub fn autosave_due(now: Instant, last_change: Instant, pending_since: Option<Instant>) -> bool {
    let waited = |t: Instant| now.saturating_duration_since(t);
    waited(last_change) >= AUTOSAVE_QUIET
        || pending_since.is_some_and(|t| waited(t) >= AUTOSAVE_MAX_WAIT)
}

// ---------------------------------------------------------------------------
// Names

/// A stable 64-bit FNV-1a hash of a path, as 16 hex digits. Stable across
/// runs and Rust versions (std's hasher is not), so a document's backups
/// stay in one folder.
pub fn path_hash(path: &Path) -> String {
    format!(
        "{:016x}",
        content_hash(path.as_os_str().to_string_lossy().as_bytes())
    )
}

fn unique() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!(
        "{:x}-{nanos:x}-{:x}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

/// A fresh recovery key for a budget: unique to this open document, so two
/// copies of the app (or an old crash's file for the same path) never write
/// over each other. It starts with the path's hash, or `untitled`.
pub fn new_recovery_key(path: Option<&Path>) -> String {
    let base = path.map(path_hash).unwrap_or_else(|| "untitled".into());
    format!("{base}-{}", unique())
}

/// Keys name files in the recovery folder, and the web view hands them back
/// (to open or discard an entry), so only plain names are accepted.
pub fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 96
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `2026-10-09 14.03.22.123` (UTC) for backup file names: sorts in time
/// order and is legal in file names everywhere.
pub fn utc_stamp(ms: u64) -> String {
    let secs = ms / 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}.{:02}.{:02}.{:03}",
        rem / 3600,
        rem / 60 % 60,
        rem % 60,
        ms % 1000
    )
}

// ---------------------------------------------------------------------------
// Writing files

/// Who may read a file written by [`write_atomically`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Only this user (0600 on Unix): recovery files, backups, the recent list.
    Private,
    /// A document: keeps the permissions of the file it replaces (a new file
    /// gets the usual ones).
    Document,
}

fn create_new(path: &Path, access: Access) -> io::Result<File> {
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

/// Creates a folder only this user can open (0700 on Unix).
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut b = fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir)?;
    #[cfg(unix)]
    {
        // A folder that already existed (made by something else) is closed
        // up too. Only ever called on the app's own folders.
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// Writes `bytes` to `path` so that `path` is always either the old file or
/// the complete new one: write a temporary file in the same folder, flush it
/// to disk, then rename it over `path` (and flush the folder, on Unix). A
/// failed write leaves the old file alone and no temporary file behind.
pub fn write_atomically(path: &Path, bytes: &[u8], access: Access) -> io::Result<()> {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "budget".into());
    let tmp = dir.join(format!(".{name}.{}.tmp", unique()));
    let result = (|| {
        let mut f = create_new(&tmp, access)?;
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

/// Whether two paths name the same file: the same path, or the same file
/// once links and `..` are resolved (only files that exist can be resolved).
pub fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (fs::canonicalize(a), fs::canonicalize(b)),
            (Ok(x), Ok(y)) if x == y
        )
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Changes made outside the app

/// A 64-bit FNV-1a hash of file content: enough to tell versions apart.
pub fn content_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// What a budget file looked like when the app last read or wrote it, to
/// notice another program (a sync service, another computer, another app)
/// changing it since.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskStamp {
    pub len: u64,
    pub modified: Option<SystemTime>,
    pub hash: u64,
}

/// How a file on disk compares with its [`DiskStamp`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiskCheck {
    /// The content we last read or wrote (`Some`: its time changed, here is
    /// the stamp to remember instead).
    Same(Option<DiskStamp>),
    /// Different content.
    Changed,
    /// The file is gone (moved or deleted); a save puts it back.
    Missing,
}

impl DiskStamp {
    /// The stamp of `bytes`, just read from or written to `path`.
    pub fn of(path: &Path, bytes: &[u8]) -> DiskStamp {
        let meta = fs::metadata(path).ok();
        DiskStamp {
            len: meta.as_ref().map_or(bytes.len() as u64, |m| m.len()),
            modified: meta.and_then(|m| m.modified().ok()),
            hash: content_hash(bytes),
        }
    }

    /// Compares `path` with this stamp. Size and time unchanged means the
    /// same file without reading it; otherwise the content decides (a sync
    /// service may touch a file without changing it).
    pub fn check(&self, path: &Path) -> DiskCheck {
        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return DiskCheck::Missing,
            Err(_) => return DiskCheck::Changed,
        };
        let modified = meta.modified().ok();
        if meta.len() == self.len && modified.is_some() && modified == self.modified {
            return DiskCheck::Same(None);
        }
        match fs::read(path) {
            Ok(bytes) if content_hash(&bytes) == self.hash && bytes.len() as u64 == self.len => {
                DiskCheck::Same(Some(DiskStamp::of(path, &bytes)))
            }
            Ok(_) => DiskCheck::Changed,
            Err(e) if e.kind() == io::ErrorKind::NotFound => DiskCheck::Missing,
            Err(_) => DiskCheck::Changed,
        }
    }
}

// ---------------------------------------------------------------------------
// Recovery files

/// The sidecar next to each recovery file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryMeta {
    pub key: String,
    /// Where the budget lives on disk, `None` for one never saved.
    pub path: Option<String>,
    pub title: String,
    pub saved_at_ms: u64,
    /// The lock file of the copy of the app that wrote it.
    pub owner: String,
}

/// What [`RecoveryStore::list`] finds: a recovery file left by a copy of the
/// app that is no longer running.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RecoveryEntry {
    pub key: String,
    pub path: Option<String>,
    /// File name, or "Untitled".
    pub name: String,
    pub title: String,
    pub saved_at_ms: u64,
}

/// The recovery folder. Each running copy of the app holds a lock on its own
/// `owner-*.lock` file for as long as it runs, so a listing can tell a
/// crashed copy's files (lock free) from a running copy's (lock held).
#[derive(Debug)]
pub struct RecoveryStore {
    dir: PathBuf,
    owner: String,
    /// Held open (and locked) until the store is dropped.
    _lock: Option<File>,
}

impl RecoveryStore {
    /// Opens (creating if needed) the recovery folder `dir` and takes this
    /// copy's owner lock.
    pub fn open(dir: &Path) -> io::Result<RecoveryStore> {
        create_private_dir(dir)?;
        let owner = format!("owner-{}.lock", unique());
        let file = create_new(&dir.join(&owner), Access::Private)?;
        // If locks aren't supported here (some network folders), recovery
        // still works; only the running-copy check is lost.
        let lock = file.try_lock().is_ok().then_some(file);
        Ok(RecoveryStore {
            dir: dir.to_path_buf(),
            owner,
            _lock: lock,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn budget_file(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.fwb"))
    }

    fn meta_file(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }

    /// Writes (or replaces) the recovery file for `key`: the budget first,
    /// then its sidecar, each atomically, readable only by this user.
    pub fn write(
        &self,
        key: &str,
        budget_text: &str,
        path: Option<&Path>,
        title: &str,
    ) -> io::Result<()> {
        if !valid_key(key) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "bad recovery key",
            ));
        }
        write_atomically(
            &self.budget_file(key),
            budget_text.as_bytes(),
            Access::Private,
        )?;
        let meta = RecoveryMeta {
            key: key.to_string(),
            path: path.and_then(Path::to_str).map(str::to_string),
            title: title.to_string(),
            saved_at_ms: now_ms(),
            owner: self.owner.clone(),
        };
        self.write_meta(&meta)
    }

    fn write_meta(&self, meta: &RecoveryMeta) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(meta).map_err(io::Error::other)?;
        write_atomically(&self.meta_file(&meta.key), &json, Access::Private)
    }

    /// Deletes the recovery file for `key`, if there is one.
    pub fn remove(&self, key: &str) -> io::Result<()> {
        if !valid_key(key) {
            return Ok(());
        }
        // Sidecar first: a budget file without one is never listed.
        remove_if_present(&self.meta_file(key))?;
        remove_if_present(&self.budget_file(key))
    }

    fn read_meta(&self, key: &str) -> Option<RecoveryMeta> {
        let text = fs::read_to_string(self.meta_file(key)).ok()?;
        let meta: RecoveryMeta = serde_json::from_str(&text).ok()?;
        (meta.key == key).then_some(meta)
    }

    /// The budget text and sidecar for `key`, if it is listed (left by a copy
    /// of the app that is no longer running).
    pub fn read(&self, key: &str) -> io::Result<(String, RecoveryMeta)> {
        let missing = || io::Error::new(io::ErrorKind::NotFound, "That recovered budget is gone.");
        if !valid_key(key) || !self.list().iter().any(|e| e.key == key) {
            return Err(missing());
        }
        let meta = self.read_meta(key).ok_or_else(missing)?;
        let text = fs::read_to_string(self.budget_file(key))?;
        Ok((text, meta))
    }

    /// Makes `key` this copy's own (after opening it), so another copy of
    /// the app starting now doesn't offer it too.
    pub fn adopt(&self, key: &str) -> io::Result<()> {
        let mut meta = self
            .read_meta(key)
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        meta.owner = self.owner.clone();
        self.write_meta(&meta)
    }

    /// Is the copy of the app that holds `owner` still running?
    fn owner_running(&self, owner: &str) -> bool {
        if owner == self.owner {
            return true;
        }
        if !owner.starts_with("owner-") || owner.contains(['/', '\\']) {
            return false;
        }
        let Ok(f) = File::open(self.dir.join(owner)) else {
            return false;
        };
        matches!(f.try_lock(), Err(fs::TryLockError::WouldBlock))
    }

    /// Recovery files left by copies of the app that are no longer running,
    /// newest first.
    pub fn list(&self) -> Vec<RecoveryEntry> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out: Vec<RecoveryEntry> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let key = name.strip_suffix(".json")?;
                if !valid_key(key) {
                    return None;
                }
                let meta = self.read_meta(key)?;
                if self.owner_running(&meta.owner) || !self.budget_file(key).is_file() {
                    return None;
                }
                let name = meta
                    .path
                    .as_deref()
                    .and_then(|p| Path::new(p).file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled".into());
                Some(RecoveryEntry {
                    key: meta.key,
                    path: meta.path,
                    name,
                    title: meta.title,
                    saved_at_ms: meta.saved_at_ms,
                })
            })
            .collect();
        out.sort_by(|a, b| b.saved_at_ms.cmp(&a.saved_at_ms).then(a.key.cmp(&b.key)));
        out
    }

    /// Removes lock files of copies that are no longer running, and stray
    /// temporary files. Recovery files themselves are only ever removed by
    /// the user's choice (or a save).
    pub fn tidy(&self) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        for e in entries.flatten() {
            let Ok(name) = e.file_name().into_string() else {
                continue;
            };
            let stale_owner =
                name.starts_with("owner-") && name.ends_with(".lock") && !self.owner_running(&name);
            let stray_tmp = name.starts_with('.')
                && name.ends_with(".tmp")
                && !name.contains(&format!(".{:x}-", std::process::id()));
            if stale_owner || stray_tmp {
                let _ = fs::remove_file(e.path());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Backups

/// One backup, for File > Revert to Backup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BackupEntry {
    /// The file name, which is also how the UI names it back.
    pub id: String,
    /// When it was taken (the time the save replaced this version).
    pub saved_at_ms: u64,
    pub bytes: u64,
}

/// The last [`BACKUPS_KEPT`] versions of each document, as they were on disk
/// before a save replaced them: `Backups/<path hash>/<name> <time> UTC.mthd`,
/// with a `source.json` naming the document. Backups taken before the rename
/// to Method Budget end in `.fwb` and are listed alongside.
#[derive(Debug)]
pub struct Backups {
    dir: PathBuf,
}

impl Backups {
    pub fn new(dir: &Path) -> Backups {
        Backups {
            dir: dir.to_path_buf(),
        }
    }

    /// The folder holding every document's backups.
    pub fn root(&self) -> &Path {
        &self.dir
    }

    /// The folder holding `document`'s backups.
    pub fn folder(&self, document: &Path) -> PathBuf {
        self.dir.join(path_hash(document))
    }

    /// `document`'s backups, oldest first.
    pub fn list(&self, document: &Path) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(self.folder(document)) else {
            return Vec::new();
        };
        let mut v: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| fwb_format::has_budget_extension(p))
            .collect();
        // Names end in a fixed-width UTC time, after the same document name.
        v.sort();
        v
    }

    /// `document`'s backups for the UI, newest first.
    pub fn entries(&self, document: &Path) -> Vec<BackupEntry> {
        let mut v: Vec<BackupEntry> = self
            .list(document)
            .into_iter()
            .filter_map(|p| {
                let meta = fs::metadata(&p).ok()?;
                let saved_at_ms = meta
                    .modified()
                    .ok()?
                    .duration_since(UNIX_EPOCH)
                    .ok()?
                    .as_millis() as u64;
                Some(BackupEntry {
                    id: p.file_name()?.to_str()?.to_string(),
                    saved_at_ms,
                    bytes: meta.len(),
                })
            })
            .collect();
        v.reverse();
        v
    }

    /// The text of `document`'s backup `id` (a name from [`Backups::entries`]).
    pub fn read(&self, document: &Path, id: &str) -> io::Result<String> {
        let found = self
            .list(document)
            .into_iter()
            .find(|p| p.file_name().is_some_and(|n| n == id))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "That backup is gone."))?;
        fs::read_to_string(found)
    }

    /// Copies `document` as it is on disk now into its backups, before a save
    /// replaces it, and keeps only the newest [`BACKUPS_KEPT`]. Nothing to do
    /// when the file doesn't exist yet, when the newest backup is the same,
    /// or when the newest backup is younger than `min_gap`.
    pub fn back_up(
        &self,
        document: &Path,
        min_gap: Option<Duration>,
    ) -> io::Result<Option<PathBuf>> {
        let existing = self.list(document);
        if let (Some(gap), Some(newest)) = (min_gap, existing.last()) {
            let age = fs::metadata(newest)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok());
            if age.is_some_and(|a| a < gap) {
                return Ok(None);
            }
        }
        let bytes = match fs::read(document) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        self.add(document, &bytes)
    }

    /// Keeps `bytes` as a backup of `document` (another version of it, such
    /// as the one open in the app), unless the newest backup is the same.
    pub fn add(&self, document: &Path, bytes: &[u8]) -> io::Result<Option<PathBuf>> {
        let existing = self.list(document);
        if let Some(newest) = existing.last() {
            if fs::read(newest).is_ok_and(|b| b == bytes) {
                return Ok(None);
            }
        }
        let folder = self.folder(document);
        create_private_dir(&folder)?;
        let source = serde_json::json!({ "path": document.to_string_lossy() });
        write_atomically(
            &folder.join("source.json"),
            source.to_string().as_bytes(),
            Access::Private,
        )?;
        let stem = document
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "budget".into());
        let mut ms = now_ms();
        let name = |ms| format!("{stem} {} UTC.{}", utc_stamp(ms), fwb_format::EXTENSION);
        let mut target = folder.join(name(ms));
        while target.exists() {
            ms += 1;
            target = folder.join(name(ms));
        }
        write_atomically(&target, bytes, Access::Private)?;
        let all = self.list(document);
        for old in all.iter().take(all.len().saturating_sub(BACKUPS_KEPT)) {
            let _ = fs::remove_file(old);
        }
        Ok(Some(target))
    }
}

// ---------------------------------------------------------------------------
// Settings

/// App-wide preferences, in `settings.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Save into the budget's file a few seconds after each change (budgets
    /// with a file; others get a recovery copy). On unless turned off.
    pub autosave: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { autosave: true }
    }
}

impl Settings {
    /// Reads `file`; missing or unreadable means the defaults.
    pub fn load(file: &Path) -> Settings {
        fs::read_to_string(file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> io::Result<()> {
        if let Some(dir) = file.parent() {
            create_private_dir(dir)?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        write_atomically(file, &bytes, Access::Private)
    }
}

// ---------------------------------------------------------------------------
// Recent files

#[derive(Default, Serialize, Deserialize)]
struct RecentJson {
    files: Vec<String>,
}

/// File > Open Recent: the last [`RECENT_LIMIT`] budgets opened or saved,
/// newest first, kept in `recent.json` (when there is a file to keep it in).
#[derive(Debug, Default)]
pub struct RecentFiles {
    file: Option<PathBuf>,
    list: Vec<PathBuf>,
}

impl RecentFiles {
    /// Reads the list from `file`; a missing or unreadable file is an empty list.
    pub fn load(file: Option<PathBuf>) -> RecentFiles {
        let list = file
            .as_deref()
            .and_then(|f| fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str::<RecentJson>(&t).ok())
            .unwrap_or_default()
            .files
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .take(RECENT_LIMIT)
            .collect();
        RecentFiles { file, list }
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.list
    }

    /// Puts `path` at the top (once), keeping at most [`RECENT_LIMIT`].
    pub fn add(&mut self, path: &Path) -> io::Result<()> {
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.list.retain(|p| *p != path);
        self.list.insert(0, path);
        self.list.truncate(RECENT_LIMIT);
        self.save()
    }

    /// Drops files that no longer exist. True if any were dropped.
    pub fn prune(&mut self) -> bool {
        let before = self.list.len();
        self.list.retain(|p| p.is_file());
        let changed = self.list.len() != before;
        if changed {
            let _ = self.save();
        }
        changed
    }

    pub fn clear(&mut self) -> io::Result<()> {
        self.list.clear();
        self.save()
    }

    fn save(&self) -> io::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(dir) = file.parent() {
            create_private_dir(dir)?;
        }
        let json = RecentJson {
            files: self
                .list
                .iter()
                .filter_map(|p| p.to_str().map(str::to_string))
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&json).map_err(io::Error::other)?;
        write_atomically(file, &bytes, Access::Private)
    }

    /// Menu labels: each file's name, with its folder's name when two
    /// share a name. `&` is doubled (menus read a single one as a mnemonic).
    pub fn labels(&self) -> Vec<String> {
        let name = |p: &Path| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string_lossy().into_owned())
        };
        self.list
            .iter()
            .map(|p| {
                let n = name(p);
                let shared = self.list.iter().filter(|q| name(q) == n).count() > 1;
                let label = match p.parent().and_then(Path::file_name) {
                    Some(dir) if shared => format!("{n} ({})", dir.to_string_lossy()),
                    _ => n,
                };
                label.replace('&', "&&")
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// The budgets open as tabs, reopened at the next start

#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct TabsJson {
    files: Vec<String>,
    active: Option<usize>,
    /// Split view: which of `files` are in which pane (src/docs.rs).
    #[serde(skip_serializing_if = "Option::is_none")]
    split: Option<crate::docs::SavedSplit>,
}

/// The files open as tabs (in tab order) and which of them is in front, kept
/// in `tabs.json` so the next start reopens them. Budgets without a file
/// aren't listed: they come back through their recovery copies.
#[derive(Debug, Default)]
pub struct OpenTabs {
    file: Option<PathBuf>,
    /// What the file holds now, so an unchanged list isn't written again.
    saved: TabsJson,
}

impl OpenTabs {
    /// Reads the list from `file`; missing or unreadable is an empty list.
    pub fn load(file: Option<PathBuf>) -> OpenTabs {
        let saved = file
            .as_deref()
            .and_then(|f| fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str::<TabsJson>(&t).ok())
            .unwrap_or_default();
        OpenTabs { file, saved }
    }

    /// The files to reopen, in tab order, and the place of the one in front
    /// among them.
    pub fn files(&self) -> (Vec<PathBuf>, Option<usize>) {
        let files: Vec<PathBuf> = self
            .saved
            .files
            .iter()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect();
        let active = self.saved.active.filter(|&i| i < files.len());
        (files, active)
    }

    /// Split view as it was kept, by place in [`OpenTabs::files`]'s list
    /// (`None`: one pane). Places past the list are dropped when restored.
    pub fn split(&self) -> Option<crate::docs::SavedSplit> {
        self.saved.split.clone()
    }

    /// Remembers `files` (tab order) with `active` in front; writes only
    /// when that differs from what is kept.
    pub fn remember(&mut self, files: &[PathBuf], active: Option<usize>) -> io::Result<()> {
        self.remember_layout(files, active, None)
    }

    /// [`OpenTabs::remember`], with split view (which files are in which
    /// pane) or `None` for one pane.
    pub fn remember_layout(
        &mut self,
        files: &[PathBuf],
        active: Option<usize>,
        split: Option<crate::docs::SavedSplit>,
    ) -> io::Result<()> {
        let next = TabsJson {
            files: files
                .iter()
                .filter_map(|p| p.to_str().map(str::to_string))
                .collect(),
            active,
            split,
        };
        if next == self.saved {
            return Ok(());
        }
        if let Some(file) = &self.file {
            if let Some(dir) = file.parent() {
                create_private_dir(dir)?;
            }
            let bytes = serde_json::to_vec_pretty(&next).map_err(io::Error::other)?;
            write_atomically(file, &bytes, Access::Private)?;
        }
        self.saved = next;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn open_tabs_are_kept_between_runs() {
        let dir = temp_dir("tabs");
        let file = dir.join("tabs.json");
        let mut t = OpenTabs::load(Some(file.clone()));
        assert_eq!(t.files(), (Vec::new(), None));
        let (a, b) = (dir.join("a.fwb"), dir.join("b.fwb"));
        t.remember(&[a.clone(), b.clone()], Some(1)).unwrap();
        let again = OpenTabs::load(Some(file.clone()));
        assert_eq!(again.files(), (vec![a.clone(), b.clone()], Some(1)));
        // Unchanged: not written again.
        fs::remove_file(&file).unwrap();
        t.remember(&[a.clone(), b.clone()], Some(1)).unwrap();
        assert!(!file.exists(), "nothing changed, nothing written");
        // A front tab past the end (or untitled, not listed) means none.
        t.remember(std::slice::from_ref(&a), None).unwrap();
        assert_eq!(OpenTabs::load(Some(file.clone())).files(), (vec![a], None));
        fs::write(&file, r#"{"files": ["x.fwb"], "active": 7}"#).unwrap();
        assert_eq!(OpenTabs::load(Some(file.clone())).files().1, None);
        fs::write(&file, "not json").unwrap();
        assert_eq!(OpenTabs::load(Some(file)).files(), (Vec::new(), None));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn split_view_is_kept_with_the_open_tabs() {
        let dir = temp_dir("tabs-split");
        let file = dir.join("tabs.json");
        let mut t = OpenTabs::load(Some(file.clone()));
        let (a, b) = (dir.join("a.fwb"), dir.join("b.fwb"));
        let split = crate::docs::SavedSplit {
            panes: [vec![0], vec![1, 0]],
            fronts: [0, 1],
            target: 1,
        };
        t.remember_layout(&[a.clone(), b.clone()], Some(1), Some(split.clone()))
            .unwrap();
        let again = OpenTabs::load(Some(file.clone()));
        assert_eq!(again.files(), (vec![a.clone(), b.clone()], Some(1)));
        assert_eq!(again.split(), Some(split));
        // One pane again: nothing about panes is written.
        t.remember(&[a.clone(), b.clone()], Some(1)).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(!text.contains("\"split\""), "{text}");
        assert_eq!(OpenTabs::load(Some(file.clone())).split(), None);
        // A file from before split view reads as one pane.
        fs::write(&file, r#"{"files": ["x.fwb"], "active": 0}"#).unwrap();
        assert_eq!(OpenTabs::load(Some(file)).split(), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn same_file_sees_through_other_spellings() {
        let dir = temp_dir("same");
        let f = dir.join("film.fwb");
        fs::write(&f, "{}").unwrap();
        assert!(same_file(&f, &f));
        fs::create_dir(dir.join("x")).unwrap();
        assert!(same_file(&f, &dir.join("x").join("..").join("film.fwb")));
        assert!(!same_file(&f, &dir.join("other.fwb")));
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A fresh empty folder under the system's temporary folder.
    pub(crate) fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fwb-safety-{tag}-{}", unique()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn autosave_waits_for_quiet_but_not_forever() {
        let t0 = Instant::now();
        let s = |n: u64| t0 + Duration::from_secs(n);
        // Quiet for 3 s after the last change.
        assert!(!autosave_due(s(2), t0, Some(t0)));
        assert!(autosave_due(s(3), t0, Some(t0)));
        // Changes every few seconds: the 30 s cap still saves.
        assert!(!autosave_due(s(29), s(28), Some(t0)));
        assert!(autosave_due(s(30), s(29), Some(t0)));
        // A clock that steps back never panics.
        assert!(!autosave_due(t0, s(10), Some(s(10))));
    }

    #[test]
    fn atomic_writes_replace_whole_files_and_leave_nothing_behind() {
        let dir = temp_dir("atomic");
        let path = dir.join("film.fwb");
        write_atomically(&path, b"one", Access::Document).unwrap();
        write_atomically(&path, b"two", Access::Document).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(names(&dir), ["film.fwb"]);

        // A failed write (the target is a folder) leaves no temporary file.
        let folder = dir.join("taken");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("x"), b"x").unwrap();
        assert!(write_atomically(&folder, b"three", Access::Document).is_err());
        assert_eq!(names(&dir), ["film.fwb", "taken"]);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let private = dir.join("private.json");
            write_atomically(&private, b"{}", Access::Private).unwrap();
            assert_eq!(
                fs::metadata(&private).unwrap().permissions().mode() & 0o777,
                0o600
            );
            // A document keeps the permissions of the file it replaces.
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            write_atomically(&path, b"four", Access::Document).unwrap();
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keys_and_names() {
        let a = new_recovery_key(Some(Path::new("/x/film.fwb")));
        let b = new_recovery_key(Some(Path::new("/x/film.fwb")));
        assert_ne!(a, b, "every open document gets its own key");
        assert!(a.starts_with(&path_hash(Path::new("/x/film.fwb"))));
        assert!(new_recovery_key(None).starts_with("untitled-"));
        assert!(valid_key(&a) && valid_key(&new_recovery_key(None)));
        for bad in ["", "../x", "a/b", "A", "x.json", "a\\b", &"a".repeat(97)] {
            assert!(!valid_key(bad), "{bad}");
        }
        assert_eq!(
            path_hash(Path::new("/x/film.fwb")),
            path_hash(Path::new("/x/film.fwb"))
        );
        assert_ne!(
            path_hash(Path::new("/x/a.fwb")),
            path_hash(Path::new("/x/b.fwb"))
        );
        assert_eq!(utc_stamp(0), "1970-01-01 00.00.00.000");
        // 2026-10-09 14:03:22.123 UTC
        assert_eq!(utc_stamp(1_791_554_602_123), "2026-10-09 14.03.22.123");
        assert_eq!(utc_stamp(951_782_400_000), "2000-02-29 00.00.00.000");
    }

    #[test]
    fn recovery_files_are_listed_only_after_their_writer_is_gone() {
        let dir = temp_dir("recovery");
        let running = RecoveryStore::open(&dir).unwrap();
        let k1 = new_recovery_key(Some(Path::new("/films/a.fwb")));
        let k2 = new_recovery_key(None);
        running
            .write(&k1, "{a}", Some(Path::new("/films/a.fwb")), "Film A")
            .unwrap();
        running.write(&k2, "{b}", None, "Untitled feature").unwrap();
        // Rewriting replaces.
        running
            .write(&k1, "{a2}", Some(Path::new("/films/a.fwb")), "Film A")
            .unwrap();

        // Another copy starting now doesn't offer a running copy's files.
        let other = RecoveryStore::open(&dir).unwrap();
        assert!(other.list().is_empty());
        assert!(other.read(&k1).is_err());
        assert!(running.list().is_empty(), "nor its own");

        // The writer stops (a crash): now they are offered, newest first.
        drop(running);
        let listed = other.list();
        assert_eq!(listed.len(), 2);
        let a = listed.iter().find(|e| e.key == k1).unwrap();
        assert_eq!(a.name, "a.fwb");
        assert_eq!(a.path.as_deref(), Some("/films/a.fwb"));
        assert_eq!(a.title, "Film A");
        let b = listed.iter().find(|e| e.key == k2).unwrap();
        assert_eq!((b.name.as_str(), b.path.as_deref()), ("Untitled", None));
        assert!(listed[0].saved_at_ms >= listed[1].saved_at_ms);
        let (text, meta) = other.read(&k1).unwrap();
        assert_eq!((text.as_str(), meta.title.as_str()), ("{a2}", "Film A"));

        // Opening one makes it the opener's: a third copy doesn't see it.
        other.adopt(&k1).unwrap();
        let third = RecoveryStore::open(&dir).unwrap();
        assert_eq!(
            third.list().iter().map(|e| &e.key).collect::<Vec<_>>(),
            [&k2]
        );

        // Discarding removes both files; removing twice is fine.
        third.remove(&k2).unwrap();
        third.remove(&k2).unwrap();
        assert!(third.list().is_empty());
        assert!(!dir.join(format!("{k2}.fwb")).exists());

        // Tidying removes the dead copy's lock, keeps the living ones'.
        third.tidy();
        let locks = names(&dir)
            .into_iter()
            .filter(|n| n.ends_with(".lock"))
            .count();
        assert_eq!(locks, 2, "{:?}", names(&dir));

        // Junk in the folder is ignored, and keys from outside are checked.
        fs::write(dir.join("not-json.json"), "nope").unwrap();
        fs::write(dir.join("../escape.json"), "{}").ok();
        assert!(third.list().is_empty());
        assert!(third.read("../escape").is_err());
        assert!(third.write("../escape", "x", None, "x").is_err());
        let _ = fs::remove_file(dir.join("../escape.json"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            let f = dir.join(format!("{k1}.fwb"));
            assert_eq!(fs::metadata(f).unwrap().permissions().mode() & 0o777, 0o600);
        }
        drop((other, third));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn backups_from_before_the_rename_are_listed_with_new_ones() {
        let dir = temp_dir("backups-legacy");
        let doc = dir.join("film.fwb");
        let backups = Backups::new(&dir.join("Backups"));
        let folder = backups.folder(&doc);
        fs::create_dir_all(&folder).unwrap();
        let old = "film 2026-01-01 00.00.00.000 UTC.fwb";
        fs::write(folder.join(old), "version 1").unwrap();
        fs::write(folder.join("notes.txt"), "not a backup").unwrap();
        fs::write(&doc, "version 2").unwrap();
        let new = backups.back_up(&doc, None).unwrap().unwrap();
        assert!(new.to_string_lossy().ends_with(" UTC.mthd"), "{new:?}");
        let entries = backups.entries(&doc);
        assert_eq!(entries.len(), 2);
        assert_eq!(backups.read(&doc, old).unwrap(), "version 1");
        assert_eq!(backups.read(&doc, &entries[0].id).unwrap(), "version 2");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn backups_keep_the_last_ten_distinct_versions() {
        let dir = temp_dir("backups");
        let doc = dir.join("work").join("film.fwb");
        let backups = Backups::new(&dir.join("Backups"));
        // Nothing on disk yet: nothing to back up.
        assert_eq!(backups.back_up(&doc, None).unwrap(), None);
        fs::create_dir_all(doc.parent().unwrap()).unwrap();
        for i in 0..13 {
            fs::write(&doc, format!("version {i}")).unwrap();
            assert!(backups.back_up(&doc, None).unwrap().is_some());
            // Saving again without changes adds nothing.
            assert_eq!(backups.back_up(&doc, None).unwrap(), None);
        }
        let kept: Vec<String> = backups
            .list(&doc)
            .iter()
            .map(|p| fs::read_to_string(p).unwrap())
            .collect();
        let want: Vec<String> = (3..13).map(|i| format!("version {i}")).collect();
        assert_eq!(kept, want);
        let first = backups.list(&doc)[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(
            first.starts_with("film ") && first.ends_with(" UTC.mthd"),
            "{first}"
        );
        let source = fs::read_to_string(backups.folder(&doc).join("source.json")).unwrap();
        assert!(source.contains("film.fwb"));

        // Autosaves back up only when the newest backup is old enough.
        fs::write(&doc, "version 13").unwrap();
        assert_eq!(backups.back_up(&doc, Some(BACKUP_GAP)).unwrap(), None);
        assert!(backups
            .back_up(&doc, Some(Duration::ZERO))
            .unwrap()
            .is_some());

        // Entries for the UI, newest first, and reading one back by name.
        let entries = backups.entries(&doc);
        assert_eq!(entries.len(), BACKUPS_KEPT);
        assert_eq!(backups.read(&doc, &entries[0].id).unwrap(), "version 13");
        assert_eq!(backups.read(&doc, &entries[1].id).unwrap(), "version 12");
        assert!(entries[0].bytes > 0);
        assert!(backups.read(&doc, "../../work/film.fwb").is_err());
        assert!(backups.read(&doc, "source.json").is_err());
        assert!(backups.entries(&dir.join("other.fwb")).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn disk_stamps_notice_changes_made_elsewhere() {
        let dir = temp_dir("stamp");
        let path = dir.join("film.fwb");
        fs::write(&path, b"ours").unwrap();
        let stamp = DiskStamp::of(&path, b"ours");
        assert_eq!(stamp.check(&path), DiskCheck::Same(None));

        // Touched (a sync service rewriting the same bytes): still the same,
        // with a fresh stamp to remember.
        let later = SystemTime::now() + Duration::from_secs(5);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(later)
            .unwrap();
        let DiskCheck::Same(Some(fresh)) = stamp.check(&path) else {
            panic!("touching isn't changing");
        };
        assert_eq!(fresh.check(&path), DiskCheck::Same(None));

        // Different bytes of the same size, then of another size.
        fs::write(&path, b"them").unwrap();
        assert_eq!(stamp.check(&path), DiskCheck::Changed);
        fs::write(&path, b"theirs, longer").unwrap();
        assert_eq!(stamp.check(&path), DiskCheck::Changed);
        fs::remove_file(&path).unwrap();
        assert_eq!(stamp.check(&path), DiskCheck::Missing);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn settings_default_to_autosave_on_and_are_kept() {
        let dir = temp_dir("settings");
        let file = dir.join("settings.json");
        assert!(Settings::load(&file).autosave);
        Settings { autosave: false }.save(&file).unwrap();
        assert!(!Settings::load(&file).autosave);
        fs::write(&file, "{}").unwrap();
        assert!(
            Settings::load(&file).autosave,
            "missing fields are defaults"
        );
        fs::write(&file, "garbage").unwrap();
        assert!(Settings::load(&file).autosave);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn recent_files_are_ordered_deduped_capped_pruned_and_kept() {
        let dir = temp_dir("recent");
        let store = dir.join("recent.json");
        let file = |n: &str| {
            let p = dir.join(n);
            fs::write(&p, "{}").unwrap();
            fs::canonicalize(p).unwrap()
        };
        let mut r = RecentFiles::load(Some(store.clone()));
        assert!(r.paths().is_empty());
        let a = file("a.fwb");
        let b = file("b.fwb");
        r.add(&a).unwrap();
        r.add(&b).unwrap();
        r.add(&a).unwrap();
        assert_eq!(r.paths(), [a.clone(), b.clone()], "newest first, once each");

        // Kept across launches.
        assert_eq!(RecentFiles::load(Some(store.clone())).paths(), r.paths());

        // At most ten.
        for i in 0..12 {
            r.add(&file(&format!("f{i:02}.fwb"))).unwrap();
        }
        assert_eq!(r.paths().len(), RECENT_LIMIT);
        assert_eq!(r.paths()[0], dir.join("f11.fwb").canonicalize().unwrap());
        assert!(!r.paths().contains(&a));

        // Missing files drop out.
        fs::remove_file(dir.join("f11.fwb")).unwrap();
        assert!(r.prune());
        assert!(!r.prune());
        assert_eq!(r.paths().len(), RECENT_LIMIT - 1);
        assert_eq!(
            RecentFiles::load(Some(store.clone())).paths().len(),
            RECENT_LIMIT - 1
        );

        r.clear().unwrap();
        assert!(RecentFiles::load(Some(store.clone())).paths().is_empty());

        // A corrupt list is an empty one.
        fs::write(&store, "not json").unwrap();
        assert!(RecentFiles::load(Some(store)).paths().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn recent_labels_tell_same_named_files_apart() {
        let r = RecentFiles {
            file: None,
            list: vec![
                PathBuf::from("/films/One/budget.fwb"),
                PathBuf::from("/films/Two/budget.fwb"),
                PathBuf::from("/films/Rock & Roll.fwb"),
            ],
        };
        assert_eq!(
            r.labels(),
            ["budget.fwb (One)", "budget.fwb (Two)", "Rock && Roll.fwb"]
        );
    }
}
