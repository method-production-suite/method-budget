//! "Explain This Number…" (roadmap 3.5; decisions.md, entry 43): the
//! command the tools panel's Explain tab calls. What it does is in
//! method-session's `explain` module (re-exported here).

use tauri::State;

use crate::app::{AppState, DocId};

pub use method_session::explain::*;

// ---------------------------------------------------------------------------
// Command (registered in app.rs, build.rs and both capabilities: the Explain
// tab can be in the tools window)

/// How total `target` of budget `doc` is worked out. Changes nothing.
#[tauri::command]
pub fn explain_number(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    target: TargetDto,
) -> Result<ExplanationDto, String> {
    method_session::explain::explain_number(&mut *state.lock_doc(doc)?, &target)
}
