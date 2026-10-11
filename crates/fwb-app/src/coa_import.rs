//! File > Import Chart of Accounts… and the bundled chart templates in New
//! From Template (the importer is `fwb_format::import::coa`, the chart and
//! applying it `fwb_core::coa`; docs/TEMPLATES.md).
//!
//! - **The file.** Rust shows the Open dialog and reads the file: any sheet
//!   of a workbook, a CSV or tab-delimited file, or another budget file
//!   (its own chart, no mapping). The page names it by a token afterwards.
//! - **The mapping.** As Import Spreadsheet does: a sheet, a header row and
//!   a column for each field, guessed first; plus the numbering pattern,
//!   inferred from the numbers and offered for change, and whether to keep
//!   sub-accounts and add our default production totals.
//! - **The preview** lists the chart as it will come in, with the rows
//!   skipped and the warnings (duplicates, accounts with no category row,
//!   empty categories), and for "apply to this budget" what would be added,
//!   renamed and kept.
//! - **Importing** opens a new tab with the chart and no lines (its first
//!   undo step, so it starts unsaved), or applies it to the budget as one
//!   undo step: missing categories and accounts added, existing ones
//!   renamed by number, nothing with lines removed.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use fwb_core::coa::{apply, ApplyOptions, ApplyReport, Chart, ChartRow, Named};
use fwb_core::coding::AccountCoding;
use fwb_core::Budget;
use fwb_format::import::coa::{
    self as coa, detect_header, guess, kinded_columns, parse_coding, read_chart, CoaField,
    CoaMapping, CoaOptions, CoaRead,
};
use fwb_format::import::{read_file, Book, FileKind, Table};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, AppState, DocId};
use crate::history::count;
use crate::session::Session;
use crate::sheet_import::{clip, read_capped, ColumnInfo, RawRow, SheetInfo, RAW_COLS, RAW_ROWS};
use crate::templates::UNTITLED;
use crate::view::View;

/// Chart rows shown in the preview.
pub const PREVIEW_ROWS: usize = 400;
/// Skipped rows, warnings and apply lists shown (counts are complete).
pub const LISTED: usize = 50;

// ---------------------------------------------------------------------------
// The file, kept by token

#[derive(Clone)]
enum Content {
    Book(Arc<Book>),
    /// A budget file's own chart.
    Chart(Arc<Chart>),
}

struct Source {
    token: u64,
    file: String,
    content: Content,
}

/// The last chart file picked (the app's state; one for every budget).
#[derive(Default)]
pub struct CoaSources {
    inner: Mutex<(u64, Option<Source>)>,
}

impl CoaSources {
    fn lock(&self) -> MutexGuard<'_, (u64, Option<Source>)> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn keep(&self, file: String, content: Content) -> u64 {
        let mut g = self.lock();
        g.0 += 1;
        let token = g.0;
        g.1 = Some(Source {
            token,
            file,
            content,
        });
        token
    }

    fn get(&self, token: u64) -> Result<(String, Content), String> {
        let g = self.lock();
        g.1.as_ref()
            .filter(|s| s.token == token)
            .map(|s| (s.file.clone(), s.content.clone()))
            .ok_or_else(|| "Another file was picked since. Import again.".to_string())
    }
}

// ---------------------------------------------------------------------------
// What the page sends and gets

/// What `coa_import_pick` returns.
#[derive(Debug, Serialize)]
pub struct Picked {
    pub token: u64,
    pub file: String,
    /// `xlsx`, `csv`, `tsv` or `budget`.
    pub kind: &'static str,
    pub sheets: Vec<SheetInfo>,
    pub sheet: usize,
    pub fields: Vec<&'static str>,
}

/// A numbering pattern between the page and Rust.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct CodingDto {
    pub category: u8,
    pub account: u8,
    #[serde(default)]
    pub sub: u8,
    /// `""` for none.
    #[serde(default)]
    pub separator: String,
}

