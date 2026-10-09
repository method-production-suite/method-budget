#!/usr/bin/env python3
"""Supply-chain cooldown check: fail if any crates.io package in Cargo.lock is too new.

Usage: python3 scripts/check-crate-ages.py [--days 14] [--lock Cargo.lock] [--only-changed REF]

Looks up each locked crates.io version's publish date (one request per second, per
crates.io's crawler policy) and lists any younger than the cooldown. With
--only-changed REF, only packages added or bumped since that git ref are checked.
Stdlib only, so it runs anywhere Python 3 does.
"""

import argparse
import datetime as dt
import json
import subprocess
import sys
import time
import tomllib
import urllib.request

UA = "film-wizard-budgeting cooldown check (https://github.com/ssandweiss/film-wizard-budgeting)"


def registry_packages(lock_text):
    lock = tomllib.loads(lock_text)
    return {
        (p["name"], p["version"])
        for p in lock.get("package", [])
        if p.get("source", "").startswith("registry+https://github.com/rust-lang/crates.io-index")
    }


def published(name, version):
    req = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}/{version}", headers={"User-Agent": UA}
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        v = json.load(r)["version"]
    return dt.datetime.fromisoformat(v["created_at"].replace("Z", "+00:00")), v["yanked"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--days", type=int, default=14)
    ap.add_argument("--lock", default="Cargo.lock")
    ap.add_argument("--only-changed", metavar="REF")
    args = ap.parse_args()

    with open(args.lock) as f:
        pkgs = registry_packages(f.read())
    if args.only_changed:
        old = subprocess.run(
            ["git", "show", f"{args.only_changed}:{args.lock}"], capture_output=True, text=True
        )
        if old.returncode == 0:
            pkgs -= registry_packages(old.stdout)

    now = dt.datetime.now(dt.timezone.utc)
    bad = []
    for i, (name, version) in enumerate(sorted(pkgs)):
        if i:
            time.sleep(1)
        when, yanked = published(name, version)
        age = (now - when).days
        flag = ""
        if age < args.days:
            flag = f"  TOO NEW (< {args.days} days)"
            bad.append(f"{name} {version}")
        if yanked:
            flag += "  YANKED"
            bad.append(f"{name} {version} (yanked)")
        print(f"{name:40} {version:16} {age:5} days{flag}")

    print(f"\nchecked {len(pkgs)} crates; {len(bad)} problem(s)")
    if bad:
        print("downgrade with: cargo update -p NAME --precise VERSION")
        sys.exit(1)


if __name__ == "__main__":
    main()
