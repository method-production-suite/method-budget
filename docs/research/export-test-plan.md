# Export test plan: a made-up budget for MMB

Hi Sam. This is a recipe. You type a tiny, fake budget into your copy of Movie Magic
Budgeting, export it in every open format, and drop the files in a folder. We then build our
importers against those files, and the totals tell us how MMB does its math where the manual
is unclear.

It should take under an hour. Nothing here is real production data, and nothing comes from an
MMB template. Every name starts with **Z** so it's easy to spot in the exported files.

Clean-room reminder (see `CLEAN-ROOM.md`): exported Excel, text, XML, JSON and PDF files are
fine for us to study. MMB's own budget file (`.mmbx` or `.mbd`) is not. Please don't put
those in the export folder or send them to us.

---

## Part 0: before you start

1. Make the folder `Screenshots/exports/` inside the project (and `Screenshots/exports/shots/`
   for screenshots). The whole `Screenshots/` folder is already gitignored, so nothing in it
   can get committed by accident. Please keep every file from this test in there.
2. In MMB, start a **new blank budget** (not from a template). Call it `FWB Export Test 01`.
3. If the blank budget comes with some starter units, fringes or other setup items, leave them
   alone. Don't use them on any line.
4. If a value won't fit or MMB refuses something, write down what happened and carry on.
   That's useful evidence too.

Tip: blank means leave the cell empty. Don't type 0 or 1 into a blank cell.

---

## Part 1: the test budget

Enter things in this order. MMB needs the setup items to exist before lines can use them.

### Step 1. Preferences

Take a screenshot of each preference screen **before** you change anything, so we learn the
defaults (`shots/01-prefs-default-*.png`). Then set:

| Setting | Value |
|---|---|
| Base currency | USD |
| Totals (display) currency | USD |
| Fringe posting | by category |
| Percentage fringe cutoff factor | 1.0 |
| Decimals shown on totals (system preference, if your version has it) | 2 |
| Net total row | on |
| Account coding | whatever the blank budget uses, as long as 4-digit numbers like 7101 work. If they don't, tell us what you used. |

Screenshot the screens again after the change (`shots/02-prefs-set-*.png`).

### Step 2. Units

Four made-up units. The hours matter for the flat fringe test.

| Name (singular) | Plural | Hours equivalence |
|---|---|---|
| Zday | Zdays | 12 |
| Zweek | Zweeks | 60 |
| Zallow | Zallow | blank (not a time unit) |
| Zbox | Zboxes | blank (not a time unit) |

### Step 3. Globals

| Name | Value or formula | Should show |
|---|---|---|
| ZWEEKS | `3` | 3 |
| ZDAYS | `ZWEEKS * 5 + 2` | 17 |
| ZRATE | `312.34` | 312.34 |

ZDAYS uses ZWEEKS on purpose (a global that references another global).

### Step 4. Fringes

| Name | Type | Rate | Unit | Cutoff |
|---|---|---|---|---|
| ZPCT | percent | 12.5 | % | 2,222.22 |
| ZPCT2 | percent | 3.21 | % | blank |
| ZFLAT | flat | 25.00 | Zday | blank |

Descriptions: anything starting with Z is fine, for example `ZPCT test fringe`.

### Step 5. Groups

| Name | In budget total | Used for |
|---|---|---|
| ZOPTB | **off** | suppressed "option B" lines |
| ZCREDIT | on | the credit's included group |
| ZNOCHG | on | excluded from both charges |

### Step 6. Sets and locations

| Kind | Name | Other fields |
|---|---|---|
| Set | ZSET1 | description `Test Set One` |
| Location | ZLOC1 | city `Testville`, region `Test Region`, country `Testland` |

### Step 7. Currencies

| Code | Rate | Notes |
|---|---|---|
| USD | 1 | base currency |
| EUR | 3.00000 | deliberately silly rate, so the conversion direction is obvious |

### Step 8. Budget info (optional, 1 minute)

If easy, fill the title with `FWB Export Test 01` and the production name with
`ZTEST PRODUCTION`. If you can add a custom text field, call it `ZINFO` with value `hello 5150`.
That shows us where budget info lands in each export.

### Step 9. Topsheet

Five rows, in this order:

| Row | Kind | Number | Name |
|---|---|---|---|
| 1 | category | 7100 | ZTEST ABOVE |
| 2 | production total | | ZTOTAL ATL |
| 3 | category | 7200 | ZTEST CREW |
| 4 | category | 7300 | ZTEST OTHER |
| 5 | production total | | ZTOTAL BTL |

