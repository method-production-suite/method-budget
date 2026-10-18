# The `method-budget` command line

`method-budget` reads and writes Method Budget files (`.mthd`: docs/FORMAT.md) without the app: check a budget, print its totals, draw a report, export a spreadsheet, import one,
and make scripted edits. It is plain Rust with no network and no GUI, so it runs the same
on a laptop, a server or in CI. Build it with `cargo build --release -p method-cli`; the
binary is `target/release/method-budget`.

**Renamed.** The command used to be `method`. That name still works for now: it is a
second binary built from the same code (`cargo install` puts both on the `PATH`), and it
prints one line to stderr, ``note: `method` is now `method-budget`; the old name will be
removed in a future release.``, before running as usual. It prints nothing extra for
`mcp` and `merge-driver`, so MCP client configs and git config that still say `method`
keep working quietly; update them when convenient. Every app in the suite names its
command line `method-<noun>` and its desktop executable `method-<noun>-app`.

Every command takes the budget file first, then flags. Flags take their value as the next
word or after `=` (`--out=x.pdf`); `-o` is short for `--out`; `--` ends the flags. A word
that looks like a negative number (`-5`) is a value, not a flag. `method-budget help` lists every
command. `method-budget --version` (or `-V`) prints the version, the same one the app's About box
shows (docs/RELEASING.md).

Commands that write a file write it **atomically**: to a temporary file in the same
folder, then renamed over the target, so a reader never sees half a file and a failed
write leaves the old file as it was. A replaced file keeps its permissions. No command ever writes over the budget it is reading
from unless that is its job (`fmt`, `set`, `apply`).

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | It worked. |
| 1 | Something failed: a file couldn't be read, parsed or written, the budget has a calculation problem, or an output already exists. |
| 2 | The command line is wrong: unknown command or flag, a missing argument or value. The message ends with the command's usage. |
| 3 | An edit was refused (`set`, `apply`): a bad script line, a name that doesn't exist, or a result that doesn't recalculate. **Nothing was written.** |

Commands added later may define further codes in their own sections. Errors go to
stderr as `method-budget: <message>`; results go to stdout.

## Commands

### `method-budget show <file.mthd> [--exclude GROUP]...`

Prints the topsheet for a person to read. `--exclude` leaves a group's lines out
(repeatable). Rows come in the topsheet's order (decisions.md, entry 60): categories and
production totals, the summary rows, the budget's fringe row (with its number and
description, where the budget places it), charges newest first, the grand total, credits
in reverse application order, their Sub Total when there are two or more, and the net
total. With totals shown in another currency, the last line is the net total in it,
converted account by account and written in that currency's own separators and symbol
(`49.253,40 €`; decisions.md, entry 61).

### `method-budget check <file.mthd>`

Reads and recalculates the budget. Prints `<file>: ok, net total 1,234.56`, or fails
(exit 1) with the reader's or the engine's message, such as
`line 40, column 21 (budget.topsheet[2].accounts[0].details[3].rate): ...`. A budget that
adds up can still print warnings after the ok line, one per line as `<file>: warning:
...`, without failing: fringes posted by production level that belong to categories
after the last production total, which no level holds (decisions.md, entry 56; kept in
the grand total unless `trailing_level_fringes` is `drop`), and credit rows on a
category or account the budget doesn't have, which count 0 (entries 57 and 61).

### `method-budget fmt <file.mthd>`

Rewrites the file in canonical form (key order, decimals, indentation), the bytes the
app would save.

### `method-budget new <file.mthd> [--template NAME]` and `method-budget sample <file.mthd>`

Write a starter budget, or a large invented test budget (5,000 rows). Both refuse to
replace an existing file. `--template` starts instead from one of the bundled charts of
accounts (its id or name, any case; `method-budget coa list` names them): its categories, accounts
and production totals, no lines, titled Untitled (docs/TEMPLATES.md). A new budget, the
starter or a chart, starts with the new-budget settings (decisions.md, entry 60): fringes
posted by budget, a Day of 12 hours, a Week of 60 and a Month of 240, a cutoff factor of
1.0, and (the starter) account numbers of category 2, account 2 and set 3 with no
separator, so the starter's accounts read `1001` to `5001` (decisions.md, entry 61).

### `method-budget totals <file.mthd> [--json] [--exclude GROUP]...`

The totals, for pipelines. Plain output is one tab-separated line per total, exact
decimals with no thousands separators:

```text
category	10	55000.00
category	11	82500.00
total_atl	294250.00
total_btl	231750.00
atl_btl	526000.00
budget_fringes	0.00
charges_total	52600.00
grand_total	578600.00
credits_total	0.00
net_total	578600.00
```

