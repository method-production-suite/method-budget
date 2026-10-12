//! Method Budget's open budgets with no window, for the desktop app and a
//! browser alike (docs/research/web-session.md).
//!
//! [`session`] holds one open budget with undo/redo, [`docs`] every budget
//! open as a tab, [`edit`] changes a budget, [`diagnose`] recalculates it
//! without giving up on the first bad cell, and [`view`] is what the UI
//! draws. Nothing here touches the file system, starts a thread or reads a
//! clock ([`clock`] is handed in by the host), so the crate builds for
//! `wasm32-unknown-unknown`.

pub mod actuals;
pub mod batch;
pub mod clock;
pub mod collab;
pub mod commands;
pub mod diagnose;
pub mod display;
pub mod docs;
pub mod edit;
pub mod explain;
pub mod find;
pub mod history;
pub mod host;
pub mod import_setup;
pub mod library;
pub mod moverows;
pub mod overtime;
pub mod packs;
pub mod quickadd;
pub mod renumber;
pub mod report_dto;
pub mod safety;
pub mod session;
pub mod setup_lists;
pub mod sharing;
pub mod templates;
pub mod versions;
pub mod view;

pub use docs::{AppState, DocGuard, DocId, Docs};
pub use host::{Error, Host, HostInfo, COMMANDS, COMMANDS_VERSION};
pub use session::Session;
pub use view::View;
