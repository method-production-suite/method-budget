//! Edits the app makes to a [`Budget`]. Each one validates its input and either
//! applies the whole change or returns a message and leaves the budget alone.
//!
//! Validation here is about structure (a name is valid, a code is unique, a row
//! exists). Formulas are stored as typed: a formula that doesn't evaluate is
//! kept and flagged by recalc (see `diagnose`), never rejected.

use std::collections::BTreeSet;

use fwb_core::coding::AccountCoding;
use fwb_core::expr::is_ident;
use fwb_core::{
    recalc, Account, Baseline, Budget, Category, Charge, ChargeKind, Credit, CreditCap, CreditRow,
    CreditSource, Currency, Dec, Detail, Formula, Fringe, FringeKind, FringePosting, Group,
    InfoValue, Location, ProductionTotal, RecalcOptions, RowId, RowKind, SetDef, TopsheetRow,
    UnitDef,
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
            RowKind::Adjustment => !matches!(
                self,
                Field::Rate | Field::Unit3 | Field::Currency | Field::Fringes
            ),
        }
    }
}

pub fn parse_kind(s: &str) -> EditResult<RowKind> {
    match s {
        "line" => Ok(RowKind::Line),
        "subtotal" => Ok(RowKind::Subtotal),
        "adjustment" => Ok(RowKind::Adjustment),
        other => Err(format!("unknown row kind `{other}`")),
    }
}

fn kind_label(k: RowKind) -> &'static str {
    match k {
        RowKind::Line => "lines",
        RowKind::Subtotal => "subtotal rows",
        RowKind::Adjustment => "adjustment rows",
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
        .ok_or_else(|| format!("row {id} no longer exists"))
}

/// The next unused row id: past every row and every baseline row, so a new
/// row is never mistaken for an old one when comparing.
pub fn next_row_id(b: &Budget) -> RowId {
    let rows = details(b).map(|d| d.id).max().unwrap_or(0);
    let base = b
        .baseline
        .as_ref()
        .and_then(|bl| bl.rows.keys().next_back().copied())
        .unwrap_or(0);
    rows.max(base).saturating_add(1)
}

