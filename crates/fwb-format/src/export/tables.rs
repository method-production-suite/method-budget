//! Building the export's tables from a budget and the report computed from it.

use std::collections::{BTreeMap, BTreeSet};

use fwb_core::expr::{quick_value, Expr};
use fwb_core::{Breakdown, Budget, Cents, Dec, Formula, FringeKind, GroupTotals, Report, RowKind};

use super::{lines_up, Cell, ExportError, Sheet, Workbook};

/// Every table, from `b` and the report `r` computed from it with the
/// default options (`fwb_core::recalc(b, &RecalcOptions::default())`).
pub fn workbook(b: &Budget, r: &Report) -> Result<Workbook, ExportError> {
    if !lines_up(b, r) {
        return Err(ExportError::ReportMismatch);
    }
    Ok(Workbook {
        title: b.title.clone(),
        sheets: vec![
            topsheet(b, r),
            accounts(b, r),
            detail(b, r),
            globals(b, r),
            fringes(b, r),
            groups(b, r),
            units(b),
            currencies(b, r),
            sets(b, r),
            locations(b, r),
        ],
    })
}

fn columns(titles: &[&str]) -> Vec<String> {
    titles.iter().map(|t| t.to_string()).collect()
}

fn yes_no(on: bool) -> Cell {
    Cell::text(if on { "Yes" } else { "No" })
}

fn money_or_empty(c: Option<Cents>) -> Cell {
    c.map_or(Cell::Empty, Cell::Money)
}

/// Names as the grid lists them: `a, b`.
fn list<'a>(names: impl IntoIterator<Item = &'a String>) -> Cell {
    Cell::text(names.into_iter().cloned().collect::<Vec<_>>().join(", "))
}

/// A formula's value with the budget's globals, or `None` when blank (or,
/// which a report computed from the budget rules out, when it has an error).
fn value(f: &Formula, globals: &BTreeMap<String, Dec>) -> Option<Dec> {
    if f.is_blank() {
        return None;
    }
    let lookup = |n: &str| globals.get(n).copied();
    quick_value(&f.0, &lookup).or_else(|| Expr::parse(&f.0).ok()?.eval(&lookup).ok())
}

fn number_or_empty(v: Option<Dec>) -> Cell {
    v.map_or(Cell::Empty, Cell::Number)
}

fn breakdown_cells(bd: &Breakdown) -> [Cell; 3] {
    [
        Cell::Money(bd.details_total),
        Cell::Money(bd.fringe_total),
        Cell::Money(bd.details_total.saturating_add(bd.fringe_total)),
    ]
}

/// Topsheet: categories by level, posted fringes, production totals, the
/// above- and below-the-line totals, charges, grand total, credits and net.
fn topsheet(b: &Budget, r: &Report) -> Sheet {
    let mut rows = Vec::new();
    let mut row = |kind: &str, code: &str, label: &str, total: Cell| {
        rows.push(vec![
            Cell::text(kind),
            Cell::text(code),
            Cell::text(label),
            total,
        ]);
    };
    for level in &r.levels {
        for cat in r.categories.get(level.start..level.end).unwrap_or_default() {
            row("Category", &cat.code, &cat.name, Cell::Money(cat.total));
        }
        if level.posted_fringes != 0 {
            row("Fringes", "", "Fringes", Cell::Money(level.posted_fringes));
        }
        if !level.name.is_empty() {
            row(
                "Production Total",
                "",
                &level.name,
                Cell::Money(level.total),
            );
        }
    }
    row(
        "Total",
        "",
        "Total above-the-line",
        Cell::Money(r.total_atl),
    );
    row(
        "Total",
        "",
        "Total below-the-line",
        Cell::Money(r.total_btl),
    );
    row(
        "Total",
        "",
        "Total above and below-the-line",
        Cell::Money(r.atl_btl),
    );
    if r.budget_fringes != 0 {
        row(
            "Fringes",
            "",
            "Total fringes",
            Cell::Money(r.budget_fringes),
        );
    }
    // The report lists applied charges and credits, in order.
    let charges = b.charges.iter().filter(|c| c.applied);
    for (c, ct) in charges.zip(&r.charges) {
        row(
            "Charge",
            c.account.as_deref().unwrap_or(""),
            &ct.name,
            Cell::Money(ct.amount),
        );
    }
    row("Total", "", "Grand total", Cell::Money(r.grand_total));
    let credits = b.credits.iter().filter(|c| c.applied);
    for (c, ct) in credits.zip(&r.credits) {
        row(
            "Credit",
            c.account.as_deref().unwrap_or(""),
            &format!("Less {}", ct.name),
            Cell::Money(ct.amount.saturating_neg()),
        );
    }
    row("Total", "", "Net total", Cell::Money(r.net_total));
    if r.totals_currency != r.base_currency {
        let cur = &r.totals_currency;
        row(
            "Total",
            "",
            &format!(
                "Grand total in {cur} (1 {cur} = {} {})",
                r.totals_rate, r.base_currency
            ),
            money_or_empty(r.in_totals_currency(r.grand_total)),
        );
        row(
            "Total",
            "",
            &format!("Net total in {cur}"),
            money_or_empty(r.in_totals_currency(r.net_total)),
        );
    }
    Sheet {
        name: "Topsheet",
        columns: columns(&["Type", "Code", "Description", "Total"]),
        rows,
    }
}

