# Diff and merge

Method Budget files are text files with stable identities inside (docs/FORMAT.md): every
detail row has a row id that survives edits and moves, accounts and categories have
numbers, and setup items have names. That makes two things possible that a budgeting tool
usually can't do: say exactly **what changed** between two budgets, and **merge** two
copies that changed apart. The engine does both (`method_core::diff`, `method_core::merge`); the
command line and the app use the same code. Decisions are in docs/decisions.md, entry 44.

## What changed (diff)

`diff(a, b)` compares an older budget `a` with a newer one `b` and lists every change in
plain words, in budget order: budget settings, setup tables, the topsheet, then account by
account in the newer budget's order (accounts that went last), then saved versions.

How things are matched:

| What | Matched by |
|---|---|
| Detail rows | Row id. A row in another account is *moved*; one at another line of the same account (outside a longest common run of rows) is *moved* too. |
| Accounts | Number (trimmed). An account whose number changed is found by the rows it holds, then by name within its category: it reads as *renumbered*, not removed and added. |
| Categories | Number, then the accounts they hold, then name. |
| Production totals | Name. |
| Globals, fringes, groups, units, currencies, sets, locations, budget info | Name. |
| Charges and credits | Name (the second of a name with the second). A change of order is listed once. |
| Versions | Id and saved time. |

Each change has its fields before and after, the money it moves by itself when known (a
row's own total, an added or removed account's total) and a sentence:

```
21-01 line 3: rate 1,200.00 -> 1,350.00 (+1,500.00 to 21-01)
21-01 line 4 added: "Dolly grip" (+4,500.00 to 21-01)
21-01 line 2 removed: "Key grip" (-6,000.00 from 21-01)
22-01 line 1: moved from 21-01 line 5, rate 900.00 -> 950.00 (+9,500.00 to 22-01, -9,000.00 from 21-01)
Account 21-07 renumbered to 21-08
Account 21-08 renamed: "Camera" -> "Camera Department"
Fringe FICA: rate 6.2 -> 7.65
Group NQ added: description "Not quoted", in budget total no
Title "Feature" -> "Feature Film"
Version "Studio draft" saved
```

Line numbers are the grid's `#` column (from 1) in the newer budget, the older one for a
row that went. Rates and other money-like numbers read with separators and two decimals;
formulas read as typed. Tags read as `fringes +FICA -WGA`; notes as `notes: 1 added, 1
edited`.

Money: both budgets are recalculated and every account (category fringe accounts
included), category and topsheet line whose total moved is listed with before, after and
change. A budget that can't be totalled leaves the money out and says so (the app passes
its lenient totals instead, so a budget with a bad cell still shows money, the cell
counting as blank as in its grid).

Not listed, because they are tools or about the file rather than content (as for
versions): display settings, report presets, tag presets, line blocks, overtime patterns,
setup order and folders, the provenance, and the compact-versions switch. Actuals are
listed in a few words (`actuals none -> 12 transactions from 1 import`).

Properties (crates/method-core/tests/merge.rs): a budget compared with itself has no changes
and no money; a budget changed in any way has some.

## Three-way merge

`merge(base, ours, theirs)` takes a common ancestor and two budgets made from it and
returns one budget with both sets of changes, plus the conflicts a person has to settle.

- A change on one side only is taken. The same change on both sides is taken once.
- Changes to **different rows**, **different fields of one row**, **different setup
  items** or **different fields of one setup item** merge on their own. Tags merge name by
  name: one side adding FICA to a line and the other adding WGA keeps both; a fringe one
  side took off goes. A charge's excluded groups, categories and accounts merge the same
  way. Notes merge note by note.
- **Conflicts**: the same field changed to different values on both sides; something
  deleted on one side and changed on the other; both sides reordering the same list
  differently. Every conflict takes **ours** by default (or the side asked for) and is
  listed with the base, ours and theirs values in words:

  ```
  21-01 line 3 rate: ours 1,350.00, theirs 1,400.00, was 1,200.00
  Fringe WGA: ours deleted, theirs changed: rate 12.5 -> 13, was kind percent, rate 12.5
  ```
- **An account or category deleted on one side and changed on the other** (its own name,
  number or notes, or anything in it: a line edited, added or moved in) is one conflict
  for the whole thing. Keeping it keeps it as the side that changed it has it, rows the
  deleting side would have taken included. Deleting it takes everything in it, except rows
  the deleting side moved somewhere else first, which stay where it put them.
- **New rows with the same id**: each side numbers new rows from the highest id it has, so
  two copies adding lines usually reuse ids. Rows added on both sides with one id are one
  row only when they are identical (same fields, same account); otherwise both are kept and
  theirs gets a new id above every id in all three budgets. Versions saved on both sides
  get the same treatment.
