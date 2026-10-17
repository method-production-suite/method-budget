#!/usr/bin/env python3
"""Make the Field Station budget the README and site pictures are taken from.

    python3 scripts/make-field-station.py            write the fixtures
    python3 scripts/make-field-station.py --check    make them again in a scratch folder;
                                                     exit 1 unless every byte is the same

Field Station is the suite's one invented production (SUITE-STANDARDS.md, section 7):
Method Schedule breaks the script down and writes its shoot days as a schedule days file,
and Method Budget budgets that schedule. This script reads that file,
fixtures/field-station/field-station.budget.json (Method Schedule owns it; README.md in
that folder), and writes beside it:

  field-station.mthd         a small independent feature shooting four days (two weeks of
                             prep, one of wrap) on the low-budget feature chart, with
                             globals for the schedule's figures, the four cast members,
                             crew, the two locations, props, wardrobe and effects lines,
                             percent fringes with cutoffs, a flat hourly fringe and a
                             contingency charge (the chart's empty Contingency
                             category is removed)
  camera-department.csv      the camera department's rental list, the spreadsheet the
                             Import Spreadsheet picture reviews

Every name and figure is made up: no real production, person or company, and no credit
line. Day counts come from the schedule file (SHOOT_DAYS, each location's days, each cast
member's and each prop's work days), so the budget agrees with the schedule; the rest is
invented and plausible for a micro-budget feature.

The budget starts from a bundled chart (`method new --template`) and is filled through
`method mcp`, the same typed tools a coding agent uses (the client is the one in
make-showcase-budgets.py), so the file is exactly what the engine writes. The budget
information dates, which no tool writes, are added in the shape docs/FORMAT.md documents,
then the file is checked and rewritten in canonical form with `method fmt`.

Uses target/release/method-budget (METHOD=... for another build). Python 3.9+, standard library
only.
"""

import filecmp
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
FIXTURES = REPO / "fixtures" / "field-station"
SCHEDULE = FIXTURES / "field-station.budget.json"
BUDGET = "field-station.mthd"
SHEET = "camera-department.csv"

_spec = importlib.util.spec_from_file_location(
    "showcase", Path(__file__).resolve().parent / "make-showcase-budgets.py")
showcase = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(showcase)
METHOD = showcase.METHOD
method = showcase.method

# ---------------------------------------------------------------------------
# The schedule's figures


def schedule():
    s = json.loads(SCHEDULE.read_text())
    if s.get("format") != "method-schedule-days" or s.get("version") != 1:
        sys.exit(f"make-field-station: {SCHEDULE} isn't a schedule days file, version 1")
    days = s["shoot_days"]
    work = {}
    for e in s["elements"]:
        work[(e["category"], e["name"])] = e["work"]
    return {
        "title": s["schedule"],
        "shoot_days": days["total"],
        "start": days["start"],
        "end": days["end"],
        "locations": days["by_location"],
        "work": work,
    }


def days(sched, category, name):
    try:
        return sched["work"][(category, name)]
    except KeyError:
        sys.exit(f"make-field-station: the schedule has no {category} element {name}")


# ---------------------------------------------------------------------------
# Setup. Rates and cutoffs are plausible, not any union's or state's.

STATION = "Cold Spring field station"
RIDGE = "Hollins Ridge fire road"

FRINGES = [
    ("FICA", {"rate": "7.65", "cutoff": "176,100.00",
              "description": "Social security and medicare"}),
    ("SUI", {"rate": "3.4", "cutoff": "7,000.00", "description": "State unemployment"}),
    ("WC", {"rate": "2.85", "description": "Workers' compensation"}),
    ("CAST_PH", {"rate": "17.5", "cutoff": "225,000.00",
                 "description": "Performers' pension and health"}),
    ("CREW_HW", {"fringe_kind": "flat", "rate": "9.85", "unit": "Hour",
                 "basis": "amount_counts", "max_hours_per_day": "12",
                 "description": "Crew health, per hour worked"}),
]
TAX = ["FICA", "SUI", "WC"]
F = {"none": [], "tax": TAX, "cast": TAX + ["CAST_PH"], "crew": TAX + ["CREW_HW"]}