/// Accounts: every account with its lines total, the fringes its lines
/// earned, and its total; a category's fringe account (when fringes post by
/// category) after the category's accounts. The Total column adds up to the
/// categories' totals.
fn accounts(b: &Budget, r: &Report) -> Sheet {
    let mut rows = Vec::new();
    for (cat, rc) in b.categories().zip(&r.categories) {
        for ra in &rc.accounts {
            rows.push(vec![
                Cell::text(&cat.code),
                Cell::text(&cat.name),
                Cell::text(&ra.code),
                Cell::text(&ra.name),
                Cell::Money(ra.details_total),
                Cell::Money(ra.fringe_total),
                Cell::Money(ra.total),
            ]);
        }
        if let Some((code, amount)) = &rc.fringe_account {
            rows.push(vec![
                Cell::text(&cat.code),
                Cell::text(&cat.name),
                Cell::text(code),
                Cell::text("Fringes"),
                Cell::Empty,
                Cell::Empty,
                Cell::Money(*amount),
            ]);
        }
    }
    Sheet {
        name: "Accounts",
        columns: columns(&[
            "Category",
            "Category Name",
            "Account",
            "Account Name",
            "Details Total",
            "Fringe Total",
            "Total",
        ]),
        rows,
    }
}

fn kind_name(k: RowKind) -> &'static str {
    match k {
        RowKind::Line => "Line",
        RowKind::Subtotal => "Subtotal",
        RowKind::Adjustment => "Adjustment",
        RowKind::Separator => "Separator",
    }
}

/// Detail: one row per detail row of every account, in budget order, with
/// formulas as typed and their values. An adjustment's rate is the amount it
/// is a percentage of (the lines above it, back to the previous adjustment).
fn detail(b: &Budget, r: &Report) -> Sheet {
    let g = &r.globals;
    let mut rows = Vec::new();
    for (cat, rc) in b.categories().zip(&r.categories) {
        for (acct, ra) in cat.accounts.iter().zip(&rc.accounts) {
            // As the engine computes it (calc/lines.rs): lines since the
            // previous counted adjustment; subtotal rows don't reset it.
            let mut since_adjustment: Cents = 0;
            for (i, (d, rr)) in acct.details.iter().zip(&ra.rows).enumerate() {
                let costed = d.kind.is_costed();
                let formula_value = |f: &Formula| {
                    if costed {
                        number_or_empty(value(f, g))
                    } else {
                        Cell::Empty
                    }
                };
                let rate = match d.kind {
                    RowKind::Adjustment => Cell::Money(since_adjustment),
                    RowKind::Line => formula_value(&d.rate),
                    _ => Cell::Empty,
                };
                if costed {
                    since_adjustment = if d.kind == RowKind::Adjustment && rr.included {
                        0
                    } else {
                        since_adjustment.saturating_add(rr.total)
                    };
                }
                let money = |c: Cents| if costed { Cell::Money(c) } else { Cell::Empty };
                rows.push(vec![
                    Cell::text(&cat.code),
                    Cell::text(&acct.code),
                    Cell::text(&acct.name),
                    Cell::Int(i as u64 + 1),
                    Cell::text(kind_name(d.kind)),
                    Cell::text(&d.description),
                    Cell::text(&d.amount.0),
                    formula_value(&d.amount),
                    Cell::text(&d.unit),
                    formula_value(&d.x),
                    Cell::text(&d.unit2),
                    Cell::text(&d.rate.0),
                    rate,
                    Cell::text(&d.unit3),
                    Cell::text(d.currency.as_deref().unwrap_or("")),
                    formula_value(&d.x4),
                    Cell::text(&d.unit4),
                    money_or_empty(rr.subtotal),
                    money(rr.total),
                    list(&d.fringes),
                    money(rr.fringe_total),
                    list(&d.groups),
                    Cell::text(d.set.as_deref().unwrap_or("")),
                    Cell::text(d.location.as_deref().unwrap_or("")),
                    Cell::text(fwb_core::notes_text(&d.notes)),
                    yes_no(rr.included),
                    Cell::Int(d.id),
                    d.fringe_range
                        .map_or(Cell::Empty, |n| Cell::Int(u64::from(n))),
                ]);
            }
        }
    }
    Sheet {
        name: "Detail",
        columns: columns(&[
            "Category",
            "Account",
            "Account Name",
            "Line",
            "Kind",
            "Description",
            "Amount Formula",
            "Amount",
            "Unit",
            "X",
            "X Unit",
            "Rate Formula",
            "Rate",
            "Rate Unit",
            "Currency",
            "4X",
            "4X Unit",
            "Subtotal",
            "Total",
            "Fringes",
            "Fringe Total",
            "Groups",
            "Set",
            "Location",
            "Note",
            "Included",
            "Row ID",
            "Fringe Range",
        ]),
        rows,
    }
}

