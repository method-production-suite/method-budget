#!/usr/bin/env python3
"""Parity test: the case list and the plan's expected figures, read from
docs/research/parity-test-plan.md section 4 (our engine's figures when the plan was
written), so the comparison can (1) check the generator reproduces them and (2) print
the plan's "If MMB differs" meaning next to a failing MMB figure.

Writes <out>/plan-cases.json:
  [{"budget": "ZTEST04", "key": "ZTEST04" | "ZTEST04-V04-2" | null, "id": "P-12",
    "pri": "P0", "texts": {column: text}, "if_differs": text,
    "expected": {field: cents}}]
Fields use the names of ours_figures.py: "line.total", "fringe.ZP125", "subtotal.shown",
"line.fringes", "account.1101", "account.1101.fringes", "fringeacct.1199",
"category.1100", "level.<NAME>", "atl", "btl", "budget_fringes", "charge.<NAME>",
"credit.<NAME>", "credit.<NAME>.row<k>.<gross|excl|net|credit|var>", "grand", "net",
"fringe.<NAME>.total", "fringe.<NAME>.lines", "group.<NAME>.total", "group.<NAME>.active",
"set.<NAME>", "location.<NAME>". Line fields belong to the case's own row; the rest are
budget-wide.

Usage: python3 -I scripts/parity/plan_cases.py <plan.md> <out-dir>
"""
import json
import os
import re
import sys
from decimal import ROUND_HALF_UP, Decimal

MONEY = r"-?\d{1,3}(?:,\d{3})*\.\d\d|-?\d+\.\d\d"


def c(text):
    return int((Decimal(text.replace(",", "")) * 100).to_integral_value(rounding=ROUND_HALF_UP))


def cells(line):
    return [x.strip() for x in line.strip().strip("|").split("|")]


FRINGE_PAIR = re.compile(r"\b(Z[A-Z0-9À-ÿ]*(?:N|G|0|\d|[A-Z]))\s+(" + MONEY + r")\b")


def line_fields(main_text, fringe_text):
    e = {}
    t = main_text.strip()
    m = re.search(MONEY, t)
    if t.startswith("shows") and m:
        e["subtotal.shown"] = c(m.group(0))
    elif m:
        e["line.total"] = c(m.group(0))
    for name, amt in FRINGE_PAIR.findall(t + " " + (fringe_text or "")):
        if name.startswith("ZTOTAL"):
            continue
        e[f"fringe.{name}"] = c(amt)
    ft = (fringe_text or "") + " " + t
    if re.search(r"\bno fringe\b|^none$", (fringe_text or "").strip()) or "no fringe" in t:
        e["line.fringes"] = 0
    if "(suppressed" in t and "line.total" not in e:
        e["line.total"] = 0
    return e


def totals_fields(text):
    """Budget-wide figures from a **Totals** paragraph."""
    e = {}
    text = text.replace("\n", " ")
    # A fallback sentence ("If LM-27 to LM-29 make MMB misbehave ... use account 1201 = ...")
    # describes another capture, not this one.
    text = re.split(r"\. If ", text)[0]
    for m in re.finditer(r"(fringe account |[Cc]ategory |[Aa]ccounts? )?(\d{4}) = (" + MONEY + r")"
                         r"(?: \((?:fringes )?(" + MONEY + r")\))?", text):
        prefix, code, amt, fr = (m.group(1) or "").strip().lower(), m.group(2), m.group(3), m.group(4)
        if prefix == "fringe account" or (prefix == "" and code.endswith(("99", "98"))):
            e[f"fringeacct.{code}"] = c(amt)
        elif prefix == "category" or (prefix == "" and code.endswith("00")):
            e[f"category.{code}"] = c(amt)
        else:
            e[f"account.{code}"] = c(amt)
            if fr:
                e[f"account.{code}.fringes"] = c(fr)
    m = re.search(r"[Gg]rand = net = (" + MONEY + ")", text) or re.search(r"[Gg]rand = net (" + MONEY + ")", text)
    if m:
        e["grand"] = e["net"] = c(m.group(1))
    for k in ("ATL", "BTL"):
        m = re.search(r"\b" + k + r" (" + MONEY + ")", text)
        if m:
            e[k.lower()] = c(m.group(1))
    for name, amt, n in re.findall(r"\b(Z[A-Z0-9]+) (" + MONEY + r") on (\d+)", text):
        e[f"fringe.{name}.total"] = c(amt)
        e[f"fringe.{name}.lines"] = int(n)
    m = re.search(r"Fringe totals: ([^.]*(?:\.\d\d[^.]*)*)\.", text)
    if m and " on " not in m.group(1):
        for name, amt in re.findall(r"\b(Z[A-Z0-9]+) (" + MONEY + ")", m.group(1)):
            e[f"fringe.{name}.total"] = c(amt)
    return e


