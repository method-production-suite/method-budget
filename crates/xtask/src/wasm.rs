//! `cargo xtask wasm-check`: the crates a browser runs build for
//! WebAssembly (docs/research/web-session.md). method-session is checked
//! without features, so nothing that only works on a desktop (its
//! `std-clock` default) sneaks in.

use std::path::Path;
use std::process::Command;

/// The target a browser build uses.
pub const TARGET: &str = "wasm32-unknown-unknown";

/// The crates checked.
pub const CRATES: &[&str] = &["method-core", "method-format", "method-session"];

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

pub fn command(root: &Path) -> Result<String, String> {
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .current_dir(root)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim() == TARGET)
        })
        .unwrap_or(false);
    if !installed {
        return Err(format!(
            "the {TARGET} target isn't installed; run `rustup target add {TARGET}` first"
        ));
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .args(check_args())
        .current_dir(root)
        .status()
        .map_err(|e| format!("couldn't run cargo: {e}"))?;
    if !status.success() {
        return Err(format!("{} don't build for {TARGET}", CRATES.join(", ")));
    }
    Ok(format!("{} build for {TARGET}\n", CRATES.join(", ")))
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
}
