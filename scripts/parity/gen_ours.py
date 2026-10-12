#!/usr/bin/env python3
"""Parity test, our side (docs/research/parity-test-plan.md, section 5.1).

Turns the section 4 inputs tables into `.mthd` budgets, one per budget and per
variation our format can express, plus `cases.json`: for every budget key, every case
id with its account code, row position (1-based, counting every row of the account,
subtotal and adjustment rows included, as `method explain 1101:3` counts), row id,
kind and description. The comparison matches MMB's rows by account and position and
checks the description.

Only our own invented inputs live here (clean room: no MMB data).

Usage: python3 -I scripts/parity/gen_ours.py <out-dir>
"""
import copy
import json
import os
import sys

# Account coding: four-digit codes typed whole (category 1100, account 1101), no
# separator. The plan's C2 step records MMB's real coding; until a run records it this
# is the plan's assumption (fringe accounts then read 1199, 1299).
CODING = {"category_width": 2, "account_width": 2, "sub_width": 0, "set_width": 0, "separator": ""}

# MMB's starter units with the hours the plan expects C2 to confirm (never our defaults).
STARTER_UNITS = {
    "Allow": {"plural": "Allow"},
    "Day": {"plural": "Days", "hours": "12"},
    "Flat": {"plural": "Flat"},
    "Hour": {"plural": "Hours", "hours": "1"},
    "Month": {"plural": "Months", "hours": "240"},
    "Week": {"plural": "Weeks", "hours": "60"},
}

# Every copy-from-line-above switch off (plan 2.3 step 3).
NEW_LINE_DEFAULTS = {"fringes": False, "groups": False, "set": False, "location": False, "currency": False}

_next_id = [0]


def nid():
    _next_id[0] += 1
    return _next_id[0]


def L(case, desc, amount="", unit="", rate="", x="", unit2="", unit3="", x4="", unit4="",
      currency=None, fringes=(), groups=(), set_=None, loc=None, rng=None, kind="line"):
    return {"case": case, "id": nid(), "kind": kind, "description": desc, "amount": amount,
            "unit": unit, "x": x, "unit2": unit2, "rate": rate, "unit3": unit3, "x4": x4,
            "unit4": unit4, "currency": currency, "fringes": sorted(fringes),
            "groups": sorted(groups), "set": set_, "location": loc, "fringe_range": rng}


def SUB(case, desc, rng=None):
    return L(case, desc, kind="subtotal", rng=rng)


def ADJ(case, desc, amount, x="", x4=""):
    return L(case, desc, amount=amount, x=x, x4=x4, kind="adjustment")


def detail_json(d):
    o = {"id": d["id"]}
    if d["kind"] != "line":
        o["kind"] = d["kind"]
    o["description"] = d["description"]
    for k in ["amount", "unit", "x", "unit2", "rate", "unit3", "x4", "unit4"]:
        if d[k] != "":
            o[k] = d[k]
    if d["currency"] is not None:
        o["currency"] = d["currency"]
    if d["fringe_range"] is not None:
        o["fringe_range"] = d["fringe_range"]
    if d["fringes"]:
        o["fringes"] = d["fringes"]
    if d["groups"]:
        o["groups"] = d["groups"]
    if d["set"]:
        o["set"] = d["set"]
    if d["location"]:
        o["location"] = d["location"]
    return o


def budget(title, topsheet, units=None, globals_=None, global_info=None, fringes=None,
           groups=None, sets=None, locations=None, currencies=None, charges=None,
           credits=None, posting=None, totals_currency=None, coding=None):
    b = {"title": title, "base_currency": "USD"}
    if totals_currency:
        b["totals_currency"] = totals_currency
    b["account_coding"] = coding or CODING
    b["fringe_posting"] = posting if posting is not None else {"kind": "category", "suffix": "99"}
    b["new_line_defaults"] = NEW_LINE_DEFAULTS
    if currencies:
        b["currencies"] = currencies
    b["units"] = units if units is not None else STARTER_UNITS
    if globals_:
        b["globals"] = globals_
    if global_info:
        b["global_info"] = global_info
    if fringes:
        b["fringes"] = fringes
    if groups:
        b["groups"] = groups
    if sets:
        b["sets"] = sets
    if locations:
        b["locations"] = locations
    ts = []
    for row in topsheet:
        if row[0] == "PT":
            ts.append({"kind": "production_total", "name": row[1]})
        else:
            _, code, name, accounts = row
            accs = [{"code": a, "name": n, "details": [detail_json(d) for d in ds]} for a, n, ds in accounts]
            ts.append({"kind": "category", "code": code, "name": name, "accounts": accs})
    b["topsheet"] = ts
    if charges:
        b["charges"] = charges
    if credits:
        b["credits"] = credits
    return {"format": "method-budget", "version": 1, "budget": b}


def pct(rate, cutoff=None):
    f = {"kind": "percent", "description": "Z test fringe", "rate": rate}
    if cutoff is not None:
        f["cutoff"] = cutoff
    return f


def flat(rate, unit=None, cutoff=None, max_hours=None):
    f = {"kind": "flat", "description": "Z test fringe", "rate": rate}
    if unit:
        f["unit"] = unit
    if max_hours is not None:
        f["max_hours_per_day"] = max_hours
    if cutoff is not None:
        f["cutoff"] = cutoff
    return f


SPEC = {}  # key -> (file json, topsheet)


def register(key, b, topsheet):
    SPEC[key] = (b, topsheet)


