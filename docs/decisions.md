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

Extended the same day (Sam): Cmd+minus inserts an adjustment (Option+Cmd+A still works),
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

## 19. Four product calls from the roadmap (2026-10-09, Sam)

- **Autosave writes the file itself**, on by default with a visible toggle. Untitled budgets
  get a recovery copy until first saved, and every document keeps rolling backups so an
  autosaved mistake can be walked back.
- **Setup shortcuts match the reference product** (e.g. Cmd+G globals, Cmd+/ charges); our
  shortcut sheet moves to Shift+Cmd+/.
- **Several budgets open as tabs in one window**, each with its own undo; split view later.
- **Compare shows both** each budget's difference from the base and optional
  Sum / Average / Variance columns in the reference's style.

## 20. Autosave into the file, recovery copies, backups (2026-10-09, autosave default by Sam)

- **Autosave is on by default and writes the budget file itself** (Sam's call), 3 seconds
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
- **Backups go in the app's data folder, not beside the budget** (`film.fwb~` was the
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

## 21. Tools panel layout (2026-10-09, Sam)

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
- **Paste adds what is missing and keeps the target's version of what differs** (Sam's
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
- **What a tab shows.** The file's name without `.fwb`, or the budget's title before it has
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
  `.fwb` in Rust's Open dialog (a file open in a tab is read from the tab, unsaved changes
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
- **A template is just a budget.** New From Template… opens any `.fwb` in a new tab with its
  chart (categories, accounts, production totals), setup, presets and settings, and without
  its detail lines, budget info, baseline, versions and line notes; "keep the detail lines"
  keeps the lines without their notes. It is titled Untitled and has no file, so the first
  Save asks where, and like a New budget it starts with no unsaved changes. Save as
  Template… writes the budget in front, stripped the same way, to a new file named in Rust's
  Save dialog and titled after it; the budget and its own file don't change, and a file open
  in a tab is refused. No template format, folder or built-in templates: our own starter
  budget (`fwb_core::starter`) remains what New opens.

## 30. Spreadsheet export as built (2026-10-09)

Roadmap 2.1 (inventory section 21: what accounting and payroll tools take in):

- **One set of tables, three writers.** `fwb_format::export` turns a budget and its report
  into plain tables (Topsheet, Accounts, Detail, Globals, Fringes, Groups, Units,
  Currencies, Sets, Locations); the Excel, CSV and tab-delimited writers only serialize
  them, so every format carries the same figures. It lives in `fwb-format` so `fwb-core`
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
  as blank. The app takes the tables from its cached recalc; `fwb export` recalculates.
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
  may not expect one); `fwb export --bom` adds it on request.
- **Files.** CSV is one file per table, `<name> - <Table>.csv` beside the name chosen in
  the Save dialog (one dialog, predictable names), instead of a folder picker. The path
  always comes from the Save dialog, the format's extension is forced, each file is written
  atomically, and an export never replaces a budget open in a tab; `fwb export` refuses to
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
  does it (`fwb_core::rename`): globals token by token the way the formula reader splits
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
- **A global in a header follows a rename** of that global (`fwb_core::rename`, counted
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
reuse: nothing here guesses the reference product's export layouts (that waits on Sam's
export test).

- **Our own readers, no crates.** An .xlsx is a ZIP of XML, so `fwb_format::import` reads
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

- **A library is a budget file**, not a new file type: `My Film Wizard Library.fwb` made in
  a folder the user picks in Rust's folder dialog, or any `.fwb` picked in the Open dialog.
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
  engine's own charge code (`fwb_core::charge_preview`), without a recalc: one pass over the
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
  (`fwb_core::renumber::plan`): every category, account and category fringe account
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
  to a new file suggested as `<title> - <version> - <date>.fwb`, optionally without
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

- **One module per subcommand.** `fwb` is a tiny dispatcher (`src/main.rs`, a table of
  name, usage and function) over `src/cmd/<name>.rs`, with shared flag parsing, exit
  codes and atomic writes in `src/cli.rs`, so new commands (diff, merge, explain,
  actuals) are one file and one table entry each. It depends on `fwb-core` and
  `fwb-format` only, never the app, and adds no crates (JSON output is written by hand).
- **Exit codes are a contract** (docs/CLI.md): 0 ok, 1 failed, 2 usage, 3 edit refused
  with nothing written. Every file a command writes goes through a temporary file and a
  rename.
- **Headless reports reuse the export's figures.** `fwb report` builds sections from a
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
- **Edits are a line-oriented script, not a query language.** `fwb apply` runs commands
  (`set global`, `set fringe ... rate|cutoff`, `set line ROW FIELD VALUE`, `add`/`remove
  tag`, `rename`, `apply`/`remove charge`, `set title`, `set compact_versions`) against a
  copy; every line must succeed and the result must recalculate, or nothing is written.
  Rows are named by id (`#17`, stable) or by account and position (`21-01:3`, as the grid
  numbers them; `21-01:*` for every line). `fwb set` is one such command from a path
  (`line.21-01:3.rate`). Renames call the engine's own rename (decision 32). `remove
  charge` stops a charge counting rather than deleting it, so it can be applied again.
  `--dry-run` prints `fwb diff`'s summary of what would change.
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
- **Not yet**: a switch for `compact_versions` in the app (the CLI's `fwb set ...
  compact_versions true` turns it on; the app keeps it once set); the PDF's proportional
  fonts, shading and the topsheet page column; scripted adding and deleting of rows and
  accounts; a per-command JSON output for `set` and `apply`.

## 42. The Budget Setup window and a compact tools panel (2026-10-09, Sam)

Sam's live testing found the panel's Budget tab (charges, credits, info, settings,
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

- **Explain this number** (`fwb_core::explain`, src/explain.rs, ui/js/explain.js,
  `fwb explain`). Any total (a line, a subtotal row, a line's fringes or one fringe on it,
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
  allocated cents; each flat fringe once per line or per unit with the hours conversion,
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
  out the target goes with the panel's state. `fwb explain file.fwb <total>` names totals
  as `21-01`, `21-01/lines`, `21-01/fringes`, `21`, `21/fringes`, `21-01:3`, `#ID`,
  `level:`, `charge:`, `credit:`, `fringe:`, `atl`, `btl`, `atl+btl`, `fringes`, `credits`,
  `grand` (or `topsheet`), `net`; `--depth N` cuts the printout.
- **Overtime helper** (`fwb_core::overtime`, src/overtime.rs, ui/js/overtime.js). User
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
  typed, tags as `+FICA -WGA`, notes as counts. The CLI, `fwb apply --dry-run` and the app
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
- **git**: `fwb merge-driver %O %A %B %P` writes a valid budget over ours even with
  conflicts (settled for ours, listed on stderr) and exits 1, so git marks the file
  conflicted without conflict markers that would make it unreadable; it is opened in the
  app and settled there. An empty base (added on both branches) is refused. `fwb merge`
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
  (`fwb_format::import::actuals`): date, account, amount or debit and credit, vendor,
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
  Cost Report and Actuals sheets in File > Export and `fwb export` (both after the ten
  standard sheets, so positional readers keep theirs).
- **Not yet**: entering a transaction by hand, ETC overrides on charges, purchase orders and
  commitments, actuals by period (a cost report as of a date), matching a re-import whose
  unmatched numbers were mapped differently the first time (the duplicate key compares the
  account the transaction ended up in).
