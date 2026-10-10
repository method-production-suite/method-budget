//! Budget diff and three-way merge (roadmap 3.1 and 3.2): scenarios and
//! property tests over random budgets and random edits.
//!
//! Uses a small seeded generator instead of a property-testing crate, so the
//! engine stays dependency-free. Each failure prints its seed for replay.

mod common;

use std::collections::BTreeMap;

use common::*;
use fwb_core::diff::{diff, ChangeKind, Place};
use fwb_core::merge::{base_as_shared, merge, merge_with, Side};
use fwb_core::*;

const CASES: u64 = 300;

/// xorshift64*: tiny, deterministic, good enough for test data.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }

    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }

    fn pick<T: Clone>(&mut self, items: &[T]) -> Option<T> {
        if items.is_empty() {
            None
        } else {
            Some(items[self.below(items.len() as u64) as usize].clone())
        }
    }
}

fn fringe(rate: &str, kind: FringeKind) -> Fringe {
    Fringe {
        short_code: None,
        description: String::new(),
        rate: rate.into(),
        kind,
    }
}

fn gen_budget(rng: &mut Rng) -> Budget {
    let mut b = Budget::new("random", "USD");
    b.currencies.insert(
        "CAD".into(),
        Currency {
            description: String::new(),
            symbol: String::new(),
            rate: d("0.73"),
        },
    );
    b.globals
        .insert("DAYS".into(), (rng.below(60) as i64 + 1).into());
    b.globals.insert("WEEKS".into(), "DAYS / 5".into());
    b.globals.insert("PREP".into(), "4".into());
    b.fringes.insert(
        "PCT".into(),
        fringe("18.5", FringeKind::Percent { cutoff: None }),
    );
    b.fringes.insert(
        "CAPPED".into(),
        fringe(
            "7.65",
            FringeKind::Percent {
                cutoff: Some(2_000_000),
            },
        ),
    );
    b.fringes
        .insert("FLAT".into(), fringe("25", FringeKind::Flat { unit: None }));
    b.fringes.insert(
        "DAILY".into(),
        fringe(
            "12.5",
            FringeKind::Flat {
                unit: Some("Day".into()),
            },
        ),
    );
    b.groups.insert("A".into(), group(true));
    b.groups.insert("B".into(), group(true));
    b.groups.insert("OFF".into(), group(false));
    let (mut id, mut code) = (0, 1000);
    for c in 0..=rng.below(4) {
        let mut accounts = Vec::new();
        for _ in 0..=rng.below(4) {
            code += 10;
            let mut details = Vec::new();
            for _ in 0..rng.below(7) {
                id += 1;
                details.push(gen_line(rng, id));
            }
            accounts.push(account(&code.to_string(), &format!("acct {code}"), details));
        }
        b.topsheet
            .push(category(&format!("{c}000"), &format!("cat {c}"), accounts));
        if c == 0 {
            b.topsheet.push(level("Total above-the-line"));
        }
    }
    b.charges.push(percent_charge("Contingency", "10"));
    b.charges.push(percent_charge("Fee", "2"));
    b
}

fn gen_line(rng: &mut Rng, id: RowId) -> Detail {
    if rng.chance(8) {
        return Detail::subtotal(id, "sub");
    }
    let amount = match rng.below(3) {
        0 => "DAYS".to_string(),
        1 => "WEEKS".to_string(),
        _ => rng.below(50).to_string(),
    };
    let rate = format!("{}.{:02}", rng.below(5_000), rng.below(100));
    let unit = ["Day", "Week", "Flat", ""][rng.below(4) as usize];
    let mut det = Detail::line(
        id,
        format!("line {id}"),
        amount.as_str(),
        unit,
        rate.as_str(),
    );
    if rng.chance(20) {
        det.currency = Some("CAD".into());
    }
    for f in ["PCT", "CAPPED", "FLAT", "DAILY"] {
        if rng.chance(25) {
            det.fringes.insert(f.into());
        }
    }
    for g in ["A", "B", "OFF"] {
        if rng.chance(15) {
            det.groups.insert(g.into());
        }
    }
    det
}

