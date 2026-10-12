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
pub mod state;
pub mod undo;

use std::collections::BTreeMap;
use std::fmt;

pub use capture::Hint;
pub use clock::{ChangeId, Frontier, Stamp};
pub use ids::{ActorId, Ids, Uid};
pub use op::{Change, Field, Kind, ListKey, ListName, Member, Op, Placement, Value};
pub use project::{project, Binding, Projection};
pub use review::{Review, ReviewItem};
pub use seq::{Node, PosId, SeqTree, Side};
pub use state::{Entity, Malformed, OrSet, Place, Register, State};
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
}

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogError::Malformed { id, reason } => write!(f, "change {id} is malformed: {reason}"),
            LogError::Fork { id } => {
                write!(f, "change {id} differs from the one already applied")
            }
            LogError::NotMine { id } => write!(f, "change {id} is not one of this actor's"),
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
#[derive(Clone, Debug)]
pub struct Log {
    actor: ActorId,
    ids: Ids,
    state: State,
    changes: BTreeMap<ChangeId, Change>,
    order: Vec<ChangeId>,
    frontier: Frontier,
    pending: Vec<Change>,
    rejected: Vec<(Change, String)>,
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
        }
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

    /// This actor's own changes with a seq above `seq`, in order.
    pub fn own_after(&self, seq: u64) -> Vec<&Change> {
        self.changes()
            .filter(|c| c.actor == self.actor && c.seq > seq)
            .collect()
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
    ) -> Result<Option<ChangeId>, Malformed> {
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
        let change = Change {
            actor: self.actor,
            seq,
            deps: self.frontier.clone(),
            lamport,
            time: time.to_string(),
            label: label.to_string(),
            ops,
        };
        self.state.check(&change)?;
        let id = change.id();
        // Checked just above, so this cannot refuse it.
        let _ = self.apply_now(change);
        Ok(Some(id))
    }

    /// Records a change made of `ops` by this actor (an undo, a change
    /// built by a tool).
    fn record_ops(&mut self, ops: Vec<Op>, label: &str, time: &str) -> Result<ChangeId, Malformed> {
        let change = Change {
            actor: self.actor,
            seq: self.own_seq() + 1,
            deps: self.frontier.clone(),
            lamport: self.state.lamport() + 1,
            time: time.to_string(),
            label: label.to_string(),
            ops,
        };
        self.state.check(&change)?;
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
            return match self.changes.get(&id) {
                Some(have) if *have == change => Ok(Applied::Duplicate),
                Some(_) => Err(LogError::Fork { id }),
                // Rejected earlier: the frontier moved past it.
                None => Ok(Applied::Duplicate),
            };
        }
        if !self.ready(&change) {
            if !self.pending.iter().any(|p| p.id() == id) {
                self.pending.push(change);
            }
            return Ok(Applied::Pending);
        }
        let result = self.apply_now(change);
        let unblocked = self.drain_pending();
        result.map(|()| Applied::Applied { unblocked })
    }

    fn ready(&self, change: &Change) -> bool {
        change.seq == self.frontier.get(&change.actor) + 1 && self.frontier.covers(&change.deps)
    }

    fn apply_now(&mut self, change: Change) -> Result<(), LogError> {
        let id = change.id();
        self.frontier.advance(change.actor, change.seq);
        match self.state.check(&change) {
            Ok(()) => {
                self.state.apply(&change);
                self.order.push(id);
                self.changes.insert(id, change);
                Ok(())
            }
            Err(m) => {
                let reason = m.to_string();
                self.rejected.push((change, reason.clone()));
                Err(LogError::Malformed { id, reason })
            }
        }
    }

    fn drain_pending(&mut self) -> usize {
        let mut applied = 0;
        loop {
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
        let new = self
            .record_ops(inv.ops.clone(), &label, time)
            .map_err(|m| LogError::Malformed {
                id,
                reason: m.to_string(),
            })?;
        Ok((Some(new), inv))
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
