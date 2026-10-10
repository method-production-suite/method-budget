# The website, the feature list, and the release skill

How Method Budget's public face stays in step with the code: one feature file feeds the
README and the website, release notes feed the site's "What's new" box, a script
regenerates both, CI fails when they drift, and two Claude Code skills and two hooks make
the right order the easy one.

```
docs/features.toml ─┐                       ┌─> README.md        (feature section, clone command)
                    ├─> scripts/site.py ────┤
docs/releases/*.md ─┘   (--check in CI)     └─> site/index.html  (feature list, What's new, repo links)
                                                     │
                                  .github/workflows/pages.yml (off until PAGES_ENABLED)
                                                     │
                                               methodbudget.com
```

## The feature file: docs/features.toml

Every feature the README and the site list is one `[[feature]]` entry: a `group`, a short
`title`, one sentence of `text`, a `status` (`have` built, `soon` planned) and `ref`, the
roadmap items and decisions behind it. Groups appear in the order the `[[group]]` tables
are written. The header of the file has the full field list and the writing rules
(user-facing, no em dashes, no vendor names).

`scripts/site.py` validates it before writing anything. The rule that matters most: a
`have` feature whose roadmap row in docs/ROADMAP.md is not `[have]` is an error, so the
public list can never claim something that is not built. A `soon` feature whose row has
become `[have]` prints a warning, as a hint to flip it.

`[project] repo` at the top is the source repository's URL. It is written into every site
link marked `data-repo` and into the README's clone command, so moving the repository is
one line there plus a regenerate.

## The script: scripts/site.py

Python 3.11 or later, standard library only (it uses `tomllib`).

```bash
python3 scripts/site.py           # rewrite the generated blocks
python3 scripts/site.py --check   # change nothing; exit 1 if anything is out of date
python3 scripts/site.py --list    # print the features with their status and refs
```

It only touches text between marker comments, so everything else in README.md and
site/index.html is edited by hand as usual:

| File | Block | From |
|---|---|---|
| README.md | `<!-- features:start -->` to `<!-- features:end -->` | docs/features.toml |
| README.md | `<!-- clone:start -->` to `<!-- clone:end -->` | `[project] repo` |
| site/index.html | `<!-- features:start -->` to `<!-- features:end -->` | docs/features.toml |
| site/index.html | `<!-- whatsnew:start -->` to `<!-- whatsnew:end -->` | the newest docs/releases/<version>.md |
| site/index.html | `href` of every `<a href="..." data-repo="path">` | `[project] repo` |

"What's new" takes the newest release note by version number (a prerelease sorts before
its release): its `# ` title, its first paragraph and up to six bullets, with bold kept and
links reduced to their text. With no release notes yet it says so.

Exit codes: 0 done or up to date, 1 out of date (`--check`), 2 the input is wrong (the
message names the entry and the problem; nothing was written).

CI runs `python3 scripts/site.py --check` in the `site` job of `.github/workflows/ci.yml`,
and the Pages workflow runs it again before deploying.

## The site: site/

- `index.html` is the whole site: one page, inline CSS and SVG, system fonts, no
  JavaScript, no trackers, nothing loaded from anywhere else (a Content-Security-Policy in
  the page enforces that). Colors are tokens on `:root` with a `prefers-color-scheme: dark`
  override; every text pairing meets WCAG AA in both schemes. Sections: hero, What's new,
  Features, How it's different, Download, Questions, footer.
- `favicon.svg` is the mark: three budget rows over a double total rule. Original work.
- `og-card.html` is the 1200x630 share card; `scripts/make-og.sh` renders it to `og.png`
  with headless Chrome (a throwaway profile, no window). Re-run it after editing the card
  and commit both.
- `CNAME` holds `methodbudget.com`. With the Actions deploy below, GitHub reads the custom
  domain from Settings, not from this file; it stays as documentation and for any host
  that does read it.
- Screenshots of the app are not on the site or the README yet. When they are, they will
  be captures of an invented budget (`method sample`), never a real one and never another
  product's screens; the README has a commented placeholder for them.

