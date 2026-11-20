//! The overtime helper (roadmap 3.10; decisions.md, entry 43): what the
//! tools panel's Overtime tab calls. The engine does the math
//! (`method_core::overtime`, exact decimals); these read what the form typed,
//! preview the result, write it into a line as one undo step, and keep the
//! budget's named overtime patterns.
//!
//! Decimals cross the bridge as strings, never JSON floats.

use method_core::overtime::{
    self, OtBand, OtError, OtInput, OtPattern, OtPer, OtResult, OtSlice, OtWrite,
};
use method_core::{Budget, Dec, RowId};
use serde::{Deserialize, Serialize};

use crate::edit::EditResult;
use crate::messages::{msg, word};
use crate::session::Session;

/// A band as the form sends it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BandDto {
    pub multiplier: String,
    /// Blank or missing on the last band (the rest of the hours).
    #[serde(default)]
    pub hours: Option<String>,
}

/// A pattern as the form sends it (the name is ignored by a preview).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternDto {
    #[serde(default)]
    pub name: String,
    pub straight_hours: String,
    #[serde(default)]
    pub bands: Vec<BandDto>,
    #[serde(default)]
    pub sixth_day: Option<String>,
    #[serde(default)]
    pub seventh_day: Option<String>,
}

fn dec(what: &str, s: &str) -> EditResult<Dec> {
    s.trim().parse::<Dec>().map_err(|e| {
        msg(
            "engine.edit.not_a_number",
            &[("what", &what), ("text", &s.trim()), ("why", &e)],
        )
    })
}

/// method-core's overtime refusal in the catalog's words (decision 94): the
/// same English it writes itself (tested), with a code the desktop can show
/// in the person's language.
pub(crate) fn ot_err(e: &OtError) -> String {
    use overtime::{MAX_OT_BANDS, MAX_OT_NAME_CHARS, MAX_OT_PATTERNS};
    match e {
        OtError::Name => msg("engine.overtime.name", &[("max", &MAX_OT_NAME_CHARS)]),
        OtError::Taken(n) => msg("engine.overtime.taken", &[("name", n)]),
        OtError::NotFound(n) => msg("engine.overtime.not_found", &[("name", n)]),
        OtError::TooManyPatterns => msg(
            "engine.overtime.too_many_patterns",
            &[("max", &MAX_OT_PATTERNS)],
        ),
        OtError::TooManyBands => msg("engine.overtime.too_many_bands", &[("max", &MAX_OT_BANDS)]),
        OtError::Value(what, rule) => match (*what, *rule) {
            ("the base rate", "can't be negative") => word("engine.overtime.base_rate_negative"),
            ("hours per day", "must be more than 0 and at most 24") => {
                word("engine.overtime.hours_per_day_range")
            }
            ("days per week", "must be 1 to 7") => word("engine.overtime.days_per_week_range"),
            ("straight-time hours", "must be 0 to 24") => {
                word("engine.overtime.straight_hours_range")
            }
            ("an overtime band's hours", "must be more than 0 and at most 24") => {
                word("engine.overtime.band_hours_range")
            }
            ("an overtime multiplier", "must be more than 0 and at most 10") => {
                word("engine.overtime.multiplier_range")
            }
            ("the sixth-day multiplier", "must be more than 0 and at most 10") => {
                word("engine.overtime.sixth_day_range")
            }
            ("the seventh-day multiplier", "must be more than 0 and at most 10") => {
                word("engine.overtime.seventh_day_range")
            }
            // A rule method-core adds later shows its own English until it
            // has a code (the test below lists every one there is).
            _ => e.to_string(),
        },
        OtError::BandHours(i) => msg("engine.overtime.band_hours_needed", &[("n", &(i + 1))]),
        OtError::Row(id) => msg("engine.overtime.line_gone", &[("id", id)]),
        OtError::NotALine => word("engine.overtime.not_a_line"),
        OtError::Overflow => word("engine.overtime.overflow"),
    }
}

fn opt_dec(what: &str, s: Option<&str>) -> EditResult<Option<Dec>> {
    match s.map(str::trim) {
        None | Some("") => Ok(None),
        Some(v) => dec(what, v).map(Some),
    }
}

