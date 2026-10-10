# Behavior note 23: creating and applying contractual charges

Source: vendor tutorial video "Creating and Applying Contractual Charges" (published 2020 to
2021, vendor tutorial, 2020/2021 era, about 2 minutes 13 seconds), captions plus extracted
frames. Clean-room: behavior and arithmetic in our own words, no frames, no vendor data.

## Behaviors observed

- Entry: right-click on the topsheet, apply submenu, create a contractual charge. This opens
  the budget setup window on its charges tab (the same window has tabs for locations,
  fringes, globals, groups, currency, units, sets, credits, charges).
- Left side: three collapsible lists: charges applied in this budget, the user's own charges,
  and two library lists (a studio defaults list and a vendor defaults list). Add and delete
  buttons sit above the user's list. An empty state invites making a new charge or picking
  one from a library list.
- A new charge starts with a placeholder name, selected for overtyping. Fields: name, charge
  type (percent or flat fee), percent (or amount), optional account number, and two
  "display in topsheet description" checkboxes: show the percentage, show the excluded
  amount.
- A live "total charge" figure updates as the percent and exclusions change, before the
  charge is applied.
- Exclusions grid below the fields, columns type, name, description, total:
  - "Exclude amount" adds a manual row (type user) with a default description and an amount.
  - "Exclude groups" is a dropdown of the budget's groups; picking one adds a row (type
    group, group name, group description, the group's amount, read-only).
  - Delete row removes the selected exclusion.
- An apply button puts it on the budget; closing the window shows it on the topsheet. The
  narration describes the usual position as after the last production total and before the
  grand total. On screen, the charge row sat right after the last production-total row and
  BEFORE the ATL, BTL and ATL+BTL summary rows, then the grand total, then credits and net.
- With "excluded amount" display on, the topsheet description reads the charge name followed
  by the excluded total in parentheses.
- The groups setup tab (seen in note 24's video) shows each group's full total and an
  "active total" (after other groups' suppression). The excluded group row used the ACTIVE
  total.

## Calculation findings vs our engine

Step-by-step check against the numbers on screen (the tutorial's sample figures are
deliberately not reproduced here). Each step matched our engine to the cent:

1. A percent charge with no exclusions: percent of ATL+BTL, rounded once to the cent.
2. Excluding a group subtracts that group's active (counted) total from the base first.
3. Excluding a second group subtracts its total too.
4. A manual excluded amount is subtracted as typed; the charge's "excluded" display shows the
   sum of every exclusion.
5. Grand total = ATL+BTL plus the charge.

- Base is ATL + BTL, charge rounded once on its base, charge added to the grand total but not
  to ATL/BTL: all match our engine.
- Group exclusion uses the group's active total (suppressed lines count 0). Ours: the
  exclusion loop sums `counted()` of tagged rows, and left-out rows have 0 money
  (`RowTotals.included`), so it is the active total too. Match. Group totals here include
  fringes (the groups tab had "include fringe amounts in group totals" on); ours includes
  fringes unless fringes post by budget. Match for this setting.
- Whether a line in two excluded groups is counted once could not be verified (no overlap
  information on screen). Ours counts each line once.
- Not a math mismatch but a display one: the setup window's live "total charge" still showed
  the previous figure after a manual excluded amount was typed (the cell was still in edit),
  while the topsheet already showed the corrected charge. The preview lags an uncommitted
  cell.
- Only one charge was shown, so stacking (charge on charge) and application order could not
  be checked in this video. Our rule (charges never stack, order is display order) stands.

## Shortcuts seen

None.

## Era caution

2020/2021 build. The library lists (studio and vendor default charges) are vendor content and
cloud features of that era. Sam's 2026 recordings win any conflict.

## What this means for Method Budget

1. [have] Percent and flat charges, manual excluded amounts, excluded groups, categories and
   accounts, applied switch, account number, ordering (`set_charge`, `move_charge`, decision
   9). Numbers match the reference to the cent in every step above.
2. [missing] P2: **Topsheet description options**: per charge, append the percent and/or the
   excluded total to its topsheet label (we show the base and excluded amount only in a
   tooltip).
3. [partial] P2: **Exclusion rows as a grid with type and live amounts**: show each excluded
   group's active total next to it, and the combined excluded amount, in the setup panel.
4. [partial] P3: **Charge row position**: the reference puts charges after the last
   production total and before the ATL/BTL summary rows; we list them after ATL and BTL.
   Pick one deliberately and document it (math is identical).
5. [missing] P2: **Live preview that includes the cell being edited** (avoid the lag seen
   here): compute the preview from the draft value on each keystroke.
6. [missing] P2: **"Create charge" from the topsheet's right-click menu**, jumping to the
   charges setup with a new charge selected and named for overtyping.
7. [missing] P3: **Charge templates from a personal library** (see note 24); we must not ship
   vendor default charges, but our own common ones (bond fee, contingency, insurance,
   overhead) authored from public practice are fine.
8. [have] Test-worthy: add this exact sequence (3%, group, group, manual amount) as an engine
   test with our own numbers.
