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
crates/method-app/ui + method-web ── cargo xtask web --demo ──> site/try/  (built, never committed)
                                                     │
                          site/ on main, copied by the suite website (method-production-suite/website)
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
so main never carries a site that is out of date (the suite website copies it from main;
below, "Publishing").

## The site: site/

Static files, no build step, no JavaScript, no trackers, no cookies. Every page carries
a Content-Security-Policy that allows only this site's own styles and images
(`default-src 'none'; style-src 'self'; img-src 'self'`), so no inline style, script or
third-party request can slip in unnoticed. System fonts only. The one exception is the
demo at `/try/` (below, "The demo"): the app itself, so it runs scripts, but only this
site's own files, under a policy of its own, and it is built rather than committed.

| Path | What it is |
|---|---|
| `index.html` | The home page: hero with the wordmark, an illustration of the app window drawn in HTML, Screenshots, What's new, Features, Why open files, Keyboard first, Command line and coding agents, Download, FAQ |
| `releases/index.html` | Every release note (generated block inside a hand-written page) |
| `404.html` | Served for any address with no page. Its links start at the site root, so it is right on methodbudget.com (in a local preview served from a subfolder its styles do not load) |
| `assets/tokens.css` | Brand colors and type, light and dark ([BRAND.md](BRAND.md)) |
| `assets/site.css` | Layout and components; names no colors of its own |
| `assets/brand/` | Mark and wordmark SVGs, light and dark (`scripts/make-brand.py`) |
| `img/` | The Field Station pictures, light and dark, retaken by `cargo xtask capture-screens` (Screenshots, below) |
| `assets/shots/` | Earlier screenshots of the app with invented budgets, WebP with PNG fallbacks (Screenshots, below) |
| `favicon.svg`, `favicon.ico`, `apple-touch-icon.png` | Browser and home-screen icons (`scripts/make-brand.py`) |
| `og.png`, `og-card.html`, `assets/og-card.css` | The 1200x630 share card and its source; `scripts/make-og.sh` renders it with headless Chrome (a throwaway profile, no window). Re-run after editing the card and commit all three |
| `try/` | The demo: the app running in the browser (below, "The demo"). Built by `cargo xtask web --demo --out site/try`, ignored by git, never committed |
| `robots.txt`, `sitemap.xml` | Crawlers: everything allowed; the sitemap lists `/`, `/releases/` and `/try/`. Add a line to the sitemap when a page is added |

Colors are tokens with a `prefers-color-scheme: dark` override, so the site follows the
visitor's system setting; every text pairing meets WCAG AA in both schemes (the table is
in BRAND.md). Layouts are mobile first and have no horizontal scroll at phone width; the
app illustration drops columns and its tools panel on narrow screens. Each page has
landmarks (header, nav, main, footer), a skip link and visible focus outlines.

### Screenshots

The README and the home page's Screenshots section open with four pictures of Field
Station, the invented production every app in the suite shows (SUITE-STANDARDS.md,
section 7), in the suite's order:

| Name | What it shows |
|---|---|
| `hero` | The topsheet, with the tools panel on the Cast category's fringes and the budget's locations |
| `explain` | Explain this number on one cast line's fringes, one of them stopping at its cutoff |
| `import` | File > Import Spreadsheet reviewing the camera department's sheet, every line checked against its total |
| `file` | The file you own: the window beside a terminal with the real output of `method-budget totals field-station.mthd` |

Each is taken light and dark and written twice: `assets/screens/<name>-light.png` and
`-dark.png` for the README, and the same files in `site/img/` for the site (the website
publishes `site/` alone, and a page never references a file outside it). Both use
`<picture>` with a `<source media="(prefers-color-scheme: dark)">` and descriptive alt text.
Every name and figure in them is invented.

They retake themselves, as a release step (RELEASING.md):

```bash
cargo xtask capture-screens                                  # retake all eight
cargo xtask review-screens --out target/screens/review.html  # old beside new
```

