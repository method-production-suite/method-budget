//! "Explain this number" (roadmap 3.5; decisions.md, entry 43).
//!
//! For any total the budget reports (a line, an account, a category, a
//! production level, a fringe total, a charge, a credit, the grand or net
//! total) [`Explainer::explain`] gives a tree of [`Step`]s that reproduces it
//! exactly, in plain words: each line as amount x X x 4X x rate with globals
//! shown by name and value, the currency conversion, every rounding step
//! (unrounded to rounded, by the budget's arithmetic: half away from zero,
//! or half up on the binary double as Movie Magic rounds, saying what the
//! double holds; decisions.md, entry 55), each fringe on each line
//! (percent or flat, the cutoff and how much of it the lines above in a
//! range left, decisions.md, entry 56; a person's one cutoff across
//! accounts and each line's share of it, entry 79), the rate table a
//! line's rate was filled from and how it stands now (entry 90), where fringes post, what a group or a
//! report filter left out and why, a charge's base and exclusions, a credit's
//! gross, excluded, net and caps.
//!
//! The tree is worked out again from the budget, not copied from the report:
//! [`Role::Sum`] steps add up their children, [`Role::Value`] steps are
//! computed from the notes under them, and [`Role::Note`] steps only explain.
//! The signed sum of the leaves equals the number the report shows (tested
//! on every total of the 5,000-line sample and the hand-checked fixtures),
//! so the explanation can never quietly disagree with the budget.
//!
//! Computed on demand only: nothing here runs when a budget is edited.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::diff::quoted;

use super::arith::{converted, float_digits, float_text, percent_of, Money, Num, Window};
use super::fringes::{
    earners, factored_cutoff, flat_count, percent_step, CountFrom, Factor, Factors, FlatCount,
};
use super::persons::{self, person_shares, Split};
use super::topsheet::{counts_at, credit_cap, credit_keep, credit_row, dropped_from};
use super::{recalc, CalcError, Ctx, RecalcOptions};
use crate::decimal::{Dec, Exact};
use crate::expr::Expr;
use crate::model::{
    Arithmetic, Budget, ChargeKind, CreditCap, CreditSource, CutoffFactor, Detail, FlatBasis,
    Formula, FringeKind, FringePosting, LeftOut, RowId, RowKind, TopsheetRow,
};
use crate::report::{Report, RowTotals};
use crate::Cents;

/// The working's fixed words for `code` with `name = value` parameters, as
/// [`Words`]: the English and what it was made from.
macro_rules! say {
    ($code:literal $(, $name:ident = $v:expr)* $(,)?) => {
        Words::said($code, vec![$((stringify!($name), ($v).to_string())),*])
    };
}

// ---------------------------------------------------------------------------
// What can be explained, and what comes back

/// A total to explain. Categories are named by their place among the
/// categories (`ci`, as in [`Report::categories`]), accounts by `ci` and
/// their place in it (`ai`), so codes that repeat can't mix them up.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    /// A detail row's total (a subtotal row: the running total it shows).
    Line(RowId),
    /// The fringe dollars one line earns, every fringe.
    LineFringes(RowId),
    /// One fringe on one line.
    LineFringe(RowId, String),
    /// An account's total as the topsheet counts it.
    Account {
        ci: usize,
        ai: usize,
    },
    /// An account's lines only (its detail total).
    AccountLines {
        ci: usize,
        ai: usize,
    },
    /// The fringes earned by an account's lines.
    AccountFringes {
        ci: usize,
        ai: usize,
    },
    /// A category's total.
    Category(usize),
    /// The fringes earned in a category (its fringe account when fringes
    /// post by category).
    CategoryFringes(usize),
    /// A production level (index into [`Report::levels`]).
    Level(usize),
    /// The fringes posted above a production total.
    LevelFringes(usize),
    Atl,
    Btl,
    AtlBtl,
    /// The budget's one Total Fringes row (fringes posted by budget).
    BudgetFringes,
    /// Applied charge `i` (index into [`Report::charges`]).
    Charge(usize),
    GrandTotal,
    /// Applied credit `i` (index into [`Report::credits`]).
    Credit(usize),
    CreditsTotal,
    NetTotal,
    /// One fringe's total across the budget (included lines).
    Fringe(String),
}

/// How a step counts toward its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Adds up its children: its amount is the signed sum of the children
    /// that aren't notes.
    Sum,
    /// Worked out here, from the notes under it (which are all notes).
    Value,
    /// Explains; counts for nothing. Its amount, if any, is for reading.
    Note,
}

/// Something in the budget a step is about, to go to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Row { ci: usize, ai: usize, row: RowId },
    Account { ci: usize, ai: usize },
    Category { ci: usize },
}

/// One step of an explanation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// Short name: a line's description, a fringe's name, "Rounding".
    pub label: String,
    /// The working in plain words ("40 Days x 3 x 450.00 = 54,000.00").
    pub text: String,
    /// `label` as a message code and its values, when it is fixed words
    /// ("Rounding", "Line 3"); `None` when it is the person's own text (a
    /// description, a fringe's name) or not catalogued: show it as it is.
    pub label_said: Option<Said>,
    /// `text` as a message code and its values, the same way.
    pub text_said: Option<Said>,
    /// What it comes to. Always there for sums and values.
    pub amount: Option<Cents>,
    pub role: Role,
    /// The parent subtracts it (a credit under the net total).
    pub minus: bool,
    pub link: Option<Link>,
    /// The same total explained on its own (to load lazily).
    pub expand: Option<Target>,
    pub children: Vec<Step>,
}

impl Step {
    fn note(label: impl Into<Words>, text: impl Into<Words>) -> Step {
        let label = label.into();
        let text = text.into();
        Step {
            label: label.text,
            text: text.text,
            label_said: label.said,
            text_said: text.said,
            amount: None,
            role: Role::Note,
            minus: false,
            link: None,
            expand: None,
            children: Vec::new(),
        }
    }

    fn note_amount(label: impl Into<Words>, text: impl Into<Words>, amount: Cents) -> Step {
        Step {
            amount: Some(amount),
            ..Step::note(label, text)
        }
    }

    fn value(label: impl Into<Words>, text: impl Into<Words>, amount: Cents) -> Step {
        Step {
            role: Role::Value,
            amount: Some(amount),
            ..Step::note(label, text)
        }
    }

    /// A sum of `children`; its amount is worked out from them.
    fn sum(label: impl Into<Words>, text: impl Into<Words>, children: Vec<Step>) -> R<Step> {
        let mut total: Cents = 0;
        for c in &children {
            if let (Role::Sum | Role::Value, Some(a)) = (c.role, c.amount) {
                let a = if c.minus {
                    a.checked_neg().ok_or_else(ovf)?
                } else {
                    a
                };
                total = total.checked_add(a).ok_or_else(ovf)?;
            }
        }
        Ok(Step {
            role: Role::Sum,
            amount: Some(total),
            children,
            ..Step::note(label, text)
        })
    }

    /// Gives the step other words for its label.
    fn relabel(&mut self, label: impl Into<Words>) {
        let label = label.into();
        self.label = label.text;
        self.label_said = label.said;
    }

    /// Gives the step other words for its text.
    fn retext(&mut self, text: impl Into<Words>) {
        let text = text.into();
        self.text = text.text;
        self.text_said = text.said;
    }

    fn with_link(mut self, link: Link) -> Step {
        self.link = Some(link);
        self
    }

    fn with_expand(mut self, t: Target) -> Step {
        self.expand = Some(t);
        self
    }

    fn with(mut self, children: Vec<Step>) -> Step {
        self.children = children;
        self
    }

    /// The signed sum of the leaves under this step (itself, when it is a
    /// value), checking on the way that every sum adds up. `None` when one
    /// doesn't, or on overflow.
    pub fn leaf_sum(&self) -> Option<i128> {
        let own = match self.role {
            Role::Note => {
                for c in &self.children {
                    c.leaf_sum()?;
                }
                return Some(0);
            }
            Role::Value => {
                if self.children.iter().any(|c| c.role != Role::Note) {
                    return None;
                }
                i128::from(self.amount?)
            }
            Role::Sum => {
                let mut s = 0i128;
                for c in &self.children {
                    s = s.checked_add(c.leaf_sum()?)?;
                }
                if s != i128::from(self.amount?) {
                    return None;
                }
                s
            }
        };
        Some(if self.minus { -own } else { own })
    }

    /// How many steps there are, this one included.
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Step::count).sum::<usize>()
    }
}

/// A total, explained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explanation {
    pub target: Target,
    /// What the number is ("Account 2100 Crew: total").
    pub title: String,
    /// `title` as a message code and its values (see [`Step::label_said`]).
    pub title_said: Option<Said>,
    /// The number as the budget reports it.
    pub reported: Cents,
    /// The arithmetic it is worked out in (the budget's).
    pub arithmetic: Arithmetic,
    /// The working; its amount is `reported`.
    pub root: Step,
}

impl Explanation {
    /// The tree as indented text, `depth` levels deep (`None`: all of it),
    /// after a line saying which arithmetic and rounding rule it uses.
    pub fn to_text(&self, depth: Option<usize>) -> String {
        let mut out = format!(
            "{}: {}\n({})\n",
            self.title,
            money(self.reported),
            self.arithmetic.rule()
        );
        write_step(&mut out, &self.root, 0, depth);
        out
    }
}

fn write_step(out: &mut String, s: &Step, level: usize, depth: Option<usize>) {
    use std::fmt::Write as _;
    let pad = "  ".repeat(level);
    let sign = if s.minus { "less " } else { "" };
    let amount = match (s.role, s.amount) {
        (Role::Note, Some(a)) => format!("  [{}]", money(a)),
        (_, Some(a)) => format!("  = {}", money(a)),
        _ => String::new(),
    };
    let text = if s.text.is_empty() {
        String::new()
    } else {
        format!(": {}", s.text)
    };
    let _ = writeln!(out, "{pad}- {sign}{}{text}{amount}", s.label);
    if depth.is_some_and(|d| level + 1 >= d) {
        if !s.children.is_empty() {
            let _ = writeln!(out, "{pad}  ...");
        }
        return;
    }
    for c in &s.children {
        write_step(out, c, level + 1, depth);
    }
}

/// Why a total can't be explained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplainError(pub String);

impl fmt::Display for ExplainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExplainError {}

impl From<CalcError> for ExplainError {
    fn from(e: CalcError) -> ExplainError {
        ExplainError(e.to_string())
    }
}

type R<T> = Result<T, ExplainError>;

fn ovf() -> ExplainError {
    ExplainError("a number is too large to explain".into())
}

fn gone(what: &str) -> ExplainError {
    ExplainError(format!("{what} is not in the budget"))
}

/// Explains one total of `b` (with `opts` filters, as the report counts).
pub fn explain(b: &Budget, opts: &RecalcOptions, target: &Target) -> R<Explanation> {
    Explainer::new(b, opts)?.explain(target)
}

// ---------------------------------------------------------------------------
// The working's fixed words (decisions.md, entry 94, twelfth pass)

/// A fixed piece of the working as a message code and its values, so a
/// front end can show it in the person's language. `label` and `text`
/// keep the English.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub code: &'static str,
    /// Each `{name}` in the code's words and its value: a number as the
    /// working writes it, or the person's own text (a description, a
    /// fringe's name, an account), never translated.
    pub params: Vec<(&'static str, String)>,
}

/// One of the working's fixed sentences: its code and its English, with a
/// form for one when it counts something (picked by its `count`).
/// method-session's message catalog has the same codes and words (tested
/// there), and the app's table translates them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wording {
    pub code: &'static str,
    pub one: Option<&'static str>,
    pub other: &'static str,
}

const fn w(code: &'static str, english: &'static str) -> Wording {
    Wording {
        code,
        one: None,
        other: english,
    }
}

const fn plural(code: &'static str, one: &'static str, other: &'static str) -> Wording {
    Wording {
        code,
        one: Some(one),
        other,
    }
}

/// The codes of the working's words all start with this.
pub const WORDING_PREFIX: &str = "engine.working.";

