#!/usr/bin/env python3
"""Make the Field Station budget: the app's sample and the README and site pictures.

    python3 scripts/make-field-station.py            write the fixtures
    python3 scripts/make-field-station.py --check    make them again in a scratch folder;
                                                     exit 1 unless every byte is the same
    python3 scripts/make-field-station.py --days FILE --out DIR
                                                     build from another schedule days file
                                                     into DIR (to try one; never checked)

Field Station is the suite's one invented production (SUITE-STANDARDS.md, sections 7 and
8): Method Schedule breaks the script down and writes its shoot days as a schedule days
file, and Method Budget budgets that schedule. This script reads that file and writes,
beside it in fixtures/field-station/:

  field-station.mthd         an independent feature of about a million dollars on the
                             low-budget feature chart: globals (every day count set from
                             the schedule by `method-budget schedule-days --apply`),
                             percent fringes with cutoffs and a flat hourly one, one
                             person paid in two accounts sharing a cutoff, groups (one
                             left out of the total), locations and sets, lines in a
                             second currency, two charges (contingency and a completion
                             bond), an incentive program limited to one location and not
                             applied (an estimate), dated notes, the original totals
                             (draft 2, frozen), draft 2 kept as a version for Compare,
                             and actuals through the second shoot week for a cost report.
                             The app bundles it and opens it as its sample.
  camera-department.csv      the camera department's rental list, the spreadsheet the
                             Import Spreadsheet picture reviews

The schedule days file is `field-station.budget.next.json`, a stand-in with the 18-day
shape agreed with Method Schedule, until Method Schedule's own export replaces it
(README.md in that folder). The script also works with the older four-day file,
`field-station.budget.json`: a figure that file doesn't count (the boathouse, the second
unit, the day players) is set to 0 and said so, so the budget is smaller but whole;
`--check` builds that one too and checks it.

Every name and figure is made up: no real production, person or company, and no credit
line. The budget starts from a bundled chart (`method-budget new --template`), gets its
day counts from the schedule (`method-budget schedule-days`), and is filled through
`method-budget mcp`, the same typed tools a coding agent uses (the client is the one in
make-showcase-budgets.py), so the file is exactly what the engine writes. What no tool
writes (the budget information dates, dated notes with fixed dates, the version, the
original totals worked out from draft 2's figures, and the actuals) is added in the shape
docs/FORMAT.md documents, then the file is checked and rewritten in canonical form with
`method-budget fmt`.

Uses target/release/method-budget (METHOD=... for another build). Python 3.9+, standard
library only.
"""

import datetime
import filecmp
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
FIXTURES = REPO / "fixtures" / "field-station"
# The schedule days file the sample is built from: the stand-in until Method Schedule's
# 18-day export lands (then it becomes field-station.budget.json again).
SCHEDULE = FIXTURES / "field-station.budget.next.json"
# Method Schedule's four-day file of today, which the script must keep working with.
OLD_SCHEDULE = FIXTURES / "field-station.budget.json"
BUDGET = "field-station.mthd"
SHEET = "camera-department.csv"
DRAFT = "field-station-draft-2.mthd"

_spec = importlib.util.spec_from_file_location(
    "showcase", Path(__file__).resolve().parent / "make-showcase-budgets.py")
showcase = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(showcase)
METHOD = showcase.METHOD
method = showcase.method

# ---------------------------------------------------------------------------
# The schedule's figures

STATION = "Cold Spring field station"
RIDGE = "Hollins Ridge fire road"
LAKE = "Kettle Lake boathouse"

# Each figure the budget takes from the schedule, and the global it goes to. A figure is
# `(kind, name)`; an element's name is `(category, name)`, looked up in the file for its
# id. Shoot days in all always go to SHOOT_DAYS (decisions.md, entry 76).
FIGURES = [
    (("location", STATION), "STATION_DAYS"),
    (("location", RIDGE), "RIDGE_DAYS"),
    (("location", LAKE), "LAKE_DAYS"),
    (("unit", "Second Unit"), "SECOND_UNIT_DAYS"),
    (("element", ("Cast", "DELACROIX")), "DELACROIX_DAYS"),
    (("element", ("Cast", "MARA")), "MARA_DAYS"),
    (("element", ("Cast", "TEO")), "TEO_DAYS"),
    (("element", ("Cast", "IVERSEN")), "IVERSEN_DAYS"),
    (("element", ("Cast", "RANGER")), "RANGER_DAYS"),
    (("element", ("Cast", "BOATHOUSE KEEPER")), "KEEPER_DAYS"),
    (("element", ("Cast", "SEARCH PILOT")), "PILOT_DAYS"),
    (("element", ("Cast", "PARK DISPATCHER")), "DISPATCHER_DAYS"),
    (("element", ("Background", "SEARCH VOLUNTEERS")), "BACKGROUND_DAYS"),
    (("element", ("Practical effects", "RAIN")), "RAIN_DAYS"),
    (("element", ("Practical effects", "WIND")), "WIND_DAYS"),
    (("element", ("Practical effects", "FOG")), "FOG_DAYS"),
    (("element", ("Wardrobe", "WEATHER COATS")), "COATS_DAYS"),
    (("element", ("Set dressing", "STATION RADIO SET")), "RADIO_SET_DAYS"),
    (("element", ("Set dressing", "GENERATOR")), "GENERATOR_DAYS"),
    (("element", ("Set dressing", "MAST LADDER")), "LADDER_DAYS"),
    (("element", ("Hand props", "HAND RADIO")), "HAND_RADIO_DAYS"),
    (("element", ("Vehicles", "PATROL TRUCK")), "TRUCK_DAYS"),
    (("element", ("Vehicles", "ROWBOAT")), "ROWBOAT_DAYS"),
]


