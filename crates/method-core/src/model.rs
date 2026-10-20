//! The budget document: what a user edits and what the native format will store.
//!
//! Hierarchy: [`Budget`] > topsheet ([`TopsheetRow`]: categories and production
//! totals) > [`Account`] > [`Detail`] rows. Maps are `BTreeMap`/`BTreeSet` so
//! iteration order, and therefore output, is stable.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

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
    /// Descriptions, units and display settings of globals, by global name.
    /// Only non-default entries for globals that exist are written out.
    pub global_info: BTreeMap<String, GlobalInfo>,
    /// Units, keyed by singular name. Lines refer to them by name.
    pub units: BTreeMap<String, UnitDef>,
    /// Fringe definitions, keyed by the name details use to apply them.
    pub fringes: BTreeMap<String, Fringe>,
    pub fringe_posting: FringePosting,
    /// Under production-level posting, what happens to the fringes of
    /// categories after the last production total: kept (the default) or
    /// dropped, as Movie Magic Budgeting does (decisions.md, entry 56).
    pub trailing_level_fringes: TrailingFringes,
    /// What every percent fringe's cutoff is multiplied by (decisions.md,
    /// entry 60). 1.0 unless the budget says otherwise.
    pub cutoff_factor: CutoffFactor,
    /// The account number, description and place of the budget's one
    /// fringe row, when fringes post by budget (decisions.md, entry 60).
    pub fringe_row: FringeRow,
    /// How totals are worked out: as Movie Magic Budgeting does (binary
    /// doubles, the default) or in exact decimals (decisions.md, entry 55).
    pub arithmetic: Arithmetic,
    /// Round every account's lines total to a whole unit, and work out
    /// everything above accounts from the rounded totals (decisions.md,
    /// entry 69). Off unless the budget says otherwise.
    pub round_account_totals: bool,
    /// How account numbers are segmented and written.
    pub account_coding: AccountCoding,
    /// What a line inserted in the editor copies from its neighbor.
    pub new_line_defaults: NewLineDefaults,
    pub groups: BTreeMap<String, Group>,
    /// Which lines groups leave in or out of the totals, budget-wide
    /// (decisions.md, entry 57).
    pub group_rules: GroupRules,
    pub sets: BTreeMap<String, SetDef>,
    pub locations: BTreeMap<String, Location>,
    /// Categories and production-total rows, in display order.
    pub topsheet: Vec<TopsheetRow>,
    /// Contractual charges, in application order.
    pub charges: Vec<Charge>,
    /// Applied credits (incentives), in application order.
    pub credits: Vec<Credit>,
    /// Caps that several credits share, keyed by name (decisions.md, entry
    /// 88): the credits naming one in `shared_cap` never come to more than
    /// it together.
    pub shared_caps: BTreeMap<String, Cents>,
    /// Frozen totals to compare against ("original" vs current).
    pub baseline: Option<Baseline>,
    /// How numbers show on screen. Not content: versions don't carry it.
    pub display: DisplayPrefs,
    /// Named full copies of the budget's content, oldest first. See
    /// [`crate::versions`] for saving and restoring them.
    pub versions: Vec<Version>,
    /// Save the versions as changes against each other instead of full
    /// copies, so many versions don't bloat the file (docs/FORMAT.md,
    /// "Version deltas"). A file setting, not content: versions don't carry
    /// it, and loading always gives full versions either way.
    pub compact_versions: bool,
    /// Saved report setups, in the order the user keeps them. Layout only,
    /// not content: versions don't carry them (see [`crate::report_preset`]).
    pub report_presets: Vec<crate::report_preset::ReportPreset>,
    /// Saved bundles of line tags, in the user's order. Tools, not content:
    /// versions don't carry them (see [`crate::presets`]).
    pub tag_presets: Vec<crate::presets::TagPreset>,
    /// Saved blocks of rows to insert, in the user's order. Tools, not
    /// content, like tag presets.
    pub line_presets: Vec<crate::presets::LinePreset>,
    /// Hand-made display order of the setup tables (see [`crate::order`]).
    /// Layout, not content: versions don't carry it.
    pub setup_order: crate::order::SetupOrder,
    /// Folders of setup items per kind (the personal library's folders, see
    /// [`crate::folders`]). Layout, not content: versions don't carry them.
    pub setup_folders: crate::folders::SetupFolders,
    /// Where this budget came from, when it is a copy made for sharing or a
    /// sub-budget (see [`Provenance`]). About the file, not content:
    /// versions don't carry it.
    pub provenance: Option<Provenance>,
    /// Actual costs from accounting and estimates to complete (see
    /// [`crate::actuals`]). About the production's spending, not the
    /// budget's content: versions don't carry them. Shared, so copying a
    /// budget for undo copies a pointer.
    pub actuals: Arc<crate::actuals::Actuals>,
    /// Saved overtime patterns for the overtime helper, in the user's
    /// order (see [`crate::overtime`]). Tools, not content, like tag
    /// presets: versions don't carry them.
    pub ot_patterns: Vec<crate::overtime::OtPattern>,
    /// Where setup items came from: the setup pack each was imported from
    /// and source notes (see [`crate::pack`]). About the items, not
    /// content: versions don't carry it.
    pub setup_sources: crate::pack::SetupSources,
    /// Rate tables the user keeps, in the user's order (see
    /// [`crate::rates`]). Tools, not content, like overtime patterns:
    /// versions don't carry them.
    pub rate_tables: Vec<crate::rates::RateTable>,
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
            global_info: BTreeMap::new(),
            units: default_units(),
            fringes: BTreeMap::new(),
            fringe_posting: FringePosting::default(),
            trailing_level_fringes: TrailingFringes::default(),
            cutoff_factor: CutoffFactor::default(),
            fringe_row: FringeRow::default(),
            arithmetic: Arithmetic::default(),
            round_account_totals: false,
            account_coding: AccountCoding::default(),
            new_line_defaults: NewLineDefaults::default(),
            groups: BTreeMap::new(),
            group_rules: GroupRules::default(),
            sets: BTreeMap::new(),
            locations: BTreeMap::new(),
            topsheet: Vec::new(),
            charges: Vec::new(),
            credits: Vec::new(),
            shared_caps: BTreeMap::new(),
            baseline: None,
            display: DisplayPrefs::default(),
            versions: Vec::new(),
            compact_versions: false,
            report_presets: Vec::new(),
            tag_presets: Vec::new(),
            line_presets: Vec::new(),
            setup_order: crate::order::SetupOrder::default(),
            setup_folders: crate::folders::SetupFolders::default(),
            provenance: None,
            actuals: Arc::default(),
            ot_patterns: Vec::new(),
            setup_sources: crate::pack::SetupSources::default(),
            rate_tables: Vec::new(),
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