def find(topsheet, case):
    for row in topsheet:
        if row[0] == "PT":
            continue
        for _, _, ds in row[3]:
            for d in ds:
                if d["case"] == case:
                    return d
    raise KeyError(case)


# ---------------------------------------------------------------- ZTEST01 LINE MATH (4.1)
def ts01():
    return [
        ("C", "1100", "ZLINES A", [
            ("1101", "Z factors", [
                L("LM-01", "Two allow", "2", "Allow", "100"),
                L("LM-02", "Two by three", "2", "", "100", x="3"),
                L("LM-03", "Four factors", "2", "", "100", x="3", x4="4"),
                L("LM-04", "Units are labels", "2", "Day", "100", x="3", unit2="Week", unit3="Allow", x4="1", unit4="Month"),
                L("LM-05", "Blank amount", "", "", "50"),
                L("LM-06", "Blank rate", "1", "", ""),
                L("LM-07", "Zero amount", "0", "", "100"),
                L("LM-08", "Zero X", "1", "", "100", x="0"),
                L("LM-09", "ZTITLE ONLY", "", "", ""),
                L("LM-10", "Negative amount", "-2", "", "100"),
                L("LM-11", "Negative rate", "2", "", "-100"),
                L("LM-12", "Both negative", "-2", "", "-100"),
            ]),
            ("1102", "Z decimals", [
                L("LM-13", "Half cent", "3", "", "432.105"),
                L("LM-14", "Float trap 1.005", "1", "", "1.005"),
                L("LM-15", "Float trap 2.675", "1", "", "2.675"),
                L("LM-16", "Even half 0.125", "1", "", "0.125"),
                L("LM-17", "Float trap 0.145", "1", "", "0.145"),
                L("LM-18", "Negative half", "-1", "", "0.125"),
                L("LM-19", "Product half", "3", "", "0.335"),
                L("LM-20", "Nine places", "1000", "", "0.123456789"),
                L("LM-21", "Below a cent", "0.5", "", "0.009999999"),
                L("LM-22", "Decimal factors", "2.5", "", "33.33", x="1.5"),
                L("LM-23", "Fraction amount", "1/3", "", "30000000.015"),
            ]),
        ]),
        ("PT", "ZTOTAL ATL"),
        ("C", "1200", "ZLINES B", [
            ("1201", "Z formulas and size", [
                L("LM-25", "Formula amount", "2*(3+4)", "", "10"),
                L("LM-26", "Division amount", "10/4", "", "10"),
                L("LM-27", "Big rate", "1", "", "999999999999.99"),
                L("LM-28", "Big half cent", "3", "", "3333333333333.335"),
                L("LM-29", "Big negative", "-1", "", "999999999999.99"),
            ]),
        ]),
        ("PT", "ZTOTAL BTL"),
    ]


t = ts01()
register("ZTEST01", budget("ZTEST01 LINE MATH", t), t)
# V01-2 (P2): LM-02 with 4X typed 1 and X cleared.
t = ts01()
d = find(t, "LM-02")
d["x4"], d["x"] = "1", ""
register("ZTEST01-V01-2", budget("ZTEST01 LINE MATH", t), t)

# ---------------------------------------------------------------- ZTEST02 GLOBALS (4.2)
def globals02(a="3"):
    g = {"ZG_A": a, "ZG_B": "ZG_A * 5 + 2", "ZG_C": "ZG_B / 3", "ZG_RATE": "312.34",
         "ZG_PCT": "7.5", "ZG_FLAT": "25", "ZG_W": "2",
         "ZG_PAREN": "((ZG_A + 1) * (ZG_B - 7)) / (2 * (ZG_A - 1))", "ZG_DEL": "4",
         "ZG_NEG": "-ZG_A", "ZG_L1": "2"}
    for i in range(2, 7):
        g[f"ZG_L{i}"] = f"ZG_L{i - 1} * 2"
    return g


def ts02():
    return [
        ("C", "1100", "ZGLOBALS A", [
            ("1101", "Z globals", [
                L("G-01", "Global amount", "ZG_B", "Day", "10"),
                L("G-02", "Global division", "ZG_C", "", "15000000"),
                L("G-03", "Deep chain", "ZG_L6", "", "0.5"),
                L("G-04", "Formula with globals", "ZG_A*(ZG_B-2)/5", "", "11.11"),
                L("G-05", "Global rate and X", "1", "", "ZG_RATE", x="ZG_A"),
                L("G-06", "Parentheses", "ZG_PAREN", "", "1.5"),
                L("G-07", "Unary minus", "ZG_NEG", "", "10"),
                L("G-08", "Global with a unit", "ZG_W", "", "100"),
                L("G-09", "Global fringe rate", "1", "", "1000", fringes=["ZFG"]),
                L("G-10", "Global flat rate", "4", "Day", "50", fringes=["ZFF"]),
                L("G-11", "To be frozen", "ZG_DEL", "", "25"),
            ]),
        ]),
        ("PT", "ZTOTAL ATL"),
        ("C", "1200", "ZGLOBALS B", [
            ("1201", "Z global spare", [L("G-13", "Plain line", "1", "", "100")]),
        ]),
        ("PT", "ZTOTAL BTL"),
    ]


