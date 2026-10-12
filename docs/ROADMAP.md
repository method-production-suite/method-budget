# Roadmap

What to build next so a production can move its real budgets off the reference product, then reach
parity, then go past it. Local files only: no cloud, no accounts, no network beyond the app's own update checks (decisions 14 and 48).
Every cloud or sharing feature below has a local-first stand-in.

Sources are cited by behavior note number (`01` to `24` in `docs/research/behaviors/`),
`inv` (sections of the feature inventory in `docs/research/`), `ETP` (export test plan),
`APP` (the "Not yet" list in `docs/APP.md`) and `D` (entries in `docs/decisions.md`).
Notes 01 and 02 are 2026 recordings and win over the 2020/2021 tutorials (03 to 24).

Status: **[have]** built, **[partial]** some of it built (engine only, display only, or
missing pieces), **[missing]** not started. Size: **S** a day or two, **M** about a week,
**L** more than a week.

---

## Where we are

The engine is done for the v1 calculation model: three levels (topsheet, accounts, detail
lines), 4-factor lines with globals and formulas, per-line currency, percent and flat
fringes with per-earner cutoffs and fringe ranges, four posting modes, groups with
suppression, ATL/BTL production levels, contractual charges, credits, baseline and variance,
account coding, plus named versions and a 2-to-16-budget compare, all in integer cents with
property tests. Budgets save as our own documented, diff-friendly `.mthd` JSON, and a small
`method` command line can show, check, tidy, create and export them (Excel, CSV, tab-delimited). The Mac desktop app (Tauri, no
npm) edits a budget at all three levels with breadcrumb jumps, account stepping, a column
menu, cell copy and paste as tab-separated text, fill down, multi-row selection, 200-step
undo, a setup panel for every table (globals, fringes, groups, units, currencies, sets,
locations, charges, credits, info, settings, baseline), the selection-driven Apply panel
(tags, dollars, fringe ranges), footer totals with session and last-change counters, and
printing of the topsheet and detail through the system print dialog. It stays fast at 5,000
lines. Not there yet: autosave, recent files and opening a `.mthd` from Finder, any way to
set bold, italic, indent, color or a note from the app (the commands exist, the UI only
shows them), versions and compare screens, find and replace, copying rows with their tags,
a right-click menu, split view (budgets now open as tabs), report choices beyond "topsheet" or "all",
signed installers, and import from the reference product's own exports (waiting on the export test;
the generic spreadsheet import, 2.2, is built and can carry it).

---

## Decisions needed from the owner

**Decided 2026-10-09 (the owner):** 1 compare math: **both** (each budget vs base, plus optional
Sum/Average/Variance columns); 2 setup keys: **adopt the reference's**, shortcut sheet moves to
Shift+Cmd+/; 8 more than one budget: **tabs in one window** (with split view later), not one
window per budget; 9 autosave: **always save into the file** (on by default, with a toggle),
recovery copies for untitled budgets and rolling backups. The rest below are still open; the
recommendation stands as the default until the owner says otherwise.

1. **Compare math.** The reference shows each linked budget, their sum and average, then
   "variance" = sum of the others minus this budget and "%" = variance over the average (21).
   Ours shows each budget's own difference from the base and percent of the base (D17).
   Options: ours only; the reference's only; ours plus optional Sum and Average columns.
   **Recommend: ours plus optional Sum and Average**, and the reference's variance only if
   a 2026 check shows it actually gets read.
2. **Keys that clash.** The reference's setup keys are Cmd+3 fringes, Cmd+4 groups, Cmd+5
   locations, Cmd+6 currency, Cmd+7 units, Cmd+8 sets, Cmd+9 preferences, Cmd+G globals,
   Cmd+\ credits, Cmd+/ charges (09, 14, 17). Ours: Cmd+1 to Cmd+9 in our own order and Cmd+/
   for the shortcut sheet. Also wanted: Cmd+Left/Right sibling steps (03), Cmd+R and
   Shift+Cmd+R ranges, Cmd+minus adjustment, Shift+Cmd+Space note (02), Cmd+B bold (20).
   Options: keep ours; adopt theirs; adopt theirs with ours as aliases. **Recommend: adopt
   theirs** (same reasoning as D15), move the shortcut sheet to Shift+Cmd+/ (Cmd+?), keep
   Cmd+[ and Cmd+] as aliases. Confirm the 2026 keys first (see Recording wishlist 1).
3. **Budget-wide group rules.** The reference lets each budget choose: do ungrouped lines
   count, does a line with one "out" and one "in" group count, do group totals include
   fringes (14, 24). We hardcode the reference's defaults. Options: keep hardcoded; add the
   three settings. **Recommend: keep hardcoded** until an imported budget turns up with a
   non-default setting; the format can take it later without breaking files.
4. **Group "full" total.** **Decided 2026-10-11 (D58)**: matched to the reference, every
   tagged line at its base plus only the fringes earned (a suppressed line adds its base
   alone).
5. **Suppressed lines in detail subtotal rows.** Ours count them as 0 everywhere. One
   tutorial frame suggests the reference's detail subtotal and total rows still showed them
   (14, unverified). **Recommend: keep ours** unless the 2026 check (Engine check 2) shows
   otherwise; if it does, match it on screen only, never in rollups.
6. **Separator with a label.** Today a separator is a bare rule with nothing to edit (D16).
   Options: bare rule; rule with optional centered text. **Recommend: bare rule**; bold
   title lines already do the labeling job. Cheap to add later.
