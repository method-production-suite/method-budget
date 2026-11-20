//! Names and places: what the page sees of people, status, review, blame and activity.

use super::*;

impl SharedDoc {
    // Names and places

    /// The name a person shows with: their own, or "Person 7f3a".
    /// Uses `root` for this computer's tool actor pools (tests).
    pub fn set_tools_root(&mut self, root: PathBuf) {
        self.tools_root = Some(root);
        self.read_local_tools();
    }

    pub(super) fn read_local_tools(&mut self) {
        self.local_tools = self
            .tools_root
            .as_deref()
            .map(|r| flog::tool_actors(r, self.actor()).into_iter().collect())
            .unwrap_or_default();
    }

    /// The person a program's actor writes for: from the log once it says
    /// so, and for this person's own programs from this computer's pool
    /// (which only this computer can see).
    pub(super) fn person_for(&self, actor: ActorId) -> Option<ActorId> {
        let person = self.shared.log.person_of(actor);
        if person != actor {
            return Some(person);
        }
        self.local_tools.contains(&actor).then(|| self.actor())
    }

    /// An actor that is surely a program whose person isn't known here
    /// yet: it made changes but never wrote a heartbeat (every app does),
    /// has no role and didn't start sharing. Other people's programs say
    /// whose they are only once roles are set (`acts_for`), so until then
    /// this computer can't honestly name them.
    fn unknown_program(&self, actor: ActorId) -> bool {
        actor != self.actor()
            && self.shared.log.owner() != Some(actor)
            && self.person_for(actor).is_none()
            && !self.people.contains_key(&actor)
            && !self.shared.log.state().grants().contains_key(&actor)
    }

    pub fn name_of(&self, actor: ActorId) -> String {
        // A command line or agent writing for someone shows as them.
        if let Some(person) = self.person_for(actor) {
            return format!("{} (command line or agent)", self.name_of(person));
        }
        if self.unknown_program(actor) {
            return "A command line or agent (whose isn\u{2019}t known yet)".into();
        }
        if actor == self.actor() {
            if !self.me.name.is_empty() {
                return self.me.name.clone();
            }
            return default_name(actor);
        }
        match self.people.get(&actor) {
            Some(p) if !p.name.trim().is_empty() => {
                p.name.trim().chars().take(NAME_MAX_CHARS).collect()
            }
            _ => default_name(actor),
        }
    }

    pub fn color_of(&self, actor: ActorId) -> String {
        if let Some(person) = self.person_for(actor) {
            return self.color_of(person);
        }
        if self.unknown_program(actor) {
            return "slate".into();
        }
        if actor == self.actor() {
            return self.me.color.clone();
        }
        match self.people.get(&actor) {
            Some(p) if COLORS.contains(&p.color.as_str()) => p.color.clone(),
            _ => default_color(actor).to_string(),
        }
    }

    fn person_dto(&self, actor: ActorId, now: Instant) -> PersonDto {
        let role = self.shared.log.role_of(actor);
        let removed = self.shared.log.removed(actor);
        let name = self.name_of(actor);
        let me = actor == self.actor();
        let presence = self.people.get(&actor);
        let seen_secs = if me {
            Some(0)
        } else {
            presence.and_then(|p| iso_secs(&p.at)).map(|at| {
                let now_s = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                now_s.saturating_sub(at)
            })
        };
        let _ = now;
        PersonDto {
            actor: actor.hex(),
            initials: initials(&name),
            name,
            color: self.color_of(actor),
            me,
            keeper: actor == self.shared.info.genesis.actor,
            seen_secs,
            away: !me && seen_secs.is_none_or(|s| s >= AWAY_AFTER.as_secs()),
            category: if me {
                self.place.0.clone()
            } else {
                presence.and_then(|p| p.category.clone())
            },
            account: if me {
                self.place.1.clone()
            } else {
                presence.and_then(|p| p.account.clone())
            },
            changes: self.shared.log.frontier().get(&actor),
            cell: if me { None } else { self.live_cell(actor) },
            selection: if me {
                Vec::new()
            } else {
                self.live_selection(actor)
            },
            linked: !me && self.live_linked(actor),
            role: role.key().to_string(),
            role_text: if removed {
                crate::i18n::phrase(&word("engine.role.removed"))
            } else {
                self.role_text(&role)
            },
            scope: role
                .scope()
                .map(|s| self.scope_codes(s))
                .unwrap_or_default(),
            signs: self.shared.log.key_of(actor).is_some(),
            removed,
        }
    }

