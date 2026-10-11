//! Tools that change detail rows: add, update, delete and move them, and put
//! fringes and groups on them or take them off. Field values go through the
//! command line's own edit language (`script::apply`), so a unit, set,
//! location or currency must exist, a color must be `#rrggbb` and so on,
//! exactly as `method set` checks them.

use std::collections::BTreeSet;

use method_core::{Budget, Detail, NewLineDefaults, Note, RowId, RowKind};
use method_format::serde_json::{json, Map, Value};

use super::read::{find_account, kind_name};
use super::{budget_path, edited, obj, path_prop, Args, Ctx, Tool, ToolError, ToolResult};
use crate::mcp::limits;
use crate::mcp::schema::{self as s, object};
use crate::script::{self, Op, RowRef, Word};

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "add_lines",
            title: "Add lines",
            description: "Adds rows to an account, in order, after a row (after_row), at the \
                start (at_start) or at the end. Each row may set any field; kind is line \
                (default), subtotal, adjustment or separator. A new line copies the tags the \
                budget's new-line defaults ask for (fringes, by default) from the nearest line \
                above it unless the row gives them or use_defaults is false. Returns the new row ids.",
            mutating: true,
            destructive: false,
            schema: || {
                let mut props = line_props();
                props.push((
                    "kind",
                    s::one_of("Row kind (default line).", &["line", "subtotal", "adjustment", "separator"]),
                ));
                props.push(("note", s::text("A note to put on the row.")));
                object(
                    vec![
                        path_prop(),
                        ("account", s::name("The account number (21-01).")),
                        ("after_row", s::integer("Insert after this row id.", 1, i64::MAX)),
                        ("at_start", s::boolean("Insert at the top of the account.")),
                        (
                            "use_defaults",
                            s::boolean("Copy tags from the line above as the budget's new-line defaults say (default true)."),
                        ),
                        ("lines", rows_schema(object(props, &[]))),
                    ],
                    &["path", "account", "lines"],
                )
            },
            run: add_lines,
        },
        Tool {
            name: "update_lines",
            title: "Update lines",
            description: "Changes fields of rows by id. Give only the fields to change; null \
                clears an optional one. fringes and groups replace the row's whole list (use \
                tag_lines to add or remove one). add_note appends a dated note.",
            mutating: true,
            destructive: false,
            schema: || {
                let mut props = line_props();
                props.insert(0, ("id", s::integer("The row id.", 1, i64::MAX)));
                props.push(("add_note", s::text("A note to add to the row.")));
                object(
                    vec![path_prop(), ("rows", rows_schema(object(props, &["id"])))],
                    &["path", "rows"],
                )
            },
            run: update_lines,
        },
        Tool {
            name: "delete_lines",
            title: "Delete lines",
            description: "Deletes rows by id (all of them, or none when one is missing).",
            mutating: true,
            destructive: true,
            schema: || {
                object(
                    vec![path_prop(), ("rows", ids_schema("The row ids to delete."))],
                    &["path", "rows"],
                )
            },
            run: delete_lines,
        },
        Tool {
            name: "move_lines",
            title: "Move lines",
            description: "Moves rows (in the order given) to an account, after a row there, at \
                its start, or at its end. Rows keep their ids and fields.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("rows", ids_schema("The row ids to move.")),
                        ("to_account", s::name("The account to move them to.")),
                        ("after_row", s::integer("Put them after this row id (not one being moved).", 1, i64::MAX)),
                        ("at_start", s::boolean("Put them at the top of the account.")),
                    ],
                    &["path", "rows", "to_account"],
                )
            },
            run: move_lines,
        },
        Tool {
            name: "tag_lines",
            title: "Tag lines",
            description: "Puts fringes and groups on rows or takes them off: rows by id, or every \
                line of an account. A fringe or group must exist to be added. Fringes go on lines \
                only; groups on lines and adjustments.",
            mutating: true,
            destructive: false,
            schema: || {
                let names = |d: &str| s::array(d, s::name("A name."), 100);
                object(
                    vec![
                        path_prop(),
                        ("rows", ids_schema("Row ids.")),
                        ("account", s::name("Every line and adjustment of this account.")),
                        ("add_fringes", names("Fringes to put on.")),
                        ("remove_fringes", names("Fringes to take off.")),
                        ("add_groups", names("Groups to put on.")),
                        ("remove_groups", names("Groups to take off.")),
                    ],
                    &["path"],
                )
            },
            run: tag_lines,
        },
    ]
}

