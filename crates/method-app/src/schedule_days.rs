//! File > Import Schedule Days…: a schedule's day counts drive the budget's
//! globals (roadmap 3.8; decisions.md, entry 76). The file is the one
//! Method Schedule writes for a budget (`method-schedule-days`).
//!
//! - **The file.** Rust shows the Open dialog and reads the file; the page
//!   names it by a token afterwards, as the other imports do.
//! - **The review** lists every figure the file counts (shoot days in all,
//!   by unit, set, location and week, each element's days) with the global
//!   it would set, that global's value now and the new one. The total goes
//!   to `SHOOT_DAYS`; any other figure only when picked, under a name the
//!   person can change. Nothing changes until Set Globals.
//! - **Applying** is one undo step. Each global set records where its
//!   figure came from (its setup source), so the next import of the same
//!   schedule picks the same globals and says which ones changed.
//!
//! The review and the apply are in method-session's `schedule_days` module
//! (re-exported here); these are the Open dialog, reading the file and the
//! Tauri commands.

use std::path::PathBuf;

use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, AppState, DocId};
use crate::sheet_import::read_capped;

pub use method_session::schedule_days::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// Shows an Open dialog for a schedule days file, reads it and returns the
/// review against budget `doc`. `None` when cancelled.
#[tauri::command]
pub async fn schedule_days_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ScheduleSources>,
    doc: Option<DocId>,
) -> Result<Option<Review>, String> {
    let start = {
        let s = state.lock_doc(doc)?;
        s.path().and_then(|p| p.parent()).map(PathBuf::from)
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title(crate::i18n::tr("dialog.import_schedule_days"))
        .add_filter(crate::i18n::tr("dialog.filter_schedule_days"), &["json"])
        .set_parent(&window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked_file) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked_file.into_path().map_err(|e| e.to_string())?;
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bytes = read_capped(&path)?;
    let b = state.lock_doc(doc)?.budget().clone();
    crate::i18n::engine_dto(method_session::schedule_days::schedule_days_pick(
        &b, &sources, file, &bytes,
    ))
    .map(Some)
}

/// The review of the picked file against budget `doc` with `picks` (`None`:
/// the review's own). Changes nothing.
#[tauri::command]
pub fn schedule_days_review(
    state: State<'_, AppState>,
    sources: State<'_, ScheduleSources>,
    doc: Option<DocId>,
    token: u64,
    picks: Option<Vec<Pick>>,
) -> Result<Review, String> {
    let b = state.lock_doc(doc)?.budget().clone();
    crate::i18n::engine_dto(method_session::schedule_days::schedule_days_review(
        &b,
        &sources,
        token,
        picks.as_deref(),
    ))
}

/// Sets the picked globals in budget `doc` as one undo step.
#[tauri::command]
pub fn schedule_days_apply<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ScheduleSources>,
    doc: Option<DocId>,
    token: u64,
    picks: Option<Vec<Pick>>,
) -> Result<Imported, String> {
    let (id, message) =
        crate::i18n::engine_err(method_session::schedule_days::schedule_days_apply(
            &state,
            &sources,
            doc,
            token,
            picks.as_deref(),
        ))?;
    Ok(Imported {
        view: finish(&window, &mut state.lock_doc(Some(id))?, None),
        message: crate::i18n::engine(&message),
    })
}
