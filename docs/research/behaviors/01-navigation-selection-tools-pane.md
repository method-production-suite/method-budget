# Behavior note 01: navigation, selection and the tools pane

Source: a ~2 minute screen recording by Sam of normal use of a licensed copy of the reference
product (2026-10-09). Clean-room: this note describes behavior in our own words. It holds no
budget data, labels copied verbatim beyond a few-word term, or images. The recording itself
stays outside the repo.

## Navigation between levels

- Three levels: topsheet, account list for one category, detail lines for one account.
  Users drill down by clicking a row's leading cell and go back up by clicking an earlier
  breadcrumb segment.
- The breadcrumb shows every level. Each segment has a dropdown that jumps straight to a
  sibling (another category or another account) without going up first.
- Next to the current breadcrumb segment are two small step buttons: previous and next
  sibling account. Sam used them to walk through accounts in order.
- The account-list level shows the category's fringe account as its own bold row near the
  bottom (category code with a fixed suffix), followed by a total row.
- On entering a level, the selection lands on the first row; at the detail level it lands on
  the first line's description cell.

## Selection

- A single click selects one cell: highlighted fill, a strong border, and a small square
  handle at its bottom-right corner (a fill handle).
- Clicking in the total column selects the whole row.
- Multiple rows can be selected; everything that summarizes "the selection" updates live.

## Status bar

- Always visible, left to right: net budget, ATL plus BTL, total ATL, total BTL, credits (in
  parentheses as a negative), and grand total.
- When drilled down, the grand total gives way to context: the current account's total and
  its parent category's total, each labeled with code and name.
- At the far right: a "this session" change amount and a "last change" amount, both zero
  until something is edited.

## Tools pane (right side)

- Two stacked, tabbed panels. Top: fringes, credits, units, globals, sets, find. Bottom:
  groups, charges, currencies, locations, notes, shortcuts. The whole pane can be toggled.
- Each list has a checkbox column that reflects the CURRENT SELECTION, not the setup:
  checked means every selected line has it, a dash means some do, empty means none. Selecting
  a topsheet row reflects all the lines beneath it.
- The amount column shows that fringe's or group's dollars within the selection; a footer
  shows the selection's combined fringe percentage and fringe total (and a groups total).
- Each list has a quick filter box, links to its setup screen and to clear, and a toggle to
  show only items applied to the selection.
- Range buttons (add range / remove range) enable based on the selection; remove is active
  when the selection is inside a fringe range.
- Groups list shows a color swatch, a line count per group, and either a dollar total or an
  "excluded" marker for the suppression group.

## Detail grid

- Columns seen: effective fringe percent per line, fringes list, groups list, description,
  amount, unit, X, currency, rate, subtotal. A corner menu toggles each optional column on or
  off (also aggregate sum, location, set, unit2, unit3, 4X, unit4).
- The subtotal column can be pinned so it stays visible while the grid scrolls sideways.
  With the tools pane open the grid was too narrow, and Sam scrolled horizontally back and
  forth to see amounts and totals: a pain point worth beating.
- Row styling carries meaning: line text takes its group's color; suppressed lines are
  greyed; a bracket in the left gutter marks a fringe range; bold lines act as person
  headers with indented lines under them; a line can be a drawn rule (separator) with no
  cost; subtotal and total rows are tinted.
- Rates show two decimals when they have cents; money totals show whole dollars with a
  currency sign and thousands separators.

## Right-click menus

- Line and account levels: cut cell, copy cell, paste, insert row, delete row, text
  formatting submenu, insert subtotal, add note. Shortcuts shown: insert row on Cmd+I,
  delete row on Cmd+D, insert subtotal on Cmd+=, add note on Cmd+Shift+Space.
- Topsheet level swaps insert subtotal for: insert production total, apply or create a
  credit, apply or create a contractual charge, and hide net total.

## Toolbar

- Zoom out / zoom level / zoom in, a "new version" action, a version history dropdown,
  and undo / redo. Top navigation: home, reports, setup, compare, find, an autosave toggle
  and a tools-pane toggle.

## What this means for Film Wizard (backlog candidates)

1. **Apply-to-selection panel**: tri-state checkboxes for fringes, groups, sets, locations
   and currency that reflect and edit the selected lines, with per-item selection totals.
   Our setup panel edits tables; this is the missing "apply" half, and it's how Sam works.
2. **Sibling stepping and breadcrumb jumps**: previous/next account buttons beside the
   breadcrumb (we have Cmd+[ and Cmd+]; add visible buttons) and a dropdown per segment.
3. **Context totals in the footer**: current account and parent totals when drilled down;
   session-change and last-change counters.
4. **Column chooser and a pinned total column**, plus a layout that avoids sideways
   scrolling at common window sizes (narrower setup panel, collapsible, or columns that
   compress).
5. **Row semantics**: group color on line text, fringe-range bracket in the gutter, bold
   and indent formatting, separator rule lines, per-line notes.
6. **Account list shows the fringe account row** (we already compute it).
7. **Effective fringe percent column** per line.
8. **Shortcut compatibility**: Sam's muscle memory is Cmd+I insert row, Cmd+D delete row,
   Cmd+= insert subtotal. Ours currently use Cmd+D for fill down. Needs a decision.
