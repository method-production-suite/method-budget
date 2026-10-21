//! Who may change what (docs/research/collaboration.md, section 7;
//! decisions.md, entry 78).
//!
//! The person who started sharing (the actor of the first change, the
//! genesis) owns the budget. Until the owner gives anyone a role, everyone
//! who opens the budget edits everything, as before roles existed. Once a
//! role has been given, someone without one reads only.
//!
//! Every app checks every change, its own and everyone else's, with
//! [`check`], and drops what the author's role doesn't allow. The check is
//! made at the change's own causal point (its `deps`): the roles, and the
//! places of lines and notes, as its author had seen them. So every app
//! decides the same, whatever order the changes arrive in: a change made
//! before its author saw a new role is judged by the old one, and a line
//! moved out of someone's categories while they edited it keeps their
//! edit.
//!
//! Signatures (decisions.md, entry 89; [`super::sign`]) make it hold
//! against anyone who can write to the shared folder: once signing has
//! started, a change in someone's name needs their key. What a change
//! claims to have seen can still be a lie, so three kinds of change also
//! judge changes that never saw them ([`Bounds`]): the owner's first role
//! (a computer it never saw and didn't name is judged as after it), the
//! owner's key (the same computers must sign), and every removal (the
//! person's changes the remover hadn't seen are kept aside).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::clock::{ChangeId, Frontier};
use super::ids::{ActorId, Uid};
use super::op::{Change, Field, Kind, Op};
use super::state::{GrantWrite, Register, State};

/// What one person may do in a shared budget.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// Started sharing: everything, and sets everyone's role. Never given.
    Owner,
    /// Everything the owner does but changing the owner, making co-owners
    /// or changing one: the owner's way to make sure roles can still be
    /// set if the owner's computer is lost. Only the owner gives it.
    CoOwner,
    /// Everything, and sets the roles of everyone but the owner and admins.
    Admin,
    /// Everything but roles and locking the originals.
    Editor,
    /// Lines and notes in these categories or accounts: add, change, move
    /// within them, delete, and apply existing fringes and groups.
    Department(BTreeSet<Uid>),
    /// Actuals (transactions, imports, estimates to complete) and notes.
    Actuals,
    /// Notes only.
    Comment,
    /// Nothing.
    ReadOnly,
}

impl Role {
    /// The name files and the page use.
    pub fn key(&self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::CoOwner => "co_owner",
            Role::Admin => "admin",
            Role::Editor => "editor",
            Role::Department(_) => "department",
            Role::Actuals => "actuals",
            Role::Comment => "comment",
            Role::ReadOnly => "read_only",
        }
    }

    /// The role named `key`, with `scope` for a department editor. `None`
    /// for an unknown name, and for a scope given with any other role (it
    /// would mean nothing, so it is refused rather than dropped).
    pub fn parse(key: &str, scope: BTreeSet<Uid>) -> Option<Role> {
        if key != "department" && !scope.is_empty() {
            return None;
        }
        Some(match key {
            "owner" => Role::Owner,
            "co_owner" => Role::CoOwner,
            "admin" => Role::Admin,
            "editor" => Role::Editor,
            "department" => Role::Department(scope),
            "actuals" => Role::Actuals,
            "comment" => Role::Comment,
            "read_only" => Role::ReadOnly,
            _ => return None,
        })
    }

    /// The categories or accounts of a department editor.
    pub fn scope(&self) -> Option<&BTreeSet<Uid>> {
        match self {
            Role::Department(s) => Some(s),
            _ => None,
        }
    }

    /// Sets other people's roles.
    pub fn sets_roles(&self) -> bool {
        matches!(self, Role::Owner | Role::CoOwner | Role::Admin)
    }

    /// Can change nothing at all.
    pub fn reads_only(&self) -> bool {
        matches!(self, Role::ReadOnly)
    }
}

/// Why a change was refused: the op at `index` isn't within its author's
/// role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub index: usize,
    /// The author's role, as judged.
    pub role: Role,
    pub reason: String,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "op {}: {}", self.index, self.reason)
    }
}

/// The write of `reg` that was current at `deps`: the last one whose
/// change is within it.
fn visible<'a, T>(reg: &'a Register<T>, deps: &Frontier) -> Option<&'a T>
where
    T: Clone,
{
    reg.history()
        .iter()
        .rev()
        .find(|(s, _)| deps.includes(s.change()))
        .map(|(_, v)| v)
}

