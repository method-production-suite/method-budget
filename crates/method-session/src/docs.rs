//! Several budgets open at once, as tabs in one window (decisions.md, entries
//! 19 and 25). Each open document is its own [`Session`]: budget, file, dirty
//! flag, undo and redo, autosave timing, recovery copy, the file's stamp for
//! changes made outside, and the account on screen. They are kept in tab order
//! with an id each and one of them in front (the active document).
//!
//! [`AppState`] holds them behind one lock. [`AppState::lock`] gives the active
//! document, [`AppState::lock_doc`] a named one (the tools window names the
//! document it is drawing, so a command it sends just as the main window
//! switches tabs still lands where the user meant). Either way the guard
//! derefs to the [`Session`], so a command reads like it did with one budget.
//! No Tauri types and no file system here: everything is tested without a
//! window. The tab commands are the desktop app's (method-app's src/tabs.rs).
//!
//! Split view (roadmap 2.26): the window can show two panes side by side,
//! each with its own tabs and its own budget in front. A budget may be in
//! both panes (two views of one document: one session, so an edit shows in
//! both). One pane is the target: its budget in front is the active
//! document, which the toolbar, the menus, the tools panel and every
//! command without a `doc` act on. [`Split`] keeps which tabs are in which
//! pane; the master tab order (the Window menu's) stays in `tabs`, and
//! closing the split merges the panes back into it.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;

use crate::session::Session;

/// Names an open document for as long as it is open. Never reused in a run.
pub type DocId = u64;

#[derive(Debug)]
struct Doc {
    id: DocId,
    session: Session,
}

/// Every open document, in tab order, and which one is in front.
#[derive(Debug)]
pub struct Docs {
    tabs: Vec<Doc>,
    active: DocId,
    next_id: DocId,
    /// Two panes side by side, or `None` (one pane, every tab in it).
    split: Option<Split>,
}

/// Which tabs are in which of the two panes, the budget in front in each,
/// and which pane is the target. Every open document is in at least one
/// pane, no pane is empty, each front is one of its pane's tabs, and the
/// target's front is the active document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Split {
    pub panes: [Vec<DocId>; 2],
    pub fronts: [DocId; 2],
    pub target: usize,
}

/// One pane as the page draws it: its tabs in order and the one in front.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PaneInfo {
    pub tabs: Vec<DocId>,
    pub front: DocId,
}

/// The window's panes (every view carries them): one, or two side by side,
/// and which is the target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PanesInfo {
    pub split: bool,
    pub target: usize,
    pub panes: Vec<PaneInfo>,
}

/// Split view as `tabs.json` keeps it: places in its list of files.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SavedSplit {
    pub panes: [Vec<usize>; 2],
    pub fronts: [usize; 2],
    pub target: usize,
}

const LAST_IN_PANE: &str = "A pane's only tab can't move to the other pane.";
const NOT_SPLIT: &str = "The window isn't split.";

/// One tab as the page draws it (and `list_tabs` gives it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TabInfo {
    pub id: DocId,
    /// What the tab shows: the file's name without its extension, or the budget's
    /// title while it has no file ([`Session::tab_label`]).
    pub label: String,
    /// The budget's title.
    pub title: String,
    /// The file's name as the title bar shows it (`Untitled.mthd` without one).
    pub name: String,
    pub path: Option<String>,
    /// Unsaved changes.
    pub dirty: bool,
    /// The file changed outside the app and the user hasn't said whose
    /// version to keep: autosave into it is paused.
    pub conflict: bool,
    /// The tab in front.
    pub active: bool,
}

impl Default for Docs {
    /// One untitled starter budget.
    fn default() -> Docs {
        Docs::with(Session::new())
    }
}

impl Docs {
    /// `first` as the only tab.
    pub fn with(first: Session) -> Docs {
        Docs {
            tabs: vec![Doc {
                id: 1,
                session: first,
            }],
            active: 1,
            next_id: 2,
            split: None,
        }
    }

