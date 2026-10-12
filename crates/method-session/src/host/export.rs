//! File > Export (src/export.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{format}` as the desktop's. The reply is `Downloads`: the files
        // for the host to offer as downloads (one, or for CSV one per
        // table), named as the desktop's Save dialog would suggest, where
        // the desktop writes them; beside them `{done, files}` as the
        // desktop replies after a save.
        "export_budget" => d.args().and_then(|a: Export| {
            json(crate::export::export_budget(d.state(), d.doc(), &a.format)?)
        }),
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Export {
    format: String,
}
