# Collaboration: many people in one budget, no server

Status: design approved by the owner on 2026-10-12 ("do it"). Phases 1 and 2, engine
side, are built: `method_core::log` (the change log, capture, projection, repairs, undo),
`method_format::log` (the folder and segments), `method log` and `method sync`
(decisions.md, entry 62). Not yet built: app wiring (the Session, presence, the review
panel and badges, Who Changed This), signatures and roles (phase 3), iroh (phase 4).
Written for the owner's
direction from a design discussion: a change log under the model, shared-folder sync
first (Dropbox, iCloud Drive, Google Drive, LucidLink; each person writes their own
change files next to the budget), then live peer-to-peer sessions over iroh (QUIC, hole
punching, public or self-hosted relays, end-to-end encrypted), with discovery on the
local network as a bonus.

Goal: Google-Docs-style collaboration on a `.mthd` budget at low or no cost, with **no
server that we run**, and without giving up what the project already has: offline work,
plain files, deterministic totals, git-friendly diffs and the three-way merge.

Contents:

0. Summary
1. Where we are today
2. Requirements from film budgeting
3. Data model
4. File format
5. Transport A: a shared folder
6. Transport B: live sessions over iroh
7. Security and permissions without a server
8. Undo, history, notifications and presence (engine side)
9. The front end
10. Phases, effort and risks
11. Recommendation
12. Open questions for the owner
13. Sources

---

## 0. Summary

- **Build our own change log, not a CRDT library.** Every edit becomes a signed *change*
  (one undo step) made of small field-level operations on entities with stable ids:
  last-writer-wins per field, add-wins sets for tags, and a move-safe ordering for rows,
  accounts and the topsheet. The state is a deterministic fold of all changes; the
  existing `Budget` model is a projection of it. It lives in `method-core` with no new
  dependencies (decision 6). Automerge, Yrs and Loro were compared (section 3.1); none
  does moves the way a budget needs except Loro, and all three would hide our domain
  behind a generic document. Loro stays the fallback if the spike goes badly.
- **The `.mthd` stays the file.** It becomes a snapshot that one person's app (the
  keeper, normally the line producer) rewrites; the change log lives beside it in
  `Feature.mthd-log/`, one folder per computer, each written by that computer only. No
  file ever has two writers, so sync services never make conflicted copies.
- **Shared folder first** (seconds of latency, works offline, zero cost, nothing new to
  install), **iroh second** (instant edits and live cursors when both people are online,
  the same changes over a different pipe). The folder doubles as the rendezvous: members
  publish their iroh addresses in their presence file, so live mode switches on by
  itself between members who are online at once.
- **Permissions are signatures.** Each computer has an ed25519 key; the budget's creator
  signs who may do what (line producer, department editor scoped to categories, actuals,
  comment, read-only). Every honest app drops changes that aren't allowed. Write rules
  are enforceable this way; read access and deletion on other machines are not.
- **Totals are never synced**, always recomputed; the binary-double arithmetic only uses
  `+ - * /` and correctly rounded parsing, which IEEE 754 makes identical on every
  platform we ship. A CI check and a totals fingerprint in presence guard it.
- **Effort**: roughly 5 to 7 months of focused work for the whole path, in phases that
  each ship something useful (section 10). The first useful release (folder sync with
  history, conflicts and presence) is about 3 months in.

---

## 1. Where we are today

What we can build on (docs/MERGE.md, docs/FORMAT.md, decisions 17, 18, 20, 40, 44):

- **Detail rows have stable ids** (`u64`, unique within a budget, kept through edits and
  moves). But they are allocated as "one past the highest", so two copies adding lines
  collide on ids; `merge.rs` re-ids theirs (decision 44).
- **Everything else is identified by its number or name**: categories and accounts by
  code, setup items by name, charges and credits by name, versions by id. References are
  by name too: a row's fringes, groups, set, location and units; formulas name globals;
  charges exclude accounts by code; the baseline is keyed by row id and code. Renames
  rewrite every reference as one undo step (roadmap call 7).
- **Diff and three-way merge** match by identity and settle conflicts field by field,
  with stable conflict keys and a git merge driver (decision 44).
- **Every edit goes through `Session::edit`** with a label ("Edit rate on 21-01 line 3")
  and a time (decision 40), and undo keeps only what changed (decision 18).
- **Autosave writes the file** and never overwrites a version it hasn't seen: a change
  on disk pauses autosave and asks keep mine / load theirs (decision 20). That dialog is
  the only collaboration story today, and it is the thing this design replaces.
- **The MCP server** refuses a change when the file changed under it (docs/MCP.md).
- `method-core` has **no dependencies at all**; crates are held to a 14-day cooldown
  (`scripts/check-crate-ages.py`).

---

## 2. Requirements from film budgeting

| Need | What it means for the design |
|---|---|
| **Offline for days** | Scouts, sets, planes, a stage with no signal. Every edit must be accepted locally and merged later without a server. Rules out anything lock-based. |
| **Many editors, big budget** | 2,000 to 5,000 lines in ~60 accounts (decision 18). A line producer, a UPM, an accountant and 5 to 15 department heads; 2 to 10 at once on a busy day. Merges must be cheap, and a remote change must not cost a full recompute. |
| **Accountability** | "Who changed the camera package rate, and when?" Every value needs an author and a time, kept for the life of the production, and hard to forge. |
| **Roles** | Line producer: everything, including setup (fringes, globals, charges), locking originals, permissions. Department heads: lines in their own categories, maybe only with existing fringes. Accountant: actuals. Studio and financiers: read-only, maybe comments. |
| **Versions and locked originals** | Named versions are frozen. The original (baseline) is locked by the line producer and nobody edits it. Restoring a version is a big, deliberate act. |
| **What-ifs** | Groups toggled in and out of the total (`in_budget_total`), sub-budgets, versions. One person's what-if must not silently change everyone's totals. |
| **Deterministic totals** | Everyone looking at the same changes sees the same cents, on Mac, Windows and Linux. A one-cent disagreement between the line producer and the studio is a credibility problem. |
| **Plain files** | A budget is still a `.mthd` that opens anywhere, diffs in git and can be emailed. |
| **No server we run, no cost** | The file-sync service people already pay for, or peer-to-peer. Anything hosted is optional. |

---

## 3. Data model

### 3.1 Options compared

| | Automerge | Yrs (Yjs port) | Loro | Our own change log |
|---|---|---|---|---|
| Current release | 0.12.0, 2026-09-16 | 0.28.0, 2026-09-17 | 1.16.2, 2026-09-21 | n/a |
| License | MIT | MIT | MIT | ours (MIT or Apache-2.0) |
| Model | JSON-like doc: maps, lists, text, counters, full history | Shared types: maps, arrays, text, XML; history mostly garbage-collected | Maps, lists, movable list, movable tree, text; history with shallow snapshots | Records with stable ids, typed fields |
| Row moves | No native list move (a 2023 paper prototypes one); move = delete + insert, so a concurrent edit and move loses the edit, two concurrent moves duplicate the row | No move on `Array` in 0.28.0 (checked in the docs); same problem | `MovableList` uses Kleppmann's move algorithm: one position per element after concurrent moves | A move is one write to the row's (parent, position) register: no duplicates by construction |
| Conflicts | Keeps all concurrent values, picks a deterministic winner (`get_all`) | Picks a winner, keeps no loser | Picks a winner | Keeps every value in the log; we decide the winner and the badge |
| Our invariants (unique numbers, references, cycles) | Ours to check, on a generic tree | Same | Same | Same, but in our own types |
| History and blame | Built in, generic paths | Weak | Built in | Built in, in budget words |
| Sync protocol | Yes (sync messages; an iroh example exists) | Yes (state vectors, awareness for presence) | Yes | Trivial with per-actor logs: "send actor A's changes after 57" |
| Dependencies | 16 direct, incl. `sha2 0.11.0-rc` and `hexane 1.0.0-alpha` (pre-releases) | not checked | not checked (phase 0 spike) | none in `method-core` |

