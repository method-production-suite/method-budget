//! Templates (roadmap 1.20; decisions.md, entry 29): File > New From
//! Template… and File > Save as Template….
//!
//! A template is an ordinary `.fwb` budget, no new format. What makes a
//! budget a good start for another is its chart (categories, accounts and
//! production totals), its setup (globals, fringes, groups, units, sets,
//! locations, currencies, charges, credits, presets) and its settings
//! (account numbering, fringe posting, new-line defaults, currencies). What
//! belongs to one production is left out: the detail lines (unless the user
//! keeps them), the budget info, the baseline, the saved versions and every
//! line note. [`strip`] does that, and both commands use it:
//!
//! - **New From Template** reads a picked `.fwb` (from its tab when it is
//!   open), strips it and opens it in a new tab with no file, titled
//!   "Untitled", so the first Save asks where.
//! - **Save as Template** writes the budget in front, stripped, to a new
//!   file picked in a Save dialog, titled after that file. The budget itself
//!   is not changed and keeps its own file.

use std::path::PathBuf;

use fwb_core::Budget;
use serde::Serialize;
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, safe_file_stem, with_fwb_extension, AppState, DocId};
use crate::safety::{self, Access};
use crate::session::Session;
use crate::view::View;

/// What a budget opened from a template is called until it is saved.
pub const UNTITLED: &str = "Untitled";

/// `b` as a template titled `title`: chart, setup and settings kept; budget
/// info, baseline, versions and the notes on categories and accounts
/// cleared; the detail rows dropped, or with `keep_lines` kept without their
/// notes.
pub fn strip(b: &Budget, keep_lines: bool, title: &str) -> Budget {
    let mut t = b.clone();
    t.title = title.trim().to_string();
    t.info.clear();
    t.baseline = None;
    t.versions.clear();
    // Actuals are one production's spending.
    t.actuals = Default::default();
    // A template is nobody's copy: a shared copy's stamp would open it read-only.
    t.provenance = None;
    for c in t.categories_mut() {
        c.notes.clear();
        for a in &mut c.accounts {
            a.notes.clear();
            if keep_lines {
                for d in &mut a.details {
                    d.notes.clear();
                }
            } else {
                a.details.clear();
            }
        }
    }
    t
}

/// The budget in `path`: from its tab when it is open (unsaved changes
/// count), else read from the file.
fn read(state: &AppState, path: &std::path::Path) -> Result<Budget, String> {
    let docs = state.docs();
    if let Some(doc) = docs.find_file(path) {
        let s = docs.get(doc).ok_or("That budget has been closed.")?;
        return Ok(s.budget().clone());
    }
    drop(docs);
    Ok(crate::session::read_budget(path)?.0)
}

