#!/usr/bin/env python3
"""Build the bundled chart-of-accounts templates (docs/TEMPLATES.md).

    python3 scripts/build-coa-templates.py            build every template, update the list
    python3 scripts/build-coa-templates.py --check    change nothing; exit 1 if anything is out of date
    python3 scripts/build-coa-templates.py --method target/debug/method-budget
                                                       use a method binary already built

Every template is listed in templates/charts/charts.json with its source file:

    templates/charts/src/<id>.csv    our own charts (committed)
    imports/coa-sources/<file>       studio charts from open-format exports (never committed,
                                     CLEAN-ROOM.md); the folder is in .gitignore

Each source is turned into templates/charts/<id>.mthd by the importer itself
(`method-budget coa import <source> -o <id>.mthd --title <name>`), so a template is exactly
what importing its source gives, byte for byte, every time.

A file in imports/coa-sources/ that the manifest doesn't list yet is added to it: its id
is made from the file name, its name is the file name without the extension, its group is
guessed from the name (edit charts.json to change any of these). Add a line for it to
templates/charts/SOURCES.md saying where it came from.

A listed source that isn't on this machine (a studio export someone else has) is skipped
and its committed template kept as it is.

Last, crates/method-format/src/charts_bundled.rs (the list the app and `method-budget new
--template` bundle) is rewritten from the manifest.

Each template built from a `kind,code,name,parent` CSV is checked against it: the same
categories and account numbers, written as in the CSV, and as many accounts as the CSV
lists, less accounts named "." (unused slots) and repeats merged into the first (an
account number listed again with the same name).

Exit codes: 0 done (or up to date), 1 out of date (--check only), 2 a source failed to
import, a template doesn't match its source, or the manifest is wrong; the failing step
says why.
"""

import json
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CHARTS = os.path.join(ROOT, "templates", "charts")
MANIFEST = os.path.join(CHARTS, "charts.json")
SOURCES = os.path.join(ROOT, "imports", "coa-sources")
BUNDLED_RS = os.path.join(ROOT, "crates", "method-format", "src", "charts_bundled.rs")
SOURCE_EXTENSIONS = (".csv", ".tsv", ".xlsx", ".mthd")
GROUPS = ("Feature", "Television", "Commercial", "Other")


def fail(msg):
    print(f"build-coa-templates: {msg}", file=sys.stderr)
    sys.exit(2)


def slug(name):
    s = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")
    return s or "chart"


def guess_group(name):
    n = name.lower()
    if re.search(r"\b(tv|television|series|episodic|pilot|network|half hour|one hour)\b", n):
        return "Television"
    if re.search(r"\b(commercial|spot|music video|branded|advert)", n):
        return "Commercial"
    if re.search(r"\b(feature|film|movie|motion picture)", n):
        return "Feature"
    return "Other"


def load_manifest():
    with open(MANIFEST, encoding="utf-8") as f:
        m = json.load(f)
    charts = m.get("charts")
    if not isinstance(charts, list):
        fail("templates/charts/charts.json needs a \"charts\" list")
    seen = set()
    for e in charts:
        for key in ("id", "name", "group", "description", "source"):
            if not isinstance(e.get(key), str) or not e[key].strip():
                fail(f"chart {e.get('id', '?')}: \"{key}\" is missing")
        if e["group"] not in GROUPS:
            fail(f"chart {e['id']}: group must be one of {', '.join(GROUPS)}")
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", e["id"]):
            fail(f"chart {e['id']}: an id is lowercase letters, digits and hyphens")
        if e["id"] in seen:
            fail(f"chart {e['id']} is listed twice")
        if chr(0x2014) in e["name"] + e["description"]:
            fail(f"chart {e['id']}: no em dashes")
        seen.add(e["id"])
    return m


