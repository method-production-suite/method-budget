//! File > Import Schedule Days… (src/schedule_days.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error, FileIn};
use crate::schedule_days::{self as sd, Imported, Pick, ScheduleSources};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{doc, file}`: the days file comes in as `file` (the desktop shows
        // an Open dialog). The review with the default picks, the file kept
        // by the reply's token.
        "schedule_days_pick" => (|| {
            let a: Picked = d.args()?;
            let b = d.lock()?.budget().clone();
            let sources = d.kept::<ScheduleSources>();
            json(sd::schedule_days_pick(
                &b,
                &sources,
                a.file.name,
                &a.file.bytes,
            )?)
        })(),
        "schedule_days_review" => (|| {
            let a: Picks = d.args()?;
            let b = d.lock()?.budget().clone();
            let sources = d.kept::<ScheduleSources>();
            json(sd::schedule_days_review(
                &b,
                &sources,
                a.token,
                a.picks.as_deref(),
            )?)
        })(),
        "schedule_days_apply" => (|| {
            let a: Picks = d.args()?;
            let sources = d.kept::<ScheduleSources>();
            let (id, message) =
                sd::schedule_days_apply(d.state(), &sources, d.doc(), a.token, a.picks.as_deref())?;
            let view = d.state().lock_doc(Some(id))?.finish(None);
            json(Imported { view, message })
        })(),
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Picked {
    file: FileIn,
}

#[derive(Deserialize)]
struct Picks {
    token: u64,
    /// `null` or left out: the review's own picks.
    #[serde(default)]
    picks: Option<Vec<Pick>>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::test_files::{call, file};
    use crate::Host;

    const DAYS: &[u8] =
        include_bytes!("../../../../fixtures/field-station/field-station.budget.json");

    #[test]
    fn a_picked_days_file_is_reviewed_and_applied_as_one_step() {
        let h = Host::new();
        let r = call(
            &h,
            "schedule_days_pick",
            json!({"doc": null, "file": file("field-station.budget.json", DAYS)}),
        );
        assert_eq!(r["file"], "field-station.budget.json");
        assert_eq!(r["schedule"], "Field Station");
        assert_eq!(r["shoot_days"], 18);
        let total = &r["figures"][0];
        assert_eq!(
            (&total["global"], &total["picked"], &total["new"]),
            (&json!("SHOOT_DAYS"), &json!(true), &json!("18"))
        );
        let picks = json!([
            {"key": "total"},
            {"key": "location:Cold Spring field station", "global": "STATION_DAYS"},
        ]);
        let r2 = call(
            &h,
            "schedule_days_review",
            json!({"token": r["token"], "picks": picks}),
        );
        assert_eq!(
            (r2["adds"].clone(), r2["updates"].clone()),
            (json!(1), json!(1))
        );
        let done = call(
            &h,
            "schedule_days_apply",
            json!({"token": r["token"], "picks": picks}),
        );
        assert_eq!(done["message"], "Set 2 globals from the schedule (1 new).");
        assert_eq!(done["view"]["can_undo"], true);
        assert_eq!(done["view"]["budget"]["globals"]["STATION_DAYS"], "12");
        call(&h, "undo", json!({}));
        assert!(h
            .dispatch(
                "schedule_days_pick",
                json!({"file": file("x.json", b"{\"a\": 1}")})
            )
            .unwrap_err()
            .message()
            .contains("isn't a schedule days file"));
    }
}
