//! File > Import Spreadsheet… (src/sheet_import.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error, FileIn};
use crate::sheet_import::{self as si, Imported, Request, SheetSources};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{doc, file}`: the spreadsheet comes in as `file` (the desktop
        // shows an Open dialog). Its sheets, kept by the reply's token.
        "sheet_import_pick" => (|| {
            let a: Pick = d.args()?;
            let sources = d.kept::<SheetSources>();
            json(si::sheet_import_pick(&sources, a.file.name, &a.file.bytes)?)
        })(),
        "sheet_import_preview" => (|| {
            let a: Choices = d.args()?;
            let b = d.lock()?.budget().clone();
            let sources = d.kept::<SheetSources>();
            json(si::sheet_import_preview(&b, &sources, a.token, &a.request)?)
        })(),
        "sheet_import" => (|| {
            let a: Choices = d.args()?;
            let sources = d.kept::<SheetSources>();
            let (id, message) =
                si::sheet_import(d.state(), &sources, d.doc(), a.token, &a.request)?;
            let view = d.state().lock_doc(Some(id))?.finish(None);
            json(Imported { view, message })
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::test_files::{call, file};
    use crate::Host;

    const CSV: &[u8] = b"Acct,Item,Qty,Rate\n1001,Rewrite,1,2500\n2101,Camera body,2,450\n";

    #[test]
    fn a_picked_csv_is_previewed_and_imported_by_token() {
        let h = Host::new();
        let p = call(
            &h,
            "sheet_import_pick",
            json!({"doc": null, "file": file("dept.csv", CSV)}),
        );
        assert_eq!(p["file"], "dept.csv");
        assert_eq!(p["kind"], "csv");
        let token = p["token"].clone();
        let request = json!({
            "sheet": 0,
            "header_row": null,
            "detect_header": true,
            "guess": true,
            "create_accounts": true,
        });
        let pv = call(
            &h,
            "sheet_import_preview",
            json!({"token": token, "request": request}),
        );
        assert_eq!(pv["line_count"], 2);
        let done = call(
            &h,
            "sheet_import",
            json!({"token": token, "request": request}),
        );
        assert!(
            done["message"]
                .as_str()
                .unwrap()
                .starts_with("Imported 2 lines"),
            "{}",
            done["message"]
        );
        assert_eq!(done["view"]["can_undo"], true);
        // Into a new tab.
        let mut new = request.clone();
        new["target"] = json!("new");
        let done = call(&h, "sheet_import", json!({"token": token, "request": new}));
        assert_eq!(done["view"]["budget"]["title"], "Untitled");
        assert_eq!(h.state().docs().len(), 2);
        // Another file picked since: the old token is refused.
        call(
            &h,
            "sheet_import_pick",
            json!({"file": file("other.csv", CSV)}),
        );
        assert!(h
            .dispatch("sheet_import", json!({"token": token, "request": request}))
            .is_err());
    }
}