### Step 10. Accounts

| Category | Account | Name |
|---|---|---|
| 7100 | 7101 | Zed Writer |
| 7100 | 7102 | Zed Producer |
| 7200 | 7201 | Zed Grip |
| 7200 | 7202 | Zed Driver |
| 7300 | 7301 | Zed Gear |

### Step 11. Detail lines

Turn on the optional detail columns first (Unit2, Unit3, 4X, Unit4, Curr, Fringes, Groups,
Set, Location) so you can see everything you type.

Type formulas exactly as shown (for example `ZWEEKS`, not 3). Type rates exactly, including
`432.105` with three decimals. Blank cells stay blank.

**Account 7101 Zed Writer**

| # | Description | Amt | Unit | X | Rate | Fringes | Other |
|---|---|---|---|---|---|---|---|
| 1 | ZED WRITER DEAL | | | | | | title line: description only |
| 2 | Draft fee | 1 | Zallow | | 12345.67 | ZPCT2 | |
| 3 | Polish | `ZWEEKS` | Zweek | | 432.105 | ZPCT2 | |

**Account 7102 Zed Producer**

| # | Description | Amt | Unit | Rate | Fringes | Other |
|---|---|---|---|---|---|---|
| 1 | Prep | 7 | Zweek | 1000.00 | ZPCT2 | |
| 2 | Shoot | 2 | Zweek | 1500.00 | | |
| 3 | (subtotal row) | | | | | insert a subtotal row here |
| 4 | Post | 1 | Zweek | 500.00 | | |
| 5 | Contingency | 0.1 | | (locked) | see note | insert a subtotal **adjustment** row here, Amt 0.1 |

Note for row 5: try to apply ZPCT2 to the adjustment row. If MMB lets you, keep it and tell us.
If it won't, that's the answer. Screenshot either way (`shots/19-adjustment-fringe.png`).

**Account 7201 Zed Grip** (one person, so bracket all three lines into one fringe range)

| # | Description | Amt | Unit | Rate | Fringes | Groups | Set | Location |
|---|---|---|---|---|---|---|---|---|
| 1 | Grip prep | 4 | Zday | `ZRATE` | ZPCT, ZFLAT | ZCREDIT | ZSET1 | ZLOC1 |
| 2 | Grip shoot | `ZDAYS` | Zday | `ZRATE` | ZPCT, ZFLAT | ZCREDIT | ZSET1 | ZLOC1 |
| 3 | Grip wrap | 2 | Zday | `ZRATE` | ZPCT, ZFLAT | ZCREDIT | ZSET1 | ZLOC1 |

Then select lines 1 to 3 and make them one fringe range.

**Account 7202 Zed Driver** (no fringe range here)

| # | Description | Amt | Unit | Rate | Fringes | Groups |
|---|---|---|---|---|---|---|
| 1 | Driver | `ZWEEKS` | Zweek | 1234.56 | ZFLAT, ZPCT2, ZPCT | |
| 2 | Driver kit | 2 | Zallow | 65.43 | ZFLAT | |
| 3 | Option B driver | 1 | Zweek | 9999.99 | ZPCT2 | ZOPTB |
| 4 | Fuel refund | 1 | Zallow | -77.77 | | |

**Account 7301 Zed Gear**

| # | Description | Amt | Unit | X | Unit2 | Curr | Rate | Unit3 | 4X | Unit4 | Fringes | Groups | Location |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | Walkie batteries | 7 | Zbox | 3 | Zweek | | 12.34 | Zbox | 2 | Zallow | | | ZLOC1 |
| 2 | Foreign lens | 5 | Zday | | | EUR | 100.00 | | | | ZPCT2 | | |
| 3 | No-charge item | 1 | Zallow | | | | 300.00 | | | | | ZNOCHG | |
| 4 | Option B lens | 1 | Zallow | | | | 888.88 | | | | | ZOPTB, ZCREDIT | |

**Notes**

- On 7301 line 1 (Walkie batteries), add a note: `ZNOTE line note 4242`.
- On account 7301 itself (one level up), add a note: `ZNOTE account note 4343`.

### Step 12. Contractual charges

Create both, then **apply the flat one first, then the percent one**. The order matters for one
of the questions.

