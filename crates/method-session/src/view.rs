//! What the UI renders.
//!
//! Two shapes, so a command's reply stays small however big the budget is
//! (see docs/APP.md, "What crosses the bridge"):
//!
//! - [`View`], after every command: the budget's outline in file-format JSON
//!   (setup tables, categories and accounts, but no detail rows), every total
//!   down to the account, calculation problems, file and undo state, and the
//!   one account on screen in full (its rows and their totals). A single-cell
//!   edit in a budget of thousands of lines sends tens of kilobytes, not
//!   megabytes.
//! - [`FullView`], on request (`get_full_view`, for printing): the whole budget
//!   and every row's totals, as the view used to be.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use method_core::{Budget, Cents, Detail, LinePreset, Report, RowId, TopsheetRow, Version};
use serde::Serialize;

use crate::diagnose::{error_message, Issue, Outcome, Target};
use crate::report_dto::{ReportDto, RowDto, VarianceDto};

/// The lean view, returned by every command that changes or shows the budget.
#[derive(Clone, Debug, Serialize)]
pub struct View {
    /// Changes whenever the budget does (an edit, undo, redo, opening or
    /// starting a budget); the same number means the same budget.
    pub rev: u64,
    /// The `budget` object as `docs/FORMAT.md` describes it, without the
    /// rows: each account has `lines` (how many rows it has) instead of
    /// `details`, the baseline has no `rows`, and `versions` lists each
    /// version's `id`, `name`, `created` and `note` only.
    pub budget: serde_json::Value,
    /// Totals down to each account (no per-row figures), or `None` when they
    /// can't be computed at all.
    pub report: Option<ReportDto>,
    /// True when `report` treats the flagged cells in `issues` as blank.
    pub partial: bool,
    /// The engine's first error, if any.
    pub error: Option<ErrorDto>,
    /// Problems with globals, fringes, charges and the like, and with the
    /// cells of `account`. Problems in other accounts are counted in
    /// `issue_count` and located by `issue_accounts`.
    pub issues: Vec<Issue>,
    /// How many problems there are in the whole budget.
    pub issue_count: usize,
    /// Accounts with a problem cell, as `[topsheet index, account index]`.
    pub issue_accounts: Vec<[usize; 2]>,
    pub file: FileDto,
    pub can_undo: bool,
    pub can_redo: bool,
    /// A row the last command created, for the UI to select.
    pub created_row: Option<RowId>,
    /// Original (baseline) vs current totals, when the budget has a baseline
    /// and totals could be computed. Rows of `account` only.
    pub variance: Option<VarianceDto>,
    /// The account on screen (see `focus_account`), in full.
    pub account: Option<AccountView>,
    /// Which open budget this is (src/docs.rs). The app fills this in, and
    /// `tabs`, after each command; a view made outside the app has 0 and no
    /// tabs.
    pub doc: crate::docs::DocId,
    /// Every open budget's tab, in tab order.
    pub tabs: Vec<crate::docs::TabInfo>,
    /// The window's panes: one, or two side by side (split view), each
    /// pane's tabs and the one in front, and which pane is the target.
    pub panes: Option<crate::docs::PanesInfo>,
    /// Cost figures (actual, estimate to complete...) when the budget has
    /// actuals: every account, category and summary row, and the lines of
    /// `account` (src/actuals.rs). The session fills it in.
    pub cost: Option<crate::actuals::CostDto>,
    /// The budget is shared through a folder (src/shared.rs): the page shows
    /// the status item, the people and the review badges.
    pub shared: bool,
    /// Something to tell the person once, after this command (an undo in a
    /// shared budget that couldn't put everything back).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    /// A budget converted from an export and not saved yet: the banner the
    /// page shows on its tab until it is dismissed or saved (decisions.md,
    /// entry 67).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub converted: Option<String>,
    /// A review file open on this budget (src/review.rs): whether it is
    /// being written or read, and which places have comments to settle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<crate::review::ReviewBrief>,
    /// Each person a row pays (decisions.md, entry 79), by the matching
    /// form of the name (`method_core::person_key`): what they
    /// are paid across accounts and each percent fringe's pay against its
    /// cutoff, for the Person column. Left out when no row has a person (or
    /// there are no totals), so other budgets' views are unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persons: Option<BTreeMap<String, PersonDto>>,
    /// Some row has a person, whatever the state of the totals: the Person
    /// column shows by default then. Left out when false.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub has_persons: bool,
}

