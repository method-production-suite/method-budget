//! The History list (roadmap 2.23): every undo step of a budget, named by the
//! command that made it ("Edit rate on 21-01 line 3", "Paste 12 rows",
//! "Rename fringe FICA to Payroll") with its time; a jump to any step (undo
//! or redo to it); and saving any step as a named version without changing
//! the budget as it is now.
//!
//! The steps themselves live in the session's undo and redo stacks
//! (src/session.rs): each keeps its label and time beside what it changed.
//! This module names steps (the helpers every command uses for its label)
//! and holds the three commands the History tab calls (ui/js/history.js).
//! The tools window may list the steps; jumping and saving a version run in
//! the main window, like Undo (decisions.md, entry 24).

use std::sync::Arc;

use method_core::Budget;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{finish, AppState, DocId};
use crate::session::History;
use crate::view::View;

pub use method_session::history::*;

// ---------------------------------------------------------------------------
// Saving a step as a version

/// Saves `past` (the budget as it was at some step) as a new version of `b`,
/// named and noted as the Versions dialog checks names and notes. Returns its
/// id. Only the version list of `b` changes.
pub fn save_step_as_version(
    b: &mut Budget,
    past: &Budget,
    name: &str,
    note: Option<&str>,
    created: &str,
) -> crate::edit::EditResult<method_core::VersionId> {
    let id = crate::versions::save(b, name, note, created)?;
    if let Some(v) = b.versions.iter_mut().find(|v| v.id == id) {
        v.budget = Arc::new(past.snapshot());
    }
    Ok(id)
}

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
    let mut s = state.lock_doc(doc)?;
    let checked = crate::versions::check_name(&name)?;
    let past = s.budget_at(step)?;
    let label = format!("Save version \u{201c}{checked}\u{201d} from the history");
    s.edit(label, |b| {
        save_step_as_version(b, &past, &checked, note.as_deref(), &created)
    })?;
    Ok(finish(&window, &mut s, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use method_core::{RowId, RowKind};

    const AT: &str = "2026-10-09T14:30:00-07:00";

    fn first_line(s: &Session) -> RowId {
        crate::edit::details(s.budget())
            .find(|d| d.kind == RowKind::Line)
            .unwrap()
            .id
    }

    #[test]
    fn a_step_saved_as_a_version_keeps_the_budget_as_it_is() {
        let mut s = Session::new();
        let id = first_line(&s);
        s.set_detail_field(id, "rate", "100").unwrap();
        let at_one = s.budget().clone();
        s.set_detail_field(id, "rate", "200").unwrap();
        let now = s.budget().clone();
        let past = s.budget_at(1).unwrap();
        s.edit("Save version", |b| {
            save_step_as_version(b, &past, " Rate at 100 ", Some("first try"), AT)
        })
        .unwrap();
        // The content is unchanged; the new version holds the step's content.
        let b = s.budget();
        assert_eq!(b.snapshot(), now.snapshot());
        let v = b.versions.last().unwrap();
        assert_eq!(v.name, "Rate at 100");
        assert_eq!(v.note.as_deref(), Some("first try"));
        assert_eq!(*v.budget, at_one.snapshot());
        // One undo step, which takes the version back off.
        assert_eq!(s.history().steps.len(), 3);
        s.undo();
        assert!(s.budget().versions.is_empty());
        // Names and times are checked as the Versions dialog checks them.
        let past = s.budget_at(0).unwrap();
        assert!(s
            .edit("x", |b| save_step_as_version(b, &past, "  ", None, AT))
            .is_err());
        assert!(s
            .edit("x", |b| save_step_as_version(b, &past, "Ok", None, "soon"))
            .is_err());
    }

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