/// Every fixed sentence of the working, by code (sorted, tested).
#[rustfmt::skip]
pub const WORDING: &[Wording] = &[
    w("engine.working.a_flat_fee", "a flat fee"),
    w("engine.working.account_fringes", "fringes earned by the account's lines"),
    w("engine.working.account_total", "account total"),
    w("engine.working.adjustment_base", "the lines above, back to the previous adjustment (subtotal rows don't restart it)"),
    w("engine.working.adjustment_base_doubles", "the lines above, back to the previous adjustment (subtotal rows don't restart it), added up in doubles"),
    w("engine.working.after_last_level_neither", "categories after the last production total are neither above nor below the line"),
    w("engine.working.applied_credit", "an applied credit"),
    w("engine.working.at_least_minimum", "the rows' net spend {spend} is at least the minimum of {minimum}"),
    w("engine.working.at_most", "at most {cap}: {capped}"),
    w("engine.working.atl_btl_is", "above-the-line plus below-the-line"),
    w("engine.working.atl_is", "the first production level, when a production total closes it"),
    w("engine.working.below_minimum", "the rows' net spend {spend} is below the minimum of {minimum}, so the credit is 0"),
    w("engine.working.blank_counts_as_0", "blank, counts as 0"),
    w("engine.working.btl_is", "every later production level a production total closes"),
    w("engine.working.cap_on_spend", "the spend the percent applies to is at most {cap}: {spend}"),
    w("engine.working.category_fringe_account", "the category's fringe account (fringe posting: by category)"),
    w("engine.working.category_fringes", "fringes earned in the category"),
    w("engine.working.category_total", "category total"),
    w("engine.working.charge_base", "every category with its fringes, wherever they post, plus the budget's fringe row: the grand total before charges (charges never apply to each other)"),
    w("engine.working.charge_percent_of", "{percent}% of ({base} - {excluded}) = {exact}"),
    w("engine.working.counted_lines_tagged", "counted lines tagged {group}, with their fringes"),
    w("engine.working.currency", "{amount} {currency} x {rate} {base} per {currency} = {exact} {base}"),
    w("engine.working.currency_unrounded", "{amount} {currency} (not rounded yet) x {rate} {base} per {currency} = {exact} {base}"),
    w("engine.working.cutoff_factor", "the budget multiplies every percent cutoff by {factor}: {typed} x {factor} = {cutoff}"),
    w("engine.working.cutoff_used_by_lines_above", "{fringe} stops at {cutoff} per earner, and the lines above in the range used it all, so this line takes nothing"),
    w("engine.working.discount", "less {percent}%: {capped} x {keep} = {exact}"),
    w("engine.working.earns_none", "this line earns none"),
    w("engine.working.earns_nothing_from_it", "this line earns nothing from it"),
    w("engine.working.every_applied_credit", "every applied credit"),
    w("engine.working.every_line_of", "every line of {account}"),
    w("engine.working.fee_converted", "{amount} {currency} x {rate} {base} per {currency} = {exact}"),
    w("engine.working.fee_less", "less {fee}: {amount}"),
    w("engine.working.fee_less_floor", "less {fee}, but never below zero: {taken} taken off, {amount}"),
    w("engine.working.flat_cutoff_unused", "{cutoff} is kept on {fringe} but not used: cutoffs apply to percent fringes only"),
    w("engine.working.flat_once_per_line", "flat, once per line"),
    w("engine.working.flat_per_unit_amount", "flat, per {unit}, amount only"),
    w("engine.working.flat_per_unit_counts", "flat, per {unit}, amount plus counts in X and 4X"),
    w("engine.working.flat_per_unit_quantity", "flat, per {unit}, full quantity"),
    w("engine.working.fringe_kind_flat", "a flat fringe, over every counted line"),
    w("engine.working.fringe_kind_percent", "a percent fringe, no cutoff, over every counted line"),
    w("engine.working.fringe_kind_percent_cutoff", "a percent fringe with a {cutoff} cutoff per earner, over every counted line"),
    w("engine.working.fringe_kind_percent_factor", "a percent fringe with a {cutoff} cutoff per earner ({typed} x the budget's cutoff factor {factor}), over every counted line"),
    w("engine.working.fringe_on_account_lines", "{fringe} on the account's lines"),
    w("engine.working.fringe_range", "range {range}: lines {lines} earn as one person, so they share one cutoff, used up in line order"),
    w("engine.working.fringes_by_line", "fringes earned by line {line}"),
    w("engine.working.fringes_on_top", "earned on top of this total, shown in Fringe $ (not in it)"),
    w("engine.working.grand_total_is", "above and below-the-line, the categories after the last production total, budget fringes and charges"),
    w("engine.working.gross_account", "account {account} with its fringes, wherever fringes post"),
    w("engine.working.gross_category", "category {category} with its fringes, wherever fringes post"),
    w("engine.working.gross_no_account", "account {account} is not in the budget, so 0"),
    w("engine.working.gross_no_category", "category {category} is not in the budget, so 0"),
    w("engine.working.label.adjustment", "Adjustment"),
    w("engine.working.label.after_last_level", "Categories after the last production total"),
    w("engine.working.label.amount", "Amount"),
    w("engine.working.label.atl", "Total above-the-line"),
    w("engine.working.label.atl_btl", "Total above and below-the-line"),
    w("engine.working.label.base", "Base"),
    w("engine.working.label.brokers_fee", "Broker's fee"),
    w("engine.working.label.btl", "Total below-the-line"),
    w("engine.working.label.cap_on_spend", "Cap on spend"),
    w("engine.working.label.charge", "Charge"),
    w("engine.working.label.count", "Count"),
    w("engine.working.label.credit", "Credit"),
    w("engine.working.label.credit_cap", "Credit cap"),
    w("engine.working.label.credits", "Credits"),
    w("engine.working.label.currency", "Currency"),
    w("engine.working.label.cutoff", "Cutoff"),
    w("engine.working.label.cutoff_factor", "Cutoff factor"),
    w("engine.working.label.cutoff_reached", "Cutoff reached"),
    w("engine.working.label.cutoff_used_up", "Cutoff used up"),
    w("engine.working.label.discount", "Discount"),
    w("engine.working.label.earnings", "Earnings"),
    w("engine.working.label.excluded", "Excluded"),
    w("engine.working.label.excluded_amount", "Excluded amount"),
    w("engine.working.label.excluded_fringes", "Excluded fringes"),
    w("engine.working.label.fee", "Fee"),
    w("engine.working.label.fringe", "Fringe"),
    w("engine.working.label.fringe_range", "Fringe range"),
    w("engine.working.label.fringes", "Fringes"),
    w("engine.working.label.fringes_earned_here", "Fringes earned here"),
    w("engine.working.label.grand_total", "Grand total"),
    w("engine.working.label.gross", "Gross"),
    w("engine.working.label.left_out", "Left out"),
    w("engine.working.label.less_credits", "Less credits"),
    w("engine.working.label.line", "Line"),
    w("engine.working.label.line_n", "Line {line}"),
    w("engine.working.label.lines", "Lines"),
    w("engine.working.label.locations", "Locations"),
    w("engine.working.label.minimum_spend", "Minimum spend"),
    w("engine.working.label.net_spend", "Net spend"),
    w("engine.working.label.net_total", "Net total"),
    w("engine.working.label.no_fringes", "No fringes"),
    w("engine.working.label.no_production_totals", "No production totals"),
    w("engine.working.label.not_counted_here", "Not counted here"),
    w("engine.working.label.not_rounded_yet", "Not rounded yet"),
    w("engine.working.label.numbered_fringes", "{number} Fringes"),
    w("engine.working.label.person", "Person"),
    w("engine.working.label.quantity", "Quantity"),
    w("engine.working.label.rate", "Rate"),
    w("engine.working.label.rate_source", "Rate source"),
    w("engine.working.label.rounding", "Rounding"),
    w("engine.working.label.row_n", "Row {row}"),
    w("engine.working.label.rows_added", "Rows added"),
    w("engine.working.label.share", "Share"),
    w("engine.working.label.shared_cap", "Shared cap"),
    w("engine.working.label.subtotal", "Subtotal"),
    w("engine.working.label.total_fringes", "Total fringes"),
    w("engine.working.label.where_fringes_post", "Where fringes post"),
    w("engine.working.label.x", "X"),
    w("engine.working.label.x4", "4X"),
    w("engine.working.left_out_adjustment_ungrouped", "the adjustment row has no group and the group rule leaves out lines with no group, so it shows but counts as 0; counted, it would be {total}"),
    w("engine.working.left_out_by_report", "the report leaves out group {groups}; counted, it would be {total}"),
    w("engine.working.left_out_group_off", "group {groups} has In budget total off, so the line shows but counts as 0; counted, it would be {total}"),
    w("engine.working.left_out_group_off_conflict", "group {groups} has In budget total off (it is also in a counted group, and the group rule leaves out such conflicts), so the line shows but counts as 0; counted, it would be {total}"),
    w("engine.working.left_out_groups_off", "groups {groups} have In budget total off, so the line shows but counts as 0; counted, it would be {total}"),
    w("engine.working.left_out_groups_off_conflict", "groups {groups} have In budget total off (it is also in a counted group, and the group rule leaves out such conflicts), so the line shows but counts as 0; counted, it would be {total}"),
    w("engine.working.left_out_no_fringes", "the line doesn't count, so it earns no fringes"),
    w("engine.working.left_out_not_shown", "the report shows only some groups, and this line has none of them; counted, it would be {total}"),
    w("engine.working.left_out_ungrouped", "the line has no group and the group rule leaves out lines with no group, so it shows but counts as 0; counted, it would be {total}"),
    w("engine.working.level_adds", "the categories since the previous production total"),
    w("engine.working.lines_tagged", "lines tagged {group} plus their fringes"),
    w("engine.working.net_total_is", "grand total less credits"),
    w("engine.working.no_production_totals", "nothing is above or below the line until a production total closes the first level"),
    w("engine.working.not_in_total_account", "not in this total: fringes post to each account's total"),
    w("engine.working.not_in_total_budget", "not in this total: fringes post to the budget's one Total Fringes row"),
    w("engine.working.not_in_total_category", "not in this total: fringes post to each category's fringe account"),
    w("engine.working.not_in_total_level", "not in this total: fringes post above each production total"),
    w("engine.working.only_lines_at", "only lines at {locations} count, with their fringes"),
    w("engine.working.per_earner_left_not_reached", "{cutoff} per earner; {left} of it left after the lines above in the range, not reached"),
    w("engine.working.per_earner_not_reached", "{cutoff} per earner, not reached"),
    w("engine.working.per_person_not_reached", "{cutoff} per person, not reached: {person} is paid {pay} with {fringe}, so each line earns its own percent"),
    w("engine.working.percent_of", "{percent}% of {base} = {exact}"),
    w("engine.working.percent_of_budget", "{percent}% of the budget"),
    w("engine.working.percent_of_capped", "{percent}% of {base} = {exact}, capped at {cap}: {credit}"),
    w("engine.working.percent_of_rate", "{percent}% of {base} = {exact} (rate {formula})"),
    w("engine.working.percent_with_cutoff", "{percent}% with a {cutoff} cutoff"),
    w("engine.working.percent_with_cutoff_per_person", "{percent}% with a {cutoff} cutoff per person"),
    plural("engine.working.person_paid_account", "{person} is paid on {count} line in account {accounts}, so they share one cutoff", "{person} is paid on {count} lines in account {accounts}, so they share one cutoff"),
    plural("engine.working.person_paid_account_no_cutoff", "{person} is paid on {count} line in account {accounts}; {fringe} has no cutoff, so each line earns its own percent", "{person} is paid on {count} lines in account {accounts}; {fringe} has no cutoff, so each line earns its own percent"),
    plural("engine.working.person_paid_accounts", "{person} is paid on {count} line in accounts {accounts}, so they share one cutoff", "{person} is paid on {count} lines in accounts {accounts}, so they share one cutoff"),
    plural("engine.working.person_paid_accounts_no_cutoff", "{person} is paid on {count} line in accounts {accounts}; {fringe} has no cutoff, so each line earns its own percent", "{person} is paid on {count} lines in accounts {accounts}; {fringe} has no cutoff, so each line earns its own percent"),
    w("engine.working.person_past_cutoff", "{person} is paid {pay} with {fringe}, past the {cutoff} cutoff, so {person} earns {percent}% of {cutoff} = {exact}"),
    w("engine.working.person_past_cutoff_rate", "{person} is paid {pay} with {fringe}, past the {cutoff} cutoff, so {person} earns {percent}% of {cutoff} = {exact} (rate {formula})"),
    w("engine.working.posted_above_level", "posted above the production total (fringe posting: by production level)"),
    w("engine.working.posted_to_account", "posted to the account (fringe posting: by account)"),
    w("engine.working.posting_account", "fringes post to each account's total (a budget setting)"),
    w("engine.working.posting_budget", "fringes post to the budget's one Total Fringes row (a budget setting)"),
    w("engine.working.posting_category", "fringes post to each category's fringe account (a budget setting)"),
    w("engine.working.posting_level", "fringes post above each production total (a budget setting)"),
    w("engine.working.rounded_once", "rows are added, capped and discounted before the one rounding"),
    w("engine.working.rounds_double", "{amount} rounds to {rounded} (spreadsheet rounding: half up on the binary value)"),
    w("engine.working.rounds_double_held", "{amount} is {held} as a binary double, so it rounds to {rounded} (spreadsheet rounding: half up on the binary value)"),
    w("engine.working.rounds_exact", "{amount} rounds half away from zero to {rounded}"),
    w("engine.working.row_adjustment", "{code} line {line}, adjustment"),
    w("engine.working.row_line", "{code} line {line}, line"),
    w("engine.working.row_line_left_out", "{code} line {line}, line, left out"),
    w("engine.working.share", "this line's part of {person}'s {amount}, by pay: {amount} x {total} / {pay}, {floor} rounded down"),
    w("engine.working.share_extra_cent", "this line's part of {person}'s {amount}, by pay: {amount} x {total} / {pay}, {floor} rounded down, plus one of the cents left over (shares rounded down; the left-over cents go to the largest remainders, then the lowest row id)"),
    w("engine.working.shared_cap_missing", "{name} is not in the budget's shared caps, so it holds nothing down"),
    w("engine.working.shared_cap_share", "{name} caps this credit and {others} at {cap} together; its share, in proportion to {capped}, is {share}"),
    w("engine.working.shared_cap_within", "{name} caps the credits sharing it at {cap} together, which they are within"),
    w("engine.working.stops_per_earner", "{fringe} stops at {cutoff} per earner"),
    w("engine.working.stops_per_earner_left", "{fringe} stops at {cutoff} per earner; {left} of it left after the lines above in the range"),
    w("engine.working.stops_per_person", "{fringe} stops at {cutoff} per person, so nothing is earned"),
    w("engine.working.subtotal_adds", "the lines and adjustments since the previous subtotal row"),
    w("engine.working.taken_off_base", "taken off the base"),
    w("engine.working.taken_off_fee", "taken off the fee (typed amounts, and the lines of excluded groups, accounts and categories with their fringes)"),
    w("engine.working.this_lines_total", "this line's total"),
    w("engine.working.title.account_fringes", "Account {account}: fringes"),
    w("engine.working.title.account_lines", "Account {account}: lines"),
    w("engine.working.title.account_total", "Account {account}: total"),
    w("engine.working.title.after_last_level_fringes", "Categories after the last production total: fringes"),
    w("engine.working.title.after_last_level_total", "Categories after the last production total: total"),
    w("engine.working.title.category_fringes", "Category {category}: fringes"),
    w("engine.working.title.category_total", "Category {category}: total"),
    w("engine.working.title.charge", "Charge {name}"),
    w("engine.working.title.credit", "Credit {name}"),
    w("engine.working.title.fringe_total", "Fringe {fringe}: total"),
    w("engine.working.title.level_fringes", "{level}: fringes"),
    w("engine.working.title.level_total", "{level}: total"),
    w("engine.working.title.line_described_fringe", "{code} line {line} ({description}): {fringe}"),
    w("engine.working.title.line_described_fringes", "{code} line {line} ({description}): fringes"),
    w("engine.working.title.line_described_total", "{code} line {line} ({description}): total"),
    w("engine.working.title.line_fringe", "{code} line {line}: {fringe}"),
    w("engine.working.title.line_fringes", "{code} line {line}: fringes"),
    w("engine.working.title.line_total", "{code} line {line}: total"),
    w("engine.working.total_fringes_is", "every fringe in the budget, outside above and below-the-line (fringe posting: by budget)"),
    w("engine.working.trailing_fringes_dropped", "no production total follows these categories, so their fringes post nowhere and count in no total (trailing level fringes: drop)"),
    w("engine.working.typed_in", "typed in"),
    w("engine.working.whole_units_half_away", "the budget rounds account totals to whole units: the lines come to {lines}, which rounds to the nearest whole unit, a half away from zero to {whole}"),
    w("engine.working.whole_units_half_up", "the budget rounds account totals to whole units: the lines come to {lines}, which rounds to the nearest whole unit, a half up to {whole}"),
];

/// Words in the working: the English, and the code and values it was made
/// from when it is fixed words (`None`: the person's own text, or not
/// catalogued).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Words {
    text: String,
    said: Option<Said>,
}

impl Words {
    /// `code`'s English with `params` filled in. Every `{name}` in it has a
    /// value and every value a `{name}` (checked in debug builds).
    fn said(code: &'static str, params: Vec<(&'static str, String)>) -> Words {
        let Ok(i) = WORDING.binary_search_by(|w| w.code.cmp(code)) else {
            debug_assert!(false, "{code} is not in WORDING");
            return Words::from(code);
        };
        let w = WORDING[i];
        let one = params.iter().any(|(n, v)| *n == "count" && v == "1");
        let english = match w.one {
            Some(o) if one => o,
            _ => w.other,
        };
        let (text, used, missing) = fill(english, &params);
        debug_assert!(
            used.len() == params.len() && !missing,
            "{code}: the values {params:?} don't fit {english:?}"
        );
        Words {
            text,
            said: Some(Said { code, params }),
        }
    }
}

impl From<String> for Words {
    fn from(text: String) -> Words {
        Words { text, said: None }
    }
}

impl From<&str> for Words {
    fn from(text: &str) -> Words {
        Words::from(text.to_string())
    }
}

impl From<&String> for Words {
    fn from(text: &String) -> Words {
        Words::from(text.clone())
    }
}

