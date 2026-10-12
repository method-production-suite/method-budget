//! Setup lists as spreadsheets (src/setup_sheets.rs).

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Downloads, Error, FileIn, FileOut};
use crate::import_setup::ImportSources;
use crate::setup_sheets::{self as ss, SheetChoice};
use crate::sheet_import::SheetSources;

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{doc, kinds, format}`: where the desktop shows a Save dialog and
        // writes the files, the reply carries them as `downloads`
        // (`FileOut`s named "Pilot setup.xlsx", or "Pilot setup - Fringes.csv"
        // and so on), beside the desktop's `done`, `files` and `message`.
        "setup_sheet_export" => (|| {
            let a: Export = d.args()?;
            let ex = ss::setup_sheet_export(&*d.lock()?, &a.kinds, &a.format)?;
            let out = format!("{}.{}", ex.stem, ex.ext());
            let downloads: Vec<FileOut> = ex
                .files(Path::new(&out))?
                .into_iter()
                .map(|f| FileOut {
                    name: f
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    bytes: f.bytes,
                })
                .collect();
            let names = downloads.iter().map(|f| f.name.clone()).collect();
            json(Downloads {
                reply: ex.exported(names),
                downloads,
            })
        })(),
        // `{doc, file}`: the spreadsheet comes in as `file` (the desktop
        // shows an Open dialog). Its sheets, kept by the reply's token.
        "setup_sheet_pick" => (|| {
            let a: Pick = d.args()?;
            let sources = d.kept::<SheetSources>();
            json(ss::setup_sheet_pick(&sources, a.file.name, &a.file.bytes)?)
        })(),
        "setup_sheet_guess" => (|| {
            let a: Guess = d.args()?;
            let sources = d.kept::<SheetSources>();
            json(ss::setup_sheet_guess(&sources, a.token, a.index, &a.kind)?)
        })(),
        "setup_sheet_review" => (|| {
            let a: Review = d.args()?;
            let sheets = d.kept::<SheetSources>();
            let imports = d.kept::<ImportSources>();
            json(ss::setup_sheet_review(
                &*d.lock()?,
                &sheets,
                &imports,
                a.token,
                &a.choices,
            )?)
        })(),
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Export {
    kinds: Vec<String>,
    format: String,
}

#[derive(Deserialize)]
struct Pick {
    file: FileIn,
}

#[derive(Deserialize)]
struct Guess {
    token: u64,
    index: usize,
    kind: String,
}

#[derive(Deserialize)]
struct Review {
    token: u64,
    choices: Vec<SheetChoice>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::test_files::{bytes_of, call, file};
    use crate::Host;

    #[test]
    fn setup_lists_go_out_as_a_workbook_and_come_back_through_the_review() {
        let h = Host::new();
        call(&h, "set_global", json!({"name": "NIGHTS", "formula": "12"}));
        let out = call(
            &h,
            "setup_sheet_export",
            json!({"kinds": ["global"], "format": "xlsx"}),
        );
        assert_eq!(out["done"], true);
        assert_eq!(out["files"], json!(["Untitled feature setup.xlsx"]));
        let wb = &out["downloads"][0];
        assert_eq!(wb["name"], "Untitled feature setup.xlsx");

        // Into another budget, which lacks the global.
        let h2 = Host::new();
        let p = call(
            &h2,
            "setup_sheet_pick",
            json!({"file": file("Setup.xlsx", &bytes_of(wb))}),
        );
        assert_eq!(p["sheets"][0]["kind"], "global");
        let g = call(
            &h2,
            "setup_sheet_guess",
            json!({"token": p["token"], "index": 0, "kind": "global"}),
        );
        let choice = json!({"index": 0, "kind": "global", "mapping": g["mapping"]});
        let r = call(
            &h2,
            "setup_sheet_review",
            json!({"token": p["token"], "choices": [choice]}),
        );
        assert_eq!(r["errors"], json!([]));
        let review = &r["preview"];
        let done = call(
            &h2,
            "import_setup",
            json!({
                "token": review["token"],
                "picks": [{"kind": "global", "name": "NIGHTS"}],
                "resolutions": [],
            }),
        );
        assert_eq!(done["view"]["budget"]["globals"]["NIGHTS"], "12");

        // CSV is a file per kind.
        let out = call(
            &h,
            "setup_sheet_export",
            json!({"kinds": ["global", "unit"], "format": "csv"}),
        );
        assert_eq!(out["downloads"].as_array().unwrap().len(), 2);
        assert!(h
            .dispatch("setup_sheet_export", json!({"kinds": [], "format": "pdf"}))
            .is_err());
    }
}
