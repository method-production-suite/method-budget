//! Copies for other people and filtered copies (roadmap 2.21 and 2.22;
//! decisions.md, entry 39): the commands behind File > Save a Copy for
//! Sharing…, a shared copy's banner and File > New Sub-Budget…. What they
//! do is method-session's `sharing` module (re-exported here); this shows
//! the Save dialog, writes the copy and opens new tabs in the window.
//!
//! Save a Copy for Sharing writes the copy to a new file picked in Rust's
//! Save dialog, atomically; a file open in a tab is refused.

use std::path::PathBuf;

use method_core::VersionId;
use tauri::{Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{apply, finish, with_budget_extension, AppState, DocId};
use crate::safety::{self, Access};
use crate::session::DocsFiles;
use crate::view::View;

pub use method_session::sharing::*;

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// What `share_copy_save` wrote.
#[derive(Debug, serde::Serialize)]
pub struct SavedCopy {
    /// The file's name.
    pub name: String,
    pub path: String,
}

/// Shows a Save dialog and writes a copy of budget `doc` (`None`: the one in
/// front), or of its version `version`, for sharing: without what
/// `leave_out` says, stamped with `note` and `saved`. The budget is not
/// changed. `None` when cancelled. Refused for a file open in a tab.
#[tauri::command]
pub async fn share_copy_save<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    version: Option<VersionId>,
    leave_out: Option<ShareInput>,
    note: Option<String>,
    saved: String,
) -> Result<Option<SavedCopy>, String> {
    let (copy, stem, dir) = {
        let s = state.lock_doc(doc)?;
        let (copy, stem) = make_copy(
            s.budget(),
            version,
            leave_out.unwrap_or_default(),
            note.as_deref(),
            &saved,
        )?;
        (
            copy,
            stem,
            s.path().and_then(|p| p.parent()).map(PathBuf::from),
        )
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title(crate::i18n::tr("dialog.save_a_copy_for_sharing"))
        .add_filter(
            crate::i18n::tr("dialog.filter_budget"),
            &[method_format::EXTENSION],
        )
        .set_file_name(format!("{stem}.{}", method_format::EXTENSION))
        .set_parent(&window);
    if let Some(dir) = dir {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let path = with_budget_extension(picked.into_path().map_err(|e| e.to_string())?);
    if state.docs().find_file(&path).is_some() {
        return Err(crate::i18n::tr("sharing.copy_open_in_tab"));
    }
    let text = method_format::to_string(&copy);
    safety::write_atomically(&path, text.as_bytes(), Access::Document)
        .map_err(|e| format!("Couldn't save {}: {e}", path.display()))?;
    Ok(Some(SavedCopy {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.display().to_string(),
    }))
}

/// Opens an editable copy of shared copy `doc` in a new tab, in front.
#[tauri::command]
pub fn share_copy_editable<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> Result<View, String> {
    let id = open_editable(&state, doc)?;
    Ok(finish(&window, &mut state.lock_doc(Some(id))?, None))
}

/// The open budget shared or editable copy `doc` (`None`: the one in front)
/// came from, if one is open.
#[tauri::command]
pub fn share_copy_original(
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> Result<Option<OriginalDto>, String> {
    let id = state.lock_doc(doc)?.id();
    Ok(method_session::sharing::share_copy_original(
        &state.docs(),
        id,
    ))
}

/// Stores copy `doc` (`None`: the one in front) as a new version of open
/// budget `original`, named `name` (one undo step in `original`). Returns
/// `original`'s view.
#[tauri::command]
pub fn share_copy_store<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    original: DocId,
    name: String,
    created: String,
) -> Result<View, String> {
    let copy = copy_to_store(&state, doc, original)?;
    apply(&window, &state, Some(original), |s| {
        method_session::sharing::share_copy_store(s, &copy, &name, &created)
    })
}

/// What `sub_budget_from_row` made.
#[derive(Debug, serde::Serialize)]
pub struct RowSubBudget {
    /// The sub-budget, in front.
    pub view: View,
    pub message: String,
}

/// Makes the sub-budget of category or account row `row` of budget `doc`
/// (`None`: the one in front): a separate file beside the budget's own,
/// named after its title (the first free "… 2", "… 3" when taken, never
/// over a file), opened in front as a saved budget. A budget with no file
/// yet gets the sub-budget in a new tab with no file.
#[tauri::command]
pub fn sub_budget_from_row<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, crate::safety_app::Safety>,
    doc: Option<DocId>,
    row: RowInput,
    saved: String,
) -> Result<RowSubBudget, String> {
    let (b, dir) = {
        let s = state.lock_doc(doc)?;
        let b = make_row_sub_budget(s.budget(), &row, &saved)?;
        (b, s.path().and_then(|p| p.parent()).map(PathBuf::from))
    };
    let Some(dir) = dir.filter(|d| d.is_dir()) else {
        let title = b.title.clone();
        let id = state
            .docs()
            .add(crate::session::Session::with_budget(b, None));
        return Ok(RowSubBudget {
            view: finish(&window, &mut state.lock_doc(Some(id))?, None),
            message: format!("Opened \"{title}\" in a new tab. It has no file until you save it."),
        });
    };
    let stem = crate::app::safe_file_stem(&b.title);
    let (id, path) = crate::start::save_new_free(&state, &safety, &b, &dir, &stem)?;
    crate::safety_app::refresh_menu(window.app_handle());
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(RowSubBudget {
        view: finish(&window, &mut state.lock_doc(Some(id))?, None),
        message: format!(
            "Made the sub-budget \"{}\", saved beside this budget as {name}.",
            b.title
        ),
    })
}

/// Opens a sub-budget of budget `doc` (`None`: the one in front) in a new
/// tab, in front, with no file.
#[tauri::command]
pub fn sub_budget_new<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    spec: SubBudgetInput,
    saved: String,
) -> Result<View, String> {
    let id = method_session::sharing::sub_budget_new(&state, doc, spec, &saved)?;
    Ok(finish(&window, &mut state.lock_doc(Some(id))?, None))
}
