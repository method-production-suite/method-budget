//! Film Wizard Budgeting engine.
//!
//! Clean-room: written from how film budgets work and from observed behaviour of
//! budgeting tools, never from proprietary code or binaries. See CLEAN-ROOM.md.

pub mod calc;
pub mod decimal;
pub mod expr;
pub mod model;

pub use calc::{recalc, CalcError, ErrorKind, RecalcOptions, Report};
pub use decimal::Dec;
pub use model::{Account, Budget, Category, ContractualCharge, Detail, Formula, Fringe, Unit};

/// Money in integer cents, so totals never drift from float rounding.
pub type Cents = i64;
