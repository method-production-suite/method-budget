//! The edits whose refusals a host tells apart, each returning an
//! [`EditError`]: input that can't be used, something that isn't in the
//! budget, or a change the budget refuses. The MCP server calls these; the
//! app calls the same edits in [`super`], which give the message as text.

use std::collections::BTreeSet;

use method_core::expr::is_ident;
use method_core::{
    Account, Budget, Category, Charge, ChargeKind, Credit, CreditCap, CreditRow, CreditSource,
    Currency, Dec, Formula, ProductionTotal, TopsheetRow,
};

use super::{
    account_code, account_exists, account_mut, amount_currency, category_code, category_exists,
    check_index, check_name, details, insert_at, money_at_least_zero, optional, optional_account,
    optional_money, parse_dec, percent, picked_names, put_fringe, ChargeInput, CreditCapInput,
    CreditInput, CreditSourceInput, CurrencyInput, EditError, FringeInput,
};

/// Adds or replaces a fringe; a flat fringe's unit that isn't one of the
/// budget's units is not found.
pub fn set_fringe(b: &mut Budget, name: &str, f: &FringeInput) -> Result<(), EditError> {
    if f.kind == "flat" {
        if let Some(u) = f.unit.as_deref().and_then(optional) {
            if !b.units.contains_key(&u) {
                return Err(EditError::NotFound(format!("there is no unit {u}")));
            }
        }
    }
    Ok(put_fringe(b, name, f)?)
}

pub fn add_category(
    b: &mut Budget,
    code: &str,
    name: &str,
    at: Option<usize>,
) -> Result<usize, EditError> {
    let code = category_code(&b.account_coding, code)?;
    if category_exists(b, &code, None) {
        return Err(EditError::Refused(format!(
            "category {code} already exists"
        )));
    }
    let row = TopsheetRow::Category(Category {
        notes: Vec::new(),
        code,
        name: name.trim().to_string(),
        accounts: Vec::new(),
    });
    Ok(insert_at(b, at, row))
}

pub fn add_production_total(
    b: &mut Budget,
    name: &str,
    at: Option<usize>,
) -> Result<usize, EditError> {
    let name = optional(name).ok_or("production total name is empty")?;
    Ok(insert_at(
        b,
        at,
        TopsheetRow::ProductionTotal(ProductionTotal { name }),
    ))
}