/// The starter units a new budget has, as the reference product starts
/// them (decisions.md, entry 60): a Day is 12 hours, a Week 60, a Month 240
/// and an Hour 1; Allow and Flat have no hours. Hours are editable per
/// budget, and a budget file keeps its own units.
pub fn default_units() -> BTreeMap<String, UnitDef> {
    let u = |plural: &str, hours: Option<i64>| UnitDef {
        plural: plural.into(),
        description: String::new(),
        hours: hours.map(Dec::from_int),
    };
    BTreeMap::from([
        ("Allow".into(), u("Allow", None)),
        ("Day".into(), u("Days", Some(12))),
        ("Flat".into(), u("Flat", None)),
        ("Hour".into(), u("Hours", Some(1))),
        ("Month".into(), u("Months", Some(240))),
        ("Week".into(), u("Weeks", Some(60))),
    ])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Currency {
    pub description: String,
    pub symbol: String,
    /// Base-currency units per one unit of this currency.
    pub rate: Dec,
    /// How amounts in it are written: separators and where the symbol
    /// goes (decisions.md, entry 61). Display only.
    pub format: crate::currency_format::CurrencyFormat,
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
    /// Optional short code shown in the grid's fringes column instead of
    /// the name (`"FICA"`). Display only: lines refer to the fringe by name.
    pub short_code: Option<String>,
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
    /// `rate` per unit, per line (never per range). How many units a line
    /// counts is set by `basis` (decisions.md, entry 54): by default the
    /// line's Amount converted into `unit` through unit hours, X and 4X left
    /// out. With no `unit`, or when it or the line's unit has no hours, the
    /// line counts once.
    Flat {
        unit: Option<String>,
        basis: FlatBasis,
        /// Most hours a day counted, for a fringe charged per hour; `None`
        /// for no cap. Ignored for other units.
        max_hours_per_day: Option<Dec>,
        /// Kept as stored but never used: cutoffs apply to percent fringes
        /// only.
        cutoff: Option<Cents>,
    },
}

impl FringeKind {
    /// A flat fringe charged per `unit` (once per line when `None`), counted
    /// the standard way, with no hours cap and no stored cutoff.
    pub fn flat(unit: Option<String>) -> FringeKind {
        FringeKind::Flat {
            unit,
            basis: FlatBasis::Amount,
            max_hours_per_day: None,
            cutoff: None,
        }
    }

    /// The stored cutoff, of either kind (only a percent fringe's counts).
    pub fn cutoff(&self) -> Option<Cents> {
        match self {
            FringeKind::Percent { cutoff } | FringeKind::Flat { cutoff, .. } => *cutoff,
        }
    }
}

/// What a flat fringe counts on a line (decisions.md, entry 54).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FlatBasis {
    /// The Amount column only, converted from the line's unit into the
    /// fringe's unit through unit hours; X and 4X never count. The standard.
    #[default]
    Amount,
    /// As `Amount`, then times X when X's unit is not a time unit (a blank
    /// unit is a count), and times 4X when 4X's unit is not.
    AmountCounts,
    /// The line's whole quantity (Amount x X x 4X), converted through unit
    /// hours: how flat fringes counted before entry 54.
    Quantity,
}

impl FlatBasis {
    pub const ALL: [FlatBasis; 3] = [
        FlatBasis::Amount,
        FlatBasis::AmountCounts,
        FlatBasis::Quantity,
    ];

    /// The name files and commands use: `amount`, `amount_counts`, `quantity`.
    pub fn name(self) -> &'static str {
        match self {
            FlatBasis::Amount => "amount",
            FlatBasis::AmountCounts => "amount_counts",
            FlatBasis::Quantity => "quantity",
        }
    }

    /// The basis named `name` (as [`FlatBasis::name`] writes it).
    pub fn parse(name: &str) -> Option<FlatBasis> {
        FlatBasis::ALL.into_iter().find(|b| b.name() == name.trim())
    }

    /// How the application labels it.
    pub fn label(self) -> &'static str {
        match self {
            FlatBasis::Amount => "Amount only (standard)",
            FlatBasis::AmountCounts => "Amount, plus counts in X and 4X",
            FlatBasis::Quantity => "Full quantity",
        }
    }
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

