//! Tools that only read: opening a budget, its topsheet, accounts and lines,
//! setup tables, charges and credits, totals, explanations, search and checks.

use std::fmt::Write as _;
use std::path::Path;

use method_core::explain::{Explainer, Role, Step, Target};
use method_core::rename::SetupKind;
use method_core::{
    Budget, ChargeKind, CreditCap, CreditSource, Detail, FringeKind, FringePosting, Report,
    RowKind, RowTotals, TopsheetRow,
};
use method_format::serde_json::{json, Map, Value};

use super::{
    budget_path, exclude_prop, headline, money, obj, options, path_prop, quoted, report_of, Args,
    Ctx, Tool, ToolError, ToolOut, ToolResult,
};
use crate::mcp::limits;
use crate::mcp::schema::{self as s, object};

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "list_budgets",
            title: "List budget files",
            description: "Lists the budget files (.mthd) in the server's folders, or in one \
                folder inside them, with their sizes. Hidden folders and symbolic links are skipped.",
            mutating: false,
            destructive: false,
            schema: || {
                object(
                    vec![
                        ("folder", s::path("A folder inside a root (default: the first root).")),
                        ("recursive", s::boolean("Look in subfolders too (default true).")),
                    ],
                    &[],
                )
            },
            run: list_budgets,
        },
        Tool {
            name: "budget_open",
            title: "Open a budget",
            description: "Opens a budget file (or re-opens it after someone else saved it) and \
                returns what budget_info returns. Re-opening a file that changed on disk takes it \
                as it is now and drops this server's undo steps for it; call it after a conflict.",
            mutating: false,
            destructive: false,
            schema: || object(vec![path_prop()], &["path"]),
            run: budget_open,
        },
        Tool {
            name: "budget_info",
            title: "Budget summary",
            description: "Title, currencies, account numbering, fringe posting, counts (categories, \
                accounts, lines, setup items, charges, credits, versions), the headline totals \
                in cents, whether the file is a read-only shared copy, its revision and the undo \
                steps this server can take back.",
            mutating: false,
            destructive: false,
            schema: || object(vec![path_prop()], &["path"]),
            run: budget_info,
        },
        Tool {
            name: "list_topsheet",
            title: "Topsheet",
            description: "The topsheet in order: categories (code, name, total, fringes earned, \
                number of accounts) and production totals (above-the-line ends at the first one), \
                then the summary rows.",
            mutating: false,
            destructive: false,
            schema: || object(vec![path_prop(), exclude_prop()], &["path"]),
            run: list_topsheet,
        },
        Tool {
            name: "list_accounts",
            title: "Accounts",
            description: "Every account (or one category's) with its category, line count, \
                total, detail total and fringes earned.",
            mutating: false,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("category", s::name("Only this category's accounts (its code).")),
                        exclude_prop(),
                    ],
                    &["path"],
                )
            },
            run: list_accounts,
        },
        Tool {
            name: "get_account",
            title: "Account detail",
            description: "One account's rows with every field (id, position n, kind, description, \
                amount, unit, x, unit2, rate, unit3, x4, unit4, currency, fringes, groups, set, \
                location, fringe_range, style, notes) and what each works out to (quantity, \
                total_cents, fringes_cents, subtotal). Row ids are stable: use them with the \
                editing tools. Long accounts come in pages (offset, limit).",
            mutating: false,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("account", s::name("The account number, as the budget writes it (21-01).")),
                        ("offset", s::integer("Skip this many rows (default 0).", 0, i64::MAX)),
                        (
                            "limit",
                            s::integer("At most this many rows (default 500).", 1, limits::MAX_ROWS_PAGE as i64),
                        ),
                        exclude_prop(),
                    ],
                    &["path", "account"],
                )
            },
            run: get_account,
        },
        Tool {
            name: "list_globals",
            title: "Globals",
            description: "Every global: name, formula, value, description, unit, and how many \
                lines and other setup use it.",
            mutating: false,
            destructive: false,
            schema: || object(vec![path_prop()], &["path"]),
            run: list_globals,
        },
        Tool {
            name: "list_setup",
            title: "Setup table",
            description: "One setup table with every field and how much each item is used: \
                fringes (rate, percent or flat, cutoff, total earned), groups (in budget total, \
                totals), sets, locations, units (hours) or currencies (exchange rate; the base \
                currency first).",
            mutating: false,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        (
                            "type",
                            s::one_of("Which table.", &["fringe", "group", "set", "location", "unit", "currency"]),
                        ),
                    ],
                    &["path", "type"],
                )
            },
            run: list_setup,
        },
        Tool {
            name: "list_charges",
            title: "Charges and credits",
            description: "Contractual charges (percent or flat, exclusions, applied or not) and \
                credits (rows, caps, discount), each with what it comes to.",
            mutating: false,
            destructive: false,
            schema: || object(vec![path_prop()], &["path"]),
            run: list_charges,
        },
        Tool {
            name: "totals",
            title: "Totals",
            description: "Every total in cents: production levels with their categories, \
                above-the-line, below-the-line, fringes, each charge, the grand total, each \
                credit and the net total (and the net in the totals currency). The summary is the \
                topsheet as method show prints it.",
            mutating: false,
            destructive: false,
            schema: || object(vec![path_prop(), exclude_prop()], &["path"]),
            run: totals,
        },
        Tool {
            name: "explain",
            title: "Explain a number",
            description: "How one total is worked out, as a tree of steps whose leaves add up to \
                it exactly (lines, globals, currency, rounding, fringes, cutoffs, posting, charges, \
                credits). total: an account (21-01, 21-01/lines, 21-01/fringes), a category (21, \
                21/fringes), a line (21-01:3 or #ID), level:NAME, charge:NAME, credit:NAME, \
                fringe:NAME, or atl, btl, atl+btl, fringes, credits, grand, net.",
            mutating: false,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("total", s::name("The total to explain.")),
                        (
                            "depth",
                            s::integer("Levels of the tree (default 4).", 1, limits::MAX_EXPLAIN_DEPTH as i64),
                        ),
                        exclude_prop(),
                    ],
                    &["path", "total"],
                )
            },
            run: explain,
        },
        Tool {
            name: "find",
            title: "Find",
            description: "Searches line descriptions, formulas and notes, account and category \
                names and numbers, setup item names and charges and credits for a piece of text \
                (ignoring case unless case_sensitive). Returns where each match is, with row ids.",
            mutating: false,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("query", s::name("The text to look for.")),
                        ("case_sensitive", s::boolean("Match letter case exactly.")),
                        (
                            "limit",
                            s::integer("At most this many matches (default 100).", 1, limits::MAX_FIND as i64),
                        ),
                    ],
                    &["path", "query"],
                )
            },
            run: find,
        },
        Tool {
            name: "validate",
            title: "Check a budget",
            description: "Recalculates the budget and reports any problem (a formula that doesn't \
                work, an unknown fringe or currency) and warnings: lines naming units, sets or \
                locations the budget doesn't have, repeated account or category numbers, and \
                fringes after the last production total that no level holds.",
            mutating: false,
            destructive: false,
            schema: || object(vec![path_prop()], &["path"]),
            run: validate,
        },
    ]
}