// ---------------------------------------------------------------------------
// Edits that name things by id, code and name, so they apply to any budget
// made from the same base.

#[derive(Clone, Debug)]
enum Edit {
    Rate(RowId, String),
    Desc(RowId, String),
    Tag(RowId, String),
    Untag(RowId, String),
    DeleteRow(RowId),
    AddRow {
        account: String,
        after: Option<RowId>,
        row: Box<Detail>,
    },
    MoveWithin {
        id: RowId,
        after: Option<RowId>,
    },
    MoveTo {
        id: RowId,
        account: String,
    },
    RenameAccount(String, String),
    RenameCategory(String, String),
    AddAccount {
        category: String,
        code: String,
    },
    DeleteAccount(String),
    FringeRate(String, String),
    GlobalValue(String, String),
    AddGlobal(String, String),
    GroupColor(String, String),
    ChargePercent(String, String),
    Title(String),
    AccountNote(String, String),
    RowNote(RowId, String),
}

fn find_row(b: &mut Budget, id: RowId) -> Option<&mut Detail> {
    b.categories_mut()
        .flat_map(|c| c.accounts.iter_mut())
        .flat_map(|a| a.details.iter_mut())
        .find(|d| d.id == id)
}

fn find_account<'a>(b: &'a mut Budget, code: &str) -> Option<&'a mut Account> {
    b.categories_mut()
        .flat_map(|c| c.accounts.iter_mut())
        .find(|a| a.code == code)
}

fn take_row(b: &mut Budget, id: RowId) -> Option<Detail> {
    for a in b.categories_mut().flat_map(|c| c.accounts.iter_mut()) {
        if let Some(i) = a.details.iter().position(|d| d.id == id) {
            return Some(a.details.remove(i));
        }
    }
    None
}

fn insert_after(a: &mut Account, after: Option<RowId>, row: Detail) {
    let at = match after {
        None => 0,
        Some(x) => a
            .details
            .iter()
            .position(|d| d.id == x)
            .map_or(a.details.len(), |i| i + 1),
    };
    a.details.insert(at, row);
}

impl Edit {
    fn apply(&self, b: &mut Budget) {
        match self {
            Edit::Rate(id, v) => {
                if let Some(d) = find_row(b, *id) {
                    d.rate = v.as_str().into();
                }
            }
            Edit::Desc(id, v) => {
                if let Some(d) = find_row(b, *id) {
                    d.description = v.clone();
                }
            }
            Edit::Tag(id, f) => {
                if let Some(d) = find_row(b, *id) {
                    d.fringes.insert(f.clone());
                }
            }
            Edit::Untag(id, f) => {
                if let Some(d) = find_row(b, *id) {
                    d.fringes.remove(f);
                }
            }
            Edit::DeleteRow(id) => {
                take_row(b, *id);
            }
            Edit::AddRow {
                account,
                after,
                row,
            } => {
                if let Some(a) = find_account(b, account) {
                    insert_after(a, *after, (**row).clone());
                }
            }
            Edit::MoveWithin { id, after } => {
                let Some(code) = b
                    .categories()
                    .flat_map(|c| &c.accounts)
                    .find(|a| a.details.iter().any(|d| d.id == *id))
                    .map(|a| a.code.clone())
                else {
                    return;
                };
                if *after == Some(*id) {
                    return;
                }
                let row = take_row(b, *id).unwrap();
                insert_after(find_account(b, &code).unwrap(), *after, row);
            }
            Edit::MoveTo { id, account } => {
                if find_account(b, account).is_none() {
                    return;
                }
                if let Some(row) = take_row(b, *id) {
                    find_account(b, account).unwrap().details.push(row);
                }
            }
            Edit::RenameAccount(code, name) => {
                if let Some(a) = find_account(b, code) {
                    a.name = name.clone();
                }
            }
            Edit::RenameCategory(code, name) => {
                if let Some(c) = b.categories_mut().find(|c| c.code == *code) {
                    c.name = name.clone();
                }
            }
            Edit::AddAccount { category, code } => {
                if let Some(c) = b.categories_mut().find(|c| c.code == *category) {
                    c.accounts.push(account(code, "new account", Vec::new()));
                }
            }
            Edit::DeleteAccount(code) => {
                for c in b.categories_mut() {
                    c.accounts.retain(|a| a.code != *code);
                }
            }
            Edit::FringeRate(name, v) => {
                if let Some(f) = b.fringes.get_mut(name) {
                    f.rate = v.as_str().into();
                }
            }
            Edit::GlobalValue(name, v) => {
                if b.globals.contains_key(name) {
                    b.globals.insert(name.clone(), v.as_str().into());
                }
            }
            Edit::AddGlobal(name, v) => {
                b.globals.insert(name.clone(), v.as_str().into());
            }
            Edit::GroupColor(name, c) => {
                if let Some(g) = b.groups.get_mut(name) {
                    g.color = Some(c.clone());
                }
            }
            Edit::ChargePercent(name, p) => {
                if let Some(c) = b.charges.iter_mut().find(|c| c.name == *name) {
                    c.kind = ChargeKind::Percent(d(p));
                }
            }
            Edit::Title(t) => b.title = t.clone(),
            Edit::AccountNote(code, text) => {
                if let Some(a) = find_account(b, code) {
                    let id = next_note_id(&a.notes);
                    a.notes.push(Note::plain(id, text.as_str()));
                }
            }
            Edit::RowNote(id, text) => {
                if let Some(d) = find_row(b, *id) {
                    let n = next_note_id(&d.notes);
                    d.notes.push(Note::plain(n, text.as_str()));
                }
            }
        }
    }
}

