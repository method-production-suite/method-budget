//! Rate tables (src/rates.rs; decisions.md, entry 90). The page sends
//! figures as strings.

use method_core::RowId;
use method_format::setup::rates::RateMapping;
use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error, FileIn};
use crate::rates::{self as r, RateTableDto};
use crate::sheet_import::SheetSources;

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "rate_table_save" => {
            d.view_with(|s, a: Save| r::rate_table_save(s, &a.table, a.replacing.as_deref()))
        }
        "rate_table_delete" => d.view_with(|s, a: Name| r::rate_table_delete(s, &a.name)),
        "rate_choices" => (|| {
            let a: Choices = d.args()?;
            json(r::choices(d.lock()?.budget(), a.row_id, a.unit.as_deref())?)
        })(),
        "rate_fill" => d.view_with(|s, a: Fill| {
            r::rate_fill(
                s,
                a.row_id,
                &a.table,
                &a.classification,
                a.unit.as_deref(),
                a.as_of.as_deref(),
            )
        }),
        "rate_update" => d.view_with(|s, a: Update| r::rate_update(s, &a.row_ids)),
        // `{doc, file}`: the spreadsheet comes in as `file` (the desktop
        // shows an Open dialog), read as a rate table, kept by the reply's
        // token.
        "rate_import_pick" => (|| {
            let a: Pick = d.args()?;
            let sources = d.kept::<SheetSources>();
            json(r::rate_import_pick(
                &*d.lock()?,
                &sources,
                a.file.name,
                &a.file.bytes,
            )?)
        })(),
        "rate_import_read" => (|| {
            let a: Read = d.args()?;
            let sources = d.kept::<SheetSources>();
            json(r::rate_import_read(
                &*d.lock()?,
                &sources,
                a.token,
                a.index,
                a.mapping,
                a.name.as_deref(),
            )?)
        })(),
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Save {
    table: RateTableDto,
    replacing: Option<String>,
}

#[derive(Deserialize)]
struct Name {
    name: String,
}

#[derive(Deserialize)]
struct Choices {
    row_id: RowId,
    #[serde(default)]
    unit: Option<String>,
}

#[derive(Deserialize)]
struct Fill {
    row_id: RowId,
    table: String,
    classification: String,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    as_of: Option<String>,
}

#[derive(Deserialize)]
struct Update {
    #[serde(default)]
    row_ids: Vec<RowId>,
}

#[derive(Deserialize)]
struct Pick {
    file: FileIn,
}

#[derive(Deserialize)]
struct Read {
    token: u64,
    index: usize,
    #[serde(default)]
    mapping: Option<RateMapping>,
    #[serde(default)]
    name: Option<String>,
}

#[cfg(test)]
mod tests {
    use method_core::RowKind;
    use serde_json::json;

    use crate::Host;

    #[test]
    fn tables_fills_and_imports_as_the_page_sends_them() {
        let h = Host::new();
        let row = crate::edit::details(h.state().lock_doc(None).unwrap().budget())
            .find(|d| d.kind == RowKind::Line)
            .unwrap()
            .id;
        // Invented names and figures.
        let table = json!({"name": "Invented Crew", "rows": [
            {"classification": "Made Up", "rates": {"Day": "410.5"}}
        ]});
        h.dispatch(
            "rate_table_save",
            json!({"table": table, "replacing": null}),
        )
        .unwrap();
        let c = h.dispatch("rate_choices", json!({"row_id": row})).unwrap();
        assert_eq!(c["choices"][0]["classification"], "Made Up");
        h.dispatch(
            "rate_fill",
            json!({"row_id": row, "table": "Invented Crew", "classification": "made up",
                   "unit": "Day"}),
        )
        .unwrap();
        // The account on screen shows how its filled lines stand.
        let (ti, ai, cat, acct) = {
            let s = h.state().lock_doc(None).unwrap();
            let b = s.budget();
            b.topsheet
                .iter()
                .enumerate()
                .find_map(|(ti, t)| match t {
                    method_core::TopsheetRow::Category(c) => c
                        .accounts
                        .iter()
                        .position(|a| a.details.iter().any(|d| d.id == row))
                        .map(|ai| (ti, ai, c.code.clone(), c.accounts[ai].code.clone())),
                    _ => None,
                })
                .unwrap()
        };
        let v = h
            .dispatch(
                "focus_account",
                json!({"category": cat, "ti": ti, "account": acct, "ai": ai}),
            )
            .unwrap();
        assert_eq!(v["rate_checks"][row.to_string()]["state"], "current");
        let filled = crate::edit::details(h.state().lock_doc(None).unwrap().budget())
            .find(|d| d.id == row)
            .unwrap()
            .clone();
        assert_eq!(filled.rate.0, "410.5");
        assert!(filled.rate_source.is_some());
        let mut changed = table.clone();
        changed["rows"][0]["rates"]["Day"] = json!("420");
        h.dispatch(
            "rate_table_save",
            json!({"table": changed, "replacing": "Invented Crew"}),
        )
        .unwrap();
        h.dispatch("rate_update", json!({"row_ids": [row]}))
            .unwrap();
        let file = json!({"name": "Invented.csv",
                          "bytes": crate::host::b64_encode(b"Classification,Day\nLead,500\n")});
        let read = h
            .dispatch("rate_import_pick", json!({"file": file}))
            .unwrap();
        assert_eq!(read["table"]["rows"][0]["classification"], "Lead");
        let again = h
            .dispatch(
                "rate_import_read",
                json!({"token": read["token"], "index": 0, "name": "Other"}),
            )
            .unwrap();
        assert_eq!(again["name"], "Other");
        h.dispatch("rate_table_delete", json!({"name": "Invented Crew"}))
            .unwrap();
        let s = h.state().lock_doc(None).unwrap();
        assert!(s.budget().rate_tables.is_empty());
        assert_eq!(s.history().steps.len(), 5, "one undo step each");
    }
}
