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

**Amended 2026-10-10 (entry 55):** this is now the `exact` arithmetic, a per-budget
setting. By default a budget works its formulas and money out in binary doubles, rounded to
cents half up on the double, as the reference product does. Money is still `Cents` either
way.

## 2. Where rounding happens (2026-10-08)

Money is rounded to cents **once per detail line**, never in the rollups:

1. `quantity = amount x X x 4X` (Dec)
2. `local total = round(quantity x rate)` in the line's currency
3. `base total = round(local total x exchange rate)` when the currency is foreign
4. each fringe = `round(earner base x percent / 100)` or `round(rate x units)`, once per earner or line

Accounts, categories, the subtotal, fringe totals and the grand total are integer sums of
those cents, so they always agree exactly. Contractual charges round once each, on their base.

**Amended 2026-10-10: money rounds once from the exact product.** Before, `quantity x rate`
went through a `Dec` multiply, which rounds to 9 places, and that result was rounded again
to cents: `0.5 x 0.009999999` is `0.0049999995`, became `0.005`, then `0.01`. Now each money
step keeps the exact product (`decimal::Exact`: a wide integer over a power of ten, no
rounding while multiplying, overflow an error rather than a panic) and rounds to cents once,
half away from zero, so that case is `0.00`. This covers the line total (`quantity x rate`),
flat fringes per unit (`rate x quantity x line hours / fringe hours`, the division folded
into the one rounding), credits (each row's `percent of net` with its caps, the rows' sum,
the overall cap and the `(100 - discount) / 100` share, rounded once at the end) and the
totals-currency conversion (`base total / totals rate`). The quantity itself (`amount x X x
4X`) is still a `Dec` at 9 places: it is a shown figure, like a formula's value, and flat
fringes and adjustment rows use the same quantity the line shows. The credit row figures in
the report (`credit`, `cap_variance`, `capped`) are the exact values rounded to 9 places for
showing; the amount is rounded from the exact values, not from those. Explain shows the
exact product in full (`0.5 x 0.009999999 = 0.0049999995`, rounding to `0.00`); for a flat
fringe per unit, whose division need not end, it shows the quotient to 9 places. The
hand-checked tutorial figures are unchanged. (What a flat fringe counts changed in entry
54: the Amount, not the quantity, and the count is an exact fraction rounded once with the
fringe, the same rule.)

**Amended 2026-10-10 (entry 55):** these are the `exact` arithmetic's steps. The default,
Movie Magic arithmetic, rounds once at the same places, but from doubles, half up on the
double, and converts a foreign line before rounding it (so step 3 rounds once, not twice).

## 3. Fringe cutoffs are per earner (2026-10-08, revised)

A percent fringe's cutoff applies per **earner**: a single line, or a **fringe range** (a run
of consecutive lines sharing `fringe_range`, e.g. one crew member's prep, shoot and wrap).
The range's fringe is computed once and split back over its lines pro rata, by largest
remainder, so the pieces sum exactly. Subtotal rows and left-out rows don't break a range
(so excluding a line still equals deleting it). There is no cross-account "person"; that matches how
budgeting tools work in practice (see docs/research/mmb-feature-inventory.md).

**Amended 2026-10-10 (entry 56):** the range's cutoff is now used up in line order, each
line's fringe rounded on its own, as the reference product works it; nothing is split pro
rata. Adjustment rows can be earners too.

## 4. Groups (2026-10-08, revised)

Groups are budget data with an "in budget total" switch: turning it off suppresses every
tagged line. Recalc options add what-if filters on top (`exclude_groups`, `only_groups`).
A left-out line still evaluates (so bad formulas are still reported) but counts as 0, and a
left-out adjustment row doesn't reset its window. Property-tested: excluding a group gives
the same totals as deleting its lines, and suppressing equals excluding. Each group reports
a full total (as if nothing were left out) and an active total.

**Amended 2026-10-10 (entry 57):** the budget's group rules, as in the reference product:
lines with no group can be left out (adjustment rows with them), a line in a counted and an
uncounted group can be counted, and group totals can be shown without fringes.

**Amended 2026-10-11 (entry 58):** a group's full total is now Movie Magic's "total":
every tagged line at its base, suppressed ones included, plus only the fringes the lines
actually earn (a suppressed line earns none). It still ignores the what-if filters.

## 5. Globals and formulas (2026-10-08)

Amount, multiplier and rate are formula strings: numbers, global names, `+ - * /`, unary
minus, parentheses. Hand-written parser plus AST walker, no `eval`. Globals can reference
each other; they're evaluated in dependency order and cycles are reported with their path.

**Amended 2026-10-10 (entry 56):** a global's name may start with digits (`1ZG`), as in
the reference product: letters, digits and `_` with at least one letter or `_`.

**Amended 2026-10-11 (entry 59):** in the app, a formula that divides by zero or uses a
global that refers back to itself is refused at entry, and a global that would refer back
to itself is refused; a circular global from a file shows #ERR and counts as 0 there.

## 6. No dependencies in the engine yet (2026-10-08)

Property tests use a small seeded generator in `tests/properties.rs` instead of `proptest`.
It keeps `method-core` dependency-free until we actually need something (serde, for the format).

## 7. Blank fields and row types (2026-10-08)

Blank Amt and Rate count as 0, blank X and 4X as 1, so a title line is just a line with no
numbers. Subtotal rows show a running total and add nothing. Adjustment rows are real costs
whose rate is locked to the lines above them (back to the previous adjustment); Amt sets
the percentage, e.g. `0.1` for 10%. (Amended by entry 56: an adjustment row can carry
fringes, charged as on a line.)

## 8. Units (2026-10-08)

Units are a per-budget table with an optional hours equivalence. Our defaults are our own
(Hour 1, Day 10, Week 50, Month 200, plus Allow and Flat) and fully editable. Flat fringes
convert between time units through hours, using the line's primary unit only. (Amended by
entry 54: the Amount converts through the primary unit; X's and 4X's units tell a count
from a time, for the bases that read them.)

## 9. Topsheet, charges and credits (2026-10-08)

- Production-total rows split the topsheet into levels; the first level is above-the-line.
  With no production totals, everything is below-the-line. (Amended by entry 56: BTL is
  only the levels a production total closes; categories after the last one, and
  everything when there is none, count in neither, as in the reference product.)
- Fringe posting is a budget setting: by budget, production level, category (fringe account
  numbered category code + suffix, default `99`) or account. Every fringe dollar posts
  exactly once (property-tested).
- Charges apply to ATL + BTL in order and never to each other. Exclusions count each line
  once even if it matches several rules. (Amended by entry 56: a percent charge applies to
  every level plus fringes posted by budget, the grand total before charges, and an
  excluded line takes its fringes off with it in every posting mode; flat fees honor
  category and account exclusions too.)
- Credit rows are summed **unrounded**, then capped and discounted, then rounded once.
  (Amended by entry 57: a credit row can also be a topsheet category, its total with its
  own fringes, and excluded fringes are one list per credit, not per row.)

**Amended 2026-10-11 (entry 59):** the topsheet lists charges newest first and credits in
reverse application order with a Sub Total of two or more, and the budget's fringe row has
its own number, description and place.

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

**Amended 2026-10-11 (entry 59):** the set may follow the account when there is no
sub-account, and a new budget starts with category 2, account 2, set 3 (a file without a
pattern still reads as 2 + 2 with a hyphen).

## 12. Baseline, display currency, global deletion (2026-10-08)

- **Baseline** ("original" vs current) is a frozen copy of row, account, category, grand and
  net totals stored in the budget. Variance is computed on demand; rows match by id, so
  edited, deleted and added lines all show up correctly.
- **Totals currency** is display only: the report carries the rate and converts on request,
  rounding once. The math always runs in the base currency. (Amended by entry 59: the
  topsheet in that currency converts each account, fringe, charge and credit on its own and
  adds them up, as the reference product does.)
- **Deleting a global** first freezes its current value into every formula that used it, so
  no total changes (property-tested).
- **Budget info** fields (text, number, date) are plain data for now; they don't feed formulas.

## 13. Native file format (2026-10-08)

JSON, not TOML: budgets are deeply nested (topsheet > categories > accounts > rows), which
TOML handles poorly. Pretty-printed with fixed key order so files diff cleanly in git. It
lives in its own crate (`method-format`) with separate file-side types, so the engine stays
dependency-free and the file layout can stay stable while the model evolves. Money is
integer cents, decimals are strings, never JSON floats. Extension `.mthd`. Spec:
`docs/FORMAT.md`. First dependencies: serde and serde_json, pinned to releases well past
the 14-day cooldown (checked by publish date; zmij is dtolnay's float formatter, pulled in
by serde_json).

## 14. UI approach: Tauri desktop app (2026-10-08, decided by the owner)

Method Budget is a **local, open-source desktop app**, not a web app. Budgets live as
`.mthd` files on the user's disk; no server, no account, works offline.

Built with **Tauri**: the Rust engine runs natively in the app process, and the UI is drawn
with web tech inside a native window (no browser, nothing hosted). Chosen over a pure-Rust
GUI (Slint, iced, egui) because spreadsheet-grade grid editing is the heart of budgeting and
the web stack's grid tooling is far more mature. Small installers for macOS, Windows and
Linux. The JS toolchain this adds is held to the same dependency cooldown and lockfile rules.

## 15. Row shortcuts follow budgeting habits (2026-10-09, the owner)

Cmd/Ctrl+I inserts a line, Cmd/Ctrl+D deletes rows (undoable), Cmd/Ctrl+= inserts a
subtotal: the keys people switching from other budgeting tools already have in their hands.
Fill down moves to Shift+Cmd/Ctrl+D rather than the spreadsheet Cmd+D. Matching common
shortcuts is about familiarity, not copying; labels and menus stay our own.

Extended the same day (the owner): Cmd+minus inserts an adjustment (Option+Cmd+A still works),
Cmd+R and Shift+Cmd+R make and remove a fringe range, Shift+Cmd+Space edits a line's note,
Cmd+Left and Cmd+Right step accounts (Cmd+[ and Cmd+] stay as aliases), and the setup
tabs take the setup keys switchers know: Cmd+G globals, Cmd+3 fringes, Cmd+4 groups,
Cmd+5 locations, Cmd+6 currencies, Cmd+7 units, Cmd+8 sets, Cmd+9 settings, Cmd+\\
credits, Cmd+/ charges. Our own Cmd+1 to Cmd+9 tab numbering collided, so it is gone, and
the shortcut sheet moved from Cmd+/ to Shift+Cmd+/. File, clipboard, undo and select-all
keys are never given up for this.

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
bulk edits under ~100 ms. Measured with `examples/perf.rs` on `method_core::sample` budgets.

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

## 19. Four product calls from the roadmap (2026-10-09, the owner)

- **Autosave writes the file itself**, on by default with a visible toggle. Untitled budgets
  get a recovery copy until first saved, and every document keeps rolling backups so an
  autosaved mistake can be walked back.
- **Setup shortcuts match the reference product** (e.g. Cmd+G globals, Cmd+/ charges); our
  shortcut sheet moves to Shift+Cmd+/.
- **Several budgets open as tabs in one window**, each with its own undo; split view later.
- **Compare shows both** each budget's difference from the base and optional
  Sum / Average / Variance columns in the reference's style.

## 20. Autosave into the file, recovery copies, backups (2026-10-09, autosave default by the owner)

- **Autosave is on by default and writes the budget file itself** (the owner's call), 3 seconds
  after the last change, at most 30 seconds after the first unsaved one, and at once when
  the window loses focus, on close or quit, and before another budget replaces it. It is
  an app-wide setting (toolbar switch and File > Autosave), not stored in budgets: one
  person's preference shouldn't travel with a file to a colleague. Undo history is per
  session and survives autosaves.
- **Recovery copies cover what autosave can't**: a budget with no file yet, Autosave off,
  a failed write, and a recovered budget not yet saved into its original file. They live
  in the app's local data folder, one per open document with its own key (not one per
  path), so an old crash's copy and a fresh edit of the same file never overwrite each
  other. A recovered budget asks before its first save replaces the original file.
- **Running copies are told apart by a file lock** (std's `File::try_lock`, no crate):
  each running app holds a lock on its own owner file, and only copies whose owner lock is
  free are offered as recovered. No pid checks, no new dependencies.
- **Backups go in the app's data folder, not beside the budget** (`film.mthd~` was the
  alternative): nothing appears in shared project folders or sync services, there is
  room for several versions, and File > Revert to Backup can list them. Because autosave
  writes often, backups are spaced: always on an explicit Save, on the first write in a
  session, then at most every 10 minutes; 10 kept per file. Restoring is an undoable edit,
  never a file copy over the budget.
- **Every write is atomic** (temporary file, fsync, rename, folder fsync on Unix); app
  data is 0600/0700 on Unix; a budget keeps the permissions of the file it replaces.