fn apply_all(b: &Budget, edits: &[Edit]) -> Budget {
    let mut out = b.clone();
    for e in edits {
        e.apply(&mut out);
    }
    out
}

fn row_ids(b: &Budget) -> Vec<RowId> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .map(|d| d.id)
        .collect()
}

fn account_codes(b: &Budget) -> Vec<String> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .map(|a| a.code.clone())
        .collect()
}

/// What one side may touch, so two sides' edits never overlap: rows by id
/// parity, accounts, categories and setup items by place.
struct Pool {
    parity: u64,
}

impl Pool {
    fn row(&self, id: RowId) -> bool {
        id % 2 == self.parity
    }

    fn nth(&self, i: usize) -> bool {
        i as u64 % 2 == self.parity
    }
}

/// Random edits of `b`. With a pool, only what the pool owns is touched and
/// nothing is deleted that the other side might use; `new_ids` numbers
/// added rows.
fn gen_edits(
    rng: &mut Rng,
    b: &Budget,
    pool: Option<&Pool>,
    new_ids: RowId,
    tag: &str,
) -> Vec<Edit> {
    let mut work = b.clone();
    let mut edits = Vec::new();
    let mut next_id = new_ids;
    let mut next_code = 0;
    let n = 1 + rng.below(8);
    for _ in 0..n {
        let rows: Vec<RowId> = row_ids(&work)
            .into_iter()
            .filter(|id| pool.is_none_or(|p| p.row(*id)))
            .collect();
        let base_codes = account_codes(b);
        let accounts: Vec<String> = account_codes(&work)
            .into_iter()
            .filter(|c| match pool {
                None => true,
                Some(p) => {
                    base_codes
                        .iter()
                        .position(|x| x == c)
                        .is_some_and(|i| p.nth(i))
                        || c.starts_with(tag)
                }
            })
            .collect();
        let cats: Vec<String> = work
            .categories()
            .enumerate()
            .filter(|(i, _)| pool.is_none_or(|p| p.nth(*i)))
            .map(|(_, c)| c.code.clone())
            .collect();
        let fringes: Vec<String> = work
            .fringes
            .keys()
            .enumerate()
            .filter(|(i, _)| pool.is_none_or(|p| p.nth(*i)))
            .map(|(_, k)| k.clone())
            .collect();
        let globals: Vec<String> = ["DAYS", "PREP"]
            .iter()
            .enumerate()
            .filter(|(i, _)| pool.is_none_or(|p| p.nth(*i)))
            .map(|(_, k)| k.to_string())
            .collect();
        let edit = match rng.below(19) {
            0 | 1 => rng
                .pick(&rows)
                .map(|id| Edit::Rate(id, format!("{}.5", rng.below(3000)))),
            2 => rng
                .pick(&rows)
                .map(|id| Edit::Desc(id, format!("{tag} desc {}", rng.below(100)))),
            3 => rng
                .pick(&rows)
                .map(|id| Edit::Tag(id, ["PCT", "FLAT"][rng.below(2) as usize].into())),
            4 => rng
                .pick(&rows)
                .map(|id| Edit::Untag(id, ["PCT", "CAPPED"][rng.below(2) as usize].into())),
            5 => rng.pick(&rows).map(Edit::DeleteRow),
            6 | 7 => rng.pick(&accounts).map(|code| {
                // Anchored to rows the other side can't delete or move.
                let ids: Vec<RowId> = work
                    .categories()
                    .flat_map(|c| &c.accounts)
                    .find(|a| a.code == code)
                    .map(|a| {
                        a.details
                            .iter()
                            .map(|d| d.id)
                            .filter(|id| pool.is_none_or(|p| p.row(*id)))
                            .collect()
                    })
                    .unwrap_or_default();
                next_id += 1;
                Edit::AddRow {
                    account: code,
                    after: rng.pick(&ids),
                    row: Box::new(Detail::line(
                        next_id,
                        format!("{tag} new"),
                        "2",
                        "Day",
                        format!("{}", 100 + rng.below(900)).as_str(),
                    )),
                }
            }),
            8 => {
                // Within an account the pool owns.
                rng.pick(&accounts).and_then(|code| {
                    let ids: Vec<RowId> = work
                        .categories()
                        .flat_map(|c| &c.accounts)
                        .find(|a| a.code == code)
                        .map(|a| {
                            a.details
                                .iter()
                                .map(|d| d.id)
                                .filter(|id| pool.is_none_or(|p| p.row(*id)))
                                .collect()
                        })
                        .unwrap_or_default();
                    let id = rng.pick(&ids)?;
                    Some(Edit::MoveWithin {
                        id,
                        after: rng.pick(&ids),
                    })
                })
            }
            9 => match (rng.pick(&rows), rng.pick(&accounts)) {
                (Some(id), Some(account)) => Some(Edit::MoveTo { id, account }),
                _ => None,
            },
            10 => rng
                .pick(&accounts)
                .map(|c| Edit::RenameAccount(c, format!("{tag} name {}", rng.below(100)))),
            11 => rng
                .pick(&cats)
                .map(|c| Edit::RenameCategory(c, format!("{tag} cat {}", rng.below(100)))),
            12 => rng.pick(&cats).map(|category| {
                next_code += 1;
                Edit::AddAccount {
                    category,
                    code: format!("{tag}{next_code}"),
                }
            }),
            13 => {
                if pool.is_none() {
                    rng.pick(&accounts).map(Edit::DeleteAccount)
                } else {
                    None
                }
            }
            14 => rng
                .pick(&fringes)
                .map(|f| Edit::FringeRate(f, format!("{}", 1 + rng.below(30)))),
            15 => rng
                .pick(&globals)
                .map(|g| Edit::GlobalValue(g, format!("{}", 1 + rng.below(60)))),
            16 => Some(Edit::AddGlobal(
                format!("{tag}_G{}", rng.below(5)),
                format!("{}", rng.below(9)),
            )),
            17 => match pool {
                Some(p) if p.parity == 1 => Some(Edit::GroupColor("A".into(), "#2060a0".into())),
                Some(_) => Some(Edit::ChargePercent(
                    "Contingency".into(),
                    format!("{}", rng.below(20)),
                )),
                None => Some(Edit::Title(format!("{tag} title"))),
            },
            _ => {
                if rng.chance(50) {
                    rng.pick(&accounts)
                        .map(|c| Edit::AccountNote(c, format!("{tag} note")))
                } else {
                    rng.pick(&rows)
                        .map(|id| Edit::RowNote(id, format!("{tag} note")))
                }
            }
        };
        if let Some(e) = edit {
            e.apply(&mut work);
            edits.push(e);
        }
    }
    edits
}

