<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="site/assets/brand/wordmark-dark.svg">
    <img src="site/assets/brand/wordmark.svg" alt="Method Budget" width="380">
  </picture>
</p>

<p align="center"><strong>Film and TV budgeting that shows its working.</strong></p>

<p align="center">
  <a href="https://methodbudget.com">methodbudget.com</a> ·
  <a href="docs/INSTALL.md">Install</a> ·
  <a href="docs/CLI.md">Command line</a> ·
  <a href="docs/FORMAT.md">File format</a> ·
  <a href="docs/ROADMAP.md">Roadmap</a>
</p>

Method Budget is a free, open-source desktop app for building and running production
budgets: topsheet, accounts and detail lines, fringes, globals, groups, charges and
credits, versions and cost reports. It keeps your budgets as plain, documented files on
your own disk, works offline, and never needs an account. No subscription, and your budget
is a file you own.

<!-- Pictures: retaken by `cargo xtask capture-screens` from the invented Field Station
     budget (fixtures/field-station/; docs/WEBSITE.md, "Screenshots"). Never a real
     budget, never another product's screens. -->
<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/screens/hero-dark.png">
    <img src="assets/screens/hero-light.png" width="92%" alt="The topsheet of Field Station, an invented four-day independent feature: category numbers, names and totals, a grand total of 393,018.91, and the tools panel showing the fringes on the Cast category and the budget's two locations.">
  </picture>
</p>
<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/screens/explain-dark.png">
    <img src="assets/screens/explain-light.png" width="92%" alt="Explain this number on the fringes of one cast line, DELACROIX, 4 days at 2,400: each fringe with its rate and cutoff, the state unemployment fringe stopping at its 7,000.00 cutoff and coming to 238.00.">
  </picture>
</p>
<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/screens/import-dark.png">
    <img src="assets/screens/import-light.png" width="92%" alt="Import Spreadsheet reviewing an invented camera department rental list: the import options, the eight lines as they will import, and the check that every line comes to the sheet's own total of 5,310.00.">
  </picture>
</p>
<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/screens/file-dark.png">
    <img src="assets/screens/file-light.png" width="92%" alt="The budget is a file you own: the app beside a terminal running the method-budget totals command on field-station.mthd, printing each category's total and the grand total of 393018.91.">
  </picture>
</p>
<p align="center"><sub>Field Station is an invented production: its title, its people and every figure are made up.</sub></p>

## Highlights

- **Every number explains itself.** Pick any total and trace it down to the lines,
  fringes, cutoffs and rounding steps that make it up. Money is kept in whole cents, so
  the topsheet always adds up.
