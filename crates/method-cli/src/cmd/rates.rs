//! `method-budget rates`: the rate tables a budget keeps (roadmap 3.9;
//! docs/CLI.md, "Rate tables"; decisions.md, entry 90). `list` and `show`
//! print them, `check` says how each line filled from one stands against
//! it now, `fill` fills lines' rates from a classification, `update` brings
//! filled lines up to what their tables say now, `import` reads a table
//! from a spreadsheet with a column mapping and `delete` removes one. Every
//! change goes through the edit scripts' `run`, so a shared budget records
//! it in its change log and `--dry-run` says what would change.

use std::fmt::Write as _;

use method_core::rates::{self, RateCheck};
use method_format::import::Table;
use method_format::serde_json::{json, Value};
use method_format::setup::rates::{self as sheet_rates, RateMapping};

use crate::cli::{load, CliError, Flags, Outcome};
use crate::script::{self, Op, RowRef};

pub const USAGE: &str = "  method-budget rates list <file.mthd> [--json]
      Print each rate table: name, rows, classifications and units.
  method-budget rates show <file.mthd> <TABLE> [--json]
      Print a table's rows: classification, unit, rate, from, to, notes.
  method-budget rates check <file.mthd> [--json]
      Print every line filled from a rate table and how it stands:
      current, changed (the table now says something else; the line keeps
      its figure), edited (changed by hand since) or missing.
  method-budget rates fill <file.mthd> <ROW> <TABLE> <CLASSIFICATION>
        [--unit UNIT] [--as-of YYYY-MM-DD] [--dry-run]
      Fill the rate of ROW (#ID, ACCOUNT:N, or ACCOUNT:* for every line of
      an account) from a classification, once, as a number; the line
      remembers where it came from. The unit is the line's own unless
      --unit says; --as-of picks the row in effect on that date.
  method-budget rates update <file.mthd> [ROW] [--dry-run]
      Give lines filled from a rate table (every one, or ROW's) what their
      table says now. Lines changed by hand since are left as they are.
  method-budget rates import <file.mthd> <sheet.xlsx|csv> [--name NAME]
        [--sheet N|NAME] [--header-row N] [--column FIELD=COLUMN]...
        [--replace] [--dry-run]
      Read a rate table from a spreadsheet: a classification column, then a
      column per unit (`--column unit:Day=C`) or a unit and a rate column.
      Columns are guessed from their titles; FIELD is classification, from,
      to, notes, unit, rate or unit:NAME, COLUMN a letter, a number from 1
      or a title, `none` to leave it out. --replace replaces a table of the
      same name.
  method-budget rates delete <file.mthd> <TABLE> [--dry-run]
      Delete a rate table. Lines filled from it keep their rates.
";

const VALUES: &[&str] = &[
    "--unit",
    "--as-of",
    "--name",
    "--sheet",
    "--header-row",
    "--column",
];
const SWITCHES: &[&str] = &["--json", "--dry-run", "--replace"];

pub fn run(args: &[String]) -> Outcome {
    let Some((action, rest)) = args.split_first() else {
        return Err(CliError::usage(
            "rates needs list, show, check, fill, update, import or delete",
            USAGE,
        ));
    };
    let f = Flags::parse(rest, VALUES, SWITCHES, USAGE)?;
    match action.as_str() {
        "list" => list(&f),
        "show" => show(&f),
        "check" => check(&f),
        "fill" => fill(&f),
        "update" => update(&f),
        "import" => import(&f),
        "delete" => delete(&f),
        other => Err(CliError::usage(
            format!(
                "unknown rates action `{other}`: use list, show, check, fill, update, import or delete"
            ),
            USAGE,
        )),
    }
}

fn json_out(v: &Value) -> Outcome {
    let mut s = method_format::serde_json::to_string_pretty(v)
        .map_err(|e| CliError::failed(e.to_string()))?;
    s.push('\n');
    Ok(s)
}

/// A table as JSON: figures as strings.
pub fn table_json(t: &rates::RateTable) -> Value {
    json!({
        "name": t.name,
        "notes": t.notes,
        "rows": t.rows.iter().map(|r| json!({
            "classification": r.classification,
            "rates": r.rates.iter().map(|(u, v)| (u.clone(), json!(v.to_string()))).collect::<method_format::serde_json::Map<_, _>>(),
            "from": r.from,
            "to": r.to,
            "notes": r.notes,
        })).collect::<Vec<_>>(),
    })
}

fn list(f: &Flags) -> Outcome {
    let path = &f.exactly(1, "the budget file", USAGE)?[0];
    let b = load(path)?;
    if f.has("--json") {
        let v: Vec<Value> = b
            .rate_tables
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "rows": t.rows.len(),
                    "classifications": classes(t),
                    "units": t.units(),
                    "notes": t.notes,
                })
            })
            .collect();
        return json_out(&json!(v));
    }
    if b.rate_tables.is_empty() {
        return Ok("no rate tables\n".into());
    }
    let mut out = String::new();
    for t in &b.rate_tables {
        let _ = writeln!(
            out,
            "{}\t{} rows\t{} classifications\t{}",
            t.name,
            t.rows.len(),
            classes(t),
            t.units().join(", ")
        );
    }
    Ok(out)
}

