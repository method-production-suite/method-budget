# Born-Secure Checklist
- [ ] Lockfile committed from day one
- [ ] No plaintext secrets — `.env` gitignored, values in 1Password (`op://`)
- [ ] Secrets in the team 1Password vault (or the app's own vault if it gets a service account), titled `Thing - What - Where`, one `app/<this-project>` tag
- [ ] A runtime service account reads only this app's vault (never a shared all-vault token)
- [ ] Dependency cooldown inherited from global config (IT-18)
- [ ] CI permissions scoped: `permissions: contents: read`
- [ ] Third-party Actions SHA-pinned (`pinact run`)