F2 = {"ZFG": pct("ZG_PCT"), "ZFF": flat("ZG_FLAT", "Day")}
GI2 = {"ZG_W": {"unit": "Week"}}
t = ts02()
register("ZTEST02", budget("ZTEST02 GLOBALS", t, globals_=globals02(), global_info=GI2, fringes=F2), t)
t = ts02()
register("ZTEST02-V02-1", budget("ZTEST02 GLOBALS", t, globals_=globals02("4"), global_info=GI2, fringes=F2), t)
# V02-2: ZG_DEL deleted; our delete freezes the value into the cell (decision 12).
t = ts02()
find(t, "G-11")["amount"] = "4"
g = globals02()
del g["ZG_DEL"]
register("ZTEST02-V02-2", budget("ZTEST02 GLOBALS", t, globals_=g, global_info=GI2, fringes=F2), t)
# V02-5 (P2): ZG_RATE shown with 0 decimals (display only).
t = ts02()
register("ZTEST02-V02-5", budget("ZTEST02 GLOBALS", t, globals_=globals02(),
                                 global_info={"ZG_W": {"unit": "Week"}, "ZG_RATE": {"decimals": 0}}, fringes=F2), t)

# ---------------------------------------------------------------- ZTEST03 SUBTOTALS AND ADJUSTMENTS (4.3)
t = [
    ("C", "1100", "ZADJUST A", [
        ("1101", "Z adjust windows", [
            L("A-01", "Line one", "1", "", "1000", fringes=["ZP10N"]),
            L("A-02", "Line two", "1", "", "2000", fringes=["ZP10N"]),
            SUB("A-03", "ZSUB 1"),
            L("A-04", "Line three", "1", "", "500"),
            ADJ("A-05", "ZADJ 10 percent", "0.1"),
            L("A-06", "Line four", "1", "", "400"),
            ADJ("A-07", "ZADJ 5 percent", "0.05"),
            L("A-07b", "Line four b", "1", "", "250"),
            SUB("A-08", "ZSUB 2"),
            ADJ("A-09", "ZADJ restart", "0"),
            L("A-10", "Line five", "1", "", "1234.5"),
            ADJ("A-11", "ZADJ half cent", "0.01"),
            L("A-12", "Line six", "1", "", "100"),
            ADJ("A-13", "ZADJ discount", "-0.1"),
            L("A-14", "Line seven", "1", "", "300"),
            ADJ("A-15", "ZADJ X and 4X", "0.1", x="2", x4="1.5"),
        ]),
        ("1102", "Z adjust fringe", [
            L("A-16", "Line eight", "1", "", "1000"),
            ADJ("A-17", "ZADJ fringed", "0.1"),
        ]),
        ("1103", "Z adjust at top", [
            ADJ("A-18", "ZADJ first row", "0.1"),
            L("A-18b", "Line nine", "1", "", "500"),
        ]),
    ]),
    ("PT", "ZTOTAL ATL"),
    ("C", "1200", "ZADJUST B", [
        ("1201", "Z subtotals only", [
            L("A-19", "Line ten", "1", "", "250"),
            SUB("A-20", "ZSUB 3"),
            SUB("A-21", "ZSUB 4"),
            L("A-22", "Line eleven", "1", "", "750"),
        ]),
    ]),
    ("PT", "ZTOTAL BTL"),
]
register("ZTEST03", budget("ZTEST03 SUBTOTALS AND ADJUSTMENTS", t, fringes={"ZP10N": pct("10")}), t)

# ---------------------------------------------------------------- ZTEST04 PERCENT FRINGES (4.4)
F4 = {"ZP125": pct("12.5", 222222), "ZP321": pct("3.21"), "ZP10": pct("10", 100000),
      "ZPG": pct("ZG_PCT"), "ZP0": pct("0")}
