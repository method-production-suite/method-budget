//! Quick entry's commands (src/quickadd.rs).

use method_core::RowId;
use serde::Deserialize;
use serde_json::Value;

use super::{Call, Error};
use crate::quickadd::{self as q, CreateInput, NewLineDefaultsInput};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "set_cell_creating" => d.view_with(|s, a: Creating| {
            q::set_cell_creating(s, a.row_id, &a.field, &a.value, &a.create)
        }),
        "set_new_line_defaults" => {
            d.view_with(|s, a: Defaults| q::set_new_line_defaults(s, &a.defaults))
        }
        _ => return None,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Creating {
    row_id: RowId,
    field: String,
    value: String,
    create: CreateInput,
}

#[derive(Deserialize)]
struct Defaults {
    defaults: NewLineDefaultsInput,
}

#[cfg(test)]
mod tests {
    use method_core::RowKind;
    use serde_json::json;

    use crate::Host;

    #[test]
    fn a_unit_added_from_its_cell_and_new_line_defaults_as_the_page_sends_them() {
        let h = Host::new();
        let row = crate::edit::details(h.state().lock_doc(None).unwrap().budget())
            .find(|d| d.kind == RowKind::Line)
            .unwrap()
            .id;
        h.dispatch(
            "set_cell_creating",
            json!({"rowId": row, "field": "unit", "value": "Gig",
                   "create": {"kind": "unit", "name": "Gig"}}),
        )
        .unwrap();
        h.dispatch(
            "set_new_line_defaults",
            json!({"defaults": {"fringes": false, "groups": true, "set": true,
                   "location": false, "currency": false}}),
        )
        .unwrap();
        let s = h.state().lock_doc(None).unwrap();
        assert!(s.budget().units.contains_key("Gig"));
        assert!(s.budget().new_line_defaults.groups);
        assert_eq!(s.history().steps.len(), 2, "one undo step each");
    }
}
