//! Explain This Number (src/explain.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};
use crate::explain::{self as e, TargetDto};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "explain_number" => explain_number(d),
        _ => return None,
    })
}

fn explain_number(d: &Call<'_>) -> Result<Value, Error> {
    let a: Explain = d.args()?;
    json(e::explain_number(&mut *d.lock()?, &a.target)?)
}

#[derive(Deserialize)]
struct Explain {
    target: TargetDto,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Host;

    #[test]
    fn the_grand_total_is_explained_as_the_page_asks() {
        let h = Host::new();
        let v = h
            .dispatch("explain_number", json!({"target": {"kind": "grand_total"}}))
            .unwrap();
        assert_eq!(v["root"]["amount"], v["reported"]);
        assert_eq!(v["target"], json!({"kind": "grand_total"}));
        assert!(h
            .dispatch("explain_number", json!({"target": {"kind": "nope"}}))
            .is_err());
    }
}
