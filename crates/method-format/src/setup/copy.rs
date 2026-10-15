//! Setup items between budgets: what rows and items refer to by name, the
//! setup they use, how an item stands in another budget, renaming what an
//! item names, and definitions in words. Shared by pasting rows, Import
//! Setup, the setup library and setup packs (the application's
//! src/clipboard.rs and src/import_setup.rs; decisions.md, entries 23, 29,
//! 37 and 50), and by the command line's `method pack`.

use std::collections::{BTreeMap, BTreeSet};

use method_core::{
    Budget, Cents, Currency, Detail, FlatBasis, Formula, Fringe, FringeKind, Group, Location,
    SetDef, UnitDef,
};

/// A result whose error is a sentence for the user.
pub type EditResult<T = ()> = Result<T, String>;

/// The kinds of setup item rows refer to by name, in the order the review
/// lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Fringe,
    Group,
    Global,
    Unit,
    Set,
    Location,
    Currency,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Fringe,
        Kind::Group,
        Kind::Global,
        Kind::Unit,
        Kind::Set,
        Kind::Location,
        Kind::Currency,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Kind::Fringe => "fringe",
            Kind::Group => "group",
            Kind::Global => "global",
            Kind::Unit => "unit",
            Kind::Set => "set",
            Kind::Location => "location",
            Kind::Currency => "currency",
        }
    }

    pub fn parse(s: &str) -> EditResult<Kind> {
        Kind::ALL
            .into_iter()
            .find(|k| k.key() == s)
            .ok_or_else(|| format!("unknown kind of setup item `{s}`"))
    }

    /// Whether "add it under a new name" makes sense. A currency code names
    /// a real currency, so a second one under another code would not.
    pub fn renamable(self) -> bool {
        self != Kind::Currency
    }
}

/// Setup items by kind and name: the same tables a [`Budget`] has, holding
/// only what a clip's rows use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Setup {
    pub fringes: BTreeMap<String, Fringe>,
    pub groups: BTreeMap<String, Group>,
    pub globals: BTreeMap<String, Formula>,
    pub units: BTreeMap<String, UnitDef>,
    pub sets: BTreeMap<String, SetDef>,
    pub locations: BTreeMap<String, Location>,
    pub currencies: BTreeMap<String, Currency>,
}

impl Setup {
    /// What `rows` use in `b` (fringes, groups, units, globals named in
    /// formulas, sets, locations, currencies), with what those use in turn: a
    /// fringe's flat unit and the globals in its rate, the globals in a
    /// global's formula, however deep. Names `b` doesn't define are left out
    /// (the rows keep them as they are).
    pub fn used_by<'a>(b: &Budget, rows: impl IntoIterator<Item = &'a Detail>) -> Setup {
        fn take<T: Clone>(from: &BTreeMap<String, T>, name: &str, to: &mut BTreeMap<String, T>) {
            if let Some(def) = from.get(name) {
                to.insert(name.to_string(), def.clone());
            }
        }
        let mut s = Setup::default();
        let mut globals: Vec<String> = Vec::new();
        let mut units: BTreeSet<String> = BTreeSet::new();
        for d in rows {
            for f in &d.fringes {
                take(&b.fringes, f, &mut s.fringes);
            }
            for g in &d.groups {
                take(&b.groups, g, &mut s.groups);
            }
            if let Some(n) = &d.set {
                take(&b.sets, n, &mut s.sets);
            }
            if let Some(n) = &d.location {
                take(&b.locations, n, &mut s.locations);
            }
            if let Some(c) = &d.currency {
                take(&b.currencies, c, &mut s.currencies);
            }
            for u in [&d.unit, &d.unit2, &d.unit3, &d.unit4] {
                if !u.is_empty() {
                    units.insert(u.clone());
                }
            }
            for f in [&d.amount, &d.x, &d.rate, &d.x4] {
                globals.extend(global_names(&f.0));
            }
        }
        for f in s.fringes.values() {
            globals.extend(global_names(&f.rate.0));
            if let FringeKind::Flat {
                unit,
                max_hours_per_day,
                ..
            } = &f.kind
            {
                units.extend(unit.iter().cloned());
                // Most hours a day count days by the Day unit.
                if max_hours_per_day.is_some() {
                    if let Some((day, _)) = method_core::calc::day_unit(&b.units) {
                        units.insert(day.to_string());
                    }
                }
            }
        }
        for u in units {
            take(&b.units, &u, &mut s.units);
        }
        while let Some(g) = globals.pop() {
            if s.globals.contains_key(&g) {
                continue;
            }
            if let Some(f) = b.globals.get(&g) {
                globals.extend(global_names(&f.0));
                s.globals.insert(g, f.clone());
            }
        }
        s
    }
}