/// `template` with each `{name}` replaced by its value in one pass, as
/// method-session's `messages::fill` does, the names that were used, and
/// whether a name had no value.
fn fill(
    template: &str,
    params: &[(&'static str, String)],
) -> (String, BTreeSet<&'static str>, bool) {
    let mut out = String::with_capacity(template.len());
    let mut used = BTreeSet::new();
    let mut missing = false;
    let mut rest = template;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let name = after.find('}').map(|end| &after[..end]).filter(|n| {
            !n.is_empty()
                && n.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        });
        match name.and_then(|n| params.iter().find(|(p, _)| *p == n)) {
            Some((p, v)) => {
                out.push_str(v);
                used.insert(*p);
                rest = &after[p.len() + 1..];
            }
            None => {
                missing |= name.is_some();
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    (out, used, missing)
}

// ---------------------------------------------------------------------------
// Number formatting for the plain words

/// `123456` cents as `1,234.56`, negatives with a minus sign.
pub fn money(c: Cents) -> String {
    crate::CurrencyFormat::default().number(c)
}

/// A decimal amount of money: at least two places, more when it has them.
pub(crate) fn dmoney(d: impl std::fmt::Display) -> String {
    let s = d.to_string();
    let (sign, s) = match s.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", s.as_str()),
    };
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    let frac = if frac.len() < 2 {
        format!("{frac:0<2}")
    } else {
        frac.to_string()
    };
    format!("{sign}{}.{frac}", group(int))
}

/// A plain decimal, thousands grouped.
fn num(d: Dec) -> String {
    let s = d.to_string();
    let (sign, s) = match s.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", s.as_str()),
    };
    match s.split_once('.') {
        Some((i, f)) => format!("{sign}{}.{f}", group(i)),
        None => format!("{sign}{}", group(s)),
    }
}

fn group(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// How `m` rounded to `rounded`, by the budget's arithmetic, when rounding
/// changed it: "6,125.819 rounds half away from zero to 6,125.82", or for
/// Movie Magic arithmetic "1.005 is 1.00499999999999989341... as a binary
/// double, so it rounds to 1.00 (Movie Magic rounding: half up on the
/// double)".
fn rounding(m: impl Into<Money>, rounded: Cents) -> Option<Step> {
    let m = m.into();
    (!m.is_cents(rounded)).then(|| {
        let text = match m {
            Money::Exact(_) => say!(
                "engine.working.rounds_exact",
                amount = dmoney(m),
                rounded = money(rounded)
            ),
            Money::F64(v) => {
                let held = float_digits(v);
                if held == float_text(v) {
                    say!(
                        "engine.working.rounds_double",
                        amount = dmoney(m),
                        rounded = money(rounded)
                    )
                } else {
                    say!(
                        "engine.working.rounds_double_held",
                        amount = dmoney(m),
                        held = dmoney(held),
                        rounded = money(rounded)
                    )
                }
            }
        };
        Step::note(say!("engine.working.label.rounding"), text)
    })
}

// ---------------------------------------------------------------------------
// The working, done once per explainer

/// One row worked out.
struct RowCalc {
    kind: RowKind,
    included: bool,
    /// Base currency; 0 when left out.
    total: Cents,
    /// The row's value step (lines and adjustments).
    value: Option<Step>,
    /// For subtotal rows: indexes of the rows it adds.
    adds: Vec<usize>,
    /// Each fringe the line earned, in name order, as a value step.
    fringes: Vec<(String, Step)>,
    fringe_total: Cents,
}

struct AccountCalc {
    rows: Vec<RowCalc>,
    fringe_total: Cents,
}

/// Works out every line and fringe of a budget once, then explains any of
/// its totals.
pub struct Explainer<'a> {
    b: &'a Budget,
    report: Report,
    accounts: Vec<Vec<AccountCalc>>,
}

impl<'a> Explainer<'a> {
    /// Recalculates `b` (errors are the engine's) and works out every line.
    pub fn new(b: &'a Budget, opts: &RecalcOptions) -> R<Explainer<'a>> {
        let report = recalc(b, opts)?;
        let ctx = Ctx::new(b, opts)?;
        let mut accounts = Vec::new();
        for (ci, cat) in b.categories().enumerate() {
            let mut list = Vec::with_capacity(cat.accounts.len());
            for (ai, acct) in cat.accounts.iter().enumerate() {
                list.push(account_calc(&ctx, ci, ai, &acct.code, &acct.details)?);
            }
            accounts.push(list);
        }
        if persons::any(b) {
            persons_calc(&ctx, &mut accounts)?;
        }
        Ok(Explainer {
            b,
            report,
            accounts,
        })
    }

    /// The report the explanations are checked against.
    pub fn report(&self) -> &Report {
        &self.report
    }

    /// Every total this budget has, in topsheet order then setup order:
    /// each line (and its fringes), account, category, level, the summary
    /// rows, each charge and credit, and each fringe's total.
    pub fn targets(&self) -> Vec<Target> {
        let mut out = Vec::new();
        for (ci, cat) in self.b.categories().enumerate() {
            for (ai, acct) in cat.accounts.iter().enumerate() {
                for (d, r) in acct.details.iter().zip(&self.accounts[ci][ai].rows) {
                    if d.kind == RowKind::Separator {
                        continue;
                    }
                    out.push(Target::Line(d.id));
                    if d.kind.is_costed() {
                        out.push(Target::LineFringes(d.id));
                        for (name, _) in &r.fringes {
                            out.push(Target::LineFringe(d.id, name.clone()));
                        }
                    }
                }
                out.push(Target::Account { ci, ai });
                out.push(Target::AccountLines { ci, ai });
                out.push(Target::AccountFringes { ci, ai });
            }
            out.push(Target::Category(ci));
            out.push(Target::CategoryFringes(ci));
        }
        for li in 0..self.report.levels.len() {
            out.push(Target::Level(li));
            out.push(Target::LevelFringes(li));
        }
        out.extend([
            Target::Atl,
            Target::Btl,
            Target::AtlBtl,
            Target::BudgetFringes,
        ]);
        out.extend((0..self.report.charges.len()).map(Target::Charge));
        out.push(Target::GrandTotal);
        out.extend((0..self.report.credits.len()).map(Target::Credit));
        out.extend([Target::CreditsTotal, Target::NetTotal]);
        out.extend(self.b.fringes.keys().cloned().map(Target::Fringe));
        out
    }

    /// The number the report shows for `t`.
    pub fn reported(&self, t: &Target) -> R<Cents> {
        let r = &self.report;
        Ok(match t {
            Target::Line(id) => {
                let (ci, ai, i) = self.find_row(*id)?;
                let row = &r.categories[ci].accounts[ai].rows[i];
                row.subtotal.unwrap_or(row.total)
            }
            Target::LineFringes(id) => {
                let (ci, ai, i) = self.find_row(*id)?;
                r.categories[ci].accounts[ai].rows[i].fringe_total
            }
            Target::LineFringe(id, name) => {
                let (ci, ai, i) = self.find_row(*id)?;
                r.categories[ci].accounts[ai].rows[i]
                    .fringes
                    .get(name)
                    .copied()
                    .unwrap_or(0)
            }
            Target::Account { ci, ai } => self.report_account(*ci, *ai)?.total,
            Target::AccountLines { ci, ai } => self.report_account(*ci, *ai)?.details_total,
            Target::AccountFringes { ci, ai } => self.report_account(*ci, *ai)?.fringe_total,
            Target::Category(ci) => self.report_category(*ci)?.total,
            Target::CategoryFringes(ci) => self.report_category(*ci)?.fringe_total,
            Target::Level(li) => {
                r.levels
                    .get(*li)
                    .ok_or_else(|| gone("that production level"))?
                    .total
            }
            Target::LevelFringes(li) => {
                r.levels
                    .get(*li)
                    .ok_or_else(|| gone("that production level"))?
                    .posted_fringes
            }
            Target::Atl => r.total_atl,
            Target::Btl => r.total_btl,
            Target::AtlBtl => r.atl_btl,
            Target::BudgetFringes => r.budget_fringes,
            Target::Charge(i) => r.charges.get(*i).ok_or_else(|| gone("that charge"))?.amount,
            Target::GrandTotal => r.grand_total,
            Target::Credit(i) => r.credits.get(*i).ok_or_else(|| gone("that credit"))?.amount,
            Target::CreditsTotal => r.credits_total,
            Target::NetTotal => r.net_total,
            Target::Fringe(name) => {
                r.fringes
                    .get(name)
                    .ok_or_else(|| gone(&format!("fringe {name}")))?
                    .total
            }
        })
    }

    fn report_account(&self, ci: usize, ai: usize) -> R<&crate::report::AccountTotals> {
        self.report
            .categories
            .get(ci)
            .and_then(|c| c.accounts.get(ai))
            .ok_or_else(|| gone("that account"))
    }

    fn report_category(&self, ci: usize) -> R<&crate::report::CategoryTotals> {
        self.report
            .categories
            .get(ci)
            .ok_or_else(|| gone("that category"))
    }

    /// Where row `id` is: (category, account, row index).
    pub fn find_row(&self, id: RowId) -> R<(usize, usize, usize)> {
        for (ci, cat) in self.b.categories().enumerate() {
            for (ai, acct) in cat.accounts.iter().enumerate() {
                if let Some(i) = acct.details.iter().position(|d| d.id == id) {
                    return Ok((ci, ai, i));
                }
            }
        }
        Err(gone(&format!("line {id}")))
    }

    /// Explains `t`.
    pub fn explain(&self, t: &Target) -> R<Explanation> {
        let reported = self.reported(t)?;
        let (title, root) = match t {
            Target::Line(id) => self.line(*id)?,
            Target::LineFringes(id) => self.line_fringes(*id)?,
            Target::LineFringe(id, name) => self.line_fringe(*id, name)?,
            Target::Account { ci, ai } => self.account(*ci, *ai)?,
            Target::AccountLines { ci, ai } => self.account_lines(*ci, *ai)?,
            Target::AccountFringes { ci, ai } => self.account_fringes(*ci, *ai)?,
            Target::Category(ci) => self.category(*ci)?,
            Target::CategoryFringes(ci) => self.category_fringes(*ci)?,
            Target::Level(li) => self.level(*li)?,
            Target::LevelFringes(li) => self.level_fringes(*li)?,
            Target::Atl => self.atl()?,
            Target::Btl => self.btl()?,
            Target::AtlBtl => self.atl_btl()?,
            Target::BudgetFringes => self.budget_fringes()?,
            Target::Charge(i) => self.charge(*i)?,
            Target::GrandTotal => self.grand_total()?,
            Target::Credit(i) => self.credit(*i)?,
            Target::CreditsTotal => self.credits_total()?,
            Target::NetTotal => self.net_total()?,
            Target::Fringe(name) => self.fringe(name)?,
        };
        Ok(Explanation {
            target: t.clone(),
            title: title.text,
            title_said: title.said,
            reported,
            arithmetic: self.b.arithmetic,
            root,
        })
    }

    // -----------------------------------------------------------------------
    // Lines

    fn line(&self, id: RowId) -> R<(Words, Step)> {
        let (ci, ai, i) = self.find_row(id)?;
        let acct = self.acct(ci, ai);
        let d = &acct.details[i];
        let r = &self.accounts[ci][ai].rows[i];
        let title = row_title(&acct.code, i, d, RowPart::Total);
        match d.kind {
            RowKind::Separator => Err(ExplainError(
                "a separator is a drawn rule: it has no total".into(),
            )),
            RowKind::Subtotal => {
                let calc = &self.accounts[ci][ai];
                let children = r
                    .adds
                    .iter()
                    .filter_map(|&k| calc.rows[k].value.clone())
                    .collect();
                let label = match d.description.trim() {
                    "" => say!("engine.working.label.subtotal"),
                    t => Words::from(t),
                };
                let s = Step::sum(label, say!("engine.working.subtotal_adds"), children)?
                    .with_link(Link::Row { ci, ai, row: id });
                Ok((title, s))
            }
            RowKind::Line | RowKind::Adjustment => {
                let mut s = r.value.clone().ok_or_else(|| gone("that line"))?;
                if r.included && r.fringe_total != 0 {
                    s.children.push(
                        Step::note_amount(
                            say!("engine.working.label.fringes"),
                            say!("engine.working.fringes_on_top"),
                            r.fringe_total,
                        )
                        .with_expand(Target::LineFringes(id)),
                    );
                }
                Ok((title, s))
            }
        }
    }

    /// The fringe steps of row `i` as a sum.
    fn row_fringes_step(&self, ci: usize, ai: usize, i: usize) -> R<Step> {
        let acct = self.acct(ci, ai);
        let d = &acct.details[i];
        let r = &self.accounts[ci][ai].rows[i];
        let mut children: Vec<Step> = r.fringes.iter().map(|(_, s)| s.clone()).collect();
        if !r.included && !d.fringes.is_empty() {
            children.push(Step::note(
                say!("engine.working.label.left_out"),
                say!("engine.working.left_out_no_fringes"),
            ));
        }
        Ok(Step::sum(
            line_label(i, d),
            say!("engine.working.fringes_by_line", line = i + 1),
            children,
        )?
        .with_link(Link::Row { ci, ai, row: d.id })
        .with_expand(Target::LineFringes(d.id)))
    }

    fn line_fringes(&self, id: RowId) -> R<(Words, Step)> {
        let (ci, ai, i) = self.find_row(id)?;
        let acct = self.acct(ci, ai);
        let title = row_title(&acct.code, i, &acct.details[i], RowPart::Fringes);
        let mut s = self.row_fringes_step(ci, ai, i)?;
        s.expand = None;
        if s.children.is_empty() {
            s.children.push(Step::note(
                say!("engine.working.label.no_fringes"),
                say!("engine.working.earns_none"),
            ));
        }
        s.children.push(self.posting_note());
        Ok((title, s))
    }

    fn line_fringe(&self, id: RowId, name: &str) -> R<(Words, Step)> {
        let (ci, ai, i) = self.find_row(id)?;
        let acct = self.acct(ci, ai);
        let r = &self.accounts[ci][ai].rows[i];
        let title = row_title(&acct.code, i, &acct.details[i], RowPart::Fringe(name));
        let s = r
            .fringes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, s)| s.clone())
            .unwrap_or_else(|| {
                Step::value(name, say!("engine.working.earns_nothing_from_it"), 0)
                    .with_link(Link::Row { ci, ai, row: id })
            });
        Ok((title, s))
    }

    // -----------------------------------------------------------------------
    // Accounts and categories

    fn acct(&self, ci: usize, ai: usize) -> &crate::model::Account {
        &self
            .b
            .categories()
            .nth(ci)
            .expect("checked by the caller")
            .accounts[ai]
    }

    fn check_account(&self, ci: usize, ai: usize) -> R<()> {
        self.accounts
            .get(ci)
            .and_then(|c| c.get(ai))
            .map(|_| ())
            .ok_or_else(|| gone("that account"))
    }

    fn account_name(&self, ci: usize, ai: usize) -> String {
        let a = self.acct(ci, ai);
        format!("{} {}", a.code, a.name).trim().to_string()
    }

    /// The account's rows as a sum (subtotal and separator rows add nothing
    /// and are left out).
    fn lines_step(&self, ci: usize, ai: usize) -> R<Step> {
        let calc = &self.accounts[ci][ai];
        let mut children: Vec<Step> = calc.rows.iter().filter_map(|r| r.value.clone()).collect();
        if self.b.round_account_totals {
            let lines = calc
                .rows
                .iter()
                .try_fold(0, |a: Cents, r| a.checked_add(r.total))
                .ok_or_else(ovf)?;
            let whole = super::arith::whole_units(self.b.arithmetic, lines).ok_or_else(ovf)?;
            if whole != lines {
                let text = match self.b.arithmetic {
                    Arithmetic::Mmb => say!(
                        "engine.working.whole_units_half_up",
                        lines = money(lines),
                        whole = money(whole)
                    ),
                    Arithmetic::Exact => say!(
                        "engine.working.whole_units_half_away",
                        lines = money(lines),
                        whole = money(whole)
                    ),
                };
                children.push(Step::value(
                    say!("engine.working.label.rounding"),
                    text,
                    whole.checked_sub(lines).ok_or_else(ovf)?,
                ));
            }
        }
        Ok(Step::sum(
            say!("engine.working.label.lines"),
            say!(
                "engine.working.every_line_of",
                account = self.account_name(ci, ai)
            ),
            children,
        )?
        .with_link(Link::Account { ci, ai })
        .with_expand(Target::AccountLines { ci, ai }))
    }

    fn account_fringes_step(&self, ci: usize, ai: usize) -> R<Step> {
        let acct = self.acct(ci, ai);
        let mut children = Vec::new();
        for (i, d) in acct.details.iter().enumerate() {
            if d.kind.is_costed() && !self.accounts[ci][ai].rows[i].fringes.is_empty() {
                children.push(self.row_fringes_step(ci, ai, i)?);
            }
        }
        Ok(Step::sum(
            self.account_name(ci, ai),
            say!("engine.working.account_fringes"),
            children,
        )?
        .with_link(Link::Account { ci, ai })
        .with_expand(Target::AccountFringes { ci, ai }))
    }

    /// The account's total as its category counts it.
    fn account_step(&self, ci: usize, ai: usize) -> R<Step> {
        let mut children = vec![self.lines_step(ci, ai)?];
        if self.b.fringe_posting == FringePosting::Account {
            let mut f = self.account_fringes_step(ci, ai)?;
            f.relabel(say!("engine.working.label.fringes"));
            f.retext(say!("engine.working.posted_to_account"));
            children.push(f);
        } else if self.accounts[ci][ai].fringe_total != 0 {
            children.push(
                Step::note_amount(
                    say!("engine.working.label.fringes_earned_here"),
                    self.not_in_this_total(),
                    self.accounts[ci][ai].fringe_total,
                )
                .with_expand(Target::AccountFringes { ci, ai }),
            );
        }
        Ok(Step::sum(
            self.account_name(ci, ai),
            say!("engine.working.account_total"),
            children,
        )?
        .with_link(Link::Account { ci, ai })
        .with_expand(Target::Account { ci, ai }))
    }

    fn account(&self, ci: usize, ai: usize) -> R<(Words, Step)> {
        self.check_account(ci, ai)?;
        let mut s = self.account_step(ci, ai)?;
        s.expand = None;
        let title = say!(
            "engine.working.title.account_total",
            account = self.account_name(ci, ai)
        );
        Ok((title, s))
    }

    fn account_lines(&self, ci: usize, ai: usize) -> R<(Words, Step)> {
        self.check_account(ci, ai)?;
        let mut s = self.lines_step(ci, ai)?;
        s.expand = None;
        let title = say!(
            "engine.working.title.account_lines",
            account = self.account_name(ci, ai)
        );
        Ok((title, s))
    }

    fn account_fringes(&self, ci: usize, ai: usize) -> R<(Words, Step)> {
        self.check_account(ci, ai)?;
        let mut s = self.account_fringes_step(ci, ai)?;
        s.expand = None;
        s.children.push(self.posting_note());
        let title = say!(
            "engine.working.title.account_fringes",
            account = self.account_name(ci, ai)
        );
        Ok((title, s))
    }

    fn category_name(&self, ci: usize) -> String {
        let c = self.b.categories().nth(ci).expect("checked by the caller");
        format!("{} {}", c.code, c.name).trim().to_string()
    }

    fn check_category(&self, ci: usize) -> R<()> {
        if ci < self.accounts.len() {
            Ok(())
        } else {
            Err(gone("that category"))
        }
    }

    fn category_fringes_step(&self, ci: usize) -> R<Step> {
        let children = (0..self.accounts[ci].len())
            .filter(|&ai| self.accounts[ci][ai].fringe_total != 0 || self.has_fringes(ci, ai))
            .map(|ai| self.account_fringes_step(ci, ai))
            .collect::<R<Vec<_>>>()?;
        Ok(Step::sum(
            self.category_name(ci),
            say!("engine.working.category_fringes"),
            children,
        )?
        .with_link(Link::Category { ci })
        .with_expand(Target::CategoryFringes(ci)))
    }

    fn has_fringes(&self, ci: usize, ai: usize) -> bool {
        self.accounts[ci][ai]
            .rows
            .iter()
            .any(|r| !r.fringes.is_empty())
    }

    fn category_step(&self, ci: usize) -> R<Step> {
        let mut children = (0..self.accounts[ci].len())
            .map(|ai| self.account_step(ci, ai))
            .collect::<R<Vec<_>>>()?;
        match &self.b.fringe_posting {
            FringePosting::Category { .. } => {
                let number = self
                    .report
                    .categories
                    .get(ci)
                    .and_then(|c| c.fringe_account.as_ref())
                    .map(|(n, _)| n.clone())
                    .unwrap_or_default();
                let mut f = self.category_fringes_step(ci)?;
                f.relabel(match number.trim() {
                    "" => say!("engine.working.label.fringes"),
                    _ => say!(
                        "engine.working.label.numbered_fringes",
                        number = number.trim_start()
                    ),
                });
                f.retext(say!("engine.working.category_fringe_account"));
                children.push(f);
            }
            FringePosting::Account => {}
            _ => {
                let total = self.category_fringe_total(ci)?;
                if total != 0 {
                    children.push(
                        Step::note_amount(
                            say!("engine.working.label.fringes_earned_here"),
                            self.not_in_this_total(),
                            total,
                        )
                        .with_expand(Target::CategoryFringes(ci)),
                    );
                }
            }
        }
        Ok(Step::sum(
            self.category_name(ci),
            say!("engine.working.category_total"),
            children,
        )?
        .with_link(Link::Category { ci })
        .with_expand(Target::Category(ci)))
    }

    fn category_fringe_total(&self, ci: usize) -> R<Cents> {
        self.accounts[ci]
            .iter()
            .try_fold(0, |a: Cents, c| a.checked_add(c.fringe_total))
            .ok_or_else(ovf)
    }

    fn category(&self, ci: usize) -> R<(Words, Step)> {
        self.check_category(ci)?;
        let mut s = self.category_step(ci)?;
        s.expand = None;
        let title = say!(
            "engine.working.title.category_total",
            category = self.category_name(ci)
        );
        Ok((title, s))
    }

    fn category_fringes(&self, ci: usize) -> R<(Words, Step)> {
        self.check_category(ci)?;
        let mut s = self.category_fringes_step(ci)?;
        s.expand = None;
        s.children.push(self.posting_note());
        let title = say!(
            "engine.working.title.category_fringes",
            category = self.category_name(ci)
        );
        Ok((title, s))
    }

    // -----------------------------------------------------------------------
    // Topsheet

    /// "not in this total: " and where fringes post.
    fn not_in_this_total(&self) -> Words {
        match &self.b.fringe_posting {
            FringePosting::Budget => say!("engine.working.not_in_total_budget"),
            FringePosting::ProductionLevel => say!("engine.working.not_in_total_level"),
            FringePosting::Category { .. } => say!("engine.working.not_in_total_category"),
            FringePosting::Account => say!("engine.working.not_in_total_account"),
        }
    }

    fn posting_note(&self) -> Step {
        let text = match &self.b.fringe_posting {
            FringePosting::Budget => say!("engine.working.posting_budget"),
            FringePosting::ProductionLevel => say!("engine.working.posting_level"),
            FringePosting::Category { .. } => say!("engine.working.posting_category"),
            FringePosting::Account => say!("engine.working.posting_account"),
        };
        Step::note(say!("engine.working.label.where_fringes_post"), text)
    }

    fn level_fringes_step(&self, li: usize) -> R<Step> {
        let lv = self
            .report
            .levels
            .get(li)
            .ok_or_else(|| gone("that production level"))?;
        let dropped = dropped_from(self.b).is_some_and(|d| lv.start >= d);
        let children = if dropped {
            vec![Step::note(
                say!("engine.working.label.left_out"),
                say!("engine.working.trailing_fringes_dropped"),
            )]
        } else if self.b.fringe_posting == FringePosting::ProductionLevel {
            (lv.start..lv.end)
                .map(|ci| self.category_fringes_step(ci))
                .collect::<R<Vec<_>>>()?
        } else {
            Vec::new()
        };
        Ok(Step::sum(
            say!("engine.working.label.fringes"),
            say!("engine.working.posted_above_level"),
            children,
        )?
        .with_expand(Target::LevelFringes(li)))
    }

    /// The production total's name, or `None` for the categories after the
    /// last one.
    fn level_name(&self, li: usize) -> Option<String> {
        match self.report.levels.get(li) {
            Some(l) if !l.name.is_empty() => Some(l.name.clone()),
            _ => None,
        }
    }

    fn level_step(&self, li: usize) -> R<Step> {
        let lv = self
            .report
            .levels
            .get(li)
            .ok_or_else(|| gone("that production level"))?;
        let mut children = (lv.start..lv.end)
            .map(|ci| self.category_step(ci))
            .collect::<R<Vec<_>>>()?;
        if self.b.fringe_posting == FringePosting::ProductionLevel {
            children.push(self.level_fringes_step(li)?);
        }
        let label = match self.level_name(li) {
            Some(n) => Words::from(n),
            None => say!("engine.working.label.after_last_level"),
        };
        Ok(
            Step::sum(label, say!("engine.working.level_adds"), children)?
                .with_expand(Target::Level(li)),
        )
    }

    fn level(&self, li: usize) -> R<(Words, Step)> {
        let mut s = self.level_step(li)?;
        s.expand = None;
        let title = match self.level_name(li) {
            Some(n) => say!("engine.working.title.level_total", level = n),
            None => say!("engine.working.title.after_last_level_total"),
        };
        Ok((title, s))
    }

    fn level_fringes(&self, li: usize) -> R<(Words, Step)> {
        let mut s = self.level_fringes_step(li)?;
        s.expand = None;
        s.children.push(self.posting_note());
        let title = match self.level_name(li) {
            Some(n) => say!("engine.working.title.level_fringes", level = n),
            None => say!("engine.working.title.after_last_level_fringes"),
        };
        Ok((title, s))
    }

    /// How many levels a real production total closes (the first is
    /// above the line only when one does).
    fn closed_levels(&self) -> usize {
        self.b
            .topsheet
            .iter()
            .filter(|r| matches!(r, TopsheetRow::ProductionTotal(_)))
            .count()
    }

    /// The levels a production total closes, as indexes into the report's
    /// levels (the trailing unnamed level is left out).
    fn closed_range(&self) -> std::ops::Range<usize> {
        0..self.closed_levels().min(self.report.levels.len())
    }

    fn atl_step(&self) -> R<Step> {
        let closed = self.closed_range().end;
        let split = usize::from(closed > 0);
        let mut children = (0..split)
            .map(|li| self.level_step(li))
            .collect::<R<Vec<_>>>()?;
        if split == 0 {
            children.push(Step::note(
                say!("engine.working.label.no_production_totals"),
                say!("engine.working.no_production_totals"),
            ));
        }
        Ok(Step::sum(
            say!("engine.working.label.atl"),
            say!("engine.working.atl_is"),
            children,
        )?
        .with_expand(Target::Atl))
    }

    fn btl_step(&self) -> R<Step> {
        let closed = self.closed_range().end;
        let split = usize::from(closed > 0);
        let mut children = (split..closed)
            .map(|li| self.level_step(li))
            .collect::<R<Vec<_>>>()?;
        if closed < self.report.levels.len() {
            children.push(Step::note(
                say!("engine.working.label.not_counted_here"),
                say!("engine.working.after_last_level_neither"),
            ));
        }
        Ok(Step::sum(
            say!("engine.working.label.btl"),
            say!("engine.working.btl_is"),
            children,
        )?
        .with_expand(Target::Btl))
    }

    /// The trailing unnamed level, when categories follow the last
    /// production total (every category when there is none).
    fn trailing_steps(&self) -> R<Vec<Step>> {
        (self.closed_range().end..self.report.levels.len())
            .map(|li| self.level_step(li))
            .collect()
    }

    fn atl_btl_step(&self) -> R<Step> {
        Ok(Step::sum(
            say!("engine.working.label.atl_btl"),
            say!("engine.working.atl_btl_is"),
            vec![self.atl_step()?, self.btl_step()?],
        )?
        .with_expand(Target::AtlBtl))
    }

    fn atl(&self) -> R<(Words, Step)> {
        let mut s = self.atl_step()?;
        s.expand = None;
        Ok((say!("engine.working.label.atl"), s))
    }

    fn btl(&self) -> R<(Words, Step)> {
        let mut s = self.btl_step()?;
        s.expand = None;
        Ok((say!("engine.working.label.btl"), s))
    }

    fn atl_btl(&self) -> R<(Words, Step)> {
        let mut s = self.atl_btl_step()?;
        s.expand = None;
        Ok((say!("engine.working.label.atl_btl"), s))
    }

    fn budget_fringes_step(&self) -> R<Step> {
        let children = if self.b.fringe_posting == FringePosting::Budget {
            (0..self.accounts.len())
                .map(|ci| self.category_fringes_step(ci))
                .collect::<R<Vec<_>>>()?
        } else {
            Vec::new()
        };
        Ok(Step::sum(
            say!("engine.working.label.total_fringes"),
            say!("engine.working.total_fringes_is"),
            children,
        )?
        .with_expand(Target::BudgetFringes))
    }

    fn budget_fringes(&self) -> R<(Words, Step)> {
        let mut s = self.budget_fringes_step()?;
        s.expand = None;
        s.children.push(self.posting_note());
        Ok((say!("engine.working.label.total_fringes"), s))
    }

    /// Everything before charges: above and below-the-line, the categories
    /// after the last production total, and the budget's fringe row.
    fn before_charges_steps(&self) -> R<Vec<Step>> {
        let mut children = vec![self.atl_btl_step()?];
        children.extend(self.trailing_steps()?);
        if self.b.fringe_posting == FringePosting::Budget {
            children.push(self.budget_fringes_step()?);
        }
        Ok(children)
    }

    fn grand_total(&self) -> R<(Words, Step)> {
        let mut children = self.before_charges_steps()?;
        for i in 0..self.report.charges.len() {
            children.push(self.charge_step(i)?);
        }
        let s = Step::sum(
            say!("engine.working.label.grand_total"),
            say!("engine.working.grand_total_is"),
            children,
        )?;
        Ok((say!("engine.working.label.grand_total"), s))
    }

    fn credits_total_step(&self) -> R<Step> {
        let children = (0..self.report.credits.len())
            .map(|i| self.credit_step(i))
            .collect::<R<Vec<_>>>()?;
        Ok(Step::sum(
            say!("engine.working.label.credits"),
            say!("engine.working.every_applied_credit"),
            children,
        )?
        .with_expand(Target::CreditsTotal))
    }

    fn credits_total(&self) -> R<(Words, Step)> {
        let mut s = self.credits_total_step()?;
        s.expand = None;
        Ok((say!("engine.working.label.credits"), s))
    }

    fn net_total(&self) -> R<(Words, Step)> {
        let (_, mut grand) = self.grand_total()?;
        grand.expand = Some(Target::GrandTotal);
        let mut credits = self.credits_total_step()?;
        credits.minus = true;
        credits.relabel(say!("engine.working.label.less_credits"));
        let s = Step::sum(
            say!("engine.working.label.net_total"),
            say!("engine.working.net_total_is"),
            vec![grand, credits],
        )?;
        Ok((say!("engine.working.label.net_total"), s))
    }

    fn fringe(&self, name: &str) -> R<(Words, Step)> {
        let f = self
            .b
            .fringes
            .get(name)
            .ok_or_else(|| gone(&format!("fringe {name}")))?;
        let mut accounts = Vec::new();
        for (ci, cat) in self.b.categories().enumerate() {
            for (ai, _) in cat.accounts.iter().enumerate() {
                let lines: Vec<Step> = self.accounts[ci][ai]
                    .rows
                    .iter()
                    .flat_map(|r| r.fringes.iter())
                    .filter(|(n, _)| n == name)
                    .map(|(_, s)| s.clone())
                    .collect();
                if !lines.is_empty() {
                    accounts.push(
                        Step::sum(
                            self.account_name(ci, ai),
                            say!("engine.working.fringe_on_account_lines", fringe = name),
                            lines,
                        )?
                        .with_link(Link::Account { ci, ai }),
                    );
                }
            }
        }
        let kind = match &f.kind {
            FringeKind::Percent { cutoff: Some(c) } => {
                let factor = self.b.cutoff_factor;
                match factored_cutoff(self.b.arithmetic, Some(*c), factor) {
                    Some(Some(eff)) if factor != CutoffFactor::One => say!(
                        "engine.working.fringe_kind_percent_factor",
                        cutoff = money(eff),
                        typed = money(*c),
                        factor = factor.name()
                    ),
                    _ => say!(
                        "engine.working.fringe_kind_percent_cutoff",
                        cutoff = money(*c)
                    ),
                }
            }
            FringeKind::Percent { cutoff: None } => say!("engine.working.fringe_kind_percent"),
            FringeKind::Flat { .. } => say!("engine.working.fringe_kind_flat"),
        };
        let s = Step::sum(name, kind, accounts)?;
        Ok((say!("engine.working.title.fringe_total", fringe = name), s))
    }

    // -----------------------------------------------------------------------
    // Charges and credits

    /// Every costed line with where it is and its counted value.
    fn costed(&self) -> impl Iterator<Item = (usize, usize, &str, &str, &Detail, &RowCalc)> + '_ {
        self.b.categories().enumerate().flat_map(move |(ci, cat)| {
            cat.accounts.iter().enumerate().flat_map(move |(ai, acct)| {
                acct.details
                    .iter()
                    .zip(&self.accounts[ci][ai].rows)
                    .filter(|(d, _)| d.kind.is_costed())
                    .map(move |(d, r)| (ci, ai, cat.code.as_str(), acct.code.as_str(), d, r))
            })
        })
    }

    /// What a percent charge is a percent of: the grand total before
    /// charges.
    fn before_charges_total(&self) -> R<Cents> {
        let s = Step::sum("Before charges", "", self.before_charges_steps()?)?;
        s.amount.ok_or_else(ovf)
    }

    fn applied_charge(&self, i: usize) -> R<&crate::model::Charge> {
        self.b
            .charges
            .iter()
            .filter(|c| c.applied)
            .nth(i)
            .ok_or_else(|| gone("that charge"))
    }

    fn charge_step(&self, i: usize) -> R<Step> {
        let c = self.applied_charge(i)?;
        let dropped = dropped_from(self.b);
        let mut excluded: Cents = 0;
        let mut notes = Vec::new();
        // Excluded lines, gathered per account with the rule that caught them.
        let mut per_account: BTreeMap<(usize, usize), (Cents, BTreeSet<String>)> = BTreeMap::new();
        for (ci, ai, cat, acct, d, r) in self.costed() {
            let mut why = BTreeSet::new();
            for g in &d.groups {
                if c.excluded_groups.contains(g) {
                    why.insert(format!("group {g}"));
                }
            }
            if c.excluded_accounts.contains(acct) {
                why.insert(format!("account {acct}"));
            }
            if c.excluded_categories.contains(cat) {
                why.insert(format!("category {cat}"));
            }
            if why.is_empty() {
                continue;
            }
            let v = if dropped.is_some_and(|d| ci >= d) {
                r.total
            } else {
                r.total.checked_add(r.fringe_total).ok_or_else(ovf)?
            };
            let e = per_account.entry((ci, ai)).or_insert((0, BTreeSet::new()));
            e.0 = e.0.checked_add(v).ok_or_else(ovf)?;
            e.1.extend(why);
        }
        let mut ex_children = Vec::new();
        for (desc, amt) in &c.excluded_amounts {
            excluded = excluded.checked_add(*amt).ok_or_else(ovf)?;
            ex_children.push(Step::note_amount(
                say!("engine.working.label.excluded_amount"),
                if desc.trim().is_empty() {
                    say!("engine.working.typed_in")
                } else {
                    Words::from(desc.trim())
                },
                *amt,
            ));
        }
        for ((ci, ai), (amt, why)) in &per_account {
            excluded = excluded.checked_add(*amt).ok_or_else(ovf)?;
            let what = if dropped.is_some_and(|d| *ci >= d) {
                "its lines (their fringes are left out of every total)"
            } else {
                "its lines and their fringes"
            };
            ex_children.push(
                Step::note_amount(
                    self.account_name(*ci, *ai),
                    format!(
                        "{what}, left out by {} (each line counted once)",
                        why.iter().cloned().collect::<Vec<_>>().join(", ")
                    ),
                    *amt,
                )
                .with_link(Link::Account { ci: *ci, ai: *ai }),
            );
        }
        let (amount, label_text) = match &c.kind {
            ChargeKind::Percent(p) => {
                let base = self.before_charges_total()?;
                notes.push(Step::note_amount(
                    say!("engine.working.label.base"),
                    say!("engine.working.charge_base"),
                    base,
                ));
                notes.push(
                    Step::note_amount(
                        say!("engine.working.label.excluded"),
                        say!("engine.working.taken_off_base"),
                        excluded,
                    )
                    .with(ex_children),
                );
                let net = base.checked_sub(excluded).ok_or_else(ovf)?;
                let arith = self.b.arithmetic;
                let exact = percent_of(arith, net, Num::of(arith, *p)).ok_or_else(ovf)?;
                let amount = exact.to_cents().ok_or_else(ovf)?;
                notes.push(Step::note(
                    say!("engine.working.label.charge"),
                    say!(
                        "engine.working.charge_percent_of",
                        percent = num(*p),
                        base = money(base),
                        excluded = money(excluded),
                        exact = dmoney(exact)
                    ),
                ));
                notes.extend(rounding(exact, amount));
                (
                    amount,
                    say!("engine.working.percent_of_budget", percent = num(*p)),
                )
            }
            ChargeKind::Flat { amount, currency } => {
                let base = match currency {
                    Some(cur) if *cur != self.b.base_currency => {
                        let rate = self
                            .b
                            .currencies
                            .get(cur)
                            .ok_or_else(|| gone(&format!("currency {cur}")))?
                            .rate;
                        let arith = self.b.arithmetic;
                        let exact = Money::cents(arith, *amount)
                            .checked_mul(Num::of(arith, rate))
                            .ok_or_else(ovf)?;
                        let conv = exact.to_cents().ok_or_else(ovf)?;
                        notes.push(Step::note(
                            say!("engine.working.label.fee"),
                            say!(
                                "engine.working.fee_converted",
                                amount = money(*amount),
                                currency = cur,
                                rate = num(rate),
                                base = self.b.base_currency,
                                exact = dmoney(exact)
                            ),
                        ));
                        notes.extend(rounding(exact, conv));
                        conv
                    }
                    _ => {
                        notes.push(Step::note_amount(
                            say!("engine.working.label.fee"),
                            say!("engine.working.a_flat_fee"),
                            *amount,
                        ));
                        *amount
                    }
                };
                notes.push(
                    Step::note_amount(
                        say!("engine.working.label.excluded"),
                        say!("engine.working.taken_off_fee"),
                        excluded,
                    )
                    .with(ex_children),
                );
                let amount = base.checked_sub(excluded).ok_or_else(ovf)?;
                notes.push(Step::note(
                    say!("engine.working.label.charge"),
                    format!("{} - {} = {}", money(base), money(excluded), money(amount)),
                ));
                (amount, say!("engine.working.a_flat_fee"))
            }
        };
        Ok(Step::value(&c.name, label_text, amount)
            .with(notes)
            .with_expand(Target::Charge(i)))
    }

    fn charge(&self, i: usize) -> R<(Words, Step)> {
        let c = self.applied_charge(i)?;
        let mut s = self.charge_step(i)?;
        s.expand = None;
        Ok((say!("engine.working.title.charge", name = c.name), s))
    }

    fn applied_credit(&self, i: usize) -> R<&crate::model::Credit> {
        self.b
            .credits
            .iter()
            .filter(|c| c.applied)
            .nth(i)
            .ok_or_else(|| gone("that credit"))
    }

    fn credit_step(&self, i: usize) -> R<Step> {
        let c = self.applied_credit(i)?;
        let arith = self.b.arithmetic;
        let mut notes = Vec::new();
        let excluded_fringes = &c.excluded_fringes;
        // Only lines at the credit's locations count (decisions.md, entry 80).
        let here = |d: &Detail| counts_at(&c.locations, d.location.as_deref());
        let mut total = Money::cents(arith, 0);
        if !c.locations.is_empty() {
            let names = c.locations.iter().cloned().collect::<Vec<_>>();
            notes.push(Step::note(
                say!("engine.working.label.locations"),
                say!("engine.working.only_lines_at", locations = names.join(", ")),
            ));
        }
        for (k, cr) in c.rows.iter().enumerate() {
            let mut row_notes = Vec::new();
            let (gross, excluded) = match &cr.source {
                CreditSource::Flat(g) => {
                    row_notes.push(Step::note_amount(
                        say!("engine.working.label.gross"),
                        say!("engine.working.typed_in"),
                        *g,
                    ));
                    (*g, 0)
                }
                CreditSource::Group { group } => {
                    let mut gross: Cents = 0;
                    let mut ex: Cents = 0;
                    let mut per_account: BTreeMap<(usize, usize), (Cents, Cents)> = BTreeMap::new();
                    for (ci, ai, _, _, d, r) in self.costed() {
                        if !r.included || !d.groups.contains(group) || !here(d) {
                            continue;
                        }
                        let v = r.total.checked_add(r.fringe_total).ok_or_else(ovf)?;
                        gross = gross.checked_add(v).ok_or_else(ovf)?;
                        let mut e: Cents = 0;
                        for (name, s) in &r.fringes {
                            if excluded_fringes.contains(name) {
                                e = e.checked_add(s.amount.unwrap_or(0)).ok_or_else(ovf)?;
                            }
                        }
                        ex = ex.checked_add(e).ok_or_else(ovf)?;
                        let slot = per_account.entry((ci, ai)).or_insert((0, 0));
                        slot.0 = slot.0.checked_add(v).ok_or_else(ovf)?;
                        slot.1 = slot.1.checked_add(e).ok_or_else(ovf)?;
                    }
                    let lines = per_account
                        .iter()
                        .map(|(&(ci, ai), &(v, _))| {
                            Step::note_amount(
                                self.account_name(ci, ai),
                                say!("engine.working.counted_lines_tagged", group = group),
                                v,
                            )
                            .with_link(Link::Account { ci, ai })
                        })
                        .collect();
                    row_notes.push(
                        Step::note_amount(
                            say!("engine.working.label.gross"),
                            say!("engine.working.lines_tagged", group = group),
                            gross,
                        )
                        .with(lines),
                    );
                    if !excluded_fringes.is_empty() {
                        let names = excluded_fringes.iter().cloned().collect::<Vec<_>>();
                        row_notes.push(Step::note_amount(
                            say!("engine.working.label.excluded_fringes"),
                            names.join(", "),
                            ex,
                        ));
                    }
                    (gross, ex)
                }
                CreditSource::Category { category } => {
                    let mut gross: Cents = 0;
                    let mut ex: Cents = 0;
                    let mut found = None;
                    for (ci, _, cat, _, d, r) in self.costed() {
                        if cat != category {
                            continue;
                        }
                        found = Some(ci);
                        if !here(d) {
                            continue;
                        }
                        let v = r.total.checked_add(r.fringe_total).ok_or_else(ovf)?;
                        gross = gross.checked_add(v).ok_or_else(ovf)?;
                        for (name, s) in &r.fringes {
                            if excluded_fringes.contains(name) {
                                ex = ex.checked_add(s.amount.unwrap_or(0)).ok_or_else(ovf)?;
                            }
                        }
                    }
                    if found.is_none() {
                        found = self.b.categories().position(|c| c.code == *category);
                    }
                    let gross_note = match found {
                        Some(ci) => Step::note_amount(
                            say!("engine.working.label.gross"),
                            say!(
                                "engine.working.gross_category",
                                category = self.category_name(ci)
                            ),
                            gross,
                        )
                        .with_link(Link::Category { ci }),
                        None => Step::note_amount(
                            say!("engine.working.label.gross"),
                            say!("engine.working.gross_no_category", category = category),
                            gross,
                        ),
                    };
                    row_notes.push(gross_note);
                    if !excluded_fringes.is_empty() {
                        let names = excluded_fringes.iter().cloned().collect::<Vec<_>>();
                        row_notes.push(Step::note_amount(
                            say!("engine.working.label.excluded_fringes"),
                            names.join(", "),
                            ex,
                        ));
                    }
                    (gross, ex)
                }
                CreditSource::Account { account } => {
                    // The first account with the number, as the engine
                    // takes it.
                    let found = self.b.categories().enumerate().find_map(|(ci, cat)| {
                        cat.accounts
                            .iter()
                            .position(|a| a.code == *account)
                            .map(|ai| (ci, ai))
                    });
                    let mut gross: Cents = 0;
                    let mut ex: Cents = 0;
                    if let Some(at) = found {
                        for (ci, ai, _, _, d, r) in self.costed() {
                            if (ci, ai) != at || !here(d) {
                                continue;
                            }
                            let v = r.total.checked_add(r.fringe_total).ok_or_else(ovf)?;
                            gross = gross.checked_add(v).ok_or_else(ovf)?;
                            for (name, s) in &r.fringes {
                                if excluded_fringes.contains(name) {
                                    ex = ex.checked_add(s.amount.unwrap_or(0)).ok_or_else(ovf)?;
                                }
                            }
                        }
                    }
                    let gross_note = match found {
                        Some((ci, ai)) => Step::note_amount(
                            say!("engine.working.label.gross"),
                            say!(
                                "engine.working.gross_account",
                                account = self.account_name(ci, ai)
                            ),
                            gross,
                        )
                        .with_link(Link::Account { ci, ai }),
                        None => Step::note_amount(
                            say!("engine.working.label.gross"),
                            say!("engine.working.gross_no_account", account = account),
                            gross,
                        ),
                    };
                    row_notes.push(gross_note);
                    if !excluded_fringes.is_empty() {
                        let names = excluded_fringes.iter().cloned().collect::<Vec<_>>();
                        row_notes.push(Step::note_amount(
                            say!("engine.working.label.excluded_fringes"),
                            names.join(", "),
                            ex,
                        ));
                    }
                    (gross, ex)
                }
            };
            let net = gross.checked_sub(excluded).ok_or_else(ovf)?;
            row_notes.push(Step::note_amount(
                say!("engine.working.label.net_spend"),
                format!("{} - {}", money(gross), money(excluded)),
                net,
            ));
            // The engine's own unrounded working, so the amounts agree.
            let (credit, _) = credit_row(arith, net, cr.percent, cr.cap).ok_or_else(ovf)?;
            let pct = |v: Money| {
                v.checked_mul(Num::of(arith, cr.percent))
                    .map(Money::percent)
            };
            let net_x = Money::cents(arith, net);
            match cr.cap {
                CreditCap::None => {
                    row_notes.push(Step::note(
                        say!("engine.working.label.credit"),
                        say!(
                            "engine.working.percent_of",
                            percent = num(cr.percent),
                            base = money(net),
                            exact = dmoney(credit)
                        ),
                    ));
                }
                CreditCap::QualifiedSpend(cap) => {
                    let spend = net_x
                        .checked_min(Money::cents(arith, cap))
                        .ok_or_else(ovf)?;
                    row_notes.push(Step::note(
                        say!("engine.working.label.cap_on_spend"),
                        say!(
                            "engine.working.cap_on_spend",
                            cap = money(cap),
                            spend = dmoney(spend)
                        ),
                    ));
                    row_notes.push(Step::note(
                        say!("engine.working.label.credit"),
                        say!(
                            "engine.working.percent_of",
                            percent = num(cr.percent),
                            base = dmoney(spend),
                            exact = dmoney(credit)
                        ),
                    ));
                }
                CreditCap::Credit(cap) => {
                    let uncapped = pct(net_x).ok_or_else(ovf)?;
                    row_notes.push(Step::note(
                        say!("engine.working.label.credit"),
                        say!(
                            "engine.working.percent_of_capped",
                            percent = num(cr.percent),
                            base = money(net),
                            exact = dmoney(uncapped),
                            cap = money(cap),
                            credit = dmoney(credit)
                        ),
                    ));
                }
            }
            row_notes.push(Step::note(
                say!("engine.working.label.not_rounded_yet"),
                say!("engine.working.rounded_once"),
            ));
            total = total.checked_add(credit).ok_or_else(ovf)?;
            let label = if cr.description.trim().is_empty() {
                say!("engine.working.label.row_n", row = k + 1)
            } else {
                Words::from(cr.description.trim())
            };
            notes.push(Step::note(label, dmoney(credit)).with(row_notes));
        }
        notes.push(Step::note(
            say!("engine.working.label.rows_added"),
            dmoney(total),
        ));
        // The engine's own figures for what comes after the rows
        // (decisions.md, entry 88): the minimum, the shared cap and the fee.
        let reported = self
            .report
            .credits
            .get(i)
            .ok_or_else(|| gone("that credit"))?;
        let mut capped = credit_cap(arith, total, c.cap).ok_or_else(ovf)?;
        if let Some(min) = c.minimum_spend {
            let mut qualified: Cents = 0;
            for r in &reported.rows {
                qualified = qualified.checked_add(r.net).ok_or_else(ovf)?;
            }
            if reported.below_minimum {
                notes.push(Step::note(
                    say!("engine.working.label.minimum_spend"),
                    say!(
                        "engine.working.below_minimum",
                        spend = money(qualified),
                        minimum = money(min)
                    ),
                ));
                capped = Money::cents(arith, 0);
            } else {
                notes.push(Step::note(
                    say!("engine.working.label.minimum_spend"),
                    say!(
                        "engine.working.at_least_minimum",
                        spend = money(qualified),
                        minimum = money(min)
                    ),
                ));
            }
        }
        if let Some(cap) = c.cap.filter(|_| !reported.below_minimum) {
            notes.push(Step::note(
                say!("engine.working.label.credit_cap"),
                say!(
                    "engine.working.at_most",
                    cap = money(cap),
                    capped = dmoney(capped)
                ),
            ));
        }
        if let Some(name) = c.shared_cap.as_deref() {
            let amount = self.b.shared_caps.get(name);
            match (amount, reported.shared) {
                (Some(cap), Some(share)) => {
                    let others: Vec<&str> = self
                        .b
                        .credits
                        .iter()
                        .filter(|o| o.applied && o.name != c.name)
                        .filter(|o| o.shared_cap.as_deref() == Some(name))
                        .map(|o| o.name.as_str())
                        .collect();
                    notes.push(Step::note(
                        say!("engine.working.label.shared_cap"),
                        say!(
                            "engine.working.shared_cap_share",
                            name = quoted(name),
                            others = others.join(", "),
                            cap = money(*cap),
                            capped = dmoney(capped),
                            share = money(share)
                        ),
                    ));
                    capped = Money::cents(arith, share);
                }
                (Some(cap), None) => notes.push(Step::note(
                    say!("engine.working.label.shared_cap"),
                    say!(
                        "engine.working.shared_cap_within",
                        name = quoted(name),
                        cap = money(*cap)
                    ),
                )),
                (None, _) => notes.push(Step::note(
                    say!("engine.working.label.shared_cap"),
                    say!("engine.working.shared_cap_missing", name = quoted(name)),
                )),
            }
        }
        let keep = credit_keep(arith, c.discount_percent).ok_or_else(ovf)?;
        let exact = capped.checked_mul_money(keep).ok_or_else(ovf)?;
        if !c.discount_percent.is_zero() {
            notes.push(Step::note(
                say!("engine.working.label.discount"),
                say!(
                    "engine.working.discount",
                    percent = num(c.discount_percent),
                    capped = dmoney(capped),
                    keep = keep,
                    exact = dmoney(exact)
                ),
            ));
        }
        let rounded = exact.to_cents().ok_or_else(ovf)?;
        notes.extend(rounding(exact, rounded));
        let amount = rounded.checked_sub(reported.fee).ok_or_else(ovf)?;
        if c.broker_fee != 0 {
            let text = if reported.fee == c.broker_fee {
                say!(
                    "engine.working.fee_less",
                    fee = money(c.broker_fee),
                    amount = money(amount)
                )
            } else {
                say!(
                    "engine.working.fee_less_floor",
                    fee = money(c.broker_fee),
                    taken = money(reported.fee),
                    amount = money(amount)
                )
            };
            notes.push(Step::note(say!("engine.working.label.brokers_fee"), text));
        }
        Ok(
            Step::value(&c.name, say!("engine.working.applied_credit"), amount)
                .with(notes)
                .with_expand(Target::Credit(i)),
        )
    }

    fn credit(&self, i: usize) -> R<(Words, Step)> {
        let c = self.applied_credit(i)?;
        let mut s = self.credit_step(i)?;
        s.expand = None;
        Ok((say!("engine.working.title.credit", name = c.name), s))
    }
}

