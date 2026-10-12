//! Renaming a setup item and every reference to it (decision 7: names stay in
//! the file, a rename rewrites the references).
//!
//! What refers to what, by name:
//!
//! - a **global**: other globals' formulas, fringe rates, and the Amount, X,
//!   Rate and 4X formulas of every row, as whole name tokens (the way the
//!   formula reader splits them, so `DAYS` never touches `SHOOT_DAYS`); the
//!   rest of each formula is kept as typed; and report presets' header and
//!   footer slots that show its value;
//! - a **fringe**: rows' fringes, and credit rows' excluded fringes;
//! - a **group**: rows' groups, charges' excluded groups, credit rows based on
//!   it, and report presets' "only these groups" filters (its color and
//!   other settings go with it);
//! - a **unit**: rows' four unit columns, flat fringes' unit, and globals'
//!   display unit ([`crate::model::GlobalInfo`]);
//! - a **set** or a **location**: rows' set or location;
//! - a **currency**: rows' currency, flat charges' currency and the totals
//!   currency. The base currency can be renamed too (the budget's base code
//!   changes with it).
//!
//! Tag presets and line blocks (`crate::presets`) are part of the budget
//! file, so they follow too: a tag preset's fringes, groups, set, location
//! and currency; a line block's rows, exactly as the budget's own rows; and
//! the block's saved copies of the setup its rows use. Those copies are
//! matched to the rows by name (they are what an insert adds back when the
//! budget no longer has an item), so a copy under the old name is filed under
//! the new one, and the copies' own references follow (a saved global's
//! formula, a saved fringe's rate and flat unit). Their definitions are left
//! as they were saved.
//!
//! The new name must be a valid name for its kind, and unused: not another
//! item of that kind, and not referred to anywhere already (a dangling
//! reference, like a unit typed on a line but not in the table). So a rename
//! never merges two names into one, and renaming back always gives the
//! identical budget. Totals never change.
//!
//! Saved versions keep the names they were saved with: they are a record of
//! the budget as it was, and restoring one brings its own names back.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::expr::{is_ident, rename_var};
use crate::model::{Budget, ChargeKind, CreditSource, Detail, Formula, FringeKind, TopsheetRow};
use crate::presets::{SetupItems, TagPreset};
use crate::report_preset::{BandField, ReportSpec};

/// The kinds of setup item that are referred to by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SetupKind {
    Global,
    Fringe,
    Group,
    Unit,
    Set,
    Location,
    Currency,
}

impl SetupKind {
    /// Every kind, in a stable order.
    pub const ALL: &'static [SetupKind] = &[
        SetupKind::Global,
        SetupKind::Fringe,
        SetupKind::Group,
        SetupKind::Unit,
        SetupKind::Set,
        SetupKind::Location,
        SetupKind::Currency,
    ];

    /// The name the application uses for the kind (`"global"`, `"fringe"`...).
    pub fn name(self) -> &'static str {
        match self {
            SetupKind::Global => "global",
            SetupKind::Fringe => "fringe",
            SetupKind::Group => "group",
            SetupKind::Unit => "unit",
            SetupKind::Set => "set",
            SetupKind::Location => "location",
            SetupKind::Currency => "currency",
        }
    }

    /// The kind with this [`SetupKind::name`].
    pub fn from_name(s: &str) -> Option<SetupKind> {
        SetupKind::ALL.iter().copied().find(|k| k.name() == s)
    }
}

/// Why a rename was refused. Nothing changes when one is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameError {
    /// There is no such item.
    Missing { kind: SetupKind, name: String },
    /// The new name is the old one.
    Same,
    /// The new name can't be a name of this kind.
    BadName { kind: SetupKind, name: String },
    /// Another item of this kind already has the new name.
    Taken { kind: SetupKind, name: String },
    /// The new name is already referred to in this many places, without an
    /// item behind it. Renaming onto it would merge the two.
    InUse {
        kind: SetupKind,
        name: String,
        places: usize,
    },
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenameError::Missing { kind, name } => {
                write!(f, "{} {name} no longer exists", kind.name())
            }
            RenameError::Same => f.write_str("that is already its name"),
            RenameError::BadName { kind, name } => match kind {
                SetupKind::Global => write!(
                    f,
                    "`{name}` can't be a global name: use letters, digits and _, with at least one letter or _"
                ),
                _ if name.is_empty() => write!(f, "the new {} name is empty", kind.name()),
                _ => write!(f, "{} names can't contain commas", kind.name()),
            },
            RenameError::Taken { kind, name } => match kind {
                SetupKind::Currency => write!(f, "{name} is already a currency here"),
                _ => write!(f, "there is already a {} named {name}", kind.name()),
            },
            RenameError::InUse {
                kind,
                name,
                places,
            } => {
                let s = if *places == 1 { "place" } else { "places" };
                write!(
                    f,
                    "{name} is already used in {places} {s} without a {} of that name; pick another name",
                    kind.name()
                )
            }
        }
    }
}

