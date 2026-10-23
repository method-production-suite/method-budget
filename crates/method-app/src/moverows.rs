//! Moving rows up or down one place: detail lines within their account,
//! accounts within their category, and categories and production totals on
//! the topsheet (roadmap 1.10). What it does is in method-session's
//! `moverows` module (re-exported here); these are the Tauri commands.

use method_core::RowId;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{finish, AppState, DocId};

pub use method_session::moverows::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json)

/// Moves detail rows one place up (`by` -1) or down (1). See [`shift_rows`].
#[tauri::command]
pub fn move_rows<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
    by: i32,
) -> Result<MovedView, String> {
    let mut s = state.lock_doc(doc)?;
    let m = crate::i18n::engine_err(method_session::moverows::move_rows(&mut s, &row_ids, by))?;
    Ok(MovedView::new(finish(&window, &mut s, None), m))
}

/// Moves accounts (by code, all in one category) one place up or down
/// among the category's accounts. See [`shift_accounts`].
#[tauri::command]
pub fn move_accounts<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    codes: Vec<String>,
    by: i32,
) -> Result<MovedView, String> {
    let mut s = state.lock_doc(doc)?;
    let m = crate::i18n::engine_err(method_session::moverows::move_accounts(&mut s, &codes, by))?;
    Ok(MovedView::new(finish(&window, &mut s, None), m))
}

/// Moves topsheet rows (categories and production totals, by place) one
/// place up or down. See [`shift_topsheet_rows`].
#[tauri::command]
pub fn move_topsheet_rows<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    indexes: Vec<usize>,
    by: i32,
) -> Result<MovedView, String> {
    let mut s = state.lock_doc(doc)?;
    let m = crate::i18n::engine_err(method_session::moverows::move_topsheet_rows(
        &mut s, &indexes, by,
    ))?;
    Ok(MovedView::new(finish(&window, &mut s, None), m))
}

#[cfg(test)]
mod tests {
    const APP_RS: &str = include_str!("app.rs");
    const BUILD_RS: &str = include_str!("../build.rs");
    const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
    const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");

    #[test]
    fn move_commands_are_wired_for_the_main_window_only() {
        for c in ["move_rows", "move_accounts", "move_topsheet_rows"] {
            assert!(APP_RS.contains(&format!("crate::moverows::{c},")), "{c}");
            assert!(BUILD_RS.contains(&format!("\"{c}\",")), "{c}");
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN_CAPS.contains(&allow), "{c}");
            assert!(!TOOLS_CAPS.contains(&allow), "{c}");
        }
    }
}
