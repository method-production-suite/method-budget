//! Edits that touch many places in a budget at once, and what a formula
//! typed in may not be.

use std::collections::{BTreeMap, BTreeSet};

use crate::calc::arith::Num;
use crate::calc::{eval_globals, overflow, CalcError};
use crate::decimal::Dec;
use crate::expr::{EvalError, Expr};
use crate::model::{Budget, Formula};

/// Rewrites `f` with `name` replaced by `value`, if it refers to `name`.
/// Formulas that don't parse are left alone; they can't be referring to it.
/// A double is written with the shortest digits that give it back, so
/// Movie Magic arithmetic gets the same value from the text.
fn freeze(f: &mut Formula, name: &str, value: Num) {
    if f.is_blank() {
        return;
    }
    let Ok(e) = Expr::parse(&f.0) else { return };
    let mut vars = Default::default();
    e.vars(&mut vars);
    if vars.contains(name) {
        let e = match value {
            Num::Dec(d) => e.substitute(name, d),
            Num::F64(v) => e.substitute_f64(name, v),
        };
        *f = Formula(e.to_string());
    }
}

/// Deletes global `name`, first replacing every reference to it (in other
/// globals, fringe rates and detail fields) with its current value, so no total
/// changes. Returns that value (to 9 places), or `None` if there was no such
/// global.
pub fn delete_global(b: &mut Budget, name: &str) -> Result<Option<Dec>, CalcError> {
    let Some(&value) = eval_globals(b)?.get(name) else {
        return Ok(None);
    };
    let shown = value
        .shown()
        .ok_or_else(|| overflow(format!("global {name}")))?;
    b.globals.remove(name);
    b.global_info.remove(name);
    for f in b.globals.values_mut() {
        freeze(f, name, value);
    }
    for fringe in b.fringes.values_mut() {
        freeze(&mut fringe.rate, name, value);
    }
    for acct in b.categories_mut().flat_map(|c| &mut c.accounts) {
        for d in &mut acct.details {
            for f in [&mut d.amount, &mut d.x, &mut d.rate, &mut d.x4] {
                freeze(f, name, value);
            }
        }
    }
    Ok(Some(shown))
}

// ---------------------------------------------------------------------------
// What a formula typed in may not be (decisions.md, entry 59)

/// Globals with no value because they refer back to themselves: those on a
/// cycle and those using one. Unknown names don't count as references.
pub fn circular_globals(b: &Budget) -> BTreeSet<String> {
    let deps: BTreeMap<&str, BTreeSet<String>> = b
        .globals
        .iter()
        .map(|(name, f)| {
            let mut vars = BTreeSet::new();
            if let Ok(e) = Expr::parse(&f.0) {
                e.vars(&mut vars);
            }
            vars.retain(|v| b.globals.contains_key(v));
            (name.as_str(), vars)
        })
        .collect();
    // A global has a value once every global it uses has one.
    let mut valued: BTreeSet<&str> = BTreeSet::new();
    loop {
        let before = valued.len();
        for (name, uses) in &deps {
            if !valued.contains(name) && uses.iter().all(|u| valued.contains(u.as_str())) {
                valued.insert(name);
            }
        }
        if valued.len() == before {
            break;
        }
    }
    deps.keys()
        .filter(|n| !valued.contains(*n))
        .map(|n| n.to_string())
        .collect()
}

/// Every global's value as recalc works it out, with the globals that
/// can't be worked out (circular ones, or ones with an error) counted as 0,
/// as the reference product shows a circular global.
fn settled_globals(b: &Budget) -> BTreeMap<String, Num> {
    let mut only = Budget::new("", b.base_currency.clone());
    only.arithmetic = b.arithmetic;
    only.globals = b.globals.clone();
    for name in circular_globals(b) {
        only.globals.insert(name, Formula::from("0"));
    }
    // Each round sets at least one broken global to 0, so this ends.
    for _ in 0..=only.globals.len() {
        match eval_globals(&only) {
            Ok(v) => return v,
            Err(e) => match e.at.strip_prefix("global ") {
                Some(n) if only.globals.contains_key(n) => {
                    only.globals.insert(n.to_string(), Formula::from("0"));
                }
                _ => break,
            },
        }
    }
    BTreeMap::new()
}

/// Whether `text` divides by zero with the budget's globals, in its
/// arithmetic. Formulas that don't parse or use an unknown name aren't
/// (they are kept and flagged).
fn divides_by_zero(b: &Budget, text: &str, globals: &BTreeMap<String, Num>) -> bool {
    matches!(
        crate::calc::eval_in(b.arithmetic, text, &|n| globals.get(n).copied()),
        Err(crate::calc::ErrorKind::Eval(EvalError::DivideByZero))
    )
}

/// Why `text`, typed into a line's Amt, X, Rate or 4X, is refused at entry
/// (decisions.md, entry 59), as the reference product refuses it: it
/// divides by zero, or it uses a global that refers back to itself, which
/// has no value. `None` when it may be entered; any other problem (a typo,
/// an unknown global) is kept as typed and flagged.
pub fn refused_formula(b: &Budget, text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let e = Expr::parse(text).ok()?;
    let mut vars = BTreeSet::new();
    e.vars(&mut vars);
    let circular = circular_globals(b);
    if let Some(g) = vars.iter().find(|v| circular.contains(*v)) {
        return Some(format!(
            "`{g}` refers back to itself through other globals, so it has no value to use"
        ));
    }
    divides_by_zero(b, text, &settled_globals(b)).then(|| "it divides by zero".to_string())
}

/// Why setting global `name` to `text` is refused at entry (decisions.md,
/// entry 59): it would make globals refer to each other (or to itself), it
/// uses a global that already does, or it divides by zero. `None` when it
/// may be set; a formula that doesn't parse or uses an unknown name is kept
/// and flagged.
pub fn refused_global(b: &Budget, name: &str, text: &str) -> Option<String> {
    let text = text.trim();
    let e = Expr::parse(text).ok()?;
    let mut after = b.clone();
    after
        .globals
        .insert(name.to_string(), Formula(text.to_string()));
    let was = circular_globals(b);
    let now = circular_globals(&after);
    if now.contains(name) {
        let mut vars = BTreeSet::new();
        e.vars(&mut vars);
        return Some(match vars.iter().find(|v| was.contains(*v)) {
            Some(g) => format!(
                "`{g}` refers back to itself through other globals, so it has no value to use"
            ),
            None => format!("{name} would refer back to itself through the globals it uses"),
        });
    }
    let mut others = settled_globals(b);
    others.remove(name);
    divides_by_zero(b, text, &others).then(|| "it divides by zero".to_string())
}

/// The unit a line's Unit takes from a global (decisions.md, entry 59):
/// when the line's Amt is exactly a global's name and that global has a
/// unit, the reference product fills the empty Unit with it. `None`
/// otherwise.
pub fn unit_from_global<'a>(b: &'a Budget, amount: &str) -> Option<&'a str> {
    let name = amount.trim();
    b.globals.get(name)?;
    let unit = b.global_info.get(name)?.unit.trim();
    (!unit.is_empty() && b.units.contains_key(unit)).then_some(unit)
}
