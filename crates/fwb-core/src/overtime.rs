//! Overtime helper for labor lines (roadmap 3.10; decisions.md, entry 43).
//!
//! A small calculator the user drives: a straight-time hourly rate, the
//! hours worked a day, how many of them are straight time, overtime bands
//! ("then 4 hours at 1.5x, then the rest at 2x") and optional sixth and
//! seventh day multipliers. It gives what a day (or a week) comes to and can
//! write that into a line: as the line's rate, or as one line per multiplier
//! (straight time and each overtime rate). No union or guild rules are
//! shipped: every number is the user's, and patterns they save are kept in
//! the budget by name ([`crate::Budget::ot_patterns`]; tools, not content,
//! like tag presets: versions don't carry them).
//!
//! Exact decimal math throughout. Nothing is rounded here: a rate is written
//! as the exact decimal, so the line itself rounds once, as every line does
//! (decisions.md, entry 2).

use std::fmt;

use crate::decimal::Dec;
use crate::model::{Budget, Detail, Formula, RowId, RowKind};

/// Most saved patterns a budget keeps.
pub const MAX_OT_PATTERNS: usize = 100;
/// Most overtime bands in a pattern.
pub const MAX_OT_BANDS: usize = 8;
/// Longest pattern name, in characters.
pub const MAX_OT_NAME_CHARS: usize = 80;
/// Largest multiplier a band or a day may have.
pub const MAX_MULTIPLIER: i64 = 10;
/// Hours in a day.
pub const DAY_HOURS: i64 = 24;

/// One overtime band: `hours` hours (after the straight time and the bands
/// before it) paid at `multiplier` times the straight rate. The last band
/// has no `hours`: it covers every hour left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtBand {
    pub multiplier: Dec,
    pub hours: Option<Dec>,
}

/// A named overtime pattern: straight time, then the bands, and the
/// multipliers for a sixth and a seventh day worked in a week (every hour of
/// such a day is paid at least that multiple; an overtime band above it
/// still wins).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtPattern {
    pub name: String,
    /// Hours a day paid at straight time before overtime starts.
    pub straight_hours: Dec,
    pub bands: Vec<OtBand>,
    pub sixth_day: Option<Dec>,
    pub seventh_day: Option<Dec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OtError {
    /// The name is blank or longer than [`MAX_OT_NAME_CHARS`].
    Name,
    /// Another pattern already has this name.
    Taken(String),
    /// No pattern has this name.
    NotFound(String),
    TooManyPatterns,
    TooManyBands,
    /// A value out of range: what, and the rule.
    Value(&'static str, &'static str),
    /// Only the last band covers "the rest"; every other band needs hours.
    BandHours(usize),
    /// The row doesn't exist, or isn't a costed line.
    Row(RowId),
    NotALine,
    Overflow,
}

impl fmt::Display for OtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OtError::Name => write!(
                f,
                "an overtime pattern needs a name of 1 to {MAX_OT_NAME_CHARS} characters"
            ),
            OtError::Taken(n) => write!(f, "there is already an overtime pattern named {n}"),
            OtError::NotFound(n) => write!(f, "there is no overtime pattern named {n}"),
            OtError::TooManyPatterns => {
                write!(
                    f,
                    "a budget keeps at most {MAX_OT_PATTERNS} overtime patterns"
                )
            }
            OtError::TooManyBands => {
                write!(f, "a pattern has at most {MAX_OT_BANDS} overtime bands")
            }
            OtError::Value(what, rule) => write!(f, "{what} {rule}"),
            OtError::BandHours(i) => write!(
                f,
                "overtime band {} needs its hours (only the last one covers the rest)",
                i + 1
            ),
            OtError::Row(id) => write!(f, "line {id} is no longer in the budget"),
            OtError::NotALine => f.write_str(
                "overtime goes on a costed line, not a subtotal, adjustment or separator",
            ),
            OtError::Overflow => f.write_str("a number is too large"),
        }
    }
}

