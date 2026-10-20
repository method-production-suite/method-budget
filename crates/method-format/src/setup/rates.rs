//! A rate table from a spreadsheet the user has (decisions.md, entry 90):
//! one sheet, mapped column by column as the spreadsheet import maps lines
//! (titles guessed, every choice the user's), into one named
//! [`RateTable`].
//!
//! Two layouts read the same way, and may be mixed:
//! - **A column per unit**: `Classification | Hourly | Daily | Weekly`,
//!   each unit column mapped to one of the budget's units;
//! - **A unit and a rate column**: `Classification | Unit | Rate`, a sheet
//!   row per unit.
//!
//! Optional `From`, `To` and `Notes` columns give each row its dates and
//! notes. Rows of one classification and period join into one table row.
//! Every row is checked; problems come back with their row numbers and
//! nothing is read while any is left.

use std::collections::{BTreeMap, BTreeSet};

use method_core::rates::{classification_key, valid_table_name, RateRow, RateTable};
use method_core::{Dec, UnitDef};
use serde::{Deserialize, Serialize};

use super::sheet::RowError;
use crate::import::actuals::{cell_date_in, DateOrder};
use crate::import::mapping::{norm, parse_decimal, parse_number_text};
use crate::import::{Table, Value};

/// Which row holds the titles and which column holds what, as the page
/// shows and sends it. Columns are 0-based.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RateMapping {
    /// The row of titles; data starts after it. `None`: no titles.
    pub header_row: Option<usize>,
    pub classification: Option<usize>,
    pub from: Option<usize>,
    pub to: Option<usize>,
    pub notes: Option<usize>,
    /// A column naming each row's unit, with [`RateMapping::rate`].
    pub unit: Option<usize>,
    /// The column of the rate per that unit.
    pub rate: Option<usize>,
    /// A column of rates per unit: unit name to column.
    pub units: BTreeMap<String, usize>,
}

const CLASSIFICATION: &[&str] = &[
    "classification",
    "class",
    "position",
    "jobtitle",
    "job",
    "title",
    "role",
    "occupation",
    "craft",
];
const FROM: &[&str] = &[
    "from",
    "effectivefrom",
    "effective",
    "start",
    "startdate",
    "begins",
];
const TO: &[&str] = &[
    "to",
    "effectiveto",
    "end",
    "enddate",
    "until",
    "through",
    "expires",
];
const NOTES: &[&str] = &["notes", "note", "comments", "comment", "remarks"];
const UNIT: &[&str] = &["unit", "per", "units", "basis"];
const RATE: &[&str] = &["rate", "amount", "wage", "scale", "figure"];

/// Whether column title `t` (normalized) names unit `u` of the budget.
fn names_unit(t: &str, u: &str, def: &UnitDef) -> bool {
    let (n, p) = (norm(u), norm(&def.plural));
    if t.is_empty() || n.is_empty() {
        return false;
    }
    let adjective = match n.as_str() {
        "hour" => Some("hourly"),
        "day" => Some("daily"),
        "week" => Some("weekly"),
        "month" => Some("monthly"),
        _ => None,
    };
    t == n
        || (!p.is_empty() && t == p)
        || t == format!("{n}rate")
        || t == format!("per{n}")
        || t == format!("rateper{n}")
        || adjective.is_some_and(|a| t == a || t == format!("{a}rate"))
}

/// The guessed mapping of table `t` for a budget with `units`: the row
/// among the first thirty whose titles name a classification column and
/// the most other columns, each column used once.
pub fn guess(t: &Table, units: &BTreeMap<String, UnitDef>) -> RateMapping {
    let mut best: Option<(usize, RateMapping)> = None;
    for r in 0..t.rows.len().min(30) {
        let titles: Vec<String> = t.rows[r].iter().map(|v| norm(v.text())).collect();
        let mut used: BTreeSet<usize> = BTreeSet::new();
        let find = |names: &[&str], used: &mut BTreeSet<usize>| -> Option<usize> {
            let c = names.iter().find_map(|n| {
                titles
                    .iter()
                    .enumerate()
                    .position(|(i, t)| !used.contains(&i) && t == n)
            })?;
            used.insert(c);
            Some(c)
        };
        let mut m = RateMapping {
            header_row: Some(r),
            ..RateMapping::default()
        };
        m.classification = find(CLASSIFICATION, &mut used);
        if m.classification.is_none() {
            continue;
        }
        for (i, t) in titles.iter().enumerate() {
            if used.contains(&i) {
                continue;
            }
            if let Some((u, _)) = units
                .iter()
                .find(|(u, d)| names_unit(t, u, d) && !m.units.contains_key(*u))
            {
                m.units.insert(u.clone(), i);
                used.insert(i);
            }
        }
        m.from = find(FROM, &mut used);
        m.to = find(TO, &mut used);
        m.notes = find(NOTES, &mut used);
        m.unit = find(UNIT, &mut used);
        m.rate = find(RATE, &mut used);
        if m.unit.is_none() || m.rate.is_none() {
            m.unit = None;
            m.rate = None;
        }
        let score = used.len();
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, m));
        }
    }
    best.map(|(_, m)| m).unwrap_or_default()
}