`--json` prints one object with every amount as an **integer of cents**, as the file
format stores money: `title`, `base_currency`, `totals_currency`, `categories` (each
with `code`, `name`, `total`, `fringe_total` and its `accounts`' `code`, `name` and
`total`), `levels` (production totals), `charges`, `credits`, the headline totals above,
and `net_total_in_totals_currency`: the net total in the totals currency, converted
account by account, fringe by fringe, charge by charge and credit by credit, then added up
(decisions.md, entry 60); the net total itself when totals show in the base currency.

```sh
method-budget totals film.mthd | awk -F'\t' '$1 == "net_total" { print $2 }'
```

### `method-budget export <file.mthd> --xlsx|--csv|--tsv <path> [--bom]`

Also `--format xlsx|csv|tsv --out <path>`. The evaluated budget as an Excel workbook
(Topsheet, Accounts, Detail and the setup tables), as one CSV file per table (named
`<stem> - <Table>.csv` beside the path), or the Detail table as tab-delimited text for
accounting tools (decision 30). A folder as the path names the file after the budget.
`--bom` starts CSV and TSV with a UTF-8 byte order mark, which Excel needs for accented
letters.

### `method-budget report <file.mthd> [--preset NAME] [--sections LIST] [--format F] [-o PATH] [--date TEXT] [--bom]`

Draws a report from one of the budget's saved report presets (docs/FORMAT.md, "Report
preset"): its sections in order, category and account exclusions, skipped accounts and
lines, "only these groups", detail columns, notes, orientation, type size, shading,
vertical rules, collapsed detail, the topsheet fringes column, header and footer.

- `--preset NAME` picks a preset by name (exact, else ignoring case). Without it, the
  budget's default preset, else the topsheet and the detail with the standard columns.
- `--sections topsheet,detail,...` replaces the preset's sections. Names: `topsheet`,
  `accounts`, `detail`, `fringes`, `globals`, `groups`, `notes`, `info`, `chart`,
  `units`, `sets`, `locations`, `currencies`.
- `--format html|txt|csv|pdf`, else from the extension of `-o`, else `txt`.
- `-o PATH` writes the report; without it (or with `-o -`) it goes to stdout. PDF needs
  `-o`.
- `--date "2026-10-09 14:30"` sets the printed date and time (the header's standard
  right slot); without it the clock is read, in UTC. Use it when the output must be the
  same on every run.
- `method-budget report <file.mthd> --list-presets` lists the presets, the default marked `*`.

Like the app's reports, a filter never changes a total: leaving accounts or lines out
prints the budget's own totals with a note, and the topsheet always lists every
category. Original and variance columns print only when the budget has a baseline.

The formats:

- **html**: one self-contained page (inline styles, no scripts) laid out for Letter
  paper in the preset's orientation. Print it from any browser; browsers repeat column
  titles on every page. Page numbers are left to the browser's own print header and
  footer.
- **txt**: fixed-width columns, figures right-aligned. Text cells over 60 characters are
  cut with `...`; notes wrap.
- **csv**: every section's tables in one file, one after another: a row with the
  section's title, the column titles, the rows, a blank line. Money is an exact decimal
  (`1234.50`), and text that a spreadsheet would run as a formula is neutralized, as in
  the spreadsheet export.
- **pdf**: the txt layout set in Courier on Letter pages, each section starting a page,
  with the header and footer on every page and "Page X of Y". Written by a small PDF
  writer in the command line itself (no crates, standard fonts, nothing embedded); the
  type shrinks, down to 4 points, until the widest line fits. Latin letters and common
  punctuation print; other characters print as `?`. It is a plain printout for
  pipelines, not the app's typeset PDF (decision 28), which needs the Mac app's web view.
  Shading, colors and the topsheet page column are on-screen and app-PDF features only.

### `method-budget set <file.mthd> <path> <value> [--dry-run]`

Changes one value and saves. Paths:

