//! Capture by diff (docs/research/collaboration.md, 3.12): the budget
//! before an edit (the current projection) and the budget after it become
//! operations, matched by row id, code and name through the projection's
//! binding, with hints from the commands that rename or renumber so a
//! rename is one write and not a delete and an add.
//!
//! Property (tests/log.rs): applying the captured ops to the state and
//! projecting again gives exactly the budget after the edit.

use std::collections::{BTreeMap, BTreeSet};

use super::clock::Stamp;
use super::ids::{ActorId, Ids, Uid};
use super::op::{Field, Kind, ListKey, ListName, Op, Placement, Value};
use super::project::{Binding, Projection};
use super::rename_kind;
use super::schema::{self, Record};
use super::seq::{PosId, SeqTree};
use super::state::State;
use crate::model::{Account, Budget, Category, Detail, Note, ProductionTotal, TopsheetRow};
use crate::renumber::RenumberPlan;

/// What an edit command knows that a diff can't see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hint {
    /// A setup item, charge or credit was renamed.
    Rename {
        kind: Kind,
        from: String,
        to: String,
    },
    /// An account or category was renumbered.
    Recode {
        kind: Kind,
        from: String,
        to: String,
    },
}

impl Hint {
    /// The hint for [`crate::rename::rename`] of a setup item.
    pub fn rename(kind: crate::rename::SetupKind, from: &str, to: &str) -> Hint {
        Hint::Rename {
            kind: rename_kind(kind),
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    /// The hints for a renumbering plan: one per number that changes.
    pub fn from_plan(plan: &RenumberPlan) -> Vec<Hint> {
        let mut out = Vec::new();
        for c in &plan.categories {
            if c.old != c.new && !c.new.is_empty() {
                out.push(Hint::Recode {
                    kind: Kind::Category,
                    from: c.old.clone(),
                    to: c.new.clone(),
                });
            }
        }
        for c in &plan.accounts {
            if c.old != c.new && !c.new.is_empty() {
                out.push(Hint::Recode {
                    kind: Kind::Account,
                    from: c.old.clone(),
                    to: c.new.clone(),
                });
            }
        }
        out
    }
}

/// How an entity in the budget after the edit relates to the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    /// It was in the budget before.
    Existing,
    /// It was deleted and comes back under the same key.
    Restore,
    /// It is new.
    New,
}

#[derive(Clone, Copy, Debug)]
struct Planned {
    uid: Uid,
    status: Status,
}

struct RowPlan<'a> {
    p: Planned,
    d: &'a Detail,
}

struct AcctPlan<'a> {
    p: Planned,
    a: &'a Account,
    rows: Vec<RowPlan<'a>>,
}

struct CatPlan<'a> {
    p: Planned,
    c: &'a Category,
    accounts: Vec<AcctPlan<'a>>,
}

enum TsPlan<'a> {
    Cat(CatPlan<'a>),
    Pt(Planned, &'a ProductionTotal),
}

/// The budget before, indexed by entity id through the binding's lists.
#[derive(Default)]
struct BeforeIndex<'a> {
    cats: BTreeMap<Uid, &'a Category>,
    pts: BTreeMap<Uid, &'a ProductionTotal>,
    accts: BTreeMap<Uid, &'a Account>,
    rows: BTreeMap<Uid, &'a Detail>,
    notes: BTreeMap<Uid, &'a Note>,
    charges: BTreeMap<Uid, &'a crate::model::Charge>,
    excluded: BTreeMap<Uid, (String, i64)>,
    credits: BTreeMap<Uid, &'a crate::model::Credit>,
    credit_rows: BTreeMap<Uid, &'a crate::model::CreditRow>,
    versions: BTreeMap<Uid, &'a crate::model::Version>,
    transactions: BTreeMap<Uid, &'a crate::actuals::Transaction>,
    batches: BTreeMap<Uid, &'a crate::actuals::ImportBatch>,
}

impl<'a> BeforeIndex<'a> {
    fn build(before: &'a Projection) -> BeforeIndex<'a> {
        let b = &before.budget;
        let lists = &before.binding.lists;
        let list = |key: ListKey| lists.get(&key).map(Vec::as_slice).unwrap_or(&[]);
        let mut ix = BeforeIndex::default();
        let mut notes = |owner: Uid, ns: &'a [Note]| {
            for (uid, n) in list(ListKey::new(owner, ListName::Notes)).iter().zip(ns) {
                ix.notes.insert(*uid, n);
            }
        };
        for (uid, row) in list(ListKey::root(ListName::Topsheet))
            .iter()
            .zip(&b.topsheet)
        {
            match row {
                TopsheetRow::Category(c) => {
                    ix.cats.insert(*uid, c);
                    notes(*uid, &c.notes);
                    for (auid, a) in list(ListKey::new(*uid, ListName::Accounts))
                        .iter()
                        .zip(&c.accounts)
                    {
                        ix.accts.insert(*auid, a);
                        notes(*auid, &a.notes);
                        for (ruid, d) in list(ListKey::new(*auid, ListName::Rows))
                            .iter()
                            .zip(&a.details)
                        {
                            ix.rows.insert(*ruid, d);
                            notes(*ruid, &d.notes);
                        }
                    }
                }
                TopsheetRow::ProductionTotal(p) => {
                    ix.pts.insert(*uid, p);
                }
            }
        }
        for (uid, c) in list(ListKey::root(ListName::Charges))
            .iter()
            .zip(&b.charges)
        {
            ix.charges.insert(*uid, c);
            for (xuid, x) in list(ListKey::new(*uid, ListName::ExcludedAmounts))
                .iter()
                .zip(&c.excluded_amounts)
            {
                ix.excluded.insert(*xuid, x.clone());
            }
        }
        for (uid, c) in list(ListKey::root(ListName::Credits))
            .iter()
            .zip(&b.credits)
        {
            ix.credits.insert(*uid, c);
            for (ruid, r) in list(ListKey::new(*uid, ListName::CreditRows))
                .iter()
                .zip(&c.rows)
            {
                ix.credit_rows.insert(*ruid, r);
            }
        }
        for (uid, v) in list(ListKey::root(ListName::Versions))
            .iter()
            .zip(&b.versions)
        {
            ix.versions.insert(*uid, v);
        }
        for (uid, t) in list(ListKey::root(ListName::Transactions))
            .iter()
            .zip(&b.actuals.transactions)
        {
            ix.transactions.insert(*uid, t.as_ref());
        }
        for (uid, t) in list(ListKey::root(ListName::Batches))
            .iter()
            .zip(&b.actuals.batches)
        {
            ix.batches.insert(*uid, t);
        }
        ix
    }
}