def schedule(path):
    s = json.loads(path.read_text())
    if s.get("format") != "method-schedule-days" or s.get("version") != 1:
        sys.exit(f"make-field-station: {path} isn't a schedule days file, version 1")
    days = s["shoot_days"]
    ids = {(e["category"], e["name"]): e["id"] for e in s["elements"]}
    picks, missing = [], []
    for (kind, name), glob in FIGURES:
        if kind == "element":
            key = f"element:{ids[name]}" if name in ids else None
            label = f"{name[0]} {name[1]}"
        else:
            have = days.get(f"by_{kind}", {})
            key = f"{kind}:{name}" if name in have else None
            label = f"{kind} {name}"
        if key:
            picks.append(f"{key}={glob}")
        else:
            missing.append((glob, label))
    return {
        "title": s["schedule"],
        "start": days["start"],
        "end": days["end"],
        "total": days["total"],
        "picks": picks,
        "missing": missing,
    }


# ---------------------------------------------------------------------------
# Setup. Rates and cutoffs are plausible, not any union's or state's.

FRINGES = [
    ("FICA", {"rate": "7.65", "cutoff": "176,100.00",
              "description": "Social security and medicare"}),
    ("SUI", {"rate": "3.4", "cutoff": "7,000.00", "description": "State unemployment"}),
    ("WC", {"rate": "2.85", "description": "Workers' compensation"}),
    ("CAST_PH", {"rate": "17.5", "cutoff": "225,000.00",
                 "description": "Performers' pension and health"}),
    ("CREW_HW", {"fringe_kind": "flat", "rate": "4.85", "unit": "Hour",
                 "basis": "amount_counts", "max_hours_per_day": "12",
                 "description": "Crew health, per hour worked"}),
]
TAX = ["FICA", "SUI", "WC"]
F = {"none": [], "tax": TAX, "cast": TAX + ["CAST_PH"], "crew": TAX + ["CREW_HW"]}

GROUPS = [
    ("Second unit", {"description": "The second unit's day on the fire road",
                     "color": "#2f6f8f"}),
    ("Aerial option", {"description": "Drone aerials over the lake, priced but not "
                       "in the budget until the financiers agree",
                       "in_budget_total": False, "color": "#8a5a00"}),
]
LOCATIONS = [
    (STATION, {"description": "Interior and exterior, the station and its yard",
               "region": "Cold Spring County"}),
    (RIDGE, {"description": "Exterior, the ridge road and the radio mast",
             "region": "Cold Spring County"}),
    (LAKE, {"description": "Exterior and interior, the boathouse, dock and shore",
            "region": "Kettle Lake"}),
]
SETS = [
    ("FIELD STATION", "The station's main room and radio desk"),
    ("LOWER STATION", "The bunk room and stores below"),
    ("RADIO MAST", "The mast above the station"),
    ("RIDGE", "The ridge top, night and day"),
    ("RIDGE ROAD", "The fire road up the ridge"),
    ("BOATHOUSE", "The boathouse and its dock"),
    ("LAKE SHORE", "The shore below the boathouse"),
]
EUR = {"rate": "1.08", "description": "Euro", "symbol": "€", "symbol_after": True,
       "separators": "point_comma"}

ALL_WKS = "PREP_WKS + SHOOT_WKS + WRAP_WKS"
DESIGNER = "Production designer"


