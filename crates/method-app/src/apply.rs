//! The apply panel: puts fringes and groups on the selected rows or takes them
//! off, gives the rows a set, a location or a currency, makes and removes
//! fringe ranges, and sums up the selection so the panel can show which items
//! the selected rows carry and what they cost.
//!
//! "The selection" is a set of detail rows. On the topsheet and account
//! levels it is every row under the selected categories or accounts (see
//! [`rows_under`]). Each edit checks everything first and only then changes
//! the budget, so a rejected edit leaves it alone; run through
//! `Session::edit` (as the commands do), each one is a single undo step.
//!
//! Which rows take what follows the grid's cells (`edit::Field`): fringes and
//! a currency go on lines; groups, a set and a location go on lines and
//! adjustments. Other rows in a selection are skipped.

use std::collections::{BTreeMap, BTreeSet};

use method_core::{Budget, Dec, Detail, FringeKind, RowId, RowKind, TagPreset, TopsheetRow};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{edit_budget_as, AppState, DocId};
use crate::edit::{details, EditResult};
use crate::report_dto::SelectionDto;
use crate::session::SelectionIndex;
use crate::view::View;

type Cents = i64;

/// Lines take fringes and a currency.
fn is_line(d: &Detail) -> bool {
    d.kind == RowKind::Line
}

/// Lines and adjustments take groups, a set and a location.
fn is_costed(d: &Detail) -> bool {
    matches!(d.kind, RowKind::Line | RowKind::Adjustment)
}

/// A name list a row can carry several of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tag {
    Fringes,
    Groups,
}

impl Tag {
    fn parse(s: &str) -> EditResult<Tag> {
        match s {
            "fringes" => Ok(Tag::Fringes),
            "groups" => Ok(Tag::Groups),
            other => Err(format!(
                "`{other}` can't be applied to rows; use fringes or groups"
            )),
        }
    }

    fn what(self) -> &'static str {
        match self {
            Tag::Fringes => "fringe",
            Tag::Groups => "group",
        }
    }

    fn takes(self, d: &Detail) -> bool {
        match self {
            Tag::Fringes => is_line(d),
            Tag::Groups => is_costed(d),
        }
    }

    fn names(self, d: &Detail) -> &BTreeSet<String> {
        match self {
            Tag::Fringes => &d.fringes,
            Tag::Groups => &d.groups,
        }
    }

    fn names_mut(self, d: &mut Detail) -> &mut BTreeSet<String> {
        match self {
            Tag::Fringes => &mut d.fringes,
            Tag::Groups => &mut d.groups,
        }
    }

    fn defined(self, b: &Budget, name: &str) -> bool {
        match self {
            Tag::Fringes => b.fringes.contains_key(name),
            Tag::Groups => b.groups.contains_key(name),
        }
    }

    fn none_take(self) -> &'static str {
        match self {
            Tag::Fringes => "fringes only go on lines, and none of the selected rows is a line",
            Tag::Groups => "none of the selected rows can be in a group",
        }
    }
}

/// A value a row carries at most one of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pick {
    Set,
    Location,
    Currency,
}

impl Pick {
    fn parse(s: &str) -> EditResult<Pick> {
        match s {
            "set" => Ok(Pick::Set),
            "location" => Ok(Pick::Location),
            "currency" => Ok(Pick::Currency),
            other => Err(format!(
                "`{other}` can't be assigned to rows; use set, location or currency"
            )),
        }
    }

    fn takes(self, d: &Detail) -> bool {
        match self {
            Pick::Set | Pick::Location => is_costed(d),
            Pick::Currency => is_line(d),
        }
    }

    fn slot(self, d: &mut Detail) -> &mut Option<String> {
        match self {
            Pick::Set => &mut d.set,
            Pick::Location => &mut d.location,
            Pick::Currency => &mut d.currency,
        }
    }

    fn none_take(self) -> &'static str {
        match self {
            Pick::Set => "none of the selected rows can take a set",
            Pick::Location => "none of the selected rows can take a location",
            Pick::Currency => {
                "currencies only go on lines, and none of the selected rows is a line"
            }
        }
    }

    /// The stored value for `typed`: blank clears it, and the base currency
    /// is stored as no currency. Anything else must be defined.
    fn value(self, b: &Budget, typed: Option<&str>) -> EditResult<Option<String>> {
        let Some(v) = typed.map(str::trim).filter(|v| !v.is_empty()) else {
            return Ok(None);
        };
        let known = match self {
            Pick::Set => b.sets.contains_key(v),
            Pick::Location => b.locations.contains_key(v),
            Pick::Currency if v == b.base_currency => return Ok(None),
            Pick::Currency => b.currencies.contains_key(v),
        };
        if known {
            return Ok(Some(v.to_string()));
        }
        Err(match self {
            Pick::Set => format!("there is no set `{v}`"),
            Pick::Location => format!("there is no location `{v}`"),
            Pick::Currency => {
                format!("there is no exchange rate for {v}; add it under Currencies first")
            }
        })
    }
}

/// The selected row ids, without repeats. Refuses an empty selection and a
/// row that no longer exists.
fn selected(b: &Budget, ids: &[RowId]) -> EditResult<BTreeSet<RowId>> {
    if ids.is_empty() {
        return Err("select one or more lines first".into());
    }
    let want: BTreeSet<RowId> = ids.iter().copied().collect();
    let have: BTreeSet<RowId> = details(b)
        .map(|d| d.id)
        .filter(|id| want.contains(id))
        .collect();
    if let Some(gone) = want.difference(&have).next() {
        return Err(format!("row {gone} no longer exists"));
    }
    Ok(want)
}

fn selected_mut<'a>(
    b: &'a mut Budget,
    ids: &'a BTreeSet<RowId>,
) -> impl Iterator<Item = &'a mut Detail> + 'a {
    b.categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
        .filter(move |d| ids.contains(&d.id))
}

// ---------------------------------------------------------------------------
// Edits

/// Puts fringe or group `name` on every selected row that can take it
/// (`on`), or takes it off them. `field` is `fringes` or `groups`. Adding
/// needs a defined name; removing also accepts a name that is only left on
/// the rows (say, from an edited file). Returns how many rows changed.
pub fn apply_tag(
    b: &mut Budget,
    ids: &[RowId],
    field: &str,
    name: &str,
    on: bool,
) -> EditResult<usize> {
    let tag = Tag::parse(field)?;
    let ids = selected(b, ids)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("{} name is empty", tag.what()));
    }
    let mut any = false;
    let mut carried = false;
    for d in details(b).filter(|d| ids.contains(&d.id) && tag.takes(d)) {
        any = true;
        carried |= tag.names(d).contains(name);
    }
    if !any {
        return Err(tag.none_take().into());
    }
    if !tag.defined(b, name) && (on || !carried) {
        return Err(format!("there is no {} `{name}`", tag.what()));
    }
    let mut changed = 0;
    for d in selected_mut(b, &ids) {
        if !tag.takes(d) {
            continue;
        }
        let names = tag.names_mut(d);
        let did = if on {
            names.insert(name.to_string())
        } else {
            names.remove(name)
        };
        changed += usize::from(did);
    }
    Ok(changed)
}