impl std::error::Error for RenameError {}

/// Whether `name` (already trimmed) can be a name of this kind: a global's
/// name is letters, digits and `_` with at least one letter or `_` (it may
/// start with digits, `1ZG`; decisions.md, entry 56); other names
/// are any text without commas (lines list several fringes or groups
/// separated by commas).
pub fn valid_name(kind: SetupKind, name: &str) -> bool {
    match kind {
        SetupKind::Global => is_ident(name),
        _ => !name.is_empty() && !name.contains(','),
    }
}

/// Whether the budget has an item of this kind named `name` (the base
/// currency counts as a currency).
pub fn exists(b: &Budget, kind: SetupKind, name: &str) -> bool {
    match kind {
        SetupKind::Global => b.globals.contains_key(name),
        SetupKind::Fringe => b.fringes.contains_key(name),
        SetupKind::Group => b.groups.contains_key(name),
        SetupKind::Unit => b.units.contains_key(name),
        SetupKind::Set => b.sets.contains_key(name),
        SetupKind::Location => b.locations.contains_key(name),
        SetupKind::Currency => b.base_currency == name || b.currencies.contains_key(name),
    }
}

/// How many places refer to `name` as an item of this kind (a formula
/// counts once however often it names the global; each unit column counts
/// on its own; a line block's saved copy of the item counts once). The
/// item's own entry in its table isn't counted.
pub fn references(b: &Budget, kind: SetupKind, name: &str) -> usize {
    let has = |s: &Option<String>| s.as_deref() == Some(name);
    let rows = b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .chain(b.line_presets.iter().flat_map(|p| &p.rows))
        .map(|d| row_refs(d, kind, name))
        .sum::<usize>();
    let tags = b
        .tag_presets
        .iter()
        .map(|t| match kind {
            SetupKind::Fringe => usize::from(t.fringes.contains(name)),
            SetupKind::Group => usize::from(t.groups.contains(name)),
            SetupKind::Set => usize::from(has(&t.set)),
            SetupKind::Location => usize::from(has(&t.location)),
            SetupKind::Currency => usize::from(has(&t.currency)),
            SetupKind::Global | SetupKind::Unit => 0,
        })
        .sum::<usize>();
    let copies = b
        .line_presets
        .iter()
        .map(|p| setup_refs(&p.setup, kind, name))
        .sum::<usize>();
    let own = match kind {
        SetupKind::Global => {
            let uses = |f: &Formula| uses_global(f, name);
            b.globals.values().filter(|f| uses(f)).count()
                + b.fringes.values().filter(|f| uses(&f.rate)).count()
                + b.report_presets
                    .iter()
                    .flat_map(|p| band_fields(&p.spec))
                    .filter(|f| matches!(f, BandField::Global(g) if g == name))
                    .count()
        }
        SetupKind::Fringe => b
            .credits
            .iter()
            .filter(|c| c.excluded_fringes.contains(name))
            .count(),
        SetupKind::Group => {
            b.charges
                .iter()
                .filter(|c| c.excluded_groups.contains(name))
                .count()
                + b.credits
                    .iter()
                    .flat_map(|c| &c.rows)
                    .filter(
                        |r| matches!(&r.source, CreditSource::Group { group, .. } if group == name),
                    )
                    .count()
                + b.report_presets
                    .iter()
                    .filter(|p| p.spec.only_groups.contains(name))
                    .count()
        }
        SetupKind::Unit => {
            b.fringes
                .values()
                .filter(|f| matches!(&f.kind, FringeKind::Flat { unit, .. } if has(unit)))
                .count()
                + b.global_info.values().filter(|g| g.unit == name).count()
        }
        SetupKind::Set | SetupKind::Location => 0,
        SetupKind::Currency => {
            b.charges
                .iter()
                .filter(|c| matches!(&c.kind, ChargeKind::Flat { currency, .. } if has(currency)))
                .count()
                + usize::from(has(&b.totals_currency))
        }
    };
    rows + tags + copies + own
}

fn uses_global(f: &Formula, name: &str) -> bool {
    crate::usage::formula_names(&f.0).any(|n| n == name)
}

