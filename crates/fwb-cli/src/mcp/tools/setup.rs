//! Tools that change the budget's setup: globals, the setup tables (fringes,
//! groups, sets, locations, units, currencies), charges and credits,
//! categories and accounts, and the title. Renames go through the engine's
//! rename, which rewrites every reference (decision 32); deletes check how
//! much an item is used first (`fwb_core::usage`) and, only when asked to,
//! take it off what uses it as the app does.

use std::collections::BTreeSet;
use std::str::FromStr;

use fwb_core::rename::{rename, valid_name, SetupKind};
use fwb_core::{
    Account, Budget, Category, Cents, Charge, ChargeKind, Credit, CreditCap, CreditRow,
    CreditSource, Currency, Dec, Formula, Fringe, FringeKind, Group, Location, SetDef, TopsheetRow,
    UnitDef,
};
use fwb_format::serde_json::{json, Map, Value};

use super::read::setup_kind;
use super::{
    budget_path, edited, formula_text, obj, path_prop, Args, Ctx, Tool, ToolError, ToolResult,
};
use crate::mcp::schema::{self as s, object};
use crate::script::{self, Op};

const TABLES: &[&str] = &["fringe", "group", "set", "location", "unit", "currency"];
const TABLES_AND_GLOBAL: &[&str] = &[
    "global", "fringe", "group", "set", "location", "unit", "currency",
];

/// Longest short code, in characters (as the app allows).
const MAX_SHORT_CODE: usize = 16;

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "set_global",
            title: "Set a global",
            description: "Sets a global's formula (adding the global when it is new), and \
                optionally its description and unit. Names are letters, digits and _, starting \
                with a letter. Lines use globals by name in their formulas (SHOOT_DAYS * 2).",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("name", s::name("The global's name.")),
                        ("formula", s::formula("Its value: a number or a formula of other globals.")),
                        ("description", s::text("What it is.")),
                        ("unit", s::nullable(s::name("A unit in the budget it is counted in, or null."))),
                    ],
                    &["path", "name", "formula"],
                )
            },
            run: set_global,
        },
        Tool {
            name: "add_setup",
            title: "Add a setup item",
            description: "Adds a fringe, group, set, location, unit or currency. fields by type: \
                fringe {fringe_kind percent|flat, rate, cutoff (percent), unit (flat), \
                description, short_code}; group {in_budget_total, description, short_code, \
                color}; set {description}; location {description, city, region, country}; unit \
                {plural, description, hours}; currency {rate (base units per one of it), \
                description, symbol}.",
            mutating: true,
            destructive: false,
            schema: || setup_schema(false),
            run: add_setup,
        },
        Tool {
            name: "update_setup",
            title: "Update a setup item",
            description: "Changes fields of a fringe, group, set, location, unit or currency (the \
                same fields as add_setup; give only those to change, null clears an optional one). \
                Use rename_setup to rename.",
            mutating: true,
            destructive: false,
            schema: || setup_schema(true),
            run: update_setup,
        },
        Tool {
            name: "rename_setup",
            title: "Rename a setup item",
            description: "Renames a global, fringe, group, set, location, unit or currency and \
                rewrites every reference to it (formulas, lines, charges, credits, presets), as the \
                app's rename does. Totals never change.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("type", s::one_of("What to rename.", TABLES_AND_GLOBAL)),
                        ("from", s::name("The current name.")),
                        ("to", s::name("The new name.")),
                    ],
                    &["path", "type", "from", "to"],
                )
            },
            run: rename_setup,
        },
        Tool {
            name: "delete_setup",
            title: "Delete a setup item",
            description: "Deletes a global, fringe, group, set, location, unit or currency. An item \
                still in use is refused with where it is used, unless force is true: then a global's \
                value is frozen into the formulas that used it (no total changes), a fringe, group, \
                set or location is taken off its lines (totals change), a unit is blanked on lines, \
                and lines in a currency go back to the base currency. A group a credit is based on, \
                and a currency the totals or a charge use, are always refused.",
            mutating: true,
            destructive: true,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("type", s::one_of("What to delete.", TABLES_AND_GLOBAL)),
                        ("name", s::name("Its name.")),
                        ("force", s::boolean("Delete it even though it is used (see above).")),
                    ],
                    &["path", "type", "name"],
                )
            },
            run: delete_setup,
        },
        Tool {
            name: "set_charge",
            title: "Add or change a charge",
            description: "Adds a contractual charge (by name), or changes the one with that name: \
                percent of above- plus below-the-line, or a flat fee, with exclusions (groups, \
                categories, accounts, amounts). Give only what to change; a new charge needs \
                charge_kind and its percent or amount. New charges are applied unless applied is false.",
            mutating: true,
            destructive: false,
            schema: || {
                let names = |d: &str| s::array(d, s::name("A name or number."), 500);
                object(
                    vec![
                        path_prop(),
                        ("name", s::name("The charge's name.")),
                        ("new_name", s::name("Rename it to this.")),
                        ("charge_kind", s::one_of("percent or flat.", &["percent", "flat"])),
                        ("percent", s::formula("Percent charges: 10 means 10%.")),
                        ("amount", money_schema("Flat charges: the fee (1500.00).")),
                        ("currency", s::nullable(s::name("Flat charges: the fee's currency, null for the base."))),
                        ("account", s::nullable(s::name("The account it posts to, or null."))),
                        ("applied", s::boolean("Whether it counts.")),
                        ("excluded_groups", names("Groups whose lines it leaves out (replaces the list).")),
                        ("excluded_categories", names("Categories it leaves out (replaces the list).")),
                        ("excluded_accounts", names("Accounts it leaves out (replaces the list).")),
                        (
                            "excluded_amounts",
                            s::array(
                                "Amounts it leaves out (replaces the list).",
                                object(
                                    vec![("description", s::text("What it is.")), ("amount", money_schema("The amount."))],
                                    &["amount"],
                                ),
                                100,
                            ),
                        ),
                        ("label_percent", s::boolean("Show the percent in the topsheet label.")),
                        ("label_excluded", s::boolean("Show the excluded total in the topsheet label.")),
                    ],
                    &["path", "name"],
                )
            },
            run: set_charge,
        },
        Tool {
            name: "set_credit",
            title: "Add or change a credit",
            description: "Adds a credit (an incentive, by name) or changes the one with that name: \
                rows that each take a percent of a flat amount or of a group's lines (less \
                excluded fringes), with caps, an overall cap and a discount. rows replaces every \
                row when given. New credits are applied unless applied is false.",
            mutating: true,
            destructive: false,
            schema: || {
                let source = object(
                    vec![
                        ("kind", s::one_of("flat or group.", &["flat", "group"])),
                        ("amount", money_schema("flat: the gross amount.")),
                        ("group", s::name("group: the group whose lines count.")),
                        ("excluded_fringes", s::array("group: fringes not counted.", s::name("A fringe."), 100)),
                    ],
                    &["kind"],
                );
                let cap = object(
                    vec![
                        ("kind", s::one_of("none, qualified_spend (caps the spend) or credit (caps the credit).", &["none", "qualified_spend", "credit"])),
                        ("amount", money_schema("The cap.")),
                    ],
                    &["kind"],
                );
                let row = object(
                    vec![
                        ("description", s::text("What it is.")),
                        ("source", source),
                        ("percent", s::formula("25 means 25% of the row's net spend.")),
                        ("cap", cap),
                    ],
                    &["source", "percent"],
                );
                object(
                    vec![
                        path_prop(),
                        ("name", s::name("The credit's name.")),
                        ("new_name", s::name("Rename it to this.")),
                        ("account", s::nullable(s::name("The account it posts to, or null."))),
                        ("applied", s::boolean("Whether it counts.")),
                        ("cap", s::nullable(money_schema("Ceiling on the sum of the rows' credits, or null."))),
                        ("discount_percent", s::formula("Taken off after the cap (5 means 5%).")),
                        ("rows", s::array("The credit's rows (replaces them all).", row, 200)),
                    ],
                    &["path", "name"],
                )
            },
            run: set_credit,
        },
        Tool {
            name: "delete_charge",
            title: "Delete a charge or credit",
            description: "Deletes a charge or a credit by name. (apply_charge with applied: false \
                keeps it but stops it counting.)",
            mutating: true,
            destructive: true,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("type", s::one_of("charge or credit.", &["charge", "credit"])),
                        ("name", s::name("Its name.")),
                    ],
                    &["path", "type", "name"],
                )
            },
            run: delete_charge,
        },
        Tool {
            name: "apply_charge",
            title: "Apply a charge or credit",
            description: "Makes a charge or credit count (applied: true) or stop counting (false).",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("type", s::one_of("charge or credit.", &["charge", "credit"])),
                        ("name", s::name("Its name.")),
                        ("applied", s::boolean("Whether it counts.")),
                    ],
                    &["path", "type", "name", "applied"],
                )
            },
            run: apply_charge,
        },
        Tool {
            name: "add_category",
            title: "Add a category",
            description: "Adds a topsheet category (number normalized by the budget's account \
                numbering), after a category, just above a production total (before_total), at \
                the start, or at the end.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("code", s::name("The category number (21).")),
                        ("name", s::text("Its name.")),
                        ("after_category", s::name("Put it after this category.")),
                        ("before_total", s::name("Put it just above this production total.")),
                        ("at_start", s::boolean("Put it first.")),
                    ],
                    &["path", "code", "name"],
                )
            },
            run: add_category,
        },
        Tool {
            name: "add_production_total",
            title: "Add a production total",
            description: "Adds a production total row to the topsheet (it subtotals the categories \
                since the previous one; the first one ends above-the-line), after a category or at \
                the end.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("name", s::name("Its name (Total above-the-line).")),
                        ("after_category", s::name("Put it after this category (default: at the end).")),
                    ],
                    &["path", "name"],
                )
            },
            run: add_production_total,
        },
        Tool {
            name: "delete_production_total",
            title: "Delete a production total",
            description: "Deletes a production total row by name. Its categories stay; they join \
                the next level.",
            mutating: true,
            destructive: true,
            schema: || object(vec![path_prop(), ("name", s::name("Its name."))], &["path", "name"]),
            run: delete_production_total,
        },
        Tool {
            name: "add_account",
            title: "Add an account",
            description: "Adds an account to a category (number normalized by the budget's account \
                numbering, unique), after an account there or at the end.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("category", s::name("The category's number.")),
                        ("code", s::name("The account number (21-05).")),
                        ("name", s::text("Its name.")),
                        ("after_account", s::name("Put it after this account.")),
                    ],
                    &["path", "category", "code", "name"],
                )
            },
            run: add_account,
        },
        Tool {
            name: "update_category",
            title: "Rename a category",
            description: "Changes a category's name or number. Its accounts stay as they are.",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("code", s::name("The category's number now.")),
                        ("new_code", s::name("Its new number.")),
                        ("name", s::text("Its new name.")),
                    ],
                    &["path", "code"],
                )
            },
            run: update_category,
        },
        Tool {
            name: "update_account",
            title: "Rename an account",
            description: "Changes an account's name or number (unique, normalized by the budget's \
                numbering).",
            mutating: true,
            destructive: false,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("code", s::name("The account's number now.")),
                        ("new_code", s::name("Its new number.")),
                        ("name", s::text("Its new name.")),
                    ],
                    &["path", "code"],
                )
            },
            run: update_account,
        },
        Tool {
            name: "delete_category",
            title: "Delete a category",
            description: "Deletes a category. One whose accounts have rows is refused unless force \
                is true (then its accounts and rows go with it).",
            mutating: true,
            destructive: true,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("code", s::name("The category's number.")),
                        ("force", s::boolean("Delete it with its accounts and rows.")),
                    ],
                    &["path", "code"],
                )
            },
            run: delete_category,
        },
        Tool {
            name: "delete_account",
            title: "Delete an account",
            description: "Deletes an account. One with rows is refused unless force is true (then \
                its rows go with it).",
            mutating: true,
            destructive: true,
            schema: || {
                object(
                    vec![
                        path_prop(),
                        ("code", s::name("The account's number.")),
                        ("force", s::boolean("Delete it with its rows.")),
                    ],
                    &["path", "code"],
                )
            },
            run: delete_account,
        },
        Tool {
            name: "set_title",
            title: "Set the title",
            description: "Changes the budget's title.",
            mutating: true,
            destructive: false,
            schema: || object(vec![path_prop(), ("title", s::text("The new title."))], &["path", "title"]),
            run: set_title,
        },
    ]
}

