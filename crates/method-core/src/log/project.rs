//! The projection: a [`Budget`] from the folded state, the same on every
//! computer, with the invariant repairs of docs/research/collaboration.md
//! 3.10 made on the way out (never written back) and listed for review.
//!
//! The projection also returns a [`Binding`]: how the budget's names,
//! codes and ids map to entity ids, which the capture needs to turn the
//! next edit into operations.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::clock::Stamp;
use super::formula;
use super::ids::{derived_row_id, Uid};
use super::op::{Field, Kind, ListKey, ListName, Member, Value};
use super::review::{Review, ReviewItem};
use super::schema::{self, Binder, Record, Resolver};
use super::state::{Entity, State};
use crate::actuals::{Actuals, BatchId};
use crate::model::{Budget, NoteId, RowId, TopsheetRow, VersionId};

/// How the projected budget's names, codes and ids map to entities.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Binding {
    /// Live named entities by kind and projected name.
    pub names: BTreeMap<(Kind, String), Uid>,
    /// Live accounts by code (the earliest created, when two share one).
    pub accounts: BTreeMap<String, Uid>,
    pub categories: BTreeMap<String, Uid>,
    /// Live rows by projected id.
    pub rows: BTreeMap<RowId, Uid>,
    /// Live notes by (owner, projected id).
    pub notes: BTreeMap<(Uid, NoteId), Uid>,
    pub versions: BTreeMap<VersionId, Uid>,
    pub transactions: BTreeMap<u64, Uid>,
    pub batches: BTreeMap<BatchId, Uid>,
    /// ETC overrides by projected account code.
    pub etc: BTreeMap<String, Uid>,
    /// Every list's projected order.
    pub lists: BTreeMap<ListKey, Vec<Uid>>,
    /// The name, code or id every live or revived entity shows with
    /// (unified duplicates map to the kept item's name).
    pub keys: BTreeMap<Uid, String>,
    pub row_ids: BTreeMap<Uid, RowId>,
    pub batch_ids: BTreeMap<Uid, BatchId>,
    /// Entities the budget shows (live, not unified).
    pub shown: BTreeSet<Uid>,
    /// Deleted entities by kind and key, for bringing one back by the same
    /// name or id.
    pub dead: BTreeMap<(Kind, String), Uid>,
}

impl Binder for Binding {
    fn named(&self, kind: Kind, name: &str) -> Option<Uid> {
        self.names.get(&(kind, name.to_string())).copied()
    }

    fn account(&self, code: &str) -> Option<Uid> {
        self.accounts.get(code).copied()
    }

    fn category(&self, code: &str) -> Option<Uid> {
        self.categories.get(code).copied()
    }

    fn row(&self, id: RowId) -> Option<Uid> {
        self.rows.get(&id).copied()
    }

    fn batch(&self, id: BatchId) -> Option<Uid> {
        self.batches.get(&id).copied()
    }
}

impl Resolver for Binding {
    fn name(&self, uid: Uid) -> Option<String> {
        self.keys.get(&uid).cloned()
    }

    fn row_id(&self, uid: Uid) -> Option<RowId> {
        self.row_ids.get(&uid).copied()
    }

    fn batch_id(&self, uid: Uid) -> Option<BatchId> {
        self.batch_ids.get(&uid).copied()
    }
}

impl Binding {
    /// The key a deleted entity is found back by.
    pub fn dead_key(kind: Kind, key: &str) -> (Kind, String) {
        (kind, key.to_string())
    }

    /// The key of a note: its owner and id.
    pub fn note_key(owner: Uid, id: NoteId) -> String {
        format!("{}:{id}", owner.hex())
    }
}

/// A projected budget with its binding and what to review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Projection {
    pub budget: Budget,
    pub binding: Binding,
    pub review: Vec<ReviewItem>,
}

impl Projection {
    /// The projection of nothing: an empty budget, no entities. What the
    /// first capture (adopting an existing budget) compares against.
    pub fn empty() -> Projection {
        let state = State::new();
        project(&state)
    }
}

