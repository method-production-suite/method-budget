//! What a session asks of a budget shared with other people (decisions.md,
//! entries 62 and 64). The desktop's shared folder (method-app's
//! src/shared.rs) implements it; the session only records edits through
//! it, takes undo and redo from it and lists its steps, so a browser host
//! could share another way (a server) without changing the session.

use std::any::Any;
use std::collections::HashMap;

use method_core::log::Hint;
use method_core::{Budget, RowId};

use crate::session::History;

/// A shared budget's change log, as a session uses it.
pub trait SharedLog: std::fmt::Debug + Send + Any {
    /// Anything of this person's not yet shared (what Save and the close
    /// question look at).
    fn is_dirty(&self) -> bool;
    fn can_undo(&self) -> bool;
    fn can_redo(&self) -> bool;
    /// Gives rows new to `after` (not in `before`) ids nobody else can make.
    /// Returns old id -> new id.
    fn randomize_new_rows(&mut self, before: &Budget, after: &mut Budget) -> HashMap<RowId, RowId>;
    /// Records the edit that made `after` as one change by this person,
    /// with what the command knows that a diff can't see (`hints`). Returns
    /// the budget as the log now projects it when anything was recorded.
    fn record(
        &mut self,
        after: &Budget,
        hints: &[Hint],
        label: &str,
    ) -> Result<Option<Budget>, String>;
    /// Takes back this person's latest step: the budget to adopt and what
    /// couldn't be put back. `None` when there is nothing to undo.
    fn undo(&mut self) -> Option<(Budget, Option<String>)>;
    fn redo(&mut self) -> Option<(Budget, Option<String>)>;
    /// This person's steps as the History list shows them.
    fn history(&self) -> History;
    /// How many steps there are, undone ones included.
    fn history_len(&self) -> usize;
    /// How many steps are applied.
    fn applied_steps(&self) -> usize;
    /// The implementation itself, for the host that made it.
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
