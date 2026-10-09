# Decisions

Short records of choices that shape the code. Newest last.

## 1. Money and decimals (2026-10-08)

- **Money is `Cents = i64`.** Never floats.
- **Everything else numeric is `Dec`**: fixed-point, 9 decimal places, stored in an `i128`.
  Quantities, rates, percentages, exchange rates and globals are all `Dec`.
- **Rounding rule: half away from zero**, everywhere (0.005 becomes 0.01, -0.005 becomes -0.01).
  It's the rule people expect from payroll and spreadsheets, and it's symmetric for credits.
- `Dec` multiply and divide round to 9 places. That can matter for repeating fractions:
  `35 / 3` is stored as `11.666666667`.

## 2. Where rounding happens (2026-10-08)

Money is rounded to cents **once per detail line**, never in the rollups:

1. `quantity = amount x X x 4X` (Dec)
2. `local total = round(quantity x rate)` in the line's currency
3. `base total = round(local total x exchange rate)` when the currency is foreign
4. each fringe = `round(earner base x percent / 100)` or `round(rate x units)`, once per earner or line

Accounts, categories, the subtotal, fringe totals and the grand total are integer sums of
those cents, so they always agree exactly. Contractual charges round once each, on their base.

## 3. Fringe cutoffs are per earner (2026-10-08, revised)

A percent fringe's cutoff applies per **earner**: a single line, or a **fringe range** (a run
of consecutive lines sharing `fringe_range`, e.g. one crew member's prep, shoot and wrap).
The range's fringe is computed once and split back over its lines pro rata, by largest
remainder, so the pieces sum exactly. Subtotal rows and left-out rows don't break a range
(so excluding a line still equals deleting it). There is no cross-account "person"; that matches how
budgeting tools work in practice (see docs/research/mmb-feature-inventory.md).

## 4. Groups (2026-10-08, revised)

Groups are budget data with an "in budget total" switch: turning it off suppresses every
tagged line. Recalc options add what-if filters on top (`exclude_groups`, `only_groups`).
A left-out line still evaluates (so bad formulas are still reported) but counts as 0, and a
left-out adjustment row doesn't reset its window. Property-tested: excluding a group gives
the same totals as deleting its lines, and suppressing equals excluding. Each group reports
a full total (as if nothing were left out) and an active total.

## 5. Globals and formulas (2026-10-08)

Amount, multiplier and rate are formula strings: numbers, global names, `+ - * /`, unary
minus, parentheses. Hand-written parser plus AST walker, no `eval`. Globals can reference
each other; they're evaluated in dependency order and cycles are reported with their path.

## 6. No dependencies in the engine yet (2026-10-08)

Property tests use a small seeded generator in `tests/properties.rs` instead of `proptest`.
It keeps `fwb-core` dependency-free until we actually need something (serde, for the format).

## 7. Blank fields and row types (2026-10-08)

Blank Amt and Rate count as 0, blank X and 4X as 1, so a title line is just a line with no
numbers. Subtotal rows show a running total and add nothing. Adjustment rows are real costs
whose rate is locked to the lines above them (back to the previous adjustment); Amt sets
the percentage, e.g. `0.1` for 10%.

## 8. Units (2026-10-08)

Units are a per-budget table with an optional hours equivalence. Our defaults are our own
(Hour 1, Day 10, Week 50, Month 200, plus Allow and Flat) and fully editable. Flat fringes
convert between time units through hours, using the line's primary unit only.

## 9. Topsheet, charges and credits (2026-10-08)

- Production-total rows split the topsheet into levels; the first level is above-the-line.
  With no production totals, everything is below-the-line.
- Fringe posting is a budget setting: by budget, production level, category (fringe account
  numbered category code + suffix, default `99`) or account. Every fringe dollar posts
  exactly once (property-tested).
- Charges apply to ATL + BTL in order and never to each other. Exclusions count each line
  once even if it matches several rules.
- Credit rows are summed **unrounded**, then capped and discounted, then rounded once.