t = [
    ("C", "1100", "ZPERCENT A", [
        ("1101", "Z single earners", [
            L("P-01", "Below cutoff", "1", "", "1000", fringes=["ZP125"]),
            L("P-02", "Above cutoff", "1", "", "5000", fringes=["ZP125"]),
            L("P-03", "At cutoff", "1", "", "2222.22", fringes=["ZP125"]),
            L("P-04", "No cutoff", "1", "", "1234.56", fringes=["ZP321"]),
            L("P-05", "Three fringes", "1", "", "1000", fringes=["ZP125", "ZP321", "ZP10"]),
            L("P-06", "Half cent fringe", "1", "", "0.04", fringes=["ZP125"]),
            L("P-07", "Negative line", "1", "", "-1000", fringes=["ZP125"]),
            L("P-08", "Zero line", "1", "", "0", fringes=["ZP125"]),
            L("P-09", "Global rate", "1", "", "1000", fringes=["ZPG"]),
            L("P-10", "Factor tie breaker", "1", "", "3703.68", fringes=["ZP125"]),
            L("P-11", "Zero percent", "1", "", "1000", fringes=["ZP0"]),
        ]),
        ("1102", "Z range one earner", [
            L("P-12", "Prep", "4", "Day", "312.34", fringes=["ZP125"], rng=1),
            L("P-13", "Shoot", "17", "Day", "312.34", fringes=["ZP125"], rng=1),
            L("P-14", "Wrap", "2", "Day", "312.34", fringes=["ZP125"], rng=1),
        ]),
        ("1103", "Z same lines no range", [
            L("P-15", "Prep", "4", "Day", "312.34", fringes=["ZP125"]),
            L("P-16", "Shoot", "17", "Day", "312.34", fringes=["ZP125"]),
            L("P-17", "Wrap", "2", "Day", "312.34", fringes=["ZP125"]),
        ]),
        ("1104", "Z range across subtotal", [
            L("P-18", "Before", "1", "", "1000", fringes=["ZP10"], rng=1),
            SUB("P-19", "ZSUB R"),
            L("P-20", "After", "1", "", "1000", fringes=["ZP10"], rng=1),
        ]),
        ("1105", "Z range mixed fringes", [
            L("P-21", "Both fringes", "1", "", "1500", fringes=["ZP125", "ZP321"], rng=1),
            L("P-22", "One fringe", "1", "", "1500", fringes=["ZP125"], rng=1),
        ]),
        ("1106", "Z range negative line", [
            L("P-23", "Pay", "1", "", "3000", fringes=["ZP10"], rng=1),
            L("P-24", "Refund", "1", "", "-500", fringes=["ZP10"], rng=1),
        ]),
    ]),
    ("PT", "ZTOTAL ATL"),
    ("C", "1200", "ZPERCENT B", [
        ("1201", "Z range suppressed middle", [
            L("P-25", "First", "1", "", "1500", fringes=["ZP10"], rng=1),
            L("P-26", "Middle out", "1", "", "1500", fringes=["ZP10"], groups=["ZOFF"], rng=1),
            L("P-27", "Last", "1", "", "1500", fringes=["ZP10"], rng=1),
        ]),
        ("1202", "Z two ranges", [
            L("P-28", "A one", "1", "", "1500", fringes=["ZP10"], rng=1),
            L("P-29", "A two", "1", "", "1500", fringes=["ZP10"], rng=1),
            L("P-30", "B one", "1", "", "1500", fringes=["ZP10"], rng=2),
            L("P-31", "B two", "1", "", "1500", fringes=["ZP10"], rng=2),
        ]),
        ("1203", "Z range partial fringe", [
            L("P-32", "Fringed", "1", "", "600", fringes=["ZP10"], rng=1),
            L("P-33", "Not fringed", "1", "", "600", rng=1),
            L("P-34", "Fringed again", "1", "", "600", fringes=["ZP10"], rng=1),
        ]),
        ("1204", "Z range rounding", [
            L("P-35", "Tiny one", "1", "", "0.04", fringes=["ZP125"], rng=1),
            L("P-36", "Tiny two", "1", "", "0.04", fringes=["ZP125"], rng=1),
            L("P-37", "Tiny three", "1", "", "0.04", fringes=["ZP125"], rng=1),
        ]),
    ]),
    ("PT", "ZTOTAL BTL"),
]
register("ZTEST04", budget("ZTEST04 PERCENT FRINGES", t, globals_={"ZG_PCT": "7.5"}, fringes=F4,
                           groups={"ZOFF": {"in_budget_total": False}}), t)

# ---------------------------------------------------------------- ZTEST05 FLAT FRINGES (4.5)
U5 = dict(STARTER_UNITS)
U5.update({"ZSHIFT": {"plural": "ZSHIFTS", "hours": "11"}, "ZHALF": {"plural": "ZHALFS", "hours": "7.5"},
           "ZEPI": {"plural": "ZEPIS"}, "ZZERO": {"plural": "ZZEROS", "hours": "0"}})


def fringes05(max_hours=None):
    return {"ZFDAY": flat("100", "Day"), "ZFHR": flat("1", "Hour", max_hours=max_hours),
            "ZFWK": flat("500", "Week"), "ZFMO": flat("2000", "Month"), "ZFALW": flat("25", "Allow"),
            "ZFFLAT": flat("30", "Flat"), "ZFSHIFT": flat("50", "ZSHIFT"), "ZFTINY": flat("0.06", "Day"),
            "ZFG": flat("ZG_FLAT", "Day"), "ZFCUT": flat("100", "Day", cutoff=15000), "ZP10N": pct("10")}


