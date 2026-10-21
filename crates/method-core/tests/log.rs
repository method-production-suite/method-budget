//! The change log: capture by diff, the fold, the projection and its
//! repairs, convergence across actors, and per-person undo
//! (docs/research/collaboration.md; decisions.md, entry 62).
//!
//! Property tests use a small seeded generator, as tests/properties.rs
//! does, so the engine stays dependency-free. Each failure prints its seed.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use common::*;
use method_core::log::{
    ActorId, Applied, Change, ChangeId, Field, Hint, Ids, Kind, Log, LogError, Op, Projection,
    Review, Role, Uid, Value,
};
use method_core::rename::SetupKind;
use method_core::*;

const T: &str = "2026-10-12T14:03:22Z";

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }
}

fn actor(n: u8) -> ActorId {
    let mut a = [0u8; 32];
    a[0] = n;
    a[31] = n;
    ActorId(a)
}

/// What the log never carries: the provenance (about the file) and
/// default global info the format drops anyway.
fn normalized(b: &Budget) -> Budget {
    let mut b = b.clone();
    b.provenance = None;
    b.global_info
        .retain(|name, info| b.globals.contains_key(name) && !info.is_default());
    b
}

fn adopt(n: u8, b: &Budget) -> Log {
    Log::adopt(actor(n), Ids::seeded(n as u64), b, "Adopted", T)
}

/// A second actor's log with the same history as `from`.
fn follower(n: u8, from: &Log) -> Log {
    let mut log = Log::new(actor(n), Ids::seeded(100 + n as u64));
    for c in from.changes() {
        log.apply(c.clone()).unwrap();
    }
    log
}

fn record(log: &mut Log, proj: &Projection, after: &Budget, hints: &[Hint], label: &str) {
    log.record(proj, after, hints, label, T)
        .unwrap_or_else(|e| panic!("{label}: {e}"));
}

fn rows(b: &Budget) -> Vec<RowId> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .map(|d| d.id)
        .collect()
}

fn row_mut(b: &mut Budget, id: RowId) -> Option<&mut Detail> {
    b.categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
        .find(|d| d.id == id)
}

fn account_codes(b: &Budget) -> Vec<String> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .map(|a| a.code.clone())
        .collect()
}

fn account_mut<'a>(b: &'a mut Budget, code: &str) -> Option<&'a mut Account> {
    b.categories_mut()
        .flat_map(|c| &mut c.accounts)
        .find(|a| a.code == code)
}

/// Takes row `id` out of wherever it is.
fn take_row(b: &mut Budget, id: RowId) -> Option<Detail> {
    for a in b.categories_mut().flat_map(|c| &mut c.accounts) {
        if let Some(i) = a.details.iter().position(|d| d.id == id) {
            return Some(a.details.remove(i));
        }
    }
    None
}

/// One random edit, as an editor would make it: the budget stays
/// consistent (no dangling names). Returns the label and the hints the
/// command would pass.
fn random_edit(rng: &mut Rng, b: &mut Budget, who: u8) -> (String, Vec<Hint>) {
    let ids = rows(b);
    let codes = account_codes(b);
    let fringes: Vec<String> = b.fringes.keys().cloned().collect();
    let globals: Vec<String> = b.globals.keys().cloned().collect();
    let next_id = b.highest_row_id().map_or(1, |m| m + 1);
    match rng.below(22) {
        0..=3 if !ids.is_empty() => {
            let id = *rng.pick(&ids);
            let d = row_mut(b, id).unwrap();
            match rng.below(4) {
                0 => d.rate = (rng.below(5000) as i64).into(),
                1 => {
                    d.amount = rng
                        .pick(&["3", "10", "DAYS", "WEEKS * 2", "1.5"])
                        .to_string()
                        .into()
                }
                2 => d.description = format!("edit {who}-{}", rng.below(100)),
                _ => d.bold = !d.bold,
            }
            (format!("Edit line {id}"), vec![])
        }
        4..=5 if !codes.is_empty() => {
            let code = rng.pick(&codes).clone();
            let a = account_mut(b, &code).unwrap();
            let at = rng.below(a.details.len() as u64 + 1) as usize;
            let mut d = Detail::line(next_id, format!("new {who}"), 2, "Day", 100);
            if let Some(f) = fringes.first() {
                d.fringes.insert(f.clone());
            }
            a.details.insert(at, d);
            (format!("Add line {next_id} to {code}"), vec![])
        }
        6 if !ids.is_empty() => {
            let id = *rng.pick(&ids);
            take_row(b, id);
            (format!("Delete line {id}"), vec![])
        }
        7..=8 if !ids.is_empty() && !codes.is_empty() => {
            let id = *rng.pick(&ids);
            let code = rng.pick(&codes).clone();
            let d = take_row(b, id).unwrap();
            let a = account_mut(b, &code).unwrap();
            let at = rng.below(a.details.len() as u64 + 1) as usize;
            a.details.insert(at, d);
            (format!("Move line {id} to {code}"), vec![])
        }
        9 if !ids.is_empty() && !fringes.is_empty() => {
            let id = *rng.pick(&ids);
            let f = rng.pick(&fringes).clone();
            let d = row_mut(b, id).unwrap();
            if !d.fringes.remove(&f) {
                d.fringes.insert(f);
            }
            (format!("Toggle fringe on {id}"), vec![])
        }
        10 => {
            let name = format!("F{who}{}", rng.below(5));
            b.fringes.entry(name.clone()).or_insert(Fringe {
                description: String::new(),
                short_code: None,
                rate: (rng.below(30) as i64).into(),
                kind: FringeKind::Percent { cutoff: None },
            });
            (format!("Add fringe {name}"), vec![])
        }
        11 if !fringes.is_empty() => {
            let f = rng.pick(&fringes).clone();
            b.fringes.remove(&f);
            for d in b
                .categories_mut()
                .flat_map(|c| &mut c.accounts)
                .flat_map(|a| &mut a.details)
            {
                d.fringes.remove(&f);
            }
            for c in &mut b.credits {
                c.excluded_fringes.remove(&f);
            }
            (format!("Delete fringe {f}"), vec![])
        }
        12 if !fringes.is_empty() => {
            let from = rng.pick(&fringes).clone();
            let to = format!("{from}_{who}");
            match rename(b, SetupKind::Fringe, &from, &to) {
                Ok(_) => (
                    format!("Rename fringe {from} to {to}"),
                    vec![Hint::rename(SetupKind::Fringe, &from, &to)],
                ),
                Err(_) => ("Nothing".into(), vec![]),
            }
        }
        13 if !globals.is_empty() => {
            let from = rng.pick(&globals).clone();
            let to = format!("{from}_{who}");
            match rename(b, SetupKind::Global, &from, &to) {
                Ok(_) => (
                    format!("Rename global {from} to {to}"),
                    vec![Hint::rename(SetupKind::Global, &from, &to)],
                ),
                Err(_) => ("Nothing".into(), vec![]),
            }
        }
        14 => {
            let name = format!("G{who}");
            b.globals
                .insert(name.clone(), (rng.below(40) as i64 + 1).into());
            (format!("Set global {name}"), vec![])
        }
        15 if !codes.is_empty() => {
            let code = rng.pick(&codes).clone();
            let new = format!("{}{}", &code[..code.len().min(2)], 50 + rng.below(49));
            if codes.contains(&new) {
                return ("Nothing".into(), vec![]);
            }
            account_mut(b, &code).unwrap().code = new.clone();
            // As the renumber command does: exclusions follow the number.
            for ch in &mut b.charges {
                if ch.excluded_accounts.remove(&code) {
                    ch.excluded_accounts.insert(new.clone());
                }
            }
            (
                format!("Renumber {code} to {new}"),
                vec![Hint::Recode {
                    kind: Kind::Account,
                    from: code,
                    to: new,
                }],
            )
        }
        16 => {
            let cats: Vec<usize> = b
                .topsheet
                .iter()
                .enumerate()
                .filter(|(_, r)| matches!(r, TopsheetRow::Category(_)))
                .map(|(i, _)| i)
                .collect();
            let i = *rng.pick(&cats);
            let code = format!("{}{}", 60 + who, rng.below(10));
            if codes.contains(&code) {
                return ("Nothing".into(), vec![]);
            }
            if let TopsheetRow::Category(c) = &mut b.topsheet[i] {
                c.accounts.push(account(&code, "New account", vec![]));
            }
            (format!("Add account {code}"), vec![])
        }
        17 if codes.len() > 1 => {
            let code = rng.pick(&codes).clone();
            for c in b.categories_mut() {
                c.accounts.retain(|a| a.code != code);
            }
            for ch in &mut b.charges {
                ch.excluded_accounts.remove(&code);
            }
            (format!("Delete account {code}"), vec![])
        }
        18 if b.topsheet.len() > 1 => {
            let i = rng.below(b.topsheet.len() as u64) as usize;
            let row = b.topsheet.remove(i);
            let j = rng.below(b.topsheet.len() as u64 + 1) as usize;
            b.topsheet.insert(j, row);
            ("Move topsheet row".into(), vec![])
        }
        19 if !ids.is_empty() => {
            let id = *rng.pick(&ids);
            let d = row_mut(b, id).unwrap();
            let nid = next_note_id(&d.notes);
            d.notes.push(Note {
                id: nid,
                text: format!("note by {who}"),
                created: T.into(),
                edited: None,
                author: Some(format!("actor {who}")),
                done: false,
            });
            (format!("Note on {id}"), vec![])
        }
        20 => {
            b.title = format!("Title by {who} {}", rng.below(10));
            ("Rename budget".into(), vec![])
        }
        21 => {
            if b.versions.len() > 2 && rng.chance(50) {
                let id = b.versions[0].id;
                delete_version(b, id).unwrap();
                ("Delete version".into(), vec![])
            } else {
                save_version(b, format!("v{who}"), T, None);
                ("Save version".into(), vec![])
            }
        }
        _ => ("Nothing".into(), vec![]),
    }
}

// ---------------------------------------------------------------------------
// Adoption and replay

#[test]
fn adopting_a_budget_projects_it_back() {
    let b = sample();
    let log = adopt(1, &b);
    assert_eq!(log.len(), 1);
    let p = log.project();
    assert_eq!(normalized(&p.budget), normalized(&b));
    assert!(p.review.is_empty(), "{:?}", p.review);
    // The genesis creates every entity and writes every setting.
    let genesis = log.changes().next().unwrap();
    assert!(genesis.ops.iter().any(
        |op| matches!(op, Op::Set { entity, field: Field::Title, .. } if *entity == Uid::ROOT)
    ));
    assert!(
        genesis
            .ops
            .iter()
            .filter(|op| matches!(op, Op::Create { .. }))
            .count()
            > 20
    );
}

/// An account credit row, a currency's format and a frozen baseline
/// (decisions.md, entry 61) project back, adopted and recorded later.
#[test]
fn account_credit_rows_currency_formats_and_frozen_baselines_project_back() {
    let mut b = sample();
    let code = account_codes(&b)[0].clone();
    let cur = b.currencies.keys().next().cloned().unwrap();
    b.currencies.get_mut(&cur).unwrap().format = CurrencyFormat {
        symbol_after: true,
        separators: Separators::PointComma,
    };
    b.credits.push(Credit {
        name: "Account credit".into(),
        account: None,
        applied: true,
        cap: None,
        discount_percent: Dec::ZERO,
        excluded_fringes: BTreeSet::new(),
        locations: BTreeSet::new(),
        minimum_spend: None,
        broker_fee: 0,
        shared_cap: None,
        rows: vec![CreditRow {
            description: "Crew".into(),
            source: CreditSource::Account {
                account: code.clone(),
            },
            percent: "10".parse().unwrap(),
            cap: CreditCap::None,
        }],
    });
    let mut log = adopt(1, &b);
    let p = log.project();
    assert_eq!(normalized(&p.budget), normalized(&b));

    // Later: the format back to the default, the baseline frozen.
    let mut after = p.budget.clone();
    after.currencies.get_mut(&cur).unwrap().format = CurrencyFormat::default();
    let r = recalc(&after, &RecalcOptions::default()).unwrap();
    let mut base = Baseline::capture(&r).unwrap();
    base.frozen = true;
    after.baseline = Some(base);
    record(&mut log, &p, &after, &[], "Edit");
    let p = log.project();
    assert_eq!(normalized(&p.budget), normalized(&after));
    assert_eq!(
        p.budget.credits.last().unwrap().rows[0].source,
        CreditSource::Account { account: code }
    );
}

#[test]
fn an_unchanged_budget_records_nothing() {
    let mut log = adopt(1, &sample());
    let p = log.project();
    let same = p.budget.clone();
    assert_eq!(log.record(&p, &same, &[], "Nothing", T).unwrap(), None);
    assert_eq!(log.len(), 1);
}

#[test]
fn replay_gives_exactly_the_edited_budget() {
    for seed in 1..=60u64 {
        let mut rng = Rng::new(seed);
        let mut log = adopt(1, &sample());
        for step in 0..25 {
            let p = log.project();
            let mut after = p.budget.clone();
            let (label, hints) = random_edit(&mut rng, &mut after, 1);
            record(&mut log, &p, &after, &hints, &label);
            let again = log.project();
            assert_eq!(
                normalized(&again.budget),
                normalized(&after),
                "seed {seed} step {step}: {label}"
            );
            assert!(
                again.review.is_empty(),
                "seed {seed} step {step}: {label}: {:?}",
                again.review
            );
        }
    }
}

#[test]
fn a_rename_with_its_hint_is_one_write() {
    let mut log = adopt(1, &sample());
    let p = log.project();
    let mut after = p.budget.clone();
    rename(&mut after, SetupKind::Fringe, "FICA", "SS").unwrap();
    record(
        &mut log,
        &p,
        &after,
        &[Hint::rename(SetupKind::Fringe, "FICA", "SS")],
        "Rename FICA",
    );
    let change = log.changes().last().unwrap();
    assert_eq!(change.ops.len(), 1, "{:?}", change.ops);
    assert!(matches!(
        &change.ops[0],
        Op::Set { field: Field::Name, value: Value::Text(n), .. } if n == "SS"
    ));
    assert_eq!(normalized(&log.project().budget), normalized(&after));

    // A global rename rewrites formulas, but the log stores them bound.
    let p = log.project();
    let mut after = p.budget.clone();
    rename(&mut after, SetupKind::Global, "SHOOT_DAYS", "DAYS").unwrap();
    record(
        &mut log,
        &p,
        &after,
        &[Hint::rename(SetupKind::Global, "SHOOT_DAYS", "DAYS")],
        "Rename global",
    );
    assert_eq!(log.changes().last().unwrap().ops.len(), 1);
    let b = log.project().budget;
    assert_eq!(b.globals["PREP_DAYS"].0, "DAYS / 3");
    let grip = b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == 2)
        .unwrap();
    assert_eq!(grip.amount.0, "DAYS + PREP_DAYS");
}

#[test]
fn moves_are_one_write_per_moved_row() {
    let mut log = adopt(1, &sample());
    let p = log.project();
    let mut after = p.budget.clone();
    // Reverse the crew account's rows: the LCS keeps one, the rest move.
    let crew = account_mut(&mut after, "2100").unwrap();
    crew.details.reverse();
    record(&mut log, &p, &after, &[], "Reverse crew");
    let change = log.changes().last().unwrap();
    assert!(change.ops.iter().all(|op| matches!(op, Op::Move { .. })));
    assert_eq!(change.ops.len(), 5);
    assert_eq!(normalized(&log.project().budget), normalized(&after));
}

// ---------------------------------------------------------------------------
// Convergence

