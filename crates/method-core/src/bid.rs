//! Bid layouts: a summary of the budget the producer lays out themselves, for
//! commercial work (roadmap 3.13; decisions.md, entry 92).
//!
//! A [`BidLayout`] is part of a report preset ([`crate::ReportSpec::bid`]),
//! printed by the `bid` section. Everything in it is the user's: header
//! fields with their own labels, named sections that take accounts by
//! category, by account or by a range of account numbers, running
//! subtotals, and markups (a percent of chosen sections, or a flat amount).
//! Like every report it holds no figures: [`bid_sheet`] works the bid out
//! from the budget's totals each time it is drawn.
//!
//! **How a bid reconciles.** Every account counts once: in the first section
//! (in layout order) that takes it, or in the Unassigned bucket, which is
//! always shown. An account counts at its lines plus the fringes they earn,
//! wherever the budget posts them. Below the sections come the budget's own
//! applied charges and credits, so that everything down to there adds up
//! to the budget's net total; the bid's markups are on top of it:
//!
//! ```text
//! bid total = budget net total + the bid's markups
//! ```
//!
//! The only other row is for fringes the budget earns but doesn't post
//! (fringes posted by production level with no production total after the
//! last categories, `trailing_level_fringes` drop): they take that amount
//! back off, shown on its own line, so the rule above still holds.
//!
//! Display data only: nothing here ever changes a budget total.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::decimal::{Dec, Exact};
use crate::model::Budget;
use crate::report::Report;
use crate::Cents;

/// Most header fields, and most lines, a layout keeps.
pub const MAX_BID_ITEMS: usize = 100;

/// Longest name or label in a layout, in characters.
pub const MAX_BID_NAME_CHARS: usize = 80;

/// Longest header field value, in characters.
pub const MAX_BID_VALUE_CHARS: usize = 200;

/// A bid layout: header fields, then the lines of the bid in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BidLayout {
    /// Printed above the bid, in order: each a label the user names
    /// ("Client") and what they fill in.
    pub fields: Vec<BidField>,
    /// The bid's lines, in print order.
    pub lines: Vec<BidLine>,
    /// List each account under its section, not only the section's total.
    pub show_accounts: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BidField {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BidLine {
    /// A section and the accounts it takes.
    Section { name: String, picks: Vec<BidPick> },
    /// Everything above it in the layout added up: sections, markups and
    /// earlier subtotals' parts (a running total, never counted twice).
    Subtotal { name: String },
    /// `percent` of the sections named in `of` (none named: every section
    /// and the Unassigned bucket). Names match ignoring case and
    /// surrounding spaces.
    Percent {
        name: String,
        percent: Dec,
        of: Vec<String>,
    },
    /// A fixed amount.
    Flat { name: String, amount: Cents },
}

impl BidLine {
    pub fn name(&self) -> &str {
        match self {
            BidLine::Section { name, .. }
            | BidLine::Subtotal { name }
            | BidLine::Percent { name, .. }
            | BidLine::Flat { name, .. } => name,
        }
    }
}

/// Which accounts a section takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BidPick {
    /// Every account in the category with this code.
    Category(String),
    /// The account with this code.
    Account(String),
    /// Every account whose code is from `from` to `to`, both included, in
    /// [`code_order`].
    Range { from: String, to: String },
}

impl BidPick {
    fn takes(&self, category: &str, account: &str) -> bool {
        match self {
            BidPick::Category(c) => c.trim() == category,
            BidPick::Account(a) => a.trim() == account,
            BidPick::Range { from, to } => {
                code_order(from.trim(), account).is_le() && code_order(account, to.trim()).is_le()
            }
        }
    }
}

/// How account codes sort for a range: runs of digits by their value
/// (`900` before `1000`, `0900` the same as `900` but before it), the rest
/// character by character. So `1000-1999` takes `1100` and `1100-01`, and a
/// coded chart (`11-02`) works too.
pub fn code_order(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) if p.is_ascii_digit() && q.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(c);
                        it.next();
                    }
                    s
                };
                let (m, n) = (take(&mut x), take(&mut y));
                let (mt, nt) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let o = mt.len().cmp(&nt.len()).then_with(|| mt.cmp(nt));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(p), Some(q)) => {
                if p != q {
                    return p.cmp(&q);
                }
                x.next();
                y.next();
            }
        }
    }
}

