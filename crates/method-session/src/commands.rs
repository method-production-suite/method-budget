//! What each budget command does, as typed functions on a [`Session`]: the
//! desktop app's Tauri commands call these (method-app's src/app.rs and
//! src/display.rs), and so does [`crate::host::Host::dispatch`]. Each edit
//! names its undo step here ("Add account 21-01 Camera"), so both hosts
//! label the History list alike.
//!
//! A command that changes the budget returns what the edit returned; the
//! host then draws the view ([`crate::docs::DocGuard::finish`]).

use method_core::{Budget, RowId, TopsheetRow};

use crate::display::{self, DisplayPrefsInput, GlobalInfoInput};
use crate::edit::{
    self, ChargeInput, CreditInput, CurrencyInput, EditResult, FringeInput, FringeRowInput,
    GroupInput, InfoInput, LocationInput, SetInput, UnitInput,
};
use crate::messages::{msg, word};
use crate::report_dto::{ChargeDto, IncentivesDto, LocationReportDto};
use crate::session::Session;
use crate::view::Focus;

/// "Add account 21-01 Camera": a code and a name (`named`'s words), or a
/// code alone (`unnamed`'s) when the name is blank.
fn label2(named: &str, unnamed: &str, code: &str, name: &str) -> String {
    let (code, name) = (code.trim(), name.trim());
    if name.is_empty() {
        msg(unnamed, &[("code", &code)]).trim_end().to_string()
    } else {
        msg(named, &[("code", &code), ("name", &name)])
    }
}

/// "Add fringe FICA" or "Edit fringe FICA": `codes` are the add and the
/// edit.
fn upsert_label(codes: [&str; 2], exists: bool, name: &str) -> String {
    let [add, edit] = codes;
    let code = if exists { edit } else { add };
    msg(code, &[("name", &name.trim())])
}

/// "Edit category 10", "Delete production total Total above the line":
/// `codes` are for a category, a production total and neither.
fn topsheet_label(codes: [&str; 3], b: &Budget, index: usize) -> String {
    let [category, total, neither] = codes;
    match b.topsheet.get(index) {
        Some(TopsheetRow::Category(c)) => msg(category, &[("code", &c.code)]),
        Some(TopsheetRow::ProductionTotal(p)) => msg(total, &[("name", &p.name)]),
        None => word(neither),
    }
}

const EDIT_TOPSHEET: [&str; 3] = [
    "engine.undo.edit_category",
    "engine.undo.edit_production_total",
    "engine.undo.edit_topsheet_row",
];
const DELETE_TOPSHEET: [&str; 3] = [
    "engine.undo.delete_category",
    "engine.undo.delete_production_total",
    "engine.undo.delete_topsheet_row",
];

/// [`Session::edit`] with the step's label worked out from the budget as it
/// is before the edit (to name a row by its account and line).
fn edit_as<T>(
    s: &mut Session,
    label: impl FnOnce(&Budget) -> String,
    f: impl FnOnce(&mut Budget) -> EditResult<T>,
) -> EditResult {
    let label = label(s.budget());
    s.edit(label, f).map(drop)
}

// ---------------------------------------------------------------------------
// Views

/// The account on screen, as the UI names it: category code and topsheet
/// index, account code and index in the category (codes win when they
/// disagree). `None` for the category or the account: no account.
pub fn focus_account(
    s: &mut Session,
    category: Option<String>,
    ti: Option<usize>,
    account: Option<String>,
    ai: Option<usize>,
) {
    let focus = category.zip(account).map(|(category, account)| Focus {
        category,
        ti: ti.unwrap_or(usize::MAX),
        account,
        ai: ai.unwrap_or(usize::MAX),
    });
    s.set_focus(focus);
}

/// What charge `index` (or a new charge, `None`) would come to as `charge`
/// describes it, without changing anything: the charges tab's live total
/// while a value is still being typed. Figured on the current totals, so it
/// matches what saving the edit gives.
pub fn preview_charge(
    s: &mut Session,
    index: Option<usize>,
    charge: &ChargeInput,
) -> Result<ChargeDto, String> {
    let (budget, outcome) = s.totals();
    let c = edit::build_charge(budget, index, charge)?;
    let report = outcome
        .report
        .as_ref()
        .ok_or("the totals can't be worked out until the budget's problems are fixed")?;
    let figured = outcome.sanitized.as_ref().unwrap_or(budget);
    let t = method_core::charge_preview(figured, report, &c).map_err(|e| e.to_string())?;
    Ok(ChargeDto::from(&t))
}