/// Takes every fringe (`field` `fringes`) or group (`groups`) off the
/// selected rows that can carry them, including names only left on the rows.
/// Refused when none of the rows can. Returns how many rows changed.
pub fn clear_tags(b: &mut Budget, ids: &[RowId], field: &str) -> EditResult<usize> {
    let tag = Tag::parse(field)?;
    let ids = selected(b, ids)?;
    if !details(b).any(|d| ids.contains(&d.id) && tag.takes(d)) {
        return Err(tag.none_take().into());
    }
    let mut changed = 0;
    for d in selected_mut(b, &ids) {
        if !tag.takes(d) {
            continue;
        }
        let names = tag.names_mut(d);
        changed += usize::from(!names.is_empty());
        names.clear();
    }
    Ok(changed)
}

/// The tags "Replace tags" sets on the selected rows, exactly: every list
/// empty and every value none takes them all off.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TagsInput {
    #[serde(default)]
    pub fringes: Vec<String>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub set: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    /// `None` or the base currency: the base currency.
    #[serde(default)]
    pub currency: Option<String>,
}

/// Makes the selected rows' tags exactly `tags` (fringes and the currency on
/// lines; groups, a set and a location on lines and adjustments), as a tag
/// preset's replace does. Every name must be defined. One undo step.
pub fn replace_row_tags(b: &mut Budget, ids: &[RowId], tags: &TagsInput) -> EditResult<usize> {
    let preset = TagPreset {
        name: String::new(),
        fringes: tags.fringes.iter().cloned().collect(),
        groups: tags.groups.iter().cloned().collect(),
        set: tags.set.clone(),
        location: tags.location.clone(),
        currency: tags.currency.clone(),
    };
    crate::presets::replace_tags(b, preset, ids)
}

/// Gives every selected row that can take it the set, location or currency
/// `value` (`field` is `set`, `location` or `currency`); `None` or blank
/// clears it, and the base currency counts as clearing the currency.
/// Returns how many rows changed.
pub fn assign_value(
    b: &mut Budget,
    ids: &[RowId],
    field: &str,
    value: Option<&str>,
) -> EditResult<usize> {
    let pick = Pick::parse(field)?;
    let ids = selected(b, ids)?;
    let value = pick.value(b, value)?;
    if !details(b).any(|d| ids.contains(&d.id) && pick.takes(d)) {
        return Err(pick.none_take().into());
    }
    let mut changed = 0;
    for d in selected_mut(b, &ids) {
        if !pick.takes(d) {
            continue;
        }
        let slot = pick.slot(d);
        if *slot != value {
            slot.clone_from(&value);
            changed += 1;
        }
    }
    Ok(changed)
}

/// What making a fringe range out of a selection would do.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RangePlan {
    /// The lines and adjustments that get the number.
    members: BTreeSet<RowId>,
    number: u32,
    /// They already are exactly that range.
    unchanged: bool,
}

/// Checks that the selection can become one fringe range: rows in one
/// account, next to each other (unselected subtotal rows in between are
/// fine; they never break a range), with at least two lines. The number is
/// new in the account unless the rows already form a range of their own.
fn range_plan(b: &Budget, ids: &[RowId]) -> EditResult<RangePlan> {
    let ids = selected(b, ids)?;
    let acct = b
        .categories()
        .flat_map(|c| &c.accounts)
        .find(|a| a.details.iter().any(|d| ids.contains(&d.id)))
        .ok_or("select one or more lines first")?;
    let at: Vec<usize> = acct
        .details
        .iter()
        .enumerate()
        .filter(|(_, d)| ids.contains(&d.id))
        .map(|(i, _)| i)
        .collect();
    if at.len() != ids.len() {
        return Err("a fringe range has to stay inside one account".into());
    }
    let (first, last) = (at[0], at[at.len() - 1]);
    let rows = &acct.details;
    let gap = (first..=last).any(|i| !ids.contains(&rows[i].id) && is_costed(&rows[i]));
    if gap {
        return Err("select lines next to each other to make a fringe range".into());
    }
    if at.iter().filter(|&&i| is_line(&rows[i])).count() < 2 {
        return Err("a fringe range needs at least two lines".into());
    }
    let members: BTreeSet<RowId> = at
        .iter()
        .map(|&i| &rows[i])
        .filter(|d| is_costed(d))
        .map(|d| d.id)
        .collect();
    let numbers: BTreeSet<Option<u32>> = at
        .iter()
        .map(|&i| &rows[i])
        .filter(|d| is_costed(d))
        .map(|d| d.fringe_range)
        .collect();
    if let [Some(n)] = numbers.iter().copied().collect::<Vec<_>>()[..] {
        let outside = rows
            .iter()
            .enumerate()
            .any(|(i, d)| (i < first || i > last) && d.fringe_range == Some(n));
        if !outside {
            return Ok(RangePlan {
                members,
                number: n,
                unchanged: true,
            });
        }
    }
    let used: BTreeSet<u32> = rows.iter().filter_map(|d| d.fringe_range).collect();
    let number = match used.last() {
        None => 1,
        Some(&top) => top
            .checked_add(1)
            .or_else(|| (1..=u32::MAX).find(|n| !used.contains(n)))
            .ok_or("this account has no fringe range numbers left")?,
    };
    Ok(RangePlan {
        members,
        number,
        unchanged: false,
    })
}

/// Makes the selected rows one fringe range (see [`range_plan`] for what
/// qualifies). Returns the range number.
pub fn make_range(b: &mut Budget, ids: &[RowId]) -> EditResult<u32> {
    let plan = range_plan(b, ids)?;
    if !plan.unchanged {
        for d in selected_mut(b, &plan.members) {
            d.fringe_range = Some(plan.number);
        }
    }
    Ok(plan.number)
}

/// Takes the selected rows out of their fringe ranges. Refused when none of
/// them is in one. Returns how many rows changed.
pub fn clear_range(b: &mut Budget, ids: &[RowId]) -> EditResult<usize> {
    let ids = selected(b, ids)?;
    let ranged = details(b)
        .filter(|d| ids.contains(&d.id) && d.fringe_range.is_some())
        .count();
    if ranged == 0 {
        return Err("none of the selected lines are in a fringe range".into());
    }
    for d in selected_mut(b, &ids) {
        d.fringe_range = None;
    }
    Ok(ranged)
}

// ---------------------------------------------------------------------------
// What the panel shows

