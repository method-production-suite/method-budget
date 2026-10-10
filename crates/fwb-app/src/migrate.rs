//! One-time copy of the app's local data from the pre-rename app identifier
//! to the current one (decisions.md, the rename entry; docs/APP.md, "Saving
//! and recovery").
//!
//! Tauri keys the app's local data folder on the bundle identifier, so
//! changing the identifier would otherwise start everyone with empty
//! settings: no Open Recent, no tabs to reopen, no library, no recovery
//! copies or backups. On startup, when the new folder has none of our state
//! and the old one exists, the old folder is **copied** (never moved: the
//! old app keeps working) into a temporary sibling folder, then renamed into
//! place, so a crash midway leaves either nothing or the whole copy. Running
//! copies' `*.lock` files and web caches are left behind. A
//! `migrated-from.json` marker records where it came from and when.
//!
//! Two passes: [`before_start`] runs before Tauri creates the window, with
//! the folders worked out from the environment the way Tauri does, so the
//! web view's own storage (display choices and saved spreadsheet mappings)
//! comes along too; [`at_setup`] repeats the check with the folders Tauri
//! reports, which does nothing when the first pass already copied. On macOS
//! the web view keeps its storage in `~/Library/WebKit/<identifier>`, copied
//! the same way when the new one doesn't exist yet.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::safety::{create_private_dir, now_ms};

/// The bundle identifier before the rename. Its data folder is what gets
/// copied.
pub const OLD_IDENTIFIER: &str = "io.github.ssandweiss.film-wizard-budgeting";

/// Written into the new folder by a migration.
pub const MARKER: &str = "migrated-from.json";

/// Entries in the data folder that mean it is already in use: when any is
/// there, nothing is copied.
const STATE: &[&str] = &[
    "settings.json",
    "recent.json",
    "tabs.json",
    "library.json",
    "Recovery",
    "Backups",
    MARKER,
];

/// Folder names never copied: web view caches, rebuilt on demand.
const SKIPPED_DIRS: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "DawnCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
    "GrShaderCache",
    "GraphiteDawnCache",
    "ShaderCache",
    "CacheStorage",
];

/// What a migration did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// There is no old folder.
    NoOldData,
    /// The new folder already has data; nothing was copied.
    AlreadyInUse,
    /// The old folder was copied: `files` files, and the paths of any that
    /// couldn't be read (left out).
    Copied { files: usize, skipped: Vec<PathBuf> },
}

/// Copies `old` (the pre-rename data folder) to `new` when `new` holds none
/// of the app's state. `new` may exist already (the web view may have made
/// it); then each copied entry moves in only where `new` has nothing by
/// that name.
pub fn migrate_data_dir(old: &Path, new: &Path) -> io::Result<Outcome> {
    if !old.is_dir() || same_dir(old, new) {
        return Ok(Outcome::NoOldData);
    }
    if STATE.iter().any(|name| new.join(name).exists()) {
        return Ok(Outcome::AlreadyInUse);
    }
    copy_into_place(old, new, true)
}

/// Copies `old` to `new` when `new` doesn't exist (the macOS web view's
/// storage folder, which has no marker of its own).
pub fn migrate_plain_dir(old: &Path, new: &Path) -> io::Result<Outcome> {
    if !old.is_dir() || same_dir(old, new) {
        return Ok(Outcome::NoOldData);
    }
    if new.exists() {
        return Ok(Outcome::AlreadyInUse);
    }
    copy_into_place(old, new, false)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// Copies `old` into a fresh temporary sibling of `new`, then renames it to
/// `new` (or, when `new` exists, moves each entry in where `new` has none).
fn copy_into_place(old: &Path, new: &Path, marker: bool) -> io::Result<Outcome> {
    let parent = new
        .parent()
        .ok_or_else(|| io::Error::other("the new folder has no parent"))?;
    fs::create_dir_all(parent)?;
    let name = new
        .file_name()
        .ok_or_else(|| io::Error::other("the new folder has no name"))?
        .to_string_lossy();
    let tmp = parent.join(format!(
        ".{name}.migrating-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let result = (|| {
        create_private_dir(&tmp)?;
        let mut files = 0;
        let mut skipped = Vec::new();
        copy_tree(old, &tmp, &mut files, &mut skipped)?;
        if marker {
            let note = serde_json::json!({
                "from": old.to_string_lossy(),
                "at_ms": now_ms(),
                "files": files,
            });
            fs::write(tmp.join(MARKER), format!("{note:#}\n"))?;
        }
        if !new.exists() && fs::rename(&tmp, new).is_ok() {
            return Ok(Outcome::Copied { files, skipped });
        }
        // `new` exists (made by the web view, or by a race): move entries in
        // one by one, each atomically, never over anything already there.
        fs::create_dir_all(new)?;
        for entry in fs::read_dir(&tmp)? {
            let entry = entry?;
            let target = new.join(entry.file_name());
            if !target.exists() {
                fs::rename(entry.path(), target)?;
            }
        }
        Ok(Outcome::Copied { files, skipped })
    })();
    let _ = fs::remove_dir_all(&tmp);
    result
}

/// Copies the regular files and folders under `from` into `to` (which
/// exists). Symbolic links, `*.lock` files and cache folders are left out;
/// a file that can't be read is listed in `skipped` and the rest goes on.
fn copy_tree(
    from: &Path,
    to: &Path,
    files: &mut usize,
    skipped: &mut Vec<PathBuf>,
) -> io::Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        let name = entry.file_name();
        let target = to.join(&name);
        if kind.is_dir() {
            if SKIPPED_DIRS.iter().any(|d| name == *d) {
                continue;
            }
            fs::create_dir_all(&target)?;
            copy_tree(&path, &target, files, skipped)?;
        } else if kind.is_file() {
            if path.extension().is_some_and(|e| e == "lock") {
                continue;
            }
            match fs::copy(&path, &target) {
                Ok(_) => *files += 1,
                Err(_) => skipped.push(path),
            }
        }
    }
    Ok(())
}