impl CodingDto {
    fn from(k: &AccountCoding) -> CodingDto {
        CodingDto {
            category: k.category_width,
            account: k.account_width,
            sub: k.sub_width,
            separator: k.separator.map(String::from).unwrap_or_default(),
        }
    }
}

/// The dialog's choices.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub sheet: usize,
    pub header_row: Option<usize>,
    #[serde(default)]
    pub mapping: BTreeMap<String, usize>,
    #[serde(default)]
    pub detect_header: bool,
    #[serde(default)]
    pub guess: bool,
    /// Write the numbers in this pattern; `None` keeps the file's.
    #[serde(default)]
    pub coding: Option<CodingDto>,
    #[serde(default)]
    pub sub_accounts: bool,
    #[serde(default)]
    pub default_levels: bool,
    /// `"current"` applies the chart to the budget; anything else is a new tab.
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub remove_unlisted: bool,
}

#[derive(Debug, Serialize)]
pub struct ChartRowDto {
    /// `category`, `account` or `level`.
    pub kind: &'static str,
    pub code: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct NoteDto {
    /// From 1; 0 for a note about the whole file.
    pub row: usize,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ApplyDto {
    pub added_categories: Vec<String>,
    pub added_accounts: Vec<String>,
    pub added_category_count: usize,
    pub added_account_count: usize,
    /// `11-01 Old name -> New name`.
    pub renamed: Vec<String>,
    pub renamed_count: usize,
    pub unchanged_accounts: usize,
    /// Accounts the chart doesn't list that have lines or notes: kept.
    pub kept: Vec<String>,
    pub kept_count: usize,
    /// Empty accounts the chart doesn't list (removed with the option).
    pub unlisted_empty: Vec<String>,
    pub unlisted_empty_count: usize,
    pub removed_categories: usize,
    pub not_fitting: Vec<String>,
}

/// What `coa_import_preview` returns.
#[derive(Debug, Serialize)]
pub struct PreviewDto {
    /// A budget file: no sheet or mapping.
    pub budget_file: bool,
    /// Read in the `kind,code,name,parent` layout: the mapping isn't used.
    pub kinded: bool,
    pub sheet: usize,
    pub header_row: Option<usize>,
    pub mapping: BTreeMap<&'static str, usize>,
    pub columns: Vec<ColumnInfo>,
    pub raw: Vec<RawRow>,
    /// The pattern the file's numbers follow.
    pub inferred: CodingDto,
    /// The pattern the chart comes in with.
    pub coding: CodingDto,
    pub rows: Vec<ChartRowDto>,
    pub categories: usize,
    pub accounts: usize,
    pub levels: usize,
    pub skipped: Vec<NoteDto>,
    pub skipped_count: usize,
    pub warnings: Vec<NoteDto>,
    pub warning_count: usize,
    pub totals_skipped: usize,
    pub sub_accounts: usize,
    pub padded: usize,
    pub levels_from_source: bool,
    pub default_levels_added: bool,
    /// Accounts named `.` (unused slots in the chart) left out.
    pub placeholders: usize,
    /// Accounts listed again under the same number and name, merged.
    pub merged_repeats: usize,
    /// Accounts listed again under a number already used with another
    /// name, kept under a new number: `10950 Rehearsal hall rentals:
    /// 10950-2` (the first [`LISTED`]).
    pub renumbered: Vec<String>,
    pub renumbered_count: usize,
    pub apply: Option<ApplyDto>,
    pub truncated: bool,
}

/// What `coa_import` did.
#[derive(Debug, Serialize)]
pub struct Imported {
    pub view: View,
    pub message: String,
}

/// One bundled chart template, as New From Template lists it.
#[derive(Debug, Serialize)]
pub struct ChartTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    pub description: &'static str,
}

// ---------------------------------------------------------------------------
// Plain functions

fn picked(token: u64, file: String, content: &Content) -> Picked {
    match content {
        Content::Chart(_) => Picked {
            token,
            file,
            kind: "budget",
            sheets: Vec::new(),
            sheet: 0,
            fields: Vec::new(),
        },
        Content::Book(book) => Picked {
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
            sheet: coa::best_sheet(book),
            fields: CoaField::ALL.iter().map(|f| f.key()).collect(),
        },
    }
}

fn options(req: &Request) -> Result<CoaOptions, String> {
    let coding = match &req.coding {
        None => None,
        Some(c) => Some(
            parse_coding(c.category, c.account, c.sub, &c.separator).map_err(|e| {
                let mut s = format!("That numbering pattern can't be used: {e}");
                s.push('.');
                s
            })?,
        ),
    };
    Ok(CoaOptions {
        coding,
        sub_accounts: req.sub_accounts,
        default_levels: req.default_levels,
    })
}

fn resolve<'b>(book: &'b Book, req: &Request) -> Result<(&'b Table, CoaMapping), String> {
    let t = book
        .tables
        .get(req.sheet)
        .ok_or("That sheet isn't in the file.")?;
    let header_row = if req.detect_header {
        detect_header(t)
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
            CoaMapping {
                header_row,
                columns: guess(t, header_row),
            },
        ));
    }
    let mut columns = BTreeMap::new();
    for (key, &col) in &req.mapping {
        let f = CoaField::parse(key).ok_or_else(|| format!("unknown field `{key}`"))?;
        if col >= t.width() {
            return Err(format!(
                "The {} column is past the sheet's last column.",
                f.name()
            ));
        }
        columns.insert(f, col);
    }
    Ok((
        t,
        CoaMapping {
            header_row,
            columns,
        },
    ))
}