impl BidLayout {
    /// The layout a new bid starts from: an example the project wrote for
    /// itself, plain and generic, to be renamed and filled in. Its sections
    /// take no accounts yet, so everything starts in Unassigned.
    pub fn example() -> BidLayout {
        let field = |label: &str| BidField {
            label: label.into(),
            value: String::new(),
        };
        let section = |name: &str| BidLine::Section {
            name: name.into(),
            picks: Vec::new(),
        };
        BidLayout {
            fields: vec![
                field("Client"),
                field("Agency"),
                field("Job number"),
                field("Dates"),
                field("Contact"),
            ],
            lines: vec![
                section("Pre-production"),
                section("Shoot"),
                section("Post"),
                BidLine::Subtotal {
                    name: "Production subtotal".into(),
                },
                BidLine::Percent {
                    name: "Markup".into(),
                    percent: Dec::from_int(10),
                    of: vec!["Pre-production".into(), "Shoot".into(), "Post".into()],
                },
                section("Fees"),
            ],
            show_accounts: false,
        }
    }

    /// Checks what the application requires of a layout it saves: names and
    /// labels of 1 to [`MAX_BID_NAME_CHARS`] characters (not just spaces),
    /// values up to [`MAX_BID_VALUE_CHARS`], section names used once
    /// (ignoring case and surrounding spaces), at most [`MAX_BID_ITEMS`]
    /// fields and lines. The file reader is more lenient.
    pub fn validate(&self) -> Result<(), BidError> {
        if self.fields.len() > MAX_BID_ITEMS || self.lines.len() > MAX_BID_ITEMS {
            return Err(BidError::TooMany);
        }
        let name_ok = |s: &str| !s.trim().is_empty() && s.chars().count() <= MAX_BID_NAME_CHARS;
        for f in &self.fields {
            if !name_ok(&f.label) {
                return Err(BidError::BadName(f.label.clone()));
            }
            if f.value.chars().count() > MAX_BID_VALUE_CHARS {
                return Err(BidError::ValueTooLong(f.label.clone()));
            }
        }
        let mut seen = BTreeSet::new();
        for l in &self.lines {
            if !name_ok(l.name()) {
                return Err(BidError::BadName(l.name().to_string()));
            }
            if let BidLine::Section { name, .. } = l {
                if !seen.insert(key(name)) {
                    return Err(BidError::RepeatedSection(name.trim().to_string()));
                }
            }
        }
        Ok(())
    }

    /// Every account and category code the layout names, for renumbering.
    pub fn codes_mut(&mut self) -> impl Iterator<Item = (bool, &mut String)> {
        self.lines.iter_mut().flat_map(|l| match l {
            BidLine::Section { picks, .. } => picks
                .iter_mut()
                .flat_map(|p| match p {
                    BidPick::Category(c) => vec![(true, c)],
                    BidPick::Account(a) => vec![(false, a)],
                    BidPick::Range { from, to } => vec![(false, from), (false, to)],
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
    }
}

fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Why a layout can't be saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BidError {
    TooMany,
    BadName(String),
    ValueTooLong(String),
    RepeatedSection(String),
}

impl std::fmt::Display for BidError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BidError::TooMany => write!(
                f,
                "a bid layout can have at most {MAX_BID_ITEMS} header fields and {MAX_BID_ITEMS} lines"
            ),
            BidError::BadName(n) if n.trim().is_empty() => {
                f.write_str("every bid line and header field needs a name")
            }
            BidError::BadName(_) => write!(
                f,
                "bid names and labels can be at most {MAX_BID_NAME_CHARS} characters"
            ),
            BidError::ValueTooLong(l) => write!(
                f,
                "the bid's \"{l}\" can be at most {MAX_BID_VALUE_CHARS} characters"
            ),
            BidError::RepeatedSection(n) => {
                write!(f, "the bid has two sections called \"{n}\"")
            }
        }
    }
}

impl std::error::Error for BidError {}