fn money_schema(d: &str) -> Value {
    json!({"type": ["string", "number"], "description": d, "maxLength": 40})
}

fn setup_schema(update: bool) -> Value {
    let fields = object(
        vec![
            ("description", s::text("Description (every type).")),
            ("short_code", s::nullable(s::name("fringe, group: short code shown in the grid."))),
            ("fringe_kind", s::one_of("fringe: percent (of each earner's pay) or flat (per line).", &["percent", "flat"])),
            ("rate", s::formula("fringe: percent (7.65) or flat amount, may use globals; currency: base units per one unit.")),
            ("cutoff", s::nullable(money_schema("fringe (percent): pay per earner it stops at (168600.00), or null."))),
            ("unit", s::nullable(s::name("fringe (flat): charge per this unit (scaled by hours), or null."))),
            ("in_budget_total", s::boolean("group: whether its lines count (false suppresses them).")),
            ("color", s::nullable(s::name("group: text color #rrggbb, or null."))),
            ("city", s::text("location: city.")),
            ("region", s::text("location: region.")),
            ("country", s::text("location: country.")),
            ("plural", s::text("unit: plural name.")),
            ("hours", s::nullable(json!({"type": ["string", "number"], "description": "unit: hours in one of it, or null.", "maxLength": 40}))),
            ("symbol", s::text("currency: symbol.")),
        ],
        &[],
    );
    let required: &[&str] = if update {
        &["path", "type", "name", "fields"]
    } else {
        &["path", "type", "name"]
    };
    object(
        vec![
            path_prop(),
            ("type", s::one_of("The table.", TABLES)),
            ("name", s::name("The item's name (a currency's code).")),
            ("fields", fields),
        ],
        required,
    )
}

