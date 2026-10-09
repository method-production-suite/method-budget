# Behavior note 13: applying fringes to lines

Source: "Applying Fringes", vendor tutorial video (published 2020/2021; vendor tutorial,
2020/2021 era), about 2 minutes, recorded on Windows. This video has no captions, so
everything below comes from extracted frames. Clean-room: behavior in our own words. No
fringe names, rates or budget data, no images. The video stays outside the repo.

## Behaviors observed

- Starting at the topsheet, the user drills into a category, then an account, by clicking
  a row's leading cell. The detail grid opens with the first line selected.
- With the fringes column switched on (note 10), each line lists its fringes.
- The user selects a run of consecutive lines (the labor lines of one person: prep,
  shoot, wrap style) and ticks fringes in the tools pane. Each ticked fringe is added to
  every selected line at once; the fringes column updates on all of them.
- The pane shows, per fringe, its dollars within the selection, and an "aggregate total"
  percent for the selection that steps up as each fringe is ticked.
- The user then clicks the add-range button with the same lines selected: a bracket
  appears in the gutter joining them into one fringe range.
- The pane's clear link was highlighted at the end (clears the fringes from the selected
  lines; its effect was not shown).
- A line further down that is a fee rather than wages was left out of the selection and
  kept its own, different fringes.

## Calculation findings

All verified from numbers on screen:

- **Fringes add to the totals, not to the line**: after ticking two percent fringes on the
  selected lines, the account's detail total row did not change, while the status bar's
  below-the-line total rose by exactly the two new fringe dollar amounts. Fringes posting
  outside the line matches our model ("they never show on the line itself") and our
  default category posting.
- **Selection amounts are exact**: each fringe's dollars for the selection equalled rate
  times the selected lines' combined total, and also equalled the sum of per-line rounded
  amounts. Consistent with decision 2 (per-line or per-earner rounding).
- **Display rounding of rates**: one fringe displayed its rate with one decimal in the
  pane, but its dollars proved the math used the full two-decimal rate. The pane rounds
  the rate for display only.
- **Cutoff per range**: the range was made after the fringes were applied, and none of
  the fringes in view hit a cutoff, so the per-range cutoff could not be confirmed here.
  Our rule (decision 3) remains an assumption to verify with Sam on a 2026 budget: make a
  range whose combined pay crosses a cutoff and compare with each line alone.
- No mismatch found.

## Era caution

Sam's 2026 notes 01 and 02 show the same select-then-tick workflow, range buttons and
selection dollars, plus the "copy fringes from the line above" preference on insert.
This 2021 video agrees with them.

## What this means for Film Wizard

1. [have] P1 **Apply panel**: tick a fringe to put it on every selected line, dollars per
   fringe within the selection, effective fringe percent, Make range and Remove range.
2. [have] P1 **Fringes post outside the line** and roll into ATL/BTL by the posting rule.
3. [partial] P1 **Cutoff across a range, verified against the reference**: we compute it
   per earner; get one 2026 recording where a range crosses a cutoff to confirm.
4. [have] P2 **Full-precision rates**: keep showing the rate as entered; never round the
   displayed rate to fewer places than the math uses (the reference's one-decimal display
   hides a real difference).
5. [partial] P2 **Clear fringes from the selection** in one click (ours: click each
   applied fringe to take it off). A "clear all" per list would match.