fn unique_ids(b: &Budget) -> bool {
    let ids = row_ids(b);
    let set: std::collections::BTreeSet<_> = ids.iter().collect();
    set.len() == ids.len()
}

fn totals(b: &Budget) -> (Cents, Cents, Vec<Cents>) {
    let r = recalc(b, &RecalcOptions::default()).unwrap();
    let accts = r
        .categories
        .iter()
        .flat_map(|c| &c.accounts)
        .map(|a| a.total)
        .collect();
    (r.grand_total, r.net_total, accts)
}

// ---------------------------------------------------------------------------
// Properties

#[test]
fn diff_of_a_budget_with_itself_is_empty() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let b = gen_budget(&mut rng);
        let d = diff(&b, &b);
        assert!(d.is_empty(), "seed {seed}: {:?}", d.items);
        // And a budget changed is never empty.
        let edits = gen_edits(&mut rng, &b, None, 10_000, "x");
        let x = apply_all(&b, &edits);
        if x != b {
            assert!(!diff(&b, &x).is_empty(), "seed {seed}: {edits:?}");
        }
    }
}

#[test]
fn merging_with_an_unchanged_side_gives_the_changed_side() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let base = gen_budget(&mut rng);
        let edits = gen_edits(&mut rng, &base, None, 10_000, "x");
        let x = apply_all(&base, &edits);
        let m = merge(&base, &x, &base);
        assert!(m.conflicts.is_empty(), "seed {seed}: {:?}", m.conflicts);
        assert_eq!(
            m.budget, x,
            "seed {seed}: merge(base, x, base) with {edits:?}"
        );
        let m = merge(&base, &base, &x);
        assert!(m.conflicts.is_empty(), "seed {seed}: {:?}", m.conflicts);
        assert_eq!(
            m.budget, x,
            "seed {seed}: merge(base, base, x) with {edits:?}"
        );
        let m = merge(&base, &x, &x);
        assert!(m.conflicts.is_empty(), "seed {seed}");
        assert_eq!(m.budget, x, "seed {seed}: merge(base, x, x)");
        assert_eq!(m.renumbered, 0);
    }
}

