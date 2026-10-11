# Behavior note 12: fringe setup

Source: "Fringe Setup", vendor tutorial video (published 2020/2021; vendor tutorial,
2020/2021 era), about 2.5 minutes, recorded on Windows. Clean-room: behavior in our own
words, read headless from captions and extracted frames. No fringe names or rates from the
video, no copied help text, no images. The video stays outside the repo.

## Behaviors observed

- Advice in the narration: set up fringes before entering labor lines.
- Ways in: Setup menu, then fringes, or the fringes toolbar button. Opens the setup window
  on the fringes tab with library/budget tabs and a folder rail (see notes 09 and 11).
- **Columns**: selection checkbox, visibility (eye), name, description, ID, rate, unit,
  cutoff, in-use, total, and a cloud (library) status icon.
- **Name versus ID**: the name is the short nickname that appears in the grid's fringes
  column. ID is an optional code; when ID is filled it is what the grid's fringe column
  shows, and when blank the name is shown.
- **Adding**: the add button inserts a new blank row directly under the selected row (not
  at the end), with the name cell in edit mode. Tab moves name, description, ID, rate,
  cutoff. The unit cell stays blank until a rate is entered; percent fringes show "%".
- **Cutoff**: the maximum wage base the fringe applies to (the narration calls it the most
  that can be paid for that fringe; the frames show it as a wage amount, not a fringe
  amount). Zero or blank means no cutoff.
- **In-use** is how many lines carry the fringe; **total** is the fringe's dollars across
  the budget. A new fringe shows 0 and 0.00.
- Deleting: select a row, then the delete button.
- The footer shows the overall fringe total split into percentage total and flat total.
- After closing, the new fringe appears in the tools pane's fringe list, in the setup
  table's order (inserted position), not alphabetically.

## Calculation findings

- The footer's fringe total equalled the sum of the per-fringe totals to the cent, and
  equalled percentage total plus flat total. Our fringe totals are integer sums of
  per-line cents (decision 2), so this always holds for us.
- Cutoff is a wage-base cap (pay above it earns no fringe), consistent with our
  `Percent { cutoff }`, where the cutoff caps the earner's subtotal. Whether the reference
  applies the cutoff per line, per range, or per person could not be verified from these
  frames (see note 13). Our rule: per earner (a line or a fringe range), decision 3.
- No mismatch found.

## Era caution

The 2026 note 01 shows the fringe list in the tools pane with checkboxes, dollars and
range buttons; the setup screen's columns were not re-checked in 2026.

## What this means for Method Budget

1. [have] P1 **Percent and flat fringes with optional cutoff, uses and total.**
2. [missing] P2 **Fringe short code (ID)**: an optional code shown in the grid's fringes
   column instead of the name, so long names stay readable in the grid.
3. [partial] P3 **Insert position and manual order**: new fringe inserted under the
   selection and table order kept (ours are alphabetical). Low priority; alphabetical is
   predictable.
4. [missing] P3 **Fringe table footer**: percentage total, flat total, overall total.
5. [missing] P3 **Folders for fringes** (per union or city), see note 09 item 4.
6. [partial] P2 **Cutoff wording**: our label says "Cutoff (pay per earner)"; keep the
   wage-base meaning explicit, since the reference's narration describes it loosely.
