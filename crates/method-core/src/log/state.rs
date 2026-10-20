//! The folded state: every entity with its registers, tag sets, place and
//! tombstone, plus the position trees of every list. Applying a change is
//! a pure function of the state and the change, and applying the same
//! changes in any causal order gives the same state.
//!
//! Registers keep every write they ever saw, so the state is also the
//! blame index ("who set this value, and what was it before") and what
//! per-person undo needs (docs/research/collaboration.md, section 8).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::access::{self, Role};
use super::clock::{ChangeId, Frontier, Stamp};
use super::ids::{ActorId, Uid};
use super::op::{Change, Field, Kind, ListKey, Member, Op, Placement, Value};
use super::seq::{PosId, SeqTree};
use super::sign::PublicKey;

/// A last-writer-wins register with its history. Writes are kept in stamp
/// order; the last one is the current value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Register<T> {
    writes: Vec<(Stamp, T)>,
}

impl<T: Clone> Register<T> {
    pub fn new(stamp: Stamp, value: T) -> Register<T> {
        Register {
            writes: vec![(stamp, value)],
        }
    }

    /// Records a write. A second write with the very same stamp replaces
    /// the first (it can only be the same op applied twice).
    pub fn write(&mut self, stamp: Stamp, value: T) {
        match self.writes.binary_search_by(|(s, _)| s.cmp(&stamp)) {
            Ok(i) => self.writes[i].1 = value,
            Err(i) => self.writes.insert(i, (stamp, value)),
        }
    }

    /// The winning write.
    pub fn current(&self) -> &(Stamp, T) {
        self.writes.last().expect("a register always has a write")
    }

    pub fn value(&self) -> &T {
        &self.current().1
    }

    pub fn stamp(&self) -> Stamp {
        self.current().0
    }

    /// The value that was current just before the write at `stamp`: the
    /// highest write below it.
    pub fn before(&self, stamp: Stamp) -> Option<&(Stamp, T)> {
        self.writes.iter().rev().find(|(s, _)| *s < stamp)
    }

    /// Every write, oldest first.
    pub fn history(&self) -> &[(Stamp, T)] {
        &self.writes
    }

    /// The writes that lost to the current one.
    pub fn losers(&self) -> &[(Stamp, T)] {
        &self.writes[..self.writes.len() - 1]
    }
}

/// An add-wins (observed-remove) set. Each add is a tag (its stamp); a
/// remove takes away the tags it had seen, so an add it hadn't seen keeps
/// the member in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrSet {
    live: BTreeMap<Member, BTreeSet<Stamp>>,
    removed: BTreeSet<Stamp>,
}

impl OrSet {
    pub fn add(&mut self, member: Member, tag: Stamp) {
        if self.removed.contains(&tag) {
            return;
        }
        self.live.entry(member).or_default().insert(tag);
    }

    pub fn remove(&mut self, member: &Member, seen: &[Stamp]) {
        for tag in seen {
            self.removed.insert(*tag);
        }
        if let Some(tags) = self.live.get_mut(member) {
            for tag in seen {
                tags.remove(tag);
            }
            if tags.is_empty() {
                self.live.remove(member);
            }
        }
    }

    pub fn contains(&self, member: &Member) -> bool {
        self.live.contains_key(member)
    }

    /// The members, in order.
    pub fn members(&self) -> impl Iterator<Item = &Member> {
        self.live.keys()
    }

    /// The live tags of `member`: what a remove must name.
    pub fn tags(&self, member: &Member) -> Vec<Stamp> {
        self.live
            .get(member)
            .map(|t| t.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }
}

/// Where an entity is: a list and a position in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub list: ListKey,
    pub pos: PosId,
}

/// One entity and everything written to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entity {
    pub kind: Kind,
    /// The stamp of its `create`.
    pub created: Stamp,
    pub fields: BTreeMap<Field, Register<Value>>,
    pub sets: BTreeMap<Field, OrSet>,
    /// Its move register, for kinds that live in a list.
    pub place: Option<Register<Place>>,
    /// Its tombstone: `true` is deleted.
    pub deleted: Register<bool>,
}

