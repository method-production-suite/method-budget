//! Review files (roadmap 3.3; decisions.md, entry 81): File > Open for
//! Review… opens a budget read-only with its comments kept in
//! `<budget>.review.json` beside it, written after every change; File >
//! Open Review Comments… opens a reviewer's file on the author's budget to
//! merge the comments as notes or dismiss them, writing what was done back
//! into that file.
//!
//! What a review is and does lives in method-session's `review` module
//! (re-exported here); these are the dialogs, reading and writing the file,
//! and the Tauri commands. The page never sees a path.

use std::path::{Path, PathBuf};

use tauri::{Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, AppState, DocId};
use crate::safety::{self, Access};
use crate::safety_app::Safety;
use crate::session::Session;
use crate::sheet_import::read_capped;
use crate::view::View;

pub use method_session::review::*;

/// Where a budget's review file goes: beside it, named after it.
fn beside(budget: &Path) -> Option<PathBuf> {
    let name = budget.file_name()?.to_string_lossy().into_owned();
    Some(budget.with_file_name(file_name_for(&name)))
}

/// Writes the review file open on `s` to its place, when it has one and
/// has changes. A write that fails is told once with the next view, and
/// the review stays unsaved (Save Review File As… picks another place).
fn write_back(s: &mut Session) {
    let Some(path) = s
        .review()
        .filter(|r| r.unsaved)
        .and_then(|r| r.path.clone())
    else {
        return;
    };
    let Ok(text) = method_session::review::text(s) else {
        return;
    };
    match safety::write_atomically(&path, text.as_bytes(), Access::Document) {
        Ok(()) => method_session::review::mark_saved(s, None, None),
        Err(e) => s.set_notice(format!(
            "The review file couldn't be written ({e}). Use Save Review File As… in the Comments tab to keep the comments somewhere else."
        )),
    }
}

/// Runs `f` on budget `doc`, writes the review file back and replies with
/// the view.
fn change<R: Runtime, T>(
    window: &WebviewWindow<R>,
    state: &AppState,
    doc: Option<DocId>,
    f: impl FnOnce(&mut Session) -> Result<T, String>,
) -> Result<View, String> {
    let mut s = state.lock_doc(doc)?;
    f(&mut s)?;
    write_back(&mut s);
    Ok(finish(window, &mut s, None))
}

/// Starts reviewing budget `s`: its review file beside it is read when
/// there is one, and written there from now on.
fn start(s: &mut Session) -> Result<(), String> {
    let path = s.path().and_then(beside).ok_or(
        "Save the budget first: its review file goes beside it. A copy saved for sharing has its place already.",
    )?;
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let existing = if path.is_file() {
        Some(method_session::review::read(&file, &read_capped(&path)?)?)
    } else {
        None
    };
    start_reviewing(s, existing, file, Some(path))
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and the capabilities: the main
// window has them all, the tools window the Comments tab's list and edits)

/// File > Open for Review…: an Open dialog for a budget, opened in a tab
/// (or brought to the front) and set for review. `None` when cancelled.
#[tauri::command]
pub async fn review_open_budget<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
) -> Result<Option<View>, String> {
    let start_dir = state
        .lock()
        .path()
        .and_then(|p| p.parent())
        .map(PathBuf::from);
    let Some(path) = crate::app::pick_budget(&window, "Open for review", start_dir)? else {
        return Ok(None);
    };
    let safety = window.state::<Safety>();
    let id = crate::safety_app::open_path(&state, &safety, &path)?;
    crate::safety_app::refresh_menu(window.app_handle());
    change(&window, &state, Some(id), start).map(Some)
}

/// Sets budget `doc` (open in a tab, saved) for review.
#[tauri::command]
pub fn review_start<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> Result<View, String> {
    change(&window, &state, doc, start)
}