/// Delivers every change each actor has that the others lack, in a
/// random order per receiver (the pending buffer sorts out dependencies).
fn exchange(rng: &mut Rng, logs: &mut [Log]) {
    let all: Vec<Change> = logs
        .iter()
        .flat_map(|l| l.changes().cloned().collect::<Vec<_>>())
        .collect();
    for log in logs.iter_mut() {
        let mut missing: Vec<Change> = all
            .iter()
            .filter(|c| !log.frontier().includes(c.id()))
            .cloned()
            .collect();
        missing.sort_by_key(|c| (c.actor, c.seq));
        missing.dedup_by_key(|c| c.id());
        rng.shuffle(&mut missing);
        for c in missing {
            log.apply(c).unwrap();
        }
        assert!(log.pending().is_empty());
        assert!(log.rejected().is_empty(), "{:?}", log.rejected());
    }
}

#[test]
fn concurrent_edits_converge_in_any_order() {
    for seed in 1..=40u64 {
        let mut rng = Rng::new(seed);
        let actors = 2 + rng.below(3) as u8;
        let first = adopt(1, &sample());
        let mut logs: Vec<Log> = (2..=actors).map(|n| follower(n, &first)).collect();
        logs.insert(0, first);
        for round in 0..4 {
            for (i, log) in logs.iter_mut().enumerate() {
                let edits = 1 + rng.below(4);
                for _ in 0..edits {
                    let p = log.project();
                    let mut after = p.budget.clone();
                    let (label, hints) = random_edit(&mut rng, &mut after, i as u8 + 1);
                    record(log, &p, &after, &hints, &label);
                }
            }
            exchange(&mut rng, &mut logs);
            let projections: Vec<Projection> = logs.iter().map(Log::project).collect();
            for (i, p) in projections.iter().enumerate().skip(1) {
                assert_eq!(
                    logs[0].frontier(),
                    logs[i].frontier(),
                    "seed {seed} round {round}"
                );
                assert_eq!(
                    normalized(&projections[0].budget),
                    normalized(&p.budget),
                    "seed {seed} round {round}: actor {} differs",
                    i + 1
                );
                assert_eq!(projections[0].review, p.review, "seed {seed} round {round}");
            }
            // A newcomer replaying everything in a shuffled order agrees too.
            let mut fresh = Log::new(actor(9), Ids::seeded(seed));
            let mut all: Vec<Change> = logs[0].changes().cloned().collect();
            rng.shuffle(&mut all);
            for c in all {
                fresh.apply(c).unwrap();
            }
            assert!(fresh.pending().is_empty(), "seed {seed}");
            assert_eq!(
                normalized(&fresh.project().budget),
                normalized(&projections[0].budget),
                "seed {seed} round {round}: newcomer"
            );
            // And the budget recalculates.
            let _ = recalc(&projections[0].budget, &RecalcOptions::default());
        }
    }
}

#[test]
fn out_of_order_changes_wait_for_their_dependencies() {
    let mut a = adopt(1, &sample());
    for i in 0..3 {
        let p = a.project();
        let mut after = p.budget.clone();
        after.title = format!("t{i}");
        record(&mut a, &p, &after, &[], "Title");
    }
    let changes: Vec<Change> = a.changes().cloned().collect();
    let mut b = Log::new(actor(2), Ids::seeded(2));
    assert_eq!(b.apply(changes[3].clone()).unwrap(), Applied::Pending);
    assert_eq!(b.apply(changes[2].clone()).unwrap(), Applied::Pending);
    assert_eq!(b.pending().len(), 2);
    assert_eq!(
        b.apply(changes[0].clone()).unwrap(),
        Applied::Applied { unblocked: 0 }
    );
    assert_eq!(
        b.apply(changes[1].clone()).unwrap(),
        Applied::Applied { unblocked: 2 }
    );
    assert!(b.pending().is_empty());
    assert_eq!(b.apply(changes[1].clone()).unwrap(), Applied::Duplicate);
    assert_eq!(b.project().budget.title, "t2");
    // The same seq with other content is a fork.
    let mut forged = changes[1].clone();
    forged.label = "forged".into();
    assert_eq!(
        b.apply(forged).unwrap_err(),
        LogError::Fork {
            id: changes[1].id()
        }
    );
}

#[test]
fn a_malformed_change_is_refused_and_listed() {
    let mut a = adopt(1, &sample());
    let bogus = Change {
        actor: actor(2),
        seq: 1,
        deps: a.frontier().clone(),
        lamport: a.lamport() + 1,
        time: T.into(),
        label: "bogus".into(),
        ops: vec![Op::Set {
            entity: Uid(12345),
            field: Field::Name,
            value: Value::text("x"),
        }],
        sig: None,
    };
    assert!(matches!(
        a.apply(bogus).unwrap_err(),
        LogError::Malformed { .. }
    ));
    assert_eq!(a.rejected().len(), 1);
    // The actor's next change still applies.
    let mut b = follower(2, &a);
    let p = b.project();
    let mut after = p.budget.clone();
    after.title = "fine".into();
    record(&mut b, &p, &after, &[], "Title");
    let next = b.changes().last().unwrap().clone();
    assert_eq!(next.seq, 1);
    // The same number from actor 2, a change that fits: it takes the refused
    // one's place, which is listed as such (MReview, pull request #73).
    assert_eq!(a.apply(next).unwrap(), Applied::Applied { unblocked: 0 });
    assert!(a.rejected().is_empty());
    assert_eq!(a.forks().len(), 1);
    assert_eq!(a.project().budget.title, "fine");
}

// ---------------------------------------------------------------------------
// Conflicts and repairs (docs/research/collaboration.md, 3.8 and 3.10)

/// Two actors from one budget, each editing it apart, then merged.
fn apart(
    edit_a: impl FnOnce(&mut Budget) -> Vec<Hint>,
    edit_b: impl FnOnce(&mut Budget) -> Vec<Hint>,
) -> (Log, Log) {
    let mut a = adopt(1, &sample());
    let mut b = follower(2, &a);
    let p = a.project();
    let mut after = p.budget.clone();
    let hints = edit_a(&mut after);
    record(&mut a, &p, &after, &hints, "A's edit");
    let p = b.project();
    let mut after = p.budget.clone();
    let hints = edit_b(&mut after);
    record(&mut b, &p, &after, &hints, "B's edit");
    let mut rng = Rng::new(1);
    let mut logs = vec![a, b];
    exchange(&mut rng, &mut logs);
    let b = logs.pop().unwrap();
    let a = logs.pop().unwrap();
    assert_eq!(
        normalized(&a.project().budget),
        normalized(&b.project().budget)
    );
    (a, b)
}

fn review_keys(p: &Projection) -> Vec<&'static str> {
    p.review.iter().map(|r| r.review.key()).collect()
}

#[test]
fn same_field_later_write_wins_and_is_flagged() {
    let (a, _) = apart(
        |b| {
            row_mut(b, 2).unwrap().rate = 500.into();
            vec![]
        },
        |b| {
            row_mut(b, 2).unwrap().rate = 600.into();
            vec![]
        },
    );
    let p = a.project();
    let grip = p
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == 2)
        .unwrap();
    // Same lamport: the higher actor id wins.
    assert_eq!(grip.rate.0, "600");
    assert_eq!(review_keys(&p), vec!["overwritten"]);
    assert!(
        p.review[0].sentence.contains("rate"),
        "{}",
        p.review[0].sentence
    );
    let history = a.blame(p.binding.rows[&2], Field::Rate);
    assert_eq!(history.len(), 3);
    assert_eq!(history[2].2, Value::Formula("600".into()));
}

#[test]
fn different_fields_of_one_row_both_keep() {
    let (a, _) = apart(
        |b| {
            row_mut(b, 2).unwrap().rate = 500.into();
            vec![]
        },
        |b| {
            row_mut(b, 2).unwrap().description = "Key grip (B)".into();
            vec![]
        },
    );
    let p = a.project();
    let grip = p
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == 2)
        .unwrap();
    assert_eq!(
        (grip.rate.0.as_str(), grip.description.as_str()),
        ("500", "Key grip (B)")
    );
    assert!(p.review.is_empty());
}

#[test]
fn tags_added_apart_are_both_kept_and_adds_win_over_removes() {
    let (a, _) = apart(
        |b| {
            row_mut(b, 1).unwrap().fringes.insert("PH".into());
            vec![]
        },
        |b| {
            let d = row_mut(b, 1).unwrap();
            d.fringes.remove("FICA");
            d.fringes.insert("PH".into());
            vec![]
        },
    );
    let p = a.project();
    let writer = p
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == 1)
        .unwrap();
    assert_eq!(writer.fringes, set(&["PH"]));
    assert!(p.review.is_empty());
    // A removes a tag while B removes and re-adds it (two changes): B's
    // add is one A never saw, so it wins.
    let mut a = adopt(1, &sample());
    let mut b = follower(2, &a);
    let p = a.project();
    let mut after = p.budget.clone();
    row_mut(&mut after, 2).unwrap().fringes.remove("PH");
    record(&mut a, &p, &after, &[], "A removes PH");
    for step in 0..2 {
        let p = b.project();
        let mut after = p.budget.clone();
        let d = row_mut(&mut after, 2).unwrap();
        if step == 0 {
            d.fringes.remove("PH");
        } else {
            d.fringes.insert("PH".into());
        }
        record(&mut b, &p, &after, &[], "B toggles PH");
    }
    let mut rng = Rng::new(2);
    let mut logs = vec![a, b];
    exchange(&mut rng, &mut logs);
    for log in &logs {
        let grip = log
            .project()
            .budget
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|d| d.id == 2)
            .unwrap()
            .clone();
        assert!(grip.fringes.contains("PH"));
    }
}

#[test]
fn delete_wins_over_an_edit_unless_still_used() {
    // A deletes a line, B edits it: gone, flagged.
    let (a, _) = apart(
        |b| {
            take_row(b, 5);
            vec![]
        },
        |b| {
            row_mut(b, 5).unwrap().rate = 1.into();
            vec![]
        },
    );
    let p = a.project();
    assert!(!rows(&p.budget).contains(&5));
    assert_eq!(review_keys(&p), vec!["deleted_while_edited"]);

    // A deletes fringe PH, B applies it to a line: PH comes back.
    let (a, _) = apart(
        |b| {
            b.fringes.remove("PH");
            for d in b
                .categories_mut()
                .flat_map(|c| &mut c.accounts)
                .flat_map(|a| &mut a.details)
            {
                d.fringes.remove("PH");
            }
            vec![]
        },
        |b| {
            row_mut(b, 8).unwrap().fringes.insert("PH".into());
            vec![]
        },
    );
    let p = a.project();
    assert!(p.budget.fringes.contains_key("PH"));
    let editor = p
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == 8)
        .unwrap();
    assert!(editor.fringes.contains("PH"));
    assert!(review_keys(&p).contains(&"revived"), "{:?}", p.review);
    assert!(
        p.review.iter().any(|r| r.sentence.contains("fringe PH")),
        "{:?}",
        p.review
    );

    // A deletes account 5100, B adds a line to it: the account comes back
    // with B's line only.
    let (a, _) = apart(
        |b| {
            for c in b.categories_mut() {
                c.accounts.retain(|a| a.code != "5100");
            }
            vec![]
        },
        |b| {
            account_mut(b, "5100").unwrap().details.push(Detail::line(
                100,
                "Assistant editor",
                10,
                "Week",
                1500,
            ));
            vec![]
        },
    );
    let p = a.project();
    let editorial = p
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .find(|a| a.code == "5100")
        .unwrap();
    assert_eq!(
        editorial.details.iter().map(|d| d.id).collect::<Vec<_>>(),
        vec![100]
    );
    assert!(review_keys(&p).contains(&"revived"));
}

#[test]
fn the_same_item_created_twice() {
    // Identical: one item.
    let (a, _) = apart(
        |b| {
            b.groups.insert("Night".into(), group(true));
            vec![]
        },
        |b| {
            b.groups.insert("Night".into(), group(true));
            vec![]
        },
    );
    let p = a.project();
    assert!(p.budget.groups.contains_key("Night"));
    assert!(p.review.is_empty(), "{:?}", p.review);
    // Different: both kept, the later as "Night 2", flagged.
    let (a, _) = apart(
        |b| {
            b.groups.insert("Night".into(), group(true));
            vec![]
        },
        |b| {
            b.groups.insert("Night".into(), group(false));
            row_mut(b, 1).unwrap().groups.insert("Night".into());
            vec![]
        },
    );
    let p = a.project();
    assert!(p.budget.groups.contains_key("Night") && p.budget.groups.contains_key("Night 2"));
    assert_eq!(review_keys(&p), vec!["duplicate_name"]);
    // B's line follows B's group under its new name.
    let writer = p
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == 1)
        .unwrap();
    assert_eq!(writer.groups, set(&["Night 2"]));
    // Editing "Night 2" afterwards edits that group, not the other.
    let mut a = a;
    let mut after = p.budget.clone();
    after.groups.get_mut("Night 2").unwrap().description = "B's".into();
    record(&mut a, &p, &after, &[], "Describe");
    let again = a.project();
    assert_eq!(again.budget.groups["Night 2"].description, "B's");
    assert_eq!(again.budget.groups["Night"].description, "");
}

#[test]
fn colliding_row_ids_are_repaired_and_flagged() {
    let (a, _) = apart(
        |b| {
            let id = b.highest_row_id().unwrap() + 1;
            account_mut(b, "1100")
                .unwrap()
                .details
                .push(Detail::line(id, "A's line", 1, "Flat", 10));
            vec![]
        },
        |b| {
            let id = b.highest_row_id().unwrap() + 1;
            account_mut(b, "2100")
                .unwrap()
                .details
                .push(Detail::line(id, "B's line", 1, "Flat", 20));
            vec![]
        },
    );
    let p = a.project();
    let ids = rows(&p.budget);
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len());
    assert_eq!(review_keys(&p), vec!["duplicate_row_id"]);
    assert!(ids.iter().any(|id| *id >= 1 << 32));
    // Editing the repaired line works through its shown id.
    let shown = *ids.iter().find(|id| **id >= 1 << 32).unwrap();
    let mut a = a;
    let mut after = p.budget.clone();
    row_mut(&mut after, shown).unwrap().rate = 99.into();
    record(&mut a, &p, &after, &[], "Edit repaired line");
    assert_eq!(normalized(&a.project().budget), normalized(&after));
}

#[test]
fn concurrent_inserts_do_not_interleave_and_moves_settle() {
    let (a, _) = apart(
        |b| {
            let acct = account_mut(b, "2100").unwrap();
            for i in 0..3 {
                acct.details.insert(
                    1 + i,
                    Detail::line(200 + i as u64, format!("a{i}"), 1, "Flat", 1),
                );
            }
            vec![]
        },
        |b| {
            let acct = account_mut(b, "2100").unwrap();
            for i in 0..3 {
                acct.details.insert(
                    1 + i,
                    Detail::line(300 + i as u64, format!("b{i}"), 1, "Flat", 1),
                );
            }
            vec![]
        },
    );
    let p = a.project();
    let crew = p
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .find(|a| a.code == "2100")
        .unwrap();
    let descs: Vec<&str> = crew
        .details
        .iter()
        .map(|d| d.description.as_str())
        .collect();
    let joined = descs.join(",");
    assert!(
        joined.contains("a0,a1,a2") && joined.contains("b0,b1,b2"),
        "{joined}"
    );
    assert_eq!(crew.details.len(), 12);
    assert!(p.review.is_empty());

    // Both move one row: it ends in one place, flagged.
    let (a, _) = apart(
        |b| {
            let d = take_row(b, 2).unwrap();
            account_mut(b, "1100").unwrap().details.push(d);
            vec![]
        },
        |b| {
            let d = take_row(b, 2).unwrap();
            account_mut(b, "5100").unwrap().details.insert(0, d);
            vec![]
        },
    );
    let p = a.project();
    assert_eq!(rows(&p.budget).iter().filter(|id| **id == 2).count(), 1);
    assert_eq!(review_keys(&p), vec!["moved_twice"]);
}

