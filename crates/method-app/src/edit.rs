//! Edits the app makes to a [`Budget`]. Each one validates its input and either
//! applies the whole change or returns a message and leaves the budget alone.
//!
//! Validation here is about structure (a name is valid, a code is unique, a row
//! exists). Formulas are stored as typed: a formula that doesn't evaluate is
//! kept and flagged by recalc (see `diagnose`), except that a formula typed
//! into a line or a global is refused when it divides by zero or uses a
//! global that refers back to itself, as the reference product refuses them
//! (decisions.md, entry 60).

use std::collections::{BTreeSet, HashMap};

use method_core::coding::AccountCoding;
use method_core::expr::is_ident;
use method_core::{
    recalc, Account, Baseline, Budget, Category, Charge, ChargeKind, Credit, CreditCap, CreditRow,
    CreditSource, Currency, Dec, Detail, FlatBasis, Formula, Fringe, FringeKind, FringePosting,
    Group, InfoValue, Location, NewLineDefaults, ProductionTotal, RecalcOptions, RowId, RowKind,
    SetDef, TopsheetRow, UnitDef,
};
use serde::Deserialize;

pub type EditResult<T = ()> = Result<T, String>;

/// A detail-row field, named as in the file format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Description,
    Amount,
    Unit,
    X,
    Unit2,
    Rate,
    Unit3,
    X4,
    Unit4,
    Currency,
    Fringes,
    Groups,
    Set,
    Location,
}

impl Field {
    pub fn parse(s: &str) -> EditResult<Field> {
        Ok(match s {
            "description" => Field::Description,
            "amount" => Field::Amount,
            "unit" => Field::Unit,
            "x" => Field::X,
            "unit2" => Field::Unit2,
            "rate" => Field::Rate,
            "unit3" => Field::Unit3,
            "x4" => Field::X4,
            "unit4" => Field::Unit4,
            "currency" => Field::Currency,
            "fringes" => Field::Fringes,
            "groups" => Field::Groups,
            "set" => Field::Set,
            "location" => Field::Location,
            other => return Err(format!("unknown field `{other}`")),
        })
    }

    /// Whether a row of `kind` uses this field.
    fn applies_to(self, kind: RowKind) -> bool {
        match kind {
            RowKind::Line => true,
            RowKind::Subtotal => self == Field::Description,
            // An adjustment row can carry fringes, as in Movie Magic
            // (decisions.md, entry 56).
            RowKind::Adjustment => !matches!(self, Field::Rate | Field::Unit3 | Field::Currency),
            // A drawn rule line shows nothing (style and notes have their own commands).
            RowKind::Separator => false,
        }
    }
}

pub fn parse_kind(s: &str) -> EditResult<RowKind> {
    match s {
        "line" => Ok(RowKind::Line),
        "subtotal" => Ok(RowKind::Subtotal),
        "adjustment" => Ok(RowKind::Adjustment),
        "separator" => Ok(RowKind::Separator),
        other => Err(format!("unknown row kind `{other}`")),
    }
}

fn kind_label(k: RowKind) -> &'static str {
    match k {
        RowKind::Line => "lines",
        RowKind::Subtotal => "subtotal rows",
        RowKind::Adjustment => "adjustment rows",
        RowKind::Separator => "separator rows",
    }
}

/// A comma-separated list of names as a set, ignoring blanks.
pub fn name_list(s: &str) -> BTreeSet<String> {
    s.split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(String::from)
        .collect()
}

fn optional(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Money typed by a user: digits with optional commas and a decimal point,
/// rounded half away from zero to cents. A leading `$` is ignored, and a
/// minus sign or accounting parentheses (`(12.00)`) make it negative.
pub fn parse_money(s: &str) -> EditResult<i64> {
    let t = s.trim();
    let not_amount = |why: &str| format!("`{t}` is not an amount: {why}");
    let (paren, inner) = match t.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        Some(inner) => (true, inner.trim()),
        None => (false, t),
    };
    let (minus, rest) = match inner.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (
            false,
            inner.strip_prefix('+').map_or(inner, str::trim_start),
        ),
    };
    if paren && minus {
        return Err(not_amount("use a minus sign or parentheses, not both"));
    }
    let rest = rest.strip_prefix('$').unwrap_or(rest);
    let cleaned: String = rest
        .chars()
        .filter(|c| *c != ',' && !c.is_whitespace())
        .collect();
    if cleaned.starts_with(['-', '+']) {
        return Err(not_amount("put the sign before the $"));
    }
    let d: Dec = cleaned.parse().map_err(|e| not_amount(&format!("{e}")))?;
    let d = if paren || minus {
        d.checked_neg()
            .ok_or_else(|| format!("`{t}` is too large"))?
    } else {
        d
    };
    d.to_cents().ok_or_else(|| format!("`{t}` is too large"))
}

fn parse_dec(s: &str, what: &str) -> EditResult<Dec> {
    s.trim()
        .parse()
        .map_err(|e| format!("{what} `{}` is not a number: {e}", s.trim()))
}

fn check_name(name: &str, what: &str) -> EditResult<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(format!("{what} name is empty"));
    }
    if n.contains(',') {
        return Err(format!("{what} names can't contain commas"));
    }
    Ok(n.to_string())
}

// ---------------------------------------------------------------------------
// Detail rows

pub fn details(b: &Budget) -> impl Iterator<Item = &Detail> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
}

fn detail_mut(b: &mut Budget, id: RowId) -> EditResult<&mut Detail> {
    b.categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
        .find(|d| d.id == id)
        .ok_or_else(|| row_gone(id))
}

/// The next unused row id: past every row, the baseline's rows and every
/// saved version's rows, so a new row is never mistaken for an old one when
/// comparing.
pub fn next_row_id(b: &Budget) -> RowId {
    b.highest_row_id().unwrap_or(0).saturating_add(1)
}

/// Rows by id, mutably, found in one pass over the budget (not one search per
/// row). Only ids in `want`; when an id repeats in the budget, the first row
/// wins, as with a search.
pub fn rows_by_id<'a>(b: &'a mut Budget, want: &BTreeSet<RowId>) -> HashMap<RowId, &'a mut Detail> {
    let mut out = HashMap::with_capacity(want.len());
    for d in b
        .categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
    {
        if want.contains(&d.id) {
            out.entry(d.id).or_insert(d);
        }
    }
    out
}

/// The error for a row that is gone.
pub fn row_gone(id: RowId) -> String {
    format!("row {id} no longer exists")
}

/// Refuses a field the row's kind doesn't use (a subtotal's rate, say).
pub fn check_field(d: &Detail, field: Field) -> EditResult {
    if field.applies_to(d.kind) {
        Ok(())
    } else {
        Err(format!("{} don't use this column", kind_label(d.kind)))
    }
}

/// Sets one field of one row, as typed.
pub fn apply_field(d: &mut Detail, field: Field, value: &str) -> EditResult {
    check_field(d, field)?;
    let formula = || Formula(value.trim().to_string());
    match field {
        Field::Description => d.description = value.to_string(),
        Field::Amount => d.amount = formula(),
        Field::X => d.x = formula(),
        Field::Rate => d.rate = formula(),
        Field::X4 => d.x4 = formula(),
        Field::Unit => d.unit = value.trim().to_string(),
        Field::Unit2 => d.unit2 = value.trim().to_string(),
        Field::Unit3 => d.unit3 = value.trim().to_string(),
        Field::Unit4 => d.unit4 = value.trim().to_string(),
        Field::Currency => d.currency = optional(value),
        Field::Fringes => d.fringes = name_list(value),
        Field::Groups => d.groups = name_list(value),
        Field::Set => d.set = optional(value),
        Field::Location => d.location = optional(value),
    }
    Ok(())
}

/// Refuses a formula typed into Amt, X, Rate or 4X that divides by zero or
/// uses a global that refers back to itself (decisions.md, entry 60).
fn check_entry(b: &Budget, field: Field, value: &str) -> EditResult {
    if !matches!(field, Field::Amount | Field::X | Field::Rate | Field::X4) {
        return Ok(());
    }
    match method_core::edit::refused_formula(b, value) {
        Some(why) => Err(format!(
            "{} can't be {}: {why}",
            field_label(field),
            value.trim()
        )),
        None => Ok(()),
    }
}

