//! Roles, keys and removing a person (decisions.md, entries 78 and 89).

use super::*;

impl SharedDoc {
    // Roles (decisions.md, entry 78)

    /// This person's role.
    pub fn role(&self) -> Role {
        self.shared.log.role_of(self.actor())
    }

    /// The owner's name, for "ask them" sentences.
    pub(super) fn owner_name(&self) -> String {
        self.shared
            .log
            .owner()
            .map_or_else(|| tr("shared.the_owner"), |o| self.name_of(o))
    }

    /// Why this person can't change anything, when their role reads only.
    pub fn refusal(&self) -> Option<String> {
        if let Some(w) = self.replaced_text() {
            return Some(w);
        }
        if let Some(w) = self.key_warning() {
            return Some(w);
        }
        if let Some(w) = self.owner_key_warning() {
            return Some(w);
        }
        if self.shared.log.removed(self.actor()) {
            return Some(tr_with(
                "shared.removed_ask",
                &[("owner", self.owner_name().as_str())],
            ));
        }
        self.role().reads_only().then(|| {
            tr_with(
                "shared.read_only_ask",
                &[("owner", self.owner_name().as_str())],
            )
        })
    }

    /// A refused change in the words the page shows: what this person's
    /// role allows, and who to ask.
    pub(super) fn refused_text(&self, e: &LogError) -> String {
        match e {
            LogError::Unsigned { reason, .. }
                if reason == method_core::log::sign::why::KEY_TAKEN =>
            {
                return self.key_warning().unwrap_or_default()
            }
            LogError::Unsigned { reason, .. }
                if reason == method_core::log::sign::why::OWNER_KEY_CHANGED =>
            {
                return self.owner_key_warning().unwrap_or_default()
            }
            LogError::Unsigned { .. } => return tr("shared.unsigned"),
            LogError::Removed { .. } => {
                return self.refusal().unwrap_or_else(|| tr("shared.removed"))
            }
            LogError::NotAllowed { .. } => {}
            _ => return tr_with("shared.not_shared", &[("why", e.to_string().as_str())]),
        }
        let owner = self.owner_name();
        let ask = tr_with("shared.ask", &[("owner", owner.as_str())]);
        match self.role() {
            Role::Department(scope) => tr_with(
                "shared.department_only",
                &[
                    ("scope", self.scope_words(&scope).as_str()),
                    ("owner", owner.as_str()),
                ],
            ),
            Role::Actuals => tr_with("shared.actuals_only", &[("owner", owner.as_str())]),
            Role::Comment => tr_with("shared.notes_only", &[("owner", owner.as_str())]),
            Role::ReadOnly => self.refusal().unwrap_or(ask),
            Role::Editor => tr("shared.editor_limits"),
            Role::Admin => tr("shared.only_make_admins"),
            Role::CoOwner => tr("shared.only_make_coowners"),
            Role::Owner => tr_with("shared.not_shared", &[("why", e.to_string().as_str())]),
        }
    }

    /// A department editor's categories and accounts, by number, in
    /// budget order.
    pub(super) fn scope_codes(&self, scope: &std::collections::BTreeSet<Uid>) -> Vec<String> {
        let b = &self.proj.budget;
        let mut out = Vec::new();
        for c in b.categories() {
            if self
                .proj
                .binding
                .categories
                .get(&c.code)
                .is_some_and(|u| scope.contains(u))
            {
                out.push(c.code.clone());
            }
            for a in &c.accounts {
                if self
                    .proj
                    .binding
                    .accounts
                    .get(&a.code)
                    .is_some_and(|u| scope.contains(u))
                {
                    out.push(a.code.clone());
                }
            }
        }
        out
    }

    /// "2000 Production and 5000 Post".
    fn scope_words(&self, scope: &std::collections::BTreeSet<Uid>) -> String {
        self.codes_words(&self.scope_codes(scope))
    }

    /// Category and account numbers with their names: "2000 Production and
    /// 2400 Grip".
    pub(super) fn codes_words(&self, codes: &[String]) -> String {
        let b = &self.proj.budget;
        let names: Vec<String> = codes
            .iter()
            .cloned()
            .map(|code| {
                let name = b
                    .categories()
                    .find(|c| c.code == code)
                    .map(|c| c.name.clone())
                    .or_else(|| {
                        b.categories()
                            .flat_map(|c| &c.accounts)
                            .find(|a| a.code == code)
                            .map(|a| a.name.clone())
                    })
                    .unwrap_or_default();
                if name.is_empty() {
                    code
                } else {
                    format!("{code} {name}")
                }
            })
            .collect();
        match names.as_slice() {
            [] => "no categories".into(),
            [one] => one.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        }
    }

