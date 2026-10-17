//! A saved column mapping as a small JSON file, for importing without the
//! app (`method-budget import --mapping`, docs/CLI.md). The app keeps its own saved
//! mappings in the page's storage; this is the same choices written down.
//!
//! ```json
//! {
//!   "sheet": "Detail",
//!   "header_row": 1,
//!   "columns": { "account_code": "A", "description": "B", "amount": 3, "rate": "E" },
//!   "create_accounts": false,
//!   "create_setup": false,
//!   "default_account": ""
//! }
//! ```
//!
//! Every field may be left out. Rows and numbered columns count from 1, as a
//! spreadsheet shows them; columns may also be letters (`"A"`, `"AA"`).
//! Without `sheet`, the visible sheet whose titles name the most fields (as
//! the app's import dialog starts); without
//! `header_row`, the header row is found (`"none"`: no titles, data from
//! the first row); without `columns`, every column is guessed from its
//! title. Field names are the import's own keys (`account_code`, `rate`,
//! `line_total`...).

use std::collections::BTreeMap;

use serde::Deserialize;

use super::mapping::{self, Field, Mapping};
use super::merge::Options;
use super::{Book, Table};

#[derive(Deserialize)]
#[serde(untagged)]
enum SheetRef {
    Number(usize),
    Name(String),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RowRef {
    Number(usize),
    Word(String),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ColumnRef {
    Number(usize),
    Letters(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    #[serde(default)]
    sheet: Option<SheetRef>,
    #[serde(default)]
    header_row: Option<RowRef>,
    #[serde(default)]
    columns: Option<BTreeMap<String, ColumnRef>>,
    #[serde(default)]
    create_accounts: bool,
    #[serde(default)]
    create_setup: bool,
    #[serde(default)]
    default_account: String,
}

/// Which sheet a mapping file names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SheetChoice {
    /// The first visible sheet with rows.
    First,
    /// By position, from 0.
    Index(usize),
    /// By name, ignoring case.
    Name(String),
}

/// The header row a mapping file names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderChoice {
    Detect,
    None,
    /// From 0.
    Row(usize),
}

/// A mapping file, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingFile {
    pub sheet: SheetChoice,
    pub header: HeaderChoice,
    /// Field to column (from 0). `None`: guess from the titles.
    pub columns: Option<BTreeMap<Field, usize>>,
    pub options: Options,
}

/// `A` as 0, `Z` as 25, `AA` as 26.
pub(crate) fn column_letters(s: &str) -> Option<usize> {
    let s = s.trim();
    if s.is_empty() || s.len() > 3 || !s.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let n = s.bytes().fold(0usize, |n, b| {
        n * 26 + usize::from(b.to_ascii_uppercase() - b'A') + 1
    });
    Some(n - 1)
}

/// Reads a mapping file.
pub fn parse(text: &str) -> Result<MappingFile, String> {
    let raw: Raw = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let sheet = match raw.sheet {
        None => SheetChoice::First,
        Some(SheetRef::Number(0)) => return Err("sheets count from 1".into()),
        Some(SheetRef::Number(n)) => SheetChoice::Index(n - 1),
        Some(SheetRef::Name(s)) => SheetChoice::Name(s),
    };
    let header = match raw.header_row {
        None => HeaderChoice::Detect,
        Some(RowRef::Number(0)) => return Err("rows count from 1".into()),
        Some(RowRef::Number(n)) => HeaderChoice::Row(n - 1),
        Some(RowRef::Word(w)) if w == "none" => HeaderChoice::None,
        Some(RowRef::Word(w)) if w == "detect" => HeaderChoice::Detect,
        Some(RowRef::Word(w)) => {
            return Err(format!(
                "header_row `{w}`: use a row number, \"none\" or \"detect\""
            ))
        }
    };
    let columns = match raw.columns {
        None => None,
        Some(map) => {
            let mut out = BTreeMap::new();
            for (key, c) in map {
                let f = Field::parse(&key).ok_or_else(|| {
                    let keys: Vec<&str> = Field::ALL.iter().map(|f| f.key()).collect();
                    format!("unknown field `{key}`: use {}", keys.join(", "))
                })?;
                let col = match c {
                    ColumnRef::Number(0) => return Err("columns count from 1".into()),
                    ColumnRef::Number(n) => n - 1,
                    ColumnRef::Letters(s) => column_letters(&s)
                        .ok_or_else(|| format!("`{s}` is not a column (use A, B... or 1, 2...)"))?,
                };
                out.insert(f, col);
            }
            Some(out)
        }
    };
    Ok(MappingFile {
        sheet,
        header,
        columns,
        options: Options {
            create_accounts: raw.create_accounts,
            create_setup: raw.create_setup,
            default_account: raw.default_account.trim().to_string(),
        },
    })
}