/// Under [`FringePosting::ProductionLevel`], what happens to the fringes of
/// categories after the last production total, which no production total
/// closes (decisions.md, entry 56). A budget setting, saved with the file;
/// versions carry it. It changes nothing under the other posting modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TrailingFringes {
    /// Posted in a fringe row of their own, in the unnamed level after the
    /// last production total, so they stay in the grand total (`method-budget
    /// check` warns that they are in no level). The default.
    #[default]
    Keep,
    /// Left out of every total, as Movie Magic Budgeting does: they have no
    /// production total to post above.
    Drop,
}

impl TrailingFringes {
    pub const ALL: [TrailingFringes; 2] = [TrailingFringes::Keep, TrailingFringes::Drop];

    /// The name files and commands use: `keep` or `drop`.
    pub fn name(self) -> &'static str {
        match self {
            TrailingFringes::Keep => "keep",
            TrailingFringes::Drop => "drop",
        }
    }

    /// The setting named `name` (as [`TrailingFringes::name`] writes it).
    pub fn parse(name: &str) -> Option<TrailingFringes> {
        TrailingFringes::ALL
            .into_iter()
            .find(|t| t.name() == name.trim())
    }
}

/// What every percent fringe's cutoff is multiplied by, as the reference
/// product's percentage fringe cutoff factor does (decisions.md, entry 60):
/// a 2,222.22 cutoff at 1.5 is 3,333.33. Flat fringes have no cutoff, so it
/// changes nothing for them. A budget setting, saved with the file only
/// when it isn't 1.0; versions carry it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CutoffFactor {
    /// 1.0: cutoffs as typed. The default.
    #[default]
    One,
    /// 1.5.
    OneAndAHalf,
    /// 2.0.
    Two,
    /// 2.5.
    TwoAndAHalf,
    /// 3.0.
    Three,
}

impl CutoffFactor {
    pub const ALL: [CutoffFactor; 5] = [
        CutoffFactor::One,
        CutoffFactor::OneAndAHalf,
        CutoffFactor::Two,
        CutoffFactor::TwoAndAHalf,
        CutoffFactor::Three,
    ];

    /// The name files and commands use: `1.0`, `1.5`, `2.0`, `2.5` or `3.0`.
    pub fn name(self) -> &'static str {
        match self {
            CutoffFactor::One => "1.0",
            CutoffFactor::OneAndAHalf => "1.5",
            CutoffFactor::Two => "2.0",
            CutoffFactor::TwoAndAHalf => "2.5",
            CutoffFactor::Three => "3.0",
        }
    }

    /// The factor written as a number (`1.5`, `2`, `2.00`, `3.0`), or
    /// `None` when it isn't one of the five.
    pub fn parse(text: &str) -> Option<CutoffFactor> {
        let d: Dec = text.trim().parse().ok()?;
        CutoffFactor::ALL.into_iter().find(|f| f.value() == d)
    }

    /// The factor as a number.
    pub fn value(self) -> Dec {
        let tenths = match self {
            CutoffFactor::One => 10,
            CutoffFactor::OneAndAHalf => 15,
            CutoffFactor::Two => 20,
            CutoffFactor::TwoAndAHalf => 25,
            CutoffFactor::Three => 30,
        };
        Dec::from_int(tenths)
            .checked_div(Dec::from_int(10))
            .unwrap_or(Dec::ONE)
    }
}

/// The budget's one fringe row when fringes post by budget (decisions.md,
/// entry 60): an account number (none by default), a description ("Total
/// Fringes" by default) and its place on the topsheet. Display only: where
/// it shows never changes a total, ATL or BTL. A budget setting, saved with
/// the file only when it isn't the default; versions carry it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FringeRow {
    /// Account number shown on the row, or `None` for none.
    pub account: Option<String>,
    /// What the row says, or `None` for [`FringeRow::DEFAULT_DESCRIPTION`].
    pub description: Option<String>,
    /// Where it shows: just above topsheet row `n` (categories and
    /// production totals counted from 0; the topsheet's length or more puts
    /// it after the last of them). `None`, the default, shows it after the
    /// above- and below-the-line summary rows.
    pub position: Option<usize>,
}

impl FringeRow {
    /// What the row says when the budget doesn't say otherwise.
    pub const DEFAULT_DESCRIPTION: &'static str = "Total Fringes";

    /// What the row says.
    pub fn description(&self) -> &str {
        self.description
            .as_deref()
            .filter(|d| !d.trim().is_empty())
            .unwrap_or(FringeRow::DEFAULT_DESCRIPTION)
    }

    /// The account number shown on the row, if any.
    pub fn account(&self) -> Option<&str> {
        self.account.as_deref().filter(|a| !a.trim().is_empty())
    }

    pub fn is_default(&self) -> bool {
        *self == FringeRow::default()
    }

