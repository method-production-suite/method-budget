//! Quick entry on the detail grid.
//!
//! - **Adding setup items from the cell**: a unit, set, location or currency
//!   typed into a cell that the budget doesn't have yet, or a global named in
//!   a formula, is added to its setup table and the cell is set, as one undo
//!   step (`set_cell_creating`). Either both happen or, when anything is
//!   wrong, neither does.
//! - **New-line defaults**: which tags an inserted line copies from its
//!   neighbor (`set_new_line_defaults`; the copying itself is in
//!   [`crate::edit::add_detail`]).
//!
//! Both commands run through `Session::edit`, so they work on a copy of the
//! budget and a rejected one changes nothing.

use method_core::{Budget, NewLineDefaults, RowId};
use serde::Deserialize;

use crate::edit::{self, CurrencyInput, EditResult, Field, LocationInput, SetInput, UnitInput};
use crate::session::Session;

/// The setup item to add along with a cell edit.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInput {
    /// `unit`, `set`, `location`, `currency` or `global`.
    pub kind: String,
    /// Its name (a currency's code).
    pub name: String,
    /// A global's formula (required); a currency's exchange rate (base
    /// units per one unit of it; blank or missing is 1). Unused otherwise.
    #[serde(default)]
    pub value: Option<String>,
}

/// What a column's values name, if they name a setup item that can be added
/// from the cell.
pub fn creatable_kind(field: Field) -> Option<&'static str> {
    match field {
        Field::Unit | Field::Unit2 | Field::Unit3 | Field::Unit4 => Some("unit"),
        Field::Set => Some("set"),
        Field::Location => Some("location"),
        Field::Currency => Some("currency"),
        Field::Amount | Field::X | Field::Rate | Field::X4 => Some("global"),
        Field::Description | Field::Fringes | Field::Groups | Field::Person => None,
    }
}

fn exists(b: &Budget, kind: &str, name: &str) -> bool {
    match kind {
        "unit" => b.units.contains_key(name),
        "set" => b.sets.contains_key(name),
        "location" => b.locations.contains_key(name),
        "currency" => name == b.base_currency || b.currencies.contains_key(name),
        "global" => b.globals.contains_key(name),
        _ => false,
    }
}

/// Adds the setup item `create` describes, then sets `field` of row `id` to
/// `value`. Refused, with nothing changed, when the item already exists, the
/// column doesn't name that kind of item, the item is invalid, or the row
/// can't take the value.
pub fn create_and_set(
    b: &mut Budget,
    id: RowId,
    field: Field,
    value: &str,
    create: &CreateInput,
) -> EditResult {
    let kind = create.kind.as_str();
    let name = create.name.trim();
    match creatable_kind(field) {
        Some(k) if k == kind => {}
        Some(_) | None => {
            return Err(format!("a {kind} can't be added from this column"));
        }
    }
    if name.is_empty() {
        return Err(format!("the new {kind} needs a name"));
    }
    if exists(b, kind, name) {
        return Err(format!("{kind} {name} already exists"));
    }
    // The item first, since a cell may only name what exists; on a copy, so
    // a cell that can't take the value adds nothing.
    let mut next = b.clone();
    add_item(&mut next, kind, name, create)?;
    edit::set_detail_field(&mut next, id, field, value)?;
    *b = next;
    Ok(())
}

/// Adds item `name` of `kind` with what the cell's form gives.
fn add_item(b: &mut Budget, kind: &str, name: &str, create: &CreateInput) -> EditResult {
    let extra = create.value.as_deref().map(str::trim).unwrap_or("");
    match kind {
        "unit" => edit::set_unit(
            b,
            name,
            &UnitInput {
                plural: String::new(),
                description: String::new(),
                hours: None,
            },
        ),
        "set" => edit::set_set(
            b,
            name,
            &SetInput {
                description: String::new(),
            },
        ),
        "location" => edit::set_location(
            b,
            name,
            &LocationInput {
                description: String::new(),
                city: String::new(),
                region: String::new(),
                country: String::new(),
            },
        ),
        "currency" => edit::set_currency(
            b,
            name,
            &CurrencyInput {
                description: String::new(),
                symbol: String::new(),
                rate: if extra.is_empty() { "1" } else { extra }.to_string(),
                symbol_after: None,
                separators: None,
            },
        ),
        "global" => edit::set_global(b, name, extra),
        other => Err(format!("unknown kind of setup item `{other}`")),
    }
}

/// The switches for what an inserted line copies from its neighbor.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewLineDefaultsInput {
    pub fringes: bool,
    pub groups: bool,
    pub set: bool,
    pub location: bool,
    pub currency: bool,
}