/// Which ids an entity uses so strongly that a deleted target must come
/// back (docs/research/collaboration.md, 3.10: "use revives").
fn strong_refs(e: &Entity) -> Vec<Uid> {
    let mut out = Vec::new();
    let mut reg = |f: Field| {
        if let Some(Value::Ref(u)) = e.get(f) {
            out.push(*u);
        }
    };
    let formulas = |fields: &[Field], out: &mut Vec<Uid>| {
        for f in fields {
            if let Some(Value::Formula(text)) = e.get(*f) {
                out.extend(formula::refs(text));
            }
        }
    };
    let sets = |fields: &[Field], out: &mut Vec<Uid>| {
        for f in fields {
            for m in e.set_members(*f) {
                if let Member::Ref(u) = m {
                    out.push(*u);
                }
            }
        }
    };
    match e.kind {
        Kind::Row => {
            for f in [
                Field::Unit,
                Field::Unit2,
                Field::Unit3,
                Field::Unit4,
                Field::Currency,
                Field::Location,
                Field::Set,
            ] {
                reg(f);
            }
            formulas(&[Field::Amount, Field::X, Field::Rate, Field::X4], &mut out);
            sets(&[Field::Fringes, Field::Groups], &mut out);
        }
        Kind::Global => {
            reg(Field::Unit);
            formulas(&[Field::Formula], &mut out);
        }
        Kind::Fringe => {
            reg(Field::Unit);
            formulas(&[Field::Rate], &mut out);
        }
        Kind::Charge => {
            reg(Field::Currency);
            sets(&[Field::ExcludedGroups], &mut out);
        }
        Kind::Credit => sets(&[Field::ExcludedFringes, Field::Locations], &mut out),
        Kind::CreditRow => reg(Field::Group),
        _ => {}
    }
    out
}

/// A free name: `name`, or `name 2`, `name 3`... not in `taken`.
fn free_name(name: &str, taken: &BTreeSet<String>) -> String {
    if !taken.contains(name) {
        return name.to_string();
    }
    (2..)
        .map(|n| format!("{name} {n}"))
        .find(|c| !taken.contains(c))
        .expect("some suffix is free")
}

/// A free id: `id` if unused, else one past the highest used.
fn free_id(id: u64, used: &BTreeSet<u64>) -> u64 {
    if !used.contains(&id) {
        return id;
    }
    used.iter().next_back().map_or(1, |m| m.saturating_add(1))
}