impl Entity {
    fn new(kind: Kind, created: Stamp) -> Entity {
        Entity {
            kind,
            created,
            fields: BTreeMap::new(),
            sets: BTreeMap::new(),
            place: None,
            deleted: Register::new(created, false),
        }
    }

    /// The current value of `field`, if it was ever written.
    pub fn get(&self, field: Field) -> Option<&Value> {
        self.fields.get(&field).map(Register::value)
    }

    /// The current members of set `field` (none when never written).
    pub fn set_members(&self, field: Field) -> Vec<&Member> {
        self.sets
            .get(&field)
            .map(|s| s.members().collect())
            .unwrap_or_default()
    }

    pub fn is_deleted(&self) -> bool {
        *self.deleted.value()
    }

    pub fn place(&self) -> Option<Place> {
        self.place.as_ref().map(|r| *r.value())
    }
}

/// One grant as the state keeps it: the role, how its granter ranked when
/// they gave it (owner 3, co-owner 2, admin 1), and what they had seen.
/// Grants made apart are settled by rank (`access::grant_at`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantWrite {
    pub role: Role,
    pub rank: u8,
    pub deps: Frontier,
    /// Written by a removal (`Op::Revoke`): the role is read only and the
    /// person's later changes are kept aside.
    pub removed: bool,
}

/// Why a change cannot be applied: it names something that doesn't
/// exist, or would put an entity where its kind can't go. A change that
/// fails is skipped whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Malformed {
    pub index: usize,
    pub reason: String,
}

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "op {}: {}", self.index, self.reason)
    }
}

/// The folded state of a log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    entities: BTreeMap<Uid, Entity>,
    lists: BTreeMap<ListKey, SeqTree>,
    /// The highest Lamport clock applied.
    lamport: u64,
    /// The budget's first change (the genesis): whoever made it started
    /// sharing and owns the budget, and every other change must have seen
    /// it. Set from the log folder's `budget.json` before anything is
    /// applied ([`State::set_genesis`]), or else the first change applied.
    genesis: Option<ChangeId>,
    /// Each person's role, every grant kept (decisions.md, entry 78).
    grants: BTreeMap<ActorId, Register<GrantWrite>>,
    /// Actors that write for a person (`Op::ActsFor`), by actor.
    links: BTreeMap<ActorId, Register<ActorId>>,
    /// Actors with a change applied that was made once roles were set
    /// (judged under roles): they can no longer say whose they are.
    judged: std::collections::BTreeSet<ActorId>,
    /// Each actor's published key and the change that published it
    /// (decisions.md, entry 89).
    keys: BTreeMap<ActorId, (PublicKey, ChangeId)>,
}

impl Default for State {
    fn default() -> State {
        State::new()
    }
}

impl State {
    /// A state with only the root entity.
    pub fn new() -> State {
        let mut entities = BTreeMap::new();
        entities.insert(Uid::ROOT, Entity::new(Kind::Root, Stamp::ZERO));
        State {
            entities,
            lists: BTreeMap::new(),
            lamport: 0,
            genesis: None,
            grants: BTreeMap::new(),
            links: BTreeMap::new(),
            judged: std::collections::BTreeSet::new(),
            keys: BTreeMap::new(),
        }
    }

    /// `actor`'s published key and the change that published it.
    pub fn key_of(&self, actor: ActorId) -> Option<(PublicKey, ChangeId)> {
        self.keys.get(&actor).copied()
    }

    /// Every published key, by actor.
    pub fn keys(&self) -> &BTreeMap<ActorId, (PublicKey, ChangeId)> {
        &self.keys
    }

    /// Who started sharing: the genesis change's actor.
    pub fn owner(&self) -> Option<ActorId> {
        self.genesis.map(|g| g.actor)
    }

    /// The budget's first change, once known.
    pub fn genesis(&self) -> Option<ChangeId> {
        self.genesis
    }