/// How many of one row's fields name `name` as an item of this kind.
fn row_refs(d: &Detail, kind: SetupKind, name: &str) -> usize {
    let has = |s: &Option<String>| s.as_deref() == Some(name);
    match kind {
        SetupKind::Global => [&d.amount, &d.x, &d.rate, &d.x4]
            .into_iter()
            .filter(|f| uses_global(f, name))
            .count(),
        SetupKind::Fringe => usize::from(d.fringes.contains(name)),
        SetupKind::Group => usize::from(d.groups.contains(name)),
        SetupKind::Unit => [&d.unit, &d.unit2, &d.unit3, &d.unit4]
            .into_iter()
            .filter(|u| *u == name)
            .count(),
        SetupKind::Set => usize::from(has(&d.set)),
        SetupKind::Location => usize::from(has(&d.location)),
        SetupKind::Currency => usize::from(has(&d.currency)),
    }
}

/// How many places in a line block's saved setup copies name `name`: the
/// copy of that item itself, and the copies that refer to it.
fn setup_refs(s: &SetupItems, kind: SetupKind, name: &str) -> usize {
    match kind {
        SetupKind::Global => {
            usize::from(s.globals.contains_key(name))
                + s.globals.values().filter(|f| uses_global(f, name)).count()
                + s.fringes.values().filter(|f| uses_global(&f.rate, name)).count()
        }
        SetupKind::Fringe => usize::from(s.fringes.contains_key(name)),
        SetupKind::Group => usize::from(s.groups.contains_key(name)),
        SetupKind::Unit => {
            usize::from(s.units.contains_key(name))
                + s.fringes
                    .values()
                    .filter(|f| matches!(&f.kind, FringeKind::Flat { unit, .. } if unit.as_deref() == Some(name)))
                    .count()
        }
        SetupKind::Set => usize::from(s.sets.contains_key(name)),
        SetupKind::Location => usize::from(s.locations.contains_key(name)),
        SetupKind::Currency => usize::from(s.currencies.contains_key(name)),
    }
}

/// Renames setup item `from` (of `kind`) to `to` and rewrites every reference
/// to it, as one change. `to` is trimmed first. Returns how many places were
/// rewritten (see [`references`] for how they are counted). On an error
/// nothing has changed.
pub fn rename(b: &mut Budget, kind: SetupKind, from: &str, to: &str) -> Result<usize, RenameError> {
    let to = to.trim();
    if !exists(b, kind, from) {
        return Err(RenameError::Missing {
            kind,
            name: from.to_string(),
        });
    }
    if to == from {
        return Err(RenameError::Same);
    }
    if !valid_name(kind, to) {
        return Err(RenameError::BadName {
            kind,
            name: to.to_string(),
        });
    }
    if exists(b, kind, to) {
        return Err(RenameError::Taken {
            kind,
            name: to.to_string(),
        });
    }
    let places = references(b, kind, to);
    if places > 0 {
        return Err(RenameError::InUse {
            kind,
            name: to.to_string(),
            places,
        });
    }
    Ok(rewrite(b, kind, from, to))
}

/// Moves `from`'s entry to `to` in a table, keeping its definition.
fn rekey<V>(map: &mut BTreeMap<String, V>, from: &str, to: &str) {
    if let Some(v) = map.remove(from) {
        map.insert(to.to_string(), v);
    }
}

/// Swaps `from` for `to` in a set of names; true when it was there.
fn swap_in(set: &mut BTreeSet<String>, from: &str, to: &str) -> bool {
    if set.remove(from) {
        set.insert(to.to_string());
        true
    } else {
        false
    }
}

/// `Some(from)` becomes `Some(to)`; true when it did.
fn swap_opt(v: &mut Option<String>, from: &str, to: &str) -> bool {
    if v.as_deref() == Some(from) {
        *v = Some(to.to_string());
        true
    } else {
        false
    }
}

/// Renames global `from` inside one formula; true when it named it.
fn swap_formula(f: &mut Formula, from: &str, to: &str) -> bool {
    let (text, n) = rename_var(&f.0, from, to);
    if n > 0 {
        f.0 = text;
    }
    n > 0
}

/// Moves `from`'s entry to `to` in a table; true when it was there.
fn rekey_hit<V>(map: &mut BTreeMap<String, V>, from: &str, to: &str) -> bool {
    match map.remove(from) {
        Some(v) => {
            map.insert(to.to_string(), v);
            true
        }
        None => false,
    }
}