/// The chart `req` reads from `content`, and what reading found.
fn read(
    content: &Content,
    req: &Request,
) -> Result<(Chart, Option<(CoaRead, CoaMapping)>), String> {
    let o = options(req)?;
    match content {
        Content::Chart(c) => {
            let mut chart = (**c).clone();
            if let Some(to) = o.coding {
                chart = chart.recode(&to).map_err(|p| {
                    format!(
                        "The numbers don't fit that pattern: {}.",
                        p.into_iter().take(5).collect::<Vec<_>>().join("; ")
                    )
                })?;
            }
            Ok((chart, None))
        }
        Content::Book(book) => {
            let (t, m) = resolve(book, req)?;
            let r = read_chart(t, &m, &o).map_err(|e| {
                let mut s = e;
                if let Some(first) = s.get_mut(0..1) {
                    first.make_ascii_uppercase();
                }
                s.push('.');
                s
            })?;
            Ok((r.chart.clone(), Some((r, m))))
        }
    }
}

fn named(n: &Named) -> String {
    format!("{} {}", n.code, n.name).trim().to_string()
}

fn apply_dto(r: &ApplyReport) -> ApplyDto {
    let list = |v: &[Named]| v.iter().take(LISTED).map(named).collect();
    ApplyDto {
        added_categories: list(&r.added_categories),
        added_accounts: list(&r.added_accounts),
        added_category_count: r.added_categories.len(),
        added_account_count: r.added_accounts.len(),
        renamed: r
            .renamed_categories
            .iter()
            .chain(&r.renamed_accounts)
            .take(LISTED)
            .map(|x| format!("{} {} \u{2192} {}", x.code, x.old, x.new))
            .collect(),
        renamed_count: r.renamed_categories.len() + r.renamed_accounts.len(),
        unchanged_accounts: r.unchanged_accounts,
        kept: list(&r.kept),
        kept_count: r.kept.len(),
        unlisted_empty: list(&r.unlisted_empty),
        unlisted_empty_count: r.unlisted_empty.len(),
        removed_categories: r.removed_categories.len(),
        not_fitting: r.not_fitting.iter().take(LISTED).cloned().collect(),
    }
}

fn apply_options(req: &Request) -> ApplyOptions {
    ApplyOptions {
        remove_unlisted: req.remove_unlisted,
    }
}

