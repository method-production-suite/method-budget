# Behavior note 06: the applied tools pane, layout and docking

Source: official vendor tutorial video "Applied Tools" (published 2020-03-04); vendor
tutorial, 2020/2021 era. Clean-room: behavior in our own words, no narration transcribed,
no frames kept, no budget data copied. Complements note 01, which describes the same pane
from the 2026 recording; note 01 wins any conflict.

## What the video shows

- The older desktop product opened fringes, globals and groups as separate floating windows
  over the budget. The newer one puts them all in one pane beside the grid so that two
  tools (for example fringes and groups) are visible together and any other tool is one
  tab away, without leaving the current screen.
- The pane has two stacked panels, each with its own tab strip, so two tools are always
  visible at once. Tabs seen across the two panels: fringes, units, globals, sets, find;
  groups, currency, locations, notes (plus shortcuts in later builds). A row of toolbar
  buttons at the top of the window (locations, fringes, globals, groups, currency, units,
  sets, credits, charges, shortcuts) jumps to the matching tool.
- Selecting a topsheet category makes the fringe and group lists show what is applied
  anywhere beneath it: ticks on the fringes used, each fringe's dollars within the
  selection, and a combined fringe percentage at the foot. Drilling down to an account's
  lines shows the same for the selected line.
- Groups list: color, a line count per group, and either a dollar total or an "excluded"
  marker for groups kept out of the total.
- Units tab: each unit shows how many lines use it, and a switch separates units in use
  from unused ones.
- Globals tab: each global shows how many lines use it, its formula and its unit; the same
  in-use switch. When the pane is wide (docked at the bottom) a description column appears
  too.
- A dock button at the pane's top right moves the whole pane from the right side to the
  bottom of the window and back. Docked at the bottom, the two panels become one wide panel
  with a single tab strip, and the grid gets the full window width.
- Each list has a "sort by applied" switch (the 2026 build calls it a filter; note 01) and
  a link to its setup screen.

## Shortcuts seen

None.

Era caution: early-2020 Windows build. The 2026 recording shows the same two-panel pane
on the right with a quick filter, setup and clear links, and an applied-only filter rather
than a sort. Dock-to-bottom was not exercised in the 2026 recordings; assume it still
exists but verify.

## What this means for Method Budget

1. **Selection-driven apply panel** [have]. Our Apply panel reflects the selection with
   tri-state boxes and dollars, including topsheet and account-list selections.
2. **Two tools visible at once** [partial] P2. Our side panel shows Apply (all tag lists
   together) or one setup tab. Seeing, say, globals while applying fringes needs a second
   panel or a split inside the panel.
3. **In-use counts for units and globals** [missing] P2. Show how many lines use each unit
   and global, with a used/unused filter. Also helps safe deletion (we can refuse or warn
   when deleting something in use).
4. **Dock the tools pane at the bottom** [missing] P2. A toggle between right and bottom
   placement; at the bottom the grid gets full width, which is our known sideways-scroll
   pain point (note 01).
5. **Toolbar buttons that open a specific tool** [partial] P3. We have Cmd+1 to Cmd+9 for
   setup tabs; a visible button row is a design call for the orchestrator, not a gap in
   function.
6. **Line counts per group** [partial] P3. Our Groups setup shows tagged-line totals; confirm
   the Apply panel shows a per-group line count across the whole budget, not only the
   selection.
