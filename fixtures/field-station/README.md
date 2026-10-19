# Field Station

The suite's one invented production (SUITE-STANDARDS.md, sections 7 and 8): the script,
its schedule and its budget, so every app's pictures tell one story and every app opens
the same production as its sample. Every name and figure is made up. The README and site
pictures are taken from these files (`cargo xtask capture-screens`, docs/WEBSITE.md,
"Screenshots"), the app opens the budget as its sample on first launch and from Help >
Open Sample (docs/APP.md, "The sample budget"), and the browser demo starts with it.

| File | What it is | Who owns it |
|---|---|---|
| `field-station.budget.json` | The schedule's shoot days (format `method-schedule-days`, version 1, `days_hash` `e9245ad76e787b1e`): 18 shoot days, Monday 3 to Wednesday 26 May 2027 (weeks of 5, 5, 5 and 3), prep from 5 April, wrap to 9 June; Cold Spring field station 12 days, Hollins Ridge fire road 3, Kettle Lake boathouse 3; the Main Unit's 18 days and a one-day Second Unit; the cast DELACROIX, MARA, TEO, IVERSEN, RANGER, DISPATCHER, PILOT, BOATHOUSE KEEPER and ANSEL (silent); background, stunts, vehicles, effects and the rest of the breakdown | **Method Schedule.** A copy of its `fixtures/field-station.budget.json` on its `main` at commit `d1616afd4c7768d729c7a4f02bdd4aa7528486ea` (its pull request #29, merged). Change it there, copy it here and run the script; never edit this copy |
| `field-station.mthd` | The budget for that schedule: an independent feature of about a million dollars on the low-budget feature chart | Made by `scripts/make-field-station.py` from the file above |
| `camera-department.csv` | The camera department's rental list, the spreadsheet the Import Spreadsheet picture reviews | Made by the same script |

Made, never edited by hand: `python3 scripts/make-field-station.py` writes the budget and
the spreadsheet again, and `--check` says whether they are what it would write (CI runs it).
Every day count in the budget comes from the schedule days file through
`method-budget schedule-days --apply`, as the real hand-off does: the shoot days, each
location's days, the second unit's days, each cast member's, stunt's and background
group's days, and the effects, wardrobe, set dressing, prop, vehicle, boat, marine and
animal days the lines use (an element's days are its day out of days total, holds
included). How many people each background group is, and the weeks of prep and wrap the
crew is paid for, are the budget's own assumptions. A figure the file doesn't have is set
to 0 and its global says "Not in this schedule".

`crates/method-format/tests/field_station.rs` checks the budget is in canonical form,
agrees with the schedule (read with Budget's own reader) and shows every feature group;
`crates/method-session/src/schedule_days.rs` checks a second import of the file finds
every global by its link and changes nothing. The command line's, the MCP tool's and the
browser host's schedule-days tests apply the same file to a starter budget.
