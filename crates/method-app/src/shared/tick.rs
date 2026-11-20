//! The timer: flushing changes to the folder, keeping the file, the heartbeat.

use super::*;

impl SharedDoc {
    // The timer

    /// One pass for the app's timer: writes this computer's changes once
    /// typing pauses, reads what others wrote, keeps the file (the
    /// keeper), and the heartbeat. `budget` is the session's budget, which
    /// is replaced when changes arrive.
    pub fn tick(&mut self, now: Instant, budget: &mut Budget) -> TickOut {
        let mut out = TickOut::default();
        if self.replaced.is_some() {
            return out;
        }
        if !self.shared.dir.exists() {
            self.error = Some(tr("shared.cant_reach"));
            return out;
        }
        // The owner started the budget again: nothing more goes into the
        // folder from this log (docs/research/owner-key.md, part 1).
        if flog::replaced(&self.shared) {
            self.live_stop();
            self.replaced = Some(
                self.shared
                    .log
                    .own_after(self.written_seq)
                    .iter()
                    .map(|c| c.label.clone())
                    .collect(),
            );
            self.error = None;
            return out;
        }
        self.flush(now, false);
        // Who is here (their names go on the changes that arrive).
        if self
            .presence_read_at
            .is_none_or(|t| now.duration_since(t) >= PRESENCE_EVERY)
        {
            self.people = self.shared.dir.presences();
            self.read_local_tools();
            self.presence_read_at = Some(now);
        }
        // What others wrote.
        let before = self.shared.log.len();
        let folds = self.shared.log.folds();
        let mut report = flog::absorb(&mut self.shared.log, &self.shared.dir, &mut self.scan);
        report.applied += self.live_receive();
        report.pending = self.shared.log.pending().len();
        if self.shared.log.folds() != folds {
            // A removal (or the owner's first role or key) left out changes
            // this computer had applied: the budget is made again.
            self.reproject();
            *budget = self.proj.budget.clone();
            out.budget_changed = true;
            self.last_change = now;
        }
        self.last_sync = Some(now);
        self.waiting = report.pending;
        // The owner's key arrived: this computer's goes in too, and the
        // owner's is remembered.
        if report.applied > 0 && self.shared.log.signer_key().is_some() {
            if let Err(e) = self.publish_key() {
                self.error = Some(e);
            }
            self.remember_owner_key();
        }
        if report.applied > 0 {
            let me = self.actor();
            let arrived: Vec<ChangeId> = self
                .shared
                .log
                .changes_from(before)
                .filter(|c| c.actor != me)
                .map(|c| c.id())
                .collect();
            if !arrived.is_empty() {
                self.reproject();
                for id in arrived {
                    if let Some(rc) = self.remote_change(id) {
                        self.recent.push(rc.clone());
                        out.remote.push(rc);
                    }
                }
                if self.recent.len() > RECENT_KEPT {
                    let cut = self.recent.len() - RECENT_KEPT;
                    self.recent.drain(..cut);
                }
                *budget = self.proj.budget.clone();
                out.budget_changed = true;
                self.last_change = now;
            }
        }
        // Invites used or past their date are sealed by whoever may give
        // their role (decisions.md, entry 97), so a link that reaches someone
        // later can't take the place of the person who joined.
        if self.seal_invites() {
            self.reproject();
            *budget = self.proj.budget.clone();
        }
        if self.keeper && self.keep_file(now, budget) {
            out.budget_changed = true;
        }
        // Given what was asked for: the request goes.
        if self
            .request
            .as_ref()
            .is_some_and(|r| self.has_asked(self.actor(), r))
        {
            self.request = None;
            self.place_changed = true;
        }
        self.heartbeat(now, false);
        self.live_send();
        out
    }

    /// Writes this computer's unwritten changes to a segment when typing
    /// has paused (or `force`).
    pub fn flush(&mut self, now: Instant, force: bool) -> bool {
        if self.unshared() == 0 {
            self.unwritten_since = None;
            return false;
        }
        let quiet = self
            .last_edit
            .is_none_or(|t| now.duration_since(t) >= FLUSH_QUIET);
        let long = self
            .unwritten_since
            .is_some_and(|t| now.duration_since(t) >= FLUSH_LATEST);
        if !(force || quiet || long) {
            return false;
        }
        match flog::flush(&self.shared.log, &self.shared.dir) {
            Ok(_) => {
                self.written_seq = self.shared.log.own_seq();
                self.unwritten_since = None;
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(tr_with(
                    "shared.cant_write_folder",
                    &[("why", e.0.as_str())],
                ));
                false
            }
        }
    }

