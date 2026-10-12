//! Setup lists as spreadsheets (decisions.md, entry 50; docs/APP.md,
//! "Setup lists in a spreadsheet"): File > Export Setup Lists… writes the
//! budget's setup kinds as an .xlsx (a sheet per kind) or CSV files (one
//! per kind), and File > Import Setup Lists… reads them back with a column
//! mapping per sheet (method_format::setup::sheet), checks every row (problems
//! listed with their row numbers, nothing imported while any is left) and
//! hands the result to Import Setup's review (src/import_setup.rs), so what
//! differs keeps this budget's version unless the user picks otherwise and
//! the import is one undo step. Into a pack: open the pack as a budget tab
//! and import there.
//!
//! The dialogs are the host's (the desktop's src/setup_sheets.rs shows
//! Rust's and reads and writes the files); the page names the file by a
//! token. This reads the bytes the host hands in and makes the bytes it
//! writes.

use std::collections::BTreeSet;
use std::path::Path;

use method_format::export::{self, Format, Options, OutFile};
use method_format::setup::import::What;
use method_format::setup::sheet::{self, RowError, SheetMapping, SHEET_KINDS};
use serde::{Deserialize, Serialize};

use crate::import_setup::{self, ImportPreview, ImportSources};
use crate::session::Session;
use crate::sheet_import::{read_book, SheetSources};

/// One kind a sheet can hold, with its columns, for the page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SheetKind {
    pub kind: &'static str,
    /// Its sheet name ("Fringes").
    pub label: &'static str,
    /// `[key, title]` per column.
    pub columns: Vec<(&'static str, &'static str)>,
}

/// Every kind a sheet can hold.
pub fn sheet_kinds() -> Vec<SheetKind> {
    SHEET_KINDS
        .into_iter()
        .map(|w| SheetKind {
            kind: w.key(),
            label: sheet::sheet_name(w),
            columns: sheet::columns(w)
                .into_iter()
                .map(|c| (c.key, c.title))
                .collect(),
        })
        .collect()
}

fn parse_kind(k: &str) -> Result<What, String> {
    let w = What::parse(k)?;
    if !SHEET_KINDS.contains(&w) {
        return Err(format!(
            "A {} can't go in a spreadsheet.",
            k.replace('_', " ")
        ));
    }
    Ok(w)
}

/// One sheet of a picked file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PickedSheet {
    pub index: usize,
    pub name: String,
    pub rows: usize,
    /// The kind its name (or the file's) suggests, if any.
    pub kind: Option<&'static str>,
    /// The guessed mapping for that kind (none without a kind).
    pub mapping: Option<SheetMapping>,
    /// Its column titles under that mapping.
    pub titles: Vec<String>,
}

/// What `setup_sheet_pick` returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SetupSheetPicked {
    pub token: u64,
    /// The file's name (never its path).
    pub file: String,
    pub sheets: Vec<PickedSheet>,
    pub kinds: Vec<SheetKind>,
}

/// The sheets of `book` (from file `file`), each with its guessed kind and
/// mapping.
pub fn picked(token: u64, file: &str, book: &method_format::import::Book) -> SetupSheetPicked {
    let one = book.tables.len() == 1;
    let sheets = book
        .tables
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.hidden)
        .map(|(i, t)| {
            let kind = sheet::kind_named(&t.name)
                .or_else(|| one.then(|| sheet::kind_named(file)).flatten());
            let mapping = kind.map(|w| sheet::guess(t, w));
            let titles = match &mapping {
                Some(m) => sheet::titles(t, m),
                None => sheet::titles(t, &SheetMapping::default()),
            };
            PickedSheet {
                index: i,
                name: t.name.clone(),
                rows: t.rows.len(),
                kind: kind.map(What::key),
                mapping,
                titles,
            }
        })
        .collect();
    SetupSheetPicked {
        token,
        file: file.to_string(),
        sheets,
        kinds: sheet_kinds(),
    }
}

/// One sheet to import: which, as what, mapped how.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetChoice {
    pub index: usize,
    pub kind: String,
    pub mapping: SheetMapping,
}

/// What `setup_sheet_review` returns: the problems, or Import Setup's
/// review of what the sheets hold.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SheetReview {
    pub errors: Vec<RowError>,
    pub preview: Option<ImportPreview>,
}

