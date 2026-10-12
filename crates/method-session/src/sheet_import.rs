//! File > Import Spreadsheet…: detail lines from an Excel workbook, a CSV
//! file or tab-delimited text, with a column mapping (roadmap 2.2; the
//! readers and the mapping are `method_format::import`).
//!
//! - **The file.** Rust shows the Open dialog and reads the file (at most
//!   [`MAX_FILE_BYTES`]); the page only ever sees its name, and names it
//!   afterwards by a token. Only the last file picked is kept.
//! - **The mapping.** The page picks a sheet, the header row and a column
//!   for each field; Rust guesses them first (`guess`), and the page may
//!   have a saved preset of its own for the sheet's layout (localStorage).
//! - **The preview** is the import worked out against the budget (or, for a
//!   new tab, the budget's chart and setup without lines) without changing
//!   it: the first lines as they will come in with what each comes to, the
//!   rows skipped or changed and why, the accounts, categories and setup
//!   items to be added or left off, the reconciliation against the sheet's
//!   own totals, and the grand total before and after.
//! - **Importing** into the budget is one undo step. Into a new tab, the tab
//!   starts as New From Template would make it from this budget (chart,
//!   setup and settings, no lines, info, baseline or versions) and the
//!   import is its first undo step, so it starts with unsaved changes.
//!
//! Showing the Open dialog and reading the file are the host's (the
//! desktop's src/sheet_import.rs); this reads the bytes it hands in and
//! keeps the workbook by token.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use method_core::{Budget, Cents, RowKind, SetupKind};
use method_format::import::mapping::{self, Field, Mapping};
use method_format::import::merge::{self, Options, Plan, Preview, SETUP_KINDS};
use method_format::import::{read_file, Book, FileKind, Table, MAX_FILE_BYTES};
use serde::{Deserialize, Serialize};

use crate::docs::{AppState, DocId};
use crate::history::count;
use crate::session::Session;
use crate::templates::{strip, UNTITLED};
use crate::view::View;

/// Rows of the sheet shown raw, for picking the header row.
pub const RAW_ROWS: usize = 12;
/// Columns shown raw.
pub const RAW_COLS: usize = 40;
/// Characters of a raw cell shown.
const RAW_CHARS: usize = 60;
/// Lines shown as they will import.
pub const PREVIEW_LINES: usize = 25;
/// Skipped rows and warnings listed (the counts are always complete).
pub const NOTES_LISTED: usize = 50;
/// Longest default account number accepted.
const MAX_ACCOUNT_CHARS: usize = 64;

// ---------------------------------------------------------------------------
// The file, kept by token between the dialog's calls

struct Source {
    token: u64,
    file: String,
    book: Arc<Book>,
}

/// The last file picked to import (the app's state; one for every budget).
#[derive(Default)]
pub struct SheetSources {
    inner: Mutex<(u64, Option<Source>)>,
}

impl SheetSources {
    fn lock(&self) -> MutexGuard<'_, (u64, Option<Source>)> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Keeps `book` (read from a file named `file`) and returns its token.
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

    /// The file with `token`, or a message when another was picked since.
    pub fn get(&self, token: u64) -> Result<(String, Arc<Book>), String> {
        let g = self.lock();
        g.1.as_ref()
            .filter(|s| s.token == token)
            .map(|s| (s.file.clone(), Arc::clone(&s.book)))
            .ok_or_else(|| "Another file was picked since. Import again.".to_string())
    }
}

// ---------------------------------------------------------------------------
// What the page sends and gets

/// One sheet of the picked file, as the dialog lists it.
#[derive(Debug, Serialize)]
pub struct SheetInfo {
    pub name: String,
    pub hidden: bool,
    pub rows: usize,
    pub columns: usize,
    pub truncated: bool,
}

/// What `sheet_import_pick` returns.
#[derive(Debug, Serialize)]
pub struct Picked {
    pub token: u64,
    /// The file's name (never its path).
    pub file: String,
    /// `xlsx`, `csv` or `tsv`.
    pub kind: &'static str,
    pub sheets: Vec<SheetInfo>,
    /// The sheet to start on: the first visible one with rows.
    pub sheet: usize,
    /// Every field, in the dialog's order.
    pub fields: Vec<&'static str>,
}

