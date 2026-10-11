#!/usr/bin/env python3
"""Parity test, the diff and the report (docs/research/parity-test-plan.md, 5.3 and 5.4).

Inputs, all under the run folder (imports/mmb-parity/<date>/):
  ours/plan-cases.json        from plan_cases.py (case list, priorities, plan figures)
  ours/figures/<KEY>.json     from ours_figures.py (our engine's figures)
  ours/mmb/<KEY>.json         MMB's figures in the same shape, written by parse_mmb.py
                              once schema-notes.md exists (section 5.2); may be absent
  captures.jsonl              screen, clipboard and PDF reads (section 2.7); may be empty
It writes <run>/parity-report.md.

A case field with no MMB figure gets the status given by --missing (BLOCKED when the run
never reached MMB, NOT RUN when the run stopped before the case). Statuses otherwise:
PASS, FAIL (with the plan's meaning when MMB's figure is one the plan lists), DISPLAY
(MMB shows whole dollars that equal ours rounded), NEEDS-CENTS (the same, on a case that
is about cents), UNKNOWN-RECORDED (our engine can't express the case; MMB's figure kept).

With --method <binary>, every FAIL also gets our `method explain` tree (depth 3) for
that figure, so the step where the two programs part ways shows without rerunning.

Usage: python3 -I scripts/parity/compare.py <run-dir> [--missing BLOCKED|NOT RUN]
       [--engine <commit>] [--note <text>] [--method <binary>]
"""
import json
import os
import re
import subprocess
import sys
from decimal import Decimal

MONEY = r"-?\d{1,3}(?:,\d{3})*\.\d\d|-?\d+\.\d\d"

# Cases about cents (section 5.3, NEEDS-CENTS).
CENTS_CASES = {f"LM-{i}" for i in range(13, 24)} | {"P-03", "P-06", "P-35", "P-36", "P-37", "F-19", "CU-04",
                                                    "A-11", "CH-ZHALF", "CR-ZCR4", "CR-ZCR6"}


def fmt(v):
    if v is None:
        return ""
    if isinstance(v, bool):
        return "yes" if v else "no"
    if isinstance(v, str):
        return v
    neg = v < 0
    v = abs(v)
    s = f"{v // 100:,}.{v % 100:02d}"
    return "-" + s if neg else s


def get(fig, field):
    """A field of a figures file (ours or MMB's normalized one), or None."""
    if fig is None:
        return None
    try:
        if field.startswith("case:"):
            _, case, f = field.split(":", 2)
            c = fig["cases"].get(case)
            if c is None:
                return None
            if f == "line.fringes" and f not in c:
                fr = [v for k, v in c.items() if k.startswith("fringe.")]
                return sum(fr) if fr else (0 if "line.total" in c else None)
            if f.startswith("fringe.") and f not in c:
                return None
            return c.get(f)
        it = fig.get("in_totals") or {}
        parts = field.split(".")
        head = parts[0]
        if head == "account":
            a = fig["accounts"].get(parts[1])
            if a is None:
                return None
            return a["fringes"] if len(parts) > 2 else a["total"]
        if head == "fringeacct":
            return fig["fringe_accounts"].get(parts[1])
        if head == "category":
            return (it.get("categories") or fig["categories"]).get(parts[1])
        if head == "level":
            return (it.get("levels") or fig["levels"]).get(field[6:])
        if head in ("atl", "btl", "atlbtl", "grand", "net"):
            return it.get(head, fig.get(head))
        if head == "budget_fringes":
            return fig.get("budget_fringes")
        if head == "credits":
            return sum(c["amount"] for c in fig["credits"].values())
        if head == "charge":
            return fig["charges"].get(field[7:])
        if head == "credit":
            cr = fig["credits"].get(parts[1])
            if cr is None:
                return None
            if len(parts) == 2:
                return cr["amount"]
            k = int(parts[2][3:]) - 1
            return cr["rows"][k].get(parts[3]) if k < len(cr["rows"]) else None
        if head == "fringe":
            fr = fig["fringes"].get(parts[1])
            return None if fr is None else fr.get(parts[2])
        if head == "group":
            g = fig["groups"].get(parts[1])
            return None if g is None else g.get(parts[2])
        if head == "set":
            return fig["sets"].get(parts[1])
        if head == "location":
            return fig["locations"].get(parts[1])
    except (KeyError, IndexError, ValueError, TypeError):
        return None
    return None


