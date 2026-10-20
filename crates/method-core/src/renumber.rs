//! Renumbering the chart of accounts when the account-number pattern changes
//! (roadmap 2.20).
//!
//! Changing a budget's [`AccountCoding`] on its own leaves the numbers
//! already in the budget as they were typed. [`plan`] works out what every
//! category and account number would become under a new pattern, and
//! [`renumber`] rewrites them and every place that names them, then sets the
//! pattern.
//!
//! # How a number is carried over
//!
//! 1. It is split into segments. A number with any allowed separator in it is
//!    split on separators (runs count as one), whatever pattern it was typed
//!    in. A number without one is cut by the current pattern's widths; the
//!    last piece may be short, and characters past the pattern's total width
//!    mean it doesn't follow the pattern (it doesn't fit).
//! 2. Each piece is fitted to the new width of its segment: a short piece is
//!    left-padded with zeros (`11` becomes `011`), a long one loses leading
//!    zeros until it fits (`011` becomes `11`). A piece that still doesn't
//!    fit (`110` in one character), or a non-empty piece for a segment the
//!    new pattern doesn't have, means the number doesn't fit. Nothing is ever
//!    cut off, so two different numbers only become one when they differ by
//!    leading zeros or were spelled differently to begin with.
//! 3. The pieces are joined with the new separator, trailing empty segments
//!    left out, as [`AccountCoding::format`] writes them.
//!
//! A category's own number is its category segment only, carried over the
//! same way; one written with its later parts all zeros (`1100` for
//! category `11`, as studio charts write them) keeps that form. An account's category segment is carried over independently of
//! its category's number, so a mismatch that was there stays there.
//!
//! # Refusals
//!
//! [`renumber`] changes nothing when a number doesn't fit or when two
//! different numbers would become the same one (a [`Conflict`]). We refuse
//! rather than add a suffix: a suffix would invent account numbers the
//! production's accountants never chose. The plan lists every conflict, so
//! the user can fix those numbers first or pick another pattern. Numbers
//! that are already duplicated (two accounts typed `11-01`) are not a new
//! conflict and are carried over together.
//!
//! # What is rewritten
//!
//! Category and account numbers; charges' excluded categories and accounts
//! and the account they post to; credits' accounts and the categories of
//! their category rows; report presets' left-out
//! categories and accounts; and the baseline's category and account keys, so
//! the original and variance columns still line up. A reference to a number
//! that is no longer in the chart is carried over by the same rule when it
//! fits and left as it is when it doesn't; if it would then name a live
//! category or account it never named before, it is dropped from exclusions
//! and the baseline (it matched nothing before, and must not start to).
//! Category fringe accounts are not stored: they are built from the
//! category's number and the suffix, so they follow by themselves.
//!
//! Saved versions keep their own numbers and their own pattern: they are
//! copies of the budget as it was. Comparing them with the renumbered budget
//! still lines up after a separator change, since comparison reads each
//! budget's numbers through that budget's own pattern segment by segment;
//! after a width change (`11` to `011`) the segments differ and the accounts
//! show as added and removed.
//!
//! Property: renumbering to another pattern and back gives the budget it
//! started with, whenever both steps succeed and its numbers were written
//! in the starting pattern (see the tests).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::actuals::normalize_code;
use crate::coding::{AccountCode, AccountCoding, CodingError, SEPARATORS};
use crate::model::{Budget, CreditSource, FringePosting, TopsheetRow};

/// A category or account number before and after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeChange {
    pub old: String,
    pub new: String,
}

/// Which kind of number a change or conflict is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeKind {
    Category,
    Account,
}

impl fmt::Display for CodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CodeKind::Category => "category",
            CodeKind::Account => "account",
        })
    }
}

/// Why a renumbering can't go ahead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conflict {
    /// This number can't be written in the new pattern.
    DoesNotFit {
        kind: CodeKind,
        code: String,
        reason: String,
    },
    /// These different numbers would all become `new`.
    Collapse {
        kind: CodeKind,
        codes: Vec<String>,
        new: String,
    },
}

impl fmt::Display for Conflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Conflict::DoesNotFit { kind, code, reason } => {
                write!(f, "{kind} {code} doesn't fit the new pattern: {reason}")
            }
            Conflict::Collapse { kind, codes, new } => {
                write!(
                    f,
                    "{kind} numbers {} would all become {new}",
                    codes.join(", ")
                )
            }
        }
    }
}

/// What a renumbering would do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenumberPlan {
    /// Every category, in topsheet order (unchanged ones too, `old == new`).
    /// Empty `new` for one that doesn't fit.
    pub categories: Vec<CodeChange>,
    /// Every account, in topsheet order.
    pub accounts: Vec<CodeChange>,
    /// Category fringe accounts (fringes posted by category only): what each
    /// category's fringe account is numbered now and would be.
    pub fringe_accounts: Vec<CodeChange>,
    /// Why it can't go ahead; empty when it can.
    pub conflicts: Vec<Conflict>,
    /// Things worth knowing that don't stop it: a fringe account that would
    /// share its number with an account.
    pub warnings: Vec<String>,
}