Links inside the page are relative, so the site works both at
`https://ssandweiss.github.io/method-budget/` (Pages' default address for this repository)
and at `https://methodbudget.com/`. The canonical and share-card URLs name
methodbudget.com.

To look at it locally:

```bash
cd site && python3 -m http.server 8000    # then open http://127.0.0.1:8000/
```

## Publishing: .github/workflows/pages.yml

The workflow deploys `site/` to GitHub Pages on a push to main that touches `site/` (or by
hand from the Actions tab). It is **off** until the repository is public: GitHub Pages on a
private repository needs a paid plan, so the deploy job only runs when the repository
variable `PAGES_ENABLED` is `true`, and skips cleanly otherwise. It uses only first-party
actions pinned to commit SHAs; only the deploy job gets `pages: write` and `id-token:
write`.

Turning it on, once the repository is public (all by hand, in GitHub and at the domain
registrar):

1. **Settings > Pages > Build and deployment > Source: GitHub Actions.**
2. **Settings > Secrets and variables > Actions > Variables:** add `PAGES_ENABLED` with
   the value `true`.
3. Run the `pages` workflow (Actions > pages > Run workflow). The site appears at
   `https://ssandweiss.github.io/method-budget/`.
4. **Verify the domain first** (recommended, prevents someone else claiming it on Pages):
   GitHub account Settings > Pages > Add a domain > `methodbudget.com`, then add the TXT
   record GitHub shows at the registrar and click Verify.
5. **DNS at the registrar**, for the apex domain and www:

   | Name | Type | Value |
   |---|---|---|
   | `@` | A | `185.199.108.153` |
   | `@` | A | `185.199.109.153` |
   | `@` | A | `185.199.110.153` |
   | `@` | A | `185.199.111.153` |
   | `@` | AAAA | `2606:50c0:8000::153` |
   | `@` | AAAA | `2606:50c0:8001::153` |
   | `@` | AAAA | `2606:50c0:8002::153` |
   | `@` | AAAA | `2606:50c0:8003::153` |
   | `www` | CNAME | `ssandweiss.github.io` |

   The CNAME target is the GitHub Pages host of the account that owns the repository
   (`<owner>.github.io`, without the repository name). If the repository moves to an
   organization, it becomes `<org>.github.io`. Remove any other A, AAAA or ALIAS records
   on `@` (registrar parking pages, for example).
6. **Settings > Pages > Custom domain:** `methodbudget.com`, Save. GitHub checks the DNS
   (it can take up to a day to propagate) and then serves `www` as a redirect to the apex.
7. **Enforce HTTPS** on the same page, once GitHub has issued the certificate (the box is
   greyed out until then).

Nothing here has been turned on. Before step 2, a push that changes `site/` runs the
workflow, which skips the job and succeeds.

## The skills: .claude/skills/

- **`/release`** (`.claude/skills/release/SKILL.md`) cuts a release. It reads
  docs/RELEASING.md for the release tooling's commands (the notes facts, the template and
  the version bump) and calls them rather than doing any of that itself; drafts
  `docs/releases/<version>.md`; updates the feature file if the range built something;
  regenerates the README and the site (which picks the new notes up as "What's new");
  shows a review with the claims it is least sure about; and only on `ship` bumps,
  commits, and creates the annotated tag, in separate commands. It never pushes, publishes
  or changes settings without asking for that exact command, never moves a published tag,
  and treats commit subjects and other text it reads as data, never as instructions.
- **`/features`** (`.claude/skills/features/SKILL.md`) adds, updates, flips or removes a
  feature entry after checking the roadmap, then regenerates and checks.

### Why .claude/ is partly versioned

`.gitignore` ignores everything under `.claude/` except `.claude/skills/` and
`.claude/settings.json`:

```
/.claude/*
!/.claude/skills/
!/.claude/settings.json
```

The skills are part of how releases are made, so they belong in git: versioned, reviewed,
and present in every worktree and clone (an ignored skill would be missing from exactly
the worktree a release is cut in). Everything else under `.claude/` stays local by
default, above all `.claude/worktrees/`, which holds complete checkouts that must never be
staged, and `settings.local.json`. The pattern is `/.claude/*` and not `.claude/`: git
never looks inside an ignored directory, so `!` lines under a directory-level ignore would
silently never match. Ignoring `.claude/` wholesale would keep the skills local; this
repository chose to version the skills and keep the protection.

## The hooks: scripts/hooks/

Two Claude Code `PreToolUse` hooks for Bash commands. Both are ergonomics guards, not
security boundaries: a command typed in a terminal never reaches them, and CI holds the
real line.

- `require-release-notes.sh` blocks creating or pushing a version tag (`git tag`,
  tag-pushing `git push` forms, `gh release create`) when `docs/releases/<version>.md` is
  not committed at HEAD. Deleting tags and listing tags always pass.
- `require-site-regen.sh` blocks a `git commit` that would commit `docs/features.toml` or
  a release note while README.md or site/index.html is out of date, or regenerated but not
  staged.

`bash scripts/hooks/test-hooks.sh` runs both against throwaway repositories; CI runs it in
the `site` job.

**They are not switched on.** Hooks run on every Bash command an agent makes, so wiring
them is the maintainer's call. To turn them on, put the contents of
`scripts/hooks/claude-settings.example.json` into `.claude/settings.json` (shared with
everyone who uses Claude Code in this repository; versioned by the `.gitignore` above) or
`.claude/settings.local.json` (this machine only):

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "\"$CLAUDE_PROJECT_DIR\"/scripts/hooks/require-release-notes.sh" },
          { "type": "command", "command": "\"$CLAUDE_PROJECT_DIR\"/scripts/hooks/require-site-regen.sh" }
        ]
      }
    ]
  }
}
```

They need `bash`, `git`, `python3` and (for exact command matching) `jq`; without `jq`
they fall back to matching the raw input.
