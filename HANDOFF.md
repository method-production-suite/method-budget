# Handoff: Method Budget

Read this whole file first, then `README.md` and `CLEAN-ROOM.md`. Everything you need is here;
don't wait on follow-up questions for anything this file already answers.

## What this is
An open-source, clean-room film and TV budgeting app in Rust: a free alternative to Movie Magic
Budgeting (Entertainment Partners). "The owner" below is the person who makes the product calls.
The model is ArtCraft (github.com/storytold), who ship clean-room Rust rewrites of Photoshop,
Premiere, etc.

- Local: the owner's working copy on this Mac (its folder name is due to change; don't hard-code it).
- Repo: https://github.com/ssandweiss/method-budget (**private** until the owner says go public).
  An older copy under another account is retired.
- GitHub: this Mac has several `gh` accounts. Pushes need `ssandweiss` active
  (`gh auth switch --user ssandweiss`). A push that 404s means the wrong account is active.
- License: **MIT OR Apache-2.0** (same terms as ArtCraft). Decided, don't revisit.

## Decided scope (v1)
1. **Engine** (`crates/method-core`): the budgeting math. Clean-room.
2. **Import from exports**: read budgets other tools *export* (Excel/CSV). No proprietary parsing.
3. **Our own open native format**: documented in-repo, diff-friendly (text, stable key order).
   Native `.mbd` (Movie Magic) support is **out**: it would need reverse engineering a
   proprietary format, which needs a lawyer first. Don't build it, don't research its layout.

## Hard rules (CLEAN-ROOM.md is the source of truth)
- Never decompile/disassemble/debug another vendor's binaries, or copy their code, help text,
  icons, or bundled data (rate books, fringe libraries).