ALL_WKS = "PREP_WKS + SHOOT_WKS + WRAP_WKS"


def plan(s):
    """The globals and every account's lines, with the schedule's day counts."""
    loc = s["locations"]
    globals_ = [
        ("SHOOT_DAYS", str(s["shoot_days"]), "Shooting days, from the schedule"),
        ("SHOOT_WKS", "SHOOT_DAYS / 5", "Shooting weeks, five-day weeks"),
        ("PREP_WKS", "2", "Weeks of prep"),
        ("WRAP_WKS", "1", "Weeks of wrap"),
        ("STATION_DAYS", str(loc[STATION]), "Shoot days at the field station"),
        ("RIDGE_DAYS", str(loc[RIDGE]), "Shoot days on the fire road"),
        ("CREW_MEALS", "30", "People fed each shooting day"),
    ]
    cast = lambda name: days(s, "Cast", name)  # noqa: E731
    prop = lambda name: days(s, "Hand props", name)  # noqa: E731
    dressing = lambda name: days(s, "Set dressing", name)  # noqa: E731
    fx = lambda name: days(s, "Practical effects", name)  # noqa: E731
    coats = days(s, "Wardrobe", "WEATHER COATS")

    # A line: (description, amount, unit, x, rate, fringe set[, location]).
    lines = {
        "10-01": [("Story rights, original screenplay", 1, "Flat", 1, 5000, "none")],
        "10-02": [("Screenplay fee", 1, "Flat", 1, 15000, "tax")],
        "10-03": [("Script printing and table read", 1, "Allow", 1, 450, "none")],
        "11-01": [("Producer fee", 1, "Flat", 1, 12000, "none")],
        "11-02": [("Associate producer", ALL_WKS, "Week", 1, 900, "crew")],
        "12-01": [("Director fee", 1, "Flat", 1, 15000, "tax")],
        "13-01": [("DELACROIX", cast("DELACROIX"), "Day", 1, 2400, "cast"),
                  ("MARA", cast("MARA"), "Day", 1, 2400, "cast"),
                  ("TEO", cast("TEO"), "Day", 1, 1800, "cast"),
                  ("Rehearsal day, three leads", 1, "Day", 3, 600, "cast")],
        "13-02": [("IVERSEN, radio voice", cast("IVERSEN"), "Day", 1, 1100, "cast"),
                  ("Overtime and adjustments", 1, "Allow", 1, 1500, "cast")],
        "13-03": [("Casting director, package", 1, "Flat", 1, 6500, "none")],
        "13-04": [("Cast travel and lodging, Cold Spring", 1, "Allow", 1, 2400, "none")],
        "20-01": [("Line producer", ALL_WKS, "Week", 1, 1800, "crew")],
        "20-02": [("First assistant director", "1 + SHOOT_WKS", "Week", 1, 1900, "crew")],
        "20-03": [("Second assistant director", "SHOOT_DAYS", "Day", 1, 350, "crew")],
        "20-04": [("Production coordinator", "PREP_WKS + SHOOT_WKS", "Week", 1, 1100, "crew")],
        "20-05": [("Script supervisor", "SHOOT_DAYS + 1", "Day", 1, 450, "crew")],
        "20-06": [("Production assistants", "SHOOT_DAYS", "Day", 3, 200, "crew")],
        "20-07": [("Production accountant", 3, "Week", 1, 950, "crew")],
        "21-01": [("Director of photography", "SHOOT_DAYS + 3", "Day", 1, 850, "crew")],
        "21-02": [("First assistant camera", "SHOOT_DAYS + 1", "Day", 1, 550, "crew"),
                  ("Second assistant camera", "SHOOT_DAYS", "Day", 1, 450, "crew")],
        "21-03": [("Digital imaging technician", "SHOOT_DAYS", "Day", 1, 500, "crew")],
        "21-04": [("Camera package, rental house estimate", "SHOOT_DAYS", "Day", 1, 1250, "none")],
        "21-05": [("Expendables and batteries", 1, "Allow", 1, 600, "none")],
        "22-01": [("Gaffer", "SHOOT_DAYS + 1", "Day", 1, 600, "crew")],
        "22-02": [("Key grip", "SHOOT_DAYS + 1", "Day", 1, 600, "crew")],
        "22-03": [("Grip and electric crew", "SHOOT_DAYS", "Day", 3, 450, "crew")],
        "22-04": [("Lighting and grip package", "SHOOT_DAYS", "Day", 1, 1400, "none")],
        "22-05": [("Expendables", 1, "Allow", 1, 900, "none")],
        "23-01": [("Production sound mixer", "SHOOT_DAYS", "Day", 1, 650, "crew")],
        "23-02": [("Boom operator", "SHOOT_DAYS", "Day", 1, 450, "crew")],
        "23-03": [("Sound package", "SHOOT_DAYS", "Day", 1, 350, "none")],
        "24-01": [("Production designer", "PREP_WKS + SHOOT_WKS", "Week", 1, 1500, "crew")],
        "24-02": [("Art department assistant", "PREP_WKS + SHOOT_WKS", "Week", 1, 900, "crew")],
        "24-03": [("Station radio set, rental", dressing("STATION RADIO SET"), "Day", 1, 95, "none"),
                  ("Generator, non-working prop", dressing("GENERATOR"), "Day", 1, 60, "none"),
                  ("Mast ladder, rental", dressing("MAST LADDER"), "Day", 1, 140, "none"),
                  ("Camp stove", 1, "Flat", 1, 85, "none"),
                  ("Hand radios, pair", prop("HAND RADIO"), "Day", 2, 35, "none"),
                  ("Paper maps, aged, with doubles", 1, "Flat", 4, 30, "none"),
                  ("Field packs", 1, "Flat", 2, 120, "none"),
                  ("Headlamps and torch", 1, "Flat", 1, 140, "none"),
                  ("Tea mugs, notebook and key ring", 1, "Allow", 1, 110, "none")],
        "24-04": [("Station interior, dress and strike", 1, "Allow", 1, 3500, "none")],
        "24-05": [("Rain, towers and water truck", fx("RAIN"), "Day", 1, 1800, "none"),
                  ("Wind, fans and operator", fx("WIND"), "Day", 1, 650, "none"),
                  ("Effects technician", fx("WIND"), "Day", 1, 550, "crew")],
        "25-01": [("Costume designer", "PREP_WKS + SHOOT_WKS", "Week", 1, 1200, "crew")],
        "25-02": [("Wardrobe assistant", "SHOOT_DAYS", "Day", 1, 300, "crew")],
        "25-03": [("Weather coats, three, with doubles", 1, "Flat", 6, 180, "none"),
                  ("Weather coats, cleaning on wet days", coats, "Day", 1, 45, "none"),
                  ("Wardrobe rentals and alterations", 1, "Allow", 1, 1400, "none")],
        "25-04": [("Makeup and hair artist", "SHOOT_DAYS", "Day", 1, 450, "crew")],
        "25-05": [("Makeup and hair supplies", 1, "Allow", 1, 500, "none")],
        "26-01": [("Location manager", "PREP_WKS + SHOOT_WKS", "Week", 1, 1300, "crew")],
        "26-02": [("Site fee, Cold Spring field station", "STATION_DAYS", "Day", 1, 1500, "none", STATION),
                  ("Site fee, Hollins Ridge fire road", "RIDGE_DAYS", "Day", 1, 400, "none", RIDGE)],
        "26-03": [("Permits and certificates", 1, "Allow", 1, 1200, "none")],
        "26-04": [("Heaters, tents and toilets", "SHOOT_DAYS", "Day", 1, 350, "none", STATION)],
        "27-01": [("Production vans", "SHOOT_DAYS + 2", "Day", 2, 165, "none"),
                  ("Fire road shuttle, four-wheel drive", "RIDGE_DAYS", "Day", 1, 240, "none", RIDGE)],
        "27-02": [("Fuel and parking", 1, "Allow", 1, 900, "none")],
        "28-01": [("Catering", "SHOOT_DAYS", "Day", "CREW_MEALS", 32, "none")],
        "28-02": [("Craft service", "SHOOT_DAYS", "Day", 1, 220, "none")],
        "29-01": [("Production office and supplies", 1, "Month", 1, 1100, "none")],
        "29-02": [("Walkie-talkie rental", "SHOOT_DAYS", "Day", 1, 120, "none")],
        "30-01": [("Drives and media", 1, "Allow", 1, 1500, "none")],
        "30-02": [("Backup storage, six months", 6, "Month", 1, 60, "none")],
        "40-01": [("Editor", 10, "Week", 1, 1600, "crew")],
        "40-02": [("Assistant editor", 6, "Week", 1, 900, "crew")],
        "40-03": [("Edit system", 3, "Month", 1, 450, "none")],
        "41-01": [("Sound design and editing", 1, "Flat", 1, 9000, "none")],
        "41-02": [("Mix", 1, "Flat", 1, 6000, "none")],
        "41-03": [("ADR and foley", 1, "Allow", 1, 2500, "none")],
        "42-01": [("Composer, package", 1, "Flat", 1, 8000, "none")],
        "42-02": [("Music licensing", 1, "Allow", 1, 3000, "none")],
        "43-01": [("Visual effects, cleanup and sky", 1, "Allow", 1, 4500, "none")],
        "43-02": [("Titles and graphics", 1, "Flat", 1, 1500, "none")],
        "44-01": [("Color grading", 1, "Flat", 1, 6500, "none")],
        "44-02": [("Conform and online", 1, "Flat", 1, 1500, "none")],
        "45-01": [("Masters", 1, "Allow", 1, 2000, "none")],
        "45-02": [("Digital cinema package", 1, "Flat", 1, 1200, "none")],
        "45-03": [("Closed captions and subtitles", 1, "Flat", 1, 900, "none")],
        "50-01": [("Production insurance", 1, "Allow", 1, 6500, "none")],
        "50-02": [("Errors and omissions", 1, "Allow", 1, 4000, "none")],
        "51-01": [("Production legal", 1, "Allow", 1, 7500, "none")],
        "51-02": [("Payroll service fees", 1, "Allow", 1, 2200, "none")],
        "52-01": [("Festival entry fees", 12, "Flat", 1, 75, "none")],
        "52-02": [("Poster, trailer and press kit", 1, "Allow", 1, 2500, "none")],
        "53-01": [("Bank fees", 1, "Allow", 1, 250, "none")],
        "53-02": [("Miscellaneous", 1, "Allow", 1, 1000, "none")],
    }
    return globals_, lines


