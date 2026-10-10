# The `fwb` command line

`fwb` reads and writes Film Wizard Budgeting files (`.fwb`, docs/FORMAT.md) without the
app: check a budget, print its totals, draw a report, export a spreadsheet, import one,
and make scripted edits. It is plain Rust with no network and no GUI, so it runs the same
on a laptop, a server or in CI. Build it with `cargo build --release -p fwb-cli`; the
binary is `target/release/fwb`.

Every command takes the budget file first, then flags. Flags take their value as the next
word or after `=` (`--out=x.pdf`); `-o` is short for `--out`; `--` ends the flags. A word
that looks like a negative number (`-5`) is a value, not a flag. `fwb help` lists every
command.

Commands that write a file write it **atomically**: to a temporary file in the same
folder, then renamed over the target, so a reader never sees half a file and a failed
write leaves the old file as it was. No command ever writes over the budget it is reading
from unless that is its job (`fmt`, `set`, `apply`).

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | It worked. |
| 1 | Something failed: a file couldn't be read, parsed or written, the budget has a calculation problem, or an output already exists. |
| 2 | The command line is wrong: unknown command or flag, a missing argument or value. The message ends with the command's usage. |
| 3 | An edit was refused (`set`, `apply`): a bad script line, a name that doesn't exist, or a result that doesn't recalculate. **Nothing was written.** |

Commands added later may define further codes in their own sections. Errors go to
stderr as `fwb: <message>`; results go to stdout.

## Commands

### `fwb show <file.fwb> [--exclude GROUP]...`

Prints the topsheet for a person to read. `--exclude` leaves a group's lines out
(repeatable).

### `fwb check <file.fwb>`

Reads and recalculates the budget. Prints `<file>: ok, net total 1,234.56`, or fails
(exit 1) with the reader's or the engine's message, such as
`line 40, column 21 (budget.topsheet[2].accounts[0].details[3].rate): ...`.

### `fwb fmt <file.fwb>`

Rewrites the file in canonical form (key order, decimals, indentation), the bytes the
app would save.

### `fwb new <file.fwb>` and `fwb sample <file.fwb>`

Write a starter budget, or a large invented test budget (5,000 rows). Both refuse to
replace an existing file.

