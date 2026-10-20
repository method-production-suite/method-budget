# A key in the owner's name that isn't the owner's

Design for roadmap 3.18's open decision "A key put in the owner's name before the owner's
app does" (docs/ROADMAP.md, Waiting on the owner). It builds on signed changes (decision
89, in pull request #73, held until 0.7.0 is tagged) and the collaboration design
(docs/research/collaboration.md, section 7). Nothing here is built: it waits on the
owner's choice. This page describes the recommended pair, options (1) and (3), closely
enough to build them in one pull request once the owner says yes.

## The problem

A shared budget's change log lives in a folder that everyone in it can write. Signing
starts when a key is published in a person's name: a `key {key}` op in a change of that
person's own. Before the owner's app has published the owner's key, anyone who can write
in the folder can publish one in the owner's name. From then on every honest app checks
the owner's changes against that key, so whoever holds it signs as the owner: sets roles,
removes people, hands over ownership.

Signed changes already find this on the owner's own computer: the key in the owner's name
isn't the one in `~/.method/signing-key`, so the app says so and signs nothing. That stops
the owner from writing into a log someone else controls. It doesn't say what happens next.

There are two windows where this can happen:

- **A budget whose roles were set before 0.7.0.** Its log is format 2 and unsigned. Signing
  starts the first time the owner opens it in 0.7.0. Until then, a member on 0.7.0 who
  plants a key first wins.
- **A new shared budget before its first role.** The owner shares it, and signing begins at
  the first role; anyone in the folder before that moment can plant a key.

Nothing in the folder can tell the real key from a planted one, because the owner's
computer held no secret before signing that a member could check against. Any policy
either asks a person or accepts a window.

## The recommendation

Two parts, which cover different moments:

1. **Start again as a new signed budget**, offered on the owner's computer only when it
   finds a key in the owner's name that isn't its own. This repairs a budget that was
   taken over before anyone noticed.
2. **Remember the first owner key seen**, on every computer. This stops a key from being
   swapped later, after members have seen the real one.

Option (2), confirming the owner's key by phone or chat, is left out. It costs every member
a step, people skip it, and it protects nobody until someone checks. It can be added later
on top of (3) without a format change: the code it would show is the remembered key's.

## Part 1: start again as a new signed budget

### When it is offered

Only on the computer whose person is the budget's owner (the genesis change names them),
and only when the key published in the owner's name differs from this computer's key.
Every other case keeps today's behaviour. A co-owner or an admin can't start again: the
owner's computer is the only one whose word about the owner's key means anything.

### What the owner sees

The toolbar item and the status panel already say "A key in your name is in this shared
budget, and it isn't this computer's." They gain one button, **Start Again…**, which opens
a sheet:

> Someone put a key in your name in this shared budget before your computer did. Changes
> signed with it count as yours, so you can't tell your own changes from theirs.
>
> Start again makes a new shared budget from this one as it stands now, signed with this
> computer's key. Everyone's roles come with it. The old history is kept aside in the
> folder, but who changed what and undo start fresh. Each person reopens the budget to
> join the new one. Anything they changed and hadn't shared yet must be made again.
>
> [Cancel] [Start Again]

Cancel leaves everything as it is: the owner's app still signs nothing and says so.

### What starting again does

All in the keeper's usual write path, so a crash at any point leaves either the old budget
or the new one, never a mix:

1. **Take the budget as it stands.** The owner's app folds the log as it does now, with
   every refused change left out. This is the budget everyone sees. The changes signed
   with the planted key are already in it; the owner can review them in the Activity tab
   before pressing Start Again, and undo any by hand after.
2. **Write a new genesis.** A new change log whose first change is signed by this
   computer's key, names this person owner, and publishes this computer's key in the same
   change. The new budget id is that change's hash, as for any new shared budget.
3. **Carry the roles over.** The new log's second change, signed by the owner, sets every
   role, admin and co-owner as they stood in the fold. A person removed by cutoff stays
   removed (read only). Keys are not carried: each member's app publishes its own again
   when it joins, as for any new budget, so a key someone else planted in a member's name
   doesn't come along either.
