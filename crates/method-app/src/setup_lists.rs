//! What the tools panel's setup lists need beyond the tables themselves:
//! how much each item is used (roadmap 2.9), their hand-made order (2.12)
//! and the short codes of fringes and groups (2.11).
//!
//! - [`setup_usage`]: for every item of every table, the lines that use it,
//!   other setup that refers to it, and its dollars where that means
//!   something (fringes: the fringe dollars; groups, sets and locations:
//!   their lines' totals with fringes, a group out of the total counted as
//!   if it were in, and a group without fringes when the group rule says
//!   so; currencies: their lines' totals in the base currency).
//!   Fetched by the panel when the budget changes, not sent with every view.
//! - [`set_setup_order`] and [`move_setup_item`]: a table's order, saved in
//!   the file (`method_core::order`). Moving by key joins quick repeats into
//!   one undo step, like moving rows.
//! - [`set_short_code`]: a fringe's or group's short code.

use std::collections::BTreeMap;

use method_core::usage::usage;
use method_core::{Budget, Cents, Report, SetupKind};
use serde::Serialize;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{edit_budget, finish, AppState, DocId};
use crate::edit::EditResult;
use crate::view::View;

/// One item's use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct UsageDto {
    /// Detail rows that name it.
    pub lines: usize,
    /// Other setup that names it (see `method_core::usage::ItemUse`).
    pub other: usize,
    /// Its dollars, in the base currency, where that means something and
    /// totals could be worked out; else `None`.
    pub total: Option<Cents>,
}

/// The table key the file and the page use for a kind (`fringes`...).
pub fn table_key(kind: SetupKind) -> &'static str {
    match kind {
        SetupKind::Global => "globals",
        SetupKind::Fringe => "fringes",
        SetupKind::Group => "groups",
        SetupKind::Unit => "units",
        SetupKind::Set => "sets",
        SetupKind::Location => "locations",
        SetupKind::Currency => "currencies",
    }
}

/// A kind from its name (`fringe`) or table key (`fringes`).
pub fn parse_kind(s: &str) -> EditResult<SetupKind> {
    SetupKind::from_name(s)
        .or_else(|| SetupKind::ALL.iter().copied().find(|k| table_key(*k) == s))
        .ok_or_else(|| format!("unknown kind of setup item `{s}`"))
}

/// The dollars of item `name` of `kind`, from the report.
fn dollars(b: &Budget, report: &Report, kind: SetupKind, name: &str) -> Option<Cents> {
    let both = |bd: &method_core::Breakdown| bd.details_total.checked_add(bd.fringe_total);
    match kind {
        SetupKind::Fringe => Some(report.fringes.get(name).map_or(0, |f| f.total)),
        SetupKind::Group => {
            let g = report.groups.get(name)?;
            let counted = b.groups.get(name).is_none_or(|d| d.in_budget_total);
            let (total, active) = g.shown(&b.group_rules)?;
            Some(if counted { active } else { total })
        }
        SetupKind::Set => report.sets.get(name).map_or(Some(0), both),
        SetupKind::Location => report.locations.get(name).map_or(Some(0), both),
        SetupKind::Currency => report
            .categories
            .iter()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.rows)
            .filter(|r| r.currency == name)
            .try_fold(0i64, |sum, r| sum.checked_add(r.total)),
        SetupKind::Global | SetupKind::Unit => None,
    }
}

/// Every table's use, by table key and item name.
pub fn usage_of(
    b: &Budget,
    report: Option<&Report>,
) -> BTreeMap<&'static str, BTreeMap<String, UsageDto>> {
    SetupKind::ALL
        .iter()
        .map(|&kind| {
            let items = usage(b, kind)
                .into_iter()
                .map(|(name, u)| {
                    let total = report.and_then(|r| dollars(b, r, kind, &name));
                    let dto = UsageDto {
                        lines: u.lines,
                        other: u.other,
                        total,
                    };
                    (name, dto)
                })
                .collect();
            (table_key(kind), items)
        })
        .collect()
}

/// `{}`: how much every setup item of budget `doc` is used.
#[tauri::command]
pub fn setup_usage(
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> Result<BTreeMap<&'static str, BTreeMap<String, UsageDto>>, String> {
    let mut s = state.lock_doc(doc)?;
    let (b, outcome) = s.totals();
    Ok(usage_of(b, outcome.report.as_ref()))
}