def plan():
    """The globals the schedule doesn't set and every account's lines."""
    globals_ = [
        ("SHOOT_WKS", "SHOOT_DAYS / 5", "Shooting weeks, five-day weeks"),
        ("PREP_WKS", "5", "Weeks of prep"),
        ("WRAP_WKS", "2", "Weeks of wrap"),
        ("CREW_MEALS", "36", "People fed each shooting day"),
        ("VOLUNTEERS", "12", "Search volunteers in the background each of their days"),
    ]
    st = {"location": STATION}
    rd = {"location": RIDGE}
    lk = {"location": LAKE}
    su = {"groups": ["Second unit"], "location": RIDGE}
    air = {"groups": ["Aerial option"], "location": LAKE}
    eur = {"currency": "EUR"}

    # A line: (description, amount, unit, x, rate, fringe set[, extra fields]).
    lines = {
        "10-01": [("Story rights, original screenplay", 1, "Flat", 1, 8000, "none")],
        "10-02": [("Screenplay fee", 1, "Flat", 1, 20000, "tax"),
                  ("Production polish", 1, "Flat", 1, 3500, "tax")],
        "10-03": [("Script printing, table read and research", 1, "Allow", 1, 1200, "none")],
        "11-01": [("Producer fee", 1, "Flat", 1, 12000, "none"),
                  ("Producer fee, second producer", 1, "Flat", 1, 8000, "none")],
        "11-02": [("Associate producer", ALL_WKS, "Week", 1, 800, "crew")],
        "11-03": [("Producers' travel and expenses", 1, "Allow", 1, 2000, "none")],
        "12-01": [("Director fee", 1, "Flat", 1, 22000, "tax")],
        "12-02": [("Director's travel and expenses", 1, "Allow", 1, 1500, "none")],
        "13-01": [("DELACROIX", "DELACROIX_DAYS", "Day", 1, 1650, "cast"),
                  ("MARA", "MARA_DAYS", "Day", 1, 1650, "cast"),
                  ("TEO", "TEO_DAYS", "Day", 1, 1300, "cast"),
                  ("Rehearsal days, three leads", 4, "Day", 3, 450, "cast")],
        "13-02": [("IVERSEN, radio voice", "IVERSEN_DAYS", "Day", 1, 1000, "cast"),
                  ("RANGER, day player", "RANGER_DAYS", "Day", 1, 900, "cast", rd),
                  ("BOATHOUSE KEEPER, day player", "KEEPER_DAYS", "Day", 1, 900, "cast", lk),
                  ("SEARCH PILOT, day player", "PILOT_DAYS", "Day", 1, 900, "cast", su),
                  ("PARK DISPATCHER, day player", "DISPATCHER_DAYS", "Day", 1, 900, "cast"),
                  ("Background, search volunteers", "BACKGROUND_DAYS", "Day", "VOLUNTEERS", 180, "cast"),
                  ("Overtime and adjustments", 1, "Allow", 1, 2500, "cast")],
        "13-03": [("Casting director, package", 1, "Flat", 1, 7500, "none"),
                  ("Background casting", 1, "Flat", 1, 1500, "none")],
        "13-04": [("Cast travel and lodging, Cold Spring", "STATION_DAYS", "Day", 3, 150, "none", st),
                  ("Cast lodging, Kettle Lake", "LAKE_DAYS", "Day", 3, 150, "none", lk)],
        "20-01": [("Line producer", ALL_WKS, "Week", 1, 1450, "crew")],
        "20-02": [("First assistant director", "2 + SHOOT_WKS", "Week", 1, 1800, "crew")],
        "20-03": [("Second assistant director", "SHOOT_DAYS + 3", "Day", 1, 340, "crew")],
        "20-04": [("Production coordinator", "PREP_WKS + SHOOT_WKS + 1", "Week", 1, 950, "crew")],
        "20-05": [("Script supervisor", "SHOOT_DAYS + 2", "Day", 1, 400, "crew")],
        "20-06": [("Production assistants", "SHOOT_DAYS", "Day", 3, 200, "crew")],
        "20-07": [("Production accountant", "PREP_WKS + SHOOT_WKS + 1", "Week", 1, 950, "crew")],
        "21-01": [("Director of photography", "SHOOT_DAYS + 6", "Day", 1, 650, "crew"),
                  ("Second unit director of photography", "SECOND_UNIT_DAYS", "Day", 1, 900, "crew", su)],
        "21-02": [("First assistant camera", "SHOOT_DAYS + 2", "Day", 1, 450, "crew"),
                  ("Second assistant camera", "SHOOT_DAYS + 1", "Day", 1, 360, "crew"),
                  ("Second unit camera assistant", "SECOND_UNIT_DAYS", "Day", 1, 420, "crew", su)],
        "21-03": [("Digital imaging technician", "SHOOT_DAYS", "Day", 1, 400, "crew")],
        "21-04": [("Camera package, rental house estimate", "SHOOT_DAYS", "Day", 1, 1050, "none"),
                  ("Second unit camera package", "SECOND_UNIT_DAYS", "Day", 1, 1100, "none", su),
                  ("Drone and pilot, lake aerials", 2, "Day", 1, 3800, "none", air)],
        "21-05": [("Expendables and batteries", 1, "Allow", 1, 1500, "none")],
        "22-01": [("Gaffer", "SHOOT_DAYS + 2", "Day", 1, 480, "crew")],
        "22-02": [("Key grip", "SHOOT_DAYS + 2", "Day", 1, 480, "crew")],
        "22-03": [("Grip and electric crew", "SHOOT_DAYS", "Day", 3, 380, "crew")],
        "22-04": [("Lighting and grip package", "SHOOT_DAYS", "Day", 1, 1050, "none"),
                  ("Tow generator and operator", "STATION_DAYS", "Day", 1, 380, "none", st)],
        "22-05": [("Expendables", 1, "Allow", 1, 2200, "none")],
        "23-01": [("Production sound mixer", "SHOOT_DAYS + 1", "Day", 1, 520, "crew")],
        "23-02": [("Boom operator", "SHOOT_DAYS", "Day", 1, 380, "crew")],
        "23-03": [("Sound package", "SHOOT_DAYS", "Day", 1, 300, "none")],
        "24-01": [("Production designer", "PREP_WKS + SHOOT_WKS", "Week", 1, 1400, "crew",
                   {"person": DESIGNER})],
        "24-02": [("Art director", "PREP_WKS + SHOOT_WKS", "Week", 1, 1000, "crew"),
                  ("Set dresser", "SHOOT_DAYS + 4", "Day", 1, 320, "crew")],
        "24-03": [("Station radio set, rental", "RADIO_SET_DAYS", "Day", 1, 95, "none",
                   {"set": "FIELD STATION", "location": STATION}),
                  ("Generator, non-working prop", "GENERATOR_DAYS", "Day", 1, 60, "none"),
                  ("Mast ladder, rental", "LADDER_DAYS", "Day", 1, 140, "none",
                   {"set": "RADIO MAST", "location": STATION}),
                  ("Hand radios, pair", "HAND_RADIO_DAYS", "Day", 2, 35, "none"),
                  ("Camp stove, maps, packs and lamps", 1, "Allow", 1, 900, "none"),
                  ("Tea mugs, notebook, key ring and torch", 1, "Allow", 1, 300, "none"),
                  ("Set dressing purchases", 1, "Allow", 1, 4500, "none")],
        "24-04": [("Station interior, build, dress and strike", 1, "Allow", 1, 12000, "none",
                   {"set": "FIELD STATION", "location": STATION}),
                  ("Boathouse dock repair and dressing", 1, "Allow", 1, 5000, "none",
                   {"set": "BOATHOUSE", "location": LAKE}),
                  ("Production designer, construction supervision", 3, "Week", 1, 1400, "crew",
                   {"person": DESIGNER})],
        "24-05": [("Rain, towers and water truck", "RAIN_DAYS", "Day", 1, 1800, "none"),
                  ("Wind, fans and operator", "WIND_DAYS", "Day", 1, 550, "none"),
                  ("Fog on the lake", "FOG_DAYS", "Day", 1, 700, "none", lk),
                  ("Effects technician", "RAIN_DAYS + WIND_DAYS + FOG_DAYS", "Day", 1, 450, "crew")],
        "25-01": [("Costume designer", "PREP_WKS + SHOOT_WKS", "Week", 1, 1050, "crew")],
        "25-02": [("Wardrobe assistant", "SHOOT_DAYS + 2", "Day", 1, 260, "crew")],
        "25-03": [("Weather coats, three, with doubles", 1, "Flat", 6, 180, "none"),
                  ("Weather coats, cleaning on wet days", "COATS_DAYS", "Day", 1, 45, "none"),
                  ("Wardrobe purchases, rentals and alterations", 1, "Allow", 1, 4000, "none")],
        "25-04": [("Makeup and hair artists", "SHOOT_DAYS", "Day", 2, 380, "crew")],
        "25-05": [("Makeup and hair supplies", 1, "Allow", 1, 1400, "none")],
        "26-01": [("Location manager", ALL_WKS, "Week", 1, 1100, "crew"),
                  ("Assistant location manager", "SHOOT_DAYS + 5", "Day", 1, 280, "crew")],
        "26-02": [("Site fee, Cold Spring field station", "STATION_DAYS", "Day", 1, 1250, "none", st),
                  ("Site fee, Hollins Ridge fire road", "RIDGE_DAYS", "Day", 1, 450, "none", rd),
                  ("Site fee, Kettle Lake boathouse", "LAKE_DAYS", "Day", 1, 1200, "none", lk)],
        "26-03": [("Permits and certificates", 1, "Allow", 1, 2800, "none")],
        "26-04": [("Heaters, tents and toilets, field station", "STATION_DAYS", "Day", 1, 300, "none", st),
                  ("Base camp and parking, fire road", "RIDGE_DAYS", "Day", 1, 260, "none", rd),
                  ("Base camp and parking, boathouse", "LAKE_DAYS", "Day", 1, 260, "none", lk),
                  ("Overnight security", "SHOOT_DAYS", "Day", 1, 260, "tax")],
        "27-01": [("Production vans", "SHOOT_DAYS + 4", "Day", 2, 150, "none"),
                  ("Fire road shuttle, four-wheel drive", "RIDGE_DAYS", "Day", 2, 220, "none", rd),
                  ("Patrol truck, picture vehicle", "TRUCK_DAYS", "Day", 1, 300, "none", rd),
                  ("Rowboat and safety boat", "ROWBOAT_DAYS", "Day", 1, 400, "none", lk),
                  ("Driver", "SHOOT_DAYS + 2", "Day", 1, 300, "crew")],
        "27-02": [("Fuel and parking", 1, "Allow", 1, 3200, "none")],
        "28-01": [("Catering, field station days", "STATION_DAYS", "Day", "CREW_MEALS", 28, "none", st),
                  ("Catering, fire road days", "RIDGE_DAYS", "Day", "CREW_MEALS + 4", 30, "none", rd),
                  ("Catering, boathouse days", "LAKE_DAYS", "Day", "CREW_MEALS", 28, "none", lk),
                  ("Background meals", "BACKGROUND_DAYS", "Day", "VOLUNTEERS", 15, "none")],
        "28-02": [("Craft service", "SHOOT_DAYS", "Day", 1, 240, "none")],
        "29-01": [("Production office and supplies", 3, "Month", 1, 1100, "none")],
        "29-02": [("Walkie-talkie rental", "SHOOT_DAYS", "Day", 1, 120, "none"),
                  ("Phones and internet", 3, "Month", 1, 250, "none")],
        "29-03": [("Shipping and courier", 1, "Allow", 1, 600, "none")],
        "30-01": [("Drives and media", 1, "Allow", 1, 3000, "none")],
        "30-02": [("Backup storage, twelve months", 12, "Month", 1, 60, "none")],
        "40-01": [("Editor", 12, "Week", 1, 1700, "tax")],
        "40-02": [("Assistant editor", 8, "Week", 1, 900, "tax")],
        "40-03": [("Edit system", 5, "Month", 1, 400, "none")],
        "41-01": [("Sound design and editing", 1, "Flat", 1, 12000, "none")],
        "41-02": [("Mix", 1, "Flat", 1, 9500, "none")],
        "41-03": [("ADR and foley", 1, "Allow", 1, 5000, "none")],
        "42-01": [("Composer, package", 1, "Flat", 1, 13000, "none", eur),
                  ("Recording session, string quartet", 1, "Flat", 1, 3200, "none", eur)],
        "42-02": [("Music licensing", 1, "Allow", 1, 5000, "none")],
        "43-01": [("Visual effects, cleanup and sky", 1, "Allow", 1, 12000, "none")],
        "43-02": [("Titles and graphics", 1, "Flat", 1, 2500, "none")],
        "44-01": [("Color grading", 1, "Flat", 1, 9500, "none")],
        "44-02": [("Conform and online", 1, "Flat", 1, 2500, "none")],
        "45-01": [("Masters", 1, "Allow", 1, 3500, "none")],
        "45-02": [("Digital cinema package", 1, "Flat", 1, 1800, "none")],
        "45-03": [("Closed captions and subtitles", 1, "Flat", 1, 1400, "none")],
        "50-01": [("Production insurance", 1, "Allow", 1, 12000, "none")],
        "50-02": [("Errors and omissions", 1, "Allow", 1, 5000, "none")],
        "51-01": [("Production legal", 1, "Allow", 1, 11000, "none")],
        "51-02": [("Payroll service fees", 1, "Allow", 1, 4000, "none"),
                  ("Cost report and audit", 1, "Allow", 1, 3500, "none")],
        "52-01": [("Festival entry fees", 15, "Flat", 1, 90, "none")],
        "52-02": [("Poster, trailer and press kit", 1, "Allow", 1, 5000, "none")],
        "53-01": [("Bank fees", 1, "Allow", 1, 500, "none")],
        "53-02": [("Miscellaneous", 1, "Allow", 1, 2000, "none")],
    }
    return globals_, lines


