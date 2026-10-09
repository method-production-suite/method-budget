# Behavior note 10: detail-level column toggles ("switches")

Source: "Toggling Switches", vendor tutorial video (published 2020/2021; vendor tutorial,
2020/2021 era), about 2 minutes, recorded on Windows. Clean-room: behavior in our own
words, read headless from captions and extracted frames. No budget data, copied help text
or images. The video stays outside the repo.

## Behaviors observed

- "Switches" are not a calculation feature: they are the show/hide toggles for optional
  columns on the detail grid. A small menu icon at the right end of the detail grid's
  header opens a checklist.
- Items in the 2021 checklist: aggregate fringe ("Agg"), fringes, groups, location, set,
  amount, units, X, 4X, currency, rate, subtotal. Turning one on adds its column at the
  left of the description (Agg, fringes, groups, location, set) or in place among the
  number columns.
- **Agg column**: one value per line, the line's combined fringe load from all its
  fringes. It was displayed as a ratio with two decimals (a line carrying about 40% in
  fringes showed 0.40), not as a percent.
- Lines with no fringes and lines whose total is zero showed Agg blank.
- A suppressed (greyed) line still showed its own Agg ratio.
- **Fringes column**: comma-separated fringe short names, truncated with an ellipsis.
- **Groups column**: comma-separated group names, colored like the line.
- **Location and set columns**: empty cells to fill per line.
- Fringe range brackets were visible in the leftmost gutter on lines that share a range.
- Clicking a line updates the tools pane: checkboxes for its fringes and groups, and each
  fringe's dollars for that line. Selecting one line inside a range showed that line's
  allocated share of each fringe.

## Calculation findings

- For a single line, each percent fringe's dollars equalled rate times the line total,
  rounded to the cent; the pane's aggregate percent equalled the sum of the line's fringe
  dollars over its total. This matches our per-line rounding (decision 2) and our
  "fringe percent" column (fringe dollars over line total).
- The tools pane showed fringe rates rounded to one decimal (a two-decimal rate displayed
  as one decimal) while the math used the full rate. See note 13.
- No mismatch in money. Display differences only: Agg as a ratio versus our percent, and
  the suppressed-line Agg (ours shows the fringe percent from the line's counted values;
  check what we show for a suppressed line).

## Era caution

Sam's 2026 note 01 lists a longer column menu (aggregate sum, unit2, unit3, unit4,
location, set, 4X) and a pinnable subtotal column, so the current product has more toggles
than this 2021 build. Note 01 wins.

## What this means for Film Wizard

1. [have] P1 **Column menu**: our corner menu toggles X, unit columns, currency, 4X,
   fringes, fringe percent, fringe dollars, groups, set and location, remembered per user.
2. [partial] P3 **Hide core columns too**: the reference also lets Amt, Units, X, Rate and
   Subtotal be hidden. Ours keeps the core columns fixed; probably fine.
3. [partial] P3 **Fringe percent on a suppressed line**: decide and test what the fringe
   percent column shows for a suppressed line (the reference shows the line's own ratio).
4. [have] P1 **Range brackets in the gutter**: we draw a bracket in the # column.