Sources and dates in section 13. The Rust automerge-repo port (samod) says of itself that
it is experimental and not for serious use yet.

### 3.2 Why our own log

1. **A budget is records, not text.** The hard part of CRDT libraries is merging
   character-level text edits. Our cells are short values that people replace, not
   paragraphs two people type into at once; spreadsheets settle these per cell, last
   writer wins, and so can we. Notes are the only long text, and they are already split
   into separate dated notes.
2. **Moves.** Department heads reorder lines and drag them between accounts all day.
   Only Loro does this right off the shelf.
3. **Our words everywhere.** Blame, the activity feed, conflict badges and permission
   checks all need "21-01 line 3, rate", "fringe FICA", "category 20". With our own
   operations they come for free; with a library each needs a mapping back from paths.
4. **Per-actor append-only logs are the shared-folder transport.** We would have to cut a
   library's binary document into per-actor files anyway.
5. **Signing and roles are per operation.** We need to look inside every change to
   reject what a role may not do.
6. **Dependency policy.** `method-core` stays dependency-free; crypto and networking go
   in a new crate.

The cost is that convergence is ours to prove. We have the habit (property tests on
random budgets and edits, decision 44), and an oracle: for edits that don't overlap,
the change log must give exactly what `merge.rs` gives.

### 3.3 Identity

Every entity needs an id that never changes and is unique without coordination.

| Entity | Today | Proposal |
|---|---|---|
| Detail row | `u64`, next after highest | Random in `[2^32, 2^53)` when made in a shared budget (below 2^53 so JavaScript readers stay exact, FORMAT.md "Big integers"). Checked against every id the budget knows, versions included. Two computers drawing the same id: about 1 in 10^9 for 5,000 rows; detected and repaired (3.10). |
| Category, account, production total | code / name | New 128-bit id (`uid`), held in the log only |
| Fringe, group, global, unit, set, location, currency, info field | name | `uid` |
| Charge, credit, credit row, excluded amount, preset, line block, overtime pattern | name or position | `uid` |
| Note | id unique in its list | `uid` |
| Version | `u64` | `uid` (the `u64` stays as a field) |
| Transaction, import batch | `u64` | random `u64` as for rows |
| Actor (one person on one computer) | n/a | ed25519 public key (32 bytes) |
| Change | n/a | (actor, seq): seq counts from 1 with no gaps |
| Operation | n/a | (actor, seq, index in change) |

The `uid`s are not written into the `.mthd` (4.7 explains why); the log holds them, and
reading an outside edit to the `.mthd` maps entities back by code and name, as `diff.rs`
already does.

### 3.4 The change record

One change per undo step, so history, undo and permissions line up with what people did:

```
change
  actor    ed25519 public key of this computer
  seq      57                      per actor, from 1, no gaps
  deps     {A: 12, B: 40, C: 3}    the last seq seen from each actor (version vector)
  lamport  1093                    1 + highest lamport seen; orders writes
  time     2026-10-12T14:03:22-07:00   claimed wall clock, for display only
  label    "Edit rate on 21-01 line 3"
  ops      [ ... ]
  sig      ed25519 signature over the bytes above
```

Operations:

| Op | Fields | Meaning |
|---|---|---|
| `create` | entity uid, kind, parent, position, initial fields | A new row, account, setup item, note... |
| `set` | entity, field, value | Last writer wins on that one field |
| `move` | entity, parent, position | Last writer wins on where it is |
| `delete` / `restore` | entity | A tombstone flag, last writer wins |
| `add` / `remove` | entity, set field, member | Add-wins set (a remove only removes the adds it saw) |
| `member` | key, role, scope, expiry | Permissions (section 7) |
| `version` | uid, name, note, frontier | A named point in history (3.5) |
| `baseline` | frozen totals | Locking the originals |

**Write order.** Two writes to one field are ordered by `(lamport, actor)`: the higher
wins, on every computer, whatever order they arrived in. Wall clocks never decide
anything (they drift, and they can lie).

### 3.5 Every budget entity, mapped

| Entity | Structure |
|---|---|
| Budget settings: title, base and totals currency, account coding, fringe posting, trailing level fringes, cutoff factor, fringe row, arithmetic, new-line defaults, group rules | One register per setting. Compound settings (account coding, fringe row) are one value, validated whole. |
| Info fields | Map of `uid` to {label, value}, each a register |
| Units, globals (+ global info), fringes, groups, sets, locations, currencies | Setup items: records by `uid`, each field a register (name, description, rate formula, cutoff, kind, unit, basis, color...). Changing a fringe from percent to flat sets `kind`; other fields stay, as the format already keeps a flat fringe's cutoff. |
| Topsheet | Movable list of categories and production totals |
| Category | Record {code, name}, notes list, movable list of accounts |
| Account | Record {code, name}, notes list, movable list of rows. Its place (category, position) is one move register, so moving an account to another category is one write. |
| Detail row | Record {id, kind, description, amount, unit, x, unit2, rate, unit3, x4, unit4, currency, fringe_range, set, location, bold, italic, indent, color}: one register each. `fringes` and `groups` are add-wins sets of setup `uid`s (so one person adding FICA and another adding WGA keeps both, as `merge.rs` does). Place = move register. |
| Notes (row, account, category) | Records {created, author, edited, done, text}; list ordered by creation |
| Charges, credits | Movable lists of records; exclusions are add-wins sets of `uid`s; excluded amounts and credit rows are movable lists of records |
| Baseline | One register holding the whole frozen totals, written only by `baseline` ops (locking is one deliberate act; nobody merges half an original) |
| Versions | A version is a named **frontier** (the version vector at the moment it was saved): its content is the state at that frontier, recomputable from the log. Name and note are registers. When the log is compacted past a version (4.5), the version is materialized as a full snapshot, as versions are stored today. Restoring a version is one ordinary change that sets the current state to the version's (computed with `diff.rs`), so it merges and undoes like any edit. |
| Actuals | Transactions and import batches as records by id; ETC as a map of account `uid` to register. Removing a batch deletes its transactions in one change. |
| Display settings, setup order, setup folders, setup sources, report presets, tag presets, line blocks, overtime patterns | Shared tools: registers or lists of records, as above. |
| Provenance, compact versions | Not synced: about one file, not the budget. |
| Undo stack, window layout, open account | Local only. |

### 3.6 Ordering and moves

Rows, accounts, the topsheet, charges and credits are ordered lists that people reorder.
The design follows Kleppmann's move algorithm (the one Loro's movable list uses):

- **Positions are elements of a sequence CRDT.** Each position records "inserted after
  position P by actor A at lamport L". The order is a deterministic walk of that tree
  (Fugue's rules, which keep two people's runs of inserted lines from interleaving: if
  Alex adds five lines after line 3 while Jordan adds four there, the result is one
  person's run then the other's, in the same order on every computer, never zipped
  together).
- **Each entity has one move register** holding (parent, position). Creating, moving
  within a list and moving to another account are all writes to it. Concurrent moves of
  one row: the later write wins, the loser's position is simply empty. A row can never
  appear twice or vanish, because the row's identity is its id, not a list slot.
- Lists are small (rows per account rarely over 1,000; accounts per category under 100),
  so a straightforward O(n) implementation is enough.

### 3.7 References: bound by id, written by name

The single biggest source of merge trouble in a budget is renaming. Today a rename
rewrites every reference. In a shared budget that is a write to hundreds of cells, and
anyone who edits one of those cells at the same moment brings the old name back.

So, inside the log, **references point at `uid`s**:

- A row's fringes, groups, set, location, units and currency, a charge's exclusions, a
  credit row's group or category, an ETC entry: all `uid`s.
- **Formulas are stored bound**: the text as typed, with each global name replaced by a
  marker for that global's `uid` (`{g:7f3a} * 5`). Shown and written with the current
  names, whitespace and all, so a formula reads exactly as typed until a rename, as now.
