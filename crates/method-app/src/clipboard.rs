//! Copying and pasting whole rows, accounts and categories with everything
//! attached (docs/APP.md, "Copy and paste rows"; decisions.md, entry 23).
//! What they do is in method-session's `clipboard` module (re-exported
//! here), the app's one row clipboard ([`ClipboardState`]) in Tauri's state;
//! these are the Tauri commands.

use tauri::{Runtime, State, WebviewWindow};

use crate::app::{apply, finish, AppState, DocId};
use crate::view::View;

pub use method_session::clipboard::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json)

/// `{pick: {what, rowIds | accounts | categories}, text, onClipboard}`:
/// copies whole rows, accounts or categories into the app's clipboard.
/// `text` is what the page put on the system clipboard; `onClipboard` false
/// means it couldn't, so only Paste Rows finds the copy.
#[tauri::command]
pub fn clip_copy(
    state: State<'_, AppState>,
    clipboard: State<'_, ClipboardState>,
    doc: Option<DocId>,
    pick: Pick,
    text: String,
    on_clipboard: bool,
) -> Result<ClipInfo, String> {
    let s = state.lock_doc(doc)?;
    method_session::clipboard::clip_copy(&s, &clipboard, &pick, &text, on_clipboard)
}

/// Like `clip_copy`, then deletes what was copied, as one undo step.
#[tauri::command]
pub fn clip_cut<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    clipboard: State<'_, ClipboardState>,
    doc: Option<DocId>,
    pick: Pick,
    text: String,
    on_clipboard: bool,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::clipboard::clip_cut(s, &clipboard, &pick, &text, on_clipboard)
    })
}

/// `{text}`: the review for pasting the copied rows here, or `null` when a
/// paste of `text` isn't theirs (nothing copied, or something else copied
/// since: the page pastes `text` as cells). `text` `null` (Paste Rows):
/// whatever the system clipboard holds.
#[tauri::command]
pub fn clip_preview(
    state: State<'_, AppState>,
    clipboard: State<'_, ClipboardState>,
    doc: Option<DocId>,
    text: Option<String>,
) -> Result<Option<Preview>, String> {
    method_session::clipboard::clip_preview(&state, &clipboard, doc, text.as_deref())
}

/// `{token, target, resolutions}`: pastes the reviewed copy, as one undo
/// step. Refused when another copy has replaced it since the review.
#[tauri::command]
pub fn clip_paste<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    clipboard: State<'_, ClipboardState>,
    doc: Option<DocId>,
    token: u64,
    target: Target,
    resolutions: Vec<Resolution>,
) -> Result<View, String> {
    let mut s = state.lock_doc(doc)?;
    let first =
        method_session::clipboard::clip_paste(&mut s, &clipboard, token, &target, &resolutions)?;
    Ok(finish(&window, &mut s, first))
}

/// Forgets the copied rows (cells were copied instead).
#[tauri::command]
pub fn clip_forget(clipboard: State<'_, ClipboardState>) {
    method_session::clipboard::clip_forget(&clipboard);
}