/// A person's running totals, as [`method_core::calc::persons::person_totals`]
/// works them out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PersonDto {
    /// The name as the person's first row spells it (the map's key is the
    /// matching form: trimmed, single spaces, lower case).
    pub name: String,
    /// How many lines and adjustment rows have the person.
    pub lines: usize,
    /// The accounts they are in, in budget order.
    pub accounts: Vec<String>,
    /// What the counted rows come to.
    pub pay: Cents,
    pub fringe_total: Cents,
    pub fringes: Vec<PersonFringeDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PersonFringeDto {
    pub fringe: String,
    /// As applied (times the cutoff factor); `None` for no cutoff.
    pub cutoff: Option<Cents>,
    /// Pay on the counted rows carrying the fringe.
    pub pay: Cents,
    pub amount: Cents,
    /// The pay is past the cutoff.
    pub reached: bool,
}

/// Every person of `b` from `report`, or `None` when there are none.
pub fn persons(b: &Budget, report: Option<&Report>) -> Option<BTreeMap<String, PersonDto>> {
    let report = report?;
    if !method_core::calc::persons::any(b) {
        return None;
    }
    // A partial report (flagged cells read as blank) still lines up with
    // `b` row for row, so its persons are as good as its totals.
    let all = method_core::calc::persons::person_totals(b, report)?;
    let out: BTreeMap<String, PersonDto> = all
        .into_iter()
        .map(|(name, p)| {
            let dto = PersonDto {
                name: p.name,
                lines: p.rows.len(),
                accounts: p.accounts,
                pay: p.pay,
                fringe_total: p.fringe_total,
                fringes: p
                    .fringes
                    .into_iter()
                    .map(|f| PersonFringeDto {
                        fringe: f.fringe,
                        cutoff: f.cutoff,
                        pay: f.pay,
                        amount: f.amount,
                        reached: f.reached,
                    })
                    .collect(),
            };
            (name, dto)
        })
        .collect();
    (!out.is_empty()).then_some(out)
}

/// One account in full: its rows in file-format JSON and their totals.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AccountView {
    /// Where it is: index in `budget.topsheet`, and in that category's accounts.
    pub ti: usize,
    pub ai: usize,
    pub code: String,
    /// The account's `details` array, exactly as the file format writes it.
    pub details: serde_json::Value,
    /// One entry per row, in order (`report.categories[..].accounts[..].rows`
    /// in the full view), or `None` without totals.
    pub rows: Option<Vec<RowDto>>,
    /// One entry per row, in order: the evaluated values of its formula
    /// cells (`{amount, x, rate, x4}`, each left out when blank, a plain
    /// number or in error), or `null` for a row with none. Empty without
    /// totals. See [`crate::display::cell_values`].
    pub values: Vec<Option<crate::display::CellValues>>,
}

