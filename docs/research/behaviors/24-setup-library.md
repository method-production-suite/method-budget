# Behavior note 24: a shared library of setup items

Source: vendor tutorial video "Using the Cloud for Setup Items" (published 2020 to 2021,
vendor tutorial, 2020/2021 era, about 2 minutes 9 seconds), captions plus extracted frames.
Clean-room: behavior in our own words, no frames, no vendor data.

## Behaviors observed

- Setup lives in one "budget setup" window with tabs per kind: locations, fringes, globals,
  groups, currency, units, sets, credits, charges.
- Each tab has a two-way toggle at the top left: the user's library versus this budget. The
  budget side is highlighted when you are editing the budget's own items.
- Left column: the item kind's folders ("my globals" with user folders such as travel and
  living, series schedule, holiday, plus an uncategorized folder), add and delete folder
  buttons, and a collapsed vendor defaults list at the bottom.
- Budget-side tables, columns seen:
  - Globals: in-use count, a visibility (eye) toggle, a D flag, name, description,
    calculation, units, decimals, value, library column.
  - Fringes: name, description, ID, rate, unit, cutoff, in-use count, total, library column;
    footer with total fringes, percentage-fringe total and flat-fringe total.
  - Units: in-use count, singular name, plural, description, hours equivalence ("hrs flat
    fr"), library column.
  - Groups: name, description, ID, in-budget-total checkbox, color, in-use count, total,
    active total, library column. Above the groups table, three budget-wide group rules:
    include or exclude lines with no group, include or exclude a line whose groups disagree
    (a status conflict), include or exclude fringe dollars in group totals.
- Not every kind has the library column (the narration says only some do).
- Uploading: click the upload icon on an item's row. A confirmation asks whether to also
  upload "dependent items" (setup items the item references). Then switch to the library
  side, open the kind, and the item is there; it can be moved between folders ("move to
  category"), and folders can be added.
- Downloading into another budget: library side, pick the kind, click the download icon in
  the item's budget column. A matching confirmation offers to bring dependent items too. The
  item appears in the budget's table.
- Library and budget copies are separate after the transfer (consistent with the feature
  inventory: no automatic sync).

## Calculation findings vs our engine

None directly. Two engine-relevant facts:

- The in-use count and total per setup item are cheap reverse lookups we can compute from the
  report (lines using a fringe, group, unit, global).
- The three group rules match the open question in the feature inventory (section 6). This
  frame shows the defaults of that era: lines with no group included, conflicting status
  excluded, fringes included in group totals. Our engine: a line with any suppressed group
  is left out (equivalent to "exclude on conflict"), ungrouped lines count, group totals
  include fringes except under budget posting. Consistent with the defaults shown; we don't
  offer the other settings.

## Shortcuts seen

None.

## Era caution

2020/2021 build, cloud storage tied to the vendor account. Method Budget is local-only
(decision 14). The 2026 recordings win any conflict.

## What this means for Method Budget

Local-first equivalent: a **personal library file** (our own open format, e.g. a `.mthd`
JSON next to the user's app data, or any folder the user picks, which they can put in a synced
folder or git repo themselves). Same transfer model: explicit copy in, explicit copy out, no
live link.

1. [missing] P1: **Personal setup library**: save fringes, globals, units, groups, charges,
   credits, currencies, sets and locations from a budget into the library, and pull them into
   any budget. Folders per kind.
2. [missing] P1: **Dependency handling on transfer**: a fringe brings its unit, a global
   brings the globals it references, a charge brings its excluded groups. Show what will come
   along and which same-named items will be replaced or kept (shares the conflict review with
   note 20).
3. [missing] P2: **Import setup from another `.mthd` file** (pick items from a budget on disk),
   which covers most of the library's value with no new file type.
4. [missing] P2: **In-use counts and totals in setup tables** (lines using each fringe, group,
   unit, global), with a click to find them. Helps deciding what is safe to delete.
5. [missing] P3: **Folders (categories) for setup items** inside a budget, independent of the
   apply panel's order.
6. [missing] P3: **Group rules as budget settings**: include or exclude ungrouped lines, how to
   resolve conflicting group status, fringes in group totals. Only if real users ask; current
   behavior matches the reference defaults.
7. [missing] P3: **Templates** (a budget with chart of accounts and setup but no lines), which
   the feature inventory already lists as v1: they overlap heavily with a library.
8. [skip] Vendor default libraries: we author our own defaults from public sources only.
