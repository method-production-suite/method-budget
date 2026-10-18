# `method-budget mcp`: budgets for coding agents

`method-budget mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server built
into the command line. It lets a coding agent (Claude Code, or any MCP client) open budget
files, read them, change them and check them with typed tools, the way a person pushes
buttons in the app: add lines, set a global, add a fringe, apply a charge, look at the
totals, explain a number, undo.

It works on **files**, not on the running app. Every tool names a budget by its path, reads
it from disk and, for a change, writes the whole file back atomically, exactly as `method-budget
set` does. Nothing is kept open between calls except what the server needs to notice
outside changes and to undo its own (see "Files, revisions and conflicts"). A live link to
an open app window is planned (see "Live app bridge (planned)").

The server is plain Rust inside the `method-budget` binary: no network, no other programs run, no
new dependencies. It reuses the engine, the file format and the command line's own edit
language, so a value the server writes is checked exactly as `method-budget set` checks it.

## Starting it

```sh
method-budget mcp [--root DIR]... [--read-only]
```

| Flag | Meaning |
|---|---|
| `--root DIR` | A folder the server may read and write in. Repeat it for several. Default: the folder it was started in. Paths in tool calls are relative to the first root, or absolute inside any root. |
| `--read-only` | Hide every tool that writes a file. They don't appear in `tools/list` and calling one is an unknown tool. |

The server reads JSON-RPC messages from stdin, one per line, and writes one response per
line to stdout. stdout carries protocol messages only; a line per tool call (its name,
whether it worked, how long it took, never its arguments) goes to stderr. It exits when
stdin closes.

## Registering it

### Claude Code

```sh
# Every budget under ~/Budgets, for every project (user scope).
claude mcp add --scope user method -- method-budget mcp --root ~/Budgets

# Look but don't touch.
claude mcp add --scope user method-ro -- method-budget mcp --root ~/Budgets --read-only

# Only this project's folder, shared with the team through .mcp.json.
claude mcp add --scope project method -- method-budget mcp --root .
```

Use the binary's full path (`/usr/local/bin/method-budget`, or
`target/release/method-budget` from a build) if `method-budget` isn't on the `PATH` Claude
Code starts programs with. `claude mcp list` shows whether it connected; `/mcp` in a
session lists its tools.

An example skill that teaches an agent how budgets work and how to use these tools safely
is in [docs/agents/method-budget-skill.md](agents/method-budget-skill.md). Copy it to
`~/.claude/skills/method-budget/SKILL.md` (or a project's `.claude/skills/`).

### Other MCP clients

Most clients take a JSON file in this shape (the key may be `mcpServers` or `servers`):

```json
{
  "mcpServers": {
    "method": {
      "command": "/usr/local/bin/method-budget",
      "args": ["mcp", "--root", "/Users/me/Budgets"]
    }
  }
}
```

The command line used to be called `method`. A registration that still runs `method mcp`
keeps working: the old name is an alias that says nothing for `mcp` (docs/CLI.md), so
stdout stays protocol only. Point it at `method-budget` when convenient; the alias will be
removed in a future release.

## Protocol

| | |
|---|---|
| Transport | stdio, newline-delimited JSON-RPC 2.0 (no embedded newlines, UTF-8). |
| Revisions | **2026-07-28** (per-request `_meta`, `server/discover`) and, through the `initialize` handshake, **2025-11-25**, **2025-06-18**, **2025-03-26** and **2024-11-05**. |
| Capabilities | `tools` only (`listChanged: false`): no resources, prompts, sampling or logging. |
| Methods | `initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`, `notifications/cancelled`; `server/discover` for 2026-07-28 requests. |

**Version negotiation.** A client that sends `initialize` gets its own revision back when it
is one of the four above, else 2025-11-25, and is served under that revision until it
initializes again. A request whose `params._meta` carries
`io.modelcontextprotocol/protocolVersion` is served on its own under 2026-07-28: it must also
carry `io.modelcontextprotocol/clientCapabilities` (else `-32602`), any other version gets
`-32022 Unsupported protocol version` with the supported list in `data`, and its result has
`resultType: "complete"` and the server's name and version in
`_meta["io.modelcontextprotocol/serverInfo"]`. A request with neither (and no handshake yet)
gets `-32602` saying so; `ping` always works.

**Results.** A tool result has two text blocks, a one or two sentence summary for a person and
the result as compact JSON, and the same JSON as `structuredContent` (revisions from
2025-06-18 on; older ones get the text only). Tools declare no output schema; the fields are
listed below.

**Conventions in results.** Money is always an integer number of cents in a key ending
`_cents` (`"net_total_cents": 3307810` is 33,078.10). Rates, quantities, percents and
exchange rates are exact decimals written as strings (`"7.65"`). Formulas are the text as
typed (`"SHOOT_DAYS * 2"`). Rows are named by their id (`14`), which never changes, and their
position in the account from 1 (`n`). Every result that reads a budget says
`changed_on_disk: true` when the file is no longer what the server last read (see below).

**Errors.** Problems with the message itself are JSON-RPC errors: `-32700` (not JSON, or a
line longer than 8 MB), `-32600` (not a request: no `"jsonrpc": "2.0"`, a method that isn't
a string, an id that is null or not a string or integer), `-32601` (unknown method),
`-32602` (bad params, unknown tool, a writing tool on a read-only server), `-32603` (a tool
failed unexpectedly). Everything a model can fix is a **tool error** instead: a result with
`isError: true`, the message as its text and `structuredContent: {"error": {"kind",
"message"}}`, where `kind` is one of:

| Kind | Means |
|---|---|
| `invalid_input` | The arguments don't fit the tool's schema, or a value is wrong (a unit the budget doesn't have, a color that isn't `#rrggbb`). |
| `not_found` | No such file, account, row, setup item, charge or total. |
| `refused` | The change isn't allowed as asked: an item still in use, a file that exists, a shared copy, a change that would stop the budget adding up. |
| `conflict` | The file changed on disk since the server last read it. Nothing was written. |
| `path_refused` | The path is outside the roots, hidden, a symlink to write through, or has the wrong extension. |
| `io` | The disk said no. |