    /// The topsheet row the fringe row shows above, for a topsheet of
    /// `len` rows (`len` itself: after the last one); `None` after the
    /// summary rows, its default place.
    pub fn above(&self, len: usize) -> Option<usize> {
        self.position.map(|p| p.min(len))
    }

    /// Keeps the row in place when a topsheet row is inserted at `at`.
    pub fn row_inserted(&mut self, at: usize) {
        if let Some(p) = &mut self.position {
            if *p >= at {
                *p += 1;
            }
        }
    }

    /// Keeps the row in place when the topsheet row at `at` is removed.
    pub fn row_removed(&mut self, at: usize) {
        if let Some(p) = &mut self.position {
            if *p > at {
                *p -= 1;
            }
        }
    }

    /// Keeps the row above the same topsheet row when the rows are put in
    /// a new order (`order[new]` is the old index of the row now at `new`).
    /// After the last row, it stays after the last row.
    pub fn rows_reordered(&mut self, order: &[usize]) {
        if let Some(p) = &mut self.position {
            *p = order
                .iter()
                .position(|&old| old == *p)
                .unwrap_or(order.len());
        }
    }
}

/// How a budget works out its totals (decisions.md, entry 55). A budget
/// setting, saved with the file; versions carry it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arithmetic {
    /// As Movie Magic Budgeting does: formulas, globals and every money
    /// step in binary doubles, each amount rounded to cents once like
    /// JavaScript's `Math.round(v * 100) / 100` (half up on the double), so
    /// `1 x 1.005` is 1.00. The default, also for files that don't say.
    #[default]
    Mmb,
    /// Exact decimals (9 places), each amount rounded to cents once, half
    /// away from zero (decisions.md, entry 2), so `1 x 1.005` is 1.01.
    Exact,
}

impl Arithmetic {
    pub const ALL: [Arithmetic; 2] = [Arithmetic::Mmb, Arithmetic::Exact];

    /// The name files and commands use: `binary` or `exact`.
    pub fn name(self) -> &'static str {
        match self {
            Arithmetic::Mmb => "binary",
            Arithmetic::Exact => "exact",
        }
    }

    /// The arithmetic named `name` (as [`Arithmetic::name`] writes it).
    /// `mmb`, the earlier name of `binary`, is still read.
    pub fn parse(name: &str) -> Option<Arithmetic> {
        let name = name.trim();
        if name == "mmb" {
            return Some(Arithmetic::Mmb);
        }
        Arithmetic::ALL.into_iter().find(|a| a.name() == name)
    }

    /// The rounding rule in words, as Explain states it.
    pub fn rule(self) -> &'static str {
        match self {
            Arithmetic::Mmb => {
                "spreadsheet arithmetic: binary doubles, each amount rounded to cents once, half up on the binary value"
            }
            Arithmetic::Exact => {
                "exact arithmetic: decimals, each amount rounded to cents once, half away from zero"
            }
        }
    }
}

/// Include or exclude: the two choices of each of Movie Magic's group rules
/// ([`GroupRules`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Inclusion {
    Include,
    Exclude,
}

impl Inclusion {
    pub const ALL: [Inclusion; 2] = [Inclusion::Include, Inclusion::Exclude];

    /// The name files and commands use: `include` or `exclude`.
    pub fn name(self) -> &'static str {
        match self {
            Inclusion::Include => "include",
            Inclusion::Exclude => "exclude",
        }
    }

    /// The choice named `name` (as [`Inclusion::name`] writes it).
    pub fn parse(name: &str) -> Option<Inclusion> {
        Inclusion::ALL.into_iter().find(|i| i.name() == name.trim())
    }
}

/// The budget-wide group rules, as Movie Magic Budgeting has them
/// (decisions.md, entry 57). A budget setting, saved with the file; versions
/// carry it. The defaults are Movie Magic's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GroupRules {
    /// Lines with no group: counted (the default) or left out. Left out
    /// takes adjustment rows with no group along with them.
    pub ungrouped_lines: Inclusion,
    /// A line in a counted group and in a group whose "in budget total" is
    /// off: left out (the default) or counted.
    pub conflict: Inclusion,
    /// Group totals shown with their lines' fringes (the default) or
    /// without. Display only: never changes a budget total.
    pub fringes_in_totals: Inclusion,
}

impl Default for GroupRules {
    fn default() -> GroupRules {
        GroupRules {
            ungrouped_lines: Inclusion::Include,
            conflict: Inclusion::Exclude,
            fringes_in_totals: Inclusion::Include,
        }
    }
}

impl GroupRules {
    /// Whether a line (or adjustment row) counts toward totals under these
    /// rules, given whether each of its groups is in the budget total
    /// (`counted`, one per group): `Err` says why it doesn't.
    pub fn counts(&self, counted: impl IntoIterator<Item = bool>) -> Result<(), LeftOut> {
        let (mut on, mut off) = (0usize, 0usize);
        for c in counted {
            if c {
                on += 1;
            } else {
                off += 1;
            }
        }
        match (on, off) {
            (0, 0) if self.ungrouped_lines == Inclusion::Exclude => Err(LeftOut::Ungrouped),
            (_, 0) => Ok(()),
            (1.., _) if self.conflict == Inclusion::Include => Ok(()),
            _ => Err(LeftOut::GroupOff),
        }
    }
}