    /// The status item and panel.
    pub fn status(&self, now: Instant) -> ShareStatus {
        let me = self.actor();
        let mut actors: BTreeSet<ActorId> = self.shared.log.frontier().0.keys().copied().collect();
        actors.extend(self.people.keys().copied());
        // People given a role before they ever opened the budget.
        actors.extend(self.shared.log.members().into_iter().map(|(a, _)| a));
        actors.insert(me);
        let people: Vec<PersonDto> = actors
            .iter()
            .filter(|a| **a != me && self.person_for(**a).is_none() && !self.unknown_program(**a))
            .map(|a| self.person_dto(*a, now))
            .collect();
        let review = self.review();
        let (locked_categories, locked_accounts) = self.locked();
        ShareStatus {
            live_on: self.live_on(),
            live_connected: self.live_connected(),
            mode: "folder".into(),
            folder: self
                .shared
                .dir
                .root()
                .parent()
                .unwrap_or(self.shared.dir.root())
                .display()
                .to_string(),
            file: self.shared.info.file.clone(),
            me: self.person_dto(me, now),
            keeper: self.keeper,
            people,
            synced_secs: self.last_sync.map(|t| now.duration_since(t).as_secs()),
            unshared: self.unshared(),
            waiting: self.waiting,
            review_count: review.len(),
            error: self.error.clone(),
            file_written: self.file_written.clone(),
            changes: self.shared.log.len(),
            recent: self.recent.clone(),
            conflicts: review
                .into_iter()
                .filter(|r| r.place.as_ref().is_some_and(|p| p.field.is_some()))
                .collect(),
            can_set_roles: self.role().sets_roles(),
            roles_started: self.shared.log.roles_started(),
            refused: self.shared.log.rejected().len() + self.shared.log.forks().len(),
            refused_changes: self.refused_changes(),
            notice: self.outside.clone(),
            signed: self.shared.log.signing_started(),
            signed_text: self.signed_text(),
            key_warning: self.key_warning(),
            can_start_again: self.signer.is_some()
                && self.shared.log.owner() == Some(me)
                && self.shared.log.key_conflict().is_some(),
            owner_key_warning: self.owner_key_warning(),
            replaced: self.replaced_text(),
            not_in_new: self
                .replaced
                .iter()
                .flatten()
                .map(|l| crate::i18n::label(l))
                .collect(),
            requests: self.requests(),
            my_request: self.request.as_ref().map(|r| self.request_dto(me, r)),
            can_ask: self.can_ask(),
            ask_name: self.owner_name(),
            locked_categories,
            locked_accounts,
            can_invite: self.can_invite(),
            invites: self.invites(),
        }
    }

    /// When a key in this person's name is in the budget and it isn't this
    /// computer's, what to tell them (decisions.md, entry 89).
    pub fn key_warning(&self) -> Option<String> {
        self.shared.log.key_conflict()?;
        Some(if self.shared.log.owner() == Some(self.actor()) {
            tr("shared.key_taken_owner")
        } else {
            tr_with("shared.key_taken", &[("owner", self.owner_name().as_str())])
        })
    }

    /// What signing means in this budget now, in a sentence.
    fn signed_text(&self) -> String {
        if self.shared.log.signing_started() {
            tr("shared.signed.on")
        } else if self.shared.log.roles_started() {
            tr("shared.signed.not_yet")
        } else {
            tr("shared.signed.after_first_role")
        }
    }

