# Method Budget native file format

This is the open, documented file format Method Budget uses to save budgets. It is
plain JSON, designed to be read and written by other tools and to diff cleanly in version
control. The reference implementation is `crates/method-format` (`to_string` and `from_str`).

File extension: `.mthd`. Encoding: UTF-8. The registered MIME type is
`application/vnd.method-budget+json`; since the content is plain JSON, tools may also
treat it as `application/json`.

## Identity

- **Extension.** Files are `.mthd` (decisions.md, entry 46).
- **The `format` field.** Always `"method-budget"`. Readers reject any other value.

## At a glance

- Pretty-printed JSON, 2-space indent, `\n` line endings, one trailing newline.
- Keys always appear in the order documented below. Map keys (names of units, fringes,
  groups and so on) are sorted by byte value. The same budget always produces the same
  bytes.
- Money is a JSON **integer** of cents (`150000` is 1,500.00). Never a float.
- Decimals (rates, percentages, exchange rates, hours) are JSON **strings**, like `"7.65"`.
  Never a JSON number.
- Formulas (amounts, rates, globals) are strings exactly as the user typed them.
- Fields at their default value are left out, so files stay small and diffs stay focused.
- Readers are strict: an unknown field is an error, so typos are caught instead of
  silently ignored.
- Detail rows keep their `id` through every edit and move, and accounts and setup items
  keep their numbers and names, so two budgets can be compared and merged by identity
  rather than by position (`method-budget diff`, `method-budget merge` and a git merge driver: docs/MERGE.md).

## Top level

```json
{
  "format": "method-budget",
  "version": 1,
  "budget": { ... }
}
```