fn apply_field(d: &mut Detail, field: Field, value: &str) -> EditResult {
    if !field.applies_to(d.kind) {
        return Err(format!("{} don't use this column", kind_label(d.kind)));
    }
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

pub fn set_detail_field(b: &mut Budget, id: RowId, field: Field, value: &str) -> EditResult {
    apply_field(detail_mut(b, id)?, field, value)
}

/// Sets one field on several rows. Rows that don't use the field are skipped;
/// it's an error only if none do.
pub fn set_rows_field(b: &mut Budget, ids: &[RowId], field: Field, value: &str) -> EditResult {
    let mut applied = 0;
    for &id in ids {
        let d = detail_mut(b, id)?;
        if field.applies_to(d.kind) {
            apply_field(d, field, value)?;
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

/// Inserts a blank row of `kind` after row `after` (or at the end of the
/// account when `after` is `None`). Returns the new row's id.
pub fn add_detail(
    b: &mut Budget,
    account: &str,
    after: Option<RowId>,
    kind: RowKind,
) -> EditResult<RowId> {
    let id = next_row_id(b);
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
    let row = match kind {
        RowKind::Line => Detail::line(id, "", "", "", ""),
        RowKind::Subtotal => Detail::subtotal(id, ""),
        RowKind::Adjustment => Detail::adjustment(id, "", ""),
    };
    acct.details.insert(at, row);
    Ok(id)
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
    let new_code = account_code(&b.account_coding, new_code)?;
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
            "`{name}` can't be a global name: use letters, digits and _, starting with a letter"
        ));
    }
    let formula = formula.trim();
    if formula.is_empty() {
        return Err(format!("global {name} needs a value"));
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
    if fwb_core::delete_global(b, name).is_err() {
        b.globals.remove(name);
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
pub struct FringeInput {
    /// `percent` or `flat`.
    pub kind: String,
    #[serde(default)]
    pub description: String,
    pub rate: String,
    /// Percent fringes: cutoff as money text, blank for none.
    #[serde(default)]
    pub cutoff: Option<String>,
    /// Flat fringes: unit name, blank for none.
    #[serde(default)]
    pub unit: Option<String>,
}

pub fn set_fringe(b: &mut Budget, name: &str, f: &FringeInput) -> EditResult {
    let name = check_name(name, "fringe")?;
    let kind = match f.kind.as_str() {
        "percent" => {
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
            FringeKind::Percent { cutoff }
        }
        "flat" => FringeKind::Flat {
            unit: f.unit.as_deref().and_then(optional),
        },
        other => return Err(format!("unknown fringe kind `{other}`")),
    };
    b.fringes.insert(
        name,
        Fringe {
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
        for row in &mut credit.rows {
            if let CreditSource::Group {
                excluded_fringes, ..
            } = &mut row.source
            {
                excluded_fringes.remove(name);
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
pub struct GroupInput {
    #[serde(default)]
    pub description: String,
    pub in_budget_total: bool,
}

pub fn set_group(b: &mut Budget, name: &str, g: &GroupInput) -> EditResult {
    let name = check_name(name, "group")?;
    b.groups.insert(
        name,
        Group {
            description: g.description.trim().to_string(),
            in_budget_total: g.in_budget_total,
        },
    );
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
    b.units
        .remove(name)
        .map(|_| ())
        .ok_or_else(|| format!("unit {name} no longer exists"))
}

#[derive(Clone, Debug, Deserialize)]
pub struct CurrencyInput {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub symbol: String,
    /// Base-currency units per one unit of this currency.
    pub rate: String,
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
    b.currencies.insert(
        code,
        Currency {
            description: c.description.trim().to_string(),
            symbol: c.symbol.trim().to_string(),
            rate,
        },
    );
    Ok(())
}

/// Deletes a currency. Refused while lines, charges or the totals use it.
pub fn delete_currency(b: &mut Budget, code: &str) -> EditResult {
    if !b.currencies.contains_key(code) {
        return Err(format!("currency {code} no longer exists"));
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
        matches!(&c.kind, fwb_core::ChargeKind::Flat { currency: Some(cur), .. } if cur == code)
    });
    if charged {
        return Err(format!("a charge is in {code}; change it first"));
    }
    b.currencies.remove(code);
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

/// An optional account number, normalized by the budget's coding.
fn optional_account(b: &Budget, typed: Option<&str>) -> EditResult<Option<String>> {
    match typed.map(str::trim) {
        None | Some("") => Ok(None),
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
}

/// Replaces charge `index`, or appends when `None`. Returns its index.
pub fn set_charge(b: &mut Budget, index: Option<usize>, c: &ChargeInput) -> EditResult<usize> {
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
    let charge = Charge {
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
    };
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
    Group {
        group: String,
        #[serde(default)]
        excluded_fringes: Vec<String>,
    },
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
    let mut old_fringes = BTreeSet::new();
    for r in old.iter().flat_map(|o| &o.rows) {
        if let CreditSource::Group {
            group,
            excluded_fringes,
        } = &r.source
        {
            old_groups.insert(group.clone());
            old_fringes.extend(excluded_fringes.iter().cloned());
        }
    }
    let groups: BTreeSet<String> = b.groups.keys().cloned().collect();
    let fringes: BTreeSet<String> = b.fringes.keys().cloned().collect();
    let mut rows = Vec::with_capacity(c.rows.len());
    for (i, r) in c.rows.iter().enumerate() {
        let n = i + 1;
        let source = match &r.source {
            CreditSourceInput::Flat { amount } => {
                CreditSource::Flat(money_at_least_zero(amount, &format!("row {n}'s amount"))?)
            }
            CreditSourceInput::Group {
                group,
                excluded_fringes,
            } => {
                let g = group.trim();
                if g.is_empty() {
                    return Err(format!("row {n} needs a group"));
                }
                if !groups.contains(g) && !old_groups.contains(g) {
                    return Err(format!("there is no group `{g}`"));
                }
                CreditSource::Group {
                    group: g.to_string(),
                    excluded_fringes: picked_names(
                        excluded_fringes,
                        &fringes,
                        Some(&old_fringes),
                        "fringe",
                    )?,
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
pub fn lock_baseline(b: &mut Budget) -> EditResult {
    let report = recalc(b, &RecalcOptions::default())
        .map_err(|e| format!("fix this first, then lock the totals: {e}"))?;
    let baseline = Baseline::capture(&report).ok_or("the totals are too large to keep")?;
    b.baseline = Some(baseline);
    Ok(())
}

pub fn clear_baseline(b: &mut Budget) -> EditResult {
    b.baseline
        .take()
        .map(|_| ())
        .ok_or_else(|| "there are no original totals to clear".to_string())
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
}
