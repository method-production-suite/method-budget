//! The apply panel's commands (src/apply.rs).

use method_core::RowId;
use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};
use crate::apply::{self as ap, TagsInput};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "apply_summary" => summary(d),
        "apply_to_rows" => {
            d.view_with(|s, a: Tag| ap::apply_to_rows(s, &a.row_ids, &a.field, &a.name, a.on))
        }
        "assign_to_rows" => d.view_with(|s, a: Assign| {
            ap::assign_to_rows(s, &a.row_ids, &a.field, a.value.as_deref())
        }),
        "clear_from_rows" => {
            d.view_with(|s, a: Field| ap::clear_from_rows(s, &a.row_ids, &a.field))
        }
        "replace_tags_on_rows" => {
            d.view_with(|s, a: Tags| ap::replace_tags_on_rows(s, &a.row_ids, &a.tags))
        }
        "make_fringe_range" => d.view_with(|s, a: Rows| ap::make_fringe_range(s, &a.row_ids)),
        "clear_fringe_range" => d.view_with(|s, a: Rows| ap::clear_fringe_range(s, &a.row_ids)),
        _ => return None,
    })
}

fn summary(d: &Call<'_>) -> Result<Value, Error> {
    let a: Selection = d.args()?;
    let mut s = d.lock()?;
    json(ap::apply_summary(
        &mut s,
        a.row_ids.as_deref().unwrap_or_default(),
        a.accounts.as_deref().unwrap_or_default(),
        a.categories.as_deref().unwrap_or_default(),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Selection {
    row_ids: Option<Vec<RowId>>,
    accounts: Option<Vec<String>>,
    categories: Option<Vec<usize>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tag {
    row_ids: Vec<RowId>,
    field: String,
    name: String,
    on: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Assign {
    row_ids: Vec<RowId>,
    field: String,
    value: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Field {
    row_ids: Vec<RowId>,
    field: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tags {
    row_ids: Vec<RowId>,
    tags: TagsInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rows {
    row_ids: Vec<RowId>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::Host;

    #[test]
    fn the_apply_panel_goes_through_json_as_the_page_sends_it() {
        let h = Host::new();
        let v = h.dispatch("get_view", json!({})).unwrap();
        let code = v["budget"]["topsheet"][0]["accounts"][0]["code"].clone();
        let mut rows = Vec::new();
        for _ in 0..2 {
            let v = h
                .dispatch("add_detail", json!({"accountCode": code, "kind": "line"}))
                .unwrap();
            rows.push(v["created_row"].clone());
        }
        h.dispatch(
            "set_group",
            json!({"name": "Alt", "group": {"description": "Option A", "in_budget_total": true}}),
        )
        .unwrap();
        h.dispatch(
            "apply_to_rows",
            json!({"rowIds": rows, "field": "groups", "name": "Alt", "on": true}),
        )
        .unwrap();
        let sum = h
            .dispatch("apply_summary", json!({"rowIds": rows}))
            .unwrap();
        assert_eq!(sum["rows"], json!(rows));
        h.dispatch("make_fringe_range", json!({"rowIds": rows}))
            .unwrap();
        let steps = h.dispatch("history_list", json!({})).unwrap();
        let labels: Vec<&str> = steps["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["label"].as_str().unwrap())
            .collect();
        assert!(labels.iter().any(|l| l.starts_with("Add group Alt to ")));
        assert!(labels
            .iter()
            .any(|l| l.starts_with("Make a fringe range of ")));
        let e = h
            .dispatch(
                "clear_from_rows",
                json!({"rowIds": [999_999], "field": "groups"}),
            )
            .unwrap_err();
        assert!(e.message().contains("999999"), "{e:?}");
    }
}
