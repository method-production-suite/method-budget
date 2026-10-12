//! Renumbering accounts to a new pattern (src/renumber.rs). The page sends
//! each width as its own argument, in snake case as for
//! `set_account_coding`.

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};
use crate::renumber as r;

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "renumber_preview" => preview(d),
        "renumber_accounts" => {
            d.view_with(|s, a: Pattern| r::renumber_accounts(s, a.widths(), a.separator.as_deref()))
        }
        _ => return None,
    })
}

fn preview(d: &Call<'_>) -> Result<Value, Error> {
    let a: Pattern = d.args()?;
    json(r::renumber_preview(
        &*d.lock()?,
        a.widths(),
        a.separator.as_deref(),
    )?)
}

#[derive(Deserialize)]
struct Pattern {
    category_width: u8,
    account_width: u8,
    sub_width: u8,
    set_width: u8,
    separator: Option<String>,
}

impl Pattern {
    fn widths(&self) -> [u8; 4] {
        [
            self.category_width,
            self.account_width,
            self.sub_width,
            self.set_width,
        ]
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Host;

    #[test]
    fn a_preview_then_the_renumbering_as_the_page_sends_them() {
        let h = Host::new();
        let q = json!({"category_width": 3, "account_width": 3, "sub_width": 0,
                       "set_width": 0, "separator": "."});
        let p = h.dispatch("renumber_preview", q.clone()).unwrap();
        assert_eq!(p["changes"], true);
        let first = p["accounts"][0]["new"].as_str().unwrap().to_string();
        h.dispatch("renumber_accounts", q).unwrap();
        let s = h.state().lock_doc(None).unwrap();
        let code = &s.budget().categories().next().unwrap().accounts[0].code;
        assert_eq!(*code, first);
    }
}
