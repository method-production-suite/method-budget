//! The repository: where it is, its files, and git (always run with an
//! argument list, never through a shell, so a ref or a path is never
//! interpreted as a command).

use std::path::{Path, PathBuf};
use std::process::Command;

/// The app's Tauri configuration, relative to the repository root.
pub const TAURI_CONF: &str = "crates/fwb-app/tauri.conf.json";
/// The notes guide, whose skeleton block starts each new notes file.
pub const NOTES_TEMPLATE: &str = "docs/release-notes-template.md";
/// Where each release's notes live.
pub const RELEASES_DIR: &str = "docs/releases";

pub fn tauri_conf(root: &Path) -> PathBuf {
    root.join(TAURI_CONF)
}

pub fn notes_path(root: &Path, version: &str) -> PathBuf {
    root.join(RELEASES_DIR).join(format!("{version}.md"))
}

/// The workspace root: the nearest directory, from here up, whose
/// Cargo.toml has a `[workspace]` table.
pub fn find_root() -> Result<PathBuf, String> {
    let start = std::env::current_dir().map_err(|e| e.to_string())?;
    for dir in start.ancestors() {
        let toml = dir.join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&toml) {
            if text.lines().any(|l| l.trim() == "[workspace]") {
                return Ok(dir.to_path_buf());
            }
        }
    }
    Err("not inside the Method Budget repository (no workspace Cargo.toml above here)".into())
}

pub fn read(root: &Path, rel: &str) -> Result<String, String> {
    std::fs::read_to_string(root.join(rel)).map_err(|e| format!("can't read {rel}: {e}"))
}

/// Runs git in `root` with `args`; its standard output, or why it failed.
pub fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("can't run git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Paths with uncommitted changes (staged, unstaged or untracked), relative
/// to the root.
pub fn dirty_paths(root: &Path) -> Result<Vec<String>, String> {
    let out = git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut paths = Vec::new();
    let mut entries = out.split('\0').filter(|e| !e.is_empty());
    while let Some(e) = entries.next() {
        if e.len() < 4 {
            continue;
        }
        let (code, path) = e.split_at(3);
        paths.push(path.to_string());
        // A rename or copy is followed by its old path.
        if code.starts_with('R') || code.starts_with('C') {
            entries.next();
        }
    }
    Ok(paths)
}
