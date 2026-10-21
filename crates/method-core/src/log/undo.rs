//! Per-person undo (docs/research/collaboration.md, section 8): undoing
//! one of my changes is a new change that puts back my previous values,
//! field by field, only where my value is still the current one. A field
//! someone changed after me is skipped, and the skip is reported.

use super::clock::Stamp;
use super::ids::Uid;
use super::op::{Change, Op, Placement};
use super::state::State;

/// The inverse of a change: the ops that undo it, and what couldn't be
/// undone because someone else wrote since.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inverse {
    pub ops: Vec<Op>,
    /// One sentence per skipped op.
    pub skipped: Vec<String>,
}

impl Inverse {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// The ops that undo `change` against `state` as it stands now.
/// `describe` names an entity for the skip sentences.
pub fn inverse(state: &State, change: &Change, describe: &dyn Fn(Uid) -> String) -> Inverse {
    let mut out = Inverse::default();
    let skip = |out: &mut Inverse, uid: Uid, what: &str, by: Stamp| {
        out.skipped.push(format!(
            "{} {what} was not put back: {} changed it after you.",
            describe(uid),
            by.actor.short()
        ));
    };
    for (index, op) in change.ops.iter().enumerate().rev() {
        let stamp = change.stamp(index);
        let entity = op.entity();
        let Some(e) = state.entity(entity) else {
            continue;
        };
        match op {
            Op::Create { .. } => {
                if !e.is_deleted() {
                    out.ops.push(Op::Delete { entity });
                }
            }
            Op::Set { field, .. } => {
                let Some(reg) = e.fields.get(field) else {
                    continue;
                };
                if reg.stamp() != stamp {
                    skip(&mut out, entity, field.name(), reg.stamp());
                    continue;
                }
                if let Some((_, prev)) = reg.before(stamp) {
                    out.ops.push(Op::Set {
                        entity,
                        field: *field,
                        value: prev.clone(),
                    });
                }
            }
            Op::Move { .. } => {
                let Some(reg) = &e.place else { continue };
                if reg.stamp() != stamp {
                    skip(&mut out, entity, "place", reg.stamp());
                    continue;
                }
                if let Some((_, prev)) = reg.before(stamp) {
                    out.ops.push(Op::Move {
                        entity,
                        place: Placement {
                            list: prev.list,
                            pos: prev.pos,
                            node: None,
                        },
                    });
                }
            }
            Op::Delete { .. } => {
                if e.deleted.stamp() == stamp {
                    out.ops.push(Op::Restore { entity });
                } else {
                    skip(&mut out, entity, "deletion", e.deleted.stamp());
                }
            }
            Op::Restore { .. } => {
                if e.deleted.stamp() == stamp {
                    out.ops.push(Op::Delete { entity });
                } else {
                    skip(&mut out, entity, "restore", e.deleted.stamp());
                }
            }
            Op::Add { field, member, .. } => {
                let mine = e
                    .sets
                    .get(field)
                    .map(|s| s.tags(member))
                    .unwrap_or_default();
                if mine.contains(&stamp) {
                    out.ops.push(Op::Remove {
                        entity,
                        field: *field,
                        member: member.clone(),
                        seen: vec![stamp],
                    });
                }
            }
            Op::Remove { field, member, .. } => {
                let present = e.sets.get(field).is_some_and(|s| s.contains(member));
                if !present {
                    out.ops.push(Op::Add {
                        entity,
                        field: *field,
                        member: member.clone(),
                    });
                }
            }
            // A role is set again in the Share dialog, never undone.
            Op::Grant { .. } | Op::ActsFor { .. } | Op::Key { .. } | Op::Revoke { .. } => {}
        }
    }
    out
}