/// The dialog's choices.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Request {
    pub sheet: usize,
    /// The header row (from 0), or `null` for none. Ignored with
    /// `detect_header`.
    pub header_row: Option<usize>,
    /// Field key -> column (from 0). Ignored with `guess`.
    #[serde(default)]
    pub mapping: BTreeMap<String, usize>,
    /// Find the header row instead of taking `header_row`.
    #[serde(default)]
    pub detect_header: bool,
    /// Guess the mapping from the header row's titles instead of taking
    /// `mapping`.
    #[serde(default)]
    pub guess: bool,
    #[serde(default)]
    pub create_accounts: bool,
    #[serde(default)]
    pub create_setup: bool,
    #[serde(default)]
    pub default_account: String,
    /// `"new"` for a new tab, else the budget itself.
    #[serde(default)]
    pub target: String,
}

#[derive(Debug, Serialize)]
pub struct ColumnInfo {
    pub index: usize,
    /// `A`, `B`... `AA`.
    pub letter: String,
    /// The header row's text in this column.
    pub title: String,
    /// The first value below the header, to recognize the column by.
    pub sample: String,
}

#[derive(Debug, Serialize)]
pub struct RawRow {
    /// The sheet's row number (from 1).
    pub row: usize,
    pub cells: Vec<String>,
}

/// A line as it will import.
#[derive(Debug, Serialize)]
pub struct LineDto {
    pub row: usize,
    pub account: String,
    pub new_account: bool,
    pub kind: &'static str,
    pub description: String,
    pub amount: String,
    pub unit: String,
    pub x: String,
    pub rate: String,
    pub currency: String,
    pub x4: String,
    pub fringes: String,
    pub groups: String,
    pub set: String,
    pub location: String,
    pub note: String,
    /// What it comes to once imported (base currency; a subtotal row's
    /// running total), when the budget recalculates.
    pub total: Option<Cents>,
    /// The sheet's own total, when mapped.
    pub sheet_total: Option<Cents>,
}

#[derive(Debug, Serialize)]
pub struct RowNoteDto {
    pub row: usize,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct SetupDto {
    /// `fringe`, `group`, `unit`, `set`, `location` or `currency`.
    pub kind: &'static str,
    pub names: Vec<String>,
    /// For left-off names: how many rows named each, in `names` order.
    pub rows: Vec<usize>,
}

#[derive(Debug, Serialize)]
pub struct MismatchDto {
    pub row: usize,
    pub account: String,
    pub description: String,
    pub sheet: Cents,
    pub computed: Cents,
}

#[derive(Debug, Serialize)]
pub struct ReconciliationDto {
    pub compared: usize,
    pub mismatched: usize,
    pub sheet_sum: Cents,
    pub computed_sum: Cents,
    /// `computed_sum - sheet_sum`.
    pub difference: Cents,
    pub first: Vec<MismatchDto>,
}

/// What `sheet_import_preview` returns.
#[derive(Debug, Serialize)]
pub struct PreviewDto {
    pub sheet: usize,
    pub header_row: Option<usize>,
    /// The header row's normalized titles: what a saved preset matches.
    pub signature: String,
    /// The mapping in effect (the guess, with `guess`): field key -> column.
    pub mapping: BTreeMap<&'static str, usize>,
    pub columns: Vec<ColumnInfo>,
    /// The top of the sheet, for picking the header row.
    pub raw: Vec<RawRow>,
    pub lines: Vec<LineDto>,
    pub line_count: usize,
    pub skipped: Vec<RowNoteDto>,
    pub skipped_count: usize,
    pub warnings: Vec<RowNoteDto>,
    pub warning_count: usize,
    pub headings: usize,
    pub new_categories: Vec<String>,
    pub new_accounts: Vec<String>,
    pub matched_accounts: usize,
    pub added: Vec<SetupDto>,
    pub dropped: Vec<SetupDto>,
    /// Sentences about what the import adds that needs looking at.
    pub notes: Vec<String>,
    pub reconciliation: Option<ReconciliationDto>,
    pub grand_total_before: Option<Cents>,
    pub grand_total_after: Option<Cents>,
    pub calc_error: Option<String>,
    pub truncated: bool,
}

/// What `sheet_import` did.
#[derive(Debug, Serialize)]
pub struct Imported {
    pub view: View,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Plain functions

/// `0` is `A`, `26` is `AA` (the export's naming).
fn letter(i: usize) -> String {
    method_format::export::xlsx::column_name(i)
}

fn kind_key(k: SetupKind) -> &'static str {
    k.name()
}

fn row_kind(k: RowKind) -> &'static str {
    match k {
        RowKind::Line => "line",
        RowKind::Subtotal => "subtotal",
        RowKind::Adjustment => "adjustment",
        RowKind::Separator => "separator",
    }
}