- **Renaming is one write** to the item's `name`, and it merges cleanly with anything.
- The `.mthd` snapshot is written with names, so the file format does not change.

### 3.8 Conflicts

| Kind | Rule (deterministic) | Shown as |
|---|---|---|
| Same field, two values | Later `(lamport, actor)` wins | A badge on the cell: "overwritten by Jordan", with Restore (9.5) |
| Different fields of one row | Both kept | Nothing: not a conflict |
| Tags added and removed | Add wins over a concurrent remove | Nothing |
| Deleted on one side, edited on the other | Delete wins, unless the entity is still **used** after the merge (3.10): a row edited inside a deleted account brings the account back, a fringe deleted while someone applied it to new lines comes back | A review item: "Alex deleted fringe WGA; Jordan still uses it on 3 lines; kept" |
| Same thing created twice (two people both add fringe "FICA") | If identical, one item; if not, both kept and the later one shown as "FICA 2" | Review item: "Two fringes named FICA" |
| Both moved the same row | Later move wins | Badge on the row: "moved by Jordan" |
| Reorders of one list | Each item's move register settles it; no "pick an order" conflict | Nothing |

Unlike `merge.rs`, a conflict never blocks: every peer converges at once and the losing
values wait in the review panel. `merge.rs` keeps its job for copies that come back by
email or through git.

### 3.9 Renumbering and moving between parents

- An account's `code` is one register. **Renumbering the chart** is one change with one
  write per renumbered account and the `account_coding` write. Charges, credits, the
  baseline and presets point at `uid`s, so nothing else needs touching (decision 39
  rewrote them one by one).
- Concurrent renumber and add: Alex renumbers to a new pattern while Jordan adds an
  account `21-05` in the old one. After the merge `21-05` doesn't fit; validation flags it
  ("21-05 doesn't fit the new numbering") and the review offers "Renumber it". Nothing is
  renumbered silently, which is decision 39's rule.
- Two people numbering new accounts the same: both kept (the model already tolerates a
  repeated number; decision 53 explains how imports avoid them), flagged for review.
- Moving an account to another category, or rows to another account, is one move write
  each, and merges with edits to their fields.

### 3.10 Invariants after a merge: repair is derived, never written

If every computer wrote its own repair ops, two computers would write different repairs.
So **repairs are part of the projection**: a pure function from the change set to the
`Budget`, the same on every computer, with each repair listed for review. A person
settles it with an ordinary edit, which is then shared.

| Invariant | Derived repair |
|---|---|
| Row ids unique | The row whose `create` sorts later shows with a new id derived from its op id; flagged |
| Setup names unique per kind | Identical items unify; otherwise the later shows as "FICA 2" (the file needs unique map keys); flagged |
| References point at live items | A used item is kept alive ("use revives"); flagged |
| Account in a deleted category, row in a deleted account | The parent is kept alive; flagged |
| Global cycles (Alex sets `A = B * 2` while Jordan sets `B = A / 2`) | Formulas kept as written; the cycle is reported by the engine's existing check (`circular_globals`) and affected cells count as blank, as the app's lenient totals already do; flagged |
| Account numbers unique, fit the pattern | Kept as written; flagged |
| Formula names a global that no longer exists | Cannot happen for bound formulas; for text typed with an unknown name, the usual formula error |

### 3.11 Deterministic totals

Totals are never sent; every computer recomputes from the same state. That only works if
the arithmetic gives the same bits everywhere.

- **Binary arithmetic (the default, decision 55)** evaluates formulas with `+ - * /` on
  `f64`, parses numbers with Rust's correctly rounded parser, and rounds to cents the
  way `Math.round(v * 100) / 100` does. IEEE 754 requires these operations to be
  correctly rounded, so they give identical results on x86-64 (SSE2) and ARM64, Mac,
  Windows and Linux. Rust never fuses a multiply and add unless asked (`mul_add`), and no
  target we ship uses the old x87 unit. Account and higher totals are integer sums of
  cents, which don't depend on order.
- **Exact arithmetic** is integer-based and deterministic by construction.
- **Things that would break it**: functions like `pow`, `exp`, `ln` or `sin` (platform
  math libraries differ in the last bit), compiler flags that allow fast-math, and
  summing doubles in an order that depends on hash maps. Rule for the future: any such
  function is implemented in our own code, not the platform's library.
- **Guards**: a CI job computes a totals fingerprint for the sample budgets on macOS
  ARM64, Windows x64 and Linux x64 and fails if they differ. In a session, each
  computer's presence carries (frontier hash, totals hash); two computers with the same
  frontier and different totals show a diagnostic and offer a bug report.
- **Order still matters where the engine is order-sensitive** (fringe range cutoffs are
  used up in row order, decision 56), which is why the ordering must converge exactly
  (3.6).

### 3.12 How local edits become changes

There are dozens of edit commands in `method-app`. Rewriting each to emit operations
would be a long, risky change. Instead: **capture by diff**. `Session::edit` already
knows the budget before and after each step and which accounts changed (decision 18). A
capture step turns that into operations, field by field, matching by row id and
`uid`, with hints from the commands that rename or renumber (so a rename is one `set`,
not a delete and an add). Property: replaying the captured changes always gives exactly
the budget the session has. The CLI and MCP server, which edit the file, are handled by
ingestion (4.4) until they learn to write changes directly.

---

## 4. File format

### 4.1 Layout

```
Feature.mthd                        snapshot: format version 1, opens in any version
Feature.mthd-log/
  budget.json                       budget id, genesis change hash, log format version
  actors/
    7f3a...c2/                      one folder per computer (actor key, hex)
      000001-000140.chg             sealed segments, written once, never changed
      000141-000163.chg
      presence.json                 rewritten every few seconds by this actor only
      checkpoint-000163.json        optional, this actor's compacted state (4.5)
    91be...04/
      ...
  archive/                          old segments after compaction (zipped)
```

- **One writer per file.** Only the computer that owns an actor folder writes in it; a
  person with a laptop and a desktop is two actors. That alone avoids every sync
  service's conflict-copy behavior.
- **Sealed segments.** A segment is written to a temporary name, flushed, then renamed
  (the atomic write the app already uses, decision 20). It is never appended to after
  that: appending to a file that a sync service is uploading is how partial uploads
  happen. A new segment is cut at most every few seconds while editing, and on idle.
  Each has a header with its length, a hash and the signatures, so a half-synced segment
  is ignored until it is whole.
- **Segment count.** An actor rolls its own small segments into a bigger one now and
  then (writing the new one before removing the old ones, all in its own folder).

### 4.2 The segment file

JSON lines, one change per line, in the canonical style of the `.mthd` (keys in a fixed
order, decimals as strings). Text keeps the project's "readable files" promise and makes
bugs debuggable; signatures cover the exact bytes written, so nothing is ever
re-serialized before checking. Compressed segments can come later if size matters.

### 4.3 Who writes the `.mthd`

If everyone rewrote the snapshot, it would have many writers and Dropbox would make
conflicted copies. So:

- **One keeper writes it**: the budget's owner by default, handed over in Share
  settings. The keeper's app rewrites the snapshot from the merged state on the same
  autosave timing as today.
- **Everyone else never writes the `.mthd`** in a shared budget. They read the log.
- When the keeper is away for a week, the snapshot is a week old; the log is the truth.
  Anyone can still "Save a Copy" (current state to a new file) at any time.
- If a conflicted copy appears anyway (two keepers during a handover), it is harmless:
  the app ignores it, and offers to clean it up.

### 4.4 Outside edits

Someone opens the `.mthd` in an older version of the app, runs `method set`, or an
agent edits it through MCP. The keeper's app (or any member's) sees the snapshot differ
from what the keeper last wrote, diffs the two with `diff.rs` (matching by row id, code
and name), and records the difference as a change by its own actor with the label
"Outside edit to Feature.mthd". Nothing is lost; it is attributed honestly. This also
replaces decision 20's "changed outside" dialog for shared budgets.

