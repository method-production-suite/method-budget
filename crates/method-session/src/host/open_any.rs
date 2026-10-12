//! File > Open for more than budgets (src/open_any.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error, FileIn};
use crate::export_import::ExportSources;
use crate::open_any as oa;
use crate::sheet_import::SheetSources;

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{file}`: the file comes in as `file` (the desktop shows an Open
        // dialog, and `open_pending` takes one the system handed over). A
        // budget opens in a tab in front (an untouched Untitled gives its
        // tab up, as `open_mthd`); an export goes to Import Budget Export's
        // review, a spreadsheet to Import Spreadsheet's dialog, each kept by
        // token.
        "open_budget" => (|| {
            let a: Open = d.args()?;
            let name = a.file.name.clone();
            let exports = d.kept::<ExportSources>();
            let sheets = d.kept::<SheetSources>();
            json(oa::open_budget(
                a.file.name,
                a.file.bytes,
                &exports,
                &sheets,
                |bytes| {
                    let text = std::str::from_utf8(&bytes).map_err(|_| {
                        format!("Couldn't read {name}: stream did not contain valid UTF-8")
                    })?;
                    d.host.open_mthd(&name, text).map_err(|e| e.message())
                },
            )?)
        })(),
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Open {
    file: FileIn,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::test_files::{call, file};
    use crate::Host;

    #[test]
    fn open_reads_budgets_exports_and_spreadsheets() {
        let h = Host::new();
        let mut b = method_core::starter::budget();
        b.title = "Opened".into();
        let text = method_format::to_string(&b);
        // A budget saved as .json opens as a budget, in place of the
        // untouched starter.
        let o = call(
            &h,
            "open_budget",
            json!({"file": file("pilot.json", text.as_bytes())}),
        );
        assert_eq!(o["kind"], "budget");
        assert_eq!(o["view"]["budget"]["title"], "Opened");
        assert_eq!(h.state().docs().len(), 1);

        // A spreadsheet goes to Import Spreadsheet's dialog, by token.
        let o = call(
            &h,
            "open_budget",
            json!({"file": file("lines.csv", b"Account,Description,Amount,Rate\n1001,Writer,1,5000\n")}),
        );
        assert_eq!(o["kind"], "sheet");
        assert_eq!(o["picked"]["file"], "lines.csv");
        let pv = call(
            &h,
            "sheet_import_preview",
            json!({"token": o["picked"]["token"], "request": {"sheet": 0, "detect_header": true, "guess": true}}),
        );
        assert_eq!(pv["line_count"], 1);

        // Anything else says what Open reads; nothing changes.
        let e = h
            .dispatch(
                "open_budget",
                json!({"file": file("picture.png", b"\x89PNG\r\n")}),
            )
            .unwrap_err();
        assert!(e.message().contains(crate::open_any::OPEN_READS));
        assert_eq!(h.state().docs().len(), 1);
    }
}
