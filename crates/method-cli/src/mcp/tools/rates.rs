//! Rate tables the user keeps (roadmap 3.9; decisions.md, entry 90): list
//! and read them, keep one (from JSON or a spreadsheet), fill lines' rates
//! from a classification, and bring filled lines up to what their tables
//! say now. Filling writes a number once and records where it came from;
//! a table changed later never changes a line on its own.

use std::collections::BTreeMap;

use method_core::rates::{self, RateCheck, RateRow, RateTable};
use method_core::{Budget, Dec, RowId, RowKind};
use method_format::serde_json::{json, Value};
use method_format::setup::rates::{self as sheet_rates, RateMapping};

use super::{budget_path, edited, obj, path_prop, Args, Ctx, Tool, ToolError, ToolOut, ToolResult};
use crate::cmd::rates::table_json;
use crate::mcp::paths;
use crate::mcp::schema::{self as s, object};

fn date() -> Value {
    json!({"type": "string", "description": "A date written YYYY-MM-DD.", "minLength": 10, "maxLength": 10})
}

fn column() -> Value {
    json!({"type": "integer", "description": "A column, numbered from 1.", "minimum": 1, "maximum": 16384})
}

fn table_schema() -> Value {
    let rate = object(
        vec![
            ("unit", s::name("A unit of the budget (Hour, Day, Week).")),
            (
                "rate",
                json!({"type": "string", "description": "The rate per one of that unit, as a decimal (\"410.50\").", "minLength": 1, "maxLength": 40}),
            ),
        ],
        &["unit", "rate"],
    );
    let row = object(
        vec![
            (
                "classification",
                s::name("The classification as you keep it (\"Grip, Key\")."),
            ),
            (
                "rates",
                s::array(
                    "Its rate per unit, at least one.",
                    rate,
                    rates::MAX_ROW_UNITS,
                ),
            ),
            ("from", s::nullable(date())),
            ("to", s::nullable(date())),
            ("notes", s::text("Notes on the row.")),
        ],
        &["classification", "rates"],
    );
    object(
        vec![
            ("name", s::name("The table's name.")),
            (
                "notes",
                s::text("Where the figures came from, what they cover."),
            ),
            ("rows", s::array("The rows.", row, rates::MAX_RATE_ROWS)),
        ],
        &["name", "rows"],
    )
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "list_rate_tables",
            title: "List rate tables",
            description: "The rate tables the budget keeps (each with its rows, classifications \
                and units), and every line filled from one with how it stands now: current, \
                changed (the table now says something else; the line keeps its figure until \
                update_rates), edited (changed by hand since) or missing. Rates are the user's \
                own; nothing comes with the app.",
            mutating: false,
            destructive: false,
            schema: || object(vec![path_prop()], &["path"]),
            run: list_rate_tables,
        },
        Tool {
            name: "get_rate_table",
            title: "Read a rate table",
            description: "One rate table in full: its notes and rows (classification, rate per \
                unit as decimal strings, from and to dates, notes).",
            mutating: false,
            destructive: false,
            schema: || {
                object(
                    vec![path_prop(), ("name", s::name("The table's name."))],
                    &["path", "name"],
                )
            },
            run: get_rate_table,
        },
        Tool {
            name: "set_rate_table",
            title: "Keep a rate table",
            description: "Adds a rate table, or replaces the one named replacing (which may \
                rename it; lines filled from it follow). Two rows of one classification can't be \
                in effect on the same day; a row with no dates is in effect every day.",
            mutating: true,
            destructive: true,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("table", table_schema()),
                        (
                            "replacing",
                            s::nullable(s::name("The table it replaces, if any.")),
                        ),
                    ],
                    &["path", "table"],
                )
            },
            run: set_rate_table,
        },
        Tool {
            name: "delete_rate_table",
            title: "Delete a rate table",
            description: "Deletes a rate table. Lines filled from it keep their rates (they say \
                the table is gone).",
            mutating: true,
            destructive: true,
            schema: || {
                object(
                    vec![path_prop(), ("name", s::name("The table's name."))],
                    &["path", "name"],
                )
            },
            run: delete_rate_table,
        },
        Tool {
            name: "import_rate_table",
            title: "Import a rate table from a spreadsheet",
            description: "Reads a rate table from a spreadsheet (.xlsx, .csv, tab-delimited \
                .txt/.tsv) into the budget: a classification column, then a column per unit \
                (columns.units {\"Day\": 3}) or a unit column and a rate column. Columns are \
                guessed from their titles; give columns to change the guess. Problems come back \
                with their rows and nothing is written while any is left. replace replaces a \
                table of the same name.",
            mutating: true,
            destructive: true,
            schema: || {
                let unit_col = object(
                    vec![
                        ("unit", s::name("A unit of the budget.")),
                        ("column", column()),
                    ],
                    &["unit", "column"],
                );
                let columns = object(
                    vec![
                        (
                            "header_row",
                            s::nullable(s::integer("The row of titles, from 1.", 1, 1_000_000)),
                        ),
                        ("classification", s::nullable(column())),
                        ("from", s::nullable(column())),
                        ("to", s::nullable(column())),
                        ("notes", s::nullable(column())),
                        ("unit", s::nullable(column())),
                        ("rate", s::nullable(column())),
                        (
                            "unit_columns",
                            s::array("A column of rates per unit.", unit_col, 64),
                        ),
                    ],
                    &[],
                );
                object(
                    vec![
                        path_prop(),
                        ("sheet", s::path("The spreadsheet file.")),
                        (
                            "sheet_index",
                            s::integer(
                                "Which sheet, from 1 (the first shown when left out).",
                                1,
                                10_000,
                            ),
                        ),
                        (
                            "name",
                            s::name("The table's name (the sheet's or file's when left out)."),
                        ),
                        ("columns", columns),
                        ("replace", s::boolean("Replace a table of the same name.")),
                        (
                            "dry_run",
                            s::boolean("Only read the sheet and say what it holds."),
                        ),
                    ],
                    &["path", "sheet"],
                )
            },
            run: import_rate_table,
        },
        Tool {
            name: "fill_rate",
            title: "Fill a line's rate from a rate table",
            description: "Fills the rates of lines (by id) from a classification of one of the \
                budget's rate tables: the figure for the line's unit (or unit), written once as a \
                number, with where it came from. A unit the table has no figure for is converted \
                from the closest unit with hours, only when both units have hours in the budget, \
                rounded to cents. as_of picks the row in effect on a date. The rate is not a \
                live link: a changed table offers update_rates and changes nothing itself.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        (
                            "ids",
                            s::array(
                                "The lines (detail row ids).",
                                s::integer("A row id.", 0, i64::MAX),
                                1000,
                            ),
                        ),
                        ("table", s::name("The rate table.")),
                        (
                            "classification",
                            s::name("The classification (case and extra spaces ignored)."),
                        ),
                        (
                            "unit",
                            s::nullable(s::name(
                                "The unit the rate is for (the line's own when left out).",
                            )),
                        ),
                        ("as_of", s::nullable(date())),
                    ],
                    &["path", "ids", "table", "classification"],
                )
            },
            run: fill_rate,
        },
        Tool {
            name: "update_rates",
            title: "Update lines to their rate tables",
            description: "Gives lines filled from a rate table (the ids given, or every one) \
                what their table says now, as one change. Lines changed by hand since they were \
                filled are left as they are.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        (
                            "ids",
                            s::array(
                                "The lines (every changed one when left out).",
                                s::integer("A row id.", 0, i64::MAX),
                                10_000,
                            ),
                        ),
                    ],
                    &["path"],
                )
            },
            run: update_rates,
        },
    ]
}