// ---------------------------------------------------------------------------
// Shared pieces

pub fn kind_name(k: RowKind) -> &'static str {
    match k {
        RowKind::Line => "line",
        RowKind::Subtotal => "subtotal",
        RowKind::Adjustment => "adjustment",
        RowKind::Separator => "separator",
    }
}

fn posting(p: &FringePosting) -> Value {
    match p {
        FringePosting::Budget => json!({"mode": "budget"}),
        FringePosting::ProductionLevel => json!({"mode": "level"}),
        FringePosting::Category { suffix } => json!({"mode": "category", "suffix": suffix}),
        FringePosting::Account => json!({"mode": "account"}),
    }
}

fn lines_in(b: &Budget) -> (usize, usize) {
    let rows = b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details);
    let mut costed = 0;
    let mut all = 0;
    for d in rows {
        all += 1;
        if d.kind.is_costed() {
            costed += 1;
        }
    }
    (costed, all)
}

/// `SharedCopy` as `shared_copy`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The budget summary `budget_info` and `budget_open` return.
fn info(ctx: &Ctx, path: &Path, shown: &str, b: &Budget, revision: &str, changed: bool) -> ToolOut {
    let (lines, rows) = lines_in(b);
    let accounts = b.categories().map(|c| c.accounts.len()).sum::<usize>();
    let levels = b
        .topsheet
        .iter()
        .filter(|r| matches!(r, TopsheetRow::ProductionTotal(_)))
        .count();
    let report = report_of(b);
    let shared = crate::mcp::docs::is_shared_copy(b);
    let undo: Vec<&str> = ctx
        .docs
        .get(path)
        .map(|d| {
            d.history
                .iter()
                .rev()
                .take(10)
                .map(|s| s.label.as_str())
                .collect()
        })
        .unwrap_or_default();
    let c = &b.account_coding;
    let mut data = obj(vec![
        ("path", json!(shown)),
        ("revision", json!(revision)),
        ("changed_on_disk", json!(changed)),
        ("title", json!(b.title)),
        ("base_currency", json!(b.base_currency)),
        ("totals_currency", json!(b.totals_currency)),
        ("read_only", json!(shared)),
        (
            "account_coding",
            json!({
                "category_width": c.category_width,
                "account_width": c.account_width,
                "sub_width": c.sub_width,
                "set_width": c.set_width,
                "separator": c.separator.map(|s| s.to_string()),
            }),
        ),
        ("fringe_posting", posting(&b.fringe_posting)),
        (
            "counts",
            json!({
                "categories": b.categories().count(),
                "production_totals": levels,
                "accounts": accounts,
                "lines": lines,
                "rows": rows,
                "globals": b.globals.len(),
                "fringes": b.fringes.len(),
                "groups": b.groups.len(),
                "sets": b.sets.len(),
                "locations": b.locations.len(),
                "units": b.units.len(),
                "currencies": b.currencies.len(),
                "charges": b.charges.len(),
                "credits": b.credits.len(),
                "versions": b.versions.len(),
            }),
        ),
        ("undo", json!(undo)),
    ]);
    if let Some(p) = &b.provenance {
        data.insert(
            "provenance".into(),
            json!({
                "kind": snake(&format!("{:?}", p.kind)),
                "source_title": p.source_title,
                "source_version": p.source_version,
            }),
        );
    }
    let mut summary = format!(
        "{shown}: {} ({}), {} categories, {accounts} accounts, {lines} lines",
        quoted(&b.title),
        b.base_currency,
        b.categories().count()
    );
    match &report {
        Ok(r) => {
            data.insert("totals".into(), headline(r));
            let _ = write!(summary, "; net total {}.", money(r.net_total));
        }
        Err(e) => {
            data.insert("problem".into(), json!(e));
            let _ = write!(summary, "; it doesn't add up: {e}.");
        }
    }
    if shared {
        summary.push_str(" A shared copy: read-only.");
    }
    if changed {
        summary.push_str(
            " The file changed on disk since this server read it; call budget_open before editing.",
        );
    }
    ToolOut {
        summary,
        data: Value::Object(data),
    }
}

