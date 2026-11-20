//! Recording this person's edits, and undo and redo of their own changes.

use super::*;

impl SharedDoc {
    // Recording

    /// Gives rows new to `after` (not in `before`) random ids from the
    /// shared range. Returns old id -> new id.
    pub fn randomize_new_rows(
        &mut self,
        before: &Budget,
        after: &mut Budget,
    ) -> HashMap<RowId, RowId> {
        let had: HashSet<RowId> = crate::edit::details(before).map(|d| d.id).collect();
        let mut taken: HashSet<RowId> = crate::edit::details(after).map(|d| d.id).collect();
        let mut map = HashMap::new();
        for d in after
            .categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
        {
            if had.contains(&d.id) || map.contains_key(&d.id) {
                continue;
            }
            let fresh =
                method_core::log::ids::random_row_id(&mut self.ids, |id| taken.contains(&id));
            taken.insert(fresh);
            map.insert(d.id, fresh);
            d.id = fresh;
        }
        map
    }

    /// Records the edit that took the projected budget to `after` as one
    /// change by this person. Returns the budget as the log now projects
    /// it (normally `after` itself) when anything was recorded.
    pub fn record(
        &mut self,
        after: &Budget,
        hints: &[Hint],
        label: &str,
    ) -> Result<Option<Budget>, String> {
        if let Some(t) = self.replaced_text() {
            return Err(t);
        }
        let id = self
            .shared
            .log
            .record(&self.proj, after, hints, label, &iso_now())
            .map_err(|e| self.refused_text(&e))?;
        let Some(id) = id else {
            return Ok(None);
        };
        self.reproject();
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        self.steps.push(Step {
            id,
            label: label.to_string(),
            at: now_ms(),
        });
        self.redo.clear();
        Ok(Some(self.proj.budget.clone()))
    }

    pub(super) fn reproject(&mut self) {
        self.proj = self.shared.log.project();
    }

    // Undo and redo, this person's only

    pub fn can_undo(&self) -> bool {
        !self.steps.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undoes this person's latest step. Returns the budget to adopt and
    /// a note when something couldn't be put back; `None` when there is
    /// nothing to undo.
    pub fn undo(&mut self) -> Option<(Budget, Option<String>)> {
        let step = self.steps.pop()?;
        let (budget, note) = self.reverse(&step, true);
        Some((budget, note))
    }

    pub fn redo(&mut self) -> Option<(Budget, Option<String>)> {
        let step = self.redo.pop()?;
        let (budget, note) = self.reverse(&step, false);
        Some((budget, note))
    }

    /// Records the inverse of `step`, moving it to the other stack.
    fn reverse(&mut self, step: &Step, undoing: bool) -> (Budget, Option<String>) {
        // The label is stored in English; the note is shown, so it's in
        // the person's language.
        let shown = crate::i18n::label(&step.label);
        let (new, inv) = match self.shared.log.undo(step.id, &iso_now()) {
            Ok(r) => r,
            Err(e @ LogError::NotAllowed { .. }) => {
                return (self.proj.budget.clone(), Some(self.refused_text(&e)))
            }
            Err(e) => {
                return (
                    self.proj.budget.clone(),
                    Some(if undoing {
                        tr_with("shared.couldnt_undo", &[("why", e.to_string().as_str())])
                    } else {
                        tr_with("shared.couldnt_redo", &[("why", e.to_string().as_str())])
                    }),
                )
            }
        };
        let mut note = None;
        match new {
            Some(id) => {
                let moved = Step {
                    id,
                    label: step.label.clone(),
                    at: step.at,
                };
                if undoing {
                    self.redo.push(moved);
                } else {
                    self.steps.push(moved);
                }
                self.reproject();
                let now = Instant::now();
                self.last_edit = Some(now);
                self.unwritten_since.get_or_insert(now);
                self.last_change = now;
                if !inv.skipped.is_empty() {
                    let params = [
                        ("label", shown.as_str()),
                        ("skips", &self.skips(&inv.skipped)),
                    ];
                    note = Some(if undoing {
                        tr_with("shared.undid_skipped", &params)
                    } else {
                        tr_with("shared.redid_skipped", &params)
                    });
                }
            }
            None => {
                let why = if inv.skipped.is_empty() {
                    tr("shared.changed_since")
                } else {
                    self.skips(&inv.skipped)
                };
                note = Some(tr_with(
                    "shared.nothing_to_put_back",
                    &[("label", shown.as_str()), ("why", why.as_str())],
                ));
            }
        }
        (self.proj.budget.clone(), note)
    }

    /// The skip sentences with people's names instead of actor ids.
    fn skips(&self, skipped: &[String]) -> String {
        let mut names: Vec<(String, String)> = Vec::new();
        for actor in self.shared.log.frontier().0.keys() {
            names.push((actor.short(), self.name_of(*actor)));
        }
        let mut out: Vec<String> = skipped
            .iter()
            .map(|s| {
                let mut s = s.clone();
                for (short, name) in &names {
                    s = s.replace(&format!(": {short} changed"), &format!(": {name} changed"));
                }
                s
            })
            .collect();
        out.dedup();
        out.join(" ")
    }

    /// This person's steps as the History list shows them.
    pub fn history(&self) -> crate::session::History {
        let steps = self
            .steps
            .iter()
            .map(|s| (s, true))
            .chain(self.redo.iter().rev().map(|s| (s, false)))
            .map(|(s, done)| crate::session::HistoryStep {
                label: s.label.clone(),
                at: s.at,
                done,
            })
            .collect();
        crate::session::History {
            steps,
            current: self.steps.len(),
            trimmed: false,
        }
    }

    pub fn history_len(&self) -> usize {
        self.steps.len() + self.redo.len()
    }

    pub fn applied_steps(&self) -> usize {
        self.steps.len()
    }
}
