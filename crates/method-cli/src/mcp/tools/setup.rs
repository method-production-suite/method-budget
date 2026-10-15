//! Tools that change the budget's setup: globals, the setup tables (fringes,
//! groups, sets, locations, units, currencies), charges and credits,
//! categories and accounts, and the title. Renames go through the engine's
//! rename, which rewrites every reference (decision 32); deletes check how
//! much an item is used first (`method_core::usage`) and, only when asked to,
//! take it off what uses it with the app's own deletes.
//!
//! Every change is made by the app's edits (`method_session::edit`), so the
//! server checks what the app checks and refuses what it refuses.

use std::collections::BTreeSet;

use method_core::rename::{rename, valid_name, SetupKind};
use method_core::{
    Budget, Cents, ChargeKind, CreditCap, CreditRow, CreditSource, Dec, FlatBasis, FringeKind,
    TopsheetRow,
};
use method_format::serde_json::{json, Map, Value};
use method_session::display::{self, GlobalInfoInput};
use method_session::edit::{
    self, parse_money_as, typed, ChargeInput, CreditCapInput, CreditInput, CreditRowInput,
    CreditSourceInput, CurrencyInput, ExcludedAmountInput, FringeInput, GroupInput, LocationInput,
    SetInput, SubCents, UnitInput,
};
use method_session::rowstyle;

use super::read::setup_kind;
use super::{
    budget_path, edited, formula_text, obj, path_prop, Args, Ctx, Tool, ToolError, ToolResult,
};
use crate::mcp::schema::{self as s, object};
use crate::script;