// ---------------------------------------------------------------------------
// Values

fn dec(v: &Value, what: &str) -> Result<Dec, ToolError> {
    let t = formula_text(v);
    Dec::from_str(t.trim())
        .map_err(|_| ToolError::invalid(format!("{what}: `{t}` is not a number")))
}

fn cents(v: &Value, what: &str) -> Result<Cents, ToolError> {
    let t = formula_text(v);
    let clean: String = t
        .trim()
        .trim_start_matches('$')
        .chars()
        .filter(|c| *c != ',')
        .collect();
    let d = Dec::from_str(&clean)
        .map_err(|_| ToolError::invalid(format!("{what}: `{t}` is not an amount")))?;
    let c = d
        .to_cents()
        .ok_or_else(|| ToolError::invalid(format!("{what}: `{t}` is too large")))?;
    if Dec::from_cents(c) != d {
        return Err(ToolError::invalid(format!(
            "{what}: `{t}` has more than two decimal places"
        )));
    }
    if c < 0 {
        return Err(ToolError::invalid(format!("{what} can't be negative")));
    }
    Ok(c)
}

fn text(f: &Map<String, Value>, k: &str) -> Option<String> {
    f.get(k)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
}

/// An optional name field: `Some(None)` when given as null or blank.
fn opt_text(f: &Map<String, Value>, k: &str) -> Option<Option<String>> {
    f.get(k).map(|v| {
        v.as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    })
}

fn color(v: Option<String>) -> Result<Option<String>, ToolError> {
    match v {
        None => Ok(None),
        Some(c) => {
            let ok =
                c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit());
            if ok {
                Ok(Some(c.to_ascii_lowercase()))
            } else {
                Err(ToolError::invalid(format!(
                    "`{c}` is not a color: use #rrggbb"
                )))
            }
        }
    }
}

/// A short code for item `name` of `kind`, refused when it has a comma, is
/// too long, or is another item's code (ignoring case) or name.
fn short_code(
    b: &Budget,
    kind: SetupKind,
    name: &str,
    typed: Option<String>,
) -> Result<Option<String>, ToolError> {
    let Some(code) = typed else { return Ok(None) };
    if code.contains(',') {
        return Err(ToolError::invalid("short codes can't contain commas"));
    }
    if code.chars().count() > MAX_SHORT_CODE {
        return Err(ToolError::invalid(format!(
            "short codes can be at most {MAX_SHORT_CODE} characters"
        )));
    }
    let others: Vec<(&String, Option<&String>)> = match kind {
        SetupKind::Fringe => b
            .fringes
            .iter()
            .map(|(n, f)| (n, f.short_code.as_ref()))
            .collect(),
        _ => b
            .groups
            .iter()
            .map(|(n, g)| (n, g.short_code.as_ref()))
            .collect(),
    };
    for (other, theirs) in others {
        if other == name {
            continue;
        }
        if theirs.is_some_and(|c| c.to_lowercase() == code.to_lowercase()) {
            return Err(ToolError::invalid(format!(
                "{code} is already the short code of {other}"
            )));
        }
        if *other == code {
            return Err(ToolError::invalid(format!(
                "{code} is the name of another {}",
                kind.name()
            )));
        }
    }
    Ok(Some(code))
}

/// Refuses fields `kind`'s items don't have.
fn check_fields(kind: SetupKind, f: &Map<String, Value>) -> Result<(), ToolError> {
    let allowed: &[&str] = match kind {
        SetupKind::Fringe => &[
            "description",
            "short_code",
            "fringe_kind",
            "rate",
            "cutoff",
            "unit",
        ],
        SetupKind::Group => &["description", "short_code", "in_budget_total", "color"],
        SetupKind::Set => &["description"],
        SetupKind::Location => &["description", "city", "region", "country"],
        SetupKind::Unit => &["description", "plural", "hours"],
        SetupKind::Currency => &["description", "symbol", "rate"],
        SetupKind::Global => &[],
    };
    for k in f.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(ToolError::invalid(format!(
                "a {} has no field `{k}` (its fields: {})",
                kind.name(),
                allowed.join(", ")
            )));
        }
    }
    Ok(())
}

