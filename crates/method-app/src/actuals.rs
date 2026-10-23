//! Actuals and the cost report (roadmap 3.7; the engine is
//! `method_core::actuals`, the spreadsheet reading `method_format::import::actuals`).
//!
//! - **File > Import Actuals…**: Rust shows the Open dialog and reads the
//!   file (CSV, tab-delimited or a workbook, as Import Spreadsheet does);
//!   the page names it afterwards by a token. The dialog maps columns
//!   (guessed from the titles), picks the date order and sign, reviews the
//!   account numbers the chart doesn't have (map, skip or keep unassigned)
//!   and the duplicates, and imports as one undo step: one import batch,
//!   which the Actuals tab can take back out whole.
//! - **The Actuals tab** lists transactions with filters, edits, reassigns
//!   and deletes them, removes imports, and sets an account's estimate to
//!   complete. Each change is one undo step.
//! - **The cost columns**: every view of a budget with actuals carries
//!   [`CostDto`], the cost figures of every account, category, level and
//!   summary row (and the lines of the account on screen), from the
//!   engine's cost report. The only pass over every transaction (the
//!   rollup) is cached by the session until the actuals or the account
//!   pattern change (`Session::cost`), so an edit to a line costs one pass
//!   over the accounts, not over the transactions.
//!
//! What the dialog and the tab do is in method-session's `actuals` module
//! (re-exported here), with the cost figures; these are the Open dialog,
//! reading the file and the Tauri commands.

use std::path::PathBuf;

use method_core::actuals::TransactionId;
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{apply, finish, AppState, DocId};
use crate::view::View;

pub use method_session::actuals::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call the Actuals tab's: list, update, delete, remove a
// batch, set an estimate to complete, lines)

/// Shows an Open dialog for an accounting export, reads it and returns its
/// sheets. `None` when cancelled.
#[tauri::command]
pub async fn actuals_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ActualsSources>,
    doc: Option<DocId>,
) -> Result<Option<Picked>, String> {
    let start = {
        let s = state.lock_doc(doc)?;
        s.path().and_then(|p| p.parent()).map(PathBuf::from)
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title(crate::i18n::tr("dialog.import_actuals"))
        .add_filter(
            crate::i18n::tr("dialog.filter_spreadsheet"),
            &["xlsx", "csv", "tsv", "txt"],
        )
        .set_parent(&window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked_file) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked_file.into_path().map_err(|e| e.to_string())?;
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bytes = crate::sheet_import::read_capped(&path)?;
    crate::i18n::engine_err(method_session::actuals::actuals_pick(
        &sources, file, &bytes,
    ))
    .map(Some)
}

/// The preview of importing with the dialog's choices into budget `doc`.
/// Changes nothing.
#[tauri::command]
pub fn actuals_preview(
    state: State<'_, AppState>,
    sources: State<'_, ActualsSources>,
    doc: Option<DocId>,
    token: u64,
    request: Request,
) -> Result<PreviewDto, String> {
    let s = state.lock_doc(doc)?;
    crate::i18n::engine_err(method_session::actuals::actuals_preview(
        s.budget(),
        &sources,
        token,
        &request,
    ))
}

/// Imports with the dialog's choices into budget `doc`, as one undo step.
/// `imported` is the page's local time (ISO 8601 with its offset).
#[tauri::command]
pub fn actuals_import<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ActualsSources>,
    doc: Option<DocId>,
    token: u64,
    request: Request,
    imported: String,
) -> Result<Imported, String> {
    let mut s = state.lock_doc(doc)?;
    let message = crate::i18n::engine_dto(method_session::actuals::actuals_import(
        &mut s, &sources, token, &request, &imported,
    ))?;
    Ok(Imported {
        view: finish(&window, &mut s, None),
        message,
    })
}

/// The Actuals tab's list. Changes nothing.
#[tauri::command]
pub fn actuals_list(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    filter: FilterInput,
) -> Result<ListDto, String> {
    let s = state.lock_doc(doc)?;
    crate::i18n::engine_err(method_session::actuals::actuals_list(s.budget(), &filter))
}

/// Changes, reassigns or unassigns transactions `ids` (one undo step).
#[tauri::command]
pub fn actuals_update<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    ids: Vec<TransactionId>,
    edit: EditInput,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::actuals::actuals_update(s, &ids, &edit)
    })
}

/// Deletes transactions `ids` (one undo step).
#[tauri::command]
pub fn actuals_delete<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    ids: Vec<TransactionId>,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::actuals::actuals_delete(s, &ids)
    })
}

/// Takes import `batch` back out with its transactions (one undo step).
#[tauri::command]
pub fn actuals_remove_batch<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    batch: u64,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::actuals::actuals_remove_batch(s, batch)
    })
}

/// Sets account `account`'s estimate-to-complete method (one undo step):
/// `method` is `budget_minus_actual`, `manual` (with `value` an amount) or
/// `percent_complete` (with `value` a percent).
#[tauri::command]
pub fn actuals_set_etc<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    account: String,
    method: String,
    value: String,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::actuals::actuals_set_etc(s, &account, &method, &value)
    })
}

/// The costed lines of account `account`, for the line a transaction pays
/// for. Changes nothing.
#[tauri::command]
pub fn actuals_lines(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    account: String,
) -> Result<Vec<LineDto>, String> {
    let s = state.lock_doc(doc)?;
    Ok(method_session::actuals::actuals_lines(s.budget(), &account))
}
