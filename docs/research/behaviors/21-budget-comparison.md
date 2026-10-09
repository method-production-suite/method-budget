# Behavior note 21: budget comparison view

Source: vendor tutorial video "Budget Comparison" (published 2020 to 2021, vendor tutorial,
2020/2021 era, about 3 minutes 14 seconds). The caption track is music-only noise, so this
note is from extracted frames alone. Clean-room: behavior and arithmetic in our own words, no
frames, no vendor data.

## Behaviors observed

- Entry point: a toolbar button whose subtitle shows the current comparison state ("None",
  then the linked budget's name when one is linked, then "Multiple" with two or more).
- It opens a dropdown panel with a search box and a list of the user's budgets (name,
  project, library, last opened) each with a Link or Unlink action. Linked budgets move to
  the top and get letters (A, B, ...) in a left column. Typing filters the list by name.
- The open budget stays the base ("this"). Linking turns the topsheet into a comparison grid
  immediately; unlinking the last one returns the normal topsheet.
- Columns with two linked budgets: account, description, Total (A), Total (B), Total (A+B),
  Avg (A,B), Total (this), Var, %. With one linked budget: Total (A), Total (this), Var, %.
- Rows: categories matched by account number, production-total rows, total fringes, total
  other charges, ATL, BTL, ATL+BTL, grand total. Each row has a small dot by its row number
  (probably a "matched" marker).
- Charges and credits are NOT matched across budgets: each budget's charge and credit rows
  are listed separately below the grand total, with a value only in that budget's column
  (and blank in the others).
- Drilling into a category shows its accounts compared the same way (matched by account
  number), with the same columns and a total row. The breadcrumb works as normal.
- Selecting a row in compare mode still drives the tools pane (fringe checkboxes reflected
  the selected category's lines in the base budget).
- Reports menu gains a "Budget Comparison" quick report: same columns, percent printed to two
  decimals, a header naming which budget is A and which is B, a per-column on/off menu
  (A+B, average, variance, percent), landscape by default, export to PDF and print.

## Calculation findings vs our engine

Worked out from the numbers on screen (checked on six or more rows; the tutorial's sample
figures are deliberately not reproduced here):

- Total (A+B) is the sum of the linked budgets' amounts for that row.
- Avg is that sum divided by the number of linked budgets.
- **Var = Total (A+B) minus the open budget's amount.**
- **% = Var / Avg**, not Var / the open budget's amount. The report shows two decimals; the
  grid rounds to whole percent. With one linked budget the average is that budget, so
  % = (linked - open) / linked.
- With two or more linked budgets the variance mixes a sum with a single base, so it is not
  a "difference from base" at all; with one linked budget it is linked minus base.

**Mismatch with decision 17 (flag, by design not a bug):** our `compare` gives each budget's
own difference from the FIRST budget (value minus base, percent of base). The reference shows
one aggregate variance (sum of others minus base, percent of the others' average). Ours is
the clearer per-budget view; the reference's sum and average columns are extra aggregates we
don't compute yet. The reference also lists charges and credits per budget instead of matching
them; we match them as single totals lines (`charges`, `credits`), which is fine but different.

Also not in our `CompareTotals`: production-level totals (the reference compares each
production-total row). Our ATL/BTL lines cover the first split only.

## Shortcuts seen

None.

## Era caution

2020/2021 build, and cloud-only at the time (linked budgets had to be stored in the vendor's
cloud). Sam's 2026 recordings win any conflict. Column names and the % definition could have
changed; re-verify the percent rule against a current export before copying the semantics.

## What this means for Film Wizard

Local-first: "link a budget" becomes "add a `.fwb` file (or a saved version) to the
comparison"; recent files and versions inside the open budget are the picker's list.

1. [partial] P1: **Comparison UI.** Engine done (decision 17, `comparison.rs`), no app
   command or view yet. Needs a picker (recent files, browse, versions of this budget), a
   compare mode on topsheet and account levels, and the base kept editable.
2. [missing] P2: **Aggregate columns**: sum of compared budgets and average, as optional
   columns, alongside our per-budget differences. Optionally a "variance vs average" view
   for people used to the reference's numbers, clearly labelled.
3. [missing] P2: **Production-level rows in compare** (each production total, not just
   ATL/BTL).
4. [partial] P2: **Charges and credits in compare**: we compare them as totals; add per-item
   rows matched by name (or listed per budget when names differ).
5. [missing] P1: **Comparison report** (print/PDF) with per-column toggles and a legend of
   which file each letter is.
6. [missing] P3: **Compare-mode drill-down to detail**: our engine has `compare_account`
   (rows matched by id or description), which the reference lacked in this era. A small
   advantage worth surfacing.
7. [have] Matching by account code across different coding patterns; versions as compare
   sources (decision 17).