/// The preview of importing `req` from `content`, against budget `b`.
fn preview(b: &Budget, content: &Content, req: &Request) -> Result<PreviewDto, String> {
    let (chart, found) = read(content, req)?;
    let rows = chart
        .rows
        .iter()
        .flat_map(|r| match r {
            ChartRow::Level(n) => vec![ChartRowDto {
                kind: "level",
                code: String::new(),
                name: n.clone(),
            }],
            ChartRow::Category(c) => std::iter::once(ChartRowDto {
                kind: "category",
                code: c.code.clone(),
                name: c.name.clone(),
            })
            .chain(c.accounts.iter().map(|a| ChartRowDto {
                kind: "account",
                code: a.code.clone(),
                name: a.name.clone(),
            }))
            .collect(),
        })
        .take(PREVIEW_ROWS)
        .collect();
    let apply = if req.target == "current" {
        let mut copy = b.clone();
        Some(apply_dto(&apply(&mut copy, &chart, &apply_options(req))?))
    } else {
        None
    };
    let notes = |v: &[fwb_format::import::merge::RowNote]| -> Vec<NoteDto> {
        v.iter()
            .take(LISTED)
            .map(|n| NoteDto {
                row: n.row,
                message: n.message.clone(),
            })
            .collect()
    };
    let mut dto = PreviewDto {
        budget_file: found.is_none(),
        kinded: false,
        sheet: req.sheet,
        header_row: None,
        mapping: BTreeMap::new(),
        columns: Vec::new(),
        raw: Vec::new(),
        inferred: CodingDto::from(&chart.coding),
        coding: CodingDto::from(&chart.coding),
        rows,
        categories: chart.category_count(),
        accounts: chart.account_count(),
        levels: chart.level_count(),
        skipped: Vec::new(),
        skipped_count: 0,
        warnings: Vec::new(),
        warning_count: 0,
        totals_skipped: 0,
        sub_accounts: 0,
        padded: 0,
        levels_from_source: chart.level_count() > 0,
        default_levels_added: false,
        placeholders: 0,
        merged_repeats: 0,
        renumbered: Vec::new(),
        renumbered_count: 0,
        apply,
        truncated: false,
    };
    if let (Some((r, m)), Content::Book(book)) = (found, content) {
        let t = &book.tables[req.sheet.min(book.tables.len().saturating_sub(1))];
        let first = m.first_data_row();
        let titles: Vec<String> = (0..t.width())
            .map(|c| match m.header_row {
                Some(h) => t.cell(h, c).text().trim().to_string(),
                None => String::new(),
            })
            .collect();
        dto.columns = (0..t.width())
            .map(|c| {
                let sample = (first..t.rows.len())
                    .map(|r| t.cell(r, c).text().trim())
                    .find(|s| !s.is_empty())
                    .unwrap_or("");
                ColumnInfo {
                    index: c,
                    letter: fwb_format::export::xlsx::column_name(c),
                    title: clip(&titles[c], 60),
                    sample: clip(sample, 40),
                }
            })
            .collect();
        dto.raw = (0..t.rows.len().min(RAW_ROWS))
            .map(|r| RawRow {
                row: r + 1,
                cells: (0..t.width().min(RAW_COLS))
                    .map(|c| clip(t.cell(r, c).text(), 60))
                    .collect(),
            })
            .collect();
        dto.kinded = kinded_columns(t, m.header_row).is_some();
        dto.header_row = m.header_row;
        dto.mapping = m.columns.iter().map(|(f, c)| (f.key(), *c)).collect();
        dto.inferred = CodingDto::from(&r.inferred);
        dto.skipped = notes(&r.skipped);
        dto.skipped_count = r.skipped.len();
        dto.warnings = notes(&r.warnings);
        dto.warning_count = r.warnings.len();
        dto.totals_skipped = r.totals_skipped;
        dto.sub_accounts = r.sub_accounts;
        dto.padded = r.padded;
        dto.levels_from_source = r.levels_from_source;
        dto.default_levels_added = r.default_levels_added;
        dto.placeholders = r.placeholders;
        dto.merged_repeats = r.merged_repeats;
        dto.renumbered = r
            .renamed_repeats
            .iter()
            .take(LISTED)
            .map(|x| match x.name.as_str() {
                "" => format!("{}: {}", x.old, x.new),
                name => format!("{} {name}: {}", x.old, x.new),
            })
            .collect();
        dto.renumbered_count = r.renamed_repeats.len();
        dto.truncated = t.truncated;
    }
    Ok(dto)
}