/// Reads `choices` of `book` into a setup-only budget for `here`: the
/// budget, or every problem with its row.
pub fn read_choices(
    here: &method_core::Budget,
    book: &method_format::import::Book,
    choices: &[SheetChoice],
) -> Result<Result<method_core::Budget, Vec<RowError>>, String> {
    if choices.is_empty() {
        return Err("Pick at least one sheet to import.".into());
    }
    let mut src = sheet::empty_source("Spreadsheet", &here.base_currency);
    let known: BTreeSet<String> = here.globals.keys().cloned().collect();
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for c in choices {
        let w = parse_kind(&c.kind)?;
        if !seen.insert(c.index) {
            return Err("A sheet was picked twice.".into());
        }
        let t = book
            .tables
            .get(c.index)
            .ok_or("That sheet is no longer in the file.")?;
        errors.extend(sheet::read(t, w, &c.mapping, &known, &mut src));
    }
    Ok(if errors.is_empty() {
        Ok(src)
    } else {
        Err(errors)
    })
}

/// What `setup_sheet_export` wrote.
#[derive(Debug, Default, Serialize)]
pub struct SheetExported {
    pub done: bool,
    pub files: Vec<String>,
    pub message: String,
}

/// A budget's setup lists ready to write: the workbook, its format and the
/// name the Save dialog suggests.
pub struct SheetExport {
    pub workbook: export::Workbook,
    pub format: Format,
    /// The suggested file name without the extension ("Pilot setup").
    pub stem: String,
    /// Charges whose excluded amounts a spreadsheet doesn't keep.
    lost: usize,
}

impl SheetExport {
    /// The file's extension: `xlsx` or `csv`.
    pub fn ext(&self) -> &'static str {
        match self.format {
            Format::Xlsx => "xlsx",
            _ => "csv",
        }
    }

    /// The Save dialog's title.
    pub fn title(&self) -> &'static str {
        match self.format {
            Format::Xlsx => "Export setup lists as an Excel workbook",
            _ => "Export setup lists as CSV files",
        }
    }

    /// The Save dialog's filter name.
    pub fn filter(&self) -> &'static str {
        match self.format {
            Format::Xlsx => "Excel workbook",
            _ => "CSV",
        }
    }

    /// The files to write for a save to `out`: one workbook, or a CSV file
    /// per kind beside it.
    pub fn files(&self, out: &Path) -> Result<Vec<OutFile>, String> {
        export::render(
            &self.workbook,
            self.format,
            out,
            Options {
                bom: self.format == Format::Csv,
            },
        )
        .map_err(|e| e.to_string())
    }

    /// What the page says once `files` (their names) are written.
    pub fn exported(&self, files: Vec<String>) -> SheetExported {
        let mut message = format!("Exported {}.", files.join(", "));
        if self.lost > 0 {
            message.push_str(&format!(
                " {} excluded amounts, which a spreadsheet doesn't keep.",
                if self.lost == 1 {
                    "One charge has".to_string()
                } else {
                    format!("{} charges have", self.lost)
                }
            ));
        }
        SheetExported {
            done: true,
            files,
            message,
        }
    }
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/setup_sheets.rs and the host's
// src/host/setup_sheets.rs call these)

/// Budget `s`'s setup `kinds` (every kind when empty) as `format` (`xlsx`
/// or `csv`), ready for the host to save.
pub fn setup_sheet_export(
    s: &Session,
    kinds: &[String],
    format: &str,
) -> Result<SheetExport, String> {
    let format = match format {
        "xlsx" => Format::Xlsx,
        "csv" => Format::Csv,
        _ => return Err("Setup lists export as an Excel workbook or CSV files.".into()),
    };
    let kinds = kinds
        .iter()
        .map(|k| parse_kind(k))
        .collect::<Result<Vec<_>, _>>()?;
    let b = s.budget();
    let stem = match s.path().and_then(|p| p.file_stem()) {
        Some(st) => st.to_string_lossy().into_owned(),
        None => crate::templates::safe_file_stem(&b.title),
    };
    let workbook = sheet::workbook(b, &kinds);
    if workbook.sheets.is_empty() {
        return Err("Pick at least one kind of setup item.".into());
    }
    Ok(SheetExport {
        workbook,
        format,
        stem: format!("{stem} setup"),
        lost: if kinds.is_empty() || kinds.contains(&What::Charge) {
            sheet::charges_losing_amounts(b)
        } else {
            0
        },
    })
}