## 10. Currency direction (2026-10-08)

A currency's `rate` is base-currency units per one unit of that currency. The reference
manual is inconsistent here, so importers must confirm direction against exported totals.

## 11. Account coding (2026-10-08)

Each budget has a coding pattern: widths for category, account, sub-account and set
segments (0 to 6, used segments contiguous) and one separator. Default is `11-01` style
(2 + 2, hyphen). Typed numbers are parsed by whichever separator was typed, or by widths
when none was; short segments are zero-padded (`11-5` becomes `11-05`) so the same account
always normalizes to the same text, which matters because account numbers are the join key
for comparison and import. Category fringe accounts are built through the pattern
(category `11` + suffix `99` = `11-99`).

## 12. Baseline, display currency, global deletion (2026-10-08)

- **Baseline** ("original" vs current) is a frozen copy of row, account, category, grand and
  net totals stored in the budget. Variance is computed on demand; rows match by id, so
  edited, deleted and added lines all show up correctly.
- **Totals currency** is display only: the report carries the rate and converts on request,
  rounding once. The math always runs in the base currency.
- **Deleting a global** first freezes its current value into every formula that used it, so
  no total changes (property-tested).
- **Budget info** fields (text, number, date) are plain data for now; they don't feed formulas.

## 13. Native file format (2026-10-08)

JSON, not TOML: budgets are deeply nested (topsheet > categories > accounts > rows), which
TOML handles poorly. Pretty-printed with fixed key order so files diff cleanly in git. It
lives in its own crate (`fwb-format`) with separate file-side types, so the engine stays
dependency-free and the file layout can stay stable while the model evolves. Money is
integer cents, decimals are strings, never JSON floats. Extension `.fwb`. Spec:
`docs/FORMAT.md`. First dependencies: serde and serde_json, pinned to releases well past
the 14-day cooldown (checked by publish date; zmij is dtolnay's float formatter, pulled in
by serde_json).

## 14. UI approach: Tauri desktop app (2026-10-08, decided by Sam)

Film Wizard Budgeting is a **local, open-source desktop app**, not a web app. Budgets live as
`.fwb` files on the user's disk; no server, no account, works offline.

Built with **Tauri**: the Rust engine runs natively in the app process, and the UI is drawn
with web tech inside a native window (no browser, nothing hosted). Chosen over a pure-Rust
GUI (Slint, iced, egui) because spreadsheet-grade grid editing is the heart of budgeting and
the web stack's grid tooling is far more mature. Small installers for macOS, Windows and
Linux. The JS toolchain this adds is held to the same dependency cooldown and lockfile rules.

## 15. Row shortcuts follow budgeting habits (2026-10-09, Sam)

Cmd/Ctrl+I inserts a line, Cmd/Ctrl+D deletes rows (undoable), Cmd/Ctrl+= inserts a
subtotal: the keys people switching from other budgeting tools already have in their hands.
Fill down moves to Shift+Cmd/Ctrl+D rather than the spreadsheet Cmd+D. Matching common
shortcuts is about familiarity, not copying; labels and menus stay our own.

## 16. Row style, notes and separators are display data (2026-10-09)