| Field     | Type    | Notes |
|-----------|---------|-------|
| `format`  | string  | Always `"method-budget"` (see [Identity](#identity)). Anything else is rejected as "not a budget file". |
| `version` | integer | Format version, currently `1`. See [Versioning](#versioning). |
| `budget`  | object  | The [budget](#budget). |

## Value types

| Name       | JSON form | Meaning |
|------------|-----------|---------|
| text       | string    | Any UTF-8 text. Leading and trailing spaces are kept. |
| cents      | integer   | Money in cents, a signed 64-bit integer. `-12345` is -123.45. |
| decimal    | string    | Fixed-point number with up to 9 decimal places, like `"12.5"`, `"-0.25"`, `"30"`. |
| formula    | string    | An expression such as `"30"`, `"SHOOT_DAYS"` or `"WEEKS * 5"`, stored as typed. |
| row id     | integer   | Unsigned 64-bit integer identifying a detail row. |
| name set   | array of strings | Unordered set of names. Written sorted with no duplicates. |

**Decimals.** Writers produce the canonical form: an optional `-`, digits, and a fractional
part only when needed, with no trailing zeros (`"4.5"`, not `"4.50"`; `"3"`, not `"3.0"`).
Readers also accept a leading `+`, trailing zeros, and a missing integer or fractional part
(`".5"`, `"7."`). They reject exponents (`"1e5"`), more than 9 decimal places, values too
large to store, and JSON numbers. Values are rounded half away from zero where the engine
rounds (see `docs/decisions.md`), but the file stores them exactly.

**Formulas** are not decimals: they keep exactly what was typed, so `"1200.50"` stays
`"1200.50"`, and a number in one may have any number of decimal places
(`"0.0000000149"`); how many count depends on the budget's [arithmetic](#arithmetic). An
empty formula means "not filled in" (see [detail rows](#detail-row)).

**Big integers.** Cents and row ids use the full 64-bit range. JavaScript and other
readers that parse every number as a double lose precision above 2^53 (about 90 trillion
dollars in cents), so such tools should use a big-integer-aware JSON parser if they need
exact values at that scale.

**Tagged objects.** Where a value can be one of several kinds, it is an object with a
`"kind"` field in `snake_case` that comes first and decides which other fields are allowed.

## Defaults and omitted fields

Unless a table below marks a field **required**, it may be left out and then takes its
default:

| Type | Default when omitted |
|------|----------------------|
| text | `""` (empty) |
| optional text, cents or integer | none (not set) |
| map (object of named entries) | empty |
| list or name set | empty |
| `applied`, `in_budget_total` | `true` |
| `bold`, `italic` | `false` |
| `indent` | `0` |
| `kind` of a detail row | `"line"` |

Writers always leave out fields that equal their default. For the fields marked optional
(optional text, cents or integer), `null` is also accepted on read and means "not set". Note that "not set" and "set to an empty string"
are different for optional text fields (for example a detail's `currency`), and the
format keeps that difference: `"currency": ""` is written when it is set but empty.

## Budget

Fields in written order:

| Field             | Type | Notes |
|-------------------|------|-------|
| `title`           | text, **required** | Budget title. |
| `base_currency`   | text, **required** | Currency code that totals are computed in and that lines without a currency use. |
| `totals_currency` | optional text | Currency the totals are shown in, when not the base currency. |
| `provenance`      | [provenance](#provenance) | Where this budget came from, when it is a copy saved for sharing, a sub-budget or a [setup pack](#setup-packs). Left out otherwise. |
| `account_coding`  | [account coding](#account-coding) | Left out when it is the default pattern. |
| `fringe_posting`  | [fringe posting](#fringe-posting) | Left out when it is the default (by category, suffix `"99"`), which is also what a missing field means. A new budget posts by budget, so its file has the field. |
| `trailing_level_fringes` | `"keep"` or `"drop"` | Under production-level posting, what happens to the fringes of categories after the last production total (see [trailing level fringes](#trailing-level-fringes)). Left out when `"keep"`, which is also what a missing field means. |
| `cutoff_factor`   | `"1.0"`, `"1.5"`, `"2.0"`, `"2.5"` or `"3.0"` | What every percent fringe's cutoff is multiplied by (see [cutoff factor](#cutoff-factor)). Left out when `"1.0"`, which is also what a missing field means. |
| `fringe_row`      | [fringe row](#fringe-row) | The account number, description and place of the budget's one fringe row (fringes posted by budget). Left out when every field is the default. |
| `arithmetic`      | `"binary"` or `"exact"` | How totals are worked out (see [arithmetic](#arithmetic)). Left out when `"binary"`, which is also what a missing `arithmetic` means. |
| `round_account_totals` | `true` or `false` | Round every account's total to a whole unit and work out everything above accounts from the rounded totals (see [account totals](#account-totals)). Default `false`, left out then. |
| `new_line_defaults` | [new-line defaults](#new-line-defaults) | Left out when it is the default (copy fringes only). |
| `info`            | map of [info values](#info-value) | Budget information fields, keyed by label. |
| `currencies`      | map of [currencies](#currency) | Foreign currencies, keyed by code. |
| `units`           | map of [units](#unit) | Keyed by singular name. Omitted means **no units**. |
| `globals`         | map of formula | Named values usable in formulas, keyed by name. A name is letters, digits and `_` with at least one letter or `_`; it may start with digits (`"1ZG"`), and all digits (`"12"`) is a number, not a name. |
| `global_info`     | map of [global info](#global-info) | Description, unit and display settings of globals, keyed by global name. Left out when no global has any. |
| `fringes`         | map of [fringes](#fringe) | Keyed by the name detail rows use. |
| `groups`          | map of [groups](#group) | Keyed by group name. |
| `group_rules`     | [group rules](#group-rules) | Which lines the groups leave in or out, budget-wide. Left out when every rule is the default. |
| `sets`            | map of [sets](#set) | Keyed by set name. |
| `locations`       | map of [locations](#location) | Keyed by location name. |
| `topsheet`        | list of [topsheet rows](#topsheet-row) | Categories and production totals, in display order. |
| `charges`         | list of [charges](#charge) | Contractual charges, in the order they were made (charges never apply to each other, so the order changes no figure). The topsheet lists them newest first, the reverse of this list. |
| `credits`         | list of [credits](#credit) | Applied credits (incentives), in application order. The topsheet lists them in reverse, then their subtotal when there are two or more. |
| `baseline`        | [baseline](#baseline) | Frozen totals for comparison. Left out when there is none. |
| `actuals`         | [actuals](#actuals) | Actual costs from accounting and estimates to complete. Left out when there are none. |
| `display`         | [display settings](#display-settings) | How numbers show on screen. Left out when every setting is the default. |
| `setup_order`     | [setup order](#setup-order) | Hand-made display order of the setup tables. Left out when every table is in name order. |
| `setup_folders`   | [setup folders](#setup-folders) | Folders of setup items per kind (the personal library's). Left out when no kind has a folder. |
| `setup_sources`   | [setup sources](#setup-sources) | Where setup items came from: the setup pack each was imported from, and source notes and links. Left out when no item has one. |
| `report_presets`  | list of [report presets](#report-preset) | Saved report setups, in the user's order. Left out when there are none. |
| `tag_presets`     | list of [tag presets](#tag-preset) | Saved bundles of line tags, in the user's order. Left out when there are none. |
| `line_presets`    | list of [line blocks](#line-block) | Saved blocks of rows to insert, in the user's order. Left out when there are none. |
| `ot_patterns`     | list of [overtime patterns](#overtime-pattern) | Saved overtime patterns for the overtime helper, in the user's order. Left out when there are none. |
| `compact_versions` | `true` or `false` | Write `versions` as changes instead of full copies ([version deltas](#version-deltas)). A file setting, not content. Default `false`, left out then. |
| `versions`        | list of [versions](#version) | Named copies of the budget, oldest first. Left out when there are none. |

A new budget in Method Budget starts with a set of editable starter units: Day (12
hours), Week (60), Month (240), Hour (1), and Allow and Flat with no hours. They are
ordinary data and are written out like any other units, so a file with no `units` field
really has no units, and a file keeps the hours it has. A new budget also starts posting
fringes by budget, with account numbers of category 2, account 2 and set 3 and no
separator (so both `fringe_posting` and `account_coding` are written), a new line copying
only its fringes from the line above, and a cutoff factor of 1.0 (`docs/decisions.md`,
entries 60 and 61). A file
without those fields keeps the defaults this page gives them.

### Account coding

How account numbers are split into segments and written. Left out entirely when it equals
the default: category width 2, account width 2, no sub-account, no set, separator `"-"`.
When present, every field is required. A new budget starts with a set width of 3 and no
separator (category 2, account 2, no sub-account, set 3, `""`, so numbers read `1101`;
`docs/decisions.md`, entry 61), so its file has the field.

| Field            | Type | Notes |
|------------------|------|-------|
| `category_width` | integer 0 to 255 | Characters in the category segment. |
| `account_width`  | integer 0 to 255 | Characters in the account segment (0 means unused). |
| `sub_width`      | integer 0 to 255 | Characters in the sub-account segment. |
| `set_width`      | integer 0 to 255 | Characters in the set segment. |
| `separator`      | string | One character placed between segments, or `""` for none. |

The file stores the pattern as is. The application's own rules (widths up to 6, which
separators are allowed, each used segment after a used one except that the set may follow
the account when there is no sub-account) are checked by the application, not by the file
reader.

### Fringe posting

Where fringe totals appear. A tagged object:

| `kind`             | Other fields | Meaning |
|--------------------|--------------|---------|
| `budget`           | none | One fringe total for the whole budget. |
| `production_level` | none | One fringe row above each production total. |
| `category`         | `suffix`: text, **required** | A fringe account at the end of each category, numbered from the category's part and the suffix, written with the account coding: `1100` with `"99"` is `1199`; a number that isn't a plain category gives its first category-width characters (`510` with `"98"` is `5198`; `docs/decisions.md`, entry 61). |
| `account`          | none | A fringe line at the end of each account. |

Wherever fringes post, a percent [charge](#charge) applies to them, budget-wide ones too.

### Trailing level fringes

Under `production_level` [posting](#fringe-posting), each level's fringes post in a row
above the production total that closes it, so the fringes of categories after the last
production total (every category, when there is none) have no production total to post
above (`docs/decisions.md`, entry 56). A budget setting; a [version](#version) keeps its
own. It changes nothing under the other posting modes.

| Value    | Meaning |
|----------|---------|
| `"keep"` | The default, also for files without the field. They post in a fringe row of the unnamed level after the last production total, so they are in the grand total and in a percent charge's base, though in neither above- nor below-the-line. `method-budget check` warns about them. |
| `"drop"` | They post nowhere, so they are in no total (lines still earn them, and fringe, group, set and location totals and credits still count them). |

### Cutoff factor

What every percent [fringe](#fringe)'s `cutoff` is multiplied by before it is used, as
the percentage fringe cutoff factor of industry-standard budgeting software works
(`docs/decisions.md`, entry 60): a 2,222.22 cutoff at `"1.5"` is 3,333.33. The product is
rounded to cents by the budget's [arithmetic](#arithmetic). Flat fringes have no cutoff,
so it changes nothing for them. One of `"1.0"` (the default, also for files without the
field), `"1.5"`, `"2.0"`, `"2.5"` and `"3.0"`, written as text; `"2"` reads as `"2.0"`,
and any other value is an error. A budget setting; a [version](#version) keeps its own.

### Fringe row

The budget's one fringe row, when fringes [post](#fringe-posting) by budget
(`docs/decisions.md`, entry 60). Display only: where it shows and what it says never
changes a total, above-the-line or below-the-line. Left out entirely when every field is
the default. A budget setting; a [version](#version) keeps its own.

| Field         | Type | Notes |
|---------------|------|-------|
| `account`     | optional text | The account number shown on the row. Not set (the default): none. |
| `description` | optional text | What the row says. Not set (the default): `Total Fringes`. |
| `position`    | optional integer | The [topsheet row](#topsheet-row) it shows just above, counted from 0; the topsheet's length or more puts it after the last one. Not set (the default): after the above- and below-the-line summary rows. |

A blank `account` or `description` reads as not set.

### Arithmetic

How the budget's totals are worked out (`docs/decisions.md`, entry 55). A budget setting;
a [version](#version) keeps its own.

| Value     | Meaning |
|-----------|---------|
| `"binary"` | The default, also for files without the field. Spreadsheet arithmetic, as industry-standard budgeting software works: formulas, globals and every money step (line totals, currency conversion, fringes, charges, credits, adjustment rows) are binary doubles, and each amount is rounded to cents once like JavaScript's `Math.round(v * 100) / 100`, a half going up on the double. So `1 x 1.005` is 1.00 (1.005 is 1.00499999999999989... as a double), `-1 x 0.125` is -0.12, and a global `17 / 3` is 5.666666666666667. A line in a foreign currency is converted before it is rounded. A number in a formula keeps every place it is written with. |
| `"exact"` | Exact decimals: numbers keep 9 places, each amount is the exact product rounded to cents once, half away from zero (`1 x 1.005` is 1.01), and a foreign line is rounded in its own currency, then converted. A number in a formula with more than 9 places is an error. |

Either way, account, category, level and grand totals are integer sums of the rounded
cents, and credit rows are added up before the credit's one rounding.

### Account totals

`round_account_totals` (`docs/decisions.md`, entry 69), as the "Decimal Places in Totals"
setting of industry-standard budgeting software works at 0: when `true`, each account's
lines total is rounded to the nearest whole unit (3,426.50 is 3,427.00), and categories,
production levels, the total before charges, a percent charge, the grand total and the net
total are worked out from the rounded accounts. Lines, fringes (on lines, accounts,
categories and the budget's fringe row), a charge's excluded lines and a credit's spend
keep their cents. Under fringe posting by account, an account's total is its rounded
lines plus its fringes. A half rounds up in `"binary"` [arithmetic](#arithmetic) and away
from zero in `"exact"`; the two differ only on a negative half (-50.50 is -50.00 or
-51.00). Default `false` (nothing is rounded beyond cents), also for files without the
field. A budget setting; a [version](#version) keeps its own.

### New-line defaults

Which tags a line inserted in the application copies from the nearest line above it (or
below it, when there is none above). An editing preference saved with the budget; it never
changes any total by itself. Left out entirely when it equals the default: `fringes` on,
every other switch off. When present, every field is required.

| Field      | Type | Notes |
|------------|------|-------|
| `fringes`  | `true` or `false` | Copy the neighbor's fringes. |
| `groups`   | `true` or `false` | Copy its groups. |
| `set`      | `true` or `false` | Copy its set. |
| `location` | `true` or `false` | Copy its location. |
| `currency` | `true` or `false` | Copy its currency. |

### Info value

A tagged object with one `value` field, **required**:

| `kind`   | `value` type | Example |
|----------|--------------|---------|
| `text`   | text | `{"kind": "text", "value": "J. Director"}` |
| `number` | decimal | `{"kind": "number", "value": "30"}` |
| `date`   | text | `{"kind": "date", "value": "2027-03-01"}` |

Dates should be ISO 8601 (`YYYY-MM-DD`). The reader does not check the date text.

### Currency

| Field         | Type | Notes |
|---------------|------|-------|
| `description` | text | |
| `symbol`      | text | |
| `rate`        | decimal, **required** | Base-currency units per one unit of this currency. |
| `symbol_after` | boolean, default `false` | Write the symbol after the value (`1.234,56 €`) instead of before it (`$1,234.56`). Left out when `false`. |
| `separators`  | `"comma_point"` or `"point_comma"`, default `"comma_point"` | Thousands and decimal separators: `1,234.56` or `1.234,56`. Left out at the default. |

`symbol_after` and `separators` are display only (`docs/decisions.md`, entry 61): totals
shown in this currency (a budget's `totals_currency`) are written with them; no figure
depends on them.

### Unit

| Field         | Type | Notes |
|---------------|------|-------|
| `plural`      | text | Plural display name. |
| `description` | text | |
| `hours`       | optional decimal | How many hours one of this unit is. Left out for non-time units. |

### Global info

What a global says about itself besides its formula. Display only: none of it changes a
value or a total. Each field is left out at its default, and a global whose fields are all
at their default has no entry. An entry whose name is not in `globals` is ignored on read
and never written.

| Field          | Type | Notes |
|----------------|------|-------|
| `description`  | text | |
| `unit`         | text | A unit name from `units` (`"Week"`), or empty for none. A unit renamed in the application is renamed here too. |
| `decimals`     | integer | Decimal places the global's value shows with, `0` to `8` (rounded half away from zero, padded with zeros). Left out to show the value as calculated. Also used for cells whose formula names this global (see [display settings](#display-settings)). |
| `show_by_name` | `true` or `false` | Cells whose formula names this global show the formula instead of its value. Default `false`. |
| `folder`       | text | A folder (category) to file the global under, or empty for none. |

```json
"global_info": {
  "SHOOT_DAYS": { "description": "Principal photography", "unit": "Day", "decimals": 0, "folder": "Schedule" }
}
```

### Fringe

A tagged object:

| `kind`    | Fields | Meaning |
|-----------|--------|---------|
| `percent` | `description` text; `short_code` optional text; `rate` formula, **required**; `cutoff` optional cents | `rate` percent of each earner's pay (`"7.65"` is 7.65%), up to `cutoff` of pay when set. |
| `flat`    | `description` text; `short_code` optional text; `rate` formula, **required**; `unit` optional text; `basis` text; `max_hours_per_day` optional decimal; `cutoff` optional cents | `rate` (in base currency) per `unit`, per line: times the number of `unit`s the line counts, as `basis` says (below). With no `unit`, or when it or the line's unit has no hours, once per line. |

Fields of a flat fringe, in written order after `rate`:

| Field | Type | Notes |
|-------|------|-------|
| `unit` | optional text | The unit it is charged per. Not set: once per line. |
| `basis` | `"amount"`, `"amount_counts"` or `"quantity"` | What a line counts. Left out when `"amount"`, which is also what a missing `basis` means. `"amount"`: the line's Amount only, converted from the line's unit into `unit` through unit hours (Amount x line unit hours / fringe unit hours; the Amount itself when the line is in `unit`); X and 4X never count. `"amount_counts"`: the same, then times X when X's unit (`unit2`) is not a time unit (a blank unit counts as a count), and times 4X when `unit4` is not. `"quantity"`: Amount x X x 4X converted through hours (how flat fringes counted before this field, kept for budgets built that way). A unit "has hours" when its `hours` are set and not 0 (under `"quantity"` without a cap, 0 hours on the line's unit count 0, as before). |
| `max_hours_per_day` | optional decimal | Most hours a day counted, for a fringe charged per hour (its unit is 1 hour long, or named Hour). Not set: no cap. The line's days are its Amount converted to days through the budget's Day unit (named Day or Days, with hours); its hours a day are X when X is filled in and its unit is an hour unit ("25 Days x 14 Hours"), else the Day unit's hours; the count is days x the smaller of hours a day and the cap. Ignored for other units, when either unit has no hours, or when there is no Day unit with hours. |
| `cutoff` | optional cents | Kept, never used: cutoffs apply to percent fringes only. A fringe switched from percent to flat keeps its cutoff here, so it comes back if the fringe is switched back. |

A flat fringe is always charged per line, never per [fringe range](#detail-row); its dollars
are rounded to cents once per line, by the budget's [arithmetic](#arithmetic): from the
exact count, half away from zero, or from the count worked out in doubles, half up on the
double (see `docs/decisions.md`, entries 2, 54 and 55).

```json
"fringes": {
  "MEALS": { "kind": "flat", "rate": "25", "unit": "Day" },
  "PENSION_HR": { "kind": "flat", "rate": "1.25", "unit": "Hour", "basis": "amount_counts", "max_hours_per_day": "12" }
}
```

`short_code` (both kinds, and groups) is a short label such as `"FICA"` that the
application shows in the grid's fringes column instead of the name. Display only: lines
always refer to a fringe by its name, so renaming one never touches its code, and a code
is never read as a name. The application keeps codes unique within fringes (ignoring case),
different from every other fringe's name, without commas, and up to 16 characters; the
reader does not check.

### Group

| Field             | Type | Notes |
|-------------------|------|-------|
| `description`     | text | |
| `short_code`      | optional text | A short label the grid's groups column shows instead of the name, as for [fringes](#fringe). Display only. |
| `in_budget_total` | boolean, default `true` | When `false`, every line tagged with the group is shown but counts as zero. |
| `color`           | optional text | Display color for the text of tagged lines, written `"#rrggbb"`. A line's own `color` wins. Never affects totals. |

### Group rules

The three budget-wide group rules (`docs/decisions.md`, entry 57),
each `"include"` or `"exclude"`. A budget setting; a [version](#version) keeps its own.
Each rule is left out at its default, and the whole object is left out when every rule
is; a missing rule means its default.

| Field               | Default | Meaning |
|---------------------|---------|---------|
| `ungrouped_lines`   | `"include"` | Lines with no group. `"exclude"` leaves out every line and every adjustment row that has no group: shown, counted as zero, earning no fringes, so subtotal rows show only the lines left. |
| `conflict`          | `"exclude"` | A line in a group whose `in_budget_total` is `false` and in a group whose is `true`. `"exclude"` leaves it out; `"include"` counts it. A line whose groups all have `in_budget_total` `false` is left out either way. |
| `fringes_in_totals` | `"include"` | Whether group totals are shown with their lines' fringes. Display only: no total changes. |

### Set

| Field         | Type | Notes |
|---------------|------|-------|
| `description` | text | |

A set with no description is written as `{}`.

### Location

| Field         | Type |
|---------------|------|
| `description` | text |
| `city`        | text |
| `region`      | text |
| `country`     | text |

### Topsheet row

A tagged object:

| `kind`             | Fields | Meaning |
|--------------------|--------|---------|
| `category`         | `code` text, **required**; `name` text, **required**; `notes` list of [notes](#note); `accounts` list of [accounts](#account) | A budget category. |
| `production_total` | `name` text, **required** | Subtotals every category since the previous production total. The first one ends above-the-line; every later one ends a level of below-the-line. Categories after the last production total (all of them when there is none) are in neither above- nor below-the-line, only in the grand total. A production total has no `code`, `notes` or `accounts`. |

### Account

| Field     | Type | Notes |
|-----------|------|-------|
| `code`    | text, **required** | Account number, e.g. `"11-01"`. |
| `name`    | text, **required** | |
| `notes`   | list of [notes](#note) | Notes on the account itself, oldest first. Left out when empty. |
| `details` | list of [detail rows](#detail-row) | In display order. |

### Detail row

Fields in written order:

| Field          | Type | Notes |
|----------------|------|-------|
| `id`           | row id, **required** | Stable identity. Unique within a budget. |
| `kind`         | `"line"`, `"subtotal"`, `"adjustment"` or `"separator"` | Default `"line"`. |
| `description`  | text, **required** | Always written, even when empty. |
| `amount`       | formula | Blank counts as 0. On an adjustment row, the percentage as a fraction (`"0.1"` is 10%). |
| `unit`         | text | Primary unit name. Flat fringes convert the Amount from it through unit hours. |
| `x`            | formula | Multiplier. Blank counts as 1. |
| `unit2`        | text | Unit shown next to X. Flat fringes read it to tell a count from a time (see [fringe](#fringe)). |
| `rate`         | formula | Blank counts as 0. |
| `unit3`        | text | Unit shown next to the rate. |
| `x4`           | formula | Fourth factor. Blank counts as 1. |
| `unit4`        | text | Unit shown next to the fourth factor. Read by flat fringes as `unit2` is. |
| `currency`     | optional text | Currency code; not set means the base currency. |
| `fringe_range` | optional integer 0 to 4294967295 | Consecutive lines and adjustment rows with the same number share each percent fringe's cutoff as one earner, used up in row order: each row takes the percent of its total, or of what is left of the cutoff when that is less (nothing once it is used), rounded on its own. |
| `fringes`      | name set | Fringes applied to this line or adjustment row. |
| `groups`       | name set | Groups this line belongs to. |
| `set`          | optional text | Set name. |
| `location`     | optional text | Location name. |
| `bold`         | boolean, default `false` | Display style. |
| `italic`       | boolean, default `false` | Display style. |
| `indent`       | integer 0 to 255, default `0` | Indent level. The application uses 0 to 4. |
| `color`        | optional text | Text color, written `"#rrggbb"`. Overrides the color of the row's groups. |
| `notes`        | list of [notes](#note) | Dated notes on the row, oldest first. Left out when empty. |

A line costs `amount x X x rate x 4X`, worked out and rounded by the budget's
[arithmetic](#arithmetic). A subtotal row shows a running total and adds
nothing. An adjustment row's rate is the sum of the lines above it (back to the previous
adjustment row); it takes no `rate`, `unit3` or `currency`, but it can carry fringes,
charged on its total as on a line's. A separator row is a drawn rule line: it has no cost and is ignored by
all math. It is never counted or fringed, it doesn't break a fringe range, and it doesn't
reset a subtotal or adjustment window. Other fields on a separator row are kept but have
no effect.

`bold`, `italic`, `indent`, `color` and `notes` are display only and never affect totals.
They apply to every kind of row. The file stores them as given: the application's own
rules (colors are `#` plus 6 hex digits, indent 0 to 4, notes up to 10,000 characters)
are checked by the application, not by the file reader.

**Older files**: before dated notes, a row kept one plain note as the text field `note`.
A reader still accepts it (on detail rows and line block rows) and reads it as one undated
note at the front of the row's `notes`, with the id after the highest in `notes` (so `1`
when there is no list). A writer never writes `note`: once saved, the row has `notes`
only. Nothing else changed, so such a file is still format version 1.

### Note

A dated note on a detail row, an account or a category. Display only. Fields in written
order:

| Field     | Type | Notes |
|-----------|------|-------|
| `id`      | integer, **required** | Unsigned 64-bit integer. The application keeps ids unique within the list the note is in and gives a new note the id after the highest there; the reader does not check. |
| `created` | text | When it was written, as an ISO 8601 date-time such as `"2026-10-09T14:30:00Z"`, as the application gave it. Left out when unknown (a note read from an older file's `note`). |
| `edited`  | optional text | When its text last changed, the same way. |
| `author`  | optional text | Who wrote it: free text from the application's local settings (there are no accounts). |
| `done`    | boolean, default `false` | Ticked off in the notes review list. |
| `text`    | text, **required** | Plain text. The application allows up to 10,000 characters. |

```json
"notes": [
  { "id": 1, "created": "2026-10-09T14:30:00Z", "author": "Alex", "text": "Check the rate with the union" },
  { "id": 2, "created": "2026-10-10T09:05:00Z", "edited": "2026-10-10T09:20:00Z", "done": true, "text": "Kit fee agreed" }
]
```

### Charge

A tagged object. Both kinds share these fields:

| Field                 | Type | Notes |
|-----------------------|------|-------|
| `name`                | text, **required** | |
| `account`             | optional text | Account the charge posts to. |
| `applied`             | boolean, default `true` | Only applied charges count. |
| `excluded_amounts`    | list of `{"description": text, "amount": cents}`, both **required** | Manual exclusions, in order. |
| `excluded_groups`     | name set | |
| `excluded_categories` | name set | Category codes. |
| `excluded_accounts`   | name set | Account codes. |
| `label_percent`       | boolean, default `false` | Show the percent after the name in the topsheet label (percent charges). Display only. Written last, left out when `false`. |
| `label_excluded`      | boolean, default `false` | Show the excluded total after the name in the topsheet label. Display only. Written after `label_percent`, left out when `false`. |

Kind-specific fields (written right after `applied`):

| `kind`    | Fields | Meaning |
|-----------|--------|---------|
| `percent` | `percent` decimal, **required** | Percent of the grand total before charges (every level, with its fringes wherever they post, plus fringes posted by budget), less exclusions (`"10"` is 10%). |
| `flat`    | `amount` cents, **required**; `currency` optional text | A fixed fee, less exclusions. |

Exclusions, for both kinds: the excluded amounts, and every line or adjustment row in an
excluded group, category or account, with its fringes (each row once, however many rules
catch it).

### Credit

| Field              | Type | Notes |
|--------------------|------|-------|
| `name`             | text, **required** | |
| `account`          | optional text | |
| `applied`          | boolean, default `true` | |
| `cap`              | optional cents | Ceiling on the sum of the row credits. |
| `discount_percent` | decimal, default `"0"` | Taken off after the cap (`"5"` is 5%). |
| `excluded_fringes` | name set, default empty | Fringes left out of the spend of every `group`, `category` and `account` row (one list per credit; `docs/decisions.md`, entries 57 and 61). A typed `flat` amount has no fringes, so it is unaffected. |
| `rows`             | list of [credit rows](#credit-row) | |

### Credit row

| Field         | Type | Notes |
|---------------|------|-------|
| `description` | text, **required** | |
| `source`      | tagged object, **required** | See below. |
| `percent`     | decimal, **required** | Percent of the row's spend (`"25"` is 25%). |
| `cap`         | tagged object | See below. Left out means no cap. |

`source` kinds:

| `kind`  | Fields | Meaning |
|---------|--------|---------|
| `flat`  | `amount` cents, **required** | A gross amount typed in directly. |
| `group` | `group` text, **required** | Lines tagged with the group, plus their fringes except the credit's `excluded_fringes`. |
| `category` | `category` text, **required** | The topsheet category with that code: its total (lines and adjustment rows that count) plus the fringes they earn, wherever fringes post, except the credit's `excluded_fringes` (`docs/decisions.md`, entry 57). A code that names no category counts 0, and `method-budget check` warns. |
| `account` | `account` text, **required** | The account with that number (the first, if two share it): its lines and adjustment rows that count plus the fringes they earn, wherever fringes post, except the credit's `excluded_fringes` (`docs/decisions.md`, entry 61). A number that names no account counts 0, and `method-budget check` warns. |

Older files put an `excluded_fringes` name set on each `group` and `category` source. A
reader still accepts it there and gives the credit the union of its rows' lists; a writer
writes the list on the credit only.

`cap` kinds:

| `kind`            | Fields | Meaning |
|-------------------|--------|---------|
| `qualified_spend` | `amount` cents, **required** | Caps the spend the percent applies to. |
| `credit`          | `amount` cents, **required** | Caps the resulting credit. |

### Baseline

Totals frozen at some point (for example the original approved budget), so the current
budget can show what changed. Rows are matched by id, accounts and categories by code.

| Field         | Type | Notes |
|---------------|------|-------|
| `rows`        | map of cents | Keyed by row id written as a decimal string (`"17"`), because JSON object keys are strings. Ordered numerically. |
| `accounts`    | map of cents | Keyed by account code. |
| `categories`  | map of cents | Keyed by category code. |
| `grand_total` | cents, **required** | |
| `net_total`   | cents, **required** | |
| `frozen`      | boolean, default `false` | The original totals are frozen: the application refuses to lock them again or clear them until this is turned off (`docs/decisions.md`, entry 61). Left out when `false`. |

### Actuals

Actual costs brought in from an accounting system (payments, invoices, journal lines), the
imports they came in with, and per-account estimate-to-complete overrides, for the cost
report (docs/decisions.md, entry 45). About the production's spending, not the budget's
content: a [version](#version) never has them (a reader rejects one that does), and
restoring a version keeps the current ones. Fields in written order, each left out when
empty; `actuals` is left out when all three are:

| Field          | Type | Notes |
|----------------|------|-------|
| `batches`      | list of [import batches](#import-batch) | Oldest first. |
| `transactions` | list of [transactions](#transaction) | In the order they came in. |
| `etc`          | map of [ETC methods](#etc-method) | Keyed by account number (an account in the chart, or a category fringe account when fringes post by category). An account without one uses budget minus actual. |

#### Import batch

One import, so it can be taken back out whole.

| Field      | Type | Notes |
|------------|------|-------|
| `id`       | integer, **required** | Unsigned 64-bit. The application keeps ids unique; the reader does not check. |
| `file`     | text | The imported file's name (never its path). |
| `imported` | text | When, as an ISO 8601 date-time such as `"2026-10-09T14:30:00-07:00"`. Stored as given. |
| `rows`     | integer, **required** | How many transactions it brought in. |

#### Transaction

| Field         | Type | Notes |
|---------------|------|-------|
| `id`          | integer, **required** | Unsigned 64-bit. The application keeps ids unique (it edits and deletes by id); the reader does not check. |
| `date`        | text | `YYYY-MM-DD`, or left out when unknown. Stored as given (the application writes real dates only). |
| `account`     | text | The account number as the budget's pattern writes it, or left out for none. A number the chart doesn't have is kept (the transaction counts as unassigned until the chart has it). |
| `line`        | optional row id | The detail row it pays for. |
| `tags`        | name set | Free-form labels. |
| `vendor`      | text | |
| `description` | text | |
| `reference`   | text | Invoice, purchase order or check number. |
| `amount`      | cents, **required** | Always written. Positive is a cost; a refund or credit is negative. |
| `batch`       | optional integer | The import it came in with; left out for one entered by hand. |

#### ETC method

A tagged object (`kind` first):

| `kind`                | Fields | Meaning |
|-----------------------|--------|---------|
| `budget_minus_actual` | none | What is left of the budget, never past zero (the same as no entry). |
| `manual`              | `amount` cents, **required** | A typed estimate to complete. |
| `percent_complete`    | `percent` decimal, **required** | The account is this far along (more than 0, at most 100), so it ends at actual / percent, rounded once to the cent. A reader keeps any value; the application treats one out of range as budget minus actual. |

```json
"actuals": {
  "batches": [{ "id": 1, "file": "september.csv", "imported": "2026-10-09T14:30:00-07:00", "rows": 2 }],
  "transactions": [
    { "id": 1, "date": "2026-09-01", "account": "10-01", "line": 1, "tags": ["PO"], "vendor": "Writer", "description": "First draft", "reference": "INV-1", "amount": 2500000, "batch": 1 },
    { "id": 2, "account": "77-01", "amount": -1234, "batch": 1 }
  ],
  "etc": { "11-01": { "kind": "percent_complete", "percent": "37.5" }, "12-01": { "kind": "manual", "amount": 10000 } }
}
```

### Display settings

How the budget shows numbers on screen, saved with it. Display only: no stored value or
total changes, and the engine keeps every value exact. Like presets, these are tools, not
content: a [version](#version) never holds them, and restoring one keeps the current
settings. Each field is left out at its default; the whole object is left out when every
field is.

| Field                  | Type | Notes |
|------------------------|------|-------|
| `rate_cutoff`          | optional decimal | Rates smaller than this (by size) use `rate_decimals_below`, the rest `rate_decimals_above`. Left out for no cutoff: every rate uses `rate_decimals_above`. |
| `rate_decimals_below`  | optional integer | Decimal places, `0` to `8`, for rates below the cutoff. Left out to show them as typed. |
| `rate_decimals_above`  | optional integer | Decimal places, `0` to `8`, for the other rates. Left out to show them as typed. |
| `rate_global_decimals` | `true` or `false` | A rate whose formula names a global with its own `decimals` (the first such global) shows with those instead. Default `true`. |
| `summary_rows`         | `true` or `false` | The topsheet shows the above-the-line, below-the-line and combined totals. Default `true`. |
| `net_total`            | `true` or `false` | The topsheet shows the net total (after credits). Default `true`. |

```json
"display": { "rate_cutoff": "10", "rate_decimals_below": 4, "rate_decimals_above": 2, "summary_rows": false }
```

### Provenance

Where a budget came from: stamped on a copy saved for sharing, a sub-budget and a setup
pack. It is about the file, not its content: a [version](#version) never has one, and
restoring a version keeps the current one. It names its source by title only, never by path. Fields
in written order:

| Field            | Type | Notes |
|------------------|------|-------|
| `kind`           | text, **required** | `"shared_copy"` (saved for sharing; Method Budget opens it read-only), `"editable_copy"` (an editable copy made from a shared copy), `"sub_budget"` (a filtered copy of another budget), or `"setup_pack"` (a [setup pack](#setup-packs)). |
| `source_title`   | text, **required** | The title of the budget it was made from. |
| `source_version` | optional text | The name of the saved version it was made from, when it wasn't the budget as it stood. |
| `saved`          | text, **required** | When it was made, as an ISO 8601 date-time such as `"2026-10-09T14:30:00-07:00"`. Stored as given; the reader does not check it. |
| `app_version`    | text, **required** | The version of the program that made it. |
| `note`           | optional text | A note from whoever made it; for a sub-budget, what it keeps (`"Categories 10, 20; groups Night; without fringes"`). |
| `pack`           | [pack details](#setup-packs) | A setup pack's details. Only with `"setup_pack"`; left out otherwise. |

Read-only is a request, not a lock: any program may edit the file, and Method Budget
offers an editable copy instead.

```json
"provenance": {
  "kind": "shared_copy",
  "source_title": "Short Film",
  "source_version": "Studio draft",
  "saved": "2026-10-09T14:30:00-07:00",
  "app_version": "0.0.1",
  "note": "For the studio's review"
}
```

### Setup order

The order the application shows each setup table in, first to last, when it isn't name
order: an object with any of `units`, `globals`, `fringes`, `groups`, `sets`, `locations`
and `currencies` (foreign currencies; the base currency is not in the table), each a list
of names. A table shows its listed names that it still has, each once and in that order,
then any other names in name order (so an item added later comes last). A writer lists
every name of a table in its shown order, leaves a table out when that is name order, and
leaves `setup_order` out when every table is. Display only: reports, the Apply lists and
the type-ahead follow it; nothing else depends on it. Layout, not content: a
[version](#version) never has one, and restoring a version keeps the current order.

```json
"setup_order": { "fringes": ["PAYROLL", "PENSION", "MEALS"] }
```

### Setup folders

Named folders of setup items, one set of folders per kind: an object whose keys are kinds
(`global`, `fringe`, `group`, `unit`, `set`, `location`, `currency`, `charge`, `credit`,
`tag_preset`, `line_preset`, `report_preset`, `ot_pattern`), each an object of folder names to lists of
item names (exact names: a charge, credit or preset by its `name`). An item is in at most
one folder of its kind; an item in none is unfiled. A folder may be empty. A folder name is
1 to 60 characters with no control characters, unique within its kind ignoring case (the
application's rule). A reader takes the object as is; a writer keeps only the known kinds,
only names that exist (each in its first folder), keeps empty folders, leaves out a kind
with no folders, and leaves `setup_folders` out when no kind has one. Display only: no
amount or assignment depends on it. Layout, not content: a [version](#version) never has
one, and saving or restoring a version keeps the current folders.

The personal setup library (docs/APP.md, "Setup library") is an ordinary budget file with
setup, no `topsheet`, and usually `setup_folders`; nothing else marks it as a library.

```json
"setup_folders": { "fringe": { "Payroll": ["FICA", "FUI"], "Union": [] } }
```

### Setup sources

Where setup items came from: an object whose keys are kinds (as for
[setup folders](#setup-folders)), each an object of item names (exact names) to a source.
Importing a [setup pack](#setup-packs) stamps every item it brings with the pack's title,
version and effective date; a pack's maker can give any item notes and links of its own;
copying an item between budgets (Import Setup, the library, packs) brings its source along,
and replacing an item with another's version replaces its source too. A source's fields,
in written order, each left out when empty:

| Field          | Type | Notes |
|----------------|------|-------|
| `pack`         | text | The title of the setup pack it was imported from. |
| `pack_version` | text | That pack's `version`. |
| `effective`    | text | When its figure takes effect, `YYYY-MM-DD` (the item's own, else the pack's). |
| `notes`        | text | Where its figure comes from, in words. |
| `urls`         | list of text | Links to its sources, each `http://` or `https://`. Shown as text; never fetched. |

A reader takes the object as is (checking nothing but the shape); a writer keeps only
known kinds' items that exist and sources that say something, and leaves `setup_sources`
out when none is left. About the items, not content: no amount depends on it, a
[version](#version) never has one, and saving or restoring a version keeps the current
sources. Renaming an item everywhere renames its source's key.

```json
"setup_sources": {
  "fringe": {
    "PENSION": { "pack": "Example Rates", "pack_version": "2026.1", "effective": "2026-01-01", "notes": "Scale sheet, page 4" }
  }
}
```

### Setup packs

A setup pack is an ordinary budget file made to share setup items: setup and no
`topsheet`, a [`provenance`](#provenance) whose `kind` is `"setup_pack"` (its
`source_title` names the budget it was made from and `saved` when), and the pack's details
in the provenance's `pack`. It usually has [setup folders](#setup-folders) (the maker's)
and [setup sources](#setup-sources) (per-item notes and links). A pack holds whatever
setup items its maker picked plus everything they need (a fringe's globals and flat unit,
a global's globals, a charge's excluded groups and currency, a credit's groups and
excluded fringes, a tag preset's tags, what a line block's rows use, a report preset's
groups and header and footer globals), and never the budget settings Import Setup carries
(`account_coding`, `fringe_posting`, `fringe_row`, `cutoff_factor`, `new_line_defaults`),
which belong to a production.
It opens in Method Budget like any budget. The pack details, in written order, each left
out when empty but the title:

| Field         | Type | Notes |
|---------------|------|-------|
| `title`       | text, **required** | 1 to 200 characters, one line. |
| `description` | text | Up to 4,000 characters. |
| `author`      | text | Who made it: free text. |
| `version`     | text | The pack's own version, as its maker writes it. |
| `effective`   | text | When its figures take effect, `YYYY-MM-DD`. |
| `region`      | text | The region or jurisdiction its figures apply to. |
| `notes`       | text | Where its figures come from, in words. Up to 4,000 characters. |
| `urls`        | list of text | Up to 20 links to its sources, each `http://` or `https://`. Shown as text; never fetched. |

The limits are the application's (the reader checks only the shape). Method Budget ships
no rate packs: they come from a separate data project after review (CLEAN-ROOM.md).

```json
"provenance": {
  "kind": "setup_pack",
  "source_title": "Payroll Desk Setup",
  "saved": "2026-10-10T09:00:00-07:00",
  "app_version": "0.1.0",
  "pack": {
    "title": "Example Rates",
    "author": "Payroll desk",
    "version": "2026.1",
    "effective": "2026-01-01",
    "region": "Example region",
    "urls": ["https://example.org/scale"]
  }
}
```

### Version

A named, full copy of the budget's content, saved inside the budget so it can be restored
or compared later. Fields in written order:

| Field     | Type | Notes |
|-----------|------|-------|
| `id`      | integer, **required** | Unsigned 64-bit integer. The application keeps ids unique within the budget's `versions`; the reader does not check. |
| `name`    | text, **required** | |
| `created` | text, **required** | When the version was saved, as an ISO 8601 date-time such as `"2026-10-09T14:30:00Z"`. Stored as given; the reader does not check it. |
| `note`    | optional text | |
| `budget`  | object | A [budget](#budget) object with every field except `versions`, `compact_versions`, `report_presets`, `tag_presets`, `line_presets`, `ot_patterns`, `display`, `setup_order`, `setup_folders`, `setup_sources`, `provenance` and `actuals`: a version cannot contain versions, presets, overtime patterns, display settings, a setup order, setup folders, setup sources, a provenance or actuals, and a reader rejects one that does. |
| `delta`   | object | Instead of `budget`: the version written as changes ([version deltas](#version-deltas)). |

Exactly one of `budget` and `delta` is required.

Restoring a version in the application first saves the current content as another version
(named `Before restoring "<name>"`), so a restore can be undone.

**File size.** Written in full, each version is a complete copy of the budget's content
(every category, account and row), so a file with N versions is roughly N + 1 times the
size of the budget alone: simple, self-contained, and readable by every version 1
reader. A budget with `compact_versions` on writes versions as
[deltas](#version-deltas) instead, which costs about one copy plus what changed. Row ids inside a version are the ids the rows had when it was saved, which
is what lets the application match rows between a version and the current budget.

Example (inside `budget`, after `baseline`), a version written in full:

```json
"versions": [
  {
    "id": 1,
    "name": "Studio draft",
    "created": "2026-10-09T14:30:00Z",
    "note": "Sent to the studio",
    "budget": {
      "title": "Short Film",
      "base_currency": "USD",
      "topsheet": [
        {
          "kind": "category",
          "code": "10",
          "name": "Above-the-line",
          "accounts": [
            {
              "code": "10-01",
              "name": "Director",
              "details": [
                {
                  "id": 1,
                  "description": "Director",
                  "amount": "1",
                  "unit": "Flat",
                  "rate": "4500"
                }
              ]
            }
          ]
        }
      ]
    }
  }
]
```

### Version deltas

With the budget's `compact_versions` set to `true`, a writer may write a version as a
`delta` instead of a full `budget`, so that many versions don't make the file many times
bigger. Loading gives exactly the same versions either way: a reader rebuilds every
version's full budget, and a delta is purely a way of writing it. The setting is off by
default, and a file with it off is unchanged (see [Versioning](#versioning)); a reader
that predates deltas rejects a file that uses them, as it rejects any field it doesn't
know, rather than misreading it.

A delta is an object with two fields, both **required**:

| Field   | Type | Notes |
|---------|------|-------|
| `base`  | `"budget"` or `"next"` | What the changes apply to. `"budget"`: the budget's own content. `"next"`: the version right after this one in the list. |
| `patch` | list of operations | A JSON Patch (RFC 6902) that turns the base into this version's budget. |

The **base** is always a budget object exactly as the reference writer writes it, in
canonical form: for `"budget"`, the file's top-level budget with every field a version
can't have left out (`versions`, `compact_versions`, the presets, `display`,
`setup_order`, `setup_folders`, `setup_sources`, `provenance`, `actuals`); for `"next"`, the next
version's budget (rebuilt first if it is a delta too). The newest version's base must be
`"budget"`. The reference writer gives the newest version `"budget"` and every older one
`"next"`, so each delta holds what changed between neighbors, and it writes a version in
full wherever a delta wouldn't be smaller.

Each **operation** is an object, fields in written order:

| Field   | Type | Notes |
|---------|------|-------|
| `op`    | `"add"`, `"remove"` or `"replace"`, **required** | As RFC 6902 defines them. (`move`, `copy` and `test` are not used and are rejected.) |
| `path`  | text, **required** | A JSON Pointer (RFC 6901) into the budget object: `/title`, `/topsheet/2/accounts/0/details/3/rate`. `~1` is `/` and `~0` is `~` in a name. |
| `value` | any JSON value | **Required** for `add` and `replace`, not allowed for `remove`. |

Operations apply in order, each to the result of the ones before it. An array index in a
path must exist (`add` may also use the length, or `-`, to append). The patched object
is then read like a full version's `budget`, with every rule above: unknown fields, a
version's forbidden fields, bad decimals and so on are errors. A delta that doesn't fit
its base (a path that isn't there, a remove with a value) is an error naming the version,
such as `budget.versions[3] ("Studio draft"): patch operation 0: ...`; nothing is
guessed.

```json
"compact_versions": true,
"versions": [
  {
    "id": 1,
    "name": "Studio draft",
    "created": "2026-10-09T14:30:00Z",
    "delta": {
      "base": "next",
      "patch": [
        { "op": "replace", "path": "/topsheet/0/accounts/0/details/0/rate", "value": "4500" }
      ]
    }
  },
  {
    "id": 2,
    "name": "Locked",
    "created": "2026-10-12T09:00:00Z",
    "delta": {
      "base": "budget",
      "patch": [
        { "op": "replace", "path": "/title", "value": "Short Film (locked)" }
      ]
    }
  }
]
```

### Report preset

A saved report setup ("preset"): which sections print and in what order, which categories
and accounts, what to leave out, the detail columns, the page layout, and the header and
footer. It holds no figures; a report is always drawn from the budget as it is. Presets are
layout, not budget content: a [version](#version) never has them (a reader rejects one
that does), and restoring a version keeps the current ones. Fields in written order:

| Field | Type | Notes |
|-------|------|-------|
| `name` | text, **required** | The application keeps names unique (ignoring case and surrounding spaces), 1 to 80 characters; the reader does not check. |
| `default` | boolean, default `false` | The preset the application's Print dialog opens with. The application keeps at most one; a reader that finds several uses the first. |
| `sections` | list of section names | In print order, each once: `"topsheet"`, `"accounts"` (each category's accounts with their totals), `"detail"`, `"fringes"`, `"globals"`, `"groups"` (with their totals), `"notes"` (every note on categories, accounts and lines, each with its date and author), `"info"` (budget information), `"chart"` (chart of accounts: every category and account code with its name), `"units"`, `"sets"` (with their totals), `"locations"` (with their totals), `"currencies"`, `"cost_report"` (budget, baseline, actual, estimate to complete, estimate at completion, variance and percent spent per account, category and the topsheet; see [actuals](#actuals)). |
| `excluded_categories` | name set | Category codes left out of the account and detail sections and the notes list. The topsheet always lists every category. |
| `excluded_accounts` | name set | Account codes left out the same way. |
| `skip_empty_accounts` | boolean, default `false` | Leave out accounts with no rows. |
| `skip_zero_accounts` | boolean, default `false` | Leave out accounts whose total is zero. |
| `skip_suppressed_lines` | boolean, default `false` | Leave out lines that don't count in the budget total (a group on them is out of it). |
| `skip_notes` | boolean, default `false` | Leave out the notes printed under detail lines (the notes section still lists them). |
| `skip_done_notes` | boolean, default `false` | Leave out notes ticked off as reviewed, under lines and in the notes section. |
| `only_groups` | name set | When not empty, detail pages print only the lines tagged with at least one of these groups. |
| `columns` | optional list of column names | The optional detail columns in print order: `"amount"`, `"unit"`, `"x"`, `"unit2"`, `"rate"`, `"unit3"`, `"currency"`, `"x4"`, `"unit4"`, `"fringes"`, `"fringe_pct"`, `"fringe_total"`, `"groups"`, `"set"`, `"location"`, `"original"`, `"variance"` (the last two only print when the budget has a baseline, then on the topsheet and account level too). Left out means the standard set; `[]` means none (the line number, description and total always print). |
| `landscape` | boolean, default `false` | Page orientation. |
| `font_size` | text, default `"medium"` | `"small"`, `"medium"` or `"large"`. |
| `shading` | boolean, default `false` | Alternate lines tinted, total rows darker. |
| `group_colors` | boolean, default `false` | Lines print in their own or their groups' colors (otherwise black). |
| `vertical_rules` | boolean, default `false` | Thin lines between the columns of every table. |
| `collapse_details` | boolean, default `false` | The detail section prints each account as one line (code, name, total, and original and variance with a baseline) instead of its rows; categories then run on without starting a page each. |
| `topsheet_fringes` | boolean, default `false` | The topsheet gets a Fringes column: what each category's lines earn in fringes, wherever they post (and each production total's sum of them). |
| `topsheet_pages` | boolean, default `false` | The topsheet gets a Page column: the page of the printout where each category's detail starts (blank when it doesn't print). |
| `header` | optional [band](#band) | Left out means the standard header (budget title, section, date printed); `{}` means none. |
| `footer` | optional [band](#band) | Left out means the standard footer (page number at the right); `{}` means none. |

Unknown section or column names are errors, like unknown fields.

#### Band

A header or footer: three optional slots, `left`, `center` and `right`, each a tagged
object saying what it shows, worked out when the report is drawn, and
`first_page_only` (boolean, default `false`): when true the band prints on the first page
only, and later pages have none. Slots:

| `kind` | Fields | Shows |
|--------|--------|-------|
| `title` | | The budget's title. |
| `section` | | The section on the page (the topsheet, a category's detail, ...). |
| `printed` | | The date and time the report was drawn. |
| `date` | | The date the report was drawn. |
| `time` | | The time of day the report was drawn. |
| `file` | | The budget's file name. |
| `page` | | "Page 3 of 12". |
| `info` | `label` text, **required** | The budget information field with that label, if there is one. |
| `global` | `name` text, **required** | That global's value, if the budget has it. Renaming the global renames it here. |
| `text` | `text` text, **required** | The text as written (the application allows up to 200 characters). |

Example (inside `budget`, after `baseline` and before `versions`):

```json
"report_presets": [
  {
    "name": "Studio pack",
    "default": true,
    "sections": ["topsheet", "accounts", "detail", "notes"],
    "excluded_categories": ["90"],
    "skip_zero_accounts": true,
    "columns": ["amount", "unit", "rate", "fringes", "original", "variance"],
    "landscape": true,
    "shading": true,
    "footer": {
      "left": { "kind": "info", "label": "Shoot days" },
      "right": { "kind": "page" }
    }
  }
]
```

### Tag preset

A named bundle of a line's tags, put on lines by the application in one of two ways: added
(fringes and groups joined with the line's own; set, location and currency filled in only
where the line has none) or replacing (the line's five tags become exactly the preset's,
an empty part clearing that tag). Like report presets, tag presets are tools, not budget
content: a [version](#version) never has them, and restoring a version keeps the current
ones. Fields in written order, each part left out when empty:

| Field      | Type | Notes |
|------------|------|-------|
| `name`     | text, **required** | The application keeps names unique within the list, ignoring case, and up to 80 characters; the reader does not check. |
| `fringes`  | name set | Fringe names. |
| `groups`   | name set | Group names. |
| `set`      | optional text | A set name. |
| `location` | optional text | A location name. |
| `currency` | optional text | A currency code. Left out means the base currency. |

The names refer to the budget's own setup tables; the reader does not check that they
exist (the application refuses to apply a preset that names a missing item).

```json
"tag_presets": [
  { "name": "Union crew", "fringes": ["PAYROLL", "PENSION"], "groups": ["Crew"], "location": "Toronto" }
]
```

### Line block

A named block of rows the application inserts copies of. Tools, not budget content, like
tag presets: a [version](#version) never has them. Fields in written order:

| Field   | Type | Notes |
|---------|------|-------|
| `name`  | text, **required** | Unique within the list, ignoring case, as for tag presets. |
| `rows`  | list of block rows | The rows in order. Left out when empty. |
| `setup` | object | The setup items the rows use, as defined when the block was saved, with what those use in turn. Left out when empty. |

A **block row** is a [detail row](#detail-row) without `id`: inserted copies get new ids.
Every other field is the same, with the same defaults; `fringe_range` numbers group the
block's own rows and are renumbered for the account a copy lands in.

`setup` holds the budget's own tables, each left out when empty, in this order:
`currencies`, `units`, `globals`, `fringes`, `groups`, `sets`, `locations`, each a map in
the shape the [budget](#budget) uses. The application uses it only to add back an item the
budget no longer has when the block is inserted; the budget's own definition of an item
always wins.

```json
"line_presets": [
  {
    "name": "Prep week",
    "rows": [
      { "description": "Coordinator", "amount": "5", "unit": "Day", "rate": "DAY_RATE", "fringes": ["PAYROLL"] },
      { "kind": "subtotal", "description": "Prep" }
    ],
    "setup": {
      "globals": { "DAY_RATE": "450" },
      "fringes": { "PAYROLL": { "kind": "percent", "rate": "18" } }
    }
  }
]
```

### Overtime pattern

A named overtime pattern for the overtime helper: straight time, then overtime bands, and
optional sixth and seventh day multipliers. Every number is the user's; nothing about any
union or guild is built in. Tools, not budget content, like tag presets: a
[version](#version) never has them, and restoring a version keeps the current ones.
Fields in written order:

| Field            | Type | Notes |
|------------------|------|-------|
| `name`           | text, **required** | 1 to 80 characters, no spaces at either end. The application keeps names unique within the list. |
| `straight_hours` | decimal, **required** | Hours a day paid at straight time before overtime starts, 0 to 24. |
| `bands`          | list of bands | The overtime bands after the straight time, in order. Left out when there are none (every hour is straight time). |
| `sixth_day`      | optional decimal | On a sixth day worked in a week every hour is paid at least this multiple (a band above it still wins). More than 0, at most 10. Left out means a sixth day is a regular day. |
| `seventh_day`    | optional decimal | The same for a seventh day. |

A **band** has `multiplier` (decimal, more than 0 and at most 10) and `hours` (decimal,
more than 0 and at most 24): how many hours, after the straight time and the bands before
it, are paid at that multiple of the straight rate. The last band leaves out `hours`: it
covers every hour left. Every other band must have it. A reader rejects a pattern that
breaks these rules.

```json
"ot_patterns": [
  {
    "name": "Crew 12s",
    "straight_hours": "8",
    "bands": [{ "multiplier": "1.5", "hours": "4" }, { "multiplier": "2" }],
    "sixth_day": "1.5"
  }
]
```

That is "8 hours straight time, then 1.5x after 8, 2x after 12; sixth day 1.5x".

## Reading rules

A reader that follows this document should:

1. Parse the JSON. Reject trailing content after the top-level object.
2. Check `format` is `"method-budget"`, then check `version`:
   reject a missing or non-integer version, and reject a version newer than the reader
   supports, with a message saying the file needs a newer program. Do this before checking other fields,
   so a newer file gets the version message.
3. Reject unknown fields anywhere, and fields that do not belong to the given `kind`.
4. Reject JSON numbers where a decimal string is expected, and floats where cents or
   integers are expected.
5. Treat a repeated field in one object as an error. For maps, a repeated key keeps the
   last value. For name sets, order and duplicates in the file do not matter.

The reference reader reports each problem with its line, column and a path such as
`budget.topsheet[2].accounts[0].details[3].rate`.

## Writing rules

A writer that wants byte-identical output to the reference writer should follow the key
order in this document, sort map keys and name sets by byte value, leave out every field
at its default, write decimals in canonical form, use 2-space indentation with `": "`
after keys, and end the file with one newline. Strings use standard JSON escaping;
non-ASCII characters are written as UTF-8, not escaped.

Other tools do not have to match byte for byte: any file that follows the reading rules is
valid. Loading and saving it in Method Budget normalizes it.

## Versioning

- The `version` integer changes only for **breaking** changes: removing or renaming a
  field, changing a field's type or meaning, or changing a default.
- **Additive** changes keep the version: a new optional field whose default preserves the
  old meaning, or a new `kind`. Old readers will reject a file that uses the new field or
  kind (they are strict), so a program that writes a new optional field must leave it out
  whenever it is at its default, which keeps simple files readable by older versions.
- Readers accept every version from 1 up to the newest they know, and reject newer
  versions with a clear "update the program" message rather than guessing.
- Every version change is recorded in the changelog below.

### Changelog

- **Version 1** (2026-10): first release.
- Version 1, additive (2026-10): group `color`; detail row `bold`, `italic`, `indent`,
  `color` and `note`; detail row kind `"separator"`. All are left out at their default,
  so files that don't use them are unchanged.
- Version 1, additive (2026-10): budget `versions` (named full copies of the budget's
  content). Left out when there are none, so files without versions are unchanged.
- Version 1, additive (2026-10): budget `new_line_defaults` (what an inserted line copies
  from its neighbor). Left out at its default, so files that don't change it are
  unchanged.
- Version 1, additive (2026-10): budget `report_presets` (saved report setups). Left out
  when there are none, so files without presets are unchanged.
- Version 1, additive (2026-10): budget `tag_presets` (saved bundles of line tags) and
  `line_presets` (saved blocks of rows, with the setup they use). Left out when there are
  none, so files without them are unchanged.
- Version 1, additive (2026-10): report preset sections `"chart"`, `"units"`, `"sets"`,
  `"locations"` and `"currencies"`; preset `vertical_rules`, `collapse_details`,
  `topsheet_fringes` and `topsheet_pages`; band `first_page_only`; band slot kinds
  `"date"`, `"time"` and `"global"`. Each is left out at its default, so presets that
  don't use them are unchanged.
- Version 1, additive (2026-10): budget `global_info` (a global's description, unit,
  decimals, show-by-name switch and folder) and `display` (rate decimals with a cutoff,
  topsheet summary rows and net total). Both are display only and left out at their
  default, so files that don't use them are unchanged.
- Version 1, additive (2026-10): dated [notes](#note) (`notes` on detail rows, line block
  rows, accounts and categories), replacing the single `note` text on a row, which a
  reader still accepts as one undated note and a writer no longer writes; fringe and group
  `short_code`; budget `setup_order`; report preset `skip_done_notes`. All are left out at
  their default, so files that don't use them are unchanged, and a file with an old
  `note` changes only when it is saved again.
- Version 1, additive (2026-10): charge `label_percent` and `label_excluded` (what the
  topsheet label shows besides the name). Display only and left out when `false`, so files
  that don't use them are unchanged.
- Version 1, additive (2026-10): budget `setup_folders` (folders of setup items per kind,
  used by the personal setup library). Left out when no kind has a folder, so files that
  don't use them are unchanged.
- Version 1, additive (2026-10): budget [`provenance`](#provenance) (where a shared copy or
  a sub-budget came from). Left out when there is none, so other files are unchanged.
- Version 1, additive (2026-10): budget `compact_versions` and version `delta`
  ([version deltas](#version-deltas): versions written as JSON Patches against the
  budget or the next version). Off by default and left out then, so files that don't
  turn it on are unchanged; a version's `budget` is no longer required when it has a
  `delta`.
- Version 1, additive (2026-10): budget [`ot_patterns`](#overtime-pattern) (saved overtime
  patterns). Left out when there are none, so files without them are unchanged.
- Version 1, additive (2026-10): budget [`actuals`](#actuals) (transactions, import
  batches and estimate-to-complete methods) and report preset section `"cost_report"`.
  Left out when there are none, so files without them are unchanged.
- Version 1, additive (2026-10): [setup packs](#setup-packs): provenance kind
  `"setup_pack"` and provenance `pack` (a pack's details), budget
  [`setup_sources`](#setup-sources) (where setup items came from), and setup folders for
  `ot_pattern`. Each is left out when empty, so files that don't use them are unchanged.
- Version 1, additive (2026-10): flat fringe `basis`, `max_hours_per_day` and `cutoff`
  (kept, never used). Each is left out at its default, so files that don't use them are
  unchanged. The engine's count changed with them (decisions.md, entry 54): a flat
  fringe without a `basis` now counts the Amount only, where it counted Amount x X x 4X
  before, so a flat fringe on a line with X or 4X filled in totals differently; give it
  `"basis": "quantity"` to keep the old figure.
- Version 1, additive (2026-10): budget [`arithmetic`](#arithmetic) (`"binary"` or
  `"exact"`), left out at `"binary"` (`"mmb"`, its earlier name, is still read). The
  engine changed with it (decisions.md, entry 55): a file without the field is now
  worked out in binary (spreadsheet) arithmetic, where it was exact
  before, so a figure that lands on or near a half cent can move by a cent (`1 x 1.005`
  is 1.00, not 1.01); give it `"arithmetic": "exact"` to keep the old figures. Formulas
  may now hold numbers with more than 9 decimal places.
- Version 1 (2026-10): credit `excluded_fringes`, one list per credit for all its group
  and category rows (decisions.md, entry 57). Left out when empty. The per-row
  `excluded_fringes` on a `group` or `category` source is still read, and the credit takes
  the union of its rows' lists, but it is no longer written, so a file whose rows had
  different lists now leaves every listed fringe out of each of those rows (it changes
  only when saved again).
- Version 1, additive (2026-10): budget
  [`trailing_level_fringes`](#trailing-level-fringes) (`"keep"` or `"drop"`), left out at
  `"keep"`; fringes on adjustment rows; global names that start with digits. The engine
  changed with them (decisions.md, entry 56): a fringe range uses its cutoff up in row
  order (each row rounded on its own) instead of splitting it pro rata; a percent charge
  applies to fringes posted by budget too, and a flat fee leaves out excluded categories
  and accounts; above- and below-the-line count only levels a production total closes. So a
  range's per-line fringes, below-the-line, and some charges can differ from before; the
  file needs no change.
- Version 1, additive (2026-10): budget [`group_rules`](#group-rules) (lines with no
  group, conflicts, fringes in group totals), left out at their defaults, which
  are what files without it already meant; credit row source kind `"category"`
  (decisions.md, entry 57). Files that don't use them are unchanged.
- Version 1, additive (2026-10): budget [`cutoff_factor`](#cutoff-factor) and
  [`fringe_row`](#fringe-row), each left out at its default (decisions.md, entry 60). An
  account coding may now use a set right after the account with no sub-account. New
  budgets start with different settings (posting by budget, set width 3, a 12-hour Day,
  60-hour Week and 240-hour Month), all written in the file; a file without those fields
  reads exactly as before, so existing files are unchanged.
- Version 1, additive (2026-10): budget [`round_account_totals`](#account-totals)
  (decisions.md, entry 69), left out when `false`, which is what files without it mean,
  so existing files are unchanged.
- Version 1, additive (2026-10): credit row source kind `"account"`, currency
  `symbol_after` and `separators`, and baseline `frozen` (decisions.md, entry 61), each
  left out at its default, so files that don't use them are unchanged. New budgets'
  account coding has no separator (`"separator": ""`, numbers read `1101`), written in
  the file as before. A category's fringe account under category posting is now numbered
  from the category's first category-width characters when its number isn't a plain
  category (`510` with suffix `98` is `5198`, was `51098`); only those budgets' fringe
  account numbers change, no figure.

## Complete example

A small budget: two categories split by production totals, one global, a fringe with a
cutoff, a group left out of the total, an adjustment row, and a contingency charge. It uses
the default account coding and fringe posting, so neither is written. This exact text is
checked by the test suite (`crates/method-format/tests/golden/small.json`).

```json
{
  "format": "method-budget",
  "version": 1,
  "budget": {
    "title": "Short Film",
    "base_currency": "USD",
    "info": {
      "Shoot days": {
        "kind": "number",
        "value": "5"
      }
    },
    "units": {
      "Day": {
        "plural": "Days",
        "hours": "12"
      },
      "Flat": {
        "plural": "Flat"
      },
      "Week": {
        "plural": "Weeks",
        "hours": "60"
      }
    },
    "globals": {
      "DAYS": "5"
    },
    "fringes": {
      "Payroll": {
        "kind": "percent",
        "description": "Payroll taxes",
        "rate": "18.5",
        "cutoff": 15000000
      }
    },
    "groups": {
      "Alt": {
        "in_budget_total": false
      }
    },
    "topsheet": [
      {
        "kind": "category",
        "code": "10",
        "name": "Above-the-line",
        "accounts": [
          {
            "code": "10-01",
            "name": "Director",
            "details": [
              {
                "id": 1,
                "description": "Director",
                "amount": "1",
                "unit": "Flat",
                "rate": "5000",
                "fringes": [
                  "Payroll"
                ]
              }
            ]
          }
        ]
      },
      {
        "kind": "production_total",
        "name": "Total Above-the-Line"
      },
      {
        "kind": "category",
        "code": "20",
        "name": "Production",
        "accounts": [
          {
            "code": "20-01",
            "name": "Camera",
            "details": [
              {
                "id": 2,
                "description": "Director of photography",
                "amount": "DAYS",
                "unit": "Day",
                "rate": "850.50",
                "fringes": [
                  "Payroll"
                ]
              },
              {
                "id": 3,
                "kind": "adjustment",
                "description": "Kit rental",
                "amount": "0.1"
              },
              {
                "id": 4,
                "description": "Drone package",
                "amount": "1",
                "unit": "Week",
                "rate": "1200",
                "groups": [
                  "Alt"
                ]
              }
            ]
          }
        ]
      },
      {
        "kind": "production_total",
        "name": "Total Below-the-Line"
      }
    ],
    "charges": [
      {
        "kind": "percent",
        "name": "Contingency",
        "percent": "10"
      }
    ]
  }
}
```

## The change log

A shared budget (docs/research/collaboration.md; decisions.md, entry 62) keeps a change
log beside the `.mthd`, in a folder named after the file: `Feature.mthd` has
`Feature.mthd-log/`. The `.mthd` itself is unchanged, format version 1 as above, and
carries no marker: a program recognizes a shared budget by the folder beside it. The log
is the truth; the `.mthd` is a snapshot of the merged state that one computer (the
keeper) rewrites. The reference implementation is `crates/method-format/src/log.rs`
(the files) over `crates/method-core/src/log/` (the model).

### Layout

```
Feature.mthd                       the snapshot, written by the keeper only
Feature.mthd-log/
  budget.json                      which budget this is
  actors/
    <actor>/                       one folder per computer, written by it only
      000001-000140.chg            a sealed segment: changes 1 to 140 of this actor
      000141-000163.chg
      written.json                 hashes of the .mthd files it wrote from the log
    <actor>/
      ...
```

- **One writer per file.** Only the computer that owns an actor folder writes in it. A
  person with two computers is two actors. No file ever has two writers, so sync services
  never make conflicted copies.
- **Sealed segments.** A segment is written to a temporary name (`.~name.<pid>`, which
  sync services skip) and renamed into place; it is never appended to afterwards. A
  computer may roll its own small segments into one bigger one, writing the new file
  before removing the old ones.
- **Temporary and foreign files are ignored**: anything that is not `<from>-<to>.chg` in
  an actor folder named by 64 hex characters (besides the heartbeat and `written.json`).
- **Files written from the log.** `written.json` (`{"hashes": [...]}`, the SHA-256 of
  each, newest last, the last 20 kept) lists every `.mthd` this computer wrote from the
  log, and the one it started sharing from. A `.mthd` whose hash any computer lists is
  merely behind the log, never an outside edit, so it is never taken in (doing so would
  undo the changes it lacks).

### `budget.json`

```json
{
  "format": "method-budget-log",
  "version": 1,
  "budget_id": "<64 hex characters>",
  "file": "Feature.mthd",
  "genesis": { "actor": "<64 hex characters>", "seq": 1 }
}
```

`budget_id` is the SHA-256 of the genesis change's line (below). `file` is the budget's
file name, never a path. Readers reject another `format` and a newer `version`.
`genesis` names the first change: its actor started sharing and owns the budget, and a
reader names it before reading any segment, so the owner never depends on the order
segments are read in. Every other change must have the genesis within its `deps`; one that
doesn't (another computer that started sharing the same file before the folder synced, or
a forged one) is refused by every reader.

`version` is 1 until someone is given a role, and 2 from the first `grant` on (decisions.md,
entry 78): the computer writing the first `grant` raises it before writing the segment,
and every segment holding a `grant` has `format` 2. A program that reads only version 1
refuses the folder as newer than it reads (Method Budget 0.3.0 then opens the `.mthd` on
its own, unshared, and says why) rather than skipping the roles and editing a stale budget.

### Segment files

A segment is UTF-8 text: a header line, then one change per line, each a single-line
JSON object. The header is

```json
{"segment": {"format": 1, "actor": "<hex>", "from": 1, "to": 140, "hash": "<sha-256 hex>"}}
```

where `hash` is the SHA-256 of every byte after the header's newline. A reader checks the
hash before using a file: a segment that is still syncing (or was altered) fails the
check and is left alone until it is whole. The changes run from `from` to `to` in
sequence, all by `actor`.

### A change

```json
{"actor": "<hex>", "seq": 57, "deps": {"<hex>": 12, "<hex>": 40}, "lamport": 1093,
 "time": "2026-10-12T14:03:22Z", "label": "Edit rate on 21-01 line 3", "ops": [...]}
```

| Field | Meaning |
|---|---|
| `actor` | The computer that made it: 32 bytes as 64 hex characters (its public key, once changes are signed). |
| `seq` | This actor's count, from 1 with no gaps. `(actor, seq)` names the change. |
| `deps` | The last `seq` the author had seen from each actor (a version vector; actors at 0 are left out). A reader holds a change until everything in `deps` has been applied. |
| `lamport` | 1 + the highest `lamport` the author had seen. Two writes to one field are settled by `(lamport, actor, seq, index)`: the higher wins everywhere. Wall clocks decide nothing. |
| `time` | The author's wall clock, ISO 8601, shown but never used to order anything. |
| `label` | What the person did, in the words the application's undo uses. |
| `ops` | The operations, in order. |

Each line is written in canonical form (keys in this order, no spaces, UTF-8), so the
same change always gives the same bytes; the budget id and, later, signatures cover those
bytes. A `sig` field is reserved for signed changes (phase 3 of the design).

### Operations

Every op names its entity by id: `"root"` for the budget's own settings, otherwise 32 hex
characters (a 128-bit id drawn by the actor that created the entity; never written into
the `.mthd`). Entity kinds: `category`, `production_total`, `account`, `row`, `note`,
`info`, `currency`, `global`, `unit`, `fringe`, `group`, `set`, `location`, `charge`,
`excluded_amount`, `credit`, `credit_row`, `version`, `transaction`, `batch`, `etc`.

| `op` | Fields | Meaning |
|---|---|---|
| `create` | `entity`, `kind`, `place` (for kinds that live in a list), `fields`, `sets` | A new entity with its initial fields and tag sets. |
| `set` | `entity`, `field`, `value` | One field, last writer wins. |
| `move` | `entity`, `place` | Where the entity is, last writer wins. |
| `delete`, `restore` | `entity` | The tombstone, last writer wins. |
| `add` | `entity`, `field`, `member` | Adds to a tag set; an add wins over a concurrent remove. |
| `remove` | `entity`, `field`, `member`, `seen` | Removes the adds the remover had seen (`seen` lists their stamps). |
| `grant` | `actor`, `role`, `scope` | Gives a person a role (decisions.md, entry 78): `role` is `co_owner`, `admin`, `editor`, `department`, `actuals`, `comment` or `read_only`; `scope` (only for `department`, left out when empty) lists the categories and accounts by entity id. Last writer wins per person. |

**Roles.** The actor of the first change (the genesis) owns the budget. Until a `grant`
appears, every actor may write anything. Once one has, an actor without a role reads only.
A reader checks every change against its author's role as of the change's own `deps` (the
roles, and where each line and note was, as its author had seen them) and skips one that
goes beyond it, listing it as refused; so every reader keeps and skips the same changes,
whatever order they arrive in. The roles: `co_owner` everything, and sets every role
but the owner's and co-owners' (only the owner gives or changes `co_owner`); `admin`
everything, and sets the roles of everyone but the owner, co-owners and admins; `editor` everything but roles and the root's
`baseline`; `department` creates, changes, moves and deletes rows and notes within its
scope only (a move must start and end there); `actuals` transactions, batches, `etc`
entries and notes; `comment` notes only; `read_only` nothing. Only the owner and
co-owners give `admin` or change an admin's role; nobody changes the owner's. Until changes are signed
this holds against honest applications only. A `grant` whose `scope` is not empty is
malformed unless its role is `department`. A program writing a shared budget without a
window (the command line, the MCP server) records its edit as a change by this computer's
actor, checked like any other, and never rewrites the `.mthd` unless that actor started
sharing; with no actor yet, it is refused once roles are set. Once roles are set, an edit
to the `.mthd` by a program that doesn't write the log can't be attributed, so it is never
taken in: the keeper keeps the edited file beside it as `Feature (outside edit
2026-10-18 140322).mthd` (never over another file) and writes the `.mthd` again from the
log.

A **stamp** is written `"<lamport>/<actor hex>/<seq>/<index>"`: the change's clock and
actor, its seq, and the op's index in it.

A **place** is `{"parent": <entity>, "list": <name>, "pos": <stamp>, "node": {"parent":
<stamp or absent>, "side": "left"|"right"}}`. Lists: `topsheet`, `charges`, `credits`,
`versions`, `transactions` and `batches` under `root`; `accounts` under a category;
`rows` under an account; `notes` under a category, account or row; `excluded_amounts`
under a charge; `credit_rows` under a credit. Positions form a tree per list (Fugue's
rule: a new position is a right child of the position before it when that one has no
right children, otherwise a left child of the position after it); the list order is the
in-order walk of the tree, so two people's runs of inserted lines never interleave.
`node` is absent when the op points an entity back at a position that already exists
(an undo).

A **member** is `{"ref": <entity>}` or `{"text": "..."}`.

A **value** is a JSON scalar (`null`, `true`/`false`, an integer, a string) or a one-key
object naming its type: `{"dec": "7.65"}` (a decimal as a string, as in the `.mthd`),
`{"ref": <entity>}` (another entity: a row's unit, a transaction's account),
`{"formula": "..."}` (a formula with every global name replaced by `{g:<entity>}`, so a
rename is one write to the global's `name`), and the compound settings written exactly as
the `.mthd` writes them: `info`, `coding`, `posting`, `fringe_row`, `new_line_defaults`,
`group_rules`, `display`, `baseline`, `snapshot` (a version's content), `report_presets`,
`tag_presets`, `line_presets`, `ot_patterns`, `setup_order`, `setup_folders`,
`setup_sources`.

### Fields by kind

Field names follow the `.mthd` (`description`, `rate`, `fringe_range`, `in_budget_total`
and so on). The differences: every named item (setup items, info fields, charges, credits)
has a `name` field, since its id is not its name; a row's `unit`, `currency`, `set` and
`location` are references when the item exists and text otherwise; a row's `fringes` and
`groups`, a charge's `excluded_groups`, `excluded_categories` and `excluded_accounts`, a
credit's `excluded_fringes` and a transaction's `tags` are sets; a fringe's `kind` is
`"percent"` or `"flat"` with `cutoff`, `unit`, `basis` and `max_hours_per_day` as
separate fields; a charge's `kind` is `"percent"` (with `percent`) or `"flat"` (with
`amount` and `currency`); a currency's `symbol_after` and `separators` are always
written (decisions.md, entry 61); a credit row's `source` is `"flat"` (`amount`),
`"group"` (`group`), `"category"` (`category`) or `"account"` (`account`, a reference
when the account exists) and its `cap_kind` is `"none"`,
`"qualified_spend"` or `"credit"` (with `cap`); a version has `id`, `name`, `created`,
`note` and `content`; a global's info (`description`, `unit`, `decimals`,
`show_by_name`, `folder`) sits on the global itself. The budget's settings and tools are
fields of `root`: `title`, `base_currency`, `totals_currency`, `fringe_posting`,
`trailing_level_fringes`, `cutoff_factor`, `fringe_row`, `arithmetic`, `account_coding`,
`new_line_defaults`, `group_rules`, `display`, `compact_versions`, `report_presets`,
`tag_presets`, `line_presets`, `setup_order`, `setup_folders`, `setup_sources`,
`ot_patterns`, `baseline` and `round_account_totals` (always written; a log from before
reads as `false`). The provenance is about one file and is not logged.

### Reading rules

1. Apply changes in causal order: a change waits until `seq - 1` of its actor and
   everything in `deps` has been applied. The result does not depend on the order
   changes arrive in.
2. A change that names an entity that doesn't exist, puts an entity in a list its kind
   can't be in, or hangs a position on a missing one is refused whole, listed for the
   owner, and the actor's count moves past it.
3. A second change with the same `(actor, seq)` and different content is a fork: the
   first stays, the second is refused.
4. The budget is a pure function of the applied changes, with these repairs made on the
   way out and never written back: two live rows with one id (the later shows with an id
   derived from its creating op, in the shared range `[2^32, 2^53)`); two live items of
   one kind with one name (identical ones are one item; otherwise the later shows as
   `"Name 2"`); a deleted item still used by a live one, or a deleted account or
   category still holding a live one, is kept; account numbers and global cycles are kept
   as written and flagged.

### Versioning

The log format has its own `version` in `budget.json` and in every segment header, with
the same rules as the `.mthd`: additive changes (a new op field, a new kind, a new value
type) keep the version and are left out when unused; breaking changes raise it, and
readers refuse a newer version with a message to update.

- **Version 1** (2026-10): first form, unsigned.