/// Spend by location and an estimate for every credit, applied or not
/// (roadmap 3.6; decisions.md, entry 80): the Credits tab's programs side
/// by side. Figured on the current totals; nothing changes.
pub fn incentives(s: &mut Session) -> Result<IncentivesDto, String> {
    let (budget, outcome) = s.totals();
    let report = outcome
        .report
        .as_ref()
        .ok_or("the totals can't be worked out until the budget's problems are fixed")?;
    let figured = outcome.sanitized.as_ref().unwrap_or(budget);
    let i = method_core::incentives::incentives(figured, report).map_err(|e| e.to_string())?;
    let mut dto = IncentivesDto::from(&i);
    // A cleaned copy counts what has a problem as blank (a credit row on a
    // missing group as 0): say so, and name each program's own problems,
    // so a short estimate never shows as a whole one.
    dto.partial = outcome.sanitized.is_some();
    for p in &mut dto.programs {
        own_problems(outcome, p.index, &mut p.problem);
    }
    Ok(dto)
}

/// A credit's own problems in a budget figured from its cleaned copy, in
/// `problem` unless it has one already.
fn own_problems(outcome: &crate::diagnose::Outcome, index: usize, problem: &mut Option<String>) {
    let own: Vec<&str> = outcome
        .issues
        .iter()
        .filter(|x| x.target == crate::diagnose::Target::Credit { index })
        .map(|x| x.message.as_str())
        .collect();
    if !own.is_empty() && problem.is_none() {
        *problem = Some(format!(
            "{}; counted as 0 here, so the estimate may be short",
            own.join("; ")
        ));
    }
}

/// The printed per-location incentive report's figures (decisions.md,
/// entry 93), from the same estimates as [`incentives`]. Nothing changes.
pub fn location_report(s: &mut Session) -> Result<LocationReportDto, String> {
    let (budget, outcome) = s.totals();
    let report = outcome
        .report
        .as_ref()
        .ok_or("the totals can't be worked out until the budget's problems are fixed")?;
    let figured = outcome.sanitized.as_ref().unwrap_or(budget);
    let i = method_core::incentives::incentives(figured, report).map_err(|e| e.to_string())?;
    let r = method_core::incentives::by_location::location_report(report, &i)
        .map_err(|e| e.to_string())?;
    let mut dto = LocationReportDto::from(&r);
    dto.partial = outcome.sanitized.is_some();
    for p in &mut dto.programs {
        own_problems(outcome, p.index, &mut p.problem);
    }
    Ok(dto)
}

// ---------------------------------------------------------------------------
// Structure

pub fn set_title(s: &mut Session, title: &str) -> EditResult {
    let label = msg("engine.undo.rename_budget", &[("title", &title.trim())]);
    s.edit(label, |b| edit::set_title(b, title))
}

pub fn add_category(s: &mut Session, code: &str, name: &str, at: Option<usize>) -> EditResult {
    let label = label2(
        "engine.undo.add_category",
        "engine.undo.add_category_unnamed",
        code,
        name,
    );
    s.edit(label, |b| edit::add_category(b, code, name, at).map(drop))
}

pub fn add_production_total(s: &mut Session, name: &str, at: Option<usize>) -> EditResult {
    let label = msg(
        "engine.undo.add_production_total",
        &[("name", &name.trim())],
    );
    s.edit(label, |b| edit::add_production_total(b, name, at).map(drop))
}

pub fn update_topsheet_row(
    s: &mut Session,
    index: usize,
    code: Option<&str>,
    name: &str,
) -> EditResult {
    edit_as(
        s,
        |b| topsheet_label(EDIT_TOPSHEET, b, index),
        |b| edit::update_topsheet_row(b, index, code, name),
    )
}

pub fn delete_topsheet_row(s: &mut Session, index: usize) -> EditResult {
    edit_as(
        s,
        |b| topsheet_label(DELETE_TOPSHEET, b, index),
        |b| edit::delete_topsheet_row(b, index),
    )
}

pub fn add_account(
    s: &mut Session,
    category_code: &str,
    code: &str,
    name: &str,
    after: Option<&str>,
) -> EditResult {
    let label = label2(
        "engine.undo.add_account",
        "engine.undo.add_account_unnamed",
        code,
        name,
    );
    s.edit(label, |b| {
        edit::add_account(b, category_code, code, name, after)
    })
    .map(drop)
}

