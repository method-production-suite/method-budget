//! Film Wizard Budgeting engine.
//!
//! Clean-room: written from how film budgets work and from observed behaviour of
//! budgeting tools, never from proprietary code or binaries. See CLEAN-ROOM.md.

pub mod calc;
pub mod coding;
pub mod compare;
pub mod comparison;
pub mod decimal;
pub mod edit;
pub mod expr;
pub mod model;
pub mod presets;
pub mod rename;
pub mod report;
pub mod report_preset;
pub mod sample;
pub mod starter;
pub mod versions;

pub use calc::breakdown::{selection_totals, SelectionTotals};
pub use calc::{recalc, CalcError, ErrorKind, RecalcOptions};
pub use compare::{variance, Change, Variance};
pub use comparison::{
    compare, compare_account, AccountComparison, Aggregate, CompareAccount, CompareCategory,
    CompareLevel, CompareLine, CompareRow, CompareTotals, Comparison, Diff, RowMatch, MAX_COMPARED,
    MIN_COMPARED,
};
pub use decimal::Dec;
pub use edit::delete_global;
pub use model::*;
pub use presets::{
    LinePreset, SetupItems, TagPreset, MAX_BLOCK_ROWS, MAX_LINE_PRESETS, MAX_TAG_PRESETS,
    MAX_TOOL_PRESET_NAME_CHARS,
};
pub use rename::{rename, RenameError, SetupKind};
pub use report::*;
pub use report_preset::{
    Band, BandField, FontSize, PresetError, ReportColumn, ReportPreset, ReportSection, ReportSpec,
    MAX_BAND_TEXT_CHARS, MAX_PRESETS, MAX_PRESET_NAME_CHARS,
};
pub use versions::{delete_version, rename_version, restore_version, save_version, VersionError};

/// Money in integer cents, so totals never drift from float rounding.
pub type Cents = i64;
