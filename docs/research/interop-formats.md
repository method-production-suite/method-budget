# Import and export: interoperable formats (clean-room research)

Status: research notes, October 2026. Written for Method Budget's import and export work
(roadmap 1.16, 1.17, 2.2, 3.7, 3.8, 3.13).

Clean-room note (see `CLEAN-ROOM.md`): everything here is a functional description in our
own words, drawn from public documentation, public help centers and public format
specifications. No vendor help text, templates, form layouts, rate tables or sample files were
copied. Nothing here describes the inside of a proprietary native file (`.mbd`, `.mmbx`,
`.mbt`, `.mbl`, `.mms`, `.mmsx`, `.mmbj` or similar), and that topic was not researched.
Where a vendor's open export has no public schema, we say so instead of guessing. Method
Budget is not affiliated with Entertainment Partners, Movie Magic, Cast & Crew or any other
vendor named here; names are used only to describe interoperability.

Contents:

1. Where we are today
2. The format matrix
3. Movie Magic Budgeting's open options in detail
4. The wider ecosystem: budgeting tools
5. The wider ecosystem: accounting, cost reports and payroll
6. The wider ecosystem: scheduling
7. Commercial bid forms and the legal line
8. Spreadsheet realities
9. Gaps
10. Proposal: what to build, in three waves
11. Open questions for the owner

---

## 1. Where we are today

What Method Budget already reads and writes (decisions 30, 36, 41, 45; `docs/CLI.md`):

| Direction | What | Notes |
|---|---|---|
| Native | `.mthd` (JSON) | Open and documented (`docs/FORMAT.md`), deterministic bytes, diff and merge friendly. Already a full-fidelity interchange format for anyone who reads JSON. |
| Export | Excel workbook (.xlsx) | Sheets: Topsheet, Accounts, Detail, Globals, Fringes, Groups, Units, Currencies, Sets, Locations; plus Cost Report and Actuals when the budget has actuals. Money exact from cents with a number format; text never a formula. |
| Export | CSV | One RFC 4180 file per table, UTF-8 with a byte order mark (the app) or without (`method export` unless `--bom`). Formula-like text neutralized. |
| Export | Tab-delimited | The Detail table only, never quoted, no byte order mark, for field-by-field importers. |
| Export | Reports | Typeset PDF from the app; `method report` writes HTML, plain text, CSV or a plain PDF from a saved preset. |
| Import | Spreadsheet lines | .xlsx, CSV and tab-delimited (our own ZIP, DEFLATE and XML readers, hostile-file limits). Sheet and header row found automatically, one column per field guessed from its title, saved mappings, heading rows that name an account, reconciliation against the sheet's line totals. `method import` with a JSON mapping file. |
| Import | Actuals | CSV, tab-delimited, .xlsx accounting exports: date, account, amount or debit and credit, vendor, description, reference, tags. Dates of most shapes, duplicate review, unmatched account review, total-row skipping, one removable batch. |
| Import | Setup and templates | Import setup from another `.mthd`, new from template, the personal library (all our own format). |

What it does not read or write yet (the gaps in section 9 expand on these): OpenDocument
(.ods), legacy Excel (.xls), Apple Numbers (.numbers), account-level (topsheet-only) budget
imports, multi-tab budgets with one tab per account, formula cells without cached values,
workbooks in the 1904 date system, XML or JSON from other tools, the reference product's
open export layouts, budget info and charges/credits in the export, an account-level
"accounting" export, a chart-of-accounts export, monthly spreads, schedule data of any kind.

This section describes the committed code as of commit `aae3157`. Work in progress in the
tree at the time of writing (a chart-of-accounts import and the 1904 date system in the
workbook reader) may already close parts of gaps 2 and 10 in section 9 and of W1.2 and
W1.5 (e) in section 10.

---

## 2. The format matrix

One row per format family. "Ours" is today's support (section 1); "W" numbers point at the
proposal in section 10. Sections 3 to 8 have the sources.

| Format | Produced by | Consumed by | Open? | Levels and fields carried | Fidelity notes | Ours today |
|---|---|---|---|---|---|---|
| `.mthd` (JSON) | Method Budget | Method Budget; any JSON reader | Open, documented | Everything, as inputs (formulas, setup, tags, notes, versions) | Full; a reader must recompute totals with our rules | Read and write |
| Interchange JSON (proposed) | Method Budget | Integrators, accounting platforms | Open, documented (planned) | Everything plus every computed total | Full, no engine needed to read | W1.4 |
| Hand-made budget workbook (.xlsx) | Excel, Google Sheets, Numbers, LibreOffice; free templates | Everyone | Open container, any layout | Anything from a topsheet to detail lines; tags rarely | Layout varies (section 8.1); values only, formulas lost | Read flat and indented detail; W1.5 for the rest |
| CSV and tab-delimited | Every tool | Every tool | Open (RFC 4180 for CSV; dialects vary) | One table per file | Delimiter, decimal and encoding guesswork (8.4) | Read and write |
| OpenDocument (.ods) | LibreOffice, Google Sheets | LibreOffice, Xero budget import | Open standard (OASIS, ISO) | As .xlsx | Same as .xlsx | No; W1.6 |
| Legacy Excel (.xls) | Excel 97 to 2003 and tools that still write it | Excel, LibreOffice | Published spec under Microsoft's Open Specification Promise | As .xlsx | Same as .xlsx | No; W3.6 |
| Apple Numbers (.numbers) | Numbers | Numbers | Proprietary, no public spec | | Export from Numbers instead | No; skip |
| Reference Excel export | Movie Magic Budgeting 10 | Wrapbook (cost tracking), users | Open container, undocumented layout | All three levels, budget info, line tags | Totals may be display-rounded (3.2) | Partly, via mapped import; W2.3 |
| Reference Tab Delimited | Movie Magic Budgeting 10 | PSL+, the vendor's accounting product, CPAS (as CSV) | Open, undocumented columns | Probably account or detail rows | Its consumer warns lines can drop | Partly, via mapped import; W2.4 |
| Reference XML (Basic) | Movie Magic Budgeting 10 | Wrapbook, Saturation, Dramatify, Shamel read "the XML" without saying which of the two | Open syntax, schema not public, kept stable | "Some" detail data, fringe detail | Built for downstream systems | No; W2.1, W2.4 |
| Reference XML (Advanced) and JSON | Movie Magic Budgeting 10 | GreenSlate (JSON); possibly the XML readers in the row above | Open syntax, schema not public | All detail data, line tags, globals used, budget info | Richest open path; setup tables unconfirmed | No; W2.1, W2.2 |
| Reference native and side files (`.mmbx`, `.mbd`, `.mbt`, `.mbl`, `.mmbj`, cloud `.zip`) | Movie Magic products; one third party writes `.mmbx` | Movie Magic products; Filmustage | Proprietary | | | Never (`CLEAN-ROOM.md`) |
| PDF budget or report | Every tool | People | Open container, unstructured | What was printed | Last resort (8.6) | No; skip |
| Other budgeting tools' exports (Showbiz CSV, Saturation CSV, Excel and JSON, Hot Budget JSON, Filmustage and Gorilla Excel, Dramatify and Celtx CSV) | Those tools | Each other, accounting platforms | Open syntax, schemas not public | Varies (section 4) | Each needs a mapping or preset | CSV and Excel via mapped import; JSON via W2.1 |
| Hot Budget workbook (.xlsb) | Hot Budget | Saturation | Binary Excel workbook with a vendor layout | Bid and actual by section and line | Needs a binary workbook reader and a layout | No; skip unless asked |
| Account-level budget file (code, description, amount, production codes) | Budgeting tools | PSL+, GreenSlate, SmartAccounting, Wrapbook, general accounting | Open (CSV, tab, .xlsx) | Account rows; series, location, set, episode and free-field codes | What accounting actually keeps | No (our tab export is detail level); W1.1 |
| Chart of accounts file | Accounting systems, budgeting tools | Accounting systems (QuickBooks, Xero, Intacct, Wrapbook, DPO) | Open (CSV, .xlsx, IIF) | Code, name, type, parent | Column names differ per product | No; W1.2 |
| General-ledger and transaction reports | QuickBooks, Xero, Sage, production accounting | Our actuals import | Open (CSV, .xlsx) | Date, account, amount or debit and credit, vendor, memo | Header blocks, combined account fields, sign conventions | Read (generic); presets W1.7 |
| QuickBooks IIF | QuickBooks Desktop | QuickBooks Desktop | Open, vendor-documented text | Lists, budgets, journal entries | Exact name matching | No; later if asked |
| OFX, QFX, QBO | Banks, card issuers | Accounting software | OFX open; QFX and QBO are Intuit variants of it | Bank transactions, no coding | Needs payee rules | No; W3.3 |
| Cost report export | Production accounting platforms; us | Studios, guarantors, us | Open (.xlsx, CSV, PDF) | Account rows with budget, actuals, POs, ETC, EFC, variance | Same vocabulary everywhere (5.3) | Write (Cost Report sheet); W3.2 extends; reading one back is W1.7 |
| Payroll log CSV (labor by line) | Payroll platforms | Commercial budgeting tools, Saturation | Open | Line, payee, days, rate, overtime hours, extras | Fringes often separate | Via actuals import (as amounts); W1.7 |
| DOOD and element reports | Scheduling tools | Budgeting tools, people | Excel or CSV open; PDF; native files proprietary | Elements with work, hold, travel and total days; shoot day counts | Counting rules differ by tool (6.1) | No; W3.1 |
| `.fdx`, Fountain | Screenwriting tools | Scheduling tools | `.fdx` readable XML; Fountain open | Scenes, characters (tags in some `.fdx`) | Not budget data | No; not needed |
| Commercial bid form | Trade association (PDF, Excel); bidding tools | Agencies, producers | Free download; license for software unclear | Lettered sections, numbered lines, estimate and actual | Legal check first (section 7) | No; W3.5 |