### 4.5 Compaction and size

Rough sizes, to be measured in the spike: a one-cell change is about 300 bytes with its
signature; a 500-cell paste is one change of about 30 KB. A busy person might make 1,000
to 2,000 changes on a heavy day, so a production with a dozen people could grow the log
to tens or a couple of hundred megabytes over months, before compression.

- **Checkpoints**: any member can write a checkpoint (the full state with every field's
  stamp, at a frontier) into their own folder. A newcomer loads the newest checkpoint
  signed by an admin, then the changes after it.
- **Archive**: segments covered by a checkpoint move to `archive/` as zip files. History
  and blame for old changes read the archive (slower, rarely needed).
- **Tombstones** (deleted rows) stay until every member has acknowledged a frontier past
  the delete, because an offline member's change can still name the row. They are small.
- **Versions** past the archive point are materialized as full snapshots, exactly as
  versions are stored today (and as deltas with `compact_versions`).

### 4.6 git, diff and merge

- The `.mthd` snapshot still diffs and merges in git with `method merge-driver`.
- If a team commits the log folder too, git merges it trivially: every segment is a new
  file with a unique path, so a git merge of two branches is the union of their
  changes. The merge driver can then **regenerate** the snapshot from the union instead
  of doing a three-way merge. Not required: git users can keep the snapshot-only workflow.
- `diff.rs` gets new jobs: ingestion (4.4), "what changed since I was last here" (the
  projection at my last-seen frontier against now), and restoring versions.