// ---------------------------------------------------------------------------
// The worked-out bid

/// A bid worked out from a budget: what a report prints.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BidSheet {
    /// The header fields, as the layout has them.
    pub fields: Vec<BidField>,
    /// The layout's lines with their figures, in order.
    pub lines: Vec<BidSheetLine>,
    /// Accounts no section takes, in budget order.
    pub unassigned: Vec<BidAccount>,
    pub unassigned_total: Cents,
    /// Fringes earned but not posted by the budget, as a negative amount
    /// (0 almost always; see the module's notes).
    pub unposted_fringes: Cents,
    /// The budget's applied charges, newest first as the topsheet shows them.
    pub charges: Vec<(String, Cents)>,
    /// The budget's applied credits, as positive amounts.
    pub credits: Vec<(String, Cents)>,
    /// The budget's net total.
    pub budget_net: Cents,
    /// The bid's markups added up (percent and flat).
    pub markups: Cents,
    /// `budget_net + markups`.
    pub total: Cents,
    /// Things the user may want to fix: an account two sections take, a
    /// pick that takes nothing, a markup on a section that doesn't exist.
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BidSheetLine {
    Section {
        name: String,
        accounts: Vec<BidAccount>,
        total: Cents,
    },
    Subtotal {
        name: String,
        amount: Cents,
    },
    /// A percent markup (`percent` set, `base` what it is a percent of) or
    /// a flat one.
    Markup {
        name: String,
        percent: Option<Dec>,
        base: Cents,
        amount: Cents,
    },
}

impl BidSheetLine {
    pub fn amount(&self) -> Cents {
        match self {
            BidSheetLine::Section { total, .. } => *total,
            BidSheetLine::Subtotal { amount, .. } | BidSheetLine::Markup { amount, .. } => *amount,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BidAccount {
    pub category: String,
    pub code: String,
    pub name: String,
    /// Its lines plus the fringes they earn, wherever those post.
    pub amount: Cents,
}

/// Why a bid can't be worked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BidOverflow;

impl std::fmt::Display for BidOverflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the bid's figures are too large to add up")
    }
}

impl std::error::Error for BidOverflow {}

fn add(a: Cents, b: Cents) -> Result<Cents, BidOverflow> {
    a.checked_add(b).ok_or(BidOverflow)
}

/// Works out `layout` for budget `b`, whose totals are `r`
/// (`recalc(b, ..)`).
pub fn bid_sheet(b: &Budget, r: &Report, layout: &BidLayout) -> Result<BidSheet, BidOverflow> {
    let mut warnings = Vec::new();
    // Which section takes each account: the first that names it.
    let sections: Vec<(usize, &str, &[BidPick])> = layout
        .lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| match l {
            BidLine::Section { name, picks } => Some((i, name.as_str(), picks.as_slice())),
            _ => None,
        })
        .collect();
    let mut taken: Vec<Vec<BidAccount>> = vec![Vec::new(); layout.lines.len()];
    let mut used: Vec<Vec<bool>> = sections.iter().map(|s| vec![false; s.2.len()]).collect();
    let mut unassigned = Vec::new();
    let mut every_account: Cents = 0;
    for (cat, rc) in b.categories().zip(&r.categories) {
        for (a, ra) in cat.accounts.iter().zip(&rc.accounts) {
            let amount = add(ra.details_total, ra.fringe_total)?;
            every_account = add(every_account, amount)?;
            let acct = BidAccount {
                category: cat.code.clone(),
                code: a.code.clone(),
                name: a.name.clone(),
                amount,
            };
            let mut owners = Vec::new();
            for (si, (_, name, picks)) in sections.iter().enumerate() {
                let mut hit = false;
                for (pi, p) in picks.iter().enumerate() {
                    if p.takes(&cat.code, &a.code) {
                        used[si][pi] = true;
                        hit = true;
                    }
                }
                if hit {
                    owners.push((si, *name));
                }
            }
            match owners.first() {
                Some(&(si, first)) => {
                    if owners.len() > 1 {
                        let others: Vec<String> =
                            owners[1..].iter().map(|o| format!("\"{}\"", o.1)).collect();
                        warnings.push(format!(
                            "account {} is in \"{first}\" and {}; it counts in \"{first}\" only",
                            a.code,
                            others.join(", ")
                        ));
                    }
                    taken[sections[si].0].push(acct);
                }
                None => unassigned.push(acct),
            }
        }
    }
    for (si, (_, name, picks)) in sections.iter().enumerate() {
        for (pi, p) in picks.iter().enumerate() {
            if !used[si][pi] {
                let what = match p {
                    BidPick::Category(c) => format!("category {c}"),
                    BidPick::Account(a) => format!("account {a}"),
                    BidPick::Range { from, to } => format!("accounts {from} to {to}"),
                };
                warnings.push(format!("\"{name}\" takes {what}, but the budget has none"));
            }
        }
    }