/// Every row id under the selection, in budget order: the rows in `row_ids`,
/// every row in the accounts numbered in `accounts`, and every row in the
/// categories at topsheet positions `categories`. Rows, accounts and
/// positions that don't exist (or a production total's position) are
/// ignored: this only reads.
pub fn rows_under(
    b: &Budget,
    row_ids: &[RowId],
    accounts: &[String],
    categories: &[usize],
) -> Vec<RowId> {
    let rows: BTreeSet<RowId> = row_ids.iter().copied().collect();
    let accts: BTreeSet<&str> = accounts.iter().map(String::as_str).collect();
    let cats: BTreeSet<usize> = categories.iter().copied().collect();
    let mut out = Vec::new();
    for (ti, row) in b.topsheet.iter().enumerate() {
        let TopsheetRow::Category(cat) = row else {
            continue;
        };
        for acct in &cat.accounts {
            let whole = cats.contains(&ti) || accts.contains(acct.code.as_str());
            out.extend(
                acct.details
                    .iter()
                    .filter(|d| whole || rows.contains(&d.id))
                    .map(|d| d.id),
            );
        }
    }
    out
}

/// One item in a list: how many selected rows carry it, and its dollars
/// within the selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Tally {
    pub count: usize,
    /// Fringes: that fringe's dollars. Everything else: the line totals of
    /// the selected rows carrying it (base currency, after any suppression).
    pub total: Cents,
}

/// Everything the apply panel shows for a selection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ApplySummary {
    /// The selected rows, in budget order. The edit commands take these.
    pub rows: Vec<RowId>,
    /// How many of them are lines (they take fringes and a currency).
    pub lines: usize,
    /// How many are lines or adjustments (they take groups, a set and a
    /// location).
    pub costed: usize,
    /// The selection's totals, as `selection_totals` returns them.
    pub totals: SelectionDto,
    /// Fringe dollars as a percentage of the line totals, rounded to two
    /// places (`"18.25"`). `None` when the line totals are 0.
    pub fringe_percent: Option<String>,
    /// The selection's fringe dollars from percent fringes, and from flat
    /// fringes (the fringe list's footer). Together they are the selection's
    /// fringe total.
    pub percent_fringes: Cents,
    pub flat_fringes: Cents,
    /// Every defined fringe and group, by name.
    pub fringes: BTreeMap<String, Tally>,
    pub groups: BTreeMap<String, Tally>,
    /// Every defined set and location, by name.
    pub sets: BTreeMap<String, Tally>,
    pub locations: BTreeMap<String, Tally>,
    /// Every defined currency, and the base currency (lines with no currency
    /// count there), by code.
    pub currencies: BTreeMap<String, Tally>,
    /// Selected rows that are in a fringe range ("remove range" needs one).
    pub in_range: usize,
    /// Why "make range" can't run on this selection, or `None` when it can.
    pub range_blocked: Option<String>,
}

/// `part` as a percentage of `whole`, rounded half away from zero to two
/// places (`"18.25"`, `"10"`). `None` when `whole` is 0.
pub fn percent(part: Cents, whole: Cents) -> Option<String> {
    if whole == 0 {
        return None;
    }
    let n = i128::from(part) * 10_000;
    let d = i128::from(whole);
    let (mut q, r) = (n / d, n % d);
    if 2 * r.abs() >= d.abs() {
        q += n.signum() * d.signum();
    }
    // q is in hundredths of a percent; as "cents" it prints with two places.
    Some(Dec::from_cents(Cents::try_from(q).ok()?).to_string())
}

/// Sums up `rows` for the panel. `totals_of` gives the selection totals of
/// any rows (the `selection_totals` command's numbers).
pub fn summary(
    b: &Budget,
    rows: Vec<RowId>,
    mut totals_of: impl FnMut(&[RowId]) -> SelectionDto,
) -> ApplySummary {
    let wanted: BTreeSet<RowId> = rows.iter().copied().collect();
    let chosen: Vec<&Detail> = details(b).filter(|d| wanted.contains(&d.id)).collect();
    let totals = totals_of(&rows);

    let blank = |names: Vec<&String>| -> BTreeMap<String, Tally> {
        names
            .into_iter()
            .map(|n| (n.clone(), Tally::default()))
            .collect()
    };
    let mut fringes = blank(b.fringes.keys().collect());
    let mut groups = blank(b.groups.keys().collect());
    for d in &chosen {
        if is_line(d) {
            for f in &d.fringes {
                if let Some(t) = fringes.get_mut(f) {
                    t.count += 1;
                }
            }
        }
        if is_costed(d) {
            for g in &d.groups {
                if let Some(t) = groups.get_mut(g) {
                    t.count += 1;
                }
            }
        }
    }
    let (mut percent_fringes, mut flat_fringes): (Cents, Cents) = (0, 0);
    for (name, t) in &mut fringes {
        t.total = totals.fringes.get(name).copied().unwrap_or(0);
        let sum = match b.fringes.get(name).map(|f| &f.kind) {
            Some(FringeKind::Flat { .. }) => &mut flat_fringes,
            _ => &mut percent_fringes,
        };
        *sum = sum.saturating_add(t.total);
    }
    for (name, t) in &mut groups {
        t.total = totals.groups.get(name).map_or(0, |g| g.details_total);
    }

    // Single values: count the rows per value, then total each used value.
    let mut by_value = |names: Vec<&String>,
                        takes: fn(&Detail) -> bool,
                        value: &dyn Fn(&Detail) -> Option<String>|
     -> BTreeMap<String, Tally> {
        let mut out = blank(names);
        let mut ids: BTreeMap<String, Vec<RowId>> = BTreeMap::new();
        for d in chosen.iter().filter(|d| takes(d)) {
            if let Some(v) = value(d).filter(|v| out.contains_key(v)) {
                ids.entry(v).or_default().push(d.id);
            }
        }
        for (v, list) in ids {
            out.insert(
                v,
                Tally {
                    count: list.len(),
                    total: totals_of(&list).lines.details_total,
                },
            );
        }
        out
    };
    let sets = by_value(b.sets.keys().collect(), is_costed, &|d: &Detail| {
        d.set.clone()
    });
    let locations = by_value(b.locations.keys().collect(), is_costed, &|d: &Detail| {
        d.location.clone()
    });
    let base = b.base_currency.clone();
    let codes: Vec<&String> = std::iter::once(&base).chain(b.currencies.keys()).collect();
    let currencies = by_value(codes, is_line, &|d: &Detail| {
        Some(
            d.currency
                .clone()
                .filter(|c| !c.trim().is_empty())
                .unwrap_or_else(|| base.clone()),
        )
    });

    let range_blocked = if rows.is_empty() {
        Some("select lines first".to_string())
    } else {
        match range_plan(b, &rows) {
            Ok(p) if p.unchanged => Some("these lines are already one fringe range".to_string()),
            Ok(_) => None,
            Err(e) => Some(e),
        }
    };

    ApplySummary {
        lines: chosen.iter().filter(|d| is_line(d)).count(),
        costed: chosen.iter().filter(|d| is_costed(d)).count(),
        fringe_percent: percent(totals.lines.fringe_total, totals.lines.details_total),
        percent_fringes,
        flat_fringes,
        in_range: chosen.iter().filter(|d| d.fringe_range.is_some()).count(),
        rows,
        totals,
        fringes,
        groups,
        sets,
        locations,
        currencies,
        range_blocked,
    }
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json)

