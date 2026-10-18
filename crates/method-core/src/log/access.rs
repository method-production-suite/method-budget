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
//! Without signatures this is enforced by honest apps only: anyone who
//! can write to the shared folder can write a change in someone else's
//! name. Signatures come with the key pairs of phase 3 (design 7).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::clock::Frontier;
use super::ids::{ActorId, Uid};
use super::op::{Change, Field, Kind, Op};
use super::state::{Register, State};

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

/// Whether any role had been given by `deps`.
pub fn started_at(grants: &BTreeMap<ActorId, Register<Role>>, deps: &Frontier) -> bool {
    grants.values().any(|r| visible(r, deps).is_some())
}

/// `actor`'s role as someone who had seen `deps` knew it.
pub fn role_at(state: &State, actor: ActorId, deps: &Frontier) -> Role {
    match state.owner() {
        None => return Role::Owner,
        Some(o) if o == actor => return Role::Owner,
        Some(_) => {}
    }
    let grants = state.grants();
    if let Some(r) = grants.get(&actor).and_then(|r| visible(r, deps)) {
        return r.clone();
    }
    if started_at(grants, deps) {
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

/// Checks every op of `change` against its author's role at the change's
/// causal point. `change` must already pass [`State::check`].
pub fn check(state: &State, change: &Change) -> Result<(), Refusal> {
    let deps = &change.deps;
    let role = role_at(state, change.actor, deps);
    let mut made = Made::new();
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
        if let Op::Grant { actor, role: given } = op {
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
            let co_owners = |r: &Role| *r == Role::CoOwner;
            let admins = |r: &Role| matches!(r, Role::Admin | Role::CoOwner);
            if role == Role::CoOwner && (co_owners(given) || co_owners(&target)) {
                return Err(refuse(
                    "only the owner makes co-owners or changes a co-owner's role".into(),
                ));
            }
            if role == Role::Admin && (admins(given) || admins(&target)) {
                return Err(refuse(
                    "only the owner and co-owners make admins or change an admin's role".into(),
                ));
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
