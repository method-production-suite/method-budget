# Budget exchange file (Method Scheduling's spec, our copy)

> Copied verbatim from Method Scheduling's docs/EXCHANGE.md at commit acf27ce (2026-10-15),
> the version 1 freeze. Our reader is `crates/method-format/src/import/schedule.rs`; roadmap
> 3.8. When the two copies differ, that repository's decisions.md settles it (as the spec
> says below) and this copy is brought back in line. Its links and entry numbers point into
> that repository.

What a budget needs from a schedule, as one small, documented JSON file: how many shoot
days there are, and for each element how many days it works, holds, travels and so on.
Written by `method-schedule budget FILE` and by the app's File > Export for Budget. The
reference implementation is `crates/method-schedule-format/src/exchange.rs`.

Status: **version 1, frozen on 2026-10-15** (decisions.md, entries 201 to 203). From here
on, changes are additive only: a new optional field, never a field removed, renamed or
given a new meaning. A change that would break a reader gets a new `format` name instead
of a new version. The spec lives in both repositories: this file, and the copy the Method
Budget repository keeps beside its reader. When they differ, the newer dated entry in
this repository's decisions.md settles it and both copies are brought back in line.

Version 0 was the draft Method Budget reviewed on 2026-10-11. Version 1 differs from it
only by additions (`days_hash`, `shoot_days.dates`) and by spelling out rules the writer
already followed; a version 0 file reads under the rules below with those two fields
missing.

Any budgeting tool may read it; nothing in it is specific to one product.

## At a glance

- UTF-8, pretty JSON, keys in the order below, one trailing newline.
- **No nulls.** An optional value the schedule doesn't have is left out, never written as
  `null` or `""`. The tables say which fields may be missing; every other field is always
  there (`schedule`, `board` and `sub_board` are the schedule's own text, so `""` when it
  left them blank).
- **Numbers are whole numbers, never negative.** Every count is a number of days. The
  other integers are `version`, an element's `id` and its `min_drop_gap` (calendar days).
  Dates are `YYYY-MM-DD`, on the schedule's own calendar, with no time or time zone.
- **Counts are kept apart and never pre-added**, because tools count days differently. A
  reader picks what it adds up. `total` is the sum of the seven counts, which is exactly
  the reference scheduling product's day out of days summary total (its Work plus Hold,
  where Work already includes counted travel, fitting, rehearsal, holidays and company
  moves; checked against its export, behavior note 15).
- **An element's `total` can be more than `shoot_days.total`.** Holidays, company moves,
  travel, fitting and rehearsal are counted on days that are not shoot days, and a held
  day is not a shoot day for the element either.
- **An element that is not in the file has 0 days** on this sub-board. A budget that
  linked to it should warn rather than keep an old number.
- **Two hashes.** `source_hash` changes whenever anything in the schedule file changes;
  `days_hash` changes only when something this file reports changes. A budget that wants
  to know whether its numbers are out of date compares `days_hash`; one that wants to know
  whether the schedule was touched at all compares `source_hash`. Both are opaque: a
  reader compares them for equality and nothing else.
- Prep and wrap days are budget assumptions, not schedule facts, so they are not counted
  here. A schedule may record a sub-board's prep start and wrap end dates; when it does,
  `shoot_days` carries them.

## Reading it

- **Check `format`.** It is always `"method-schedule-days"`. A file with another format
  name is not this file.
- **Read every `version` you meet.** This writer writes `1`; a reader takes `0` and `1`,
  and a later version too, since versions after 1 only add fields. It ignores fields it
  doesn't know, at any level. A breaking change would come with a new `format` name, so a
  reader never has to refuse a version number.