#[test]
fn edits_that_dont_overlap_merge_cleanly_commute_and_total_as_recalculated() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let base = gen_budget(&mut rng);
        let o_edits = gen_edits(&mut rng, &base, Some(&Pool { parity: 0 }), 10_000, "O");
        let t_edits = gen_edits(&mut rng, &base, Some(&Pool { parity: 1 }), 20_000, "T");
        let ours = apply_all(&base, &o_edits);
        let theirs = apply_all(&base, &t_edits);
        let m = merge(&base, &ours, &theirs);
        assert!(
            m.conflicts.is_empty(),
            "seed {seed}: {:?}\nours {o_edits:?}\ntheirs {t_edits:?}",
            m.conflicts
        );
        assert!(unique_ids(&m.budget), "seed {seed}");
        // Commutes.
        let swapped = merge(&base, &theirs, &ours);
        assert!(swapped.conflicts.is_empty(), "seed {seed}");
        assert_eq!(
            m.budget, swapped.budget,
            "seed {seed}: ours {o_edits:?}\ntheirs {t_edits:?}"
        );
        // The same as making both sets of edits one after the other, so the
        // merged totals are what recalculating that budget gives.
        let both = apply_all(&ours, &t_edits);
        assert_eq!(
            m.budget, both,
            "seed {seed}: ours {o_edits:?}\ntheirs {t_edits:?}"
        );
        assert_eq!(totals(&m.budget), totals(&both), "seed {seed}");
        // And the diff's money is the recalculated difference.
        let dm = diff(&base, &m.budget);
        let grand = totals(&m.budget).0 - totals(&base).0;
        let listed = dm
            .totals
            .iter()
            .find(|l| l.label == "Grand total")
            .map_or(0, |l| l.delta);
        assert_eq!(listed, grand, "seed {seed}");
    }
}

