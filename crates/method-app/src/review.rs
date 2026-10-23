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
use crate::safety::{self, Access, DiskCheck, DiskStamp, DiskStampExt};
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

/// What the person is told when the review file changed on disk since it
/// was read: it is left alone.
pub const CHANGED_ON_DISK: &str = "The review file changed on disk since it was read (someone else may have written it), so it wasn't written over. Your comments are kept here: in the Comments tab, Save Review File As… keeps them beside it, or Replace the File writes yours over it.";

/// Writes the review file open on `s` to its place, unless the file there
/// changed since it was read or written (its [`DiskStamp`], as the budget's
/// own saves check): then nothing is written, the review is marked in
/// conflict and the person is told. With `force` it is written anyway.
/// `Ok(false)` when there was nothing to write (no place, no changes).
fn write_to_place(s: &mut Session, force: bool) -> Result<bool, String> {
    let Some(st) = s.review() else {
        return Ok(false);
    };
    let Some(path) = st.path.clone() else {
        return Ok(false);
    };
    if !force && !method_session::review::needs_write(s) {
        return Ok(false);
    }
    if !force {
        let changed = match &st.stamp {
            _ if st.conflict => true,
            Some(stamp) => matches!(stamp.check(&path), DiskCheck::Changed),
            // Nothing was read: a file that appeared meanwhile is someone else's.
            None => path.exists(),
        };
        if changed {
            if let Some(st) = s.review_mut() {
                st.conflict = true;
            }
            return Err(CHANGED_ON_DISK.into());
        }
    }
    let text = crate::i18n::engine_err(method_session::review::text(s))?;
    safety::write_atomically(&path, text.as_bytes(), Access::Document)
        .map_err(|e| format!("The review file couldn't be written ({e}). Use Save Review File As… in the Comments tab to keep the comments somewhere else."))?;
    let stamp = DiskStamp::of(&path, text.as_bytes());
    method_session::review::mark_saved(s, None, None);
    if let Some(st) = s.review_mut() {
        st.stamp = Some(stamp);
        st.conflict = false;
    }
    Ok(true)
}

/// Writes the review file back after a change, when it needs it. A write
/// refused or failed is told once with the next view, and the comments stay
/// here, unsaved. A conflict already told isn't told again.
pub(crate) fn write_back(s: &mut Session) {
    let told = s.review().is_some_and(|r| r.conflict);
    if let Err(e) = write_to_place(s, false) {
        if !(told && e == CHANGED_ON_DISK) {
            s.set_notice(e);
        }
    }
}

/// Closes the review file, writing it first: a write refused or failed
/// keeps it open, with the reason, so no comment is lost.
fn close_writing(s: &mut Session) -> Result<(), String> {
    write_to_place(s, false).map_err(|e| format!("The review file stays open: {e}"))?;
    if s.review().is_some_and(|r| r.path.is_none()) && method_session::review::needs_write(s) {
        return Err("The review file stays open: it has comments not saved anywhere yet. Use Save Review File As… first.".into());
    }
    close(s);
    Ok(())
}

/// Writes the review file to `path` (its own place, or another), written
/// over even when it changed on disk only with `force` or when it is
/// another file the person picked in the Save dialog.
fn save_at(s: &mut Session, path: PathBuf, force: bool) -> Result<(), String> {
    let same = s.review().and_then(|r| r.path.as_ref()) == Some(&path);
    if !same {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        if let Some(st) = s.review_mut() {
            st.path = Some(path);
            st.stamp = None;
            st.conflict = false;
            if let Some(n) = name {
                st.file = n;
            }
        }
        return write_to_place(s, true).map(|_| ());
    }
    write_to_place(s, force).map(|_| ())
}

