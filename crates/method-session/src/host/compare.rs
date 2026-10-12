//! The compare screen's commands (src/compare.rs). The files picked to
//! compare with are kept in the host's one [`CompareFiles`], by token.

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error, FileIn};
use crate::compare::{self as c, CompareFiles, Source};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    reply(d).transpose()
}

/// The reply to `d`, or `None` when it names none of these commands.
fn reply(d: &Call<'_>) -> Result<Option<Value>, Error> {
    let v = match d.command() {
        "compare_budgets" => {
            let a: Budgets = d.args()?;
            let files = d.kept::<CompareFiles>();
            json(c::compare_budgets(d.state(), &files, d.doc(), &a.sources)?)
        }
        "compare_detail" => {
            let a: Detail = d.args()?;
            let files = d.kept::<CompareFiles>();
            json(c::compare_detail(
                d.state(),
                &files,
                d.doc(),
                &a.sources,
                &a.code,
            )?)
        }
        // `{files: [FileIn]}`: the `.mthd` files the person picked (the
        // desktop shows its Open dialog here). The reply is the desktop's,
        // each file's `path` being its name.
        "compare_pick_files" => {
            let a: PickFiles = d.args()?;
            json(c::pick_files(&d.kept::<CompareFiles>(), &a.files)?)
        }
        _ => return Ok(None),
    };
    v.map(Some)
}

#[derive(Deserialize)]
struct Budgets {
    sources: Vec<Source>,
}

#[derive(Deserialize)]
struct Detail {
    sources: Vec<Source>,
    code: String,
}

#[derive(Deserialize)]
struct PickFiles {
    files: Vec<FileIn>,
}
