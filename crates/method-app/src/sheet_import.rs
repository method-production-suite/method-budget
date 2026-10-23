//! File > Import Spreadsheet…: detail lines from an Excel workbook, a CSV
//! file or tab-delimited text, with a column mapping (roadmap 2.2; the
//! readers and the mapping are `method_format::import`).
//!
//! - **The file.** Rust shows the Open dialog and reads the file (at most
//!   [`MAX_FILE_BYTES`]); the page only ever sees its name, and names it
//!   afterwards by a token. Only the last file picked is kept.
//! - **The mapping.** The page picks a sheet, the header row and a column
//!   for each field; Rust guesses them first (`guess`), and the page may
//!   have a saved preset of its own for the sheet's layout (localStorage).
//! - **The preview** is the import worked out against the budget (or, for a
//!   new tab, the budget's chart and setup without lines) without changing
//!   it: the first lines as they will come in with what each comes to, the
//!   rows skipped or changed and why, the accounts, categories and setup
//!   items to be added or left off, the reconciliation against the sheet's
//!   own totals, and the grand total before and after.
//! - **Importing** into the budget is one undo step. Into a new tab, the tab
//!   starts as New From Template would make it from this budget (chart,
//!   setup and settings, no lines, info, baseline or versions) and the
//!   import is its first undo step, so it starts with unsaved changes.
//!
//! What the dialog does with the file is in method-session's
//! `sheet_import` module (re-exported here); these are the Open dialog,
//! reading the file and the Tauri commands.

use std::path::PathBuf;

use method_format::import::MAX_FILE_BYTES;
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, AppState, DocId};

pub use method_session::sheet_import::*;

/// Reads the file at `path` for import: at most [`MAX_FILE_BYTES`] (Import
/// Actuals reads its files the same way, src/actuals.rs).
pub(crate) fn read_capped(path: &std::path::Path) -> Result<Vec<u8>, String> {
    // One byte over the limit is read, so check_size words the refusal.
    let bytes = method_format::fsio::read_capped(path, MAX_FILE_BYTES as u64 + 1)
        .map_err(|e| format!("Couldn't read the file: {e}"))?;
    check_size(bytes.len())?;
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// Shows an Open dialog for a spreadsheet, reads it and returns its sheets.
/// `None` when cancelled.
#[tauri::command]
pub async fn sheet_import_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, SheetSources>,
    doc: Option<DocId>,
) -> Result<Option<Picked>, String> {
    let start = {
        let s = state.lock_doc(doc)?;
        s.path().and_then(|p| p.parent()).map(PathBuf::from)
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title(crate::i18n::tr("dialog.import_spreadsheet"))
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
    let bytes = read_capped(&path)?;
    crate::i18n::engine_err(method_session::sheet_import::sheet_import_pick(
        &sources, file, &bytes,
    ))
    .map(Some)
}

/// The preview of importing with the dialog's choices into budget `doc`
/// (`None`: the one in front). Changes nothing.
#[tauri::command]
pub fn sheet_import_preview(
    state: State<'_, AppState>,
    sources: State<'_, SheetSources>,
    doc: Option<DocId>,
    token: u64,
    request: Request,
) -> Result<PreviewDto, String> {
    let b = state.lock_doc(doc)?.budget().clone();
    crate::i18n::engine_dto(method_session::sheet_import::sheet_import_preview(
        &b, &sources, token, &request,
    ))
}

/// Imports with the dialog's choices: into budget `doc` as one undo step,
/// or into a new tab (in front) made from its chart and setup.
#[tauri::command]
pub fn sheet_import<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, SheetSources>,
    doc: Option<DocId>,
    token: u64,
    request: Request,
) -> Result<Imported, String> {
    let (id, message) = crate::i18n::engine_err(method_session::sheet_import::sheet_import(
        &state, &sources, doc, token, &request,
    ))?;
    Ok(Imported {
        view: finish(&window, &mut state.lock_doc(Some(id))?, None),
        message: crate::i18n::engine(&message),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_read_with_a_cap() {
        let dir = crate::safety::tests::temp_dir("sheet-import-read");
        let path = dir.join("small.csv");
        std::fs::write(&path, b"Acct,Rate\n10-01,5\n").unwrap();
        assert_eq!(read_capped(&path).unwrap().len(), 18);
        assert!(read_capped(&dir.join("missing.csv")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
