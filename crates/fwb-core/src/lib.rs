//! Film Wizard Budgeting engine.
//!
//! Clean-room: written from how film budgets work and from observed behaviour of
//! budgeting tools, never from proprietary code or binaries. See CLEAN-ROOM.md.

pub mod calc;
pub mod coding;
pub mod compare;
pub mod decimal;
pub mod edit;
pub mod expr;
pub mod model;
pub mod report;

pub use calc::breakdown::{selection_totals, SelectionTotals};
pub use calc::{recalc, CalcError, ErrorKind, RecalcOptions};
pub use compare::{variance, Change, Variance};
pub use decimal::Dec;
pub use edit::delete_global;
pub use model::*;
pub use report::*;

/// Money in integer cents, so totals never drift from float rounding.
pub type Cents = i64;