pub fn update_account(s: &mut Session, code: &str, new_code: &str, name: &str) -> EditResult {
    let label = if code.trim() == new_code.trim() {
        msg("engine.undo.rename_account", &[("code", &code.trim())])
    } else {
        msg(
            "engine.undo.renumber_account",
            &[("code", &code.trim()), ("new", &new_code.trim())],
        )
    };
    s.edit(label, |b| edit::update_account(b, code, new_code, name))
        .map(drop)
}

pub fn delete_account(s: &mut Session, code: &str) -> EditResult {
    let label = msg("engine.undo.delete_account", &[("code", &code.trim())]);
    s.edit(label, |b| edit::delete_account(b, code))
}

// ---------------------------------------------------------------------------
// Detail rows

pub fn set_detail_field(s: &mut Session, row_id: RowId, field: &str, value: &str) -> EditResult {
    s.set_detail_field(row_id, field, value)
}

pub fn set_rows_field(s: &mut Session, row_ids: &[RowId], field: &str, value: &str) -> EditResult {
    let parsed = edit::Field::parse(field)?;
    edit_as(
        s,
        |b| crate::history::rows_field_label(b, row_ids, field),
        |b| edit::set_rows_field(b, row_ids, parsed, value),
    )
}

/// The new row's id.
pub fn add_detail(
    s: &mut Session,
    account_code: &str,
    after: Option<RowId>,
    kind: &str,
) -> EditResult<RowId> {
    s.add_detail(account_code, after, kind)
}

pub fn delete_details(s: &mut Session, row_ids: &[RowId]) -> EditResult {
    edit_as(
        s,
        |b| crate::history::delete_rows_label(b, row_ids),
        |b| edit::delete_details(b, row_ids),
    )
}

/// Sets many detail cells as one undo step, recalculating once. `action`
/// is what the page was doing (paste, fill, cut, clear), for the step's
/// name in the History list; only known words are used.
pub fn set_cells(
    s: &mut Session,
    edits: &[crate::batch::CellEdit],
    action: Option<&str>,
) -> EditResult {
    let label = |b: &Budget| {
        let cells: Vec<(RowId, &str, &str)> = edits
            .iter()
            .map(|e| (e.row_id, e.field.as_str(), e.value.as_str()))
            .collect();
        crate::history::cells_label(b, &cells, action)
    };
    edit_as(s, label, |b| crate::batch::set_cells(b, edits))
}

// ---------------------------------------------------------------------------
// Setup tables

pub fn set_global(s: &mut Session, name: &str, formula: &str) -> EditResult {
    edit_as(
        s,
        |b| {
            let code = if b.globals.contains_key(name.trim()) {
                "engine.undo.set_global"
            } else {
                "engine.undo.add_global"
            };
            msg(
                code,
                &[("name", &name.trim()), ("formula", &formula.trim())],
            )
        },
        |b| edit::set_global(b, name, formula),
    )
}

pub fn delete_global(s: &mut Session, name: &str) -> EditResult {
    let label = msg("engine.undo.delete_global", &[("name", &name.trim())]);
    s.edit(label, |b| edit::delete_global(b, name))
}

/// A global's description, unit, decimals, show-by-name and folder.
pub fn set_global_info(s: &mut Session, name: &str, info: &GlobalInfoInput) -> EditResult {
    let label = msg("engine.undo.edit_global_details", &[("name", &name.trim())]);
    s.edit(label, |b| display::apply_global_info(b, name, info))
}

/// The budget's numbers-on-screen settings.
pub fn set_display_prefs(s: &mut Session, prefs: &DisplayPrefsInput) -> EditResult {
    s.edit(word("engine.undo.numbers_on_screen"), |b| {
        display::apply_display_prefs(b, prefs)
    })
}

pub fn set_fringe(s: &mut Session, name: &str, fringe: &FringeInput) -> EditResult {
    edit_as(
        s,
        |b| {
            upsert_label(
                ["engine.undo.add_fringe", "engine.undo.edit_fringe"],
                b.fringes.contains_key(name.trim()),
                name,
            )
        },
        |b| edit::set_fringe(b, name, fringe),
    )
}

pub fn delete_fringe(s: &mut Session, name: &str) -> EditResult {
    let label = msg("engine.undo.delete_fringe", &[("name", &name.trim())]);
    s.edit(label, |b| edit::delete_fringe(b, name))
}

