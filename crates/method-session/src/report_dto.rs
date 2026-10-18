//! JSON shapes for [`method_core::Report`], [`method_core::SelectionTotals`]
//! and [`method_core::incentives::Incentives`].
//!
//! The engine stays serde-free, so the app mirrors its report here. Field names
//! match the engine's. Money is integer cents (JSON integers), every [`Dec`] is
//! a canonical decimal string, never a JSON float.

use std::collections::BTreeMap;

use method_core::{
    Breakdown, CategoryTotals, Change, ChargeTotals, CreditRowTotals, CreditTotals, Dec,
    FringeSummary, GroupTotals, LevelTotals, Report, RowId, RowKind, RowTotals, SelectionTotals,
    Variance,
};
use serde::Serialize;

type Cents = i64;

fn dec(d: Dec) -> String {
    d.to_string()
}

pub fn kind_name(k: RowKind) -> &'static str {
    match k {
        RowKind::Line => "line",
        RowKind::Subtotal => "subtotal",
        RowKind::Adjustment => "adjustment",
        RowKind::Separator => "separator",
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReportDto {
    pub base_currency: String,
    pub totals_currency: String,
    pub totals_rate: String,
    pub globals: BTreeMap<String, String>,
    pub categories: Vec<CategoryDto>,
    pub levels: Vec<LevelDto>,
    pub budget_fringes: Cents,
    pub total_atl: Cents,
    pub total_btl: Cents,
    pub atl_btl: Cents,
    pub charges: Vec<ChargeDto>,
    pub grand_total: Cents,
    pub credits: Vec<CreditDto>,
    pub credits_total: Cents,
    pub net_total: Cents,
    pub fringes: BTreeMap<String, FringeSummaryDto>,
    pub groups: BTreeMap<String, GroupTotalsDto>,
    pub sets: BTreeMap<String, BreakdownDto>,
    pub locations: BTreeMap<String, BreakdownDto>,
    /// Grand and net totals converted to `totals_currency`, when that is not
    /// the base currency (and the conversion fits).
    pub display: Option<DisplayTotalsDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DisplayTotalsDto {
    pub grand_total: Cents,
    pub net_total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LevelDto {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub posted_fringes: Cents,
    pub total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FringeAccountDto {
    pub code: String,
    pub amount: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CategoryDto {
    pub code: String,
    pub name: String,
    pub accounts: Vec<AccountDto>,
    pub details_total: Cents,
    pub fringe_total: Cents,
    pub fringe_account: Option<FringeAccountDto>,
    pub total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AccountDto {
    pub code: String,
    pub name: String,
    /// Per-row figures. Left out of the lean view (`View`), which carries
    /// the rows of the account on screen only (`View::account`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<RowDto>>,
    pub details_total: Cents,
    pub fringe_total: Cents,
    pub total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RowDto {
    pub id: RowId,
    pub kind: &'static str,
    pub included: bool,
    pub quantity: String,
    pub currency: String,
    pub local_total: Cents,
    pub total: Cents,
    pub subtotal: Option<Cents>,
    pub fringes: BTreeMap<String, Cents>,
    pub fringe_total: Cents,
    /// The fringes with no cutoff and no range (the full fringe percent's
    /// dollars; decisions.md, entry 61).
    pub fringe_full_total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChargeDto {
    pub name: String,
    pub base: Cents,
    pub excluded: Cents,
    pub amount: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreditDto {
    pub name: String,
    pub rows: Vec<CreditRowDto>,
    pub capped: String,
    pub amount: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreditRowDto {
    pub gross: Cents,
    pub excluded_fringes: Cents,
    pub net: Cents,
    pub credit: String,
    /// Under the row's cap (positive) or over it (negative), unrounded, in
    /// currency units like `credit`; `None` with no cap.
    pub cap_variance: Option<String>,
}

/// Spend by location and every credit's estimate (decisions.md, entry 80).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IncentivesDto {
    pub locations: Vec<LocationSpendDto>,
    pub programs: Vec<ProgramDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LocationSpendDto {
    /// `null`: lines with no location.
    pub location: Option<String>,
    pub known: bool,
    pub lines: Cents,
    pub fringes: Cents,
    pub total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProgramDto {
    pub index: usize,
    pub name: String,
    pub applied: bool,
    pub locations: Vec<String>,
    pub cap: Option<Cents>,
    pub discount_percent: String,
    /// The estimate; `null` when it can't be worked out (`problem`).
    pub amount: Option<Cents>,
    pub problem: Option<String>,
    pub qualified: Cents,
    pub by_location: Vec<ProgramSpendDto>,
    pub rows_total: String,
    pub capped: bool,
    /// The credit's figures as the topsheet would list it; `null` with a
    /// problem.
    pub figures: Option<CreditDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProgramSpendDto {
    pub location: Option<String>,
    pub net: Cents,
}

impl From<&method_core::incentives::Incentives> for IncentivesDto {
    fn from(i: &method_core::incentives::Incentives) -> IncentivesDto {
        IncentivesDto {
            locations: i
                .locations
                .iter()
                .map(|l| LocationSpendDto {
                    location: l.location.clone(),
                    known: l.known,
                    lines: l.lines,
                    fringes: l.fringes,
                    total: l.total,
                })
                .collect(),
            programs: i
                .programs
                .iter()
                .map(|p| ProgramDto {
                    index: p.index,
                    name: p.name.clone(),
                    applied: p.applied,
                    locations: p.locations.iter().cloned().collect(),
                    cap: p.cap,
                    discount_percent: dec(p.discount_percent),
                    amount: p.amount(),
                    problem: p.problem.clone(),
                    qualified: p.qualified,
                    by_location: p
                        .by_location
                        .iter()
                        .map(|(location, net)| ProgramSpendDto {
                            location: location.clone(),
                            net: *net,
                        })
                        .collect(),
                    rows_total: dec(p.rows_total),
                    capped: p.capped,
                    figures: p.figures.as_ref().map(CreditDto::from),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FringeSummaryDto {
    pub uses: u32,
    pub total: Cents,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BreakdownDto {
    pub details_total: Cents,
    pub fringe_total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GroupTotalsDto {
    pub full: BreakdownDto,
    pub active: BreakdownDto,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SelectionDto {
    pub lines: BreakdownDto,
    pub fringes: BTreeMap<String, Cents>,
    pub groups: BTreeMap<String, BreakdownDto>,
}

fn map<K: Clone + Ord, A, B>(m: &BTreeMap<K, A>, f: impl Fn(&A) -> B) -> BTreeMap<K, B> {
    m.iter().map(|(k, v)| (k.clone(), f(v))).collect()
}

impl From<&Breakdown> for BreakdownDto {
    fn from(b: &Breakdown) -> BreakdownDto {
        BreakdownDto {
            details_total: b.details_total,
            fringe_total: b.fringe_total,
        }
    }
}

impl From<&GroupTotals> for GroupTotalsDto {
    fn from(g: &GroupTotals) -> GroupTotalsDto {
        GroupTotalsDto {
            full: (&g.full).into(),
            active: (&g.active).into(),
        }
    }
}

impl From<&FringeSummary> for FringeSummaryDto {
    fn from(f: &FringeSummary) -> FringeSummaryDto {
        FringeSummaryDto {
            uses: f.uses,
            total: f.total,
        }
    }
}

impl From<&RowTotals> for RowDto {
    fn from(r: &RowTotals) -> RowDto {
        RowDto {
            id: r.id,
            kind: kind_name(r.kind),
            included: r.included,
            quantity: dec(r.quantity),
            currency: r.currency.clone(),
            local_total: r.local_total,
            total: r.total,
            subtotal: r.subtotal,
            fringes: r.fringes.clone(),
            fringe_total: r.fringe_total,
            fringe_full_total: r.fringe_full_total,
        }
    }
}

impl From<&CategoryTotals> for CategoryDto {
    fn from(c: &CategoryTotals) -> CategoryDto {
        CategoryDto::with_rows(c, true)
    }
}

impl CategoryDto {
    /// The category's totals, with each account's per-row figures when `rows`.
    pub fn with_rows(c: &CategoryTotals, rows: bool) -> CategoryDto {
        CategoryDto {
            code: c.code.clone(),
            name: c.name.clone(),
            accounts: c
                .accounts
                .iter()
                .map(|a| AccountDto {
                    code: a.code.clone(),
                    name: a.name.clone(),
                    rows: rows.then(|| a.rows.iter().map(RowDto::from).collect()),
                    details_total: a.details_total,
                    fringe_total: a.fringe_total,
                    total: a.total,
                })
                .collect(),
            details_total: c.details_total,
            fringe_total: c.fringe_total,
            fringe_account: c
                .fringe_account
                .as_ref()
                .map(|(code, amount)| FringeAccountDto {
                    code: code.clone(),
                    amount: *amount,
                }),
            total: c.total,
        }
    }
}

impl From<&LevelTotals> for LevelDto {
    fn from(l: &LevelTotals) -> LevelDto {
        LevelDto {
            name: l.name.clone(),
            start: l.start,
            end: l.end,
            posted_fringes: l.posted_fringes,
            total: l.total,
        }
    }
}

impl From<&ChargeTotals> for ChargeDto {
    fn from(c: &ChargeTotals) -> ChargeDto {
        ChargeDto {
            name: c.name.clone(),
            base: c.base,
            excluded: c.excluded,
            amount: c.amount,
        }
    }
}

impl From<&CreditRowTotals> for CreditRowDto {
    fn from(r: &CreditRowTotals) -> CreditRowDto {
        CreditRowDto {
            gross: r.gross,
            excluded_fringes: r.excluded_fringes,
            net: r.net,
            credit: dec(r.credit),
            cap_variance: r.cap_variance.map(dec),
        }
    }
}

impl From<&CreditTotals> for CreditDto {
    fn from(c: &CreditTotals) -> CreditDto {
        CreditDto {
            name: c.name.clone(),
            rows: c.rows.iter().map(CreditRowDto::from).collect(),
            capped: dec(c.capped),
            amount: c.amount,
        }
    }
}

impl From<&Report> for ReportDto {
    fn from(r: &Report) -> ReportDto {
        ReportDto::with_rows(r, true)
    }
}

impl ReportDto {
    /// Every total, with per-row figures when `rows` (the printed report and
    /// the full view need them; the lean view does not).
    pub fn with_rows(r: &Report, rows: bool) -> ReportDto {
        ReportDto {
            base_currency: r.base_currency.clone(),
            totals_currency: r.totals_currency.clone(),
            totals_rate: dec(r.totals_rate),
            globals: map(&r.globals, |d| dec(*d)),
            categories: r
                .categories
                .iter()
                .map(|c| CategoryDto::with_rows(c, rows))
                .collect(),
            levels: r.levels.iter().map(LevelDto::from).collect(),
            budget_fringes: r.budget_fringes,
            total_atl: r.total_atl,
            total_btl: r.total_btl,
            atl_btl: r.atl_btl,
            charges: r.charges.iter().map(ChargeDto::from).collect(),
            grand_total: r.grand_total,
            credits: r.credits.iter().map(CreditDto::from).collect(),
            credits_total: r.credits_total,
            net_total: r.net_total,
            fringes: map(&r.fringes, |f| f.into()),
            groups: map(&r.groups, |g| g.into()),
            sets: map(&r.sets, |b| b.into()),
            locations: map(&r.locations, |b| b.into()),
            // Converted account by account (decisions.md, entry 60).
            display: r.display.as_ref().map(|d| DisplayTotalsDto {
                grand_total: d.grand_total,
                net_total: d.net_total,
            }),
        }
    }
}

impl From<&SelectionTotals> for SelectionDto {
    fn from(s: &SelectionTotals) -> SelectionDto {
        SelectionDto {
            lines: (&s.lines).into(),
            fringes: s.fringes.clone(),
            groups: map(&s.groups, |b| b.into()),
        }
    }
}

/// One compared figure against the baseline. `None` means it didn't exist on
/// that side (a row added or deleted since the baseline was locked).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ChangeDto {
    pub original: Option<Cents>,
    pub current: Option<Cents>,
    /// Current minus original, a missing side counting as 0.
    pub variance: Option<Cents>,
}

/// Original (baseline) vs current totals. Row keys are row ids written as
/// decimal strings (JSON object keys); accounts and categories are by code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VarianceDto {
    /// Every row in the full view; in the lean view only the rows of the
    /// account on screen.
    pub rows: BTreeMap<RowId, ChangeDto>,
    /// Rows (in the whole budget) whose total moved, or that were added or
    /// deleted, since the baseline was locked.
    pub rows_changed: usize,
    pub accounts: BTreeMap<String, ChangeDto>,
    pub categories: BTreeMap<String, ChangeDto>,
    pub grand_total: ChangeDto,
    pub net_total: ChangeDto,
}

impl VarianceDto {
    /// The comparison with only the rows `keep` accepts listed (all of them
    /// still count in `rows_changed`).
    pub fn with_rows(v: &Variance, keep: impl Fn(RowId) -> bool) -> VarianceDto {
        VarianceDto {
            rows: v
                .rows
                .iter()
                .filter(|(id, _)| keep(**id))
                .map(|(id, c)| (*id, c.into()))
                .collect(),
            rows_changed: v.rows.values().filter(|c| c.variance() != Some(0)).count(),
            accounts: map(&v.accounts, |c| c.into()),
            categories: map(&v.categories, |c| c.into()),
            grand_total: (&v.grand_total).into(),
            net_total: (&v.net_total).into(),
        }
    }
}

impl From<&Change> for ChangeDto {
    fn from(c: &Change) -> ChangeDto {
        ChangeDto {
            original: c.original,
            current: c.current,
            variance: c.variance(),
        }
    }
}

impl From<&Variance> for VarianceDto {
    fn from(v: &Variance) -> VarianceDto {
        VarianceDto::with_rows(v, |_| true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::{recalc, starter, RecalcOptions};

    #[test]
    fn money_is_integers_and_decimals_are_strings() {
        let mut b = starter::budget();
        b.fringe_posting = method_core::FringePosting::Category {
            suffix: "99".into(),
        };
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let v = serde_json::to_value(ReportDto::from(&r)).unwrap();
        assert_eq!(v["grand_total"].as_i64(), Some(r.grand_total));
        assert_eq!(v["totals_rate"], "1");
        assert_eq!(v["globals"]["SHOOT_WEEKS"], "5");
        let row = &v["categories"][3]["accounts"][0]["rows"][0];
        assert_eq!(row["kind"], "line");
        assert_eq!(row["quantity"], "6");
        assert!(row["total"].is_i64());
        assert!(row["fringes"]["PAYROLL"].is_i64());
        assert_eq!(v["levels"][0]["name"], "Total above-the-line");
        assert_eq!(v["charges"][0]["name"], "Contingency");
        assert!(v["categories"][0]["fringe_account"]["code"].is_string());
        assert!(v["display"].is_null());
        // Nothing anywhere is a float.
        fn no_floats(v: &serde_json::Value) {
            match v {
                serde_json::Value::Number(n) => assert!(!n.is_f64(), "float {n}"),
                serde_json::Value::Array(a) => a.iter().for_each(no_floats),
                serde_json::Value::Object(o) => o.values().for_each(no_floats),
                _ => {}
            }
        }
        no_floats(&v);
    }

    #[test]
    fn totals_in_another_currency() {
        let mut b = starter::budget();
        b.currencies.insert(
            "EUR".into(),
            method_core::Currency {
                description: String::new(),
                symbol: String::new(),
                rate: "2".parse().unwrap(),
                format: Default::default(),
            },
        );
        b.totals_currency = Some("EUR".into());
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let d = ReportDto::from(&r).display.unwrap();
        assert_eq!(d.grand_total, r.grand_total / 2);
        assert_eq!(d.net_total, r.net_total / 2);
    }
}