def add_new_sources(m):
    """Manifest entries for files in imports/coa-sources/ not listed yet."""
    if not os.path.isdir(SOURCES):
        return []
    listed = {os.path.normpath(e["source"]) for e in m["charts"]}
    ids = {e["id"] for e in m["charts"]}
    added = []
    for file in sorted(os.listdir(SOURCES)):
        if file.startswith(".") or not file.lower().endswith(SOURCE_EXTENSIONS):
            continue
        rel = os.path.normpath(os.path.join("imports", "coa-sources", file))
        if rel in listed:
            continue
        name = os.path.splitext(file)[0].strip()
        base = slug(name)
        cid, n = base, 2
        while cid in ids:
            cid, n = f"{base}-{n}", n + 1
        ids.add(cid)
        entry = {
            "id": cid,
            "name": name,
            "group": guess_group(name),
            "description": f"Follows the {name} chart of accounts",
            "source": rel.replace(os.sep, "/"),
        }
        m["charts"].append(entry)
        added.append(entry)
    return added


def method_command(args):
    if args.method:
        return [os.path.abspath(args.method)]
    return ["cargo", "run", "--quiet", "--locked", "-p", "method-cli", "--bin", "method-budget", "--"]


def build(entry, out, method):
    src = os.path.join(ROOT, entry["source"])
    cmd = method + ["coa", "import", src, "-o", out, "--title", entry["name"], "--force"]
    if entry.get("mapping"):
        cmd += ["--mapping", os.path.join(ROOT, entry["mapping"])]
    if entry.get("currency"):
        cmd += ["--currency", entry["currency"]]
    r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    if r.returncode != 0:
        fail(f"{entry['source']}: {r.stderr.strip() or r.stdout.strip()}")
    return r.stdout


def same_name(a, b):
    tidy = lambda s: " ".join(s.split()).lower()
    return tidy(a) == tidy(b)


def kinded_counts(path):
    """What a `kind,code,name,parent` CSV should import as: its category numbers and its
    account numbers, both as written, and the account count. Accounts named "." are unused
    slots (left out); an account number listed again with the same name is merged into the
    first (docs/TEMPLATES.md). `None` for any other layout."""
    import csv

    with open(path, encoding="utf-8-sig", newline="") as f:
        rows = list(csv.reader(f))
    if not rows:
        return None
    head = [h.strip().lower() for h in rows[0]]
    if not {"kind", "code", "name"} <= set(head):
        return None
    k, c, n = head.index("kind"), head.index("code"), head.index("name")
    cell = lambda r, i: r[i].strip() if i < len(r) else ""
    cats, codes, seen, accounts = [], set(), {}, 0
    for r in rows[1:]:
        kind = cell(r, k).lower().replace("-", "").replace(" ", "")
        code, name = cell(r, c), cell(r, n)
        if kind == "category" and code and code not in cats:
            cats.append(code)
        elif kind in ("account", "subaccount") and code and name != ".":
            if any(same_name(x, name) for x in seen.get(code, [])):
                continue
            seen.setdefault(code, []).append(name)
            codes.add(code)
            accounts += 1
    return {"categories": cats, "account_codes": codes, "accounts": accounts}


def built_counts(text):
    import json as _json

    rows = _json.loads(text)["budget"].get("topsheet", [])
    cats = [r for r in rows if r.get("kind") == "category"]
    return {
        "categories": [r["code"] for r in cats],
        "account_codes": {a["code"] for r in cats for a in r.get("accounts", [])},
        "accounts": sum(len(r.get("accounts", [])) for r in cats),
    }


def verify_counts(entry, text):
    """For a `kind,code,name,parent` source: the template has every category and account
    of the source, numbers as written. Returns what doesn't match (empty when all do)."""
    want = kinded_counts(os.path.join(ROOT, entry["source"]))
    if want is None:
        return []
    got = built_counts(text)
    problems = []
    if got["categories"] != want["categories"]:
        missing = [c for c in want["categories"] if c not in got["categories"]]
        problems.append(f"{len(got['categories'])} categories, the source has "
                        f"{len(want['categories'])} (missing: {', '.join(missing[:5]) or 'none'})")
    if got["accounts"] != want["accounts"]:
        problems.append(f"{got['accounts']} accounts, the source has {want['accounts']} "
                        "(after merging repeats and leaving out \".\" slots)")
    missing = sorted(want["account_codes"] - got["account_codes"])
    if missing:
        problems.append(f"account numbers not kept as written: {', '.join(missing[:5])}")
    return problems


