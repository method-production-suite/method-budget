//! Renumbering accounts when the account-number pattern changes (roadmap
//! 2.20; decisions.md, entry 39): the Settings tab's "Account numbers" form.
//! What it does is in method-session's `renumber` module (re-exported here);
//! these are the Tauri commands. Changing the pattern without renumbering
//! stays `set_account_coding`.

use tauri::{Runtime, State, WebviewWindow};

use crate::app::{AppState, DocId};
use crate::view::View;

pub use method_session::renumber::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs, and the main window's and the
// Budget Setup window's capabilities: the Settings tab is in Budget Setup)

/// What renumbering budget `doc` to the pattern would do. Changes nothing.
/// Each width is its own argument, as for `set_account_coding`.
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "snake_case")]
pub fn renumber_preview(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    category_width: u8,
    account_width: u8,
    sub_width: u8,
    set_width: u8,
    separator: Option<String>,
) -> Result<PlanDto, String> {
    method_session::renumber::renumber_preview(
        &*state.lock_doc(doc)?,
        [category_width, account_width, sub_width, set_width],
        separator.as_deref(),
    )
}

/// Sets the pattern and renumbers every category and account to it, with
/// everything that names them, as one undo step.
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "snake_case")]
pub fn renumber_accounts<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    category_width: u8,
    account_width: u8,
    sub_width: u8,
    set_width: u8,
    separator: Option<String>,
) -> Result<View, String> {
    let widths = [category_width, account_width, sub_width, set_width];
    crate::app::apply(&window, &state, doc, |s| {
        method_session::renumber::renumber_accounts(s, widths, separator.as_deref())
    })
}
