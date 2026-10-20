//! Rate tables the user keeps (roadmap 3.9; decisions.md, entry 90): what
//! Budget Setup's Rate Tables tab and a line's Fill Rate from Table call.
//! The rules are `method_core::rates`; these read what the page sends, make
//! each change one undo step, and say how a filled line stands against its
//! table now. Nothing here changes a figure on its own: filling and
//! updating are always edits the person asks for.
//!
//! Figures cross the bridge as strings, never JSON floats.

use std::collections::BTreeMap;

use method_core::rates::{self, RateCheck, RateRow, RateTable};
use method_core::{Budget, Dec, Detail, RowId, RowKind};
use method_format::import::Book;
use method_format::setup::rates::{self as sheet_rates, RateMapping};
use method_format::setup::sheet::RowError;
use serde::{Deserialize, Serialize};

use crate::edit::{details, EditResult};
use crate::session::Session;
use crate::sheet_import::{read_book, SheetSources};

/// A row as the page sends and gets it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateRowDto {
    pub classification: String,
    /// Unit name to the rate per one, as typed.
    #[serde(default)]
    pub rates: BTreeMap<String, String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub notes: String,
}

/// A table as the page sends and gets it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateTableDto {
    pub name: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub rows: Vec<RateRowDto>,
}