impl std::error::Error for OtError {}

fn in_range(
    v: Dec,
    lo: Dec,
    hi: Dec,
    what: &'static str,
    rule: &'static str,
) -> Result<(), OtError> {
    if v < lo || v > hi {
        Err(OtError::Value(what, rule))
    } else {
        Ok(())
    }
}

fn multiplier_ok(m: Dec, what: &'static str) -> Result<(), OtError> {
    if m <= Dec::ZERO || m > Dec::from_int(MAX_MULTIPLIER) {
        return Err(OtError::Value(what, "must be more than 0 and at most 10"));
    }
    Ok(())
}

impl OtPattern {
    /// Checks the numbers (not the name).
    pub fn validate(&self) -> Result<(), OtError> {
        in_range(
            self.straight_hours,
            Dec::ZERO,
            Dec::from_int(DAY_HOURS),
            "straight-time hours",
            "must be 0 to 24",
        )?;
        if self.bands.len() > MAX_OT_BANDS {
            return Err(OtError::TooManyBands);
        }
        let last = self.bands.len().saturating_sub(1);
        for (i, band) in self.bands.iter().enumerate() {
            multiplier_ok(band.multiplier, "an overtime multiplier")?;
            match band.hours {
                Some(h) if i < last || h > Dec::ZERO => in_range(
                    h,
                    Dec::from_cents(1),
                    Dec::from_int(DAY_HOURS),
                    "an overtime band's hours",
                    "must be more than 0 and at most 24",
                )?,
                None if i < last => return Err(OtError::BandHours(i)),
                _ => {}
            }
        }
        if let Some(m) = self.sixth_day {
            multiplier_ok(m, "the sixth-day multiplier")?;
        }
        if let Some(m) = self.seventh_day {
            multiplier_ok(m, "the seventh-day multiplier")?;
        }
        Ok(())
    }

    /// Checks the name and the numbers.
    pub fn validate_all(&self) -> Result<(), OtError> {
        let n = self.name.trim();
        if n.is_empty() || n.chars().count() > MAX_OT_NAME_CHARS || n != self.name {
            return Err(OtError::Name);
        }
        self.validate()
    }

    /// Where each band starts and its multiplier: "after 8 at 1.5x, after
    /// 12 at 2x" as `[(8, 1.5), (12, 2)]`. `None` on overflow.
    pub fn thresholds(&self) -> Option<Vec<(Dec, Dec)>> {
        let mut at = self.straight_hours;
        let mut out = Vec::with_capacity(self.bands.len());
        for b in &self.bands {
            out.push((at, b.multiplier));
            if let Some(h) = b.hours {
                at = at.checked_add(h)?;
            }
        }
        Some(out)
    }

    /// The pattern in words: "8 hours straight time, then 1.5x after 8,
    /// 2x after 12; sixth day 1.5x".
    pub fn describe(&self) -> String {
        let mut s = format!("{} hours straight time", self.straight_hours);
        if let Some(t) = self.thresholds() {
            let parts: Vec<String> = t.iter().map(|(at, m)| format!("{m}x after {at}")).collect();
            if !parts.is_empty() {
                s.push_str(", then ");
                s.push_str(&parts.join(", "));
            }
        }
        if let Some(m) = self.sixth_day {
            s.push_str(&format!("; sixth day {m}x"));
        }
        if let Some(m) = self.seventh_day {
            s.push_str(&format!("; seventh day {m}x"));
        }
        s
    }
}

/// What a day or a week is worked out for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtPer {
    /// One regular day (no sixth or seventh day multiplier).
    Day,
    /// A week of `days_per_week` days.
    Week,
}

/// The numbers the user types for one calculation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtInput {
    /// The straight-time hourly rate.
    pub base_rate: Dec,
    pub hours_per_day: Dec,
    /// Days worked a week, 1 to 7: days 6 and 7 get the sixth and seventh
    /// day multipliers.
    pub days_per_week: u8,
}

