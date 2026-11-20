//! Invites (decisions.md, entry 97).

use super::*;

impl SharedDoc {
    // Invites (decisions.md, entry 97; docs/APP.md, "Inviting someone")

    /// Whether this person may invite someone now.
    pub fn can_invite(&self) -> bool {
        self.replaced.is_none()
            && self.shared.log.key_conflict().is_none()
            && self.shared.log.owner_key_changed().is_none()
            && !self.shared.log.removed(self.actor())
            && self.role().sets_roles()
    }

    /// Records an invite giving `role` ("editor", "department" with `scope`'s
    /// category or account numbers, "actuals", "comment", "read_only", or for
    /// the owner and co-owners "admin", and the owner "co_owner") for `days`
    /// days, and returns its link. The link holds the invite's secret, which
    /// no computer keeps: the log holds only its public key, so the link is
    /// shown once.
    pub fn create_invite(
        &mut self,
        role: &str,
        scope: &[String],
        days: u64,
    ) -> Result<CreatedInvite, String> {
        if let Some(t) = self.replaced_text() {
            return Err(t);
        }
        if !self.can_invite() {
            return Err(self.refusal().unwrap_or_else(|| {
                tr_with(
                    "shared.only_invite",
                    &[("owner", self.owner_name().as_str())],
                )
            }));
        }
        if !INVITE_DAYS.contains(&days) {
            return Err(tr("shared.invite_days"));
        }
        if role == "owner" {
            return Err(tr("shared.one_owner_invite"));
        }
        if role != "department" && !scope.is_empty() {
            return Err(tr("shared.scope_only_department"));
        }
        let mut uids = BTreeSet::new();
        for code in scope {
            let code = code.trim();
            let uid = self
                .proj
                .binding
                .categories
                .get(code)
                .or_else(|| self.proj.binding.accounts.get(code))
                .ok_or_else(|| tr_with("shared.no_code", &[("code", code)]))?;
            uids.insert(*uid);
        }
        let given = Role::parse(role, uids).ok_or_else(|| tr("shared.not_a_role_invite"))?;
        if given.scope().is_some_and(|s| s.is_empty()) {
            return Err(tr("shared.pick_category"));
        }
        if !method_core::log::may_give(&self.role(), &Role::ReadOnly, &given) {
            return Err(match self.role() {
                Role::CoOwner => tr("shared.only_invite_coowner"),
                _ => tr("shared.only_invite_admin"),
            });
        }
        let secret = flog::sign::new_invite_secret().map_err(|e| e.0)?;
        let id = flog::sign::new_invite_id().map_err(|e| e.0)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let expires = method_core::log::invite::rfc3339_at((now + days * 86_400) as i64);
        let role_text = self.role_text(&given);
        let label = msg("engine.undo.invite", &[("role", &self.role_words(&given))]);
        self.shared
            .log
            .invite(
                id,
                given,
                &expires,
                flog::sign::invite_key(&secret),
                &label,
                &iso_now(),
            )
            .map_err(|e| self.refused_text(&e))?;
        self.reproject();
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        let link = crate::invite_link::InviteLink {
            budget: self.shared.info.budget_id.clone(),
            invite: id,
            secret,
            file: self.file_name(),
        };
        Ok(CreatedInvite {
            link: link.to_text(),
            role_text,
            expires,
        })
    }

    /// Seals the invites this person may cancel that were used, or are past
    /// their date by this computer's clock: one cancel each, written like any
    /// change. True when anything was recorded.
    pub fn seal_invites(&mut self) -> bool {
        if !self.can_invite() {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let sealed = self.shared.log.seal_invites(now, &iso_now());
        if sealed.is_empty() {
            return false;
        }
        let at = Instant::now();
        self.last_edit = Some(at);
        self.unwritten_since.get_or_insert(at);
        self.last_change = at;
        true
    }

    /// The budget's file name, as the link names it.
    fn file_name(&self) -> String {
        self.shared
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.shared.info.file.clone())
    }

    /// The shared budget's id (`budget.json`'s `budget_id`).
    pub fn budget_id(&self) -> &str {
        &self.shared.info.budget_id
    }

