//! Setup packs into and out of a budget (src/packs.rs). The Packs list and
//! everything to do with the library are the desktop's: they live in files
//! and folders on the computer.

use method_format::setup::pack::PackDetails;
use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error, FileIn, FileOut};
use crate::import_setup::{source_label, ImportSources, ItemRef, Resolution};
use crate::packs::{self as p, PackExported, PackImported};
use crate::templates::{budget_from_bytes, safe_file_stem};

/// What a pack from or into the library is told here.
const NO_LIBRARY: &str = "There is no setup library here: packs go into and out of the budget.";

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{doc, source}`: `source` is `budget` (the library is the
        // desktop's).
        "pack_items" => (|| {
            let a: Source = d.args()?;
            budget_only(&a.source)?;
            let s = d.lock()?;
            json(p::items_of(s.tab_label(), s.budget()))
        })(),
        // `{doc, source, picks, details, created}`: `{file, export}`, the
        // pack as a `FileOut` named after its title where the desktop shows
        // a Save dialog and writes it, and the desktop's reply.
        "pack_export" => (|| {
            let a: Export = d.args()?;
            budget_only(&a.source)?;
            let (built, items) = {
                let s = d.lock()?;
                p::pack_build(s.budget(), &a.picks, &a.details, &a.created)?
            };
            let name = format!(
                "{}.{}",
                safe_file_stem(&a.details.title),
                method_format::EXTENSION
            );
            json(Exported {
                export: PackExported {
                    message: p::exported_message(&name, items),
                    name: name.clone(),
                },
                file: FileOut {
                    name,
                    bytes: method_format::to_string(&built).into_bytes(),
                },
            })
        })(),
        // `{doc, into, file}`: the pack comes in as `file` (the desktop
        // shows an Open dialog); `into` is `budget`. The review, its token
        // kept for `pack_import`.
        "pack_import_pick" => (|| {
            let a: Pick = d.args()?;
            budget_only(&a.into)?;
            let src = budget_from_bytes(&a.file.name, &a.file.bytes)?;
            let sources = d.kept::<ImportSources>();
            let s = d.lock()?;
            let id = s.id();
            json(p::review_into_budget(
                s.docs(),
                &sources,
                id,
                source_label(&a.file.name),
                src,
            )?)
        })(),
        "pack_import" => (|| {
            let a: Import = d.args()?;
            let sources = d.kept::<ImportSources>();
            let mut s = d.lock()?;
            let message = p::pack_import(&mut s, &sources, a.token, &a.picks, &a.resolutions)?;
            json(PackImported {
                view: s.finish(None),
                message,
            })
        })(),
        _ => return None,
    })
}

fn budget_only(place: &str) -> Result<(), Error> {
    match place {
        "budget" => Ok(()),
        "library" => Err(Error::Refused(NO_LIBRARY.into())),
        other => Err(Error::Refused(format!("unknown pack source `{other}`"))),
    }
}

#[derive(serde::Serialize)]
struct Exported {
    #[serde(flatten)]
    export: PackExported,
    file: FileOut,
}

#[derive(Deserialize)]
struct Source {
    source: String,
}

#[derive(Deserialize)]
struct Export {
    source: String,
    picks: Vec<ItemRef>,
    details: PackDetails,
    created: String,
}

#[derive(Deserialize)]
struct Pick {
    into: String,
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

    use crate::host::test_files::{bytes_of, call, file};
    use crate::Host;

    #[test]
    fn a_pack_goes_out_of_one_budget_and_into_another() {
        let h = Host::new();
        call(&h, "set_global", json!({"name": "DAYS", "formula": "20"}));
        let items = call(&h, "pack_items", json!({"source": "budget"}));
        assert!(items["kinds"].as_array().unwrap().len() > 3);
        let out = call(
            &h,
            "pack_export",
            json!({
                "source": "budget",
                "picks": [{"kind": "global", "name": "DAYS"}],
                "details": {"title": "Schedule", "version": "1", "effective": "2026-10-01"},
                "created": "2026-10-10",
            }),
        );
        assert_eq!(out["name"], "Schedule.mthd");
        assert_eq!(out["file"]["name"], "Schedule.mthd");
        let pack = bytes_of(&out["file"]);

        // Into a second budget.
        let mut b = method_core::starter::budget();
        b.title = "Other".into();
        call(
            &h,
            "open_mthd",
            json!({"name": "Other.mthd", "text": method_format::to_string(&b)}),
        );
        let rv = call(
            &h,
            "pack_import_pick",
            json!({"into": "budget", "file": file("Schedule.mthd", &pack)}),
        );
        assert_eq!(rv["into"], "budget");
        let done = call(
            &h,
            "pack_import",
            json!({"token": rv["token"], "picks": [{"kind": "global", "name": "DAYS"}], "resolutions": []}),
        );
        assert!(done["message"]
            .as_str()
            .unwrap()
            .starts_with("Imported from Schedule"));
        assert_eq!(done["view"]["file"]["dirty"], true);

        let e = h
            .dispatch("pack_items", json!({"source": "library"}))
            .unwrap_err();
        assert!(e.message().starts_with("There is no setup library here"));
    }
}