def line(desc, amount, unit, x, rate, fr, location=None):
    row = {"description": desc, "amount": str(amount), "unit": unit, "x": str(x),
           "rate": str(rate), "fringes": F[fr]}
    if location:
        row["location"] = location
    return row


# The camera department's rental list: a title row, then the header and the lines, each
# with the department's own total, which the import checks every figure against.
CAMERA_SHEET = [
    ["Field Station, camera department rental list", "", "", "", "", "", ""],
    ["Acct #", "Description", "Qty", "Unit", "X", "Rate", "Total"],
    ["21-04", "Camera body", "4", "Days", "1", "450.00", "1,800.00"],
    ["21-04", "Prime lens set, five lenses", "4", "Days", "1", "325.00", "1,300.00"],
    ["21-04", "Zoom lens", "4", "Days", "1", "150.00", "600.00"],
    ["21-04", "Wireless focus and monitor", "4", "Days", "1", "180.00", "720.00"],
    ["21-04", "Tripod, head and sticks", "4", "Days", "1", "65.00", "260.00"],
    ["21-04", "Batteries and charging", "4", "Days", "2", "20.00", "160.00"],
    ["21-05", "Filters and diffusion", "1", "Allow", "1", "350.00", "350.00"],
    ["21-05", "Lens tissue and camera tape", "1", "Allow", "1", "120.00", "120.00"],
]