/// Whether someone with role `granter` may give `given` to a person (not the
/// owner) whose role is `target`: the owner anything but ownership, a
/// co-owner anything but co-owners, an admin anything but admins and
/// co-owners, nobody else anything. The rule [`check`] judges a `grant` by,
/// for the interface to ask before it offers a choice.
pub fn may_give(granter: &Role, target: &Role, given: &Role) -> bool {
    if !granter.sets_roles() || *given == Role::Owner || *target == Role::Owner {
        return false;
    }
    let co_owners = |r: &Role| *r == Role::CoOwner;
    let admins = |r: &Role| matches!(r, Role::Admin | Role::CoOwner);
    match granter {
        Role::CoOwner => !(co_owners(given) || co_owners(target)),
        Role::Admin => !(admins(given) || admins(target)),
        _ => true,
    }
}

/// How a granter ranks when grants made apart meet: owner over co-owner
/// over admin (nobody else gives roles).
pub fn rank(role: &Role) -> u8 {
    match role {
        Role::Owner => 3,
        Role::CoOwner => 2,
        Role::Admin => 1,
        _ => 0,
    }
}

/// Whether any role had been given by `deps`.
pub fn started_at(grants: &BTreeMap<ActorId, Register<GrantWrite>>, deps: &Frontier) -> bool {
    grants.values().any(|r| visible(r, deps).is_some())
}

/// Whether roles had started by `deps`: a role given, or an invite made
/// (decisions.md, entry 97: an invite is a role given ahead).
pub fn started(state: &State, deps: &Frontier) -> bool {
    started_at(state.grants(), deps) || state.invites().values().any(|i| deps.includes(i.change))
}

/// A person's role from their grants as someone who had seen `deps` knew
/// them. A grant made after seeing another replaces it; among grants none
/// of which saw the others, the higher-ranked granter wins, then the later
/// stamp, so every computer settles them alike.
pub fn grant_at<'a>(reg: &'a Register<GrantWrite>, deps: &Frontier) -> Option<&'a Role> {
    grant_write_at(reg, deps).map(|w| &w.role)
}

/// The grant write [`grant_at`] settles on.
pub fn grant_write_at<'a>(
    reg: &'a Register<GrantWrite>,
    deps: &Frontier,
) -> Option<&'a GrantWrite> {
    let seen: Vec<&(super::clock::Stamp, GrantWrite)> = reg
        .history()
        .iter()
        .filter(|(s, _)| deps.includes(s.change()))
        .collect();
    seen.iter()
        .filter(|(s, _)| {
            !seen
                .iter()
                .any(|(o, w)| o.change() != s.change() && w.deps.includes(s.change()))
        })
        .max_by_key(|(s, w)| (w.rank, *s))
        .map(|(_, w)| w)
}

/// The person `actor` writes for at `deps` (`Op::ActsFor`), or itself.
pub fn person_at(state: &State, actor: ActorId, deps: &Frontier) -> ActorId {
    state
        .links()
        .get(&actor)
        .and_then(|r| visible(r, deps))
        .copied()
        .unwrap_or(actor)
}

/// `actor`'s role as someone who had seen `deps` knew it. An actor that
/// writes for a person has the person's role.
pub fn role_at(state: &State, actor: ActorId, deps: &Frontier) -> Role {
    let actor = person_at(state, actor, deps);
    match state.owner() {
        None => return Role::Owner,
        Some(o) if o == actor => return Role::Owner,
        Some(_) => {}
    }
    let grants = state.grants();
    if let Some(r) = grants.get(&actor).and_then(|r| grant_at(r, deps)) {
        return r.clone();
    }
    if started(state, deps) {
        Role::ReadOnly
    } else {
        Role::Editor
    }
}

/// What the change itself creates: each new entity's kind and the owner
/// of the list it goes in.
type Made = BTreeMap<Uid, (Kind, Option<Uid>)>;

/// The owner of the list `uid` was in at `deps` (an account for a line, a
/// category for an account...), or where this change put it.
fn parent_at(state: &State, made: &Made, uid: Uid, deps: &Frontier) -> Option<Uid> {
    if let Some((_, parent)) = made.get(&uid) {
        return *parent;
    }
    let e = state.entity(uid)?;
    let reg = e.place.as_ref()?;
    visible(reg, deps).map(|p| p.list.parent)
}