def line(desc, amount, unit, x, rate, fr, extra=None):
    row = {"description": desc, "amount": str(amount), "unit": unit, "x": str(x),
           "rate": str(rate), "fringes": F[fr]}
    row.update(extra or {})
    return row


# Draft 2, the draft the financiers approved: draft 3 (the budget) is these changes
# applied to it. The second unit and the lake fog were added after; the camera package
# came in lower than estimated; the composer's package and visual effects moved.
DRAFT2_GLOBALS = {"CREW_MEALS": "40", "PREP_WKS": "6"}
DRAFT2_LINES = [
    ("21-04", "Camera package, rental house estimate", "rate", "1200"),
    ("42-01", "Composer, package", "rate", "11500"),
    ("43-01", "Visual effects, cleanup and sky", "rate", "16000"),
    ("24-04", "Station interior, build, dress and strike", "rate", "10500"),
]
DRAFT2_DELETE = [("21-01", "Second unit director of photography"),
                 ("21-02", "Second unit camera assistant"),
                 ("21-04", "Second unit camera package"),
                 ("24-05", "Fog on the lake")]

# Dated notes on a few lines: (account, line, [(created, author, text, done)]). Authors
# are roles, never names.
NOTES = [
    ("13-01", "DELACROIX", [
        ("2027-02-11T10:20:00-08:00", "Line producer",
         "Day rate agreed with the agent; holds are paid at the full rate.", False)]),
    ("21-04", "Camera package, rental house estimate", [
        ("2027-02-24T15:05:00-08:00", "Line producer",
         "Second quote came in lower; this is the rental house's weekly deal.", True),
        ("2027-04-09T09:40:00-07:00", "Production accountant",
         "Confirm the insurance certificate before pickup.", False)]),
    ("24-04", "Station interior, build, dress and strike", [
        ("2027-03-30T16:45:00-07:00", "Production designer",
         "Includes the strike and putting the station back as we found it.", False)]),
    ("26-02", "Site fee, Cold Spring field station", [
        ("2027-04-02T11:00:00-07:00", "Location manager",
         "Twelve days at the station, two of them nights; the fee is the same.", False)]),
]