/// Global names a formula mentions, read the way the formula lexer reads
/// them (letters, digits and `_`, not starting with a digit). Works on
/// formulas that don't parse too.
pub fn global_names(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    scan_names(src, |name| {
        out.insert(name.to_string());
        name.to_string()
    });
    out
}

/// `src` with every global named in `renames` renamed, name by name (never
/// inside a longer name); everything else stays as typed.
pub fn rename_globals(src: &str, renames: &BTreeMap<String, String>) -> String {
    if renames.is_empty() {
        return src.to_string();
    }
    scan_names(src, |name| {
        renames
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    })
}

/// Walks `src` like the formula lexer: numbers (digits and points) are kept,
/// each name is replaced by what `f` returns, other characters are kept.
fn scan_names(src: &str, mut f: impl FnMut(&str) -> String) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        if c.is_ascii_digit() || c == b'.' {
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            out.push_str(&src[start..i]);
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push_str(&f(&src[start..i]));
        } else {
            let ch = src[i..].chars().next().unwrap_or('?');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

// ---------------------------------------------------------------------------
// How an item stands in another budget

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Not in the target: it will be added.
    Add,
    /// The same definition in the target: nothing to do.
    Same,
    /// A different definition in the target under the same name.
    Differs,
}

pub fn status<T: PartialEq>(copied: Option<&T>, here: Option<&T>) -> Status {
    match here {
        None => Status::Add,
        Some(h) if Some(h) == copied => Status::Same,
        Some(_) => Status::Differs,
    }
}

// ---------------------------------------------------------------------------
// Definitions in words

/// `123456` cents as `1,234.56`, negatives with a minus sign.
pub fn money(c: Cents) -> String {
    method_core::CurrencyFormat::default().number(c)
}

/// Parts in words, joined; `fallback` when every part is empty.
fn words(parts: &[&str], fallback: &str) -> String {
    let kept: Vec<&str> = parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if kept.is_empty() {
        fallback.to_string()
    } else {
        kept.join("; ")
    }
}

pub fn describe_fringe(f: &Fringe) -> String {
    let rate = f.rate.0.trim();
    let how = match &f.kind {
        FringeKind::Percent { cutoff: None } => format!("{rate}% with no cutoff"),
        FringeKind::Percent { cutoff: Some(c) } => format!("{rate}% up to {}", money(*c)),
        FringeKind::Flat {
            unit,
            basis,
            max_hours_per_day,
            cutoff,
        } => {
            let mut how = format!("{rate} flat per {}", unit.as_deref().unwrap_or("line"));
            match basis {
                FlatBasis::Amount => {}
                FlatBasis::AmountCounts => how.push_str(", counting counts in X and 4X too"),
                FlatBasis::Quantity => how.push_str(", counting the full quantity"),
            }
            if let Some(h) = max_hours_per_day {
                how.push_str(&format!(", up to {h} hours a day"));
            }
            if let Some(c) = cutoff {
                how.push_str(&format!(", cutoff {} kept but not used", money(*c)));
            }
            how
        }
    };
    words(&[how.as_str(), f.description.as_str()], "")
}

pub fn describe_group(g: &Group) -> String {
    let counted = if g.in_budget_total {
        "counted in the total"
    } else {
        "left out of the total"
    };
    let color = g
        .color
        .as_deref()
        .map(|c| format!("color {c}"))
        .unwrap_or_default();
    words(&[counted, color.as_str(), g.description.as_str()], "")
}

pub fn describe_global(f: &Formula) -> String {
    format!("= {}", f.0.trim())
}

pub fn describe_unit(u: &UnitDef) -> String {
    let hours = match u.hours {
        Some(h) => format!("{h} hours"),
        None => "not a time unit".to_string(),
    };
    let plural = if u.plural.trim().is_empty() {
        String::new()
    } else {
        format!("plural {}", u.plural.trim())
    };
    words(
        &[hours.as_str(), plural.as_str(), u.description.as_str()],
        "",
    )
}

pub fn describe_set(s: &SetDef) -> String {
    words(&[s.description.as_str()], "no description")
}

pub fn describe_location(l: &Location) -> String {
    words(
        &[
            l.description.as_str(),
            l.city.as_str(),
            l.region.as_str(),
            l.country.as_str(),
        ],
        "no description",
    )
}