    pub fn active(&self) -> DocId {
        self.active
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// Never true: there is always at least one tab.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Every document's id, in tab order.
    pub fn ids(&self) -> Vec<DocId> {
        self.tabs.iter().map(|d| d.id).collect()
    }

    pub fn index_of(&self, id: DocId) -> Option<usize> {
        self.tabs.iter().position(|d| d.id == id)
    }

    pub fn get(&self, id: DocId) -> Option<&Session> {
        self.tabs.iter().find(|d| d.id == id).map(|d| &d.session)
    }

    pub fn get_mut(&mut self, id: DocId) -> Option<&mut Session> {
        self.tabs
            .iter_mut()
            .find(|d| d.id == id)
            .map(|d| &mut d.session)
    }

    fn active_index(&self) -> usize {
        self.index_of(self.active).unwrap_or(0)
    }

    /// The tabs as the page draws them.
    pub fn tabs(&self) -> Vec<TabInfo> {
        self.tabs
            .iter()
            .map(|d| {
                let s = &d.session;
                TabInfo {
                    id: d.id,
                    label: s.tab_label(),
                    title: s.budget().title.clone(),
                    name: s.display_name(),
                    path: s.path().map(|p| p.display().to_string()),
                    dirty: s.is_dirty(),
                    conflict: s.in_conflict(),
                    active: d.id == self.active,
                }
            })
            .collect()
    }

    fn new_id(&mut self) -> DocId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Opens `s` as a new tab at the end (of the target pane, when split)
    /// and brings it to the front.
    pub fn add(&mut self, s: Session) -> DocId {
        let id = self.new_id();
        self.tabs.push(Doc { id, session: s });
        self.active = id;
        if let Some(sp) = &mut self.split {
            sp.panes[sp.target].push(id);
            sp.fronts[sp.target] = id;
        }
        id
    }

    /// Puts `s` in the place of tab `id`, as a new document (a new id), and
    /// brings it to the front. Returns the new id and the session it replaced.
    pub fn replace(&mut self, id: DocId, s: Session) -> Result<(DocId, Session), String> {
        let at = self.index_of(id).ok_or(CLOSED)?;
        let new = self.new_id();
        let old = std::mem::replace(
            &mut self.tabs[at],
            Doc {
                id: new,
                session: s,
            },
        );
        self.active = new;
        if let Some(sp) = &mut self.split {
            for p in 0..2 {
                for t in &mut sp.panes[p] {
                    if *t == id {
                        *t = new;
                    }
                }
                if sp.fronts[p] == id {
                    sp.fronts[p] = new;
                }
            }
            // It comes to the front in the target pane (where an Open lands).
            if !sp.panes[sp.target].contains(&new) {
                sp.target = 1 - sp.target;
            }
            sp.fronts[sp.target] = new;
        }
        Ok((new, old.session))
    }

    /// Brings tab `id` to the front: in the target pane when it is there,
    /// else in the other pane, which becomes the target.
    pub fn switch(&mut self, id: DocId) -> Result<(), String> {
        self.index_of(id).ok_or(CLOSED)?;
        self.active = id;
        if let Some(sp) = &mut self.split {
            if !sp.panes[sp.target].contains(&id) {
                sp.target = 1 - sp.target;
            }
            sp.fronts[sp.target] = id;
        }
        Ok(())
    }

    // Split view

    /// The two panes, when the window is split.
    pub fn split(&self) -> Option<&Split> {
        self.split.as_ref()
    }

    /// The panes as the page draws them.
    pub fn layout(&self) -> PanesInfo {
        match &self.split {
            Some(sp) => PanesInfo {
                split: true,
                target: sp.target,
                panes: (0..2)
                    .map(|p| PaneInfo {
                        tabs: sp.panes[p].clone(),
                        front: sp.fronts[p],
                    })
                    .collect(),
            },
            None => PanesInfo {
                split: false,
                target: 0,
                panes: vec![PaneInfo {
                    tabs: self.ids(),
                    front: self.active,
                }],
            },
        }
    }

    /// Splits the window: the first pane keeps every tab, and a second pane
    /// at its right shows the budget in front too (two views of it). The
    /// new pane is the target. Refused when the window is split already.
    pub fn split_right(&mut self) -> Result<(), String> {
        if self.split.is_some() {
            return Err("The window is split already.".into());
        }
        self.split = Some(Split {
            panes: [self.ids(), vec![self.active]],
            fronts: [self.active, self.active],
            target: 1,
        });
        Ok(())
    }

    /// Closes the split: the panes merge into one, the first pane's tabs
    /// then the second's (each budget once), and the target's budget stays
    /// in front.
    pub fn close_split(&mut self) -> Result<(), String> {
        let sp = self.split.take().ok_or(NOT_SPLIT)?;
        let mut order: Vec<DocId> = Vec::with_capacity(self.tabs.len());
        for id in sp.panes[0].iter().chain(&sp.panes[1]) {
            if !order.contains(id) {
                order.push(*id);
            }
        }
        // Anything not in a pane (never, but keep it) goes at the end.
        for d in &self.tabs {
            if !order.contains(&d.id) {
                order.push(d.id);
            }
        }
        self.tabs
            .sort_by_key(|d| order.iter().position(|o| *o == d.id).unwrap_or(usize::MAX));
        Ok(())
    }

    /// Makes `pane` the target: its budget in front becomes the active one.
    pub fn set_target(&mut self, pane: usize) -> Result<(), String> {
        match &mut self.split {
            Some(sp) if pane < 2 => {
                sp.target = pane;
                self.active = sp.fronts[pane];
                Ok(())
            }
            Some(_) => Err("There is no such pane.".into()),
            None if pane == 0 => Ok(()),
            None => Err(NOT_SPLIT.into()),
        }
    }

    /// Brings tab `id` to the front of `pane` (which becomes the target).
    pub fn switch_in(&mut self, pane: usize, id: DocId) -> Result<(), String> {
        self.index_of(id).ok_or(CLOSED)?;
        match &mut self.split {
            Some(sp) if pane < 2 && sp.panes[pane].contains(&id) => {
                sp.target = pane;
                sp.fronts[pane] = id;
                self.active = id;
                Ok(())
            }
            Some(_) => Err("That budget isn't in that pane.".into()),
            None => self.switch(id),
        }
    }

    /// Moves tab `id` to place `to` among `pane`'s tabs (the end when past
    /// it). One pane: the tab order itself.
    pub fn move_in_pane(&mut self, pane: usize, id: DocId, to: usize) -> Result<(), String> {
        let Some(sp) = &mut self.split else {
            return self.move_tab(id, to);
        };
        let list = sp.panes.get_mut(pane).ok_or("There is no such pane.")?;
        let from = list
            .iter()
            .position(|t| *t == id)
            .ok_or("That budget isn't in that pane.")?;
        let t = list.remove(from);
        let to = to.min(list.len());
        list.insert(to, t);
        Ok(())
    }

    /// Moves tab `id` from pane `from` to the other pane, at place `to`
    /// there (the end when `None` or past it), in front there; that pane
    /// becomes the target. A budget the other pane shows already just comes
    /// to the front there (moved to `to` when given). A pane's only tab
    /// can't move: the pane would be empty.
    pub fn move_to_pane(
        &mut self,
        id: DocId,
        from: usize,
        to: Option<usize>,
    ) -> Result<(), String> {
        let sp = self.split.as_mut().ok_or(NOT_SPLIT)?;
        if from > 1 {
            return Err("There is no such pane.".into());
        }
        let at = sp.panes[from]
            .iter()
            .position(|t| *t == id)
            .ok_or("That budget isn't in that pane.")?;
        if sp.panes[from].len() == 1 {
            return Err(LAST_IN_PANE.into());
        }
        sp.panes[from].remove(at);
        if sp.fronts[from] == id {
            let list = &sp.panes[from];
            sp.fronts[from] = list[at.min(list.len() - 1)];
        }
        let other = 1 - from;
        let dest = &mut sp.panes[other];
        if let Some(i) = dest.iter().position(|t| *t == id) {
            if to.is_some() {
                dest.remove(i);
            }
        }
        if !dest.contains(&id) {
            let to = to.unwrap_or(dest.len()).min(dest.len());
            dest.insert(to, id);
        }
        sp.fronts[other] = id;
        sp.target = other;
        self.active = id;
        Ok(())
    }

    /// Takes tab `id` out of `pane` only, when the other pane shows that
    /// budget too: nothing closes. Returns false (nothing done) otherwise,
    /// when closing the tab means closing the budget.
    pub fn close_in_pane(&mut self, pane: usize, id: DocId) -> bool {
        let Some(sp) = &mut self.split else {
            return false;
        };
        if pane > 1 || !sp.panes[pane].contains(&id) || !sp.panes[1 - pane].contains(&id) {
            return false;
        }
        sp.panes[pane].retain(|t| *t != id);
        if sp.panes[pane].is_empty() {
            // Its last tab: the pane goes, and the other one is all there is.
            self.split = None;
            self.active = id;
            return true;
        }
        if sp.fronts[pane] == id {
            sp.fronts[pane] = sp.panes[pane][0];
        }
        self.active = sp.fronts[sp.target];
        true
    }

    /// Drops document `id` from the panes (it is closing); a pane left empty
    /// ends the split.
    fn unsplit_without(&mut self, id: DocId) {
        let Some(sp) = &mut self.split else {
            return;
        };
        for p in 0..2 {
            let list = &mut sp.panes[p];
            if let Some(at) = list.iter().position(|t| *t == id) {
                list.remove(at);
                if sp.fronts[p] == id && !list.is_empty() {
                    sp.fronts[p] = list[at.min(list.len() - 1)];
                }
            }
        }
        if let Some(left) = (0..2).find(|&p| !sp.panes[p].is_empty() && sp.panes[1 - p].is_empty())
        {
            let front = sp.fronts[left];
            let order = sp.panes[left].clone();
            self.split = None;
            self.tabs
                .sort_by_key(|d| order.iter().position(|o| *o == d.id).unwrap_or(usize::MAX));
            if self.active == id || self.index_of(self.active).is_none() {
                self.active = front;
            }
            return;
        }
        if self.active == id {
            self.active = sp.fronts[sp.target];
        }
    }

    /// Split view to keep in `tabs.json`, by place in the list of files
    /// [`Docs::files`] gives: budgets without a file are left out, and a
    /// pane left with none means no split is kept.
    pub fn saved_split(&self) -> Option<SavedSplit> {
        let sp = self.split.as_ref()?;
        let filed: Vec<DocId> = self
            .tabs
            .iter()
            .filter(|d| d.session.path().is_some())
            .map(|d| d.id)
            .collect();
        let place = |id: &DocId| filed.iter().position(|f| f == id);
        let panes: [Vec<usize>; 2] = [
            sp.panes[0].iter().filter_map(place).collect(),
            sp.panes[1].iter().filter_map(place).collect(),
        ];
        if panes.iter().any(Vec::is_empty) {
            return None;
        }
        let front = |p: usize| place(&sp.fronts[p]).unwrap_or(panes[p][0]);
        Some(SavedSplit {
            fronts: [front(0), front(1)],
            panes,
            target: sp.target,
        })
    }

    /// Puts split view back as `saved` says, `ids` being the documents the
    /// saved list of files opened as (`None` for a file that couldn't be).
    /// Files missing from a pane are left out; a pane left empty means one
    /// pane. True when the window is split afterwards.
    pub fn restore_split(&mut self, saved: &SavedSplit, ids: &[Option<DocId>]) -> bool {
        let pick = |list: &[usize]| -> Vec<DocId> {
            let mut out: Vec<DocId> = Vec::new();
            for i in list {
                if let Some(Some(id)) = ids.get(*i) {
                    if self.index_of(*id).is_some() && !out.contains(id) {
                        out.push(*id);
                    }
                }
            }
            out
        };
        let mut panes = [pick(&saved.panes[0]), pick(&saved.panes[1])];
        if panes.iter().any(Vec::is_empty) {
            return false;
        }
        // Budgets open but in neither pane (opened some other way) join the
        // first pane.
        for d in &self.tabs {
            if !panes[0].contains(&d.id) && !panes[1].contains(&d.id) {
                panes[0].push(d.id);
            }
        }
        let front = |p: usize, list: &Vec<DocId>| match ids.get(saved.fronts[p]) {
            Some(Some(id)) if list.contains(id) => *id,
            _ => list[0],
        };
        let fronts = [front(0, &panes[0]), front(1, &panes[1])];
        let target = saved.target.min(1);
        self.active = fronts[target];
        self.split = Some(Split {
            panes,
            fronts,
            target,
        });
        true
    }

    /// The tab `delta` places after the one in front (before it, for a
    /// negative `delta`), wrapping round.
    pub fn step(&self, delta: isize) -> DocId {
        let n = self.tabs.len() as isize;
        let at = (self.active_index() as isize + delta).rem_euclid(n.max(1));
        self.tabs[at as usize].id
    }

    /// Moves tab `id` so that it ends up at `to` in tab order (the end when
    /// past it). Which tab is in front doesn't change. (The master order:
    /// split, each pane's own order is [`Docs::move_in_pane`].)
    pub fn move_tab(&mut self, id: DocId, to: usize) -> Result<(), String> {
        let from = self.index_of(id).ok_or(CLOSED)?;
        let doc = self.tabs.remove(from);
        let to = to.min(self.tabs.len());
        self.tabs.insert(to, doc);
        Ok(())
    }

    /// Takes tab `id` out and gives back its session. The last tab can't go
    /// (the window closes instead). When it was in front, the tab that takes
    /// its place comes to the front: the next one, or the one before it at
    /// the end.
    pub fn remove(&mut self, id: DocId) -> Result<Session, String> {
        let at = self.index_of(id).ok_or(CLOSED)?;
        if self.tabs.len() == 1 {
            return Err("The last budget can't be closed; close the window instead.".into());
        }
        let doc = self.tabs.remove(at);
        if self.split.is_some() {
            // Split: the tab next to it in its pane takes its place.
            self.unsplit_without(id);
            return Ok(doc.session);
        }
        if self.active == id {
            self.active = self.tabs[at.min(self.tabs.len() - 1)].id;
        }
        Ok(doc.session)
    }

    /// The tab whose budget lives in the file at `path`, if one does, with
    /// `same` telling whether two paths name one file (the desktop sees
    /// through other spellings of a path: method-app's `DocsFiles`).
    pub fn find_file_by(&self, path: &Path, same: impl Fn(&Path, &Path) -> bool) -> Option<DocId> {
        self.tabs
            .iter()
            .find(|d| d.session.path().is_some_and(|p| same(p, path)))
            .map(|d| d.id)
    }

    /// The only tab, when it is an untitled budget nobody has touched: an
    /// Open puts the file in its place instead of opening a second tab.
    pub fn lone_untouched(&self) -> Option<DocId> {
        match self.tabs.as_slice() {
            [only] if only.session.path().is_none() && only.session.is_untouched() => Some(only.id),
            _ => None,
        }
    }

    /// The tabs with unsaved changes, in tab order.
    pub fn dirty(&self) -> Vec<DocId> {
        self.tabs
            .iter()
            .filter(|d| d.session.is_dirty())
            .map(|d| d.id)
            .collect()
    }

    /// What to reopen at the next start: the files open as tabs (budgets
    /// without one aren't listed) and the place among them of the one in
    /// front, if it has a file.
    pub fn files(&self) -> (Vec<PathBuf>, Option<usize>) {
        let mut files = Vec::new();
        let mut active = None;
        for d in &self.tabs {
            if let Some(p) = d.session.path() {
                if d.id == self.active {
                    active = Some(files.len());
                }
                files.push(p.to_path_buf());
            }
        }
        (files, active)
    }
}

const CLOSED: &str = "That budget has been closed.";

/// Every open budget, behind one lock.
#[derive(Debug, Default)]
pub struct AppState {
    docs: Mutex<Docs>,
}

impl AppState {
    pub fn with(docs: Docs) -> AppState {
        AppState {
            docs: Mutex::new(docs),
        }
    }

