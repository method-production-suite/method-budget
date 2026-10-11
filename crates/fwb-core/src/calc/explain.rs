//! "Explain this number" (roadmap 3.5; decisions.md, entry 43).
//!
//! For any total the budget reports (a line, an account, a category, a
//! production level, a fringe total, a charge, a credit, the grand or net
//! total) [`Explainer::explain`] gives a tree of [`Step`]s that reproduces it
//! exactly, in plain words: each line as amount x X x 4X x rate with globals
//! shown by name and value, the currency conversion, every rounding step
//! (unrounded to rounded, half away from zero), each fringe on each line
//! (percent or flat, the cutoff and how much of it each line of a range used,
//! the largest-remainder share), where fringes post, what a group or a
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

use super::fringes::{allocate, earners};
use super::topsheet::{credit_cap, credit_keep, credit_row};
use super::{recalc, CalcError, Ctx, RecalcOptions};
use crate::decimal::{mul_cents, percent_of, Dec, Exact};
use crate::expr::Expr;
use crate::model::{
    Budget, ChargeKind, CreditCap, CreditSource, Detail, Formula, FringeKind, FringePosting, RowId,
    RowKind, TopsheetRow,
};
use crate::report::{Report, RowTotals};
use crate::Cents;

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
    fn note(label: impl Into<String>, text: impl Into<String>) -> Step {
        Step {
            label: label.into(),
            text: text.into(),
            amount: None,
            role: Role::Note,
            minus: false,
            link: None,
            expand: None,
            children: Vec::new(),
        }
    }

    fn note_amount(label: impl Into<String>, text: impl Into<String>, amount: Cents) -> Step {
        Step {
            amount: Some(amount),
            ..Step::note(label, text)
        }
    }

    fn value(label: impl Into<String>, text: impl Into<String>, amount: Cents) -> Step {
        Step {
            role: Role::Value,
            amount: Some(amount),
            ..Step::note(label, text)
        }
    }

    /// A sum of `children`; its amount is worked out from them.
    fn sum(label: impl Into<String>, text: impl Into<String>, children: Vec<Step>) -> R<Step> {
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
    /// The number as the budget reports it.
    pub reported: Cents,
    /// The working; its amount is `reported`.
    pub root: Step,
}

