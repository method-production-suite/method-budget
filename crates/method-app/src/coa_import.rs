//! File > Import Chart of Accounts… and the bundled chart templates in New
//! From Template (the importer is `method_format::import::coa`, the chart and
//! applying it `method_core::coa`; docs/TEMPLATES.md).
//!
//! - **The file.** Rust shows the Open dialog and reads the file: any sheet
//!   of a workbook, a CSV or tab-delimited file, or another budget file
//!   (its own chart, no mapping). The page names it by a token afterwards.
//! - **The mapping.** As Import Spreadsheet does: a sheet, a header row and
//!   a column for each field, guessed first; plus the numbering pattern,
//!   inferred from the numbers and offered for change, and whether to keep
//!   sub-accounts and add our default production totals.
//! - **The preview** lists the chart as it will come in, with the rows
//!   skipped and the warnings (duplicates, accounts with no category row,
//!   empty categories), and for "apply to this budget" what would be added,
//!   renamed and kept.
//! - **Importing** opens a new tab with the chart and no lines (its first
//!   undo step, so it starts unsaved), or applies it to the budget as one
//!   undo step: missing categories and accounts added, existing ones
//!   renamed by number, nothing with lines removed.
//!
//! What the dialog does with the file is in method-session's `coa_import`
//! module (re-exported here); these are the Open dialog, reading the file
//! and the Tauri commands.

use std::path::PathBuf;

use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, AppState, DocId};
use crate::sheet_import::read_capped;
use crate::view::View;

pub use method_session::coa_import::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// Shows an Open dialog for a chart of accounts and reads it. `None` when
/// cancelled.
#[tauri::command]
pub async fn coa_import_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, CoaSources>,
    doc: Option<DocId>,
) -> Result<Option<Picked>, String> {
    let start = {
        let s = state.lock_doc(doc)?;
        s.path().and_then(|p| p.parent()).map(PathBuf::from)
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title(crate::i18n::tr("dialog.import_chart_of_accounts"))
        .add_filter(
            crate::i18n::tr("dialog.filter_chart_of_accounts"),
            &["xlsx", "csv", "tsv", "txt", method_format::EXTENSION],
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
    let bytes = read_capped(&path)?;
    crate::i18n::engine_err(method_session::coa_import::coa_import_pick(
        &sources, file, &bytes,
    ))
    .map(Some)
}

/// The preview of importing with the dialog's choices (against budget
/// `doc` for "apply to this budget"). Changes nothing.
#[tauri::command]
pub fn coa_import_preview(
    state: State<'_, AppState>,
    sources: State<'_, CoaSources>,
    doc: Option<DocId>,
    token: u64,
    request: Request,
) -> Result<PreviewDto, String> {
    let b = state.lock_doc(doc)?.budget().clone();
    crate::i18n::engine_err(method_session::coa_import::coa_import_preview(
        &b, &sources, token, &request,
    ))
}

/// Imports with the dialog's choices: a new tab with the chart, or applied
/// to budget `doc` as one undo step.
#[tauri::command]
pub fn coa_import<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, CoaSources>,
    doc: Option<DocId>,
    token: u64,
    request: Request,
) -> Result<Imported, String> {
    let (id, message) = crate::i18n::engine_err(method_session::coa_import::coa_import(
        &state, &sources, doc, token, &request,
    ))?;
    Ok(Imported {
        view: finish(&window, &mut state.lock_doc(Some(id))?, None),
        message: crate::i18n::engine(&message),
    })
}

/// The bundled chart templates.
#[tauri::command]
pub fn chart_templates() -> Vec<ChartTemplate> {
    method_session::coa_import::chart_templates()
}

/// Opens bundled chart `id` as a new budget in a new tab (as New From
/// Template does with a file: no file, Untitled, no unsaved changes).
#[tauri::command]
pub fn chart_template_new<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<View, String> {
    let doc = crate::i18n::engine_err(method_session::coa_import::chart_template_new(&state, &id))?;
    Ok(finish(&window, &mut state.lock_doc(Some(doc))?, None))
}
