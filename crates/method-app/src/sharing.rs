//! Copies for other people and filtered copies (roadmap 2.21 and 2.22;
//! decisions.md, entry 39): the commands behind File > Save a Copy for
//! Sharing…, a shared copy's banner (Make Editable Copy, Compare With
//! Original, Store as a Version of the Original), and File > New
//! Sub-Budget…. The engine does the work (`method_core::share`,
//! `method_core::subbudget`); this checks what the page sends, shows the Save
//! dialog and opens new tabs.
//!
//! - **Save a Copy for Sharing** writes a snapshot of the budget in front (or
//!   of one of its versions) to a new file picked in Rust's Save dialog,
//!   suggested as `<title> - <version> - <date>.mthd`, optionally without
//!   notes, budget info, versions, baseline or unused setup items, stamped
//!   with a `provenance` (source title, version, when, app version, a note).
//!   The budget and its own file don't change; a file open in a tab is
//!   refused.
//! - A budget whose provenance says "shared copy" opens **read-only**: every
//!   edit is refused in [`crate::session::Session::edit`] with [`READ_ONLY`].
//!   Make Editable Copy opens the same budget in a new untitled tab, stamped
//!   as an editable copy.
//! - When a shared (or editable) copy is open next to the budget it came
//!   from (matched by title, [`method_core::share::came_from`]), the page offers
//!   to compare them (the compare screen) or to store the copy as a version
//!   of the original (one undo step in the original).
//! - **New Sub-Budget** opens a filtered copy of the budget in front in a new
//!   tab with no file, titled after its parent, stamped with it.
//!
//! The page sends the time (`saved`, `created`) as its local ISO 8601
//! date-time, checked for shape only, as for versions; the engine never
//! reads the clock. A copy never carries a path: the provenance names the
//! source by title only.

use std::path::PathBuf;

use method_core::share::{self, ShareOptions};
use method_core::subbudget::{self, SubBudgetSpec, TagFilter};
use method_core::{Provenance, ProvenanceKind, VersionId, MAX_NOTE_CHARS};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{edit_budget, finish, safe_file_stem, with_budget_extension, AppState, DocId};
use crate::docs::Docs;
use crate::edit::EditResult;
use crate::safety::{self, Access};
use crate::session::Session;
use crate::templates::UNTITLED;
use crate::versions::{check_created, check_name, check_note};
use crate::view::View;

/// What an edit to a shared copy is told.
pub const READ_ONLY: &str =
    "This is a copy saved for sharing, so it opens read-only. Make an editable copy to change it.";

/// The application version stamped on copies.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// What to leave out of a shared copy, as the page sends it.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ShareInput {
    pub notes: bool,
    pub info: bool,
    pub versions: bool,
    pub baseline: bool,
    pub unused_setup: bool,
    pub actuals: bool,
}

impl From<ShareInput> for ShareOptions {
    fn from(i: ShareInput) -> ShareOptions {
        ShareOptions {
            drop_notes: i.notes,
            drop_info: i.info,
            drop_versions: i.versions,
            drop_baseline: i.baseline,
            drop_unused_setup: i.unused_setup,
            drop_actuals: i.actuals,
        }
    }
}

/// The file name a shared copy is suggested under, without its extension:
/// `<title> - <version> - <date>` (no version part when the budget is shared
/// as it stands), safe for any file system.
pub fn suggested_stem(title: &str, version: Option<&str>, saved: &str) -> String {
    let title = title.trim();
    let mut parts = vec![if title.is_empty() { UNTITLED } else { title }.to_string()];
    if let Some(v) = version.map(str::trim).filter(|v| !v.is_empty()) {
        parts.push(v.to_string());
    }
    if let Some(date) = saved.get(..10) {
        parts.push(date.to_string());
    }
    safe_file_stem(&parts.join(" - "))
}

