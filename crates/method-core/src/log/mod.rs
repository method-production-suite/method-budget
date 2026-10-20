//! The change log under the budget (docs/research/collaboration.md,
//! sections 3 and 8; decisions.md, entry 62).
//!
//! Every edit is a [`Change`]: one undo step, by one actor, made of small
//! operations on entities with stable ids: last-writer-wins registers per
//! field, add-wins sets for tags, a move register per entity with
//! positions from a Fugue tree per list. The [`State`] is a deterministic
//! fold of all changes, and the [`Budget`] is its [`project`]ion, with the
//! repairs of 3.10 made on the way out and listed for review. Applying the
//! same changes in any causal order gives the same state and the same
//! budget.
//!
//! Edits are captured by diff ([`capture`]): the budget before and after
//! an edit become ops, matched by row id, code and name through the
//! projection's [`Binding`]. [`Log`] is one actor's view: its state, the
//! changes it has applied, what it is waiting for, and its own clock.
//!
//! No networking and no files here: the on-disk log lives in
//! `method-format` (`method_format::log`), transports come later.

pub mod access;
pub mod capture;
pub mod clock;
pub mod formula;
pub mod hash;
pub mod ids;
pub mod op;
pub mod project;
pub mod review;
pub mod schema;
pub mod seq;
pub mod sign;
pub mod state;
pub mod undo;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

pub use access::{Bounds, Refusal, Role};
pub use capture::Hint;
pub use clock::{ChangeId, Frontier, Stamp};
pub use ids::{ActorId, Ids, Uid};
pub use op::{Change, Field, Kind, ListKey, ListName, Member, Op, Placement, Value};
pub use project::{project, Binding, Projection};
pub use review::{Review, ReviewItem};
pub use seq::{Node, PosId, SeqTree, Side};
pub use sign::{PublicKey, Signature, Signer, Verifier};
pub use state::{Entity, GrantWrite, Malformed, OrSet, Place, Register, State};
pub use undo::Inverse;

use crate::model::Budget;
use crate::rename::SetupKind;

/// The entity kind of a setup table.
pub fn rename_kind(k: SetupKind) -> Kind {
    match k {
        SetupKind::Global => Kind::Global,
        SetupKind::Fringe => Kind::Fringe,
        SetupKind::Group => Kind::Group,
        SetupKind::Unit => Kind::Unit,
        SetupKind::Set => Kind::Set,
        SetupKind::Location => Kind::Location,
        SetupKind::Currency => Kind::Currency,
    }
}

/// Why a change was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogError {
    /// The change doesn't fit the state (it names something that doesn't
    /// exist, or puts an entity where it can't go). It was skipped.
    Malformed { id: ChangeId, reason: String },
    /// A different change with the same actor and seq was applied
    /// before: the actor forked its history.
    Fork { id: ChangeId },
    /// Not one of this actor's own changes, or not applied.
    NotMine { id: ChangeId },
    /// The change goes beyond its author's role (decisions.md, entry 78).
    /// Refused by every app, so it is listed and skipped.
    NotAllowed { id: ChangeId, refusal: Refusal },
    /// The change's signature is missing or doesn't match its author's key
    /// (decisions.md, entry 89). Kept aside and listed.
    Unsigned { id: ChangeId, reason: String },
    /// The change was made after its author was removed, without having
    /// seen the removal (decisions.md, entry 89). Kept aside and listed.
    Removed { id: ChangeId },
}

/// Why a change is in [`Log::forks`]: it doesn't pass, and the change with
/// the same number that does was taken in.
pub const FORK_REPLACED: &str =
    "refused, and the change with the same number that passes was taken in";
/// Why a change is in [`Log::forks`]: it doesn't pass, and neither does
/// the other change with its number.
pub const FORK_REFUSED_TOO: &str = "refused, as was the other change with the same number";
/// Why a change is in [`Log::forks`] for now: its number was settled before
/// the changes it depends on arrived; it is weighed again when they do.
pub const FORK_WAITING: &str = "waiting for the changes it depends on";
/// Why two changes with one number that both pass are both left out (a
/// fork): in [`Log::forks`], and with [`refused::FORK`] in the rejected list.
pub const FORK_SECOND: &str =
    "two different changes that both pass claim the same number, so neither was taken in";