/// Reads a picked spreadsheet of setup lists (`bytes` from a file named
/// `file`), keeps it and returns its sheets with guessed kinds and
/// mappings.
pub fn setup_sheet_pick(
    sources: &SheetSources,
    file: String,
    bytes: &[u8],
) -> Result<SetupSheetPicked, String> {
    let book = read_book(&file, bytes)?;
    let token = sources.keep(file.clone(), book);
    let (_, book) = sources.get(token)?;
    Ok(picked(token, &file, &book))
}

/// The guessed mapping of sheet `index` as `kind`, and its titles.
pub fn setup_sheet_guess(
    sources: &SheetSources,
    token: u64,
    index: usize,
    kind: &str,
) -> Result<PickedSheet, String> {
    let (_, book) = sources.get(token)?;
    let w = parse_kind(kind)?;
    let t = book
        .tables
        .get(index)
        .ok_or("That sheet is no longer in the file.")?;
    let m = sheet::guess(t, w);
    Ok(PickedSheet {
        index,
        name: t.name.clone(),
        rows: t.rows.len(),
        kind: Some(w.key()),
        titles: sheet::titles(t, &m),
        mapping: Some(m),
    })
}

/// Reads the chosen sheets for budget `s`: their problems with row
/// numbers, or (with none) Import Setup's review, whose token
/// `import_setup` takes.
pub fn setup_sheet_review(
    s: &Session,
    sheets: &SheetSources,
    imports: &ImportSources,
    token: u64,
    choices: &[SheetChoice],
) -> Result<SheetReview, String> {
    let (file, book) = sheets.get(token)?;
    match read_choices(s.budget(), &book, choices)? {
        Err(errors) => Ok(SheetReview {
            errors,
            preview: None,
        }),
        Ok(src) => {
            let p = import_setup::preview(s.budget(), &src, 0, &file, "");
            let token = imports.keep(file, src);
            Ok(SheetReview {
                errors: Vec::new(),
                preview: Some(ImportPreview { token, ..p }),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use method_core::starter;
    use method_format::import::read_file;

    #[test]
    fn sheets_are_guessed_read_and_reviewed() {
        let mut b = starter::budget();
        b.globals.insert("DAYS".into(), "20".into());
        let wb = sheet::workbook(&b, &[What::Global, What::Fringe]);
        let bytes = method_format::export::xlsx::write(&wb).unwrap();
        let book = read_file(&bytes, "Show setup.xlsx").unwrap();
        let p = picked(1, "Show setup.xlsx", &book);
        assert_eq!(p.sheets.len(), 2);
        assert_eq!(p.sheets[0].kind, Some("global"));
        assert_eq!(p.sheets[1].kind, Some("fringe"));
        assert_eq!(p.kinds.len(), SHEET_KINDS.len());
        let choices: Vec<SheetChoice> = p
            .sheets
            .iter()
            .map(|s| SheetChoice {
                index: s.index,
                kind: s.kind.unwrap().to_string(),
                mapping: s.mapping.clone().unwrap(),
            })
            .collect();
        // Into a budget that has them all: nothing differs.
        let src = read_choices(&b, &book, &choices).unwrap().unwrap();
        assert_eq!(src.globals, b.globals);
        let review = import_setup::preview(&b, &src, 0, "x", "");
        assert!(review
            .items
            .iter()
            .all(|i| i.status == "same" || i.kind.contains("_")));

        // Problems come back with rows, and nothing is built.
        let csv = "Name,Formula\nGOOD,1\nBAD NAME,1\nREF,MISSING + 1\n";
        let book = read_file(csv.as_bytes(), "Globals.csv").unwrap();
        let p = picked(2, "Globals.csv", &book);
        assert_eq!(p.sheets[0].kind, Some("global"));
        let choice = SheetChoice {
            index: 0,
            kind: "global".into(),
            mapping: p.sheets[0].mapping.clone().unwrap(),
        };
        let errs = read_choices(
            Session::new().budget(),
            &book,
            std::slice::from_ref(&choice),
        )
        .unwrap()
        .unwrap_err();
        let rows: Vec<usize> = errs.iter().map(|e| e.row).collect();
        assert_eq!(rows, [3, 4]);
        // A kind no sheet holds, a sheet twice, no sheets: refused.
        let bad = SheetChoice {
            kind: "fringe_posting".into(),
            ..choice.clone()
        };
        assert!(read_choices(&b, &book, &[bad]).is_err());
        assert!(read_choices(&b, &book, &[choice.clone(), choice]).is_err());
        assert!(read_choices(&b, &book, &[]).is_err());
    }
}
