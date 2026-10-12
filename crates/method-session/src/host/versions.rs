//! The Versions dialog's commands (src/versions.rs).

use method_core::VersionId;
use serde::Deserialize;
use serde_json::Value;

use super::{Call, Error};
use crate::versions as v;

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "save_version" => {
            d.view_with(|s, a: Save| v::save_version(s, &a.name, a.note.as_deref(), &a.created))
        }
        "restore_version" => d.view_with(|s, a: Restore| v::restore_version(s, a.id, &a.created)),
        "rename_version" => d.view_with(|s, a: Rename| v::rename_version(s, a.id, &a.name)),
        "delete_version" => d.view_with(|s, a: Id| v::delete_version(s, a.id)),
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Save {
    name: String,
    note: Option<String>,
    created: String,
}

#[derive(Deserialize)]
struct Restore {
    id: VersionId,
    created: String,
}

#[derive(Deserialize)]
struct Rename {
    id: VersionId,
    name: String,
}

#[derive(Deserialize)]
struct Id {
    id: VersionId,
}