impl Explanation {
    /// The tree as indented text, `depth` levels deep (`None`: all of it).
    pub fn to_text(&self, depth: Option<usize>) -> String {
        let mut out = format!("{}: {}\n", self.title, money(self.reported));
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
// Number formatting for the plain words

/// `123456` cents as `1,234.56`, negatives with a minus sign.
pub fn money(c: Cents) -> String {
    let sign = if c < 0 { "-" } else { "" };
    let a = c.unsigned_abs();
    format!("{sign}{}.{:02}", group(&(a / 100).to_string()), a % 100)
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

/// "rounded half away from zero to X" when rounding changed `exact`.
fn rounding(exact: impl Into<Exact>, rounded: Cents) -> Option<Step> {
    let exact = exact.into();
    (!exact.is_cents(rounded)).then(|| {
        Step::note(
            "Rounding",
            format!(
                "{} rounds half away from zero to {}",
                dmoney(exact),
                money(rounded)
            ),
        )
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
        let ctx = Ctx {
            b,
            globals: report.globals.clone(),
            opts,
            unfiltered: false,
        };
        let mut accounts = Vec::new();
        for (ci, cat) in b.categories().enumerate() {
            let mut list = Vec::with_capacity(cat.accounts.len());
            for (ai, acct) in cat.accounts.iter().enumerate() {
                list.push(account_calc(&ctx, ci, ai, &acct.code, &acct.details)?);
            }
            accounts.push(list);
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
                    if d.kind == RowKind::Line {
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
            title,
            reported,
            root,
        })
    }

    // -----------------------------------------------------------------------
    // Lines

    fn line(&self, id: RowId) -> R<(String, Step)> {
        let (ci, ai, i) = self.find_row(id)?;
        let acct = self.acct(ci, ai);
        let d = &acct.details[i];
        let r = &self.accounts[ci][ai].rows[i];
        let title = format!("{}: total", row_name(&acct.code, i, d));
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
                let s = Step::sum(
                    d.description.trim().to_string().or("Subtotal"),
                    "the lines and adjustments since the previous subtotal row",
                    children,
                )?
                .with_link(Link::Row { ci, ai, row: id });
                Ok((title, s))
            }
            RowKind::Line | RowKind::Adjustment => {
                let mut s = r.value.clone().ok_or_else(|| gone("that line"))?;
                if d.kind == RowKind::Line && r.included && r.fringe_total != 0 {
                    s.children.push(
                        Step::note_amount(
                            "Fringes",
                            "earned on top of this total, shown in Fringe $ (not in it)",
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
                "Left out",
                "the line doesn't count, so it earns no fringes",
            ));
        }
        Ok(Step::sum(
            line_label(i, d),
            format!("fringes earned by line {}", i + 1),
            children,
        )?
        .with_link(Link::Row { ci, ai, row: d.id })
        .with_expand(Target::LineFringes(d.id)))
    }

    fn line_fringes(&self, id: RowId) -> R<(String, Step)> {
        let (ci, ai, i) = self.find_row(id)?;
        let acct = self.acct(ci, ai);
        let title = format!("{}: fringes", row_name(&acct.code, i, &acct.details[i]));
        let mut s = self.row_fringes_step(ci, ai, i)?;
        s.expand = None;
        if s.children.is_empty() {
            s.children
                .push(Step::note("No fringes", "this line earns none"));
        }
        s.children.push(self.posting_note());
        Ok((title, s))
    }

    fn line_fringe(&self, id: RowId, name: &str) -> R<(String, Step)> {
        let (ci, ai, i) = self.find_row(id)?;
        let acct = self.acct(ci, ai);
        let r = &self.accounts[ci][ai].rows[i];
        let title = format!("{}: {name}", row_name(&acct.code, i, &acct.details[i]));
        let s = r
            .fringes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, s)| s.clone())
            .unwrap_or_else(|| {
                Step::value(name, "this line earns nothing from it", 0).with_link(Link::Row {
                    ci,
                    ai,
                    row: id,
                })
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
        let children = calc.rows.iter().filter_map(|r| r.value.clone()).collect();
        Ok(Step::sum(
            "Lines",
            format!("every line of {}", self.account_name(ci, ai)),
            children,
        )?
        .with_link(Link::Account { ci, ai })
        .with_expand(Target::AccountLines { ci, ai }))
    }

    fn account_fringes_step(&self, ci: usize, ai: usize) -> R<Step> {
        let acct = self.acct(ci, ai);
        let mut children = Vec::new();
        for (i, d) in acct.details.iter().enumerate() {
            if d.kind == RowKind::Line && !self.accounts[ci][ai].rows[i].fringes.is_empty() {
                children.push(self.row_fringes_step(ci, ai, i)?);
            }
        }
        Ok(Step::sum(
            self.account_name(ci, ai),
            "fringes earned by the account's lines",
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
            f.label = "Fringes".into();
            f.text = "posted to the account (fringe posting: by account)".into();
            children.push(f);
        } else if self.accounts[ci][ai].fringe_total != 0 {
            children.push(
                Step::note_amount(
                    "Fringes earned here",
                    format!("not in this total: {}", self.posting_words()),
                    self.accounts[ci][ai].fringe_total,
                )
                .with_expand(Target::AccountFringes { ci, ai }),
            );
        }
        Ok(
            Step::sum(self.account_name(ci, ai), "account total", children)?
                .with_link(Link::Account { ci, ai })
                .with_expand(Target::Account { ci, ai }),
        )
    }

    fn account(&self, ci: usize, ai: usize) -> R<(String, Step)> {
        self.check_account(ci, ai)?;
        let mut s = self.account_step(ci, ai)?;
        s.expand = None;
        Ok((format!("Account {}: total", self.account_name(ci, ai)), s))
    }

    fn account_lines(&self, ci: usize, ai: usize) -> R<(String, Step)> {
        self.check_account(ci, ai)?;
        let mut s = self.lines_step(ci, ai)?;
        s.expand = None;
        Ok((format!("Account {}: lines", self.account_name(ci, ai)), s))
    }

    fn account_fringes(&self, ci: usize, ai: usize) -> R<(String, Step)> {
        self.check_account(ci, ai)?;
        let mut s = self.account_fringes_step(ci, ai)?;
        s.expand = None;
        s.children.push(self.posting_note());
        Ok((format!("Account {}: fringes", self.account_name(ci, ai)), s))
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
            "fringes earned in the category",
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
                f.label = format!("{number} Fringes").trim().to_string();
                f.text = "the category's fringe account (fringe posting: by category)".into();
                children.push(f);
            }
            FringePosting::Account => {}
            _ => {
                let total = self.category_fringe_total(ci)?;
                if total != 0 {
                    children.push(
                        Step::note_amount(
                            "Fringes earned here",
                            format!("not in this total: {}", self.posting_words()),
                            total,
                        )
                        .with_expand(Target::CategoryFringes(ci)),
                    );
                }
            }
        }
        Ok(
            Step::sum(self.category_name(ci), "category total", children)?
                .with_link(Link::Category { ci })
                .with_expand(Target::Category(ci)),
        )
    }

    fn category_fringe_total(&self, ci: usize) -> R<Cents> {
        self.accounts[ci]
            .iter()
            .try_fold(0, |a: Cents, c| a.checked_add(c.fringe_total))
            .ok_or_else(ovf)
    }

    fn category(&self, ci: usize) -> R<(String, Step)> {
        self.check_category(ci)?;
        let mut s = self.category_step(ci)?;
        s.expand = None;
        Ok((format!("Category {}: total", self.category_name(ci)), s))
    }

    fn category_fringes(&self, ci: usize) -> R<(String, Step)> {
        self.check_category(ci)?;
        let mut s = self.category_fringes_step(ci)?;
        s.expand = None;
        s.children.push(self.posting_note());
        Ok((format!("Category {}: fringes", self.category_name(ci)), s))
    }

    // -----------------------------------------------------------------------
    // Topsheet

    fn posting_words(&self) -> String {
        match &self.b.fringe_posting {
            FringePosting::Budget => "fringes post to the budget's one Total Fringes row".into(),
            FringePosting::ProductionLevel => "fringes post above each production total".into(),
            FringePosting::Category { .. } => {
                "fringes post to each category's fringe account".into()
            }
            FringePosting::Account => "fringes post to each account's total".into(),
        }
    }

    fn posting_note(&self) -> Step {
        Step::note(
            "Where fringes post",
            format!("{} (a budget setting)", self.posting_words()),
        )
    }

    fn level_fringes_step(&self, li: usize) -> R<Step> {
        let lv = self
            .report
            .levels
            .get(li)
            .ok_or_else(|| gone("that production level"))?;
        let children = if self.b.fringe_posting == FringePosting::ProductionLevel {
            (lv.start..lv.end)
                .map(|ci| self.category_fringes_step(ci))
                .collect::<R<Vec<_>>>()?
        } else {
            Vec::new()
        };
        Ok(Step::sum(
            "Fringes",
            "posted above the production total (fringe posting: by production level)",
            children,
        )?
        .with_expand(Target::LevelFringes(li)))
    }

    fn level_name(&self, li: usize) -> String {
        match self.report.levels.get(li) {
            Some(l) if !l.name.is_empty() => l.name.clone(),
            _ => "Categories after the last production total".into(),
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
        Ok(Step::sum(
            self.level_name(li),
            "the categories since the previous production total",
            children,
        )?
        .with_expand(Target::Level(li)))
    }

    fn level(&self, li: usize) -> R<(String, Step)> {
        let mut s = self.level_step(li)?;
        s.expand = None;
        Ok((format!("{}: total", self.level_name(li)), s))
    }

    fn level_fringes(&self, li: usize) -> R<(String, Step)> {
        let mut s = self.level_fringes_step(li)?;
        s.expand = None;
        s.children.push(self.posting_note());
        Ok((format!("{}: fringes", self.level_name(li)), s))
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

    fn atl_step(&self) -> R<Step> {
        let split = usize::from(self.closed_levels() > 0).min(self.report.levels.len());
        let mut children = (0..split)
            .map(|li| self.level_step(li))
            .collect::<R<Vec<_>>>()?;
        if split == 0 {
            children.push(Step::note(
                "No production totals",
                "everything is below the line until a production total ends the above-the-line part",
            ));
        }
        Ok(Step::sum(
            "Total above-the-line",
            "the first production level",
            children,
        )?
        .with_expand(Target::Atl))
    }

    fn btl_step(&self) -> R<Step> {
        let split = usize::from(self.closed_levels() > 0).min(self.report.levels.len());
        let children = (split..self.report.levels.len())
            .map(|li| self.level_step(li))
            .collect::<R<Vec<_>>>()?;
        Ok(Step::sum(
            "Total below-the-line",
            "every production level after the first",
            children,
        )?
        .with_expand(Target::Btl))
    }

    fn atl_btl_step(&self) -> R<Step> {
        Ok(Step::sum(
            "Total above and below-the-line",
            "above-the-line plus below-the-line",
            vec![self.atl_step()?, self.btl_step()?],
        )?
        .with_expand(Target::AtlBtl))
    }

    fn atl(&self) -> R<(String, Step)> {
        let mut s = self.atl_step()?;
        s.expand = None;
        Ok(("Total above-the-line".into(), s))
    }

    fn btl(&self) -> R<(String, Step)> {
        let mut s = self.btl_step()?;
        s.expand = None;
        Ok(("Total below-the-line".into(), s))
    }

    fn atl_btl(&self) -> R<(String, Step)> {
        let mut s = self.atl_btl_step()?;
        s.expand = None;
        Ok(("Total above and below-the-line".into(), s))
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
            "Total fringes",
            "every fringe in the budget, outside above and below-the-line (fringe posting: by budget)",
            children,
        )?
        .with_expand(Target::BudgetFringes))
    }

    fn budget_fringes(&self) -> R<(String, Step)> {
        let mut s = self.budget_fringes_step()?;
        s.expand = None;
        s.children.push(self.posting_note());
        Ok(("Total fringes".into(), s))
    }

    fn grand_total(&self) -> R<(String, Step)> {
        let mut children = vec![self.atl_btl_step()?];
        if self.b.fringe_posting == FringePosting::Budget {
            children.push(self.budget_fringes_step()?);
        }
        for i in 0..self.report.charges.len() {
            children.push(self.charge_step(i)?);
        }
        let s = Step::sum(
            "Grand total",
            "above and below-the-line, plus budget fringes and charges",
            children,
        )?;
        Ok(("Grand total".into(), s))
    }

    fn credits_total_step(&self) -> R<Step> {
        let children = (0..self.report.credits.len())
            .map(|i| self.credit_step(i))
            .collect::<R<Vec<_>>>()?;
        Ok(Step::sum("Credits", "every applied credit", children)?
            .with_expand(Target::CreditsTotal))
    }

    fn credits_total(&self) -> R<(String, Step)> {
        let mut s = self.credits_total_step()?;
        s.expand = None;
        Ok(("Credits".into(), s))
    }

    fn net_total(&self) -> R<(String, Step)> {
        let (_, mut grand) = self.grand_total()?;
        grand.expand = Some(Target::GrandTotal);
        let mut credits = self.credits_total_step()?;
        credits.minus = true;
        credits.label = "Less credits".into();
        let s = Step::sum(
            "Net total",
            "grand total less credits",
            vec![grand, credits],
        )?;
        Ok(("Net total".into(), s))
    }

    fn fringe(&self, name: &str) -> R<(String, Step)> {
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
                            format!("{name} on the account's lines"),
                            lines,
                        )?
                        .with_link(Link::Account { ci, ai }),
                    );
                }
            }
        }
        let kind = match &f.kind {
            FringeKind::Percent { cutoff: Some(c) } => {
                format!("a percent fringe with a {} cutoff per earner", money(*c))
            }
            FringeKind::Percent { cutoff: None } => "a percent fringe, no cutoff".into(),
            FringeKind::Flat { .. } => "a flat fringe".into(),
        };
        let s = Step::sum(name, format!("{kind}, over every counted line"), accounts)?;
        Ok((format!("Fringe {name}: total"), s))
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

    fn atl_btl_total(&self) -> R<Cents> {
        self.atl_btl_step()?.amount.ok_or_else(ovf)
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
        let by_code = matches!(c.kind, ChargeKind::Percent(_));
        let budget_posting = self.b.fringe_posting == FringePosting::Budget;
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
            if by_code {
                if c.excluded_accounts.contains(acct) {
                    why.insert(format!("account {acct}"));
                }
                if c.excluded_categories.contains(cat) {
                    why.insert(format!("category {cat}"));
                }
            }
            if why.is_empty() {
                continue;
            }
            let v = if budget_posting {
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
                "Excluded amount",
                if desc.trim().is_empty() {
                    "typed in".to_string()
                } else {
                    desc.trim().to_string()
                },
                *amt,
            ));
        }
        for ((ci, ai), (amt, why)) in &per_account {
            excluded = excluded.checked_add(*amt).ok_or_else(ovf)?;
            let what = if budget_posting {
                "its lines"
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
                let base = self.atl_btl_total()?;
                notes.push(
                    Step::note_amount(
                        "Base",
                        "total above and below-the-line (charges never apply to each other)",
                        base,
                    )
                    .with_expand(Target::AtlBtl),
                );
                notes.push(
                    Step::note_amount("Excluded", "taken off the base", excluded).with(ex_children),
                );
                let net = base.checked_sub(excluded).ok_or_else(ovf)?;
                let exact = Exact::cents(net)
                    .checked_mul(*p)
                    .map(Exact::percent)
                    .ok_or_else(ovf)?;
                let amount = percent_of(net, *p).ok_or_else(ovf)?;
                notes.push(Step::note(
                    "Charge",
                    format!(
                        "{}% of ({} - {}) = {}",
                        num(*p),
                        money(base),
                        money(excluded),
                        dmoney(exact)
                    ),
                ));
                notes.extend(rounding(exact, amount));
                (amount, format!("{}% of the budget", num(*p)))
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
                        let exact = Exact::cents(*amount).checked_mul(rate).ok_or_else(ovf)?;
                        let conv = mul_cents(*amount, rate).ok_or_else(ovf)?;
                        notes.push(Step::note(
                            "Fee",
                            format!(
                                "{} {cur} x {} {} per {cur} = {}",
                                money(*amount),
                                num(rate),
                                self.b.base_currency,
                                dmoney(exact)
                            ),
                        ));
                        notes.extend(rounding(exact, conv));
                        conv
                    }
                    _ => {
                        notes.push(Step::note_amount("Fee", "a flat fee", *amount));
                        *amount
                    }
                };
                let ex_text =
                    "taken off the fee (flat fees leave out groups and typed amounts only)";
                notes.push(Step::note_amount("Excluded", ex_text, excluded).with(ex_children));
                let amount = base.checked_sub(excluded).ok_or_else(ovf)?;
                notes.push(Step::note(
                    "Charge",
                    format!("{} - {} = {}", money(base), money(excluded), money(amount)),
                ));
                (amount, "a flat fee".to_string())
            }
        };
        Ok(Step::value(&c.name, label_text, amount)
            .with(notes)
            .with_expand(Target::Charge(i)))
    }

    fn charge(&self, i: usize) -> R<(String, Step)> {
        let c = self.applied_charge(i)?;
        let mut s = self.charge_step(i)?;
        s.expand = None;
        Ok((format!("Charge {}", c.name), s))
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
        let mut notes = Vec::new();
        let mut total = Exact::cents(0);
        for (k, cr) in c.rows.iter().enumerate() {
            let mut row_notes = Vec::new();
            let (gross, excluded) = match &cr.source {
                CreditSource::Flat(g) => {
                    row_notes.push(Step::note_amount("Gross", "typed in", *g));
                    (*g, 0)
                }
                CreditSource::Group {
                    group,
                    excluded_fringes,
                } => {
                    let mut gross: Cents = 0;
                    let mut ex: Cents = 0;
                    let mut per_account: BTreeMap<(usize, usize), (Cents, Cents)> = BTreeMap::new();
                    for (ci, ai, _, _, d, r) in self.costed() {
                        if !r.included || !d.groups.contains(group) {
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
                                format!("counted lines tagged {group}, with their fringes"),
                                v,
                            )
                            .with_link(Link::Account { ci, ai })
                        })
                        .collect();
                    row_notes.push(
                        Step::note_amount(
                            "Gross",
                            format!("lines tagged {group} plus their fringes"),
                            gross,
                        )
                        .with(lines),
                    );
                    if !excluded_fringes.is_empty() {
                        let names = excluded_fringes.iter().cloned().collect::<Vec<_>>();
                        row_notes.push(Step::note_amount("Excluded fringes", names.join(", "), ex));
                    }
                    (gross, ex)
                }
            };
            let net = gross.checked_sub(excluded).ok_or_else(ovf)?;
            row_notes.push(Step::note_amount(
                "Net spend",
                format!("{} - {}", money(gross), money(excluded)),
                net,
            ));
            // The engine's own exact working, so the amounts agree.
            let (credit, _) = credit_row(net, cr.percent, cr.cap).ok_or_else(ovf)?;
            let pct = |v: Exact| v.checked_mul(cr.percent).map(Exact::percent);
            let net_x = Exact::cents(net);
            match cr.cap {
                CreditCap::None => {
                    row_notes.push(Step::note(
                        "Credit",
                        format!(
                            "{}% of {} = {}",
                            num(cr.percent),
                            money(net),
                            dmoney(credit)
                        ),
                    ));
                }
                CreditCap::QualifiedSpend(cap) => {
                    let spend = net_x.checked_min(Exact::cents(cap)).ok_or_else(ovf)?;
                    row_notes.push(Step::note(
                        "Cap on spend",
                        format!(
                            "the spend the percent applies to is at most {}: {}",
                            money(cap),
                            dmoney(spend)
                        ),
                    ));
                    row_notes.push(Step::note(
                        "Credit",
                        format!(
                            "{}% of {} = {}",
                            num(cr.percent),
                            dmoney(spend),
                            dmoney(credit)
                        ),
                    ));
                }
                CreditCap::Credit(cap) => {
                    let uncapped = pct(net_x).ok_or_else(ovf)?;
                    row_notes.push(Step::note(
                        "Credit",
                        format!(
                            "{}% of {} = {}, capped at {}: {}",
                            num(cr.percent),
                            money(net),
                            dmoney(uncapped),
                            money(cap),
                            dmoney(credit)
                        ),
                    ));
                }
            }
            row_notes.push(Step::note(
                "Not rounded yet",
                "rows are added, capped and discounted before the one rounding",
            ));
            total = total.checked_add(credit).ok_or_else(ovf)?;
            let label = if cr.description.trim().is_empty() {
                format!("Row {}", k + 1)
            } else {
                cr.description.trim().to_string()
            };
            notes.push(Step::note(label, dmoney(credit)).with(row_notes));
        }
        notes.push(Step::note("Rows added", dmoney(total)));
        let capped = credit_cap(total, c.cap).ok_or_else(ovf)?;
        if let Some(cap) = c.cap {
            notes.push(Step::note(
                "Credit cap",
                format!("at most {}: {}", money(cap), dmoney(capped)),
            ));
        }
        let keep = credit_keep(c.discount_percent).ok_or_else(ovf)?;
        let exact = capped.checked_mul_exact(keep).ok_or_else(ovf)?;
        if !c.discount_percent.is_zero() {
            notes.push(Step::note(
                "Discount",
                format!(
                    "less {}%: {} x {} = {}",
                    num(c.discount_percent),
                    dmoney(capped),
                    keep,
                    dmoney(exact)
                ),
            ));
        }
        let amount = exact.to_cents().ok_or_else(ovf)?;
        notes.extend(rounding(exact, amount));
        Ok(Step::value(&c.name, "an applied credit", amount)
            .with(notes)
            .with_expand(Target::Credit(i)))
    }

    fn credit(&self, i: usize) -> R<(String, Step)> {
        let c = self.applied_credit(i)?;
        let mut s = self.credit_step(i)?;
        s.expand = None;
        Ok((format!("Credit {}", c.name), s))
    }
}

