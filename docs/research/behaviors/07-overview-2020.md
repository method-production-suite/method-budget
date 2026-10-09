# Behavior note 07: product overview (2020 relaunch)

Source: official vendor tutorial video "Overview" (published 2020-03-04); vendor tutorial,
2020/2021 era. Clean-room: behavior in our own words, no narration transcribed, no frames
kept, no budget data or setup data copied. A fast tour, so each item is shallow; later notes
go deeper.

## Behaviors shown

- **Keys kept from the old product.** The relaunch was pitched on keeping the keystrokes and
  shortcut keys long-time users already had. Confirms decision 15: shortcut familiarity is
  a real switching cost.
- **Start screen.** A budget browser opens first: budgets in folders, a "shared with me"
  section, an archive, last-opened dates, and an import action for files from the previous
  generation of the product.
- **Setup window.** One setup window with a tab per table (locations, fringes, globals,
  groups, currency, units, sets, credits, charges). Lists can be organized into user
  categories (folders). A personal library ("cloud") holds setup items saved from other
  budgets; pulling an item in also offers to bring in the items it depends on.
- **Live setup.** Changing a global's value in setup updates every detail line that uses it
  and every total at once, with no save or recalculate step.
- **Autosave.** Changes save as you go.
- **Breadcrumb navigation** with jump lists (note 03).
- **Tools pane** beside the grid, movable to the bottom with a button (note 06). Applying
  fringes and groups to lines updates totals live.
- **Create setup items from the cell.** Typing a name that doesn't exist yet in a
  currency, unit, location, set or global cell offers to add it to setup right there: a
  small prompt says the code isn't defined and offers cancel or add. Type-ahead suggests
  existing entries as you type (note 02 covers units and globals in the 2026 build).
- **Detail grid with a location column first** in one demo: columns are configurable per
  budget view.
- **Notes on any line,** written in a notes tab of the tools pane with save and cancel; the
  line shows a note marker.
- **Reports.** Pick categories and accounts to include, add extra reports and order them,
  add or remove columns from a column menu (fringe, original, variance seen), page header
  and footer built by dragging field tags (budget name, page number, dates), what you see is
  what prints, export to PDF.
- **Sharing** a copy of a budget with other users by email, with a sent-status list.

## Shortcuts seen

None beyond the claim that the old keys still work.

Era caution: launch-era (March 2020) tour on Windows. Cloud library, sharing, account
sign-in and the start screen are online features; they matter for parity of feel but are
outside our local, no-network design (decision 14).

## What this means for Film Wizard

1. **Live recalculation on any setup change** [have]. Every command returns a fully
   recalculated view.
2. **Create unknown setup items from the cell** [missing] P1. Today an unknown currency in a
   cell is refused ("add it under Currencies first"). Offer an inline "add it" prompt for
   units, globals, sets, locations and currencies, then apply the edit, as one undo step.
3. **Type-ahead in setup-backed cells** [missing] P1. Same item as note 02 #1; the 2020 tour
   shows it was a headline feature from the start.
4. **Line notes edited in a pane** [partial] P2. We store and show notes (decision 16);
   check there is a comfortable way to write a long note (a notes panel tied to the
   selected line, not only a dialog).
5. **Autosave** [missing] P2. Local files make this a choice: autosave to the file, or to a
   recovery copy beside it. Needs a decision (it changes what Undo and "dirty" mean).
6. **Report builder: choose sections, extra reports, columns, header and footer fields**
   [partial] P2. We print topsheet and detail with variance; selecting which categories
   and accounts to print, column toggles and header/footer fields are not there.
7. **Personal setup library across budgets** [missing] P2. Local equivalent: a user library
   file of fringes, globals, groups and units, with "bring dependencies too" on import.
   Must hold only the user's own items, never bundled vendor rate data.
8. **Start screen with recent budgets and folders** [partial] P3. We rely on the OS file
   dialog; a recent-files list is a cheap win.
9. **Import from the previous product generation** [missing] P3. Only via open export
   formats (CLEAN-ROOM.md); native formats are out of scope.
10. **Sharing and cloud** [missing] P3. Out of scope by design; files can be shared as files.
