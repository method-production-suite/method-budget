//! Edits that touch many places in a budget at once.

use crate::calc::{eval_globals, CalcError};
use crate::decimal::Dec;
use crate::expr::Expr;
use crate::model::{Budget, Formula};

/// Rewrites `f` with `name` replaced by `value`, if it refers to `name`.
/// Formulas that don't parse are left alone; they can't be referring to it.
fn freeze(f: &mut Formula, name: &str, value: Dec) {
    if f.is_blank() {
        return;
    }
    let Ok(e) = Expr::parse(&f.0) else { return };
    let mut vars = Default::default();
    e.vars(&mut vars);
    if vars.contains(name) {
        *f = Formula(e.substitute(name, value).to_string());
    }
}

/// Deletes global `name`, first replacing every reference to it (in other
/// globals, fringe rates and detail fields) with its current value, so no total
/// changes. Returns that value, or `None` if there was no such global.
pub fn delete_global(b: &mut Budget, name: &str) -> Result<Option<Dec>, CalcError> {
    let Some(&value) = eval_globals(b)?.get(name) else {
        return Ok(None);
    };
    b.globals.remove(name);
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
    Ok(Some(value))
}
