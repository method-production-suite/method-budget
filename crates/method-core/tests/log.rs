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
    // Seen as seq 1 again from actor 2: the refused one moved the frontier.
    assert_eq!(a.apply(next).unwrap(), Applied::Duplicate);
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
        .grant(actor(4), Role::Actuals, "Sam does actuals", T)
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