/// The sentence applying a chart ends with.
fn apply_message(r: &ApplyReport) -> String {
    let mut parts = Vec::new();
    if !r.added_categories.is_empty() {
        parts.push(format!(
            "added {}",
            count(r.added_categories.len(), "category", "categories")
        ));
    }
    if !r.added_accounts.is_empty() {
        parts.push(format!(
            "added {}",
            count(r.added_accounts.len(), "account", "accounts")
        ));
    }
    let renamed = r.renamed_categories.len() + r.renamed_accounts.len();
    if renamed > 0 {
        parts.push(format!("renamed {renamed}"));
    }
    if !r.removed_accounts.is_empty() {
        parts.push(format!(
            "removed {}",
            count(r.removed_accounts.len(), "empty account", "empty accounts")
        ));
    }
    let mut s = if parts.is_empty() {
        "The chart matches this budget: nothing to change.".to_string()
    } else {
        let mut s = format!("Applied the chart: {}.", parts.join(", "));
        if let Some(first) = s.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        s
    };
    if !r.kept.is_empty() {
        s.push_str(&format!(
            " Kept {} the chart doesn't list (they have lines).",
            count(r.kept.len(), "account", "accounts")
        ));
    }
    s
}

/// Applies `req`'s chart to the budget in `s`, as one undo step.
fn import_into(s: &mut Session, content: &Content, req: &Request) -> Result<String, String> {
    let (chart, _) = read(content, req)?;
    let opts = apply_options(req);
    let rep = s.edit("Apply a chart of accounts", |b| apply(b, &chart, &opts))?;
    Ok(apply_message(&rep))
}

/// A new session holding `req`'s chart and nothing else, as its first undo
/// step (so it starts unsaved), titled Untitled in `currency`.
fn import_new(
    content: &Content,
    req: &Request,
    currency: &str,
) -> Result<(Session, String), String> {
    let (chart, _) = read(content, req)?;
    if chart.category_count() == 0 {
        return Err("No categories were found.".into());
    }
    let mut s = Session::with_budget(Budget::new(UNTITLED, currency), None);
    let made = chart.to_budget(UNTITLED, currency);
    s.edit("Import a chart of accounts", |b| {
        *b = made;
        Ok(())
    })?;
    let msg = format!(
        "New budget with {} and {}, and no lines.",
        count(chart.category_count(), "category", "categories"),
        count(chart.account_count(), "account", "accounts")
    );
    Ok((s, msg))
}

/// Reads a picked file: a budget file's chart, or a spreadsheet.
fn read_content(bytes: &[u8], file: &str) -> Result<Content, String> {
    if coa::is_budget_file(bytes, file) {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| format!("Couldn't read {file}: it isn't text."))?;
        let chart =
            coa::chart_from_budget_text(text).map_err(|e| format!("Couldn't read {file}: {e}."))?;
        return Ok(Content::Chart(Arc::new(chart)));
    }
    let book = read_file(bytes, file).map_err(|e| format!("Couldn't read {file}: {e}."))?;
    Ok(Content::Book(Arc::new(book)))
}

