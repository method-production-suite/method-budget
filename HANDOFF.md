# Handoff: Film Wizard Budgeting

Read this whole file first, then `README.md` and `CLEAN-ROOM.md`. Everything you need is here;
don't wait on follow-up questions for anything this file already answers.

## What this is
An open-source, clean-room film and TV budgeting app in Rust: a free alternative to Movie Magic
Budgeting (Entertainment Partners). A Darkwell project owned by Sam Sandweiss. The model is ArtCraft
(github.com/storytold), who ship clean-room Rust rewrites of Photoshop, Premiere, etc.

- Local: `~/Documents/CODING_PROJECTS/Projects/02_Darkwell/film-wizard-budgeting`
- Repo: https://github.com/ssandweiss/film-wizard-budgeting (**private** until Sam says go public).
  The older cwpsandywhois/film-wizard-budgeting copy is retired.
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
- Never decompile/disassemble/debug another vendor's binaries, or copy their code, templates,
  help text, icons, or bundled data (rate books, fringe libraries).
- Allowed: how film budgets work as a domain, public union/guild rate info, normal UI use of
  other tools, their open-format exports.
- Money is integer cents (`Cents = i64`). No floats for money, ever. Rates/percentages need an
  explicit decimal type and a documented rounding rule.
- Born-secure: commit lockfiles, no secrets in git, CI stays `permissions: contents: read`,
  SHA-pin any third-party Action. Machine-wide dependency cooldowns exist; don't bypass them.
- Stage explicit paths, never `git add -A` blindly.

## Current state (2026-10-08)
The engine (`crates/fwb-core`, zero dependencies) covers the v1 calc model:
- Budget > topsheet (categories + production totals) > accounts > detail rows (line,
  subtotal, adjustment), with stable `RowId`s.
- Formulas with globals (own parser, cycle detection), 4-factor lines, per-line currency.
- Fringes: percent with cutoffs per earner (line or fringe range, split back by largest
  remainder), flat with unit-hours conversion, four posting modes.
- Groups with in-budget-total suppression, full/active totals; sets, locations,
  selection totals.
- ATL/BTL levels, contractual charges (percent and flat, exclusions), credits (three cap
  types, overall cap, discount, unrounded row sums), grand and net totals.
- Tests: unit tests per stage, hand-checked scenarios (`tests/recalc.rs`), seeded property
  tests (`tests/properties.rs`). Design choices: `docs/decisions.md`.

Research (clean-room, own words): `docs/research/mmb-feature-inventory.md` from EP's public
manual. Sam's MMB screenshots live in `Screenshots/` and notes on them in
`docs/research/mmb-screens.md`; **both are gitignored and must stay that way** (vendor UI
plus Sam's real production budget figures). Never use those figures in tests.

## Next
1. Still-v1 engine gaps: account coding rules (segment widths, separator), budget info
   fields, display currency, original/variance baseline, delete-global-to-value.
2. Native format: serde, TOML or JSON with stable ordering; document in `docs/FORMAT.md`.
3. Import from MMB exports (tab, XML, JSON, Excel). Column layouts aren't public: plan is
   to design a test budget, have Sam enter and export it from his licensed copy, and
   build against those files. Currency direction must be confirmed from them.
4. UI (Tauri or web/WASM; decide then and record in `docs/decisions.md`).

## Working style for Sam
- Casual and short. **No em dashes** in anything you write for him.
- Keep moving on long builds: parallelize, batch updates, only stop him for real decisions.
- Flag anything that touches the legal line (file formats, bundled data) before doing it.
