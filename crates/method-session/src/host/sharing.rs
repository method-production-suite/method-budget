//! Copies for sharing, a shared copy's banner and sub-budgets
//! (src/sharing.rs).

use method_core::VersionId;
use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Downloads, Error, FileOut};
use crate::docs::DocId;
use crate::sharing::{self as sh, RowInput, ShareInput, SubBudgetInput};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{doc, version, leaveOut, note, saved}`: the copy as a download
        // under its suggested name (`<title> - <version> - <date>.mthd`),
        // where the desktop shows a Save dialog and writes it
        // (`Downloads`). The budget is not changed.
        "share_copy_save" => (|| {
            let a: Save = d.args()?;
            let s = d.lock()?;
            let (copy, stem) = sh::make_copy(
                s.budget(),
                a.version,
                a.leave_out.unwrap_or_default(),
                a.note.as_deref(),
                &a.saved,
            )?;
            json(Downloads::one(FileOut {
                name: format!("{stem}.{}", method_format::EXTENSION),
                bytes: method_format::to_string(&copy).into_bytes(),
            }))
        })(),
        "share_copy_editable" => (|| {
            let id = sh::open_editable(d.state(), d.doc())?;
            json(d.state().lock_doc(Some(id))?.finish(None))
        })(),
        "share_copy_original" => (|| {
            let id = d.lock()?.id();
            json(sh::share_copy_original(&d.state().docs(), id))
        })(),
        "share_copy_store" => (|| {
            let a: Store = d.args()?;
            let copy = sh::copy_to_store(d.state(), d.doc(), a.original)?;
            let mut s = d.state().lock_doc(Some(a.original))?;
            sh::share_copy_store(&mut s, &copy, &a.name, &a.created)?;
            json(s.finish(None))
        })(),
        "sub_budget_new" => (|| {
            let a: SubBudget = d.args()?;
            let id = sh::sub_budget_new(d.state(), d.doc(), a.spec, &a.saved)?;
            json(d.state().lock_doc(Some(id))?.finish(None))
        })(),
        // `{doc, row: {kind, code}, saved}`: the sub-budget of one category
        // or account row in a new tab with no file, as `{view, message}`
        // (the desktop saves it beside its parent's file).
        "sub_budget_from_row" => (|| {
            let a: FromRow = d.args()?;
            let id = sh::row_sub_budget_new(d.state(), d.doc(), &a.row, &a.saved)?;
            let view = d.state().lock_doc(Some(id))?.finish(None);
            json(serde_json::json!({
                "view": view,
                "message": "Opened the sub-budget in a new tab. It has no file until you save it.",
            }))
        })(),
        _ => return None,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Save {
    version: Option<VersionId>,
    leave_out: Option<ShareInput>,
    note: Option<String>,
    saved: String,
}

#[derive(Deserialize)]
struct Store {
    original: DocId,
    name: String,
    created: String,
}

#[derive(Deserialize)]
struct SubBudget {
    spec: SubBudgetInput,
    saved: String,
}

#[derive(Deserialize)]
struct FromRow {
    row: RowInput,
    saved: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::test_files::{bytes_of, call};
    use crate::Host;

    const AT: &str = "2026-10-09T14:30:00-07:00";

    #[test]
    fn a_copy_for_sharing_goes_out_as_a_file_and_comes_back_read_only() {
        let h = Host::new();
        let first = call(&h, "set_title", json!({"title": "Big Show"}))["doc"].clone();
        let out = call(
            &h,
            "share_copy_save",
            json!({"leaveOut": {"notes": true}, "note": "For the studio", "saved": AT}),
        );
        assert_eq!(out["name"], "Big Show - 2026-10-09.mthd");
        let text = String::from_utf8(bytes_of(&out["downloads"][0])).unwrap();
        let v = call(
            &h,
            "open_mthd",
            json!({"name": "Big Show - 2026-10-09.mthd", "text": text}),
        );
        let copy = v["doc"].clone();
        let e = h
            .dispatch("set_title", json!({"doc": copy, "title": "x"}))
            .unwrap_err();
        assert_eq!(e.message(), crate::session::READ_ONLY);

        let o = call(&h, "share_copy_original", json!({"doc": copy}));
        assert_eq!(o["doc"], first);
        let v = call(
            &h,
            "share_copy_store",
            json!({"doc": copy, "original": first, "name": "Studio", "created": AT}),
        );
        assert_eq!(v["doc"], first);
        assert_eq!(v["budget"]["versions"][0]["name"], "Studio");

        let v = call(&h, "share_copy_editable", json!({"doc": copy}));
        assert_ne!(v["doc"], copy);
        call(&h, "set_title", json!({"title": "Mine now"}));

        let v = call(
            &h,
            "sub_budget_new",
            json!({"doc": first, "spec": {"drop_notes": true}, "saved": AT}),
        );
        assert_eq!(v["budget"]["title"], "Big Show (sub-budget)");

        let v = call(
            &h,
            "sub_budget_from_row",
            json!({"doc": first, "row": {"kind": "category", "code": "21"}, "saved": AT}),
        );
        assert_eq!(v["view"]["budget"]["title"], "Big Show - Camera");
        assert!(v["message"].as_str().unwrap().contains("new tab"));
    }
}