impl OtInput {
    pub fn validate(&self) -> Result<(), OtError> {
        if self.base_rate < Dec::ZERO {
            return Err(OtError::Value("the base rate", "can't be negative"));
        }
        in_range(
            self.hours_per_day,
            Dec::from_cents(1),
            Dec::from_int(DAY_HOURS),
            "hours per day",
            "must be more than 0 and at most 24",
        )?;
        if !(1..=7).contains(&self.days_per_week) {
            return Err(OtError::Value("days per week", "must be 1 to 7"));
        }
        Ok(())
    }
}

/// Hours paid at one multiplier, and what they come to (exact).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtSlice {
    pub multiplier: Dec,
    pub hours: Dec,
    /// hours x base rate x multiplier, not rounded.
    pub pay: Dec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayKind {
    Regular,
    Sixth,
    Seventh,
}

/// One day worked out: its slices by multiplier, lowest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtDay {
    pub kind: DayKind,
    pub slices: Vec<OtSlice>,
    pub hours: Dec,
    /// Hours times their multipliers: what the day pays in straight hours.
    pub equivalent_hours: Dec,
    pub pay: Dec,
}

/// A calculation's result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtResult {
    /// One regular day.
    pub day: OtDay,
    /// Every day of the week, in order.
    pub week: Vec<OtDay>,
    /// The week's hours merged by multiplier, lowest first.
    pub week_slices: Vec<OtSlice>,
    pub week_hours: Dec,
    pub week_equivalent_hours: Dec,
    pub week_pay: Dec,
}

impl OtResult {
    /// The slices and what they come to, per day or per week.
    pub fn per(&self, per: OtPer) -> (&[OtSlice], Dec) {
        match per {
            OtPer::Day => (&self.day.slices, self.day.pay),
            OtPer::Week => (&self.week_slices, self.week_pay),
        }
    }

    /// Pay per hour worked, on average (9 places).
    pub fn average_hourly(&self, per: OtPer) -> Option<Dec> {
        let (pay, hours) = match per {
            OtPer::Day => (self.day.pay, self.day.hours),
            OtPer::Week => (self.week_pay, self.week_hours),
        };
        pay.checked_div(hours)
    }
}

/// Adds `hours` at `multiplier` to `slices`, merging an equal multiplier.
fn add_slice(slices: &mut Vec<OtSlice>, multiplier: Dec, hours: Dec, rate: Dec) -> Option<()> {
    if hours <= Dec::ZERO {
        return Some(());
    }
    let pay = rate.checked_mul(multiplier)?.checked_mul(hours)?;
    match slices.iter_mut().find(|s| s.multiplier == multiplier) {
        Some(s) => {
            s.hours = s.hours.checked_add(hours)?;
            s.pay = s.pay.checked_add(pay)?;
        }
        None => slices.push(OtSlice {
            multiplier,
            hours,
            pay,
        }),
    }
    Some(())
}

fn finish_day(kind: DayKind, mut slices: Vec<OtSlice>) -> Option<OtDay> {
    slices.sort_by_key(|s| s.multiplier);
    let mut hours = Dec::ZERO;
    let mut eq = Dec::ZERO;
    let mut pay = Dec::ZERO;
    for s in &slices {
        hours = hours.checked_add(s.hours)?;
        eq = eq.checked_add(s.hours.checked_mul(s.multiplier)?)?;
        pay = pay.checked_add(s.pay)?;
    }
    Some(OtDay {
        kind,
        slices,
        hours,
        equivalent_hours: eq,
        pay,
    })
}

