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
pub mod diagnose;
pub mod docs;
pub mod edit;
pub mod menu;
pub mod quickadd;
pub mod report_dto;
pub mod rowstyle;
pub mod safety;
pub mod safety_app;
pub mod session;
pub mod tabs;
pub mod tools;
pub mod view;

pub use app::run;
pub use session::Session;
pub use view::View;
