//! Lenient recalculation for the editor.
//!
//! The engine stops at the first problem, which is right for a file check but
//! wrong for an editor: one bad formula must not blank the whole budget. When
//! recalc fails we find every cell the engine would reject (using the same
//! rules), flag each one, and recalculate a copy with those cells treated as
//! blank. The budget itself is never changed: the user's text stays as typed.

use std::collections::{BTreeMap, BTreeSet};

use fwb_core::expr::Expr;
use fwb_core::{
    recalc, Budget, CalcError, ChargeKind, CreditSource, Dec, Formula, RecalcOptions, Report,
    RowId, RowKind,
};
use serde::Serialize;

/// What a problem is attached to, so the UI can flag the right place.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    /// A detail-row cell; `field` is the file-format key (`amount`, `rate`...).
    Cell {
        row_id: RowId,
        field: &'static str,
    },
    Global {
        name: String,
    },
    Fringe {
        name: String,
    },
    TotalsCurrency,
    Charge {
        index: usize,
    },
    Credit {
        index: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub target: Target,
    pub message: String,
}

/// Result of a lenient recalc.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// Totals, when they could be computed (possibly from the sanitized copy).
    pub report: Option<Report>,
    /// The budget `report` was computed from, when that is not the real one.
    pub sanitized: Option<Budget>,
    /// The engine's first error on the real budget.
    pub error: Option<CalcError>,
    /// Every problem found, in budget order.
    pub issues: Vec<Issue>,
}

impl Outcome {
    /// True when totals exist but treat flagged cells as blank.
    pub fn partial(&self) -> bool {
        self.report.is_some() && self.error.is_some()
    }
}

/// The message part of a [`CalcError`], without its location prefix.
pub fn error_message(e: &CalcError) -> String {
    let full = e.to_string();
    full.strip_prefix(&format!("{}: ", e.at))
        .unwrap_or(&full)
        .to_string()
}

pub fn lenient_recalc(b: &Budget) -> Outcome {
    let opts = RecalcOptions::default();
    let error = match recalc(b, &opts) {
        Ok(report) => {
            return Outcome {
                report: Some(report),
                sanitized: None,
                error: None,
                issues: Vec::new(),
            }
        }
        Err(e) => e,
    };
    let (clean, issues) = sanitize(b);
    match recalc(&clean, &opts) {
        Ok(report) => Outcome {
            report: Some(report),
            sanitized: Some(clean),
            error: Some(error),
            issues,
        },
        Err(_) => Outcome {
            report: None,
            sanitized: None,
            error: Some(error),
            issues,
        },
    }
}

/// Evaluates globals alone, replacing broken ones with `0` until the rest work.
/// Returns their values and the issues found.
fn settle_globals(clean: &mut Budget) -> (BTreeMap<String, Dec>, Vec<Issue>) {
    let mut issues = Vec::new();
    let mut flagged = BTreeSet::new();
    // Each round fixes at least one global, so this terminates.
    for _ in 0..=clean.globals.len() {
        let mut only = Budget::new("", clean.base_currency.clone());
        only.globals = clean.globals.clone();
        let e = match recalc(&only, &RecalcOptions::default()) {
            Ok(r) => return (r.globals, issues),
            Err(e) => e,
        };
        let message = error_message(&e);
        let names: Vec<String> = match &e.kind {
            fwb_core::ErrorKind::GlobalCycle(cycle) => cycle
                .iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .cloned()
                .collect(),
            _ => {
                e.at.strip_prefix("global ")
                    .map(|n| vec![n.to_string()])
                    .unwrap_or_default()
            }
        };
        let names: Vec<String> = names
            .into_iter()
            .filter(|n| clean.globals.contains_key(n))
            .collect();
        if names.is_empty() {
            break;
        }
        for name in names {
            if flagged.insert(name.clone()) {
                issues.push(Issue {
                    target: Target::Global { name: name.clone() },
                    message: message.clone(),
                });
            }
            if !fwb_core::expr::is_ident(&name) {
                clean.globals.remove(&name);
            } else {
                clean.globals.insert(name, Formula::from("0"));
            }
        }
    }
    (BTreeMap::new(), issues)
}