/// Whether `uid` is, or was at `deps`, inside one of `scope`'s categories
/// or accounts (a line in an account, a note on a line in one...).
fn in_scope(
    state: &State,
    made: &Made,
    scope: &BTreeSet<Uid>,
    mut uid: Uid,
    deps: &Frontier,
) -> bool {
    // A note on a line in an account in a category: four steps at most.
    for _ in 0..5 {
        if scope.contains(&uid) {
            return true;
        }
        match parent_at(state, made, uid, deps) {
            Some(p) if p != Uid::ROOT => uid = p,
            _ => return false,
        }
    }
    false
}

fn kind_of(state: &State, made: &Made, uid: Uid) -> Option<Kind> {
    made.get(&uid)
        .map(|(k, _)| *k)
        .or_else(|| state.entity(uid).map(|e| e.kind))
}

/// The person a change writes for: the one its own `acts_for` names, or
/// the one its actor writes for at its `deps`, or the actor itself.
pub fn person_of_change(state: &State, change: &Change) -> ActorId {
    change
        .ops
        .iter()
        .find_map(|op| match op {
            Op::ActsFor { person } => Some(*person),
            _ => None,
        })
        .unwrap_or_else(|| person_at(state, change.actor, &change.deps))
}

/// A change that also judges changes that never saw it: what it is, and
/// who it had heard of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound {
    pub id: ChangeId,
    pub deps: Frontier,
    /// Actors with a change it had seen, the owner, and anyone a role it
    /// had seen (or gives) names: everyone it knew of.
    pub known: BTreeSet<ActorId>,
}

impl Bound {
    /// Whether `change` is judged as after this one although it never saw
    /// it: made by someone this one didn't know of, without having seen it.
    pub fn catches_stranger(&self, change: &Change) -> bool {
        change.id() != self.id
            && !self.known.contains(&change.actor)
            && !change.deps.includes(self.id)
    }
}

/// A removal: who, and everything the remover had seen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Removal {
    pub id: ChangeId,
    pub person: ActorId,
    pub deps: Frontier,
}

impl Removal {
    /// Whether `change`, made for `person`, falls after the cutoff: the
    /// remover hadn't seen it and it hadn't seen the removal.
    pub fn cuts(&self, person: ActorId, change: &Change) -> bool {
        person == self.person
            && change.seq > self.deps.get(&change.actor)
            && !change.deps.includes(self.id)
    }
}

/// The changes that judge changes outside their own future (decisions.md,
/// entry 89). Taken from the changes applied; when one arrives after
/// changes it judges differently, the log folds everything again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bounds {
    /// The owner's first change giving a role.
    pub roles: Option<Bound>,
    /// The owner's first change publishing a key: signing starts.
    pub signing: Option<Bound>,
    /// Every removal applied.
    pub removals: Vec<Removal>,
    /// Every claim of an invite applied (decisions.md, entry 97): a claim
    /// made apart from one of these may lose the invite to it.
    pub claims: Vec<super::invite::ClaimBound>,
    /// Every invite cancelled: a claim its canceller hadn't seen is refused.
    pub cancels: Vec<super::invite::CancelBound>,
}

impl Bounds {
    /// What `change` adds, now that it is applied to `state`.
    pub fn note(&mut self, state: &State, change: &Change) {
        let owner = state.owner();
        let known = || {
            let mut k: BTreeSet<ActorId> = change
                .deps
                .0
                .iter()
                .filter(|(_, s)| **s > 0)
                .map(|(a, _)| *a)
                .collect();
            k.extend(owner);
            k.insert(change.actor);
            for (a, reg) in state.grants() {
                if grant_at(reg, &change.deps).is_some() {
                    k.insert(*a);
                }
            }
            for op in &change.ops {
                if let Op::Grant { actor, .. } | Op::Revoke { actor } = op {
                    k.insert(*actor);
                }
            }
            k
        };
        if Some(change.actor) == owner {
            if self.roles.is_none() && change.grants() {
                self.roles = Some(Bound {
                    id: change.id(),
                    deps: change.deps.clone(),
                    known: known(),
                });
            }
            if self.signing.is_none() && change.key().is_some() {
                self.signing = Some(Bound {
                    id: change.id(),
                    deps: change.deps.clone(),
                    known: known(),
                });
            }
        }
        for op in &change.ops {
            match op {
                Op::Revoke { actor } => self.removals.push(Removal {
                    id: change.id(),
                    person: *actor,
                    deps: change.deps.clone(),
                }),
                Op::Claim { invite, .. } => self.claims.push(super::invite::ClaimBound {
                    id: change.id(),
                    invite: *invite,
                    lamport: change.lamport,
                    deps: change.deps.clone(),
                }),
                Op::CancelInvite { invite } => self.cancels.push(super::invite::CancelBound {
                    id: change.id(),
                    invite: *invite,
                    deps: change.deps.clone(),
                }),
                _ => {}
            }
        }
    }