/// The budget at the `path` argument and the recalculated report (with the
/// `exclude_groups` filter).
fn load(ctx: &mut Ctx, a: &Args) -> Result<(Budget, Report, String, bool), ToolError> {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let v = ctx.docs.view(&path, &shown)?;
    let b = v.budget.into_owned();
    let r = method_core::recalc(&b, &options(a))
        .map_err(|e| ToolError::refused(format!("{shown} doesn't add up: {e}")))?;
    Ok((b, r, shown, v.changed_on_disk))
}

fn note_changed(data: &mut Map<String, Value>, changed: bool) {
    if changed {
        data.insert("changed_on_disk".into(), json!(true));
    }
}

/// A detail row with what it works out to.
pub fn row_json(d: &Detail, n: usize, rt: Option<&RowTotals>) -> Value {
    let notes: Vec<Value> = d
        .notes
        .iter()
        .map(|nt| json!({"id": nt.id, "text": nt.text, "created": nt.created, "done": nt.done}))
        .collect();
    let mut v = json!({
        "id": d.id,
        "n": n,
        "kind": kind_name(d.kind),
        "description": d.description,
        "amount": d.amount.0,
        "unit": d.unit,
        "x": d.x.0,
        "unit2": d.unit2,
        "rate": d.rate.0,
        "unit3": d.unit3,
        "x4": d.x4.0,
        "unit4": d.unit4,
        "currency": d.currency,
        "fringes": d.fringes,
        "groups": d.groups,
        "set": d.set,
        "location": d.location,
        "fringe_range": d.fringe_range,
        "bold": d.bold,
        "italic": d.italic,
        "indent": d.indent,
        "color": d.color,
        "notes": notes,
    });
    if let Some(r) = rt {
        v["included"] = json!(r.included);
        v["quantity"] = json!(r.quantity.to_string());
        v["row_currency"] = json!(r.currency);
        v["local_total_cents"] = json!(r.local_total);
        v["total_cents"] = json!(r.total);
        v["subtotal_cents"] = json!(r.subtotal);
        v["fringes_cents"] = json!(r.fringes);
        v["fringe_total_cents"] = json!(r.fringe_total);
    }
    v
}

// ---------------------------------------------------------------------------
// The tools

fn list_budgets(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let dir = ctx.roots.folder(a.str("folder"))?;
    let recursive = a.opt_bool("recursive").unwrap_or(true);
    let mut found: Vec<Value> = Vec::new();
    let mut truncated = false;
    let mut stack = vec![(dir.clone(), 0usize)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            // `file_type` doesn't follow symlinks: links are skipped.
            let Ok(ft) = e.file_type() else { continue };
            let p = e.path();
            if ft.is_dir() {
                if recursive && depth + 1 < limits::MAX_LIST_DEPTH {
                    stack.push((p, depth + 1));
                }
            } else if ft.is_file() && method_format::has_budget_extension(&p) {
                if found.len() >= limits::MAX_LISTED_FILES {
                    truncated = true;
                    break;
                }
                let bytes = e.metadata().map(|m| m.len()).unwrap_or(0);
                found.push(json!({"path": ctx.roots.show(&p), "bytes": bytes}));
            }
        }
    }
    found.sort_by(|x, y| x["path"].as_str().cmp(&y["path"].as_str()));
    let summary = format!(
        "{} budget file(s) in {}{}.",
        found.len(),
        dir.display(),
        if truncated { " (list cut short)" } else { "" }
    );
    Ok(ToolOut {
        summary,
        data: json!({"folder": dir.display().to_string(), "files": found, "truncated": truncated}),
    })
}