fn view(ctx: &mut Ctx, a: &Args) -> Result<(Budget, String), ToolError> {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let v = ctx.docs.view(&path, &shown)?;
    Ok((v.budget.into_owned(), shown))
}

fn filled(b: &Budget) -> Vec<Value> {
    let mut out = Vec::new();
    for a in b.categories().flat_map(|c| &c.accounts) {
        for (i, d) in a.details.iter().enumerate() {
            let (Some(src), Some(c)) = (&d.rate_source, rates::check(b, d)) else {
                continue;
            };
            let mut v = json!({
                "id": d.id,
                "account": a.code,
                "n": i + 1,
                "state": c.key(),
                "table": src.table,
                "classification": src.classification,
                "unit": src.unit,
                "as_of": src.as_of,
                "rate": d.rate.0,
                "filled": src.rate.to_string(),
            });
            match c {
                RateCheck::Changed { now } => v["now"] = json!(now.to_string()),
                RateCheck::Missing(m) => v["message"] = json!(m),
                _ => {}
            }
            out.push(v);
        }
    }
    out
}

fn list_rate_tables(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (b, shown) = view(ctx, a)?;
    let tables: Vec<Value> = b
        .rate_tables
        .iter()
        .map(
            |t| json!({"name": t.name, "rows": t.rows.len(), "units": t.units(), "notes": t.notes}),
        )
        .collect();
    let lines = filled(&b);
    let changed = lines.iter().filter(|l| l["state"] == "changed").count();
    Ok(ToolOut {
        summary: format!(
            "{shown} keeps {} rate tables; {} lines were filled from one, {changed} of them from a table that now says something else.",
            tables.len(),
            lines.len()
        ),
        data: json!({"path": shown, "tables": tables, "filled_lines": lines}),
    })
}

