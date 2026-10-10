//! The overtime helper (roadmap 3.10; decisions.md, entry 43): the commands
//! the tools panel's Overtime tab calls. The engine does the math
//! (`fwb_core::overtime`, exact decimals); these read what the form typed,
//! preview the result, write it into a line as one undo step, and keep the
//! budget's named overtime patterns.
//!
//! Decimals cross the bridge as strings, never JSON floats.

use fwb_core::overtime::{self, OtBand, OtInput, OtPattern, OtPer, OtResult, OtSlice, OtWrite};
use fwb_core::{Budget, Dec, RowId};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{edit_budget, edit_budget_as, AppState, DocId};
use crate::edit::EditResult;
use crate::view::View;

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
    s.trim().parse::<Dec>().map_err(|e| format!("{what}: {e}"))
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
                multiplier: dec(&format!("band {n} multiplier"), &b.multiplier)?,
                hours: opt_dec(&format!("band {n} hours"), b.hours.as_deref())?,
            });
        }
        // The last band covers the rest, whatever its hours box says.
        if let Some(last) = bands.last_mut() {
            last.hours = None;
        }
        let p = OtPattern {
            name: self.name.trim().to_string(),
            straight_hours: dec("straight-time hours", &self.straight_hours)?,
            bands,
            sixth_day: opt_dec("sixth-day multiplier", self.sixth_day.as_deref())?,
            seventh_day: opt_dec("seventh-day multiplier", self.seventh_day.as_deref())?,
        };
        if named {
            p.validate_all()
        } else {
            p.validate()
        }
        .map_err(|e| e.to_string())?;
        Ok(p)
    }
}

/// The numbers typed for one calculation.
pub fn input(base_rate: &str, hours_per_day: &str, days_per_week: u8) -> EditResult<OtInput> {
    let i = OtInput {
        base_rate: dec("base rate", base_rate)?,
        hours_per_day: dec("hours per day", hours_per_day)?,
        days_per_week,
    };
    i.validate().map_err(|e| e.to_string())?;
    Ok(i)
}

pub fn per(s: &str) -> EditResult<OtPer> {
    match s {
        "day" => Ok(OtPer::Day),
        "week" => Ok(OtPer::Week),
        _ => Err(format!("unknown period `{s}`: day or week")),
    }
}

pub fn write_mode(s: &str) -> EditResult<OtWrite> {
    match s {
        "rate" => Ok(OtWrite::Rate),
        "split" => Ok(OtWrite::Split),
        _ => Err(format!("unknown way to write `{s}`: rate or split")),
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
    let r = overtime::calculate(&p, &i).map_err(|e| e.to_string())?;
    Ok(result_dto(&p, &r))
}

/// Writes the result into line `row` (a pure budget edit).
#[allow(clippy::too_many_arguments)]
pub fn apply(
    b: &mut Budget,
    row: RowId,
    pattern: &PatternDto,
    base_rate: &str,
    hours_per_day: &str,
    days_per_week: u8,
    period: &str,
    write: &str,
) -> EditResult<Vec<RowId>> {
    let p = pattern.pattern(false)?;
    let i = input(base_rate, hours_per_day, days_per_week)?;
    overtime::apply(b, row, &p, &i, per(period)?, write_mode(write)?).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and both capabilities: the
// Overtime tab can be in the tools window)

/// What the form's numbers come to. Changes nothing.
#[tauri::command(rename_all = "snake_case")]
pub fn ot_calculate(
    pattern: PatternDto,
    base_rate: String,
    hours_per_day: String,
    days_per_week: u8,
) -> Result<ResultDto, String> {
    preview(&pattern, &base_rate, &hours_per_day, days_per_week)
}

/// Writes the overtime result into line `row_id` of budget `doc`, as its
/// rate or as one line per multiplier (`write`: "rate" or "split"), per day
/// or week (`period`), as one undo step.
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "snake_case")]
pub fn ot_apply<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_id: RowId,
    pattern: PatternDto,
    base_rate: String,
    hours_per_day: String,
    days_per_week: u8,
    period: String,
    write: String,
) -> Result<View, String> {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| match crate::history::row_place(b, row_id) {
            Some((code, line)) => format!("Overtime on {code} line {line}"),
            None => "Overtime".to_string(),
        },
        |b| {
            apply(
                b,
                row_id,
                &pattern,
                &base_rate,
                &hours_per_day,
                days_per_week,
                &period,
                &write,
            )
        },
    )
}

/// Saves a named overtime pattern in budget `doc`: a new one, or in place of
/// `replacing` (which may rename it). One undo step.
#[tauri::command(rename_all = "snake_case")]
pub fn ot_save_pattern<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    pattern: PatternDto,
    replacing: Option<String>,
) -> Result<View, String> {
    let label = format!("Save overtime pattern {}", pattern.name.trim());
    edit_budget(&window, &state, doc, label, |b| {
        let p = pattern.pattern(true)?;
        overtime::save_pattern(b, p, replacing.as_deref()).map_err(|e| e.to_string())
    })
}

/// Deletes the overtime pattern `name` from budget `doc`. One undo step.
#[tauri::command(rename_all = "snake_case")]
pub fn ot_delete_pattern<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> Result<View, String> {
    edit_budget(
        &window,
        &state,
        doc,
        format!("Delete overtime pattern {name}"),
        |b| {
            overtime::delete_pattern(b, &name)
                .map(|_| ())
                .map_err(|e| e.to_string())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;

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
            .find(|d| d.kind == fwb_core::RowKind::Line)
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

    #[test]
    fn writing_is_one_undo_step() {
        let mut s = Session::new();
        let id = first_line(s.budget());
        let before = s.budget().clone();
        let n = rows_in(s.budget(), id);
        let ids = s
            .edit("Overtime", |b| {
                apply(b, id, &crew(), "50", "14", 5, "day", "split")
            })
            .unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(rows_in(s.budget(), id), n + 2);
        s.undo();
        assert_eq!(s.budget(), &before);
        s.edit("Overtime", |b| {
            apply(b, id, &crew(), "50", "14", 5, "week", "rate")
        })
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
                &crew(),
                "50",
                "14",
                5,
                "day",
                "rate"
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
