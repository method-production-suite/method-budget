# Setup packs that ship with Method Budget

This folder is bundled with the app. Every `.mthd` file in it appears in the
Library tab's **Setup packs** list as "comes with the app", next to the packs
in your own Packs folder.

It is empty on purpose.

## What a setup pack is

A setup pack is an ordinary Method Budget file (`.mthd`) with setup items and
no lines: globals, fringes, groups, units, sets, locations, currencies,
charges, credits, tag presets, line blocks, report presets and overtime
patterns, whatever its maker picked, plus everything those items need. Its
`provenance` says `setup_pack` and carries the pack's details: a title, a
description, who made it, a version, the date its figures take effect, the
region or jurisdiction they apply to, and source notes and links, for the pack
and for each item (docs/FORMAT.md, "Setup packs").

Importing a pack goes through the same review as File > Import Setup From…:
items the budget lacks are added, items it has set up differently keep the
budget's version unless you take the pack's or add it under a new name, and
the whole import is one undo step. Every item that comes in is marked with the
pack's title, version and effective date, so you can always see where a rate
came from.

Make one with File > Export Setup Pack… (or the Library tab, or
`method-budget pack export` on the command line, docs/CLI.md).

## Why there are no rate packs here

This repository ships no rate data, fringe libraries or union tables
(CLEAN-ROOM.md). Rate packs come from a separate data project, where each
figure is checked against its published source and reviewed before release.
Packs from there can be dropped into your Packs folder (Library tab, Setup
packs, Show Packs Folder), or added here only after that review.
