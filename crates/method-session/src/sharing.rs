//! Copies for other people and filtered copies (roadmap 2.21 and 2.22;
//! decisions.md, entry 39): what is behind File > Save a Copy for
//! Sharing…, a shared copy's banner (Make Editable Copy, Compare With
//! Original, Store as a Version of the Original), and File > New
//! Sub-Budget…. The engine does the work (`method_core::share`,
//! `method_core::subbudget`); this checks what the page sends and opens new
//! tabs. Showing the Save dialog and writing the copy are the desktop's
//! (method-app's src/sharing.rs).
//!
//! - **Save a Copy for Sharing** makes a snapshot of the budget in front (or
//!   of one of its versions), suggested as `<title> - <version> - <date>`,
//!   optionally without notes, budget info, versions, baseline or unused
//!   setup items, stamped with a `provenance` (source title, version, when,
//!   app version, a note). The budget doesn't change.
//! - A budget whose provenance says "shared copy" opens **read-only**: every
//!   edit is refused in [`crate::session::Session::edit`] with [`read_only`].
//!   Make Editable Copy opens the same budget in a new untitled tab, stamped
//!   as an editable copy.
//! - When a shared (or editable) copy is open next to the budget it came
//!   from (matched by title, [`method_core::share::came_from`]), the page offers
//!   to compare them (the compare screen) or to store the copy as a version
//!   of the original (one undo step in the original).
//! - **New Sub-Budget** opens a filtered copy of the budget in front in a new
//!   tab with no file, titled after its parent, stamped with it.
//! - **New Sub-Budget from This Category or Account** (decision 91) makes a
//!   budget of one row: its accounts and lines and only the setup they use
//!   ([`make_row_sub_budget`] lists what carries over), with the row's
//!   totals. The desktop saves it beside the parent's file.
//!
//! The page sends the time (`saved`, `created`) as its local ISO 8601
//! date-time, checked for shape only, as for versions; the engine never
//! reads the clock. A copy never carries a path: the provenance names the
//! source by title only.

use method_core::share::{self, ShareOptions};
use method_core::subbudget::{self, SubBudgetSpec, TagFilter};
use method_core::{Budget, Provenance, ProvenanceKind, VersionId, MAX_NOTE_CHARS};
use serde::{Deserialize, Serialize};

use crate::docs::{AppState, DocId, Docs};
use crate::edit::EditResult;
use crate::session::Session;
use crate::templates::{safe_file_stem, UNTITLED};
use crate::versions::{check_created, check_name, check_note};

/// What an edit to a shared copy is told.
pub use crate::session::read_only;

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
// A sub-budget from one row (roadmap 3.19; decisions.md, entry 91)

/// The category or account row a sub-budget starts from, as the page sends
/// it: `{kind: "category" | "account", code}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RowInput {
    pub kind: String,
    pub code: String,
}

/// The sub-budget of one category or account row of `parent`: that row's
/// accounts and every line in them, and only the setup those lines use.
///
/// What carries over, exactly:
/// - **The row**: the category with all its accounts, or the account under
///   its own category (the category's other accounts left out); every line,
///   subtotal, adjustment and separator in them, with their notes, fringes,
///   groups, sets, locations and units. Production total rows stay in place.
/// - **The setup those lines use**, and what that setup needs in turn: the
///   fringes, globals (a global another kept global names comes too),
///   units, groups, sets, locations and foreign currencies. Anything no
///   kept line uses is left out, with its place in a folder, its hand-made
///   order and where it came from.
/// - **The budget settings and details**: base and totals currency,
///   arithmetic, fringe posting, the fringe row, cutoff factor, account
///   numbering, new-line defaults, group rules, rounding, budget info and
///   display settings, and the overtime patterns.
///
/// Left out: every other category and account, charges and credits (they
/// work on the whole budget), tag, line and report presets, saved versions,
/// the baseline and actuals. So the row keeps the totals it has in `parent`
/// (a fringe cutoff shared with lines outside the row is the exception:
/// each line is counted only with the lines kept).
///
/// Titled `<parent title> - <row name>` and stamped as a sub-budget of
/// `parent`, its note naming the row.
pub fn make_row_sub_budget(parent: &Budget, row: &RowInput, saved: &str) -> EditResult<Budget> {
    let saved = check_created(saved)?;
    let code = row.code.trim();
    let mut spec = SubBudgetSpec::default();
    let (what, name) = match row.kind.as_str() {
        "category" => {
            let c = parent
                .categories()
                .find(|c| c.code == code)
                .ok_or_else(|| format!("There is no category {code}."))?;
            spec.categories.insert(c.code.clone());
            ("category", c.name.trim().to_string())
        }
        "account" => {
            let a = parent
                .categories()
                .flat_map(|c| &c.accounts)
                .find(|a| a.code == code)
                .ok_or_else(|| format!("There is no account {code}."))?;
            spec.accounts.insert(a.code.clone());
            ("account", a.name.trim().to_string())
        }
        other => {
            return Err(format!(
                "A sub-budget starts from a category or an account, not \"{other}\"."
            ))
        }
    };
    let mut b = subbudget::sub_budget(parent, &spec).map_err(|e| e.to_string())?;
    b.charges.clear();
    b.credits.clear();
    b.tag_presets.clear();
    b.line_presets.clear();
    b.report_presets.clear();
    share::prune_unused_setup(&mut b);
    b.setup_order = method_core::order::normalized_all(&b);
    b.setup_folders = method_core::folders::normalized(&b);
    b.setup_sources = method_core::pack::normalized(&b);
    let parent_title = match parent.title.trim() {
        "" => UNTITLED,
        t => t,
    };
    let row_name = if name.is_empty() {
        code.to_string()
    } else {
        format!("{code} {name}")
    };
    b.title = format!(
        "{parent_title} - {}",
        if name.is_empty() { code } else { &name }
    );
    b.provenance = Some(Provenance {
        kind: ProvenanceKind::SubBudget,
        source_title: parent.title.trim().to_string(),
        source_version: None,
        saved,
        app_version: APP_VERSION.to_string(),
        note: Some(format!("From {what} {row_name}")),
        pack: None,
    });
    Ok(b)
}

