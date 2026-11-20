//! Access requests (docs/research/collaboration.md, 9.7).

use super::*;

impl SharedDoc {
    // Access requests (docs/research/collaboration.md, 9.7; docs/APP.md,
    // "Asking for access")

    /// The category an account is in, by number.
    fn category_of_account(&self, account: &str) -> Option<String> {
        self.proj
            .budget
            .categories()
            .find(|c| c.accounts.iter().any(|a| a.code == account))
            .map(|c| c.code.clone())
    }

    /// Whether `role` lets its person change lines in category or account
    /// `code`.
    fn covers(&self, role: &Role, code: &str) -> bool {
        match role {
            Role::Owner | Role::CoOwner | Role::Admin | Role::Editor => true,
            Role::Department(scope) => {
                let codes = self.scope_codes(scope);
                codes.iter().any(|c| c == code)
                    || (self.proj.binding.accounts.contains_key(code)
                        && self
                            .category_of_account(code)
                            .is_some_and(|cat| codes.contains(&cat)))
            }
            _ => false,
        }
    }

    /// A department editor's categories with no part in their scope, and
    /// accounts outside it; empty for every other role.
    pub(super) fn locked(&self) -> (Vec<String>, Vec<String>) {
        let role = self.role();
        if !matches!(role, Role::Department(_)) || self.shared.log.removed(self.actor()) {
            return (Vec::new(), Vec::new());
        }
        let mut cats = Vec::new();
        let mut accounts = Vec::new();
        for c in self.proj.budget.categories() {
            let mut any = self.covers(&role, &c.code);
            for a in &c.accounts {
                if self.covers(&role, &a.code) {
                    any = true;
                } else {
                    accounts.push(a.code.clone());
                }
            }
            if !any {
                cats.push(c.code.clone());
            }
        }
        (cats, accounts)
    }

    /// Whether `actor` already has what `req` asks for: the categories or
    /// accounts it names, for a department; the role or more, otherwise.
    pub(super) fn has_asked(&self, actor: ActorId, req: &AccessRequest) -> bool {
        if self.shared.log.removed(actor) {
            return false;
        }
        let role = self.shared.log.role_of(actor);
        let covered = !req.codes.is_empty() && req.codes.iter().all(|c| self.covers(&role, c));
        let full = matches!(
            role,
            Role::Owner | Role::CoOwner | Role::Admin | Role::Editor
        );
        match req.role.as_str() {
            "department" => covered || full,
            "editor" => full || covered,
            "actuals" => full || role == Role::Actuals || covered,
            "comment" => full || matches!(role, Role::Actuals | Role::Comment) || covered,
            _ => true,
        }
    }

    /// What Allow gives for `req` by `actor` (`only`: the categories or
    /// accounts asked about, as a department editor), as a role key and
    /// scope for [`SharedDoc::set_role`].
    fn granted_for(
        &self,
        actor: ActorId,
        req: &AccessRequest,
        only: bool,
    ) -> (String, Vec<String>) {
        if only || req.role == "department" {
            let mut scope = match self.shared.log.role_of(actor) {
                Role::Department(s) if !self.shared.log.removed(actor) => self.scope_codes(&s),
                _ => Vec::new(),
            };
            for c in &req.codes {
                if !scope.contains(c) {
                    scope.push(c.clone());
                }
            }
            ("department".into(), scope)
        } else {
            (req.role.clone(), Vec::new())
        }
    }

    /// A scope's numbers as the uids a role holds (unknown numbers left out).
    fn scope_uids(&self, codes: &[String]) -> BTreeSet<Uid> {
        codes
            .iter()
            .filter_map(|c| {
                self.proj
                    .binding
                    .categories
                    .get(c)
                    .or_else(|| self.proj.binding.accounts.get(c))
                    .copied()
            })
            .collect()
    }

    /// A request as the panel shows it.
    pub(super) fn request_dto(&self, actor: ActorId, req: &AccessRequest) -> RequestDto {
        let (key, scope) = self.granted_for(actor, req, false);
        let given = Role::parse(&key, self.scope_uids(&scope)).unwrap_or(Role::ReadOnly);
        let name = self.name_of(actor);
        let same = |n: &str| n.trim().to_lowercase() == name.trim().to_lowercase();
        let name_taken = self
            .people
            .keys()
            .copied()
            .chain(std::iter::once(self.actor()))
            .chain(self.shared.log.owner())
            .filter(|a| *a != actor)
            .any(|a| same(&self.name_of(a)));
        RequestDto {
            actor: actor.hex(),
            name_taken,
            // Applied changes only: a refused one still moves the frontier.
            new_here: !self.shared.log.changes().any(|c| c.actor == actor),
            code: computer_code(actor),
            name,
            color: self.color_of(actor),
            role: req.role.clone(),
            role_text: self.role_text(&given),
            codes: req.codes.clone(),
            codes_text: self.codes_words(&req.codes),
            only: !req.codes.is_empty() && req.role != "department",
            note: req.note.clone(),
            at: req.at.clone(),
        }
    }

    /// Whether this person could allow `req` by `actor` under the rules for
    /// setting roles.
    fn may_allow(&self, actor: ActorId, req: &AccessRequest) -> bool {
        if actor == self.actor() || Some(actor) == self.shared.log.owner() {
            return false;
        }
        let (key, scope) = self.granted_for(actor, req, false);
        let Some(given) = Role::parse(&key, self.scope_uids(&scope)) else {
            return false;
        };
        method_core::log::may_give(&self.role(), &self.shared.log.role_of(actor), &given)
    }