| Name | Account | Type | Amount | Excluded amounts | Excluded groups |
|---|---|---|---|---|---|
| ZFLATCHG | 7801 | flat fee, USD | 2,500.00 | one row: `ZEXCL B`, 222.00 | ZNOCHG |
| ZPCTCHG | 7802 | percent | 5 | one row: `ZEXCL A`, 111.00 | ZNOCHG |

Don't exclude any categories or accounts.

### Step 13. Credit

One credit, then apply it.

| Field | Value |
|---|---|
| Name | ZINCENTIVE |
| Account | 7901 |
| Overall applied credit cap | 1,840.00 |
| Discount | 2 % |

Credit rows:

| Row | Type | Source | Percent | Cap type | Cap | Excluded fringes |
|---|---|---|---|---|---|---|
| 1 | included group | ZCREDIT | 20 | none | | ZFLAT |
| 2 | flat amount | 1,000.00 | 10 | credit cap | 50.00 | |
| 3 | flat amount | 3,000.00 | 15 | qualified spend cap | 2,000.00 | |

### Step 14. Quick check

Compare your topsheet to the table in Part 2. A mismatch isn't a mistake on your part: it's
the answer to a question. Just don't fix it; screenshot it and move on. (If one line is wildly
off, do double check that line's typing first.)

---

## Part 2: what we predict

These are the numbers our engine gives, using our rules in `docs/decisions.md`: money rounded
to cents once per line, half away from zero; percent fringes once per earner (a line or a
fringe range); fringes posted by category. They were computed twice, once in a spreadsheet-style
script and once by running our actual engine, and both agree to the cent.

If MMB shows totals in whole dollars, compare to the nearest dollar. Rounded displays may not
add up exactly; that's normal.

### Lines

