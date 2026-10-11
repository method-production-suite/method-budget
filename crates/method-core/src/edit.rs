//! Edits that touch many places in a budget at once.

use crate::calc::arith::Num;
use crate::calc::{eval_globals, overflow, CalcError};
use crate::decimal::Dec;
use crate::expr::Expr;
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