pub fn set_group(s: &mut Session, name: &str, group: &GroupInput) -> EditResult {
    edit_as(
        s,
        |b| {
            upsert_label(
                ["engine.undo.add_group", "engine.undo.edit_group"],
                b.groups.contains_key(name.trim()),
                name,
            )
        },
        |b| edit::set_group(b, name, group),
    )
}

/// One of the budget's group rules (decisions.md, entry 57): `rule` is
/// `ungrouped_lines`, `conflict` or `fringes_in_totals`, `value` is
/// `include` or `exclude`.
pub fn set_group_rule(s: &mut Session, rule: &str, value: &str) -> EditResult {
    s.edit(word("engine.undo.group_rule"), |b| {
        edit::set_group_rule(b, rule, value)
    })
}

/// How the budget works out its totals (decisions.md, entry 55): `value` is
/// `binary` or `exact`.
pub fn set_arithmetic(s: &mut Session, value: &str) -> EditResult {
    s.edit(word("engine.undo.rounding"), |b| {
        edit::set_arithmetic(b, value)
    })
}

/// Whether account totals are rounded to whole units (decisions.md, entry
/// 69).
pub fn set_round_account_totals(s: &mut Session, value: bool) -> EditResult {
    s.edit(word("engine.undo.account_total_rounding"), |b| {
        edit::set_round_account_totals(b, value)
    })
}

/// What every percent fringe's cutoff is multiplied by (decisions.md, entry
/// 59): `value` is `1.0`, `1.5`, `2.0`, `2.5` or `3.0`.
pub fn set_cutoff_factor(s: &mut Session, value: &str) -> EditResult {
    s.edit(word("engine.undo.cutoff_factor"), |b| {
        edit::set_cutoff_factor(b, value)
    })
}

/// The budget's fringe row: its account number, description and place on
/// the topsheet (decisions.md, entry 60).
pub fn set_fringe_row(s: &mut Session, row: &FringeRowInput) -> EditResult {
    s.edit(word("engine.undo.fringe_row"), |b| {
        edit::set_fringe_row(b, row)
    })
}

/// The fringes of categories after the last production total (decisions.md,
/// entry 56): `value` is `keep` or `drop`.
pub fn set_trailing_level_fringes(s: &mut Session, value: &str) -> EditResult {
    s.edit(word("engine.undo.trailing_fringes"), |b| {
        edit::set_trailing_level_fringes(b, value)
    })
}

pub fn delete_group(s: &mut Session, name: &str) -> EditResult {
    let label = msg("engine.undo.delete_group", &[("name", &name.trim())]);
    s.edit(label, |b| edit::delete_group(b, name))
}

pub fn set_unit(s: &mut Session, name: &str, unit: &UnitInput) -> EditResult {
    edit_as(
        s,
        |b| {
            upsert_label(
                ["engine.undo.add_unit", "engine.undo.edit_unit"],
                b.units.contains_key(name.trim()),
                name,
            )
        },
        |b| edit::set_unit(b, name, unit),
    )
}

/// `clear_lines`: also blank the unit columns that name it.
pub fn delete_unit(s: &mut Session, name: &str, clear_lines: bool) -> EditResult {
    let label = msg("engine.undo.delete_unit", &[("name", &name.trim())]);
    s.edit(label, |b| edit::delete_unit_with(b, name, clear_lines))
}

pub fn set_currency(s: &mut Session, code: &str, currency: &CurrencyInput) -> EditResult {
    edit_as(
        s,
        |b| {
            let code_ = code.trim();
            let exists = b.currencies.contains_key(code_);
            let c = if exists {
                "engine.undo.edit_currency"
            } else {
                "engine.undo.add_currency"
            };
            msg(c, &[("code", &code_)])
        },
        |b| edit::set_currency(b, code, currency),
    )
}

/// `clear_lines`: lines in it go back to the base currency first.
pub fn delete_currency(s: &mut Session, code: &str, clear_lines: bool) -> EditResult {
    let label = msg("engine.undo.delete_currency", &[("code", &code.trim())]);
    s.edit(label, |b| edit::delete_currency_with(b, code, clear_lines))
}

// ---------------------------------------------------------------------------
// Charges, credits, sets, locations, budget info, settings and the baseline
// (the topsheet setup tabs in ui/js/topsheet-setup.js)