/// How the grid names a formula column.
fn field_label(field: Field) -> &'static str {
    match field {
        Field::Amount => "Amt",
        Field::X => "X",
        Field::Rate => "Rate",
        Field::X4 => "4X",
        _ => "This cell",
    }
}

/// Sets a field, and an empty Unit from the global an Amt names
/// ([`method_core::edit::unit_from_global`]).
fn apply_entry(d: &mut Detail, field: Field, value: &str, unit: Option<&str>) -> EditResult {
    apply_field(d, field, value)?;
    if let Some(u) = unit {
        if d.unit.trim().is_empty() {
            d.unit = u.to_string();
        }
    }
    Ok(())
}

pub fn set_detail_field(b: &mut Budget, id: RowId, field: Field, value: &str) -> EditResult {
    check_entry(b, field, value)?;
    let unit = (field == Field::Amount)
        .then(|| method_core::edit::unit_from_global(b, value).map(str::to_string))
        .flatten();
    apply_entry(detail_mut(b, id)?, field, value, unit.as_deref())
}

/// Sets one field on several rows. Rows that don't use the field are skipped;
/// it's an error only if none do.
pub fn set_rows_field(b: &mut Budget, ids: &[RowId], field: Field, value: &str) -> EditResult {
    check_entry(b, field, value)?;
    let unit = (field == Field::Amount)
        .then(|| method_core::edit::unit_from_global(b, value).map(str::to_string))
        .flatten();
    let mut rows = rows_by_id(b, &ids.iter().copied().collect());
    let mut applied = 0;
    for &id in ids {
        let d = rows.get_mut(&id).ok_or_else(|| row_gone(id))?;
        if field.applies_to(d.kind) {
            apply_entry(d, field, value, unit.as_deref())?;
            applied += 1;
        }
    }
    if applied == 0 && !ids.is_empty() {
        return Err("none of the selected rows use this column".into());
    }
    Ok(())
}

fn account_mut<'a>(b: &'a mut Budget, code: &str) -> EditResult<&'a mut Account> {
    b.categories_mut()
        .flat_map(|c| &mut c.accounts)
        .find(|a| a.code == code)
        .ok_or_else(|| format!("account {code} no longer exists"))
}

/// Inserts a row of `kind` after row `after` (or at the end of the account
/// when `after` is `None`). Returns the new row's id.
///
/// The row is blank, except that a new line copies the tags the budget's
/// [`NewLineDefaults`] ask for (fringes, by default) from its neighbor: see
/// [`copy_from_neighbor`].
pub fn add_detail(
    b: &mut Budget,
    account: &str,
    after: Option<RowId>,
    kind: RowKind,
) -> EditResult<RowId> {
    let id = next_row_id(b);
    let defaults = b.new_line_defaults;
    let acct = account_mut(b, account)?;
    let at = match after {
        None => acct.details.len(),
        Some(after) => {
            acct.details
                .iter()
                .position(|d| d.id == after)
                .ok_or_else(|| format!("row {after} is not in account {account}"))?
                + 1
        }
    };
    let mut row = match kind {
        RowKind::Line => Detail::line(id, "", "", "", ""),
        RowKind::Subtotal => Detail::subtotal(id, ""),
        RowKind::Adjustment => Detail::adjustment(id, "", ""),
        RowKind::Separator => Detail::separator(id),
    };
    if kind == RowKind::Line {
        copy_from_neighbor(&mut row, &acct.details, at, defaults);
    }
    acct.details.insert(at, row);
    Ok(id)
}

/// Gives a line about to be inserted at index `at` of `rows` the tags
/// `defaults` asks for, copied from the nearest line above that index, or
/// below it when there is no line above (subtotals, adjustments and
/// separators are passed over). With no line in the account, nothing changes.
pub fn copy_from_neighbor(row: &mut Detail, rows: &[Detail], at: usize, defaults: NewLineDefaults) {
    let at = at.min(rows.len());
    let is_line = |d: &&Detail| d.kind == RowKind::Line;
    let Some(from) = rows[..at]
        .iter()
        .rev()
        .find(is_line)
        .or_else(|| rows[at..].iter().find(is_line))
    else {
        return;
    };
    if defaults.fringes {
        row.fringes = from.fringes.clone();
    }
    if defaults.groups {
        row.groups = from.groups.clone();
    }
    if defaults.set {
        row.set = from.set.clone();
    }
    if defaults.location {
        row.location = from.location.clone();
    }
    if defaults.currency {
        row.currency = from.currency.clone();
    }
}

/// Deletes rows by id. Returns how many were removed.
pub fn delete_details(b: &mut Budget, ids: &[RowId]) -> EditResult<usize> {
    let ids: BTreeSet<RowId> = ids.iter().copied().collect();
    let mut removed = 0;
    for acct in b.categories_mut().flat_map(|c| &mut c.accounts) {
        let before = acct.details.len();
        acct.details.retain(|d| !ids.contains(&d.id));
        removed += before - acct.details.len();
    }
    if removed == 0 {
        return Err("those rows no longer exist".into());
    }
    Ok(removed)
}

// ---------------------------------------------------------------------------
// Topsheet: categories and production totals

/// The category segment of a typed category number, normalized by the
/// budget's coding (`5` becomes `05`). Falls back to the trimmed text when
/// the budget's own pattern is invalid.
fn category_code(coding: &AccountCoding, typed: &str) -> EditResult<String> {
    if coding.validate().is_err() {
        return optional(typed).ok_or_else(|| "category number is empty".to_string());
    }
    let code = coding.parse(typed).map_err(|e| e.to_string())?;
    Ok(code.category)
}

fn account_code(coding: &AccountCoding, typed: &str) -> EditResult<String> {
    if coding.validate().is_err() {
        return optional(typed).ok_or_else(|| "account number is empty".to_string());
    }
    coding.normalize(typed).map_err(|e| e.to_string())
}

fn category_exists(b: &Budget, code: &str, except: Option<usize>) -> bool {
    b.topsheet
        .iter()
        .enumerate()
        .any(|(i, r)| Some(i) != except && matches!(r, TopsheetRow::Category(c) if c.code == code))
}

fn account_exists(b: &Budget, code: &str, except: Option<&str>) -> bool {
    b.categories()
        .flat_map(|c| &c.accounts)
        .any(|a| a.code == code && Some(a.code.as_str()) != except)
}

fn insert_at(b: &mut Budget, at: Option<usize>, row: TopsheetRow) -> usize {
    let at = at.unwrap_or(b.topsheet.len()).min(b.topsheet.len());
    b.topsheet.insert(at, row);
    // The budget's fringe row stays above the row it was above.
    b.fringe_row.row_inserted(at);
    at
}

/// Adds a category at topsheet position `at` (end when `None`). Returns its index.
pub fn add_category(
    b: &mut Budget,
    code: &str,
    name: &str,
    at: Option<usize>,
) -> EditResult<usize> {
    let code = category_code(&b.account_coding, code)?;
    if category_exists(b, &code, None) {
        return Err(format!("category {code} already exists"));
    }
    let row = TopsheetRow::Category(Category {
        notes: Vec::new(),
        code,
        name: name.trim().to_string(),
        accounts: Vec::new(),
    });
    Ok(insert_at(b, at, row))
}

pub fn add_production_total(b: &mut Budget, name: &str, at: Option<usize>) -> EditResult<usize> {
    let name = optional(name).ok_or("production total name is empty")?;
    Ok(insert_at(
        b,
        at,
        TopsheetRow::ProductionTotal(ProductionTotal { name }),
    ))
}

/// Renames (and for categories renumbers) a topsheet row. Accounts stay put.
pub fn update_topsheet_row(
    b: &mut Budget,
    index: usize,
    code: Option<&str>,
    name: &str,
) -> EditResult {
    let new_code = match (b.topsheet.get(index), code) {
        (None, _) => return Err("that topsheet row no longer exists".into()),
        // The number as it is stays as it is, even one the pattern can't
        // read (a chart's own numbers may not follow it).
        (Some(TopsheetRow::Category(c)), Some(typed)) if typed.trim() == c.code => None,
        (Some(TopsheetRow::Category(_)), Some(typed)) => {
            let code = category_code(&b.account_coding, typed)?;
            if category_exists(b, &code, Some(index)) {
                return Err(format!("category {code} already exists"));
            }
            Some(code)
        }
        _ => None,
    };
    match &mut b.topsheet[index] {
        TopsheetRow::Category(c) => {
            if let Some(code) = new_code {
                c.code = code;
            }
            c.name = name.trim().to_string();
        }
        TopsheetRow::ProductionTotal(p) => {
            p.name = optional(name).ok_or("production total name is empty")?;
        }
    }
    Ok(())
}

