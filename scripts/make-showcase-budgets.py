#!/usr/bin/env python3
"""Make the invented budgets the website's screenshots are taken from.

    python3 scripts/make-showcase-budgets.py [OUT_DIR]     (default: target/showcase)

Writes, into OUT_DIR (refusing to overwrite unless --force):

  lanterns-draft-3.mthd   an independent feature (about 2.4 million), with fringes
                          (percent with cutoffs and a flat hourly one), globals,
                          contingency, a completion bond, a production incentive,
                          draft 2 kept as a version, and invented actuals for a cost
                          report
  lanterns-draft-2.mthd   the same feature one draft earlier, for Compare and
                          What Changed
  quarry-hill-104.mthd    one episode of an invented one-hour series
  first-light-30.mthd     an invented 30-second commercial

Everything here is made up: titles, character names, rates and vendors. No real
production, person or company. Each budget starts from a bundled chart of accounts
(`method-budget new --template`) and is filled through the command line's own MCP server
(`method-budget mcp`), the same typed tools a coding agent uses, so the files are exactly what
the engine writes. The two things neither tool writes, a saved version and actuals, are
added to the JSON by this script in the shape docs/FORMAT.md documents, then the file is
checked and rewritten in canonical form with `method-budget fmt`.

Uses target/release/method-budget (set METHOD=... to use another). Python 3.9+, standard
library only.
"""

import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
METHOD = os.environ.get("METHOD", str(REPO / "target" / "release" / "method-budget"))

# ---------------------------------------------------------------------------
# The MCP client: one server per output folder, newline-delimited JSON-RPC.


class Mcp:
    def __init__(self, root):
        self.p = subprocess.Popen(
            [METHOD, "mcp", "--root", str(root)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            text=True,
        )
        self.n = 0
        self._rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                 "clientInfo": {"name": "make-showcase-budgets", "version": "1"}})
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def _send(self, msg):
        self.p.stdin.write(json.dumps(msg) + "\n")
        self.p.stdin.flush()

    def _rpc(self, method, params):
        self.n += 1
        self._send({"jsonrpc": "2.0", "id": self.n, "method": method, "params": params})
        while True:
            line = self.p.stdout.readline()
            if not line:
                sys.exit("make-showcase-budgets: the MCP server stopped")
            msg = json.loads(line)
            if msg.get("id") == self.n:
                if "error" in msg:
                    sys.exit(f"make-showcase-budgets: {method}: {msg['error']}")
                return msg["result"]

    def call(self, tool, **args):
        res = self._rpc("tools/call", {"name": tool, "arguments": args})
        if res.get("isError"):
            sys.exit(f"make-showcase-budgets: {tool} {json.dumps(args)[:300]}: "
                     f"{res['content'][0]['text']}")
        return res.get("structuredContent", {})

    def close(self):
        self.p.stdin.close()
        self.p.wait(timeout=10)


def method(*args):
    r = subprocess.run([METHOD, *args], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"make-showcase-budgets: method-budget {' '.join(args)}: {r.stderr or r.stdout}")
    return r.stdout


# ---------------------------------------------------------------------------
# Shared setup: fringes. Rates and cutoffs are plausible, not any union's or state's.

FRINGES = [
    ("FICA", {"rate": "7.65", "cutoff": "176,100.00", "description": "Social security and medicare"}),
    ("SUI", {"rate": "3.4", "cutoff": "7,000.00", "description": "State unemployment"}),
    ("WC", {"rate": "2.85", "description": "Workers' compensation"}),
    ("WR_PH", {"rate": "19", "description": "Writers' pension and health"}),
    ("DIR_PH", {"rate": "15.5", "cutoff": "250,000.00", "description": "Directors' pension and health"}),
    ("CAST_PH", {"rate": "21", "cutoff": "225,000.00", "description": "Performers' pension and health"}),
    ("CREW_PH", {"rate": "8.5", "description": "Crew pension"}),
    ("CREW_HW", {"fringe_kind": "flat", "rate": "9.85", "unit": "Hour", "basis": "amount_counts",
                 "max_hours_per_day": "12", "description": "Crew health, per hour worked"}),
]
TAX = ["FICA", "SUI", "WC"]
F = {
    "none": [],
    "writer": TAX + ["WR_PH"],
    "dir": TAX + ["DIR_PH"],
    "cast": TAX + ["CAST_PH"],
    "crew": TAX + ["CREW_PH", "CREW_HW"],
    "tax": TAX,
}

