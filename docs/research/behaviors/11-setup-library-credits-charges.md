# Behavior note 11: custom setup items, library, credits and charges screens

Source: "Custom Categories", vendor tutorial video (published 2020/2021; vendor tutorial,
2020/2021 era), under 1 minute, recorded on Windows. The narration is a short pitch; most
of what is below comes from the frames. Clean-room: behavior in our own words, read
headless. No budget data, copied help text, rate tables or images. The video stays outside
the repo.

## Behaviors observed

- **Reuse across budgets**: the pitch is that custom setup items (fringes, globals,
  credits, charges) live with the user's account, so any budget can use them, instead of
  being copied from an old budget.
- **Folders everywhere**: the fringes tab showed user folders (for example one per city
  or union) beside an uncategorized folder and a vendor folder; globals had many topical
  folders. Selecting a folder filters the list to its items.
- **Fringe list kinds**: one list mixes percent fringes and flat fringes. A flat fringe's
  unit cell shows a time unit (week, day) or "Allow" (a flat allowance); a percent fringe
  shows "%". The footer splits the fringe dollars into a percentage total and a flat
  total, and their sum.
- **Credits screen**: left rail with "applied in budget" and the user's saved credits.
  A credit has a name, an applied credit cap, a discount percent, an account number, and
  read-only net credit and discount amount. Its rows each name a source (a group in the
  example) with gross total, excluded-fringe total, net total, cap type, cap, percent,
  credit amount and a cap variance. Toolbar: add row, delete rows, a menu of groups to
  include, and a list of fringes to exclude from the credit base.
- **Charges screen**: left rail with "applied in budget" and saved charges. A charge has a
  name, a charge type (percent shown), the percent, an account number, a read-only total
  charge, and two "display in topsheet description" checkboxes (show the percent, show the
  excluded amount). Its exclusion rows are typed: a manual user amount, or a group (with
  that group's total). Toolbar: exclude amount, delete rows, exclude groups menu.

## Calculation findings

- **Percent charge, verified to the cent** from numbers on screen: total charge equalled
  the percent times (ATL plus BTL, less the manual excluded amount, less each excluded
  group's active total including its fringes), rounded once. This matches our engine:
  charges apply to ATL + BTL, excluded groups subtract their counted lines with their
  fringes when fringes post inside ATL/BTL, one rounding (decision 9 and `topsheet.rs`).
- Excluded groups used the group's ACTIVE total (with a suppressed group already out),
  not its full total. Ours also subtracts only counted lines. Match.
- Not shown: what happens when a line is in two excluded groups (we count it once).

## Era caution

The "applied in budget" versus library split and account-based storage are 2020/2021
cloud-era features. Sam's 2026 notes have not covered the credits or charges screens yet.

## What this means for Method Budget

1. [have] P1 **Percent charge with manual and group exclusions**: matches on screen.
2. [partial] P2 **Charge display options**: show the percent and/or the excluded amount
   in the topsheet description. Check what our topsheet row shows.
3. [partial] P2 **Credit editor parity**: per-row gross, excluded fringes, net, cap type,
   cap, percent, credit amount and cap variance shown live. We have credit rows with caps
   and a discount; check that the editor shows the per-row breakdown and the variance to
   cap.
4. [have] P3 **Credit "exclude fringes"**: our group credit rows already carry
   `excluded_fringes` (per row; the reference sets the list once per credit, a UI
   convenience we could add).
5. [missing] P3 **Library of saved setup items with folders** (also note 09, item 5).
6. [have] P1 **Percentage and flat fringes in one table**; [missing] P3 a footer with
   percent total, flat total and their sum.
