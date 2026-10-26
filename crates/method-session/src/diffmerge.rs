//! What Changed and Merge Changes From (roadmap 3.1 and 3.2; decisions.md,
//! entry 44). The engine does the work (`method_core::diff`,
//! `method_core::merge`); this gathers the budgets, totals them the way their
//! grids do, and turns the results into what the page draws
//! (ui/js/diffmerge.js).
//!
//! - **What Changed…** lists what changed from another budget to the one in
//!   front: an open tab, a saved version of it, a file picked in the host's
//!   Open dialog (by token, as for comparing), or its own file as last saved
//!   (which the host reads). Each change says where it is in the budget in front, so a click
//!   goes there.
//! - **Merge Changes From…** merges a returned copy (an open tab or a file)
//!   into the budget in front, against a base: found from the copy's
//!   provenance (the version it was shared from, worked out as it was sent:
//!   `method_core::merge::base_as_shared`), or a version, an open tab or a file
//!   the user picks. The preview lists what the merge brings in and every
//!   conflict; picks go back as `{key: "ours" | "theirs"}` and the merge is
//!   worked out again. Applying is one undo step, worked out on the budget
//!   in front as it is then.
//!
//! Paths never come from the page; budgets are copied out under the lock and
//! compared after it, as for comparing (src/compare.rs). Picked files are
//! kept with the files picked for comparing ([`CompareFiles`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use method_core::diff::{diff_with, BudgetDiff, DiffItem, MoneyLine, Place, Table};
use method_core::merge::{base_as_shared, merge_with, Merged, Side};
use method_core::{Budget, Report, RowId, VersionId};
use serde::{Deserialize, Serialize};

use crate::compare::{CompareFiles, Picked};
use crate::diagnose::lenient_recalc;
use crate::docs::{AppState, DocGuard, DocId, Docs};
use crate::host::FileIn;
use crate::messages::{msg, word};

/// Most picks a merge takes (one per conflict).
const MAX_PICKS: usize = 50_000;
/// Longest conflict key the page may send back.
const MAX_KEY: usize = 400;

/// What the budget in front is compared with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Against {
    /// An open budget.
    Tab { doc: DocId },
    /// A version saved in the budget in front.
    Version { id: VersionId },
    /// A file picked with `merge_pick_file`.
    File { token: u64 },
    /// The budget's own file, as last saved.
    Saved,
}

/// Where a returned copy comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CopySource {
    Tab { doc: DocId },
    File { token: u64 },
}

/// The base of a merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BaseSource {
    /// From the copy's provenance: the version of the budget in front it
    /// was shared from.
    Auto,
    Version {
        id: VersionId,
    },
    Tab {
        doc: DocId,
    },
    File {
        token: u64,
    },
}

// ---------------------------------------------------------------------------
// What the page draws

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SideDto {
    pub label: String,
    /// A path, or a version's time.
    pub detail: String,
    /// Some cells have problems; their money counts as blank.
    pub partial: bool,
}