# A line: (description, amount, unit, x, rate, fringe set).

# ---------------------------------------------------------------------------
# 1. "Lanterns Over Kettle Lake", an independent feature, draft 3.

FEATURE_GLOBALS = [
    ("PREP_WKS", "6", "Weeks of prep"),
    ("SHOOT_DAYS", "24", "Shooting days"),
    ("SHOOT_WKS", "SHOOT_DAYS / 5", "Shooting weeks, five-day weeks"),
    ("WRAP_WKS", "1", "Weeks of wrap"),
    ("CREW_MEALS", "70", "People fed each shooting day"),
]
ALL_WKS = "PREP_WKS + SHOOT_WKS + WRAP_WKS"

FEATURE_LINES = {
    "10-01": [("Option and purchase price, novel", 1, "Allow", 1, 25000, "none")],
    "11-01": [("Screenplay, writer's fee", 1, "Flat", 1, 65000, "writer"),
              ("Production polish", 1, "Flat", 1, 15000, "writer")],
    "12-02": [("Producer fee", 1, "Flat", 1, 75000, "none"),
              ("Producer fee, second producer", 1, "Flat", 1, 40000, "none")],
    "12-04": [("Associate producer", ALL_WKS, "Week", 1, 1800, "crew")],
    "13-01": [("Director fee", 1, "Flat", 1, 90000, "dir")],
    "14-01": [("Mae Calder, lead", 1, "Flat", 1, 85000, "cast"),
              ("Ivo Brandt, lead", 1, "Flat", 1, 60000, "cast")],
    "14-02": [("Hollis Calder", 3, "Week", 1, 6500, "cast"),
              ("June Abernathy", 2, "Week", 1, 4800, "cast"),
              ("Deputy Ray Oduya", 2, "Week", 1, 4800, "cast"),
              ("Petra Lind", 1.4, "Week", 1, 4800, "cast"),
              ("The ferryman", 4, "Day", 1, 1150, "cast"),
              ("Young Mae", 5, "Day", 1, 1150, "cast"),
              ("Rehearsal days", 3, "Day", 4, 1150, "cast"),
              ("Overtime", 1, "Allow", 1, 6000, "cast")],
    "14-03": [("Day players", 26, "Day", 1, 1150, "cast")],
    "14-04": [("Casting director, package", 1, "Flat", 1, 32000, "none")],
    "15-01": [("Stunt coordinator", 6, "Day", 1, 1300, "cast")],
    "15-02": [("Stunt performers", 8, "Day", 1, 1150, "cast")],
    "20-01": [("Unit production manager", ALL_WKS, "Week", 1, 2200, "crew")],
    "20-02": [("First assistant director", "PREP_WKS - 2 + SHOOT_WKS", "Week", 1, 2350, "crew")],
    "20-03": [("Second assistant director", "SHOOT_WKS + 1", "Week", 1, 1550, "crew")],
    "20-05": [("Production coordinator", ALL_WKS, "Week", 1, 1250, "crew")],
    "20-07": [("Script supervisor", "SHOOT_WKS + 0.4", "Week", 1, 1700, "crew")],
    "20-08": [("Production accountant", "PREP_WKS + SHOOT_WKS + 3", "Week", 1, 1600, "crew")],
    "20-10": [("Set production assistants", "SHOOT_WKS", "Week", 4, 700, "crew")],
    "21-01": [("Background performers", "SHOOT_DAYS", "Day", 10, 150, "cast")],
    "21-04": [("Background casting", 1, "Flat", 1, 4200, "none")],
    "22-01": [("Production designer", "PREP_WKS + SHOOT_WKS", "Week", 1, 2150, "crew")],
    "22-02": [("Art director", "PREP_WKS + SHOOT_WKS", "Week", 1, 1700, "crew")],
    "22-08": [("Art department purchases", 1, "Allow", 1, 9000, "none")],
    "23-03": [("Carpenters", 4, "Week", 3, 1250, "crew")],
    "23-06": [("Construction materials", 1, "Allow", 1, 24500, "none")],
    "24-01": [("Set decorator", "PREP_WKS + SHOOT_WKS", "Week", 1, 1600, "crew")],
    "24-03": [("Set dressers", "SHOOT_WKS + 2", "Week", 3, 1100, "crew")],
    "24-05": [("Set dressing purchases", 1, "Allow", 1, 21000, "none")],
    "24-06": [("Set dressing rentals", "SHOOT_WKS", "Week", 1, 2750, "none")],
    "25-01": [("Property master", "PREP_WKS - 2 + SHOOT_WKS", "Week", 1, 1550, "crew")],
    "25-04": [("Prop purchases and rentals", 1, "Allow", 1, 10500, "none")],
    "27-01": [("Director of photography", "SHOOT_WKS + 2", "Week", 1, 3400, "crew")],
    "27-02": [("Camera operator, B camera", "SHOOT_WKS", "Week", 1, 2300, "crew")],
    "27-03": [("First assistant camera", "SHOOT_WKS + 0.4", "Week", 2, 1800, "crew")],
    "27-04": [("Second assistant camera", "SHOOT_WKS + 0.4", "Week", 2, 1500, "crew")],
    "27-05": [("Digital imaging technician", "SHOOT_WKS", "Week", 1, 1750, "crew")],
    "27-07": [("Still photographer", 8, "Day", 1, 450, "crew")],
    "27-08": [("Camera bodies", "SHOOT_WKS + 0.4", "Week", 2, 1650, "none"),
              ("Prime lens set", "SHOOT_WKS + 0.4", "Week", 1, 1400, "none"),
              ("Zoom lenses", "SHOOT_WKS", "Week", 2, 450, "none"),
              ("Wireless focus", "SHOOT_WKS", "Week", 1, 650, "none"),
              ("Camera support", "SHOOT_WKS", "Week", 1, 400, "none"),
              ("Filters", 1, "Allow", 1, 2500, "none")],
    "28-01": [("Gaffer", "SHOOT_WKS + 0.4", "Week", 1, 2000, "crew")],
    "28-02": [("Best boy electric", "SHOOT_WKS + 0.4", "Week", 1, 1750, "crew")],
    "28-03": [("Lighting technicians", "SHOOT_WKS", "Week", 3, 1600, "crew")],
    "28-06": [("Lighting package", "SHOOT_WKS", "Week", 1, 5500, "none")],
    "28-07": [("Generator", "SHOOT_WKS", "Week", 1, 1150, "none")],
    "29-01": [("Key grip", "SHOOT_WKS + 0.4", "Week", 1, 2000, "crew")],
    "29-02": [("Best boy grip", "SHOOT_WKS + 0.4", "Week", 1, 1750, "crew")],
    "29-03": [("Dolly grip", "SHOOT_WKS", "Week", 1, 1700, "crew")],
    "29-04": [("Company grips", "SHOOT_WKS", "Week", 3, 1550, "crew")],
    "29-06": [("Grip package", "SHOOT_WKS", "Week", 1, 3900, "none")],
    "30-01": [("Production sound mixer", "SHOOT_WKS + 0.4", "Week", 1, 2100, "crew")],
    "30-02": [("Boom operator", "SHOOT_WKS", "Week", 1, 1600, "crew")],
    "30-04": [("Sound package", "SHOOT_WKS", "Week", 1, 1800, "none")],
    "32-01": [("Costume designer", "PREP_WKS + SHOOT_WKS", "Week", 1, 1900, "crew")],
    "32-04": [("Costumers", "SHOOT_WKS", "Week", 2, 1150, "crew")],
    "32-06": [("Wardrobe purchases", 1, "Allow", 1, 18000, "none")],
    "33-01": [("Makeup department head", "SHOOT_WKS + 1", "Week", 1, 1800, "crew")],
    "33-03": [("Hair department head", "SHOOT_WKS + 1", "Week", 1, 1750, "crew")],
    "33-06": [("Makeup and hair supplies", 1, "Allow", 1, 4900, "none")],
    "35-01": [("Location manager", ALL_WKS, "Week", 1, 1900, "crew")],
    "35-04": [("Site fees, lake house and boathouse", 12, "Day", 1, 2300, "none"),
              ("Site fees, town exteriors", 6, "Day", 1, 1450, "none")],
    "35-05": [("Permits", 1, "Allow", 1, 5850, "none")],
    "35-07": [("Security, overnight", "SHOOT_DAYS", "Day", 2, 300, "tax")],
    "36-03": [("Drivers", "SHOOT_WKS", "Week", 6, 1200, "crew")],
    "36-04": [("Production vehicles", "SHOOT_WKS + 1", "Week", 1, 5050, "none")],
    "36-06": [("Fuel", 1, "Allow", 1, 8000, "none")],
    "37-01": [("Catering", "SHOOT_DAYS", "Day", "CREW_MEALS", 0, "none")],
    "37-02": [("Craft service", "SHOOT_DAYS", "Day", 1, 400, "none")],
    "38-03": [("Production office rental", 3, "Month", 1, 4200, "none")],
    "39-01": [("Media and drives", 1, "Allow", 1, 5850, "none")],
    "40-02": [("Hotel rooms, lake week", 7, "Day", 22, 100, "none")],
    "50-01": [("Editor", 20, "Week", 1, 2450, "crew")],
    "50-02": [("Assistant editor", 22, "Week", 1, 1450, "crew")],
    "50-05": [("Edit suite rentals", 5, "Month", 1, 2100, "none")],
    "51-06": [("Pre-mix and final mix", 1, "Allow", 1, 50500, "none")],
    "52-01": [("Composer, package", 1, "Flat", 1, 36000, "none")],
    "52-05": [("Licensed songs", 4, "Flat", 1, 6200, "none")],
    "53-04": [("Visual effects, cleanup and sky replacements", 1, "Allow", 1, 55000, "none")],
    "55-02": [("Color grading", 1, "Allow", 1, 21000, "none")],
    "55-05": [("Masters and deliverables", 1, "Allow", 1, 15500, "none")],
    "60-01": [("Production package", 1, "Allow", 1, 30000, "none")],
    "60-03": [("Errors and omissions", 1, "Allow", 1, 6200, "none")],
    "61-01": [("Production legal", 1, "Allow", 1, 24500, "none")],
    "61-04": [("Payroll service fees", 1, "Allow", 1, 8000, "none")],
}

