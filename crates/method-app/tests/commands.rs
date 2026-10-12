//! The app's command layer without a window: edit -> recalc -> view, undo and
//! redo, the dirty flag, setup tables, and saving and opening files.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use method_app::edit::{self, CurrencyInput, FringeInput, GroupInput, UnitInput};
use method_app::view::Focus;
use method_app::Session;
use method_core::{Budget, Inclusion, RowId};
use serde_json::Value;

/// Everything, every row (the full view the printed report uses).
fn view(s: &mut Session) -> Value {
    serde_json::to_value(s.full_view()).unwrap()
}

fn ids_in(s: &Session, account: &str) -> Vec<RowId> {
    s.budget()
        .categories()
        .flat_map(|c| &c.accounts)
        .find(|a| a.code == account)
        .unwrap()
        .details
        .iter()
        .map(|d| d.id)
        .collect()
}

/// An account's report entry from a view.
fn account<'a>(v: &'a Value, code: &str) -> &'a Value {
    v["report"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["accounts"].as_array().unwrap())
        .find(|a| a["code"] == code)
        .unwrap()
}

fn temp_dir() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "method-app-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn starter_view_has_budget_json_and_totals() {
    let mut s = Session::new();
    let v = view(&mut s);
    assert_eq!(v["budget"]["title"], "Untitled feature");
    assert_eq!(v["budget"]["topsheet"][0]["kind"], "category");
    assert!(v["report"]["grand_total"].as_i64().unwrap() > 0);
    assert_eq!(v["partial"], false);
    assert!(v["error"].is_null());
    assert_eq!(v["issues"].as_array().unwrap().len(), 0);
    assert_eq!(v["file"]["name"], "Untitled.mthd");
    assert_eq!(v["file"]["dirty"], false);
    assert_eq!(v["can_undo"], false);
}

#[test]
fn editing_a_cell_recalcs_and_undo_redo_restore() {
    let mut s = Session::new();
    let before = view(&mut s);
    let id = ids_in(&s, "50-01")[0];
    assert_eq!(account(&before, "50-01")["total"], 1_500_000);

    s.set_detail_field(id, "rate", "20000").unwrap();
    let v = view(&mut s);
    assert_eq!(account(&v, "50-01")["total"], 2_000_000);
    // Contingency is 10%, so the grand total moves by 5,000 + 500.
    assert_eq!(
        v["report"]["grand_total"].as_i64().unwrap()
            - before["report"]["grand_total"].as_i64().unwrap(),
        550_000
    );
    assert_eq!(v["file"]["dirty"], true);
    assert_eq!(v["can_undo"], true);

    assert!(s.undo());
    let v = view(&mut s);
    assert_eq!(account(&v, "50-01")["total"], 1_500_000);
    assert_eq!(v["file"]["dirty"], false, "back at the saved state");
    assert_eq!(v["can_redo"], true);

    assert!(s.redo());
    assert_eq!(account(&view(&mut s), "50-01")["total"], 2_000_000);
    assert!(s.is_dirty());

    // A new edit clears redo.
    s.undo();
    s.set_detail_field(id, "amount", "2").unwrap();
    assert!(!s.can_redo());
    assert_eq!(account(&view(&mut s), "50-01")["total"], 3_000_000);
}

#[test]
fn no_op_edits_leave_history_alone() {
    let mut s = Session::new();
    let id = ids_in(&s, "50-01")[0];
    s.set_detail_field(id, "rate", "15000").unwrap(); // already 15000
    assert!(!s.can_undo() && !s.is_dirty());
}

#[test]
fn rejected_edits_change_nothing() {
    let mut s = Session::new();
    assert!(s.set_detail_field(999_999, "rate", "1").is_err());
    assert!(s.set_detail_field(1, "bogus", "1").is_err());
    assert!(s
        .edit("Test edit", |b| edit::set_global(b, "9.5lives", "1"))
        .is_err());
    assert!(s
        .edit("Test edit", |b| edit::add_account(
            b, "21", "21-01", "Dup", None
        ))
        .is_err());
    assert!(!s.can_undo() && !s.is_dirty());
}

#[test]
fn a_bad_formula_is_kept_flagged_and_the_rest_still_totals() {
    let mut s = Session::new();
    let good = view(&mut s);
    let id = ids_in(&s, "21-01")[0];
    s.set_detail_field(id, "amount", "SHOOT_WEKS + 1").unwrap();
    let v = view(&mut s);

    // The edit is kept exactly as typed.
    let detail = &v["budget"]["topsheet"][6]["accounts"][0]["details"][0];
    assert_eq!(detail["amount"], "SHOOT_WEKS + 1");
    // The engine's message, and the exact cell.
    assert_eq!(
        v["error"]["at"],
        "account 21-01 line 1 (Director of photography): amount"
    );
    assert_eq!(v["error"]["message"], "unknown global `SHOOT_WEKS`");
    assert_eq!(v["issues"][0]["target"]["kind"], "cell");
    assert_eq!(v["issues"][0]["target"]["row_id"], id);
    assert_eq!(v["issues"][0]["target"]["field"], "amount");
    // Totals still show, with that cell counted as blank.
    assert_eq!(v["partial"], true);
    let row = &account(&v, "21-01")["rows"][0];
    assert_eq!(row["total"], 0);
    assert_eq!(
        account(&v, "50-01")["total"],
        account(&good, "50-01")["total"]
    );

    // Fixing it clears the error.
    s.set_detail_field(id, "amount", "SHOOT_WEEKS + 1").unwrap();
    let v = view(&mut s);
    assert!(v["error"].is_null() && v["partial"] == false);
    assert_eq!(v["report"]["grand_total"], good["report"]["grand_total"]);
}

#[test]
fn unknown_fringe_on_a_row_is_flagged_on_that_cell() {
    let mut s = Session::new();
    let id = ids_in(&s, "22-01")[0];
    s.set_detail_field(id, "fringes", "PAYROLL, NOPE").unwrap();
    let v = view(&mut s);
    assert_eq!(v["issues"][0]["target"]["field"], "fringes");
    assert_eq!(v["issues"][0]["message"], "unknown fringe `NOPE`");
    // PAYROLL still applies in the partial totals.
    assert!(
        account(&v, "22-01")["rows"][0]["fringes"]["PAYROLL"]
            .as_i64()
            .unwrap()
            > 0
    );
}

#[test]
fn adding_and_deleting_rows() {
    let mut s = Session::new();
    let ids = ids_in(&s, "20-01");
    let line = s.add_detail("20-01", Some(ids[0]), "line").unwrap();
    let sub = s.add_detail("20-01", None, "subtotal").unwrap();
    let adj = s.add_detail("20-01", Some(sub), "adjustment").unwrap();
    assert_eq!(ids_in(&s, "20-01"), [ids[0], line, ids[1], sub, adj]);
    assert_eq!(
        (line, sub, adj),
        (14, 15, 16),
        "ids continue after the starter's 13"
    );

    // The lean view carries the account on screen in full.
    s.set_focus(Some(Focus {
        category: "20".into(),
        ti: 5,
        account: "20-01".into(),
        ai: 0,
    }));
    let v = serde_json::to_value(s.view_with(Some(adj))).unwrap();
    assert_eq!(v["created_row"], adj);
    assert_eq!(v["account"]["code"], "20-01");
    let details = &v["account"]["details"];
    assert_eq!(details[1]["description"], "");
    assert!(
        details[1].get("kind").is_none(),
        "lines omit the default kind"
    );
    assert_eq!(details[3]["kind"], "subtotal");
    assert_eq!(details[4]["kind"], "adjustment");

    // The adjustment is 10% of the lines above it.
    s.set_detail_field(adj, "amount", "0.1").unwrap();
    let v = view(&mut s);
    let acct = account(&v, "20-01");
    let lines: i64 = acct["rows"].as_array().unwrap()[..3]
        .iter()
        .map(|r| r["total"].as_i64().unwrap())
        .sum();
    assert_eq!(acct["rows"][3]["subtotal"], lines);
    assert_eq!(acct["rows"][4]["total"], lines / 10);
    // Subtotal rows only take a description; adjustments have no rate.
    assert!(s.set_detail_field(sub, "rate", "5").is_err());
    assert!(s.set_detail_field(adj, "rate", "5").is_err());
    s.set_detail_field(sub, "description", "Staff subtotal")
        .unwrap();

    s.edit("Test edit", |b| edit::delete_details(b, &[line, adj]))
        .unwrap();
    assert_eq!(ids_in(&s, "20-01"), [ids[0], ids[1], sub]);
    assert!(s.add_detail("20-01", Some(9999), "line").is_err());
    assert!(s.add_detail("20-01", None, "header").is_err());
}

