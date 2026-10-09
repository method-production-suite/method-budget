# Movie Magic Budgeting: feature inventory (clean-room research)

Status: research notes, October 2026. Covers Movie Magic Budgeting 10 (MMB10, current release line 10.35.x) and, where useful, the retired Movie Magic Budgeting 7 (MMB7).

Clean-room note: everything below is a functional description written in our own words from public sources (EP's public manual, release notes, public support pages, a public EP migration guide, third-party help pages and reviews). No help text, templates, rate tables, fringe libraries, chart-of-accounts lists, screenshots or bundled data were copied. Nothing here describes the internal layout of the native `.mbd` or `.mmbx` files, and that topic was deliberately not researched. Where a source gave a worked numeric example we restate the rule, not the example.

Priority key used in every section:

- **v1**: needed for a credible first release (or for importing real budgets faithfully).
- **later**: valuable, but can follow v1.
- **skip**: out of scope, vendor-ecosystem specific, or legally off-limits.

---

## Summary

MMB is a three-level, grid-based budgeting desktop app (Electron-style, Mac and Windows) with cloud sync. Its core is:

1. A **hierarchy**: Topsheet (categories) > Account level (accounts within a category) > Detail level (line items within an account). Topsheet also carries structural rows: production totals (level breaks, the first of which splits above-the-line from below-the-line), fringe total rows, contractual charges, grand total, applied credits, and net total.
2. **Detail lines** computed as a product of up to four factors (`Amt x X x Rate x 4X`), each with an optional unit, plus a per-line currency, and per-line tags (fringes, groups, location, set).
3. **Setup tools** shared across the budget: Fringes, Units, Globals, Groups, Locations, Sets, Currencies, Contractual Charges, Credits, Shortcuts, Budget Info. Each tool lives in a per-budget table and can be copied to and from a per-user **Library**.
4. **Fringes**: percent (with a per-person cutoff evaluated over a "fringe range" of lines) or flat (per unit of time, converted via each unit's hours equivalence). Fringe dollars are not shown on the line; they are **posted** to a summary row at the budget, production-level, category or account level.
5. **Globals**: named formula variables usable in any numeric cell, with dependency chains and error states.
6. **Groups**: multi-valued tags with color, priority and an "in budget total" switch that suppresses lines (the main what-if mechanism), and that feed sub-budgets, reports, charges and credits.
7. **Contractual charges** (percent or flat, with excluded amounts, groups, categories and accounts) and **credits** (incentives with qualified-spend caps, credit caps, fringe exclusions and a broker discount) on the Topsheet.
8. **Multi-currency** per line with a base currency and a separate totals display currency.
9. **Scenario tooling**: Original/Variance columns, versions, a 200-step undo history with revert, sub-budgets (filtered copies), and side-by-side comparison of up to 16 budgets by account number.
10. **Reporting**: a WYSIWYG report designer (cover page, topsheet, category summary, account details, chart of accounts, group totals, budget info, notes, setup-table reports), PDF export and printing, report templates exported as portable files.
11. **Exports**: native MMB10 file, legacy MMB7 file, Tab Delimited text, XML (Basic), XML (Advanced), JSON, Excel. No general spreadsheet import (only paste-from-spreadsheet).

The main gaps users complain about: no real-time multi-user editing (copies are sent, not shared), no actuals/cost tracking, no Excel import, subscription and login friction, MMB7 round-trip data loss, rounding surprises, opaque legacy fringe behavior, and performance on large budgets.

---

## 1. Budget hierarchy and structure

Features:

- Three editable levels. Topsheet rows are **categories** (sometimes called header accounts); each category contains **accounts**; each account contains **detail lines**. The sum of an account's detail lines is the account total; category totals roll up their accounts.
- A budget always opens on the Topsheet. Drilling down is by double-clicking a row's line-number cell or a keyboard chord; going up is by a breadcrumb or a keyboard chord. The breadcrumb segments are dropdowns that jump to sibling categories or accounts.
- Next/previous account navigation wraps across category boundaries.
- Row-level structure on the Topsheet:
  - **Categories** (numbered and described).
  - **Production Totals** ("level breaks"): user-inserted, renameable subtotal rows that sum all categories above them up to the previous production total. Any number allowed. The **first** production total defines the above-the-line (ATL) / below-the-line (BTL) boundary.
  - Optional **summary totals** rows (Total ATL, Total BTL, Total ATL+BTL), controlled by a per-budget preference.
  - **Total Fringes** row(s), depending on the fringe posting mode (see section 4).
  - **Contractual charges**, listed below the last production total, in the order they were applied.
  - **Grand Total** = all production totals + charges + posted fringes.
  - **Applied credits** between grand total and net total, with a credits subtotal row when two or more are applied.
  - **Net Total** = grand total minus credits. Can be switched off per budget.
- **Account-level subtotal rows** can be inserted between detail lines (running total of lines since the previous subtotal row or the top of the account). They can be hidden from reports while still calculating.
- **Subtotal adjustment rows** at the detail level (see section 2).
- A footer strip always shows: grand total, ATL+BTL, total ATL, total BTL, credits, a **session change** total (net change since the budget was opened or the counter was reset) and a **last change** amount (net effect of the most recent edit), with a reset button.
- Deleting, cutting or copying a category or account carries all children; renaming or renumbering does not move children.
- Optional "hide zero-balance accounts" on the Topsheet (visibility only, totals unchanged).
- Per-line text formatting (bold, italic, underline, font color) at all three levels; a line's font color overrides its group color; formatting prints.

Priority for us: **v1** for the three levels, production totals with ATL/BTL split, grand/net totals, account subtotal rows, subtotal adjustments, footer totals. **later** for session/change counters, text formatting, hide-zero.

Sources: https://mmb-docs.ep.com/Topsheet/Topsheet_Overview.html, https://mmb-docs.ep.com/Topsheet/Topsheet_Structure.html, https://mmb-docs.ep.com/Topsheet/Topsheet_ProductionTotals.html, https://mmb-docs.ep.com/Accounts/Accounts_Overview.html, https://mmb-docs.ep.com/Details/Details.html, https://mmb-docs.ep.com/Budget_Navigation/navigation.html, https://mmb-docs.ep.com/Budget_Preferences/Budget_Preferences_Overview.html

---

## 2. Detail lines: columns and calculation

Columns available at the detail level (default visible unless noted):

| Column | Meaning (our words) |
|---|---|
| Line number | Row index within the account (not the account number). |
| Description | Free text; leading and repeated spaces preserved. Note icon appears here when notes exist. |
| Agg (optional) | Effective fringe burden of the line: fringe dollars divided by line total, i.e. after cutoffs. |
| Agg Sum (optional) | Plain sum of the percent fringe rates assigned to the line, before cutoffs. Read-only. |
| Fringes (optional) | Comma-separated list of applied fringes, ordered by each fringe's display order. |
| Groups (optional) | Comma-separated list of applied groups, ordered by display order. |
| Location (optional) | Single location code. |
| Set (optional) | Single set code. |
| Amt | First factor. Accepts a number, a fraction, or a formula with globals. |
| Unit | Unit for Amt. This is the **primary unit**, the only one used for flat-fringe hour conversion. |
| X | Second multiplier. |
| Unit2 (optional) | Unit label for X. |
| Curr | Currency of the Rate; blank means base currency. |
| Rate | The rate; may be a formula with globals. |
| Unit3 (optional) | Unit label for Rate. |
| 4X (optional) | Third multiplier. |
| Unit4 (optional) | Unit label for 4X. |
| Subtotal | Read-only line total. |

Calculation behavior:

- Line subtotal = Amt x X x Rate x 4X, converted from the line currency into the totals display currency. Blank multipliers behave as 1.
- Every numeric input cell can hold a formula using `+ - * /`, parentheses, literal numbers and global names. Invalid formulas, division by zero, circular global references and references to deleted globals produce a visible `#ERR` state (red highlight).
- Unit labels pluralize automatically when the associated value is not exactly 1.
- Fractions typed into Amt or multiplier columns can be kept as typed (default) or displayed as decimals; the stored value is the same.
- Negative values display in parentheses.
- If a hidden column (Unit2, Unit3, 4X, Unit4, or a non-base currency) holds a value that changes the subtotal (anything other than blank or 1), the row gets a red marker so users are not fooled by hidden factors. Screen-only, never printed or exported.
- **Subtotal row**: running total of lines above it back to the previous subtotal row or account top.
- **Subtotal adjustment row**: Rate is locked to the sum of all preceding lines in the account (back to the top, or to the previous subtotal adjustment row; ordinary subtotal rows are ignored). Amt defaults to 1 and the user edits Amt or the multipliers to express, for example, a contingency or discount percentage of the lines above. The result is a real cost (or credit if negative). Setting Amt to 0 on an adjustment right after a subtotal is the documented trick to restart the accumulation window.
- New-line preference: when inserting a detail line, optionally copy fringes, groups, locations and sets from the line above.

Priority for us: **v1** for all columns, the four-factor product, formulas with globals, `#ERR` states, subtotal rows, subtotal adjustment rows. **later** for the hidden-column marker, fraction display toggle.

Sources: https://mmb-docs.ep.com/Budget_Navigation/columns.html, https://mmb-docs.ep.com/Budget_Navigation/calculations.html, https://mmb-docs.ep.com/Details/Details.html, https://mmb-docs.ep.com/System_Preferences/Sys_Prefs_Overview.html, https://mmb-docs.ep.com/release_notes.html

---

## 3. Units

Features:

- Unit table: singular name (required), plural name, description, and an **hours equivalence** used for flat-rate fringe conversion (blank for non-time units).
- `%` is a protected built-in unit; it marks percentage fringes and cannot be edited or duplicated.
- Blank budgets ship with a small starter set of time units plus "allow" and "flat" style units (we must ship our own defaults, not theirs).
- Type-ahead in any unit cell; typing an unknown unit offers to create it.
- Bulk apply a unit to many selected lines from the tools pane; clicking a unit already on every selected line removes it.
- Deleting an in-use unit warns first.

Priority for us: **v1**.

Sources: https://mmb-docs.ep.com/Setup/units.html, https://mmb-docs.ep.com/ApplyTools/Applying_Units.html

---

## 4. Fringes

Setup fields: name (unique), description, short ID, rate (number or formula with globals), unit (`%` for percentage, any time unit for flat), cutoff, display order (1 to 20), category (folder), plus derived columns for in-use count and total dollars.

Calculation semantics as documented:

- **Percentage fringe**: rate x the base, where the base is the subtotal of the line (or of the whole **fringe range**, see below) before any fringes. If a cutoff is set, the base is the smaller of that subtotal and the cutoff. Cutoffs apply only to percentage fringes.
- **Cutoff scope is per line or per fringe range, not per budget and not per named person.** A fringe range is a user-made bracket over two or more contiguous detail lines (typically prep, shoot and wrap for one crew member) that makes the fringe engine treat them as one earner. Each fringe is evaluated independently for that range. Lines outside a range are each their own earner, so two separate lines for the same person will each get a full cutoff unless bracketed. There is no cross-account "person" identity.
- **Flat fringe**: a fixed amount charged once per line it is applied to. Exception: when the fringe's unit and the line's **primary** unit both have hours equivalences, the fringe is converted proportionally (fringe rate per its unit, scaled by the ratio of hours, times the line quantity), so a per-day fringe on a weekly line becomes a per-week amount. Only the primary unit (after Amt) is considered; Unit2 to Unit4 are ignored. The documented example also aggregates quantities across a fringe range for flat fringes, which conflicts with the "once per line regardless of ranges" statement; see open questions.
- **Posting**: fringe dollars are never shown on the source line. They are summed into a fringe row according to a per-budget posting mode:
  - by **Budget**: one Total Fringes row on the Topsheet, by default just above the grand total; can be moved anywhere on the Topsheet; optional custom account number and description for international layouts.
  - by **Production Level**: a fringe row directly above each production total (editable account number).
  - by **Category**: a fringe account appended to each category, numbered by combining the category number with a configurable suffix (padded or truncated to the account-coding width; alphanumeric category codes are kept).
  - by **Account**: a fringe line at the end of each account.
- **Percentage Fringe Cutoff Factor**: a per-budget multiplier (1.0 to 3.0 in half steps, default 1.0) that scales how percentage cutoffs are applied. It exists because MMB7 budgets carried a hidden value that silently changed fringe math. Exact formula is not documented; see open questions.
- Topsheet and account levels can show a Fringe column (fringe total per row).
- Fringe tools pane shows the selected line's aggregate percent and fringe dollar total; shows partial (dash) state when the selection mixes.
- Sub-budget **fringe lock**: a sub-budget can freeze the parent's fringe dollars and aggregate percentages even though lines were filtered out; any edit that would recalculate fringes asks to release the lock.
- Deleting an in-use fringe warns; duplicate creates a "(Copy)".
- MMB7 had an explicit preference for how the aggregate fringe percentage is computed (inferred from its manual's contents page).

Priority for us: **v1** for percent and flat fringes, cutoffs, fringe ranges, unit hours conversion, all four posting modes, Agg column. **later** for the cutoff factor (needed only for faithful import of legacy budgets, so research it before v1 import), fringe lock in sub-budgets, display order.

Sources: https://mmb-docs.ep.com/Setup/fringes.html, https://mmb-docs.ep.com/ApplyTools/Applying_Fringes.html, https://mmb-docs.ep.com/Budget_Preferences/Budget_Prefs_Fringe.html, https://mmb-docs.ep.com/Budget_Navigation/columns.html, https://mmb-docs.ep.com/Sub_Budgets/Sub_Budget_Overview.html, https://mmb-docs.ep.com/release_notes.html, https://www.scribd.com/document/47125706/MMB-User-Manual (MMB7 contents page only)

---

## 5. Globals

Features:

- Table fields: unique name, description, calculation (number or formula referencing other globals), optional unit, decimal places for display (`Dec`), a "display by name" flag, category (folder), in-use count.
- Names may begin with a digit in MMB10 (MMB7 did not allow this, so exports to MMB7 can break).
- Usable in any numeric cell (Amt, X, Rate, 4X) and in fringe rates and other globals. Type-ahead appears after two characters.
- Display: while editing, the cell shows the formula; after commit it shows the evaluated value, **unless** any global in the formula has "display by name" set, in which case the whole formula stays visible as text.
- Any change propagates immediately everywhere.
- Deleting a global in use replaces every reference with its current numeric value (it does not break the cells).
- Circular references and references to missing globals produce `#ERR`; a find mode lists all `#ERR` globals.
- Find and replace by global covers Amt, X, Rate and 4X; replacement can be another global, a number, or a formula, and only the targeted global inside a multi-global formula is replaced.
- Globals can be inserted into report headers, footers and cover page as live tags.
- Optional preference: the first global used in a Rate formula dictates that cell's displayed decimals.
- MMB7 organized globals into named "global groups"; MMB10 uses setup categories for the same purpose.

Priority for us: **v1** for named variables, formulas, propagation, cycle detection, delete-to-value. **later** for display-by-name, Dec, find/replace by global, header tags.

Sources: https://mmb-docs.ep.com/Setup/globals.html, https://mmb-docs.ep.com/ApplyTools/Applying_Globals.html, https://mmb-docs.ep.com/Find/Find_Globals.html, https://mmb-docs.ep.com/Budget_Preferences/Budget_Prefs_Rate.html, https://mmb-docs.ep.com/Projects_and_Budgets/importexport.html, https://www.youtube.com/watch?v=xiGMytdaZLs (title and description only)

---

## 6. Groups

Features:

- Table fields: unique name, description, short ID, **InBT** ("in budget total") switch, color, color priority (1 to 20), display order (1 to 20), category.
- A detail line can carry many groups.
- **Suppression**: if a group's InBT is off, any line carrying it is excluded from all totals and shown greyed (optional grey bar). This is the main what-if mechanism (for example "Option B cast", "Second unit").
- **Color**: line text takes the color of its highest-priority group; if none of its groups has a priority, the most recently applied group wins. Assigning a priority that is already taken swaps the two.
- Mass apply from Topsheet or category rows cascades the group to every existing detail line underneath (does not tag lines added later). Partial coverage shows a tri-state indicator.
- Group totals visible in the tools pane ("active totals"), and a Group Totals report.
- Groups drive: sub-budget filtering, contractual charge exclusions, credit inclusions, report exclusion of suppressed lines, find and replace (multi-group AND search with an exact-match option).
- MMB7 documented rules for a line whose groups disagree about suppression and for whether fringe dollars count in group totals (contents page only); MMB10 behavior for conflicts is not documented.

Priority for us: **v1** for multi-tag groups, suppression, group totals. **later** for color priority, display order, mass apply.

Sources: https://mmb-docs.ep.com/Setup/groups.html, https://mmb-docs.ep.com/ApplyTools/Applying_Groups.html, https://mmb-docs.ep.com/Find/Find_Fringes.html, https://mmb-docs.ep.com/System_Preferences/Sys_Prefs_Overview.html, https://mmb-docs.ep.com/release_notes.html

---

## 7. Sets and Locations

Features:

- **Set**: one code per detail line (name, description). Used for set-based roll-ups, reports, sub-budgets, and passed to accounting (sets travel with GL codes into the vendor's accounting product). The account-coding preference also has a width for the set segment.
- **Location**: one per detail line (name, city, state or region, country). Used for jurisdiction roll-ups, sub-budgets and incentive analysis.
- Both can be created on the fly by typing an unknown value into the cell (with a confirm), applied in bulk from the tools pane, and show a partial state at account and category level.
- Pasting an unknown set or location from outside creates it automatically.

Priority for us: **v1** (both are single-valued tags; cheap to implement and present in exports).

Sources: https://mmb-docs.ep.com/Setup/set.html, https://mmb-docs.ep.com/ApplyTools/Applying_Sets.html, https://mmb-docs.ep.com/Setup/location.html, https://mmb-docs.ep.com/ApplyTools/Applying_Locations.html, https://www.ep.com/blog/eight-things-you-might-not-know-about-movie-magic-budgeting/

---

## 8. Currencies

Features:

- Currency table: code (with type-ahead from a list of world currencies), description, symbol, symbol-after-value flag, separator pattern (comma thousands with period decimal, or period thousands with comma decimal), and an exchange **rate** stored to five decimal places.
- Two budget-level selections: **Base currency** (assumed for any line with a blank Curr) and **Totals and subtotals currency** (the display currency for all computed totals, which also determines symbol and separators).
- The Rate cell always shows the value as typed in its own currency; only the subtotal is converted.
- Changing the base currency silently reinterprets every blank-currency line in the new base, so totals change.
- Deleting a currency makes its lines fall back to base currency, so totals change.
- Bulk apply currency from the tools pane; partial state above detail level.
- Flat contractual charges carry their own currency.

Conversion direction: the manual's examples are not mutually consistent. One example treats a currency's rate as "units of the reference currency per one unit of this currency" (a line in the foreign currency is multiplied by its rate to reach the display currency). Another example converts a base-currency amount into a foreign display currency by multiplying by the display currency's rate as well. We must not copy either reading blindly; see open questions. Our engine should store an explicit, unambiguous direction and convert exported data by checking totals.

Priority for us: **v1** for per-line currency, base currency, separate display currency, separators. Conversion direction must be pinned down against real exports before claiming import fidelity.

Sources: https://mmb-docs.ep.com/Setup/currencies.html, https://mmb-docs.ep.com/ApplyTools/Applying_Currency.html, https://downloads.adc.ep.com/doc/Get_Most_Out_Of_MMB10.pdf

---

## 9. Contractual charges

Features:

- A charge is a Topsheet-level cost of type **Percent** or **Flat Fee**, with a name, optional account number, and display options to append the percentage and/or the excluded amount to its Topsheet description.
- Charges are created in setup and must then be **applied** to affect the budget; unapplied charges are kept for reuse. Applied ones are marked in the list.
- **Percent charge** = percent x (ATL + BTL total minus any manually entered excluded amounts minus the totals of excluded groups minus any excluded categories or accounts).
- **Flat fee** = the fee amount (in its chosen currency) minus excluded amounts and excluded group totals, per the manual. The "percentage" display on a flat fee is informational only (fee as a share of ATL+BTL).
- Exclusions: manual "excluded amount" rows with description and value; excluded groups; and (via the tools pane) exclusion of specific highlighted categories or accounts at Topsheet or category level.
- Position: applied charges appear between the last production total and the summary totals, in application order. To reorder, un-apply and re-apply. Older imported budgets may have charges elsewhere.
- Charges add to the grand total, not to ATL/BTL.
- MMB7 also allowed excluding individual line items and "budget-linked" items (contents page only).

Priority for us: **v1** for percent and flat charges with group and amount exclusions and category/account exclusions; charge ordering. Reviewing whether charges stack (a charge on another charge) is an open question.

Sources: https://mmb-docs.ep.com/Setup/charges.html, https://mmb-docs.ep.com/ApplyTools/Apply_Charges.html, https://mmb-docs.ep.com/Topsheet/Topsheet_Structure.html

---

## 10. Applied credits (incentives)

Features:

- A credit models a tax incentive, rebate or refund deducted after the grand total.
- Credit header: name, overall **applied credit cap**, **discount percentage** (for example a broker fee on selling the credit), account number.
- Credit table rows of two types:
  - **Flat amount**: entered gross total; net equals gross.
  - **Included group**: gross is the total of lines tagged with that group; net is gross minus the dollars of any **excluded fringes** on those lines.
- Each row has a percentage and a cap type:
  - **None**: credit = net x percent.
  - **Qualified spend cap**: credit = min(net, cap) x percent.
  - **Credit cap**: credit = min(net x percent, cap).
- Final credit = min(sum of row credits, overall applied cap), then reduced by the discount percentage.
- Credits can also be associated with Topsheet category or account rows directly from the tools pane (category-type or account-type associations).
- Summary columns: gross total, excluded fringe total, net total, credit amount.
- Order on the Topsheet is reverse of application order (opposite of charges). A credits subtotal row appears with two or more.
- A print view of applied credits exists in the setup screen.

Priority for us: **later** (high value for indie users, but not needed to compute a cost budget). Keep the data model ready in v1 so imports do not lose credits.

Sources: https://mmb-docs.ep.com/Setup/credits.html, https://mmb-docs.ep.com/ApplyTools/Apply_Credits.html, https://mmb-docs.ep.com/Topsheet/Topsheet_Structure.html

---

## 11. Account coding (chart-of-accounts format)

Features:

- Per-budget pattern: widths (1 to 6 characters) for category, account, sub-account and set segments, and one separator (none, space, hyphen, period, comma, slash, colon, semicolon) used for every segment.
- Typed account numbers are parsed against the pattern: typed separators are normalized to the chosen one; short entries leave trailing segments empty; long entries are truncated.
- Alphanumeric codes are allowed (for example a letter inside the category segment), including for generated fringe accounts.
- Templates carry the chart of accounts; comparison matches budgets on account numbers.

Priority for us: **v1** (account numbers are the join key for comparison, import and accounting exports).

Sources: https://mmb-docs.ep.com/Budget_Preferences/Budget_Prefs_Account_Coding.html, https://mmb-docs.ep.com/Budget_Preferences/Budget_Prefs_Fringe.html

---

## 12. Setup tables, library and templates

Features:

- Every tool (fringes, units, globals, groups, currencies, locations, sets, charges, credits, shortcuts, budget info) has a setup table opened in a separate window, scoped to the active budget.
- Tools can be organized into named **categories** (folders) per tool; tables sort ascending, descending, or by a custom drag order (setup order is independent of the tools-pane order).
- **Library**: a per-user store of tools. Copy to library and copy to budget both offer to bring **dependencies** (for example a fringe's unit, or the globals a global references). Copies overwrite same-named items at the destination, including moving them into the source's category. Library and budget copies never sync automatically.
- **Templates**: a template is a budget with its chart of accounts and tools but no detail lines. Users can create a template from any budget, organize templates in folders, import and export template files, and start a new budget from a template. EP ships its own templates (we must author ours from public COA conventions).
- Budget Info: built-in metadata fields (title, production, season, episode and episode count, status, base country/region/city, shoot day counts, production dates, year) plus user-defined fields typed as text, number or date. Usable in report headers and included in some exports.

Priority for us: **v1** for templates (create from budget, new from template) and budget info. **later** for a cross-budget library with dependency copying (high user value, moderate effort).

Sources: https://mmb-docs.ep.com/Setup/setup_tables.html, https://mmb-docs.ep.com/Setup/library_and_budget.html, https://mmb-docs.ep.com/Projects_and_Budgets/templates.html, https://mmb-docs.ep.com/Setup/budget_info.html

---

## 13. Shortcuts (reusable line blocks and tool sets)

Features:

- **Tools shortcut**: a saved bundle of applied tools (fringes, groups, set, location, currency) captured from one line, applied to selected lines in **add** mode (merge) or **replace** mode (clear then apply).
- **Line shortcut**: a saved block of one or more detail lines with chosen columns, inserted above the selection.
- Up to ten numeric hotkeys can be bound to shortcuts.
- Shortcuts are editable in place (cells, tool assignments, inserted rows, subtotal and adjustment rows).
- Shortcuts are budget-specific.
- Scheduling import: element libraries exported from EP's scheduling product can be loaded as line shortcuts, one shortcut per scheduling category, with each element's day count in Amt. (One-line mention only, per scope.)

Priority for us: **later** for shortcuts. Schedule import is **later** and should target open formats only.

Sources: https://mmb-docs.ep.com/ApplyTools/shortcuts.html, https://mmb-docs.ep.com/Setup/shortcuts_setup.html, https://mmb-docs.ep.com/Setup/mms6_import.html

---

## 14. Scenario and comparison tools

Features:

- **Original and Variance** columns (Topsheet and account level): Original is a frozen snapshot of each row's total, captured for the whole budget with one refresh button (zero for a new budget). Variance = current total minus original (positive means the row grew). A preference locks the original so it cannot be refreshed by accident. These are not separate budgets; they are a per-row baseline stored inside the budget.
- **Versions**: one click duplicates the current budget into a new budget with a numeric suffix in the same folder, or from any point in the recent history list.
- **Sub-budgets**: a filtered copy built in three steps: (1) choose categories and accounts, optionally excluding empty or zero accounts; (2) filter lines by tool values (groups, locations, sets, currencies and so on), where options within one tool are OR'd, different tools are AND'ed, "include lines without any value" is a per-tool switch, and a line matching both a kept and a removed value is kept; (3) optionally strip all fringes (changes totals) and/or all notes, and optionally lock the parent's fringe dollars. The result is an independent budget with no live link to the parent.
- **Comparison**: pick a base budget and link up to 15 others (cloud-stored only). The Topsheet and account levels switch to a comparison grid matched **by account number**, with a total per linked budget, sum, average, base total, variance (sum minus base) and variance percent. Base column can sit left or right. Detail level stays editable on the base budget only, and the comparison updates live. Cut, copy, paste, insert and delete are disabled at the upper levels while comparing. Comparison report available. For detail-line comparison users must use split-screen instead.

Priority for us: **v1** for Original/Variance and versions (cheap, heavily used). **later** for sub-budgets and multi-budget comparison.

Sources: https://mmb-docs.ep.com/Budget_Navigation/Orig_Var.html, https://mmb-docs.ep.com/History_Versions/versions.html, https://mmb-docs.ep.com/Sub_Budgets/Sub_Budget_Overview.html, https://mmb-docs.ep.com/Comparison/Comparison_Overview.html

---

## 15. History, undo, saving and file management

Features:

- Undo/redo up to 200 steps per session; non-destructive view changes are not recorded. Setup-table edits are undoable.
- History panel lists each recorded action; any step can be reverted to, or branched into a new version.
- Autosave on by default; can be turned off for manual save and save-as. Unsaved-changes prompt on close.
- Budgets are stored in an encrypted local store and synced to the vendor cloud; offline use is allowed for up to seven days before a license check. Only one active session per account.
- Local files: budgets can also be saved as standalone files on disk, exported in bulk as a zip, and imported in bulk.
- Manage screen: projects/folders nested five deep, favorites, archive (read-only, export only), trash with restore, search, move-to with folder creation, created and last-opened dates, accounting link column.
- Unique budget names per folder; duplicates get a numeric suffix.

Priority for us: **v1** for undo/redo, autosave to local file, versions. **later** for history panel with revert. **skip** vendor cloud, licensing, folders (we use the file system and git-friendly files).

Sources: https://mmb-docs.ep.com/History_Versions/undo-redo.html, https://mmb-docs.ep.com/History_Versions/history.html, https://mmb-docs.ep.com/Projects_and_Budgets/Saving_Offline.html, https://mmb-docs.ep.com/Projects_and_Budgets/openbudgets.html, https://mmb-docs.ep.com/Projects_and_Budgets/budgets.html, https://mmb-docs.ep.com/Projects_and_Budgets/projects.html

---

## 16. Collaboration, notes and sharing

Features:

- **Notes** on any row at any level, multiple per row, with author identity, timestamp, replies, edit and delete. A Notes report prints all notes. Notes do not survive import or export to other formats.
- **Send a copy**: point-to-point transfer of a snapshot to another licensed user by email address; updates are sent as new copies, never merged. Recipients see them in a "sent to me" folder.
- **Accounting link**: a budget can be linked to the vendor's accounting product by license number; the accounting side reads the latest saved version.
- Split-screen: two budgets side by side, tabs movable between panes, independent keyboard focus; the main way to copy lines between budgets.

Priority for us: **later** for notes (cheap, valuable). **skip** send-a-copy and vendor accounting link; our equivalent is a shareable open file plus open exports.

Sources: https://mmb-docs.ep.com/Notes/Notes_Overview.html, https://mmb-docs.ep.com/Send_a_Copy/Send_a_Copy.html, https://mmb-docs.ep.com/Projects_and_Budgets/accounting_integrations.html, https://mmb-docs.ep.com/Budget_Navigation/SplitScreen_mode.html

---

## 17. Find and replace

Features:

- Modes: go-to account by number (exact match jumps immediately), description, fringes, globals, groups, location, set, Amt, X, 4X, Rate, Unit (all four unit columns, including hidden ones), currency.
- Results list grouped by account, relevance ordered, first 200 then load-all; next/previous match navigation with a counter.
- Actions per mode: replace, add (fringes and groups, skipping lines that already have it), remove (trash). Numeric replace overwrites the whole cell including any globals; numeric search matches the final value exactly. Description replace overwrites the whole field.
- Multi-term AND search for fringes and groups, with an exact-match toggle.
- Subtotal adjustment rows are included in results (rate replacement disabled for them).

Priority for us: **later** (account jump is **v1**).

Sources: https://mmb-docs.ep.com/Find/Find_Overview.html, https://mmb-docs.ep.com/Find/Find_Go.html, https://mmb-docs.ep.com/Find/Find_Desc.html, https://mmb-docs.ep.com/Find/Find_Fringes.html, https://mmb-docs.ep.com/Find/Find_Globals.html, https://mmb-docs.ep.com/Find/Find_Amount_x_4x_rate.html, https://mmb-docs.ep.com/Find/Find_Units.html

---

## 18. Grid editing and keyboard behavior

Features:

- Spreadsheet-style grid; at least one cell is always active. Typing overwrites; F2 or double-click edits in place; a chord exits edit mode without moving.
- Enter moves down, Tab moves right, shifted versions reverse. Enter or Tab past the last row creates a new row.
- Delete/Backspace clears a cell; deletes the row only when the full row is selected. Deleting a category or account confirms because children go too.
- Multi-select: drag, shift-arrows, ctrl/cmd-click for non-contiguous cells or rows, select all.
- Bulk entry: select many cells in one column, type once, every cell gets the value (one undo step).
- Fill handle: drag a small square to copy selected cells of one row up or down.
- Copy/paste of cells (overwrite in place, repeated to fill a larger target) or rows (insert above target; categories and accounts carry their children). Paste between budgets brings dependent tools along, but **same-named tools in the target win**, which can silently change the pasted line's math. Paste to and from spreadsheets moves visible values only; pasted columns must match visible columns.
- Read-only cells are shaded; the active cell and selection have distinct colors.
- Column chooser per level; column widths and visibility persist per budget; auto-fit by double-clicking a border.
- Grid zoom 75 to 125 percent, saved per budget.
- Keyboard map covers: open setup tables by number keys, budget preferences, charges, credits, report setup, find, go to account, level up/down/next/previous, go to topsheet/accounts/details, first/last row, insert row, delete row, insert subtotal, insert subtotal adjustment, make and remove fringe range, apply fringes and apply groups panes, toggle tools pane, switch tabs, add note, shortcut hotkeys, text formatting, currency symbol entry.
- Tools pane: dockable right or bottom, or popped out to its own window; tabs reorderable; per-tool search, "filter applied" and "filter in use"; tri-state checkboxes for mixed selections.

Priority for us: **v1** for core navigation, Enter/Tab semantics, insert/delete row, copy/paste rows and cells, multi-select, level navigation shortcuts, column chooser. **later** for fill handle, bulk entry, zoom, pop-out pane.

Sources: https://mmb-docs.ep.com/Budget_Navigation/Keyboard_Shortcuts.html, https://mmb-docs.ep.com/Budget_Navigation/select.html, https://mmb-docs.ep.com/Budget_Navigation/CopyPaste.html, https://mmb-docs.ep.com/Budget_Navigation/navigation.html, https://mmb-docs.ep.com/ApplyTools/Apply_Tools_Overview.html

---

## 19. Screens and workflow states

Screens and what each shows:

- **Open and manage budgets**: tabs for cloud, on this device, templates; folder tree; budget list with dates; actions for new blank, new from template, import, export, duplicate, rename, move, archive, delete, create template, create sub-budget, open.
- **Budget window**: top bar (home, send, reports, setup, compare, find, save, autosave toggle, tools toggle, online status); tab strip of open budgets; breadcrumb; main grid for the current level; tools pane; footer totals; history and version buttons above the grid.
  - Topsheet state: line number, account, category description, optional fringe, original, total, variance; plus structural rows.
  - Account state: same columns for accounts in one category, plus generated fringe accounts when posting by category.
  - Detail state: the detail columns listed in section 2, with fringe range brackets drawn at the left edge.
  - Comparison state: replaces Topsheet and account grids with the comparison columns.
- **Setup window**: separate window, one tab per tool plus budget info; budget/library toggle; category rail; table.
- **Budget preferences**: fringe posting, detail-line copy-down, rate display rules, summary totals, lock originals, net totals, account coding, hide zero accounts; a system tab for grid shading, horizontal lines, suppression bar, fraction display, total decimals, default zoom.
- **Report setup**: preview pane, template list, formatting controls, included sections dialog, cover/header/footer editor.

Typical workflow (as taught): set account coding and preferences, build or pick a template, set up units, globals, fringes, groups, locations, sets and currencies, enter detail lines and apply tools, bracket fringe ranges, add production totals, charges and credits, capture originals, then iterate with versions, sub-budgets and comparison, and finally print or export.

Priority for us: informational; drives UI planning.

Sources: https://mmb-docs.ep.com/Budget_Navigation/navigation.html, https://mmb-docs.ep.com/Projects_and_Budgets/openbudgets.html, https://mmb-docs.ep.com/Budget_Preferences/Budget_Preferences_Overview.html, https://mmb-docs.ep.com/System_Preferences/Sys_Prefs_Overview.html, https://downloads.adc.ep.com/doc/Get_Most_Out_Of_MMB10.pdf, https://www.scribd.com/document/47125706/MMB-User-Manual (contents page: setup order)

---

## 20. Reports, printing and PDF

Features:

- WYSIWYG report setup: the preview is the output. Column chooser and resizable columns per report section.
- Formatting: vertical lines, font size (5, 6, 8 default, 10 pt), portrait or landscape, alternating row shading, group colors or black and white, decimal display override, collapse details to one line per account, page breaks between sections on or off, section spacing.
- Report templates: several per budget, exportable and importable as portable files.
- Sections (reorderable by drag): cover page, topsheet, category summary, account details, chart of accounts, group totals, budget information, notes, and setup-table reports (fringes, globals, groups, units, locations, sets, currencies, credits), each filterable by tool category or credit name.
- Account selection: tree of categories and accounts with tri-state checkboxes; exclude empty accounts, zero-total accounts, suppressed lines; optionally hide the fringe breakdown on account details; hide category totals or account subtotal rows.
- Topsheet exclusions: zero-total accounts, category summary rows, collapse credits into one row, hide total charges, hide total fringes.
- Cover page (four text zones) and first-page/other-page header and footer (three zones each), with insertable live tags for budget info fields, globals, and reference values (page number, dates, time, file name, created and modified dates, base and display currency, counts of categories, accounts, lines and in-use tools).
- Output: PDF export or direct print. Comparison report is separate. Older reports suppressed "0" and "1" values on empty lines for cleaner output (MMB7 had a print option for fields equal to 1 or 0).

Priority for us: **v1** for topsheet, account detail and category summary PDF with basic header/footer. **later** for the designer, templates, setup reports, tags, cover page.

Sources: https://mmb-docs.ep.com/Reports_Printing/Reports_Overview.html, https://mmb-docs.ep.com/Reports_Printing/Reports_Included_Sections.html, https://mmb-docs.ep.com/Reports_Printing/Header_Footer.html, https://mmb-docs.ep.com/release_notes.html

---

## 21. Import and export formats (critical for our import path)

Export menu items in MMB10 (File > Export):

| Export | File | What is publicly known about its contents |
|---|---|---|
| MMB10 local budget | `.mmbx` | Full-fidelity native file. **Proprietary; out of scope (do not reverse engineer).** |
| MMB7 budget | `.mbd` | Legacy native file for MMB7 users; loses MMB10-only data (notes, budget info, some tags; globals starting with digits are a problem). **Out of scope.** |
| Tab Delimited | `.txt` | Plain tab-separated text, available since 10.1. Used by accounting systems (the vendor's own accounting product and a third-party payroll/accounting product both document importing it). One third-party guide warns that not every line may survive the conversion, so it is likely an account-level or detail-level flat list. Exact columns not publicly documented. |
| XML (Basic) | `.xml` | Machine-readable, deliberately kept stable for downstream systems; contains "some" detail-level data. Formerly called XML (Detailed); a 2021 update added more fringe detail. Imported by at least two third-party products (a payroll platform and a production-management platform), which suggests it carries account numbers, descriptions and totals. Exact schema not public. |
| XML (Advanced) | `.xml` | All detail-level data including attached groups, fringes, sets, locations, which globals each line uses, and budget info fields. |
| JSON | `.json` | Same data as XML (Advanced), in JSON. |
| Excel | `.xlsx` | Whole budget across all three levels plus budget info, with fringes, groups, globals, sets and locations listed per detail line. Number display follows the user's decimal-totals setting and the fraction display setting. |
| PDF | `.pdf` | Report output from report setup (and a credits print view). |
| Report template | portable report template file | Report configuration only (sections, exclusions, formatting, header/footer assignment). |
| Bulk cloud export | `.zip` of native files | Proprietary contents; out of scope. |

Import paths: native MMB10 and MMB7 budget files, MMB7 template files, scheduling element libraries (as shortcuts), report template files. There is **no general spreadsheet or CSV import**; users can only paste visible values from a spreadsheet into matching visible columns. A third-party tool (a script breakdown product) exports directly in a format MMB10 can open, and another competitor imports MMB "CSV" exports.

Implications for us:

- Our importers should target, in order: **XML (Advanced)** and **JSON** (richest, structured), **Excel** (most commonly emailed), **Tab Delimited** (accounting-oriented), **XML (Basic)**. PDF parsing is a last resort.
- We need real sample files of each export produced by a licensed user from a test budget we design (covering every feature in this document) before writing parsers. Exports are open formats and are explicitly allowed by CLEAN-ROOM.md.
- Check whether exports carry formulas or only evaluated values, per-line fringe dollars or only names, fringe range membership, cutoffs, currency rates, group InBT flags, subtotal and adjustment rows, production totals, charges and credits.

Priority for us: **v1** for XML (Advanced)/JSON and Excel import, and for our own open exports (CSV/XLSX/JSON). **later** for Tab Delimited and XML (Basic). **skip** `.mmbx`, `.mbd`, `.mbt`, `.mbl` and zip bundles.

Sources: https://mmb-docs.ep.com/Projects_and_Budgets/importexport.html, https://mmb-docs.ep.com/release_notes.html, https://downloads.adc.ep.com/doc/Get_Most_Out_Of_MMB10.pdf, https://www.ep.com/support/movie-magic-budgeting/, https://www.ep.com/blog/eight-things-you-might-not-know-about-movie-magic-budgeting/, https://support.castandcrew.com/hc/en-us/articles/30846871607572-PSL-Importing-a-Budget-from-Movie-Magic, https://help.wrapbook.com/docs/import-a-project-budget-from-movie-magic-pas, https://dramatify.com/how-to/import-a-movie-magic-budget, https://www.youtube.com/watch?v=gxSzjOZxOQg (description only), https://filmustage.com/blog/filmustage-update-movie-magic-budgeting-export-multi-episodic-projects-next-gen-storyboards/ (search excerpt), https://www.reddit.com/r/FilmTVBudgeting/comments/1d0a67z/hello_my_friend_is_new_to_movie_magic_budgeting/ (search excerpt)

---

## 22. Rounding and number display

Known behavior:

- **Totals display as whole currency units by default.** A system preference (off by default, added mid 2026) shows a chosen number of decimals on totals at all three levels; reports can override it, and the Excel export follows it.
- **Rate column display rounding** is display only: a cutoff value splits rates into "below" and "above" bands, each with its own decimal places (as entered, or 0 to 8), padded with zeros or rounded half up. Optionally the first global used in a rate dictates its decimals. Calculation uses the full stored value.
- Currency rates hold five decimals.
- Global `Dec` setting controls displayed decimals for that global.
- Users report bottom-line totals that differ by about one currency unit from the sum of displayed lines, consistent with summing unrounded values while displaying rounded ones (or vice versa).
- Vendor screenshots show percent fringe rates displaying with floating-point artifacts (values a hair above or below the typed rate), which suggests binary floating-point storage of rates. Our engine uses decimal arithmetic and should document its rounding policy explicitly (where rounding happens: per line, per fringe, per posted total, or only at display).

Priority for us: **v1** for a documented, deterministic rounding policy and display options for totals and rates. Matching MMB's exact rounding on import is an open question.

Sources: https://mmb-docs.ep.com/Budget_Preferences/Budget_Prefs_Rate.html, https://mmb-docs.ep.com/System_Preferences/Sys_Prefs_Overview.html, https://mmb-docs.ep.com/Setup/currencies.html, https://mmb-docs.ep.com/Setup/globals.html, https://www.reddit.com/r/FilmTVBudgeting/comments/kh2w1m/how_do_i_disable_movie_magic_budgeting_from/ (search excerpt), https://downloads.adc.ep.com/doc/Get_Most_Out_Of_MMB10.pdf

---

## 23. Known calculation quirks and edge cases

- Fringe cutoffs only work "per person" if the user remembers to bracket that person's lines into a fringe range; unbracketed split lines over-fringe silently.
- Fringe ranges must be contiguous lines inside one account; a person paid across accounts cannot share a cutoff.
- Flat fringe unit conversion ignores Unit2 to Unit4, so a line expressed as "1 person x 5 days" with the time unit in a secondary column will not convert.
- The legacy percentage cutoff factor could change fringe totals invisibly in imported MMB7 budgets until it was surfaced in 2026.
- Importing MMB7 budgets "cleans up" errors, so totals can change on import; global syntax differences produce `#ERR`.
- Changing or deleting a currency silently reinterprets lines.
- Deleting a global freezes its value into every cell (irreversible except by undo).
- Paste between budgets uses the target's same-named fringes or globals, changing the pasted line's math.
- Subtotal adjustments ignore subtotal rows and accumulate from the previous adjustment row, not the previous subtotal.
- Hidden multiplier columns can change totals without being visible (hence the red marker).
- Charges and credits imported from older budgets can sit in non-standard Topsheet positions that new ones cannot reproduce.
- The manual's currency examples disagree on conversion direction (see section 8).
- Comparison matches only on account numbers, so renumbered budgets do not line up.

Priority for us: use as a test checklist; design our engine to avoid each trap or make it visible.

Sources: pages cited in sections 4, 5, 8, 9, 14 and 18; https://mmb-docs.ep.com/Projects_and_Budgets/importexport.html

---

## 24. Competitor features MMB lacks (or does weakly)

- **Real-time multi-user editing and cloud collaboration** (Saturation, Celtx; Showbiz offers cloud collaboration as an add-on). MMB only sends copies.
- **Actuals and cost tracking against budget**, purchase orders, petty cash, expense cards, budget-versus-actual and estimate-at-completion reports (Saturation, Gorilla's accounting module, Showbiz, Hot Budget actualization).
- **Overtime calculator** for labor lines (Showbiz).
- **Ratebook integration**: pick a union classification and rate type to fill the rate (Gorilla add-on). MMB has no built-in rate book.
- **Schedule-driven quantities**: link to a schedule and pull day-out-of-days counts, cast, crew and elements into detail lines (Gorilla; MMB only imports scheduling libraries as static shortcuts).
- **Optional fourth level** fed by department spreadsheets with an import map (Gorilla).
- **Spreadsheet import** with column mapping (Gorilla, Saturation's CSV import).
- **Commercial bid formats** (industry-standard commercial bid form templates, markup and bid/actual workflow) in Hot Budget and Showbiz.
- **Free tier and trials** (Saturation free tier, Showbiz trial). MMB has no free tier or trial.
- **Dynamic currency links** (Showbiz advertises them).
- StudioBinder and Cinergy budgeting were not examined in depth (see open questions).

Priority for us: **later** for actuals, schedule import, spreadsheet import with mapping (the last is cheap and a strong differentiator, consider pulling into v1 alongside MMB export import). **skip** payment rails, expense cards.

Sources: https://junglesoftware.com/movie-magic-budgeting-alternative/, https://sethero.com/blog/top-6-film-budgeting-softwares-compared/, https://www.wrapbook.com/blog/best-film-budgeting-software, https://saturation.io/, https://saturation.io/versus/hot-budget, https://www.mediaservices.com/showbiz-software/showbiz-budgeting/, https://saturation.io/blog/is-movie-magic-budgeting-free

---

## Gaps and pain points

User complaints and structural weaknesses gathered from forums, reviews and the vendor's own release notes (which frame each fix as a previous problem):

- **Subscription only, no trial, no free tier**, roughly 43 USD per month; single active session per account; mandatory login and multi-factor authentication; offline limited to seven days.
- **No real collaboration**: "send a copy" produces divergent forks; recipients of studio budgets are asked to send MMB7 files instead.
- **No backup or portable file story** for years (local files only arrived in 2024); a forum thread complains the only backup was exporting to MMB7.
- **MMB7 round trips lose data or behave oddly** (notes, budget info, tags, digit-leading globals, units, flat fringes when base and display currency differ).
- **No Excel/CSV import**; only paste of visible values.
- **Rounding**: totals were whole dollars only until 2026; bottom lines can be off by about one unit.
- **Hidden legacy fringe factor** changed fringe math with no visible cause.
- **Performance** on large budgets (repeated release-note fixes, a beta "faster grid", lag in setup screens and tools pane).
- **Fewer visible rows than MMB7** until grid zoom was added.
- **Many basic editing features arrived late**: fill handle, bulk value entry, non-adjacent selection, per-line formatting, editable shortcuts, tab reordering, applying sets/locations in bulk.
- **Report designer limits**: fringe and category print order did not follow setup order until 2026; no per-section filtering until 2026.
- **Comparison only by account number**, cloud budgets only, max 16, no detail-line comparison.
- **No actuals, cost reports, rate book, overtime calculator, or schedule linkage.**
- **Steep learning curve** cited by reviewers; vendor removed in-app tutorials in 2026 while rebuilding them.
- **No localization** (English only).

Sources: https://www.reddit.com/r/FilmTVBudgeting/comments/16xrk7v/reasons_i_hate_mmb_10_part_137/ (search excerpts), https://www.reddit.com/r/FilmTVBudgeting/comments/10swt8y/mmb10_feedback/ (search excerpt), https://www.reddit.com/r/FilmTVBudgeting/comments/1er66hn/mmb_update/ (search excerpt), https://www.reddit.com/r/FilmTVBudgeting/comments/kh2w1m/how_do_i_disable_movie_magic_budgeting_from/ (search excerpt), https://mmb-docs.ep.com/release_notes.html, https://www.ep.com/support/movie-magic-budgeting/, https://downloads.adc.ep.com/doc/Get_Most_Out_Of_MMB10.pdf, https://saturation.io/blog/is-movie-magic-budgeting-free, https://www.instagram.com/reel/Dcr28fhjVXC/ (search excerpt). Reddit pages could not be fetched directly, only search-result excerpts were seen.

---

## Open questions

1. **Export schemas.** Exact columns and structure of Tab Delimited, XML (Basic), XML (Advanced), JSON and Excel exports are not public. Action: have a licensed user export a purpose-built test budget in every format; analyze only those open-format files.
2. **Currency conversion direction.** The manual's examples conflict. Verify with an exported multi-currency test budget.
3. **Percentage Fringe Cutoff Factor.** What does a factor of 1.5 or 2.0 actually do (scale the cutoff amount, or the base)? Needed for faithful import of legacy budgets.
4. **Flat fringes and ranges.** Are flat fringes charged once per line or once per range, and are quantities aggregated across a range for the hours conversion? The manual says both.
5. **Do flat fringes multiply by line quantity** when the line unit has no hours equivalence (for example an "allow" unit)?
6. **Fringes on subtotal adjustment rows**: can fringes be applied to them, and do adjustments include fringe dollars of preceding lines?
7. **Charges base**: does the ATL+BTL base include posted fringes in every posting mode (notably post-by-budget, where the fringe row sits outside the production totals)? Do charges compound on each other? Does a flat fee really subtract excluded amounts?
8. **Credits and currency**: are credit caps in base or display currency?
9. **Group conflicts**: if a line carries one suppressed and one included group, is it suppressed? (MMB7 had an explicit rule.)
10. **Do suppressed lines still count toward fringe cutoffs** within a fringe range?
11. **Rounding points**: are line subtotals, fringe amounts or posted fringe totals rounded before summing? Reproduce the reported one-unit drift.
12. **Original column** after structural edits (inserted or deleted accounts) and inside sub-budgets.
13. **Globals with units**: does a global's unit do anything in calculation, or is it a label?
14. **Units on X, Rate and 4X**: purely labels, or used anywhere besides display?
15. **Account-level subtotal rows** versus detail subtotal rows: the manual describes both under different names; confirm they are the same feature.
16. **StudioBinder and Cinergy budgeting features** were not researched in depth; a follow-up pass is needed.
17. **MMB7 features absent from MMB10** (formula maker window, caption renaming, cost-report comparison, line-item charge exclusions) were seen only as MMB7 table-of-contents entries; decide whether any matter.