4. **Keep the old log aside.** The old log folder is renamed beside the budget to
   `<budget>.log-replaced-<date>`, with a one-line `REPLACED` file naming the new budget
   id. Nothing in it is deleted. Every app ignores a folder with that file.
5. **Write the `.mthd`.** The keeper's file is written from the new log, as after any
   change, so someone opening the file without the log sees the same budget.

### What everyone else sees

A member's app that opens the budget, or is watching the folder, finds the old log
replaced:

- The status item says "The owner started this shared budget again. Reopen it to join."
  with a **Reopen** button. Until they reopen, the budget is read only on that computer.
- Reopening joins the new log as a new member would: their app publishes its key and
  remembers the new owner key (part 2).
- Their unshared changes in the old log are listed in the status panel as "Not in the new
  budget", with what each one did, so they can make them again by hand. The app doesn't
  replay them: they were made against a history that no longer exists, and replaying
  them silently is how a planted change could slip through.

A member on an older version sees the old log disappear and the new one appear. Format 3
already makes 0.6.x and earlier refuse a signed log, so they open the file on its own with
the existing "newer than this version reads" notice.

### The format

No new op. A replacement is a new log with a normal genesis, plus the `REPLACED` marker
on the old folder. FORMAT.md gains a paragraph describing the marker and the rule that an
app reading a folder with it ignores it. Format version stays 3.

### What it costs

- Who changed what and per-person undo restart from the new genesis. The old log is
  still on disk if someone needs to look.
- Members' unshared changes are listed but must be made again.
- Everyone reopens once.

These costs land only in the case they repair: a budget that was already taken over.

## Part 2: remember the first owner key seen

### What each computer keeps

A small file per budget in the app's data folder (never in the shared folder, which anyone
can write): `shared/<budget id>.owner-key`, holding the owner's actor id and the first key
this computer saw published for it, and the date it was seen. The command line and the MCP
server read and write the same file, as they share the actor id and signing key.

### When it is written

The first time this computer folds a log in which the owner has a key, after the existing
checks pass. On the owner's own computer it is the owner's own key, written when it
publishes it.

### When it is checked

Every fold. If the log's key for the owner differs from the remembered one:

- Nothing signed with the different key is applied on this computer from that change on.
  The changes are listed in the status panel as "Signed with an owner key this computer
  hasn't seen before".
- The status item says "The owner's key in this shared budget changed. Ask the owner
  before you go on." with **Show Details**.
- The budget is read only here until the owner starts again (part 1) or the person
  chooses **Trust the New Key…**, which asks first, says what it means, and rewrites the
  remembered file. That button exists for the honest case: an owner who moved to a new
  computer without handing over first.

### What it doesn't catch

A key planted before this computer ever looked. Part 1 covers that case from the owner's
side. Both together leave one gap: a member who joined only after a planted key and whose
owner never opens the budget again. That member sees the planted key as the owner's. The
owner's app, the moment it opens the budget, finds the mismatch and offers part 1.

## Tests

- Engine (method-core): a planted owner key before the first role is found by the owner's
  fold; starting again yields a log whose genesis is signed by the real key, whose roles
  equal the old fold's, and whose fold equals the old fold's budget (property-tested over
  random budgets and role sets). A removed person stays read only.
- Format (method-format): a folder with `REPLACED` is ignored; a crash between each step
  leaves either the old or the new log readable (a new test that stops the write after each
  step).
- Remembered key: first fold writes it; a changed key stops applying from that change and
  lists the rest; Trust the New Key rewrites it; the command line and MCP refuse with a new
  code, `OWNER_KEY_CHANGED`, and write nothing.
- App (method-app tests): the Start Again sheet appears only on the owner's computer with
  a mismatched key; a member's computer shows Reopen after a replacement; unshared changes
  are listed, not replayed.

## Size

About a week: most of it is the replacement's write path and its crash tests. The
remembered key is a day.