#[test]
fn renumber_and_rename_merge_with_concurrent_edits() {
    // A renumbers an account while B edits a line in it and adds a charge
    // exclusion naming it: both follow the new number.
    let (a, _) = apart(
        |b| {
            account_mut(b, "2100").unwrap().code = "2150".into();
            vec![Hint::Recode {
                kind: Kind::Account,
                from: "2100".into(),
                to: "2150".into(),
            }]
        },
        |b| {
            row_mut(b, 2).unwrap().rate = 475.into();
            b.charges[0].excluded_accounts.insert("2100".into());
            vec![]
        },
    );
    let p = a.project();
    let crew = p
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .find(|a| a.code == "2150")
        .unwrap();
    assert_eq!(crew.details[0].rate.0, "475");
    assert_eq!(p.budget.charges[0].excluded_accounts, set(&["2150"]));
    assert!(p.review.is_empty(), "{:?}", p.review);

    // A renames a global while B types a formula that uses it.
    let (a, _) = apart(
        |b| {
            rename(b, SetupKind::Global, "SHOOT_DAYS", "DAYS").unwrap();
            vec![Hint::rename(SetupKind::Global, "SHOOT_DAYS", "DAYS")]
        },
        |b| {
            row_mut(b, 1).unwrap().amount = "SHOOT_DAYS * 2".into();
            vec![]
        },
    );
    let p = a.project();
    let writer = p
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == 1)
        .unwrap();
    assert_eq!(writer.amount.0, "DAYS * 2");
    assert!(p.review.is_empty(), "{:?}", p.review);

    // Both set globals into a cycle: kept as written, flagged.
    let (a, _) = apart(
        |b| {
            b.globals
                .insert("SHOOT_DAYS".into(), "PREP_DAYS * 3".into());
            vec![]
        },
        |b| {
            b.globals
                .insert("PREP_DAYS".into(), "SHOOT_DAYS / 3".into());
            vec![]
        },
    );
    let p = a.project();
    assert!(matches!(&p.review[0].review, Review::CircularGlobals { names } if names.len() == 2));
}

// ---------------------------------------------------------------------------
// Undo

#[test]
fn undo_puts_back_my_values_and_skips_what_others_changed_since() {
    let mut a = adopt(1, &sample());
    let mut b = follower(2, &a);
    // A changes a rate and a description in one change.
    let p = a.project();
    let mut after = p.budget.clone();
    {
        let d = row_mut(&mut after, 2).unwrap();
        d.rate = 500.into();
        d.description = "Key grip (A)".into();
    }
    record(&mut a, &p, &after, &[], "Edit grip");
    let mine = a.changes().last().unwrap().id();
    // B sees it and changes the description after.
    for c in a.since(b.frontier()) {
        b.apply(c.clone()).unwrap();
    }
    let p = b.project();
    let mut after = p.budget.clone();
    row_mut(&mut after, 2).unwrap().description = "Key grip (B)".into();
    record(&mut b, &p, &after, &[], "B's description");
    for c in b.since(a.frontier()) {
        a.apply(c.clone()).unwrap();
    }
    // A undoes: the rate goes back, the description stays B's.
    let (undo, inv) = a.undo(mine, T).unwrap();
    assert!(undo.is_some());
    assert_eq!(inv.ops.len(), 1);
    assert_eq!(inv.skipped.len(), 1);
    assert!(inv.skipped[0].contains("description"), "{}", inv.skipped[0]);
    let p = a.project();
    let grip = p
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == 2)
        .unwrap();
    assert_eq!(
        (grip.rate.0.as_str(), grip.description.as_str()),
        ("450", "Key grip (B)")
    );
    // B can't undo A's change.
    assert_eq!(b.undo(mine, T).unwrap_err(), LogError::NotMine { id: mine });
    // Redo is the undo of the undo.
    let (redo, _) = a.undo(undo.unwrap(), T).unwrap();
    assert!(redo.is_some());
    let grip = a
        .project()
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == 2)
        .unwrap()
        .clone();
    assert_eq!(grip.rate.0, "500");
}

#[test]
fn undo_of_adds_deletes_and_moves() {
    let mut a = adopt(1, &sample());
    // Add a line, then undo: it is gone.
    let p = a.project();
    let mut after = p.budget.clone();
    account_mut(&mut after, "1100")
        .unwrap()
        .details
        .push(Detail::line(50, "Temp", 1, "Flat", 1));
    record(&mut a, &p, &after, &[], "Add");
    let add = a.changes().last().unwrap().id();
    a.undo(add, T).unwrap();
    assert!(!rows(&a.project().budget).contains(&50));
    // Delete a line, then undo: it is back in its place.
    let p = a.project();
    let before = p.budget.clone();
    let mut after = p.budget.clone();
    take_row(&mut after, 3);
    record(&mut a, &p, &after, &[], "Delete");
    let del = a.changes().last().unwrap().id();
    a.undo(del, T).unwrap();
    assert_eq!(normalized(&a.project().budget), normalized(&before));
    // Move a line, then undo: back where it was.
    let p = a.project();
    let before = p.budget.clone();
    let mut after = p.budget.clone();
    let d = take_row(&mut after, 1).unwrap();
    account_mut(&mut after, "5100").unwrap().details.push(d);
    record(&mut a, &p, &after, &[], "Move");
    let mv = a.changes().last().unwrap().id();
    assert!(matches!(
        a.changes().last().unwrap().ops[0],
        Op::Move { .. }
    ));
    a.undo(mv, T).unwrap();
    assert_eq!(normalized(&a.project().budget), normalized(&before));
    // Undo as a property: undoing every change of a run, newest first,
    // gives the adopted budget back.
    let mut rng = Rng::new(7);
    let mut log = adopt(3, &sample());
    let start = log.project().budget;
    let mut made = Vec::new();
    for _ in 0..12 {
        let p = log.project();
        let mut after = p.budget.clone();
        let (label, hints) = random_edit(&mut rng, &mut after, 3);
        if let Some(id) = log.record(&p, &after, &hints, &label, T).unwrap() {
            made.push(id);
        }
    }
    for id in made.into_iter().rev() {
        let (_, inv) = log.undo(id, T).unwrap();
        assert!(inv.skipped.is_empty(), "{:?}", inv.skipped);
    }
    assert_eq!(normalized(&log.project().budget), normalized(&start));
}

// ---------------------------------------------------------------------------
// Everything the budget holds goes through the log

#[test]
fn tools_versions_actuals_and_settings_round_trip() {
    let mut b = sample();
    b.display.summary_rows = false;
    b.fringe_posting = FringePosting::Budget;
    b.cutoff_factor = CutoffFactor::OneAndAHalf;
    b.info
        .insert("Director".into(), InfoValue::Text("Someone".into()));
    b.locations.insert(
        "LA".into(),
        Location {
            description: "Los Angeles".into(),
            ..Location::default()
        },
    );
    b.sets.insert("Cutting room".into(), SetDef::default());
    save_version(&mut b, "v1", T, Some("first".into()));
    b.tag_presets.push(TagPreset {
        name: "Crew".into(),
        fringes: set(&["FICA"]),
        ..TagPreset::default()
    });
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    b.baseline = Baseline::capture(&r);
    actuals::add_batch(
        &mut b,
        "ledger.csv",
        T,
        vec![actuals::Transaction {
            id: 0,
            date: "2026-10-01".into(),
            account: "2100".into(),
            line: Some(2),
            tags: set(&["PO"]),
            vendor: "Grips Inc".into(),
            description: "Week 1".into(),
            reference: "1001".into(),
            amount: 123_456,
            batch: None,
        }],
    )
    .unwrap();
    actuals::set_etc(&mut b, "2100", Some(actuals::EtcMethod::Manual(50_000))).unwrap();
    let mut log = adopt(1, &b);
    let p = log.project();
    assert_eq!(normalized(&p.budget), normalized(&b));
    assert_eq!(p.budget.versions[0].name, "v1");
    // Renumbering the account the transaction and ETC point at follows.
    let mut after = p.budget.clone();
    account_mut(&mut after, "2100").unwrap().code = "2200".into();
    let mut t = (*after.actuals.transactions[0]).clone();
    t.account = "2200".into();
    let mut acts = (*after.actuals).clone();
    acts.transactions[0] = Arc::new(t);
    let etc = acts.etc.remove("2100").unwrap();
    acts.etc.insert("2200".into(), etc);
    after.actuals = Arc::new(acts);
    record(
        &mut log,
        &p,
        &after,
        &[Hint::Recode {
            kind: Kind::Account,
            from: "2100".into(),
            to: "2200".into(),
        }],
        "Renumber",
    );
    let change = log.changes().last().unwrap();
    assert_eq!(change.ops.len(), 1, "{:?}", change.ops);
    assert_eq!(normalized(&log.project().budget), normalized(&after));
}

#[test]
fn a_big_budget_adopts_edits_and_merges() {
    // About the size of a feature budget: 5,000 rows, 80 accounts.
    let big = sample::budget(&sample::SampleSpec::default());
    let start = std::time::Instant::now();
    let mut a = adopt(1, &big);
    let adopted = start.elapsed();
    let p = a.project();
    let projected = start.elapsed() - adopted;
    assert_eq!(normalized(&p.budget), normalized(&big));
    let mut after = p.budget.clone();
    let id = rows(&after)[2500];
    row_mut(&mut after, id).unwrap().rate = 777.into();
    let t = std::time::Instant::now();
    record(&mut a, &p, &after, &[], "One cell");
    let recorded = t.elapsed();
    assert_eq!(a.changes().last().unwrap().ops.len(), 1);
    let mut b = follower(2, &a);
    let pb = b.project();
    let mut after = pb.budget.clone();
    let id = rows(&after)[100];
    take_row(&mut after, id);
    record(&mut b, &pb, &after, &[], "Delete a line");
    let mut rng = Rng::new(3);
    let mut logs = vec![a, b];
    exchange(&mut rng, &mut logs);
    assert_eq!(
        normalized(&logs[0].project().budget),
        normalized(&logs[1].project().budget)
    );
    eprintln!("5,000 rows: adopt {adopted:?}, project {projected:?}, one-cell edit {recorded:?}");
}

// ---------------------------------------------------------------------------
// Roles: who may change what (docs/research/collaboration.md, section 7;
// decisions.md, entry 78)

fn category_uid(log: &Log, code: &str) -> Uid {
    log.project().binding.categories[code]
}

fn scope(log: &Log, codes: &[&str]) -> Role {
    Role::Department(codes.iter().map(|c| category_uid(log, c)).collect())
}

/// Records an edit made by `f` on the log's projection.
fn try_edit(log: &mut Log, f: impl FnOnce(&mut Budget)) -> Result<Option<ChangeId>, LogError> {
    let p = log.project();
    let mut after = p.budget.clone();
    f(&mut after);
    log.record(&p, &after, &[], "edit", T)
}

/// Hands `to` every change of `from` it hasn't seen; refusals are kept in
/// `to`'s rejected list.
fn send(from: &Log, to: &mut Log) {
    let missing: Vec<Change> = from.since(to.frontier()).into_iter().cloned().collect();
    for c in missing {
        let _ = to.apply(c);
    }
}

fn rejected_ids(log: &Log) -> BTreeSet<ChangeId> {
    log.rejected().iter().map(|(c, _)| c.id()).collect()
}

/// A change by `who` that claims to have seen everything in `seen`: what a
/// changed or old app could write, never this one.
fn forged(who: ActorId, seq: u64, seen: &Log, ops: Vec<Op>) -> Change {
    Change {
        actor: who,
        seq,
        deps: seen.frontier().clone(),
        lamport: seen.lamport() + 1,
        time: T.into(),
        label: "forged".into(),
        ops,
        sig: None,
    }
}

fn title_op(t: &str) -> Op {
    Op::Set {
        entity: Uid::ROOT,
        field: Field::Title,
        value: Value::text(t),
    }
}

#[test]
fn without_roles_everyone_edits_everything() {
    let a = adopt(1, &sample());
    let mut b = follower(2, &a);
    assert_eq!(a.owner(), Some(actor(1)));
    assert_eq!(a.role_of(actor(1)), Role::Owner);
    assert_eq!(a.role_of(actor(2)), Role::Editor);
    assert!(!a.roles_started());
    try_edit(&mut b, |b| b.title = "Mine".into()).unwrap();
    assert!(b.rejected().is_empty());
}

#[test]
fn a_department_editor_changes_only_lines_and_notes_in_scope() {
    let mut a = adopt(1, &sample());
    let role = scope(&a, &["2000"]);
    a.grant(actor(2), role.clone(), "Jordan edits 2000", T)
        .unwrap();
    assert!(a.roles_started());
    let mut b = follower(2, &a);
    assert_eq!(b.role_of(actor(2)), role);
    // Someone never given a role reads only, once roles have started.
    assert_eq!(a.role_of(actor(3)), Role::ReadOnly);

    // In 2000 Production: a rate, a new line, a note on the account, a
    // fringe applied, a line deleted.
    try_edit(&mut b, |b| row_mut(b, 2).unwrap().rate = 500.into())
        .unwrap()
        .unwrap();
    try_edit(&mut b, |b| {
        account_mut(b, "2100").unwrap().details.push(Detail::line(
            900,
            "Dolly grip",
            10,
            "Day",
            400,
        ));
    })
    .unwrap()
    .unwrap();
    try_edit(&mut b, |b| {
        account_mut(b, "2100").unwrap().notes.push(Note {
            id: 1,
            text: "Check the dolly rate".into(),
            created: T.into(),
            ..Note::default()
        });
    })
    .unwrap()
    .unwrap();
    try_edit(&mut b, |b| {
        row_mut(b, 5).unwrap().fringes.insert("FICA".into());
    })
    .unwrap()
    .unwrap();
    try_edit(&mut b, |b| {
        take_row(b, 7);
    })
    .unwrap()
    .unwrap();
    let mine = b.len();

    // Outside it: a line in 1000, the title, a fringe's rate, a new account.
    let refused = [
        try_edit(&mut b, |b| row_mut(b, 1).unwrap().rate = 1.into()),
        try_edit(&mut b, |b| b.title = "Mine".into()),
        try_edit(&mut b, |b| {
            b.fringes.get_mut("FICA").unwrap().rate = "8".into()
        }),
        try_edit(&mut b, |b| {
            b.categories_mut()
                .find(|c| c.code == "2000")
                .unwrap()
                .accounts
                .push(account("2200", "Camera", vec![]));
        }),
    ];
    for r in refused {
        assert!(matches!(r, Err(LogError::NotAllowed { .. })), "{r:?}");
    }
    // A refused edit leaves nothing behind.
    assert_eq!(b.len(), mine);
    assert!(b.rejected().is_empty());

    // Everyone else takes in what was allowed.
    send(&b, &mut a);
    assert!(a.rejected().is_empty(), "{:?}", a.rejected());
    assert_eq!(
        normalized(&a.project().budget),
        normalized(&b.project().budget)
    );
}