/// Removes a topsheet row; a category takes its accounts and rows with it.
pub fn delete_topsheet_row(b: &mut Budget, index: usize) -> EditResult {
    if index >= b.topsheet.len() {
        return Err("that topsheet row no longer exists".into());
    }
    b.topsheet.remove(index);
    b.fringe_row.row_removed(index);
    Ok(())
}

// ---------------------------------------------------------------------------
// Accounts

/// Adds an account to category `category`, after account `after` (or at the
/// end). The number is normalized by the budget's coding and must be unique.
pub fn add_account(
    b: &mut Budget,
    category: &str,
    code: &str,
    name: &str,
    after: Option<&str>,
) -> EditResult<String> {
    let code = account_code(&b.account_coding, code)?;
    if account_exists(b, &code, None) {
        return Err(format!("account {code} already exists"));
    }
    let cat = b
        .categories_mut()
        .find(|c| c.code == category)
        .ok_or_else(|| format!("category {category} no longer exists"))?;
    let at = match after {
        None => cat.accounts.len(),
        Some(a) => {
            cat.accounts
                .iter()
                .position(|x| x.code == a)
                .ok_or_else(|| format!("account {a} is not in category {category}"))?
                + 1
        }
    };
    cat.accounts.insert(
        at,
        Account {
            notes: Vec::new(),
            code: code.clone(),
            name: name.trim().to_string(),
            details: Vec::new(),
        },
    );
    Ok(code)
}

pub fn update_account(
    b: &mut Budget,
    code: &str,
    new_code: &str,
    name: &str,
) -> EditResult<String> {
    // The number as it is stays as it is, even one the pattern can't read
    // (a chart's own numbers may not follow it).
    let new_code = if new_code.trim() == code {
        code.to_string()
    } else {
        account_code(&b.account_coding, new_code)?
    };
    if new_code != code && account_exists(b, &new_code, Some(code)) {
        return Err(format!("account {new_code} already exists"));
    }
    let acct = account_mut(b, code)?;
    acct.code = new_code.clone();
    acct.name = name.trim().to_string();
    Ok(new_code)
}

pub fn delete_account(b: &mut Budget, code: &str) -> EditResult {
    for cat in b.categories_mut() {
        if let Some(i) = cat.accounts.iter().position(|a| a.code == code) {
            cat.accounts.remove(i);
            return Ok(());
        }
    }
    Err(format!("account {code} no longer exists"))
}

// ---------------------------------------------------------------------------
// Budget-wide settings and setup tables

pub fn set_title(b: &mut Budget, title: &str) -> EditResult {
    b.title = title.trim().to_string();
    Ok(())
}

pub fn set_global(b: &mut Budget, name: &str, formula: &str) -> EditResult {
    let name = name.trim();
    if !is_ident(name) {
        return Err(format!(
            "`{name}` can't be a global name: use letters, digits and _, with at least one letter or _"
        ));
    }
    let formula = formula.trim();
    if formula.is_empty() {
        return Err(format!("global {name} needs a value"));
    }
    // Refused as the reference product refuses it (decisions.md, entry 60).
    if let Some(why) = method_core::edit::refused_global(b, name, formula) {
        return Err(format!("global {name} can't be {formula}: {why}"));
    }
    b.globals
        .insert(name.to_string(), Formula(formula.to_string()));
    Ok(())
}

/// Deletes a global, freezing its value into every formula that used it. If the
/// globals can't be evaluated (say this one is broken), it is just removed and
/// any formula still using it gets flagged.
pub fn delete_global(b: &mut Budget, name: &str) -> EditResult {
    if !b.globals.contains_key(name) {
        return Err(format!("global {name} no longer exists"));
    }
    if method_core::delete_global(b, name).is_err() {
        b.globals.remove(name);
        b.global_info.remove(name);
    }
    method_core::order::forget(b, method_core::SetupKind::Global, name);
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
pub struct FringeInput {
    /// `percent` or `flat`.
    pub kind: String,
    #[serde(default)]
    pub description: String,
    pub rate: String,
    /// Cutoff as money text, blank for none. Used by percent fringes; a flat
    /// fringe keeps it but never uses it.
    #[serde(default)]
    pub cutoff: Option<String>,
    /// Flat fringes: unit name, blank for none.
    #[serde(default)]
    pub unit: Option<String>,
    /// Flat fringes: what a line counts, `amount` (blank), `amount_counts`
    /// or `quantity` (decisions.md, entry 54).
    #[serde(default)]
    pub basis: Option<String>,
    /// Flat fringes charged per hour: most hours a day, blank for no cap.
    #[serde(default)]
    pub max_hours_per_day: Option<String>,
    /// Short code: missing keeps the fringe's own, `""` clears it.
    #[serde(default)]
    pub short_code: Option<String>,
}

/// Longest short code, in characters.
pub const MAX_SHORT_CODE_CHARS: usize = 16;

/// A short code for item `name` of `kind` (fringe or group), as typed:
/// `None` when blank. Refused when it has a comma (lines list items
/// separated by commas), is too long, or is another item's code (ignoring
/// case) or name.
pub fn check_short_code(
    b: &Budget,
    kind: method_core::SetupKind,
    name: &str,
    typed: &str,
) -> EditResult<Option<String>> {
    let code = typed.trim();
    if code.is_empty() {
        return Ok(None);
    }
    let what = kind.name();
    if code.contains(',') {
        return Err("short codes can't contain commas".into());
    }
    let len = code.chars().count();
    if len > MAX_SHORT_CODE_CHARS {
        return Err(format!(
            "that short code is {len} characters; short codes can be at most {MAX_SHORT_CODE_CHARS}"
        ));
    }
    let others: Vec<(&String, Option<&String>)> = match kind {
        method_core::SetupKind::Fringe => b
            .fringes
            .iter()
            .map(|(n, f)| (n, f.short_code.as_ref()))
            .collect(),
        method_core::SetupKind::Group => b
            .groups
            .iter()
            .map(|(n, g)| (n, g.short_code.as_ref()))
            .collect(),
        _ => return Err(format!("a {what} has no short code")),
    };
    for (other, their) in others {
        if other == name {
            continue;
        }
        if their.is_some_and(|c| c.to_lowercase() == code.to_lowercase()) {
            return Err(format!(
                "{code} is already the short code of the {what} {other}"
            ));
        }
        if other == code {
            return Err(format!("{code} is the name of another {what}"));
        }
    }
    Ok(Some(code.to_string()))
}

/// Sets or clears (blank) the short code of fringe or group `name`.
pub fn set_short_code(
    b: &mut Budget,
    kind: method_core::SetupKind,
    name: &str,
    typed: &str,
) -> EditResult {
    let code = check_short_code(b, kind, name, typed)?;
    let slot = match kind {
        method_core::SetupKind::Fringe => b.fringes.get_mut(name).map(|f| &mut f.short_code),
        method_core::SetupKind::Group => b.groups.get_mut(name).map(|g| &mut g.short_code),
        _ => return Err(format!("a {} has no short code", kind.name())),
    };
    let slot = slot.ok_or_else(|| format!("{} {name} no longer exists", kind.name()))?;
    *slot = code;
    Ok(())
}

pub fn set_fringe(b: &mut Budget, name: &str, f: &FringeInput) -> EditResult {
    let name = check_name(name, "fringe")?;
    let cutoff = match f.cutoff.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(text) => {
            let c = parse_money(text)?;
            if c < 0 {
                return Err("a cutoff can't be negative".into());
            }
            Some(c)
        }
    };
    let kind = match f.kind.as_str() {
        "percent" => FringeKind::Percent { cutoff },
        "flat" => {
            let basis = match f.basis.as_deref().map(str::trim) {
                None | Some("") => FlatBasis::Amount,
                Some(text) => FlatBasis::parse(text)
                    .ok_or_else(|| format!("unknown flat fringe basis `{text}`"))?,
            };
            let max_hours_per_day = match f.max_hours_per_day.as_deref().map(str::trim) {
                None | Some("") => None,
                Some(text) => {
                    let h = parse_dec(text, "max hours per day")?;
                    if h <= Dec::ZERO {
                        return Err("max hours per day must be more than 0".into());
                    }
                    Some(h)
                }
            };
            // The cutoff is kept as it is, never used: cutoffs apply to
            // percent fringes only.
            FringeKind::Flat {
                unit: f.unit.as_deref().and_then(optional),
                basis,
                max_hours_per_day,
                cutoff,
            }
        }
        other => return Err(format!("unknown fringe kind `{other}`")),
    };
    let short_code = match f.short_code.as_deref() {
        Some(typed) => check_short_code(b, method_core::SetupKind::Fringe, &name, typed)?,
        None => b.fringes.get(&name).and_then(|old| old.short_code.clone()),
    };
    b.fringes.insert(
        name,
        Fringe {
            short_code,
            description: f.description.trim().to_string(),
            rate: Formula(f.rate.trim().to_string()),
            kind,
        },
    );
    Ok(())
}