#[test]
fn any_two_sets_of_edits_merge_into_a_sound_budget() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed ^ 0xABCD);
        let base = gen_budget(&mut rng);
        let o = apply_all(&base, &gen_edits(&mut rng, &base, None, 10_000, "O"));
        let t = apply_all(&base, &gen_edits(&mut rng, &base, None, 10_000, "T"));
        for side in [Side::Ours, Side::Theirs] {
            let m = merge_with(&base, &o, &t, side, &BTreeMap::new());
            assert!(unique_ids(&m.budget), "seed {seed}");
            assert!(m.conflicts.iter().all(|c| c.taken == side), "seed {seed}");
            // Every conflict can be picked either way.
            let flipped: BTreeMap<String, Side> = m
                .conflicts
                .iter()
                .map(|c| {
                    (
                        c.key.clone(),
                        if side == Side::Ours {
                            Side::Theirs
                        } else {
                            Side::Ours
                        },
                    )
                })
                .collect();
            let f = merge_with(&base, &o, &t, side, &flipped);
            assert!(unique_ids(&f.budget), "seed {seed}");
            assert!(f
                .conflicts
                .iter()
                .all(|c| flipped.get(&c.key).is_none_or(|s| *s == c.taken)));
        }
    }
}

// ---------------------------------------------------------------------------
// Scenarios

fn sample_base() -> Budget {
    let mut rng = Rng(7);
    loop {
        let b = gen_budget(&mut rng);
        if row_ids(&b).len() >= 6 && account_codes(&b).len() >= 3 {
            return b;
        }
    }
}

fn first_rows(b: &Budget, n: usize) -> Vec<RowId> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .filter(|d| d.kind == RowKind::Line)
        .map(|d| d.id)
        .take(n)
        .collect()
}

#[test]
fn the_same_field_changed_both_ways_is_a_conflict_ours_by_default() {
    let base = sample_base();
    let id = first_rows(&base, 1)[0];
    let ours = apply_all(&base, &[Edit::Rate(id, "1350".into())]);
    let theirs = apply_all(&base, &[Edit::Rate(id, "1400".into())]);
    let m = merge(&base, &ours, &theirs);
    assert_eq!(m.conflicts.len(), 1, "{:?}", m.conflicts);
    let c = &m.conflicts[0];
    assert_eq!(c.key, format!("row:{id}:rate"));
    assert_eq!(c.taken, Side::Ours);
    assert_eq!(c.ours, "1,350.00");
    assert_eq!(c.theirs, "1,400.00");
    assert!(
        c.text.contains("rate: ours 1,350.00, theirs 1,400.00, was"),
        "{}",
        c.text
    );
    assert_eq!(m.budget, ours);
    // Picking theirs.
    let picks = BTreeMap::from([(c.key.clone(), Side::Theirs)]);
    let m = merge_with(&base, &ours, &theirs, Side::Ours, &picks);
    assert_eq!(m.budget, theirs);
    assert_eq!(m.conflicts[0].taken, Side::Theirs);
    // Different fields of one row merge.
    let theirs = apply_all(
        &base,
        &[
            Edit::Desc(id, "Renamed".into()),
            Edit::Tag(id, "FLAT".into()),
        ],
    );
    let ours = apply_all(
        &base,
        &[Edit::Rate(id, "1350".into()), Edit::Tag(id, "PCT".into())],
    );
    let m = merge(&base, &ours, &theirs);
    assert!(m.conflicts.is_empty());
    let both = apply_all(
        &ours,
        &[
            Edit::Desc(id, "Renamed".into()),
            Edit::Tag(id, "FLAT".into()),
        ],
    );
    assert_eq!(m.budget, both);
}