/// Everything, for printing: the view as it was before the lean view.
#[derive(Clone, Debug, Serialize)]
pub struct FullView {
    pub rev: u64,
    /// The whole `budget` object as `docs/FORMAT.md` describes it, except
    /// that `versions` lists each version's `id`, `name`, `created` and `note`.
    pub budget: serde_json::Value,
    pub report: Option<ReportDto>,
    pub partial: bool,
    pub error: Option<ErrorDto>,
    pub issues: Vec<Issue>,
    pub file: FileDto,
    pub can_undo: bool,
    pub can_redo: bool,
    pub variance: Option<VarianceDto>,
    /// Cost figures with every line's sums, when the budget has actuals.
    pub cost: Option<crate::actuals::CostDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ErrorDto {
    /// Where, like `account 11-01 line 2 (Producer): rate`.
    pub at: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileDto {
    pub path: Option<String>,
    pub name: String,
    pub dirty: bool,
}

/// The account the UI shows, as it names it: category code and topsheet
/// index, account code and index in the category. Codes win over indexes,
/// the same way the UI finds its place after an edit (`resolveNav`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Focus {
    pub category: String,
    pub ti: usize,
    pub account: String,
    pub ai: usize,
}

impl Focus {
    /// Where the account is now: (topsheet index, account index), or `None`
    /// when its category or the account is gone.
    pub fn resolve(&self, b: &Budget) -> Option<(usize, usize)> {
        let cats: Vec<(usize, &method_core::Category)> = b
            .topsheet
            .iter()
            .enumerate()
            .filter_map(|(ti, r)| match r {
                TopsheetRow::Category(c) if c.code == self.category => Some((ti, c)),
                _ => None,
            })
            .collect();
        let &(ti, cat) = cats
            .iter()
            .find(|(ti, _)| *ti == self.ti)
            .or(cats.first())?;
        let ai = cat
            .accounts
            .iter()
            .enumerate()
            .find(|(i, a)| a.code == self.account && *i == self.ai)
            .or_else(|| {
                cat.accounts
                    .iter()
                    .enumerate()
                    .find(|(_, a)| a.code == self.account)
            })
            .map(|(i, _)| i)?;
        Some((ti, ai))
    }
}

/// Everything about the session a view shows, apart from the budget.
pub struct ViewState<'a> {
    pub rev: u64,
    pub outcome: &'a Outcome,
    pub path: Option<&'a Path>,
    pub name: String,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
}

impl ViewState<'_> {
    fn file(&self) -> FileDto {
        FileDto {
            path: self.path.map(|p| p.display().to_string()),
            name: self.name.clone(),
            dirty: self.dirty,
        }
    }

    fn error(&self) -> Option<ErrorDto> {
        self.outcome.error.as_ref().map(|e| ErrorDto {
            at: e.at.clone(),
            message: error_message(e),
        })
    }
}

/// The budget as file-format JSON (the `budget` member of the document).
pub fn budget_json(b: &Budget) -> serde_json::Value {
    let text = method_format::to_string(b);
    let mut doc: serde_json::Value =
        serde_json::from_str(&text).expect("the writer produces valid JSON");
    doc.get_mut("budget")
        .map(serde_json::Value::take)
        .unwrap_or_default()
}

/// A version as the views list it: everything but its content.
fn version_entry(v: &Version) -> serde_json::Value {
    let mut o = serde_json::json!({ "id": v.id, "name": v.name, "created": v.created });
    if let Some(note) = &v.note {
        o["note"] = note.clone().into();
    }
    o
}

/// A line block as the views list it: its name and how many rows it holds
/// (the rows and their setup stay in Rust until the block is inserted).
fn line_preset_entry(p: &LinePreset) -> serde_json::Value {
    serde_json::json!({ "name": p.name, "count": p.rows.len() })
}

/// Takes the heavy parts out of a budget (detail rows, the baseline's row
/// totals, versions, line blocks, actuals) and puts them back when dropped,
/// even on a panic, so the outline can be written without copying the budget.
struct Stripped<'a> {
    b: &'a mut Budget,
    details: Vec<Vec<Detail>>,
    baseline_rows: Option<BTreeMap<RowId, Cents>>,
    versions: Vec<Version>,
    line_presets: Vec<LinePreset>,
    /// The transactions go to the page through the Actuals tab's list and
    /// the cost figures, never in the outline.
    actuals: std::sync::Arc<method_core::actuals::Actuals>,
    keep_details: bool,
}

