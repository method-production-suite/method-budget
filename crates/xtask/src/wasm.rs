//! `cargo xtask wasm-check`: the crates a browser runs build for
//! WebAssembly (docs/research/web-session.md). method-session is checked
//! with its default features only (the bundled charts), never `std-clock`,
//! so nothing that only works on a desktop sneaks in, and method-web, the
//! browser's own crate (no bundled charts), is built (linked) for the target
//! too.

use std::path::Path;
use std::process::Command;

/// The target a browser build uses.
pub const TARGET: &str = "wasm32-unknown-unknown";

/// The crates checked.
pub const CRATES: &[&str] = &["method-core", "method-format", "method-session"];

/// The crate built, as the browser loads it (`cargo xtask web`).
pub const WEB_CRATE: &str = "method-web";

/// The `cargo check` arguments for the crates in [`CRATES`].
pub fn check_args() -> Vec<String> {
    let mut args = vec!["check".to_string(), "--locked".to_string()];
    for c in CRATES {
        args.push("-p".into());
        args.push((*c).into());
    }
    args.push("--target".into());
    args.push(TARGET.into());
    args
}

/// The `cargo build` arguments for [`WEB_CRATE`] (the dev profile: this
/// checks it links, `cargo xtask web` makes the small one).
pub fn build_args() -> Vec<String> {
    ["build", "--locked", "-p", WEB_CRATE, "--target", TARGET]
        .map(String::from)
        .to_vec()
}

/// Whether rustup has [`TARGET`].
pub fn target_installed(root: &Path) -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .current_dir(root)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim() == TARGET)
        })
        .unwrap_or(false)
}

pub fn command(root: &Path) -> Result<String, String> {
    if !target_installed(root) {
        return Err(format!(
            "the {TARGET} target isn't installed; run `rustup target add {TARGET}` first"
        ));
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(&cargo)
        .args(check_args())
        .current_dir(root)
        .status()
        .map_err(|e| format!("couldn't run cargo: {e}"))?;
    if !status.success() {
        return Err(format!("{} don't build for {TARGET}", CRATES.join(", ")));
    }
    let status = Command::new(&cargo)
        .args(build_args())
        .current_dir(root)
        .status()
        .map_err(|e| format!("couldn't run cargo: {e}"))?;
    if !status.success() {
        return Err(format!("{WEB_CRATE} doesn't build for {TARGET}"));
    }
    Ok(format!(
        "{} and {WEB_CRATE} build for {TARGET}\n",
        CRATES.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_crate_is_checked_for_the_browser_without_features() {
        let args = check_args();
        assert!(args.windows(2).any(|w| w == ["-p", "method-session"]));
        assert!(args.windows(2).any(|w| w == ["--target", TARGET]));
        assert!(!args.iter().any(|a| a.contains("feature")));
    }

    #[test]
    fn the_browser_crate_is_built_for_the_browser() {
        let args = build_args();
        assert!(args.windows(2).any(|w| w == ["-p", WEB_CRATE]));
        assert!(args.windows(2).any(|w| w == ["--target", TARGET]));
    }
}