fn log(what: &str, old: &Path, new: &Path, outcome: &io::Result<Outcome>) {
    match outcome {
        Ok(Outcome::Copied { files, skipped }) => {
            eprintln!(
                "Method Budget: copied {what} from {} to {} ({files} files)",
                old.display(),
                new.display()
            );
            for p in skipped {
                eprintln!("Method Budget: could not copy {}", p.display());
            }
        }
        Ok(_) => {}
        Err(e) => eprintln!(
            "Method Budget: could not copy {what} from {}: {e}",
            old.display()
        ),
    }
}

/// The folder Tauri puts every app's local data folder in, worked out from
/// the environment the way Tauri does (`dirs::data_local_dir`).
fn local_data_base() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library").join("Application Support"))
    } else if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".local").join("share")))
    }
}

/// Before Tauri starts: copies the old data folder (and, on macOS, the old
/// web view storage) for the app identified as `identifier`.
pub fn before_start(identifier: &str) {
    if identifier == OLD_IDENTIFIER {
        return;
    }
    if let Some(base) = local_data_base() {
        let (old, new) = (base.join(OLD_IDENTIFIER), base.join(identifier));
        let done = migrate_data_dir(&old, &new);
        log("app data", &old, &new, &done);
    }
    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            let webkit = PathBuf::from(home).join("Library").join("WebKit");
            let (old, new) = (webkit.join(OLD_IDENTIFIER), webkit.join(identifier));
            let done = migrate_plain_dir(&old, &new);
            log("web view storage", &old, &new, &done);
        }
    }
}