    let unassigned_total = unassigned
        .iter()
        .try_fold(0, |s: Cents, a| add(s, a.amount))?;
    let section_totals: Vec<(String, Cents)> = sections
        .iter()
        .map(|&(i, name, _)| {
            taken[i]
                .iter()
                .try_fold(0, |s: Cents, a| add(s, a.amount))
                .map(|t| (key(name), t))
        })
        .collect::<Result<_, _>>()?;

    let mut lines = Vec::new();
    let mut running: Cents = 0;
    let mut markups: Cents = 0;
    for (i, l) in layout.lines.iter().enumerate() {
        let line = match l {
            BidLine::Section { name, .. } => {
                let accounts = std::mem::take(&mut taken[i]);
                let total = accounts
                    .iter()
                    .try_fold(0, |s: Cents, a| add(s, a.amount))?;
                BidSheetLine::Section {
                    name: name.clone(),
                    accounts,
                    total,
                }
            }
            BidLine::Subtotal { name } => BidSheetLine::Subtotal {
                name: name.clone(),
                amount: running,
            },
            BidLine::Percent { name, percent, of } => {
                let base = if of.is_empty() {
                    let all = section_totals
                        .iter()
                        .try_fold(0, |s: Cents, t| add(s, t.1))?;
                    add(all, unassigned_total)?
                } else {
                    let mut base = 0;
                    let wanted: BTreeSet<String> = of.iter().map(|n| key(n)).collect();
                    for w in &wanted {
                        if !section_totals.iter().any(|t| &t.0 == w) {
                            warnings.push(format!(
                                "\"{name}\" is a percent of \"{}\", which isn't a section of this bid",
                                of.iter().find(|n| &key(n) == w).map_or("", |n| n.trim())
                            ));
                        }
                    }
                    for (k, t) in &section_totals {
                        if wanted.contains(k) {
                            base = add(base, *t)?;
                        }
                    }
                    base
                };
                let amount = Exact::cents(base)
                    .checked_mul(*percent)
                    .map(Exact::percent)
                    .and_then(Exact::to_cents)
                    .ok_or(BidOverflow)?;
                markups = add(markups, amount)?;
                BidSheetLine::Markup {
                    name: name.clone(),
                    percent: Some(*percent),
                    base,
                    amount,
                }
            }
            BidLine::Flat { name, amount } => {
                markups = add(markups, *amount)?;
                BidSheetLine::Markup {
                    name: name.clone(),
                    percent: None,
                    base: 0,
                    amount: *amount,
                }
            }
        };
        if !matches!(line, BidSheetLine::Subtotal { .. }) {
            running = add(running, line.amount())?;
        }
        lines.push(line);
    }

    let charges_total = r
        .charges
        .iter()
        .try_fold(0, |s: Cents, c| add(s, c.amount))?;
    let before_charges = r
        .grand_total
        .checked_sub(charges_total)
        .ok_or(BidOverflow)?;
    let unposted_fringes = before_charges
        .checked_sub(every_account)
        .ok_or(BidOverflow)?;
    let total = add(r.net_total, markups)?;
    Ok(BidSheet {
        fields: layout.fields.clone(),
        lines,
        unassigned,
        unassigned_total,
        unposted_fringes,
        charges: r
            .charges
            .iter()
            .rev()
            .map(|c| (c.name.clone(), c.amount))
            .collect(),
        credits: r
            .credits
            .iter()
            .rev()
            .map(|c| (c.name.clone(), c.amount))
            .collect(),
        budget_net: r.net_total,
        markups,
        total,
        warnings,
    })
}