/// File > Open Review Comments…: an Open dialog for a review file, opened
/// on budget `doc` to merge. `None` when cancelled.
#[tauri::command]
pub async fn review_open<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> Result<Option<View>, String> {
    let (dir, suggested) = {
        let s = state.lock_doc(doc)?;
        (
            s.path().and_then(|p| p.parent()).map(PathBuf::from),
            default_file(&s),
        )
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Open review comments")
        .add_filter("Review file", &["json"])
        .set_file_name(suggested)
        .set_parent(&window);
    if let Some(dir) = dir {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let review = method_session::review::read(&file, &read_capped(&path)?)?;
    change(&window, &state, doc, |s| {
        open_for_author(s, review, file, Some(path))
    })
    .map(Some)
}

/// Closes the review file on budget `doc` (written first when it can be).
#[tauri::command]
pub fn review_close<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> Result<View, String> {
    change(&window, &state, doc, |s| {
        close(s);
        Ok(())
    })
}

/// The Comments tab's list.
#[tauri::command]
pub fn review_list(state: State<'_, AppState>, doc: Option<DocId>) -> Result<ReviewList, String> {
    list(&*state.lock_doc(doc)?)
}

/// `{on, text, created, author?}`: a comment on `on`.
#[tauri::command]
pub fn review_add<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    on: Anchor,
    text: String,
    created: String,
    author: Option<String>,
) -> Result<View, String> {
    change(&window, &state, doc, |s| {
        add(s, on, &text, &created, author.as_deref())
    })
}

/// `{id, text, edited}`: blank text deletes the comment.
#[tauri::command]
pub fn review_update<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    id: CommentId,
    text: String,
    edited: String,
) -> Result<View, String> {
    change(&window, &state, doc, |s| update(s, id, &text, &edited))
}

/// `{id, resolved}`.
#[tauri::command]
pub fn review_resolve<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    id: CommentId,
    resolved: bool,
) -> Result<View, String> {
    change(&window, &state, doc, |s| resolve(s, id, resolved))
}

/// `{id}`.
#[tauri::command]
pub fn review_delete<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    id: CommentId,
) -> Result<View, String> {
    change(&window, &state, doc, |s| delete(s, id))
}

/// `{id}`: a dismissed comment open again.
#[tauri::command]
pub fn review_restore<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    id: CommentId,
) -> Result<View, String> {
    change(&window, &state, doc, |s| restore(s, id))
}

/// `{picks, at, by?}`: merges and dismisses, the merges one undo step.
#[tauri::command]
pub fn review_settle<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    picks: Vec<Pick>,
    at: String,
    by: Option<String>,
) -> Result<SettledView, String> {
    let mut message = String::new();
    let view = change(&window, &state, doc, |s| {
        message = settle(s, &picks, &at, by.as_deref())?.message;
        Ok(())
    })?;
    Ok(SettledView { view, message })
}

/// Writes the review file to its place, or (with `save_as`, or with no
/// place yet) where a Save dialog says. `None` when cancelled.
#[tauri::command]
pub async fn review_save<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    save_as: Option<bool>,
) -> Result<Option<View>, String> {
    let (current, file, dir) = {
        let s = state.lock_doc(doc)?;
        let r = s.review().ok_or(NO_REVIEW)?;
        (
            r.path.clone(),
            r.file.clone(),
            s.path().and_then(|p| p.parent()).map(PathBuf::from),
        )
    };
    let path = match current {
        Some(p) if !save_as.unwrap_or(false) => p,
        _ => {
            let mut dialog = window
                .dialog()
                .file()
                .set_title("Save review file")
                .add_filter("Review file", &["json"])
                .set_file_name(file)
                .set_parent(&window);
            if let Some(dir) = dir {
                dialog = dialog.set_directory(dir);
            }
            let Some(picked) = dialog.blocking_save_file() else {
                return Ok(None);
            };
            picked.into_path().map_err(|e| e.to_string())?
        }
    };
    let mut s = state.lock_doc(doc)?;
    let text = method_session::review::text(&s)?;
    safety::write_atomically(&path, text.as_bytes(), Access::Document)
        .map_err(|e| format!("The review file couldn't be written: {e}"))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    method_session::review::mark_saved(&mut s, Some(path), name);
    Ok(Some(finish(&window, &mut s, None)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_review_file_sits_beside_the_budget() {
        assert_eq!(
            beside(Path::new("/films/Field Station.mthd")),
            Some(PathBuf::from("/films/Field Station.review.json"))
        );
    }

    #[test]
    fn a_reviewer_s_comments_are_written_beside_the_budget_after_each_change() {
        let dir = std::env::temp_dir().join(format!("method-review-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let budget = dir.join("Field Station.mthd");
        let mut s = Session::new();
        std::fs::write(&budget, method_format::to_string(s.budget())).unwrap();
        s.mark_saved(
            &budget,
            method_session::safety::DiskStamp {
                len: 0,
                modified: None,
                hash: 0,
            },
        );
        start(&mut s).unwrap();
        let code = s.budget().categories().next().unwrap().code.clone();
        add(&mut s, Anchor::Category(code), "Check", "2027-01-05", None).unwrap();
        write_back(&mut s);
        let file = dir.join("Field Station.review.json");
        let back = method_session::review::read("x", &std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(back.comments.len(), 1);
        assert!(!s.review().unwrap().unsaved);
        // Opening for review again reads it back.
        close(&mut s);
        start(&mut s).unwrap();
        assert_eq!(list(&s).unwrap().entries.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
