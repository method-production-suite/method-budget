# Accessibility audit (roadmap 3.14, first slice)

2026-10-18. What a screen reader, a keyboard-only user and someone who needs contrast get
from the main window, and what is left. Checked in the browser build in headless Chrome
(its accessibility tree, the way `scripts/web-smoke.mjs` now checks it) and by reading the
page code; no screen reader was run. Decision 82 in `docs/decisions.md` records the
choices.

Key: **built** (there before this slice), **fixed here**, **left** (not done yet).

## The budget grid

- [x] Built: every action has a key (docs/APP.md, "Keyboard and menus"); the grid keeps
  focus in one place and moves a cursor cell, with a visible ring (accent, 2 px) while it
  has focus and a grey one when it doesn't.
- [x] Fixed here: the grid is an ARIA grid. The focused host is `role="grid"` (named
  "Budget grid", multi-select), with `aria-activedescendant` on the cursor cell, so a
  screen reader follows the cursor without focus moving into cells. Rows are `row`, cells
  `gridcell`, the description column `rowheader`, headers `columnheader`.
- [x] Fixed here: places for long grids. `aria-rowcount` and `aria-colcount` on the grid,
  `aria-rowindex` on each row and `aria-colindex` on each cell, so a grid drawing only the
  rows near the visible ones (over 150 rows) still says "row 212 of 400".
- [x] Fixed here: states. Totals and other computed cells are `aria-readonly`; every cell
  of a selectable row is `aria-selected` true or false: true in a selected range or a
  picked row, false otherwise, the cursor on its own included (review follow-up).
- [x] Review follow-up: each cell has its own id from its row key and column key, so
  `aria-activedescendant` changes on every move and keeps naming the same cell across a
  redraw or a scroll.
- [x] Fixed here: the cursor cell's name is what it shows, its column (the full name when
  the label is short: "X (multiplier)", "Currency") and the row's description:
  "45.00, Rate, Gaffer". A blank cell says "blank"; a cell with a problem adds it. The cell
  editor is named by its column and row too.
- [ ] Left: the right-click menu, the type-ahead lists and the column menu were not
  re-checked with a screen reader in this slice (their roles were already set:
  `menu`, `listbox` and `option`).

## Totals and messages

- [x] Built: the toast is `role="alert"`; the update notice, the banners and Find's
  result count are `role="status"`; form errors are `role="alert"`.
- [x] Fixed here: an edit that moves the net total is announced politely, after things
  settle (700 ms; a run of quick edits is one sentence): "Net total 2,437,299.45, up
  26,317.51". A hidden live region outside the app (`#announce`) carries it, so an open
  dialog doesn't silence it. An edit that moves no money says nothing.
- [ ] Left: the footer's account and category totals and the selection totals are not
  announced; a screen reader reads them by moving to the footer.

## Toolbar, tabs and panels

- [x] Built: toolbar buttons have text names; groups are labelled ("File", "History");
  Tools says `aria-pressed`; the budget tabs, the tools panel's tabs and Budget Setup's
  tabs are `tablist`/`tab`/`tabpanel` with arrow keys, Home and End; the panel's edge is a
  focusable `separator` with its value.
- [x] Built: icon-only controls are labelled: the ‹ › account steps, the tab strip's
  arrows and +, Find's previous and next, the report preview's page buttons, the
  convert-banner's close, the twisties and the conflict badge. A tab's × is mouse only
  (Cmd+W closes a tab) and hidden from screen readers on purpose.
- [x] Fixed here (checked, not changed): no control on the topsheet or a detail grid lacks
  an accessible name; the smoke test now fails if one does.

## Dialogs

- [x] Built: one dialog at a time; the page behind goes inert (Tab can't leave, keys can't
  reach the grid); Escape cancels; focus goes back where it was. Most dialogs (Versions,
  the start window, imports, Report Setup, the shortcut sheet) were already
  `role="dialog"`, `aria-modal`, labelled by their title.
- [x] Fixed here: the common small dialogs (rename, add a category, confirm) had no role:
  they are now `role="dialog"`, `aria-modal="true"`, labelled by the title and described
  by the message. The popovers by a cell or a panel item say `aria-modal` too, and every
  dialog opened through `customDialog` gets `role="dialog"` and `aria-modal` if its
  builder didn't set them (Compare, What Changed, Merge and the unsaved-changes question
  lacked `aria-modal`).
- [ ] Left: when the element that opened a dialog was redrawn while it was open, focus
  goes back to the page rather than to its replacement. In the browser build, Budget Setup
  is a frame over the page without dialog semantics (the app's own window is fine).

## Visible focus and contrast

- [x] Built: `:focus-visible` is a 2 px accent outline everywhere (5.2:1 on the page in
  light, 8.5:1 in dark, above the 3:1 a focus indicator needs); text fields and the cell
  editor have the same ring.
- [x] Fixed here: secondary text (`--text-muted`) was 4.35:1 on table heads, 4.19:1 on a
  selected row in light and 4.18:1 there in dark, under the 4.5:1 body text needs. It now
  uses BRAND.md's muted (#59626D light, 5.3:1 or better on every surface; #9AA3AD dark,
  4.85:1 or better).
