//! File > Import Setup From…: setup items picked from another budget file
//! (roadmap 1.20; decisions.md, entries 15, 23 and 29).
//!
//! - **The source.** Rust shows the Open dialog and reads the file (a file
//!   open in a tab is read from the tab, so its unsaved changes count). Paths
//!   never come from the page; it names the source by a token afterwards.
//! - **The review.** Every setup item in the source is listed by kind with
//!   how it stands here: new, the same, or different under the same name.
//!   Each item also lists what it needs (a fringe's flat unit and the globals
//!   in its rate, the globals a global names, a charge's excluded groups and
//!   currency, a credit's groups and excluded fringes, a tag preset's tags,
//!   what a line block's rows use, a report preset's groups), transitively,
//!   so the page can tick them along.
//! - **Importing** brings the picked items and everything they need. New
//!   items are added; items that differ keep this budget's version
//!   (decision 12) unless the user picks the imported one or "add it under a
//!   new name", which the imported items that refer to it then use. One undo
//!   step. The same dependency walk, conflict rule, renaming and definitions
//!   in words as pasting rows (src/clipboard.rs), which this reuses.
//!
//! The review and the import are in method-session's `import_setup`
//! module (re-exported here); these are the dialog, reading the file and the
//! Tauri commands.

use std::path::{Path, PathBuf};

use method_core::Budget;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{finish, AppState, DocId};
use crate::docs::Docs;
use crate::session::DocsFiles;

pub use method_session::import_setup::*;

/// The budget in `path` and its name: from its tab when it is open (unsaved
/// changes count), else read from the file. Refused when it is budget
/// `target` itself.
pub fn read_source(docs: &Docs, target: DocId, path: &Path) -> Result<(String, Budget), String> {
    if let Some(doc) = docs.find_file(path) {
        if doc == target {
            return Err("That is this budget. Pick another file to import from.".into());
        }
        let s = docs.get(doc).ok_or("That budget has been closed.")?;
        return Ok((s.tab_label(), s.budget().clone()));
    }
    let (budget, _) = crate::session::read_budget(path)?;
    let label = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    Ok((label, budget))
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// Shows an Open dialog for the budget to import setup from, reads it and
/// returns the review for budget `doc` (`None`: the one in front). `None`
/// when cancelled.
#[tauri::command]
pub async fn import_setup_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ImportSources>,
    doc: Option<DocId>,
) -> Result<Option<ImportPreview>, String> {
    let (target, start) = {
        let s = state.lock_doc(doc)?;
        (s.id(), s.path().and_then(|p| p.parent()).map(PathBuf::from))
    };
    let Some(path) = crate::app::pick_budget(&window, "Import setup from", start)? else {
        return Ok(None);
    };
    let (label, budget) = read_source(&state.docs(), target, &path)?;
    let s = state.lock_doc(Some(target))?;
    Ok(Some(method_session::import_setup::import_setup_pick(
        &s,
        &sources,
        label,
        budget,
        &path.display().to_string(),
    )))
}

/// `{token, picks, resolutions}`: imports the picked items (and what they
/// need) from the reviewed source into budget `doc`, as one undo step.
#[tauri::command]
pub fn import_setup<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ImportSources>,
    doc: Option<DocId>,
    token: u64,
    picks: Vec<ItemRef>,
    resolutions: Vec<Resolution>,
) -> Result<ImportDone, String> {
    let mut s = state.lock_doc(doc)?;
    let counts =
        method_session::import_setup::import_setup(&mut s, &sources, token, &picks, &resolutions)?;
    Ok(ImportDone::new(finish(&window, &mut s, None), counts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use crate::session::SessionFiles;

    #[test]
    fn a_source_open_in_a_tab_is_read_from_the_tab_and_never_itself() {
        let dir = crate::safety::tests::temp_dir("import-source");
        let path = dir.join("other.mthd");
        let mut other = Session::new();
        other.save_to(&path).unwrap();
        other
            .edit("Test edit", |b| crate::edit::set_global(b, "UNSAVED", "1"))
            .unwrap();
        let mut docs = Docs::default();
        let first = docs.active();
        let tab = docs.add(other);
        let (label, b) = read_source(&docs, first, &path).unwrap();
        assert_eq!(label, "other");
        assert!(b.globals.contains_key("UNSAVED"), "unsaved changes count");
        assert!(read_source(&docs, tab, &path).is_err(), "not from itself");
        docs.remove(tab).unwrap();
        let (_, b) = read_source(&docs, first, &path).unwrap();
        assert!(!b.globals.contains_key("UNSAVED"), "read from the file");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn commands_are_wired_for_the_main_window_only() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
        const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");
        for (module, c) in [
            ("import_setup", "import_setup_pick"),
            ("import_setup", "import_setup"),
            ("templates", "template_new"),
            ("templates", "template_save"),
        ] {
            assert!(
                APP_RS.contains(&format!("crate::{module}::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(
                MAIN_CAPS.contains(&allow),
                "{c} not allowed for the main window"
            );
            assert!(
                !TOOLS_CAPS.contains(&allow),
                "{c} allowed for the tools window"
            );
        }
    }
}
