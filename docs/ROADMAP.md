# Roadmap

What to build next so Sam can move his real budgets off the reference product, then reach
parity, then go past it. Local files only: no cloud, no accounts, no network (decision 14).
Every cloud or sharing feature below has a local-first stand-in.

Sources are cited by behavior note number (`01` to `24` in `docs/research/behaviors/`),
`inv` (sections of `docs/research/mmb-feature-inventory.md`), `ETP` (export test plan),
`APP` (the "Not yet" list in `docs/APP.md`) and `D` (entries in `docs/decisions.md`).
Notes 01 and 02 are Sam's own 2026 recordings and win over the 2020/2021 tutorials (03 to 24).

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
property tests. Budgets save as our own documented, diff-friendly `.fwb` JSON, and a small
`fwb` command line can show, check, tidy and create them. The Mac desktop app (Tauri, no
npm) edits a budget at all three levels with breadcrumb jumps, account stepping, a column
menu, cell copy and paste as tab-separated text, fill down, multi-row selection, 200-step
undo, a setup panel for every table (globals, fringes, groups, units, currencies, sets,
locations, charges, credits, info, settings, baseline), the selection-driven Apply panel
(tags, dollars, fringe ranges), footer totals with session and last-change counters, and
printing of the topsheet and detail through the system print dialog. It stays fast at 5,000
lines. Not there yet: autosave, recent files and opening a `.fwb` from Finder, any way to
set bold, italic, indent, color or a note from the app (the commands exist, the UI only
shows them), versions and compare screens, find and replace, copying rows with their tags,
a right-click menu, more than one budget open, report choices beyond "topsheet" or "all",
signed installers, and import from the reference product (waiting on the export test).

---

## Decisions needed from Sam

**Decided 2026-10-09 (Sam):** 1 compare math: **both** (each budget vs base, plus optional
Sum/Average/Variance columns); 2 setup keys: **adopt the reference's**, shortcut sheet moves to
Shift+Cmd+/; 8 more than one budget: **tabs in one window** (with split view later), not one
window per budget; 9 autosave: **always save into the file** (on by default, with a toggle),
recovery copies for untitled budgets and rolling backups. The rest below are still open; the
recommendation stands as the default until Sam says otherwise.

1. **Compare math.** The reference shows each linked budget, their sum and average, then
   "variance" = sum of the others minus this budget and "%" = variance over the average (21).
   Ours shows each budget's own difference from the base and percent of the base (D17).
   Options: ours only; the reference's only; ours plus optional Sum and Average columns.
   **Recommend: ours plus optional Sum and Average**, and the reference's variance only if
   Sam's 2026 check shows he actually reads it.
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
4. **Group "full" total.** Ours is "as if nothing were suppressed"; the reference's total
   shrinks with suppressed lines' fringes (14). **Recommend: keep ours, label it "if
   counted"**. Display only, no math change.
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
   "save to the file automatically" as a per-user setting, off by default**, so a `.fwb`
   in git or a shared folder only changes when Sam means it to.
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
    Options: Sam personally; a Darkwell company account (needs a D-U-N-S number); stay
    ad-hoc. **Recommend: a Darkwell organization account**, so the signer name on the
    installer is the project, not a person. Windows signing can wait until there is a
    Windows user.
15. **Where presets and setup libraries live.** Options: in the budget file; a per-user
    library file; both. **Recommend: tag presets in the budget (they travel with it); a
    library as its own open file in a folder Sam picks; first, "import setup from another
    .fwb"**, which covers most of the value with no new file type (24).

---

## Phase 1: Switch-over ready

What Sam needs to do real work every day. Ordered by priority.

