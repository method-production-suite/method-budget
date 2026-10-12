//! What the tools panel's setup lists need beyond the tables themselves:
//! how much each item is used (roadmap 2.9), their hand-made order (2.12)
//! and the short codes of fringes and groups (2.11). What they do is in
//! method-session's `setup_lists` module (re-exported here); these are the
//! Tauri commands.

use std::collections::BTreeMap;

use tauri::{Runtime, State, WebviewWindow};

use crate::app::{apply, AppState, DocId};
use crate::view::View;

pub use method_session::setup_lists::*;

/// `{}`: how much every setup item of budget `doc` is used.
#[tauri::command]
pub fn setup_usage(
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> Result<BTreeMap<&'static str, BTreeMap<String, UsageDto>>, String> {
    Ok(method_session::setup_lists::setup_usage(
        &mut *state.lock_doc(doc)?,
    ))
}

/// `{kind, names}`: the table's order, first to last (names it doesn't
/// have are skipped; the rest follow in name order).
#[tauri::command]
pub fn set_setup_order<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    kind: String,
    names: Vec<String>,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::setup_lists::set_setup_order(s, &kind, &names)
    })
}

/// `{kind, name, by}`.
#[tauri::command]
pub fn move_setup_item<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    kind: String,
    name: String,
    by: i32,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::setup_lists::move_setup_item(s, &kind, &name, by)
    })
}

/// `{kind, name, code}`: a fringe's or group's short code; blank clears it.
#[tauri::command]
pub fn set_short_code<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    kind: String,
    name: String,
    code: String,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::setup_lists::set_short_code(s, &kind, &name, &code)
    })
}

#[cfg(test)]
mod tests {
    /// The new commands are registered, listed and allowed in both windows.
    #[test]
    fn setup_list_commands_are_wired() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN: &str = include_str!("../capabilities/main.json");
        const TOOLS: &str = include_str!("../capabilities/tools.json");
        for (module, c) in [
            ("setup_lists", "setup_usage"),
            ("setup_lists", "set_setup_order"),
            ("setup_lists", "move_setup_item"),
            ("setup_lists", "set_short_code"),
            ("notes", "add_note"),
            ("notes", "update_note"),
            ("notes", "delete_note"),
            ("notes", "set_note_done"),
        ] {
            assert!(
                APP_RS.contains(&format!("crate::{module}::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN.contains(&allow), "{c} not allowed for the main window");
            assert!(
                TOOLS.contains(&allow),
                "{c} not allowed for the tools window"
            );
        }
    }
}