    /// Whether `change` (already applied) would be judged differently by
    /// the bounds `new` adds.
    pub fn changes_verdict(state: &State, new: &Bounds, old: &Bounds, change: &Change) -> bool {
        if change.claim().is_some()
            && super::invite::cut(state, new, change) != super::invite::cut(state, old, change)
        {
            return true;
        }
        if state.owner() == Some(change.actor) {
            return false;
        }
        let fresh = |b: &Option<Bound>, o: &Option<Bound>| {
            b.as_ref()
                .filter(|b| o.as_ref().map(|o| o.id) != Some(b.id))
                .is_some_and(|b| b.catches_stranger(change))
        };
        if fresh(&new.roles, &old.roles) || fresh(&new.signing, &old.signing) {
            return true;
        }
        let person = person_of_change(state, change);
        new.removals
            .iter()
            .filter(|r| !old.removals.contains(r))
            .any(|r| r.cuts(person, change))
    }

    /// Whether `change` falls after a removal of its person.
    pub fn removed(&self, state: &State, change: &Change) -> Option<&Removal> {
        if self.removals.is_empty() {
            return None;
        }
        let person = person_of_change(state, change);
        self.removals.iter().find(|r| r.cuts(person, change))
    }

    /// Whether `change` must be signed: its author has a key (or publishes
    /// one in it), it was made after the owner's key, or its author is a
    /// stranger to the owner's key change.
    pub fn must_sign(&self, state: &State, change: &Change) -> bool {
        if state.key_of(change.actor).is_some() || change.key().is_some() {
            return true;
        }
        self.signing
            .as_ref()
            .is_some_and(|s| change.deps.includes(s.id) || s.catches_stranger(change))
    }
}

/// Checks every op of `change` against its author's role at the change's
/// causal point. `change` must already pass [`State::check`].
pub fn check(state: &State, change: &Change) -> Result<(), Refusal> {
    check_within(state, &Bounds::default(), change)
}