pub fn apply_new_line_defaults(b: &mut Budget, d: &NewLineDefaultsInput) -> EditResult {
    b.new_line_defaults = NewLineDefaults {
        fringes: d.fringes,
        groups: d.groups,
        set: d.set,
        location: d.location,
        currency: d.currency,
    };
    Ok(())
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/quickadd.rs and the host's src/host/quickadd.rs
// call these)

/// Adds the setup item `create` describes and sets `field` of row `row_id`
/// to `value`, as one undo step named like any cell edit.
pub fn set_cell_creating(
    s: &mut Session,
    row_id: RowId,
    field: &str,
    value: &str,
    create: &CreateInput,
) -> EditResult {
    let parsed = Field::parse(field)?;
    let label = crate::history::cell_label(s.budget(), row_id, field);
    s.edit(label, |b| create_and_set(b, row_id, parsed, value, create))
}

/// Sets which tags an inserted line copies from its neighbor.
pub fn set_new_line_defaults(s: &mut Session, defaults: &NewLineDefaultsInput) -> EditResult {
    s.edit("Change new line defaults", |b| {
        apply_new_line_defaults(b, defaults)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::{Detail, RowKind};

    fn create(kind: &str, name: &str, value: Option<&str>) -> CreateInput {
        CreateInput {
            kind: kind.into(),
            name: name.into(),
            value: value.map(String::from),
        }
    }

    /// The first account with at least two lines: (code, its row ids).
    fn lines(s: &Session) -> (String, Vec<RowId>) {
        let a = s
            .budget()
            .categories()
            .flat_map(|c| &c.accounts)
            .find(|a| a.details.iter().filter(|d| d.kind == RowKind::Line).count() >= 2)
            .unwrap();
        (a.code.clone(), a.details.iter().map(|d| d.id).collect())
    }

    fn row(s: &Session, id: RowId) -> &Detail {
        edit::details(s.budget()).find(|d| d.id == id).unwrap()
    }

    fn quick(s: &mut Session, id: RowId, field: &str, value: &str, c: &CreateInput) -> EditResult {
        let field = Field::parse(field).unwrap();
        s.edit("Test edit", |b| create_and_set(b, id, field, value, c))
    }

    #[test]
    fn a_new_unit_and_the_cell_are_one_undo_step() {
        let mut s = Session::new();
        let (_, ids) = lines(&s);
        let id = ids[0];
        let before = s.budget().clone();
        let steps = |s: &Session| s.can_undo();
        assert!(!steps(&s));
        quick(
            &mut s,
            id,
            "unit",
            "Man-day",
            &create("unit", "Man-day", None),
        )
        .unwrap();
        assert!(s.budget().units.contains_key("Man-day"));
        assert_eq!(row(&s, id).unit, "Man-day");
        // The other unit columns take units too.
        quick(
            &mut s,
            id,
            "unit3",
            "Per 12h",
            &create("unit", " Per 12h ", None),
        )
        .unwrap();
        assert!(
            s.budget().units.contains_key("Per 12h"),
            "names are trimmed"
        );
        assert_eq!(row(&s, id).unit3, "Per 12h");

        assert!(s.undo());
        assert!(!s.budget().units.contains_key("Per 12h"));
        assert!(s.undo(), "one step for the first add");
        assert_eq!(s.budget(), &before, "both the unit and the cell are back");
        assert!(!s.can_undo());
        assert!(s.redo());
        assert_eq!(row(&s, id).unit, "Man-day");
        assert!(s.budget().units.contains_key("Man-day"));
    }

    #[test]
    fn sets_locations_and_currencies_are_added_from_their_cells() {
        let mut s = Session::new();
        let (_, ids) = lines(&s);
        let id = ids[0];
        quick(&mut s, id, "set", "Diner", &create("set", "Diner", None)).unwrap();
        quick(
            &mut s,
            id,
            "location",
            "Tulsa",
            &create("location", "Tulsa", None),
        )
        .unwrap();
        quick(
            &mut s,
            id,
            "currency",
            "EUR",
            &create("currency", "EUR", None),
        )
        .unwrap();
        let b = s.budget();
        assert!(b.sets.contains_key("Diner") && b.locations.contains_key("Tulsa"));
        assert_eq!(
            b.currencies["EUR"].rate,
            method_core::Dec::from_int(1),
            "rate 1 unless given"
        );
        let d = row(&s, id);
        assert_eq!(
            (
                d.set.as_deref(),
                d.location.as_deref(),
                d.currency.as_deref()
            ),
            (Some("Diner"), Some("Tulsa"), Some("EUR"))
        );
        quick(
            &mut s,
            ids[1],
            "currency",
            "GBP",
            &create("currency", "GBP", Some("1.27")),
        )
        .unwrap();
        assert_eq!(s.budget().currencies["GBP"].rate.to_string(), "1.27");
        // Totals stay whole: the new currency has a rate, so nothing is flagged.
        assert!(s.view().issues.is_empty());
    }

    #[test]
    fn a_global_named_in_a_formula_is_added_with_its_value() {
        let mut s = Session::new();
        let (account, ids) = lines(&s);
        let id = ids[0];
        let total = |s: &mut Session| {
            let v = serde_json::to_value(s.full_view()).unwrap();
            v["report"]["categories"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|c| c["accounts"].as_array().unwrap().clone())
                .find(|a| a["code"] == account.as_str())
                .unwrap()["rows"][0]["total"]
                .as_i64()
                .unwrap()
        };
        s.set_detail_field(id, "rate", "100").unwrap();
        quick(
            &mut s,
            id,
            "amount",
            "PREP_DAYS * 2",
            &create("global", "PREP_DAYS", Some("3")),
        )
        .unwrap();
        assert_eq!(s.budget().globals["PREP_DAYS"].0, "3");
        assert_eq!(row(&s, id).amount.0, "PREP_DAYS * 2");
        assert!(s.view().issues.is_empty());
        let x = row(&s, id).x.0.clone();
        let x: i64 = if x.trim().is_empty() {
            1
        } else {
            x.trim().parse().unwrap()
        };
        assert_eq!(total(&mut s), 6 * 100 * 100 * x, "3 * 2 at 100.00");
        s.undo();
        assert!(!s.budget().globals.contains_key("PREP_DAYS"));
        assert_ne!(row(&s, id).amount.0, "PREP_DAYS * 2");
    }

    #[test]
    fn refusals_change_nothing() {
        let mut s = Session::new();
        let (_, ids) = lines(&s);
        let id = ids[0];
        quick(&mut s, id, "unit", "Gig", &create("unit", "Gig", None)).unwrap();
        let before = s.budget().clone();
        let cases: Vec<(&str, &str, CreateInput)> = vec![
            // Already there.
            ("unit", "Day", create("unit", "Day", None)),
            ("unit2", "Gig", create("unit", "Gig", None)),
            ("currency", "USD", create("currency", "USD", None)),
            // The wrong column for the kind.
            ("rate", "Week", create("unit", "Week2", None)),
            ("set", "X", create("location", "X", None)),
            ("description", "Y", create("set", "Y", None)),
            ("fringes", "NEWFR", create("global", "NEWFR", Some("1"))),
            // Not valid items.
            ("unit", "", create("unit", "  ", None)),
            ("set", "A,B", create("set", "A,B", None)),
            ("amount", "2 DAYS", create("global", "2 DAYS", Some("1"))),
            ("amount", "NEWG", create("global", "NEWG", None)),
            ("amount", "NEWG", create("global", "NEWG", Some("  "))),
            ("currency", "JPY", create("currency", "JPY", Some("0"))),
            ("currency", "JPY", create("currency", "JPY", Some("abc"))),
            ("unit", "Thing", create("widget", "Thing", None)),
        ];
        for (field, value, c) in &cases {
            let r = quick(&mut s, id, field, value, c);
            assert!(r.is_err(), "{field} {value:?} {c:?} should be refused");
            assert_eq!(s.budget(), &before, "{field} {value:?}: nothing changed");
        }
        // No undo steps were added: one undo goes back past the unit added above.
        assert!(s.undo());
        assert!(!s.budget().units.contains_key("Gig"));
        assert!(!s.can_undo());
        s.redo();
        // A row that is gone: the item isn't added either.
        assert!(quick(
            &mut s,
            99_999,
            "unit",
            "Orphan",
            &create("unit", "Orphan", None)
        )
        .is_err());
        assert!(!s.budget().units.contains_key("Orphan"));
        // A row that doesn't use the column: nothing added.
        let sub = s.add_detail(&lines(&s).0, None, "subtotal").unwrap();
        let before = s.budget().clone();
        assert!(quick(&mut s, sub, "unit", "Lump", &create("unit", "Lump", None)).is_err());
        assert_eq!(s.budget(), &before);
    }

    #[test]
    fn inserted_lines_copy_what_the_new_line_defaults_ask_for() {
        let mut s = Session::new();
        let (account, ids) = lines(&s);
        let first = ids[0];
        // Give the line above some tags of every kind.
        s.edit("Test edit", |b| {
            b.groups.insert(
                "Shoot".into(),
                method_core::Group {
                    short_code: None,
                    description: String::new(),
                    in_budget_total: true,
                    color: None,
                },
            );
            edit::set_set(
                b,
                "Diner",
                &SetInput {
                    description: String::new(),
                },
            )?;
            edit::set_location(
                b,
                "Tulsa",
                &LocationInput {
                    description: String::new(),
                    city: String::new(),
                    region: String::new(),
                    country: String::new(),
                },
            )?;
            edit::set_currency(
                b,
                "EUR",
                &CurrencyInput {
                    description: String::new(),
                    symbol: String::new(),
                    rate: "1.1".into(),
                    symbol_after: None,
                    separators: None,
                },
            )?;
            let fringe = b.fringes.keys().next().cloned().unwrap();
            edit::set_detail_field(b, first, Field::Fringes, &fringe)?;
            edit::set_detail_field(b, first, Field::Groups, "Shoot")?;
            edit::set_detail_field(b, first, Field::Set, "Diner")?;
            edit::set_detail_field(b, first, Field::Location, "Tulsa")?;
            edit::set_detail_field(b, first, Field::Currency, "EUR")
        })
        .unwrap();
        let above = row(&s, first).clone();
        assert!(!above.fringes.is_empty());

        // By default: fringes only, and the description starts empty.
        assert_eq!(s.budget().new_line_defaults, NewLineDefaults::default());
        let a = s.add_detail(&account, Some(first), "line").unwrap();
        let d = row(&s, a);
        assert_eq!(d.fringes, above.fringes);
        assert!(d.groups.is_empty() && d.set.is_none() && d.location.is_none());
        assert!(d.currency.is_none() && d.description.is_empty());
        assert!(d.amount.is_blank() && d.rate.is_blank());

        // One undo step takes the inserted line away.
        s.undo();
        assert!(edit::details(s.budget()).all(|d| d.id != a));

        // Every switch on.
        let all = NewLineDefaultsInput {
            fringes: true,
            groups: true,
            set: true,
            location: true,
            currency: true,
        };
        s.edit("Test edit", |b| apply_new_line_defaults(b, &all))
            .unwrap();
        let b2 = s.add_detail(&account, Some(first), "line").unwrap();
        let d = row(&s, b2);
        assert_eq!(
            (&d.fringes, &d.groups, &d.set, &d.location, &d.currency),
            (
                &above.fringes,
                &above.groups,
                &above.set,
                &above.location,
                &above.currency
            )
        );
        assert_eq!(d.fringe_range, None, "fringe ranges are never copied");

        // Every switch off: a blank line. Other kinds never copy.
        let none = NewLineDefaultsInput {
            fringes: false,
            groups: false,
            set: false,
            location: false,
            currency: false,
        };
        s.edit("Test edit", |b| apply_new_line_defaults(b, &none))
            .unwrap();
        let c = s.add_detail(&account, Some(first), "line").unwrap();
        assert!(row(&s, c).fringes.is_empty() && row(&s, c).groups.is_empty());
        s.edit("Test edit", |b| apply_new_line_defaults(b, &all))
            .unwrap();
        let adj = s.add_detail(&account, Some(first), "adjustment").unwrap();
        assert!(row(&s, adj).groups.is_empty() && row(&s, adj).set.is_none());

        // The setting is one undo step and is saved with the file.
        s.undo();
        s.undo();
        assert!(
            !s.budget().new_line_defaults.groups,
            "back to every switch off"
        );
        let text = method_format::to_string(s.budget());
        assert!(text.contains("\"new_line_defaults\""));
        let back = method_format::from_str(&text).unwrap();
        assert_eq!(back.new_line_defaults, s.budget().new_line_defaults);
    }

    #[test]
    fn the_neighbor_is_the_nearest_line_above_else_below() {
        let tagged = |id: RowId, fringe: &str| {
            let mut d = Detail::line(id, "", "", "", "");
            d.fringes.insert(fringe.into());
            d
        };
        let rows = vec![
            Detail::subtotal(1, ""),
            tagged(2, "A"),
            Detail::separator(3),
            Detail::adjustment(4, "", ""),
            tagged(5, "B"),
        ];
        let d = NewLineDefaults::default();
        let pick = |at: usize| {
            let mut row = Detail::line(99, "", "", "", "");
            edit::copy_from_neighbor(&mut row, &rows, at, d);
            row.fringes.into_iter().collect::<Vec<_>>().join(",")
        };
        assert_eq!(pick(0), "A", "nothing above: the first line below");
        assert_eq!(pick(1), "A");
        assert_eq!(pick(2), "A");
        assert_eq!(pick(4), "A", "passes over the separator and the adjustment");
        assert_eq!(pick(5), "B");
        assert_eq!(pick(50), "B", "past the end is the end");
        let mut row = Detail::line(99, "", "", "", "");
        edit::copy_from_neighbor(&mut row, &[Detail::subtotal(1, "")], 1, d);
        assert!(row.fringes.is_empty(), "no line at all: nothing copied");
    }
}
