//! Every tool's happy path and its refusals, through `tools/call`, on the
//! starter budget (rows 1 to 13; 21-01 Camera holds rows 8 and 9; the
//! PAYROLL fringe is 10% and posts to each category's 99 account).

use method_format::serde_json::{json, Value};

use crate::mcp::tests::Fixture;

fn net(f: &mut Fixture, path: &str) -> i64 {
    f.tool("totals", json!({"path": path})).unwrap()["net_total_cents"]
        .as_i64()
        .unwrap()
}

fn rows(f: &mut Fixture, account: &str) -> Vec<Value> {
    f.tool(
        "get_account",
        json!({"path": "film.mthd", "account": account}),
    )
    .unwrap()["rows"]
        .as_array()
        .unwrap()
        .clone()
}

fn err_contains(r: Result<Value, String>, want: &str) {
    let e = r.expect_err(want);
    assert!(e.contains(want), "wanted `{want}` in: {e}");
}

#[test]
fn reading_tools_describe_the_budget() {
    let mut f = Fixture::new("read", false);
    let open = f.tool("budget_open", json!({"path": "film.mthd"})).unwrap();
    assert_eq!(open["title"], "Untitled feature");
    assert_eq!(open["counts"]["categories"], 10);
    assert_eq!(open["counts"]["lines"], 13);
    assert_eq!(open["read_only"], false);
    // A new budget posts fringes by budget (decisions.md, entry 60).
    assert_eq!(open["fringe_posting"]["mode"], "budget");
    assert_eq!(open["totals"]["net_total_cents"], 57_860_000);
    let info = f.tool("budget_info", json!({"path": "film.mthd"})).unwrap();
    assert_eq!(info["revision"], open["revision"]);

    let top = f
        .tool("list_topsheet", json!({"path": "film.mthd"}))
        .unwrap();
    let trows = top["rows"].as_array().unwrap();
    assert_eq!(trows.len(), 14);
    assert_eq!(trows[0]["code"], "10");
    // The writing line alone: its fringe posts to the budget's fringe row.
    assert_eq!(trows[0]["total_cents"], 5_000_000);
    assert_eq!(trows[4]["type"], "production_total");
    assert_eq!(trows[4]["above_the_line"], true);

    let accts = f
        .tool("list_accounts", json!({"path": "film.mthd"}))
        .unwrap();
    assert_eq!(accts["accounts"].as_array().unwrap().len(), 10);
    let cast = f
        .tool(
            "list_accounts",
            json!({"path": "film.mthd", "category": "13"}),
        )
        .unwrap();
    assert_eq!(cast["accounts"][0]["code"], "13-01");
    err_contains(
        f.tool(
            "list_accounts",
            json!({"path": "film.mthd", "category": "77"}),
        ),
        "no category 77",
    );

    let cam = f
        .tool(
            "get_account",
            json!({"path": "film.mthd", "account": "21-01"}),
        )
        .unwrap();
    let r = cam["rows"].as_array().unwrap();
    assert_eq!(
        (r[0]["id"].clone(), r[1]["id"].clone()),
        (json!(8), json!(9))
    );
    assert_eq!(r[0]["amount"], "SHOOT_WEEKS + 1");
    assert_eq!(r[0]["quantity"], "6");
    assert_eq!(r[0]["total_cents"], 2_700_000);
    assert_eq!(r[0]["fringes"], json!(["PAYROLL"]));
    assert_eq!(r[0]["fringes_cents"]["PAYROLL"], 270_000);
    let page = f
        .tool(
            "get_account",
            json!({"path": "film.mthd", "account": "21-01", "offset": 1, "limit": 1}),
        )
        .unwrap();
    assert_eq!(page["rows"][0]["n"], 2);
    err_contains(
        f.tool(
            "get_account",
            json!({"path": "film.mthd", "account": "99-99"}),
        ),
        "no account 99-99",
    );

    let g = f
        .tool("list_globals", json!({"path": "film.mthd"}))
        .unwrap();
    let weeks = g["globals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "SHOOT_WEEKS")
        .unwrap()
        .clone();
    assert_eq!(weeks["value"], "5");
    assert!(weeks["lines"].as_u64().unwrap() > 0);
    let fr = f
        .tool("list_setup", json!({"path": "film.mthd", "type": "fringe"}))
        .unwrap();
    assert_eq!(fr["items"][0]["name"], "PAYROLL");
    assert_eq!(fr["items"][0]["fringe_kind"], "percent");
    let cur = f
        .tool(
            "list_setup",
            json!({"path": "film.mthd", "type": "currency"}),
        )
        .unwrap();
    assert_eq!(cur["items"][0]["base"], true);
    let ch = f
        .tool("list_charges", json!({"path": "film.mthd"}))
        .unwrap();
    assert_eq!(ch["charges"][0]["name"], "Contingency");
    assert_eq!(ch["charges"][0]["amount_cents"], 5_260_000);

    let t = f.tool("totals", json!({"path": "film.mthd"})).unwrap();
    assert_eq!(t["grand_total_cents"], 57_860_000);
    assert_eq!(t["levels"][0]["name"], "Total above-the-line");
    assert_eq!(t["levels"][0]["total_cents"], t["total_atl_cents"]);

    let ex = f
        .tool("explain", json!({"path": "film.mthd", "total": "21-01"}))
        .unwrap();
    let acct_total = accts["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["code"] == "21-01")
        .unwrap()["total_cents"]
        .clone();
    assert_eq!(ex["reported_cents"], acct_total);
    assert_eq!(ex["tree"]["amount_cents"], acct_total);
    let shallow = f
        .tool(
            "explain",
            json!({"path": "film.mthd", "total": "grand", "depth": 1}),
        )
        .unwrap();
    assert!(shallow["tree"]["more_steps"].as_u64().unwrap() > 0);
    assert!(shallow["tree"].get("children").is_none());
    err_contains(
        f.tool(
            "explain",
            json!({"path": "film.mthd", "total": "nothing-here"}),
        ),
        "doesn't name a total",
    );

    let found = f
        .tool("find", json!({"path": "film.mthd", "query": "camera"}))
        .unwrap();
    let wheres: Vec<&str> = found["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["where"].as_str().unwrap())
        .collect();
    assert!(
        wheres.contains(&"account") && wheres.contains(&"line") && wheres.contains(&"category"),
        "{wheres:?}"
    );
    let few = f
        .tool(
            "find",
            json!({"path": "film.mthd", "query": "SHOOT", "limit": 2}),
        )
        .unwrap();
    assert_eq!(few["matches"].as_array().unwrap().len(), 2);
    assert!(few["total_matches"].as_u64().unwrap() > 2);
    let none = f
        .tool(
            "find",
            json!({"path": "film.mthd", "query": "camera", "case_sensitive": true}),
        )
        .unwrap();
    assert!(none["matches"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["where"] == "line"));

    let v = f.tool("validate", json!({"path": "film.mthd"})).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["warnings"], json!([]));

    std::fs::create_dir_all(f.path("sub/.hidden")).unwrap();
    std::fs::copy(f.path("film.mthd"), f.path("sub/two.mthd")).unwrap();
    std::fs::copy(f.path("film.mthd"), f.path("sub/.hidden/three.mthd")).unwrap();
    let list = f.tool("list_budgets", json!({})).unwrap();
    let paths: Vec<&str> = list["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["film.mthd", "sub/two.mthd"]);
    let flat = f.tool("list_budgets", json!({"recursive": false})).unwrap();
    assert_eq!(flat["files"].as_array().unwrap().len(), 1);
}

#[test]
fn paths_outside_the_roots_are_refused() {
    let mut f = Fixture::new("escape", false);
    let outside =
        std::env::temp_dir().join(format!("method-mcp-outside-{}.mthd", std::process::id()));
    crate::cli::save(outside.to_str().unwrap(), &method_core::starter::budget()).unwrap();
    for p in ["../x.mthd", outside.to_str().unwrap()] {
        let e = f.tool("budget_info", json!({"path": p})).unwrap_err();
        assert!(
            e.contains("outside") || e.contains("no such file"),
            "{p}: {e}"
        );
    }
    err_contains(
        f.tool("budget_info", json!({"path": "missing.mthd"})),
        "no such file",
    );
    std::fs::write(f.path("notes.txt"), "hello").unwrap();
    err_contains(
        f.tool("budget_info", json!({"path": "notes.txt"})),
        "only works with .mthd",
    );
    err_contains(
        f.tool(
            "save_as",
            json!({"path": "film.mthd", "out": "../escaped.mthd"}),
        ),
        "outside",
    );
    err_contains(
        f.tool(
            "export",
            json!({"path": "film.mthd", "format": "xlsx", "out": ".git/x.xlsx"}),
        ),
        "folder doesn't exist",
    );
    std::fs::create_dir_all(f.path(".git")).unwrap();
    err_contains(
        f.tool(
            "export",
            json!({"path": "film.mthd", "format": "xlsx", "out": ".git/x.xlsx"}),
        ),
        "hidden",
    );
    err_contains(
        f.tool(
            "export",
            json!({"path": "film.mthd", "format": "xlsx", "out": "hook.sh"}),
        ),
        "only works with .xlsx",
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, f.path("link.mthd")).unwrap();
        err_contains(
            f.tool("budget_info", json!({"path": "link.mthd"})),
            "outside",
        );
        err_contains(
            f.tool("set_title", json!({"path": "link.mthd", "title": "x"})),
            "outside",
        );
        std::os::unix::fs::symlink(f.path("film.mthd"), f.path("inner.mthd")).unwrap();
        err_contains(
            f.tool(
                "save_as",
                json!({"path": "film.mthd", "out": "inner.mthd", "overwrite": true}),
            ),
            "symbolic link",
        );
    }
    assert_eq!(
        crate::cli::load(outside.to_str().unwrap()).unwrap().title,
        "Untitled feature"
    );
    let _ = std::fs::remove_file(&outside);
}

#[test]
fn line_tools_add_update_tag_move_and_delete() {
    let mut f = Fixture::new("lines", false);
    let before = net(&mut f, "film.mthd");
    let r = f
        .tool(
            "add_lines",
            json!({"path": "film.mthd", "account": "21-01", "after_row": 8, "lines": [
                {"description": "Focus puller", "amount": "SHOOT_WEEKS", "unit": "Week", "rate": 2500, "note": "Union minimum"}
            ]}),
        )
        .unwrap();
    assert_eq!(r["changed"], true);
    assert_eq!(r["added"], json!([{"id": 14, "n": 2}]));
    // 5 weeks x 2,500.00 in category 21; the 10% fringe it copied from the
    // line above posts to the budget's fringe row. The net total takes both,
    // plus the 10% contingency on both.
    let moved = r["totals"]["categories_moved"].as_array().unwrap();
    assert_eq!(moved[0]["code"], "21");
    assert_eq!(
        moved[0]["after_cents"].as_i64().unwrap() - moved[0]["before_cents"].as_i64().unwrap(),
        1_250_000
    );
    assert_eq!(net(&mut f, "film.mthd") - before, 1_512_500);
    let cam = rows(&mut f, "21-01");
    assert_eq!(cam[1]["fringes"], json!(["PAYROLL"]));
    assert_eq!(cam[1]["notes"][0]["text"], "Union minimum");
    assert!(cam[1]["notes"][0]["created"]
        .as_str()
        .unwrap()
        .ends_with('Z'));

    let r = f
        .tool(
            "add_lines",
            json!({"path": "film.mthd", "account": "21-01", "at_start": true, "use_defaults": false, "lines": [
                {"kind": "subtotal", "description": "Crew"}, {"description": "Loader", "amount": 1, "unit": "Flat", "rate": "500"}
            ]}),
        )
        .unwrap();
    assert_eq!(r["added"], json!([{"id": 15, "n": 1}, {"id": 16, "n": 2}]));
    assert_eq!(rows(&mut f, "21-01")[1]["fringes"], json!([]));

    let file_before = std::fs::read(f.path("film.mthd")).unwrap();
    err_contains(
        f.tool(
            "add_lines",
            json!({"path": "film.mthd", "account": "21-01", "lines": [{"unit": "Fortnight"}]}),
        ),
        "there is no unit Fortnight",
    );
    err_contains(
        f.tool(
            "add_lines",
            json!({"path": "film.mthd", "account": "88-01", "lines": [{}]}),
        ),
        "no account 88-01",
    );
    err_contains(
        f.tool(
            "add_lines",
            json!({"path": "film.mthd", "account": "21-01", "after_row": 1, "lines": [{}]}),
        ),
        "not in account 21-01",
    );
    err_contains(
        f.tool("add_lines", json!({"path": "film.mthd", "account": "21-01", "lines": [{"kind": "subtotal", "rate": 5}]})),
        "has no rate",
    );
    err_contains(
        f.tool(
            "add_lines",
            json!({"path": "film.mthd", "account": "21-01", "lines": []}),
        ),
        "at least 1",
    );
    assert_eq!(
        std::fs::read(f.path("film.mthd")).unwrap(),
        file_before,
        "refused edits write nothing"
    );

    let r = f
        .tool(
            "update_lines",
            json!({"path": "film.mthd", "rows": [
                {"id": 14, "rate": "3000", "fringes": [], "add_note": "Raised"},
                {"id": 16, "unit": null, "bold": true, "color": "#AA0000", "fringe_range": 2}
            ]}),
        )
        .unwrap();
    assert_eq!(r["changed"], true);
    let cam = rows(&mut f, "21-01");
    let focus = cam.iter().find(|x| x["id"] == 14).unwrap();
    assert_eq!(focus["rate"], "3000");
    assert_eq!(focus["fringes"], json!([]));
    assert_eq!(focus["notes"].as_array().unwrap().len(), 2);
    let loader = cam.iter().find(|x| x["id"] == 16).unwrap();
    assert_eq!(
        (
            loader["unit"].clone(),
            loader["bold"].clone(),
            loader["color"].clone()
        ),
        (json!(""), json!(true), json!("#aa0000"))
    );
    err_contains(
        f.tool(
            "update_lines",
            json!({"path": "film.mthd", "rows": [{"id": 999, "rate": 1}]}),
        ),
        "no row #999",
    );
    err_contains(
        f.tool(
            "update_lines",
            json!({"path": "film.mthd", "rows": [{"id": 14, "color": "red"}]}),
        ),
        "not a color",
    );
    err_contains(
        f.tool(
            "update_lines",
            json!({"path": "film.mthd", "rows": [{"id": 14, "fringes": ["NOPE"]}]}),
        ),
        "no fringe NOPE",
    );
    let same = f
        .tool(
            "update_lines",
            json!({"path": "film.mthd", "rows": [{"id": 14, "rate": "3000"}]}),
        )
        .unwrap();
    assert_eq!(same["changed"], false);

    let r = f
        .tool(
            "tag_lines",
            json!({"path": "film.mthd", "account": "22-01", "add_fringes": ["PAYROLL"]}),
        )
        .unwrap();
    assert_eq!(r["rows_changed"], 1);
    assert_eq!(rows(&mut f, "22-01")[0]["fringes"], json!(["PAYROLL"]));
    let r = f
        .tool(
            "tag_lines",
            json!({"path": "film.mthd", "rows": [10, 15], "remove_fringes": ["PAYROLL"]}),
        )
        .unwrap();
    assert_eq!(
        (r["rows_changed"].clone(), r["rows_skipped"].clone()),
        (json!(1), json!(1))
    );
    err_contains(
        f.tool(
            "tag_lines",
            json!({"path": "film.mthd", "rows": [10], "add_groups": ["Night"]}),
        ),
        "no group Night",
    );
    err_contains(
        f.tool(
            "tag_lines",
            json!({"path": "film.mthd", "add_groups": ["Night"]}),
        ),
        "give rows",
    );
    err_contains(
        f.tool("tag_lines", json!({"path": "film.mthd", "rows": [10]})),
        "fringes or groups",
    );

    let r = f
        .tool(
            "move_lines",
            json!({"path": "film.mthd", "rows": [14, 16], "to_account": "20-01", "at_start": true}),
        )
        .unwrap();
    assert_eq!(r["moved"], 2);
    let staff = rows(&mut f, "20-01");
    assert_eq!(
        (staff[0]["id"].clone(), staff[1]["id"].clone()),
        (json!(14), json!(16))
    );
    let r = f
        .tool(
            "move_lines",
            json!({"path": "film.mthd", "rows": [16], "to_account": "20-01", "after_row": 7}),
        )
        .unwrap();
    assert_eq!(r["moved"], 1);
    assert_eq!(rows(&mut f, "20-01").last().unwrap()["id"], 16);
    err_contains(
        f.tool(
            "move_lines",
            json!({"path": "film.mthd", "rows": [14], "to_account": "20-01", "after_row": 14}),
        ),
        "can't be one of the rows",
    );
    err_contains(
        f.tool(
            "move_lines",
            json!({"path": "film.mthd", "rows": [14], "to_account": "99-01"}),
        ),
        "no account 99-01",
    );

    let r = f
        .tool(
            "delete_lines",
            json!({"path": "film.mthd", "rows": [14, 15, 16]}),
        )
        .unwrap();
    assert_eq!(r["deleted"], 3);
    err_contains(
        f.tool(
            "delete_lines",
            json!({"path": "film.mthd", "rows": [1, 999]}),
        ),
        "no such row(s): #999",
    );
    assert_eq!(
        rows(&mut f, "10-01").len(),
        1,
        "a refused delete deletes nothing"
    );
}

#[test]
fn globals_and_setup_tables() {
    let mut f = Fixture::new("setup", false);
    let r = f
        .tool(
            "set_global",
            json!({"path": "film.mthd", "name": "SHOOT_DAYS", "formula": 30}),
        )
        .unwrap();
    assert_eq!(r["value"], "30");
    assert!(
        r["totals"]["after"]["net_total_cents"].as_i64()
            > r["totals"]["before"]["net_total_cents"].as_i64()
    );
    let r = f
        .tool("set_global", json!({"path": "film.mthd", "name": "OT_FACTOR", "formula": "1.5", "description": "Overtime", "unit": "Hour"}))
        .unwrap();
    assert_eq!(r["value"], "1.5");
    let g = f
        .tool("list_globals", json!({"path": "film.mthd"}))
        .unwrap();
    let ot = g["globals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "OT_FACTOR")
        .unwrap()
        .clone();
    assert_eq!(
        (ot["description"].clone(), ot["unit"].clone()),
        (json!("Overtime"), json!("Hour"))
    );
    err_contains(
        f.tool(
            "set_global",
            json!({"path": "film.mthd", "name": "bad name", "formula": 1}),
        ),
        "can't be a global name",
    );
    err_contains(
        f.tool(
            "set_global",
            json!({"path": "film.mthd", "name": "SHOOT_DAYS", "formula": "NOPE * 2"}),
        ),
        "wouldn't add up",
    );
    err_contains(
        f.tool(
            "set_global",
            json!({"path": "film.mthd", "name": "X", "formula": ""}),
        ),
        "needs a value",
    );

    let r = f
        .tool("add_setup", json!({"path": "film.mthd", "type": "fringe", "name": "FICA", "fields": {"rate": "7.65", "cutoff": "168,600.00", "short_code": "FI"}}))
        .unwrap();
    assert_eq!(r["changed"], true);
    let fr = f
        .tool("list_setup", json!({"path": "film.mthd", "type": "fringe"}))
        .unwrap();
    let fica = fr["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "FICA")
        .unwrap()
        .clone();
    assert_eq!(
        (fica["cutoff_cents"].clone(), fica["short_code"].clone()),
        (json!(16_860_000), json!("FI"))
    );
    f.tool("update_setup", json!({"path": "film.mthd", "type": "fringe", "name": "FICA", "fields": {"rate": 6.2, "cutoff": null}})).unwrap();
    let fr = f
        .tool("list_setup", json!({"path": "film.mthd", "type": "fringe"}))
        .unwrap();
    let fica = fr["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "FICA")
        .unwrap()
        .clone();
    assert_eq!(
        (fica["rate"].clone(), fica["cutoff_cents"].clone()),
        (json!("6.2"), Value::Null)
    );
    f.tool("update_setup", json!({"path": "film.mthd", "type": "fringe", "name": "FICA", "fields": {"fringe_kind": "flat", "unit": "Day"}})).unwrap();
    // How a flat fringe counts: its basis and most hours a day.
    f.tool("add_setup", json!({"path": "film.mthd", "type": "fringe", "name": "PENHR", "fields": {"fringe_kind": "flat", "rate": "1.5", "unit": "Hour", "basis": "amount_counts", "max_hours_per_day": "12"}})).unwrap();
    let pen = |f: &mut Fixture| {
        f.tool("list_setup", json!({"path": "film.mthd", "type": "fringe"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == "PENHR")
            .unwrap()
            .clone()
    };
    let p = pen(&mut f);
    assert_eq!(
        (
            p["basis"].clone(),
            p["max_hours_per_day"].clone(),
            p["unit"].clone()
        ),
        (json!("amount_counts"), json!("12"), json!("Hour"))
    );
    // Other fields leave them alone; null puts each back to its default.
    f.tool(
        "update_setup",
        json!({"path": "film.mthd", "type": "fringe", "name": "PENHR", "fields": {"rate": 2}}),
    )
    .unwrap();
    assert_eq!(pen(&mut f)["basis"], json!("amount_counts"));
    f.tool("update_setup", json!({"path": "film.mthd", "type": "fringe", "name": "PENHR", "fields": {"basis": null, "max_hours_per_day": null}})).unwrap();
    let p = pen(&mut f);
    assert_eq!(
        (p["basis"].clone(), p["max_hours_per_day"].clone()),
        (json!("amount"), Value::Null)
    );
    let fields =
        |v: Value| json!({"path": "film.mthd", "type": "fringe", "name": "PENHR", "fields": v});
    err_contains(
        f.tool("update_setup", fields(json!({"basis": "hours"}))),
        "must be one of",
    );
    err_contains(
        f.tool("update_setup", fields(json!({"max_hours_per_day": 0}))),
        "more than 0",
    );
    err_contains(
        f.tool("update_setup", fields(json!({"cutoff": "100"}))),
        "cutoffs apply to percent fringes only",
    );
    err_contains(
        f.tool(
            "update_setup",
            fields(json!({"fringe_kind": "percent", "basis": "quantity"})),
        ),
        "a percent fringe has no basis",
    );
    // A percent fringe's cutoff is kept when it turns flat, and back.
    f.tool("add_setup", json!({"path": "film.mthd", "type": "fringe", "name": "CAPPED", "fields": {"rate": "5", "cutoff": "1000"}})).unwrap();
    f.tool("update_setup", json!({"path": "film.mthd", "type": "fringe", "name": "CAPPED", "fields": {"fringe_kind": "flat"}})).unwrap();
    let list = f
        .tool("list_setup", json!({"path": "film.mthd", "type": "fringe"}))
        .unwrap();
    let capped = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "CAPPED")
        .unwrap()
        .clone();
    assert_eq!(capped["unused_cutoff_cents"], json!(100_000));
    f.tool("update_setup", json!({"path": "film.mthd", "type": "fringe", "name": "CAPPED", "fields": {"fringe_kind": "percent"}})).unwrap();
    let list = f
        .tool("list_setup", json!({"path": "film.mthd", "type": "fringe"}))
        .unwrap();
    let capped = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "CAPPED")
        .unwrap()
        .clone();
    assert_eq!(capped["cutoff_cents"], json!(100_000));
    err_contains(
        f.tool(
            "add_setup",
            json!({"path": "film.mthd", "type": "fringe", "name": "FICA", "fields": {"rate": 1}}),
        ),
        "already a fringe",
    );
    err_contains(f.tool("add_setup", json!({"path": "film.mthd", "type": "fringe", "name": "X", "fields": {"in_budget_total": true}})), "no field `in_budget_total`");
    err_contains(
        f.tool(
            "add_setup",
            json!({"path": "film.mthd", "type": "fringe", "name": "Y", "fields": {}}),
        ),
        "needs a rate",
    );
    err_contains(
        f.tool(
            "add_setup",
            json!({"path": "film.mthd", "type": "fringe", "name": "A,B", "fields": {"rate": 1}}),
        ),
        "can't be a fringe name",
    );
    err_contains(
        f.tool(
            "update_setup",
            json!({"path": "film.mthd", "type": "set", "name": "Nope", "fields": {}}),
        ),
        "no set Nope",
    );
    f.tool(
        "add_setup",
        json!({"path": "film.mthd", "type": "group", "name": "G", "fields": {"short_code": "GG"}}),
    )
    .unwrap();
    err_contains(f.tool("add_setup", json!({"path": "film.mthd", "type": "group", "name": "H", "fields": {"short_code": "gg"}})), "already the short code");

    f.tool("add_setup", json!({"path": "film.mthd", "type": "group", "name": "Night", "fields": {"in_budget_total": false, "color": "#336699"}})).unwrap();
    let before = net(&mut f, "film.mthd");
    f.tool(
        "tag_lines",
        json!({"path": "film.mthd", "rows": [10], "add_groups": ["Night"]}),
    )
    .unwrap();
    assert!(
        net(&mut f, "film.mthd") < before,
        "a group out of the total suppresses its lines"
    );
    f.tool("add_setup", json!({"path": "film.mthd", "type": "set", "name": "Diner", "fields": {"description": "Int. diner"}})).unwrap();
    f.tool("add_setup", json!({"path": "film.mthd", "type": "location", "name": "Stage 4", "fields": {"city": "Burbank", "country": "US"}})).unwrap();
    f.tool("add_setup", json!({"path": "film.mthd", "type": "unit", "name": "Shift", "fields": {"plural": "Shifts", "hours": 12}})).unwrap();
    f.tool("add_setup", json!({"path": "film.mthd", "type": "currency", "name": "EUR", "fields": {"rate": "1.1", "symbol": "E"}})).unwrap();
    f.tool(
        "update_setup",
        json!({"path": "film.mthd", "type": "currency", "name": "EUR", "fields": {"rate": 1.2}}),
    )
    .unwrap();
    err_contains(
        f.tool(
            "add_setup",
            json!({"path": "film.mthd", "type": "currency", "name": "GBP", "fields": {"rate": 0}}),
        ),
        "more than 0",
    );
    err_contains(
        f.tool(
            "add_setup",
            json!({"path": "film.mthd", "type": "currency", "name": "USD", "fields": {"rate": 1}}),
        ),
        "already a currency",
    );
    f.tool("update_lines", json!({"path": "film.mthd", "rows": [{"id": 9, "currency": "EUR", "set": "Diner", "location": "Stage 4", "unit": "Shift"}]})).unwrap();
    let loc = f
        .tool(
            "list_setup",
            json!({"path": "film.mthd", "type": "location"}),
        )
        .unwrap();
    assert_eq!(loc["items"][0]["lines"], 1);

    let r = f.tool("rename_setup", json!({"path": "film.mthd", "type": "fringe", "from": "PAYROLL", "to": "Payroll taxes"})).unwrap();
    assert_eq!(r["references"], 9);
    assert_eq!(
        rows(&mut f, "10-01")[0]["fringes"],
        json!(["Payroll taxes"])
    );
    assert_eq!(
        r["totals"]["before"]["net_total_cents"],
        r["totals"]["after"]["net_total_cents"]
    );
    f.tool(
        "rename_setup",
        json!({"path": "film.mthd", "type": "global", "from": "SHOOT_WEEKS", "to": "WEEKS"}),
    )
    .unwrap();
    assert_eq!(rows(&mut f, "21-01")[0]["amount"], "WEEKS + 1");
    err_contains(
        f.tool(
            "rename_setup",
            json!({"path": "film.mthd", "type": "fringe", "from": "FICA", "to": "Payroll taxes"}),
        ),
        "already a fringe",
    );
    err_contains(
        f.tool(
            "rename_setup",
            json!({"path": "film.mthd", "type": "set", "from": "Nope", "to": "X"}),
        ),
        "no longer exists",
    );

    err_contains(
        f.tool(
            "delete_setup",
            json!({"path": "film.mthd", "type": "fringe", "name": "Payroll taxes"}),
        ),
        "used by",
    );
    let r = f
        .tool(
            "delete_setup",
            json!({"path": "film.mthd", "type": "fringe", "name": "Payroll taxes", "force": true}),
        )
        .unwrap();
    assert_eq!(r["lines_affected"], 9);
    assert_eq!(rows(&mut f, "10-01")[0]["fringes"], json!([]));
    let before = net(&mut f, "film.mthd");
    err_contains(
        f.tool(
            "delete_setup",
            json!({"path": "film.mthd", "type": "global", "name": "WEEKS"}),
        ),
        "frozen",
    );
    f.tool(
        "delete_setup",
        json!({"path": "film.mthd", "type": "global", "name": "WEEKS", "force": true}),
    )
    .unwrap();
    assert_eq!(
        net(&mut f, "film.mthd"),
        before,
        "deleting a global freezes its value"
    );
    assert_eq!(rows(&mut f, "21-01")[0]["amount"], "6 + 1");
    f.tool(
        "delete_setup",
        json!({"path": "film.mthd", "type": "set", "name": "Diner", "force": true}),
    )
    .unwrap();
    f.tool(
        "delete_setup",
        json!({"path": "film.mthd", "type": "unit", "name": "Shift", "force": true}),
    )
    .unwrap();
    f.tool(
        "delete_setup",
        json!({"path": "film.mthd", "type": "currency", "name": "EUR", "force": true}),
    )
    .unwrap();
    assert_eq!(rows(&mut f, "21-01")[1]["currency"], Value::Null);
    err_contains(
        f.tool(
            "delete_setup",
            json!({"path": "film.mthd", "type": "currency", "name": "USD"}),
        ),
        "base currency",
    );
    err_contains(
        f.tool(
            "delete_setup",
            json!({"path": "film.mthd", "type": "location", "name": "Nowhere"}),
        ),
        "no location Nowhere",
    );
    // An unused item goes without force.
    f.tool(
        "delete_setup",
        json!({"path": "film.mthd", "type": "fringe", "name": "FICA"}),
    )
    .unwrap();
}

#[test]
fn charges_credits_categories_accounts_and_title() {
    let mut f = Fixture::new("charges", false);
    let r = f.tool("set_charge", json!({"path": "film.mthd", "name": "Completion bond", "charge_kind": "flat", "amount": "25,000.00"})).unwrap();
    assert_eq!(r["added"], true);
    let ch = f
        .tool("list_charges", json!({"path": "film.mthd"}))
        .unwrap();
    let bond = ch["charges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Completion bond")
        .unwrap()
        .clone();
    assert_eq!(
        (
            bond["flat_amount_cents"].clone(),
            bond["amount_cents"].clone()
        ),
        (json!(2_500_000), json!(2_500_000))
    );
    let r = f.tool("set_charge", json!({"path": "film.mthd", "name": "Contingency", "percent": 5, "excluded_categories": ["50"]})).unwrap();
    assert_eq!(r["added"], false);
    let before = net(&mut f, "film.mthd");
    f.tool(
        "apply_charge",
        json!({"path": "film.mthd", "type": "charge", "name": "Completion bond", "applied": false}),
    )
    .unwrap();
    assert_eq!(before - net(&mut f, "film.mthd"), 2_500_000);
    err_contains(
        f.tool("set_charge", json!({"path": "film.mthd", "name": "New"})),
        "needs charge_kind",
    );
    err_contains(f.tool("set_charge", json!({"path": "film.mthd", "name": "New", "charge_kind": "percent", "percent": 3, "excluded_groups": ["Nope"]})), "no group Nope");
    err_contains(
        f.tool(
            "set_charge",
            json!({"path": "film.mthd", "name": "New", "charge_kind": "flat", "amount": "(1.005)"}),
        ),
        // Money rounds as the app's does (1.005 is 1.01); a negative amount
        // is refused (owner's decision, 2026-10-15).
        "can't be negative",
    );
    err_contains(
        f.tool(
            "apply_charge",
            json!({"path": "film.mthd", "type": "charge", "name": "Nope", "applied": true}),
        ),
        "no charge Nope",
    );
    f.tool(
        "delete_charge",
        json!({"path": "film.mthd", "type": "charge", "name": "Completion bond"}),
    )
    .unwrap();
    err_contains(
        f.tool(
            "delete_charge",
            json!({"path": "film.mthd", "type": "charge", "name": "Completion bond"}),
        ),
        "no charge",
    );

    f.tool(
        "add_setup",
        json!({"path": "film.mthd", "type": "group", "name": "Local", "fields": {}}),
    )
    .unwrap();
    f.tool(
        "tag_lines",
        json!({"path": "film.mthd", "account": "21-01", "add_groups": ["Local"]}),
    )
    .unwrap();
    let r = f
        .tool("set_credit", json!({"path": "film.mthd", "name": "State incentive", "discount_percent": 0, "rows": [
            {"description": "Camera spend", "source": {"kind": "group", "group": "Local"}, "percent": 25},
            {"source": {"kind": "flat", "amount": "1000"}, "percent": "10", "cap": {"kind": "credit", "amount": 50}}
        ]}))
        .unwrap();
    assert_eq!(r["added"], true);
    let cr = f
        .tool("list_charges", json!({"path": "film.mthd"}))
        .unwrap();
    let credit = &cr["credits"][0];
    assert_eq!(credit["rows"].as_array().unwrap().len(), 2);
    assert!(credit["amount_cents"].as_i64().unwrap() > 0);
    err_contains(
        f.tool(
            "delete_setup",
            json!({"path": "film.mthd", "type": "group", "name": "Local", "force": true}),
        ),
        "used by the credit",
    );
    f.tool(
        "apply_charge",
        json!({"path": "film.mthd", "type": "credit", "name": "State incentive", "applied": false}),
    )
    .unwrap();
    err_contains(
        f.tool("set_credit", json!({"path": "film.mthd", "name": "Bad", "rows": [{"source": {"kind": "group", "group": "Nope"}, "percent": 1}]})),
        "no group",
    );
    // A row on a topsheet category (decisions.md, entry 57).
    f.tool(
        "set_credit",
        json!({"path": "film.mthd", "name": "Camera credit", "rows": [
            {"source": {"kind": "category", "category": "21"}, "percent": 10}
        ]}),
    )
    .unwrap();
    let cr = f
        .tool("list_charges", json!({"path": "film.mthd"}))
        .unwrap();
    let credit = cr["credits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Camera credit")
        .unwrap();
    assert_eq!(
        credit["rows"][0]["source"],
        json!({"kind": "category", "category": "21"})
    );
    assert!(credit["amount_cents"].as_i64().unwrap() > 0);
    // One excluded-fringe list per credit, kept when the rows change.
    let full = credit["amount_cents"].as_i64().unwrap();
    f.tool(
        "set_credit",
        json!({"path": "film.mthd", "name": "Camera credit", "excluded_fringes": ["PAYROLL"]}),
    )
    .unwrap();
    f.tool(
        "set_credit",
        json!({"path": "film.mthd", "name": "Camera credit", "rows": [
            {"source": {"kind": "category", "category": "21"}, "percent": 10}
        ]}),
    )
    .unwrap();
    let cr = f
        .tool("list_charges", json!({"path": "film.mthd"}))
        .unwrap();
    let credit = cr["credits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Camera credit")
        .unwrap();
    assert_eq!(credit["excluded_fringes"], json!(["PAYROLL"]));
    assert!(credit["amount_cents"].as_i64().unwrap() < full);
    err_contains(
        f.tool(
            "set_credit",
            json!({"path": "film.mthd", "name": "Camera credit", "excluded_fringes": ["NOPE"]}),
        ),
        "no fringe NOPE",
    );
    err_contains(
        f.tool("set_credit", json!({"path": "film.mthd", "name": "Bad", "rows": [{"source": {"kind": "category", "category": "99"}, "percent": 1}]})),
        "no category",
    );
    // A row on an account (decisions.md, entry 61).
    f.tool(
        "set_credit",
        json!({"path": "film.mthd", "name": "Camera account", "rows": [
            {"source": {"kind": "account", "account": "21-01"}, "percent": 10}
        ]}),
    )
    .unwrap();
    let cr = f
        .tool("list_charges", json!({"path": "film.mthd"}))
        .unwrap();
    let credit = cr["credits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Camera account")
        .unwrap();
    assert_eq!(
        credit["rows"][0]["source"],
        json!({"kind": "account", "account": "21-01"})
    );
    assert!(credit["amount_cents"].as_i64().unwrap() > 0);
    err_contains(
        f.tool("set_credit", json!({"path": "film.mthd", "name": "Bad", "rows": [{"source": {"kind": "account", "account": "99-99"}, "percent": 1}]})),
        "no account",
    );
    f.tool(
        "delete_charge",
        json!({"path": "film.mthd", "type": "credit", "name": "Camera account"}),
    )
    .unwrap();
    f.tool(
        "delete_charge",
        json!({"path": "film.mthd", "type": "credit", "name": "Camera credit"}),
    )
    .unwrap();
    f.tool(
        "delete_charge",
        json!({"path": "film.mthd", "type": "credit", "name": "State incentive"}),
    )
    .unwrap();

    let r = f.tool("add_category", json!({"path": "film.mthd", "code": "3", "name": "Art", "before_total": "Total production"})).unwrap();
    assert_eq!(r["code"], "03");
    let top = f
        .tool("list_topsheet", json!({"path": "film.mthd"}))
        .unwrap();
    let pos = top["rows"]
        .as_array()
        .unwrap()
        .iter()
        .position(|x| x["code"] == "03")
        .unwrap();
    assert_eq!(top["rows"][pos + 1]["name"], "Total production");
    let r = f
        .tool(
            "add_account",
            json!({"path": "film.mthd", "category": "03", "code": "3-1", "name": "Art department"}),
        )
        .unwrap();
    assert_eq!(r["code"], "03-01");
    f.tool("add_lines", json!({"path": "film.mthd", "account": "03-01", "lines": [{"description": "Designer", "amount": 10, "unit": "Week", "rate": 3000}]})).unwrap();
    f.tool(
        "update_account",
        json!({"path": "film.mthd", "code": "03-01", "name": "Art", "new_code": "03-02"}),
    )
    .unwrap();
    f.tool(
        "update_category",
        json!({"path": "film.mthd", "code": "03", "name": "Art and sets"}),
    )
    .unwrap();
    err_contains(
        f.tool(
            "add_account",
            json!({"path": "film.mthd", "category": "03", "code": "21-01", "name": "Dup"}),
        ),
        "already exists",
    );
    err_contains(
        f.tool(
            "add_category",
            json!({"path": "film.mthd", "code": "21", "name": "Dup"}),
        ),
        "already exists",
    );
    err_contains(
        f.tool(
            "delete_account",
            json!({"path": "film.mthd", "code": "03-02"}),
        ),
        "has 1 row(s)",
    );
    err_contains(
        f.tool(
            "delete_category",
            json!({"path": "film.mthd", "code": "03"}),
        ),
        "force: true",
    );
    f.tool(
        "delete_category",
        json!({"path": "film.mthd", "code": "03", "force": true}),
    )
    .unwrap();
    err_contains(
        f.tool(
            "delete_account",
            json!({"path": "film.mthd", "code": "03-02"}),
        ),
        "no account",
    );

    f.tool(
        "set_title",
        json!({"path": "film.mthd", "title": "  The Big One  "}),
    )
    .unwrap();
    assert_eq!(
        f.tool("budget_info", json!({"path": "film.mthd"})).unwrap()["title"],
        "The Big One"
    );
}

#[test]
fn conflicts_shared_copies_and_undo() {
    let mut f = Fixture::new("conflict", false);
    let path = f.path("film.mthd");
    f.tool("budget_open", json!({"path": "film.mthd"})).unwrap();
    f.tool("set_title", json!({"path": "film.mthd", "title": "Mine"}))
        .unwrap();
    // Someone else saves the file.
    let mut theirs = crate::cli::load(path.to_str().unwrap()).unwrap();
    theirs.title = "Theirs".into();
    crate::cli::save(path.to_str().unwrap(), &theirs).unwrap();
    let info = f.tool("budget_info", json!({"path": "film.mthd"})).unwrap();
    assert_eq!(
        (info["title"].clone(), info["changed_on_disk"].clone()),
        (json!("Theirs"), json!(true))
    );
    let d = f
        .tool("diff", json!({"path": "film.mthd", "since": "disk"}))
        .unwrap();
    assert_eq!(d["changed"], true);
    assert!(
        d["changes"][0]["text"].as_str().unwrap().contains("Theirs"),
        "{d}"
    );
    err_contains(
        f.tool(
            "update_lines",
            json!({"path": "film.mthd", "rows": [{"id": 1, "rate": 1}]}),
        ),
        "changed on disk",
    );
    err_contains(
        f.tool("undo", json!({"path": "film.mthd"})),
        "changed on disk",
    );
    assert_eq!(
        crate::cli::load(path.to_str().unwrap()).unwrap().title,
        "Theirs"
    );
    let open = f.tool("budget_open", json!({"path": "film.mthd"})).unwrap();
    assert_eq!(
        (open["reloaded"].clone(), open["undo_dropped"].clone()),
        (json!(true), json!(1))
    );
    f.tool(
        "update_lines",
        json!({"path": "film.mthd", "rows": [{"id": 1, "rate": 60000}]}),
    )
    .unwrap();
    f.tool("set_title", json!({"path": "film.mthd", "title": "Again"}))
        .unwrap();
    let info = f.tool("budget_info", json!({"path": "film.mthd"})).unwrap();
    assert_eq!(info["undo"], json!(["Set title", "Edit 1 row(s)"]));
    let d = f.tool("diff", json!({"path": "film.mthd"})).unwrap();
    assert_eq!(d["change_count"], 2, "{d}");
    let u = f.tool("undo", json!({"path": "film.mthd"})).unwrap();
    assert_eq!(u["undone"], "Set title");
    assert_eq!(
        crate::cli::load(path.to_str().unwrap()).unwrap().title,
        "Theirs"
    );
    f.tool("undo", json!({"path": "film.mthd"})).unwrap();
    err_contains(
        f.tool("undo", json!({"path": "film.mthd"})),
        "nothing to undo",
    );
    assert_eq!(rows(&mut f, "10-01")[0]["rate"], "50000");

    // A shared copy is read-only; an editable copy of it isn't.
    let shared = method_core::share::share_copy(
        &method_core::starter::budget(),
        None,
        &method_core::share::ShareOptions::default(),
        "2026-10-10T12:00:00Z",
        "0",
        None,
    )
    .unwrap();
    crate::cli::save(f.path("shared.mthd").to_str().unwrap(), &shared).unwrap();
    assert_eq!(
        f.tool("budget_info", json!({"path": "shared.mthd"}))
            .unwrap()["read_only"],
        true
    );
    err_contains(
        f.tool("set_title", json!({"path": "shared.mthd", "title": "x"})),
        "read-only",
    );
    f.tool(
        "save_as",
        json!({"path": "shared.mthd", "out": "editable.mthd", "editable": true}),
    )
    .unwrap();
    assert_eq!(
        f.tool("budget_info", json!({"path": "editable.mthd"}))
            .unwrap()["read_only"],
        false
    );
    f.tool(
        "set_title",
        json!({"path": "editable.mthd", "title": "Notes"}),
    )
    .unwrap();
    err_contains(
        f.tool(
            "save_as",
            json!({"path": "film.mthd", "out": "editable.mthd"}),
        ),
        "already exists",
    );
    err_contains(
        f.tool("save_as", json!({"path": "film.mthd", "out": "film.mthd"})),
        "the budget itself",
    );
}

#[test]
fn diff_merge_and_new_budgets() {
    let mut f = Fixture::new("merge", false);
    f.tool("save_as", json!({"path": "film.mthd", "out": "base.mthd"}))
        .unwrap();
    f.tool("save_as", json!({"path": "film.mthd", "out": "ours.mthd"}))
        .unwrap();
    f.tool(
        "save_as",
        json!({"path": "film.mthd", "out": "theirs.mthd"}),
    )
    .unwrap();
    f.tool("set_title", json!({"path": "ours.mthd", "title": "Ours"}))
        .unwrap();
    f.tool(
        "update_lines",
        json!({"path": "theirs.mthd", "rows": [{"id": 8, "rate": 5000}]}),
    )
    .unwrap();
    let d = f
        .tool(
            "diff",
            json!({"path": "theirs.mthd", "against": "base.mthd"}),
        )
        .unwrap();
    assert_eq!(d["change_count"], 1);
    assert!(d["totals"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["delta_cents"].as_i64().unwrap() > 0));
    let dry = f.tool("merge", json!({"base": "base.mthd", "ours": "ours.mthd", "theirs": "theirs.mthd", "out": "merged.mthd", "dry_run": true})).unwrap();
    assert_eq!(dry["written"], false);
    assert!(!f.path("merged.mthd").exists());
    let m = f.tool("merge", json!({"base": "base.mthd", "ours": "ours.mthd", "theirs": "theirs.mthd", "out": "merged.mthd"})).unwrap();
    assert_eq!(m["conflicts"], json!([]));
    let merged = crate::cli::load(f.path("merged.mthd").to_str().unwrap()).unwrap();
    assert_eq!(merged.title, "Ours");
    f.tool(
        "update_lines",
        json!({"path": "ours.mthd", "rows": [{"id": 8, "rate": 4800}]}),
    )
    .unwrap();
    let m = f
        .tool("merge", json!({"base": "base.mthd", "ours": "ours.mthd", "theirs": "theirs.mthd", "out": "merged.mthd", "prefer": "theirs", "overwrite": true}))
        .unwrap();
    assert_eq!(m["conflicts"].as_array().unwrap().len(), 1);
    assert_eq!(m["conflicts"][0]["taken"], "theirs");
    err_contains(f.tool("merge", json!({"base": "base.mthd", "ours": "ours.mthd", "theirs": "theirs.mthd", "out": "merged.mthd"})), "already exists");
    err_contains(f.tool("merge", json!({"base": "base.mthd", "ours": "ours.mthd", "theirs": "theirs.mthd", "out": "ours.mthd"})), "one of the budgets");

    let n = f
        .tool(
            "new_budget",
            json!({"path": "blank.mthd", "from": "blank", "title": "Short", "currency": "EUR"}),
        )
        .unwrap();
    assert_eq!(n["title"], "Short");
    let info = f
        .tool("budget_info", json!({"path": "blank.mthd"}))
        .unwrap();
    assert_eq!(
        (
            info["base_currency"].clone(),
            info["counts"]["categories"].clone()
        ),
        (json!("EUR"), json!(0))
    );
    f.tool("new_budget", json!({"path": "starter.mthd"}))
        .unwrap();
    assert_eq!(
        f.tool("budget_info", json!({"path": "starter.mthd"}))
            .unwrap()["counts"]["lines"],
        13
    );
    f.tool(
        "new_budget",
        json!({"path": "from-template.mthd", "template": "theirs.mthd", "title": "Next show"}),
    )
    .unwrap();
    let t = f
        .tool("budget_info", json!({"path": "from-template.mthd"}))
        .unwrap();
    assert_eq!(
        (
            t["counts"]["accounts"].clone(),
            t["counts"]["lines"].clone()
        ),
        (json!(10), json!(0))
    );
    f.tool(
        "new_budget",
        json!({"path": "kept.mthd", "template": "theirs.mthd", "keep_lines": true}),
    )
    .unwrap();
    assert_eq!(
        f.tool("budget_info", json!({"path": "kept.mthd"})).unwrap()["counts"]["lines"],
        13
    );
    err_contains(
        f.tool("new_budget", json!({"path": "starter.mthd"})),
        "already exists",
    );
    f.tool(
        "new_budget",
        json!({"path": "starter.mthd", "from": "blank", "overwrite": true}),
    )
    .unwrap();
    // Replacing a file is an undoable change of it.
    f.tool("undo", json!({"path": "starter.mthd"})).unwrap();
    assert_eq!(
        f.tool("budget_info", json!({"path": "starter.mthd"}))
            .unwrap()["counts"]["lines"],
        13
    );
}

#[test]
fn export_and_import() {
    let mut f = Fixture::new("export", false);
    let x = f
        .tool(
            "export",
            json!({"path": "film.mthd", "format": "xlsx", "out": "film.xlsx"}),
        )
        .unwrap();
    assert_eq!(x["files"].as_array().unwrap().len(), 1);
    method_format::export::xlsx::verify(&std::fs::read(f.path("film.xlsx")).unwrap(), 10).unwrap();
    let c = f
        .tool(
            "export",
            json!({"path": "film.mthd", "format": "csv", "out": "Film.csv"}),
        )
        .unwrap();
    assert_eq!(c["files"].as_array().unwrap().len(), 10);
    assert!(f.path("Film - Topsheet.csv").exists());
    f.tool(
        "export",
        json!({"path": "film.mthd", "format": "tsv", "out": "detail.txt"}),
    )
    .unwrap();
    f.tool("export", json!({"path": "film.mthd", "format": "html", "out": "report.html", "date": "2026-10-09 14:30"})).unwrap();
    let html = std::fs::read_to_string(f.path("report.html")).unwrap();
    assert!(html.contains("Untitled feature") && html.contains("2026-10-09"));
    f.tool("export", json!({"path": "film.mthd", "format": "pdf", "out": "report.pdf", "sections": ["topsheet"]})).unwrap();
    assert!(std::fs::read(f.path("report.pdf"))
        .unwrap()
        .starts_with(b"%PDF"));
    f.tool(
        "export",
        json!({"path": "film.mthd", "format": "report_csv", "out": "report.csv"}),
    )
    .unwrap();
    err_contains(
        f.tool(
            "export",
            json!({"path": "film.mthd", "format": "xlsx", "out": "film.xlsx"}),
        ),
        "already exists",
    );
    f.tool(
        "export",
        json!({"path": "film.mthd", "format": "xlsx", "out": "film.xlsx", "overwrite": true}),
    )
    .unwrap();
    err_contains(
        f.tool(
            "export",
            json!({"path": "film.mthd", "format": "pdf", "out": "report.html"}),
        ),
        "only works with .pdf",
    );
    err_contains(
        f.tool(
            "export",
            json!({"path": "film.mthd", "format": "html", "out": "r2.html", "preset": "Nope"}),
        ),
        "no report preset",
    );

    std::fs::write(f.path("camera.csv"), "Account,Description,Amount,Unit,Rate\n21-01,Steadicam,3,Week,4000\n21-01,Drone,2,Day,1500\n").unwrap();
    let mapping = json!({"header_row": 1, "columns": {"account_code": "A", "description": "B", "amount": "C", "unit": "D", "rate": "E"}});
    let dry = f.tool("import_sheet", json!({"sheet": "camera.csv", "into": "film.mthd", "out": "imported.mthd", "mapping": mapping, "dry_run": true})).unwrap();
    assert_eq!(
        (dry["lines"].clone(), dry["written"].clone()),
        (json!(2), json!(false))
    );
    assert!(!f.path("imported.mthd").exists());
    let imp = f.tool("import_sheet", json!({"sheet": "camera.csv", "into": "film.mthd", "out": "imported.mthd", "mapping": mapping})).unwrap();
    assert_eq!(imp["written"], true);
    let lines = f
        .tool("budget_info", json!({"path": "imported.mthd"}))
        .unwrap()["counts"]["lines"]
        .clone();
    assert_eq!(lines, 15);
    // Into the budget itself: an undoable change.
    f.tool(
        "import_sheet",
        json!({"sheet": "camera.csv", "into": "film.mthd", "out": "film.mthd", "mapping": mapping}),
    )
    .unwrap();
    assert_eq!(
        f.tool("budget_info", json!({"path": "film.mthd"})).unwrap()["counts"]["lines"],
        15
    );
    f.tool("undo", json!({"path": "film.mthd"})).unwrap();
    assert_eq!(
        f.tool("budget_info", json!({"path": "film.mthd"})).unwrap()["counts"]["lines"],
        13
    );
    err_contains(f.tool("import_sheet", json!({"sheet": "camera.csv", "into": "film.mthd", "out": "imported.mthd", "mapping": mapping})), "already exists");
    err_contains(
        f.tool(
            "import_sheet",
            json!({"sheet": "camera.csv", "out": "new.mthd", "mapping": mapping}),
        ),
        "create_accounts",
    );
    err_contains(f.tool("import_sheet", json!({"sheet": "camera.csv", "out": "new.mthd", "mapping": {"columns": {"rate": "ZZZZ"}}})), "");
    err_contains(
        f.tool(
            "import_sheet",
            json!({"sheet": "film.mthd", "out": "new.mthd"}),
        ),
        "only works with .xlsx",
    );
}

#[test]
fn import_a_budget_export() {
    let mut f = Fixture::new("exchange", false);
    let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../method-format/tests/fixtures/exchange/by-budget-credits.json");
    std::fs::copy(src, f.path("export.json")).unwrap();
    // Only compare: nothing written.
    let r = f
        .tool("import_budget_export", json!({"file": "export.json"}))
        .unwrap();
    assert_eq!(r["format"], "json");
    assert_eq!(r["written"], Value::Null);
    assert_eq!(r["reconciliation"]["matches"], true);
    assert_eq!(r["reconciliation"]["net_cents"], 545_047);
    assert!(r["notes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["kind"] == "not_imported"));
    let w = f
        .tool(
            "import_budget_export",
            json!({"file": "export.json", "out": "imported.mthd", "title": "Credits"}),
        )
        .unwrap();
    assert_eq!(w["written"], "imported.mthd");
    let info = f
        .tool("budget_info", json!({"path": "imported.mthd"}))
        .unwrap();
    assert_eq!(info["counts"]["lines"], 3);
    err_contains(
        f.tool(
            "import_budget_export",
            json!({"file": "export.json", "out": "imported.mthd"}),
        ),
        "already exists",
    );
    std::fs::write(f.path("notes.txt"), "hello").unwrap();
    err_contains(
        f.tool("import_budget_export", json!({"file": "notes.txt"})),
        "budget export",
    );
    err_contains(
        f.tool(
            "import_budget_export",
            json!({"file": "export.json", "format": "pdf"}),
        ),
        "",
    );
}

/// A row's person (decisions.md, entry 79): set and cleared with
/// `update_lines`, shown by `get_account` only when set.
#[test]
fn update_lines_sets_and_clears_a_person() {
    let mut f = Fixture::new("person", false);
    let before = net(&mut f, "film.mthd");
    let r = f
        .tool(
            "update_lines",
            json!({"path": "film.mthd", "rows": [
                {"id": 8, "person": "  Dana Reyes "},
                {"id": 9, "person": "Dana Reyes"}
            ]}),
        )
        .unwrap();
    assert_eq!(r["changed"], true);
    let cam = rows(&mut f, "21-01");
    for id in [8, 9] {
        let row = cam.iter().find(|x| x["id"] == id).unwrap();
        assert_eq!(row["person"], "Dana Reyes");
    }
    // PAYROLL has no cutoff, so nothing moves.
    assert_eq!(net(&mut f, "film.mthd"), before);
    let text = std::fs::read_to_string(f.path("film.mthd")).unwrap();
    assert!(text.contains("\"person\": \"Dana Reyes\""));

    f.tool(
        "update_lines",
        json!({"path": "film.mthd", "rows": [{"id": 8, "person": null}, {"id": 9, "person": null}]}),
    )
    .unwrap();
    let cam = rows(&mut f, "21-01");
    assert!(cam.iter().all(|x| x.get("person").is_none()));
    let text = std::fs::read_to_string(f.path("film.mthd")).unwrap();
    assert!(!text.contains("\"person\""));
}

#[test]
fn globals_from_a_schedules_days() {
    let mut f = Fixture::new("schedule", false);
    let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/field-station/field-station.budget.json");
    std::fs::copy(src, f.path("days.json")).unwrap();
    // A dry run: the review, nothing changed.
    let r = f
        .tool(
            "schedule_days",
            json!({"path": "film.mthd", "file": "days.json", "dry_run": true}),
        )
        .unwrap();
    assert_eq!(r["schedule"], "Field Station");
    assert_eq!(r["applied"], false);
    assert_eq!(r["figures"][0]["global"], "SHOOT_DAYS");
    assert_eq!(r["figures"][0]["current"], "25");
    assert_eq!(r["figures"][0]["action"], "update");
    let before = net(&mut f, "film.mthd");
    // Applied, with a location under a name of our own.
    let r = f
        .tool(
            "schedule_days",
            json!({"path": "film.mthd", "file": "days.json",
                   "pick": [{"key": "location:Cold Spring field station", "global": "STATION_DAYS"}]}),
        )
        .unwrap();
    assert_eq!(r["applied"], true);
    assert_eq!(r["message"], "Set 2 globals from the schedule (1 new).");
    assert_ne!(net(&mut f, "film.mthd"), before);
    let g = f
        .tool("list_globals", json!({"path": "film.mthd"}))
        .unwrap();
    assert!(g.to_string().contains("STATION_DAYS"), "{g}");
    // Undo takes back the whole import.
    let u = f.tool("undo", json!({"path": "film.mthd"})).unwrap();
    assert!(u["undone"]
        .as_str()
        .unwrap()
        .starts_with("Set globals from the schedule"));
    assert_eq!(net(&mut f, "film.mthd"), before);
    err_contains(
        f.tool(
            "schedule_days",
            json!({"path": "film.mthd", "file": "days.json", "pick": [{"key": "set:NOWHERE"}]}),
        ),
        "no figure",
    );
    // Nothing picked: the summary says so, and nothing changes.
    let days = method_session::schedule_days::read(
        "days.json",
        &std::fs::read(f.path("days.json")).unwrap(),
    )
    .unwrap();
    let b = method_core::starter::budget();
    let none = method_session::schedule_days::review(&b, &days, Some(&[]));
    let said = crate::mcp::tools::files::schedule_summary("days.json", &none, None, false);
    assert!(
        said.contains("Nothing is picked; nothing changed."),
        "{said}"
    );
    assert!(!said.contains("already"), "{said}");
    let r = f
        .tool(
            "schedule_days",
            json!({"path": "film.mthd", "file": "days.json", "skip": ["total"]}),
        )
        .unwrap();
    assert_eq!(r["applied"], false);
    assert_eq!(net(&mut f, "film.mthd"), before);
    // A linked figure gone from the file is not set to 0 unattended.
    f.tool(
        "schedule_days",
        json!({"path": "film.mthd", "file": "days.json",
               "pick": [{"key": "location:Cold Spring field station", "global": "STATION_DAYS"}]}),
    )
    .unwrap();
    let moved = std::fs::read_to_string(f.path("days.json"))
        .unwrap()
        .replace("Cold Spring field station", "Cold Spring annex");
    std::fs::write(f.path("moved.json"), moved).unwrap();
    let r = f
        .tool(
            "schedule_days",
            json!({"path": "film.mthd", "file": "moved.json"}),
        )
        .unwrap();
    let gone = r["figures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["key"] == "linked:STATION_DAYS")
        .unwrap()
        .clone();
    assert_eq!(gone["picked"], false);
    assert_eq!(r["applied"], false);
    let text = std::fs::read_to_string(f.path("film.mthd")).unwrap();
    assert!(text.contains("\"STATION_DAYS\": \"4\""), "{text}");
    std::fs::write(f.path("notes.json"), "{}").unwrap();
    err_contains(
        f.tool(
            "schedule_days",
            json!({"path": "film.mthd", "file": "notes.json"}),
        ),
        "isn't a schedule days file",
    );
}

/// A credit limited to a location, and every program's estimate side by
/// side (roadmap 3.6; decisions.md, entry 80).
#[test]
fn incentives_estimate_programs_by_location() {
    let mut f = Fixture::new("incentives", false);
    f.tool(
        "add_setup",
        json!({"path": "film.mthd", "type": "location", "name": "Ridge", "fields": {}}),
    )
    .unwrap();
    f.tool(
        "update_lines",
        json!({"path": "film.mthd", "rows": [{"id": 8, "location": "Ridge"}]}),
    )
    .unwrap();
    let net_before = net(&mut f, "film.mthd");
    f.tool(
        "set_credit",
        json!({"path": "film.mthd", "name": "Ridge program", "applied": false,
               "locations": ["Ridge"],
               "rows": [{"source": {"kind": "account", "account": "21-01"}, "percent": 25}]}),
    )
    .unwrap();
    // Not applied: the net total is as it was.
    assert_eq!(net(&mut f, "film.mthd"), net_before);
    let got = f.tool("incentives", json!({"path": "film.mthd"})).unwrap();
    let p = &got["programs"][0];
    assert_eq!(p["name"], "Ridge program");
    assert_eq!(p["applied"], false);
    assert_eq!(p["locations"], json!(["Ridge"]));
    let q = p["qualified"].as_i64().unwrap();
    assert!(q > 0);
    assert_eq!(p["by_location"], json!([{"location": "Ridge", "net": q}]));
    assert_eq!(p["amount"].as_i64().unwrap(), q / 4);
    let ridge = got["locations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["location"] == "Ridge")
        .unwrap();
    assert_eq!(ridge["total"].as_i64().unwrap(), q);

    let cr = f
        .tool("list_charges", json!({"path": "film.mthd"}))
        .unwrap();
    assert_eq!(cr["credits"][0]["locations"], json!(["Ridge"]));
    err_contains(
        f.tool(
            "set_credit",
            json!({"path": "film.mthd", "name": "Harbor", "locations": ["Harbor"]}),
        ),
        "no location Harbor",
    );
}