#[test]
fn changes_outside_a_role_are_refused_by_every_peer() {
    let mut a = adopt(1, &sample());
    a.grant(actor(2), Role::Comment, "Jordan comments", T)
        .unwrap();
    let mut c = follower(3, &a);
    let bad = forged(actor(2), 1, &a, vec![title_op("Forged")]);
    let id = bad.id();
    assert!(matches!(
        a.apply(bad.clone()),
        Err(LogError::NotAllowed { .. })
    ));
    assert!(matches!(c.apply(bad), Err(LogError::NotAllowed { .. })));
    assert_eq!(rejected_ids(&a), BTreeSet::from([id]));
    assert_eq!(rejected_ids(&c), BTreeSet::from([id]));
    assert_eq!(a.project().budget.title, "Sample Feature");
    // A note is within a commenter's role.
    let mut b = follower(2, &a);
    try_edit(&mut b, |b| {
        row_mut(b, 2).unwrap().notes.push(Note {
            id: 1,
            text: "Is this the weekly rate?".into(),
            created: T.into(),
            ..Note::default()
        });
    })
    .unwrap()
    .unwrap();
    assert!(matches!(
        try_edit(&mut b, |b| row_mut(b, 2).unwrap().rate = 1.into()),
        Err(LogError::NotAllowed { .. })
    ));
}

#[test]
fn a_change_made_before_seeing_a_role_is_judged_by_what_its_author_had_seen() {
    let mut a = adopt(1, &sample());
    let mut b = follower(2, &a);
    // Apart: the owner makes Jordan read only; Jordan, not knowing yet,
    // changes the title.
    a.grant(actor(2), Role::ReadOnly, "Jordan reads", T)
        .unwrap();
    try_edit(&mut b, |b| b.title = "Jordan's title".into())
        .unwrap()
        .unwrap();
    // In either order, everyone keeps Jordan's change.
    let mut c = follower(3, &a);
    let mut d = Log::new(actor(4), Ids::seeded(4));
    for ch in a.changes().take(1) {
        d.apply(ch.clone()).unwrap();
    }
    send(&b, &mut d);
    send(&a, &mut d);
    send(&b, &mut c);
    send(&b, &mut a);
    send(&a, &mut b);
    for log in [&a, &b, &c, &d] {
        assert!(log.rejected().is_empty(), "{:?}", log.rejected());
        assert_eq!(log.project().budget.title, "Jordan's title");
    }
    // Now Jordan has seen it: the next edit is refused at once.
    assert!(matches!(
        try_edit(&mut b, |b| b.title = "Again".into()),
        Err(LogError::NotAllowed { .. })
    ));
}

#[test]
fn scope_is_judged_where_the_line_was_when_it_was_edited() {
    let mut a = adopt(1, &sample());
    let role = scope(&a, &["2000"]);
    a.grant(actor(2), role, "Jordan edits 2000", T).unwrap();
    let mut b = follower(2, &a);
    let base = follower(3, &a);
    // Apart: the owner moves line 2 out to 1100; Jordan changes its rate.
    try_edit(&mut a, |b| {
        let d = take_row(b, 2).unwrap();
        account_mut(b, "1100").unwrap().details.push(d);
    })
    .unwrap()
    .unwrap();
    try_edit(&mut b, |b| row_mut(b, 2).unwrap().rate = 500.into())
        .unwrap()
        .unwrap();
    // Two peers, the changes in opposite orders: both keep Jordan's rate.
    let mut first_move = base.clone();
    send(&a, &mut first_move);
    send(&b, &mut first_move);
    let mut first_rate = base.clone();
    send(&b, &mut first_rate);
    send(&a, &mut first_rate);
    for log in [&first_move, &first_rate] {
        assert!(log.rejected().is_empty(), "{:?}", log.rejected());
    }
    assert_eq!(first_move.frontier(), first_rate.frontier());
    assert_eq!(
        normalized(&first_move.project().budget),
        normalized(&first_rate.project().budget)
    );
    // Once Jordan sees the move, line 2 is out of reach.
    send(&a, &mut b);
    assert!(matches!(
        try_edit(&mut b, |b| row_mut(b, 2).unwrap().rate = 600.into()),
        Err(LogError::NotAllowed { .. })
    ));
}

#[test]
fn only_the_owner_and_admins_change_roles() {
    let mut a = adopt(1, &sample());
    a.grant(actor(2), Role::Admin, "Priya is an admin", T)
        .unwrap();
    a.grant(actor(3), Role::Editor, "Jordan edits", T).unwrap();
    let mut admin = follower(2, &a);
    let mut editor = follower(3, &a);
    // An admin sets other people's roles, but makes no admins and leaves
    // the owner alone.
    admin
        .grant(actor(4), Role::Actuals, "Riley does actuals", T)
        .unwrap();
    for (who, role) in [
        (actor(4), Role::Admin),
        (actor(1), Role::ReadOnly),
        (actor(2), Role::Editor),
    ] {
        assert!(matches!(
            admin.grant(who, role, "no", T),
            Err(LogError::NotAllowed { .. })
        ));
    }
    // An editor sets nobody's role.
    assert!(matches!(
        editor.grant(actor(4), Role::Comment, "no", T),
        Err(LogError::NotAllowed { .. })
    ));
    // Nobody hands out ownership, and the owner's role is fixed.
    assert!(a.grant(actor(3), Role::Owner, "no", T).is_err());
    assert!(a.grant(actor(1), Role::ReadOnly, "no", T).is_err());
    // An editor changes the budget but doesn't lock the originals.
    try_edit(&mut editor, |b| b.title = "Edited".into())
        .unwrap()
        .unwrap();
    // The owner's grants, the admin's grant and the editor's edit reach
    // everyone.
    send(&admin, &mut a);
    send(&editor, &mut a);
    assert!(a.rejected().is_empty(), "{:?}", a.rejected());
    assert_eq!(a.role_of(actor(4)), Role::Actuals);
    let members = a.members();
    assert_eq!(members.len(), 3, "{members:?}");
}

/// Lenient exchange: every log gets every change it lacks, in a random
/// order; refusals stay in each log's rejected list.
fn exchange_all(rng: &mut Rng, logs: &mut [Log], extra: &[Change]) {
    let all: Vec<Change> = logs
        .iter()
        .flat_map(|l| l.changes().cloned().collect::<Vec<_>>())
        .chain(extra.iter().cloned())
        .collect();
    for log in logs.iter_mut() {
        let mut missing: Vec<Change> = all
            .iter()
            .filter(|c| !log.frontier().includes(c.id()))
            .cloned()
            .collect();
        missing.sort_by_key(|c| (c.actor, c.seq));
        missing.dedup_by_key(|c| c.id());
        rng.shuffle(&mut missing);
        for c in missing {
            match log.apply(c) {
                Ok(_) | Err(LogError::NotAllowed { .. }) => {}
                Err(e) => panic!("{e}"),
            }
        }
        assert!(log.pending().is_empty());
    }
}

#[test]
fn roles_converge_with_random_edits_in_any_order() {
    // (taken, refused at once) over every seed, for the department editor.
    let mut department = (0, 0);
    for seed in 0..24u64 {
        let mut rng = Rng::new(seed);
        let mut owner = adopt(1, &sample());
        let dept = scope(&owner, &["2000"]);
        owner.grant(actor(2), dept, "Department", T).unwrap();
        owner.grant(actor(3), Role::Editor, "Editor", T).unwrap();
        let mut logs = vec![owner.clone(), follower(2, &owner), follower(3, &owner)];
        let mut forged_seq = 0;
        let mut extra = Vec::new();
        for _round in 0..4 {
            for (i, log) in logs.iter_mut().enumerate() {
                for _ in 0..3 {
                    let p = log.project();
                    let mut after = p.budget.clone();
                    let (label, hints) = random_edit(&mut rng, &mut after, i as u8 + 1);
                    match log.record(&p, &after, &hints, &label, T) {
                        Ok(Some(_)) if i == 1 => department.0 += 1,
                        Err(LogError::NotAllowed { .. }) if i == 1 => department.1 += 1,
                        Ok(_) | Err(LogError::NotAllowed { .. }) => {}
                        Err(e) => panic!("seed {seed}: {label}: {e}"),
                    }
                }
                // The owner now and then changes the others' roles.
                if i == 0 && rng.chance(40) {
                    let codes: Vec<&str> = ["1000", "2000", "5000"]
                        .into_iter()
                        .filter(|_| rng.chance(50))
                        .collect();
                    let role = match rng.below(4) {
                        0 => Role::ReadOnly,
                        1 => Role::Editor,
                        2 => Role::Comment,
                        _ => Role::Department(
                            codes
                                .iter()
                                .filter_map(|c| log.project().binding.categories.get(*c).copied())
                                .collect(),
                        ),
                    };
                    let who = actor(*rng.pick(&[2u8, 3]));
                    log.grant(who, role, "Role", T).unwrap();
                }
            }
            // Someone outside the budget writes a change that claims to
            // have seen the roles.
            if rng.chance(50) {
                forged_seq += 1;
                extra.push(forged(
                    actor(9),
                    forged_seq,
                    &logs[0],
                    vec![title_op(&format!("Forged {forged_seq}"))],
                ));
            }
            exchange_all(&mut rng, &mut logs, &extra);
        }
        let first = &logs[0];
        for log in &logs[1..] {
            assert_eq!(log.frontier(), first.frontier(), "seed {seed}");
            assert_eq!(
                normalized(&log.project().budget),
                normalized(&first.project().budget),
                "seed {seed}"
            );
            assert_eq!(rejected_ids(log), rejected_ids(first), "seed {seed}");
            assert_eq!(log.members(), first.members(), "seed {seed}");
        }
        assert_eq!(
            rejected_ids(first).len(),
            forged_seq as usize,
            "seed {seed}"
        );
    }
    assert!(
        department.0 > 20 && department.1 > 20,
        "department editor: {department:?}"
    );
}

/// Every order of `items`.
fn orders<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let first = rest.remove(i);
        for mut tail in orders(&rest) {
            tail.insert(0, first.clone());
            out.push(tail);
        }
    }
    out
}

#[test]
fn a_forged_first_change_never_takes_the_budget() {
    // Actor 5 starts sharing and gives actor 2 a role.
    let mut a = adopt(5, &sample());
    let genesis = a.changes().next().unwrap().id();
    a.grant(actor(2), Role::Editor, "Jordan edits", T).unwrap();
    // Actor 1 (sorting before 5) writes a first change of its own, made
    // without the budget's first change: a forged one, and a second
    // computer starting to share the same file before the folder synced.
    let forged = Change {
        actor: actor(1),
        seq: 1,
        deps: Default::default(),
        lamport: 1,
        time: T.into(),
        label: "forged".into(),
        ops: vec![Op::Grant {
            actor: actor(3),
            role: Role::Admin,
        }],
        sig: None,
    };
    let second_start = adopt(0, &sample()).changes().next().unwrap().clone();
    let mut all: Vec<Change> = a.changes().cloned().collect();
    all.push(forged.clone());
    all.push(second_start.clone());
    for (n, order) in orders(&all).into_iter().enumerate() {
        let mut fresh = Log::new(actor(9), Ids::seeded(n as u64));
        fresh.set_genesis(genesis);
        for c in order {
            let _ = fresh.apply(c);
        }
        assert_eq!(fresh.owner(), Some(actor(5)), "order {n}");
        assert_eq!(
            rejected_ids(&fresh),
            BTreeSet::from([forged.id(), second_start.id()]),
            "order {n}"
        );
        assert_eq!(fresh.role_of(actor(3)), Role::ReadOnly, "order {n}");
        assert_eq!(fresh.role_of(actor(2)), Role::Editor, "order {n}");
        assert_eq!(fresh.frontier(), &{
            let mut f = a.frontier().clone();
            f.advance(actor(1), 1);
            f.advance(actor(0), 1);
            f
        });
        assert_eq!(
            normalized(&fresh.project().budget),
            normalized(&a.project().budget),
            "order {n}"
        );
    }
    // The log that started sharing refuses both too.
    assert!(a.apply(forged).is_err());
    assert!(a.apply(second_start).is_err());
    assert_eq!(a.owner(), Some(actor(5)));
}

#[test]
fn a_co_owner_sets_roles_admins_included() {
    let mut a = adopt(1, &sample());
    a.grant(actor(2), Role::CoOwner, "Priya co-owns", T)
        .unwrap();
    let mut co = follower(2, &a);
    assert!(co.role_of(actor(2)).sets_roles());
    // A co-owner makes and changes admins, as the owner does.
    co.grant(actor(3), Role::Admin, "Riley is an admin", T)
        .unwrap();
    co.grant(actor(3), Role::Editor, "Riley edits", T).unwrap();
    co.grant(actor(3), Role::Admin, "Riley is an admin again", T)
        .unwrap();
    // Only the owner makes co-owners or changes one, and nobody changes
    // the owner.
    for (who, role) in [(actor(4), Role::CoOwner), (actor(1), Role::ReadOnly)] {
        assert!(matches!(
            co.grant(who, role, "no", T),
            Err(LogError::NotAllowed { .. })
        ));
    }
    let mut admin = follower(3, &co);
    assert!(matches!(
        admin.grant(actor(2), Role::Editor, "no", T),
        Err(LogError::NotAllowed { .. })
    ));
    assert!(matches!(
        admin.grant(actor(4), Role::CoOwner, "no", T),
        Err(LogError::NotAllowed { .. })
    ));
    // The owner can still change the co-owner.
    send(&co, &mut a);
    assert!(a.rejected().is_empty(), "{:?}", a.rejected());
    assert_eq!(a.role_of(actor(3)), Role::Admin);
    a.grant(actor(2), Role::Admin, "Priya is an admin", T)
        .unwrap();
    // Now an admin, Priya no longer changes admins.
    send(&a, &mut co);
    assert!(matches!(
        co.grant(actor(3), Role::Editor, "no", T),
        Err(LogError::NotAllowed { .. })
    ));
}

/// Two grants for one person made apart: the higher-ranked granter wins
/// (owner over co-owner over admin), whatever the clocks say; a grant made
/// after seeing another replaces it as before.
#[test]
fn concurrent_grants_go_to_the_higher_ranked_granter() {
    let mut owner = adopt(1, &sample());
    owner
        .grant(actor(2), Role::CoOwner, "Priya co-owns", T)
        .unwrap();
    owner
        .grant(actor(3), Role::Admin, "Riley admin", T)
        .unwrap();
    let mut co = follower(2, &owner);
    let mut admin = follower(3, &owner);
    // Apart: the owner makes Jordan a co-owner and Kim an admin; the
    // co-owner, not knowing, makes Jordan read only (with a later clock);
    // the admin, not knowing, makes Kim an editor.
    owner
        .grant(actor(4), Role::CoOwner, "Jordan co-owns", T)
        .unwrap();
    owner.grant(actor(5), Role::Admin, "Kim admin", T).unwrap();
    for _ in 0..3 {
        try_edit(&mut co, |b| b.title.push('!')).unwrap();
    }
    co.grant(actor(4), Role::ReadOnly, "Jordan reads", T)
        .unwrap();
    admin.grant(actor(5), Role::Editor, "Kim edits", T).unwrap();
    let all: Vec<Change> = [&owner, &co, &admin]
        .iter()
        .flat_map(|l| l.changes().cloned().collect::<Vec<_>>())
        .collect();
    let mut rng = Rng::new(7);
    for n in 0..6 {
        let mut fresh = Log::new(actor(9), Ids::seeded(n));
        fresh.set_genesis(owner.changes().next().unwrap().id());
        let mut cs = all.clone();
        cs.sort_by_key(|c| c.id());
        cs.dedup_by_key(|c| c.id());
        rng.shuffle(&mut cs);
        for c in cs {
            let _ = fresh.apply(c);
        }
        assert!(fresh.rejected().is_empty(), "{:?}", fresh.rejected());
        assert_eq!(fresh.role_of(actor(4)), Role::CoOwner, "run {n}");
        assert_eq!(fresh.role_of(actor(5)), Role::Admin, "run {n}");
    }
    // Once the co-owner has seen the owner's grant, a later one of theirs
    // for someone they may change replaces it as usual.
    send(&owner, &mut co);
    send(&admin, &mut co);
    co.grant(actor(5), Role::Editor, "Kim edits", T).unwrap();
    assert_eq!(co.role_of(actor(5)), Role::Editor);
}

