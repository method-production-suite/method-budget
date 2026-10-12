//! File > Import Budget Export… (src/export_import.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error, FileIn};
use crate::export_import::{self as ei, ExportSources, Imported};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{doc, file}`: the export comes in as `file` (the desktop shows an
        // Open dialog). The review, kept by its token.
        "export_import_pick" => (|| {
            let a: Pick = d.args()?;
            let sources = d.kept::<ExportSources>();
            json(ei::export_import_pick(
                &sources,
                a.file.name,
                a.file.bytes,
                None,
            )?)
        })(),
        "export_import_preview" => (|| {
            let a: Layout = d.args()?;
            let sources = d.kept::<ExportSources>();
            json(ei::export_import_preview(
                &sources,
                a.token,
                a.format.as_deref(),
            )?)
        })(),
        "export_import" => (|| {
            let a: Layout = d.args()?;
            let sources = d.kept::<ExportSources>();
            let (id, message) =
                ei::export_import(d.state(), &sources, a.token, a.format.as_deref())?;
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
struct Layout {
    token: u64,
    format: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::test_files::{call, file};
    use crate::Host;

    const EXPORT: &str = r#"{"metadata": {"budgetdata": {"BaseCurrency": "USD", "ConvCurrency": "USD"}},
     "categories": [{"cID": 0, "cNumber": "1100", "cDescription": "Story", "cFringe": 0, "cOriginal": 0, "cTotal": 300, "cVariance": 300}],
     "accounts": [{"categoryID": 0, "aID": 0, "aNumber": "1101", "aDescription": "Rights", "aFringe": 0, "aOriginal": 0, "aTotal": 300, "aVariance": 300}],
     "details": [{"accountID": 0, "dDescription": "Option", "dAmount": 3, "dUnit": "Weeks", "dX": 1, "dUnit2": "", "dCurrency": "", "dRate": 100, "dUnit3": "", "dUnit4": "", "dSubtotal": 300, "dFourthMlt": 1, "dFringes": [], "dGroups": [], "dGlobalsUsed": [], "dLocation": "", "dSet": "", "dAggPercent": "", "dAggSumPercent": ""}]}"#;

    #[test]
    fn a_picked_export_is_reviewed_and_imported_in_a_new_tab() {
        let h = Host::new();
        let r = call(
            &h,
            "export_import_pick",
            json!({"file": file("pilot.json", EXPORT.as_bytes())}),
        );
        assert_eq!(
            (r["categories"].clone(), r["lines"].clone()),
            (json!(1), json!(1))
        );
        assert_eq!(r["format"], "json");
        let r2 = call(
            &h,
            "export_import_preview",
            json!({"token": r["token"], "format": "json"}),
        );
        assert_eq!(r2["grand"], r["grand"]);
        let done = call(
            &h,
            "export_import",
            json!({"token": r["token"], "format": null}),
        );
        assert!(done["message"].as_str().unwrap().contains("1 line"));
        assert_eq!(done["view"]["can_undo"], true);
        assert_eq!(h.state().docs().len(), 2);
        assert!(h
            .dispatch(
                "export_import_pick",
                json!({"file": file("x.txt", b"hello")})
            )
            .unwrap_err()
            .message()
            .contains("Couldn't read x.txt"));
    }
}