fn blank_to_none(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

impl RateTableDto {
    /// The table, trimmed and checked (blank rates left out; a row with no
    /// classification and no rates is dropped as an empty line).
    pub fn table(&self) -> EditResult<RateTable> {
        let mut rows = Vec::with_capacity(self.rows.len());
        for (i, r) in self.rows.iter().enumerate() {
            let classification = r.classification.trim().to_string();
            let mut rates = BTreeMap::new();
            for (u, v) in &r.rates {
                let v = v.trim();
                if v.is_empty() {
                    continue;
                }
                let n: Dec = v.replace(',', "").parse().map_err(|_| {
                    format!(
                        "Row {} ({}): the rate per {u}, \u{201c}{v}\u{201d}, isn't a number.",
                        i + 1,
                        classification
                    )
                })?;
                rates.insert(u.trim().to_string(), n);
            }
            if classification.is_empty() && rates.is_empty() && r.notes.trim().is_empty() {
                continue;
            }
            rows.push(RateRow {
                classification,
                rates,
                from: blank_to_none(&r.from),
                to: blank_to_none(&r.to),
                notes: r.notes.trim().to_string(),
            });
        }
        let t = RateTable {
            name: rates::valid_table_name(&self.name).map_err(|e| e.0)?,
            notes: self.notes.trim().to_string(),
            rows,
        };
        t.validate().map_err(|e| e.0)?;
        Ok(t)
    }
}

impl From<&RateTable> for RateTableDto {
    fn from(t: &RateTable) -> RateTableDto {
        RateTableDto {
            name: t.name.clone(),
            notes: t.notes.clone(),
            rows: t
                .rows
                .iter()
                .map(|r| RateRowDto {
                    classification: r.classification.clone(),
                    rates: r
                        .rates
                        .iter()
                        .map(|(u, v)| (u.clone(), v.to_string()))
                        .collect(),
                    from: r.from.clone(),
                    to: r.to.clone(),
                    notes: r.notes.clone(),
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// How a line stands

/// How a line filled from a table stands, for the grid and the menus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RateCheckDto {
    /// `current`, `changed`, `edited` or `missing`.
    pub state: &'static str,
    /// What the table says now, when `changed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub now: Option<String>,
    /// Where the rate came from and how it stands, in words.
    pub words: String,
}

/// How line `d` stands against its table; `None` when it wasn't filled
/// from one.
pub fn check_dto(b: &Budget, d: &Detail) -> Option<RateCheckDto> {
    let c = rates::check(b, d)?;
    Some(RateCheckDto {
        state: c.key(),
        now: match &c {
            RateCheck::Changed { now } => Some(now.to_string()),
            _ => None,
        },
        words: rates::source_words(b, d).unwrap_or_default(),
    })
}

/// Every line among `rows` filled from a table, by id, with how it stands.
pub fn checks(b: &Budget, rows: &[RowId]) -> BTreeMap<RowId, RateCheckDto> {
    details(b)
        .filter(|d| d.rate_source.is_some() && rows.contains(&d.id))
        .filter_map(|d| check_dto(b, d).map(|c| (d.id, c)))
        .collect()
}

// ---------------------------------------------------------------------------
// Choices for a line

/// One classification and period a line could take its rate from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RateChoice {
    pub table: String,
    pub classification: String,
    /// The period in words ("" when the row has no dates).
    pub period: String,
    /// The date to fill with, to pick this period (`None` for a row with
    /// no dates).
    pub as_of: Option<String>,
    /// The rate for the line's unit, when the table gives one.
    pub rate: Option<String>,
    /// The table's own unit for it, and its figure.
    pub table_unit: Option<String>,
    pub table_rate: Option<String>,
    pub converted: bool,
    /// Why there is no rate for the line's unit.
    pub problem: Option<String>,
    /// The row's notes.
    pub notes: String,
}

/// What `rate_choices` returns for a line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RateChoices {
    pub row_id: RowId,
    /// The unit a fill would use (the line's rate unit, else its amount
    /// unit); empty when the line has none.
    pub unit: String,
    /// The budget's units, for the picker's unit choice.
    pub units: Vec<String>,
    pub choices: Vec<RateChoice>,
    /// How the line stands now, when it was filled from a table.
    pub current: Option<RateCheckDto>,
}

fn line(b: &Budget, row: RowId) -> EditResult<&Detail> {
    details(b)
        .find(|d| d.id == row)
        .ok_or_else(|| "That line is no longer in the budget.".to_string())
}

/// Every classification and period line `row` could take its rate from,
/// per `unit` (the line's own when `None`), in table order.
pub fn choices(b: &Budget, row: RowId, unit: Option<&str>) -> EditResult<RateChoices> {
    let d = line(b, row)?;
    if d.kind != RowKind::Line {
        return Err("Only a line has a rate to fill.".into());
    }
    let unit = unit
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if d.unit3.trim().is_empty() {
                d.unit.trim().to_string()
            } else {
                d.unit3.trim().to_string()
            }
        });
    let mut out = Vec::new();
    for t in &b.rate_tables {
        for r in &t.rows {
            let as_of = r.pick_date().map(str::to_string);
            // With no unit, the row's only one (as a fill does).
            let u = if unit.is_empty() && r.rates.len() == 1 {
                r.rates.keys().next().cloned().unwrap_or_default()
            } else {
                unit.clone()
            };
            let found = if u.is_empty() {
                Err(rates::RateError(
                    "The line has no unit: pick one for the rate.".into(),
                ))
            } else {
                rates::rate_for(r, &u, &b.units, &t.name)
            };
            out.push(match found {
                Ok(f) => RateChoice {
                    table: t.name.clone(),
                    classification: r.classification.clone(),
                    period: r.period(),
                    as_of,
                    rate: Some(f.rate.to_string()),
                    table_unit: Some(f.table_unit.clone()),
                    table_rate: Some(f.table_rate.to_string()),
                    converted: f.converted.is_some(),
                    problem: None,
                    notes: r.notes.clone(),
                },
                Err(e) => RateChoice {
                    table: t.name.clone(),
                    classification: r.classification.clone(),
                    period: r.period(),
                    as_of,
                    rate: None,
                    table_unit: None,
                    table_rate: None,
                    converted: false,
                    problem: Some(e.0),
                    notes: r.notes.clone(),
                },
            });
        }
    }
    Ok(RateChoices {
        row_id: row,
        unit,
        units: b.units.keys().cloned().collect(),
        choices: out,
        current: check_dto(b, d),
    })
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/rates.rs and the host's src/host/rates.rs
// call these)

/// Saves a rate table: a new one, or in place of `replacing` (which may
/// rename it; lines filled from it follow). One undo step.
pub fn rate_table_save(
    s: &mut Session,
    table: &RateTableDto,
    replacing: Option<&str>,
) -> EditResult {
    let t = table.table()?;
    let label = format!("Save rate table {}", t.name);
    s.edit(label, |b| {
        rates::save_table(b, t, replacing).map_err(|e| e.0)
    })
}

/// Deletes the rate table `name`; lines filled from it keep their figures.
/// One undo step.
pub fn rate_table_delete(s: &mut Session, name: &str) -> EditResult {
    s.edit(format!("Delete rate table {name}"), |b| {
        rates::delete_table(b, name).map(|_| ()).map_err(|e| e.0)
    })
}

/// What a fill wrote, for the page to say.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FilledDto {
    pub rate: String,
    pub unit: String,
    pub before: String,
    /// How the figure was reached, when converted.
    pub converted: Option<String>,
}

/// Fills line `row_id`'s rate from `classification` of `table` (per
/// `unit`, on `as_of`), as one undo step.
pub fn rate_fill(
    s: &mut Session,
    row_id: RowId,
    table: &str,
    classification: &str,
    unit: Option<&str>,
    as_of: Option<&str>,
) -> EditResult<FilledDto> {
    let label = match crate::history::row_place(s.budget(), row_id) {
        Some((code, n)) => format!("Fill rate on {code} line {n} from {table}"),
        None => format!("Fill rate from {table}"),
    };
    s.edit(label, |b| {
        let f = rates::fill(b, row_id, table, classification, unit, as_of).map_err(|e| e.0)?;
        Ok(FilledDto {
            rate: f.found.rate.to_string(),
            converted: f.found.conversion_words(&f.unit),
            unit: f.unit,
            before: f.before,
        })
    })
}

/// Updates lines `rows` (every line whose table now says something else,
/// when empty) to what their tables say now, as one undo step. Returns how
/// many changed; lines changed by hand since they were filled are left as
/// they are.
pub fn rate_update(s: &mut Session, rows: &[RowId]) -> EditResult<usize> {
    let changed: Vec<RowId> = rates::changed_lines(s.budget())
        .into_iter()
        .map(|(id, _, _)| id)
        .filter(|id| rows.is_empty() || rows.contains(id))
        .collect();
    if changed.is_empty() {
        return Err(if rows.len() == 1 {
            match details(s.budget()).find(|d| d.id == rows[0]).and_then(|d| rates::check(s.budget(), d)) {
                Some(RateCheck::Current) => "That line already has its table's rate.".to_string(),
                Some(RateCheck::Edited) => "That line's rate was changed by hand since it was filled: pick a classification again to fill it.".to_string(),
                Some(RateCheck::Missing(m)) => m,
                _ => "That line's rate wasn't filled from a rate table.".to_string(),
            }
        } else {
            "No line's table says anything different now.".to_string()
        });
    }
    let label = if changed.len() == 1 {
        "Update rate to the table".to_string()
    } else {
        format!("Update {} rates to their tables", changed.len())
    };
    s.edit(label, |b| {
        for id in &changed {
            rates::update(b, *id).map_err(|e| e.0)?;
        }
        Ok(changed.len())
    })
}

// ---------------------------------------------------------------------------
// A table from a spreadsheet

/// One sheet of a picked file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RateSheet {
    pub index: usize,
    pub name: String,
    pub rows: usize,
}