    /// The requests waiting that this person could allow, oldest first:
    /// not declined here, not yet given, from people (never programs).
    pub fn requests(&self) -> Vec<RequestDto> {
        if self.replaced.is_some() || !self.role().sets_roles() {
            return Vec::new();
        }
        let mut out: Vec<(String, RequestDto)> = self
            .people
            .iter()
            .filter_map(|(actor, p)| {
                let req = p.request.as_ref()?;
                if !ASKABLE.contains(&req.role.as_str())
                    || self.person_for(*actor).is_some()
                    || self.declined.contains(&Declined {
                        actor: actor.hex(),
                        at: req.at.clone(),
                    })
                    || self.has_asked(*actor, req)
                    || !self.may_allow(*actor, req)
                {
                    return None;
                }
                Some((req.at.clone(), self.request_dto(*actor, req)))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out.into_iter().map(|(_, r)| r).collect()
    }

    /// Whether this person may ask for access: roles are set and theirs is
    /// less than everything.
    pub fn can_ask(&self) -> bool {
        self.replaced.is_none()
            && self.shared.log.roles_started()
            && self.shared.log.owner() != Some(self.actor())
            && matches!(
                self.role(),
                Role::Department(_) | Role::Actuals | Role::Comment | Role::ReadOnly
            )
    }

    /// Asks the owner, co-owners and admins for `role` ("editor",
    /// "department", "actuals" or "comment"), about the categories or
    /// accounts `codes`, with a short `note`: written into this computer's
    /// heartbeat now, replacing any request before it. Nothing goes into the
    /// change log and nothing leaves the budget's folder.
    pub fn request_access(
        &mut self,
        role: &str,
        codes: &[String],
        note: &str,
    ) -> Result<(), String> {
        if !self.shared.log.roles_started() {
            return Err(tr("shared.nothing_to_ask"));
        }
        if !self.can_ask() {
            return Err(tr("shared.can_change_all"));
        }
        if !ASKABLE.contains(&role) {
            return Err(tr("shared.not_askable"));
        }
        let mut list: Vec<String> = Vec::new();
        for code in codes {
            let code = code.trim().to_string();
            if code.is_empty() || list.contains(&code) {
                continue;
            }
            if !self.proj.binding.categories.contains_key(&code)
                && !self.proj.binding.accounts.contains_key(&code)
            {
                return Err(tr_with("shared.no_code", &[("code", code.as_str())]));
            }
            list.push(code);
        }
        if role == "department" && list.is_empty() {
            return Err(tr("shared.say_which"));
        }
        let note: String = note
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(REQUEST_NOTE_MAX_CHARS)
            .collect();
        let req = AccessRequest {
            role: role.to_string(),
            codes: list,
            note,
            at: iso_now(),
        };
        if self.has_asked(self.actor(), &req) {
            return Err(tr("shared.can_change_that"));
        }
        self.request = Some(req);
        self.heartbeat(Instant::now(), true);
        Ok(())
    }

    /// Takes this person's request back.
    pub fn withdraw_request(&mut self) {
        if self.request.take().is_some() {
            self.heartbeat(Instant::now(), true);
        }
    }

    /// The request by `actor` (64 hex characters) made at `at`, as its
    /// heartbeat holds it now.
    fn request_of(&self, actor: &str, at: &str) -> Result<(ActorId, AccessRequest), String> {
        let who = ActorId::parse(actor).ok_or_else(|| tr("shared.which_person"))?;
        let req = self
            .people
            .get(&who)
            .and_then(|p| p.request.clone())
            .filter(|r| r.at == at)
            .ok_or_else(|| tr("shared.request_gone"))?;
        Ok((who, req))
    }

    /// Allows the request by `actor` made at `at`: gives what it asks for,
    /// or with `only` the categories or accounts it names as a department
    /// editor (added to the ones they have), through [`SharedDoc::set_role`],
    /// so it is checked and signed like any role set in the Share dialog.
    pub fn allow_request(&mut self, actor: &str, at: &str, only: bool) -> Result<(), String> {
        let (who, req) = self.request_of(actor, at)?;
        if only && req.codes.is_empty() {
            return Err(tr("shared.request_no_codes"));
        }
        let (key, scope) = self.granted_for(who, &req, only);
        self.set_role(actor, &key, &scope)
    }

    /// Declines the request by `actor` made at `at`: not shown again on
    /// this computer (remembered beside this computer's sharing id, never in
    /// the shared folder); nothing is written to the change log. A request
    /// made again later shows again.
    pub fn decline_request(&mut self, actor: &str, at: &str) -> Result<(), String> {
        let (who, req) = self.request_of(actor, at)?;
        self.declined.insert(Declined {
            actor: who.hex(),
            at: req.at,
        });
        self.write_declined()
    }

    /// Where this computer remembers declined requests (the app passes a
    /// folder beside the sharing id; tests pass their own). Reads what is
    /// remembered there for this budget.
    pub fn set_declined_dir(&mut self, dir: Option<PathBuf>) {
        self.declined_dir = dir;
        if let Some(path) = self.declined_path() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(list) = serde_json::from_str::<Vec<Declined>>(&text) {
                    self.declined.extend(list);
                }
            }
        }
    }

    fn declined_path(&self) -> Option<PathBuf> {
        let dir = self.declined_dir.as_ref()?;
        Some(dir.join(format!("{}.json", self.shared.info.budget_id)))
    }

    fn write_declined(&self) -> Result<(), String> {
        let Some(path) = self.declined_path() else {
            return Ok(());
        };
        let list: Vec<&Declined> = self.declined.iter().collect();
        let text = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| tr_with("shared.cant_remember", &[("why", e.to_string().as_str())]))?;
        }
        std::fs::write(&path, text)
            .map_err(|e| tr_with("shared.cant_remember", &[("why", e.to_string().as_str())]))
    }
}
