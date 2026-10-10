# Behavior note 14: suppressing groups

Source: "Suppressing Groups", vendor tutorial video (published 2021; vendor tutorial,
2020/2021 era), under 2 minutes, recorded on Windows. Clean-room: behavior in our own
words, read headless from captions and extracted frames. No group names, budget data,
copied help text or images. The video stays outside the repo.

## Behaviors observed

- Ways in: Setup menu, then groups (Ctrl+4 in this build), or the groups toolbar button.
- **Three budget-wide group policies** sit above the group list, each an include/exclude
  dropdown with a sentence after it:
  1. whether lines with no group count by default (set to include);
  2. what to do when a line's groups disagree, one in the total and one out (set to
     exclude);
  3. whether fringe dollars are part of group totals (set to include).
- **Columns**: selection checkbox, visibility (eye), name, description, ID, "InBT" (in
  budget total) checkbox, color swatch with a picker, in-use (line count), total, active
  total, cloud status. Headers sort the list.
- Unchecking InBT suppresses the group: its active total drops to zero; the narration
  says its total stays the same (see the findings: on screen it did not quite).
- The narration says suppressed lines change font and background color at the detail
  level and their subtotal cell shows an "Excl" marker before the amount. The frames
  agree: greyed rows, amounts kept visible with the marker.
- In the tools pane's group list, a suppressed group shows "Excl" instead of a dollar
  amount.
- The main window's status bar did not change while the setup window was open; it
  appears to refresh after closing.

## Calculation findings

- **Conflict rule = exclude**: before any change, the suppression group's active total was
  already below its total, because some of its lines were also in other suppressed
  groups. A line in any out-of-total group is out. This matches our engine (decision 4:
  a line is suppressed if any of its groups is out).
- **Suppression removes fringes too**: when the group was suppressed, its total (fringes
  included, per policy 3) dropped by a large amount and the totals of overlapping groups
  dropped as well. The drop is consistent with the suppressed lines' fringes no longer
  being computed, while their base amounts stay in the group's "total".
  Our engine also drops fringes for left-out lines (they count as 0 and don't join a
  range's earner). Match on the money.
- **MISMATCH (display only)**: the reference's group "total" is the tagged lines' base
  amounts plus the fringes actually computed, so it shrinks when lines are suppressed. Our
  "full" group total is computed as if nothing were suppressed, fringes included
  (`GroupTotals::full`, decision 4). Same active totals, different full totals. Decide
  whether "full" should mean "as if counted" (ours, more useful for what-ifs) or follow
  the reference; either way label it clearly.
- **Unverified, possibly important**: in the closing detail-level frames, five lines were
  shown suppressed with "Excl" amounts, yet the account's subtotal row, its total row and
  the status bar still matched the values from before those lines were suppressed. This
  is either an edited demo clip or a sign that detail-level subtotal and total rows keep
  showing suppressed amounts. Our engine counts suppressed lines as 0 in every subtotal
  and total. Sam's 2026 note 02 says totals recalculated on the spot when a suppression
  group was removed, which points to an artifact, but whether the detail total row itself
  excludes suppressed lines should be checked in a 2026 recording.
- **Policies we hardcode**: unassigned lines always count; conflict always excludes;
  group totals report details and fringes separately (both shown). The reference makes
  the first two switchable. "Include" on conflict would mean a line counts if any of its
  groups is in the total. "Exclude" for unassigned lines is like our `only_groups`
  recalc option, but saved in the budget.

## Shortcuts seen (Windows, 2021 build)

- Setup menu: budget preferences Ctrl+9, locations Ctrl+5, fringes Ctrl+3, globals Ctrl+G,
  groups Ctrl+4, currency Ctrl+6, units Ctrl+7, sets Ctrl+8, credits Ctrl+backslash,
  charges Ctrl+slash (matches the macOS list in note 09).

## Era caution

Sam's 2026 note 02 covers applying and removing the suppression group from the tools pane
with immediate recalculation, greyed lines, and a small rounding drift on toggling. The
policy dropdowns and the "total" versus "active total" columns were not re-checked in 2026.

## What this means for Method Budget

1. [have] P1 **In-budget-total switch, greyed lines, full and counted totals, conflict =
   exclude.**
2. [partial] P1 **Verify detail subtotal/total with suppressed lines** against a 2026
   recording (see the unverified finding). Ours excludes them everywhere.
3. [missing] P2 **Budget-wide group policies**: conflict rule (exclude or include) and
   whether ungrouped lines count, saved in the budget, property-tested like decision 4.
4. [partial] P3 **Group "full" total meaning**: keep ours but label it ("if counted"), or
   add the reference's figure. Display only.
5. [partial] P2 **"Excl" marker with the amount kept visible**: we grey and strike
   through; the reference keeps the amount readable with a marker. Check readability.
6. [missing] P3 **Group short code (ID) and sortable group table columns**.
7. [have] P1 **Group line count** in the apply panel; [partial] P3 in-use count in the
   groups setup tab.