| # | Item | Notes | Status | Size | Depends |
|---|---|---|---|---|---|
| 1.1 | **Never lose work**: autosave into the file (default on, toggle), recovery copy for untitled budgets, rolling backups, restore after a crash, recent files list, open a `.fwb` by double-clicking it (file type is registered; the app ignores the open event) | 07, inv 15, APP | [partial] | M | decision 9 |
| 1.2 | **Type-ahead in cells**: units, currencies, sets, locations by prefix; global names in Amt, X, Rate, 4X; Enter or Tab accepts | 02, 07, 09, inv 3, 5 | [missing] | M | none |
| 1.3 | **Add on the fly**: typing an unknown unit, currency, set, location or global offers "add it", then applies the edit, one undo step (today an unknown currency is refused) | 02, 07, inv 3, 7 | [missing] | S | 1.2 |
| 1.4 | **New-line defaults**: copy fringes (and optionally groups, set, location, currency) from the line above, Amt 1, empty account shows one ready line; per-budget switches in the file | 02, 03, 17, inv 2 | [missing] | S | decision 10 |
| 1.5 | **Right-click menus** at each level: cut, copy, paste, insert, delete, subtotal, adjustment, note, text style, make or remove range; topsheet adds production total, charge, credit | 01, 02, 19, 20, 23 | [missing] | M | none |
| 1.6 | **Style and note editing in the app**: bold, italic, indent, line color, group color, and a notes editor (side panel, Shift+Cmd+Space, click the note mark) for lines; engine and commands already exist | 01, 19, D16 | [partial] | M | 1.5, decision 11 |
| 1.7 | **Shortcut parity pass**: setup keys, Cmd+Left/Right sibling steps on the account list too (stepping categories), range, adjustment, note and bold keys, visible prev/next buttons on the deepest crumb | 01, 02, 03, 09, 14, 17, 19, 20 | [partial] | S | decision 2 |
| 1.8 | **Copy and paste rows with their tags**: our own clipboard payload (plus TSV for spreadsheets) carrying fringes, groups, set, location, currency, notes, style and ranges; "paste as new rows" above the cursor; accounts and categories with their lines | 20, inv 18 | [missing] | L | 1.9 for cross-budget |
| 1.9 | **Budget tabs in one window** (Sam's call), each tab its own undo; copying between tabs is normal clipboard work; paste conflict review; split view later | 04, 20, inv 16 | [missing] | M | decisions 8, 12 |
| 1.10 | **Move rows**: move up or down by key, cut and paste rows as rows, keep fringe ranges whole | APP, inv 18 | [missing] | S | 1.8 |
| 1.11 | **Find and replace**: typed search (go to account number, description, fringe, global, group, set, location, unit, currency, notes), results by account with the matching column, jump to each, replace or remove in checked results only, token-exact for globals, one undo step | 18, inv 5, 17 | [missing] | L | none |
| 1.12 | **Versions screen**: save a version, list, rename, add a note, restore (reversible), delete | 01, D17, inv 14 | [partial] | M | none |
| 1.13 | **Original and variance columns in the grid** at topsheet and account levels (data already in every view; reports already print it) | inv 14, 17, APP | [partial] | S | none |
| 1.14 | **Compare screen**: pick versions of this budget or other `.fwb` files, compare grid on topsheet and account levels (base stays editable), drill to detail lines (our edge), print it | 21, 04, 22, D17 | [partial] | L | 1.12, decision 1 |
| 1.15 | **Report sections and PDF**: choose topsheet, category summary, account detail; a tree of categories and accounts; include or leave out suppressed lines and empty accounts; portrait or landscape; page numbers, date and file name in the footer; save straight to PDF | 07, 15, inv 20 | [partial] | L | none |
| 1.16 | **Import from MMB exports**: XML (Advanced) and JSON first, then Excel; a reconciliation report against the export's own totals, line by line | inv 21, ETP | [missing] | L | Sam runs the export test |
| 1.17 | **Engine fixes from the export test**: currency direction, cutoff factor, flat fringe rules, adjustment rows, charge base, rounding points, whatever the totals disagree on | ETP, inv Q2 to Q11 | [partial] | M | 1.16 samples |
| 1.18 | **Rename setup items** with every reference rewritten (globals token-exact in formulas, fringes, groups, units, sets, locations, currencies) | 09, APP | [missing] | M | decision 7 |
| 1.19 | **Tag presets**: save a line's tags as a named bundle, apply to a selection with add or replace, in the Apply panel, stored in the budget | 05, inv 13 | [missing] | M | decision 15 |
| 1.20 | **Start a budget from another**: "import setup from another .fwb" (pick items, dependencies come along) and "new from template" (a budget with its chart and setup, no lines) | 24, 07, inv 12 | [missing] | M | decision 15 |
| 1.21 | **Signed and notarized Mac build**, no Gatekeeper step | INSTALL.md | [partial] | S | decision 14 |

Already in place and worth keeping as-is: drill and climb keys and cursor memory (03),
breadcrumb jump lists (01, 03), Apply panel with tri-state boxes, dollars and ranges (01,
06, 13), column menu with pinned # and Total (01, 10), footer context totals and change
counters (01), percent and flat charges with exclusions matched to the cent (11, 23),
live recalc on every setup change (07). A test that toggling a group twice returns every
total to the cent exists in spirit (D4 property tests); keep it explicit (02).

---

## Phase 2: Parity and polish

| # | Item | Notes | Status | Size | Depends |
|---|---|---|---|---|---|
| 2.1 | **Spreadsheet export**: CSV and Excel of topsheet, accounts and detail with tags; a tab-delimited flavor for accounting tools | inv 21 | [missing] | M | none |
| 2.2 | **Spreadsheet import with column mapping** (any department's sheet, not just MMB's), reusing the 1.16 readers | inv 24 | [missing] | M | 1.16 |
| 2.3 | **Report layout options**: font size, vertical rules, shading, group colors or black and white, collapse details to one line per account | 15, inv 20 | [missing] | M | 1.15 |
| 2.4 | **Report templates in the budget**: named, re-rendered from live data, with header and footer zones and live fields (page, dates, time, file, budget info, globals), first-page-only switch | 15, inv 20 | [missing] | L | 1.15 |
| 2.5 | **Extra report sections**: setup tables, budget info, chart of accounts, group totals, notes report; topsheet fringe column and "detail starts on page N" | 15, 19, inv 20 | [partial] | M | 1.15 |
| 2.6 | **Live report preview** with page navigation and a section jump list | 15 | [missing] | M | 2.4 |
| 2.7 | **Compare extras**: optional Sum and Average columns, each production-total row, charges and credits per item, legend of which file is which | 21 | [partial] | M | 1.14 |
| 2.8 | **Notes, fuller**: several dated notes per line, notes on accounts and categories, a notes list with jump-to (a review checklist) | 19, inv 16 | [partial] | M | 1.6 |
| 2.9 | **In-use counts and totals in setup tables** (units, globals, fringes, groups, sets, locations), used/unused filter, warn before deleting something in use, click to find uses | 06, 09, 12, 14, 18, 24, inv 3 | [partial] | M | 1.11 |
| 2.10 | **Globals, fuller**: description, unit, display decimals, "show by name" in cells, formula view while a range is selected | 02, 09, 17, inv 5 | [partial] | M | none |
| 2.11 | **Short codes (ID) for fringes and groups**, shown in the grid's tag columns | 12, 14, inv 4, 6 | [missing] | S | none |
| 2.12 | **Custom order for setup tables** (drag, saved in the file, followed by reports and the Apply panel), click-to-sort headers that cycle back to custom, no dragging while filtered | 12, 16, inv 12 | [missing] | M | none |
| 2.13 | **Apply panel extras**: clear all per list, replace all tags, per-group line count across the budget, fringe table footer (percent, flat, total) | 05, 06, 11, 12, 13 | [partial] | S | none |
| 2.14 | **Line-block presets**: save lines (with a checklist of fields) and insert copies at the cursor, one undo step | 05, inv 13 | [missing] | M | 1.19 |
| 2.15 | **Personal setup library**: an open library file in a folder Sam picks, folders per kind, copy in and out with dependencies and a conflict review, no live link | 07, 09, 11, 24, inv 12 | [missing] | L | 1.20, decision 15 |
| 2.16 | **Charges and credits polish**: percent and excluded amount in the topsheet label, exclusion grid with live group totals, preview that includes the cell being typed, credit rows showing gross, excluded fringes, net and variance to cap, "create charge" from the topsheet menu | 11, 23, inv 9, 10 | [partial] | M | 1.5 |
| 2.17 | **Two tools at once and dock at the bottom**, so the grid gets full width beside a setup list | 01, 04, 06, inv 18 | [missing] | M | none |
| 2.18 | **Display preferences**: rate decimals with a cutoff band, totals decimals, hide zero accounts, show or hide summary rows and the net total, grid zoom | 17, inv 1, 18, 22 | [missing] | M | none |
| 2.19 | **Hidden-factor marker**: flag a line whose hidden X, 4X, unit or currency column changes its total | inv 2, 23 | [missing] | S | none |
| 2.20 | **Renumber accounts when the coding pattern changes** | APP, inv 11 | [missing] | M | none |
| 2.21 | **Share-ready copies**: save a snapshot named by title, version and date, optionally without notes or private info, stamped with where it came from; open read-only; offer to compare a returned copy or store it as a version | 22, inv 16 | [missing] | M | 1.14 |
| 2.22 | **Sub-budgets**: a filtered copy by categories, accounts and tags, optionally without fringes or notes | 04, inv 14 | [missing] | M | none |
| 2.23 | **History list**: every undo step listed, jump back, or branch it into a version | inv 15 | [missing] | M | 1.12 |
| 2.24 | **Group rules as budget settings** (only if decision 3 changes) | 14, 24 | [missing] | M | decision 3 |
| 2.25 | **Windows and Linux live check**: printing, menu keys, installers, file association | APP | [partial] | M | none |

---

## Phase 3: Beyond the reference

Where competitors and the pain points (inv "Gaps and pain points", inv 24) say we can win.

| # | Item | Why | Status | Size | Depends |
|---|---|---|---|---|---|
| 3.1 | **Budget diff**: `fwb diff` and an in-app "what changed" between two files or versions, by row id and account code, in plain words with money deltas | no real collaboration in the reference; our files are text | [missing] | M | 1.14 |
| 3.2 | **Three-way merge**: `fwb merge` plus a git merge driver, merging edits to different lines automatically and listing real conflicts for a person to pick; turns "send a copy" forks into merges | the reference only sends divergent copies | [missing] | L | 3.1 |
| 3.3 | **Review files**: a reviewer marks up a read-only copy, comments land in a small sidecar file Sam can merge back | local stand-in for shared comments | [missing] | M | 2.21 |
| 3.4 | **Version deltas** so many versions don't bloat the file | D17 | [missing] | M | none |
| 3.5 | **"Explain this number"**: trace any total down to lines, fringes, cutoffs, ranges and rounding steps | opaque fringe math and rounding complaints | [missing] | M | none |
| 3.6 | **Incentive modeling**: per-location qualified spend, several programs side by side, caps and broker discounts, what-if by moving lines between locations; programs authored by the user from public rules, never bundled vendor data | locations and credits exist; no one makes this easy | [missing] | L | 2.15 |
| 3.7 | **Actuals and cost reports**: import actuals (CSV), budget vs actual vs estimate to complete by account | top competitor gap | [missing] | L | 2.2 |
| 3.8 | **Schedule-linked quantities**: read a schedule's day counts from an open export and drive globals | competitor feature; the reference only imports static blocks | [missing] | L | 2.2 |
| 3.9 | **Rate tables**: user-kept tables from public union rates; pick a classification to fill the rate (flag the legal line before shipping any data) | competitor add-on | [missing] | L | 2.15 |
| 3.10 | **Overtime helper** for labor lines | competitor feature | [missing] | M | none |
| 3.11 | **Person-level cutoffs across accounts**, opt-in, for one person paid in several accounts | inv 23 quirk | [missing] | M | engine check 1 |
| 3.12 | **Scriptable command line**: batch edits, headless PDF and spreadsheet output for pipelines | open format advantage | [partial] | M | 2.1, 1.15 |
| 3.13 | **Commercial bid layouts** (industry bid form style) | competitor feature; check the legal line on form layouts | [missing] | L | 2.4 |
| 3.14 | **Localization and accessibility pass** | reference is English only | [missing] | M | none |

Skipped on purpose: accounts and sign-in, cloud sync, a send-to-user inbox, live currency
rates (no network), vendor template or rate data, native `.mbd`/`.mmbx` files.

---

## Engine checks to run

Semantic questions only Sam's licensed copy can settle. Most are covered by the export test
(ETP); the quick test is for settling one on its own in a few minutes.

| # | Question | Our rule today | Quick test for Sam | In ETP |
|---|---|---|---|---|
| 1 | Is a percent cutoff per fringe range? | per earner: a line or a range (D3) | Three lines of 1,000 each, a 12.5% fringe with a 2,000 cutoff. Fringe total with them bracketed into a range: ours says 250. Unbracketed: 375 | yes (7201) |
| 2 | Do detail subtotal and total rows drop suppressed lines? | yes, they count as 0 everywhere | Subtotal row under two lines, suppress one line's group, read the subtotal row and the account total row | no |
| 3 | Does toggling a group off and on drift the total? | never; exact to the cent | In a real budget note the net total, suppress a group, restore it, read the net total and "last change" again | no |
| 4 | Currency direction | rate = base units per one foreign unit (D10) | One line of 100 with currency EUR at rate 3. Total 300 means ours; 33.33 means the other way | yes (7301, V3) |
| 5 | What does the percentage cutoff factor do? | not modeled | Export test variation V2 (factor 2.0) and read the driver line's fringe against the table | yes (V2) |
| 6 | Flat fringes: once per line or per range; do they multiply on non-time units? | per line, hours conversion on the primary unit only | Export test grip and driver kit lines | yes |
| 7 | Adjustment rows: can they be fringed, do they include fringes above, do they restart at a subtotal? | no, no, no | Export test account 7102 plus screenshot 19 | yes |
| 8 | Charge base and stacking | ATL + BTL with posted fringes, no stacking, flat fee minus exclusions | Export test ZPCTCHG and ZFLATCHG, and V1 for post-by-budget | yes |
| 9 | Line with one "out" group and one "in" group | out | Export test Option B lens; or tag one line with both and read its total | yes |
| 10 | Do suppressed lines inside a range still count toward the cutoff? | no | Bracket three lines that cross a cutoff, suppress the middle one, read the fringe on the other two | no |
| 11 | Where does rounding happen? | once per line, half away from zero (D1, D2) | Export test Polish line (1,296.315): 1,296.32 or 1,296.31 | yes |
| 12 | Group "total" column after suppression | ours is "as if counted" | Open the groups setup in 2026 and read total vs active total for a suppressed group | no |
| 13 | Does a global's unit do anything? | not modeled | Give a global the unit "week", type it into a line's Amt, see whether the line's unit fills in | no |
| 14 | 2026 new-line and preference defaults | none yet | Screenshot budget preferences in a fresh blank budget (ETP step 1 does this) | yes |
| 15 | Fringe percent shown on a suppressed line | the counted value | Turn on the Agg column, suppress a fringed line, read its Agg | no |
| 16 | Next account at the end of a category | wraps across categories | On the last account of a category, press the next-account step | no |

---

## Recording wishlist

Short clips (1 to 3 minutes, the Z test budget is fine, no real figures needed) that fill
gaps the 2020/2021 tutorials leave or that need 2026 confirmation. In priority order.

1. **Two minutes of Sam's real keystrokes** with an on-screen key display: moving, inserting,
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