/// "Key grip" or "Line 3" when it has no description.
fn line_label(i: usize, d: &Detail) -> Words {
    let t = d.description.trim();
    if t.is_empty() {
        say!("engine.working.label.line_n", line = i + 1)
    } else {
        Words::from(t)
    }
}

/// What of a row an explanation's title names.
enum RowPart<'a> {
    Total,
    Fringes,
    Fringe(&'a str),
}

/// "2100 line 3 (Key grip): total", "2100 line 3: FICA".
fn row_title(code: &str, i: usize, d: &Detail, part: RowPart<'_>) -> Words {
    let t = d.description.trim();
    let line = i + 1;
    match (t.is_empty(), part) {
        (true, RowPart::Total) => {
            say!("engine.working.title.line_total", code = code, line = line)
        }
        (false, RowPart::Total) => say!(
            "engine.working.title.line_described_total",
            code = code,
            line = line,
            description = t
        ),
        (true, RowPart::Fringes) => {
            say!(
                "engine.working.title.line_fringes",
                code = code,
                line = line
            )
        }
        (false, RowPart::Fringes) => say!(
            "engine.working.title.line_described_fringes",
            code = code,
            line = line,
            description = t
        ),
        (true, RowPart::Fringe(f)) => say!(
            "engine.working.title.line_fringe",
            code = code,
            line = line,
            fringe = f
        ),
        (false, RowPart::Fringe(f)) => say!(
            "engine.working.title.line_described_fringe",
            code = code,
            line = line,
            description = t,
            fringe = f
        ),
    }
}