/// One day: straight time, then each band, each hour paid at least `floor`
/// times the rate (a sixth or seventh day).
fn day(p: &OtPattern, input: &OtInput, kind: DayKind, floor: Dec) -> Option<OtDay> {
    let rate = input.base_rate;
    let mut left = input.hours_per_day;
    let mut slices = Vec::new();
    // Takes up to `want` hours (all that are left for `None`) off `left`.
    let take = |left: &mut Dec, want: Option<Dec>| -> Option<Dec> {
        let h = match want {
            Some(w) => w.min(*left),
            None => *left,
        };
        *left = left.checked_sub(h)?;
        Some(h)
    };
    let straight = take(&mut left, Some(p.straight_hours))?;
    add_slice(&mut slices, Dec::ONE.max(floor), straight, rate)?;
    for b in &p.bands {
        if left <= Dec::ZERO {
            break;
        }
        let h = take(&mut left, b.hours)?;
        add_slice(&mut slices, b.multiplier.max(floor), h, rate)?;
    }
    // Hours past every band (the last band had hours): its multiplier goes on.
    if left > Dec::ZERO {
        let m = p.bands.last().map_or(Dec::ONE, |b| b.multiplier);
        add_slice(&mut slices, m.max(floor), left, rate)?;
    }
    finish_day(kind, slices)
}

/// Works out a day and a week of `input` under `p`.
pub fn calculate(p: &OtPattern, input: &OtInput) -> Result<OtResult, OtError> {
    p.validate()?;
    input.validate()?;
    let ovf = || OtError::Overflow;
    let regular = day(p, input, DayKind::Regular, Dec::ZERO).ok_or_else(ovf)?;
    let mut week = Vec::with_capacity(usize::from(input.days_per_week));
    for n in 1..=input.days_per_week {
        let (kind, floor) = match (n, p.sixth_day, p.seventh_day) {
            (6, Some(m), _) => (DayKind::Sixth, m),
            (7, _, Some(m)) => (DayKind::Seventh, m),
            _ => (DayKind::Regular, Dec::ZERO),
        };
        week.push(if kind == DayKind::Regular {
            regular.clone()
        } else {
            day(p, input, kind, floor).ok_or_else(ovf)?
        });
    }
    let mut merged = Vec::new();
    for d in &week {
        for s in &d.slices {
            match merged
                .iter_mut()
                .find(|m: &&mut OtSlice| m.multiplier == s.multiplier)
            {
                Some(m) => {
                    m.hours = m.hours.checked_add(s.hours).ok_or_else(ovf)?;
                    m.pay = m.pay.checked_add(s.pay).ok_or_else(ovf)?;
                }
                None => merged.push(s.clone()),
            }
        }
    }
    let total = finish_day(DayKind::Regular, merged).ok_or_else(ovf)?;
    Ok(OtResult {
        day: regular,
        week,
        week_slices: total.slices,
        week_hours: total.hours,
        week_equivalent_hours: total.equivalent_hours,
        week_pay: total.pay,
    })
}

// ---------------------------------------------------------------------------
// Writing into a line

/// How the result goes into the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtWrite {
    /// The line's rate becomes the day's (or week's) pay, exactly.
    Rate,
    /// The line becomes one line per multiplier: straight time, then each
    /// overtime rate, each with that part of the pay as its rate.
    Split,
}

/// "straight time, 8 hrs" or "1.5x, 4 hrs".
fn slice_words(s: &OtSlice) -> String {
    let unit = if s.hours == Dec::ONE { "hr" } else { "hrs" };
    if s.multiplier == Dec::ONE {
        format!("straight time, {} {unit}", s.hours)
    } else {
        format!("{}x, {} {unit}", s.multiplier, s.hours)
    }
}

