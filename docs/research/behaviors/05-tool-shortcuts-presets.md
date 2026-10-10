# Behavior note 05: shortcuts (saved line presets)

Source: official vendor tutorial video "Using Shortcuts" (published 2020-12-30); vendor
tutorial, 2020/2021 era. The video has no narration captions; this note is from the
on-screen action only. Clean-room: behavior in our own words, no frames kept, no budget
data copied.

"Shortcuts" here are not keyboard shortcuts. They are named presets saved from existing
lines and replayed onto other lines. They live in a "shortcuts" tab of the tools pane (Sam's
note 01 lists that tab in the bottom panel).

## Creating a shortcut

- Select one or more detail lines, open the shortcuts tab and click its new-shortcut
  button. A two-step dialog follows.
- Step 1 picks the kind:
  - **Apply tools to existing lines**: captures the tags of the selected line (fringes,
    groups, set, location, currency) to stamp onto other lines later.
  - **Create new lines**: captures whole lines, including fields such as amount and X, and
    inserts copies of them each time it is used.
- Step 2 is a checklist of what to include. For the tools kind: fringes, groups, location,
  set, currency. For the new-lines kind, also description, amount, X, 4X, rate, unit and
  the extra unit columns. Fields that held nothing on the source lines appeared unticked;
  the rest were ticked by default and could be unticked. Back returns to step 1.
- The new shortcut appears in the tab's list with a default numbered name, a short
  description of its contents (the tool abbreviations it carries, or a line count such as
  "5 lines" for the new-lines kind) and its action buttons. A search box filters the list
  and a setup link manages them.

## Using a shortcut

- Tools kind: two buttons per shortcut, add and replace. Add puts the shortcut's tags on
  the selected lines on top of what they already have; replace swaps the selected lines'
  tags for the shortcut's. In the demo, using it on a line that had different fringes and
  groups left the line with exactly the shortcut's fringes and group, and the line re-styled
  in the group's color at once.
- New-lines kind: one insert button. With a line selected, insert put the saved lines in at
  the selection (the selected line and everything below moved down) with all their values,
  so the account's subtotal and total grew by the saved lines' amount immediately.
- Each use updates the tools pane checkboxes and the status bar totals right away.

## Shortcuts seen

None (no key bindings were shown for running a shortcut).

Era caution: late-2020 build. The tab still exists in 2026 (note 01), but the dialog's
steps, the include list and the add versus replace buttons may have changed. Whether a
"create new lines" shortcut keeps fringe ranges, notes or row formatting was not shown.

## What this means for Method Budget

1. **Tag presets: save a line's tags and stamp them onto a selection** [missing] P1. Our
   Apply panel tags a selection one item at a time. A saved bundle (for example "union crew
   day player": a set of fringes, a group and a location) applied in one click, with add or
   replace, removes the most repetitive clicking in budgeting. Fits naturally in the Apply
   panel; store presets in the budget file so they travel with it (format addition).
2. **Line-block presets: save lines and insert copies** [missing] P2. Saved blocks of lines
   (a standard prep, shoot, wrap breakdown) inserted at the cursor with their values and
   tags as one undo step. Overlaps with copy and paste between budgets; decide whether
   presets live in the budget, in a per-user library, or both.
3. **Choose what a preset carries** [missing] P2. A checklist when saving, defaulting to the
   fields that are set on the source lines.
4. **Add versus replace when applying tags** [partial] P2. Our Apply panel toggles single
   items and replaces single-value fields (set, location, currency). A replace-all-tags
   action is not there.
