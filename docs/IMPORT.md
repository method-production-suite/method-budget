# Importing a budget export

Method Budget can make a whole budget from another budgeting program's open exports, and
checks the result against the export's own figures before you rely on it (roadmap 1.16,
decision 59). Five layouts are read:

| Layout | What it is | `--format` |
|--------|------------|------------|
| Budget exchange JSON | One `.json` file: metadata, the topsheet, accounts and detail rows, with every figure. | `json` |
| Budget exchange XML, advanced | The same as the JSON, as XML elements. | `xml-advanced` |
| Budget exchange XML, basic | The same structure without metadata, without the lines' fringes, groups and globals, and with every figure in whole units. | `xml` |
| Budget export workbook | An `.xlsx` with four sheets: Budget Metadata, Topsheet, Categories, Account Details. No fringe rates. | `xlsx` |
| Tab-delimited topsheet | A `.txt` with the topsheet and each account's total, in whole units. Nothing below the accounts. | `tab` |

The layout is found from the file's content; `--format` (or the dialog's **Read as**) reads
it as another. Prefer the JSON or the advanced XML: they carry the most.

- **Command line**: `method-budget import-export <file> [-o out.mthd] [--report] [--json]`
  (docs/CLI.md). Without `-o` it only reads and compares.
- **App**: File > Import Budget Export… shows the review (what came over, the check against
  the export's totals, the notes) and opens the budget in a new tab, unsaved (docs/APP.md).
- **MCP**: the `import_budget_export` tool (docs/MCP.md).

## What comes over

Categories, accounts and detail rows in order, with descriptions, Amount, X, Rate and 4X as
the file writes them (a number such as `0.3333333333333333` keeps every digit), units,
currencies, fringes, groups, sets and locations by name, adjustment rows, the budget
information fields (title, production, season, shoot days, dates...), and the originals as
the budget's baseline. The empty entry rows the exporting program keeps at the end of each
account and category are passed over (an empty row between lines stays, as an empty line).

## What is worked out from the figures

An export doesn't carry everything the budget had. Where its figures decide it, the import
works it out, and the notes list each one under **Check**:

- **Account numbering**: the pattern the category and account numbers follow.
- **Fringe posting**: a Total Fringes row at the end of each account is posting by account;
  a fringe account in each category (`1199 Total Fringes`) is by category, with its suffix;
  Total Fringes rows between the categories are by production level, and a production total
  goes in after each (the export doesn't name them: they are called Production total 1,
  2...); one Total Fringes row after the categories is by budget. When categories follow the
  last level's fringe row and the export leaves their fringes out, the budget is set to
  leave them out too (`trailing_level_fringes: drop`).
- **Fringes**: name, kind and rate from the fringe rows' figures. A percent fringe's
  **cutoff** is worked out from what it was charged on against what its lines come to, and
  **fringe ranges** from the lines that earned less than their rate (a line that earned
  nothing once the cutoff was used up, or less than its own total allows): such a line
  joins as few of the lines above it as fit its percentage best, passing over lines that
  come to nothing (a 0-day premium) and counting what its flat fringes make; cutoffs and
  ranges are worked out in turn until they settle. Where the lines' two-place percentages
  can't show a range (a fitting that takes a shoot just over a cutoff), an account whose
  fringes still come out over gets the join that brings them to the export's figure, and
  the cutoffs are worked out again. The workbook has
  no fringe rates: they come from each line's fringe percentages, and the cutoffs from the
  lines' percentages and the accounts' fringe totals (any cutoff that gives the export's
  totals exactly; the middle of the range that does). The basic XML doesn't say which
  fringes each line has: a line gets the one set of fringes whose percentages add up to its.
- **Adjustment rows**: a row in the base (or totals) currency whose rate is the total of the
  rows above it since the previous adjustment row.
- **Exchange rates**: from each foreign line's total against its own amount (the rate with
  the fewest decimal places, up to 6, that gives every line of that currency). A budget that
  shows its totals in another currency keeps that setting.
