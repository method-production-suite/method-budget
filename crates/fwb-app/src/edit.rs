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
    Account, Budget, Category, CreditSource, Currency, Dec, Detail, Formula, Fringe, FringeKind,
    Group, ProductionTotal, RowId, RowKind, TopsheetRow, UnitDef,
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
/// rounded half away from zero to cents.
pub fn parse_money(s: &str) -> EditResult<i64> {
    let cleaned: String = s
        .chars()
        .filter(|c| *c != ',' && !c.is_whitespace())
        .collect();
    let d: Dec = cleaned
        .parse()
        .map_err(|e| format!("`{}` is not an amount: {e}", s.trim()))?;
    d.to_cents()
        .ok_or_else(|| format!("`{}` is too large", s.trim()))
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
