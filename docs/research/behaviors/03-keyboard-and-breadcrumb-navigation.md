# Behavior note 03: keyboard and breadcrumb navigation

Sources: two official vendor tutorial videos, "Keyboard Navigation" (published 2021-04-13)
and "Breadcrumb Navigation" (published 2020-03-04); vendor tutorial, 2020/2021 era.
Clean-room: behavior described in our own words from the narration and on-screen action. No
narration transcribed, no frames kept, no budget data copied. Sam's 2026 recordings (notes
01 and 02) win any conflict.

## Moving between levels with the keyboard

- Plain Up and Down move the row cursor at every level, including the topsheet.
- Cmd+Down (Ctrl+Down on Windows) opens the row under the cursor: a category opens its
  account list, an account opens its detail lines. Cmd+Up (Ctrl+Up) goes back up one level.
  Pressing Cmd+Up twice from the detail level lands on the topsheet.
- Cmd+Right and Cmd+Left (Ctrl on Windows) step sideways to the next or previous sibling
  without going up first. In an account list they step to the next or previous category's
  account list; in the detail view they step to the next or previous account. The narration
  presents this as the fast way to browse a budget: walk across siblings, arrow to a row,
  Cmd+Down into it.
- An on-screen keycap overlay in the video confirms the Up/Down chord pairs; the narration
  gives the Left/Right pair.

## Where the cursor lands

- Entering a category (by keyboard or by stepping sideways) puts the cursor on the first
  account row. Entering an account puts it on the first detail line's description cell.
- Going back up restores the cursor to the row you came from: returning to the topsheet
  from a category's accounts highlights that category.
- An empty category or account is never a blank screen: it shows one empty row with the
  cursor on it, ready for typing. An empty account's placeholder line already carries the
  usual defaults (amount 1, X 1, rate 0).

## Breadcrumb

- The breadcrumb reads topsheet, then category, then account. Each segment after the
  topsheet carries a small pair of up and down step buttons in front of its label and a
  dropdown arrow after it. Only the deepest segment shows the step buttons; the parent
  segment keeps its dropdown.
- The step buttons cycle through the siblings of that segment (the mouse equivalent of
  Cmd+Left/Right).
- The dropdown lists every sibling with its code and name, scrolled so the current one is
  visible and shown in bold. Picking one jumps straight there.
- Picking a different category from the category segment's dropdown while viewing detail
  lines goes to that category's ACCOUNT LIST, not to one of its accounts. Picking a
  different account from the account segment's dropdown opens that account's lines.
- Clicking the topsheet segment goes all the way up.
- Drilling down with the mouse: double-click the row number (the leftmost line-number
  cell), not just any cell.

## Shortcuts seen

| Action | Mac | Windows |
|---|---|---|
| Move row cursor | Up, Down | Up, Down |
| Open the row (category or account) | Cmd+Down | Ctrl+Down |
| Up one level | Cmd+Up | Ctrl+Up |
| Next sibling (category in account list, account in detail) | Cmd+Right | Ctrl+Right |
| Previous sibling | Cmd+Left | Ctrl+Left |

Era caution: Sam's 2026 recording (note 01) shows the step buttons as previous/next buttons
beside the current segment, which matches; whether stepping sideways still stops at the end
of a category or wraps across categories was not shown in either video. The 2021 video was
recorded on macOS, the 2020 one on Windows; keys matched apart from Cmd versus Ctrl.

## What this means for Film Wizard

1. **Cmd/Ctrl+Down and Cmd/Ctrl+Up to drill and climb** [have]. Ours already do this.
2. **Cmd/Ctrl+Left and Cmd/Ctrl+Right as sibling steps** [partial] P1. We step accounts with
   Cmd+[ and Cmd+] on the detail level only. Add Left/Right as the primary pair (keep
   [ and ] as aliases) and make it work on the account list too, stepping categories. Check
   it doesn't steal Cmd+Left/Right from a text field while editing.
3. **Sibling stepping on the account-list level** [missing] P1. Step to the previous or
   next category's accounts without returning to the topsheet; the breadcrumb's category
   segment should get the step buttons when it is the deepest one.
4. **Cursor memory when climbing** [have]. We restore the row you came from.
5. **Empty account or category shows one ready row** [partial] P2. Decide whether an empty
   account should show a placeholder line with defaults (amount 1, X 1) so the user can type
   straight away instead of inserting first.
6. **Breadcrumb dropdown semantics** [have]. Our jump lists exist; verify that choosing a
   category from the category segment while on detail lands on its account list (the
   reference behavior), and that the current item is visibly marked and scrolled into view.
7. **Double-click on the row number drills down** [partial] P3. Ours drills on a
   double-click anywhere in a category or account row. The reference ties drilling to the
   row number, which leaves double-click free to edit a code or name in place. Worth
   deciding which we want.