    /// The keeper's work: take in an edit made to the `.mthd` by another
    /// program, and rewrite the file from the merged state once the budget
    /// is quiet. True when the budget changed.
    fn keep_file(&mut self, now: Instant, budget: &mut Budget) -> bool {
        let path = self.shared.path.clone();
        let mut changed = false;
        if self.disk.is_some() && !self.file_matches(&path) {
            if let Ok(bytes) = std::fs::read(&path) {
                if bytes == flog::snapshot_text(&self.shared, &self.proj).as_bytes() {
                    // Another program of this person's wrote what the log
                    // holds (the owner's command line): nothing to do.
                    self.snapshot_len = self.shared.log.len();
                    self.disk = Some(DiskStamp::of(&path, &bytes));
                } else if self.shared.dir.is_written(&bytes) {
                    // An older version of the budget put back (a sync
                    // service's history, a backup): behind the log, so it
                    // is written again rather than taken in.
                    self.write_file(&path);
                    self.outside = Some(format!(
                        "{} was put back to an older version of the budget, so it was written again from everyone\u{2019}s changes. Nothing was lost.",
                        self.shared.info.file
                    ));
                    return changed;
                }
            }
        }
        if self.disk.is_some() && !self.file_matches(&path) && self.shared.log.roles_started() {
            // Roles are set: nobody can say whose edit the file holds, so it
            // isn't taken in as the keeper's (decisions.md, entry 78). It is
            // kept beside the file and the file is written again.
            match flog::set_aside(&self.shared, &iso_now()) {
                Ok(Some(kept)) => {
                    self.outside = Some(format!(
                        "Someone changed {} outside Method Budget. Because roles are set, that change wasn\u{2019}t taken in: it was kept as \u{201c}{}\u{201d} beside it, and the file was written again from everyone\u{2019}s changes.",
                        self.shared.info.file,
                        kept.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                    ));
                    self.snapshot_len = self.shared.log.len();
                    self.note_snapshot(&path);
                }
                Ok(None) => {}
                Err(e) => self.error = Some(e.0),
            }
            self.disk = std::fs::read(&path).ok().map(|b| DiskStamp::of(&path, &b));
        } else if self.disk.is_some() && !self.file_matches(&path) {
            match flog::ingest(&mut self.shared, &iso_now()) {
                Ok(Some(_)) => {
                    self.reproject();
                    *budget = self.proj.budget.clone();
                    changed = true;
                    self.last_change = now;
                    self.unwritten_since.get_or_insert(now);
                }
                Ok(None) => {}
                Err(e) => self.error = Some(e.0),
            }
            // Whatever it held, the file is now known.
            self.disk = std::fs::read(&path).ok().map(|b| DiskStamp::of(&path, &b));
        }
        if self.snapshot_len != self.shared.log.len()
            && now.duration_since(self.last_change) >= SNAPSHOT_QUIET
        {
            self.write_file(&path);
        }
        changed
    }

    /// Writes the merged budget as the `.mthd` now (the keeper).
    pub fn write_file(&mut self, path: &Path) -> bool {
        match flog::write_snapshot(&self.shared) {
            Ok(_) => {
                self.snapshot_len = self.shared.log.len();
                self.note_snapshot(path);
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(tr_with("shared.cant_write_file", &[("why", e.0.as_str())]));
                false
            }
        }
    }

    /// Remembers the file as just written (its stamp, and a note in this
    /// actor's folder for the next start).
    pub(super) fn note_snapshot(&mut self, path: &Path) {
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let stamp = DiskStamp::of(path, &bytes);
        let written = iso_now();
        let note = SnapshotNote {
            hash: stamp.hash.to_string(),
            written: written.clone(),
        };
        if let Ok(text) = serde_json::to_string(&note) {
            let _ = flog::write_sealed(&self.snapshot_note_path(), text.as_bytes());
        }
        self.disk = Some(stamp);
        self.file_written = Some(written);
    }

    fn snapshot_note_path(&self) -> PathBuf {
        self.shared
            .dir
            .root()
            .join("actors")
            .join(self.actor().hex())
            .join("snapshot.json")
    }

    /// The stamp of the file as this keeper last wrote it, from its note,
    /// when the file still has that content.
    pub(super) fn read_snapshot_note(&mut self, path: &Path) -> Option<DiskStamp> {
        let text = std::fs::read_to_string(self.snapshot_note_path()).ok()?;
        let note: SnapshotNote = serde_json::from_str(&text).ok()?;
        let hash: u64 = note.hash.parse().ok()?;
        let bytes = std::fs::read(path).ok()?;
        let stamp = DiskStamp::of(path, &bytes);
        self.file_written = Some(note.written);
        (stamp.hash == hash).then_some(stamp)
    }

    /// Whether the `.mthd` still holds what the keeper last wrote.
    pub(super) fn file_matches(&mut self, path: &Path) -> bool {
        let Some(stamp) = &self.disk else {
            return false;
        };
        match stamp.check(path) {
            crate::safety::DiskCheck::Same(fresh) => {
                if let Some(f) = fresh {
                    self.disk = Some(f);
                }
                true
            }
            crate::safety::DiskCheck::Changed => false,
            // Gone: nothing to take in; the next snapshot puts it back.
            crate::safety::DiskCheck::Missing => true,
        }
    }

    /// Writes the heartbeat when it is due, or where this person is changed.
    pub(super) fn heartbeat(&mut self, now: Instant, force: bool) {
        let due = self
            .heartbeat_at
            .is_none_or(|t| now.duration_since(t) >= HEARTBEAT_EVERY);
        if !(force || due || self.place_changed) {
            return;
        }
        let p = Presence {
            name: self.me.name.clone(),
            color: self.me.color.clone(),
            category: self.place.0.clone(),
            account: self.place.1.clone(),
            at: iso_now(),
            frontier: self.shared.log.frontier().digest(),
            request: self.request.clone(),
            live: self.live.as_ref().and_then(|l| l.mesh.address().ok()),
        };
        if self.shared.dir.write_presence(self.actor(), &p).is_ok() {
            self.heartbeat_at = Some(now);
            self.place_changed = false;
        }
    }

    /// Everything for the way out: changes to the folder and (the keeper)
    /// the file.
    pub fn flush_all(&mut self) {
        let now = Instant::now();
        self.flush(now, true);
        if self.keeper && self.snapshot_len != self.shared.log.len() {
            let path = self.shared.path.clone();
            self.write_file(&path);
        }
    }
}