| Path | Value |
|------|-------|
| `title` | Text. |
| `compact_versions` | `true` or `false`: save versions as deltas (docs/FORMAT.md, "Version deltas"). |
| `arithmetic` | `binary` (the default: totals worked out in binary doubles, as spreadsheets and industry-standard budgeting software work them; `mmb` is still accepted) or `exact` (exact decimals, half away from zero). See decisions.md, entry 55. |
| `round_account_totals` | `true` or `false` (the default): round every account's total to a whole unit and work out categories, charges and the grand and net totals from the rounded accounts. See decisions.md, entry 69. |
| `trailing_level_fringes` | `keep` (the default) or `drop`: under production-level posting, keep the fringes of categories after the last production total in the grand total, or drop them from every total. See decisions.md, entry 56. |
| `cutoff_factor` | `1.0` (the default), `1.5`, `2.0`, `2.5` or `3.0`: what every percent fringe's cutoff is multiplied by. See decisions.md, entry 60. |
| `fringe_row.account` | The account number on the budget's fringe row (fringes posted by budget), or `none`. Display only. |
| `fringe_row.description` | What the budget's fringe row says, or `none` for `Total Fringes`. Display only. |
| `fringe_row.position` | The topsheet row the budget's fringe row shows above, counted from 0 (the number of topsheet rows puts it after the last one), or `none` for after the summary rows (the default). Changes no total. |
| `group_rules.ungrouped_lines` | `include` (the default) or `exclude`: lines and adjustment rows with no group count, or are left out. See decisions.md, entry 57. |
| `group_rules.conflict` | `exclude` (the default) or `include`: a line in a group that counts and a group that doesn't is left out, or counts. |
| `group_rules.fringes_in_totals` | `include` (the default) or `exclude`: group totals shown with or without their fringes. Display only. |
| `global.NAME` | A formula. Adds the global when it's new. |
| `fringe.NAME.rate` | A formula. |
| `fringe.NAME.cutoff` | An amount (`168,600.00`), or `none`. Percent fringes only (`none` also clears a cutoff a flat fringe kept, which it never uses). |
| `fringe.NAME.basis` | What a flat fringe counts on a line: `amount` (the standard: the Amount only, converted through unit hours), `amount_counts` (also X and 4X when their units aren't time units) or `quantity` (Amount x X x 4X). Flat fringes only (decisions.md, entry 54). |
| `fringe.NAME.max_hours_per_day` | Most hours a day a flat fringe charged per hour counts (`12`), or `none`. |
| `line.ROW.FIELD` | See `set line` below for rows and fields. |
| `charge.NAME.applied` | `true` or `false`. |

A name or account code may contain dots: the path splits at its first and last dots.
`none` (or an empty `""`) clears an optional value. The command prints each change and
the totals that moved; `--dry-run` prints the changes and a full `method-budget diff`-style
summary and writes nothing. A refused edit exits 3 and writes nothing.

```sh
method-budget set film.mthd line.21-01:2.rate 900
method-budget set film.mthd fringe.FICA.cutoff 168,600.00 --dry-run
```

### `method-budget apply <file.mthd> <script.txt> [--dry-run]`

Runs an edit script (below) and saves. `-` reads the script from stdin. Every line must
succeed and the result must recalculate, or nothing is written and the command exits 3
naming the line (`edit.txt line 4: there is no fringe FICA`). The output lists every
change, then the totals before and after; `--dry-run` shows the changes and a full
diff summary and writes nothing.

### `method-budget import <sheet> -o <out.mthd> [--mapping M.json] [--into B.mthd] [--title T] [--currency C] [--force] [--dry-run]`

Adds a sheet's rows to a budget as detail lines, with the app's own importer (decision
36): `.xlsx` workbooks, CSV and tab-delimited text, with the same limits, account
matching, reconciliation and cautious defaults.

- `--into B.mthd` imports into a copy of that budget, whose chart and setup give the lines
  their meaning (fringes, globals, currencies). Without it the lines go into a new empty
  budget titled after the sheet's file (`--title` and `--currency`, default USD, change
  that); give the mapping `"create_accounts": true` then.
- `-o` is refused when it exists, unless it is the `--into` budget or `--force` is given.
- The output says how many lines went in, what was added, every skipped row and warning
  (the first 20), and how the sheet's line totals compare with the budget's.

The **mapping file** is JSON; every field may be left out:

```json
{
  "sheet": "Detail",
  "header_row": 1,
  "columns": { "account_code": "A", "description": "B", "amount": 3, "unit": "D", "rate": "E", "line_total": "H" },
  "create_accounts": false,
  "create_setup": false,
  "default_account": ""
}
```

| Field | Meaning |
|-------|---------|
| `sheet` | A sheet name (any case) or number from 1. Default: the visible sheet whose titles name the most fields, as the app's import dialog starts. |
| `header_row` | The row of column titles, from 1; `"none"` for no titles; `"detect"` (the default) finds it. |
| `columns` | Field to column, as a letter (`"A"`, `"AA"`) or a number from 1. Default: every column guessed from its title. Fields: `category_code`, `category_name`, `account_code`, `account_name`, `kind`, `description`, `amount`, `unit`, `x`, `rate`, `currency`, `x4`, `fringes`, `fringe_range`, `groups`, `set`, `location`, `note`, `line_total`. |
| `create_accounts` | Add accounts and categories the budget doesn't have (else their rows are skipped). |
| `create_setup` | Add fringes, groups, units, sets, locations and currencies it doesn't have (else they are left off the lines). |
| `default_account` | The account for rows without one. |

