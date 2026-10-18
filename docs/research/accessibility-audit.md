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
- [x] Fixed here: states. Totals and other computed cells are `aria-readonly`; the cursor
  cell and every cell in a selected range are `aria-selected`.
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

## Localization: where the words live today

Nothing is translated, and this slice starts no translation framework. For a later slice:

- **Page text** is written inline in the UI scripts (`crates/method-app/ui/js/*.js`, about
  730 `text:`, `title:` and `aria-label` strings, plus sentences built with template
  strings, many with plural forms written out: "1 row", "3 rows") and in `ui/index.html`.
- **Shortcut names** live in `ui/js/shortcuts.js`; the **menu bar** in
  `crates/method-app/src/menu.rs` (Rust, matched to shortcuts.js by a test).
- **Messages from the engine** (validation errors, problems in cells, import reviews) are
  English strings from `method-core`, `method-format` and `method-session`, shown as they
  come. Native dialogs (unsaved changes on quit) are built in Rust.
- **Numbers** use the budget's own decimal and grouping settings (`display.js`); **dates**
  in History, Notes and Activity already follow the system locale (`toLocaleDateString`).
- **Elsewhere**: the command line, the MCP tools, the website (`site/`), the release notes
  and the README are English.

What a localization slice would need:

1. A string table per language and a small lookup (`t('key', {count})`) in the page, with
   plural rules from the platform's `Intl.PluralRules`, so no new dependency.
2. Every inline string moved to the table, in passes by area, with a test that fails on a
   new inline string.
3. Engine messages given stable codes with their words looked up in the page (or a Rust
   table for the CLI and native dialogs).
4. The menu bar and native dialogs reading the same table from Rust.
5. Layout room for longer words (toolbar, tab strips, column heads) and a right-to-left
   check if such a language is picked.
6. A choice of language: the system's, with an override in the preferences.

Which languages come first is the owner's call (docs/ROADMAP.md, "Waiting on the owner").
