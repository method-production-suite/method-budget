//! Fringes for layouts that leave part of them out, worked out from the
//! fringe percentages each line shows (`Agg Sum`: its fringes' nominal
//! percentages added up, a flat fringe as the percent its dollars make of
//! the line).
//!
//! - **No fringe figures** (the Excel workbook): each fringe's kind and rate
//!   from the lines. Where a line has one fringe not yet known, what is left
//!   of its percentage is that fringe's: the same on every line, a percent
//!   fringe; else a flat amount per the lines' unit, or per line.
//! - **No fringe names on the lines** (basic XML): each line's fringes are
//!   the one set of known fringes whose percentages add up to the line's.
//!
//! Both are guesses the figures support, and the notes say so.

use std::collections::{BTreeMap, BTreeSet};

use method_core::{default_units, Dec, UnitDef};

use super::build::{resolve_units, simplest, STARTER_HOURS};
use super::model::{Export, Num, TopRow, XDetail, XFringe};

/// A fringe as worked out.
#[derive(Clone, Debug, PartialEq)]
struct Def {
    percent: bool,
    rate: f64,
    /// A flat fringe's unit (the budget's unit name); `None` per line.
    unit: Option<String>,
}

/// Unit hours by every name the export writes for a unit.
struct Hours {
    map: BTreeMap<String, String>,
    units: BTreeMap<String, UnitDef>,
}

impl Hours {
    fn new(x: &Export) -> Hours {
        let mut units = default_units();
        for (k, plural, h) in STARTER_HOURS {
            units.insert(
                k.to_string(),
                UnitDef {
                    plural: plural.to_string(),
                    description: String::new(),
                    hours: Some(Dec::from_int(h)),
                },
            );
        }
        let mut seen: BTreeMap<String, bool> = BTreeMap::new();
        for d in details(x) {
            let n = d.unit.trim();
            if !n.is_empty() {
                *seen.entry(n.to_string()).or_default() |=
                    d.amount.as_ref().is_some_and(|a| a.value != 1.0);
            }
        }
        let (map, _) = resolve_units(&seen, &mut units);
        Hours { map, units }
    }

    fn key(&self, name: &str) -> String {
        let t = name.trim();
        self.map.get(t).cloned().unwrap_or_else(|| t.to_string())
    }

    fn of(&self, unit: &str) -> Option<f64> {
        self.units
            .get(&self.key(unit))
            .and_then(|u| u.hours)
            .map(|h| h.to_f64())
            .filter(|h| *h > 0.0)
    }
}

fn details(x: &Export) -> impl Iterator<Item = &XDetail> {
    x.rows
        .iter()
        .filter_map(|r| match r {
            TopRow::Category(c) => Some(c),
            _ => None,
        })
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .filter(|d| d.fringe_row.is_none() && !d.is_blank())
}

/// The line's own total in the base currency, worked out from its numbers
/// as the exporting program does (a double, rounded to cents), for a line
/// in the base currency; else the export's figure.
pub fn line_total(d: &XDetail) -> f64 {
    let v = |n: &Option<Num>, blank: f64| n.as_ref().map_or(blank, |n| n.value);
    if d.currency.trim().is_empty() {
        let t = v(&d.amount, 0.0) * v(&d.x, 1.0) * v(&d.rate, 0.0) * v(&d.x4, 1.0);
        (t * 100.0).round() / 100.0
    } else {
        d.subtotal
    }
}

/// What fringe `f` makes of line `d` (total `e`), as a percentage.
fn share(f: &Def, d: &XDetail, e: f64, hours: &Hours) -> f64 {
    if f.percent {
        return f.rate;
    }
    let amount = d.amount.as_ref().map_or(0.0, |n| n.value);
    let units = match (
        f.unit.as_deref().and_then(|u| hours.of(u)),
        hours.of(&d.unit),
    ) {
        (Some(fh), Some(lh)) => amount * lh / fh,
        _ => 1.0,
    };
    f.rate * units / e * 100.0
}

/// How far a line's shown percentage may be from its fringes' nominal
/// ones: it is shown to two places, and is the fringes' dollars (each
/// rounded to the cent, `fringes` of them) over the line's total `e`.
fn tolerance(e: f64, fringes: usize) -> f64 {
    0.0051 + 100.0 * 0.0051 * fringes.max(1) as f64 / e.abs()
}