// ---------------------------------------------------------------------------
// Working out one account

/// A unit name with its plural when the quantity isn't one.
fn unit_words(b: &Budget, unit: &str, qty: Dec) -> String {
    let u = unit.trim();
    if u.is_empty() {
        return String::new();
    }
    match b.units.get(u) {
        Some(def) if qty != Dec::ONE && !def.plural.trim().is_empty() => {
            format!(" {}", def.plural.trim())
        }
        _ => format!(" {u}"),
    }
}

/// A number as the working shows it: a decimal, or a double with the
/// shortest digits that give it back (`0.3333333333333333`).
fn nwords(n: Num) -> String {
    match n {
        Num::Dec(d) => num(d),
        Num::F64(v) => {
            // Grouped like a decimal.
            let t = float_text(v);
            let (sign, t) = match t.strip_prefix('-') {
                Some(r) => ("-", r),
                None => ("", t.as_str()),
            };
            match t.split_once('.') {
                Some((i, f)) => format!("{sign}{}.{f}", group(i)),
                None => format!("{sign}{}", group(t)),
            }
        }
    }
}

/// A formula and its value: `40` or `SHOOT_DAYS + PREP_DAYS = 40
/// (SHOOT_DAYS = 30, PREP_DAYS = 10)`.
fn formula_words(ctx: &Ctx, f: &Formula, value: Num) -> String {
    let src = f.0.trim();
    if let Ok(v) = src.parse::<Dec>() {
        if ctx.num(v) == value {
            return nwords(value);
        }
    }
    // A number with more places than a decimal keeps (Movie Magic only).
    let plain = !src.is_empty() && src.bytes().all(|c| c.is_ascii_digit() || c == b'.');
    if plain && matches!(value, Num::F64(v) if src.parse::<f64>() == Ok(v)) {
        return src.to_string();
    }
    let mut vars = BTreeSet::new();
    if let Ok(e) = Expr::parse(src) {
        e.vars(&mut vars);
    }
    let globals: Vec<String> = vars
        .iter()
        .filter_map(|n| ctx.globals.get(n).map(|v| format!("{n} = {}", nwords(*v))))
        .collect();
    if globals.is_empty() {
        format!("{src} = {}", nwords(value))
    } else if globals.len() == 1 && vars.len() == 1 && vars.contains(src) {
        format!("global {}", globals[0])
    } else {
        format!("{src} = {} ({})", nwords(value), globals.join(", "))
    }
}