# Draft 2 differs from draft 3 here (draft 3 = these changes applied to draft 2):
# two more shooting days, a DP rate agreed higher, a third lighting technician
# week cut, visual effects trimmed, and a stunt day added.
DRAFT2_EDITS = {
    "SHOOT_DAYS": "22",
    ("27-01", 0, "rate"): 3150,
    ("53-04", 0, "rate"): 72000,
    ("15-01", 0, "amount"): 5,
    ("52-05", 0, "amount"): 3,
    ("14-02", 0, "amount"): 2,
}

# ---------------------------------------------------------------------------
# 2. "Quarry Hill", episode 104, an invented one-hour drama.

TV_GLOBALS = [
    ("PREP_DAYS", "8", "Episode prep days"),
    ("SHOOT_DAYS", "8", "Episode shooting days"),
    ("CREW_MEALS", "95", "People fed each shooting day"),
]
TV_LINES = {
    "11-01": [("Teleplay, episode 104", 1, "Flat", 1, 42000, "writer")],
    "11-02": [("Staff writers, episode share", 1, "Flat", 4, 7800, "writer")],
    "12-01": [("Executive producers, episode share", 1, "Flat", 3, 30000, "none")],
    "12-03": [("Producer", 1, "Flat", 1, 18000, "none")],
    "13-01": [("Episode director", "PREP_DAYS + SHOOT_DAYS", "Day", 1, 2950, "dir")],
    "14-01": [("Series regulars, six", 1, "Flat", 6, 38000, "cast")],
    "14-02": [("Recurring cast", 4, "Day", 3, 2400, "cast")],
    "14-03": [("Guest star, role of Dell Harrow", 1, "Week", 1, 9800, "cast")],
    "14-04": [("Day players", 9, "Day", 1, 1150, "cast")],
    "14-05": [("Casting director, episode", 1, "Flat", 1, 12500, "none")],
    "20-01": [("Unit production manager", 3, "Week", 1, 4300, "crew")],
    "20-02": [("First assistant director", "PREP_DAYS + SHOOT_DAYS", "Day", 1, 850, "crew")],
    "20-03": [("Second assistant directors", "SHOOT_DAYS", "Day", 2, 560, "crew")],
    "20-09": [("Set production assistants", "SHOOT_DAYS", "Day", 6, 230, "crew")],
    "21-01": [("Background performers", "SHOOT_DAYS", "Day", 25, 216, "cast")],
    "22-01": [("Production designer, episode", 3, "Week", 1, 4200, "crew")],
    "22-06": [("Art department purchases and rentals", 1, "Allow", 1, 26000, "none")],
    "23-03": [("Construction materials, swing sets", 1, "Allow", 1, 48000, "none")],
    "24-03": [("Set dressing purchases", 1, "Allow", 1, 32000, "none")],
    "25-03": [("Prop purchases and rentals", 1, "Allow", 1, 14000, "none")],
    "27-01": [("Director of photography", "SHOOT_DAYS + 2", "Day", 1, 1150, "crew")],
    "27-02": [("Camera operators", "SHOOT_DAYS", "Day", 2, 780, "crew")],
    "27-03": [("Camera assistants", "SHOOT_DAYS", "Day", 4, 620, "crew")],
    "27-05": [("Camera package, three bodies", "SHOOT_DAYS", "Day", 1, 3800, "none")],
    "28-01": [("Gaffer", "SHOOT_DAYS + 1", "Day", 1, 720, "crew")],
    "28-03": [("Lighting technicians", "SHOOT_DAYS", "Day", 5, 610, "crew")],
    "28-04": [("Lighting package", "SHOOT_DAYS", "Day", 1, 3400, "none")],
    "29-01": [("Key grip", "SHOOT_DAYS + 1", "Day", 1, 720, "crew")],
    "29-03": [("Dolly and company grips", "SHOOT_DAYS", "Day", 5, 600, "crew")],
    "29-04": [("Grip package", "SHOOT_DAYS", "Day", 1, 2200, "none")],
    "30-01": [("Production sound mixer", "SHOOT_DAYS", "Day", 1, 760, "crew")],
    "32-03": [("Costumers", "SHOOT_DAYS", "Day", 3, 520, "crew")],
    "32-04": [("Wardrobe purchases and rentals", 1, "Allow", 1, 22000, "none")],
    "33-01": [("Makeup artists", "SHOOT_DAYS", "Day", 3, 560, "crew")],
    "33-02": [("Hairstylists", "SHOOT_DAYS", "Day", 2, 560, "crew")],
    "35-03": [("Site fees, quarry and diner", 5, "Day", 1, 6500, "none")],
    "35-04": [("Permits", 1, "Allow", 1, 7500, "none")],
    "36-02": [("Drivers", "SHOOT_DAYS + 2", "Day", 9, 410, "crew")],
    "36-03": [("Production vehicles", 2, "Week", 1, 14500, "none")],
    "37-01": [("Catering", "SHOOT_DAYS", "Day", "CREW_MEALS", 34, "none")],
    "38-01": [("Stage rental, episode share", "SHOOT_DAYS", "Day", 1, 5200, "none")],
    "50-01": [("Episode editor", 5, "Week", 1, 4100, "crew")],
    "50-02": [("Assistant editor", 6, "Week", 1, 2300, "crew")],
    "51-03": [("Mix", 1, "Allow", 1, 28000, "none")],
    "52-01": [("Composer, episode", 1, "Flat", 1, 18000, "none")],
    "53-02": [("Visual effects vendors, episode", 1, "Allow", 1, 65000, "none")],
    "55-01": [("Color grading", 1, "Allow", 1, 12000, "none")],
    "55-04": [("Masters and deliverables", 1, "Allow", 1, 9500, "none")],
    "60-01": [("Production package, episode share", 1, "Allow", 1, 21000, "none")],
    "61-02": [("Payroll service fees", 1, "Allow", 1, 6500, "none")],
    "71-01": [("Standing sets, episode share", 1, "Allow", 1, 45000, "none")],
    "74-01": [("Main title sequence, episode share", 1, "Allow", 1, 8500, "none")],
}