- Allowed: how film budgets work as a domain, public union/guild rate info, normal UI use of
  other tools, their open-format exports, and studio/network charts of accounts (studios share
  them with production partners; the owner's call, 2026-10-10).
- Money is integer cents (`Cents = i64`). No floats for money, ever. Rates/percentages need an
  explicit decimal type and a documented rounding rule.
- Born-secure: commit lockfiles, no secrets in git, CI stays `permissions: contents: read`,
  SHA-pin any third-party Action. Machine-wide dependency cooldowns exist; don't bypass them.
- Stage explicit paths, never `git add -A` blindly.

## Current state (2026-10-11)
A working local desktop app (Tauri v2, `crates/method-app`, zero-npm UI) on a tested engine.
The product is **Method Budget** (methodbudget.com; decisions.md, entry 46). The crates are
`method-core`, `method-format`, `method-cli` and `method-app`. Never shorten the name to "MB"
(that is how people shorten Movie Magic Budgeting).
Read docs/ROADMAP.md first (phases, decisions, engine checks), then docs/APP.md (how the app
is built: lean views, multi-document tabs, security model, saving and recovery, printing),
docs/decisions.md (every product and design call, numbered), docs/FORMAT.md (`.mthd` spec).

- **Engine** (`method-core`, zero deps): topsheet levels, fringes (cutoffs per earner, ranges,
  unit-hour flats, four posting modes), groups, charges, credits, currencies, globals,
  account coding, baseline/variance, versions, multi-budget compare, report presets,
  incremental recalc. Checked against vendor tutorial figures to the cent.
- **Format** (`method-format`): `.mthd` JSON, documented,
  version 1, additive changes only.
- **CLI** (`method-cli`, binary `method`): show, check, fmt, new,
  sample (5,000-line test budget), and more (docs/CLI.md).
- **App**: tabs, grid with keyboard flow, autocomplete and add-on-the-fly, right-click menus,
  formatting, notes, tools panel (two tab panes, dock right/bottom/pop-out window), apply to
  selection, row copy/paste with tags across budgets, find and replace, versions and compare,
  print setup with presets and PDF, autosave into the file with recovery and backups,
  outside-change guard, Movie Magic-style shortcuts. ~5 ms per edit at 5,000 lines.
- **Research**: docs/research/behaviors/01-24 (the 2026 recordings 01-02 outrank the 2020/2021
  vendor tutorials 03-24), feature inventory, interop formats, the overnight parity test plan,
  and docs/research/parity-tracker.md (every MMB feature: status, evidence, what's left).
- **Also built**: diff and three-way merge with a git driver, Explain This Number, overtime
  helper, actuals and cost report, setup packs and library, chart of accounts import with 91
  bundled templates, the `method mcp` server for coding agents, updater and release tooling,
  website and brand. Flat fringes count the way Movie Magic does (decision 54, verified in the
  real app): Amount only by default, with `amount_counts` and `quantity` as options and an
  optional max hours per day.
- **Master rule**: match Movie Magic Budgeting as closely as possible. When unsure, check it in
  the real app (an agent may drive MMB 10 on this Mac with the owner's permission, using
  throwaway "ZTEST" budgets only and full-screen control for the grid).
- **Rate data** lives in a separate private repo, methodbudget/method-budget-data (local
  folder next to this one), bundled into releases as setup packs later.

## Studio chart templates (2026-10-10)
All 84 studio/network charts in `imports/coa-sources/` (gitignored) are bundled templates
now, next to our own 7 (91 in `templates/charts/charts.json`); `needs-review/` is empty.
The importer fix is done (decision 53, docs/TEMPLATES.md): a `kind,code,name,parent` sheet
keeps every number as written and places accounts by `parent`; empty categories are
allowed; an account named `.` is dropped; a repeated number with the same name is merged,
with another name it comes in as `10950-2` (listed in the import). Matching by number never
cuts a number short (`AccountCoding::read`/`key`). The build script checks each template's
counts against its CSV. Groups for the 36 new charts follow their names (unclear ones are
Other). `imports/coa-sources/IMPORT-NOTES.md` has the per-chart detail.

## Next
1. **First, after the project folder rename** (film-wizard-budgeting to method-budget): the
   app's saved library path in `~/Library/Application Support/com.methodbudget.desktop/
   library.json` still points into the old folder; repoint it to
   `<new folder>/Test Library/My Method Budget Library.mthd` (and `recent.json`).
2. **Overnight parity run**: execute docs/research/parity-test-plan.md against the real MMB.
   The owner still has to answer its section 7 questions; proposed defaults: allow the
   decimals-on-totals setting for the night and restore it, stop at 06:30, MMB files in
   `~/ZTEST-mmb-parity/<date>/` and trashed after comparison, start comparing if P0 finishes
   early, commit the generator under `scripts/parity/`, skip cloud-only compare cases.
3. **Known issue**: splitting a line with the overtime helper copies its flat fringes onto
   every part, so a per-day fringe is charged once per part. Fix so only one part carries it.
4. Flight Deck (a line producer's separate app) exports `.mthd` and will write
   `basis: "amount_counts"` and `max_hours_per_day: "12"`; keep the format stable for it.
5. Remaining roadmap: import from MMB's open exports (1.16/1.17, now unblocked: drive MMB to
   make sample exports), episode-level budgeting (the owner confirmed it's wanted), the
   import/export waves in docs/research/interop-formats.md, Phase 3 wave B. Flip the
   "Signed and notarized Mac app" feature to have when a signed release ships.
6. Sister project: `../method-scheduling` (kickoff in its docs/KICKOFF.md).

## How to work here
- Never launch the app or drive the GUI while the owner is at the computer unless they say so;
  verify with tests, `node --check` and a browser-pane harness with a fake `__TAURI__`.
  Launching only to take a window screenshot (no keystrokes) has been OK with the owner.
- Parallel agents: each owns new files; shared files get small separated edits after a
  re-read. Report progress to the savvy-flow panel (`mcp__savvy-progress__*`).
- Every locked crate must be at least 14 days old (`scripts/check-crate-ages.py`); Rust is
  pinned to 1.96.1 (`rust-toolchain.toml`) so CI and local agree.
- GitHub: push as `ssandweiss` (repo-local credential helper is set; don't flip the global
  gh account). Use `GH_TOKEN=$(gh auth token --user ssandweiss) gh ... -R
  ssandweiss/method-budget` for gh commands.
- Clean room: never open the owner's `mmb-decomp` project from a Method Budget session (see
  CLEAN-ROOM.md and ROADMAP "Optional, outside this repo").

## Working style for the owner
- Casual and short. **No em dashes** in anything you write for them.
- Keep moving on long builds: parallelize, batch updates, only stop them for real decisions.
- Flag anything that touches the legal line (file formats, bundled data) before doing it.
