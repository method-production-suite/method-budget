//! Tag presets' and line blocks' commands (src/presets.rs).

use method_core::RowId;
use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};
use crate::presets::{self as p, BlockFields, TagParts};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "save_tag_preset" => d.view_with(|s, a: SaveTags| p::save_tag_preset(s, a.index, a.preset)),
        "tag_preset_from_line" => {
            d.view_with(|s, a: FromLine| p::tag_preset_from_line(s, a.row_id, &a.name, a.parts))
        }
        "apply_tag_preset" => d.view_with(|s, a: ApplyTags| {
            p::apply_tag_preset(
                s,
                a.index,
                &a.mode,
                a.row_ids.as_deref().unwrap_or_default(),
                a.accounts.as_deref().unwrap_or_default(),
                a.categories.as_deref().unwrap_or_default(),
            )
        }),
        "save_line_preset" => {
            d.view_with(|s, a: SaveLines| p::save_line_preset(s, &a.name, &a.row_ids, a.keep))
        }
        "line_preset_preview" => preview(d),
        "insert_line_preset" => insert(d),
        "rename_tool_preset" => {
            d.view_with(|s, a: Rename| p::rename_tool_preset(s, &a.which, a.index, &a.name))
        }
        "delete_tool_preset" => {
            d.view_with(|s, a: Which| p::delete_tool_preset(s, &a.which, a.index))
        }
        "move_tool_preset" => {
            d.view_with(|s, a: Move| p::move_tool_preset(s, &a.which, a.index, a.to))
        }
        _ => return None,
    })
}

fn preview(d: &Call<'_>) -> Result<Value, Error> {
    let a: Index = d.args()?;
    json(p::line_preset_preview(&*d.lock()?, a.index)?)
}

/// As the desktop replies: the view, noting the first new row.
fn insert(d: &Call<'_>) -> Result<Value, Error> {
    let a: Insert = d.args()?;
    let mut s = d.lock()?;
    let first = p::insert_line_preset(&mut s, a.index, &a.account, a.before_row)?;
    json(s.finish(first))
}

#[derive(Deserialize)]
struct SaveTags {
    index: Option<usize>,
    preset: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FromLine {
    row_id: RowId,
    name: String,
    parts: Option<TagParts>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyTags {
    index: usize,
    mode: String,
    row_ids: Option<Vec<RowId>>,
    accounts: Option<Vec<String>>,
    categories: Option<Vec<usize>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveLines {
    name: String,
    row_ids: Vec<RowId>,
    keep: Option<BlockFields>,
}

#[derive(Deserialize)]
struct Index {
    index: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Insert {
    index: usize,
    account: String,
    before_row: Option<RowId>,
}

#[derive(Deserialize)]
struct Rename {
    which: String,
    index: usize,
    name: String,
}

#[derive(Deserialize)]
struct Which {
    which: String,
    index: usize,
}

#[derive(Deserialize)]
struct Move {
    which: String,
    index: usize,
    to: usize,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::Host;

    #[test]
    fn presets_go_through_json_as_the_page_sends_them() {
        let h = Host::new();
        let v = h.dispatch("get_view", json!({})).unwrap();
        let code = v["budget"]["topsheet"][0]["accounts"][0]["code"].clone();
        let v = h
            .dispatch("add_detail", json!({"accountCode": code, "kind": "line"}))
            .unwrap();
        let row = v["created_row"].clone();

        h.dispatch(
            "save_line_preset",
            json!({"name": "Crew", "rowIds": [row], "keep": null}),
        )
        .unwrap();
        let review = h
            .dispatch("line_preset_preview", json!({"index": 0}))
            .unwrap();
        assert_eq!(review["review"], false);
        let v = h
            .dispatch(
                "insert_line_preset",
                json!({"index": 0, "account": code, "beforeRow": row}),
            )
            .unwrap();
        assert!(v["created_row"].as_u64().unwrap() > row.as_u64().unwrap());

        h.dispatch(
            "tag_preset_from_line",
            json!({"rowId": row, "name": "Plain"}),
        )
        .unwrap();
        // The line has the preset's tags already: no change, no step.
        h.dispatch(
            "apply_tag_preset",
            json!({"index": 0, "mode": "replace", "rowIds": [row]}),
        )
        .unwrap();
        h.dispatch(
            "rename_tool_preset",
            json!({"which": "lines", "index": 0, "name": "Crew block"}),
        )
        .unwrap();
        let steps = h.dispatch("history_list", json!({})).unwrap();
        let labels: Vec<&str> = steps["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["label"].as_str().unwrap())
            .collect();
        let insert = format!("Insert a line block in {}", code.as_str().unwrap());
        assert_eq!(
            labels[1..],
            [
                "Save line block Crew",
                insert.as_str(),
                "Save tag preset Plain",
                "Rename a preset to Crew block",
            ]
        );
        let e = h
            .dispatch("delete_tool_preset", json!({"which": "rows", "index": 0}))
            .unwrap_err();
        assert!(matches!(e, crate::host::Error::Refused(_)), "{e:?}");
        h.dispatch("delete_tool_preset", json!({"which": "tags", "index": 0}))
            .unwrap();
    }
}