/// Works out the fringes' kinds and rates from the lines (for a layout with
/// no fringe figures). Returns them as figures (with no amounts), and the
/// names that couldn't be worked out.
pub fn fringe_figures(x: &Export) -> (Vec<XFringe>, Vec<String>, Vec<String>) {
    let hours = Hours::new(x);
    let lines: Vec<(&XDetail, f64, f64)> = details(x)
        .filter(|d| !d.fringes.is_empty())
        .filter_map(|d| {
            let e = line_total(d);
            (e != 0.0).then_some(())?;
            d.fringe_percent_sum.map(|s| (d, e, s))
        })
        .collect();
    let mut names: BTreeSet<String> = details(x).flat_map(|d| d.fringes.iter().cloned()).collect();
    let mut known: BTreeMap<String, Def> = BTreeMap::new();
    let mut single_line: BTreeSet<String> = BTreeSet::new();
    loop {
        let mut obs: BTreeMap<String, Vec<(f64, &XDetail, f64)>> = BTreeMap::new();
        for (d, e, s) in &lines {
            let unknown: Vec<&String> = d
                .fringes
                .iter()
                .filter(|n| !known.contains_key(*n))
                .collect();
            if let [u] = unknown[..] {
                let rest: f64 = d
                    .fringes
                    .iter()
                    .filter_map(|n| known.get(n))
                    .map(|f| share(f, d, *e, &hours))
                    .sum();
                obs.entry(u.clone()).or_default().push((s - rest, d, *e));
            }
        }
        if obs.is_empty() {
            break;
        }
        let before = known.len();
        for (u, o) in obs {
            // The most precise line first: the largest.
            let (r0, d0, e0) = *o
                .iter()
                .max_by(|a, b| a.2.abs().total_cmp(&b.2.abs()))
                .expect("observations");
            let tol = |d: &XDetail, e: f64| tolerance(e, d.fringes.len());
            let constant = o
                .iter()
                .all(|(r, d, e)| (r - r0).abs() <= tol(d, *e) + tol(d0, e0));
            let def = if constant {
                let lo = o
                    .iter()
                    .map(|(r, d, e)| r - tol(d, *e))
                    .fold(f64::MIN, f64::max);
                let hi = o
                    .iter()
                    .map(|(r, d, e)| r + tol(d, *e))
                    .fold(f64::MAX, f64::min);
                Some(Def {
                    percent: true,
                    rate: simplest(lo.min(hi), hi.max(lo)).parse().unwrap_or(r0),
                    unit: None,
                })
            } else {
                // Flat: its dollars on each line, per the lines' unit or
                // per line.
                let dollars: Vec<(f64, &XDetail)> =
                    o.iter().map(|(r, d, e)| (r * e / 100.0, *d)).collect();
                let unit_keys: BTreeSet<String> =
                    dollars.iter().map(|(_, d)| hours.key(&d.unit)).collect();
                let per_unit: Vec<f64> = dollars
                    .iter()
                    .filter_map(|(v, d)| {
                        let a = d.amount.as_ref().map_or(0.0, |n| n.value);
                        (a != 0.0).then(|| v / a)
                    })
                    .collect();
                let same = |v: &[f64]| {
                    !v.is_empty()
                        && v.iter()
                            .all(|x| (x - v[0]).abs() <= 0.011 * v[0].abs().max(1.0))
                };
                if unit_keys.len() == 1 && same(&per_unit) && per_unit.len() == dollars.len() {
                    let unit = unit_keys.into_iter().next().filter(|u| !u.is_empty());
                    Some(Def {
                        percent: false,
                        rate: (per_unit[0] * 100.0).round() / 100.0,
                        unit,
                    })
                } else {
                    let flat: Vec<f64> = dollars.iter().map(|(v, _)| *v).collect();
                    same(&flat).then(|| Def {
                        percent: false,
                        rate: (flat[0] * 100.0).round() / 100.0,
                        unit: None,
                    })
                }
            };
            if let Some(mut def) = def {
                if o.len() == 1 {
                    single_line.insert(u.clone());
                    // One line can't tell a percentage from an amount per
                    // unit: on a line counted in a time unit, the amount
                    // per unit wins when it is the rounder number (25 a
                    // Day, not 2.5%).
                    let (r, d, e) = o[0];
                    let amount = d.amount.as_ref().map_or(0.0, |n| n.value);
                    if def.percent && hours.of(&d.unit).is_some() && amount != 0.0 {
                        let per_unit = ((r * e / 100.0) / amount * 100.0).round() / 100.0;
                        let places = |v: f64| {
                            let t = super::build::trim_number(v);
                            t.split_once('.').map_or(0, |(_, f)| f.len())
                        };
                        if per_unit > 0.0 && places(per_unit) < places(def.rate) {
                            def = Def {
                                percent: false,
                                rate: per_unit,
                                unit: Some(hours.key(&d.unit)),
                            };
                        }
                    }
                }
                known.insert(u, def);
            }
        }
        if known.len() == before {
            break;
        }
    }
    // What each percent fringe was worked out on, as near as the lines'
    // rounded percentages tell: what the line earned, less its other
    // fringes at their full rates, over this one's rate (a negative line
    // counts whole, as the export's own figures count it).
    let worked_on = |name: &str, f: &Def| -> f64 {
        if !f.percent || f.rate <= 0.0 {
            return f64::NAN;
        }
        details(x)
            .filter(|d| d.fringes.iter().any(|n| n == name))
            .map(|d| {
                let e = line_total(d);
                if e <= 0.0 {
                    return e;
                }
                let others: f64 = d
                    .fringes
                    .iter()
                    .filter(|n| *n != name)
                    .filter_map(|n| known.get(n))
                    .map(|g| share(g, d, e, &hours))
                    .sum();
                let earned = d.fringe_percent.unwrap_or(0.0);
                ((earned - others) * e / f.rate).clamp(0.0, e)
            })
            .sum()
    };
    let figures = known
        .iter()
        .map(|(name, f)| XFringe {
            percent: f.percent,
            name: name.clone(),
            rate: super::build::trim_number(f.rate),
            unit: f.unit.clone().unwrap_or_default(),
            amount: worked_on(name, f),
            total: 0.0,
        })
        .collect();
    names.retain(|n| !known.contains_key(n));
    (
        figures,
        names.into_iter().collect(),
        single_line.into_iter().collect(),
    )
}