/// The prefixes of [`Log::rejected`]'s reasons, so a reader can tell why
/// without parsing sentences.
pub mod refused {
    /// Beyond the author's role.
    pub const NOT_ALLOWED: &str = "not allowed: ";
    /// A signature missing or not matching.
    pub const SIGNATURE: &str = "signature: ";
    /// After the author was removed.
    pub const REMOVED: &str = "removed: ";
    /// Two changes that both pass claim one number.
    pub const FORK: &str = "fork: ";
}

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogError::Malformed { id, reason } => write!(f, "change {id} is malformed: {reason}"),
            LogError::Fork { id } => {
                write!(f, "change {id} differs from the one already applied")
            }
            LogError::NotMine { id } => write!(f, "change {id} is not one of this actor's"),
            LogError::NotAllowed { id, refusal } => {
                write!(f, "change {id} is not allowed: {}", refusal.reason)
            }
            LogError::Unsigned { id, reason } => {
                write!(f, "change {id} isn't signed properly: {reason}")
            }
            LogError::Removed { id } => {
                write!(f, "change {id} was made after its author was removed")
            }
        }
    }
}

impl std::error::Error for LogError {}

/// What happened to a change handed to [`Log::apply`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Applied {
    /// Applied, with how many pending changes it unblocked.
    Applied { unblocked: usize },
    /// Held until the changes it depends on arrive.
    Pending,
    /// Already applied; nothing to do.
    Duplicate,
}

/// One actor's log: the folded state, every change applied (in order),
/// changes waiting for their dependencies, and changes refused.
#[derive(Clone)]
pub struct Log {
    actor: ActorId,
    ids: Ids,
    state: State,
    changes: BTreeMap<ChangeId, Change>,
    order: Vec<ChangeId>,
    frontier: Frontier,
    pending: Vec<Change>,
    rejected: Vec<(Change, String)>,
    /// Every change taken in (applied or refused), in the order taken: what
    /// a fold starts again from.
    received: Vec<ChangeId>,
    /// The changes that judge changes which never saw them.
    bounds: Bounds,
    /// This computer's key, when it signs (decisions.md, entry 89).
    signer: Option<Arc<dyn Signer>>,
    /// What checks signatures; without it a change that needs one is kept
    /// aside.
    verifier: Option<Arc<dyn Verifier>>,
    /// How many times everything was folded again.
    folds: u64,
    /// Changes that claimed a number already taken by another change of
    /// the same actor: a forgery that lost its place to the real change, or
    /// two different changes that both pass (a fork, which signing makes
    /// impossible without the key). Listed loudly, never applied.
    forks: Vec<(Change, String)>,
}

impl fmt::Debug for Log {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Log")
            .field("actor", &self.actor)
            .field("changes", &self.order.len())
            .field("frontier", &self.frontier)
            .field("pending", &self.pending.len())
            .field("rejected", &self.rejected.len())
            .field("signs", &self.signer.is_some())
            .finish()
    }
}

impl Log {
    /// An empty log for `actor`.
    pub fn new(actor: ActorId, ids: Ids) -> Log {
        Log {
            actor,
            ids,
            state: State::new(),
            changes: BTreeMap::new(),
            order: Vec::new(),
            frontier: Frontier::new(),
            pending: Vec::new(),
            rejected: Vec::new(),
            received: Vec::new(),
            bounds: Bounds::default(),
            signer: None,
            verifier: None,
            folds: 0,
            forks: Vec::new(),
        }
    }

    /// Changes that claimed a number another change of the same actor has:
    /// a refused change that a valid one took the place of, or a second
    /// valid change at a number already applied (a fork). Never applied.
    pub fn forks(&self) -> &[(Change, String)] {
        &self.forks
    }

    /// The key published in this actor's name, when it isn't this
    /// computer's own (decisions.md, entry 89): someone else put a key in
    /// for it. This actor then signs nothing and records nothing.
    pub fn key_conflict(&self) -> Option<PublicKey> {
        let mine = self.signer.as_ref()?.public();
        self.state
            .key_of(self.actor)
            .map(|(k, _)| k)
            .filter(|k| *k != mine)
    }

    /// How many times the log has folded everything again because a change
    /// arrived that judged changes applied before it (a removal, the
    /// owner's first role or key): a reader's cue that changes it had may
    /// have been left out since.
    pub fn folds(&self) -> u64 {
        self.folds
    }

    /// Checks signatures with `verifier` from now on.
    pub fn set_verifier(&mut self, verifier: Arc<dyn Verifier>) {
        self.verifier = Some(verifier);
    }

    /// Signs this actor's changes with `signer` from now on, publishing its
    /// key with the first change that needs it.
    pub fn set_signer(&mut self, signer: Arc<dyn Signer>) {
        self.signer = Some(signer);
    }

    /// This computer's public key, when it has one.
    pub fn signer_key(&self) -> Option<PublicKey> {
        self.signer.as_ref().map(|s| s.public())
    }

