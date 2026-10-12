# Movie Magic Budgeting parity tracker

Status: private research notes. Last reconciled 2026-10-11 against engine `main` at 8c1c772,
then updated for decision 59 (new-budget defaults, cutoff factor, totals currency, topsheet
order, fringe row, global units, formula entry).

One row per feature of Movie Magic Budgeting 10 (MMB, 10.35.7) that we track, mapped to where
Method Budget has it and how we know it matches. It reconciles five sources:

- `docs/research/mmb-feature-inventory.md` (inv): every MMB feature, sections 1 to 24, each
  tagged v1, later or skip. The section tables below follow its numbering.
- `docs/ROADMAP.md`: our work items (1.x, 2.x, 3.x) and the engine checks (EC n).
- `docs/decisions.md` (D n): entries 54 to 59 are the parity work.
- The parity run of 2026-10-10 and 2026-10-11 in the real app (gitignored, main checkout only):
  `imports/mmb-parity/2026-10-10/run-log.md` (case by case, every figure read) and
  `parity-report.md` (sections A to E), with raw exports in `exports/ZTESTnn/` and the
  2026-10-11 XML, Excel and tab-delimited exports in `imports/mmb-parity/2026-10-11/exports/`.
  Case ids (LM-14, P-02, F-19, CU-04, G-02, T-01, GR-03, V06-2 and so on) are the run's.
  `crates/method-core/tests/mmb_parity.rs` reproduces every figure the engine was changed for.
- `docs/APP.md`, `docs/CLI.md`, `docs/FORMAT.md` and the code, for what is actually built.

## How to read it

Each row: **Feature** (in our words) | **Inv** priority from the inventory (v1, later, skip; "ours"
for features MMB does not have) | **Status** | **Where** (roadmap id, decision, file) |
**Evidence** (parity case ids, "unit tests only", or none) | **What's left**.

Status key:

| Status | Meaning |
|---|---|
| **Verified** | Built, and our figures or behavior matched the real app in the parity run (directly, or the engine was changed afterwards and `mmb_parity.rs` reproduces the captured figures). |
| **Built** | Built and tested on our side, never checked against the real app. Many app features also still need a live check in our own app (APP.md "Not yet"). |
| **Partial** | Some of it built, or built but known to differ from MMB without a decision to differ. |
| **Missing** | Not built. |
| **Skip** | Deliberately out of scope; the reason is in What's left. |
| **Differs by choice** | We intentionally deviate (a decision says so), usually with MMB's behavior as an opt-in or ours as an opt-in. |

"Verified" is about the real app's figures and visible behavior. It does not mean our app's UI
has had its live check.

## How to update it

- After a parity run: add the run folder to the sources above, move rows from Built to Verified
  (or to Partial when MMB differs), add the case ids to Evidence, and add any new MMB behavior to
  section F.
- After an engine or app change: update Status and Where (decision number, roadmap id). When a
  decision settles a difference, the row becomes Verified (we now match) or Differs by choice.
- Recount the summary table: `grep -c` the Status column per section, or recount by hand.
- Keep it in our own words. Never paste MMB help text, templates or exported data here.

---

## Summary

Counts of rows per status, per section.

| Section | Verified | Built | Partial | Missing | Skip | Differs by choice | Rows |
|---|---|---|---|---|---|---|---|
| 1 Budget hierarchy and structure | 13 | 9 | 1 | 0 | 0 | 0 | 23 |
| 2 Detail lines: columns and calculation | 21 | 7 | 2 | 2 | 0 | 0 | 32 |
| 3 Units | 3 | 5 | 0 | 0 | 0 | 1 | 9 |
| 4 Fringes | 27 | 8 | 1 | 1 | 0 | 1 | 38 |
| 5 Globals | 9 | 5 | 2 | 0 | 0 | 0 | 16 |
| 6 Groups | 9 | 6 | 0 | 1 | 0 | 0 | 16 |
| 7 Sets and locations | 2 | 4 | 0 | 0 | 0 | 0 | 6 |
| 8 Currencies | 7 | 5 | 0 | 1 | 0 | 0 | 13 |
| 9 Contractual charges | 13 | 4 | 0 | 1 | 1 | 0 | 19 |
| 10 Applied credits | 11 | 0 | 0 | 2 | 0 | 0 | 13 |
| 11 Account coding | 0 | 4 | 1 | 0 | 0 | 0 | 5 |
| 12 Setup tables, library and templates | 0 | 6 | 1 | 0 | 0 | 0 | 7 |
| 13 Shortcuts | 0 | 2 | 0 | 2 | 0 | 0 | 4 |
| 14 Scenario and comparison | 4 | 4 | 1 | 1 | 0 | 1 | 11 |
| 15 History, undo, saving and file management | 0 | 5 | 0 | 0 | 2 | 0 | 7 |
| 16 Collaboration, notes and sharing | 0 | 3 | 0 | 0 | 2 | 1 | 6 |
| 17 Find and replace | 0 | 3 | 0 | 0 | 0 | 0 | 3 |
| 18 Grid editing and keyboard | 0 | 10 | 2 | 0 | 0 | 2 | 14 |
| 19 Screens and workflow states | 0 | 3 | 0 | 0 | 1 | 0 | 4 |
| 20 Reports, printing and PDF | 0 | 8 | 2 | 2 | 0 | 0 | 12 |
| 21 Import and export formats | 0 | 3 | 2 | 2 | 1 | 0 | 8 |
| 22 Rounding and number display | 4 | 2 | 1 | 0 | 0 | 1 | 8 |
| 24 Competitor features MMB lacks | 0 | 4 | 0 | 4 | 3 | 0 | 11 |
| F MMB behavior facts | 11 | 8 | 1 | 0 | 0 | 3 | 23 |
| **Total** | **134** | **118** | **17** | **19** | **10** | **10** | **308** |

Section 23 (calculation quirks) has no rows of its own: each quirk is a row in the section it
belongs to. Section 19 is informational in the inventory; only its concrete screens are listed.

---

