# Film Wizard Budgeting native file format

This is the open, documented file format Film Wizard Budgeting uses to save budgets. It is
plain JSON, designed to be read and written by other tools and to diff cleanly in version
control. The reference implementation is `crates/fwb-format` (`to_string` and `from_str`).

File extension: `.fwb`. Encoding: UTF-8. The registered MIME type is
`application/vnd.film-wizard-budget+json`; since the content is plain JSON, tools may also
treat it as `application/json`.

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

## Top level

```json
{
  "format": "film-wizard-budget",
  "version": 1,
  "budget": { ... }
}
```

| Field     | Type    | Notes |
|-----------|---------|-------|
| `format`  | string  | Always `"film-wizard-budget"`. Anything else is rejected as "not a budget file". |
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
| `account_coding`  | [account coding](#account-coding) | Left out when it is the default pattern. |
| `fringe_posting`  | [fringe posting](#fringe-posting) | Left out when it is the default (by category, suffix `"99"`). |
| `new_line_defaults` | [new-line defaults](#new-line-defaults) | Left out when it is the default (copy fringes only). |
| `info`            | map of [info values](#info-value) | Budget information fields, keyed by label. |
| `currencies`      | map of [currencies](#currency) | Foreign currencies, keyed by code. |
| `units`           | map of [units](#unit) | Keyed by singular name. Omitted means **no units**. |
| `globals`         | map of formula | Named values usable in formulas, keyed by name. |
| `fringes`         | map of [fringes](#fringe) | Keyed by the name detail rows use. |
| `groups`          | map of [groups](#group) | Keyed by group name. |
| `sets`            | map of [sets](#set) | Keyed by set name. |
| `locations`       | map of [locations](#location) | Keyed by location name. |
| `topsheet`        | list of [topsheet rows](#topsheet-row) | Categories and production totals, in display order. |
| `charges`         | list of [charges](#charge) | Contractual charges, in application order. |
| `credits`         | list of [credits](#credit) | Applied credits (incentives), in application order. |
| `baseline`        | [baseline](#baseline) | Frozen totals for comparison. Left out when there is none. |
| `report_presets`  | list of [report presets](#report-preset) | Saved report setups, in the user's order. Left out when there are none. |
| `versions`        | list of [versions](#version) | Named copies of the budget, oldest first. Left out when there are none. |

A new budget in Film Wizard Budgeting starts with a set of editable starter units (Allow,
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

### Fringe

A tagged object:

| `kind`    | Fields | Meaning |
|-----------|--------|---------|
| `percent` | `description` text; `rate` formula, **required**; `cutoff` optional cents | `rate` percent of each earner's pay (`"7.65"` is 7.65%), up to `cutoff` of pay when set. |
| `flat`    | `description` text; `rate` formula, **required**; `unit` optional text | `rate` (in base currency) once per line, or per unit when `unit` and the line's unit both have hours. |

### Group

| Field             | Type | Notes |
|-------------------|------|-------|
| `description`     | text | |
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
| `category`         | `code` text, **required**; `name` text, **required**; `accounts` list of [accounts](#account) | A budget category. |
| `production_total` | `name` text, **required** | Subtotals every category since the previous production total. The first one ends above-the-line. A production total has no `code` or `accounts`. |

### Account

| Field     | Type | Notes |
|-----------|------|-------|
| `code`    | text, **required** | Account number, e.g. `"11-01"`. |
| `name`    | text, **required** | |
| `details` | list of [detail rows](#detail-row) | In display order. |

### Detail row

Fields in written order:

| Field          | Type | Notes |
|----------------|------|-------|
| `id`           | row id, **required** | Stable identity. Unique within a budget. |
| `kind`         | `"line"`, `"subtotal"`, `"adjustment"` or `"separator"` | Default `"line"`. |
| `description`  | text, **required** | Always written, even when empty. |
| `amount`       | formula | Blank counts as 0. On an adjustment row, the percentage as a fraction (`"0.1"` is 10%). |
| `unit`         | text | Primary unit name. |
| `x`            | formula | Multiplier. Blank counts as 1. |
| `unit2`        | text | Unit shown next to X. |
| `rate`         | formula | Blank counts as 0. |
| `unit3`        | text | Unit shown next to the rate. |
| `x4`           | formula | Fourth factor. Blank counts as 1. |
| `unit4`        | text | Unit shown next to the fourth factor. |
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
| `note`         | optional text | A plain-text note on the row. The application allows up to 10,000 characters. |

A line costs `amount x X x rate x 4X`. A subtotal row shows a running total and adds
nothing. An adjustment row's rate is the sum of the lines above it (back to the previous
adjustment row). A separator row is a drawn rule line: it has no cost and is ignored by
all math. It is never counted or fringed, it doesn't break a fringe range, and it doesn't
reset a subtotal or adjustment window. Other fields on a separator row are kept but have
no effect.

`bold`, `italic`, `indent`, `color` and `note` are display only and never affect totals.
They apply to every kind of row. The file stores them as given: the application's own
rules (colors are `#` plus 6 hex digits, indent 0 to 4, notes up to 10,000 characters)
are checked by the application, not by the file reader.

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

### Version

A named, full copy of the budget's content, saved inside the budget so it can be restored
or compared later. Fields in written order:

| Field     | Type | Notes |
|-----------|------|-------|
| `id`      | integer, **required** | Unsigned 64-bit integer. The application keeps ids unique within the budget's `versions`; the reader does not check. |
| `name`    | text, **required** | |
| `created` | text, **required** | When the version was saved, as an ISO 8601 date-time such as `"2026-10-09T14:30:00Z"`. Stored as given; the reader does not check it. |
| `note`    | optional text | |
| `budget`  | object, **required** | A [budget](#budget) object with every field except `versions` and `report_presets`: a version cannot contain versions or report presets, and a reader rejects one that does. |

Restoring a version in the application first saves the current content as another version
(named `Before restoring "<name>"`), so a restore can be undone.

**File size.** Each version is a complete copy of the budget's content (every category,
account and row), so a file with N versions is roughly N + 1 times the size of the budget
alone. That is deliberate for version 1: versions are simple, self-contained and survive
any later edit. Row ids inside a version are the ids the rows had when it was saved, which
is what lets the application match rows between a version and the current budget.

Example (inside `budget`, after `baseline`):

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
| `sections` | list of section names | In print order, each once: `"topsheet"`, `"accounts"` (each category's accounts with their totals), `"detail"`, `"fringes"`, `"globals"`, `"groups"` (with their totals), `"notes"` (every note, by account and line), `"info"` (budget information). |
| `excluded_categories` | name set | Category codes left out of the account and detail sections and the notes list. The topsheet always lists every category. |
| `excluded_accounts` | name set | Account codes left out the same way. |
| `skip_empty_accounts` | boolean, default `false` | Leave out accounts with no rows. |
| `skip_zero_accounts` | boolean, default `false` | Leave out accounts whose total is zero. |
| `skip_suppressed_lines` | boolean, default `false` | Leave out lines that don't count in the budget total (a group on them is out of it). |
| `skip_notes` | boolean, default `false` | Leave out the notes printed under detail lines (the notes section still lists them). |
| `only_groups` | name set | When not empty, detail pages print only the lines tagged with at least one of these groups. |
| `columns` | optional list of column names | The optional detail columns in print order: `"amount"`, `"unit"`, `"x"`, `"unit2"`, `"rate"`, `"unit3"`, `"currency"`, `"x4"`, `"unit4"`, `"fringes"`, `"fringe_pct"`, `"fringe_total"`, `"groups"`, `"set"`, `"location"`, `"original"`, `"variance"` (the last two only print when the budget has a baseline, then on the topsheet and account level too). Left out means the standard set; `[]` means none (the line number, description and total always print). |
| `landscape` | boolean, default `false` | Page orientation. |
| `font_size` | text, default `"medium"` | `"small"`, `"medium"` or `"large"`. |
| `shading` | boolean, default `false` | Alternate lines tinted, total rows darker. |
| `group_colors` | boolean, default `false` | Lines print in their own or their groups' colors (otherwise black). |
| `header` | optional [band](#band) | Left out means the standard header (budget title, section, date printed); `{}` means none. |
| `footer` | optional [band](#band) | Left out means the standard footer (page number at the right); `{}` means none. |

Unknown section or column names are errors, like unknown fields.

#### Band

A header or footer: three optional slots, `left`, `center` and `right`, each a tagged
object saying what it shows, worked out when the report is drawn:

| `kind` | Fields | Shows |
|--------|--------|-------|
| `title` | | The budget's title. |
| `section` | | The section on the page (the topsheet, a category's detail, ...). |
| `printed` | | The date and time the report was drawn. |
| `file` | | The budget's file name. |
| `page` | | "Page 3 of 12". |
| `info` | `label` text, **required** | The budget information field with that label, if there is one. |
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

## Reading rules

A reader that follows this document should:

1. Parse the JSON. Reject trailing content after the top-level object.
2. Check `format` is `"film-wizard-budget"`, then check `version`: reject a missing or
   non-integer version, and reject a version newer than the reader supports, with a
   message saying the file needs a newer program. Do this before checking other fields,
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
valid. Loading and saving it in Film Wizard Budgeting normalizes it.

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

## Complete example

A small budget: two categories split by production totals, one global, a fringe with a
cutoff, a group left out of the total, an adjustment row, and a contingency charge. It uses
the default account coding and fringe posting, so neither is written. This exact text is
checked by the test suite (`crates/fwb-format/tests/golden/small.json`).

```json
{
  "format": "film-wizard-budget",
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