# ---------------------------------------------------------------------------
# 3. "First Light", a 30-second spot for an invented oat drink.

SPOT_GLOBALS = [
    ("SHOOT_DAYS", "2", "Shooting days"),
    ("PREP_DAYS", "4", "Prep days"),
]
SPOT_LINES = {
    "10-01": [("Director, shoot days", "SHOOT_DAYS", "Day", 1, 12500, "dir")],
    "10-03": [("Director, prep and post", "PREP_DAYS", "Day", 1, 3750, "dir")],
    "11-01": [("Principal, the runner", "SHOOT_DAYS + 1", "Day", 1, 1150, "cast")],
    "11-02": [("Supporting, cafe owner and friend", "SHOOT_DAYS", "Day", 2, 950, "cast")],
    "11-03": [("Background", "SHOOT_DAYS", "Day", 18, 216, "cast")],
    "11-04": [("Casting, two days of sessions", 2, "Day", 1, 2400, "none")],
    "20-01": [("Line producer", "PREP_DAYS + SHOOT_DAYS + 1", "Day", 1, 950, "crew")],
    "20-03": [("First assistant director", "SHOOT_DAYS + 1", "Day", 1, 1050, "crew")],
    "20-05": [("Production assistants", "SHOOT_DAYS", "Day", 5, 300, "crew")],
    "21-01": [("Director of photography", "SHOOT_DAYS + 2", "Day", 1, 2800, "crew")],
    "21-02": [("Camera assistants", "SHOOT_DAYS + 1", "Day", 3, 750, "crew")],
    "21-04": [("Camera package, high-speed body", "SHOOT_DAYS", "Day", 1, 6800, "none")],
    "22-01": [("Gaffer", "SHOOT_DAYS + 1", "Day", 1, 850, "crew")],
    "22-03": [("Grip and electric crew", "SHOOT_DAYS", "Day", 8, 700, "crew")],
    "22-04": [("Lighting and grip package", "SHOOT_DAYS", "Day", 1, 7500, "none")],
    "23-01": [("Sound mixer", "SHOOT_DAYS", "Day", 1, 900, "crew")],
    "24-01": [("Production designer", "PREP_DAYS + SHOOT_DAYS", "Day", 1, 1100, "crew")],
    "24-04": [("Props and set dressing, cafe", 1, "Allow", 1, 16500, "none")],
    "25-01": [("Stylist", "PREP_DAYS + SHOOT_DAYS", "Day", 1, 850, "crew")],
    "25-03": [("Makeup and hair artists", "SHOOT_DAYS", "Day", 2, 800, "crew")],
    "26-02": [("Site fees, riverside path and cafe", "SHOOT_DAYS", "Day", 1, 8500, "none")],
    "27-02": [("Production vehicles", "SHOOT_DAYS + 1", "Day", 1, 2600, "none")],
    "27-03": [("Camera car and driver", 1, "Day", 1, 5800, "none")],
    "28-01": [("Catering", "SHOOT_DAYS", "Day", 55, 38, "none")],
    "40-01": [("Editor", 6, "Day", 1, 1500, "crew")],
    "40-03": [("Edit suite", 6, "Day", 1, 750, "none")],
    "41-01": [("Color grading", 1, "Day", 1, 6500, "none")],
    "42-01": [("Sound design", 1, "Allow", 1, 7500, "none")],
    "42-02": [("Mix", 1, "Allow", 1, 5500, "none")],
    "42-03": [("Music licensing, one track", 1, "Flat", 1, 25000, "none")],
    "43-01": [("Visual effects, product shots and sky", 1, "Allow", 1, 18000, "none")],
    "44-01": [("Versions and cutdowns, 15 and 6 seconds", 4, "Flat", 1, 1250, "none")],
    "50-01": [("Production insurance", 1, "Allow", 1, 6500, "none")],
    "52-01": [("Payroll service fees", 1, "Allow", 1, 2800, "none")],
}