Arguments are checked against the tool's `inputSchema` before it runs: unknown properties,
wrong types, values out of range and missing required ones are refused with the property's
name.

**Cancellation.** A request cancelled with `notifications/cancelled` before the server reaches
it never runs. One cancelled while it runs gets no response; if it was a change it has
already been written (undo takes it back).

## Files, revisions and conflicts

The server remembers, per file (by its real path), the bytes it last read or wrote, the
budget as it was when it opened it, and the undo steps of its own changes. It opens a file
the first time a tool names it, and `budget_open` opens it again on purpose.

- **Revision.** `budget_info` and every change return the file's `revision`: a 16 hex digit
  fingerprint of its bytes. It changes with every write.
- **Reads always show the file as it is now.** When someone else saved it since the server
  last read it, the read says `changed_on_disk: true` and shows their version.
- **Changes are refused after an outside save.** A change checks that the file on disk is
  byte for byte what the server last read or wrote; if not, it is refused with a `conflict`
  error and nothing is written. Call `budget_open` (which takes the file as it is and drops
  the undo steps, since they would undo onto someone else's work), look at what changed
  (`diff` with `since: "disk"` before re-opening), and make the change again.
- **Every change is checked.** It is made on a copy; when the budget added up before and
  wouldn't after (an unknown global, a cycle), it is refused. A change that changes nothing
  writes nothing.
- **Writes are atomic.** A temporary file in the same folder, flushed, then renamed over the
  budget, keeping its permissions. A reader never sees half a file.
- **Undo.** `undo` writes the file back as it was before the server's last change to it, up
  to 50 steps per file (fewer for very large files: 256 MB of steps at most), as long as
  nobody else saved it since. Replacing a whole file (`save_as`, `new_budget`, `merge`,
  `import_sheet` with `overwrite`) is an undoable change of that file too.
- **Shared copies are read-only.** A budget saved for sharing (its provenance says so) is
  refused for every change, as the app refuses it; `save_as` with `editable: true` makes an
  editable copy.
- At most 32 files are kept open; past that the one used longest ago is forgotten, with its
  undo steps.

## Tools

43 tools; `--read-only` keeps the 14 marked read.

| Group | Tools |
|---|---|
| Files and summaries (read) | `list_budgets`, `budget_open`, `budget_info` |
| Reading (read) | `list_topsheet`, `list_accounts`, `get_account`, `list_globals`, `list_setup`, `list_charges`, `incentives`, `totals`, `explain`, `find`, `validate`, `diff` |
| Lines | `add_lines`, `update_lines`, `delete_lines`, `move_lines`, `tag_lines` |
| Setup | `set_global`, `add_setup`, `update_setup`, `rename_setup`, `delete_setup` |
| Charges and credits | `set_charge`, `set_credit`, `apply_charge`, `delete_charge` |
| Topsheet | `add_category`, `update_category`, `delete_category`, `add_production_total`, `delete_production_total`, `add_account`, `update_account`, `delete_account`, `set_title` |
| Files | `new_budget`, `save_as`, `export`, `import_sheet`, `import_budget_export`, `merge`, `undo` |

Every tool that works on one budget takes `path`. Tools that read totals take
`exclude_groups` (a list of group names whose lines are left out, as `method-budget totals
--exclude` does) where it makes sense.

### Files and summaries

**`list_budgets`** `{folder?, recursive?}`: the `.mthd` files under the first root
(or `folder`), with their sizes; hidden folders and symlinks are skipped (at most 2,000
files, 8 folders deep).

**`budget_open`** `{path}`: opens or re-opens a file; returns what `budget_info` returns plus
`reloaded` and `undo_dropped`.

**`budget_info`** `{path}`: `title`, `base_currency`, `totals_currency`, `account_coding`
(segment widths and separator), `fringe_posting` (`budget`, `level`, `category` with its
`suffix`, or `account`), `counts` (categories, production totals, accounts, lines, rows,
globals, fringes, groups, sets, locations, units, currencies, charges, credits, versions),
`totals` (the headline totals in cents) or `problem`, `read_only`, `provenance`,
`revision`, `changed_on_disk` and `undo` (the labels of the last undo steps, newest first).

```json
{"name": "budget_info", "arguments": {"path": "Feature/draft-3.mthd"}}
```

### Reading

**`list_topsheet`** `{path}`: the topsheet in order: categories (`code`, `name`, `accounts`,
`total_cents`, `fringe_total_cents`, `fringe_account`) and production totals (`name`,
`total_cents`, `above_the_line` for the first), plus the headline totals.

**`list_accounts`** `{path, category?}`: accounts with `category`, `rows`, `lines`,
`total_cents`, `details_total_cents`, `fringe_total_cents`.

**`get_account`** `{path, account, offset?, limit?}`: the account's rows, 500 at a time by
default (5,000 at most). Each row: `id`, `n`, `kind` (`line`, `subtotal`, `adjustment`,
`separator`), `description`, `amount`, `unit`, `x`, `unit2`, `rate`, `unit3`, `x4`, `unit4`,
`currency`, `fringes`, `groups`, `set`, `location`, `fringe_range`, `bold`, `italic`,
`indent`, `color`, `notes`; and what it works out to: `included`, `quantity`,
`row_currency`, `local_total_cents`, `total_cents`, `subtotal_cents`, `fringes_cents` (per
fringe), `fringe_total_cents`, `fringe_full_total_cents` (the fringes with no cutoff and no
range: each percent fringe on the whole total, the flat ones as earned; 0 on a row that
doesn't count; decisions.md, entry 61).

```json
{"name": "get_account", "arguments": {"path": "film.mthd", "account": "21-01"}}
```

**`list_globals`** `{path}`: `name`, `formula`, `value`, `description`, `unit`, `lines` (rows
using it), `other_uses`.

**`list_setup`** `{path, type}`: one table (`fringe`, `group`, `set`, `location`, `unit`,
`currency`) with every field and its use (`lines`, `other_uses`) and money where it means
something (a fringe's `total_cents`, a group's active and full totals: `active_total_cents`, `active_fringes_cents`,
`full_total_cents` (lines, suppressed ones at their base) and `full_fringes_cents` (only the
fringes earned; decisions.md, entry 58), a set's or location's
totals). A percent fringe has `cutoff_cents`; a flat fringe has `unit`, `basis` and
`max_hours_per_day` (a decimal string or null), and `unused_cutoff_cents` when it keeps a
cutoff it never uses. The base currency is listed first with `base: true`; the others have
`symbol_after` and `separators` (`comma_point` or `point_comma`; decisions.md, entry 61).

**`list_charges`** `{path}`: charges (`charge_kind`, `percent` or `flat_amount_cents` and
`currency`, `applied`, `account`, the exclusions, and `base_cents`, `excluded_cents`,
`amount_cents` when applied) and credits (`rows` with their `source`, `percent` and `cap`,
`cap_cents`, `discount_percent`, `locations`, `amount_cents`).

**`incentives`** `{path}`: spend at each location (`locations`: `location`, `null` for
lines with none, `known`, `lines`, `fringes`, `total`) and every credit as an incentive
program, applied or not (`programs`: `name`, `applied`, `locations`, `cap`,
`discount_percent`, `qualified` and `by_location`, `amount` after the caps and discount,
`capped`, `problem`, `figures`), money in integer cents (decisions.md, entry 80). Nothing
changes.

**`totals`** `{path, exclude_groups?}`: `levels` (each production total with its categories
and their accounts), `total_atl_cents`, `total_btl_cents`, `atl_btl_cents`,
`budget_fringes_cents`, `charges_cents` and each charge, `grand_total_cents`, `credits_cents`
and each credit, `net_total_cents`, `net_total_in_totals_currency_cents` (converted
account by account, fringe by fringe, charge by charge and credit by credit, then added up;
decisions.md, entry 60). The summary is the topsheet as `method-budget show` prints it.

**`explain`** `{path, total, depth?}`: how one total is worked out (the engine's explain, as
`method-budget explain` and the app's Explain tab show it). `total` is an account (`21-01`,
`21-01/lines`, `21-01/fringes`), a category (`21`, `21/fringes`), a line (`21-01:3` or
`#14`), `level:NAME`, `charge:NAME`, `credit:NAME`, `fringe:NAME`, or `atl`, `btl`,
`atl+btl`, `fringes`, `credits`, `grand`, `net`. Returns `title`, `reported_cents` and
`tree`: steps with `label`, `text` (the working in words), `amount_cents`, `role` (`sum`
adds its children, `value` is worked out from notes, `note` only explains), `minus`, and
`children` down to `depth` levels (default 4, at most 16; 2,000 steps at most), or
`more_steps` where it stops. The leaves add up to the total exactly.

```json
{"name": "explain", "arguments": {"path": "film.mthd", "total": "fringe:FICA", "depth": 3}}
```

**`find`** `{path, query, case_sensitive?, limit?}`: where a piece of text appears: line
descriptions, formulas and notes (`row`, `n`, `field`), account and category numbers and
names, setup item names, charges and credits. 100 matches by default, 1,000 at most, with
`total_matches`.

**`validate`** `{path}`: `ok`, `problem` (why it doesn't add up, if it doesn't),
`net_total_cents` and `warnings`: lines naming units, sets or locations the budget doesn't
have, account or category numbers used twice, fringes of categories after the last
production total that no level holds (fringes posted by production level; decisions.md,
entry 56), and credit rows on a category or account the budget doesn't have, which count
0 (entries 57 and 61).

**`diff`** `{path, against?, since?}`: what changed, in plain words, with the money that
moved per account, category and topsheet line (the engine's diff, as `method-budget diff`).
`against` compares another budget (the older one) with `path`. Without it, `since:
"session"` (the default) shows what changed since the server opened the file (its own
changes, and any outside save it has since re-opened), and `since: "disk"` shows what someone
else saved since the server last read it. Returns `changed`, `change_count`, `changes` (each
`kind`, `group`, `subject`, `text`, `money_cents`, `fields` with `before` and `after`; at
most 1,000), `accounts`, `categories`, `totals` (each `label`, `before_cents`,
`after_cents`, `delta_cents`).

### Lines

**`add_lines`** `{path, account, after_row?, at_start?, use_defaults?, lines}`: adds rows in
order after a row, at the top, or at the end (up to 5,000 per call). Each row may give
`kind` (`line` by default, `subtotal`, `adjustment`, `separator`), any field `update_lines`
takes, and `note`. A new line copies the tags the budget's new-line defaults ask for
(fringes, by default) from the nearest line above it, rows earlier in the same call
included, unless it gives them itself or `use_defaults` is false. Returns `added` (`id` and
`n` of each).

```json
{"name": "add_lines", "arguments": {"path": "film.mthd", "account": "21-01", "after_row": 8,
  "lines": [{"description": "1st AC", "amount": "SHOOT_DAYS", "unit": "Day", "rate": 450}]}}
```

**`update_lines`** `{path, rows: [{id, ...fields, add_note?}]}`: changes fields by row id.
Fields: `description`; the formulas `amount`, `x`, `rate`, `x4` (text or a number); the units
`unit`, `unit2`, `unit3`, `unit4` (a unit in the budget, or null); `currency` (a currency in
the budget, or null for the base), `set`, `location` (each must exist, or null);
`fringe_range` (a number or null); `bold`, `italic`; `indent` (0 to 4); `color` (`#rrggbb`
or null); `fringes` and `groups` (each replaces the row's whole list). A subtotal row takes
only the description and style; an adjustment takes no rate, rate unit or currency (it does
take fringes, decisions.md, entry 56);
a separator only style. Every row in the call must work or nothing is written.

**`delete_lines`** `{path, rows: [ids]}`: all of them, or none when one is missing.

**`move_lines`** `{path, rows: [ids], to_account, after_row?, at_start?}`: moves rows, in the
order given, keeping their ids.

**`tag_lines`** `{path, rows?, account?, add_fringes?, remove_fringes?, add_groups?,
remove_groups?}`: puts fringes and groups on rows (by id, or every line and adjustment of an
account) or takes them off. A fringe or group must exist to be added. Fringes and groups go
on lines and adjustments; other rows are counted in `rows_skipped`.

Every change returns `changed`, `revision`, and `totals`: the headline totals `before` and
`after` and `categories_moved` (each category whose total moved), so a model can check the
effect without another call; the summary gives the net total's move.

### Setup

**`set_global`** `{path, name, formula, description?, unit?}`: sets a global's formula, adding
it when new; returns its `value`.

**`add_setup`** / **`update_setup`** `{path, type, name, fields}`: a fringe, group, set,
location, unit or currency. `update_setup` changes only the fields given (null clears an
optional one). Fields by type:

| Type | Fields |
|---|---|
| fringe | `fringe_kind` (`percent`, the default, or `flat`), `rate` (a formula: `7.65` is 7.65%, or a flat amount), `cutoff` (percent: pay per earner it stops at, `"168,600.00"`, or null; a flat fringe keeps one it had, from turning flat, but never uses it, and takes only null), `unit` (flat: charged per this unit; the line's Amount converts into it through unit hours, and a line or fringe whose unit has no hours is charged once), `basis` (flat: `amount`, the standard and what null means: the Amount column only, X and 4X never count; `amount_counts`: also times X and 4X when their units aren't time units, a blank unit being a count; `quantity`: Amount x X x 4X), `max_hours_per_day` (flat, charged per hour: most hours a day counted, `12`, or null; days are the Amount converted through the Day unit, hours a day are X when X is in hours, else the Day unit's hours), `description`, `short_code`. Fields left out keep their value; the cutoff stays when the kind changes, a flat fringe's unit, basis and cap go when it turns percent (decisions.md, entry 54) |
| group | `in_budget_total` (false: its lines are shown but count as 0), `description`, `short_code`, `color` |
| set | `description` |
| location | `description`, `city`, `region`, `country` |
| unit | `plural`, `description`, `hours` (hours in one of it, or null) |
| currency | `rate` (base-currency units per one of it, more than 0), `description`, `symbol`, `symbol_after` (true writes totals shown in it as `1.234,56 €` instead of `€1.234,56`), `separators` (`comma_point`, `1,234.56`, or `point_comma`, `1.234,56`; decisions.md, entry 61). Left out, they keep their value |

```json
{"name": "add_setup", "arguments": {"path": "film.mthd", "type": "fringe", "name": "FICA",
  "fields": {"rate": "7.65", "cutoff": "168,600.00"}}}
{"name": "add_setup", "arguments": {"path": "film.mthd", "type": "fringe", "name": "PENSION_HR",
  "fields": {"fringe_kind": "flat", "rate": "1.25", "unit": "Hour", "max_hours_per_day": "12"}}}
```

**`rename_setup`** `{path, type, from, to}`: renames a global, fringe, group, set, location,
unit or currency and rewrites every reference (formulas, lines, charges, credits, presets),
with the engine's rename (decision 32); returns `references`. Totals never change.

**`delete_setup`** `{path, type, name, force?}`: an item nothing uses goes. One still in use is
refused, saying how many lines and other settings use it, unless `force` is true: a global's
value is then frozen into the formulas that used it (no total changes), a fringe, group, set
or location is taken off its lines (totals may change), a unit is blanked where it is named,
and lines in a currency go back to the base currency. A group a credit is based on, a
currency the totals or a flat charge use, and the base currency are always refused.

### Charges and credits

**`set_charge`** `{path, name, new_name?, charge_kind?, percent?, amount?, currency?, account?,
applied?, excluded_groups?, excluded_categories?, excluded_accounts?, excluded_amounts?,
label_percent?, label_excluded?}`: adds a charge or changes the one with that name (only the
fields given). A new charge needs `charge_kind` and its `percent` (of above- plus
below-the-line) or flat `amount`, and is applied unless `applied` is false.

```json
{"name": "set_charge", "arguments": {"path": "film.mthd", "name": "Contingency",
  "charge_kind": "percent", "percent": 10, "excluded_categories": ["50"]}}
```

**`set_credit`** `{path, name, new_name?, account?, applied?, cap?, discount_percent?,
excluded_fringes?, locations?, rows?}`: adds a credit or changes it; `rows` replaces every row.
`locations` limits every group, category and account row to lines at those locations,
with their fringes (decisions.md, entry 80); empty counts every line, left out keeps it,
and a location the budget doesn't have is refused.
`excluded_fringes` is the credit's one list of fringes left out of every group, category
and account row's spend (decisions.md, entries 57 and 61); left out keeps it, and changing
the rows never changes it. Each row: `source` (`{"kind": "flat", "amount": "250000"}`,
`{"kind": "group", "group": "Local hire"}`, `{"kind": "category", "category": "2100"}`: a
topsheet category's total with the fringes its lines earn, wherever fringes post, or
`{"kind": "account", "account": "2101"}`: an account's lines with the fringes they earn,
wherever fringes post), `percent`, `cap` (`{"kind": "none" | "qualified_spend" |
"credit", "amount"}`), `description`. A group, category, account or fringe the budget
doesn't have is refused. `list_charges` shows each credit's
`excluded_fringes` and `locations`.

**`apply_charge`** `{path, type, name, applied}`: makes a charge or credit count or stop
counting. **`delete_charge`** `{path, type, name}` removes it.

### Topsheet

**`add_category`** `{path, code, name, after_category? | before_total? | at_start?}`: the
number is normalized by the budget's account numbering (`3` becomes `03`).
**`add_production_total`** `{path, name, after_category?}`: a production total subtotals the
categories since the previous one; the first one ends above-the-line.
**`add_account`** `{path, category, code, name, after_account?}`: the number is normalized
and must be unique. **`update_category`** and **`update_account`** `{path, code, new_code?,
name?}`. **`delete_category`** and **`delete_account`** `{path, code, force?}`: refused while
they hold rows, unless `force`. **`delete_production_total`** `{path, name}`.
**`set_title`** `{path, title}`.

### Files

**`new_budget`** `{path, from?, template?, keep_lines?, title?, currency?, overwrite?}`: the
starter budget (a small chart of accounts with placeholder rates; the default), a `blank`
one, or one from a `template` budget: its chart, setup and settings, its lines only with
`keep_lines`, never its info, versions, notes, actuals or provenance (as the app's New From
Template). The starter and a blank budget start with the new-budget settings
(decisions.md, entry 60): fringes posted by budget, account numbers of category 2, account
2 and set 3, a Day of 12 hours, a Week of 60, a Month of 240, and a cutoff factor of 1.0.

**`save_as`** `{path, out, editable?, overwrite?}`: a copy in the canonical form;
`editable: true` turns a shared copy's copy editable, keeping its provenance.

**`export`** `{path, format, out, preset?, sections?, date?, bom?, overwrite?}`: `xlsx` (one
workbook), `csv` (one file per table, `<stem> - <Table>.csv` beside `out`), `tsv` (the detail
as tab-delimited text, `.txt` or `.tsv`), or a report from a saved preset (or the topsheet
and detail) as `html`, `txt`, `pdf` or `report_csv`, exactly as `method-budget export` and `method-budget
report` make them. `out` must have the format's extension; files that exist are replaced
only with `overwrite`. `date` fixes the printed date for output that must not change.

**`import_sheet`** `{sheet, out, into?, mapping?, title?, currency?, overwrite?, dry_run?}`:
a spreadsheet's rows as detail lines, with the app's importer and limits (decision 36), as
`method-budget import`. `mapping` is the same object as `method-budget import --mapping` (`sheet`,
`header_row`, `columns`, `create_accounts`, `create_setup`, `default_account`). With
`into`, the lines go into a copy of that budget, or into the budget itself (as an undoable
change) when `out` is the same file. Returns the lines added, what was created, skipped rows
and warnings (the first 20), the sheet-versus-budget reconciliation and the totals.

```json
{"name": "import_sheet", "arguments": {"sheet": "camera.xlsx", "into": "film.mthd",
  "out": "film.mthd", "mapping": {"columns": {"account_code": "A", "description": "B",
  "amount": "C", "unit": "D", "rate": "E"}}}}
```

**`import_budget_export`** `{file, out?, format?, title?, overwrite?, dry_run?, all_differences?}`:
a whole budget from another budgeting program's export (budget exchange JSON or XML, its
Excel workbook or tab-delimited topsheet; `format` `json`, `xml`, `xml-advanced`, `xlsx` or
`tab` when the content doesn't say), as `method-budget import-export` (docs/IMPORT.md). Returns
`counts`, `notes` (`kind` `check`, `not_imported` or `info`: what was worked out from the
figures and should be checked, and what couldn't come over) and `reconciliation`: the
imported budget's totals against the export's own (`matches`, the grand and net totals in
cents, `tallies` per kind of figure, the first 50 `differences`, all with
`all_differences`). Writes `out` (a new file unless `overwrite`) when given, unless
`dry_run`; without `out` it only reads and compares.

```json
{"name": "import_budget_export", "arguments": {"file": "pilot.json", "out": "pilot.mthd"}}
```

**`merge`** `{base, ours, theirs, out, prefer?, dry_run?, overwrite?}`: the engine's
three-way merge (docs/MERGE.md): returns `conflicts` (each with `what`, `base`, `ours`,
`theirs`, `taken` and `text`), `renumbered`, `problem` and the merged totals; never writes
over one of the three.

**`undo`** `{path}`: takes back the server's last change to the file; returns `undone` (its
label), the totals before and after and `undo_left`.

## Safety model

The server treats the client as able to make mistakes and budget files as untrusted data.

- **Folders.** Every path must resolve inside a root. Relative paths start at the first root.
  An existing file is resolved with every symlink followed and must still be inside a root,
  so a link that leads out is refused; nothing is ever written through a symlink. Nothing
  below a root whose name starts with `.` (`.git`, `.ssh`, `.claude`, hidden files) is read
  or written.
- **Extensions.** Budgets are read and written only as `.mthd`; spreadsheets are
  read only as `.xlsx`, `.csv`, `.tsv` or `.txt`; an export's file must have its format's
  extension. The server can't be pointed at a script, a config file or a git hook.
- **No replacing by accident.** A new file (`new_budget`, `save_as`, `export`,
  `import_sheet`, `merge`) never replaces one that exists unless `overwrite` is true, and a
  budget is replaced only by another budget, as an undoable change refused like any other
  when the file changed on disk. A file that isn't a budget the server can read is never
  replaced.
- **Outside changes win.** A change is refused when the file isn't byte for byte what the
  server last read or wrote (see "Files, revisions and conflicts").
- **Atomic writes** that keep the file's permissions; a change that would stop the budget
  adding up is refused; shared copies are read-only.
- **Read-only mode** removes every writing tool from the list, and calling one fails.
- **Nothing runs, nothing leaves.** The server never starts a program, opens a socket or
  reads anything but the files named in calls (and the folders `list_budgets` walks).
- **Budget text is data.** Titles, descriptions, notes and names come back as JSON strings;
  summaries quote budget text (cut to 60 characters, control characters removed) and never
  present it as an instruction. The server's instructions to the client say so too.
- **Limits** (below) bound memory and time for every input.
- What it doesn't defend against: someone who can already write inside a root (they can
  change budgets without the server), or a race where a file is swapped for a link between
  the check and the write.

## Limits

| | |
|---|---|
| One protocol message | 8 MB |
| A budget file read or written | 128 MB |
| Rows in one batch (`add_lines`, `update_lines`, `delete_lines`, `move_lines`, `tag_lines` ids) | 5,000 |
| Free text (description, note) | 10,000 characters |
| A name or code | 200 characters |
| A formula | 4,096 characters (the engine's limit) |
| A path | 4,096 bytes |
| Open files | 32 |
| Undo steps per file | 50, and 256 MB |
| `get_account` page | 5,000 rows |
| `explain` | 16 levels, 2,000 steps |
| `find` | 1,000 matches |
| `diff` changes listed | 1,000 |
| `list_budgets` | 2,000 files, 8 folders deep |

Spreadsheets also have the importer's own limits (docs/APP.md, "Security model").

## Live app bridge (planned)

The file server can't see what is on screen or edit a budget that is open with unsaved
changes: it would be refused with a conflict as soon as the app saves. The bridge is a second
mode that reaches into the running app instead. It is designed here, not built; how its
switch, approval and status look in the app is still to be designed (open questions below).

- **Opt-in, per app.** Off by default. An app setting turns it on; turning it off closes
  every connection.
- **Local only.** The app listens on a Unix domain socket in its own data folder (mode 0600,
  in a 0700 folder) on macOS and Linux, and on a named pipe whose security descriptor admits
  only the current user on Windows. No TCP port, nothing on the network.
- **Approved tokens.** `method-budget mcp --app` connects to the socket and asks for access with a
  name ("Claude Code in ~/Budgets") and a short code it prints. The app asks the user to
  approve it, showing the same name and code; approving issues a random 256-bit token, kept
  by the command line in the user's keychain (or a 0600 file) and by the app as a hash, with
  its last use, so the user can revoke it. Tokens expire after 30 days unused, and a token
  may be read-only.
- **Same protocol, same tools.** The bridge speaks MCP to the client exactly as the file
  server does, with the same tools and schemas, so a skill works with both. A `path` names an
  open budget's file (or `window:N` for an untitled one); `list_budgets` lists the open tabs.
- **Edits go through the session.** Every change is applied with the app's own
  `Session::edit`, with a label naming the agent ("Agent: Add 3 lines to 21-01"), so it shows
  in the History tab, takes part in undo and redo with everything else, autosaves and
  recovers as the user's own edits do, and redraws every window showing the budget. Changes
  run on the app's thread one at a time, between the user's own edits.
- **The app's rules hold.** Shared copies stay read-only (`Session::edit` refuses them, as
  now), and the per-window rules apply: a budget is changed only where the app itself would
  let the user change it, never behind a dialog that is editing it.
- **Open questions for the app's design:** where the switch lives, what the approval prompt
  looks like, whether a connected agent is shown in the window (and how its changes are
  pointed out), and which windows or panes count as off limits.

## See also

docs/CLI.md (the command line the server reuses), docs/FORMAT.md (the file format),
docs/MERGE.md (diff and merge), docs/APP.md ("Security model"), and decisions.md (the
entry on the MCP server).
