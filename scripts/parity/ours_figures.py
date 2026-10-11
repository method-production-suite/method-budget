#!/usr/bin/env python3
"""Parity test, our side: our engine's figures for every generated budget
(docs/research/parity-test-plan.md, section 5.1 step 4).

For each `<ours>/<KEY>.mthd` it runs the command line (`method totals --json`,
`method export --csv`, `method explain <account>/fringes --depth 3`,
`method explain credit:<NAME> --depth 4`) and writes `<ours>/figures/<KEY>.json` in the
same normalized shape the MMB parser must produce (section 5.2), money as integer cents:

  {cases: {<case>: {"line.total", "line.fringes", "fringe.<NAME>", "subtotal.shown",
                    "line.included"}},
   accounts: {<code>: {"total", "fringes"}}, fringe_accounts: {<code>: total},
   categories: {<code>: total}, levels: {<name>: total}, atl, btl, atlbtl,
   budget_fringes, charges: {<name>: amount}, credits: {<name>: {"amount",
   "rows": [{"gross", "excl", "net", "credit", "var"}]}}, grand, net, net_in_totals,
   fringes: {<name>: {"total", "lines"}}, groups: {<name>: {"total", "active"}},
   sets: {<name>: total}, locations: {<name>: total}}

Usage: python3 -I scripts/parity/ours_figures.py <ours-dir> <method-binary>
"""
import csv
import json
import os
import re
import subprocess
import sys
from decimal import ROUND_HALF_UP, Decimal


def cents(text):
    text = (text or "").strip().replace(",", "")
    if text == "":
        return None
    neg = text.startswith("(") and text.endswith(")")
    if neg:
        text = text[1:-1]
    v = int((Decimal(text) * 100).to_integral_value(rounding=ROUND_HALF_UP))
    return -v if neg else v


def run(binary, *args):
    r = subprocess.run([binary, *args], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"{' '.join(args)}: {r.stderr.strip()}")
    return r.stdout


def read_csv(path):
    if not os.path.exists(path):
        return []
    with open(path, newline="", encoding="utf-8-sig") as fh:
        return list(csv.DictReader(fh))


LINE_RE = re.compile(r"^  - .*: fringes earned by line (\d+)  = (\S+)$")
FRINGE_RE = re.compile(r"^    - ([^:]+): .*  = (\S+)$")


def line_fringes(binary, path, account):
    """{position: {fringe name: cents}} for one account, from `method explain`."""
    out = {}
    cur = None
    for raw in run(binary, "explain", path, f"{account}/fringes", "--depth", "3").splitlines():
        m = LINE_RE.match(raw)
        if m:
            cur = int(m.group(1))
            out[cur] = {}
            continue
        m = FRINGE_RE.match(raw)
        if m and cur is not None:
            out[cur][m.group(1).strip()] = cents(m.group(2))
    return out


ROW_RE = re.compile(r"^  - (.+): (-?[\d,]+(?:\.\d+)?)$")
NOT_ROW = ("  - Rows added:", "  - Credit cap:", "  - Discount:", "  - Rounding:")


def credit_rows(binary, path, name):
    rows = []
    row = None
    for raw in run(binary, "explain", path, f"credit:{name}", "--depth", "4").splitlines():
        m = ROW_RE.match(raw)
        if m and not raw.startswith(NOT_ROW):
            row = {"description": m.group(1), "credit": cents(m.group(2)), "credit_exact": m.group(2)}
            rows.append(row)
            continue
        if row is None or not raw.startswith("    - "):
            continue
        s = raw[6:]
        br = re.search(r"\[(-?[\d,]+\.\d\d)\]", s)
        if s.startswith("Gross:") and br:
            row["gross"] = cents(br.group(1))
        elif s.startswith("Excluded fringes:") and br:
            row["excl"] = cents(br.group(1))
        elif s.startswith("Net spend:") and br:
            row["net"] = cents(br.group(1))
        elif s.startswith("Cap on spend:"):
            capped = cents(s.rsplit(":", 1)[1])
            row["var"] = capped - row.get("net", 0)
        elif s.startswith("Credit:") and "capped at" in s:
            mm = re.search(r"= (-?[\d,]+\.\d\d), capped at", s)
            if mm:
                row["var"] = row["credit"] - cents(mm.group(1))
    for row in rows:
        row.setdefault("excl", 0)
        row.setdefault("var", 0)
    return rows