trait OrText {
    fn or(self, fallback: &str) -> String;
}

impl OrText for String {
    fn or(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

/// "Key grip" or "Line 3" when it has no description.
fn line_label(i: usize, d: &Detail) -> String {
    let t = d.description.trim();
    if t.is_empty() {
        format!("Line {}", i + 1)
    } else {
        t.to_string()
    }
}

/// "2100 line 3 (Key grip)".
fn row_name(code: &str, i: usize, d: &Detail) -> String {
    let t = d.description.trim();
    if t.is_empty() {
        format!("{code} line {}", i + 1)
    } else {
        format!("{code} line {} ({t})", i + 1)
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

/// A formula and its value: `40` or `SHOOT_DAYS + PREP_DAYS = 40
/// (SHOOT_DAYS = 30, PREP_DAYS = 10)`.
fn formula_words(ctx: &Ctx, f: &Formula, value: Dec) -> String {
    let src = f.0.trim();
    if let Ok(v) = src.parse::<Dec>() {
        if v == value {
            return num(value);
        }
    }
    let mut vars = BTreeSet::new();
    if let Ok(e) = Expr::parse(src) {
        e.vars(&mut vars);
    }
    let globals: Vec<String> = vars
        .iter()
        .filter_map(|n| ctx.globals.get(n).map(|v| format!("{n} = {}", num(*v))))
        .collect();
    if globals.is_empty() {
        format!("{src} = {}", num(value))
    } else if globals.len() == 1 && vars.len() == 1 && vars.contains(src) {
        format!("global {}", globals[0])
    } else {
        format!("{src} = {} ({})", num(value), globals.join(", "))
    }
}

/// Why a line is left out, in words.
fn left_out(ctx: &Ctx, d: &Detail) -> String {
    let off: Vec<&str> = d
        .groups
        .iter()
        .filter(|g| ctx.b.groups.get(*g).is_some_and(|x| !x.in_budget_total))
        .map(String::as_str)
        .collect();
    if !off.is_empty() {
        let s = if off.len() == 1 { "" } else { "s" };
        return format!(
            "group{s} {} {} In budget total off, so the line shows but counts as 0",
            off.join(", "),
            if off.len() == 1 { "has" } else { "have" }
        );
    }
    let ex: Vec<&str> = d
        .groups
        .iter()
        .filter(|g| ctx.opts.exclude_groups.contains(*g))
        .map(String::as_str)
        .collect();
    if !ex.is_empty() {
        return format!("the report leaves out group {}", ex.join(", "));
    }
    "the report shows only some groups, and this line has none of them".into()
}

fn account_calc(ctx: &Ctx, ci: usize, ai: usize, code: &str, details: &[Detail]) -> R<AccountCalc> {
    let base = &ctx.b.base_currency;
    let mut rows: Vec<RowCalc> = Vec::with_capacity(details.len());
    let mut since_adjustment: Cents = 0;
    let mut adj_rows: Vec<usize> = Vec::new();
    let mut sub_rows: Vec<usize> = Vec::new();
    let mut quantities: Vec<Dec> = Vec::with_capacity(details.len());
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
                quantities.push(Dec::ZERO);
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
                quantities.push(Dec::ZERO);
                continue;
            }
            RowKind::Line | RowKind::Adjustment => {}
        }

        let amount = ctx.eval(&d.amount, Dec::ZERO, &blank_at)?;
        let x = ctx.eval(&d.x, Dec::ONE, &blank_at)?;
        let x4 = ctx.eval(&d.x4, Dec::ONE, &blank_at)?;
        let quantity = amount
            .checked_mul(x)
            .and_then(|q| q.checked_mul(x4))
            .ok_or_else(ovf)?;
        let included = ctx.includes(d, &blank_at)?;
        let mut notes = Vec::new();

        // Amount, X and 4X.
        let amount_text = if d.amount.is_blank() {
            "blank, counts as 0".to_string()
        } else {
            format!(
                "{}{}",
                formula_words(ctx, &d.amount, amount),
                unit_words(ctx.b, &d.unit, amount)
            )
        };
        notes.push(Step::note("Amount", amount_text));
        if !d.x.is_blank() {
            notes.push(Step::note(
                "X",
                format!(
                    "{}{}",
                    formula_words(ctx, &d.x, x),
                    unit_words(ctx.b, &d.unit2, x)
                ),
            ));
        }
        if !d.x4.is_blank() {
            notes.push(Step::note(
                "4X",
                format!(
                    "{}{}",
                    formula_words(ctx, &d.x4, x4),
                    unit_words(ctx.b, &d.unit4, x4)
                ),
            ));
        }
        if !d.x.is_blank() || !d.x4.is_blank() {
            let mut parts = vec![num(amount)];
            if !d.x.is_blank() {
                parts.push(num(x));
            }
            if !d.x4.is_blank() {
                parts.push(num(x4));
            }
            notes.push(Step::note(
                "Quantity",
                format!("{} = {}", parts.join(" x "), num(quantity)),
            ));
        }

        let total = match d.kind {
            RowKind::Adjustment => {
                let exact = Exact::cents(since_adjustment)
                    .checked_mul(quantity)
                    .ok_or_else(ovf)?;
                let t = mul_cents(since_adjustment, quantity).ok_or_else(ovf)?;
                let pct = quantity.checked_mul(Dec::from_int(100)).ok_or_else(ovf)?;
                let window: Vec<Step> = adj_rows
                    .iter()
                    .map(|&k| {
                        let row = details[k].id;
                        Step::note_amount(line_label(k, &details[k]), "", rows[k].total)
                            .with_link(Link::Row { ci, ai, row })
                    })
                    .collect();
                notes.push(
                    Step::note_amount(
                        "Base",
                        "the lines above, back to the previous adjustment (subtotal rows don't restart it)",
                        since_adjustment,
                    )
                    .with(window),
                );
                notes.push(Step::note(
                    "Adjustment",
                    format!(
                        "{} x {} ({}%) = {}",
                        money(since_adjustment),
                        num(quantity),
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
                    "blank, counts as 0".to_string()
                } else {
                    format!(
                        "{}{}",
                        formula_words(ctx, &d.rate, rate),
                        unit_words(ctx.b, &d.unit3, Dec::ONE)
                    )
                };
                notes.push(Step::note("Rate", rate_text));
                let exact = Exact::dec(quantity).checked_mul(rate).ok_or_else(ovf)?;
                let local = exact.to_cents().ok_or_else(ovf)?;
                let cur_code = match &d.currency {
                    Some(c) if c != base => Some(c),
                    _ => None,
                };
                let cur_words = cur_code.map(|c| format!(" {c}")).unwrap_or_default();
                notes.push(Step::note(
                    "Line",
                    format!(
                        "{} x {} = {}{cur_words}",
                        num(quantity),
                        num(rate),
                        dmoney(exact)
                    ),
                ));
                notes.extend(rounding(exact, local));
                match cur_code {
                    Some(c) => {
                        let cur = ctx
                            .b
                            .currencies
                            .get(c)
                            .ok_or_else(|| gone(&format!("currency {c}")))?;
                        let exact = Exact::cents(local).checked_mul(cur.rate).ok_or_else(ovf)?;
                        let t = mul_cents(local, cur.rate).ok_or_else(ovf)?;
                        notes.push(Step::note(
                            "Currency",
                            format!(
                                "{} {c} x {} {base} per {c} = {} {base}",
                                money(local),
                                num(cur.rate),
                                dmoney(exact)
                            ),
                        ));
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
                "Left out",
                format!(
                    "{}; counted, it would be {}",
                    left_out(ctx, d),
                    money(total)
                ),
                0,
            ));
        }
        if d.kind == RowKind::Adjustment && included {
            since_adjustment = 0;
            adj_rows.clear();
        } else {
            since_adjustment = since_adjustment
                .checked_add(total_counted)
                .ok_or_else(ovf)?;
            adj_rows.push(i);
        }
        sub_rows.push(i);

        let what = if d.kind == RowKind::Adjustment {
            "adjustment"
        } else if included {
            "line"
        } else {
            "line, left out"
        };
        let value = Step::value(
            line_label(i, d),
            format!("{code} line {}, {what}", i + 1),
            total_counted,
        )
        .with(notes)
        .with_link(link)
        .with_expand(Target::Line(d.id));
        quantities.push(quantity);
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

    fringes_calc(ctx, ci, ai, details, &mut rows, &quantities)?;

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
    quantities: &[Dec],
) -> R<()> {
    let blank_at = || String::new();
    // Rates, once per fringe the lines carry.
    let mut rates: BTreeMap<&str, Dec> = BTreeMap::new();
    for d in details {
        if d.kind != RowKind::Line {
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
    let rate_words = |name: &str, rate: Dec| -> String {
        let f = &ctx.b.fringes[name];
        let src = f.rate.0.trim();
        if src.parse::<Dec>().is_ok() {
            String::new()
        } else {
            format!(" (rate {})", formula_words(ctx, &f.rate, rate))
        }
    };
    let earns: Vec<bool> = details
        .iter()
        .zip(rows.iter())
        .map(|(d, r)| d.kind == RowKind::Line && r.included)
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
        // Every line of an earner from a range shares its number.
        let range = details[earner[0]].fringe_range;
        for name in names {
            let f = &ctx.b.fringes[name];
            let FringeKind::Percent { cutoff } = f.kind else {
                continue;
            };
            let rate = rates[name];
            let lines: Vec<usize> = earner
                .iter()
                .copied()
                .filter(|&i| earns[i] && details[i].fringes.contains(name))
                .collect();
            if lines.is_empty() {
                continue;
            }
            let weights: Vec<Cents> = lines.iter().map(|&i| rows[i].total).collect();
            let base = weights
                .iter()
                .try_fold(0, |a: Cents, w| a.checked_add(*w))
                .ok_or_else(ovf)?;
            let capped = match cutoff {
                Some(c) => base.min(c),
                None => base,
            };
            let amount = percent_of(capped, rate).ok_or_else(ovf)?;
            let exact = Exact::cents(capped)
                .checked_mul(rate)
                .map(Exact::percent)
                .ok_or_else(ovf)?;
            let shares = allocate(amount, &weights).ok_or_else(ovf)?;
            let cutoff_use = allocate(capped, &weights).ok_or_else(ovf)?;
            let several = lines.len() > 1;
            for (k, &i) in lines.iter().enumerate() {
                let mut notes = Vec::new();
                if let Some(n) = range {
                    let others: Vec<String> = lines.iter().map(|&j| (j + 1).to_string()).collect();
                    notes.push(Step::note(
                        "Fringe range",
                        format!(
                            "range {n}: lines {} earn as one person, so they share one cutoff",
                            others.join(", ")
                        ),
                    ));
                }
                if several {
                    let parts: Vec<String> = weights.iter().map(|w| money(*w)).collect();
                    notes.push(Step::note_amount(
                        "Earnings",
                        format!("lines in the range carrying {name}: {}", parts.join(" + ")),
                        base,
                    ));
                } else {
                    notes.push(Step::note_amount("Earnings", "this line's total", base));
                }
                match cutoff {
                    Some(c) if base > c => {
                        let mut t = format!("{name} stops at {} per earner", money(c));
                        if several {
                            t.push_str(&format!(
                                "; this line uses {} of it (pro rata, largest remainder)",
                                money(cutoff_use[k])
                            ));
                        }
                        notes.push(Step::note_amount("Cutoff reached", t, capped));
                    }
                    Some(c) => {
                        notes.push(Step::note(
                            "Cutoff",
                            format!("{} per earner, not reached", money(c)),
                        ));
                    }
                    None => {}
                }
                notes.push(Step::note(
                    "Fringe",
                    format!(
                        "{}% of {} = {}{}",
                        num(rate),
                        money(capped),
                        dmoney(exact),
                        rate_words(name, rate)
                    ),
                ));
                notes.extend(rounding(exact, amount));
                if several {
                    let exact_share = Dec::from_cents(amount)
                        .checked_mul(Dec::from_cents(weights[k]))
                        .and_then(|v| v.checked_div(Dec::from_cents(base)));
                    let text = match exact_share {
                        Some(s) if base != 0 => format!(
                            "{} x {} / {} = {}, which is {} once the shares are split by largest remainder (they add up to exactly {})",
                            money(amount),
                            money(weights[k]),
                            money(base),
                            dmoney(s),
                            money(shares[k]),
                            money(amount)
                        ),
                        _ => format!(
                            "{} of the range's {} (the earnings net to zero, so the first line takes it)",
                            money(shares[k]),
                            money(amount)
                        ),
                    };
                    notes.push(Step::note("This line's share", text));
                }
                let kind_text = match cutoff {
                    Some(c) => format!("{}% with a {} cutoff", num(rate), money(c)),
                    None => format!("{}%", num(rate)),
                };
                let s = Step::value(name, kind_text, shares[k])
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

    // Flat fringes, per line.
    let hours = |u: &str| ctx.b.units.get(u).and_then(|def| def.hours);
    for (i, d) in details.iter().enumerate() {
        if !earns[i] {
            continue;
        }
        for name in &d.fringes {
            let f = &ctx.b.fringes[name];
            let FringeKind::Flat { unit } = &f.kind else {
                continue;
            };
            let rate = rates[name.as_str()];
            let q = quantities[i];
            let mut notes = Vec::new();
            let fh = unit.as_deref().and_then(hours);
            let lh = hours(&d.unit);
            let (amount, text) = match (fh, lh) {
                (Some(fh), Some(lh)) if !fh.is_zero() => {
                    // Rounded once from the exact amount, as the engine does.
                    // The division need not end, so the working shows it
                    // to 9 places.
                    let product = Exact::dec(rate)
                        .checked_mul(q)
                        .and_then(|v| v.checked_mul(lh))
                        .ok_or_else(ovf)?;
                    let amount = product.div_to_cents(fh).ok_or_else(ovf)?;
                    let exact = product.div_to_dec(fh).ok_or_else(ovf)?;
                    let fu = unit.as_deref().unwrap_or_default();
                    notes.push(Step::note(
                        "Fringe",
                        format!(
                            "{} per {fu} x {} (quantity) x {} hours per {} / {} hours per {fu} = {}{}",
                            dmoney(rate),
                            num(q),
                            num(lh),
                            d.unit.trim(),
                            num(fh),
                            dmoney(exact),
                            rate_words(name, rate)
                        ),
                    ));
                    notes.extend(rounding(exact, amount));
                    (amount, format!("flat, per {fu}"))
                }
                _ => {
                    let amount = rate.to_cents().ok_or_else(ovf)?;
                    let why = match unit.as_deref() {
                        Some(u) if !u.trim().is_empty() => format!(
                            "{} once on this line: it is per {u}, but {} hours to convert by",
                            dmoney(rate),
                            if fh.is_none() || fh.is_some_and(Dec::is_zero) {
                                format!("{u} has no")
                            } else {
                                format!("the line's unit `{}` has no", d.unit.trim())
                            }
                        ),
                        _ => format!("{} once on this line", dmoney(rate)),
                    };
                    notes.push(Step::note(
                        "Fringe",
                        format!("{why}{}", rate_words(name, rate)),
                    ));
                    notes.extend(rounding(rate, amount));
                    (amount, "flat, once per line".to_string())
                }
            };
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
        assert_eq!(dmoney("1296.315".parse::<Dec>().unwrap()), "1,296.315");
        assert_eq!(dmoney("-54000".parse::<Dec>().unwrap()), "-54,000.00");
        assert_eq!(dmoney("0.5".parse::<Dec>().unwrap()), "0.50");
        assert_eq!(dmoney(Exact::new(49_999_995, 10)), "0.0049999995");
        assert_eq!(dmoney(Exact::new(-5_400_000, 2)), "-54,000.00");
        assert_eq!(num("1234567.25".parse().unwrap()), "1,234,567.25");
        assert_eq!(num("-12".parse().unwrap()), "-12");
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