#[test]
fn deleted_on_one_side_and_changed_on_the_other_is_a_conflict() {
    let base = sample_base();
    let id = first_rows(&base, 1)[0];
    let ours = apply_all(&base, &[Edit::DeleteRow(id)]);
    let theirs = apply_all(&base, &[Edit::Rate(id, "99".into())]);
    let m = merge(&base, &ours, &theirs);
    assert_eq!(m.conflicts.len(), 1);
    assert_eq!(m.conflicts[0].key, format!("row:{id}"));
    assert_eq!(m.conflicts[0].ours, "deleted");
    assert!(
        m.conflicts[0].theirs.starts_with("changed: rate"),
        "{}",
        m.conflicts[0].theirs
    );
    assert_eq!(m.budget, ours, "ours (deleted) by default");
    let m = merge_with(&base, &ours, &theirs, Side::Theirs, &BTreeMap::new());
    assert_eq!(m.budget, theirs);
    // Deleted on one side, untouched on the other: just deleted.
    let m = merge(&base, &base, &ours);
    assert!(m.conflicts.is_empty());
    assert_eq!(m.budget, ours);
}

#[test]
fn rows_added_on_both_sides_with_one_id_are_both_kept() {
    let base = sample_base();
    let next = base.highest_row_id().unwrap() + 1;
    let codes = account_codes(&base);
    let ours = apply_all(
        &base,
        &[Edit::AddRow {
            account: codes[0].clone(),
            after: None,
            row: Box::new(Detail::line(next, "Dolly grip", "10", "Day", "450")),
        }],
    );
    let theirs = apply_all(
        &base,
        &[Edit::AddRow {
            account: codes[1].clone(),
            after: None,
            row: Box::new(Detail::line(next, "Gaffer", "5", "Day", "500")),
        }],
    );
    let m = merge(&base, &ours, &theirs);
    assert!(m.conflicts.is_empty());
    assert_eq!(m.renumbered, 1);
    assert!(unique_ids(&m.budget));
    let descs: Vec<&str> = m
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .map(|d| d.description.as_str())
        .collect();
    assert!(descs.contains(&"Dolly grip") && descs.contains(&"Gaffer"));
    // The same row added on both sides is one row.
    let m = merge(&base, &ours, &ours.clone());
    assert_eq!(m.budget, ours);
}

#[test]
fn an_account_deleted_on_one_side_and_changed_on_the_other_is_one_conflict() {
    let base = sample_base();
    let (code, row) = base
        .categories()
        .flat_map(|c| &c.accounts)
        .find_map(|a| a.details.first().map(|d| (a.code.clone(), d.id)))
        .unwrap();
    let ours = apply_all(&base, &[Edit::DeleteAccount(code.clone())]);
    let theirs = apply_all(
        &base,
        &[
            Edit::Rate(row, "77".into()),
            Edit::RenameAccount(code.clone(), "Kept".into()),
        ],
    );
    let m = merge(&base, &ours, &theirs);
    assert_eq!(m.conflicts.len(), 1, "{:?}", m.conflicts);
    assert!(m.conflicts[0].key.starts_with("account:"));
    assert_eq!(m.budget, ours);
    let picks = BTreeMap::from([(m.conflicts[0].key.clone(), Side::Theirs)]);
    let m = merge_with(&base, &ours, &theirs, Side::Ours, &picks);
    assert_eq!(m.budget, theirs, "kept as theirs has it");
}

#[test]
fn setup_items_merge_field_by_field() {
    let base = sample_base();
    let mut ours = base.clone();
    ours.fringes.get_mut("PCT").unwrap().rate = "20".into();
    let mut theirs = base.clone();
    theirs.fringes.get_mut("PCT").unwrap().description = "Payroll".into();
    theirs.globals.insert("NEW".into(), "3".into());
    let m = merge(&base, &ours, &theirs);
    assert!(m.conflicts.is_empty());
    let f = &m.budget.fringes["PCT"];
    assert_eq!(f.rate.0, "20");
    assert_eq!(f.description, "Payroll");
    assert!(m.budget.globals.contains_key("NEW"));
    // A fringe deleted on one side and edited on the other.
    let mut ours = base.clone();
    ours.fringes.remove("FLAT");
    let mut theirs = base.clone();
    theirs.fringes.get_mut("FLAT").unwrap().rate = "30".into();
    let m = merge(&base, &ours, &theirs);
    assert_eq!(m.conflicts.len(), 1);
    assert_eq!(m.conflicts[0].key, "fringe:FLAT");
}