Our own Excel export imports back with no mapping at all.

### `method-budget import-export <export> [-o <out.mthd>] [--format F] [--title T] [--report] [--json] [--force]`

Makes a whole budget from another budgeting program's export and checks it against the
export's own figures (docs/IMPORT.md): budget exchange JSON or XML (basic or advanced), its
Excel workbook or its tab-delimited topsheet. The layout is found from the content;
`--format json|xml|xml-advanced|xlsx|tab` reads it as another.

- It prints what came over (categories, accounts, lines, setup), then the notes: what was
  worked out from the figures and should be checked (`check:`: fringe posting, cutoffs,
  ranges, exchange rates, unit hours), what couldn't come over (`not imported:`: formulas
  and globals, production totals, the rules of charges and credits, suppressed lines), and
  `note:` lines.
- Then the check against the export's totals: the grand and net totals, how many lines,
  account totals and fringes, fringe accounts or rows, category totals and fringes, charges
  and credits were compared and how many differ, and the first 20 differing figures with
  both values (`--report` lists every one).
- `-o` writes the budget (refused when it exists, unless `--force`); without it nothing is
  written. `--title` names the budget (default: the export's title, else the file's name).
- `--json` prints the same as JSON: `format`, `title`, `counts`, `notes` (`kind`: `check`,
  `not_imported` or `info`), and `reconciliation` (`currency`, `matches`, `calc_error`, the
  grand and net totals in cents, `tallies` per kind, `differences` with `export_cents` and
  `imported_cents`, `differences_not_listed`), plus `written`.

Exit code 0 whenever the file was read (a figure that differs is reported, not an error); 1
when it can't be read or `-o` exists; 2 for a bad `--format`.

```sh
method-budget import-export "Pilot budget.json"                 # read and compare only
method-budget import-export "Pilot budget.json" -o pilot.mthd   # and write the budget
method-budget import-export pilot.xml --report --json > check.json
```

### `method-budget schedule-days <budget.mthd> <days.json> [--pick KEY[=GLOBAL]]... [--skip KEY]... [--apply] [--json]`

Sets a budget's globals from a schedule's day counts, as the app's File > Import Schedule
Days… does (docs/APP.md, "Globals from a schedule's days"). The file is the days file
Method Schedule writes for a budget (`format: "method-schedule-days"`, versions 0 and 1;
docs/research/schedule-exchange.md).