def ts05():
    return [
        ("C", "1100", "ZFLAT A", [
            ("1101", "Z rechecked shapes", [
                L("F-01", "25 days", "25", "Day", "100", fringes=["ZFDAY"]),
                L("F-02", "25 days x 3", "25", "Day", "100", x="3", fringes=["ZFDAY"]),
                L("F-03", "25 days x 14 hours", "25", "Day", "100", x="14", unit2="Hour", fringes=["ZFHR"]),
                L("F-04", "25 days x 14 hours x 2", "25", "Day", "10", x="14", unit2="Hour", x4="2", fringes=["ZFHR"]),
                L("F-05", "2 weeks", "2", "Week", "100", fringes=["ZFDAY"]),
                L("F-06", "300 hours", "300", "Hour", "100", fringes=["ZFDAY"]),
                L("F-07", "2 allow", "2", "Allow", "100", fringes=["ZFDAY"]),
            ]),
            ("1102", "Z conversions", [
                L("F-08", "1 month per day", "1", "Month", "100", fringes=["ZFDAY"]),
                L("F-09", "3 weeks per month", "3", "Week", "100", fringes=["ZFMO"]),
                L("F-10", "10 days per week", "10", "Day", "100", fringes=["ZFWK"]),
                L("F-11", "7 days per week", "7", "Day", "100", fringes=["ZFWK"]),
                L("F-12", "1 day per shift", "1", "Day", "100", fringes=["ZFSHIFT"]),
                L("F-13", "1 shift per day", "1", "ZSHIFT", "100", fringes=["ZFDAY"]),
                L("F-14", "3 half shifts per hour", "3", "ZHALF", "100", fringes=["ZFHR"]),
                L("F-15", "2.5 days", "2.5", "Day", "100", fringes=["ZFDAY"]),
                L("F-16", "Half day per hour", "0.5", "Day", "100", fringes=["ZFHR"]),
                L("F-17", "1 hour per day", "1", "Hour", "100", fringes=["ZFDAY"]),
                L("F-18", "Third of a day", "1/3", "Day", "100", fringes=["ZFDAY"]),
                L("F-19", "Half cent flat", "1", "Hour", "100", fringes=["ZFTINY"]),
            ]),
            ("1103", "Z charged once", [
                L("F-20", "2 episodes", "2", "ZEPI", "100", fringes=["ZFDAY"]),
                L("F-21", "4 no unit", "4", "", "100", fringes=["ZFDAY"]),
                L("F-22", "4 days per allow", "4", "Day", "100", fringes=["ZFALW"]),
                L("F-23", "4 days per flat", "4", "Day", "100", fringes=["ZFFLAT"]),
                L("F-24", "1 zero hour unit", "1", "ZZERO", "100", fringes=["ZFDAY"]),
            ]),
            ("1104", "Z edge amounts", [
                L("F-25", "Minus 2 days", "-2", "Day", "100", fringes=["ZFDAY"]),
                L("F-26", "Zero days", "0", "Day", "100", fringes=["ZFDAY"]),
                L("F-27", "25 days rate 0", "25", "Day", "0", fringes=["ZFDAY"]),
                L("F-28", "1 day x 0", "1", "Day", "100", x="0", fringes=["ZFDAY"]),
                L("F-29", "Global days", "ZG_DAYS", "Day", "100", fringes=["ZFDAY"]),
                L("F-30", "Global flat rate", "4", "Day", "100", fringes=["ZFG"]),
                L("F-31", "Flat with cutoff", "2", "Day", "100", fringes=["ZFCUT"]),
                L("F-32", "Flat and percent", "5", "Day", "200", fringes=["ZFDAY", "ZP10N"]),
            ]),
        ]),
        ("PT", "ZTOTAL ATL"),
        ("C", "1200", "ZFLAT B", [
            ("1201", "Z flat in a range", [
                L("F-33", "Range ten days", "10", "Day", "100", fringes=["ZFDAY"], rng=1),
                L("F-34", "Range fifteen days", "15", "Day", "100", fringes=["ZFDAY"], rng=1),
            ]),
            ("1202", "Z flat suppressed", [
                L("F-35", "Suppressed 4 days", "4", "Day", "100", fringes=["ZFDAY"], groups=["ZOFF"]),
            ]),
        ]),
        ("PT", "ZTOTAL BTL"),
    ]


G5 = {"ZG_FLAT": "25", "ZG_DAYS": "17"}
GR5 = {"ZOFF": {"in_budget_total": False}}
t = ts05()
register("ZTEST05", budget("ZTEST05 FLAT FRINGES", t, units=U5, globals_=G5, fringes=fringes05(), groups=GR5), t)
t = ts05()
register("ZTEST05-V05-1", budget("ZTEST05 FLAT FRINGES", t, units=U5, globals_=G5, fringes=fringes05("10"), groups=GR5), t)
t = ts05()
register("ZTEST05-V05-2", budget("ZTEST05 FLAT FRINGES", t, units=U5, globals_=G5, fringes=fringes05(), groups=GR5,
                                 posting={"kind": "budget"}), t)

# ---------------------------------------------------------------- ZTEST06 FRINGE POSTING (4.6)
def ts06():
    return [
        ("C", "1100", "ZATL", [
            ("1101", "Z atl one", [L("PM-01", "Star", "1", "", "10000", fringes=["ZP10N"])]),
            ("1102", "Z atl two", [L("PM-02", "Writer", "1", "", "5000", fringes=["ZP10N"])]),
        ]),
        ("PT", "ZTOTAL ATL"),
        ("C", "2100", "ZPROD", [
            ("2101", "Z prod one", [L("PM-03", "Grip", "1", "", "8000", fringes=["ZP10N"])]),
            ("2102", "Z prod two", [L("PM-04", "Gear", "1", "", "2000")]),
        ]),
        ("PT", "ZTOTAL PROD"),
        ("C", "3100", "ZPOST", [
            ("3101", "Z post one", [
                L("PM-05", "Editor", "1", "", "3000", fringes=["ZP10N"]),
                L("PM-06", "Assistant", "4", "Day", "100", fringes=["ZFDAY"]),
            ]),
        ]),
        ("PT", "ZTOTAL POST"),
        ("C", "4100", "ZOTHER", [
            ("4101", "Z other one", [L("PM-07", "Insurance", "1", "", "1000", fringes=["ZP10N"])]),
        ]),
    ]


F6 = {"ZP10N": pct("10"), "ZFDAY": flat("100", "Day")}
CH6 = [{"kind": "percent", "name": "ZPCT5", "percent": "5"}]
for key, posting in [("ZTEST06", None), ("ZTEST06-V06-1", {"kind": "account"}),
                     ("ZTEST06-V06-2", {"kind": "production_level"}), ("ZTEST06-V06-3", {"kind": "budget"}),
                     ("ZTEST06-V06-4", {"kind": "category", "suffix": "98"})]:
    t = ts06()
    register(key, budget("ZTEST06 FRINGE POSTING", t, fringes=F6, charges=copy.deepcopy(CH6), posting=posting), t)