- **Unit hours**: the usual starter hours (Day 12, Week 60, Hour 1, Month 240); a unit the
  budget added gets its hours from what the flat fringes counted when only one value fits.
- **Percent charges**: a charge that is a simple percentage of the total before charges
  (`Contingency 10%`) comes in as a percent charge with no exclusions: the percentage has
  at most 2 decimal places (the fewest that work, and only one value at that many), our
  engine gives the export's figure to the cent with it, and a step of that size moves the
  charge by 1.00 or more (so a fixed fee on a small budget isn't taken for a percentage).
  Any other charge comes in as a fixed amount. Layouts with cents in the base currency
  only.
- **Charges and credits with account numbers**: a topsheet row with a number but no fringe
  figure and no accounts (`600-00 Contingency`, `900-01 Tax credit`) is a charge, or a
  credit when it is negative, posted to that number; an empty category still shows a
  fringe figure of 0.
- **Percent units**: a line whose Amount, X, 4X or Rate is in `%` (also `Pct`, `Percent`)
  holds a percentage: when the line's total in the export is the product over 100, that
  number comes in divided by 100 (`1.5 %` of 5,500,000 is Amount `1.5/100`, 82,500.00),
  the unit is dropped and the line has a note.
- **Account totals in whole units**: when every account total in the export is its lines
  rounded to a whole unit and some aren't their lines as they are, the exporting budget
  rounds account totals (the other program's "Decimal Places in Totals" at 0), so the
  budget does too (`round_account_totals`, Settings > Calculation): categories, charges
  and the grand and net totals then come out of the rounded accounts as the export's do.
  A budget shown in another currency keeps cents and the notes say so.

## What can't come over

Listed in the notes under **Not imported**:

- **Formulas and globals.** Exports keep only values. Each line that used globals keeps its
  numbers and gets a note naming them; no global is created.
- **Production totals**, except under production-level posting: above- and below-the-line
  are 0 until they are added in Budget Setup.
- **Lines left out of the totals by a group** (suppressed lines): the exports omit them, and
  their groups unless other lines use them. The groups that come over count in the total.
- **Charges and credits** come in as fixed amounts that give the same totals (but simple
  percent charges, above); their exclusions, rows, caps and discounts aren't in the
  export. A credit is never made a percentage: its spend can be a group, a category, an
  account or a flat amount, and the export doesn't say which. In the layouts with whole
  units they come in at the whole unit.
- **Unit hours** of the starter units, currency names and symbols, location details, fringe
  descriptions, line notes, styles and fringe cutoff factors.
- The basic XML has no groups or globals on its lines; the tab-delimited file has no lines
  at all: each account comes in as one line holding its total (fringe accounts too), and
  its Total Fringes rows as fixed charges.

## The check against the export's totals

The imported budget is recalculated by our engine (its arithmetic setting, by default as the
reference product works) and compared with the export's own figures: every line, account
(total and fringes), fringe account or Total Fringes row, category (total and fringes),
charge, credit, the grand total (categories, fringe rows and charges added up) and the net
total. Each kind says how many were compared and how many differ, then every figure that
differs is listed with both values.

- Layouts in whole units are compared rounded to whole units.
- A budget shown in another currency is compared as the export shows it: each line, and
  each fringe's total, converted and rounded, then added up.
- A difference is information, not an error: the budget still imports. Usual causes are a
  cutoff or range the figures didn't decide, unit hours of a unit no fringe counted, and
  figures the exporting program only shows (an adjustment row converted in the totals
  currency).

The exports of the 2026-10-10 and 2026-10-11 parity runs all come to their own figures, but
one shown in pounds, which is off by 0.01 to 0.03 there; the JSON writes a 100.005 charge as
100.00 where the workbook keeps 100.005 (shown as 100.01). A feature budget's JSON exports
(2026-10-12), written with account totals in whole units and with cents, both come to
their own figures (decisions 68 and 69).