/// Writes the result for `input` under `p` into line `row`, per day or
/// week. Returns the ids of the lines it ends up as (the line's own id
/// first). Splitting keeps the line's amount, units, X, 4X and tags on every
/// part, its notes on the first, and puts the parts in one fringe range
/// (the line's own, else a new one) so percent fringe cutoffs still treat
/// them as one person.
pub fn apply(
    b: &mut Budget,
    row: RowId,
    p: &OtPattern,
    input: &OtInput,
    per: OtPer,
    write: OtWrite,
) -> Result<Vec<RowId>, OtError> {
    let result = calculate(p, input)?;
    let (slices, pay) = result.per(per);
    let next_id = b.highest_row_id().unwrap_or(0);
    let next_range = b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .filter_map(|d| d.fringe_range)
        .max()
        .map_or(1, |m| m.saturating_add(1));
    let (details, at) = b
        .categories_mut()
        .flat_map(|c| c.accounts.iter_mut())
        .find_map(|a| {
            let i = a.details.iter().position(|d| d.id == row)?;
            Some((&mut a.details, i))
        })
        .ok_or(OtError::Row(row))?;
    if details[at].kind != RowKind::Line {
        return Err(OtError::NotALine);
    }
    let parts: Vec<&OtSlice> = slices.iter().filter(|s| s.hours > Dec::ZERO).collect();
    if write == OtWrite::Rate || parts.len() < 2 {
        details[at].rate = Formula::from(pay);
        return Ok(vec![row]);
    }
    let original = details[at].clone();
    let base = original.description.trim().to_string();
    let range = original.fringe_range.or(Some(next_range));
    let mut ids = Vec::with_capacity(parts.len());
    let mut new_rows: Vec<Detail> = Vec::with_capacity(parts.len());
    for (k, s) in parts.iter().enumerate() {
        let id = if k == 0 {
            row
        } else {
            next_id.checked_add(k as u64).ok_or(OtError::Overflow)?
        };
        let description = if base.is_empty() {
            slice_words(s)
        } else {
            format!("{base}: {}", slice_words(s))
        };
        let mut d = Detail {
            id,
            description,
            rate: Formula::from(s.pay),
            fringe_range: range,
            ..original.clone()
        };
        if k > 0 {
            d.notes.clear();
        }
        ids.push(id);
        new_rows.push(d);
    }
    details.splice(at..=at, new_rows);
    Ok(ids)
}

// ---------------------------------------------------------------------------
// Saved patterns

/// Saves `p` under its name: a new pattern at the end, or (with
/// `replacing`) in place of the pattern of that name, which may rename it.
pub fn save_pattern(b: &mut Budget, p: OtPattern, replacing: Option<&str>) -> Result<(), OtError> {
    p.validate_all()?;
    let clash = b.ot_patterns.iter().position(|x| x.name == p.name);
    match replacing {
        Some(old) => {
            let i = b
                .ot_patterns
                .iter()
                .position(|x| x.name == old)
                .ok_or_else(|| OtError::NotFound(old.to_string()))?;
            if clash.is_some_and(|c| c != i) {
                return Err(OtError::Taken(p.name));
            }
            b.ot_patterns[i] = p;
        }
        None => {
            if clash.is_some() {
                return Err(OtError::Taken(p.name));
            }
            if b.ot_patterns.len() >= MAX_OT_PATTERNS {
                return Err(OtError::TooManyPatterns);
            }
            b.ot_patterns.push(p);
        }
    }
    Ok(())
}

