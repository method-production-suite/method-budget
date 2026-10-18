//! Recalculation that reuses what an edit didn't touch.
//!
//! Stages 1 and 2 (evaluating rows and their fringes) are most of a recalc,
//! and they work account by account: an account's rows depend only on the
//! account itself and on the budget's arithmetic, globals, fringes, cutoff
//! factor, currencies, units, groups and group rules (and the recalc
//! options). An edit usually changes one account, so
//! [`RecalcCache`] keeps every account's evaluated rows, and the next recalc
//! evaluates again only the accounts whose rows changed (all of them when one
//! of those tables changed). The cache keeps rows as stage 2 leaves them,
//! before the person pass (decisions.md, entry 79): a person's cutoff
//! reaches across accounts, so that pass, like stages 3 and 4, runs in full
//! every time. The result is exactly [`recalc`](super::recalc)'s.

use std::collections::BTreeMap;

use crate::calc::{recalc_with, CalcError, RecalcOptions};
use crate::model::{
    Account, Arithmetic, Budget, Currency, CutoffFactor, Formula, Fringe, Group, GroupRules,
    UnitDef,
};
use crate::report::{Report, RowTotals};
use crate::Cents;

/// Keeps each account's evaluated rows between recalcs. Use one per budget
/// being edited; it compares contents, so it is never stale, only slower
/// when it has to start over.
#[derive(Clone, Debug, Default)]
pub struct RecalcCache {
    key: Option<Key>,
    accounts: Accounts,
}

/// Everything besides an account that its rows depend on.
#[derive(Clone, Debug)]
struct Key {
    arithmetic: Arithmetic,
    base_currency: String,
    currencies: BTreeMap<String, Currency>,
    globals: BTreeMap<String, Formula>,
    units: BTreeMap<String, UnitDef>,
    fringes: BTreeMap<String, Fringe>,
    cutoff_factor: CutoffFactor,
    groups: BTreeMap<String, Group>,
    group_rules: GroupRules,
    opts: RecalcOptions,
}

impl Key {
    fn of(b: &Budget, opts: &RecalcOptions) -> Key {
        Key {
            arithmetic: b.arithmetic,
            base_currency: b.base_currency.clone(),
            currencies: b.currencies.clone(),
            globals: b.globals.clone(),
            units: b.units.clone(),
            fringes: b.fringes.clone(),
            cutoff_factor: b.cutoff_factor,
            groups: b.groups.clone(),
            group_rules: b.group_rules,
            opts: opts.clone(),
        }
    }

    fn matches(&self, b: &Budget, opts: &RecalcOptions) -> bool {
        self.arithmetic == b.arithmetic
            && self.base_currency == b.base_currency
            && self.currencies == b.currencies
            && self.globals == b.globals
            && self.units == b.units
            && self.fringes == b.fringes
            && self.cutoff_factor == b.cutoff_factor
            && self.groups == b.groups
            && self.group_rules == b.group_rules
            && self.opts == *opts
    }
}

impl RecalcCache {
    pub fn new() -> RecalcCache {
        RecalcCache::default()
    }

    /// Recalculates `b` as [`recalc`](super::recalc) does, evaluating only the
    /// accounts that changed since the last call.
    pub fn recalc(&mut self, b: &Budget, opts: &RecalcOptions) -> Result<Report, CalcError> {
        if !self.key.as_ref().is_some_and(|k| k.matches(b, opts)) {
            self.accounts = Accounts::default();
            self.key = Some(Key::of(b, opts));
        }
        let out = recalc_with(b, opts, Some(&mut self.accounts));
        let count = b.categories().map(|c| c.accounts.len()).sum();
        self.accounts.entries.truncate(count);
        out
    }

    /// Forgets everything (the next recalc evaluates every account).
    pub fn clear(&mut self) {
        *self = RecalcCache::default();
    }
}

/// One account as last evaluated.
#[derive(Clone, Debug)]
struct Entry {
    account: Account,
    rows: Vec<RowTotals>,
    details_total: Cents,
    fringe_total: Cents,
    /// Its unfiltered rows and their sums, once worked out.
    full: Option<(Vec<RowTotals>, Cents, Cents)>,
}

/// Evaluated accounts by position in budget order.
#[derive(Clone, Debug, Default)]
pub(crate) struct Accounts {
    entries: Vec<Option<Entry>>,
}