#[cfg(test)]
// Cents written as dollars_cents.
#[allow(clippy::inconsistent_digit_grouping)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::{recalc, RecalcOptions};

    fn d(s: &str) -> Dec {
        s.parse().unwrap()
    }

    fn line(id: u64, rate: &str) -> Detail {
        Detail::line(id, "line", "1", "Flat", rate)
    }

    fn acct(code: &str, rate: &str, id: u64) -> Account {
        Account {
            code: code.into(),
            name: format!("Account {code}"),
            details: vec![line(id, rate)],
            notes: Vec::new(),
        }
    }

    fn cat(code: &str, accounts: Vec<Account>) -> TopsheetRow {
        TopsheetRow::Category(Category {
            code: code.into(),
            name: format!("Category {code}"),
            accounts,
            notes: Vec::new(),
        })
    }

    /// 1000: 1100 (100.00), 1200 (200.00); 2000: 2100 (300.00), 2200
    /// (400.00); 3000: 3100 (500.00).
    fn budget() -> Budget {
        let mut b = Budget::new("Spot", "USD");
        b.topsheet = vec![
            cat("1000", vec![acct("1100", "100", 1), acct("1200", "200", 2)]),
            cat("2000", vec![acct("2100", "300", 3), acct("2200", "400", 4)]),
            cat("3000", vec![acct("3100", "500", 5)]),
        ];
        b
    }

    fn section(name: &str, picks: Vec<BidPick>) -> BidLine {
        BidLine::Section {
            name: name.into(),
            picks,
        }
    }

    fn sheet(b: &Budget, l: &BidLayout) -> BidSheet {
        let r = recalc(b, &RecalcOptions::default()).unwrap();
        bid_sheet(b, &r, l).unwrap()
    }

    fn totals(s: &BidSheet) -> Vec<(String, Cents)> {
        s.lines
            .iter()
            .map(|l| {
                let name = match l {
                    BidSheetLine::Section { name, .. }
                    | BidSheetLine::Subtotal { name, .. }
                    | BidSheetLine::Markup { name, .. } => name.clone(),
                };
                (name, l.amount())
            })
            .collect()
    }

    #[test]
    fn codes_sort_by_number() {
        assert_eq!(code_order("900", "1000"), Ordering::Less);
        assert_eq!(code_order("1100", "1100-01"), Ordering::Less);
        assert_eq!(code_order("1100-01", "1999"), Ordering::Less);
        assert_eq!(code_order("11-02", "11-10"), Ordering::Less);
        assert_eq!(code_order("0900", "900"), Ordering::Less);
        assert_eq!(code_order("A100", "B1"), Ordering::Less);
        assert_eq!(code_order("2000", "2000"), Ordering::Equal);
        let r = BidPick::Range {
            from: "1000".into(),
            to: "1999".into(),
        };
        assert!(r.takes("10", "1100-01"));
        assert!(r.takes("10", "1999"));
        assert!(!r.takes("10", "2000"));
        assert!(!r.takes("10", "999"));
    }

    #[test]
    fn accounts_land_in_one_section_and_the_rest_are_unassigned() {
        let b = budget();
        let l = BidLayout {
            lines: vec![
                section("Prep", vec![BidPick::Category("1000".into())]),
                section(
                    "Shoot",
                    vec![
                        BidPick::Account("2100".into()),
                        // 1200 is Prep's already: it stays there.
                        BidPick::Range {
                            from: "1200".into(),
                            to: "2099".into(),
                        },
                    ],
                ),
            ],
            ..BidLayout::default()
        };
        let s = sheet(&b, &l);
        assert_eq!(
            totals(&s),
            vec![("Prep".into(), 300_00), ("Shoot".into(), 300_00)]
        );
        let un: Vec<&str> = s.unassigned.iter().map(|a| a.code.as_str()).collect();
        assert_eq!(un, vec!["2200", "3100"]);
        assert_eq!(s.unassigned_total, 900_00);
        assert_eq!(s.budget_net, 1500_00);
        assert_eq!(s.total, 1500_00);
        // Nothing counted twice: sections and Unassigned are the budget.
        assert_eq!(300_00 + 300_00 + s.unassigned_total, s.budget_net);
        assert_eq!(
            s.warnings,
            vec!["account 1200 is in \"Prep\" and \"Shoot\"; it counts in \"Prep\" only"]
        );
    }

    #[test]
    fn markups_on_chosen_sections_and_flat_fees() {
        let b = budget();
        let l = BidLayout {
            lines: vec![
                section("Prep", vec![BidPick::Category("1000".into())]),
                section("Shoot", vec![BidPick::Category("2000".into())]),
                section("Post", vec![BidPick::Category("3000".into())]),
                BidLine::Subtotal {
                    name: "Subtotal".into(),
                },
                BidLine::Percent {
                    name: "Markup".into(),
                    percent: d("12.5"),
                    of: vec![" shoot ".into(), "Post".into()],
                },
                BidLine::Flat {
                    name: "Handling".into(),
                    amount: 250_00,
                },
                BidLine::Percent {
                    name: "Overall".into(),
                    percent: d("1"),
                    of: Vec::new(),
                },
                BidLine::Subtotal {
                    name: "With markups".into(),
                },
            ],
            ..BidLayout::default()
        };
        let s = sheet(&b, &l);
        // 12.5% of 700 + 500 = 150; 1% of everything (1500) = 15.
        assert_eq!(
            totals(&s),
            vec![
                ("Prep".into(), 300_00),
                ("Shoot".into(), 700_00),
                ("Post".into(), 500_00),
                ("Subtotal".into(), 1500_00),
                ("Markup".into(), 150_00),
                ("Handling".into(), 250_00),
                ("Overall".into(), 15_00),
                ("With markups".into(), 1915_00),
            ]
        );
        match &s.lines[4] {
            BidSheetLine::Markup { base, percent, .. } => {
                assert_eq!((*base, *percent), (1200_00, Some(d("12.5"))))
            }
            l => panic!("{l:?}"),
        }
        assert_eq!(s.markups, 415_00);
        assert_eq!(s.total, s.budget_net + 415_00);
        assert!(s.unassigned.is_empty());
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    }

    #[test]
    fn percent_markups_round_once_half_away_from_zero() {
        let mut b = budget();
        b.topsheet = vec![cat("1000", vec![acct("1100", "0.05", 1)])];
        let l = BidLayout {
            lines: vec![
                section("All", vec![BidPick::Category("1000".into())]),
                BidLine::Percent {
                    name: "Half".into(),
                    percent: d("50"),
                    of: vec!["All".into()],
                },
            ],
            ..BidLayout::default()
        };
        // 50% of 0.05 is 0.025: 0.03.
        assert_eq!(sheet(&b, &l).markups, 3);
    }

    #[test]
    fn the_budgets_charges_and_credits_carry_through() {
        let mut b = budget();
        b.charges.push(Charge {
            name: "Contingency".into(),
            account: None,
            applied: true,
            kind: ChargeKind::Percent(d("10")),
            excluded_amounts: Vec::new(),
            excluded_groups: BTreeSet::new(),
            excluded_categories: BTreeSet::new(),
            excluded_accounts: BTreeSet::new(),
            label_percent: false,
            label_excluded: false,
        });
        b.credits.push(Credit {
            name: "Rebate".into(),
            account: None,
            applied: true,
            cap: None,
            discount_percent: Dec::ZERO,
            excluded_fringes: BTreeSet::new(),
            locations: BTreeSet::new(),
            rows: vec![CreditRow {
                description: String::new(),
                source: CreditSource::Flat(100_00),
                percent: Dec::from_int(100),
                cap: CreditCap::None,
            }],
        });
        let s = sheet(&b, &BidLayout::example());
        assert_eq!(s.charges, vec![("Contingency".into(), 150_00)]);
        assert_eq!(s.credits, vec![("Rebate".into(), 100_00)]);
        assert_eq!(s.budget_net, 1500_00 + 150_00 - 100_00);
        // The example takes nothing yet: all of it is Unassigned, and its
        // markup is 10% of nothing.
        assert_eq!(s.unassigned_total, 1500_00);
        assert_eq!(s.total, s.budget_net);
    }

    #[test]
    fn fringes_count_with_their_accounts_wherever_they_post() {
        for posting in [
            FringePosting::Account,
            FringePosting::Budget,
            FringePosting::ProductionLevel,
            FringePosting::Category {
                suffix: "99".into(),
            },
        ] {
            let mut b = budget();
            b.fringes.insert(
                "PCT".into(),
                Fringe {
                    short_code: None,
                    description: String::new(),
                    rate: "10".into(),
                    kind: FringeKind::Percent { cutoff: None },
                },
            );
            b.topsheet
                .push(TopsheetRow::ProductionTotal(ProductionTotal {
                    name: "Total".into(),
                }));
            b.fringe_posting = posting.clone();
            if let TopsheetRow::Category(c) = &mut b.topsheet[0] {
                c.accounts[0].details[0].fringes.insert("PCT".into());
            }
            let l = BidLayout {
                lines: vec![section("Prep", vec![BidPick::Account("1100".into())])],
                ..BidLayout::default()
            };
            let s = sheet(&b, &l);
            assert_eq!(s.lines[0].amount(), 110_00, "{posting:?}");
            assert_eq!(s.unposted_fringes, 0, "{posting:?}");
            assert_eq!(s.lines[0].amount() + s.unassigned_total, s.budget_net);
        }
    }

    #[test]
    fn fringes_the_budget_drops_are_taken_back_off() {
        let mut b = budget();
        b.fringes.insert(
            "PCT".into(),
            Fringe {
                short_code: None,
                description: String::new(),
                rate: "10".into(),
                kind: FringeKind::Percent { cutoff: None },
            },
        );
        b.fringe_posting = FringePosting::ProductionLevel;
        b.trailing_level_fringes = TrailingFringes::Drop;
        if let TopsheetRow::Category(c) = &mut b.topsheet[2] {
            c.accounts[0].details[0].fringes.insert("PCT".into());
        }
        let s = sheet(&b, &BidLayout::example());
        assert_eq!(s.unassigned_total, 1550_00);
        assert_eq!(s.unposted_fringes, -50_00);
        assert_eq!(s.budget_net, 1500_00);
        assert_eq!(s.total, 1500_00);
    }

    #[test]
    fn picks_and_markups_that_find_nothing_are_named() {
        let b = budget();
        let l = BidLayout {
            lines: vec![
                section(
                    "Prep",
                    vec![
                        BidPick::Category("9000".into()),
                        BidPick::Range {
                            from: "5000".into(),
                            to: "5999".into(),
                        },
                    ],
                ),
                BidLine::Percent {
                    name: "Fee".into(),
                    percent: d("5"),
                    of: vec!["Wrap".into()],
                },
            ],
            ..BidLayout::default()
        };
        let s = sheet(&b, &l);
        assert_eq!(
            s.warnings,
            vec![
                "\"Prep\" takes category 9000, but the budget has none",
                "\"Prep\" takes accounts 5000 to 5999, but the budget has none",
                "\"Fee\" is a percent of \"Wrap\", which isn't a section of this bid",
            ]
        );
        assert_eq!(s.markups, 0);
    }

    #[test]
    fn validate() {
        assert_eq!(BidLayout::example().validate(), Ok(()));
        let mut l = BidLayout::example();
        l.lines.push(section(" shoot ", Vec::new()));
        assert_eq!(l.validate(), Err(BidError::RepeatedSection("shoot".into())));
        let mut l = BidLayout::example();
        l.fields[0].label = " ".into();
        assert!(matches!(l.validate(), Err(BidError::BadName(_))));
        let mut l = BidLayout::example();
        l.fields[0].value = "x".repeat(MAX_BID_VALUE_CHARS + 1);
        assert!(matches!(l.validate(), Err(BidError::ValueTooLong(_))));
        let mut l = BidLayout::example();
        l.lines = (0..=MAX_BID_ITEMS)
            .map(|i| BidLine::Subtotal {
                name: format!("S{i}"),
            })
            .collect();
        assert_eq!(l.validate(), Err(BidError::TooMany));
    }
}
