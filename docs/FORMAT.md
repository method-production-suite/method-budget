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
  rather than by position (`method diff`, `method merge` and a git merge driver: docs/MERGE.md).

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
`"1200.50"`. An empty formula means "not filled in" (see [detail rows](#detail-row)).

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
| `fringe_posting`  | [fringe posting](#fringe-posting) | Left out when it is the default (by category, suffix `"99"`). |
| `new_line_defaults` | [new-line defaults](#new-line-defaults) | Left out when it is the default (copy fringes only). |
| `info`            | map of [info values](#info-value) | Budget information fields, keyed by label. |
| `currencies`      | map of [currencies](#currency) | Foreign currencies, keyed by code. |
| `units`           | map of [units](#unit) | Keyed by singular name. Omitted means **no units**. |
| `globals`         | map of formula | Named values usable in formulas, keyed by name. |
| `global_info`     | map of [global info](#global-info) | Description, unit and display settings of globals, keyed by global name. Left out when no global has any. |
| `fringes`         | map of [fringes](#fringe) | Keyed by the name detail rows use. |
| `groups`          | map of [groups](#group) | Keyed by group name. |
| `sets`            | map of [sets](#set) | Keyed by set name. |
| `locations`       | map of [locations](#location) | Keyed by location name. |
| `topsheet`        | list of [topsheet rows](#topsheet-row) | Categories and production totals, in display order. |
| `charges`         | list of [charges](#charge) | Contractual charges, in application order. |
| `credits`         | list of [credits](#credit) | Applied credits (incentives), in application order. |
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

A new budget in Method Budget starts with a set of editable starter units (Allow,
Day, Flat, Hour, Month, Week). They are ordinary data and are written out like any other
units, so a file with no `units` field really has no units.

### Account coding

How account numbers are split into segments and written. Left out entirely when it equals
the default: category width 2, account width 2, no sub-account, no set, separator `"-"`.
When present, every field is required.

| Field            | Type | Notes |
|------------------|------|-------|
| `category_width` | integer 0 to 255 | Characters in the category segment. |
| `account_width`  | integer 0 to 255 | Characters in the account segment (0 means unused). |
| `sub_width`      | integer 0 to 255 | Characters in the sub-account segment. |
| `set_width`      | integer 0 to 255 | Characters in the set segment. |
| `separator`      | string | One character placed between segments, or `""` for none. |

The file stores the pattern as is. The application's own rules (widths up to 6, which
separators are allowed) are checked by the application, not by the file reader.

### Fringe posting

Where fringe totals appear. A tagged object:

| `kind`             | Other fields | Meaning |
|--------------------|--------------|---------|
| `budget`           | none | One fringe total for the whole budget. |
| `production_level` | none | One fringe row above each production total. |
| `category`         | `suffix`: text, **required** | A fringe account at the end of each category, numbered from the category code and the suffix. |
| `account`          | none | A fringe line at the end of each account. |

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
are rounded to cents once per line, half away from zero, from the exact count (see
`docs/decisions.md`, entries 2 and 54).

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
| `production_total` | `name` text, **required** | Subtotals every category since the previous production total. The first one ends above-the-line. A production total has no `code`, `notes` or `accounts`. |

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
| `fringe_range` | optional integer 0 to 4294967295 | Consecutive lines with the same number share percent-fringe cutoffs as one earner. |
| `fringes`      | name set | Fringes applied to this line. |
| `groups`       | name set | Groups this line belongs to. |
| `set`          | optional text | Set name. |
| `location`     | optional text | Location name. |
| `bold`         | boolean, default `false` | Display style. |
| `italic`       | boolean, default `false` | Display style. |
| `indent`       | integer 0 to 255, default `0` | Indent level. The application uses 0 to 4. |
| `color`        | optional text | Text color, written `"#rrggbb"`. Overrides the color of the row's groups. |
| `notes`        | list of [notes](#note) | Dated notes on the row, oldest first. Left out when empty. |

A line costs `amount x X x rate x 4X`. A subtotal row shows a running total and adds
nothing. An adjustment row's rate is the sum of the lines above it (back to the previous
adjustment row). A separator row is a drawn rule line: it has no cost and is ignored by
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
| `percent` | `percent` decimal, **required** | Percent of above- plus below-the-line, less exclusions (`"10"` is 10%). |
| `flat`    | `amount` cents, **required**; `currency` optional text | A fixed fee, less excluded amounts and excluded group totals. |

### Credit

| Field              | Type | Notes |
|--------------------|------|-------|
| `name`             | text, **required** | |
| `account`          | optional text | |
| `applied`          | boolean, default `true` | |
| `cap`              | optional cents | Ceiling on the sum of the row credits. |
| `discount_percent` | decimal, default `"0"` | Taken off after the cap (`"5"` is 5%). |
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
| `group` | `group` text, **required**; `excluded_fringes` name set | Lines tagged with the group, plus their fringes except the excluded ones. |

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
groups and header and footer globals), and never the three budget settings
(`account_coding`, `fringe_posting`, `new_line_defaults`), which belong to a production.
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
        "hours": "10"
      },
      "Flat": {
        "plural": "Flat"
      },
      "Week": {
        "plural": "Weeks",
        "hours": "50"
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
