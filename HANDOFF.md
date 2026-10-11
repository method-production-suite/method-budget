# Handoff: Method Budget

Read this whole file first, then `README.md` and `CLEAN-ROOM.md`. Everything you need is here;
don't wait on follow-up questions for anything this file already answers.

## What this is
An open-source, clean-room film and TV budgeting app in Rust: a free alternative to Movie Magic
Budgeting (Entertainment Partners). A Darkwell project owned by Sam Sandweiss. The model is ArtCraft
(github.com/storytold), who ship clean-room Rust rewrites of Photoshop, Premiere, etc.

- Local: `~/Documents/CODING_PROJECTS/Projects/02_Darkwell/film-wizard-budgeting`
- Repo: https://github.com/ssandweiss/method-budget (**private** until Sam says go public).
  The local folder keeps its old name for now. The older cwpsandywhois copy is retired.
- GitHub: this Mac has several `gh` accounts. Pushes need `ssandweiss` active
  (`gh auth switch --user ssandweiss`). A push that 404s means the wrong account is active.
- License: **MIT OR Apache-2.0** (same terms as ArtCraft). Decided, don't revisit.

## Decided scope (v1)
1. **Engine** (`crates/fwb-core`): the budgeting math. Clean-room.
2. **Import from exports**: read budgets other tools *export* (Excel/CSV). No proprietary parsing.
3. **Our own open native format**: documented in-repo, diff-friendly (text, stable key order).
   Native `.mbd` (Movie Magic) support is **out**: it would need reverse engineering a
   proprietary format, which needs a lawyer first. Don't build it, don't research its layout.

## Hard rules (CLEAN-ROOM.md is the source of truth)
- Never decompile/disassemble/debug another vendor's binaries, or copy their code, help text,
  icons, or bundled data (rate books, fringe libraries).
- Allowed: how film budgets work as a domain, public union/guild rate info, normal UI use of
  other tools, their open-format exports, and studio/network charts of accounts (studios share
  them with production partners; Sam's call, 2026-10-10).
- Money is integer cents (`Cents = i64`). No floats for money, ever. Rates/percentages need an
  explicit decimal type and a documented rounding rule.
- Born-secure: commit lockfiles, no secrets in git, CI stays `permissions: contents: read`,
  SHA-pin any third-party Action. Machine-wide dependency cooldowns exist; don't bypass them.
- Stage explicit paths, never `git add -A` blindly.

## Current state (2026-10-09)
A working local desktop app (Tauri v2, `crates/fwb-app`, zero-npm UI) on a tested engine.
The product is **Method Budget** (methodbudget.com; decisions.md, entry 46). Crate and
module names (`fwb-core` and so on) are internal and unchanged for now. Never shorten the
name to "MB" (that is how people shorten Movie Magic Budgeting).
Read docs/ROADMAP.md first (phases, decisions, engine checks), then docs/APP.md (how the app
is built: lean views, multi-document tabs, security model, saving and recovery, printing),
docs/decisions.md (every product and design call, numbered), docs/FORMAT.md (`.mthd` spec).

- **Engine** (`fwb-core`, zero deps): topsheet levels, fringes (cutoffs per earner, ranges,
  unit-hour flats, four posting modes), groups, charges, credits, currencies, globals,
  account coding, baseline/variance, versions, multi-budget compare, report presets,
  incremental recalc. Checked against vendor tutorial figures to the cent.
- **Format** (`fwb-format`): `.mthd` JSON (older `.fwb` files still open), documented,
  version 1, additive changes only.
- **CLI** (`fwb-cli`, binary `method`; `fwb` is a deprecated alias): show, check, fmt, new,
  sample (5,000-line test budget), and more (docs/CLI.md).
- **App**: tabs, grid with keyboard flow, autocomplete and add-on-the-fly, right-click menus,
  formatting, notes, tools panel (two tab panes, dock right/bottom/pop-out window), apply to
  selection, row copy/paste with tags across budgets, find and replace, versions and compare,
  print setup with presets and PDF, autosave into the file with recovery and backups,
  outside-change guard, Movie Magic-style shortcuts. ~5 ms per edit at 5,000 lines.
- **Research**: docs/research/behaviors/01-24 (Sam's recordings 01-02 outrank the 2020/2021
  vendor tutorials 03-24), feature inventory, export test plan.

## Studio chart templates (2026-10-10, uncommitted)
48 studio/network charts of accounts are now bundled templates, next to our own 7 (55 in
`templates/charts/charts.json`; sources listed in `templates/charts/SOURCES.md`). Their CSVs are in
`imports/coa-sources/` (gitignored, follows `EXPORT-SPEC.md`). 36 more are parked in
`imports/coa-sources/needs-review/` because `method coa import` mangles or rejects them.
**Read `imports/coa-sources/IMPORT-NOTES.md` before touching the importer or those files.** In short:
- The importer ignores the CSV's `parent` column and guesses category vs account from code width,
  so mixed-width charts lose most accounts (HBO NY keeps 285 of 616), merge categories (DISNEY
  STAGE 75 to 8) or fail (NETFLIX). It also shortens category codes (`3100` saved as `31`).
  Fix: place accounts by `parent` when present, keep codes verbatim.
- 22 charts have categories with no accounts; the importer allows them but
  `every_bundled_template_opens_and_recalculates` doesn't. Product call: relax the test,
  add placeholders, or drop them.
- Repeated account codes: the importer keeps the first and drops the rest silently. In the built
  Academic charts that loses real accounts ("Stand-ins").
After an importer fix, move files from `needs-review/` up a level and run
`python3 scripts/build-coa-templates.py`, then `cargo test -p fwb-format`.

## Next
Phase 1 remaining (ROADMAP): import from MMB exports (1.16, blocked until Sam runs
docs/research/export-test-plan.md; he tabled it for now), engine fixes from that test
(1.17), signed Mac build (1.21, needs Sam's call on the signing team). Then Phase 2.
Also: the importer fixes in "Studio chart templates" above.

## How to work here
- Never launch the app or drive the GUI while Sam is at the computer unless he says so;
  verify with tests, `node --check` and a browser-pane harness with a fake `__TAURI__`.
  Launching only to take a window screenshot (no keystrokes) has been OK with Sam.
- Parallel agents: each owns new files; shared files get small separated edits after a
  re-read. Report progress to the savvy-flow panel (`mcp__savvy-progress__*`).
- Every locked crate must be at least 14 days old (`scripts/check-crate-ages.py`); Rust is
  pinned to 1.96.1 (`rust-toolchain.toml`) so CI and local agree.
- GitHub: push as `ssandweiss` (repo-local credential helper is set; don't flip the global
  gh account). Use `GH_TOKEN=$(gh auth token --user ssandweiss) gh ... -R
  ssandweiss/method-budget` for gh commands.
- Clean room: never open Sam's `mmb-decomp` project from a Method Budget session (see
  CLEAN-ROOM.md and ROADMAP "Optional, outside this repo").

## Working style for Sam
- Casual and short. **No em dashes** in anything you write for him.
- Keep moving on long builds: parallelize, batch updates, only stop him for real decisions.
- Flag anything that touches the legal line (file formats, bundled data) before doing it.
