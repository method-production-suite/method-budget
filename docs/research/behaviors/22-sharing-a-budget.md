# Behavior note 22: sharing a budget

Source: vendor tutorial video "Sharing a Budget" (published 2020 to 2021, vendor tutorial,
2020/2021 era, about 1 minute 38 seconds), captions plus extracted frames. Clean-room:
behavior in our own words, no frames, no vendor data.

## Behaviors observed

- A Share button in the main toolbar opens a separate window ("share a copy") for the open
  budget.
- The window keeps a per-budget recipient list: email, name, date sent, status (not sent /
  sent), and a Send action per row. Add recipient and delete recipient buttons at the top.
  Typing an email fills the name with the same text by default.
- Recipients must be registered users of the vendor's service.
- Sending transfers a snapshot of the budget as it is at that moment. The recipient gets no
  later changes unless the sender sends again ("send update", which is another snapshot, not
  a merge). A green confirmation banner appears and the row's status becomes sent with a
  relative time.
- Recipients stay in the list for later updates or for other budgets.
- Receiving side: the open/new budget screen has a "shared with me" section listing senders
  by email; an unread dot marks a sender with a budget not yet opened. Selecting a sender
  lists the budgets received from them; opening one adds it as a tab like any other budget.

## Calculation findings vs our engine

None (no math shown). The received budget totals matched the sender's in the frame shown.

## Shortcuts seen

None.

## Era caution

2020/2021 build. The share service depends on the vendor's accounts and cloud. Film Wizard
is local-only by Sam's decision (decision 14): no accounts, no server.

## What this means for Film Wizard

Local-first equivalent: the `.fwb` file IS the shareable copy. People send it by email, chat
or a shared drive they already use. What we can add is making that hand-off clean and making
received copies easy to reconcile.

1. [have] Budgets are plain `.fwb` files on disk, documented and diffable (decision 13);
   sending one is sending a file.
2. [missing] P2: **"Save a copy for sharing"**: write a snapshot file with a clear name
   (title, version name, date), optionally stripping notes or private budget info, and
   recording in the copy where and when it came from (source title, version, timestamp).
3. [missing] P2: **Open a received copy next to mine and compare**: when a file with the same
   origin is opened, offer to compare it with the current budget (note 21) or save it as a
   version inside mine (decision 17).
4. [missing] P3: **Sent log**: a per-budget list of copies saved for sharing (to whom, when,
   which version) as plain budget data, no network.
5. [missing] P3: **Read-only open** for a received file, so a reviewer can't change totals by
   accident.
6. [skip] Account-based send, recipient registry, inbox of shared budgets: needs a server.