/// Why a line is left out, in words, and what it would be counted
/// (`total`): "...; counted, it would be 1,200.00".
fn left_out(ctx: &Ctx, d: &Detail, total: Cents) -> Words {
    let total = money(total);
    let rules = ctx.b.group_rules;
    let counted = d
        .groups
        .iter()
        .map(|g| ctx.b.groups.get(g).is_none_or(|x| x.in_budget_total));
    match rules.counts(counted) {
        Ok(()) => {}
        Err(LeftOut::Ungrouped) => {
            return if d.kind == RowKind::Adjustment {
                say!(
                    "engine.working.left_out_adjustment_ungrouped",
                    total = total
                )
            } else {
                say!("engine.working.left_out_ungrouped", total = total)
            };
        }
        Err(LeftOut::GroupOff) => {
            let off: Vec<&str> = d
                .groups
                .iter()
                .filter(|g| ctx.b.groups.get(*g).is_some_and(|x| !x.in_budget_total))
                .map(String::as_str)
                .collect();
            let groups = off.join(", ");
            let conflict = off.len() < d.groups.len();
            return match (off.len() == 1, conflict) {
                (true, false) => say!(
                    "engine.working.left_out_group_off",
                    groups = groups,
                    total = total
                ),
                (false, false) => say!(
                    "engine.working.left_out_groups_off",
                    groups = groups,
                    total = total
                ),
                (true, true) => say!(
                    "engine.working.left_out_group_off_conflict",
                    groups = groups,
                    total = total
                ),
                (false, true) => say!(
                    "engine.working.left_out_groups_off_conflict",
                    groups = groups,
                    total = total
                ),
            };
        }
    }
    let ex: Vec<&str> = d
        .groups
        .iter()
        .filter(|g| ctx.opts.exclude_groups.contains(*g))
        .map(String::as_str)
        .collect();
    if !ex.is_empty() {
        return say!(
            "engine.working.left_out_by_report",
            groups = ex.join(", "),
            total = total
        );
    }
    say!("engine.working.left_out_not_shown", total = total)
}

fn account_calc(ctx: &Ctx, ci: usize, ai: usize, code: &str, details: &[Detail]) -> R<AccountCalc> {
    let base = &ctx.b.base_currency;
    let mut rows: Vec<RowCalc> = Vec::with_capacity(details.len());
    let mut since_adjustment = Window::default();
    let mut adj_rows: Vec<usize> = Vec::new();
    let mut sub_rows: Vec<usize> = Vec::new();
    let zero = Factors {
        amount: ctx.num(Dec::ZERO),
        x: ctx.num(Dec::ONE),
        x4: ctx.num(Dec::ONE),
        quantity: ctx.num(Dec::ZERO),
    };
    let mut factors: Vec<Factors> = Vec::with_capacity(details.len());
    let blank_at = || String::new();

    for (i, d) in details.iter().enumerate() {
        let link = Link::Row { ci, ai, row: d.id };
        match d.kind {
            RowKind::Subtotal => {
                rows.push(RowCalc {
                    kind: d.kind,
                    included: true,
                    total: 0,
                    value: None,
                    adds: std::mem::take(&mut sub_rows),
                    fringes: Vec::new(),
                    fringe_total: 0,
                });
                factors.push(zero);
                continue;
            }
            RowKind::Separator => {
                rows.push(RowCalc {
                    kind: d.kind,
                    included: true,
                    total: 0,
                    value: None,
                    adds: Vec::new(),
                    fringes: Vec::new(),
                    fringe_total: 0,
                });
                factors.push(zero);
                continue;
            }
            RowKind::Line | RowKind::Adjustment => {}
        }

        let amount = ctx.eval(&d.amount, Dec::ZERO, &blank_at)?;
        let x = ctx.eval(&d.x, Dec::ONE, &blank_at)?;
        let x4 = ctx.eval(&d.x4, Dec::ONE, &blank_at)?;
        let quantity = Factors::quantity(amount, x, x4).ok_or_else(ovf)?;
        let shown = |n: Num| n.shown().ok_or_else(ovf);
        let included = ctx.includes(d, &blank_at)?;
        let mut notes = Vec::new();

        // Amount, X and 4X.
        let amount_text = if d.amount.is_blank() {
            say!("engine.working.blank_counts_as_0")
        } else {
            Words::from(format!(
                "{}{}",
                formula_words(ctx, &d.amount, amount),
                unit_words(ctx.b, &d.unit, shown(amount)?)
            ))
        };
        notes.push(Step::note(say!("engine.working.label.amount"), amount_text));
        if !d.x.is_blank() {
            notes.push(Step::note(
                say!("engine.working.label.x"),
                format!(
                    "{}{}",
                    formula_words(ctx, &d.x, x),
                    unit_words(ctx.b, &d.unit2, shown(x)?)
                ),
            ));
        }
        if !d.x4.is_blank() {
            notes.push(Step::note(
                say!("engine.working.label.x4"),
                format!(
                    "{}{}",
                    formula_words(ctx, &d.x4, x4),
                    unit_words(ctx.b, &d.unit4, shown(x4)?)
                ),
            ));
        }
        if !d.x.is_blank() || !d.x4.is_blank() {
            let mut parts = vec![nwords(amount)];
            if !d.x.is_blank() {
                parts.push(nwords(x));
            }
            if !d.x4.is_blank() {
                parts.push(nwords(x4));
            }
            notes.push(Step::note(
                say!("engine.working.label.quantity"),
                format!("{} = {}", parts.join(" x "), nwords(quantity)),
            ));
        }

        let total = match d.kind {
            RowKind::Adjustment => {
                let exact = since_adjustment
                    .money(ctx.arith)
                    .checked_mul(quantity)
                    .ok_or_else(ovf)?;
                let t = exact.to_cents().ok_or_else(ovf)?;
                let pct = shown(quantity)?
                    .checked_mul(Dec::from_int(100))
                    .ok_or_else(ovf)?;
                let window: Vec<Step> = adj_rows
                    .iter()
                    .map(|&k| {
                        let row = details[k].id;
                        Step::note_amount(line_label(k, &details[k]), "", rows[k].total)
                            .with_link(Link::Row { ci, ai, row })
                    })
                    .collect();
                let base_text = match ctx.arith {
                    Arithmetic::Exact => say!("engine.working.adjustment_base"),
                    Arithmetic::Mmb => say!("engine.working.adjustment_base_doubles"),
                };
                notes.push(
                    Step::note_amount(
                        say!("engine.working.label.base"),
                        base_text,
                        since_adjustment.cents,
                    )
                    .with(window),
                );
                let base_words = match ctx.arith {
                    Arithmetic::Exact => money(since_adjustment.cents),
                    Arithmetic::Mmb => dmoney(since_adjustment.money(ctx.arith)),
                };
                notes.push(Step::note(
                    say!("engine.working.label.adjustment"),
                    format!(
                        "{base_words} x {} ({}%) = {}",
                        nwords(quantity),
                        num(pct),
                        dmoney(exact)
                    ),
                ));
                notes.extend(rounding(exact, t));
                t
            }
            _ => {
                let rate = ctx.eval(&d.rate, Dec::ZERO, &blank_at)?;
                let rate_text = if d.rate.is_blank() {
                    say!("engine.working.blank_counts_as_0")
                } else {
                    Words::from(format!(
                        "{}{}",
                        formula_words(ctx, &d.rate, rate),
                        unit_words(ctx.b, &d.unit3, Dec::ONE)
                    ))
                };
                notes.push(Step::note(say!("engine.working.label.rate"), rate_text));
                // Where the rate came from, when it was filled from a rate
                // table (decisions.md, entry 90): words only.
                if let Some(words) = crate::rates::source_words(ctx.b, d) {
                    notes.push(Step::note(say!("engine.working.label.rate_source"), words));
                }
                let line = Money::num(quantity).checked_mul(rate).ok_or_else(ovf)?;
                let local = line.to_cents().ok_or_else(ovf)?;
                let cur_code = match &d.currency {
                    Some(c) if c != base => Some(c),
                    _ => None,
                };
                let cur_words = cur_code.map(|c| format!(" {c}")).unwrap_or_default();
                notes.push(Step::note(
                    say!("engine.working.label.line"),
                    format!(
                        "{} x {} = {}{cur_words}",
                        nwords(quantity),
                        nwords(rate),
                        dmoney(line)
                    ),
                ));
                // Movie Magic converts the unrounded amount: the line rounds
                // once, in the base currency.
                let rounds_here = cur_code.is_none() || ctx.arith == Arithmetic::Exact;
                if rounds_here {
                    notes.extend(rounding(line, local));
                }
                match cur_code {
                    Some(c) => {
                        let cur = ctx
                            .b
                            .currencies
                            .get(c)
                            .ok_or_else(|| gone(&format!("currency {c}")))?;
                        let exact = converted(ctx.arith, line, local, cur.rate).ok_or_else(ovf)?;
                        let t = exact.to_cents().ok_or_else(ovf)?;
                        let text = if rounds_here || line.is_cents(local) {
                            say!(
                                "engine.working.currency",
                                amount = money(local),
                                currency = c,
                                rate = num(cur.rate),
                                base = base,
                                exact = dmoney(exact)
                            )
                        } else {
                            say!(
                                "engine.working.currency_unrounded",
                                amount = dmoney(line),
                                currency = c,
                                rate = num(cur.rate),
                                base = base,
                                exact = dmoney(exact)
                            )
                        };
                        notes.push(Step::note(say!("engine.working.label.currency"), text));
                        notes.extend(rounding(exact, t));
                        t
                    }
                    None => local,
                }
            }
        };
        let total_counted = if included { total } else { 0 };
        if !included {
            notes.push(Step::note_amount(
                say!("engine.working.label.left_out"),
                left_out(ctx, d, total),
                0,
            ));
        }
        if d.kind == RowKind::Adjustment && included {
            since_adjustment.clear();
            adj_rows.clear();
        } else {
            since_adjustment.add(total_counted).ok_or_else(ovf)?;
            adj_rows.push(i);
        }
        sub_rows.push(i);

        let text = if d.kind == RowKind::Adjustment {
            say!("engine.working.row_adjustment", code = code, line = i + 1)
        } else if included {
            say!("engine.working.row_line", code = code, line = i + 1)
        } else {
            say!(
                "engine.working.row_line_left_out",
                code = code,
                line = i + 1
            )
        };
        let value = Step::value(line_label(i, d), text, total_counted)
            .with(notes)
            .with_link(link)
            .with_expand(Target::Line(d.id));
        factors.push(Factors {
            amount,
            x,
            x4,
            quantity,
        });
        rows.push(RowCalc {
            kind: d.kind,
            included,
            total: total_counted,
            value: Some(value),
            adds: Vec::new(),
            fringes: Vec::new(),
            fringe_total: 0,
        });
    }

    fringes_calc(ctx, ci, ai, details, &mut rows, &factors)?;

    let fringe_total = rows
        .iter()
        .try_fold(0, |a: Cents, r| a.checked_add(r.fringe_total))
        .ok_or_else(ovf)?;
    Ok(AccountCalc { rows, fringe_total })
}

