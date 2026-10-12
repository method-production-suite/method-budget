//! Quick entry on the detail grid: adding a setup item from the cell that
//! names it, and which tags an inserted line copies from its neighbor. What
//! they do is in method-session's `quickadd` module (re-exported here);
//! these are the Tauri commands.

use method_core::RowId;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{apply, AppState, DocId};
use crate::view::View;

pub use method_session::quickadd::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json)

/// `{rowId, field, value, create: {kind, name, value?}}`: adds the setup
/// item and sets the cell, as one undo step.
#[tauri::command]
pub fn set_cell_creating<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_id: RowId,
    field: String,
    value: String,
    create: CreateInput,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::quickadd::set_cell_creating(s, row_id, &field, &value, &create)
    })
}

/// `{defaults: {fringes, groups, set, location, currency}}`.
#[tauri::command]
pub fn set_new_line_defaults<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    defaults: NewLineDefaultsInput,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::quickadd::set_new_line_defaults(s, &defaults)
    })
}