/// The longest common subsequence of two sequences of ids, as the set of
/// ids that stay put.
fn stable_set(before: &[Uid], after: &[Uid]) -> BTreeSet<Uid> {
    let (n, m) = (before.len(), after.len());
    if n == 0 || m == 0 {
        return BTreeSet::new();
    }
    let mut table = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[at(i, j)] = if before[i] == after[j] {
                table[at(i + 1, j + 1)] + 1
            } else {
                table[at(i + 1, j)].max(table[at(i, j + 1)])
            };
        }
    }
    let mut out = BTreeSet::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if before[i] == after[j] {
            out.insert(before[i]);
            i += 1;
            j += 1;
        } else if table[at(i + 1, j)] >= table[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

struct Capture<'a> {
    state: &'a State,
    before: &'a Projection,
    ix: BeforeIndex<'a>,
    /// The binding as it stands after this edit: hints applied, new ids
    /// drawn.
    local: Binding,
    ids: &'a mut Ids,
    lamport: u64,
    actor: ActorId,
    seq: u64,
    ops: Vec<Op>,
    trees: BTreeMap<ListKey, SeqTree>,
    placed: BTreeMap<Uid, PosId>,
    matched: BTreeSet<Uid>,
    /// Dead entities this edit brings back.
    restoring: BTreeSet<Uid>,
}

impl<'a> Capture<'a> {
    fn next_stamp(&self) -> Stamp {
        Stamp {
            lamport: self.lamport,
            actor: self.actor,
            seq: self.seq,
            index: self.ops.len() as u32,
        }
    }

    fn fresh(&mut self) -> Uid {
        loop {
            let uid = Uid::random(self.ids);
            if self.state.entity(uid).is_none() {
                return uid;
            }
        }
    }

    fn pos_of(&self, uid: Uid) -> Option<PosId> {
        self.placed.get(&uid).copied().or_else(|| {
            self.state
                .entity(uid)
                .and_then(|e| e.place())
                .map(|p| p.pos)
        })
    }

    /// A new position right after `left` in `key` (at the start for
    /// `None`), for the op about to be emitted.
    fn placement(&mut self, key: ListKey, left: Option<Uid>) -> Placement {
        let left_pos = left.and_then(|u| self.pos_of(u));
        let pos = self.next_stamp();
        let state = self.state;
        let tree = self
            .trees
            .entry(key)
            .or_insert_with(|| state.list(&key).cloned().unwrap_or_default());
        let left_pos = left_pos.filter(|p| tree.contains(p));
        let node = tree.after(left_pos);
        tree.add(pos, node);
        Placement {
            list: key,
            pos,
            node: Some(node),
        }
    }

    /// Field and set writes that take `entity` from `before` to `after`.
    fn diff(&mut self, entity: Uid, before: Option<&Record>, after: &Record) {
        let empty = Record::default();
        let before = before.unwrap_or(&empty);
        for (f, v) in &after.fields {
            if before.fields.get(f) != Some(v) {
                self.ops.push(Op::Set {
                    entity,
                    field: *f,
                    value: v.clone(),
                });
            }
        }
        for (f, members) in &after.sets {
            let was = before.sets.get(f);
            for m in members {
                if !was.is_some_and(|w| w.contains(m)) {
                    self.ops.push(Op::Add {
                        entity,
                        field: *f,
                        member: m.clone(),
                    });
                }
            }
            if let Some(was) = was {
                for m in was {
                    if !members.contains(m) {
                        let seen = self
                            .state
                            .entity(entity)
                            .and_then(|e| e.sets.get(f))
                            .map(|s| s.tags(m))
                            .unwrap_or_default();
                        self.ops.push(Op::Remove {
                            entity,
                            field: *f,
                            member: m.clone(),
                            seen,
                        });
                    }
                }
            }
        }
    }

