//! File > Import Actuals… and the Actuals tab (src/actuals.rs).

use method_core::actuals::TransactionId;
use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error, FileIn};
use crate::actuals::{self as ac, ActualsSources, EditInput, FilterInput, Imported, Request};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{doc, file}`: the accounting export comes in as `file` (the
        // desktop shows an Open dialog). Its sheets, kept by the reply's
        // token.
        "actuals_pick" => (|| {
            let a: Pick = d.args()?;
            let sources = d.kept::<ActualsSources>();
            json(ac::actuals_pick(&sources, a.file.name, &a.file.bytes)?)
        })(),
        "actuals_preview" => (|| {
            let a: Choices = d.args()?;
            let sources = d.kept::<ActualsSources>();
            let s = d.lock()?;
            json(ac::actuals_preview(
                s.budget(),
                &sources,
                a.token,
                &a.request,
            )?)
        })(),
        "actuals_import" => (|| {
            let a: Import = d.args()?;
            let sources = d.kept::<ActualsSources>();
            let mut s = d.lock()?;
            let message = ac::actuals_import(&mut s, &sources, a.token, &a.request, &a.imported)?;
            json(Imported {
                view: s.finish(None),
                message,
            })
        })(),
        "actuals_list" => (|| {
            let a: List = d.args()?;
            json(ac::actuals_list(d.lock()?.budget(), &a.filter)?)
        })(),
        "actuals_update" => d.view_with(|s, a: Update| ac::actuals_update(s, &a.ids, &a.edit)),
        "actuals_delete" => d.view_with(|s, a: Ids| ac::actuals_delete(s, &a.ids)),
        "actuals_remove_batch" => d.view_with(|s, a: Batch| ac::actuals_remove_batch(s, a.batch)),
        "actuals_set_etc" => {
            d.view_with(|s, a: Etc| ac::actuals_set_etc(s, &a.account, &a.method, &a.value))
        }
        "actuals_lines" => (|| {
            let a: Account = d.args()?;
            json(ac::actuals_lines(d.lock()?.budget(), &a.account))
        })(),
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Pick {
    file: FileIn,
}

#[derive(Deserialize)]
struct Choices {
    token: u64,
    request: Request,
}

#[derive(Deserialize)]
struct Import {
    token: u64,
    request: Request,
    imported: String,
}

#[derive(Deserialize)]
struct List {
    filter: FilterInput,
}

#[derive(Deserialize)]
struct Update {
    ids: Vec<TransactionId>,
    edit: EditInput,
}

#[derive(Deserialize)]
struct Ids {
    ids: Vec<TransactionId>,
}

#[derive(Deserialize)]
struct Batch {
    batch: u64,
}

#[derive(Deserialize)]
struct Etc {
    account: String,
    method: String,
    value: String,
}

#[derive(Deserialize)]
struct Account {
    account: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::test_files::{call, file};
    use crate::Host;

    const LEDGER: &[u8] = b"Date,Account,Vendor,Description,Amount\n9/1/2026,1001,Writer,Draft,\"2,500.00\"\n9/2/2026,1101,Producer,Week 1,1500\n9/3/2026,9999,Mystery,Unknown,50\n";

    #[test]
    fn actuals_are_imported_listed_and_changed_through_json() {
        let h = Host::new();
        let p = call(
            &h,
            "actuals_pick",
            json!({"file": file("ledger.csv", LEDGER)}),
        );
        assert_eq!(p["file"], "ledger.csv");
        let request = json!({
            "sheet": 0,
            "header_row": null,
            "detect_header": true,
            "guess": true,
            "skip_existing": true,
        });
        let pv = call(
            &h,
            "actuals_preview",
            json!({"token": p["token"], "request": request}),
        );
        assert_eq!(pv["count"], 3);
        let done = call(
            &h,
            "actuals_import",
            json!({
                "token": p["token"],
                "request": request,
                "imported": "2026-10-09T14:30:00-07:00",
            }),
        );
        assert!(done["message"]
            .as_str()
            .unwrap()
            .starts_with("Imported 3 transactions"));
        assert!(
            done["view"]["cost"].is_object(),
            "the view has cost figures"
        );

        let list = call(&h, "actuals_list", json!({"filter": {}}));
        assert_eq!(
            (list["count"].clone(), list["unassigned"].clone()),
            (json!(3), json!(1))
        );
        let id = list["transactions"][0]["id"].clone();
        let batch = list["batches"][0]["id"].clone();
        call(
            &h,
            "actuals_update",
            json!({"ids": [id], "edit": {"amount": "12.30"}}),
        );
        let list = call(&h, "actuals_list", json!({"filter": {}}));
        assert_eq!(list["total"], 1_230 + 150_000 + 5_000);
        call(
            &h,
            "actuals_set_etc",
            json!({"account": "1001", "method": "manual", "value": "100"}),
        );
        let lines = call(&h, "actuals_lines", json!({"account": "1001"}));
        assert_eq!(lines[0]["n"], 1);
        call(&h, "actuals_delete", json!({"ids": [id]}));
        assert_eq!(call(&h, "actuals_list", json!({"filter": {}}))["count"], 2);
        let v = call(&h, "actuals_remove_batch", json!({"batch": batch}));
        assert_eq!(v["can_undo"], true);
        assert_eq!(call(&h, "actuals_list", json!({"filter": {}}))["count"], 0);
    }
}