impl RenumberPlan {
    /// Whether any category or account number would change.
    pub fn changes_anything(&self) -> bool {
        self.categories
            .iter()
            .chain(&self.accounts)
            .any(|c| c.old != c.new)
    }
}

/// Why [`renumber`] changed nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenumberError {
    /// The new pattern breaks a pattern rule.
    Pattern(CodingError),
    /// Some numbers don't fit or would collapse; the plan says which.
    Conflicts(Box<RenumberPlan>),
}

impl fmt::Display for RenumberError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenumberError::Pattern(e) => write!(f, "{e}"),
            RenumberError::Conflicts(plan) => {
                let n = plan.conflicts.len();
                let first = plan.conflicts.first().map(|c| c.to_string());
                match (n, first) {
                    (1, Some(c)) => write!(f, "can't renumber: {c}"),
                    (_, Some(c)) => write!(f, "can't renumber ({n} problems): {c}, ..."),
                    _ => f.write_str("can't renumber"),
                }
            }
        }
    }
}

impl std::error::Error for RenumberError {}

fn is_separator(c: char) -> bool {
    SEPARATORS.contains(&c)
}

fn widths(c: &AccountCoding) -> [u8; 4] {
    [c.category_width, c.account_width, c.sub_width, c.set_width]
}

/// Splits a stored number into its pieces (rule 1). `Err` says why it
/// doesn't follow `from`.
fn split(from: &AccountCoding, code: &str) -> Result<Vec<String>, String> {
    let code = code.trim();
    if code.is_empty() {
        return Err("it is empty".into());
    }
    if let Some(c) = code
        .chars()
        .find(|c| !c.is_ascii_alphanumeric() && !is_separator(*c))
    {
        return Err(format!("it has the character {c:?}"));
    }
    if code.chars().any(is_separator) {
        let parts: Vec<String> = code
            .split(is_separator)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();
        if parts.is_empty() {
            return Err("it is empty".into());
        }
        return Ok(parts);
    }
    let mut rest = code;
    let mut parts = Vec::new();
    // The used segments in order (a set may follow the account directly).
    for w in widths(from).into_iter().filter(|w| *w > 0) {
        if rest.is_empty() {
            break;
        }
        let n = rest.len().min(usize::from(w));
        let (head, tail) = rest.split_at(n);
        parts.push(head.to_string());
        rest = tail;
    }
    if !rest.is_empty() {
        return Err("it is longer than the pattern it is in now".into());
    }
    Ok(parts)
}

/// Fits one piece to a new width (rule 2).
fn fit(piece: &str, width: u8) -> Result<String, String> {
    let width = usize::from(width);
    if piece.len() <= width {
        return Ok(format!("{piece:0>width$}"));
    }
    let trimmed = piece.trim_start_matches('0');
    let keep = trimmed.len().max(1);
    if keep > width {
        let unit = if width == 1 {
            "character"
        } else {
            "characters"
        };
        return Err(format!("{piece} is too long for {width} {unit}"));
    }
    // Keep enough zeros to fill the width (all of them gone means 0).
    Ok(piece[piece.len() - width..].to_string())
}

/// Carries a number over from `from` to `to`, using at most `max_segments`
/// segments (1 for a category's own number).
fn carry(
    from: &AccountCoding,
    to: &AccountCoding,
    code: &str,
    max_segments: usize,
) -> Result<String, String> {
    let pieces = split(from, code)?;
    let to_widths = widths(to);
    // The segments the new pattern uses, in order: the pieces fill them.
    let slots: Vec<usize> = (0..4).filter(|&i| to_widths[i] > 0).collect();
    let room = slots.len().min(max_segments);
    let mut segs: [String; 4] = Default::default();
    for (i, piece) in pieces.iter().enumerate() {
        if i >= room {
            return Err(if max_segments == 1 {
                "a category number has one part".to_string()
            } else {
                format!("the new pattern has no room for its part {piece}")
            });
        }
        segs[slots[i]] = fit(piece, to_widths[slots[i]])?;
    }
    let [category, account, sub, set] = segs;
    Ok(to.format(&AccountCode {
        category,
        account,
        sub,
        set,
    }))
}

/// The pieces of a category number written with its later parts all zeros
/// (`1100` for category `11`, `11-00`), or `None` for any other number.
fn zero_rest(from: &AccountCoding, code: &str) -> Option<Vec<String>> {
    let pieces = split(from, code).ok()?;
    (pieces.len() > 1 && pieces[1..].iter().all(|p| p.bytes().all(|b| b == b'0'))).then_some(pieces)
}

/// Carries over a category's own number. One written with its later parts
/// all zeros (`1100` for category `11`, as many studio charts write them)
/// keeps that form (`1100` becomes `011000` from 2 + 2 to 3 + 3), or, when
/// the new pattern has no room for it, becomes its category part.
pub fn carry_category(
    from: &AccountCoding,
    to: &AccountCoding,
    code: &str,
) -> Result<String, String> {
    match zero_rest(from, code) {
        Some(pieces) => carry(from, to, code, 4).or_else(|_| carry(from, to, &pieces[0], 1)),
        None => carry(from, to, code, 1),
    }
}