/// The fields a row may set, as schema properties.
fn line_props() -> Vec<(&'static str, Value)> {
    let unit = |d: &str| s::nullable(s::name(d));
    vec![
        ("description", s::text("Description.")),
        ("amount", s::formula("Amount (a number, a global or a formula); blank counts as 0.")),
        ("unit", unit("Unit of the amount (a unit in the budget: Day, Week, Flat...).")),
        ("x", s::formula("X multiplier; blank counts as 1.")),
        ("unit2", unit("Unit of X.")),
        ("rate", s::formula("Rate; blank counts as 0.")),
        ("unit3", unit("Unit of the rate.")),
        ("x4", s::formula("Fourth multiplier; blank counts as 1.")),
        ("unit4", unit("Unit of the fourth multiplier.")),
        ("currency", s::nullable(s::name("Currency code (one in the budget); null for the base currency."))),
        ("set", s::nullable(s::name("Set (one in the budget), or null."))),
        ("location", s::nullable(s::name("Location (one in the budget), or null."))),
        (
            "fringe_range",
            s::nullable(s::integer("Fringe range number: neighboring lines with the same number share percent fringe cutoffs as one earner.", 0, i64::from(u32::MAX))),
        ),
        ("bold", s::boolean("Bold text.")),
        ("italic", s::boolean("Italic text.")),
        ("indent", s::integer("Indent, 0 to 4.", 0, 4)),
        ("color", s::nullable(s::name("Text color as #rrggbb, or null."))),
        ("fringes", s::array("Fringes on the row (replaces the list).", s::name("A fringe name."), 100)),
        ("groups", s::array("Groups on the row (replaces the list).", s::name("A group name."), 100)),
    ]
}

fn rows_schema(item: Value) -> Value {
    let mut v = s::array("The rows.", item, limits::MAX_BATCH);
    v["minItems"] = json!(1);
    v
}

fn ids_schema(d: &str) -> Value {
    let mut v = s::array(d, s::integer("A row id.", 1, i64::MAX), limits::MAX_BATCH);
    v["minItems"] = json!(1);
    v
}

/// The script fields, in the order they are applied.
const SCRIPT_FIELDS: &[&str] = &[
    "description",
    "amount",
    "unit",
    "x",
    "unit2",
    "rate",
    "unit3",
    "x4",
    "unit4",
    "currency",
    "set",
    "location",
    "fringe_range",
    "bold",
    "italic",
    "indent",
    "color",
];

/// Whether a row of `kind` uses `field` (as the app's grid decides).
fn applies(kind: RowKind, field: &str) -> bool {
    const STYLE: &[&str] = &["bold", "italic", "indent", "color"];
    match kind {
        RowKind::Line => true,
        RowKind::Subtotal => field == "description" || STYLE.contains(&field),
        RowKind::Adjustment => !matches!(field, "rate" | "unit3" | "currency" | "fringes"),
        RowKind::Separator => STYLE.contains(&field),
    }
}

fn word(v: &Value) -> Word {
    match v {
        Value::Null => Word::plain(""),
        Value::String(s) => Word {
            text: s.clone(),
            quoted: true,
        },
        Value::Bool(b) => Word::plain(b.to_string()),
        other => Word::plain(other.to_string()),
    }
}

fn row(b: &Budget, id: RowId) -> Option<&Detail> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == id)
}

fn row_mut(b: &mut Budget, id: RowId) -> Option<&mut Detail> {
    b.categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
        .find(|d| d.id == id)
}

