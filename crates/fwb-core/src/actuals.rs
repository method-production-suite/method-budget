//! Actual costs against the budget (roadmap 3.7): transactions brought in
//! from accounting, estimates to complete, and the cost report.
//!
//! A budget keeps its actuals in [`Actuals`]: a list of [`Transaction`]s
//! (date, account number, vendor, description, amount in cents...), the
//! [`ImportBatch`]es they came in with (so one import can be taken back out
//! whole), and per-account estimate-to-complete overrides ([`EtcMethod`]).
//! Actuals are about the production's spending, not the budget's content:
//! a version never holds them, and restoring a version keeps them.
//!
//! # The cost report
//!
//! [`cost_report`] puts the actuals next to the budget's own totals, for
//! every account, category fringe account, category, production level,
//! the summary rows and the grand total ([`CostFigures`]):
//!
//! - **Budget**: the total as the topsheet shows it (an account's total
//!   includes its fringes only when fringes post to the account).
//! - **Baseline**: the locked original total, when the budget has a
//!   baseline (accounts, categories and the grand total).
//! - **Actual**: the sum of the transactions coded to the account. Account
//!   numbers are compared as the budget's pattern writes them, so `1101`
//!   and `11-01` are the same account. When two accounts share a number,
//!   the first one in the chart gets its transactions.
//! - **ETC** (estimate to complete), per account by its method:
//!   budget minus actual (the default: what is left, never past zero, so
//!   an account over budget has nothing left to spend), a manual amount,
//!   or percent complete (the account is that far along, so the estimate
//!   at completion is the actual divided by it, rounded once to the cent).
//! - **EAC** (estimate at completion) = actual + ETC.
//! - **Variance** = budget - EAC: negative means over budget.
//! - **Percent spent** = actual / budget, to two places (none for a zero
//!   budget).
//!
//! Categories, levels and the totals add up their parts' figures exactly
//! (integer cents); only percent spent is worked out from the sums. Budget
//! items outside the chart (fringes posted by budget or level, contractual
//! charges) have no actuals and keep their whole budget to complete.
//! Transactions whose account isn't in the chart (or that have none) are
//! the **unassigned** actuals: they count in the total actual, with
//! nothing to complete. Credits are not part of the cost report.
//!
//! Everything here is plain functions on budgets. Rolling the transactions
//! up ([`rollup`]) is the only part that visits every transaction; it
//! depends on the actuals and the account pattern only, so callers can
//! cache it, and [`cost_report`] costs one pass over the accounts.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use crate::coding::AccountCoding;
use crate::decimal::{self, Dec};
use crate::model::{Budget, FringePosting, RowId, TopsheetRow};
use crate::report::Report;
use crate::Cents;

/// Identity of a [`Transaction`], unique within a budget's actuals.
pub type TransactionId = u64;
/// Identity of an [`ImportBatch`], unique within a budget's actuals.
pub type BatchId = u64;

/// Most transactions one budget keeps.
pub const MAX_TRANSACTIONS: usize = 250_000;
/// Longest vendor, description or reference, in characters.
pub const MAX_TEXT_CHARS: usize = 500;
/// Most tags on one transaction.
pub const MAX_TAGS: usize = 20;
/// Longest tag, in characters.
pub const MAX_TAG_CHARS: usize = 60;
/// Longest file name an import batch keeps, in characters.
pub const MAX_FILE_CHARS: usize = 255;

/// One actual cost: a payment, invoice or journal line from accounting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transaction {
    pub id: TransactionId,
    /// ISO 8601 date, `YYYY-MM-DD`, or empty when unknown.
    pub date: String,
    /// Account number as the budget's pattern writes it, or empty for
    /// none. A number the chart doesn't have counts as unassigned.
    pub account: String,
    /// The detail row it pays for, when known.
    pub line: Option<RowId>,
    /// Free-form labels (`PO`, `Petty cash`).
    pub tags: BTreeSet<String>,
    pub vendor: String,
    pub description: String,
    /// Invoice, purchase order or check number.
    pub reference: String,
    pub amount: Cents,
    /// The import it came in with; `None` for one entered by hand.
    pub batch: Option<BatchId>,
}

/// One import of actuals, so it can be taken back out whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportBatch {
    pub id: BatchId,
    /// The file's name (never its path).
    pub file: String,
    /// When it was imported, as an ISO 8601 date-time provided by the
    /// caller (the engine never reads the clock).
    pub imported: String,
    /// How many transactions it brought in.
    pub rows: u64,
}

/// How an account's estimate to complete is worked out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EtcMethod {
    /// What is left of the budget, never past zero (the default).
    BudgetMinusActual,
    /// A typed amount.
    Manual(Cents),
    /// The account is this percent complete (more than 0, at most 100), so
    /// it ends at actual / percent.
    PercentComplete(Dec),
}

impl EtcMethod {
    /// The name the file format and the app use.
    pub fn name(self) -> &'static str {
        match self {
            EtcMethod::BudgetMinusActual => "budget_minus_actual",
            EtcMethod::Manual(_) => "manual",
            EtcMethod::PercentComplete(_) => "percent_complete",
        }
    }

    /// Whether a percent complete is in range (more than 0, at most 100).
    pub fn is_valid(self) -> bool {
        match self {
            EtcMethod::PercentComplete(p) => p > Dec::ZERO && p <= Dec::from_int(100),
            _ => true,
        }
    }
}

/// A budget's actuals: transactions, the imports they came in with, and
/// estimate-to-complete overrides. Shared (`Arc`) by the budget so copying
/// a budget for undo costs a pointer; each transaction is shared too, so a
/// changed copy keeps the unchanged transactions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Actuals {
    /// In the order they came in.
    pub transactions: Vec<Arc<Transaction>>,
    /// Oldest first.
    pub batches: Vec<ImportBatch>,
    /// Estimate-to-complete methods by account number (accounts without
    /// one use budget minus actual).
    pub etc: BTreeMap<String, EtcMethod>,
}

impl Actuals {
    /// No transactions, imports or overrides.
    pub fn is_empty(&self) -> bool {
        self.transactions.is_empty() && self.batches.is_empty() && self.etc.is_empty()
    }

    /// The id a new transaction gets: one past the highest.
    pub fn next_transaction_id(&self) -> TransactionId {
        self.transactions
            .iter()
            .map(|t| t.id)
            .max()
            .map_or(1, |m| m.saturating_add(1))
    }

    /// The id a new import batch gets: one past the highest (counting
    /// batch ids still on transactions).
    pub fn next_batch_id(&self) -> BatchId {
        self.batches
            .iter()
            .map(|b| b.id)
            .chain(self.transactions.iter().filter_map(|t| t.batch))
            .max()
            .map_or(1, |m| m.saturating_add(1))
    }

    pub fn batch(&self, id: BatchId) -> Option<&ImportBatch> {
        self.batches.iter().find(|b| b.id == id)
    }
}

