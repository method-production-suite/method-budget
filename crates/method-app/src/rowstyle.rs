//! Row semantics that never touch the math: text style (bold, italic, indent,
//! color) and group colors. What they do is in method-session's `rowstyle`
//! module (re-exported here); these are the Tauri commands.

use tauri::ipc::{InvokeBody, Request};
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{apply, AppState, DocId};
use crate::edit::GroupInput;
use crate::view::View;

pub use method_session::rowstyle::*;

// ---------------------------------------------------------------------------
// Commands

/// `{rowIds, bold?, italic?, indent?, color?: string | null}`.
#[tauri::command]
pub fn set_row_style<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    request: Request<'_>,
) -> Result<View, String> {
    let InvokeBody::Json(args) = request.body() else {
        return Err("bad row style: expected JSON arguments".into());
    };
    let input = RowStyleInput::from_json(args)?;
    apply(&window, &state, doc, |s| {
        method_session::rowstyle::set_row_style(s, &input)
    })
}

/// `{name, color: string | null}`.
#[tauri::command]
pub fn set_group_color<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    color: Option<String>,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::rowstyle::set_group_color(s, &name, color.as_deref())
    })
}

/// `{name, group: {description, in_budget_total}, color: string | null}`:
/// everything about an existing group in one undo step.
#[tauri::command]
pub fn update_group<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    group: GroupInput,
    color: Option<String>,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::rowstyle::update_group(s, &name, &group, color.as_deref())
    })
}

#[cfg(test)]
mod tests {

    /// Every command here is registered, listed for access control and
    /// allowed for the window (a command missing from any of the three
    /// can't be called).
    #[test]
    fn row_style_commands_are_wired() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const CAPS: &str = include_str!("../capabilities/main.json");
        for c in ["set_row_style", "set_group_color", "update_group"] {
            assert!(
                APP_RS.contains(&format!("crate::rowstyle::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(CAPS.contains(&allow), "{c} not allowed in capabilities");
        }
    }
}
