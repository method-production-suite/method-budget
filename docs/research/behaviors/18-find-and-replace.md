# Behavior note 18: find and replace

Source: vendor tutorial video "Find and Replace" (published around Mar 2020 to Apr 2021;
vendor tutorial, 2020/2021 era), about 1.5 minutes, Windows. Clean-room: behavior in our own
words; no copied text or images. The video stays outside the repo.

## Where it lives

- A Find button in the top bar opens a Find tab in the right-hand tools pane (the same pane
  as fringes, units, globals, sets). The budget grid stays usable beside it.

## Searching

- First a dropdown picks what kind of thing to look for. Options seen: description (detail
  line), fringes, globals, groups, location (column), set (column), and more below a scroll
  (not shown). So find is typed: it searches one field or one kind of tag at a time, not
  free text across everything.
- Then a search field. As you type, suggestions appear under it. For globals they show the
  global's full path (library folder, sub-folder, name), and you pick one. There were no
  visible options for match case, whole word or regular expressions.
- Results list in the pane: one row per account that has a match (account number and
  name), each with a plus to expand. Expanded, it lists the matching lines with the column
  where the match is (for a global used in X, the column shows X).
- Each matching line has a go-to arrow that jumps the grid to that account and selects the
  matching cell (the breadcrumb updates and the cell is highlighted).

## Replacing

- Every result row and every account row has a checkbox; a header checkbox selects all.
- A "replace with" field with its own suggestions (other globals, shown with their paths).
  The Replace button is enabled once a replacement is chosen and something is checked.
- Replacing swaps the item in the checked places only. Totals recalculate at once. A small
  "N updated" message appears near undo, and undo is available.
- Replaced items drop out of the results because they no longer match.
- Instead of replacing, a delete (trash) button removes the found item from the checked
  places (for a global, the reference is taken out of those cells; totals dropped). What
  the cell holds afterwards was not visible.

## Shortcuts seen

- None in this video.

## Era caution

- Windows build, 2020. Sam's 2026 recording (note 01) still shows a Find tab in the tools
  pane and a Find item in the top navigation, so the feature lives on; the list of search
  types and the result layout may have changed.

## What this means for Method Budget

1. [missing] P1 **Find panel**: pick a field or tag type (description, fringe, global,
   group, location, set, unit, currency, any formula text), type with suggestions, see
   results grouped by account with the matching column, and jump to each one.
2. [missing] P1 **Replace in the checked results only**, one undo step, with a count of
   what changed and results that refresh after the change. Replacing a global inside a
   formula must swap the name as a token, never a substring of another name.
3. [missing] P2 **Remove from checked results**: take a fringe, group, set or location off
   those lines (ties into our apply commands); for a global in a formula, decide what the
   cell becomes (freeze to its value, like deleting a global does, decision 12).
4. [missing] P2 **Plain text search across descriptions and notes** with match-case and
   whole-word options, which the reference product did not show.
5. [missing] P3 **Find a global's uses** from the globals tab (in-use count already exists
   in their setup tables; we show a value, not a use count).