/// The stamp of the file at `path` just read as `bytes`.
fn read_stamp(path: &Path, bytes: &[u8]) -> DiskStamp {
    DiskStamp::of(path, bytes)
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
    let (existing, stamp) = if path.is_file() {
        let bytes = read_capped(&path)?;
        let stamp = read_stamp(&path, &bytes);
        (
            Some(crate::i18n::engine_err(method_session::review::read(
                &file, &bytes,
            ))?),
            Some(stamp),
        )
    } else {
        (None, None)
    };
    start_reviewing(s, existing, file, Some(path))?;
    if let Some(st) = s.review_mut() {
        st.stamp = stamp;
    }
    Ok(())
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
    let Some(path) = crate::app::pick_budget(
        &window,
        &crate::i18n::tr("dialog.open_for_review"),
        start_dir,
    )?
    else {
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
        .set_title(crate::i18n::tr("dialog.open_review_comments"))
        .add_filter(crate::i18n::tr("dialog.filter_review_file"), &["json"])
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
    let bytes = read_capped(&path)?;
    let review = crate::i18n::engine_err(method_session::review::read(&file, &bytes))?;
    let stamp = read_stamp(&path, &bytes);
    change(&window, &state, doc, |s| {
        open_for_author(s, review, file, Some(path))?;
        if let Some(st) = s.review_mut() {
            st.stamp = Some(stamp);
        }
        Ok(())
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
    let mut s = state.lock_doc(doc)?;
    close_writing(&mut s)?;
    Ok(finish(&window, &mut s, None))
}

/// The Comments tab's list.
#[tauri::command]
pub fn review_list(state: State<'_, AppState>, doc: Option<DocId>) -> Result<ReviewList, String> {
    crate::i18n::engine_dto(list(&*state.lock_doc(doc)?))
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
        message = crate::i18n::engine(&settle(s, &picks, &at, by.as_deref())?.message);

        Ok(())
    })?;
    Ok(SettledView { view, message })
}

/// Writes the review file to its place, or (with `save_as`, or with no
/// place yet) where a Save dialog says. Its own place is written over when
/// it changed on disk only with `force` (Replace the File); a file picked
/// in the Save dialog always is (the dialog asked). `None` when cancelled.
#[tauri::command]
pub async fn review_save<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    save_as: Option<bool>,
    force: Option<bool>,
) -> Result<Option<View>, String> {
    let (current, file, dir) = {
        let s = state.lock_doc(doc)?;
        let r = s.review().ok_or_else(no_review)?;
        (
            r.path.clone(),
            r.file.clone(),
            s.path().and_then(|p| p.parent()).map(PathBuf::from),
        )
    };
    let (path, force) = match current {
        Some(p) if !save_as.unwrap_or(false) => (p, force.unwrap_or(false)),
        _ => {
            let mut dialog = window
                .dialog()
                .file()
                .set_title(crate::i18n::tr("dialog.save_review_file"))
                .add_filter(crate::i18n::tr("dialog.filter_review_file"), &["json"])
                .set_file_name(file)
                .set_parent(&window);
            if let Some(dir) = dir {
                dialog = dialog.set_directory(dir);
            }
            let Some(picked) = dialog.blocking_save_file() else {
                return Ok(None);
            };
            (picked.into_path().map_err(|e| e.to_string())?, true)
        }
    };
    let mut s = state.lock_doc(doc)?;
    save_at(&mut s, path, force)?;
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

    /// A saved starter budget in a fresh folder named `name`, open for
    /// review: the session, the folder and the review file's path.
    fn reviewing_in(name: &str) -> (Session, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("method-review-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
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
        (s, dir.clone(), dir.join("Field Station.review.json"))
    }

    fn comment(s: &mut Session, text: &str) {
        let code = s.budget().categories().next().unwrap().code.clone();
        add(s, Anchor::Category(code), text, "2027-01-05", None).unwrap();
    }

    fn comments_in(file: &Path) -> Vec<String> {
        method_session::review::read("x", &std::fs::read(file).unwrap())
            .unwrap()
            .comments
            .into_iter()
            .map(|c| c.text)
            .collect()
    }

    #[test]
    fn a_reviewer_s_comments_are_written_beside_the_budget_after_each_change() {
        let (mut s, dir, file) = reviewing_in("written");
        comment(&mut s, "Check");
        write_back(&mut s);
        assert_eq!(comments_in(&file), ["Check"]);
        assert!(!s.review().unwrap().unsaved);
        // Another change: the file is ours (its stamp matches), so it is
        // written again.
        comment(&mut s, "And this");
        write_back(&mut s);
        assert_eq!(comments_in(&file), ["Check", "And this"]);
        // Opening for review again reads it back.
        close_writing(&mut s).unwrap();
        start(&mut s).unwrap();
        assert_eq!(list(&s).unwrap().entries.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_review_file_changed_on_disk_is_not_written_over() {
        let (mut s, dir, file) = reviewing_in("conflict");
        comment(&mut s, "Mine 1");
        write_back(&mut s);
        // Someone else writes the file meanwhile.
        let mut theirs = method_session::review::read("x", &std::fs::read(&file).unwrap()).unwrap();
        theirs.comments[0].text = "Theirs".into();
        std::fs::write(&file, method_format::review::write(&theirs)).unwrap();
        comment(&mut s, "Mine 2");
        write_back(&mut s);
        assert_eq!(comments_in(&file), ["Theirs"], "not written over");
        assert_eq!(s.take_notice().as_deref(), Some(CHANGED_ON_DISK));
        let st = s.review().unwrap();
        assert!(st.conflict && st.unsaved);
        assert!(brief(&s).unwrap().conflict);
        // Told once: another change doesn't say it again.
        comment(&mut s, "Mine 3");
        write_back(&mut s);
        assert_eq!(s.take_notice(), None);
        // Closing would lose them: refused, still open.
        assert!(close_writing(&mut s).unwrap_err().contains("stays open"));
        assert!(s.review().is_some());
        // Saving to its own place needs Replace the File.
        assert!(save_at(&mut s, file.clone(), false).is_err());
        assert_eq!(comments_in(&file), ["Theirs"]);
        // Saved beside it, ours go there and the other file is left alone.
        let beside_it = dir.join("Field Station (mine).review.json");
        save_at(&mut s, beside_it.clone(), false).unwrap();
        assert_eq!(comments_in(&beside_it), ["Mine 1", "Mine 2", "Mine 3"]);
        assert_eq!(comments_in(&file), ["Theirs"]);
        assert!(!s.review().unwrap().conflict);
        assert_eq!(s.review().unwrap().file, "Field Station (mine).review.json");
        // Or replaced on purpose.
        save_at(&mut s, file.clone(), true).unwrap();
        assert_eq!(comments_in(&file), ["Mine 1", "Mine 2", "Mine 3"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_keeps_the_review_open() {
        let (mut s, dir, file) = reviewing_in("failed");
        // The folder goes away, so the file can't be written.
        std::fs::remove_dir_all(&dir).unwrap();
        comment(&mut s, "Keep me");
        write_back(&mut s);
        assert!(s.take_notice().unwrap().contains("couldn't be written"));
        let err = close_writing(&mut s).unwrap_err();
        assert!(err.contains("stays open"), "{err}");
        let st = s.review().unwrap();
        assert!(st.unsaved);
        assert_eq!(st.review.comments.len(), 1);
        assert!(!file.exists());
    }
}