- **Never write over a version we haven't seen.** Each read or write of the budget file
  records its size, time and an FNV-1a content hash; every autosave and Save into it
  compares first (size and time unchanged means unchanged; otherwise the content
  decides, so a sync service touching the file isn't a conflict). A change pauses
  autosave for that budget and asks: keep mine, load theirs (both back up the other
  version first, and refuse if they can't) or save mine as a copy. Loading theirs is an
  undoable edit. A missing file isn't a conflict; saving recreates it.
- **Files from the system** (macOS open events, a path on the command line on Windows and
  Linux) wait in Rust until the page has asked about unsaved changes, then open. No
  single-instance plugin (it would add crates): a second launch on Windows or Linux is a
  second copy of the app.

## 21. Tools panel layout (2026-10-09, the owner)

The side panel becomes two stacked tab panes, compact like the reference product's: top
pane Fringes, Groups, Globals, Units; bottom pane Sets, Locations, Currency, Notes, Budget
(Budget holds sub-tabs: Charges, Credits, Info, Settings, Baseline). Apply and Tables merge:
each list's checkbox applies the item to the selection and double-click edits it in place.
Tabs can be dragged between panes. Three positions: docked right (default), docked at the
bottom with the panes side by side, or popped out into its own window; the choice is
remembered.

## 22. Right-click menus, text style keys, editing setup items where they are listed (2026-10-09)

- **Keys.** Bold is Cmd+B. Cmd+I inserts a line (decision 15), so italic is Shift+Cmd+I;
  Cmd+[ and Cmd+] step accounts, so Indent More and Less are Option+Cmd+Right and Left.
  Text Color and Clear Formatting are menu only (Format menu and the right-click menu).
  The right-click menu also opens with Shift+F10 or the keyboard's menu key, at the cursor
  cell. Windows note: some older Intel graphics drivers take Ctrl+Alt+arrows for screen
  rotation; the menu items always work.
- **One undo step each.** `set_row_style` takes `indentBy` (each row moves and stays within
  0 to 4), so indenting a selection at mixed depths is one step. A new `update_group`
  sets a group's description, "counts in total" and color together, for the same reason;
  `set_group_color` stays for callers that only change the color.
- **Palette.** Eight colors, each with a light and a dark value, both at 4.5:1 or better
  on every grid background including selected rows. The file keeps one `#hex` (the light
  value); a dark window shows the dark one through static CSS keyed on the row's color
  class. Colors that came in some other way show as stored, and the group editor keeps
  them unless another is picked.
- **Double-click in the apply lists** (revised with decision 21's layout, entry 24). Only
  the checkbox ticks or unticks an item (a click on the box, or Space); a click on the
  item's name only selects it, and a double-click on the name (or Enter or F2) edits it. So
  a double-click never toggles anything and there is nothing to take back. (Until the
  layout landed, a click anywhere on the item toggled it, and a double-click undid that
  first click's toggle before opening the editor; that workaround is gone.) A double-click
  on the box itself is two ticks, as with any checkbox.
- **Topsheet and account list.** A double-click on a number or a description edits it in
  place; a double-click elsewhere on the row, or Enter, still opens it.
- **Cut, Copy and Paste from the right-click menu** fire the same clipboard events as the
  keys (`document.execCommand`), so they reach the grid's own handlers with no clipboard
  plugin or new crate. A web view may ask before pasting text another app copied (macOS
  shows a small Paste button) or refuse; then a message points at Cmd+V.

## 23. Copying rows with everything on them; the paste review (2026-10-09)

- **Whole rows copy as rows.** With the selection across every column (Select All, or
  Shift+arrows end to end), Copy and Cut take the rows: lines, accounts on the account
  list, categories on the topsheet, each with everything in it. Any narrower selection is
  a cell copy, as before. Edit > Copy Rows, Cut Rows and Paste Rows do it whatever the
  columns, for the menu bar and the right-click menu.
- **The rows live in Rust, keyed by the clipboard text.** One clipboard for the app, not
  per budget, so budgets opening as tabs (decision 19) share it. The page puts the rows'
  tab-separated cells on the system clipboard, exactly as a cell copy would; Rust keeps the
  rows and the setup they use, and that text is the key: a paste of any other text is a
  cell paste and forgets the rows. Not a custom clipboard type (WebKit doesn't keep those
  reliably across apps), not HTML (WebKit may rewrite it on paste), and not a marker in the
  text (it would land in spreadsheet cells). The one gap: identical text copied elsewhere
  would paste the rows, which is harmless.
- **A copy carries its setup**, transitively: fringes (and their flat units and the globals
  in their rates), groups, units, sets, locations, currencies and every global a formula
  names, however deep. Items the source doesn't define are left as names.
- **Paste adds what is missing and keeps the target's version of what differs** (roadmap
  decision 12), after a review that shows both definitions; per item the user can instead
  replace the target's version, or add the copied one under a new name (`Name (2)`, globals
  `NAME_2`), which the pasted rows and the copied items that refer to it then use (formulas
  rewritten token by token). Currencies are codes, so they can't be renamed. Same-name,
  same-definition items need nothing; a paste with nothing to review goes straight in.
  Everything a paste does is one undo step; a cut is copy plus a delete step.
- **Pasted content gets fresh identity**: row ids past every id the budget has used
  (versions and the baseline included), fringe ranges renumbered new in the target
  account, accounts and categories their own number when free, else the next free one in
  the same segment, wrapping round; a pasted category's accounts move under its number.
  Lines in the source's base currency count in the target's base currency (the review
  says so when the two differ).

## 24. The tools panel as built (2026-10-09)

Decision 21's layout, with the calls it needed:

- **One list per item kind, both halves in it.** Fringes, Groups, Sets, Locations and
  Currency are each a tab listing every item with its checkbox for the selection, its
  dollars in the selection, and the item's own editor. What the old setup tabs showed
  about the whole budget (a fringe's line count and total, a group's or set's total) moved
  to the item's tooltip. Deleting moved to the Delete key on an item and a Delete button
  in its editor, each asking next to the item first; adding is a "New ..." row at the
  bottom of every list (Globals and Units included), opening the same small form with the
  name first. Globals and Units keep their in-place rows.
- **The selection's totals sit in the panel header**, shared by both panes, with the three
  place buttons; the fringe list's foot keeps the fringe total, effective percent and the
  range buttons.
- **Two windows, one session.** The popped-out panel is a second web view on the same page
  (`?panel=1`) calling the same commands; nothing is copied or merged. Rust tells the
  other windows when a command changed the budget (a new view `rev`), and they fetch a
  fresh view, so each window always draws Rust's current budget. Pages listen only under
  their own label: Tauri hands every event to listeners registered for "any" window,
  which would make a window hear its own changes and the other window's menu commands.
- **The main window owns the panel's preferences** (place, size, tab layout, the tools
  window's geometry). The tools window sends its tab changes over, so nothing depends on
  the two web views sharing local storage.
- **The tools window is an ordinary window**, not always on top and not attached to the
  main one, so it can sit on a second screen and stay put. Rust places it where it was
  only if its title bar would be on a screen that is there now (else centered), drops the
  menu bar Windows and Linux would give it, closes it with the main window, and docks the
  panel back at the right when it closes on its own. Close Window closes whichever window
  is in front. Autosave on losing focus (decision 20) waits a moment and skips it when
  focus only moved to the other window: that isn't leaving the app.
- **Keys in the tools window** follow the main window's rules: text fields keep their
  editing keys, the tab keys work there, and the rest (Save, Undo, grid commands) go to
  the main window as menu commands; the menu bar does the same while it is in front. Undo
  of the budget therefore always runs in the main window, where its footer counters live.
- **Its own capability.** `capabilities/tools.json` grants the tools window only the
  commands the panel's scripts call (a test compares the two lists both ways), plus
  listening for and sending events. No file, save, print, undo, clipboard, recovery or
  window commands.
- **Tabs are dragged with pointer events**, not HTML drag and drop, which Windows' web view
  turns off while Tauri's file-drop handling is on; Option+Shift+arrows move a tab from the
  keyboard. The two panes split the height (or width) evenly; a divider between them can
  come later.

## 25. Budget tabs as built (2026-10-09)

Decision 19's "several budgets open as tabs in one window", with the calls it needed:

- **One document per tab, all in Rust.** The app state holds every open budget as its own
  `Session` (budget, file, dirty flag, undo and redo, autosave timing, recovery copy,
  backups, the file's stamp for outside changes, the account on screen) with an id, in tab
  order, one of them in front. Every command works on the budget in front unless its `doc`
  argument names another, and each window sends the budget it is showing with every command,
  so an edit made just as another tab comes to the front lands where the user was looking.
  Ids are never reused, and revs are never shared between budgets, so "a new rev" still
  means "something to redraw" for the other window. The row clipboard stays one for the app
  (decision 23).
- **Opening never replaces.** New, Open, Open Recent, a recovered budget and a file from the
  Finder each open in a new tab; a file already open just has its tab brought forward (paths
  compared once links are resolved), and an untouched Untitled budget alone in the window
  gives its tab up, as editors do. So New and Open no longer ask about unsaved changes. A
  recovered budget takes the place of its own file's tab when that was reopened untouched.
- **Closing asks only what autosave couldn't settle.** Close Tab autosaves first, then asks
  Save, Close Without Saving or Cancel in the page. Closing the window or quitting autosaves
  every tab, then asks once in a native dialog that names each budget left: Save All (the
  default; untitled budgets get their own Save dialog, and a cancel or a failure stops it
  with nothing closed), Discard All, Cancel. Cmd+W closes the tab, the window on the last
  one; Close Window moves to Shift+Cmd+W, as in browsers and other tabbed apps.
- **Keys.** Ctrl+Tab and Ctrl+Shift+Tab, and Shift+Cmd+] and Shift+Cmd+[, step through the
  tabs; Cmd+1 and Cmd+2 (free since decision 15) go to the first and second. Cmd+3 to Cmd+9
  stay the tools tabs. The shortcut list gained a `ctrl` key (Control on a Mac too) and an
  `anywhere` flag for grid commands that also work from a text field, since these keys mean
  nothing there.
- **What a tab shows.** The file's name without `.mthd`, or the budget's title before it has
  a file; a dot for unsaved changes; the close button on the tab in front and the one under
  the pointer; the title and path in its tooltip. The Window menu lists every budget, the
  one in front ticked. The window title is the budget in front.
- **Each tab comes back where it was left**: level, category, account, cursor, scroll and the
  footer's change counters are kept in the page per tab; Rust keeps each budget's account on
  screen, so its rows come with the view.
- **The next start reopens the files**, in tab order with the same one in front, from
  `tabs.json` in the app's data folder (paths only, rewritten when the list changes). Budgets
  with no file come back through their recovery copies, never from this list.
- **Split view is later** (behavior note 04): a second pane would be a second grid sending a
  different `doc`; nothing about tabs needs to change for it.

## 26. Find and replace as built (2026-10-09)

Behavior note 18 and roadmap 1.11, with the calls it needed:

- **A tab in the tools panel**, not a bar over the grid: Find sits with the setup tables as
  in the reference product, so it docks, moves between panes and pops out with them, and
  its results can sit next to the grid while the grid is edited. Cmd+F and Shift+Cmd+F
  open it (find or replace mode) wherever the panel is.
- **Next and previous are F3 and Shift+F3** (and Enter and Shift+Enter in the search box):
  Cmd+G stays the Globals tab (decision 15), so the usual Cmd+G and Shift+Cmd+G aren't
  free.
- **Typed, one kind of thing at a time**, as the reference product does, plus formula text
  and notes, which it didn't show. Text has match case and whole word; names (globals and
  setup items) match exactly, picked from suggestions. A global matches only as a whole
  name the way the formula reader splits a formula, so `DAYS` never touches `SHOOT_DAYS`.
- **Replace only what is checked**, every result checked to begin with, as one undo step,
  with the count of places changed. Every checked place must still match when Replace
  runs, or nothing changes: a result list a moment old never edits a cell that changed
  since. Results are searched again after every change, so replaced places drop out.
- **A global goes in as a name, a number or a formula**: inside a longer formula a formula
  goes in parentheses so it means the same; a cell that is only the global takes it as
  typed. **Remove** on a global puts its current value in its place, as deleting a global
  does (decision 12), so no total moves; on an item it takes the item off the lines.
- **Searching never waits for totals**: it reads the budget as it is, in budget order, in
  well under a millisecond at 5,000 lines; at most 1,000 results are sent at once, with the
  full count.
- **Renaming a setup item everywhere is separate** (and not built): replace swaps one
  existing item for another in the places checked; it never creates or renames one.

## 27. Versions, compare and original columns as built (2026-10-09)

Decisions 17 and 19 in the app, with the calls they needed:

- **Versions share their content.** `Version::budget` is an `Arc<BudgetSnapshot>` (was a
  `Box`): every edit works on a copy of the budget for undo, and copying N versions' rows on
  every keystroke would undo decision 18. Now a copy costs a pointer per version, and
  comparing two version lists stops at equal pointers. Content is never changed in place,
  so nothing else changed; the file format is the same. Measured (release, this Mac, 5,000
  rows): a one-cell edit with its view and JSON takes 3.4 ms with 10 versions, as with
  none; copying and comparing 10 boxed versions alone took 8.7 ms more per edit.
- **Every version command is one undo step**: save, rename, delete, and restore with its
  automatic `Before restoring "<name>"` version, so Undo takes back a whole restore. Names
  are one line, trimmed, 1 to 200 characters; notes up to 10,000 (as line notes), blank
  meaning none; `created` is the page's local time with its offset, checked for shape only
  (the engine still never reads the clock). Questions (restore, delete) are asked on the
  version's own row in the dialog, since one dialog is open at a time.
- **Compare sources**: the budget in front against 1 to 15 open tabs, versions of it, or
  files picked in a native dialog from Rust (named afterwards by a token; the last 32 are
  kept; a picked file open in a tab is that tab). Each is compared as its own grid totals
  it, problem cells blank, and says so. Budgets are copied out under the lock and compared
  after it, so a large comparison never holds other commands up (16 budgets of 5,000 rows:
  about 210 ms in all, under 1 ms of it holding the lock; 84 KB of JSON). Lines match by id when
  every budget is the base or one of its versions, else by description.
- **Both readings (decision 19).** Each budget's difference from the base (amount, percent
  of the base) is on by default; Sum and Average of the others, Variance (that sum minus the
  base) and Variance % (of the average, exact rather than of the rounded average, its sign
  following the variance) are optional columns. `CompareLine::aggregate` computes them in
  the engine, in cents, the average rounded once; percents go to the page as text with two
  decimals, rounded once.
- **Engine additions**: production totals are compared, matched by name (trimmed,
  case-insensitive; the unnamed level after the last one isn't listed, as the topsheet
  doesn't show it); each matched category, account and line says where it is in each
  budget (`at`, `ids`), so a click can go there. Charges and credits stay single total lines.
- **Orig and Var** show at every level when there is a baseline, before Total (Total stays
  the rightmost column, pinned on the lines), on by default then and hidden from the
  column menu without one. One choice covers all three levels.
- **Printing** goes through the report module's `printHtml`: the comparison as shown
  (columns ticked, rows open), landscape, with a legend of which budget each letter is. It
  is loaded when asked for, so the screen works without it.
- **Not yet**: charges and credits per item.

## 28. Reports: our own pages, presets in the budget, PDF without the print sheet (2026-10-09)

Roadmap 1.15 and 2.3 to 2.6 (behavior notes 15 and 19), as built:

- **We lay out the pages ourselves.** Checked in a hidden WKWebView printing to PDF: the
  system web view repeats no `thead` on later pages, draws no `tfoot` per page, ignores
  `@page` margin boxes (so no page numbers) and `@page` size, and prints fixed elements
  once. So `report.js` measures every row once in a hidden page-wide box and deals rows
  onto Letter-sized page boxes (816 by 1056 CSS pixels), each with its own header, column
  headings and footer, keeping a heading with what follows (a category or account heading
  with its first lines, the last line with the account's totals, the topsheet's summary
  together). "Page X of Y" works everywhere because we count the pages. The preview in
  the dialog is those same pages scaled down, so it is what prints.
- **Each page box is one sheet.** WebKit lays a printed document out at least 1.25 times
  the paper's width in CSS pixels and scales it to fit; a document exactly 96 px to the
  inch wide (both orientations are) prints at 100%, page box for sheet. The print
  operation is given Letter, the orientation and zero margins (the boxes carry their own).
- **PDF without new crates.** Tauri's `print()` always shows the print sheet with the
  shared page setup, and `createPDF` makes one tall unpaged page. AppKit's
  `NSPrintOperation` on the web view (`printOperationWithPrintInfo:`, macOS 11+) with a
  save disposition and a file URL writes a paginated PDF with no panel. We reach it with
  plain Objective-C runtime calls (`src/pdf_mac.rs`, the app's only `unsafe` code): no
  blocks, no struct returns, a one-method delegate class to hear when it is done. The same
  call with the panel shown is Print, so the orientation is preset there too. Windows and
  Linux keep `window.print()` and the dialog's own PDF choice for now.
- **Presets live in the budget file** (`report_presets`, additive, left out when empty) as
  layout only: sections in order, categories and accounts left out (listed as exclusions,
  so new categories and accounts print until someone unticks them), what to skip,
  columns, layout, header and footer. One default, enforced by the app. Versions don't
  carry them and restoring keeps the current ones: a report layout is not budget content.
  Each change is one undo step.
- **Filters never change totals.** Leaving lines or accounts out of a report prints the
  budget's own totals with a note saying rows were left out; the topsheet always lists
  every category. A report is a view, not a recalculation.
- **Notes print as numbered footnotes** under each account's totals (long ones cut at 700
  characters there) and in full in the notes list, with matching numbers; a switch leaves
  them off the detail pages for printouts that leave the office.
- **Not yet**: paper sizes other than Letter, a "detail starts on page N" topsheet column,
  setup tables beyond fringes, globals and groups (units, sets, locations, currencies,
  chart of accounts), dragging column widths on the preview.

## 29. Starting from another budget: import setup and templates (2026-10-09)

Roadmap 1.20 (behavior notes 07 and 24, decision 15), as built:

- **Import from a budget file, no library format yet.** File > Import Setup From… picks any
  `.mthd` in Rust's Open dialog (a file open in a tab is read from the tab, unsaved changes
  and all; a budget can't import from itself) and reviews every item in it. The personal
  library (2.15) can later be a budget file used this way.
- **What can come over**: globals, fringes, groups, units, sets, locations, currencies,
  charges, credits, tag presets, line blocks, report presets, and three budget settings
  (account numbering, fringe posting, new-line defaults). Not the chart of accounts, lines,
  budget info, base or totals currency, baseline or versions: those belong to a production,
  and rows already travel by copy and paste (decision 23).
- **The paste review's rules, reused** (src/clipboard.rs): same name is the same item;
  missing items are added; an item that differs keeps this budget's version (decision 12)
  unless the user picks the imported one or "add it under a new name" (`Name (2)`, globals
  `NAME_2`), which every imported item that refers to it then uses (a fringe's rate, a
  global's formula, a charge's excluded groups, a credit's groups and fringes, a tag preset,
  a line block's rows and its own copy of their setup, a report preset's groups).
  Currencies and settings take no new name; a setting comes in only with "use the imported
  version". Charges and credits match by exact name, presets ignoring case (as their names
  are kept unique); a replaced one keeps its place, a replaced report preset keeps being the
  default if it was, and an added one never is. The other budget's base currency is never
  imported, and flat charges in it count in this budget's base currency.
- **Dependencies come along, shown.** Each item in the review lists everything it needs in
  the source, however deep (a fringe's flat unit and its rate's globals, a global's globals,
  a charge's excluded groups and currency, a credit's groups and excluded fringes, a tag
  preset's tags, what a line block's rows use, a report preset's groups). Ticking an item
  ticks those, locked while something ticked needs them; Rust walks the same closure again
  when importing, so the page can't leave one out. Each kind has a box to tick all of it.
- **One undo step**, worked out completely before anything changes, so a refusal (an item
  gone, an unknown choice, more presets than a budget keeps) changes nothing. The page names
  the source by a token, never a path; only the last file picked is kept.
- **A template is just a budget.** New From Template… opens any `.mthd` in a new tab with its
  chart (categories, accounts, production totals), setup, presets and settings, and without
  its detail lines, budget info, baseline, versions and line notes; "keep the detail lines"
  keeps the lines without their notes. It is titled Untitled and has no file, so the first
  Save asks where, and like a New budget it starts with no unsaved changes. Save as
  Template… writes the budget in front, stripped the same way, to a new file named in Rust's
  Save dialog and titled after it; the budget and its own file don't change, and a file open
  in a tab is refused. No template format, folder or built-in templates: our own starter
  budget (`method_core::starter`) remains what New opens.

## 30. Spreadsheet export as built (2026-10-09)

Roadmap 2.1 (inventory section 21: what accounting and payroll tools take in):

- **One set of tables, three writers.** `method_format::export` turns a budget and its report
  into plain tables (Topsheet, Accounts, Detail, Globals, Fringes, Groups, Units,
  Currencies, Sets, Locations); the Excel, CSV and tab-delimited writers only serialize
  them, so every format carries the same figures. It lives in `method-format` so `method-core`
  stays dependency-free, and it is a one-way copy for other tools, not a second native
  format.
- **No new crates.** An .xlsx is a ZIP of XML parts, so we write both ourselves: stored
  (uncompressed) ZIP entries with a compile-time CRC-32 table and a fixed 1980-01-01
  timestamp (the same budget gives the same bytes), and the minimal SpreadsheetML parts
  (content types, relationships, workbook, styles, shared strings, one part per sheet,
  core and app properties). Stored entries make a bigger file (about 3 MB for a 5,000-row
  budget); deflate can come later if that matters.
- **Money is exact.** Cents become a decimal string (`1234.56`) without ever passing
  through a float; the workbook stores that string as the cell's number with a
  `#,##0.00;(#,##0.00)` format (the app's own negatives in parentheses). Decimals are
  written as the engine holds them. Text is always a shared string, never a formula.
- **The export is the budget's real figures.** Unfiltered (every group as the budget sets
  it), and refused while any cell has a problem, because the grid then counts those cells
  as blank. The app takes the tables from its cached recalc; `method export` recalculates.
- **Detail is every row**, subtotals, adjustments and separators included (with their
  kind), so its row count is the budget's. Totals stay summable: a subtotal row's running
  total is in its own Subtotal column, not Total, and a category fringe account's amount is
  in Total only. An adjustment's rate is the amount it is a percentage of, worked out the
  way the engine does.
- **CSV and tab-delimited.** CSV is RFC 4180 (CRLF, quoting only when needed). Tab-delimited
  is for importers that read field by field, so it never quotes; tabs and line breaks in a
  field become spaces. Both neutralize text a spreadsheet would run as a formula (a
  leading `=`, `+`, `-`, `@`, tab or CR gets a `'`, plain numbers like `-5` excepted),
  because a budget file can come from anyone. The app's CSV files start with a byte order
  mark (Excel needs it for accented letters); its tab-delimited file doesn't (importers
  may not expect one); `method export --bom` adds it on request.
- **Files.** CSV is one file per table, `<name> - <Table>.csv` beside the name chosen in
  the Save dialog (one dialog, predictable names), instead of a folder picker. The path
  always comes from the Save dialog, the format's extension is forced, each file is written
  atomically, and an export never replaces a budget open in a tab; `method export` refuses to
  write over the budget it reads.
- **Not yet**: deflate compression, a choice of filters (groups) for the export, Original
  and Variance columns, budget info and charges/credits as their own sheets.

## 31. Tag presets and line blocks as built (2026-10-09)

Roadmap 1.19 and 2.14 (behavior note 05, decision 15), as built:

- **In the budget, as tools.** `tag_presets` and `line_presets` are additive budget fields,
  left out when empty (FORMAT.md). Like report presets they are not content: versions
  don't carry them, a version holding some is rejected, and restoring a version keeps the
  current ones. Names are unique ignoring case, up to 80 characters; a budget keeps up to
  200 tag presets and 100 blocks of up to 500 rows.
- **Add never takes away; Replace is exact.** Add joins the preset's fringes and groups
  with the line's, and fills set, location and currency only where the line has none (a
  base-currency line has none), so Add can't change a choice already made. Replace makes
  all five tags exactly the preset's: an empty part clears that tag. We chose "fill only
  when empty" over "overwrite" for Add because overwriting a set or location is what
  Replace is for, and Add stays safe to use on a mixed selection. Row eligibility follows
  the apply lists (fringes and currency on lines; groups, set and location on lines and
  adjustments). One undo step; no change, no step.
- **A preset that names something gone is refused, not half applied.** Deleting a fringe,
  group, set or location takes it off lines but not off presets; applying such a preset
  says which items are missing and changes nothing, and the editor shows them to untick.
  Renames don't follow into presets (there is no rename of setup items yet).
- **Blocks reuse the paste of copied rows.** Inserting a block is the clipboard's paste
  (src/clipboard.rs) of the block's rows: new ids above the cursor line, fringe ranges new
  in the account, missing setup added, one undo step. A block keeps its own copy of the
  setup its rows use (taken with the clipboard's dependency walk), only to offer back an
  item the budget no longer has; the review lists just those, and an item set up
  differently now keeps the budget's version without asking, since the block came from
  this budget. Block rows are stored without ids; their fringe ranges are numbered 1, 2...
  within the block.
- **The checklist when saving a block** is four boxes (amount/X/rate/4X, units, tags with
  fringe ranges, notes), each starting unticked when none of the lines use it.
  Descriptions, row kinds and text styles always come along. A tag preset made from a
  line has a box per tag, ticked when the line has it.
- **Lean views** list blocks as `{name, count}`; rows and setup stay in Rust.
- **Not yet**: reordering presets with the mouse (Option+Shift+Up and Down only), editing
  a block's rows in place, presets in a personal library (2.15).

## 32. Moving rows and renaming setup items as built (2026-10-09)

Roadmap 1.10 and 1.18 (decision 7), with the calls they needed:

- **Option+Up and Option+Down move rows**, at all three levels: lines within their account,
  accounts within their category, categories and production totals on the topsheet. The
  keys were free (Option+Shift+arrows are the tools tabs', Cmd+arrows change level and
  step accounts); text fields keep them for the caret. Any selection, contiguous or not:
  each selected row steps past the nearest unselected row, selected rows keep their order,
  rows at the edge stay and the rest close up. Nothing moving is no change and no undo step.
- **Fringe ranges are never split.** A range whose lines are all selected moves whole, with
  the subtotal and separator rows between its lines (they join the selection). A selected
  line from a range whose other lines stay behind leaves the range (its number cleared),
  and the lines staying behind count as one block that a moving row steps over in one
  press, never landing inside. The alternatives were refusing the move (annoying when a
  line is simply in the wrong place) or dragging the range along (moves lines nobody
  selected). Ranges that were already interleaved in a file join into one block.
- **Quick presses are one undo step**: `Session::edit_merging` joins an edit to the
  previous step when that step was made by it less than a second ago, nothing happened
  since (no other edit, undo or redo: the rev is unchanged), and it names the same rows
  (each move hands out the key the next press must bring). The joined step is rebuilt from
  where the run began, so a run that ends where it started leaves no step at all and the
  dirty mark goes back. Holding the key sends at most one press behind the one running.
  Accounts and topsheet rows are named by place, so each press is built from the newest
  view; the reply gives the moved rows' new places (or ids) and the grid selects them.
- **Renaming rewrites references and keeps names in the file** (decision 7). The engine
  does it (`method_core::rename`): globals token by token the way the formula reader splits
  them, with the rest of each formula kept byte for byte (not reprinted through the parse
  tree, which would respace formulas and break "rename and back gives the same file");
  fringes, groups, units, sets, locations and currencies everywhere they are named,
  including charges, credits, report presets' group filters, tag presets and line blocks.
  A line block's saved copies of setup are filed under the new name too: they are matched
  to the block's rows by name, so leaving them under the old one would stop an insert from
  adding back an item the budget has since lost. Their definitions stay as saved.
- **The new name must be free**: not another item of the kind and not a name already used
  without an item behind it (a unit typed on lines, a global a formula names, a copy in a
  line block). Allowing it would merge two names, which renaming back couldn't undo.
  Property-tested on generated budgets: every total is unchanged, and renaming back gives
  the identical budget, for every kind including the base currency.
- **Where to rename**: the apply lists' editor gets the name as its first field (a new name
  renames first, then any other change is saved under it: two undo steps when both change,
  since a rename is its own command); right-click on a list item, or on a global's or
  unit's name, offers Rename everywhere; a global's or unit's name opens the form on a
  double-click, Enter or F2; Find's replace panel links to it for the item searched for;
  the base currency field in Settings now renames through it too (it used
  `set_base_currency`, which left lines that named the old code pointing nowhere), so the
  tools window no longer needs `set_base_currency`. Each rename says how many places
  changed. Saved versions keep their own names: they record the budget as it was.
- **Not yet**: dragging rows by the # column (the grid draws only rows near the viewport
  on long accounts, so a drag would need its own auto-scroll and drop marks), and moving
  lines between accounts other than by cut and paste.

## 33. The rest of the report and compare items (2026-10-09)

Roadmap 2.3 to 2.7, after an audit of what decisions 27 and 28 already built:

- **Additive preset fields only** (FORMAT.md changelog): sections `chart`, `units`, `sets`,
  `locations`, `currencies`; `vertical_rules`, `collapse_details`, `topsheet_fringes`,
  `topsheet_pages`; a band's `first_page_only`; slot kinds `date`, `time` and `global`.
  Each is left out at its default, so earlier presets and the golden file are unchanged.
- **A slot still shows one thing.** The reference lets text and fields mix in one zone;
  ours keeps one field per slot (decision 28) and adds separate date and time fields and a
  global's value. Mixed text would need a small template language in the file; not worth
  it until someone asks.
- **A global in a header follows a rename** of that global (`method_core::rename`, counted
  among its references), and a preset imported from another budget brings the globals its
  bands show along with it, renamed with them.
- **The topsheet's Page column is filled after the pages are dealt.** Its cells have a
  fixed width and are measured empty, so writing the numbers in moves nothing and one
  layout pass is enough. The number is the page with the category's heading on the
  detail level; blank when the detail level doesn't print the category.
- **One line per account** prints the account's code, name and total (original and
  variance with a baseline) under each category's heading, with the category's totals.
  The detail columns drop to #, description, total and the baseline's two, since there
  are no line figures to show, and the categories run on instead of each starting a page,
  which is the point of a short version. Line notes don't print in it (the notes section
  still lists them).
- **First page only** gives the later pages the band's room: the paginator works with the
  first page's room and the later pages' room separately.