/// Adds (`existing` false) or changes item `name` of `kind` from `f`.
fn put_item(
    b: &mut Budget,
    kind: SetupKind,
    name: &str,
    f: &Map<String, Value>,
    existing: bool,
) -> Result<(), ToolError> {
    check_fields(kind, f)?;
    let has = fwb_core::rename::exists(b, kind, name);
    if existing && !has {
        return Err(ToolError::not_found(format!(
            "there is no {} {name}",
            kind.name()
        )));
    }
    if !existing {
        if has {
            return Err(ToolError::refused(format!(
                "there is already a {} named {name} (use update_setup to change it)",
                kind.name()
            )));
        }
        if !valid_name(kind, name) {
            return Err(ToolError::invalid(format!(
                "`{name}` can't be a {} name (no commas)",
                kind.name()
            )));
        }
    }
    let desc = text(f, "description");
    match kind {
        SetupKind::Fringe => {
            let old = b.fringes.get(name).cloned();
            let rate = match f.get("rate") {
                Some(v) => Formula(formula_text(v).trim().to_string()),
                None => old
                    .as_ref()
                    .map(|o| o.rate.clone())
                    .ok_or_else(|| ToolError::invalid("a new fringe needs a rate"))?,
            };
            let want = f.get("fringe_kind").and_then(Value::as_str);
            let was_percent = old
                .as_ref()
                .is_none_or(|o| matches!(o.kind, FringeKind::Percent { .. }));
            let percent = want.map_or(was_percent, |k| k == "percent");
            let fk = if percent {
                if f.contains_key("unit") && f.get("unit").is_some_and(|v| !v.is_null()) {
                    return Err(ToolError::invalid("a percent fringe has no unit"));
                }
                let cutoff = match f.get("cutoff") {
                    Some(Value::Null) => None,
                    Some(v) => Some(cents(v, "cutoff")?),
                    None => match old.as_ref().map(|o| &o.kind) {
                        Some(FringeKind::Percent { cutoff }) => *cutoff,
                        _ => None,
                    },
                };
                FringeKind::Percent { cutoff }
            } else {
                if f.get("cutoff").is_some_and(|v| !v.is_null()) {
                    return Err(ToolError::invalid("a flat fringe has no cutoff"));
                }
                let unit = match opt_text(f, "unit") {
                    Some(u) => u,
                    None => match old.as_ref().map(|o| &o.kind) {
                        Some(FringeKind::Flat { unit }) => unit.clone(),
                        _ => None,
                    },
                };
                if let Some(u) = &unit {
                    if !b.units.contains_key(u) {
                        return Err(ToolError::not_found(format!("there is no unit {u}")));
                    }
                }
                FringeKind::Flat { unit }
            };
            let code = match opt_text(f, "short_code") {
                Some(c) => short_code(b, kind, name, c)?,
                None => old.as_ref().and_then(|o| o.short_code.clone()),
            };
            b.fringes.insert(
                name.to_string(),
                Fringe {
                    description: desc
                        .or_else(|| old.as_ref().map(|o| o.description.clone()))
                        .unwrap_or_default(),
                    short_code: code,
                    rate,
                    kind: fk,
                },
            );
        }
        SetupKind::Group => {
            let old = b.groups.get(name).cloned();
            let code = match opt_text(f, "short_code") {
                Some(c) => short_code(b, kind, name, c)?,
                None => old.as_ref().and_then(|o| o.short_code.clone()),
            };
            let color = match opt_text(f, "color") {
                Some(c) => color(c)?,
                None => old.as_ref().and_then(|o| o.color.clone()),
            };
            b.groups.insert(
                name.to_string(),
                Group {
                    description: desc
                        .or_else(|| old.as_ref().map(|o| o.description.clone()))
                        .unwrap_or_default(),
                    short_code: code,
                    in_budget_total: f
                        .get("in_budget_total")
                        .and_then(Value::as_bool)
                        .or_else(|| old.as_ref().map(|o| o.in_budget_total))
                        .unwrap_or(true),
                    color,
                },
            );
        }
        SetupKind::Set => {
            let old = b.sets.get(name).cloned().unwrap_or_default();
            b.sets.insert(
                name.to_string(),
                SetDef {
                    description: desc.unwrap_or(old.description),
                },
            );
        }
        SetupKind::Location => {
            let old = b.locations.get(name).cloned().unwrap_or_default();
            b.locations.insert(
                name.to_string(),
                Location {
                    description: desc.unwrap_or(old.description),
                    city: text(f, "city").unwrap_or(old.city),
                    region: text(f, "region").unwrap_or(old.region),
                    country: text(f, "country").unwrap_or(old.country),
                },
            );
        }
        SetupKind::Unit => {
            let old = b.units.get(name).cloned();
            let hours = match f.get("hours") {
                Some(Value::Null) => None,
                Some(v) => {
                    let h = dec(v, "hours")?;
                    if h < Dec::ZERO {
                        return Err(ToolError::invalid("hours can't be negative"));
                    }
                    Some(h)
                }
                None => old.as_ref().and_then(|o| o.hours),
            };
            b.units.insert(
                name.to_string(),
                UnitDef {
                    plural: text(f, "plural")
                        .or_else(|| old.as_ref().map(|o| o.plural.clone()))
                        .unwrap_or_else(|| name.to_string()),
                    description: desc
                        .or_else(|| old.as_ref().map(|o| o.description.clone()))
                        .unwrap_or_default(),
                    hours,
                },
            );
        }
        SetupKind::Currency => {
            if name == b.base_currency {
                return Err(ToolError::refused(format!("{name} is the base currency")));
            }
            let old = b.currencies.get(name).cloned();
            let rate = match f.get("rate") {
                Some(v) => dec(v, "rate")?,
                None => old
                    .as_ref()
                    .map(|o| o.rate)
                    .ok_or_else(|| ToolError::invalid("a new currency needs a rate"))?,
            };
            if rate <= Dec::ZERO {
                return Err(ToolError::invalid("an exchange rate must be more than 0"));
            }
            b.currencies.insert(
                name.to_string(),
                Currency {
                    description: desc
                        .or_else(|| old.as_ref().map(|o| o.description.clone()))
                        .unwrap_or_default(),
                    symbol: text(f, "symbol")
                        .or_else(|| old.as_ref().map(|o| o.symbol.clone()))
                        .unwrap_or_default(),
                    rate,
                },
            );
        }
        SetupKind::Global => return Err(ToolError::invalid("use set_global for globals")),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The tools

fn set_global(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let name = a.req("name")?.trim().to_string();
    let formula = a.get("formula").map(formula_text).unwrap_or_default();
    if formula.trim().is_empty() {
        return Err(ToolError::invalid(format!("global {name} needs a value")));
    }
    let description = a.str("description").map(str::to_string);
    let unit = if a.has("unit") {
        Some(a.str("unit").map(str::to_string))
    } else {
        None
    };
    let label = format!("Set global {name}");
    let e = ctx.docs.edit(&path, &shown, &label, |b| {
        if let Some(Some(u)) = &unit {
            if !b.units.contains_key(u) {
                return Err(ToolError::not_found(format!("there is no unit {u}")));
            }
        }
        let log = script::apply(b, &Op::Global(name.clone(), formula.trim().to_string()))
            .map_err(ToolError::invalid)?;
        let info = b.global_info.entry(name.clone()).or_default();
        if let Some(d) = &description {
            info.description = d.trim().to_string();
        }
        if let Some(u) = &unit {
            info.unit = u.clone().unwrap_or_default();
        }
        if info.is_default() {
            b.global_info.remove(&name);
        }
        Ok(log)
    })?;
    let value = e
        .after
        .as_ref()
        .ok()
        .and_then(|r| r.globals.get(&name))
        .map(|d| d.to_string());
    let extra = obj(vec![
        ("name", json!(name)),
        ("value", json!(value)),
        ("changes", json!(e.out)),
    ]);
    Ok(edited(&shown, &e, &format!("Set global {name}"), extra))
}

fn add_setup(ctx: &mut Ctx, a: &Args) -> ToolResult {
    put_setup(ctx, a, false)
}

fn update_setup(ctx: &mut Ctx, a: &Args) -> ToolResult {
    put_setup(ctx, a, true)
}

fn put_setup(ctx: &mut Ctx, a: &Args, existing: bool) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let kind = setup_kind(a.req("type")?)?;
    let name = a.req("name")?.trim().to_string();
    let fields = a
        .get("fields")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let verb = if existing { "Change" } else { "Add" };
    let label = format!("{verb} {} {name}", kind.name());
    let e = ctx.docs.edit(&path, &shown, &label, |b| {
        put_item(b, kind, &name, &fields, existing)
    })?;
    let what = format!(
        "{} {} {name}",
        if existing { "Changed" } else { "Added" },
        kind.name()
    );
    Ok(edited(
        &shown,
        &e,
        &what,
        obj(vec![("type", json!(kind.name())), ("name", json!(name))]),
    ))
}

fn rename_setup(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let kind = setup_kind(a.req("type")?)?;
    let from = a.req("from")?.to_string();
    let to = a.req("to")?.trim().to_string();
    let label = format!("Rename {} {from} to {to}", kind.name());
    let e = ctx.docs.edit(&path, &shown, &label, |b| {
        rename(b, kind, &from, &to).map_err(|e| ToolError::refused(e.to_string()))
    })?;
    let what = format!(
        "Renamed {} {from} to {to} ({} reference(s) rewritten)",
        kind.name(),
        e.out
    );
    Ok(edited(
        &shown,
        &e,
        &what,
        obj(vec![("references", json!(e.out))]),
    ))
}

fn delete_setup(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let kind = setup_kind(a.req("type")?)?;
    let name = a.req("name")?.to_string();
    let force = a.bool("force");
    let label = format!("Delete {} {name}", kind.name());
    let e = ctx.docs.edit(&path, &shown, &label, |b| {
        delete_item(b, kind, &name, force)
    })?;
    let (lines, other) = e.out;
    let what = format!("Deleted {} {name}", kind.name());
    let extra = obj(vec![
        ("lines_affected", json!(lines)),
        ("other_uses", json!(other)),
    ]);
    Ok(edited(&shown, &e, &what, extra))
}

/// Deletes item `name` of `kind`; its use (lines, other) before.
fn delete_item(
    b: &mut Budget,
    kind: SetupKind,
    name: &str,
    force: bool,
) -> Result<(usize, usize), ToolError> {
    let use_ = fwb_core::usage::usage(b, kind).get(name).copied();
    let exists = match kind {
        SetupKind::Currency => b.currencies.contains_key(name),
        _ => fwb_core::rename::exists(b, kind, name),
    };
    if !exists {
        if kind == SetupKind::Currency && name == b.base_currency {
            return Err(ToolError::refused(format!("{name} is the base currency")));
        }
        return Err(ToolError::not_found(format!(
            "there is no {} {name}",
            kind.name()
        )));
    }
    let u = use_.unwrap_or_default();
    // Refusals force doesn't override.
    if kind == SetupKind::Group {
        if let Some(c) = b.credits.iter().find(|c| {
            c.rows
                .iter()
                .any(|r| matches!(&r.source, CreditSource::Group { group, .. } if group == name))
        }) {
            return Err(ToolError::refused(format!(
                "group {name} is used by the credit {}; change the credit first",
                c.name
            )));
        }
    }
    if kind == SetupKind::Currency {
        if b.totals_currency.as_deref() == Some(name) {
            return Err(ToolError::refused(format!(
                "totals are shown in {name}; change that first"
            )));
        }
        if b.charges
            .iter()
            .any(|c| matches!(&c.kind, ChargeKind::Flat { currency: Some(cur), .. } if cur == name))
        {
            return Err(ToolError::refused(format!(
                "a charge is in {name}; change it first"
            )));
        }
    }
    if u.is_used() && !force {
        let then = match kind {
            SetupKind::Global => {
                "its value would be frozen into the formulas that use it (no total changes)"
            }
            SetupKind::Fringe | SetupKind::Group | SetupKind::Set | SetupKind::Location => {
                "it would be taken off those lines (totals may change)"
            }
            SetupKind::Unit => "it would be blanked on those lines and settings",
            SetupKind::Currency => "those lines would go back to the base currency (totals change)",
        };
        return Err(ToolError::refused(format!(
            "{} {name} is used by {} line(s) and {} other setting(s); nothing was deleted. Pass \
             force: true to delete it anyway: {then}.",
            kind.name(),
            u.lines,
            u.other
        )));
    }
    match kind {
        SetupKind::Global => {
            if fwb_core::delete_global(b, name).is_err() {
                b.globals.remove(name);
                b.global_info.remove(name);
            }
        }
        SetupKind::Fringe => {
            b.fringes.remove(name);
            for d in details_mut(b) {
                d.fringes.remove(name);
            }
            for c in &mut b.credits {
                for r in &mut c.rows {
                    if let CreditSource::Group {
                        excluded_fringes, ..
                    } = &mut r.source
                    {
                        excluded_fringes.remove(name);
                    }
                }
            }
        }
        SetupKind::Group => {
            b.groups.remove(name);
            for d in details_mut(b) {
                d.groups.remove(name);
            }
            for c in &mut b.charges {
                c.excluded_groups.remove(name);
            }
        }
        SetupKind::Set => {
            b.sets.remove(name);
            for d in details_mut(b) {
                if d.set.as_deref() == Some(name) {
                    d.set = None;
                }
            }
        }
        SetupKind::Location => {
            b.locations.remove(name);
            for d in details_mut(b) {
                if d.location.as_deref() == Some(name) {
                    d.location = None;
                }
            }
        }
        SetupKind::Unit => {
            b.units.remove(name);
            for d in details_mut(b) {
                for u in [&mut d.unit, &mut d.unit2, &mut d.unit3, &mut d.unit4] {
                    if u == name {
                        u.clear();
                    }
                }
            }
            for f in b.fringes.values_mut() {
                if let FringeKind::Flat { unit } = &mut f.kind {
                    if unit.as_deref() == Some(name) {
                        *unit = None;
                    }
                }
            }
            for gi in b.global_info.values_mut() {
                if gi.unit == name {
                    gi.unit.clear();
                }
            }
            b.global_info.retain(|_, gi| !gi.is_default());
        }
        SetupKind::Currency => {
            for d in details_mut(b) {
                if d.currency.as_deref() == Some(name) {
                    d.currency = None;
                }
            }
            b.currencies.remove(name);
        }
    }
    fwb_core::order::forget(b, kind, name);
    Ok((u.lines, u.other))
}

fn details_mut(b: &mut Budget) -> impl Iterator<Item = &mut fwb_core::Detail> {
    b.categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
}

/// An account number as the budget writes it: one already in the budget
/// exactly as typed (a chart's own numbers may not follow the pattern),
/// else the typed number in the budget's pattern.
fn account_number(b: &Budget, typed: &str) -> Result<String, ToolError> {
    let t = typed.trim();
    if t.is_empty() {
        return Err(ToolError::invalid("the account number is empty"));
    }
    if b.categories()
        .flat_map(|c| &c.accounts)
        .any(|a| a.code == t)
    {
        return Ok(t.to_string());
    }
    if b.account_coding.validate().is_err() {
        return Ok(t.to_string());
    }
    b.account_coding
        .normalize(t)
        .map_err(|e| ToolError::invalid(format!("{t}: {e}")))
}

/// A category number as the budget writes it: one already in the budget
/// exactly as typed, else the typed number's category part.
fn category_number(b: &Budget, typed: &str) -> Result<String, ToolError> {
    let t = typed.trim();
    if t.is_empty() {
        return Err(ToolError::invalid("the category number is empty"));
    }
    if b.categories().any(|c| c.code == t) {
        return Ok(t.to_string());
    }
    if b.account_coding.validate().is_err() {
        return Ok(t.to_string());
    }
    b.account_coding
        .parse(t)
        .map(|c| c.category)
        .map_err(|e| ToolError::invalid(format!("{t}: {e}")))
}

/// Names picked for a list: each must be in `known` or already in `kept`.
fn picked(
    names: &[String],
    known: &BTreeSet<String>,
    kept: Option<&BTreeSet<String>>,
    what: &str,
) -> Result<BTreeSet<String>, ToolError> {
    let mut out = BTreeSet::new();
    for n in names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        if !known.contains(n) && !kept.is_some_and(|k| k.contains(n)) {
            return Err(ToolError::not_found(format!("there is no {what} {n}")));
        }
        out.insert(n.to_string());
    }
    Ok(out)
}

