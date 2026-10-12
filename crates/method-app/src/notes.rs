//! Dated notes on lines, accounts and categories (roadmap 2.8), and the
//! budget-wide list the Notes tab shows as a review checklist. What they do
//! is in method-session's `notes` module (re-exported here); these are the
//! Tauri commands.

use method_core::NoteId;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{apply, AppState, DocId};
use crate::view::View;

pub use method_session::notes::*;

// ---------------------------------------------------------------------------
// Commands

/// `{owner, text, created, author?}`: adds a note to the owner.
#[tauri::command]
pub fn add_note<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    owner: NoteOwner,
    text: String,
    created: String,
    author: Option<String>,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::notes::add_note(s, &owner, &text, &created, author.as_deref())
    })
}

/// `{owner, id, text, edited}`: changes a note's text (blank removes it).
#[tauri::command]
pub fn update_note<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    owner: NoteOwner,
    id: NoteId,
    text: String,
    edited: String,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::notes::update_note(s, &owner, id, &text, &edited)
    })
}

/// `{owner, id}`.
#[tauri::command]
pub fn delete_note<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    owner: NoteOwner,
    id: NoteId,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::notes::delete_note(s, &owner, id)
    })
}

/// `{owner, id, done}`.
#[tauri::command]
pub fn set_note_done<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    owner: NoteOwner,
    id: NoteId,
    done: bool,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::notes::set_note_done(s, &owner, id, done)
    })
}