#[test]
fn bulk_field_edits_are_one_undo_step() {
    let mut s = Session::new();
    let ids = ids_in(&s, "21-01");
    s.edit("Test edit", |b| {
        edit::set_group(
            b,
            "Second unit",
            &GroupInput {
                short_code: None,
                description: String::new(),
                in_budget_total: true,
            },
        )
    })
    .unwrap();
    s.edit("Test edit", |b| {
        edit::set_rows_field(b, &ids, edit::Field::Groups, "Second unit")
    })
    .unwrap();
    let v = view(&mut s);
    assert_eq!(
        v["report"]["groups"]["Second unit"]["active"]["details_total"],
        account(&v, "21-01")["details_total"]
    );
    s.undo();
    assert!(s
        .budget()
        .categories()
        .flat_map(|c| &c.accounts)
        .all(|a| a.details.iter().all(|d| d.groups.is_empty())));
}

#[test]
fn groups_out_of_the_total_grey_rows_and_drop_their_money() {
    let mut s = Session::new();
    let id = ids_in(&s, "21-01")[1];
    let g = |on| GroupInput {
        short_code: None,
        description: "Alternate camera".into(),
        in_budget_total: on,
    };
    s.edit("Test edit", |b| edit::set_group(b, "Alt", &g(true)))
        .unwrap();
    s.set_detail_field(id, "groups", "Alt").unwrap();
    let full = account(&view(&mut s), "21-01")["total"].as_i64().unwrap();
    s.edit("Test edit", |b| edit::set_group(b, "Alt", &g(false)))
        .unwrap();
    let v = view(&mut s);
    let row = &account(&v, "21-01")["rows"][1];
    assert_eq!(row["included"], false);
    assert_eq!(row["total"], 0);
    assert_eq!(
        account(&v, "21-01")["total"].as_i64().unwrap(),
        full - 3_000_000
    );
    // Deleting the group untags the row and brings the money back.
    s.edit("Test edit", |b| edit::delete_group(b, "Alt"))
        .unwrap();
    let v = view(&mut s);
    assert_eq!(account(&v, "21-01")["total"].as_i64().unwrap(), full);
    assert!(v["budget"]["topsheet"][6]["accounts"][0]["details"][1]
        .get("groups")
        .is_none());
}

#[test]
fn globals_set_evaluate_and_delete_by_freezing() {
    let mut s = Session::new();
    s.edit("Test edit", |b| edit::set_global(b, "SHOOT_DAYS", "30"))
        .unwrap();
    let v = view(&mut s);
    assert_eq!(v["report"]["globals"]["SHOOT_WEEKS"], "6");
    let before = v["report"]["grand_total"].clone();

    s.edit("Test edit", |b| edit::delete_global(b, "SHOOT_WEEKS"))
        .unwrap();
    let v = view(&mut s);
    assert!(v["budget"]["globals"].get("SHOOT_WEEKS").is_none());
    assert_eq!(
        v["report"]["grand_total"], before,
        "deleting froze the value"
    );
    let lead = &v["budget"]["topsheet"][3]["accounts"][0]["details"][0];
    assert_eq!(lead["amount"], "6 + 1");

    // A broken global is flagged but can still be deleted.
    s.edit("Test edit", |b| edit::set_global(b, "BROKEN", "1 +"))
        .unwrap();
    let v = view(&mut s);
    assert_eq!(v["issues"][0]["target"]["kind"], "global");
    assert_eq!(v["issues"][0]["target"]["name"], "BROKEN");
    assert_eq!(v["partial"], true);
    s.edit("Test edit", |b| edit::delete_global(b, "BROKEN"))
        .unwrap();
    assert!(view(&mut s)["error"].is_null());

    assert!(s
        .edit("Test edit", |b| edit::set_global(b, "X", "  "))
        .is_err());
    assert!(s
        .edit("Test edit", |b| edit::delete_global(b, "NOPE"))
        .is_err());
}

#[test]
fn fringes_units_and_currencies() {
    let mut s = Session::new();
    let pension = FringeInput {
        short_code: None,
        kind: "percent".into(),
        description: "Pension".into(),
        rate: "6".into(),
        cutoff: Some("100,000".into()),
        unit: None,
        basis: None,
        max_hours_per_day: None,
    };
    s.edit("Test edit", |b| edit::set_fringe(b, "PENSION", &pension))
        .unwrap();
    let director = ids_in(&s, "12-01")[0];
    s.set_detail_field(director, "fringes", "PAYROLL, PENSION")
        .unwrap();
    let v = view(&mut s);
    let row = &account(&v, "12-01")["rows"][0];
    assert_eq!(
        row["fringes"]["PENSION"], 600_000,
        "6% of the 100,000 cutoff"
    );
    assert_eq!(v["report"]["fringes"]["PENSION"]["uses"], 1);

    let meal = FringeInput {
        short_code: None,
        kind: "flat".into(),
        description: String::new(),
        rate: "50".into(),
        cutoff: None,
        unit: Some("Day".into()),
        basis: None,
        max_hours_per_day: None,
    };
    s.edit("Test edit", |b| edit::set_fringe(b, "MEAL", &meal))
        .unwrap();
    assert!(s
        .edit("Test edit", |b| edit::set_fringe(b, "A,B", &meal))
        .unwrap_err()
        .contains("commas"));
    assert!(s
        .edit("Test edit", |b| edit::set_fringe(
            b,
            "BAD",
            &FringeInput {
                kind: "weird".into(),
                ..meal.clone()
            }
        ))
        .is_err());

    // How a flat fringe counts: its basis and most hours a day, written to
    // the file JSON the views read.
    let pen_hr = FringeInput {
        unit: Some("Hour".into()),
        basis: Some("amount_counts".into()),
        max_hours_per_day: Some("12".into()),
        ..meal.clone()
    };
    s.edit("Test edit", |b| edit::set_fringe(b, "PENHR", &pen_hr))
        .unwrap();
    let f = &view(&mut s)["budget"]["fringes"]["PENHR"];
    assert_eq!(
        (f["basis"].clone(), f["max_hours_per_day"].clone()),
        (serde_json::json!("amount_counts"), serde_json::json!("12"))
    );
    for (field, bad, why) in [
        ("basis", "hours", "unknown flat fringe basis"),
        ("max", "0", "more than 0"),
        ("max", "lots", "max hours per day"),
    ] {
        let mut input = pen_hr.clone();
        if field == "basis" {
            input.basis = Some(bad.into());
        } else {
            input.max_hours_per_day = Some(bad.into());
        }
        let e = s
            .edit("Test edit", |b| edit::set_fringe(b, "PENHR", &input))
            .unwrap_err();
        assert!(e.contains(why), "{e}");
    }
    // A percent fringe's cutoff is kept when it turns flat (never used),
    // and is back when it turns percent again.
    let kept = FringeInput {
        kind: "flat".into(),
        ..pension.clone()
    };
    s.edit("Test edit", |b| edit::set_fringe(b, "PENSION", &kept))
        .unwrap();
    assert_eq!(
        view(&mut s)["budget"]["fringes"]["PENSION"]["cutoff"],
        10_000_000
    );
    s.edit("Test edit", |b| edit::set_fringe(b, "PENSION", &pension))
        .unwrap();
    assert_eq!(
        view(&mut s)["budget"]["fringes"]["PENSION"]["kind"],
        "percent"
    );
    s.edit("Test edit", |b| edit::delete_fringe(b, "PENHR"))
        .unwrap();

    // Deleting a fringe takes it off every row.
    s.edit("Test edit", |b| edit::delete_fringe(b, "PENSION"))
        .unwrap();
    assert!(view(&mut s)["error"].is_null());
    let d = s
        .budget()
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == director)
        .unwrap();
    assert_eq!(d.fringes.len(), 1);

    let unit = UnitInput {
        plural: "Shifts".into(),
        description: String::new(),
        hours: Some("12".into()),
    };
    s.edit("Test edit", |b| edit::set_unit(b, "Shift", &unit))
        .unwrap();
    assert_eq!(view(&mut s)["budget"]["units"]["Shift"]["hours"], "12");
    assert!(s
        .edit("Test edit", |b| edit::set_unit(
            b,
            "Bad",
            &UnitInput {
                hours: Some("-1".into()),
                ..unit.clone()
            }
        ))
        .is_err());
    s.edit("Test edit", |b| edit::delete_unit(b, "Shift"))
        .unwrap();

    let eur = CurrencyInput {
        description: "Euro".into(),
        symbol: "€".into(),
        rate: "1.1".into(),
    };
    s.edit("Test edit", |b| edit::set_currency(b, "EUR", &eur))
        .unwrap();
    let mix = ids_in(&s, "41-01")[0];
    s.set_detail_field(mix, "currency", "EUR").unwrap();
    let v = view(&mut s);
    let row = &account(&v, "41-01")["rows"][0];
    assert_eq!(row["currency"], "EUR");
    assert_eq!(row["local_total"], 2_000_000);
    assert_eq!(row["total"], 2_200_000);
    assert!(s
        .edit("Test edit", |b| edit::delete_currency(b, "EUR"))
        .unwrap_err()
        .contains("1 line uses EUR"));
    assert!(s
        .edit("Test edit", |b| edit::set_currency(b, "USD", &eur))
        .unwrap_err()
        .contains("base currency"));
    assert!(s
        .edit("Test edit", |b| edit::set_currency(
            b,
            "JPY",
            &CurrencyInput {
                rate: "0".into(),
                ..eur.clone()
            }
        ))
        .is_err());
    s.set_detail_field(mix, "currency", "").unwrap();
    s.edit("Test edit", |b| edit::delete_currency(b, "EUR"))
        .unwrap();
}