fn classes(t: &rates::RateTable) -> usize {
    t.rows
        .iter()
        .map(|r| rates::classification_key(&r.classification))
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

fn show(f: &Flags) -> Outcome {
    let p = f.exactly(2, "the budget file and a table name", USAGE)?;
    let b = load(&p[0])?;
    let t = rates::table(&b, &p[1])
        .ok_or_else(|| CliError::failed(format!("{}: there is no rate table {}", p[0], p[1])))?;
    if f.has("--json") {
        return json_out(&table_json(t));
    }
    let mut out = String::new();
    if !t.notes.is_empty() {
        let _ = writeln!(out, "notes\t{}", t.notes.replace('\n', " "));
    }
    for r in &t.rows {
        for (u, v) in &r.rates {
            let _ = writeln!(
                out,
                "{}\t{u}\t{v}\t{}\t{}\t{}",
                r.classification,
                r.from.as_deref().unwrap_or(""),
                r.to.as_deref().unwrap_or(""),
                r.notes.replace('\n', " ")
            );
        }
    }
    Ok(out)
}

fn check(f: &Flags) -> Outcome {
    let path = &f.exactly(1, "the budget file", USAGE)?[0];
    let b = load(path)?;
    let mut rows = Vec::new();
    for a in b.categories().flat_map(|c| &c.accounts) {
        for (i, d) in a.details.iter().enumerate() {
            let (Some(s), Some(c)) = (&d.rate_source, rates::check(&b, d)) else {
                continue;
            };
            let (now, message) = match &c {
                RateCheck::Changed { now } => (Some(now.to_string()), None),
                RateCheck::Missing(m) => (None, Some(m.clone())),
                _ => (None, None),
            };
            rows.push(json!({
                "row": format!("{}:{}", a.code, i + 1),
                "id": d.id,
                "state": c.key(),
                "table": s.table,
                "classification": s.classification,
                "unit": s.unit,
                "as_of": s.as_of,
                "rate": d.rate.0,
                "filled": s.rate.to_string(),
                "now": now,
                "message": message,
            }));
        }
    }
    if f.has("--json") {
        return json_out(&json!(rows));
    }
    if rows.is_empty() {
        return Ok("no line was filled from a rate table\n".into());
    }
    let mut out = String::new();
    for r in &rows {
        let s = |k: &str| r[k].as_str().unwrap_or("").to_string();
        let extra = match s("state").as_str() {
            "changed" => format!("\tthe table now says {}", s("now")),
            "missing" => format!("\t{}", s("message")),
            _ => String::new(),
        };
        let _ = writeln!(
            out,
            "{}\t{}\t{} / {} per {}\t{}{extra}",
            s("row"),
            s("state"),
            s("table"),
            s("classification"),
            s("unit"),
            s("rate")
        );
    }
    Ok(out)
}

fn fill(f: &Flags) -> Outcome {
    let p = f.exactly(
        4,
        "the budget file, a row, a table and a classification",
        USAGE,
    )?;
    let rows = RowRef::parse(&p[1]).map_err(|e| CliError::usage(e, USAGE))?;
    let op = Op::RateFill {
        rows,
        table: p[2].clone(),
        classification: p[3].clone(),
        unit: f.value("--unit").map(str::to_string),
        as_of: f.value("--as-of").map(str::to_string),
    };
    script::run(
        &p[0],
        &[(format!("rates fill {}", p[1]), op)],
        f.has("--dry-run"),
    )
}

fn update(f: &Flags) -> Outcome {
    let p = f.positional();
    let (path, row) = match p {
        [path] => (path, None),
        [path, row] => (
            path,
            Some(RowRef::parse(row).map_err(|e| CliError::usage(e, USAGE))?),
        ),
        [] => return Err(CliError::usage("missing the budget file", USAGE)),
        _ => {
            return Err(CliError::usage(
                format!("unexpected argument `{}`", p[2]),
                USAGE,
            ))
        }
    };
    script::run(
        path,
        &[("rates update".into(), Op::RateUpdate(row))],
        f.has("--dry-run"),
    )
}

fn delete(f: &Flags) -> Outcome {
    let p = f.exactly(2, "the budget file and a table name", USAGE)?;
    script::run(
        &p[0],
        &[(
            format!("rates delete {}", p[1]),
            Op::RateTableDelete(p[1].clone()),
        )],
        f.has("--dry-run"),
    )
}

/// Column `spec` of table `t` (whose titles are on `header`): a letter, a
/// number from 1, or a title; `None` for `none`.
fn column(t: &Table, header: Option<usize>, spec: &str) -> Result<Option<usize>, String> {
    let s = spec.trim();
    if s.eq_ignore_ascii_case("none") || s.is_empty() {
        return Ok(None);
    }
    if let Ok(n) = s.parse::<usize>() {
        return if n >= 1 {
            Ok(Some(n - 1))
        } else {
            Err(format!("column `{s}`: numbers start at 1"))
        };
    }
    if s.len() <= 3 && s.chars().all(|c| c.is_ascii_alphabetic()) {
        let mut n = 0usize;
        for c in s.to_ascii_uppercase().bytes() {
            n = n * 26 + usize::from(c - b'A' + 1);
        }
        // A short word could be a title too ("Day"): a title wins.
        if let Some(i) = header.and_then(|h| title_column(t, h, s)) {
            return Ok(Some(i));
        }
        return Ok(Some(n - 1));
    }
    header
        .and_then(|h| title_column(t, h, s))
        .map(Some)
        .ok_or_else(|| format!("no column is titled `{s}`"))
}

fn title_column(t: &Table, header: usize, title: &str) -> Option<usize> {
    let want = method_format::import::mapping::norm(title);
    (0..t.width()).find(|&c| method_format::import::mapping::norm(t.cell(header, c).text()) == want)
}

fn import(f: &Flags) -> Outcome {
    let p = f.exactly(2, "the budget file and the spreadsheet", USAGE)?;
    let (path, sheet_path) = (&p[0], &p[1]);
    let b = load(path)?;
    let bytes = method_format::fsio::read_capped(
        std::path::Path::new(sheet_path),
        method_format::fsio::MAX_READ_BYTES,
    )
    .map_err(|e| CliError::failed(format!("{sheet_path}: {e}")))?;
    let file = std::path::Path::new(sheet_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let book = method_format::import::read_file(&bytes, &file)
        .map_err(|e| CliError::failed(format!("{sheet_path}: {e}")))?;
    let index = match f.value("--sheet") {
        None => book.tables.iter().position(|t| !t.hidden).unwrap_or(0),
        Some(s) => match s.parse::<usize>() {
            Ok(n) if n >= 1 && n <= book.tables.len() => n - 1,
            _ => book
                .tables
                .iter()
                .position(|t| t.name == s)
                .ok_or_else(|| CliError::rejected(format!("{sheet_path}: no sheet {s}")))?,
        },
    };
    let t = book
        .tables
        .get(index)
        .ok_or_else(|| CliError::rejected(format!("{sheet_path}: the file has no sheets")))?;
    let mut m: RateMapping = sheet_rates::guess(t, &b.units);
    if let Some(h) = f.value("--header-row") {
        let n: usize = h
            .parse()
            .map_err(|_| CliError::usage(format!("`{h}` is not a row number"), USAGE))?;
        m.header_row = n.checked_sub(1);
    }
    for spec in f.values("--column") {
        let (field, col) = spec
            .split_once('=')
            .ok_or_else(|| CliError::usage(format!("`{spec}`: write FIELD=COLUMN"), USAGE))?;
        let col = column(t, m.header_row, col).map_err(|e| CliError::usage(e, USAGE))?;
        match field.trim() {
            "classification" => m.classification = col,
            "from" => m.from = col,
            "to" => m.to = col,
            "notes" => m.notes = col,
            "unit" => m.unit = col,
            "rate" => m.rate = col,
            other => match other.strip_prefix("unit:") {
                Some(u) if b.units.contains_key(u) => {
                    match col {
                        Some(c) => m.units.insert(u.to_string(), c),
                        None => m.units.remove(u),
                    };
                }
                Some(u) => {
                    return Err(CliError::rejected(format!(
                        "{path}: the budget has no unit {u}"
                    )))
                }
                None => {
                    return Err(CliError::usage(
                        format!("`{other}` is not a field: classification, from, to, notes, unit, rate or unit:NAME"),
                        USAGE,
                    ))
                }
            },
        }
    }
    let name = match f.value("--name") {
        Some(n) => n.to_string(),
        None if book.tables.len() > 1 => t.name.clone(),
        None => file
            .rsplit_once('.')
            .map_or(file.as_str(), |(s, _)| s)
            .to_string(),
    };
    let table = sheet_rates::read(t, &m, &name).map_err(|errors| {
        let mut msg = format!("{sheet_path}: nothing was imported:");
        for e in errors {
            let _ = write!(msg, "\n  {} row {}: {}", e.sheet, e.row, e.message);
        }
        CliError::rejected(msg)
    })?;
    script::run(
        path,
        &[(
            format!("rates import {sheet_path}"),
            Op::RateTable(table, f.has("--replace")),
        )],
        f.has("--dry-run"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_are_letters_numbers_or_titles() {
        let t = method_format::import::read_file(b"Position,Day,Week\nLead,1,2\n", "x.csv")
            .unwrap()
            .tables
            .remove(0);
        assert_eq!(column(&t, Some(0), "B"), Ok(Some(1)));
        assert_eq!(column(&t, Some(0), "3"), Ok(Some(2)));
        assert_eq!(column(&t, Some(0), "Day"), Ok(Some(1)), "a title wins");
        assert_eq!(column(&t, Some(0), "position"), Ok(Some(0)));
        assert_eq!(column(&t, Some(0), "none"), Ok(None));
        assert!(column(&t, Some(0), "0").is_err());
        assert!(column(&t, Some(0), "Not A Title").is_err());
    }
}