    /// The changes left out, newest last, with why in plain words.
    fn refused_changes(&self) -> Vec<RefusedDto> {
        // Changes refused, then other changes claiming the same number as
        // another (MReview, pull request #73): both listed.
        let all: Vec<&(method_core::log::Change, String)> = self
            .shared
            .log
            .rejected()
            .iter()
            .chain(self.shared.log.forks())
            .collect();
        all[all.len().saturating_sub(REFUSED_SHOWN)..]
            .iter()
            .copied()
            .map(|(c, reason)| RefusedDto {
                actor: c.actor.hex(),
                name: self.name_of(c.actor),
                color: self.color_of(c.actor),
                time: c.time.clone(),
                label: crate::i18n::label(&c.label),
                why: refused_why(reason),
            })
            .collect()
    }

    /// Where an entity is in the projected budget, for the page.
    pub fn place_of(&self, uid: Uid, field: Option<Field>) -> Option<PlaceDto> {
        let state = self.shared.log.state();
        let e = state.entity(uid)?;
        let b = &self.proj.binding;
        let text = method_core::log::project::describe(state, b, uid);
        let parent = e
            .place()
            .map(|p| p.list.parent)
            .filter(|p| *p != Uid::ROOT)
            .and_then(|p| b.keys.get(&p).cloned());
        let key = match e.kind {
            Kind::Row => b.row_ids.get(&uid).map(|id| id.to_string())?,
            _ => b.keys.get(&uid).cloned()?,
        };
        Some(PlaceDto {
            kind: e.kind.name().to_string(),
            key,
            parent,
            field: field.map(|f| f.name().to_string()),
            text,
        })
    }

    pub(super) fn remote_change(&self, id: ChangeId) -> Option<RemoteChange> {
        let c = self.shared.log.change(id)?;
        let mut seen = BTreeSet::new();
        let mut places = Vec::new();
        for op in &c.ops {
            let field = match op {
                method_core::log::Op::Set { field, .. } => Some(*field),
                _ => None,
            };
            let key = (op.entity(), field);
            if !seen.insert(key) {
                continue;
            }
            if let Some(p) = self.place_of(op.entity(), field) {
                places.push(p);
            }
            if places.len() >= 200 {
                break;
            }
        }
        Some(RemoteChange {
            actor: c.actor.hex(),
            name: self.name_of(c.actor),
            color: self.color_of(c.actor),
            label: crate::i18n::label(&c.label),
            time: c.time.clone(),
            places,
        })
    }

    /// A register value as a cell shows it (a formula with its names).
    fn value_text(&self, v: &Value) -> String {
        match v {
            Value::Null => String::new(),
            Value::Text(s) => s.clone(),
            Value::Formula(f) => {
                let keys = &self.proj.binding.keys;
                method_core::log::formula::unbind(f, &|u| keys.get(&u).cloned())
            }
            other => other.brief(),
        }
    }

    fn value_dto(
        &self,
        stamp: Stamp,
        time: &str,
        value: &Value,
        kept: bool,
        index: usize,
    ) -> ValueDto {
        let label = self
            .shared
            .log
            .change(stamp.change())
            .map(|c| crate::i18n::label(&c.label))
            .unwrap_or_default();
        ValueDto {
            value: self.value_text(value),
            actor: stamp.actor.hex(),
            name: self.name_of(stamp.actor),
            color: self.color_of(stamp.actor),
            me: stamp.actor == self.actor(),
            time: time.to_string(),
            label,
            kept,
            index,
        }
    }

