//! One open budget, as method-session keeps it ([`Session`], re-exported
//! here), with what only the desktop does to it: reading and writing its
//! file, noticing changes made to the file outside the app, and sharing it
//! through a folder ([`SessionFiles`]); finding a tab by its file
//! ([`DocsFiles`]). Bring the traits into scope to call these as methods,
//! as before the split (decision 70).

use std::path::Path;
use std::time::Instant;

use method_core::Budget;
pub use method_session::session::*;

use crate::docs::{DocId, Docs};
use crate::safety::{self, Access, DiskStamp, DiskStampExt};
use crate::shared::SharedDoc;

/// The desktop's file and folder side of a [`Session`].
pub trait SessionFiles {
    /// Reads a budget file and starts a fresh session on it.
    fn open(path: &Path) -> Result<Session, String>;
    /// Writes the budget to `path` (through a temporary file in the same
    /// folder, so a failed write never leaves half a file), then remembers
    /// the path and marks the budget saved.
    fn save_to(&mut self, path: &Path) -> Result<(), String>;
    /// Whether the budget's file changed on disk since the app last read or
    /// wrote it. Missing (moved or deleted) is not a change: a save puts the
    /// file back. Unknown (no file, or not read by this session) is not
    /// either.
    fn changed_on_disk(&mut self) -> bool;
    /// Replaces the budget with the file's current content ("Load
    /// theirs"), as one edit so Undo brings this version back
    /// ([`Session::adopt_file_version`]).
    fn load_from_disk(&mut self) -> Result<(), String>;
    /// Whether the budget lives in the file at `path` (the same file by
    /// another spelling of its path counts too).
    fn is_file(&self, path: &Path) -> bool;
    /// The shared folder's log, when the budget is shared.
    fn shared(&self) -> Option<&SharedDoc>;
    fn shared_mut(&mut self) -> Option<&mut SharedDoc>;
    /// Starts sharing: the budget must have a file, with no change log
    /// beside it yet. Unsaved changes go into the file first. This
    /// computer keeps the file.
    fn start_sharing(
        &mut self,
        actor: method_core::log::ActorId,
        me: crate::shared::Person,
    ) -> Result<(), String>;
    /// Joins the shared budget whose change log is beside its file: the
    /// merged budget replaces what the file held.
    fn join_shared(
        &mut self,
        actor: method_core::log::ActorId,
        me: crate::shared::Person,
    ) -> Result<method_format::log::LoadReport, String>;
    /// One pass of the sharing timer (src/shared_app.rs): what arrived
    /// from other people, for the page.
    fn share_tick(&mut self, now: Instant) -> Option<crate::shared::TickOut>;
    /// Everything of this person's into the folder now, and (the keeper)
    /// the file: before closing, quitting or losing focus.
    fn share_flush(&mut self);
    /// Writes value `index` of a field's history back (the review panel
    /// and Who Changed This).
    fn share_use_value(
        &mut self,
        kind: &str,
        key: &str,
        field: &str,
        index: usize,
    ) -> Result<(), String>;
}

impl SessionFiles for Session {
    fn open(path: &Path) -> Result<Session, String> {
        let path = &safety::through_link(path);
        let (budget, stamp) = read_budget(path)?;
        let mut s = Session::with_budget(budget, Some(path.to_path_buf()));
        s.set_disk_stamp(Some(stamp));
        Ok(s)
    }

    fn save_to(&mut self, path: &Path) -> Result<(), String> {
        // Save As onto a link writes the file it points to (Save already
        // has the resolved path from open).
        let path = &safety::through_link(path);
        if let Some(sh) = self.shared_mut() {
            // A shared budget lives in its shared folder: Save puts this
            // person's changes there (and the keeper's file is rewritten);
            // nobody else writes the file (docs/research/collaboration.md, 4.3).
            if !safety::same_file(path, sh.path()) {
                return Err(shared_stays());
            }
            sh.flush_all();
            return match sh.status(Instant::now()).error {
                Some(e) => Err(e),
                None => Ok(()),
            };
        }
        let text = method_format::to_string(self.budget());
        safety::write_atomically(path, text.as_bytes(), Access::Document)
            .map_err(|e| format!("Couldn't save {}: {e}", path.display()))?;
        self.mark_saved(path, DiskStamp::of(path, text.as_bytes()));
        Ok(())
    }

    fn changed_on_disk(&mut self) -> bool {
        if self.is_shared() {
            // The keeper takes outside edits in as changes (src/shared.rs);
            // nobody else writes the file.
            return false;
        }
        let (Some(path), Some(stamp)) = (self.path(), self.disk_stamp()) else {
            return false;
        };
        match stamp.check(path) {
            safety::DiskCheck::Changed => true,
            safety::DiskCheck::Same(fresh) => {
                if let Some(fresh) = fresh {
                    self.set_disk_stamp(Some(fresh));
                }
                false
            }
            safety::DiskCheck::Missing => false,
        }
    }

