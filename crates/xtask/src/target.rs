//! Where cargo puts what it builds. On a shared machine every checkout of a
//! repository builds into one folder (`CARGO_TARGET_DIR`), so nothing here
//! assumes `./target`: `cargo metadata` says where it is.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The build folder of the workspace at `root`: `cargo metadata`'s
/// `target_directory`, which follows `CARGO_TARGET_DIR` and cargo's
/// configuration; `root/target` when cargo can't say.
pub fn dir(root: &Path) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
        .and_then(|v| v["target_directory"].as_str().map(PathBuf::from))
        .unwrap_or_else(|| root.join("target"))
}
