//! New From Template and Save as Template (src/templates.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Downloads, Error, FileIn, FileOut};
use crate::templates as t;

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{keepLines, file}`: the picked budget comes in as `file` (the
        // desktop shows an Open dialog). The new budget's view.
        "template_new" => (|| {
            let a: New = d.args()?;
            let b = t::budget_from_bytes(&a.file.name, &a.file.bytes)?;
            let id = t::template_new(d.state(), &b, a.keep_lines);
            json(d.state().lock_doc(Some(id))?.finish(None))
        })(),
        // `{doc, keepLines}`: the template as a download named after the
        // budget ("Pilot template.mthd"), where the desktop shows a Save
        // dialog and writes it (`Downloads`). The budget is not changed.
        "template_save" => (|| {
            let a: Save = d.args()?;
            let s = d.lock()?;
            let stem = t::template_name(&s);
            json(Downloads::one(FileOut {
                name: format!("{stem}.{}", method_format::EXTENSION),
                bytes: t::template_save(&s, a.keep_lines, &stem).into_bytes(),
            }))
        })(),
        _ => return None,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct New {
    keep_lines: bool,
    file: FileIn,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Save {
    keep_lines: bool,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::test_files::{bytes_of, call, file};
    use crate::Host;

    #[test]
    fn a_template_comes_in_as_a_file_and_goes_out_as_one() {
        let h = Host::new();
        call(&h, "set_title", json!({"title": "Pilot"}));
        let out = call(&h, "template_save", json!({"keepLines": false}));
        assert_eq!(out["name"], "Pilot template.mthd");
        let v = call(
            &h,
            "template_new",
            json!({"keepLines": true, "file": file("Pilot template.mthd", &bytes_of(&out["downloads"][0]))}),
        );
        assert_eq!(v["budget"]["title"], "Untitled");
        assert_eq!(h.state().docs().len(), 2, "a new tab");
        let e = h
            .dispatch(
                "template_new",
                json!({"keepLines": false, "file": file("x.mthd", b"{}")}),
            )
            .unwrap_err();
        assert!(e.message().starts_with("x.mthd isn't a readable budget"));
    }
}