    fn load_from_disk(&mut self) -> Result<(), String> {
        let path = self.path().ok_or("This budget has no file.")?.to_path_buf();
        let (theirs, stamp) = read_budget(&path)?;
        self.adopt_file_version(theirs, stamp)
    }

    fn is_file(&self, path: &Path) -> bool {
        self.path().is_some_and(|p| safety::same_file(p, path))
    }

    fn shared(&self) -> Option<&SharedDoc> {
        self.shared_log()?.as_any().downcast_ref()
    }

    fn shared_mut(&mut self) -> Option<&mut SharedDoc> {
        self.shared_log_mut()?.as_any_mut().downcast_mut()
    }

    fn start_sharing(
        &mut self,
        actor: method_core::log::ActorId,
        me: crate::shared::Person,
    ) -> Result<(), String> {
        if self.is_shared() {
            return Err("This budget is already shared.".into());
        }
        if self.is_read_only() {
            return Err(read_only());
        }
        let path = self
            .path()
            .ok_or("Save the budget first, then share it.")?
            .to_path_buf();
        if self.is_dirty() {
            self.save_to(&path)?;
        }
        let doc = SharedDoc::start(&path, actor, me)?;
        self.attach_shared(Box::new(doc), None);
        Ok(())
    }

    fn join_shared(
        &mut self,
        actor: method_core::log::ActorId,
        me: crate::shared::Person,
    ) -> Result<method_format::log::LoadReport, String> {
        if self.is_shared() {
            return Err("This budget is already shared.".into());
        }
        let path = self.path().ok_or("This budget has no file.")?.to_path_buf();
        let (doc, report) = SharedDoc::open(&path, actor, me)?;
        let merged = doc.budget().clone();
        self.attach_shared(Box::new(doc), Some(merged));
        Ok(report)
    }

    fn share_tick(&mut self, now: Instant) -> Option<crate::shared::TickOut> {
        self.with_shared(
            |log, budget| match log.as_any_mut().downcast_mut::<SharedDoc>() {
                Some(sh) => {
                    let out = sh.tick(now, budget);
                    let changed = out.budget_changed;
                    (Some(out), changed)
                }
                None => (None, false),
            },
        )
        .flatten()
    }

    fn share_flush(&mut self) {
        if let Some(sh) = self.shared_mut() {
            sh.flush_all();
        }
    }

    fn share_use_value(
        &mut self,
        kind: &str,
        key: &str,
        field: &str,
        index: usize,
    ) -> Result<(), String> {
        self.check_writable()?;
        let sh = self.shared_mut().ok_or("This budget isn't shared.")?;
        let budget = sh.use_value(kind, key, field, index)?;
        self.adopt_projected(
            budget,
            &method_session::messages::word("engine.undo.put_back_value"),
        );
        Ok(())
    }
}

/// The desktop's file side of [`Docs`].
pub trait DocsFiles {
    /// The tab whose budget lives in the file at `path`, if one does (the
    /// same file by another spelling of its path counts too).
    fn find_file(&self, path: &Path) -> Option<DocId>;
}

impl DocsFiles for Docs {
    fn find_file(&self, path: &Path) -> Option<DocId> {
        self.find_file_by(path, safety::same_file)
    }
}