`capture-screens` (crates/xtask/src/screens.rs) builds the browser build into
`target/screens/web` (as `cargo xtask web` does) and the command line, runs the command
line's totals on the budget for the terminal picture, then runs
`scripts/capture-screens.mjs`. That serves the build on 127.0.0.1, opens the budget in
headless Chrome (over the DevTools pipe, `scripts/chrome-pipe.mjs`, no npm) in a 1280 by
772 page at 2x under a drawn 28-pixel title bar (a 1280 by 800 window), steps into each
state as a person would (clicks on the grid, the menu commands the menu bar sends), waits
for the page to reach it, and fails naming the step if it doesn't. A second pass draws the
Mac window around each capture (rounded corners, shadow, traffic lights, the title Method
Budget) on a transparent background. A picture is copied only when its bytes differ, so a
run that changes nothing changes no file; `review-screens` writes one self-contained page
with each changed picture old beside new and lists them. It needs what `cargo xtask web`
needs, Node 18 or later and Chrome (`CHROME=<path>` for another); it runs on a Mac and is
not run in CI.

**Capture mode is in the driver only.** Nothing of it ships in the desktop app or changes
the demo at `/try/`: before the page's own scripts run, the driver wraps the browser
build's `invoke` so that `host_info` answers as the desktop does (the page then lays itself
out as the Mac app: no File button, no browser-only notes), the few desktop-only commands
the page asks at start-up get the desktop's "nothing to show" answers, and Import
Spreadsheet is handed the invented sheet instead of a file picker. Any other command the
browser build can't answer fails the capture, naming it. Preferences a person would have
saved (the tools panel's width, its tabs in one pane for the Explain picture) are set in the
page's storage before it opens.

**Same pixels every run.** The driver hides what changes on its own (the text cursor's
blink, animations and transitions, scrollbars), rests the pointer on the footer, starts
each picture from a fresh page with fresh storage, and takes a capture only once two in a
row are the same. Fonts are the Mac's own. Two runs on one Mac give identical files. Honest
limits: it is Chrome drawing the page, not the Mac's own web view, and the frame is drawn,
not captured.

The budget is `fixtures/field-station/field-station.mthd`, an independent feature of
about a million dollars and the app's own sample (decisions.md, entry 84), made by
`python3 scripts/make-field-station.py` from Method Schedule's schedule days file for the
same production (fixtures/field-station/README.md); the sheet is `camera-department.csv`
beside it. The budget has original totals and actuals, so the capture hides the Orig, Var
and cost report columns in the page's saved column choices (as a person working on the
budget rather than the cost report would): the pictures show the budget's own totals.

The six earlier screenshots in the gallery (`site/assets/shots/`: topsheet, detail lines
with fringes, Explain this number, Budget Setup's fringes, Compare, and the cost report)
follow the Field Station set. They are captures of the real Mac app with the invented
budgets from `scripts/make-showcase-budgets.py`, taken by hand:

1. `cargo build --release -p method-cli`, then `python3 scripts/make-showcase-budgets.py`
   writes the invented budgets into `target/showcase/` (`--force` to replace them).
2. Build the app (`cd crates/method-app && cargo tauri build --bundles app`), open the
   budgets in it, set the window to 1440x900 points and capture each window at 2x without
   its shadow (`screencapture -x -o -l <window id> <name>.png`). Budget Setup is captured
   as its own window at 1160x460.
3. `scripts/make-screenshots.sh <folder of captures>` writes `site/assets/shots/<name>.webp`
   and a 256-color `<name>.png` fallback at half the capture's size (needs `cwebp` and
   `ffmpeg`).

Links inside the pages are relative (except 404.html), so the site works at
`https://methodbudget.com/`, in a local preview, and under the suite website's
`/<slug>/` local build. The canonical and share-card URLs name methodbudget.com.

