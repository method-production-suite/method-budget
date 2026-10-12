//! Building the export's tables from a budget and the report computed from it.

use std::collections::{BTreeMap, BTreeSet};

use method_core::expr::{quick_value, Expr};
use method_core::{
    Breakdown, Budget, Cents, Charge, Credit, Dec, Formula, FringeKind, GroupTotals, Report,
    RowKind, TopsheetLine,
};

use super::{lines_up, Cell, ExportError, Sheet, Workbook};

/// Every table, from `b` and the report `r` computed from it with the
/// default options (`method_core::recalc(b, &RecalcOptions::default())`).
pub fn workbook(b: &Budget, r: &Report) -> Result<Workbook, ExportError> {
    if !lines_up(b, r) {
        return Err(ExportError::ReportMismatch);
    }
    let mut sheets = vec![
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
    ];
    // A budget with actuals also gets its cost report and transactions.
    if !b.actuals.is_empty() {
        if let Some(c) = method_core::actuals::cost_report_of(b, r) {
            sheets.push(cost_report(&c));
        }
        sheets.push(actuals(b));
    }
    Ok(Workbook {
        title: b.title.clone(),
        sheets,
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
/// above- and below-the-line totals, the budget's fringe row, charges,
/// grand total, credits and net, in the order [`Report::topsheet_lines`]
/// lists them.
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
    // In the topsheet's order (decisions.md, entry 59): the budget's fringe
    // row where it is placed, charges newest first, credits in reverse
    // application order with their subtotal.
    let charges: Vec<&Charge> = b.charges.iter().filter(|c| c.applied).collect();
    let credits: Vec<&Credit> = b.credits.iter().filter(|c| c.applied).collect();
    for l in r.topsheet_lines(b) {
        match l {
            TopsheetLine::Category(i) => {
                let cat = &r.categories[i];
                row("Category", &cat.code, &cat.name, Cell::Money(cat.total));
            }
            TopsheetLine::LevelFringes(i) => {
                row(
                    "Fringes",
                    "",
                    "Fringes",
                    Cell::Money(r.levels[i].posted_fringes),
                );
            }
            TopsheetLine::Level(i) => {
                let level = &r.levels[i];
                row(
                    "Production Total",
                    "",
                    &level.name,
                    Cell::Money(level.total),
                );
            }
            TopsheetLine::Atl => row(
                "Total",
                "",
                "Total above-the-line",
                Cell::Money(r.total_atl),
            ),
            TopsheetLine::Btl => row(
                "Total",
                "",
                "Total below-the-line",
                Cell::Money(r.total_btl),
            ),
            TopsheetLine::AtlBtl => row(
                "Total",
                "",
                "Total above and below-the-line",
                Cell::Money(r.atl_btl),
            ),
            TopsheetLine::BudgetFringes => row(
                "Fringes",
                b.fringe_row.account().unwrap_or(""),
                b.fringe_row.description(),
                Cell::Money(r.budget_fringes),
            ),
            TopsheetLine::Charge(i) => {
                let ct = &r.charges[i];
                let code = charges.get(i).and_then(|c| c.account.as_deref());
                row(
                    "Charge",
                    code.unwrap_or(""),
                    &ct.name,
                    Cell::Money(ct.amount),
                );
            }
            TopsheetLine::Grand => row("Total", "", "Grand total", Cell::Money(r.grand_total)),
            TopsheetLine::Credit(i) => {
                let ct = &r.credits[i];
                let code = credits.get(i).and_then(|c| c.account.as_deref());
                row(
                    "Credit",
                    code.unwrap_or(""),
                    &format!("Less {}", ct.name),
                    Cell::Money(ct.amount.saturating_neg()),
                );
            }
            TopsheetLine::CreditsSubtotal => row(
                "Total",
                "",
                "Sub Total",
                Cell::Money(r.credits_total.saturating_neg()),
            ),
            TopsheetLine::Net => row("Total", "", "Net total", Cell::Money(r.net_total)),
        }
    }
    if r.totals_currency != r.base_currency {
        let cur = &r.totals_currency;
        row(
            "Total",
            "",
            &format!(
                "Grand total in {cur} (1 {cur} = {} {})",
                r.totals_rate, r.base_currency
            ),
            money_or_empty(r.display.as_ref().map(|d| d.grand_total)),
        );
        row(
            "Total",
            "",
            &format!("Net total in {cur}"),
            money_or_empty(r.display.as_ref().map(|d| d.net_total)),
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
                    Cell::text(method_core::notes_text(&d.notes)),
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
            // A flat fringe's kept cutoff is never used, so it isn't shown.
            let (kind, cutoff, unit, basis, max_hours) = match &f.kind {
                FringeKind::Percent { cutoff } => ("Percent", *cutoff, None, "", None),
                FringeKind::Flat {
                    unit,
                    basis,
                    max_hours_per_day,
                    ..
                } => (
                    "Flat",
                    None,
                    unit.as_deref(),
                    basis.label(),
                    *max_hours_per_day,
                ),
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
                Cell::text(basis),
                max_hours.map_or(Cell::Empty, Cell::Number),
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
            "Basis",
            "Max Hours Per Day",
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
            let [details, fringes, _] = breakdown_cells(&t.full);
            // Total and Counted Total as the app shows them: without the
            // fringes when the group rule `fringes_in_totals` is Exclude.
            let shown = t.shown(&b.group_rules);
            vec![
                Cell::text(name),
                Cell::text(&g.description),
                yes_no(g.in_budget_total),
                details,
                fringes,
                money_or_empty(shown.map(|(total, _)| total)),
                money_or_empty(shown.map(|(_, counted)| counted)),
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

// ---------------------------------------------------------------------------
// Actuals (only for a budget that has them)

/// Cost Report: every account, category fringe account and category with
/// its budget, baseline, actual, ETC, EAC, variance and percent spent, then
/// the production levels, the summary rows, unassigned actuals and the
/// grand total (method_core::actuals::cost_report).
fn cost_report(c: &method_core::actuals::CostReport) -> Sheet {
    use method_core::actuals::{CostFigures, EtcMethod};
    let mut rows = Vec::new();
    let method = |m: Option<EtcMethod>| match m {
        None | Some(EtcMethod::BudgetMinusActual) => Cell::Empty,
        Some(EtcMethod::Manual(_)) => Cell::text("Manual"),
        Some(EtcMethod::PercentComplete(p)) => Cell::text(format!("{p}% complete")),
    };
    let mut row = |kind: &str, code: &str, name: &str, f: &CostFigures, m: Cell| {
        rows.push(vec![
            Cell::text(kind),
            Cell::text(code),
            Cell::text(name),
            Cell::Money(f.budget),
            money_or_empty(f.baseline),
            Cell::Money(f.actual),
            Cell::Int(f.count),
            Cell::Money(f.etc),
            m,
            Cell::Money(f.eac),
            Cell::Money(f.variance),
            number_or_empty(f.percent_spent),
        ]);
    };
    for lv in &c.levels {
        for cat in c.categories.get(lv.start..lv.end).unwrap_or_default() {
            for a in &cat.accounts {
                row("Account", &a.code, &a.name, &a.figures, method(a.method));
            }
            if let Some(fa) = &cat.fringe_account {
                row(
                    "Fringe Account",
                    &fa.code,
                    &fa.name,
                    &fa.figures,
                    method(fa.method),
                );
            }
            row("Category", &cat.code, &cat.name, &cat.figures, Cell::Empty);
        }
        if let Some(f) = &lv.posted_fringes {
            row("Fringes", "", "Fringes", f, Cell::Empty);
        }
        if !lv.name.is_empty() {
            row("Production Total", "", &lv.name, &lv.figures, Cell::Empty);
        }
    }
    row(
        "Total",
        "",
        "Total above-the-line",
        &c.total_atl,
        Cell::Empty,
    );
    row(
        "Total",
        "",
        "Total below-the-line",
        &c.total_btl,
        Cell::Empty,
    );
    row(
        "Total",
        "",
        "Total above and below-the-line",
        &c.atl_btl,
        Cell::Empty,
    );
    if let Some(f) = &c.budget_fringes {
        row("Fringes", "", &f.name, &f.figures, Cell::Empty);
    }
    for ch in &c.charges {
        row("Charge", "", &ch.name, &ch.figures, Cell::Empty);
    }
    if c.unassigned.count > 0 {
        row(
            "Unassigned",
            "",
            "Unassigned actuals",
            &c.unassigned,
            Cell::Empty,
        );
    }
    row("Total", "", "Grand total", &c.total, Cell::Empty);
    Sheet {
        name: "Cost Report",
        columns: columns(&[
            "Type",
            "Code",
            "Description",
            "Budget",
            "Baseline",
            "Actual",
            "Transactions",
            "Estimate to Complete",
            "ETC Method",
            "Estimate at Completion",
            "Variance",
            "Percent Spent",
        ]),
        rows,
    }
}

/// Actuals: every transaction, in the order they came in, with the name of
/// its account (empty when the chart doesn't have it) and the line it pays
/// for, if any.
fn actuals(b: &Budget) -> Sheet {
    let chart = method_core::actuals::chart(b);
    let coding = b.account_coding;
    let lines: BTreeMap<method_core::RowId, &str> = b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .map(|d| (d.id, d.description.as_str()))
        .collect();
    let files: BTreeMap<u64, &str> = b
        .actuals
        .batches
        .iter()
        .map(|x| (x.id, x.file.as_str()))
        .collect();
    let rows = b
        .actuals
        .transactions
        .iter()
        .map(|t| {
            let entry = chart.get(&method_core::actuals::normalize_code(&coding, &t.account));
            vec![
                Cell::text(&t.date),
                Cell::text(&t.account),
                Cell::text(entry.map_or("", |e| e.name.as_str())),
                Cell::text(&t.vendor),
                Cell::text(&t.description),
                Cell::text(&t.reference),
                list(&t.tags),
                Cell::Money(t.amount),
                Cell::text(t.line.and_then(|l| lines.get(&l).copied()).unwrap_or("")),
                Cell::text(t.batch.and_then(|x| files.get(&x).copied()).unwrap_or("")),
            ]
        })
        .collect();
    Sheet {
        name: "Actuals",
        columns: columns(&[
            "Date",
            "Account",
            "Account Name",
            "Vendor",
            "Description",
            "Reference",
            "Tags",
            "Amount",
            "Line",
            "Imported From",
        ]),
        rows,
    }
}
