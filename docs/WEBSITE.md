# The website, the feature list, and the release skill

How Method Budget's public face stays in step with the code: one feature file feeds the
README and the website, release notes feed the site's "What's new" box and its release
notes page, a script regenerates all of it, CI fails when they drift, and two Claude Code
skills and two hooks make the right order the easy one. How the site looks (name, voice,
colors, type, the mark) is in [BRAND.md](BRAND.md).

```
docs/features.toml ─┐                       ┌─> README.md                (feature section, clone command)
                    ├─> scripts/site.py ────┼─> site/index.html          (feature list, What's new, repo links)
docs/releases/*.md ─┘   (--check in CI)     └─> site/releases/index.html (every release note, repo links)
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
python3 scripts/site.py --files   # print the files it writes (the hook reads this)
```

It only touches text between marker comments, so everything else in the generated files
is edited by hand as usual:

| File | Block | From |
|---|---|---|
| README.md | `<!-- features:start -->` to `<!-- features:end -->` | docs/features.toml |
| README.md | `<!-- clone:start -->` to `<!-- clone:end -->` | `[project] repo` |
| site/index.html | `<!-- features:start -->` to `<!-- features:end -->` | docs/features.toml |
| site/index.html | `<!-- whatsnew:start -->` to `<!-- whatsnew:end -->` | the newest docs/releases/<version>.md |
| site/index.html | `href` of every `<a href="..." data-repo="path">` | `[project] repo` |
| site/releases/index.html | `<!-- releases:start -->` to `<!-- releases:end -->` | every docs/releases/<version>.md |
| site/releases/index.html | `href` of every `data-repo` link | `[project] repo` |

"What's new" takes the newest release note by version number (a prerelease sorts before
its release): its `# ` title, its first paragraph and up to six bullets, with bold kept and
links reduced to their text. The release notes page renders every note in full, newest
first, each with an anchor (`releases/#v0-1-0`): headings, paragraphs and bullets, in the
shape docs/release-notes-template.md fixes. HTML comments are dropped; links show their
text only, as in the app. With no release notes yet both say so.

Exit codes: 0 done or up to date, 1 out of date (`--check`), 2 the input is wrong (the
message names the entry and the problem; nothing was written).

CI runs `python3 scripts/site.py --check` in the `site` job of `.github/workflows/ci.yml`,
and the Pages workflow runs it again before deploying.

## The site: site/

Static files, no build step, no JavaScript, no trackers, no cookies. Every page carries
a Content-Security-Policy that allows only this site's own styles and images
(`default-src 'none'; style-src 'self'; img-src 'self'`), so no inline style, script or
third-party request can slip in unnoticed. System fonts only.

| Path | What it is |
|---|---|
| `index.html` | The home page: hero with the wordmark, an illustration of the app window drawn in HTML, What's new, Features, Why open files, Keyboard first, Command line and coding agents, Download, FAQ |
| `releases/index.html` | Every release note (generated block inside a hand-written page) |
| `404.html` | Served by GitHub Pages for any address with no page. Its links start at the site root, so it is right on methodbudget.com (at the `github.io/method-budget/` preview address its styles do not load) |
| `assets/tokens.css` | Brand colors and type, light and dark ([BRAND.md](BRAND.md)) |
| `assets/site.css` | Layout and components; names no colors of its own |
| `assets/brand/` | Mark and wordmark SVGs, light and dark (`scripts/make-brand.py`) |
| `favicon.svg`, `favicon.ico`, `apple-touch-icon.png` | Browser and home-screen icons (`scripts/make-brand.py`) |
| `og.png`, `og-card.html`, `assets/og-card.css` | The 1200x630 share card and its source; `scripts/make-og.sh` renders it with headless Chrome (a throwaway profile, no window). Re-run after editing the card and commit all three |
| `robots.txt`, `sitemap.xml` | Crawlers: everything allowed; the sitemap lists `/` and `/releases/`. Add a line to the sitemap when a page is added |
| `CNAME` | `methodbudget.com`. A deploy from GitHub Actions ignores this file (the custom domain is set in Settings, below); it stays as documentation and for any other host |

Colors are tokens with a `prefers-color-scheme: dark` override, so the site follows the
visitor's system setting; every text pairing meets WCAG AA in both schemes (the table is
in BRAND.md). Layouts are mobile first and have no horizontal scroll at phone width; the
app illustration drops columns and its tools panel on narrow screens. Each page has
landmarks (header, nav, main, footer), a skip link and visible focus outlines.

Screenshots of the app are not on the site or the README yet. When they are, they will
be captures of an invented budget (`method sample`), never a real one and never another
product's screens; they replace the HTML illustration, and the README has a commented
placeholder for them.

Links inside the pages are relative (except 404.html), so the site works both at
`https://ssandweiss.github.io/method-budget/` (Pages' default address for this
repository) and at `https://methodbudget.com/`. The canonical and share-card URLs name
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
registrar). The order matters: GitHub asks for the custom domain to be added to Pages
**before** the DNS records point at it, so nobody else can claim the domain in between,
and recommends verifying the domain first. Checked against GitHub's Pages documentation
in October 2026.

1. **Verify the domain** (prevents takeovers). Profile picture > **Settings** >
   **Pages** (under "Code, planning, and automation") > **Add a domain** > enter
   `methodbudget.com` > **Add domain**. Add the TXT record GitHub shows at the registrar
   (named `_github-pages-challenge-<owner>.methodbudget.com`), wait for it to appear
   (`dig _github-pages-challenge-<owner>.methodbudget.com +nostats +nocomments +nocmd TXT`),
   then **Verify**. Keep the TXT record afterwards; removing it un-verifies the domain.
   For an organization, the same lives under the organization's **Settings** > **Pages**.
2. **Repository Settings > Pages > Build and deployment > Source: GitHub Actions.**
3. **Repository Settings > Secrets and variables > Actions > Variables:** add
   `PAGES_ENABLED` with the value `true`.
4. Run the `pages` workflow (Actions > pages > Run workflow). The site appears at
   `https://ssandweiss.github.io/method-budget/`.
5. **Repository Settings > Pages > Custom domain:** `methodbudget.com`, **Save**. With
   the Actions source no CNAME file is committed or needed.
6. **DNS at the registrar**, for the apex domain and www:

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

   The CNAME target is the Pages host of the account that owns the repository
   (`<owner>.github.io`, without the repository name). If the repository moves to an
   organization, it becomes `<org>.github.io`. Remove any other A, AAAA or ALIAS records
   on `@` first (a registrar's parking page, for example), and never add a wildcard
   record (`*.methodbudget.com`): GitHub warns it invites takeovers even on a verified
   domain. If the domain has CAA records, one must allow `letsencrypt.org`, which issues
   the Pages certificate. With both the apex and `www` set up, GitHub redirects
   `www.methodbudget.com` to `methodbudget.com` on its own.
7. Check the records (`dig methodbudget.com +noall +answer -t A`, the same with
   `-t AAAA`, and `dig www.methodbudget.com +nostats +nocomments +nocmd`). DNS changes can
   take up to a day to spread.
8. **Enforce HTTPS** on the Pages settings page once GitHub has issued the certificate
   (the box is greyed out until then, up to a day). If it stays stuck, **Remove** the
   custom domain and **Save** it again to restart the certificate.

Nothing here has been turned on. Until step 3, a push that changes `site/` runs the
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
  a release note while a generated file (whatever `python3 scripts/site.py --files`
  lists: README.md, site/index.html, site/releases/index.html) is out of date, or
  regenerated but not staged.

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