## 1. Budget hierarchy and structure

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Three levels: topsheet categories, accounts, detail lines, totals rolling up | v1 | Verified | D9, FORMAT Budget | Every ZTEST; ZTEST04 export account totals; ZTEST06 categories 16,500/10,800/4,100/1,100 | none |
| Drill down and climb, breadcrumb with sibling jump lists | v1 | Built | 1.7, APP Keyboard and menus | none | Live check of keys in the app |
| Next/previous account wraps across categories | v1 | Built | 1.7, EC 16 | none | EC 16: check in MMB on the last account of a category |
| Production totals (level breaks), any number, renameable | v1 | Verified | D9, D56 | ZTEST06, ZTEST11 | none |
| ATL is the first level (rows up to the first production total) | v1 | Verified | D56 (A5) | ZTEST11 T-01, T-02, ZTEST06 ATL 16,500 | none |
| BTL is every later level a production total closes | v1 | Verified | D56 (A5) | ZTEST06 BTL 14,900, V11-3b 0/10,000 | none |
| Categories after the last production total count in neither ATL nor BTL; none at all means ATL = BTL = 0 | v1 | Verified | D56 (A5) | T-01 0/0, T-02 1,000/0, ZTEST06 | none |
| Empty first level (production total at the top) is ATL 0, not skipped | v1 | Verified | D56 | T-04 (V11-3) | none |
| Summary rows (Total ATL, BTL, ATL+BTL) with a show/hide setting | v1 | Built | 2.18, FORMAT display `summary_rows` | none | MMB adds the three rows itself on the first Insert Production Total; check ours shows them the same way |
| Total Fringes row(s) placed per posting mode | v1 | Verified | D9, D56 | ZTEST06 V06-1/2/3 | none |
| Budget-posted Total Fringes row movable (Move Fringe Total, Reset to Default Position) | v1 | Built | D59, FORMAT `fringe_row.position`, Settings > Fringe totals | Unit tests (V06-3 totals unchanged at every place); seen in MMB menus (ZTEST11), PM-POS not tested | PM-POS: check in MMB that moving it changes no ATL/BTL (ours keeps the math unchanged) |
| Budget fringe row's own account number and description | v1 | Verified | D59, FORMAT `fringe_row` | Defaults read at C2: no number, "Total Fringes" | none |
| Contractual charge rows below the last production total, before summary rows | v1 | Verified | D59, `Report::topsheet_lines` | V09-2 (creation order, newest first) | Order matched. Position: ours after the summary rows (brief); ROADMAP decision 13 says MMB puts them before: confirm in the next run |
| Grand total = levels + posted fringes + charges | v1 | Verified | D9, D56 | ZTEST06 34,125, ZTEST09 41,389.91 | none |
| Applied credits between grand and net, "Sub Total" row with two or more | v1 | Verified | D9, D59, `Report::topsheet_lines` | ZTEST10: ZCR3, 5, 6, 4, 2, 1, Sub Total 3,349.53, net 5,450.47 | none |
| Net total = grand minus credits | v1 | Verified | D9 | ZTEST10 net 5,450.47, V10-2 1,812.87 | none |
| Net total show/hide | v1 | Built | 2.18, FORMAT display `net_total` | V06-2 follow-up: toggling it changes no figure in MMB | none |
| Account-level subtotal rows (running total since previous subtotal) | v1 | Verified | D7 | ZTEST03 A-03 3,000.00 | Hiding them from reports while they still calculate: not found in report options |
| Footer totals: grand, ATL+BTL, ATL, BTL, credits | v1 | Built | APP footer | none | none |
| Session change and last change counters with reset | later | Built | APP footer | MMB's own counter read 0.00 after toggles (V07-1b); ours unit tests only | none |
| Deleting, cutting, copying a category or account carries its children | v1 | Built | 1.8 | unit tests only | none |
| Hide zero-balance accounts (view only) | later | Built | 2.18 (View menu) | none | none |
| Per-line bold, italic, underline, font color; line color beats group color | later | Partial | 1.6, D16 | none | Underline not offered (bold, italic, indent, color are) |