# ---------------------------------------------------------------------------


def line(desc, amount, unit, x, rate, fr):
    return {"description": desc, "amount": str(amount), "unit": unit, "x": str(x),
            "rate": str(rate), "fringes": F[fr]}


def fill(m, path, title, globals_, lines):
    m.call("set_title", path=path, title=title)
    for name, formula, desc in globals_:
        m.call("set_global", path=path, name=name, formula=formula, description=desc)
    for name, fields in FRINGES:
        m.call("add_setup", path=path, type="fringe", name=name, fields=fields)
    for account, rows in lines.items():
        m.call("add_lines", path=path, account=account, use_defaults=False,
               lines=[line(*r) for r in rows])


def build(out, force):
    out.mkdir(parents=True, exist_ok=True)
    names = ["lanterns-draft-2.mthd", "lanterns-draft-3.mthd", "quarry-hill-104.mthd",
             "first-light-30.mthd"]
    for n in names:
        p = out / n
        if p.exists():
            if not force:
                sys.exit(f"make-showcase-budgets: {p} exists (use --force)")
            p.unlink()

    method("new", str(out / "lanterns-draft-3.mthd"), "--template", "feature")
    method("new", str(out / "quarry-hill-104.mthd"), "--template", "tv-one-hour")
    method("new", str(out / "first-light-30.mthd"), "--template", "commercial")

    m = Mcp(out)

    # The feature, draft 3.
    p = "lanterns-draft-3.mthd"
    fill(m, p, "Lanterns Over Kettle Lake, draft 3", FEATURE_GLOBALS, FEATURE_LINES)
    m.call("set_charge", path=p, name="Contingency", charge_kind="percent", percent=10,
           account="65-01")
    m.call("set_charge", path=p, name="Completion bond", charge_kind="percent", percent=2.5,
           account="64-01")
    m.call("set_credit", path=p, name="State production incentive", rows=[
        {"source": {"kind": "category", "category": c}, "percent": 25,
         "cap": {"kind": "none"}, "description": "Qualified local spend"}
        for c in ["20", "22", "23", "24", "25", "27", "28", "29", "30", "32", "33", "35",
                  "36", "37"]])

    # Draft 2: a copy with the earlier numbers.
    m.call("save_as", path=p, out="lanterns-draft-2.mthd")
    d2 = "lanterns-draft-2.mthd"
    m.call("set_title", path=d2, title="Lanterns Over Kettle Lake, draft 2")
    m.call("set_global", path=d2, name="SHOOT_DAYS", formula=DRAFT2_EDITS["SHOOT_DAYS"])
    accounts = {}
    for key, value in DRAFT2_EDITS.items():
        if not isinstance(key, tuple):
            continue
        account, idx, field = key
        if account not in accounts:
            accounts[account] = m.call("get_account", path=d2, account=account)
        rows = [r for r in accounts[account]["rows"] if r.get("kind", "line") == "line"]
        m.call("update_lines", path=d2, rows=[{"id": rows[idx]["id"], field: str(value)}])

    # The series episode.
    p = "quarry-hill-104.mthd"
    fill(m, p, "Quarry Hill, episode 104, \"The Long Field\"", TV_GLOBALS, TV_LINES)
    m.call("set_charge", path=p, name="Contingency", charge_kind="percent", percent=5,
           account="64-01")

    # The commercial.
    p = "first-light-30.mthd"
    fill(m, p, "Fernvale Oat Drink, \"First Light\" :30", SPOT_GLOBALS, SPOT_LINES)
    m.call("set_charge", path=p, name="Production fee", charge_kind="percent", percent=15,
           account="51-01")
    m.call("set_charge", path=p, name="Weather contingency", charge_kind="percent", percent=5,
           account="53-01")
    m.close()

    add_version_and_actuals(out)
    for n in names:
        method("fmt", str(out / n))
        method("check", str(out / n))
    for n in names:
        print(method("show", str(out / n)))