# Hand-written expectations the tables state in prose (variations, posting table,
# special cases). Values in plain dollars; converted to cents below.
MANUAL = {
    ("ZTEST01", "ZTEST01-V01-2", "V01-2"): {"case:LM-02:line.total": "200.00"},
    ("ZTEST02", "ZTEST02-V02-1", "V02-1"): {"case:G-01:line.total": "220.00", "case:G-02:line.total": "110000000.00",
                                            "case:G-04:line.total": "177.76", "case:G-05:line.total": "1249.36",
                                            "case:G-06:line.total": "18.75", "case:G-07:line.total": "-40.00",
                                            "grand": "110003432.87"},
    ("ZTEST02", "ZTEST02-V02-2", "V02-2"): {"case:G-11:line.total": "100.00"},
    ("ZTEST02", "ZTEST02-V02-5", "V02-5"): {"case:G-05:line.total": "937.02"},
    ("ZTEST05", "ZTEST05-V05-1", "V05-1"): {"case:F-03:fringe.ZFHR": "250.00", "case:F-04:fringe.ZFHR": "250.00",
                                            "case:F-14:fringe.ZFHR": "18.75", "case:F-16:fringe.ZFHR": "5.00",
                                            "fringe.ZFHR.total": "523.75", "grand": "115249.97"},
    ("ZTEST05", "ZTEST05-V05-2", "V05-2"): {"budget_fringes": "22721.39", "grand": "115354.72"},
    ("ZTEST07", "ZTEST07-V07-1", "V07-1"): {"case:GR-02:line.total": "2000.00", "case:GR-02:fringe.ZP10N": "200.00",
                                            "case:GR-03:line.total": "3000.00", "case:GR-06:subtotal.shown": "10500.00",
                                            "case:GR-07:line.total": "1050.00", "account.1101": "11550.00",
                                            "fringeacct.1199": "700.00", "category.1100": "12250.00",
                                            "grand": "12850.00", "set.ZSET2": "2200.00"},
    ("ZTEST07", "ZTEST07-V07-1b", "V07-1b"): {"grand": "7150.00"},
    ("ZTEST07", "ZTEST07-V07-2", "V07-2"): {"case:GR-06:subtotal.shown": "500.00", "case:GR-07:line.total": "50.00",
                                            "account.1101": "550.00", "category.1100": "550.00", "grand": "1150.00",
                                            "group.ZIN2.active": "600.00"},
    ("ZTEST08", "ZTEST08-V08-1", "V08-1"): {"category.1100": "49088.10", "category.1200": "166.67",
                                            "atl": "49088.10", "btl": "166.67", "grand": "49254.77", "net": "49254.77"},
    ("ZTEST08", "ZTEST08-V08-2", "V08-2"): {"category.1100": "119283.88", "category.1200": "405.00",
                                            "grand": "119688.88", "net": "119688.88"},
    ("ZTEST09", "ZTEST09-V09-1", "V09-1"): {"atl": "10500.00", "btl": "10300.00", "budget_fringes": "1800.00",
                                            "charge.ZFLAT": "1478.00", "charge.ZPCT5": "1040.00",
                                            "charge.ZPCTX": "1988.90", "charge.ZPCTCAT": "250.00",
                                            "charge.ZPCTACC": "609.00", "charge.ZOVER": "1800.00",
                                            "charge.ZFLATEUR": "3000.00", "charge.ZFLATCAT": "10000.00",
                                            "charge.ZHALF": "92.04", "grand": "42857.94", "net": "42857.94"},
    ("ZTEST09", "ZTEST09-V09-2", "V09-2"): {"charge.ZFLAT": "1478.00", "charge.ZPCT5": "1130.00",
                                            "charge.ZPCTX": "2168.90", "charge.ZHALF": "100.01", "grand": "43389.91"},
    ("ZTEST09", "ZTEST09-V09-3", "V09-3"): {"atl": "11000.00", "btl": "10800.00", "charge.ZFLAT": "2278.00",
                                            "charge.ZPCT5": "1090.00", "charge.ZPCTX": "2168.90",
                                            "charge.ZPCTCAT": "260.00", "charge.ZPCTACC": "654.00",
                                            "charge.ZOVER": "1980.00", "charge.ZHALF": "96.47",
                                            "grand": "43327.37", "net": "43327.37"},
    ("ZTEST09", "ZTEST09-V09-4", "V09-4"): {"charge.ZPCTX": "2168.90", "grand": "43389.91"},
    ("ZTEST10", "ZTEST10-V10-1", "V10-1"): {"credit.ZCR1": "1205.40", "credit.ZCR2": "825.00", "atl": "4000.00",
                                            "btl": "4000.00", "budget_fringes": "800.00", "grand": "8800.00",
                                            "net": "5450.47"},
    ("ZTEST10", "ZTEST10-V10-2", "V10-2"): {"credit.ZCR1": "343.00", "category.1100": "0.00", "grand": "4300.00",
                                            "credits": "2487.13", "net": "1812.87"},
    # ZTEST06 posting table (section 4.6): main and V06-1 to V06-3, V06-4 like main.
    ("ZTEST06", "ZTEST06", "PM-CAT"): {"account.1101": "10000.00", "account.1102": "5000.00",
                                       "fringeacct.1199": "1500.00", "fringeacct.2199": "800.00",
                                       "fringeacct.3199": "700.00", "fringeacct.4199": "100.00",
                                       "category.1100": "16500.00", "level.ZTOTAL ATL": "16500.00",
                                       "category.2100": "10800.00", "level.ZTOTAL PROD": "10800.00",
                                       "category.3100": "4100.00", "level.ZTOTAL POST": "4100.00",
                                       "category.4100": "1100.00", "atl": "16500.00", "btl": "16000.00",
                                       "charge.ZPCT5": "1625.00", "grand": "34125.00", "net": "34125.00"},
    ("ZTEST06", "ZTEST06-V06-1", "PM-ACC"): {"account.1101": "11000.00", "account.1102": "5500.00",
                                             "category.1100": "16500.00", "level.ZTOTAL ATL": "16500.00",
                                             "category.2100": "10800.00", "level.ZTOTAL PROD": "10800.00",
                                             "category.3100": "4100.00", "level.ZTOTAL POST": "4100.00",
                                             "category.4100": "1100.00", "atl": "16500.00", "btl": "16000.00",
                                             "charge.ZPCT5": "1625.00", "grand": "34125.00", "net": "34125.00"},
    ("ZTEST06", "ZTEST06-V06-2", "PM-LVL"): {"account.1101": "10000.00", "account.1102": "5000.00",
                                             "category.1100": "15000.00", "level.ZTOTAL ATL": "16500.00",
                                             "category.2100": "10000.00", "level.ZTOTAL PROD": "10800.00",
                                             "category.3100": "3400.00", "level.ZTOTAL POST": "4100.00",
                                             "category.4100": "1000.00", "atl": "16500.00", "btl": "16000.00",
                                             "charge.ZPCT5": "1625.00", "grand": "34125.00", "net": "34125.00"},
    ("ZTEST06", "ZTEST06-V06-3", "PM-BUD"): {"account.1101": "10000.00", "account.1102": "5000.00",
                                             "category.1100": "15000.00", "level.ZTOTAL ATL": "15000.00",
                                             "category.2100": "10000.00", "level.ZTOTAL PROD": "10000.00",
                                             "category.3100": "3400.00", "level.ZTOTAL POST": "3400.00",
                                             "category.4100": "1000.00", "atl": "15000.00", "btl": "14400.00",
                                             "budget_fringes": "3100.00", "charge.ZPCT5": "1470.00",
                                             "grand": "33970.00", "net": "33970.00"},
    ("ZTEST06", "ZTEST06-V06-4", "PM-SUF"): {"fringeacct.1198": "1500.00", "fringeacct.2198": "800.00",
                                             "fringeacct.3198": "700.00", "fringeacct.4198": "100.00",
                                             "grand": "34125.00"},
    ("ZTEST11", "ZTEST11", "T-01"): {"atl": "0.00", "btl": "10100.00", "grand": "10100.00"},
    ("ZTEST11", "ZTEST11-V11-1", "T-02"): {"atl": "1100.00", "btl": "9000.00", "grand": "10100.00"},
    ("ZTEST11", "ZTEST11-V11-2", "T-03"): {"atl": "1100.00", "level.ZTOTAL PROD": "2000.00",
                                           "level.ZTOTAL POST": "3000.00", "btl": "9000.00", "grand": "10100.00"},
    ("ZTEST11", "ZTEST11-V11-3", "T-04"): {"level.ZTOP": "0.00", "atl": "0.00", "btl": "10100.00", "grand": "10100.00"},
    ("ZTEST11", "ZTEST11-V11-6", "T-07"): {"fringeacct.1A99": "10.00", "grand": "10210.00"},
    ("ZTEST12", "ZTEST12-S1", "B-S1"): {"case:B-01:line.total": "1000.00", "case:B-01:fringe.ZP10N": "100.00",
                                        "case:B-04:line.total": "500.00", "category.1100": "1100.00",
                                        "category.2100": "500.00", "grand": "1600.00"},
    ("ZTEST12", "ZTEST12-S2", "B-S2"): {"grand": "1500.00"},
    ("ZTEST12", "ZTEST12", "B-S4"): {"grand": "6600.00", "category.1100": "3100.00", "category.2100": "3500.00",
                                     "fringeacct.1199": "100.00"},
    ("ZTEST12", "ZTEST12-S5", "B-S5"): {"account.1101": "3500.00", "fringeacct.1199": "100.00", "account.2101": "3700.00",
                                        "category.1100": "3600.00", "category.2100": "3700.00", "grand": "7300.00"},
    ("ZTEST12", "ZTEST12-S6", "B-S6"): {"grand": "7300.00"},
    ("ZTEST12", "ZTEST12-S7", "B-S7"): {"grand": "7850.00", "account.1101": "4000.00", "fringeacct.1199": "150.00",
                                        "category.1100": "4150.00"},
    ("ZTEST13", "ZTEST13", "E-05"): {"account.1201": "202.00"},
    ("ZTEST13", "ZTEST13", "E-TOT"): {"category.1100": "221.00", "fringeacct.1199": "10.00",
                                      "category.1200": "202.00", "grand": "423.00", "net": "423.00"},
}