/// Why the group rules leave a line out ([`GroupRules::counts`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeftOut {
    /// It has no group, and lines with no group are excluded.
    Ungrouped,
    /// A group of its has "in budget total" off (and conflicts are
    /// excluded, when it also has a counted group).
    GroupOff,
}

/// Which tags a new line copies from the nearest line above it (below it when
/// there is none above) when it is inserted in the editor. A budget setting,
/// saved with the file. Never affects totals by itself: it only decides what
/// a fresh line starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewLineDefaults {
    pub fringes: bool,
    pub groups: bool,
    pub set: bool,
    pub location: bool,
    pub currency: bool,
}

impl Default for NewLineDefaults {
    /// Fringes on, everything else off: a new line usually pays the same
    /// people's fringes as its neighbors, but groups, sets, locations and
    /// currencies are deliberate choices.
    fn default() -> NewLineDefaults {
        NewLineDefaults {
            fringes: true,
            groups: false,
            set: false,
            location: false,
            currency: false,
        }
    }
}

/// Most decimal places a display setting can ask for ([`GlobalInfo::decimals`],
/// [`DisplayPrefs`] rate bands).
pub const MAX_DISPLAY_DECIMALS: u8 = 8;

/// What a global says about itself, besides its formula: a description, the
/// unit its value is in, how its value is shown, and a folder to file it
/// under. Display only: none of it changes any value or total. Keyed by the
/// global's name in [`Budget::global_info`]; a global without an entry has
/// the default (all empty).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlobalInfo {
    pub description: String,
    /// A unit name from [`Budget::units`] (`"Week"`), or empty for none.
    pub unit: String,
    /// Decimal places its value shows with, 0 to [`MAX_DISPLAY_DECIMALS`]
    /// (padded with zeros or rounded half away from zero for display);
    /// `None` shows the value as calculated.
    pub decimals: Option<u8>,
    /// Cells whose formula uses this global show the formula, not its value.
    pub show_by_name: bool,
    /// A folder (category) to file it under, or empty for none.
    pub folder: String,
}

impl GlobalInfo {
    pub fn is_default(&self) -> bool {
        *self == GlobalInfo::default()
    }
}

/// How the budget shows numbers on screen, saved with the file. Display only:
/// none of it changes a stored value or a total (decisions.md, entry 2: the
/// engine stays exact). Tools, not content, like report presets: versions
/// don't carry it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayPrefs {
    /// Rates whose size is below this show with `rate_decimals_below`, the
    /// rest with `rate_decimals_above`. `None`: every rate uses
    /// `rate_decimals_above`.
    pub rate_cutoff: Option<Dec>,
    /// Decimal places for rates below the cutoff; `None` shows them as typed.
    pub rate_decimals_below: Option<u8>,
    /// Decimal places for rates at or above the cutoff (every rate when there
    /// is no cutoff); `None` shows them as typed.
    pub rate_decimals_above: Option<u8>,
    /// A rate whose formula names a global with its own decimals shows with
    /// the first such global's decimals instead of the band's.
    pub rate_global_decimals: bool,
    /// The topsheet shows the above-the-line, below-the-line and combined
    /// summary rows.
    pub summary_rows: bool,
    /// The topsheet shows the net total (after credits).
    pub net_total: bool,
}