impl Accounts {
    /// The counted rows and sums of `acct`, at position `k`, if it is the
    /// account evaluated there last time.
    pub(crate) fn counted(
        &self,
        k: usize,
        acct: &Account,
    ) -> Option<(Vec<RowTotals>, Cents, Cents)> {
        match self.entries.get(k) {
            Some(Some(e)) if e.account == *acct => {
                Some((e.rows.clone(), e.details_total, e.fringe_total))
            }
            _ => None,
        }
    }

    /// Keeps a freshly evaluated account.
    pub(crate) fn keep(
        &mut self,
        k: usize,
        acct: &Account,
        rows: &[RowTotals],
        details_total: Cents,
        fringe_total: Cents,
    ) {
        if self.entries.len() <= k {
            self.entries.resize_with(k + 1, || None);
        }
        self.entries[k] = Some(Entry {
            account: acct.clone(),
            rows: rows.to_vec(),
            details_total,
            fringe_total,
            full: None,
        });
    }

    /// The unfiltered rows at position `k`, if worked out before. Only asked
    /// after [`Accounts::counted`] or [`Accounts::keep`] settled that position
    /// in the same recalc, so the entry is the current account's.
    pub(crate) fn full(&self, k: usize) -> Option<(Vec<RowTotals>, Cents, Cents)> {
        match self.entries.get(k) {
            Some(Some(Entry {
                full: Some((rows, d, f)),
                ..
            })) => Some((rows.clone(), *d, *f)),
            _ => None,
        }
    }

