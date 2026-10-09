//! Original vs current: freezing totals into a [`Baseline`] and diffing against it.

use std::collections::BTreeMap;

use crate::model::{Baseline, RowId};
use crate::report::Report;
use crate::Cents;

/// One compared figure. `None` means it didn't exist on that side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub original: Option<Cents>,
    pub current: Option<Cents>,
}

impl Change {
    /// Current minus original, treating a missing side as 0.
    pub fn variance(&self) -> Option<Cents> {
        self.current
            .unwrap_or(0)
            .checked_sub(self.original.unwrap_or(0))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variance {
    pub rows: BTreeMap<RowId, Change>,
    pub accounts: BTreeMap<String, Change>,
    pub categories: BTreeMap<String, Change>,
    pub grand_total: Change,
    pub net_total: Change,
}

/// Adds `v` under `k`. Codes should be unique; if not, their totals combine.
fn add<K: Ord>(m: &mut BTreeMap<K, Cents>, k: K, v: Cents) -> Option<()> {
    let slot = m.entry(k).or_insert(0);
    *slot = slot.checked_add(v)?;
    Some(())
}

impl Baseline {
    /// Freezes the totals in `r`: each costed row, account and category total as
    /// shown on the topsheet, and the grand and net totals. `None` on overflow.
    pub fn capture(r: &Report) -> Option<Baseline> {
        let mut b = Baseline {
            grand_total: r.grand_total,
            net_total: r.net_total,
            ..Default::default()
        };
        for cat in &r.categories {
            add(&mut b.categories, cat.code.clone(), cat.total)?;
            for acct in &cat.accounts {
                add(&mut b.accounts, acct.code.clone(), acct.total)?;
                for row in acct.rows.iter().filter(|r| r.kind.is_costed()) {
                    b.rows.insert(row.id, row.total);
                }
            }
        }
        Some(b)
    }
}

fn diff<K: Ord + Clone>(
    orig: &BTreeMap<K, Cents>,
    cur: &BTreeMap<K, Cents>,
) -> BTreeMap<K, Change> {
    let mut out: BTreeMap<K, Change> = orig
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                Change {
                    original: Some(*v),
                    current: None,
                },
            )
        })
        .collect();
    for (k, v) in cur {
        out.entry(k.clone())
            .or_insert(Change {
                original: None,
                current: None,
            })
            .current = Some(*v);
    }
    out
}

/// Compares `current` against `baseline`. `None` on overflow.
pub fn variance(baseline: &Baseline, current: &Report) -> Option<Variance> {
    let now = Baseline::capture(current)?;
    Some(Variance {
        rows: diff(&baseline.rows, &now.rows),
        accounts: diff(&baseline.accounts, &now.accounts),
        categories: diff(&baseline.categories, &now.categories),
        grand_total: Change {
            original: Some(baseline.grand_total),
            current: Some(now.grand_total),
        },
        net_total: Change {
            original: Some(baseline.net_total),
            current: Some(now.net_total),
        },
    })
}
