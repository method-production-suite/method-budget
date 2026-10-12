# The start window and New Budget (roadmap 3.19)

Status: design, 2026-10-12. Phase 1 (section 9) is built: decisions.md, entry 63; docs/APP.md,
"The start window".

The owner likes the reference product's home screen, the one shown before a budget is
open, minus its cloud parts. This note says what that screen offers, from the reference's
public manual only (clean room: described in our own words, nothing copied, no layout
measurements, no assets), and what ours should be in a local-first app where a budget is a
file and nothing is ever uploaded.

---

## 1. What the reference offers (public manual)

From the manual's pages on the home screen, projects, budgets, templates, budget info and
saving (all read 2026-10-12):

- **A home screen first.** After signing in, the reference lands on a screen for opening
  and managing budgets, reachable again from the File menu, a Home button in each budget
  window and the Open shortcut
  (<https://mmb-docs.ep.com/Projects_and_Budgets/openbudgets.html>). Its 2024 release notes
  describe it as a home page for the most recent files, with one tab for files on this
  computer and one for cloud files (<https://mmb-docs.ep.com/release_notes.html>).
- **Projects are folders.** A left rail groups budgets and templates into fixed system
  folders (the user's projects, budgets sent by others, favorites, archive, trash) and the
  user's own folders, which can nest a few levels and be sorted by name; new budgets land
  in an "uncategorized" area until moved (openbudgets.html above;
  <https://mmb-docs.ep.com/Projects_and_Budgets/projects.html>). Projects can be created,
  renamed, archived (their budgets go with them) and deleted once empty.
- **Budgets are rows** with a created date (sortable), a star for favorites, and actions:
  open (double-click), rename in place, duplicate, move to another project, archive
  (archived budgets can only be exported), delete (through the archive or the trash), new
  version and new sub-budget (openbudgets.html;
  <https://mmb-docs.ep.com/Projects_and_Budgets/budgets.html>). Names must be unique in a
  folder; a clash gets a number appended.
- **New budget** is either blank (an untitled budget appears in the current folder and is
  renamed in place) or from a template: own templates and the vendor's, in their own tab
  and folders. A template keeps the chart of accounts and the setup tools and drops the
  detail lines (budgets.html;
  <https://mmb-docs.ep.com/Projects_and_Budgets/templates.html>).
- **Budget info** has a fixed set of default fields (title, production, season, episode,
  episode count, status, the production's base country, region and city, location and stage
  shoot days, start and end dates, year) plus custom text, number and date fields; they
  feed cover pages, headers and footers and exports
  (<https://mmb-docs.ep.com/Setup/budget_info.html>).
- **Cloud underneath.** Budgets live in an encrypted local store synced to the vendor's
  servers, autosaved by default; the manual asks users to manage budgets inside the app
  rather than as files, and some features (compare) need the cloud copy
  (<https://mmb-docs.ep.com/Projects_and_Budgets/Saving_Offline.html>, openbudgets.html).
  An accounting link id shows as a read-only column.

What the reference's New Budget does **not** do, by its manual: ask for production type,
dates, location, currency or posting up front. A new budget is untitled with the template's
setup, and the details are filled in later in Budget Info and the setup screens.

**Left out on purpose** (roadmap: "Skipped on purpose"): signing in, cloud sync, the
sent-to-me inbox, cloud-only features, an in-app trash (the system's trash does that),
accounting link ids.

---

## 2. Principles for ours

1. **A budget is a file, a project is a folder.** No database, no index that can drift from
   the disk. Anything the window remembers (recent, pinned, project folders) is a list of
   paths in the app's own data folder, and anything it shows about a file (title, total) is
   read from the file and cached by its modified time and size.
2. **Works with what people already use**: Finder, a shared Dropbox or LucidLink folder,
   git. Moving a file in the Finder moves it in the app (it is found again from its
   folder, or shown as missing with a Remove button).
3. **Keyboard first, plain words, no vendor names.** Type to search, arrows and Enter to
   open, one key to pin. Labels say what happens ("New Budget…", "Show in Folder").
4. **Fast.** The window opens instantly from a cache and fills in totals as files are read.
5. **The page never names a path** (docs/APP.md, "Security model"): it gets tokens and
   display strings; Rust holds the paths.

---

## 3. When it shows

- **At launch** when nothing was reopened: no tabs from last time, no file double-clicked,
  no recovered budgets to offer. It sits over the untouched Untitled budget, which gives
  its tab up to whatever is opened or created (as Open does today).
  A "Show this window when the app starts" box turns that off.
- **File > New… (Cmd+N)** and the toolbar's New button open it any time. New Tab (Cmd+T)
  and the tab strip's **+** keep making a starter budget at once, for people who want the
  old fast path.
- **Escape** closes it and goes back to the budget behind.
- Later (phase 2): when the last tab closes, the start window instead of closing the app's
  window (open question 1).

---

## 4. The start window

```
+-- Method Budget -----------------------------------------------------------------+
|  [ New Budget...  Cmd+N ]  [ Open...  Cmd+O ]          [ Search budgets       ]  |
|                                                                                  |
|  PINNED                                                                          |
|   Big Show                         54,210,330.00 USD   Today 14:05   Unpin       |
|   Big Show.mthd  ~/Productions/Big Show                    open                  |
|   Pilot (studio cut)                3,904,120.00 USD   Oct 9                     |
|   Pilot v3.mthd  ~/Dropbox/Pilot                                                 |
|                                                                                  |
|  RECENT                                                                          |
|   Spot 2027 "Coast"                   412,500.00 USD   Oct 11                    |
|   Coast.mthd  ~/Commercials                                                      |
|   Doc series ep 4                         can't find this file      Remove       |
|   Episode 4.mthd  ~/Old drive/Docs                                               |
|                                                                                  |
|  Enter opens, P pins, Delete takes it off the list (the file stays).             |
|  [x] Show this window when the app starts                          [ Close ]     |
+----------------------------------------------------------------------------------+
```

- **Rows**: the budget's title, its file name and folder (home shortened to `~`), the net
  total (the topsheet's bottom line) in its base currency, when the file last changed,
  "open" when it is in a tab (its unsaved changes count), and a sentence when it can't be
  read or found. Row actions show on hover and focus: Pin or Unpin, Show in Folder, Remove
  (recent only; the file is never touched).
- **Pinned** first (kept until unpinned, any number up to 100), then **Recent** (the last
  ten opened or saved, the same list as File > Open Recent, without the pinned ones).
- **Search** filters both lists as you type: every word must appear in the title, the file
  name or the folder. Typing anywhere in the window types into the search box; Down moves
  into the list, Enter opens the first match.
- **Keys**: Up and Down move, Enter opens, P pins or unpins, Delete or Backspace removes
  from Recent, Cmd+N starts a new budget, Cmd+O shows the Open dialog, Escape clears the
  search and then closes.
- **Thumbnails** (phase 2): a small drawing of the topsheet, one bar per category scaled to
  its share of the total, made from the cached summary, no images on disk.

With projects (phase 2) the window gets a left column:

```
+-- Method Budget -----------------------------------------------------------------+
|  HOME           |  [ New Budget...]  [ Open...]                [ Search        ]  |
|   Recent        |                                                                 |
|   Pinned        |  BIG SHOW  ~/Productions/Big Show                 Show Folder   |
|  PROJECTS       |   Big Show                    54,210,330.00   Today   budget    |
|   Big Show      |   Big Show - Camera           2,110,400.00   Oct 10  sub-budget |
|   Pilot         |   Big Show - v3 - Oct 1       53,980,000.00   Oct 1   shared    |
|   Coast         |   schedule/Big Show.days.json   38 shoot days  Oct 11  schedule  |
|  + Add Folder   |   actuals/2026-09 GL.csv        1,204 rows     Oct 3   actuals   |
|                 |   shared folder: 3 people, Jordan in 2300 Camera now            |
+-----------------+-----------------------------------------------------------------+
```

---

## 5. Projects as folders (phase 2)

A project is a folder the user adds (**+ Add Folder**, Rust's folder dialog) or the folder
a new budget is saved in. The app keeps the list in its data folder and reads each one
shallowly (two levels, at most a few hundred files) when the window opens. Nothing is
written into the folder unless the user makes something there. What it finds, grouped:

| Found | How it is recognized | Shown as |
|---|---|---|
| A budget | `.mthd` with no provenance | budget |
| Its versions | named versions live inside the file (decision 17); Save As copies are separate budgets | a count on the budget's row; copies as budgets |
| Sub-budgets | provenance says sub-budget of a title (decision 39) | under their parent, by title |
| Shared copies | provenance says shared or editable copy | under their source, by title |
| Templates and setup packs | provenance `setup_pack`, or a budget with no lines | in a Templates section, offered by New Budget |
| Change log | a `X.mthd-log/` folder beside `X.mthd` (decision 62) | "shared folder" badge on the budget, people from presence (section 7) |
| Schedule days | a `sched-days` exchange file (`format: method-schedule-days`) | schedule, with its shoot days and date (section 8) |
| Actuals exports | `.csv`, `.tsv`, `.xlsx` in an `actuals` folder, or ones already imported (their batch names the file) | actuals |

An optional `project.json` (later, only if needed) could pin the main budget and name the
project; the folder's name does that until then. Project actions: New Budget here,
Show Folder, Remove from list (the folder stays). Duplicate, new version copy and new
sub-budget act on a selected budget (they exist today as File menu commands).

---

## 6. New Budget

One form, top to bottom, every field optional but the title. Tab and Shift+Tab move,
Cmd+Enter creates, Escape goes back to the start window.

```
+-- New Budget --------------------------------------------------------------------+
|  Title        [ Big Show                                    ]                    |
|  Type         [ Feature                v]   Season [   ]  Episode [   ]          |
|                                             Episodes [   ]  (series)            |
|                                                                                  |
|  DATES        Prep start [2027-01-04]  Shoot start [2027-02-01]                  |
|               Shoot end  [2027-03-12]  Wrap end    [2027-03-26]                  |
|  LENGTH       Prep weeks [4 ]  Shoot days [30 ]  Wrap weeks [2 ]                 |
|  LOCATION     Country [United States]  Region [California]  City [Los Angeles]   |
|  CURRENCY     [ USD ]  money is entered and totalled in this currency            |
|                                                                                  |
|  START FROM   [ Feature film (our chart)                                     v]  |
|               Starter budget (sample lines) / Empty / our charts / studio charts |
|               / A budget file...                                                 |
|  SETTINGS     Fringes post  [ in one budget row v]   Arithmetic [ spreadsheet v] |
|                                                                                  |
|  (phase 2+)   Setup  [ ] Everything in my setup library  [ ] Pack: ...           |
|               Rates  [ ] Union rates for ... (once rate data ships)              |
|                                                                                  |
|                                          [ Back ]   [ Create Budget  Cmd+Enter ] |
+----------------------------------------------------------------------------------+
```

Where each field goes (all through the existing edit functions, so the same checks apply):

| Field | Goes to |
|---|---|
| Title | the budget's title (and the suggested file name at the first Save) |
| Type | budget info "Production type" (Feature, TV episode, TV season, Commercial, Documentary, Other); also picks the matching chart until Start From is changed by hand |
| Season, Episode | info "Season", "Episode" (text) |
| Episodes | info "Episodes" (a number), for a season |
| Dates | info "Prep start", "Shoot start", "Shoot end", "Wrap end" (dates) |
| Prep weeks, Shoot days, Wrap weeks | globals `PREP_WEEKS`, `SHOOT_DAYS`, `WRAP_WEEKS` (the starter's names, so its formulas pick them up; `SHOOT_WEEKS` stays `SHOOT_DAYS / 5`) |
| Country, Region, City | info "Country", "Region", "City" |
| Currency | the base currency (refused when a template lists it as a foreign currency) |
| Start from | the starter budget, an empty budget, a bundled chart (decision 51) or a budget file (stripped as New From Template strips it, decision 29) |
| Fringes post | fringe posting: one budget row (the new-budget default, decision 60), each production total, each category, each account |
| Arithmetic | spreadsheet (binary, the default, decision 55) or exact decimal |

The info field names are ours, in plain words. Dates are not turned into day counts: shoot
days depend on the week and holidays, so they are typed, or come from a schedule (section
8). The budget opens in a new tab (or in place of the untouched Untitled), titled, with no
file: the first Save asks where, suggesting the title. Creating is one undo step, so
closing it asks to save.

Later sections of the form, in their phases:

- **Setup** (phase 2): tick setup library items or setup packs to bring in (decisions 37,
  50), using Import Setup's rules with everything new.
- **Rates** (phase 3): pick union agreements and a jurisdiction from the rate library once
  rate data ships (roadmap 3.9, the separate data project); production-held and payroll
  company tables stay check-only, never bundled.
- **Save in** (phase 2): a folder (a project), with the file named after the title, so the
  budget has its file and autosave from the first minute.

---

## 7. Shared folders and the change log

The change log (decision 62; docs/research/collaboration.md) keeps a `X.mthd-log/` folder
beside the budget in a shared folder. The start window is where that shows first:

- A budget with a log folder gets a **shared** badge; its row lists who was in it lately
  and where, read from the actors' `presence.json` files (design 5), once presence is built.
- Opening one asks once "Join as <name>?" (design 9.1); a project folder inside Dropbox,
  iCloud Drive, Google Drive or LucidLink is named as such.
- **New Budget > Save in** a shared folder offers "Share with people in this folder" as the
  last step, which makes the log folder (design 9.1's dialog).
- Nothing in the start window writes to a log folder: reading presence is enough.

---

## 8. Schedules

Method Scheduling writes a `sched-days` exchange file (roadmap 3.8; that project's
docs/EXCHANGE.md): shoot days in all, by unit, set, location and week, the shoot start and
end, prep start and wrap end when the schedule has them, and each element's days, with a
`source_hash`.

- **New Budget > Link a schedule…** (phase 3, with 3.8): pick an exchange file to fill
  Shoot days, the dates and later the cast and crew day counts; the link keeps the file's
  path (relative to the project folder when inside it) and its `source_hash`.
- The project view lists exchange files in the folder, and a linked budget's row says
  "schedule changed since linked" when the hash differs, with a button to review the new
  counts (3.8's own review).

---

## 9. Phases

| Phase | What | Depends |
|---|---|---|
| **1 (built)** | Start window: pinned and recent with title, net total, modified time and folder, search, open, pin, show in folder, remove from list; shown at launch when nothing is open and from File > New… and the New button; New Budget form: title, type, season, episode, episodes, dates, schedule length, location, currency, start from (starter, empty, bundled charts, a budget file), fringe posting, arithmetic | 1.20, 3.16 |
| 2 | Projects as folders (section 5), thumbnails, Save In a folder at create, setup library and packs at create, the start window when the last tab closes, duplicate and new sub-budget from a row | phase 1 |
| 3 | Rates at create (3.9, once rate data ships), Link a schedule (3.8), shared badges and presence (3.18 phase 2), Join | 3.8, 3.9, 3.18 |
| 4 | Sorting (name, modified, total), created date, archived projects hidden | phase 2 |

---

## 10. Open questions

1. When the last tab closes, show the start window instead of closing the app's window?
   (Today Cmd+W on the last tab closes the window.) Proposed: yes, in phase 2.
2. Default Start From: our chart for the type (built so), or the starter budget with its
   sample lines and placeholder rates (what New made before)?
3. Info field names: ours in plain words (built so), or the reference's default field names,
   which would map one to one onto its exports and cover pages?
4. Should Create ask where to save at once (a file and autosave from the start) instead of
   leaving the budget Untitled until the first Save? Proposed: phase 2's Save In field.
5. Should pinned and recent follow a file moved in the Finder (macOS bookmarks can), or is
   "can't find this file" with Remove enough?
6. Does New Tab (Cmd+T) stay the instant starter budget, or open the start window too?