| Acct | # | Line | Line total | Fringes on this line |
|---|---|---|---|---|
| 7101 | 1 | ZED WRITER DEAL | 0.00 | |
| 7101 | 2 | Draft fee | 12,345.67 | ZPCT2 396.30 |
| 7101 | 3 | Polish (3 x 432.105 = 1,296.315) | 1,296.32 | ZPCT2 41.61 |
| 7102 | 1 | Prep | 7,000.00 | ZPCT2 224.70 |
| 7102 | 2 | Shoot | 3,000.00 | |
| 7102 | 3 | subtotal row (shows) | 10,000.00 | adds nothing |
| 7102 | 4 | Post | 500.00 | |
| 7102 | 5 | Contingency (10% of 10,500) | 1,050.00 | none (we don't fringe adjustment rows) |
| 7201 | 1 | Grip prep (4 x 312.34) | 1,249.36 | ZPCT 48.31, ZFLAT 100.00 |
| 7201 | 2 | Grip shoot (17 x 312.34) | 5,309.78 | ZPCT 205.32, ZFLAT 425.00 |
| 7201 | 3 | Grip wrap (2 x 312.34) | 624.68 | ZPCT 24.15, ZFLAT 50.00 |
| 7202 | 1 | Driver (3 x 1,234.56) | 3,703.68 | ZFLAT 375.00, ZPCT2 118.89, ZPCT 277.78 |
| 7202 | 2 | Driver kit | 130.86 | ZFLAT 25.00 |
| 7202 | 3 | Option B driver (suppressed) | 0.00 | none |
| 7202 | 4 | Fuel refund | (77.77) | |
| 7301 | 1 | Walkie batteries (7 x 3 x 12.34 x 2) | 518.28 | |
| 7301 | 2 | Foreign lens (500.00 EUR x 3) | 1,500.00 | ZPCT2 48.15 |
| 7301 | 3 | No-charge item | 300.00 | |
| 7301 | 4 | Option B lens (suppressed) | 0.00 | |

How the tricky fringes work under our rules:

- **Grip range, ZPCT**: the three lines are one earner. 7,183.82 is capped at 2,222.22, and
  12.5% of that is 277.78, split back over the lines by size (48.31 + 205.32 + 24.15).
- **Driver, ZPCT**: its own earner, capped at 2,222.22, so also 277.78.
- **Driver, ZFLAT**: 25 per Zday on a Zweek line. One Zweek is 60 / 12 = 5 Zdays, so
  25 x 3 weeks x 5 = 375.00.
- **Driver kit, ZFLAT**: Zallow has no hours, so no conversion: 25.00 once.

### Accounts

| Account | Lines | Fringes earned |
|---|---|---|
| 7101 Zed Writer | 13,641.99 | 437.91 |
| 7102 Zed Producer | 11,550.00 | 224.70 |
| 7201 Zed Grip | 7,183.82 | 852.78 |
| 7202 Zed Driver | 3,756.77 | 796.67 |
| 7301 Zed Gear | 2,318.28 | 48.15 |

### Topsheet (fringes posted by category)

| Row | Lines | Fringe account | Total |
|---|---|---|---|
| 7100 ZTEST ABOVE | 25,191.99 | 662.61 | 25,854.60 |
| ZTOTAL ATL | | | **25,854.60** |
| 7200 ZTEST CREW | 10,940.59 | 1,649.45 | 12,590.04 |
| 7300 ZTEST OTHER | 2,318.28 | 48.15 | 2,366.43 |
| ZTOTAL BTL | | | **14,956.47** |
| ATL + BTL | | | 40,811.07 |
| ZFLATCHG (2,500 - 222 - 300) | | | 1,978.00 |
| ZPCTCHG (5% of 40,811.07 - 111 - 300) | | | 2,020.00 |
| **Grand total** | | | **44,809.07** |
| ZINCENTIVE | | | (1,803.20) |
| **Net total** | | | **43,005.87** |

### Credit detail

| Row | Gross | Excluded fringes | Net | Credit |
|---|---|---|---|---|
| 1 group ZCREDIT (grip lines plus their fringes) | 8,036.60 | 575.00 | 7,461.60 | 1,492.32 |
| 2 flat, credit cap 50 | 1,000.00 | 0 | 1,000.00 | 50.00 |
| 3 flat, qualified spend cap 2,000 | 3,000.00 | 0 | 3,000.00 | 300.00 |
| Sum | | | | 1,842.32 |
| After overall cap | | | | 1,840.00 |
| After 2% discount | | | | **1,803.20** |

### Other totals you may see

| Thing | Predicted |
|---|---|
| Fringe ZPCT total | 555.56 (4 lines) |
| Fringe ZPCT2 total | 829.65 (5 counted lines) |
| Fringe ZFLAT total | 975.00 (5 lines) |
| Group ZCREDIT | 7,183.82 |
| Group ZNOCHG | 300.00 |
| Group ZOPTB if it were switched on | 10,888.87 (counts 0 now) |
| Set ZSET1 | 7,183.82 |
| Location ZLOC1 | 7,702.10 |

### If you see a different number

Each row assumes only that one thing differs from our rules. If two things differ at once the
numbers combine, but the line and account rows above will still pin each one down.

| Where | If MMB shows | It means |
|---|---|---|
| 7101 Polish | 1,296.31 | MMB doesn't round a half cent up (or rounds later) |
| 7102 Contingency | 50.00 (7100 = 24,854.60) | adjustment rows restart at a subtotal row |
| 7102 Contingency | 1,072.47 (7100 = 25,877.07) | adjustment rows include fringes of the lines above |
| 7102 Contingency fringe | 33.71 (7100 = 25,888.31) | MMB fringes adjustment rows (only if it let you apply ZPCT2) |
| 7201 ZPCT | 512.04 total (156.17, 277.78, 78.09; 7200 = 12,824.30) | the range didn't act as one earner |
| 7201 ZPCT | 897.98 total | the cutoff was ignored |
| ZFLAT on Driver | 25.00 (or 75.00) | no hours conversion (or per-unit with no conversion) |
| ZFLAT, all lines unconverted | 7200 = 11,740.04 | flat fringes never convert by hours |
| ZFLAT on Driver kit | 50.00 (7200 = 12,615.04) | flat fringes multiply by quantity when the unit has no hours |
| Option B driver | 9,999.99 (7200 = 22,911.03) | group suppression didn't apply |
| Foreign lens | 166.67 (7300 = 990.30) | **currency direction is the other way: divide by the rate** |
| Foreign lens ZPCT2 | 16.05 (7300 = 2,334.33) | the fringe was taken on the EUR amount, not converted |
| Option B lens | 888.88 (7300 = 3,255.31) | a line with one "off" group and one "on" group still counts |
| ZFLATCHG | 2,500.00 or 2,278.00 | flat fees ignore all exclusions, or only the group exclusion |
| ZPCTCHG | 1,901.99 | the charge base leaves fringes out |
| ZPCTCHG | 2,118.90 | charges stack (the percent charge includes the flat charge) |
| ZINCENTIVE | 1,751.03 | the group's gross leaves fringes out (credit row 1 = 1,436.76) |
| Credit row 1 | 1,607.32 | the excluded fringe wasn't taken off |
| Credit row 1 | 1,670.10 | Option B lens counted toward the group |
| Credit rows 2 or 3 | 100.00 or 450.00 | that cap type didn't apply |

---

## Part 3: exports

Do these in order. If an export asks for options, take the defaults and screenshot the options
window (`shots/30-export-options-<format>.png`).

### Main exports (budget exactly as entered)

| # | What | Save as (in `Screenshots/exports/`) |
|---|---|---|
| 1 | Tab delimited | `fwbtest-tab.txt` |
| 2 | XML, basic | `fwbtest-xml-basic.xml` |
| 3 | XML, advanced | `fwbtest-xml-advanced.xml` |
| 4 | JSON | `fwbtest.json` |
| 5 | Excel (with totals at 2 decimals) | `fwbtest.xlsx` |
| 6 | Excel again with totals back to whole dollars (only if you changed that setting) | `fwbtest-wholedollars.xlsx` |
| 7 | PDF: topsheet report | `fwbtest-topsheet.pdf` |
| 8 | PDF: account details for all accounts, fringe breakdown shown, suppressed lines included | `fwbtest-details.pdf` |
| 9 | PDF: setup reports (fringes, globals, groups, units, sets, locations, currencies, credits) | `fwbtest-setup.pdf` |
| 10 | PDF: the credit print view from the credit setup screen | `fwbtest-credits.pdf` |
| 11 | PDF: notes report, if offered | `fwbtest-notes.pdf` |

Put the totals decimal setting back to 2 after export 6.

### Variations (change one thing, export, change it back)

Each one takes a couple of minutes. Change the setting, save the files, then **put it back**
before the next one. Our predictions are in Part 2b below.

| # | Change | Save as |
|---|---|---|
| V1 | Fringe posting: by budget | `fwbtest-v1-postbudget-topsheet.pdf` and `fwbtest-v1-postbudget.json` |
| V2 | Percentage fringe cutoff factor: 2.0 | `fwbtest-v2-factor2-topsheet.pdf` and `fwbtest-v2-factor2-details.pdf` |
| V3 | Totals (display) currency: EUR | `fwbtest-v3-eur-topsheet.pdf` and `fwbtest-v3-eur.json` |

When all three are back, take one last topsheet screenshot and check it matches Part 2 again.

### Part 2b: predictions for the variations

**V1, fringes posted by budget.** Categories drop their fringe accounts and one total fringes
row appears.

| Row | Ours | If the percent charge counts the fringe row |
|---|---|---|
| 7100 / 7200 / 7300 | 25,191.99 / 10,940.59 / 2,318.28 | same |
| ATL / BTL | 25,191.99 / 13,258.87 | same |
| Total fringes row | 2,360.21 | same |
| ZFLATCHG | 1,978.00 | same |
| ZPCTCHG | 1,901.99 | 2,020.00 |
| Grand total | 44,691.06 | 44,809.07 |
| Net total | 42,887.86 | 43,005.87 |

**V2, cutoff factor 2.0.** Our engine has no factor yet, so we don't know. Find the column that
matches:

| | No effect | Cutoff x 2 | Fringe x 2 | Cutoff / 2 |
|---|---|---|---|---|
| Grip range ZPCT | 277.78 | 555.56 | 555.56 | 138.89 |
| Driver ZPCT | 277.78 | 462.96 | 555.56 | 138.89 |
| ZPCT total | 555.56 | 1,018.52 | 1,111.12 | 277.78 |
| 7200 total | 12,590.04 | 13,053.00 | 13,145.60 | 12,312.26 |
| Grand total | 44,809.07 | 45,295.18 | 45,392.41 | 44,517.40 |
| Net total | 43,005.87 | 43,491.98 | 43,589.21 | 42,739.15 |

The driver line is the tie breaker: its pay (3,703.68) sits between the normal cutoff and
double the cutoff.

**V3, totals shown in EUR.** Line rates should not change, only totals.

| Total | Ours (divide by 3) | Other direction (multiply by 3) |
|---|---|---|
| ATL | 8,618.20 | 77,563.80 |
| BTL | 4,985.49 | 44,869.41 |
| Grand total | 14,936.36 | 134,427.21 |
| Credit | 601.07 | 5,409.60 |
| Net total | 14,335.29 | 129,017.61 |

A cent or two off is fine here (MMB may convert each row before adding).

---

## Part 4: screenshots

Save in `Screenshots/exports/shots/`. Plain PNGs from the screen are fine.

| # | Screen |
|---|---|
| 01 | Each preference screen before changes (defaults) |
| 02 | Each preference screen after Step 1 |
| 03 | Units setup table |
| 04 | Globals setup table |
| 05 | Fringes setup table (with the in-use and total columns if shown) |
| 06 | Groups setup table |
| 07 | Sets and locations setup tables |
| 08 | Currencies setup table |
| 10 | Topsheet |
| 11 to 15 | Each account's detail lines, one per account, all optional columns shown |
| 16 | The fringe range bracket on account 7201 |
| 17 | Each charge's setup, showing its exclusions |
| 18 | Credit setup, showing the row columns (gross, excluded fringes, net, credit) |
| 19 | The adjustment-row fringe attempt (Step 11) |
| 20 | The fringe tools pane with the Driver line selected (fringe percent and dollars) |
| 21 | Topsheet in each variation, V1 to V3 |
| 22 | Anything that surprised you |

That's it. Thank you!

---

## Part 5: what each file tells us

For the engineers. Question numbers are the open questions in
`docs/research/mmb-feature-inventory.md`.

| Question | Evidence |
|---|---|
| Q1 export schemas | All five exports of the same budget. Every value is distinctive (12345.67, 432.105, 312.34, 9999.99, Z names), so each field can be found by searching. Compare `fwbtest.xlsx` with `fwbtest-wholedollars.xlsx` to see whether Excel stores values or display-rounded text. |
| Q2 currency direction | Foreign lens line total (1,500.00 vs 166.67) in every export and the topsheet; V3 totals (divide vs multiply); how the EUR rate 3.00000 is written in XML/JSON. |
| Q3 cutoff factor | V2 topsheet and details: grip range and driver ZPCT, using the V2 table. |
| Q4 flat fringes and ranges | Grip ZFLAT per line (100, 425, 50) in the details PDF and any per-line fringe data in the advanced exports. |
| Q5 flat fringe on a non-time unit | Driver kit ZFLAT: 25.00 (once) vs 50.00 (times quantity). |
| Q6 fringes on adjustment rows | Screenshot 19, plus Contingency amount: 1,050.00 vs 1,072.47, and any fringe on it. |
| Q7 charge base, stacking, flat exclusions | ZPCTCHG (2,020.00 / 1,901.99 / 2,118.90), ZFLATCHG (1,978.00 / 2,278.00 / 2,500.00), and V1 for the post-by-budget case. |
| Q8 credit currency | Not tested (all credit amounts in USD). Possible follow-up. |
| Q9 group conflicts | Option B lens (ZOPTB off plus ZCREDIT on): 0.00 vs 888.88. |
| Q10 suppressed lines and cutoffs | Not tested directly; a follow-up could put a suppressed line inside the grip range. |
| Q11 rounding points | Polish line (1,296.315 to 1,296.32); grip ZPCT split; credit rows summed unrounded (1,492.32 has no rounding, the 2% discount on 1,840 is exact). Excel and XML show whether line totals are stored rounded. |
| Q13 globals with units | Not tested (no global has a unit). |
| Q14 units on X, Rate, 4X | Walkie batteries: Unit2 Zweek must not trigger anything; check which unit columns each export carries. |
| Q15 subtotal rows | Producer subtotal row (10,000.00): whether exports list it as its own row type, and that it adds nothing. |
| Formulas vs values | Polish (`ZWEEKS`), grip lines (`ZDAYS`, `ZRATE`): whether exports carry the formula text, the value, or both. |
| Fringe ranges in exports | Whether the grip range membership appears anywhere. |
| Title lines | ZED WRITER DEAL: how a line with no numbers is exported. |
| Negative values | Fuel refund -77.77: sign or parentheses in each format. |
| Suppression flag | ZOPTB's in-budget-total switch and whether suppressed lines appear in each export. |
| Sets, locations, groups | Grip lines and Walkie batteries; location fields (Testville, Test Region, Testland). |
| Production totals, charges, credits | Whether each export includes topsheet rows beyond categories (ZTOTAL ATL/BTL, 7801, 7802, 7901). |
| Notes | Search each export for `4242` and `4343`. The manual says notes don't export; this confirms it. |
| Budget info | Search for `ZTEST PRODUCTION` and `hello 5150`. |
| Fringe posting in exports | Main exports (by category) vs `fwbtest-v1-postbudget.json`: how fringe accounts or the fringe row are represented, and what number MMB gives the generated fringe accounts. |
| Account coding defaults | Screenshot 01. |