/// The shared copy of budget `b` (its version `version`, if any) and the
/// suggested file stem. Checks the page's input.
pub fn make_copy(
    b: &method_core::Budget,
    version: Option<VersionId>,
    leave_out: ShareInput,
    note: Option<&str>,
    saved: &str,
) -> EditResult<(method_core::Budget, String)> {
    let saved = check_created(saved)?;
    let note = check_note(note)?;
    let copy = share::share_copy(b, version, &leave_out.into(), &saved, APP_VERSION, note)
        .map_err(|e| e.to_string())?;
    let version_name = copy
        .provenance
        .as_ref()
        .and_then(|p| p.source_version.clone());
    let stem = suggested_stem(&b.title, version_name.as_deref(), &saved);
    Ok((copy, stem))
}

/// Opens an editable copy of shared copy `doc` in a new tab, in front, with
/// no file; its id.
pub fn open_editable(state: &AppState, doc: Option<DocId>) -> Result<DocId, String> {
    let copy = {
        let s = state.lock_doc(doc)?;
        if !s.is_read_only() {
            return Err("This budget isn't a copy saved for sharing.".into());
        }
        share::editable_copy(s.budget())
    };
    Ok(state.docs().add(Session::with_budget(copy, None)))
}

/// The first open budget (in tab order) that copy `doc` came from, if any.
pub fn original_of(docs: &Docs, doc: DocId) -> Option<DocId> {
    let copy = docs.get(doc)?.budget();
    copy.provenance.as_ref()?;
    docs.ids().into_iter().filter(|id| *id != doc).find(|id| {
        docs.get(*id)
            .is_some_and(|s| share::came_from(copy, s.budget()))
    })
}

/// Stores copy `copy`'s content as a new version of budget `original`,
/// named `name`, at `created`; the version's note says where it came from.
/// Returns the version's id.
pub fn store_as_version(
    original: &mut method_core::Budget,
    copy: &method_core::Budget,
    name: &str,
    created: &str,
) -> EditResult<VersionId> {
    let name = check_name(name)?;
    let created = check_created(created)?;
    let note = copy.provenance.as_ref().map(|p| {
        let mut n = format!("From a copy saved for sharing on {}", p.saved);
        if let Some(v) = &p.source_version {
            n.push_str(&format!(" (version \"{v}\")"));
        }
        if let Some(text) = &p.note {
            n.push_str(": ");
            n.push_str(text);
        }
        n.chars().take(MAX_NOTE_CHARS).collect()
    });
    Ok(method_core::versions::add_version(
        original,
        name,
        created,
        note,
        copy.snapshot(),
    ))
}

/// A tag filter as the page sends it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct TagFilterInput {
    pub values: Vec<String>,
    pub include_untagged: bool,
}

impl From<TagFilterInput> for TagFilter {
    fn from(i: TagFilterInput) -> TagFilter {
        TagFilter {
            values: i.values.into_iter().map(|v| v.trim().to_string()).collect(),
            include_untagged: i.include_untagged,
        }
    }
}

/// What goes into a sub-budget, as the page sends it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct SubBudgetInput {
    pub categories: Vec<String>,
    pub accounts: Vec<String>,
    pub groups: TagFilterInput,
    pub sets: TagFilterInput,
    pub locations: TagFilterInput,
    pub skip_empty_accounts: bool,
    pub drop_fringes: bool,
    pub drop_notes: bool,
}

impl From<SubBudgetInput> for SubBudgetSpec {
    fn from(i: SubBudgetInput) -> SubBudgetSpec {
        SubBudgetSpec {
            categories: i.categories.into_iter().collect(),
            accounts: i.accounts.into_iter().collect(),
            groups: i.groups.into(),
            sets: i.sets.into(),
            locations: i.locations.into(),
            skip_empty_accounts: i.skip_empty_accounts,
            drop_fringes: i.drop_fringes,
            drop_notes: i.drop_notes,
        }
    }
}

