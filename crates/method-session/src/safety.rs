//! The parts of keeping work safe that need no file system: when autosave
//! is due, the names of recovery copies, and the stamp a session keeps of
//! its file to notice changes made outside. Reading and writing files is
//! the desktop's (method-app's `src/safety.rs`).

use std::path::Path;
use std::time::{Duration, SystemTime};

/// Autosave waits for this long without a change...
pub const AUTOSAVE_QUIET: Duration = Duration::from_secs(3);
/// ...but never longer than this after the first change it hasn't saved.
pub const AUTOSAVE_MAX_WAIT: Duration = Duration::from_secs(30);

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Whether an autosave is due at `now`: the budget has been quiet for
/// [`AUTOSAVE_QUIET`] since `last_change`, or [`AUTOSAVE_MAX_WAIT`] has
/// passed since `pending_since`, the first change not yet in the recovery
/// file. Times are [`crate::clock::mono_ms`] readings.
pub fn autosave_due(now: u64, last_change: u64, pending_since: Option<u64>) -> bool {
    let waited = |t: u64| now.saturating_sub(t);
    waited(last_change) >= ms(AUTOSAVE_QUIET)
        || pending_since.is_some_and(|t| waited(t) >= ms(AUTOSAVE_MAX_WAIT))
}

/// A stable 64-bit FNV-1a hash of a path, as 16 hex digits. Stable across
/// runs and Rust versions (std's hasher is not), so a document's backups
/// stay in one folder.
pub fn path_hash(path: &Path) -> String {
    format!(
        "{:016x}",
        content_hash(path.as_os_str().to_string_lossy().as_bytes())
    )
}

/// A fresh recovery key for a budget: unique to this open document, so two
/// copies of the app (or an old crash's file for the same path) never write
/// over each other. It starts with the path's hash, or `untitled`.
pub fn new_recovery_key(path: Option<&Path>) -> String {
    let base = path.map(path_hash).unwrap_or_else(|| "untitled".into());
    format!("{base}-{}", crate::clock::unique())
}

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
/// changing it since. Made and checked by the desktop, which reads files.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autosave_waits_for_quiet_but_not_forever() {
        let t0 = 1_000_000;
        let s = |n: u64| t0 + n * 1000;
        // Quiet for 3 s after the last change.
        assert!(!autosave_due(s(2), t0, Some(t0)));
        assert!(autosave_due(s(3), t0, Some(t0)));
        // Changes every few seconds: the 30 s cap still saves.
        assert!(!autosave_due(s(29), s(28), Some(t0)));
        assert!(autosave_due(s(30), s(29), Some(t0)));
        // A clock that steps back never panics.
        assert!(!autosave_due(t0, s(10), Some(s(10))));
    }
}
