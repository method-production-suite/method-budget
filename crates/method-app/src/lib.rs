//! Method Budget desktop app.
//!
//! Everything except [`app`] (and the other Tauri glue: [`tabs`],
//! [`safety_app`], [`tools`], [`menu`]) is plain Rust with no window:
//! [`session`] holds one open budget with undo/redo, [`docs`] every budget
//! open as a tab, [`edit`] changes a budget, [`diagnose`] recalculates it
//! without giving up on the first bad cell, and [`view`] is what the UI draws.
//! The UI itself is static HTML/CSS/JS in `ui/`; see docs/APP.md.

pub mod actuals;
pub mod app;
pub mod apply;
pub mod bookmark;
pub mod clipboard;
pub mod coa_import;
pub mod compare;
pub mod diffmerge;
pub mod display;
pub mod explain;
pub mod export;
pub mod export_import;
pub mod find;
pub mod help;
pub mod history;
pub mod i18n;
pub mod import_setup;
pub mod launch;
pub mod library;
pub mod menu;
pub mod moverows;
pub mod notes;
pub mod open_any;
pub mod overtime;
pub mod packs;
#[cfg(target_os = "macos")]
mod pdf_mac;
pub mod presets;
pub mod quickadd;
pub mod rates;
pub mod rename;
pub mod renumber;
pub mod reports;
pub mod review;
pub mod rowstyle;
pub mod safety;
pub mod safety_app;
pub mod sample;
pub mod schedule_days;
pub mod session;
pub mod setup_lists;
pub mod setup_sheets;
pub mod setupwin;
pub mod shared;
pub mod shared_app;
pub mod sharing;
pub mod sheet_import;
pub mod start;
pub mod tabs;
pub mod templates;
pub mod tools;
pub mod updater;
pub mod versions;

// Moved to method-session (docs/research/web-session.md), and re-exported
// so the rest of the app keeps its `crate::edit` paths.
pub use method_session::{batch, diagnose, docs, edit, report_dto, view};

pub use app::run;
pub use session::Session;
pub use view::View;