pub fn update_topsheet_row(
    b: &mut Budget,
    index: usize,
    code: Option<&str>,
    name: &str,
) -> Result<(), EditError> {
    let new_code = match (b.topsheet.get(index), code) {
        (None, _) => {
            return Err(EditError::NotFound(
                "that topsheet row no longer exists".into(),
            ))
        }
        // The number as it is stays as it is, even one the pattern can't
        // read (a chart's own numbers may not follow it).
        (Some(TopsheetRow::Category(c)), Some(typed)) if typed.trim() == c.code => None,
        (Some(TopsheetRow::Category(_)), Some(typed)) => {
            let code = category_code(&b.account_coding, typed)?;
            if category_exists(b, &code, Some(index)) {
                return Err(EditError::Refused(format!(
                    "category {code} already exists"
                )));
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

pub fn delete_topsheet_row(b: &mut Budget, index: usize) -> Result<(), EditError> {
    if index >= b.topsheet.len() {
        return Err(EditError::NotFound(
            "that topsheet row no longer exists".into(),
        ));
    }
    b.topsheet.remove(index);
    b.fringe_row.row_removed(index);
    Ok(())
}

pub fn add_account(
    b: &mut Budget,
    category: &str,
    code: &str,
    name: &str,
    after: Option<&str>,
) -> Result<String, EditError> {
    let code = account_code(&b.account_coding, code)?;
    if account_exists(b, &code, None) {
        return Err(EditError::Refused(format!("account {code} already exists")));
    }
    let cat = b
        .categories_mut()
        .find(|c| c.code == category)
        .ok_or_else(|| EditError::NotFound(format!("category {category} no longer exists")))?;
    let at = match after {
        None => cat.accounts.len(),
        Some(a) => {
            cat.accounts
                .iter()
                .position(|x| x.code == a)
                .ok_or_else(|| {
                    EditError::NotFound(format!("account {a} is not in category {category}"))
                })?
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
) -> Result<String, EditError> {
    // The number as it is stays as it is, even one the pattern can't read
    // (a chart's own numbers may not follow it).
    let new_code = if new_code.trim() == code {
        code.to_string()
    } else {
        account_code(&b.account_coding, new_code)?
    };
    if new_code != code && account_exists(b, &new_code, Some(code)) {
        return Err(EditError::Refused(format!(
            "account {new_code} already exists"
        )));
    }
    let acct = account_mut(b, code).map_err(EditError::NotFound)?;
    acct.code = new_code.clone();
    acct.name = name.trim().to_string();
    Ok(new_code)
}

pub fn delete_account(b: &mut Budget, code: &str) -> Result<(), EditError> {
    for cat in b.categories_mut() {
        if let Some(i) = cat.accounts.iter().position(|a| a.code == code) {
            cat.accounts.remove(i);
            return Ok(());
        }
    }
    Err(EditError::NotFound(format!(
        "account {code} no longer exists"
    )))
}

pub fn set_global(b: &mut Budget, name: &str, formula: &str) -> Result<(), EditError> {
    let name = name.trim();
    if !is_ident(name) {
        return Err(EditError::Invalid(format!(
            "`{name}` can't be a global name: use letters, digits and _, with at least one letter or _"
        )));
    }
    let formula = formula.trim();
    if formula.is_empty() {
        return Err(EditError::Invalid(format!("global {name} needs a value")));
    }
    // Refused as the reference product refuses it (decisions.md, entry 60).
    if let Some(why) = method_core::edit::refused_global(b, name, formula) {
        return Err(EditError::Refused(format!(
            "global {name} can't be {formula}: {why}"
        )));
    }
    b.globals
        .insert(name.to_string(), Formula(formula.to_string()));
    Ok(())
}

pub fn delete_group(b: &mut Budget, name: &str) -> Result<(), EditError> {
    if !b.groups.contains_key(name) {
        return Err(EditError::NotFound(format!(
            "group {name} no longer exists"
        )));
    }
    check_group_delete(b, name)?;
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

/// Why group `name` can't be deleted however few lines use it: a credit is
/// based on it.
pub fn check_group_delete(b: &Budget, name: &str) -> Result<(), EditError> {
    let used_by = b.credits.iter().find(|c| {
        c.rows
            .iter()
            .any(|r| matches!(&r.source, CreditSource::Group { group, .. } if group == name))
    });
    match used_by {
        Some(c) => Err(EditError::Refused(format!(
            "group {name} is used by the credit {}; change the credit first",
            c.name
        ))),
        None => Ok(()),
    }
}

pub fn set_currency(b: &mut Budget, code: &str, c: &CurrencyInput) -> Result<(), EditError> {
    let code = check_name(code, "currency")?;
    if code == b.base_currency {
        return Err(EditError::Refused(format!("{code} is the base currency")));
    }
    let rate = parse_dec(&c.rate, "rate")?;
    if rate <= Dec::ZERO {
        return Err(EditError::Invalid(
            "an exchange rate must be more than 0".into(),
        ));
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

pub fn delete_currency_with(b: &mut Budget, code: &str, move_lines: bool) -> Result<(), EditError> {
    if !b.currencies.contains_key(code) {
        return Err(EditError::NotFound(format!(
            "currency {code} no longer exists"
        )));
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
        return Err(EditError::Refused(format!(
            "{lines} {s} {code}; change them first"
        )));
    }
    check_currency_delete(b, code)?;
    b.currencies.remove(code);
    method_core::order::forget(b, method_core::SetupKind::Currency, code);
    Ok(())
}

/// Why currency `code` can't be deleted whatever its lines are in: the
/// totals are shown in it, or a charge is.
pub fn check_currency_delete(b: &Budget, code: &str) -> Result<(), EditError> {
    if b.totals_currency.as_deref() == Some(code) {
        return Err(EditError::Refused(format!(
            "totals are shown in {code}; change that first"
        )));
    }
    let charged = b
        .charges
        .iter()
        .any(|c| matches!(&c.kind, ChargeKind::Flat { currency: Some(cur), .. } if cur == code));
    if charged {
        return Err(EditError::Refused(format!(
            "a charge is in {code}; change it first"
        )));
    }
    Ok(())
}

pub fn set_charge(
    b: &mut Budget,
    index: Option<usize>,
    c: &ChargeInput,
) -> Result<usize, EditError> {
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

pub fn build_charge(
    b: &Budget,
    index: Option<usize>,
    c: &ChargeInput,
) -> Result<Charge, EditError> {
    let old = match index {
        Some(i) => {
            check_index(b.charges.len(), i, "charge").map_err(EditError::NotFound)?;
            Some(b.charges[i].clone())
        }
        None => None,
    };
    let name = c.name.trim();
    if name.is_empty() {
        return Err(EditError::Invalid("charge name is empty".into()));
    }
    if b.charges
        .iter()
        .enumerate()
        .any(|(i, x)| Some(i) != index && x.name == name)
    {
        return Err(EditError::Refused(format!(
            "there is already a charge named {name}"
        )));
    }
    let kind = match c.kind.as_str() {
        "percent" => {
            ChargeKind::Percent(percent(c.percent.as_deref().unwrap_or(""), "the percent")?)
        }
        "flat" => {
            let text = c.amount.as_deref().unwrap_or("").trim();
            if text.is_empty() {
                return Err(EditError::Invalid("a flat charge needs an amount".into()));
            }
            ChargeKind::Flat {
                amount: money_at_least_zero(text, "the amount")?,
                currency: amount_currency(b, c.currency.as_deref()).map_err(EditError::NotFound)?,
            }
        }
        other => return Err(EditError::Invalid(format!("unknown charge kind `{other}`"))),
    };
    let mut excluded_amounts = Vec::with_capacity(c.excluded_amounts.len());
    for x in &c.excluded_amounts {
        excluded_amounts.push((
            x.description.trim().to_string(),
            money_at_least_zero(&x.amount, "an excluded amount")?,
        ));
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
        )
        .map_err(EditError::NotFound)?,
        excluded_categories: picked_names(
            &c.excluded_categories,
            &categories,
            old.as_ref().map(|o| &o.excluded_categories),
            "category",
        )
        .map_err(EditError::NotFound)?,
        excluded_accounts: picked_names(
            &c.excluded_accounts,
            &accounts,
            old.as_ref().map(|o| &o.excluded_accounts),
            "account",
        )
        .map_err(EditError::NotFound)?,
        label_percent: c
            .label_percent
            .unwrap_or_else(|| old.as_ref().is_some_and(|o| o.label_percent)),
        label_excluded: c
            .label_excluded
            .unwrap_or_else(|| old.as_ref().is_some_and(|o| o.label_excluded)),
    })
}

pub fn set_credit(
    b: &mut Budget,
    index: Option<usize>,
    c: &CreditInput,
) -> Result<usize, EditError> {
    let old = match index {
        Some(i) => {
            check_index(b.credits.len(), i, "credit").map_err(EditError::NotFound)?;
            Some(b.credits[i].clone())
        }
        None => None,
    };
    let name = c.name.trim();
    if name.is_empty() {
        return Err(EditError::Invalid("credit name is empty".into()));
    }
    if b.credits
        .iter()
        .enumerate()
        .any(|(i, x)| Some(i) != index && x.name == name)
    {
        return Err(EditError::Refused(format!(
            "there is already a credit named {name}"
        )));
    }
    let discount_percent = match c.discount_percent.as_deref().map(str::trim) {
        None | Some("") => Dec::ZERO,
        Some(t) => {
            let d = percent(t, "the discount")?;
            if d > Dec::from_int(100) {
                return Err(EditError::Invalid(
                    "the discount can't be more than 100%".into(),
                ));
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
    let locations: BTreeSet<String> = b.locations.keys().cloned().collect();
    let old_locations = old
        .as_ref()
        .map(|o| o.locations.clone())
        .unwrap_or_default();
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
                    return Err(EditError::Invalid(format!("row {n} needs a group")));
                }
                if !groups.contains(g) && !old_groups.contains(g) {
                    return Err(EditError::NotFound(format!("there is no group `{g}`")));
                }
                CreditSource::Group {
                    group: g.to_string(),
                }
            }
            CreditSourceInput::Category { category } => {
                let c = category.trim();
                if c.is_empty() {
                    return Err(EditError::Invalid(format!("row {n} needs a category")));
                }
                if !categories.contains(c) && !old_categories.contains(c) {
                    return Err(EditError::NotFound(format!("there is no category `{c}`")));
                }
                CreditSource::Category {
                    category: c.to_string(),
                }
            }
            CreditSourceInput::Account { account } => {
                let a = account.trim();
                if a.is_empty() {
                    return Err(EditError::Invalid(format!("row {n} needs an account")));
                }
                if !accounts.contains(a) && !old_accounts.contains(a) {
                    return Err(EditError::NotFound(format!("there is no account `{a}`")));
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
        excluded_fringes: picked_names(&c.excluded_fringes, &fringes, Some(&old_fringes), "fringe")
            .map_err(EditError::NotFound)?,
        locations: picked_names(&c.locations, &locations, Some(&old_locations), "location")
            .map_err(EditError::NotFound)?,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(e: EditError) -> &'static str {
        match e {
            EditError::Invalid(_) => "invalid",
            EditError::NotFound(_) => "not_found",
            EditError::Refused(_) => "refused",
        }
    }

    #[test]
    fn refusals_say_their_kind() {
        let mut b = method_core::starter::budget();
        let again = add_category(&mut b, "21", "Again", None);
        assert_eq!(again.clone().map_err(kind), Err("refused"));
        let nowhere = add_account(&mut b, "99", "99-01", "Nowhere", None);
        assert_eq!(nowhere.map_err(kind), Err("not_found"));
        assert_eq!(set_global(&mut b, "A B", "2").map_err(kind), Err("invalid"));
        let looped = set_global(&mut b, "LOOP", "LOOP + 1");
        assert_eq!(looped.map_err(kind), Err("refused"));
        assert_eq!(delete_group(&mut b, "Nope").map_err(kind), Err("not_found"));
        // The text is the app's message, unchanged.
        let e = again.unwrap_err();
        assert_eq!(e.to_string(), "category 21 already exists");
        assert_eq!(String::from(e), "category 21 already exists");
    }
}
