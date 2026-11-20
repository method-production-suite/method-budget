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

mod account;
mod charges;
mod totals;
mod wording;

use account::{account_calc, persons_calc};
use totals::line_label;
pub(crate) use wording::dmoney;
use wording::{group, num, rounding, Words};
pub use wording::{money, Said, Wording, WORDING, WORDING_PREFIX};

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
        let src = include_str!("explain/wording.rs");
        let body = &src[..src.find("pub const WORDING").unwrap()];
        let rest = &src[src.find("/// Words in the working").unwrap()..];
        let mut used = BTreeSet::new();
        for part in [
            include_str!("explain.rs"),
            include_str!("explain/account.rs"),
            include_str!("explain/charges.rs"),
            include_str!("explain/totals.rs"),
            body,
            rest,
        ] {
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