fn budget_open(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let (doc, state) = ctx.docs.open(&path, &shown)?;
    let b = doc.budget.clone();
    let rev = crate::mcp::docs::revision(&doc.base);
    let mut out = info(ctx, &path, &shown, &b, &rev, false);
    if let Value::Object(d) = &mut out.data {
        d.insert("reloaded".into(), json!(state.reloaded));
        d.insert("undo_dropped".into(), json!(state.dropped_undo));
    }
    if state.reloaded {
        out.summary.push_str(&format!(
            " Reloaded: the file had changed on disk ({} undo step(s) dropped).",
            state.dropped_undo
        ));
    }
    Ok(out)
}

fn budget_info(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let v = ctx.docs.view(&path, &shown)?;
    let (b, rev, changed) = (v.budget.into_owned(), v.revision, v.changed_on_disk);
    Ok(info(ctx, &path, &shown, &b, &rev, changed))
}

fn list_topsheet(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (b, r, shown, changed) = load(ctx, a)?;
    let mut rows = Vec::new();
    let mut ci = 0;
    let mut li = 0;
    for row in &b.topsheet {
        match row {
            TopsheetRow::Category(c) => {
                let t = &r.categories[ci];
                rows.push(json!({
                    "type": "category",
                    "code": c.code,
                    "name": c.name,
                    "accounts": c.accounts.len(),
                    "total_cents": t.total,
                    "fringe_total_cents": t.fringe_total,
                    "fringe_account": t.fringe_account.as_ref().map(|(code, amt)| json!({"code": code, "amount_cents": amt})),
                }));
                ci += 1;
            }
            TopsheetRow::ProductionTotal(p) => {
                let lvl = r.levels.iter().filter(|l| !l.name.is_empty()).nth(li);
                rows.push(json!({
                    "type": "production_total",
                    "name": p.name,
                    "total_cents": lvl.map(|l| l.total),
                    "posted_fringes_cents": lvl.map(|l| l.posted_fringes),
                    "above_the_line": li == 0,
                }));
                li += 1;
            }
        }
    }
    let mut data = obj(vec![
        ("path", json!(shown)),
        ("title", json!(b.title)),
        ("rows", json!(rows)),
        ("totals", headline(&r)),
    ]);
    note_changed(&mut data, changed);
    Ok(ToolOut {
        summary: format!(
            "{shown}: {} topsheet rows; ATL {}, BTL {}, grand total {}, net total {}.",
            rows.len(),
            money(r.total_atl),
            money(r.total_btl),
            money(r.grand_total),
            money(r.net_total)
        ),
        data: Value::Object(data),
    })
}

fn list_accounts(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (b, r, shown, changed) = load(ctx, a)?;
    let only = a.str("category");
    if let Some(code) = only {
        if !b.categories().any(|c| c.code == code) {
            return Err(ToolError::not_found(format!(
                "{shown} has no category {code}"
            )));
        }
    }
    let mut list = Vec::new();
    for (c, ct) in b.categories().zip(&r.categories) {
        if only.is_some_and(|o| o != c.code) {
            continue;
        }
        for (acct, at) in c.accounts.iter().zip(&ct.accounts) {
            list.push(json!({
                "code": acct.code,
                "name": acct.name,
                "category": c.code,
                "rows": acct.details.len(),
                "lines": acct.details.iter().filter(|d| d.kind.is_costed()).count(),
                "total_cents": at.total,
                "details_total_cents": at.details_total,
                "fringe_total_cents": at.fringe_total,
            }));
        }
    }
    let mut data = obj(vec![("path", json!(shown)), ("accounts", json!(list))]);
    note_changed(&mut data, changed);
    Ok(ToolOut {
        summary: format!("{shown}: {} account(s).", list.len()),
        data: Value::Object(data),
    })
}

/// The account with this number: category index, account index.
pub fn find_account(b: &Budget, code: &str) -> Option<(usize, usize)> {
    b.categories().enumerate().find_map(|(ci, c)| {
        c.accounts
            .iter()
            .position(|a| a.code == code)
            .map(|ai| (ci, ai))
    })
}