7. **Renaming setup items.** We reference globals, fringes, groups, units and so on by name
   (readable diffs); the reference uses hidden ids (09). Options: rename rewrites every
   reference as one undo step; switch the file to ids. **Recommend: rename rewrites
   references** (token-exact for formulas), keep names in the file.
8. **More than one budget open.** Options: tabs in one window with split screen (04); one
   window per budget. **Recommend: one window per budget**, each with its own undo; side by
   side comes free from the OS, and a shared clipboard handles copying between them.
9. **Autosave.** The reference autosaves by default (07, inv 15). Options: save to the file
   on every edit; keep a recovery copy beside it and save to the file only on Cmd+S; a
   setting. **Recommend: always keep a recovery copy (debounced, crash-safe) and offer
   "save to the file automatically" as a per-user setting, off by default**, so a `.mthd`
   in git or a shared folder only changes when someone means it to.
10. **New-line defaults.** The reference copies fringes from the line above (groups, set,
    location, currency off) and starts Amt at 1 (02, 17). Options: match; copy everything;
    copy nothing. **Recommend: match, as per-budget switches saved in the file**, and an
    empty account shows one ready line (03).
11. **Notes model.** One plain note per line today (D16); the reference keeps several dated
    notes per line or account, with author (19). **Recommend: a list of dated notes per
    line, account and category, author from a local name setting, no replies.**
12. **Paste conflicts between budgets.** The reference keeps the target's same-named fringe
    or global, which can change the pasted line's math silently (20). **Recommend: target
    wins by default, but show a review list (what differs, what will be added, the total
    change) before the paste lands.**
13. **Charge row position on the topsheet.** Reference: after the last production total,
    before the ATL/BTL summary rows; ours: after them (23). Math is identical. **Recommend:
    match the reference**, since printed topsheets get read side by side.
14. **Who signs the Mac app.** Notarization needs an Apple Developer ID (INSTALL.md).
    Options: a personal account; a company account (needs a D-U-N-S number); stay
    ad-hoc. **Recommend: an organization account**, so the signer name on the
    installer is an organization, not a person. Windows signing can wait until there is a
    Windows user.
15. **Where presets and setup libraries live.** Options: in the budget file; a per-user
    library file; both. **Recommend: tag presets in the budget (they travel with it); a
    library as its own open file in a folder the user picks; first, "import setup from another
    .mthd"**, which covers most of the value with no new file type (24).

---

## Phase 1: Switch-over ready

What the owner needs to do real work every day. Ordered by priority.