/// Reads and parses a budget file, with the stamp of what was read.
pub(crate) fn read_budget(path: &Path) -> Result<(Budget, DiskStamp), String> {
    // Capped: a file far larger than any budget is refused before it is
    // read into memory.
    let bytes = method_format::fsio::read_capped(path, method_format::fsio::MAX_READ_BYTES)
        .map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        format!(
            "Couldn't read {}: stream did not contain valid UTF-8",
            path.display()
        )
    })?;
    let budget = method_format::from_str(text)
        .map_err(|e| format!("{} isn't a readable budget: {e}", path.display()))?;
    Ok((budget, DiskStamp::of(path, &bytes)))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::docs::DocId;
    use method_core::sample::{self, SampleSpec};

    fn small_sample(seed: u64) -> Budget {
        sample::budget(&SampleSpec {
            seed,
            rows: 120,
            accounts: 9,
            categories: 3,
            fringes: 6,
            groups: 4,
            baseline: true,
        })
    }

    #[test]
    fn a_recovered_budget_is_unsaved_and_asks_before_replacing_its_file() {
        let dir = safety::tests::temp_dir("session-recovered");
        let path = dir.join("film.mthd");
        let mut s = Session::recovered(small_sample(7), Some(path.clone()), "abc-1".into());
        assert!(s.is_dirty());
        assert!(!s.can_undo());
        assert_eq!(s.recovery_key(), "abc-1");
        assert!(s.recovery_current(), "its copy already holds it");
        assert!(!s.autosave_due(method_session::clock::mono_ms() + 600_000));
        assert!(s.needs_overwrite_confirm() && !s.can_autosave_to_file());
        assert!(!s.has_backed_up(&path));
        s.save_to(&path).unwrap();
        assert!(!s.is_dirty() && s.recovery_stale());
        assert!(!s.needs_overwrite_confirm() && s.can_autosave_to_file());
        // Never saved: nothing to confirm, still unsaved.
        let u = Session::recovered(small_sample(7), None, "abc-2".into());
        assert!(u.is_dirty() && !u.needs_overwrite_confirm());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn saving_a_linked_budget_writes_the_file_the_link_points_to() {
        let dir = safety::tests::temp_dir("session-link");
        let (target, link) = (dir.join("real.mthd"), dir.join("link.mthd"));
        Session::new().save_to(&target).unwrap();
        std::os::unix::fs::symlink("real.mthd", &link).unwrap();
        let is_link = |p: &Path| {
            std::fs::symlink_metadata(p)
                .unwrap()
                .file_type()
                .is_symlink()
        };

        // Opened through the link: Save updates the target, the link stays.
        let mut s = Session::open(&link).unwrap();
        let saved_to = s.path().unwrap().to_path_buf();
        assert!(safety::same_file(&saved_to, &target) && !is_link(&saved_to));
        s.save_to(&saved_to).unwrap();
        assert!(is_link(&link));

        // Save As onto the link: the same.
        let mut other = Session::with_budget(small_sample(3), None);
        other.save_to(&link).unwrap();
        assert!(is_link(&link));
        let text = method_format::to_string(other.budget());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), text);
        assert!(safety::same_file(other.path().unwrap(), &target));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // Tabs found and reopened by their files (src/docs.rs in method-session).

    #[test]
    fn open_files_are_found_and_listed_in_tab_order() {
        let dir = safety::tests::temp_dir("docs-files");
        let (p1, p2) = (dir.join("one.mthd"), dir.join("two.mthd"));
        let mut d = Docs::default();
        let untitled = d.active();
        let mut one = Session::new();
        one.save_to(&p1).unwrap();
        let mut two = Session::new();
        two.save_to(&p2).unwrap();
        let a = d.add(one);
        let b = d.add(two);
        assert_eq!(d.find_file(&p1), Some(a));
        assert_eq!(d.find_file(&dir.join(".").join("two.mthd")), Some(b));
        assert_eq!(d.find_file(&dir.join("three.mthd")), None);
        assert_eq!(d.files(), (vec![p1.clone(), p2.clone()], Some(1)));
        d.switch(untitled).unwrap();
        assert_eq!(d.files(), (vec![p1, p2], None), "untitled isn't listed");
        let t = d.tabs();
        assert_eq!(t[1].label, "one");
        assert_eq!(t[1].name, "one.mthd");
        assert!(t[1].path.as_deref().unwrap().ends_with("one.mthd"));
        std::fs::remove_dir_all(&dir).unwrap();
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
    fn split_view_is_kept_by_file() {
        let dir = safety::tests::temp_dir("docs-split");
        let files: Vec<PathBuf> = ["one", "two", "three"]
            .iter()
            .map(|n| dir.join(format!("{n}.mthd")))
            .collect();
        let mut d = Docs::default();
        let untitled = d.active();
        let mut ids = Vec::new();
        for f in &files {
            let mut s = Session::new();
            s.save_to(f).unwrap();
            ids.push(d.add(s));
        }
        assert_eq!(d.saved_split(), None, "one pane");
        d.switch(ids[1]).unwrap();
        d.split_right().unwrap(); // left [u 0 1 2], right [1]
        d.move_to_pane(ids[2], 0, None).unwrap(); // left [u 0 1], right [1 2]
        d.switch_in(0, untitled).unwrap();
        d.switch_in(0, ids[0]).unwrap();
        let saved = d.saved_split().unwrap();
        // By place among the files (the untitled budget isn't kept).
        assert_eq!(saved.panes, [vec![0, 1], vec![1, 2]]);
        assert_eq!(saved.fronts, [0, 2]);
        assert_eq!(saved.target, 0);

        // The next start: the files open again, then the panes.
        let mut next = Docs::default();
        let reopened: Vec<Option<DocId>> = files
            .iter()
            .map(|f| Some(next.add(Session::open(f).unwrap())))
            .collect();
        assert!(next.restore_split(&saved, &reopened));
        check(&next);
        let l = next.layout();
        let first = next.ids()[0];
        let r = |i: usize| reopened[i].unwrap();
        assert_eq!(
            l.panes[0].tabs,
            [r(0), r(1), first],
            "the untitled one joins the left"
        );
        assert_eq!(l.panes[1].tabs, [r(1), r(2)]);
        assert_eq!(
            (l.panes[0].front, l.panes[1].front, l.target),
            (r(0), r(2), 0)
        );
        assert_eq!(next.active(), r(0));

        // A file that couldn't be read leaves its pane short; a pane left
        // with nothing means one pane.
        let mut d2 = Docs::default();
        let only = d2.add(Session::open(&files[0]).unwrap());
        assert!(!d2.restore_split(&saved, &[Some(only), None, None]));
        assert!(d2.split().is_none());
        // A pane of untitled budgets only isn't kept.
        let mut d3 = Docs::default();
        d3.add(Session::open(&files[0]).unwrap());
        d3.switch(d3.ids()[0]).unwrap();
        d3.split_right().unwrap(); // right: the untitled one
        assert_eq!(d3.saved_split(), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