    /// A log whose first change (the genesis) creates everything in
    /// `budget`, by `actor`.
    pub fn adopt(actor: ActorId, ids: Ids, budget: &Budget, label: &str, time: &str) -> Log {
        let mut log = Log::new(actor, ids);
        let empty = Projection::empty();
        log.record(&empty, budget, &[], label, time)
            .expect("the first change always fits an empty state");
        log
    }

    pub fn actor(&self) -> ActorId {
        self.actor
    }

    /// Names the budget's first change before anything is applied (the
    /// log folder's `budget.json` records it): its actor owns the budget,
    /// and a change that hasn't seen it is refused, whatever order the
    /// changes arrive in (decisions.md, entry 78).
    pub fn set_genesis(&mut self, genesis: ChangeId) {
        self.state.set_genesis(genesis);
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    /// Everything applied so far.
    pub fn frontier(&self) -> &Frontier {
        &self.frontier
    }

    /// The highest Lamport clock applied.
    pub fn lamport(&self) -> u64 {
        self.state.lamport()
    }

    /// This actor's last sequence number.
    pub fn own_seq(&self) -> u64 {
        self.frontier.get(&self.actor)
    }

    /// The changes applied, in the order they were applied.
    pub fn changes(&self) -> impl Iterator<Item = &Change> {
        self.order.iter().filter_map(|id| self.changes.get(id))
    }

    pub fn change(&self, id: ChangeId) -> Option<&Change> {
        self.changes.get(&id)
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Applied changes outside `frontier`: what a peer at that frontier is
    /// missing, in applied order.
    pub fn since(&self, frontier: &Frontier) -> Vec<&Change> {
        self.changes()
            .filter(|c| !frontier.includes(c.id()))
            .collect()
    }

    /// This actor's own changes with a seq above `seq`, in order: those
    /// applied and those refused (refused after the fact: a removal that
    /// arrived later), so the folder never has a gap in its sequence.
    pub fn own_after(&self, seq: u64) -> Vec<&Change> {
        let mut out: Vec<&Change> = self
            .changes()
            .chain(self.rejected.iter().map(|(c, _)| c))
            .filter(|c| c.actor == self.actor && c.seq > seq)
            .collect();
        out.sort_by_key(|c| c.seq);
        out.dedup_by_key(|c| c.seq);
        out
    }

    /// Changes waiting for their dependencies.
    pub fn pending(&self) -> &[Change] {
        &self.pending
    }

    /// Changes refused, with why.
    pub fn rejected(&self) -> &[(Change, String)] {
        &self.rejected
    }

    /// The budget as it stands, its binding, and what to review.
    pub fn project(&self) -> Projection {
        let mut p = project(&self.state);
        self.concurrency_reviews(&mut p);
        p
    }

    /// Records an edit: the ops that take `before` (the current
    /// projection) to `after` become a change by this actor, applied at
    /// once. `None` when nothing changed. Fails only on a capture bug (an
    /// op the state refuses), which leaves the log untouched.
    pub fn record(
        &mut self,
        before: &Projection,
        after: &Budget,
        hints: &[Hint],
        label: &str,
        time: &str,
    ) -> Result<Option<ChangeId>, LogError> {
        let seq = self.own_seq() + 1;
        let lamport = self.state.lamport() + 1;
        let ops = capture::capture(
            &self.state,
            before,
            after,
            hints,
            &mut self.ids,
            self.actor,
            seq,
            lamport,
        );
        if ops.is_empty() {
            return Ok(None);
        }
        let change = self.sealed(Change {
            actor: self.actor,
            seq,
            deps: self.frontier.clone(),
            lamport,
            time: time.to_string(),
            label: label.to_string(),
            ops,
            sig: None,
        });
        self.check_own(&change)?;
        let id = change.id();
        // Checked just above, so this cannot refuse it.
        let _ = self.apply_now(change);
        Ok(Some(id))
    }

    /// Whether this actor's next change should publish its key: it signs,
    /// has no key in the log yet, and signing has started (or this is the
    /// owner, and roles exist or this change gives the first).
    fn wants_key(&self, change: &Change) -> bool {
        self.signer.is_some()
            && self.state.key_of(self.actor).is_none()
            && (self.bounds.signing.is_some()
                || (self.state.owner() == Some(self.actor)
                    && (self.roles_started() || change.grants())))
    }

    /// One of this actor's own changes, made ready to apply: its key added
    /// when it publishes one (as the last op, so no op's stamp moves), and
    /// signed when this actor signs.
    fn sealed(&self, mut change: Change) -> Change {
        let Some(signer) = &self.signer else {
            return change;
        };
        if self.key_conflict().is_some() {
            // Never sign as an actor whose published key isn't ours.
            return change;
        }
        if change.key().is_none() && self.wants_key(&change) {
            change.ops.push(Op::Key {
                key: signer.public(),
            });
        }
        if self.state.key_of(self.actor).is_some() || change.key().is_some() {
            change.sig = None;
            change.sig = Some(signer.sign(&change));
        }
        change
    }

    /// Checks one of this actor's own changes before it is applied: it
    /// must fit the state, be signed when it must, and fit this person's
    /// role. Nothing is recorded when it doesn't.
    fn check_own(&self, change: &Change) -> Result<(), LogError> {
        if self.key_conflict().is_some() {
            return Err(LogError::Unsigned {
                id: change.id(),
                reason: sign::why::KEY_TAKEN.to_string(),
            });
        }
        self.judge(change).map_err(|b| b.1)
    }

    /// Every check a change passes before it is applied, in order: it fits
    /// the state, its signature (decisions.md, entry 89), its author wasn't
    /// removed before it, and its author's role. The reason for the
    /// rejected list and the error, when it doesn't.
    fn judge(&self, change: &Change) -> Result<(), Box<(String, LogError)>> {
        let id = change.id();
        if let Err(m) = self.state.check(change) {
            let reason = m.to_string();
            return Err(Box::new((
                reason.clone(),
                LogError::Malformed { id, reason },
            )));
        }
        if let Err(why) = self.check_signature(change) {
            return Err(Box::new((
                format!("{}{why}", refused::SIGNATURE),
                LogError::Unsigned {
                    id,
                    reason: why.to_string(),
                },
            )));
        }
        if self.bounds.removed(&self.state, change).is_some() {
            return Err(Box::new((
                format!(
                    "{}made after its author was removed, without having seen it",
                    refused::REMOVED
                ),
                LogError::Removed { id },
            )));
        }
        access::check_within(&self.state, &self.bounds, change).map_err(|refusal| {
            Box::new((
                format!("{}{}", refused::NOT_ALLOWED, refusal.reason),
                LogError::NotAllowed { id, refusal },
            ))
        })
    }

    /// The signature rules (decisions.md, entry 89; [`sign`]).
    fn check_signature(&self, change: &Change) -> Result<(), &'static str> {
        let actor = change.actor;
        let published = change.key();
        let had = self.state.key_of(actor);
        if published.is_some() && had.is_some() {
            return Err(sign::why::TWO_KEYS);
        }
        if !self.bounds.must_sign(&self.state, change) {
            return match change.sig {
                Some(_) => Err(sign::why::NO_KEY),
                None => Ok(()),
            };
        }
        let Some(key) = published.or(had.map(|(k, _)| k)) else {
            return Err(sign::why::MISSING);
        };
        if change.sig.is_none() {
            return Err(sign::why::MISSING);
        }
        let Some(verifier) = &self.verifier else {
            return Err(sign::why::CANT_CHECK);
        };
        if !verifier.verify(&key, change) {
            return Err(sign::why::MISMATCH);
        }
        // A program signs with its person's key, which must be in the log
        // where the program writes from.
        let person = access::person_of_change(&self.state, change);
        let says_whose = change.ops.iter().any(|op| matches!(op, Op::ActsFor { .. }));
        if person != actor && (says_whose || published.is_some()) {
            match self.state.key_of(person) {
                Some((k, at)) if change.deps.includes(at) => {
                    if k != key {
                        return Err(sign::why::NOT_PERSONS_KEY);
                    }
                }
                _ => return Err(sign::why::PERSON_HAS_NO_KEY),
            }
        }
        Ok(())
    }

    /// Records a change made of `ops` by this actor (an undo, a change
    /// built by a tool).
    fn record_ops(&mut self, ops: Vec<Op>, label: &str, time: &str) -> Result<ChangeId, LogError> {
        let change = self.sealed(Change {
            actor: self.actor,
            seq: self.own_seq() + 1,
            deps: self.frontier.clone(),
            lamport: self.state.lamport() + 1,
            time: time.to_string(),
            label: label.to_string(),
            ops,
            sig: None,
        });
        self.check_own(&change)?;
        let id = change.id();
        let _ = self.apply_now(change);
        Ok(id)
    }

    /// Applies a change from any actor (this one's own, read back from
    /// disk, included). Out-of-order changes wait in [`Log::pending`]
    /// until what they depend on has arrived; a change that doesn't fit
    /// is refused and listed in [`Log::rejected`].
    pub fn apply(&mut self, change: Change) -> Result<Applied, LogError> {
        let id = change.id();
        if self.frontier.includes(id) {
            return self.contest(change);
        }
        if !self.ready(&change) {
            // Two different changes with one number may both wait; the
            // first taken holds the place and the other contests it.
            if !self.pending.contains(&change) {
                self.pending.push(change);
            }
            return Ok(Applied::Pending);
        }
        let result = self.apply_now(change);
        let unblocked = self.drain_pending();
        self.settle_waiting_claims();
        result.map(|()| Applied::Applied { unblocked })
    }

    /// A change whose number this log has already taken. The same change is
    /// a duplicate. A different one is kept with the other changes that
    /// claim that number and everything is folded again, where the fold
    /// settles the number the same way whatever order they came in: the one
    /// change that passes is taken in and the others listed in
    /// [`Log::forks`]; when none passes, the number is refused; when more
    /// than one passes (a fork, which signing makes impossible without the
    /// key), none is taken in and all are listed.
    fn contest(&mut self, change: Change) -> Result<Applied, LogError> {
        let id = change.id();
        let known = self.changes.get(&id) == Some(&change)
            || self.rejected.iter().any(|(c, _)| *c == change)
            || self.forks.iter().any(|(c, _)| *c == change);
        if known {
            return Ok(Applied::Duplicate);
        }
        self.forks.push((change.clone(), String::new()));
        self.refold();
        if self.changes.get(&id) == Some(&change) {
            Ok(Applied::Applied { unblocked: 0 })
        } else if self
            .forks
            .iter()
            .any(|(c, why)| *c == change && why == FORK_SECOND)
        {
            Err(LogError::Fork { id })
        } else {
            Ok(Applied::Duplicate)
        }
    }

    /// Takes `first` in the fold, or whichever of it and the other changes
    /// claiming its number (`others`) passes; see [`Log::contest`].
    fn take_contested(&mut self, first: Change, others: Vec<Change>) -> bool {
        if others.is_empty() {
            return self.take(first, false).is_ok();
        }
        let mut all = vec![first];
        all.extend(others);
        let passing: Vec<usize> = (0..all.len())
            .filter(|i| self.judge(&all[*i]).is_ok())
            .collect();
        match passing.as_slice() {
            [one] => {
                let winner = all.remove(*one);
                for c in all {
                    self.forks.push((c, FORK_REPLACED.into()));
                }
                self.take(winner, false).is_ok()
            }
            [] => {
                let first = all.remove(0);
                for c in all {
                    self.forks.push((c, FORK_REFUSED_TOO.into()));
                }
                self.take(first, false).is_ok()
            }
            _ => {
                let first = all.remove(0);
                for c in all {
                    self.forks.push((c, FORK_SECOND.into()));
                }
                self.frontier.advance(first.actor, first.seq);
                self.received.push(first.id());
                self.rejected
                    .push((first, format!("{}{FORK_SECOND}", refused::FORK)));
                false
            }
        }
    }

    /// Folds again when a claim on a settled number now has what it depends
    /// on, so it is weighed with the others as if it had come in time.
    fn settle_waiting_claims(&mut self) {
        let due = self
            .forks
            .iter()
            .any(|(c, why)| why == FORK_WAITING && self.frontier.covers(&c.deps));
        if due {
            self.refold();
        }
    }

    fn ready(&self, change: &Change) -> bool {
        change.seq == self.frontier.get(&change.actor) + 1 && self.frontier.covers(&change.deps)
    }

    fn apply_now(&mut self, change: Change) -> Result<(), LogError> {
        self.take(change, true)
    }

    /// Applies or refuses one change whose dependencies are in. With
    /// `refold`, a change that adds a bound judging changes already applied
    /// differently makes the log fold everything again.
    fn take(&mut self, change: Change, refold: bool) -> Result<(), LogError> {
        let id = change.id();
        self.frontier.advance(change.actor, change.seq);
        self.received.push(id);
        if let Err(b) = self.judge(&change) {
            let (reason, e) = *b;
            self.rejected.push((change, reason));
            return Err(e);
        }
        self.state.apply(&change);
        self.order.push(id);
        let mut bounds = self.bounds.clone();
        bounds.note(&self.state, &change);
        self.changes.insert(id, change);
        if bounds != self.bounds && refold {
            let old = std::mem::replace(&mut self.bounds, bounds);
            let judged_again = self.order.iter().any(|c| {
                *c != id
                    && self.changes.get(c).is_some_and(|c| {
                        access::Bounds::changes_verdict(&self.state, &self.bounds, &old, c)
                    })
            });
            if judged_again {
                self.refold();
            }
        } else if refold {
            self.bounds = bounds;
        }
        Ok(())
    }

    /// Folds every change taken in again from an empty state, in the order
    /// they were taken, judged with the bounds known now, until the bounds
    /// the fold ends with are the ones it was judged with (a removal can be
    /// undone by its author's own removal). Rare: only when a change arrives
    /// that judges changes applied before it.
    fn refold(&mut self) {
        self.folds += 1;
        for _ in 0..16 {
            let mut taken: BTreeMap<ChangeId, Change> = std::mem::take(&mut self.changes);
            for (c, _) in std::mem::take(&mut self.rejected) {
                taken.entry(c.id()).or_insert(c);
            }
            let received = std::mem::take(&mut self.received);
            let genesis = self.state.genesis();
            self.state = State::new();
            if let Some(g) = genesis {
                self.state.set_genesis(g);
            }
            self.order.clear();
            self.frontier = Frontier::new();
            let mut fresh = Bounds::default();
            // In the order taken, each once its dependencies are in again (a
            // change that took a refused one's place may depend on later ones).
            let mut waiting: Vec<Change> = received
                .into_iter()
                .filter_map(|id| taken.remove(&id))
                .collect();
            // Every change that claimed a number another has, considered
            // again with the others when that number comes up.
            let mut others: Vec<Change> = std::mem::take(&mut self.forks)
                .into_iter()
                .map(|(c, _)| c)
                .collect();
            // Claims not ready when their number was settled.
            let mut unplaced: Vec<Change> = Vec::new();
            loop {
                // Every ready change whose number nobody else claims.
                let mut progress = false;
                let mut rest = Vec::new();
                for c in waiting {
                    let contested = others.iter().any(|o| o.id() == c.id());
                    if !contested && self.ready(&c) {
                        let id = c.id();
                        if self.take(c, false).is_ok() {
                            let c = self.changes.get(&id).expect("just applied");
                            fresh.note(&self.state, c);
                        }
                        progress = true;
                    } else {
                        rest.push(c);
                    }
                }
                waiting = rest;
                if progress {
                    continue;
                }
                // Then the lowest claimed number with a claim ready, settled
                // among all its claims ready by now: decided only once nothing
                // else can go on, so the same claims are there in any order.
                let Some(id) = waiting
                    .iter()
                    .chain(others.iter())
                    .filter(|c| others.iter().any(|o| o.id() == c.id()) && self.ready(c))
                    .map(Change::id)
                    .min()
                else {
                    break;
                };
                let mut claims: Vec<Change> = Vec::new();
                let (mine, rest): (Vec<Change>, Vec<Change>) =
                    waiting.into_iter().partition(|c| c.id() == id);
                waiting = rest;
                claims.extend(mine);
                let (mine, rest): (Vec<Change>, Vec<Change>) = std::mem::take(&mut others)
                    .into_iter()
                    .partition(|c| c.id() == id);
                others = rest;
                claims.extend(mine);
                let (ready, late): (Vec<Change>, Vec<Change>) =
                    claims.into_iter().partition(|c| self.ready(c));
                unplaced.extend(late);
                let mut ready = ready.into_iter();
                let first = ready.next().expect("a claim is ready");
                if self.take_contested(first, ready.collect()) {
                    let c = self.changes.get(&id).expect("just applied");
                    fresh.note(&self.state, c);
                }
            }
            for c in waiting {
                if !self.pending.contains(&c) {
                    self.pending.push(c);
                }
            }
            // Claims not placed: still waiting for changes that haven't
            // arrived (settled again when they do), or needing their own
            // number's other claim to have been taken in (refused).
            for c in others.into_iter().chain(unplaced) {
                let why = if self.frontier.covers(&c.deps) {
                    FORK_REFUSED_TOO
                } else {
                    FORK_WAITING
                };
                self.forks.push((c, why.into()));
            }
            if fresh == self.bounds {
                return;
            }
            self.bounds = fresh;
        }
    }

    fn drain_pending(&mut self) -> usize {
        let mut applied = 0;
        loop {
            // A waiting change whose number was taken meanwhile contests it.
            if let Some(i) = self
                .pending
                .iter()
                .position(|c| self.frontier.includes(c.id()))
            {
                let change = self.pending.remove(i);
                if matches!(self.contest(change), Ok(Applied::Applied { .. })) {
                    applied += 1;
                }
                continue;
            }
            let Some(i) = self.pending.iter().position(|c| self.ready(c)) else {
                return applied;
            };
            let change = self.pending.remove(i);
            // A refused change is already listed; keep draining.
            let _ = self.apply_now(change);
            applied += 1;
        }
    }

    /// The ops that would undo this actor's change `id` now, and what
    /// would be skipped. Nothing is recorded.
    pub fn inverse(&self, id: ChangeId) -> Result<Inverse, LogError> {
        let change = self
            .changes
            .get(&id)
            .filter(|c| c.actor == self.actor)
            .ok_or(LogError::NotMine { id })?;
        let p = project(&self.state);
        let describe = |uid: Uid| project::describe(&self.state, &p.binding, uid);
        Ok(undo::inverse(&self.state, change, &describe))
    }

    /// Undoes this actor's change `id`: records a new change that puts
    /// back its previous values where they are still current. Returns the
    /// new change (none when nothing could be put back) and the skips.
    pub fn undo(
        &mut self,
        id: ChangeId,
        time: &str,
    ) -> Result<(Option<ChangeId>, Inverse), LogError> {
        let inv = self.inverse(id)?;
        if inv.ops.is_empty() {
            return Ok((None, inv));
        }
        let label = format!(
            "Undo: {}",
            self.changes
                .get(&id)
                .map(|c| c.label.as_str())
                .unwrap_or("")
        );
        let new = self.record_ops(inv.ops.clone(), &label, time)?;
        Ok((Some(new), inv))
    }

    /// Records one write by this actor: `field` of `entity` set to
    /// `value` (a person picking a value back from the history, or the
    /// one that lost to a concurrent write). `None` when the entity is
    /// unknown.
    pub fn set_field(
        &mut self,
        entity: Uid,
        field: Field,
        value: Value,
        label: &str,
        time: &str,
    ) -> Result<Option<ChangeId>, LogError> {
        if self.state.entity(entity).is_none() {
            return Ok(None);
        }
        let ops = vec![Op::Set {
            entity,
            field,
            value,
        }];
        self.record_ops(ops, label, time).map(Some)
    }

    // Roles (decisions.md, entry 78)

    /// Gives `actor` a role, as a change by this actor. Refused unless
    /// this actor sets roles and may give this one.
    pub fn grant(
        &mut self,
        actor: ActorId,
        role: Role,
        label: &str,
        time: &str,
    ) -> Result<ChangeId, LogError> {
        self.record_ops(vec![Op::Grant { actor, role }], label, time)
    }

    /// Removes `actor` (decisions.md, entry 89): a change by this actor
    /// that makes them read only and keeps aside their changes this actor
    /// hasn't seen. Refused unless this actor may set their role.
    pub fn revoke(
        &mut self,
        actor: ActorId,
        label: &str,
        time: &str,
    ) -> Result<ChangeId, LogError> {
        self.record_ops(vec![Op::Revoke { actor }], label, time)
    }

    /// Publishes this computer's key now, as a change of its own, when it
    /// should have one in the log and hasn't (signing has started, or this
    /// is the owner of a budget with roles). `None` when there is nothing
    /// to publish.
    pub fn publish_key(&mut self, label: &str, time: &str) -> Result<Option<ChangeId>, LogError> {
        let probe = Change {
            actor: self.actor,
            seq: 0,
            deps: Frontier::new(),
            lamport: 0,
            time: String::new(),
            label: String::new(),
            ops: Vec::new(),
            sig: None,
        };
        if !self.wants_key(&probe) {
            return Ok(None);
        }
        self.record_ops(Vec::new(), label, time).map(Some)
    }

    /// `actor`'s published key.
    pub fn key_of(&self, actor: ActorId) -> Option<PublicKey> {
        self.state.key_of(actor).map(|(k, _)| k)
    }

    /// Whether signing has started for this budget: the owner's key is in
    /// the log.
    pub fn signing_started(&self) -> bool {
        self.bounds.signing.is_some()
    }

    /// Whether `actor` (a person) was removed and not given a role since.
    pub fn removed(&self, actor: ActorId) -> bool {
        self.state
            .grants()
            .get(&actor)
            .and_then(|r| access::grant_write_at(r, &self.frontier))
            .is_some_and(|w| w.removed)
    }

    /// The bounds the log judges with (the owner's first role and key, and
    /// every removal).
    pub fn bounds(&self) -> &Bounds {
        &self.bounds
    }

    /// Who started sharing.
    pub fn owner(&self) -> Option<ActorId> {
        self.state.owner()
    }

    /// `actor`'s role now, as this log knows it.
    pub fn role_of(&self, actor: ActorId) -> Role {
        access::role_at(&self.state, actor, &self.frontier)
    }

    /// Whether the owner has given anyone a role: from then on, someone
    /// without one reads only.
    pub fn roles_started(&self) -> bool {
        access::started_at(self.state.grants(), &self.frontier)
    }

    /// Everyone given a role, with the current one.
    pub fn members(&self) -> Vec<(ActorId, Role)> {
        self.state
            .grants()
            .keys()
            .map(|a| (*a, self.role_of(*a)))
            .collect()
    }

    /// Says, as a change by this actor, that it writes for `person` (a
    /// command line or agent with an actor of its own; decisions.md, entry
    /// 78). Refused once this actor writes for someone.
    pub fn acts_for(
        &mut self,
        person: ActorId,
        label: &str,
        time: &str,
    ) -> Result<ChangeId, LogError> {
        self.record_ops(vec![Op::ActsFor { person }], label, time)
    }

    /// The person `actor` writes for, or `actor` itself.
    pub fn person_of(&self, actor: ActorId) -> ActorId {
        access::person_at(&self.state, actor, &self.frontier)
    }

    /// The applied changes from the `from`th one on, in applied order
    /// (what arrived since the log had `from` changes).
    pub fn changes_from(&self, from: usize) -> impl Iterator<Item = &Change> {
        self.order
            .iter()
            .skip(from)
            .filter_map(|id| self.changes.get(id))
    }

    /// Every write to one field of an entity, oldest first: who set it,
    /// when (its change's time), and to what.
    pub fn blame(&self, entity: Uid, field: Field) -> Vec<(Stamp, String, Value)> {
        let Some(e) = self.state.entity(entity) else {
            return Vec::new();
        };
        let Some(reg) = e.fields.get(&field) else {
            return Vec::new();
        };
        reg.history()
            .iter()
            .map(|(s, v)| {
                let time = self
                    .changes
                    .get(&s.change())
                    .map(|c| c.time.clone())
                    .unwrap_or_default();
                (*s, time, v.clone())
            })
            .collect()
    }

    /// Review items that need the changes' dependencies: values that lost
    /// to a concurrent write, moves that lost, deletes that won over
    /// concurrent edits.
    fn concurrency_reviews(&self, p: &mut Projection) {
        let deps = |s: Stamp| self.changes.get(&s.change()).map(|c| &c.deps);
        // The loser's change is unknown to the winner: neither saw the other.
        let concurrent = |winner: Stamp, loser: Stamp| -> bool {
            winner.change() != loser.change()
                && deps(winner).is_some_and(|d| !d.includes(loser.change()))
                && deps(loser).is_some_and(|d| !d.includes(winner.change()))
        };
        let mut items = Vec::new();
        for (uid, e) in self.state.entities() {
            if *uid == Uid::ROOT && e.fields.is_empty() {
                continue;
            }
            let live = p.binding.keys.contains_key(uid) || *uid == Uid::ROOT;
            if live {
                for (field, reg) in &e.fields {
                    let (ws, wv) = reg.current();
                    let lost: Vec<(Stamp, Value)> = reg
                        .losers()
                        .iter()
                        .filter(|(ls, _)| concurrent(*ws, *ls))
                        .cloned()
                        .collect();
                    if !lost.is_empty() {
                        items.push(Review::Overwritten {
                            entity: *uid,
                            field: *field,
                            kept: (*ws, wv.clone()),
                            lost,
                        });
                    }
                }
                if let Some(reg) = &e.place {
                    let ws = reg.stamp();
                    let lost: Vec<Stamp> = reg
                        .losers()
                        .iter()
                        .map(|(s, _)| *s)
                        .filter(|ls| concurrent(ws, *ls))
                        .collect();
                    if !lost.is_empty() {
                        items.push(Review::MovedTwice {
                            entity: *uid,
                            kept: ws,
                            lost,
                        });
                    }
                }
            } else if e.is_deleted() {
                let ds = e.deleted.stamp();
                let edits: Vec<Stamp> = e
                    .fields
                    .values()
                    .flat_map(|r| r.history().iter().map(|(s, _)| *s))
                    .chain(
                        e.place
                            .iter()
                            .flat_map(|r| r.history().iter().map(|(s, _)| *s)),
                    )
                    .filter(|s| concurrent(ds, *s))
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                if !edits.is_empty() {
                    items.push(Review::DeletedWhileEdited {
                        entity: *uid,
                        deleted: ds,
                        edits,
                    });
                }
            }
        }
        for review in items {
            let sentence = project::sentence_for(&self.state, &p.binding, &review);
            p.review.push(ReviewItem { review, sentence });
        }
    }
}