# The incentive program: an invented county program for spend at the field station,
# worked out but not applied (an estimate; decisions.md, entry 80).
PROGRAM = "Cold Spring County film program"
PROGRAM_ROWS = [("24", "Art department at the station", 20),
                ("26", "Locations at the station", 25),
                ("28", "Catering at the station", 25)]

# Actual costs through the second shoot week: (account, share of its budget, payee,
# description). Every payee is invented.
ACTUALS = [
    ("10-01", 1.0, "Writer's payroll", "Story rights, purchase price"),
    ("10-02", 0.8, "Writer's payroll", "Screenplay fee, first steps"),
    ("11-01", 0.5, "Producers' payroll", "Producer fees, first installments"),
    ("12-01", 0.6, "Director's payroll", "Director fee, prep and two weeks"),
    ("13-01", 0.42, "Cast payroll", "Principal cast, weeks one and two"),
    ("13-03", 1.0, "Northfield Casting", "Casting package"),
    ("20-01", 0.6, "Crew payroll", "Prep and weeks one and two"),
    ("20-04", 0.62, "Crew payroll", "Prep and weeks one and two"),
    ("21-04", 0.5, "Larkfield Camera Rentals", "Camera package, weeks one and two"),
    ("22-04", 0.52, "Tamarack Grip and Lighting", "Lighting and grip, weeks one and two"),
    ("24-04", 1.08, "Birchwood Lumber", "Lumber and hardware, station interior"),
    ("24-03", 0.7, "Various", "Set dressing purchases, petty cash"),
    ("25-03", 0.85, "Various", "Wardrobe purchases"),
    ("26-02", 0.45, "Cold Spring Field Station Trust", "Site fees, first weeks"),
    ("26-03", 1.0, "Cold Spring County", "Film permit"),
    ("27-01", 0.4, "Fernhill Vehicle Rental", "Vans and shuttle, weeks one and two"),
    ("28-01", 0.48, "Hearthstone Catering", "Catering, weeks one and two"),
    ("29-01", 0.67, "Mill Street Properties", "Office rent, two months"),
    ("50-01", 1.0, "Insurance broker", "Production package premium"),
    ("51-01", 0.4, "Production counsel", "Legal, prep"),
]