Groups get a `color`; detail rows get `bold`, `italic`, `indent` (0 to 4), `color` (wins
over the group's) and a plain-text `note` (up to 10,000 characters). A new row kind,
`separator`, is a drawn rule line. None of it touches the math: separators are never
counted or fringed, don't break fringe ranges and don't reset subtotal or adjustment
windows, so removing every separator and all styling gives an identical report
(property-tested). All of it is additive in format version 1 and left out at its default,
so existing files and the golden file are unchanged. Colors are checked (`#` plus 6 hex
digits, stored lowercase) and limits enforced by the app's commands, not the file reader.

## 17. Versions and multi-budget compare (2026-10-09)

- **Versions live inside the budget.** `Budget::versions` holds named full copies of the
  content (`BudgetSnapshot`: everything but the version list, so versions never nest), with
  an id, a name, a caller-supplied ISO 8601 `created` time (the engine never reads the
  clock) and an optional note. A version is a whole copy, so N versions make the file
  roughly N + 1 times as big. Simple and robust for v1; deltas can come later if files
  get large. The baseline (decision 12) is unchanged and is part of each version's content.
- **Restore is reversible**: the content being replaced is saved first as an automatic
  version (`Before restoring "<name>"`), then the version's content replaces the current
  one. The version list is kept. Ids are one past the highest in use. Names and notes
  are stored as given; any limits are the app's commands' job, like other display data.
- **Row ids stay meaningful across versions.** `Budget::highest_row_id` covers current
  rows, the baseline and every version, so new rows never reuse an id a version still has.
- **Compare matches by account code** through each budget's own coding: codes are parsed
  into segments and compared segment by segment, so `1100`, `11-00` and `11.00` (under a
  `.` pattern) all match. A code that doesn't fit falls back to its trimmed text. Accounts
  match within their category; category fringe accounts are listed and matched only with
  each other. 2 to 16 budgets (a base and up to 15 others). Order is the first budget's,
  then anything new in the order later budgets show it. Differences are against the first
  budget, a missing value counting as 0; the percent is against the base's size (sign
  follows the amount) and is `None` when the base is 0 or missing.
- **Detail compare** is per account: rows match by id (versions of one budget) or by
  trimmed, case-insensitive description with the n-th duplicate matching the n-th. Only
  costed rows. Property-tested: comparing a version with the current budget gives exactly
  the baseline variance for every category, account and row.

## 18. Edits stay instant on big budgets (2026-10-09)

Real budgets have thousands of lines (2,000+ across ~60 accounts is common). Every command
used to send the whole budget and every row's totals (2 MB at 5,000 rows) and the UI redrew
everything. Target: a one-cell edit in a 5,000-row budget under ~30 ms from key to screen,
bulk edits under ~100 ms. Measured with `examples/perf.rs` on `fwb_core::sample` budgets.

- **A lean view, not a diff.** Every command returns the budget's outline (no detail rows),
  totals down to the account, and the one account on screen in full; `focus_account`
  says which, `get_full_view` gives everything for printing. Each reply stands alone, so
  there is no client copy to keep in sync and nothing to go stale after an undo or an
  error. Size follows the account on screen (about 35 KB plus 0.5 KB a row), not the
  budget. Problems elsewhere come as a count and a list of accounts; variance rows only
  for the account on screen, with a count of changed rows. Versions are listed, not sent.
- **Same numbers, less work.** Percent fringes visit only the fringes an earner's lines
  carry (it visited every fringe in the account for every earner); the unfiltered pass
  for "full" group totals skips accounts where every row counts (it re-ran every
  account); plain numbers and bare globals skip the parser. A per-account recalc cache
  (`RecalcCache`) re-evaluates only accounts whose rows changed, keyed by the tables rows
  depend on. Results are bit-identical: the old engine is kept as a test reference and
  compared on generated budgets with random edits, filters and every posting mode.
- **Undo keeps what changed**, not whole budgets: the changed rows (or an account's rows
  when rows were added or removed), the rest only when it changed, the version list only
  when it changed, the whole budget only for structural edits. 200 steps of one-cell edits
  now hold 200 rows, not 200 budgets.
- **The grid patches and windows.** Unchanged rows keep their elements; grids over 150
  rows draw only the rows near the viewport. Cursor scrolling waits for the next frame so
  a redraw costs one layout.
- **Numbers** (release build, this Mac, 5,000 rows, a 416-line account on screen): one-cell
  edit in Rust 17.9 ms and 2,000 KB of JSON before, 5.0 ms and 205 KB after; 500-cell paste
  19.0 ms before, 9.8 ms after; drawing the reply after a one-cell edit in a UI harness
  (Chromium, fake bridge with the canned replies) 227 ms before, 13 ms after; opening the
  big account 192 ms before, 24 ms after.
