//! The overtime helper (roadmap 3.10; decisions.md, entry 43): the commands
//! the tools panel's Overtime tab calls. What they do is in method-session's
//! `overtime` module (re-exported here).
//!
//! Decimals cross the bridge as strings, never JSON floats.

use method_core::RowId;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{AppState, DocId};
use crate::view::View;

pub use method_session::overtime::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and both capabilities: the
// Overtime tab can be in the tools window)

/// What the form's numbers come to. Changes nothing.
#[tauri::command(rename_all = "snake_case")]
pub fn ot_calculate(
    pattern: PatternDto,
    base_rate: String,
    hours_per_day: String,
    days_per_week: u8,
) -> Result<ResultDto, String> {
    method_session::overtime::ot_calculate(&pattern, &base_rate, &hours_per_day, days_per_week)
}

/// Writes the overtime result into line `row_id` of budget `doc`, as its
/// rate or as one line per multiplier (`write`: "rate" or "split"), per day
/// or week (`period`), as one undo step.
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "snake_case")]
pub fn ot_apply<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_id: RowId,
    pattern: PatternDto,
    base_rate: String,
    hours_per_day: String,
    days_per_week: u8,
    period: String,
    write: String,
) -> Result<View, String> {
    crate::app::apply(&window, &state, doc, |s| {
        method_session::overtime::ot_apply(
            s,
            row_id,
            &pattern,
            &base_rate,
            &hours_per_day,
            days_per_week,
            &period,
            &write,
        )
    })
}

/// Saves a named overtime pattern in budget `doc`: a new one, or in place of
/// `replacing` (which may rename it). One undo step.
#[tauri::command(rename_all = "snake_case")]
pub fn ot_save_pattern<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    pattern: PatternDto,
    replacing: Option<String>,
) -> Result<View, String> {
    crate::app::apply(&window, &state, doc, |s| {
        method_session::overtime::ot_save_pattern(s, &pattern, replacing.as_deref())
    })
}

/// Deletes the overtime pattern `name` from budget `doc`. One undo step.
#[tauri::command(rename_all = "snake_case")]
pub fn ot_delete_pattern<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> Result<View, String> {
    crate::app::apply(&window, &state, doc, |s| {
        method_session::overtime::ot_delete_pattern(s, &name)
    })
}