    /// Names the budget's first change (from the log folder's record of
    /// it), before anything is applied. Ignored once one is known.
    pub fn set_genesis(&mut self, genesis: ChangeId) {
        if self.genesis.is_none() {
            self.genesis = Some(genesis);
        }
    }

    /// Every role given, with every earlier one.
    pub fn grants(&self) -> &BTreeMap<ActorId, Register<GrantWrite>> {
        &self.grants
    }

    /// Whether `actor` has a change applied that was made once roles were
    /// set.
    pub fn judged(&self, actor: ActorId) -> bool {
        self.judged.contains(&actor)
    }

    /// Actors that write for a person, with every such write.
    pub fn links(&self) -> &BTreeMap<ActorId, Register<ActorId>> {
        &self.links
    }

    pub fn entity(&self, uid: Uid) -> Option<&Entity> {
        self.entities.get(&uid)
    }

    pub fn root(&self) -> &Entity {
        self.entities
            .get(&Uid::ROOT)
            .expect("the root always exists")
    }

    /// Every entity, root included, by id.
    pub fn entities(&self) -> impl Iterator<Item = (&Uid, &Entity)> {
        self.entities.iter()
    }

    /// The position tree of a list, if the list ever had a position.
    pub fn list(&self, key: &ListKey) -> Option<&SeqTree> {
        self.lists.get(key)
    }

    /// The highest Lamport clock applied so far.
    pub fn lamport(&self) -> u64 {
        self.lamport
    }