/// Why an actuals edit was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActualsError {
    /// No transaction with this id.
    NotFound(TransactionId),
    /// No import with this id.
    BatchNotFound(BatchId),
    /// A value that can't be kept, in words.
    Invalid(String),
    /// More than [`MAX_TRANSACTIONS`].
    TooMany,
}

impl fmt::Display for ActualsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActualsError::NotFound(id) => write!(f, "there is no transaction {id}"),
            ActualsError::BatchNotFound(id) => write!(f, "there is no import {id}"),
            ActualsError::Invalid(m) => f.write_str(m),
            ActualsError::TooMany => {
                write!(f, "a budget keeps at most {MAX_TRANSACTIONS} transactions")
            }
        }
    }
}

impl std::error::Error for ActualsError {}

// ---------------------------------------------------------------------------
// Dates

/// Days in `month` of `year` (Gregorian), or 0 for a month out of range.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

/// `YYYY-MM-DD` for a real calendar date in the years 1 to 9999.
pub fn iso_date(year: i32, month: u32, day: u32) -> Option<String> {
    if !(1..=9999).contains(&year) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Whether `s` is a real date written `YYYY-MM-DD`.
pub fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let p = &s[r];
        p.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| p.parse().ok())?
    };
    match (num(0..4), num(5..7), num(8..10)) {
        (Some(y), Some(m), Some(d)) => iso_date(y as i32, m, d).is_some(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Checking values

fn check_text(what: &str, s: &str) -> Result<(), ActualsError> {
    if s.chars().count() > MAX_TEXT_CHARS {
        return Err(ActualsError::Invalid(format!(
            "the {what} is longer than {MAX_TEXT_CHARS} characters"
        )));
    }
    Ok(())
}

/// Tags trimmed, without blanks, each at most [`MAX_TAG_CHARS`] and at most
/// [`MAX_TAGS`] of them.
pub fn clean_tags<'a>(
    tags: impl IntoIterator<Item = &'a str>,
) -> Result<BTreeSet<String>, ActualsError> {
    let mut out = BTreeSet::new();
    for t in tags {
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        if t.chars().count() > MAX_TAG_CHARS || t.chars().any(char::is_control) {
            return Err(ActualsError::Invalid(format!(
                "a tag is one line of at most {MAX_TAG_CHARS} characters"
            )));
        }
        out.insert(t.to_string());
    }
    if out.len() > MAX_TAGS {
        return Err(ActualsError::Invalid(format!(
            "a transaction has at most {MAX_TAGS} tags"
        )));
    }
    Ok(out)
}

/// Checks one transaction's fields (not its id or batch).
pub fn check_transaction(t: &Transaction) -> Result<(), ActualsError> {
    if !t.date.is_empty() && !is_iso_date(&t.date) {
        return Err(ActualsError::Invalid(format!(
            "{:?} isn't a date written YYYY-MM-DD",
            t.date
        )));
    }
    if t.account.chars().count() > 64 {
        return Err(ActualsError::Invalid(
            "the account number is too long".into(),
        ));
    }
    check_text("vendor", &t.vendor)?;
    check_text("description", &t.description)?;
    check_text("reference", &t.reference)?;
    clean_tags(t.tags.iter().map(String::as_str))?;
    Ok(())
}

/// An account number as the budget's pattern writes it; a number the
/// pattern can't read, or could only read by cutting it short (`11-012`
/// under `11-01`), is kept as typed (trimmed), so it never lands on an
/// account it doesn't name.
pub fn normalize_code(coding: &AccountCoding, code: &str) -> String {
    coding.key(code)
}

/// What an account number in the chart is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartEntry {
    /// The number as the chart spells it.
    pub code: String,
    pub name: String,
    /// A category's fringe account (fringes posted by category).
    pub fringe: bool,
}

/// Every account number transactions can be coded to, by its normalized
/// spelling: the accounts (the first of any that share a number) and,
/// when fringes post by category, each category's fringe account.
pub fn chart(b: &Budget) -> BTreeMap<String, ChartEntry> {
    let coding = b.account_coding;
    let mut out = BTreeMap::new();
    for c in b.categories() {
        for a in &c.accounts {
            out.entry(normalize_code(&coding, &a.code))
                .or_insert_with(|| ChartEntry {
                    code: a.code.clone(),
                    name: a.name.clone(),
                    fringe: false,
                });
        }
    }
    if let FringePosting::Category { suffix } = &b.fringe_posting {
        for c in b.categories() {
            let number = coding.category_fringe_account(&c.code, suffix);
            out.entry(normalize_code(&coding, &number))
                .or_insert_with(|| ChartEntry {
                    code: number,
                    name: format!("{} fringes", c.name).trim().to_string(),
                    fringe: true,
                });
        }
    }
    out
}

/// The account number `typed` names in `b`'s chart, as the chart spells it:
/// `Ok("")` for blank (no account), an error when the chart has no such
/// account.
pub fn chart_code(b: &Budget, typed: &str) -> Result<String, ActualsError> {
    let key = normalize_code(&b.account_coding, typed);
    if key.is_empty() {
        return Ok(String::new());
    }
    chart(b).get(&key).map(|e| e.code.clone()).ok_or_else(|| {
        ActualsError::Invalid(format!("this budget has no account {}", typed.trim()))
    })
}

/// Whether `id` is a detail row of the account numbered `code` (normalized
/// spelling compared).
fn line_in_account(b: &Budget, code: &str, id: RowId) -> bool {
    let coding = b.account_coding;
    let key = normalize_code(&coding, code);
    b.categories()
        .flat_map(|c| &c.accounts)
        .filter(|a| normalize_code(&coding, &a.code) == key)
        .any(|a| a.details.iter().any(|d| d.id == id))
}

// ---------------------------------------------------------------------------
// Edits (each a plain function on a budget; the app wraps each in one undo
// step)

/// Adds an import of `transactions` (their ids and batch are set here),
/// named by the file it came from and when. Returns the new batch's id.
pub fn add_batch(
    b: &mut Budget,
    file: &str,
    imported: &str,
    transactions: Vec<Transaction>,
) -> Result<BatchId, ActualsError> {
    if transactions.is_empty() {
        return Err(ActualsError::Invalid(
            "there are no transactions to import".into(),
        ));
    }
    let file = file.trim();
    if file.chars().count() > MAX_FILE_CHARS || file.chars().any(char::is_control) {
        return Err(ActualsError::Invalid("the file name is too long".into()));
    }
    if imported.chars().count() > 64 || imported.chars().any(char::is_control) {
        return Err(ActualsError::Invalid(
            "the import time is not a date-time".into(),
        ));
    }
    for t in &transactions {
        check_transaction(t)?;
    }
    if b.actuals.transactions.len() + transactions.len() > MAX_TRANSACTIONS {
        return Err(ActualsError::TooMany);
    }
    let a = Arc::make_mut(&mut b.actuals);
    let batch = a.next_batch_id();
    let mut id = a.next_transaction_id();
    let rows = transactions.len() as u64;
    for mut t in transactions {
        t.id = id;
        t.batch = Some(batch);
        id = id.checked_add(1).ok_or(ActualsError::TooMany)?;
        a.transactions.push(Arc::new(t));
    }
    a.batches.push(ImportBatch {
        id: batch,
        file: file.to_string(),
        imported: imported.to_string(),
        rows,
    });
    Ok(batch)
}

