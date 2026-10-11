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
//! The dialogs are Rust's; the page names the file by a token.

use std::collections::BTreeSet;
use std::path::PathBuf;

use method_format::export::{self, Format, Options};
use method_format::import::read_file;
use method_format::setup::import::What;
use method_format::setup::sheet::{self, RowError, SheetMapping, SHEET_KINDS};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{AppState, DocId};
use crate::import_setup::{self, ImportPreview, ImportSources};
use crate::sheet_import::{read_capped, SheetSources};

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

// ---------------------------------------------------------------------------
// Commands (registered in app.rs and build.rs; the main window's File menu
// only, capabilities/main.json)

/// What `setup_sheet_export` wrote.
#[derive(Debug, Default, Serialize)]
pub struct SheetExported {
    pub done: bool,
    pub files: Vec<String>,
    pub message: String,
}

/// Writes budget `doc`'s setup `kinds` (every kind when empty) as `format`
/// (`xlsx` or `csv`) where Rust's Save dialog says.
#[tauri::command]
pub async fn setup_sheet_export<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    kinds: Vec<String>,
    format: String,
) -> Result<SheetExported, String> {
    let format = match format.as_str() {
        "xlsx" => Format::Xlsx,
        "csv" => Format::Csv,
        _ => return Err("Setup lists export as an Excel workbook or CSV files.".into()),
    };
    let kinds = kinds
        .iter()
        .map(|k| parse_kind(k))
        .collect::<Result<Vec<_>, _>>()?;
    let (wb, stem, dir, lost) = {
        let s = state.lock_doc(doc)?;
        let b = s.budget();
        let stem = match s.path().and_then(|p| p.file_stem()) {
            Some(st) => st.to_string_lossy().into_owned(),
            None => crate::app::safe_file_stem(&b.title),
        };
        (
            sheet::workbook(b, &kinds),
            format!("{stem} setup"),
            s.path().and_then(|p| p.parent()).map(PathBuf::from),
            if kinds.is_empty() || kinds.contains(&What::Charge) {
                sheet::charges_losing_amounts(b)
            } else {
                0
            },
        )
    };
    if wb.sheets.is_empty() {
        return Err("Pick at least one kind of setup item.".into());
    }
    let (title, ext) = match format {
        Format::Xlsx => ("Export setup lists as an Excel workbook", "xlsx"),
        _ => ("Export setup lists as CSV files", "csv"),
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title(title)
        .add_filter(
            if ext == "xlsx" {
                "Excel workbook"
            } else {
                "CSV"
            },
            &[ext],
        )
        .set_file_name(format!("{stem}.{ext}"))
        .set_parent(&window);
    if let Some(dir) = dir {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_save_file() else {
        return Ok(SheetExported::default());
    };
    let out = crate::export::with_extension(picked.into_path().map_err(|e| e.to_string())?, format);
    let files = export::render(
        &wb,
        format,
        &out,
        Options {
            bom: format == Format::Csv,
        },
    )
    .map_err(|e| e.to_string())?;
    let files = crate::export::write_files(&state.docs(), &files)?;
    let mut message = format!("Exported {}.", files.join(", "));
    if lost > 0 {
        message.push_str(&format!(
            " {} excluded amounts, which a spreadsheet doesn't keep.",
            if lost == 1 {
                "One charge has".to_string()
            } else {
                format!("{lost} charges have")
            }
        ));
    }
    Ok(SheetExported {
        done: true,
        files,
        message,
    })
}

/// Shows an Open dialog for a spreadsheet of setup lists, reads it and
/// returns its sheets with guessed kinds and mappings. `None` when
/// cancelled.
#[tauri::command]
pub async fn setup_sheet_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, SheetSources>,
    doc: Option<DocId>,
) -> Result<Option<SetupSheetPicked>, String> {
    let start = {
        let s = state.lock_doc(doc)?;
        s.path().and_then(|p| p.parent()).map(PathBuf::from)
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Import setup lists")
        .add_filter("Spreadsheet", &["xlsx", "csv", "tsv", "txt"])
        .set_parent(&window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(file) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bytes = read_capped(&path)?;
    let book = read_file(&bytes, &name).map_err(|e| format!("Couldn't read {name}: {e}."))?;
    let token = sources.keep(name.clone(), book);
    let (_, book) = sources.get(token)?;
    Ok(Some(picked(token, &name, &book)))
}

/// The guessed mapping of sheet `index` as `kind`, and its titles.
#[tauri::command]
pub fn setup_sheet_guess(
    sources: State<'_, SheetSources>,
    token: u64,
    index: usize,
    kind: String,
) -> Result<PickedSheet, String> {
    let (_, book) = sources.get(token)?;
    let w = parse_kind(&kind)?;
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

/// Reads the chosen sheets for budget `doc`: their problems with row
/// numbers, or (with none) Import Setup's review, whose token
/// `import_setup` takes.
#[tauri::command]
pub fn setup_sheet_review(
    state: State<'_, AppState>,
    sheets: State<'_, SheetSources>,
    imports: State<'_, ImportSources>,
    doc: Option<DocId>,
    token: u64,
    choices: Vec<SheetChoice>,
) -> Result<SheetReview, String> {
    let (file, book) = sheets.get(token)?;
    let s = state.lock_doc(doc)?;
    match read_choices(s.budget(), &book, &choices)? {
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

    #[test]
    fn sheet_commands_are_wired_for_the_main_window_only() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN: &str = include_str!("../capabilities/main.json");
        const TOOLS: &str = include_str!("../capabilities/tools.json");
        const SETUP: &str = include_str!("../capabilities/setup.json");
        for c in [
            "setup_sheet_export",
            "setup_sheet_pick",
            "setup_sheet_guess",
            "setup_sheet_review",
        ] {
            assert!(
                APP_RS.contains(&format!("crate::setup_sheets::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN.contains(&allow), "{c} not allowed for the main window");
            assert!(!TOOLS.contains(&allow), "{c} allowed for the tools window");
            assert!(!SETUP.contains(&allow), "{c} allowed for the setup window");
        }
    }
}