fn number(t: &Table, r: usize, c: usize) -> Result<Option<Dec>, String> {
    match t.cell(r, c) {
        Value::Number(n) => parse_decimal(n).map(Some),
        v => parse_number_text(v.text(), t.decimal_comma),
    }
    .map_err(|_| {
        format!(
            "\u{201c}{}\u{201d} isn't a number",
            t.cell(r, c).text().trim()
        )
    })
}

/// Reads table `t` under mapping `m` as a rate table named `name`: the
/// table, or every problem with its row.
pub fn read(t: &Table, m: &RateMapping, name: &str) -> Result<RateTable, Vec<RowError>> {
    let sheet = t.name.clone();
    let at_header = m.header_row.map_or(1, |r| r + 1);
    let problem = |row: usize, message: String| RowError {
        sheet: sheet.clone(),
        row,
        message,
    };
    let name = valid_table_name(name).map_err(|e| vec![problem(at_header, e.0)])?;
    let Some(class_col) = m.classification else {
        return Err(vec![problem(
            at_header,
            "Pick the column that holds the classifications.".into(),
        )]);
    };
    if m.units.is_empty() && (m.unit.is_none() || m.rate.is_none()) {
        return Err(vec![problem(
            at_header,
            "Pick a column for each unit's rate, or a Unit column and a Rate column.".into(),
        )]);
    }
    let mapped: Vec<usize> = [Some(class_col), m.from, m.to, m.notes, m.unit, m.rate]
        .into_iter()
        .flatten()
        .chain(m.units.values().copied())
        .collect();
    let mut errors = Vec::new();
    let mut table = RateTable {
        name,
        notes: String::new(),
        rows: Vec::new(),
    };
    for r in m.header_row.map_or(0, |h| h + 1)..t.rows.len() {
        if mapped.iter().all(|&c| t.cell(r, c).is_empty()) {
            continue;
        }
        let at = r + 1;
        let result = (|| -> Result<(), String> {
            let class = t.cell(r, class_col).text().trim().to_string();
            if class.is_empty() {
                return Err("no classification".into());
            }
            let date = |c: Option<usize>, what: &str| -> Result<Option<String>, String> {
                let Some(c) = c else { return Ok(None) };
                cell_date_in(t.cell(r, c), DateOrder::default(), t.date1904).map_err(|_| {
                    format!(
                        "{what} \u{201c}{}\u{201d} isn't a date",
                        t.cell(r, c).text().trim()
                    )
                })
            };
            let (from, to) = (date(m.from, "the From date")?, date(m.to, "the To date")?);
            let mut rates: BTreeMap<String, Dec> = BTreeMap::new();
            for (u, &c) in &m.units {
                if let Some(v) =
                    number(t, r, c).map_err(|e| format!("{class}: the rate per {u}: {e}"))?
                {
                    rates.insert(u.clone(), v);
                }
            }
            if let (Some(uc), Some(rc)) = (m.unit, m.rate) {
                let u = t.cell(r, uc).text().trim().to_string();
                let v = number(t, r, rc).map_err(|e| format!("{class}: the rate: {e}"))?;
                match (u.is_empty(), v) {
                    (true, None) => {}
                    (true, Some(_)) => return Err(format!("{class}: the rate needs its unit")),
                    (false, None) => return Err(format!("{class}: the rate per {u} is missing")),
                    (false, Some(v)) => {
                        if rates.insert(u.clone(), v).is_some() {
                            return Err(format!("{class} has two rates per {u}"));
                        }
                    }
                }
            }
            if rates.is_empty() {
                return Err(format!("{class} has no rate"));
            }
            let notes = m
                .notes
                .map(|c| t.cell(r, c).text().trim().to_string())
                .unwrap_or_default();
            let key = classification_key(&class);
            let same = |x: &RateRow| {
                classification_key(&x.classification) == key && x.from == from && x.to == to
            };
            match table.rows.iter().position(same) {
                Some(j) => {
                    let row = &mut table.rows[j];
                    for (u, v) in rates {
                        if row.rates.insert(u.clone(), v).is_some() {
                            return Err(format!(
                                "{class} has two rates per {u} for the same dates"
                            ));
                        }
                    }
                    if !notes.is_empty() && row.notes != notes {
                        if !row.notes.is_empty() {
                            row.notes.push_str("; ");
                        }
                        row.notes.push_str(&notes);
                    }
                }
                None => table.rows.push(RateRow {
                    classification: class,
                    rates,
                    from,
                    to,
                    notes,
                }),
            }
            Ok(())
        })();
        if let Err(message) = result {
            errors.push(problem(at, message));
        }
    }
    if errors.is_empty() {
        if table.rows.is_empty() {
            errors.push(problem(
                at_header,
                "The sheet has no rates under those titles.".into(),
            ));
        } else if let Err(e) = table.validate() {
            errors.push(problem(at_header, e.0));
        }
    }
    if errors.is_empty() {
        Ok(table)
    } else {
        Err(errors)
    }
}