# ---------------------------------------------------------------------------
# The camera department's rental list: a title row, then the header and the lines, each
# with the department's own total, which the import checks every figure against.


def camera_sheet(shoot_days):
    rows = [("21-04", "Camera body", shoot_days, "Days", 1, 450),
            ("21-04", "Prime lens set, five lenses", shoot_days, "Days", 1, 325),
            ("21-04", "Zoom lens", shoot_days, "Days", 1, 150),
            ("21-04", "Wireless focus and monitor", shoot_days, "Days", 1, 180),
            ("21-04", "Tripod, head and sticks", shoot_days, "Days", 1, 65),
            ("21-04", "Batteries and charging", shoot_days, "Days", 2, 20),
            ("21-05", "Filters and diffusion", 1, "Allow", 1, 350),
            ("21-05", "Lens tissue and camera tape", 1, "Allow", 1, 120)]
    out = [["Field Station, camera department rental list", "", "", "", "", "", ""],
           ["Acct #", "Description", "Qty", "Unit", "X", "Rate", "Total"]]
    for code, desc, qty, unit, x, rate in rows:
        out.append([code, desc, str(qty), unit, str(x), f"{rate:,.2f}",
                    f"{qty * x * rate:,.2f}"])
    return out


def csv_text(rows):
    def cell(v):
        return f'"{v}"' if any(c in v for c in ',"\n') else v
    return "".join(",".join(cell(v) for v in r) + "\n" for r in rows)


