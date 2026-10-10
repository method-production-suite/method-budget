//! Release plumbing shared by the desktop app and the release tooling
//! (`crates/xtask`, run as `cargo xtask ...`; docs/RELEASING.md):
//!
//! - [`version`]: the semantic versions the app and its releases carry.
//! - [`notes`]: the release notes in `docs/releases/<version>.md`, read into
//!   plain data (the app's What's New window shows it with `textContent`,
//!   never as markup) and checked before a release.
//! - [`minisign`]: the two minisign containers the updater deals in (an
//!   artifact's `.sig` and the public key in `tauri.conf.json`), read and
//!   checked for shape and key id, never verified: the updater plugin does
//!   the cryptography at install time.
//!
//! No dependencies, so linking it costs the app nothing.

pub mod minisign;
pub mod notes;
pub mod version;