VERSION_DROP = {"versions", "compact_versions", "report_presets", "tag_presets", "line_presets",
                "ot_patterns", "display", "setup_order", "setup_folders", "setup_sources",
                "provenance", "actuals"}

# Actual spending to date (prep done, shooting week one): (account, share of its budget,
# vendor or payee, description). Invented payees.
ACTUALS = [
    ("10-01", 1.0, "Kettle Lake Rights LLC", "Option exercise and purchase"),
    ("11-01", 0.85, "Writer payroll", "Screenplay fee, steps one and two"),
    ("12-02", 0.6, "Producer payroll", "Producer fees, first installments"),
    ("13-01", 0.55, "Director payroll", "Director fee, prep and first week"),
    ("14-01", 0.5, "Cast payroll", "Lead cast, first half"),
    ("14-04", 1.0, "North Fork Casting", "Casting package"),
    ("20-01", 0.62, "Crew payroll", "Prep and week one"),
    ("20-02", 0.55, "Crew payroll", "Prep and week one"),
    ("20-05", 0.6, "Crew payroll", "Prep and week one"),
    ("22-01", 0.7, "Crew payroll", "Prep and week one"),
    ("22-08", 0.93, "Various", "Art purchases, petty cash"),
    ("23-06", 1.12, "Bramble Lumber", "Lumber and hardware, boathouse set"),
    ("24-05", 0.74, "Various", "Set dressing purchases"),
    ("27-01", 0.4, "Crew payroll", "Prep and week one"),
    ("27-08", 0.25, "Lakeside Camera Rentals", "Camera package, week one"),
    ("28-06", 0.22, "Lakeside Grip and Lighting", "Lighting package, week one"),
    ("32-06", 0.81, "Various", "Wardrobe purchases"),
    ("35-04", 0.5, "Kettle Lake Township", "Site fees, first six days"),
    ("35-05", 1.0, "Kettle Lake Township", "Film permit"),
    ("36-04", 0.3, "Two Pines Vehicle Rental", "Production vehicles, weeks one and two"),
    ("37-01", 0.21, "Hearth and Field Catering", "Catering, week one"),
    ("38-03", 0.67, "Mill Street Properties", "Office rent, two months"),
    ("60-01", 1.0, "Insurance broker", "Production package premium"),
    ("61-01", 0.45, "Production counsel", "Legal, prep"),
]