- [x] Fixed here: the note amber (`--note`) in light was #D29A12, 2.5:1 on white, and it
  is used for text in the import reviews and for the note mark on cells. It now uses
  BRAND.md's pencil amber, #8A5A00 (5.9:1). The share panel's "warn" text uses it too.
- [x] Fixed here: a focused item in a popup menu showed only the selection tint (1.1:1
  against the menu); it now also has the accent ring.
- [ ] Left: the grid's own focus is shown by its cursor ring; an empty grid (an account
  with no lines) shows focus only by a faint border. The sharing palette's green and amber
  dots are below 3:1 in light but always sit beside a text label. Other per-file colors
  were not swept one by one.

## Not checked in this slice

- A real screen reader: VoiceOver on macOS, Narrator or NVDA on Windows, Orca on Linux.
  The tree is right in Chrome; what each reader says in each system web view needs hands
  (listed for the owner in docs/ROADMAP.md).
- Windows high contrast (`forced-colors`) and reduced motion.
- The printed reports and the spreadsheet export (not screen content).

## Localization: where the words live

The framework is built and English is its only language (decision 94; how it works:
docs/APP.md, "Languages"). Adding a language is a table: `ui/i18n/<code>.js`, imported in
`ui/js/i18n.js`. Which languages come first is the owner's call (docs/ROADMAP.md,
"Waiting on the owner").

What the slice built, against the list this audit gave:

1. **A string table and a lookup, no new dependency**: built. `ui/i18n/en.js` (about
   1,200 entries, 41 of them with plural forms) and `t(key, params)` in `ui/js/i18n.js`,
   with `{name}` filled in, plural forms picked by `Intl.PluralRules`, English for a key a
   language lacks (with a console warning) and the system's language picked, English by
   default. The table loads as one of the page's own scripts under its
   Content-Security-Policy; the browser build carries it.
2. **Strings moved to the table, by area, with a test that fails on a new inline string**:
   partly. Converted: the start window and New Budget (`start.js`, `startlist.js`), the Help
   menu's page parts (`help.js`, `licenses.js`, `updates.js`), the shortcut list
   (`shortcuts.js`), Budget Setup's frame and its list, Globals, Units and Rate Tables
   tabs (`setupwin.js`, `setuptabs.js`, `setup.js`, `setuplist.js`, `setuplists.js`,
   `setupbands.js`, `setupframe.js`, `apply.js`, `rates.js`), Find and Replace
   (`find.js`), the main window's dialogs, toolbar tips, banners and messages
   (`main.js`), the common dialog buttons (`modal.js`) and the status bar (`footer.js`).
   The wording is unchanged; plurals written out ("1 row", "3 rows") became plural
   entries. `src/i18n.rs` fails on a new `text:`, `title:`, `label:`, `placeholder:`,
   `message:`, `hint:`, `aria-label` or `toast()` literal with letters in a converted
   script, on a key missing from the table and on an entry nothing uses.
3. **Engine messages with stable codes**: not started. Validation errors, problems in
   cells and import reviews still come from `method-core`, `method-format` and
   `method-session` as English sentences.
4. **The menu bar and native dialogs reading the same table**: the menu bar is built
   (`src/menu.rs` takes every item's words from `menu.<id>`, and its menus' names, the app
   menu, Open Recent, Autosave, Revert to Backup and Check for Updates from the same
   table; the test matching it to `shortcuts.js` still holds). The native dialogs
   (unsaved changes on quit, file pickers' titles) are still English in Rust.
5. **Room for longer words and a right-to-left check**: waits for a language.
6. **A choice of language**: the system's, picked at start; no override in the
   preferences yet (there is nothing to choose between).

Still English inline, for the next passes:

- **Scripts kept out of this slice** while other work on them is open: `share.js`,
  `activity.js`, `topsheet-setup.js` (Budget Setup's Charges, Credits, Info, Settings and
  Baseline tabs), `report.js` and `reportsetup.js`.
- **The rest of the page**: the tools panel's other tabs and dialogs (`library.js`,
  `packs.js`, `notes.js`, `history.js`, `presets.js`, `explain.js`, `overtime.js`,
  `actuals.js`, `actualsimport.js`, `comments.js`, `review.js`, `compare.js`,
  `diffmerge.js`, `versions.js`, `importsetup.js`, `sheetimport.js`, `coaimport.js`,
  `exchangeimport.js`, `scheduledays.js`, `sharing.js`, `context.js`, `levels.js`,
  `grid.js` and the rest), `util.js`'s shared phrases (a flat fringe's count words, a
  setup pack's source), the toolbar in `ui/index.html`, and words that come from Rust as
  data (a start-window row's kind, the chart groups).
- **The engine's messages** (item 3) and **the native dialogs** (item 4).
- **Elsewhere**: the command line, the MCP tools, the website (`site/`), the release
  notes and the README are English and stay so for now.

Numbers use the budget's own decimal and grouping settings (`display.js`); dates in
History, Notes and Activity already follow the system locale (`toLocaleDateString`).