## 2. Detail lines: columns and calculation

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Line = Amt x X x Rate x 4X | v1 | Verified | D55, `calc::lines` | LM-01..13, D54 line 7,000.00 | none |
| Cleared Amt or Rate counts as 0 | v1 | Verified | FORMAT detail row | LM-05..09 | none |
| Amt as number, fraction or formula | v1 | Verified | D5, D55 | LM-23 (1/3), G-01 | none |
| Primary Unit (after Amt), used for flat-fringe hours | v1 | Verified | D54 | F-08..F-18 | none |
| X multiplier | v1 | Verified | D55 | LM cases, G-05 | none |
| 4X multiplier | v1 | Verified | D55 | D54 check (25 Days x 14 x 4X 2 = 7,000.00) | CU-03 (4X with currency) skipped |
| Unit2, Unit3, Unit4 are labels only | v1 | Verified | D8 | ZTEST01 ("unit columns are labels"), D54 | none |
| Per-line currency (Curr) | v1 | Verified | D10 | CU-01..06 | none |
| Rate as number or formula with globals | v1 | Verified | D5 | G-05 | none |
| Binary-double arithmetic, cents rounded like JS Math.round (default) | v1 | Verified | D55, FORMAT `arithmetic` | LM-14, LM-17, LM-18, LM-23, LM-28, F-19, V04-2 | none |
| Exact-decimal arithmetic as a per-budget option | ours | Built | D55, `arithmetic: exact` | unit tests only | none (ours only) |
| Rates and numbers keep more than 9 decimal places | v1 | Verified | D55 (A9) | V01-1 (0.0000000149 x 1e8 = 1.49) | none |
| Formulas: + - * /, parentheses, unary minus, precedence | v1 | Verified | D5 | G-04, G-06, G-07 | none |
| Bad formula, divide by zero, circular or missing global shown as an error | v1 | Partial | D5, D59, APP problem cells | V02-4: divide by zero and circular globals refused at entry (app), as MMB | A typo or unknown global is still kept and flagged (MMB refuses any invalid Amt); D59 keeps that for now |
| Description keeps leading and repeated spaces | v1 | Built | FORMAT | unit tests only | none |
| Agg column (effective fringe percent after cutoffs) | v1 | Built | APP column menu "Fringe %" | none | EC 15: value on a suppressed line not checked |
| Agg Sum column (sum of percent rates before cutoffs) | v1 | Missing | none found | MMB exports carry `dAggSumPercent` (2026-10-11 XML) | Add an optional column |
| Fringes, Groups, Location, Set columns | v1 | Built | APP column menu, 2.11 short codes | none | none |
| Unit labels pluralize when value is not 1 | v1 | Built | FORMAT unit `plural` | unit tests only | none |
| Fraction display as typed vs decimal | later | Missing | none found | MMB shows 1/3 as 0.33333 in Amt (parity report B) | Low priority |
| Negative values in parentheses | v1 | Built | APP export format, grid | none | none |
| Hidden-factor marker (hidden X, 4X or currency changes the total) | later | Built | 2.19 | none | Live check in app |
| Subtotal row: running total back to the previous subtotal row | v1 | Verified | D7 | ZTEST03 A-03, second subtotal 1,520.00 | none |
| Subtotal rows include adjustment rows above them | v1 | Verified | D7 | ZTEST03 1,520.00 | none |
| Adjustment row: rate locked to the sum since the previous adjustment (subtotal rows ignored) | v1 | Verified | D7 | ZTEST03 A-05 350.00, A-07 20.00 | none |
| Adjustment Amt 0 restarts the window | v1 | Verified | D7 | ZTEST03 A-09 | none |
| X multiplies an adjustment | v1 | Verified | D7 | ZTEST03 A-15 60.00 | none |
| Adjustment sum added up in doubles | v1 | Verified | D55 | CU-10 (132,347.66999999998) | none |
| Adjustments exclude fringe dollars of the lines above | v1 | Verified | D7, EC 7 | ZTEST07 adjustment 550 = 10% of 5,500 lines | none |
| Fringes allowed on adjustment rows | v1 | Verified | D56 (A10) | ZTEST03 ZP10N 35.00 on 350.00 | Flat fringe on an adjustment counts its Amount (chosen, not checked) |
| Suppressed lines drop out of subtotal and adjustment windows | v1 | Verified | D4, EC 2 | ZTEST07 subtotal 5,500, GR-02 | none |
| New-line copy from line above (fringes on, others off by default) | v1 | Partial | 1.4, FORMAT `new_line_defaults` | MMB defaults read at C2 (Fringe on, Group/Location/Set/Currency off) | Defaults match; Amt starting at 1 and one ready line in an empty account still not compared |

