//! The History list (roadmap 2.23): every undo step of a budget, named by the
//! command that made it ("Edit rate on 21-01 line 3", "Paste 12 rows",
//! "Rename fringe FICA to Payroll") with its time; a jump to any step (undo
//! or redo to it); and saving any step as a named version without changing
//! the budget as it is now.
//!
//! Naming steps and saving a step as a version are method-session's
//! `history` module (re-exported here); these are the three commands the
//! History tab calls (ui/js/history.js). The tools window may list the
//! steps; jumping and saving a version run in the main window, like Undo
//! (decisions.md, entry 24).

use tauri::{Runtime, State, WebviewWindow};

use crate::app::{apply, finish, AppState, DocId};
use crate::session::History;
use crate::view::View;

pub use method_session::history::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may list the steps only)

/// Every undo and redo step of budget `doc` (`None`: the one in front),
/// oldest first, with how many are applied.
#[tauri::command]
pub fn history_list(state: State<'_, AppState>, doc: Option<DocId>) -> Result<History, String> {
    Ok(state.lock_doc(doc)?.history())
}

/// Undoes or redoes budget `doc`'s steps until `to` of them are applied
/// (0: the start of the list): one change, however many steps it passes.
#[tauri::command]
pub fn history_jump<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    to: usize,
) -> Result<View, String> {
    let mut s = state.lock_doc(doc)?;
    s.jump_to(to)?;
    Ok(finish(&window, &mut s, None))
}

/// Saves the budget as it was with `step` steps applied as a new version
/// named `name` (one undo step, like any version save), leaving its content
/// as it is now. `created` is the page's local time.
#[tauri::command]
pub fn history_save_version<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    step: usize,
    name: String,
    note: Option<String>,
    created: String,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::history::history_save_version(s, step, &name, note.as_deref(), &created)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn history_commands_are_wired() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
        const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");
        for c in ["history_list", "history_jump", "history_save_version"] {
            assert!(APP_RS.contains(&format!("crate::history::{c},")), "{c}");
            assert!(BUILD_RS.contains(&format!("\"{c}\",")), "{c}");
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN_CAPS.contains(&allow), "{c} for the main window");
            // Jumping and saving run in the main window, like Undo.
            assert_eq!(TOOLS_CAPS.contains(&allow), c == "history_list", "{c}");
        }
    }
}