/// Removes the pattern named `name`.
pub fn delete_pattern(b: &mut Budget, name: &str) -> Result<OtPattern, OtError> {
    let i = b
        .ot_patterns
        .iter()
        .position(|x| x.name == name)
        .ok_or_else(|| OtError::NotFound(name.to_string()))?;
    Ok(b.ot_patterns.remove(i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{recalc, RecalcOptions};
    use crate::decimal::mul_cents;
    use crate::model::{Account, Category, Fringe, FringeKind, TopsheetRow};

    fn d(s: &str) -> Dec {
        s.parse().unwrap()
    }

    /// 8 straight, then 4 at 1.5x, then the rest at 2x (after 8 and 12).
    fn standard() -> OtPattern {
        OtPattern {
            name: "Ten-hour crew".into(),
            straight_hours: d("8"),
            bands: vec![
                OtBand {
                    multiplier: d("1.5"),
                    hours: Some(d("4")),
                },
                OtBand {
                    multiplier: d("2"),
                    hours: None,
                },
            ],
            sixth_day: Some(d("1.5")),
            seventh_day: Some(d("2")),
        }
    }

    fn input(rate: &str, hours: &str, days: u8) -> OtInput {
        OtInput {
            base_rate: d(rate),
            hours_per_day: d(hours),
            days_per_week: days,
        }
    }

    fn slices(day: &OtDay) -> Vec<(String, String, String)> {
        day.slices
            .iter()
            .map(|s| {
                (
                    s.multiplier.to_string(),
                    s.hours.to_string(),
                    s.pay.to_string(),
                )
            })
            .collect()
    }

    fn t(m: &str, h: &str, p: &str) -> (String, String, String) {
        (m.into(), h.into(), p.into())
    }

    #[test]
    fn a_day_splits_into_bands_exactly() {
        let r = calculate(&standard(), &input("50", "14", 5)).unwrap();
        assert_eq!(
            slices(&r.day),
            [t("1", "8", "400"), t("1.5", "4", "300"), t("2", "2", "200")]
        );
        assert_eq!(r.day.pay, d("900"));
        assert_eq!(r.day.equivalent_hours, d("18"));
        assert_eq!(r.week_pay, d("4500"));
        assert_eq!(r.average_hourly(OtPer::Day), Some(d("64.285714286")));
        assert_eq!(
            standard().thresholds().unwrap(),
            [(d("8"), d("1.5")), (d("12"), d("2"))]
        );
        assert_eq!(
            standard().describe(),
            "8 hours straight time, then 1.5x after 8, 2x after 12; sixth day 1.5x; seventh day 2x"
        );
    }

    #[test]
    fn short_days_and_fractional_cents_stay_exact() {
        // 33.35 an hour: 1.5x is 50.025, never rounded here.
        let r = calculate(&standard(), &input("33.35", "9", 5)).unwrap();
        assert_eq!(
            slices(&r.day),
            [t("1", "8", "266.8"), t("1.5", "1", "50.025")]
        );
        assert_eq!(r.day.pay, d("316.825"));
        let r = calculate(&standard(), &input("33.35", "6.5", 5)).unwrap();
        assert_eq!(slices(&r.day), [t("1", "6.5", "216.775")]);
        // Quarter hours.
        let r = calculate(&standard(), &input("41.17", "12.25", 5)).unwrap();
        // 8 x 41.17 + 4 x 61.755 + 0.25 x 82.34.
        assert_eq!(r.day.pay, d("596.965"));
    }

    #[test]
    fn sixth_and_seventh_days_raise_every_hour_to_their_multiple() {
        let r = calculate(&standard(), &input("50", "14", 7)).unwrap();
        let six = &r.week[5];
        assert_eq!(six.kind, DayKind::Sixth);
        // 12 hours at 1.5x (straight and the first band), 2 at 2x.
        assert_eq!(slices(six), [t("1.5", "12", "900"), t("2", "2", "200")]);
        let seven = &r.week[6];
        assert_eq!(slices(seven), [t("2", "14", "1400")]);
        // Five regular days of 900, then 1,100 and 1,400.
        assert_eq!(r.week_pay, d("7000"));
        assert_eq!(
            r.week_slices
                .iter()
                .map(|s| (s.multiplier.to_string(), s.hours.to_string()))
                .collect::<Vec<_>>(),
            [
                ("1".to_string(), "40".to_string()),
                ("1.5".into(), "32".into()),
                ("2".into(), "26".into())
            ]
        );
        // Without day multipliers, days six and seven are regular days.
        let p = OtPattern {
            sixth_day: None,
            seventh_day: None,
            ..standard()
        };
        let r = calculate(&p, &input("50", "14", 7)).unwrap();
        assert_eq!(r.week_pay, d("6300"));
        assert!(r.week.iter().all(|d| d.kind == DayKind::Regular));
    }

    #[test]
    fn hours_past_the_last_band_keep_its_multiplier() {
        let p = OtPattern {
            bands: vec![OtBand {
                multiplier: d("1.5"),
                hours: Some(d("2")),
            }],
            ..standard()
        };
        let r = calculate(&p, &input("10", "13", 1)).unwrap();
        assert_eq!(slices(&r.day), [t("1", "8", "80"), t("1.5", "5", "75")]);
        let none = OtPattern {
            bands: Vec::new(),
            ..standard()
        };
        let r = calculate(&none, &input("10", "13", 1)).unwrap();
        assert_eq!(slices(&r.day), [t("1", "13", "130")]);
    }

    #[test]
    fn bad_numbers_are_refused() {
        let mut p = standard();
        p.bands[0].hours = None;
        assert_eq!(p.validate(), Err(OtError::BandHours(0)));
        let mut p = standard();
        p.bands[1].multiplier = Dec::ZERO;
        assert!(matches!(p.validate(), Err(OtError::Value(..))));
        let mut p = standard();
        p.straight_hours = d("25");
        assert!(p.validate().is_err());
        let mut p = standard();
        p.sixth_day = Some(d("11"));
        assert!(p.validate().is_err());
        let mut p = standard();
        p.bands = vec![p.bands[0].clone(); MAX_OT_BANDS + 1];
        assert_eq!(p.validate(), Err(OtError::TooManyBands));
        assert!(calculate(&standard(), &input("50", "0", 5)).is_err());
        assert!(calculate(&standard(), &input("50", "8", 8)).is_err());
        assert!(calculate(&standard(), &input("-1", "8", 5)).is_err());
        let mut p = standard();
        p.name = " padded".into();
        assert_eq!(p.validate_all(), Err(OtError::Name));
    }

    fn budget() -> Budget {
        let mut b = Budget::new("OT", "USD");
        let mut grip = Detail::line(1, "Key grip", 20, "Day", 0);
        grip.fringes.insert("PEN".into());
        b.fringes.insert(
            "PEN".into(),
            Fringe {
                short_code: None,
                description: String::new(),
                rate: "10".into(),
                kind: FringeKind::Percent {
                    cutoff: Some(1_000_000),
                },
            },
        );
        b.topsheet.push(TopsheetRow::Category(Category {
            code: "20".into(),
            name: "Crew".into(),
            notes: Vec::new(),
            accounts: vec![Account {
                code: "20-01".into(),
                name: "Grip".into(),
                notes: Vec::new(),
                details: vec![grip, Detail::line(2, "Next", 1, "Flat", 5)],
            }],
        }));
        b
    }

    fn rows(b: &Budget) -> &[Detail] {
        &b.categories().next().unwrap().accounts[0].details
    }

    #[test]
    fn writing_the_rate_rounds_once_on_the_line() {
        let mut b = budget();
        let ids = apply(
            &mut b,
            1,
            &standard(),
            &input("33.35", "9", 5),
            OtPer::Day,
            OtWrite::Rate,
        )
        .unwrap();
        assert_eq!(ids, [1]);
        assert_eq!(rows(&b)[0].rate.0, "316.825");
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        // 20 days x 316.825 = 6,336.50 exactly: one rounding, at the line.
        assert_eq!(r.categories[0].accounts[0].rows[0].total, 633_650);
    }

    #[test]
    fn splitting_makes_one_line_per_multiplier_in_one_range() {
        let mut b = budget();
        let before_total = {
            let mut c = b.clone();
            apply(
                &mut c,
                1,
                &standard(),
                &input("33.35", "13", 5),
                OtPer::Day,
                OtWrite::Rate,
            )
            .unwrap();
            recalc(&c, &RecalcOptions::default()).unwrap().categories[0].total
        };
        let ids = apply(
            &mut b,
            1,
            &standard(),
            &input("33.35", "13", 5),
            OtPer::Day,
            OtWrite::Split,
        )
        .unwrap();
        assert_eq!(ids, [1, 3, 4]);
        let ds = rows(&b);
        assert_eq!(ds.len(), 4);
        assert_eq!(ds[0].description, "Key grip: straight time, 8 hrs");
        assert_eq!(ds[1].description, "Key grip: 1.5x, 4 hrs");
        assert_eq!(ds[2].description, "Key grip: 2x, 1 hr");
        assert_eq!(ds[3].id, 2, "the next line stays after them");
        assert_eq!(
            ds[..3]
                .iter()
                .map(|d| d.rate.0.as_str())
                .collect::<Vec<_>>(),
            ["266.8", "200.1", "66.7"]
        );
        assert!(ds[..3].iter().all(|d| d.fringe_range == Some(1)
            && d.amount.0 == "20"
            && d.unit == "Day"
            && d.fringes.contains("PEN")));
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let totals: Vec<i64> = r.categories[0].accounts[0]
            .rows
            .iter()
            .map(|r| r.total)
            .collect();
        // Each line rounds once: 20 x 266.8, 20 x 200.1, 20 x 66.7.
        assert_eq!(totals, [533_600, 400_200, 133_400, 500]);
        for (row, d) in r.categories[0].accounts[0].rows.iter().zip(ds).take(3) {
            let rate: Dec = d.rate.0.parse().unwrap();
            assert_eq!(Some(row.total), mul_cents(2_000, rate));
        }
        // One person: the range keeps the cutoff shared, so fringes match
        // the single-rate line.
        assert_eq!(r.categories[0].total, before_total);
    }

    #[test]
    fn writes_refuse_other_rows_and_keep_ranges() {
        let mut b = budget();
        if let TopsheetRow::Category(c) = &mut b.topsheet[0] {
            c.accounts[0].details.push(Detail::subtotal(9, "sub"));
            c.accounts[0].details[0].fringe_range = Some(7);
        }
        let p = standard();
        let i = input("50", "14", 6);
        assert_eq!(
            apply(&mut b, 9, &p, &i, OtPer::Day, OtWrite::Rate),
            Err(OtError::NotALine)
        );
        assert_eq!(
            apply(&mut b, 99, &p, &i, OtPer::Day, OtWrite::Rate),
            Err(OtError::Row(99))
        );
        let ids = apply(&mut b, 1, &p, &i, OtPer::Week, OtWrite::Split).unwrap();
        assert_eq!(ids.len(), 3);
        assert!(rows(&b)[..3].iter().all(|d| d.fringe_range == Some(7)));
        // Week: five regular days and a sixth at 1.5x.
        assert_eq!(rows(&b)[1].description, "Key grip: 1.5x, 32 hrs");
        // One slice only: splitting writes the rate.
        let mut b = budget();
        let ids = apply(
            &mut b,
            1,
            &p,
            &input("50", "8", 5),
            OtPer::Day,
            OtWrite::Split,
        )
        .unwrap();
        assert_eq!(ids, [1]);
        assert_eq!(rows(&b)[0].rate.0, "400");
    }

    #[test]
    fn patterns_are_saved_renamed_and_deleted_by_name() {
        let mut b = Budget::new("OT", "USD");
        save_pattern(&mut b, standard(), None).unwrap();
        assert_eq!(
            save_pattern(&mut b, standard(), None),
            Err(OtError::Taken("Ten-hour crew".into()))
        );
        let mut other = standard();
        other.name = "Twelve".into();
        save_pattern(&mut b, other.clone(), None).unwrap();
        let mut renamed = standard();
        renamed.name = "Twelve".into();
        assert!(save_pattern(&mut b, renamed, Some("Ten-hour crew")).is_err());
        let mut renamed = standard();
        renamed.name = "Ten".into();
        save_pattern(&mut b, renamed, Some("Ten-hour crew")).unwrap();
        assert_eq!(
            b.ot_patterns
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["Ten", "Twelve"]
        );
        assert_eq!(delete_pattern(&mut b, "Twelve").unwrap(), other);
        assert!(delete_pattern(&mut b, "Twelve").is_err());
        let mut bad = standard();
        bad.name = String::new();
        assert_eq!(save_pattern(&mut b, bad, None), Err(OtError::Name));
    }
}