- **Sets and locations totals** are the engine's, which count only lines in the budget
  total (unlike groups' "every tagged line"); the section says so.
- **Charges and credits per item in compare** are matched by name, trimmed and ignoring
  case, like production totals: a charge only some budgets have shows `none` in the others
  and `not in ...`, so differently named items list per budget. The totals lines stay.
- **Page navigation** in the preview: first, previous, next, last and a page number to
  type; the number follows scrolling. The preview pane is now positioned, so a page's
  offset is measured from it (the Go to section list relied on that too).
- **Not yet**: mixed text and fields in a slot, the budget's created and modified dates
  (the file keeps none), one header design shared by several presets, charges and credits
  as their own report sections, the reference's point sizes and "page break between
  sections" switch (every section still starts a page).

## 34. Globals' display info and display settings as built (2026-10-09)

Roadmap 2.10, 2.18 and 2.19. Everything here is display only: the engine stays exact
(entries 1 and 2) and nothing changes a stored value or a total (Rust tests check the
report is identical after each new command).

- **Cells show values.** A formula cell shows its value (sent per row of the account on
  screen as `values`, evaluated in Rust from the report's globals, never in JavaScript),
  in the formula color with the formula on hover, in the formula bar and while editing.
  This is a change: formula cells used to show the formula. Globals set to **show by
  name** keep their cells showing the formula. **View > Show or Hide Formulas in
  Selection** (Option+Cmd+F, on by default as in the reference) shows formulas in every
  cell of a selected range; a single cursor cell keeps its value (the formula bar shows
  it). Copy, paste and fill down always use the formula as typed.
- **Global info** (`global_info` in the file, content, so versions carry it): description,
  unit (from the units table), decimals 0 to 8, show by name, folder. A separate map rather
  than changing `globals` from formulas to objects, so every reader of `globals` (the
  engine, find, reports, quick entry) is untouched. Renaming or deleting a global moves or
  drops its entry; renaming a unit renames it on globals, and a global's unit counts as a
  use of that unit name (so a rename can't merge two names). The writer leaves out entries
  for names that aren't globals and the reader ignores them. Folders are a field only: the
  Globals list doesn't group by folder yet (the custom order work may want to).
- **Which settings live where.** In the budget (`display`, tools not content, like presets:
  versions don't carry it and a restore keeps the current one): rate decimals with a cutoff
  band, whether a rate's global decimals win, the topsheet's summary rows and net total,
  because they describe how this budget reads (the reference keeps them per budget). Per
  user, in local storage: totals with or without cents (the reference has it as a system
  preference), hiding zero accounts, grid zoom and the formula view, because they are how
  one person likes to look at any budget, and toggling them shouldn't dirty the file or
  add undo steps.
- **Rounding for display** is half away from zero, padded with zeros, done on the decimal
  strings with BigInt (no floats). Rates: a size below the cutoff uses the below setting,
  at or above it (or every rate with no cutoff) the above setting, "as typed" when unset;
  a rate formula's first global with its own decimals wins unless switched off. Amount, X
  and 4X formulas use their first global's decimals, else the value as calculated; plain
  numbers stay as typed except rates.
- **Hidden factors** mark only what can change a total: X or 4X other than blank or 1 (a
  formula by its value; a failing one is already flagged as a problem) and a non-base
  currency, when that column is hidden. The unit columns are labels in our engine (flat
  fringes use the primary unit only), so a hidden unit never marks a line. The mark is a dot
  in the # cell with the reason as its tooltip; reports don't draw it.
- **Zoom** scales the grid's font, row heights and column widths (70% to 150%), not the CSS
  `zoom` property, so the grid's measuring, windowing and pinned columns keep working.
  Option+Cmd+= and Option+Cmd+minus because Cmd+= and Cmd+minus insert rows.
- **Hiding zero accounts** keeps the row under the cursor, rows with problem cells, and rows
  whose baseline original wasn't zero.
- **Not yet**: reports and exports don't follow the screen settings (the report setup has
  its own options); a global's unit doesn't fill in a line's unit; no grouping by folder.

## 35. Dated notes, in-use counts, short codes and custom order as built (2026-10-09)

Roadmap 2.8, 2.9, 2.11 and 2.12 (behavior notes 06, 09, 12, 14, 16, 19, 24; the notes
model is the roadmap's call 11: several dated notes, no replies), with the calls they
needed:

- **Notes are a list on lines, accounts and categories.** `Detail::note` became
  `notes: Vec<Note>`, and `Account` and `Category` got `notes` too. A note has an id
  (unique within its list; a new one gets one past the highest there), its text, a
  `created` time, an optional `edited` time and `author`, and `done` (ticked off in the
  review list). Times are ISO 8601 text from the page, as for versions: Rust never reads
  the clock for them. The author is free text from a local setting in the Notes tab (no
  accounts, decision 14); it lives in the window's local storage, so the popped-out tools
  window may not see a name typed in the main window. No replies or threads (low value for
  one person's local file).
- **The file keeps version 1.** The writer writes `notes` (left out when empty) and never
  `note`; the reader still takes a row's old `note` string (detail rows and line block
  rows) as one undated note at the front, with an id after the list's highest. The
  alternative, keeping `note` for one note and adding `notes` for more, would have left two
  places for the same thing forever. The golden file has no notes and is unchanged.
- **Owners are addressed by identity, not position**: a row by id, an account by number,
  a category by code (both unique in a budget), so a command sent from the tools window
  lands on the right item even if the other window moved things. Four commands, each one
  undo step: add, update (blank text deletes; the same text changes nothing), delete, done.
- **Shift+Cmd+Space adds a note** to the line, account or category under the cursor (a
  small dialog), at all three levels and in the right-click menus; editing and deleting are
  the Notes tab's job, since there can be several. Find's Notes scope searches every note
  on a line (not account or category notes); replacing that leaves a note empty removes
  it. Templates drop every note; copied accounts and categories keep theirs; line blocks
  keep their rows' notes when Notes is ticked.
- **Reports print every note**, each with its date and author: a line's notes together
  under its footnote number (cut at 700 characters there), and the notes list with
  category and account notes before their lines'. `skip_done_notes` on a report preset
  leaves reviewed ones out of both.
- **In-use counts come from their own command** (`setup_usage`), fetched by the panel a
  moment after the budget changes and only while a setup list is drawn, not sent with
  every view (decision 18). One pass over the rows per table; a global's uses are the
  name tokens in each formula, as the formula reader splits them. Lines count once per
  row; "other" counts setup that names the item (globals and fringe rates for a global,
  flat fringes and globals' units for a unit, credits and charges for a group, credits for
  a fringe, charges and the totals currency for a currency). Used means either. Dollars
  come from the report: a fringe's own fringe dollars; a group's, set's or location's lines
  with their fringes (a group out of the total counted as if it were in); a currency's
  lines in the base currency; none for globals and units.
- **Deleting something in use takes it off the lines**, which is what deleting a fringe,
  group, set or location already did; the question now says how many lines and the button
  says so. A currency in use used to be refused: now `clearLines` moves its lines to the
  base currency first (charges and the totals currency still refuse). A unit used to stay
  on lines as plain text: with lines using it, the delete now blanks those unit cells, so
  nothing names a unit that isn't there. A global still freezes its value into formulas.
- **Short codes are display only.** `short_code` on fringes and groups, unique within its
  kind ignoring case, never another item's name, no commas, up to 16 characters (checked
  by the commands, not the reader). The grid's Fringes and Groups cells show codes where
  items have them, with the names in the tooltip, while the cell's edit and copy value
  stays the names, so editing, pasting and every reference still use names, and a rename
  never touches a code. The type-ahead matches codes and puts the name in; a code typed
  without it is not read as the item (it would be an unknown fringe, flagged).
- **The order is an explicit list per table, omitted when alphabetical.** The tables stay
  `BTreeMap`s sorted by name; `Budget::setup_order` keeps names first to last. Shown
  order: listed names that exist, then the rest in byte order, so a new item lands last
  and a stale name (deleted elsewhere, or in a restored version) is harmless. The writer
  writes each table's full shown order, or nothing when that is byte order, so the file
  never holds stale names. Deletes drop the name and renames rewrite it (not counted as a
  place). Layout, not content, like display settings: versions don't carry it and
  restoring one keeps the current order. The setup lists, the Apply lists, the type-ahead
  (which keeps a hand-made order instead of sorting matches alphabetically) and printed
  setup tables follow it. Dragging uses pointer events and a handle (as the tab strips
  do); Option+Shift+Up and Down move the focused item, quick presses joining one undo
  step (as moving rows does). The Name head's sort (A to Z, Z to A, back to the budget's
  order) is a view and never changes the file. No moving while sorted or filtered, so
  hidden items never move. Folders (behavior note 16) are not built.
- **Not yet**: account and category notes in Find; a note's mark is not clickable (the
  Notes tab follows the cursor instead); the author name isn't shared between the two
  windows; reordering the base currency.

## 36. Spreadsheet import with a column mapping as built (2026-10-09)

Roadmap 2.2 (inventory section 24), built ahead of 1.16 as the generic part that 1.16 will
reuse: nothing here guesses the reference product's export layouts (that waits on the
export test).

