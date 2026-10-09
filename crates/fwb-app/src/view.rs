//! What the UI renders after every command: the budget in its file-format JSON,
//! the totals, any calculation problems, and file/undo state.

use std::path::Path;

use fwb_core::{Budget, RowId};
use serde::Serialize;

use crate::diagnose::{error_message, Issue, Outcome};
use crate::report_dto::ReportDto;

#[derive(Clone, Debug, Serialize)]
pub struct View {
    /// The `budget` object exactly as `docs/FORMAT.md` describes it (fields at
    /// their default are left out).
    pub budget: serde_json::Value,
    /// Totals, or `None` when they can't be computed at all.
    pub report: Option<ReportDto>,
    /// True when `report` treats the flagged cells in `issues` as blank.
    pub partial: bool,
    /// The engine's first error, if any.
    pub error: Option<ErrorDto>,
    /// Every problem found, so each bad cell can be flagged.
    pub issues: Vec<Issue>,
    pub file: FileDto,
    pub can_undo: bool,
    pub can_redo: bool,
    /// A row the last command created, for the UI to select.
    pub created_row: Option<RowId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ErrorDto {
    /// Where, like `account 11-01 line 2 (Producer): rate`.
    pub at: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileDto {
    pub path: Option<String>,
    pub name: String,
    pub dirty: bool,
}

/// The budget as file-format JSON (the `budget` member of the document).
pub fn budget_json(b: &Budget) -> serde_json::Value {
    let text = fwb_format::to_string(b);
    let mut doc: serde_json::Value =
        serde_json::from_str(&text).expect("the writer produces valid JSON");
    doc.get_mut("budget")
        .map(serde_json::Value::take)
        .unwrap_or_default()
}

impl View {
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        budget: &Budget,
        outcome: &Outcome,
        path: Option<&Path>,
        name: String,
        dirty: bool,
        can_undo: bool,
        can_redo: bool,
        created_row: Option<RowId>,
    ) -> View {
        View {
            budget: budget_json(budget),
            report: outcome.report.as_ref().map(ReportDto::from),
            partial: outcome.partial(),
            error: outcome.error.as_ref().map(|e| ErrorDto {
                at: e.at.clone(),
                message: error_message(e),
            }),
            issues: outcome.issues.clone(),
            file: FileDto {
                path: path.map(|p| p.display().to_string()),
                name,
                dirty,
            },
            can_undo,
            can_redo,
            created_row,
        }
    }
}
