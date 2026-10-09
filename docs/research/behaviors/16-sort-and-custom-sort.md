# Behavior note 16: sorting setup tables and apply lists, custom order

Source: vendor tutorial video "Sort & Custom Sort" (published around Mar 2020 to Apr 2021;
vendor tutorial, 2020/2021 era), about 3.5 minutes, macOS. Clean-room: behavior in our own
words; no copied text or images. The video stays outside the repo.

## Scope: what sorting applies to

- Sorting here is about the setup tables (fringes shown) and the apply lists in the tools
  pane. It is not about sorting budget lines or accounts; nothing in the video reorders
  the chart of accounts or detail lines.
- It changes display order only. No amounts, names or assignments change. The custom order
  is remembered (the narration says the product stores it).

## Setup screen: folders on the left rail

- The fringe setup screen has a left rail of folders (categories) that organize the
  fringe list: a default "uncategorized" folder plus user folders. There is a "move to
  category" action and add/delete for folders.
- A sort dropdown above the rail offers custom, A to Z and Z to A. The default folder stays
  pinned at the top whatever the sort. Under custom, folders show drag handles and can be
  dragged into any order. The same applies to the cloud library side (out of scope for us).

## Setup screen: the table

- Each row has a drag handle; under custom order rows can be dragged anywhere.
- Sortable column headers show a faint sort icon; some columns (description) have none.
  Name, rate, cutoff, in-use and total looked sortable.
- Clicking a sortable header cycles: ascending, descending, then back to custom order.
  The active sort header is shown bold. Switching to another header starts that header's
  cycle; to get back to custom you click the currently active header until it clears.
- Custom order survives sorting: returning to custom restores the hand-made arrangement.

## Apply lists in the tools pane

- Same handles and the same three-state header cycle on the name column.
- The "show applied only" filter can be on while sorting, but dragging (editing the custom
  order) is disabled while filtered, because the full list must be visible to place items.
- A search box filters the list as you type (prefix of the name; typing a few letters
  narrowed fringes to matches).
- Whether the apply list's custom order is the same as the setup table's custom order was
  not clear: the two showed different orders in the video. Unknown.

## Shortcuts seen

- None.

## Era caution

- In Sam's 2026 recording (note 01) the apply lists have a filter box, setup/clear links and
  an applied-only toggle, consistent with this video. The Windows-era video in note 18
  shows a "sort by applied" checkbox instead of a filter, so the control has changed over
  time. Folder organization of setup tables may be tied to the vendor's cloud library.

## What this means for Film Wizard

1. [missing] P2 **Custom order for setup tables** (fringes, groups, globals, units, sets,
   locations, currencies): drag to reorder, stored in the budget file so reports and the
   apply panel follow it. Today every list is alphabetical by byte order.
2. [missing] P2 **Click-to-sort headers** in setup tables with a three-step cycle
   (ascending, descending, back to custom) and a visible active-sort marker. View-only.
3. [have] P3 **Search box and applied-only toggle** in the apply panel lists.
4. [missing] P3 **Folders for setup items** (group fringes into user folders). Useful for
   large fringe libraries; low priority until we have a library.
5. [missing] P3 **Block reordering while filtered** if we add drag order to the apply
   panel, so a partial list can't scramble the hidden items.