/// Where a click goes in the budget in front (main.js `jumpTo`, or a setup
/// tab).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GoDto {
    /// `"category"` (the topsheet, on a category), `"account"` (an account
    /// or a row), `"setup"` (a setup tab: `tab`), `"versions"`.
    pub kind: &'static str,
    pub ti: Option<usize>,
    pub ai: Option<usize>,
    pub category: Option<String>,
    pub account: Option<String>,
    pub row_id: Option<RowId>,
    pub field: Option<&'static str>,
    pub tab: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FieldDto {
    pub field: &'static str,
    pub label: &'static str,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChangeDto {
    /// `"added"`, `"removed"`, `"changed"`, `"moved"`, `"renamed"`,
    /// `"renumbered"`.
    pub kind: &'static str,
    /// The heading it goes under: "Budget", "Fringes", "Topsheet",
    /// "21-01 Camera", "Versions".
    pub group: String,
    /// `"budget"`, `"setup"`, `"topsheet"`, `"account"`, `"versions"`.
    pub group_kind: &'static str,
    pub subject: String,
    pub text: String,
    pub money: Option<i64>,
    pub fields: Vec<FieldDto>,
    pub go: Option<GoDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MoneyDto {
    pub label: String,
    pub before: Option<i64>,
    pub after: Option<i64>,
    pub delta: i64,
    pub go: Option<GoDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WhatChangedDto {
    /// The older budget.
    pub from: SideDto,
    /// The budget in front.
    pub to: SideDto,
    pub changes: Vec<ChangeDto>,
    pub accounts: Vec<MoneyDto>,
    pub categories: Vec<MoneyDto>,
    pub totals: Vec<MoneyDto>,
    pub money_known: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConflictDto {
    pub key: String,
    pub group: String,
    pub what: String,
    pub base: Option<String>,
    pub ours: String,
    pub theirs: String,
    /// `"ours"` or `"theirs"`.
    pub taken: &'static str,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MergeDto {
    pub copy: SideDto,
    /// The base used, when there is one.
    pub base: Option<SideDto>,
    /// Why there is no base yet: the page asks for one.
    pub needs_base: Option<String>,
    /// Whether the base was worked out as the copy was sent.
    pub as_sent: bool,
    pub conflicts: Vec<ConflictDto>,
    /// What the merge changes in the budget in front.
    pub changes: Vec<ChangeDto>,
    pub totals: Vec<MoneyDto>,
    pub renumbered: usize,
    pub problem: Option<String>,
}

fn tab_of(table: Table) -> &'static str {
    match table {
        Table::Info => "info",
        Table::Global => "globals",
        Table::Fringe => "fringes",
        Table::Group => "groups",
        Table::Unit => "units",
        Table::Currency => "currencies",
        Table::Set => "sets",
        Table::Location => "locations",
        Table::Charge => "charges",
        Table::Credit => "credits",
    }
}

/// Row fields that are grid columns, to land on.
const GRID_FIELDS: [&str; 14] = [
    "description",
    "amount",
    "unit",
    "x",
    "unit2",
    "rate",
    "unit3",
    "currency",
    "x4",
    "unit4",
    "fringes",
    "groups",
    "set",
    "location",
];

/// Where `place` is in the budget in front, if it is there.
fn go_of(place: &Place, field: Option<&'static str>) -> Option<GoDto> {
    let blank = GoDto {
        kind: "account",
        ti: None,
        ai: None,
        category: None,
        account: None,
        row_id: None,
        field: None,
        tab: None,
    };
    match place {
        Place::Budget => Some(GoDto {
            kind: "setup",
            tab: Some(if field == Some("baseline") {
                "baseline"
            } else {
                "settings"
            }),
            ..blank
        }),
        Place::Setup { table, .. } => Some(GoDto {
            kind: "setup",
            tab: Some(tab_of(*table)),
            ..blank
        }),
        Place::Topsheet {
            code: Some(code),
            ti: Some(ti),
            ..
        } => Some(GoDto {
            kind: "category",
            ti: Some(*ti),
            category: Some(code.clone()),
            ..blank
        }),
        Place::Topsheet { .. } => None,
        Place::Account {
            code,
            category,
            at: Some((ti, ai)),
            row,
            ..
        } => Some(GoDto {
            kind: "account",
            ti: Some(*ti),
            ai: Some(*ai),
            category: Some(category.clone()),
            account: Some(code.clone()),
            row_id: *row,
            field: row.and(field).filter(|f| GRID_FIELDS.contains(f)),
            ..blank
        }),
        Place::Account { .. } => None,
        Place::Versions => Some(GoDto {
            kind: "versions",
            ..blank
        }),
    }
}

fn group_kind(place: &Place) -> &'static str {
    match place {
        Place::Budget => "budget",
        Place::Setup { .. } => "setup",
        Place::Topsheet { .. } => "topsheet",
        Place::Account { .. } => "account",
        Place::Versions => "versions",
    }
}

fn change_dto(i: &DiffItem, rows_in_front: &dyn Fn(RowId) -> bool) -> ChangeDto {
    // A row that went has no place to go to: its account instead.
    let place = match &i.place {
        Place::Account {
            row: Some(r),
            category,
            code,
            name,
            at,
            line,
        } if !rows_in_front(*r) => Place::Account {
            category: category.clone(),
            code: code.clone(),
            name: name.clone(),
            at: *at,
            row: None,
            line: *line,
        },
        p => p.clone(),
    };
    let field = i.fields.first().map(|f| f.field);
    ChangeDto {
        kind: i.kind.key(),
        group: i.place.group(),
        group_kind: group_kind(&i.place),
        subject: i.subject.clone(),
        text: i.text.clone(),
        money: i.money,
        fields: i
            .fields
            .iter()
            .map(|f| FieldDto {
                field: f.field,
                label: f.label,
                before: f.before.clone(),
                after: f.after.clone(),
            })
            .collect(),
        go: go_of(&place, field),
    }
}

fn money_dto(l: &MoneyLine) -> MoneyDto {
    MoneyDto {
        label: l.label.clone(),
        before: l.before,
        after: l.after,
        delta: l.delta,
        go: l.place.as_ref().and_then(|p| go_of(p, None)),
    }
}

fn changes_of(d: &BudgetDiff, front: &Budget) -> Vec<ChangeDto> {
    let ids: std::collections::HashSet<RowId> = front
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .map(|d| d.id)
        .collect();
    d.items
        .iter()
        .map(|i| change_dto(i, &|r| ids.contains(&r)))
        .collect()
}

// ---------------------------------------------------------------------------
// Gathering

/// A budget copied out, with its totals as its grid works them out once
/// known.
struct Held {
    side: SideDto,
    budget: Arc<Budget>,
    report: Option<Report>,
    totalled: bool,
}

impl Held {
    fn raw(label: String, detail: String, budget: Arc<Budget>) -> Held {
        Held {
            side: SideDto {
                label,
                detail,
                partial: false,
            },
            budget,
            report: None,
            totalled: false,
        }
    }

    /// Totals it (cells with problems count as blank), outside the lock.
    fn total(&mut self) {
        if self.totalled {
            return;
        }
        let o = lenient_recalc(&self.budget);
        self.side.partial = o.partial();
        self.report = o.report;
        self.totalled = true;
    }
}

/// An open budget with the totals its session keeps.
fn held_open(docs: &mut Docs, doc: DocId) -> Result<Held, String> {
    let s = docs
        .get_mut(doc)
        .ok_or_else(|| word("engine.docs.closed"))?;
    let label = s.tab_label();
    let detail = s
        .path()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let (b, o) = s.totals();
    Ok(Held {
        side: SideDto {
            label,
            detail,
            partial: o.partial(),
        },
        budget: Arc::new(b.clone()),
        report: o.report.clone(),
        totalled: true,
    })
}

fn held_file(files: &CompareFiles, token: u64) -> Result<Held, String> {
    let (label, path, b) = files
        .get(token)
        .ok_or_else(|| word("engine.merge.file_gone"))?;
    Ok(Held::raw(label, path, b))
}

fn held_version(docs: &Docs, front: DocId, id: VersionId) -> Result<Held, String> {
    let s = docs
        .get(front)
        .ok_or_else(|| word("engine.merge.budget_closed"))?;
    let v = s
        .budget()
        .version(id)
        .ok_or_else(|| word("engine.merge.version_deleted"))?;
    Ok(Held::raw(
        msg("engine.merge.version_label", &[("name", &v.name)]),
        v.created.clone(),
        Arc::new(v.budget.to_budget()),
    ))
}

/// What the older side is, before anything is read from disk.
enum Older {
    Held(Box<Held>),
    /// The budget's own file, read by the host after letting go of the lock.
    Saved(PathBuf),
}

/// Copies out budget `front` and what it is compared with. Call it under
/// the app's lock, and [`Gathered::diff`] after letting go.
pub fn gather_diff(
    docs: &mut Docs,
    files: &CompareFiles,
    front: DocId,
    against: Against,
) -> Result<Gathered, String> {
    let to = held_open(docs, front)?;
    let from = match against {
        Against::Tab { doc } if doc == front => return Err(word("engine.merge.same_budget")),
        Against::Tab { doc } => Older::Held(Box::new(held_open(docs, doc)?)),
        Against::Version { id } => {
            // A version holds content only: its versions and actuals are
            // the budget's, so they aren't listed as changes.
            let mut h = held_version(docs, front, id)?;
            let mut b = (*h.budget).clone();
            b.versions = to.budget.versions.clone();
            b.actuals = Arc::clone(&to.budget.actuals);
            h.budget = Arc::new(b);
            Older::Held(Box::new(h))
        }
        Against::File { token } => Older::Held(Box::new(held_file(files, token)?)),
        Against::Saved => {
            let s = docs
                .get(front)
                .ok_or_else(|| word("engine.merge.budget_closed"))?;
            let path = s.path().ok_or_else(|| word("engine.merge.never_saved"))?;
            Older::Saved(path.to_path_buf())
        }
    };
    Ok(Gathered { from, to })
}

pub struct Gathered {
    from: Older,
    to: Held,
}

impl Gathered {
    /// What changed from the older budget to the one in front.
    /// `read_saved` reads the budget's own file as last saved, for
    /// [`Against::Saved`].
    pub fn diff(
        self,
        read_saved: impl FnOnce(&Path) -> Result<Budget, String>,
    ) -> Result<WhatChangedDto, String> {
        let mut from = match self.from {
            Older::Held(h) => *h,
            Older::Saved(path) => {
                let b = read_saved(&path)?;
                Held::raw(
                    word("engine.merge.saved_file"),
                    path.display().to_string(),
                    Arc::new(b),
                )
            }
        };
        from.total();
        let to = self.to;
        let d = diff_with(
            &from.budget,
            from.report.as_ref(),
            &to.budget,
            to.report.as_ref(),
        );
        Ok(WhatChangedDto {
            changes: changes_of(&d, &to.budget),
            accounts: d.accounts.iter().map(money_dto).collect(),
            categories: d.categories.iter().map(money_dto).collect(),
            totals: d.totals.iter().map(money_dto).collect(),
            money_known: d.money_known,
            from: from.side,
            to: to.side,
        })
    }
}

/// The budgets of a merge, copied out.
pub struct MergeInputs {
    ours: Held,
    copy: Held,
    base: Option<Held>,
    as_sent: bool,
    needs_base: Option<String>,
}

/// The version `copy` was shared from, if it came from `ours`.
fn shared_from(ours: &Budget, copy: &Budget) -> Result<(VersionId, String), String> {
    let p = copy
        .provenance
        .as_ref()
        .ok_or_else(|| word("engine.merge.choose_base"))?;
    if !method_core::share::came_from(copy, ours) {
        return Err(msg(
            "engine.merge.other_budget",
            &[("title", &p.source_title)],
        ));
    }
    let Some(name) = &p.source_version else {
        return Err(word("engine.merge.not_from_version"));
    };
    ours.versions
        .iter()
        .rev()
        .find(|v| v.name == *name)
        .map(|v| (v.id, v.name.clone()))
        .ok_or_else(|| msg("engine.merge.version_gone", &[("name", name)]))
}

/// Copies out the budget in front, the returned copy and the base. Call it
/// under the app's lock, and [`MergeInputs::merge`] after letting go.
pub fn gather_merge(
    docs: &mut Docs,
    files: &CompareFiles,
    front: DocId,
    copy: CopySource,
    base: BaseSource,
) -> Result<MergeInputs, String> {
    let ours = held_open(docs, front)?;
    let copy = match copy {
        CopySource::Tab { doc } if doc == front => return Err(word("engine.merge.from_itself")),
        CopySource::Tab { doc } => held_open(docs, doc)?,
        CopySource::File { token } => held_file(files, token)?,
    };
    // A copy saved for sharing: its base is worked out as it was sent.
    let shared = copy.budget.provenance.is_some();
    let (base, as_sent, needs_base) = match base {
        BaseSource::Auto => match shared_from(&ours.budget, &copy.budget) {
            Ok((id, name)) => {
                let mut h = held_version(docs, front, id)?;
                h.side.label = msg("engine.merge.version_shared_label", &[("name", &name)]);
                (Some(h), true, None)
            }
            Err(why) => (None, false, Some(why)),
        },
        BaseSource::Version { id } => (Some(held_version(docs, front, id)?), shared, None),
        BaseSource::Tab { doc } if doc == front => return Err(word("engine.merge.front_not_base")),
        BaseSource::Tab { doc } => (Some(held_open(docs, doc)?), false, None),
        BaseSource::File { token } => (Some(held_file(files, token)?), false, None),
    };
    Ok(MergeInputs {
        ours,
        copy,
        base,
        as_sent,
        needs_base,
    })
}

/// The page's picks: conflict key to `"ours"` or `"theirs"`.
fn read_picks(picks: &BTreeMap<String, String>) -> Result<BTreeMap<String, Side>, String> {
    if picks.len() > MAX_PICKS {
        return Err(word("engine.merge.too_many_choices"));
    }
    picks
        .iter()
        .map(|(k, v)| {
            if k.len() > MAX_KEY {
                return Err(word("engine.merge.not_a_conflict"));
            }
            let side =
                Side::from_key(v).ok_or_else(|| msg("engine.merge.not_a_side", &[("side", v)]))?;
            Ok((k.clone(), side))
        })
        .collect()
}

fn conflict_dto(c: &method_core::merge::Conflict) -> ConflictDto {
    ConflictDto {
        key: c.key.clone(),
        group: c.place.group(),
        what: c.what.clone(),
        base: c.base.clone(),
        ours: c.ours.clone(),
        theirs: c.theirs.clone(),
        taken: c.taken.key(),
        text: c.text.clone(),
    }
}

/// The merged budget of `ours`, `copy` and `base` with `picks`, the base
/// worked out as the copy was sent when `as_sent`.
fn merged(
    ours: &Budget,
    copy: &Budget,
    base: &Budget,
    as_sent: bool,
    picks: &BTreeMap<String, Side>,
) -> Merged {
    if as_sent {
        let sent = base_as_shared(base, ours, copy);
        merge_with(&sent, ours, copy, Side::Ours, picks)
    } else {
        merge_with(base, ours, copy, Side::Ours, picks)
    }
}

impl MergeInputs {
    /// The preview: what the merge brings in and every conflict.
    pub fn merge(mut self, picks: &BTreeMap<String, String>) -> Result<MergeDto, String> {
        let picks = read_picks(picks)?;
        let Some(base) = self.base.as_mut() else {
            return Ok(MergeDto {
                copy: self.copy.side,
                base: None,
                needs_base: self.needs_base,
                as_sent: false,
                conflicts: Vec::new(),
                changes: Vec::new(),
                totals: Vec::new(),
                renumbered: 0,
                problem: None,
            });
        };
        let m = merged(
            &self.ours.budget,
            &self.copy.budget,
            &base.budget,
            self.as_sent,
            &picks,
        );
        let after = lenient_recalc(&m.budget);
        let d = diff_with(
            &self.ours.budget,
            self.ours.report.as_ref(),
            &m.budget,
            after.report.as_ref(),
        );
        Ok(MergeDto {
            copy: self.copy.side,
            base: self.base.map(|b| b.side),
            needs_base: None,
            as_sent: self.as_sent,
            conflicts: m.conflicts.iter().map(conflict_dto).collect(),
            // Where things are after the merge is where the page goes.
            changes: changes_of(&d, &m.budget),
            totals: d.totals.iter().map(money_dto).collect(),
            renumbered: m.renumbered,
            problem: m.problem,
        })
    }

    /// The copy's label, for the undo step.
    fn copy_label(&self) -> String {
        self.copy.side.label.clone()
    }
}

// ---------------------------------------------------------------------------
// Commands

/// What changed from `against` to budget `doc` (`None`: the one in front).
/// `read_saved` reads the budget's own file as last saved.
pub fn what_changed(
    state: &AppState,
    files: &CompareFiles,
    doc: Option<DocId>,
    against: Against,
    read_saved: impl FnOnce(&Path) -> Result<Budget, String>,
) -> Result<WhatChangedDto, String> {
    let gathered = {
        let front = state.lock_doc(doc)?.id();
        gather_diff(&mut state.docs(), files, front, against)?
    };
    gathered.diff(read_saved)
}

/// The merge of copy `copy` into budget `doc` (`None`: the one in front)
/// against `base`, with `picks` for its conflicts: a preview, nothing
/// changes.
pub fn merge_preview(
    state: &AppState,
    files: &CompareFiles,
    doc: Option<DocId>,
    copy: CopySource,
    base: BaseSource,
    picks: Option<BTreeMap<String, String>>,
) -> Result<MergeDto, String> {
    let inputs = {
        let front = state.lock_doc(doc)?.id();
        gather_merge(&mut state.docs(), files, front, copy, base)?
    };
    inputs.merge(&picks.unwrap_or_default())
}

/// Merges copy `copy` into budget `doc` against `base` with `picks`, as one
/// undo step. The merge is worked out on the budget as it is now. The
/// budget's lock, for its view.
pub fn merge_apply<'a>(
    state: &'a AppState,
    files: &CompareFiles,
    doc: Option<DocId>,
    copy: CopySource,
    base: BaseSource,
    picks: Option<BTreeMap<String, String>>,
) -> Result<DocGuard<'a>, String> {
    let picks = read_picks(&picks.unwrap_or_default())?;
    let (front, inputs) = {
        let front = state.lock_doc(doc)?.id();
        let inputs = gather_merge(&mut state.docs(), files, front, copy, base)?;
        (front, inputs)
    };
    let Some(base) = inputs.base.as_ref().map(|b| Arc::clone(&b.budget)) else {
        return Err(inputs
            .needs_base
            .clone()
            .unwrap_or_else(|| word("engine.merge.choose_base_first")));
    };
    let label = msg(
        "engine.undo.merge_changes",
        &[("source", &inputs.copy_label())],
    );
    let copy = Arc::clone(&inputs.copy.budget);
    let as_sent = inputs.as_sent;
    let mut s = state.lock_doc(Some(front))?;
    s.edit(label, move |b| {
        let m = merged(b, &copy, &base, as_sent, &picks);
        *b = m.budget;
        Ok(())
    })?;
    Ok(s)
}