/// What reading a sheet as a rate table gives the page: the mapping (the
/// guess, or the page's own), the column titles, and the table or every
/// problem with its row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RateImportRead {
    pub token: u64,
    /// The file's name, never its path.
    pub file: String,
    pub sheets: Vec<RateSheet>,
    pub index: usize,
    pub mapping: RateMapping,
    pub titles: Vec<String>,
    /// The budget's units, for the unit columns.
    pub units: Vec<String>,
    /// The name the table gets (the sheet's or the file's when none given).
    pub name: String,
    /// Whether this budget already has a table of that name.
    pub exists: bool,
    pub table: Option<RateTableDto>,
    pub errors: Vec<RowError>,
}

fn stem(file: &str) -> String {
    let base = file.rsplit(['/', '\\']).next().unwrap_or(file);
    base.rsplit_once('.')
        .map_or(base, |(s, _)| s)
        .trim()
        .to_string()
}

/// Reads sheet `index` of `book` (from `file`, kept as `token`) for budget
/// `b`: under `mapping`, or the guess when `None`, named `name` (or after
/// the sheet or file when blank).
pub fn read_sheet(
    b: &Budget,
    token: u64,
    file: &str,
    book: &Book,
    index: usize,
    mapping: Option<RateMapping>,
    name: Option<&str>,
) -> EditResult<RateImportRead> {
    let visible: Vec<RateSheet> = book
        .tables
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.hidden)
        .map(|(i, t)| RateSheet {
            index: i,
            name: t.name.clone(),
            rows: t.rows.len(),
        })
        .collect();
    let t = book
        .tables
        .get(index)
        .ok_or("That sheet is no longer in the file.")?;
    let mapping = mapping.unwrap_or_else(|| sheet_rates::guess(t, &b.units));
    let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None if book.tables.len() > 1 => t.name.trim().to_string(),
        None => stem(file),
    };
    let (table, errors) = match sheet_rates::read(t, &mapping, &name) {
        Ok(t) => (Some(RateTableDto::from(&t)), Vec::new()),
        Err(e) => (None, e),
    };
    Ok(RateImportRead {
        token,
        file: file.to_string(),
        sheets: visible,
        index,
        titles: sheet_rates::titles(t, &mapping),
        mapping,
        units: b.units.keys().cloned().collect(),
        exists: b.rate_tables.iter().any(|x| x.name == name.trim()),
        name,
        table,
        errors,
    })
}