/// `s` cut to `n` characters, with an ellipsis when it was longer.
pub fn clip(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((at, _)) => format!("{}\u{2026}", &s[..at]),
        None => s.to_string(),
    }
}

/// The sheet to start on: the visible one whose titles name the most
/// fields (our own export's Detail sheet, a department's budget sheet),
/// else the first visible one with rows.
fn best_sheet(book: &Book) -> usize {
    let mut best: Option<(usize, usize)> = None; // (fields guessed, sheet)
    for (i, t) in book.tables.iter().enumerate() {
        if t.hidden || t.rows.is_empty() {
            continue;
        }
        let n = mapping::guess(t, mapping::detect_header(t)).len();
        if best.is_none_or(|(b, _)| n > b) {
            best = Some((n, i));
        }
    }
    best.map_or(0, |(_, i)| i)
}

/// What the dialog starts with for `book`.
pub fn picked(token: u64, file: String, book: &Book) -> Picked {
    let sheet = best_sheet(book);
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
        sheet,
        fields: Field::ALL.iter().map(|f| f.key()).collect(),
    }
}

/// The sheet and mapping a request names, checked against the file.
fn resolve<'b>(book: &'b Book, req: &Request) -> Result<(&'b Table, Mapping), String> {
    let t = book
        .tables
        .get(req.sheet)
        .ok_or("That sheet isn't in the file.")?;
    let header_row = if req.detect_header {
        mapping::detect_header(t)
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
                columns: mapping::guess(t, header_row),
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
    let default_account = req.default_account.trim().to_string();
    if default_account.chars().count() > MAX_ACCOUNT_CHARS {
        return Err("The default account number is too long.".into());
    }
    Ok(Options {
        create_accounts: req.create_accounts,
        create_setup: req.create_setup,
        default_account,
    })
}

/// The budget an import with `req` goes into: `b`, or for a new tab `b`'s
/// chart, setup and settings without lines.
fn target(b: &Budget, req: &Request) -> Budget {
    if req.target == "new" {
        strip(b, false, UNTITLED)
    } else {
        b.clone()
    }
}

/// Sentences about what an import adds that needs looking at.
fn plan_notes(plan: &Plan) -> Vec<String> {
    let mut out = Vec::new();
    let list = |k: SetupKind| -> Option<String> {
        plan.added
            .get(&k)
            .filter(|s| !s.is_empty())
            .map(|s| s.iter().cloned().collect::<Vec<_>>().join(", "))
    };
    if let Some(names) = list(SetupKind::Fringe) {
        out.push(format!(
            "New fringes start at 0%: {names}. Set their rates in Fringes."
        ));
    }
    if let Some(names) = list(SetupKind::Currency) {
        out.push(format!(
            "New currencies start at a rate of 1: {names}. Set their rates in Currencies."
        ));
    }
    out
}

/// The preview of importing `req` from `book` into budget `b`.
pub fn preview_dto(b: &Budget, book: &Book, req: &Request) -> Result<PreviewDto, String> {
    let (t, m) = resolve(book, req)?;
    let o = options(req)?;
    let into = target(b, req);
    let p: Preview = merge::preview(&into, t, &m, &o);
    let first = m.first_data_row();
    let titles = mapping::titles(t, m.header_row);
    let columns = (0..t.width())
        .map(|c| {
            let sample = (first..t.rows.len())
                .map(|r| t.cell(r, c).text().trim())
                .find(|s| !s.is_empty())
                .unwrap_or("");
            ColumnInfo {
                index: c,
                letter: letter(c),
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
    let plan = &p.plan;
    let lines = plan
        .lines
        .iter()
        .zip(&p.totals)
        .take(PREVIEW_LINES)
        .map(|(l, total)| {
            let d = &l.detail;
            let join = |s: &std::collections::BTreeSet<String>| {
                s.iter().cloned().collect::<Vec<_>>().join(", ")
            };
            LineDto {
                row: l.row,
                account: l.account.clone(),
                new_account: l.new_account,
                kind: row_kind(d.kind),
                description: clip(&d.description, 120),
                amount: d.amount.0.clone(),
                unit: d.unit.clone(),
                x: d.x.0.clone(),
                rate: d.rate.0.clone(),
                currency: d.currency.clone().unwrap_or_default(),
                x4: d.x4.0.clone(),
                fringes: join(&d.fringes),
                groups: join(&d.groups),
                set: d.set.clone().unwrap_or_default(),
                location: d.location.clone().unwrap_or_default(),
                note: clip(&method_core::notes_text(&d.notes), 120),
                total: *total,
                sheet_total: l.sheet_total,
            }
        })
        .collect();
    let notes = |v: &[merge::RowNote]| -> Vec<RowNoteDto> {
        v.iter()
            .take(NOTES_LISTED)
            .map(|n| RowNoteDto {
                row: n.row,
                message: n.message.clone(),
            })
            .collect()
    };
    let added = SETUP_KINDS
        .iter()
        .filter_map(|k| {
            let names = plan.added.get(k)?;
            Some(SetupDto {
                kind: kind_key(*k),
                names: names.iter().cloned().collect(),
                rows: Vec::new(),
            })
        })
        .collect();
    let dropped = SETUP_KINDS
        .iter()
        .filter_map(|k| {
            let names = plan.dropped.get(k)?;
            Some(SetupDto {
                kind: kind_key(*k),
                names: names.keys().cloned().collect(),
                rows: names.values().copied().collect(),
            })
        })
        .collect();
    let reconciliation = p.reconciliation.as_ref().map(|r| ReconciliationDto {
        compared: r.compared,
        mismatched: r.mismatched,
        sheet_sum: r.sheet_sum,
        computed_sum: r.computed_sum,
        difference: r.computed_sum.saturating_sub(r.sheet_sum),
        first: r
            .first
            .iter()
            .map(|x| MismatchDto {
                row: x.row,
                account: x.account.clone(),
                description: clip(&x.description, 120),
                sheet: x.sheet,
                computed: x.computed,
            })
            .collect(),
    });
    Ok(PreviewDto {
        sheet: req.sheet,
        header_row: m.header_row,
        signature: mapping::signature(t, m.header_row),
        mapping: m.columns.iter().map(|(f, c)| (f.key(), *c)).collect(),
        columns,
        raw,
        lines,
        line_count: plan.lines.len(),
        skipped: notes(&plan.skipped),
        skipped_count: plan.skipped.len(),
        warnings: notes(&plan.warnings),
        warning_count: plan.warnings.len(),
        headings: plan.headings,
        new_categories: plan
            .new_categories
            .iter()
            .map(|c| format!("{} {}", c.code, c.name).trim().to_string())
            .collect(),
        new_accounts: plan
            .new_accounts
            .iter()
            .map(|a| format!("{} {}", a.code, a.name).trim().to_string())
            .collect(),
        matched_accounts: plan.matched_accounts.len(),
        added,
        dropped,
        notes: plan_notes(plan),
        reconciliation,
        grand_total_before: p.grand_total_before,
        grand_total_after: p.grand_total_after,
        calc_error: p.calc_error,
        truncated: t.truncated,
    })
}

/// The sentence an import ends with.
pub fn message(plan: &Plan, applied: &merge::Applied) -> String {
    let accounts: std::collections::BTreeSet<&str> =
        plan.lines.iter().map(|l| l.account.as_str()).collect();
    let mut s = format!(
        "Imported {} into {}",
        count(plan.lines.len(), "line", "lines"),
        count(accounts.len(), "account", "accounts")
    );
    let mut added = Vec::new();
    if applied.accounts > 0 {
        added.push(count(applied.accounts, "new account", "new accounts"));
    }
    if applied.categories > 0 {
        added.push(count(applied.categories, "new category", "new categories"));
    }
    if applied.setup > 0 {
        added.push(count(applied.setup, "new setup item", "new setup items"));
    }
    if !added.is_empty() {
        s.push_str(&format!(" ({})", added.join(", ")));
    }
    s.push('.');
    if !plan.skipped.is_empty() {
        s.push_str(&format!(
            " {} skipped.",
            count(plan.skipped.len(), "row was", "rows were")
        ));
    }
    s
}

/// Imports `req` from `book` into the budget in `s`, as one undo step.
pub fn import_into(s: &mut Session, book: &Book, req: &Request) -> Result<String, String> {
    let (t, m) = resolve(book, req)?;
    let o = options(req)?;
    let (plan, applied) = s.edit("Import lines from a spreadsheet", |b| {
        merge::import(b, t, &m, &o)
    })?;
    Ok(message(&plan, &applied))
}

/// A new session holding `b`'s chart, setup and settings with `req`
/// imported into it (its first undo step).
pub fn import_new(b: &Budget, book: &Book, req: &Request) -> Result<(Session, String), String> {
    let mut s = Session::with_budget(strip(b, false, UNTITLED), None);
    let msg = import_into(&mut s, book, req)?;
    Ok((s, msg))
}

/// Refuses a picked file of `len` bytes when it is too large to import
/// (more than [`MAX_FILE_BYTES`]).
pub fn check_size(len: usize) -> Result<(), String> {
    if len > MAX_FILE_BYTES {
        return Err(format!(
            "The file is too large to import (more than {} MB).",
            MAX_FILE_BYTES >> 20
        ));
    }
    Ok(())
}

/// Reads a picked spreadsheet, `bytes` from a file named `file`, as a
/// workbook.
pub fn read_book(file: &str, bytes: &[u8]) -> Result<Book, String> {
    check_size(bytes.len())?;
    read_file(bytes, file).map_err(|e| format!("Couldn't read {file}: {e}."))
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/sheet_import.rs and the host's
// src/host/sheet_import.rs call these)

/// Reads a picked spreadsheet (`bytes` from a file named `file`), keeps it
/// and returns its sheets.
pub fn sheet_import_pick(
    sources: &SheetSources,
    file: String,
    bytes: &[u8],
) -> Result<Picked, String> {
    let book = read_book(&file, bytes)?;
    let token = sources.keep(file.clone(), book);
    let (_, book) = sources.get(token)?;
    Ok(picked(token, file, &book))
}

/// The preview of importing with the dialog's choices into budget `b`.
/// Changes nothing.
pub fn sheet_import_preview(
    b: &Budget,
    sources: &SheetSources,
    token: u64,
    request: &Request,
) -> Result<PreviewDto, String> {
    let (_, book) = sources.get(token)?;
    preview_dto(b, &book, request)
}

/// Imports with the dialog's choices: into budget `doc` as one undo step,
/// or into a new tab (in front) made from its chart and setup. The budget
/// imported into, and the message saying what came in.
pub fn sheet_import(
    state: &AppState,
    sources: &SheetSources,
    doc: Option<DocId>,
    token: u64,
    request: &Request,
) -> Result<(DocId, String), String> {
    let (_, book) = sources.get(token)?;
    if request.target == "new" {
        let b = state.lock_doc(doc)?.budget().clone();
        let (session, message) = import_new(&b, &book, request)?;
        let id = state.docs().add(session);
        return Ok((id, message));
    }
    let mut s = state.lock_doc(doc)?;
    let message = import_into(&mut s, &book, request)?;
    Ok((s.id(), message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docs::Docs;
    use method_core::{recalc, starter, RecalcOptions};
    use method_format::import::Value;

    fn book(rows: &[&[&str]]) -> Book {
        let mut t = Table {
            name: "Sheet1".into(),
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

    fn sheet() -> Book {
        book(&[
            &["Camera department"],
            &[],
            &["Acct", "Item", "Qty", "Rate", "Fringes", "Total"],
            &["10-01", "Rewrite", "1", "2500", "PAYROLL", "2500"],
            &["55-01", "Camera body", "2", "450", "", "900"],
            &["", "Lenses", "1", "300", "", "350"],
        ])
    }

    #[test]
    fn the_preview_guesses_and_reconciles() {
        let b = starter::budget_with_separator('-');
        let s = sheet();
        let p = picked(7, "dept.csv".into(), &s);
        assert_eq!(p.kind, "csv");
        assert_eq!(p.sheets[0].rows, 6);
        assert_eq!(p.fields.len(), Field::ALL.len());
        let req = Request {
            detect_header: true,
            guess: true,
            create_accounts: true,
            create_setup: true,
            ..Request::default()
        };
        let pv = preview_dto(&b, &s, &req).unwrap();
        assert_eq!(pv.header_row, Some(2));
        assert_eq!(pv.mapping.get("account_code"), Some(&0));
        assert_eq!(pv.mapping.get("description"), Some(&1));
        assert_eq!(pv.mapping.get("line_total"), Some(&5));
        assert_eq!(pv.columns[3].title, "Rate");
        assert_eq!(pv.columns[3].sample, "2500");
        assert_eq!(pv.raw.len(), 6);
        assert_eq!(pv.line_count, 3);
        assert_eq!(pv.new_accounts, ["55-01"]);
        assert_eq!(pv.new_categories, ["55"]);
        assert_eq!(pv.lines[2].account, "55-01", "carried down");
        assert_eq!(pv.lines[1].total, Some(90_000));
        let rec = pv.reconciliation.unwrap();
        assert_eq!((rec.compared, rec.mismatched), (3, 1));
        assert_eq!(rec.difference, -5_000);
        assert_eq!(rec.first[0].row, 6);
        assert!(pv.grand_total_after.unwrap() > pv.grand_total_before.unwrap());

        // The same choices sent back as a mapping give the same preview.
        let req2 = Request {
            detect_header: false,
            guess: false,
            header_row: pv.header_row,
            mapping: pv
                .mapping
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            ..req.clone()
        };
        let pv2 = preview_dto(&b, &s, &req2).unwrap();
        assert_eq!(pv2.line_count, 3);
        // Bad requests are refused.
        let bad = Request {
            mapping: BTreeMap::from([("rate".into(), 99)]),
            ..req2.clone()
        };
        assert!(preview_dto(&b, &s, &bad).is_err());
        let bad = Request {
            mapping: BTreeMap::from([("nonsense".into(), 0)]),
            ..req2.clone()
        };
        assert!(preview_dto(&b, &s, &bad).is_err());
        let bad = Request {
            sheet: 3,
            ..req2.clone()
        };
        assert!(preview_dto(&b, &s, &bad).is_err());
        let bad = Request {
            default_account: "9".repeat(100),
            ..req2
        };
        assert!(preview_dto(&b, &s, &bad).is_err());
    }

    #[test]
    fn importing_into_the_budget_is_one_undo_step() {
        let mut s = Session::new();
        let before = s.budget().clone();
        let req = Request {
            detect_header: true,
            guess: true,
            create_accounts: true,
            create_setup: true,
            ..Request::default()
        };
        let msg = import_into(&mut s, &sheet(), &req).unwrap();
        assert_eq!(
            msg,
            "Imported 3 lines into 2 accounts (1 new account, 1 new category)."
        );
        assert!(s.is_dirty());
        recalc(s.budget(), &RecalcOptions::default()).unwrap();
        assert!(s.undo());
        assert_eq!(s.budget(), &before);
        assert!(!s.can_undo());
    }

    #[test]
    fn importing_into_a_new_tab_keeps_the_setup() {
        let state = AppState::with(Docs::default());
        let b = state.lock().budget().clone();
        let req = Request {
            detect_header: true,
            guess: true,
            create_accounts: true,
            create_setup: false,
            target: "new".into(),
            ..Request::default()
        };
        let pv = preview_dto(&b, &sheet(), &req).unwrap();
        // Into a budget without lines: the grand total starts at the
        // charges on nothing.
        assert_eq!(pv.grand_total_before, Some(0));
        let (session, msg) = import_new(&b, &sheet(), &req).unwrap();
        assert!(msg.starts_with("Imported 3 lines"), "{msg}");
        let id = state.docs().add(session);
        let docs = state.docs();
        let s = docs.get(id).unwrap();
        assert!(s.path().is_none() && s.is_dirty());
        assert_eq!(s.budget().fringes, b.fringes);
        let lines: usize = s
            .budget()
            .categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.details.len())
            .sum();
        assert_eq!(lines, 3, "only the imported lines");
        assert_eq!(docs.active(), id);
    }

    #[test]
    fn nothing_to_import_changes_nothing() {
        let mut s = Session::new();
        let req = Request {
            detect_header: true,
            guess: true,
            ..Request::default()
        };
        let e = import_into(
            &mut s,
            &book(&[&["Acct", "Item"], &["99-99", "Nowhere"]]),
            &req,
        )
        .unwrap_err();
        assert!(e.contains("No rows"), "{e}");
        assert!(!s.is_dirty() && !s.can_undo());
    }

    #[test]
    fn the_dialog_starts_on_the_sheet_that_maps_best() {
        let notes = book(&[&["Notes"], &["Prepared by someone"]])
            .tables
            .remove(0);
        let mut hidden = sheet().tables.remove(0);
        hidden.hidden = true;
        let budget = sheet().tables.remove(0);
        let b = Book {
            kind: FileKind::Xlsx,
            tables: vec![notes, hidden, budget],
        };
        assert_eq!(picked(1, "w.xlsx".into(), &b).sheet, 2);
        // A guess for a header row the page picked.
        let req = Request {
            header_row: Some(2),
            guess: true,
            ..Request::default()
        };
        let pv = preview_dto(&starter::budget_with_separator('-'), &sheet(), &req).unwrap();
        assert_eq!(pv.header_row, Some(2));
        assert_eq!(pv.mapping.get("rate"), Some(&3));
        let req = Request {
            header_row: Some(99),
            ..Request::default()
        };
        assert!(preview_dto(&starter::budget_with_separator('-'), &sheet(), &req).is_err());
    }

    #[test]
    fn sources_keep_only_the_last_file() {
        let src = SheetSources::default();
        let a = src.keep("a.csv".into(), sheet());
        let b = src.keep("b.csv".into(), sheet());
        assert!(src.get(a).is_err());
        assert_eq!(src.get(b).unwrap().0, "b.csv");
    }

    #[test]
    fn picked_files_are_capped_and_read() {
        assert!(check_size(MAX_FILE_BYTES).is_ok());
        assert!(check_size(MAX_FILE_BYTES + 1)
            .unwrap_err()
            .contains("too large"));
        let src = SheetSources::default();
        let p = sheet_import_pick(&src, "small.csv".into(), b"Acct,Rate\n10-01,5\n").unwrap();
        assert_eq!((p.kind, p.sheets[0].rows), ("csv", 2));
        assert!(src.get(p.token).is_ok());
    }

    #[test]
    fn no_em_dashes_in_messages() {
        let s = sheet();
        let pv = preview_dto(
            &starter::budget_with_separator('-'),
            &s,
            &Request {
                detect_header: true,
                guess: true,
                create_setup: true,
                create_accounts: false,
                ..Request::default()
            },
        )
        .unwrap();
        let all = format!("{pv:?}");
        assert!(!all.contains('\u{2014}'));
    }
}