# ---------------------------------------------------------------------------


def find_row(m, path, account, desc):
    for r in m.call("get_account", path=path, account=account)["rows"]:
        if r.get("description") == desc:
            return r["id"]
    sys.exit(f"make-field-station: no line {desc!r} in {account}")


def build(out, days_file):
    out.mkdir(parents=True, exist_ok=True)
    s = schedule(days_file)
    globals_, lines = plan()
    path = out / BUDGET
    for f in (path, out / DRAFT):
        if f.exists():
            f.unlink()
    method("new", str(path), "--template", "low-budget-feature")
    # Every day count from the schedule, as the hand-off does it (decisions.md, 76).
    picks = [a for p in s["picks"] for a in ("--pick", p)]
    method("schedule-days", str(path), str(days_file), *picks, "--apply")

    m = showcase.Mcp(out)
    p = BUDGET
    m.call("set_title", path=p, title=s["title"])
    for glob, label in s["missing"]:
        m.call("set_global", path=p, name=glob, formula="0",
               description=f"Not in this schedule: {label}")
    for name, formula, desc in globals_:
        m.call("set_global", path=p, name=name, formula=formula, description=desc)
    for name, fields in FRINGES:
        m.call("add_setup", path=p, type="fringe", name=name, fields=fields)
    for name, fields in GROUPS:
        m.call("add_setup", path=p, type="group", name=name, fields=fields)
    for name, fields in LOCATIONS:
        m.call("add_setup", path=p, type="location", name=name, fields=fields)
    for name, desc in SETS:
        m.call("add_setup", path=p, type="set", name=name, fields={"description": desc})
    m.call("add_setup", path=p, type="currency", name="EUR", fields=EUR)
    m.call("add_account", path=p, category="24", code="24-05", name="Practical effects")
    for account, rows in lines.items():
        m.call("add_lines", path=p, account=account, use_defaults=False,
               lines=[line(*r) for r in rows])
    # The contingency and a completion bond are charges on the topsheet. The chart's
    # Contingency category would only ever show 0.00 beside it, so it goes.
    m.call("set_charge", path=p, name="Contingency", charge_kind="percent", percent=10)
    m.call("set_charge", path=p, name="Completion bond", charge_kind="percent", percent=2,
           excluded_categories=["50"])
    m.call("delete_category", path=p, code="54")
    m.call("set_credit", path=p, name=PROGRAM, applied=False, locations=[STATION],
           cap="40000", rows=[
               {"source": {"kind": "category", "category": c}, "percent": pct,
                "cap": {"kind": "none"}, "description": d}
               for c, d, pct in PROGRAM_ROWS])

    # Draft 2: a copy with the approved draft's figures, for the version and the
    # original totals.
    m.call("save_as", path=p, out=DRAFT)
    for name, formula in DRAFT2_GLOBALS.items():
        m.call("set_global", path=DRAFT, name=name, formula=formula)
    for account, desc, field, value in DRAFT2_LINES:
        m.call("update_lines", path=DRAFT, rows=[{"id": find_row(m, DRAFT, account, desc),
                                                  field: value}])
    m.call("delete_lines", path=DRAFT,
           rows=[find_row(m, DRAFT, a, d) for a, d in DRAFT2_DELETE])
    m.call("set_title", path=DRAFT, title=f"{s['title']}, draft 2")
    original = baseline(m, DRAFT)
    m.close()

    doc = json.loads(path.read_text())
    b = doc["budget"]
    # The dates New Budget fills from a schedule (Prep start, Shoot start, Shoot end, Wrap
    # end): prep and wrap from the budget's own weeks around the schedule's shoot days.
    start = datetime.date.fromisoformat(s["start"])
    end = datetime.date.fromisoformat(s["end"])
    prep = start - datetime.timedelta(weeks=int(b["globals"]["PREP_WKS"]))
    wrap = end + datetime.timedelta(weeks=int(b["globals"]["WRAP_WKS"]))
    b["info"] = {
        "Production type": {"kind": "text", "value": "Feature"},
        "Prep start": {"kind": "date", "value": prep.isoformat()},
        "Shoot start": {"kind": "date", "value": s["start"]},
        "Shoot end": {"kind": "date", "value": s["end"]},
        "Wrap end": {"kind": "date", "value": wrap.isoformat()},
    }
    add_notes(b)
    b["baseline"] = original
    draft = json.loads((out / DRAFT).read_text())["budget"]
    snap = {k: v for k, v in draft.items() if k not in showcase.VERSION_DROP}
    b["versions"] = [{"id": 1, "name": "Draft 2", "created": "2027-02-26T17:30:00-08:00",
                      "note": "Approved by the financiers", "budget": snap}]
    path.write_text(json.dumps(doc, indent=2) + "\n")
    (out / DRAFT).unlink()
    # Exact decimal arithmetic: a percent of a round figure is that figure, with no
    # binary rounding step for the Explain picture to show.
    method("set", str(path), "arithmetic", "exact")
    add_actuals(path, end)
    method("fmt", str(path))
    method("check", str(path))
    (out / SHEET).write_text(csv_text(camera_sheet(s["total"])))
    return [path, out / SHEET]


