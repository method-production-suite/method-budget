//! Find and replace's commands (src/find.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};
use crate::find::{self as f, FindQuery, Replaced, Target};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "find" => find(d),
        "replace" => replace(d),
        _ => return None,
    })
}

fn find(d: &Call<'_>) -> Result<Value, Error> {
    let a: Find = d.args()?;
    json(f::find(&*d.lock()?, &a.query)?)
}

fn replace(d: &Call<'_>) -> Result<Value, Error> {
    let a: Replace = d.args()?;
    let mut s = d.lock()?;
    let updated = f::replace(&mut s, &a.query, &a.targets, a.with.as_deref())?;
    json(Replaced {
        view: s.finish(None),
        updated,
    })
}

#[derive(Deserialize)]
struct Find {
    query: FindQuery,
}

#[derive(Deserialize)]
struct Replace {
    query: FindQuery,
    targets: Vec<Target>,
    with: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Host;

    #[test]
    fn find_then_replace_as_the_page_sends_them() {
        let h = Host::new();
        let name = h
            .state()
            .lock_doc(None)
            .unwrap()
            .budget()
            .categories()
            .flat_map(|c| &c.accounts)
            .next()
            .unwrap()
            .name
            .clone();
        let query = json!({"scope": "account", "text": name});
        let found = h.dispatch("find", json!({ "query": query })).unwrap();
        assert!(found["count"].as_u64().unwrap() >= 1);
        let targets: Vec<_> = found["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| json!({"row_id": t["row_id"], "account": t["account"], "field": t["field"]}))
            .collect();
        let v = h
            .dispatch(
                "replace",
                json!({"query": query, "targets": targets, "with": "Renamed"}),
            )
            .unwrap();
        assert_eq!(v["updated"].as_u64(), Some(targets.len() as u64));
        assert!(v.get("budget").is_some(), "the reply is a view too");
    }
}