impl Default for DisplayPrefs {
    fn default() -> DisplayPrefs {
        DisplayPrefs {
            rate_cutoff: None,
            rate_decimals_below: None,
            rate_decimals_above: None,
            rate_global_decimals: true,
            summary_rows: true,
            net_total: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub description: String,
    /// Optional short code shown in the grid's groups column instead of the
    /// name (`"NQ"`). Display only: lines refer to the group by name.
    pub short_code: Option<String>,
    /// "In budget total". When false, every line tagged with this group is
    /// suppressed: shown, but counted as 0 everywhere.
    pub in_budget_total: bool,
    /// Display color for tagged lines' text, as `#rrggbb`. A line's own
    /// [`Detail::color`] wins. Display only: never affects totals.
    pub color: Option<String>,
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
    /// Notes on the category itself, oldest first.
    pub notes: Vec<Note>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// Chart-of-accounts number, e.g. `"1100"`.
    pub code: String,
    pub name: String,
    pub details: Vec<Detail>,
    /// Notes on the account itself, oldest first.
    pub notes: Vec<Note>,
}

/// Identity of a [`Note`], unique within the list it is in (one row's,
/// account's or category's notes).
pub type NoteId = u64;

/// One dated note on a detail row, an account or a category. Display data:
/// never affects totals.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Note {
    pub id: NoteId,
    /// Plain text, up to [`MAX_NOTE_CHARS`] characters.
    pub text: String,
    /// When it was written, as an ISO 8601 date-time provided by the caller
    /// (the engine never reads the clock). Empty when unknown: a note carried
    /// over from a file that kept a single undated note per row.
    pub created: String,
    /// When its text last changed, if it has.
    pub edited: Option<String>,
    /// Who wrote it: free text from a local setting, if any.
    pub author: Option<String>,
    /// Ticked off in the notes review list.
    pub done: bool,
}

impl Note {
    /// An undated note with just its text.
    pub fn plain(id: NoteId, text: impl Into<String>) -> Note {
        Note {
            id,
            text: text.into(),
            ..Note::default()
        }
    }
}

/// Every note's text, oldest first, with a blank line between them: one
/// cell of a spreadsheet or the clipboard. Blank notes are skipped.
pub fn notes_text(notes: &[Note]) -> String {
    notes
        .iter()
        .map(|n| n.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The id a note added to `notes` gets: one past the highest there.
pub fn next_note_id(notes: &[Note]) -> NoteId {
    notes
        .iter()
        .map(|n| n.id)
        .max()
        .map_or(1, |m| m.saturating_add(1))
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
    /// A drawn rule line between rows. No cost, ignored by all math: never
    /// counted or fringed, and it doesn't break fringe ranges or reset subtotal
    /// and adjustment windows.
    Separator,
}

impl RowKind {
    /// Whether rows of this kind carry a cost of their own (lines and
    /// adjustments). Subtotal and separator rows add nothing.
    pub fn is_costed(self) -> bool {
        matches!(self, RowKind::Line | RowKind::Adjustment)
    }
}

/// Deepest [`Detail::indent`] level.
pub const MAX_INDENT: u8 = 4;

/// Longest [`Note::text`], in characters.
pub const MAX_NOTE_CHARS: usize = 10_000;

/// A person's name as rows are matched by it (decisions.md, entry 79):
/// the ends trimmed, each inner run of spaces one space, and lower case, so
/// "Mara Lee", "mara  lee " and "MARA LEE" are one person.
pub fn person_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
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
    /// Primary unit name. Flat fringes convert the Amount from it through
    /// unit hours.
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
    /// The person this row pays (decisions.md, entry 79), opt-in. Every
    /// counted line and adjustment row with the same person, in any
    /// account, shares each percent fringe's cutoff as one earner; a row
    /// with a person leaves its fringe range for percent fringes. `None`
    /// (the default) changes nothing. See [`Detail::person_name`].
    pub person: Option<String>,
    /// Where the rate came from, when it was filled from a rate table
    /// (decisions.md, entry 90). Never changes a figure: the rate is the
    /// number written; this only says where it came from.
    pub rate_source: Option<crate::rates::RateSource>,
    /// Text style. Display only: never affects totals.
    pub bold: bool,
    pub italic: bool,
    /// Indent level, 0 to [`MAX_INDENT`].
    pub indent: u8,
    /// Text color as `#rrggbb`; overrides the color of the line's groups.
    pub color: Option<String>,
    /// Notes on the row, oldest first.
    pub notes: Vec<Note>,
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
            person: None,
            rate_source: None,
            bold: false,
            italic: false,
            indent: 0,
            color: None,
            notes: Vec::new(),
        }
    }

    /// The row's person as typed, trimmed; `None` when unset or blank.
    pub fn person_name(&self) -> Option<&str> {
        self.person
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
    }

    /// The row's person as the engine matches it ([`person_key`]); `None`
    /// when unset or blank.
    pub fn person_key(&self) -> Option<String> {
        self.person_name().map(person_key)
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

    /// A separator: a drawn rule line with no cost.
    pub fn separator(id: RowId) -> Detail {
        Detail::blank(id, RowKind::Separator, String::new())
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
    /// Show the percent after the name in the topsheet label (percent
    /// charges only). Display only: never changes a total.
    pub label_percent: bool,
    /// Show the excluded total after the name in the topsheet label.
    /// Display only.
    pub label_excluded: bool,
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
    /// Fringes left out of the spend of every group and category row of
    /// this credit (typed-amount rows have no fringes). One list per
    /// credit, not per row (decisions.md, entry 57).
    pub excluded_fringes: BTreeSet<String>,
    /// Where the spend has to be (decisions.md, entry 80): when not empty,
    /// every group, category and account row counts only the lines and
    /// adjustment rows whose location is one of these, with their fringes.
    /// A typed amount is unaffected. Empty (the default) counts every line,
    /// wherever it is.
    pub locations: BTreeSet<String>,
    /// The qualified spend below which the program pays nothing
    /// (decisions.md, entry 88): compared with the rows' net spend added
    /// up, after the locations and excluded fringes, before any cap.
    /// `None`: no minimum.
    pub minimum_spend: Option<Cents>,
    /// A flat fee taken off after the cap and the discount, never taking
    /// the credit below zero (decisions.md, entry 88). 0: no fee.
    pub broker_fee: Cents,
    /// The name of a cap in the budget's `shared_caps` this credit shares
    /// with others (decisions.md, entry 88). `None`: no shared cap.
    pub shared_cap: Option<String>,
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
    /// the credit's `excluded_fringes`.
    Group { group: String },
    /// The total of topsheet category `category` (its counted lines and
    /// adjustment rows), plus their fringes wherever fringes post, except
    /// the credit's `excluded_fringes` (decisions.md, entry 57). A category
    /// that isn't in the budget is 0.
    Category { category: String },
    /// The total of account `account` (its counted lines and adjustment
    /// rows), plus the fringes they earn wherever fringes post, except the
    /// credit's `excluded_fringes` (decisions.md, entry 61). An account
    /// that isn't in the budget is 0; of two with the same number, the
    /// first.
    Account { account: String },
}

impl CreditSource {
    /// Whether the row's spend includes fringes, so the credit's excluded
    /// fringes apply to it (group, category and account rows; not a typed
    /// amount).
    pub fn has_fringes(&self) -> bool {
        !matches!(self, CreditSource::Flat(_))
    }
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
    /// Frozen: these original totals can't be captured again or cleared
    /// until this is turned off, as the reference product's "lock original
    /// totals" switch keeps them (decisions.md, entry 61). Edits still
    /// move the current totals and so the variance.
    pub frozen: bool,
}

/// What kind of copy a budget with a [`Provenance`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProvenanceKind {
    /// Saved for sharing (File > Save a Copy for Sharing). The application
    /// opens it read-only.
    SharedCopy,
    /// An editable copy made from a shared copy; it keeps the shared copy's
    /// source, so it can still be compared with or stored in the original.
    EditableCopy,
    /// A filtered copy of a parent budget (File > New Sub-Budget).
    SubBudget,
    /// A setup pack: setup items and no lines, made to share (see
    /// [`crate::pack`]). Its [`Provenance::pack`] holds the pack's details.
    SetupPack,
}

/// Where a budget came from: stamped on copies made for sharing and on
/// sub-budgets. Display data: never affects totals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub kind: ProvenanceKind,
    /// The title of the budget it was made from.
    pub source_title: String,
    /// The version it was made from (its name), if it was a saved version
    /// rather than the budget as it stood.
    pub source_version: Option<String>,
    /// When it was made, as an ISO 8601 date-time provided by the caller
    /// (the engine never reads the clock).
    pub saved: String,
    /// The application version that made it (`"0.0.1"`).
    pub app_version: String,
    /// A note from whoever made it, or (for a sub-budget) what it keeps.
    pub note: Option<String>,
    /// A setup pack's details (title, version, effective date, sources).
    /// Only on [`ProvenanceKind::SetupPack`].
    pub pack: Option<crate::pack::PackInfo>,
}