def add_version_and_actuals(out):
    d2 = json.loads((out / "lanterns-draft-2.mthd").read_text())
    d3path = out / "lanterns-draft-3.mthd"
    d3 = json.loads(d3path.read_text())
    snap = {k: v for k, v in d2["budget"].items() if k not in VERSION_DROP}
    d3["budget"]["versions"] = [{"id": 1, "name": "Draft 2", "created": "2026-09-18T17:40:00-07:00",
                                 "note": "Sent to financiers", "budget": snap}]
    d3path.write_text(json.dumps(d3, indent=2) + "\n")

    totals = json.loads(method("totals", str(d3path), "--json"))
    acct = {a["code"]: a["total"] for c in totals["categories"] for a in c["accounts"]}

    def budget_cents(code):
        return acct[code]

    tx = []
    for i, (code, share, vendor, desc) in enumerate(ACTUALS, start=1):
        cents = round(budget_cents(code) * share)
        day = 1 + (i * 7) % 28
        month = 9 if i < 16 else 10
        tx.append({"id": i, "date": f"2026-{month:02d}-{day:02d}", "account": code,
                   "vendor": vendor, "description": desc, "reference": f"INV-{3100 + i * 7}",
                   "amount": cents, "batch": 1})
    d3 = json.loads(d3path.read_text())
    d3["budget"]["actuals"] = {
        "batches": [{"id": 1, "file": "ledger-through-week-one.csv",
                     "imported": "2026-10-09T09:15:00-07:00", "rows": len(tx)}],
        "transactions": tx,
        "etc": {"23-06": {"kind": "manual", "amount": 0},
                "27-08": {"kind": "percent_complete", "percent": "25"}},
    }
    d3path.write_text(json.dumps(d3, indent=2) + "\n")


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if a != "--force"]
    out = Path(args[0]) if args else REPO / "target" / "showcase"
    if not Path(METHOD).exists():
        sys.exit(f"make-showcase-budgets: no {METHOD} (cargo build --release -p method-cli)")
    build(out.resolve(), "--force" in sys.argv)
