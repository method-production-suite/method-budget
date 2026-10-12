//! What a person should look at after a merge (docs/research/
//! collaboration.md, 3.8 and 3.10): repairs the projection made, values
//! that lost to a concurrent write, and things kept as written but
//! doubtful. Every item is derived from the log, never written to it; a
//! person settles it with an ordinary edit.

use std::fmt;

use super::clock::Stamp;
use super::ids::Uid;
use super::op::{Field, Kind, Value};
use crate::model::RowId;

/// One thing to review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Review {
    /// Two live rows had one id; the later one shows with `now`.
    DuplicateRowId { row: Uid, was: RowId, now: RowId },
    /// Two live items of one kind had one name and differ; the later one
    /// shows as `now`.
    DuplicateName {
        kind: Kind,
        kept: Uid,
        renamed: Uid,
        name: String,
        now: String,
    },
    /// A deleted item was kept because `used_by` still uses it, or is
    /// inside it.
    Revived { item: Uid, used_by: Uid },
    /// Two live accounts or categories have one number.
    DuplicateCode {
        kind: Kind,
        code: String,
        entities: Vec<Uid>,
    },
    /// An account number that doesn't fit the budget's numbering.
    BadCode {
        account: Uid,
        code: String,
        reason: String,
    },
    /// Globals that refer back to themselves.
    CircularGlobals { names: Vec<String> },
    /// A field written by two people at once: `kept` won, `lost` is
    /// what the others wrote.
    Overwritten {
        entity: Uid,
        field: Field,
        kept: (Stamp, Value),
        lost: Vec<(Stamp, Value)>,
    },
    /// Moved by two people at once: the later move won.
    MovedTwice {
        entity: Uid,
        kept: Stamp,
        lost: Vec<Stamp>,
    },
    /// Deleted by one person while another edited it: the delete won.
    DeletedWhileEdited {
        entity: Uid,
        deleted: Stamp,
        edits: Vec<Stamp>,
    },
}

impl Review {
    /// A short key for the kind of item, for filtering and the application.
    pub fn key(&self) -> &'static str {
        match self {
            Review::DuplicateRowId { .. } => "duplicate_row_id",
            Review::DuplicateName { .. } => "duplicate_name",
            Review::Revived { .. } => "revived",
            Review::DuplicateCode { .. } => "duplicate_code",
            Review::BadCode { .. } => "bad_code",
            Review::CircularGlobals { .. } => "circular_globals",
            Review::Overwritten { .. } => "overwritten",
            Review::MovedTwice { .. } => "moved_twice",
            Review::DeletedWhileEdited { .. } => "deleted_while_edited",
        }
    }

    /// The entity the item is mainly about, if one.
    pub fn entity(&self) -> Option<Uid> {
        match self {
            Review::DuplicateRowId { row, .. } => Some(*row),
            Review::DuplicateName { renamed, .. } => Some(*renamed),
            Review::Revived { item, .. } => Some(*item),
            Review::DuplicateCode { entities, .. } => entities.first().copied(),
            Review::BadCode { account, .. } => Some(*account),
            Review::CircularGlobals { .. } => None,
            Review::Overwritten { entity, .. }
            | Review::MovedTwice { entity, .. }
            | Review::DeletedWhileEdited { entity, .. } => Some(*entity),
        }
    }
}

/// A review item with its sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewItem {
    pub review: Review,
    pub sentence: String,
}

impl fmt::Display for ReviewItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.sentence)
    }
}