/// Reads a picked spreadsheet (`bytes` from a file named `file`), keeps it
/// and reads its first sheet as a rate table for budget `s`.
pub fn rate_import_pick(
    s: &Session,
    sources: &SheetSources,
    file: String,
    bytes: &[u8],
) -> EditResult<RateImportRead> {
    let book = read_book(&file, bytes)?;
    let first = book
        .tables
        .iter()
        .position(|t| !t.hidden)
        .ok_or("That file has no sheets to read.")?;
    let token = sources.keep(file.clone(), book);
    let (_, book) = sources.get(token)?;
    read_sheet(s.budget(), token, &file, &book, first, None, None)
}

/// Reads sheet `index` of the kept file `token` again, under the page's
/// `mapping` (the guess when `None`) and `name`.
pub fn rate_import_read(
    s: &Session,
    sources: &SheetSources,
    token: u64,
    index: usize,
    mapping: Option<RateMapping>,
    name: Option<&str>,
) -> EditResult<RateImportRead> {
    let (file, book) = sources.get(token)?;
    read_sheet(s.budget(), token, &file, &book, index, mapping, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every name and figure here is invented.
    fn table() -> RateTableDto {
        serde_json::from_value(serde_json::json!({
            "name": " Invented Crew ",
            "notes": "Made-up figures.",
            "rows": [
                {"classification": "Grip, Key", "rates": {"Hour": "50", "Week": ""}},
                {"classification": "Grip, Best Boy", "rates": {"Day": "410.50"},
                 "from": "2030-01-01", "to": ""},
                {"classification": "", "rates": {}}
            ]
        }))
        .unwrap()
    }

    fn first_line(b: &Budget) -> RowId {
        details(b).find(|d| d.kind == RowKind::Line).unwrap().id
    }

    #[test]
    fn the_page_sends_strings_and_blank_cells_are_left_out() {
        let t = table().table().unwrap();
        assert_eq!(t.name, "Invented Crew");
        assert_eq!(t.rows.len(), 2, "an empty line is dropped");
        assert_eq!(t.rows[0].rates.len(), 1);
        assert_eq!(t.rows[1].rates["Day"].to_string(), "410.5");
        assert_eq!(t.rows[1].to, None);
        let mut bad = table();
        bad.rows[0].rates.insert("Day".into(), "lots".into());
        assert!(bad.table().unwrap_err().contains("isn't a number"));
        assert_eq!(RateTableDto::from(&t).table().unwrap(), t);
    }

    #[test]
    fn saving_filling_and_updating_are_one_undo_step_each() {
        let mut s = Session::new();
        let id = first_line(s.budget());
        let steps = |s: &Session| s.history().steps.len();
        rate_table_save(&mut s, &table(), None).unwrap();
        assert_eq!(steps(&s), 1);
        let c = choices(s.budget(), id, Some("Day")).unwrap();
        assert_eq!(c.choices.len(), 2);
        assert_eq!(
            c.choices[0].rate.as_deref(),
            Some("600"),
            "converted from Hour"
        );
        assert!(c.choices[0].converted);
        let before = s.budget().clone();
        let f = rate_fill(&mut s, id, "Invented Crew", "Grip, Key", Some("Day"), None).unwrap();
        assert_eq!(f.rate, "600");
        assert!(f.converted.unwrap().contains("50 per Hour"));
        assert_eq!(steps(&s), 2);
        s.undo();
        assert_eq!(s.budget(), &before);
        s.redo();
        // The table changes: the line keeps its figure until updated.
        let mut t = table();
        t.rows[0].rates.insert("Hour".into(), "55".into());
        rate_table_save(&mut s, &t, Some("Invented Crew")).unwrap();
        let line_rate = |s: &Session| {
            details(s.budget())
                .find(|d| d.id == id)
                .unwrap()
                .rate
                .0
                .clone()
        };
        assert_eq!(line_rate(&s), "600");
        let ck = checks(s.budget(), &[id]);
        assert_eq!(ck[&id].state, "changed");
        assert_eq!(ck[&id].now.as_deref(), Some("660"));
        assert_eq!(rate_update(&mut s, &[]).unwrap(), 1);
        assert_eq!(line_rate(&s), "660");
        assert_eq!(checks(s.budget(), &[id])[&id].state, "current");
        assert!(rate_update(&mut s, &[id]).unwrap_err().contains("already"));
        rate_table_delete(&mut s, "Invented Crew").unwrap();
        assert_eq!(checks(s.budget(), &[id])[&id].state, "missing");
        assert_eq!(line_rate(&s), "660", "deleting a table changes no figure");
    }

    #[test]
    fn a_spreadsheet_is_read_into_a_table() {
        let mut s = Session::new();
        let sources = SheetSources::default();
        let csv = "Classification,Daily,Weekly\nLead,500,2400\nHelper,,1500\n";
        let r =
            rate_import_pick(&s, &sources, "Invented rates.csv".into(), csv.as_bytes()).unwrap();
        assert_eq!(r.name, "Invented rates");
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let t = r.table.clone().unwrap();
        assert_eq!(t.rows.len(), 2);
        rate_table_save(&mut s, &t, None).unwrap();
        // Read again with another mapping and name: problems with rows.
        let mut m = r.mapping.clone();
        m.units.remove("Day");
        m.units.insert("Day".into(), 0);
        let again = rate_import_read(&s, &sources, r.token, 0, Some(m), Some("Other")).unwrap();
        assert!(!again.errors.is_empty());
        assert!(!again.exists);
        let same = rate_import_read(&s, &sources, r.token, 0, None, None).unwrap();
        assert!(same.exists);
    }
}
