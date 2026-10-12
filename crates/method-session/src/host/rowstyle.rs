//! Row style and group colors' commands (src/rowstyle.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{Call, Error};
use crate::edit::GroupInput;
use crate::rowstyle::{self as r, RowStyleInput};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // The whole arguments object, as the desktop reads it, so an absent
        // `color` and `color: null` differ.
        "set_row_style" => d.view_with(|s, a: Value| {
            let input = RowStyleInput::from_json(&a)?;
            r::set_row_style(s, &input)
        }),
        "set_group_color" => {
            d.view_with(|s, a: Color| r::set_group_color(s, &a.name, a.color.as_deref()))
        }
        "update_group" => {
            d.view_with(|s, a: Update| r::update_group(s, &a.name, &a.group, a.color.as_deref()))
        }
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Color {
    name: String,
    color: Option<String>,
}

#[derive(Deserialize)]
struct Update {
    name: String,
    group: GroupInput,
    color: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::edit::details;
    use crate::host::Host;

    #[test]
    fn row_style_goes_through_json_as_the_page_sends_it() {
        let h = Host::new();
        let v = h.dispatch("get_view", json!({})).unwrap();
        let code = v["budget"]["topsheet"][0]["accounts"][0]["code"].clone();
        let v = h
            .dispatch("add_detail", json!({"accountCode": code, "kind": "line"}))
            .unwrap();
        let row = v["created_row"].clone();
        let find = || {
            let id = row.as_u64().unwrap();
            let s = h.state().lock_doc(None).unwrap();
            let d = details(s.budget()).find(|d| d.id == id).unwrap();
            (d.bold, d.italic, d.color.clone())
        };
        h.dispatch(
            "set_row_style",
            json!({"rowIds": [row], "bold": true, "color": "#ABCDEF"}),
        )
        .unwrap();
        assert_eq!(find(), (true, false, Some("#abcdef".to_string())));
        // `color: null` clears; absent leaves it.
        h.dispatch("set_row_style", json!({"rowIds": [row], "italic": true}))
            .unwrap();
        assert_eq!(find(), (true, true, Some("#abcdef".to_string())));
        h.dispatch("set_row_style", json!({"rowIds": [row], "color": null}))
            .unwrap();
        assert_eq!(find(), (true, true, None));
        let steps = h.dispatch("history_list", json!({})).unwrap();
        assert_eq!(steps["steps"][1]["label"], "Format lines");
        let e = h
            .dispatch("set_row_style", json!({"bold": true}))
            .unwrap_err();
        assert!(e.message().starts_with("bad row style"), "{e:?}");
    }
}