fn names(v: &Value) -> BTreeSet<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(|s| s.trim().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The time now as ISO 8601, for a note's date.
fn now() -> String {
    let (date, time) = crate::render::now_utc();
    format!("{date}T{}:00Z", time.trim_end_matches(" UTC"))
}

/// Sets every field `spec` gives on row `id`.
fn apply_fields(
    b: &mut Budget,
    id: RowId,
    spec: &Map<String, Value>,
    log: &mut Vec<String>,
) -> Result<(), ToolError> {
    let kind = row(b, id)
        .ok_or_else(|| ToolError::not_found(format!("there is no row #{id}")))?
        .kind;
    for key in spec.keys() {
        let known =
            SCRIPT_FIELDS.contains(&key.as_str()) || matches!(key.as_str(), "fringes" | "groups");
        if known && !applies(kind, key) {
            return Err(ToolError::invalid(format!(
                "row #{id} is a {} row, which has no {key}",
                kind_name(kind)
            )));
        }
    }
    for field in SCRIPT_FIELDS {
        if let Some(v) = spec.get(*field) {
            let op = Op::Line(RowRef::Id(id), field.to_string(), word(v));
            let lines =
                script::apply(b, &op).map_err(|e| ToolError::invalid(format!("row #{id}: {e}")))?;
            log.extend(lines);
        }
    }
    for (key, group) in [("fringes", false), ("groups", true)] {
        let Some(v) = spec.get(key) else { continue };
        let want = names(v);
        for n in &want {
            let exists = if group {
                b.groups.contains_key(n)
            } else {
                b.fringes.contains_key(n)
            };
            if !exists {
                let what = if group { "group" } else { "fringe" };
                return Err(ToolError::not_found(format!(
                    "row #{id}: there is no {what} {n}"
                )));
            }
        }
        let d = row_mut(b, id).expect("checked");
        let slot = if group { &mut d.groups } else { &mut d.fringes };
        if *slot != want {
            log.push(format!(
                "#{id} {key} -> {}",
                want.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
            *slot = want;
        }
    }
    for key in ["note", "add_note"] {
        if let Some(text) = spec.get(key).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                let d = row_mut(b, id).expect("checked");
                let nid = method_core::next_note_id(&d.notes);
                d.notes.push(Note {
                    id: nid,
                    text: text.to_string(),
                    created: now(),
                    edited: None,
                    author: None,
                    done: false,
                });
                log.push(format!("#{id} note added"));
            }
        }
    }
    Ok(())
}

/// Gives a new line the tags `defaults` asks for from the nearest line
/// above `at` (below, when there is none above), skipping fields `spec`
/// sets itself. As the app's new-line rule does.
fn copy_from_neighbor(
    new: &mut Detail,
    rows: &[Detail],
    at: usize,
    defaults: NewLineDefaults,
    spec: &Map<String, Value>,
) {
    let at = at.min(rows.len());
    let is_line = |d: &&Detail| d.kind == RowKind::Line;
    let Some(from) = rows[..at]
        .iter()
        .rev()
        .find(is_line)
        .or_else(|| rows[at..].iter().find(is_line))
    else {
        return;
    };
    if defaults.fringes && !spec.contains_key("fringes") {
        new.fringes = from.fringes.clone();
    }
    if defaults.groups && !spec.contains_key("groups") {
        new.groups = from.groups.clone();
    }
    if defaults.set && !spec.contains_key("set") {
        new.set = from.set.clone();
    }
    if defaults.location && !spec.contains_key("location") {
        new.location = from.location.clone();
    }
    if defaults.currency && !spec.contains_key("currency") {
        new.currency = from.currency.clone();
    }
}

fn parse_kind(spec: &Map<String, Value>) -> RowKind {
    match spec.get("kind").and_then(Value::as_str) {
        Some("subtotal") => RowKind::Subtotal,
        Some("adjustment") => RowKind::Adjustment,
        Some("separator") => RowKind::Separator,
        _ => RowKind::Line,
    }
}

fn where_to(
    after: Option<u64>,
    at_start: bool,
    rows: &[Detail],
    account: &str,
) -> Result<usize, ToolError> {
    match (after, at_start) {
        (Some(_), true) => Err(ToolError::invalid("give after_row or at_start, not both")),
        (Some(id), false) => rows
            .iter()
            .position(|d| d.id == id)
            .map(|i| i + 1)
            .ok_or_else(|| ToolError::not_found(format!("row #{id} is not in account {account}"))),
        (None, true) => Ok(0),
        (None, false) => Ok(rows.len()),
    }
}

fn add_lines(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let account = a.req("account")?.to_string();
    let specs: Vec<Map<String, Value>> = a
        .list("lines")
        .iter()
        .filter_map(|v| v.as_object().cloned())
        .collect();
    let after = a.u64("after_row");
    let at_start = a.bool("at_start");
    let use_defaults = a.opt_bool("use_defaults").unwrap_or(true);
    let label = format!("Add {} row(s) to {account}", specs.len());
    let e = ctx.docs.edit(&path, &shown, &label, |b| {
        let (ci, ai) = find_account(b, &account)
            .ok_or_else(|| ToolError::not_found(format!("there is no account {account}")))?;
        let first = b
            .highest_row_id()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| ToolError::refused("row ids are used up"))?;
        let defaults = b.new_line_defaults;
        let acct = &b.categories().nth(ci).expect("found").accounts[ai];
        let start = where_to(after, at_start, &acct.details, &account)?;
        let mut ids = Vec::with_capacity(specs.len());
        let mut log = Vec::new();
        // One row at a time, fields and all, so a row later in the batch
        // copies its tags from the rows before it.
        for (i, spec) in specs.iter().enumerate() {
            let id = first
                .checked_add(i as RowId)
                .ok_or_else(|| ToolError::refused("row ids are used up"))?;
            let at = start + i;
            let kind = parse_kind(spec);
            let mut new = match kind {
                RowKind::Line => Detail::line(id, "", "", "", ""),
                RowKind::Subtotal => Detail::subtotal(id, ""),
                RowKind::Adjustment => Detail::adjustment(id, "", ""),
                RowKind::Separator => Detail::separator(id),
            };
            let acct = &mut b.categories_mut().nth(ci).expect("found").accounts[ai];
            if kind == RowKind::Line && use_defaults {
                copy_from_neighbor(&mut new, &acct.details, at, defaults, spec);
            }
            acct.details.insert(at, new);
            apply_fields(b, id, spec, &mut log)?;
            ids.push(id);
        }
        let acct = &b.categories().nth(ci).expect("found").accounts[ai];
        let added: Vec<Value> = ids
            .iter()
            .map(|id| {
                let n = acct
                    .details
                    .iter()
                    .position(|d| d.id == *id)
                    .map_or(0, |i| i + 1);
                json!({"id": id, "n": n})
            })
            .collect();
        Ok(added)
    })?;
    let what = format!("Added {} row(s) to {account}", e.out.len());
    let extra = obj(vec![("account", json!(account)), ("added", json!(e.out))]);
    Ok(edited(&shown, &e, &what, extra))
}