/// Opens the sub-budget of row `row` of budget `doc` (`None`: the one in
/// front) in a new tab, in front, with no file; its id. (The desktop saves
/// it beside its parent's file instead, src/sharing.rs.)
pub fn row_sub_budget_new(
    state: &AppState,
    doc: Option<DocId>,
    row: &RowInput,
    saved: &str,
) -> Result<DocId, String> {
    let b = {
        let s = state.lock_doc(doc)?;
        make_row_sub_budget(s.budget(), row, saved)?
    };
    Ok(state.docs().add(Session::with_budget(b, None)))
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/sharing.rs and the host's src/host/sharing.rs
// call these)

/// The open budget copy `doc` came from, for the page's offer.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct OriginalDto {
    pub doc: DocId,
    /// Its tab's label.
    pub label: String,
}

/// The open budget copy `doc` came from, if one is open.
pub fn share_copy_original(docs: &Docs, doc: DocId) -> Option<OriginalDto> {
    original_of(docs, doc).and_then(|o| {
        docs.get(o).map(|s| OriginalDto {
            doc: o,
            label: s.tab_label(),
        })
    })
}

/// Copy `doc` (`None`: the one in front) as it is, to store as a version of
/// open budget `original`. Refused when they are the same budget.
pub fn copy_to_store(
    state: &AppState,
    doc: Option<DocId>,
    original: DocId,
) -> Result<Budget, String> {
    let s = state.lock_doc(doc)?;
    if s.id() == original {
        return Err("A budget can't be stored as a version of itself.".into());
    }
    Ok(s.budget().clone())
}

/// Stores `copy` as a new version of budget `original`, named `name` (one
/// undo step in `original`).
pub fn share_copy_store(
    original: &mut Session,
    copy: &Budget,
    name: &str,
    created: &str,
) -> EditResult<VersionId> {
    let label = format!("Store a returned copy as version \"{}\"", name.trim());
    original.edit(label, |b| store_as_version(b, copy, name, created))
}

