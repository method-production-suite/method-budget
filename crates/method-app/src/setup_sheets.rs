//! Setup lists as spreadsheets (decisions.md, entry 50; docs/APP.md,
//! "Setup lists in a spreadsheet"): File > Export Setup Lists… writes the
//! budget's setup kinds as an .xlsx (a sheet per kind) or CSV files (one
//! per kind), and File > Import Setup Lists… reads them back with a column
//! mapping per sheet (method_format::setup::sheet), checks every row (problems
//! listed with their row numbers, nothing imported while any is left) and
//! hands the result to Import Setup's review (src/import_setup.rs), so what
//! differs keeps this budget's version unless the user picks otherwise and
//! the import is one undo step. Into a pack: open the pack as a budget tab
//! and import there.
//!
//! The dialogs are Rust's; the page names the file by a token. Reading the
//! sheets and making the files are method-session's `setup_sheets` module
//! (re-exported here); these are the dialogs, the files and the Tauri
//! commands.

use std::path::PathBuf;

use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{AppState, DocId};
use crate::import_setup::ImportSources;
use crate::sheet_import::{read_capped, SheetSources};

pub use method_session::setup_sheets::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs and build.rs; the main window's File menu
// only, capabilities/main.json)

/// Writes budget `doc`'s setup `kinds` (every kind when empty) as `format`
/// (`xlsx` or `csv`) where Rust's Save dialog says.
#[tauri::command]
pub async fn setup_sheet_export<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    kinds: Vec<String>,
    format: String,
) -> Result<SheetExported, String> {
    let (ex, dir) = {
        let s = state.lock_doc(doc)?;
        (
            method_session::setup_sheets::setup_sheet_export(&s, &kinds, &format)?,
            s.path().and_then(|p| p.parent()).map(PathBuf::from),
        )
    };
    let ext = ex.ext();
    let (title, filter) = if ext == "xlsx" {
        (
            crate::i18n::tr("dialog.export_setup_lists_xlsx"),
            crate::i18n::tr("dialog.filter_xlsx"),
        )
    } else {
        (
            crate::i18n::tr("dialog.export_setup_lists_csv"),
            crate::i18n::tr("dialog.filter_csv"),
        )
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title(title)
        .add_filter(filter, &[ext])
        .set_file_name(format!("{}.{ext}", ex.stem))
        .set_parent(&window);
    if let Some(dir) = dir {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_save_file() else {
        return Ok(SheetExported::default());
    };
    let out =
        crate::export::with_extension(picked.into_path().map_err(|e| e.to_string())?, ex.format);
    let files = crate::export::write_files(&state.docs(), &ex.files(&out)?)?;
    Ok(ex.exported(files))
}

/// Shows an Open dialog for a spreadsheet of setup lists, reads it and
/// returns its sheets with guessed kinds and mappings. `None` when
/// cancelled.
#[tauri::command]
pub async fn setup_sheet_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, SheetSources>,
    doc: Option<DocId>,
) -> Result<Option<SetupSheetPicked>, String> {
    let start = {
        let s = state.lock_doc(doc)?;
        s.path().and_then(|p| p.parent()).map(PathBuf::from)
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title(crate::i18n::tr("dialog.import_setup_lists"))
        .add_filter(
            crate::i18n::tr("dialog.filter_spreadsheet"),
            &["xlsx", "csv", "tsv", "txt"],
        )
        .set_parent(&window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(file) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bytes = read_capped(&path)?;
    method_session::setup_sheets::setup_sheet_pick(&sources, name, &bytes).map(Some)
}

/// The guessed mapping of sheet `index` as `kind`, and its titles.
#[tauri::command]
pub fn setup_sheet_guess(
    sources: State<'_, SheetSources>,
    token: u64,
    index: usize,
    kind: String,
) -> Result<PickedSheet, String> {
    method_session::setup_sheets::setup_sheet_guess(&sources, token, index, &kind)
}

/// Reads the chosen sheets for budget `doc`: their problems with row
/// numbers, or (with none) Import Setup's review, whose token
/// `import_setup` takes.
#[tauri::command]
pub fn setup_sheet_review(
    state: State<'_, AppState>,
    sheets: State<'_, SheetSources>,
    imports: State<'_, ImportSources>,
    doc: Option<DocId>,
    token: u64,
    choices: Vec<SheetChoice>,
) -> Result<SheetReview, String> {
    let s = state.lock_doc(doc)?;
    method_session::setup_sheets::setup_sheet_review(&s, &sheets, &imports, token, &choices)
}

#[cfg(test)]
mod tests {
    #[test]
    fn sheet_commands_are_wired_for_the_main_window_only() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN: &str = include_str!("../capabilities/main.json");
        const TOOLS: &str = include_str!("../capabilities/tools.json");
        const SETUP: &str = include_str!("../capabilities/setup.json");
        for c in [
            "setup_sheet_export",
            "setup_sheet_pick",
            "setup_sheet_guess",
            "setup_sheet_review",
        ] {
            assert!(
                APP_RS.contains(&format!("crate::setup_sheets::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN.contains(&allow), "{c} not allowed for the main window");
            assert!(!TOOLS.contains(&allow), "{c} allowed for the tools window");
            assert!(!SETUP.contains(&allow), "{c} allowed for the setup window");
        }
    }
}
