//! Find and replace: a typed search over one budget, and replacing what was
//! found in the places the user checked. What they do is in method-session's
//! `find` module (re-exported here); these are the Tauri commands.

use tauri::{Runtime, State, WebviewWindow};

use crate::app::{finish, AppState, DocId};

pub use method_session::find::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json and
// tools.json: the Find tab is part of the tools panel)

/// The places in budget `doc` that match `query`.
#[tauri::command]
pub fn find(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    query: FindQuery,
) -> Result<Found, String> {
    crate::i18n::engine_err(method_session::find::find(&*state.lock_doc(doc)?, &query))
}

/// Replaces in the checked results (`targets`) as one undo step; `with`
/// `null` removes. See [`replace_in`].
#[tauri::command]
pub fn replace<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    query: FindQuery,
    targets: Vec<Target>,
    with: Option<String>,
) -> Result<Replaced, String> {
    let mut s = state.lock_doc(doc)?;
    let updated = crate::i18n::engine_err(method_session::find::replace(
        &mut s,
        &query,
        &targets,
        with.as_deref(),
    ))?;
    Ok(Replaced {
        view: finish(&window, &mut s, None),
        updated,
    })
}

#[cfg(test)]
mod tests {
    const APP_RS: &str = include_str!("app.rs");
    const BUILD_RS: &str = include_str!("../build.rs");
    const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
    const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");

    #[test]
    fn find_commands_are_wired_for_both_windows() {
        for c in ["find", "replace"] {
            assert!(
                APP_RS.contains(&format!("crate::find::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{c}\"");
            assert!(
                MAIN_CAPS.contains(&allow),
                "{c} not allowed for the main window"
            );
            assert!(
                TOOLS_CAPS.contains(&allow),
                "{c} not allowed for the tools window"
            );
        }
    }
}
