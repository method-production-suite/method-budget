//! Named versions saved inside a budget: the Versions dialog's commands
//! (decisions.md, entries 17 and 27). What they do is in method-session's
//! `versions` module (re-exported here); these are the Tauri commands.

use method_core::VersionId;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{apply, AppState, DocId};
use crate::view::View;

pub use method_session::versions::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// main window only)

/// Saves the budget as a new version, `created` being the page's local time.
#[tauri::command]
pub fn save_version<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    note: Option<String>,
    created: String,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::versions::save_version(s, &name, note.as_deref(), &created)
    })
}

/// Restores version `id` (one undo step; the replaced content is kept as an
/// automatic version too).
#[tauri::command]
pub fn restore_version<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    id: VersionId,
    created: String,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::versions::restore_version(s, id, &created)
    })
}

#[tauri::command]
pub fn rename_version<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    id: VersionId,
    name: String,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::versions::rename_version(s, id, &name)
    })
}

#[tauri::command]
pub fn delete_version<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    id: VersionId,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::versions::delete_version(s, id)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_commands_are_wired_for_the_main_window_only() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
        const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");
        for c in [
            "save_version",
            "restore_version",
            "rename_version",
            "delete_version",
        ] {
            assert!(
                APP_RS.contains(&format!("crate::versions::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(
                MAIN_CAPS.contains(&allow),
                "{c} not allowed for the main window"
            );
            assert!(
                !TOOLS_CAPS.contains(&allow),
                "{c} allowed for the tools window"
            );
        }
    }
}