/// Deletes a fringe and takes it off every row that used it.
pub fn delete_fringe(b: &mut Budget, name: &str) -> EditResult {
    if b.fringes.remove(name).is_none() {
        return Err(format!("fringe {name} no longer exists"));
    }
    for acct in b.categories_mut().flat_map(|c| &mut c.accounts) {
        for d in &mut acct.details {
            d.fringes.remove(name);
        }
    }
    for credit in &mut b.credits {
        credit.excluded_fringes.remove(name);
    }
    method_core::order::forget(b, method_core::SetupKind::Fringe, name);
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
pub struct GroupInput {
    #[serde(default)]
    pub description: String,
    pub in_budget_total: bool,
    /// Short code: missing keeps the group's own, `""` clears it.
    #[serde(default)]
    pub short_code: Option<String>,
}

pub fn set_group(b: &mut Budget, name: &str, g: &GroupInput) -> EditResult {
    let name = check_name(name, "group")?;
    // Color has its own command (rowstyle::set_group_color); keep it here.
    let color = b.groups.get(&name).and_then(|old| old.color.clone());
    let short_code = match g.short_code.as_deref() {
        Some(typed) => check_short_code(b, method_core::SetupKind::Group, &name, typed)?,
        None => b.groups.get(&name).and_then(|old| old.short_code.clone()),
    };
    b.groups.insert(
        name,
        Group {
            short_code,
            description: g.description.trim().to_string(),
            in_budget_total: g.in_budget_total,
            color,
        },
    );
    Ok(())
}

/// Sets one group rule (decisions.md, entry 57): `ungrouped_lines`,
/// `conflict` or `fringes_in_totals`, to `include` or `exclude`.
pub fn set_group_rule(b: &mut Budget, rule: &str, value: &str) -> EditResult {
    let v = method_core::Inclusion::parse(value)
        .ok_or_else(|| format!("`{value}` is not include or exclude"))?;
    let r = &mut b.group_rules;
    match rule {
        "ungrouped_lines" => r.ungrouped_lines = v,
        "conflict" => r.conflict = v,
        "fringes_in_totals" => r.fringes_in_totals = v,
        _ => return Err(format!("`{rule}` is not a group rule")),
    }
    Ok(())
}

/// Sets how the budget works out its totals (decisions.md, entry 55):
/// `binary` or `exact`.
pub fn set_arithmetic(b: &mut Budget, value: &str) -> EditResult {
    b.arithmetic = method_core::Arithmetic::parse(value)
        .ok_or_else(|| format!("`{value}` is not binary or exact"))?;
    Ok(())
}

/// Turns rounding account totals to whole units on or off (decisions.md,
/// entry 69).
pub fn set_round_account_totals(b: &mut Budget, on: bool) -> EditResult {
    b.round_account_totals = on;
    Ok(())
}

/// Sets what every percent fringe's cutoff is multiplied by (decisions.md,
/// entry 60): `1.0`, `1.5`, `2.0`, `2.5` or `3.0`.
pub fn set_cutoff_factor(b: &mut Budget, value: &str) -> EditResult {
    b.cutoff_factor = method_core::CutoffFactor::parse(value).ok_or_else(|| {
        format!(
            "`{}` is not a cutoff factor: use 1.0, 1.5, 2.0, 2.5 or 3.0",
            value.trim()
        )
    })?;
    Ok(())
}

/// The budget's fringe row as the Settings tab sends it (decisions.md,
/// entry 60).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct FringeRowInput {
    /// Account number shown on the row; blank for none.
    #[serde(default)]
    pub account: Option<String>,
    /// What the row says; blank for "Total Fringes".
    #[serde(default)]
    pub description: Option<String>,
    /// The topsheet row it shows above (the topsheet's length: after the
    /// last row); `None` for after the summary rows.
    #[serde(default)]
    pub position: Option<usize>,
}

/// Longest fringe row description, in characters.
pub const MAX_FRINGE_ROW_CHARS: usize = 200;

