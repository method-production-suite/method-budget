# Handoff: Film Wizard Budgeting

Read this whole file first, then `README.md` and `CLEAN-ROOM.md`. Everything you need is here;
don't wait on follow-up questions for anything this file already answers.

## What this is
An open-source, clean-room film and TV budgeting app in Rust: a free alternative to Movie Magic
Budgeting (Entertainment Partners). A Darkwell project owned by Sam Sandweiss. The model is ArtCraft
(github.com/storytold), who ship clean-room Rust rewrites of Photoshop, Premiere, etc.

- Local: `~/Documents/CODING_PROJECTS/Projects/02_Darkwell/film-wizard-budgeting`
- Repo: https://github.com/cwpsandywhois/film-wizard-budgeting (**private** until Sam says go public)
- GitHub: this Mac has several `gh` accounts. Pushes need `cwpsandywhois` active
  (`gh auth switch --user cwpsandywhois`). A push that 404s means the wrong account is active.
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

## Current state
Scaffold only: Cargo workspace, `fwb-core` stub (just `Cents` + an empty test), README,
CLEAN-ROOM.md, licenses, CI (`cargo test --workspace --locked`). Zero dependencies.
CI is green as of 2026-10-08 (`gh run list` to recheck).

## Next: the engine data model
Build in `fwb-core`, test-first, no UI yet:
- **Hierarchy**: Budget > Topsheet categories (Above-the-line, Production, Post, Other) >
  Accounts (e.g. 1100 Writing) > Details (line items).
- **Detail line**: description, amount, units (Days/Weeks/Flat/Allow/etc.), multiplier ("X"),
  rate, currency, fringes applied, groups, location/set tags. Total = amount x X x rate.
- **Fringes**: percentage or flat-per-unit, with optional cutoffs (cap a fringe at a wage
  ceiling). Applied per detail, rolled up separately so the topsheet shows fringe totals.
- **Globals**: named variables (e.g. `SHOOT_DAYS = 30`) usable in amount/rate fields; changing
  one recalculates everything. Needs a tiny expression evaluator with no `eval`.
- **Contractual charges**: percent-of-subtotal lines (contingency, insurance, fee), with
  per-account exclusions.
- **Groups**: tag details to include/exclude in a recalculated total (e.g. "Option A").
- **Currencies**: per-detail currency with a budget-level rate table; totals in base currency.
- **Recalc**: deterministic, pure. Same input gives byte-identical output. Property-test the
  rollups (sum of details == account total == topsheet line).

Then: the native format (serde, probably TOML or JSON with stable ordering; pick one and
document it in `docs/FORMAT.md`), then Excel/CSV import, then a UI (Tauri or web/WASM; decide
when you get there and write the reasoning in `docs/decisions.md`).

## Working style for Sam
- Casual and short. **No em dashes** in anything you write for him.
- Keep moving on long builds: parallelize, batch updates, only stop him for real decisions.
- Flag anything that touches the legal line (file formats, bundled data) before doing it.