/// Takes import `id` back out: its record and every transaction still
/// carrying it. Returns how many transactions went.
pub fn remove_batch(b: &mut Budget, id: BatchId) -> Result<usize, ActualsError> {
    let known =
        b.actuals.batch(id).is_some() || b.actuals.transactions.iter().any(|t| t.batch == Some(id));
    if !known {
        return Err(ActualsError::BatchNotFound(id));
    }
    let a = Arc::make_mut(&mut b.actuals);
    let before = a.transactions.len();
    a.transactions.retain(|t| t.batch != Some(id));
    a.batches.retain(|x| x.id != id);
    Ok(before - a.transactions.len())
}

/// A change to some transactions: each field that is `Some` is set on every
/// one of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionEdit {
    /// `YYYY-MM-DD`, or empty for no date.
    pub date: Option<String>,
    /// An account in the chart (any spelling), or empty for unassigned.
    pub account: Option<String>,
    /// A detail row of the transaction's account, or `Some(None)` for none.
    pub line: Option<Option<RowId>>,
    pub tags: Option<BTreeSet<String>>,
    pub vendor: Option<String>,
    pub description: Option<String>,
    pub reference: Option<String>,
    pub amount: Option<Cents>,
}

/// Applies `e` to the transactions with these ids (every one must exist).
/// Moving transactions to another account drops a line they named that
/// isn't in it. Returns how many changed.
pub fn edit_transactions(
    b: &mut Budget,
    ids: &[TransactionId],
    e: &TransactionEdit,
) -> Result<usize, ActualsError> {
    let wanted: BTreeSet<TransactionId> = ids.iter().copied().collect();
    if wanted.is_empty() {
        return Err(ActualsError::Invalid("no transactions are selected".into()));
    }
    for id in &wanted {
        if !b.actuals.transactions.iter().any(|t| t.id == *id) {
            return Err(ActualsError::NotFound(*id));
        }
    }
    let account = match &e.account {
        Some(typed) => Some(chart_code(b, typed)?),
        None => None,
    };
    let tags = match &e.tags {
        Some(t) => Some(clean_tags(t.iter().map(String::as_str))?),
        None => None,
    };
    // Work out every new transaction first, so a refusal changes nothing.
    let mut changed: Vec<(usize, Transaction)> = Vec::new();
    for (i, t) in b.actuals.transactions.iter().enumerate() {
        if !wanted.contains(&t.id) {
            continue;
        }
        let mut n = (**t).clone();
        if let Some(d) = &e.date {
            n.date = d.trim().to_string();
        }
        if let Some(a) = &account {
            n.account = a.clone();
        }
        if let Some(tags) = &tags {
            n.tags = tags.clone();
        }
        if let Some(v) = &e.vendor {
            n.vendor = v.trim().to_string();
        }
        if let Some(v) = &e.description {
            n.description = v.trim().to_string();
        }
        if let Some(v) = &e.reference {
            n.reference = v.trim().to_string();
        }
        if let Some(v) = e.amount {
            n.amount = v;
        }
        match e.line {
            Some(Some(row)) => {
                if !line_in_account(b, &n.account, row) {
                    return Err(ActualsError::Invalid(
                        "that line isn't in the transaction's account".into(),
                    ));
                }
                n.line = Some(row);
            }
            Some(None) => n.line = None,
            None => {
                if let Some(row) = n.line {
                    if !line_in_account(b, &n.account, row) {
                        n.line = None;
                    }
                }
            }
        }
        check_transaction(&n)?;
        if n != **t {
            changed.push((i, n));
        }
    }
    let count = changed.len();
    if count > 0 {
        let a = Arc::make_mut(&mut b.actuals);
        for (i, n) in changed {
            a.transactions[i] = Arc::new(n);
        }
    }
    Ok(count)
}

/// Deletes the transactions with these ids (every one must exist). Returns
/// how many went.
pub fn delete_transactions(b: &mut Budget, ids: &[TransactionId]) -> Result<usize, ActualsError> {
    let wanted: BTreeSet<TransactionId> = ids.iter().copied().collect();
    for id in &wanted {
        if !b.actuals.transactions.iter().any(|t| t.id == *id) {
            return Err(ActualsError::NotFound(*id));
        }
    }
    let a = Arc::make_mut(&mut b.actuals);
    let before = a.transactions.len();
    a.transactions.retain(|t| !wanted.contains(&t.id));
    Ok(before - a.transactions.len())
}

/// Sets how the estimate to complete of account `typed` (any spelling of a
/// number in the chart) is worked out; `None` or budget minus actual takes
/// the override away.
pub fn set_etc(b: &mut Budget, typed: &str, method: Option<EtcMethod>) -> Result<(), ActualsError> {
    let code = chart_code(b, typed)?;
    if code.is_empty() {
        return Err(ActualsError::Invalid("choose an account".into()));
    }
    if let Some(m) = method {
        if !m.is_valid() {
            return Err(ActualsError::Invalid(
                "percent complete is more than 0 and at most 100".into(),
            ));
        }
    }
    let coding = b.account_coding;
    let key = normalize_code(&coding, &code);
    let a = Arc::make_mut(&mut b.actuals);
    // One override per account, whatever spelling an older one has.
    a.etc.retain(|k, _| normalize_code(&coding, k) != key);
    match method {
        None | Some(EtcMethod::BudgetMinusActual) => {}
        Some(m) => {
            a.etc.insert(code, m);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Duplicates

/// What makes two transactions the same for duplicate detection: date,
/// account (normalized), amount and description (trimmed, spaces collapsed,
/// capitals ignored).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DuplicateKey {
    pub date: String,
    pub account: String,
    pub amount: Cents,
    pub description: String,
}

/// The duplicate key of a transaction with these fields.
pub fn duplicate_key(
    coding: &AccountCoding,
    date: &str,
    account: &str,
    amount: Cents,
    description: &str,
) -> DuplicateKey {
    DuplicateKey {
        date: date.trim().to_string(),
        account: normalize_code(coding, account),
        amount,
        description: description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase(),
    }
}

/// Every transaction's duplicate key, with how many transactions have it.
pub fn duplicate_keys(a: &Actuals, coding: &AccountCoding) -> BTreeMap<DuplicateKey, usize> {
    let mut out = BTreeMap::new();
    for t in &a.transactions {
        *out.entry(duplicate_key(
            coding,
            &t.date,
            &t.account,
            t.amount,
            &t.description,
        ))
        .or_insert(0) += 1;
    }
    out
}

/// Groups of transactions that are duplicates of each other (two or more
/// with the same key), each group's ids in order.
pub fn duplicate_groups(a: &Actuals, coding: &AccountCoding) -> Vec<Vec<TransactionId>> {
    let mut by_key: BTreeMap<DuplicateKey, Vec<TransactionId>> = BTreeMap::new();
    for t in &a.transactions {
        by_key
            .entry(duplicate_key(
                coding,
                &t.date,
                &t.account,
                t.amount,
                &t.description,
            ))
            .or_default()
            .push(t.id);
    }
    by_key.into_values().filter(|g| g.len() > 1).collect()
}

// ---------------------------------------------------------------------------
// Rolling up

/// A sum of transactions and how many there were.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sum {
    pub amount: Cents,
    pub count: u64,
}

impl Sum {
    fn add(&mut self, amount: Cents) -> Option<()> {
        self.amount = self.amount.checked_add(amount)?;
        self.count += 1;
        Some(())
    }

    fn add_sum(&mut self, o: Sum) -> Option<()> {
        self.amount = self.amount.checked_add(o.amount)?;
        self.count += o.count;
        Some(())
    }
}

/// The transactions summed by account and by line: the only pass over
/// every transaction. Depends on the actuals and the account pattern only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rollup {
    /// By account number as the pattern writes it (`""`: no account),
    /// whether or not the chart has it.
    pub accounts: BTreeMap<String, Sum>,
    /// By the detail row they name.
    pub lines: BTreeMap<RowId, Sum>,
    pub total: Sum,
}