    /// The invites waiting to be used and the ones used, oldest first, for
    /// someone who may give roles.
    pub fn invites(&self) -> Vec<InviteDto> {
        if !self.can_invite() {
            return Vec::new();
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let me = self.actor();
        let mine = self.role();
        let mut out: Vec<InviteDto> = self
            .shared
            .log
            .state()
            .invites()
            .values()
            .filter_map(|rec| {
                let used = rec.claimed.map(|c| c.actor);
                let open = method_core::log::invite::open(rec, now);
                if used.is_none() && !open {
                    return None;
                }
                let by = self.shared.log.person_of(rec.by);
                Some(InviteDto {
                    id: rec.id.hex(),
                    by: self.name_of(rec.by),
                    by_color: self.color_of(rec.by),
                    role: rec.role.key().to_string(),
                    role_text: self.role_text(&rec.role),
                    made: rec.time.clone(),
                    expires: rec.expires.clone(),
                    state: if used.is_some() { "used" } else { "open" }.into(),
                    joined: used.map(|a| self.name_of(a)),
                    joined_color: used.map(|a| self.color_of(a)),
                    can_cancel: open
                        && (by == me
                            || method_core::log::may_give(&mine, &Role::ReadOnly, &rec.role)),
                })
            })
            .collect();
        out.sort_by(|a, b| a.made.cmp(&b.made).then(a.id.cmp(&b.id)));
        out
    }

    /// Cancels invite `id` (32 hex characters): a claim this computer
    /// hasn't seen is refused on every computer. A used invite may be
    /// cancelled too, which seals it: the use this computer has seen stands.
    pub fn cancel_invite(&mut self, id: &str) -> Result<(), String> {
        let uid = Uid::parse(id).ok_or_else(|| tr("shared.which_invite"))?;
        let rec = self
            .shared
            .log
            .state()
            .invites()
            .get(&uid)
            .cloned()
            .ok_or_else(|| tr("shared.invite_gone"))?;
        if rec.cancelled.is_some() {
            return Ok(());
        }
        let label = msg(
            "engine.undo.cancel_invite",
            &[("role", &self.role_words(&rec.role))],
        );
        self.shared
            .log
            .cancel_invite(uid, &label, &iso_now())
            .map_err(|e| match e {
                LogError::NotAllowed { .. } => tr("shared.only_cancel"),
                e => self.refused_text(&e),
            })?;
        self.reproject();
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        Ok(())
    }

    /// Takes up the invite in `link` as this person (File > Join from
    /// Link…): checks it against the budget as it reached this computer,
    /// records the claim (signed, with this computer's key in the log first),
    /// and writes it to the folder now. Returns the role it gave, in words.
    /// The errors are sentences for the person.
    pub fn join_invite(&mut self, link: &crate::invite_link::InviteLink) -> Result<String, String> {
        if let Some(t) = self.replaced_text() {
            return Err(t);
        }
        if link.budget != self.shared.info.budget_id {
            return Err(wrong_budget(&link.file));
        }
        if let Some(w) = self.key_warning().or_else(|| self.owner_key_warning()) {
            return Err(w);
        }
        let Some(rec) = self.shared.log.state().invites().get(&link.invite).cloned() else {
            return Err(invite_words(method_core::log::invite::why::NOT_FOUND, None));
        };
        let me = self.actor();
        let role = self.role();
        let removed = self.shared.log.removed(me);
        if self.shared.log.owner() == Some(me) || (!removed && !role.reads_only()) {
            return Err(tr_with(
                "shared.already_member",
                &[("role", self.role_text(&role).as_str())],
            ));
        }
        if rec.claimed.is_some() {
            return Err(invite_words(method_core::log::invite::why::USED, None));
        }
        if rec.cancelled.is_some() {
            return Err(invite_words(method_core::log::invite::why::CANCELLED, None));
        }
        let now = iso_now();
        let expired = match (
            method_core::log::invite::rfc3339_secs(&now),
            method_core::log::invite::rfc3339_secs(&rec.expires),
        ) {
            (Some(a), Some(b)) => a > b,
            _ => true,
        };
        if expired {
            return Err(invite_words(
                method_core::log::invite::why::EXPIRED,
                Some(&rec.expires),
            ));
        }
        // This computer's key is in the log before the claim (it goes in
        // when the budget is opened; once more here in case it didn't).
        self.publish_key()?;
        let proof = flog::sign::claim_proof(&link.secret, link.invite, me);
        let label = msg(
            "engine.undo.join_from_invite",
            &[("role", &self.role_words(&rec.role))],
        );
        self.shared
            .log
            .claim(link.invite, proof, &label, &now)
            .map_err(|e| match e {
                LogError::Invite { reason, .. } => invite_words(&reason, Some(&rec.expires)),
                e => self.refused_text(&e),
            })?;
        self.reproject();
        let at = Instant::now();
        self.last_change = at;
        self.last_edit = Some(at);
        self.unwritten_since.get_or_insert(at);
        self.flush(at, true);
        Ok(self.role_text(&self.role()))
    }
}
