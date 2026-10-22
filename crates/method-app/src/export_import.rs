//! File > Import Budget Export…: a whole budget from another budgeting
//! program's open export (budget exchange JSON or XML, its Excel or
//! tab-delimited export), with the reconciliation shown before anything is
//! imported (the importer is `method_format::import::exchange`; roadmap
//! 1.16, docs/APP.md).
//!
//! - **The file.** Rust shows the Open dialog and reads the file; the page
//!   names it by a token afterwards, as Import Spreadsheet does.
//! - **The review** says what was read (categories, accounts, lines, setup),
//!   the notes (what was worked out from the export's figures and should
//!   be checked, and what couldn't come over), and the reconciliation: the
//!   imported budget's totals against the export's own, kind by kind, with
//!   every figure that differs. The layout is detected; the dialog can read
//!   the file as another one.
//! - **Importing** opens the budget in a new tab, unsaved (its first undo
//!   step), like Import Chart of Accounts' new budget. Its first Save starts
//!   in the export's folder under the export's name with `.mthd`.
//! - **Converting.** An export opened with Open, from Finder or dropped on
//!   the window (src/open_any.rs) gets the same review, worded as a
//!   conversion, and the new tab says once where it came from, that the
//!   export is left as it was and that saving keeps a `.mthd` (decisions.md,
//!   entry 67). Nothing is ever written in the export's own layout.
//!
//! The review and the import are in method-session's `export_import`
//! module (re-exported here); these are the Open dialog, reading the file
//! and the Tauri commands.

use std::path::PathBuf;

use method_format::import::exchange;
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, AppState, DocId};
use crate::sheet_import::read_capped;

pub use method_session::export_import::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// Shows an Open dialog for a budget export, reads it and returns the
/// review. `None` when cancelled.
#[tauri::command]
pub async fn export_import_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ExportSources>,
    doc: Option<DocId>,
) -> Result<Option<Review>, String> {
    let start = {
        let s = state.lock_doc(doc)?;
        s.path().and_then(|p| p.parent()).map(PathBuf::from)
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title(crate::i18n::tr("dialog.import_budget_export"))
        .add_filter(
            crate::i18n::tr("dialog.filter_budget_export"),
            exchange::READABLE_EXTENSIONS,
        )
        .set_parent(&window);
    for f in exchange::READABLE {
        dialog = dialog.add_filter(f.label(), f.extensions());
    }
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
    method_session::export_import::export_import_pick(&sources, file, bytes, Some(path)).map(Some)
}

/// The review of the picked export read as `format` (`None`: detected).
/// Changes nothing.
#[tauri::command]
pub fn export_import_preview(
    sources: State<'_, ExportSources>,
    token: u64,
    format: Option<String>,
) -> Result<Review, String> {
    method_session::export_import::export_import_preview(&sources, token, format.as_deref())
}

/// Imports the picked export as a new budget in a new tab.
#[tauri::command]
pub fn export_import<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ExportSources>,
    token: u64,
    format: Option<String>,
) -> Result<Imported, String> {
    let (id, message) =
        method_session::export_import::export_import(&state, &sources, token, format.as_deref())?;
    Ok(Imported {
        view: finish(&window, &mut state.lock_doc(Some(id))?, None),
        message,
    })
}