fn update_lines(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let specs: Vec<Map<String, Value>> = a
        .list("rows")
        .iter()
        .filter_map(|v| v.as_object().cloned())
        .collect();
    let label = format!("Edit {} row(s)", specs.len());
    let e = ctx.docs.edit(&path, &shown, &label, |b| {
        let mut log = Vec::new();
        for spec in &specs {
            let id = spec.get("id").and_then(Value::as_u64).unwrap_or(0);
            apply_fields(b, id, spec, &mut log)?;
        }
        Ok(log)
    })?;
    let what = format!("Updated {} row(s)", specs.len());
    let mut changes = e.out.clone();
    changes.truncate(500);
    let extra = obj(vec![("changes", json!(changes))]);
    Ok(edited(&shown, &e, &what, extra))
}

fn ids(a: &Args, key: &str) -> Vec<RowId> {
    a.list(key).iter().filter_map(Value::as_u64).collect()
}

fn check_ids(b: &Budget, want: &[RowId]) -> Result<(), ToolError> {
    let have: BTreeSet<RowId> = b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .map(|d| d.id)
        .collect();
    let missing: Vec<String> = want
        .iter()
        .filter(|id| !have.contains(id))
        .map(|id| format!("#{id}"))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(ToolError::not_found(format!(
            "no such row(s): {}; nothing was changed",
            missing.join(", ")
        )))
    }
}

fn delete_lines(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let want = ids(a, "rows");
    let label = format!("Delete {} row(s)", want.len());
    let e = ctx.docs.edit(&path, &shown, &label, |b| {
        check_ids(b, &want)?;
        let set: BTreeSet<RowId> = want.iter().copied().collect();
        let mut removed = 0;
        for acct in b.categories_mut().flat_map(|c| &mut c.accounts) {
            let before = acct.details.len();
            acct.details.retain(|d| !set.contains(&d.id));
            removed += before - acct.details.len();
        }
        Ok(removed)
    })?;
    let what = format!("Deleted {} row(s)", e.out);
    Ok(edited(
        &shown,
        &e,
        &what,
        obj(vec![("deleted", json!(e.out))]),
    ))
}