    /// Writes one entity of a list: its fields, and its place when it is
    /// new, restored or moved. `before` is its record before the edit
    /// (none for a new one); `stable` says it keeps its place.
    #[allow(clippy::too_many_arguments)]
    fn member(
        &mut self,
        key: ListKey,
        p: Planned,
        kind: Kind,
        before: Option<Record>,
        after: Record,
        stable: bool,
        left: Option<Uid>,
    ) {
        match p.status {
            Status::New => {
                let place = self.placement(key, left);
                self.placed.insert(p.uid, place.pos);
                self.ops.push(Op::Create {
                    entity: p.uid,
                    kind,
                    place: Some(place),
                    fields: after.fields,
                    sets: after.sets,
                });
            }
            Status::Restore => {
                self.ops.push(Op::Restore { entity: p.uid });
                let current = self.state.entity(p.uid).map(Record::of);
                self.diff(p.uid, current.as_ref(), &after);
                let place = self.placement(key, left);
                self.placed.insert(p.uid, place.pos);
                self.ops.push(Op::Move {
                    entity: p.uid,
                    place,
                });
            }
            Status::Existing => {
                self.matched.insert(p.uid);
                self.diff(p.uid, before.as_ref(), &after);
                if !stable {
                    let place = self.placement(key, left);
                    self.placed.insert(p.uid, place.pos);
                    self.ops.push(Op::Move {
                        entity: p.uid,
                        place,
                    });
                }
            }
        }
    }

    /// Which existing members of `key` keep their place, given the order
    /// wanted after the edit.
    fn stable(&self, key: ListKey, after: &[Planned]) -> BTreeSet<Uid> {
        let after_uids: BTreeSet<Uid> = after
            .iter()
            .filter(|p| p.status == Status::Existing)
            .map(|p| p.uid)
            .collect();
        let before_order: Vec<Uid> = self
            .before
            .binding
            .lists
            .get(&key)
            .map(|v| {
                v.iter()
                    .copied()
                    .filter(|u| after_uids.contains(u))
                    .collect()
            })
            .unwrap_or_default();
        let before_set: BTreeSet<Uid> = before_order.iter().copied().collect();
        let after_order: Vec<Uid> = after
            .iter()
            .map(|p| p.uid)
            .filter(|u| before_set.contains(u))
            .collect();
        stable_set(&before_order, &after_order)
    }

    /// A named entity's status under the local binding: existing,
    /// restored from the dead, or new (with a fresh id drawn and bound).
    fn plan_named(&mut self, kind: Kind, name: &str) -> Planned {
        if let Some(uid) = self.local.names.get(&(kind, name.to_string())) {
            let status = if self.before.binding.shown.contains(uid) {
                Status::Existing
            } else if self.restoring.contains(uid) {
                Status::Restore
            } else {
                Status::New
            };
            return Planned { uid: *uid, status };
        }
        if let Some(uid) = self.before.binding.dead.get(&(kind, name.to_string())) {
            let uid = *uid;
            self.local.names.insert((kind, name.to_string()), uid);
            self.restoring.insert(uid);
            return Planned {
                uid,
                status: Status::Restore,
            };
        }
        let uid = self.fresh();
        self.local.names.insert((kind, name.to_string()), uid);
        Planned {
            uid,
            status: Status::New,
        }
    }

    /// An entity known by an id (rows, notes, versions, transactions,
    /// batches): existing when the binding has it, restored when a dead
    /// one has the id, else new.
    fn plan_by_id(&mut self, kind: Kind, live: Option<Uid>, dead_key: &str) -> Planned {
        if let Some(uid) = live {
            return Planned {
                uid,
                status: Status::Existing,
            };
        }
        if let Some(uid) = self.before.binding.dead.get(&(kind, dead_key.to_string())) {
            return Planned {
                uid: *uid,
                status: Status::Restore,
            };
        }
        Planned {
            uid: self.fresh(),
            status: Status::New,
        }
    }

    fn notes(&mut self, owner: Uid, notes: &[Note]) {
        let key = ListKey::new(owner, ListName::Notes);
        let mut claimed = BTreeSet::new();
        let mut plans = Vec::new();
        for n in notes {
            let live = self
                .before
                .binding
                .notes
                .get(&(owner, n.id))
                .copied()
                .filter(|u| claimed.insert(*u));
            let p = self.plan_by_id(Kind::Note, live, &Binding::note_key(owner, n.id));
            claimed.insert(p.uid);
            plans.push(p);
        }
        let stable = self.stable(key, &plans);
        let mut left = None;
        for (p, n) in plans.iter().zip(notes) {
            let before = self.ix.notes.get(&p.uid).map(|n| schema::encode_note(n));
            self.member(
                key,
                *p,
                Kind::Note,
                before,
                schema::encode_note(n),
                stable.contains(&p.uid),
                left,
            );
            left = Some(p.uid);
        }
    }

    // -----------------------------------------------------------------------
    // Phase A: matching