#[test]
fn reordering_on_both_sides_is_a_conflict() {
    let base = sample_base();
    let (code, ids) = base
        .categories()
        .flat_map(|c| &c.accounts)
        .find(|a| a.details.len() >= 3)
        .map(|a| {
            (
                a.code.clone(),
                a.details.iter().map(|d| d.id).collect::<Vec<_>>(),
            )
        })
        .unwrap();
    let _ = code;
    let ours = apply_all(
        &base,
        &[Edit::MoveWithin {
            id: ids[0],
            after: Some(ids[2]),
        }],
    );
    let theirs = apply_all(
        &base,
        &[Edit::MoveWithin {
            id: ids[2],
            after: None,
        }],
    );
    let m = merge(&base, &ours, &theirs);
    assert_eq!(m.conflicts.len(), 1, "{:?}", m.conflicts);
    assert!(m.conflicts[0].key.ends_with(":order"));
    assert_eq!(m.budget, ours);
}

#[test]
fn a_returned_shared_copy_merges_against_what_was_sent() {
    let mut original = sample_base();
    let id = first_rows(&original, 1)[0];
    find_row(&mut original, id).unwrap().notes = vec![Note::plain(1, "private")];
    save_version(&mut original, "Sent", "2026-10-09T10:00:00Z", None);
    let vid = original.versions[0].id;
    // The copy left notes out; the reviewer changed a rate.
    let opts = share::ShareOptions {
        drop_notes: true,
        ..Default::default()
    };
    let mut copy = share::share_copy(
        &original,
        Some(vid),
        &opts,
        "2026-10-09T11:00:00Z",
        "0.0.1",
        None,
    )
    .unwrap();
    let other = first_rows(&original, 2)[1];
    find_row(&mut copy, other).unwrap().rate = "1234".into();
    // Meanwhile the original moved on.
    find_row(&mut original, id).unwrap().description = "Ours now".into();
    let sent = original.version(vid).unwrap().budget.to_budget();
    let base = base_as_shared(&sent, &original, &copy);
    let m = merge(&base, &original, &copy);
    assert!(m.conflicts.is_empty(), "{:?}", m.conflicts);
    let b = &m.budget;
    let mut b = b.clone();
    assert_eq!(
        find_row(&mut b, id).unwrap().notes.len(),
        1,
        "our notes stay"
    );
    assert_eq!(find_row(&mut b, id).unwrap().description, "Ours now");
    assert_eq!(find_row(&mut b, other).unwrap().rate.0, "1234");
    assert_eq!(b.versions.len(), 1, "our version stays");
    assert_eq!(b.provenance, original.provenance);
    // Without working out what was sent, the notes would read as deleted.
    let naive = merge(&sent, &original, &copy);
    let mut nb = naive.budget.clone();
    assert!(find_row(&mut nb, id).unwrap().notes.is_empty() || !naive.conflicts.is_empty());
}

#[test]
fn diff_lists_changes_in_plain_words_grouped_by_account() {
    let base = sample_base();
    let ids = first_rows(&base, 2);
    let x = apply_all(
        &base,
        &[Edit::Rate(ids[0], "1350".into()), Edit::DeleteRow(ids[1])],
    );
    let d = diff(&base, &x);
    let row = d
        .items
        .iter()
        .find(|i| matches!(i.place, Place::Account { row: Some(r), .. } if r == ids[0]))
        .unwrap();
    assert_eq!(row.kind, ChangeKind::Changed);
    assert!(
        row.text.contains(": rate ") && row.text.contains("-> 1,350.00"),
        "{}",
        row.text
    );
    let gone = d
        .items
        .iter()
        .find(|i| matches!(i.place, Place::Account { row: Some(r), .. } if r == ids[1]))
        .unwrap();
    assert_eq!(gone.kind, ChangeKind::Removed);
    assert!(!d.totals.is_empty());
}