    /// The review list (design, 9.5).
    pub fn review(&self) -> Vec<ReviewDto> {
        self.proj
            .review
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let (place, values) = match &item.review {
                    Review::Overwritten {
                        entity,
                        field,
                        kept,
                        lost,
                    } => {
                        let history = self.shared.log.blame(*entity, *field);
                        let mut values = Vec::new();
                        let find = |s: Stamp| history.iter().position(|(h, _, _)| *h == s);
                        if let Some(i) = find(kept.0) {
                            values.push(self.value_dto(kept.0, &history[i].1, &kept.1, true, i));
                        }
                        for (s, v) in lost {
                            if let Some(i) = find(*s) {
                                values.push(self.value_dto(*s, &history[i].1, v, false, i));
                            }
                        }
                        (self.place_of(*entity, Some(*field)), values)
                    }
                    r => (r.entity().and_then(|u| self.place_of(u, None)), Vec::new()),
                };
                ReviewDto {
                    index,
                    key: item.review.key().to_string(),
                    sentence: self.sentence_with_names(&item.sentence),
                    place,
                    values,
                }
            })
            .collect()
    }

    /// A review sentence with people's names in place of actor ids.
    fn sentence_with_names(&self, s: &str) -> String {
        let mut s = s.to_string();
        for actor in self.shared.log.frontier().0.keys() {
            s = s.replace(&actor.short(), &self.name_of(*actor));
        }
        s
    }

    /// The entity for a place the page names: a row by id, an account or
    /// category by code, a setup item by kind and name.
    pub fn entity_at(&self, kind: &str, key: &str) -> Option<Uid> {
        let b = &self.proj.binding;
        match kind {
            "row" => b.rows.get(&key.parse::<RowId>().ok()?).copied(),
            "account" => b.accounts.get(key).copied(),
            "category" => b.categories.get(key).copied(),
            "budget" => Some(Uid::ROOT),
            other => {
                let k = Kind::parse(other)?;
                b.names.get(&(k, key.to_string())).copied()
            }
        }
    }

    /// Who changed a field, oldest first.
    pub fn blame(&self, kind: &str, key: &str, field: &str) -> Result<Vec<ValueDto>, String> {
        let uid = self
            .entity_at(kind, key)
            .ok_or_else(|| tr("shared.entity_gone"))?;
        let field = Field::parse(field).ok_or_else(|| tr("shared.unknown_field"))?;
        let history = self.shared.log.blame(uid, field);
        let last = history.len().saturating_sub(1);
        Ok(history
            .iter()
            .enumerate()
            .map(|(i, (s, time, v))| self.value_dto(*s, time, v, i == last, i))
            .collect())
    }

    /// Writes value `index` of a field's history back as this person's
    /// change. Returns the budget to adopt.
    pub fn use_value(
        &mut self,
        kind: &str,
        key: &str,
        field: &str,
        index: usize,
    ) -> Result<Budget, String> {
        let uid = self
            .entity_at(kind, key)
            .ok_or_else(|| tr("shared.entity_gone"))?;
        let f = Field::parse(field).ok_or_else(|| tr("shared.unknown_field"))?;
        let history = self.shared.log.blame(uid, f);
        let (_, _, value) = history.get(index).ok_or_else(|| tr("shared.value_gone"))?;
        let place = self.place_of(uid, Some(f));
        let label = match place {
            Some(p) => msg(
                "engine.undo.put_back_field",
                &[("field", &f.name()), ("place", &p.text)],
            ),
            None => msg("engine.undo.put_back_field_budget", &[("field", &f.name())]),
        };
        let id = self
            .shared
            .log
            .set_field(uid, f, value.clone(), &label, &iso_now())
            .map_err(|e| self.refused_text(&e))?
            .ok_or_else(|| tr("shared.entity_gone"))?;
        self.reproject();
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        self.steps.push(Step {
            id,
            label,
            at: now_ms(),
        });
        self.redo.clear();
        Ok(self.proj.budget.clone())
    }

    /// Recent changes by everyone, newest first.
    pub fn activity(&self, limit: usize) -> Vec<ActivityDto> {
        let changes: Vec<_> = self.shared.log.changes().collect();
        changes
            .iter()
            .rev()
            .take(limit)
            .map(|c| {
                let first = c.ops.first().map(|op| {
                    let field = match op {
                        method_core::log::Op::Set { field, .. } => Some(*field),
                        _ => None,
                    };
                    (op.entity(), field)
                });
                ActivityDto {
                    actor: c.actor.hex(),
                    name: self.name_of(c.actor),
                    color: self.color_of(c.actor),
                    me: c.actor == self.actor(),
                    time: c.time.clone(),
                    label: crate::i18n::label(&c.label),
                    ops: c.ops.len(),
                    place: first.and_then(|(u, f)| self.place_of(u, f)),
                    signed: c.sig.is_some(),
                }
            })
            .collect()
    }
}