    /// A role in words, as the People list shows it.
    pub fn role_text(&self, role: &Role) -> String {
        crate::i18n::phrase(&self.role_words(role))
    }

    /// A role in the catalog's English, as a change's label holds it (the
    /// label is stored, so it stays English; [`Self::role_text`] is what
    /// the person reads).
    pub(super) fn role_words(&self, role: &Role) -> String {
        match role {
            Role::Owner => word("engine.role.owner"),
            Role::CoOwner => word("engine.role.co_owner"),
            Role::Admin => word("engine.role.admin"),
            Role::Editor => word("engine.role.editor"),
            Role::Department(scope) => msg(
                "engine.role.department",
                &[("scope", &self.scope_words(scope))],
            ),
            Role::Actuals => word("engine.role.actuals"),
            Role::Comment => word("engine.role.comment"),
            Role::ReadOnly => word("engine.role.read_only"),
        }
    }

    /// Sets what the person `actor` (64 hex characters) may change:
    /// `role` is "co_owner", "admin", "editor", "department", "actuals", "comment" or
    /// "read_only"; `scope` lists a department editor's categories or
    /// accounts by number. One change by this person, shared like any
    /// edit; not an undo step (set the role again to change it back).
    pub fn set_role(&mut self, actor: &str, role: &str, scope: &[String]) -> Result<(), String> {
        let who = ActorId::parse(actor).ok_or_else(|| tr("shared.which_person"))?;
        if Some(who) == self.shared.log.owner() || role == "owner" {
            return Err(tr("shared.one_owner"));
        }
        if role != "department" && !scope.is_empty() {
            return Err(tr("shared.scope_only_department"));
        }
        let mut uids = std::collections::BTreeSet::new();
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
        let role = Role::parse(role, uids).ok_or_else(|| tr("shared.not_a_role"))?;
        if role.scope().is_some_and(|s| s.is_empty()) {
            return Err(tr("shared.pick_category"));
        }
        let label = msg(
            "engine.undo.set_role",
            &[
                ("name", &self.name_of(who)),
                ("role", &self.role_words(&role)),
            ],
        );
        // Tied to the key in their name (decisions.md, entry 103). Once
        // signing has started, a role beyond read only goes only to someone
        // whose key is in the budget.
        let key = self.shared.log.key_of(who);
        if key.is_none() && self.shared.log.signing_started() && role != Role::ReadOnly {
            return Err(tr_with(
                "shared.no_key_yet",
                &[("name", &self.name_of(who))],
            ));
        }
        // A role from before roles were tied to keys is tied by Confirm key,
        // after the code check, never by setting a role over it.
        let current = self.shared.log.role_of(who);
        if role != Role::ReadOnly
            && current != Role::ReadOnly
            && key.is_some()
            && self.shared.log.role_key(who) != key
        {
            return Err(tr_with(
                "shared.confirm_key_first",
                &[("name", &self.name_of(who))],
            ));
        }
        let me = self.role();
        self.shared
            .log
            .grant_tied(who, role, key, &label, &iso_now())
            .map_err(|e| match me {
                Role::Admin => tr("shared.only_make_admins"),
                Role::CoOwner => tr("shared.only_make_coowners"),
                Role::Owner => tr_with("shared.role_not_set", &[("why", e.to_string().as_str())]),
                _ => tr_with(
                    "shared.only_set_roles",
                    &[("owner", self.owner_name().as_str())],
                ),
            })?;
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        Ok(())
    }

    /// Ties the person `actor`'s role to the key in their name, after the
    /// owner has checked its code with them (decisions.md, entry 103): the
    /// same role given again, with that key. `code` is the code the owner
    /// was shown, so a key that changed in between isn't tied.
    pub fn confirm_key(&mut self, actor: &str, code: &str) -> Result<(), String> {
        let who = ActorId::parse(actor).ok_or_else(|| tr("shared.which_person"))?;
        let key = self
            .shared
            .log
            .key_of(who)
            .ok_or_else(|| tr_with("shared.no_key_yet", &[("name", &self.name_of(who))]))?;
        if key_code(&key) != code {
            return Err(tr("shared.key_changed"));
        }
        let role = self.shared.log.role_of(who);
        let label = msg("engine.undo.confirm_key", &[("name", &self.name_of(who))]);
        let me = self.role();
        self.shared
            .log
            .grant_tied(who, role, Some(key), &label, &iso_now())
            .map_err(|e| match me {
                Role::Owner => tr_with("shared.role_not_set", &[("why", e.to_string().as_str())]),
                _ => tr_with(
                    "shared.only_set_roles",
                    &[("owner", self.owner_name().as_str())],
                ),
            })?;
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        Ok(())
    }