    fn plan_topsheet<'b>(&mut self, after: &'b Budget, hints: &[Hint]) -> Vec<TsPlan<'b>> {
        // Accounts first, across all categories: by code, then by the rows
        // they hold, then by renumbering hints.
        let mut claimed: BTreeSet<Uid> = BTreeSet::new();
        let after_accts: Vec<&Account> = after.categories().flat_map(|c| &c.accounts).collect();
        let mut acct_uid: Vec<Option<Uid>> = vec![None; after_accts.len()];
        for (i, a) in after_accts.iter().enumerate() {
            if let Some(uid) = self.before.binding.accounts.get(&a.code) {
                if claimed.insert(*uid) {
                    acct_uid[i] = Some(*uid);
                }
            }
        }
        let unmatched_before: Vec<Uid> = self
            .ix
            .accts
            .keys()
            .copied()
            .filter(|u| !claimed.contains(u))
            .collect();
        // By shared rows, best overlap first.
        let mut scores: Vec<(usize, usize, Uid)> = Vec::new();
        for (i, a) in after_accts.iter().enumerate() {
            if acct_uid[i].is_some() {
                continue;
            }
            let ids: BTreeSet<_> = a.details.iter().map(|d| d.id).collect();
            for buid in &unmatched_before {
                let shared = self.ix.accts[buid]
                    .details
                    .iter()
                    .filter(|d| ids.contains(&d.id))
                    .count();
                if shared > 0 {
                    scores.push((shared, i, *buid));
                }
            }
        }
        scores.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
        for (_, i, buid) in scores {
            if acct_uid[i].is_none() && claimed.insert(buid) {
                acct_uid[i] = Some(buid);
            }
        }
        for h in hints {
            if let Hint::Recode {
                kind: Kind::Account,
                from,
                to,
            } = h
            {
                if let Some(uid) = self.before.binding.accounts.get(from).copied() {
                    if claimed.contains(&uid) {
                        continue;
                    }
                    if let Some(i) = (0..after_accts.len())
                        .find(|&i| after_accts[i].code == *to && acct_uid[i].is_none())
                    {
                        acct_uid[i] = Some(uid);
                        claimed.insert(uid);
                    }
                }
            }
        }
        // Categories: by code, then by the accounts they hold, then hints.
        let mut cat_claimed: BTreeSet<Uid> = BTreeSet::new();
        let after_cats: Vec<&Category> = after.categories().collect();
        let mut cat_uid: Vec<Option<Uid>> = vec![None; after_cats.len()];
        for (i, c) in after_cats.iter().enumerate() {
            if let Some(uid) = self.before.binding.categories.get(&c.code) {
                if cat_claimed.insert(*uid) {
                    cat_uid[i] = Some(*uid);
                }
            }
        }
        // The matched account uids of each after category, in order.
        let mut acct_iter = acct_uid.iter();
        let after_cat_accts: Vec<Vec<Option<Uid>>> = after_cats
            .iter()
            .map(|c| acct_iter.by_ref().take(c.accounts.len()).copied().collect())
            .collect();
        let mut scores: Vec<(usize, usize, Uid)> = Vec::new();
        for (i, _) in after_cats.iter().enumerate() {
            if cat_uid[i].is_some() {
                continue;
            }
            let mine: BTreeSet<Uid> = after_cat_accts[i].iter().flatten().copied().collect();
            for buid in self.ix.cats.keys() {
                if cat_claimed.contains(buid) {
                    continue;
                }
                let shared = self
                    .before
                    .binding
                    .lists
                    .get(&ListKey::new(*buid, ListName::Accounts))
                    .map(|v| v.iter().filter(|u| mine.contains(u)).count())
                    .unwrap_or(0);
                if shared > 0 {
                    scores.push((shared, i, *buid));
                }
            }
        }
        scores.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
        for (_, i, buid) in scores {
            if cat_uid[i].is_none() && cat_claimed.insert(buid) {
                cat_uid[i] = Some(buid);
            }
        }
        for h in hints {
            if let Hint::Recode {
                kind: Kind::Category,
                from,
                to,
            } = h
            {
                if let Some(uid) = self.before.binding.categories.get(from).copied() {
                    if cat_claimed.contains(&uid) {
                        continue;
                    }
                    if let Some(i) = after_cats
                        .iter()
                        .enumerate()
                        .position(|(i, c)| &c.code == to && cat_uid[i].is_none())
                    {
                        cat_uid[i] = Some(uid);
                        cat_claimed.insert(uid);
                    }
                }
            }
        }
        // Production totals: by name in order, leftovers paired in order.
        let before_pts: Vec<Uid> = self
            .before
            .binding
            .lists
            .get(&ListKey::root(ListName::Topsheet))
            .map(|v| {
                v.iter()
                    .copied()
                    .filter(|u| self.ix.pts.contains_key(u))
                    .collect()
            })
            .unwrap_or_default();
        let after_pts: Vec<&ProductionTotal> = after
            .topsheet
            .iter()
            .filter_map(|r| match r {
                TopsheetRow::ProductionTotal(p) => Some(p),
                _ => None,
            })
            .collect();
        let mut pt_claimed: BTreeSet<Uid> = BTreeSet::new();
        let mut pt_uid: Vec<Option<Uid>> = vec![None; after_pts.len()];
        for (i, p) in after_pts.iter().enumerate() {
            if let Some(uid) = before_pts
                .iter()
                .find(|u| !pt_claimed.contains(u) && self.ix.pts[u].name == p.name)
            {
                pt_uid[i] = Some(*uid);
                pt_claimed.insert(*uid);
            }
        }
        let leftover: Vec<Uid> = before_pts
            .iter()
            .copied()
            .filter(|u| !pt_claimed.contains(u))
            .collect();
        let mut leftover = leftover.into_iter();
        for slot in pt_uid.iter_mut().filter(|s| s.is_none()) {
            if let Some(uid) = leftover.next() {
                *slot = Some(uid);
            }
        }
        // Rows: by id, then by a dead row's id.
        let mut row_claimed: BTreeSet<Uid> = BTreeSet::new();
        let mut plans = Vec::new();
        let (mut ci, mut ai, mut pi) = (0, 0, 0);
        for row in &after.topsheet {
            match row {
                TopsheetRow::Category(c) => {
                    let p = match cat_uid[ci] {
                        Some(uid) => Planned {
                            uid,
                            status: Status::Existing,
                        },
                        None => Planned {
                            uid: self.fresh(),
                            status: Status::New,
                        },
                    };
                    // The code binds to this category after the edit, new
                    // or renumbered.
                    self.local.categories.insert(c.code.clone(), p.uid);
                    let mut accounts = Vec::new();
                    for a in &c.accounts {
                        let ap = match acct_uid[ai] {
                            Some(uid) => Planned {
                                uid,
                                status: Status::Existing,
                            },
                            None => Planned {
                                uid: self.fresh(),
                                status: Status::New,
                            },
                        };
                        ai += 1;
                        self.local.accounts.insert(a.code.clone(), ap.uid);
                        let mut rows = Vec::new();
                        for d in &a.details {
                            let live = self
                                .before
                                .binding
                                .rows
                                .get(&d.id)
                                .copied()
                                .filter(|u| row_claimed.insert(*u));
                            let rp = self.plan_by_id(Kind::Row, live, &d.id.to_string());
                            row_claimed.insert(rp.uid);
                            if rp.status != Status::Existing {
                                self.local.rows.insert(d.id, rp.uid);
                            }
                            rows.push(RowPlan { p: rp, d });
                        }
                        accounts.push(AcctPlan { p: ap, a, rows });
                    }
                    ci += 1;
                    plans.push(TsPlan::Cat(CatPlan { p, c, accounts }));
                }
                TopsheetRow::ProductionTotal(p) => {
                    let pp = match pt_uid[pi] {
                        Some(uid) => Planned {
                            uid,
                            status: Status::Existing,
                        },
                        None => Planned {
                            uid: self.fresh(),
                            status: Status::New,
                        },
                    };
                    pi += 1;
                    plans.push(TsPlan::Pt(pp, p));
                }
            }
        }
        plans
    }

    // -----------------------------------------------------------------------
    // Phase B: writing

    fn write_topsheet(&mut self, plans: &[TsPlan<'_>]) {
        let key = ListKey::root(ListName::Topsheet);
        let planned: Vec<Planned> = plans
            .iter()
            .map(|p| match p {
                TsPlan::Cat(c) => c.p,
                TsPlan::Pt(p, _) => *p,
            })
            .collect();
        let stable = self.stable(key, &planned);
        let mut left = None;
        for plan in plans {
            match plan {
                TsPlan::Cat(cat) => {
                    let before = self
                        .ix
                        .cats
                        .get(&cat.p.uid)
                        .map(|c| schema::encode_category(c));
                    self.member(
                        key,
                        cat.p,
                        Kind::Category,
                        before,
                        schema::encode_category(cat.c),
                        stable.contains(&cat.p.uid),
                        left,
                    );
                    left = Some(cat.p.uid);
                    self.notes(cat.p.uid, &cat.c.notes);
                    self.write_accounts(cat);
                }
                TsPlan::Pt(p, pt) => {
                    let before = self
                        .ix
                        .pts
                        .get(&p.uid)
                        .map(|x| schema::encode_production_total(x));
                    self.member(
                        key,
                        *p,
                        Kind::ProductionTotal,
                        before,
                        schema::encode_production_total(pt),
                        stable.contains(&p.uid),
                        left,
                    );
                    left = Some(p.uid);
                }
            }
        }
    }

    fn write_accounts(&mut self, cat: &CatPlan<'_>) {
        let key = ListKey::new(cat.p.uid, ListName::Accounts);
        let planned: Vec<Planned> = cat.accounts.iter().map(|a| a.p).collect();
        let stable = self.stable(key, &planned);
        let mut left = None;
        for acct in &cat.accounts {
            let before = self
                .ix
                .accts
                .get(&acct.p.uid)
                .map(|a| schema::encode_account(a));
            self.member(
                key,
                acct.p,
                Kind::Account,
                before,
                schema::encode_account(acct.a),
                stable.contains(&acct.p.uid),
                left,
            );
            left = Some(acct.p.uid);
            self.notes(acct.p.uid, &acct.a.notes);
            let rkey = ListKey::new(acct.p.uid, ListName::Rows);
            let rplanned: Vec<Planned> = acct.rows.iter().map(|r| r.p).collect();
            let rstable = self.stable(rkey, &rplanned);
            let mut rleft = None;
            for row in &acct.rows {
                let before = self
                    .ix
                    .rows
                    .get(&row.p.uid)
                    .map(|d| schema::encode_row(d, &self.before.binding));
                let after = schema::encode_row(row.d, &self.local);
                self.member(
                    rkey,
                    row.p,
                    Kind::Row,
                    before,
                    after,
                    rstable.contains(&row.p.uid),
                    rleft,
                );
                rleft = Some(row.p.uid);
                self.notes(row.p.uid, &row.d.notes);
            }
        }
    }

    /// Setup items of one kind: a map by name.
    fn write_map(
        &mut self,
        kind: Kind,
        after: Vec<(String, Record)>,
        before_of: &dyn Fn(&str) -> Option<Record>,
    ) {
        for (name, rec) in after {
            let p = self.plan_named(kind, &name);
            match p.status {
                Status::New => {
                    self.ops.push(Op::Create {
                        entity: p.uid,
                        kind,
                        place: None,
                        fields: rec.fields,
                        sets: rec.sets,
                    });
                }
                Status::Restore => {
                    self.ops.push(Op::Restore { entity: p.uid });
                    let current = self.state.entity(p.uid).map(Record::of);
                    self.diff(p.uid, current.as_ref(), &rec);
                }
                Status::Existing => {
                    self.matched.insert(p.uid);
                    // The item's name before the edit (a hint may have
                    // moved it).
                    let orig = self
                        .before
                        .binding
                        .keys
                        .get(&p.uid)
                        .cloned()
                        .unwrap_or_default();
                    let before = before_of(&orig);
                    self.diff(p.uid, before.as_ref(), &rec);
                }
            }
        }
    }

    /// Draws ids for every named item the edit adds, so references to
    /// them (a formula naming a new global) bind before anything is
    /// written.
    fn reserve_named(&mut self, after: &Budget) {
        let names: Vec<(Kind, String)> = after
            .units
            .keys()
            .map(|n| (Kind::Unit, n.clone()))
            .chain(after.currencies.keys().map(|n| (Kind::Currency, n.clone())))
            .chain(after.info.keys().map(|n| (Kind::Info, n.clone())))
            .chain(after.globals.keys().map(|n| (Kind::Global, n.clone())))
            .chain(after.fringes.keys().map(|n| (Kind::Fringe, n.clone())))
            .chain(after.groups.keys().map(|n| (Kind::Group, n.clone())))
            .chain(after.sets.keys().map(|n| (Kind::Set, n.clone())))
            .chain(after.locations.keys().map(|n| (Kind::Location, n.clone())))
            .chain(after.charges.iter().map(|c| (Kind::Charge, c.name.clone())))
            .chain(after.credits.iter().map(|c| (Kind::Credit, c.name.clone())))
            .collect();
        for (kind, name) in names {
            self.plan_named(kind, &name);
        }
    }

    fn write_setup(&mut self, after: &Budget) {
        let before = &self.before.budget;
        let bb = &self.before.binding;
        let units: Vec<_> = after
            .units
            .iter()
            .map(|(n, u)| (n.clone(), schema::encode_unit(n, u)))
            .collect();
        self.write_map(Kind::Unit, units, &|n| {
            before.units.get(n).map(|u| schema::encode_unit(n, u))
        });
        let currencies: Vec<_> = after
            .currencies
            .iter()
            .map(|(n, c)| (n.clone(), schema::encode_currency(n, c)))
            .collect();
        self.write_map(Kind::Currency, currencies, &|n| {
            before
                .currencies
                .get(n)
                .map(|c| schema::encode_currency(n, c))
        });
        let info: Vec<_> = after
            .info
            .iter()
            .map(|(n, v)| (n.clone(), schema::encode_info(n, v)))
            .collect();
        self.write_map(Kind::Info, info, &|n| {
            before.info.get(n).map(|v| schema::encode_info(n, v))
        });
        let default_info = crate::model::GlobalInfo::default();
        let globals: Vec<_> = after
            .globals
            .iter()
            .map(|(n, f)| {
                let info = after.global_info.get(n).unwrap_or(&default_info);
                (n.clone(), schema::encode_global(n, f, info, &self.local))
            })
            .collect();
        self.write_map(Kind::Global, globals, &|n| {
            before.globals.get(n).map(|f| {
                let info = before.global_info.get(n).unwrap_or(&default_info);
                schema::encode_global(n, f, info, bb)
            })
        });
        let fringes: Vec<_> = after
            .fringes
            .iter()
            .map(|(n, f)| (n.clone(), schema::encode_fringe(n, f, &self.local)))
            .collect();
        self.write_map(Kind::Fringe, fringes, &|n| {
            before
                .fringes
                .get(n)
                .map(|f| schema::encode_fringe(n, f, bb))
        });
        let groups: Vec<_> = after
            .groups
            .iter()
            .map(|(n, g)| (n.clone(), schema::encode_group(n, g)))
            .collect();
        self.write_map(Kind::Group, groups, &|n| {
            before.groups.get(n).map(|g| schema::encode_group(n, g))
        });
        let sets: Vec<_> = after
            .sets
            .iter()
            .map(|(n, s)| (n.clone(), schema::encode_set(n, s)))
            .collect();
        self.write_map(Kind::Set, sets, &|n| {
            before.sets.get(n).map(|s| schema::encode_set(n, s))
        });
        let locations: Vec<_> = after
            .locations
            .iter()
            .map(|(n, l)| (n.clone(), schema::encode_location(n, l)))
            .collect();
        self.write_map(Kind::Location, locations, &|n| {
            before
                .locations
                .get(n)
                .map(|l| schema::encode_location(n, l))
        });
    }

    fn write_charges(&mut self, after: &Budget) {
        let key = ListKey::root(ListName::Charges);
        let plans: Vec<Planned> = after
            .charges
            .iter()
            .map(|c| self.plan_named(Kind::Charge, &c.name))
            .collect();
        let stable = self.stable(key, &plans);
        let mut left = None;
        for (p, c) in plans.iter().zip(&after.charges) {
            let before = self
                .ix
                .charges
                .get(&p.uid)
                .map(|x| schema::encode_charge(x, &self.before.binding));
            let rec = schema::encode_charge(c, &self.local);
            self.member(
                key,
                *p,
                Kind::Charge,
                before,
                rec,
                stable.contains(&p.uid),
                left,
            );
            left = Some(p.uid);
            // Excluded amounts: matched by value, in order.
            let xkey = ListKey::new(p.uid, ListName::ExcludedAmounts);
            let before_x: Vec<Uid> = self
                .before
                .binding
                .lists
                .get(&xkey)
                .cloned()
                .unwrap_or_default();
            let mut claimed = BTreeSet::new();
            let xplans: Vec<Planned> = c
                .excluded_amounts
                .iter()
                .map(|x| {
                    match before_x
                        .iter()
                        .find(|u| !claimed.contains(*u) && self.ix.excluded.get(*u) == Some(x))
                    {
                        Some(u) => {
                            claimed.insert(*u);
                            Planned {
                                uid: *u,
                                status: Status::Existing,
                            }
                        }
                        None => Planned {
                            uid: self.fresh(),
                            status: Status::New,
                        },
                    }
                })
                .collect();
            let xstable = self.stable(xkey, &xplans);
            let mut xleft = None;
            for (xp, (desc, amt)) in xplans.iter().zip(&c.excluded_amounts) {
                let before = self
                    .ix
                    .excluded
                    .get(&xp.uid)
                    .map(|(d, a)| schema::encode_excluded_amount(d, *a));
                self.member(
                    xkey,
                    *xp,
                    Kind::ExcludedAmount,
                    before,
                    schema::encode_excluded_amount(desc, *amt),
                    xstable.contains(&xp.uid),
                    xleft,
                );
                xleft = Some(xp.uid);
            }
        }
    }

    fn write_credits(&mut self, after: &Budget) {
        let key = ListKey::root(ListName::Credits);
        let plans: Vec<Planned> = after
            .credits
            .iter()
            .map(|c| self.plan_named(Kind::Credit, &c.name))
            .collect();
        let stable = self.stable(key, &plans);
        let mut left = None;
        for (p, c) in plans.iter().zip(&after.credits) {
            let before = self
                .ix
                .credits
                .get(&p.uid)
                .map(|x| schema::encode_credit(x, &self.before.binding));
            let rec = schema::encode_credit(c, &self.local);
            self.member(
                key,
                *p,
                Kind::Credit,
                before,
                rec,
                stable.contains(&p.uid),
                left,
            );
            left = Some(p.uid);
            // Credit rows: matched by description and source, in order.
            let rkey = ListKey::new(p.uid, ListName::CreditRows);
            let before_r: Vec<Uid> = self
                .before
                .binding
                .lists
                .get(&rkey)
                .cloned()
                .unwrap_or_default();
            let mut claimed = BTreeSet::new();
            let rplans: Vec<Planned> = c
                .rows
                .iter()
                .map(|row| {
                    let same = |u: &Uid| {
                        self.ix.credit_rows.get(u).is_some_and(|x| {
                            x.description == row.description && x.source == row.source
                        })
                    };
                    match before_r.iter().find(|u| !claimed.contains(*u) && same(u)) {
                        Some(u) => {
                            claimed.insert(*u);
                            Planned {
                                uid: *u,
                                status: Status::Existing,
                            }
                        }
                        None => Planned {
                            uid: self.fresh(),
                            status: Status::New,
                        },
                    }
                })
                .collect();
            let rstable = self.stable(rkey, &rplans);
            let mut rleft = None;
            for (rp, row) in rplans.iter().zip(&c.rows) {
                let before = self
                    .ix
                    .credit_rows
                    .get(&rp.uid)
                    .map(|x| schema::encode_credit_row(x, &self.before.binding));
                let rec = schema::encode_credit_row(row, &self.local);
                self.member(
                    rkey,
                    *rp,
                    Kind::CreditRow,
                    before,
                    rec,
                    rstable.contains(&rp.uid),
                    rleft,
                );
                rleft = Some(rp.uid);
            }
        }
    }

    fn write_versions(&mut self, after: &Budget) {
        let key = ListKey::root(ListName::Versions);
        let mut claimed = BTreeSet::new();
        let plans: Vec<Planned> = after
            .versions
            .iter()
            .map(|v| {
                let live = self
                    .before
                    .binding
                    .versions
                    .get(&v.id)
                    .copied()
                    .filter(|u| claimed.insert(*u));
                let p = self.plan_by_id(Kind::Version, live, &v.id.to_string());
                claimed.insert(p.uid);
                p
            })
            .collect();
        let stable = self.stable(key, &plans);
        let mut left = None;
        for (p, v) in plans.iter().zip(&after.versions) {
            let before = self
                .ix
                .versions
                .get(&p.uid)
                .map(|x| schema::encode_version(x));
            self.member(
                key,
                *p,
                Kind::Version,
                before,
                schema::encode_version(v),
                stable.contains(&p.uid),
                left,
            );
            left = Some(p.uid);
        }
    }

    fn write_actuals(&mut self, after: &Budget) {
        let actuals = &after.actuals;
        // Batches before transactions, so a transaction's batch binds.
        let bkey = ListKey::root(ListName::Batches);
        let mut claimed = BTreeSet::new();
        let bplans: Vec<Planned> = actuals
            .batches
            .iter()
            .map(|b| {
                let live = self
                    .before
                    .binding
                    .batches
                    .get(&b.id)
                    .copied()
                    .filter(|u| claimed.insert(*u));
                let p = self.plan_by_id(Kind::Batch, live, &b.id.to_string());
                claimed.insert(p.uid);
                if p.status != Status::Existing {
                    self.local.batches.insert(b.id, p.uid);
                }
                p
            })
            .collect();
        let bstable = self.stable(bkey, &bplans);
        let mut left = None;
        for (p, b) in bplans.iter().zip(&actuals.batches) {
            let before = self.ix.batches.get(&p.uid).map(|x| schema::encode_batch(x));
            self.member(
                bkey,
                *p,
                Kind::Batch,
                before,
                schema::encode_batch(b),
                bstable.contains(&p.uid),
                left,
            );
            left = Some(p.uid);
        }
        let tkey = ListKey::root(ListName::Transactions);
        let mut claimed = BTreeSet::new();
        let tplans: Vec<Planned> = actuals
            .transactions
            .iter()
            .map(|t| {
                let live = self
                    .before
                    .binding
                    .transactions
                    .get(&t.id)
                    .copied()
                    .filter(|u| claimed.insert(*u));
                let p = self.plan_by_id(Kind::Transaction, live, &t.id.to_string());
                claimed.insert(p.uid);
                p
            })
            .collect();
        let tstable = self.stable(tkey, &tplans);
        let mut left = None;
        for (p, t) in tplans.iter().zip(&actuals.transactions) {
            let before = self
                .ix
                .transactions
                .get(&p.uid)
                .map(|x| schema::encode_transaction(x, &self.before.binding));
            let rec = schema::encode_transaction(t, &self.local);
            self.member(
                tkey,
                *p,
                Kind::Transaction,
                before,
                rec,
                tstable.contains(&p.uid),
                left,
            );
            left = Some(p.uid);
        }
        // ETC overrides: a map by account, matched through the account
        // entity so a renumbered account keeps its override.
        let before_etc = &self.before.budget.actuals.etc;
        let before_by_account: Vec<(Value, Uid)> = self
            .before
            .binding
            .etc
            .values()
            .filter_map(|uid| {
                self.state
                    .entity(*uid)
                    .and_then(|e| e.get(Field::Account))
                    .map(|v| (v.clone(), *uid))
            })
            .collect();
        for (code, method) in &actuals.etc {
            let rec = schema::encode_etc(code, *method, &self.local);
            let account = rec
                .fields
                .get(&Field::Account)
                .cloned()
                .unwrap_or(Value::Null);
            let p = match before_by_account
                .iter()
                .find(|(v, _)| *v == account)
                .map(|(_, u)| u)
            {
                Some(uid) => Planned {
                    uid: *uid,
                    status: Status::Existing,
                },
                None => self.plan_by_id(Kind::Etc, None, code),
            };
            // The before record, under the account's old code.
            let before_code = self
                .before
                .binding
                .etc
                .iter()
                .find(|(_, u)| **u == p.uid)
                .map(|(c, _)| c.clone());
            match p.status {
                Status::New => self.ops.push(Op::Create {
                    entity: p.uid,
                    kind: Kind::Etc,
                    place: None,
                    fields: rec.fields,
                    sets: rec.sets,
                }),
                Status::Restore => {
                    self.ops.push(Op::Restore { entity: p.uid });
                    let current = self.state.entity(p.uid).map(Record::of);
                    self.diff(p.uid, current.as_ref(), &rec);
                }
                Status::Existing => {
                    self.matched.insert(p.uid);
                    let before = before_code.and_then(|c| {
                        before_etc
                            .get(&c)
                            .map(|m| schema::encode_etc(&c, *m, &self.before.binding))
                    });
                    self.diff(p.uid, before.as_ref(), &rec);
                }
            }
        }
    }

    fn write_root(&mut self, after: &Budget) {
        let before = schema::encode_root(&self.before.budget);
        let rec = schema::encode_root(after);
        let root = self.state.root();
        for (f, v) in &rec.fields {
            if before.fields.get(f) != Some(v) || root.get(*f).is_none() {
                self.ops.push(Op::Set {
                    entity: Uid::ROOT,
                    field: *f,
                    value: v.clone(),
                });
            }
        }
    }

    fn write_deletes(&mut self) {
        let gone: Vec<Uid> = self
            .before
            .binding
            .shown
            .iter()
            .copied()
            .filter(|u| !self.matched.contains(u))
            .collect();
        for uid in gone {
            self.ops.push(Op::Delete { entity: uid });
        }
    }
}

