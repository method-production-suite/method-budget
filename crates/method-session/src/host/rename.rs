//! Renaming a setup item everywhere (src/rename.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};
use crate::rename::{self as r, Renamed};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "rename_setup_item" => rename(d),
        _ => return None,
    })
}

/// As the desktop replies: the view, the new name and how many places use
/// it.
fn rename(d: &Call<'_>) -> Result<Value, Error> {
    let a: Rename = d.args()?;
    let mut s = d.lock()?;
    let (name, places) = r::rename_setup_item(&mut s, &a.kind, &a.from, &a.to)?;
    let view = s.finish(None);
    json(Renamed { view, name, places })
}

#[derive(Deserialize)]
struct Rename {
    kind: String,
    from: String,
    to: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::Host;

    #[test]
    fn a_rename_goes_through_json_as_the_page_sends_it() {
        let h = Host::new();
        h.dispatch("set_global", json!({"name": "DAYS", "formula": "5"}))
            .unwrap();
        let v = h
            .dispatch(
                "rename_setup_item",
                json!({"kind": "global", "from": "DAYS", "to": " BUILD_DAYS "}),
            )
            .unwrap();
        assert_eq!(v["name"], "BUILD_DAYS");
        assert_eq!(v["places"], 0);
        assert_eq!(v["can_undo"], true, "the view, flattened");
        let steps = h.dispatch("history_list", json!({})).unwrap();
        assert_eq!(
            steps["steps"][1]["label"],
            "Rename global DAYS to BUILD_DAYS"
        );
        let e = h
            .dispatch(
                "rename_setup_item",
                json!({"kind": "thing", "from": "a", "to": "b"}),
            )
            .unwrap_err();
        assert!(e.message().contains("unknown kind"), "{e:?}");
    }
}