    /// Signs this person's changes with `key` (this computer's key pair,
    /// kept in the app's data folder, never in the shared folder;
    /// decisions.md, entry 89). When the budget's changes are signed, or
    /// this is the owner of a budget with roles, the key goes into the
    /// log now, as a change of its own.
    /// Where this computer remembers owner keys (the app passes
    /// [`flog::owner_keys_dir`]; tests pass a folder of their own).
    pub fn set_owner_keys_dir(&mut self, dir: Option<PathBuf>) {
        self.owner_keys = dir;
    }

    pub fn use_key(&mut self, key: std::sync::Arc<dyn Signer>) -> Result<(), String> {
        self.shared.log.set_signer(key.clone());
        self.signer = Some(key);
        self.publish_key()?;
        self.remember_owner_key();
        Ok(())
    }

    /// Remembers the owner's key the first time this computer sees it
    /// (docs/research/owner-key.md, part 2).
    pub(super) fn remember_owner_key(&mut self) {
        let Some(dir) = self.owner_keys.clone() else {
            return;
        };
        if self.signer.is_none() {
            return;
        }
        let me = self.actor();
        if let Err(e) = flog::remember_owner_key(&mut self.shared, &dir, me, &iso_now()) {
            self.error = Some(tr_with(
                "shared.cant_remember_key",
                &[("why", e.0.as_str())],
            ));
        }
    }

    /// Start Again (docs/research/owner-key.md, part 1): on the owner's
    /// computer, when a key in the owner's name isn't this computer's, a new
    /// shared budget made from this one as it stands, signed with this
    /// computer's key and with everyone's roles, takes the log folder's
    /// place; the old one is kept aside. The budget itself doesn't change;
    /// who changed what and undo start fresh, and everyone else reopens it.
    pub fn start_again(&mut self) -> Result<(), String> {
        if self.shared.log.owner() != Some(self.actor()) || self.shared.log.key_conflict().is_none()
        {
            return Err(tr("shared.start_again_owner"));
        }
        let signer = self.signer.clone().ok_or_else(|| tr("shared.no_key"))?;
        // Everything of this person's is in the folder first, so the old
        // log kept aside is whole.
        self.flush(Instant::now(), true);
        let shared = flog::start_again(
            &self.shared,
            signer,
            self.owner_keys.as_deref(),
            Ids::from_os(),
            &iso_now(),
        )
        .map_err(|e| e.0)?;
        self.adopt_log(shared);
        self.keeper = true;
        let path = self.shared.path.clone();
        self.note_snapshot(&path);
        Ok(())
    }

    /// Opens this budget afresh from its folder: joins the budget the owner
    /// started again, or takes in what a newly trusted owner key kept
    /// aside. Returns the budget as it now stands.
    pub fn reopen(&mut self) -> Result<Budget, String> {
        let actor = self.actor();
        let path = self.shared.path.clone();
        let (shared, report) =
            flog::open_in(&path, actor, Ids::from_os(), self.owner_keys.as_deref())
                .map_err(|e| e.0)?;
        self.keeper = shared.info.genesis.actor == actor;
        self.adopt_log(shared);
        self.waiting = report.pending;
        if let Some(signer) = self.signer.clone() {
            self.use_key(signer)?;
        }
        Ok(self.proj.budget.clone())
    }

    /// Trust the New Key (docs/research/owner-key.md, part 2): this
    /// computer remembers the owner's new key in place of the one it saw
    /// before (an owner who moved to a new computer), and the budget is
    /// opened afresh so what the old key kept aside comes in.
    pub fn trust_owner_key(&mut self) -> Result<Budget, String> {
        let key = self
            .shared
            .log
            .owner_key_changed()
            .ok_or_else(|| tr("shared.owner_key_same"))?;
        let dir = self
            .owner_keys
            .clone()
            .ok_or_else(|| tr("shared.nowhere_to_remember"))?;
        flog::trust_owner_key(&dir, &self.shared.path, &self.shared.info, key, &iso_now())
            .map_err(|e| e.0)?;
        self.reopen()
    }