/// Sets the budget's fringe row: its account number, description and place
/// on the topsheet (decisions.md, entry 60). Display only: no total
/// changes. The number may be any letters, digits and separators, like a
/// charge's account.
pub fn set_fringe_row(b: &mut Budget, r: &FringeRowInput) -> EditResult {
    let blank = |s: &Option<String>| {
        s.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let account = blank(&r.account);
    if let Some(a) = &account {
        if let Some(c) = a
            .chars()
            .find(|c| !c.is_ascii_alphanumeric() && !method_core::coding::SEPARATORS.contains(c))
        {
            return Err(format!(
                "the fringe row's account number can only use letters, digits and separators, not {c:?}"
            ));
        }
    }
    let description = blank(&r.description);
    if description
        .as_ref()
        .is_some_and(|d| d.chars().count() > MAX_FRINGE_ROW_CHARS)
    {
        return Err(format!(
            "the fringe row's description is longer than {MAX_FRINGE_ROW_CHARS} characters"
        ));
    }
    if r.position.is_some_and(|p| p > b.topsheet.len()) {
        return Err("that topsheet row no longer exists".into());
    }
    b.fringe_row = method_core::FringeRow {
        account,
        description,
        position: r.position,
    };
    Ok(())
}

/// Sets what happens to the fringes of categories after the last production
/// total (decisions.md, entry 56): `keep` or `drop`.
pub fn set_trailing_level_fringes(b: &mut Budget, value: &str) -> EditResult {
    b.trailing_level_fringes = method_core::TrailingFringes::parse(value)
        .ok_or_else(|| format!("`{value}` is not keep or drop"))?;
    Ok(())
}

/// Deletes a group and untags every row. Refused while a credit is based on it.
pub fn delete_group(b: &mut Budget, name: &str) -> EditResult {
    if !b.groups.contains_key(name) {
        return Err(format!("group {name} no longer exists"));
    }
    let used_by = b.credits.iter().find(|c| {
        c.rows
            .iter()
            .any(|r| matches!(&r.source, CreditSource::Group { group, .. } if group == name))
    });
    if let Some(c) = used_by {
        return Err(format!(
            "group {name} is used by the credit {}; change the credit first",
            c.name
        ));
    }
    b.groups.remove(name);
    for acct in b.categories_mut().flat_map(|c| &mut c.accounts) {
        for d in &mut acct.details {
            d.groups.remove(name);
        }
    }
    for charge in &mut b.charges {
        charge.excluded_groups.remove(name);
    }
    method_core::order::forget(b, method_core::SetupKind::Group, name);
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
pub struct UnitInput {
    #[serde(default)]
    pub plural: String,
    #[serde(default)]
    pub description: String,
    /// Hours in one of this unit, blank for non-time units.
    #[serde(default)]
    pub hours: Option<String>,
}

pub fn set_unit(b: &mut Budget, name: &str, u: &UnitInput) -> EditResult {
    let name = check_name(name, "unit")?;
    let hours = match u.hours.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(text) => {
            let h = parse_dec(text, "hours")?;
            if h < Dec::ZERO {
                return Err("hours can't be negative".into());
            }
            Some(h)
        }
    };
    b.units.insert(
        name,
        UnitDef {
            plural: u.plural.trim().to_string(),
            description: u.description.trim().to_string(),
            hours,
        },
    );
    Ok(())
}

/// Deletes a unit definition. Rows keep their unit text; it just no longer
/// converts for flat fringes.
pub fn delete_unit(b: &mut Budget, name: &str) -> EditResult {
    delete_unit_with(b, name, false)
}

/// Deletes a unit definition; with `clear_lines`, also blanks every unit
/// column on a row that names it (so nothing is left naming a unit that
/// isn't there). As one change.
pub fn delete_unit_with(b: &mut Budget, name: &str, clear_lines: bool) -> EditResult {
    if b.units.remove(name).is_none() {
        return Err(format!("unit {name} no longer exists"));
    }
    if clear_lines {
        for d in b
            .categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
        {
            for u in [&mut d.unit, &mut d.unit2, &mut d.unit3, &mut d.unit4] {
                if u == name {
                    u.clear();
                }
            }
        }
    }
    method_core::order::forget(b, method_core::SetupKind::Unit, name);
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
pub struct CurrencyInput {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub symbol: String,
    /// Base-currency units per one unit of this currency.
    pub rate: String,
    /// The symbol after the value (`1.234,56 €`). Left out: keep the
    /// currency's own (before, for a new one; decisions.md, entry 61).
    #[serde(default)]
    pub symbol_after: Option<bool>,
    /// `comma_point` (`1,234.56`) or `point_comma` (`1.234,56`). Left out:
    /// keep the currency's own (`comma_point` for a new one).
    #[serde(default)]
    pub separators: Option<String>,
}

pub fn set_currency(b: &mut Budget, code: &str, c: &CurrencyInput) -> EditResult {
    let code = check_name(code, "currency")?;
    if code == b.base_currency {
        return Err(format!("{code} is the base currency"));
    }
    let rate = parse_dec(&c.rate, "rate")?;
    if rate <= Dec::ZERO {
        return Err("an exchange rate must be more than 0".into());
    }
    let old = b
        .currencies
        .get(&code)
        .map(|o| o.format)
        .unwrap_or_default();
    let separators = match c.separators.as_deref() {
        None => old.separators,
        Some(s) => method_core::Separators::parse(s).ok_or_else(|| {
            format!(
                "separators should be 1,234.56 or 1.234,56, not `{}`",
                s.trim()
            )
        })?,
    };
    b.currencies.insert(
        code,
        Currency {
            description: c.description.trim().to_string(),
            symbol: c.symbol.trim().to_string(),
            rate,
            format: method_core::CurrencyFormat {
                symbol_after: c.symbol_after.unwrap_or(old.symbol_after),
                separators,
            },
        },
    );
    Ok(())
}

/// Deletes a currency. Refused while lines, charges or the totals use it.
pub fn delete_currency(b: &mut Budget, code: &str) -> EditResult {
    delete_currency_with(b, code, false)
}

/// Deletes a currency; with `move_lines`, lines in it go back to the base
/// currency first (as one change). Still refused while charges or the
/// totals use it.
pub fn delete_currency_with(b: &mut Budget, code: &str, move_lines: bool) -> EditResult {
    if !b.currencies.contains_key(code) {
        return Err(format!("currency {code} no longer exists"));
    }
    if move_lines {
        for d in b
            .categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
        {
            if d.currency.as_deref() == Some(code) {
                d.currency = None;
            }
        }
    }
    let lines = details(b)
        .filter(|d| d.currency.as_deref() == Some(code))
        .count();
    if lines > 0 {
        let s = if lines == 1 { "line uses" } else { "lines use" };
        return Err(format!("{lines} {s} {code}; change them first"));
    }
    if b.totals_currency.as_deref() == Some(code) {
        return Err(format!("totals are shown in {code}; change that first"));
    }
    let charged = b.charges.iter().any(|c| {
        matches!(&c.kind, method_core::ChargeKind::Flat { currency: Some(cur), .. } if cur == code)
    });
    if charged {
        return Err(format!("a charge is in {code}; change it first"));
    }
    b.currencies.remove(code);
    method_core::order::forget(b, method_core::SetupKind::Currency, code);
    Ok(())
}

// ---------------------------------------------------------------------------
// Charges, credits, sets, locations, budget info, settings and the baseline

/// Money text that must not be negative.
fn money_at_least_zero(s: &str, what: &str) -> EditResult<i64> {
    let c = parse_money(s)?;
    if c < 0 {
        return Err(format!("{what} can't be negative"));
    }
    Ok(c)
}

/// Optional money text: missing or blank is `None`.
fn optional_money(s: Option<&str>, what: &str) -> EditResult<Option<i64>> {
    match s.map(str::trim) {
        None | Some("") => Ok(None),
        Some(t) => money_at_least_zero(t, what).map(Some),
    }
}

/// A percentage that must not be negative.
fn percent(s: &str, what: &str) -> EditResult<Dec> {
    if s.trim().is_empty() {
        return Err(format!("{what} needs a value"));
    }
    let p = parse_dec(s, what)?;
    if p < Dec::ZERO {
        return Err(format!("{what} can't be negative"));
    }
    Ok(p)
}

/// Names picked from a list: each must be `known` (or already in `kept`, so an
/// item that refers to something since removed can still be edited).
fn picked_names(
    names: &[String],
    known: &BTreeSet<String>,
    kept: Option<&BTreeSet<String>>,
    what: &str,
) -> EditResult<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for n in names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        if !known.contains(n) && !kept.is_some_and(|k| k.contains(n)) {
            return Err(format!("there is no {what} `{n}`"));
        }
        out.insert(n.to_string());
    }
    Ok(out)
}

/// An optional account number: one already in the budget exactly as typed,
/// else normalized by the budget's coding.
fn optional_account(b: &Budget, typed: Option<&str>) -> EditResult<Option<String>> {
    match typed.map(str::trim) {
        None | Some("") => Ok(None),
        Some(t) if account_exists(b, t, None) => Ok(Some(t.to_string())),
        Some(t) => account_code(&b.account_coding, t).map(Some),
    }
}

/// A foreign currency a flat amount is in: blank or the base currency is `None`.
fn amount_currency(b: &Budget, typed: Option<&str>) -> EditResult<Option<String>> {
    match typed.map(str::trim) {
        None | Some("") => Ok(None),
        Some(c) if c == b.base_currency => Ok(None),
        Some(c) if b.currencies.contains_key(c) => Ok(Some(c.to_string())),
        Some(c) => Err(format!(
            "there is no exchange rate for {c}; add it under Currencies first"
        )),
    }
}

fn check_index(len: usize, index: usize, what: &str) -> EditResult {
    if index < len {
        Ok(())
    } else {
        Err(format!("that {what} no longer exists"))
    }
}

/// Moves item `index` to position `to` (both positions in the current list).
fn move_item<T>(list: &mut Vec<T>, index: usize, to: usize, what: &str) -> EditResult {
    check_index(list.len(), index, what)?;
    if to >= list.len() {
        return Err(format!("can't move the {what} there"));
    }
    let item = list.remove(index);
    list.insert(to, item);
    Ok(())
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
pub struct ExcludedAmountInput {
    #[serde(default)]
    pub description: String,
    /// Money text.
    pub amount: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChargeInput {
    pub name: String,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default = "yes")]
    pub applied: bool,
    /// `percent` or `flat`.
    pub kind: String,
    /// Percent charges: `10` is 10%.
    #[serde(default)]
    pub percent: Option<String>,
    /// Flat charges: money text.
    #[serde(default)]
    pub amount: Option<String>,
    /// Flat charges: currency code, blank for the base currency.
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub excluded_amounts: Vec<ExcludedAmountInput>,
    #[serde(default)]
    pub excluded_groups: Vec<String>,
    #[serde(default)]
    pub excluded_categories: Vec<String>,
    #[serde(default)]
    pub excluded_accounts: Vec<String>,
    /// Show the percent in the topsheet label. Left out: keep the charge's
    /// own setting (off for a new charge).
    #[serde(default)]
    pub label_percent: Option<bool>,
    /// Show the excluded total in the topsheet label. Left out: keep it.
    #[serde(default)]
    pub label_excluded: Option<bool>,
}

/// Replaces charge `index`, or appends when `None`. Returns its index.
pub fn set_charge(b: &mut Budget, index: Option<usize>, c: &ChargeInput) -> EditResult<usize> {
    let charge = build_charge(b, index, c)?;
    Ok(match index {
        Some(i) => {
            b.charges[i] = charge;
            i
        }
        None => {
            b.charges.push(charge);
            b.charges.len() - 1
        }
    })
}