- **Our own readers, no crates.** An .xlsx is a ZIP of XML, so `method_format::import` reads
  it by hand: a DEFLATE decoder (RFC 1951: stored, fixed and dynamic Huffman blocks, after
  the structure of Mark Adler's public-domain reference decoder), a ZIP reader working
  from the central directory, a small XML tokenizer, and the SpreadsheetML parts found
  through the package's relationships; plus RFC 4180 CSV and tab-delimited text with
  delimiter sniffing and the usual encodings. The decoder is checked against zlib's own
  streams (Python's standard library, committed as fixtures with the script that made
  them), against a test-only encoder that builds random valid streams of every block
  type, and on every truncation, bit flips and noise.
- **Every file is hostile until read.** Limits: 50 MB per file, 200 MB decompressed per
  workbook, an entry decoded into at most its declared size and refused unread when that
  is over 1,000 times its compressed size (beyond 1 MB), CRCs checked, 10,000 entries,
  100,000 rows, 256 columns, 2 million cells a sheet, 32,767 characters a cell, 128 MB of
  cell text a workbook (shared strings are copied into cells, so a short file could
  otherwise ask for gigabytes), XML nesting 128 deep. No DTDs and
  no entities beyond the predefined five, so nothing is ever expanded or fetched.
  Encrypted entries, ZIP64 and split archives are refused (no budget sheet needs them).
- **Values, not formulas or styles.** A workbook's cached values are what its sheet shows;
  reading Excel formulas would mean evaluating Excel. Number cells keep the digits the file
  wrote and are rounded once to the engine's nine places (`0.30000000000000004` is 0.3);
  dates come in as serial numbers. A text cell that is a formula our engine can work out
  (its globals exist) is kept as a formula; anything else that isn't a number is left blank
  and listed, never guessed.
- **One column per field, guessed from titles.** Each field has a list of titles that mean
  it (exact matches beat titles that contain one; each column is used once), so our own
  export maps itself and a department sheet's `Acct #`, `Qty`, `Unit Price` and `Ext.
  Total` come out right. We chose fields to columns (a select per field) over a select per
  column because the preview shows lines as they will import, not the raw sheet. The
  fields are where a line goes (category, account), what it holds (description, amount,
  unit, X, rate, currency, 4X, fringes, groups, set, location, note) and the line total to
  check, plus row kind and fringe range, which an exact round trip of our own export needs
  (adjustment rows and fringe range cutoffs change totals); the export's
  Detail sheet gained a Fringe Range column, added last so positional importers keep
  their columns.
- **Rows as budget sheets write them.** Account numbers go through the budget's account
  numbering and match anywhere in the budget; one that doesn't fit is refused, never cut
  short (cutting `110005` to `11-00` could merge two accounts). An empty account cell takes
  the account above it, and a row with only an account (or an account and a description
  over rows that leave the account blank) is a heading that names it. New accounts and
  categories (when asked for) are placed after the last one numbered below them. Lines go
  at the end of their accounts with new ids, fringe ranges new in the account (as pasting
  does, decision 23). Missing names are added (fringes at 0%, currencies at a rate of 1,
  said in the preview) or left off the line and listed; a fringe, group or currency the
  budget lacks can't stay on a line, since the engine would refuse it.
- **Defaults are cautious.** Adding accounts and adding setup items both start off, so a
  typo in an account number shows as a skipped row instead of a new account; a saved
  mapping keeps both choices.
- **Reconciliation is the real calculation.** The preview applies the import to a copy and
  recalculates, so each line's total is what the grid will show (adjustments see the lines
  above them, groups suppress, currencies convert), compared with the sheet's line total
  in cents; a subtotal row compares its running total.
- **A new tab is New From Template of this budget plus the import.** It keeps the chart,
  setup and settings that make the imported lines' totals mean something (fringes,
  globals, currencies), and the import is its first undo step, so it starts unsaved. The
  alternative, an empty chart built from the sheet alone, loses every fringe rate.
- **Presets stay with the user.** Saved mappings live in the web view's storage (per user,
  like the other display choices), matched by the header row's normalized titles; they
  never go into a budget file, since the same sheet layout feeds many budgets.
- **Not yet**: reading formulas or number formats, more than one column per field (such
  as two description columns joined), a library of preset mappings to share, and the
  reference product's own layouts (1.16).

## 37. The personal setup library as built (2026-10-09)

Roadmap 2.15 (behavior notes 09, 11 and 24; decision 15), as built:

- **A library is a budget file**, not a new file type: `My Method Budget Library.mthd` made in
  a folder the user picks in Rust's folder dialog, or any `.mthd` picked in the Open dialog.
  It has setup and no lines, starts empty (no starter units either: we ship no library
  data, CLEAN-ROOM.md) and opens in the app like any budget. One additive field,
  `setup_folders` (kind to folder to item names), holds its folders; any budget may carry
  it, it is left out when empty, and like `setup_order` it is layout, not content
  (versions don't carry it; restoring keeps it; Rename everywhere keeps an item in its
  folder). Which file is the library is remembered per user in the app data folder's own
  `library.json` (next to `settings.json`, kept separate so the library's setting never
  races the Autosave switch), written atomically.
- **Copying reuses Import Setup** (entry 29) in both directions: the same dependency
  closure, the same review dialog (importsetup.js, with its words swapped) and the same
  keep, take or new-name choices. Adding to a budget keeps the budget's version of what
  differs by default (decision 12) and is one undo step. Saving to the library is the same
  import with the library as the target, so the library keeps its version by default, as
  the brief asked; behavior note 09 saw the reference overwrite the library's copy of a
  dependency, which "Replace it with this budget's" still does on request. The three
  budget settings are not library items. New items land unfiled. No live link: every copy
  is one-time.
- **Writes** read the library file, apply the change and write it back atomically
  (`safety::write_atomically`) while every budget is locked, so no tab opens it halfway.
  A review keeps the library as it saw it under a token; adding or saving with a stale
  token is refused ("The library changed since"), and saving also checks that the file
  still holds what the review saw. No backups of the library yet.
- **Open as a tab: refuse, don't route.** When the library file is open as a budget tab,
  the Library tab reads it from that tab (unsaved changes count) and adding from it works,
  but every change from the Library tab is refused with a message (make the change in the
  tab, or close it). That was the simplest safe choice: routing edits through the tab
  would mix file writes and that tab's undo history and autosave; refusing keeps exactly
  one writer per file. Adding into the library's own tab is refused too.
- **The tools window may use it.** The Library tab is part of the panel, so its commands
  (the two dialogs included) are granted in `capabilities/tools.json` too. That widens
  entry 24's "no file commands" by exactly this: the dialogs are Rust's, parented to the
  calling window, and the page never names a path; the only file written is the one the
  user picked as the library.
- **Not yet**: removing an item from the library or editing it there (open the library as
  a budget for that), a target folder chosen while saving, and backups of the library file.

## 38. Apply panel extras and charges and credits polish as built (2026-10-09)

- **Clear per list** (fringes and groups) takes every name off the selected lines that can
  carry it, names left on lines from an edited file included, so a stale name can always be
  cleared (`clear_from_rows`). Sets, locations and currencies already had Clear.
- **Replace tags** reuses a tag preset's replace (`put_tags` in src/presets.rs, shared by
  `apply_tag_preset` and `replace_tags_on_rows`), so the two can never disagree about which
  rows take what. The form starts from what every selected line shares (a fringe ticked only
  when all the lines have it; a set, location or currency only when they all have the same),
  because a tri-state box can't say "leave as is" in a replace. Names it sets must be
  defined.
- **Per-group line count across the budget** was already the Lines column (decision 35), so
  nothing new was built for it.
- **Fringe list foot**: the summary carries the selection's percent and flat fringe dollars
  from Rust (summed by fringe kind from the same per-fringe figures the list shows), so they
  always add up to the fringe total; a name the budget doesn't define counts as percent.
- **Charge label switches are charge content**, stored on the charge (`label_percent`,
  `label_excluded`, left out when off) rather than in display settings: they belong to one
  charge, travel with it when it is copied or imported, and a version keeps them like the
  rest of the charge. Display only: no total changes. The percent shows only for percent
  charges. The label reads "Contingency 10% (excluding 25,000.00)"; the reference shows the
  excluded total in parentheses, and "excluding" keeps it from reading as a negative. Both
  the screen and the printed topsheet use it; spreadsheet export and compare keep the plain
  name, since they match charges by name.
- `set_charge` takes each switch as optional: left out keeps the charge's own setting, so a
  caller that doesn't know about them never turns them off.
- **Live preview** is a read-only command (`preview_charge`) that checks the charge as
  `set_charge` would (`edit::build_charge`) and figures it on the cached report with the
  engine's own charge code (`method_core::charge_preview`), without a recalc: one pass over the
  lines per keystroke batch (debounced 120 ms), exactly what saving gives, never a guess in
  JavaScript. A value that isn't a number yet keeps the old total, dimmed. Only an applied
  charge's heading total updates; an unapplied one still says "Not applied".
- **Exclusion grid**: each group's amount is its counted lines' totals plus their fringes
  unless fringes post by budget, the same rule the engine uses for the base, read from the
  report's active group totals. The grid lists every group; per-group amounts can overlap,
  so the combined "Left out in all" comes from the engine, which counts each line once.
- **Credit cap variance** is computed in the engine (`CreditRowTotals::cap_variance`,
  unrounded): the cap less the net spend for a cap on the spend, the cap less the uncapped
  credit for a cap on the credit; positive is room under the cap. The editor rounds credit
  figures to cents for display, with the exact value in the tooltip, because rows add up
  before rounding (decision 9).
- **Create charge and Create credit** on the topsheet's right-click menu open the tab with
  the add form's name box focused, which is our version of "a new charge named for
  overtyping"; the older Add or apply items stay for turning existing ones on or off.
- **Not yet**: a preview total for an unapplied charge, a credit-wide list of excluded
  fringes (rows still carry their own), and label switches in the spreadsheet export.

## 39. Renumbering accounts, copies for sharing and sub-budgets as built (2026-10-09)

Roadmap 2.20, 2.21 and 2.22, with the calls they needed:

- **Renumbering is offered when the pattern changes, never done silently.** Applying a
  new pattern in Settings > Account numbers first asks for a preview
  (`method_core::renumber::plan`): every category, account and category fringe account
  number now and after. When nothing would change the pattern is applied as before;
  otherwise the form offers "Renumber accounts to the new pattern", "Change the pattern
  only" (numbers keep their spelling, as before) or Cancel. Renumbering sets the pattern
  and every number in one undo step.
- **How a number carries over.** It is split on whatever separator it has, or, with
  none, by the current pattern's widths. Each piece is fitted to its new width: short
  pieces are padded with zeros, long ones lose leading zeros only. Nothing is ever cut
  off, so a piece that still doesn't fit, or a filled part the new pattern has no room
  for, is a conflict. Property-tested: renumbering to another pattern and back gives the
  same budget whenever both steps succeed, and never moves money.
- **Conflicts refuse rather than suffix.** Two different numbers becoming one (`011-001`
  and `011-01` both as `11-01`) or a number that doesn't fit stops the renumbering with
  every conflict listed; the user fixes those numbers or picks another pattern. A suffix
  would invent account numbers nobody chose. Numbers already duplicated are not a new
  conflict. A category fringe account that would share a number with a real account is
  a warning, not a refusal.
- **What follows the numbers**: charges' excluded categories and accounts and their
  account, credits' accounts, report presets' left-out categories and accounts, and the
  baseline's category and account keys, so Orig and Var still line up. A reference to a
  number no longer in the chart is carried over by the same rule (left as it is when it
  doesn't fit), and dropped from exclusions and the baseline if it would start naming a
  live number it never named. Versions keep their own numbers and pattern: comparison
  reads each budget through its own pattern segment by segment, so a separator change
  still matches, but a width change (`11` to `011`) shows those accounts as added and
  removed. Where the user is on screen falls back to the topsheet when the category or
  account on screen was renumbered.
- **A shared copy is a stamped snapshot.** File > Save a Copy for Sharing… writes the
  budget, or one saved version of it (with the current presets and display settings),
  to a new file suggested as `<title> - <version> - <date>.mthd`, optionally without
  notes (lines, accounts, categories, line blocks), budget info, versions, baseline, or
  setup items nothing uses (pruned until none is left, keeping what tag and report
  presets name, so totals never move). The copy gets an additive `provenance` (kind,
  source title, version, saved, app version, note). The budget and its own file are not
  changed; a file open in a tab is refused.
- **Read-only is a request in the file, enforced in one place.** A budget whose
  provenance says `shared_copy` opens read-only: `Session::edit` (and the merging edit)
  refuse every change with one message, so no command can slip past. A banner under the
  breadcrumb says where it came from and offers Make Editable Copy, which opens the same
  budget in a new untitled tab stamped `editable_copy` (still traceable to its source).
  Templates drop the provenance, so a template made from a shared copy is editable.
- **A returned copy finds its original by title.** The copy carries no path or id of
  the sender's budget (nothing about the sender's disk travels), so a shared or
  editable copy matches an open budget whose title is its source title, trimmed and
  ignoring case (never another shared copy, never a sub-budget). When such a copy comes
  to the front for the first time next to its original, the page asks once: Compare
  With Original (the compare screen, the original as the base) or Store as a Version of
  the Original (the copy's content as a new version of the original, one undo step
  there, its note saying when the copy was made). The banner keeps both choices while
  the original is open.
- **Sub-budgets.** File > New Sub-Budget… picks categories and accounts (an account is
  kept when its category or it is ticked; nothing ticked keeps the whole chart), then
  groups, sets and locations: any ticked value within a kind, every kind with a tick, and
  per kind "lines with no ..." (inventory 14). Filters apply to costed rows; subtotal
  and separator rows stay while their account keeps a costed row. Options: leave out
  accounts left empty (and categories left empty), take fringes off every line (the
  fringe table stays), leave out notes. Versions and the baseline are left out (their
  totals describe the whole parent). It opens in a new tab with no file, titled
  after its parent ("Big Show (sub-budget)"), stamped `sub_budget` with its parent's title and a one-line
  description of the choices. Totals are recalculated normally: there is no fringe lock
  in this first version, so percent charges and fringe cutoffs see only what was kept.
- **Not yet**: suffixing conflicts instead of refusing, a sent log of shared copies,
  sharing straight from the Versions dialog, a fringe lock for sub-budgets, and
  matching renumbered accounts across a width change in comparisons.

## 40. Split view and the History list as built (2026-10-09)

Roadmap 2.26 (behavior note 04) and 2.23, with the calls they needed:

- **Two panes, one set of budgets.** Split view is two panes side by side, each with its own
  tab strip, breadcrumb, level buttons, grid and footer totals. Rust keeps which tabs are in
  which pane, each pane's budget in front, and which pane is the target (`Docs::split`); the
  master tab order (the Window menu's) is unchanged, and closing the split merges the panes
  back into it, the first pane's tabs then the second's. A budget can be in both panes: one
  session, so an edit in one pane shows in the other and both share one undo history.
- **The target is the pane last clicked or focused.** The toolbar, the menus, every key, the
  tools panel (docked or popped out) and the formula bar act on it; its budget in front is
  the active document, so every command without a `doc`, and the tools window, follow it.
  It is marked by an accent line along its tab strip and its tab in front in bold; the
  other pane's tab in front is in plain weight. A click picks the target at once; the
  page tells Rust (`set_target_pane`, with that pane's account) and ignores the target in
  replies to earlier commands until Rust has heard.
- **Split Right shows the budget in front in a second pane too**, at the same level and
  account, and the new pane is the target (as editors split: two views of one file). With
  the window split already, Split Right (Shift+Cmd+\\; Cmd+\\ is the Credits tab) goes to the
  other pane. Close Split, Move Tab to Other Pane (also on a tab's right-click menu, which
  also has Close Tab and Split Right or Close Split) and dragging a tab onto the other
  strip do the rest. A pane's only tab can't move (the pane would be empty); closing a tab
  that the other pane also shows only takes it from that pane; a pane left empty ends the
  split. New and Open land in the target pane.
- **Each pane fetches its own account.** The view stays lean (decision 18): commands answer
  with the target's account. When the other pane shows the same budget, it takes the same
  view if it shows no account or the same one, and otherwise asks for its own
  (`pane_view`: the view at its account, changing nothing and telling no other window).
  So an edit costs one more lean view only when both panes show one budget at different
  accounts. Rust's account on screen per budget follows the target; a budget coming to the
  front in a pane remembered at another account asks for that account first.
- **Kept across launches** in `tabs.json` (which files are in which pane, each pane's front,
  the target), by place in its list of files; budgets without a file aren't kept, and a
  pane left with none means one pane. The divider's place is a per-user choice in the web
  view's storage, like the tools panel's size.
- **Every undo step has a label and a time.** `Session::edit` takes a label: what the
  command did, named the way the grid names things ("Edit rate on 21-01 line 3", "Paste 12
  rows", "Rename fringe FICA to Payroll", "Delete 4 lines from 20-01"). Commands that name
  rows work their labels out from the budget before the edit (`edit_budget_as`); a batch of
  cells says what the page was doing (`set_cells` takes `action`: paste, fill, cut or clear,
  known words only). Labels are one line of at most 120 characters. Times are the wall
  clock (the app reads it; the engine still never does). Undo and redo carry a step's label
  and time across.
- **The History tab** (bottom tools pane) lists the target budget's steps oldest first, a
  Start row for the state before the oldest kept step, the current step marked and the
  ones Redo would bring back fainter. A click undoes or redoes to just after that step, as
  one change however many steps it passes. "Version…" on a step saves the budget as it was
  then as a named version without changing it now: the steps are walked on the budget and
  walked back (a patch applied twice gives what it started from), so only the copy is
  made; the save itself is one undo step, like any version save, so it drops steps Redo
  would have brought back. The tools window may list the steps; jumping and saving run in
  the main window, like Undo (decision 24), so the tools window's capability gains only
  `history_list`.
- **Not yet**: more than two panes or a vertical split; per-pane tools panels (the
  reference docks the tools under each pane when split); keeping a step saved as a version
  from dropping the redo steps.

## 41. The scriptable command line and version deltas as built (2026-10-09)

Roadmap 3.12 and 3.4 (decision 17 left deltas for later):

- **One module per subcommand.** `method` is a tiny dispatcher (`src/main.rs`, a table of
  name, usage and function) over `src/cmd/<name>.rs`, with shared flag parsing, exit
  codes and atomic writes in `src/cli.rs`, so new commands (diff, merge, explain,
  actuals) are one file and one table entry each. It depends on `method-core` and
  `method-format` only, never the app, and adds no crates (JSON output is written by hand).
- **Exit codes are a contract** (docs/CLI.md): 0 ok, 1 failed, 2 usage, 3 edit refused
  with nothing written. Every file a command writes goes through a temporary file and a
  rename.
- **Headless reports reuse the export's figures.** `method report` builds sections from a
  saved preset with the same tables the spreadsheet export makes (decision 30), so a
  report and an export never disagree, and filters never change totals (decision 28).
  HTML is one self-contained print-ready page; text is fixed-width; CSV is the sections
  one after another.
- **PDF: our own small writer, not the app's.** The app's PDF is AppKit printing its web
  view (decision 28), which needs a window. A typeset PDF without one would need font
  metrics and layout we don't have, so the command line sets the plain-text layout in
  Courier (a standard PDF font: nothing embedded, every reader has it, and a fixed
  advance keeps figures aligned) on Letter pages with the preset's header, footer and
  "Page X of Y": about 250 lines of std, PDF 1.4, WinAnsi text. Shading and colors stay
  with the HTML and the app.
- **Edits are a line-oriented script, not a query language.** `method apply` runs commands
  (`set global`, `set fringe ... rate|cutoff`, `set line ROW FIELD VALUE`, `add`/`remove
  tag`, `rename`, `apply`/`remove charge`, `set title`, `set compact_versions`) against a
  copy; every line must succeed and the result must recalculate, or nothing is written.
  Rows are named by id (`#17`, stable) or by account and position (`21-01:3`, as the grid
  numbers them; `21-01:*` for every line). `method set` is one such command from a path
  (`line.21-01:3.rate`). Renames call the engine's own rename (decision 32). `remove
  charge` stops a charge counting rather than deleting it, so it can be applied again.
  `--dry-run` prints `method diff`'s summary of what would change.
- **Import reuses the app's importer** (decision 36) with a small JSON mapping file
  (sheet, header row, field-to-column by letter or number, the two cautious switches);
  without one, the header is found and columns guessed as the dialog does.
- **Version deltas are opt-in per file.** FORMAT.md's rule is additive-only within
  version 1 and strict readers: a new field must be left out at its default so older
  readers keep working. So deltas sit behind a budget-level `compact_versions` switch, off
  by default: files without it are byte-identical to before, and an older reader shown a
  compact file refuses it cleanly (unknown field) rather than misreading it. A format
  version 2 was rejected: it would make every file unreadable to older builds for a
  feature most files don't need.
- **Deltas are JSON Patches between neighbors.** Each version is an RFC 6902 patch
  (add, remove, replace) against the version after it, the newest against the budget
  itself, computed on the canonical JSON (common prefix and suffix, then a longest common
  subsequence of array elements, edited elements patched inside when that is smaller).
  A standard patch means any tool can rebuild versions without our code. The writer keeps
  a full copy wherever a delta isn't smaller. Reading rebuilds every version as a full
  `Arc<BudgetSnapshot>` and reads the patched object as strictly as a full version, so
  the rest of the program never sees a delta; a patch that doesn't fit is an error naming
  the version. Ten versions of the 5,000-row sample: 26 MB in full, 2.0 MB as deltas
  (the budget alone is 2.0 MB).
- **Not yet**: a switch for `compact_versions` in the app (the CLI's `method set ...
  compact_versions true` turns it on; the app keeps it once set); the PDF's proportional
  fonts, shading and the topsheet page column; scripted adding and deleting of rows and
  accounts; a per-command JSON output for `set` and `apply`.

## 42. The Budget Setup window and a compact tools panel (2026-10-09, the owner)

The owner's live testing found the panel's Budget tab (charges, credits, info, settings,
baseline) too big for a pane, and asked to match the reference product's balance: one setup
window with the full editor of every setup table, and tool tabs in the main window made
small and quick for applying (behavior notes 07, 09, 12, 23, 24, and the 2026 notes 01 and
06, which win on the pane). As built:

- **One Budget Setup window**, a sibling of the tools window (decision 24): a second web
  view labelled `setup` on the same page (`index.html?panel=setup`, `setupwin.js`), made in
  Rust (`src/setupwin.rs`) with a fixed page, calling the same commands on the same budgets.
  It follows the budget in front (the target pane's when split): its commands name the
  budget it drew, and `view-changed` reaches it like any window, so it refreshes on edits
  made elsewhere and when another tab comes forward. The main window owns its tab, place and
  size (per user, in the web view's storage) and tells it what to show (`setup-state`). A
  sibling window rather than a mode of the tools window, because both can be open at once
  and a window's capability is tied to its label: one more label, one more capability file,
  and the tools window's code untouched but for its Setup buttons.
- **Its tabs**: Fringes, Groups, Globals, Units, Sets, Locations, Currency, Charges,
  Credits, Info, Settings and Baseline, each the full editor, reused rather than rewritten.
  The five tag lists are `apply.js`'s lists drawn as setup tables (every column of the
  definition: a fringe's description, rate, unit and cutoff; a group's color and whether it
  counts; a location's place; a currency's rate and symbol), with the same in-place
  editing, adding, deleting, renaming, ordering, short codes and in-use counts, a footer of
  the budget's percent and flat fringe dollars, and no selection, so the summary is never
  fetched there and nothing is applied. Globals and Units are `setup.js`'s full tables
  (a global's details form included); the rest are `topsheet-setup.js`'s, renumbering
  preview included. The setup library sits beside each table (`library.js`, set to the
  table's kind) when the window is at least 860 pixels wide. Budget folders are not in this
  step: the budget has no folders for items yet (the library's are shown in its rail).
- **Ways in**: a Setup button on the toolbar, View > Budget Setup (Open Budget Setup and one
  item per tab), and the setup keys long-time users know (Cmd+3 to Cmd+9, Cmd+G, Cmd+/,
  Cmd+\\), which now open Budget Setup on their tab from any window instead of a panel tab.
  The topsheet's Create charge and Create credit open it on that tab with the new item's
  name box focused; Set up fringes and groups on the grid's menus open it on that table.
  Escape (when no form, menu or uncommitted typing wants it) and Close Window or Close Tab
  close it, and the main window takes focus back into the grid.
- **The panel is for applying and browsing.** The Budget tab is gone (a saved layout that
  had it simply loses it). Fringes, Groups, Sets, Locations and Currency keep their
  checkboxes, selection dollars, line counts, filter, Applied only and Clear, now show a
  fringe's or currency's rate as the key figure (the description moved to the tooltip),
  and gain a Setup button and an Edit in Budget Setup item on the right-click menu. Their
  double-click quick editor stays (decision 22 already covers editing where an item is
  listed; it is a small form, not a bulky one). Globals stay editable in place (formula,
  rename, delete, add) without the details form; Units show their plural and hours as text
  (edited in Budget Setup). Presets, Find, Notes, History and Library stay as they were:
  they act on the selection, the grid or the library, not on a table's definitions, and
  are already compact. The panel's own tabs are listed under View > Tools Panel, without
  keys.
- **Strict capabilities.** `capabilities/setup.json` grants the setup window exactly what
  its scripts call, minus the apply half of `apply.js` (a test checks both directions and
  that setup mode never fetches a summary): the setup tables, charges, credits, info,
  settings and renumbering, the baseline, and the library's commands. No selection edits,
  notes, find, history, files, undo, tabs or windows; its other keys go to the main window
  as menu commands. The tools window lost the commands only the Budget tab used
  (charges, credits, info, settings, renumbering, the baseline, the numbers-on-screen
  settings and a global's details), since the panel no longer calls them.
- **Not yet**: the budget's own folders for setup items (the reference's folder rail inside
  the budget); per-item selection inside a table for Move To and bulk delete; a line count
  click in Budget Setup brings up Find in the panel but leaves the setup window in front.

## 43. Explain this number and the overtime helper as built (2026-10-09)

Roadmap 3.5 and 3.10.

- **Explain this number** (`method_core::explain`, src/explain.rs, ui/js/explain.js,
  `method explain`). Any total (a line, a subtotal row, a line's fringes or one fringe on it,
  an account's total, lines or fringes, a category and its fringes, a production level and
  its posted fringes, ATL, BTL, both, total fringes, each charge, the grand total, each
  credit, all credits, the net total, one fringe across the budget) gives a tree of steps.
  A **sum** adds the steps under it; a **value** is worked out from the notes under it;
  a **note** explains and counts for nothing; a step can be **less** (credits under the
  net total). The tree is **worked out again from the budget**, with the engine's own
  pieces (formula evaluation, `earners`, `allocate`, `percent_of`, `mul_cents`), not copied
  from the report, so a test can hold it to the report: on every total of the 5,000-line
  sample, of smaller samples under every posting mode with exclude and only filters, and of
  the hand-checked fixture, the signed sum of the leaves equals the number shown, and every
  sum adds up. The explainer recalculates first (errors are the engine's), then works out
  every line once; any number of totals are explained from that.
- What it says: each line as amount, X, 4X and rate with globals by name and value
  (`SHOOT_DAYS + PREP_DAYS = 40 (PREP_DAYS = 10, SHOOT_DAYS = 30) Days`), the quantity, the
  product, the currency conversion and **every rounding** shown as unrounded to rounded,
  half away from zero; an adjustment's base with the lines in its window; a left-out line
  with the group (In budget total off) or report filter that left it out and what it would
  be; each percent fringe with the earner's earnings, the cutoff, for a range the lines that
  share it and **how much of the cutoff each line used** (the capped base split pro rata by
  largest remainder, the same split the engine gives the fringe), the exact share and the
  allocated cents (amended by entry 56: the cutoff left after the lines above, used up in
  line order, each line rounded on its own); each flat fringe once per line or per unit with the hours conversion,
  and why it fell back to once; where fringes post, wherever that decides what a total
  holds; a charge's base, each excluded account with the rules that caught it (each line
  counted once) and typed amounts; a credit's rows (gross with the accounts it came from,
  excluded fringes, net, percent, caps), the rows added unrounded, the overall cap, the
  discount and the one rounding.
- **On demand only.** Nothing runs on an edit; RecalcCache and the reference engine are
  untouched. In the app the command explains from the budget the totals on screen came
  from (the sanitized copy when cells have problems, said as partial). What crosses the
  bridge stays small: a step that can be explained on its own sends its children only near
  the top (depth 2); deeper ones, and a note that points at another total (a line's
  fringes, a charge's base), arrive closed with `more` and are fetched when opened. A grand
  total of 5,000 lines sends under 400 steps.
- In the app: **Explain This Number…** first in the right-click menu on any money cell at
  every level, total rows the cursor can't reach included (grid.js `peek`), Edit > Explain
  This Number…, and **Cmd+E** (Ctrl+E) on the cursor cell (free in the shortcut list and
  the menu bar). It opens the **Explain** tab (first pane, after Find; saved layouts get it
  added), a collapsible tree with plain-words lines and amounts; a click goes to the
  step's line, account or category (the notes and find jump); keys as a tree. The tree is
  asked for again when the budget changes, keeping what was open. With the panel popped
  out the target goes with the panel's state. `method explain file.mthd <total>` names totals
  as `21-01`, `21-01/lines`, `21-01/fringes`, `21`, `21/fringes`, `21-01:3`, `#ID`,
  `level:`, `charge:`, `credit:`, `fringe:`, `atl`, `btl`, `atl+btl`, `fringes`, `credits`,
  `grand` (or `topsheet`), `net`; `--depth N` cuts the printout.
- **Overtime helper** (`method_core::overtime`, src/overtime.rs, ui/js/overtime.js). User
  driven, no union or guild rules shipped. A pattern is straight-time hours, then bands
  ("4 hours at 1.5x", the last band "the rest at 2x"; stored as hours per band, so there is
  nothing to keep in order), and optional sixth and seventh day multipliers. On a sixth or
  seventh day every hour is paid at least the day's multiple and a band above it still
  wins (`max`), which covers both "time and a half all day" and "double time after 12 on
  the sixth day" without more settings. Hours past a last band that has hours keep its
  multiple. Inputs: straight-time hourly rate, hours a day (more than 0, at most 24), days a
  week (1 to 7; days 6 and 7 get their multipliers when set, else they are regular days).
- **Exact decimals, rounding once per line.** Pay is hours x rate x multiple in `Dec`,
  never rounded by the helper. Writing **as the rate** puts the day's (one regular day) or
  the week's pay into the line's rate as the exact decimal, so the line rounds once as
  every line does. Writing **as split lines** makes one line per multiple, each keeping the
  line's amount, units, X, 4X, currency and tags with its part of the pay as its rate (each
  line rounds once), the first keeping the line's id and notes, new ids after the highest
  in the budget, and all of them in **one fringe range** (the line's own, else the next
  free number) so percent fringe cutoffs still treat them as one person (tested: the
  category total matches the single-rate line). One multiple only: the rate is written.
  Either way one undo step.
- **Patterns** are saved in the budget by name (`ot_patterns`, docs/FORMAT.md, "Overtime
  pattern"; additive, left out when empty), checked when read, tools not content like tag
  presets: versions don't carry them, restoring keeps them, a version that has them is
  rejected, a shared copy of a version takes the current ones. The tab follows the cursor
  line, previews as you type (`ot_calculate`), and saves, updates and deletes patterns.
- New commands `explain_number`, `ot_calculate`, `ot_apply`, `ot_save_pattern`,
  `ot_delete_pattern` are in build.rs and both capabilities (the tabs can be in the tools
  window); the panel test's script list includes explain.js and overtime.js.
- **Not yet**: explaining group, set and location totals, the selection footer, or the
  Orig and Var columns; a pattern applied to many lines at once; per-day schedules (a week
  of different day lengths); meal penalties, turnaround and minimum calls (they are rules a
  user could want, but each needs its own design).

## 44. Budget diff and three-way merge as built (2026-10-09)

Roadmap 3.1 and 3.2 (docs/MERGE.md), with the calls they needed:

- **Identity, never position.** Rows match by id; accounts and categories by number, then
  (a renumbered one) by the rows or accounts they hold, then by name within a matched
  category; production totals, setup items, charges and credits by name (the n-th of a
  repeated name with the n-th); versions by id and saved time. A renumber reads as
  renumbered, not as an account removed and another added. A row that changed place in
  its account is "moved" only when it falls outside a longest common run of rows (a
  longest increasing subsequence), so one row dragged up doesn't mark every row it passed.
- **Words come from the engine.** Each change carries its fields before and after and a
  sentence in the grid's terms (`21-01 line 3: rate 1,200.00 -> 1,350.00 (+1,500.00 to
  21-01)`, line numbers as the `#` column counts); rates read as money, formulas as
  typed, tags as `+FICA -WGA`, notes as counts. The CLI, `method apply --dry-run` and the app
  print the same sentences.
- **Money comes from totals, not from the changes.** Both budgets are totalled and every
  account, category fringe account, category and topsheet line that moved is listed. The
  engine recalculates (`diff`), or takes the caller's reports (`diff_with`), so the app
  passes its lenient totals and a budget with a bad cell still shows money. A budget that
  can't be totalled leaves the money out and says so.
- **Tools aren't content**, as for versions: display settings, presets, setup order and
  folders, overtime patterns, compact versions and the provenance are not listed by a
  diff. Actuals are, in a few words. In the app, a version compared with the budget borrows
  the budget's versions and actuals, which a version never holds.
- **Merge granularity: field by field.** Different rows, different fields of one row and
  different fields of one setup item merge on their own; tag sets and a charge's
  exclusions merge name by name; notes note by note. The same field changed both ways is a
  conflict; so is deleted on one side and changed on the other.
- **A container deleted on one side and changed on the other is one conflict**, not one
  per row: keep it (as the side that changed it has it, its rows included) or delete it
  (with everything in it, except rows the deleting side moved elsewhere first). Asking row
  by row would offer half an account.
- **Conflicts take ours by default, every one listed with base, ours and theirs.** Keys
  are stable for the same three budgets, so the app shows the conflicts, the user picks per
  item and the merge is worked out again with those picks. "Ours" is the budget being
  merged into: the file the user owns.
- **New rows with one id are two rows.** Each copy numbers new rows from the highest it
  has, so independent additions usually collide. Identical rows (same fields, same
  account) are one; otherwise theirs get fresh ids above every id in all three budgets
  (versions included, as decision 17 requires). Versions saved on both sides the same way.
- **Order: whoever reordered wins**; what the other side added goes after the row or
  account it followed there. Both reordering one list differently is a conflict (pick an
  order), never an interleaving.
- **The provenance is always ours** (it is about the file being merged into); settings and
  tools merge as whole values. The merged budget is recalculated, never summed from
  deltas, and a merged budget that can't be totalled while both sides can is reported.
- **Property-tested** (random budgets and edits): diff(a, a) is empty;
  merge(base, x, base) = merge(base, base, x) = merge(base, x, x) = x; edits that don't
  overlap merge cleanly, commute, and equal applying both edit sets one after the other
  (so the merged totals are that budget's recalculated totals); any two edit sets merge
  into a budget with unique row ids and every conflict can be picked either way. 4,000
  cases pass; the suite runs 300. A 5,000-row diff or merge takes about 30 ms with the
  files read.
- **A returned shared copy merges against what was sent.** The copy's provenance names
  the version it was shared from (decision 39); that version is the base, but first worked
  out as sent: what the copy left out isn't recorded, so it is read from the copy (no base
  note in it means notes were left out; likewise info, the baseline and unused setup), the
  base's versions are those both the copy and the original still have (an original's
  deleted version can come back; a reviewer's never goes), and its tools are the copy's.
  A copy saved from the budget as it stood has no version to go back to: the user picks
  the file that was sent (or a version, or an open budget).
