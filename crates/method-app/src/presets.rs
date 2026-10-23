//! Tag presets and line blocks, kept in the budget (decision 15; docs/APP.md,
//! "Presets"). What they do is in method-session's `presets` module
//! (re-exported here); these are the Tauri commands.

use method_core::RowId;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{apply, finish, AppState, DocId};
use crate::view::View;

pub use method_session::presets::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json and
// tools.json)

/// `{index: number | null, preset}`, the preset in the file format's shape
/// (docs/FORMAT.md, "Tag preset").
#[tauri::command]
pub fn save_tag_preset<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: Option<usize>,
    preset: serde_json::Value,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::presets::save_tag_preset(s, index, preset)
    })
}

/// `{rowId, name, parts}`: a new tag preset from a line's tags.
#[tauri::command]
pub fn tag_preset_from_line<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_id: RowId,
    name: String,
    parts: Option<TagParts>,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::presets::tag_preset_from_line(s, row_id, &name, parts)
    })
}

/// `{index, mode, rowIds, accounts, categories}`: puts tag preset `index` on
/// the selection (rows by id, whole accounts by number, whole categories by
/// topsheet position), `mode` `add` or `replace`. One undo step.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn apply_tag_preset<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
    mode: String,
    row_ids: Option<Vec<RowId>>,
    accounts: Option<Vec<String>>,
    categories: Option<Vec<usize>>,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::presets::apply_tag_preset(
            s,
            index,
            &mode,
            row_ids.as_deref().unwrap_or_default(),
            accounts.as_deref().unwrap_or_default(),
            categories.as_deref().unwrap_or_default(),
        )
    })
}

/// `{name, rowIds, keep}`: a new line block from the rows.
#[tauri::command]
pub fn save_line_preset<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    row_ids: Vec<RowId>,
    keep: Option<BlockFields>,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::presets::save_line_preset(s, &name, &row_ids, keep)
    })
}

/// `{index}`: what inserting the block would add back, before inserting it.
#[tauri::command]
pub fn line_preset_preview(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
) -> Result<crate::clipboard::Preview, String> {
    crate::i18n::engine_dto(method_session::presets::line_preset_preview(
        &*state.lock_doc(doc)?,
        index,
    ))
}

/// `{index, account, beforeRow}`: inserts copies of the block above the row
/// (the account's end when `null`), as one undo step. The view's
/// `created_row` is the first new row.
#[tauri::command]
pub fn insert_line_preset<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
    account: String,
    before_row: Option<RowId>,
) -> Result<View, String> {
    let mut s = state.lock_doc(doc)?;
    let first = crate::i18n::engine_err(method_session::presets::insert_line_preset(
        &mut s, index, &account, before_row,
    ))?;
    Ok(finish(&window, &mut s, first))
}

/// `{which: "tags" | "lines", index, name}`.
#[tauri::command]
pub fn rename_tool_preset<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    which: String,
    index: usize,
    name: String,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::presets::rename_tool_preset(s, &which, index, &name)
    })
}

/// `{which, index}`.
#[tauri::command]
pub fn delete_tool_preset<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    which: String,
    index: usize,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::presets::delete_tool_preset(s, &which, index)
    })
}

/// `{which, index, to}`.
#[tauri::command]
pub fn move_tool_preset<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    which: String,
    index: usize,
    to: usize,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::presets::move_tool_preset(s, &which, index, to)
    })
}
