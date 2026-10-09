# Behavior note 20: copying and pasting lines between budgets

Source: vendor tutorial video "Copy and Paste Between Budgets" (published 2020 to 2021,
vendor tutorial, 2020/2021 era, about 1 minute 48 seconds), watched headless as captions plus
extracted frames. Clean-room: behavior in our own words, no frames or narration kept, no
vendor data.

## Behaviors observed

- Two budgets were open at once as tabs in one window (one tab per budget, each with its own
  menu button). Switching tabs swaps the whole grid, breadcrumb and tools pane.
- Copying works at any level: topsheet (categories), account list (accounts) or detail
  (lines). The demo copied three detail lines from one budget's account into the same
  account number in a second budget whose account held only one blank line.
- Selecting the rows: Shift+click to extend, or Shift+Down. Then Cmd/Ctrl+C or the right-click
  menu. The context menu wording follows the selection: with whole rows selected it offers
  cut/copy rows, with a single cell selected it offers cut/copy cell and paste rows (when
  rows are on the clipboard).
- After copying, the copied rows get a hatched "marching" overlay so the user can see what
  is on the clipboard, and it stays visible while switching to the other budget.
- Pasting rows inserts them ABOVE the selected line. In the demo the target's existing blank
  line ended up as line 4, after the three pasted lines, rather than being overwritten.
- The pasted lines carry their applied tools. Before the paste lands, a modal lists every
  setup item referenced by the copied lines that ALREADY EXISTS by name in the target
  budget: one row per item with type (fringes in the demo), library, category (folder) and
  name. The rule shown: the target budget's values are kept. Choices: cancel, or paste and
  continue.
- So for a same-named fringe with a different rate or cutoff in the target, the pasted lines
  pick up the TARGET's definition; nothing in the target's setup tables is overwritten. The
  narration says this explicitly about cutoffs.
- After the paste, the tools pane in the target showed the copied lines' fringes checked (all
  four fringes the source lines used). The group on the source budget (a suppression group)
  was not on the copied lines, so group carry-over was not shown.
- Not shown, so unknown: what happens to a referenced item that does NOT exist in the target
  (the documented behavior elsewhere is that it is added; this video did not show the
  "missing items" case), globals referenced in formulas, sets, locations, currencies and
  whether the conflict modal also covers groups and globals. The modal's type column implies
  it does cover other kinds.

## Calculation findings vs our engine

- All lines in the demo were zero-rate, so no totals could be checked.
- Conflict rule (target wins, same name = same item) is a policy, not math. It means pasted
  lines can change value when they land (the target's fringe rate or cutoff applies). Our
  design should say so to the user.

## Shortcuts seen

- Cmd/Ctrl+C copy rows, Cmd/Ctrl+V paste rows, Cmd/Ctrl+X cut rows; context menu also shows
  Cmd+I insert row, Cmd+D delete rows, Cmd+B bold rows, Cmd+Shift+Space add note, Cmd+R
  make a fringe range (all consistent with notes 01 and 02).

## Era caution

2020/2021 build. Sam's 2026 recordings (notes 01 and 02) win any conflict. The two-budgets-in-
tabs layout and the marquee overlay may have changed; the target-wins conflict rule is the
part most likely still current.

## What this means for Film Wizard

1. [missing] P1: **Copy rows between budgets with their tools.** Our copy is tab-separated
   cell text (APP.md), which loses fringes, groups, set, location, currency, notes, styles
   and fringe ranges. Add a structured clipboard payload (our own JSON, also put on the
   system clipboard as TSV for spreadsheets) so rows pasted into another open `.fwb` keep
   everything. Same at the account and category level (copy whole accounts with their lines).
2. [missing] P1: **Paste conflict review.** On paste, list referenced setup items: those that
   exist in the target with different values (target wins by default, offer "use source
   values" per item as an option we can add later) and those missing in the target (added,
   with dependencies such as a fringe's unit or a global's referenced globals). Show the
   total change the paste will cause. One undo step.
3. [missing] P2: **Several budgets open at once.** Tabs or multiple windows, so copying
   between budgets is a normal clipboard action. Multiple windows fit Tauri well and keep each
   budget's undo history separate.
4. [partial] P2: **Paste rows inserts above the selection** rather than overwriting cells.
   We have cell paste; add an explicit "paste as new rows" (and make it the default when the
   clipboard holds whole rows).
5. [missing] P3: **Visible clipboard marker** on copied rows until the next copy or Escape.
6. [have] Cut, copy, paste keys match habits (decision 15).