/// The sub-budget of `parent` that `spec` describes, titled after its parent
/// ("Big Show (sub-budget)") and stamped with it.
pub fn make_sub_budget(
    parent: &method_core::Budget,
    spec: SubBudgetInput,
    saved: &str,
) -> EditResult<method_core::Budget> {
    let saved = check_created(saved)?;
    let spec: SubBudgetSpec = spec.into();
    let mut b = subbudget::sub_budget(parent, &spec).map_err(|e| e.to_string())?;
    let parent_title = parent.title.trim();
    b.title = format!(
        "{} (sub-budget)",
        if parent_title.is_empty() {
            UNTITLED
        } else {
            parent_title
        }
    );
    b.provenance = Some(Provenance {
        kind: ProvenanceKind::SubBudget,
        source_title: parent.title.trim().to_string(),
        source_version: None,
        saved,
        app_version: APP_VERSION.to_string(),
        note: Some(subbudget::describe(&spec)),
        pack: None,
    });
    Ok(b)
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// What `share_copy_save` wrote.
#[derive(Debug, Serialize)]
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
        .set_title("Save a copy for sharing")
        .add_filter(crate::app::FILTER_NAME, &[method_format::EXTENSION])
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
        return Err("That file is open in a tab. Save the copy under another name.".into());
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

/// The open budget copy `doc` came from, for the page's offer.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct OriginalDto {
    pub doc: DocId,
    /// Its tab's label.
    pub label: String,
}