/// The panel's numbers for a selection: rows by id, whole accounts by
/// number, whole categories by topsheet position. Rows and accounts that are
/// gone are left out; only a closed budget (`doc`) is refused.
#[tauri::command]
pub fn apply_summary(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Option<Vec<RowId>>,
    accounts: Option<Vec<String>>,
    categories: Option<Vec<usize>>,
) -> Result<ApplySummary, String> {
    let mut s = state.lock_doc(doc)?;
    let (budget, outcome) = s.totals();
    let rows = rows_under(
        budget,
        row_ids.as_deref().unwrap_or_default(),
        accounts.as_deref().unwrap_or_default(),
        categories.as_deref().unwrap_or_default(),
    );
    // Rows are looked up once for every total the panel shows.
    let index = SelectionIndex::new(budget, outcome);
    Ok(summary(budget, rows, |ids| index.totals(ids)))
}

/// Puts a fringe or group on the rows (`on`), or takes it off.
#[tauri::command]
pub fn apply_to_rows<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
    field: String,
    name: String,
    on: bool,
) -> Result<View, String> {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| {
            let what = tag_noun(&field);
            let rows = crate::history::rows_phrase(b, &row_ids);
            if on {
                format!("Add {what} {} to {rows}", name.trim())
            } else {
                format!("Remove {what} {} from {rows}", name.trim())
            }
        },
        |b| apply_tag(b, &row_ids, &field, &name, on),
    )
}

/// Gives the rows a set, location or currency; `null` clears it.
#[tauri::command]
pub fn assign_to_rows<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
    field: String,
    value: Option<String>,
) -> Result<View, String> {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| {
            let rows = crate::history::rows_phrase(b, &row_ids);
            match value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                Some(v) => format!("Set {} {v} on {rows}", tag_noun(&field)),
                None => format!("Clear {} on {rows}", tag_noun(&field)),
            }
        },
        |b| assign_value(b, &row_ids, &field, value.as_deref()),
    )
}

/// Takes every fringe or group (`field`) off the rows.
#[tauri::command]
pub fn clear_from_rows<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
    field: String,
) -> Result<View, String> {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| {
            format!(
                "Clear {} on {}",
                tag_noun(&field),
                crate::history::rows_phrase(b, &row_ids)
            )
        },
        |b| clear_tags(b, &row_ids, &field),
    )
}

/// Sets exactly `tags` on the rows (fringes, groups, set, location,
/// currency), as a tag preset's replace does.
#[tauri::command]
pub fn replace_tags_on_rows<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
    tags: TagsInput,
) -> Result<View, String> {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| {
            format!(
                "Set the tags on {}",
                crate::history::rows_phrase(b, &row_ids)
            )
        },
        |b| replace_row_tags(b, &row_ids, &tags),
    )
}

#[tauri::command]
pub fn make_fringe_range<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
) -> Result<View, String> {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| {
            format!(
                "Make a fringe range of {}",
                crate::history::rows_phrase(b, &row_ids)
            )
        },
        |b| make_range(b, &row_ids),
    )
}

#[tauri::command]
pub fn clear_fringe_range<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
) -> Result<View, String> {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| {
            format!(
                "Remove the fringe range from {}",
                crate::history::rows_phrase(b, &row_ids)
            )
        },
        |b| clear_range(b, &row_ids),
    )
}