---

## 3. Movie Magic Budgeting's open options in detail

Movie Magic Budgeting 10 (MMB) is the reference product. Inventory section 21 already lists
its export menu; this section adds what the public manual, the release notes and third-party
importer documentation say about each option, and sorts each into open or proprietary.

### 3.1 Open versus proprietary at a glance

| Option | Direction | File | Open? | Usable by us? |
|---|---|---|---|---|
| Excel | export | `.xlsx` | Open container (Office Open XML); layout not documented | Yes, from files a licensed user exports |
| Tab Delimited | export | `.txt` | Open (plain text); columns not documented | Yes, same |
| XML (Basic) | export | `.xml` | Open syntax; schema not public | Yes, same |
| XML (Advanced) | export | `.xml` | Open syntax; schema not public | Yes, same |
| JSON | export | `.json` | Open syntax; schema not public | Yes, same |
| PDF reports | export | `.pdf` | Open container; meant for people | Last resort only |
| Paste cells to and from a spreadsheet | both | clipboard text | Open (tab-separated text) | Already compatible: our grid copies and pastes tab-separated text |
| Report templates | both | `.mmbj` | Vendor-specific file, undocumented | No (treat as proprietary) |
| Templates | both | `.mmbx`, `.mbd` | Proprietary native | No |
| MMB7 budget and template | both | `.mbd`, `.mbt` | Proprietary native | No |
| Scheduling element libraries | import | `.mbl` (from the scheduling product) | Proprietary | No; see section 6 for open schedule data |
| Bulk cloud export | export | `.zip` of native files | Proprietary contents | No |

Sources: https://mmb-docs.ep.com/Projects_and_Budgets/importexport.html,
https://mmb-docs.ep.com/release_notes.html, https://mmb-docs.ep.com/Projects_and_Budgets/templates.html,
https://mmb-docs.ep.com/Reports_Printing/Reports_Overview.html,
https://mmb-docs.ep.com/Setup/mms6_import.html, https://downloads.adc.ep.com/doc/Get_Most_Out_Of_MMB10.pdf

### 3.2 Excel export

- **What the vendor says it holds.** All three levels of the budget (topsheet, accounts,
  detail) and the budget's metadata in one standalone workbook, including the Budget Info
  fields, and for each detail line the fringes, groups, globals, sets and locations attached
  to it. Added in mid 2023.
- **Numbers follow display settings.** Totals show whole currency units unless the user
  turns on decimal display for totals (a system preference added in 2026, off by default),
  and the Excel export follows that setting. A late 2026 fix made fractional values follow
  the "as entered" display setting. So a default export may carry display-rounded totals,
  not the exact figures. Anyone producing a sample for us should turn decimal totals on.
- **What is left out.** The hidden-column marker never appears in Excel exports. Notes are
  not said to export.
- **Unknown without a sample:** whether it is one sheet or several, whether detail rows
  carry the account number on every row or only in heading rows, whether cells are values
  or formulas, whether a line's fringe dollars are present or only fringe names, how
  subtotal and adjustment rows, production totals, charges and credits appear, and where
  Budget Info lands.
- **Fidelity for us:** good for structure, line values and tag names; fringe rates, global
  formulas and charge or credit rules may not be recoverable unless the workbook lists the
  setup tables.

Sources: https://mmb-docs.ep.com/release_notes.html (10.24.15, 10.34.9, 10.35.7),
https://mmb-docs.ep.com/System_Preferences/Sys_Prefs_Overview.html,
https://mmb-docs.ep.com/Budget_Navigation/columns.html,
https://downloads.adc.ep.com/doc/Get_Most_Out_Of_MMB10.pdf

### 3.3 Tab Delimited export

- Added in early 2020 so users could take budget data into a spreadsheet. Plain
  tab-separated text.
- Columns are not documented publicly. Older community posts describe the much earlier
  desktop versions as offering only a simple tab export, and users worked around it by
  copying an account's detail rows and pasting them into a spreadsheet.
- One production-accounting vendor tells users to export a delimited file from MMB, open it
  in Excel and save it as CSV before importing (the article names a "Comma Delimited"
  export, which suggests it was written for the older desktop version or that the importer
  simply needs CSV).
- **Fidelity for us:** probably a flat list (account or detail level) with account numbers,
  descriptions and amounts; our generic importer can likely read it today with a mapping.
  A preset needs a sample.

Sources: https://mmb-docs.ep.com/release_notes.html (10.1),
https://www.reddit.com/r/FilmTVBudgeting/comments/gcg71c/exporting_movie_magic_to_excel/ (search excerpt only),
https://support.castandcrew.com/hc/en-us/articles/30846620132500-CPAS-How-can-I-import-my-Movie-Magic-Budgeting-file-into-CPAS

### 3.4 XML (Basic)

- Introduced in 2020 as "Detailed XML", renamed XML (Basic) in 2023 when the advanced
  export arrived. Early 2021 added more fringe detail.
- The vendor says it carries "some" detail-level information and that its contents are
  kept stable so downstream systems can depend on it. That stability promise is why
  payroll, accounting and production-management platforms read it (section 5).
- Schema not public.
- **Fidelity for us:** likely account numbers, descriptions, line values and fringe totals;
  probably not globals, groups or the setup needed to recalculate. Good for reconciliation
  and account-level budgets, weaker for a living budget.

Sources: https://mmb-docs.ep.com/Projects_and_Budgets/importexport.html,
https://mmb-docs.ep.com/release_notes.html (10.3, 10.9, 10.24.15)

### 3.5 XML (Advanced) and JSON

- Added in 2023 (XML) and late 2023 (JSON). The JSON carries the same data as XML
  (Advanced); the vendor added it "for easier parsing".
- The vendor says both carry all detail-level information, including the groups, fringes,
  sets and locations on each line, which globals each line uses, and the Budget Info fields.
- Schema not public. Whether setup tables (fringe rates and cutoffs, global formulas, unit
  hours, currency rates, group in-total switches, charges and credits) are included is not
  stated; the phrase "which globals have been used" suggests references, not necessarily
  definitions.
- **Fidelity for us:** the richest open path in. If the setup tables are included, a
  faithful rebuild with recalculation and reconciliation is possible; if not, lines come in
  with their tag names and the sheet's own totals become the reconciliation target, and the
  user fills fringe rates.
- At least one accounting platform imports the JSON directly, reducing it to account rows
  (account code, description, budget amount) for its cost report (section 5).

Sources: https://mmb-docs.ep.com/Projects_and_Budgets/importexport.html,
https://mmb-docs.ep.com/release_notes.html (10.24.15, 10.25.20),
https://helpcenter.greenslate.com/en/articles/13371759-how-to-import-a-budget-from-movie-magic-budgeting

### 3.6 PDF

- Reports export to PDF from the report setup screen: cover page, topsheet, category
  summary, account details, chart of accounts, group totals, budget info, notes and setup
  tables. The comparison report also exports to PDF.
- PDFs are made for people. Parsing them means recovering columns from text positions; it is
  slow, fragile and loses tags and setup. Use only when nothing else exists (section 8.6).

Sources: https://mmb-docs.ep.com/Reports_Printing/Reports_Overview.html,
https://mmb-docs.ep.com/release_notes.html (10.5)

### 3.7 Paste from a spreadsheet

- Added in 2020. Copying out of MMB gives the visible values as plain cells; much of a row's
  underlying data is lost.
- Pasting in expects cells that line up with the columns currently visible in MMB, starting
  at the selected cell. Values should be plain (no spreadsheet formulas); any globals or
  other tools referenced must already exist. Sets and locations that don't exist are created
  automatically. Mismatched columns give invalid data.
- Our grid already copies and pastes tab-separated text (roadmap 1.8). Nothing to build for
  parity; our mapped import is the stronger answer.

Sources: https://mmb-docs.ep.com/Budget_Navigation/CopyPaste.html,
https://mmb-docs.ep.com/release_notes.html (10.6)

### 3.8 Scheduling element import

- MMB imports a library file written by the vendor's scheduling product (`.mbl`, from
  scheduling version 6, and per the vendor's guide version 10 too). Each schedule category
  becomes a detail-line shortcut holding one line per element. It is a static copy: no link
  back to the schedule.
- The `.mbl` file is a proprietary format, so this path is closed to us. The open
  equivalent is a schedule's own CSV or Excel exports (element lists, day out of days), in
  section 6.

Sources: https://mmb-docs.ep.com/Setup/mms6_import.html,
https://mmb-docs.ep.com/release_notes.html (10.8),
https://downloads.adc.ep.com/doc/Get_Most_Out_Of_MMB10.pdf

### 3.9 Templates and report templates

