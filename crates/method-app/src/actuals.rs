//! Actuals and the cost report (roadmap 3.7; the engine is
//! `method_core::actuals`, the spreadsheet reading `method_format::import::actuals`).
//!
//! - **File > Import Actuals…**: Rust shows the Open dialog and reads the
//!   file (CSV, tab-delimited or a workbook, as Import Spreadsheet does);
//!   the page names it afterwards by a token. The dialog maps columns
//!   (guessed from the titles), picks the date order and sign, reviews the
//!   account numbers the chart doesn't have (map, skip or keep unassigned)
//!   and the duplicates, and imports as one undo step: one import batch,
//!   which the Actuals tab can take back out whole.
//! - **The Actuals tab** lists transactions with filters, edits, reassigns
//!   and deletes them, removes imports, and sets an account's estimate to
//!   complete. Each change is one undo step.
//! - **The cost columns**: every view of a budget with actuals carries
//!   [`CostDto`], the cost figures of every account, category, level and
//!   summary row (and the lines of the account on screen), from the
//!   engine's cost report. The only pass over every transaction (the
//!   rollup) is cached by the session until the actuals or the account
//!   pattern change (`Session::cost`), so an edit to a line costs one pass
//!   over the accounts, not over the transactions.
//!
//! Everything but the commands at the end is plain functions on budgets, so
//! it is tested without a window.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use method_core::actuals::{
    self, CostAccount, CostFigures, CostReport, EtcMethod, Filter, Rollup, Sum, TransactionEdit,
    TransactionId,
};
use method_core::{Budget, Cents, Report, RowId, TopsheetRow};
use method_format::import::actuals::{self as ia, CodeChoice, DateOrder, Field, Mapping, Options};
use method_format::import::mapping::{self as sheet_mapping, parse_number_text};
use method_format::import::{read_file, Book, FileKind, Table};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{edit_budget, finish, AppState, DocId};
use crate::history::count;
use crate::session::Session;
use crate::sheet_import::clip;
use crate::view::View;

/// Rows of the sheet shown raw, for picking the header row.
const RAW_ROWS: usize = 12;
const RAW_COLS: usize = 40;
const RAW_CHARS: usize = 60;
/// Transactions shown as they will import.
pub const PREVIEW_ROWS: usize = 25;
/// Skipped rows, warnings and duplicates listed (the counts are complete).
pub const NOTES_LISTED: usize = 50;
/// Most transactions one list shows (the count and total are complete).
pub const LIST_LIMIT: usize = 500;

// ---------------------------------------------------------------------------
// The cost figures every view carries

/// One row's cost figures, as the page draws them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FiguresDto {
    pub budget: Cents,
    pub baseline: Option<Cents>,
    pub actual: Cents,
    pub count: u64,
    pub etc: Cents,
    pub eac: Cents,
    pub variance: Cents,
    /// Percent spent with two decimals (`"12.5"`), or `null` for a zero budget.
    pub percent: Option<String>,
    /// Over budget (variance below zero).
    pub over: bool,
}

impl From<&CostFigures> for FiguresDto {
    fn from(f: &CostFigures) -> FiguresDto {
        FiguresDto {
            budget: f.budget,
            baseline: f.baseline,
            actual: f.actual,
            count: f.count,
            etc: f.etc,
            eac: f.eac,
            variance: f.variance,
            percent: f.percent_spent.map(|p| p.to_string()),
            over: f.is_over(),
        }
    }
}

/// An estimate-to-complete method as the page reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MethodDto {
    /// `manual` or `percent_complete` (budget minus actual has none).
    pub kind: &'static str,
    /// The manual amount, in cents.
    pub amount: Option<Cents>,
    /// The percent complete (`"40"`).
    pub percent: Option<String>,
}