/// The visible sheet whose titles name the most fields (our own export's
/// Detail sheet, a department's budget sheet), else the first: the sheet
/// the app's import dialog starts on.
fn best_sheet(book: &Book) -> Option<&Table> {
    let mut best: Option<(usize, &Table)> = None;
    for t in &book.tables {
        if t.hidden || t.rows.is_empty() {
            continue;
        }
        let n = mapping::guess(t, mapping::detect_header(t)).len();
        if best.is_none_or(|(b, _)| n > b) {
            best = Some((n, t));
        }
    }
    best.map(|(_, t)| t).or_else(|| book.tables.first())
}

impl MappingFile {
    /// The table and mapping this file means for `book`.
    pub fn resolve<'b>(&self, book: &'b Book) -> Result<(&'b Table, Mapping), String> {
        let t = match &self.sheet {
            SheetChoice::First => best_sheet(book),
            SheetChoice::Index(i) => book.tables.get(*i),
            SheetChoice::Name(n) => book
                .tables
                .iter()
                .find(|t| t.name.eq_ignore_ascii_case(n.trim())),
        }
        .ok_or_else(|| {
            let names: Vec<String> = book
                .tables
                .iter()
                .map(|t| format!("\"{}\"", t.name))
                .collect();
            format!(
                "that sheet isn't in the file (sheets: {})",
                names.join(", ")
            )
        })?;
        let header_row = match self.header {
            HeaderChoice::Detect => mapping::detect_header(t),
            HeaderChoice::None => None,
            HeaderChoice::Row(r) if r < t.rows.len().max(1) => Some(r),
            HeaderChoice::Row(_) => {
                return Err("the header row is past the end of the sheet".into())
            }
        };
        let columns = match &self.columns {
            None => mapping::guess(t, header_row),
            Some(cols) => {
                let width = t.width();
                for (f, &c) in cols {
                    if c >= width {
                        return Err(format!(
                            "the {} column is past the sheet's last column",
                            f.name()
                        ));
                    }
                }
                cols.clone()
            }
        };
        Ok((
            t,
            Mapping {
                header_row,
                columns,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters() {
        assert_eq!(column_letters("A"), Some(0));
        assert_eq!(column_letters("z"), Some(25));
        assert_eq!(column_letters("AA"), Some(26));
        assert_eq!(column_letters("A1"), None);
        assert_eq!(column_letters(""), None);
    }

    #[test]
    fn parses() {
        let m = parse(
            r#"{"sheet": "Detail", "header_row": 2, "columns": {"rate": "E", "amount": 3},
                "create_accounts": true}"#,
        )
        .unwrap();
        assert_eq!(m.sheet, SheetChoice::Name("Detail".into()));
        assert_eq!(m.header, HeaderChoice::Row(1));
        let cols = m.columns.unwrap();
        assert_eq!(cols[&Field::Rate], 4);
        assert_eq!(cols[&Field::Amount], 2);
        assert!(m.options.create_accounts);
        let m = parse("{}").unwrap();
        assert_eq!(m.sheet, SheetChoice::First);
        assert_eq!(m.header, HeaderChoice::Detect);
        assert!(m.columns.is_none());
        assert_eq!(
            parse(r#"{"header_row": "none"}"#).unwrap().header,
            HeaderChoice::None
        );
        for bad in [
            r#"{"columns": {"nope": 1}}"#,
            r#"{"columns": {"rate": 0}}"#,
            r#"{"columns": {"rate": "1A"}}"#,
            r#"{"header_row": "first"}"#,
            r#"{"sheet": 0}"#,
            r#"{"extra": 1}"#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