    /// Takes `shared` in place of this budget's log: a budget started again,
    /// or the same budget read afresh. Undo, the recent list and the
    /// counts start over.
    fn adopt_log(&mut self, shared: Shared) {
        self.shared = shared;
        if let Some(signer) = self.signer.clone() {
            self.shared.log.set_signer(signer);
        }
        self.written_seq = self.shared.dir.highest_written(self.actor());
        self.snapshot_len = self.shared.log.len();
        self.steps.clear();
        self.redo.clear();
        self.recent.clear();
        self.replaced = None;
        self.error = None;
        self.waiting = 0;
        self.scan = self.shared.dir.scan();
        self.last_sync = Some(Instant::now());
        self.reproject();
    }

    /// When the owner's key isn't the one this computer saw first, what to
    /// tell the person (docs/research/owner-key.md, part 2).
    pub fn owner_key_warning(&self) -> Option<String> {
        self.shared.log.owner_key_changed()?;
        Some(tr_with(
            "shared.owner_key_changed",
            &[("owner", self.owner_name().as_str())],
        ))
    }

    /// What to tell the person once the owner started the budget again.
    pub(super) fn replaced_text(&self) -> Option<String> {
        self.replaced.as_ref()?;
        Some(tr("shared.replaced"))
    }

    /// Puts this computer's key into the log when it should be there and
    /// isn't (after the owner's key arrives, for everyone else).
    pub(super) fn publish_key(&mut self) -> Result<(), String> {
        let added = self
            .shared
            .log
            .publish_key("Added this computer\u{2019}s key", &iso_now())
            .map_err(|e| self.refused_text(&e))?;
        let signed = self.sign_profile()?;
        if added.is_some() || signed {
            self.reproject();
            let now = Instant::now();
            self.last_edit = Some(now);
            self.unwritten_since.get_or_insert(now);
            self.last_change = now;
        }
        Ok(())
    }

    /// Writes this person's name and color as a signed change of their own
    /// (decisions.md, entry 103) when it isn't what the log already says.
    /// Only once this computer's key is in the log: a budget shared without
    /// roles stays at the history format it has. Whether one was written.
    pub(super) fn sign_profile(&mut self) -> Result<bool, String> {
        let log = &self.shared.log;
        let mine = log.signer_key();
        if mine.is_none()
            || log.key_of(self.actor()) != mine
            || method_core::log::profile::normal_name(&self.me.name).is_empty()
        {
            return Ok(false);
        }
        let label = msg("engine.undo.profile", &[("name", &self.me.name)]);
        let me = self.me.clone();
        let wrote = self
            .shared
            .log
            .set_profile(&me.name, &me.color, &label, &iso_now())
            .map_err(|e| self.refused_text(&e))?;
        Ok(wrote.is_some())
    }

    /// Removes the person `actor` (64 hex characters): they read only, and
    /// their changes this computer hasn't seen are left out on every
    /// computer (decisions.md, entry 89). One change by this person, shared
    /// like any edit; giving them a role again lets them back in from then on.
    pub fn remove_person(&mut self, actor: &str) -> Result<(), String> {
        let who = ActorId::parse(actor).ok_or_else(|| tr("shared.which_person"))?;
        if Some(who) == self.shared.log.owner() {
            return Err(tr("shared.owner_stays"));
        }
        if who == self.actor() {
            return Err(tr("shared.not_yourself"));
        }
        let label = msg("engine.undo.remove_person", &[("name", &self.name_of(who))]);
        let me = self.role();
        self.shared
            .log
            .revoke(who, &label, &iso_now())
            .map_err(|e| match (me, e) {
                (_, e @ (LogError::Unsigned { .. } | LogError::Removed { .. })) => {
                    self.refused_text(&e)
                }
                (Role::Admin, _) => tr("shared.only_remove_admins"),
                (Role::CoOwner, _) => tr("shared.only_remove_coowners"),
                (Role::Owner, e) => {
                    tr_with("shared.not_removed", &[("why", e.to_string().as_str())])
                }
                _ => tr_with(
                    "shared.only_remove_people",
                    &[("owner", self.owner_name().as_str())],
                ),
            })?;
        self.reproject();
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        Ok(())
    }
}