impl<'a> Stripped<'a> {
    fn new(b: &'a mut Budget, keep_details: bool) -> Stripped<'a> {
        let details = if keep_details {
            Vec::new()
        } else {
            b.categories_mut()
                .flat_map(|c| &mut c.accounts)
                .map(|a| std::mem::take(&mut a.details))
                .collect()
        };
        let baseline_rows = if keep_details {
            None
        } else {
            b.baseline.as_mut().map(|bl| std::mem::take(&mut bl.rows))
        };
        let versions = std::mem::take(&mut b.versions);
        let line_presets = std::mem::take(&mut b.line_presets);
        let actuals = std::mem::take(&mut b.actuals);
        Stripped {
            b,
            details,
            baseline_rows,
            versions,
            line_presets,
            actuals,
            keep_details,
        }
    }
}

impl Drop for Stripped<'_> {
    fn drop(&mut self) {
        if !self.keep_details {
            let rows = std::mem::take(&mut self.details);
            for (a, d) in self
                .b
                .categories_mut()
                .flat_map(|c| &mut c.accounts)
                .zip(rows)
            {
                a.details = d;
            }
            if let (Some(bl), Some(rows)) = (self.b.baseline.as_mut(), self.baseline_rows.take()) {
                bl.rows = rows;
            }
        }
        self.b.versions = std::mem::take(&mut self.versions);
        self.b.line_presets = std::mem::take(&mut self.line_presets);
        self.b.actuals = std::mem::take(&mut self.actuals);
    }
}

/// The budget's JSON with versions and line blocks listed but not
/// included; without rows (each account gets `lines` instead) unless
/// `with_rows`.
fn budget_value(b: &mut Budget, with_rows: bool) -> serde_json::Value {
    let lines: Vec<usize> = b
        .categories()
        .flat_map(|c| &c.accounts)
        .map(|a| a.details.len())
        .collect();
    let versions: Vec<serde_json::Value> = b.versions.iter().map(version_entry).collect();
    let blocks: Vec<serde_json::Value> = b.line_presets.iter().map(line_preset_entry).collect();
    let mut json = {
        let s = Stripped::new(b, with_rows);
        budget_json(s.b)
    };
    if !with_rows {
        let accounts = json
            .get_mut("topsheet")
            .and_then(serde_json::Value::as_array_mut)
            .into_iter()
            .flatten()
            .filter(|r| r["kind"] == "category")
            .flat_map(|r| {
                r.get_mut("accounts")
                    .and_then(serde_json::Value::as_array_mut)
                    .into_iter()
                    .flatten()
            });
        for (a, n) in accounts.zip(lines) {
            if let Some(o) = a.as_object_mut() {
                o.remove("details");
                o.insert("lines".into(), n.into());
            }
        }
    }
    if !versions.is_empty() {
        json["versions"] = versions.into();
    }
    if !blocks.is_empty() {
        json["line_presets"] = blocks.into();
    }
    json
}

/// One account's `details` array in file-format JSON.
fn details_value(b: &Budget, ti: usize, ai: usize) -> serde_json::Value {
    let Some(TopsheetRow::Category(cat)) = b.topsheet.get(ti) else {
        return serde_json::Value::Array(Vec::new());
    };
    let Some(acct) = cat.accounts.get(ai) else {
        return serde_json::Value::Array(Vec::new());
    };
    // Written through a one-account budget, so the rows come out exactly as
    // a file holds them.
    let mut one = Budget::new("", b.base_currency.clone());
    one.topsheet
        .push(TopsheetRow::Category(method_core::Category {
            notes: Vec::new(),
            code: cat.code.clone(),
            name: cat.name.clone(),
            accounts: vec![acct.clone()],
        }));
    let mut json = budget_json(&one);
    json["topsheet"][0]["accounts"][0]
        .get_mut("details")
        .map(serde_json::Value::take)
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()))
}

/// The report's position of the category at topsheet index `ti`.
fn category_index(b: &Budget, ti: usize) -> usize {
    b.topsheet[..ti.min(b.topsheet.len())]
        .iter()
        .filter(|r| matches!(r, TopsheetRow::Category(_)))
        .count()
}

