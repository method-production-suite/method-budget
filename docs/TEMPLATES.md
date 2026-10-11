# Chart-of-accounts templates

Method Budget ships charts of accounts as templates: New From Template lists them under
**Charts of accounts** (grouped Feature, Television, Commercial, Other), and `method new
<file> --template <id>` starts from one (`method coa list` names them). A template is an
ordinary budget file holding categories, accounts and production totals, and nothing else:
no lines, fringes, globals, rates or charges.

Any chart can also be brought in by hand: **File > Import Chart of Accounts…** in the app,
or `method coa import` on the command line (docs/CLI.md), from a spreadsheet, a CSV file or
another budget. It makes a new budget with that chart, or applies the chart to the budget
in front (adds what is missing, renames by number, never removes an account with lines).

## Where things are

| Path | What it is | Committed |
|---|---|---|
| `templates/charts/charts.json` | The list: id, name, group, one-line description, source file | yes |
| `templates/charts/<id>.mthd` | Each template, built from its source | yes |
| `templates/charts/src/*.csv` | Our own charts, written by us | yes |
| `templates/charts/SOURCES.md` | Where each chart came from, when, what was kept | yes |
| `imports/coa-sources/` | Raw studio chart files used as sources | **never** (in .gitignore) |
| `scripts/build-coa-templates.py` | Builds every template from its source and updates the list | yes |
| `crates/fwb-format/src/charts_bundled.rs` | The list the app and the command line bundle, written by the script | yes |

## Adding a studio chart

1. Get the chart as a CSV file in the layout below (from the studio's published chart, or
   from an open-format export of a template you are licensed to use: export it to Excel or
   CSV, then arrange the columns as below; keep only numbers, names and totals).
2. Save it as `imports/coa-sources/<Template Name>.csv`, for example
   `imports/coa-sources/Studio Features.csv`. The file name becomes the template's name.
3. Run the build script from the repository's folder:

   ```sh
   python3 scripts/build-coa-templates.py
   ```

   It adds the new file to `templates/charts/charts.json` (id from the file name, group
   guessed from the name: "TV", "Series", "Episodic" go under Television, "Commercial" or
   "Music video" under Commercial, "Feature", "Film" or "Movie" under Feature, anything
   else under Other), builds `templates/charts/<id>.mthd`, prints what it read and anything
   to check (duplicates, accounts without a category, rows it skipped), and rewrites the
   bundled list.
4. Check the new entry in `charts.json`: its name, its group and its description. A studio
   chart is described as following that studio's chart ("Follows the Studio Features chart
   of accounts"), never with a logo or anything more.
5. Add a line for it to `templates/charts/SOURCES.md`: where the chart came from, the date,
   and that only numbers, names and totals were kept.
6. Rebuild the app (`cargo build`); the template shows in New From Template.

The raw file stays in `imports/coa-sources/` on your machine and is never committed. On a
machine without it, the script skips that source and keeps the committed template.

Excel workbooks (`.xlsx`) and other budget files (`.mthd`) also work as sources, read the
same way as File > Import Chart of Accounts reads them; the CSV layout below is the one
that needs no guessing.

## The CSV layout

UTF-8, comma separated, a header row, normal CSV quoting (quote any field with a comma).
Columns `kind,code,name,parent`, in any order; an extra `notes` column (or any other) is
ignored.

| Column | What goes in it |
|---|---|
| `kind` | `category`, `account`, `subaccount` or `total` |
| `code` | the number exactly as the chart shows it, leading zeros and separators kept (`1100`, `11-01`, `1101.05`); blank for a total |
| `name` | the description |
| `parent` | for an account, its category's code; for a sub-account, its account's code; blank for categories and totals |

Rows go in chart order. A production total (`TOTAL ABOVE-THE-LINE`) goes where it sits on
the topsheet, right after the categories it totals.

```csv
kind,code,name,parent
category,1100,STORY & RIGHTS,
account,1101,Rights Purchased,1100
account,1102,Writers,1100
subaccount,1102.01,First Draft,1102
category,1200,PRODUCER & STAFF,
account,1201,Executive Producer,1200
total,,TOTAL ABOVE-THE-LINE,
category,2000,PRODUCTION STAFF,
account,2001,Unit Production Manager,2000
total,,TOTAL PRODUCTION,
```

How it is read:

