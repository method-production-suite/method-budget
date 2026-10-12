//! Display settings' commands (roadmap 2.10 and 2.18). What they do is in
//! method-session's `display` module, re-exported here.

use tauri::{Runtime, State, WebviewWindow};

use crate::app::{edit_budget, AppState, DocId};
use crate::view::View;

pub use method_session::display::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/*.json)

/// `{name, info: {description, unit, decimals, show_by_name, folder}}`.
#[tauri::command]
pub fn set_global_info<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    info: GlobalInfoInput,
) -> Result<View, String> {
    edit_budget(
        &window,
        &state,
        doc,
        format!("Edit the details of global {}", name.trim()),
        |b| apply_global_info(b, &name, &info),
    )
}

/// `{prefs: {rate_cutoff, rate_decimals_below, rate_decimals_above,
/// rate_global_decimals, summary_rows, net_total}}`.
#[tauri::command]
pub fn set_display_prefs<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    prefs: DisplayPrefsInput,
) -> Result<View, String> {
    edit_budget(&window, &state, doc, "Change numbers on screen", |b| {
        apply_display_prefs(b, &prefs)
    })
}
