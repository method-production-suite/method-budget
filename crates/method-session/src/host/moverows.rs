//! Moving rows up or down one place (src/moverows.rs).

use method_core::RowId;
use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};
use crate::edit::EditResult;
use crate::moverows::{self as m, MovedView, Shifted};
use crate::session::Session;

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "move_rows" => moved(d, |s, a: Rows| m::move_rows(s, &a.row_ids, a.by)),
        "move_accounts" => moved(d, |s, a: Accounts| m::move_accounts(s, &a.codes, a.by)),
        "move_topsheet_rows" => moved(d, |s, a: Topsheet| {
            m::move_topsheet_rows(s, &a.indexes, a.by)
        }),
        _ => return None,
    })
}

/// Reads the arguments as `A`, runs the move and replies with the view and
/// what moved.
fn moved<A: serde::de::DeserializeOwned>(
    d: &Call<'_>,
    f: impl FnOnce(&mut Session, A) -> EditResult<Shifted>,
) -> Result<Value, Error> {
    let a: A = d.args()?;
    let mut s = d.lock()?;
    let shifted = f(&mut s, a)?;
    json(MovedView::new(s.finish(None), shifted))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rows {
    row_ids: Vec<RowId>,
    by: i32,
}

#[derive(Deserialize)]
struct Accounts {
    codes: Vec<String>,
    by: i32,
}

#[derive(Deserialize)]
struct Topsheet {
    indexes: Vec<usize>,
    by: i32,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Host;

    #[test]
    fn lines_accounts_and_topsheet_rows_move_as_the_page_sends_them() {
        let h = Host::new();
        // Two lines in the first account, and a second account after it.
        let (codes, rows) = {
            let mut s = h.state().lock_doc(None).unwrap();
            let c = s.budget().categories().next().unwrap();
            let (category, account) = (c.code.clone(), c.accounts[0].code.clone());
            s.add_detail(&account, None, "line").unwrap();
            s.add_detail(&account, None, "line").unwrap();
            let other = s
                .edit("Add", |b| {
                    crate::edit::add_account(
                        b,
                        &category,
                        &format!("{category}-99"),
                        "Second",
                        Some(&account),
                    )
                })
                .unwrap();
            let c = s.budget().categories().next().unwrap();
            let rows: Vec<u64> = c.accounts[0].details.iter().map(|d| d.id).collect();
            (vec![account, other], rows)
        };
        let v = h
            .dispatch("move_rows", json!({"rowIds": [rows[1]], "by": -1}))
            .unwrap();
        assert_eq!(v["moved"], json!([rows[1]]));
        assert_eq!(v["changed"], true);
        assert!(v.get("budget").is_some(), "the reply is a view too");
        let v = h
            .dispatch("move_accounts", json!({"codes": [codes[1]], "by": -1}))
            .unwrap();
        assert_eq!(v["moved"], json!([0]));
        let v = h
            .dispatch("move_topsheet_rows", json!({"indexes": [0], "by": -1}))
            .unwrap();
        assert_eq!(v["changed"], false, "the first row can't go higher");
    }
}