/// The operations that take the budget of `before` (the projection of
/// `state`) to `after`. Empty when nothing changed. The ops are the body
/// of a change by `actor` with the given sequence number and Lamport
/// clock; `ids` draws the ids of new entities.
#[allow(clippy::too_many_arguments)]
pub fn capture(
    state: &State,
    before: &Projection,
    after: &Budget,
    hints: &[Hint],
    ids: &mut Ids,
    actor: ActorId,
    seq: u64,
    lamport: u64,
) -> Vec<Op> {
    let mut local = before.binding.clone();
    for h in hints {
        if let Hint::Rename { kind, from, to } = h {
            if let Some(uid) = local.names.remove(&(*kind, from.clone())) {
                local.names.insert((*kind, to.clone()), uid);
            }
        }
    }
    let ix = BeforeIndex::build(before);
    let mut c = Capture {
        state,
        before,
        ix,
        local,
        ids,
        lamport,
        actor,
        seq,
        ops: Vec::new(),
        trees: BTreeMap::new(),
        placed: BTreeMap::new(),
        matched: BTreeSet::new(),
        restoring: BTreeSet::new(),
    };
    c.reserve_named(after);
    let plans = c.plan_topsheet(after, hints);
    c.write_setup(after);
    c.write_topsheet(&plans);
    c.write_charges(after);
    c.write_credits(after);
    c.write_versions(after);
    c.write_actuals(after);
    c.write_root(after);
    c.write_deletes();
    c.ops
}