fn get_rate_table(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (b, shown) = view(ctx, a)?;
    let name = a.req("name")?;
    let t = rates::table(&b, name)
        .ok_or_else(|| ToolError::not_found(format!("there is no rate table {name} in {shown}")))?;
    Ok(ToolOut {
        summary: format!(
            "Rate table {} in {shown}: {} rows.",
            super::quoted(&t.name),
            t.rows.len()
        ),
        data: table_json(t),
    })
}

/// The table the tool was given, checked.
fn table_arg(v: &Value) -> Result<RateTable, ToolError> {
    let name = v["name"].as_str().unwrap_or_default();
    let mut rows = Vec::new();
    for r in v["rows"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
        let mut rates_by: BTreeMap<String, Dec> = BTreeMap::new();
        for x in r["rates"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            let unit = x["unit"].as_str().unwrap_or_default().trim().to_string();
            let text = x["rate"].as_str().unwrap_or_default();
            let rate: Dec = text
                .trim()
                .parse()
                .map_err(|e| ToolError::invalid(format!("the rate per {unit}, {text:?}: {e}")))?;
            if rates_by.insert(unit.clone(), rate).is_some() {
                return Err(ToolError::invalid(format!(
                    "{} has two rates per {unit}",
                    r["classification"].as_str().unwrap_or_default()
                )));
            }
        }
        let opt = |k: &str| {
            r[k].as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        rows.push(RateRow {
            classification: r["classification"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_string(),
            rates: rates_by,
            from: opt("from"),
            to: opt("to"),
            notes: r["notes"].as_str().unwrap_or_default().trim().to_string(),
        });
    }
    let t = RateTable {
        name: rates::valid_table_name(name).map_err(|e| ToolError::invalid(e.0))?,
        notes: v["notes"].as_str().unwrap_or_default().trim().to_string(),
        rows,
    };
    t.validate().map_err(|e| ToolError::invalid(e.0))?;
    Ok(t)
}

fn set_rate_table(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let t = table_arg(a.get("table").unwrap_or(&Value::Null))?;
    let replacing = a.str("replacing").map(str::to_string);
    let name = t.name.clone();
    let e = ctx
        .docs
        .edit(&path, &shown, &format!("Save rate table {name}"), |b| {
            rates::save_table(b, t, replacing.as_deref()).map_err(|e| match replacing {
                Some(_) if e.0.starts_with("There is no") => ToolError::not_found(e.0),
                _ => ToolError::refused(e.0),
            })
        })?;
    Ok(edited(
        &shown,
        &e,
        &format!("Kept rate table {}", super::quoted(&name)),
        obj(vec![("name", json!(name))]),
    ))
}

fn delete_rate_table(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let name = a.req("name")?.to_string();
    let e = ctx
        .docs
        .edit(&path, &shown, &format!("Delete rate table {name}"), |b| {
            rates::delete_table(b, &name)
                .map(|_| ())
                .map_err(|e| ToolError::not_found(e.0))
        })?;
    Ok(edited(
        &shown,
        &e,
        &format!("Deleted rate table {}", super::quoted(&name)),
        obj(vec![("name", json!(name))]),
    ))
}

fn import_rate_table(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let sheet = ctx.roots.existing(a.req("sheet")?, paths::SHEET_EXTS)?;
    let sheet_shown = ctx.roots.show(&sheet);
    let bytes = crate::mcp::docs::read_file(&sheet, &sheet_shown)?;
    let file = sheet
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let book = method_format::import::read_file(&bytes, &file)
        .map_err(|e| ToolError::invalid(format!("{sheet_shown}: {e}")))?;
    let index = match a.u64("sheet_index") {
        Some(n) => usize::try_from(n - 1).unwrap_or(usize::MAX),
        None => book.tables.iter().position(|t| !t.hidden).unwrap_or(0),
    };
    let t = book
        .tables
        .get(index)
        .ok_or_else(|| ToolError::invalid(format!("{sheet_shown} has no sheet {}", index + 1)))?;
    let b = ctx.docs.view(&path, &shown)?.budget.into_owned();
    let mut m: RateMapping = sheet_rates::guess(t, &b.units);
    if let Some(c) = a.get("columns") {
        let col = |k: &str| c.get(k).map(|v| v.as_u64().map(|n| n as usize - 1));
        if let Some(h) = c.get("header_row") {
            m.header_row = h.as_u64().map(|n| n as usize - 1);
        }
        for (k, slot) in [
            ("classification", &mut m.classification),
            ("from", &mut m.from),
            ("to", &mut m.to),
            ("notes", &mut m.notes),
            ("unit", &mut m.unit),
            ("rate", &mut m.rate),
        ] {
            if let Some(v) = col(k) {
                *slot = v;
            }
        }
        if let Some(list) = c.get("unit_columns").and_then(Value::as_array) {
            m.units.clear();
            for u in list {
                let unit = u["unit"].as_str().unwrap_or_default();
                if !b.units.contains_key(unit) {
                    return Err(ToolError::not_found(format!("{shown} has no unit {unit}")));
                }
                let n = u["column"].as_u64().unwrap_or(1) as usize;
                m.units.insert(unit.to_string(), n - 1);
            }
        }
    }
    let name = match a.str("name") {
        Some(n) => n.to_string(),
        None if book.tables.len() > 1 => t.name.clone(),
        None => file
            .rsplit_once('.')
            .map_or(file.as_str(), |(s, _)| s)
            .to_string(),
    };
    let mapping = json!({
        "header_row": m.header_row.map(|r| r + 1),
        "classification": m.classification.map(|c| c + 1),
        "from": m.from.map(|c| c + 1),
        "to": m.to.map(|c| c + 1),
        "notes": m.notes.map(|c| c + 1),
        "unit": m.unit.map(|c| c + 1),
        "rate": m.rate.map(|c| c + 1),
        "unit_columns": m.units.iter().map(|(u, c)| json!({"unit": u, "column": c + 1})).collect::<Vec<_>>(),
    });
    let table = match sheet_rates::read(t, &m, &name) {
        Ok(t) => t,
        Err(errors) => {
            let list: Vec<Value> = errors
                .iter()
                .map(|e| json!({"sheet": e.sheet, "row": e.row, "message": e.message}))
                .collect();
            let first = errors
                .first()
                .map(|e| format!("; row {}: {}", e.row, e.message))
                .unwrap_or_default();
            return Err(ToolError::refused(format!(
                "{sheet_shown}: nothing was imported, {} problems{first}. Mapping used: {mapping}. Problems: {}",
                list.len(),
                json!(list)
            )));
        }
    };
    if a.bool("dry_run") {
        return Ok(ToolOut {
            summary: format!(
                "{sheet_shown} holds a rate table of {} rows; nothing written.",
                table.rows.len()
            ),
            data: json!({"path": shown, "table": table_json(&table), "mapping": mapping, "dry_run": true}),
        });
    }
    let replace = a.bool("replace");
    let tname = table.name.clone();
    let rows = table.rows.len();
    let e = ctx
        .docs
        .edit(&path, &shown, &format!("Import rate table {tname}"), |b| {
            let exists = b.rate_tables.iter().any(|x| x.name == tname);
            if exists && !replace {
                return Err(ToolError::conflict(format!(
                "there is already a rate table named {tname}: give another name, or replace: true"
            )));
            }
            rates::save_table(b, table, exists.then_some(tname.as_str()))
                .map_err(|e| ToolError::refused(e.0))
        })?;
    Ok(edited(
        &shown,
        &e,
        &format!(
            "Imported rate table {} ({rows} rows)",
            super::quoted(&tname)
        ),
        obj(vec![
            ("name", json!(tname)),
            ("rows", json!(rows)),
            ("mapping", mapping),
        ]),
    ))
}

fn ids(a: &Args) -> Vec<RowId> {
    a.list("ids").iter().filter_map(Value::as_u64).collect()
}

fn fill_rate(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let want = ids(a);
    if want.is_empty() {
        return Err(ToolError::invalid("give at least one line id"));
    }
    let table = a.req("table")?.to_string();
    let class = a.req("classification")?.to_string();
    let unit = a.str("unit").map(str::to_string);
    let as_of = a.str("as_of").map(str::to_string);
    let e = ctx
        .docs
        .edit(&path, &shown, &format!("Fill rates from {table}"), |b| {
            let mut out = Vec::new();
            for id in &want {
                let d = method_session::edit::details(b)
                    .find(|d| d.id == *id)
                    .ok_or_else(|| ToolError::not_found(format!("there is no line {id}")))?;
                if d.kind != RowKind::Line {
                    return Err(ToolError::invalid(format!(
                        "row {id} is not a line: only a line has a rate to fill"
                    )));
                }
                let f = rates::fill(b, *id, &table, &class, unit.as_deref(), as_of.as_deref())
                    .map_err(|e| ToolError::refused(format!("line {id}: {e}")))?;
                out.push(json!({
                    "id": id,
                    "before": f.before,
                    "rate": f.found.rate.to_string(),
                    "unit": f.unit,
                    "classification": f.found.classification,
                    "converted": f.found.conversion_words(&f.unit),
                }));
            }
            Ok(out)
        })?;
    let n = e.out.len();
    let lines = json!(e.out);
    Ok(edited(
        &shown,
        &e,
        &format!("Filled {n} rates from {}", super::quoted(&table)),
        obj(vec![("lines", lines)]),
    ))
}

fn update_rates(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let want = ids(a);
    let e = ctx
        .docs
        .edit(&path, &shown, "Update rates to their tables", |b| {
            let mut out = Vec::new();
            for (id, was, now) in rates::changed_lines(b) {
                if !want.is_empty() && !want.contains(&id) {
                    continue;
                }
                rates::update(b, id).map_err(|e| ToolError::refused(format!("line {id}: {e}")))?;
                out.push(json!({"id": id, "before": was.to_string(), "rate": now.to_string()}));
            }
            Ok(out)
        })?;
    let n = e.out.len();
    let lines = json!(e.out);
    Ok(edited(
        &shown,
        &e,
        &format!("Updated {n} rates to their tables"),
        obj(vec![("lines", lines)]),
    ))
}