/// Projects `state` into a budget.
pub fn project(state: &State) -> Projection {
    let mut review: Vec<Review> = Vec::new();
    let mut binding = Binding::default();

    // 1. Who is live: not deleted, or deleted but still used or still
    //    holding something live.
    let mut live: BTreeSet<Uid> = BTreeSet::new();
    let mut queue: Vec<Uid> = Vec::new();
    for (uid, e) in state.entities() {
        if *uid != Uid::ROOT && !e.is_deleted() {
            live.insert(*uid);
            queue.push(*uid);
        }
    }
    while let Some(uid) = queue.pop() {
        let Some(e) = state.entity(uid) else { continue };
        if let Some(p) = e.place() {
            let parent = p.list.parent;
            if parent != Uid::ROOT && state.entity(parent).is_some() && !live.contains(&parent) {
                live.insert(parent);
                queue.push(parent);
                review.push(Review::Revived {
                    item: parent,
                    used_by: uid,
                });
            }
        }
        for target in strong_refs(e) {
            if target != Uid::ROOT && state.entity(target).is_some() && !live.contains(&target) {
                live.insert(target);
                queue.push(target);
                review.push(Review::Revived {
                    item: target,
                    used_by: uid,
                });
            }
        }
    }

    // 2. Names: unique per named kind; identical duplicates unify.
    let mut shown: BTreeSet<Uid> = live.clone();
    for kind in Kind::ALL.into_iter().filter(|k| k.is_named()) {
        let mut groups: BTreeMap<String, Vec<(Stamp, Uid)>> = BTreeMap::new();
        for uid in &live {
            let e = state.entity(*uid).expect("live entities exist");
            if e.kind == kind {
                let name = e
                    .get(Field::Name)
                    .and_then(Value::as_text)
                    .unwrap_or_default()
                    .to_string();
                groups.entry(name).or_default().push((e.created, *uid));
            }
        }
        let mut taken: BTreeSet<String> = groups.keys().cloned().collect();
        for (name, mut members) in groups {
            members.sort();
            let (_, first) = members[0];
            binding.keys.insert(first, name.clone());
            binding.names.insert((kind, name.clone()), first);
            let first_record = Record::of(state.entity(first).expect("live")).without_name();
            for (_, uid) in &members[1..] {
                let record = Record::of(state.entity(*uid).expect("live")).without_name();
                if record == first_record {
                    shown.remove(uid);
                    binding.keys.insert(*uid, name.clone());
                } else {
                    let now = free_name(&name, &taken);
                    taken.insert(now.clone());
                    binding.keys.insert(*uid, now.clone());
                    binding.names.insert((kind, now.clone()), *uid);
                    review.push(Review::DuplicateName {
                        kind,
                        kept: first,
                        renamed: *uid,
                        name: name.clone(),
                        now,
                    });
                }
            }
        }
    }

    // 3. Codes: kept as written, flagged when repeated or misfit.
    let coding = match state.root().get(Field::AccountCoding) {
        Some(Value::Coding(c)) => *c,
        _ => Default::default(),
    };
    for kind in [Kind::Category, Kind::Account] {
        let mut groups: BTreeMap<String, Vec<(Stamp, Uid)>> = BTreeMap::new();
        for uid in &live {
            let e = state.entity(*uid).expect("live");
            if e.kind == kind {
                let code = e
                    .get(Field::Code)
                    .and_then(Value::as_text)
                    .unwrap_or_default()
                    .to_string();
                groups.entry(code).or_default().push((e.created, *uid));
            }
        }
        for (code, mut members) in groups {
            members.sort();
            for (_, uid) in &members {
                binding.keys.insert(*uid, code.clone());
            }
            let first = members[0].1;
            match kind {
                Kind::Account => {
                    binding.accounts.insert(code.clone(), first);
                    if let Err(e) = coding.parse(&code) {
                        for (_, uid) in &members {
                            review.push(Review::BadCode {
                                account: *uid,
                                code: code.clone(),
                                reason: e.to_string(),
                            });
                        }
                    }
                }
                _ => {
                    binding.categories.insert(code.clone(), first);
                }
            }
            if members.len() > 1 {
                review.push(Review::DuplicateCode {
                    kind,
                    code,
                    entities: members.iter().map(|(_, u)| *u).collect(),
                });
            }
        }
    }

    // 4. Row ids: unique; the later of two shows with a derived id.
    {
        let mut groups: BTreeMap<RowId, Vec<(Stamp, Uid)>> = BTreeMap::new();
        for uid in &live {
            let e = state.entity(*uid).expect("live");
            if e.kind == Kind::Row {
                groups
                    .entry(schema::record_id(e).unwrap_or(0))
                    .or_default()
                    .push((e.created, *uid));
            }
        }
        let mut used: BTreeSet<RowId> = groups.keys().copied().collect();
        for (id, mut members) in groups {
            members.sort();
            let (_, first) = members[0];
            binding.rows.insert(id, first);
            binding.row_ids.insert(first, id);
            binding.keys.insert(first, id.to_string());
            for (created, uid) in &members[1..] {
                let mut now = derived_row_id(created.digest());
                while used.contains(&now) {
                    now = if now + 1 < super::ids::SHARED_ROW_ID_END {
                        now + 1
                    } else {
                        super::ids::SHARED_ROW_ID_MIN
                    };
                }
                used.insert(now);
                binding.rows.insert(now, *uid);
                binding.row_ids.insert(*uid, now);
                binding.keys.insert(*uid, now.to_string());
                review.push(Review::DuplicateRowId {
                    row: *uid,
                    was: id,
                    now,
                });
            }
        }
    }

    // 5. Lists, in order.
    let mut by_list: BTreeMap<ListKey, Vec<(Stamp, Uid)>> = BTreeMap::new();
    for uid in &shown {
        let e = state.entity(*uid).expect("shown");
        if let Some(p) = e.place() {
            by_list.entry(p.list).or_default().push((p.pos, *uid));
        }
    }
    for (key, members) in by_list {
        let mut at: BTreeMap<Stamp, Vec<Uid>> = BTreeMap::new();
        for (pos, uid) in members {
            at.entry(pos).or_default().push(uid);
        }
        let mut order = Vec::new();
        if let Some(tree) = state.list(&key) {
            for pos in tree.order() {
                if let Some(uids) = at.remove(&pos) {
                    order.extend(uids);
                }
            }
        }
        for (_, uids) in at {
            order.extend(uids);
        }
        binding.lists.insert(key, order);
    }

    // 6. Ids unique within their lists (notes, versions, transactions,
    //    batches): silently, they are not user-facing.
    let list_ids = |binding: &Binding, key: &ListKey| -> Vec<(Uid, u64)> {
        let mut used = BTreeSet::new();
        let mut out = Vec::new();
        for uid in binding.lists.get(key).map(Vec::as_slice).unwrap_or(&[]) {
            let e = state.entity(*uid).expect("shown");
            let id = free_id(schema::record_id(e).unwrap_or(1).max(1), &used);
            used.insert(id);
            out.push((*uid, id));
        }
        out
    };
    let note_lists: Vec<ListKey> = binding
        .lists
        .keys()
        .filter(|k| k.name == ListName::Notes)
        .copied()
        .collect();
    for key in note_lists {
        for (uid, id) in list_ids(&binding, &key) {
            binding.notes.insert((key.parent, id), uid);
            binding.keys.insert(uid, id.to_string());
        }
    }
    for (uid, id) in list_ids(&binding, &ListKey::root(ListName::Versions)) {
        binding.versions.insert(id, uid);
        binding.keys.insert(uid, id.to_string());
    }
    for (uid, id) in list_ids(&binding, &ListKey::root(ListName::Transactions)) {
        binding.transactions.insert(id, uid);
        binding.keys.insert(uid, id.to_string());
    }
    for (uid, id) in list_ids(&binding, &ListKey::root(ListName::Batches)) {
        binding.batches.insert(id, uid);
        binding.batch_ids.insert(uid, id);
        binding.keys.insert(uid, id.to_string());
    }
    // Production totals, excluded amounts and credit rows are known by
    // their place only; their key is their name or description.
    for uid in &shown {
        let e = state.entity(*uid).expect("shown");
        match e.kind {
            Kind::ProductionTotal => {
                binding.keys.insert(
                    *uid,
                    e.get(Field::Name)
                        .and_then(Value::as_text)
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            Kind::ExcludedAmount | Kind::CreditRow => {
                binding.keys.insert(
                    *uid,
                    e.get(Field::Description)
                        .and_then(Value::as_text)
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            _ => {}
        }
    }
    binding.shown = shown.clone();

    // 7. The budget.
    let mut b = Budget::new("", "");
    b.units.clear();
    schema::decode_root(state.root(), &mut b);
    let r = &binding;
    for uid in &shown {
        let e = state.entity(*uid).expect("shown");
        let name = binding.keys.get(uid).cloned().unwrap_or_default();
        match e.kind {
            Kind::Info => {
                b.info.insert(name, schema::decode_info(e));
            }
            Kind::Currency => {
                b.currencies.insert(name, schema::decode_currency(e));
            }
            Kind::Global => {
                let (f, info) = schema::decode_global(e, r);
                b.globals.insert(name.clone(), f);
                if !info.is_default() {
                    b.global_info.insert(name, info);
                }
            }
            Kind::Unit => {
                b.units.insert(name, schema::decode_unit(e));
            }
            Kind::Fringe => {
                b.fringes.insert(name, schema::decode_fringe(e, r));
            }
            Kind::Group => {
                b.groups.insert(name, schema::decode_group(e));
            }
            Kind::Set => {
                b.sets.insert(name, schema::decode_set(e));
            }
            Kind::Location => {
                b.locations.insert(name, schema::decode_location(e));
            }
            _ => {}
        }
    }
    let list =
        |key: ListKey| -> &[Uid] { binding.lists.get(&key).map(Vec::as_slice).unwrap_or(&[]) };
    let notes_of = |owner: Uid| -> Vec<crate::model::Note> {
        list(ListKey::new(owner, ListName::Notes))
            .iter()
            .map(|uid| {
                let e = state.entity(*uid).expect("shown");
                let id = binding.keys[uid].parse().unwrap_or(1);
                schema::decode_note(e, id)
            })
            .collect()
    };
    for uid in list(ListKey::root(ListName::Topsheet)) {
        let e = state.entity(*uid).expect("shown");
        match e.kind {
            Kind::Category => {
                let mut c = schema::decode_category(e);
                c.notes = notes_of(*uid);
                for auid in list(ListKey::new(*uid, ListName::Accounts)) {
                    let ae = state.entity(*auid).expect("shown");
                    let mut a = schema::decode_account(ae);
                    a.notes = notes_of(*auid);
                    for ruid in list(ListKey::new(*auid, ListName::Rows)) {
                        let re = state.entity(*ruid).expect("shown");
                        let id = binding.row_ids.get(ruid).copied().unwrap_or(0);
                        let mut d = schema::decode_row(re, r, id);
                        d.notes = notes_of(*ruid);
                        a.details.push(d);
                    }
                    c.accounts.push(a);
                }
                b.topsheet.push(TopsheetRow::Category(c));
            }
            Kind::ProductionTotal => {
                b.topsheet.push(TopsheetRow::ProductionTotal(
                    schema::decode_production_total(e),
                ));
            }
            _ => {}
        }
    }
    for uid in list(ListKey::root(ListName::Charges)) {
        let e = state.entity(*uid).expect("shown");
        let mut c = schema::decode_charge(e, r);
        for xuid in list(ListKey::new(*uid, ListName::ExcludedAmounts)) {
            c.excluded_amounts.push(schema::decode_excluded_amount(
                state.entity(*xuid).expect("shown"),
            ));
        }
        b.charges.push(c);
    }
    for uid in list(ListKey::root(ListName::Credits)) {
        let e = state.entity(*uid).expect("shown");
        let mut c = schema::decode_credit(e, r);
        for ruid in list(ListKey::new(*uid, ListName::CreditRows)) {
            c.rows.push(schema::decode_credit_row(
                state.entity(*ruid).expect("shown"),
                r,
            ));
        }
        b.credits.push(c);
    }
    for uid in list(ListKey::root(ListName::Versions)) {
        let e = state.entity(*uid).expect("shown");
        let id = binding.keys[uid].parse().unwrap_or(1);
        b.versions.push(schema::decode_version(e, id));
    }
    let mut actuals = Actuals::default();
    for uid in list(ListKey::root(ListName::Transactions)) {
        let e = state.entity(*uid).expect("shown");
        let id = binding.keys[uid].parse().unwrap_or(1);
        actuals
            .transactions
            .push(Arc::new(schema::decode_transaction(e, r, id)));
    }
    for uid in list(ListKey::root(ListName::Batches)) {
        let e = state.entity(*uid).expect("shown");
        let id = binding.batch_ids.get(uid).copied().unwrap_or(1);
        actuals.batches.push(schema::decode_batch(e, id));
    }
    let mut etc_uids = Vec::new();
    for uid in &shown {
        let e = state.entity(*uid).expect("shown");
        if e.kind == Kind::Etc {
            let (code, method) = schema::decode_etc(e, r);
            etc_uids.push((code.clone(), *uid));
            actuals.etc.entry(code).or_insert(method);
        }
    }

    if !actuals.is_empty() {
        b.actuals = Arc::new(actuals);
    }

    // 8. Deleted entities, by key, for restores.
    let mut dead: BTreeMap<(Kind, String), Uid> = BTreeMap::new();
    for (uid, e) in state.entities() {
        if *uid == Uid::ROOT || live.contains(uid) {
            continue;
        }
        let key = match e.kind {
            k if k.is_named() => e
                .get(Field::Name)
                .and_then(Value::as_text)
                .map(str::to_string),
            Kind::Row | Kind::Version | Kind::Transaction | Kind::Batch => {
                schema::record_id(e).map(|id| id.to_string())
            }
            Kind::Note => e
                .place()
                .and_then(|p| schema::record_id(e).map(|id| Binding::note_key(p.list.parent, id))),
            Kind::Account | Kind::Category => e
                .get(Field::Code)
                .and_then(Value::as_text)
                .map(str::to_string),
            Kind::Etc => Some(schema::decode_etc(e, r).0),
            _ => None,
        };
        if let Some(key) = key {
            let slot = dead.entry((e.kind, key)).or_insert(*uid);
            let have = state
                .entity(*slot)
                .map(|x| x.created)
                .unwrap_or(Stamp::ZERO);
            if e.created > have {
                *slot = *uid;
            }
        }
    }
    binding.dead = dead;
    for (code, uid) in etc_uids {
        binding.etc.entry(code).or_insert(uid);
    }

    // 9. Cycles are reported, not repaired.
    let cycles = crate::edit::circular_globals(&b);
    if !cycles.is_empty() {
        review.push(Review::CircularGlobals {
            names: cycles.into_iter().collect(),
        });
    }

    let items = review
        .into_iter()
        .map(|rv| {
            let sentence = sentence_for(state, &binding, &rv);
            ReviewItem {
                review: rv,
                sentence,
            }
        })
        .collect();
    Projection {
        budget: b,
        binding,
        review: items,
    }
}

/// What an entity is called in sentences: "line 1001 in 21-01", "fringe
/// FICA", "account 21-01"...
pub fn describe(state: &State, binding: &Binding, uid: Uid) -> String {
    let Some(e) = state.entity(uid) else {
        return "something that no longer exists".into();
    };
    let key = binding
        .keys
        .get(&uid)
        .cloned()
        .or_else(|| {
            e.get(Field::Name)
                .or(e.get(Field::Code))
                .or(e.get(Field::Id))
                .map(Value::brief)
        })
        .unwrap_or_default();
    let owner = || {
        e.place()
            .map(|p| p.list.parent)
            .filter(|p| *p != Uid::ROOT)
            .and_then(|p| binding.keys.get(&p).cloned())
    };
    match e.kind {
        Kind::Root => "the budget".into(),
        Kind::Row => match owner() {
            Some(acct) => format!("line {key} in {acct}"),
            None => format!("line {key}"),
        },
        Kind::Note => match owner() {
            Some(o) => format!("a note on {o}"),
            None => "a note".into(),
        },
        Kind::ExcludedAmount => format!("excluded amount {key:?}"),
        Kind::CreditRow => match owner() {
            Some(c) => format!("credit row {key:?} of {c}"),
            None => format!("credit row {key:?}"),
        },
        Kind::Account | Kind::Category => format!("{} {key}", e.kind.word()),
        _ => format!("{} {key}", e.kind.word()),
    }
}

/// The sentence for a review item.
pub fn sentence_for(state: &State, binding: &Binding, r: &Review) -> String {
    let d = |uid: Uid| describe(state, binding, uid);
    match r {
        Review::DuplicateRowId { row, was, now } => {
            format!(
                "Two lines were given id {was}; {} now shows as line {now}.",
                d(*row)
            )
        }
        Review::DuplicateName {
            kind,
            name,
            now,
            kept: _,
            renamed: _,
        } => format!(
            "Two {}s named {name:?} were made apart and differ; the later one shows as {now:?}.",
            kind.word()
        ),
        Review::Revived { item, used_by } => {
            format!(
                "{} was deleted but {} still uses it; kept.",
                d(*item),
                d(*used_by)
            )
        }
        Review::DuplicateCode {
            kind,
            code,
            entities,
        } => format!(
            "{} {}s have the number {code}.",
            entities.len(),
            kind.word()
        ),
        Review::BadCode {
            account,
            code,
            reason,
        } => format!("{} doesn't fit the numbering ({reason}).", d(*account)).replace(
            &format!("account {code} doesn't"),
            &format!("Account {code} doesn't"),
        ),
        Review::CircularGlobals { names } => {
            format!("Globals refer back to themselves: {}.", names.join(", "))
        }
        Review::Overwritten {
            entity,
            field,
            kept,
            lost,
        } => format!(
            "{} {} was changed by two people: {} ({}) kept, {} overwritten.",
            d(*entity),
            field.name(),
            kept.1.brief(),
            kept.0.actor.short(),
            lost.iter()
                .map(|(s, v)| format!("{} ({})", v.brief(), s.actor.short()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Review::MovedTwice { entity, kept, lost } => format!(
            "{} was moved by two people; {}'s move kept, {}'s not.",
            d(*entity),
            kept.actor.short(),
            lost.iter()
                .map(|s| s.actor.short())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Review::DeletedWhileEdited {
            entity,
            deleted,
            edits,
        } => format!(
            "{} was deleted by {} while {} edited it; the delete won.",
            d(*entity),
            deleted.actor.short(),
            edits
                .iter()
                .map(|s| s.actor.short())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}
