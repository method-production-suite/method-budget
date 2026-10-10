# Behavior note 04: split screen and budget tabs

Source: official vendor tutorial video "Split Screen Mode" (published 2021-04-13); vendor
tutorial, 2020/2021 era. Clean-room: behavior in our own words, no narration transcribed,
no frames kept, no budget data copied.

## Budgets as tabs

- Several budgets can be open in one window, each as a tab under the toolbar. Clicking a
  tab switches the whole window (grid, breadcrumb, tools pane, status bar) to that budget.
- Each tab has a small menu button. Its items, in order: rename, close the budget, the
  keyboard shortcut sheet, open in split screen (or exit split screen when split), create a
  sub-budget, and share a version (greyed out in the demo).

## Split screen

- Needs at least two open budgets. Choosing the split item on a tab's menu moves that
  budget into a new right-hand pane; the window now shows two budgets side by side, each
  with its own tab strip, breadcrumb and grid.
- One pane is the editing target at a time. It is marked by a dot before the tab name and a
  bold name. Clicking into the other pane's tab makes it the target. The single toolbar at
  the top (tools, reports, find) acts on the target budget.
- With three or more budgets open, a tab whose pane has other tabs offers a "move to the
  other pane" item. A pane's last remaining tab doesn't offer it (that would leave a pane
  empty).
- Every tab's menu offers "exit split screen", which merges all tabs back into one pane.
- The tools pane, when shown in split mode, docks along the bottom of each pane instead of
  the right side, as one tab strip (fringes, units, globals, sets, groups, currency,
  locations, find, notes, shortcuts). Each pane's tools show that pane's budget. Clicking a
  tab in a pane switches what that pane's tools show.
- Totals, the status bar and grid behaviors stay per budget; the demo used split mode to
  compare two budgets' topsheets by eye (different account coding in each).

## Shortcuts seen

None new. The tab menu has an entry for the keyboard shortcut sheet.

Era caution: the 2021 build only. Not seen in Sam's 2026 recordings, so the split layout,
the tab menu items and the sub-budget and share-version entries may have changed.

## What this means for Method Budget

1. **More than one budget open at once** [missing] P2. We are one budget per window and
   have no second window. The simplest local-desktop answer is multiple windows (one
   session each) rather than in-window tabs; that also gives side-by-side for free via the
   operating system. Needs a decision before any tab UI.
2. **Side-by-side compare of two budgets** [partial] P2. The engine already compares 2 to
   16 budgets and versions (decision 17). A read-only second pane, or a second window
   showing a version or another file, would cover the "look at both" use case.
3. **Tools pane docked at the bottom** [missing] P3. See note 06; split mode is the main
   reason the reference docks it at the bottom.
4. **Per-budget tab menu: rename, close, shortcut sheet** [partial] P3. We have the
   shortcut sheet (Cmd+/) and close; rename is the budget title in Info.
5. **Sub-budgets** [missing] P3. Out of scope for now; note only.