/// Every header and footer slot that shows something.
fn band_fields(s: &ReportSpec) -> impl Iterator<Item = &BandField> {
    [&s.header, &s.footer]
        .into_iter()
        .flatten()
        .flat_map(|b| [&b.left, &b.center, &b.right])
        .flatten()
}

/// Renames in one row's fields; returns how many changed.
fn row_rewrite(d: &mut Detail, kind: SetupKind, from: &str, to: &str) -> usize {
    let mut n = 0;
    match kind {
        SetupKind::Global => {
            for f in [&mut d.amount, &mut d.x, &mut d.rate, &mut d.x4] {
                n += usize::from(swap_formula(f, from, to));
            }
        }
        SetupKind::Fringe => n += usize::from(swap_in(&mut d.fringes, from, to)),
        SetupKind::Group => n += usize::from(swap_in(&mut d.groups, from, to)),
        SetupKind::Unit => {
            for u in [&mut d.unit, &mut d.unit2, &mut d.unit3, &mut d.unit4] {
                if u == from {
                    *u = to.to_string();
                    n += 1;
                }
            }
        }
        SetupKind::Set => n += usize::from(swap_opt(&mut d.set, from, to)),
        SetupKind::Location => n += usize::from(swap_opt(&mut d.location, from, to)),
        SetupKind::Currency => n += usize::from(swap_opt(&mut d.currency, from, to)),
    }
    n
}

/// Renames in a line block's saved setup copies; returns how many changed.
fn setup_rewrite(s: &mut SetupItems, kind: SetupKind, from: &str, to: &str) -> usize {
    let mut n = 0;
    match kind {
        SetupKind::Global => {
            n += usize::from(rekey_hit(&mut s.globals, from, to));
            for f in s.globals.values_mut() {
                n += usize::from(swap_formula(f, from, to));
            }
            for fr in s.fringes.values_mut() {
                n += usize::from(swap_formula(&mut fr.rate, from, to));
            }
        }
        SetupKind::Fringe => n += usize::from(rekey_hit(&mut s.fringes, from, to)),
        SetupKind::Group => n += usize::from(rekey_hit(&mut s.groups, from, to)),
        SetupKind::Unit => {
            n += usize::from(rekey_hit(&mut s.units, from, to));
            for fr in s.fringes.values_mut() {
                if let FringeKind::Flat { unit, .. } = &mut fr.kind {
                    n += usize::from(swap_opt(unit, from, to));
                }
            }
        }
        SetupKind::Set => n += usize::from(rekey_hit(&mut s.sets, from, to)),
        SetupKind::Location => n += usize::from(rekey_hit(&mut s.locations, from, to)),
        SetupKind::Currency => n += usize::from(rekey_hit(&mut s.currencies, from, to)),
    }
    n
}

/// Renames in a tag preset; returns how many parts changed.
fn tag_rewrite(t: &mut TagPreset, kind: SetupKind, from: &str, to: &str) -> usize {
    usize::from(match kind {
        SetupKind::Fringe => swap_in(&mut t.fringes, from, to),
        SetupKind::Group => swap_in(&mut t.groups, from, to),
        SetupKind::Set => swap_opt(&mut t.set, from, to),
        SetupKind::Location => swap_opt(&mut t.location, from, to),
        SetupKind::Currency => swap_opt(&mut t.currency, from, to),
        SetupKind::Global | SetupKind::Unit => false,
    })
}