    /// Checks every op of `change` against this state (with the entities
    /// the change itself creates counted as present), without applying
    /// anything.
    pub fn check(&self, change: &Change) -> Result<(), Malformed> {
        // Every change but the first was made from the shared history, so
        // it has seen the first. One that hasn't (a second computer that
        // started sharing the same file, or a forged one) belongs to
        // another history and is refused everywhere alike.
        if let Some(g) = self.genesis {
            if change.id() != g && !change.deps.includes(g) {
                return Err(Malformed {
                    index: 0,
                    reason: "made without this budget's first change".into(),
                });
            }
        }
        let mut made: BTreeMap<Uid, Kind> = BTreeMap::new();
        let mut positions: BTreeSet<(ListKey, PosId)> = BTreeSet::new();
        let kind_of = |made: &BTreeMap<Uid, Kind>, uid: Uid| -> Option<Kind> {
            made.get(&uid)
                .copied()
                .or_else(|| self.entities.get(&uid).map(|e| e.kind))
        };
        let bad = |index: usize, reason: String| Malformed { index, reason };
        let check_place = |index: usize,
                           made: &BTreeMap<Uid, Kind>,
                           positions: &BTreeSet<(ListKey, PosId)>,
                           kind: Kind,
                           place: &Placement|
         -> Result<(), Malformed> {
            if kind.list() != Some(place.list.name) {
                return Err(bad(
                    index,
                    format!(
                        "a {} cannot be in a {} list",
                        kind.name(),
                        place.list.name.name()
                    ),
                ));
            }
            match kind_of(made, place.list.parent) {
                Some(owner) if place.list.name.owners().contains(&owner) => {}
                Some(owner) => {
                    return Err(bad(
                        index,
                        format!("a {} has no {} list", owner.name(), place.list.name.name()),
                    ))
                }
                None => return Err(bad(index, "the list's owner does not exist".into())),
            }
            let tree = self.lists.get(&place.list);
            let have = |p: &PosId| {
                tree.is_some_and(|t| t.contains(p)) || positions.contains(&(place.list, *p))
            };
            match &place.node {
                Some(node) => {
                    if have(&place.pos) {
                        return Err(bad(index, "the position already exists".into()));
                    }
                    if let Some(p) = &node.parent {
                        if !have(p) {
                            return Err(bad(index, "the position's parent does not exist".into()));
                        }
                    }
                }
                None => {
                    if !have(&place.pos) {
                        return Err(bad(index, "the position does not exist".into()));
                    }
                }
            }
            Ok(())
        };
        for (index, op) in change.ops.iter().enumerate() {
            match op {
                Op::Create {
                    entity,
                    kind,
                    place,
                    ..
                } => {
                    if *kind == Kind::Root {
                        return Err(bad(index, "the root cannot be created".into()));
                    }
                    if kind_of(&made, *entity).is_some() {
                        return Err(bad(index, "the entity already exists".into()));
                    }
                    match (kind.list(), place) {
                        (Some(_), Some(p)) => {
                            check_place(index, &made, &positions, *kind, p)?;
                            if p.node.is_some() {
                                positions.insert((p.list, p.pos));
                            }
                        }
                        (Some(_), None) => {
                            return Err(bad(index, format!("a {} needs a place", kind.name())))
                        }
                        (None, Some(_)) => {
                            return Err(bad(index, format!("a {} has no place", kind.name())))
                        }
                        (None, None) => {}
                    }
                    made.insert(*entity, *kind);
                }
                Op::Move { entity, place } => {
                    let Some(kind) = kind_of(&made, *entity) else {
                        return Err(bad(index, "the entity does not exist".into()));
                    };
                    check_place(index, &made, &positions, kind, place)?;
                    if place.node.is_some() {
                        positions.insert((place.list, place.pos));
                    }
                }
                Op::Set { entity, field, .. } => {
                    if kind_of(&made, *entity).is_none() {
                        return Err(bad(index, "the entity does not exist".into()));
                    }
                    if field.is_set() {
                        return Err(bad(
                            index,
                            format!("{} is a set, not a field", field.name()),
                        ));
                    }
                }
                Op::Delete { entity } | Op::Restore { entity } => {
                    if *entity == Uid::ROOT {
                        return Err(bad(index, "the root cannot be deleted".into()));
                    }
                    if kind_of(&made, *entity).is_none() {
                        return Err(bad(index, "the entity does not exist".into()));
                    }
                }
                Op::Add { entity, field, .. } | Op::Remove { entity, field, .. } => {
                    if kind_of(&made, *entity).is_none() {
                        return Err(bad(index, "the entity does not exist".into()));
                    }
                    if !field.is_set() {
                        return Err(bad(
                            index,
                            format!("{} is a field, not a set", field.name()),
                        ));
                    }
                }
                Op::Grant { .. } | Op::Revoke { .. } => {}
                Op::Key { .. } => {
                    if change
                        .ops
                        .iter()
                        .filter(|o| matches!(o, Op::Key { .. }))
                        .count()
                        > 1
                    {
                        return Err(bad(index, "one key per change".into()));
                    }
                }
                Op::ActsFor { person } => {
                    if *person == change.actor {
                        return Err(bad(index, "an actor can't write for itself".into()));
                    }
                }
            }
        }
        Ok(())
    }