fn move_lines(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let want = ids(a, "rows");
    let to = a.req("to_account")?.to_string();
    let after = a.u64("after_row");
    let at_start = a.bool("at_start");
    let label = format!("Move {} row(s) to {to}", want.len());
    let e = ctx.docs.edit(&path, &shown, &label, |b| {
        check_ids(b, &want)?;
        let set: BTreeSet<RowId> = want.iter().copied().collect();
        if set.len() != want.len() {
            return Err(ToolError::invalid("a row id is given twice"));
        }
        if after.is_some_and(|id| set.contains(&id)) {
            return Err(ToolError::invalid(
                "after_row can't be one of the rows being moved",
            ));
        }
        let (ci, ai) = find_account(b, &to)
            .ok_or_else(|| ToolError::not_found(format!("there is no account {to}")))?;
        let mut taken: Vec<Detail> = Vec::new();
        for acct in b.categories_mut().flat_map(|c| &mut c.accounts) {
            let (out, keep): (Vec<Detail>, Vec<Detail>) = std::mem::take(&mut acct.details)
                .into_iter()
                .partition(|d| set.contains(&d.id));
            acct.details = keep;
            taken.extend(out);
        }
        taken.sort_by_key(|d| want.iter().position(|id| *id == d.id));
        let acct = &mut b.categories_mut().nth(ci).expect("found").accounts[ai];
        let at = where_to(after, at_start, &acct.details, &to)?;
        let n = taken.len();
        for (i, d) in taken.into_iter().enumerate() {
            acct.details.insert(at + i, d);
        }
        Ok(n)
    })?;
    let what = format!("Moved {} row(s) to {to}", e.out);
    Ok(edited(
        &shown,
        &e,
        &what,
        obj(vec![("moved", json!(e.out)), ("to_account", json!(to))]),
    ))
}

fn tag_lines(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let mut want = ids(a, "rows");
    let account = a.str("account").map(str::to_string);
    if want.is_empty() && account.is_none() {
        return Err(ToolError::invalid("give rows (ids) or an account"));
    }
    let lists = [
        (a.strings("add_fringes"), true, false),
        (a.strings("remove_fringes"), false, false),
        (a.strings("add_groups"), true, true),
        (a.strings("remove_groups"), false, true),
    ];
    if lists.iter().all(|(l, _, _)| l.is_empty()) {
        return Err(ToolError::invalid(
            "give fringes or groups to add or remove",
        ));
    }
    let e = ctx.docs.edit(&path, &shown, "Tag rows", |b| {
        check_ids(b, &want)?;
        if let Some(code) = &account {
            let (ci, ai) = find_account(b, code)
                .ok_or_else(|| ToolError::not_found(format!("there is no account {code}")))?;
            let acct = &b.categories().nth(ci).expect("found").accounts[ai];
            want.extend(
                acct.details
                    .iter()
                    .filter(|d| d.kind.is_costed())
                    .map(|d| d.id),
            );
        }
        for (names, add, group) in &lists {
            if !add {
                continue;
            }
            for n in names {
                let exists = if *group {
                    b.groups.contains_key(n)
                } else {
                    b.fringes.contains_key(n)
                };
                if !exists {
                    let what = if *group { "group" } else { "fringe" };
                    return Err(ToolError::not_found(format!("there is no {what} {n}")));
                }
            }
        }
        let mut changed = BTreeSet::new();
        let mut skipped = BTreeSet::new();
        for id in &want {
            let d = row_mut(b, *id).expect("checked");
            for (names, add, group) in &lists {
                if names.is_empty() {
                    continue;
                }
                let ok = if *group {
                    d.kind.is_costed()
                } else {
                    d.kind == RowKind::Line
                };
                if !ok {
                    skipped.insert(*id);
                    continue;
                }
                let set = if *group {
                    &mut d.groups
                } else {
                    &mut d.fringes
                };
                for n in names {
                    let did = if *add {
                        set.insert(n.clone())
                    } else {
                        set.remove(n)
                    };
                    if did {
                        changed.insert(*id);
                    }
                }
            }
        }
        Ok((changed.len(), skipped.len()))
    })?;
    let (n, skipped) = e.out;
    let what = format!("Tagged {n} row(s)");
    let extra = obj(vec![
        ("rows_changed", json!(n)),
        ("rows_skipped", json!(skipped)),
    ]);
    Ok(edited(&shown, &e, &what, extra))
}
