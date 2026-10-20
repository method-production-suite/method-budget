//! Tools that work across files: what changed (diff), three-way merge,
//! exports and reports, spreadsheet import, new budgets, copies and undo.
//! They reuse the engine (`method_core::diff`, `merge`), the format's export
//! and import, and the command line's report renderer.

use std::path::Path;

use method_core::diff::{diff_with, BudgetDiff, MoneyLine};
use method_core::merge::{merge_with, Side};
use method_core::{recalc, Budget, RecalcOptions, ReportSection, ReportSpec};
use method_format::serde_json::{self, json, Map, Value};

use super::{
    budget_path, headline, money, obj, path_prop, quoted, report_of, Args, Ctx, Tool, ToolError,
    ToolOut, ToolResult,
};
use crate::mcp::docs::{parse, read_file};
use crate::mcp::schema::{self as s, object};
use crate::mcp::{limits, paths};

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "diff",
            title: "What changed",
            description: "What changed in a budget, in plain words (rows by id, accounts by \
                number) with the money that moved per account, category and topsheet line. \
                against: another budget file (changes from it to path). Without it, since: \
                \"session\" (default: what this server changed since it opened the file) or \
                \"disk\" (what someone else saved since this server last read it).",
            mutating: false,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        (
                            "against",
                            s::path("Another budget file to compare with (the older one)."),
                        ),
                        (
                            "since",
                            s::one_of("Without against: session or disk.", &["session", "disk"]),
                        ),
                    ],
                    &["path"],
                )
            },
            run: diff,
        },
        Tool {
            name: "merge",
            title: "Three-way merge",
            description: "Merges two budgets made from a common base: changes to different rows, \
                fields and setup items merge on their own; the same field changed both ways (or \
                deleted on one side and changed on the other) is a conflict, settled for prefer \
                (ours by default) and listed with the base, ours and theirs values. Writes the \
                result to out (a new file unless overwrite) unless dry_run.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        ("base", s::path("The common base budget.")),
                        ("ours", s::path("Our budget.")),
                        ("theirs", s::path("Their budget.")),
                        ("out", s::path("Where to write the merged budget (.mthd).")),
                        (
                            "prefer",
                            s::one_of(
                                "Which side conflicts take (default ours).",
                                &["ours", "theirs"],
                            ),
                        ),
                        (
                            "dry_run",
                            s::boolean("Only report what would merge and the conflicts."),
                        ),
                        ("overwrite", s::boolean("Replace out if it exists.")),
                    ],
                    &["base", "ours", "theirs", "out"],
                )
            },
            run: merge,
        },
        Tool {
            name: "export",
            title: "Export or print",
            description: "Writes the budget out: xlsx (one workbook: topsheet, accounts, detail, \
                setup tables), csv (one file per table, named '<stem> - <Table>.csv'), tsv (the \
                detail as tab-delimited text), or a report from a saved preset (or the topsheet \
                and detail) as html, txt, pdf or report_csv. The file's extension must match the \
                format; existing files are replaced only with overwrite.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        (
                            "format",
                            s::one_of("The format.", &["xlsx", "csv", "tsv", "html", "txt", "pdf", "report_csv"]),
                        ),
                        ("out", s::path("Where to write it.")),
                        ("preset", s::name("Reports: a saved report preset's name.")),
                        (
                            "sections",
                            s::array(
                                "Reports: the sections instead of the preset's.",
                                s::one_of(
                                    "A section.",
                                    &["topsheet", "accounts", "detail", "fringes", "globals", "groups", "notes", "info", "chart", "units", "sets", "locations", "currencies"],
                                ),
                                20,
                            ),
                        ),
                        ("date", s::name("Reports: the printed date and time instead of the clock (2026-10-09 14:30).")),
                        ("bom", s::boolean("CSV and text: start with a UTF-8 byte order mark (for spreadsheet apps that need it).")),
                        ("overwrite", s::boolean("Replace files that exist.")),
                    ],
                    &["path", "format", "out"],
                )
            },
            run: export,
        },
        Tool {
            name: "import_sheet",
            title: "Import a spreadsheet",
            description: "Adds a spreadsheet's rows (.xlsx, .csv, tab-delimited .txt/.tsv) to a \
                budget as detail lines, with the app's importer: into a copy of the into budget \
                (its chart and setup give the lines their meaning), or a new empty one. mapping \
                is the same JSON as method-budget import --mapping: {sheet, header_row, columns: {field: \
                column letter or number}, create_accounts, create_setup, default_account}; \
                without it the header row is found and columns guessed. Writes out (a new file \
                unless overwrite, or the into budget itself as an undoable change) unless dry_run.",
            mutating: true,
            destructive: false,
            schema: || {
                let col = json!({"type": ["string", "integer"], "description": "A column letter (A, AA) or number from 1.", "maxLength": 3, "minimum": 1, "maximum": 16384});
                let fields = [
                    "category_code",
                    "category_name",
                    "account_code",
                    "account_name",
                    "kind",
                    "description",
                    "amount",
                    "unit",
                    "x",
                    "rate",
                    "currency",
                    "x4",
                    "fringes",
                    "fringe_range",
                    "groups",
                    "set",
                    "location",
                    "note",
                    "line_total",
                ];
                let columns = object(fields.iter().map(|f| (*f, col.clone())).collect(), &[]);
                let mapping = object(
                    vec![
                        ("sheet", json!({"type": ["string", "integer"], "description": "A sheet name or number from 1.", "maxLength": 200, "minimum": 1, "maximum": 10000})),
                        ("header_row", json!({"type": ["string", "integer"], "description": "The row of titles from 1, \"none\" or \"detect\".", "maxLength": 10, "minimum": 1, "maximum": 1000000})),
                        ("columns", columns),
                        ("create_accounts", s::boolean("Add accounts and categories the budget doesn't have.")),
                        ("create_setup", s::boolean("Add fringes, groups, units, sets, locations and currencies it doesn't have.")),
                        ("default_account", s::text("The account for rows without one.")),
                    ],
                    &[],
                );
                object(
                    vec![
                        ("sheet", s::path("The spreadsheet file.")),
                        ("out", s::path("The budget to write (.mthd).")),
                        ("into", s::path("A budget to import into (a copy of it, or itself when out is the same file).")),
                        ("mapping", mapping),
                        ("title", s::text("Title of a new budget (or a new title).")),
                        ("currency", s::name("Base currency of a new budget (default USD).")),
                        ("overwrite", s::boolean("Replace out if it exists (another file than into).")),
                        ("dry_run", s::boolean("Only report what would be imported.")),
                    ],
                    &["sheet", "out"],
                )
            },
            run: import_sheet,
        },
        Tool {
            name: "import_budget_export",
            title: "Import a budget export",
            description: "Makes a whole budget from another budgeting program's budget export \
                (budget exchange JSON or XML, or its .xlsx spreadsheet or tab-delimited export; format \
                json, xml, xml-advanced, xlsx or tab when the file doesn't say) and compares \
                its totals, worked out by our engine, with the export's own figures: every \
                line, account, category, fringe row, charge, credit and the grand and net \
                totals. Returns what came over, notes on what was worked out (fringe posting, \
                cutoffs, ranges, exchange rates, unit hours: check them) and what couldn't come \
                over (formulas, globals, production totals, the rules of charges and credits), \
                then the reconciliation with the first 50 differences (all with \
                all_differences). Writes out (a new file unless overwrite) when given, unless \
                dry_run.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        (
                            "file",
                            s::path("The export file (.json, .xml, .xlsx, .txt)."),
                        ),
                        (
                            "out",
                            s::path("The budget to write (.mthd); leave out to only check."),
                        ),
                        (
                            "format",
                            s::one_of(
                                "The export's layout, when the file doesn't say.",
                                &["json", "xml", "xml-advanced", "xlsx", "tab"],
                            ),
                        ),
                        (
                            "title",
                            s::text(
                                "The budget's title (default: the export's, else the file name).",
                            ),
                        ),
                        ("overwrite", s::boolean("Replace out if it exists.")),
                        (
                            "dry_run",
                            s::boolean("Only read and compare; write nothing."),
                        ),
                        (
                            "all_differences",
                            s::boolean("List every difference, not the first 50."),
                        ),
                    ],
                    &["file"],
                )
            },
            run: import_budget_export,
        },
        Tool {
            name: "schedule_days",
            title: "Globals from a schedule's days",
            description: "Reads a schedule's days file (format method-schedule-days, the file \
                Method Schedule writes for a budget) and sets the budget's globals from its day \
                counts. Returns every figure the file counts (key total, unit:NAME, set:NAME, \
                location:NAME, week:YYYY-MM-DD, element:ID) with the global it would set, that \
                global's formula now and the new value, and what changes. Shoot days in all go \
                to SHOOT_DAYS; any other figure only when picked (pick: its key, and global to \
                name the global, else its default name); globals an earlier import set are \
                picked again, and skip leaves one out. Prep and wrap are never touched. Applies \
                as one undoable change unless dry_run.",
            mutating: true,
            destructive: false,
            schema: || {
                let pick = object(
                    vec![
                        ("key", s::text("A figure's key, as the review lists it.")),
                        ("global", s::name("The global to set (default: the figure's default name).")),
                    ],
                    &["key"],
                );
                object(
                    vec![
                        path_prop(),
                        ("file", s::path("The schedule days file (.json).")),
                        ("pick", s::array("Figures to set besides the ones picked to start.", pick, 1000)),
                        ("skip", s::array("Keys of figures to leave out.", s::text("A figure's key."), 1000)),
                        ("dry_run", s::boolean("Only review; change nothing.")),
                    ],
                    &["path", "file"],
                )
            },
            run: schedule_days,
        },
        Tool {
            name: "new_budget",
            title: "New budget",
            description: "Writes a new budget: the starter (a small chart of accounts with \
                placeholder rates), a blank one, or from a template budget (its chart, setup and \
                settings; its lines only with keep_lines; never its info, versions, notes or \
                actuals). Refuses a file that exists unless overwrite.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        (
                            "from",
                            s::one_of("starter (default) or blank.", &["starter", "blank"]),
                        ),
                        ("template", s::path("A budget to start from instead.")),
                        (
                            "keep_lines",
                            s::boolean("With template: keep its lines (without notes)."),
                        ),
                        ("title", s::text("The title.")),
                        ("currency", s::name("The base currency code (default USD).")),
                        ("overwrite", s::boolean("Replace the file if it exists.")),
                    ],
                    &["path"],
                )
            },
            run: new_budget,
        },
        Tool {
            name: "save_as",
            title: "Save a copy",
            description: "Writes a copy of the budget to another file (in the canonical form). \
                editable: true makes the copy of a read-only shared copy editable (its provenance \
                kept), as the app's Make Editable Copy does.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("out", s::path("The new file (.mthd).")),
                        (
                            "editable",
                            s::boolean("Make a shared copy's copy editable."),
                        ),
                        ("overwrite", s::boolean("Replace out if it exists.")),
                    ],
                    &["path", "out"],
                )
            },
            run: save_as,
        },
        Tool {
            name: "undo",
            title: "Undo",
            description: "Takes back the last change this server made to the file (it keeps up to \
                50 per file, since it opened it). Refused if someone else saved the file since.",
            mutating: true,
            destructive: false,
            schema: || object(vec![path_prop()], &["path"]),
            run: undo,
        },
    ]
}