#[test]
fn categories_accounts_and_production_totals() {
    let mut s = Session::new();
    // Codes are normalized by the budget's coding pattern.
    s.edit("Test edit", |b| edit::add_category(b, "6", "Other", None))
        .unwrap();
    s.edit("Test edit", |b| {
        edit::add_account(b, "06", "6-1", "Publicity", None)
    })
    .unwrap();
    let v = view(&mut s);
    let last = v["budget"]["topsheet"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["code"], "06");
    assert_eq!(last["accounts"][0]["code"], "06-01");
    assert!(s
        .edit("Test edit", |b| edit::add_category(b, "06", "Again", None))
        .unwrap_err()
        .contains("already exists"));

    s.add_detail("06-01", None, "line").unwrap();
    let id = ids_in(&s, "06-01")[0];
    s.set_detail_field(id, "amount", "1").unwrap();
    s.set_detail_field(id, "rate", "2500").unwrap();
    let v = view(&mut s);
    // After the last production total, so it counts in neither ATL nor BTL
    // (decisions.md, entry 56), only in the grand total.
    let atl_btl = v["report"]["atl_btl"].as_i64().unwrap();
    let grand = v["report"]["grand_total"].as_i64().unwrap();

    s.edit("Test edit", |b| {
        edit::add_production_total(b, "Total publicity", None)
    })
    .unwrap();
    let v = view(&mut s);
    assert_eq!(
        v["report"]["levels"].as_array().unwrap().last().unwrap()["total"],
        250_000
    );
    // A production total after it brings it below the line.
    assert_eq!(v["report"]["atl_btl"].as_i64().unwrap(), atl_btl + 250_000);
    assert_eq!(v["report"]["grand_total"].as_i64().unwrap(), grand);

    // Renumber and rename keep the accounts.
    let idx = v["budget"]["topsheet"].as_array().unwrap().len() - 2;
    s.edit("Test edit", |b| {
        edit::update_topsheet_row(b, idx, Some("60"), "Publicity")
    })
    .unwrap();
    s.edit("Test edit", |b| {
        edit::update_account(b, "06-01", "60-01", "Unit publicist")
    })
    .unwrap();
    assert_eq!(ids_in(&s, "60-01"), [id]);
    assert!(s
        .edit("Test edit", |b| edit::update_account(
            b, "60-01", "50-01", "Clash"
        ))
        .is_err());

    s.edit("Test edit", |b| edit::delete_account(b, "60-01"))
        .unwrap();
    s.edit("Test edit", |b| edit::delete_topsheet_row(b, idx + 1))
        .unwrap();
    s.edit("Test edit", |b| edit::delete_topsheet_row(b, idx))
        .unwrap();
    assert_eq!(s.budget(), &method_core::starter::budget());
}

#[test]
fn selection_totals_sum_the_chosen_rows() {
    let mut s = Session::new();
    let ids = ids_in(&s, "13-01");
    let sel = serde_json::to_value(s.selection(&ids)).unwrap();
    let v = view(&mut s);
    let acct = account(&v, "13-01");
    assert_eq!(sel["lines"]["details_total"], acct["details_total"]);
    assert_eq!(sel["lines"]["fringe_total"], acct["fringe_total"]);
    assert_eq!(sel["fringes"]["PAYROLL"], acct["fringe_total"]);
    let none = serde_json::to_value(s.selection(&[])).unwrap();
    assert_eq!(none["lines"]["details_total"], 0);
}