/// The charge `c` describes, checked as [`set_charge`] checks it, without
/// changing the budget: as an edit of charge `index` (whose exclusions of
/// items the budget no longer has may stay), or a new one when `None`.
pub fn build_charge(b: &Budget, index: Option<usize>, c: &ChargeInput) -> EditResult<Charge> {
    let old = match index {
        Some(i) => {
            check_index(b.charges.len(), i, "charge")?;
            Some(b.charges[i].clone())
        }
        None => None,
    };
    let name = c.name.trim();
    if name.is_empty() {
        return Err("charge name is empty".into());
    }
    if b.charges
        .iter()
        .enumerate()
        .any(|(i, x)| Some(i) != index && x.name == name)
    {
        return Err(format!("there is already a charge named {name}"));
    }
    let kind = match c.kind.as_str() {
        "percent" => {
            ChargeKind::Percent(percent(c.percent.as_deref().unwrap_or(""), "the percent")?)
        }
        "flat" => {
            let text = c.amount.as_deref().unwrap_or("").trim();
            if text.is_empty() {
                return Err("a flat charge needs an amount".into());
            }
            ChargeKind::Flat {
                amount: money_at_least_zero(text, "the amount")?,
                currency: amount_currency(b, c.currency.as_deref())?,
            }
        }
        other => return Err(format!("unknown charge kind `{other}`")),
    };
    let mut excluded_amounts = Vec::with_capacity(c.excluded_amounts.len());
    for x in &c.excluded_amounts {
        excluded_amounts.push((x.description.trim().to_string(), parse_money(&x.amount)?));
    }
    let groups: BTreeSet<String> = b.groups.keys().cloned().collect();
    let categories: BTreeSet<String> = b.categories().map(|c| c.code.clone()).collect();
    let accounts: BTreeSet<String> = b
        .categories()
        .flat_map(|c| &c.accounts)
        .map(|a| a.code.clone())
        .collect();
    Ok(Charge {
        name: name.to_string(),
        account: optional_account(b, c.account.as_deref())?,
        applied: c.applied,
        kind,
        excluded_amounts,
        excluded_groups: picked_names(
            &c.excluded_groups,
            &groups,
            old.as_ref().map(|o| &o.excluded_groups),
            "group",
        )?,
        excluded_categories: picked_names(
            &c.excluded_categories,
            &categories,
            old.as_ref().map(|o| &o.excluded_categories),
            "category",
        )?,
        excluded_accounts: picked_names(
            &c.excluded_accounts,
            &accounts,
            old.as_ref().map(|o| &o.excluded_accounts),
            "account",
        )?,
        label_percent: c
            .label_percent
            .unwrap_or_else(|| old.as_ref().is_some_and(|o| o.label_percent)),
        label_excluded: c
            .label_excluded
            .unwrap_or_else(|| old.as_ref().is_some_and(|o| o.label_excluded)),
    })
}

pub fn delete_charge(b: &mut Budget, index: usize) -> EditResult {
    check_index(b.charges.len(), index, "charge")?;
    b.charges.remove(index);
    Ok(())
}

