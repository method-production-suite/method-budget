//! Method Budget's open budgets with no window, for the desktop app and a
//! browser alike (docs/research/web-session.md).
//!
//! [`session`] holds one open budget with undo/redo, [`docs`] every budget
//! open as a tab, [`edit`] changes a budget, [`diagnose`] recalculates it
//! without giving up on the first bad cell, and [`view`] is what the UI
//! draws. Nothing here touches the file system, starts a thread or reads a
//! clock ([`clock`] is handed in by the host), so the crate builds for
//! `wasm32-unknown-unknown`.

#![forbid(unsafe_code)]

pub mod actuals;
pub mod apply;
pub mod batch;
pub mod clipboard;
pub mod clock;
pub mod coa_import;
pub mod collab;
pub mod commands;
pub mod compare;
pub mod diagnose;
pub mod diffmerge;
pub mod display;
pub mod docs;
pub mod edit;
pub mod explain;
pub mod export;
pub mod export_import;
pub mod find;
pub mod history;
pub mod host;
pub mod import_setup;
pub mod library;
pub mod moverows;
pub mod new_budget;
pub mod notes;
pub mod open_any;
pub mod overtime;
pub mod packs;
pub mod presets;
pub mod projects;
pub mod quickadd;
pub mod rates;
pub mod rename;
pub mod renumber;
pub mod report_dto;
pub mod reports;
pub mod review;
pub mod rowstyle;
pub mod safety;
pub mod sample;
pub mod schedule_days;
pub mod session;
pub mod setup_lists;
pub mod setup_sheets;
pub mod sharing;
pub mod sheet_import;
pub mod tabs;
pub mod templates;
pub mod versions;
pub mod view;
pub mod what_if;

pub use docs::{AppState, DocGuard, DocId, Docs};
pub use host::{Downloads, Error, FileIn, FileOut, Host, HostInfo, COMMANDS, COMMANDS_VERSION};
pub use session::Session;
pub use view::View;