| # | Item | Notes | Status | Size | Depends |
|---|---|---|---|---|---|
| 1.1 | **Never lose work**: autosave into the file (default on, toggle), recovery copy for untitled budgets, rolling backups, restore after a crash, recent files list, open a `.mthd` by double-clicking it (file type is registered; the app ignores the open event) | 07, inv 15, APP | [have] | M | decision 9 |
| 1.2 | **Type-ahead in cells**: units, currencies, sets, locations by prefix; global names in Amt, X, Rate, 4X; Enter or Tab accepts | 02, 07, 09, inv 3, 5 | [have] | M | none |
| 1.3 | **Add on the fly**: typing an unknown unit, currency, set, location or global offers "add it", then applies the edit, one undo step (today an unknown currency is refused) | 02, 07, inv 3, 7 | [have] | S | 1.2 |
| 1.4 | **New-line defaults**: copy fringes (and optionally groups, set, location, currency) from the line above, Amt 1, empty account shows one ready line; per-budget switches in the file | 02, 03, 17, inv 2 | [have] | S | decision 10 |
| 1.5 | **Right-click menus** at each level: cut, copy, paste, insert, delete, subtotal, adjustment, note, text style, make or remove range; topsheet adds production total, charge, credit | 01, 02, 19, 20, 23 | [have] | M | none |
| 1.6 | **Style and note editing in the app**: bold, italic, indent, line color, group color, and a notes editor (side panel, Shift+Cmd+Space, click the note mark) for lines; engine and commands already exist | 01, 19, D16 | [have] | M | 1.5, decision 11 |
| 1.7 | **Shortcut parity pass**: setup keys, Cmd+Left/Right sibling steps on the account list too (stepping categories), range, adjustment, note and bold keys, visible prev/next buttons on the deepest crumb | 01, 02, 03, 09, 14, 17, 19, 20 | [have] | S | decision 2 |
| 1.8 | **Copy and paste rows with their tags**: our own clipboard payload (plus TSV for spreadsheets) carrying fringes, groups, set, location, currency, notes, style and ranges; "paste as new rows" above the cursor; accounts and categories with their lines | 20, inv 18 | [have] | L | 1.9 for cross-budget |
| 1.9 | **Budget tabs in one window** (the owner's call), each tab its own undo; copying between tabs is normal clipboard work; paste conflict review; split view later. Built (decision 25): a tab per budget with its own undo, autosave, recovery and backups; open, new, close with the unsaved question, reorder by drag, Ctrl+Tab, Shift+Cmd+] and [, Cmd+1 and Cmd+2, a Window menu, the tools panel and its window following the tab in front, one question about every unsaved tab on quit, the tabs reopened at the next start. Split view is 2.26; the keys and the close question need a live check | 04, 20, inv 16 | [have] | M | decisions 8, 12 |
| 1.10 | **Move rows**: move up or down by key, cut and paste rows as rows, keep fringe ranges whole. Built (decision 32): Option+Up and Option+Down, Edit > Move Up and Move Down and the right-click menu move the selected lines, accounts or topsheet rows one place, any selection, order kept, quick presses one undo step; a fully selected range moves whole, a line leaving a range never splits it; cut and paste rows is 1.8. Dragging by the # column isn't built; the keys need a live check | APP, inv 18 | [have] | S | 1.8 |
| 1.11 | **Find and replace**: typed search (go to account number, description, fringe, global, group, set, location, unit, currency, notes), results by account with the matching column, jump to each, replace or remove in checked results only, token-exact for globals, one undo step. Built (decision 26): the tools panel's Find tab (Cmd+F, Shift+Cmd+F, F3), every scope above plus formula text, match case and whole word, name suggestions, a global replaced by a global, number or formula, Remove freezing a global's value; renaming a setup item everywhere is not part of it; the keys and the tools window need a live check | 18, inv 5, 17 | [have] | L | none |
| 1.12 | **Versions screen**: save a version, list, rename, add a note, restore (reversible), delete | 01, D17, inv 14 | [have] | M | none |
| 1.13 | **Original and variance columns in the grid** at topsheet and account levels (data already in every view; reports already print it) | inv 14, 17, APP | [have] | S | none |
| 1.14 | **Compare screen**: pick versions of this budget or other `.mthd` files, compare grid on topsheet and account levels (base stays editable), drill to detail lines (our edge), print it | 21, 04, 22, D17 | [have] | L | 1.12, decision 1 |
| 1.15 | **Report sections and PDF**: choose topsheet, category summary, account detail; a tree of categories and accounts; include or leave out suppressed lines and empty accounts; portrait or landscape; page numbers, date and file name in the footer; save straight to PDF | 07, 15, inv 20 | [have] | L | none |
| 1.16 | **Import from the reference product's exports**: XML (Advanced) and JSON first, then Excel; a reconciliation report against the export's own totals, line by line. The generic import (2.2) is built to be reused: the Excel and CSV readers, the mapping, the line placement, the setup creation and the line-by-line reconciliation are there; what is left is a reader per export layout (from real samples, not guessed), a preset mapping for its Excel export, and its setup tables (fringe rates, globals) | inv 21, ETP | [partial] | L | the owner runs the export test |
| 1.17 | **Engine fixes from the export test**: currency direction, cutoff factor, flat fringe rules (done: D54, engine check 6), adjustment rows, charge base, rounding points, whatever the totals disagree on | ETP, inv Q2 to Q11 | [partial] | M | 1.16 samples |
| 1.18 | **Rename setup items** with every reference rewritten (globals token-exact in formulas, fringes, groups, units, sets, locations, currencies). Built (decision 32): the editor's name field, Rename everywhere in the lists' right-click menu, a global's or unit's name, Find's replace panel and the base currency setting; one undo step, says how many places changed; charges, credits, report, tag and line presets follow; the new name must be free (no merging), so renaming back gives the identical budget (property-tested) | 09, APP | [have] | M | decision 7 |
| 1.19 | **Tag presets**: save a line's tags as a named bundle, apply to a selection with add or replace, in the Apply panel, stored in the budget. Built (decision 31): a Presets tab in the tools panel (after Units) and Apply preset > in the right-click menu; Save tags as preset… from the cursor line (a box per tag), edit, rename, delete, reorder (keyboard); Add (fringes and groups joined, set, location and currency only where a line has none) or Replace (the five tags become exactly the preset's) on lines or everything under selected accounts or categories, one undo step, a preset naming a deleted item refused. Needs a live check in the app | 05, inv 13 | [have] | M | decision 15 |
| 1.20 | **Start a budget from another**: "import setup from another .mthd" (pick items, dependencies come along) and "new from template" (a budget with its chart and setup, no lines). Built (decision 29): File > Import Setup From… (every kind of setup item, the presets and the three budget settings, each new, the same or different here, what an item needs ticked along with it, the paste review's keep, imported or new-name choices, one undo step); File > New From Template… (any `.mthd`, opened in a new untitled tab without its lines, info, baseline, versions or notes, "keep the detail lines" optional) and Save as Template… (the same stripping, into a new file). Bundled chart-of-accounts templates are 3.16; the native dialogs need a live check | 24, 07, inv 12 | [have] | M | decision 15 |
| 1.21 | **Signed and notarized Mac build**, no Gatekeeper step. Built as a pipeline (decision 48): the release workflow signs the universal app and disk image with the Developer ID, notarizes them with an App Store Connect API key, staples and checks them with `codesign` and `spctl`, each step skipped cleanly without its secrets (docs/RELEASING.md). The first signed release needs the secrets set | INSTALL.md, RELEASING.md | [have] | S | decision 14 |

Already in place and worth keeping as-is: drill and climb keys and cursor memory (03),
breadcrumb jump lists (01, 03), Apply panel with tri-state boxes, dollars and ranges (01,
06, 13), column menu with pinned # and Total (01, 10), footer context totals and change
counters (01), percent and flat charges with exclusions matched to the cent (11, 23),
live recalc on every setup change (07). A test that toggling a group twice returns every
total to the cent exists in spirit (D4 property tests); keep it explicit (02).

---

### Phase 1 addition (the owner, 2026-10-09)

| # | Item | Source | Status | Size | Depends |
|---|---|---|---|---|---|
| 1.22 | **Tools panel rebuild**: two stacked tab panes (Apply and Tables merged; checkbox applies, double-click edits), drag tabs between panes, dock right (default) / bottom / pop-out window, remembered. Built (decision 24). The Budget tab and every table's full editor moved to a Budget Setup window (toolbar Setup, View > Budget Setup, the setup keys), and the panel's tabs became compact apply lists (decision 42); the pop-out window and Budget Setup need a live check | the owner's mock review, decision 21 | [have] | L | inline editing (in progress) |
| 1.23 | **Updates inside the app**, one version everywhere, release notes. Built (decision 48): background checks (after start, then every six hours; Help > Check for Updates Automatically turns them off) and download, signature checked against the app's own key, "Update ready: Restart to update" in the toolbar, every budget saved and the tabs and split view back after the restart, What's New once after an update and Help > Release Notes; `cargo xtask` checks and bumps the version and drafts notes facts; the release workflow writes the update manifest. Off until the updater key exists and the repository is public; needs a live check of a real update | RELEASING.md, APP.md | [have] | M | 1.21 |

## Phase 2: Parity and polish

| # | Item | Notes | Status | Size | Depends |
|---|---|---|---|---|---|
| 2.1 | **Spreadsheet export**: CSV and Excel of topsheet, accounts and detail with tags; a tab-delimited flavor for accounting tools. Built (decision 30): File > Export (Excel Workbook, CSV Files, Tab-Delimited Detail) and `method export`; one workbook with Topsheet, Accounts, Detail and the setup tables (globals, fringes, groups, units, currencies, sets, locations), money exact from cents, header rows bold and frozen; written without new crates (our own ZIP and SpreadsheetML); opening the workbook in Excel and Numbers needs a live check | inv 21 | [have] | M | none |
| 2.2 | **Spreadsheet import with column mapping** (any department's sheet, not just the reference product's). Built (decision 36): File > Import Spreadsheet… reads .xlsx, CSV and tab-delimited files with our own ZIP, DEFLATE and XML readers (no crates, hostile-file limits); the dialog picks the sheet and header row (both found automatically), a column per field (guessed from the titles), whether to add missing accounts, categories and setup items, the account for rows without one, and this budget (one undo step) or a new tab with its chart and setup; the preview shows the first lines with their totals, skipped rows and why, and a reconciliation against the sheet's line totals; mappings are saved per user and come back for a sheet with the same titles; our own export imports back to the same totals (tested). Needs a live check with real Excel and Numbers files | inv 24 | [have] | M | none |
| 2.3 | **Report layout options**: font size, vertical rules, shading, group colors or black and white, collapse details to one line per account. Built (decisions 28 and 33): type size small, medium or large, shading, group colors, lines between columns, and the detail level as one line per account, all kept in the preset. Point sizes (the reference's 5 to 10 pt) are three steps here | 15, inv 20 | [have] | M | 1.15 |
| 2.4 | **Report templates in the budget**: named, re-rendered from live data, with header and footer zones and live fields (page, dates, time, file, budget info, globals), first-page-only switch. Built (decisions 28 and 33): named presets in the `.mthd` (save, save as new, rename, delete, default), drawn from the budget as it is; three slots per band, each one field (title, section, date and time, date, time, file, page, an info field, a global, text); a first-page-only switch per band. Not built: text and fields mixed in one slot, created and modified dates (the budget keeps none), one header design shared by several presets | 15, inv 20 | [have] | L | 1.15 |
| 2.5 | **Extra report sections**: setup tables, budget info, chart of accounts, group totals, notes report; topsheet fringe column and "detail starts on page N". Built (decisions 28 and 33): fringes, globals, groups with totals, units, sets and locations with totals, currencies, chart of accounts, budget information and the notes list as sections; optional Fringes and Page columns on the topsheet. Charges and credits as their own setup tables are not a section yet (the topsheet lists them) | 15, 19, inv 20 | [have] | M | 1.15 |
| 2.6 | **Live report preview** with page navigation and a section jump list. Built (decisions 28 and 33): the setup dialog's preview is the printed pages, redrawn as choices change, with first, previous, next and last, a page number to type, the page count and a Go to section list | 15 | [have] | M | 2.4 |
| 2.7 | **Compare extras**: optional Sum and Average columns, each production-total row, charges and credits per item, legend of which file is which. Built (decisions 27 and 33): Sum, Average, Variance and Variance % columns, each production total, each charge and credit under its total (matched by name), the column headers naming each letter's budget and the printed comparison's legend, printing and PDF | 21 | [have] | M | 1.14 |
| 2.8 | **Notes, fuller**: several dated notes per line, notes on accounts and categories, a notes list with jump-to (a review checklist). Built (decision 35): dated notes (author from a local name setting, edited time, no replies) on lines, accounts and categories, older files' single note read as one undated note; the Notes tab's cards with add, edit and delete for the cursor's line, account or category; All notes as a checklist (tick off as reviewed, Open only, jump to a line, account or category); Shift+Cmd+Space adds a note at every level; reports print every note dated, with a switch to leave reviewed ones out. Needs a live check in the app | 19, inv 16 | [have] | M | 1.6 |
| 2.9 | **In-use counts and totals in setup tables** (units, globals, fringes, groups, sets, locations), used/unused filter, warn before deleting something in use, click to find uses. Built (decision 35): every setup list (currencies too) shows each item's line count and, where it means something, its dollars across the budget; Show All, Used or Unused; a click on a count runs Find for its uses; deleting something in use says how many lines and offers to take it off them (currency lines back to the base, unit columns blanked) or Cancel. Needs a live check in the app | 06, 09, 12, 14, 18, 24, inv 3 | [have] | M | 1.11 |
| 2.10 | **Globals, fuller**: description, unit, display decimals, "show by name" in cells, formula view while a range is selected. Built (decision 34): formula cells show their value, a global's details (description, unit, decimals, show by name, folder) in the Globals tab, View > Show or Hide Formulas in Selection (Option+Cmd+F). Folders don't group the list yet. Needs a live check in the app | 02, 09, 17, inv 5 | [have] | M | none |
| 2.11 | **Short codes (ID) for fringes and groups**, shown in the grid's tag columns. Built (decision 35): an optional short code per fringe and group (unique ignoring case, not another item's name), set in the item's editor or its right-click menu, shown in the list, in the grid's Fringes and Groups columns (names in the tooltip) and matched by the type-ahead; display only, so renames are unaffected. Needs a live check in the app | 12, 14, inv 4, 6 | [have] | S | none |
| 2.12 | **Custom order for setup tables** (drag, saved in the file, followed by reports and the Apply panel), click-to-sort headers that cycle back to custom, no dragging while filtered. Built (decision 35): a hand-made order per table in the file (`setup_order`, left out when alphabetical), set by dragging an item's handle or Option+Shift+Up and Down; followed by every setup list, the type-ahead and printed setup tables; the Name head cycles A to Z, Z to A and back; no moving while sorted or filtered. Folders are not built. Needs a live check in the app | 12, 16, inv 12 | [have] | M | none |
| 2.13 | **Apply panel extras**: clear all per list, replace all tags, per-group line count across the budget, fringe table footer (percent, flat, total). Built (decision 38): Clear on the fringe and group lists, Replace tags… under the selection's totals (a tag-preset-style form, starting from what every selected line shares), the fringe list's foot split into percent, flat and total; the per-group line count across the budget was already there (the Lines column, decision 35). Needs a live check in the app | 05, 06, 11, 12, 13 | [have] | S | none |
| 2.14 | **Line-block presets**: save lines (with a checklist of fields) and insert copies at the cursor, one undo step. Built (decision 31): Save lines as block… (keep amounts, units, tags, notes; descriptions, kinds and styles always), Insert above the cursor line from the Presets tab or Insert line block > in the right-click menu, new ids and fringe ranges, the block's own copy of its setup offered back through the paste review when the budget lost an item. Needs a live check in the app | 05, inv 13 | [have] | M | 1.19 |
| 2.15 | **Personal setup library**: an open library file in a folder the user picks, folders per kind, copy in and out with dependencies and a conflict review, no live link. Built (decision 37): the library is a `.mthd` with setup and no lines (`My Method Budget Library.mthd` made in a folder picked in Rust's dialog, or any budget file; remembered per user; switchable), starting empty; a Library tab in the tools panel (second pane, draggable) browses it by kind and folder with a filter and each item's definition; Add to This Budget and Save to Library (the tab, or a setup list's right-click menu) run Import Setup's review both ways (dependencies along; the target keeps its version unless the user picks the other or a new name), adding being one undo step; folders made, renamed, deleted and items moved, kept in the file as `setup_folders`; written atomically; while the library is open as a tab it is read from there and changes from the Library tab are refused. Needs a live check of the dialogs and the tab in the app | 07, 09, 11, 24, inv 12 | [have] | L | 1.20, decision 15 |
| 2.16 | **Charges and credits polish**: percent and excluded amount in the topsheet label, exclusion grid with live group totals, preview that includes the cell being typed, credit rows showing gross, excluded fringes, net and variance to cap, "create charge" from the topsheet menu. Built (decision 38): two label switches per charge (saved in the budget, screen and printed topsheet), every group with a box and its amount in the base, a live total from the typed text (`preview_charge`), credit row figures with the cap variance, Create charge… and Create credit… on the topsheet's right-click menu. Needs a live check in the app | 11, 23, inv 9, 10 | [have] | M | 1.5 |
| 2.17 | **Two tools at once and dock at the bottom**, so the grid gets full width beside a setup list (done with 1.22) | 01, 04, 06, inv 18 | [have] | M | none |
| 2.18 | **Display preferences**: rate decimals with a cutoff band, totals decimals, hide zero accounts, show or hide summary rows and the net total, grid zoom. Built (decision 34): rate band and summary rows in Budget > Settings (saved in the budget); totals' cents, zero accounts and zoom in the View menu (per user). Screen only: reports keep their own options. Needs a live check in the app | 17, inv 1, 18, 22 | [have] | M | none |
| 2.19 | **Hidden-factor marker**: flag a line whose hidden X, 4X, unit or currency column changes its total. Built (decision 34): a dot in the # cell naming the hidden X, 4X or currency; unit columns never change a total, so they never mark. Needs a live check in the app | inv 2, 23 | [have] | S | none |
| 2.20 | **Renumber accounts when the coding pattern changes**. Built (decision 39): Apply in Settings > Account numbers previews old and new numbers (categories, accounts, fringe accounts) and offers to renumber, one undo step, rewriting charges, credits, report presets and baseline keys; conflicts refuse. Needs a live check in the app | APP, inv 11 | [have] | M | none |
| 2.21 | **Share-ready copies**: save a snapshot named by title, version and date, optionally without notes or private info, stamped with where it came from; open read-only; offer to compare a returned copy or store it as a version. Built (decision 39): File > Save a Copy for Sharing…, `provenance` in the file, read-only banner with Make Editable Copy, Compare With Original and Store as a Version. Needs a live check in the app | 22, inv 16 | [have] | M | 1.14 |
| 2.22 | **Sub-budgets**: a filtered copy by categories, accounts and tags, optionally without fringes or notes. Built (decision 39): File > New Sub-Budget…, groups, sets and locations OR within a kind and AND across kinds, new untitled tab stamped with its parent; no fringe lock yet. Needs a live check in the app | 04, inv 14 | [have] | M | none |
| 2.23 | **History list**: every undo step listed, jump back, or branch it into a version. Built (decision 40): every undo step named by its command ("Edit rate on 21-01 line 3", "Paste 12 rows") with its time; the History tab (bottom tools pane) lists them oldest first, a click undoes or redoes to a step, "Version…" saves a step as a named version without changing the budget. Needs a live check (the tab in the tools window, long histories) | inv 15 | [have] | M | 1.12 |
| 2.24 | **Group rules as budget settings** (only if decision 3 changes) | 14, 24 | [missing] | M | decision 3 |
| 2.25 | **Windows and Linux live check**: printing, menu keys, installers, file association | APP | [partial] | M | none |
| 2.26 | **Split view**: two budget tabs side by side in one window, each pane with its own tab strip, breadcrumb and grid; one pane is the target (dot and bold tab) and the toolbar and tools act on it; move a tab to the other pane; exit merges the panes. Built (decision 40): View > Split Right (Shift+Cmd+\\) and Close Split; each pane its own tabs, breadcrumb, grid and footer; the target (accent on its strip, its tab bold) is what the toolbar, menus, keys, tools panel and formula bar act on; one budget can show in both panes; Move Tab to Other Pane (Window menu, a tab's right-click menu) or drag a tab onto the other strip; a resizable divider; the panes kept in tabs.json. Needs a live check (the keys and menus in the system web views, the tools window following the target) | 04 | [have] | M | 1.9 |

---

## Phase 3: Beyond the reference

Where competitors and the pain points (inv "Gaps and pain points", inv 24) say we can win.

| # | Item | Why | Status | Size | Depends |
|---|---|---|---|---|---|
| 3.1 | **Budget diff**: `method diff` and an in-app "what changed" between two files or versions, by row id and account code, in plain words with money deltas. Built (decision 44; docs/MERGE.md): `method_core::diff` matches rows by id, accounts and categories by number (renumbered ones by what they hold), setup items, charges and credits by name, and lists every change with its fields before and after and a sentence (`21-01 line 3: rate 1,200.00 -> 1,350.00 (+1,500.00 to 21-01)`), plus the account, category and topsheet money that moved; `method diff a b [--json] [--exit-code]`; File > What Changed… against the saved file, a version, an open budget or a file, grouped by account, a click going to the change. Needs a live check in the app | no real collaboration in the reference; our files are text | [have] | M | 1.14 |
| 3.2 | **Three-way merge**: `method merge` plus a git merge driver, merging edits to different lines automatically and listing real conflicts for a person to pick; turns "send a copy" forks into merges. Built (decision 44; docs/MERGE.md): `method_core::merge` merges different rows, fields and setup items on their own (tags name by name, notes note by note), lists same-field clashes, deleted-versus-changed rows, items, accounts and categories and both-side reorders as conflicts (ours by default, a pick per conflict), keeps rows both sides added with one id as two; property-tested; `method merge base ours theirs -o out [--theirs]` (exit 1 with conflicts) and `method merge-driver %O %A %B %P` for git; File > Merge Changes From… with the base found from a shared copy's provenance (worked out as it was sent), conflicts picked per item, one undo step. Needs a live check in the app | the reference only sends divergent copies | [have] | L | 3.1 |
| 3.3 | **Review files**: a reviewer marks up a read-only copy, comments land in a small sidecar file the author can merge back | local stand-in for shared comments | [missing] | M | 2.21 |
| 3.4 | **Version deltas** so many versions don't bloat the file. Built (decision 41): a budget-level `compact_versions` switch (off by default, so every other file is unchanged and older readers still read it) writes each version as a JSON Patch against the next version (the newest against the budget), keeping a full copy where a delta isn't smaller; reading rebuilds full snapshots either way. Ten versions of the 5,000-row sample: 2.0 MB instead of 26 MB (the budget alone is 2.0 MB). `method set f.mthd compact_versions true` turns it on; no app switch yet | D17 | [have] | M | none |
| 3.5 | **"Explain this number"**: trace any total down to lines, fringes, cutoffs, ranges and rounding steps. Built (decision 43): `method_core::explain` works any total out again from the budget as a tree of steps whose leaves add up to it exactly (tested on every total of the 5,000-line sample, the hand-checked fixtures, every posting mode and filter): lines with globals by name and value, currency, every rounding, fringes per line with cutoffs, ranges and largest-remainder shares, posting, left-out lines and why, charge bases and exclusions, credit gross, excluded, net and caps. In the app: right-click a total (any level, total rows too) > Explain This Number…, or Cmd+E on the cursor cell, opens the Explain tab (collapsible tree, deep steps fetched when opened, a click goes to the line); `method explain file.mthd <total>` on the command line. Needs a live check in the app | opaque fringe math and rounding complaints | [have] | M | none |
| 3.6 | **Incentive modeling**: per-location qualified spend, several programs side by side, caps and broker discounts, what-if by moving lines between locations; programs authored by the user from public rules, never bundled vendor data | locations and credits exist; no one makes this easy | [missing] | L | 2.15 |
| 3.7 | **Actuals and cost reports**: import actuals (CSV), budget vs actual vs estimate to complete by account. Built (decision 45): actuals kept in the budget (`actuals`: transactions, import batches, per-account ETC methods; not content, so versions don't carry them); File > Import Actuals… reads CSV, tab-delimited and Excel exports with a column mapping (date, account, amount or debit and credit, vendor, description, reference, tags; guessed, saved per user), dates of every common shape, account numbers through the budget's numbering, a review of numbers the chart lacks (keep unassigned, skip, map) and of duplicates (date, account, amount, description), one undo step and one removable import; `method_core::actuals` works out actual, ETC (budget minus actual, manual, percent complete), EAC, variance and percent spent per account, category fringe account, category, level and the topsheet exactly to the cent, with an unassigned bucket (property-tested); the grid's Actual, ETC, EAC, EAC Var and % Spent columns on the topsheet and account list (Actual on lines), over-budget marked; the Actuals tab (filter, edit, reassign, delete, remove an import, set an account's ETC); a Cost report print section and Cost Report and Actuals export sheets. The rollup over the transactions is cached, so an edit costs the same with 20,000 transactions. Needs a live check with real accounting exports | top competitor gap | [have] | L | 2.2 |
| 3.8 | **Schedule-linked quantities**: read a schedule's day counts from an open export and drive globals. Agreed input: the `sched-days` exchange file written by Method Scheduling (`sched budget FILE`, spec in that repo's docs/EXCHANGE.md); globals keep their source as (category, board ID, name) with the element id as fallback, a performer's days default to the DOOD total, a missing element counts 0 with a warning | competitor feature; the reference only imports static blocks | [missing] | L | 2.2 |
| 3.9 | **Rate tables**: user-kept tables from public union rates; pick a classification to fill the rate (flag the legal line before shipping any data) | competitor add-on | [missing] | L | 2.15 |
| 3.10 | **Overtime helper** for labor lines. Built (decision 43): the tools panel's Overtime tab (also Edit > Overtime Helper… and the line's right-click menu) on the cursor line: straight-time rate, hours a day, straight-time hours, overtime bands, sixth and seventh day multipliers, days a week; shows a day and a week exactly (pay per multiplier, straight-time equivalent hours, average an hour) and writes the day's or week's pay into the line as its rate or as one line per multiplier in one fringe range, one undo step. Patterns saved by name in the budget (`ot_patterns`, tools, not content). No union rules shipped. Needs a live check in the app | competitor feature | [have] | M | none |
| 3.11 | **Person-level cutoffs across accounts**, opt-in, for one person paid in several accounts | inv 23 quirk | [missing] | M | engine check 1 |
| 3.12 | **Scriptable command line**: batch edits, headless PDF and spreadsheet output for pipelines. Built (decision 41, docs/CLI.md): `method report` (a saved preset as HTML, text, CSV or a plain Courier PDF from our own small writer), `method export --xlsx/--csv/--tsv`, `method totals [--json]`, `method set` and `method apply` (a line-oriented edit script: globals, fringe rates and cutoffs, line fields by row id or account and line, tags, renames, charges; atomic, all or nothing, `--dry-run` with a diff), `method import` with a JSON column mapping; documented exit codes. The app's typeset PDF stays Mac-app only | open format advantage | [have] | M | 2.1, 1.15 |
| 3.13 | **Commercial bid layouts** (industry bid form style) | competitor feature; check the legal line on form layouts | [missing] | L | 2.4 |
| 3.14 | **Localization and accessibility pass** | reference is English only | [missing] | M | none |
| 3.15 | **Setup packs**: import and export sets of setup items (globals, groups, fringes, units, sets, locations, currencies, charges, credits, tag presets, overtime patterns) as shareable packs with their sources, and setup lists as spreadsheets. Built (decision 50): a pack is a `.mthd` with setup and no lines, provenance `setup_pack` with a title, description, author, version, effective date, region, notes and links; File > Export Setup Pack… and the Library tab's Setup packs section pick items by kind and folder with their dependencies; File > Import Setup Pack… and the listed packs (your Packs folder and the bundled `packs/`, empty but for a README) run Import Setup's review into a budget (one undo step) or the library, stamping what comes in (`setup_sources`); File > Export > Setup Lists… and Import Setup Lists… (a sheet per kind, checked row by row); `method pack export`, `import` and `show`. Rate packs come from a separate data project after review. Needs a live check of the dialogs and the bundled folder | shareable setup, 2026-10-10; 3.9 | [have] | L | 1.20, 2.15 |
| 3.16 | **Charts of accounts**: bundled chart templates and importing any chart. Built (decision 51; docs/TEMPLATES.md): File > Import Chart of Accounts… and `method coa import` read a chart from any sheet of a workbook, CSV, tab-delimited text or another budget (flat lists, outlines, side-by-side columns, number and name in one cell, separators or none, lost leading zeros, header, total and level rows, the `kind,code,name,parent` layout), infer the numbering pattern (changeable) and preview with duplicates and gaps; into a new budget with no lines, or applied to this one (adds, renames by number, never removes an account with lines, one undo step). Seven charts written by us and the studio charts built from the owner's exports ship in New From Template under Charts of accounts and in `method new --template`; the sources sit in `imports/coa-sources/` (never committed) and are built by `scripts/build-coa-templates.py`. Needs a live check of the dialogs in the app | top request: start from the studio's chart | [have] | L | 2.2, 1.20 |
| 3.17 | **Agents can work on budgets**: a Model Context Protocol server so Claude Code and other coding agents open, read, edit, check and report on budgets with typed tools. Built (decision 52; docs/MCP.md): `method mcp [--root DIR]... [--read-only]` over stdio, both protocol eras (the `initialize` handshake from 2024-11-05 to 2025-11-25, and 2026-07-28 per-request metadata); 43 tools: open and summarize, topsheet, accounts, lines with what they work out to, globals, setup tables, charges and credits, totals, explain, find, validate, diff, merge, add, update, delete, move and tag lines, setup add, change, rename everywhere and delete with a use check, charges and credits, categories, production totals and accounts, export and reports, spreadsheet import, new budgets from the starter or a template, copies and undo; every path inside a root (symlinks out, hidden folders and wrong extensions refused), atomic writes, a change refused when the file changed on disk since the server read it, shared copies read-only, input limits; an example agent skill in docs/agents/. A live bridge to the running app (edits through the session, in History and undo) is designed in docs/MCP.md, not built | agents should push the buttons too | [have] | L | 3.12 |

Skipped on purpose: accounts and sign-in, cloud sync, a send-to-user inbox, live currency
rates (no network), vendor template or rate data, other products' native file formats.

---

## Engine checks to run

Semantic questions only a licensed copy can settle. Most are covered by the export test
(ETP); the quick test is for settling one on its own in a few minutes.

| # | Question | Our rule today | Quick test | In ETP |
|---|---|---|---|---|
| 1 | Is a percent cutoff per fringe range? | per earner: a line or a range (D3) | Three lines of 1,000 each, a 12.5% fringe with a 2,000 cutoff. Fringe total with them bracketed into a range: ours says 250. Unbracketed: 375 | yes (7201) |
| 2 | Do detail subtotal and total rows drop suppressed lines? | yes, they count as 0 everywhere | Subtotal row under two lines, suppress one line's group, read the subtotal row and the account total row | no |
| 3 | Does toggling a group off and on drift the total? | never; exact to the cent | In a real budget note the net total, suppress a group, restore it, read the net total and "last change" again | no |
| 4 | Currency direction | rate = base units per one foreign unit (D10) | One line of 100 with currency EUR at rate 3. Total 300 means ours; 33.33 means the other way | yes (7301, V3) |
| 5 | What does the percentage cutoff factor do? | not modeled | Export test variation V2 (factor 2.0) and read the driver line's fringe against the table | yes (V2) |
| 6 | Flat fringes: once per line or per range; do they multiply on non-time units? | **Answered 2026-10-11 (D54)**: per line; the Amount only, converted through unit hours; X and 4X never count; once when either unit has no hours. Built, with an optional basis (amount plus counts, or the full quantity) and most hours a day | Export test grip and driver kit lines | yes |
| 7 | Adjustment rows: can they be fringed, do they include fringes above, do they restart at a subtotal? | no, no, no | Export test account 7102 plus screenshot 19 | yes |
| 8 | Charge base and stacking | ATL + BTL with posted fringes, no stacking, flat fee minus exclusions | Export test ZPCTCHG and ZFLATCHG, and V1 for post-by-budget | yes |
| 9 | Line with one "out" group and one "in" group | out | Export test Option B lens; or tag one line with both and read its total | yes |
| 10 | Do suppressed lines inside a range still count toward the cutoff? | no | Bracket three lines that cross a cutoff, suppress the middle one, read the fringe on the other two | no |
| 11 | Where does rounding happen? | once per line, half away from zero (D1, D2) | Export test Polish line (1,296.315): 1,296.32 or 1,296.31 | yes |
| 12 | Group "total" column after suppression | **Answered 2026-10-11 (D58)**: base of every tagged line plus only the fringes earned (ZOFF 5,000.00 total, 0.00 active). Built | Open the groups setup in 2026 and read total vs active total for a suppressed group | yes |
| 13 | Does a global's unit do anything? | not modeled | Give a global the unit "week", type it into a line's Amt, see whether the line's unit fills in | no |
| 14 | 2026 new-line and preference defaults | none yet | Screenshot budget preferences in a fresh blank budget (ETP step 1 does this) | yes |
| 15 | Fringe percent shown on a suppressed line | the counted value | Turn on the Agg column, suppress a fringed line, read its Agg | no |
| 16 | Next account at the end of a category | wraps across categories | On the last account of a category, press the next-account step | no |

---

## Recording wishlist

Short clips (1 to 3 minutes, the Z test budget is fine, no real figures needed) that fill
gaps the 2020/2021 tutorials leave or that need 2026 confirmation. In priority order.

1. **Two minutes of real keystrokes** with an on-screen key display: moving, inserting,
   setup keys, ranges, notes. Settles decision 2 and item 1.7 in one go.
2. **Reports in 2026**: open report setup, pick sections and accounts, change orientation,
   add a page number, export a PDF. Shapes 1.15 and 2.3 to 2.6.
3. **Compare in 2026**: link one budget, then two, and hover or read the variance and
   percent columns on a row whose numbers are easy. Settles decision 1.
4. **Versions and history**: make a version, look at the history list, revert a step, branch
   one into a version. Shapes 1.12 and 2.23.
5. **Copy rows between two budgets** where the target has a same-named fringe with a
   different rate and lacks a group the rows use. Settles decision 12 and fills note 20's gaps.
6. **Find and replace a global and a fringe** in 2026, including the remove action and what
   a cell holds afterwards. Shapes 1.11.
7. **Suppression detail**: suppress a group with a subtotal row under the lines, then open
   the groups setup to show total and active total. Settles engine checks 2 and 12.
8. **Saving and opening**: the autosave switch, closing with unsaved work, saving a local
   file, opening a recent budget. Shapes 1.1 and decision 9.