/// From Tauri's setup, before anything reads the data folder: the same
/// check with the folder Tauri reports (`new`); the old one is its sibling.
pub fn at_setup(new: &Path) {
    let Some(old) = new.parent().map(|p| p.join(OLD_IDENTIFIER)) else {
        return;
    };
    let done = migrate_data_dir(&old, new);
    log("app data", &old, new, &done);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::tests::temp_dir;

    /// An old data folder with everything the app keeps there.
    fn old_folder(base: &Path) -> PathBuf {
        let old = base.join(OLD_IDENTIFIER);
        for d in ["Recovery", "Backups/0123", "EBWebView/Default/Cache"] {
            fs::create_dir_all(old.join(d)).unwrap();
        }
        let files = [
            ("settings.json", "{\"autosave\": false}"),
            ("recent.json", "[\"/films/a.fwb\"]"),
            (
                "tabs.json",
                "{\"files\": [\"/films/a.fwb\"], \"active\": 0}",
            ),
            ("library.json", "{\"path\": \"/films/lib.fwb\"}"),
            ("Recovery/k1.fwb", "{}"),
            ("Recovery/k1.json", "{}"),
            ("Recovery/owner-1.lock", ""),
            ("Backups/0123/a 2026 UTC.fwb", "v1"),
            ("Backups/0123/source.json", "{}"),
            ("EBWebView/Default/prefs", "kept"),
            ("EBWebView/Default/Cache/big", "dropped"),
        ];
        for (name, text) in files {
            fs::write(old.join(name), text).unwrap();
        }
        old
    }

    fn read(p: &Path) -> String {
        fs::read_to_string(p).unwrap()
    }

    #[test]
    fn copies_everything_once_and_leaves_the_old_folder() {
        let base = temp_dir("migrate");
        let old = old_folder(&base);
        let new = base.join("com.methodbudget.desktop");
        let done = migrate_data_dir(&old, &new).unwrap();
        assert_eq!(
            done,
            Outcome::Copied {
                files: 9,
                skipped: vec![]
            }
        );
        assert_eq!(read(&new.join("settings.json")), "{\"autosave\": false}");
        assert_eq!(read(&new.join("Recovery/k1.fwb")), "{}");
        assert_eq!(read(&new.join("Backups/0123/a 2026 UTC.fwb")), "v1");
        assert_eq!(read(&new.join("EBWebView/Default/prefs")), "kept");
        assert!(!new.join("Recovery/owner-1.lock").exists(), "locks stay");
        assert!(!new.join("EBWebView/Default/Cache").exists(), "caches stay");
        assert!(read(&new.join(MARKER)).contains(OLD_IDENTIFIER));
        // Copied, not moved.
        assert_eq!(read(&old.join("settings.json")), "{\"autosave\": false}");
        assert!(old.join("Recovery/owner-1.lock").exists());
        // No temporary folder left behind.
        let leftovers: Vec<_> = fs::read_dir(&base)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("migrating"))
            .collect();
        assert!(leftovers.is_empty());

        // Once only: later starts leave the new folder alone.
        fs::write(new.join("settings.json"), "{\"autosave\": true}").unwrap();
        fs::write(old.join("recent.json"), "[]").unwrap();
        assert_eq!(migrate_data_dir(&old, &new).unwrap(), Outcome::AlreadyInUse);
        assert_eq!(read(&new.join("settings.json")), "{\"autosave\": true}");
        assert_eq!(read(&new.join("recent.json")), "[\"/films/a.fwb\"]");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_new_folder_with_only_web_view_files_gets_the_rest() {
        let base = temp_dir("migrate-partial");
        let old = old_folder(&base);
        let new = base.join("com.methodbudget.desktop");
        fs::create_dir_all(new.join("EBWebView")).unwrap();
        fs::write(new.join("EBWebView/fresh"), "new").unwrap();
        let done = migrate_data_dir(&old, &new).unwrap();
        assert!(matches!(done, Outcome::Copied { .. }), "{done:?}");
        assert!(read(&new.join("tabs.json")).contains("a.fwb"));
        assert_eq!(
            read(&new.join("library.json")),
            "{\"path\": \"/films/lib.fwb\"}"
        );
        assert_eq!(read(&new.join("EBWebView/fresh")), "new", "never replaced");
        assert!(!new.join("EBWebView/Default").exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn nothing_happens_without_an_old_folder_or_with_data_already() {
        let base = temp_dir("migrate-none");
        let new = base.join("new");
        let old = base.join(OLD_IDENTIFIER);
        assert_eq!(migrate_data_dir(&old, &new).unwrap(), Outcome::NoOldData);
        assert!(!new.exists());
        let old = old_folder(&base);
        fs::create_dir_all(new.join("Backups")).unwrap();
        assert_eq!(migrate_data_dir(&old, &new).unwrap(), Outcome::AlreadyInUse);
        assert!(!new.join("settings.json").exists());
        // The same folder twice (an identifier that didn't change).
        assert_eq!(migrate_data_dir(&old, &old).unwrap(), Outcome::NoOldData);
        fs::remove_dir_all(&base).unwrap();
    }

    /// Links in the old folder are left out, never followed: a link to a
    /// folder or a file elsewhere copies nothing from there.
    #[cfg(unix)]
    #[test]
    fn links_are_never_followed_out_of_the_old_folder() {
        use std::os::unix::fs::symlink;
        let base = temp_dir("migrate-links");
        let old = old_folder(&base);
        let outside = base.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "not ours").unwrap();
        symlink(&outside, old.join("Linked folder")).unwrap();
        symlink(outside.join("secret.txt"), old.join("Recovery/linked.fwb")).unwrap();
        let new = base.join("com.methodbudget.desktop");
        let done = migrate_data_dir(&old, &new).unwrap();
        assert_eq!(
            done,
            Outcome::Copied {
                files: 9,
                skipped: vec![]
            }
        );
        assert!(fs::symlink_metadata(new.join("Linked folder")).is_err());
        assert!(fs::symlink_metadata(new.join("Recovery/linked.fwb")).is_err());
        assert_eq!(read(&new.join("Recovery/k1.fwb")), "{}");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn web_view_storage_copies_only_into_a_missing_folder() {
        let base = temp_dir("migrate-webkit");
        let old = base.join("old");
        fs::create_dir_all(old.join("WebsiteData/LocalStorage")).unwrap();
        fs::write(old.join("WebsiteData/LocalStorage/db"), "prefs").unwrap();
        let new = base.join("new");
        let done = migrate_plain_dir(&old, &new).unwrap();
        assert!(matches!(done, Outcome::Copied { files: 1, .. }), "{done:?}");
        assert_eq!(read(&new.join("WebsiteData/LocalStorage/db")), "prefs");
        assert!(!new.join(MARKER).exists());
        fs::write(old.join("WebsiteData/LocalStorage/db"), "changed").unwrap();
        assert_eq!(
            migrate_plain_dir(&old, &new).unwrap(),
            Outcome::AlreadyInUse
        );
        assert_eq!(read(&new.join("WebsiteData/LocalStorage/db")), "prefs");
        fs::remove_dir_all(&base).unwrap();
    }
}