pub fn describe_currency(c: &Currency) -> String {
    let rate = format!("rate {}", c.rate);
    let symbol = if c.symbol.trim().is_empty() {
        String::new()
    } else {
        format!("symbol {}", c.symbol.trim())
    };
    // Its format, when not the usual `$1,234.56` (decisions.md, entry 61).
    let format = if c.format.is_default() {
        String::new()
    } else {
        format!("written {}", c.amount_text(123_456))
    };
    words(
        &[
            rate.as_str(),
            symbol.as_str(),
            format.as_str(),
            c.description.as_str(),
        ],
        "",
    )
}

// ---------------------------------------------------------------------------
// Renaming what copied items and rows name

/// Names the pasted content and setup use instead of the copied ones (the
/// "add it under a new name" choices).
#[derive(Debug, Default)]
pub struct Renames {
    pub fringes: BTreeMap<String, String>,
    pub groups: BTreeMap<String, String>,
    pub globals: BTreeMap<String, String>,
    pub units: BTreeMap<String, String>,
    pub sets: BTreeMap<String, String>,
    pub locations: BTreeMap<String, String>,
}

impl Renames {
    pub fn of(&mut self, kind: Kind) -> Option<&mut BTreeMap<String, String>> {
        match kind {
            Kind::Fringe => Some(&mut self.fringes),
            Kind::Group => Some(&mut self.groups),
            Kind::Global => Some(&mut self.globals),
            Kind::Unit => Some(&mut self.units),
            Kind::Set => Some(&mut self.sets),
            Kind::Location => Some(&mut self.locations),
            Kind::Currency => None,
        }
    }
}

pub fn renamed(map: &BTreeMap<String, String>, name: &str) -> String {
    map.get(name).cloned().unwrap_or_else(|| name.to_string())
}

/// A copied row as it lands: references renamed, the base currency as none.
pub fn rewrite(d: &mut Detail, r: &Renames, base: &str) {
    if !r.fringes.is_empty() {
        d.fringes = d.fringes.iter().map(|n| renamed(&r.fringes, n)).collect();
    }
    if !r.groups.is_empty() {
        d.groups = d.groups.iter().map(|n| renamed(&r.groups, n)).collect();
    }
    for u in [&mut d.unit, &mut d.unit2, &mut d.unit3, &mut d.unit4] {
        if let Some(to) = r.units.get(u.as_str()) {
            *u = to.clone();
        }
    }
    if let Some(s) = &mut d.set {
        *s = renamed(&r.sets, s);
    }
    if let Some(l) = &mut d.location {
        *l = renamed(&r.locations, l);
    }
    for f in [&mut d.amount, &mut d.x, &mut d.rate, &mut d.x4] {
        f.0 = rename_globals(&f.0, &r.globals);
    }
    if d.currency.as_deref() == Some(base) {
        d.currency = None;
    }
}

/// Puts the copied item `from` into `b` as `to`, with the names it uses renamed.
pub fn write(b: &mut Budget, s: &Setup, kind: Kind, from: &str, to: &str, r: &Renames) {
    let to = to.to_string();
    match kind {
        Kind::Fringe => {
            if let Some(f) = s.fringes.get(from) {
                let mut f = f.clone();
                f.rate.0 = rename_globals(&f.rate.0, &r.globals);
                if let FringeKind::Flat { unit: Some(u), .. } = &mut f.kind {
                    *u = renamed(&r.units, u);
                }
                b.fringes.insert(to, f);
            }
        }
        Kind::Global => {
            if let Some(f) = s.globals.get(from) {
                b.globals
                    .insert(to, Formula(rename_globals(&f.0, &r.globals)));
            }
        }
        Kind::Group => {
            if let Some(g) = s.groups.get(from) {
                b.groups.insert(to, g.clone());
            }
        }
        Kind::Unit => {
            if let Some(u) = s.units.get(from) {
                b.units.insert(to, u.clone());
            }
        }
        Kind::Set => {
            if let Some(x) = s.sets.get(from) {
                b.sets.insert(to, x.clone());
            }
        }
        Kind::Location => {
            if let Some(x) = s.locations.get(from) {
                b.locations.insert(to, x.clone());
            }
        }
        Kind::Currency => {
            if let Some(x) = s.currencies.get(from) {
                b.currencies.insert(to, x.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_in_words() {
        assert_eq!(money(0), "0.00");
        assert_eq!(money(5), "0.05");
        assert_eq!(money(16_860_000), "168,600.00");
        assert_eq!(money(-100_000), "-1,000.00");
        assert_eq!(money(i64::MIN), "-92,233,720,368,547,758.08");
    }
}