// ---------------------------------------------------------------------------
// Diff

fn money_lines(lines: &[MoneyLine]) -> Value {
    json!(lines
        .iter()
        .map(|l| json!({"label": l.label, "before_cents": l.before, "after_cents": l.after, "delta_cents": l.delta}))
        .collect::<Vec<_>>())
}

/// A diff as JSON, at most [`limits::MAX_DIFF_ITEMS`] changes in full.
pub fn diff_json(d: &BudgetDiff) -> Value {
    let changes: Vec<Value> = d
        .items
        .iter()
        .take(limits::MAX_DIFF_ITEMS)
        .map(|i| {
            json!({
                "kind": i.kind.key(),
                "group": i.place.group(),
                "subject": i.subject,
                "text": i.text,
                "money_cents": i.money,
                "fields": i.fields.iter().map(|f| json!({"field": f.field, "before": f.before, "after": f.after})).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "changed": !d.is_empty(),
        "money_known": d.money_known,
        "change_count": d.items.len(),
        "changes": changes,
        "truncated": d.items.len() > limits::MAX_DIFF_ITEMS,
        "accounts": money_lines(&d.accounts),
        "categories": money_lines(&d.categories),
        "totals": money_lines(&d.totals),
    })
}

fn diff_out(from: &str, to: &str, a: &Budget, b: &Budget) -> ToolOut {
    let opts = RecalcOptions::default();
    let (ra, rb) = (recalc(a, &opts).ok(), recalc(b, &opts).ok());
    let d = diff_with(a, ra.as_ref(), b, rb.as_ref());
    let mut data = diff_json(&d);
    data["from"] = json!(from);
    data["to"] = json!(to);
    let summary = if d.is_empty() {
        format!("No changes from {from} to {to}.")
    } else {
        let text = d.to_text();
        let mut s: String = text.lines().take(80).collect::<Vec<_>>().join("\n");
        if text.lines().count() > 80 {
            s.push_str("\n...");
        }
        s
    };
    ToolOut { summary, data }
}

fn diff(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    if a.str("against").is_some() {
        let (other, other_shown) = budget_path(ctx, a, "against")?;
        let old = parse(&read_file(&other, &other_shown)?, &other_shown)?;
        let now = ctx.docs.view(&path, &shown)?.budget.into_owned();
        return Ok(diff_out(&other_shown, &shown, &old, &now));
    }
    match a.str("since").unwrap_or("session") {
        "disk" => {
            let now = ctx.docs.view(&path, &shown)?.budget.into_owned();
            let base = ctx
                .docs
                .get(&path)
                .map(|d| d.budget.clone())
                .unwrap_or_else(|| now.clone());
            Ok(diff_out(
                &format!("{shown} as this server last read it"),
                &format!("{shown} on disk"),
                &base,
                &now,
            ))
        }
        _ => {
            let now = ctx.docs.view(&path, &shown)?.budget.into_owned();
            let opened = match ctx.docs.get(&path) {
                Some(d) => parse(&d.opened, &shown)?,
                None => now.clone(),
            };
            Ok(diff_out(
                &format!("{shown} as opened"),
                &shown,
                &opened,
                &now,
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Merge

fn merge(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let mut inputs = Vec::new();
    for key in ["base", "ours", "theirs"] {
        let (p, shown) = budget_path(ctx, a, key)?;
        let b = parse(&read_file(&p, &shown)?, &shown)?;
        inputs.push((p, shown, b));
    }
    let out = ctx.roots.target(a.req("out")?, paths::BUDGET_EXTS)?;
    let out_shown = ctx.roots.show(&out);
    if inputs.iter().any(|(p, _, _)| *p == out) {
        return Err(ToolError::refused(format!(
            "{out_shown} is one of the budgets being merged; choose another out"
        )));
    }
    let side = if a.str("prefer") == Some("theirs") {
        Side::Theirs
    } else {
        Side::Ours
    };
    let m = merge_with(
        &inputs[0].2,
        &inputs[1].2,
        &inputs[2].2,
        side,
        &Default::default(),
    );
    let dry = a.bool("dry_run");
    if !dry {
        ctx.docs.write_new(
            &out,
            &out_shown,
            "Merge",
            m.budget.clone(),
            a.bool("overwrite"),
        )?;
    }
    let conflicts: Vec<Value> = m
        .conflicts
        .iter()
        .map(|c| {
            json!({"key": c.key, "what": c.what, "base": c.base, "ours": c.ours, "theirs": c.theirs, "taken": c.taken.key(), "text": c.text})
        })
        .collect();
    let mut summary = format!(
        "{} {out_shown}: {} conflict(s){}.",
        if dry { "Would write" } else { "Wrote" },
        conflicts.len(),
        if conflicts.is_empty() {
            String::new()
        } else {
            format!(", settled for {}", side.key())
        }
    );
    if let Some(p) = &m.problem {
        summary.push_str(&format!(" The merged budget has a problem: {p}."));
    }
    let totals = report_of(&m.budget).ok().map(|r| headline(&r));
    Ok(ToolOut {
        summary,
        data: json!({
            "out": out_shown,
            "written": !dry,
            "conflicts": conflicts,
            "renumbered": m.renumbered,
            "problem": m.problem,
            "totals": totals,
        }),
    })
}

// ---------------------------------------------------------------------------
// Export

fn export(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let format = a.req("format")?;
    let exts: &[&str] = match format {
        "xlsx" => &["xlsx"],
        "csv" | "report_csv" => &["csv"],
        "tsv" => &["txt", "tsv"],
        "html" => &["html", "htm"],
        "txt" => &["txt"],
        _ => &["pdf"],
    };
    let out = ctx.roots.target(a.req("out")?, exts)?;
    let overwrite = a.bool("overwrite");
    let b = ctx.docs.view(&path, &shown)?.budget.into_owned();
    let bom = a.bool("bom");
    let files: Vec<(std::path::PathBuf, Vec<u8>)> = match format {
        "xlsx" | "csv" | "tsv" => {
            let f = method_format::export::Format::parse(format).expect("listed");
            let wb = method_format::export::workbook_of(&b)
                .map_err(|e| ToolError::refused(format!("{shown}: {e}")))?;
            method_format::export::render(&wb, f, &out, method_format::export::Options { bom })
                .map_err(|e| ToolError::refused(e.to_string()))?
                .into_iter()
                .map(|f| (f.path, f.bytes))
                .collect()
        }
        _ => vec![(out.clone(), report_bytes(&b, &shown, &path, a, format)?)],
    };
    // Every file is checked as a target in its own right (a CSV export
    // writes several, named after `out`).
    let mut checked = Vec::new();
    for (p, bytes) in files {
        let typed = p.to_string_lossy().into_owned();
        let real = ctx.roots.target(&typed, exts)?;
        if real.exists() && !overwrite {
            return Err(ToolError::refused(format!(
                "{} already exists; pass overwrite: true to replace it (nothing was written)",
                ctx.roots.show(&real)
            )));
        }
        checked.push((real, bytes));
    }
    let mut written = Vec::new();
    for (p, bytes) in &checked {
        crate::cli::write_atomic(p, bytes).map_err(|e| ToolError::io(e.message))?;
        written.push(json!({"path": ctx.roots.show(p), "bytes": bytes.len()}));
    }
    Ok(ToolOut {
        summary: format!("Wrote {} file(s) from {shown} as {format}.", written.len()),
        data: json!({"path": shown, "format": format, "files": written}),
    })
}

fn report_bytes(
    b: &Budget,
    shown: &str,
    path: &Path,
    a: &Args,
    format: &str,
) -> Result<Vec<u8>, ToolError> {
    let mut spec = match a.str("preset") {
        Some(name) => {
            let key = name.trim().to_lowercase();
            b.report_presets
                .iter()
                .find(|p| p.name == name)
                .or_else(|| {
                    b.report_presets
                        .iter()
                        .find(|p| p.name.trim().to_lowercase() == key)
                })
                .map(|p| p.spec.clone())
                .ok_or_else(|| {
                    ToolError::not_found(format!(
                        "{shown} has no report preset called {}",
                        quoted(name)
                    ))
                })?
        }
        None => b
            .report_presets
            .iter()
            .find(|p| p.default)
            .map(|p| p.spec.clone())
            .unwrap_or_else(ReportSpec::default),
    };
    let sections = a.strings("sections");
    if !sections.is_empty() {
        let mut list = Vec::new();
        for n in &sections {
            let sct = ReportSection::from_name(n)
                .ok_or_else(|| ToolError::invalid(format!("unknown report section {n}")))?;
            if !list.contains(&sct) {
                list.push(sct);
            }
        }
        spec.sections = list;
    }
    let r = recalc(b, &RecalcOptions::default())
        .map_err(|e| ToolError::refused(format!("{shown}: {e}")))?;
    let (date, time) = match a.str("date") {
        Some(d) => match d.split_once(' ') {
            Some((date, time)) => (date.to_string(), time.trim().to_string()),
            None => (d.to_string(), String::new()),
        },
        None => crate::render::now_utc(),
    };
    let ctx = crate::render::Context {
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        date,
        time,
    };
    let doc = crate::render::build(b, &r, &spec, &ctx)
        .map_err(|e| ToolError::refused(format!("{shown}: {e}")))?;
    Ok(match format {
        "html" => crate::render::html::to_html(&doc).into_bytes(),
        "txt" => crate::render::text::to_txt(&doc).into_bytes(),
        "report_csv" => crate::render::text::to_csv(&doc, a.bool("bom")).into_bytes(),
        _ => crate::render::pdf::to_pdf(&doc),
    })
}

// ---------------------------------------------------------------------------
// Import

fn import_sheet(ctx: &mut Ctx, a: &Args) -> ToolResult {
    use method_format::import::{mapping_file, merge as imp, read_file as read_book};
    let sheet = ctx.roots.existing(a.req("sheet")?, paths::SHEET_EXTS)?;
    let sheet_shown = ctx.roots.show(&sheet);
    let out = ctx.roots.target(a.req("out")?, paths::BUDGET_EXTS)?;
    let out_shown = ctx.roots.show(&out);
    let mapping_text = match a.get("mapping") {
        Some(m) => serde_json::to_string(m).unwrap_or_else(|_| "{}".into()),
        None => "{}".into(),
    };
    let mapping = mapping_file::parse(&mapping_text)
        .map_err(|e| ToolError::invalid(format!("mapping: {e}")))?;
    let bytes = read_file(&sheet, &sheet_shown)?;
    let file_name = sheet
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let book = read_book(&bytes, &file_name)
        .map_err(|e| ToolError::invalid(format!("{sheet_shown}: {e}")))?;
    let (table, m) = mapping
        .resolve(&book)
        .map_err(|e| ToolError::invalid(format!("{sheet_shown}: {e}")))?;

    let into = match a.str("into") {
        Some(_) => Some(budget_path(ctx, a, "into")?),
        None => None,
    };
    let into_self = into.as_ref().is_some_and(|(p, _)| *p == out);
    let mut b = match &into {
        Some((p, shown)) => ctx.docs.view(p, shown)?.budget.into_owned(),
        None => {
            let stem = sheet
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Imported".into());
            Budget::new(stem, a.str("currency").unwrap_or("USD"))
        }
    };
    if let Some(t) = a.str("title") {
        b.title = t.trim().to_string();
    }
    let preview = imp::preview(&b, table, &m, &mapping.options);
    let (plan, applied) = imp::import(&mut b, table, &m, &mapping.options).map_err(|e| {
        let mut msg = format!("{sheet_shown}: {e}");
        for n in preview.plan.skipped.iter().take(5) {
            msg.push_str(&format!("; row {}: {}", n.row, n.message));
        }
        if into.is_none() && !mapping.options.create_accounts {
            msg.push_str(" (a new budget has no accounts: give into, or create_accounts: true in the mapping)");
        }
        ToolError::refused(msg)
    })?;
    let r = recalc(&b, &RecalcOptions::default()).map_err(|e| {
        ToolError::refused(format!(
            "{sheet_shown}: the imported budget doesn't add up: {e}"
        ))
    })?;
    let notes = |list: &[imp::RowNote]| -> Value {
        json!(list
            .iter()
            .take(20)
            .map(|n| json!({"row": n.row, "message": n.message}))
            .collect::<Vec<_>>())
    };
    let dry = a.bool("dry_run");
    let mut written = false;
    if !dry {
        if into_self {
            let (p, shown) = into.as_ref().expect("into");
            let new = b.clone();
            ctx.docs
                .edit(p, shown, &format!("Import {file_name}"), move |cur| {
                    *cur = new;
                    Ok(())
                })?;
        } else {
            ctx.docs.write_new(
                &out,
                &out_shown,
                &format!("Import {file_name}"),
                b.clone(),
                a.bool("overwrite"),
            )?;
        }
        written = true;
    }
    let reconciliation = preview.reconciliation.as_ref().map(|rec| {
        json!({
            "compared": rec.compared,
            "mismatched": rec.mismatched,
            "sheet_sum_cents": rec.sheet_sum,
            "computed_sum_cents": rec.computed_sum,
            "first": rec.first.iter().take(20).map(|mm| json!({"row": mm.row, "account": mm.account, "description": mm.description, "sheet_cents": mm.sheet, "computed_cents": mm.computed})).collect::<Vec<_>>(),
        })
    });
    let summary = format!(
        "{} {} line(s) from sheet {} of {sheet_shown} into {} account(s){}; grand total {}.",
        if dry { "Would import" } else { "Imported" },
        applied.row_ids.len(),
        quoted(&table.name),
        plan.matched_accounts.len() + plan.new_accounts.len(),
        if plan.skipped.is_empty() {
            String::new()
        } else {
            format!(", {} row(s) skipped", plan.skipped.len())
        },
        money(r.grand_total)
    );
    Ok(ToolOut {
        summary,
        data: json!({
            "out": out_shown,
            "written": written,
            "sheet": table.name,
            "lines": applied.row_ids.len(),
            "row_ids": applied.row_ids.iter().take(limits::MAX_BATCH).collect::<Vec<_>>(),
            "added": {"categories": applied.categories, "accounts": applied.accounts, "setup": applied.setup},
            "skipped_count": plan.skipped.len(),
            "skipped": notes(&plan.skipped),
            "warnings": notes(&plan.warnings),
            "reconciliation": reconciliation,
            "totals": headline(&r),
        }),
    })
}

fn import_budget_export(ctx: &mut Ctx, a: &Args) -> ToolResult {
    use method_format::import::exchange::{self, ExportFormat};
    let file = ctx.roots.existing(a.req("file")?, paths::EXPORT_EXTS)?;
    let shown = ctx.roots.show(&file);
    let format =
        match a.str("format") {
            Some(f) => Some(ExportFormat::parse(f).ok_or_else(|| {
                ToolError::invalid(format!("format: unknown layout {}", quoted(f)))
            })?),
            None => None,
        };
    let bytes = read_file(&file, &shown)?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (mut im, mut rec) = exchange::import(&bytes, &name, format)
        .map_err(|e| ToolError::invalid(format!("{shown}: {e}")))?;
    if let Some(t) = a.str("title") {
        im.budget.title = t.trim().to_string();
        rec = exchange::reconcile(&im);
    }
    let mut written = Value::Null;
    if let (Some(o), false) = (a.str("out"), a.bool("dry_run")) {
        let out = ctx.roots.target(o, paths::BUDGET_EXTS)?;
        let out_shown = ctx.roots.show(&out);
        ctx.docs.write_new(
            &out,
            &out_shown,
            &format!("Import {name}"),
            im.budget.clone(),
            a.bool("overwrite"),
        )?;
        written = json!(out_shown);
    }
    let limit = if a.bool("all_differences") {
        None
    } else {
        Some(50)
    };
    let mut data = crate::cmd::import_export::to_json(&im, &rec, limit);
    if let Value::Object(m) = &mut data {
        m.insert("written".into(), written.clone());
    }
    let compared = rec.compared();
    let summary = format!(
        "Read {shown} ({}): {} categories, {} accounts, {} lines. {}{}",
        im.format.label(),
        im.counts.categories,
        im.counts.accounts,
        im.counts.lines,
        match (&rec.calc_error, rec.differences.len()) {
            (Some(e), _) => format!("The imported budget doesn't recalculate: {e}."),
            (None, 0) => format!(
                "All {compared} of the export's figures match (grand total {}).",
                money(rec.export_grand)
            ),
            (None, n) => format!(
                "{n} of the export's {compared} figures differ (grand total: export {}, imported {}).",
                money(rec.export_grand),
                rec.grand.map_or("?".into(), money)
            ),
        },
        match written.as_str() {
            Some(o) => format!(" Wrote {o}."),
            None => String::new(),
        }
    );
    Ok(ToolOut { summary, data })
}

fn schedule_days(ctx: &mut Ctx, a: &Args) -> ToolResult {
    use method_session::schedule_days as sd;
    let (path, shown) = budget_path(ctx, a, "path")?;
    let file = ctx.roots.existing(a.req("file")?, paths::SCHEDULE_EXTS)?;
    let file_shown = ctx.roots.show(&file);
    let bytes = read_file(&file, &file_shown)?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let days = sd::read(&name, &bytes).map_err(ToolError::invalid)?;
    let add: Vec<(String, Option<String>)> = a
        .list("pick")
        .iter()
        .filter_map(|p| {
            let key = p.get("key")?.as_str()?.to_string();
            let global = p.get("global").and_then(Value::as_str).map(str::to_string);
            Some((key, global))
        })
        .collect();
    let skip = a.strings("skip");
    let b = ctx.docs.view(&path, &shown)?.budget.into_owned();
    let picks = sd::picks_with(&b, &days, &add, &skip).map_err(ToolError::invalid)?;
    let mut review = sd::review(&b, &days, Some(&picks));
    review.file = file_shown.clone();
    if let Some(p) = review.problems.first() {
        return Err(ToolError::invalid(format!("{p} Nothing was changed.")));
    }
    let dry = a.bool("dry_run");
    let mut message = None;
    if !dry && review.can_apply {
        let label = format!("Set globals from the schedule {}", days.schedule.trim());
        let e = ctx.docs.edit(&path, &shown, label.trim(), |b| {
            sd::apply(b, &days, Some(&picks)).map_err(ToolError::refused)
        })?;
        message = Some(e.out.message());
    }
    let mut data = serde_json::to_value(&review).map_err(|e| ToolError::io(e.to_string()))?;
    if let Value::Object(m) = &mut data {
        m.remove("token");
        m.insert("applied".into(), json!(message.is_some()));
        m.insert("message".into(), json!(message));
    }
    let summary = schedule_summary(&file_shown, &review, message.as_deref(), dry);
    Ok(ToolOut { summary, data })
}

/// The `schedule_days` tool's sentence: what happened, never more.
pub(crate) fn schedule_summary(
    file: &str,
    review: &method_session::schedule_days::Review,
    message: Option<&str>,
    dry_run: bool,
) -> String {
    let picked = review.figures.iter().filter(|f| f.picked).count();
    let what = match message {
        Some(m) => m.to_string(),
        None if picked == 0 => "Nothing is picked; nothing changed.".into(),
        None if !review.problems.is_empty() => format!("Refused: {}", review.problems.join(" ")),
        None if !review.can_apply => format!(
            "The {} already set from this schedule; nothing changed.",
            count_words(picked, "picked global is", "picked globals are")
        ),
        None if dry_run => format!(
            "Dry run: would set {} new and {} changed globals ({} already match); nothing changed.",
            review.adds, review.updates, review.same
        ),
        None => "Nothing changed.".into(),
    };
    let gone = review
        .figures
        .iter()
        .filter(|f| f.kind == "missing" && !f.picked)
        .count();
    let mut s = format!(
        "{file}: {} ({}, {}), {} shoot days. {what}",
        review.schedule, review.board, review.sub_board, review.shoot_days
    );
    if !review.changed.is_empty() {
        s.push_str(&format!(
            " The schedule changed since these were set from it: {}.",
            review.changed.join(", ")
        ));
    }
    if gone > 0 {
        s.push_str(&format!(
            " {} set from figures no longer in the file, left as they are (pick linked:GLOBAL to set one to 0).",
            count_words(gone, "global was", "globals were")
        ));
    }
    s
}

/// "1 global" or "3 globals".
fn count_words(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

// ---------------------------------------------------------------------------
// New budgets, copies, undo

/// A budget as a template: chart, setup and settings kept; info, baseline,
/// versions, actuals, provenance and category and account notes cleared;
/// rows dropped, or kept without their notes (as the app's New From
/// Template does).
fn strip(b: &Budget, keep_lines: bool) -> Budget {
    let mut t = b.clone();
    t.info.clear();
    t.baseline = None;
    t.versions.clear();
    t.actuals = Default::default();
    t.provenance = None;
    for c in t.categories_mut() {
        c.notes.clear();
        for acct in &mut c.accounts {
            acct.notes.clear();
            if keep_lines {
                for d in &mut acct.details {
                    d.notes.clear();
                }
            } else {
                acct.details.clear();
            }
        }
    }
    t
}

fn new_budget(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let out = ctx.roots.target(a.req("path")?, paths::BUDGET_EXTS)?;
    let shown = ctx.roots.show(&out);
    let currency = a.str("currency").map(|c| c.trim().to_string());
    let (mut b, from) = match a.str("template") {
        Some(_) => {
            let (p, tshown) = budget_path(ctx, a, "template")?;
            let t = parse(&read_file(&p, &tshown)?, &tshown)?;
            (
                strip(&t, a.bool("keep_lines")),
                format!("template {tshown}"),
            )
        }
        None if a.str("from") == Some("blank") => (
            method_core::starter::blank(
                "Untitled",
                currency.clone().unwrap_or_else(|| "USD".into()),
            ),
            "blank".to_string(),
        ),
        None => (method_core::starter::budget(), "starter".to_string()),
    };
    if let Some(c) = currency.filter(|c| *c != b.base_currency) {
        let base = b.base_currency.clone();
        method_core::rename::rename(&mut b, method_core::SetupKind::Currency, &base, &c)
            .map_err(|e| ToolError::invalid(format!("currency: {e}")))?;
    }
    if let Some(t) = a.str("title") {
        b.title = t.trim().to_string();
    }
    let rev = ctx
        .docs
        .write_new(&out, &shown, "New budget", b.clone(), a.bool("overwrite"))?;
    let totals = report_of(&b).ok().map(|r| headline(&r));
    Ok(ToolOut {
        summary: format!(
            "Wrote {shown}: a new budget {} from the {from}.",
            quoted(&b.title)
        ),
        data: json!({"path": shown, "revision": rev, "from": from, "title": b.title, "totals": totals}),
    })
}

fn save_as(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let out = ctx.roots.target(a.req("out")?, paths::BUDGET_EXTS)?;
    let out_shown = ctx.roots.show(&out);
    if out == path {
        return Err(ToolError::refused(
            "out is the budget itself; choose another file",
        ));
    }
    let mut b = ctx.docs.view(&path, &shown)?.budget.into_owned();
    if a.bool("editable") {
        b = method_core::share::editable_copy(&b);
    }
    let rev = ctx
        .docs
        .write_new(&out, &out_shown, "Save a copy", b, a.bool("overwrite"))?;
    Ok(ToolOut {
        summary: format!("Wrote a copy of {shown} to {out_shown}."),
        data: json!({"path": shown, "out": out_shown, "revision": rev}),
    })
}

fn undo(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let (label, was, now) = ctx.docs.undo(&path, &shown)?;
    let (rb, ra) = (report_of(&was).ok(), report_of(&now).ok());
    let mut data: Map<String, Value> = obj(vec![
        ("path", json!(shown)),
        ("undone", json!(label)),
        (
            "revision",
            json!(ctx
                .docs
                .get(&path)
                .map(|d| crate::mcp::docs::revision(&d.base))),
        ),
        (
            "undo_left",
            json!(ctx.docs.get(&path).map(|d| d
                .history
                .iter()
                .rev()
                .map(|s| s.label.clone())
                .collect::<Vec<_>>())),
        ),
    ]);
    let mut summary = format!("Undid {} in {shown}.", quoted(&label));
    if let (Some(b), Some(r)) = (&rb, &ra) {
        data.insert(
            "totals".into(),
            json!({"before": headline(b), "after": headline(r)}),
        );
        summary.push_str(&format!(
            " Net total {} -> {}.",
            money(b.net_total),
            money(r.net_total)
        ));
    }
    Ok(ToolOut {
        summary,
        data: Value::Object(data),
    })
}
