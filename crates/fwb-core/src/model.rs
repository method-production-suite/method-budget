//! The budget document: what a user edits and what the native format will store.
//!
//! Hierarchy: [`Budget`] > topsheet ([`TopsheetRow`]: categories and production
//! totals) > [`Account`] > [`Detail`] rows. Maps are `BTreeMap`/`BTreeSet` so
//! iteration order, and therefore output, is stable.

use std::collections::{BTreeMap, BTreeSet};

use crate::coding::AccountCoding;
use crate::decimal::Dec;
use crate::Cents;

/// Stable identity of a detail row. Survives reordering and edits, so versions,
/// comparisons and imports can match rows. Unique within a budget.
pub type RowId = u64;

/// An expression source string such as `"30"`, `"SHOOT_DAYS"` or `"WEEKS * 5"`.
/// Blank means "not filled in"; see [`Detail`] for what blank counts as.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Formula(pub String);

impl Formula {
    pub fn is_blank(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl From<&str> for Formula {
    fn from(s: &str) -> Formula {
        Formula(s.to_string())
    }
}

impl From<String> for Formula {
    fn from(s: String) -> Formula {
        Formula(s)
    }
}

impl From<i64> for Formula {
    fn from(n: i64) -> Formula {
        Formula(n.to_string())
    }
}

impl From<Dec> for Formula {
    fn from(d: Dec) -> Formula {
        Formula(d.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budget {
    pub title: String,
    /// Currency code that blank-currency lines use and totals are computed in.
    pub base_currency: String,
    /// Currency totals are shown in, when not the base currency.
    pub totals_currency: Option<String>,
    /// Typed metadata (shoot days, director, start date...), user-extensible.
    pub info: BTreeMap<String, InfoValue>,
    /// Foreign currencies, keyed by code.
    pub currencies: BTreeMap<String, Currency>,
    /// Named values usable in any formula. May refer to each other, not cyclically.
    pub globals: BTreeMap<String, Formula>,
    /// Units, keyed by singular name. Lines refer to them by name.
    pub units: BTreeMap<String, UnitDef>,
    /// Fringe definitions, keyed by the name details use to apply them.
    pub fringes: BTreeMap<String, Fringe>,
    pub fringe_posting: FringePosting,
    /// How account numbers are segmented and written.
    pub account_coding: AccountCoding,
    pub groups: BTreeMap<String, Group>,
    pub sets: BTreeMap<String, SetDef>,
    pub locations: BTreeMap<String, Location>,
    /// Categories and production-total rows, in display order.
    pub topsheet: Vec<TopsheetRow>,
    /// Contractual charges, in application order.
    pub charges: Vec<Charge>,
    /// Applied credits (incentives), in application order.
    pub credits: Vec<Credit>,
    /// Frozen totals to compare against ("original" vs current).
    pub baseline: Option<Baseline>,
}

impl Budget {
    pub fn new(title: impl Into<String>, base_currency: impl Into<String>) -> Budget {
        Budget {
            title: title.into(),
            base_currency: base_currency.into(),
            totals_currency: None,
            info: BTreeMap::new(),
            currencies: BTreeMap::new(),
            globals: BTreeMap::new(),
            units: default_units(),
            fringes: BTreeMap::new(),
            fringe_posting: FringePosting::default(),
            account_coding: AccountCoding::default(),
            groups: BTreeMap::new(),
            sets: BTreeMap::new(),
            locations: BTreeMap::new(),
            topsheet: Vec::new(),
            charges: Vec::new(),
            credits: Vec::new(),
            baseline: None,
        }
    }

    pub fn categories(&self) -> impl Iterator<Item = &Category> {
        self.topsheet.iter().filter_map(|r| match r {
            TopsheetRow::Category(c) => Some(c),
            TopsheetRow::ProductionTotal(_) => None,
        })
    }

    pub fn categories_mut(&mut self) -> impl Iterator<Item = &mut Category> {
        self.topsheet.iter_mut().filter_map(|r| match r {
            TopsheetRow::Category(c) => Some(c),
            TopsheetRow::ProductionTotal(_) => None,
        })
    }
}

/// Our starter units. Hours equivalences are editable per budget (decisions.md).
pub fn default_units() -> BTreeMap<String, UnitDef> {
    let u = |plural: &str, hours: Option<i64>| UnitDef {
        plural: plural.into(),
        description: String::new(),
        hours: hours.map(Dec::from_int),
    };
    BTreeMap::from([
        ("Allow".into(), u("Allow", None)),
        ("Day".into(), u("Days", Some(10))),
        ("Flat".into(), u("Flat", None)),
        ("Hour".into(), u("Hours", Some(1))),
        ("Month".into(), u("Months", Some(200))),
        ("Week".into(), u("Weeks", Some(50))),
    ])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Currency {
    pub description: String,
    pub symbol: String,
    /// Base-currency units per one unit of this currency.
    pub rate: Dec,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDef {
    pub plural: String,
    pub description: String,
    /// How many hours one of this unit is. `None` for non-time units.
    pub hours: Option<Dec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fringe {
    pub description: String,
    /// For `Percent`, the percentage (`"7.65"` is 7.65%). For `Flat`, the amount
    /// in base-currency units. May use globals.
    pub rate: Formula,
    pub kind: FringeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FringeKind {
    /// `rate`% of the earner's subtotal, capped at `cutoff` when set. The earner
    /// is a single line, or a whole fringe range (see [`Detail::fringe_range`]).
    Percent { cutoff: Option<Cents> },
    /// `rate` once per line. When `unit` and the line's primary unit both have
    /// hours, it's charged per unit instead, scaled by the hours ratio and
    /// multiplied by the line's quantity (amount x X x 4X).
    Flat { unit: Option<String> },
}

/// Where fringe dollars show up. They never show on the line itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FringePosting {
    /// One Total Fringes row for the budget, outside ATL and BTL.
    Budget,
    /// One fringe row above each production total, inside that level.
    ProductionLevel,
    /// A fringe account at the end of each category, numbered from the category's
    /// code and `suffix` by the budget's account coding (e.g. `11` + `99` = `11-99`).
    Category { suffix: String },
    /// A fringe line at the end of each account.
    Account,
}

impl Default for FringePosting {
    fn default() -> FringePosting {
        FringePosting::Category {
            suffix: "99".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub description: String,
    /// "In budget total". When false, every line tagged with this group is
    /// suppressed: shown, but counted as 0 everywhere.
    pub in_budget_total: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetDef {
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Location {
    pub description: String,
    pub city: String,
    pub region: String,
    pub country: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopsheetRow {
    Category(Category),
    /// Subtotals every category since the previous production total. The first
    /// one marks the end of above-the-line.
    ProductionTotal(ProductionTotal),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductionTotal {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Category {
    pub code: String,
    pub name: String,
    pub accounts: Vec<Account>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// Chart-of-accounts number, e.g. `"1100"`.
    pub code: String,
    pub name: String,
    pub details: Vec<Detail>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// A costed line: amount x X x rate x 4X. With everything blank it's a title line.
    Line,
    /// Shows the running total of rows since the previous subtotal row. Adds nothing.
    Subtotal,
    /// A real cost whose rate is locked to the sum of the lines above it, back to
    /// the previous adjustment row. Amount and multipliers set the percentage.
    Adjustment,
}

/// One row in an account.
///
/// Blank formulas: `amount` and `rate` count as 0, `x` and `x4` as 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detail {
    pub id: RowId,
    pub kind: RowKind,
    pub description: String,
    pub amount: Formula,
    /// Primary unit name. The only unit used for flat-fringe hour conversion.
    pub unit: String,
    pub x: Formula,
    pub unit2: String,
    pub rate: Formula,
    pub unit3: String,
    pub x4: Formula,
    pub unit4: String,
    /// `None` means the base currency. Ignored on adjustment rows.
    pub currency: Option<String>,
    /// Names of fringes from [`Budget::fringes`] applied to this line.
    pub fringes: BTreeSet<String>,
    pub groups: BTreeSet<String>,
    pub location: Option<String>,
    pub set: Option<String>,
    /// Consecutive lines with the same `Some(n)` form one fringe range: percent
    /// fringe cutoffs apply to their combined subtotal, as one earner.
    pub fringe_range: Option<u32>,
}

impl Detail {
    fn blank(id: RowId, kind: RowKind, description: String) -> Detail {
        Detail {
            id,
            kind,
            description,
            amount: Formula::default(),
            unit: String::new(),
            x: Formula::default(),
            unit2: String::new(),
            rate: Formula::default(),
            unit3: String::new(),
            x4: Formula::default(),
            unit4: String::new(),
            currency: None,
            fringes: BTreeSet::new(),
            groups: BTreeSet::new(),
            location: None,
            set: None,
            fringe_range: None,
        }
    }

    /// A costed line: `amount` `unit` at `rate`, with blank X and 4X.
    pub fn line(
        id: RowId,
        description: impl Into<String>,
        amount: impl Into<Formula>,
        unit: impl Into<String>,
        rate: impl Into<Formula>,
    ) -> Detail {
        Detail {
            amount: amount.into(),
            unit: unit.into(),
            rate: rate.into(),
            ..Detail::blank(id, RowKind::Line, description.into())
        }
    }

    pub fn subtotal(id: RowId, description: impl Into<String>) -> Detail {
        Detail::blank(id, RowKind::Subtotal, description.into())
    }

    /// An adjustment row; `amount` is usually a fraction like `"0.1"` for 10%.
    pub fn adjustment(
        id: RowId,
        description: impl Into<String>,
        amount: impl Into<Formula>,
    ) -> Detail {
        Detail {
            amount: amount.into(),
            ..Detail::blank(id, RowKind::Adjustment, description.into())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Charge {
    pub name: String,
    pub account: Option<String>,
    /// Only applied charges count. Unapplied ones are kept for reuse.
    pub applied: bool,
    pub kind: ChargeKind,
    /// Manual exclusions: (description, amount).
    pub excluded_amounts: Vec<(String, Cents)>,
    pub excluded_groups: BTreeSet<String>,
    pub excluded_categories: BTreeSet<String>,
    pub excluded_accounts: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChargeKind {
    /// Percent of ATL + BTL, less exclusions. `10` means 10%.
    Percent(Dec),
    /// A fixed fee, less excluded amounts and excluded group totals.
    Flat {
        amount: Cents,
        currency: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credit {
    pub name: String,
    pub account: Option<String>,
    pub applied: bool,
    /// Ceiling on the sum of the row credits.
    pub cap: Option<Cents>,
    /// Taken off after the cap, e.g. a broker's fee for selling the credit. `5` = 5%.
    pub discount_percent: Dec,
    pub rows: Vec<CreditRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreditRow {
    pub description: String,
    pub source: CreditSource,
    /// `25` means 25% of the row's net spend.
    pub percent: Dec,
    pub cap: CreditCap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreditSource {
    /// A gross amount typed in directly.
    Flat(Cents),
    /// The total of lines tagged with `group`, plus their fringes except
    /// `excluded_fringes`.
    Group {
        group: String,
        excluded_fringes: BTreeSet<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreditCap {
    None,
    /// Caps the spend the percent applies to.
    QualifiedSpend(Cents),
    /// Caps the resulting credit.
    Credit(Cents),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InfoValue {
    Text(String),
    Number(Dec),
    /// ISO 8601 date, `YYYY-MM-DD`.
    Date(String),
}

/// Totals frozen at some point, so the current budget can show what changed.
/// Rows are matched by id, accounts and categories by code.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Baseline {
    pub rows: BTreeMap<RowId, Cents>,
    pub accounts: BTreeMap<String, Cents>,
    pub categories: BTreeMap<String, Cents>,
    pub grand_total: Cents,
    pub net_total: Cents,
}