def meaning(if_differs, mmb):
    """The plan's meaning for MMB's figure, when the text lists that figure."""
    if mmb is None or not if_differs:
        return ""
    for m in re.finditer(r"(" + MONEY + r")(?: \([^)]*\))?(?: or (" + MONEY + r"))?: ([^;]+)", if_differs):
        for g in (m.group(1), m.group(2)):
            if g and int(Decimal(g.replace(",", "")) * 100) == mmb:
                return m.group(3).strip()
    return "unexplained"


def explain_target(ids, case, field):
    """The `method explain` total for a figure."""
    if field == "line.total" or field == "subtotal.shown":
        return f"#{ids[case]['id']}" if case in ids else None
    if field.startswith("fringe.") and field.count(".") == 1 or field == "line.fringes":
        return f"{ids[case]['account']}/fringes" if case in ids else None
    head, _, rest = field.partition(".")
    if head in ("account", "category"):
        return rest.split(".")[0]
    if head in ("charge", "credit"):
        return f"{head}:{rest.split('.')[0]}"
    if head == "fringe":
        return f"fringe:{rest.split('.')[0]}"
    if head == "level":
        return f"level:{rest}"
    if head in ("atl", "btl", "grand", "net"):
        return head
    if head == "budget_fringes":
        return "fringes"
    return None


