//! The budget document: what a user edits and what the native format will store.
//!
//! Hierarchy: [`Budget`] > [`Category`] (topsheet line) > [`Account`] > [`Detail`].
//! Maps are `BTreeMap`/`BTreeSet` so iteration order, and therefore output, is stable.

use std::collections::{BTreeMap, BTreeSet};

use crate::decimal::Dec;
use crate::Cents;

/// An expression source string such as `"30"`, `"SHOOT_DAYS"` or `"WEEKS * 5"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formula(pub String);

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
    /// ISO-style code totals are reported in, e.g. `"USD"`.
    pub base_currency: String,
    /// Base-currency units per one unit of each foreign currency.
    pub exchange_rates: BTreeMap<String, Dec>,
    /// Named values usable in any formula. May refer to each other, not cyclically.
    pub globals: BTreeMap<String, Formula>,
    /// Fringe definitions, keyed by the name details use to apply them.
    pub fringes: BTreeMap<String, Fringe>,
    /// Topsheet lines, in display order.
    pub categories: Vec<Category>,
    /// Applied below the line, in order, to the budget subtotal.
    pub contractual_charges: Vec<ContractualCharge>,
}

impl Budget {
    pub fn new(title: impl Into<String>, base_currency: impl Into<String>) -> Budget {
        Budget {
            title: title.into(),
            base_currency: base_currency.into(),
            exchange_rates: BTreeMap::new(),
            globals: BTreeMap::new(),
            fringes: BTreeMap::new(),
            categories: Vec::new(),
            contractual_charges: Vec::new(),
        }
    }
}

/// A topsheet line such as "Above-the-line" or "Post".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Category {
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

/// One line item. Total = amount x multiplier x rate, in `currency`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detail {
    pub description: String,
    pub amount: Formula,
    pub units: Unit,
    /// The "X" column: people, episodes, and so on.
    pub multiplier: Formula,
    pub rate: Formula,
    /// `None` means the budget's base currency.
    pub currency: Option<String>,
    /// Names of fringes from [`Budget::fringes`] applied to this line.
    pub fringes: BTreeSet<String>,
    pub groups: BTreeSet<String>,
    pub location: Option<String>,
    pub set: Option<String>,
}

impl Detail {
    /// A line with multiplier 1, base currency and no fringes, groups or tags.
    pub fn new(
        description: impl Into<String>,
        amount: impl Into<Formula>,
        units: Unit,
        rate: impl Into<Formula>,
    ) -> Detail {
        Detail {
            description: description.into(),
            amount: amount.into(),
            units,
            multiplier: Formula::from(1),
            rate: rate.into(),
            currency: None,
            fringes: BTreeSet::new(),
            groups: BTreeSet::new(),
            location: None,
            set: None,
        }
    }
}

/// Label for what `amount` counts. Informational; it does not change the math.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unit {
    Allow,
    Flat,
    Hours,
    Days,
    Weeks,
    Months,
    Each,
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fringe {
    /// `percent` of the line's base-currency total (18.5 means 18.5%). With a
    /// `wage_cap`, only the first `wage_cap` cents of each line are fringed.
    Percent {
        percent: Dec,
        wage_cap: Option<Cents>,
    },
    /// `per_unit` cents for every unit on the line (amount x multiplier), with
    /// units optionally capped at `unit_cap`.
    FlatPerUnit {
        per_unit: Cents,
        unit_cap: Option<Dec>,
    },
}

/// A percent-of-subtotal line such as contingency, insurance or a fee.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractualCharge {
    pub name: String,
    pub percent: Dec,
    /// Account codes left out of this charge's base.
    pub excluded_accounts: BTreeSet<String>,
    /// Whether fringes count toward the base, not just line totals.
    pub include_fringes: bool,
}