/// Carries over a category's own number as its category part only (`1100`
/// becomes `11` under 2 + 2): how a chart's categories are written into a
/// budget it is applied to.
pub fn carry_category_part(
    from: &AccountCoding,
    to: &AccountCoding,
    code: &str,
) -> Result<String, String> {
    match zero_rest(from, code) {
        Some(pieces) => carry(from, to, &pieces[0], 1),
        None => carry(from, to, code, 1),
    }
}

/// Carries over an account number.
pub fn carry_account(
    from: &AccountCoding,
    to: &AccountCoding,
    code: &str,
) -> Result<String, String> {
    carry(from, to, code, 4)
}

/// Old -> new for every distinct live number of one kind, plus the
/// conflicts among them. `codes` are in display order.
fn map_codes(
    kind: CodeKind,
    codes: &[String],
    convert: impl Fn(&str) -> Result<String, String>,
    changes: &mut Vec<CodeChange>,
    conflicts: &mut Vec<Conflict>,
) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut by_new: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut unfit = BTreeSet::new();
    for code in codes {
        match convert(code) {
            Ok(new) => {
                changes.push(CodeChange {
                    old: code.clone(),
                    new: new.clone(),
                });
                if map.insert(code.clone(), new.clone()).is_none() {
                    by_new.entry(new).or_default().push(code.clone());
                }
            }
            Err(reason) => {
                changes.push(CodeChange {
                    old: code.clone(),
                    new: String::new(),
                });
                if unfit.insert(code.clone()) {
                    conflicts.push(Conflict::DoesNotFit {
                        kind,
                        code: code.clone(),
                        reason,
                    });
                }
            }
        }
    }
    for (new, olds) in by_new {
        if olds.len() > 1 {
            conflicts.push(Conflict::Collapse {
                kind,
                codes: olds,
                new,
            });
        }
    }
    map
}

fn category_codes(b: &Budget) -> Vec<String> {
    b.categories().map(|c| c.code.clone()).collect()
}

fn account_codes(b: &Budget) -> Vec<String> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .map(|a| a.code.clone())
        .collect()
}

/// What renumbering `b` to pattern `to` would do, from the budget's current
/// pattern. Changes nothing. `Err` when `to` breaks a pattern rule.
pub fn plan(b: &Budget, to: &AccountCoding) -> Result<RenumberPlan, CodingError> {
    Ok(plan_with_maps(b, to)?.0)
}

type Maps = (BTreeMap<String, String>, BTreeMap<String, String>);

fn plan_with_maps(b: &Budget, to: &AccountCoding) -> Result<(RenumberPlan, Maps), CodingError> {
    to.validate()?;
    let from = b.account_coding;
    let mut plan = RenumberPlan::default();
    let cats = map_codes(
        CodeKind::Category,
        &category_codes(b),
        |c| carry_category(&from, to, c),
        &mut plan.categories,
        &mut plan.conflicts,
    );
    let accts = map_codes(
        CodeKind::Account,
        &account_codes(b),
        |c| carry_account(&from, to, c),
        &mut plan.accounts,
        &mut plan.conflicts,
    );
    if let FringePosting::Category { suffix } = &b.fringe_posting {
        let live: BTreeSet<&String> = accts.values().collect();
        for c in &plan.categories {
            let old = from.category_fringe_account(&c.old, suffix);
            let new = if c.new.is_empty() {
                String::new()
            } else {
                to.category_fringe_account(&c.new, suffix)
            };
            if old.is_empty() && new.is_empty() {
                continue;
            }
            if !new.is_empty() && live.contains(&new) {
                plan.warnings.push(format!(
                    "category {}'s fringe account would be numbered {new}, like an account in the chart",
                    c.new
                ));
            }
            plan.fringe_accounts.push(CodeChange { old, new });
        }
    }
    Ok((plan, (cats, accts)))
}

/// How a reference to a number is carried over (see the module docs).
enum Carried {
    Keep(String),
    /// It would start naming a live number it never named: drop it from
    /// exclusions and the baseline.
    Clash(String),
}

fn carry_ref(
    code: &str,
    live: &BTreeMap<String, String>,
    live_new: &BTreeSet<String>,
    convert: impl Fn(&str) -> Result<String, String>,
) -> Carried {
    if let Some(new) = live.get(code) {
        return Carried::Keep(new.clone());
    }
    let new = convert(code).unwrap_or_else(|_| code.to_string());
    if live_new.contains(&new) {
        Carried::Clash(new)
    } else {
        Carried::Keep(new)
    }
}

fn carry_set(
    set: &BTreeSet<String>,
    live: &BTreeMap<String, String>,
    live_new: &BTreeSet<String>,
    convert: impl Fn(&str) -> Result<String, String> + Copy,
) -> BTreeSet<String> {
    set.iter()
        .filter_map(|c| match carry_ref(c, live, live_new, convert) {
            Carried::Keep(n) => Some(n),
            Carried::Clash(_) => None,
        })
        .collect()
}

fn carry_label(
    code: &Option<String>,
    live: &BTreeMap<String, String>,
    live_new: &BTreeSet<String>,
    convert: impl Fn(&str) -> Result<String, String>,
) -> Option<String> {
    code.as_ref()
        .map(|c| match carry_ref(c, live, live_new, convert) {
            Carried::Keep(n) | Carried::Clash(n) => n,
        })
}