/// Charges apply in order, so their order matters.
pub fn move_charge(b: &mut Budget, index: usize, to: usize) -> EditResult {
    move_item(&mut b.charges, index, to, "charge")
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreditSourceInput {
    /// A gross amount typed in, as money text.
    Flat { amount: String },
    /// A group's lines with their fringes, less the credit's excluded ones.
    Group { group: String },
    /// A topsheet category's total with its fringes, less the credit's
    /// excluded ones (decisions.md, entry 57).
    Category { category: String },
    /// An account's total with its fringes, less the credit's excluded
    /// ones (decisions.md, entry 61).
    Account { account: String },
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreditCapInput {
    #[default]
    None,
    /// Caps the spend the percent applies to (money text).
    QualifiedSpend { amount: String },
    /// Caps the row's credit (money text).
    Credit { amount: String },
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreditRowInput {
    #[serde(default)]
    pub description: String,
    pub source: CreditSourceInput,
    /// `25` is 25% of the row's spend.
    pub percent: String,
    #[serde(default)]
    pub cap: CreditCapInput,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreditInput {
    pub name: String,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default = "yes")]
    pub applied: bool,
    /// Ceiling on the sum of the row credits, as money text; blank for none.
    #[serde(default)]
    pub cap: Option<String>,
    /// Taken off after the cap; `5` is 5%. Blank means 0.
    #[serde(default)]
    pub discount_percent: Option<String>,
    /// Fringes left out of every group and category row's spend: one list
    /// per credit (decisions.md, entry 57).
    #[serde(default)]
    pub excluded_fringes: Vec<String>,
    #[serde(default)]
    pub rows: Vec<CreditRowInput>,
}

/// Replaces credit `index`, or appends when `None`. Returns its index.
pub fn set_credit(b: &mut Budget, index: Option<usize>, c: &CreditInput) -> EditResult<usize> {
    let old = match index {
        Some(i) => {
            check_index(b.credits.len(), i, "credit")?;
            Some(b.credits[i].clone())
        }
        None => None,
    };
    let name = c.name.trim();
    if name.is_empty() {
        return Err("credit name is empty".into());
    }
    if b.credits
        .iter()
        .enumerate()
        .any(|(i, x)| Some(i) != index && x.name == name)
    {
        return Err(format!("there is already a credit named {name}"));
    }
    let discount_percent = match c.discount_percent.as_deref().map(str::trim) {
        None | Some("") => Dec::ZERO,
        Some(t) => {
            let d = percent(t, "the discount")?;
            if d > Dec::from_int(100) {
                return Err("the discount can't be more than 100%".into());
            }
            d
        }
    };
    // Old rows' references stay valid while editing, like charge exclusions.
    let mut old_groups = BTreeSet::new();
    let mut old_categories = BTreeSet::new();
    let mut old_accounts = BTreeSet::new();
    let old_fringes = old
        .as_ref()
        .map(|o| o.excluded_fringes.clone())
        .unwrap_or_default();
    for r in old.iter().flat_map(|o| &o.rows) {
        match &r.source {
            CreditSource::Group { group } => {
                old_groups.insert(group.clone());
            }
            CreditSource::Category { category } => {
                old_categories.insert(category.clone());
            }
            CreditSource::Account { account } => {
                old_accounts.insert(account.clone());
            }
            CreditSource::Flat(_) => {}
        }
    }
    let groups: BTreeSet<String> = b.groups.keys().cloned().collect();
    let categories: BTreeSet<String> = b.categories().map(|c| c.code.clone()).collect();
    let accounts: BTreeSet<String> = b
        .categories()
        .flat_map(|c| &c.accounts)
        .map(|a| a.code.clone())
        .collect();
    let fringes: BTreeSet<String> = b.fringes.keys().cloned().collect();
    let mut rows = Vec::with_capacity(c.rows.len());
    for (i, r) in c.rows.iter().enumerate() {
        let n = i + 1;
        let source = match &r.source {
            CreditSourceInput::Flat { amount } => {
                CreditSource::Flat(money_at_least_zero(amount, &format!("row {n}'s amount"))?)
            }
            CreditSourceInput::Group { group } => {
                let g = group.trim();
                if g.is_empty() {
                    return Err(format!("row {n} needs a group"));
                }
                if !groups.contains(g) && !old_groups.contains(g) {
                    return Err(format!("there is no group `{g}`"));
                }
                CreditSource::Group {
                    group: g.to_string(),
                }
            }
            CreditSourceInput::Category { category } => {
                let c = category.trim();
                if c.is_empty() {
                    return Err(format!("row {n} needs a category"));
                }
                if !categories.contains(c) && !old_categories.contains(c) {
                    return Err(format!("there is no category `{c}`"));
                }
                CreditSource::Category {
                    category: c.to_string(),
                }
            }
            CreditSourceInput::Account { account } => {
                let a = account.trim();
                if a.is_empty() {
                    return Err(format!("row {n} needs an account"));
                }
                if !accounts.contains(a) && !old_accounts.contains(a) {
                    return Err(format!("there is no account `{a}`"));
                }
                CreditSource::Account {
                    account: a.to_string(),
                }
            }
        };
        let cap = match &r.cap {
            CreditCapInput::None => CreditCap::None,
            CreditCapInput::QualifiedSpend { amount } => {
                CreditCap::QualifiedSpend(money_at_least_zero(amount, &format!("row {n}'s cap"))?)
            }
            CreditCapInput::Credit { amount } => {
                CreditCap::Credit(money_at_least_zero(amount, &format!("row {n}'s cap"))?)
            }
        };
        rows.push(CreditRow {
            description: r.description.trim().to_string(),
            source,
            percent: percent(&r.percent, &format!("row {n}'s percent"))?,
            cap,
        });
    }
    let credit = Credit {
        name: name.to_string(),
        account: optional_account(b, c.account.as_deref())?,
        applied: c.applied,
        cap: optional_money(c.cap.as_deref(), "the cap")?,
        discount_percent,
        excluded_fringes: picked_names(
            &c.excluded_fringes,
            &fringes,
            Some(&old_fringes),
            "fringe",
        )?,
        rows,
    };
    Ok(match index {
        Some(i) => {
            b.credits[i] = credit;
            i
        }
        None => {
            b.credits.push(credit);
            b.credits.len() - 1
        }
    })
}

pub fn delete_credit(b: &mut Budget, index: usize) -> EditResult {
    check_index(b.credits.len(), index, "credit")?;
    b.credits.remove(index);
    Ok(())
}

pub fn move_credit(b: &mut Budget, index: usize, to: usize) -> EditResult {
    move_item(&mut b.credits, index, to, "credit")
}

#[derive(Clone, Debug, Deserialize)]
pub struct SetInput {
    #[serde(default)]
    pub description: String,
}

pub fn set_set(b: &mut Budget, name: &str, s: &SetInput) -> EditResult {
    let name = check_name(name, "set")?;
    b.sets.insert(
        name,
        SetDef {
            description: s.description.trim().to_string(),
        },
    );
    Ok(())
}

/// Deletes a set and takes it off every row that used it.
pub fn delete_set(b: &mut Budget, name: &str) -> EditResult {
    if b.sets.remove(name).is_none() {
        return Err(format!("set {name} no longer exists"));
    }
    for acct in b.categories_mut().flat_map(|c| &mut c.accounts) {
        for d in &mut acct.details {
            if d.set.as_deref() == Some(name) {
                d.set = None;
            }
        }
    }
    method_core::order::forget(b, method_core::SetupKind::Set, name);
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
pub struct LocationInput {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub country: String,
}

pub fn set_location(b: &mut Budget, name: &str, l: &LocationInput) -> EditResult {
    let name = check_name(name, "location")?;
    b.locations.insert(
        name,
        Location {
            description: l.description.trim().to_string(),
            city: l.city.trim().to_string(),
            region: l.region.trim().to_string(),
            country: l.country.trim().to_string(),
        },
    );
    Ok(())
}

/// Deletes a location and takes it off every row that used it.
pub fn delete_location(b: &mut Budget, name: &str) -> EditResult {
    if b.locations.remove(name).is_none() {
        return Err(format!("location {name} no longer exists"));
    }
    for acct in b.categories_mut().flat_map(|c| &mut c.accounts) {
        for d in &mut acct.details {
            if d.location.as_deref() == Some(name) {
                d.location = None;
            }
        }
    }
    method_core::order::forget(b, method_core::SetupKind::Location, name);
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
pub struct InfoInput {
    /// `text`, `number` or `date`.
    pub kind: String,
    #[serde(default)]
    pub value: String,
}

/// A real calendar date written `YYYY-MM-DD`.
pub fn check_date(s: &str) -> EditResult<String> {
    let t = s.trim();
    let bad = || format!("`{t}` is not a date; write it as YYYY-MM-DD, like 2027-03-01");
    let bytes = t.as_bytes();
    let shape = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    if !shape {
        return Err(bad());
    }
    let num = |r: std::ops::Range<usize>| t[r].parse::<u32>().map_err(|_| bad());
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(bad()),
    };
    if d == 0 || d > days {
        return Err(bad());
    }
    Ok(t.to_string())
}

/// Adds or replaces a budget information field.
pub fn set_info(b: &mut Budget, name: &str, v: &InfoInput) -> EditResult {
    let name = name.trim();
    if name.is_empty() {
        return Err("the field needs a label".into());
    }
    let value = match v.kind.as_str() {
        "text" => InfoValue::Text(v.value.trim().to_string()),
        "number" => InfoValue::Number(parse_dec(&v.value, name)?),
        "date" => InfoValue::Date(check_date(&v.value)?),
        other => return Err(format!("unknown field kind `{other}`")),
    };
    b.info.insert(name.to_string(), value);
    Ok(())
}

pub fn delete_info(b: &mut Budget, name: &str) -> EditResult {
    b.info
        .remove(name)
        .map(|_| ())
        .ok_or_else(|| format!("field {name} no longer exists"))
}

/// Where fringe totals post. `suffix` is for `category` only; when it is
/// left out, the current suffix (or `99`) is kept.
pub fn set_fringe_posting(b: &mut Budget, mode: &str, suffix: Option<&str>) -> EditResult {
    b.fringe_posting = match mode {
        "budget" => FringePosting::Budget,
        "production_level" => FringePosting::ProductionLevel,
        "account" => FringePosting::Account,
        "category" => {
            let suffix = match suffix {
                Some(s) => s.trim().to_string(),
                None => match &b.fringe_posting {
                    FringePosting::Category { suffix } => suffix.clone(),
                    _ => "99".to_string(),
                },
            };
            if suffix.is_empty() {
                return Err("enter the fringe account suffix, like 99".into());
            }
            if let Some(c) = suffix.chars().find(|c| !c.is_ascii_alphanumeric()) {
                return Err(format!(
                    "the suffix can only use letters and digits, not {c:?}"
                ));
            }
            FringePosting::Category { suffix }
        }
        other => return Err(format!("unknown fringe posting `{other}`")),
    };
    Ok(())
}

/// Sets the account-number pattern. `separator` is one character, or blank
/// (or `None`) for none. Existing numbers keep their spelling.
pub fn set_account_coding(b: &mut Budget, widths: [u8; 4], separator: Option<&str>) -> EditResult {
    let separator = match separator {
        None | Some("") => None,
        Some(s) => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => return Err("the separator is one character, or none".into()),
            }
        }
    };
    let coding = AccountCoding {
        category_width: widths[0],
        account_width: widths[1],
        sub_width: widths[2],
        set_width: widths[3],
        separator,
    };
    coding.validate().map_err(|e| e.to_string())?;
    b.account_coding = coding;
    Ok(())
}

/// Renames the base currency. Rates are not converted: they keep meaning
/// "base units per one unit of that currency".
pub fn set_base_currency(b: &mut Budget, code: &str) -> EditResult {
    let code = check_name(code, "currency")?;
    if b.currencies.contains_key(&code) {
        return Err(format!(
            "{code} is in the currency list; delete it there first"
        ));
    }
    if b.totals_currency.as_deref() == Some(code.as_str()) {
        b.totals_currency = None;
    }
    b.base_currency = code;
    Ok(())
}

/// The currency totals are shown in. `None`, blank or the base currency
/// shows them in the base currency.
pub fn set_totals_currency(b: &mut Budget, code: Option<&str>) -> EditResult {
    b.totals_currency = match code.map(str::trim) {
        None | Some("") => None,
        Some(c) if c == b.base_currency => None,
        Some(c) if b.currencies.contains_key(c) => Some(c.to_string()),
        Some(c) => return Err(format!("there is no exchange rate for {c}; add it first")),
    };
    Ok(())
}

/// Freezes the current totals as the baseline (replacing any earlier one).
/// Refused while the budget has calculation errors, since the totals would be
/// missing or partial.
/// Why the original totals can't change: they are frozen (decisions.md,
/// entry 61).
const FROZEN: &str = "the original totals are frozen; unfreeze them first";

pub fn lock_baseline(b: &mut Budget) -> EditResult {
    if b.baseline.as_ref().is_some_and(|x| x.frozen) {
        return Err(FROZEN.into());
    }
    let report = recalc(b, &RecalcOptions::default())
        .map_err(|e| format!("fix this first, then lock the totals: {e}"))?;
    let baseline = Baseline::capture(&report).ok_or("the totals are too large to keep")?;
    b.baseline = Some(baseline);
    Ok(())
}

pub fn clear_baseline(b: &mut Budget) -> EditResult {
    if b.baseline.as_ref().is_some_and(|x| x.frozen) {
        return Err(FROZEN.into());
    }
    b.baseline
        .take()
        .map(|_| ())
        .ok_or_else(|| "there are no original totals to clear".to_string())
}

/// Freezes the original totals, or unfreezes them: while frozen they can't
/// be locked again or cleared, as the reference product's "lock original
/// totals" switch keeps them (decisions.md, entry 61).
pub fn set_baseline_frozen(b: &mut Budget, frozen: bool) -> EditResult {
    let base = b
        .baseline
        .as_mut()
        .ok_or("there are no original totals to freeze")?;
    base.frozen = frozen;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_parsing() {
        assert_eq!(parse_money("150,000"), Ok(15_000_000));
        assert_eq!(parse_money(" 12.345 "), Ok(1235));
        assert_eq!(parse_money("-3"), Ok(-300));
        assert!(parse_money("12a").is_err());
        assert!(parse_money("").is_err());
    }

    #[test]
    fn money_parsing_dollars_and_parentheses() {
        assert_eq!(parse_money("1,234.56"), Ok(123_456));
        assert_eq!(parse_money("1234.56"), Ok(123_456));
        assert_eq!(parse_money("$1,234.56"), Ok(123_456));
        assert_eq!(parse_money("(12.00)"), Ok(-1200));
        assert_eq!(parse_money(" ( $12 ) "), Ok(-1200));
        assert_eq!(parse_money("-$5"), Ok(-500));
        assert_eq!(parse_money("- 5"), Ok(-500));
        assert_eq!(parse_money("+5"), Ok(500));
        assert_eq!(parse_money(".5"), Ok(50));
        for junk in [
            "$", "()", "(-5)", "$-5", "$$5", "5$", "(5", "5)", "1.2.3", "abc", "--5", "1e3", "+-5",
        ] {
            assert!(parse_money(junk).is_err(), "{junk:?} should be rejected");
        }
        assert!(parse_money("99999999999999999999").is_err());
    }

    #[test]
    fn dates() {
        assert_eq!(check_date(" 2027-03-01 "), Ok("2027-03-01".into()));
        assert!(check_date("2024-02-29").is_ok(), "leap year");
        assert!(check_date("2000-02-29").is_ok(), "400-year leap");
        for bad in [
            "2023-02-29",
            "1900-02-29",
            "2027-13-01",
            "2027-00-10",
            "2027-04-31",
            "2027-04-00",
            "2027-4-1",
            "27-04-01",
            "2027/04/01",
            "2027-04-01x",
            "+027-04-01",
            "",
        ] {
            assert!(check_date(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn moving_items() {
        let mut v = vec!['a', 'b', 'c'];
        move_item(&mut v, 0, 2, "x").unwrap();
        assert_eq!(v, ['b', 'c', 'a']);
        move_item(&mut v, 2, 0, "x").unwrap();
        assert_eq!(v, ['a', 'b', 'c']);
        move_item(&mut v, 1, 1, "x").unwrap();
        assert_eq!(v, ['a', 'b', 'c']);
        assert!(move_item(&mut v, 3, 0, "x").is_err());
        assert!(move_item(&mut v, 0, 3, "x").is_err());
        assert_eq!(v, ['a', 'b', 'c']);
    }

    #[test]
    fn picked_names_allow_known_or_kept() {
        let known = BTreeSet::from(["A".to_string()]);
        let kept = BTreeSet::from(["Gone".to_string()]);
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            picked_names(&names(&[" A ", "", "Gone"]), &known, Some(&kept), "group").unwrap(),
            BTreeSet::from(["A".to_string(), "Gone".to_string()])
        );
        assert!(picked_names(&names(&["Gone"]), &known, None, "group").is_err());
        assert!(picked_names(&names(&["B"]), &known, Some(&kept), "group").is_err());
    }

    #[test]
    fn account_coding_separators() {
        let mut b = Budget::new("t", "USD");
        set_account_coding(&mut b, [2, 3, 0, 0], Some(" ")).unwrap();
        assert_eq!(b.account_coding.separator, Some(' '));
        set_account_coding(&mut b, [2, 3, 0, 0], Some("")).unwrap();
        assert_eq!(b.account_coding.separator, None);
        set_account_coding(&mut b, [2, 3, 2, 0], None).unwrap();
        assert_eq!(b.account_coding.sub_width, 2);
        assert!(set_account_coding(&mut b, [2, 2, 0, 0], Some("--")).is_err());
        assert!(set_account_coding(&mut b, [2, 2, 0, 0], Some("_")).is_err());
        assert!(set_account_coding(&mut b, [0, 2, 0, 0], None).is_err());
        assert!(set_account_coding(&mut b, [2, 0, 2, 0], None).is_err());
        assert!(set_account_coding(&mut b, [7, 2, 0, 0], None).is_err());
        assert_eq!(b.account_coding.sub_width, 2, "rejections change nothing");
    }

    #[test]
    fn name_lists() {
        assert_eq!(
            name_list(" B, A ,,A"),
            BTreeSet::from(["A".to_string(), "B".to_string()])
        );
        assert!(name_list("  ").is_empty());
    }

    #[test]
    fn fields_by_kind() {
        assert!(Field::Rate.applies_to(RowKind::Line));
        assert!(!Field::Rate.applies_to(RowKind::Adjustment));
        assert!(Field::Amount.applies_to(RowKind::Adjustment));
        assert!(!Field::Amount.applies_to(RowKind::Subtotal));
        assert!(Field::Description.applies_to(RowKind::Subtotal));
        assert!(Field::parse("bogus").is_err());
    }

    #[test]
    fn charge_label_switches_are_set_kept_and_cleared() {
        let mut b = method_core::starter::budget();
        let input = |v: serde_json::Value| -> ChargeInput { serde_json::from_value(v).unwrap() };
        // A new charge is off unless asked.
        let at = set_charge(
            &mut b,
            None,
            &input(serde_json::json!({"name": "Bond", "kind": "percent", "percent": "2"})),
        )
        .unwrap();
        assert!(!b.charges[at].label_percent && !b.charges[at].label_excluded);
        set_charge(
            &mut b,
            Some(at),
            &input(
                serde_json::json!({"name": "Bond", "kind": "percent", "percent": "2",
                "label_percent": true, "label_excluded": true}),
            ),
        )
        .unwrap();
        assert!(b.charges[at].label_percent && b.charges[at].label_excluded);
        // Left out: each keeps the charge's own setting.
        set_charge(
            &mut b,
            Some(at),
            &input(serde_json::json!({"name": "Bond", "kind": "percent", "percent": "3"})),
        )
        .unwrap();
        assert!(b.charges[at].label_percent && b.charges[at].label_excluded);
        set_charge(
            &mut b,
            Some(at),
            &input(
                serde_json::json!({"name": "Bond", "kind": "percent", "percent": "3",
                "label_excluded": false}),
            ),
        )
        .unwrap();
        assert!(b.charges[at].label_percent && !b.charges[at].label_excluded);
    }

    #[test]
    fn build_charge_checks_without_changing_the_budget() {
        let b = method_core::starter::budget();
        let before = b.clone();
        let c: ChargeInput = serde_json::from_value(serde_json::json!({
            "name": "Contingency", "kind": "percent", "percent": "12.5",
            "excluded_amounts": [{"description": "Typed", "amount": "1,000"}]
        }))
        .unwrap();
        let charge = build_charge(&b, Some(0), &c).unwrap();
        assert_eq!(charge.kind, ChargeKind::Percent("12.5".parse().unwrap()));
        assert_eq!(charge.excluded_amounts, vec![("Typed".into(), 100_000)]);
        assert_eq!(b, before);
        let bad: ChargeInput = serde_json::from_value(
            serde_json::json!({"name": "X", "kind": "percent", "percent": "1."}),
        )
        .unwrap();
        // "1." is a number; a dangling sign is not.
        assert!(build_charge(&b, None, &bad).is_ok());
        let bad: ChargeInput = serde_json::from_value(
            serde_json::json!({"name": "X", "kind": "percent", "percent": "-"}),
        )
        .unwrap();
        assert!(build_charge(&b, None, &bad).is_err());
        assert!(build_charge(&b, Some(9), &c).is_err());
    }
}