fn get_account(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (b, r, shown, changed) = load(ctx, a)?;
    let code = a.req("account")?;
    let (ci, ai) = find_account(&b, code)
        .ok_or_else(|| ToolError::not_found(format!("{shown} has no account {code}")))?;
    let cat = b.categories().nth(ci).expect("found");
    let acct = &cat.accounts[ai];
    let at = &r.categories[ci].accounts[ai];
    let offset = a.u64("offset").unwrap_or(0) as usize;
    let limit = a.u64("limit").unwrap_or(500) as usize;
    let rows: Vec<Value> = acct
        .details
        .iter()
        .zip(&at.rows)
        .enumerate()
        .skip(offset)
        .take(limit)
        .map(|(i, (d, rt))| row_json(d, i + 1, Some(rt)))
        .collect();
    let notes: Vec<&str> = acct.notes.iter().map(|n| n.text.as_str()).collect();
    let mut data = obj(vec![
        ("path", json!(shown)),
        ("account", json!(acct.code)),
        ("name", json!(acct.name)),
        ("category", json!({"code": cat.code, "name": cat.name})),
        ("notes", json!(notes)),
        ("row_count", json!(acct.details.len())),
        ("offset", json!(offset)),
        ("rows", json!(rows)),
        ("total_cents", json!(at.total)),
        ("details_total_cents", json!(at.details_total)),
        ("fringe_total_cents", json!(at.fringe_total)),
    ]);
    note_changed(&mut data, changed);
    let more = acct.details.len().saturating_sub(offset + rows.len());
    Ok(ToolOut {
        summary: format!(
            "{shown}: account {} {} has {} row(s), total {}{}.",
            acct.code,
            quoted(&acct.name),
            acct.details.len(),
            money(at.total),
            if more > 0 {
                format!("; {more} more row(s) after these (use offset)")
            } else {
                String::new()
            }
        ),
        data: Value::Object(data),
    })
}

fn list_globals(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let v = ctx.docs.view(&path, &shown)?;
    let b = v.budget.as_ref();
    let report = report_of(b).ok();
    let uses = method_core::usage::usage(b, SetupKind::Global);
    let list: Vec<Value> = b
        .globals
        .iter()
        .map(|(name, f)| {
            let gi = b.global_info.get(name).cloned().unwrap_or_default();
            let u = uses.get(name).copied().unwrap_or_default();
            json!({
                "name": name,
                "formula": f.0,
                "value": report.as_ref().and_then(|r| r.globals.get(name)).map(|d| d.to_string()),
                "description": gi.description,
                "unit": gi.unit,
                "lines": u.lines,
                "other_uses": u.other,
            })
        })
        .collect();
    Ok(ToolOut {
        summary: format!("{shown}: {} global(s).", list.len()),
        data: json!({"path": shown, "globals": list, "changed_on_disk": v.changed_on_disk}),
    })
}

/// A setup kind from the `type` argument (`global` only where allowed).
pub fn setup_kind(s: &str) -> Result<SetupKind, ToolError> {
    SetupKind::from_name(s).ok_or_else(|| ToolError::invalid(format!("`{s}` is not a setup table")))
}