def rust_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def bundled_rs(m):
    lines = [
        "// Written by scripts/build-coa-templates.py from templates/charts/charts.json:",
        "// do not edit. A test checks it agrees with the manifest.",
        "",
        "/// The bundled charts, in the manifest's order.",
        "pub static BUNDLED: &[Bundled] = &[",
    ]
    for e in m["charts"]:
        if not os.path.exists(os.path.join(CHARTS, f"{e['id']}.mthd")):
            continue
        lines += [
            "    Bundled {",
            f"        id: {rust_str(e['id'])},",
            f"        name: {rust_str(e['name'])},",
            f"        group: {rust_str(e['group'])},",
            f"        description: {rust_str(e['description'])},",
            f"        text: include_str!(\"../../../templates/charts/{e['id']}.mthd\"),",
            "    },",
        ]
    lines += ["];", ""]
    return "\n".join(lines)


def manifest_text(m):
    return json.dumps(m, indent=2, ensure_ascii=False) + "\n"


def read(path):
    try:
        with open(path, encoding="utf-8") as f:
            return f.read()
    except FileNotFoundError:
        return None


def main():
    import argparse

    p = argparse.ArgumentParser(description="Build the bundled chart-of-accounts templates.")
    p.add_argument("--check", action="store_true", help="change nothing; exit 1 if out of date")
    p.add_argument("--method", help="a method binary to use instead of cargo run")
    args = p.parse_args()

    m = load_manifest()
    added = add_new_sources(m)
    method = method_command(args)
    stale = []
    if added:
        stale.append("templates/charts/charts.json (new sources: " + ", ".join(e["source"] for e in added) + ")")

    with tempfile.TemporaryDirectory() as tmp:
        for e in m["charts"]:
            target = os.path.join(CHARTS, f"{e['id']}.mthd")
            if not os.path.exists(os.path.join(ROOT, e["source"])):
                state = "kept the committed template" if os.path.exists(target) else "not bundled"
                print(f"skip {e['id']}: {e['source']} isn't on this machine ({state})")
                continue
            out = os.path.join(tmp, f"{e['id']}.mthd")
            report = build(e, out, method)
            new = read(out)
            problems = verify_counts(e, new)
            if problems:
                fail(f"{e['source']}: the template doesn't match its source: " + "; ".join(problems))
            if new != read(target):
                stale.append(f"templates/charts/{e['id']}.mthd")
                if not args.check:
                    with open(target, "w", encoding="utf-8") as f:
                        f.write(new)
            first = report.strip().splitlines()[0] if report.strip() else ""
            print(f"{e['id']}: {first}")
            for line in report.strip().splitlines()[1:]:
                if line.startswith(("skipped:", "check:")):
                    print(f"  {line}")

    rs = bundled_rs(m)
    if rs != read(BUNDLED_RS):
        stale.append("crates/method-format/src/charts_bundled.rs")
    if args.check:
        if stale:
            print("out of date:\n  " + "\n  ".join(stale))
            sys.exit(1)
        print("up to date")
        return
    if added or manifest_text(m) != read(MANIFEST):
        with open(MANIFEST, "w", encoding="utf-8") as f:
            f.write(manifest_text(m))
    with open(BUNDLED_RS, "w", encoding="utf-8") as f:
        f.write(rs)
    for e in added:
        print(f"added {e['id']} ({e['group']}) to charts.json: check its name, group and "
              "description, and add it to templates/charts/SOURCES.md")
    print("done" if stale else "already up to date")


if __name__ == "__main__":
    main()