def csv_text(rows):
    def cell(v):
        return f'"{v}"' if any(c in v for c in ',"\n') else v
    return "".join(",".join(cell(v) for v in r) + "\n" for r in rows)


def build(out):
    out.mkdir(parents=True, exist_ok=True)
    s = schedule()
    globals_, lines = plan(s)
    path = out / BUDGET
    if path.exists():
        path.unlink()
    method("new", str(path), "--template", "low-budget-feature")

    m = showcase.Mcp(out)
    p = BUDGET
    m.call("set_title", path=p, title=s["title"])
    for name, formula, desc in globals_:
        m.call("set_global", path=p, name=name, formula=formula, description=desc)
    for name, fields in FRINGES:
        m.call("add_setup", path=p, type="fringe", name=name, fields=fields)
    for name, desc in [(STATION, "Interior and exterior, the station and its yard"),
                       (RIDGE, "Exterior, the ridge road and the radio mast")]:
        m.call("add_setup", path=p, type="location", name=name, fields={"description": desc})
    m.call("add_account", path=p, category="24", code="24-05", name="Practical effects")
    for account, rows in lines.items():
        m.call("add_lines", path=p, account=account, use_defaults=False,
               lines=[line(*r) for r in rows])
    # The contingency is a charge on the topsheet. The chart's Contingency category would
    # only ever show 0.00 beside it, so it goes.
    m.call("set_charge", path=p, name="Contingency", charge_kind="percent", percent=10)
    m.call("delete_category", path=p, code="54")
    m.close()

    # The dates New Budget fills from a schedule (Prep start, Shoot start, Shoot end, Wrap
    # end): two weeks of prep before the first shoot day, a week of wrap after the last.
    doc = json.loads(path.read_text())
    doc["budget"]["info"] = {
        "Prep start": {"kind": "date", "value": "2027-04-19"},
        "Shoot start": {"kind": "date", "value": s["start"]},
        "Shoot end": {"kind": "date", "value": s["end"]},
        "Wrap end": {"kind": "date", "value": "2027-05-14"},
    }
    path.write_text(json.dumps(doc, indent=2) + "\n")
    # Exact decimal arithmetic: a percent of a round figure is that figure, with no
    # binary rounding step for the Explain picture to show.
    method("set", str(path), "arithmetic", "exact")
    method("fmt", str(path))
    method("check", str(path))
    (out / SHEET).write_text(csv_text(CAMERA_SHEET))
    return [path, out / SHEET]


def main(argv):
    if not Path(METHOD).exists():
        sys.exit(f"make-field-station: no {METHOD} (cargo build --release -p method-cli)")
    if "--check" in argv:
        with tempfile.TemporaryDirectory(prefix="field-station-") as tmp:
            made = build(Path(tmp))
            stale = [f.name for f in made
                     if not (FIXTURES / f.name).exists()
                     or not filecmp.cmp(f, FIXTURES / f.name, shallow=False)]
        if stale:
            print("make-field-station: out of date (run python3 scripts/make-field-station.py): "
                  + ", ".join(stale), file=sys.stderr)
            return 1
        print("make-field-station: the fixtures are up to date")
        return 0
    for f in build(FIXTURES):
        print(f"wrote {f.relative_to(REPO)}")
    print(method("totals", str(FIXTURES / BUDGET)), end="")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