def baseline(m, p):
    """The original totals as the app locks them (Baseline::capture): each costed
    row's, account's and category's total, and the grand and net totals, in cents."""
    t = m.call("totals", path=p)
    rows, accounts, categories = {}, {}, {}
    for level in t["levels"]:
        for cat in level["categories"]:
            categories[cat["code"]] = categories.get(cat["code"], 0) + cat["total_cents"]
            for acct in cat["accounts"]:
                accounts[acct["code"]] = accounts.get(acct["code"], 0) + acct["total_cents"]
                for r in m.call("get_account", path=p, account=acct["code"])["rows"]:
                    if r.get("kind", "line") in ("line", "adjustment"):
                        rows[str(r["id"])] = r["total_cents"]
    return {"rows": dict(sorted(rows.items(), key=lambda kv: int(kv[0]))),
            "accounts": accounts, "categories": categories,
            "grand_total": t["grand_total_cents"], "net_total": t["net_total_cents"],
            "frozen": True}


def add_notes(b):
    for account, desc, notes in NOTES:
        for cat in b["topsheet"]:
            for acct in cat.get("accounts", []):
                if acct["code"] != account:
                    continue
                for d in acct.get("details", []):
                    if d.get("description") == desc:
                        d["notes"] = [
                            {"id": i, "created": c, "author": a, "done": done, "text": t}
                            for i, (c, a, t, done) in enumerate(notes, start=1)]


def add_actuals(path, end):
    totals = json.loads(method("totals", str(path), "--json"))
    acct = {a["code"]: a["total"] for c in totals["categories"] for a in c["accounts"]}
    # Through the second shoot week: the Friday before the third.
    cutoff = datetime.date.fromisoformat("2027-05-14")
    first = datetime.date.fromisoformat("2027-03-15")
    span = (cutoff - first).days
    tx = []
    for i, (code, share, vendor, desc) in enumerate(ACTUALS, start=1):
        day = first + datetime.timedelta(days=(i * 23) % (span + 1))
        tx.append({"id": i, "date": day.isoformat(), "account": code, "vendor": vendor,
                   "description": desc, "reference": f"INV-{4100 + i * 13}",
                   "amount": round(acct[code] * share), "batch": 1})
    doc = json.loads(path.read_text())
    doc["budget"]["actuals"] = {
        "batches": [{"id": 1, "file": "ledger-through-week-two.csv",
                     "imported": "2027-05-17T08:30:00-07:00", "rows": len(tx)}],
        "transactions": tx,
        "etc": {"24-04": {"kind": "manual", "amount": 0},
                "21-04": {"kind": "percent_complete", "percent": "50"}},
    }
    path.write_text(json.dumps(doc, indent=2) + "\n")


def main(argv):
    if not Path(METHOD).exists():
        sys.exit(f"make-field-station: no {METHOD} (cargo build --release -p method-cli)")
    if "--days" in argv:
        i = argv.index("--days")
        days = Path(argv[i + 1]).resolve()
        out = Path(argv[argv.index("--out") + 1]).resolve() if "--out" in argv else None
        if out is None or out == FIXTURES:
            sys.exit("make-field-station: --days needs --out, a folder other than the fixtures")
        for f in build(out, days):
            print(f"wrote {f}")
        print(method("totals", str(out / BUDGET)), end="")
        return 0
    if "--check" in argv:
        with tempfile.TemporaryDirectory(prefix="field-station-") as tmp:
            made = build(Path(tmp), SCHEDULE)
            stale = [f.name for f in made
                     if not (FIXTURES / f.name).exists()
                     or not filecmp.cmp(f, FIXTURES / f.name, shallow=False)]
            # Today's four-day file still makes a whole budget.
            build(Path(tmp) / "four-day", OLD_SCHEDULE)
        if stale:
            print("make-field-station: out of date (run python3 scripts/make-field-station.py): "
                  + ", ".join(stale), file=sys.stderr)
            return 1
        print("make-field-station: the fixtures are up to date")
        return 0
    for f in build(FIXTURES, SCHEDULE):
        print(f"wrote {f.relative_to(REPO)}")
    print(method("totals", str(FIXTURES / BUDGET)), end="")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