fn set_charge(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let name = a.req("name")?.trim().to_string();
    let args = a.0.clone();
    let label = format!("Set charge {name}");
    let e = ctx.docs.edit(&path, &shown, &label, |b| {
        let a = Args(&args);
        let index = b.charges.iter().position(|c| c.name == name);
        let old = index.map(|i| b.charges[i].clone());
        let new_name = a
            .str("new_name")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| name.clone());
        if new_name.is_empty() {
            return Err(ToolError::invalid("the charge's name is empty"));
        }
        if b.charges
            .iter()
            .enumerate()
            .any(|(i, c)| Some(i) != index && c.name == new_name)
        {
            return Err(ToolError::refused(format!(
                "there is already a charge named {new_name}"
            )));
        }
        let want_kind = a.str("charge_kind");
        let old_flat = old.as_ref().and_then(|o| match &o.kind {
            ChargeKind::Flat { amount, currency } => Some((*amount, currency.clone())),
            ChargeKind::Percent(_) => None,
        });
        let old_percent = old.as_ref().and_then(|o| match &o.kind {
            ChargeKind::Percent(p) => Some(*p),
            ChargeKind::Flat { .. } => None,
        });
        let flat = match want_kind {
            Some(k) => k == "flat",
            None => match &old {
                Some(_) => old_flat.is_some(),
                None => {
                    return Err(ToolError::invalid(
                        "a new charge needs charge_kind (percent or flat)",
                    ))
                }
            },
        };
        let kind = if flat {
            let amount = match a.get("amount") {
                Some(v) => cents(v, "amount")?,
                None => old_flat
                    .as_ref()
                    .map(|f| f.0)
                    .ok_or_else(|| ToolError::invalid("a flat charge needs an amount"))?,
            };
            let currency = if a.has("currency") {
                match a.str("currency").map(str::trim).filter(|c| !c.is_empty()) {
                    None => None,
                    Some(c) if c == b.base_currency => None,
                    Some(c) if b.currencies.contains_key(c) => Some(c.to_string()),
                    Some(c) => {
                        return Err(ToolError::not_found(format!("there is no currency {c}")))
                    }
                }
            } else {
                old_flat.and_then(|f| f.1)
            };
            ChargeKind::Flat { amount, currency }
        } else {
            let p = match a.get("percent") {
                Some(v) => dec(v, "percent")?,
                None => old_percent
                    .ok_or_else(|| ToolError::invalid("a percent charge needs a percent"))?,
            };
            if p < Dec::ZERO {
                return Err(ToolError::invalid("the percent can't be negative"));
            }
            ChargeKind::Percent(p)
        };
        let groups: BTreeSet<String> = b.groups.keys().cloned().collect();
        let categories: BTreeSet<String> = b.categories().map(|c| c.code.clone()).collect();
        let accounts: BTreeSet<String> = b
            .categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.code.clone())
            .collect();
        let list =
            |key: &str, known: &BTreeSet<String>, kept: Option<&BTreeSet<String>>, what: &str| {
                if a.has(key) {
                    picked(&a.strings(key), known, kept, what)
                } else {
                    Ok(kept.cloned().unwrap_or_default())
                }
            };
        let excluded_amounts = if a.has("excluded_amounts") {
            let mut v = Vec::new();
            for x in a.list("excluded_amounts") {
                let desc = x
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let amt = x
                    .get("amount")
                    .map(|v| cents(v, "excluded amount"))
                    .transpose()?
                    .unwrap_or(0);
                v.push((desc, amt));
            }
            v
        } else {
            old.as_ref()
                .map(|o| o.excluded_amounts.clone())
                .unwrap_or_default()
        };
        let account = if a.has("account") {
            match a.str("account").map(str::trim).filter(|s| !s.is_empty()) {
                Some(t) => Some(account_number(b, t)?),
                None => None,
            }
        } else {
            old.as_ref().and_then(|o| o.account.clone())
        };
        let charge = Charge {
            name: new_name.clone(),
            account,
            applied: a
                .opt_bool("applied")
                .or(old.as_ref().map(|o| o.applied))
                .unwrap_or(true),
            kind,
            excluded_amounts,
            excluded_groups: list(
                "excluded_groups",
                &groups,
                old.as_ref().map(|o| &o.excluded_groups),
                "group",
            )?,
            excluded_categories: list(
                "excluded_categories",
                &categories,
                old.as_ref().map(|o| &o.excluded_categories),
                "category",
            )?,
            excluded_accounts: list(
                "excluded_accounts",
                &accounts,
                old.as_ref().map(|o| &o.excluded_accounts),
                "account",
            )?,
            label_percent: a
                .opt_bool("label_percent")
                .or(old.as_ref().map(|o| o.label_percent))
                .unwrap_or(false),
            label_excluded: a
                .opt_bool("label_excluded")
                .or(old.as_ref().map(|o| o.label_excluded))
                .unwrap_or(false),
        };
        match index {
            Some(i) => b.charges[i] = charge,
            None => b.charges.push(charge),
        }
        Ok(index.is_none())
    })?;
    let what = format!("{} charge {name}", if e.out { "Added" } else { "Changed" });
    Ok(edited(
        &shown,
        &e,
        &what,
        obj(vec![("name", json!(name)), ("added", json!(e.out))]),
    ))
}