/// Opens a sub-budget of budget `doc` (`None`: the one in front) in a new
/// tab, in front, with no file; its id.
pub fn sub_budget_new(
    state: &AppState,
    doc: Option<DocId>,
    spec: SubBudgetInput,
    saved: &str,
) -> Result<DocId, String> {
    let b = {
        let s = state.lock_doc(doc)?;
        make_sub_budget(s.budget(), spec, saved)?
    };
    Ok(state.docs().add(Session::with_budget(b, None)))
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
        assert_eq!(e, read_only());
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

    /// The starter as "Big Show", with a version, a tag preset, an unused
    /// global and a group on one Camera line, to see what a row's
    /// sub-budget carries.
    fn row_parent() -> Budget {
        let mut b = show();
        b.globals.insert("UNUSED".into(), "3".into());
        b.groups.insert(
            "Night".into(),
            method_core::Group {
                description: String::new(),
                short_code: None,
                in_budget_total: true,
                color: None,
            },
        );
        let cam = b.categories_mut().find(|c| c.code == "21").unwrap();
        cam.accounts[0].details[0].groups.insert("Night".into());
        b.tag_presets.push(method_core::presets::TagPreset {
            name: "Nights".into(),
            groups: ["Night".to_string()].into(),
            ..Default::default()
        });
        b
    }

    fn keys<V>(m: &std::collections::BTreeMap<String, V>) -> Vec<&str> {
        m.keys().map(String::as_str).collect()
    }

    fn row(kind: &str, code: &str) -> RowInput {
        RowInput {
            kind: kind.into(),
            code: code.into(),
        }
    }

    #[test]
    fn a_rows_sub_budget_carries_its_accounts_lines_and_the_setup_they_use() {
        let parent = row_parent();
        let b = make_row_sub_budget(&parent, &row("category", "21"), AT).unwrap();
        assert_eq!(b.title, "Big Show - Camera");
        let p = b.provenance.as_ref().unwrap();
        assert_eq!(p.kind, ProvenanceKind::SubBudget);
        assert_eq!(p.source_title, "Big Show");
        assert_eq!(p.note.as_deref(), Some("From category 21 Camera"));
        // The row, whole.
        let cats: Vec<&str> = b.categories().map(|c| c.code.as_str()).collect();
        assert_eq!(cats, ["21"]);
        let parent_cam = parent.categories().find(|c| c.code == "21").unwrap();
        let cam = b.categories().next().unwrap();
        assert_eq!(cam.accounts, parent_cam.accounts, "every line as it was");
        // Exactly the setup its lines use, and what that needs.
        assert_eq!(keys(&b.fringes), ["PAYROLL"]);
        assert_eq!(keys(&b.globals), ["SHOOT_DAYS", "SHOOT_WEEKS"]);
        assert_eq!(keys(&b.groups), ["Night"]);
        let mut units = keys(&b.units);
        units.sort_unstable();
        assert_eq!(units, ["Week"]);
        // The budget settings and details come along.
        assert_eq!(b.base_currency, parent.base_currency);
        assert_eq!(b.fringe_posting, parent.fringe_posting);
        assert_eq!(b.arithmetic, parent.arithmetic);
        assert_eq!(b.account_coding, parent.account_coding);
        assert_eq!(b.info, parent.info);
        // What works on the whole budget stays behind.
        assert!(b.charges.is_empty() && b.credits.is_empty());
        assert!(b.tag_presets.is_empty() && b.report_presets.is_empty());
        assert!(b.line_presets.is_empty());
        assert!(b.versions.is_empty() && b.baseline.is_none());
        // It round-trips through the file format.
        let text = method_format::to_string(&b);
        assert_eq!(method_format::from_str(&text).unwrap(), b);

        // A row with no fringes or globals brings none.
        let ins = make_row_sub_budget(&parent, &row("category", "50"), AT).unwrap();
        assert!(ins.fringes.is_empty() && ins.globals.is_empty() && ins.groups.is_empty());
        assert_eq!(ins.title, "Big Show - Insurance");
    }

    #[test]
    fn a_rows_sub_budget_keeps_the_rows_totals() {
        let parent = row_parent();
        let opts = method_core::RecalcOptions::default();
        let whole = method_core::recalc(&parent, &opts).unwrap();
        for cat in &whole.categories {
            let b = make_row_sub_budget(&parent, &row("category", &cat.code), AT).unwrap();
            let r = method_core::recalc(&b, &opts).unwrap();
            assert_eq!(r.categories.len(), 1);
            let mine = &r.categories[0];
            assert_eq!(mine.total, cat.total, "category {}", cat.code);
            assert_eq!(mine.fringe_total, cat.fringe_total, "category {}", cat.code);
            for (a, want) in mine.accounts.iter().zip(&cat.accounts) {
                assert_eq!((a.code.as_str(), a.total), (want.code.as_str(), want.total));
            }
            // With no charges or credits, its bottom line is the row and
            // the fringes it earns.
            assert_eq!(r.charges.len(), 0);
            assert_eq!(r.net_total, r.grand_total);
            for acct in &cat.accounts {
                let b = make_row_sub_budget(&parent, &row("account", &acct.code), AT).unwrap();
                let r = method_core::recalc(&b, &opts).unwrap();
                let only: Vec<&str> = r.categories[0]
                    .accounts
                    .iter()
                    .map(|a| a.code.as_str())
                    .collect();
                assert_eq!(only, [acct.code.as_str()]);
                assert_eq!(r.categories[0].accounts[0].total, acct.total);
                assert_eq!(r.categories[0].accounts[0].fringe_total, acct.fringe_total);
            }
        }
    }

    #[test]
    fn a_rows_sub_budget_refuses_what_isnt_there() {
        let parent = row_parent();
        assert!(make_row_sub_budget(&parent, &row("category", "99"), AT).is_err());
        assert!(make_row_sub_budget(&parent, &row("account", "9999"), AT).is_err());
        assert!(make_row_sub_budget(&parent, &row("line", "21"), AT).is_err());
        assert!(make_row_sub_budget(&parent, &row("category", "21"), "today").is_err());
        // Opened in a new tab with no file (the browser's way).
        let state = AppState::with(Docs::with(Session::with_budget(parent, None)));
        let id = row_sub_budget_new(&state, None, &row("account", "2101"), AT).unwrap();
        let docs = state.docs();
        let s = docs.get(id).unwrap();
        assert!(s.path().is_none());
        assert_eq!(s.budget().title, "Big Show - Camera");
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