fn list_setup(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let kind = setup_kind(a.req("type")?)?;
    let v = ctx.docs.view(&path, &shown)?;
    let b = v.budget.as_ref();
    let report = report_of(b).ok();
    let uses = method_core::usage::usage(b, kind);
    let used = |name: &str| uses.get(name).copied().unwrap_or_default();
    let mut items: Vec<Value> = Vec::new();
    match kind {
        SetupKind::Fringe => {
            for (name, f) in &b.fringes {
                let total = report
                    .as_ref()
                    .and_then(|r| r.fringes.get(name))
                    .map(|s| s.total);
                let mut item = json!({
                    "name": name, "description": f.description, "short_code": f.short_code,
                    "rate": f.rate.0,
                    "lines": used(name).lines, "other_uses": used(name).other, "total_cents": total,
                });
                match &f.kind {
                    FringeKind::Percent { cutoff } => {
                        item["fringe_kind"] = json!("percent");
                        item["cutoff_cents"] = json!(cutoff);
                    }
                    FringeKind::Flat {
                        unit,
                        basis,
                        max_hours_per_day,
                        cutoff,
                    } => {
                        item["fringe_kind"] = json!("flat");
                        item["unit"] = json!(unit);
                        item["basis"] = json!(basis.name());
                        item["max_hours_per_day"] = json!(max_hours_per_day.map(|h| h.to_string()));
                        // Kept from before, never used: cutoffs apply to
                        // percent fringes only.
                        if let Some(c) = cutoff {
                            item["unused_cutoff_cents"] = json!(c);
                        }
                    }
                }
                items.push(item);
            }
        }
        SetupKind::Group => {
            for (name, g) in &b.groups {
                let t = report.as_ref().and_then(|r| r.groups.get(name));
                items.push(json!({
                    "name": name, "description": g.description, "short_code": g.short_code,
                    "in_budget_total": g.in_budget_total, "color": g.color,
                    "lines": used(name).lines, "other_uses": used(name).other,
                    "active_total_cents": t.map(|t| t.active.details_total),
                    "active_fringes_cents": t.map(|t| t.active.fringe_total),
                    "full_total_cents": t.map(|t| t.full.details_total),
                    "full_fringes_cents": t.map(|t| t.full.fringe_total),
                }));
            }
        }
        SetupKind::Set => {
            for (name, d) in &b.sets {
                let t = report.as_ref().and_then(|r| r.sets.get(name));
                items.push(json!({
                    "name": name, "description": d.description, "lines": used(name).lines,
                    "total_cents": t.map(|t| t.details_total), "fringes_cents": t.map(|t| t.fringe_total),
                }));
            }
        }
        SetupKind::Location => {
            for (name, l) in &b.locations {
                let t = report.as_ref().and_then(|r| r.locations.get(name));
                items.push(json!({
                    "name": name, "description": l.description, "city": l.city, "region": l.region,
                    "country": l.country, "lines": used(name).lines,
                    "total_cents": t.map(|t| t.details_total), "fringes_cents": t.map(|t| t.fringe_total),
                }));
            }
        }
        SetupKind::Unit => {
            for (name, u) in &b.units {
                items.push(json!({
                    "name": name, "plural": u.plural, "description": u.description,
                    "hours": u.hours.map(|h| h.to_string()),
                    "lines": used(name).lines, "other_uses": used(name).other,
                }));
            }
        }
        SetupKind::Currency => {
            items.push(json!({"name": b.base_currency, "base": true, "rate": "1"}));
            for (code, c) in &b.currencies {
                items.push(json!({
                    "name": code, "base": false, "description": c.description, "symbol": c.symbol,
                    "rate": c.rate.to_string(), "lines": used(code).lines, "other_uses": used(code).other,
                }));
            }
        }
        SetupKind::Global => {}
    }
    Ok(ToolOut {
        summary: format!("{shown}: {} {}(s).", items.len(), kind.name()),
        data: json!({"path": shown, "type": kind.name(), "items": items, "changed_on_disk": v.changed_on_disk}),
    })
}

fn list_charges(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let v = ctx.docs.view(&path, &shown)?;
    let b = v.budget.as_ref();
    let report = report_of(b).ok();
    let charges: Vec<Value> = b
        .charges
        .iter()
        .map(|c| {
            let computed = report
                .as_ref()
                .and_then(|r| r.charges.iter().find(|t| t.name == c.name))
                .filter(|_| c.applied);
            let mut j = json!({
                "name": c.name,
                "applied": c.applied,
                "account": c.account,
                "excluded_groups": c.excluded_groups,
                "excluded_categories": c.excluded_categories,
                "excluded_accounts": c.excluded_accounts,
                "excluded_amounts": c.excluded_amounts.iter().map(|(d, amt)| json!({"description": d, "amount_cents": amt})).collect::<Vec<_>>(),
                "base_cents": computed.map(|t| t.base),
                "excluded_cents": computed.map(|t| t.excluded),
                "amount_cents": computed.map(|t| t.amount),
            });
            match &c.kind {
                ChargeKind::Percent(p) => {
                    j["charge_kind"] = json!("percent");
                    j["percent"] = json!(p.to_string());
                }
                ChargeKind::Flat { amount, currency } => {
                    j["charge_kind"] = json!("flat");
                    j["flat_amount_cents"] = json!(amount);
                    j["currency"] = json!(currency);
                }
            }
            j
        })
        .collect();
    let credits: Vec<Value> = b
        .credits
        .iter()
        .map(|c| {
            let computed = report
                .as_ref()
                .and_then(|r| r.credits.iter().find(|t| t.name == c.name))
                .filter(|_| c.applied);
            let rows: Vec<Value> = c
                .rows
                .iter()
                .map(|row| {
                    let source = match &row.source {
                        CreditSource::Flat(amt) => json!({"kind": "flat", "amount_cents": amt}),
                        CreditSource::Group { group } => json!({"kind": "group", "group": group}),
                        CreditSource::Category { category } => {
                            json!({"kind": "category", "category": category})
                        }
                    };
                    let cap = match row.cap {
                        CreditCap::None => json!({"kind": "none"}),
                        CreditCap::QualifiedSpend(c) => json!({"kind": "qualified_spend", "amount_cents": c}),
                        CreditCap::Credit(c) => json!({"kind": "credit", "amount_cents": c}),
                    };
                    json!({"description": row.description, "source": source, "percent": row.percent.to_string(), "cap": cap})
                })
                .collect();
            json!({
                "name": c.name,
                "applied": c.applied,
                "account": c.account,
                "cap_cents": c.cap,
                "discount_percent": c.discount_percent.to_string(),
                "excluded_fringes": c.excluded_fringes,
                "rows": rows,
                "amount_cents": computed.map(|t| t.amount),
            })
        })
        .collect();
    Ok(ToolOut {
        summary: format!(
            "{shown}: {} charge(s), {} credit(s).",
            charges.len(),
            credits.len()
        ),
        data: json!({"path": shown, "charges": charges, "credits": credits, "changed_on_disk": v.changed_on_disk}),
    })
}