- **Every number is kept exactly as written**, categories and accounts alike: mixed
  widths (`0501` with `050101` and `1101`), letters (`1A00`, `39AA`), trailing dots
  (`4AA.`), separators and spaces (`5600-100`). Nothing is guessed from the numbers.
- **Each account goes in the category its `parent` names.** Without a `parent` column,
  an account goes in the category above it. An account naming a category that isn't in
  the chart, or with no category above it, is skipped and listed.
- **The numbering pattern is worked out for typing new numbers**: the one that writes the
  most account numbers exactly as they are, with the category part their category names
  (`1101` under `1100` is 2 + 2). It is shown in the import and used when you type a new
  number; it never changes or refuses a number the chart has, and anything that matches
  numbers (actuals, comparison, imports) reads a number through it only when nothing
  would be cut off.
- **Sub-accounts become accounts.** A budget has categories and accounts, no third level,
  so a sub-account is kept as an account of its own right after its account, its number
  as written (`1102.01`). The import says how many it folded in.
- **Totals.** Each total becomes a production total, its name tidied (`TOTAL
  ABOVE-THE-LINE` as `Total above-the-line`). The first production total marks the end of
  above-the-line. A total that comes straight after another total (`TOTAL BELOW-THE-LINE`
  after `TOTAL OTHER`) adds up totals rather than categories; the topsheet works out above-
  and below-the-line itself, so it is left out and said so.
- **Categories with no accounts** (section headers, departments with no default accounts)
  are kept, and listed to check. They total 0.
- **Accounts named `.`** are unused slots in the chart: left out and counted. An account
  with no name comes in with no name, listed to check.
- **Repeated account numbers** (see below): the same number with the same name is merged
  into one; the same number with another name comes in under a new number.
- A chart that names no totals gets our default production totals by category number
  (`1` above the line, `2` and `3` production, `4` post, the rest other).

## Repeated account numbers

Some charts list an account number twice. With the same name (ignoring capitals and
spacing) it is the same account, so the two are merged into one and the import says so.
With another name (`10950 RENTALS`, `10950 REHEARSAL HALL RENTALS`) it is another account,
and it comes in under a new number: the chart's number, the chart's separator (`-` when
it has none or uses a space) and a count, the first one not used anywhere in the chart
(`10950-2`, `10950-3`; `11-01` gives `11-01-2`).

Both can't keep the same number because everything that matches accounts by number would
treat them as one: transactions coded `10950` would all land on the first, a comparison
would add the two together, and imports and lookups by number would find only the first.
The new numbers are listed in the import: under **Renumbered** (and in a sentence at the
top) in the app's preview, and as `check:` lines from `method coa import`. Rename them to
whatever the production uses; renumbering to another pattern refuses them until then,
since they don't follow the pattern. Decision 53 in docs/decisions.md has the reasoning.

## Other spreadsheet shapes

File > Import Chart of Accounts and `method coa import` read most charts as studios and
accounting systems print them, without the layout above: a flat list of accounts, an
outline of category rows and account rows (in one number column, or indented one column),
category and account columns side by side (merged category cells read as blanks below
their first row), a number and name in one cell (`1101 Story rights`). Numbers with or
without separators, numbers that lost their leading zeros in a spreadsheet (`101` for
`0101`), rows above the header, repeated header rows, total rows and level headings
(`ABOVE THE LINE`, `POST PRODUCTION`) are handled. The dialog shows the guessed columns and
pattern and lets you change them; `--mapping` does the same on the command line.
In these shapes the numbers are what tell categories from accounts, so the pattern is
worked out from them first and a category written `1100` over accounts `1101` is stored
as category `11`. Repeated account numbers are handled as above.

## Checks

- `python3 scripts/build-coa-templates.py --check` changes nothing and fails if a template
  or the bundled list is out of date (a source on this machine whose template differs).
- The build script also checks each template built from a `kind,code,name,parent` CSV
  against the CSV: the same categories and account numbers, written as in the CSV, and as
  many accounts as the CSV lists, less accounts named `.` and merged repeats. It stops on
  any difference.
- `cargo test -p fwb-format charts` checks the bundled list agrees with `charts.json`,
  that every template opens, has no lines or setup, recalculates (categories with no
  accounts total 0), compares with itself and gives a cost report with no two accounts
  (or category fringe accounts) read as the same number, and is written as it reads, and
  that every template whose source is in the repository rebuilds byte for byte.