# Cases our format can't express or that only record MMB's behavior.
UNKNOWN = {"V01-1", "V02-3", "V02-4", "V04-1", "V04-2", "V04-3", "V07-3", "V07-4", "V07-5", "V08-3", "V10-3",
           "V10-4", "T-06", "B-S3", "PM-SEP", "PM-POS", "T-05", "T-08", "E-NOTES", "E-INFO", "E-03"}

MAIN_BUDGET_CASES_SPECIAL = {"CH-GRAND", "CR-NET"}


def main():
    if len(sys.argv) != 3:
        sys.exit("usage: plan_cases.py <plan.md> <out-dir>")
    plan, out = sys.argv[1], sys.argv[2]
    lines = open(plan, encoding="utf-8").read().splitlines()
    budget = None
    rows = []
    totals = {}
    i = 0
    while i < len(lines):
        ln = lines[i]
        m = re.match(r"### 4\.(\d+) (ZTEST\d\d)?", ln)
        if m:
            n = int(m.group(1))
            budget = m.group(2) or {14: "X", 15: "RP", 16: "RT"}.get(n)
        elif ln.startswith("## 5"):
            break
        if budget and ln.startswith("**Totals") or (budget == "ZTEST09" and ln.startswith("**Expected (main)**: ATL")):
            para = [ln]
            j = i + 1
            while j < len(lines) and lines[j].strip() and not lines[j].startswith(("|", "**", "#")):
                para.append(lines[j])
                j += 1
            totals.setdefault(budget, {}).update(totals_fields(" ".join(para)))
        if budget == "ZTEST10" and ln.startswith("Line figures:"):
            para = " ".join(lines[i:i + 3])
            totals.setdefault(budget, {}).update(totals_fields(para))
        if ln.startswith("| Case") or ln.startswith("| Var"):
            header = cells(ln)
            if "Pri" not in header:
                i += 1
                continue
            j = i + 2
            while j < len(lines) and lines[j].startswith("|"):
                cs = cells(lines[j])
                rec = dict(zip(header, cs))
                rid = rec.get("Case") or rec.get("Var") or ""
                if rid and rid != "main":
                    rows.append({"budget": budget, "id": rid, "pri": rec.get("Pri", ""),
                                 "texts": {k: v for k, v in rec.items() if k not in ("Case", "Var", "Pri")},
                                 "if_differs": rec.get("If MMB differs", rec.get("If it fails", ""))})
                j += 1
            i = j
            continue
        i += 1

    manual_by_id = {(b, rid): (key, exp) for (b, key, rid), exp in MANUAL.items()}
    out_rows = []
    seen = set()
    for r in rows:
        b, rid = r["budget"], r["id"]
        key = b if b and b.startswith("ZTEST") else None
        exp = {}
        texts = r["texts"]
        main = texts.get("Expected line total") or texts.get("Line total") or texts.get("Expected line total (USD)") \
            or texts.get("Expected") or ""
        fringe = texts.get("Expected fringes") or texts.get("Expected fringe")
        if (b, rid) in manual_by_id:
            key, mexp = manual_by_id[(b, rid)]
            for f, v in mexp.items():
                exp[f] = c(v)
        elif rid in UNKNOWN or not key:
            key = None if rid in UNKNOWN else key
        elif rid.startswith("V"):
            key = f"{b}-{rid}"
        elif rid.startswith("CH-") and rid not in MAIN_BUDGET_CASES_SPECIAL:
            m = re.search(MONEY, main)
            if m:
                exp[f"charge.{rid[3:]}"] = c(m.group(0))
        elif rid == "CH-GRAND":
            m = re.search(MONEY, main)
            exp["grand"] = exp["net"] = c(m.group(0))
        elif rid == "CR-NET":
            m = re.findall(MONEY, main)
            exp["credits"], exp["net"] = c(m[0]), c(m[1])
        elif re.match(r"CR-ZCR\d+-row\d", rid):
            cr, rown = re.match(r"CR-(ZCR\d+)-row(\d)", rid).groups()
            for label, field in (("gross", "gross"), ("excluded fringes", "excl"), ("net", "net"),
                                 ("credit", "credit"), ("cap variance", "var")):
                m = re.search(r"\b" + label + r" (" + MONEY + ")", main)
                if m:
                    exp[f"credit.{cr}.row{rown}.{field}"] = c(m.group(1))
        elif re.match(r"CR-ZCR\d+$", rid):
            m = re.search(MONEY, main)
            exp[f"credit.{rid[3:]}"] = c(m.group(0))
        elif rid.startswith("GR-TOT-"):
            g = rid[7:]
            for label, field in (("total", "total"), ("active", "active")):
                m = re.search(r"\b" + label + r" (" + MONEY + ")", main)
                if m:
                    exp[f"group.{g}.{field}"] = c(m.group(1))
        elif rid in ("GR-SET", "GR-LOC"):
            kind = "set" if rid == "GR-SET" else "location"
            for name, amt in re.findall(r"\b(Z(?:SET|LOC)\d) (" + MONEY + ")", main):
                exp[f"{kind}.{name}"] = c(amt)
        elif rid.startswith("E-") and rid != "E-05":
            if rid in ("E-01", "E-02", "E-09"):
                exp = {f"case:{rid}:{k}": v for k, v in line_fields(main, fringe).items()}
        else:
            if texts.get("Check") is None and texts.get("Layout or change") is None and texts.get("Step") is None:
                exp = {f"case:{rid}:{k}": v for k, v in line_fields(main, fringe).items()}
        seen.add((b, rid))
        out_rows.append({"budget": b, "key": key, "id": rid, "pri": r["pri"], "texts": texts,
                         "if_differs": r["if_differs"], "expected": exp})
    # Manual entries that are not table rows (paragraph variations).
    for (b, key, rid), mexp in MANUAL.items():
        if (b, rid) not in seen:
            pri = {"V01-2": "P2", "V05-1": "P1", "V05-2": "P2", "V07-1b": "P1"}.get(rid, "P1")
            out_rows.append({"budget": b, "key": key, "id": rid, "pri": pri, "texts": {}, "if_differs": "",
                             "expected": {f: c(v) for f, v in mexp.items()}})
    # One "TOTALS" case per budget for the main budget-wide figures.
    pri_of = {"ZTEST02": "P1", "ZTEST11": "P1", "ZTEST12": "P1", "ZTEST13": "P2"}
    for b, exp in sorted(totals.items()):
        out_rows.append({"budget": b, "key": b, "id": f"{b[4:]}-TOTALS", "pri": pri_of.get(b, "P0"),
                         "texts": {}, "if_differs": "account, category, level and summary totals", "expected": exp})
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "plan-cases.json"), "w", encoding="utf-8") as fh:
        json.dump(out_rows, fh, indent=1, ensure_ascii=False)
        fh.write("\n")
    by_pri = {}
    for r in out_rows:
        by_pri[r["pri"]] = by_pri.get(r["pri"], 0) + 1
    print(f"{len(out_rows)} case rows; by priority {dict(sorted(by_pri.items()))}")


if __name__ == "__main__":
    main()
