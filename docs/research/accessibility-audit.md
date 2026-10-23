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
   4,370 entries, 196 of them with plural forms) and `t(key, params)` in `ui/js/i18n.js`,
   with `{name}` filled in, plural forms picked by `Intl.PluralRules`, English for a key a
   language lacks (with a console warning) and the system's language picked, English by
   default. The table loads as one of the page's own scripts under its
   Content-Security-Policy; the browser build carries it.
2. **Strings moved to the table, by area, with a test that fails on a new inline string**:
   built, for the page. Converted: the start window and New Budget (`start.js`, `startlist.js`), the Help
   menu's page parts (`help.js`, `licenses.js`, `updates.js`), the shortcut list
   (`shortcuts.js`), Budget Setup's frame and its list, Globals, Units and Rate Tables
   tabs (`setupwin.js`, `setuptabs.js`, `setup.js`, `setuplist.js`, `setuplists.js`,
   `setupbands.js`, `setupframe.js`, `apply.js`, `rates.js`), Find and Replace
   (`find.js`), the main window's dialogs, toolbar tips, banners and messages
   (`main.js`), the common dialog buttons (`modal.js`) and the status bar (`footer.js`);
   in the second pass, sharing (`share.js`), the Activity tab (`activity.js`), Budget
   Setup's Charges, Credits, Info, Settings and Baseline tabs (`topsheet-setup.js`), the
   printed reports (`report.js`) and the report setup (`reportsetup.js`); in the third
   pass, the tools panel's frame and window (`tools.js`, `toolswin.js`), Notes, History,
   Versions, the Library, Review and Compare (`notes.js`, `history.js`, `versions.js`,
   `library.js`, `review.js`, `compare.js`), the tab strips and split view (`tabs.js`,
   `pane.js`), the grid's right-click menu (`context.js`), `util.js`'s shared phrases and
   the toolbar and fixed words of `ui/index.html` (keyed with `data-i18n` attributes that
   `fillPage()` fills at boot); and in the fourth pass the rest: setup packs, presets,
   Explain, Overtime, actuals and their import, comments, Import Setup and the templates
   dialog, the spreadsheet, chart, budget-export and schedule-day imports, the rate
   picker, Rename and Renumber, What Changed and Merge, sharing's banners, the grid's
   levels, quick entry, row colors and autocomplete, backups and recovery, the display
   settings and announcements, pasting and the small scripts (`packs.js`, `presets.js`,
   `explain.js`, `overtime.js`, `actuals.js`, `actualsimport.js`, `comments.js`,
   `importsetup.js`, `sheetimport.js`, `coaimport.js`, `exchangeimport.js`,
   `scheduledays.js`, `ratepick.js`, `rename.js`, `renumber.js`, `diffmerge.js`,
   `sharing.js`, `levels.js`, `quickentry.js`, `rowformat.js`, `autocomplete.js`,
   `safety.js`, `display-settings.js`, `display.js`, `convertbanner.js`, `clipboard.js`,
   `order.js`, `panelsize.js`, `menu.js`, `api.js`). Every script in `ui/js` is now on the
   test's list, the ones with no words too, and a script left off it fails the test. The
   flat fringe's count words and the review's "to add" are whole sentences now, and
   `util.js`'s `plural()` is gone. The wording is unchanged; plurals written out ("1 row",
   "3 rows") became plural entries. `src/i18n.rs` fails on a new `text:`, `title:`, `label:`, `placeholder:`,
   `message:`, `hint:`, `aria-label` or `toast()` literal with letters in a converted
   script, on a word in `ui/index.html` with no key or English that differs from the
   table's, on a key missing from the table and on an entry nothing uses.