impl PatternDto {
    /// The pattern, its numbers checked (the name too when `named`).
    pub fn pattern(&self, named: bool) -> EditResult<OtPattern> {
        let mut bands = Vec::with_capacity(self.bands.len());
        for (i, b) in self.bands.iter().enumerate() {
            let n = i + 1;
            bands.push(OtBand {
                multiplier: dec(
                    &msg("engine.what.ot_band_multiplier", &[("n", &n)]),
                    &b.multiplier,
                )?,
                hours: opt_dec(
                    &msg("engine.what.ot_band_hours", &[("n", &n)]),
                    b.hours.as_deref(),
                )?,
            });
        }
        // The last band covers the rest, whatever its hours box says.
        if let Some(last) = bands.last_mut() {
            last.hours = None;
        }
        let p = OtPattern {
            name: self.name.trim().to_string(),
            straight_hours: dec(&word("engine.what.ot_straight_hours"), &self.straight_hours)?,
            bands,
            sixth_day: opt_dec(
                &word("engine.what.ot_sixth_day_multiplier"),
                self.sixth_day.as_deref(),
            )?,
            seventh_day: opt_dec(
                &word("engine.what.ot_seventh_day_multiplier"),
                self.seventh_day.as_deref(),
            )?,
        };
        if named {
            p.validate_all()
        } else {
            p.validate()
        }
        .map_err(|e| ot_err(&e))?;
        Ok(p)
    }
}

/// The numbers typed for one calculation.
pub fn input(base_rate: &str, hours_per_day: &str, days_per_week: u8) -> EditResult<OtInput> {
    let i = OtInput {
        base_rate: dec(&word("engine.what.base_rate"), base_rate)?,
        hours_per_day: dec(&word("engine.what.hours_per_day"), hours_per_day)?,
        days_per_week,
    };
    i.validate().map_err(|e| ot_err(&e))?;
    Ok(i)
}

pub fn per(s: &str) -> EditResult<OtPer> {
    match s {
        "day" => Ok(OtPer::Day),
        "week" => Ok(OtPer::Week),
        _ => Err(msg("engine.overtime.unknown_period", &[("text", &s)])),
    }
}

