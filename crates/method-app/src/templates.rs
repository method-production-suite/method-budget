//! Templates (roadmap 1.20; decisions.md, entry 29): File > New From
//! Template… and File > Save as Template….
//!
//! A template is an ordinary budget file, no new format. What makes a
//! budget a good start for another is its chart (categories, accounts and
//! production totals), its setup (globals, fringes, groups, units, sets,
//! locations, currencies, charges, credits, presets) and its settings
//! (account numbering, fringe posting, new-line defaults, currencies). What
//! belongs to one production is left out: the detail lines (unless the user
//! keeps them), the budget info, the baseline, the saved versions and every
//! line note. [`strip`] does that, and both commands use it:
//!
//! - **New From Template** reads a picked budget file (from its tab when it is
//!   open), strips it and opens it in a new tab with no file, titled
//!   "Untitled", so the first Save asks where.
//! - **Save as Template** writes the budget in front, stripped, to a new
//!   file picked in a Save dialog, titled after that file. The budget itself
//!   is not changed and keeps its own file.
//!
//! What they do with the budget is in method-session's `templates` module
//! (re-exported here); these are the dialogs, the files and the Tauri
//! commands.

use std::path::PathBuf;

use method_core::Budget;
use serde::Serialize;
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, with_budget_extension, AppState, DocId};
use crate::safety::{self, Access};
use crate::session::DocsFiles;
use crate::view::View;

pub use method_session::templates::*;

/// The budget in `path`: from its tab when it is open (unsaved changes
/// count), else read from the file.
pub(crate) fn read(state: &AppState, path: &std::path::Path) -> Result<Budget, String> {
    let docs = state.docs();
    if let Some(doc) = docs.find_file(path) {
        let s = docs.get(doc).ok_or("That budget has been closed.")?;
        return Ok(s.budget().clone());
    }
    drop(docs);
    Ok(crate::session::read_budget(path)?.0)
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// Shows an Open dialog for a template (any budget file) and opens a new budget
/// from it in a new tab, in front: its chart, setup and settings, no lines
/// unless `keep_lines`. `None` when cancelled.
#[tauri::command]
pub async fn template_new<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    keep_lines: bool,
) -> Result<Option<View>, String> {
    let start = state
        .lock()
        .path()
        .and_then(|p| p.parent())
        .map(PathBuf::from);
    let Some(path) = crate::app::pick_budget(
        &window,
        &crate::i18n::tr("dialog.new_budget_from_template"),
        start,
    )?
    else {
        return Ok(None);
    };
    let template = read(&state, &path)?;
    let id = method_session::templates::template_new(&state, &template, keep_lines);
    Ok(Some(finish(&window, &mut state.lock_doc(Some(id))?, None)))
}

/// What `template_save` wrote.
#[derive(Debug, Serialize)]
pub struct SavedTemplate {
    /// The file's name.
    pub name: String,
    pub path: String,
}

/// Shows a Save dialog and writes budget `doc` (`None`: the one in front)
/// there as a template: no lines unless `keep_lines`, no info, baseline,
/// versions or notes, titled after the file. The budget is not changed.
/// `None` when cancelled. Refused for a file open in a tab.
#[tauri::command]
pub async fn template_save<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    keep_lines: bool,
) -> Result<Option<SavedTemplate>, String> {
    let (id, suggested, dir) = {
        let s = state.lock_doc(doc)?;
        (
            s.id(),
            template_name(&s),
            s.path().and_then(|p| p.parent()).map(PathBuf::from),
        )
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title(crate::i18n::tr("dialog.save_as_template"))
        .add_filter(
            crate::i18n::tr("dialog.filter_budget"),
            &[method_format::EXTENSION],
        )
        .set_file_name(format!("{suggested}.{}", method_format::EXTENSION))
        .set_parent(&window);
    if let Some(dir) = dir {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let path = with_budget_extension(picked.into_path().map_err(|e| e.to_string())?);
    let text = {
        let docs = state.docs();
        if docs.find_file(&path).is_some() {
            return Err(crate::i18n::tr("templates.open_in_tab"));
        }
        let s = docs.get(id).ok_or("That budget has been closed.")?;
        let title = path
            .file_stem()
            .map(|t| t.to_string_lossy().into_owned())
            .unwrap_or_default();
        method_session::templates::template_save(s, keep_lines, &title)
    };
    safety::write_atomically(&path, text.as_bytes(), Access::Document)
        .map_err(|e| format!("Couldn't save {}: {e}", path.display()))?;
    Ok(Some(SavedTemplate {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.display().to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docs::Docs;
    use crate::session::{Session, SessionFiles};

    #[test]
    fn templates_are_read_from_their_tab_when_open() {
        let dir = safety::tests::temp_dir("template-read");
        let path = dir.join("t.mthd");
        let mut s = Session::new();
        s.save_to(&path).unwrap();
        s.edit("Test edit", |b| crate::edit::set_global(b, "UNSAVED", "1"))
            .unwrap();
        let state = AppState::with(Docs::with(s));
        assert!(read(&state, &path).unwrap().globals.contains_key("UNSAVED"));
        let other = dir.join("u.mthd");
        Session::new().save_to(&other).unwrap();
        assert!(!read(&state, &other)
            .unwrap()
            .globals
            .contains_key("UNSAVED"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
