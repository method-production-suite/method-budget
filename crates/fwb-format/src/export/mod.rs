//! Spreadsheet export: the evaluated budget as plain tables, written as an
//! Excel workbook (.xlsx), CSV files or tab-delimited text.
//!
//! [`workbook`] turns a budget and its report into [`Sheet`]s: Topsheet,
//! Accounts, Detail and the setup tables (Globals, Fringes, Groups, Units,
//! Currencies, Sets, Locations). The writers ([`csv`], [`xlsx`]) only
//! serialize those tables, so every format carries the same figures.
//!
//! Money stays integer cents until it is written, then becomes an exact
//! decimal string (`1234.56`, never a float). Decimals (quantities, rates,
//! globals) are written exactly as the engine holds them.
//!
//! Everything here is plain std: the .xlsx is a ZIP of XML parts, written by
//! [`zip`] (stored entries, CRC-32) and [`xlsx`] (the SpreadsheetML parts).

pub mod csv;
mod tables;
pub mod xlsx;
pub mod zip;

use std::fmt;
use std::path::{Path, PathBuf};

use fwb_core::{Budget, Cents, Dec, Report};

pub use tables::workbook;

/// One cell's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Text(String),
    /// Money in cents, written as an exact decimal with two places.
    Money(Cents),
    /// A decimal (quantity, rate, percent, global), written exactly.
    Number(Dec),
    /// A whole number (line number, count, row id).
    Int(u64),
}

impl Cell {
    pub fn text(s: impl Into<String>) -> Cell {
        let s = s.into();
        if s.is_empty() {
            Cell::Empty
        } else {
            Cell::Text(s)
        }
    }
}

/// One table: a header row of column titles, then data rows of the same
/// width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sheet {
    /// The sheet's name in the workbook, and the end of its CSV file name.
    pub name: &'static str,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

/// Every table of one export, in workbook order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workbook {
    /// The budget's title (the workbook's document title).
    pub title: String,
    pub sheets: Vec<Sheet>,
}

/// Sheet names, in workbook order.
pub const SHEET_NAMES: [&str; 10] = [
    "Topsheet",
    "Accounts",
    "Detail",
    "Globals",
    "Fringes",
    "Groups",
    "Units",
    "Currencies",
    "Sets",
    "Locations",
];

/// The sheet that the tab-delimited flavor writes.
pub const DETAIL: &str = "Detail";

impl Workbook {
    pub fn sheet(&self, name: &str) -> Option<&Sheet> {
        self.sheets.iter().find(|s| s.name == name)
    }
}

/// Why an export could not be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportError {
    /// The report was not computed from this budget (its categories,
    /// accounts or rows don't line up).
    ReportMismatch,
    /// Too big for a plain ZIP file (an .xlsx of 4 GB or more).
    Zip(zip::ZipError),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::ReportMismatch => {
                f.write_str("the totals are out of date with the budget; try again")
            }
            ExportError::Zip(e) => write!(f, "the workbook is {e}"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<zip::ZipError> for ExportError {
    fn from(e: zip::ZipError) -> ExportError {
        ExportError::Zip(e)
    }
}

/// The export formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// One Excel workbook, a sheet per table.
    Xlsx,
    /// One CSV file per table.
    Csv,
    /// The Detail table as tab-delimited text, for accounting tools.
    Tsv,
}

impl Format {
    /// `xlsx`, `csv` or `tsv` (any case).
    pub fn parse(s: &str) -> Option<Format> {
        match s.trim().to_ascii_lowercase().as_str() {
            "xlsx" => Some(Format::Xlsx),
            "csv" => Some(Format::Csv),
            "tsv" => Some(Format::Tsv),
            _ => None,
        }
    }

    /// The file extension this format is saved with.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Xlsx => "xlsx",
            Format::Csv => "csv",
            Format::Tsv => "txt",
        }
    }
}

/// Export choices beyond the format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Start CSV and tab-delimited files with a UTF-8 byte order mark, which
    /// Excel needs to read accented letters correctly. Ignored for .xlsx.
    pub bom: bool,
}

/// A file an export writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutFile {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

/// The files exporting `wb` as `format` to `out` writes, with their bytes
/// (nothing is written here). `.xlsx` and tab-delimited are one file at
/// `out`. CSV is one file per table next to `out`, named from its stem:
/// `out` = `Film.csv` gives `Film - Topsheet.csv`, `Film - Accounts.csv`...
pub fn render(
    wb: &Workbook,
    format: Format,
    out: &Path,
    opts: Options,
) -> Result<Vec<OutFile>, ExportError> {
    Ok(match format {
        Format::Xlsx => vec![OutFile {
            path: out.to_path_buf(),
            bytes: xlsx::write(wb)?,
        }],
        Format::Tsv => {
            let detail = wb.sheet(DETAIL).ok_or(ExportError::ReportMismatch)?;
            vec![OutFile {
                path: out.to_path_buf(),
                bytes: csv::write(detail, csv::Delimiter::Tab, opts.bom),
            }]
        }
        Format::Csv => wb
            .sheets
            .iter()
            .map(|s| OutFile {
                path: csv_path(out, s.name),
                bytes: csv::write(s, csv::Delimiter::Comma, opts.bom),
            })
            .collect(),
    })
}