const TABLES: &[&str] = &["fringe", "group", "set", "location", "unit", "currency"];
const TABLES_AND_GLOBAL: &[&str] = &[
    "global", "fringe", "group", "set", "location", "unit", "currency",
];

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "set_global",
            title: "Set a global",
            description: "Sets a global's formula (adding the global when it is new), and \
                optionally its description and unit. Names are letters, digits and _, with at \
                least one letter or _ (1ZG is a name). Lines use globals by name in their formulas (SHOOT_DAYS * 2).",
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
                fringe {fringe_kind percent|flat, rate, cutoff (percent), unit (flat), basis \
                (flat), max_hours_per_day (flat), description, short_code}; group \
                {in_budget_total, description, short_code, \
                color}; set {description}; location {description, city, region, country}; unit \
                {plural, description, hours}; currency {rate (base units per one of it), \
                description, symbol, symbol_after, separators}.",
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
                rows that each take a percent of a flat amount, of a group's lines, of a \
                topsheet category's total or of an account's total, with their fringes (less \
                the credit's excluded fringes, one list for all its group, category and \
                account rows), with \
                caps, an overall cap and a discount. rows replaces every \
                row when given. New credits are applied unless applied is false.",
            mutating: true,
            destructive: false,
            schema: || {
                let source = object(
                    vec![
                        ("kind", s::one_of("flat, group, category or account.", &["flat", "group", "category", "account"])),
                        ("amount", money_schema("flat: the gross amount.")),
                        ("group", s::name("group: the group whose lines count.")),
                        ("category", s::name("category: the topsheet category whose total counts, with its fringes.")),
                        ("account", s::name("account: the account whose total counts, with its fringes.")),
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
                        ("excluded_fringes", s::array("Fringes left out of the spend of every group, category and account row (replaces the list).", s::name("A fringe."), 100)),
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
            ("cutoff", s::nullable(money_schema("fringe (percent): pay per earner it stops at (168600.00), or null. A flat fringe keeps one it had but never uses it; only null is taken for a flat fringe."))),
            ("unit", s::nullable(s::name("fringe (flat): charge per this unit (the line's Amount converted through unit hours; once per line when null or when either unit has no hours), or null."))),
            ("basis", s::nullable(s::one_of("fringe (flat): what a line counts. amount (the standard): the Amount column only, X and 4X never count; amount_counts: also times X and 4X when their units aren't time units (a blank unit is a count); quantity: Amount x X x 4X. null is amount.", &["amount", "amount_counts", "quantity"]))),
            ("max_hours_per_day", s::nullable(json!({"type": ["string", "number"], "description": "fringe (flat, charged per hour): most hours a day counted (12), or null for no cap. Days come from the line's Amount through the Day unit; hours a day are X when X is in hours, else the Day unit's hours.", "maxLength": 40}))),
            ("in_budget_total", s::boolean("group: whether its lines count (false suppresses them).")),
            ("color", s::nullable(s::name("group: text color #rrggbb, or null."))),
            ("city", s::text("location: city.")),
            ("region", s::text("location: region.")),
            ("country", s::text("location: country.")),
            ("plural", s::text("unit: plural name.")),
            ("hours", s::nullable(json!({"type": ["string", "number"], "description": "unit: hours in one of it, or null.", "maxLength": 40}))),
            ("symbol", s::text("currency: symbol.")),
            ("symbol_after", s::boolean("currency: write the symbol after the value (1.234,56 €) instead of before it ($1,234.56).")),
            ("separators", s::one_of("currency: comma_point (1,234.56) or point_comma (1.234,56).", &["comma_point", "point_comma"])),
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
//
// The edits themselves are the app's (method_session::edit): these read the
// tool's arguments, fill in what an update leaves out from the item as it
// is, and hand the whole item over. Money is read exactly (never rounded)
// here and passed on as text that reads back to the same cents.

fn cents(v: &Value, what: &str) -> Result<Cents, ToolError> {
    let c = parse_money_as(&formula_text(v), SubCents::Refuse)
        .map_err(|e| ToolError::invalid(format!("{what}: {e}")))?;
    if c < 0 {
        return Err(ToolError::invalid(format!("{what} can't be negative")));
    }
    Ok(c)
}

/// An amount in cents as money text the app reads back exactly.
fn money_text(c: Cents) -> String {
    Dec::from_cents(c).to_string()
}

/// [`cents`] as money text.
fn exact(v: &Value, what: &str) -> Result<String, ToolError> {
    cents(v, what).map(money_text)
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

/// A short code field as the app's edits take it: left out keeps the
/// item's own, blank or null clears it.
fn short_code(f: &Map<String, Value>) -> Option<String> {
    opt_text(f, "short_code").map(Option::unwrap_or_default)
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
            "basis",
            "max_hours_per_day",
        ],
        SetupKind::Group => &["description", "short_code", "in_budget_total", "color"],
        SetupKind::Set => &["description"],
        SetupKind::Location => &["description", "city", "region", "country"],
        SetupKind::Unit => &["description", "plural", "hours"],
        SetupKind::Currency => &[
            "description",
            "symbol",
            "rate",
            "symbol_after",
            "separators",
        ],
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
    let has = method_core::rename::exists(b, kind, name);
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
                Some(v) => formula_text(v),
                None => old
                    .as_ref()
                    .map(|o| o.rate.0.clone())
                    .ok_or_else(|| ToolError::invalid("a new fringe needs a rate"))?,
            };
            let want = f.get("fringe_kind").and_then(Value::as_str);
            let was_percent = old
                .as_ref()
                .is_none_or(|o| matches!(o.kind, FringeKind::Percent { .. }));
            let percent = want.map_or(was_percent, |k| k == "percent");
            // The cutoff kept either way: a flat fringe keeps one it had
            // (never used), so switching back to percent brings it back.
            let old_cutoff = old.as_ref().and_then(|o| o.kind.cutoff());
            let cutoff =
                if percent {
                    for k in ["unit", "basis", "max_hours_per_day"] {
                        if f.get(k).is_some_and(|v| !v.is_null()) {
                            return Err(ToolError::invalid(format!(
                                "a percent fringe has no {}",
                                k.replace('_', " ")
                            )));
                        }
                    }
                    match f.get("cutoff") {
                        Some(Value::Null) => None,
                        Some(v) => Some(cents(v, "cutoff")?),
                        None => old_cutoff,
                    }
                } else {
                    match f.get("cutoff") {
                        None => old_cutoff,
                        Some(Value::Null) => None,
                        Some(_) => return Err(ToolError::invalid(
                            "a flat fringe has no cutoff: cutoffs apply to percent fringes only",
                        )),
                    }
                };
            let (old_unit, old_basis, old_max) = match old.as_ref().map(|o| &o.kind) {
                Some(FringeKind::Flat {
                    unit,
                    basis,
                    max_hours_per_day,
                    ..
                }) => (unit.clone(), *basis, *max_hours_per_day),
                _ => (None, FlatBasis::Amount, None),
            };
            let unit = opt_text(f, "unit").unwrap_or(old_unit);
            if let Some(u) = &unit {
                if !b.units.contains_key(u) {
                    return Err(ToolError::not_found(format!("there is no unit {u}")));
                }
            }
            let basis = match f.get("basis") {
                None => Some(old_basis.name().to_string()),
                Some(v) => v.as_str().map(str::to_string),
            };
            let max_hours_per_day = match f.get("max_hours_per_day") {
                None => old_max.map(|h| h.to_string()),
                Some(Value::Null) => None,
                Some(v) => Some(formula_text(v)),
            };
            let input = FringeInput {
                kind: if percent { "percent" } else { "flat" }.to_string(),
                description: desc
                    .or_else(|| old.as_ref().map(|o| o.description.clone()))
                    .unwrap_or_default(),
                rate,
                cutoff: cutoff.map(money_text),
                unit,
                basis,
                max_hours_per_day,
                short_code: short_code(f),
            };
            edit::set_fringe(b, name, &input)?;
        }
        SetupKind::Group => {
            let old = b.groups.get(name).cloned();
            let input = GroupInput {
                description: desc
                    .or_else(|| old.as_ref().map(|o| o.description.clone()))
                    .unwrap_or_default(),
                in_budget_total: f
                    .get("in_budget_total")
                    .and_then(Value::as_bool)
                    .or_else(|| old.as_ref().map(|o| o.in_budget_total))
                    .unwrap_or(true),
                short_code: short_code(f),
            };
            edit::set_group(b, name, &input)?;
            if let Some(color) = opt_text(f, "color") {
                rowstyle::apply_group_color(b, name, color.as_deref())?;
            }
        }
        SetupKind::Set => {
            let old = b.sets.get(name).cloned().unwrap_or_default();
            let input = SetInput {
                description: desc.unwrap_or(old.description),
            };
            edit::set_set(b, name, &input)?;
        }
        SetupKind::Location => {
            let old = b.locations.get(name).cloned().unwrap_or_default();
            let input = LocationInput {
                description: desc.unwrap_or(old.description),
                city: text(f, "city").unwrap_or(old.city),
                region: text(f, "region").unwrap_or(old.region),
                country: text(f, "country").unwrap_or(old.country),
            };
            edit::set_location(b, name, &input)?;
        }
        SetupKind::Unit => {
            let old = b.units.get(name).cloned();
            let hours = match f.get("hours") {
                Some(Value::Null) => None,
                Some(v) => Some(formula_text(v)),
                None => old.as_ref().and_then(|o| o.hours).map(|h| h.to_string()),
            };
            let input = UnitInput {
                plural: text(f, "plural")
                    .or_else(|| old.as_ref().map(|o| o.plural.clone()))
                    .unwrap_or_else(|| name.to_string()),
                description: desc
                    .or_else(|| old.as_ref().map(|o| o.description.clone()))
                    .unwrap_or_default(),
                hours,
            };
            edit::set_unit(b, name, &input)?;
        }
        SetupKind::Currency => {
            let old = b.currencies.get(name).cloned();
            let rate = match (f.get("rate"), &old) {
                (Some(v), _) => formula_text(v),
                (None, Some(o)) => o.rate.to_string(),
                // The base currency is refused for being it.
                (None, None) if name == b.base_currency => String::new(),
                (None, None) => return Err(ToolError::invalid("a new currency needs a rate")),
            };
            let input = CurrencyInput {
                description: desc
                    .or_else(|| old.as_ref().map(|o| o.description.clone()))
                    .unwrap_or_default(),
                symbol: text(f, "symbol")
                    .or_else(|| old.as_ref().map(|o| o.symbol.clone()))
                    .unwrap_or_default(),
                rate,
                // How amounts in it are written (decisions.md, entry 61):
                // kept unless given.
                symbol_after: f.get("symbol_after").and_then(Value::as_bool),
                separators: text(f, "separators"),
            };
            edit::typed::set_currency(b, name, &input)?;
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
        let log = script::set_global(b, &name, &formula)?;
        if description.is_some() || unit.is_some() {
            let info = b.global_info.get(&name).cloned().unwrap_or_default();
            let input = GlobalInfoInput {
                description: description.clone().unwrap_or(info.description),
                unit: unit.clone().map_or(info.unit, Option::unwrap_or_default),
                decimals: info.decimals,
                show_by_name: info.show_by_name,
                folder: info.folder,
            };
            display::apply_global_info(b, &name, &input)?;
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

/// Deletes item `name` of `kind` with the app's delete, once its use
/// (lines, other) is known and allowed: anything in use needs `force`.
fn delete_item(
    b: &mut Budget,
    kind: SetupKind,
    name: &str,
    force: bool,
) -> Result<(usize, usize), ToolError> {
    let exists = match kind {
        SetupKind::Currency => b.currencies.contains_key(name),
        _ => method_core::rename::exists(b, kind, name),
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
    // Refusals force doesn't override.
    match kind {
        SetupKind::Group => typed::check_group_delete(b, name)?,
        SetupKind::Currency => typed::check_currency_delete(b, name)?,
        _ => {}
    }
    let u = method_core::usage::usage(b, kind)
        .get(name)
        .copied()
        .unwrap_or_default();
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
        SetupKind::Global => edit::delete_global(b, name)?,
        SetupKind::Fringe => edit::delete_fringe(b, name)?,
        SetupKind::Group => typed::delete_group(b, name)?,
        SetupKind::Set => edit::delete_set(b, name)?,
        SetupKind::Location => edit::delete_location(b, name)?,
        SetupKind::Unit => {
            edit::delete_unit_with(b, name, true)?;
            // Also blanked where settings name it, so nothing is left
            // naming a unit that isn't there (the app's delete leaves them).
            for f in b.fringes.values_mut() {
                if let FringeKind::Flat { unit, .. } = &mut f.kind {
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
        SetupKind::Currency => typed::delete_currency_with(b, name, true)?,
    }
    Ok((u.lines, u.other))
}

/// The charges' or credits' index of the one named `name`.
fn named<T>(
    list: &[T],
    name: &str,
    of: impl Fn(&T) -> &str,
    what: &str,
) -> Result<usize, ToolError> {
    list.iter()
        .position(|x| of(x) == name)
        .ok_or_else(|| ToolError::not_found(format!("there is no {what} {name}")))
}

/// An optional text argument: `Some(None)` when given as null or blank,
/// `None` when left out (keep it).
fn opt_arg(a: &Args, key: &str) -> Option<Option<String>> {
    a.has(key).then(|| {
        a.str(key)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    })
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
        let flat = match (a.str("charge_kind"), &old) {
            (Some(k), _) => k == "flat",
            (None, Some(o)) => matches!(o.kind, ChargeKind::Flat { .. }),
            (None, None) => {
                return Err(ToolError::invalid(
                    "a new charge needs charge_kind (percent or flat)",
                ))
            }
        };
        let (old_amount, old_currency, old_percent) = match old.as_ref().map(|o| &o.kind) {
            Some(ChargeKind::Flat { amount, currency }) => (Some(*amount), currency.clone(), None),
            Some(ChargeKind::Percent(p)) => (None, None, Some(*p)),
            None => (None, None, None),
        };
        let excluded_amounts = if a.has("excluded_amounts") {
            let mut v = Vec::new();
            for x in a.list("excluded_amounts") {
                v.push(ExcludedAmountInput {
                    description: x
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    amount: match x.get("amount") {
                        Some(v) => exact(v, "excluded amount")?,
                        None => money_text(0),
                    },
                });
            }
            v
        } else {
            old.iter()
                .flat_map(|o| &o.excluded_amounts)
                .map(|(description, c)| ExcludedAmountInput {
                    description: description.clone(),
                    amount: money_text(*c),
                })
                .collect()
        };
        let list = |key: &str, kept: Option<&BTreeSet<String>>| {
            if a.has(key) {
                a.strings(key)
            } else {
                kept.into_iter().flatten().cloned().collect()
            }
        };
        let input = ChargeInput {
            name: a
                .str("new_name")
                .map_or_else(|| name.clone(), str::to_string),
            account: opt_arg(&a, "account")
                .unwrap_or_else(|| old.as_ref().and_then(|o| o.account.clone())),
            applied: a
                .opt_bool("applied")
                .or(old.as_ref().map(|o| o.applied))
                .unwrap_or(true),
            kind: if flat { "flat" } else { "percent" }.to_string(),
            percent: match a.get("percent") {
                Some(v) => Some(formula_text(v)),
                None => old_percent.map(|p| p.to_string()),
            },
            amount: match a.get("amount") {
                Some(v) => Some(exact(v, "amount")?),
                None => old_amount.map(money_text),
            },
            currency: opt_arg(&a, "currency").unwrap_or(old_currency),
            excluded_amounts,
            excluded_groups: list("excluded_groups", old.as_ref().map(|o| &o.excluded_groups)),
            excluded_categories: list(
                "excluded_categories",
                old.as_ref().map(|o| &o.excluded_categories),
            ),
            excluded_accounts: list(
                "excluded_accounts",
                old.as_ref().map(|o| &o.excluded_accounts),
            ),
            label_percent: a.opt_bool("label_percent"),
            label_excluded: a.opt_bool("label_excluded"),
        };
        typed::set_charge(b, index, &input)?;
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

/// A credit row as the app's edit takes it.
fn credit_row_input(r: &CreditRow) -> CreditRowInput {
    CreditRowInput {
        description: r.description.clone(),
        source: match &r.source {
            CreditSource::Flat(c) => CreditSourceInput::Flat {
                amount: money_text(*c),
            },
            CreditSource::Group { group } => CreditSourceInput::Group {
                group: group.clone(),
            },
            CreditSource::Category { category } => CreditSourceInput::Category {
                category: category.clone(),
            },
            CreditSource::Account { account } => CreditSourceInput::Account {
                account: account.clone(),
            },
        },
        percent: r.percent.to_string(),
        cap: match r.cap {
            CreditCap::None => CreditCapInput::None,
            CreditCap::QualifiedSpend(c) => CreditCapInput::QualifiedSpend {
                amount: money_text(c),
            },
            CreditCap::Credit(c) => CreditCapInput::Credit {
                amount: money_text(c),
            },
        },
    }
}

/// Credit row `n` (from 1) from the tool's arguments.
fn credit_row_arg(r: &Value, n: usize) -> Result<CreditRowInput, ToolError> {
    let src = r.get("source").cloned().unwrap_or(Value::Null);
    let text = |key: &str| {
        src.get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let source = match src.get("kind").and_then(Value::as_str) {
        Some("group") => CreditSourceInput::Group {
            group: text("group"),
        },
        Some("category") => CreditSourceInput::Category {
            category: text("category"),
        },
        Some("account") => CreditSourceInput::Account {
            account: text("account"),
        },
        _ => {
            let amt = src.get("amount").ok_or_else(|| {
                ToolError::invalid(format!("row {n}: a flat source needs an amount"))
            })?;
            CreditSourceInput::Flat {
                amount: exact(amt, &format!("row {n}'s amount"))?,
            }
        }
    };
    let cap = match r.get("cap") {
        None => CreditCapInput::None,
        Some(c) => {
            let amount = || {
                exact(
                    c.get("amount").unwrap_or(&Value::Null),
                    &format!("row {n}'s cap"),
                )
            };
            match c.get("kind").and_then(Value::as_str) {
                Some("qualified_spend") => CreditCapInput::QualifiedSpend { amount: amount()? },
                Some("credit") => CreditCapInput::Credit { amount: amount()? },
                _ => CreditCapInput::None,
            }
        }
    };
    Ok(CreditRowInput {
        description: r
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        source,
        percent: formula_text(r.get("percent").unwrap_or(&Value::Null)),
        cap,
    })
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
        let rows = if a.has("rows") {
            let mut rows = Vec::new();
            for (i, r) in a.list("rows").iter().enumerate() {
                rows.push(credit_row_arg(r, i + 1)?);
            }
            rows
        } else {
            old.iter()
                .flat_map(|o| &o.rows)
                .map(credit_row_input)
                .collect()
        };
        let input = CreditInput {
            name: a
                .str("new_name")
                .map_or_else(|| name.clone(), str::to_string),
            account: opt_arg(&a, "account")
                .unwrap_or_else(|| old.as_ref().and_then(|o| o.account.clone())),
            applied: a
                .opt_bool("applied")
                .or(old.as_ref().map(|o| o.applied))
                .unwrap_or(true),
            cap: if a.has("cap") {
                a.get("cap").map(|v| exact(v, "cap")).transpose()?
            } else {
                old.as_ref().and_then(|o| o.cap).map(money_text)
            },
            discount_percent: match a.get("discount_percent") {
                Some(v) => Some(formula_text(v)),
                None => old.as_ref().map(|o| o.discount_percent.to_string()),
            },
            excluded_fringes: if a.has("excluded_fringes") {
                a.strings("excluded_fringes")
            } else {
                old.iter()
                    .flat_map(|o| &o.excluded_fringes)
                    .cloned()
                    .collect()
            },
            rows,
        };
        typed::set_credit(b, index, &input)?;
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
        if credit {
            let i = named(&b.credits, &name, |c| c.name.as_str(), what)?;
            edit::delete_credit(b, i)?;
        } else {
            let i = named(&b.charges, &name, |c| c.name.as_str(), what)?;
            edit::delete_charge(b, i)?;
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
            let i = named(&b.credits, &name, |c| c.name.as_str(), what)?;
            b.credits[i].applied = applied;
        } else {
            let i = named(&b.charges, &name, |c| c.name.as_str(), what)?;
            b.charges[i].applied = applied;
        }
        Ok(())
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

/// The topsheet index of category `code`.
fn category_at(b: &Budget, code: &str) -> Result<usize, ToolError> {
    b.topsheet
        .iter()
        .position(|r| matches!(r, TopsheetRow::Category(c) if c.code == code))
        .ok_or_else(|| ToolError::not_found(format!("there is no category {code}")))
}

/// The topsheet index of production total `name`.
fn total_at(b: &Budget, name: &str) -> Result<usize, ToolError> {
    b.topsheet
        .iter()
        .position(|r| matches!(r, TopsheetRow::ProductionTotal(p) if p.name == name))
        .ok_or_else(|| ToolError::not_found(format!("there is no production total {name}")))
}

fn add_category(ctx: &mut Ctx, a: &Args) -> ToolResult {
    let (path, shown) = budget_path(ctx, a, "path")?;
    let typed = a.req("code")?.to_string();
    let name = a.req("name")?.trim().to_string();
    let after = a.str("after_category").map(str::to_string);
    let before_total = a.str("before_total").map(str::to_string);
    let at_start = a.bool("at_start");
    let given =
        usize::from(after.is_some()) + usize::from(before_total.is_some()) + usize::from(at_start);
    if given > 1 {
        return Err(ToolError::invalid(
            "give one of after_category, before_total and at_start",
        ));
    }
    let e = ctx
        .docs
        .edit(&path, &shown, &format!("Add category {typed}"), |b| {
            let at = if let Some(c) = &after {
                category_at(b, c)? + 1
            } else if let Some(t) = &before_total {
                total_at(b, t)?
            } else if at_start {
                0
            } else {
                b.topsheet.len()
            };
            let i = edit::typed::add_category(b, &typed, &name, Some(at))?;
            match &b.topsheet[i] {
                TopsheetRow::Category(c) => Ok(c.code.clone()),
                TopsheetRow::ProductionTotal(_) => unreachable!("a category was added"),
            }
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
            let at = match &after {
                Some(c) => category_at(b, c)? + 1,
                None => b.topsheet.len(),
            };
            typed::add_production_total(b, &name, Some(at))?;
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
            let i = total_at(b, &name)?;
            typed::delete_topsheet_row(b, i)?;
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
            Ok(edit::typed::add_account(
                b,
                &category,
                &typed,
                &name,
                after.as_deref(),
            )?)
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
            let i = category_at(b, &code)?;
            let TopsheetRow::Category(c) = &b.topsheet[i] else {
                unreachable!("found as a category")
            };
            let name = name.clone().unwrap_or_else(|| c.name.clone());
            typed::update_topsheet_row(b, i, new_code.as_deref(), &name)?;
            match &b.topsheet[i] {
                TopsheetRow::Category(c) => Ok(c.code.clone()),
                TopsheetRow::ProductionTotal(_) => unreachable!("still a category"),
            }
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
            let current = b
                .categories()
                .flat_map(|c| &c.accounts)
                .find(|x| x.code == code)
                .map(|x| x.name.clone());
            let name = name.clone().or(current).unwrap_or_default();
            let new_code = new_code.as_deref().unwrap_or(&code);
            Ok(typed::update_account(b, &code, new_code, &name)?)
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
            let i = category_at(b, &code)?;
            let TopsheetRow::Category(c) = &b.topsheet[i] else {
                unreachable!("found as a category")
            };
            let rows: usize = c.accounts.iter().map(|x| x.details.len()).sum();
            if rows > 0 && !force {
                return Err(ToolError::refused(format!(
                    "category {code} has {} account(s) with {rows} row(s); nothing was deleted. \
                     Pass force: true to delete them with it.",
                    c.accounts.len()
                )));
            }
            typed::delete_topsheet_row(b, i)?;
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
            let rows = b
                .categories()
                .flat_map(|c| &c.accounts)
                .find(|x| x.code == code)
                .map(|x| x.details.len())
                .ok_or_else(|| ToolError::not_found(format!("there is no account {code}")))?;
            if rows > 0 && !force {
                return Err(ToolError::refused(format!(
                    "account {code} has {rows} row(s); nothing was deleted. Pass force: true to \
                     delete them with it."
                )));
            }
            typed::delete_account(b, &code)?;
            Ok(rows)
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
    let title = a.req("title")?.to_string();
    let e = ctx.docs.edit(&path, &shown, "Set title", |b| {
        Ok(edit::set_title(b, &title)?)
    })?;
    Ok(edited(&shown, &e, "Set the title", Map::new()))
}
