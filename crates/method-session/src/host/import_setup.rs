//! File > Import Setup From… (src/import_setup.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error, FileIn};
use crate::import_setup::{self as is, ImportDone, ImportSources, ItemRef, Resolution};
use crate::templates::budget_from_bytes;

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{doc, file}`: the budget to import from comes in as `file` (the
        // desktop shows an Open dialog). The review, its token kept for
        // `import_setup`.
        "import_setup_pick" => (|| {
            let a: Pick = d.args()?;
            let source = budget_from_bytes(&a.file.name, &a.file.bytes)?;
            let label = is::source_label(&a.file.name);
            let s = d.lock()?;
            let sources = d.kept::<ImportSources>();
            json(is::import_setup_pick(
                &s,
                &sources,
                label,
                source,
                &a.file.name,
            ))
        })(),
        "import_setup" => (|| {
            let a: Import = d.args()?;
            let sources = d.kept::<ImportSources>();
            let mut s = d.lock()?;
            let counts = is::import_setup(&mut s, &sources, a.token, &a.picks, &a.resolutions)?;
            json(ImportDone::new(s.finish(None), counts))
        })(),
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Pick {
    file: FileIn,
}

#[derive(Deserialize)]
struct Import {
    token: u64,
    picks: Vec<ItemRef>,
    resolutions: Vec<Resolution>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::test_files::{call, file};
    use crate::Host;

    #[test]
    fn setup_is_imported_from_a_picked_budget() {
        let h = Host::new();
        let mut src = method_core::starter::budget();
        src.globals.insert("NIGHTS".into(), "30".into());
        let text = method_format::to_string(&src);
        let p = call(
            &h,
            "import_setup_pick",
            json!({"doc": null, "file": file("Other show.mthd", text.as_bytes())}),
        );
        assert_eq!(p["source"], "Other show");
        let token = p["token"].clone();
        let done = call(
            &h,
            "import_setup",
            json!({
                "token": token,
                "picks": [{"kind": "global", "name": "NIGHTS"}],
                "resolutions": [],
            }),
        );
        assert!(done["message"].as_str().unwrap().contains("1"), "{done}");
        assert_eq!(done["view"]["budget"]["globals"]["NIGHTS"], "30");
        // A stale token is refused.
        let e = h
            .dispatch(
                "import_setup",
                json!({"token": 999, "picks": [], "resolutions": []}),
            )
            .unwrap_err();
        assert_eq!(e.message(), "Another file was picked since. Import again.");
    }
}