fn carry_keys<V: Copy>(
    map: &BTreeMap<String, V>,
    live: &BTreeMap<String, String>,
    live_new: &BTreeSet<String>,
    convert: impl Fn(&str) -> Result<String, String> + Copy,
) -> BTreeMap<String, V> {
    let mut out = BTreeMap::new();
    // Live numbers first, so a stale key can never take a live one's place.
    for (k, v) in map.iter().filter(|(k, _)| live.contains_key(*k)) {
        out.insert(live[k].clone(), *v);
    }
    for (k, v) in map.iter().filter(|(k, _)| !live.contains_key(*k)) {
        if let Carried::Keep(n) = carry_ref(k, live, live_new, convert) {
            out.entry(n).or_insert(*v);
        }
    }
    out
}

/// Renumbers every category and account of `b` to pattern `to` (from the
/// budget's current pattern), rewrites everything that names them (see the
/// module docs) and sets the pattern. Returns the plan it carried out. On
/// any conflict nothing changes. Renumbering to the pattern the budget
/// already has just tidies spellings (pads short pieces, for example).
pub fn renumber(b: &mut Budget, to: AccountCoding) -> Result<RenumberPlan, RenumberError> {
    let (plan, (cats, accts)) = plan_with_maps(b, &to).map_err(RenumberError::Pattern)?;
    if !plan.conflicts.is_empty() {
        return Err(RenumberError::Conflicts(Box::new(plan)));
    }
    let from = b.account_coding;
    let cat_new: BTreeSet<String> = cats.values().cloned().collect();
    let acct_new: BTreeSet<String> = accts.values().cloned().collect();
    let cat_conv = |c: &str| carry_category(&from, &to, c);
    let acct_conv = |c: &str| carry_account(&from, &to, c);
    // Actuals match accounts by the number as the pattern reads it (`1001`
    // is `10-01`), the first account of any that share one, as the cost
    // report does: keyed that way here, before the numbers change.
    let mut acct_read: BTreeMap<String, String> = BTreeMap::new();
    for c in b.categories() {
        for a in &c.accounts {
            acct_read
                .entry(normalize_code(&from, &a.code))
                .or_insert_with(|| accts[&a.code].clone());
        }
    }

    for row in &mut b.topsheet {
        if let TopsheetRow::Category(c) = row {
            c.code = cats[&c.code].clone();
            for a in &mut c.accounts {
                a.code = accts[&a.code].clone();
            }
        }
    }
    for ch in &mut b.charges {
        ch.excluded_categories = carry_set(&ch.excluded_categories, &cats, &cat_new, cat_conv);
        ch.excluded_accounts = carry_set(&ch.excluded_accounts, &accts, &acct_new, acct_conv);
        ch.account = carry_label(&ch.account, &accts, &acct_new, acct_conv);
    }
    for cr in &mut b.credits {
        cr.account = carry_label(&cr.account, &accts, &acct_new, acct_conv);
        for row in &mut cr.rows {
            match &mut row.source {
                CreditSource::Category { category } => {
                    let label = Some(std::mem::take(category));
                    *category = carry_label(&label, &cats, &cat_new, cat_conv).unwrap_or_default();
                }
                CreditSource::Account { account } => {
                    let label = Some(std::mem::take(account));
                    *account =
                        carry_label(&label, &accts, &acct_new, acct_conv).unwrap_or_default();
                }
                _ => {}
            }
        }
    }
    for p in &mut b.report_presets {
        let spec = &mut p.spec;
        spec.excluded_categories = carry_set(&spec.excluded_categories, &cats, &cat_new, cat_conv);
        spec.excluded_accounts = carry_set(&spec.excluded_accounts, &accts, &acct_new, acct_conv);
        // A bid's picks and a range's ends follow too (decisions.md, entry
        // 92); a range end that names no account is converted by the
        // pattern like any stale number, so the range keeps its place.
        for (is_cat, code) in spec.bid.iter_mut().flat_map(|l| l.codes_mut()) {
            let label = Some(std::mem::take(code));
            *code = if is_cat {
                carry_label(&label, &cats, &cat_new, cat_conv)
            } else {
                carry_label(&label, &accts, &acct_new, acct_conv)
            }
            .unwrap_or_default();
        }
    }
    if let Some(base) = &mut b.baseline {
        base.categories = carry_keys(&base.categories, &cats, &cat_new, cat_conv);
        base.accounts = carry_keys(&base.accounts, &accts, &acct_new, acct_conv);
    }
    if !b.actuals.is_empty() {
        carry_actuals(
            std::sync::Arc::make_mut(&mut b.actuals),
            &Live {
                by_code: &accts,
                by_reading: &acct_read,
                new: &acct_new,
                from: &from,
            },
            acct_conv,
        );
    }
    b.account_coding = to;
    Ok(plan)
}

/// The live accounts as actuals see them.
struct Live<'a> {
    /// Old number as the chart spells it to new number.
    by_code: &'a BTreeMap<String, String>,
    /// Old number as the old pattern reads it ([`normalize_code`]) to new
    /// number.
    by_reading: &'a BTreeMap<String, String>,
    /// Every new number.
    new: &'a BTreeSet<String>,
    from: &'a AccountCoding,
}