## 3. Units

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Unit table: singular, plural, description, hours equivalence | v1 | Verified | D8, FORMAT unit | F-08..F-18 conversions | none |
| A unit with 0 hours behaves as no hours | v1 | Verified | D54 | F-24 | none |
| `%` as a protected unit marking percent fringes | v1 | Differs by choice | FORMAT fringe `kind` | none | Ours: a fringe kind (percent or flat), not a unit. Importer maps `%` to percent |
| Starter units for new budgets | v1 | Verified | D59, `model.rs default_units`, `starter::new_budget_settings` | ZTEST05: Day 12, Week 60, Hour 1, Month 240, Allow and Flat no hours | none (MMB's `%` unit is our fringe kind) |
| Type-ahead in unit cells | v1 | Built | 1.2 | none | none |
| Unknown unit offers "add it" | v1 | Built | 1.3 | none | none |
| Bulk apply a unit to many lines | v1 | Built | 1.8 cell paste, fill down | none | Tools-pane toggle (click again removes) not built as such |
| Deleting an in-use unit warns | v1 | Built | 2.9 | unit tests only | none |
| Unit hours must be whole numbers | v1 | Built | FORMAT unit `hours` (decimal) | MMB refuses 7.5 ("must be a whole number"), ZTEST05 | Ours accepts decimals (superset); importer unaffected |

## 4. Fringes

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Fringe name, description | v1 | Built | FORMAT fringe | none | none |
| Short ID shown in the grid | v1 | Built | 2.11 | none | none |
| Percent fringe = rate x line base | v1 | Verified | D3, D55 | P-01..P-10 | none |
| Percent cutoff, single earner | v1 | Verified | D3 | P-02 277.78, P-05, P-10 | none |
| Cutoff in base currency | v1 | Verified | D10 | CU-08 277.78 | none |
| Fringe on converted foreign amount | v1 | Verified | D10 | CU-07 300.00 | none |
| Fringe ranges: bracket contiguous lines as one earner | v1 | Verified | D3, D56 | ZTEST04 1102 range total 277.78 | none |
| Range cutoff used up in line order, each line rounded alone | v1 | Verified | D56 (A2) | 1102 156.17/121.61/0.00, 1105 187.50/90.28 | none |
| Range across a subtotal row | v1 | Verified | D56 | 1104 100.00/0.00 | none |
| Line without the fringe inside a range uses none of the cutoff | v1 | Verified | D56 | 1203 60.00/none/40.00 | none |
| Suppressed line inside a range uses none of the cutoff | v1 | Verified | D56, EC 10 | 1201 100.00/Excl/0.00 | none |
| Negative line after the cutoff is used earns 0 | v1 | Verified | D56 | 1106 100.00/0.00 | none |
| Two ranges in one account keep separate cutoffs | v1 | Verified | D56 | 1202 100/0/100/0 | none |
| Per-line rounding inside a range | v1 | Verified | D56 | 1204 0.01 x 3 = 0.03 | none |
| Ranges stay inside one account; no cross-account person | v1 | Verified | D3 | inv 23 (documented MMB limit) | Person-level cutoffs across accounts are 3.11 (ours, Missing, later) |
| Making and removing ranges | v1 | Built | 1.5, Apply panel | none | Bracket drawing and range editing in the grid itself (APP Not yet) |
| Flat fringe counts the Amount only, through unit hours | v1 | Verified | D54 | D54 table, F-08..F-18 | none |
| Flat fringe charged once when either unit has no hours | v1 | Verified | D54 | F-20..F-24, D54 "2 Allow" | none |
| X and 4X never count for a flat fringe | v1 | Verified | D54 | F-28, D54 | none |
| Flat fringe: negative, zero and zero-total lines | v1 | Verified | D54 | F-25, F-26, F-27 | none |
| Flat fringe in base currency, not converted | v1 | Verified | D10 | CU-09 | none |
| Cutoff ignored on a flat fringe | v1 | Verified | D54 | F-31 | none |
| Percent base excludes flat fringe dollars | v1 | Verified | D3 | F-32 | none |
| Flat fringe in doubles (count first, then rate) | v1 | Verified | D55 | F-19 0.00 | none |
| Flat fringes charged per line, not per range | v1 | Built | D54 | F-33..F-35 skipped | Next parity run: a range with a flat fringe |
| Flat fringe basis `amount_counts` and `quantity` (opt-in) | ours | Built | D54, FORMAT fringe `basis` | unit tests only | none (ours only) |
| Most hours a day cap on per-hour fringes | ours | Built | D54 `max_hours_per_day` | MMB has no such field (ZTEST05) | none (ours only) |
| Posting by budget | v1 | Verified | D9 | V06-3, ZTEST08 | none |
| Posting by production level | v1 | Verified | D9, D56 | V06-2 (with a closing production total) | none |
| Production-level posting drops trailing-category fringes | v1 | Differs by choice | D56 (A6), `trailing_level_fringes` | V06-2 grand 34,020; follow-up: not a setting in MMB | Ours keeps them by default and warns; `drop` matches MMB |
| Posting by category, suffix 99 | v1 | Verified | D9, D11 | ZTEST06 main (1199/2199/3199/4199) | none |
| Changing the suffix renumbers fringe accounts | v1 | Verified | D11 | V06-4 (98) | none |
| Category fringe account numbering for 3-digit or alphanumeric codes | v1 | Built | `coding.category_fringe_account` | PM-CAT skipped | Next parity run: a 3-digit category |
| Posting by account | v1 | Verified | D9 | V06-1 | none |
| Percentage fringe cutoff factor (1.0 to 3.0) | later | Verified | D59, FORMAT `cutoff_factor`, Settings > Calculation | V04-1/2/3: P-02 416.67/555.55/625.00, P-10 416.67/462.96/462.96, P-01 125.00 | 2.5 not read in MMB; factored cutoff rounded to cents (chosen) |
| Sub-budget fringe lock | later | Missing | 2.22, `subbudget.rs` | MMB wizard offers "Lock Fringe Values" (ZTEST12) | Not built |
| Fringe display order and folders | later | Partial | 2.12 custom order, 2.15 library folders | none | Folders in the budget's own setup tables not built |
| In-use count, total dollars, delete warning | v1 | Built | 2.9 | unit tests only | none |

## 5. Globals

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Named globals referencing other globals (chains) | v1 | Verified | D5 | G-03, chain L1..L6 = 64 | none |
| Globals as doubles | v1 | Verified | D55 (A8) | G-02 85,000,000.00 | none |
| Global names may start with digits | v1 | Verified | D56 (A11) | V02-3 "1ZG" | none |
| Globals in Amt, X, Rate | v1 | Verified | D5 | G-01, G-05 | none |
| Change propagates everywhere | v1 | Verified | D5 | V02-1 | none |
| Deleting a global in use freezes its value | v1 | Verified | D12, D55 | V02-2 | none |
| Circular or missing reference is an error | v1 | Verified | D5, D59 | V02-4: #ERR with value 0, refused in a cell and as a global (app) | Engine and `method check` still stop on a cycle (kept); a missing global is flagged |
| Global's unit fills the line's Unit when typed into Amt | v1 | Verified | D59, `edit::unit_from_global` (app entry) | G-08: MMB fills "Weeks" | Importer awareness (import agent's work) |
| Description, unit, decimals, show by name | later | Built | 2.10, D34 | none | none |
| Global folders | later | Partial | 2.10 | none | Folders don't group the list yet |
| Type-ahead for global names | v1 | Built | 1.2 | MMB: Tab while the list is open garbles the cell (ZTEST02) | none |
| Find and replace by global | later | Built | 1.11, D26 | none | none |
| Globals in report header and footer | later | Built | 2.4 | none | none |
| First global in a Rate dictates its decimals | later | Verified | D34, display `rate_global_decimals` | C2: MMB default on; ours default true | none |
| Formula view toggle for a selection | later | Built | 2.10 | none | none |
| A fringe rate can be a global | v1 | Partial | FORMAT fringe `rate` formula | MMB refuses ("Rate must be a positive number"), ZTEST04 | Ours allows (superset); exports to MMB must freeze it |

## 6. Groups

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Many groups per line | v1 | Verified | D4 | ZTEST07 GR-04 | none |
| In budget total off suppresses the line | v1 | Verified | D4 | GR-02 Excl | none |
| Toggling a group off and on never drifts | v1 | Verified | D4, EC 3 | V07-1b 7,150 exactly, Chng 0 | none |
| Conflict rule, default exclude | v1 | Verified | D57, EC 9 | GR-03 | none |
| Conflict rule include | v1 | Verified | D57 `group_rules.conflict` | V07-4 grand 10,450 | none |
| Non-grouped lines rule (exclude also drops ungrouped adjustment rows) | v1 | Verified | D57 `group_rules.ungrouped_lines` | V07-3 grand 6,100 | none |
| Fringes in group totals rule | v1 | Built | D57 `group_rules.fringes_in_totals` | Default seen (ZTEST04 header); exclude not tried | Next run: switch to Exclude and read a group total |
| Group total = base of every tagged line + fringes actually earned | v1 | Verified | D58 | ZTEST07 ZOFF 5,000/0, ZTEST10 ZCRED 6,500/4,500 | none |
| Group active total | v1 | Verified | D58 | ZTEST07 ZIN 8,500/5,500 | none |
| Group color | later | Built | FORMAT group `color` | none | none |
| Color priority 1 to 20 | later | Missing | none | none | Ours: first group by name with a color |
| Display order | later | Built | 2.12 custom order | none | none |
| Mass apply from topsheet or category cascades to existing lines | later | Built | 1.19 (lines under selected accounts or categories) | none | none |
| Tri-state boxes for mixed selections | later | Built | 1.22 Apply panel | none | none |
| Group totals report | v1 | Built | 2.5 | none | none |
| Groups drive sub-budgets, charge exclusions, credit rows | v1 | Verified | D9, 2.22 | ZOVER, ZCR1, ZTEST12 S1 | none |

## 7. Sets and locations

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| One set per line | v1 | Built | FORMAT set | none | none |
| One location per line, with city, region, country | v1 | Built | FORMAT location | none | none |
| Set and location totals include fringes, leave out suppressed lines | v1 | Verified | D4 breakdowns | GR-SET S1 5,500 S2 0; GR-LOC L1 1,600 L2 600 | none |
| Set codes limited to 1 to 3 characters | v1 | Built | FORMAT (no limit) | MMB refuses longer (ZTEST07) | Ours accepts any length (superset); export to MMB would need short codes |
| Create on the fly, bulk apply, partial state | v1 | Built | 1.3, 1.22 | none | none |
| Set and location in account coding and exports | v1 | Verified | D11 `set_width`, 2.1 | MMB JSON carries dLocation, dSet (ZTEST00) | none |

## 8. Currencies

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Currency table: code, description, symbol, rate | v1 | Built | FORMAT currency | none | none |
| Direction: line x rate = base | v1 | Verified | D10, EC 4 | CU-01 1,500.00 | none |
| Foreign line converted unrounded, rounded once | v1 | Verified | D55 (A7) | CU-04 1.01 | none |
| Rate precision beyond 5 places | v1 | Verified | D10 (9 places) | CU-06 CAD 1.234565 | none |
| Other currencies (GBP, JPY) | v1 | Verified | D10 | CU-02, CU-05 | none |
| Flat charge in its own currency | v1 | Verified | D9 | ZFLATEUR 3,000.00 | none |
| Fringes and cutoffs in base currency | v1 | Verified | D10 | CU-07, CU-08, CU-09 | none |
| Totals shown in another currency | v1 | Verified | D59, `Report::display` | V08-1 EUR 225.92 / 49,253.40, V08-2 GBP 118,731.57 / 549.00 / 119,685.57 | Account-posted and level fringe rows converted per fringe (chosen, not checked) |
| Symbol after the value, separator pattern (1,000.00 vs 1.000,00) | v1 | Missing | none in FORMAT | none | Add to currency or display settings |
| Changing the base currency reinterprets blank-currency lines | v1 | Built | 1.18 base rename | V08-3 skipped | Next run: V08-3 |
| Deleting a currency puts its lines back on base | v1 | Built | 2.9 | none | none |
| Bulk apply currency, type-ahead, add on the fly | v1 | Built | 1.2, 1.3, 1.22 | none | none |
| Currency type-ahead from a world currency list | v1 | Built | 1.2 | none | none |

## 9. Contractual charges

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Percent charge | v1 | Verified | D9 | ZPCT5 1,130.00 | none |
| Percent base = everything before charges, budget-posted fringes included | v1 | Verified | D56 (A3) | V06-3 1,625, V09-1 unchanged | none |
| Typed excluded amounts | v1 | Verified | D9 | ZPCTX 2,168.90 | none |
| Excluded groups, a line in two excluded groups counted once | v1 | Verified | D9 | ZOVER 1,980.00 | none |
| Excluded category (with its fringes, any posting mode) | v1 | Verified | D56 | ZPCTCAT 270.00 | none |
| Excluded account | v1 | Verified | D9 | ZPCTACC 663.00 | none |
| Flat fee less typed and group exclusions | v1 | Verified | D9 | ZFLAT 1,478.00, V09-3 2,278.00 | none |
| Flat fee honors category and account exclusions | v1 | Verified | D56 (A4) | ZFLATCAT 8,000.00 | Fee below its exclusions goes negative (chosen, not checked) |
| Charges never stack; application order irrelevant | v1 | Verified | D9, EC 8 | V09-2 | none |
| Half-cent rounding on a percent charge | v1 | Verified | D55 | ZHALF 100.01, V09-3 96.47 | none |
| Suppressed group changes charge bases | v1 | Verified | D4, D9 | V09-3 | none |
| Applied vs unapplied charges kept for reuse | v1 | Built | FORMAT charge `applied` | none | none |
| Percent and excluded amount in the topsheet label | v1 | Built | 2.16 | none | none |
| Charge account number | v1 | Built | FORMAT charge `account` | none | none |
| Percent kept beyond its 2-place display | v1 | Verified | FORMAT charge `percent` decimal | ZHALF 0.4425 shown 0.44 | none |
| Charge position and order on the topsheet | v1 | Verified | D59, ROADMAP decision 13 | V09-2: creation order, newest first | Position: see section 1 |
| Switching percent to flat carries the computed amount | later | Missing | none | ZTEST09 note | Small editor nicety |
| Category and account exclusions set from the topsheet (tools pane) | v1 | Built | 2.16 exclusion grid | MMB: tools-pane Charges tab only | none |
| Line-item and budget-linked exclusions (MMB7 only) | skip | Skip | none | none | MMB7 contents page only; not in MMB10 |

## 10. Applied credits

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Group row gross (fringes in, suppressed out) | later | Verified | D9 | ZCR1 row 1 4,500 | none |
| Excluded fringes per credit | later | Verified | D57 (amended 2026-10-11) | ZCR1 excl 100, net 4,400 | none |
| Flat (typed) row | later | Verified | D9 | ZCR1 rows 2 and 3 | none |
| Cap types None, qualified spend, credit cap | later | Verified | D9 | ZCR1 CR 50, QS 300 | none |
| Overall cap, then discount | later | Verified | D9 | ZCR1 1,205.40, ZCR6 810.00 | none |
| Rows summed unrounded, rounded once | later | Verified | D9, D55 | ZCR4 0.01, ZCR3 411.52 | none |
| Category row (gross = category + its fringes, wherever they post) | later | Verified | D57 | V10-3 +4,300 | none |
| Excluded fringes apply to category rows too | later | Verified | D57 | 2026-10-11 run: category 1100 gross 4,500, excl 100 | none |
| Suppressing a credit's group | later | Verified | D4 | V10-2 ZCR1 343.00, net 1,812.87 | none |
| Gross, excluded, net, credit and cap variance columns | later | Verified | 2.16 | ZTEST10 editor columns | none |
| Account-type credit rows | later | Missing | D57 (category only) | Not tried in MMB | Check in MMB whether an account row exists, then add |
| Credit print view and credits setup report | later | Missing | 2.5 (not a section yet) | none | Add a report section |
| Credit order on the topsheet | later | Verified | D59 | ZTEST10 reverse application order, Sub Total | none |

## 11. Account coding

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Widths for category, account, sub-account, set; one separator | v1 | Built | D11, FORMAT account coding | none | none |
| Typed numbers parsed and normalized against the pattern | v1 | Built | D11 | unit tests only | none |
| Alphanumeric codes, generated fringe accounts too | v1 | Built | D11 | PM-CAT skipped | See section 4 |
| Renumber on pattern change | v1 | Built | 2.20 | none | none |
| Default pattern | v1 | Partial | D59, `AccountCoding::new_budget` | MMB default: Category 2, Account 2, Set 3 (ZTEST07); widths matched for new budgets | Separator: ours `-`, MMB exports write `1101` (none?): owner call |

## 12. Setup tables, library and templates

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| A setup table per tool | v1 | Built | 1.22, D42 Budget Setup window | none | Live check |
| Custom order, sort by header | later | Built | 2.12 | none | none |
| Setup folders per tool in the budget | later | Partial | 2.15 (`setup_folders`, library) | none | Folders in a budget's own tables |
| Library with dependency copying and conflict review | later | Built | 2.15, D37 | none | none |
| Templates: create from budget, new from template | v1 | Built | 1.20, D29 | none | none |
| Bundled chart-of-accounts templates | v1 | Built | 3.16, D51 | none | none |
| Budget info: built-in and typed custom fields | v1 | Built | FORMAT info value | MMB exports list built-in fields (2026-10-11 XML `defaultfields`) | Importer maps them to info labels |

## 13. Shortcuts

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Tools shortcut, add or replace mode | later | Built | 1.19, D31 tag presets | none | none |
| Line shortcut (saved block of lines) | later | Built | 2.14, D31 | none | none |
| Numeric hotkeys bound to shortcuts | later | Missing | none found | none | Low priority |
| Scheduling element import as shortcuts | later | Missing | 3.8 (schedule-linked globals instead) | none | Open formats only |

## 14. Scenario and comparison

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Original and variance columns (variance = total minus original) | v1 | Verified | 1.13, D12, D27 | ZTEST12 S5b (500), (200), (700) | none |
| A deleted line's original stays in its account and category | v1 | Verified | D27 | ZTEST12 S5b | none |
| Lock Original Totals (freeze, refresh disabled) | v1 | Partial | ours: Baseline Lock captures | ZTEST12: MMB lock only freezes; originals start at 0 | Add a freeze switch; ours has no baseline until captured |
| Versions copy totals and originals | v1 | Verified | 1.12, D17 | ZTEST12 S7 | none |
| Versions screen (rename, note, restore, delete) | v1 | Built | 1.12 | none | none |
| Sub-budget filter by categories, accounts and tags | later | Verified | 2.22, D39 | ZTEST12 S1 grand 1,650 | S2, S3 skipped |
| Sub-budget "include lines without any value" switch | later | Built | 2.22 | ZTEST12 wizard | none |
| Sub-budget inherits the parent's originals | later | Built | 2.22 | MMB does (ZTEST12) | Check ours carries the baseline |
| Sub-budget strips fringes or notes | later | Built | 2.22 | none | none |
| Sub-budget fringe lock | later | Missing | `subbudget.rs` | none | See section 4 |
| Comparison of up to 16 budgets by account number | later | Differs by choice | 1.14, 2.7, D17, D27 | MMB compare needs cloud budgets, so it could not be checked | Ours compares local files and versions, base stays editable, drills to lines; math is "both" (decision 1) |

## 15. History, undo, saving and file management

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Undo and redo, 200 steps | v1 | Built | D18 | none | none |
| History list with revert and branch to version | later | Built | 2.23, D40 | none | Live check |
| Autosave on by default, toggle, unsaved prompt | v1 | Built | 1.1, D20 | none | none |
| Local standalone files | v1 | Built | `.mthd`, D13 | none | none |
| Recent files, open from Finder | v1 | Built | 1.1 | none | none |
| Vendor cloud, licensing, one session per account | skip | Skip | D14, D48 | none | Local files only by design |
| Manage screen (folders, favorites, archive, trash) | skip | Skip | none | none | The file system does this |

## 16. Collaboration, notes and sharing

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Notes on lines, accounts and categories, dated, several per row | later | Built | 2.8, D35 | none | none |
| Note replies | later | Differs by choice | ROADMAP decision 11 | none | No replies by choice |
| Send a copy | skip | Skip | stand-in 2.21 share-ready copies | none | Plus diff and merge (3.1, 3.2) |
| Vendor accounting link | skip | Skip | none | none | Open exports instead (2.1) |
| Split screen | later | Built | 2.26 | none | Live check |
| Notes report | later | Built | 2.5, 2.8 | none | none |

## 17. Find and replace

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Go to account number | v1 | Built | 1.11 | none | none |
| Find by description, fringe, global, group, set, location, unit, currency, numbers | later | Built | 1.11, D26 | none | Live check of keys |
| Replace, add, remove; token-exact globals | later | Built | 1.11 | none | none |

## 18. Grid editing and keyboard

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Enter down, Tab right, new row past the last | v1 | Built | APP Using it | MMB: Return starts the next row at its Description (C3/C4) | none |
| Insert and delete rows, confirm on parents | v1 | Built | 1.5 | none | none |
| Multi-select, non-contiguous rows | v1 | Built | APP | none | none |
| Bulk entry (type once into many cells) | later | Partial | fill down, cell paste | none | Type-once into a selection not confirmed |
| Fill handle drag | later | Partial | fill down | none | No drag handle |
| Copy and paste cells and rows with children and tags | v1 | Built | 1.8, D23 | none | none |
| Paste between budgets: same-named tools in the target win | v1 | Differs by choice | D23 paste review | none | Target wins, but a review list shows first |
| Paste to and from spreadsheets as tab-separated text | v1 | Built | 1.8 | MMB rejected a tab-separated paste (C3/C4) | none |
| Column chooser | v1 | Built | APP column menu | none | none |
| Column widths and visibility per budget | later | Differs by choice | APP (remembered per user) | none | Ours per user, not per budget |
| Grid zoom | later | Built | 2.18 (70 to 150%) | none | none |
| Keyboard map, setup keys as MMB's | v1 | Built | 1.7, ROADMAP decision 2 | none | Recording wishlist 1 |
| Right-click menus | v1 | Built | 1.5 | none | none |
| Tools pane dock right or bottom, pop out, tabs | later | Built | 1.22, 2.17 | none | Live check |

## 19. Screens and workflow states

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Budget window: tabs, breadcrumb, grid, tools, footer | info | Built | 1.9, APP | none | none |
| Budget preferences screen | info | Built | Budget Setup > Settings | MMB prefs read at C2 | none |
| Report setup with preview | info | Built | 2.6 | none | none |
| Open and manage screen with cloud tabs | info | Skip | none | none | The file system and Finder |

## 20. Reports, printing and PDF

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Topsheet, category summary, account detail | v1 | Built | 1.15, D28 | none | Live check of print and PDF |
| Account tree with tri-state boxes; leave out empty, zero, suppressed | v1 | Built | 1.15 | none | none |
| Portrait or landscape, page numbers, date, file name | v1 | Built | 1.15 | none | none |
| PDF and print | v1 | Built | 1.15 | none | Letter paper only |
| Formatting: type size, rules, shading, group colors, collapse details | later | Built | 2.3 | none | Point sizes are three steps |
| Report templates with header and footer zones and live fields | later | Built | 2.4 | none | Mixed text and fields in one slot; created and modified dates |
| Report templates as portable files | later | Partial | 2.4 (presets in the budget) | none | Presets travel by Import Setup, not as their own file |
| Setup-table sections, chart of accounts, budget info, group totals, notes | later | Built | 2.5 | none | none |
| Charges and credits as setup sections | later | Missing | 2.5 | none | Add sections |
| Cover page | later | Missing | none found | none | Add |
| Topsheet exclusions: collapse credits, hide total charges, hide total fringes | later | Partial | 1.15 | none | Not all of them found in the options |
| Live preview with page navigation | later | Built | 2.6 | none | none |

## 21. Import and export formats

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Import MMB XML (Advanced) and JSON | v1 | Partial | 1.16 (generic import 2.2 to reuse) | Samples now on disk: 2026-10-10 JSON (ZTEST00..12), 2026-10-11 XML Advanced, XML Basic, Excel, tab-delimited | Write the readers from the samples; exports lack global formulas, fringe cutoffs and suppressed lines (section F), so setup must come from elsewhere |
| Import MMB Excel | v1 | Partial | 1.16, 2.2 | 2026-10-11 Excel samples | Preset mapping for its layout |
| Import MMB tab-delimited and XML (Basic) | later | Missing | 1.16 | 2026-10-11 samples (account-level list: category, account, totals, a Total Fringes account) | Readers |
| Our open exports: Excel, CSV, tab-delimited | v1 | Built | 2.1, D30 | none | Live check in Excel and Numbers |
| Our native format `.mthd` (documented JSON) | v1 | Built | D13, FORMAT | none | none |
| Spreadsheet import with column mapping | v1 | Built | 2.2, D36 | none | Live check with real files |
| MMB-shaped exports (JSON or XML a downstream tool reads) | later | Missing | interop-formats.md | MMB JSON field names known (ZTEST00) | Allowed (interop decisions); not built |
| MMB native files (.mmbx, .mbd, templates, zip) | skip | Skip | HANDOFF, CLEAN-ROOM | none | Proprietary; never research the layout |

## 22. Rounding and number display

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Rounding points: each line once; rollups are sums of rounded cents | v1 | Verified | D2, D55, EC 11 | ZTEST01 grand 10,000,010,005,314.61 | none |
| Half-up on the double (1.005 is 1.00) | v1 | Verified | D55 | LM-14, LM-17, LM-18, F-19 | none |
| Totals with or without cents | v1 | Differs by choice | 2.18 (View menu), D59 | MMB System Prefs default 0 decimals (C2) | Owner: totals keep showing cents (D59) |
| Rate display band (decimals below and above a cutoff) | later | Built | 2.18, display `rate_cutoff` | MMB default cutoff blank (C2) | none |
| Global display decimals | later | Built | 2.10 | MMB shows 0 dp for a whole-number global rate (parity B) | none |
| Shown values are decimals even in binary arithmetic | v1 | Verified | D55 | ZTEST01 LM-23 Amt 1/3 | none |
| Currency rate decimals | v1 | Verified | D10 | CU-06 | none |
| Report pages follow screen display settings | later | Partial | APP Not yet | none | Reports keep their own layout |

## 24. Competitor features MMB lacks

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| Actuals and cost report | later | Built | 3.7, D45 | none | Live check with real accounting exports |
| Overtime helper | later | Built | 3.10, D43 | none | none |
| Spreadsheet import with mapping | later | Built | 2.2 | none | none |
| Diff and three-way merge (our answer to send-a-copy forks) | ours | Built | 3.1, 3.2, D44 | none | none |
| Rate book | later | Missing | 3.9 | none | Rate packs from a separate data project |
| Schedule-linked quantities | later | Missing | 3.8 | none | Method Scheduling `sched-days` input agreed |
| Commercial bid layouts | later | Missing | 3.13 | none | Legal check on form layouts |
| Incentive modeling across locations | later | Missing | 3.6 | none | none |
| Real-time multi-user editing | skip | Skip | D14 | none | Local files; diff and merge instead |
| Payment rails, expense cards | skip | Skip | inv 24 | none | Out of scope |
| Live currency links | skip | Skip | ROADMAP "Skipped on purpose" | none | No network |

## F. MMB behavior facts from the 2026-10-10 and 2026-10-11 runs

Facts the real app showed that matter for the UI, defaults and the importer. Status says where
we stand against each.

| Feature | Inv | Status | Where | Evidence | What's left |
|---|---|---|---|---|---|
| New budgets post fringes by Budget | v1 | Verified | D59, `starter::new_budget_settings` | C2, ZTEST06 | none (a file without the field still reads by category) |
| New budgets: cutoff factor 1.0, copy Fringe from line above on, Lock Original Totals off | v1 | Verified | 1.4, D29, D59 | C2 | none |
| Decimal places in totals defaults to 0 (whole units) | v1 | Differs by choice | 2.18, D59 | C2 | Owner: keep cents |
| Unit hours must be whole numbers; 0 shows blank and means no hours | v1 | Built | FORMAT unit `hours` | ZTEST05 | Ours accepts decimals (superset, no decision); 0 behaves the same (Verified F-24) |
| Set codes are 1 to 3 characters | v1 | Built | FORMAT set | ZTEST07 | Ours has no limit (superset, no decision) |
| A fringe rate can't be a global; rate 0 refused | v1 | Built | FORMAT fringe `rate` | ZTEST04 | Ours allows both (superset, no decision) |
| A bad Amt or a circular global is refused at entry | v1 | Partial | D5, D59 | V02-4 | Divide by zero and circular refused; other bad formulas kept and flagged |
| Circular globals show #ERR with value 0 | v1 | Verified | D59, Globals table | V02-4 | none |
| A global's unit fills the line's Unit | v1 | Verified | D59 | G-08 | none |
| Tab-separated paste into the grid fails in MMB | v1 | Built | 1.8 (ours works) | C3/C4 | none |
| Category and account charge exclusions live in the tools pane Charges tab | v1 | Built | 2.16 | ZTEST09 | none |
| Charges list in creation order, newest first | v1 | Verified | D59 | V09-2 | none |
| Credits list in reverse application order with Sub Total and Net total | later | Verified | D59 | ZTEST10 | none |
| First Insert Production Total adds the summary rows | v1 | Built | 2.18 | ZTEST06, ZTEST11 | Compare ours |
| Budget fringe row can be moved and reset | v1 | Built | D59, Settings > Fringe totals (place: after the summary rows resets it) | ZTEST11 (menus) | PM-POS not tested |
| Production-level posting drop is not a setting | v1 | Differs by choice | D56 | V06-2 follow-up | Ours keeps by default |
| Lock Original Totals only freezes the column; originals start at 0 | v1 | Verified | D27 | ZTEST12 | Freeze switch is Partial (section 14) |
| Versions and sub-budgets inherit originals | v1 | Verified | D17, 2.22 | ZTEST12 S7, S1 | Sub-budget side: check ours |
| Compare needs cloud budgets | later | Differs by choice | 1.14 | ZTEST12 | Ours is local; cannot be parity-checked |
| JSON export omits suppressed lines and per-line fringe dollars; charges and credits appear as categories with a blank number | v1 | Built | importer input (1.16) | ZTEST04, ZTEST09 JSON | Importer must not expect them |
| JSON export rounds a half-cent charge differently from the screen | v1 | Verified | 1.16 note | ZHALF 100.01 on screen, 100 in JSON | Importer reconciles against screen rules, not JSON charge totals |
| XML Advanced carries evaluated Amt and the names of globals used, not formulas; no fringe cutoffs or global table | v1 | Verified | 1.16 note | 2026-10-11 ZTEST02, ZTEST04 XML | Import must rebuild globals and cutoffs from another source or ask |
| Exports carry Agg and Agg Sum per line, originals and variances per category and account | v1 | Verified | 1.16 note | 2026-10-10 JSON, 2026-10-11 XML | Use for reconciliation |

---

## What's left, by priority

### 1. v1 items that are Missing or Partial

1. **Import MMB XML Advanced, JSON and Excel** (21; roadmap 1.16). Samples are on disk now.
   Exports lack global formulas, fringe cutoffs and suppressed lines, so the importer needs a
   way to bring setup in (ask, or a setup sheet) and must reconcile against the export's totals.
2. **Done in D59** (2026-10-11): cutoff factor, totals in another currency, new-budget
   defaults (totals keep cents by the owner's call), charge and credit order on the
   topsheet, the budget fringe row's number, description and place, a global's unit
   filling the line's Unit, and refusing divide by zero and circular globals at entry.
   Left from them: the new-budget account separator (ours `-`, MMB's exports show none),
   charge position relative to the summary rows (ROADMAP decision 13), PM-POS, and refusing
   other bad formulas at entry (kept and flagged for now).
3. **Currency separator pattern and symbol after the value** (8).
4. **Lock Original Totals as a freeze switch** (14).
5. **Agg Sum column** (2).
6. **Underline** text style (1), hiding subtotal rows in reports (1), topsheet report exclusions
   (20).
7. **New-line defaults**: Amt starting at 1 and one ready line, compare with MMB (2).

### 2. Built but not checked in MMB (next parity run candidates)

1. Flat fringe inside a fringe range (F-33..F-35).
2. Category fringe account numbering for 3-digit and alphanumeric categories (PM-CAT).
3. Budget fringe row position (PM-POS): ours keeps ATL and BTL unchanged wherever it shows.
4. Base currency change (V08-3) and deleting a currency.
5. Fringes in group totals rule set to Exclude.
6. Agg (fringe %) on a suppressed line (EC 15).
7. Next account at the end of a category (EC 16).
8. Account-type credit rows: do they exist in MMB.
9. Credit caps in base or display currency (inv Q8).
10. Sub-budget S2 and S3 steps, and whether ours carries originals into a sub-budget.
11. Flat fee smaller than its exclusions (negative fee).
12. Flat fringe on an adjustment row.
13. Separator rows (V06-5) and ZTEST13 (unicode, 200 lines: needs a non-paste path).
14. Summary rows appearing on the first production total, compared with ours.
15. Keyboard map (Recording wishlist 1), reports in 2026 (wishlist 2).

### 3. Later

Credits print view and charges/credits report sections, cover page, report templates as files,
color priority, numeric shortcut hotkeys, fraction display, setup folders in a budget's own
tables, sub-budget fringe lock, MMB-shaped exports, tab-delimited and XML Basic import, rate
book (3.9), schedule-linked quantities (3.8), incentive modeling (3.6), commercial bid layouts
(3.13), person-level cutoffs across accounts (3.11).
