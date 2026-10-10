//! Film Wizard Budgeting desktop app.
//!
//! Everything except [`app`] (and the other Tauri glue: [`tabs`],
//! [`safety_app`], [`tools`], [`menu`]) is plain Rust with no window:
//! [`session`] holds one open budget with undo/redo, [`docs`] every budget
//! open as a tab, [`edit`] changes a budget, [`diagnose`] recalculates it
//! without giving up on the first bad cell, and [`view`] is what the UI draws.
//! The UI itself is static HTML/CSS/JS in `ui/`; see docs/APP.md.

pub mod app;
pub mod apply;
pub mod batch;
pub mod clipboard;
pub mod compare;
pub mod diagnose;
pub mod display;
pub mod docs;
pub mod edit;
pub mod export;
pub mod find;
pub mod history;
pub mod import_setup;
pub mod library;
pub mod menu;
pub mod moverows;
pub mod notes;
#[cfg(target_os = "macos")]
mod pdf_mac;
pub mod presets;
pub mod quickadd;
pub mod rename;
pub mod renumber;
pub mod report_dto;
pub mod reports;
pub mod rowstyle;
pub mod safety;
pub mod safety_app;
pub mod session;
pub mod setup_lists;
pub mod sharing;
pub mod sheet_import;
pub mod tabs;
pub mod templates;
pub mod tools;
pub mod versions;
pub mod view;

pub use app::run;
pub use session::Session;
pub use view::View;
