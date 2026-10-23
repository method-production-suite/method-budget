//! What Changed and Merge Changes From (roadmap 3.1 and 3.2; decisions.md,
//! entry 44): what they do is in method-session's `diffmerge` module
//! (re-exported here); these are the Tauri commands, the native Open dialog
//! for a budget file and reading the budget's own file as last saved. Paths
//! never come from the page.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use method_core::Budget;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{AppState, DocId};
use crate::compare::{CompareFiles, Picked};
use crate::view::View;

pub use method_session::diffmerge::*;

/// The budget's own file as last saved.
fn read_saved(path: &Path) -> Result<Budget, String> {
    Ok(crate::session::read_budget(path)?.0)
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// main window only)

/// What changed from `against` to budget `doc` (`None`: the one in front).
#[tauri::command]
pub fn what_changed(
    state: State<'_, AppState>,
    files: State<'_, CompareFiles>,
    doc: Option<DocId>,
    against: Against,
) -> Result<WhatChangedDto, String> {
    crate::i18n::engine_dto(method_session::diffmerge::what_changed(
        &state, &files, doc, against, read_saved,
    ))
}

/// The merge of copy `copy` into budget `doc` (`None`: the one in front)
/// against `base`, with `picks` for its conflicts: a preview, nothing
/// changes.
#[tauri::command]
pub fn merge_preview(
    state: State<'_, AppState>,
    files: State<'_, CompareFiles>,
    doc: Option<DocId>,
    copy: CopySource,
    base: BaseSource,
    picks: Option<BTreeMap<String, String>>,
) -> Result<MergeDto, String> {
    crate::i18n::engine_dto(method_session::diffmerge::merge_preview(
        &state, &files, doc, copy, base, picks,
    ))
}

/// Merges copy `copy` into budget `doc` against `base` with `picks`, as one
/// undo step. The merge is worked out on the budget as it is now.
#[tauri::command]
pub fn merge_apply<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    files: State<'_, CompareFiles>,
    doc: Option<DocId>,
    copy: CopySource,
    base: BaseSource,
    picks: Option<BTreeMap<String, String>>,
) -> Result<View, String> {
    let mut s = crate::i18n::engine_err(method_session::diffmerge::merge_apply(
        &state, &files, doc, copy, base, picks,
    ))?;
    Ok(crate::app::finish(&window, &mut s, None))
}

/// Shows an Open dialog for one budget (`purpose`: `"diff"` to see what
/// changed, `"copy"` a returned copy to merge from, `"base"` the base) and
/// reads it. `None` when cancelled. A file open in a tab comes back as that
/// tab.
#[tauri::command]
pub async fn merge_pick_file<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    files: State<'_, CompareFiles>,
    purpose: String,
) -> Result<Option<Picked>, String> {
    pick_title(&purpose)?;
    let title = &match purpose.as_str() {
        "diff" => crate::i18n::tr("dialog.merge_pick_diff"),
        "copy" => crate::i18n::tr("dialog.merge_pick_copy"),
        _ => crate::i18n::tr("dialog.merge_pick_base"),
    };
    let start = state
        .lock()
        .path()
        .and_then(|p| p.parent())
        .map(PathBuf::from);
    let Some(path) = crate::app::pick_budget(&window, title, start)? else {
        return Ok(None);
    };
    let docs = state.docs();
    Ok(crate::compare::pick(&docs, &files, &[path])?
        .into_iter()
        .next())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docs::Docs;
    use crate::session::Session;
    use crate::session::SessionFiles;
    use method_core::RowId;

    fn first_line(b: &Budget) -> (RowId, String) {
        b.categories()
            .flat_map(|c| &c.accounts)
            .find_map(|a| {
                a.details
                    .iter()
                    .find(|d| !d.rate.is_blank())
                    .map(|d| (d.id, a.code.clone()))
            })
            .unwrap()
    }

    #[test]
    fn what_changed_against_the_saved_file() {
        let dir = std::env::temp_dir().join(format!("method-diffmerge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("saved.mthd");
        let mut s = Session::new();
        s.save_to(&path).unwrap();
        let (row, _) = first_line(s.budget());
        s.set_detail_field(row, "rate", "12345").unwrap();
        let mut docs = Docs::with(s);
        let front = docs.active();
        let d = gather_diff(&mut docs, &CompareFiles::default(), front, Against::Saved)
            .unwrap()
            .diff(read_saved)
            .unwrap();
        assert_eq!(d.from.label, "The saved file");
        assert_eq!(d.changes.len(), 1);
        assert!(d.changes[0].text.contains("-> 12,345.00"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