    /// Every open document. A panic mid-command can't leave a budget
    /// half-edited (edits apply to a copy), so a poisoned lock is still safe.
    pub fn docs(&self) -> MutexGuard<'_, Docs> {
        self.docs.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The document in front.
    pub fn lock(&self) -> DocGuard<'_> {
        let docs = self.docs();
        let at = docs.active_index();
        DocGuard { docs, at }
    }

    /// Document `doc`, or the one in front when `doc` is `None`. Refused
    /// when that document has been closed.
    pub fn lock_doc(&self, doc: Option<DocId>) -> Result<DocGuard<'_>, String> {
        let docs = self.docs();
        let at = match doc {
            Some(id) => docs.index_of(id).ok_or(CLOSED)?,
            None => docs.active_index(),
        };
        Ok(DocGuard { docs, at })
    }
}

/// One open document, with every other one locked too. Derefs to its
/// [`Session`].
pub struct DocGuard<'a> {
    docs: MutexGuard<'a, Docs>,
    at: usize,
}

impl DocGuard<'_> {
    pub fn id(&self) -> DocId {
        self.docs.tabs[self.at].id
    }

    /// Whether this document is the one in front.
    pub fn is_active(&self) -> bool {
        self.id() == self.docs.active
    }

    /// Every open document (to list the tabs, say).
    pub fn docs(&self) -> &Docs {
        &self.docs
    }

    /// Brings this document's tab to the front (split: in the target pane
    /// when it is there, else in the other one, which becomes the target).
    pub fn bring_to_front(&mut self) {
        let id = self.id();
        let _ = self.docs.switch(id);
    }
}