/// The Open dialog's title for picking one budget (`purpose`: `"diff"` to
/// see what changed, `"copy"` a returned copy to merge from, `"base"` the
/// base).
pub fn pick_title(purpose: &str) -> Result<&'static str, String> {
    Ok(match purpose {
        "diff" => "What changed since this budget",
        "copy" => "Merge changes from a returned copy",
        "base" => "Choose the base: the budget both copies started from",
        _ => return Err("Unknown file choice.".into()),
    })
}

/// Keeps `file`, a budget picked for `purpose` (as [`pick_title`] names
/// them), by token.
pub fn merge_pick_file(
    files: &CompareFiles,
    purpose: &str,
    file: &FileIn,
) -> Result<Option<Picked>, String> {
    pick_title(purpose)?;
    Ok(
        crate::compare::pick_files(files, std::slice::from_ref(file))?
            .into_iter()
            .next(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use method_core::share::{share_copy, ShareOptions};

    const AT: &str = "2026-10-09T14:30:00-07:00";

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

    fn set_rate(b: &mut Budget, id: RowId, rate: &str) {
        let d = b
            .categories_mut()
            .flat_map(|c| c.accounts.iter_mut())
            .flat_map(|a| a.details.iter_mut())
            .find(|d| d.id == id)
            .unwrap();
        d.rate = rate.into();
    }

    #[test]
    fn what_changed_against_a_tab_and_a_version() {
        let mut base = Session::new();
        base.edit("Title", |b| crate::edit::set_title(b, "Feature"))
            .unwrap();
        let (row, code) = first_line(base.budget());
        base.edit("Version", |b| {
            method_core::save_version(b, "Draft", AT, None);
            Ok(())
        })
        .unwrap();
        let vid = base.budget().versions[0].id;
        let mut other = Session::with_budget(base.budget().clone(), None);
        other.set_detail_field(row, "rate", "99999").unwrap();
        let mut docs = Docs::with(base);
        let front = docs.active();
        let tab = docs.add(other);
        let files = CompareFiles::default();

        // The tab (newer) against the front (older)? The front is newer:
        // what changed from the tab to the front is the rate going back.
        let d = gather_diff(&mut docs, &files, front, Against::Tab { doc: tab })
            .unwrap()
            .diff(|_| Err("no file".into()))
            .unwrap();
        let c = d
            .changes
            .iter()
            .find(|c| c.go.as_ref().and_then(|g| g.row_id) == Some(row))
            .unwrap();
        assert_eq!(c.kind, "changed");
        assert!(c.text.starts_with(&format!("{code} line ")), "{}", c.text);
        assert!(c.text.contains("rate 99,999.00 ->"), "{}", c.text);
        let go = c.go.as_ref().unwrap();
        assert_eq!(go.kind, "account");
        assert_eq!(go.account.as_deref(), Some(code.as_str()));
        assert_eq!(go.field, Some("rate"));
        assert_eq!(c.group_kind, "account");
        assert!(d.money_known);
        assert!(d.totals.iter().any(|t| t.label == "Grand total"));

        // Against its own version: no change.
        let d = gather_diff(&mut docs, &files, front, Against::Version { id: vid })
            .unwrap()
            .diff(|_| Err("no file".into()))
            .unwrap();
        assert!(d.changes.is_empty(), "{:?}", d.changes);
        assert_eq!(d.from.label, "Version \"Draft\"");

        // Against itself, or a file never saved.
        assert!(gather_diff(&mut docs, &files, front, Against::Tab { doc: front }).is_err());
        assert!(gather_diff(&mut docs, &files, front, Against::Saved).is_err());
    }

    #[test]
    fn merging_a_returned_copy_finds_its_base_and_applies_picks() {
        let mut orig = Session::new();
        orig.edit("Title", |b| crate::edit::set_title(b, "Big Show"))
            .unwrap();
        orig.edit("Version", |b| {
            method_core::save_version(b, "Sent", AT, None);
            Ok(())
        })
        .unwrap();
        let vid = orig.budget().versions[0].id;
        let (row, _) = first_line(orig.budget());
        // The reviewer's copy, from the version, without notes.
        let opts = ShareOptions {
            drop_notes: true,
            ..Default::default()
        };
        let mut copy = share_copy(orig.budget(), Some(vid), &opts, AT, "0.0.1", None).unwrap();
        set_rate(&mut copy, row, "777");
        // Ours changed the same rate meanwhile: one conflict.
        orig.set_detail_field(row, "rate", "555").unwrap();
        let mut docs = Docs::with(orig);
        let front = docs.active();
        let tab = docs.add(Session::with_budget(copy, None));
        let files = CompareFiles::default();
        let m = gather_merge(
            &mut docs,
            &files,
            front,
            CopySource::Tab { doc: tab },
            BaseSource::Auto,
        )
        .unwrap()
        .merge(&BTreeMap::new())
        .unwrap();
        assert!(m.as_sent);
        assert!(m.base.as_ref().unwrap().label.contains("Sent"));
        assert_eq!(m.conflicts.len(), 1, "{:?}", m.conflicts);
        assert_eq!(m.conflicts[0].taken, "ours");
        assert!(m.changes.is_empty(), "ours kept: nothing changes");
        let key = m.conflicts[0].key.clone();
        let picks = BTreeMap::from([(key, "theirs".to_string())]);
        let m = gather_merge(
            &mut docs,
            &files,
            front,
            CopySource::Tab { doc: tab },
            BaseSource::Auto,
        )
        .unwrap()
        .merge(&picks)
        .unwrap();
        assert_eq!(m.conflicts[0].taken, "theirs");
        assert_eq!(m.changes.len(), 1);
        assert!(
            m.changes[0].text.contains("rate 555.00 -> 777.00"),
            "{}",
            m.changes[0].text
        );
        // Bad picks are refused.
        let bad = BTreeMap::from([("x".to_string(), "mine".to_string())]);
        assert!(gather_merge(
            &mut docs,
            &files,
            front,
            CopySource::Tab { doc: tab },
            BaseSource::Auto
        )
        .unwrap()
        .merge(&bad)
        .is_err());
        // A copy with no provenance needs a base.
        let plain = docs.add(Session::new());
        let m = gather_merge(
            &mut docs,
            &files,
            front,
            CopySource::Tab { doc: plain },
            BaseSource::Auto,
        )
        .unwrap()
        .merge(&BTreeMap::new())
        .unwrap();
        assert!(m.needs_base.is_some() && m.base.is_none());
        assert!(gather_merge(
            &mut docs,
            &files,
            front,
            CopySource::Tab { doc: front },
            BaseSource::Auto
        )
        .is_err());
    }

    #[test]
    fn a_returned_copy_merges_through_the_host() {
        use serde_json::json;
        let h = crate::host::Host::new();
        let call =
            |c: &str, a: serde_json::Value| h.dispatch(c, a).unwrap_or_else(|e| panic!("{c}: {e}"));
        call("set_title", json!({"title": "Big Show"}));
        let v = call(
            "save_version",
            json!({"name": "Sent", "note": null, "created": AT}),
        );
        let vid: VersionId =
            serde_json::from_value(v["budget"]["versions"][0]["id"].clone()).unwrap();
        let ours = h.state().lock().budget().clone();
        let (row, _) = first_line(&ours);
        let mut copy = share_copy(
            &ours,
            Some(vid),
            &ShareOptions::default(),
            AT,
            "0.0.1",
            None,
        )
        .unwrap();
        set_rate(&mut copy, row, "777");
        h.state()
            .lock()
            .set_detail_field(row, "rate", "555")
            .unwrap();

        // The page sends the picked file as it sends any argument.
        let text = method_format::to_string(&copy);
        let file = crate::host::test_files::file("Returned.mthd", text.as_bytes());
        let p = call("merge_pick_file", json!({"purpose": "copy", "file": file}));
        assert_eq!(p["label"], "Returned");
        let copy_src = p["source"].clone();
        let base = json!({"kind": "auto"});
        let m = call("merge_preview", json!({"copy": copy_src, "base": base}));
        assert_eq!(m["as_sent"], true);
        let key = m["conflicts"][0]["key"].as_str().unwrap().to_string();
        let picks = json!({ key: "theirs" });
        let v = call(
            "merge_apply",
            json!({"copy": copy_src, "base": base, "picks": picks}),
        );
        assert!(v["rev"].is_number());
        let rate = |b: &Budget| {
            crate::edit::details(b)
                .find(|d| d.id == row)
                .map(|d| d.rate.0.clone())
        };
        assert_eq!(rate(h.state().lock().budget()).as_deref(), Some("777"));

        let d = call(
            "what_changed",
            json!({"against": {"kind": "version", "id": vid}}),
        );
        assert_eq!(d["from"]["label"], "Version \"Sent\"");
        let e = h
            .dispatch("what_changed", json!({"against": {"kind": "saved"}}))
            .unwrap_err();
        assert_eq!(e.message(), "This budget hasn't been saved to a file yet.");
        call("save_mthd", json!({"name": "Big Show.mthd"}));
        let e = h
            .dispatch("what_changed", json!({"against": {"kind": "saved"}}))
            .unwrap_err();
        assert!(e.message().contains("Choose it as a file"), "{e}");
        let e = h
            .dispatch("merge_pick_file", json!({"purpose": "x", "file": file}))
            .unwrap_err();
        assert_eq!(e.message(), "Unknown file choice.");
    }
}