/// `{kind, names}`: the table's order, first to last (names it doesn't
/// have are skipped; the rest follow in name order).
#[tauri::command]
pub fn set_setup_order<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    kind: String,
    names: Vec<String>,
) -> Result<View, String> {
    let kind = parse_kind(&kind)?;
    edit_budget(
        &window,
        &state,
        doc,
        format!("Reorder {}", kind.name()),
        |b| {
            method_core::order::set_order(b, kind, &names);
            Ok(())
        },
    )
}

/// Moves item `name` `by` places (negative is up) in its table's order.
/// Refused at either end. Quick repeats on the same item are one undo step.
pub fn move_item(b: &mut Budget, kind: SetupKind, name: &str, by: i32) -> EditResult {
    if !method_core::rename::exists(b, kind, name) {
        return Err(format!("{} {name} no longer exists", kind.name()));
    }
    let by = isize::try_from(by).unwrap_or(0);
    if !method_core::order::move_by(b, kind, name, by) {
        return Err(format!("{name} can't move further"));
    }
    Ok(())
}

/// `{kind, name, by}`.
#[tauri::command]
pub fn move_setup_item<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    kind: String,
    name: String,
    by: i32,
) -> Result<View, String> {
    let kind = parse_kind(&kind)?;
    let mut s = state.lock_doc(doc)?;
    let key = format!("setup-order\u{1}{}\u{1}{name}", kind.name());
    let label = format!(
        "Move {} {} {}",
        kind.name(),
        name.trim(),
        if by < 0 { "up" } else { "down" }
    );
    s.edit_merging(&key, label, |b| {
        move_item(b, kind, &name, by).map(|()| ((), key.clone()))
    })?;
    Ok(finish(&window, &mut s, None))
}

