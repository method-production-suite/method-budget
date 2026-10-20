//! Rate tables the user keeps (roadmap 3.9; decisions.md, entry 90): the
//! commands Budget Setup's Rate Tables tab (ui/js/rates.js) and a line's
//! Fill Rate from Table (ui/js/ratepick.js) call. What they do is in
//! method-session's `rates` module (re-exported here); the only thing the
//! desktop adds is the Open dialog for a spreadsheet.
//!
//! Figures cross the bridge as strings, never JSON floats.

use std::path::PathBuf;

use method_core::RowId;
use method_format::setup::rates::RateMapping;
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{AppState, DocId};
use crate::sheet_import::{read_capped, SheetSources};
use crate::view::View;

pub use method_session::rates::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs and build.rs; the main window and the
// Budget Setup window, capabilities/main.json and setup.json)

/// Saves a rate table in budget `doc`: a new one, or in place of
/// `replacing` (which may rename it). One undo step.
#[tauri::command(rename_all = "snake_case")]
pub fn rate_table_save<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    table: RateTableDto,
    replacing: Option<String>,
) -> Result<View, String> {
    crate::app::apply(&window, &state, doc, |s| {
        method_session::rates::rate_table_save(s, &table, replacing.as_deref())
    })
}

/// Deletes the rate table `name` from budget `doc`. One undo step.
#[tauri::command(rename_all = "snake_case")]
pub fn rate_table_delete<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> Result<View, String> {
    crate::app::apply(&window, &state, doc, |s| {
        method_session::rates::rate_table_delete(s, &name)
    })
}

/// Every classification line `row_id` of budget `doc` could take its rate
/// from, per `unit` (the line's own when `None`). Changes nothing.
#[tauri::command(rename_all = "snake_case")]
pub fn rate_choices(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_id: RowId,
    unit: Option<String>,
) -> Result<RateChoices, String> {
    let s = state.lock_doc(doc)?;
    method_session::rates::choices(s.budget(), row_id, unit.as_deref())
}

/// Fills line `row_id`'s rate from `classification` of `table`, as one
/// undo step.
#[tauri::command(rename_all = "snake_case")]
#[allow(clippy::too_many_arguments)]
pub fn rate_fill<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_id: RowId,
    table: String,
    classification: String,
    unit: Option<String>,
    as_of: Option<String>,
) -> Result<View, String> {
    crate::app::apply(&window, &state, doc, |s| {
        method_session::rates::rate_fill(
            s,
            row_id,
            &table,
            &classification,
            unit.as_deref(),
            as_of.as_deref(),
        )
    })
}

/// Updates lines `row_ids` (every changed one when empty) to what their
/// tables say now, as one undo step.
#[tauri::command(rename_all = "snake_case")]
pub fn rate_update<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
) -> Result<View, String> {
    crate::app::apply(&window, &state, doc, |s| {
        method_session::rates::rate_update(s, &row_ids)
    })
}

/// Shows an Open dialog for a spreadsheet, reads it and returns its first
/// sheet read as a rate table (guessed mapping). `None` when cancelled.
#[tauri::command(rename_all = "snake_case")]
pub async fn rate_import_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, SheetSources>,
    doc: Option<DocId>,
) -> Result<Option<RateImportRead>, String> {
    let start = {
        let s = state.lock_doc(doc)?;
        s.path().and_then(|p| p.parent()).map(PathBuf::from)
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Import a rate table")
        .add_filter("Spreadsheet", &["xlsx", "csv", "tsv", "txt"])
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
    let s = state.lock_doc(doc)?;
    method_session::rates::rate_import_pick(&s, &sources, name, &bytes).map(Some)
}

/// Reads sheet `index` of the picked file again under `mapping` (the
/// guess when `None`), named `name`.
#[tauri::command(rename_all = "snake_case")]
pub fn rate_import_read(
    state: State<'_, AppState>,
    sources: State<'_, SheetSources>,
    doc: Option<DocId>,
    token: u64,
    index: usize,
    mapping: Option<RateMapping>,
    name: Option<String>,
) -> Result<RateImportRead, String> {
    let s = state.lock_doc(doc)?;
    method_session::rates::rate_import_read(&s, &sources, token, index, mapping, name.as_deref())
}

#[cfg(test)]
mod tests {
    const COMMANDS: [&str; 7] = [
        "rate_table_save",
        "rate_table_delete",
        "rate_choices",
        "rate_fill",
        "rate_update",
        "rate_import_pick",
        "rate_import_read",
    ];

    /// Every rate table command is wired for the main window; the Budget
    /// Setup window gets the ones its tab calls.
    #[test]
    fn rate_commands_are_wired() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN: &str = include_str!("../capabilities/main.json");
        const SETUP: &str = include_str!("../capabilities/setup.json");
        for c in COMMANDS {
            assert!(
                APP_RS.contains(&format!("crate::rates::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN.contains(&allow), "{c} not allowed for the main window");
        }
        for c in [
            "rate_table_save",
            "rate_table_delete",
            "rate_update",
            "rate_import_pick",
            "rate_import_read",
        ] {
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(
                SETUP.contains(&allow),
                "{c} not allowed for the setup window"
            );
        }
    }
}