impl Deref for DocGuard<'_> {
    type Target = Session;

    fn deref(&self) -> &Session {
        &self.docs.tabs[self.at].session
    }
}

impl DerefMut for DocGuard<'_> {
    fn deref_mut(&mut self) -> &mut Session {
        &mut self.docs.tabs[self.at].session
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit;

    fn titled(t: &str) -> Session {
        let mut s = Session::new();
        s.edit("Test edit", |b| edit::set_title(b, t)).unwrap();
        s
    }

    fn labels(d: &Docs) -> Vec<String> {
        d.tabs().into_iter().map(|t| t.label).collect()
    }

    #[test]
    fn tabs_open_switch_move_and_close() {
        let mut d = Docs::default();
        assert_eq!(d.len(), 1);
        let first = d.active();
        let b = d.add(titled("B"));
        let c = d.add(titled("C"));
        assert_eq!(d.active(), c, "a new tab comes to the front");
        assert_eq!(labels(&d), ["Untitled feature", "B", "C"]);
        assert_eq!(d.step(1), first, "next wraps round");
        assert_eq!(d.step(-1), b);
        d.switch(b).unwrap();
        assert!(d.tabs().iter().all(|t| t.active == (t.id == b)));

        // Moving keeps the tab in front.
        d.move_tab(first, 2).unwrap();
        assert_eq!(d.ids(), [b, c, first]);
        d.move_tab(first, 99).unwrap();
        assert_eq!(d.ids(), [b, c, first]);
        d.move_tab(c, 0).unwrap();
        assert_eq!(d.ids(), [c, b, first]);
        assert_eq!(d.active(), b);

        // Closing the tab in front brings the next one forward, or the one
        // before it at the end.
        assert_eq!(d.remove(b).unwrap().budget().title, "B");
        assert_eq!(d.active(), first);
        d.remove(first).unwrap();
        assert_eq!(d.active(), c);
        assert!(d.remove(c).is_err(), "the last tab stays");
        assert!(d.switch(b).is_err() && d.move_tab(b, 0).is_err());
        assert!(d.get(b).is_none());
        // Ids are never reused.
        let e = d.add(Session::new());
        assert!(e > c && e != b);
    }

    #[test]
    fn untouched_untitled_tabs_and_files() {
        let mut d = Docs::default();
        let first = d.active();
        assert_eq!(d.lone_untouched(), Some(first));
        // Changed and changed back is still touched: it has undo history.
        d.get_mut(first)
            .unwrap()
            .edit("Test edit", |b| edit::set_title(b, "X"))
            .unwrap();
        d.get_mut(first).unwrap().undo();
        assert!(!d.get(first).unwrap().is_dirty());
        assert_eq!(d.lone_untouched(), None);

        let mut d = Docs::default();
        let first = d.active();
        let (b, old) = d.replace(first, titled("B")).unwrap();
        assert_ne!(b, first, "a new document gets a new id");
        assert_eq!(old.budget().title, "Untitled feature");
        assert_eq!(d.ids(), [b]);
        assert_eq!(d.active(), b);
        assert_eq!(d.lone_untouched(), None, "B has been edited");
        d.add(Session::new());
        assert_eq!(d.lone_untouched(), None, "not the only tab");
        assert_eq!(d.dirty(), [b]);
    }

    /// Split view's panes: what each holds and shows, the target, and every
    /// invariant (each pane non-empty, fronts in their panes, every budget in
    /// a pane, the target's front the active one).
    fn check(d: &Docs) {
        let Some(sp) = d.split() else {
            return;
        };
        for p in 0..2 {
            assert!(!sp.panes[p].is_empty(), "pane {p} empty");
            assert!(sp.panes[p].contains(&sp.fronts[p]), "front of {p}");
            for id in &sp.panes[p] {
                assert!(d.get(*id).is_some(), "{id} in pane {p} is open");
            }
        }
        for id in d.ids() {
            assert!(sp.panes.iter().any(|l| l.contains(&id)), "{id} in a pane");
        }
        assert_eq!(d.active(), sp.fronts[sp.target]);
    }

    #[test]
    fn split_view_shows_one_budget_in_two_panes() {
        let mut d = Docs::default();
        let a = d.active();
        let b = d.add(titled("B"));
        assert!(d.close_split().is_err(), "not split");
        assert!(!d.layout().split);
        assert_eq!(d.layout().panes[0].tabs, [a, b]);
        d.split_right().unwrap();
        check(&d);
        assert!(d.split_right().is_err(), "split already");
        // The right pane shows the budget in front too, and is the target.
        let l = d.layout();
        assert!(l.split);
        assert_eq!(l.target, 1);
        assert_eq!(l.panes[0].tabs, [a, b]);
        assert_eq!(l.panes[1].tabs, [b]);
        assert_eq!((l.panes[0].front, l.panes[1].front), (b, b));
        assert_eq!(d.active(), b);

        // Each pane keeps its own budget in front; the target's is active.
        d.switch_in(0, a).unwrap();
        check(&d);
        assert_eq!(d.active(), a);
        assert_eq!(d.layout().target, 0);
        d.set_target(1).unwrap();
        assert_eq!(d.active(), b, "the right pane still shows B");
        assert_eq!(d.layout().panes[0].front, a);
        assert!(d.switch_in(1, a).is_err(), "A isn't in the right pane");
        assert!(d.set_target(2).is_err());

        // A new budget opens in the target pane.
        let c = d.add(titled("C"));
        check(&d);
        assert_eq!(d.layout().panes[1].tabs, [b, c]);
        assert_eq!(d.active(), c);
        // Window > a budget only in the other pane: that pane becomes the target.
        d.switch(a).unwrap();
        check(&d);
        assert_eq!(d.layout().target, 0);
        assert_eq!(d.layout().panes[1].front, c);

        // The same session either way: an edit shows in both panes.
        d.get_mut(b)
            .unwrap()
            .edit("Test edit", |bud| edit::set_title(bud, "Both"))
            .unwrap();
        let l = d.layout();
        assert!(l.panes[0].tabs.contains(&b) && l.panes[1].tabs.contains(&b));
        assert_eq!(d.get(b).unwrap().budget().title, "Both");
    }

    #[test]
    fn tabs_move_between_panes_and_merge_back() {
        let mut d = Docs::default();
        let a = d.active();
        let b = d.add(titled("B"));
        let c = d.add(titled("C"));
        d.split_right().unwrap(); // left [a b c], right [c]
                                  // Reordering is per pane.
        d.move_in_pane(0, a, 2).unwrap();
        assert_eq!(d.layout().panes[0].tabs, [b, c, a]);
        assert_eq!(d.ids(), [a, b, c], "the master order is kept");
        assert!(d.move_in_pane(1, a, 0).is_err(), "A isn't on the right");
        // Moving A right: out of the left pane, in front on the right.
        d.move_to_pane(a, 0, Some(0)).unwrap();
        check(&d);
        let l = d.layout();
        assert_eq!(l.panes[0].tabs, [b, c]);
        assert_eq!(l.panes[1].tabs, [a, c]);
        assert_eq!((l.target, l.panes[1].front), (1, a));
        // C is on both sides: moving it left just brings it forward there.
        d.move_to_pane(c, 1, None).unwrap();
        check(&d);
        let l = d.layout();
        assert_eq!(l.panes[0].tabs, [b, c]);
        assert_eq!(l.panes[1].tabs, [a]);
        assert_eq!((l.target, l.panes[0].front), (0, c));
        // A pane's only tab stays.
        assert_eq!(d.move_to_pane(a, 1, None).unwrap_err(), LAST_IN_PANE);
        assert!(d.move_to_pane(b, 1, None).is_err(), "B isn't on the right");
        // Merging: the left pane's tabs, then the right's, the target's
        // budget in front.
        d.close_split().unwrap();
        assert!(d.split().is_none());
        assert_eq!(d.ids(), [b, c, a]);
        assert_eq!(d.active(), c);
        assert_eq!(d.layout().panes.len(), 1);
    }

    #[test]
    fn closing_in_a_pane_and_closing_a_budget() {
        let mut d = Docs::default();
        let a = d.active();
        let b = d.add(titled("B"));
        d.split_right().unwrap(); // left [a b], right [b]
                                  // B is in both panes: closing the left tab only takes it from there.
        assert!(d.close_in_pane(0, b));
        check(&d);
        assert_eq!(d.layout().panes[0].tabs, [a]);
        assert_eq!(d.layout().panes[0].front, a);
        assert_eq!(d.get(b).unwrap().budget().title, "B", "still open");
        // A is only on the left: closing it means closing the budget.
        assert!(!d.close_in_pane(0, a));
        assert!(!d.close_in_pane(1, a));

        // Closing a budget takes it out of its panes; a pane left empty ends
        // the split.
        let c = d.add(titled("C")); // the target is the right pane
        assert_eq!(d.layout().panes[1].tabs, [b, c]);
        d.remove(c).unwrap();
        check(&d);
        assert_eq!(d.active(), b, "the tab beside it in its pane");
        d.remove(a).unwrap();
        assert!(d.split().is_none(), "the left pane had nothing else");
        assert_eq!(d.ids(), [b]);
        assert_eq!(d.active(), b);

        // The last tab of a pane closing on its own (in both panes) ends
        // the split too.
        let e = d.add(titled("E"));
        d.split_right().unwrap(); // left [b e], right [e]
        assert!(d.close_in_pane(1, e));
        assert!(d.split().is_none());
        assert_eq!(d.ids(), [b, e]);
        assert_eq!(d.active(), e);
    }

    #[test]
    fn an_open_in_place_of_an_untouched_tab_keeps_its_pane() {
        let mut d = Docs::default();
        let a = d.active();
        d.split_right().unwrap(); // left [a], right [a]
        let (n, _) = d.replace(a, titled("N")).unwrap();
        check(&d);
        let l = d.layout();
        assert_eq!(
            (l.panes[0].tabs.clone(), l.panes[1].tabs.clone()),
            (vec![n], vec![n])
        );
        assert_eq!(d.active(), n);
    }

    #[test]
    fn guards_reach_the_document_they_name() {
        let state = AppState::default();
        let first = state.docs().active();
        let b = state.docs().add(titled("B"));
        assert_eq!(state.lock().id(), b);
        assert!(state.lock().is_active());
        {
            let mut g = state.lock_doc(Some(first)).unwrap();
            assert!(!g.is_active());
            g.edit("Test edit", |bud| edit::set_title(bud, "First"))
                .unwrap();
            assert_eq!(g.docs().len(), 2);
        }
        assert_eq!(state.docs().get(first).unwrap().budget().title, "First");
        assert_eq!(state.lock().budget().title, "B", "the active one untouched");
        assert_eq!(state.lock_doc(None).unwrap().id(), b);
        state.docs().remove(first).unwrap();
        assert!(state.lock_doc(Some(first)).is_err());
    }
}