fn method_dto(m: Option<EtcMethod>) -> Option<MethodDto> {
    match m? {
        EtcMethod::BudgetMinusActual => None,
        EtcMethod::Manual(c) => Some(MethodDto {
            kind: "manual",
            amount: Some(c),
            percent: None,
        }),
        EtcMethod::PercentComplete(p) => Some(MethodDto {
            kind: "percent_complete",
            amount: None,
            percent: Some(p.to_string()),
        }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CostAccountDto {
    pub code: String,
    pub figures: FiguresDto,
    pub method: Option<MethodDto>,
}

impl From<&CostAccount> for CostAccountDto {
    fn from(a: &CostAccount) -> CostAccountDto {
        CostAccountDto {
            code: a.code.clone(),
            figures: FiguresDto::from(&a.figures),
            method: method_dto(a.method),
        }
    }
}

/// A category: its accounts in the chart's order (as the report lists them).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CostCategoryDto {
    pub code: String,
    pub name: String,
    pub figures: FiguresDto,
    pub accounts: Vec<CostAccountDto>,
    pub fringe_account: Option<CostAccountDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CostLevelDto {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub figures: FiguresDto,
    pub posted_fringes: Option<FiguresDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CostItemDto {
    pub name: String,
    pub figures: FiguresDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SumDto {
    pub code: String,
    pub amount: Cents,
    pub count: u64,
}

/// The cost report for the page: in step with the view's `report`
/// (categories and levels by position).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CostDto {
    /// Transactions in the budget, and imports.
    pub transactions: usize,
    pub batches: usize,
    pub categories: Vec<CostCategoryDto>,
    pub levels: Vec<CostLevelDto>,
    pub total_atl: FiguresDto,
    pub total_btl: FiguresDto,
    pub atl_btl: FiguresDto,
    pub budget_fringes: Option<CostItemDto>,
    pub charges: Vec<CostItemDto>,
    pub unassigned: FiguresDto,
    /// The numbers unassigned transactions carry (`""`: none).
    pub unassigned_codes: Vec<SumDto>,
    pub total: FiguresDto,
    /// Transactions that name a detail row, by row id: the lean view's
    /// account on screen only, every row in the full view.
    pub lines: BTreeMap<RowId, SumDto>,
    /// Problems working it out (a sum too large), if any.
    pub error: Option<String>,
}

/// The cost figures of `b` (whose report is `r`) with its actuals rolled up
/// as `roll` (`None`: the sums overflowed); `lines` keeps the line sums
/// whose rows it lets through.
pub fn cost_dto(
    b: &Budget,
    r: &Report,
    roll: Option<&Rollup>,
    lines: impl Fn(RowId) -> bool,
) -> CostDto {
    match roll.and_then(|roll| actuals::cost_report(b, r, roll)) {
        Some(c) => cost_dto_of(b, &c, lines),
        None => CostDto {
            transactions: b.actuals.transactions.len(),
            batches: b.actuals.batches.len(),
            categories: Vec::new(),
            levels: Vec::new(),
            total_atl: FiguresDto::from(&CostFigures::default()),
            total_btl: FiguresDto::from(&CostFigures::default()),
            atl_btl: FiguresDto::from(&CostFigures::default()),
            budget_fringes: None,
            charges: Vec::new(),
            unassigned: FiguresDto::from(&CostFigures::default()),
            unassigned_codes: Vec::new(),
            total: FiguresDto::from(&CostFigures::default()),
            lines: BTreeMap::new(),
            error: Some("The actuals add up to more than the app can hold.".into()),
        },
    }
}

fn cost_dto_of(b: &Budget, c: &CostReport, lines: impl Fn(RowId) -> bool) -> CostDto {
    let sum = |code: &str, s: &Sum| SumDto {
        code: code.to_string(),
        amount: s.amount,
        count: s.count,
    };
    CostDto {
        transactions: b.actuals.transactions.len(),
        batches: b.actuals.batches.len(),
        categories: c
            .categories
            .iter()
            .map(|cat| CostCategoryDto {
                code: cat.code.clone(),
                name: cat.name.clone(),
                figures: FiguresDto::from(&cat.figures),
                accounts: cat.accounts.iter().map(CostAccountDto::from).collect(),
                fringe_account: cat.fringe_account.as_ref().map(CostAccountDto::from),
            })
            .collect(),
        levels: c
            .levels
            .iter()
            .map(|l| CostLevelDto {
                name: l.name.clone(),
                start: l.start,
                end: l.end,
                figures: FiguresDto::from(&l.figures),
                posted_fringes: l.posted_fringes.as_ref().map(FiguresDto::from),
            })
            .collect(),
        total_atl: FiguresDto::from(&c.total_atl),
        total_btl: FiguresDto::from(&c.total_btl),
        atl_btl: FiguresDto::from(&c.atl_btl),
        budget_fringes: c.budget_fringes.as_ref().map(|f| CostItemDto {
            name: f.name.clone(),
            figures: FiguresDto::from(&f.figures),
        }),
        charges: c
            .charges
            .iter()
            .map(|x| CostItemDto {
                name: x.name.clone(),
                figures: FiguresDto::from(&x.figures),
            })
            .collect(),
        unassigned: FiguresDto::from(&c.unassigned),
        unassigned_codes: c.unassigned_codes.iter().map(|(k, s)| sum(k, s)).collect(),
        total: FiguresDto::from(&c.total),
        lines: c
            .lines
            .iter()
            .filter(|(id, _)| lines(**id))
            .map(|(id, s)| (*id, sum("", s)))
            .collect(),
        error: None,
    }
}

/// The rollup cache a session keeps: the actuals it was made from (kept
/// alive here, so a pointer match means the very same actuals) and the
/// account pattern.
#[derive(Debug, Default)]
pub struct RollupCache {
    entry: Option<(
        Arc<method_core::actuals::Actuals>,
        method_core::coding::AccountCoding,
        Option<Arc<Rollup>>,
    )>,
    /// How many times the rollup was worked out (for tests).
    pub computed: u64,
}

impl RollupCache {
    /// `b`'s actuals rolled up, worked out again only when they or the
    /// account pattern changed. `None` on overflow.
    pub fn get(&mut self, b: &Budget) -> Option<Arc<Rollup>> {
        if let Some((a, coding, roll)) = &self.entry {
            if Arc::ptr_eq(a, &b.actuals) && *coding == b.account_coding {
                return roll.clone();
            }
        }
        self.computed += 1;
        let roll = actuals::rollup(&b.actuals, &b.account_coding).map(Arc::new);
        self.entry = Some((Arc::clone(&b.actuals), b.account_coding, roll.clone()));
        roll
    }
}

// ---------------------------------------------------------------------------
// The file, kept by token between the dialog's calls

struct Source {
    token: u64,
    file: String,
    book: Arc<Book>,
}

/// The last file picked to import actuals from (the app's state).
#[derive(Default)]
pub struct ActualsSources {
    inner: Mutex<(u64, Option<Source>)>,
}

impl ActualsSources {
    fn lock(&self) -> MutexGuard<'_, (u64, Option<Source>)> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn keep(&self, file: String, book: Book) -> u64 {
        let mut g = self.lock();
        g.0 += 1;
        let token = g.0;
        g.1 = Some(Source {
            token,
            file,
            book: Arc::new(book),
        });
        token
    }

    pub fn get(&self, token: u64) -> Result<(String, Arc<Book>), String> {
        let g = self.lock();
        g.1.as_ref()
            .filter(|s| s.token == token)
            .map(|s| (s.file.clone(), Arc::clone(&s.book)))
            .ok_or_else(|| "Another file was picked since. Import again.".to_string())
    }
}

// ---------------------------------------------------------------------------
// The import dialog

#[derive(Debug, Serialize)]
pub struct SheetInfo {
    pub name: String,
    pub hidden: bool,
    pub rows: usize,
    pub columns: usize,
    pub truncated: bool,
}

/// What `actuals_pick` returns.
#[derive(Debug, Serialize)]
pub struct Picked {
    pub token: u64,
    /// The file's name (never its path).
    pub file: String,
    pub kind: &'static str,
    pub sheets: Vec<SheetInfo>,
    pub sheet: usize,
    pub fields: Vec<&'static str>,
}

/// One unmatched account number's choice, as the page sends it.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct CodeChoiceInput {
    /// `keep`, `skip` or `map`.
    pub choice: String,
    /// The account to map to (with `map`).
    #[serde(default)]
    pub account: String,
}

/// The dialog's choices.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Request {
    pub sheet: usize,
    pub header_row: Option<usize>,
    #[serde(default)]
    pub mapping: BTreeMap<String, usize>,
    #[serde(default)]
    pub detect_header: bool,
    #[serde(default)]
    pub guess: bool,
    /// `month_first`, `day_first`, or empty to find it from the column.
    #[serde(default)]
    pub date_order: String,
    #[serde(default)]
    pub negate: bool,
    #[serde(default)]
    pub carry_account: bool,
    /// Unmatched account number (its key) -> what to do with its rows.
    #[serde(default)]
    pub codes: BTreeMap<String, CodeChoiceInput>,
    /// Skip rows matching a transaction the budget has (on unless sent).
    #[serde(default = "yes")]
    pub skip_existing: bool,
    #[serde(default)]
    pub skip_repeats: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct ColumnInfo {
    pub index: usize,
    pub letter: String,
    pub title: String,
    pub sample: String,
}

#[derive(Debug, Serialize)]
pub struct RawRow {
    pub row: usize,
    pub cells: Vec<String>,
}

/// A transaction as it will import.
#[derive(Debug, Serialize)]
pub struct PlannedDto {
    pub row: usize,
    pub date: String,
    pub account: String,
    /// The account's name, when the chart has it.
    pub account_name: String,
    pub assigned: bool,
    pub vendor: String,
    pub description: String,
    pub reference: String,
    pub tags: String,
    pub amount: Cents,
}

#[derive(Debug, Serialize)]
pub struct RowNoteDto {
    pub row: usize,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct UnmatchedDto {
    pub key: String,
    pub text: String,
    pub rows: usize,
    pub amount: Cents,
    /// `keep`, `skip` or `map`.
    pub choice: &'static str,
    pub account: String,
}

#[derive(Debug, Serialize)]
pub struct DuplicateDto {
    pub row: usize,
    /// The earlier row it repeats; `null`: it matches a transaction the
    /// budget has.
    pub repeats: Option<usize>,
    pub skipped: bool,
    pub date: String,
    pub account: String,
    pub amount: Cents,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct ChartDto {
    pub code: String,
    pub name: String,
}

/// What `actuals_preview` returns.
#[derive(Debug, Serialize)]
pub struct PreviewDto {
    pub sheet: usize,
    pub header_row: Option<usize>,
    pub signature: String,
    pub mapping: BTreeMap<&'static str, usize>,
    pub columns: Vec<ColumnInfo>,
    pub raw: Vec<RawRow>,
    pub transactions: Vec<PlannedDto>,
    pub count: usize,
    pub total: Cents,
    pub skipped: Vec<RowNoteDto>,
    pub skipped_count: usize,
    pub warnings: Vec<RowNoteDto>,
    pub warning_count: usize,
    pub headings: usize,
    pub unmatched: Vec<UnmatchedDto>,
    pub duplicates: Vec<DuplicateDto>,
    pub duplicate_count: usize,
    pub duplicates_skipped: usize,
    pub date_order: &'static str,
    pub date_order_found: bool,
    /// Accounts in the chart that get transactions.
    pub accounts: usize,
    /// Transactions that will import unassigned.
    pub unassigned: usize,
    /// The chart, for mapping unmatched numbers.
    pub chart: Vec<ChartDto>,
    /// The budget's actual to date now, and after the import.
    pub actual_before: Cents,
    pub actual_after: Cents,
    pub truncated: bool,
}

/// What `actuals_import` did.
#[derive(Debug, Serialize)]
pub struct Imported {
    pub view: View,
    pub message: String,
}

/// The sheet to start on: the visible one whose titles name the most
/// actuals fields.
fn best_sheet(book: &Book) -> usize {
    let mut best: Option<(usize, usize)> = None;
    for (i, t) in book.tables.iter().enumerate() {
        if t.hidden || t.rows.is_empty() {
            continue;
        }
        let n = ia::guess(t, ia::detect_header(t)).len();
        if best.is_none_or(|(b, _)| n > b) {
            best = Some((n, i));
        }
    }
    best.map_or(0, |(_, i)| i)
}

/// What the dialog starts with for `book`.
pub fn picked(token: u64, file: String, book: &Book) -> Picked {
    Picked {
        token,
        file,
        kind: match book.kind {
            FileKind::Xlsx => "xlsx",
            FileKind::Csv => "csv",
            FileKind::Tsv => "tsv",
        },
        sheets: book
            .tables
            .iter()
            .map(|t| SheetInfo {
                name: t.name.clone(),
                hidden: t.hidden,
                rows: t.rows.len(),
                columns: t.width(),
                truncated: t.truncated,
            })
            .collect(),
        sheet: best_sheet(book),
        fields: Field::ALL.iter().map(|f| f.key()).collect(),
    }
}

fn resolve<'b>(book: &'b Book, req: &Request) -> Result<(&'b Table, Mapping), String> {
    let t = book
        .tables
        .get(req.sheet)
        .ok_or("That sheet isn't in the file.")?;
    let header_row = if req.detect_header {
        ia::detect_header(t)
    } else {
        if let Some(h) = req.header_row {
            if h >= t.rows.len().max(1) {
                return Err("The header row is past the end of the sheet.".into());
            }
        }
        req.header_row
    };
    if req.guess {
        return Ok((
            t,
            Mapping {
                header_row,
                columns: ia::guess(t, header_row),
            },
        ));
    }
    let width = t.width();
    let mut columns = BTreeMap::new();
    for (key, &col) in &req.mapping {
        let f = Field::parse(key).ok_or_else(|| format!("unknown field `{key}`"))?;
        if col >= width {
            return Err(format!(
                "The {} column is past the sheet's last column.",
                f.name()
            ));
        }
        columns.insert(f, col);
    }
    Ok((
        t,
        Mapping {
            header_row,
            columns,
        },
    ))
}

fn options(req: &Request) -> Result<Options, String> {
    let date_order = match req.date_order.as_str() {
        "" => None,
        s => Some(DateOrder::parse(s).ok_or_else(|| format!("unknown date order `{s}`"))?),
    };
    let mut codes = BTreeMap::new();
    for (key, c) in &req.codes {
        if key.chars().count() > 64 || c.account.chars().count() > 64 {
            return Err("An account number is too long.".into());
        }
        let choice = match c.choice.as_str() {
            "keep" => CodeChoice::Keep,
            "skip" => CodeChoice::Skip,
            "map" if c.account.trim().is_empty() => CodeChoice::Keep,
            "map" => CodeChoice::Map(c.account.trim().to_string()),
            other => return Err(format!("unknown choice `{other}`")),
        };
        codes.insert(key.clone(), choice);
    }
    Ok(Options {
        date_order,
        negate: req.negate,
        carry_account: req.carry_account,
        codes,
        skip_existing: req.skip_existing,
        skip_repeats: req.skip_repeats,
    })
}

fn chart_list(b: &Budget) -> Vec<ChartDto> {
    // In chart order: accounts as the topsheet lists them, fringe accounts
    // after their category's accounts.
    let chart = actuals::chart(b);
    let coding = b.account_coding;
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |code: &str, out: &mut Vec<ChartDto>| {
        let key = actuals::normalize_code(&coding, code);
        if let Some(e) = chart.get(&key) {
            if seen.insert(key) {
                out.push(ChartDto {
                    code: e.code.clone(),
                    name: e.name.clone(),
                });
            }
        }
    };
    for row in &b.topsheet {
        if let TopsheetRow::Category(c) = row {
            for a in &c.accounts {
                push(&a.code, &mut out);
            }
            if let method_core::FringePosting::Category { suffix } = &b.fringe_posting {
                let number = coding.category_fringe_account(&c.code, suffix);
                push(&number, &mut out);
            }
        }
    }
    out
}

/// The preview of importing `req` from `book` into budget `b`.
pub fn preview_dto(b: &Budget, book: &Book, req: &Request) -> Result<PreviewDto, String> {
    let (t, m) = resolve(book, req)?;
    let o = options(req)?;
    let p = ia::plan(b, t, &m, &o);
    let chart = actuals::chart(b);
    let coding = b.account_coding;
    let first = m.first_data_row();
    let titles = sheet_mapping::titles(t, m.header_row);
    let columns = (0..t.width())
        .map(|c| {
            let sample = (first..t.rows.len())
                .map(|r| t.cell(r, c).text().trim())
                .find(|s| !s.is_empty())
                .unwrap_or("");
            ColumnInfo {
                index: c,
                letter: method_format::export::xlsx::column_name(c),
                title: clip(&titles[c], RAW_CHARS),
                sample: clip(sample, 40),
            }
        })
        .collect();
    let raw = (0..t.rows.len().min(RAW_ROWS))
        .map(|r| RawRow {
            row: r + 1,
            cells: (0..t.width().min(RAW_COLS))
                .map(|c| clip(t.cell(r, c).text(), RAW_CHARS))
                .collect(),
        })
        .collect();
    let notes = |v: &[method_format::import::merge::RowNote]| -> Vec<RowNoteDto> {
        v.iter()
            .take(NOTES_LISTED)
            .map(|n| RowNoteDto {
                row: n.row,
                message: n.message.clone(),
            })
            .collect()
    };
    let name_of = |code: &str| {
        chart
            .get(&actuals::normalize_code(&coding, code))
            .map(|e| e.name.clone())
            .unwrap_or_default()
    };
    let before: Cents = b
        .actuals
        .transactions
        .iter()
        .fold(0i64, |s, t| s.saturating_add(t.amount));
    Ok(PreviewDto {
        sheet: req.sheet,
        header_row: m.header_row,
        signature: sheet_mapping::signature(t, m.header_row),
        mapping: m.columns.iter().map(|(f, c)| (f.key(), *c)).collect(),
        columns,
        raw,
        transactions: p
            .transactions
            .iter()
            .take(PREVIEW_ROWS)
            .map(|x| {
                let t = &x.transaction;
                PlannedDto {
                    row: x.row,
                    date: t.date.clone(),
                    account: t.account.clone(),
                    account_name: if x.assigned {
                        name_of(&t.account)
                    } else {
                        String::new()
                    },
                    assigned: x.assigned,
                    vendor: clip(&t.vendor, 80),
                    description: clip(&t.description, 120),
                    reference: clip(&t.reference, 40),
                    tags: t.tags.iter().cloned().collect::<Vec<_>>().join(", "),
                    amount: t.amount,
                }
            })
            .collect(),
        count: p.transactions.len(),
        total: p.total,
        skipped: notes(&p.skipped),
        skipped_count: p.skipped.len(),
        warnings: notes(&p.warnings),
        warning_count: p.warnings.len(),
        headings: p.headings,
        unmatched: p
            .unmatched
            .iter()
            .map(|u| {
                let (choice, account) = match &u.choice {
                    CodeChoice::Keep => ("keep", String::new()),
                    CodeChoice::Skip => ("skip", String::new()),
                    CodeChoice::Map(a) => ("map", a.clone()),
                };
                UnmatchedDto {
                    key: u.key.clone(),
                    text: clip(&u.text, 40),
                    rows: u.rows,
                    amount: u.amount,
                    choice,
                    account,
                }
            })
            .collect(),
        duplicates: p
            .duplicates
            .iter()
            .take(NOTES_LISTED)
            .map(|d| DuplicateDto {
                row: d.row,
                repeats: d.repeats,
                skipped: d.skipped,
                date: d.date.clone(),
                account: d.account.clone(),
                amount: d.amount,
                description: clip(&d.description, 120),
            })
            .collect(),
        duplicate_count: p.duplicates.len(),
        duplicates_skipped: p.duplicates.iter().filter(|d| d.skipped).count(),
        date_order: p.date_order.key(),
        date_order_found: p.date_order_found,
        accounts: p.accounts.len(),
        unassigned: p.transactions.iter().filter(|t| !t.assigned).count(),
        chart: chart_list(b),
        actual_before: before,
        actual_after: before.saturating_add(p.total),
        truncated: t.truncated,
    })
}

/// The sentence an import ends with.
pub fn message(p: &ia::Plan) -> String {
    let unassigned = p.transactions.iter().filter(|t| !t.assigned).count();
    let mut s = format!(
        "Imported {} into {}",
        count(p.transactions.len(), "transaction", "transactions"),
        count(p.accounts.len(), "account", "accounts")
    );
    if unassigned > 0 {
        s.push_str(&format!(" ({unassigned} unassigned)"));
    }
    s.push('.');
    let skipped = p.skipped.len() + p.duplicates.iter().filter(|d| d.skipped).count();
    if skipped > 0 {
        s.push_str(&format!(
            " {} skipped.",
            count(skipped, "row was", "rows were")
        ));
    }
    s
}

/// Imports `req` from `book` (a file named `file`) into the budget in `s`
/// at `imported`, as one undo step.
pub fn import_into(
    s: &mut Session,
    book: &Book,
    file: &str,
    req: &Request,
    imported: &str,
) -> Result<String, String> {
    let (t, m) = resolve(book, req)?;
    let o = options(req)?;
    let imported = crate::versions::check_created(imported)?;
    let label = format!("Import actuals from {}", clip(file, 60));
    let (p, _) = s.edit(label, |b| ia::import(b, t, &m, &o, file, &imported))?;
    Ok(message(&p))
}

// ---------------------------------------------------------------------------
// The Actuals tab

/// Which transactions to list, as the page sends it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct FilterInput {
    pub account: String,
    pub unassigned: bool,
    pub vendor: String,
    pub text: String,
    pub from: String,
    pub to: String,
    pub batch: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct TransactionDto {
    pub id: TransactionId,
    pub date: String,
    pub account: String,
    /// The account's name; empty when the chart doesn't have it.
    pub account_name: String,
    pub assigned: bool,
    pub line: Option<RowId>,
    /// The line's description and number in its account, when it names one.
    pub line_label: String,
    pub tags: Vec<String>,
    pub vendor: String,
    pub description: String,
    pub reference: String,
    pub amount: Cents,
    pub batch: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct BatchDto {
    pub id: u64,
    pub file: String,
    pub imported: String,
    pub rows: u64,
    /// Its transactions still in the budget.
    pub remaining: usize,
}

/// What `actuals_list` returns.
#[derive(Debug, Serialize)]
pub struct ListDto {
    /// The first [`LIST_LIMIT`] matches, by date.
    pub transactions: Vec<TransactionDto>,
    /// Every match, and what they add up to.
    pub count: usize,
    pub total: Cents,
    /// Every transaction in the budget, and the unassigned ones.
    pub all: usize,
    pub unassigned: usize,
    pub batches: Vec<BatchDto>,
    /// The chart, for the account filter and reassigning.
    pub chart: Vec<ChartDto>,
}

fn filter_of(f: &FilterInput) -> Result<Filter, String> {
    for d in [&f.from, &f.to] {
        if !d.trim().is_empty() && !actuals::is_iso_date(d.trim()) {
            return Err(format!("{d:?} isn't a date written YYYY-MM-DD."));
        }
    }
    Ok(Filter {
        account: Some(f.account.trim().to_string()).filter(|a| !a.is_empty()),
        unassigned: f.unassigned,
        vendor: f.vendor.clone(),
        text: f.text.clone(),
        from: f.from.trim().to_string(),
        to: f.to.trim().to_string(),
        batch: f.batch,
    })
}

/// Where each row is, for naming a transaction's line: row id -> (line
/// number in its account, description).
fn line_labels(b: &Budget) -> BTreeMap<RowId, (usize, String)> {
    let mut out = BTreeMap::new();
    for a in b.categories().flat_map(|c| &c.accounts) {
        for (i, d) in a.details.iter().enumerate() {
            out.entry(d.id).or_insert((i + 1, d.description.clone()));
        }
    }
    out
}

/// The Actuals tab's list for `f`.
pub fn list_dto(b: &Budget, f: &FilterInput) -> Result<ListDto, String> {
    let filter = filter_of(f)?;
    let chart = actuals::chart(b);
    let coding = b.account_coding;
    let matches = actuals::filter(b, &filter);
    let total = matches.iter().fold(0i64, |s, t| s.saturating_add(t.amount));
    let labels = line_labels(b);
    let unassigned = b
        .actuals
        .transactions
        .iter()
        .filter(|t| !chart.contains_key(&actuals::normalize_code(&coding, &t.account)))
        .count();
    let transactions = matches
        .iter()
        .take(LIST_LIMIT)
        .map(|t| {
            let entry = chart.get(&actuals::normalize_code(&coding, &t.account));
            TransactionDto {
                id: t.id,
                date: t.date.clone(),
                account: t.account.clone(),
                account_name: entry.map(|e| e.name.clone()).unwrap_or_default(),
                assigned: entry.is_some(),
                line: t.line,
                line_label: t
                    .line
                    .and_then(|l| labels.get(&l))
                    .map(|(n, d)| {
                        if d.trim().is_empty() {
                            format!("Line {n}")
                        } else {
                            format!("Line {n}: {}", clip(d, 60))
                        }
                    })
                    .unwrap_or_default(),
                tags: t.tags.iter().cloned().collect(),
                vendor: t.vendor.clone(),
                description: t.description.clone(),
                reference: t.reference.clone(),
                amount: t.amount,
                batch: t.batch,
            }
        })
        .collect();
    let batches = b
        .actuals
        .batches
        .iter()
        .map(|x| BatchDto {
            id: x.id,
            file: x.file.clone(),
            imported: x.imported.clone(),
            rows: x.rows,
            remaining: b
                .actuals
                .transactions
                .iter()
                .filter(|t| t.batch == Some(x.id))
                .count(),
        })
        .collect();
    Ok(ListDto {
        transactions,
        count: matches.len(),
        total,
        all: b.actuals.transactions.len(),
        unassigned,
        batches,
        chart: chart_list(b),
    })
}

/// A change to transactions, as the page sends it: each field present is
/// set. `amount` is typed text (`1,234.50`, `(25)`), read exactly.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct EditInput {
    pub date: Option<String>,
    pub account: Option<String>,
    pub line: Option<RowId>,
    /// Takes the line off (`line` is then ignored).
    pub clear_line: bool,
    pub tags: Option<String>,
    pub vendor: Option<String>,
    pub description: Option<String>,
    pub reference: Option<String>,
    pub amount: Option<String>,
}

/// Typed money as cents, read exactly (no floats).
pub fn parse_money(s: &str) -> Result<Cents, String> {
    match parse_number_text(s, false) {
        Ok(Some(d)) => d
            .to_cents()
            .ok_or_else(|| "The amount is too large.".to_string()),
        Ok(None) => Err("Type an amount.".into()),
        Err(_) => Err(format!("{:?} isn't an amount.", s.trim())),
    }
}

/// The engine's edit for `e`.
pub fn edit_of(e: &EditInput) -> Result<TransactionEdit, String> {
    Ok(TransactionEdit {
        date: e.date.as_ref().map(|d| d.trim().to_string()),
        account: e.account.clone(),
        line: if e.clear_line {
            Some(None)
        } else {
            e.line.map(Some)
        },
        tags: match &e.tags {
            Some(t) => {
                Some(actuals::clean_tags(t.split([',', ';'])).map_err(|err| err.to_string())?)
            }
            None => None,
        },
        vendor: e.vendor.clone(),
        description: e.description.clone(),
        reference: e.reference.clone(),
        amount: match &e.amount {
            Some(a) => Some(parse_money(a)?),
            None => None,
        },
    })
}

/// The undo step's name for an edit of `n` transactions.
fn edit_label(b: &Budget, n: usize, e: &EditInput) -> String {
    let what = count(n, "actual", "actuals");
    let only_account = e.account.is_some()
        && e.date.is_none()
        && e.tags.is_none()
        && e.vendor.is_none()
        && e.description.is_none()
        && e.reference.is_none()
        && e.amount.is_none();
    match &e.account {
        Some(a) if only_account => match actuals::chart_code(b, a) {
            Ok(code) if code.is_empty() => format!("Unassign {what}"),
            Ok(code) => format!("Move {what} to {code}"),
            Err(_) => format!("Edit {what}"),
        },
        _ => format!("Edit {what}"),
    }
}

/// An estimate-to-complete method from the page: `budget_minus_actual`,
/// `manual` with an amount, or `percent_complete` with a percent.
pub fn method_of(kind: &str, value: &str) -> Result<Option<EtcMethod>, String> {
    match kind {
        "" | "budget_minus_actual" => Ok(None),
        "manual" => Ok(Some(EtcMethod::Manual(parse_money(value)?))),
        "percent_complete" => {
            let t = value.trim().trim_end_matches('%');
            let p = match parse_number_text(t, false) {
                Ok(Some(p)) => p,
                _ => return Err(format!("{:?} isn't a percent.", value.trim())),
            };
            Ok(Some(EtcMethod::PercentComplete(p)))
        }
        other => Err(format!("unknown method `{other}`")),
    }
}

/// A line of an account, for picking the line a transaction pays for.
#[derive(Debug, Serialize)]
pub struct LineDto {
    pub id: RowId,
    /// Its number in the account (from 1).
    pub n: usize,
    pub description: String,
}

/// The costed lines of account `code` (any spelling).
pub fn lines_of(b: &Budget, code: &str) -> Vec<LineDto> {
    let coding = b.account_coding;
    let key = actuals::normalize_code(&coding, code);
    if key.is_empty() {
        return Vec::new();
    }
    b.categories()
        .flat_map(|c| &c.accounts)
        .find(|a| actuals::normalize_code(&coding, &a.code) == key)
        .map(|a| {
            a.details
                .iter()
                .enumerate()
                .filter(|(_, d)| d.kind.is_costed())
                .map(|(i, d)| LineDto {
                    id: d.id,
                    n: i + 1,
                    description: clip(&d.description, 80),
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call the Actuals tab's: list, update, delete, remove a
// batch, set an estimate to complete, lines)

/// Shows an Open dialog for an accounting export, reads it and returns its
/// sheets. `None` when cancelled.
#[tauri::command]
pub async fn actuals_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ActualsSources>,
    doc: Option<DocId>,
) -> Result<Option<Picked>, String> {
    let start = {
        let s = state.lock_doc(doc)?;
        s.path().and_then(|p| p.parent()).map(PathBuf::from)
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Import actuals")
        .add_filter("Spreadsheet", &["xlsx", "csv", "tsv", "txt"])
        .set_parent(&window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked_file) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked_file.into_path().map_err(|e| e.to_string())?;
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bytes = crate::sheet_import::read_capped(&path)?;
    let book = read_file(&bytes, &file).map_err(|e| format!("Couldn't read {file}: {e}."))?;
    let token = sources.keep(file.clone(), book);
    let (_, book) = sources.get(token)?;
    Ok(Some(picked(token, file, &book)))
}

/// The preview of importing with the dialog's choices into budget `doc`.
/// Changes nothing.
#[tauri::command]
pub fn actuals_preview(
    state: State<'_, AppState>,
    sources: State<'_, ActualsSources>,
    doc: Option<DocId>,
    token: u64,
    request: Request,
) -> Result<PreviewDto, String> {
    let (_, book) = sources.get(token)?;
    let s = state.lock_doc(doc)?;
    preview_dto(s.budget(), &book, &request)
}

/// Imports with the dialog's choices into budget `doc`, as one undo step.
/// `imported` is the page's local time (ISO 8601 with its offset).
#[tauri::command]
pub fn actuals_import<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ActualsSources>,
    doc: Option<DocId>,
    token: u64,
    request: Request,
    imported: String,
) -> Result<Imported, String> {
    let (file, book) = sources.get(token)?;
    let mut s = state.lock_doc(doc)?;
    let message = import_into(&mut s, &book, &file, &request, &imported)?;
    Ok(Imported {
        view: finish(&window, &mut s, None),
        message,
    })
}

/// The Actuals tab's list. Changes nothing.
#[tauri::command]
pub fn actuals_list(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    filter: FilterInput,
) -> Result<ListDto, String> {
    let s = state.lock_doc(doc)?;
    list_dto(s.budget(), &filter)
}

/// Changes, reassigns or unassigns transactions `ids` (one undo step).
#[tauri::command]
pub fn actuals_update<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    ids: Vec<TransactionId>,
    edit: EditInput,
) -> Result<View, String> {
    let e = edit_of(&edit)?;
    let label = {
        let s = state.lock_doc(doc)?;
        edit_label(s.budget(), ids.len(), &edit)
    };
    edit_budget(&window, &state, doc, label, |b| {
        actuals::edit_transactions(b, &ids, &e)
            .map(|_| ())
            .map_err(|err| err.to_string())
    })
}

/// Deletes transactions `ids` (one undo step).
#[tauri::command]
pub fn actuals_delete<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    ids: Vec<TransactionId>,
) -> Result<View, String> {
    let label = format!("Delete {}", count(ids.len(), "actual", "actuals"));
    edit_budget(&window, &state, doc, label, |b| {
        actuals::delete_transactions(b, &ids)
            .map(|_| ())
            .map_err(|err| err.to_string())
    })
}

/// Takes import `batch` back out with its transactions (one undo step).
#[tauri::command]
pub fn actuals_remove_batch<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    batch: u64,
) -> Result<View, String> {
    let label = {
        let s = state.lock_doc(doc)?;
        match s.budget().actuals.batch(batch) {
            Some(x) if !x.file.is_empty() => format!("Remove the import of {}", clip(&x.file, 60)),
            _ => "Remove an import of actuals".to_string(),
        }
    };
    edit_budget(&window, &state, doc, label, |b| {
        actuals::remove_batch(b, batch)
            .map(|_| ())
            .map_err(|err| err.to_string())
    })
}

/// Sets account `account`'s estimate-to-complete method (one undo step):
/// `method` is `budget_minus_actual`, `manual` (with `value` an amount) or
/// `percent_complete` (with `value` a percent).
#[tauri::command]
pub fn actuals_set_etc<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    account: String,
    method: String,
    value: String,
) -> Result<View, String> {
    let m = method_of(&method, &value)?;
    let label = format!(
        "Set the estimate to complete of {}",
        clip(account.trim(), 40)
    );
    edit_budget(&window, &state, doc, label, |b| {
        actuals::set_etc(b, &account, m).map_err(|err| err.to_string())
    })
}

/// The costed lines of account `account`, for the line a transaction pays
/// for. Changes nothing.
#[tauri::command]
pub fn actuals_lines(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    account: String,
) -> Result<Vec<LineDto>, String> {
    let s = state.lock_doc(doc)?;
    Ok(lines_of(s.budget(), &account))
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::{recalc, starter, RecalcOptions};
    use method_format::import::Value;

    fn book(rows: &[&[&str]]) -> Book {
        let mut t = Table {
            name: "Ledger".into(),
            ..Table::default()
        };
        t.rows = rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|s| {
                        if s.is_empty() {
                            Value::Empty
                        } else {
                            Value::Text(s.to_string())
                        }
                    })
                    .collect()
            })
            .collect();
        Book {
            kind: FileKind::Csv,
            tables: vec![t],
        }
    }

    fn ledger() -> Book {
        book(&[
            &["Date", "Account", "Vendor", "Description", "Amount"],
            &["9/1/2026", "1001", "Writer", "Draft", "2,500.00"],
            &["9/2/2026", "11-01", "Producer", "Week 1", "1500"],
            &["9/3/2026", "9999", "Mystery", "Unknown", "50"],
        ])
    }

    fn guessing() -> Request {
        Request {
            detect_header: true,
            guess: true,
            skip_existing: true,
            ..Request::default()
        }
    }

    const NOW: &str = "2026-10-09T14:30:00-07:00";

    #[test]
    fn the_preview_guesses_and_reviews() {
        let mut b = starter::budget();
        b.fringe_posting = method_core::FringePosting::Category {
            suffix: "99".into(),
        };
        let p = picked(3, "ledger.csv".into(), &ledger());
        assert_eq!(p.fields.len(), Field::ALL.len());
        let pv = preview_dto(&b, &ledger(), &guessing()).unwrap();
        assert_eq!(pv.header_row, Some(0));
        assert_eq!(pv.mapping.get("amount"), Some(&4));
        assert_eq!(pv.count, 3);
        assert_eq!(pv.total, 250_000 + 150_000 + 5_000);
        assert_eq!(pv.transactions[0].account, "10-01");
        assert_eq!(pv.transactions[0].account_name, "Writing");
        assert_eq!(pv.unmatched.len(), 1);
        assert_eq!(pv.unassigned, 1);
        assert_eq!(
            (pv.unmatched[0].key.as_str(), pv.unmatched[0].choice),
            ("99-99", "keep")
        );
        assert!(
            pv.chart.iter().any(|c| c.code == "10-99"),
            "fringe accounts too"
        );
        assert_eq!(pv.date_order, "month_first");
        assert_eq!(pv.actual_before, 0);
        assert_eq!(pv.actual_after, pv.total);
        // Choices come back as sent.
        let req = Request {
            detect_header: false,
            guess: false,
            header_row: pv.header_row,
            mapping: pv
                .mapping
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            codes: BTreeMap::from([(
                "99-99".to_string(),
                CodeChoiceInput {
                    choice: "map".into(),
                    account: "12-01".into(),
                },
            )]),
            date_order: "day_first".into(),
            ..guessing()
        };
        let pv = preview_dto(&b, &ledger(), &req).unwrap();
        assert_eq!(pv.unmatched[0].choice, "map");
        assert_eq!(pv.transactions[2].account, "12-01");
        assert_eq!(pv.transactions[0].date, "2026-01-09", "day first");
        for bad in [
            Request {
                date_order: "sideways".into(),
                ..req.clone()
            },
            Request {
                mapping: BTreeMap::from([("amount".into(), 99)]),
                ..req.clone()
            },
            Request {
                codes: BTreeMap::from([(
                    "x".into(),
                    CodeChoiceInput {
                        choice: "maybe".into(),
                        account: String::new(),
                    },
                )]),
                ..req.clone()
            },
        ] {
            assert!(preview_dto(&b, &ledger(), &bad).is_err());
        }
    }

    #[test]
    fn importing_is_one_undo_step_and_one_batch() {
        let mut s = Session::new();
        let before = s.budget().clone();
        let msg = import_into(&mut s, &ledger(), "ledger.csv", &guessing(), NOW).unwrap();
        assert_eq!(
            msg,
            "Imported 3 transactions into 2 accounts (1 unassigned)."
        );
        assert_eq!(s.budget().actuals.batches[0].file, "ledger.csv");
        assert_eq!(s.budget().actuals.batches[0].imported, NOW);
        // The same file again adds nothing.
        let e = import_into(&mut s, &ledger(), "ledger.csv", &guessing(), NOW).unwrap_err();
        assert!(e.contains("No transactions"), "{e}");
        assert!(import_into(&mut s, &ledger(), "x.csv", &guessing(), "yesterday").is_err());
        assert!(s.undo());
        assert_eq!(s.budget(), &before);
    }

    #[test]
    fn views_carry_cost_figures_and_cache_the_rollup() {
        let mut s = Session::new();
        assert!(s.view().cost.is_none(), "no actuals, no cost figures");
        import_into(&mut s, &ledger(), "ledger.csv", &guessing(), NOW).unwrap();
        let v = s.view();
        let cost = v.cost.as_ref().unwrap();
        assert_eq!(cost.transactions, 3);
        assert_eq!(cost.categories[0].accounts[0].figures.actual, 250_000);
        assert_eq!(cost.unassigned.actual, 5_000);
        let r = recalc(s.budget(), &RecalcOptions::default()).unwrap();
        assert_eq!(cost.total.budget, r.grand_total);
        // The budget JSON doesn't carry the transactions.
        assert!(v.budget.get("actuals").is_none());
        let computed = s.rollups_computed();
        // Editing a line doesn't roll the actuals up again...
        let id = s.budget().categories().next().unwrap().accounts[0].details[0].id;
        s.set_detail_field(id, "rate", "60000").unwrap();
        let v = s.view();
        assert_eq!(s.rollups_computed(), computed);
        let a = &v.cost.as_ref().unwrap().categories[0].accounts[0].figures;
        assert_eq!(a.actual, 250_000);
        // ...but changing the actuals does, once.
        s.edit("Delete 1 actual", |b| {
            actuals::delete_transactions(b, &[1])
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .unwrap();
        let v = s.view();
        let _ = s.view();
        assert_eq!(s.rollups_computed(), computed + 1);
        assert_eq!(
            v.cost.as_ref().unwrap().categories[0].accounts[0]
                .figures
                .actual,
            0
        );
        // Undo brings the old actuals back; the full view has them too.
        assert!(s.undo());
        let full = s.full_view();
        assert_eq!(full.cost.as_ref().unwrap().total.actual, 405_000);
        assert!(full.budget.get("actuals").is_none());
    }

    #[test]
    fn line_sums_follow_the_account_on_screen() {
        let mut s = Session::new();
        import_into(&mut s, &ledger(), "ledger.csv", &guessing(), NOW).unwrap();
        let line = s.budget().categories().next().unwrap().accounts[0].details[0].id;
        let e = edit_of(&EditInput {
            line: Some(line),
            ..EditInput::default()
        })
        .unwrap();
        s.edit("Edit 1 actual", |b| {
            actuals::edit_transactions(b, &[1], &e)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .unwrap();
        assert!(
            s.view().cost.unwrap().lines.is_empty(),
            "no account on screen"
        );
        s.set_focus(Some(crate::view::Focus {
            category: "10".into(),
            ti: 0,
            account: "10-01".into(),
            ai: 0,
        }));
        let v = s.view();
        assert_eq!(
            v.cost.unwrap().lines.get(&line).map(|x| x.amount),
            Some(250_000)
        );
    }

    #[test]
    fn lists_filter_and_edits_parse_exactly() {
        let mut s = Session::new();
        import_into(&mut s, &ledger(), "ledger.csv", &guessing(), NOW).unwrap();
        let all = list_dto(s.budget(), &FilterInput::default()).unwrap();
        assert_eq!((all.count, all.all, all.unassigned), (3, 3, 1));
        assert_eq!(all.total, 405_000);
        assert_eq!(all.batches[0].remaining, 3);
        assert_eq!(all.transactions[0].account_name, "Writing");
        let f = FilterInput {
            unassigned: true,
            ..FilterInput::default()
        };
        assert_eq!(list_dto(s.budget(), &f).unwrap().count, 1);
        let f = FilterInput {
            account: "1101".into(),
            ..FilterInput::default()
        };
        assert_eq!(
            list_dto(s.budget(), &f).unwrap().transactions[0].vendor,
            "Producer"
        );
        let f = FilterInput {
            from: "9/1/2026".into(),
            ..FilterInput::default()
        };
        assert!(list_dto(s.budget(), &f).is_err());
        assert_eq!(parse_money("1,234.565").unwrap(), 123_457);
        assert_eq!(parse_money("(25)").unwrap(), -2_500);
        assert!(parse_money("lots").is_err());
        assert!(parse_money(" ").is_err());
        assert_eq!(
            method_of("manual", "$100").unwrap(),
            Some(EtcMethod::Manual(10_000))
        );
        assert_eq!(
            method_of("percent_complete", "40%").unwrap(),
            Some(EtcMethod::PercentComplete("40".parse().unwrap()))
        );
        assert_eq!(method_of("budget_minus_actual", "").unwrap(), None);
        assert!(method_of("percent_complete", "most").is_err());
        assert!(method_of("vibes", "1").is_err());
        let e = EditInput {
            account: Some("12-01".into()),
            ..EditInput::default()
        };
        assert_eq!(edit_label(s.budget(), 2, &e), "Move 2 actuals to 12-01");
        let e = EditInput {
            account: Some(String::new()),
            ..EditInput::default()
        };
        assert_eq!(edit_label(s.budget(), 1, &e), "Unassign 1 actual");
        let e = EditInput {
            tags: Some("PO, Petty cash".into()),
            amount: Some("12.30".into()),
            ..EditInput::default()
        };
        let te = edit_of(&e).unwrap();
        assert_eq!(te.amount, Some(1_230));
        assert_eq!(te.tags.unwrap().len(), 2);
        let lines = lines_of(s.budget(), "1001");
        assert_eq!(lines[0].n, 1);
        assert!(lines_of(s.budget(), "").is_empty());
    }

    #[test]
    fn no_em_dashes_in_messages() {
        let pv = preview_dto(&starter::budget(), &ledger(), &guessing()).unwrap();
        assert!(!format!("{pv:?}").contains('\u{2014}'));
        let mut s = Session::new();
        let msg = import_into(&mut s, &ledger(), "ledger.csv", &guessing(), NOW).unwrap();
        assert!(!msg.contains('\u{2014}'));
    }
}