/// `{kind, name, code}`: a fringe's or group's short code; blank clears it.
#[tauri::command]
pub fn set_short_code<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    kind: String,
    name: String,
    code: String,
) -> Result<View, String> {
    let kind = parse_kind(&kind)?;
    edit_budget(
        &window,
        &state,
        doc,
        format!("Set the short code of {}", name.trim()),
        |b| crate::edit::set_short_code(b, kind, &name, &code),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{self, FringeInput, GroupInput};
    use crate::session::Session;
    use method_core::{SetDef, TopsheetRow};

    fn rows(b: &mut Budget) -> Vec<&mut method_core::Detail> {
        b.topsheet
            .iter_mut()
            .filter_map(|r| match r {
                TopsheetRow::Category(c) => Some(c),
                TopsheetRow::ProductionTotal(_) => None,
            })
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
            .filter(|d| d.kind == method_core::RowKind::Line)
            .collect()
    }

    #[test]
    fn counts_and_totals_follow_the_lines() {
        let mut s = Session::new();
        s.edit("Test edit", |b| {
            edit::set_set(
                b,
                "INT",
                &edit::SetInput {
                    description: String::new(),
                },
            )?;
            edit::set_set(
                b,
                "EXT",
                &edit::SetInput {
                    description: String::new(),
                },
            )?;
            edit::set_currency(
                b,
                "CAD",
                &edit::CurrencyInput {
                    description: String::new(),
                    symbol: String::new(),
                    rate: "0.5".into(),
                    symbol_after: None,
                    separators: None,
                },
            )?;
            let mut r = rows(b);
            r[0].set = Some("INT".into());
            r[1].set = Some("INT".into());
            r[2].currency = Some("CAD".into());
            Ok(())
        })
        .unwrap();
        let (b, outcome) = s.totals();
        let report = outcome.report.as_ref().unwrap();
        let u = usage_of(b, Some(report));
        assert_eq!(u["sets"]["INT"].lines, 2);
        assert_eq!(
            u["sets"]["EXT"],
            UsageDto {
                lines: 0,
                other: 0,
                total: Some(0)
            }
        );
        let bd = &report.sets["INT"];
        assert_eq!(
            u["sets"]["INT"].total,
            Some(bd.details_total + bd.fringe_total)
        );
        assert_eq!(u["currencies"]["CAD"].lines, 1);
        assert!(u["currencies"]["CAD"].total.is_some());
        // The starter's PAYROLL fringe is on some lines, with its dollars.
        let p = u["fringes"]["PAYROLL"];
        assert_eq!(p.lines as u32, report.fringes["PAYROLL"].uses);
        assert_eq!(p.total, Some(report.fringes["PAYROLL"].total));
        // Globals and units have counts, no dollars.
        assert!(u["globals"].values().all(|g| g.total.is_none()));
        assert!(u["globals"]["SHOOT_DAYS"].other >= 1, "SHOOT_WEEKS uses it");
        assert!(u["units"].values().any(|x| x.lines > 0));
        // Without totals, counts still come.
        let u = usage_of(b, None);
        assert_eq!(u["sets"]["INT"].total, None);
        assert_eq!(u["sets"]["INT"].lines, 2);
    }

    #[test]
    fn deleting_in_use_items_from_lines() {
        let mut s = Session::new();
        s.edit("Test edit", |b| {
            edit::set_currency(
                b,
                "CAD",
                &edit::CurrencyInput {
                    description: String::new(),
                    symbol: String::new(),
                    rate: "0.5".into(),
                    symbol_after: None,
                    separators: None,
                },
            )?;
            rows(b)[0].currency = Some("CAD".into());
            Ok(())
        })
        .unwrap();
        // A currency in use is refused, unless its lines move to the base.
        assert!(s
            .edit("Test edit", |b| edit::delete_currency(b, "CAD"))
            .is_err());
        s.edit("Test edit", |b| edit::delete_currency_with(b, "CAD", true))
            .unwrap();
        assert!(!s.budget().currencies.contains_key("CAD"));
        assert!(edit::details(s.budget()).all(|d| d.currency.is_none()));
        // A unit: kept on lines, or cleared from them.
        let used = usage(s.budget(), SetupKind::Unit)["Week"].lines;
        assert!(used > 0);
        let mut kept = s.budget().clone();
        edit::delete_unit(&mut kept, "Week").unwrap();
        assert_eq!(
            edit::details(&kept).filter(|d| d.unit == "Week").count(),
            edit::details(s.budget())
                .filter(|d| d.unit == "Week")
                .count()
        );
        s.edit("Test edit", |b| edit::delete_unit_with(b, "Week", true))
            .unwrap();
        assert!(edit::details(s.budget()).all(|d| d.unit != "Week"
            && d.unit2 != "Week"
            && d.unit3 != "Week"
            && d.unit4 != "Week"));
        assert!(s.undo());
        assert!(s.budget().units.contains_key("Week"));
    }

    #[test]
    fn order_is_saved_moved_and_follows_renames_and_deletes() {
        let mut s = Session::new();
        s.edit("Test edit", |b| {
            for n in ["A", "B", "C"] {
                b.sets.insert(n.into(), SetDef::default());
            }
            Ok(())
        })
        .unwrap();
        let names = |s: &Session| method_core::order::ordered(s.budget(), SetupKind::Set);
        s.edit("Test edit", |b| {
            method_core::order::set_order(b, SetupKind::Set, &["C".into(), "A".into()]);
            Ok(())
        })
        .unwrap();
        assert_eq!(names(&s), ["C", "A", "B"]);
        // Saved in the file, and read back.
        let back = method_format::from_str(&s.file_text()).unwrap();
        assert_eq!(
            method_core::order::ordered(&back, SetupKind::Set),
            ["C", "A", "B"]
        );
        s.edit("Test edit", |b| move_item(b, SetupKind::Set, "B", -1))
            .unwrap();
        assert_eq!(names(&s), ["C", "B", "A"]);
        assert!(s
            .edit("Test edit", |b| move_item(b, SetupKind::Set, "C", -1))
            .is_err());
        assert!(s
            .edit("Test edit", |b| move_item(b, SetupKind::Set, "nope", 1))
            .is_err());
        s.edit("Test edit", |b| {
            method_core::rename(b, SetupKind::Set, "B", "Bee").map_err(|e| e.to_string())
        })
        .unwrap();
        assert_eq!(names(&s), ["C", "Bee", "A"]);
        s.edit("Test edit", |b| edit::delete_set(b, "C")).unwrap();
        assert_eq!(names(&s), ["Bee", "A"]);
        assert_eq!(s.budget().setup_order.sets, ["Bee", "A"]);
        // Back to name order: nothing kept, nothing written.
        s.edit("Test edit", |b| move_item(b, SetupKind::Set, "Bee", 1))
            .unwrap();
        assert!(s.budget().setup_order.is_empty());
        assert!(!s.file_text().contains("setup_order"));
        assert_eq!(parse_kind("fringes").unwrap(), SetupKind::Fringe);
        assert_eq!(parse_kind("group").unwrap(), SetupKind::Group);
        assert!(parse_kind("nope").is_err());
    }

    #[test]
    fn short_codes_are_unique_and_kept_by_edits() {
        let mut s = Session::new();
        let fringe = |code: Option<&str>| FringeInput {
            kind: "percent".into(),
            description: String::new(),
            rate: "7.65".into(),
            cutoff: None,
            unit: None,
            basis: None,
            max_hours_per_day: None,
            short_code: code.map(String::from),
        };
        s.edit("Test edit", |b| {
            edit::set_fringe(b, "FICA tax", &fringe(Some(" FICA ")))
        })
        .unwrap();
        assert_eq!(
            s.budget().fringes["FICA tax"].short_code.as_deref(),
            Some("FICA")
        );
        // Edited without a code: kept.
        s.edit("Test edit", |b| {
            edit::set_fringe(b, "FICA tax", &fringe(None))
        })
        .unwrap();
        assert_eq!(
            s.budget().fringes["FICA tax"].short_code.as_deref(),
            Some("FICA")
        );
        // Taken (any case), the name of another, a comma, too long: refused.
        assert!(s
            .edit("Test edit", |b| edit::set_fringe(
                b,
                "Other",
                &fringe(Some("fica"))
            ))
            .is_err());
        assert!(s
            .edit("Test edit", |b| edit::set_short_code(
                b,
                SetupKind::Fringe,
                "FICA tax",
                "PAYROLL"
            ))
            .is_err());
        assert!(s
            .edit("Test edit", |b| edit::set_short_code(
                b,
                SetupKind::Fringe,
                "FICA tax",
                "A,B"
            ))
            .is_err());
        let long = "X".repeat(edit::MAX_SHORT_CODE_CHARS + 1);
        assert!(s
            .edit("Test edit", |b| edit::set_short_code(
                b,
                SetupKind::Fringe,
                "FICA tax",
                &long
            ))
            .is_err());
        assert!(s
            .edit("Test edit", |b| edit::set_short_code(
                b,
                SetupKind::Set,
                "x",
                "y"
            ))
            .is_err());
        // Its own code again is fine; blank clears.
        s.edit("Test edit", |b| {
            edit::set_short_code(b, SetupKind::Fringe, "FICA tax", "FICA")
        })
        .unwrap();
        s.edit("Test edit", |b| {
            edit::set_short_code(b, SetupKind::Fringe, "FICA tax", " ")
        })
        .unwrap();
        assert_eq!(s.budget().fringes["FICA tax"].short_code, None);
        // Groups too; a rename keeps the code and the lines' references.
        s.edit("Test edit", |b| {
            edit::set_group(
                b,
                "Hold",
                &GroupInput {
                    description: String::new(),
                    in_budget_total: false,
                    short_code: Some("NQ".into()),
                },
            )
        })
        .unwrap();
        s.edit("Test edit", |b| {
            rows(b)[0].groups.insert("Hold".into());
            method_core::rename(b, SetupKind::Group, "Hold", "On hold")
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .unwrap();
        assert_eq!(
            s.budget().groups["On hold"].short_code.as_deref(),
            Some("NQ")
        );
        assert!(edit::details(s.budget()).any(|d| d.groups.contains("On hold")));
    }

    /// The new commands are registered, listed and allowed in both windows.
    #[test]
    fn setup_list_commands_are_wired() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN: &str = include_str!("../capabilities/main.json");
        const TOOLS: &str = include_str!("../capabilities/tools.json");
        for (module, c) in [
            ("setup_lists", "setup_usage"),
            ("setup_lists", "set_setup_order"),
            ("setup_lists", "move_setup_item"),
            ("setup_lists", "set_short_code"),
            ("notes", "add_note"),
            ("notes", "update_note"),
            ("notes", "delete_note"),
            ("notes", "set_note_done"),
        ] {
            assert!(
                APP_RS.contains(&format!("crate::{module}::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN.contains(&allow), "{c} not allowed for the main window");
            assert!(
                TOOLS.contains(&allow),
                "{c} not allowed for the tools window"
            );
        }
    }
}