- **git**: `method merge-driver %O %A %B %P` writes a valid budget over ours even with
  conflicts (settled for ours, listed on stderr) and exits 1, so git marks the file
  conflicted without conflict markers that would make it unreadable; it is opened in the
  app and settled there. An empty base (added on both branches) is refused. `method merge`
  exits 1 when conflicts were settled (its own code under decision 41's contract), and
  writes the file either way.
- **In the app**: File > What Changed… (against the saved file, a version, an open budget
  or a file; a click goes to the row's cell, the account, the category, the setup tab or the
  Versions dialog) and File > Merge Changes From… (the copy, the base, a preview with a
  side to pick per conflict, one undo step worked out on the budget as it is then). Both
  reuse the compare screen's frame and its file tokens; four new commands, main window only.
- **Not yet**: merging from a returned copy's banner; matching renumbered accounts across a
  width change by what they hold when they hold no rows (they match by name instead);
  merging actuals transaction by transaction (they merge as a whole); a three-way view of
  one row.

## 45. Actuals and the cost report as built (2026-10-09)

Roadmap 3.7 (inventory 24 and "Gaps and pain points": no actuals or cost reports in the
reference), with the calls it needed:

- **Actuals live in the budget, but aren't its content.** `Budget::actuals` holds the
  transactions (date, account number, optional line, tags, vendor, description, reference,
  amount in cents, the import it came with), the import batches (file name, when, how many)
  and per-account estimate-to-complete methods. Like presets and the setup order they are
  not version content: versions never hold them and restoring a version keeps today's, since
  money already spent doesn't change with a budget revision. Templates and sub-budgets leave
  them out (they describe one production, like the baseline); Save a Copy for Sharing can
  leave them out (vendor payments are often private); renumbering accounts carries
  transactions and overrides with their accounts by the same rule as the baseline (a
  transaction on a number the chart doesn't have keeps its spelling when it would start
  naming a live account).
- **Copies stay cheap.** The actuals are an `Arc` in the budget and each transaction an
  `Arc` too: copying a budget for an edit's undo costs one pointer, a changed copy keeps the
  unchanged transactions shared (so 200 undo steps of transaction edits hold pointers, not
  200 ledgers), and the undo patch's comparison stops at equal pointers.
- **Edits stay instant (decision 18).** The cost report needs one pass over every
  transaction (summing them by account and line) and one over the accounts. The session
  caches the first, keyed by the actuals' `Arc` (kept alive by the cache, so a pointer
  match can only be the same actuals) and the account pattern; a line edit only redoes the
  pass over the accounts. Measured (release, 5,000-row sample, 20,000 transactions): a
  one-cell edit with its view takes 4.31 ms against 4.23 ms without actuals, the rollup is
  never redone during line edits, the cost report from a rollup takes 0.04 ms. The view
  carries the cost figures of every account (about 21 KB for 80 accounts) and the line sums
  of the account on screen only; the transactions themselves never go out with a view (the
  Actuals tab asks for a filtered page of them).
- **The figures.** Budget is each row's total as the topsheet shows it. Actual is the sum of
  the transactions on the account's number, compared as the budget's pattern writes it
  (`1101` is `11-01`); when two accounts share a number the first in the chart gets them, so
  nothing counts twice. ETC is per account: budget minus actual, never past zero in the
  budget's own direction (an account over budget has nothing left to spend; a negative
  budget is a credit still to come), a manual amount, or percent complete (EAC = actual /
  percent, rounded once half away from zero, the one division in the module, done in i128
  by `decimal::whole_from_percent`). EAC = actual + ETC, variance = budget - EAC (negative is
  over), percent spent = actual / budget to two places (`decimal::percent_ratio`, rounded
  once; none for a zero budget). Categories, levels and the totals add up their parts'
  figures in integer cents, so every column sums exactly (property-tested over every posting
  mode with random transactions and overrides). Category fringe accounts are rows of their
  own (transactions can be coded to `11-99`); fringes posted by budget or level and charges
  have no account to code to and keep their whole budget to complete; transactions whose
  number the chart lacks (or have none) are the unassigned actuals, in the total actual with
  nothing to complete. Credits are left out: a cost report is gross spending.
- **Importing reuses the spreadsheet readers**, with its own field list and mapping
  (`method_format::import::actuals`): date, account, amount or debit and credit, vendor,
  description, reference, tags. Dates take every shape accounting exports write (workbook
  serials in the 1900 system, ISO, compact, slashes, dots, month names); slash dates are
  read month first unless the column has a date only day-first can read (or the user says
  so). Account numbers that don't fit the pattern are kept as written and listed, never cut
  short. Unmatched numbers default to **keep unassigned**, not skip: nothing is lost, the
  cost report shows them apart, and adding the account later picks them up. A row with no
  date and no account, or a "Total..." label and no date, is a total row and skipped, since
  importing a report's totals would double the actuals.
- **Duplicates by date, account, amount and description** (trimmed, spaces collapsed,
  capitals ignored), the brief's key. Rows matching a transaction already in the budget are
  skipped by default, so importing the same file twice adds nothing; rows repeating an
  earlier row of the same file are imported by default, since two equal purchases on one
  day are real. Both are listed in the review with a switch each.
- **One import, one undo step, one batch.** The batch keeps the file's name and the page's
  local time; removing it takes out every transaction still carrying it, edited ones
  included (the confirmation says so).
- **On screen like Orig and Var.** Actual, ETC, EAC, EAC Var and % Spent go before Total on
  the topsheet and the account list, shown by default once there are actuals and hidden or
  shown one by one from the column menu (decision 27's convention); lines get Actual (the
  transactions assigned to them) and the account's total row its whole actual. Over budget
  is the EAC variance in the danger color on screen and bold on paper. The Actuals tab is a
  tools panel tab (its commands are the tools window's too; importing stays in the main
  window's File menu, like Import Spreadsheet); its estimate-to-complete form follows the
  grid's cursor account.
- **Printed and exported.** A Cost report section in the report setup (accounts left out of
  a report leave the listing, never the totals, decision 28) and, for a budget with actuals,
  Cost Report and Actuals sheets in File > Export and `method export` (both after the ten
  standard sheets, so positional readers keep theirs).
- **Not yet**: entering a transaction by hand, ETC overrides on charges, purchase orders and
  commitments, actuals by period (a cost report as of a date), matching a re-import whose
  unmatched numbers were mapped differently the first time (the duplicate key compares the
  account the transaction ended up in).

## 46. The product is Method Budget (2026-10-09, the owner)

methodbudget.com is the site, and the repo is https://github.com/ssandweiss/method-budget.
Never shorten the name to "MB" (that is how people shorten Movie Magic Budgeting), and no
file extension of ours resembles `.mbd` or `.mmbx`. What it covers:

- **User-visible name everywhere**: window title, macOS app menu (About and Quit Method
  Budget), the About box (links methodbudget.com), dialogs, toasts, report and PDF metadata,
  workbook metadata, CLI help. Internal names follow it: the crates are `method-core`,
  `method-format`, `method-cli` and `method-app` (the app's executable is
  `method-budget`), the UI's document events are `method:*`, its localStorage keys start
  with `method.`, and the report styles hang off `.method-report`.
- **Files are `.mthd`**, media type `application/vnd.method-budget+json`, with
  `"format": "method-budget"` (`method_format::FORMAT_NAME`). That is the only extension
  and the only identifier read or written: there is no compatibility code for any earlier
  working name, extension, identifier or data folder (removed 2026-10-10). New files, Save
  As, shared copies, templates, sub-budgets, backups and recovery copies are all `.mthd`.
- **The library**: a new one is `My Method Budget Library.mthd`.
- **The command line is `method`.**
- **App identity**: product name Method Budget, bundle identifier `com.methodbudget.desktop`
  (not `.app`: Tauri warns against identifiers ending in `.app` on macOS, where it is the
  bundle extension). The local data folder follows the identifier.

## 47. One feature file for the README and the website; versioned skills (2026-10-09)

- **Single source.** The public feature list lives in `docs/features.toml` (TOML, read
  with Python's standard `tomllib`; no new tooling). `scripts/site.py` writes it into
  README.md and site/index.html between marker comments, writes the site's "What's new"
  from the newest `docs/releases/<version>.md`, and points the site's GitHub links and the
  README's clone command at `[project] repo`, so a repository move is one line.
  `--check` runs in CI (`site` job) and before every Pages deploy.
- **The list can't overclaim.** A feature marked `have` must name roadmap items that are
  `[have]` in docs/ROADMAP.md, or the script refuses; decision references must exist.
- **The site** is one static page: inline CSS and SVG, system fonts, no JavaScript, no
  third-party requests (enforced by a Content-Security-Policy), light and dark tokens,
  AA contrast. Deployed by `.github/workflows/pages.yml`, which stays off until the
  repository is public and the variable `PAGES_ENABLED` is `true` (Pages on a private
  repository needs a paid plan). Turning it on and the DNS: docs/WEBSITE.md.
- **Skills are versioned.** `.gitignore` ignores `/.claude/*` except `skills/` and
  `settings.json`, so `/release` and `/features` travel with the repository into every
  worktree, while `.claude/worktrees/` and `settings.local.json` stay out.
- **Hooks are shipped, not switched on.** `scripts/hooks/require-release-notes.sh`
  and `require-site-regen.sh` are tested in CI; wiring them into
  Claude Code settings is left to the maintainer (`scripts/hooks/claude-settings.example.json`).

## 48. Updates inside the app, one version, release notes, signed Mac releases (2026-10-10)

- **The app updates itself.** `tauri-plugin-updater` (2.12.0, rustls only), driven from Rust
  (src/updater.rs): a check two minutes after start and every six hours, a background
  download whose signature is checked against the public key built into the app, a quiet
  "Update ready: Restart to update" in the toolbar, and Restart to update saving every
  budget (Autosave's rules, so the outside-change guard holds; Save All, Discard All or
  Cancel for what can't be autosaved) and reopening the tabs and split view. Never a dialog
  while working. Help > Check for Updates checks at once and is the only thing that says
  anything about a failed check; Help > Check for Updates Automatically turns the timed
  checks off. No window is granted the plugin's commands; the page has four narrow
  commands of ours.
- **This is the app's first network access**, and its only one. Decision 14's "works
  offline" still holds: nothing needs the network, and the web view still has none (its CSP
  is unchanged). The updater's client is https-only and stays on the endpoint's host (plus
  GitHub's download hosts). Documented in SECURITY.md and docs/APP.md.
- **Hosting is GitHub Releases**, the endpoint written once
  (`plugins.updater.endpoints` in `tauri.conf.json`). While the repository is private every
  check fails quietly; updates start once it is public. The manifest (`latest.json`) is
  built by `cargo xtask update-manifest`, which refuses malformed signatures, a key the app
  doesn't trust and unknown platforms. Updater archives are made for macOS only for now
  (Windows and Linux builds are untested).
- **Method Budget has its own updater key**, generated with `cargo tauri signer generate`,
  kept in the team 1Password vault and the repository's Actions secrets, shared with no
  other app (whoever holds it can ship code to every installed copy). Until it exists the
  app carries a placeholder and never checks. `requireSignedVersion` stays off because the
  pinned Tauri CLI (2.11.4) doesn't record the version in its signatures yet.
- **One version.** `[workspace.package] version` in the root Cargo.toml; every crate
  inherits it, `tauri.conf.json` and Cargo.lock copy it, `method --version` and the About
  box show it. `cargo xtask check-version` (CI) fails on any drift or on a version without
  notes; `cargo xtask bump` changes every place at once or none, and refuses on a dirty tree.
  The release tooling is Rust with tests (`crates/xtask`, `crates/method-release`), not
  workflow script.
- **Release notes** are `docs/releases/<version>.md` in the shape of
  docs/release-notes-template.md, written for people who budget films. The app embeds its
  own version's notes at build time and shows them once after an update (What's New) and
  from Help > Release Notes, as plain text. A tag whose notes aren't ready builds nothing.
  `cargo xtask notes-facts` lists the commits since the last tag for drafting, treating
  their subjects as untrusted data.
- **Mac releases are signed as Clockwork Post Production, LLC** with its existing
  Developer ID Application certificate, notarized with an App Store Connect API key,
  stapled, and checked with `codesign` and `spctl` in the release workflow. Each step skips
  cleanly when its secrets are absent (the build is then ad-hoc signed, as before).
  Roadmap 1.21 is built as a pipeline; the first signed release needs the secrets
  (docs/RELEASING.md).

## 49. Brand system, a multi-page static site, and generated brand files (2026-10-10)

- **The mark is a budget page with its corner turned back**: right-aligned figures and the
  accountant's double rule under the total, cut out of the page so it sits on any
  background. It replaces the film-frame-and-bars app icon and the three-rows favicon.
  Drawn on a 64-unit grid, with a separate 16-unit drawing for favicons so it stays sharp
  at 16 pixels. No wands, stars, clapperboards or letterforms, nothing that echoes another
  product's mark.
- **The wordmark is drawn, not typed**: lowercase `method budget` from strokes on a
  100-unit x-height (geometric, one weight), so it looks the same everywhere and ships no
  font file. `scripts/make-brand.py` (standard library only) draws the mark and wordmark
  SVGs, `favicon.svg`, `favicon.ico` and `apple-touch-icon.png`; `--check` runs in CI and
  before a Pages deploy, comparing PNG pixels rather than bytes so zlib differences between
  machines don't matter. `make_icon.py` draws the app icon from the same geometry (white
  mark on a teal tile); the bundle's sizes and formats are unchanged.
- **The accent is the app's teal** (`#0E7468` light, `#2CC7B2` dark), on warm paper and
  ink, so the site and the app read as one product. The indigo explored during naming was
  dropped for that reason. Tokens live in `site/assets/tokens.css`; every text pairing
  meets WCAG AA in both schemes (docs/BRAND.md lists the ratios).
- **System fonts only**, as in the app: nothing downloaded, no font license to carry.
  A serif wordmark (an OFL face, self-hosted) was the alternative; drawing the wordmark
  made it unnecessary.
- **No JavaScript on the site at all**, not even a theme toggle: the site follows
  `prefers-color-scheme`. CSS moved to files so every page's CSP is
  `default-src 'none'; style-src 'self'; img-src 'self'`, with no `'unsafe-inline'`.
- **The site is several static pages**: home (hero, an HTML illustration of the app
  window with an invented budget, What's new, features, open files, keyboard, command line
  and coding agents, download, FAQ), a release notes page generated from every
  `docs/releases/*.md`, a 404 page, robots.txt and a sitemap. `scripts/site.py --files`
  lists the files it generates, and the `require-site-regen` hook reads that list instead
  of naming the files itself.
- **Brand rules are written down** in docs/BRAND.md: always "Method Budget", never
  initials; plain, specific voice with no hype and no em dashes; other vendors named only
  in one factual non-affiliation line.

## 50. Setup packs, where setup items came from, and setup lists in spreadsheets (2026-10-10)

Sets of setup items to share and start from, built on Import Setup (entry 29), the library
(entry 37) and the paste review's rules (entry 23). As built:

- **A setup pack is a budget file, stamped.** No new file type: a `.mthd` with setup and no
  lines whose `provenance` has the new kind `setup_pack` and a `pack` with its details
  (title, description, author as free text, version, effective date, region, source notes
  and links). Putting the details in the provenance kept the change to one optional field
  on an existing object instead of a new budget field; a pack is never read-only, is never
  "a copy of" another budget for compare or store-as-version (`share::came_from` says no),
  and opens, edits and saves like any budget. The three budget settings never go in a pack.
- **Where an item came from is its own field.** `setup_sources` (kind, item name, then pack
  title, pack version, effective date, notes, links) rather than text stamped into an item's
  description: a description is part of the definition, so stamping it would make every
  re-import of the same pack read "different here" and would change what prints. Like
  `setup_folders` it is about the items, not content: versions don't carry it, restoring
  keeps it, Rename everywhere carries it, a writer keeps only items that exist. Every copy
  between budgets (Import Setup, the library, packs) brings an item's source along and
  replacing an item replaces its source; importing a pack then stamps what came in with the
  pack's title, version and effective date (an item's own effective date wins).
- **The import logic moved to method_format::setup** so the command line can share it: the
  dependency walk, statuses, conflict choices and renaming (`setup::import`, from
  src/import_setup.rs) and the setup half of the row clipboard (`setup::copy`, from
  src/clipboard.rs) moved verbatim, and the app re-exports them, so the app's behavior and
  tests are unchanged. method-format already held the spreadsheet import, so this added no
  crate and no dependency. While there: overtime patterns became an importable kind
  (Import Setup, the library and packs), and a global's description, unit and display
  settings now come with it.
- **Export builds by importing into an empty budget**, so a pack's contents are exactly
  Import Setup's closure of the picks, with the source's folders and item sources kept. The
  page lists items by kind and folder with a box for each (dependencies ticked along as in
  the review); Rust walks the closure again, checks the details (a title; dates
  `YYYY-MM-DD`; links `http://` or `https://` only, shown as text and never opened) and
  writes atomically after its own Save dialog, refusing a file open in a tab.
- **Import is Import Setup's review** with the pack's details on top and each item's source
  under it, into a budget (one undo step) or the library (its checked write, entry 37). The
  pack commands are allowed in all three windows because the Library tab, where packs are
  listed, is in each; the spreadsheet commands are the main window's File menu only.
- **Where packs live**: the user's `Packs` folder in the app's local data folder (made when
  first listed, so dropped-in packs appear; Show Packs Folder opens it) and the repository's
  `packs/` folder, bundled as a resource. That folder ships empty but for a README: rate
  packs come from a separate data project after review (CLEAN-ROOM.md). A listed pack is
  named to Rust by a token from the last listing, never a path.
- **Setup lists in spreadsheets** reuse the export writers and the import readers: a sheet
  (or CSV file) per kind with each item's whole definition and its source columns; reading
  maps columns by title per sheet, checks every row and lists every problem with its sheet
  and row, imports nothing while any is left, and hands a clean read to Import Setup's
  review. A charge's excluded amounts, line blocks, report presets and the settings stay
  out of sheets. Into a pack: open the pack as a tab and import there.
- **The command line**: `method pack export`, `import` (`--ours` by default, `--theirs`,
  `--dry-run`) and `show`; `--created` defaults to today's UTC date, the one place the
  command line reads the clock for a stamp.
- **The source shows** in the review, the spreadsheet export, `method pack show` and the
  Budget Setup window's tooltips for fringes, groups, sets, locations, currencies and
  globals. **Not yet**: editing an item's source in the app, clearing a stamp when its
  figure is edited by hand, removing a source when an item is deleted (the writer drops it;
  until saved, an item re-added under the same name in the same session would show it).

## 51. Charts of accounts: importing any chart, and bundled chart templates (2026-10-10)

Roadmap 3.16, under CLEAN-ROOM.md's exception for studio charts of accounts:

- **A chart is numbers, names and levels.** `method_core::coa::Chart` holds categories,
  accounts and production totals in topsheet order with the numbering pattern they are in,
  and nothing else. It becomes a budget with no lines or setup (`to_budget`, our default
  units only) or is applied to a budget (`apply`). Every other column of a source file is
  ignored, so no description, note, fringe, global or rate can come in by accident.
- **One importer, many shapes** (`method_format::import::coa`, on the readers of decision 36):
  a flat list, a one-column or indented outline, side-by-side category and account columns
  (blank category cells below the first, as merged cells read), number and name in one
  cell; numbers with or without separators; rows above the header, repeated headers, total
  rows and level headings. Columns are guessed from titles and, failing that, from the
  cells (the leftmost column of numbers; a second one is the account column when its
  numbers start with the first's or seldom share a row with it). A category written
  `1100` over accounts `1101` is category `11`, since our categories are the category
  segment (decision 11); `101-00` with a separator is category `101`.
- **The pattern is inferred, never assumed.** The commonest separator sets it; widths come
  from the parts, or without separators from the smallest category width whose prefixes
  cover at least half the accounts (so a stray account with no category row doesn't throw
  it off). Numbers one character short of the others of their kind are taken to have lost
  a leading zero in a spreadsheet and get it back, said in the preview. The user can ask
  for another pattern; numbers are carried over as renumbering does (decision 39) and a
  clash refuses.
- **The owner's studio-chart layout is first class.** A header with `kind`, `code`,
  `name` (and `parent`) switches to a reader that takes each row's kind as given:
  categories, accounts, sub-accounts and totals, codes as written. A budget has no
  sub-account level, so sub-accounts become accounts of their own numbered after their
  account (`1102.01` under `1102` as `110201`, a sub-account width added to the pattern)
  and the import says so. A total right after another total (`TOTAL BELOW-THE-LINE` after
  `TOTAL OTHER`) sums totals, which our production totals can't express (each subtotals the
  categories since the previous one), so it is left out with a note.
- **Levels.** Headings and totals with the usual level words become production totals
  with our names (`Total above-the-line`, `Total production`, `Total post`, `Total other`);
  others become `Total <heading>`. A chart naming none gets ours by the category number's
  first digit (1 above the line, 2 and 3 production, 4 post, the rest other), offered as a
  switch, never when all categories fall in one group.
- **Applying never loses work.** Missing categories and accounts are added in number order
  (a new category after the last one numbered below it, so it stays inside its level),
  existing ones renamed by number, production totals untouched. Accounts the chart doesn't
  list are kept and listed; the opt-in removal takes only accounts with no lines and no
  notes, then categories left empty. One undo step; property: the grand total never moves.
- **A new budget from a chart starts unsaved** (the import is its first undo step, as a
  sheet import into a new tab), while a bundled template opens like New From Template
  (no unsaved changes): picking a shipped chart is not work to lose.
- **Templates are built, not hand-edited.** `templates/charts/charts.json` lists each
  template and its source; `scripts/build-coa-templates.py` runs `method coa import` on
  each source and writes `templates/charts/<id>.mthd` and the bundled list
  (`charts_bundled.rs`, `include_str!` of each file, so the app and `method new --template`
  carry them with no runtime files). Same source, same bytes; a test rebuilds every
  template whose source is in the repository and compares. Studio sources stay in the
  ignored `imports/coa-sources/`; a machine without one keeps its committed template.
  New files dropped there are added to the manifest by the script, named after the file.
- **What ships**: seven charts written by us (feature, low-budget feature, one-hour and
  half-hour TV with amortization, commercial, music video, documentary) in the default
  2 + 2 hyphen pattern, plus the studio and network charts built from the owner's CSV
  exports, each listed in `templates/charts/SOURCES.md`. Public incentive-program charts
  found during research are listed there too but not bundled: downloading them needs the
  owner's go-ahead, and they are PDFs to convert first.
