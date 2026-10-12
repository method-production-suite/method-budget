//! What Changed and Merge Changes From (src/diffmerge.rs). Picked files are
//! kept with those picked for comparing, in the host's one
//! [`CompareFiles`].

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error, FileIn};
use crate::compare::CompareFiles;
use crate::diffmerge::{self as m, Against, BaseSource, CopySource};

/// What Changed against the budget's own file: the host keeps no file.
const NO_SAVED_FILE: &str =
    "The saved file can't be read here. Choose it as a file to see what changed.";

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    reply(d).transpose()
}

/// The reply to `d`, or `None` when it names none of these commands.
fn reply(d: &Call<'_>) -> Result<Option<Value>, Error> {
    let v = match d.command() {
        // `{against: {kind: "saved"}}` is refused: the host has no file to
        // read (the page picks the file instead).
        "what_changed" => {
            let a: WhatChanged = d.args()?;
            let files = d.kept::<CompareFiles>();
            json(m::what_changed(
                d.state(),
                &files,
                d.doc(),
                a.against,
                |_| Err(NO_SAVED_FILE.to_string()),
            )?)
        }
        "merge_preview" => {
            let a: Merge = d.args()?;
            let files = d.kept::<CompareFiles>();
            json(m::merge_preview(
                d.state(),
                &files,
                d.doc(),
                a.copy,
                a.base,
                a.picks,
            )?)
        }
        "merge_apply" => {
            let a: Merge = d.args()?;
            let files = d.kept::<CompareFiles>();
            let mut s = m::merge_apply(d.state(), &files, d.doc(), a.copy, a.base, a.picks)?;
            json(s.finish(None))
        }
        // `{purpose, file: FileIn}`: the `.mthd` file the person picked
        // (the desktop shows its Open dialog here). The reply is the
        // desktop's, the file's `path` being its name.
        "merge_pick_file" => {
            let a: PickFile = d.args()?;
            let files = d.kept::<CompareFiles>();
            json(m::merge_pick_file(&files, &a.purpose, &a.file)?)
        }
        _ => return Ok(None),
    };
    v.map(Some)
}

#[derive(Deserialize)]
struct WhatChanged {
    against: Against,
}

#[derive(Deserialize)]
struct Merge {
    copy: CopySource,
    base: BaseSource,
    picks: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize)]
struct PickFile {
    purpose: String,
    file: FileIn,
}
