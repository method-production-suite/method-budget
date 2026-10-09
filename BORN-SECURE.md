# Born-Secure Checklist
- [ ] Lockfile committed from day one
- [ ] No plaintext secrets — `.env` gitignored, values in 1Password (`op://`)
- [ ] Secrets in the company's own vault (CWP: `CWP Tech`, or the app's own vault if it gets a service account; Darkwell: `Darkwell`; personal: `Personal Dev`), titled `Thing - What - Where`, one `app/<this-project>` tag
- [ ] A runtime service account reads only this app's vault (never a shared all-vault token)
- [ ] Dependency cooldown inherited from global config (IT-18)
- [ ] CI permissions scoped: `permissions: contents: read`
- [ ] Third-party Actions SHA-pinned (`pinact run`)
