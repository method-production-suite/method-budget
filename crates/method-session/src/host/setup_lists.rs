//! The setup lists' use, order and short codes (src/setup_lists.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};
use crate::setup_lists as l;

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "setup_usage" => d.lock().and_then(|mut s| json(l::setup_usage(&mut s))),
        "set_setup_order" => d.view_with(|s, a: Order| l::set_setup_order(s, &a.kind, &a.names)),
        "move_setup_item" => {
            d.view_with(|s, a: Move| l::move_setup_item(s, &a.kind, &a.name, a.by))
        }
        "set_short_code" => {
            d.view_with(|s, a: ShortCode| l::set_short_code(s, &a.kind, &a.name, &a.code))
        }
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Order {
    kind: String,
    names: Vec<String>,
}

#[derive(Deserialize)]
struct Move {
    kind: String,
    name: String,
    by: i32,
}

#[derive(Deserialize)]
struct ShortCode {
    kind: String,
    name: String,
    code: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Host;

    #[test]
    fn usage_order_and_short_codes_as_the_page_sends_them() {
        let h = Host::new();
        let (fringes, units) = {
            let s = h.state().lock_doc(None).unwrap();
            let b = s.budget();
            let f: Vec<String> = b.fringes.keys().cloned().collect();
            let u: Vec<String> = b.units.keys().cloned().collect();
            (f, u)
        };
        assert!(!fringes.is_empty() && units.len() >= 2);
        let usage = h.dispatch("setup_usage", json!({})).unwrap();
        assert!(usage["fringes"][&fringes[0]]["lines"].is_u64());

        let reversed: Vec<&String> = units.iter().rev().collect();
        h.dispatch(
            "set_setup_order",
            json!({"kind": "units", "names": reversed}),
        )
        .unwrap();
        h.dispatch(
            "move_setup_item",
            json!({"kind": "unit", "name": units[0], "by": -1}),
        )
        .unwrap();
        h.dispatch(
            "set_short_code",
            json!({"kind": "fringe", "name": fringes[0], "code": "ZZ"}),
        )
        .unwrap();
        let s = h.state().lock_doc(None).unwrap();
        assert_eq!(
            s.budget().fringes[&fringes[0]].short_code.as_deref(),
            Some("ZZ")
        );
        assert_eq!(s.history().steps.len(), 3, "one undo step each");
    }
}
