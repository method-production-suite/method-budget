//! The app's command layer without a window: edit -> recalc -> view, undo and
//! redo, the dirty flag, setup tables, and saving and opening files.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use fwb_app::edit::{self, CurrencyInput, FringeInput, GroupInput, UnitInput};
use fwb_app::Session;
use fwb_core::{Budget, RowId};
use serde_json::Value;

fn view(s: &mut Session) -> Value {
    serde_json::to_value(s.view()).unwrap()
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
        "fwb-app-test-{}-{}",
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
    assert_eq!(v["file"]["name"], "Untitled.fwb");
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
    assert!(s.edit(|b| edit::set_global(b, "9lives", "1")).is_err());
    assert!(s
        .edit(|b| edit::add_account(b, "21", "21-01", "Dup", None))
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

    let v = serde_json::to_value(s.view_with(Some(adj))).unwrap();
    assert_eq!(v["created_row"], adj);
    let details = &v["budget"]["topsheet"][5]["accounts"][0]["details"];
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

    s.edit(|b| edit::delete_details(b, &[line, adj])).unwrap();
    assert_eq!(ids_in(&s, "20-01"), [ids[0], ids[1], sub]);
    assert!(s.add_detail("20-01", Some(9999), "line").is_err());
    assert!(s.add_detail("20-01", None, "header").is_err());
}

#[test]
fn bulk_field_edits_are_one_undo_step() {
    let mut s = Session::new();
    let ids = ids_in(&s, "21-01");
    s.edit(|b| {
        edit::set_group(
            b,
            "Second unit",
            &GroupInput {
                description: String::new(),
                in_budget_total: true,
            },
        )
    })
    .unwrap();
    s.edit(|b| edit::set_rows_field(b, &ids, edit::Field::Groups, "Second unit"))
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
        description: "Alternate camera".into(),
        in_budget_total: on,
    };
    s.edit(|b| edit::set_group(b, "Alt", &g(true))).unwrap();
    s.set_detail_field(id, "groups", "Alt").unwrap();
    let full = account(&view(&mut s), "21-01")["total"].as_i64().unwrap();
    s.edit(|b| edit::set_group(b, "Alt", &g(false))).unwrap();
    let v = view(&mut s);
    let row = &account(&v, "21-01")["rows"][1];
    assert_eq!(row["included"], false);
    assert_eq!(row["total"], 0);
    assert_eq!(
        account(&v, "21-01")["total"].as_i64().unwrap(),
        full - 3_000_000
    );
    // Deleting the group untags the row and brings the money back.
    s.edit(|b| edit::delete_group(b, "Alt")).unwrap();
    let v = view(&mut s);
    assert_eq!(account(&v, "21-01")["total"].as_i64().unwrap(), full);
    assert!(v["budget"]["topsheet"][6]["accounts"][0]["details"][1]
        .get("groups")
        .is_none());
}

#[test]
fn globals_set_evaluate_and_delete_by_freezing() {
    let mut s = Session::new();
    s.edit(|b| edit::set_global(b, "SHOOT_DAYS", "30")).unwrap();
    let v = view(&mut s);
    assert_eq!(v["report"]["globals"]["SHOOT_WEEKS"], "6");
    let before = v["report"]["grand_total"].clone();

    s.edit(|b| edit::delete_global(b, "SHOOT_WEEKS")).unwrap();
    let v = view(&mut s);
    assert!(v["budget"]["globals"].get("SHOOT_WEEKS").is_none());
    assert_eq!(
        v["report"]["grand_total"], before,
        "deleting froze the value"
    );
    let lead = &v["budget"]["topsheet"][3]["accounts"][0]["details"][0];
    assert_eq!(lead["amount"], "6 + 1");

    // A broken global is flagged but can still be deleted.
    s.edit(|b| edit::set_global(b, "BROKEN", "1 +")).unwrap();
    let v = view(&mut s);
    assert_eq!(v["issues"][0]["target"]["kind"], "global");
    assert_eq!(v["issues"][0]["target"]["name"], "BROKEN");
    assert_eq!(v["partial"], true);
    s.edit(|b| edit::delete_global(b, "BROKEN")).unwrap();
    assert!(view(&mut s)["error"].is_null());

    assert!(s.edit(|b| edit::set_global(b, "X", "  ")).is_err());
    assert!(s.edit(|b| edit::delete_global(b, "NOPE")).is_err());
}