3. **Engine messages with stable codes**: first and second slices built (decision 94,
   fifth and sixth passes).
   `method-session/src/messages.rs` is the engine's catalog: each message a code
   (`engine.edit.row_gone`) and its English with `{name}` parameters, made at its source
   with `msg()`, so the command line, the MCP server and the browser build print the same
   English. The app recognizes the code and parameters in an error
   (`messages::recognize()`) and shows the table's words for it (`i18n::engine()`),
   falling back to the text. Converted: every refusal and validation error of the edits
   (`edit.rs`, `edit/typed.rs`, the cell batch's limit): 97 messages and 34 nouns they
   name. The second slice converted Find and Replace, presets, apply, pasting, moving rows,
   setup lists, tabs and panes, the session's read-only and review refusals and "That step
   is no longer in the history.", Review and its settle summary, What Changed and Merge's
   refusals and side labels, actuals, the imports and their summaries (spreadsheets,
   charts of accounts, budget exports, schedule days with their notes and problems) and
   Explain's refusal (214 messages and 17 nouns), and catalogued method-core's own
   reasons (`engine.core.*`, 14: a formula's divide by zero, unknown global and number too
   large, a number that doesn't read, the numbering pattern's errors) without method-core
   reading a table. Summaries are messages joined by a space and count phrases
   (`engine.phrase.*`) only recognized inside them, so the generic templates don't
   swallow other text. The desktop's own words read the table: sharing's refusals, its
   write errors and invite-link reasons (`src/shared.rs`, 70 `shared.*` entries). Still
   sentences: the undo labels (Edit menu, History tab), Explain's steps, What Changed's
   change sentences and Merge's conflict sentences (method-core), method-core's formula
   parse and calculation errors (left out because their templates would win over the
   message they sit in) and its other errors, method-format's import problems, the
   Activity tab's reasons a change was left out, and the method-session modules not yet
   converted (notes, versions, the Library, packs, rates, overtime, Compare, reports,
   rename, renumber).
4. **The menu bar and native dialogs reading the same table**: the menu bar is built
   (`src/menu.rs` takes every item's words from `menu.<id>`, and its menus' names, the app
   menu, Open Recent, Autosave, Revert to Backup and Check for Updates from the same
   table; the test matching it to `shortcuts.js` still holds). The native dialogs are
   built too: every file dialog's title and file-type names, the unsaved-changes question
   with its buttons, Couldn't save and the recovered budget's Replace question come from
   `dialog.*` through `tr()`, and so do the window titles (Tools, Budget Setup, the main
   window's name with "(edited)", `window.*`). Left: the budget-export dialog's per-format
   file-type names (from method-format).
5. **Room for longer words and a right-to-left check**: waits for a language.
6. **A choice of language**: the system's, picked at start; no override in the
   preferences yet (there is nothing to choose between).

Still English, and why:

- **Words that come from Rust as data**: the engine's messages outside the edits (item
  3), import problems and warnings, Explain's steps, a diff's conflict text, sharing's
  refusals and file errors. The page shows them as they arrive; a chart group's name and a diff's change kind,
  which arrive as ids, are worded through the table.
- **Inline on purpose**: key names in shortcut text (`dom.js`'s "Enter", "Ctrl", as in
  `shortcuts.js`), the "Untitled" file name that matches Rust's, the page's `<title>`
  (the product's name, in `ui/index.html`), and the separators that join list items
  (" · ", ", ").
- **Pieces a translator may want whole**: the overtime band row's words around its
  inputs ("then", "hours at"), the schedule-day summary's count phrases joined with
  commas, a kind's name filled into sentences as `{what}` (Rename's "Enter a {what}
  name.", the paste review's "{action}"), and a few long tooltips put together as lines,
  each line an entry. Languages with genders or other word orders may need these split
  into whole entries when they arrive.
- **The rest of the engine's messages** (item 3) and **two window titles** (item 4).
- **Elsewhere**: the command line, the MCP tools, the website (`site/`), the release
  notes and the README are English and stay so for now.

Numbers use the budget's own decimal and grouping settings (`display.js`); dates in
History, Notes and Activity already follow the system locale (`toLocaleDateString`).
