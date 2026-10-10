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