# ---------------------------------------------------------------- ZTEST07 GROUPS, SETS AND LOCATIONS (4.7)
def ts07():
    return [
        ("C", "1100", "ZGROUPS A", [
            ("1101", "Z groups", [
                L("GR-01", "In", "1", "", "1000", fringes=["ZP10N"], groups=["ZIN"], set_="ZSET1", loc="ZLOC1"),
                L("GR-02", "Out", "1", "", "2000", fringes=["ZP10N"], groups=["ZOFF"], set_="ZSET2"),
                L("GR-03", "In and out", "1", "", "3000", groups=["ZIN", "ZOFF"]),
                L("GR-04", "In twice", "1", "", "4000", fringes=["ZP10N"], groups=["ZIN", "ZIN2"], set_="ZSET1"),
                L("GR-05", "No group", "1", "", "500", loc="ZLOC1"),
                SUB("GR-06", "ZSUB G"),
                ADJ("GR-07", "ZADJ G", "0.1"),
            ]),
        ]),
        ("PT", "ZTOTAL ATL"),
        ("C", "1200", "ZGROUPS B", [
            ("1201", "Z groups B", [L("GR-08", "In two", "1", "", "600", groups=["ZIN2"], loc="ZLOC2")]),
        ]),
        ("PT", "ZTOTAL BTL"),
    ]


OFF = {"in_budget_total": False}
for key, gr in [("ZTEST07", {"ZIN": {}, "ZIN2": {}, "ZOFF": dict(OFF)}),
                ("ZTEST07-V07-1", {"ZIN": {}, "ZIN2": {}, "ZOFF": {}}),
                ("ZTEST07-V07-1b", {"ZIN": {}, "ZIN2": {}, "ZOFF": dict(OFF)}),
                ("ZTEST07-V07-2", {"ZIN": dict(OFF), "ZIN2": {}, "ZOFF": dict(OFF)})]:
    t = ts07()
    register(key, budget("ZTEST07 GROUPS SETS AND LOCATIONS", t, fringes={"ZP10N": pct("10")}, groups=gr,
                         sets={"ZSET1": {"description": "Set one"}, "ZSET2": {"description": "Set two"}},
                         locations={"ZLOC1": {"description": "Loc one", "city": "Testville",
                                              "region": "Test Region", "country": "Testland"},
                                    "ZLOC2": {"description": "Loc two"}}), t)

# ---------------------------------------------------------------- ZTEST08 CURRENCIES (4.8)
CUR = {"EUR": {"rate": "3"}, "GBP": {"rate": "1.23457"}, "JPY": {"rate": "0.00667"}, "CAD": {"rate": "1.234565"}}


def ts08():
    return [
        ("C", "1100", "ZCURRENCY", [
            ("1101", "Z foreign lines", [
                L("CU-01", "Euro lens", "5", "", "100", currency="EUR"),
                L("CU-02", "Pound line", "1", "", "100", currency="GBP"),
                L("CU-03", "Pound 4X", "1", "", "1", x4="3", currency="GBP"),
                L("CU-04", "Euro rounding", "1", "", "0.335", currency="EUR"),
                L("CU-05", "Yen line", "1", "", "10000", currency="JPY"),
                L("CU-06", "Six place rate", "1", "", "100000", currency="CAD"),
                L("CU-07", "Euro percent fringe", "1", "", "1000", currency="EUR", fringes=["ZP10N"]),
                L("CU-08", "Euro cutoff fringe", "1", "", "1000", currency="EUR", fringes=["ZP125"]),
                L("CU-09", "Euro flat fringe", "4", "Day", "100", currency="EUR", fringes=["ZFDAY"]),
                ADJ("CU-10", "ZADJ foreign", "0.1"),
                L("CU-11", "Base line", "1", "", "1000"),
            ]),
        ]),
        ("PT", "ZTOTAL ATL"),
        ("C", "1200", "ZCURRENCY B", [
            ("1201", "Z base only", [L("CU-12", "Base line two", "1", "", "500")]),
        ]),
        ("PT", "ZTOTAL BTL"),
    ]


F8 = {"ZP10N": pct("10"), "ZP125": pct("12.5", 222222), "ZFDAY": flat("25", "Day")}
for key, tc in [("ZTEST08", None), ("ZTEST08-V08-1", "EUR"), ("ZTEST08-V08-2", "GBP")]:
    t = ts08()
    register(key, budget("ZTEST08 CURRENCIES", t, currencies=CUR, fringes=F8, totals_currency=tc), t)

# ---------------------------------------------------------------- ZTEST09 CONTRACTUAL CHARGES (4.9)
def ts09():
    return [
        ("C", "1100", "ZATL", [
            ("1101", "Z star", [L("CH-01", "Star", "1", "", "10000", fringes=["ZP10N"])]),
            ("1102", "Z rights", [L("CH-02", "Rights", "1", "", "500", groups=["ZNOCHG"])]),
        ]),
        ("PT", "ZTOTAL ATL"),
        ("C", "2100", "ZBTL A", [
            ("2101", "Z crew", [
                L("CH-03", "Grip", "1", "", "8000", fringes=["ZP10N"]),
                L("CH-04", "Kit", "1", "", "300", groups=["ZNOCHG", "ZX2"]),
            ]),
        ]),
        ("C", "3100", "ZBTL B", [
            ("3101", "Z gear", [L("CH-05", "Gear", "1", "", "2000", groups=["ZX2"])]),
        ]),
        ("PT", "ZTOTAL BTL"),
    ]