- Templates (a budget's chart of accounts and setup without detail lines) import and export
  only as native budget files. Closed to us; our answer is New From Template and Import
  Setup from any `.mthd` (built), plus a chart-of-accounts import from CSV (proposed).
- Report templates export and import as `.mmbj` files (late 2025). The format is not
  documented. We treat it as vendor-specific and do not read it; our report presets live in
  the `.mthd`.

Sources: https://mmb-docs.ep.com/Projects_and_Budgets/templates.html,
https://mmb-docs.ep.com/Reports_Printing/Reports_Overview.html,
https://mmb-docs.ep.com/release_notes.html (10.31.10, 10.34.9)

### 3.10 Budget Info, episodes and sub-budgets in exports

- Budget Info holds title, production, season, episode number, total episodes, status,
  base country, region and city, location and stage shoot days, start and end dates,
  production year, and user fields typed as text, number or date. The vendor says it appears
  in the Excel, XML (Advanced) and JSON exports and in report headers.
- The manual describes no separate multi-episode export. A series is one budget per episode
  (with the episode fields in Budget Info), or one budget where some other tag (groups, for
  example) marks what belongs to which episode. Sub-budgets are ordinary filtered budgets
  and export like any other.
- Downstream accounting tools ask for the episode (or location) code at import time and
  stamp it on every row, which tells us they expect one export per episode (section 5).

Sources: https://mmb-docs.ep.com/Setup/budget_info.html,
https://mmb-docs.ep.com/Sub_Budgets/Sub_Budget_Overview.html,
https://helpcenter.greenslate.com/en/articles/13371759-how-to-import-a-budget-from-movie-magic-budgeting

### 3.11 Who imports which MMB export

Each downstream product reads a different one of the open exports. This is the clearest
public evidence of what each export is good for.

| Product | Reads | What it keeps or asks for |
|---|---|---|
| The vendor's own accounting product | Tab Delimited, or the MMB7 native file; newer versions link to the budget through the vendor's cloud instead | The manual's accounting page names only these two exports for accounting programs. |
| PSL+ (Cast & Crew production accounting) | Tab Delimited `.txt` (its manual also mentions a dedicated export flavor for it in the older product) | Asks for a short budget name (six characters), a description, default series, location and set codes, whether to add missing accounts to the chart, and whether to combine with, replace or keep an existing budget; warns that the conversion may drop lines, so line amounts should be checked. |
| CPAS (Cast & Crew) | A delimited export re-saved from Excel as CSV | No field list published. |
| GreenSlate (accounting and cost report) | JSON, up to 50 MB | Shows account code, account description, budget amount and an approved-overage column; asks for an effective date, a currency and one episode or location code applied to every row. Account level only. |
| Wrapbook, production accounting mode | XML | Budget, estimate final cost (EFC) or both; a default location, production codes and a cost-code override choice; re-import supported. |
| Wrapbook, cost tracking mode | Excel (.xlsx) from this and three other budgeting tools | Columns not published. |
| Saturation (budgeting and actuals) | XML (its help article); other pages also mention the native file or CSV | Maps the data onto a chart of accounts automatically; no export back. |
| Dramatify (production management) | XML (does not say which XML) | Cannot read native files. |
| Shamel Studio (budgeting) | XML | Brings in topsheet categories, accounts and details; lists import limitations in the dialog. |
| Filmustage (script breakdown and budgeting) | The native `.mmbx` only | Out of our scope. It also writes `.mmbx`, the native format, not an open one. |
| Showbiz Budgeting, Gorilla Budgeting | Advertise importing MMB budgets; the file type is shown only in videos | Unknown. |

Takeaways:

1. **Accounting wants accounts, not lines.** Every accounting importer that documents its
   fields keeps account code, description and amount, plus episode, location, set or
   production codes it asks for at import. That is a small, generic file we can write today
   (W1.1).
2. **XML is the common language for detail.** Budgeting, production-management and one
   accounting tool read the XML; the newest accounting importer reads JSON. Nobody
   publishes the schema they read, so each importer was built from sample files.
3. **Tab Delimited is the legacy accounting path**, and its own consumer warns that lines can
   go missing, consistent with a lossy flat file.

Sources: https://mmb-docs.ep.com/Projects_and_Budgets/accounting_integrations.html,
https://support.castandcrew.com/hc/en-us/articles/51949802916116-PSL-Importing-a-Budget-from-Movie-Magic,
https://info.castandcrew.com/hubfs/KB/QSG/PSL%2B%20Quick%20Start%20Guides/PSL-manual-2026.pdf,
https://support.castandcrew.com/hc/en-us/articles/30846620132500-CPAS-How-can-I-import-my-Movie-Magic-Budgeting-file-into-CPAS,
https://helpcenter.greenslate.com/en/articles/13371759-how-to-import-a-budget-from-movie-magic-budgeting,
https://help.wrapbook.com/docs/import-a-project-budget-from-movie-magic-pas,
https://help.wrapbook.com/docs/import-a-project-budget,
https://support.saturation.io/en/articles/12464948-importing-a-movie-magic-budget-into-saturation-saturation-2-0,
https://saturation.io/integrations, https://dramatify.com/how-to/import-a-movie-magic-budget,
https://www.shamelstudio.com/help/movie-magic-budget-import,
https://help.filmustage.com/en/articles/15396789-import-a-budget-from-movie-magic,
https://help.filmustage.com/en/articles/10249502-export-the-budget,
https://junglesoftware.com/film-budgeting-software/,
https://platform.softwareone.com/product/showbiz-budgeting/PCP-0606-8679

### 3.12 What we can do without sample files, and what needs them

- **Without samples:** a generic structured-file importer (pick the repeating element of
  any XML or JSON file and map its fields, like our spreadsheet mapping); our existing
  spreadsheet and tab-delimited importer, which already reads heading rows that name an
  account; an account-level budget import; reconciliation against any total column.
- **With samples (optional, the owner's licensed copy, `docs/research/export-test-plan.md`):**
  presets that recognize each MMB layout automatically, setup-table recovery (fringe rates,
  cutoffs, global formulas), and answers to the engine questions in inventory Q1 to Q15.
  Nothing in sections 10.1 and 10.3 depends on them.

---

## 4. The wider ecosystem: budgeting tools

How other budgeting tools move budgets in and out, from their public help pages. No tool
publishes the schema of its own export, and none reads another tool's file without a
dedicated converter built from samples.

| Tool | Imports | Exports | Open or proprietary | Notes |
|---|---|---|---|---|
| **Showbiz Budgeting** (Media Services) | Movie Magic budgets (file type not documented), actuals from two other bidding and budgeting tools, payroll from its owner's payroll products and timecards, a schedule file from Yamdu | A CSV ("Excel (csv)") per phase, with optional actuals (invoices, petty cash, card) | CSV open; native file proprietary | Its CSV leaves out overtime calculations and multipliers (per a competitor's import guide). Ships commercial templates it calls compliant with the trade association's bid form. |
| **Hot Budget** (commercials) | Other Hot Budget documents: budget, PO, payroll and petty cash logs, chart of accounts; payroll logs from a payroll platform's CSV | JSON (version 3, from a menu on its cost summary sheet; bid and actual amounts plus PO and petty cash logs; leaves out topsheet company details; stops working if the workbook is re-saved as plain .xlsx) | The workbook is a macro-driven Excel binary workbook (.xlsb) with a vendor layout; the JSON is open syntax, schema not public | Lines follow the trade association's bid sections. An accounting system imports the JSON twice, once as budget and once as actual. |
| **Gorilla Budgeting** (Jungle Software) | Movie Magic budgets (type not documented); a fourth level built by department heads in Excel and brought in under an account with an "import map"; cast, crew, locations and elements with day counts from its sister scheduling app | Excel, tab-separated text, FileMaker and others | Exports open; import map spec not public | The schedule link (day out of days totals into line quantities) is the competitor feature roadmap 3.8 answers. |
| **Saturation** (cloud) | The reference's XML (help article; other pages say the native file or CSV), Showbiz CSV, Hot Budget's .xlsb read directly (each cost summary section to an account), scheduling files and day out of days PDFs, payroll CSVs through a column mapper | CSV (current view, phase totals, a grand total row), Excel (a topsheet plus one sheet per account with indented child rows, fringes, globals, tags), JSON (phases, lines, globals, units, fringes, currencies, tags, ledger mappings, incentives, credits, report design), PDF | Exports open; schemas not public; also a cloud API | Re-importing a payroll file duplicates transactions (their docs). Their JSON is the closest public analogue to the interchange export we propose (W1.4). |
| **Filmustage** | The reference's native `.mmbx` only | PDF, .xlsx (a topsheet tab, fringes and deductions tabs, one tab per category), and `.mmbx` | Its `.mmbx` export writes a proprietary native format | A third party writing the native file; not a path for us. |
| **Dramatify** | The reference's XML | CSV with a choice of contents | Open | |
| **Shamel Studio** | The reference's XML | not documented | | Asks for shoot days, state, city and dates after import. |
| **Celtx** | (catalog CSV) | Budget as CSV; catalog CSV | Open, fields not public | Categories with subcategories. |
| **Yamdu** | Schedule data; budgeting in beta | A "download for Showbiz" file from a shooting schedule (format not public) | Unknown | Uses the MovieLabs ontology (below) for some data exchange. |
| **StudioBinder** | none documented | none documented; offers a free Google Sheets budget template | | |

Other observations:

- **Excel layouts converge on "topsheet plus a sheet per account or category"** (Saturation,
  Filmustage), against our "Topsheet, Accounts, Detail" flat tables. Both are reasonable; a
  per-account layout is what people print and email, ours is what machines read. An
  optional "one sheet per category" layout (W1.3) would match what recipients expect.
- **Bid and actual side by side** is the commercials norm (Hot Budget, Showbiz); one
  accounting system takes the bid and the actual as two separate uploads of the same file.
- **No open interchange standard for budgets exists.** The MovieLabs Ontology for Media
  Creation (JSON and RDF schemas, Creative Commons Attribution license) covers
  participants, narrative and production elements and is used by at least one
  pre-production tool for exchange; we found nothing in it for budgets or costs. It may
  matter later for schedule and breakdown data.

Sources: https://platform.softwareone.com/product/showbiz-budgeting/PCP-0606-8679,
https://www.mediaservices.com/blog/showbiz-budgeting-10-ten-features/,
https://support.saturation.io/en/articles/12464849-how-to-import-showbiz-budget-into-saturation,
https://support.yamdu.com/en/articles/58246-budgeting-with-yamdu,
https://support.castandcrew.com/hc/en-us/articles/51644231851540-CAPS-How-to-Import-a-Hot-Budget-V3-JSON-Export-to-a-CAPS-Project,
https://downloads.hotbudget.com/HotBudget_UserGuide-2.0.pdf,
https://help.wrapbook.com/docs/export-a-payroll-log-to-hot-budget,
https://junglesoftware.com/film-budgeting-software/,
https://junglesoftware.com/faqs/comparing-movie-magic-budgeting-to-gorilla/,
https://support.saturation.io/en/articles/12464948-importing-a-movie-magic-budget-into-saturation-saturation-2-0,
https://saturation.io/integrations, https://saturation.io/integrations/hot-budget,
https://saturation.io/integrations/movie-magic-scheduling,
https://support.saturation.io/en/articles/16497887-export-a-budget-saturation-3-0,
https://support.saturation.io/en/articles/16443024-import-payroll-files-saturation-3-0,
https://docs.saturation.io/,
https://help.filmustage.com/en/articles/15396789-import-a-budget-from-movie-magic,
https://help.filmustage.com/en/articles/10249502-export-the-budget,
https://dramatify.com/how-to/import-a-movie-magic-budget,
https://dramatify.com/how-to/export-a-budget-to-csv-excel-or-google-sheet-format,
https://www.shamelstudio.com/help/movie-magic-budget-import,
https://support.celtx.com/hc/en-us/articles/217387757-Budgeting-Overview,
https://support.celtx.com/hc/en-us/articles/232272968-The-Catalog,
https://support.studiobinder.com/en/articles/1979982-how-to-use-our-film-budget-template,
https://movielabs.com/ontology-for-media-creation/, https://mc.movielabs.com/docs/ontology/

---

## 5. The wider ecosystem: accounting, cost reports and payroll

### 5.1 Production accounting platforms

What the platforms that run a production's books document about taking a budget in and
handing cost data out. Most production-accounting help is behind a login; these are the
public parts.

| Platform | Budget in | Other imports | Exports | Notes |
|---|---|---|---|---|
| **PSL+** (Cast & Crew) | The reference's Tab Delimited `.txt`; older guides mention a dedicated export flavor and the vendor's older budgeting product | A generic CSV or tab-delimited transaction import for journal entries, POs, invoices, petty cash and variance entries, columns in a fixed order: account (up to 16 characters, must match the chart's spelling exactly, so a hyphenated and an unhyphenated code differ), series, location and set codes (4 characters each), a tax-form code and an insurance code (2 each), four free fields (4 each), description (30 characters), amount | Cost reports to Excel or PDF; an episodic estimate-final-cost report | Configurable account mask (four characters by default, patterns like two-hyphen-two allowed); header accounts can't be posted to. Several budgets per show, copy and merge. Payroll edit files load as unposted entries. |
| **CPAS** (Cast & Crew) | A delimited budget re-saved as CSV | | | |
| **Hours+** (Cast & Crew) | | Timecards | A hot cost report (actual hours against budgeted hours, day by day) to Excel | |
| **DPO** (Sargent-Disc, Cast & Crew) | | A chart-of-accounts sheet: account code and name required, parent optional (sent to support to load) | | |
| **SmartAccounting** (Entertainment Partners) | Linked to the reference product's cloud budget, or the reference's Tab Delimited or MMB7 file | Payroll invoices and approved purchase orders flow in from the vendor's own products | A cost report worksheet of about 16 columns: account, description, actuals this period and to date, POs, total cost, estimate to complete (ETC), estimate final cost (EFC), budget, approved overages, adjusted budget, variance, change in variance, percent remaining; only ETC, EFC, budget and overages editable, ETC and EFC linked | Episode and location codes apply to every account and turn each account into a total of its codes; sets are switched on per account. Budget names under 34 characters. Import file spec not public. |
| **Vista** (Entertainment Partners) | A third party says the reference product can export to it | | | No public spec. |
| **GreenSlate** | The reference's JSON, up to 50 MB, reduced to account code, description, amount and approved overage; one episode or location code, a currency and an effective date per upload | | Cost report; daily ledger and payroll feeds to one streamer, which requires its own chart of accounts or a mapping to it | ETC and EFC linked; EFC set automatically on the first upload only; revisions dated, not in closed periods. |
| **Wrapbook** (payroll and accounting) | Production accounting mode: the reference's XML (budget, EFC or both; default location and production codes; re-import). Cost tracking mode: .xlsx from four budgeting tools | Chart of accounts from CSV, tab-delimited, .xlsx, XML or PDF (it guesses the mask, ranges, types, separator and parent and child structure, then shows a draft); card transactions as journal entries from CSV (header: number, description, date, period, type, vendor, currency; lines: account code, description, amount, asset, tax-form fields, location, production, set, four free fields) | Budget as PDF only; chart of accounts as CSV or PDF; cost reports as PDF, .xlsx or CSV with renamable, reorderable columns (account, description, period actuals, actuals to date, POs, total to date, ETC, EFC, budget, approved overages, total budget, variance, period variance, percent complete), filtered by location, production code, set and a free field; a payroll-log CSV for a commercials budgeting tool (line number, payee, overtime base, days, rate, hours at each overtime multiple, taxable and non-taxable extras; fringes left out) | A one-way chart-of-accounts mapping (production chart to studio chart) for reports. Episodic shows usually keep one budget per episode, with episodic summary and detail reports. Dashboard formulas published: total to date = actuals + POs; ETC = EFC - total to date; total budget = budget + overages; variance = EFC - budget. Posts payroll to general accounting software as one bill per payroll, one ledger account per line. |
| **MediaWeb** (Media Services) | Showbiz and reference budgets | Journal entries and POs pasted from Excel | | Its payroll files are fixed-width text (per a competitor's import guide). |
| **Saturation** | (section 4) | Payroll CSVs through a column mapper, contacts by name, earnings and fringes by account code | Syncs card transactions to general accounting software | Re-importing a payroll file duplicates it. |

Sources: https://support.castandcrew.com/hc/en-us/articles/51949802916116-PSL-Importing-a-Budget-from-Movie-Magic,
https://info.castandcrew.com/hubfs/KB/QSG/PSL%2B%20Quick%20Start%20Guides/PSL-manual-2026.pdf,
https://support.castandcrew.com/hc/en-us/articles/35382132906260-PSL-How-to-Import-an-Excel-File-into-PSL,
https://support.castandcrew.com/hc/en-us/articles/30846620132500-CPAS-How-can-I-import-my-Movie-Magic-Budgeting-file-into-CPAS,
https://support.castandcrew.com/hc/en-us/articles/30846809314196-Hours-How-to-Generate-a-Hot-Cost-Report,
https://support.castandcrew.com/hc/en-us/articles/51913076448916-PSL-How-do-I-load-PSL-Chart-of-Accounts-into-DPO,
https://mmb-docs.ep.com/Projects_and_Budgets/accounting_integrations.html,
https://downloads.adc.ep.com/doc/EP_SmartAccounting_Cost_Report_Worksheet_Troubleshooting.pdf,
https://www.ep.com/support/smartaccounting/,
https://junglesoftware.com/faqs/comparing-movie-magic-budgeting-to-gorilla/,
https://helpcenter.greenslate.com/en/articles/13371759-how-to-import-a-budget-from-movie-magic-budgeting,
https://support.greenslate.com/how-to-make-budget-changes,
https://support.greenslate.com/how-to-make-cost-report-changes-with-cost-report-v2.0,
https://support.greenslate.com/articles/6000195689portalid6000007110,
https://help.wrapbook.com/docs/import-a-project-budget-from-movie-magic-pas,
https://help.wrapbook.com/docs/import-a-project-budget,
https://help.wrapbook.com/docs/export-a-project-budget,
https://help.wrapbook.com/docs/chart-of-accounts-coa,
https://help.wrapbook.com/docs/coa-mapping-pas,
https://help.wrapbook.com/docs/cost-reports-pas,
https://help.wrapbook.com/docs/budget-efc-dashboard-pas,
https://help.wrapbook.com/docs/importing-pcard-journal-entries-in-bulk-pas,
https://help.wrapbook.com/docs/export-a-payroll-log-to-hot-budget,
https://help.wrapbook.com/docs/set-up-the-quickbooks-online-qbo-integration-in-wrapbook.md,
https://www.mediaservices.com/blog/mediaweb-simplified-production-accounting/,
https://support.saturation.io/en/articles/16443024-import-payroll-files-saturation-3-0

### 5.2 General accounting software

Small productions and production companies often keep books in general accounting
software. All of these publish their import layouts.

| Product | Chart of accounts import | Budget import | Transactions in and out | Notes |
|---|---|---|---|---|
| **QuickBooks Online** | Account name, type, detail type, optional account number; sub-accounts written as "Parent:Child"; no opening balances | A downloaded .xlsx template (10 MB or less), profit-and-loss budgets only, optionally by class or location, months across, no added rows or columns | Journal entry CSV: journal number, date, account name, description, debits, credits (class and location when those features are on; colon notation for sub-accounts); reports export to Excel or CSV | Account names must match exactly. A user reports a short limit on account number length (unverified). |
| **QuickBooks Desktop** | IIF files: tab-separated ASCII with header rows starting with "!" and transactions in blocks (transaction, split lines, end) | IIF covers budgets too | IIF general journal entries; report exports | Names must match exactly; credit split lines are negative. IIF is documented by the vendor (an import kit with headers). |
| **Xero** | CSV: code (required, unique, up to 10 characters), name (required), type (from Xero's list), tax code (required), description and a few switches | CSV, .xlsx or .ods, first sheet only: accounts down the first column written as "Name (Code)", months across the first row; values replace the same account and month | Manual journal CSV (date on the first line of each journal only, description, account code, tax rate, signed amount, two tracking category pairs; each journal must balance); bank statement CSV with one signed amount column, optionally precoded with account code and tax rate | No sub-accounts: tracking categories instead (where set, location or episode would go). Changing both code and name in one import replaces the account. |
| **Sage Intacct** | CSV: account number (up to 24 characters), name, balance sheet or income statement, normal balance, close-to account, flags for required dimensions | CSV against an existing budget: account, optional department, location and other dimensions, one column per reporting period with headers matching its period names exactly | | Dimensions are where set, location or episode codes go. |
| **Sage 300 Construction and Real Estate** | | Its own budget import tool and format | Its own GL transaction import | Formats documented per tool. |

Bank and card files: **OFX** is an open standard (now kept by the Financial Data Exchange
group; versions 1 are SGML, 2 are XML). **QFX** is Intuit's variant and **QBO** is OFX with
an Intuit bank identifier added. They carry bank activity (signed amount, date, payee,
memo, a bank-issued unique id per transaction) and no budget coding, so they need payee
rules or a review to land on accounts.

Sources: https://quickbooks.intuit.com/community/reports-and-accounting-5/replacing-old-chart-of-account-34897,
https://quickbooks.intuit.com/community/install-12/can-you-import-a-chart-of-accounts-with-sub-account-structure-to-qbo-the-sample-spreadsheet-only-shows-columns-for-account-number-account-name-type-and-detail-type-78000,
https://quickbooks.intuit.com/learn-support/en-us/help-article/taxation/create-import-budgets-quickbooks-online/L7SvmSAsU_US_en_US,
https://quickbooks.intuit.com/learn-support/en-us/help-article/import-export-data-files/import-journal-entries-quickbooks-online/L4tQBwbs7_US_en_US,
https://quickbooks.intuit.com/learn-support/en-us/help-article/list-management/iif-overview-import-kit-sample-files-headers/L5CZIpJne_US_en_US,
https://quickbooks.intuit.com/learn-support/en-us/help-article/import-export-data-files/export-import-edit-iif-files/L56LT9Z0Q_US_en_US,
https://central.xero.com/0/article/Import-a-chart-of-accounts-US,
https://central.xero.com/0/article/Import-a-budget,
https://central.xero.com/0/article/Add-import-and-post-manual-journals-US,
https://central.xero.com/0/article/Import-a-precoded-CSV-bank-statement,
https://www.intacct.com/ia/docs/en_US/help_action/More/Uploading_Data/GL/import-chart-of-accounts.htm,
https://www.intacct.com/ia/docs/en_ZA/help_action/More/Uploading_Data/GL/import-budgets.htm,
https://ethosystems.com/blog/import-data-into-sage-300-cre/,
https://financialdataexchange.org/about-fdx/ofx-work-group/,
https://en.wikipedia.org/wiki/Open_Financial_Exchange

### 5.3 Cost reports, hot costs and episodes

- **The cost report** is the weekly document studios and completion guarantors read. Across
  the platforms above the columns are the same vocabulary: account, description, original
  budget, approved overages or revisions, revised budget, actuals this period, actuals to
  date, open POs (commitments), total to date, estimate to complete, estimate final cost,
  variance, change in variance since last period, percent complete, and a variance note.
  ETC and EFC are linked (change one, the other follows). Exports are Excel, CSV and PDF.
- **Our cost report** (decision 45) already has budget, baseline, actual, ETC with a method,
  EAC (the same idea as EFC) and variance per account. Missing against the platforms:
  period actuals (an as-of date), commitments, approved overages as their own column,
  change in variance since the last report, a variance note.
- **Hot cost** is a daily report from the production report's times: overtime, meal
  penalties and fringes against that day's budget. It lives in payroll and timecard tools,
  not budgeting; the useful export from us is the day's budgeted hours and rates by crew
  line.
- **Episodes and amortization.** Series budgets are usually a pattern budget (a typical
  episode) plus an amortization budget of shared costs divided across episodes. Accounting
  systems carry the episode as a code: per line (series code), per account row (episode
  codes on every account), one code per upload, or one budget per episode. So an export
  that can be cut per episode, or carry an episode code column, fits all of them.

Sources: https://help.wrapbook.com/docs/cost-reports-pas,
https://downloads.adc.ep.com/doc/EP_SmartAccounting_Cost_Report_Worksheet_Troubleshooting.pdf,
https://info.castandcrew.com/hubfs/KB/QSG/PSL%2B%20Quick%20Start%20Guides/PSL-manual-2026.pdf,
https://www.peasebell.com/insights/production-accounting-101/,
https://johnwellsproductions.com/patterns-amorts-and-tax-incentives-oh-my

---

## 6. The wider ecosystem: scheduling

What a budget needs from a schedule is small: how many shoot days (by stage and location),
and for each element (cast member, crew position, vehicle, animal, piece of equipment) how
many days it works, holds and travels. That is the Day Out of Days (DOOD).

### 6.1 Tools and what they export

| Tool | Budget-relevant exports | Open? | Notes |
|---|---|---|---|
| **Movie Magic Scheduling 10** | DOOD Summary, Elements and Breakdown reports as PDF or Excel; the one-line schedule as .xlsx; a library file for the budgeting product (`.mbl`, categories with working-day counts) | Excel and PDF open; `.mbl`, `.mmsx`, `.msd`, `.sex` proprietary | Imports screenplay files (.fdx) for scene headers and speaking parts only. DOOD rules are documented: totals of days worked and held; travel, fitting and rehearsal count as worked; drop days don't count; company moves and holidays count only inside an element's work span when holds are allowed; the working week is set per board. |
| **Gorilla Scheduling** | Excel and .fdx; DOOD totals feed its sister budgeting app directly | Excel open | |
| **StudioBinder** | DOOD as print-to-PDF; the scheduling page advertises CSV export | PDF; CSV (columns not documented) | |
| **Celtx** | DOOD download or print | Format not stated | Uses combined start-work-finish codes. |
| **Filmustage** | DOOD as PDF or .xlsx, one category or all (fewer codes: no travel, rehearsal, drop or pick-up); a summary view with first day, last day and span; schedule exports in the scheduling product's native formats | .xlsx open; native exports proprietary | |
| **SetHero** | Converts PDF breakdowns printed from older scheduling versions into .xlsx and JSON; imports another tool's .xlsx reports | Open outputs | Says the scheduling product's files are encrypted, so third parties read its printed PDF reports instead. |
| **Yamdu** | Imports schedules from PDF reports printed with custom layouts; a schedule file for Showbiz | | Same reason. |
| **Saturation** | Reads scheduling files and DOOD PDFs to bring in cast day counts, locations and shoot-day globals | | Closest competitor to roadmap 3.8; it reads proprietary files we won't. |
| **Final Draft** | `.fdx` is XML (vendor schema); exports the scheduling product's exchange format from version 11; navigator data to CSV | `.fdx` readable XML; exchange format proprietary | The scheduling product can open a plain .fdx but sees no tags. |
| **Fountain** | Plain-text screenplay markup with an open syntax | Open | No breakdown tags, so it gives scenes, not elements or day counts. Not useful for budgeting. |

### 6.2 What a DOOD export carries

Across the documented tools: element id or cast number and name, category, a status code
per shoot day (start, work, finish, hold, travel, drop, pick-up, rehearsal, fitting, holiday,
company move, with variations), start and finish dates, counts of work, hold and travel
days, days off, total days worked (sometimes total paid days), notes, and the schedule's
shoot day count and span. Prep and wrap days are not in any documented DOOD; they are
budget assumptions.

### 6.3 What this means for us

- The only open, practical inputs are **Excel (or CSV) DOOD and element reports**, and
  stripboard or one-line exports for shoot day counts. PDF reports are the fallback other
  tools use because the native files are closed; we should not go there first.
- A DOOD sheet is a table: one row per element, a few count columns. Our mapped importer can
  already read it as a table; what's missing is mapping rows to **globals** (or to line
  quantities) and a safe re-import that shows the money effect before applying (W3.1).
- Each tool counts differently (holds, travel, holidays). The import should bring each
  count separately and let the user choose which feed a global, never pre-add them.

Sources: https://mms-docs.ep.com/ScheduleFiles/ExportingScheduleData.html,
https://mms-docs.ep.com/Reports/DOODSummary.html,
https://mms-docs.ep.com/Reports/ElementsReport.html,
https://mms-docs.ep.com/Reports/BreakdownReport.html,
https://mms-docs.ep.com/Breakdown/ImportingBreakdownSheets.html,
https://mms-docs.ep.com/DayOutofDays/DayOutofDays.html,
https://mms-docs.ep.com/DayOutofDays/DOODRules.html,
https://mms-docs.ep.com/Reports/DOODReport.html,
https://www.ep.com/movie-magic-scheduling/,
https://junglesoftware.com/movie-magic-scheduling-alternative/,
https://junglesoftware.com/day-out-of-days-report/,
https://support.studiobinder.com/en/articles/11574471-how-to-export-your-dood-report,
https://www.studiobinder.com/production-scheduling-software-filmmaker/,
https://support.celtx.com/hc/en-us/articles/225674908-Day-out-of-Days-Report,
https://help.filmustage.com/en/articles/8278194-view-and-export-the-dood-report,
https://help.filmustage.com/en/articles/4802061-export-a-project-to-movie-magic-scheduling-or-gorilla,
https://sethero.com/movie-magic-scheduling-exporter-to-excel/,
https://sethero.com/blog/how-to-export-data-from-movie-magic-scheduling-to-excel/,
https://help.sethero.com/en/articles/1513675-importing-data-from-scenechronize,
https://support.yamdu.com/en/articles/58206-import-movie-magic-schedule-to-yamdu,
https://saturation.io/integrations/movie-magic-scheduling,
https://kb.finaldraft.com/hc/en-us/articles/15575252743316-Can-Final-Draft-export-to-Movie-Magic-Scheduling-and-Budgeting,
https://kb.finaldraft.com/hc/en-us/articles/21045168358420-Can-I-export-data-from-the-Navigator,
https://fountain.io/syntax/

---

## 7. Commercial bid forms and the legal line

**Flag for the owner and a lawyer. This is not legal advice.**

- **Who owns it.** The commercial production bid form used in the US comes from the
  Association of Independent Commercial Producers (AICP), first published in 1975. The
  current revision is offered free from its website as a PDF, and the 2019 revision was
  released in Excel and PDF. The association says it shared the new structure with the
  bidding software makers that build on the form (several are named, including two
  products discussed above). Lettered sections A to N stayed the same in that revision;
  two later sections were reassigned.
- **License.** No copyright notice or license was found on the form PDF itself (a partial
  check). The association's website disclaimer claims copyright in site content, limits use
  to personal, non-commercial purposes, and says downloadable forms may not be sold,
  licensed or redistributed without written consent; that paragraph appears aimed at its
  form agreements, so whether it covers the bid form is unclear. "AICP" is a registered
  trademark. No public licensing program for software vendors was found.
- **How the line appears to fall (US).** Copyright does not protect systems or methods, and
  US regulations exclude blank forms designed to record information rather than convey it
  (a court applied that rule to a medical billing form). That suggests the idea of lettered
  sections with numbered lines, and line numbers as facts, are not protected, while the
  association's own prose (instructions, glossary, guidelines), any creative arrangement,
  and the trademark are a different matter.
- **Practice in the market.** Several budgeting tools advertise templates compliant with
  the form or reports in its layout, and one payroll platform notes that commercials
  usually use the form's line numbers instead of a chart of accounts.
- **Other countries.** The UK producers' association publishes a production contract with
  advertisers' bodies but no public budget form was found. Nothing public was found for
  other bodies.

**Recommendation.** Build commercial bid support as our own layout model (sections, line
numbers, estimate and actual columns, markup and fee lines), let users author or import
their own form-shaped templates, and do not ship anything that reproduces the
association's form text or layout, or uses its name, until the owner has a lawyer's view
or a written permission. Reading a user's own filled-in bid sheet through the mapped import
is fine.

Sources: https://aicp.com/business-resources/business-affairs-information/bidding-resources,
https://aicp.com/news-roundup/news-from-aicp/aicp-introduces-updated-and-revised-bid-form,
https://aicp.com/about/disclaimer,
https://www.law.cornell.edu/cfr/text/37/202.1,
https://www.law.cornell.edu/uscode/text/17/102,
https://law.justia.com/cases/federal/district-courts/FSupp/731/403/1877871/,
https://www.mediaservices.com/blog/showbiz-budgeting-10-ten-features/,
https://downloads.hotbudget.com/HotBudget_UserGuide-2.0.pdf,
https://saturation.io/templates/aicp,
https://help.wrapbook.com/docs/chart-of-accounts-coa,
https://www.a-p-a.net/join/members-resources/

---

## 8. Spreadsheet realities

Most budgets that are not in a dedicated tool arrive as a spreadsheet attached to an email.
These are the shapes and traps an importer meets. This section is general knowledge of the
file formats plus the public specifications cited.

### 8.1 Common shapes of a budget workbook

1. **Flat detail list.** One sheet, one row per line, an account code on every row,
   columns for description, quantity, unit, rate and total. The easiest case; our importer
   handles it today.
2. **Indented detail list.** Same sheet, but the account code appears once in a heading row
   ("2100 Producers") and the lines under it leave the account blank; category headings
   above that; a subtotal row after each account ("Total 2100"). Our importer already treats
   an account-only row as a heading and carries the account down; it does not yet recognize
   and skip subtotal rows the way the actuals importer does.
3. **Topsheet only.** A summary of categories or accounts with one amount each, often the
   only thing a financier or a studio sends. There are no lines to import; the right result
   is one "allow" line per account (amount 1, rate = the account total), so the budget
   totals match and can be broken down later.
4. **Topsheet tab plus one tab per account or department.** The topsheet's cells are
   formulas pointing at the department tabs. The tab name or a header cell names the
   account. Needs a "read every tab" mode with the account taken from the tab.
5. **Department sheets** (camera, art, wardrobe) with their own columns: rarely the budget's
   columns, often days and weeks as separate columns, kit fees, a running total. The mapped
   import is the answer; two columns joined (for example a name and a role) is a common
   need it doesn't cover yet.
6. **Cost report or ledger** from accounting: account, description, then several money
   columns (budget, actual, commitments, estimate to complete, estimate final cost,
   variance). This is actuals data, or a budget to compare against, not detail lines.
7. **Bid forms** (commercials): a fixed grid of numbered lines grouped into lettered
   sections with estimate and actual columns side by side (section 7).

Free templates that circulate widely (spreadsheet vendors, budgeting-tool blogs, template
sites) come in shapes 1 to 4, usually a topsheet plus detail, in Excel, Google Sheets and
Numbers versions; they are what an importer should expect from small productions. We
looked at their descriptions only and copied nothing.

Sources: https://www.smartsheet.com/content/film-budget-templates,
https://www.studiobinder.com/blog/the-essential-guide-for-crafting-film-budgets-with-free-film-budget-template/,
https://storiara.com/templates/budget-top-sheet

### 8.2 Traps inside .xlsx files

- **Merged cells.** A merged header keeps its value only in the top-left cell; the others
  are empty. Category names spanning columns, or an account code merged down several rows,
  read as blanks below the first row. A reader should optionally fill merged ranges.
- **Subtotal and total rows.** Rows whose description starts with "Total", rows with an
  amount but no account and no quantity, and grand-total rows at the bottom. Importing them
  doubles the money. The actuals importer already skips such rows; the line importer should
  too, and the reconciliation should compare against them instead.
- **Formulas versus values.** An .xlsx stores a formula and, usually, its last calculated
  value. We read the value. Files written by scripts and some web apps (for example the
  Python openpyxl library) can contain formulas with no cached value; those cells read as
  blank. The importer should warn when it sees formulas without values and suggest opening
  and saving the file in a spreadsheet app first.
- **Numbers stored as text** ("1,250.00", "$ 400", "(75)") and percentages stored as
  0.125 with a percent format. Our readers already parse currency text; percent formats need
  the number format, which we don't read yet.
- **Dates.** Serial numbers counted from 1900, or from 1904 in workbooks marked with the
  1904 date system (older Mac Excel files). A reader that ignores the flag is four years
  off.
- **Hidden rows, columns and sheets.** Often scratch work or old versions. Our importer
  already picks a visible sheet by default; hidden rows should be listed in the preview, not
  silently imported.
- **Several tables on one sheet**, notes above the header row, blank spacer rows and
  columns. Header-row detection handles the first; spacer rows are passed over.
- **Legacy .xls** (Excel 97 to 2003, BIFF8). A binary format, but Microsoft publishes its
  full specification ([MS-XLS]) under its Open Specification Promise, so reading it is
  allowed; it is a sizable reader to write (compound file container plus records). Many
  people still email .xls.

Sources: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xls/cd03cb5f-ca02-4934-a391-bb674cb8aa06,
https://www.loc.gov/preservation/digital/formats/fdd/fdd000510.shtml,
https://aident.ai/blog/openpyxl-data-only-formula-cache,
https://stackoverflow.com/questions/28517508/how-can-i-use-openpyxl-to-read-an-excel-cell-value-and-not-the-formula-computing

### 8.3 Other spreadsheet apps

- **LibreOffice and OpenOffice (.ods).** OpenDocument Spreadsheet is an OASIS and ISO
  standard: a ZIP of XML, so our ZIP, DEFLATE and XML readers already cover the container;
  only the content schema (tables, rows, cells, repeated-row and repeated-column counts,
  value types) is new. Cached values are stored next to formulas, as in .xlsx. LibreOffice
  also writes .xlsx and CSV.
- **Apple Numbers (.numbers).** A proprietary package format with no public specification.
  Numbers exports to Excel, CSV (one file per table, or a choice), PDF and others from File >
  Export To; we should ask users to export rather than read .numbers.
- **Google Sheets.** No local file; users download as .xlsx (all sheets), .ods (all
  sheets), PDF, or CSV and TSV (the current sheet only). The .xlsx download keeps cached
  values, so it reads well.

Sources: https://docs.oasis-open.org/office/OpenDocument/v1.3/OpenDocument-v1.3-part3-schema.html,
https://www.loc.gov/preservation/digital/formats/fdd/fdd000439.shtml,
https://support.apple.com/guide/numbers/export-to-excel-or-another-file-format-tan3b922d4ad/mac,
https://support.google.com/docs/answer/6000292?hl=en&co=GENIE.Platform%3DAndroid,
https://www.reddit.com/r/googlesheets/comments/1bws2uu/any_way_to_download_an_entire_spreadsheet_not/ (search excerpt only)

### 8.4 CSV dialects

- **Delimiter.** Comma in North America and the UK; semicolon wherever the comma is the
  decimal mark (most of continental Europe and South America), because Excel uses the
  operating system's list separator. Tab for "text" exports. Pipe now and then from
  accounting systems. Our reader sniffs comma, tab and semicolon; adding pipe is trivial.
- **Decimal mark and grouping.** `1,234.56` versus `1.234,56` (and `1 234,56` with a thin or
  non-breaking space in French locales, `1'234.56` in Swiss ones). A semicolon file is a
  strong hint of decimal commas, which our reader already uses; a column-by-column check
  (does every value parse one way and not the other?) is more reliable.
- **Encoding.** UTF-8 with or without a byte order mark; Excel's "Unicode text" is UTF-16
  with a mark; older Windows exports are Windows-1252; old Mac exports may be Mac Roman.
  We handle the first three.
- **Line endings and quoting.** CRLF, LF or old CR; quoted fields with embedded line breaks
  and doubled quotes (RFC 4180). We handle these.
- **Formula injection.** Cells starting with `=`, `+`, `-` or `@` can run as formulas when a
  CSV is opened in a spreadsheet. Our CSV writer neutralizes them; importers should treat
  such text as text.

Sources: https://www.ablebits.com/office-addins-blog/change-excel-csv-delimiter/,
https://learn.microsoft.com/en-us/answers/questions/5164914/csv-file-are-using-wrong-separator,
https://www.rfc-editor.org/rfc/rfc4180

### 8.5 Paste

Copying a block from Excel, Numbers or Google Sheets puts tab-separated text (and usually
HTML) on the clipboard. Our grid already pastes tab-separated text into the visible
columns. A "paste as new lines with mapping" (the import dialog fed from the clipboard
instead of a file) would cover the common "I just want these 40 rows in this account" case.

### 8.6 PDF budgets

Only as a last resort. A PDF carries positioned text, not cells. Recovering a budget means
grouping text runs into rows by their vertical position, finding column boundaries from the
header, and reading numbers, with every page header, footer and subtotal in the way.
Scanned PDFs need OCR on top. Output would always need a review screen and a reconciliation
against the PDF's printed totals. Effort large, fidelity low: lines and amounts at best,
never tags or setup.

---

## 9. Gaps

Where we fall short of what the ecosystem expects, roughly in order of how often a user
would hit it:

1. **No account-level budget export.** Every accounting platform takes account code,
   description and amount (plus episode, location, set or free-field codes). Our
   tab-delimited export, described in the app as the accounting shape, is the Detail table.
2. **No chart-of-accounts file**, in or out.
3. **Spreadsheet import misses common shapes**: subtotal and total rows get imported as
   lines unless a Kind column says otherwise; no account-level (topsheet-only) mode; no
   one-tab-per-account mode; merged cells read as blanks; no paste-as-lines.
4. **No structured (XML or JSON) import of any kind**, so none of the reference's richest
   exports, nor other tools' JSON, can be read even with a manual mapping.
5. **Export misses** budget info, charges and credits as their own sheet, Original and
   Variance columns, a per-line fringe breakdown, a per-category sheet layout, and a group
   filter.
6. **No documented evaluated interchange.** `.mthd` is open but holds inputs; a consumer
   must reimplement our engine to get totals.
7. **Actuals import has no presets** for the general accounting reports most small
   productions export, cannot read a cost report (budget, EFC) back, and has no payroll-log
   shape.
8. **Cost report export** lacks period actuals, commitments, approved overages and the
   change-in-variance column the platforms use.
9. **Episodes**: no per-episode cut or episode code column for exports; no pattern and
   amortization model.
10. **File types**: no .ods, no .xls, no 1904 dates, no warning for formulas without cached
    values, no pipe delimiter.
11. **Schedules**: nothing yet (roadmap 3.8).
12. **Commercial bid layouts**: nothing yet, and a legal question first (roadmap 3.13).
13. **Chart mapping to a studio's chart** (one streamer requires its own chart or a mapping
    to it; a payroll platform offers a one-way mapping for reports): nothing yet.

---

## 10. Proposal: what to build, in three waves

Sizes follow the roadmap: **S** a day or two, **M** about a week, **L** more than a week.
"Samples" says what test files each item needs. Synthetic means files we write ourselves
from public documentation (committed as fixtures); real means exports from a person's
own copy of another tool (kept out of the repository unless the owner decides otherwise).
Anything that needs the owner's licensed copy of the reference product is marked
**optional**: the owner has accepted that risk and decides if and when.

The ordering principle: first the doors every downstream system already uses (account-level
budgets, charts of accounts, plain spreadsheets, accounting exports), then the reference
product's open exports, then the production loop (schedules, bank feeds, bid forms).

### 10.1 Wave 1: open doors (no samples from any vendor needed)

| # | Feature | Value | Effort | Legal notes | Depends | Samples |
|---|---|---|---|---|---|---|
| W1.1 | **Accounting budget export** (account level). One row per postable account: account code, description, amount. Options: the code's spelling (the budget's pattern, digits only, another separator, zero padding); category code and name columns; category fringe accounts as their own rows and a chosen account for fringes posted by budget or level; charges and credits as rows with their account numbers (on or off); production code columns (series or episode, location, set, up to four free fields) filled from a default or from the line's set, location or a group, which splits an account into one row per code; one file per episode (a group or set chosen as the episode) or an episode column; no totals row; UTF-8 with an optional byte order mark. CSV, tab-delimited and .xlsx. Presets: a generic one, more as platforms document their budget imports; length checks against the limits platforms publish (section 5.1 has one platform's four-character production codes and thirty-character descriptions), shown as warnings. The amounts must add up to the topsheet to the cent, with the check shown in the preview. | High. Every accounting, cost report and payroll platform in section 5 takes the budget at account level; today our tab-delimited export is the Detail table, which they can't use. Also the honest answer to "my accountant's system only takes the reference product's file": give them the columns they actually keep. | S (M with the code columns and episode cut) | None: generic columns, our own data. Platform names in preset labels are descriptive only. | none | Synthetic; the platforms' public column lists (section 5.1). |
| W1.2 | **Chart of accounts export and import.** Export: code, name, level (category or account), parent code, above or below the line, postable or header. Presets with the column names and rules general accounting software documents (section 5.2): QuickBooks Online (name, type, detail type, number, "Parent:Child" names), Xero (code up to 10 characters, name, type, tax code), Sage Intacct (number, name, income statement type, debit normal balance), and a generic code, name, parent sheet as production platforms take. Import: the same shapes (or any sheet of codes and names, parent optional) become a template budget with empty accounts, or add missing accounts to this one, through the existing mapping dialog. | Medium-high. Accountants set up the ledger from the budget's chart; productions that start from an accountant's or a studio's chart can start a budget from it. | S | Never bundle any vendor's, studio's or guild's chart; users bring their own. | none | Synthetic from the published column lists. |
| W1.3 | **Export completeness** (decision 30's "not yet"): a Budget Info sheet, a Charges and Credits sheet (rules and computed amounts), Original and Variance columns, choosing which groups count (as the report does), a Line Fringes sheet (one row per line and fringe with its amount), an optional people-friendly layout with one sheet per category (what other tools' Excel exports look like), deflate compression for big workbooks. | Medium. Closes the gap with the reference's Excel export (budget info, tags) and gives payroll and accounting the fringe split they reconcile against. | M | None. | none | None (round-trip tests). |
| W1.4 | **Interchange JSON** (`method export --json`, File > Export > JSON): the evaluated budget, documented and versioned with a published JSON Schema: budget info, currencies and rates, account pattern, production levels, categories, accounts and lines with every computed total, each line's fringe amounts, tags, included flag and range, setup with definitions (fringe rates and cutoffs, global formulas and values, units and hours, groups and their switches), charges and credits with computed amounts, ATL, BTL, grand and net totals, optionally actuals and the cost report. Money as exact decimal strings. An XML rendering of the same model is a small add-on for systems that prefer XML. | High for integrators. `.mthd` is open but stores inputs, so a reader must recompute; this export carries the answers. It is what the reference's XML (Advanced) and JSON exports do for their downstream systems, in a documented form. | M | Our own format; document it under an open license so platforms can adopt it. | W1.3 helps | None (schema tests). |
| W1.5 | **Spreadsheet import, second round.** (a) Detect and skip subtotal and total rows (the actuals importer's rule) and reconcile against them; (b) an **account-level mode**: a topsheet or account summary becomes one allow line per account so totals match; (c) an **every-tab mode** for workbooks with one tab per account or department, the account taken from the tab name or a chosen cell; (d) fill merged ranges; (e) the 1904 date system; (f) warn about formulas with no cached value; (g) two columns joined into one field (name and role into description); (h) pipe delimiter and a per-column decimal-comma check; (i) **paste as lines**: the import dialog fed from the clipboard. | High. Section 8 shapes 2 to 5 are the everyday budget email; (b) also covers the account-level XML (Basic) or tab files from the reference once a sample shows their columns. | M (each part S) | None. | none | Synthetic workbooks for every shape in 8.1 and trap in 8.2; a few real anonymized budgets if users offer them. |
| W1.6 | **OpenDocument (.ods) reader.** Our ZIP, DEFLATE and XML readers cover the container; only the table schema is new (rows and columns with repeat counts, typed values, cached formula results). | Medium. LibreOffice users and Google Sheets' .ods download. | S | OASIS standard, free to implement. | none | Synthetic (made with LibreOffice from our own test sheets). |
| W1.7 | **Actuals presets and two new shapes.** Presets: column titles, sign conventions and header-block skipping for the transaction and ledger reports of QuickBooks, Xero and Sage (section 5.2); splitting combined "code name", "Name (Code)" or "Parent:Child" account fields into a number; debit and credit or signed amount remembered per preset. New shapes: a **cost report** read back (account, budget, actual to date, POs, EFC) as account-level actuals or as a comparison baseline, and a **payroll log** CSV (line, payee, days, rate, overtime hours, extras) as labor actuals on lines. | High for small productions on general accounting software and for anyone reconciling against the accountant's weekly cost report; the importer exists, presets make it one click. | S per preset, M per new shape | Column titles are facts; no vendor text copied. | 3.7 (built) | Synthetic from each vendor's public column documentation; real anonymized exports welcome. |

### 10.2 Wave 2: the reference product's open exports

| # | Feature | Value | Effort | Legal notes | Depends | Samples |
|---|---|---|---|---|---|---|
| W2.1 | **Structured-file importer** (any XML or JSON): open the file, pick the repeating element for detail lines (and optionally for accounts and categories, or take them from the ancestors), map child fields to our fields, preview and reconcile like the spreadsheet import, save the mapping per file shape; `method import` reads the same mapping file. Uses our XML tokenizer (no DTDs, no entities) and the JSON parser we already ship. | High. Opens every XML or JSON export (the reference product's, other budgeting tools', accounting platforms') without guessing a schema, and is the base for W2.2. | M | Reading open exports is allowed by `CLEAN-ROOM.md`. | none | Synthetic XML and JSON shaped like a budget tree. |
| W2.2 | **Reference XML (Advanced) and JSON preset** (roadmap 1.16): recognize the layout, rebuild the chart, lines, tags and whatever setup the file carries, recalculate, and reconcile line by line and account by account against the file's own totals; list what didn't carry over. | Highest for switching real budgets. | L | Allowed open export. Keep the raw sample exports out of the repository; commit only fixtures we write ourselves in the same shape, after the owner decides. | W2.1; samples | **Optional**: the owner's licensed copy, using `docs/research/export-test-plan.md`. |
| W2.3 | **Reference Excel preset**: a saved mapping (or an every-tab mapping) for its workbook; part of it may already work through W1.5 with a manual mapping. | High (the most emailed form). | M | Same as W2.2. | W1.5; samples | **Optional**, as W2.2. |
| W2.4 | **Reference Tab Delimited and XML (Basic) presets**: probably account- or detail-level flat lists; mostly useful as reconciliation targets and for account-level budgets. | Medium. | S each | Same. | W1.5 (b), W2.1; samples | **Optional**, as W2.2. |
| W2.5 | **Engine fixes from the export test** (roadmap 1.17): currency direction, cutoff factor, flat fringe rules, adjustment rows, charge base, rounding. | High for faithful totals. | M | None. | W2.2 samples | **Optional**, as W2.2. |
| W2.6 | **Writing the reference's export layouts** so platforms that accept only those files would take ours. **Not recommended now.** | Medium: removes the "my accountant needs that file" blocker. | M | Needs samples to know the layouts, a check that nothing in a layout is protected, and care with trademarks (never call our files the vendor's). Prefer W1.1 and W1.4 and asking platforms to accept them. Lawyer's view first. | W2.2 | **Optional**, as W2.2. |

What works with no samples at all: W2.1 in full; W1.5 (b) and the existing mapped import
will already read many of the reference's Excel and tab exports with a manual mapping; the
reconciliation tells the user how close the import came.

### 10.3 Wave 3: the production loop

| # | Feature | Value | Effort | Legal notes | Depends | Samples |
|---|---|---|---|---|---|---|
| W3.1 | **Schedule day counts** (roadmap 3.8): read a Day Out of Days or stripboard export (CSV or .xlsx from the scheduling tools in section 6) with a mapping (element, category, start and finish dates, work, hold and travel days kept apart, total days, shoot days by stage or location; prep and wrap stay budget assumptions, since no DOOD carries them); turn chosen rows into globals (`CAST_1_DAYS`, `SHOOT_DAYS_STAGE`), remember the source per global, and on re-import show which globals change and what that does to the totals (the diff we already have) before applying. | High: the competitor feature the reference lacks (it imports static blocks only). | L | Read only open exports; never a scheduling tool's native file. | W1.5 readers | Synthetic Day Out of Days files from public descriptions of the columns; real exports from free tiers of schedule tools, made by a user. |
| W3.2 | **Cost report export layouts**: the accounting-style columns (original budget, revised budget, period actual, actual to date, commitments, estimate to complete, estimate final cost, variance, percent) as a saved report layout and export sheet; period and to-date need actuals by period, commitments need purchase orders. | Medium-high for productions without an accounting platform. | M | Column names are industry vocabulary; no vendor layout copied. | 3.7; actuals by period (not built) | Synthetic. |
| W3.3 | **Bank and card statements** (OFX, QFX, CSV) as actuals, with payee rules that suggest an account and a review of uncoded rows. OFX is an open, published standard. QBO is the same format with an Intuit header. | Medium for small productions paying by card. | M | OFX is open. | 3.7 | Synthetic from the published OFX specification. |
| W3.4 | **Monthly spread exports**: budgets by month in the layouts general accounting software documents (QuickBooks Online's template, Xero's "Name (Code)" rows with months across, Intacct's period-named columns, IIF budgets); needs a cash-flow feature first. | Medium, later. | L | None. | Cash-flow spreading (not on the roadmap) | Synthetic. |
| W3.5 | **Commercial bid layouts** (roadmap 3.13): our own layout model (lettered sections, numbered lines, estimate and actual side by side, markup and fee lines), user-authored. Do not ship or mimic a trade association's form unless it is licensed (section 7). Import of bid sheets the user already has is W1.5 with a mapping. | Medium (opens commercials). | L | Owner and lawyer to settle the license question before any form-shaped template ships. | 2.4 | Synthetic. |
| W3.6 | **Legacy .xls reader** ([MS-XLS], published under Microsoft's Open Specification Promise). | Low-medium; asking users to re-save works. | L | Allowed. | none | Synthetic (saved from Excel or LibreOffice). |
| W3.7 | **Chart mapping** (this budget's chart to a studio's or streamer's chart): a two-column mapping kept in the budget, used by the account-level export, the chart export and the cost report so the receiving side sees its own codes; many-to-one allowed, amounts unchanged. | Medium (studio and streamer deliveries). | M | Users bring the studio's chart; never bundled. | W1.1, W1.2 | Synthetic. |
| skip | **PDF import** (last resort), **Apple Numbers files** (export from Numbers instead), **any native vendor file** (`.mbd`, `.mmbx`, `.mbt`, `.mbl`, `.mmbj`, scheduling files) | | | `CLEAN-ROOM.md` | | |

### 10.4 Suggested order inside the waves

1. W1.1 accounting budget export and W1.2 chart of accounts (small, unblock every downstream
   system).
2. W1.5 (a), (b), (c), (i): total rows, account-level mode, every-tab mode, paste as lines.
3. W1.4 interchange JSON with W1.3's extra sheets.
4. W1.7 actuals presets, W1.6 .ods.
5. W2.1 structured-file importer; then W2.2 to W2.5 if and when the owner produces samples.
6. W3.1 schedule day counts; then W3.2, W3.7, W3.3; W3.5 after the legal check.

---

## 11. Open questions for the owner

1. **Reference samples.** Will you run the export test (`docs/research/export-test-plan.md`)
   with your licensed copy, and if so, which formats first? Recommendation: JSON and Excel
   first, then XML (Advanced), Tab Delimited, XML (Basic). Wave 1 and W2.1 don't wait on it.
2. **Which downstream systems matter to you?** Which accounting, payroll or cost report
   platform do your productions actually hand budgets to? That decides which named presets
   W1.1, W1.2 and W1.7 get first.
3. **Episodes and amortization.** Do your series budgets need per-episode or amortized
   exports (accounting platforms stamp an episode or location code on every row)? If yes,
   is "episode" a tag we should add, or do groups or sets cover it?
4. **Writing the reference's export layouts (W2.6).** Should we ever write files shaped like
   the reference's XML or JSON exports so platforms that only accept those would take ours?
   Default: no, pending samples and a lawyer's view; we push our documented interchange and
   account-level files instead.
5. **Raw sample files in the repository.** If you produce samples, may reduced versions of
   them be committed as test fixtures, or should fixtures always be our own synthetic files
   in the same shape (the default)?
6. **Interchange XML.** JSON only for the interchange export, or also an XML rendering for
   systems that prefer XML?
7. **Commercial bid forms (W3.5).** Are commercials in scope? If yes, are you willing to get
   a license or a lawyer's view on the trade association's form before we build anything
   form-shaped?
8. **Cash flow.** Do you want monthly spreads (needed for budgets in general accounting
   software and for cash-flow reports)? It is a feature of its own before W3.4.
9. **Legacy .xls.** Worth a reader, or is "re-save as .xlsx" acceptable?
