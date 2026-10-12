//! Renaming a setup item with every reference to it rewritten, as one undo
//! step (roadmap 1.18, decision 7). What it does is in method-session's
//! `rename` module (re-exported here); this is the Tauri command the tools
//! panel calls from an item's editor, its right-click menu, and the Find tab.

use tauri::{Runtime, State, WebviewWindow};

use crate::app::{apply, AppState, DocId};

pub use method_session::rename::*;

/// Renames a setup item everywhere, as one undo step. See [`rename_item`].
#[tauri::command]
pub fn rename_setup_item<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    kind: String,
    from: String,
    to: String,
) -> Result<Renamed, String> {
    let mut out = (String::new(), 0);
    let view = apply(&window, &state, doc, |s| {
        out = method_session::rename::rename_setup_item(s, &kind, &from, &to)?;
        Ok(())
    })?;
    Ok(Renamed {
        view,
        name: out.0,
        places: out.1,
    })
}

#[cfg(test)]
mod tests {

    const APP_RS: &str = include_str!("app.rs");
    const BUILD_RS: &str = include_str!("../build.rs");
    const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
    const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");

    #[test]
    fn the_rename_command_is_wired_for_both_windows() {
        assert!(APP_RS.contains("crate::rename::rename_setup_item,"));
        assert!(BUILD_RS.contains("\"rename_setup_item\","));
        assert!(MAIN_CAPS.contains("\"allow-rename-setup-item\""));
        assert!(TOOLS_CAPS.contains("\"allow-rename-setup-item\""));
    }
}
