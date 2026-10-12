//! The overtime helper's commands (src/overtime.rs). The page sends their
//! arguments in snake case.

use method_core::RowId;
use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};
use crate::overtime::{self as o, PatternDto};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "ot_calculate" => calculate(d),
        "ot_apply" => d.view_with(|s, a: Apply| {
            o::ot_apply(
                s,
                a.row_id,
                &a.pattern,
                &a.base_rate,
                &a.hours_per_day,
                a.days_per_week,
                &a.period,
                &a.write,
            )
        }),
        "ot_save_pattern" => {
            d.view_with(|s, a: Save| o::ot_save_pattern(s, &a.pattern, a.replacing.as_deref()))
        }
        "ot_delete_pattern" => d.view_with(|s, a: Name| o::ot_delete_pattern(s, &a.name)),
        _ => return None,
    })
}

fn calculate(d: &Call<'_>) -> Result<Value, Error> {
    let a: Calculate = d.args()?;
    json(o::ot_calculate(
        &a.pattern,
        &a.base_rate,
        &a.hours_per_day,
        a.days_per_week,
    )?)
}

#[derive(Deserialize)]
struct Calculate {
    pattern: PatternDto,
    base_rate: String,
    hours_per_day: String,
    days_per_week: u8,
}

#[derive(Deserialize)]
struct Apply {
    row_id: RowId,
    pattern: PatternDto,
    base_rate: String,
    hours_per_day: String,
    days_per_week: u8,
    period: String,
    write: String,
}

#[derive(Deserialize)]
struct Save {
    pattern: PatternDto,
    replacing: Option<String>,
}

#[derive(Deserialize)]
struct Name {
    name: String,
}

#[cfg(test)]
mod tests {
    use method_core::RowKind;
    use serde_json::json;

    use crate::Host;

    #[test]
    fn calculate_apply_and_patterns_as_the_page_sends_them() {
        let h = Host::new();
        let row = crate::edit::details(h.state().lock_doc(None).unwrap().budget())
            .find(|d| d.kind == RowKind::Line)
            .unwrap()
            .id;
        let pattern = json!({
            "name": "Crew 12s",
            "straight_hours": "8",
            "bands": [{"multiplier": "1.5", "hours": "4"}, {"multiplier": "2"}],
        });
        let numbers = json!({"pattern": pattern, "base_rate": "40", "hours_per_day": "12",
                             "days_per_week": 5});
        let r = h.dispatch("ot_calculate", numbers.clone()).unwrap();
        assert!(r.is_object());

        let mut a = numbers.clone();
        a["row_id"] = json!(row);
        a["period"] = json!("day");
        a["write"] = json!("rate");
        h.dispatch("ot_apply", a).unwrap();
        h.dispatch(
            "ot_save_pattern",
            json!({"pattern": pattern, "replacing": null}),
        )
        .unwrap();
        assert_eq!(
            h.state().lock_doc(None).unwrap().budget().ot_patterns.len(),
            1
        );
        h.dispatch("ot_delete_pattern", json!({"name": "Crew 12s"}))
            .unwrap();
        let s = h.state().lock_doc(None).unwrap();
        assert!(s.budget().ot_patterns.is_empty());
        assert_eq!(s.history().steps.len(), 3, "one undo step each");
    }
}