/// Fringe value steps for every line of one account (the engine's stage 2,
/// worked out again with its working shown).
fn fringes_calc(
    ctx: &Ctx,
    ci: usize,
    ai: usize,
    details: &[Detail],
    rows: &mut [RowCalc],
    factors: &[Factors],
) -> R<()> {
    let blank_at = || String::new();
    // Rates, once per fringe the lines carry.
    let mut rates: BTreeMap<&str, Num> = BTreeMap::new();
    for d in details {
        if !d.kind.is_costed() {
            continue;
        }
        for name in &d.fringes {
            if rates.contains_key(name.as_str()) {
                continue;
            }
            let f = ctx
                .b
                .fringes
                .get(name)
                .ok_or_else(|| gone(&format!("fringe {name}")))?;
            rates.insert(name.as_str(), ctx.eval(&f.rate, Dec::ZERO, &blank_at)?);
        }
    }
    // A fringe's rate when it is a formula: " (rate SAG_PCT = 18.5)".
    let rate_formula = |name: &str, rate: Num| -> Option<String> {
        let f = &ctx.b.fringes[name];
        let src = f.rate.0.trim();
        if src.parse::<Dec>().is_ok() {
            None
        } else {
            Some(formula_words(ctx, &f.rate, rate))
        }
    };
    let earns: Vec<bool> = details
        .iter()
        .zip(rows.iter())
        .map(|(d, r)| d.kind.is_costed() && r.included)
        .collect();
    // The engine's earners, from a row list that has what it reads.
    let shadow: Vec<RowTotals> = rows
        .iter()
        .map(|r| RowTotals {
            id: 0,
            kind: r.kind,
            included: r.included,
            quantity: Dec::ZERO,
            currency: String::new(),
            local_total: 0,
            total: r.total,
            subtotal: None,
            fringes: BTreeMap::new(),
            fringe_total: 0,
            fringe_full_total: 0,
        })
        .collect();
    let mut found: Vec<BTreeMap<String, Step>> = vec![BTreeMap::new(); details.len()];

    for earner in earners(details, &shadow) {
        let mut names: BTreeSet<&str> = BTreeSet::new();
        for &i in &earner {
            if earns[i] {
                names.extend(details[i].fringes.iter().map(String::as_str));
            }
        }
        // Every row of an earner from a range shares its number.
        let range = details[earner[0]].fringe_range;
        for name in names {
            let f = &ctx.b.fringes[name];
            let FringeKind::Percent { cutoff: typed } = f.kind else {
                continue;
            };
            // Times the budget's cutoff factor, as the engine applies it.
            let cutoff = factored_cutoff(ctx.arith, typed, ctx.b.cutoff_factor).ok_or_else(ovf)?;
            let rate = rates[name];
            let lines: Vec<usize> = earner
                .iter()
                .copied()
                .filter(|&i| earns[i] && details[i].fringes.contains(name))
                .collect();
            // The cutoff, used up in row order, as the engine uses it.
            let mut left = cutoff;
            for (k, &i) in lines.iter().enumerate() {
                let total = rows[i].total;
                let before = left;
                let (taken, exact) =
                    percent_step(ctx.arith, rate, &mut left, total).ok_or_else(ovf)?;
                let amount = exact.to_cents().ok_or_else(ovf)?;
                let mut notes = Vec::new();
                if let Some(n) = range {
                    let others: Vec<String> = lines.iter().map(|&j| (j + 1).to_string()).collect();
                    notes.push(Step::note(
                        say!("engine.working.label.fringe_range"),
                        say!(
                            "engine.working.fringe_range",
                            range = n,
                            lines = others.join(", ")
                        ),
                    ));
                }
                notes.push(Step::note_amount(
                    say!("engine.working.label.earnings"),
                    say!("engine.working.this_lines_total"),
                    total,
                ));
                if let (Some(t), Some(c)) = (typed, cutoff) {
                    if ctx.b.cutoff_factor != CutoffFactor::One {
                        notes.push(cutoff_factor_note(ctx.b.cutoff_factor, t, c));
                    }
                }
                if let (Some(c), Some(l)) = (cutoff, before) {
                    let cutoff = money(c);
                    let left = money(l.max(0));
                    if l <= 0 && k > 0 {
                        notes.push(Step::note_amount(
                            say!("engine.working.label.cutoff_used_up"),
                            say!(
                                "engine.working.cutoff_used_by_lines_above",
                                fringe = name,
                                cutoff = cutoff
                            ),
                            0,
                        ));
                    } else if total > l || l <= 0 {
                        let text = if k == 0 {
                            say!(
                                "engine.working.stops_per_earner",
                                fringe = name,
                                cutoff = cutoff
                            )
                        } else {
                            say!(
                                "engine.working.stops_per_earner_left",
                                fringe = name,
                                cutoff = cutoff,
                                left = left
                            )
                        };
                        notes.push(Step::note_amount(
                            say!("engine.working.label.cutoff_reached"),
                            text,
                            taken,
                        ));
                    } else {
                        let text = if k == 0 {
                            say!("engine.working.per_earner_not_reached", cutoff = cutoff)
                        } else {
                            say!(
                                "engine.working.per_earner_left_not_reached",
                                cutoff = cutoff,
                                left = left
                            )
                        };
                        notes.push(Step::note(say!("engine.working.label.cutoff"), text));
                    }
                }
                notes.push(Step::note(
                    say!("engine.working.label.fringe"),
                    percent_words(rate, money(taken), exact, rate_formula(name, rate)),
                ));
                notes.extend(rounding(exact, amount));
                let kind_text = match cutoff {
                    Some(c) => say!(
                        "engine.working.percent_with_cutoff",
                        percent = nwords(rate),
                        cutoff = money(c)
                    ),
                    None => Words::from(format!("{}%", nwords(rate))),
                };
                let s = Step::value(name, kind_text, amount)
                    .with(notes)
                    .with_link(Link::Row {
                        ci,
                        ai,
                        row: details[i].id,
                    })
                    .with_expand(Target::LineFringe(details[i].id, name.to_string()));
                found[i].insert(name.to_string(), s);
            }
        }
    }

    // Flat fringes, per row.
    for (i, d) in details.iter().enumerate() {
        if !earns[i] {
            continue;
        }
        for name in &d.fringes {
            let f = &ctx.b.fringes[name];
            let FringeKind::Flat { unit, .. } = &f.kind else {
                continue;
            };
            let rate = rates[name.as_str()];
            // Counted as the engine counts, rounded once from the unrounded
            // amount; an exact division that need not end shows to 9 places.
            let c = flat_count(ctx.arith, &ctx.b.units, &f.kind, d, factors[i]).ok_or_else(ovf)?;
            let amount = c.amount(rate).ok_or_else(ovf)?;
            let exact = c.exact_amount(rate).ok_or_else(ovf)?;
            let (mut notes, text) = flat_words(ctx.b, name, d, unit.as_deref(), &c, rate, exact)?;
            if let (Some(n), Some(formula)) = (notes.last_mut(), rate_formula(name, rate)) {
                // The flat fringe's working is not catalogued (`text_said`
                // stays `None`), so its English can grow.
                debug_assert!(n.text_said.is_none());
                n.text.push_str(&format!(" (rate {formula})"));
            }
            notes.extend(rounding(exact, amount));
            let s = Step::value(name.as_str(), text, amount)
                .with(notes)
                .with_link(Link::Row { ci, ai, row: d.id })
                .with_expand(Target::LineFringe(d.id, name.clone()));
            found[i].insert(name.clone(), s);
        }
    }

    for (i, map) in found.into_iter().enumerate() {
        let mut total: Cents = 0;
        for s in map.values() {
            total = total.checked_add(s.amount.unwrap_or(0)).ok_or_else(ovf)?;
        }
        rows[i].fringe_total = total;
        rows[i].fringes = map.into_iter().collect();
    }
    Ok(())
}

/// "2100", "2100 and 2200", "2100, 2200 and 3100".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Percent fringe steps for every row with a person (the engine's person
/// pass, decisions.md, entry 79, worked out again with its working shown),
/// across every account, with the rows' and accounts' fringe totals added
/// up again.
fn persons_calc(ctx: &Ctx, accounts: &mut [Vec<AccountCalc>]) -> R<()> {
    // Each person's earning rows, as (ci, ai, row), in budget order.
    // By the matching form of the name; shown as the first row spells it.
    let mut people: BTreeMap<String, Vec<(usize, usize, usize)>> = BTreeMap::new();
    let mut shown: BTreeMap<String, &str> = BTreeMap::new();
    for (ci, cat) in ctx.b.categories().enumerate() {
        for (ai, acct) in cat.accounts.iter().enumerate() {
            for (i, d) in acct.details.iter().enumerate() {
                let Some(name) = d.person_name().filter(|_| d.kind.is_costed()) else {
                    continue;
                };
                let key = persons::person_key(name);
                shown.entry(key.clone()).or_insert(name);
                if accounts[ci][ai].rows.get(i).is_some_and(|r| r.included) {
                    people.entry(key).or_default().push((ci, ai, i));
                }
            }
        }
    }
    let blank_at = || String::new();
    let mut found: BTreeMap<(usize, usize, usize), BTreeMap<String, Step>> = BTreeMap::new();
    let detail = |ci: usize, ai: usize, i: usize| -> R<&Detail> {
        ctx.b
            .categories()
            .nth(ci)
            .and_then(|c| c.accounts.get(ai))
            .and_then(|a| a.details.get(i))
            .ok_or_else(|| gone("row"))
    };
    let code = |ci: usize, ai: usize| -> String {
        ctx.b
            .categories()
            .nth(ci)
            .and_then(|c| c.accounts.get(ai))
            .map(|a| a.code.clone())
            .unwrap_or_default()
    };
    for (key, at) in &people {
        let person = shown.get(key).copied().unwrap_or(key.as_str());
        let mut names: BTreeSet<&str> = BTreeSet::new();
        for &(ci, ai, i) in at {
            names.extend(detail(ci, ai, i)?.fringes.iter().map(String::as_str));
        }
        for name in names {
            let Some(f) = ctx.b.fringes.get(name) else {
                continue;
            };
            let FringeKind::Percent { cutoff: typed } = f.kind else {
                continue;
            };
            let cutoff = factored_cutoff(ctx.arith, typed, ctx.b.cutoff_factor).ok_or_else(ovf)?;
            let rate = ctx.eval(&f.rate, Dec::ZERO, &blank_at)?;
            let mut lines = Vec::new();
            for &(ci, ai, i) in at {
                let d = detail(ci, ai, i)?;
                if d.fringes.contains(name) {
                    lines.push((ci, ai, i, d.id, accounts[ci][ai].rows[i].total));
                }
            }
            let input: Vec<(RowId, Cents)> = lines.iter().map(|l| (l.3, l.4)).collect();
            let (split, shares) = person_shares(ctx.arith, rate, cutoff, &input).ok_or_else(ovf)?;
            let mut codes: Vec<String> = Vec::new();
            for l in &lines {
                let c = code(l.0, l.1);
                if !codes.contains(&c) {
                    codes.push(c);
                }
            }
            let count = lines.len();
            let accounts = and_list(&codes);
            let who = match (codes.len() == 1, cutoff.is_some()) {
                (true, true) => say!(
                    "engine.working.person_paid_account",
                    person = person,
                    count = count,
                    accounts = accounts
                ),
                (false, true) => say!(
                    "engine.working.person_paid_accounts",
                    person = person,
                    count = count,
                    accounts = accounts
                ),
                (true, false) => say!(
                    "engine.working.person_paid_account_no_cutoff",
                    person = person,
                    count = count,
                    accounts = accounts,
                    fringe = name
                ),
                (false, false) => say!(
                    "engine.working.person_paid_accounts_no_cutoff",
                    person = person,
                    count = count,
                    accounts = accounts,
                    fringe = name
                ),
            };
            let rate_formula = if f.rate.0.trim().parse::<Dec>().is_ok() {
                None
            } else {
                Some(formula_words(ctx, &f.rate, rate))
            };
            for (l, share) in lines.iter().zip(&shares) {
                let (ci, ai, i, id, total) = *l;
                let mut notes = vec![
                    Step::note(say!("engine.working.label.person"), who.clone()),
                    Step::note_amount(
                        say!("engine.working.label.earnings"),
                        say!("engine.working.this_lines_total"),
                        total,
                    ),
                ];
                if let (Some(t), Some(c)) = (typed, cutoff) {
                    if ctx.b.cutoff_factor != CutoffFactor::One {
                        notes.push(cutoff_factor_note(ctx.b.cutoff_factor, t, c));
                    }
                }
                let pay: Cents = input
                    .iter()
                    .try_fold(0, |a: Cents, r| a.checked_add(r.1))
                    .ok_or_else(ovf)?;
                match split {
                    Split::Own => {
                        if let Some(c) = cutoff {
                            notes.push(Step::note_amount(
                                say!("engine.working.label.cutoff"),
                                say!(
                                    "engine.working.per_person_not_reached",
                                    cutoff = money(c),
                                    person = person,
                                    pay = money(pay),
                                    fringe = name
                                ),
                                pay,
                            ));
                        }
                        let exact = percent_of(ctx.arith, total, rate).ok_or_else(ovf)?;
                        notes.push(Step::note(
                            say!("engine.working.label.fringe"),
                            percent_words(rate, money(total), exact, rate_formula.clone()),
                        ));
                        notes.extend(rounding(exact, share.amount));
                    }
                    Split::Nothing => {
                        let c = cutoff.unwrap_or(0);
                        notes.push(Step::note_amount(
                            say!("engine.working.label.cutoff_used_up"),
                            say!(
                                "engine.working.stops_per_person",
                                fringe = name,
                                cutoff = money(c)
                            ),
                            0,
                        ));
                    }
                    Split::Shared {
                        pay,
                        cutoff: c,
                        exact,
                        amount,
                    } => {
                        let text = match &rate_formula {
                            None => say!(
                                "engine.working.person_past_cutoff",
                                person = person,
                                pay = money(pay),
                                fringe = name,
                                cutoff = money(c),
                                percent = nwords(rate),
                                exact = dmoney(exact)
                            ),
                            Some(formula) => say!(
                                "engine.working.person_past_cutoff_rate",
                                person = person,
                                pay = money(pay),
                                fringe = name,
                                cutoff = money(c),
                                percent = nwords(rate),
                                exact = dmoney(exact),
                                formula = formula
                            ),
                        };
                        notes.push(Step::note_amount(
                            say!("engine.working.label.cutoff_reached"),
                            text,
                            amount,
                        ));
                        notes.extend(rounding(exact, amount));
                        let floor = if share.extra_cent {
                            share.amount.checked_sub(1).ok_or_else(ovf)?
                        } else {
                            share.amount
                        };
                        let code = if share.extra_cent {
                            "engine.working.share_extra_cent"
                        } else {
                            "engine.working.share"
                        };
                        let text = Words::said(
                            code,
                            vec![
                                ("person", person.to_string()),
                                ("amount", money(amount)),
                                ("total", money(total)),
                                ("pay", money(pay)),
                                ("floor", money(floor)),
                            ],
                        );
                        notes.push(Step::note(say!("engine.working.label.share"), text));
                    }
                }
                let kind_text = match cutoff {
                    Some(c) => say!(
                        "engine.working.percent_with_cutoff_per_person",
                        percent = nwords(rate),
                        cutoff = money(c)
                    ),
                    None => Words::from(format!("{}%", nwords(rate))),
                };
                let s = Step::value(name, kind_text, share.amount)
                    .with(notes)
                    .with_link(Link::Row { ci, ai, row: id })
                    .with_expand(Target::LineFringe(id, name.to_string()));
                found
                    .entry((ci, ai, i))
                    .or_default()
                    .insert(name.to_string(), s);
            }
        }
    }
    for ((ci, ai, i), steps) in found {
        let row = &mut accounts[ci][ai].rows[i];
        let mut map: BTreeMap<String, Step> =
            std::mem::take(&mut row.fringes).into_iter().collect();
        map.extend(steps);
        let mut total: Cents = 0;
        for s in map.values() {
            total = total.checked_add(s.amount.unwrap_or(0)).ok_or_else(ovf)?;
        }
        row.fringe_total = total;
        row.fringes = map.into_iter().collect();
    }
    for list in accounts.iter_mut() {
        for a in list.iter_mut() {
            a.fringe_total = a
                .rows
                .iter()
                .try_fold(0, |t: Cents, r| t.checked_add(r.fringe_total))
                .ok_or_else(ovf)?;
        }
    }
    Ok(())
}

/// The budget's cutoff factor applied to `typed`, giving `cutoff`.
fn cutoff_factor_note(factor: CutoffFactor, typed: Cents, cutoff: Cents) -> Step {
    Step::note(
        say!("engine.working.label.cutoff_factor"),
        say!(
            "engine.working.cutoff_factor",
            factor = factor.name(),
            typed = money(typed),
            cutoff = money(cutoff)
        ),
    )
}

/// "18.5% of 1,000.00 = 185.00", with the rate's formula after it when it
/// has one.
fn percent_words(
    rate: Num,
    base: String,
    exact: impl fmt::Display,
    formula: Option<String>,
) -> Words {
    match formula {
        None => say!(
            "engine.working.percent_of",
            percent = nwords(rate),
            base = base,
            exact = dmoney(exact)
        ),
        Some(formula) => say!(
            "engine.working.percent_of_rate",
            percent = nwords(rate),
            base = base,
            exact = dmoney(exact),
            formula = formula
        ),
    }
}

/// `1 hour`, `12 hours`.
fn hours_words(h: Dec) -> String {
    if h == Dec::ONE {
        "1 hour".into()
    } else {
        format!("{} hours", num(h))
    }
}