/// The rename itself, with the checks done.
fn rewrite(b: &mut Budget, kind: SetupKind, from: &str, to: &str) -> usize {
    let mut n = 0;
    // Every row: the budget's own, and the line blocks'.
    for d in b
        .topsheet
        .iter_mut()
        .filter_map(|r| match r {
            TopsheetRow::Category(c) => Some(c),
            TopsheetRow::ProductionTotal(_) => None,
        })
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
        .chain(b.line_presets.iter_mut().flat_map(|p| &mut p.rows))
    {
        n += row_rewrite(d, kind, from, to);
    }
    for p in &mut b.line_presets {
        n += setup_rewrite(&mut p.setup, kind, from, to);
    }
    for t in &mut b.tag_presets {
        n += tag_rewrite(t, kind, from, to);
    }
    let mut count = |hit: bool| n += usize::from(hit);
    match kind {
        SetupKind::Global => {
            rekey(&mut b.globals, from, to);
            rekey(&mut b.global_info, from, to);
            for f in b.globals.values_mut() {
                count(swap_formula(f, from, to));
            }
            for fr in b.fringes.values_mut() {
                count(swap_formula(&mut fr.rate, from, to));
            }
            for p in &mut b.report_presets {
                let s = &mut p.spec;
                for band in [&mut s.header, &mut s.footer].into_iter().flatten() {
                    for slot in [&mut band.left, &mut band.center, &mut band.right] {
                        if let Some(BandField::Global(g)) = slot {
                            if g == from {
                                *g = to.to_string();
                                count(true);
                            }
                        }
                    }
                }
            }
        }
        SetupKind::Fringe => {
            rekey(&mut b.fringes, from, to);
            for c in &mut b.credits {
                count(swap_in(&mut c.excluded_fringes, from, to));
            }
        }
        SetupKind::Group => {
            rekey(&mut b.groups, from, to);
            for c in &mut b.charges {
                count(swap_in(&mut c.excluded_groups, from, to));
            }
            for r in b.credits.iter_mut().flat_map(|c| &mut c.rows) {
                if let CreditSource::Group { group, .. } = &mut r.source {
                    if group == from {
                        *group = to.to_string();
                        count(true);
                    }
                }
            }
            for p in &mut b.report_presets {
                count(swap_in(&mut p.spec.only_groups, from, to));
            }
        }
        SetupKind::Unit => {
            rekey(&mut b.units, from, to);
            for fr in b.fringes.values_mut() {
                if let FringeKind::Flat { unit, .. } = &mut fr.kind {
                    count(swap_opt(unit, from, to));
                }
            }
            // A global's unit (display only) follows too.
            for g in b.global_info.values_mut() {
                if g.unit == from {
                    g.unit = to.to_string();
                    count(true);
                }
            }
        }
        SetupKind::Set => rekey(&mut b.sets, from, to),
        SetupKind::Location => rekey(&mut b.locations, from, to),
        SetupKind::Currency => {
            if b.base_currency == from {
                b.base_currency = to.to_string();
            }
            rekey(&mut b.currencies, from, to);
            for c in &mut b.charges {
                if let ChargeKind::Flat { currency, .. } = &mut c.kind {
                    count(swap_opt(currency, from, to));
                }
            }
            count(swap_opt(&mut b.totals_currency, from, to));
        }
    }
    // Its place in a hand-made order (not a reference: not counted).
    crate::order::rename_in(b, kind, from, to);
    // And its folder (the library's folders, not counted either).
    crate::folders::rename_item(b, crate::folders::kind_of(kind), from, to);
    // And where it came from (a setup pack's stamp, not counted).
    crate::pack::rename_item(b, crate::folders::kind_of(kind), from, to);
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::report_preset::{ReportPreset, ReportSpec};
    use crate::{recalc, Dec, RecalcOptions};

    fn budget() -> Budget {
        let mut b = Budget::new("t", "USD");
        b.globals.insert("DAYS".into(), "5".into());
        b.globals.insert("SHOOT_DAYS".into(), "DAYS*4".into());
        b.globals.insert("WEEKS".into(), "SHOOT_DAYS / 5".into());
        b.currencies.insert(
            "CAD".into(),
            Currency {
                description: String::new(),
                symbol: "$".into(),
                rate: "0.73".parse().unwrap(),
                format: Default::default(),
            },
        );
        b.fringes.insert(
            "PEN".into(),
            Fringe {
                short_code: None,
                description: String::new(),
                rate: "DAYS + 1".into(),
                kind: FringeKind::Percent { cutoff: None },
            },
        );
        b.fringes.insert(
            "MEAL".into(),
            Fringe {
                short_code: None,
                description: String::new(),
                rate: "12".into(),
                kind: FringeKind::flat(Some("Day".into())),
            },
        );
        b.groups.insert(
            "Crew".into(),
            Group {
                short_code: None,
                description: "shooting crew".into(),
                in_budget_total: true,
                color: Some("#2060a0".into()),
            },
        );
        b.sets.insert("Kitchen".into(), SetDef::default());
        b.locations.insert("Studio".into(), Location::default());
        let mut l1 = Detail::line(1, "Grip", "SHOOT_DAYS", "Day", "450");
        l1.fringes.insert("PEN".into());
        l1.fringes.insert("MEAL".into());
        l1.groups.insert("Crew".into());
        l1.set = Some("Kitchen".into());
        l1.location = Some("Studio".into());
        l1.x = "DAYS".into();
        l1.unit2 = "Day".into();
        let mut l2 = Detail::line(2, "Gaffer", "2", "Week", "WEEKS * 100");
        l2.currency = Some("CAD".into());
        // A unit typed on a line that isn't in the table.
        l2.unit3 = "Shift".into();
        l2.groups.insert("Crew".into());
        b.topsheet.push(TopsheetRow::Category(Category {
            notes: Vec::new(),
            code: "20".into(),
            name: "Crew".into(),
            accounts: vec![Account {
                notes: Vec::new(),
                code: "20-01".into(),
                name: "Grip".into(),
                details: vec![l1, Detail::subtotal(3, "sub"), l2],
            }],
        }));
        b.charges.push(Charge {
            name: "Fee".into(),
            account: None,
            applied: true,
            kind: ChargeKind::Flat {
                amount: 10_000,
                currency: Some("CAD".into()),
            },
            excluded_amounts: Vec::new(),
            excluded_groups: ["Crew".to_string()].into(),
            excluded_categories: BTreeSet::new(),
            excluded_accounts: BTreeSet::new(),
            label_percent: false,
            label_excluded: false,
        });
        b.credits.push(Credit {
            name: "Credit".into(),
            account: None,
            applied: true,
            cap: None,
            discount_percent: Dec::ZERO,
            excluded_fringes: ["PEN".to_string()].into(),
            rows: vec![CreditRow {
                description: String::new(),
                source: CreditSource::Group {
                    group: "Crew".into(),
                },
                percent: "25".parse().unwrap(),
                cap: CreditCap::None,
            }],
        });
        b.report_presets.push(ReportPreset {
            name: "Crew only".into(),
            default: false,
            spec: ReportSpec {
                only_groups: ["Crew".to_string()].into(),
                ..ReportSpec::default()
            },
        });
        b.totals_currency = Some("CAD".into());
        b
    }

    fn row(b: &Budget, id: RowId) -> &Detail {
        b.categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|d| d.id == id)
            .unwrap()
    }

    #[test]
    fn a_global_in_a_report_header_follows_its_rename() {
        use crate::report_preset::{Band, BandField};
        let mut b = budget();
        b.report_presets[0].spec.footer = Some(Band {
            left: Some(BandField::Global("DAYS".into())),
            right: Some(BandField::Global("SHOOT_DAYS".into())),
            ..Band::default()
        });
        let refs = references(&b, SetupKind::Global, "DAYS");
        let n = rename(&mut b, SetupKind::Global, "DAYS", "PREP_DAYS").unwrap();
        // The three formula uses and the footer slot; never SHOOT_DAYS's slot.
        assert_eq!(n, 4);
        assert_eq!(refs, n);
        let f = b.report_presets[0].spec.footer.as_ref().unwrap();
        assert_eq!(f.left, Some(BandField::Global("PREP_DAYS".into())));
        assert_eq!(f.right, Some(BandField::Global("SHOOT_DAYS".into())));
        let mut back = b.clone();
        rename(&mut back, SetupKind::Global, "PREP_DAYS", "DAYS").unwrap();
        assert_eq!(
            back.report_presets[0].spec.footer.as_ref().unwrap().left,
            Some(BandField::Global("DAYS".into()))
        );
    }

    #[test]
    fn a_global_is_renamed_token_by_token_everywhere() {
        let mut b = budget();
        let before = recalc(&b, &RecalcOptions::default()).unwrap();
        let n = rename(&mut b, SetupKind::Global, "DAYS", "PREP_DAYS").unwrap();
        // SHOOT_DAYS's formula, PEN's rate, row 1's X. Never inside SHOOT_DAYS.
        assert_eq!(n, 3);
        assert_eq!(b.globals["SHOOT_DAYS"].0, "PREP_DAYS*4");
        assert_eq!(b.globals["PREP_DAYS"].0, "5");
        assert!(!b.globals.contains_key("DAYS"));
        assert_eq!(b.fringes["PEN"].rate.0, "PREP_DAYS + 1");
        assert_eq!(row(&b, 1).amount.0, "SHOOT_DAYS");
        assert_eq!(row(&b, 1).x.0, "PREP_DAYS");
        let after = recalc(&b, &RecalcOptions::default()).unwrap();
        assert_eq!(after.net_total, before.net_total);
        assert_eq!(after.globals["PREP_DAYS"], before.globals["DAYS"]);
    }

    #[test]
    fn every_kind_reaches_every_reference() {
        let cases: &[(SetupKind, &str, &str, usize)] = &[
            // Row 1's fringes, the credit's excluded fringe.
            (SetupKind::Fringe, "PEN", "Pension", 2),
            // Rows 1 and 2, the charge, the credit, the report preset.
            (SetupKind::Group, "Crew", "Shoot crew", 5),
            // Row 1's unit and unit2, MEAL's flat unit.
            (SetupKind::Unit, "Day", "Shoot day", 3),
            (SetupKind::Set, "Kitchen", "Kitchen int", 1),
            (SetupKind::Location, "Studio", "Stage 4", 1),
            // Row 2, the flat charge, the totals currency.
            (SetupKind::Currency, "CAD", "CDN", 3),
        ];
        for &(kind, from, to, places) in cases {
            let mut b = budget();
            assert_eq!(references(&b, kind, from), places, "{kind:?}");
            assert_eq!(rename(&mut b, kind, from, to), Ok(places), "{kind:?}");
            assert_eq!(references(&b, kind, from), 0, "{kind:?}");
            assert_eq!(references(&b, kind, to), places, "{kind:?}");
            assert!(exists(&b, kind, to) && !exists(&b, kind, from), "{kind:?}");
        }
        let mut b = budget();
        rename(&mut b, SetupKind::Group, "Crew", "Shoot crew").unwrap();
        // The definition, color included, goes with the name.
        assert_eq!(b.groups["Shoot crew"].color.as_deref(), Some("#2060a0"));
        assert_eq!(b.groups["Shoot crew"].description, "shooting crew");
    }

    #[test]
    fn the_base_currency_can_be_renamed() {
        let mut b = budget();
        let mut l = row(&b, 1).clone();
        l.id = 9;
        l.currency = Some("USD".into());
        if let TopsheetRow::Category(c) = &mut b.topsheet[0] {
            c.accounts[0].details.push(l);
        }
        assert_eq!(rename(&mut b, SetupKind::Currency, "USD", "US$"), Ok(1));
        assert_eq!(b.base_currency, "US$");
        assert_eq!(row(&b, 9).currency.as_deref(), Some("US$"));
        // Not onto a currency in the list.
        assert_eq!(
            rename(&mut b, SetupKind::Currency, "US$", "CAD"),
            Err(RenameError::Taken {
                kind: SetupKind::Currency,
                name: "CAD".into()
            })
        );
        // Nor a foreign currency onto the base code.
        assert!(matches!(
            rename(&mut b, SetupKind::Currency, "CAD", "US$"),
            Err(RenameError::Taken { .. })
        ));
    }

    #[test]
    fn bad_renames_change_nothing() {
        let b0 = budget();
        let refuse = |kind, from: &str, to: &str| {
            let mut b = b0.clone();
            let e = rename(&mut b, kind, from, to).unwrap_err();
            assert_eq!(b, b0, "{e}");
            e
        };
        assert!(matches!(
            refuse(SetupKind::Global, "NOPE", "X"),
            RenameError::Missing { .. }
        ));
        assert_eq!(
            refuse(SetupKind::Global, "DAYS", " DAYS "),
            RenameError::Same
        );
        assert!(matches!(
            refuse(SetupKind::Global, "DAYS", "2.5DAYS"),
            RenameError::BadName { .. }
        ));
        assert!(matches!(
            refuse(SetupKind::Global, "DAYS", "12"),
            RenameError::BadName { .. }
        ));
        assert!(matches!(
            refuse(SetupKind::Global, "DAYS", "WEEKS"),
            RenameError::Taken { .. }
        ));
        assert!(matches!(
            refuse(SetupKind::Fringe, "PEN", "A, B"),
            RenameError::BadName { .. }
        ));
        assert!(matches!(
            refuse(SetupKind::Group, "Crew", "  "),
            RenameError::BadName { .. }
        ));
        // A unit typed on lines but not in the table is in use.
        assert_eq!(
            refuse(SetupKind::Unit, "Day", "Shift"),
            RenameError::InUse {
                kind: SetupKind::Unit,
                name: "Shift".into(),
                places: 1
            }
        );
        // A global name a formula already uses (unknown so far) is in use.
        let mut b = b0.clone();
        b.globals.insert("X".into(), "LATER + 1".into());
        assert!(matches!(
            rename(&mut b, SetupKind::Global, "DAYS", "LATER"),
            Err(RenameError::InUse { places: 1, .. })
        ));
        assert!(RenameError::InUse {
            kind: SetupKind::Unit,
            name: "Week".into(),
            places: 2
        }
        .to_string()
        .contains("2 places"));
    }

    #[test]
    fn renaming_back_gives_the_same_budget() {
        for kind in SetupKind::ALL.iter().copied() {
            let (from, to) = match kind {
                SetupKind::Global => ("DAYS", "D2"),
                SetupKind::Fringe => ("MEAL", "Meals"),
                SetupKind::Group => ("Crew", "Team"),
                SetupKind::Unit => ("Day", "Jour"),
                SetupKind::Set => ("Kitchen", "K"),
                SetupKind::Location => ("Studio", "S"),
                SetupKind::Currency => ("USD", "USN"),
            };
            let b0 = budget();
            let mut b = b0.clone();
            let n = rename(&mut b, kind, from, to).unwrap();
            assert_ne!(b, b0, "{kind:?}");
            assert_eq!(rename(&mut b, kind, to, from), Ok(n), "{kind:?}");
            assert_eq!(b, b0, "{kind:?}");
        }
    }

    /// The budget with a tag preset and a line block using everything.
    fn with_presets() -> Budget {
        use crate::presets::{LinePreset, SetupItems, TagPreset};
        let mut b = budget();
        b.tag_presets.push(TagPreset {
            name: "Crew day".into(),
            fringes: ["PEN".to_string()].into(),
            groups: ["Crew".to_string()].into(),
            set: Some("Kitchen".into()),
            location: Some("Studio".into()),
            currency: Some("CAD".into()),
        });
        let mut row = row(&b, 1).clone();
        row.id = 0;
        row.currency = Some("CAD".into());
        let mut setup = SetupItems::default();
        setup.globals.insert("DAYS".into(), "5".into());
        setup.globals.insert("SHOOT_DAYS".into(), "DAYS*4".into());
        setup.fringes.insert("PEN".into(), b.fringes["PEN"].clone());
        setup
            .fringes
            .insert("MEAL".into(), b.fringes["MEAL"].clone());
        setup.groups.insert("Crew".into(), b.groups["Crew"].clone());
        setup.units.insert("Day".into(), b.units["Day"].clone());
        setup.sets.insert("Kitchen".into(), SetDef::default());
        setup.locations.insert("Studio".into(), Location::default());
        setup
            .currencies
            .insert("CAD".into(), b.currencies["CAD"].clone());
        b.line_presets.push(LinePreset {
            name: "Grip day".into(),
            rows: vec![row],
            setup,
        });
        b
    }

    #[test]
    fn presets_and_blocks_follow_a_rename() {
        let b0 = with_presets();
        let cases: &[(SetupKind, &str, &str, usize)] = &[
            // Budget 3; block row X, the copy, SHOOT_DAYS's copy, PEN's copy rate.
            (SetupKind::Global, "DAYS", "D", 7),
            // Budget 2; tag preset, block row, the copy.
            (SetupKind::Fringe, "PEN", "Pension", 5),
            (SetupKind::Group, "Crew", "Team", 8),
            // Budget 3; block row unit and unit2, the copy, MEAL's copy unit.
            (SetupKind::Unit, "Day", "Jour", 7),
            (SetupKind::Set, "Kitchen", "K", 4),
            (SetupKind::Location, "Studio", "S", 4),
            (SetupKind::Currency, "CAD", "CDN", 6),
        ];
        for &(kind, from, to, places) in cases {
            let mut b = b0.clone();
            assert_eq!(references(&b, kind, from), places, "{kind:?}");
            assert_eq!(rename(&mut b, kind, from, to), Ok(places), "{kind:?}");
            assert_eq!(references(&b, kind, from), 0, "{kind:?}");
            assert_eq!(rename(&mut b, kind, to, from), Ok(places), "{kind:?}");
            assert_eq!(b, b0, "{kind:?}");
        }
        let mut b = b0.clone();
        rename(&mut b, SetupKind::Global, "DAYS", "D").unwrap();
        let block = &b.line_presets[0];
        assert_eq!(block.rows[0].x.0, "D");
        assert_eq!(block.setup.globals["SHOOT_DAYS"].0, "D*4");
        assert_eq!(block.setup.globals["D"].0, "5");
        assert_eq!(block.setup.fringes["PEN"].rate.0, "D + 1");
        // A name only a block still uses is taken.
        let mut b = b0.clone();
        b.line_presets[0]
            .setup
            .globals
            .insert("ONLY_HERE".into(), "1".into());
        assert!(matches!(
            rename(&mut b, SetupKind::Global, "WEEKS", "ONLY_HERE"),
            Err(RenameError::InUse { places: 1, .. })
        ));
    }

    #[test]
    fn kinds_have_names() {
        for &k in SetupKind::ALL {
            assert_eq!(SetupKind::from_name(k.name()), Some(k));
        }
        assert_eq!(SetupKind::from_name("nope"), None);
    }
}