/// Sums `a`'s transactions by account (read with `coding`) and by line.
/// `None` when a sum overflows.
pub fn rollup(a: &Actuals, coding: &AccountCoding) -> Option<Rollup> {
    let mut out = Rollup::default();
    // Most transactions repeat a few spellings: normalize each once.
    let mut seen: BTreeMap<&str, String> = BTreeMap::new();
    for t in &a.transactions {
        let key = seen
            .entry(t.account.as_str())
            .or_insert_with(|| normalize_code(coding, &t.account))
            .clone();
        out.accounts.entry(key).or_default().add(t.amount)?;
        if let Some(line) = t.line {
            out.lines.entry(line).or_default().add(t.amount)?;
        }
        out.total.add(t.amount)?;
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// The cost report

/// One row's cost figures (see the module docs).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CostFigures {
    pub budget: Cents,
    /// The locked original total, when there is one for this row.
    pub baseline: Option<Cents>,
    pub actual: Cents,
    /// How many transactions make up `actual`.
    pub count: u64,
    pub etc: Cents,
    /// `actual + etc`.
    pub eac: Cents,
    /// `budget - eac`: negative is over budget.
    pub variance: Cents,
    /// `actual / budget` as a percent, two places; `None` for a zero budget.
    pub percent_spent: Option<Dec>,
}

impl CostFigures {
    /// Over budget: the estimate at completion is more than the budget.
    pub fn is_over(&self) -> bool {
        self.variance < 0
    }

    /// Figures for a row with this budget, actual and estimate to complete.
    fn new(budget: Cents, baseline: Option<Cents>, actual: Sum, etc: Cents) -> Option<CostFigures> {
        let eac = actual.amount.checked_add(etc)?;
        Some(CostFigures {
            budget,
            baseline,
            actual: actual.amount,
            count: actual.count,
            etc,
            eac,
            variance: budget.checked_sub(eac)?,
            percent_spent: decimal::percent_ratio(actual.amount, budget),
        })
    }

    /// The sum of `parts` (budget, actual, ETC, EAC and variance added;
    /// percent spent from the sums). The baseline is `baseline`.
    fn sum<'a>(
        parts: impl IntoIterator<Item = &'a CostFigures>,
        baseline: Option<Cents>,
    ) -> Option<CostFigures> {
        let mut out = CostFigures {
            baseline,
            ..CostFigures::default()
        };
        for p in parts {
            out.budget = out.budget.checked_add(p.budget)?;
            out.actual = out.actual.checked_add(p.actual)?;
            out.count += p.count;
            out.etc = out.etc.checked_add(p.etc)?;
            out.eac = out.eac.checked_add(p.eac)?;
            out.variance = out.variance.checked_add(p.variance)?;
        }
        out.percent_spent = decimal::percent_ratio(out.actual, out.budget);
        Some(out)
    }
}

/// The estimate to complete for a row with this budget and actual.
/// `None` on overflow. An out-of-range percent counts as budget minus actual.
pub fn etc_for(method: EtcMethod, budget: Cents, actual: Cents) -> Option<Cents> {
    match method {
        EtcMethod::Manual(c) => Some(c),
        EtcMethod::PercentComplete(p) if method.is_valid() => {
            decimal::whole_from_percent(actual, p)?.checked_sub(actual)
        }
        _ => {
            let left = budget.checked_sub(actual)?;
            // Never past zero, in the budget's own direction (a negative
            // budget is a credit still to come).
            Some(if budget >= 0 {
                left.max(0)
            } else {
                left.min(0)
            })
        }
    }
}

/// An account (or category fringe account) in the cost report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostAccount {
    pub code: String,
    pub name: String,
    pub figures: CostFigures,
    /// Its estimate-to-complete override, if it has one.
    pub method: Option<EtcMethod>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostCategory {
    pub code: String,
    pub name: String,
    pub accounts: Vec<CostAccount>,
    /// The category's fringe account, when fringes post by category.
    pub fringe_account: Option<CostAccount>,
    pub figures: CostFigures,
}

/// One production level (`Report::levels`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostLevel {
    /// Empty for the trailing unnamed level.
    pub name: String,
    /// Indexes into [`CostReport::categories`]: `start..end`.
    pub start: usize,
    pub end: usize,
    /// Fringes posted above its total (by production level), if any.
    pub posted_fringes: Option<CostFigures>,
    pub figures: CostFigures,
}

/// A budget item outside the chart: budget fringes or a charge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostItem {
    pub name: String,
    pub figures: CostFigures,
}

/// Every cost figure of a budget (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostReport {
    /// In topsheet order (the report's categories).
    pub categories: Vec<CostCategory>,
    pub levels: Vec<CostLevel>,
    pub total_atl: CostFigures,
    pub total_btl: CostFigures,
    pub atl_btl: CostFigures,
    /// The budget-wide fringes row (fringes posted by budget), if any.
    pub budget_fringes: Option<CostItem>,
    /// Applied charges, in order.
    pub charges: Vec<CostItem>,
    /// Transactions whose account isn't in the chart, or have none.
    pub unassigned: CostFigures,
    /// The account numbers of the unassigned transactions (`""`: none),
    /// with their sums.
    pub unassigned_codes: BTreeMap<String, Sum>,
    /// Everything: the budget is the grand total.
    pub total: CostFigures,
    /// Sums of the transactions that name a detail row, by row.
    pub lines: BTreeMap<RowId, Sum>,
}