fn globals(b: &Budget, r: &Report) -> Sheet {
    let rows = b
        .globals
        .iter()
        .map(|(name, f)| {
            vec![
                Cell::text(name),
                Cell::text(&f.0),
                number_or_empty(r.globals.get(name).copied()),
            ]
        })
        .collect();
    Sheet {
        name: "Globals",
        columns: columns(&["Name", "Formula", "Value"]),
        rows,
    }
}

fn fringes(b: &Budget, r: &Report) -> Sheet {
    let rows = b
        .fringes
        .iter()
        .map(|(name, f)| {
            let (kind, cutoff, unit) = match &f.kind {
                FringeKind::Percent { cutoff } => ("Percent", *cutoff, None),
                FringeKind::Flat { unit } => ("Flat", None, unit.as_deref()),
            };
            let summary = r.fringes.get(name).cloned().unwrap_or_default();
            vec![
                Cell::text(name),
                Cell::text(&f.description),
                Cell::text(kind),
                Cell::text(&f.rate.0),
                number_or_empty(value(&f.rate, &r.globals)),
                money_or_empty(cutoff),
                Cell::text(unit.unwrap_or("")),
                Cell::Int(u64::from(summary.uses)),
                Cell::Money(summary.total),
            ]
        })
        .collect();
    Sheet {
        name: "Fringes",
        columns: columns(&[
            "Name",
            "Description",
            "Type",
            "Rate Formula",
            "Rate",
            "Cutoff",
            "Unit",
            "Lines",
            "Total",
        ]),
        rows,
    }
}

fn groups(b: &Budget, r: &Report) -> Sheet {
    let rows = b
        .groups
        .iter()
        .map(|(name, g)| {
            let t: GroupTotals = r.groups.get(name).cloned().unwrap_or_default();
            let [details, fringes, total] = breakdown_cells(&t.full);
            let counted = t.active.details_total.saturating_add(t.active.fringe_total);
            vec![
                Cell::text(name),
                Cell::text(&g.description),
                yes_no(g.in_budget_total),
                details,
                fringes,
                total,
                Cell::Money(counted),
            ]
        })
        .collect();
    Sheet {
        name: "Groups",
        columns: columns(&[
            "Name",
            "Description",
            "In Budget Total",
            "Details Total",
            "Fringe Total",
            "Total",
            "Counted Total",
        ]),
        rows,
    }
}

fn units(b: &Budget) -> Sheet {
    let rows = b
        .units
        .iter()
        .map(|(name, u)| {
            vec![
                Cell::text(name),
                Cell::text(&u.plural),
                Cell::text(&u.description),
                number_or_empty(u.hours),
            ]
        })
        .collect();
    Sheet {
        name: "Units",
        columns: columns(&["Unit", "Plural", "Description", "Hours"]),
        rows,
    }
}

fn currencies(b: &Budget, r: &Report) -> Sheet {
    let rows = b
        .currencies
        .iter()
        .map(|(code, c)| {
            vec![
                Cell::text(code),
                Cell::text(&c.description),
                Cell::text(&c.symbol),
                Cell::Number(c.rate),
            ]
        })
        .collect();
    Sheet {
        name: "Currencies",
        columns: vec![
            "Code".into(),
            "Description".into(),
            "Symbol".into(),
            format!("Rate in {}", r.base_currency),
        ],
        rows,
    }
}

/// The names a setup table defines, then any the lines use without it
/// defining them (so their totals still show).
fn names<'a, T>(
    defined: &'a BTreeMap<String, T>,
    used: &'a BTreeMap<String, Breakdown>,
) -> BTreeSet<&'a String> {
    defined.keys().chain(used.keys()).collect()
}

fn sets(b: &Budget, r: &Report) -> Sheet {
    let rows = names(&b.sets, &r.sets)
        .into_iter()
        .map(|name| {
            let desc = b.sets.get(name).map(|s| s.description.as_str());
            let [details, fringes, total] =
                breakdown_cells(&r.sets.get(name).cloned().unwrap_or_default());
            vec![
                Cell::text(name),
                Cell::text(desc.unwrap_or("")),
                details,
                fringes,
                total,
            ]
        })
        .collect();
    Sheet {
        name: "Sets",
        columns: columns(&[
            "Set",
            "Description",
            "Details Total",
            "Fringe Total",
            "Total",
        ]),
        rows,
    }
}

fn locations(b: &Budget, r: &Report) -> Sheet {
    let rows = names(&b.locations, &r.locations)
        .into_iter()
        .map(|name| {
            let loc = b.locations.get(name).cloned().unwrap_or_default();
            let [details, fringes, total] =
                breakdown_cells(&r.locations.get(name).cloned().unwrap_or_default());
            vec![
                Cell::text(name),
                Cell::text(loc.description),
                Cell::text(loc.city),
                Cell::text(loc.region),
                Cell::text(loc.country),
                details,
                fringes,
                total,
            ]
        })
        .collect();
    Sheet {
        name: "Locations",
        columns: columns(&[
            "Location",
            "Description",
            "City",
            "Region",
            "Country",
            "Details Total",
            "Fringe Total",
            "Total",
        ]),
        rows,
    }
}