- Every figure the file counts has a key: `total` (shoot days in all), `unit:NAME`,
  `set:NAME`, `location:NAME`, `week:YYYY-MM-DD` (the Monday) and `element:ID` (an
  element's days in all, the day out of days total).
- The shoot days in all go to `SHOOT_DAYS`. Any other figure only with `--pick KEY`, to its
  default global (the name in capitals with `_DAYS`: `location:Cold Spring field station`
  is `COLD_SPRING_FIELD_STATION_DAYS`) or to the one named after the last `=`
  (`--pick "location:Cold Spring field station=STATION_DAYS"`, an existing global to
  update it). A default is never a global the budget already has: that gets `_2`, `_3`...
  (in the Field Station budget `set:RIDGE` defaults to `RIDGE_DAYS_2`, since `RIDGE_DAYS` is
  the fire road location's); name the existing global with `=GLOBAL` to update it.
- Each global set records where its figure came from (its setup source), with the
  figure's key; for an element, its id, category, board ID and name. Running again with
  the same schedule finds each global's figure by that key (a renamed or renumbered cast
  member keeps its link; one whose name and number both changed, or that now matches
  several elements, is listed unpicked with a note asking which, never at 0), picks those globals again and says which ones the schedule has changed. A global
  set from a figure the file no longer has (a location dropped or renamed, an element cut)
  is listed under the key `linked:GLOBAL` at 0 days, unpicked: `--apply` leaves it alone,
  and only `--pick linked:GLOBAL` sets it to 0. `--skip KEY` leaves a picked figure out.
- Prep and wrap globals are never touched: they are the budget's own assumptions.
- It prints the schedule, the globals it would set (each with its new value, the old one
  and the figure's key), every figure not picked with its key and default global, and how
  many globals are new, changed or already set. Nothing is written unless `--apply`, which
  saves the budget atomically. A new global gets the figure as its description (and the
  Day unit when the budget has one).
- `--json` prints the review: `schedule`, `board`, `sub_board`, `version`, `days_hash`,
  `shoot_days`, `start`, `end`, `notes`, `figures` (each with `key`, `kind`, `label`,
  `days`, `global`, `picked`, `linked`, `current`, `current_value`, `new`, `action`: `add`,
  `update` or `same`, `changed`, `problem`, `note`), `adds`, `updates`, `same`, `changed`,
  `problems`, `can_apply`, `written` and `message`.

Exit code 0 when the file was read (and, with `--apply`, the budget written); 1 when a file
can't be read or isn't a schedule days file; 3 when a pick is refused (a key the file
doesn't have, a name that can't be a global, two figures in one global), with nothing
written.

```sh
method-budget schedule-days film.mthd field-station.budget.json          # review only
method-budget schedule-days film.mthd field-station.budget.json \
  --pick "location:Cold Spring field station=STATION_DAYS" --apply
```

### `method-budget review <budget.mthd> [<review.json>] ...`

A review file's comments on a budget (roadmap 3.3; docs/FORMAT.md, "Review files"), as the
app's Comments tab works with them (docs/APP.md, "Review files"). The review file is
`<budget>.review.json` beside the budget unless named.

- `method-budget review film.mthd [--json]` lists every comment at its place in the budget
  (the whole budget's first, then categories, accounts and lines in budget order, then
  setup items, then those whose place is gone, marked `gone:` with where they were), with
  its author, time, status (`open`, `resolved`, `merged`, `dismissed`) and why it can't
  become a note when it can't, then how many are left to settle.
- `--add ON --text TEXT [--author NAME] [--at TIME]` writes a comment, making the review
  file when there is none; the budget is not touched. `ON` is `row:ID`, `account:NUMBER`
  (or just the number), `category:CODE` (or just the code), a setup item as `KIND:NAME`
  (`fringe:FICA`; `global`, `group`, `unit`, `set`, `location`, `currency`, `charge`,
  `credit`) or `budget`. It must be in the budget now.
- `--merge ID|all` (repeatable) puts comments into the budget as dated notes on their line,
  account or category (`all`: every one to settle that can be); `--dismiss ID` sets one
  aside; `--by NAME` says who. A comment whose place is gone, or on a setup item or the
  whole budget, stays open and is counted. Nothing is written unless `--apply`, which
  saves the budget and the review file (with each comment's outcome) atomically. Merging
  a comment already in the budget adds nothing.
- `--at` is the time written into a comment or an outcome (ISO 8601); by default the time
  now, in UTC.
- `--json` prints the list: `file`, `budget` (the reviewed budget's title and, for a copy
  saved for sharing, where it came from), `title`, `matches`, `comments`, `to_settle`,
  `entries` (each with `id`, `on`, `text`, `author`, `created`, `edited`, `resolved`,
  `outcome`, `status`, `found`, `place`, `written_at`, `can_merge`, `why_not` and where it
  is: `ti`, `ai`, `row_id`, `line`, `category`, `account`) and `message`.

Exit code 0 when the files were read (and written, with `--add` or `--apply`); 1 when a
file can't be read or isn't a review file; 3 when a comment is refused (its place isn't in
the budget, blank text, an id the review doesn't have), with nothing written.

```sh
method-budget review film.mthd --add 21-01 --text "Why ten weeks of prep?" --author Jordan
method-budget review film.mthd                               # each comment at its place
method-budget review film.mthd --merge all --dismiss 3 --by Sky --apply
```

### `method-budget coa import <chart> -o <out.mthd> [--sheet N|NAME] [--mapping M.json] [--title T] [--currency C] [--into B.mthd] [--remove-unlisted] [--force] [--dry-run]`

Reads a chart of accounts (category and account numbers, names and production totals,
nothing else) from an Excel workbook, a CSV or tab-delimited file, or another budget file,
and writes it as a new budget with no lines: a template, titled after the file unless
`--title` says otherwise. The header row, the columns and the numbering pattern are found
(docs/TEMPLATES.md lists the shapes it reads, and the `kind,code,name,parent` layout that
needs no guessing, keeps every number as written and places each account under the
category it names); `--sheet` picks a sheet, `--mapping` names the rest. It prints what it
read (counts, the pattern, rows skipped and anything to check, including each repeated
account number given a new one, `10950` as `10950-2`).

`--into B.mthd` applies the chart to a copy of that budget instead and writes the result to
`-o` (which may be the same file): categories and accounts it lacks are added in number
order, the ones it has are renamed to the chart's names, numbers are carried over to the
budget's own pattern (ones that don't fit are left out and listed), production totals are
left as they are. Accounts the chart doesn't list are kept and listed; with
`--remove-unlisted` the empty ones (no lines, no notes) are removed, and categories the
chart doesn't list once they are empty. An account with lines is never removed.

The same input always writes the same bytes, which is what makes the bundled templates
reproducible (`scripts/build-coa-templates.py` runs this command).

A chart mapping file:

```json
{
  "sheet": 1,
  "header_row": 3,
  "columns": { "category_code": "A", "account_code": "A", "account_name": "B", "level": "C" },
  "coding": { "category": 2, "account": 2, "sub": 0, "separator": "-" },
  "sub_accounts": false,
  "default_levels": true
}
```

| Field | Meaning |
|-------|---------|
| `sheet` | A sheet name (any case) or number from 1. Default: the visible sheet with the most numbers. |
| `header_row` | The row of column titles, from 1; `"none"`; `"detect"` (the default). |
| `columns` | Field to column (letter or number from 1): `category_code`, `category_name`, `account_code`, `account_name`, `level`. The category and account number may be the same column (a one-column outline); a name left out is the text to the right of the number. Default: guessed from the titles and the cells. |
| `coding` | Write the numbers in this pattern (widths 1 to 6, separator one of `-` `.` space `/` `:` `;` `,` or `""`). Default: the pattern the numbers follow. |
| `sub_accounts` | Keep sub-accounts as accounts of their own (else left out). The `kind,code,name,parent` layout always keeps them. |
| `default_levels` | Add our production totals by category number when the file names none (default true). |

### `method-budget coa list`

Lists the bundled chart-of-accounts templates by group, with their ids.

### `method-budget explain <file.mthd> <total> [--depth N] [--exclude GROUP]...`

Prints how one total is worked out, as an indented tree whose leaves add up to it exactly
(roadmap 3.5, decision 43; the app's Explain tab shows the same steps). Each step is
`- label: the working in words  = amount`; a step in brackets, `[amount]`, only explains
and adds nothing; `less` marks a step taken off. `--depth N` prints N levels (`...` marks
what is left out). The line under the title states the budget's arithmetic and rounding
rule (decisions.md, entry 55), and each Rounding step says how its amount rounded: half
away from zero (exact), or half up on the binary double, with what the double holds
when that decides it (`1.005 is 1.00499999999999989341... as a binary double, so it
rounds to 1.00`). `<total>` is one of:

| Total | Meaning |
|-------|---------|
| `21-01`, `21-01/lines`, `21-01/fringes` | An account's total, its lines, or the fringes its lines earn (the number as typed or as the budget's coding writes it; an account number wins over a category number) |
| `21`, `21/fringes` (or `category:21`) | A category's total, or the fringes earned in it |
| `21-01:3`, `#ID`, `line:ID` | Line 3 of an account, or a row by id (a subtotal row explains its running total) |
| `level:NAME` or `level:N` | A production level |
| `charge:NAME`, `credit:NAME` (or `:N`, counting applied ones from 1) | One charge or credit |
| `fringe:NAME` | One fringe's total across the budget |
| `atl`, `btl`, `atl+btl`, `fringes`, `credits`, `grand` (or `topsheet`), `net` | The summary rows |

A total that isn't in the budget is a usage error (exit 2).

### `method-budget diff <old.mthd> <new.mthd> [--json] [--exit-code]`

What changed from the first budget to the second (roadmap 3.1; docs/MERGE.md): every
change in plain words (`21-01 line 3: rate 1,200.00 -> 1,350.00 (+1,500.00 to 21-01)`),
grouped under the budget's settings, each setup table, the topsheet and account by
account, then the topsheet, category and account totals that moved (before, after,
change). Rows match by id, accounts and categories by number (a renumbered one is found by
what it holds). `--json` prints one object, money in integer cents. Exit 0; with
`--exit-code`, 1 when anything changed.

### `method-budget merge <base.mthd> <ours.mthd> <theirs.mthd> -o <out.mthd> [--ours|--theirs]`

Three-way merge (roadmap 3.2; docs/MERGE.md): both budgets' changes since their common
base. Different rows, fields and setup items merge on their own; the same field changed
both ways, or something deleted on one side and changed on the other, is a conflict,
settled for ours (or `--theirs`) and listed with the base, ours and theirs values. The
merged budget is written either way, never over one of the three budgets (that is
`merge-driver`'s job). **Exit 1 when conflicts were settled** that way,
0 when there were none.

### `method-budget merge-driver <base> <ours> <theirs> [<path>] [--theirs]`

`method-budget merge` for git, writing the result over `<ours>`; exit 1 when conflicts remain, so
git marks the file conflicted (it stays a valid budget, conflicts settled for ours). Set
up with `*.mthd merge=method-budget` in `.gitattributes`
and `git config merge.method-budget.driver "method-budget merge-driver %O %A %B %P"` (docs/MERGE.md,
"With git").

### Setup packs: `method-budget pack export`, `import` and `show`

A setup pack is a budget file with setup items and no lines, stamped with the pack's
details (docs/FORMAT.md, "Setup packs"; docs/APP.md, "Setup packs").

`method-budget pack export <budget.mthd> -o <pack.mthd> [--kinds K,K] [--items NAME,NAME] [--item NAME] [--title T] [--description D] [--author A] [--pack-version V] [--effective YYYY-MM-DD] [--region R] [--notes N] [--url URL]... [--created DATE]`
writes a pack of the budget's setup items of `--kinds` (`globals`, `fringes`, `groups`,
`units`, `sets`, `locations`, `currencies`, `charges`, `credits`, `tag-presets`,
`line-blocks`, `report-presets`, `ot-patterns`; every kind when left out), or only the
named `--items` (split on commas; `--item` takes a name whole, for names with commas;
`kind:Name` says which kind), with everything they need: a fringe's globals and unit, a
global's globals, a charge's groups, a credit's groups and fringes, a tag preset's tags.
The budget settings never go in a pack. The title defaults to the budget's; `--created`
defaults to today (UTC). The details are checked (a date `YYYY-MM-DD`, links `http://` or
`https://`); a refusal is exit 3. The pack is written atomically, never over the budget.

`method-budget pack import <pack.mthd> <budget.mthd> [--ours | --theirs] [--kinds K,K] [--items NAME,NAME] [--item NAME] [--dry-run]`
imports the pack's items (or the picked ones, with what they need) into the budget and
saves it atomically: items the budget lacks are added; where it has an item of the same
name set up differently it keeps its own (`--ours`, the default) or takes the pack's
(`--theirs`). Every item that comes in is stamped with the pack's title, version and
effective date (`setup_sources`). Any budget file can be imported from (nothing is
stamped then). `--dry-run` lists each item as add, same, keep (ours) or replace (theirs)
and writes nothing.

`method-budget pack show <pack.mthd>` prints the pack's details, its items with their
definitions, and where each came from.

```sh
method-budget pack export show.mthd -o "Crew Rates.mthd" --kinds fringes,globals \
  --title "Crew Rates" --pack-version 2026.1 --effective 2026-01-01 --region "Example"
method-budget pack import "Crew Rates.mthd" new-show.mthd --dry-run
method-budget pack import "Crew Rates.mthd" new-show.mthd --theirs
```

### `method-budget mcp [--root DIR]... [--read-only]`

A Model Context Protocol server on stdin and stdout, so coding agents (Claude Code and
other MCP clients) can open, read, edit, check, diff, merge, export and import budgets with
typed tools instead of shell commands. Every path must be inside a `--root` folder
(repeatable; default: the current folder); `--read-only` hides every tool that writes.
Changes are written atomically, refused when the file changed on disk since the server read
it, and undoable within the session. It runs until stdin closes; logs go to stderr. Register
it with `claude mcp add method -- method-budget mcp --root ~/Budgets`. The tools, the safety model
and the limits are in docs/MCP.md.

### `method-budget log <file.mthd> [--init] [--last N] [--json]`

The budget's change log, the folder `<file.mthd>-log/` beside it (docs/FORMAT.md, "The
change log"; decisions.md, entry 62): the budget id, every actor (computer) with how many
changes it made and its last one, changes waiting for ones that haven't arrived or
refused, what to review after a merge (a value two people changed at once, a deleted item
still in use, two items with one name...), and the last N changes (10) with their actor,
time, label and operations. `--init` starts the log: the file as it is becomes the first
change, by this computer. `--json` prints one JSON object. This computer's actor id lives
in `~/.method/actor` (or `$METHOD_ACTOR_FILE`), made on first use.

### `method-budget sync <file.mthd|FOLDER> [--no-write] [--roll]`

Merges a shared budget: reads every actor's segments, records an outside edit to the
`.mthd` (a save by a program that doesn't write the log, such as `method-budget set`) as a change
by this computer, writes this computer's unwritten changes as a sealed segment, and writes
the merged budget back as the `.mthd`, which is the keeper's job (collaboration design,
4.3). With a folder, every `.mthd` in it that has a log beside it. `--no-write` merges and
reports without rewriting the `.mthd`; `--roll` joins this computer's segment files into
one. Prints what it did and what to review; exit 1 when a budget could not be merged.

## Edit scripts

`method-budget apply` reads one command per line (UTF-8; a byte order mark at the start is
ignored). Blank lines and lines starting with `#` are skipped. Words are separated by spaces; put a value with spaces in double quotes, with
`\"` for a quote and `\\` for a backslash. A command with too many words is refused
rather than guessed at.

```text
# Week 3 changes
set title "Feature, draft 3"
set compact_versions true
set global SHOOT_DAYS 32
set global WEEKS "SHOOT_DAYS / 5"
set fringe FICA rate 7.65
set fringe FICA cutoff 168,600.00
set fringe MEALS basis amount_counts
set fringe PENSION_HR max_hours_per_day 12
set line 21-01:2 rate 900
set line #17 description "Director of photography"
set line 21-01:* location "Stage 4"
add tag 21-01:* fringe FICA
remove tag #17 group Night
rename location "Stage 4" "Stage 5"
apply charge Contingency
remove charge Insurance
```

| Command | Does |
|---------|------|
| `set title TEXT` | The budget's title. |
| `set compact_versions true\|false` | Save versions as deltas. |
| `set arithmetic binary\|exact` | How totals are worked out (see `arithmetic` above). |
| `set round_account_totals true\|false` | Account totals in whole units (see `round_account_totals` above). |
| `set trailing_level_fringes keep\|drop` | Fringes after the last production total (see `trailing_level_fringes` above). |
| `set cutoff_factor 1.0\|1.5\|2.0\|2.5\|3.0` | What every percent fringe's cutoff is multiplied by (see `cutoff_factor` above). |
| `set fringe_row account\|description\|position VALUE` | One field of the budget's fringe row, `none` to clear it (see `fringe_row.FIELD` above). |
| `set group_rules RULE include\|exclude` | One group rule: `ungrouped_lines`, `conflict` or `fringes_in_totals` (see `group_rules.RULE` above). |
| `set global NAME FORMULA` | Sets a global's formula, adding the global when it's new. |
| `set fringe NAME rate FORMULA` | A fringe's rate. |
| `set fringe NAME cutoff AMOUNT\|none` | A percent fringe's cutoff per earner; whole cents only. A flat fringe has none to set (`none` clears one it kept). |
| `set fringe NAME basis amount\|amount_counts\|quantity` | What a flat fringe counts on a line (see `fringe.NAME.basis` above). |
| `set fringe NAME max_hours_per_day HOURS\|none` | A flat fringe's most hours a day (above 0), for one charged per hour. |
| `set line ROW FIELD VALUE` | A field of one or more rows. |
| `add tag ROW fringe\|group NAME` | Puts a fringe or group on rows. The fringe or group must exist. |
| `remove tag ROW fringe\|group NAME` | Takes it off. |
| `rename KIND OLD NEW` | Renames a global, fringe, group, unit, set, location or currency and every reference to it, exactly as the app's rename does (decision 32). |
| `apply charge NAME` | Makes a contractual charge count. |
| `remove charge NAME` (or `unapply`) | Makes it stop counting. The charge stays in the budget, so `apply charge` brings it back. |

**Rows.** `#17` is the row with id 17 (ids are stable: docs/FORMAT.md). `21-01:3` is the
third row of account 21-01, counting every row (subtotals and separators too), as the
grid numbers them. `21-01:*` is every line and adjustment row of the account.

**Line fields.** `description`; the formulas `amount`, `x`, `rate`, `x4`; the units
`unit`, `unit2`, `unit3`, `unit4` (a unit in the budget, or `""`); `currency` (a currency
in the budget or the base currency), `set`, `location` (each must exist, or `none`);
`fringe_range` (a number or `none`); `bold`, `italic` (`true`/`false`); `indent` (0 to
4); `color` (`#rrggbb` or `none`).

Each command acts on the budget as the commands before it left it, so a later line can
use a global an earlier one added. Formulas aren't checked line by line: the whole
result is recalculated at the end, and any problem (an unknown global, a fringe range
that doesn't work) refuses the script.

## Examples

```sh
# Nightly: check every budget in a folder and keep a PDF and a workbook of each.
for f in budgets/*.mthd; do
  method-budget check "$f" || exit 1
  method-budget report "$f" --preset "Studio pack" -o "out/$(basename "$f" .mthd).pdf"
  method-budget export "$f" --xlsx "out/$(basename "$f" .mthd).xlsx"
done

# Try a rate change, then make it.
method-budget set film.mthd line.21-01:2.rate 950 --dry-run
method-budget set film.mthd line.21-01:2.rate 950

# A department's sheet into a copy of the budget.
method-budget import camera.xlsx --mapping camera.json --into film.mthd -o film-with-camera.mthd
```