/// Why `f` can't be evaluated, if it can't. Blank is fine.
fn check(f: &Formula, globals: &BTreeMap<String, Dec>) -> Option<String> {
    if f.is_blank() {
        return None;
    }
    match Expr::parse(&f.0) {
        Err(e) => Some(e.to_string()),
        Ok(e) => e
            .eval(&|n| globals.get(n).copied())
            .err()
            .map(|e| e.to_string()),
    }
}

/// A copy of `b` with every problem the engine would stop on neutralized, and
/// the list of those problems.
pub fn sanitize(b: &Budget) -> (Budget, Vec<Issue>) {
    let mut clean = b.clone();
    let (globals, mut issues) = settle_globals(&mut clean);

    if let Some(c) = &clean.totals_currency {
        if *c != clean.base_currency && !clean.currencies.contains_key(c) {
            issues.push(Issue {
                target: Target::TotalsCurrency,
                message: format!("no exchange rate for `{c}`"),
            });
            clean.totals_currency = None;
        }
    }

    // Fringe rates that don't evaluate count as 0.
    let mut bad_fringes = Vec::new();
    for (name, f) in &clean.fringes {
        if let Some(message) = check(&f.rate, &globals) {
            issues.push(Issue {
                target: Target::Fringe { name: name.clone() },
                message,
            });
            bad_fringes.push(name.clone());
        }
    }
    for name in bad_fringes {
        if let Some(f) = clean.fringes.get_mut(&name) {
            f.rate = Formula::from("0");
        }
    }

    let fringes: BTreeSet<String> = clean.fringes.keys().cloned().collect();
    let groups: BTreeSet<String> = clean.groups.keys().cloned().collect();
    let currencies: BTreeSet<String> = clean.currencies.keys().cloned().collect();
    let base = clean.base_currency.clone();
    for acct in clean.categories_mut().flat_map(|c| &mut c.accounts) {
        for d in &mut acct.details {
            if d.kind == RowKind::Subtotal {
                continue;
            }
            let id = d.id;
            let mut flag = |field: &'static str, message: String| {
                issues.push(Issue {
                    target: Target::Cell { row_id: id, field },
                    message,
                });
            };
            let line = d.kind == RowKind::Line;
            let mut formulas: Vec<(&'static str, &mut Formula)> = vec![
                ("amount", &mut d.amount),
                ("x", &mut d.x),
                ("x4", &mut d.x4),
            ];
            if line {
                formulas.insert(2, ("rate", &mut d.rate));
            }
            for (field, f) in formulas {
                if let Some(message) = check(f, &globals) {
                    flag(field, message);
                    *f = Formula::default();
                }
            }
            if line {
                if let Some(c) = &d.currency {
                    if *c != base && !currencies.contains(c) {
                        flag("currency", format!("no exchange rate for `{c}`"));
                        d.currency = None;
                    }
                }
                let unknown: Vec<String> = d.fringes.difference(&fringes).cloned().collect();
                if !unknown.is_empty() {
                    let list = unknown.join("`, `");
                    flag("fringes", format!("unknown fringe `{list}`"));
                    d.fringes.retain(|n| fringes.contains(n));
                }
            }
            let unknown: Vec<String> = d.groups.difference(&groups).cloned().collect();
            if !unknown.is_empty() {
                let list = unknown.join("`, `");
                flag("groups", format!("unknown group `{list}`"));
                d.groups.retain(|n| groups.contains(n));
            }
        }
    }

    for (index, c) in clean.charges.iter_mut().enumerate() {
        if let ChargeKind::Flat {
            currency: currency @ Some(_),
            ..
        } = &mut c.kind
        {
            let code = currency.clone().unwrap_or_default();
            if code != base && !currencies.contains(&code) {
                issues.push(Issue {
                    target: Target::Charge { index },
                    message: format!("no exchange rate for `{code}`"),
                });
                *currency = None;
            }
        }
    }
    for (index, c) in clean.credits.iter_mut().enumerate() {
        for row in &mut c.rows {
            if let CreditSource::Group { group, .. } = &row.source {
                if !groups.contains(group) {
                    issues.push(Issue {
                        target: Target::Credit { index },
                        message: format!("unknown group `{group}`"),
                    });
                    row.source = CreditSource::Flat(0);
                }
            }
        }
    }
    (clean, issues)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fwb_core::starter;

    fn first_row_id(b: &Budget, account: &str) -> RowId {
        b.categories()
            .flat_map(|c| &c.accounts)
            .find(|a| a.code == account)
            .unwrap()
            .details[0]
            .id
    }

    fn detail_mut(b: &mut Budget, id: RowId) -> &mut fwb_core::Detail {
        b.categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
            .find(|d| d.id == id)
            .unwrap()
    }

    #[test]
    fn clean_budget_has_no_issues() {
        let o = lenient_recalc(&starter::budget());
        assert!(o.report.is_some() && o.error.is_none() && o.issues.is_empty());
        assert!(!o.partial());
    }

    #[test]
    fn bad_cells_are_flagged_and_the_rest_still_totals() {
        let mut b = starter::budget();
        let good = recalc(&b, &RecalcOptions::default()).unwrap();
        let id = first_row_id(&b, "50-01");
        let other = first_row_id(&b, "10-01");
        detail_mut(&mut b, id).rate = "15000 +".into();
        detail_mut(&mut b, other).amount = "NOPE * 2".into();
        detail_mut(&mut b, other).fringes.insert("GHOST".into());

        let o = lenient_recalc(&b);
        assert!(o.partial());
        assert_eq!(
            o.error.as_ref().unwrap().at,
            "account 10-01 line 1 (Screenplay): amount"
        );
        let targets: Vec<_> = o.issues.iter().map(|i| i.target.clone()).collect();
        assert_eq!(
            targets,
            [
                Target::Cell {
                    row_id: other,
                    field: "amount"
                },
                Target::Cell {
                    row_id: other,
                    field: "fringes"
                },
                Target::Cell {
                    row_id: id,
                    field: "rate"
                },
            ]
        );
        assert_eq!(o.issues[0].message, "unknown global `NOPE`");
        // The broken lines count as blank; everything else is unchanged.
        let r = o.report.unwrap();
        assert_eq!(r.categories[0].total, 0);
        assert_eq!(r.categories[1].total, good.categories[1].total);
        // The real budget keeps what was typed.
        assert_eq!(detail_mut(&mut b, id).rate.0, "15000 +");
    }

    #[test]
    fn broken_globals_are_flagged_and_count_as_zero() {
        let mut b = starter::budget();
        b.globals.insert("A".into(), "B + 1".into());
        b.globals.insert("B".into(), "A".into());
        b.globals.insert("C".into(), "1 / 0".into());
        let o = lenient_recalc(&b);
        assert!(o.partial());
        let names: BTreeSet<String> = o
            .issues
            .iter()
            .filter_map(|i| match &i.target {
                Target::Global { name } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(names, BTreeSet::from(["A".into(), "B".into(), "C".into()]));
        assert_eq!(o.report.unwrap().globals["SHOOT_WEEKS"], Dec::from_int(5));
    }

    #[test]
    fn bad_fringe_rate_and_currency_are_flagged() {
        let mut b = starter::budget();
        b.fringes.get_mut("PAYROLL").unwrap().rate = "TEN".into();
        let id = first_row_id(&b, "41-01");
        detail_mut(&mut b, id).currency = Some("XYZ".into());
        let o = lenient_recalc(&b);
        assert!(o.partial());
        assert_eq!(
            o.issues[0].target,
            Target::Fringe {
                name: "PAYROLL".into()
            }
        );
        assert!(o.issues.iter().any(|i| i.target
            == Target::Cell {
                row_id: id,
                field: "currency"
            }));
        assert_eq!(o.report.unwrap().categories[0].fringe_total, 0);
    }

    #[test]
    fn unfixable_problems_give_no_report_but_keep_the_error() {
        let mut b = starter::budget();
        // Duplicate row ids can't be neutralized.
        let id = first_row_id(&b, "10-01");
        let other = first_row_id(&b, "11-01");
        detail_mut(&mut b, other).id = id;
        let o = lenient_recalc(&b);
        assert!(o.report.is_none() && !o.partial());
        assert!(error_message(o.error.as_ref().unwrap()).contains("used twice"));
    }
}