/// The column titles of table `t` under `m` (or "Column A" ... without).
pub fn titles(t: &Table, m: &RateMapping) -> Vec<String> {
    super::sheet::titles(
        t,
        &super::sheet::SheetMapping {
            header_row: m.header_row,
            columns: BTreeMap::new(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::read_file;

    fn table(text: &str) -> Table {
        read_file(text.as_bytes(), "Invented rates.csv")
            .unwrap()
            .tables
            .remove(0)
    }

    fn d(s: &str) -> Dec {
        s.parse().unwrap()
    }

    #[test]
    fn a_column_per_unit_is_guessed_and_read() {
        // Made-up classifications and figures.
        let t = table(
            "Invented rate card\n\
             \n\
             Position,Hourly,Daily,Weekly,Effective From,Notes\n\
             \"Grip, Key\",50,\"$600.00\",,2030-01-01,Made up\n\
             \"Grip, Best Boy\",,410.5,2000,,\n\
             ,,,,,\n",
        );
        let units = method_core::default_units();
        let m = guess(&t, &units);
        assert_eq!(m.header_row, Some(2));
        assert_eq!(m.classification, Some(0));
        assert_eq!(m.units.get("Hour"), Some(&1));
        assert_eq!(m.units.get("Day"), Some(&2));
        assert_eq!(m.units.get("Week"), Some(&3));
        assert_eq!((m.from, m.notes, m.unit), (Some(4), Some(5), None));
        let rt = read(&t, &m, " Invented Card ").unwrap();
        assert_eq!(rt.name, "Invented Card");
        assert_eq!(rt.rows.len(), 2);
        assert_eq!(rt.rows[0].rates["Day"], d("600"));
        assert_eq!(rt.rows[0].from.as_deref(), Some("2030-01-01"));
        assert_eq!(rt.rows[0].notes, "Made up");
        assert_eq!(rt.rows[1].rates.len(), 2);
        assert_eq!(titles(&t, &m)[1], "Hourly");
    }

    #[test]
    fn a_unit_and_a_rate_column_join_rows() {
        let t = table(
            "Classification,Unit,Rate\n\
             Lead,Day,500\n\
             Lead,Week,2400\n\
             Helper,Hour,20\n",
        );
        let m = guess(&t, &method_core::default_units());
        assert_eq!((m.unit, m.rate), (Some(1), Some(2)));
        let rt = read(&t, &m, "Made Up").unwrap();
        assert_eq!(rt.rows.len(), 2);
        assert_eq!(rt.rows[0].rates.len(), 2);
    }

    #[test]
    fn problems_come_back_with_their_rows() {
        let t = table(
            "Classification,Day,From\n\
             Lead,500,\n\
             ,1,\n\
             Helper,abc,\n\
             Extra,,\n\
             Late,1,someday\n",
        );
        let m = guess(&t, &method_core::default_units());
        let errs = read(&t, &m, "Made Up").unwrap_err();
        let rows: Vec<usize> = errs.iter().map(|e| e.row).collect();
        assert_eq!(rows, [3, 4, 5, 6], "{errs:?}");
        // Overlapping periods are refused once the rows are read.
        let t = table(
            "Classification,Day,From\n\
             Lead,500,2030-01-01\n\
             Lead,510,2030-06-01\n",
        );
        let errs = read(&t, &guess(&t, &method_core::default_units()), "Made Up").unwrap_err();
        assert!(errs[0].message.contains("same days"), "{errs:?}");
        // No name, no classification column, no rate column.
        assert!(read(&t, &RateMapping::default(), "").is_err());
        let mut m = guess(&t, &method_core::default_units());
        m.units.clear();
        assert!(read(&t, &m, "Made Up").unwrap_err()[0]
            .message
            .contains("unit"));
    }
}