/// [`check`], with the changes that judge changes which never saw them.
pub fn check_within(state: &State, bounds: &Bounds, change: &Change) -> Result<(), Refusal> {
    let deps = &change.deps;
    // A computer the owner's first role never knew of, writing as if it
    // hadn't seen it: judged as after it, where it has no role.
    let stranger = state.owner() != Some(change.actor)
        && bounds
            .roles
            .as_ref()
            .is_some_and(|r| r.catches_stranger(change));
    let role = if stranger {
        Role::ReadOnly
    } else {
        role_at(state, change.actor, deps)
    };
    let mut made = Made::new();
    let mut made_links = false;
    for (index, op) in change.ops.iter().enumerate() {
        let refuse = |reason: String| Refusal {
            index,
            role: role.clone(),
            reason,
        };
        if let Op::Create {
            entity,
            kind,
            place,
            ..
        } = op
        {
            made.insert(*entity, (*kind, place.map(|p| p.list.parent)));
        }
        if let Op::ActsFor { person } = op {
            // Once per actor, and never for an actor that itself writes for
            // someone.
            let linked = |a: ActorId| {
                state
                    .links()
                    .get(&a)
                    .is_some_and(|r| visible(r, deps).is_some())
            };
            if linked(change.actor) || made_links {
                return Err(refuse("this actor already writes for someone".into()));
            }
            if linked(*person) {
                return Err(refuse("that actor itself writes for someone".into()));
            }
            // Only a new program's actor says whose it is, once roles exist,
            // and only for the owner or a member: never the owner's own
            // actor, a member's own (which has a role of its own), or one
            // whose changes were already judged under roles.
            let grants = state.grants();
            if !started(state, deps) {
                return Err(refuse(
                    "an actor says whose it is only once roles are set".into(),
                ));
            }
            if state.owner() == Some(change.actor)
                || grants
                    .get(&change.actor)
                    .is_some_and(|r| grant_at(r, deps).is_some())
            {
                return Err(refuse(
                    "a person's own actor doesn't write for someone else".into(),
                ));
            }
            if state.judged(change.actor) {
                return Err(refuse(
                    "this actor's changes were already judged under roles".into(),
                ));
            }
            let member = state.owner() == Some(*person)
                || grants
                    .get(person)
                    .is_some_and(|r| grant_at(r, deps).is_some());
            if !member {
                return Err(refuse(
                    "an actor writes only for the owner or someone with a role".into(),
                ));
            }
            made_links = true;
            continue;
        }
        if let Op::Key { .. } = op {
            // A key is about the computer, not the budget: anyone may
            // publish theirs (the signature checks say whether it is theirs).
            continue;
        }
        if let Op::Claim { .. } = op {
            // About the claimant's own joining, judged with the invite
            // (`super::invite`), not by a role they don't have yet.
            continue;
        }
        if let Op::Invite { role: given, .. } = op {
            // A role given ahead to someone new: the rules of a grant to a
            // person with no role.
            if !role.sets_roles() {
                return Err(refuse("only the owner and admins invite people".into()));
            }
            if !may_give(&role, &Role::ReadOnly, given) {
                return Err(refuse(if role == Role::CoOwner {
                    "only the owner invites a co-owner".into()
                } else {
                    "only the owner and co-owners invite an admin".into()
                }));
            }
            continue;
        }
        if let Op::CancelInvite { invite } = op {
            let Some(rec) = state.invites().get(invite) else {
                return Err(refuse("there is no such invite".into()));
            };
            if !deps.includes(rec.change) {
                return Err(refuse("made without having seen the invite".into()));
            }
            let mine = person_at(state, rec.by, &rec.deps) == person_of_change(state, change);
            if !(mine || (role.sets_roles() && may_give(&role, &Role::ReadOnly, &rec.role))) {
                return Err(refuse(
                    "only the person who made an invite, or someone who may give its role, cancels it"
                        .into(),
                ));
            }
            continue;
        }
        let granted = match op {
            Op::Grant { actor, role } => Some((actor, role.clone())),
            Op::Revoke { actor } => Some((actor, Role::ReadOnly)),
            _ => None,
        };
        if let Some((actor, given)) = granted {
            let given = &given;
            if !role.sets_roles() {
                return Err(refuse("only the owner and admins set roles".into()));
            }
            if *given == Role::Owner {
                return Err(refuse("ownership isn't given with a role".into()));
            }
            if state.owner() == Some(*actor) {
                return Err(refuse("the owner's role doesn't change".into()));
            }
            let target = role_at(state, *actor, deps);
            if !may_give(&role, &target, given) {
                return Err(refuse(if role == Role::CoOwner {
                    "only the owner makes co-owners or changes a co-owner's role".into()
                } else {
                    "only the owner and co-owners make admins or change an admin's role".into()
                }));
            }
            continue;
        }
        let entity = op.entity();
        let kind = kind_of(state, &made, entity);
        match &role {
            Role::Owner | Role::CoOwner | Role::Admin => {}
            Role::Editor => {
                if let Op::Set {
                    field: Field::Baseline,
                    ..
                } = op
                {
                    if entity == Uid::ROOT {
                        return Err(refuse(
                            "only the owner and admins lock the originals".into(),
                        ));
                    }
                }
            }
            Role::Department(scope) => {
                if !matches!(kind, Some(Kind::Row | Kind::Note)) {
                    return Err(refuse(
                        "a department editor changes lines and notes only".into(),
                    ));
                }
                let inside = match op {
                    Op::Create { place, .. } => place
                        .map(|p| in_scope(state, &made, scope, p.list.parent, deps))
                        .unwrap_or(false),
                    Op::Move { place, .. } => {
                        in_scope(state, &made, scope, entity, deps)
                            && in_scope(state, &made, scope, place.list.parent, deps)
                    }
                    _ => in_scope(state, &made, scope, entity, deps),
                };
                if !inside {
                    return Err(refuse(
                        "outside the categories and accounts this person edits".into(),
                    ));
                }
            }
            Role::Actuals => {
                if !matches!(
                    kind,
                    Some(Kind::Transaction | Kind::Batch | Kind::Etc | Kind::Note)
                ) {
                    return Err(refuse("this person changes actuals and notes only".into()));
                }
            }
            Role::Comment => {
                if kind != Some(Kind::Note) {
                    return Err(refuse("this person writes notes only".into()));
                }
            }
            Role::ReadOnly => {
                return Err(refuse("this person reads only".into()));
            }
        }
    }
    Ok(())
}