/// A command line or agent writes as an actor of its own, which says
/// whose it is; it has that person's role, so two programs on one computer
/// never share a sequence of changes (decisions.md, entry 78).
#[test]
fn a_tool_actor_has_its_persons_role() {
    let mut owner = adopt(1, &sample());
    owner
        .grant(actor(2), Role::Comment, "Jordan comments", T)
        .unwrap();
    owner
        .grant(actor(3), Role::Editor, "Riley edits", T)
        .unwrap();
    // Jordan's command line (actor 12) and Riley's (actor 13).
    let mut jt = follower(12, &owner);
    jt.acts_for(actor(2), "Command line for Jordan", T).unwrap();
    assert_eq!(jt.role_of(actor(12)), Role::Comment);
    assert!(matches!(
        try_edit(&mut jt, |b| b.title = "Jordan's".into()),
        Err(LogError::NotAllowed { .. })
    ));
    let mut st = follower(13, &owner);
    st.acts_for(actor(3), "Command line for Riley", T).unwrap();
    try_edit(&mut st, |b| b.title = "Riley's".into())
        .unwrap()
        .unwrap();
    // Without saying whose it is, a new actor reads only once roles exist.
    let mut stray = follower(14, &owner);
    assert_eq!(stray.role_of(actor(14)), Role::ReadOnly);
    assert!(try_edit(&mut stray, |b| b.title = "x".into()).is_err());
    // An actor speaks for one person only, once, and never for itself.
    assert!(st.acts_for(actor(2), "again", T).is_err());
    let mut me = follower(15, &owner);
    assert!(me.acts_for(actor(15), "myself", T).is_err());
    // Everyone else agrees.
    send(&jt, &mut owner);
    send(&st, &mut owner);
    assert!(owner.rejected().is_empty(), "{:?}", owner.rejected());
    assert_eq!(owner.role_of(actor(13)), Role::Editor);
    assert_eq!(owner.person_of(actor(13)), actor(3));
    assert_eq!(owner.person_of(actor(3)), actor(3));
    assert_eq!(owner.project().budget.title, "Riley's");
    // The person's role changes, and their tool's with it.
    owner
        .grant(actor(3), Role::ReadOnly, "Riley reads", T)
        .unwrap();
    assert_eq!(owner.role_of(actor(13)), Role::ReadOnly);
}

/// `acts_for` can't raise anyone: a member's own actor, the owner's, an
/// actor already judged under roles, a claim made before roles, and a
/// claim for someone who isn't the owner or a member are all refused, on
/// every computer in any order (MReview, pull request #27).
#[test]
fn acts_for_never_raises_a_member() {
    let mut owner = adopt(1, &sample());
    let genesis = owner.changes().next().unwrap().id();
    // A computer that will claim the owner before any role exists.
    let mut early = follower(7, &owner);
    owner
        .grant(actor(2), Role::ReadOnly, "Jordan reads", T)
        .unwrap();
    owner
        .grant(actor(3), Role::Editor, "Riley edits", T)
        .unwrap();
    let mut jordan = follower(2, &owner);
    let mut riley = follower(3, &owner);
    let mut stranger = follower(8, &owner);
    let title = |t: &str| Op::Set {
        entity: Uid::ROOT,
        field: Field::Title,
        value: Value::text(t),
    };
    // Each refused at once by its own log.
    assert!(matches!(
        jordan.acts_for(actor(1), "Jordan as owner", T),
        Err(LogError::NotAllowed { .. })
    ));
    assert!(matches!(
        owner.acts_for(actor(3), "Owner as Riley", T),
        Err(LogError::NotAllowed { .. })
    ));
    assert!(matches!(
        stranger.acts_for(actor(6), "For nobody", T),
        Err(LogError::NotAllowed { .. })
    ));
    assert!(matches!(
        early.acts_for(actor(1), "Early claim", T),
        Err(LogError::NotAllowed { .. })
    ));
    // Riley edits as an editor (judged under roles), then claims the owner.
    try_edit(&mut riley, |b| b.title = "Riley's".into())
        .unwrap()
        .unwrap();
    assert!(matches!(
        riley.acts_for(actor(1), "Riley as owner", T),
        Err(LogError::NotAllowed { .. })
    ));
    // Forged versions of each, written anyway, with an edit only the owner
    // could make riding along.
    let forge = |who: u8, seq: u64, seen: &Log, person: u8| Change {
        actor: actor(who),
        seq,
        deps: seen.frontier().clone(),
        lamport: seen.lamport() + 1,
        time: T.into(),
        label: "claim".into(),
        ops: vec![
            Op::ActsFor {
                person: actor(person),
            },
            title("Taken"),
            Op::Grant {
                actor: actor(who),
                role: Role::Admin,
            },
        ],
        sig: None,
    };
    let bad = vec![
        forge(2, 1, &owner, 1),
        forge(1, owner.own_seq() + 1, &owner, 3),
        forge(8, 1, &owner, 6),
        forge(3, riley.own_seq() + 1, &riley, 1),
        Change {
            deps: {
                let mut f = method_core::log::Frontier::new();
                f.advance(actor(1), 1);
                f
            },
            ..forge(7, 1, &owner, 1)
        },
    ];
    let mut honest: Vec<Change> = owner.changes().cloned().collect();
    honest.extend(riley.changes().filter(|c| c.actor == actor(3)).cloned());
    let mut rng = Rng::new(11);
    for n in 0..8 {
        let mut fresh = Log::new(actor(20), Ids::seeded(n));
        fresh.set_genesis(genesis);
        let mut all: Vec<Change> = honest.iter().chain(&bad).cloned().collect();
        rng.shuffle(&mut all);
        for c in all {
            let _ = fresh.apply(c);
        }
        let refused = rejected_ids(&fresh);
        for c in &bad {
            assert!(refused.contains(&c.id()), "run {n}: {:?} kept", c.id());
        }
        assert_eq!(fresh.project().budget.title, "Riley's", "run {n}");
        assert_eq!(fresh.role_of(actor(2)), Role::ReadOnly, "run {n}");
        assert_eq!(fresh.role_of(actor(7)), Role::ReadOnly, "run {n}");
        assert_eq!(fresh.role_of(actor(8)), Role::ReadOnly, "run {n}");
    }
}

/// A program limited to `locations`, on the first account (decisions.md,
/// entry 80).
fn program_at(locations: &[&str]) -> Credit {
    Credit {
        name: "Ridge program".into(),
        account: None,
        applied: false,
        cap: None,
        discount_percent: Dec::ZERO,
        excluded_fringes: BTreeSet::new(),
        locations: set(locations),
        minimum_spend: None,
        broker_fee: 0,
        shared_cap: None,
        rows: vec![CreditRow {
            description: "Spend".into(),
            source: CreditSource::Flat(100_000),
            percent: "25".parse().unwrap(),
            cap: CreditCap::None,
        }],
    }
}

/// Deleting a location on one replica keeps its name in a credit limited to
/// it on every replica, and the location stays deleted: a credit's
/// locations are a weak reference (decisions.md, entry 80).
#[test]
fn deleting_a_location_keeps_its_name_in_a_credit_without_reviving_it() {
    let mut start = sample();
    start.locations.insert("Ridge".into(), Location::default());
    start.credits.push(program_at(&["Ridge"]));
    let first = rows(&start)[0];
    row_mut(&mut start, first).unwrap().location = Some("Ridge".into());

    let mut a = adopt(1, &start);
    let mut b = follower(2, &a);
    let p = a.project();
    assert_eq!(normalized(&p.budget), normalized(&start));
    let mut after = p.budget.clone();
    after.locations.remove("Ridge");
    row_mut(&mut after, first).unwrap().location = None;
    record(&mut a, &p, &after, &[], "Delete location Ridge");

    // B edits something else meanwhile.
    let p = b.project();
    let mut other = p.budget.clone();
    other.title = "Elsewhere".into();
    record(&mut b, &p, &other, &[], "Title");

    let mut logs = vec![a, b];
    exchange(&mut Rng::new(7), &mut logs);
    for log in &logs {
        let p = log.project();
        assert!(!p.budget.locations.contains_key("Ridge"), "revived");
        let c = p.budget.credits.last().unwrap();
        assert_eq!(c.locations, set(&["Ridge"]));
        assert!(!review_keys(&p).contains(&"revived"), "{:?}", p.review);
    }
}

/// A credit without locations writes no `locations` in the change log, so
/// logs that don't use them stay readable by older versions.
#[test]
fn a_credit_without_locations_logs_no_locations() {
    let mut start = sample();
    start.credits.push(program_at(&[]));
    let log = adopt(1, &start);
    let none = log
        .changes()
        .flat_map(|c| &c.ops)
        .any(|op| format!("{op:?}").contains("Locations"));
    assert!(!none, "a Locations op was written");

    // With one, it is written and projects back.
    let mut with = sample();
    with.locations.insert("Ridge".into(), Location::default());
    with.credits.push(program_at(&["Ridge"]));
    let log = adopt(1, &with);
    assert!(log
        .changes()
        .flat_map(|c| &c.ops)
        .any(|op| format!("{op:?}").contains("Locations")));
    assert_eq!(normalized(&log.project().budget), normalized(&with));

    // Clearing them takes every name off, though the record leaves the set out.
    let mut log = log;
    let p = log.project();
    let mut cleared = p.budget.clone();
    cleared.credits.last_mut().unwrap().locations.clear();
    record(&mut log, &p, &cleared, &[], "Spend anywhere");
    let p = log.project();
    assert!(p.budget.credits.last().unwrap().locations.is_empty());
    assert_eq!(normalized(&p.budget), normalized(&cleared));
}

// ---------------------------------------------------------------------------
// Signed changes and removal by cutoff (decisions.md, entry 89). The engine
// holds no cryptography: these tests sign with a stand-in scheme (a hash of
// the key and the change), enough to tell a matching signature from a
// missing, altered or someone else's one. method-format's tests use Ed25519.

use method_core::log::{Frontier, PublicKey, Signature, Signer, Verifier};

struct TestKey(u8);

fn test_sig(key: &PublicKey, change: &Change) -> Signature {
    let mut c = change.clone();
    c.sig = None;
    let h = method_core::log::hash::sha256(format!("{key:?}{c:?}").as_bytes());
    let mut out = [0u8; 64];
    out[..32].copy_from_slice(&h);
    out[32..].copy_from_slice(&h);
    Signature(out)
}

impl Signer for TestKey {
    fn public(&self) -> PublicKey {
        PublicKey([self.0; 32])
    }
    fn sign(&self, change: &Change) -> Signature {
        test_sig(&self.public(), change)
    }
}

struct TestVerifier;

impl Verifier for TestVerifier {
    fn verify(&self, key: &PublicKey, change: &Change) -> bool {
        change.sig == Some(test_sig(key, change))
    }
    fn verify_message(&self, key: &PublicKey, message: &[u8], sig: &Signature) -> bool {
        *sig == test_message_sig(key, message)
    }
}

/// The stand-in scheme over bytes: what an invite key signs for a claim.
fn test_message_sig(key: &PublicKey, message: &[u8]) -> Signature {
    let mut m = format!("{key:?}").into_bytes();
    m.extend_from_slice(message);
    let h = method_core::log::hash::sha256(&m);
    let mut out = [0u8; 64];
    out[..32].copy_from_slice(&h);
    out[32..].copy_from_slice(&h);
    Signature(out)
}

/// `log` checking signatures, and signing with key `k` when given.
fn keyed(mut log: Log, k: Option<u8>) -> Log {
    log.set_verifier(Arc::new(TestVerifier));
    if let Some(k) = k {
        log.set_signer(Arc::new(TestKey(k)));
    }
    log
}

/// A second actor's log with `from`'s history, checking signatures and
/// signing with key `k`.
fn signed_follower(n: u8, from: &Log, k: u8) -> Log {
    let mut log = keyed(Log::new(actor(n), Ids::seeded(100 + n as u64)), Some(k));
    deliver(&mut log, &written(&[from]));
    assert!(log.rejected().is_empty(), "{:?}", log.rejected());
    log
}

/// An owner (actor 1, key 1) who has made actor 2 an editor: signing has
/// started with that first role.
fn signed_budget() -> Log {
    let mut owner = keyed(adopt(1, &sample()), Some(1));
    owner
        .grant(actor(2), Role::Editor, "Jordan edits", T)
        .unwrap();
    owner
}