fn set_credit(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let name = a.req("name")?.trim().to_string();
    let args = a.0.clone();
    let label = format!("Set credit {name}");
    let e = ctx.docs.edit(&path, &shown, &label, |b| {
        let a = Args(&args);
        let index = b.credits.iter().position(|c| c.name == name);
        let old = index.map(|i| b.credits[i].clone());
        let new_name = a
            .str("new_name")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| name.clone());
        if new_name.is_empty() {
            return Err(ToolError::invalid("the credit's name is empty"));
        }
        if b.credits
            .iter()
            .enumerate()
            .any(|(i, c)| Some(i) != index && c.name == new_name)
        {
            return Err(ToolError::refused(format!(
                "there is already a credit named {new_name}"
            )));
        }
        let discount_percent = match a.get("discount_percent") {
            Some(v) => {
                let d = dec(v, "discount_percent")?;
                if d < Dec::ZERO || d > Dec::from_int(100) {
                    return Err(ToolError::invalid("the discount is from 0 to 100%"));
                }
                d
            }
            None => old.as_ref().map_or(Dec::ZERO, |o| o.discount_percent),
        };
        let cap = if a.has("cap") {
            a.get("cap").map(|v| cents(v, "cap")).transpose()?
        } else {
            old.as_ref().and_then(|o| o.cap)
        };
        let rows = if a.has("rows") {
            let mut rows = Vec::new();
            for (i, r) in a.list("rows").iter().enumerate() {
                let n = i + 1;
                let src = r.get("source").cloned().unwrap_or(Value::Null);
                let source = match src.get("kind").and_then(Value::as_str) {
                    Some("group") => {
                        let g = src
                            .get("group")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        if !b.groups.contains_key(&g) {
                            return Err(ToolError::not_found(format!(
                                "row {n}: there is no group `{g}`"
                            )));
                        }
                        let ex: Vec<String> = src
                            .get("excluded_fringes")
                            .and_then(Value::as_array)
                            .map(|l| {
                                l.iter()
                                    .filter_map(|x| x.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let known: BTreeSet<String> = b.fringes.keys().cloned().collect();
                        CreditSource::Group {
                            group: g,
                            excluded_fringes: picked(&ex, &known, None, "fringe")?,
                        }
                    }
                    _ => {
                        let amt = src.get("amount").ok_or_else(|| {
                            ToolError::invalid(format!("row {n}: a flat source needs an amount"))
                        })?;
                        CreditSource::Flat(cents(amt, &format!("row {n}'s amount"))?)
                    }
                };
                let percent = dec(
                    r.get("percent").unwrap_or(&Value::Null),
                    &format!("row {n}'s percent"),
                )?;
                if percent < Dec::ZERO {
                    return Err(ToolError::invalid(format!(
                        "row {n}'s percent can't be negative"
                    )));
                }
                let cap = match r.get("cap") {
                    None => CreditCap::None,
                    Some(c) => {
                        let amount = || -> Result<Cents, ToolError> {
                            cents(
                                c.get("amount").unwrap_or(&Value::Null),
                                &format!("row {n}'s cap"),
                            )
                        };
                        match c.get("kind").and_then(Value::as_str) {
                            Some("qualified_spend") => CreditCap::QualifiedSpend(amount()?),
                            Some("credit") => CreditCap::Credit(amount()?),
                            _ => CreditCap::None,
                        }
                    }
                };
                rows.push(CreditRow {
                    description: r
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                    source,
                    percent,
                    cap,
                });
            }
            rows
        } else {
            old.as_ref().map(|o| o.rows.clone()).unwrap_or_default()
        };
        let account = if a.has("account") {
            match a.str("account").map(str::trim).filter(|s| !s.is_empty()) {
                Some(t) => Some(account_number(b, t)?),
                None => None,
            }
        } else {
            old.as_ref().and_then(|o| o.account.clone())
        };
        let credit = Credit {
            name: new_name,
            account,
            applied: a
                .opt_bool("applied")
                .or(old.as_ref().map(|o| o.applied))
                .unwrap_or(true),
            cap,
            discount_percent,
            rows,
        };
        match index {
            Some(i) => b.credits[i] = credit,
            None => b.credits.push(credit),
        }
        Ok(index.is_none())
    })?;
    let what = format!("{} credit {name}", if e.out { "Added" } else { "Changed" });
    Ok(edited(
        &shown,
        &e,
        &what,
        obj(vec![("name", json!(name)), ("added", json!(e.out))]),
    ))
}

fn delete_charge(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let credit = a.req("type")? == "credit";
    let name = a.req("name")?.to_string();
    let what = if credit { "credit" } else { "charge" };
    let label = format!("Delete {what} {name}");
    let e = ctx.docs.edit(&path, &shown, &label, |b| {
        let before = if credit {
            b.credits.len()
        } else {
            b.charges.len()
        };
        if credit {
            b.credits.retain(|c| c.name != name);
        } else {
            b.charges.retain(|c| c.name != name);
        }
        let after = if credit {
            b.credits.len()
        } else {
            b.charges.len()
        };
        if before == after {
            return Err(ToolError::not_found(format!("there is no {what} {name}")));
        }
        Ok(())
    })?;
    Ok(edited(
        &shown,
        &e,
        &format!("Deleted {what} {name}"),
        Map::new(),
    ))
}

fn apply_charge(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let credit = a.req("type")? == "credit";
    let name = a.req("name")?.to_string();
    let applied = a.bool("applied");
    let what = if credit { "credit" } else { "charge" };
    let verb = if applied { "Apply" } else { "Unapply" };
    let label = format!("{verb} {what} {name}");
    let e = ctx.docs.edit(&path, &shown, &label, |b| {
        if credit {
            let c = b
                .credits
                .iter_mut()
                .find(|c| c.name == name)
                .ok_or_else(|| ToolError::not_found(format!("there is no credit {name}")))?;
            c.applied = applied;
            Ok(())
        } else {
            script::apply(b, &Op::Charge(name.clone(), applied))
                .map(|_| ())
                .map_err(ToolError::not_found)
        }
    })?;
    let state = if applied {
        "applied"
    } else {
        "no longer applied"
    };
    Ok(edited(
        &shown,
        &e,
        &format!("The {what} {name} is {state}"),
        obj(vec![("applied", json!(applied))]),
    ))
}

fn add_category(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let typed = a.req("code")?.to_string();
    let name = a.req("name")?.trim().to_string();
    let after = a.str("after_category").map(str::to_string);
    let before_total = a.str("before_total").map(str::to_string);
    let at_start = a.bool("at_start");
    let e = ctx
        .docs
        .edit(&path, &shown, &format!("Add category {typed}"), |b| {
            let code = category_number(b, &typed)?;
            if b.categories().any(|c| c.code == code) {
                return Err(ToolError::refused(format!(
                    "category {code} already exists"
                )));
            }
            let given = usize::from(after.is_some())
                + usize::from(before_total.is_some())
                + usize::from(at_start);
            if given > 1 {
                return Err(ToolError::invalid(
                    "give one of after_category, before_total and at_start",
                ));
            }
            let at = if let Some(c) = &after {
                b.topsheet
                    .iter()
                    .position(|r| matches!(r, TopsheetRow::Category(x) if x.code == *c))
                    .map(|i| i + 1)
                    .ok_or_else(|| ToolError::not_found(format!("there is no category {c}")))?
            } else if let Some(t) = &before_total {
                b.topsheet
                    .iter()
                    .position(|r| matches!(r, TopsheetRow::ProductionTotal(p) if p.name == *t))
                    .ok_or_else(|| {
                        ToolError::not_found(format!("there is no production total {t}"))
                    })?
            } else if at_start {
                0
            } else {
                b.topsheet.len()
            };
            b.topsheet.insert(
                at,
                TopsheetRow::Category(Category {
                    code: code.clone(),
                    name: name.clone(),
                    accounts: Vec::new(),
                    notes: Vec::new(),
                }),
            );
            Ok(code)
        })?;
    let what = format!("Added category {}", e.out);
    Ok(edited(&shown, &e, &what, obj(vec![("code", json!(e.out))])))
}

fn add_production_total(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let name = a.req("name")?.trim().to_string();
    let after = a.str("after_category").map(str::to_string);
    let e = ctx.docs.edit(
        &path,
        &shown,
        &format!("Add production total {name}"),
        |b| {
            if name.is_empty() {
                return Err(ToolError::invalid("the production total's name is empty"));
            }
            let at = match &after {
                Some(c) => b
                    .topsheet
                    .iter()
                    .position(|r| matches!(r, TopsheetRow::Category(x) if x.code == *c))
                    .map(|i| i + 1)
                    .ok_or_else(|| ToolError::not_found(format!("there is no category {c}")))?,
                None => b.topsheet.len(),
            };
            b.topsheet.insert(
                at,
                TopsheetRow::ProductionTotal(fwb_core::ProductionTotal { name: name.clone() }),
            );
            Ok(())
        },
    )?;
    Ok(edited(
        &shown,
        &e,
        &format!("Added production total {name}"),
        Map::new(),
    ))
}

fn delete_production_total(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let name = a.req("name")?.to_string();
    let e = ctx.docs.edit(
        &path,
        &shown,
        &format!("Delete production total {name}"),
        |b| {
            let i = b
                .topsheet
                .iter()
                .position(|r| matches!(r, TopsheetRow::ProductionTotal(p) if p.name == name))
                .ok_or_else(|| {
                    ToolError::not_found(format!("there is no production total {name}"))
                })?;
            b.topsheet.remove(i);
            Ok(())
        },
    )?;
    Ok(edited(
        &shown,
        &e,
        &format!("Deleted production total {name}"),
        Map::new(),
    ))
}

fn add_account(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let category = a.req("category")?.to_string();
    let typed = a.req("code")?.to_string();
    let name = a.req("name")?.trim().to_string();
    let after = a.str("after_account").map(str::to_string);
    let e = ctx
        .docs
        .edit(&path, &shown, &format!("Add account {typed}"), |b| {
            let code = account_number(b, &typed)?;
            if b.categories()
                .flat_map(|c| &c.accounts)
                .any(|x| x.code == code)
            {
                return Err(ToolError::refused(format!("account {code} already exists")));
            }
            let cat = b
                .categories_mut()
                .find(|c| c.code == category)
                .ok_or_else(|| ToolError::not_found(format!("there is no category {category}")))?;
            let at = match &after {
                Some(x) => cat
                    .accounts
                    .iter()
                    .position(|acct| acct.code == *x)
                    .map(|i| i + 1)
                    .ok_or_else(|| {
                        ToolError::not_found(format!("account {x} is not in category {category}"))
                    })?,
                None => cat.accounts.len(),
            };
            cat.accounts.insert(
                at,
                Account {
                    code: code.clone(),
                    name: name.clone(),
                    details: Vec::new(),
                    notes: Vec::new(),
                },
            );
            Ok(code)
        })?;
    let what = format!("Added account {}", e.out);
    Ok(edited(&shown, &e, &what, obj(vec![("code", json!(e.out))])))
}

fn update_category(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let code = a.req("code")?.to_string();
    let new_code = a.str("new_code").map(str::to_string);
    let name = a.str("name").map(|s| s.trim().to_string());
    let e = ctx
        .docs
        .edit(&path, &shown, &format!("Change category {code}"), |b| {
            let new_code = new_code
                .as_deref()
                .map(|t| category_number(b, t))
                .transpose()?;
            if let Some(n) = &new_code {
                if *n != code && b.categories().any(|c| c.code == *n) {
                    return Err(ToolError::refused(format!("category {n} already exists")));
                }
            }
            let cat = b
                .categories_mut()
                .find(|c| c.code == code)
                .ok_or_else(|| ToolError::not_found(format!("there is no category {code}")))?;
            if let Some(n) = new_code {
                cat.code = n;
            }
            if let Some(n) = &name {
                cat.name = n.clone();
            }
            Ok(cat.code.clone())
        })?;
    let what = format!("Changed category {code}");
    Ok(edited(&shown, &e, &what, obj(vec![("code", json!(e.out))])))
}

fn update_account(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let code = a.req("code")?.to_string();
    let new_code = a.str("new_code").map(str::to_string);
    let name = a.str("name").map(|s| s.trim().to_string());
    let e = ctx
        .docs
        .edit(&path, &shown, &format!("Change account {code}"), |b| {
            let new_code = new_code
                .as_deref()
                .map(|t| account_number(b, t))
                .transpose()?;
            if let Some(n) = &new_code {
                if *n != code
                    && b.categories()
                        .flat_map(|c| &c.accounts)
                        .any(|x| x.code == *n)
                {
                    return Err(ToolError::refused(format!("account {n} already exists")));
                }
            }
            let acct = b
                .categories_mut()
                .flat_map(|c| &mut c.accounts)
                .find(|x| x.code == code)
                .ok_or_else(|| ToolError::not_found(format!("there is no account {code}")))?;
            if let Some(n) = new_code {
                acct.code = n;
            }
            if let Some(n) = &name {
                acct.name = n.clone();
            }
            Ok(acct.code.clone())
        })?;
    let what = format!("Changed account {code}");
    Ok(edited(&shown, &e, &what, obj(vec![("code", json!(e.out))])))
}

fn delete_category(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let code = a.req("code")?.to_string();
    let force = a.bool("force");
    let e = ctx
        .docs
        .edit(&path, &shown, &format!("Delete category {code}"), |b| {
            let i = b
                .topsheet
                .iter()
                .position(|r| matches!(r, TopsheetRow::Category(c) if c.code == code))
                .ok_or_else(|| ToolError::not_found(format!("there is no category {code}")))?;
            let TopsheetRow::Category(c) = &b.topsheet[i] else {
                unreachable!()
            };
            let rows: usize = c.accounts.iter().map(|x| x.details.len()).sum();
            if rows > 0 && !force {
                return Err(ToolError::refused(format!(
                "category {code} has {} account(s) with {rows} row(s); nothing was deleted. Pass \
                 force: true to delete them with it.",
                c.accounts.len()
            )));
            }
            b.topsheet.remove(i);
            Ok(rows)
        })?;
    let what = format!("Deleted category {code}");
    Ok(edited(
        &shown,
        &e,
        &what,
        obj(vec![("rows_deleted", json!(e.out))]),
    ))
}

fn delete_account(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let code = a.req("code")?.to_string();
    let force = a.bool("force");
    let e = ctx
        .docs
        .edit(&path, &shown, &format!("Delete account {code}"), |b| {
            for cat in b.categories_mut() {
                if let Some(i) = cat.accounts.iter().position(|x| x.code == code) {
                    let rows = cat.accounts[i].details.len();
                    if rows > 0 && !force {
                        return Err(ToolError::refused(format!(
                        "account {code} has {rows} row(s); nothing was deleted. Pass force: true \
                         to delete them with it."
                    )));
                    }
                    cat.accounts.remove(i);
                    return Ok(rows);
                }
            }
            Err(ToolError::not_found(format!("there is no account {code}")))
        })?;
    let what = format!("Deleted account {code}");
    Ok(edited(
        &shown,
        &e,
        &what,
        obj(vec![("rows_deleted", json!(e.out))]),
    ))
}

fn set_title(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let title = a.req("title")?.trim().to_string();
    let e = ctx.docs.edit(&path, &shown, "Set title", |b| {
        script::apply(b, &Op::Title(title.clone())).map_err(ToolError::invalid)
    })?;
    Ok(edited(&shown, &e, "Set the title", Map::new()))
}