/// The cost report of `b`, whose report `r` was computed from it, with its
/// actuals rolled up as `roll` (see [`rollup`]). `None` on overflow.
pub fn cost_report(b: &Budget, r: &Report, roll: &Rollup) -> Option<CostReport> {
    let coding = b.account_coding;
    let base = b.baseline.as_ref();
    let methods: BTreeMap<String, EtcMethod> = b
        .actuals
        .etc
        .iter()
        .map(|(k, m)| (normalize_code(&coding, k), *m))
        .collect();
    // Each account number's actuals go to the first row with that number.
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    let mut take = |code: &str| -> (String, Sum) {
        let key = normalize_code(&coding, code);
        if key.is_empty() || !claimed.insert(key.clone()) {
            return (key, Sum::default());
        }
        let sum = roll.accounts.get(&key).copied().unwrap_or_default();
        (key, sum)
    };
    let mut account_row =
        |code: &str, name: &str, budget: Cents, baseline: Option<Cents>| -> Option<CostAccount> {
            let (key, actual) = take(code);
            let method = methods.get(&key).copied();
            let etc = etc_for(
                method.unwrap_or(EtcMethod::BudgetMinusActual),
                budget,
                actual.amount,
            )?;
            Some(CostAccount {
                code: code.to_string(),
                name: name.to_string(),
                figures: CostFigures::new(budget, baseline, actual, etc)?,
                method,
            })
        };

    let mut categories = Vec::with_capacity(r.categories.len());
    for cat in &r.categories {
        let mut accounts = Vec::with_capacity(cat.accounts.len());
        for a in &cat.accounts {
            let baseline = base.and_then(|bl| bl.accounts.get(&a.code).copied());
            accounts.push(account_row(&a.code, &a.name, a.total, baseline)?);
        }
        let fringe_account = match &cat.fringe_account {
            Some((code, amount)) => Some(account_row(
                code,
                format!("{} fringes", cat.name).trim(),
                *amount,
                None,
            )?),
            None => None,
        };
        let baseline = base.and_then(|bl| bl.categories.get(&cat.code).copied());
        let figures = CostFigures::sum(
            accounts
                .iter()
                .map(|a| &a.figures)
                .chain(fringe_account.iter().map(|a| &a.figures)),
            baseline,
        )?;
        categories.push(CostCategory {
            code: cat.code.clone(),
            name: cat.name.clone(),
            accounts,
            fringe_account,
            figures,
        });
    }

    // Budget items outside the chart keep their whole budget to complete.
    let outside = |budget: Cents| {
        let etc = etc_for(EtcMethod::BudgetMinusActual, budget, 0)?;
        CostFigures::new(budget, None, Sum::default(), etc)
    };
    let mut levels = Vec::with_capacity(r.levels.len());
    for lv in &r.levels {
        let posted_fringes = if lv.posted_fringes != 0 {
            Some(outside(lv.posted_fringes)?)
        } else {
            None
        };
        let figures = CostFigures::sum(
            categories
                .get(lv.start..lv.end)
                .unwrap_or_default()
                .iter()
                .map(|c| &c.figures)
                .chain(posted_fringes.iter()),
            None,
        )?;
        levels.push(CostLevel {
            name: lv.name.clone(),
            start: lv.start,
            end: lv.end,
            posted_fringes,
            figures,
        });
    }
    let has_levels = b
        .topsheet
        .iter()
        .any(|row| matches!(row, TopsheetRow::ProductionTotal(_)));
    let (atl_levels, btl_levels) = if has_levels && !levels.is_empty() {
        levels.split_at(1)
    } else {
        levels.split_at(0)
    };
    let total_atl = CostFigures::sum(atl_levels.iter().map(|l| &l.figures), None)?;
    let total_btl = CostFigures::sum(btl_levels.iter().map(|l| &l.figures), None)?;
    let atl_btl = CostFigures::sum([&total_atl, &total_btl], None)?;
    let budget_fringes = if r.budget_fringes != 0 {
        Some(CostItem {
            name: "Total fringes".into(),
            figures: outside(r.budget_fringes)?,
        })
    } else {
        None
    };
    let mut charges = Vec::with_capacity(r.charges.len());
    for c in &r.charges {
        charges.push(CostItem {
            name: c.name.clone(),
            figures: outside(c.amount)?,
        });
    }

    // Whatever no row claimed is unassigned: nothing to complete.
    let mut unassigned_codes = BTreeMap::new();
    let mut unassigned_sum = Sum::default();
    for (key, sum) in &roll.accounts {
        if !claimed.contains(key) {
            unassigned_codes.insert(key.clone(), *sum);
            unassigned_sum.add_sum(*sum)?;
        }
    }
    let unassigned = CostFigures::new(0, None, unassigned_sum, 0)?;
    let total = CostFigures::sum(
        [&atl_btl]
            .into_iter()
            .chain(budget_fringes.iter().map(|f| &f.figures))
            .chain(charges.iter().map(|c| &c.figures))
            .chain([&unassigned]),
        base.map(|bl| bl.grand_total),
    )?;
    Some(CostReport {
        categories,
        levels,
        total_atl,
        total_btl,
        atl_btl,
        budget_fringes,
        charges,
        unassigned,
        unassigned_codes,
        total,
        lines: roll.lines.clone(),
    })
}

/// [`rollup`] then [`cost_report`], for callers that don't cache the rollup.
pub fn cost_report_of(b: &Budget, r: &Report) -> Option<CostReport> {
    cost_report(b, r, &rollup(&b.actuals, &b.account_coding)?)
}

// ---------------------------------------------------------------------------
// Listing

/// Which transactions to list. Every field left at its default lets all
/// through.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// An account number (any spelling): only its transactions.
    pub account: Option<String>,
    /// Only transactions whose account isn't in the chart (or have none).
    pub unassigned: bool,
    /// Text the vendor contains (capitals ignored).
    pub vendor: String,
    /// Text the vendor, description, reference or a tag contains.
    pub text: String,
    /// Dates from this one on (`YYYY-MM-DD`), inclusive.
    pub from: String,
    /// Dates up to this one (`YYYY-MM-DD`), inclusive.
    pub to: String,
    /// Only this import's transactions.
    pub batch: Option<BatchId>,
}