impl Provenance {
    /// Whether the application should open this budget read-only.
    pub fn is_shared_copy(&self) -> bool {
        self.kind == ProvenanceKind::SharedCopy
    }
}

/// Identity of a [`Version`]. Unique within a budget's version list.
pub type VersionId = u64;

/// A named copy of a budget's content, saved inside the budget.
///
/// A version is a full copy (every category, account and row), so each one
/// adds about as much to the file as the budget itself. That is fine for v1;
/// see docs/decisions.md.
///
/// The content is shared (`Arc`) and never changed in place: copying a budget
/// (every edit works on a copy, for undo) copies a pointer per version, not
/// every version's rows, and comparing two lists of the same versions stops at
/// the pointers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub id: VersionId,
    pub name: String,
    /// When it was saved, as an ISO 8601 date-time provided by the caller
    /// (for example `2026-10-09T14:30:00Z`). The engine never reads the clock
    /// and stores this text as given.
    pub created: String,
    pub note: Option<String>,
    pub budget: Arc<BudgetSnapshot>,
}

/// Everything in a [`Budget`] except its versions: what a [`Version`] stores.
/// Leaving the version list out means versions never nest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetSnapshot {
    pub title: String,
    pub base_currency: String,
    pub totals_currency: Option<String>,
    pub info: BTreeMap<String, InfoValue>,
    pub currencies: BTreeMap<String, Currency>,
    pub globals: BTreeMap<String, Formula>,
    pub global_info: BTreeMap<String, GlobalInfo>,
    pub units: BTreeMap<String, UnitDef>,
    pub fringes: BTreeMap<String, Fringe>,
    pub fringe_posting: FringePosting,
    pub trailing_level_fringes: TrailingFringes,
    pub cutoff_factor: CutoffFactor,
    pub fringe_row: FringeRow,
    pub arithmetic: Arithmetic,
    pub round_account_totals: bool,
    pub account_coding: AccountCoding,
    pub new_line_defaults: NewLineDefaults,
    pub groups: BTreeMap<String, Group>,
    pub group_rules: GroupRules,
    pub sets: BTreeMap<String, SetDef>,
    pub locations: BTreeMap<String, Location>,
    pub topsheet: Vec<TopsheetRow>,
    pub charges: Vec<Charge>,
    pub credits: Vec<Credit>,
    pub shared_caps: BTreeMap<String, Cents>,
    pub baseline: Option<Baseline>,
}

impl Budget {
    /// A copy of this budget's content, without its versions.
    pub fn snapshot(&self) -> BudgetSnapshot {
        // Destructured completely, so a new Budget field is a compile error here.
        let Budget {
            title,
            base_currency,
            totals_currency,
            info,
            currencies,
            globals,
            global_info,
            units,
            fringes,
            fringe_posting,
            trailing_level_fringes,
            cutoff_factor,
            fringe_row,
            arithmetic,
            round_account_totals,
            account_coding,
            new_line_defaults,
            groups,
            group_rules,
            sets,
            locations,
            topsheet,
            charges,
            credits,
            shared_caps,
            baseline,
            display: _,
            versions: _,
            compact_versions: _,
            report_presets: _,
            tag_presets: _,
            line_presets: _,
            setup_order: _,
            setup_folders: _,
            provenance: _,
            actuals: _,
            ot_patterns: _,
            setup_sources: _,
            rate_tables: _,
        } = self;
        BudgetSnapshot {
            title: title.clone(),
            base_currency: base_currency.clone(),
            totals_currency: totals_currency.clone(),
            info: info.clone(),
            currencies: currencies.clone(),
            globals: globals.clone(),
            global_info: global_info.clone(),
            units: units.clone(),
            fringes: fringes.clone(),
            fringe_posting: fringe_posting.clone(),
            trailing_level_fringes: *trailing_level_fringes,
            cutoff_factor: *cutoff_factor,
            fringe_row: fringe_row.clone(),
            arithmetic: *arithmetic,
            round_account_totals: *round_account_totals,
            account_coding: *account_coding,
            new_line_defaults: *new_line_defaults,
            groups: groups.clone(),
            group_rules: *group_rules,
            sets: sets.clone(),
            locations: locations.clone(),
            topsheet: topsheet.clone(),
            charges: charges.clone(),
            credits: credits.clone(),
            shared_caps: shared_caps.clone(),
            baseline: baseline.clone(),
        }
    }