def load(path):
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def main():
    args = sys.argv[1:]
    if not args:
        sys.exit(__doc__)
    run = args[0]
    opt = {"--missing": "NOT RUN", "--engine": "", "--note": "", "--method": ""}
    i = 1
    while i < len(args):
        opt[args[i]] = args[i + 1]
        i += 2
    ours = os.path.join(run, "ours")
    cases = load(os.path.join(ours, "plan-cases.json"))
    captures = []
    cap_path = os.path.join(run, "captures.jsonl")
    if os.path.exists(cap_path):
        with open(cap_path, encoding="utf-8") as fh:
            captures = [json.loads(x) for x in fh if x.strip()]

    figs, mmbs = {}, {}
    results = []        # (budget, case, pri, field, mmb, ours, plan, status, meaning)
    gen_bad = []        # generator check: ours differs from the plan's figure
    for r in cases:
        key = r["key"]
        if key and key not in figs:
            figs[key] = load(os.path.join(ours, "figures", key + ".json"))
            mmbs[key] = load(os.path.join(ours, "mmb", key + ".json"))
        fig = figs.get(key) if key else None
        mmb = mmbs.get(key) if key else None
        if not r["expected"]:
            # Unknown or behavior-only case: nothing of ours to compare.
            rec = [c for c in captures if c.get("case") == r["id"]]
            if rec:
                for c in rec:
                    results.append((r["budget"], r["id"], r["pri"], c.get("field", ""), c.get("shown", c.get("mmb")),
                                    None, None, "UNKNOWN-RECORDED", r["if_differs"][:120]))
            else:
                results.append((r["budget"], r["id"], r["pri"], "(record MMB's figures)", None, None, None,
                                 opt["--missing"], "unknown: record MMB's figure" if not key else ""))
            continue
        for field, plan in r["expected"].items():
            o = get(fig, field)
            if o != plan:
                gen_bad.append((key, r["id"], field, plan, o))
            m = get(mmb, field)
            shown_whole = False
            if m is None:
                cap = [c for c in captures if c.get("budget") == key.split("-")[0] and c.get("case") == r["id"]
                       and (c.get("field") == field.split(":")[-1] or c.get("field") == field)]
                if cap:
                    txt = str(cap[-1].get("mmb", "")).replace(",", "").replace("(", "-").replace(")", "")
                    try:
                        m = int((Decimal(txt) * 100).to_integral_value())
                        shown_whole = "." not in txt
                    except Exception:
                        m = None
            if m is None:
                status, mean = opt["--missing"], ""
            elif m == o:
                status, mean = "PASS", ""
            elif shown_whole and o is not None and round(o / 100) * 100 == m:
                status = "NEEDS-CENTS" if r["id"] in CENTS_CASES else "DISPLAY"
                mean = ""
            else:
                status, mean = "FAIL", meaning(r["if_differs"], m)
            results.append((r["budget"], r["id"], r["pri"], field.split(":")[-1] if field.startswith("case:") else field,
                            m, o, plan, status, mean))

    # Counts per priority, by case (a case fails if any of its fields fails).
    order = ["FAIL", "NEEDS-CENTS", "DISPLAY", "UNKNOWN-RECORDED", "SKIPPED", "BLOCKED", "NOT RUN", "PASS"]
    per_case = {}
    for b, cid, pri, *_rest in results:
        st = _rest[4]
        k = (b, cid, pri)
        prev = per_case.get(k)
        if prev is None or order.index(st) < order.index(prev):
            per_case[k] = st
    lines = [f"# Parity report {os.path.basename(os.path.normpath(run))}",
             f"MMB 10.35.7, engine {opt['--engine'] or 'unknown'}, plan docs/research/parity-test-plan.md", ""]
    if opt["--note"]:
        lines += [opt["--note"], ""]
    for pri in ("P0", "P1", "P2"):
        cs = [st for (b, cid, p), st in per_case.items() if p == pri]
        cnt = {}
        for st in cs:
            cnt[st] = cnt.get(st, 0) + 1
        lines.append(f"{pri}: {len(cs)} cases, " + ", ".join(f"{n} {st}" for st, n in sorted(cnt.items(), key=lambda x: order.index(x[0]))))
    total_fields = sum(1 for x in results if x[6] is not None)
    lines += ["", f"Fields compared per case: {total_fields} figures in all "
              f"(a case can carry several: its line total and each fringe).", ""]

    lines += ["## Generator check (our engine now against the plan's section 4 figures)", ""]
    if gen_bad:
        lines.append(f"{len(gen_bad)} of {total_fields} plan figures differ from what our engine gives today. "
                     "Each is a generator bug, a plan typo or an engine change since the plan was written; "
                     "settle them before reading a FAIL below.")
        lines += ["", "| Budget | Case | Field | Plan | Ours now |", "|---|---|---|---|---|"]
        for key, cid, field, plan, o in gen_bad:
            lines.append(f"| {key} | {cid} | {field} | {fmt(plan)} | {fmt(o)} |")
    else:
        lines.append(f"All {total_fields} plan figures are reproduced exactly by the generated budgets "
                     "(ours/*.mthd) with the engine above.")
    lines.append("")

    fails = [x for x in results if x[7] == "FAIL"]
    fails.sort(key=lambda x: (x[2], x[0], x[1]))
    lines += ["## Failures (P0 first)", ""]
    if fails:
        lines += ["| Case | Budget | Field | MMB | Ours | Delta | Meaning |", "|---|---|---|---|---|---|---|"]
        for b, cid, pri, field, m, o, plan, st, mean in fails:
            d = fmt(m - o) if (m is not None and o is not None) else ""
            lines.append(f"| {cid} | {b} | {field} | {fmt(m)} | {fmt(o)} | {d} | {mean} |")
    else:
        lines.append("None (no MMB figure was compared).")
    lines.append("")
    if fails and opt["--method"]:
        ids = load(os.path.join(ours, "cases.json")) or {}
        lines += ["### Our working for each failure", ""]
        for b, cid, pri, field, m, o, plan, st, mean in fails:
            key = next((r["key"] for r in cases if r["id"] == cid and r["budget"] == b), b)
            target = explain_target(ids.get(key, {}), cid, field)
            if not target:
                continue
            res = subprocess.run([opt["--method"], "explain", os.path.join(ours, key + ".mthd"), target,
                                  "--depth", "3"], capture_output=True, text=True)
            lines += [f"{cid} {field} (`method explain {key}.mthd {target}`):", "", "```text",
                      (res.stdout or res.stderr).rstrip(), "```", ""]

    unk = [x for x in results if x[7] == "UNKNOWN-RECORDED"]
    lines += ["## Unknown: MMB's figures for features we lack", ""]
    if unk:
        lines += ["| Case | Field | MMB | Notes |", "|---|---|---|---|"]
        for b, cid, pri, field, m, o, plan, st, mean in unk:
            lines.append(f"| {cid} | {field} | {fmt(m)} | {mean} |")
    else:
        lines.append("None recorded.")
    lines.append("")

    lines += ["## Every case", ""]
    budgets = []
    for x in results:
        if x[0] not in budgets:
            budgets.append(x[0])
    for b in budgets:
        lines += [f"### {b}", "", "| Case | Pri | Field | MMB | Ours | Delta | Status |", "|---|---|---|---|---|---|---|"]
        for bb, cid, pri, field, m, o, plan, st, mean in results:
            if bb != b:
                continue
            d = fmt(m - o) if (m is not None and o is not None and isinstance(m, int)) else ""
            lines.append(f"| {cid} | {pri} | {field} | {fmt(m)} | {fmt(o)} | {d} | {st} |")
        lines.append("")
    out = os.path.join(run, "parity-report.md")
    with open(out, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines))
    print(f"wrote {out}: {len(per_case)} cases, {total_fields} figures, {len(gen_bad)} generator differences, "
          f"{len(fails)} FAIL fields")


if __name__ == "__main__":
    main()