/// The transactions `f` lets through, by date (undated last) then id.
pub fn filter<'a>(b: &'a Budget, f: &Filter) -> Vec<&'a Transaction> {
    let coding = b.account_coding;
    let chart = if f.unassigned { Some(chart(b)) } else { None };
    let account = f.account.as_deref().map(|a| normalize_code(&coding, a));
    let vendor = f.vendor.trim().to_lowercase();
    let text = f.text.trim().to_lowercase();
    let from = f.from.trim();
    let to = f.to.trim();
    let mut out: Vec<&Transaction> = b
        .actuals
        .transactions
        .iter()
        .map(|t| &**t)
        .filter(|t| {
            if let Some(a) = &account {
                if normalize_code(&coding, &t.account) != *a {
                    return false;
                }
            }
            if let Some(chart) = &chart {
                if chart.contains_key(&normalize_code(&coding, &t.account)) {
                    return false;
                }
            }
            if !vendor.is_empty() && !t.vendor.to_lowercase().contains(&vendor) {
                return false;
            }
            if !text.is_empty()
                && ![&t.vendor, &t.description, &t.reference]
                    .iter()
                    .any(|s| s.to_lowercase().contains(&text))
                && !t.tags.iter().any(|g| g.to_lowercase().contains(&text))
            {
                return false;
            }
            if !from.is_empty() && (t.date.is_empty() || t.date.as_str() < from) {
                return false;
            }
            if !to.is_empty() && (t.date.is_empty() || t.date.as_str() > to) {
                return false;
            }
            if f.batch.is_some() && t.batch != f.batch {
                return false;
            }
            true
        })
        .collect();
    out.sort_by(|x, y| (x.date.is_empty(), &x.date, x.id).cmp(&(y.date.is_empty(), &y.date, y.id)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Account, Baseline, Category, Charge, ChargeKind, Detail, ProductionTotal};
    use crate::{recalc, RecalcOptions};

    fn d(s: &str) -> Dec {
        s.parse().unwrap()
    }

    fn tx(date: &str, account: &str, amount: Cents, description: &str) -> Transaction {
        Transaction {
            date: date.into(),
            account: account.into(),
            amount,
            description: description.into(),
            vendor: "Vendor".into(),
            ..Transaction::default()
        }
    }

    /// Two categories split by a production total: 10 (accounts 10-01 at
    /// 1,000.00 and 10-02 at 500.00) above the line, 20 (20-01 at 2,000.00)
    /// below, a 10% charge, fringes posted by account.
    fn budget() -> Budget {
        let mut b = Budget::new("Test", "USD");
        b.fringe_posting = FringePosting::Account;
        let acct = |code: &str, id: RowId, amount: &str| Account {
            code: code.into(),
            name: format!("Account {code}"),
            details: vec![Detail::line(id, "Line", amount, "", "1")],
            notes: Vec::new(),
        };
        b.topsheet = vec![
            TopsheetRow::Category(Category {
                code: "10".into(),
                name: "Story".into(),
                accounts: vec![acct("10-01", 1, "1000"), acct("10-02", 2, "500")],
                notes: Vec::new(),
            }),
            TopsheetRow::ProductionTotal(ProductionTotal {
                name: "Total above-the-line".into(),
            }),
            TopsheetRow::Category(Category {
                code: "20".into(),
                name: "Crew".into(),
                accounts: vec![acct("20-01", 3, "2000")],
                notes: Vec::new(),
            }),
        ];
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
        b
    }

    fn import(b: &mut Budget, txs: Vec<Transaction>) -> BatchId {
        add_batch(b, "ledger.csv", "2026-10-09T12:00:00Z", txs).unwrap()
    }

    fn report(b: &Budget) -> CostReport {
        let r = recalc(b, &RecalcOptions::default()).unwrap();
        cost_report_of(b, &r).unwrap()
    }

    #[test]
    fn dates_are_checked() {
        assert!(is_iso_date("2026-10-09"));
        assert!(is_iso_date("2024-02-29"));
        assert!(!is_iso_date("2026-02-29"));
        assert!(!is_iso_date("1900-02-29"));
        assert!(is_iso_date("2000-02-29"));
        assert!(!is_iso_date("2026-13-01"));
        assert!(!is_iso_date("2026-1-01"));
        assert!(!is_iso_date("26-10-09"));
        assert!(!is_iso_date("2026/10/09"));
        assert!(!is_iso_date("+026-10-09"));
        assert_eq!(iso_date(2026, 4, 31), None);
        assert_eq!(iso_date(2026, 4, 30).as_deref(), Some("2026-04-30"));
    }

    #[test]
    fn etc_methods_are_exact_to_the_cent() {
        let bma = EtcMethod::BudgetMinusActual;
        assert_eq!(etc_for(bma, 100_000, 40_000), Some(60_000));
        // Over budget: nothing left, never negative.
        assert_eq!(etc_for(bma, 100_000, 120_000), Some(0));
        // A credit (negative budget) still to come.
        assert_eq!(etc_for(bma, -50_000, -20_000), Some(-30_000));
        assert_eq!(etc_for(bma, -50_000, -60_000), Some(0));
        assert_eq!(
            etc_for(EtcMethod::Manual(12_345), 100_000, 99_999),
            Some(12_345)
        );
        assert_eq!(etc_for(EtcMethod::Manual(-500), 0, 0), Some(-500));
        // 40% complete with 1,000.00 spent: 2,500.00 at completion.
        let pc = |p: &str| EtcMethod::PercentComplete(d(p));
        assert_eq!(etc_for(pc("40"), 0, 100_000), Some(150_000));
        // 1.00 at 30%: 3.33 at completion (rounded once), so 2.33 to go.
        assert_eq!(etc_for(pc("30"), 0, 100), Some(233));
        // 0.01 at 40%: 0.025 rounds away from zero to 0.03.
        assert_eq!(etc_for(pc("40"), 0, 1), Some(2));
        assert_eq!(etc_for(pc("100"), 5_000, 7_000), Some(0));
        // Out of range counts as budget minus actual.
        assert_eq!(etc_for(pc("0"), 5_000, 1_000), Some(4_000));
        assert_eq!(etc_for(pc("150"), 5_000, 1_000), Some(4_000));
        assert_eq!(etc_for(bma, i64::MIN, 1), None);
    }

    #[test]
    fn rollups_by_account_category_and_level() {
        let mut b = budget();
        import(
            &mut b,
            vec![
                tx("2026-09-01", "10-01", 30_000, "Draft"),
                tx("2026-09-02", "1001", 20_000, "Polish"), // same account, other spelling
                tx("2026-09-03", "10-02", 70_000, "Rights"), // over its 500.00
                tx("2026-09-04", "20-01", 50_000, "Gaffer"),
                tx("2026-09-05", "99-01", 1_234, "Not in the chart"),
                tx("2026-09-06", "", 66, "No account"),
            ],
        );
        let c = report(&b);
        let a1 = &c.categories[0].accounts[0].figures;
        assert_eq!((a1.budget, a1.actual, a1.count), (100_000, 50_000, 2));
        assert_eq!((a1.etc, a1.eac, a1.variance), (50_000, 100_000, 0));
        assert_eq!(a1.percent_spent, Some(d("50")));
        let a2 = &c.categories[0].accounts[1].figures;
        assert_eq!(
            (a2.budget, a2.actual, a2.etc, a2.eac),
            (50_000, 70_000, 0, 70_000)
        );
        assert_eq!(a2.variance, -20_000);
        assert!(a2.is_over());
        assert_eq!(a2.percent_spent, Some(d("140")));
        let cat = &c.categories[0].figures;
        assert_eq!(
            (cat.budget, cat.actual, cat.etc, cat.eac, cat.variance),
            (150_000, 120_000, 50_000, 170_000, -20_000)
        );
        assert_eq!(cat.percent_spent, Some(d("80")));
        // Levels: 10 above the line, 20 below.
        assert_eq!(c.levels.len(), 2);
        assert_eq!(c.total_atl.budget, 150_000);
        assert_eq!(c.total_btl.budget, 200_000);
        assert_eq!(c.atl_btl.actual, 170_000);
        // The charge keeps its whole budget to complete.
        assert_eq!(c.charges[0].figures.budget, 35_000);
        assert_eq!(c.charges[0].figures.etc, 35_000);
        assert_eq!(c.charges[0].figures.variance, 0);
        // Unassigned: the unknown number and the blank one.
        assert_eq!(c.unassigned.actual, 1_300);
        assert_eq!(c.unassigned.count, 2);
        assert_eq!((c.unassigned.etc, c.unassigned.variance), (0, -1_300));
        assert_eq!(c.unassigned_codes.keys().collect::<Vec<_>>(), ["", "99-01"]);
        // The total is the grand total, every actual, and everything adds up.
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        assert_eq!(c.total.budget, r.grand_total);
        assert_eq!(c.total.actual, 171_300);
        assert_eq!(c.total.count, 6);
        assert_eq!(c.total.eac, c.total.actual + c.total.etc);
        assert_eq!(c.total.variance, c.total.budget - c.total.eac);
        // Rows that name a line.
        assert!(c.lines.is_empty());
    }

    #[test]
    fn overrides_change_one_account() {
        let mut b = budget();
        import(&mut b, vec![tx("2026-09-01", "20-01", 50_000, "Gaffer")]);
        set_etc(&mut b, "2001", Some(EtcMethod::PercentComplete(d("25")))).unwrap();
        let c = report(&b);
        let a = &c.categories[1].accounts[0];
        assert_eq!(a.method, Some(EtcMethod::PercentComplete(d("25"))));
        assert_eq!(
            (a.figures.etc, a.figures.eac, a.figures.variance),
            (150_000, 200_000, 0)
        );
        set_etc(&mut b, "20-01", Some(EtcMethod::Manual(10_000))).unwrap();
        assert_eq!(b.actuals.etc.len(), 1, "one override per account");
        let c = report(&b);
        assert_eq!(c.categories[1].accounts[0].figures.eac, 60_000);
        // Budget minus actual takes the override away.
        set_etc(&mut b, "20-01", Some(EtcMethod::BudgetMinusActual)).unwrap();
        assert!(b.actuals.etc.is_empty());
        assert!(set_etc(&mut b, "99-99", Some(EtcMethod::Manual(1))).is_err());
        assert!(set_etc(&mut b, "20-01", Some(EtcMethod::PercentComplete(d("0")))).is_err());
        assert!(set_etc(
            &mut b,
            "20-01",
            Some(EtcMethod::PercentComplete(d("100.1")))
        )
        .is_err());
    }

    #[test]
    fn category_fringe_accounts_and_baselines() {
        let mut b = budget();
        b.fringe_posting = FringePosting::Category {
            suffix: "99".into(),
        };
        b.fringes.insert(
            "TAX".into(),
            crate::model::Fringe {
                description: String::new(),
                short_code: None,
                rate: "10".into(),
                kind: crate::model::FringeKind::Percent { cutoff: None },
            },
        );
        for c in b.categories_mut() {
            for a in &mut c.accounts {
                for dd in &mut a.details {
                    dd.fringes.insert("TAX".into());
                }
            }
        }
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        b.baseline = Baseline::capture(&r);
        import(
            &mut b,
            vec![tx("2026-09-01", "10-99", 4_000, "Payroll taxes")],
        );
        let c = report(&b);
        let fa = c.categories[0].fringe_account.as_ref().unwrap();
        assert_eq!(fa.code, "10-99");
        assert_eq!(
            (fa.figures.budget, fa.figures.actual, fa.figures.etc),
            (15_000, 4_000, 11_000)
        );
        assert_eq!(c.categories[0].figures.budget, 165_000);
        assert_eq!(c.categories[0].figures.baseline, Some(165_000));
        assert_eq!(c.categories[0].accounts[0].figures.baseline, Some(100_000));
        assert_eq!(c.total.baseline, Some(r.grand_total));
        assert_eq!(c.unassigned.actual, 0);
        // The fringe account is in the chart for edits too.
        assert_eq!(chart_code(&b, "1099").unwrap(), "10-99");
    }

    #[test]
    fn shared_numbers_count_once() {
        let mut b = budget();
        if let TopsheetRow::Category(c) = &mut b.topsheet[2] {
            let mut twin = c.accounts[0].clone();
            twin.details[0].id = 9;
            c.accounts.push(twin);
        }
        import(&mut b, vec![tx("2026-09-01", "20-01", 10_000, "Once")]);
        let c = report(&b);
        assert_eq!(c.categories[1].accounts[0].figures.actual, 10_000);
        assert_eq!(c.categories[1].accounts[1].figures.actual, 0);
        assert_eq!(c.total.actual, 10_000);
    }

    #[test]
    fn numbers_the_pattern_would_cut_short_stay_unassigned() {
        let mut b = budget();
        import(
            &mut b,
            vec![
                tx("2026-09-01", "10-1", 100, "Padded: 10-01"),
                tx("2026-09-02", "10-012", 2_000, "A piece too long"),
                tx(
                    "2026-09-03",
                    "100123",
                    30_000,
                    "More digits than the pattern",
                ),
            ],
        );
        let c = report(&b);
        assert_eq!(c.categories[0].accounts[0].figures.actual, 100);
        assert_eq!(c.unassigned.actual, 32_000);
        assert_eq!(
            c.unassigned_codes.keys().collect::<Vec<_>>(),
            ["10-012", "100123"]
        );
        assert_eq!(normalize_code(&b.account_coding, " 10-012 "), "10-012");
        assert_eq!(normalize_code(&b.account_coding, "1001"), "10-01");
        assert!(chart_code(&b, "10-012").is_err());
        let unassigned = Filter {
            unassigned: true,
            ..Filter::default()
        };
        assert_eq!(filter(&b, &unassigned).len(), 2);
    }

    #[test]
    fn line_rollups() {
        let mut b = budget();
        let mut t = tx("2026-09-01", "10-01", 2_500, "Draft");
        t.line = Some(1);
        import(&mut b, vec![t, tx("2026-09-01", "10-01", 100, "Other")]);
        let c = report(&b);
        assert_eq!(
            c.lines.get(&1),
            Some(&Sum {
                amount: 2_500,
                count: 1
            })
        );
        assert_eq!(c.categories[0].accounts[0].figures.actual, 2_600);
    }

    #[test]
    fn batches_come_and_go_whole() {
        let mut b = budget();
        let first = import(
            &mut b,
            vec![
                tx("2026-09-01", "10-01", 1, "a"),
                tx("2026-09-01", "10-01", 2, "b"),
            ],
        );
        let second = import(&mut b, vec![tx("2026-09-02", "10-02", 3, "c")]);
        assert_eq!((first, second), (1, 2));
        let ids: Vec<_> = b
            .actuals
            .transactions
            .iter()
            .map(|t| (t.id, t.batch))
            .collect();
        assert_eq!(ids, [(1, Some(1)), (2, Some(1)), (3, Some(2))]);
        assert_eq!(b.actuals.batches[0].rows, 2);
        assert_eq!(remove_batch(&mut b, first).unwrap(), 2);
        assert_eq!(b.actuals.transactions.len(), 1);
        assert_eq!(b.actuals.batches.len(), 1);
        assert_eq!(
            remove_batch(&mut b, first),
            Err(ActualsError::BatchNotFound(1))
        );
        // New ids never reuse a removed batch's while its rows remain.
        let third = import(&mut b, vec![tx("2026-09-03", "", 4, "d")]);
        assert_eq!(third, 3);
        assert!(add_batch(&mut b, "x.csv", "now", Vec::new()).is_err());
        let bad = Transaction {
            date: "2026-02-30".into(),
            ..Transaction::default()
        };
        assert!(add_batch(&mut b, "x.csv", "now", vec![bad]).is_err());
    }

    #[test]
    fn edits_reassign_and_delete() {
        let mut b = budget();
        let mut t = tx("2026-09-01", "10-01", 1_000, "Draft");
        t.line = Some(1);
        import(&mut b, vec![t, tx("2026-09-02", "99-99", 2_000, "Unknown")]);
        // Reassigning drops a line that isn't in the new account.
        let n = edit_transactions(
            &mut b,
            &[1],
            &TransactionEdit {
                account: Some("1002".into()),
                ..TransactionEdit::default()
            },
        )
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(b.actuals.transactions[0].account, "10-02");
        assert_eq!(b.actuals.transactions[0].line, None);
        // A line must be in the account.
        let e = TransactionEdit {
            line: Some(Some(1)),
            ..TransactionEdit::default()
        };
        assert!(edit_transactions(&mut b, &[1], &e).is_err());
        let e = TransactionEdit {
            line: Some(Some(2)),
            vendor: Some(" Acme ".into()),
            tags: Some(["PO".to_string(), " ".to_string()].into()),
            amount: Some(-50),
            date: Some("2026-09-30".into()),
            ..TransactionEdit::default()
        };
        edit_transactions(&mut b, &[1], &e).unwrap();
        let t = &b.actuals.transactions[0];
        assert_eq!(
            (t.line, t.vendor.as_str(), t.amount),
            (Some(2), "Acme", -50)
        );
        assert_eq!(t.tags.iter().collect::<Vec<_>>(), ["PO"]);
        // Accounts not in the chart, bad dates and unknown ids are refused,
        // and a refusal changes nothing.
        let before = b.clone();
        for e in [
            TransactionEdit {
                account: Some("88-88".into()),
                ..TransactionEdit::default()
            },
            TransactionEdit {
                date: Some("09/30/2026".into()),
                ..TransactionEdit::default()
            },
        ] {
            assert!(edit_transactions(&mut b, &[1, 2], &e).is_err());
            assert_eq!(b, before);
        }
        assert_eq!(
            edit_transactions(&mut b, &[7], &TransactionEdit::default()),
            Err(ActualsError::NotFound(7))
        );
        // Unassigning is a blank account.
        let e = TransactionEdit {
            account: Some(String::new()),
            ..TransactionEdit::default()
        };
        edit_transactions(&mut b, &[1], &e).unwrap();
        assert_eq!(b.actuals.transactions[0].account, "");
        assert_eq!(b.actuals.transactions[0].line, None);
        assert_eq!(delete_transactions(&mut b, &[1, 2]).unwrap(), 2);
        assert!(b.actuals.transactions.is_empty());
        assert!(delete_transactions(&mut b, &[1]).is_err());
    }

    #[test]
    fn unchanged_transactions_stay_shared() {
        let mut b = budget();
        import(
            &mut b,
            vec![
                tx("2026-09-01", "10-01", 1, "a"),
                tx("2026-09-01", "10-01", 2, "b"),
            ],
        );
        let copy = b.clone();
        assert!(
            Arc::ptr_eq(&copy.actuals, &b.actuals),
            "a copy is a pointer"
        );
        let e = TransactionEdit {
            amount: Some(5),
            ..TransactionEdit::default()
        };
        edit_transactions(&mut b, &[2], &e).unwrap();
        assert!(Arc::ptr_eq(
            &copy.actuals.transactions[0],
            &b.actuals.transactions[0]
        ));
        assert!(!Arc::ptr_eq(
            &copy.actuals.transactions[1],
            &b.actuals.transactions[1]
        ));
        assert_eq!(copy.actuals.transactions[1].amount, 2);
    }

    #[test]
    fn duplicates_by_date_account_amount_and_description() {
        let mut b = budget();
        import(
            &mut b,
            vec![
                tx("2026-09-01", "10-01", 1_000, "Lens  rental"),
                tx("2026-09-01", "1001", 1_000, " lens rental "),
                tx("2026-09-01", "10-01", 1_001, "Lens rental"),
                tx("2026-09-02", "10-01", 1_000, "Lens rental"),
            ],
        );
        let coding = b.account_coding;
        assert_eq!(duplicate_groups(&b.actuals, &coding), vec![vec![1, 2]]);
        let keys = duplicate_keys(&b.actuals, &coding);
        let k = duplicate_key(&coding, "2026-09-01", "10.01", 1_000, "LENS RENTAL");
        assert_eq!(keys.get(&k), Some(&2));
    }

    #[test]
    fn filters_list_by_account_vendor_text_and_date() {
        let mut b = budget();
        let mut t = tx("2026-09-03", "10-01", 1, "Lens rental");
        t.vendor = "Acme Cameras".into();
        t.tags.insert("PO".into());
        import(
            &mut b,
            vec![
                t,
                tx("2026-09-01", "1002", 2, "Script"),
                tx("", "77-77", 3, "Mystery"),
                tx("2026-09-02", "", 4, "Cash"),
            ],
        );
        let ids = |f: &Filter| filter(&b, f).iter().map(|t| t.id).collect::<Vec<_>>();
        assert_eq!(
            ids(&Filter::default()),
            [2, 4, 1, 3],
            "by date, undated last"
        );
        let f = Filter {
            account: Some("10-02".into()),
            ..Filter::default()
        };
        assert_eq!(ids(&f), [2]);
        let f = Filter {
            unassigned: true,
            ..Filter::default()
        };
        assert_eq!(ids(&f), [4, 3]);
        let f = Filter {
            vendor: "acme".into(),
            ..Filter::default()
        };
        assert_eq!(ids(&f), [1]);
        let f = Filter {
            text: "po".into(),
            ..Filter::default()
        };
        assert_eq!(ids(&f), [1]);
        let f = Filter {
            from: "2026-09-02".into(),
            to: "2026-09-03".into(),
            ..Filter::default()
        };
        assert_eq!(ids(&f), [4, 1]);
        let f = Filter {
            batch: Some(9),
            ..Filter::default()
        };
        assert!(ids(&f).is_empty());
    }

    #[test]
    fn rollup_overflow_is_none() {
        let mut b = budget();
        import(
            &mut b,
            vec![tx("", "10-01", i64::MAX, "a"), tx("", "10-01", 1, "b")],
        );
        assert_eq!(rollup(&b.actuals, &b.account_coding), None);
    }
}