/// Where table `sheet` of a CSV export to `out` goes: `<stem> - <sheet>.csv`
/// beside `out`, where the stem is `out`'s file name without a `.csv`
/// extension.
pub fn csv_path(out: &Path, sheet: &str) -> PathBuf {
    let name = out
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = match name.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case("csv") && !stem.is_empty() => stem,
        _ => name.as_str(),
    };
    let stem = if stem.trim().is_empty() {
        "Budget"
    } else {
        stem
    };
    out.with_file_name(format!("{stem} - {sheet}.csv"))
}

/// Exports a budget straight from the file: recalculates it (the default
/// options: every group as the budget sets it) and builds every table.
/// Fails with the engine's message when the budget has a problem.
pub fn workbook_of(b: &Budget) -> Result<Workbook, String> {
    let r = fwb_core::recalc(b, &fwb_core::RecalcOptions::default()).map_err(|e| e.to_string())?;
    workbook(b, &r).map_err(|e| e.to_string())
}

/// `1234567` cents as `12345.67`, `-5` as `-0.05`: exact, no grouping.
pub fn money_text(c: Cents) -> String {
    let a = c.unsigned_abs();
    let sign = if c < 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", a / 100, a % 100)
}

/// The report's money for display: `#,##0.00` with negatives in
/// parentheses, as the app shows it. Used for column widths.
pub(crate) fn money_display_len(c: Cents) -> usize {
    let digits = (c.unsigned_abs() / 100).to_string().len();
    // digits, separators, point and cents, parentheses
    digits + (digits - 1) / 3 + 3 + if c < 0 { 2 } else { 0 }
}

/// Whether `r` was computed from `b`: same categories, accounts and rows.
pub(crate) fn lines_up(b: &Budget, r: &Report) -> bool {
    let cats: Vec<_> = b.categories().collect();
    cats.len() == r.categories.len()
        && cats.iter().zip(&r.categories).all(|(c, rc)| {
            c.code == rc.code
                && c.accounts.len() == rc.accounts.len()
                && c.accounts.iter().zip(&rc.accounts).all(|(a, ra)| {
                    a.code == ra.code
                        && a.details.len() == ra.rows.len()
                        && a.details.iter().zip(&ra.rows).all(|(d, rr)| d.id == rr.id)
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_is_exact_from_cents() {
        assert_eq!(money_text(0), "0.00");
        assert_eq!(money_text(5), "0.05");
        assert_eq!(money_text(-5), "-0.05");
        assert_eq!(money_text(100), "1.00");
        assert_eq!(money_text(123_456_789), "1234567.89");
        assert_eq!(money_text(-100_000), "-1000.00");
        // Values a float can't hold exactly stay exact.
        assert_eq!(money_text(9_007_199_254_740_993), "90071992547409.93");
        assert_eq!(money_text(i64::MAX), "92233720368547758.07");
        assert_eq!(money_text(i64::MIN), "-92233720368547758.08");
        // 0.1 + 0.2 in cents is exactly 0.30.
        assert_eq!(money_text(10 + 20), "0.30");
    }

    #[test]
    fn display_lengths() {
        assert_eq!(money_display_len(0), 4); // 0.00
        assert_eq!(money_display_len(99_999), 6); // 999.99
        assert_eq!(money_display_len(100_000), 8); // 1,000.00
        assert_eq!(money_display_len(-123_456_789), 14); // (1,234,567.89)
    }

    #[test]
    fn formats() {
        assert_eq!(Format::parse("XLSX"), Some(Format::Xlsx));
        assert_eq!(Format::parse(" csv "), Some(Format::Csv));
        assert_eq!(Format::parse("tsv"), Some(Format::Tsv));
        assert_eq!(Format::parse("xls"), None);
        assert_eq!(Format::Tsv.extension(), "txt");
    }

    #[test]
    fn csv_paths_sit_beside_the_chosen_name() {
        let p = |s: &str, sheet: &str| csv_path(Path::new(s), sheet);
        assert_eq!(
            p("/x/My Film.csv", "Topsheet"),
            PathBuf::from("/x/My Film - Topsheet.csv")
        );
        assert_eq!(
            p("/x/My Film.CSV", "Detail"),
            PathBuf::from("/x/My Film - Detail.csv")
        );
        assert_eq!(p("/x/v1.2", "Units"), PathBuf::from("/x/v1.2 - Units.csv"));
        assert_eq!(p("/x/.csv", "Sets"), PathBuf::from("/x/.csv - Sets.csv"));
        assert_eq!(p("", "Sets"), PathBuf::from("Budget - Sets.csv"));
    }
}