- `merge.rs` stays for copies that come back by email (decision 44's shared copies) and as
  a test oracle.

### 4.7 Backwards compatibility

- The `.mthd` stays format version 1. **No marker is added to it**: readers are strict
  and reject unknown fields, so a marker would lock older app versions out of shared
  budgets. The app recognizes a shared budget by the `Feature.mthd-log/` folder next to it
  whose `budget.json` names it.
- Moving or emailing the `.mthd` alone gives a normal, detached budget. Coming back, it
  merges like any returned copy (`merge.rs`), or by ingestion if it is put back.
- Older app versions open the snapshot read-write; their saves are outside edits (4.4).
- The log format has its own version in `budget.json`, with the same rules as FORMAT.md's
  versioning (strict readers, additive changes keep the version).

---

## 5. Transport A: a shared folder

The owner's first step, and the one with the best cost-benefit: people already share
production folders through Dropbox, Google Drive, iCloud or LucidLink.

**Write pattern** (4.1): per-actor folders, sealed segments, atomic renames, temporary
files named with a `.~` prefix, which Dropbox documents as a temporary-file pattern it
doesn't sync (help page updated 2025-05-28). Never two writers per file. Same person on
two computers is two actors.

**Reading**: the app watches `Feature.mthd-log/actors/`. Proposal: poll the folder every
2 seconds while the window is active and every 10 to 30 seconds in the background (a
directory listing and size and time check is cheap), and add native file events
(FSEvents, ReadDirectoryChangesW, inotify) later; the `notify` crate would be a new
dependency, and polling is plenty for seconds-scale latency. New segments are verified,
ordered by their `deps`, and held back until the changes they depend on have arrived
(segment 7 can sync before segment 6).

**Latency**: whatever the service gives; nothing we can speed up. No service publishes
figures we could find. Expect seconds for Dropbox and Google Drive on a good connection,
and anything from seconds to minutes for iCloud Drive. To be measured in phase 2 on all
four services, with the results in this doc.

**Services, one by one**:

| Service | What to handle |
|---|---|
| Dropbox | Conflicted copies only happen with two writers to one file (avoided by design). Online-only files are placeholders until opened. |
| iCloud Drive | Since macOS Sonoma, evicted files are "dataless" (the old `.name.icloud` stub files are gone); a file's `st_flags` has `SF_DATALESS`. Reading one triggers a download that can block, so the reader checks the flag, asks for the download, and shows "Waiting for iCloud to download 3 changes" instead of hanging. Recommend keeping the budget's folder downloaded. |
| Google Drive for desktop | Streaming mode makes placeholders much like iCloud's; mirroring mode is plain files. Same handling. |
| LucidLink | A streaming file system with a local cache (5 GB by default per a vendor brief) and write-back caching, so another computer may see a new segment late. Its file locking is reported to vary by platform; we don't need it. Pin the budget's folder for offline work. |
| OneDrive, Box, SMB shares, Syncthing | Same pattern works; untested. |

**Presence through heartbeats**: each actor rewrites its own `presence.json` every 10 to
15 seconds while the budget is open: name, color, where they are (topsheet, category,
account), what they last edited and when, their frontier hash and totals hash, and their
iroh address for live mode (6). Older than 2 minutes means away. That is enough for "Jordan
is in 2300 Camera" with a delay of a few seconds to a minute, and not enough for live
cursors. Heartbeats cost a small upload every few seconds per person; a setting can slow
them down.

**Offline**: changes pile up as segments in the actor's own folder; the service uploads
them when it can. Nothing in the app needs to know it was offline, except the status
indicator.

---

## 6. Transport B: live sessions over iroh

**What iroh is now.** iroh 1.0 shipped on 2026-06-15 after four years and dozens of
pre-releases; the latest is 1.3.0 (2026-09-28), MIT OR Apache-2.0. Each endpoint is
dialed by its public key ("dial keys, not IPs"): iroh tries a direct QUIC connection with
hole punching and falls back to a relay. The 1.0 announcement promises wire
compatibility across all v1 endpoints and says wire-breaking changes only come with a
major version.

**Architecture**:

- Each computer runs one iroh endpoint while a shared budget is open. Its endpoint key
  can be the actor key itself, or a separate key bound to it by a signed record (to
  decide in phase 3; both are ed25519, so whether iroh's key types can sign our changes
  directly is worth checking then).
- A small protocol of our own on its own ALPN (`method-budget/sync/1`): hello (budget
  id, membership proof), exchange version vectors, send each other the missing changes
  by (actor, seq) range, then stream new changes as they are made. Presence and cursors
  go as QUIC datagrams, throttled to about 10 a second, never stored.
- **Topology**: with at most a dozen or so people online at once, everyone connects to
  everyone online. Each computer passes on changes it hasn't seen others acknowledge, so
  a partial mesh still converges. iroh-gossip exists (HyParView and Plumtree) but we
  don't need it at this size, and its maturity after 1.0 is unconfirmed.
- **Changes are the same changes**: received live, they are also written to the
  receiver's local store, and the author writes them to its own folder segment as
  always. A change that arrives twice (live, then through the folder) is recognized by
  (actor, seq) and dropped.

**Finding each other**:

| Situation | How |
|---|---|
| Members of a shared-folder budget | Their `presence.json` holds their iroh address: live mode turns on by itself when two members are online. The folder is the rendezvous; no link needed. |
| Live link with no shared folder | An invite link carries the inviter's endpoint address and an invite capability (7). The newcomer dials the inviter, gets the log and the other members' addresses. |
| Same office or stage network | mDNS discovery (iroh's separate `iroh-mdns-address-lookup` crate, 0.x) finds members on the LAN with no relay at all. Short codes work here because nothing leaves the network. |

A short code that works over the internet ("join with 4F7-K2Q") needs a lookup service
that maps codes to addresses, which is a server. Without one, internet invites are a link
or a QR code (9.1). See open question 4.

**Relays and cost**:

| Option | Cost (n0's pricing page, accessed 2026-10-12) | Notes |
|---|---|---|
| n0 public relays | $0 | US, EU and Singapore. Rate-limited (limits can change any time), no authentication, no uptime guarantee. Fine for budgets: sync is kilobytes. |
| n0 hosted, Pro | $19 a month | Authenticated, 5 MB/s, 100 GB egress, 10,000 concurrent connections |
| n0 dedicated region | $199 a month per region | No rate limits, version locking |
| Self-hosted `iroh-relay` | a small VPS | Open-source binary; access by allow list, deny list, shared tokens or an HTTP callout. Needs TLS certificates and open ports. A studio's IT could run one; we never do. |

Relays only forward encrypted packets; they see which endpoints talk and their IP
addresses, not content. n0 has retired relays for old versions on a schedule (0.9x and
release candidates ended 2026-09-30, 0.35 ends 2026-12-31) and promises v1 relays until
v1's end of life, so staying on v1 releases matters.

**How often direct connections work**: n0 says about 95% of data in a connection
typically moves directly between devices (1.0 announcement); a third-party write-up puts
hole-punch success around 90%. An independent 2025 measurement of a similar technique in
libp2p found about 70%. Studio networks that block UDP fall back to the relay over HTTPS,
which still works, just through the relay. Budget traffic is so small that relayed is
fine.

**Encryption**: connections are QUIC with TLS 1.3, authenticated by the endpoint keys, so
only the two ends can read them. On top, every change is signed by its author (7), so a
relayed or forwarded change can't be altered.

**Dependency review** (nothing is added now):

| Crate | Version, date | License | Notes |
|---|---|---|---|
| `iroh` | 1.3.0, 2026-09-28 (1.0.0 2026-06-15) | MIT OR Apache-2.0 | 338 KB crate; about 40 direct dependencies, about two thirds already in our lockfile through Tauri and the updater (`tokio`, `rustls`, `reqwest`, `http`, `bytes`, `serde`, `url`, `tracing`...). New: n0's QUIC stack (`noq`, `noq-proto`, `noq-udp`), `blake3`, `ed25519-dalek` (a **3.0.0 release candidate**, a pre-release), `netwatch`, `papaya`, `backon`, `strum`, `rand 0.10`, and the `iroh-base`, `iroh-dns`, `iroh-relay`, `iroh-metrics` family. `portmapper` is optional. |
| `iroh-mdns-address-lookup` | 0.x | (check) | Only for LAN discovery |
| `automerge` | 0.12.0, 2026-09-16 | MIT | Not proposed; pre-release `sha2` and `hexane` |
| `yrs` | 0.28.0, 2026-09-17 | MIT | Not proposed |
| `loro` | 1.16.2, 2026-09-21 | MIT | Fallback only |

- **Cooldown**: iroh 1.3.0 is exactly 14 days old today; iroh ships every 2 to 3 weeks,
  so we pin an exact version and upgrade deliberately, each upgrade passing the
  cooldown check like any crate.
- **Binary size and compile time**: to measure in a throwaway branch at the start of
  phase 4. The pre-release `ed25519-dalek` is the dependency to watch: if we sign changes
  with a stable `ed25519-dalek` 2.x before iroh arrives, we'd carry two copies until
  they meet. Using iroh's key types for signing from the start avoids that, at the cost
  of pulling `iroh-base` in early. To decide in phase 3.
- All networking lives in a new crate (`method-sync`), never in `method-core`.

---

## 7. Security and permissions without a server

**Keys**: each computer makes an ed25519 key pair the first time it joins a shared
budget, kept in the system keychain (or a 0600 file, as the MCP bridge plans for its
tokens). A person's name and color go with it, as they type them; there are no accounts.
A person with two computers links them with a signed "same person" record so history
shows one name.

**The budget's root of trust**: the first change (genesis) is signed by the creator and
names them owner. Its hash is the budget id. Every later permission traces back to it.

**Membership records** (`member` operations, signed by an owner or admin):

| Role | May |
|---|---|
| Owner (line producer) | Everything; add and remove admins; hand over ownership and the keeper role |
| Admin (UPM, co-line producer) | Everything but ownership |
| Editor | Content: lines, accounts, categories; setup items if the grant says so |
| Department editor | Lines and notes in the categories or accounts named in the grant; apply existing fringes and groups; no setup, no settings, no charges or credits |
| Accountant | Actuals and ETC; notes |
| Commenter | Notes only |
| Viewer | Nothing; reads |

Grants can expire. Every peer checks every change: valid signature, the actor was a
member when it made the change (judged at the change's own causal point, its `deps`),
and each operation is within the role and scope. A change that fails is not applied by
any honest app, and is listed in a "rejected changes" log for the owner.

**Read-only links**: a viewer gets the data; nothing can stop them reading what they have
received. In folder mode, read access is the folder's own sharing permission (a view-only
Dropbox share), and our role only stops their app from making changes. In live mode, peers
only send the log to authenticated members.

**Revoking**: the owner writes a revocation naming the member and **the last seq of
theirs that is accepted**. Everything after it is rejected everywhere, whatever causal
history the change claims. This closes the classic hole where a removed member
backdates changes to look concurrent with the removal (one of the open problems Ink &
Switch lists for Keyhive). The removed member's genuine offline work after that seq is
lost unless someone re-applies it from the rejected-changes list.

**What can and can't be enforced peer-to-peer**:

| Enforceable | Not enforceable |
|---|---|
| Who may make which change (signatures, roles, scopes) | Reading: whoever has the data can read it, forever |
| Authorship: nobody can forge another person's change | Deleting data from someone else's computer |
| Integrity: altered or truncated changes are detected | Wall-clock times (they are claims; order comes from lamport clocks) |
| Removal from future changes | A member copying the budget and starting their own |
| A member's vandalism within their scope is signed, attributable and revertible | Preventing that vandalism in the first place |

**Threat model: a malicious member or peer**:

| Attack | Defense |
|---|---|
| Inject changes as someone else | Signatures |
| Change outside their role or categories | Rejected by every honest peer |
| Vandalism inside their scope | Blame, revert their changes in one step ("Undo everything Jordan did since 2 PM"), revoke |
| Replay old changes | (actor, seq) dedupe |
| Fork their own history (two different changes with the same seq, shown to different peers) | Detected when both meet; both rejected and the actor flagged for the owner |
| Flood with huge changes | Size and rate limits per change and per actor; oversize rejected |
| Bogus checkpoint that hides or invents state | Only admin-signed checkpoints are trusted; any peer can re-verify by replaying the archive |
| Delete other people's folders in the shared folder | Every member keeps a full local copy of all changes in the app's data folder and re-publishes missing segments (byte-identical, signed, so two members restoring the same file at once make identical copies); the service's own version history helps too |
| A malicious relay | Sees endpoints and IPs only; content is encrypted and signed |
| A stolen laptop | Revoke that computer's key |

**Encryption at rest (optional)**: segments in a shared folder are as readable to the
sync service as the `.mthd` is today. A budget key could encrypt segments
(XChaCha20-Poly1305), with the key handed to members inside their membership record,
encrypted to their public key. That protects against the service, not against members,
and the keeper's `.mthd` snapshot would still be plain unless it's encrypted too. Rotating
the key on revoke protects future changes only. Worth it for studios that ask; not in
the first release (open question 6).

---

## 8. Undo, history, notifications and presence (engine side)

- **Per-person undo**. Undo makes a new change that puts back *my* previous values, field
  by field, only where my value is still the current one. If Jordan changed the field
  after me, that field is skipped and the app says so. Undoing a new row deletes it;
  undoing a delete restores it. Redo is the same in reverse. The History tab's steps
  (decision 40) become my steps; other people's changes never enter my undo stack.
- **Blame**. An index from (entity, field) to the operations that wrote it gives "who set
  this value, when, and what it was before", instantly for recent changes and from the
  archive for old ones. Rows, accounts and categories also get "last changed by" by
  rolling up their fields.
- **Activity**. The log is already an activity feed: one entry per change, with its
  label, author and time, filterable by person, account and kind.
- **What changed since I was last here**. The app remembers my last-seen frontier per
  budget; `diff.rs` between that point and now gives the sentences and money MERGE.md
  already prints.
- **Notifications** are derived on each computer from incoming changes and a few rules
  (my department was edited, someone wrote a note that names me, my value was
  overwritten, originals were locked, my access changed). No server, so no email or push
  when the app is closed.
- **Presence and cursors**: folder mode from heartbeats (seconds to a minute old); live
  mode from datagrams (instant). Never stored in the log.
- **Proposals (later)**: a department editor's changes can be held as a proposal stream
  that honest peers don't apply until an admin writes an `accept`. This is "suggesting
  mode" for a budget, and it falls out of the log design almost for free.
- **Personal what-ifs (later)**: a local overlay that flips groups in and out of *my*
  totals only and is never shared, with a clear banner. Or a private draft: my changes
  stay local until I share them.

---

## 9. The front end

This section proposes how sharing looks and feels. It is a starting point for the
owner's design calls, not settled design: every screen below is a proposal, and the
choices that most need the owner are in section 12. Names in the mockups are invented.

**Two modes, one experience.** Everything below works in both transports; the difference
is speed. Folder mode: changes appear in seconds to a minute, presence says where people
were a little while ago, no live cursors. Live mode: changes and cursors are instant. The
app says which is in effect and never pretends folder mode is live.

### 9.1 Starting to share

A **Share** button at the right end of the toolbar (also File > Share...) opens one dialog.

```
+-- Share "Feature Film" ------------------------------------------------+
|                                                                        |
|  How do people get each other's changes?                               |
|                                                                        |
|  (o) Through a shared folder                                           |
|      Dropbox, iCloud Drive, Google Drive, LucidLink... Works offline.  |
|      Changes arrive in a few seconds. Folder: ~/Dropbox/Feature/       |
|                                                                        |
|  ( ) Live link only                                                    |
|      Changes and cursors arrive instantly while you're both online.    |
|      No shared folder needed. Your computer keeps the budget.          |
|                                                                        |
|  People                                                                |
|   Alex Kim (you)    Owner, keeps the file                              |
|   Jordan Lee        Edit: 2000 Production > 2300 Camera   [Change v]   |
|   Priya Shah        Actuals                               [Change v]   |
|   Studio review     Read only                  expires Nov 1  [Remove] |
|                                                                        |
|  Invite someone                                                        |
|   Can  [ Edit                    v]  in  [ Only 2300 Camera       v]   |
|   Expires [ in 7 days v ]                                              |
|   [ Copy invite link ]   [ Show QR code ]   [ Copy code (this network)]|
|                                                                        |
|                                                         [ Done ]       |
+------------------------------------------------------------------------+
```

- **Roles in the picker**, in film words: Edit everything, Edit some categories, Actuals,
  Comment, Read only. "Edit some categories" opens a checklist of the topsheet.
- **Invite link**: `https://methodbudget.com/join#...` with everything after `#` (the
  ticket and the invite capability) never sent to the website, which stays static. The
  page tries to open the app through the `methodbudget://` scheme and, if nothing
  happens, offers the download and says "Then click the link again". It is a secret;
  the dialog says "Anyone with this link can join as Editor until it's used or expires",
  and each link works once by default.
- **QR code** for people in the same room (a phone camera opens the same link on a
  laptop through AirDrop or copy, so this is mostly for laptop-to-laptop with a camera
  app; worth checking if it earns its place).
- **Code**: a short code only on the same network (mDNS), since a code over the internet
  needs a lookup server (open question 4).
- **Choosing folder mode** first asks where the shared folder is, moves or copies the
  budget there (asking first), and creates the `Feature.mthd-log/` folder. If the folder
  is inside a known sync service, the dialog names it.

**What the invited person sees** after clicking the link:

```
+-- Join a budget -------------------------------------------------------+
|  Alex Kim invited you to "Feature Film"                                |
|  You can: edit 2300 Camera. Everything else is read only.              |
|                                                                        |
|  Your name  [ Jordan Lee        ]   Color [ teal v ]                   |
|                                                                        |
|  Keep it  (o) In the shared folder: ~/Dropbox/Feature/  (found it)     |
|           ( ) On this computer only (live while Alex is online)        |
|                                                                        |
|                                       [ Cancel ]   [ Join ]            |
+------------------------------------------------------------------------+
```

- If the app finds the budget's folder already synced on this computer, it offers it.
- **Joining a budget already in a shared folder** needs no link at all if you already
  have a role: open the `.mthd`, and the app sees the log folder and asks "Join as Jordan
  Lee?" Someone who opens it without a role gets read only and a "Request edit access"
  button (9.7).
- **Custom URL scheme**: registering `methodbudget://` needs Tauri's deep-link plugin, a
  new dependency; on Windows and Linux, sending the link to an app that is already running
  needs single-instance handling, which decision 20 avoided to save crates. Fallback that
  needs neither: **File > Join from Link...**, paste the link or code.

### 9.2 Connection status, always visible

A small status item in the toolbar, next to the avatars, with plain words:

```
  Live . All changes shared                    (green dot)
  Synced through Dropbox . 12 s ago             (green dot)
  Sharing 4 changes...                          (animated dot, folder writing or uploading)
  Offline . 37 changes will share when you're back   (grey dot)
  Waiting for iCloud to download 3 changes      (amber dot)
  2 to review                                   (amber badge, opens the review panel)
  Can't reach the shared folder                 (red dot, opens help)
```

Clicking it opens a panel: mode, folder path or link, each person's last contact, how
many of my changes are not yet seen by anyone, the keeper and when the file was last
written.

**First run in folder mode**, once per budget:

```
+------------------------------------------------------------------------+
|  This budget is shared through Dropbox.                                |
|  Your changes are saved on this computer and shared through the        |
|  folder. Others see them in a few seconds when they're online, or      |
|  the next time they open it. You can keep working offline.             |
|  There's no Save: everything you type is kept.          [ Got it ]     |
+------------------------------------------------------------------------+
```

### 9.3 Presence

Avatars (initials in the person's color) in the toolbar; hover for where they are.

```
  (AK)(JL)(PS) +2      hover JL:  Jordan Lee . editing 2300 Camera, line 4
                                  live            [ Go there ] [ Follow ]
                       folder:    Jordan Lee . in 2300 Camera . 40 s ago
                                                 [ Go there ]
```

- **On the topsheet and breadcrumb**, a small stack of initials next to each category and
  account someone is in: `2300 Camera  (JL)`.
- **In the grid (live mode)**: the other person's cell gets an outline in their color with
  their initials tag; a selection is a tinted range. Never a text cursor inside my cell.

```
  #  Description        Amount Unit   X  Rate        Total
  1  Director of Photo  10     Weeks  1   4,500.00   45,000.00
  2  1st AC             10     Weeks  1 [ 2,800.00]  28,000.00   <- JL outline, teal
  3  2nd AC             10     Weeks  1   2,400.00   24,000.00
                                        +--+
                                        |JL|
```

- **Folder mode** shows no cursors (heartbeats are too slow to be honest about a cell),
  only "in 2300 Camera" at the account level.
- **Follow**: my view jumps to where they are and follows as they move; any key, click or
  scroll stops following ("Stopped following Jordan"). Live mode only.

### 9.4 Live updates without getting in the way

Rules, in order of importance:

1. **Never move my cursor, my selection or my scroll.** Rows inserted above the viewport
   keep the row I'm looking at in the same place on screen (anchored by row id).
2. **Never touch the cell I'm typing in.** If someone changes it while I type, a hint
   appears under the editor: "Jordan just changed this to 2,950.00". Whatever I commit
   is a normal later write and wins; Jordan then sees the badge (9.5).
3. **Show where things changed**: the changed cell gets a brief wash of the editor's
   color (about 1.5 seconds) and a small dot that stays until I've looked at the row.
   Totals that moved flash the same way. No counting-up animations; respect the system's
   reduce-motion setting.
4. **The account I'm in** changes under me only where others edited it; a row they added
   slides in with their color.

**When the ground moves**:

| Event | What I see |
|---|---|
| Someone deletes the row I'm on | It stays, struck through and grey, until my cursor leaves it: "Jordan deleted this line." If I was typing, my value is kept and offered: [Bring the line back with my change] [Let it go] |
| Someone deletes the account I'm in | The grid stays with a banner: "Jordan deleted 2300 Camera. [Go to the topsheet] [Ask to restore]" (or [Restore] if I may) |
| Someone renumbers or renames the account I'm in | The breadcrumb updates in place, a quiet toast: "2300 Camera is now 2310 Camera (Alex renumbered the chart)". My place is kept by id. |
| Someone moves the row I'm on to another account | The row leaves with a short animation and a toast: "Line moved to 2310 Camera Rentals by Jordan. [Go there]" |
| A setup item I'm editing in the tools panel changes | The same hint as for cells |

### 9.5 Conflicts

Last write wins and nothing blocks (3.8). The loser is never lost:

```
  Rate  [ 2,950.00 ](!)
        +--------------------------------------------------+
        | Changed by two people                            |
        | Jordan Lee: 2,950.00 at 2:14 PM  (kept)          |
        | You:        2,900.00 at 2:13 PM                  |
        | [ Keep 2,950.00 ]  [ Use 2,900.00 ]  [ History ] |
        +--------------------------------------------------+
```

- The badge shows on both people's screens; it goes away when either picks or edits the
  cell. Picking "Use 2,900.00" is just a new edit, shared like any other.
- **The review panel** (from the status item's "2 to review", and a tab in the tools
  panel) lists everything that needs a person: overwritten values, deleted-but-still-used
  items, two fringes with one name, accounts that don't fit the numbering, a cycle in
  globals, rejected changes (owner only). Each item says what happened in a sentence,
  shows the money it moves, and has the actions that settle it.
- **Setup items**: a fringe rate changed by two people shows the badge on the fringe in
  the tools panel and in the review panel, with how many lines and how much money each
  value means ("Keeping 7.65 instead of 7.5 adds 1,240.00 to the budget").

### 9.6 History and accountability

**Who changed this** (right-click a cell > Who Changed This..., or a shortcut):

```
+-- Rate on 2300 line 2 (1st AC) ---------------------------------------+
|  2,950.00   Jordan Lee      today 2:14 PM   "Edit rate on 2300 line 2" |
|  2,900.00   Alex Kim        today 2:13 PM                               |
|  2,800.00   Alex Kim        Oct 3          Paste 40 rows               |
|  2,750.00   Imported        Sep 28         From "Feature v3.xlsx"      |
|                                    [ Restore a value ]  [ Close ]      |
+------------------------------------------------------------------------+
```

- **Activity** tab in the tools panel: a feed of changes with person, time, label and
  money, grouped by hour, filters for person, category and "only things that moved
  money". Click goes to the place (as What Changed already does).
- **Topsheet blame**: an optional column "Last changed" with "today, Jordan" per category
  and account.
- **Per-person undo, explained**: Edit > Undo says "Undo my last change: Edit rate on
  2300 line 2". The History tab shows *my* steps, with a switch for "everyone" (read
  only). When part of an undo is skipped: "Undid your rate change. Your description
  wasn't put back: Jordan changed it after you." Owners also get "Undo everything Jordan
  did since 2:00 PM..." for cleanups.
- **What changed since you were last here**: on opening a shared budget, a quiet banner
  "14 changes by 3 people since yesterday. [Show]" opens What Changed for that span.

### 9.7 Roles in the interface

- **Read only**: a "Read only" chip in the toolbar; cells don't enter edit mode; menus
  that change things are disabled with the reason in their tooltip. Same as the existing
  read-only shared copies, so it should feel familiar.
- **Department editing**: categories and accounts outside my scope are shown normally but
  their cells have a muted text color and a small lock on the account row; the topsheet
  shows a lock next to them. Typing in one shows: "2400 Grip is read only for you.
  [Ask Alex for access]".
- **Requesting access**: writes a small request into my presence file (folder) or sends
  it live; the owner gets a notification with [Allow] [Allow 2400 Grip only] [Decline].
  No email: nothing is sent outside the budget.
- **Setup for department editors**: fringes, globals and groups are visible and appliable
  but not editable, with "Ask the line producer" on hover.

### 9.8 Notifications

Quiet by default: a badge on the avatar stack and the Activity tab, nothing that pops up,
except three toasts worth interrupting for:

- my value was overwritten while I was looking at it (once per cell),
- my access changed,
- the originals were locked or a version was restored.

A setting per budget turns on toasts for "my department changed" and "someone mentioned
me in a note" (`@Jordan` in a note). macOS and Windows system notifications only when the
app is in the background and the setting is on.

### 9.9 Versions and originals with collaborators

- **Saving a version**: anyone who can edit may save one; it's shared and shows who saved
  it. The Versions dialog gains a "Saved by" column.
- **Restoring a version** and **locking the originals** (the baseline): owner and admins
  only. Both show everyone a toast and a line in Activity ("Alex locked the originals").
- A restored version is one change, so "Undo my last change" undoes the restore.
- Comparing with a version works as now; versions are frozen by definition.

### 9.10 Words and accessibility

No engineering words in the interface. Proposed vocabulary:

| Not this | This |
|---|---|
| CRDT, op log, actor, frontier | (never shown) |
| Sync, replicate | Share changes, shared |
| Peer, endpoint, relay, NAT | Live connection; "through a helper server" only in the status details |
| Conflict, merge | Changed by two people; to review |
| Capability, key, signature | Invite, access; "verified" in history details |
| Revoke | Remove from budget |
| Tombstone | Deleted |
| Keeper | Keeps the file |

Accessibility: every color cue (cursor outlines, flashes, badges) also has a shape or text
(initials tag, the (!) badge, a dot); colors are picked to stay distinguishable for
common color-vision differences; screen readers announce remote changes only for the row
in focus, and politely; all of it respects reduce motion.

### 9.11 Transport by feature

| Feature | Folder mode | Live mode |
|---|---|---|
| Changes appear | seconds to a minute | under a second |
| Presence | account level, from heartbeats, "40 s ago" | cell level, instant |
| Cursors and selections | no | yes |
| Follow | no ("Go there" only) | yes |
| Typing hint "Jordan just changed this" | rarely (too slow to catch) | yes |
| Conflicts | more often (longer windows) | rare |
| Works offline | yes | no; falls back to folder or queues |
| Access requests | through presence files | instant |

---

## 10. Phases, effort and risks

Estimates are for one experienced engineer (or the agent workflow with the owner
reviewing), and are honest rather than tight.

| Phase | What | Effort | Milestone |
|---|---|---|---|
| 0. Spike | Change model, fold and projection for rows and setup in `method-core`; convergence property tests; measure state and log size on a 5,000-row budget; compare against a Loro prototype for ordering | 2 weeks | Go or no-go on the hand-rolled model |
| 1. The log, single-user (built in the engine, 2026-10-12) | `uid`s for all entities; random row ids; capture by diff in `Session::edit`; the log written beside the budget; replay equals the budget, always; per-person undo on top of the log | 4 to 6 weeks | Every edit in the app is in the log, invisible to the user |
| 2. Shared folder (engine side built: segments, outside edits, keeper, `method sync`) | Per-actor segments, polling, ingestion of outside edits, keeper, presence heartbeats, status item, first-run, review panel, conflict badges, Who Changed This, Activity | 5 to 7 weeks | Three computers on Dropbox editing one 5,000-row budget for a week without a lost change; iCloud, Google Drive and LucidLink tested and documented |
| 3. People and permissions | Keys in the keychain, signatures, membership and scopes, verification, revocation, invites for folder mode, read-only and department UI, access requests | 3 to 4 weeks | A department editor can't change what they mustn't; removing someone works |
| 4. Live | `method-sync` crate with iroh, our sync protocol, links and the join page, cursors, follow, relay settings, docs for self-hosting a relay | 4 to 6 weeks | Two people on different networks see each other's cursors and edits instantly; falls back to the relay behind a UDP-blocking network |
| 5. Extras | LAN discovery and codes, compaction and archive, encryption at rest, proposals, personal what-ifs, notifications | 3 to 5 weeks, pick and choose | |

First release worth announcing: after phase 3 (folder sharing with permissions), about 4
months in. Live mode about 1 to 1.5 months later.

**Risks**:

| Risk | Mitigation |
|---|---|
| Convergence bugs in our own model | Property tests from day one: any delivery order gives the same state; matches `merge.rs` for non-overlapping edits; fuzzing with random partitions of three to five actors |
| Capture by diff misses intent (a rename seen as delete and add) | Hints from rename and renumber commands; property test that replay equals the session's budget |
| Recompute cost when remote changes stream in | Batch incoming changes per frame; the per-account recalc cache (decision 18) only redoes touched accounts |
| Sync services behave oddly (iCloud delays, placeholders, partial files) | Sealed, hashed segments; dataless detection; a test matrix per service before calling it supported |
| The log grows too big | Checkpoints, archive, measured in phase 0 |
| Keys confuse people | No word "key" in the interface; computers are linked by invite; recovery is "ask the owner to re-invite you" |
| Floating-point determinism breaks with a new formula function | Rule: no platform math library in the engine; CI fingerprint on three platforms |
| Dependency weight and pre-releases (iroh's `ed25519-dalek` RC) | Networking in its own crate, added only in phase 4, after a size and audit check |
| n0 changes its free relay terms | Relay is configurable; self-hosted relay documented; folder mode doesn't need relays at all |
| It competes with the parity work for time | Phase 0 and 1 are invisible and can run alongside; decide the order with the owner |

---

## 11. Recommendation

1. **Do phase 0 now** (two weeks): it settles the riskiest question, our own model versus
   Loro, with numbers.
2. **Then the owner's order**: the log under the model, shared-folder sync with history
   and conflicts, then permissions, then iroh live sessions, then LAN discovery.
3. **Our own change log in `method-core`**, no CRDT library. Loro is the fallback.
4. **Keep `.mthd` format version 1 untouched**; the log lives beside it; one keeper writes
   the snapshot.
5. **Bind references by id inside the log**, write names in the file.
6. **Repairs are derived, never written**; nothing blocks; everything is reviewable.
7. **No server we run**, ever: n0's free relays by default, a self-hosted relay for those
   who want one, folder mode for those who want neither.

---

## 12. Open questions for the owner

1. **Keeper**: is it fine that only the owner's computer rewrites the `.mthd`, so the file
   can lag while they're away (the log stays current and anyone can Save a Copy)?
2. **Department scope**: may a department editor add lines with existing fringes and
   groups? Change globals? Add accounts in their category? What exactly is line-producer
   only?
3. **What-ifs**: should switching a group in or out of the total be personal (not shared)
   by default, and shared only on purpose?
4. **Codes**: links and QR only over the internet, short codes on the same network only? A
   short code over the internet needs someone's lookup server (n0's or a third party's).
5. **Relays**: n0's free public relays as the default (they see who talks to whom and IP
   addresses, never content)? Or ask the user the first time?
6. **Encryption at rest** in the shared folder: wanted in the first release, later, or
   never?
7. **Names in history**: signed history keeps whatever name each person typed, forever, in
   every copy. Fine for a production? (Relevant to anyone sharing budgets publicly too.)
8. **Conflict default**: last writer wins with a badge (Google Sheets style, proposed), or
   hold the second change for review?
9. **A removed member's late work**: dropped into the owner's rejected list (proposed), or
   offered to the owner for one-click acceptance?
10. **git**: should the log go into git too (union merge, regenerated snapshot), or stay
    folder-only?
11. **Priority**: run phases 0 and 1 alongside the parity work, or after it?
12. **Proposals** (department heads suggest, line producer accepts): wanted early? It
    changes how department roles feel.
13. **Join page**: OK to host `methodbudget.com/join` as a static page on the existing site
    (the invite stays in the link's fragment and never reaches the server)?
14. **New dependencies when their time comes**: Tauri's deep-link plugin (and
    single-instance on Windows and Linux) for `methodbudget://` links, `ed25519-dalek` or
    iroh's key crate in phase 3, iroh in phase 4. Each would pass the cooldown and a size
    check first.

---

## 13. Sources

Accessed 2026-10-12 unless noted. Dates in parentheses are publication or release dates.

- iroh 1.0 announcement, "Dial Keys, not IPs" (2026-06-15): https://iroh.computer/blog/v1
- iroh versions and license on crates.io (1.3.0, 2026-09-28; 1.0.0, 2026-06-15):
  https://crates.io/crates/iroh
- iroh 1.3.0 dependencies: https://crates.io/api/v1/crates/iroh/1.3.0/dependencies
- iroh pricing (Community $0, Pro $19 a month, Dedicated $199 a month per region; page
  undated): https://www.iroh.computer/pricing
- iroh-relay 1.0.0, self-hosting and access control: https://docs.rs/crate/iroh-relay/1.0.0
- iroh local discovery (mDNS in `iroh-mdns-address-lookup`):
  https://docs.iroh.computer/connecting/local-discovery
- iroh with Automerge (example protocol): https://docs.iroh.computer/protocols/automerge
- Third-party write-up of iroh 1.0 (about 90% hole-punch success, unverified):
  https://pinggy.io/blog/iroh_1_0_dial_keys_not_ips/
- "Challenging Tribal Knowledge: Large Scale Measurement Campaign on Decentralized NAT
  Traversal" (libp2p DCUtR, about 70%; 2025): https://arxiv.org/pdf/2510.27500
- automerge on crates.io (0.12.0, 2026-09-16; MIT): https://crates.io/crates/automerge
- automerge 0.12.0 dependencies: https://crates.io/api/v1/crates/automerge/0.12.0/dependencies
- Automerge 3.0 (July 2025; memory cut by more than 10x, same file format):
  https://automerge.org/blog/automerge-3/
- Da and Kleppmann, "Extending JSON CRDTs with Move Operations" (2023):
  https://arxiv.org/html/2311.14007v1
- Kleppmann, "Moving Elements in List CRDTs" (PaPoC 2020):
  https://martin.kleppmann.com/2020/04/27/papoc-list-move.html
- Weidner and Kleppmann, "The Art of the Fugue: Minimizing Interleaving in Collaborative
  Text Editing" (2023): https://arxiv.org/abs/2305.00583
- yrs on crates.io (0.28.0, 2026-09-17; MIT): https://crates.io/crates/yrs
- yrs 0.28.0 `Array` trait (no move): https://docs.rs/yrs/latest/yrs/types/array/trait.Array.html
- loro on crates.io (1.16.2, 2026-09-21; MIT): https://crates.io/crates/loro
- Loro list and movable list: https://loro.dev/docs/tutorial/list
- samod, Rust automerge-repo (experimental by its own README): https://github.com/rgbkrk/samod
- Ink & Switch, Keyhive notebook 01 (local-first access control, concurrent revocation):
  https://www.inkandswitch.com/keyhive/notebook/01/
- "macOS Sonoma has changed iCloud Drive radically" (2023-10-25):
  https://eclecticlight.co/2023/10/25/macos-sonoma-has-changed-icloud-drive-radically/
- "Explainer: File Provider and cloud services" (2026-05-09):
  https://eclecticlight.co/2026/05/09/explainer-file-provider-and-cloud-services/
- Dropbox, files that don't sync, incl. `~$` and `.~` temporary files (updated
  2025-05-28): https://help.dropbox.com/sync/files-not-syncing
- University of Michigan, "Dropbox: Understanding and Resolving Conflicted Copies":
  https://teamdynamix.umich.edu/TDClient/30/Portal/KB/Article/11835/Dropbox-Understanding-and-Resolving-Conflicted-Copies
- LucidLink cache and write-back (vendor brief, 2019):
  https://storageswiss.com/2019/10/29/how-to-create-a-cloud-file-server-lucidlink/
- LucidLink locking by platform (user forum report):
  https://forums.sketchup.com/t/file-locking-between-mac-and-pc-on-shared-drives-and-cloud-storage/342205