def charges09(zflat_last=False, labels=False):
    c = [
        {"kind": "flat", "name": "ZFLAT", "amount": 250000,
         "excluded_amounts": [{"description": "ZEXCL B", "amount": 22200}], "excluded_groups": ["ZNOCHG"]},
        {"kind": "percent", "name": "ZPCT5", "percent": "5"},
        {"kind": "percent", "name": "ZPCTX", "percent": "10",
         "excluded_amounts": [{"description": "ZEXCL A", "amount": 11100}], "excluded_groups": ["ZNOCHG"]},
        {"kind": "percent", "name": "ZPCTCAT", "percent": "2", "excluded_categories": ["2100"]},
        {"kind": "percent", "name": "ZPCTACC", "percent": "3", "excluded_accounts": ["1102"]},
        {"kind": "percent", "name": "ZOVER", "percent": "10", "excluded_groups": ["ZNOCHG", "ZX2"]},
        {"kind": "flat", "name": "ZFLATEUR", "amount": 100000, "currency": "EUR"},
        {"kind": "flat", "name": "ZFLATCAT", "amount": 1000000, "excluded_categories": ["3100"]},
        {"kind": "percent", "name": "ZHALF", "percent": "0.4425"},
    ]
    if zflat_last:
        c.append(c.pop(0))
    if labels:
        c[[x["name"] for x in c].index("ZPCTX")].update({"label_percent": True, "label_excluded": True})
    return c


for key, posting, gr, kw in [
        ("ZTEST09", None, {"ZNOCHG": {}, "ZX2": {}}, {}),
        ("ZTEST09-V09-1", {"kind": "budget"}, {"ZNOCHG": {}, "ZX2": {}}, {}),
        ("ZTEST09-V09-2", None, {"ZNOCHG": {}, "ZX2": {}}, {"zflat_last": True}),
        ("ZTEST09-V09-3", None, {"ZNOCHG": dict(OFF), "ZX2": {}}, {}),
        ("ZTEST09-V09-4", None, {"ZNOCHG": {}, "ZX2": {}}, {"labels": True})]:
    t = ts09()
    register(key, budget("ZTEST09 CONTRACTUAL CHARGES", t, fringes={"ZP10N": pct("10")}, groups=gr,
                         currencies={"EUR": {"rate": "3"}}, charges=charges09(**kw), posting=posting), t)

# ---------------------------------------------------------------- ZTEST10 CREDITS (4.10)
def ts10():
    return [
        ("C", "1100", "ZATL", [
            ("1101", "Z crew credit", [
                L("CR-01", "Grip days", "4", "Day", "1000", fringes=["ZP10N", "ZFDAY"], groups=["ZCRED"]),
                L("CR-02", "Suppressed grip", "1", "", "2000", fringes=["ZP10N"], groups=["ZCRED", "ZOFF"]),
            ]),
        ]),
        ("PT", "ZTOTAL ATL"),
        ("C", "2100", "ZBTL", [
            ("2101", "Z crew two", [
                L("CR-03", "Electric", "1", "", "3000", fringes=["ZP10N"], groups=["ZCRED2"]),
                L("CR-04", "Plain", "1", "", "1000"),
            ]),
        ]),
        ("PT", "ZTOTAL BTL"),
    ]


def credits10():
    return [
        {"name": "ZCR1", "account": "7901", "cap": 124000, "discount_percent": "2", "excluded_fringes": ["ZFDAY"], "rows": [
            {"description": "Group row", "source": {"kind": "group", "group": "ZCRED"}, "percent": "20"},
            {"description": "Flat credit cap", "source": {"kind": "flat", "amount": 100000}, "percent": "10",
             "cap": {"kind": "credit", "amount": 5000}},
            {"description": "Flat spend cap", "source": {"kind": "flat", "amount": 300000}, "percent": "15",
             "cap": {"kind": "qualified_spend", "amount": 200000}},
        ]},
        {"name": "ZCR2", "rows": [{"description": "Group two", "source": {"kind": "group", "group": "ZCRED2"}, "percent": "25"}]},
        {"name": "ZCR3", "rows": [{"description": "Odd percent", "source": {"kind": "flat", "amount": 123457}, "percent": "33.333"}]},
        {"name": "ZCR4", "rows": [{"description": "Half cent A", "source": {"kind": "flat", "amount": 5}, "percent": "10"},
                                  {"description": "Half cent B", "source": {"kind": "flat", "amount": 5}, "percent": "10"}]},
        {"name": "ZCR5", "discount_percent": "2.5",
         "rows": [{"description": "Discount half cent", "source": {"kind": "flat", "amount": 100100}, "percent": "10"}]},
        {"name": "ZCR6", "cap": 90000, "discount_percent": "10",
         "rows": [{"description": "Cap then discount", "source": {"kind": "flat", "amount": 1000000}, "percent": "10"}]},
    ]


F10 = {"ZP10N": pct("10"), "ZFDAY": flat("25", "Day")}
for key, posting, gr in [
        ("ZTEST10", None, {"ZCRED": {}, "ZCRED2": {}, "ZOFF": dict(OFF)}),
        ("ZTEST10-V10-1", {"kind": "budget"}, {"ZCRED": {}, "ZCRED2": {}, "ZOFF": dict(OFF)}),
        ("ZTEST10-V10-2", None, {"ZCRED": dict(OFF), "ZCRED2": {}, "ZOFF": dict(OFF)})]:
    t = ts10()
    register(key, budget("ZTEST10 CREDITS", t, fringes=F10, groups=gr, credits=credits10(), posting=posting), t)

