//! Display settings (roadmap 2.10 and 2.18). None of it changes a stored
//! value or a total: the engine stays exact and only the screen rounds
//! (decisions.md, entries 1 and 2).
//!
//! - `set_global_info`: a global's description, unit (from the units
//!   table), display decimals, "show by name" switch and folder.
//! - `set_display_prefs`: the budget's screen settings saved with the file:
//!   rate decimals with a cutoff band, whether a rate's first global's
//!   decimals win, and whether the topsheet shows its summary rows and the
//!   net total.
//! - [`cell_values`]: the evaluated value of each formula cell in the account
//!   on screen, which the view sends so the grid can show values (and the
//!   hidden-factor marker can tell when a hidden X or 4X isn't 1). Plain
//!   numbers are left out: the grid shows them as typed.
//!
//! Per-user choices (totals decimals, hiding zero accounts, grid zoom, the
//! formula view in a selection) are kept by the web view, not here
//! (ui/js/display.js).

use std::collections::BTreeMap;

use fwb_core::expr::Expr;
use fwb_core::{Budget, Dec, Detail, DisplayPrefs, GlobalInfo, RowKind, MAX_DISPLAY_DECIMALS};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{edit_budget, AppState, DocId};
use crate::edit::EditResult;
use crate::view::View;

/// Longest global description, in characters.
pub const MAX_GLOBAL_DESCRIPTION_CHARS: usize = 500;
/// Longest folder name, in characters.
pub const MAX_FOLDER_CHARS: usize = 100;

/// A global's description, unit, decimals, show-by-name and folder, as the
/// UI sends them (every field; missing ones are empty).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalInfoInput {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub decimals: Option<u8>,
    #[serde(default)]
    pub show_by_name: bool,
    #[serde(default)]
    pub folder: String,
}

fn check_places(what: &str, n: Option<u8>) -> EditResult {
    match n {
        Some(n) if n > MAX_DISPLAY_DECIMALS => Err(format!(
            "{what} must be 0 to {MAX_DISPLAY_DECIMALS} decimal places"
        )),
        _ => Ok(()),
    }
}

/// Sets what global `name` says about itself. Text is trimmed; a unit must
/// be one of the budget's units (or empty). All-default info removes the
/// entry. Never changes a value.
pub fn apply_global_info(b: &mut Budget, name: &str, input: &GlobalInfoInput) -> EditResult {
    if !b.globals.contains_key(name) {
        return Err(format!("global {name} no longer exists"));
    }
    let description = input.description.trim().to_string();
    if description.chars().count() > MAX_GLOBAL_DESCRIPTION_CHARS {
        return Err(format!(
            "a global's description can be at most {MAX_GLOBAL_DESCRIPTION_CHARS} characters"
        ));
    }
    let unit = input.unit.trim().to_string();
    if !unit.is_empty() && !b.units.contains_key(&unit) {
        return Err(format!("{unit} is not a unit in this budget"));
    }
    check_places("a global's decimals", input.decimals)?;
    let folder = input.folder.trim().to_string();
    if folder.chars().count() > MAX_FOLDER_CHARS {
        return Err(format!(
            "a folder name can be at most {MAX_FOLDER_CHARS} characters"
        ));
    }
    if folder.contains(['\n', '\r']) {
        return Err("a folder name has to be one line".into());
    }
    let info = GlobalInfo {
        description,
        unit,
        decimals: input.decimals,
        show_by_name: input.show_by_name,
        folder,
    };
    if info.is_default() {
        b.global_info.remove(name);
    } else {
        b.global_info.insert(name.to_string(), info);
    }
    Ok(())
}

/// The budget's display settings, as the UI sends them (every field).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayPrefsInput {
    /// A positive number, or blank (or missing) for no cutoff.
    #[serde(default)]
    pub rate_cutoff: Option<String>,
    #[serde(default)]
    pub rate_decimals_below: Option<u8>,
    #[serde(default)]
    pub rate_decimals_above: Option<u8>,
    pub rate_global_decimals: bool,
    pub summary_rows: bool,
    pub net_total: bool,
}

/// Replaces the budget's display settings. Never changes a value.
pub fn apply_display_prefs(b: &mut Budget, input: &DisplayPrefsInput) -> EditResult {
    let rate_cutoff = match input.rate_cutoff.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(t) => {
            let d: Dec = t
                .parse()
                .map_err(|_| format!("the rate cutoff `{t}` isn't a number"))?;
            if d <= Dec::ZERO {
                return Err("the rate cutoff has to be more than 0".into());
            }
            Some(d)
        }
    };
    check_places("rate decimals below the cutoff", input.rate_decimals_below)?;
    check_places("rate decimals", input.rate_decimals_above)?;
    b.display = DisplayPrefs {
        rate_cutoff,
        rate_decimals_below: input.rate_decimals_below,
        rate_decimals_above: input.rate_decimals_above,
        rate_global_decimals: input.rate_global_decimals,
        summary_rows: input.summary_rows,
        net_total: input.net_total,
    };
    Ok(())
}