    pub(crate) fn keep_full(
        &mut self,
        k: usize,
        rows: &[RowTotals],
        details_total: Cents,
        fringe_total: Cents,
    ) {
        if let Some(Some(e)) = self.entries.get_mut(k) {
            e.full = Some((rows.to_vec(), details_total, fringe_total));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::calc::recalc;
    use crate::model::{Detail, FringePosting, Inclusion, RowKind, TopsheetRow};
    use crate::sample::{self, SampleSpec};

    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) % n.max(1)
        }
    }

    fn flip(i: Inclusion) -> Inclusion {
        match i {
            Inclusion::Include => Inclusion::Exclude,
            Inclusion::Exclude => Inclusion::Include,
        }
    }

    fn details_mut(b: &mut Budget) -> Vec<&mut Detail> {
        b.categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
            .collect()
    }

    /// One random edit: cells (some that break the budget), rows added and
    /// removed, globals, fringes, groups, structure.
    fn edit(rng: &mut Rng, b: &mut Budget) {
        let n = details_mut(b).len() as u64;
        let k = rng.below(n) as usize;
        let fringes: Vec<String> = b.fringes.keys().cloned().collect();
        let groups: Vec<String> = b.groups.keys().cloned().collect();
        match rng.below(17) {
            0..=3 => details_mut(b)[k].rate = format!("{}", rng.below(9_000)).into(),
            4 => details_mut(b)[k].amount = "SHOOT_DAYS / 2".into(),
            5 => details_mut(b)[k].amount = "NOPE".into(),
            6 => {
                let f = fringes[rng.below(fringes.len() as u64) as usize].clone();
                details_mut(b)[k].fringes.insert(f);
            }
            7 => {
                let g = groups[rng.below(groups.len() as u64) as usize].clone();
                details_mut(b)[k].groups.insert(g);
            }
            8 => {
                let id = 1_000_000 + rng.below(1_000_000);
                let acct = b
                    .categories_mut()
                    .flat_map(|c| &mut c.accounts)
                    .next()
                    .unwrap();
                acct.details
                    .insert(0, Detail::line(id, "new", "2", "Day", "300"));
            }
            9 => {
                let id = details_mut(b)[k].id;
                for a in b.categories_mut().flat_map(|c| &mut c.accounts) {
                    a.details.retain(|d| d.id != id);
                }
            }
            10 => {
                b.globals
                    .insert("SHOOT_DAYS".into(), format!("{}", rng.below(80) + 1).into());
            }
            11 => {
                if let Some(f) = b.fringes.values_mut().next() {
                    f.rate = format!("{}.5", rng.below(20)).into();
                }
            }
            12 => {
                let g = groups[rng.below(groups.len() as u64) as usize].clone();
                if let Some(g) = b.groups.get_mut(&g) {
                    g.in_budget_total = !g.in_budget_total;
                }
            }
            13 => {
                // Flip the group rules that change which lines count.
                let r = &mut b.group_rules;
                if rng.below(2) == 0 {
                    r.ungrouped_lines = flip(r.ungrouped_lines);
                } else {
                    r.conflict = flip(r.conflict);
                }
            }
            14 | 15 => {
                // A person: their cutoff reaches into other accounts.
                let p = [Some("Dana Reyes"), Some("Lee Park"), None][rng.below(3) as usize];
                details_mut(b)[k].person = p.map(String::from);
            }
            _ => {
                // Move a category: every position after it changes.
                if let Some(TopsheetRow::Category(_)) = b.topsheet.first() {
                    let first = b.topsheet.remove(0);
                    b.topsheet.push(first);
                }
            }
        }
    }

    /// Over long random edit sequences (and undos, which are just edits back
    /// to an earlier budget), the cache gives exactly what a full recalc
    /// gives: the same reports and the same errors.
    #[test]
    fn cached_recalc_equals_recalc() {
        for seed in 1..=6u64 {
            let mut b = sample::budget(&SampleSpec {
                seed,
                rows: 400,
                accounts: 14,
                categories: 4,
                fringes: 12,
                groups: 5,
                baseline: false,
            });
            if seed % 2 == 0 {
                b.fringe_posting = FringePosting::Account;
            }
            if seed > 3 {
                sample::persons(&mut b, seed);
            }
            let mut cache = RecalcCache::new();
            let mut rng = Rng(seed * 104_729 + 7);
            let mut history = vec![b.clone()];
            let opts_list = [
                RecalcOptions::default(),
                RecalcOptions {
                    exclude_groups: b.groups.keys().take(1).cloned().collect(),
                    only_groups: None,
                },
                RecalcOptions {
                    exclude_groups: BTreeSet::new(),
                    only_groups: Some(b.groups.keys().skip(1).take(2).cloned().collect()),
                },
            ];
            for step in 0..80 {
                if step % 9 == 8 && history.len() > 2 {
                    // Back to an earlier state, as undo does.
                    b = history[history.len() - 3].clone();
                } else {
                    edit(&mut rng, &mut b);
                }
                history.push(b.clone());
                let opts = &opts_list[(step / 20) % opts_list.len()];
                assert_eq!(
                    cache.recalc(&b, opts),
                    recalc(&b, opts),
                    "seed {seed} step {step}"
                );
            }
        }
    }

    /// With numbers on rounding edges (money rounded to cents once, from
    /// the exact product), the cache still gives exactly what the full
    /// recalc and the frozen reference engine give, edit after edit.
    #[test]
    fn cached_recalc_equals_reference_on_rounding_edges() {
        for seed in 1..=4u64 {
            let mut b = sample::budget(&SampleSpec {
                seed,
                rows: 300,
                accounts: 10,
                categories: 3,
                fringes: 10,
                groups: 4,
                baseline: false,
            });
            sample::fine_decimals(&mut b, seed);
            if seed % 2 == 1 {
                b.fringe_posting = FringePosting::Account;
            }
            // Cutoffs times a factor that can land on half a cent.
            if seed > 2 {
                b.cutoff_factor = crate::model::CutoffFactor::OneAndAHalf;
            }
            let opts = RecalcOptions::default();
            let mut cache = RecalcCache::new();
            let mut rng = Rng(seed * 7_919 + 3);
            for step in 0..40 {
                if step > 0 {
                    edit(&mut rng, &mut b);
                    if step % 4 == 0 {
                        sample::fine_decimals(&mut b, seed * 100 + step);
                    }
                }
                let want = super::super::reference::recalc(&b, &opts);
                assert_eq!(cache.recalc(&b, &opts), want, "seed {seed} step {step}");
                assert_eq!(recalc(&b, &opts), want, "seed {seed} step {step}");
            }
        }
    }

    /// One random edit to how flat fringes count: a fringe's basis, cap or
    /// unit, a unit's hours, or a line's X, 4X and their units.
    fn flat_edit(rng: &mut Rng, b: &mut Budget) {
        use crate::model::{FlatBasis, FringeKind};
        let n = details_mut(b).len() as u64;
        let k = rng.below(n) as usize;
        let flats: Vec<String> = b
            .fringes
            .iter()
            .filter(|(_, f)| matches!(f.kind, FringeKind::Flat { .. }))
            .map(|(n, _)| n.clone())
            .collect();
        let pick = flats.get(rng.below(flats.len() as u64) as usize).cloned();
        let flat = pick.and_then(|n| b.fringes.get_mut(&n));
        match (rng.below(6), flat) {
            (0, Some(f)) => {
                if let FringeKind::Flat { basis, .. } = &mut f.kind {
                    *basis = FlatBasis::ALL[rng.below(3) as usize];
                }
            }
            (1, Some(f)) => {
                if let FringeKind::Flat {
                    max_hours_per_day, ..
                } = &mut f.kind
                {
                    *max_hours_per_day =
                        (rng.below(3) > 0).then(|| crate::Dec::from_int(rng.below(16) as i64 + 1));
                }
            }
            (2, Some(f)) => {
                if let FringeKind::Flat { unit, .. } = &mut f.kind {
                    *unit = ["Hour", "Day", "Week", "Episode"]
                        .get(rng.below(5) as usize)
                        .map(|u| u.to_string());
                }
            }
            (3, _) => {
                if let Some(u) = b.units.get_mut("Day") {
                    u.hours = Some(crate::Dec::from_int(rng.below(14) as i64));
                }
            }
            (4, _) => {
                let d = &mut details_mut(b)[k];
                d.x = format!("{}", rng.below(16)).into();
                d.unit2 = ["", "Hour", "Episode", "Week"][rng.below(4) as usize].into();
            }
            _ => {
                let d = &mut details_mut(b)[k];
                d.x4 = format!("{}", rng.below(4) + 1).into();
                d.unit4 = ["", "Episode", "Day"][rng.below(3) as usize].into();
            }
        }
    }

    /// With flat fringes counted every way (each basis, caps, hours a day
    /// from X, counts in X and 4X), the cache gives exactly what the full
    /// recalc and the frozen reference engine give, edit after edit.
    #[test]
    fn cached_recalc_equals_reference_with_every_flat_basis() {
        for seed in 1..=4u64 {
            let mut b = sample::budget(&SampleSpec {
                seed,
                rows: 300,
                accounts: 10,
                categories: 3,
                fringes: 15,
                groups: 4,
                baseline: false,
            });
            sample::flat_bases(&mut b, seed);
            if seed % 2 == 0 {
                sample::fine_decimals(&mut b, seed);
            }
            let opts = RecalcOptions::default();
            let mut cache = RecalcCache::new();
            let mut rng = Rng(seed * 6_247 + 11);
            for step in 0..60 {
                if step > 0 {
                    if step % 3 == 0 {
                        edit(&mut rng, &mut b);
                    } else {
                        flat_edit(&mut rng, &mut b);
                    }
                }
                let want = super::super::reference::recalc(&b, &opts);
                assert_eq!(cache.recalc(&b, &opts), want, "seed {seed} step {step}");
                assert_eq!(recalc(&b, &opts), want, "seed {seed} step {step}");
            }
        }
    }

    #[test]
    fn unchanged_accounts_are_not_evaluated_again() {
        let mut b = sample::budget(&SampleSpec {
            rows: 200,
            accounts: 8,
            categories: 2,
            ..SampleSpec::default()
        });
        let opts = RecalcOptions::default();
        let mut cache = RecalcCache::new();
        cache.recalc(&b, &opts).unwrap();
        // Poison account 3's cached rows: a reused account shows the poison.
        if let Some(Some(e)) = cache.accounts.entries.get_mut(3) {
            if let Some(r) = e.rows.iter_mut().find(|r| r.kind == RowKind::Line) {
                r.quantity = crate::decimal::Dec::from_int(-7);
            }
        }
        // Edit another account: account 3 comes from the cache.
        let first = b
            .categories_mut()
            .flat_map(|c| &mut c.accounts)
            .next()
            .unwrap();
        first.details[0].rate = "1".into();
        let r = cache.recalc(&b, &opts).unwrap();
        let acct3 = r
            .categories
            .iter()
            .flat_map(|c| &c.accounts)
            .nth(3)
            .unwrap();
        assert!(acct3
            .rows
            .iter()
            .any(|r| r.quantity == crate::decimal::Dec::from_int(-7)));
        // A global change starts over.
        b.globals.insert("SHOOT_DAYS".into(), "43".into());
        let r = cache.recalc(&b, &opts).unwrap();
        assert_eq!(r, recalc(&b, &opts).unwrap());
    }
}