/// The open budget shared or editable copy `doc` (`None`: the one in front)
/// came from, if one is open.
#[tauri::command]
pub fn share_copy_original(
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> Result<Option<OriginalDto>, String> {
    let id = state.lock_doc(doc)?.id();
    let docs = state.docs();
    Ok(original_of(&docs, id).and_then(|o| {
        docs.get(o).map(|s| OriginalDto {
            doc: o,
            label: s.tab_label(),
        })
    }))
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
    let copy = {
        let s = state.lock_doc(doc)?;
        if s.id() == original {
            return Err("A budget can't be stored as a version of itself.".into());
        }
        s.budget().clone()
    };
    let label = format!("Store a returned copy as version \"{}\"", name.trim());
    edit_budget(&window, &state, Some(original), label, |b| {
        store_as_version(b, &copy, &name, &created)
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
    let b = {
        let s = state.lock_doc(doc)?;
        make_sub_budget(s.budget(), spec, &saved)?
    };
    let id = state.docs().add(Session::with_budget(b, None));
    Ok(finish(&window, &mut state.lock_doc(Some(id))?, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::{starter, Budget, InfoValue};

    const AT: &str = "2026-10-09T14:30:00-07:00";

    fn show() -> Budget {
        let mut b = starter::budget();
        b.title = "Big Show".into();
        b.info
            .insert("Director".into(), InfoValue::Text("Someone".into()));
        method_core::save_version(&mut b, "Studio draft", AT, None);
        b
    }

    #[test]
    fn suggested_names() {
        assert_eq!(
            suggested_stem("Big Show", None, AT),
            "Big Show - 2026-10-09"
        );
        assert_eq!(
            suggested_stem("Big: Show", Some("Studio draft"), AT),
            "Big  Show - Studio draft - 2026-10-09"
        );
        assert_eq!(suggested_stem("  ", Some(" "), AT), "Untitled - 2026-10-09");
    }

    #[test]
    fn copies_check_their_input_and_name_the_version() {
        let b = show();
        let (copy, stem) =
            make_copy(&b, None, ShareInput::default(), Some("For the studio"), AT).unwrap();
        assert_eq!(stem, "Big Show - 2026-10-09");
        let p = copy.provenance.as_ref().unwrap();
        assert!(p.is_shared_copy());
        assert_eq!(p.app_version, APP_VERSION);
        assert_eq!(p.note.as_deref(), Some("For the studio"));

        let leave = ShareInput {
            info: true,
            versions: true,
            ..ShareInput::default()
        };
        let id = b.versions[0].id;
        let (copy, stem) = make_copy(&b, Some(id), leave, None, AT).unwrap();
        assert_eq!(stem, "Big Show - Studio draft - 2026-10-09");
        assert!(copy.info.is_empty() && copy.versions.is_empty());

        assert!(make_copy(&b, None, ShareInput::default(), None, "yesterday").is_err());
        assert!(make_copy(&b, Some(99), ShareInput::default(), None, AT).is_err());
        let long = "x".repeat(MAX_NOTE_CHARS + 1);
        assert!(make_copy(&b, None, ShareInput::default(), Some(&long), AT).is_err());
        // The copy round-trips through the file format.
        let text = method_format::to_string(&copy);
        assert_eq!(method_format::from_str(&text).unwrap(), copy);
    }

    #[test]
    fn a_shared_copy_is_read_only_until_an_editable_copy_is_made() {
        let (copy, _) = make_copy(&show(), None, ShareInput::default(), None, AT).unwrap();
        let mut s = Session::with_budget(copy, None);
        assert!(s.is_read_only());
        let before = s.budget().clone();
        let e = s
            .edit("Test", |b| crate::edit::set_title(b, "Changed"))
            .unwrap_err();
        assert_eq!(e, READ_ONLY);
        assert_eq!(s.budget(), &before);
        assert!(!s.can_undo() && !s.is_dirty());

        let state = AppState::with(Docs::with(s));
        let first = state.docs().active();
        let id = open_editable(&state, None).unwrap();
        let docs = state.docs();
        assert_ne!(id, first);
        let e = docs.get(id).unwrap();
        assert!(!e.is_read_only() && e.path().is_none() && !e.is_dirty());
        assert_eq!(
            e.budget().provenance.as_ref().unwrap().kind,
            ProvenanceKind::EditableCopy
        );
        drop(docs);
        // Only a shared copy has an editable copy.
        assert!(open_editable(&state, Some(id)).is_err());
        // A template made from a shared copy is nobody's copy.
        let docs = state.docs();
        let t = crate::templates::strip(docs.get(first).unwrap().budget(), true, "T");
        assert!(t.provenance.is_none());
    }

    #[test]
    fn a_returned_copy_finds_its_original_and_is_stored_as_a_version() {
        let original = show();
        let (copy, _) =
            make_copy(&original, None, ShareInput::default(), Some("notes"), AT).unwrap();
        let mut docs = Docs::with(Session::with_budget(original.clone(), None));
        let orig_id = docs.active();
        let other = docs.add(Session::new());
        let copy_id = docs.add(Session::with_budget(copy.clone(), None));
        assert_eq!(original_of(&docs, copy_id), Some(orig_id));
        assert_eq!(
            original_of(&docs, other),
            None,
            "no provenance, no original"
        );
        assert_eq!(original_of(&docs, orig_id), None);

        let state = AppState::with(docs);
        let before = original.versions.len();
        {
            let mut s = state.lock_doc(Some(orig_id)).unwrap();
            let id = s
                .edit("Store", |b| store_as_version(b, &copy, "Returned copy", AT))
                .unwrap();
            let v = s.budget().version(id).unwrap();
            assert_eq!(v.name, "Returned copy");
            assert!(v.note.as_deref().unwrap().contains("notes"));
            assert_eq!(*v.budget, copy.snapshot());
            assert_eq!(s.budget().versions.len(), before + 1);
            // One undo step.
            s.undo();
            assert_eq!(s.budget().versions.len(), before);
        }
        let mut b = original.clone();
        assert!(store_as_version(&mut b, &copy, " ", AT).is_err());
    }

    #[test]
    fn sub_budgets_are_titled_after_their_parent_and_stamped() {
        let parent = show();
        let first_cat = parent.categories().next().unwrap().code.clone();
        let spec = SubBudgetInput {
            categories: vec![first_cat.clone()],
            drop_notes: true,
            ..SubBudgetInput::default()
        };
        let b = make_sub_budget(&parent, spec, AT).unwrap();
        assert_eq!(b.title, "Big Show (sub-budget)");
        assert_eq!(b.categories().count(), 1);
        let p = b.provenance.as_ref().unwrap();
        assert_eq!(p.kind, ProvenanceKind::SubBudget);
        assert_eq!(p.source_title, "Big Show");
        assert_eq!(
            p.note.as_deref(),
            Some(format!("Categories {first_cat}; without notes").as_str())
        );
        assert!(b.versions.is_empty());
        let s = Session::with_budget(b, None);
        assert!(!s.is_read_only(), "a sub-budget is editable");
        assert!(make_sub_budget(
            &parent,
            SubBudgetInput {
                accounts: vec!["NOPE".into()],
                ..SubBudgetInput::default()
            },
            AT
        )
        .is_err());
    }
}
