# Field Station

The suite's one invented production (SUITE-STANDARDS.md, sections 7 and 8): the script,
its schedule and its budget, so every app's pictures tell one story and every app opens
the same production as its sample. Every name and figure is made up. The README and site
pictures are taken from these files (`cargo xtask capture-screens`, docs/WEBSITE.md,
"Screenshots"), the app opens the budget as its sample on first launch and from Help >
Open Sample (docs/APP.md, "The sample budget"), and the browser demo starts with it.

| File | What it is | Who owns it |
|---|---|---|
| `field-station.budget.next.json` | **A stand-in.** The schedule's shoot days in the shape agreed with Method Schedule (format `method-schedule-days`, version 1): 18 shoot days, Monday 3 to Wednesday 26 May 2027 (weeks of 5, 5, 5 and 3), Cold Spring field station 12 days, Hollins Ridge fire road 3, Kettle Lake boathouse 3, the cast DELACROIX, MARA, TEO and IVERSEN and four day players (RANGER, BOATHOUSE KEEPER, SEARCH PILOT, PARK DISPATCHER), background on 6 days, the Main Unit's 18 days and a one-day Second Unit | **Written by hand here** until Method Schedule's own export of its 18-day sample lands. That export replaces it: copy it here as `field-station.budget.json`, delete this file, point `SCHEDULE` in the script at it, and run the script |
| `field-station.budget.json` | Method Schedule's schedule days file of today: four shoot days, Monday 3 to Thursday 6 May 2027, two locations | **Method Schedule.** A copy of its `fixtures/field-station.budget.json`. Change it there and copy it here; never edit this copy. The script still builds a (smaller) budget from it, and `--check` makes sure it does |
| `field-station.mthd` | The budget for the 18-day schedule: an independent feature of about a million dollars on the low-budget feature chart | Made by `scripts/make-field-station.py` from the stand-in |
| `camera-department.csv` | The camera department's rental list, the spreadsheet the Import Spreadsheet picture reviews | Made by the same script |

Made, never edited by hand: `python3 scripts/make-field-station.py` writes the budget and
the spreadsheet again, and `--check` says whether they are what it would write (CI runs it).
Every day count in the budget comes from the schedule days file through
`method-budget schedule-days --apply`, as the real hand-off does: the shoot days, each
location's days, the second unit's days, each cast member's and the background's days,
and the effects, wardrobe, set dressing, prop and vehicle days the lines use. A figure the
file doesn't have is set to 0 and its global says "Not in this schedule".

`crates/method-format/tests/field_station.rs` checks the budget is in canonical form,
agrees with the stand-in (read with Budget's own reader) and shows every feature group;
`crates/method-session/src/schedule_days.rs` checks a second import of the stand-in finds
every global by its link and changes nothing. The command line's and the MCP tool's
schedule-days tests use the four-day file on a starter budget, so they keep reading Method
Schedule's own copy.