    /// Applies `change`, which must pass [`State::check`] and whose
    /// dependencies must already be applied (the log enforces both).
    pub fn apply(&mut self, change: &Change) {
        if change.lamport > self.lamport {
            self.lamport = change.lamport;
        }
        if self.genesis.is_none() {
            self.genesis = Some(change.id());
        }
        if access::started_at(&self.grants, &change.deps) {
            self.judged.insert(change.actor);
        }
        for (index, op) in change.ops.iter().enumerate() {
            let stamp = change.stamp(index);
            match op {
                Op::Create {
                    entity,
                    kind,
                    place,
                    fields,
                    sets,
                } => {
                    let mut e = Entity::new(*kind, stamp);
                    for (f, v) in fields {
                        e.fields.insert(*f, Register::new(stamp, v.clone()));
                    }
                    for (f, members) in sets {
                        let mut s = OrSet::default();
                        for m in members {
                            s.add(m.clone(), stamp);
                        }
                        e.sets.insert(*f, s);
                    }
                    if let Some(p) = place {
                        self.hang(p);
                        e.place = Some(Register::new(
                            stamp,
                            Place {
                                list: p.list,
                                pos: p.pos,
                            },
                        ));
                    }
                    self.entities.insert(*entity, e);
                }
                Op::Set {
                    entity,
                    field,
                    value,
                } => {
                    if let Some(e) = self.entities.get_mut(entity) {
                        match e.fields.get_mut(field) {
                            Some(r) => r.write(stamp, value.clone()),
                            None => {
                                e.fields.insert(*field, Register::new(stamp, value.clone()));
                            }
                        }
                    }
                }
                Op::Move { entity, place } => {
                    self.hang(place);
                    if let Some(e) = self.entities.get_mut(entity) {
                        let p = Place {
                            list: place.list,
                            pos: place.pos,
                        };
                        match &mut e.place {
                            Some(r) => r.write(stamp, p),
                            None => e.place = Some(Register::new(stamp, p)),
                        }
                    }
                }
                Op::Delete { entity } => {
                    if let Some(e) = self.entities.get_mut(entity) {
                        e.deleted.write(stamp, true);
                    }
                }
                Op::Restore { entity } => {
                    if let Some(e) = self.entities.get_mut(entity) {
                        e.deleted.write(stamp, false);
                    }
                }
                Op::Add {
                    entity,
                    field,
                    member,
                } => {
                    if let Some(e) = self.entities.get_mut(entity) {
                        e.sets.entry(*field).or_default().add(member.clone(), stamp);
                    }
                }
                Op::Remove {
                    entity,
                    field,
                    member,
                    seen,
                } => {
                    if let Some(e) = self.entities.get_mut(entity) {
                        e.sets.entry(*field).or_default().remove(member, seen);
                    }
                }
                Op::Grant { actor, role } => {
                    self.write_grant(change, stamp, *actor, role.clone(), false);
                }
                Op::Revoke { actor } => {
                    self.write_grant(change, stamp, *actor, Role::ReadOnly, true);
                }
                Op::Key { key } => {
                    self.keys.entry(change.actor).or_insert((*key, change.id()));
                }
                Op::ActsFor { person } => match self.links.get_mut(&change.actor) {
                    Some(r) => r.write(stamp, *person),
                    None => {
                        self.links
                            .insert(change.actor, Register::new(stamp, *person));
                    }
                },
            }
        }
    }

    fn write_grant(
        &mut self,
        change: &Change,
        stamp: Stamp,
        actor: ActorId,
        role: Role,
        removed: bool,
    ) {
        let w = GrantWrite {
            role,
            rank: access::rank(&access::role_at(self, change.actor, &change.deps)),
            deps: change.deps.clone(),
            removed,
        };
        match self.grants.get_mut(&actor) {
            Some(r) => r.write(stamp, w),
            None => {
                self.grants.insert(actor, Register::new(stamp, w));
            }
        }
    }

    fn hang(&mut self, place: &Placement) {
        if let Some(node) = place.node {
            self.lists
                .entry(place.list)
                .or_default()
                .add(place.pos, node);
        }
    }

    /// The live entities of a list, in order: those not deleted whose
    /// place is in it. `keep` says which entities count as live (the
    /// projection passes its revived set).
    pub fn ordered(&self, key: &ListKey, keep: &dyn Fn(Uid) -> bool) -> Vec<Uid> {
        let mut at: BTreeMap<PosId, Vec<Uid>> = BTreeMap::new();
        for (uid, e) in &self.entities {
            if let Some(p) = e.place() {
                if p.list == *key && keep(*uid) {
                    at.entry(p.pos).or_default().push(*uid);
                }
            }
        }
        if at.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        if let Some(tree) = self.lists.get(key) {
            for pos in tree.order() {
                if let Some(uids) = at.remove(&pos) {
                    out.extend(uids);
                }
            }
        }
        // Positions the tree doesn't know (never with causal delivery, but
        // the order must be total anyway): by position stamp.
        for (_, uids) in at {
            out.extend(uids);
        }
        out
    }
}