/// Every change each log made, applied or refused: what the folder holds.
fn written(logs: &[&Log]) -> Vec<Change> {
    let mut all: Vec<Change> = logs
        .iter()
        .flat_map(|l| {
            l.changes()
                .cloned()
                .chain(l.rejected().iter().map(|(c, _)| c.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    all.sort_by_key(|c| (c.actor, c.seq));
    // Two different changes with one number (a forgery and the real one)
    // are both kept: each computer settles them.
    all.dedup();
    all
}

/// Hands `to` every change in `changes` it hasn't taken in yet.
fn deliver(to: &mut Log, changes: &[Change]) {
    for c in changes {
        if !to.frontier().includes(c.id()) {
            let _ = to.apply(c.clone());
        }
    }
}

fn reason_of(log: &Log, id: ChangeId) -> Option<String> {
    log.rejected()
        .iter()
        .find(|(c, _)| c.id() == id)
        .map(|(_, r)| r.clone())
}

#[test]
fn the_first_role_starts_signing_and_every_change_after_it_is_signed() {
    let owner = signed_budget();
    assert!(owner.signing_started());
    let grant = owner.changes().last().unwrap();
    assert_eq!(grant.key(), Some(PublicKey([1; 32])));
    assert!(grant.sig.is_some());
    assert_eq!(owner.key_of(actor(1)), Some(PublicKey([1; 32])));
    // The adopted first change, made before signing, stays unsigned.
    assert!(owner.changes().next().unwrap().sig.is_none());
    // Jordan's computer publishes its key with its first change and signs
    // it, and everyone checking signatures takes both in.
    let mut jordan = signed_follower(2, &owner, 2);
    try_edit(&mut jordan, |b| b.title = "Jordan's".into())
        .unwrap()
        .unwrap();
    let mine = jordan.changes().last().unwrap();
    assert_eq!(mine.key(), Some(PublicKey([2; 32])));
    assert!(mine.sig.is_some());
    try_edit(&mut jordan, |b| b.title = "Jordan's again".into())
        .unwrap()
        .unwrap();
    let second = jordan.changes().last().unwrap();
    assert_eq!(second.key(), None, "a key is published once");
    assert!(second.sig.is_some());
    let mut reader = keyed(Log::new(actor(9), Ids::seeded(9)), None);
    deliver(&mut reader, &written(&[&owner, &jordan]));
    assert!(reader.rejected().is_empty(), "{:?}", reader.rejected());
    assert_eq!(reader.project().budget.title, "Jordan's again");
    assert_eq!(reader.key_of(actor(2)), Some(PublicKey([2; 32])));
}

#[test]
fn a_budget_without_roles_never_signs() {
    let mut owner = keyed(adopt(1, &sample()), Some(1));
    let mut jordan = signed_follower(2, &owner, 2);
    try_edit(&mut owner, |b| b.title = "Owner's".into())
        .unwrap()
        .unwrap();
    try_edit(&mut jordan, |b| b.title = "Jordan's".into())
        .unwrap()
        .unwrap();
    for l in [&owner, &jordan] {
        assert!(!l.signing_started());
        assert!(l.changes().all(|c| c.sig.is_none() && c.key().is_none()));
    }
}

#[test]
fn a_tampered_change_is_kept_aside_with_why() {
    let owner = signed_budget();
    let mut jordan = signed_follower(2, &owner, 2);
    try_edit(&mut jordan, |b| b.title = "Jordan's".into())
        .unwrap()
        .unwrap();
    try_edit(&mut jordan, |b| b.title = "Jordan's second".into())
        .unwrap()
        .unwrap();
    let mut changes = written(&[&owner, &jordan]);
    let last = changes.last_mut().unwrap();
    last.ops = vec![title_op("Altered on the way")];
    let altered = last.id();
    let mut reader = keyed(Log::new(actor(9), Ids::seeded(9)), None);
    for c in &changes {
        let r = reader.apply(c.clone());
        if c.id() == altered {
            assert!(matches!(r, Err(LogError::Unsigned { .. })), "{r:?}");
        }
    }
    assert_eq!(reader.project().budget.title, "Jordan's");
    let why = reason_of(&reader, altered).unwrap();
    assert!(
        why.starts_with(method_core::log::refused::SIGNATURE),
        "{why}"
    );
    assert!(why.contains("doesn't match"), "{why}");
}

#[test]
fn a_change_without_a_signature_once_signing_has_started_is_kept_aside() {
    let owner = signed_budget();
    let mut jordan = signed_follower(2, &owner, 2);
    try_edit(&mut jordan, |b| b.title = "Jordan's".into())
        .unwrap()
        .unwrap();
    // In Jordan's name, after Jordan's key: no signature.
    let unsigned = forged(
        actor(2),
        jordan.own_seq() + 1,
        &jordan,
        vec![title_op("Unsigned")],
    );
    // Signed by someone else's key.
    let mut wrong = forged(
        actor(2),
        jordan.own_seq() + 1,
        &jordan,
        vec![title_op("Wrong key")],
    );
    wrong.sig = Some(TestKey(7).sign(&wrong));
    // A stranger who claims to have written before the owner's key and the
    // first role, unsigned.
    let mut early = Frontier::new();
    early.advance(actor(1), 1);
    let stranger = Change {
        deps: early.clone(),
        ..forged(actor(8), 1, &owner, vec![title_op("Before roles, it says")])
    };
    // The same stranger with a key of its own, signed: it signs, but it is
    // judged as after the first role, where it has none.
    let mut keyed_stranger = Change {
        deps: early,
        ..forged(
            actor(6),
            1,
            &owner,
            vec![
                title_op("Signed, before roles, it says"),
                Op::Key {
                    key: TestKey(6).public(),
                },
            ],
        )
    };
    keyed_stranger.sig = Some(TestKey(6).sign(&keyed_stranger));
    for (bad, what) in [
        (&unsigned, "isn't signed"),
        (&wrong, "doesn't match"),
        (&stranger, "isn't signed"),
        (&keyed_stranger, "reads only"),
    ] {
        let mut reader = keyed(Log::new(actor(9), Ids::seeded(9)), None);
        deliver(&mut reader, &written(&[&owner, &jordan]));
        let _ = reader.apply(bad.clone());
        let why = reason_of(&reader, bad.id()).unwrap_or_else(|| panic!("{what}: taken in"));
        assert!(why.contains(what), "{what}: {why}");
        assert_eq!(reader.project().budget.title, "Jordan's");
    }
    // A log that can't check signatures keeps aside every signed change.
    let mut blind = Log::new(actor(9), Ids::seeded(9));
    deliver(&mut blind, &written(&[&owner]));
    let why = reason_of(&blind, owner.changes().last().unwrap().id()).unwrap();
    assert!(why.contains("can't be checked"), "{why}");
}

#[test]
fn a_removed_persons_later_changes_are_kept_aside_and_earlier_ones_stay() {
    let mut owner = signed_budget();
    let mut jordan = signed_follower(2, &owner, 2);
    // Seen by the owner before the removal: stays.
    try_edit(&mut jordan, |b| b.title = "Before".into())
        .unwrap()
        .unwrap();
    send(&jordan, &mut owner);
    // Made apart, the owner not having seen it: after the cutoff.
    try_edit(&mut jordan, |b| {
        let id = rows(b)[0];
        row_mut(b, id).unwrap().description = "after".into();
    })
    .unwrap()
    .unwrap();
    let late = jordan.changes().last().unwrap().id();
    let removal = owner.revoke(actor(2), "Remove Jordan", T).unwrap();
    assert!(owner.removed(actor(2)));
    assert_eq!(owner.role_of(actor(2)), Role::ReadOnly);
    // Every order: the late change is kept aside, the early one kept.
    let all = written(&[&owner, &jordan]);
    let mut rng = Rng::new(5);
    for n in 0..12 {
        let mut fresh = keyed(Log::new(actor(9), Ids::seeded(n)), None);
        let mut shuffled = all.clone();
        rng.shuffle(&mut shuffled);
        deliver(&mut fresh, &shuffled);
        assert!(fresh.pending().is_empty());
        assert_eq!(fresh.project().budget.title, "Before", "order {n}");
        let why = reason_of(&fresh, late).unwrap_or_else(|| panic!("order {n}: late taken in"));
        assert!(why.starts_with(method_core::log::refused::REMOVED), "{why}");
        assert_eq!(
            fresh.rejected().len(),
            1,
            "order {n}: {:?}",
            fresh.rejected()
        );
    }
    // Jordan's own computer takes the removal in and lets the late change go.
    send(&owner, &mut jordan);
    assert!(reason_of(&jordan, late).is_some());
    assert_eq!(jordan.project().budget.title, "Before");
    // And can't change anything now.
    assert!(try_edit(&mut jordan, |b| b.title = "Still here".into()).is_err());
    // A role given again lets Jordan back in from then on.
    owner
        .grant(actor(2), Role::Editor, "Jordan edits again", T)
        .unwrap();
    assert!(!owner.removed(actor(2)));
    send(&owner, &mut jordan);
    try_edit(&mut jordan, |b| b.title = "Back".into())
        .unwrap()
        .unwrap();
    deliver(&mut owner, &written(&[&jordan]));
    assert!(owner.pending().is_empty());
    assert_eq!(owner.project().budget.title, "Back");
    assert!(reason_of(&owner, late).is_some(), "still kept aside");
    let _ = removal;
}

#[test]
fn only_those_who_set_a_persons_role_remove_them() {
    let mut owner = signed_budget();
    owner
        .grant(actor(3), Role::Admin, "Riley admin", T)
        .unwrap();
    owner.grant(actor(4), Role::Editor, "Kim edits", T).unwrap();
    let mut jordan = signed_follower(2, &owner, 2);
    assert!(matches!(
        jordan.revoke(actor(4), "Remove Kim", T),
        Err(LogError::NotAllowed { .. })
    ));
    let mut riley = signed_follower(3, &owner, 3);
    assert!(riley.revoke(actor(1), "Remove the owner", T).is_err());
    riley.revoke(actor(4), "Remove Kim", T).unwrap();
    owner.grant(actor(5), Role::Admin, "Lee admin", T).unwrap();
    send(&owner, &mut riley);
    assert!(
        riley.revoke(actor(5), "Remove Lee", T).is_err(),
        "an admin doesn't remove an admin"
    );
}

/// A command line or agent signs with its person's key: the same key file
/// on the same computer (decisions.md, entry 89).
#[test]
fn a_program_signs_with_its_persons_key() {
    let owner = signed_budget();
    let mut jordan = signed_follower(2, &owner, 2);
    jordan.publish_key("Key", T).unwrap().unwrap();
    // Jordan's command line, with Jordan's key.
    let mut tool = signed_follower(12, &owner, 2);
    deliver(&mut tool, &written(&[&jordan]));
    tool.acts_for(actor(2), "Command line for Jordan", T)
        .unwrap();
    try_edit(&mut tool, |b| b.title = "From the command line".into())
        .unwrap()
        .unwrap();
    // A program with a key of its own can't write for Jordan.
    let mut other = signed_follower(13, &owner, 13);
    deliver(&mut other, &written(&[&jordan]));
    assert!(matches!(
        other.acts_for(actor(2), "Not Jordan's", T),
        Err(LogError::Unsigned { .. })
    ));
    let mut reader = keyed(Log::new(actor(9), Ids::seeded(9)), None);
    deliver(&mut reader, &written(&[&owner, &jordan, &tool]));
    assert!(reader.rejected().is_empty(), "{:?}", reader.rejected());
    assert_eq!(reader.project().budget.title, "From the command line");
    assert_eq!(reader.person_of(actor(12)), actor(2));
}

/// Signed changes, forged and altered ones, removals and roles given again,
/// exchanged in random orders: every log, and a newcomer, ends with the same
/// frontier, budget and set of changes kept aside.
#[test]
fn signed_changes_and_removals_converge_in_any_order() {
    let mut cut = 0;
    // Forgeries at a member's next number that lost their place to the
    // real change.
    let mut replaced = 0;
    for seed in 0..24u64 {
        let mut rng = Rng::new(seed);
        let mut owner = signed_budget();
        owner
            .grant(actor(3), Role::Editor, "Riley edits", T)
            .unwrap();
        owner
            .grant(actor(4), scope(&owner, &["2000"]), "Kim's department", T)
            .unwrap();
        owner.grant(actor(5), Role::Editor, "Lee edits", T).unwrap();
        // Lee publishes a key and then says nothing more: every later
        // change in Lee's name is a forgery (so none collides with a real
        // one, which would be a fork, a separate rule).
        let mut lee = signed_follower(5, &owner, 5);
        lee.publish_key("Key", T).unwrap().unwrap();
        let mut logs = vec![owner.clone()];
        for n in 2..=4u8 {
            logs.push(signed_follower(n, &owner, n));
        }
        let mut collisions: Vec<Change> = Vec::new();
        let mut extra: Vec<Change> = lee
            .changes()
            .filter(|c| c.actor == actor(5))
            .cloned()
            .collect();
        for round in 0..4 {
            for (i, log) in logs.iter_mut().enumerate() {
                for _ in 0..2 {
                    let p = log.project();
                    let mut after = p.budget.clone();
                    let (label, hints) = random_edit(&mut rng, &mut after, i as u8 + 1);
                    match log.record(&p, &after, &hints, &label, T) {
                        Ok(_) | Err(LogError::NotAllowed { .. }) => {}
                        Err(e) => panic!("seed {seed}: {label}: {e}"),
                    }
                }
                if i == 0 && rng.chance(50) {
                    let who = actor(*rng.pick(&[2u8, 3, 4]));
                    if log.removed(who) || rng.chance(30) {
                        log.grant(who, Role::Editor, "Back", T).unwrap();
                    } else {
                        log.revoke(who, "Remove", T).unwrap();
                    }
                }
            }
            // Forgeries in Lee's name: unsigned, signed with another key,
            // and a real signed change of someone else's relabelled as Lee's.
            let seen = &logs[rng.below(4) as usize];
            let seq = extra.iter().filter(|c| c.actor == actor(5)).count() as u64 + 1;
            let mut f = forged(
                actor(5),
                seq,
                seen,
                vec![title_op(&format!("Forged {round}"))],
            );
            match rng.below(3) {
                0 => {}
                1 => f.sig = Some(TestKey(9).sign(&f)),
                _ => {
                    if let Some(real) = seen.changes().filter(|c| c.sig.is_some()).last() {
                        f = Change {
                            actor: actor(5),
                            seq,
                            deps: real.deps.clone(),
                            ..real.clone()
                        };
                    }
                }
            }
            extra.push(f);
            // A forgery at a member's next number, before the member writes
            // it: unsigned or signed with another key. It reaches the other
            // computers now, the member's own only at the end.
            if rng.chance(60) {
                let v = &logs[1 + rng.below(3) as usize];
                let mut f = forged(
                    v.actor(),
                    v.own_seq() + 1,
                    v,
                    vec![title_op(&format!("Taken {round}"))],
                );
                if rng.chance(50) {
                    f.sig = Some(TestKey(9).sign(&f));
                }
                collisions.push(f);
            }
            // Exchange everything written so far, in a random order per log
            // (a change whose number another has taken is handed over too).
            let mut all = written(&logs.iter().collect::<Vec<_>>());
            all.extend(extra.iter().cloned());
            for log in logs.iter_mut() {
                let mut missing: Vec<Change> = all
                    .iter()
                    .chain(collisions.iter().filter(|c| c.actor != log.actor()))
                    .filter(|c| log.change(c.id()) != Some(*c))
                    .cloned()
                    .collect();
                rng.shuffle(&mut missing);
                for c in missing {
                    let _ = log.apply(c);
                }
            }
        }
        let mut all = written(&logs.iter().collect::<Vec<_>>());
        all.extend(extra.iter().cloned());
        all.extend(collisions.iter().cloned());
        let mut newcomer = keyed(Log::new(actor(30), Ids::seeded(seed)), None);
        rng.shuffle(&mut all);
        deliver_all(&mut newcomer, &all);
        // Two more newcomers: every forgery at a member's number first, and
        // every one last.
        let is_collision = |c: &Change| collisions.contains(c);
        let mut forged_first = keyed(Log::new(actor(31), Ids::seeded(seed)), None);
        let mut real_first = keyed(Log::new(actor(32), Ids::seeded(seed)), None);
        let (fakes, reals): (Vec<Change>, Vec<Change>) =
            all.iter().cloned().partition(|c| is_collision(c));
        deliver_all(&mut forged_first, &[fakes.clone(), reals.clone()].concat());
        deliver_all(&mut real_first, &[reals, fakes].concat());
        for log in logs.iter_mut() {
            let mut mine = all.clone();
            rng.shuffle(&mut mine);
            deliver_all(log, &mine);
        }
        replaced += newcomer
            .forks()
            .iter()
            .filter(|(_, why)| why == method_core::log::FORK_REPLACED)
            .count();
        let first = &logs[0];
        // No forgery at a member's number is ever applied.
        for f in &collisions {
            for log in logs.iter().chain([&newcomer, &forged_first, &real_first]) {
                assert_ne!(
                    log.change(f.id()),
                    Some(f),
                    "seed {seed}: forgery {} applied",
                    f.id()
                );
            }
        }
        for log in logs
            .iter()
            .skip(1)
            .chain([&newcomer, &forged_first, &real_first])
        {
            assert!(log.pending().is_empty(), "seed {seed}");
            assert_eq!(log.frontier(), first.frontier(), "seed {seed}");
            assert_eq!(
                rejected_ids(log),
                rejected_ids(first),
                "seed {seed}: log of {:?}",
                log.actor()
            );
            assert_eq!(
                normalized(&log.project().budget),
                normalized(&first.project().budget),
                "seed {seed}"
            );
            assert_eq!(log.members(), first.members(), "seed {seed}");
        }
        // Lee's key change stands; every forgery after it is kept aside
        // (for its signature, or for not fitting at all).
        let forgeries = extra.iter().filter(|c| c.seq > 1).count();
        let kept_aside = first
            .rejected()
            .iter()
            .filter(|(c, _)| c.actor == actor(5))
            .count();
        assert_eq!(kept_aside, forgeries, "seed {seed}");
        cut += first
            .rejected()
            .iter()
            .filter(|(_, r)| r.starts_with(method_core::log::refused::REMOVED))
            .count();
    }
    assert!(cut > 5, "removals cut {cut} changes in all");
    assert!(replaced > 5, "forgeries that lost their place: {replaced}");
}

/// Hands `to` every change in `changes` (a change it already has is a
/// duplicate; one whose number another took contests it).
fn deliver_all(to: &mut Log, changes: &[Change]) {
    for c in changes {
        let _ = to.apply(c.clone());
    }
}

/// A refused change never keeps the real change with its number out: the
/// real one takes its place whichever arrives first, and the refused one
/// is listed (MReview, pull request #73).
#[test]
fn a_real_change_takes_the_place_of_a_forgery_at_its_number_in_either_order() {
    let owner = signed_budget();
    let mut jordan = signed_follower(2, &owner, 2);
    try_edit(&mut jordan, |b| b.title = "Jordan's".into())
        .unwrap()
        .unwrap();
    // A forgery at Jordan's next number, unsigned.
    let forgery = forged(
        actor(2),
        jordan.own_seq() + 1,
        &jordan,
        vec![title_op("Forged")],
    );
    try_edit(&mut jordan, |b| b.title = "Jordan's second".into())
        .unwrap()
        .unwrap();
    let real = jordan.changes().last().unwrap().clone();
    assert_eq!(real.id(), forgery.id());
    let base = written(&[&owner, &jordan]);
    for forged_first in [true, false] {
        let mut reader = keyed(Log::new(actor(9), Ids::seeded(9)), None);
        let mut order: Vec<Change> = base
            .iter()
            .filter(|c| c.id() != real.id())
            .cloned()
            .collect();
        if forged_first {
            order.push(forgery.clone());
            order.push(real.clone());
        } else {
            order.push(real.clone());
            order.push(forgery.clone());
        }
        deliver_all(&mut reader, &order);
        assert_eq!(
            reader.change(real.id()),
            Some(&real),
            "forged first {forged_first}"
        );
        assert_eq!(reader.project().budget.title, "Jordan's second");
        assert!(reader.rejected().is_empty(), "{:?}", reader.rejected());
        assert_eq!(reader.forks().len(), 1);
        assert_eq!(reader.forks()[0].0, forgery);
        // The same reason in either order: it doesn't pass, the real one does.
        assert_eq!(reader.forks()[0].1, method_core::log::FORK_REPLACED);
    }
}

/// Two different changes at one number that both pass can only come from
/// one actor writing twice (or, before signing, a forgery): neither is taken
/// in, in either order, and both are listed: never one applied in silence.
#[test]
fn two_valid_changes_with_one_number_are_refused_loudly() {
    let a = adopt(1, &sample());
    let title = a.project().budget.title.clone();
    let probe = follower(2, &a);
    let one = forged(actor(2), 1, &probe, vec![title_op("One")]);
    let two = forged(actor(2), 1, &probe, vec![title_op("Two")]);
    for (x, y) in [(&one, &two), (&two, &one)] {
        let mut b = follower(2, &a);
        b.apply(x.clone()).unwrap();
        assert!(matches!(b.apply(y.clone()), Err(LogError::Fork { .. })));
        assert_eq!(b.project().budget.title, title);
        assert_eq!(b.forks().len(), 1);
        assert_eq!(b.forks()[0].1, method_core::log::FORK_SECOND);
        assert_eq!(b.rejected().len(), 1);
        assert!(b.rejected()[0]
            .1
            .starts_with(method_core::log::refused::FORK));
        // Handed over again, nothing changes.
        let _ = b.apply(y.clone());
        let _ = b.apply(x.clone());
        assert_eq!(b.forks().len(), 1);
        assert_eq!(b.rejected().len(), 1);
    }
}

/// A key in this computer's name that isn't its own (someone else put one in
/// first): it signs nothing and records nothing as that actor, and says so.
#[test]
fn a_key_published_in_my_name_stops_me_signing() {
    let owner = signed_budget();
    // Someone with the folder publishes a key for Jordan before Jordan does.
    let mut taken = forged(
        actor(2),
        1,
        &owner,
        vec![Op::Key {
            key: TestKey(66).public(),
        }],
    );
    taken.sig = Some(TestKey(66).sign(&taken));
    let mut changes = written(&[&owner]);
    changes.push(taken);
    let mut jordan = keyed(Log::new(actor(2), Ids::seeded(2)), Some(2));
    deliver(&mut jordan, &changes);
    assert_eq!(jordan.key_conflict(), Some(TestKey(66).public()));
    let before = jordan.own_seq();
    let err = try_edit(&mut jordan, |b| b.title = "Mine".into()).unwrap_err();
    match err {
        LogError::Unsigned { reason, .. } => {
            assert_eq!(reason, method_core::log::sign::why::KEY_TAKEN)
        }
        e => panic!("{e}"),
    }
    assert!(jordan.publish_key("Key", T).is_err() || jordan.own_seq() == before);
    assert_eq!(jordan.own_seq(), before, "nothing recorded");
    // With no key of its own, a log has nothing to compare.
    let mut reader = keyed(Log::new(actor(9), Ids::seeded(9)), None);
    deliver(&mut reader, &changes);
    assert_eq!(reader.key_conflict(), None);
}

// ---------------------------------------------------------------------------
// The owner's key (docs/research/owner-key.md)

/// A folder whose owner key isn't the one this computer remembers (someone
/// rebuilt the log from the budget's first change with a key of their own):
/// the owner's changes with that key are kept aside, the log says the
/// owner's key changed, and nothing is recorded here. Without the
/// remembered key the same folder is taken in, which is the gap part 1
/// covers from the owner's side.
#[test]
fn a_remembered_owner_key_keeps_a_different_one_out() {
    let owner = signed_budget();
    let genesis = owner.changes().next().unwrap().clone();
    let mut after_genesis = Log::new(actor(9), Ids::seeded(9));
    after_genesis.apply(genesis.clone()).unwrap();
    let mut swapped = forged(
        actor(1),
        2,
        &after_genesis,
        vec![
            Op::Grant {
                actor: actor(2),
                role: Role::Editor,
            },
            Op::Key {
                key: TestKey(66).public(),
            },
        ],
    );
    swapped.sig = Some(TestKey(66).sign(&swapped));
    let folder = vec![genesis, swapped.clone()];

    // Jordan's computer saw key 1 first.
    let mut jordan = keyed(Log::new(actor(2), Ids::seeded(2)), Some(2));
    jordan.pin_owner_key(TestKey(1).public());
    deliver(&mut jordan, &folder);
    assert_eq!(jordan.owner_key_changed(), Some(TestKey(66).public()));
    let why = reason_of(&jordan, swapped.id()).expect("kept aside");
    assert!(
        why.contains(method_core::log::sign::why::OWNER_KEY_CHANGED),
        "{why}"
    );
    assert_eq!(jordan.key_of(actor(1)), None);
    let before = jordan.own_seq();
    match try_edit(&mut jordan, |b| b.title = "Mine".into()).unwrap_err() {
        LogError::Unsigned { reason, .. } => {
            assert_eq!(reason, method_core::log::sign::why::OWNER_KEY_CHANGED)
        }
        e => panic!("{e}"),
    }
    assert_eq!(jordan.own_seq(), before, "nothing recorded");

    // The real folder raises nothing on the same computer.
    let mut real = keyed(Log::new(actor(2), Ids::seeded(3)), Some(2));
    real.pin_owner_key(TestKey(1).public());
    deliver(&mut real, &written(&[&owner]));
    assert!(real.rejected().is_empty(), "{:?}", real.rejected());
    assert_eq!(real.owner_key_changed(), None);
    try_edit(&mut real, |b| b.title = "Mine".into())
        .unwrap()
        .unwrap();

    // A computer that remembers nothing takes the swapped folder in.
    let mut fresh = keyed(Log::new(actor(2), Ids::seeded(4)), Some(2));
    deliver(&mut fresh, &folder);
    assert!(fresh.rejected().is_empty(), "{:?}", fresh.rejected());
    assert_eq!(fresh.key_of(actor(1)), Some(TestKey(66).public()));
    assert_eq!(fresh.owner_key_changed(), None);
}

/// A budget started again publishes the owner's key in the change after its
/// first, before any role: signing starts there, and every computer checking
/// signatures takes it in.
#[test]
fn publishing_the_owners_key_before_any_role_starts_signing() {
    let mut owner = keyed(adopt(1, &sample()), Some(1));
    assert!(!owner.signing_started());
    let id = owner.publish_key_now("Key", T).unwrap().expect("published");
    assert!(owner.signing_started());
    assert_eq!(owner.key_of(actor(1)), Some(TestKey(1).public()));
    assert!(owner.change(id).unwrap().sig.is_some());
    assert_eq!(owner.publish_key_now("Key", T).unwrap(), None, "once");
    owner
        .grant(actor(2), Role::Editor, "Jordan edits", T)
        .unwrap();
    assert!(owner.changes().last().unwrap().sig.is_some());
    let mut reader = keyed(Log::new(actor(9), Ids::seeded(9)), None);
    deliver(&mut reader, &written(&[&owner]));
    assert!(reader.rejected().is_empty(), "{:?}", reader.rejected());
    assert!(reader.signing_started());
}

/// A credit's minimum spend, broker's fee and shared cap, and the budget's
/// shared caps (decisions.md, entry 88), are written in the change log only
/// when set, project back, and clear when taken off.
#[test]
fn credit_terms_and_shared_caps_log_only_when_set() {
    let mut start = sample();
    start.credits.push(program_at(&[]));
    let log = adopt(1, &start);
    let written = |log: &Log, word: &str| {
        log.changes()
            .flat_map(|c| &c.ops)
            .any(|op| format!("{op:?}").contains(word))
    };
    for word in ["MinimumSpend", "BrokerFee", "SharedCap"] {
        assert!(!written(&log, word), "{word} written");
    }

    let mut with = start.clone();
    let c = with.credits.last_mut().unwrap();
    c.minimum_spend = Some(50_000);
    c.broker_fee = 1_000;
    c.shared_cap = Some("State".into());
    with.shared_caps.insert("State".into(), 20_000);
    let mut log = adopt(1, &with);
    for word in ["MinimumSpend", "BrokerFee", "SharedCaps"] {
        assert!(written(&log, word), "{word} not written");
    }
    assert_eq!(normalized(&log.project().budget), normalized(&with));

    // Changing a shared cap's amount, then taking every term off.
    let p = log.project();
    let mut changed = p.budget.clone();
    changed.shared_caps.insert("State".into(), 30_000);
    record(&mut log, &p, &changed, &[], "Shared cap");
    let p = log.project();
    assert_eq!(p.budget.shared_caps.get("State"), Some(&30_000));
    let mut cleared = p.budget.clone();
    let c = cleared.credits.last_mut().unwrap();
    c.minimum_spend = None;
    c.broker_fee = 0;
    c.shared_cap = None;
    cleared.shared_caps.clear();
    record(&mut log, &p, &cleared, &[], "No terms");
    let p = log.project();
    assert_eq!(normalized(&p.budget), normalized(&cleared));
}

// ---------------------------------------------------------------------------
// Invites (decisions.md, entry 97). In the stand-in scheme an invite's
// secret `s` has the key `PublicKey([s; 32])`, and a claim's proof is that
// key's stand-in signature over the claim message.

use method_core::log::invite::{self as invites, claim_message};

/// An expiry a week after `T`.
const EXPIRES: &str = "2026-10-19T14:03:22Z";

fn invite_key(secret: u8) -> PublicKey {
    PublicKey([secret; 32])
}

/// The proof that `who` holds invite `invite`'s secret `secret`.
fn proof(secret: u8, invite: Uid, who: ActorId) -> Signature {
    test_message_sig(&invite_key(secret), &claim_message(invite, who))
}

/// A claim by `who` (key `who`'s number, published in it) of `invite` with
/// secret `secret`, made having seen everything `seen` has, at `time`: what
/// another computer could write, signed as theirs.
fn claim_change(who: u8, seen: &Log, invite: Uid, secret: u8, time: &str) -> Change {
    let mut c = Change {
        actor: actor(who),
        seq: 1,
        deps: seen.frontier().clone(),
        lamport: seen.lamport() + 1,
        time: time.into(),
        label: "Joined".into(),
        ops: vec![
            Op::Claim {
                invite,
                proof: proof(secret, invite, actor(who)),
            },
            Op::Key {
                key: PublicKey([who; 32]),
            },
        ],
        sig: None,
    };
    c.sig = Some(TestKey(who).sign(&c));
    c
}

fn invite_reason(log: &Log, id: ChangeId) -> Option<String> {
    reason_of(log, id).and_then(|r| {
        r.strip_prefix(method_core::log::refused::INVITE)
            .map(str::to_string)
    })
}

#[test]
fn an_invite_taken_up_gives_its_role_as_a_grant_would() {
    let mut owner = signed_budget();
    let role = scope(&owner, &["2000"]);
    let inv = Uid(0xA1);
    owner
        .invite(inv, role.clone(), EXPIRES, invite_key(50), "Invite", T)
        .unwrap();
    // Riley's computer has the folder but no role yet.
    let mut riley = signed_follower(6, &owner, 6);
    assert_eq!(riley.role_of(actor(6)), Role::ReadOnly);
    assert!(try_edit(&mut riley, |b| row_mut(b, 2).unwrap().rate = 7.into()).is_err());
    let claim = riley
        .claim(inv, proof(50, inv, actor(6)), "Joined", T)
        .unwrap();
    // The claim publishes Riley's key and is signed with it.
    let c = riley.change(claim).unwrap();
    assert_eq!(c.key(), Some(PublicKey([6; 32])));
    assert!(c.sig.is_some());
    assert_eq!(riley.role_of(actor(6)), role);
    assert_eq!(riley.invite_claimant(inv), Some(actor(6)));
    // Within the role, then outside it.
    try_edit(&mut riley, |b| row_mut(b, 2).unwrap().rate = 500.into())
        .unwrap()
        .unwrap();
    assert!(matches!(
        try_edit(&mut riley, |b| row_mut(b, 1).unwrap().rate = 1.into()),
        Err(LogError::NotAllowed { .. })
    ));
    // Everyone takes the claim and the edit in, in any order.
    let all = written(&[&owner, &riley]);
    let mut rng = Rng::new(11);
    for n in 0..8 {
        let mut fresh = keyed(Log::new(actor(9), Ids::seeded(n)), None);
        let mut shuffled = all.clone();
        rng.shuffle(&mut shuffled);
        deliver(&mut fresh, &shuffled);
        assert!(fresh.rejected().is_empty(), "{:?}", fresh.rejected());
        assert_eq!(fresh.role_of(actor(6)), role);
        assert_eq!(
            normalized(&fresh.project().budget),
            normalized(&riley.project().budget)
        );
    }
    // Used: a second person with the link is refused, on their own computer
    // first, and by everyone when written anyway.
    send(&riley, &mut owner);
    let mut morgan = signed_follower(7, &owner, 7);
    let e = morgan
        .claim(inv, proof(50, inv, actor(7)), "Joined", T)
        .unwrap_err();
    assert_eq!(
        e,
        LogError::Invite {
            id: ChangeId {
                actor: actor(7),
                seq: 1
            },
            reason: invites::why::USED.into()
        }
    );
    let late = claim_change(7, &owner, inv, 50, T);
    let _ = owner.apply(late.clone());
    assert_eq!(
        invite_reason(&owner, late.id()).as_deref(),
        Some(invites::why::USED)
    );
    assert_eq!(owner.role_of(actor(7)), Role::ReadOnly);
}

#[test]
fn claims_that_dont_stand_are_refused_and_listed() {
    let mut owner = signed_budget();
    let inv = Uid(0xB2);
    owner
        .invite(inv, Role::Editor, EXPIRES, invite_key(51), "Invite", T)
        .unwrap();
    // Every computer judges what reaches it, whatever its author's app did.
    let check = |c: Change, why: &str| {
        let mut o = owner.clone();
        let _ = o.apply(c.clone());
        assert_eq!(invite_reason(&o, c.id()).as_deref(), Some(why), "{c:?}");
        if c.actor != actor(1) {
            assert_eq!(o.role_of(c.actor), Role::ReadOnly);
        }
        assert_eq!(o.invite_claimant(inv), None);
    };
    // The wrong secret.
    let mut riley = signed_follower(6, &owner, 6);
    let e = riley
        .claim(inv, proof(52, inv, actor(6)), "Joined", T)
        .unwrap_err();
    assert!(
        matches!(e, LogError::Invite { ref reason, .. } if reason == invites::why::WRONG_SECRET)
    );
    assert!(riley.len() == owner.len(), "nothing recorded");
    check(
        claim_change(6, &owner, inv, 52, T),
        invites::why::WRONG_SECRET,
    );
    // Another person's proof, copied: it names the other actor.
    let mut copied = claim_change(6, &owner, inv, 51, T);
    copied.ops[0] = Op::Claim {
        invite: inv,
        proof: proof(51, inv, actor(8)),
    };
    copied.sig = None;
    copied.sig = Some(TestKey(6).sign(&copied));
    check(copied, invites::why::WRONG_SECRET);
    // After the expiry, by the claim's own clock.
    let after = "2026-10-20T00:00:00Z";
    let e = riley
        .claim(inv, proof(51, inv, actor(6)), "Joined", after)
        .unwrap_err();
    assert!(matches!(e, LogError::Invite { ref reason, .. } if reason == invites::why::EXPIRED));
    check(
        claim_change(6, &owner, inv, 51, after),
        invites::why::EXPIRED,
    );
    // An invite nobody had made, or one the claim hadn't seen.
    let before = signed_budget();
    let mut unseen = claim_change(6, &before, inv, 51, T);
    unseen.deps = before.frontier().clone();
    check(unseen, invites::why::NOT_FOUND);
    // The owner doesn't join their own budget.
    let mut own = claim_change(1, &owner, inv, 51, T);
    own.ops.truncate(1);
    own.seq = owner.own_seq() + 1;
    own.sig = None;
    own.sig = Some(TestKey(1).sign(&own));
    check(own, invites::why::OWNER);
}

#[test]
fn an_inviter_gives_only_what_their_role_gives_where_the_claim_is_made() {
    let mut owner = signed_budget();
    owner.grant(actor(3), Role::Admin, "Kim admin", T).unwrap();
    let mut kim = signed_follower(3, &owner, 3);
    // An admin invites editors, never admins.
    assert!(matches!(
        kim.invite(Uid(1), Role::Admin, EXPIRES, invite_key(60), "Invite", T),
        Err(LogError::NotAllowed { .. })
    ));
    let inv = Uid(0xC3);
    kim.invite(inv, Role::Editor, EXPIRES, invite_key(61), "Invite", T)
        .unwrap();
    send(&kim, &mut owner);
    // Kim is made an editor before the newcomer claims: the claim, made
    // having seen that, is refused.
    owner.grant(actor(3), Role::Editor, "Kim edits", T).unwrap();
    let mut riley = signed_follower(6, &owner, 6);
    let e = riley
        .claim(inv, proof(61, inv, actor(6)), "Joined", T)
        .unwrap_err();
    assert!(
        matches!(e, LogError::Invite { ref reason, .. } if reason == invites::why::NOT_ALLOWED)
    );
    let c = claim_change(6, &owner, inv, 61, T);
    let _ = owner.apply(c.clone());
    assert_eq!(
        invite_reason(&owner, c.id()).as_deref(),
        Some(invites::why::NOT_ALLOWED)
    );
    // A claim made before seeing it stands: judged where it was made.
    let mut before = owner.clone();
    let mut early_owner = signed_budget();
    early_owner
        .grant(actor(3), Role::Admin, "Kim admin", T)
        .unwrap();
    deliver(&mut early_owner, &written(&[&kim]));
    let early = claim_change(7, &early_owner, inv, 61, T);
    let _ = before.apply(early.clone());
    assert_eq!(reason_of(&before, early.id()), None);
    assert_eq!(before.role_of(actor(7)), Role::Editor);
}

#[test]
fn a_cancel_refuses_claims_its_author_hadnt_seen_in_any_order() {
    let mut owner = signed_budget();
    let used = Uid(0xD4);
    let open = Uid(0xD5);
    owner
        .invite(used, Role::Comment, EXPIRES, invite_key(70), "Invite", T)
        .unwrap();
    owner
        .invite(open, Role::Actuals, EXPIRES, invite_key(71), "Invite", T)
        .unwrap();
    // Morgan takes up the first, and the owner sees it before cancelling.
    let mut morgan = signed_follower(7, &owner, 7);
    morgan
        .claim(used, proof(70, used, actor(7)), "Joined", T)
        .unwrap();
    send(&morgan, &mut owner);
    // Riley has the second link and claims it without having seen the
    // cancel, then adds a transaction... any edit at all is enough.
    let mut riley = signed_follower(6, &owner, 6);
    owner.cancel_invite(used, "Cancel", T).unwrap();
    owner.cancel_invite(open, "Cancel", T).unwrap();
    let late = riley
        .claim(open, proof(71, open, actor(6)), "Joined", T)
        .unwrap();
    assert_eq!(
        riley.role_of(actor(6)),
        Role::Actuals,
        "until the cancel arrives"
    );
    // Only the canceller's author may cancel... and Jordan, an editor, can't.
    let mut jordan = signed_follower(2, &owner, 2);
    assert!(matches!(
        jordan.cancel_invite(used, "Cancel", T),
        Err(LogError::NotAllowed { .. })
    ));
    let all = written(&[&owner, &riley, &morgan]);
    let mut rng = Rng::new(17);
    for n in 0..12 {
        let mut fresh = keyed(Log::new(actor(9), Ids::seeded(n)), None);
        let mut shuffled = all.clone();
        rng.shuffle(&mut shuffled);
        deliver(&mut fresh, &shuffled);
        assert!(fresh.pending().is_empty());
        assert_eq!(
            invite_reason(&fresh, late).as_deref(),
            Some(invites::why::CANCELLED),
            "order {n}"
        );
        assert_eq!(fresh.role_of(actor(6)), Role::ReadOnly);
        // The claim the canceller had seen stands.
        assert_eq!(fresh.role_of(actor(7)), Role::Comment, "order {n}");
        assert_eq!(fresh.invite_claimant(used), Some(actor(7)));
        assert_eq!(fresh.rejected().len(), 1, "{:?}", fresh.rejected());
    }
    // Riley's computer takes the cancel in and lets the claim go.
    send(&owner, &mut riley);
    assert_eq!(
        invite_reason(&riley, late).as_deref(),
        Some(invites::why::CANCELLED)
    );
    assert_eq!(riley.role_of(actor(6)), Role::ReadOnly);
    // A removed inviter's invites are cancelled the same way.
    owner.grant(actor(3), Role::Admin, "Kim admin", T).unwrap();
    let mut kim = signed_follower(3, &owner, 3);
    let kims = Uid(0xD6);
    kim.invite(kims, Role::Editor, EXPIRES, invite_key(72), "Invite", T)
        .unwrap();
    send(&kim, &mut owner);
    let unseen = claim_change(8, &owner, kims, 72, T);
    owner.revoke(actor(3), "Remove Kim", T).unwrap();
    let _ = owner.apply(unseen.clone());
    assert_eq!(
        invite_reason(&owner, unseen.id()).as_deref(),
        Some(invites::why::CANCELLED)
    );
}

#[test]
fn two_claims_made_apart_settle_on_the_same_one_everywhere() {
    let mut owner = signed_budget();
    let inv = Uid(0xE5);
    owner
        .invite(inv, Role::Editor, EXPIRES, invite_key(80), "Invite", T)
        .unwrap();
    let mut riley = signed_follower(6, &owner, 6);
    let mut morgan = signed_follower(7, &owner, 7);
    riley
        .claim(inv, proof(80, inv, actor(6)), "Joined", T)
        .unwrap();
    morgan
        .claim(inv, proof(80, inv, actor(7)), "Joined", T)
        .unwrap();
    // Each edits under the role they think they have.
    try_edit(&mut riley, |b| b.title = "Riley's".into())
        .unwrap()
        .unwrap();
    try_edit(&mut morgan, |b| b.title = "Morgan's".into())
        .unwrap()
        .unwrap();
    // Same Lamport clock: the lower actor (Riley) wins, in every order and
    // on both their computers.
    let all = written(&[&owner, &riley, &morgan]);
    let mut rng = Rng::new(23);
    for n in 0..12 {
        let mut fresh = keyed(Log::new(actor(9), Ids::seeded(n)), None);
        let mut shuffled = all.clone();
        rng.shuffle(&mut shuffled);
        deliver(&mut fresh, &shuffled);
        assert_eq!(fresh.invite_claimant(inv), Some(actor(6)), "order {n}");
        assert_eq!(fresh.role_of(actor(7)), Role::ReadOnly);
        assert_eq!(fresh.project().budget.title, "Riley's");
        let why = invite_reason(
            &fresh,
            ChangeId {
                actor: actor(7),
                seq: 1,
            },
        );
        assert_eq!(why.as_deref(), Some(invites::why::USED), "order {n}");
        // Morgan's edit, made under a role that didn't stand, is left out.
        assert_eq!(fresh.rejected().len(), 2, "{:?}", fresh.rejected());
    }
    deliver(&mut morgan, &written(&[&riley]));
    assert_eq!(morgan.role_of(actor(7)), Role::ReadOnly);
    assert!(morgan.folds() > 0);
}

#[test]
fn invites_claims_and_cancels_converge_in_any_order() {
    let mut used = 0;
    let mut cancelled = 0;
    let mut joined = 0;
    let roles = |log: &Log| {
        vec![
            Role::Editor,
            Role::Comment,
            Role::Actuals,
            scope(log, &["2000"]),
        ]
    };
    for seed in 0..24u64 {
        let mut rng = Rng::new(seed);
        let mut owner = signed_budget();
        owner.grant(actor(3), Role::Admin, "Kim admin", T).unwrap();
        let mut logs = vec![owner.clone(), signed_follower(3, &owner, 3)];
        for n in 10..=14u8 {
            logs.push(signed_follower(n, &owner, n));
        }
        // Invites made so far: (id, secret).
        let mut made: Vec<(Uid, u8)> = Vec::new();
        let mut extra: Vec<Change> = Vec::new();
        let mut ghost = 40u8;
        for round in 0..5u8 {
            for (i, log) in logs.iter_mut().enumerate() {
                // The owner and Kim invite and cancel.
                if i < 2 && rng.chance(60) {
                    let id = Uid(seed as u128 * 1000 + round as u128 * 10 + i as u128 + 1);
                    let secret = 100 + made.len() as u8;
                    let role = rng.pick(&roles(log)).clone();
                    let expires = if rng.chance(20) {
                        "2026-10-01T00:00:00Z"
                    } else {
                        EXPIRES
                    };
                    log.invite(id, role, expires, invite_key(secret), "Invite", T)
                        .unwrap();
                    made.push((id, secret));
                }
                if i < 2 && !made.is_empty() && rng.chance(25) {
                    let (id, _) = *rng.pick(&made);
                    let _ = log.cancel_invite(id, "Cancel", T);
                }
                // Newcomers take up invites they have seen, now and then
                // with the wrong secret.
                if i >= 2 && !made.is_empty() && rng.chance(50) {
                    let (id, secret) = *rng.pick(&made);
                    let who = log.actor();
                    let secret = if rng.chance(15) { secret + 1 } else { secret };
                    let _ = log.claim(id, proof(secret, id, who), "Joined", T);
                }
                // Everyone edits; what their role doesn't allow is refused.
                let p = log.project();
                let mut after = p.budget.clone();
                let (label, hints) = random_edit(&mut rng, &mut after, i as u8 + 1);
                match log.record(&p, &after, &hints, &label, T) {
                    Ok(_) | Err(LogError::NotAllowed { .. }) | Err(LogError::Invite { .. }) => {}
                    Err(e) => panic!("seed {seed}: {label}: {e}"),
                }
            }
            // Someone who never ran this app writes a claim straight into
            // the folder: right or wrong secret, maybe after the expiry.
            if !made.is_empty() && rng.chance(50) {
                let seen = &logs[rng.below(logs.len() as u64) as usize];
                let (id, secret) = *rng.pick(&made);
                if seen.state().invites().contains_key(&id) {
                    let secret = if rng.chance(30) { secret + 7 } else { secret };
                    let time = if rng.chance(20) {
                        "2026-11-01T00:00:00Z"
                    } else {
                        T
                    };
                    extra.push(claim_change(ghost, seen, id, secret, time));
                    ghost += 1;
                }
            }
            let mut all = written(&logs.iter().collect::<Vec<_>>());
            all.extend(extra.iter().cloned());
            for log in logs.iter_mut() {
                let mut missing: Vec<Change> = all
                    .iter()
                    .filter(|c| !log.frontier().includes(c.id()))
                    .cloned()
                    .collect();
                rng.shuffle(&mut missing);
                if rng.chance(50) {
                    missing.truncate(missing.len() / 2);
                }
                for c in missing {
                    let _ = log.apply(c);
                }
            }
        }
        let mut all = written(&logs.iter().collect::<Vec<_>>());
        all.extend(extra.iter().cloned());
        let mut newcomers = Vec::new();
        for n in 0..3u8 {
            let mut fresh = keyed(Log::new(actor(30 + n), Ids::seeded(seed)), None);
            let mut shuffled = all.clone();
            rng.shuffle(&mut shuffled);
            if n == 2 {
                shuffled.reverse();
            }
            deliver_all(&mut fresh, &shuffled);
            newcomers.push(fresh);
        }
        for log in logs.iter_mut() {
            let mut mine = all.clone();
            rng.shuffle(&mut mine);
            deliver_all(log, &mine);
        }
        let first = &logs[0];
        for log in logs.iter().skip(1).chain(newcomers.iter()) {
            assert!(log.pending().is_empty(), "seed {seed}");
            assert_eq!(log.frontier(), first.frontier(), "seed {seed}");
            assert_eq!(rejected_ids(log), rejected_ids(first), "seed {seed}");
            assert_eq!(
                normalized(&log.project().budget),
                normalized(&first.project().budget),
                "seed {seed}"
            );
            assert_eq!(log.members(), first.members(), "seed {seed}");
            assert_eq!(
                log.state().invites(),
                first.state().invites(),
                "seed {seed}"
            );
        }
        // One claim at most per invite, and only one that stands.
        for (id, rec) in first.state().invites() {
            let claims = first
                .changes()
                .filter(|c| c.claim().is_some_and(|(i, _)| i == *id))
                .count();
            assert!(
                claims <= 1,
                "seed {seed}: invite {id:?} used {claims} times"
            );
            if let Some(c) = rec.claimed {
                joined += 1;
                // A role given by the claim (another invite or a grant may
                // have changed it since).
                assert!(
                    first.members().iter().any(|(a, _)| *a == c.actor),
                    "seed {seed}"
                );
            }
        }
        let why = |w: &str| {
            first
                .rejected()
                .iter()
                .filter(|(_, r)| r.strip_prefix(method_core::log::refused::INVITE) == Some(w))
                .count()
        };
        used += why(invites::why::USED);
        cancelled += why(invites::why::CANCELLED);
    }
    assert!(joined > 10, "invites taken up: {joined}");
    assert!(used > 3, "claims refused as used: {used}");
    assert!(cancelled > 3, "claims refused as cancelled: {cancelled}");
}
