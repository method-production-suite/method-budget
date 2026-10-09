# Behavior note 02: entering a line, autocomplete, applying tags to a selection

Source: a ~2 minute screen recording by Sam of normal use of a licensed copy of the reference
product (2026-10-09). Clean-room: behavior in our own words, no budget data, no copied UI
text beyond a few-word term. The recording stays outside the repo.

## Moving around

- Keyboard stepping through accounts is fast: the view jumps account to account with the
  selection landing on the first line's description cell each time.
- Up/Down in the description column moves the cell selection line by line, including onto
  blank lines and suppressed (greyed) lines.

## Selecting everything and applying a group

- Selecting all lines in an account highlights every row. While the whole account is
  selected, number cells that hold a global show the GLOBAL'S NAME instead of its value
  (the formula is visible), then switch back to values when the selection collapses.
- With the selection active, clicking a group's checkbox in the tools pane removes it from
  (or adds it to) every selected line at once. Lines re-style immediately (greyed lines
  un-grey when the suppression group is removed; text takes the group color when a colored
  group is added), the group's line count updates, and all totals, including credits that
  depend on groups, recalculate on the spot.
- The session-change and last-change amounts in the status bar update with every edit.
- Rounding drift in the reference product: turning the same group off and back on did not
  return the budget to its exact starting total; it ended a few dollars away. Our engine is
  deterministic and exact; worth a test that toggling twice is a no-op.

## Inserting and typing a new line

- Inserting a line under the current one creates a placeholder description (a default
  label the user overtypes) with amount 1.
- The new line arrives with the same fringes as the line(s) around it already applied, but
  not their groups (a "copy from above" preference). The fringe checkboxes in the tools pane
  show this right away.
- Tab order across a line: description, amount, unit, X, currency, rate.
- Unit cell autocomplete: typing a letter opens a dropdown of matching units (prefix match),
  the first match highlighted, plus an "add this as a new unit" entry at the top. Enter or
  Tab accepts the highlighted match. Custom units (e.g. man-days, per-12-hour) live in the
  same list.
- Rate cell autocomplete: typing letters opens a dropdown of matching GLOBAL names (prefix
  match, alphabetical, scrollable). Accepting one puts the global into the formula; the cell
  then shows the evaluated value.
- After each commit the line total, fringe dollars, the effective fringe percent column and
  all rollups update immediately.

## Right-click on a detail cell (fuller list than note 01)

- Cut / copy / paste cell, insert row (Cmd+I), delete row (Cmd+D), text formatting submenu,
  insert subtotal adjustment (Cmd+minus), insert subtotal (Cmd+=), add note
  (Cmd+Shift+Space), make a fringe range (Cmd+R), remove a fringe range (Cmd+Shift+R). Range
  items are disabled unless the selection allows them.

## What this means for Film Wizard (backlog candidates)

1. **Autocomplete in unit and rate/amount cells**: prefix-matched dropdown of units (with
   "add new unit") and of global names in any formula cell; Enter/Tab accepts.
2. **New-line defaults**: a preference to copy fringes (and optionally groups, set,
   location, currency) from the line above when inserting; a placeholder description.
3. **Formula visibility while selecting**: show formulas (global names) instead of values
   when a cell or the whole account is selected, or provide a toggle.
4. **Shortcut parity additions**: Cmd+minus insert adjustment, Cmd+R make range,
   Cmd+Shift+R remove range, Cmd+Shift+Space add note (Cmd+I, Cmd+D, Cmd+= are done).
5. **Exactness test**: applying and then removing the same group (or any tag) returns every
   total to the cent.