/// Puts each line's fringes on it, from the line's fringe percentage and
/// the fringes' figures (for a layout whose lines don't name them).
/// Returns how many lines got fringes and how many couldn't be decided.
pub fn line_fringes(x: &mut Export, figures: &[XFringe]) -> (usize, usize) {
    let hours = Hours::new(x);
    let mut defs: BTreeMap<String, Def> = BTreeMap::new();
    for f in figures {
        defs.entry(f.name.clone()).or_insert_with(|| Def {
            percent: f.percent,
            rate: f.rate.trim().parse().unwrap_or(0.0),
            unit: (!f.unit.trim().is_empty()).then(|| hours.key(&f.unit)),
        });
    }
    let names: Vec<(&String, &Def)> = defs.iter().collect();
    let max = if names.len() > 40 { 3 } else { 4 };
    let (mut done, mut undecided) = (0, 0);
    for r in &mut x.rows {
        let TopRow::Category(c) = r else { continue };
        for d in c
            .accounts
            .iter_mut()
            .flat_map(|a| a.details.iter_mut())
            .filter(|d| d.fringe_row.is_none())
        {
            let Some(s) = d.fringe_percent_sum else {
                continue;
            };
            let e = line_total(d);
            if e == 0.0 || names.is_empty() {
                undecided += 1;
                continue;
            }
            let shares: Vec<f64> = names.iter().map(|(_, f)| share(f, d, e, &hours)).collect();
            // The smallest sets whose percentages add up to the line's.
            let mut found: Vec<Vec<usize>> = Vec::new();
            for size in 1..=max.min(names.len()) {
                subsets(names.len(), size, &mut |set| {
                    let sum: f64 = set.iter().map(|i| shares[*i]).sum();
                    if (sum - s).abs() <= tolerance(e, set.len()) && found.len() < 2 {
                        found.push(set.to_vec());
                    }
                });
                if !found.is_empty() {
                    break;
                }
            }
            match found.as_slice() {
                [one] => {
                    d.fringes = one.iter().map(|i| names[*i].0.clone()).collect();
                    done += 1;
                }
                _ => undecided += 1,
            }
        }
    }
    (done, undecided)
}

/// Calls `f` with every set of `k` indexes below `n`, in order.
fn subsets(n: usize, k: usize, f: &mut dyn FnMut(&[usize])) {
    fn go(start: usize, n: usize, k: usize, cur: &mut Vec<usize>, f: &mut dyn FnMut(&[usize])) {
        if cur.len() == k {
            f(cur);
            return;
        }
        for i in start..n {
            if n - i < k - cur.len() {
                break;
            }
            cur.push(i);
            go(i + 1, n, k, cur, f);
            cur.pop();
        }
    }
    go(0, n, k, &mut Vec::new(), f);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsets_in_order() {
        let mut seen = Vec::new();
        subsets(4, 2, &mut |s| seen.push(s.to_vec()));
        assert_eq!(seen.len(), 6);
        assert_eq!(seen[0], vec![0, 1]);
        assert_eq!(seen[5], vec![2, 3]);
    }
}