pub fn write_mode(s: &str) -> EditResult<OtWrite> {
    match s {
        "rate" => Ok(OtWrite::Rate),
        "split" => Ok(OtWrite::Split),
        _ => Err(msg("engine.overtime.unknown_write", &[("text", &s)])),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SliceDto {
    pub multiplier: String,
    pub hours: String,
    /// Exact, not rounded.
    pub pay: String,
}

impl From<&OtSlice> for SliceDto {
    fn from(s: &OtSlice) -> SliceDto {
        SliceDto {
            multiplier: s.multiplier.to_string(),
            hours: s.hours.to_string(),
            pay: s.pay.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DayDto {
    /// "regular", "sixth" or "seventh".
    pub kind: &'static str,
    pub slices: Vec<SliceDto>,
    pub hours: String,
    pub equivalent_hours: String,
    pub pay: String,
}

impl From<&overtime::OtDay> for DayDto {
    fn from(d: &overtime::OtDay) -> DayDto {
        DayDto {
            kind: match d.kind {
                overtime::DayKind::Regular => "regular",
                overtime::DayKind::Sixth => "sixth",
                overtime::DayKind::Seventh => "seventh",
            },
            slices: d.slices.iter().map(SliceDto::from).collect(),
            hours: d.hours.to_string(),
            equivalent_hours: d.equivalent_hours.to_string(),
            pay: d.pay.to_string(),
        }
    }
}

/// A preview, every amount exact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResultDto {
    pub day: DayDto,
    pub week: Vec<DayDto>,
    pub week_slices: Vec<SliceDto>,
    pub week_hours: String,
    pub week_equivalent_hours: String,
    pub week_pay: String,
    /// Pay per hour worked, on average, per day and per week.
    pub day_hourly: Option<String>,
    pub week_hourly: Option<String>,
    /// The pattern in words.
    pub describe: String,
}

pub fn result_dto(p: &OtPattern, r: &OtResult) -> ResultDto {
    ResultDto {
        day: DayDto::from(&r.day),
        week: r.week.iter().map(DayDto::from).collect(),
        week_slices: r.week_slices.iter().map(SliceDto::from).collect(),
        week_hours: r.week_hours.to_string(),
        week_equivalent_hours: r.week_equivalent_hours.to_string(),
        week_pay: r.week_pay.to_string(),
        day_hourly: r.average_hourly(OtPer::Day).map(|d| d.to_string()),
        week_hourly: r.average_hourly(OtPer::Week).map(|d| d.to_string()),
        describe: p.describe(),
    }
}

/// What the form's numbers come to.
pub fn preview(
    pattern: &PatternDto,
    base_rate: &str,
    hours_per_day: &str,
    days_per_week: u8,
) -> EditResult<ResultDto> {
    let p = pattern.pattern(false)?;
    let i = input(base_rate, hours_per_day, days_per_week)?;
    let r = overtime::calculate(&p, &i).map_err(|e| ot_err(&e))?;
    Ok(result_dto(&p, &r))
}

/// What the Overtime form sends to write a result into a line: the pattern,
/// the three boxes it is worked out from, per day or week (`period`), and
/// as the line's rate or one line per multiplier (`write`: "rate" or
/// "split").
#[derive(Debug, Clone, Copy)]
pub struct ApplyForm<'a> {
    pub pattern: &'a PatternDto,
    pub base_rate: &'a str,
    pub hours_per_day: &'a str,
    pub days_per_week: u8,
    pub period: &'a str,
    pub write: &'a str,
}

/// Writes the result into line `row` (a pure budget edit).
pub fn apply(b: &mut Budget, row: RowId, form: &ApplyForm<'_>) -> EditResult<Vec<RowId>> {
    let p = form.pattern.pattern(false)?;
    let i = input(form.base_rate, form.hours_per_day, form.days_per_week)?;
    overtime::apply(b, row, &p, &i, per(form.period)?, write_mode(form.write)?)
        .map_err(|e| ot_err(&e))
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/overtime.rs and the host's src/host/overtime.rs
// call these)

/// What the form's numbers come to. Changes nothing.
pub fn ot_calculate(
    pattern: &PatternDto,
    base_rate: &str,
    hours_per_day: &str,
    days_per_week: u8,
) -> Result<ResultDto, String> {
    preview(pattern, base_rate, hours_per_day, days_per_week)
}

/// Writes the overtime result into line `row_id` as `form` says, as one
/// undo step.
pub fn ot_apply(s: &mut Session, row_id: RowId, form: &ApplyForm<'_>) -> EditResult<Vec<RowId>> {
    let label = match crate::history::row_place(s.budget(), row_id) {
        Some((code, line)) => msg(
            "engine.undo.overtime_at",
            &[("code", &code), ("line", &line)],
        ),
        None => word("engine.undo.overtime"),
    };
    s.edit(label, |b| apply(b, row_id, form))
}

/// Saves a named overtime pattern: a new one, or in place of `replacing`
/// (which may rename it). One undo step.
pub fn ot_save_pattern(
    s: &mut Session,
    pattern: &PatternDto,
    replacing: Option<&str>,
) -> EditResult {
    let label = msg(
        "engine.undo.save_overtime_pattern",
        &[("name", &pattern.name.trim())],
    );
    s.edit(label, |b| {
        let p = pattern.pattern(true)?;
        overtime::save_pattern(b, p, replacing).map_err(|e| ot_err(&e))
    })
}

/// Deletes the overtime pattern `name`. One undo step.
pub fn ot_delete_pattern(s: &mut Session, name: &str) -> EditResult {
    let label = msg("engine.undo.delete_overtime_pattern", &[("name", &name)]);
    s.edit(label, |b| {
        overtime::delete_pattern(b, name)
            .map(|_| ())
            .map_err(|e| ot_err(&e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;

    /// Every refusal method-core's overtime can give comes out in the same
    /// English, recognized as its own code (so the desktop can translate
    /// it), and none falls back to method-core's text.
    #[test]
    fn every_overtime_refusal_keeps_its_english_and_has_a_code() {
        let ten = "must be more than 0 and at most 10";
        let errors = [
            (OtError::Name, "engine.overtime.name"),
            (OtError::Taken("Crew 12s".into()), "engine.overtime.taken"),
            (
                OtError::NotFound("Crew 12s".into()),
                "engine.overtime.not_found",
            ),
            (
                OtError::TooManyPatterns,
                "engine.overtime.too_many_patterns",
            ),
            (OtError::TooManyBands, "engine.overtime.too_many_bands"),
            (
                OtError::Value("the base rate", "can't be negative"),
                "engine.overtime.base_rate_negative",
            ),
            (
                OtError::Value("hours per day", "must be more than 0 and at most 24"),
                "engine.overtime.hours_per_day_range",
            ),
            (
                OtError::Value("days per week", "must be 1 to 7"),
                "engine.overtime.days_per_week_range",
            ),
            (
                OtError::Value("straight-time hours", "must be 0 to 24"),
                "engine.overtime.straight_hours_range",
            ),
            (
                OtError::Value(
                    "an overtime band's hours",
                    "must be more than 0 and at most 24",
                ),
                "engine.overtime.band_hours_range",
            ),
            (
                OtError::Value("an overtime multiplier", ten),
                "engine.overtime.multiplier_range",
            ),
            (
                OtError::Value("the sixth-day multiplier", ten),
                "engine.overtime.sixth_day_range",
            ),
            (
                OtError::Value("the seventh-day multiplier", ten),
                "engine.overtime.seventh_day_range",
            ),
            (OtError::BandHours(1), "engine.overtime.band_hours_needed"),
            (OtError::Row(42), "engine.overtime.line_gone"),
            (OtError::NotALine, "engine.overtime.not_a_line"),
            (OtError::Overflow, "engine.overtime.overflow"),
        ];
        for (e, code) in errors {
            let text = ot_err(&e);
            assert_eq!(text, e.to_string(), "{code}");
            let r = crate::messages::recognize(&text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(r.code, code, "{text}");
        }
    }

    /// Each value rule method-core checks is one of those above: a pattern
    /// or input breaking each one gives a coded message.
    #[test]
    fn the_rules_method_core_checks_all_have_codes() {
        let mut bad_band = crew();
        bad_band.bands[0].multiplier = "11".into();
        let mut bad_hours = crew();
        bad_hours.straight_hours = "25".into();
        let mut bad_sixth = crew();
        bad_sixth.sixth_day = Some("0".into());
        let mut bad_seventh = crew();
        bad_seventh.seventh_day = Some("12".into());
        let mut texts = vec![];
        for p in [bad_band, bad_hours, bad_sixth, bad_seventh] {
            texts.push(p.pattern(false).unwrap_err());
        }
        for (rate, hours, days) in [("-1", "10", 5), ("50", "25", 5), ("50", "10", 8)] {
            texts.push(input(rate, hours, days).unwrap_err());
        }
        for t in texts {
            let r = crate::messages::recognize(&t).unwrap_or_else(|| panic!("{t}"));
            assert!(
                r.code.starts_with(concat!("engine", ".overtime.")),
                "{t}: {}",
                r.code
            );
        }
    }

    /// A box that doesn't read as a number says which box, in the
    /// catalog's words, with the box's name a noun the desktop translates.
    #[test]
    fn a_box_that_is_not_a_number_names_the_box() {
        let mut p = crew();
        p.bands[1].multiplier = "two".into();
        let t = p.pattern(false).unwrap_err();
        assert!(
            t.starts_with("band 2 multiplier `two` is not a number"),
            "{t}"
        );
        let r = crate::messages::recognize(&t).unwrap();
        assert_eq!(r.code, "engine.edit.not_a_number");
        let what = &r.params.iter().find(|(n, _)| *n == "what").unwrap().1;
        assert_eq!(
            crate::messages::recognize_nested(what).unwrap().code,
            "engine.what.ot_band_multiplier"
        );
        let e = per("month").unwrap_err();
        assert_eq!(
            crate::messages::recognize(&e).unwrap().code,
            "engine.overtime.unknown_period"
        );
        let e = write_mode("both").unwrap_err();
        assert_eq!(
            crate::messages::recognize(&e).unwrap().code,
            "engine.overtime.unknown_write"
        );
    }

    fn crew() -> PatternDto {
        serde_json::from_value(serde_json::json!({
            "name": "Crew 12s",
            "straight_hours": "8",
            "bands": [{"multiplier": "1.5", "hours": "4"}, {"multiplier": "2", "hours": "99"}],
            "sixth_day": "1.5",
            "seventh_day": ""
        }))
        .unwrap()
    }

    fn first_line(b: &Budget) -> RowId {
        b.categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|d| d.kind == method_core::RowKind::Line)
            .unwrap()
            .id
    }

    fn rows_in(b: &Budget, id: RowId) -> usize {
        b.categories()
            .flat_map(|c| &c.accounts)
            .find(|a| a.details.iter().any(|d| d.id == id))
            .unwrap()
            .details
            .len()
    }

    #[test]
    fn the_form_reads_strings_and_previews_exactly() {
        let p = crew().pattern(true).unwrap();
        assert_eq!(p.bands[1].hours, None, "the last band covers the rest");
        assert_eq!(p.seventh_day, None, "a blank box is none");
        let r = preview(&crew(), "33.35", "13", 6).unwrap();
        assert_eq!(r.day.pay, "533.6");
        assert_eq!(r.day.slices.len(), 3);
        assert_eq!(r.week.len(), 6);
        assert_eq!(r.week[5].kind, "sixth");
        assert!(r.describe.contains("1.5x after 8"), "{}", r.describe);
        let json = serde_json::to_value(&r).unwrap();
        assert!(json["day"]["pay"].is_string(), "decimals are strings");
        // Bad boxes say which.
        let e = preview(&crew(), "abc", "13", 6).unwrap_err();
        assert!(e.starts_with("base rate"), "{e}");
        let mut bad = crew();
        bad.bands[0].hours = Some(String::new());
        assert!(preview(&bad, "50", "13", 5).unwrap_err().contains("band 1"));
        assert!(preview(&crew(), "50", "13", 9).is_err());
        assert!(per("month").is_err() && write_mode("both").is_err());
    }

    /// The form at a 50 base rate, 14 hours a day, 5 days a week.
    fn form<'a>(pattern: &'a PatternDto, period: &'a str, write: &'a str) -> ApplyForm<'a> {
        ApplyForm {
            pattern,
            base_rate: "50",
            hours_per_day: "14",
            days_per_week: 5,
            period,
            write,
        }
    }

    #[test]
    fn writing_is_one_undo_step() {
        let mut s = Session::new();
        let id = first_line(s.budget());
        let before = s.budget().clone();
        let n = rows_in(s.budget(), id);
        let ids = s
            .edit("Overtime", |b| apply(b, id, &form(&crew(), "day", "split")))
            .unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(rows_in(s.budget(), id), n + 2);
        s.undo();
        assert_eq!(s.budget(), &before);
        s.edit("Overtime", |b| apply(b, id, &form(&crew(), "week", "rate")))
            .unwrap();
        let rate = &crate::edit::details(s.budget())
            .find(|d| d.id == id)
            .unwrap()
            .rate
            .0;
        assert_eq!(rate, "4500");
        assert!(s
            .edit("Overtime", |b| apply(
                b,
                999_999,
                &form(&crew(), "day", "rate")
            ))
            .unwrap_err()
            .contains("no longer in the budget"));
    }

    #[test]
    fn patterns_are_kept_in_the_budget_and_its_json() {
        let mut s = Session::new();
        s.edit("Save", |b| {
            overtime::save_pattern(b, crew().pattern(true)?, None).map_err(|e| e.to_string())
        })
        .unwrap();
        let v = s.view();
        assert_eq!(v.budget["ot_patterns"][0]["name"], "Crew 12s");
        let mut unnamed = crew();
        unnamed.name = "  ".into();
        assert!(unnamed.pattern(true).is_err());
        s.edit("Delete", |b| {
            overtime::delete_pattern(b, "Crew 12s")
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .unwrap();
        assert!(s.budget().ot_patterns.is_empty());
    }
}
