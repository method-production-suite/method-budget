# Field Station

The suite's one invented production (SUITE-STANDARDS.md, section 7): the script, its
schedule and its budget, so every app's pictures tell one story. Every name and figure is
made up. The README and site pictures are taken from these files
(`cargo xtask capture-screens`, docs/WEBSITE.md, "Screenshots").

| File | What it is | Who owns it |
|---|---|---|
| `field-station.budget.json` | The schedule's shoot days (format `method-schedule-days`, version 1): four shoot days, Monday 3 to Thursday 6 May 2027, the two locations, and each cast member's and element's days | **Method Schedule.** A copy of its `fixtures/field-station.budget.json`. Change it there and copy it here; never edit this copy |
| `field-station.mthd` | The budget for that schedule: a small independent feature on the low-budget feature chart | Made by `scripts/make-field-station.py` from the file above |
| `camera-department.csv` | The camera department's rental list, the spreadsheet the Import Spreadsheet picture reviews | Made by the same script |

Made, never edited by hand: `python3 scripts/make-field-station.py` writes the budget and
the spreadsheet again, and `--check` says whether they are what it would write (CI runs it).
`crates/method-format/tests/field_station.rs` checks the budget is in canonical form and
agrees with the schedule, read with Budget's own reader.
