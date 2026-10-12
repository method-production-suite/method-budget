//! Method Budget's open budgets with no window, for the desktop app and a
//! browser alike (docs/research/web-session.md).
//!
//! [`edit`] changes a budget, [`diagnose`] recalculates it without giving
//! up on the first bad cell, and [`report_dto`] is the engine's report as
//! the page reads it. Nothing here touches the file system, starts a
//! thread or reads a clock, so the crate builds for
//! `wasm32-unknown-unknown`.

pub mod diagnose;
pub mod edit;
pub mod report_dto;
