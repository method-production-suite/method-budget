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
//! No Tauri types here: everything is tested without a window. The commands
//! are in `src/tabs.rs`.

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
}

/// One tab as the page draws it (and `list_tabs` gives it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TabInfo {
    pub id: DocId,
    /// What the tab shows: the file's name without `.fwb`, or the budget's
    /// title while it has no file ([`Session::tab_label`]).
    pub label: String,
    /// The budget's title.
    pub title: String,
    /// The file's name as the title bar shows it (`Untitled.fwb` without one).
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

    /// Opens `s` as a new tab at the end and brings it to the front.
    pub fn add(&mut self, s: Session) -> DocId {
        let id = self.new_id();
        self.tabs.push(Doc { id, session: s });
        self.active = id;
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
        Ok((new, old.session))
    }

    /// Brings tab `id` to the front.
    pub fn switch(&mut self, id: DocId) -> Result<(), String> {
        self.index_of(id).ok_or(CLOSED)?;
        self.active = id;
        Ok(())
    }

    /// The tab `delta` places after the one in front (before it, for a
    /// negative `delta`), wrapping round.
    pub fn step(&self, delta: isize) -> DocId {
        let n = self.tabs.len() as isize;
        let at = (self.active_index() as isize + delta).rem_euclid(n.max(1));
        self.tabs[at as usize].id
    }

    /// Moves tab `id` so that it ends up at `to` in tab order (the end when
    /// past it). Which tab is in front doesn't change.
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
        if self.active == id {
            self.active = self.tabs[at.min(self.tabs.len() - 1)].id;
        }
        Ok(doc.session)
    }

    /// The tab whose budget lives in the file at `path`, if one does.
    pub fn find_file(&self, path: &Path) -> Option<DocId> {
        self.tabs
            .iter()
            .find(|d| d.session.is_file(path))
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

    /// Brings this document's tab to the front.
    pub fn bring_to_front(&mut self) {
        self.docs.active = self.id();
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
        s.edit(|b| edit::set_title(b, t)).unwrap();
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
            .edit(|b| edit::set_title(b, "X"))
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

    #[test]
    fn open_files_are_found_and_listed_in_tab_order() {
        let dir = crate::safety::tests::temp_dir("docs-files");
        let (p1, p2) = (dir.join("one.fwb"), dir.join("two.fwb"));
        let mut d = Docs::default();
        let untitled = d.active();
        let mut one = Session::new();
        one.save_to(&p1).unwrap();
        let mut two = Session::new();
        two.save_to(&p2).unwrap();
        let a = d.add(one);
        let b = d.add(two);
        assert_eq!(d.find_file(&p1), Some(a));
        assert_eq!(d.find_file(&dir.join(".").join("two.fwb")), Some(b));
        assert_eq!(d.find_file(&dir.join("three.fwb")), None);
        assert_eq!(d.files(), (vec![p1.clone(), p2.clone()], Some(1)));
        d.switch(untitled).unwrap();
        assert_eq!(d.files(), (vec![p1, p2], None), "untitled isn't listed");
        let t = d.tabs();
        assert_eq!(t[1].label, "one");
        assert_eq!(t[1].name, "one.fwb");
        assert!(t[1].path.as_deref().unwrap().ends_with("one.fwb"));
        std::fs::remove_dir_all(&dir).unwrap();
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
            g.edit(|bud| edit::set_title(bud, "First")).unwrap();
            assert_eq!(g.docs().len(), 2);
        }
        assert_eq!(state.docs().get(first).unwrap().budget().title, "First");
        assert_eq!(state.lock().budget().title, "B", "the active one untouched");
        assert_eq!(state.lock_doc(None).unwrap().id(), b);
        state.docs().remove(first).unwrap();
        assert!(state.lock_doc(Some(first)).is_err());
    }
}