- **One sub-board per file.** `board` and `sub_board` name where the counts come from. A
  production with several units or blocks exports one file per sub-board; the budget
  decides which file drives its shoot days (usually the main unit's) and may read others
  for their element counts. A file with several sub-boards in it is not planned for
  version 1; if one comes, it will be a new optional field, so this shape keeps working.

## Top level

| Field | Type | Notes |
|---|---|---|
| `format` | string | Always `"method-schedule-days"`. |
| `version` | integer | `1`. Version 0 files (the draft) read the same way. |
| `schedule` | string | The schedule's title. |
| `board` | string | The board the counts come from. |
| `sub_board` | string | The sub-board (unit, block) the counts come from. |
| `generated` | date | The day the file was written, on the writing computer's local calendar. Optional. |
| `source_hash` | string | 16 hex digits over the whole schedule file. See [The hashes](#the-hashes). |
| `days_hash` | string | 16 hex digits over only what this file reports. See [The hashes](#the-hashes). New in version 1. |
| `shoot_days` | object | See below. |
| `elements` | array | One entry per element that works or has a counted event on the sub-board. |

## `shoot_days`

Everything here is about this one sub-board: its days, and the dates that bound its
schedule. Prep start and wrap end stay in this object (decision 201): they are dates of
the sub-board's schedule like `start` and `end`, set per sub-board, and a reader already
finds them here in version 0 files.

| Field | Type | Notes |
|---|---|---|
| `start` | date | The sub-board's start date. |
| `end` | date | The date of the last shoot day. Absent when there are no days. |
| `total` | integer | Shoot days (day breaks) on the sub-board. |
| `by_unit` | object | Unit name to the number of shoot days with at least one sheet of that unit. |
| `by_set` | object | The same by set. |
| `by_location` | object | The same by location. |
| `by_week` | object | The Monday of each calendar week with shoot days, to the number of shoot days that week. These do add up to `total`. |
| `dates` | array of dates | The date of every shoot day, in order, one per day: a reader can group them by any week, month or pay period. Empty when there are no days. New in version 1. |
| `prep_start` | date | Optional. The first day of prep, when the schedule records one for the sub-board. Absent otherwise. |
| `wrap_end` | date | Optional. The last day of wrap, when the schedule records one. Absent otherwise. |

A day with sheets on two sets counts once under each, so the `by_unit`, `by_set` and
`by_location` counts don't add up to `total`. Names are trimmed and compared exactly, so
`Boatyard` and `BOATYARD` are two keys; empty names are left out. The keys of every `by_`
object are in byte order of their UTF-8 text (so `by_week` runs in date order, and
`Zebra` comes before `apple`); a reader that shows them may sort them its own way.

Per element dates (which days each element works) are not in version 1. They may come
later as an optional field on each element.

## Elements

| Field | Type | Notes |
|---|---|---|
| `id` | integer | The element's id in the schedule file. See [Element identity](#element-identity). |
| `category` | string | Cast, Props and so on: the category's name as the schedule has it. Present for every element of a schedule that passes its checks; a writer leaves it out rather than write `""`. |
| `board_id` | string | Cast number or similar. Absent when there is none, never `""`. |
| `name` | string | |
| `first` | date | First strip day on this sub-board. Events never move it. Absent when the element has no strip day here (it only has counted events, a fitting before the shoot say). |
| `last` | date | Last strip day on this sub-board. Absent exactly when `first` is. |
| `work` | integer | Days in a scheduled strip. |
| `hold` | integer | Held days. |
| `travel` | integer | Travel days. |
| `fitting` | integer | Fitting days. |
| `rehearsal` | integer | Rehearsal days. |
| `holiday` | integer | Holidays that count (inside the work span, holds on, not in the drop). |
| `company_move` | integer | Company travel days that count, under the same rule. |
| `total` | integer | The sum of the seven counts above. It can exceed `shoot_days.total`. |
| `holds` | boolean | The element's hold switch, so a reader knows how `hold` arose. |
| `drops` | boolean | Its drop switch. |
| `min_drop_gap` | integer | Its minimum drop gap in calendar days. |

The counting rules are the ones checked against the reference scheduling product
(decisions.md, entries 10 to 12).

## Element identity

There is no key that is both unique and permanent; a budget should hold on to the `id`
and the names together, and ask when they disagree.

- **`id`** is a whole number the schedule gives each thing it holds, from one counter
  (`next_id` in the schedule file, docs/FORMAT.md) shared by every kind of thing. Within
  one schedule file it is unique, and deleting an element never hands its id out again,
  with these exceptions, which are honest limits rather than rules a reader can rely on:
  - **Undo.** Undoing the step that added an element also takes back the counter, so the
    next element added gets the same id. A file saved or exported between the add and the
    undo has the id on the first element; the next file has it on the second.
  - **Copies.** Save As, a copy in Finder and a template are the same file with the same
    ids. From then on each copy counts on its own, so two copies edited apart give the same
    new id to different elements. An id means one element only within one line of a
    schedule's history.
  - **Merge** (docs/MERGE.md) keeps every id ours has. An element only theirs added keeps
    its id unless ours used that id for something else (or its category got a new
    id), in which case it gets a new one.
    An element both sides added with one name in one category becomes one element with
    ours' id.
  - **Merging elements** on the Elements tab keeps the survivor's id; the others' ids go
    out of the file.
  - **Script import** into a new schedule gives every element a new id. Importing a
    revised draft into an existing schedule keeps the id of every element it matches by
    name in its category and gives new ones new ids.
- **`category`, `board_id` and `name`** are free text that the user can change at any
  time. Within one file no two elements in a category share a name (ignoring case) or a
  board ID, so the three together do pick out one element there. They are not unique
  across files or over time: renaming a category or an element breaks a match by name, a
  name freed by a rename or a delete can be given to a different element, and names are
  compared ignoring case (`Kit` and `KIT` are one name to the schedule, two strings to a
  reader comparing bytes).

## The hashes

Both are FNV-1a 64 written as 16 lowercase hex digits. They notice a change; they are not
security hashes, and a reader compares them only for equality, never decodes them.

- **`source_hash`** is taken over the schedule's canonical file text (docs/FORMAT.md),
  the bytes a save would write. Any change to the schedule changes it, view settings
  included (a strip layout, strip colors, what day breaks show), so a budget may see it
  change when no number it uses did. It equals the file on disk only once the schedule is
  saved; the app's export uses the schedule as it is open.
- **`days_hash`** is taken over only what this file reports for the sub-board: the bytes
  of the object `{"shoot_days":…,"elements":[…]}`, where the two values are the file's own
  `shoot_days` and `elements` written as compact JSON (no whitespace outside strings), with
  their keys and items in the file's order. So it changes when a shoot day, a date, a
  count, an element's name, category, board ID or switches, or the set of elements
  changes, and not when the title, the stamped date, a synopsis or the view changes. Its
  coverage is fixed at version 1: a field added later inside `shoot_days` or an element is
  left out of it, so an unchanged schedule keeps its `days_hash` across writer versions.
  A reader may check it by removing the whitespace outside strings from the file's text
  and hashing from `"shoot_days":` to the end, with a `{` in front; it doesn't need to.

## Example

```json
{
  "format": "method-schedule-days",
  "version": 1,
  "schedule": "Harbor Light",
  "board": "Board 1",
  "sub_board": "Main",
  "generated": "2026-10-15",
  "source_hash": "3f0c2a91d7e45b18",
  "days_hash": "9b41e07c25d3a6f0",
  "shoot_days": {
    "start": "2027-03-01",
    "end": "2027-03-03",
    "total": 3,
    "by_unit": {},
    "by_set": {
      "BOATYARD": 1,
      "HARBOR WALL": 1,
      "NET LOFT": 2
    },
    "by_location": {},
    "by_week": {
      "2027-03-01": 3
    },
    "dates": [
      "2027-03-01",
      "2027-03-02",
      "2027-03-03"
    ]
  },
  "elements": [
    {
      "id": 7,
      "category": "Cast",
      "board_id": "1",
      "name": "KIT",
      "first": "2027-03-01",
      "last": "2027-03-03",
      "work": 3,
      "hold": 0,
      "travel": 0,
      "fitting": 0,
      "rehearsal": 0,
      "holiday": 0,
      "company_move": 0,
      "total": 3,
      "holds": true,
      "drops": false,
      "min_drop_gap": 1
    }
  ]
}
```

## Agreed with the budgeting side

On 2026-10-11 Method Budget reviewed version 0 and will read this file for its
schedule-linked globals (its roadmap 3.8):

1. Elements map to budget globals by (category, board ID, name), falling back to `id`;
   when board ID and name both changed but `id` matches, the budget asks before relinking.
2. A performer's days default to `total`; the separate counts are offered as alternatives.
3. One sub-board per file.
4. Counts by set and location are enough for now.

Added from that review: `source_hash`, `by_week`, and the rule that a missing element
means 0 days.

On 2026-10-15, after its reader read version 0, Method Budget asked for the points settled
in version 1 (decisions.md, entries 201 to 203): `first` and `last` optional, prep and
wrap kept in `shoot_days`, `days_hash`, the shoot dates list, the identity rules above,
absent rather than empty values, byte order for the `by_` keys, the reading rules and the
number rule.
