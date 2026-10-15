//! Spreadsheet import: reading Excel workbooks (.xlsx), CSV and
//! tab-delimited files into plain [`Table`]s, then mapping a table's columns
//! to budget fields ([`mapping`]) and adding its rows to a budget as detail
//! lines ([`merge`]).
//!
//! Nothing here is specific to one program's export: the readers take any
//! workbook or delimited file, and the mapping says which column is which.
//! Importing another tool's export (roadmap 1.16) is this with a preset
//! mapping and a reader for its layout.
//!
//! Every file is untrusted input. Everything is plain std, read by hand:
//!
//! - [`inflate`]: DEFLATE (RFC 1951), with an output cap;
//! - [`zipread`]: the ZIP directory and entries, with size, ratio and CRC
//!   checks;
//! - [`xml`]: a small XML tokenizer that refuses DTDs and unknown entities;
//! - [`xlsxread`]: the SpreadsheetML parts (workbook, relationships, shared
//!   strings, sheets);
//! - [`csvread`]: RFC 4180 CSV and tab-delimited text, byte order marks,
//!   UTF-16, Windows-1252, delimiter sniffing.
//!
//! Limits on what a table holds ([`MAX_ROWS`], [`MAX_COLS`],
//! [`MAX_CELL_CHARS`]) keep a hostile file from using up memory; a table cut
//! short by them says so ([`Table::truncated`]).

pub mod actuals;
/// A chart of accounts (numbers, names, levels) from a sheet or a budget.
pub mod coa;
pub mod csvread;
/// A whole budget from another program's open exports (JSON, XML, Excel,
/// tab-delimited), with a reconciliation against the export's figures.
pub mod exchange;
pub mod inflate;
pub mod mapping;
/// A column mapping saved as a JSON file (`method import --mapping`).
pub mod mapping_file;
pub mod merge;
/// A schedule's day counts from Method Scheduling's budget exchange file
/// (`method-schedule-days`), for schedule-linked globals.
pub mod schedule;
pub mod xlsxread;
pub mod xml;
pub mod zipread;

use std::fmt;

/// Most rows read from one sheet or file.
pub const MAX_ROWS: usize = 100_000;
/// Most columns read from one sheet or file.
pub const MAX_COLS: usize = 256;
/// Longest text kept in one cell, in characters (Excel's own limit).
pub const MAX_CELL_CHARS: usize = 32_767;
/// Most cells (counting the empty ones a row is padded with) read from one
/// sheet or file, so a sheet of far-apart cells can't use up memory.
pub const MAX_CELLS: usize = 2_000_000;
/// Most bytes of text placed in the cells of one workbook (a shared string
/// is copied into every cell that uses it).
pub const MAX_TEXT_BYTES: usize = 128 << 20;
/// Largest file the app reads for import, in bytes.
pub const MAX_FILE_BYTES: usize = 50 << 20;

/// Why a file could not be read, in words for the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadError(pub String);

impl ReadError {
    pub fn new(msg: impl Into<String>) -> ReadError {
        ReadError(msg.into())
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ReadError {}

/// One cell as read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Empty,
    Text(String),
    /// A number cell, as the file wrote it (`1234.5`, `1.5E-3`). Kept as
    /// text so nothing passes through a float.
    Number(String),
    Bool(bool),
}

impl Value {
    /// The cell as text: a number as written, a boolean as `TRUE`/`FALSE`.
    pub fn text(&self) -> &str {
        match self {
            Value::Empty => "",
            Value::Text(s) | Value::Number(s) => s,
            Value::Bool(true) => "TRUE",
            Value::Bool(false) => "FALSE",
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Value::Empty => true,
            Value::Text(s) => s.trim().is_empty(),
            _ => false,
        }
    }
}

/// One sheet of a workbook, or a whole delimited file: rows of cells, row
/// `i` being the sheet's row `i + 1`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    pub name: String,
    /// A sheet the workbook hides.
    pub hidden: bool,
    pub rows: Vec<Vec<Value>>,
    /// Rows, columns or text were left out to stay within [`MAX_ROWS`],
    /// [`MAX_COLS`] or [`MAX_CELL_CHARS`].
    pub truncated: bool,
    /// Numbers in text cells use a decimal comma (`1.234,56`): set for
    /// semicolon-delimited files, which come from places that write them.
    pub decimal_comma: bool,
    /// Date serial numbers count from 1904-01-01 (serial 0), not the usual
    /// 1900 system: set for a workbook that says so (`date1904`, older Mac
    /// Excel). Every sheet of such a workbook has it; `false` otherwise.
    pub date1904: bool,
}

impl Table {
    /// The widest row's length.
    pub fn width(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }

    /// The cell at row `r`, column `c` (empty past the edges).
    pub fn cell(&self, r: usize, c: usize) -> &Value {
        self.rows
            .get(r)
            .and_then(|row| row.get(c))
            .unwrap_or(&Value::Empty)
    }

    /// Drops empty cells at the ends of rows and empty rows at the end.
    pub(crate) fn trim(&mut self) {
        for row in &mut self.rows {
            while row.last().is_some_and(Value::is_empty) {
                row.pop();
            }
        }
        while self.rows.last().is_some_and(Vec::is_empty) {
            self.rows.pop();
        }
    }

    /// Puts `v` at row `r`, column `c`, within the limits (marking the table
    /// truncated when it can't). Returns how many cells the row grew by.
    pub(crate) fn put(&mut self, r: usize, c: usize, v: Value) -> usize {
        if r >= MAX_ROWS || c >= MAX_COLS {
            self.truncated = true;
            return 0;
        }
        if self.rows.len() <= r {
            self.rows.resize_with(r + 1, Vec::new);
        }
        let row = &mut self.rows[r];
        let grew = (c + 1).saturating_sub(row.len());
        if grew > 0 {
            row.resize(c + 1, Value::Empty);
        }
        row[c] = v;
        grew
    }
}

/// What kind of file was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Xlsx,
    Csv,
    Tsv,
}

impl FileKind {
    pub fn key(self) -> &'static str {
        match self {
            FileKind::Xlsx => "xlsx",
            FileKind::Csv => "csv",
            FileKind::Tsv => "tsv",
        }
    }
}

/// A file read for import: one table per sheet (one for a delimited file).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Book {
    pub kind: FileKind,
    pub tables: Vec<Table>,
}

/// Reads `bytes`, a file named `file_name` (the name only guides the reader:
/// a ZIP is read as a workbook whatever it is called, and a `.tsv` or `.txt`
/// file is first tried as tab-delimited).
pub fn read_file(bytes: &[u8], file_name: &str) -> Result<Book, ReadError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ReadError::new(format!(
            "the file is too large to import (more than {} MB)",
            MAX_FILE_BYTES >> 20
        )));
    }
    let ext = file_name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        return Ok(Book {
            kind: FileKind::Xlsx,
            tables: xlsxread::read_xlsx(bytes)?,
        });
    }
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return Err(ReadError::new(
            "this is an older Excel workbook (.xls); save it as an Excel Workbook (.xlsx) or CSV to import it",
        ));
    }
    if matches!(ext.as_str(), "xlsx" | "xlsm" | "xltx") {
        return Err(ReadError::new(
            "the file isn't an Excel workbook (it isn't a ZIP package)",
        ));
    }
    let hint = match ext.as_str() {
        "tsv" | "tab" | "txt" => Some(csvread::Delimiter::Tab),
        _ => None,
    };
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(s, _)| s)
        .to_string();
    let read = csvread::read_csv(bytes, &stem, hint)?;
    let kind = if read.delimiter == csvread::Delimiter::Tab {
        FileKind::Tsv
    } else {
        FileKind::Csv
    };
    Ok(Book {
        kind,
        tables: vec![read.table],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_place_and_trim_cells() {
        let mut t = Table::default();
        t.put(2, 1, Value::Text("x".into()));
        t.put(0, 0, Value::Number("1".into()));
        t.put(0, 3, Value::Text("  ".into()));
        t.put(4, 0, Value::Empty);
        t.trim();
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.rows[0], vec![Value::Number("1".into())]);
        assert!(t.rows[1].is_empty());
        assert_eq!(t.cell(2, 1).text(), "x");
        assert_eq!(t.cell(9, 9), &Value::Empty);
        assert_eq!(t.width(), 2);
        t.put(MAX_ROWS, 0, Value::Bool(true));
        t.put(0, MAX_COLS, Value::Bool(true));
        assert!(t.truncated);
        assert_eq!(t.rows.len(), 3);
    }

    #[test]
    fn files_are_read_by_content() {
        let book = read_file(b"Account,Rate\r\n1100,50\r\n", "sheet.xlsx.csv").unwrap();
        assert_eq!(book.kind, FileKind::Csv);
        assert_eq!(book.tables[0].name, "sheet.xlsx");
        let book = read_file(b"Account\tRate\n1100\t50\n", "export.txt").unwrap();
        assert_eq!(book.kind, FileKind::Tsv);
        assert!(read_file(b"Account,Rate", "budget.xlsx")
            .unwrap_err()
            .0
            .contains("ZIP"));
        assert!(read_file(&[0xD0, 0xCF, 0x11, 0xE0, 0, 0], "old.xls")
            .unwrap_err()
            .0
            .contains(".xls"));
    }
}