- **Not yet**: a sub-account level in the model, reading PDFs, saved column mappings for
  chart imports (the sheet import's presets aren't shared), production totals applied to
  an existing budget, a mapping from one chart's numbers to another's (cross-walk).

## 52. Coding agents work on budget files through an MCP server (2026-10-10)

Roadmap 3.17. Coding agents (Claude Code and others) should be able to push the buttons
and help build budgets. `method mcp` is a Model Context Protocol server in the command line
(docs/MCP.md):

- **Files first, the app later.** The server works on budget files: each tool names a
  budget by path, reads it and writes the whole file back atomically, as `method set` does.
  A live bridge into the running app (local socket, a token the user approves, edits through
  `Session::edit` so they show in History and undo, shared copies and per-window rules
  respected) is designed in docs/MCP.md and left for later: it needs app UI that isn't
  designed yet.
- **No new dependency.** JSON-RPC over stdio is written by hand on `serde_json`, which
  `method-format` already uses and now re-exports, so Cargo.lock is unchanged. One message per
  line, logs to stderr only.
- **Both protocol eras.** The `initialize` handshake (revisions 2024-11-05 to 2025-11-25,
  answered with the client's revision or the newest we know) and revision 2026-07-28, where
  every request carries its version and capabilities in `_meta` and is served on its own
  (`server/discover`, `-32022` for an unknown version). Clients today mostly speak the first;
  the second is the current specification.
- **The path is the handle.** The protocol's current revision is stateless and asks state to
  be named on every call; a budget's real path does that. Per file the server keeps the bytes
  it last read or wrote, the budget as it opened it (for `diff` since the session started)
  and up to 50 undo steps of its own changes.
- **Outside changes win.** A change is refused, nothing written, when the file isn't byte for
  byte what the server last read or wrote (the app or a person saved it); the agent re-opens
  it and decides again. Reads show the file as it is now, flagged. Clients see a 64-bit
  fingerprint as the file's revision; conflicts are decided on the bytes.
- **Reuse, not a second engine.** Line fields go through the command line's edit language
  (`script::apply`), so values are checked exactly as `method set` checks them; renames use
  the engine's rename (decision 32), deletes its use counts, explain, diff, merge, export,
  report rendering and import are the engine's, the format's and the command line's own.
  Setup and charge editing follow the app's rules (unique names, short codes, a currency in
  use can't go, a group a credit is based on can't go); they are small and written again in
  the command line because it never depends on the app. Moving them into the engine, so both
  share one copy, is a candidate cleanup.
- **Every change is checked**: made on a copy; refused when the budget added up before and
  wouldn't after; a change that changes nothing writes nothing; shared copies are refused as
  the app refuses them.
- **Safety model.** Every path resolves inside a root (`--root`, default the current folder)
  with symlinks followed; nothing is written through a symlink; nothing below a root whose
  name starts with `.` is touched; budgets are only `.mthd`, sheets only spreadsheet
  extensions, exports only their format's extension, so the server can't write a script or a
  hook; existing files are replaced only with `overwrite`, and never a file that isn't a
  readable budget; `--read-only` removes the writing tools from the list; no program is run,
  no socket opened. Inputs have limits (8 MB messages, 128 MB files, 5,000 rows a batch,
  text, name, formula and path lengths). Arguments are checked against each tool's schema
  before it runs. Budget text is returned as data and the server's instructions say never to
  follow it.
- **Results for both people and programs**: a short summary, the JSON as text, and the same
  JSON as `structuredContent`. Money is integer cents in keys ending `_cents`, so a model
  can't read cents as dollars; rates are exact decimal strings. Problems a model can fix are
  tool errors with a kind (`invalid_input`, `not_found`, `refused`, `conflict`,
  `path_refused`, `io`); malformed messages are JSON-RPC errors.
- **Not yet**: resources and prompts, output schemas, versions and the baseline, budget info
  fields, report presets, notes on accounts and categories, renumbering, sub-budgets and
  actuals as tools, and the live app bridge.

## 53. Charts keep their numbers as written; repeated numbers get new ones (2026-10-10)

Follows decision 51, after the studio charts showed what reading structure from numbers
loses (accounts as wide as their categories dropped, `1A00` and `1B00` read as one
category, `3100` stored as `31`, a chart refused outright):

- **A sheet that names its rows is read as named.** In the `kind,code,name,parent` layout
  each account goes in the category its `parent` names (else the category above it, when
  there is no `parent` column), and every category and account number is kept exactly as
  written: mixed widths, letters (`1A00`, `39AA`), trailing dots (`4AA.`), separators
  (`5600-100`), spaces. Sub-accounts keep their numbers too, right after their account.
  Structure is guessed from number widths only for the other shapes, which have no other
  way to tell a category from an account; they still store a category as its part.
- **The pattern is for typing, not for reading.** It is still worked out (the one that
  writes the most account numbers exactly as they are, with the category part their
  category names; `infer_coding_lenient`), shown in the import and used for new numbers.
  It never reshapes or refuses a number already in a budget.
- **Matching never cuts a number short.** Everything that finds an account by number
  reads it through the pattern only when nothing would be cut off
  (`AccountCoding::read`, `key`, `category_key`), else as written, so `050101` is never
  `05-01` and `0501` is never `050101`. Checked one by one: actuals (assignment, the cost
  report, filters, duplicates detection, estimate overrides) and comparison by number
  (categories by the category they name, so `1100` and `11` line up); explain and the
  command line's targets (as written first, then as the pattern reads it); the MCP tools
  and the app's references to accounts (charges, credits) take a number already in the
  budget as written; editing a name leaves the number alone even when the pattern can't
  read it; sheet imports find existing accounts as written before checking a new number
  fits; applying a chart matches categories by the category they name; pasting keeps a
  category's full-width number. Diff and merge already matched numbers as written.
  Renumbering keeps a category written `1100` in that form (`011000` at 3 + 3) and refuses,
  listing each one, numbers it can't carry (`050101` under 2 + 2). A category's fringe
  account (fringes posted by category) is its number as written plus the suffix when the
  number isn't a plain category under the pattern (`0501` gives `050199`), so two
  categories never share one because the pattern cut them to the same part.
- **Empty categories are allowed.** Section headers and departments with no default
  accounts are part of the charts; they total 0 everywhere.
- **Repeated account numbers.** One listed again with the same name (ignoring case and
  spacing) is the same account: merged, and said so. One listed again with another name
  is another account, and it gets a new number: the chart's number, its separator (`-`
  when it has none, or a space), and a count (`10950`, `10950-2`, `10950-3`), the first not
  used anywhere in the chart. Keeping both under one number was ruled out because
  matching by number can't tell them apart and would misattribute money without a word:
  every transaction coded `10950` goes to the first account, comparison adds the two
  together, imports and lookups land on the first. The new numbers are listed in the
  import (a "Renumbered" list and a sentence in the preview, `check:` lines from the
  command line). They don't follow the pattern, so renumbering refuses them until renamed.
- **Placeholders.** An account named `.` is an unused slot in a chart: left out and
  counted. An account with no name comes in with no name.
- **Checked against the source.** The build script compares each template built from a
  `kind,code,name,parent` CSV with the CSV (categories and account numbers as written,
  account count less placeholders and merged repeats) and fails on any difference; a test
  checks every bundled template recalculates, compares and gives a cost report with no two
  accounts or category fringe accounts read as one number.
- **Not yet**: a typed new number still follows the pattern's parsing rules (a part too
  long is cut); the guessed shapes don't drop `.` placeholders (they come in unnamed).

## 54. Flat fringes count the Amount, as the reference product does (2026-10-11)

Checked in the reference product on 2026-10-11 (roadmap engine check 6), with units Day =
12 hours, Week = 60, Hour = 1 and Allow with no hours, a 100.00 per Day fringe ZDAY and a
1.00 per Hour fringe ZHR:

| Line | ZDAY | ZHR |
|---|---|---|
| 25 Days | 2,500.00 | |
| 25 Days x 3 in X (no unit) | 2,500.00 | |
| 25 Days x 14 Hours (X with Unit2 Hour) | 2,500.00 | |
| 25 Days x 14 Hours x 4X 2 (line 7,000.00) | 2,500.00 | 300.00 (25 x 12) |
| 2 Weeks | 1,000.00 (10 days) | 120.00 |
| 300 Hours | 2,500.00 (25 days) | 300.00 |
| 2 Allow | 100.00 | 1.00 |

So a flat fringe counts the **Amount column only**, converted from the line's unit into the
fringe's unit through unit hours; X and 4X never count; and a line whose unit has no hours
(or a fringe whose unit has none) is charged **once**, not times the Amount. We counted
Amount x X x 4X before (entry 8). The blank cells were not read; the tests hold our rule's
figures for them (300.00 each).

- **Three bases, stored on the fringe** (`basis`, FORMAT.md): `amount`, the default and
  what a missing `basis` means, is the checked rule. `amount_counts` also multiplies X when
  Unit2 is not a time unit (a blank unit counts as a count) and 4X when Unit4 is not: 3
  crew x 25 days counts 75 days, 14 Hours in X are ignored, 2 Episodes in 4X count. For
  fringes that really are per person or per episode. `quantity` is the old rule, step for
  step (Amount x X x 4X as the line shows it, converted through hours, 0 hours on the line's
  unit counting 0), so a budget built on it can keep its figures. A time unit is one whose
  hours are set and not 0; a unit with 0 hours counts as having none.
- **The count is exact**: the product of the Amount, the line unit's hours and any counts
  over the fringe unit's hours (and, with a cap, the Day's), rounded to cents once with the
  rate, half away from zero (entry 2). With one divisor it is the same arithmetic as before,
  so `quantity` is bit-identical to the old engine.
- **Most hours a day** (`max_hours_per_day`, optional): for a fringe charged per hour (its
  unit is 1 hour long, or named Hour), the count is days x min(hours a day, cap). Days are
  the Amount converted to days through the budget's Day unit (named Day or Days, Day first,
  with hours set and not 0). Hours a day are X when X is filled in and its unit is an hour
  unit (the line reads "N Days x H Hours"), else the Day unit's hours. With a 12-hour Day:
  25 Days at cap 12 counts 300 hours, at cap 10 250; 25 Days x 14 Hours at cap 12 counts
  300, at cap 16 350 (below the cap X's hours all count); 2 Weeks at cap 10 is 10 days x 10
  = 100; 300 Hours at cap 10 is 25 days x 10 = 250. X read as hours a day is never
  multiplied again; under
  `amount_counts` a counted 4X still multiplies, under `quantity` 4X does. Ignored for a
  fringe not charged per hour, when either unit has no hours, and when there is no Day unit
  with hours. Hour and Day are found by name as well as hours, so renaming the Day unit
  changes where a cap applies.
- **Cutoffs are for percent fringes only.** A flat fringe keeps a cutoff it has (switched
  from percent, from a file or a spreadsheet) but never uses it; the editor says so in a
  quiet line, Explain notes it, and switching back to percent brings it back. The command
  line and the MCP tools clear one but refuse to set one; a setup spreadsheet's cutoff on a
  flat fringe is kept, where it was refused before.
- **Still per line**: fringe ranges only matter for percent cutoffs; a flat fringe is
  charged on each line on its own.
- **Explain says the rule and the count**: "ZDAY 100.00 per Day x 25 Days (amount only; 14
  Hours in X ignored) = 2,500.00", "x 1 (Allow has no hours: charged once)", a Count note
  with the conversion ("2 Weeks x 60 hours per Week / 12 hours per Day = 10 Days"), the cap
  ("14 hours a day from X, capped at 12 hours a day: 25 days x 12 hours = 300 Hours") or the
  counts ("25 Days x 3 (X) = 75 Days"). Engine, reference engine and Explain share one
  count (`calc::fringes::flat_count`); the leaves still add up to every total.
- **Everywhere a fringe is edited or shown**: the Budget Setup editor and the tools panel
  (a Basis list: "Amount only (standard)", "Amount, plus counts in X and 4X", "Full
  quantity"; a Max hours per day box for a fringe charged per hour, sent only while it
  shows), the lists' rate text and the printed fringe list ("1.00 USD per Hour, plus counts
  in X and 4X, up to 12 hours a day"), setup sheets (Basis and Max Hours Per Day columns,
  reading the labels or the file's names), the budget export's Fringes sheet (two columns
  added last), diff and merge (each field on its own, the kind's cutoff, unit, basis and cap
  merged one by one), `method set fringe.NAME.basis` and `.max_hours_per_day` and the
  same script commands, the MCP setup tools. Copies, packs, the library and import setup
  carry the fields, and bring the Day unit along with a capped fringe.
- **Figures that changed**: in the hand-checked sample (tests/common), the Key grip's 50.00
  per Day fringe on 40 Days x 3 is 2,000.00, not 6,000.00, so the Production category is
  70,149.01 (was 74,149.01), the grand total 282,134.14 (was 286,634.14, with contingency
  and insurance on the smaller base) and the net 272,234.14 (was 276,734.14). The export
  test plan's predictions are unchanged: none of its flat-fringed lines has X or 4X (Driver
  ZFLAT on 3 Zweeks is still 15 Zdays, 375.00; Driver kit on 2 Zallow still 25.00 once).

## 55. Movie Magic arithmetic by default, exact decimals as an option (2026-10-10, the owner)

The parity run of 2026-10-10 (imports/mmb-parity/2026-10-10, run-log.md and
parity-report.md, item A1, A7, A8 and A9) showed the reference product, Movie Magic
Budgeting 10, works its figures out in binary doubles, not in exact decimals. The owner's
call, under the rule to match it as closely as possible: **match its arithmetic by
default, and keep our exact decimals (entries 1 and 2) as a setting per budget.**

- **The setting** is the budget's `arithmetic`: `mmb` (the default, also for every file
  that doesn't say, so existing files change to it) or `exact`. It is content: versions
  carry it, diff lists it with the budget settings, merge merges it like one, the recalc
  cache starts over when it changes. FORMAT.md writes it only when it is `exact`;
  `method set FILE arithmetic exact` and the script command `set arithmetic exact` switch
  it.