impl Live<'_> {
    /// The new number of the live account `code` names, read the way the
    /// cost report reads it: `1001` names `10-01` under a `11-01` pattern,
    /// but a number too long for the pattern (`10012`) names nothing.
    fn get(&self, code: &str) -> Option<&String> {
        self.by_code
            .get(code)
            .or_else(|| self.by_reading.get(&normalize_code(self.from, code)))
    }

    fn carry(&self, code: &str, convert: impl Fn(&str) -> Result<String, String>) -> Carried {
        match self.get(code) {
            Some(new) => Carried::Keep(new.clone()),
            None => carry_ref(code, self.by_code, self.new, convert),
        }
    }
}

/// Transactions and estimate-to-complete overrides follow their accounts,
/// matched the way the cost report matches them (any spelling of a live
/// number). A transaction coded to a number not in the chart is carried
/// over by the same rule, but keeps its spelling when it would start naming
/// a live account it never named (it stays unassigned).
fn carry_actuals(
    a: &mut crate::actuals::Actuals,
    live: &Live<'_>,
    convert: impl Fn(&str) -> Result<String, String> + Copy,
) {
    for t in &mut a.transactions {
        if t.account.is_empty() {
            continue;
        }
        let new = match live.carry(&t.account, convert) {
            Carried::Keep(n) => n,
            Carried::Clash(_) => continue,
        };
        if new != t.account {
            std::sync::Arc::make_mut(t).account = new;
        }
    }
    let mut etc = BTreeMap::new();
    // Live numbers first, so a stale key can never take a live one's place.
    for (k, v) in &a.etc {
        if let Some(new) = live.get(k) {
            etc.insert(new.clone(), *v);
        }
    }
    for (k, v) in a.etc.iter().filter(|(k, _)| live.get(k).is_none()) {
        if let Carried::Keep(n) = live.carry(k, convert) {
            etc.entry(n).or_insert(*v);
        }
    }
    a.etc = etc;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Account, Baseline, Category, Charge, ChargeKind, Credit, Detail};
    use crate::{recalc, Dec, RecalcOptions};

    fn pat(c: u8, a: u8, s: u8, t: u8, sep: Option<char>) -> AccountCoding {
        AccountCoding {
            category_width: c,
            account_width: a,
            sub_width: s,
            set_width: t,
            separator: sep,
        }
    }

    fn budget(coding: AccountCoding, chart: &[(&str, &[&str])]) -> Budget {
        let mut b = Budget::new("Show", "USD");
        b.account_coding = coding;
        let mut id = 0;
        for (cat, accts) in chart {
            b.topsheet.push(TopsheetRow::Category(Category {
                code: cat.to_string(),
                name: format!("Category {cat}"),
                accounts: accts
                    .iter()
                    .map(|a| {
                        id += 1;
                        Account {
                            code: a.to_string(),
                            name: format!("Account {a}"),
                            details: vec![Detail::line(id, "Line", 1, "Flat", 100 * id as i64)],
                            notes: Vec::new(),
                        }
                    })
                    .collect(),
                notes: Vec::new(),
            }));
        }
        b
    }

    /// A percent charge with no exclusions (from the starter budget, so new
    /// charge fields come along with their defaults).
    fn charge(name: &str, percent: i64) -> Charge {
        let mut c = crate::starter::budget().charges[0].clone();
        c.name = name.into();
        c.account = None;
        c.kind = ChargeKind::Percent(Dec::from_int(percent));
        c.excluded_amounts.clear();
        c.excluded_groups.clear();
        c.excluded_categories.clear();
        c.excluded_accounts.clear();
        c
    }

    fn codes(b: &Budget) -> Vec<String> {
        b.categories()
            .flat_map(|c| {
                std::iter::once(c.code.clone()).chain(c.accounts.iter().map(|a| a.code.clone()))
            })
            .collect()
    }

    #[test]
    fn separator_and_width_changes() {
        let mut b = budget(
            pat(2, 2, 0, 0, Some('-')),
            &[("11", &["11-01", "11-02"]), ("20", &["20-00"])],
        );
        let p = renumber(&mut b, pat(3, 3, 0, 0, Some('.'))).unwrap();
        assert_eq!(codes(&b), ["011", "011.001", "011.002", "020", "020.000"]);
        assert_eq!(
            p.accounts[0],
            CodeChange {
                old: "11-01".into(),
                new: "011.001".into()
            }
        );
        assert!(p.changes_anything());
        assert_eq!(b.account_coding, pat(3, 3, 0, 0, Some('.')));
        // And back.
        renumber(&mut b, pat(2, 2, 0, 0, Some('-'))).unwrap();
        assert_eq!(codes(&b), ["11", "11-01", "11-02", "20", "20-00"]);
    }

    #[test]
    fn no_separator_numbers_are_cut_by_the_current_widths() {
        let mut b = budget(pat(2, 2, 0, 0, None), &[("11", &["1100", "1101"])]);
        renumber(&mut b, pat(2, 2, 0, 0, Some('-'))).unwrap();
        assert_eq!(codes(&b), ["11", "11-00", "11-01"]);
        // A sub-account segment can be added: numbers stay as they are.
        renumber(&mut b, pat(2, 2, 2, 0, Some('-'))).unwrap();
        assert_eq!(codes(&b), ["11", "11-00", "11-01"]);
    }

    #[test]
    fn numbers_that_dont_fit_or_collapse_are_refused() {
        // 110 can't go into 2 characters.
        let mut b = budget(pat(3, 3, 0, 0, Some('-')), &[("110", &["110-001"])]);
        let before = b.clone();
        let e = renumber(&mut b, pat(2, 2, 0, 0, Some('-'))).unwrap_err();
        let RenumberError::Conflicts(p) = e else {
            panic!()
        };
        assert!(
            matches!(&p.conflicts[0], Conflict::DoesNotFit { kind: CodeKind::Category, code, .. } if code == "110")
        );
        assert_eq!(b, before, "refused: nothing changed");

        // 011-001 and 011-01 (typed earlier with another pattern) both become 11-01.
        let mut b = budget(
            pat(3, 3, 0, 0, Some('-')),
            &[("011", &["011-001", "011-01"])],
        );
        let e = renumber(&mut b, pat(2, 2, 0, 0, Some('-'))).unwrap_err();
        let RenumberError::Conflicts(p) = e else {
            panic!()
        };
        assert_eq!(
            p.conflicts,
            vec![Conflict::Collapse {
                kind: CodeKind::Account,
                codes: vec!["011-001".into(), "011-01".into()],
                new: "11-01".into()
            }]
        );
        assert!(e_text(&p).contains("would all become 11-01"));

        // A filled sub-account part has nowhere to go.
        let mut b = budget(pat(2, 2, 2, 0, Some('-')), &[("11", &["11-01-05"])]);
        assert!(renumber(&mut b, pat(2, 2, 0, 0, Some('-'))).is_err());

        // A number longer than its pattern doesn't follow it.
        let b = budget(pat(2, 2, 0, 0, None), &[("11", &["110011"])]);
        let p = plan(&b, &pat(2, 2, 0, 0, Some('-'))).unwrap();
        assert_eq!(p.conflicts.len(), 1);
        assert_eq!(p.accounts[0].new, "");
    }

    fn e_text(p: &RenumberPlan) -> String {
        RenumberError::Conflicts(Box::new(p.clone())).to_string()
    }

    #[test]
    fn duplicates_already_there_are_not_a_conflict() {
        let mut b = budget(pat(2, 2, 0, 0, Some('-')), &[("11", &["11-01", "11-01"])]);
        renumber(&mut b, pat(2, 2, 0, 0, Some('.'))).unwrap();
        assert_eq!(codes(&b), ["11", "11.01", "11.01"]);
    }

    #[test]
    fn a_bad_pattern_is_refused() {
        let mut b = budget(pat(2, 2, 0, 0, Some('-')), &[("11", &["11-01"])]);
        assert!(matches!(
            renumber(&mut b, pat(0, 2, 0, 0, None)),
            Err(RenumberError::Pattern(CodingError::CategoryRequired))
        ));
        assert!(plan(&b, &pat(2, 0, 2, 0, None)).is_err());
    }

    #[test]
    fn references_follow_and_variance_still_matches() {
        let mut b = budget(
            pat(2, 2, 0, 0, Some('-')),
            &[("11", &["11-01", "11-02"]), ("20", &["20-01"])],
        );
        let mut ch = charge("Contingency", 10);
        ch.account = Some("20-01".into());
        ch.excluded_categories = ["11".to_string(), "99".to_string()].into();
        // 30-01 is no longer in the chart: carried over, still naming nothing.
        ch.excluded_accounts = ["11-02".to_string(), "30-01".to_string()].into();
        b.charges.push(ch);
        b.credits.push(Credit {
            name: "Rebate".into(),
            account: Some("11-01".into()),
            applied: true,
            cap: None,
            discount_percent: Dec::ZERO,
            excluded_fringes: BTreeSet::new(),
            locations: BTreeSet::new(),
            rows: Vec::new(),
        });
        let mut preset = crate::ReportPreset {
            name: "Mine".into(),
            default: false,
            spec: crate::ReportSpec::default(),
        };
        preset.spec.excluded_categories.insert("20".into());
        preset.spec.excluded_accounts.insert("11-01".into());
        preset.spec.bid = Some(crate::BidLayout {
            lines: vec![crate::BidLine::Section {
                name: "Shoot".into(),
                picks: vec![
                    crate::BidPick::Category("20".into()),
                    crate::BidPick::Account("11-02".into()),
                    crate::BidPick::Range {
                        from: "11-01".into(),
                        to: "11-99".into(),
                    },
                ],
            }],
            ..crate::BidLayout::default()
        });
        b.report_presets.push(preset);
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        b.baseline = Baseline::capture(&r);
        let grand = r.grand_total;

        renumber(&mut b, pat(3, 3, 0, 0, Some('.'))).unwrap();
        let ch = &b.charges[0];
        assert_eq!(ch.account.as_deref(), Some("020.001"));
        assert_eq!(
            ch.excluded_categories,
            ["011".to_string(), "099".to_string()].into()
        );
        assert_eq!(
            ch.excluded_accounts,
            ["011.002".to_string(), "030.001".to_string()].into()
        );
        assert_eq!(b.credits[0].account.as_deref(), Some("011.001"));
        assert_eq!(
            b.report_presets[0].spec.excluded_categories,
            ["020".to_string()].into()
        );
        assert_eq!(
            b.report_presets[0].spec.excluded_accounts,
            ["011.001".to_string()].into()
        );
        let bid = b.report_presets[0].spec.bid.as_ref().unwrap();
        assert_eq!(
            bid.lines[0],
            crate::BidLine::Section {
                name: "Shoot".into(),
                picks: vec![
                    crate::BidPick::Category("020".into()),
                    crate::BidPick::Account("011.002".into()),
                    crate::BidPick::Range {
                        from: "011.001".into(),
                        to: "011.099".into(),
                    },
                ],
            }
        );
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        assert_eq!(r.grand_total, grand, "totals don't move");
        let v = crate::variance(b.baseline.as_ref().unwrap(), &r).unwrap();
        assert!(
            v.accounts
                .values()
                .all(|c| c.original.is_some() && c.current.is_some()),
            "every account still matches"
        );
        assert!(v.categories.values().all(|c| c.original == c.current));
    }

    #[test]
    fn a_stale_reference_never_starts_matching_a_live_number() {
        // Chart has 11-01 (2/2). An exclusion typed earlier as "011-01"
        // (another pattern) named nothing; under 2/2 it becomes 11-01.
        let mut b = budget(pat(3, 2, 0, 0, Some('-')), &[("011", &["011-01"])]);
        let mut ch = charge("Fee", 5);
        ch.excluded_accounts = ["11-01".to_string()].into();
        b.charges.push(ch);
        renumber(&mut b, pat(2, 2, 0, 0, Some('-'))).unwrap();
        assert_eq!(codes(&b), ["11", "11-01"]);
        assert!(
            b.charges[0].excluded_accounts.is_empty(),
            "dropped, not matched"
        );
    }

    #[test]
    fn fringe_accounts_are_listed_and_a_clash_is_a_warning() {
        let b = budget(pat(2, 2, 0, 0, Some('-')), &[("11", &["11-01", "11-99"])]);
        let p = plan(&b, &pat(2, 3, 0, 0, Some('-'))).unwrap();
        assert_eq!(
            p.fringe_accounts,
            vec![CodeChange {
                old: "11-99".into(),
                new: "11-099".into()
            }]
        );
        assert_eq!(p.warnings.len(), 1, "11-099 is also an account");
        let mut b2 = b.clone();
        b2.fringe_posting = FringePosting::Account;
        assert!(plan(&b2, &pat(2, 3, 0, 0, Some('-')))
            .unwrap()
            .fringe_accounts
            .is_empty());
    }

    // -----------------------------------------------------------------------
    // Property tests

    /// xorshift64*: deterministic and dependency-free.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    fn random_pattern(r: &mut Rng) -> AccountCoding {
        let used = 1 + r.below(4) as usize;
        let mut w = [0u8; 4];
        for x in w.iter_mut().take(used) {
            *x = 1 + r.below(6) as u8;
        }
        let seps = [
            None,
            Some('-'),
            Some('.'),
            Some(' '),
            Some('/'),
            Some(':'),
            Some(','),
            Some(';'),
        ];
        pat(
            w[0],
            w[1],
            w[2],
            w[3],
            seps[r.below(seps.len() as u64) as usize],
        )
    }

    fn random_piece(r: &mut Rng, width: u8) -> String {
        // Mostly small numbers, so they fit narrower patterns; sometimes letters.
        let digits = 1 + r.below(u64::from(width).min(3)) as usize;
        let mut s: String = (0..digits)
            .map(|_| {
                if r.below(10) == 0 {
                    (b'A' + r.below(26) as u8) as char
                } else {
                    (b'0' + r.below(10) as u8) as char
                }
            })
            .collect();
        s = format!("{s:0>w$}", w = usize::from(width));
        s
    }

    /// A budget whose numbers are written in its own pattern.
    fn random_budget(r: &mut Rng) -> Budget {
        let coding = random_pattern(r);
        let used = widths(&coding).iter().take_while(|w| **w > 0).count();
        let mut chart: Vec<(String, Vec<String>)> = Vec::new();
        for _ in 0..1 + r.below(4) {
            let cat = random_piece(r, coding.category_width);
            let mut accts = Vec::new();
            if used > 1 {
                for _ in 0..r.below(5) {
                    let n = 2 + r.below((used - 1) as u64) as usize;
                    let w = widths(&coding);
                    let mut code = AccountCode {
                        category: cat.clone(),
                        ..AccountCode::default()
                    };
                    code.account = random_piece(r, w[1]);
                    if n > 2 {
                        code.sub = random_piece(r, w[2]);
                    }
                    if n > 3 {
                        code.set = random_piece(r, w[3]);
                    }
                    accts.push(coding.format(&code));
                }
            }
            chart.push((cat, accts));
        }
        let refs: Vec<(&str, Vec<&str>)> = chart
            .iter()
            .map(|(c, a)| (c.as_str(), a.iter().map(String::as_str).collect()))
            .collect();
        let refs: Vec<(&str, &[&str])> = refs.iter().map(|(c, a)| (*c, a.as_slice())).collect();
        let mut b = budget(coding, &refs);
        // Some references: exclusions and a baseline.
        let accts = account_codes(&b);
        let cats = category_codes(&b);
        let mut ch = charge("Fee", 5);
        ch.account = accts.first().cloned();
        if let Some(a) = accts.last() {
            ch.excluded_accounts.insert(a.clone());
        }
        ch.excluded_categories.insert(cats[0].clone());
        b.charges.push(ch);
        if let Ok(rep) = recalc(&b, &RecalcOptions::default()) {
            b.baseline = Baseline::capture(&rep);
        }
        b
    }

    #[test]
    fn renumbering_there_and_back_gives_the_same_budget() {
        let mut r = Rng(0x5EED_2200_0000_0001);
        let mut round_trips = 0;
        for _ in 0..3000 {
            let original = random_budget(&mut r);
            let to = random_pattern(&mut r);
            let mut b = original.clone();
            let total = recalc(&b, &RecalcOptions::default()).map(|x| x.grand_total);
            match renumber(&mut b, to) {
                Err(RenumberError::Conflicts(_)) => {
                    assert_eq!(b, original, "a refusal changes nothing");
                    continue;
                }
                Err(e) => panic!("{e}"),
                Ok(_) => {}
            }
            assert_eq!(b.account_coding, to);
            // Every number is now canonical in the new pattern.
            for c in account_codes(&b) {
                assert_eq!(to.normalize(&c).unwrap(), c);
            }
            assert_eq!(
                recalc(&b, &RecalcOptions::default()).map(|x| x.grand_total),
                total,
                "renumbering never moves money"
            );
            // Renumbering to the same pattern again changes nothing.
            let mut again = b.clone();
            renumber(&mut again, to).unwrap();
            assert_eq!(again, b);
            // And back.
            match renumber(&mut b, original.account_coding) {
                Ok(_) => {
                    assert_eq!(b, original, "there and back to {to:?}");
                    round_trips += 1;
                }
                Err(RenumberError::Conflicts(p)) => panic!("back failed: {:?}", p.conflicts),
                Err(e) => panic!("{e}"),
            }
        }
        assert!(
            round_trips > 500,
            "enough cases went through ({round_trips})"
        );
    }

    /// A transaction typed `1001` names account `10-01` (the cost report
    /// reads it that way), so a width change carries it to `010-01` instead
    /// of leaving it on a spelling that now reads as `100-01`. A number too
    /// long for the old pattern named nothing and keeps its spelling.
    #[test]
    fn actuals_follow_any_spelling_of_their_account() {
        use crate::actuals::{add_batch, chart, cost_report_of, set_etc, EtcMethod, Transaction};
        let mut b = budget(
            pat(2, 2, 0, 0, Some('-')),
            &[("10", &["10-01", "10-02"]), ("20", &["20-01"])],
        );
        let tx = |account: &str, amount| Transaction {
            account: account.into(),
            amount,
            ..Transaction::default()
        };
        let txs = vec![
            tx("1001", 100),
            tx("10-02", 200),
            tx("10.01", 300),
            tx("10012", 400),
            tx("98-01", 500),
        ];
        add_batch(&mut b, "ledger.csv", "", txs).unwrap();
        set_etc(&mut b, "20-01", Some(EtcMethod::Manual(42))).unwrap();
        // An override under another spelling of a live number.
        std::sync::Arc::make_mut(&mut b.actuals)
            .etc
            .insert("1002".into(), EtcMethod::Manual(7));
        let actual_of = |b: &Budget| {
            let r = recalc(b, &RecalcOptions::default()).unwrap();
            let c = cost_report_of(b, &r).unwrap();
            let per: Vec<_> = c
                .categories
                .iter()
                .flat_map(|c| &c.accounts)
                .map(|a| (a.figures.actual, a.figures.etc))
                .collect();
            (per, c.unassigned.actual, c.total.etc)
        };
        let before = actual_of(&b);
        assert_eq!(before.0[0].0, 400, "1001 and 10.01 are 10-01");
        assert_eq!(before.1, 900, "10012 and 98-01 name nothing");

        renumber(&mut b, pat(3, 2, 0, 0, Some('-'))).unwrap();
        let accounts: Vec<&str> = b
            .actuals
            .transactions
            .iter()
            .map(|t| t.account.as_str())
            .collect();
        assert_eq!(accounts, ["010-01", "010-02", "010-01", "10012", "098-01"]);
        let etc: Vec<_> = b.actuals.etc.iter().collect();
        assert_eq!(
            etc,
            [
                (&"010-02".to_string(), &EtcMethod::Manual(7)),
                (&"020-01".to_string(), &EtcMethod::Manual(42)),
            ]
        );
        assert_eq!(actual_of(&b), before, "nothing moves between accounts");
        // The too-long number still names nothing in the new chart.
        let c = chart(&b);
        assert!(!c.contains_key(&normalize_code(&b.account_coding, "10012")));
    }
}