/// The bundled charts, in the manifest's order.
pub fn templates() -> Vec<ChartTemplate> {
    fwb_format::charts::all()
        .iter()
        .map(|c| ChartTemplate {
            id: c.id,
            name: c.name,
            group: c.group,
            description: c.description,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// Shows an Open dialog for a chart of accounts and reads it. `None` when
/// cancelled.
#[tauri::command]
pub async fn coa_import_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, CoaSources>,
    doc: Option<DocId>,
) -> Result<Option<Picked>, String> {
    let start = {
        let s = state.lock_doc(doc)?;
        s.path().and_then(|p| p.parent()).map(PathBuf::from)
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Import chart of accounts")
        .add_filter(
            "Chart of accounts",
            &[
                "xlsx",
                "csv",
                "tsv",
                "txt",
                fwb_format::EXTENSION,
                fwb_format::LEGACY_EXTENSION,
            ],
        )
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
    let bytes = read_capped(&path)?;
    let content = read_content(&bytes, &file)?;
    let token = sources.keep(file.clone(), content.clone());
    Ok(Some(picked(token, file, &content)))
}

/// The preview of importing with the dialog's choices (against budget
/// `doc` for "apply to this budget"). Changes nothing.
#[tauri::command]
pub fn coa_import_preview(
    state: State<'_, AppState>,
    sources: State<'_, CoaSources>,
    doc: Option<DocId>,
    token: u64,
    request: Request,
) -> Result<PreviewDto, String> {
    let (_, content) = sources.get(token)?;
    let b = state.lock_doc(doc)?.budget().clone();
    preview(&b, &content, &request)
}

/// Imports with the dialog's choices: a new tab with the chart, or applied
/// to budget `doc` as one undo step.
#[tauri::command]
pub fn coa_import<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, CoaSources>,
    doc: Option<DocId>,
    token: u64,
    request: Request,
) -> Result<Imported, String> {
    let (_, content) = sources.get(token)?;
    if request.target != "current" {
        let currency = state.lock_doc(doc)?.budget().base_currency.clone();
        let (session, message) = import_new(&content, &request, &currency)?;
        let id = state.docs().add(session);
        return Ok(Imported {
            view: finish(&window, &mut state.lock_doc(Some(id))?, None),
            message,
        });
    }
    let mut s = state.lock_doc(doc)?;
    let message = import_into(&mut s, &content, &request)?;
    Ok(Imported {
        view: finish(&window, &mut s, None),
        message,
    })
}

/// The bundled chart templates.
#[tauri::command]
pub fn chart_templates() -> Vec<ChartTemplate> {
    templates()
}

/// Opens bundled chart `id` as a new budget in a new tab (as New From
/// Template does with a file: no file, Untitled, no unsaved changes).
#[tauri::command]
pub fn chart_template_new<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<View, String> {
    let chart = fwb_format::charts::find(&id)
        .ok_or("That chart template isn't bundled with this version.")?;
    let b = chart
        .budget()
        .map_err(|e| format!("The {} template can't be read: {e}.", chart.name))?;
    let doc = crate::templates::open_new(&state, &b, false);
    Ok(finish(&window, &mut state.lock_doc(Some(doc))?, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fwb_core::{recalc, starter, RecalcOptions};

    const CSV: &[u8] = b"Account,Description\n,ABOVE THE LINE\n1100,STORY\n1101,Rights\n1101,RIGHTS\n,PRODUCTION\n2000,STAFF\n2001,Manager\n";

    fn content() -> Content {
        read_content(CSV, "chart.csv").unwrap()
    }

    fn start() -> Request {
        Request {
            detect_header: true,
            guess: true,
            default_levels: true,
            ..Request::default()
        }
    }

    #[test]
    fn the_preview_lists_the_chart_and_its_warnings() {
        let p = preview(&starter::budget(), &content(), &start()).unwrap();
        assert_eq!((p.categories, p.accounts, p.levels), (2, 2, 2));
        assert_eq!(p.header_row, Some(0));
        assert_eq!(p.mapping.get("account_code"), Some(&0));
        assert_eq!(
            p.inferred,
            CodingDto {
                category: 2,
                account: 2,
                sub: 0,
                separator: String::new()
            }
        );
        assert!(p
            .warnings
            .iter()
            .any(|w| w.message.contains("listed again")));
        assert_eq!((p.merged_repeats, p.renumbered_count), (1, 0));
        assert!(p.apply.is_none());
        assert_eq!(p.rows[0].kind, "category");
        // Another pattern.
        let mut req = start();
        req.coding = Some(CodingDto {
            category: 2,
            account: 2,
            sub: 0,
            separator: "-".into(),
        });
        let p = preview(&starter::budget(), &content(), &req).unwrap();
        assert_eq!(p.rows[1].code, "11-01");
        req.coding = Some(CodingDto {
            category: 9,
            account: 2,
            sub: 0,
            separator: String::new(),
        });
        assert!(preview(&starter::budget(), &content(), &req).is_err());
    }

    #[test]
    fn repeated_numbers_and_unused_slots_are_shown() {
        let csv = b"kind,code,name,parent\ncategory,10900,LOCATIONS,\naccount,10950,RENTALS,10900\naccount,10950,REHEARSAL HALL RENTALS,10900\naccount,10950,Rentals,10900\naccount,10951,.,10900\n";
        let content = read_content(csv, "chart.csv").unwrap();
        let p = preview(&starter::budget(), &content, &start()).unwrap();
        assert_eq!(p.accounts, 2);
        assert_eq!(p.renumbered, ["10950 REHEARSAL HALL RENTALS: 10950-2"]);
        assert_eq!(
            (p.renumbered_count, p.merged_repeats, p.placeholders),
            (1, 1, 1)
        );
        assert!(p.warnings.iter().any(|w| w.message.contains("10950-2")));
    }

    #[test]
    fn a_new_tab_starts_unsaved_with_the_chart_only() {
        let (s, msg) = import_new(&content(), &start(), "USD").unwrap();
        assert!(msg.contains("2 categories"));
        assert!(s.is_dirty());
        assert_eq!(s.budget().title, UNTITLED);
        assert_eq!(s.budget().categories().count(), 2);
        assert!(s.budget().fringes.is_empty());
    }

    #[test]
    fn applying_is_one_undo_step_and_keeps_lines() {
        let mut s = Session::with_budget(starter::budget(), None);
        let before = recalc(s.budget(), &RecalcOptions::default())
            .unwrap()
            .grand_total;
        let mut req = start();
        req.target = "current".into();
        req.remove_unlisted = true;
        let p = preview(s.budget(), &content(), &req).unwrap();
        let a = p.apply.unwrap();
        assert!(a.kept_count > 0, "accounts with lines are reported");
        let msg = import_into(&mut s, &content(), &req).unwrap();
        assert!(msg.contains("Kept"), "{msg}");
        assert_eq!(
            recalc(s.budget(), &RecalcOptions::default())
                .unwrap()
                .grand_total,
            before
        );
        assert!(s
            .budget()
            .categories()
            .any(|c| c.accounts.iter().any(|a| a.name == "Rights")));
        assert!(s.undo());
        assert_eq!(s.budget(), &starter::budget());
    }

    #[test]
    fn a_budget_file_is_its_own_chart() {
        let text = fwb_format::to_string(&starter::budget());
        let c = read_content(text.as_bytes(), "other.mthd").unwrap();
        let p = preview(&starter::budget(), &c, &Request::default()).unwrap();
        assert!(p.budget_file);
        let n: usize = starter::budget()
            .categories()
            .map(|c| c.accounts.len())
            .sum();
        assert_eq!(p.accounts, n);
    }

    #[test]
    fn every_bundled_chart_opens_as_a_template() {
        assert!(!templates().is_empty());
        for t in templates() {
            let b = fwb_format::charts::find(t.id).unwrap().budget().unwrap();
            let state = AppState::with(crate::docs::Docs::default());
            let id = crate::templates::open_new(&state, &b, false);
            let docs = state.docs();
            let s = docs.get(id).unwrap();
            assert_eq!(s.budget().title, UNTITLED);
            assert!(!s.is_dirty());
            recalc(s.budget(), &RecalcOptions::default()).unwrap();
        }
    }
}
