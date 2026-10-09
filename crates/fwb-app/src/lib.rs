//! Film Wizard Budgeting desktop app.
//!
//! Everything except [`app`] is plain Rust with no window: [`session`] holds the
//! open budget with undo/redo, [`edit`] changes it, [`diagnose`] recalculates it
//! without giving up on the first bad cell, and [`view`] is what the UI draws.
//! The UI itself is static HTML/CSS/JS in `ui/`; see docs/APP.md.

pub mod app;
pub mod apply;
pub mod batch;
pub mod diagnose;
pub mod edit;
pub mod menu;
pub mod report_dto;
pub mod rowstyle;
pub mod session;
pub mod view;

pub use app::run;
pub use session::Session;
pub use view::View;