/// Opens `template` stripped (as [`strip`] says) in a new tab, in front,
/// with no file; its id.
pub fn open_new(state: &AppState, template: &Budget, keep_lines: bool) -> DocId {
    let b = strip(template, keep_lines, UNTITLED);
    state.docs().add(Session::with_budget(b, None))
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// Shows an Open dialog for a template (any `.fwb`) and opens a new budget
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
    let mut dialog = window
        .dialog()
        .file()
        .set_title("New budget from template")
        .add_filter("Film Wizard budget", &["fwb"])
        .set_parent(&window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let template = read(&state, &path)?;
    let id = open_new(&state, &template, keep_lines);
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
        let stem = match s.path().and_then(|p| p.file_stem()) {
            Some(stem) => stem.to_string_lossy().into_owned(),
            None => s.budget().title.trim().to_string(),
        };
        (
            s.id(),
            format!("{} template", safe_file_stem(&stem)),
            s.path().and_then(|p| p.parent()).map(PathBuf::from),
        )
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Save as template")
        .add_filter("Film Wizard budget", &["fwb"])
        .set_file_name(format!("{suggested}.fwb"))
        .set_parent(&window);
    if let Some(dir) = dir {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let path = with_fwb_extension(picked.into_path().map_err(|e| e.to_string())?);
    let text = {
        let docs = state.docs();
        if docs.find_file(&path).is_some() {
            return Err("That file is open in a tab. Save the template under another name.".into());
        }
        let s = docs.get(id).ok_or("That budget has been closed.")?;
        let title = path
            .file_stem()
            .map(|t| t.to_string_lossy().into_owned())
            .unwrap_or_default();
        fwb_format::to_string(&strip(s.budget(), keep_lines, &title))
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
    use fwb_core::{starter, Baseline, InfoValue};

    fn full() -> Budget {
        let mut b = starter::budget();
        b.title = "Big Show".into();
        b.info
            .insert("Director".into(), InfoValue::Text("Someone".into()));
        b.baseline = Some(Baseline::default());
        fwb_core::save_version(&mut b, "Draft", "2026-10-09T10:00:00Z", None);
        let c = b.categories_mut().next().unwrap();
        c.notes = vec![fwb_core::Note::plain(1, "category note")];
        c.accounts[0].notes = vec![fwb_core::Note::plain(1, "account note")];
        let d = &mut c.accounts[0].details[0];
        d.notes = vec![fwb_core::Note::plain(1, "private")];
        b
    }

    fn lines(b: &Budget) -> usize {
        b.categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.details.len())
            .sum()
    }

    #[test]
    fn a_template_keeps_chart_setup_and_settings_only() {
        let b = full();
        let t = strip(&b, false, "Feature template");
        assert_eq!(t.title, "Feature template");
        assert_eq!(lines(&t), 0, "no detail lines");
        assert!(t.info.is_empty() && t.baseline.is_none() && t.versions.is_empty());
        // The chart, setup and settings are as they were.
        let codes = |b: &Budget| -> Vec<String> {
            b.categories()
                .flat_map(|c| &c.accounts)
                .map(|a| a.code.clone())
                .collect()
        };
        assert_eq!(codes(&t), codes(&b));
        assert_eq!(t.topsheet.len(), b.topsheet.len());
        assert_eq!(t.globals, b.globals);
        assert_eq!(t.fringes, b.fringes);
        assert_eq!(t.charges, b.charges);
        assert_eq!(t.units, b.units);
        assert_eq!(t.account_coding, b.account_coding);
        assert_eq!(t.fringe_posting, b.fringe_posting);
        assert_eq!(t.base_currency, b.base_currency);
        // It recalculates (to its charges' share of nothing).
        let r = fwb_core::recalc(&t, &fwb_core::RecalcOptions::default()).unwrap();
        assert_eq!(r.grand_total, 0);

        let kept = strip(&b, true, "With lines");
        assert_eq!(lines(&kept), lines(&b), "keep lines");
        assert!(kept
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .all(|d| d.notes.is_empty()));
        assert!(kept
            .categories()
            .all(|c| c.notes.is_empty() && c.accounts.iter().all(|a| a.notes.is_empty())));
        // The format round-trips it.
        let text = fwb_format::to_string(&t);
        assert_eq!(fwb_format::from_str(&text).unwrap(), t);
    }

    #[test]
    fn a_template_opens_untitled_in_a_new_tab() {
        let state = AppState::with(Docs::default());
        let first = state.docs().active();
        let id = open_new(&state, &full(), false);
        let docs = state.docs();
        assert_ne!(id, first);
        assert_eq!(docs.len(), 2, "a new tab, the first one kept");
        assert_eq!(docs.active(), id, "in front");
        let s = docs.get(id).unwrap();
        assert!(s.path().is_none(), "no file: the first Save asks where");
        assert_eq!(s.budget().title, UNTITLED);
        assert_eq!(s.tab_label(), UNTITLED);
        assert_eq!(lines(s.budget()), 0);
        assert!(s.budget().versions.is_empty());
    }

    #[test]
    fn templates_are_read_from_their_tab_when_open() {
        let dir = safety::tests::temp_dir("template-read");
        let path = dir.join("t.fwb");
        let mut s = Session::new();
        s.save_to(&path).unwrap();
        s.edit("Test edit", |b| crate::edit::set_global(b, "UNSAVED", "1"))
            .unwrap();
        let state = AppState::with(Docs::with(s));
        assert!(read(&state, &path).unwrap().globals.contains_key("UNSAVED"));
        let other = dir.join("u.fwb");
        Session::new().save_to(&other).unwrap();
        assert!(!read(&state, &other)
            .unwrap()
            .globals
            .contains_key("UNSAVED"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