To look at it locally:

```bash
cargo xtask web --demo --out site/try     # the demo, if you want /try/ too
cd site && python3 -m http.server 8000    # then open http://127.0.0.1:8000/
```

### The demo: try/

"Try it in your browser" (a button in the hero, a link in the nav and on the release
notes page) opens `/try/`: the real app's page (`crates/method-app/ui`) running against
the engine compiled to WebAssembly (docs/research/web-session.md). It starts with the
app's own sample budget, Field Station (the budget in the pictures,
`fixtures/field-station/field-station.mthd`, made by `scripts/make-field-station.py`;
invented, never a real budget; decisions.md, entry 84). Budgets live in the page:
Open reads a file the visitor picks, Save and every export download a file, and nothing
is sent anywhere. Reloading the page starts over.

`cargo xtask web --demo --out site/try` (crates/xtask/src/web.rs) builds it: the page,
the shim and the module (about 5.2 MB, 1.4 MB gzipped), the charts of accounts as
separate files in `charts/` (fetched one at a time, only when New From Template lists or
picks one), and, for the demo only:

- `demo/demo.js`, `demo/demo.css` (from `crates/method-web/demo/`): a slim note at the
  top of the app saying it runs entirely in the browser, nothing is uploaded, files
  opened stay on the computer and Save downloads a `.mthd` file, with a link to the Mac
  app's download and back home. It can be hidden; it is not shown in the Budget Setup
  frame. Under 760 pixels wide it adds that the app works best on a computer: on a
  phone the grid and the tools panel do not fit side by side, and the app wants a
  keyboard.
- `sample/Field Station.mthd`, copied from the fixtures, which demo.js opens (`?open=`)
  unless the address names another budget. The browser smoke test
  (`scripts/web-smoke.mjs`) opens the same file.
- A Content-Security-Policy on the page: `default-src 'self'; script-src 'self'
  'wasm-unsafe-eval'; style-src 'self'; img-src 'self' data: blob:; connect-src 'self';
  frame-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'` (and
  `font-src 'self'`). Everything is this site's own file; `'wasm-unsafe-eval'` is what
  lets the browser compile the module. No inline script or style, nothing from anyone
  else, no cookies, no analytics. The page keeps a few preferences (display settings, import mappings) in the
  browser's local storage, as the app does; no budget is kept there.

The note's words follow the site's rules: plain text, no em dashes, no other product
named. `site/try/` is in `.gitignore`: the built module, wasm-bindgen's JS and the copy
of the page never enter git. The demo needs the WebAssembly target and the wasm-bindgen
command line of Cargo.lock's version (docs/research/web-session.md, "Building").

## Publishing: the suite website

methodbudget.com is served by the suite website, the repository
`method-production-suite/website` (Cloudflare Pages project `method-website`), which
serves every Method app's site on its own domain. That repository copies this one's
`site/` from `main` (its `tools/sync.py` reads the pushed `origin/main`, never a local
working tree), adds the suite bar and band, and publishes. So:

- What is on `main` in `site/` is what goes live at the website's next sync. Keep it
  correct there: CI's `site` job runs `python3 scripts/site.py --check` and
  `python3 scripts/make-brand.py --check` on every push.
- The custom domain, its DNS and HTTPS are set on the Cloudflare Pages project, not
  here. There is no `CNAME` file and no Pages workflow in this repository (GitHub Pages
  was retired in October 2026).
- Only committed files are copied. The demo at `/try/` is built, not committed
  (`site/try/` is in `.gitignore`), so it goes live only if the suite website builds it
  with `cargo xtask web --demo`; CI still builds the page with `cargo xtask web` and
  smoke-tests it in headless Chrome on every push (`ci.yml`, "The page in a browser").

To change the site: edit `site/` here (or the feature file and release notes, then
`python3 scripts/site.py`), push to `main`, then sync and publish from the website
repository (its README has the commands).

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