/// Where every row is: row id to (topsheet index, account index). The first
/// row wins if an id repeats.
fn row_places(b: &Budget) -> BTreeMap<RowId, [usize; 2]> {
    let mut out = BTreeMap::new();
    for (ti, r) in b.topsheet.iter().enumerate() {
        let TopsheetRow::Category(c) = r else {
            continue;
        };
        for (ai, a) in c.accounts.iter().enumerate() {
            for d in &a.details {
                out.entry(d.id).or_insert([ti, ai]);
            }
        }
    }
    out
}

fn variance(b: &Budget, report: Option<&Report>) -> Option<method_core::Variance> {
    b.baseline
        .as_ref()
        .zip(report)
        .and_then(|(base, report)| method_core::variance(base, report))
}

impl View {
    /// The lean view of `b`, with the account at `focus` (topsheet index,
    /// account index) in full.
    pub fn build(
        b: &mut Budget,
        st: &ViewState,
        focus: Option<(usize, usize)>,
        created_row: Option<RowId>,
    ) -> View {
        let outcome = st.outcome;
        let report = outcome.report.as_ref();
        let account = focus.map(|(ti, ai)| {
            let code = match &b.topsheet[ti] {
                TopsheetRow::Category(c) => c.accounts[ai].code.clone(),
                TopsheetRow::ProductionTotal(_) => String::new(),
            };
            let ci = category_index(b, ti);
            AccountView {
                ti,
                ai,
                code,
                details: details_value(b, ti, ai),
                rows: report
                    .and_then(|r| r.categories.get(ci))
                    .and_then(|c| c.accounts.get(ai))
                    .map(|a| a.rows.iter().map(RowDto::from).collect()),
                values: match (&b.topsheet[ti], report) {
                    (TopsheetRow::Category(c), Some(r)) => {
                        crate::display::cell_values(&c.accounts[ai].details, &r.globals)
                    }
                    _ => Vec::new(),
                },
            }
        });
        let shown: BTreeSet<RowId> = focus
            .and_then(|(ti, ai)| match &b.topsheet[ti] {
                TopsheetRow::Category(c) => c.accounts.get(ai),
                TopsheetRow::ProductionTotal(_) => None,
            })
            .map(|a| a.details.iter().map(|d| d.id).collect())
            .unwrap_or_default();

        let is_cell = |i: &Issue| matches!(i.target, Target::Cell { .. });
        let issues: Vec<Issue> = outcome
            .issues
            .iter()
            .filter(|i| match i.target {
                Target::Cell { row_id, .. } => shown.contains(&row_id),
                _ => true,
            })
            .cloned()
            .collect();
        let issue_accounts: Vec<[usize; 2]> = if outcome.issues.iter().any(is_cell) {
            let places = row_places(b);
            outcome
                .issues
                .iter()
                .filter_map(|i| match i.target {
                    Target::Cell { row_id, .. } => places.get(&row_id).copied(),
                    _ => None,
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };

        View {
            rev: st.rev,
            variance: variance(b, report)
                .as_ref()
                .map(|v| VarianceDto::with_rows(v, |id| shown.contains(&id))),
            budget: budget_value(b, false),
            report: report.map(|r| ReportDto::with_rows(r, false)),
            partial: outcome.partial(),
            error: st.error(),
            issues,
            issue_count: outcome.issues.len(),
            issue_accounts,
            file: st.file(),
            can_undo: st.can_undo,
            can_redo: st.can_redo,
            created_row,
            account,
            doc: 0,
            tabs: Vec::new(),
            panes: None,
            cost: None,
            shared: false,
            notice: None,
            converted: None,
            review: None,
            persons: persons(b, report),
            has_persons: method_core::calc::persons::any(b),
        }
    }
}

impl FullView {
    pub fn build(b: &mut Budget, st: &ViewState) -> FullView {
        let report = st.outcome.report.as_ref();
        FullView {
            rev: st.rev,
            variance: variance(b, report).as_ref().map(VarianceDto::from),
            budget: budget_value(b, true),
            report: report.map(ReportDto::from),
            partial: st.outcome.partial(),
            error: st.error(),
            issues: st.outcome.issues.clone(),
            file: st.file(),
            can_undo: st.can_undo,
            can_redo: st.can_redo,
            cost: None,
        }
    }
}