/// A row tag field as a step label names it: "fringe", "group", "set".
fn tag_noun(field: &str) -> &str {
    match field {
        "fringes" => "fringe",
        "groups" => "group",
        "set" => "set",
        "location" => "location",
        "currency" => "currency",
        _ => "tag",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{self, Field};
    use crate::session::Session;
    use method_core::{Fringe, FringeKind, SetDef};

    /// The detail row ids of one account, in order.
    fn ids(b: &Budget, account: &str) -> Vec<RowId> {
        b.categories()
            .flat_map(|c| &c.accounts)
            .find(|a| a.code == account)
            .unwrap()
            .details
            .iter()
            .map(|d| d.id)
            .collect()
    }

    fn row(b: &Budget, id: RowId) -> Detail {
        details(b).find(|d| d.id == id).cloned().unwrap()
    }

    /// The starter budget with a group, a set, a location, a second fringe
    /// and a currency defined.
    fn budget() -> Budget {
        let mut b = method_core::starter::budget();
        edit::set_fringe(
            &mut b,
            "PENSION",
            &serde_json::from_value(serde_json::json!({"kind": "percent", "rate": "5"})).unwrap(),
        )
        .unwrap();
        edit::set_group(
            &mut b,
            "Crew",
            &serde_json::from_value(serde_json::json!({"in_budget_total": true})).unwrap(),
        )
        .unwrap();
        edit::set_set(
            &mut b,
            "Stage A",
            &serde_json::from_value(serde_json::json!({})).unwrap(),
        )
        .unwrap();
        edit::set_location(
            &mut b,
            "Toronto",
            &serde_json::from_value(serde_json::json!({})).unwrap(),
        )
        .unwrap();
        edit::set_currency(
            &mut b,
            "CAD",
            &serde_json::from_value(serde_json::json!({"rate": "0.75"})).unwrap(),
        )
        .unwrap();
        b
    }

    fn session() -> Session {
        Session::with_budget(budget(), None)
    }

    /// Account 20-01 with a subtotal after its first line and an adjustment
    /// at the end: [line, subtotal, line, adjustment].
    fn mixed(s: &mut Session) -> Vec<RowId> {
        let first = ids(s.budget(), "20-01")[0];
        s.edit("Test edit", |b| {
            edit::add_detail(b, "20-01", Some(first), RowKind::Subtotal)
        })
        .unwrap();
        s.edit("Test edit", |b| {
            edit::add_detail(b, "20-01", None, RowKind::Adjustment)
        })
        .unwrap();
        ids(s.budget(), "20-01")
    }

    #[test]
    fn fringes_go_on_the_lines_and_come_off_as_one_undo_step_each() {
        let mut s = session();
        let rows = mixed(&mut s);
        let before = s.budget().clone();

        let n = s
            .edit("Test edit", |b| {
                apply_tag(b, &rows, "fringes", "PENSION", true)
            })
            .unwrap();
        assert_eq!(n, 2, "only the two lines take a fringe");
        assert!(row(s.budget(), rows[0]).fringes.contains("PENSION"));
        assert!(row(s.budget(), rows[2]).fringes.contains("PENSION"));
        assert!(row(s.budget(), rows[1]).fringes.is_empty());
        assert!(row(s.budget(), rows[3]).fringes.is_empty());
        // The lines keep the fringe they had.
        assert!(row(s.budget(), rows[0]).fringes.contains("PAYROLL"));

        // Applying it again changes nothing and adds no undo step.
        assert_eq!(
            s.edit("Test edit", |b| apply_tag(
                b, &rows, "fringes", "PENSION", true
            )),
            Ok(0)
        );
        assert!(s.undo());
        assert_eq!(s.budget(), &before);
        assert!(s.redo());

        let off = s
            .edit("Test edit", |b| {
                apply_tag(b, &rows[..1], "fringes", "PAYROLL", false)
            })
            .unwrap();
        assert_eq!(off, 1);
        assert!(!row(s.budget(), rows[0]).fringes.contains("PAYROLL"));
        assert!(row(s.budget(), rows[2]).fringes.contains("PAYROLL"));
    }

    #[test]
    fn groups_go_on_lines_and_adjustments() {
        let mut s = session();
        let rows = mixed(&mut s);
        let n = s
            .edit("Test edit", |b| apply_tag(b, &rows, "groups", "Crew", true))
            .unwrap();
        assert_eq!(n, 3, "two lines and the adjustment; not the subtotal");
        assert!(row(s.budget(), rows[3]).groups.contains("Crew"));
        assert!(row(s.budget(), rows[1]).groups.is_empty());
        let n = s
            .edit("Test edit", |b| {
                apply_tag(b, &rows, "groups", "Crew", false)
            })
            .unwrap();
        assert_eq!(n, 3);
        assert!(details(s.budget()).all(|d| d.groups.is_empty()));
    }

    #[test]
    fn bad_tag_requests_are_refused_and_change_nothing() {
        let mut s = session();
        let rows = mixed(&mut s);
        let before = s.budget().clone();
        let refused = |s: &mut Session, ids: &[RowId], field: &str, name: &str, on: bool| {
            let err = s
                .edit("Test edit", |b| apply_tag(b, ids, field, name, on))
                .unwrap_err();
            assert_eq!(s.budget(), &before, "{err}");
            err
        };
        assert!(refused(&mut s, &rows, "fringes", "NOPE", true).contains("no fringe `NOPE`"));
        assert!(refused(&mut s, &rows, "groups", "Nobody", true).contains("no group"));
        // Removing a name nobody carries and nobody defined.
        assert!(refused(&mut s, &rows, "fringes", "NOPE", false).contains("NOPE"));
        assert!(refused(&mut s, &rows, "units", "Week", true).contains("units"));
        assert!(refused(&mut s, &rows, "fringes", "  ", true).contains("empty"));
        assert!(refused(&mut s, &[], "fringes", "PAYROLL", true).contains("select"));
        // A row that's gone sinks the whole request, even after good rows.
        let gone = [rows[0], rows[2], 999_999];
        assert!(refused(&mut s, &gone, "fringes", "PENSION", true).contains("999999"));
        // Only a subtotal and an adjustment: no line to take a fringe.
        assert!(refused(&mut s, &[rows[1], rows[3]], "fringes", "PAYROLL", true).contains("lines"));
        assert!(refused(&mut s, &[rows[1]], "groups", "Crew", true).contains("group"));
    }

    #[test]
    fn a_stale_name_left_on_rows_can_still_come_off() {
        let mut b = budget();
        let rows = ids(&b, "13-01");
        for d in selected_mut(&mut b, &rows.iter().copied().collect()) {
            d.fringes.insert("OLD".into());
        }
        assert_eq!(apply_tag(&mut b, &rows, "fringes", "OLD", false), Ok(2));
        assert!(apply_tag(&mut b, &rows, "fringes", "OLD", true).is_err());
    }

    #[test]
    fn sets_locations_and_currencies_are_assigned_and_cleared() {
        let mut s = session();
        let rows = mixed(&mut s);
        let n = s
            .edit("Test edit", |b| {
                assign_value(b, &rows, "set", Some("Stage A"))
            })
            .unwrap();
        assert_eq!(n, 3, "lines and the adjustment");
        assert_eq!(row(s.budget(), rows[3]).set.as_deref(), Some("Stage A"));
        assert_eq!(row(s.budget(), rows[1]).set, None);

        s.edit("Test edit", |b| {
            assign_value(b, &rows, "location", Some(" Toronto "))
        })
        .unwrap();
        assert_eq!(
            row(s.budget(), rows[0]).location.as_deref(),
            Some("Toronto")
        );

        let n = s
            .edit("Test edit", |b| {
                assign_value(b, &rows, "currency", Some("CAD"))
            })
            .unwrap();
        assert_eq!(n, 2, "only lines have a currency");
        assert_eq!(row(s.budget(), rows[2]).currency.as_deref(), Some("CAD"));
        assert_eq!(row(s.budget(), rows[3]).currency, None);

        // The base currency, blank and null all clear it.
        let before = s.budget().clone();
        assert_eq!(
            s.edit("Test edit", |b| assign_value(
                b,
                &rows,
                "currency",
                Some("USD")
            )),
            Ok(2)
        );
        assert!(details(s.budget()).all(|d| d.currency.is_none()));
        s.undo();
        assert_eq!(s.budget(), &before);
        assert_eq!(
            s.edit("Test edit", |b| assign_value(
                b,
                &rows,
                "currency",
                Some("")
            )),
            Ok(2)
        );
        s.undo();
        assert_eq!(
            s.edit("Test edit", |b| assign_value(b, &rows, "currency", None)),
            Ok(2)
        );

        assert_eq!(
            s.edit("Test edit", |b| assign_value(b, &rows, "set", None)),
            Ok(3)
        );
        assert!(details(s.budget()).all(|d| d.set.is_none()));
        // Nothing left to clear: no change, no undo step.
        let steps = s.can_undo();
        assert_eq!(
            s.edit("Test edit", |b| assign_value(b, &rows, "set", None)),
            Ok(0)
        );
        assert_eq!(s.can_undo(), steps);
    }

    #[test]
    fn bad_assignments_are_refused_and_change_nothing() {
        let mut s = session();
        let rows = mixed(&mut s);
        let before = s.budget().clone();
        let cases: [(&[RowId], &str, Option<&str>, &str); 7] = [
            (&rows, "set", Some("Stage Z"), "no set `Stage Z`"),
            (&rows, "location", Some("Mars"), "no location"),
            (&rows, "currency", Some("EUR"), "exchange rate for EUR"),
            (
                &rows,
                "fringes",
                Some("PAYROLL"),
                "set, location or currency",
            ),
            (&[], "set", Some("Stage A"), "select"),
            (&[rows[0], 999_999], "set", Some("Stage A"), "999999"),
            (&[rows[1], rows[3]], "currency", Some("CAD"), "lines"),
        ];
        for (ids, field, value, want) in cases {
            let err = s
                .edit("Test edit", |b| assign_value(b, ids, field, value))
                .unwrap_err();
            assert!(err.contains(want), "{field} {value:?}: {err}");
            assert_eq!(s.budget(), &before);
        }
        let err = s
            .edit("Test edit", |b| {
                assign_value(b, &[rows[1]], "set", Some("Stage A"))
            })
            .unwrap_err();
        assert!(err.contains("set"), "{err}");
        assert_eq!(s.budget(), &before);
    }

    #[test]
    fn eligibility_matches_the_grid_cells() {
        // The rows that take each tag here are the rows whose grid cell for it
        // can be edited (edit::Field), for every kind of row.
        let mut s = session();
        let rows = mixed(&mut s);
        let b = s.budget().clone();
        for id in rows {
            let d = row(&b, id);
            for (field, takes) in [
                (Field::Fringes, Tag::Fringes.takes(&d)),
                (Field::Groups, Tag::Groups.takes(&d)),
                (Field::Set, Pick::Set.takes(&d)),
                (Field::Location, Pick::Location.takes(&d)),
                (Field::Currency, Pick::Currency.takes(&d)),
            ] {
                let mut c = b.clone();
                let cell = edit::set_detail_field(&mut c, id, field, "X").is_ok();
                assert_eq!(cell, takes, "{:?} on a {:?} row", field, d.kind);
            }
        }
    }

    #[test]
    fn make_range_numbers_a_new_range_in_the_account() {
        let mut s = session();
        let rows = mixed(&mut s); // [line, subtotal, line, adjustment]
        let n = s.edit("Test edit", |b| make_range(b, &rows)).unwrap();
        assert_eq!(n, 1);
        let b = s.budget().clone();
        assert_eq!(row(&b, rows[0]).fringe_range, Some(1));
        assert_eq!(row(&b, rows[2]).fringe_range, Some(1));
        assert_eq!(row(&b, rows[3]).fringe_range, Some(1));
        assert_eq!(
            row(&b, rows[1]).fringe_range,
            None,
            "subtotals never matter"
        );

        // The same rows again: already that range, so nothing changes.
        let undo_before = s.can_undo();
        assert_eq!(s.edit("Test edit", |b| make_range(b, &rows)), Ok(1));
        assert_eq!(s.budget(), &b);
        assert_eq!(s.can_undo(), undo_before);

        // Lines on both sides of an unselected subtotal row still qualify.
        s.undo();
        assert_eq!(
            s.edit("Test edit", |b| make_range(b, &[rows[0], rows[2]])),
            Ok(1)
        );
        assert_eq!(row(s.budget(), rows[3]).fringe_range, None);

        // Part of an existing range becomes a new range with its own number.
        let last = s
            .edit("Test edit", |b| {
                edit::add_detail(b, "20-01", None, RowKind::Line)
            })
            .unwrap();
        let all = [rows.clone(), vec![last]].concat();
        // 1 is taken by the first two lines, so the whole run gets 2.
        assert_eq!(s.edit("Test edit", |b| make_range(b, &all)), Ok(2));
        assert_eq!(s.edit("Test edit", |b| make_range(b, &all[2..])), Ok(3));
        assert_eq!(row(s.budget(), rows[0]).fringe_range, Some(2));
        assert_eq!(row(s.budget(), rows[2]).fringe_range, Some(3));
        assert_eq!(row(s.budget(), rows[3]).fringe_range, Some(3));
        assert_eq!(row(s.budget(), last).fringe_range, Some(3));
    }

    #[test]
    fn a_range_shares_the_fringe_cutoff() {
        // Two lines of 20,000 with a 10% fringe capped at 25,000 of wages:
        // apart, each pays 2,000; as one earner they pay 2,500 between them.
        let mut s = session();
        s.edit("Test edit", |b| {
            b.fringes.insert(
                "CAPPED".into(),
                Fringe {
                    short_code: None,
                    description: String::new(),
                    rate: "10".into(),
                    kind: FringeKind::Percent {
                        cutoff: Some(2_500_000),
                    },
                },
            );
            Ok(())
        })
        .unwrap();
        let first = ids(s.budget(), "50-01")[0];
        let second = s
            .edit("Test edit", |b| {
                edit::add_detail(b, "50-01", Some(first), RowKind::Line)
            })
            .unwrap();
        let rows = [first, second];
        for id in rows {
            for (f, v) in [(Field::Amount, "1"), (Field::Rate, "20000")] {
                s.edit("Test edit", |b| edit::set_detail_field(b, id, f, v))
                    .unwrap();
            }
        }
        s.edit("Test edit", |b| {
            apply_tag(b, &rows, "fringes", "CAPPED", true)
        })
        .unwrap();
        let fringe = |s: &mut Session| s.selection(&rows).fringes["CAPPED"];
        assert_eq!(fringe(&mut s), 400_000);
        s.edit("Test edit", |b| make_range(b, &rows)).unwrap();
        assert_eq!(fringe(&mut s), 250_000);
        s.edit("Test edit", |b| clear_range(b, &rows)).unwrap();
        assert_eq!(fringe(&mut s), 400_000);
    }

    #[test]
    fn bad_ranges_are_refused_and_change_nothing() {
        let mut s = session();
        let rows = mixed(&mut s); // [line, subtotal, line, adjustment]
        let before = s.budget().clone();
        let other = ids(&before, "21-01")[0];
        let cases: [(Vec<RowId>, &str); 5] = [
            (vec![rows[0], other], "one account"),
            (vec![rows[0], rows[3]], "next to each other"),
            (vec![rows[0], rows[1]], "two lines"),
            (vec![], "select"),
            (vec![rows[0], 999_999], "999999"),
        ];
        for (ids, want) in cases {
            let err = s.edit("Test edit", |b| make_range(b, &ids)).unwrap_err();
            assert!(err.contains(want), "{ids:?}: {err}");
            assert_eq!(s.budget(), &before);
        }
        // Nothing to remove.
        let err = s.edit("Test edit", |b| clear_range(b, &rows)).unwrap_err();
        assert!(err.contains("fringe range"), "{err}");
        assert!(s.edit("Test edit", |b| clear_range(b, &[999_999])).is_err());
        assert!(s.edit("Test edit", |b| clear_range(b, &[])).is_err());
        assert_eq!(s.budget(), &before);
    }

    #[test]
    fn remove_range_clears_only_the_selected_rows() {
        let mut s = session();
        let rows = mixed(&mut s);
        s.edit("Test edit", |b| make_range(b, &rows)).unwrap();
        let before = s.budget().clone();
        assert_eq!(s.edit("Test edit", |b| clear_range(b, &rows[2..])), Ok(2));
        assert_eq!(row(s.budget(), rows[0]).fringe_range, Some(1));
        assert_eq!(row(s.budget(), rows[2]).fringe_range, None);
        assert_eq!(row(s.budget(), rows[3]).fringe_range, None);
        assert!(s.undo());
        assert_eq!(s.budget(), &before);
    }

    #[test]
    fn rows_under_reads_rows_accounts_and_categories() {
        let b = budget();
        let cast = ids(&b, "13-01");
        let staff = ids(&b, "20-01");
        let camera = ids(&b, "21-01");
        // Topsheet positions: 13 Cast is at 3, the first production total at 4.
        assert_eq!(rows_under(&b, &[], &[], &[3]), cast);
        assert!(rows_under(&b, &[], &[], &[4, 99]).is_empty());
        assert_eq!(
            rows_under(&b, &[], &["21-01".into(), "20-01".into()], &[]),
            [staff.clone(), camera.clone()].concat(),
            "budget order, not the order asked"
        );
        // Rows, accounts and categories together, without repeats.
        let all = rows_under(&b, &[camera[1], cast[0], 999_999], &["13-01".into()], &[3]);
        assert_eq!(all, [cast.clone(), vec![camera[1]]].concat());
        assert!(rows_under(&b, &[], &["99-99".into()], &[]).is_empty());
    }

    #[test]
    fn effective_percent_rounds_half_away_from_zero() {
        assert_eq!(percent(1_000, 10_000).as_deref(), Some("10"));
        assert_eq!(percent(1_825, 10_000).as_deref(), Some("18.25"));
        assert_eq!(percent(1, 3).as_deref(), Some("33.33"));
        assert_eq!(percent(2, 3).as_deref(), Some("66.67"));
        assert_eq!(percent(1, 40_000).as_deref(), Some("0"));
        assert_eq!(
            percent(5, 100_000).as_deref(),
            Some("0.01"),
            "0.005 rounds up"
        );
        assert_eq!(percent(-5, 100_000).as_deref(), Some("-0.01"));
        assert_eq!(percent(5, 0), None);
    }

    #[test]
    fn the_summary_reflects_the_selected_rows() {
        let mut s = session();
        let rows = mixed(&mut s); // [line, subtotal, line, adjustment]
        s.edit("Test edit", |b| {
            apply_tag(b, &rows[..1], "fringes", "PENSION", true)
        })
        .unwrap();
        s.edit("Test edit", |b| {
            apply_tag(b, &rows[..1], "groups", "Crew", true)
        })
        .unwrap();
        s.edit("Test edit", |b| {
            assign_value(b, &rows[2..3], "set", Some("Stage A"))
        })
        .unwrap();
        s.edit("Test edit", |b| {
            assign_value(b, &rows[2..3], "currency", Some("CAD"))
        })
        .unwrap();
        let b = s.budget().clone();
        let sum = summary(&b, rows.clone(), |ids| s.selection(ids));

        assert_eq!(sum.rows, rows);
        assert_eq!((sum.lines, sum.costed), (2, 3));
        // PAYROLL on both lines, PENSION on one; dollars from selection_totals.
        assert_eq!(sum.fringes["PAYROLL"].count, 2);
        assert_eq!(sum.fringes["PENSION"].count, 1);
        assert_eq!(sum.fringes["PAYROLL"].total, sum.totals.fringes["PAYROLL"]);
        assert!(sum.fringes["PENSION"].total > 0);
        assert_eq!(sum.groups["Crew"].count, 1);
        assert_eq!(
            sum.groups["Crew"].total,
            s.selection(&rows[..1]).lines.details_total
        );
        assert_eq!(sum.sets["Stage A"].count, 1);
        assert_eq!(
            sum.sets["Stage A"].total,
            s.selection(&rows[2..3]).lines.details_total
        );
        assert_eq!(sum.locations["Toronto"], Tally::default());
        assert_eq!(sum.currencies["USD"].count, 1);
        assert_eq!(sum.currencies["CAD"].count, 1);
        assert_eq!(
            sum.currencies["USD"].total + sum.currencies["CAD"].total,
            s.selection(&[rows[0], rows[2]]).lines.details_total
        );
        let t = &sum.totals.lines;
        assert_eq!(sum.fringe_percent, percent(t.fringe_total, t.details_total));
        assert!(sum.fringe_percent.is_some());
        assert_eq!(sum.in_range, 0);
        assert_eq!(sum.range_blocked, None);

        // After making the range: "remove" applies, "make" says why not.
        s.edit("Test edit", |b| make_range(b, &rows)).unwrap();
        let b = s.budget().clone();
        let sum = summary(&b, rows.clone(), |ids| s.selection(ids));
        assert_eq!(sum.in_range, 3);
        assert!(sum.range_blocked.unwrap().contains("already"));

        // Nothing selected.
        let none = summary(&b, Vec::new(), |ids| s.selection(ids));
        assert_eq!((none.lines, none.costed), (0, 0));
        assert_eq!(none.fringe_percent, None);
        assert!(none.range_blocked.is_some());
        assert_eq!(none.fringes["PAYROLL"], Tally::default());
    }

    #[test]
    fn a_suppressed_group_counts_its_lines_but_no_dollars() {
        let mut s = session();
        let hold = serde_json::from_value(serde_json::json!({"in_budget_total": false})).unwrap();
        s.edit("Test edit", |b| edit::set_group(b, "Hold", &hold))
            .unwrap();
        let rows = ids(s.budget(), "13-01");
        s.edit("Test edit", |b| apply_tag(b, &rows, "groups", "Hold", true))
            .unwrap();
        let b = s.budget().clone();
        let sum = summary(&b, rows, |ids| s.selection(ids));
        assert_eq!(sum.groups["Hold"].count, 2);
        assert_eq!(sum.groups["Hold"].total, 0);
        assert_eq!(sum.fringe_percent, None);
    }

    #[test]
    fn the_summary_is_plain_json() {
        let mut s = session();
        let rows = ids(s.budget(), "13-01");
        s.edit("Test edit", |b| {
            b.sets.insert("Stage B".into(), SetDef::default());
            Ok(())
        })
        .unwrap();
        let b = s.budget().clone();
        let v = serde_json::to_value(summary(&b, rows, |ids| s.selection(ids))).unwrap();
        assert!(v["rows"].is_array());
        assert!(v["totals"]["lines"]["details_total"].is_i64());
        assert!(v["fringes"]["PAYROLL"]["count"].is_u64());
        assert!(v["fringes"]["PAYROLL"]["total"].is_i64());
        assert!(v["fringe_percent"].is_string());
        assert!(v["range_blocked"].is_null());
        assert!(v["sets"]["Stage B"]["count"].is_u64());
        fn no_floats(v: &serde_json::Value) {
            match v {
                serde_json::Value::Number(n) => assert!(!n.is_f64(), "float {n}"),
                serde_json::Value::Array(a) => a.iter().for_each(no_floats),
                serde_json::Value::Object(o) => o.values().for_each(no_floats),
                _ => {}
            }
        }
        no_floats(&v);
    }

    #[test]
    fn clear_takes_every_fringe_or_group_off_the_rows_that_carry_them() {
        let mut b = budget();
        let rows = ids(&b, "21-01");
        apply_tag(&mut b, &rows, "fringes", "PENSION", true).unwrap();
        apply_tag(&mut b, &rows, "groups", "Crew", true).unwrap();
        // A name only left on a row (say, from an edited file) goes too.
        b.categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
            .find(|d| d.id == rows[0])
            .unwrap()
            .groups
            .insert("Gone".into());
        let other = ids(&b, "20-01")[0];
        let before_other = row(&b, other);

        assert_eq!(clear_tags(&mut b, &rows, "fringes"), Ok(rows.len()));
        assert!(rows.iter().all(|id| row(&b, *id).fringes.is_empty()));
        assert!(rows.iter().all(|id| row(&b, *id).groups.contains("Crew")));
        assert_eq!(clear_tags(&mut b, &rows, "groups"), Ok(rows.len()));
        assert!(rows.iter().all(|id| row(&b, *id).groups.is_empty()));
        assert_eq!(
            row(&b, other),
            before_other,
            "rows not selected are left alone"
        );
        // Nothing left to clear: no change.
        assert_eq!(clear_tags(&mut b, &rows, "groups"), Ok(0));
    }

    #[test]
    fn bad_clears_are_refused_and_change_nothing() {
        let b0 = budget();
        let rows = ids(&b0, "21-01");
        let mut b = b0.clone();
        assert!(clear_tags(&mut b, &rows, "set").is_err());
        assert!(clear_tags(&mut b, &[], "fringes").is_err());
        assert!(clear_tags(&mut b, &[999_999], "fringes").is_err());
        // A subtotal row takes no fringes.
        let mut s = session();
        let mixed_rows = mixed(&mut s);
        let mut b2 = s.budget().clone();
        assert!(clear_tags(&mut b2, &mixed_rows[1..2], "fringes").is_err());
        assert_eq!(&b2, s.budget());
        assert_eq!(b, b0);
    }

    #[test]
    fn replace_tags_sets_exactly_the_chosen_tags() {
        let mut b = budget();
        let mut s = session();
        let rows = mixed(&mut s); // [line, subtotal, line, adjustment]
        b.clone_from(s.budget());
        apply_tag(&mut b, &rows, "groups", "Crew", true).unwrap();
        assign_value(&mut b, &rows, "location", Some("Toronto")).unwrap();
        let tags = TagsInput {
            fringes: vec!["PENSION".into()],
            groups: Vec::new(),
            set: Some("Stage A".into()),
            location: None,
            currency: Some("CAD".into()),
        };
        assert_eq!(replace_row_tags(&mut b, &rows, &tags), Ok(3));
        for id in [rows[0], rows[2]] {
            let d = row(&b, id);
            assert_eq!(d.fringes, ["PENSION".to_string()].into());
            assert!(d.groups.is_empty() && d.location.is_none());
            assert_eq!(d.set.as_deref(), Some("Stage A"));
            assert_eq!(d.currency.as_deref(), Some("CAD"));
        }
        // The adjustment takes the set but no fringes or currency.
        let adj = row(&b, rows[3]);
        assert!(adj.fringes.is_empty() && adj.currency.is_none());
        assert_eq!(adj.set.as_deref(), Some("Stage A"));
        // The base currency is stored as none; empty tags clear everything.
        let none = TagsInput {
            currency: Some(b.base_currency.clone()),
            ..TagsInput::default()
        };
        replace_row_tags(&mut b, &rows, &none).unwrap();
        let d = row(&b, rows[0]);
        assert!(d.fringes.is_empty() && d.set.is_none() && d.currency.is_none());
    }

    #[test]
    fn replace_tags_refuses_unknown_names_and_changes_nothing() {
        let b0 = budget();
        let rows = ids(&b0, "21-01");
        let mut b = b0.clone();
        for tags in [
            TagsInput {
                fringes: vec!["NOPE".into()],
                ..TagsInput::default()
            },
            TagsInput {
                set: Some("Stage Z".into()),
                ..TagsInput::default()
            },
            TagsInput {
                currency: Some("EUR".into()),
                ..TagsInput::default()
            },
        ] {
            assert!(replace_row_tags(&mut b, &rows, &tags).is_err());
        }
        assert!(replace_row_tags(&mut b, &[], &TagsInput::default()).is_err());
        assert_eq!(b, b0);
        let unknown = serde_json::from_value::<TagsInput>(serde_json::json!({"notes": []}));
        assert!(unknown.is_err(), "unknown fields are refused");
    }

    #[test]
    fn the_summary_splits_fringe_dollars_into_percent_and_flat() {
        let mut b = budget();
        edit::set_fringe(
            &mut b,
            "KIT",
            &serde_json::from_value(serde_json::json!({"kind": "flat", "rate": "25"})).unwrap(),
        )
        .unwrap();
        let rows = ids(&b, "21-01");
        apply_tag(&mut b, &rows, "fringes", "KIT", true).unwrap();
        apply_tag(&mut b, &rows, "fringes", "PENSION", true).unwrap();
        let mut s = Session::with_budget(b.clone(), None);
        let sum = summary(&b, rows.clone(), |ids| s.selection(ids));
        let lines = rows
            .iter()
            .filter(|id| row(&b, **id).kind == RowKind::Line)
            .count();
        assert!(lines > 0);
        assert_eq!(sum.flat_fringes, sum.fringes["KIT"].total);
        assert_eq!(
            sum.percent_fringes,
            sum.fringes["PAYROLL"].total + sum.fringes["PENSION"].total
        );
        assert!(sum.flat_fringes > 0 && sum.percent_fringes > 0);
        assert_eq!(
            sum.percent_fringes + sum.flat_fringes,
            sum.totals.lines.fringe_total
        );
    }

    /// The new commands are registered, listed and allowed where they are
    /// used: the apply edits in the tools window too, the charge preview in
    /// the Budget Setup window (its Charges tab, src/setupwin.rs).
    #[test]
    fn apply_and_charge_preview_commands_are_wired() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN: &str = include_str!("../capabilities/main.json");
        const TOOLS: &str = include_str!("../capabilities/tools.json");
        const SETUP: &str = include_str!("../capabilities/setup.json");
        for (handler, c) in [
            ("crate::apply::clear_from_rows,", "clear_from_rows"),
            (
                "crate::apply::replace_tags_on_rows,",
                "replace_tags_on_rows",
            ),
            ("            preview_charge,", "preview_charge"),
        ] {
            assert!(APP_RS.contains(handler), "{c} not in generate_handler!");
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN.contains(&allow), "{c} not allowed for the main window");
            let (window, caps) = if c == "preview_charge" {
                ("Budget Setup", SETUP)
            } else {
                ("tools", TOOLS)
            };
            assert!(
                caps.contains(&allow),
                "{c} not allowed for the {window} window"
            );
        }
    }
}