# ---------------------------------------------------------------- ZTEST11 TOPSHEET LEVELS (4.11)
def ts11(layout, alpha=False):
    cats = {
        "1100": ("1100", "ZLEVEL ONE", [("1101", "Z one", [L("T-01", "One", "1", "", "1000", fringes=["ZP10N"])])]),
        "2100": ("2100", "ZLEVEL TWO", [("2101", "Z two", [L("T-02", "Two", "1", "", "2000")])]),
        "3100": ("3100", "ZLEVEL THREE", [("3101", "Z three", [L("T-03", "Three", "1", "", "3000")])]),
        "4100": ("4100", "ZLEVEL FOUR", [("4101", "Z four", [L("T-04", "Four", "1", "", "4000")])]),
    }
    if alpha:
        cats["1A00"] = ("1A00", "ZALPHA", [("1A01", "Z alpha", [L("T-07", "Alpha", "1", "", "100", fringes=["ZP10N"])])])
    out = []
    for item in layout:
        if item.startswith("PT "):
            out.append(("PT", item[3:]))
        else:
            code, name, accts = cats[item]
            out.append(("C", code, name, accts))
    return out


V3 = ["PT ZTOP", "1100", "PT ZTOTAL ATL", "2100", "PT ZTOTAL PROD", "3100", "PT ZTOTAL POST", "4100"]
for key, layout, alpha in [
        ("ZTEST11", ["1100", "2100", "3100", "4100"], False),
        ("ZTEST11-V11-1", ["1100", "PT ZTOTAL ATL", "2100", "3100", "4100"], False),
        ("ZTEST11-V11-2", ["1100", "PT ZTOTAL ATL", "2100", "PT ZTOTAL PROD", "3100", "PT ZTOTAL POST", "4100"], False),
        ("ZTEST11-V11-3", V3, False),
        ("ZTEST11-V11-6", V3[:2] + ["1A00"] + V3[2:], True)]:
    t = ts11(layout, alpha)
    register(key, budget("ZTEST11 TOPSHEET LEVELS", t, fringes={"ZP10N": pct("10")}), t)

# ---------------------------------------------------------------- ZTEST12 BASELINE, VERSIONS, SUB-BUDGETS (4.12)
def ts12(step):
    b01 = L("B-01", "Star", "1", "", "1500" if step == "S7" else "1000",
            fringes=[] if step == "S2" else ["ZP10N"], groups=["ZIN"])
    b02 = L("B-02", "Writer", "1", "", "2500" if step in ("S5", "S6", "S7") else "2000")
    b03 = L("B-03", "Grip", "1", "", "3000", groups=["ZIN2"])
    b04 = L("B-04", "Kit", "1", "", "500", groups=["ZIN"])
    b05 = L("B-05", "New gear", "1", "", "700")
    if step in ("S1", "S2"):  # sub-budget: groups filter ZIN only, empty accounts kept
        a, b = [b01], [b04]
    elif step in ("S5", "S6", "S7"):
        a, b = [b01, b02], [b03, b05]
    else:
        a, b = [b01, b02], [b03, b04]
    return [("C", "1100", "ZBASE A", [("1101", "Z base one", a)]), ("PT", "ZTOTAL ATL"),
            ("C", "2100", "ZBASE B", [("2101", "Z base two", b)]), ("PT", "ZTOTAL BTL")]


for key, step in [("ZTEST12", "main"), ("ZTEST12-S1", "S1"), ("ZTEST12-S2", "S2"), ("ZTEST12-S5", "S5"),
                  ("ZTEST12-S6", "S6"), ("ZTEST12-S7", "S7")]:
    t = ts12(step)
    fr = {} if step == "S2" else {"ZP10N": pct("10")}
    register(key, budget("ZTEST12 ORIGINAL AND VARIANCE", t, fringes=fr, groups={"ZIN": {}, "ZIN2": {}}), t)

# ---------------------------------------------------------------- ZTEST13 EDGE CASES (4.13)
t = [
    ("C", "1100", "ZEDGE A", [
        ("1101", "Z unicode Ünïcödé", [
            L("E-01", "Café crème Ünïcödé 日本語 ✓", "1", "", "10"),
            L("E-02", "Unicode tags", "1", "", "100", fringes=["ZFRÏNGE"], groups=["ZGRÜPPE"]),
            L("E-03", "  ZSPACES  ", "1", "", "1"),
            L("E-09", "Typed ones", "2", "", "50", x="1", x4="1"),
        ]),
    ]),
    ("PT", "ZTOTAL ATL"),
    ("C", "1200", "ZEDGE B", [
        ("1201", "Z many lines", [L("E-05" if i == 1 else f"E-05.{i:03d}", f"Many {i:03d}", "1", "", "1.01")
                                  for i in range(1, 201)]),
    ]),
    ("PT", "ZTOTAL BTL"),
]
register("ZTEST13", budget("ZTEST13 EDGE CASES", t, fringes={"ZFRÏNGE": pct("10")}, groups={"ZGRÜPPE": {}}), t)


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: gen_ours.py <out-dir>")
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    cases = {}
    for key, (b, ts) in SPEC.items():
        with open(os.path.join(out, key + ".mthd"), "w", encoding="utf-8") as fh:
            json.dump(b, fh, indent=2, ensure_ascii=False)
            fh.write("\n")
        m = {}
        for row in ts:
            if row[0] == "PT":
                continue
            for acode, _, ds in row[3]:
                for pos, d in enumerate(ds, 1):
                    m[d["case"]] = {"account": acode, "position": pos, "id": d["id"], "kind": d["kind"],
                                    "description": d["description"], "fringes": d["fringes"]}
        cases[key] = m
    with open(os.path.join(out, "cases.json"), "w", encoding="utf-8") as fh:
        json.dump(cases, fh, indent=1, ensure_ascii=False)
        fh.write("\n")
    print(f"wrote {len(SPEC)} budgets and cases.json to {out}")


if __name__ == "__main__":
    main()