- **Movie Magic arithmetic, as checked**:
  - Formulas, globals and every money step are doubles, each operation rounding to a
    double. A line is `((Amount x X) x 4X) x Rate`, rounded to cents once like JavaScript's
    `Math.round(v * 100) / 100`: `v * 100` in doubles, then the nearest whole number, a
    half going toward plus infinity (Math.round's own rule, not `floor(x + 0.5)`, which
    gets 0.49999999999999994 wrong). Checked: 1 x 1.005 is 1.00, 0.145 is 0.14, -1 x 0.125
    is -0.12, 2.675 is 2.68 (2.67499999999999982... times 100 rounds to the double 267.5),
    3 x 432.105 is
    1,296.32, 1/3 (0.3333333333333333) x 30,000,000.015 is 10,000,000.00, 3 x
    3,333,333,333,333.335 is 10,000,000,000,000.00. ZTEST01 grand total
    10,000,010,005,314.61.
  - A foreign line converts the unrounded amount and rounds once: 0.335 EUR at 3 is
    1.0050000000000001, so 1.01 (we rounded to 0.34 EUR first: 1.02). The line's own
    currency total still shows rounded (0.34).
  - Globals are doubles: `17 / 3` is 5.666666666666667, and x 15,000,000 is
    85,000,000.00 (9 places made it .01). A deleted global is frozen into formulas with the
    shortest digits that give its double back, so no total moves.
  - A number in a formula keeps every place it is written with: 100,000,000 x 0.0000000149
    is 1.49. The file always kept formulas as typed; the formula reader now takes more than
    9 places (exact arithmetic still refuses them, with "more than 9 decimal places").
  - A percent fringe is `base x percent / 100` on the base's dollars: 12.5% of a 4,444.44
    cutoff is 555.55499..., so 555.55 (the cutoff factor case V04-2). A flat fringe's count
    is worked out first (`Amount x line unit hours / fringe unit hours`, left to right),
    then times the rate: 0.06 per 12-hour Day on 1 Hour is 0.06 x 0.08333333333333333 =
    0.004999999999999999, so 0.00 (F-19). A percent charge is the same as a percent fringe
    (0.4425% of 22,600.00 is 100.01, of 21,800.00 96.47: those land above the half in
    doubles, as they do exactly).
  - An adjustment row takes its percent of the rows above added up in doubles, row by row
    (the reference product stores that sum, 132,347.66999999998, as the row's rate).
  - Credit rows are added up unrounded, capped and discounted, then rounded once, as
    before, in doubles.
  - Accounts, categories, levels, the grand and net totals stay integer sums of the
    rounded cents, so rollups always agree.
- **Not checked, chosen**: where the order of operations could matter and no figure
  decided it, we multiply left to right as written above (percent: base x percent, then /
  100; flat: count, then x rate; a capped flat count: Amount x line hours x hours a day /
  Day hours / fringe hours; credits: percent of net, then / 100, then x (100 - discount) /
  100). The most-hours-a-day cap and the `amount_counts` basis are ours, so their order is
  ours too.
- **Exact arithmetic is unchanged**, figure for figure (the same budgets give 1.01, 0.15,
  -0.13, 9,999,999.99 and 10,000,000,000,000.01 on ZTEST01; 0.01 on F-19; 555.56; 1.02;
  85,000,000.01).
- **Shown values** stay decimals: a line's quantity and the report's globals are the
  doubles rounded to 9 places for showing; the money they make is worked out from the
  doubles.
- **One place in the code**: `calc::arith` (`Num`, a decimal or a double; `Money`, an
  unrounded amount, exact or a double; `mmb_cents`). The engine, the reference engine and
  Explain all go through it, so the switch is in one place and Explain's leaves still add
  up to every total in both arithmetics (the explain, property and reference tests run
  both).
- **Explain says the rule**: a line under the title ("Movie Magic arithmetic: binary
  doubles, each amount rounded to cents once, half up on the double", or the exact rule),
  and each Rounding step says what the double holds when that decides it ("1.005 is
  1.00499999999999989341... as a binary double, so it rounds to 1.00 (Movie Magic
  rounding: half up on the double)").
- **Left as they were**: showing totals in another currency (`Report::in_totals_currency`)
  still divides exactly, per total (the reference product converts and rounds each row,
  parity report section B: a display change for later). Settings stored as decimals
  (currency rates, charge and credit percents, unit hours, max hours a day) keep 9 places,
  and the spreadsheet importers still round numbers to 9 places. The editor's lenient
  recalc checks formulas in the budget's arithmetic.

## 56. Ranges, charges, levels and posting as Movie Magic works them (2026-10-10, the owner)

The parity run of 2026-10-10 (imports/mmb-parity/2026-10-10, run-log.md and
parity-report.md, section A items 2 to 6, 10 and 11) found six more places where Movie
Magic Budgeting 10 works a total out differently. The owner's rule is to match it by
default; where matching drops money, our behavior stays as the default and Movie Magic's
is a setting.

- **A range uses its cutoff up in line order** (A2; replaces the pro rata split of entry
  3). Within an earner (a fringe range, or one row), the rows carrying a percent fringe are
  taken in order: each takes the percent of its total, or of the cutoff still left when
  that is less, rounded to cents on its own; the cutoff left goes down by the row's whole
  total. Once nothing is left a row takes nothing. Rows without the fringe, subtotal and
  separator rows and rows that don't count (suppressed or filtered out) are skipped and use
  none of it. A row on its own is the same rule with the whole cutoff, so a credit on its
  own still earns its (negative) percent, while a credit after the cutoff is used earns 0.
  Without a cutoff each row is simply its own percent, rounded alone. Checked (ZTEST04):
  ZP125 12.5% with a 2,222.22 cutoff on Prep 1,249.36 / Shoot 5,309.78 / Wrap 624.68 is
  156.17 / 121.61 / 0.00; two 1,500.00 lines with ZP125 and ZP321 (3.21%, no cutoff) give
  ZP125 187.50 then 90.28 and ZP321 48.15; ZP10 (10%, cutoff 1,000.00) on First 1,500.00 /
  a suppressed Middle / Last 1,500.00 is 100.00 / none / 0.00, across a subtotal row 100.00
  / 0.00, in two ranges of 1,500.00 + 1,500.00 100.00 / 0.00 each, on Fringed 600.00 / Not
  fringed 600.00 / Fringed again 600.00 60.00 / none / 40.00, on Pay 3,000.00 / Refund
  -500.00 100.00 / 0.00 (on its own the refund earns -50.00); three 0.04 lines with ZP125
  are 0.01 each (0.03), not 0.015 rounded once. No setting: the proration is gone.
- **A percent charge's base is everything before charges** (A3; replaces entry 9's "ATL +
  BTL", which left fringes posted by budget out). It is every level (every category with
  its fringes, wherever they post, and the categories after the last production total)
  plus the budget's Total Fringes row: the grand total before charges. Excluding a line,
  account or category takes the line's fringes off with it whatever the posting mode, so a
  charge comes to the same in every posting mode. Checked: ZTEST06 posting by budget, ZP10N
  fringes 3,100.00 on 29,400.00 of lines, ZPCT5 5% = 1,625.00, grand 34,125.00; ZTEST09
  posting by budget changes no charge (ZPCTCAT 2% excluding category 2100 with its 800.00
  of fringes is still 270.00).
- **A flat fee honors category and account exclusions** (A4): typed amounts and the lines
  of excluded groups, accounts and categories (each line once, with its fringes) come off
  the fee, as for a percent charge. Checked (ZTEST09): ZFLATCAT 10,000.00 excluding
  category 3100 (2,000.00) is 8,000.00; ZFLAT 2,500.00 less 222.00 typed and group ZNOCHG
  (800.00) is still 1,478.00, and 2,278.00 with ZNOCHG out of the budget total. A fee
  smaller than its exclusions goes below zero (not checked; no figure decided it).
- **ATL and BTL follow the production totals** (A5; amends entry 9). Above-the-line is the
  first level (the rows up to the first production total) when a production total closes
  it; below-the-line is every later level a production total closes; categories after the
  last production total count in neither, and with no production total nothing is above or
  below the line (ATL = BTL = 0). An empty first level (a production total at the very
  top) is ATL 0, not skipped. The grand total is unchanged: every level, the trailing one
  too, plus budget fringes and charges. Checked (ZTEST11, four categories of 1,000.00 to
  4,000.00): no production totals 0 / 0; a total after 1100 1,000.00 / 0; a total above
  1100 as well 0 / 1,000.00; and a closing total after 4100 0 / 10,000.00. ZTEST06 (totals
  ATL, PROD and POST, 4100 trailing with 1,100.00) is ATL 16,500.00, BTL 14,900.00.
- **Fringes after the last production total** (A6). Posting by production level, Movie
  Magic puts each level's fringes in a row above its production total, and a category after
  the last one loses its fringes: not a setting, and adding a closing production total
  brings them back (V06-2: ZPCT5 1,620.00, grand 34,020.00 instead of 34,125.00). That
  drops money, so **we keep them by default**, in a fringe row of the unnamed last level,
  and `method check` and the MCP `validate` tool warn: "fringes of categories after the
  last production total are not in any level (4100 100.00): kept in the grand total; Movie
  Magic leaves them out (trailing_level_fringes drop matches it)". The budget setting
  **`trailing_level_fringes`** (`keep`, the default, or `drop`; FORMAT.md) drops them as
  Movie Magic does: the trailing level posts no fringes, they count in no total and no
  charge base, and an excluded line there comes off a charge without them. It changes
  nothing under the other posting modes. With no production total at all every category
  is trailing, so `drop` drops every fringe (not checked; it follows from the rule).
  Breakdowns (fringe, group, set and location totals) and credits still count fringes as
  earned.
- **Adjustment rows can carry fringes** (A10). A fringe on an adjustment row is charged
  as on a line: ZP10N on the 350.00 adjustment is 35.00 (ZTEST03, account 1101 fringes
  335.00). An adjustment row is an earner of its own, or part of a fringe range when it
  carries the range's number (it already broke a range otherwise, and still does). Not
  checked, chosen: a flat fringe on an adjustment row counts its Amount the way it would on
  a line (entry 54; the Amount is the percentage, so a per-line fringe is the usual one).
  The grid shows the Fringes cell, fringe dollars and fringe percent on adjustment rows;
  the apply panel, tag presets, right-click Apply Fringe, `set line` and the MCP tools put
  fringes on them.
- **Global names may start with digits** (A11; amends entry 5). Movie Magic accepts a
  global named `1ZG` and `1ZG*1` in Amt is 7.00 (V02-3). Our grammar can read that
  unambiguously, so it does: a name is letters, digits and `_` with at least one letter or
  `_`; the reader takes a run of digits that runs on into a letter or `_` as a name, and a
  run of digits and dots otherwise as a number (`1.5ZG` is still a number then a name, so
  an error). No formula that parsed before reads differently: a number was never directly
  followed by a name in a valid formula (there is no implied multiplication or exponent),
  so only some error messages change (`2SHOOT` was "unexpected token", now "unknown global
  `2SHOOT`"). Renaming follows the same split, so `DAYS` is not touched inside `2DAYS`.
  Importers need not rename such globals. All digits (`12`) is a number, never a name.
- **Explain says it**: a range line reads "range 1: lines 1, 2 earn as one person, so they
  share one cutoff, used up in line order", then the line's earnings and the cutoff left
  after the lines above ("Cutoff used up: ... so this line takes nothing" when none is);
  ATL and BTL list the levels they hold, BTL notes that the categories after the last
  production total are in neither, the grand total adds that trailing level, and a charge's
  base is "every category with its fringes, wherever they post, plus the budget's fringe
  row". Its leaves still add up to every total (the explain tests cover every new case).
- **Figures that changed**: in the hand-checked sample (tests/common) the grip's range
  splits its 765.00 of FICA 765.00 / 0.00 (was 737.68 / 27.32); every account, category
  and grand total is unchanged. A budget with categories after its last production total
  shows a smaller BTL (and with none, ATL and BTL of 0); a budget posting fringes by budget
  gets larger percent charges; flat fees with category or account exclusions get smaller.
- Tests: crates/method-core/tests/mmb_parity.rs reproduces every figure above (both
  arithmetics where they could differ), the property tests generate fringed adjustment
  rows and the `drop` setting ("every fringe dollar posts exactly once, unless it is
  dropped"), and the reference engine works ranges out on its own.

## 57. Category credit rows and the group rules, as Movie Magic has them (2026-10-10, the owner)

The parity run of 2026-10-10 (imports/mmb-parity/2026-10-10, run-log.md, "ZTEST07
GROUPS" and "V10-3 category credit") found two things Movie Magic Budgeting 10 does that
we didn't. The owner's rule is to match it by default (entries 55 and 56).

- **A credit row can be a topsheet category** (V10-3; amends entry 9). Besides a typed
  amount and a group, a row's spend can be one category: `{"kind": "category",
  "category": "2100"}` (FORMAT.md, "Credit row"). Its gross is the category's total (its
  lines and adjustment rows that count) plus the fringes they earn, **wherever fringes
  post**, as Movie Magic counts it: with fringes posted by budget, ticking ZCR3 on category
  2100 (4,000.00 of lines, 300.00 of fringes) raised the credit's gross from 1,234.57 to
  5,534.57 (+4,300.00), and the new row starts at 0%, so the credit stayed 411.52. At 10%
  the row is 430.00 in every posting mode and both arithmetics. Like a group row, a
  category row's fringes are those its lines earn, so a category after the last production
  total keeps them when `trailing_level_fringes` is `drop` (entry 56), and a left-out line
  earns none.
- **Excluded fringes on a category row: checked in Movie Magic on 2026-10-11.** Movie Magic
  keeps one excluded-fringe list per credit and applies it to group and category rows alike
  (a credit excluding ZFDAY, with category 1100 added: gross 4,500.00, excluded 100.00, net
  4,400.00). We apply it the same way: the listed fringes come off the category row's
  gross (2100 less ZP10N is 4,300.00 - 300.00 = 4,000.00, so 10% is 400.00).
  **Amended 2026-10-11: the list belongs to the credit.** Our first model kept a list on
  each group and category row, and the app's credit editor dropped it when a row's Spend
  changed from a group to a category. Now, as in Movie Magic, a credit has one
  `excluded_fringes` list (FORMAT.md, "Credit") applied to all its group and category rows
  (a typed amount has none), the app shows it once under the credit's Discount, and the
  MCP `set_credit` takes it on the credit. A file with per-row lists is read as the union
  of them on the credit (FORMAT.md, changelog). Diff, merge, rename, usage, delete, setup
  sheets (the Excluded Fringes column repeats the credit's list on each of its rows) and
  import handle it as a credit field. ZTEST10 V10-1 still holds (ZCR1 1,205.40, net
  5,450.47), and switching ZCR1's group row to category 1100 keeps its ZFDAY exclusion.
- **A row's category follows the chart.** Renumbering carries it like a charge's excluded
  categories; renaming a fringe renames it in the credit's exclusions; a fringe's use
  count includes them. A row on a category the budget doesn't have counts 0 (as a charge's
  excluded category that matches nothing), and `method check` and the MCP `validate` tool
  warn: "credit ZCAT row 1 is on category 9900, which is not in the budget, so its spend is
  0". The app and the MCP `set_credit` refuse a category the budget doesn't have, as they
  refuse an unknown group.
- **The group rules** (ZTEST07; amends entry 4). Movie Magic's GROUPS setup carries three
  budget-wide rules, each Include or Exclude; ours is the budget setting `group_rules`
  (FORMAT.md, "Group rules"), each rule written only when it isn't Movie Magic's default,
  and the object only when one isn't:
  - `ungrouped_lines` (default `include`). `exclude` leaves out every line with no group
    **and every adjustment row with no group** (an adjustment row has no group of its own
    unless one is put on it), as Movie Magic does; subtotal rows then show only the lines
    left. Checked (V07-3): lines In 1,000.00 (ZIN, ZP10N), Out 2,000.00 (ZOFF, off), In
    and out 3,000.00 (ZIN and ZOFF), In twice 4,000.00 (ZIN and ZIN2, ZP10N), No group
    500.00, a subtotal, a 10% adjustment, and In two 600.00 (ZIN2) in 1201, fringes posted
    by budget: Include gives subtotal 5,500.00, adjustment 550.00, grand 7,150.00; Exclude
    gives subtotal 5,000.00, the adjustment and No group left out, grand 6,100.00.
  - `conflict` (default `exclude`). A line in a counted group and a group whose "in budget
    total" is off: left out (what we always did), or counted with `include` (V07-4: In and
    out counts, ZOFF's active total is 3,000.00, grand 10,450.00). A line whose groups are
    all off is left out either way.
  - `fringes_in_totals` (default `include`): whether group totals show their lines'
    fringes. Display only, as in Movie Magic: the group dollars in the setup tables and the
    printed Groups section's Total column leave fringes out with `exclude`; no budget total
    changes, and the report still carries each group's fringes separately.
  A line a rule leaves out behaves as a suppressed line always has (entry 4): it shows,
  counts as 0, earns no fringes and doesn't reset an adjustment window. The recalc
  options' what-if filters still apply on top. Explain says why: "the line has no group
  and the group rule leaves out lines with no group", or for a conflict "(it is also in a
  counted group, and the group rule leaves out such conflicts)".
- **Where they are set**: `method set FILE group_rules.RULE include|exclude` and `set
  group_rules RULE include|exclude` in scripts (CLI.md); in the app, a Group rules block at
  the end of Budget Setup's Groups table (the `set_group_rule` command, APP.md); the credit
  editor's Spend menu lists the categories after the groups ("Category: 2100 Electric").
  Setup sheets carry a credit row's category in a Category column. Versions, diff, merge
  and the recalc cache handle the rules like the other budget settings.
- **Still different** (not in scope here): Movie Magic's group "total" counts only the
  fringes a line actually earns (ZOFF total 5,000.00), while our full total gives a
  suppressed line its fringes as if it counted (5,200.00). Matched since (entry 58).
- Tests: crates/method-core/tests/mmb_parity.rs reproduces every figure above in both
  arithmetics (and V10-3 in every posting mode); the format, CLI, MCP and app tests cover
  writing, reading and setting them.

## 58. A group's total counts only the fringes earned, as Movie Magic shows it (2026-10-11, the owner)

The parity run of 2026-10-10 (imports/mmb-parity/2026-10-10, run-log.md, ZTEST07 and
ZTEST10) read the "total" Movie Magic Budgeting 10 shows next to a group's active total.
It is every tagged line at its base, suppressed lines included, plus only the fringes
actually computed. A suppressed line earns no fringe, so it adds its base alone. Ours gave
a suppressed line its fringes as if it counted. The owner's rule is to match it (entries 55
to 57); this amends entries 4 and 57.

- **The figures**: ZTEST07, ZOFF (in budget total off) holds Out 2,000.00 (10% fringe
  ZP10N) and In and out 3,000.00 (no fringe): total 5,000.00, active 0.00 (ours was
  5,200.00). With ZOFF turned on: 5,200.00 and 5,200.00. ZIN: 8,500.00 (1,000.00 + 100.00,
  3,000.00 suppressed at its base, 4,000.00 + 400.00), active 5,500.00. ZIN2: 5,000.00 and
  5,000.00. ZTEST10, ZCRED: 6,500.00 (4,000.00 with 500.00 of fringes, 2,000.00 suppressed
  at its base), active 4,500.00; ZOFF 2,000.00 and 0.00.
- **How**: the report's `groups[name].full` is worked out in one place
  (`add_group_total` in crates/method-core/src/calc/breakdown.rs): each line's base from
  the unfiltered pass, its fringes from a pass under the group rules alone. The recalc
  options' what-if filters (`exclude_groups`, `only_groups`) still don't move it; that
  extra pass runs only when a filter is set. `GroupTotals::shown` gives the total and the
  active total as group lists, setup tables and reports show them, without fringes when
  the group rule `fringes_in_totals` is Exclude (entry 57). Every place that shows a
  group's total (the setup lists and their tooltips, the printed Groups section, the
  Groups export sheet, `list_setup` over MCP, which now also gives `full_fringes_cents`)
  reads that figure.
- Tests: crates/method-core/tests/mmb_parity.rs reproduces every figure above in both
  arithmetics (ZTEST10 in every posting mode), with filters set and with the fringes rule
  set to Exclude.

## 59. Importing a whole budget from another program's exports (2026-10-11)

Roadmap 1.16, built from the export files of the 2026-10-10 and 2026-10-11 parity runs
(the reference product's documented, user-facing open exports: JSON, XML basic and
advanced, Excel, tab-delimited) and nothing else (CLEAN-ROOM.md). The owner approved all
five. docs/IMPORT.md is the user's account; this is why it works the way it does.

- **One reader of the layout, one builder.** Every layout is made into the tree the JSON
  export is (the XML element for element, the workbook's four sheets row by row, the
  tab-delimited topsheet line by line), read into one model
  (`method_format::import::exchange::model::Export`) and made into a budget by one builder
  (`exchange::build`). What a layout lacks is flagged on the model (no fringe names on the
  lines, no fringe figures, whole units, totals only) and the builder works it out or says
  so. The XML is read with our own tokenizer (no DTDs or external entities), the JSON with a
  small reader of our own that keeps each number's digits (no float parses a number twice;
  no new dependency), the workbook with our ZIP and SpreadsheetML readers.
- **Worked out, never guessed silently.** Exports keep values, not rules. Where the figures
  decide something the import works it out and lists it under Check: fringe posting (from
  where the Total Fringes rows and accounts are), the account numbering, adjustment rows (a
  base-currency row whose rate is the total of the rows above it), exchange rates (the
  simplest rate that gives every line of the currency), percent fringe cutoffs (solved from
  what the fringe was charged on against what its lines come to, using the cutoff in line
  order, entry 56) and fringe ranges (lines that earned less than their rate), hours of
  added units (from what the flat fringes counted, when one value fits), and for the
  workbook every fringe's kind and rate from the lines' fringe percentages, its cutoffs
  refined against the accounts' exact fringe totals by recalculating. What the figures
  can't decide is listed under Not imported: formulas and globals (lines keep the values
  and a note naming the globals; no global is created, since no value is unambiguous: a
  global can sit in any factor or inside an expression), production totals (added only
  under production-level posting, where the fringe rows show the levels), suppressed lines
  (the exports omit them), and the rules of charges and credits (they come in as fixed
  amounts that give the same totals; a charge that is a round percentage of the total
  before charges is pointed out).
- **The starter units' hours** are the reference product's (Day 12, Week 60, Hour 1, Month
  240, entry 54's checks), since the exports carry none and a budget made there starts with
  them; the note says so.
- **The reconciliation is the test.** The built budget is recalculated by our engine (its
  default arithmetic, entry 55) and every figure the export has is compared: lines,
  accounts and their fringes, fringe accounts or rows, categories and their fringes,
  charges, credits, the grand total (the topsheet rows added up) and the net total. Layouts
  in whole units compare in whole units; a budget shown in another currency compares as the
  export shows it (each line and each fringe's total converted and rounded, then added).
  Every difference is listed; none stops the import.
- **Figures read as their digits.** A figure is rounded to cents from its shortest decimal
  spelling, half away from zero (entry 1), so the workbook's 100.005 charge is 100.01 as the
  program shows it, though the double is a hair below.
- **Results on the parity runs** (imports/mmb-parity, not in the repository): all 35 JSON
  exports and all 13 exports of the other layouts come to their own figures, except
  ZTEST08 V08-2 (totals shown in pounds: the program works its adjustment row out in pounds,
  0.01 on the row, 0.03 on the totals). ZTEST10 nets 5,450.47 from the JSON, the advanced
  XML and the workbook (5,450 from the basic XML, in whole units); ZTEST06 by category comes
  to 34,125.00; ZTEST09 comes to 41,389.90 from the JSON and the advanced XML (they write the
  100.005 charge as 100.00) and 41,389.91 from the workbook, as the program shows it. The
  advanced XML imports identically to the JSON; the workbook too but for a cutoff (2,222.24
  where the JSON's figures give 2,222.22: any value from 2,222.20 to 2,222.28 gives the same
  totals) and the unrounded charge; the basic XML loses groups and globals and rounds
  charges and credits to whole units; the tab-delimited file brings each account's total
  as one line.
- **Surfaces.** `method import-export` (docs/CLI.md), the `import_budget_export` MCP tool
  (docs/MCP.md) and File > Import Budget Export… in the app, whose review follows Import
  Spreadsheet's and opens the budget in a new unsaved tab (docs/APP.md). User-facing text
  says "budget exchange JSON/XML" and "budget export", never the other product's name.
- Tests: crates/method-format/tests/exchange.rs (reduced, renamed copies of the exports in
  tests/fixtures/exchange, every layout of four budgets kept twins; each comes to its own
  figures and each layout imports as its JSON twin but for what it lacks; with
  `METHOD_EXPORT_SAMPLES` pointing at the parity runs' folders, every real export too), the
  readers' unit tests, `method import-export` end to end, the MCP tool, and the app's review.

## 60. New-budget defaults, the cutoff factor, totals in another currency and the topsheet's order, as the reference product has them (2026-10-11, the owner)

The parity run of 2026-10-10 (imports/mmb-parity/2026-10-10, run-log.md: C2, ZTEST02,
ZTEST04 V04-1 to V04-3, ZTEST05, ZTEST07, ZTEST08 V08-1 and V08-2, ZTEST09 V09-2, ZTEST10,
ZTEST11) read the reference product's new-budget settings and several things it shows that
we didn't. The owner's rule is to match it by default, with our extras opt-in (entries 55 to
58); the owner approved the new-budget defaults below **except totals keep showing cents**
(the reference shows whole units by default; ours stays a per-user View choice).

- **New budgets start as the reference product starts them.** Fringes post by budget; account
  numbers are category 2, account 2, no sub-account, set 3; the starter units are Day (12
  hours), Week (60), Month (240), Hour (1), and Allow and Flat with no hours (ZTEST05; the
  reference's seventh unit, `%`, marks percent fringes, which is our fringe `kind`); a new
  line copies its fringes from the line above and nothing else (as before); the cutoff
  factor is 1.0. `method_core::starter::new_budget_settings` sets them, and every new
  budget gets them: the starter (`method new`, the app's New, the MCP `new_budget`),
  `starter::blank` (the MCP's `blank`), and a budget made from a chart (`Chart::to_budget`,
  so `method coa import`, File > Import Chart of Accounts into a new budget, and the bundled
  chart templates, rebuilt with `scripts/build-coa-templates.py`; a chart keeps its own
  account numbering). `default_units` changed, so `Budget::new` has the new hours too.
  **Existing files keep their own values**: units are always written in a file, and a file
  without `fringe_posting` or `account_coding` still reads as by category (suffix 99) and 2 +
  2 with `-`, the file defaults (FORMAT.md). A new budget's file writes both.
- **A set may follow the account with no sub-account** (amends entry 11). The reference's
  pattern (set 3, no sub-account) broke our rule that used segments are contiguous, so the
  rule is now: the sub-account and the set each need the account; the set may skip an
  unused sub-account. Parsing, formatting, renumbering and the app's coding form follow it
  (`11-01-5` reads as `11-01-005`). The separator stays `-` for new budgets: the brief
  named only the widths, though the reference's exports write numbers without one
  (`1101`). Open for the owner.
- **The cutoff factor** (EC 5, roadmap 1.17): a budget setting `cutoff_factor`, one of 1.0,
  1.5, 2.0, 2.5 and 3.0 (FORMAT.md, "Cutoff factor"), written only when it isn't 1.0, that
  multiplies every percent fringe's cutoff; flat fringes have none to multiply. Checked:
  ZP125 (12.5%, cutoff 2,222.22) on P-02 (5,000.00) is 416.67, 555.55 and 625.00 at 1.5,
  2.0 and 3.0, on P-10 (3,703.68) 416.67, 462.96 and 462.96, and on P-01 (1,000.00) 125.00
  throughout. 4,444.44 x 12.5% is 555.555, 555.55499... in doubles, so 555.55 (exact
  arithmetic: 555.56). **Chosen, not checked**: the factored cutoff is rounded to cents by
  the budget's arithmetic before it is used (a cutoff with an odd cent lands on a half cent at 1.5),
  and 2.5 was not read in the reference. Explain shows the step ("the budget multiplies
  every percent cutoff by 1.5: 2,222.22 x 1.5 = 3,333.33"). Versions, diff, merge and the
  recalc cache carry it like the other calculation settings; `method set FILE cutoff_factor
  1.5`, the script's `set cutoff_factor 1.5`, and Settings > Calculation set it.
- **Totals in another currency** (amends entry 12): the reference converts each account's
  total and rounds it, converts a fringe row fringe by fringe, then adds those up. V08-1
  (EUR at 3): 1100 48,860.81, 1200 166.67, fringes 225.92 (100.00 + 92.59 + 33.33, not
  677.78 / 3 = 225.93), grand 49,253.40 (not 147,760.22 / 3 = 49,253.41). V08-2 (GBP at
  1.23457): 118,731.57, 405.00, 549.00 (243.00 + 225.00 + 81.00, not 549.01), grand
  119,685.57. The run log read V08-1 as line by line, but V08-2 rules that out: line by line,
  1100 would be 118,731.60 in GBP; the account total divided once gives 118,731.57. So
  `Report::display` (`DisplayTotals`) converts each account (with its fringe line, when
  fringes post by account), a category's fringe account as one more account, the budget's
  and each level's fringe row fringe by fringe, and each charge and credit on its own, each
  rounded by the budget's arithmetic; categories, levels, ATL, BTL, grand and net are sums
  of those. **Chosen, not checked**: accounts posting their fringes and production-level
  fringe rows (V08 posted by budget). Where totals show in the other currency (the app's
  "Net total in", the printed and exported grand and net totals in it, `method show`,
  `method totals`, the MCP `totals`), the figures are these. `Report::in_totals_currency`
  still converts one amount exactly.
- **The topsheet's order** (amends entry 9; `Report::topsheet_lines`, used by `method show`,
  the printed report from the command line, the spreadsheet export, and mirrored by the
  app's grid and printed topsheet): after the summary rows and the budget's fringe row,
  charges **newest first** (V09-2: creation order, newest first; a budget's charge list is
  in the order they were made, which changes no figure since charges never stack, so the
  topsheet lists it in reverse), then the grand total, credits **in reverse application
  order**, a **Sub Total** of the credits when there are two or more (shown as a negative,
  like the credit rows), and the net total. ZTEST10: ZCR3, ZCR5, ZCR6, ZCR4, ZCR2, ZCR1, Sub
  Total 3,349.53, net 5,450.47. Charges stay after the summary rows, where they were; ROADMAP
  decision 13 notes the reference puts them before the summary rows, which no run has
  confirmed. Open for the next run.
- **The budget's fringe row** (fringes posted by budget) has an account number (none by
  default), a description (`Total Fringes` by default, as C2 read them) and a place: after
  the summary rows by default, or just above any topsheet row (the reference's Move Fringe
  Total and Reset Fringe Total to Default Position). A budget setting `fringe_row`
  (FORMAT.md, "Fringe row"), written only when not the default. **Display only**: moving it
  changes no total, ATL or BTL (not checked in the reference, PM-POS; kept unchanged). The
  place is a topsheet index; the app's own topsheet edits (adding, deleting, moving and
  cutting categories and production totals) keep it above the same row, and an index past
  the end shows it after the last row. `method set FILE fringe_row.account|description|
  position`, the script's `set fringe_row ...`, and Settings > Fringe totals (shown when
  fringes post by budget) set it. Import Setup doesn't carry it (its place belongs to one
  topsheet).
- **A global's unit fills the line's Unit** (G-08, EC 13): typing an Amt that is exactly a
  global's name, when the global has a unit the budget has and the line's Unit is empty,
  fills the Unit with it (`method_core::edit::unit_from_global`). In the app's cell entry
  only; the command line, MCP tools and importers set the Unit they are given.
- **Formulas refused at entry** (V02-4; amends entry 5): in the app, a formula typed into
  Amt, X, Rate or 4X is refused when it divides by zero (`ZG_A/0`, or `1 / (SHOOT_DAYS -
  25)`) or uses a global that refers back to itself; a global's formula is refused when it
  divides by zero, would make globals refer to each other or to itself, or uses one that
  does (`method_core::edit::refused_formula`, `refused_global`). The cell or global keeps
  what it had and the message says why. A global that refers back to itself anyway (from a
  file) shows #ERR in the Globals table and counts as 0, as in the reference; the app's
  lenient recalc already valued it 0. **Unchanged by choice**: any other problem (a typo, an
  unknown global) is still kept as typed and flagged, where the reference refuses every
  invalid expression; the engine, `method check` and `method set` still stop on a
  circular global with the cycle named.
- Tests: crates/method-core/tests/mmb_parity.rs reproduces every figure above (V04 at every
  factor in both arithmetics, V08-1 and V08-2, ZTEST10's credit order and Sub Total, V09-2's
  charge order, V06-3 with the fringe row at every place, G-08, V02-4, and the new-budget
  settings); the coding, format, CLI, MCP and app tests cover the rest. The hand-checked
  sample keeps its figures; tests that leaned on the starter posting by category set it
  explicitly.

## 61. Account numbers without a separator, account credit rows, fringe account numbers, the full fringe percent, frozen original totals and currency formats, as the reference product has them (2026-10-12, the owner)

The owner's rule is to match the reference product by default, with our extras opt-in
(entries 55 to 60). This entry settles what entry 60 left open and takes in parity run 2
(imports/mmb-parity/2026-10-12/run-log.md, cases R2-01 to R2-08; gitignored, main checkout
only) and the 2026-10-11 exports. Tracker: docs/research/parity-tracker.md.

- **New budgets number accounts with no separator** (amends entry 60, which kept `-` and
  left it open). `AccountCoding::new_budget` is category 2, account 2, no sub-account, set
  3 and no separator, so numbers read `1101`, as the reference's exports write them. The
  starter's own accounts follow it (`1001` to `5001`, were `10-01` to `50-01`): left
  dashed, its numbers didn't follow its own pattern, and pasting accounts into a category
  picked a free-looking `2001` beside `20-01`, which the pattern reads as the same number.
  Existing files are unchanged: a file without `account_coding` still reads as 2 + 2 with
  `-`. A typed separator still splits (`11-01` is stored as `1101`). Lookups by number in
  edit scripts and the MCP line and account tools stay exact as before, so `21-01` doesn't
  find `2101` there (as `2101` never found `21-01`); Explain targets and imports read
  numbers through the pattern as before. Tests that name the starter's accounts with a dash
  use `starter::budget_with_separator('-')`.
- **The Fringe totals hint** (Settings) said charges don't apply to the budget's fringe
  row. Since entry 56 a percent charge counts it, so it now says so.
- **Import Setup carries the cutoff factor and the fringe row** as budget settings, like
  account numbering, fringe posting and new-line defaults: items `cutoff_factor` and
  `fringe_row`, kept as this budget has them unless the imported one is picked. The fringe
  row brings its number and description; its place stays this budget's, since it names a
  row of one topsheet (amends entry 60, "Import Setup doesn't carry it"). Setup packs and
  the setup library hold no budget setting, these two included.
- **Account credit rows** (R2-04; amends entry 57). Selecting an account and ticking a
  credit adds a row on that account: its gross is the account's lines plus the fringes
  they earn, wherever fringes post (1101: 4,000.00 of lines and 500.00 of fringes posted by
  budget, 4,500.00). Ours: credit row source `{"kind": "account", "account": "1101"}`
  (FORMAT.md, "Credit row"): the account's counted lines and adjustment rows plus the
  fringes they earn, less the credit's excluded fringes; of two accounts with the same
  number, the first. One the budget doesn't have counts 0, and `method check` and the MCP
  `validate` tool warn, as for a category row. Renumbering carries it. The credit editor's
  Spend menu lists the accounts after the categories ("Account: 1101 Crew"); `set_credit`
  in the app and over MCP takes it and refuses an account the budget doesn't have; setup
  sheets carry it in a Row Account column; Explain names the account. Changing an
  account's number by hand doesn't follow into credit rows, as it never did into charge
  exclusions or category rows.
- **Category fringe account numbers** (R2-07; amends entry 53). Under category posting
  the reference numbers a category's fringe account from the category's first
  category-width characters and the suffix: with category 2, account 2 and suffix 98,
  category `510` is `5198`, `63` is `6398`, `7000` is `7098`, and `1100` with 99 is `1199`.
  Ours took a number that isn't a plain category whole (`51098`) so two categories would
  never share one; now `AccountCoding::category_fringe_account` takes the first
  category-width letters and digits when the pattern can't read the number as a plain
  category, written with the pattern (`51-98` with `-`). Two categories whose numbers
  start alike now share a fringe account number (`0501` and `0502` are both `0599`), as in
  the reference; renumbering already warns when a fringe account's number is an account's.
  Plain categories (`11`, `1100`, `11-00`, `5` as `05`) are as before.
- **Flat fringes ignore ranges and cutoffs** (R2-06): ZFDAYR, 100.00 a Day with a 500.00
  cutoff, on two 3-Day lines in one range is 300.00 + 300.00, as ours already worked it
  (entry 54). Test added.
- **Fringe percents on a suppressed line** (R2-05, EC 15): the reference shows 0.00 for
  the effective percent and 0.00% for the full one; a suppressed line earns nothing either
  way, as ours. **Still different, display only**: our grid leaves the Fringe % cell blank
  on a suppressed line, a line without fringes and an adjustment row without fringes,
  where the reference writes 0.00%. Left for the owner.
- **Fringes in group totals set to Exclude** (R2-02): ZIN 8,000.00 / 5,000.00, ZIN2
  4,600.00 / 4,600.00, ZOFF 5,000.00 / 0.00, grand total unchanged: as ours (entries 57
  and 58). Test added.
- **Moving the budget's fringe row** (R2-01, PM-POS): above 2100 ZPROD it changed nothing
  (ATL 15,000.00, BTL 13,400.00, grand 34,125.00): display only, as ours (entry 60).
- **The full fringe percent** (the reference's optional Agg Sum column). The 2026-10-11
  exports rule out "the sum of the percent rates": a 0.04 line with a 12.5% fringe shows
  25% (0.005 rounds to 0.01) and a line with a flat fringe counts it (Grip days, 100.00 flat
  and 400.00 at 10% on 4,000.00, shows 12.5%). So it is the line's fringes with no cutoff
  and no range over its total: `RowTotals::fringe_full_total` is each percent fringe on the
  row's whole total, rounded on its own, plus the flat fringes as earned, and 0 on a row
  that doesn't earn (the reference shows 0.00% on a suppressed line, R2-05). A range's
  line past the cutoff earns 0.00 and is still 12.5% before cutoffs, as the exports show.
  The grid's optional **Full fringe %** column (hidden until chosen, like the reference's)
  shows it; MCP `get_account` rows carry `fringe_full_total_cents`; the reference engine
  works it out on its own. **Chosen, not checked**: each percent fringe is rounded before
  they are added up (no figure decided the order).
- **Frozen original totals** (the reference's "lock original totals for comparison",
  ZTEST12). The switch only freezes the original column: capturing it again is disabled
  while it is on. Ours: the baseline's `frozen` flag (FORMAT.md, "Baseline"), set by
  `set_baseline_frozen` from the Baseline tab's **Freeze the original totals**; while it is
  on, Lock again and Clear are refused (and off on the tab), and edits still move the
  variance. **Still different**: the reference's original totals exist from the start (at
  0 until captured); ours exist once locked, so the switch shows only then.
- **Currency formats** (inventory section 8, from the reference's documentation; no run
  read them). A currency has `symbol_after` (`1.234,56 €` rather than `€1.234,56`) and
  `separators` (`comma_point`, `1,234.56`, or `point_comma`, `1.234,56`), both display
  only and left out of the file at their defaults (`method_core::currency_format`,
  `Currency::amount_text`). Totals shown in another currency are written in its format:
  the topsheet's "Net total in" row, the printed topsheet's "Grand total in" and "Net
  total in" lines, and `method show`'s last line. The currency editor, `set_currency`,
  the MCP `add_setup` / `update_setup` and `list_setup`, setup sheets (Symbol After and
  Separators columns), diff, merge and Import Setup carry them. **Not done**: the
  reference writes every computed total in the totals currency's format; ours writes base
  figures as `1,234.56` everywhere, and spreadsheet exports keep numbers as numbers.
- **Changing the base currency** (R2-03, V08-3): the reference keeps every rate as typed,
  reads blank-currency lines as the new base (Base line 1,000.00 becomes 1,234.57 at
  1.23457) and converts other currencies through an extra hop (CAD 123,456.51, was .50).
  Ours renames the base (entry 32), so amounts don't move. Recorded; not built.
- **Next account at the end of a category** (R2-08) was not driven; EC 16 stays open.
- **The change log** (entry 62) records them: a currency's `symbol_after` and
  `separators` (always written, so going back to the default is a write; a log from before
  reads as the default; the two fields come after the others, so no earlier field's order
  changes), a credit row's `"account"` source (a reference when the account exists), and
  the baseline's `frozen` with the baseline.
- Tests: crates/method-core/tests/mmb_parity.rs (R2-02, R2-04 in every posting mode and
  both arithmetics, R2-05, R2-06, R2-07, the full fringe percent on the exports' figures,
  the new-budget separator), the coding, starter and currency-format unit tests, the
  format round trips (account rows, currency formats, a frozen baseline), the app's
  credit, currency, baseline and Import Setup tests, the MCP credit tool test, `method
  show`'s and the change log's (tests/log.rs).

## 62. The change log under the budget, as built (2026-10-12, the owner's direction)

The owner approved docs/research/collaboration.md ("do it"): a change log of our own under
the model, shared folders first, iroh later. Phases 1 and 2 are built on the engine side
(`method_core::log`, `method_format::log`, `method log`, `method sync`); the app, presence,
permissions and live sessions are not. Entry 61 is another agent's and unrelated.

- **The model** (`crates/method-core/src/log/`). Every entity (category, production total,
  account, row, note, info field, currency, global, unit, fringe, group, set, location,
  charge, excluded amount, credit, credit row, version, transaction, import batch, ETC
  override) has a 128-bit random id drawn by the actor that creates it, held in the log
  only; the `.mthd` stays format version 1 with no marker (design 4.7). Each field is a
  last-writer-wins register that keeps every write it ever saw (the blame index and what
  undo needs); tags (a row's fringes and groups, a charge's exclusions, a credit's excluded
  fringes, a transaction's tags) are add-wins sets with observed-remove; each listed entity
  has one move register holding (list, position), with positions from a Fugue tree per
  list (design 3.6: runs inserted at one spot by two people never interleave); the
  tombstone is a register too. A change is one undo step: actor, seq (from 1, no gaps),
  deps (a version vector), a Lamport clock, a wall-clock time for display, a label and
  the ops. Writes are ordered by `(lamport, actor, seq, index)`; wall clocks decide
  nothing. Changes are applied in causal order (a log holds the rest as pending); a
  malformed change is refused whole and the actor's count moves past it; a second change
  with the same (actor, seq) and other content is a fork and refused.
- **References bind by id, the file writes names** (design 3.7): a row's unit, currency,
  set, location, fringes and groups, a charge's excluded groups, categories and accounts,
  a credit row's group or category, a transaction's account, line and batch, an ETC
  override's account, and every global in a formula (`{g:<id>}` markers in the stored
  text, spacing kept). A rename or renumber is one write to the item's name or code, and
  everything follows; the capture takes hints from the rename and renumber commands
  (`Hint::rename`, `Hint::Recode`, `Hint::from_plan`) so it is one write and not a
  delete and an add. Without a hint it is still correct, only noisier.
- **Capture by diff** (design 3.12): `Log::record(before, after, hints, label, time)`
  compares the current projection with the edited budget, matching rows by id, accounts
  and categories by code (then by what they hold, then by hints), production totals by
  name and place, setup items, charges and credits by name, notes, versions, transactions
  and batches by id, ETC overrides by account, excluded amounts and credit rows by value.
  Order changes are the fewest moves (longest common subsequence). A deleted item
  re-added under its key (a row id, a name) is restored, not recreated. Property, in
  crates/method-core/tests/log.rs: replaying the captured change gives exactly the edited
  budget (25 random edits over 60 seeds: cell edits, inserts, deletes, moves within and
  across accounts, tags, fringe and global adds, deletes and renames, renumbers, account
  and category adds, deletes and moves, notes, title, versions).
- **The projection and its repairs** (design 3.10) are a pure function of the state, made
  on the way out and never written, each listed for review with a sentence: two live
  rows with one id (the later, by its creating op, shows with an id derived from that op
  in `[2^32, 2^53)`); two live items of one kind with one name (identical ones unify;
  otherwise the later shows as "FICA 2"); a deleted item still used by a live one, or a
  deleted account or category still holding a live row or account, comes back ("use
  revives"); account numbers that repeat or don't fit the numbering and global cycles are
  kept as written and flagged. Charge exclusions and credit category rows naming a deleted
  account or category are weak references (dropped, not reviving: an excluded account
  that is gone costs nothing). After a merge the log also reports a value two people
  changed at once (the later `(lamport, actor)` wins; the loser is shown, and the badge
  goes away when anyone writes the field knowing both), a row moved by two people, and
  an item deleted by one while another edited it (the delete wins). Convergence property,
  same test file: 2 to 4 actors making random edits apart for 4 rounds, exchanging changes
  in random orders per receiver (the pending buffer sorts dependencies), plus a newcomer
  replaying everything shuffled, all end with the same frontier, the same budget and the
  same review list, 40 seeds.
- **Per-person undo** (design 8): `Log::undo` records a new change that puts back my
  previous values only where mine is still the current one, field by field; a field
  someone changed after me is skipped and the skip is a sentence. Undoing an add deletes,
  undoing a delete restores, undoing a move points the entity back at its old position
  (an existing node); redo is the undo of the undo. Nobody can undo another actor's
  change. Property: undoing every change of a random run, newest first, gives the adopted
  budget back.
- **Choices the design left open, settled by its defaults** (section 12): the keeper is
  one computer, the owner's by default, and `method sync` acts as keeper on request (1);
  a department editor may add lines and notes in the categories or accounts granted, apply
  existing fringes and groups, and nothing else, enforced in phase 3 (2); switching a
  group in or out of the total is shared like any edit today, a personal overlay is a
  phase 5 extra (3); links and QR codes over the internet, short codes on the same network
  only (4); n0's public relays by default, configurable (5); no encryption at rest in the
  first release (6); history keeps the name each person typed (7); last writer wins with a
  badge, nothing blocks (8, built); a removed member's late work goes to the owner's
  rejected list (9); the log is a folder beside the file; committing it to git works
  (every segment is a new file) but isn't required (10); phases 1 and 2 ran alongside the
  parity work (11); proposals and (12) the join page (13) wait for their phases; no
  dependency was added: signatures come in phase 3 with a stable `ed25519-dalek` 2.x
  reviewed under the cooldown then, iroh in phase 4 (14).
- **Choices of our own.** Versions are stored materialized (the version's content as one
  register, as the file stores a version), not as frontiers: a restore or an outside copy
  can then be a version too, the projection never replays the log, and compaction has
  nothing to materialize; the frontier form stays a possible optimization. Actuals are
  records (transactions and batches by id, ETC by account), so two accountants' imports
  merge. Tools and compound settings (display, report, tag and line presets, overtime
  patterns, setup order, folders and sources, the baseline, account coding, fringe row,
  group rules, new-line defaults) are one register each, as the design allows. The engine
  reads no clock and no randomness for budget content, as before; ids come from a seeded
  generator (`Ids::from_os` seeds it from the standard library's randomly keyed hasher,
  `Ids::seeded` for tests), and SHA-256 is written in `method-core` (dependency-free) for
  derived ids, segment hashes and the budget id. A one-cell edit on a 5,000-row budget
  captures in about 140 ms and the projection takes about 120 ms in a debug build;
  caching the projection in the session is app work.
- **On disk** (`crates/method-format/src/log.rs`; docs/FORMAT.md, "The change log"):
  `Feature.mthd-log/budget.json` and `actors/<actor>/<from>-<to>.chg`, one folder per
  computer written by it only, sealed segments (a header line with the actor, range and
  the SHA-256 of the body; one canonical JSON line per change) written through a `.~`
  temporary and a rename, never appended to, rolled into one by their owner on request.
  A half-synced segment fails its hash and is left until whole. `load`, `absorb` (apply
  what a scan finds new), `flush` (write this actor's unwritten changes), `ingest` (an
  outside edit to the `.mthd` becomes a change by this actor, design 4.4),
  `write_snapshot` (the keeper writes the merged budget as the `.mthd`, keeping the file's
  provenance) and `roll` are the hooks an application polls with; nothing watches files on
  its own. Unsigned: `sig` is reserved; state checkpoints and the archive (design 4.5)
  are not built, a newcomer replays every segment.
- **Command line**: `method log FILE [--init] [--last N] [--json]` and `method sync
  FILE|FOLDER [--no-write] [--roll]` (docs/CLI.md). The command line's actor id lives in
  `~/.method/actor` (or `$METHOD_ACTOR_FILE`), 64 hex characters, made on first use and
  readable by its owner only; in phase 3 it becomes a key pair.
- **Not built, in order**: the app's Session recording every edit and absorbing the
  folder (projection caching, random row ids in shared budgets through
  `log::ids::random_row_id`, hints from its rename and renumber commands); presence
  heartbeats and the status item; the review panel, badges and Who Changed This
  (`Log::blame`); signatures, membership and scopes; checkpoints and the archive; iroh.

## 63. The start window and New Budget, phase 1 (2026-10-12)

Roadmap 3.19. The owner likes the reference product's home screen minus its cloud parts.
What it offers was read from its public manual only and is described in our own words in
docs/research/new-project-window.md (with the pages cited), which also has the later phases
and the open questions. Phase 1 is built (docs/APP.md, "The start window and New Budget").

- **A budget is a file, a project is a folder.** No database or index that can drift from
  the disk: the window shows the pinned budgets (the app's own `start.json`, at most 100)
  and Open Recent's list, each read from its file (title, net total, base currency) and
  kept until the file's modified time or size changes; a budget open in a tab is summed
  from the tab. Projects as folders, thumbnails and sorting are phase 2 and later.
- **When it shows**: at launch when nothing was reopened, no file was handed over and no
  recovered budget is offered (over the untouched Untitled budget, which gives its tab up
  to whatever is opened or made, as Open does), and from File > New… (Cmd+N, renamed from
  New) and the toolbar's New. New Tab (Cmd+T) and the tab strip's **+** keep making a
  starter budget at once. A box in the window turns the launch showing off (local storage,
  per user). It is a dialog in the main window, not a second window: the dialog code
  already keeps the page behind inert and handles Escape and focus, and the menus keep
  working (New and Open are answered by the window while it shows).
- **New Budget is one form** where only the title is needed: type, season, episode or
  episode count, four dates, schedule length, location, currency, start from, fringe
  posting and arithmetic. Everything goes through the setup tabs' own edit functions
  (`edit::set_info`, `set_global`, `set_base_currency`, `set_fringe_posting`,
  `set_arithmetic`), so nothing new can be refused differently there. The details go into
  budget info under plain names of our own (Production type, Season, Episode, Episodes,
  Prep start, Shoot start, Shoot end, Wrap end, Country, Region, City); the schedule's
  length into the starter's globals `PREP_WEEKS`, `SHOOT_DAYS`, `WRAP_WEEKS`. Dates are
  never turned into day counts (shoot days depend on the week and holidays; a schedule
  link, roadmap 3.8, will fill them).
- **Start from** the starter, an empty budget, a bundled chart (decision 51) or a picked
  budget file, the last two stripped as New From Template strips (decision 29). Until Start
  From is changed by hand, the type picks our own chart for it.
- **The new budget opens with no file**, as one undo step ("New budget"), so closing it
  asks to save and an Open doesn't replace it. Saving into a project folder at creation
  is phase 2.
- **Paths stay in Rust**: rows by a token of the last listing, the picked file by its
  token, as the Packs list and the importers do.
- **Not built**: projects as folders, thumbnails, Save In at creation, setup library and
  packs at creation, rates (3.9), the schedule link (3.8), shared-folder badges and
  presence (3.18), the start window when the last tab closes.