- **Order**: the side that reordered a list (the topsheet, a category's accounts, an
  account's rows) wins; rows and accounts the other side added go right after the one they
  followed there.
- Budget settings (title, currencies, account numbers, fringe posting, new-line defaults,
  the original totals) and tools (display settings, presets, setup order and folders,
  overtime patterns, compact versions, actuals) merge as whole values: the side that
  changed one wins, both changing it differently is a conflict. The provenance is always
  ours: it describes the file being merged into.
- A merged budget that can't be totalled although both sides can (one side deleted a
  fringe the other side used, say) is reported, with the first problem.

Conflict keys are stable for the same three budgets, so a person can be shown the
conflicts, pick a side for each and the merge be worked out again with those picks
(`merge_with(base, ours, theirs, default, picks)`). Money is never merged: the merged
budget is recalculated like any other.

Properties (crates/method-core/tests/merge.rs, random budgets and random edits):
`merge(base, x, base) == x`, `merge(base, base, x) == x` and `merge(base, x, x) == x` with
no conflicts; edits that don't overlap merge with no conflict, give the same budget in
either order (`merge(base, o, t) == merge(base, t, o)`), give exactly the budget that
making both sets of edits one after the other gives (so its recalculated totals are
those), and the diff's money equals the recalculated difference; any two sets of edits
merge into a budget with unique row ids, and every conflict can be picked either way.

### A returned shared copy

A copy saved for sharing (File > Save a Copy for Sharing…, decision 39) says which budget
and which version it was made from. When that version is still in the budget, it is the
natural base. But the copy may have left things out (notes, budget info, the baseline,
unused setup items), and a base that still has them would read their absence in the copy
as the reviewer deleting them. So the base is first worked out *as it was sent*
(`method_core::merge::base_as_shared`): what was left out isn't recorded, so it is read from
the copy (no note of the base's in the copy means notes were left out; no info field the
same, info; a missing baseline, the baseline; every unused setup item missing, unused
setup). The base's versions are those the copy and the original both still have, and its
tools are the copy's, as a copy carries the original's tools from when it was saved.

## Command line

```
method-budget diff <old.mthd> <new.mthd> [--json] [--exit-code]
method-budget merge <base.mthd> <ours.mthd> <theirs.mthd> -o <out.mthd> [--ours|--theirs]
method-budget merge-driver <base> <ours> <theirs> [<path>] [--theirs]
```

- `method-budget diff` prints the changes grouped under their headings, then the topsheet,
  category and account money. `--json` prints one object: `changes` (each with `kind`,
  `group`, `place`, `subject`, `text`, `money` in cents and `fields` with `before` and
  `after`), `accounts`, `categories` and `totals` (each `label`, `before`, `after`,
  `delta` in cents) and `money_known`. Exit 0; with `--exit-code`, 1 when anything changed.
- `method-budget merge` writes the merged budget to `-o` (atomically) and prints what it did and
  every conflict with the side it took. Exit 0 with no conflicts; **exit 1 when conflicts
  were settled** by the default side (the merged file is written either way, so it can be
  opened and checked); 2 for a usage error.
- `method-budget merge-driver` is the same merge for git: it writes the result over `<ours>`.

### With git

Tell git which files use the driver, in the repository's `.gitattributes`:

```
*.mthd merge=method-budget
```

and what the driver is, once per repository (or with `--global`):

```
git config merge.method-budget.name "Method Budget merge"
git config merge.method-budget.driver "method-budget merge-driver %O %A %B %P"
```

`%O` is the common base, `%A` ours (which the result replaces), `%B` theirs and `%P` the
file's path, for messages. A clean merge is committed as usual. When conflicts remain the
driver still writes a valid budget, every conflict settled for ours, lists them and exits
1, so git marks the file conflicted (`UU`) without writing conflict markers into it: open
it in the app (or run `method-budget merge ... --theirs`) to settle them, then `git add` it. When
both branches added the same file there is no base (git passes an empty one); the driver
says so and leaves ours. Use `--theirs` in the driver line to settle conflicts for theirs
instead.

## In the app

- **File > What Changed…** compares the budget in front with its file as last saved, one
  of its versions, another open budget or a file picked in the Open dialog, and lists
  every change grouped like the diff above, with the money tables. A click on a change goes
  there: a row's cell, an account, a category on the topsheet, a setup tab, the Versions
  dialog.
- **File > Merge Changes From…** picks the returned copy (an open budget or a file) and
  the base: *From the copy* finds the version it was shared from (worked out as sent),
  or pick a version, an open budget or a file. The preview lists every conflict with Ours
  and Theirs to pick (ours to begin with) and what the merge brings into the budget; Merge
  applies it as one undo step. See docs/APP.md, "What changed and merging".