/// The evaluated values of one row's formula cells, as decimal strings in
/// canonical form. A field is left out when it is blank, a plain number
/// (shown as typed), or can't be evaluated (the cell has a problem).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CellValues {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x4: Option<String>,
}

impl CellValues {
    fn is_empty(&self) -> bool {
        *self == CellValues::default()
    }
}

/// One formula's value, unless it is blank or a plain number.
fn formula_value(src: &str, globals: &BTreeMap<String, Dec>) -> Option<String> {
    let t = src.trim();
    if t.is_empty() || t.parse::<Dec>().is_ok() {
        return None;
    }
    let lookup = |n: &str| globals.get(n).copied();
    let v = match fwb_core::expr::quick_value(t, &lookup) {
        Some(v) => v,
        None => Expr::parse(t).ok()?.eval(&lookup).ok()?,
    };
    Some(v.to_string())
}

/// The values of every row's formula cells (`None` for a row with none), in
/// row order. `globals` are the evaluated globals (the report's).
/// Adjustment rows' rates are locked to the lines above, so they are left
/// out; subtotal and separator rows have no cells to evaluate.
pub fn cell_values(details: &[Detail], globals: &BTreeMap<String, Dec>) -> Vec<Option<CellValues>> {
    details
        .iter()
        .map(|d| {
            if !d.kind.is_costed() {
                return None;
            }
            let v = CellValues {
                amount: formula_value(&d.amount.0, globals),
                x: formula_value(&d.x.0, globals),
                rate: if d.kind == RowKind::Line {
                    formula_value(&d.rate.0, globals)
                } else {
                    None
                },
                x4: formula_value(&d.x4.0, globals),
            };
            (!v.is_empty()).then_some(v)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/*.json)

/// `{name, info: {description, unit, decimals, show_by_name, folder}}`.
#[tauri::command]
pub fn set_global_info<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    info: GlobalInfoInput,
) -> Result<View, String> {
    edit_budget(
        &window,
        &state,
        doc,
        format!("Edit the details of global {}", name.trim()),
        |b| apply_global_info(b, &name, &info),
    )
}

/// `{prefs: {rate_cutoff, rate_decimals_below, rate_decimals_above,
/// rate_global_decimals, summary_rows, net_total}}`.
#[tauri::command]
pub fn set_display_prefs<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    prefs: DisplayPrefsInput,
) -> Result<View, String> {
    edit_budget(&window, &state, doc, "Change numbers on screen", |b| {
        apply_display_prefs(b, &prefs)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

    fn budget() -> Budget {
        let mut b = Budget::new("T", "USD");
        b.globals.insert("DAYS".into(), "5".into());
        b.globals.insert("WEEKS".into(), "DAYS / 5".into());
        b
    }

    fn info(unit: &str, decimals: Option<u8>, show: bool) -> GlobalInfoInput {
        GlobalInfoInput {
            description: "  Shoot days  ".into(),
            unit: unit.into(),
            decimals,
            show_by_name: show,
            folder: " Schedule ".into(),
        }
    }

    #[test]
    fn global_info_is_set_trimmed_and_checked() {
        let mut b = budget();
        apply_global_info(&mut b, "DAYS", &info("Day", Some(1), true)).unwrap();
        let g = &b.global_info["DAYS"];
        assert_eq!(g.description, "Shoot days");
        assert_eq!(g.folder, "Schedule");
        assert_eq!(
            (g.unit.as_str(), g.decimals, g.show_by_name),
            ("Day", Some(1), true)
        );

        assert!(apply_global_info(&mut b, "NOPE", &info("", None, false)).is_err());
        let e = apply_global_info(&mut b, "DAYS", &info("Fortnight", None, false)).unwrap_err();
        assert!(e.contains("not a unit"), "{e}");
        assert!(apply_global_info(&mut b, "DAYS", &info("", Some(9), false)).is_err());
        let mut long = info("", None, false);
        long.folder = "x".repeat(MAX_FOLDER_CHARS + 1);
        assert!(apply_global_info(&mut b, "DAYS", &long).is_err());
        // Nothing changed by the refused ones.
        assert_eq!(b.global_info["DAYS"].unit, "Day");

        // All defaults removes the entry.
        apply_global_info(&mut b, "DAYS", &GlobalInfoInput::default()).unwrap();
        assert!(b.global_info.is_empty());
    }

    #[test]
    fn global_info_never_changes_a_total() {
        let mut s = Session::with_budget(fwb_core::sample::budget(&Default::default()), None);
        let before = s.totals().1.report.clone();
        let name = s.budget().globals.keys().next().unwrap().clone();
        let unit = s.budget().units.keys().next().unwrap().clone();
        s.edit("Test edit", |b| {
            apply_global_info(b, &name, &info(&unit, Some(3), true))
        })
        .unwrap();
        assert_eq!(s.totals().1.report, before);
        assert!(s.can_undo());
    }

    #[test]
    fn info_follows_a_rename_and_goes_with_a_delete() {
        let mut b = budget();
        apply_global_info(&mut b, "DAYS", &info("Day", Some(0), false)).unwrap();
        fwb_core::rename(&mut b, fwb_core::SetupKind::Global, "DAYS", "SHOOT").unwrap();
        assert!(b.global_info.contains_key("SHOOT"));
        assert!(!b.global_info.contains_key("DAYS"));
        // Renaming the unit carries the global's unit along, and back again.
        let before = b.clone();
        fwb_core::rename(&mut b, fwb_core::SetupKind::Unit, "Day", "Shoot day").unwrap();
        assert_eq!(b.global_info["SHOOT"].unit, "Shoot day");
        fwb_core::rename(&mut b, fwb_core::SetupKind::Unit, "Shoot day", "Day").unwrap();
        assert_eq!(b, before);
        crate::edit::delete_global(&mut b, "SHOOT").unwrap();
        assert!(b.global_info.is_empty());
    }

    #[test]
    fn a_unit_named_only_by_a_global_blocks_renaming_onto_it() {
        let mut b = budget();
        apply_global_info(&mut b, "DAYS", &info("Day", None, false)).unwrap();
        // Delete the unit the global names: the name stays on the global.
        b.units.remove("Day");
        let e = fwb_core::rename(&mut b, fwb_core::SetupKind::Unit, "Week", "Day").unwrap_err();
        assert!(
            matches!(e, fwb_core::RenameError::InUse { places: 1, .. }),
            "{e:?}"
        );
    }

    fn prefs(cutoff: Option<&str>, below: Option<u8>, above: Option<u8>) -> DisplayPrefsInput {
        DisplayPrefsInput {
            rate_cutoff: cutoff.map(String::from),
            rate_decimals_below: below,
            rate_decimals_above: above,
            rate_global_decimals: false,
            summary_rows: false,
            net_total: true,
        }
    }

    #[test]
    fn display_prefs_are_checked() {
        let mut b = budget();
        apply_display_prefs(&mut b, &prefs(Some(" 10.50 "), None, Some(2))).unwrap();
        assert_eq!(b.display.rate_cutoff, Some("10.5".parse().unwrap()));
        assert_eq!(b.display.rate_decimals_above, Some(2));
        assert!(!b.display.summary_rows && !b.display.rate_global_decimals);
        apply_display_prefs(&mut b, &prefs(Some(""), None, None)).unwrap();
        assert_eq!(b.display.rate_cutoff, None);
        for bad in [
            prefs(Some("abc"), None, None),
            prefs(Some("0"), None, None),
            prefs(Some("-1"), None, None),
            prefs(None, Some(9), None),
            prefs(None, None, Some(200)),
        ] {
            let keep = b.display.clone();
            assert!(apply_display_prefs(&mut b, &bad).is_err());
            assert_eq!(b.display, keep);
        }
    }

    #[test]
    fn display_prefs_never_change_a_total() {
        let mut s = Session::with_budget(fwb_core::sample::budget(&Default::default()), None);
        let before = s.totals().1.report.clone();
        s.edit("Test edit", |b| {
            apply_display_prefs(b, &prefs(Some("100"), Some(4), Some(0)))
        })
        .unwrap();
        assert_eq!(s.totals().1.report, before);
    }

    #[test]
    fn formula_cells_get_values_plain_numbers_do_not() {
        let globals = BTreeMap::from([("DAYS".to_string(), "5".parse::<Dec>().unwrap())]);
        let mut line = Detail::line(1, "a", "DAYS * 2", "Day", "450");
        line.x = "1.5".into();
        line.x4 = "DAYS / 2".into();
        let mut bad = Detail::line(2, "b", "NOPE", "", "1 +");
        bad.x = " ".into();
        let adj = Detail::adjustment(3, "c", "DAYS / 100");
        let sub = Detail::subtotal(4, "s");
        let v = cell_values(&[line, bad, adj, sub], &globals);
        assert_eq!(
            v[0],
            Some(CellValues {
                amount: Some("10".into()),
                x: None,
                rate: None,
                x4: Some("2.5".into()),
            })
        );
        assert_eq!(v[1], None, "errors are left to the cell's problem");
        assert_eq!(v[2].as_ref().unwrap().amount.as_deref(), Some("0.05"));
        assert_eq!(v[3], None);
        // Serialized without the empty fields.
        let json = serde_json::to_string(&v[0]).unwrap();
        assert_eq!(json, r#"{"amount":"10","x4":"2.5"}"#);
    }
}