pub fn set_charge(s: &mut Session, index: Option<usize>, charge: &ChargeInput) -> EditResult {
    let label = word(if index.is_some() {
        "engine.undo.edit_charge"
    } else {
        "engine.undo.add_charge"
    });
    s.edit(label, |b| edit::set_charge(b, index, charge))
        .map(drop)
}

pub fn delete_charge(s: &mut Session, index: usize) -> EditResult {
    s.edit(word("engine.undo.delete_charge"), |b| {
        edit::delete_charge(b, index)
    })
}

pub fn move_charge(s: &mut Session, index: usize, to: usize) -> EditResult {
    s.edit(word("engine.undo.move_charge"), |b| {
        edit::move_charge(b, index, to)
    })
}

pub fn set_credit(s: &mut Session, index: Option<usize>, credit: &CreditInput) -> EditResult {
    let label = word(if index.is_some() {
        "engine.undo.edit_credit"
    } else {
        "engine.undo.add_credit"
    });
    s.edit(label, |b| edit::set_credit(b, index, credit))
        .map(drop)
}

pub fn delete_credit(s: &mut Session, index: usize) -> EditResult {
    s.edit(word("engine.undo.delete_credit"), |b| {
        edit::delete_credit(b, index)
    })
}

pub fn move_credit(s: &mut Session, index: usize, to: usize) -> EditResult {
    s.edit(word("engine.undo.move_credit"), |b| {
        edit::move_credit(b, index, to)
    })
}

pub fn set_set(s: &mut Session, name: &str, set: &SetInput) -> EditResult {
    edit_as(
        s,
        |b| {
            upsert_label(
                ["engine.undo.add_set", "engine.undo.edit_set"],
                b.sets.contains_key(name.trim()),
                name,
            )
        },
        |b| edit::set_set(b, name, set),
    )
}

pub fn delete_set(s: &mut Session, name: &str) -> EditResult {
    let label = msg("engine.undo.delete_set", &[("name", &name.trim())]);
    s.edit(label, |b| edit::delete_set(b, name))
}

pub fn set_location(s: &mut Session, name: &str, location: &LocationInput) -> EditResult {
    edit_as(
        s,
        |b| {
            upsert_label(
                ["engine.undo.add_location", "engine.undo.edit_location"],
                b.locations.contains_key(name.trim()),
                name,
            )
        },
        |b| edit::set_location(b, name, location),
    )
}

pub fn delete_location(s: &mut Session, name: &str) -> EditResult {
    let label = msg("engine.undo.delete_location", &[("name", &name.trim())]);
    s.edit(label, |b| edit::delete_location(b, name))
}

pub fn set_info(s: &mut Session, name: &str, value: &InfoInput) -> EditResult {
    let label = msg("engine.undo.edit_info", &[("name", &name.trim())]);
    s.edit(label, |b| edit::set_info(b, name, value))
}

pub fn delete_info(s: &mut Session, name: &str) -> EditResult {
    let label = msg("engine.undo.delete_info", &[("name", &name.trim())]);
    s.edit(label, |b| edit::delete_info(b, name))
}

pub fn set_fringe_posting(s: &mut Session, mode: &str, suffix: Option<&str>) -> EditResult {
    s.edit(word("engine.undo.fringe_posting"), |b| {
        edit::set_fringe_posting(b, mode, suffix)
    })
}

/// Category, account, sub-account and set widths, and the separator.
pub fn set_account_coding(s: &mut Session, widths: [u8; 4], separator: Option<&str>) -> EditResult {
    s.edit(word("engine.undo.account_numbering"), |b| {
        edit::set_account_coding(b, widths, separator)
    })
}

pub fn set_totals_currency(s: &mut Session, code: Option<&str>) -> EditResult {
    s.edit(word("engine.undo.totals_currency"), |b| {
        edit::set_totals_currency(b, code)
    })
}

pub fn lock_baseline(s: &mut Session) -> EditResult {
    s.edit(word("engine.undo.lock_baseline"), edit::lock_baseline)
}

pub fn set_baseline_frozen(s: &mut Session, frozen: bool) -> EditResult {
    let label = word(if frozen {
        "engine.undo.freeze_totals"
    } else {
        "engine.undo.unfreeze_totals"
    });
    s.edit(label, |b| edit::set_baseline_frozen(b, frozen))
}

pub fn clear_baseline(s: &mut Session) -> EditResult {
    s.edit(word("engine.undo.clear_baseline"), edit::clear_baseline)
}