### `fwb totals <file.fwb> [--json] [--exclude GROUP]...`

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
and `net_total_in_totals_currency` (`null` if it can't be converted).

```sh
fwb totals film.fwb | awk -F'\t' '$1 == "net_total" { print $2 }'
```

### `fwb export <file.fwb> --xlsx|--csv|--tsv <path> [--bom]`

Also `--format xlsx|csv|tsv --out <path>`. The evaluated budget as an Excel workbook
(Topsheet, Accounts, Detail and the setup tables), as one CSV file per table (named
`<stem> - <Table>.csv` beside the path), or the Detail table as tab-delimited text for
accounting tools (decision 30). A folder as the path names the file after the budget.
`--bom` starts CSV and TSV with a UTF-8 byte order mark, which Excel needs for accented
letters.

### `fwb report <file.fwb> [--preset NAME] [--sections LIST] [--format F] [-o PATH] [--date TEXT] [--bom]`

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
- `fwb report <file.fwb> --list-presets` lists the presets, the default marked `*`.

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

### `fwb set <file.fwb> <path> <value> [--dry-run]`

Changes one value and saves. Paths:

| Path | Value |
|------|-------|
| `title` | Text. |
| `compact_versions` | `true` or `false`: save versions as deltas (docs/FORMAT.md, "Version deltas"). |
| `global.NAME` | A formula. Adds the global when it's new. |
| `fringe.NAME.rate` | A formula. |
| `fringe.NAME.cutoff` | An amount (`168,600.00`), or `none`. Percent fringes only. |
| `line.ROW.FIELD` | See `set line` below for rows and fields. |
| `charge.NAME.applied` | `true` or `false`. |

A name or account code may contain dots: the path splits at its first and last dots.
`none` (or an empty `""`) clears an optional value. The command prints each change and
the totals that moved; `--dry-run` prints the changes and a full `fwb diff`-style
summary and writes nothing. A refused edit exits 3 and writes nothing.

```sh
fwb set film.fwb line.21-01:2.rate 900
fwb set film.fwb fringe.FICA.cutoff 168,600.00 --dry-run
```

### `fwb apply <file.fwb> <script.txt> [--dry-run]`

Runs an edit script (below) and saves. `-` reads the script from stdin. Every line must
succeed and the result must recalculate, or nothing is written and the command exits 3
naming the line (`edit.txt line 4: there is no fringe FICA`). The output lists every
change, then the totals before and after; `--dry-run` shows the changes and a full
diff summary and writes nothing.

### `fwb import <sheet> -o <out.fwb> [--mapping M.json] [--into B.fwb] [--title T] [--currency C] [--force] [--dry-run]`

Adds a sheet's rows to a budget as detail lines, with the app's own importer (decision
36): `.xlsx` workbooks, CSV and tab-delimited text, with the same limits, account
matching, reconciliation and cautious defaults.

- `--into B.fwb` imports into a copy of that budget, whose chart and setup give the lines
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

### `fwb explain <file.fwb> <total> [--depth N] [--exclude GROUP]...`

Prints how one total is worked out, as an indented tree whose leaves add up to it exactly
(roadmap 3.5, decision 43; the app's Explain tab shows the same steps). Each step is
`- label: the working in words  = amount`; a step in brackets, `[amount]`, only explains
and adds nothing; `less` marks a step taken off. `--depth N` prints N levels (`...` marks
what is left out). `<total>` is one of:

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

### `fwb diff <old.fwb> <new.fwb> [--json] [--exit-code]`

What changed from the first budget to the second (roadmap 3.1; docs/MERGE.md): every
change in plain words (`21-01 line 3: rate 1,200.00 -> 1,350.00 (+1,500.00 to 21-01)`),
grouped under the budget's settings, each setup table, the topsheet and account by
account, then the topsheet, category and account totals that moved (before, after,
change). Rows match by id, accounts and categories by number (a renumbered one is found by
what it holds). `--json` prints one object, money in integer cents. Exit 0; with
`--exit-code`, 1 when anything changed.

### `fwb merge <base.fwb> <ours.fwb> <theirs.fwb> -o <out.fwb> [--ours|--theirs]`

Three-way merge (roadmap 3.2; docs/MERGE.md): both budgets' changes since their common
base. Different rows, fields and setup items merge on their own; the same field changed
both ways, or something deleted on one side and changed on the other, is a conflict,
settled for ours (or `--theirs`) and listed with the base, ours and theirs values. The
merged budget is written either way. **Exit 1 when conflicts were settled** that way,
0 when there were none.

### `fwb merge-driver <base> <ours> <theirs> [<path>] [--theirs]`

`fwb merge` for git, writing the result over `<ours>`; exit 1 when conflicts remain, so
git marks the file conflicted (it stays a valid budget, conflicts settled for ours). Set
up with `*.fwb merge=fwb` in `.gitattributes` and `git config merge.fwb.driver "fwb
merge-driver %O %A %B %P"` (docs/MERGE.md, "With git").

## Edit scripts

`fwb apply` reads one command per line. Blank lines and lines starting with `#` are
skipped. Words are separated by spaces; put a value with spaces in double quotes, with
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
| `set global NAME FORMULA` | Sets a global's formula, adding the global when it's new. |
| `set fringe NAME rate FORMULA` | A fringe's rate. |
| `set fringe NAME cutoff AMOUNT\|none` | A percent fringe's cutoff per earner; whole cents only. |
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
for f in budgets/*.fwb; do
  fwb check "$f" || exit 1
  fwb report "$f" --preset "Studio pack" -o "out/$(basename "$f" .fwb).pdf"
  fwb export "$f" --xlsx "out/$(basename "$f" .fwb).xlsx"
done

# Try a rate change, then make it.
fwb set film.fwb line.21-01:2.rate 950 --dry-run
fwb set film.fwb line.21-01:2.rate 950

# A department's sheet into a copy of the budget.
fwb import camera.xlsx --mapping camera.json --into film.fwb -o film-with-camera.fwb
```