#[test]
fn fringes_units_and_currencies() {
    let mut s = Session::new();
    let pension = FringeInput {
        kind: "percent".into(),
        description: "Pension".into(),
        rate: "6".into(),
        cutoff: Some("100,000".into()),
        unit: None,
    };
    s.edit(|b| edit::set_fringe(b, "PENSION", &pension))
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
        kind: "flat".into(),
        description: String::new(),
        rate: "50".into(),
        cutoff: None,
        unit: Some("Day".into()),
    };
    s.edit(|b| edit::set_fringe(b, "MEAL", &meal)).unwrap();
    assert!(s
        .edit(|b| edit::set_fringe(b, "A,B", &meal))
        .unwrap_err()
        .contains("commas"));
    assert!(s
        .edit(|b| edit::set_fringe(
            b,
            "BAD",
            &FringeInput {
                kind: "weird".into(),
                ..meal.clone()
            }
        ))
        .is_err());

    // Deleting a fringe takes it off every row.
    s.edit(|b| edit::delete_fringe(b, "PENSION")).unwrap();
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
    s.edit(|b| edit::set_unit(b, "Shift", &unit)).unwrap();
    assert_eq!(view(&mut s)["budget"]["units"]["Shift"]["hours"], "12");
    assert!(s
        .edit(|b| edit::set_unit(
            b,
            "Bad",
            &UnitInput {
                hours: Some("-1".into()),
                ..unit.clone()
            }
        ))
        .is_err());
    s.edit(|b| edit::delete_unit(b, "Shift")).unwrap();

    let eur = CurrencyInput {
        description: "Euro".into(),
        symbol: "€".into(),
        rate: "1.1".into(),
    };
    s.edit(|b| edit::set_currency(b, "EUR", &eur)).unwrap();
    let mix = ids_in(&s, "41-01")[0];
    s.set_detail_field(mix, "currency", "EUR").unwrap();
    let v = view(&mut s);
    let row = &account(&v, "41-01")["rows"][0];
    assert_eq!(row["currency"], "EUR");
    assert_eq!(row["local_total"], 2_000_000);
    assert_eq!(row["total"], 2_200_000);
    assert!(s
        .edit(|b| edit::delete_currency(b, "EUR"))
        .unwrap_err()
        .contains("1 line uses EUR"));
    assert!(s
        .edit(|b| edit::set_currency(b, "USD", &eur))
        .unwrap_err()
        .contains("base currency"));
    assert!(s
        .edit(|b| edit::set_currency(
            b,
            "JPY",
            &CurrencyInput {
                rate: "0".into(),
                ..eur.clone()
            }
        ))
        .is_err());
    s.set_detail_field(mix, "currency", "").unwrap();
    s.edit(|b| edit::delete_currency(b, "EUR")).unwrap();
}

#[test]
fn categories_accounts_and_production_totals() {
    let mut s = Session::new();
    // Codes are normalized by the budget's coding pattern.
    s.edit(|b| edit::add_category(b, "6", "Other", None))
        .unwrap();
    s.edit(|b| edit::add_account(b, "06", "6-1", "Publicity", None))
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
        .edit(|b| edit::add_category(b, "06", "Again", None))
        .unwrap_err()
        .contains("already exists"));

    s.add_detail("06-01", None, "line").unwrap();
    let id = ids_in(&s, "06-01")[0];
    s.set_detail_field(id, "amount", "1").unwrap();
    s.set_detail_field(id, "rate", "2500").unwrap();
    let v = view(&mut s);
    // After the last production total, so it counts below the line.
    let atl_btl = v["report"]["atl_btl"].as_i64().unwrap();

    s.edit(|b| edit::add_production_total(b, "Total publicity", None))
        .unwrap();
    let v = view(&mut s);
    assert_eq!(
        v["report"]["levels"].as_array().unwrap().last().unwrap()["total"],
        250_000
    );
    assert_eq!(v["report"]["atl_btl"].as_i64().unwrap(), atl_btl);

    // Renumber and rename keep the accounts.
    let idx = v["budget"]["topsheet"].as_array().unwrap().len() - 2;
    s.edit(|b| edit::update_topsheet_row(b, idx, Some("60"), "Publicity"))
        .unwrap();
    s.edit(|b| edit::update_account(b, "06-01", "60-01", "Unit publicist"))
        .unwrap();
    assert_eq!(ids_in(&s, "60-01"), [id]);
    assert!(s
        .edit(|b| edit::update_account(b, "60-01", "50-01", "Clash"))
        .is_err());

    s.edit(|b| edit::delete_account(b, "60-01")).unwrap();
    s.edit(|b| edit::delete_topsheet_row(b, idx + 1)).unwrap();
    s.edit(|b| edit::delete_topsheet_row(b, idx)).unwrap();
    assert_eq!(s.budget(), &fwb_core::starter::budget());
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
    let path = dir.join("film.fwb");
    let mut s = Session::new();
    s.edit(|b| edit::set_title(b, "Round Trip")).unwrap();
    assert!(s.is_dirty());
    s.save_to(&path).unwrap();
    assert!(!s.is_dirty());
    assert_eq!(s.display_name(), "film.fwb");
    // No temporary files left behind.
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);

    // Undo after saving makes it dirty again; redo makes it clean.
    s.undo();
    assert!(s.is_dirty());
    s.redo();
    assert!(!s.is_dirty());

    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text, fwb_format::to_string(s.budget()));
    let mut opened = Session::open(&path).unwrap();
    assert_eq!(opened.budget(), s.budget());
    assert_eq!(opened.path(), Some(path.as_path()));
    assert!(!opened.is_dirty() && !opened.can_undo());
    assert_eq!(view(&mut opened)["file"]["name"], "film.fwb");

    std::fs::write(dir.join("bad.fwb"), "{ \"format\": \"something-else\" }").unwrap();
    let err = Session::open(&dir.join("bad.fwb")).unwrap_err();
    assert!(err.contains("not a Film Wizard budget file"), "{err}");
    assert!(Session::open(&dir.join("missing.fwb")).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_file_with_unfixable_errors_still_opens_and_shows_the_problem() {
    let dir = temp_dir();
    let path = dir.join("dupe.fwb");
    let mut b: Budget = fwb_core::starter::budget();
    // Two rows with the same id: the engine refuses to total this budget.
    let first = b.categories().next().unwrap().accounts[0].details[0].id;
    b.categories_mut().nth(1).unwrap().accounts[0].details[0].id = first;
    std::fs::write(&path, fwb_format::to_string(&b)).unwrap();

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
    for i in 0..(fwb_app::session::UNDO_LIMIT + 20) {
        s.set_detail_field(id, "amount", &(i + 2).to_string())
            .unwrap();
    }
    let mut steps = 0;
    while s.undo() {
        steps += 1;
    }
    assert_eq!(steps, fwb_app::session::UNDO_LIMIT);
    // The oldest snapshots were dropped, so this isn't the starter any more,
    // and it isn't the saved state either.
    assert!(s.is_dirty());
}
