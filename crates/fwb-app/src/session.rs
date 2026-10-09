//! One open budget: the authoritative [`Budget`], where it lives on disk, whether
//! it has unsaved changes, and undo/redo history. No Tauri types here, so the
//! whole edit -> recalc -> view path is testable without a window.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use fwb_core::{selection_totals, starter, Budget, RowId};

use crate::diagnose::{lenient_recalc, Outcome};
use crate::edit::{self, EditResult};
use crate::report_dto::SelectionDto;
use crate::view::View;

/// Undo steps kept. Snapshots are whole budgets, which are small.
pub const UNDO_LIMIT: usize = 200;

#[derive(Clone, Debug)]
struct Snapshot {
    budget: Budget,
    rev: u64,
}

#[derive(Debug)]
pub struct Session {
    budget: Budget,
    path: Option<PathBuf>,
    /// Identifies the current state. Every change gets a fresh number, and undo
    /// brings the old number back, so "dirty" is just `rev != saved_rev`.
    rev: u64,
    next_rev: u64,
    /// The rev that matches the file on disk (or the fresh budget).
    saved_rev: u64,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    outcome: Option<Outcome>,
}

impl Default for Session {
    fn default() -> Session {
        Session::new()
    }
}

impl Session {
    /// A session on a fresh starter budget, not yet saved anywhere.
    pub fn new() -> Session {
        Session::with_budget(starter::budget(), None)
    }

    pub fn with_budget(budget: Budget, path: Option<PathBuf>) -> Session {
        Session {
            budget,
            path,
            rev: 0,
            next_rev: 1,
            saved_rev: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            outcome: None,
        }
    }

    pub fn budget(&self) -> &Budget {
        &self.budget
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.rev != self.saved_rev
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The file name shown in the title bar.
    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.fwb".to_string())
    }

    /// Reads a budget file and starts a fresh session on it.
    pub fn open(path: &Path) -> Result<Session, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
        let budget = fwb_format::from_str(&text)
            .map_err(|e| format!("{} isn't a readable budget: {e}", path.display()))?;
        Ok(Session::with_budget(budget, Some(path.to_path_buf())))
    }

    /// Writes the budget to `path` (through a temporary file in the same folder,
    /// so a failed write never leaves half a file), then remembers the path and
    /// marks the budget saved.
    pub fn save_to(&mut self, path: &Path) -> Result<(), String> {
        let text = fwb_format::to_string(&self.budget);
        write_atomically(path, text.as_bytes())
            .map_err(|e| format!("Couldn't save {}: {e}", path.display()))?;
        self.path = Some(path.to_path_buf());
        self.saved_rev = self.rev;
        Ok(())
    }

    /// Applies `f` to a copy of the budget. On success the copy becomes current
    /// and the old budget goes on the undo stack; on error nothing changes. An
    /// edit that changes nothing leaves history and the dirty flag alone.
    pub fn edit<T>(&mut self, f: impl FnOnce(&mut Budget) -> EditResult<T>) -> EditResult<T> {
        let mut next = self.budget.clone();
        let out = f(&mut next)?;
        if next != self.budget {
            let old = std::mem::replace(&mut self.budget, next);
            self.undo.push(Snapshot {
                budget: old,
                rev: self.rev,
            });
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.rev = self.next_rev;
            self.next_rev += 1;
            self.outcome = None;
        }
        Ok(out)
    }

    pub fn undo(&mut self) -> bool {
        let Some(s) = self.undo.pop() else {
            return false;
        };
        let cur = Snapshot {
            budget: std::mem::replace(&mut self.budget, s.budget),
            rev: self.rev,
        };
        self.redo.push(cur);
        self.rev = s.rev;
        self.outcome = None;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(s) = self.redo.pop() else {
            return false;
        };
        let cur = Snapshot {
            budget: std::mem::replace(&mut self.budget, s.budget),
            rev: self.rev,
        };
        self.undo.push(cur);
        self.rev = s.rev;
        self.outcome = None;
        true
    }

    /// The (cached) lenient recalc of the current budget.
    pub fn outcome(&mut self) -> &Outcome {
        self.outcome
            .get_or_insert_with(|| lenient_recalc(&self.budget))
    }

    /// Everything the UI shows.
    pub fn view(&mut self) -> View {
        self.view_with(None)
    }

    /// The view, noting a row an edit just created so the UI can select it.
    pub fn view_with(&mut self, created_row: Option<RowId>) -> View {
        self.outcome();
        let outcome = self.outcome.as_ref().expect("just computed");
        View::build(
            &self.budget,
            outcome,
            self.path.as_deref(),
            self.display_name(),
            self.is_dirty(),
            self.can_undo(),
            self.can_redo(),
            created_row,
        )
    }

    /// Totals for the selected rows, or zeros when totals can't be computed.
    pub fn selection(&mut self, ids: &[RowId]) -> SelectionDto {
        let ids: BTreeSet<RowId> = ids.iter().copied().collect();
        self.outcome();
        let outcome = self.outcome.as_ref().expect("just computed");
        let Some(report) = &outcome.report else {
            return SelectionDto::default();
        };
        let budget = outcome.sanitized.as_ref().unwrap_or(&self.budget);
        selection_totals(budget, report, &ids)
            .map(|s| SelectionDto::from(&s))
            .unwrap_or_default()
    }

    // Convenience wrappers used by the commands and tests.

    pub fn set_detail_field(&mut self, id: RowId, field: &str, value: &str) -> EditResult {
        let field = edit::Field::parse(field)?;
        self.edit(|b| edit::set_detail_field(b, id, field, value))
    }

    pub fn add_detail(
        &mut self,
        account: &str,
        after: Option<RowId>,
        kind: &str,
    ) -> EditResult<RowId> {
        let kind = edit::parse_kind(kind)?;
        self.edit(|b| edit::add_detail(b, account, after, kind))
    }
}

/// Writes `bytes` to a temporary sibling of `path`, then renames it over `path`.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "budget".into());
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}