/// How a flat fringe's basis reads in the working.
fn basis_words(b: FlatBasis) -> &'static str {
    match b {
        FlatBasis::Amount => "amount only",
        FlatBasis::AmountCounts => "amount plus counts in X and 4X",
        FlatBasis::Quantity => "full quantity",
    }
}

/// The notes for flat fringe `name` on line `d` (its count `c`, `rate` per
/// `unit`, `exact` before rounding) and its value step's text: how many
/// units the line counts and why, then the fringe itself, e.g. "ZDAY
/// 100.00 per Day x 25 Days (amount only; 14 Hours in X ignored) =
/// 2,500.00".
fn flat_words(
    b: &Budget,
    name: &str,
    d: &Detail,
    unit: Option<&str>,
    c: &FlatCount,
    rate: Num,
    exact: Money,
) -> R<(Vec<Step>, Words)> {
    let fu = unit.map(str::trim).filter(|u| !u.is_empty());
    let fu_text = fu.unwrap_or("line");
    let count = c.count().ok_or_else(ovf)?;
    let lu = d.unit.trim();
    let qty = |v: Dec, u: &str| format!("{}{}", num(v), unit_words(b, u, v));
    let factor_words = |f: Factor, v: Dec| match f {
        Factor::X => format!("{} in X", qty(v, &d.unit2)),
        Factor::X4 => format!("{} in 4X", qty(v, &d.unit4)),
    };
    let old_quantity =
        c.basis == FlatBasis::Quantity && !matches!(c.from, CountFrom::Capped { .. });
    let mut paren: Vec<String> = Vec::new();
    let mut clauses: Vec<String> = Vec::new();
    // The count before X and 4X, and whether it is a "once".
    let (start, once) = match &c.from {
        CountFrom::PerLine => {
            paren.push("once per line".into());
            (Dec::ONE, true)
        }
        CountFrom::FringeNoHours(u) => {
            paren.push(format!("{} has no hours: charged once", u.trim()));
            (Dec::ONE, true)
        }
        CountFrom::LineNoHours(u) if u.trim().is_empty() => {
            paren.push("the line has no unit: charged once".into());
            (Dec::ONE, true)
        }
        CountFrom::LineNoHours(u) => {
            paren.push(format!("{} has no hours: charged once", u.trim()));
            (Dec::ONE, true)
        }
        CountFrom::NoAmount => {
            paren.push("the Amount is 0: nothing counted".into());
            (Dec::ZERO, true)
        }
        CountFrom::SameUnit => {
            paren.push(basis_words(c.basis).into());
            if old_quantity && (!d.x.is_blank() || !d.x4.is_blank()) {
                clauses.push(format!(
                    "the line's quantity (amount x X x 4X) is {}",
                    qty(c.base, lu)
                ));
            }
            (c.base, false)
        }
        CountFrom::Hours {
            line_hours,
            fringe_hours,
        } => {
            paren.push(basis_words(c.basis).into());
            let converted = Exact::dec(c.base)
                .checked_mul(*line_hours)
                .and_then(|e| e.div_to_dec(*fringe_hours))
                .ok_or_else(ovf)?;
            clauses.push(format!(
                "{}{} x {} per {lu} / {} per {fu_text} = {}",
                if old_quantity { "quantity " } else { "" },
                qty(c.base, lu),
                hours_words(*line_hours),
                hours_words(*fringe_hours),
                qty(converted, fu_text)
            ));
            (converted, false)
        }
        CountFrom::Capped {
            line_hours,
            fringe_hours,
            day_unit,
            day_hours,
            per_day,
            cap,
            from_x,
        } => {
            paren.push(basis_words(c.basis).into());
            let days = Exact::dec(c.base)
                .checked_mul(*line_hours)
                .and_then(|e| e.div_to_dec(*day_hours))
                .ok_or_else(ovf)?;
            let used = (*per_day).min(*cap);
            let days_text = if lu == day_unit {
                format!("{} days", num(days))
            } else {
                format!(
                    "{} x {} per {lu} / {} per {day_unit} = {} days",
                    qty(c.base, lu),
                    hours_words(*line_hours),
                    hours_words(*day_hours),
                    num(days)
                )
            };
            let per_day_text = if *from_x {
                format!("{} a day from X", hours_words(*per_day))
            } else {
                format!("{} a day (one {day_unit})", hours_words(*per_day))
            };
            let cap_text = if *per_day > *cap {
                paren.push(format!("capped at {} a day", hours_words(*cap)));
                format!("capped at {} a day", hours_words(*cap))
            } else {
                format!("within the cap of {} a day", hours_words(*cap))
            };
            let hours = days.checked_mul(used).ok_or_else(ovf)?;
            let converted = if *fringe_hours == Dec::ONE {
                hours
            } else {
                Exact::dec(hours)
                    .div_to_dec(*fringe_hours)
                    .ok_or_else(ovf)?
            };
            let per_unit = if *fringe_hours == Dec::ONE {
                String::new()
            } else {
                format!(" / {} per {fu_text}", hours_words(*fringe_hours))
            };
            clauses.push(format!(
                "{days_text}; {per_day_text}, {cap_text}: {} days x {}{per_unit} = {}",
                num(days),
                hours_words(used),
                qty(converted, fu_text)
            ));
            (converted, false)
        }
    };
    if !c.counted.is_empty() {
        if once {
            paren.push(basis_words(c.basis).into());
        }
        let parts: Vec<String> = c
            .counted
            .iter()
            .map(|&(f, v)| match f {
                Factor::X => format!("{} (X)", qty(v, &d.unit2)),
                Factor::X4 => format!("{} (4X)", qty(v, &d.unit4)),
            })
            .collect();
        let from = if once {
            "1 (once)".to_string()
        } else {
            qty(start, fu_text)
        };
        clauses.push(format!(
            "{from} x {} = {}",
            parts.join(" x "),
            if once {
                num(count)
            } else {
                qty(count, fu_text)
            }
        ));
    }
    if !c.ignored.is_empty() {
        let parts: Vec<String> = c.ignored.iter().map(|&(f, v)| factor_words(f, v)).collect();
        paren.push(format!("{} ignored", parts.join(" and ")));
    }
    let count_text = if once {
        num(count)
    } else {
        qty(count, fu_text)
    };
    let mut notes = Vec::new();
    if !clauses.is_empty() {
        notes.push(Step::note(
            say!("engine.working.label.count"),
            clauses.join("; "),
        ));
    }
    if let FringeKind::Flat {
        cutoff: Some(cut), ..
    } = &b.fringes[name].kind
    {
        notes.push(Step::note(
            say!("engine.working.label.cutoff"),
            say!(
                "engine.working.flat_cutoff_unused",
                cutoff = money(*cut),
                fringe = name
            ),
        ));
    }
    notes.push(Step::note(
        say!("engine.working.label.fringe"),
        format!(
            "{name} {} per {fu_text} x {count_text} ({}) = {}",
            dmoney(Money::num(rate)),
            paren.join("; "),
            dmoney(exact)
        ),
    ));
    let text = match (&c.from, fu, c.basis) {
        (CountFrom::PerLine, _, _) | (_, None, _) => say!("engine.working.flat_once_per_line"),
        (_, Some(u), FlatBasis::Amount) => say!("engine.working.flat_per_unit_amount", unit = u),
        (_, Some(u), FlatBasis::AmountCounts) => {
            say!("engine.working.flat_per_unit_counts", unit = u)
        }
        (_, Some(u), FlatBasis::Quantity) => {
            say!("engine.working.flat_per_unit_quantity", unit = u)
        }
    };
    Ok((notes, text))
}

// ---------------------------------------------------------------------------
// Naming a total from text (the command line)

impl Target {
    /// A total named the way a person types it:
    /// - `grand` (or `topsheet`), `net`, `atl`, `btl`, `atl+btl`, `fringes`
    ///   (the budget's Total Fringes row), `credits`
    /// - `charge:NAME` or `charge:N`, `credit:NAME` or `credit:N` (N counts
    ///   applied ones from 1), `level:NAME` or `level:N`, `fringe:NAME`
    /// - `#ID` or `line:ID`: a row by id; `ACCOUNT:N`: line N of an account
    /// - `ACCOUNT` or `account:ACCOUNT`, `CATEGORY` or `category:CATEGORY`
    ///   (an account number wins over a category number), with `/lines` or
    ///   `/fringes` after it for those parts
    pub fn parse(b: &Budget, report: &Report, text: &str) -> Result<Target, ExplainError> {
        let t = text.trim();
        let lower = t.to_ascii_lowercase();
        let bad = || ExplainError(format!("`{t}` doesn't name a total in this budget"));
        match lower.as_str() {
            "grand" | "grand-total" | "total" | "topsheet" => return Ok(Target::GrandTotal),
            "net" | "net-total" => return Ok(Target::NetTotal),
            "atl" => return Ok(Target::Atl),
            "btl" => return Ok(Target::Btl),
            "atl+btl" | "atlbtl" | "atl-btl" => return Ok(Target::AtlBtl),
            "fringes" | "budget-fringes" => return Ok(Target::BudgetFringes),
            "credits" => return Ok(Target::CreditsTotal),
            _ => {}
        }
        let pick = |list: Vec<&str>, key: &str| -> Option<usize> {
            if let Ok(n) = key.parse::<usize>() {
                if n >= 1 && n <= list.len() {
                    return Some(n - 1);
                }
            }
            list.iter().position(|n| n.eq_ignore_ascii_case(key))
        };
        if let Some((kind, key)) = t.split_once(':') {
            let key = key.trim();
            match kind.trim().to_ascii_lowercase().as_str() {
                "charge" => {
                    let names = report.charges.iter().map(|c| c.name.as_str()).collect();
                    return pick(names, key).map(Target::Charge).ok_or_else(bad);
                }
                "credit" => {
                    let names = report.credits.iter().map(|c| c.name.as_str()).collect();
                    return pick(names, key).map(Target::Credit).ok_or_else(bad);
                }
                "level" => {
                    let names = report.levels.iter().map(|l| l.name.as_str()).collect();
                    return pick(names, key).map(Target::Level).ok_or_else(bad);
                }
                "fringe" => {
                    return b
                        .fringes
                        .keys()
                        .find(|k| k.eq_ignore_ascii_case(key))
                        .map(|k| Target::Fringe(k.clone()))
                        .ok_or_else(bad);
                }
                "line" => {
                    return key.parse().map(Target::Line).map_err(|_| bad());
                }
                "account" => return find_account(b, key).ok_or_else(bad),
                "category" => return find_category(b, key).ok_or_else(bad),
                _ => {
                    // ACCOUNT:N, line N of an account.
                    if let (Some(Target::Account { ci, ai }), Ok(n)) =
                        (find_account(b, kind.trim()), key.parse::<usize>())
                    {
                        let acct = &b.categories().nth(ci).ok_or_else(bad)?.accounts[ai];
                        return n
                            .checked_sub(1)
                            .and_then(|i| acct.details.get(i))
                            .map(|d| Target::Line(d.id))
                            .ok_or_else(bad);
                    }
                    return Err(bad());
                }
            }
        }
        if let Some(id) = t.strip_prefix('#') {
            return id.trim().parse().map(Target::Line).map_err(|_| bad());
        }
        let (code, part) = match t.split_once('/') {
            Some((c, p)) => (c.trim(), Some(p.trim().to_ascii_lowercase())),
            None => (t, None),
        };
        if let Some(Target::Account { ci, ai }) = find_account(b, code) {
            return match part.as_deref() {
                None => Ok(Target::Account { ci, ai }),
                Some("lines") => Ok(Target::AccountLines { ci, ai }),
                Some("fringes") => Ok(Target::AccountFringes { ci, ai }),
                _ => Err(bad()),
            };
        }
        if let Some(Target::Category(ci)) = find_category(b, code) {
            return match part.as_deref() {
                None => Ok(Target::Category(ci)),
                Some("fringes") => Ok(Target::CategoryFringes(ci)),
                _ => Err(bad()),
            };
        }
        Err(bad())
    }
}

/// An account by number: exactly as written, else as the budget's coding
/// normalizes it (`11-5` is `11-05`), never by cutting it short.
fn find_account(b: &Budget, code: &str) -> Option<Target> {
    let coding = &b.account_coding;
    let find = |hit: &dyn Fn(&str) -> bool| {
        b.categories().enumerate().find_map(|(ci, cat)| {
            cat.accounts
                .iter()
                .position(|a| hit(&a.code))
                .map(|ai| Target::Account { ci, ai })
        })
    };
    // As written first, so a number the pattern would read as another
    // account's never shadows the account it names exactly.
    if let Some(t) = find(&|a| a == code) {
        return Some(t);
    }
    let norm = coding.read(code).map(|c| coding.format(&c))?;
    find(&|a| a == norm)
}

fn find_category(b: &Budget, code: &str) -> Option<Target> {
    b.categories()
        .position(|c| c.code == code)
        .map(Target::Category)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_words() {
        assert_eq!(money(0), "0.00");
        assert_eq!(money(-5), "-0.05");
        assert_eq!(money(123_456_789), "1,234,567.89");
        assert_eq!(money(-100_000), "-1,000.00");
        assert_eq!(money(i64::MIN), "-92,233,720,368,547,758.08");
        assert_eq!(dmoney("1296.315".parse::<Dec>().unwrap()), "1,296.315");
        assert_eq!(dmoney("-54000".parse::<Dec>().unwrap()), "-54,000.00");
        assert_eq!(dmoney("0.5".parse::<Dec>().unwrap()), "0.50");
        assert_eq!(dmoney(Exact::new(49_999_995, 10)), "0.0049999995");
        assert_eq!(dmoney(Exact::new(-5_400_000, 2)), "-54,000.00");
        assert_eq!(num("1234567.25".parse().unwrap()), "1,234,567.25");
        assert_eq!(num("-12".parse().unwrap()), "-12");
    }

    /// The working's words: sorted, each code written here once or more,
    /// every code written here in the table, and each one filled with
    /// values for all its names.
    #[test]
    fn the_wording_table_is_sorted_and_every_code_is_used() {
        for pair in WORDING.windows(2) {
            assert!(
                pair[0].code < pair[1].code,
                "{} is out of order",
                pair[1].code
            );
        }
        let src = include_str!("explain.rs");
        let body = &src[..src.find("pub const WORDING").unwrap()];
        let rest = &src[src.find("/// Words in the working").unwrap()..];
        let mut used = BTreeSet::new();
        for part in [body, rest] {
            for (at, _) in part.match_indices("\"engine.working.") {
                let code = &part[at + 1..];
                let code = &code[..code.find('"').unwrap()];
                if code != WORDING_PREFIX {
                    used.insert(code);
                }
            }
        }
        for code in &used {
            assert!(
                WORDING.iter().any(|w| w.code == *code),
                "{code} is used but not in WORDING"
            );
        }
        for w in WORDING {
            assert!(w.code.starts_with(WORDING_PREFIX), "{}", w.code);
            assert!(used.contains(w.code), "{} is never said", w.code);
            for english in w.one.into_iter().chain([w.other]) {
                assert!(!english.contains('\u{2014}') && !english.contains('!'));
                let mut names = Vec::new();
                let mut t = english;
                while let Some(at) = t.find('{') {
                    let end = t[at..].find('}').unwrap() + at;
                    let name: &'static str = Box::leak(t[at + 1..end].to_string().into_boxed_str());
                    if !names.contains(&name) {
                        names.push(name);
                    }
                    t = &t[end + 1..];
                }
                let params: Vec<(&'static str, String)> = names
                    .iter()
                    .map(|n| {
                        (
                            *n,
                            if *n == "count" {
                                "2".into()
                            } else {
                                format!("<{n}>")
                            },
                        )
                    })
                    .collect();
                let words = Words::said(w.code, params.clone());
                assert_eq!(words.said.as_ref().unwrap().params, params);
                assert!(!words.text.contains('{'), "{}: {}", w.code, words.text);
            }
        }
    }

    #[test]
    fn leaf_sum_checks_sums() {
        let a = Step::value("a", "", 5);
        let b = Step {
            minus: true,
            ..Step::value("b", "", 2)
        };
        let s = Step::sum("s", "", vec![a.clone(), b, Step::note("n", "")]).unwrap();
        assert_eq!(s.amount, Some(3));
        assert_eq!(s.leaf_sum(), Some(3));
        let mut broken = s.clone();
        broken.amount = Some(4);
        assert_eq!(broken.leaf_sum(), None);
        // A value's children are notes only.
        let mut v = a;
        v.children.push(Step::value("x", "", 1));
        assert_eq!(v.leaf_sum(), None);
    }
}