fn totals(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (b, r, shown, changed) = load(ctx, a)?;
    let levels: Vec<Value> = r
        .levels
        .iter()
        .map(|l| {
            let cats: Vec<Value> = r.categories[l.start..l.end]
                .iter()
                .map(|c| {
                    json!({
                        "code": c.code, "name": c.name, "total_cents": c.total,
                        "fringe_total_cents": c.fringe_total,
                        "accounts": c.accounts.iter().map(|a| json!({"code": a.code, "name": a.name, "total_cents": a.total})).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({"name": l.name, "total_cents": l.total, "posted_fringes_cents": l.posted_fringes, "categories": cats})
        })
        .collect();
    let mut data = obj(vec![
        ("path", json!(shown)),
        ("title", json!(b.title)),
        ("base_currency", json!(r.base_currency)),
        ("totals_currency", json!(r.totals_currency)),
        ("levels", json!(levels)),
        (
            "charges",
            json!(r.charges.iter().map(|c| json!({"name": c.name, "base_cents": c.base, "excluded_cents": c.excluded, "amount_cents": c.amount})).collect::<Vec<_>>()),
        ),
        (
            "credits",
            json!(r.credits.iter().map(|c| json!({"name": c.name, "amount_cents": c.amount})).collect::<Vec<_>>()),
        ),
        ("net_total_in_totals_currency_cents", json!(r.in_totals_currency(r.net_total))),
    ]);
    if let Value::Object(h) = headline(&r) {
        data.extend(h);
    }
    note_changed(&mut data, changed);
    Ok(ToolOut {
        summary: crate::cmd::show::topsheet(&b, &r),
        data: Value::Object(data),
    })
}

fn step_json(st: &Step, depth: usize, max_depth: usize, budget: &mut usize) -> Value {
    *budget = budget.saturating_sub(1);
    let mut v = json!({
        "label": st.label,
        "text": st.text,
        "amount_cents": st.amount,
        "role": match st.role { Role::Sum => "sum", Role::Value => "value", Role::Note => "note" },
        "minus": st.minus,
    });
    if st.children.is_empty() {
        return v;
    }
    if depth + 1 >= max_depth || *budget == 0 {
        v["more_steps"] = json!(st.count() - 1);
        return v;
    }
    let kids: Vec<Value> = st
        .children
        .iter()
        .map(|c| step_json(c, depth + 1, max_depth, budget))
        .collect();
    v["children"] = json!(kids);
    v
}

fn explain(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let v = ctx.docs.view(&path, &shown)?;
    let b = v.budget.as_ref();
    let opts = options(a);
    let ex = Explainer::new(b, &opts).map_err(|e| ToolError::refused(format!("{shown}: {e}")))?;
    let what = a.req("total")?;
    let target = Target::parse(b, ex.report(), what).map_err(|e| ToolError::not_found(e.0))?;
    let e = ex
        .explain(&target)
        .map_err(|e| ToolError::refused(format!("{shown}: {e}")))?;
    let depth = a.u64("depth").unwrap_or(4) as usize;
    let mut room = limits::MAX_EXPLAIN_STEPS;
    let tree = step_json(&e.root, 0, depth, &mut room);
    let text = e.to_text(Some(depth));
    let mut summary: String = text.lines().take(200).collect::<Vec<_>>().join("\n");
    if text.lines().count() > 200 {
        summary.push_str("\n...");
    }
    Ok(ToolOut {
        summary,
        data: json!({
            "path": shown,
            "total": what,
            "title": e.title,
            "reported_cents": e.reported,
            "tree": tree,
            "changed_on_disk": v.changed_on_disk,
        }),
    })
}

fn find(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let v = ctx.docs.view(&path, &shown)?;
    let b = v.budget.as_ref();
    let query = a.req("query")?;
    let exact = a.bool("case_sensitive");
    let limit = a.u64("limit").unwrap_or(100) as usize;
    let needle = if exact {
        query.to_string()
    } else {
        query.to_lowercase()
    };
    let hit = |text: &str| {
        if exact {
            text.contains(&needle)
        } else {
            text.to_lowercase().contains(&needle)
        }
    };
    let mut out: Vec<Value> = Vec::new();
    let mut total = 0usize;
    let mut push = |v: Value| {
        total += 1;
        if out.len() < limit {
            out.push(v);
        }
    };
    for c in b.categories() {
        if hit(&c.code) || hit(&c.name) {
            push(json!({"where": "category", "code": c.code, "name": c.name}));
        }
        for acct in &c.accounts {
            if hit(&acct.code) || hit(&acct.name) {
                push(json!({"where": "account", "code": acct.code, "name": acct.name}));
            }
            for (i, d) in acct.details.iter().enumerate() {
                let fields = [
                    ("description", d.description.as_str()),
                    ("amount", d.amount.0.as_str()),
                    ("x", d.x.0.as_str()),
                    ("rate", d.rate.0.as_str()),
                    ("x4", d.x4.0.as_str()),
                ];
                for (field, text) in fields {
                    if !text.is_empty() && hit(text) {
                        push(
                            json!({"where": "line", "account": acct.code, "row": d.id, "n": i + 1, "field": field, "text": text}),
                        );
                    }
                }
                for n in &d.notes {
                    if hit(&n.text) {
                        push(
                            json!({"where": "line", "account": acct.code, "row": d.id, "n": i + 1, "field": "note", "text": n.text}),
                        );
                    }
                }
            }
        }
    }
    let tables: [(&str, Vec<&String>); 7] = [
        ("global", b.globals.keys().collect()),
        ("fringe", b.fringes.keys().collect()),
        ("group", b.groups.keys().collect()),
        ("set", b.sets.keys().collect()),
        ("location", b.locations.keys().collect()),
        ("unit", b.units.keys().collect()),
        ("currency", b.currencies.keys().collect()),
    ];
    for (kind, names) in tables {
        for n in names {
            if hit(n) {
                push(json!({"where": "setup", "type": kind, "name": n}));
            }
        }
    }
    for c in &b.charges {
        if hit(&c.name) {
            push(json!({"where": "charge", "name": c.name}));
        }
    }
    for c in &b.credits {
        if hit(&c.name) {
            push(json!({"where": "credit", "name": c.name}));
        }
    }
    Ok(ToolOut {
        summary: format!(
            "{shown}: {total} match(es) for {}{}.",
            quoted(query),
            if total > out.len() {
                format!(", first {} shown", out.len())
            } else {
                String::new()
            }
        ),
        data: json!({"path": shown, "matches": out, "total_matches": total, "changed_on_disk": v.changed_on_disk}),
    })
}

fn validate(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let v = ctx.docs.view(&path, &shown)?;
    let b = v.budget.as_ref();
    let report = report_of(b);
    let mut warnings: Vec<String> = Vec::new();
    let mut warn = |w: String| {
        if warnings.len() < 200 {
            warnings.push(w);
        }
    };
    let mut seen_cat = std::collections::BTreeSet::new();
    let mut seen_acct = std::collections::BTreeSet::new();
    for c in b.categories() {
        if !seen_cat.insert(c.code.as_str()) {
            warn(format!("category number {} is used more than once", c.code));
        }
        for acct in &c.accounts {
            if !seen_acct.insert(acct.code.as_str()) {
                warn(format!(
                    "account number {} is used more than once",
                    acct.code
                ));
            }
            for (i, d) in acct.details.iter().enumerate() {
                let at = format!("{}:{} (#{})", acct.code, i + 1, d.id);
                for u in [&d.unit, &d.unit2, &d.unit3, &d.unit4] {
                    if !u.is_empty() && !b.units.contains_key(u) {
                        warn(format!("{at}: unit {u} is not in the units table"));
                    }
                }
                if let Some(s) = &d.set {
                    if !b.sets.contains_key(s) {
                        warn(format!("{at}: set {s} is not in the sets table"));
                    }
                }
                if let Some(l) = &d.location {
                    if !b.locations.contains_key(l) {
                        warn(format!("{at}: location {l} is not in the locations table"));
                    }
                }
            }
        }
    }
    // What the totals leave out (decisions.md, entry 56).
    if let Ok(r) = &report {
        for w in method_core::warnings(b, r) {
            warn(w);
        }
    }
    let ok = report.is_ok();
    let summary = match &report {
        Ok(r) => format!(
            "{shown}: ok, net total {}{}.",
            money(r.net_total),
            if warnings.is_empty() {
                String::new()
            } else {
                format!(", {} warning(s)", warnings.len())
            }
        ),
        Err(e) => format!("{shown}: doesn't add up: {e}"),
    };
    Ok(ToolOut {
        summary,
        data: json!({
            "path": shown,
            "ok": ok,
            "problem": report.as_ref().err(),
            "net_total_cents": report.as_ref().ok().map(|r| r.net_total),
            "warnings": warnings,
            "changed_on_disk": v.changed_on_disk,
        }),
    })
}