- **Bring your budgets with you.** Open the budget exports other budgeting tools write
  (JSON, XML, Excel or tab-delimited), department spreadsheets and accounting exports.
  Each import is checked figure by figure against the totals in the file. See
  [Importing](#importing).
- **Your budget is an open file.** Budgets are `.mthd` files: plain, documented JSON
  ([docs/FORMAT.md](docs/FORMAT.md)). Any tool can read them, they diff cleanly, and you
  are never locked in.
- **Keyboard first.** Drill into an account, climb back to the topsheet, step between
  accounts, fill down and move rows without reaching for the mouse.
- **Scriptable.** The `method-budget` command line runs the same engine with no window,
  and a git merge driver combines two people's edits to one budget.

## Yours to keep

- **Free, for good.** Open source under MIT or Apache-2.0. No subscription, no account.
- **Your file, on your disk.** Budgets are `.mthd` files with a published
  [specification](docs/FORMAT.md), so you can open them in twenty years, whatever happens
  to any vendor or to this project.
- **Works offline.** Nothing is uploaded. The app goes online only to check for its own
  updates, and you can turn that off.
- **Nothing invents your numbers.** There is no generative AI in the app: every figure
  comes from what you typed and the rules you set.

## Status, honestly

Alpha, ready for testing. It's tested against thousands of figures, but you will find
rough edges: keep a copy of budgets that matter, and check numbers before they leave the
room.

| Platform | Built | Signed | Tested by hand | Updates itself |
|---|---|---|---|---|
| macOS, Apple silicon | Yes | Yes, notarized by Apple | Yes | Yes |
| macOS, Intel | Up to 0.2; later releases are Apple silicon only | Yes | Not checked | Stays on its version |
| Windows (x64) | Yes | Not yet | Not yet | Not yet |
| Linux (x64, `.deb` and AppImage) | Yes | Not applicable | Not yet | Not yet |

Not there yet: tested Windows and Linux builds, updates in place on those two, and starter
templates with charts of accounts (being written now). The [roadmap](docs/ROADMAP.md) has
the rest. Windows and Linux: help us get there.

## Features

The list below is generated from [docs/features.toml](docs/features.toml) by
`python3 scripts/site.py`, which also writes the list on the website. Edit that file, not
this section.

<!-- features:start (generated by scripts/site.py from docs/features.toml: edit that, then run python3 scripts/site.py) -->
### Budgeting engine

- **Topsheet, accounts and detail lines.** The familiar three levels, with every total rolling up live as you type.
- **Money to the cent.** Every amount is kept in whole cents and rounds the way production budgets have always rounded, with an exact mode if you want one.
- **Formulas and globals.** Amount, quantity and rate cells take formulas and named globals; change a global and every line that uses it follows.
- **Currencies.** Any line can be in its own currency, converted to the budget's base currency at rates you set.
- **Groups and what-ifs.** Tag lines into groups, leave a group out of the total to see a what-if, and get back to the exact cent when you put it back.
- **Explain this number.** Trace any total down to the lines, fringes, cutoffs, ranges and rounding steps that make it up.
- **Overtime helper.** Work out a day or a week of straight time, overtime bands and sixth and seventh days, then write the pay into the line.
- **Fast on big budgets.** An edit recalculates in about 5 ms on a 5,000-line budget.

### Fringes, globals, charges and credits

- **Fringes with cutoffs and ranges.** Percent and flat fringes, cutoffs per earner, fringe ranges across several lines, unit-hour flats and four ways to post them.
- **Globals with details.** Give a global a description, a unit and decimals, show it by name in cells, and see the formulas behind a selection.
- **Contractual charges.** Percent and flat charges with exclusions, a live preview while you type, and the percent shown on the topsheet if you want it.
- **Credits.** Credit rows show gross, excluded fringes, net and how far you are from a cap.
- **Apply panel.** Select lines, then tick fringes, groups, sets and locations on or off, with live totals for the selection.
- **In-use counts.** Every setup list shows how many lines use each item and what they cost, warns before you delete one in use, and finds them in a click.

### Editing, split view and history

- **Keyboard-first grid.** Drill in, climb out, step between accounts, fill down and move rows without touching the mouse; a shortcut sheet lists every key.
- **Type-ahead and add on the fly.** Units, currencies, sets, locations and globals complete as you type, and a new one can be added right from the cell.
- **Never lose work.** Autosave into the file, recovery copies for untitled budgets, rolling backups and a restore after a crash.
- **Budget tabs and split view.** Open several budgets as tabs, each with its own undo, and put two side by side in one window.
- **History list.** Every undo step is listed by name and time; click one to go back to it, or save it as a version.
- **Find and replace.** Search descriptions, account numbers, fringes, globals, groups, notes and formulas, then replace only in the results you tick, as one undo step.
- **Copy rows with everything on them.** Rows keep their fringes, groups, notes, style and ranges when copied, even between budgets, with a review of what the paste will change.
- **Notes.** Dated notes on lines, accounts and categories, and a notes list that works as a review checklist.
- **Signed and notarized Mac app.** The Mac app is signed and notarized by Apple, so it opens like any other app with no extra security step.

### Versions, compare, diff and merge

- **Named versions.** Save, rename, annotate and restore versions, all kept inside the budget file.
- **Compare up to 16 budgets.** Versions or other files side by side at topsheet and account level, with sum, average and variance columns, down to detail lines.
- **Original and variance columns.** Keep a locked baseline and see each account's change from it right in the grid.
- **What changed.** A plain-words diff of two budgets or versions: every changed line, field and setup item, with the money it moved.
- **Three-way merge.** Two copies that changed apart merge on their own where they touched different things; real clashes are listed for you to pick.
- **Share-ready copies.** Save a dated snapshot, optionally without notes or private info, that opens read-only and compares back against the original.
- **Small version history.** Versions can be stored as compact patches, so ten versions of a large budget take about the space of one.

### Actuals and cost reports

- **Import actuals.** Bring in transactions from CSV, tab-delimited or Excel accounting exports with a column mapping, duplicate checks and a review of unknown accounts.
- **Cost report.** Budget, actual to date, estimate to complete and variance by account, with each account's estimate method your choice.

### Reports, PDF and spreadsheets

- **Reports saved in the budget.** Pick sections, accounts and layout, save them as named presets with header and footer fields, and print straight to PDF.
- **Live preview.** The setup dialog shows the printed pages as you change them, with page navigation and a jump list of sections.
- **More than the topsheet.** Category summaries, account detail, setup tables, chart of accounts, group totals, budget information and a notes list.
- **Spreadsheet export.** One Excel workbook with topsheet, accounts, detail and setup tables, plus CSV and tab-delimited files for accounting.
- **Spreadsheet import.** Turn any department's Excel, CSV or tab-delimited sheet into budget lines with a column mapping and a line-by-line check of the totals.
- **Import from other budgeting apps.** Open a budget from another program's JSON, XML, Excel or tab-delimited export, with a line-by-line check against that program's own totals.

### Setup library and templates

- **Start from another budget.** Import setup items from any budget, with what they depend on, or start a new budget from a template.
- **Personal setup library.** Keep your fringes, globals, groups and the rest in a library file of your own and copy them in and out with a conflict review.
- **Tag presets and line blocks.** Save a set of tags or a block of lines once and apply or insert them anywhere.
- **Rename everywhere.** Rename a fringe, global, group or unit and every reference follows, formulas included, as one undo step.
- **Renumber accounts.** Change the account numbering pattern and preview every old and new number before it applies.
- **Sub-budgets.** Make a filtered copy by categories, accounts, groups, sets or locations, for a department or a unit.

### Command line and git

- **Open file format.** Budgets are `.mthd` files: plain, documented JSON that diffs cleanly and that any tool can read.
- **Scriptable command line.** `method-budget` checks budgets, prints totals, draws reports as PDF, HTML or CSV, and exports and imports spreadsheets, with no window and no network.
- **Batch edits.** `method-budget set` and `method-budget apply` run edit scripts all or nothing, with a dry run that shows what would change.
- **Git merge driver.** Keep budgets in git and let `method-budget merge-driver` merge edits to different lines, flagging only real conflicts.

### On the roadmap

- **Windows and Linux, tested.** Installers for both are built today; printing, keys and file types still need a hands-on check.
- **Review files.** A reviewer marks up a read-only copy, and their comments come back in a small file you merge.
- **Incentive modeling.** Qualified spend by location, several programs side by side, caps and what-ifs, from rules you enter yourself.
- **Schedule-linked quantities.** Read day counts from a schedule's export and let them drive your globals.
- **Accessibility and translations.** A full accessibility pass and the app in more languages.
- **Work on a budget together.** Several people in one budget at once, through a shared Dropbox or iCloud folder or a live link, with who changed what and undo that only takes back your own edits.
- **Rate card lookup.** Pick a union classification and fill in the rate from public rate cards, with the source and effective date shown.
- **New project window.** Start a budget from one window with its title, chart of accounts, schedule, currency and rate setup, ready to fill in.

<!-- features:end -->

## Install

Method Budget is a desktop app for macOS on Apple silicon, with Windows and Linux builds
that have not been tested by hand yet. Download the Mac app from the
[latest release](https://github.com/method-production-suite/method-budget/releases/latest),
or build it from source. [docs/INSTALL.md](docs/INSTALL.md) covers building, installing on a
Mac, the one-time security step for builds that are not notarized, and what signed releases
need.

### Build from source

Get the code:

<!-- clone:start (generated by scripts/site.py from docs/features.toml: edit that, then run python3 scripts/site.py) -->
```bash
git clone https://github.com/method-production-suite/method-budget.git
```
<!-- clone:end -->

You need [Rust](https://rustup.rs) (the version is pinned in `rust-toolchain.toml`, and
rustup picks it up on its own). There is no JavaScript toolchain: no npm, no bundler.

```bash
cargo test --workspace            # the engine, the file format, the command line and the app
cargo run -p method-app           # run the desktop app
```

On Linux, install the WebKitGTK build packages first (listed in [docs/APP.md](docs/APP.md)).
Installers (`.app` and `.dmg`, `.msi`, `.deb` and `.AppImage`) are built with the pinned
Tauri CLI, as [docs/INSTALL.md](docs/INSTALL.md) describes.

## Command line quickstart

The `method-budget` command reads and writes budgets without the app: same engine, no window, no
network. Install it from a checkout:

```bash
cargo install --path crates/method-cli --locked
```

Then:

```bash
method-budget new my-film.mthd                      # a starter budget
method-budget show my-film.mthd                     # the topsheet
method-budget check my-film.mthd                    # recalculate and report problems
method-budget totals my-film.mthd --json            # every total, in cents, for scripts
method-budget explain my-film.mthd net              # how a total is made up
method-budget report my-film.mthd --sections topsheet -o topsheet.pdf
method-budget export my-film.mthd --xlsx my-film.xlsx
method-budget diff old.mthd new.mthd                # what changed, in plain words
```

Every command, flag and exit code is in
[docs/CLI.md](docs/CLI.md).

### Budgets in git

Budgets are text with stable ids inside, so git can merge two people's edits. Tell git
to use the merge driver:

```bash
echo '*.mthd merge=method' >> .gitattributes
git config merge.method.name "Method Budget merge"
git config merge.method.driver "method-budget merge-driver %O %A %B %P"
```

Edits to different lines merge on their own; real clashes are left for you to settle in
the app. Details: [docs/MERGE.md](docs/MERGE.md).

## Coding agents

`method-budget mcp` is an MCP server, so Claude Code and other MCP clients can open, read and edit
budgets with typed tools: lines, globals, fringes, charges, totals, explain, export, import and
undo, all through the same engine and the same exact arithmetic as the app. It only touches
files inside the folders you give it, writes atomically, won't overwrite a file someone else
just saved, and has a read-only mode. Nothing is sent anywhere.

```bash
claude mcp add method -- method-budget mcp --root ~/Budgets
```

Every tool and the safety model: [docs/MCP.md](docs/MCP.md).

## Importing

Method Budget reads the open files other programs export for their users, never their
native files ([CLEAN-ROOM.md](CLEAN-ROOM.md)).

| From | Formats | What comes across |
|---|---|---|
| Movie Magic Budgeting | JSON (best), XML Advanced, Excel, XML Basic, tab-delimited | Accounts, lines, fringes, globals, groups, charges and credits; Basic XML has no groups or globals, tab-delimited has account totals only |
| Any spreadsheet | Excel, CSV, tab-delimited | Budget lines, with a column mapping |
| Accounting systems | Excel, CSV, tab-delimited | Actuals for cost reports, with a column mapping |
| Another Method budget | `.mthd` | Fringes, globals, charges, credits and other setup |

In the app: **File > Import Budget Export…**. From the terminal:

```sh
method-budget import-export "Pilot budget.json" -o pilot.mthd
```

Every import ends with a review that compares each line, account, fringe and total with
the figures in the file. Suppressed lines are not in the exports, and charge and credit
rules arrive as the fixed amounts they came to.

Method Budget is independent and not affiliated with or endorsed by Entertainment
Partners. Movie Magic is a trademark of Entertainment Partners.

## File format

Budgets are pretty-printed JSON with a fixed key order, money as integer cents and
decimals as strings, so the same budget always produces the same bytes. The full
specification is [docs/FORMAT.md](docs/FORMAT.md). It is ours to document and yours to
use: write tools that read or produce it.

## Templates and charts of accounts

Any budget can be a template: File > New From Template starts a new budget from another
one's chart of accounts, setup and settings without its lines, and File > Save as
Template writes one. Starter templates with charts of accounts written for this project
are being added now.

## Clean room

Method Budget is an independent, clean-room project by the Method Budget contributors,
not affiliated with Entertainment Partners or Movie Magic Budgeting. Trademarks belong to
their respective owners. It uses no other vendor's code, templates, help text, icons or
data, and reads budgets from other tools only through the open files they export. The
rules every contribution follows are in [CLEAN-ROOM.md](CLEAN-ROOM.md).

## Help build it

**If you make films.** The most useful help needs no code:

- Import an old budget's export and tell us about any figure that comes out different.
- Check the fringes and rate setups for your region and union, and tell us what's wrong
  or missing.
- Contribute a chart-of-accounts template or an invented sample budget.

Open an [issue](https://github.com/method-production-suite/method-budget/issues), or use
**Help > Report a Bug…** in the app, which fills in the version for you. Please never attach
a real budget, schedule or call sheet: an invented one that shows the problem
(`method-budget sample` makes a large test budget) is always enough.

**If you write code.** Start with an issue labelled
[good first issue](https://github.com/method-production-suite/method-budget/labels/good%20first%20issue).
Before you open a pull request, run the checks CI runs:

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Read [CLEAN-ROOM.md](CLEAN-ROOM.md) first: it is short, and it is the one hard rule.
Design choices and their reasons are numbered in [docs/decisions.md](docs/decisions.md),
what comes next is in [docs/ROADMAP.md](docs/ROADMAP.md), and how the project looks and
sounds is in [docs/BRAND.md](docs/BRAND.md).

## Part of the Method Production Suite

Method Budget is one of the [Method Production Suite](https://methodproduction.com)'s free,
open-source tools for film and TV production.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Copyright (c) 2026 Method Production Suite Team and the Method Budget contributors.
Contributions are accepted under the same terms. The third-party code the app and the
command line ship, with its licences, is listed in
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) (generated by `cargo xtask notices`).