#[test]
fn save_open_round_trip_and_dirty_flag() {
    let dir = temp_dir();
    let path = dir.join("film.mthd");
    let mut s = Session::new();
    s.edit("Test edit", |b| edit::set_title(b, "Round Trip"))
        .unwrap();
    assert!(s.is_dirty());
    s.save_to(&path).unwrap();
    assert!(!s.is_dirty());
    assert_eq!(s.display_name(), "film.mthd");
    // No temporary files left behind.
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);

    // Undo after saving makes it dirty again; redo makes it clean.
    s.undo();
    assert!(s.is_dirty());
    s.redo();
    assert!(!s.is_dirty());

    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text, method_format::to_string(s.budget()));
    let mut opened = Session::open(&path).unwrap();
    assert_eq!(opened.budget(), s.budget());
    assert_eq!(opened.path(), Some(path.as_path()));
    assert!(!opened.is_dirty() && !opened.can_undo());
    assert_eq!(view(&mut opened)["file"]["name"], "film.mthd");

    std::fs::write(dir.join("bad.mthd"), "{ \"format\": \"something-else\" }").unwrap();
    let err = Session::open(&dir.join("bad.mthd")).unwrap_err();
    assert!(err.contains("not a Method Budget file"), "{err}");
    assert!(Session::open(&dir.join("missing.mthd")).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_file_with_unfixable_errors_still_opens_and_shows_the_problem() {
    let dir = temp_dir();
    let path = dir.join("dupe.mthd");
    let mut b: Budget = method_core::starter::budget();
    // Two rows with the same id: the engine refuses to total this budget.
    let first = b.categories().next().unwrap().accounts[0].details[0].id;
    b.categories_mut().nth(1).unwrap().accounts[0].details[0].id = first;
    std::fs::write(&path, method_format::to_string(&b)).unwrap();

    let mut s = Session::open(&path).unwrap();
    let v = view(&mut s);
    assert!(v["report"].is_null());
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("used twice"));
    assert!(v["budget"]["topsheet"].is_array());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn undo_history_is_capped() {
    let mut s = Session::new();
    let id = ids_in(&s, "50-01")[0];
    for i in 0..(method_app::session::UNDO_LIMIT + 20) {
        s.set_detail_field(id, "amount", &(i + 2).to_string())
            .unwrap();
    }
    let mut steps = 0;
    while s.undo() {
        steps += 1;
    }
    assert_eq!(steps, method_app::session::UNDO_LIMIT);
    // The oldest snapshots were dropped, so this isn't the starter any more,
    // and it isn't the saved state either.
    assert!(s.is_dirty());
}

// ---------------------------------------------------------------------------
// Topsheet setup: charges, credits, sets, locations, budget info, settings and
// the baseline. Inputs are built from the JSON the UI sends.

fn input<T: serde::de::DeserializeOwned>(v: Value) -> T {
    serde_json::from_value(v).unwrap()
}

/// Asserts an edit is refused and changes nothing: same budget, same history,
/// same dirty flag.
fn refused<T: std::fmt::Debug>(
    s: &mut Session,
    f: impl FnOnce(&mut Budget) -> edit::EditResult<T>,
) -> String {
    let before = s.budget().clone();
    let (undo, dirty) = (s.can_undo(), s.is_dirty());
    let e = s.edit("Test edit", f).unwrap_err();
    assert_eq!(
        s.budget(),
        &before,
        "a refused edit changed the budget: {e}"
    );
    assert_eq!((s.can_undo(), s.is_dirty()), (undo, dirty), "{e}");
    e
}

/// Applies an edit and checks it is exactly one undo step.
fn one_step<T: std::fmt::Debug>(
    s: &mut Session,
    f: impl FnOnce(&mut Budget) -> edit::EditResult<T>,
) -> T {
    let before = s.budget().clone();
    let out = s.edit("Test edit", f).unwrap();
    assert_ne!(s.budget(), &before, "the edit changed nothing");
    let after = s.budget().clone();
    assert!(s.undo());
    assert_eq!(s.budget(), &before, "one undo goes back to before the edit");
    assert!(s.redo());
    assert_eq!(s.budget(), &after);
    out
}

fn grand(v: &Value) -> i64 {
    v["report"]["grand_total"].as_i64().unwrap()
}

fn net(v: &Value) -> i64 {
    v["report"]["net_total"].as_i64().unwrap()
}

#[test]
fn charges_are_added_edited_moved_and_deleted() {
    let mut s = Session::new();
    let v0 = view(&mut s);
    let insurance: edit::ChargeInput = input(serde_json::json!({
        "name": "Insurance", "kind": "flat", "amount": "$1,000.00",
        "applied": true, "excluded_amounts": [], "excluded_groups": [],
        "excluded_categories": [], "excluded_accounts": []
    }));
    let at = one_step(&mut s, |b| edit::set_charge(b, None, &insurance));
    assert_eq!(at, 1);
    let v = view(&mut s);
    assert_eq!(grand(&v) - grand(&v0), 100_000);
    assert_eq!(v["budget"]["charges"][1]["kind"], "flat");
    assert_eq!(v["budget"]["charges"][1]["amount"], 100_000);
    assert_eq!(v["report"]["charges"][1]["amount"], 100_000);

    // Edit in place: a percent with a manual exclusion and a category left out.
    let fee: edit::ChargeInput = input(serde_json::json!({
        "name": "Completion bond", "account": "5002", "applied": true,
        "kind": "percent", "percent": "2",
        "excluded_amounts": [{"description": "Bond deposit", "amount": "(1,000)"}],
        "excluded_groups": [], "excluded_categories": ["50"], "excluded_accounts": []
    }));
    one_step(&mut s, |b| edit::set_charge(b, Some(1), &fee));
    let c = &s.budget().charges[1];
    assert_eq!(c.name, "Completion bond");
    assert_eq!(
        c.account.as_deref(),
        Some("50-02"),
        "normalized by the coding"
    );
    assert_eq!(c.excluded_amounts, vec![("Bond deposit".into(), -100_000)]);
    let v = view(&mut s);
    assert_eq!(v["budget"]["charges"][1]["excluded_categories"][0], "50");
    assert!(v["report"]["charges"][1]["excluded"].as_i64().unwrap() > 0);

    // Unapplied charges stay but don't count.
    let mut off = fee.clone();
    off.applied = false;
    one_step(&mut s, |b| edit::set_charge(b, Some(1), &off));
    let v = view(&mut s);
    assert_eq!(v["budget"]["charges"][1]["applied"], false);
    assert_eq!(grand(&v), grand(&v0));

    // Order matters and moves are one step each.
    one_step(&mut s, |b| edit::move_charge(b, 1, 0));
    let names: Vec<_> = s.budget().charges.iter().map(|c| c.name.clone()).collect();
    assert_eq!(names, ["Completion bond", "Contingency"]);
    one_step(&mut s, |b| edit::delete_charge(b, 0));
    assert_eq!(s.budget().charges.len(), 1);
    assert_eq!(grand(&view(&mut s)), grand(&v0));
}

#[test]
fn bad_charges_are_refused_and_change_nothing() {
    let mut s = Session::new();
    let charge = |v: Value| -> edit::ChargeInput { input(v) };
    let ok = serde_json::json!({"name": "Fee", "kind": "percent", "percent": "5"});
    let with = |k: &str, val: Value| {
        let mut c = ok.clone();
        c[k] = val;
        charge(c)
    };
    let cases = [
        with("name", "  ".into()),
        with("name", "Contingency".into()),
        with("kind", "sometimes".into()),
        with("percent", "-1".into()),
        with("percent", "ten".into()),
        with("percent", Value::Null),
        with("excluded_groups", serde_json::json!(["Nope"])),
        with("excluded_categories", serde_json::json!(["99"])),
        with("excluded_accounts", serde_json::json!(["99-99"])),
        with(
            "excluded_amounts",
            serde_json::json!([{"description": "x", "amount": "12a"}]),
        ),
        with("account", "11_01".into()),
        charge(serde_json::json!({"name": "Fee", "kind": "flat"})),
        charge(serde_json::json!({"name": "Fee", "kind": "flat", "amount": "abc"})),
        charge(serde_json::json!({"name": "Fee", "kind": "flat", "amount": "-5"})),
        charge(
            serde_json::json!({"name": "Fee", "kind": "flat", "amount": "5", "currency": "EUR"}),
        ),
    ];
    for c in &cases {
        refused(&mut s, |b| edit::set_charge(b, None, c));
    }
    let good = charge(ok);
    refused(&mut s, |b| edit::set_charge(b, Some(5), &good));
    refused(&mut s, |b| edit::delete_charge(b, 1));
    refused(&mut s, |b| edit::move_charge(b, 0, 1));
    refused(&mut s, |b| edit::move_charge(b, 3, 0));
    // Renaming a charge to its own name is fine.
    let same =
        charge(serde_json::json!({"name": "Contingency", "kind": "percent", "percent": "12"}));
    one_step(&mut s, |b| edit::set_charge(b, Some(0), &same));
}

#[test]
fn a_charge_keeps_an_exclusion_whose_group_is_gone() {
    let mut s = Session::new();
    // A file can name a group the budget no longer has; editing the charge's
    // other fields must still work.
    s.edit("Test edit", |b| {
        b.charges[0].excluded_groups.insert("Old".into());
        Ok(())
    })
    .unwrap();
    let c: edit::ChargeInput = input(serde_json::json!({
        "name": "Contingency", "kind": "percent", "percent": "8", "excluded_groups": ["Old"]
    }));
    one_step(&mut s, |b| edit::set_charge(b, Some(0), &c));
    // But a new charge can't pick it.
    refused(&mut s, |b| {
        edit::set_charge(
            b,
            None,
            &edit::ChargeInput {
                name: "New".into(),
                ..c.clone()
            },
        )
    });
}

#[test]
fn credits_reduce_the_net_total() {
    let mut s = Session::new();
    s.edit("Test edit", |b| {
        edit::set_group(
            b,
            "Local",
            &GroupInput {
                short_code: None,
                description: String::new(),
                in_budget_total: true,
            },
        )
    })
    .unwrap();
    let v0 = view(&mut s);
    let credit: edit::CreditInput = input(serde_json::json!({
        "name": "State incentive", "applied": true, "cap": null, "discount_percent": "0",
        "excluded_fringes": ["PAYROLL"],
        "rows": [
            {"description": "Typed spend", "source": {"kind": "flat", "amount": "100,000"},
             "percent": "25", "cap": {"kind": "none"}},
            {"description": "Local crew", "source": {"kind": "group", "group": "Local"},
             "percent": "30", "cap": {"kind": "credit", "amount": "5,000"}}
        ]
    }));
    let at = one_step(&mut s, |b| edit::set_credit(b, None, &credit));
    assert_eq!(at, 0);
    let v = view(&mut s);
    assert_eq!(grand(&v), grand(&v0));
    assert_eq!(v["report"]["credits"][0]["amount"], 2_500_000);
    assert_eq!(net(&v), grand(&v) - 2_500_000);
    let rows = &v["budget"]["credits"][0]["rows"];
    assert_eq!(rows[0]["source"]["amount"], 10_000_000);
    assert_eq!(rows[1]["source"]["group"], "Local");
    assert!(rows[1]["source"].get("excluded_fringes").is_none());
    assert_eq!(v["budget"]["credits"][0]["excluded_fringes"][0], "PAYROLL");
    assert_eq!(rows[1]["cap"]["kind"], "credit");
    assert_eq!(rows[1]["cap"]["amount"], 500_000);

    // Cap and discount: 25,000 capped at 20,000, less 10%.
    let mut capped = credit.clone();
    capped.cap = Some("$20,000".into());
    capped.discount_percent = Some("10".into());
    one_step(&mut s, |b| edit::set_credit(b, Some(0), &capped));
    assert_eq!(view(&mut s)["report"]["credits"][0]["amount"], 1_800_000);

    // A group a credit uses can't be deleted (existing rule), so the credit
    // goes first.
    refused(&mut s, |b| edit::delete_group(b, "Local"));
    let second: edit::CreditInput = input(serde_json::json!({"name": "Rebate"}));
    one_step(&mut s, |b| edit::set_credit(b, None, &second));
    one_step(&mut s, |b| edit::move_credit(b, 1, 0));
    assert_eq!(s.budget().credits[0].name, "Rebate");
    assert!(s.budget().credits[0].applied, "applied defaults to true");
    one_step(&mut s, |b| edit::delete_credit(b, 1));
    one_step(&mut s, |b| edit::delete_group(b, "Local"));
    assert_eq!(net(&view(&mut s)), grand(&v0));
}

/// A credit row on a topsheet category counts the category with its
/// fringes (decisions.md, entry 57), and the group rules are one command
/// each.
#[test]
fn category_credit_rows_and_group_rules() {
    let mut s = Session::new();
    let v0 = view(&mut s);
    let code = s.budget().categories().next().unwrap().code.clone();
    let cat = &v0["report"]["categories"][0];
    let want = cat["details_total"].as_i64().unwrap() + cat["fringe_total"].as_i64().unwrap();
    let credit: edit::CreditInput = input(serde_json::json!({
        "name": "Category", "rows": [
            {"description": "First category", "source": {"kind": "category", "category": code},
             "percent": "10", "cap": {"kind": "none"}}
        ]
    }));
    one_step(&mut s, |b| edit::set_credit(b, None, &credit));
    let v = view(&mut s);
    assert_eq!(
        v["budget"]["credits"][0]["rows"][0]["source"]["kind"],
        "category"
    );
    assert_eq!(v["report"]["credits"][0]["rows"][0]["gross"], want);
    assert_eq!(v["report"]["credits"][0]["amount"], (want + 5) / 10);

    assert!(
        v["budget"].get("group_rules").is_none(),
        "default is left out"
    );
    one_step(&mut s, |b| {
        edit::set_group_rule(b, "ungrouped_lines", "exclude")
    });
    let v = view(&mut s);
    assert_eq!(v["budget"]["group_rules"]["ungrouped_lines"], "exclude");
    assert_ne!(grand(&v), grand(&v0), "the starter's lines have no group");
    one_step(&mut s, |b| {
        edit::set_group_rule(b, "fringes_in_totals", "exclude")
    });
    one_step(&mut s, |b| edit::set_group_rule(b, "conflict", "include"));
    refused(&mut s, |b| edit::set_group_rule(b, "conflict", "maybe"));
    refused(&mut s, |b| edit::set_group_rule(b, "nope", "include"));
    let r = s.budget().group_rules;
    assert_eq!(
        (r.ungrouped_lines, r.conflict, r.fringes_in_totals),
        (Inclusion::Exclude, Inclusion::Include, Inclusion::Exclude)
    );
}

/// Rounding and the fringes after the last production total are one
/// command each, one undo step, and saved only when not the default.
#[test]
fn arithmetic_and_trailing_level_fringes() {
    let mut s = Session::new();
    let v0 = view(&mut s);
    assert!(
        v0["budget"].get("arithmetic").is_none(),
        "default is left out"
    );
    assert!(v0["budget"].get("trailing_level_fringes").is_none());

    one_step(&mut s, |b| edit::set_arithmetic(b, "exact"));
    let v = view(&mut s);
    assert_eq!(v["budget"]["arithmetic"], "exact");
    one_step(&mut s, |b| edit::set_arithmetic(b, "binary"));
    assert!(view(&mut s)["budget"].get("arithmetic").is_none());
    refused(&mut s, |b| edit::set_arithmetic(b, "float"));
    assert_eq!(s.budget().arithmetic, method_core::Arithmetic::Mmb);

    one_step(&mut s, |b| edit::set_trailing_level_fringes(b, "drop"));
    assert_eq!(view(&mut s)["budget"]["trailing_level_fringes"], "drop");
    refused(&mut s, |b| edit::set_trailing_level_fringes(b, "lose"));
    one_step(&mut s, |b| edit::set_trailing_level_fringes(b, "keep"));
    assert_eq!(
        s.budget().trailing_level_fringes,
        method_core::TrailingFringes::Keep
    );
}

#[test]
fn bad_credits_are_refused_and_change_nothing() {
    let mut s = Session::new();
    let row = |src: Value, pct: &str, cap: Value| serde_json::json!({"description": "r", "source": src, "percent": pct, "cap": cap});
    let flat = serde_json::json!({"kind": "flat", "amount": "10"});
    let none = serde_json::json!({"kind": "none"});
    let credit = |extra: Value| -> edit::CreditInput {
        let mut c = serde_json::json!({"name": "C"});
        for (k, v) in extra.as_object().unwrap() {
            c[k] = v.clone();
        }
        input(c)
    };
    let cases = [
        credit(serde_json::json!({"name": ""})),
        credit(serde_json::json!({"cap": "-1"})),
        credit(serde_json::json!({"cap": "lots"})),
        credit(serde_json::json!({"discount_percent": "101"})),
        credit(serde_json::json!({"discount_percent": "-1"})),
        credit(serde_json::json!({"account": "#1"})),
        credit(serde_json::json!({"rows": [row(flat.clone(), "x", none.clone())]})),
        credit(serde_json::json!({"rows": [row(flat.clone(), "-5", none.clone())]})),
        credit(
            serde_json::json!({"rows": [row(serde_json::json!({"kind": "flat", "amount": "-10"}), "5", none.clone())]}),
        ),
        credit(
            serde_json::json!({"rows": [row(serde_json::json!({"kind": "group", "group": "Nope"}), "5", none.clone())]}),
        ),
        credit(
            serde_json::json!({"rows": [row(serde_json::json!({"kind": "group", "group": ""}), "5", none.clone())]}),
        ),
        credit(
            serde_json::json!({"rows": [row(serde_json::json!({"kind": "category", "category": "Nope"}), "5", none.clone())]}),
        ),
        credit(
            serde_json::json!({"rows": [row(serde_json::json!({"kind": "category", "category": " "}), "5", none.clone())]}),
        ),
        credit(
            serde_json::json!({"rows": [row(flat.clone(), "5", serde_json::json!({"kind": "qualified_spend", "amount": "-1"}))]}),
        ),
    ];
    for c in &cases {
        refused(&mut s, |b| edit::set_credit(b, None, c));
    }
    one_step(&mut s, |b| {
        edit::set_credit(b, None, &credit(serde_json::json!({})))
    });
    refused(&mut s, |b| {
        edit::set_credit(b, None, &credit(serde_json::json!({})))
    });
    refused(&mut s, |b| {
        edit::set_credit(b, Some(1), &credit(serde_json::json!({"name": "D"})))
    });
    refused(&mut s, |b| edit::delete_credit(b, 1));
    refused(&mut s, |b| edit::move_credit(b, 0, 1));
    // Unknown JSON shapes don't even reach the edit.
    assert!(serde_json::from_value::<edit::CreditRowInput>(row(
        serde_json::json!({"kind": "teleport"}),
        "5",
        none
    ))
    .is_err());
}

#[test]
fn sets_and_locations_tag_rows_and_deleting_untags_them() {
    let mut s = Session::new();
    let id = ids_in(&s, "21-01")[0];
    let set: edit::SetInput = input(serde_json::json!({"description": "Diner interior"}));
    one_step(&mut s, |b| edit::set_set(b, "Diner", &set));
    let loc: edit::LocationInput = input(serde_json::json!({
        "description": "Stage 4", "city": "Burbank", "region": "CA", "country": "US"
    }));
    one_step(&mut s, |b| edit::set_location(b, "Stage", &loc));
    s.set_detail_field(id, "set", "Diner").unwrap();
    s.set_detail_field(id, "location", "Stage").unwrap();
    let v = view(&mut s);
    assert_eq!(
        v["budget"]["sets"]["Diner"]["description"],
        "Diner interior"
    );
    assert_eq!(v["budget"]["locations"]["Stage"]["city"], "Burbank");
    assert!(
        v["report"]["sets"]["Diner"]["details_total"]
            .as_i64()
            .unwrap()
            > 0
    );

    for bad in ["", "  ", "A,B"] {
        refused(&mut s, |b| edit::set_set(b, bad, &set));
        refused(&mut s, |b| edit::set_location(b, bad, &loc));
    }
    refused(&mut s, |b| edit::delete_set(b, "Nope"));
    refused(&mut s, |b| edit::delete_location(b, "Nope"));

    one_step(&mut s, |b| edit::delete_set(b, "Diner"));
    one_step(&mut s, |b| edit::delete_location(b, "Stage"));
    let d = edit::details(s.budget()).find(|d| d.id == id).unwrap();
    assert_eq!((d.set.clone(), d.location.clone()), (None, None));
    assert!(s.budget().sets.is_empty() && s.budget().locations.is_empty());
}

#[test]
fn budget_info_fields_are_typed() {
    let mut s = Session::new();
    let info = |kind: &str, value: &str| -> edit::InfoInput {
        input(serde_json::json!({"kind": kind, "value": value}))
    };
    one_step(&mut s, |b| {
        edit::set_info(b, "Director", &info("text", " J. Director "))
    });
    one_step(&mut s, |b| {
        edit::set_info(b, "Shoot days", &info("number", "30"))
    });
    one_step(&mut s, |b| {
        edit::set_info(b, "Start", &info("date", "2027-03-01"))
    });
    let v = view(&mut s);
    assert_eq!(v["budget"]["info"]["Director"]["value"], "J. Director");
    assert_eq!(v["budget"]["info"]["Shoot days"]["kind"], "number");
    assert_eq!(v["budget"]["info"]["Shoot days"]["value"], "30");
    assert_eq!(v["budget"]["info"]["Start"]["kind"], "date");

    refused(&mut s, |b| {
        edit::set_info(b, "Shoot days", &info("number", "thirty"))
    });
    refused(&mut s, |b| {
        edit::set_info(b, "Start", &info("date", "2027-02-30"))
    });
    refused(&mut s, |b| {
        edit::set_info(b, "Start", &info("date", "March 1"))
    });
    refused(&mut s, |b| edit::set_info(b, " ", &info("text", "x")));
    refused(&mut s, |b| edit::set_info(b, "X", &info("colour", "x")));
    refused(&mut s, |b| edit::delete_info(b, "Nope"));
    // Changing the kind of an existing field replaces it.
    one_step(&mut s, |b| {
        edit::set_info(b, "Shoot days", &info("text", "about 30"))
    });
    one_step(&mut s, |b| edit::delete_info(b, "Director"));
    assert_eq!(s.budget().info.len(), 2);
}

#[test]
fn fringe_posting_coding_and_currencies() {
    let mut s = Session::new();
    // A new budget posts fringes by budget (decisions.md, entry 59): one
    // fringe row, no fringe accounts.
    let v = view(&mut s);
    assert_eq!(v["budget"]["fringe_posting"]["kind"], "budget");
    assert!(v["report"]["categories"][1]["fringe_account"].is_null());
    assert!(v["report"]["budget_fringes"].as_i64().unwrap() > 0);
    // By category with suffix 99, the fringe account is 11-99.
    one_step(&mut s, |b| edit::set_fringe_posting(b, "category", None));
    let v = view(&mut s);
    assert_eq!(
        v["report"]["categories"][1]["fringe_account"]["code"],
        "11-99"
    );
    let g = grand(&v);

    for mode in ["budget", "production_level", "account"] {
        one_step(&mut s, |b| edit::set_fringe_posting(b, mode, None));
        let v = view(&mut s);
        assert_eq!(v["budget"]["fringe_posting"]["kind"], mode);
        // Charges count fringes wherever they post (decisions.md, entry 56).
        assert_eq!(grand(&v), g, "{mode} posting doesn't change the total");
    }
    // Back to category: a missing suffix means the default.
    one_step(&mut s, |b| edit::set_fringe_posting(b, "category", None));
    assert!(
        view(&mut s)["budget"].get("fringe_posting").is_none(),
        "default is left out"
    );
    one_step(&mut s, |b| {
        edit::set_fringe_posting(b, "category", Some("FR"))
    });
    assert_eq!(
        view(&mut s)["report"]["categories"][1]["fringe_account"]["code"],
        "11-FR"
    );
    refused(&mut s, |b| {
        edit::set_fringe_posting(b, "category", Some(""))
    });
    refused(&mut s, |b| {
        edit::set_fringe_posting(b, "category", Some("9-9"))
    });
    refused(&mut s, |b| edit::set_fringe_posting(b, "weekly", None));

    // Account coding: validated, written to the file, used for new numbers.
    one_step(&mut s, |b| {
        edit::set_account_coding(b, [2, 4, 0, 0], Some("."))
    });
    let v = view(&mut s);
    assert_eq!(v["budget"]["account_coding"]["account_width"], 4);
    assert_eq!(v["budget"]["account_coding"]["separator"], ".");
    let code = s
        .edit("Test edit", |b| {
            edit::add_account(b, "21", "21-7", "Grip", None)
        })
        .unwrap();
    assert_eq!(code, "21.0007");
    refused(&mut s, |b| edit::set_account_coding(b, [2, 0, 2, 0], None));
    refused(&mut s, |b| {
        edit::set_account_coding(b, [2, 2, 0, 0], Some("_"))
    });

    // Currencies.
    s.edit("Test edit", |b| {
        edit::set_currency(
            b,
            "EUR",
            &CurrencyInput {
                description: String::new(),
                symbol: String::new(),
                rate: "2".into(),
            },
        )
    })
    .unwrap();
    one_step(&mut s, |b| edit::set_totals_currency(b, Some("EUR")));
    let v = view(&mut s);
    assert_eq!(v["budget"]["totals_currency"], "EUR");
    assert_eq!(v["report"]["display"]["grand_total"], grand(&v) / 2);
    refused(&mut s, |b| edit::set_totals_currency(b, Some("GBP")));
    refused(&mut s, |b| edit::set_base_currency(b, "EUR"));
    refused(&mut s, |b| edit::set_base_currency(b, " "));
    one_step(&mut s, |b| edit::set_base_currency(b, "CAD"));
    assert_eq!(view(&mut s)["budget"]["base_currency"], "CAD");
    one_step(&mut s, |b| edit::set_totals_currency(b, None));
    assert!(view(&mut s)["budget"]["totals_currency"].is_null());
    // The base currency as the totals currency means "none".
    s.edit("Test edit", |b| edit::set_totals_currency(b, Some("EUR")))
        .unwrap();
    one_step(&mut s, |b| edit::set_totals_currency(b, Some("CAD")));
    assert_eq!(s.budget().totals_currency, None);
}

#[test]
fn baseline_lock_shows_variance_and_clear_removes_it() {
    let mut s = Session::new();
    assert!(view(&mut s)["variance"].is_null());
    refused(&mut s, edit::clear_baseline);

    one_step(&mut s, edit::lock_baseline);
    let v = view(&mut s);
    let var = &v["variance"];
    assert_eq!(var["grand_total"]["original"], grand(&v));
    assert_eq!(var["grand_total"]["variance"], 0);
    assert_eq!(var["net_total"]["variance"], 0);
    assert_eq!(v["budget"]["baseline"]["grand_total"], grand(&v));

    // Edit a row, add one, delete one.
    let ids = ids_in(&s, "50-01");
    s.set_detail_field(ids[0], "rate", "20000").unwrap();
    let added = s.add_detail("50-01", None, "line").unwrap();
    s.set_detail_field(added, "amount", "1").unwrap();
    s.set_detail_field(added, "rate", "100").unwrap();
    let gone = ids_in(&s, "21-01")[0];
    s.edit("Test edit", |b| edit::delete_details(b, &[gone]))
        .unwrap();

    let v = view(&mut s);
    let var = &v["variance"];
    let row = &var["rows"][ids[0].to_string()];
    assert_eq!(row["original"], 1_500_000);
    assert_eq!(row["current"], 2_000_000);
    assert_eq!(row["variance"], 500_000);
    let new = &var["rows"][added.to_string()];
    assert!(new["original"].is_null());
    assert_eq!(new["variance"], 10_000);
    let old = &var["rows"][gone.to_string()];
    assert!(old["current"].is_null());
    assert!(old["variance"].as_i64().unwrap() < 0);
    assert_eq!(var["accounts"]["50-01"]["variance"], 510_000);
    assert_eq!(var["categories"]["50"]["variance"], 510_000);
    let g = &var["grand_total"];
    assert_eq!(
        g["variance"].as_i64().unwrap(),
        g["current"].as_i64().unwrap() - g["original"].as_i64().unwrap()
    );
    assert_eq!(g["current"], grand(&v));
    // A new row never reuses a baseline row's id.
    let next = s.add_detail("21-01", None, "line").unwrap();
    assert!(next > gone);

    // Locking again takes the new totals; clearing removes the variance.
    one_step(&mut s, edit::lock_baseline);
    assert_eq!(view(&mut s)["variance"]["grand_total"]["variance"], 0);
    one_step(&mut s, edit::clear_baseline);
    let v = view(&mut s);
    assert!(v["variance"].is_null());
    assert!(v["budget"].get("baseline").is_none());
}

#[test]
fn baseline_is_refused_while_the_budget_has_errors() {
    let mut s = Session::new();
    let id = ids_in(&s, "21-01")[0];
    s.set_detail_field(id, "amount", "NOPE * 2").unwrap();
    let e = refused(&mut s, edit::lock_baseline);
    assert!(e.contains("NOPE"), "{e}");
    // With a baseline locked earlier, partial totals still give a variance.
    s.set_detail_field(id, "amount", "1").unwrap();
    s.edit("Test edit", edit::lock_baseline).unwrap();
    s.set_detail_field(id, "amount", "NOPE * 2").unwrap();
    let v = view(&mut s);
    assert_eq!(v["partial"], true);
    assert!(
        v["variance"]["rows"][id.to_string()]["variance"]
            .as_i64()
            .unwrap()
            < 0
    );
}

/// The lean view every command returns is the full view cut down: the
/// budget without rows (with each account's row count), totals without
/// per-row figures, and the account on screen in full.
#[test]
fn lean_view_is_the_full_view_cut_down() {
    let mut b = method_core::sample::budget(&method_core::sample::SampleSpec {
        rows: 600,
        accounts: 14,
        categories: 4,
        ..Default::default()
    });
    // Two problem cells in different accounts, and a version saved.
    let snapshot = b.snapshot();
    b.versions.push(method_core::Version {
        id: 1,
        name: "First pass".into(),
        created: "2026-10-09T10:00:00Z".into(),
        note: Some("before notes".into()),
        budget: std::sync::Arc::new(snapshot),
    });
    let mut s = Session::with_budget(b, None);
    let places: Vec<(usize, usize, String, String)> = s
        .budget()
        .topsheet
        .iter()
        .enumerate()
        .filter_map(|(ti, r)| match r {
            method_core::TopsheetRow::Category(c) => Some((ti, c)),
            _ => None,
        })
        .flat_map(|(ti, c)| {
            c.accounts
                .iter()
                .enumerate()
                .map(move |(ai, a)| (ti, ai, c.code.clone(), a.code.clone()))
        })
        .collect();
    let first = ids_in(&s, &places[0].3)[0];
    let last = *ids_in(&s, &places[places.len() - 1].3).last().unwrap();
    s.set_detail_field(first, "rate", "NOPE").unwrap();
    s.set_detail_field(last, "amount", "1 +").unwrap();

    let full = view(&mut s);
    for (ti, ai, cat, code) in places.iter().cloned() {
        s.set_focus(Some(Focus {
            category: cat,
            ti,
            account: code.clone(),
            ai,
        }));
        let lean = serde_json::to_value(s.view()).unwrap();
        assert_eq!(lean["rev"], full["rev"]);

        // Budget: the full one with rows swapped for counts.
        let mut want = full["budget"].clone();
        for r in want["topsheet"].as_array_mut().unwrap() {
            let accounts = r.get_mut("accounts").and_then(Value::as_array_mut);
            for a in accounts.into_iter().flatten() {
                let n = a
                    .as_object_mut()
                    .unwrap()
                    .remove("details")
                    .map_or(0, |d| d.as_array().unwrap().len());
                a["lines"] = n.into();
            }
        }
        want["baseline"].as_object_mut().unwrap().remove("rows");
        assert_eq!(lean["budget"], want);
        assert_eq!(lean["budget"]["versions"][0]["name"], "First pass");
        assert!(lean["budget"]["versions"][0].get("budget").is_none());

        // Report: the full one without rows; the account's rows separately.
        let mut want = full["report"].clone();
        let mut rows = Value::Null;
        let ci = places
            .iter()
            .map(|p| p.0)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .position(|t| t == ti)
            .unwrap();
        for (i, c) in want["categories"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            for (j, a) in c["accounts"].as_array_mut().unwrap().iter_mut().enumerate() {
                let r = a.as_object_mut().unwrap().remove("rows").unwrap();
                if i == ci && j == ai {
                    rows = r;
                }
            }
        }
        assert_eq!(lean["report"], want);
        let acct = &lean["account"];
        assert_eq!(
            (acct["ti"].as_u64(), acct["ai"].as_u64()),
            (Some(ti as u64), Some(ai as u64))
        );
        assert_eq!(acct["code"], code.as_str());
        assert_eq!(acct["rows"], rows);
        assert_eq!(
            acct["details"],
            full["budget"]["topsheet"][ti]["accounts"][ai]["details"]
        );

        // Problems: all counted, the ones in this account listed.
        assert_eq!(lean["issue_count"], 2);
        assert_eq!(lean["issue_accounts"].as_array().unwrap().len(), 2);
        let ids: Vec<RowId> = ids_in(&s, &code);
        let listed = lean["issues"].as_array().unwrap();
        let want: Vec<&Value> = full["issues"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| ids.contains(&i["target"]["row_id"].as_u64().unwrap()))
            .collect();
        assert_eq!(listed.iter().collect::<Vec<_>>(), want);
        for k in ["error", "partial", "file", "can_undo", "can_redo"] {
            assert_eq!(lean[k], full[k], "{k}");
        }

        // Variance: the account's rows; the same totals and count.
        let lv = &lean["variance"];
        for k in [
            "accounts",
            "categories",
            "grand_total",
            "net_total",
            "rows_changed",
        ] {
            assert_eq!(lv[k], full["variance"][k], "{k}");
        }
        let lrows = lv["rows"].as_object().unwrap();
        assert!(lrows.keys().all(|k| ids.contains(&k.parse().unwrap())));
        assert!(full["variance"]["rows_changed"].as_u64().unwrap() >= 2);
    }
    // No account on screen: no rows at all.
    s.set_focus(None);
    let lean = serde_json::to_value(s.view()).unwrap();
    assert!(lean["account"].is_null());
    assert!(lean["issues"].as_array().unwrap().is_empty());
    assert!(lean["variance"]["rows"].as_object().unwrap().is_empty());
}

/// A focused account is found by code first, as the UI finds its place.
#[test]
fn focus_follows_the_account_code() {
    let mut s = Session::new();
    // 21-01 is the only account in category 21, at topsheet index 6.
    s.set_focus(Some(Focus {
        category: "21".into(),
        ti: 99,
        account: "21-01".into(),
        ai: 7,
    }));
    assert_eq!(s.focused(), Some((6, 0)));
    let v = serde_json::to_value(s.view()).unwrap();
    assert_eq!(v["account"]["code"], "21-01");
    s.set_focus(Some(Focus {
        category: "21".into(),
        ti: 6,
        account: "99-99".into(),
        ai: 0,
    }));
    assert_eq!(s.focused(), None);
    assert!(serde_json::to_value(s.view()).unwrap()["account"].is_null());
}

/// The cutoff factor and the budget's fringe row are one command each, one
/// undo step, and saved only when not the default (decisions.md, entry 59).
/// Moving the fringe row changes no total, and it stays above the same
/// topsheet row when rows are added or removed above it.
#[test]
fn cutoff_factor_and_fringe_row() {
    let mut s = Session::new();
    let v0 = view(&mut s);
    assert!(v0["budget"].get("cutoff_factor").is_none());
    assert!(v0["budget"].get("fringe_row").is_none());
    let g = grand(&v0);

    one_step(&mut s, |b| edit::set_cutoff_factor(b, "1.5"));
    assert_eq!(view(&mut s)["budget"]["cutoff_factor"], "1.5");
    refused(&mut s, |b| edit::set_cutoff_factor(b, "4"));
    one_step(&mut s, |b| edit::set_cutoff_factor(b, "1"));
    assert!(view(&mut s)["budget"].get("cutoff_factor").is_none());

    let row = |account: &str, description: &str, position: Option<usize>| edit::FringeRowInput {
        account: Some(account.into()),
        description: Some(description.into()),
        position,
    };
    one_step(&mut s, |b| {
        edit::set_fringe_row(b, &row("9900", "Fringe benefits", Some(4)))
    });
    let v = view(&mut s);
    assert_eq!(v["budget"]["fringe_row"]["account"], "9900");
    assert_eq!(v["budget"]["fringe_row"]["position"], 4);
    assert_eq!(grand(&v), g, "moving the fringe row changes no total");
    refused(&mut s, |b| edit::set_fringe_row(b, &row("99#00", "", None)));
    refused(&mut s, |b| edit::set_fringe_row(b, &row("", "", Some(99))));
    // A category added above it keeps it above the same row.
    s.edit("Test edit", |b| {
        edit::add_category(b, "05", "Development", Some(0))
    })
    .unwrap();
    assert_eq!(s.budget().fringe_row.position, Some(5));
    s.edit("Test edit", |b| edit::delete_topsheet_row(b, 0))
        .unwrap();
    assert_eq!(s.budget().fringe_row.position, Some(4));
    one_step(&mut s, |b| edit::set_fringe_row(b, &row("", "", None)));
    assert!(view(&mut s)["budget"].get("fringe_row").is_none());
}

/// Typed into a line, a formula that divides by zero or uses a global that
/// refers back to itself is refused; a global that would refer back to
/// itself is refused; anything else is kept and flagged (decisions.md,
/// entry 59). An Amt that is exactly a global with a unit fills an empty
/// Unit.
#[test]
fn formula_entry_refusals_and_a_global_unit() {
    let mut s = Session::new();
    let id = ids_in(&s, "13-01")[0];
    let e = refused(&mut s, |b| {
        edit::set_detail_field(b, id, edit::Field::Amount, "SHOOT_DAYS / 0")
    });
    assert!(e.contains("divides by zero"), "{e}");
    refused(&mut s, |b| {
        edit::set_rows_field(b, &[id], edit::Field::Rate, "1 / (SHOOT_DAYS - 25)")
    });
    // A typo is kept, as before.
    one_step(&mut s, |b| {
        edit::set_detail_field(b, id, edit::Field::Rate, "NOPE")
    });

    one_step(&mut s, |b| edit::set_global(b, "ZG_X", "ZG_Y + 1"));
    let e = refused(&mut s, |b| edit::set_global(b, "ZG_Y", "ZG_X + 1"));
    assert!(e.contains("refer back to itself"), "{e}");
    refused(&mut s, |b| edit::set_global(b, "ZG_Y", "2 / 0"));

    // A global with a unit fills an empty Unit, and only an empty one.
    s.edit("Test edit", |b| {
        edit::set_global(b, "ZG_W", "2")?;
        b.global_info.insert(
            "ZG_W".into(),
            method_core::GlobalInfo {
                unit: "Week".into(),
                ..Default::default()
            },
        );
        edit::set_detail_field(b, id, edit::Field::Unit, "")
    })
    .unwrap();
    one_step(&mut s, |b| {
        edit::set_detail_field(b, id, edit::Field::Amount, "ZG_W")
    });
    let d = s
        .budget()
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == id)
        .unwrap()
        .clone();
    assert_eq!(d.unit, "Week");
    s.edit("Test edit", |b| {
        edit::set_detail_field(b, id, edit::Field::Unit, "Day")
    })
    .unwrap();
    s.edit("Test edit", |b| {
        edit::set_detail_field(b, id, edit::Field::Amount, "ZG_W")
    })
    .unwrap();
    let d = s
        .budget()
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == id)
        .unwrap()
        .clone();
    assert_eq!(d.unit, "Day", "a Unit already there stays");
}
