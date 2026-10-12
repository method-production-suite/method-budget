//! The apply panel: puts fringes and groups on the selected rows or takes them
//! off, gives the rows a set, a location or a currency, makes and removes
//! fringe ranges, and sums up the selection. What they do is in
//! method-session's `apply` module (re-exported here); these are the Tauri
//! commands.

use method_core::RowId;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{apply, AppState, DocId};
use crate::view::View;

pub use method_session::apply::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json)

/// The panel's numbers for a selection: rows by id, whole accounts by
/// number, whole categories by topsheet position. Rows and accounts that are
/// gone are left out; only a closed budget (`doc`) is refused.
#[tauri::command]
pub fn apply_summary(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Option<Vec<RowId>>,
    accounts: Option<Vec<String>>,
    categories: Option<Vec<usize>>,
) -> Result<ApplySummary, String> {
    Ok(method_session::apply::apply_summary(
        &mut *state.lock_doc(doc)?,
        row_ids.as_deref().unwrap_or_default(),
        accounts.as_deref().unwrap_or_default(),
        categories.as_deref().unwrap_or_default(),
    ))
}

/// Puts a fringe or group on the rows (`on`), or takes it off.
#[tauri::command]
pub fn apply_to_rows<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
    field: String,
    name: String,
    on: bool,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::apply::apply_to_rows(s, &row_ids, &field, &name, on)
    })
}

/// Gives the rows a set, location or currency; `null` clears it.
#[tauri::command]
pub fn assign_to_rows<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
    field: String,
    value: Option<String>,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::apply::assign_to_rows(s, &row_ids, &field, value.as_deref())
    })
}

/// Takes every fringe or group (`field`) off the rows.
#[tauri::command]
pub fn clear_from_rows<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
    field: String,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::apply::clear_from_rows(s, &row_ids, &field)
    })
}

/// Sets exactly `tags` on the rows (fringes, groups, set, location,
/// currency), as a tag preset's replace does.
#[tauri::command]
pub fn replace_tags_on_rows<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
    tags: TagsInput,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::apply::replace_tags_on_rows(s, &row_ids, &tags)
    })
}

#[tauri::command]
pub fn make_fringe_range<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::apply::make_fringe_range(s, &row_ids)
    })
}

#[tauri::command]
pub fn clear_fringe_range<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::apply::clear_fringe_range(s, &row_ids)
    })
}

#[cfg(test)]
mod tests {

    /// The new commands are registered, listed and allowed where they are
    /// used: the apply edits in the tools window too, the charge preview in
    /// the Budget Setup window (its Charges tab, src/setupwin.rs).
    #[test]
    fn apply_and_charge_preview_commands_are_wired() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN: &str = include_str!("../capabilities/main.json");
        const TOOLS: &str = include_str!("../capabilities/tools.json");
        const SETUP: &str = include_str!("../capabilities/setup.json");
        for (handler, c) in [
            ("crate::apply::clear_from_rows,", "clear_from_rows"),
            (
                "crate::apply::replace_tags_on_rows,",
                "replace_tags_on_rows",
            ),
            ("            preview_charge,", "preview_charge"),
        ] {
            assert!(APP_RS.contains(handler), "{c} not in generate_handler!");
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN.contains(&allow), "{c} not allowed for the main window");
            let (window, caps) = if c == "preview_charge" {
                ("Budget Setup", SETUP)
            } else {
                ("tools", TOOLS)
            };
            assert!(
                caps.contains(&allow),
                "{c} not allowed for the {window} window"
            );
        }
    }
}
