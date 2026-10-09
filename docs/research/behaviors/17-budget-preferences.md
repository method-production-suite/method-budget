# Behavior note 17: budget preferences

Source: vendor tutorial video "Budget Preferences" (published around Mar 2020 to Apr 2021;
vendor tutorial, 2020/2021 era), about 1 minute, macOS. Clean-room: behavior in our own
words; no copied text or images. The video stays outside the repo. The narration is brief,
so most of this comes from reading the dialog in the frames; effects marked "inferred" were
not demonstrated.

## Where it lives

- Setup menu, first item, Cmd+9 (Ctrl+9 on Windows). It opens a separate preferences
  window with a single tab for this budget, so these are per-budget settings saved with the
  file, not app-wide preferences. A close button; no OK/cancel, so changes apply directly
  (inferred).

## Fields, grouped as shown

**Fringe**
- Post fringes by: a dropdown, set to budget in the video. A fringe account number field
  (prefilled 99) sits beside it and is greyed out until a posting mode needs it (by
  category, where the category fringe account is category code plus that number).

**Detail level**
- When adding a detail line, copy from the line above: separate checkboxes for fringe,
  group, location, set and currency. Fringe was on by default, the rest off. This matches
  Sam's 2026 recording (note 02): new lines inherit fringes, not groups.

**Rate column**
- A display cutoff value (blank by default), then the number of decimal places for rates
  below the cutoff and for rates above it, each defaulting to "as entered in budget".
  This is display formatting only (inferred): small rates can show more decimals than big
  ones.
- An "override with first-used global" checkbox, on by default, with a help marker.
  Inferred meaning: when a rate cell uses a global, that global's own decimals setting (the
  globals table has a decimals column) wins over the rate-column rule.

**Totals**
- Display summary totals: on by default. Its exact effect was not shown; likely whether the
  topsheet shows the summary rows (inferred).
- Lock original totals for comparison: off by default. Inferred: freezes the current totals
  as the "original" used by original and variance columns.

**Account coding**
- Number of digits for category, account, sub-account and set (dropdowns, 2, 2, none, 3 in
  the video) and a category/account separator dropdown.

## Shortcuts seen

- Setup menu keys in this era: preferences Cmd+9, locations Cmd+5, fringes Cmd+3, globals
  Cmd+G, groups Cmd+4, currency Cmd+6, units Cmd+7, sets Cmd+8, credits Cmd+\, charges Cmd+/.

## Era caution

- Five to six years old. Note 02 (2026) confirms the copy-from-above behavior still exists.
  The rest of the dialog may have grown or moved; confirm before matching labels or
  defaults.

## What this means for Film Wizard

1. [missing] P1 **Copy from the line above on insert**: per-budget switches for fringes,
   groups, location, set and currency; fringes on by default. Our insert makes an empty
   line today (`edit::add_detail`). Already flagged in note 02; this confirms the defaults.
2. [have] P2 **Fringe posting mode and fringe account suffix** (Settings tab; decision 9).
3. [have] P2 **Account coding widths and separator**, including a set segment (decision 11).
4. [partial] P2 **Lock original totals**: our baseline (decision 12) is a frozen copy set
   from the Baseline tab; matches the idea. Check the wording and that it is one obvious
   action.
5. [missing] P2 **Rate display decimals**: a cutoff with separate decimal counts below and
   above it, default "as entered". Display only, never touches the math.
6. [missing] P3 **Per-global decimals that win over the rate rule** when a rate is a global.
7. [missing] P3 **Show or hide summary totals on the topsheet** (needs confirming what it
   actually does).
8. [partial] P3 **Shortcut to the budget settings**: our setup tabs are on Cmd+1 to Cmd+9 in
   our own order and Cmd+/ is our shortcut sheet; no change suggested, just note the
   difference for switchers.