    /// Splits off the content, leaving the version list (and the report,
    /// tag and line presets, which are not content) behind.
    pub fn into_parts(self) -> (BudgetSnapshot, Vec<Version>) {
        let Budget {
            title,
            base_currency,
            totals_currency,
            info,
            currencies,
            globals,
            global_info,
            units,
            fringes,
            fringe_posting,
            trailing_level_fringes,
            cutoff_factor,
            fringe_row,
            arithmetic,
            round_account_totals,
            account_coding,
            new_line_defaults,
            groups,
            group_rules,
            sets,
            locations,
            topsheet,
            charges,
            credits,
            shared_caps,
            baseline,
            display: _,
            versions,
            compact_versions: _,
            report_presets: _,
            tag_presets: _,
            line_presets: _,
            setup_order: _,
            setup_folders: _,
            provenance: _,
            actuals: _,
            ot_patterns: _,
            setup_sources: _,
            rate_tables: _,
        } = self;
        let content = BudgetSnapshot {
            title,
            base_currency,
            totals_currency,
            info,
            currencies,
            globals,
            global_info,
            units,
            fringes,
            fringe_posting,
            trailing_level_fringes,
            cutoff_factor,
            fringe_row,
            arithmetic,
            round_account_totals,
            account_coding,
            new_line_defaults,
            groups,
            group_rules,
            sets,
            locations,
            topsheet,
            charges,
            credits,
            shared_caps,
            baseline,
        };
        (content, versions)
    }

    /// The version with this id.
    pub fn version(&self, id: VersionId) -> Option<&Version> {
        self.versions.iter().find(|v| v.id == id)
    }

    /// The highest row id anywhere in the budget: current rows, the baseline,
    /// and every version's rows and baseline. A new row id above this can never
    /// be mistaken for an older row when comparing against a version.
    pub fn highest_row_id(&self) -> Option<RowId> {
        fn content_max(s: &BudgetSnapshot) -> Option<RowId> {
            let rows = s
                .categories()
                .flat_map(|c| &c.accounts)
                .flat_map(|a| &a.details)
                .map(|d| d.id)
                .max();
            let base = s
                .baseline
                .as_ref()
                .and_then(|b| b.rows.keys().next_back().copied());
            rows.max(base)
        }
        let rows = self
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .map(|d| d.id)
            .max();
        let base = self
            .baseline
            .as_ref()
            .and_then(|b| b.rows.keys().next_back().copied());
        self.versions
            .iter()
            .map(|v| content_max(&v.budget))
            .fold(rows.max(base), Option::max)
    }
}

impl BudgetSnapshot {
    /// This content as a budget with no versions (for recalc or comparison).
    pub fn to_budget(&self) -> Budget {
        self.clone().into_budget(Vec::new())
    }

    /// This content as a budget carrying `versions` (and no presets).
    pub fn into_budget(self, versions: Vec<Version>) -> Budget {
        let BudgetSnapshot {
            title,
            base_currency,
            totals_currency,
            info,
            currencies,
            globals,
            global_info,
            units,
            fringes,
            fringe_posting,
            trailing_level_fringes,
            cutoff_factor,
            fringe_row,
            arithmetic,
            round_account_totals,
            account_coding,
            new_line_defaults,
            groups,
            group_rules,
            sets,
            locations,
            topsheet,
            charges,
            credits,
            shared_caps,
            baseline,
        } = self;
        Budget {
            title,
            base_currency,
            totals_currency,
            info,
            currencies,
            globals,
            global_info,
            units,
            fringes,
            fringe_posting,
            trailing_level_fringes,
            cutoff_factor,
            fringe_row,
            arithmetic,
            round_account_totals,
            account_coding,
            new_line_defaults,
            groups,
            group_rules,
            sets,
            locations,
            topsheet,
            charges,
            credits,
            shared_caps,
            baseline,
            display: DisplayPrefs::default(),
            versions,
            compact_versions: false,
            report_presets: Vec::new(),
            tag_presets: Vec::new(),
            line_presets: Vec::new(),
            setup_order: crate::order::SetupOrder::default(),
            setup_folders: crate::folders::SetupFolders::default(),
            provenance: None,
            actuals: Arc::default(),
            ot_patterns: Vec::new(),
            setup_sources: crate::pack::SetupSources::default(),
            rate_tables: Vec::new(),
        }
    }

    pub fn categories(&self) -> impl Iterator<Item = &Category> {
        self.topsheet.iter().filter_map(|r| match r {
            TopsheetRow::Category(c) => Some(c),
            TopsheetRow::ProductionTotal(_) => None,
        })
    }
}