def figures(binary, ours, key, cases):
    path = os.path.join(ours, key + ".mthd")
    tot = json.loads(run(binary, "totals", path, "--json"))
    outdir = os.path.join(ours, "out")
    os.makedirs(outdir, exist_ok=True)
    run(binary, "export", path, "--csv", os.path.join(outdir, key + ".csv"))
    tab = lambda t: read_csv(os.path.join(outdir, f"{key} - {t}.csv"))

    f = {"title": tot["title"], "cases": {}, "accounts": {}, "fringe_accounts": {}, "categories": {},
         "levels": {}, "charges": {}, "credits": {}, "fringes": {}, "groups": {}, "sets": {}, "locations": {}}
    for c in tot["categories"]:
        f["categories"][c["code"]] = c["total"]
    for lv in tot["levels"]:
        f["levels"][lv["name"]] = lv["amount"]
    f["atl"], f["btl"], f["atlbtl"] = tot["total_atl"], tot["total_btl"], tot["atl_btl"]
    f["budget_fringes"] = tot["budget_fringes"]
    f["grand"], f["net"] = tot["grand_total"], tot["net_total"]
    f["net_in_totals"] = tot.get("net_total_in_totals_currency")
    f["totals_currency"] = tot.get("totals_currency")
    f["base_currency"] = tot.get("base_currency")
    f["credits_total"] = tot.get("credits_total", 0)
    f["charges_total"] = tot.get("charges_total", 0)
    # Totals shown in another currency: our engine keeps totals in the base currency and
    # converts the grand and net totals; the plan reads every summary figure as the base
    # amount divided by the rate, rounded once, half away from zero (section 4.8 V08-1).
    if f["totals_currency"] and f["totals_currency"] != f["base_currency"]:
        with open(path, encoding="utf-8") as fh:
            rate = Decimal(json.load(fh)["budget"]["currencies"][f["totals_currency"]]["rate"])
        conv = lambda v: int((Decimal(v) / rate).to_integral_value(rounding=ROUND_HALF_UP))
        f["in_totals"] = {"categories": {k: conv(v) for k, v in f["categories"].items()},
                          "levels": {k: conv(v) for k, v in f["levels"].items()},
                          "atl": conv(f["atl"]), "btl": conv(f["btl"]), "atlbtl": conv(f["atlbtl"]),
                          "grand": f["net_in_totals"] if f["credits_total"] == 0 else conv(f["grand"]),
                          "net": f["net_in_totals"]}
    for ch in tot["charges"]:
        f["charges"][ch["name"]] = ch["amount"]
    for cr in tot["credits"]:
        f["credits"][cr["name"]] = {"amount": cr["amount"], "rows": credit_rows(binary, path, cr["name"])}

    for r in tab("Accounts"):
        if r["Account Name"] == "Fringes" and r["Details Total"] == "":
            f["fringe_accounts"][r["Account"]] = cents(r["Total"])
        else:
            f["accounts"][r["Account"]] = {"total": cents(r["Total"]), "fringes": cents(r["Fringe Total"]) or 0}
    f["topsheet"] = [{"type": r["Type"], "code": r["Code"], "description": r["Description"],
                      "total": cents(r["Total"])} for r in tab("Topsheet")]
    for r in tab("Fringes"):
        f["fringes"][r["Name"]] = {"total": cents(r["Total"]), "lines": int(r["Lines"] or 0)}
    for r in tab("Groups"):
        f["groups"][r["Name"]] = {"total": cents(r["Total"]), "active": cents(r["Counted Total"])}
    for r in tab("Sets"):
        f["sets"][r["Set"]] = cents(r["Total"])
    for r in tab("Locations"):
        f["locations"][r["Location"]] = cents(r["Total"])

    by_id = {str(v["id"]): k for k, v in cases.items()}
    fr_cache = {}
    for r in tab("Detail"):
        case = by_id.get(r["Row ID"])
        if case is None:
            continue
        info = cases[case]
        c = {}
        if r["Kind"] == "Subtotal":
            c["subtotal.shown"] = cents(r["Subtotal"])
        else:
            c["line.total"] = cents(r["Total"])
            c["line.included"] = r["Included"] == "Yes"
            if r["Kind"] == "Line":
                c["line.fringes"] = cents(r["Fringe Total"]) or 0
                acct = info["account"]
                if info["fringes"]:
                    if acct not in fr_cache:
                        fr_cache[acct] = line_fringes(binary, path, acct)
                    for name, amt in fr_cache[acct].get(int(r["Line"]), {}).items():
                        c[f"fringe.{name}"] = amt
        f["cases"][case] = c
    return f


def main():
    if len(sys.argv) != 3:
        sys.exit("usage: ours_figures.py <ours-dir> <method-binary>")
    ours, binary = sys.argv[1], os.path.abspath(sys.argv[2])
    with open(os.path.join(ours, "cases.json"), encoding="utf-8") as fh:
        all_cases = json.load(fh)
    os.makedirs(os.path.join(ours, "figures"), exist_ok=True)
    for key in sorted(all_cases):
        f = figures(binary, ours, key, all_cases[key])
        with open(os.path.join(ours, "figures", key + ".json"), "w", encoding="utf-8") as fh:
            json.dump(f, fh, indent=1, ensure_ascii=False)
            fh.write("\n")
    print(f"wrote figures for {len(all_cases)} budgets")


if __name__ == "__main__":
    main()
