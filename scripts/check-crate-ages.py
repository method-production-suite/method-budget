#!/usr/bin/env python3
"""Supply-chain cooldown check: fail if any crates.io package in Cargo.lock is too new.

Usage: python3 scripts/check-crate-ages.py [--days 14] [--lock Cargo.lock] [--only-changed REF]

Looks up each locked crates.io version's publish date (one request per second, per
crates.io's crawler policy) and lists any younger than the cooldown. A package from
anywhere else (a git repository, another registry) has no publish date to check, so it
fails the check too: fail closed. With --only-changed REF, only packages added or bumped
since that git ref are checked. Stdlib only, so it runs anywhere Python 3 does.

One exception, the suite's (SUITE-STANDARDS.md section 6, from Method Reader's copy): a
package from a git repository listed in scripts/crate-age-git-allow.txt passes when
Cargo.toml pins it with rev = the full 40-character commit (and the lock agrees), because a
pinned commit cannot change under us and the repository was reviewed when it was added to
that list. Anything else from git still fails.
"""

import argparse
import datetime as dt
import json
import pathlib
import re
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request

UA = "method-budget cooldown check (https://github.com/method-production-suite/method-budget)"


CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
GIT_ALLOW = pathlib.Path(__file__).with_name("crate-age-git-allow.txt")
# The manifest's own pin, not only the lock's: the lock writes the full commit
# after "#" whatever Cargo.toml asked for (a branch, a tag), so require the query
# to be rev=<that same 40-character commit>.
PINNED = re.compile(r"^git\+(https://[^?#]+)\?rev=([0-9a-f]{40})#\2$")


def git_allowed(source, allow_file=GIT_ALLOW):
    """True for a git source from an allowlisted repository, pinned to a full commit."""
    m = PINNED.match(source)
    if not m or not allow_file.exists():
        return False
    allowed = {line.split("#", 1)[0].strip() for line in allow_file.read_text().splitlines()} - {""}
    return m.group(1) in allowed


def external_packages(lock_text):
    """(name, version, source) of every locked package that isn't a workspace member."""
    lock = tomllib.loads(lock_text)
    return {
        (p["name"], p["version"], p["source"]) for p in lock.get("package", []) if p.get("source")
    }


# Rate limited (429) or briefly down (5xx): wait and ask again, longer each time, as
# crates.io's Retry-After says when it says. A full check makes hundreds of requests,
# and other repositories' checks share the limit, so one 429 must not fail the run.
RETRY_STATUS = {429, 500, 502, 503, 504}
RETRY_WAITS = (5, 15, 30, 60, 120, 300, 300)


def retry_wait(err, attempt):
    """Seconds to wait before asking again, or None to give up and raise. A network
    failure (a timeout, a dropped connection) is asked again like a 5xx."""
    if attempt >= len(RETRY_WAITS):
        return None
    if not isinstance(err, urllib.error.HTTPError):
        return RETRY_WAITS[attempt]
    if err.code not in RETRY_STATUS:
        return None
    after = (err.headers or {}).get("Retry-After", "")
    if after.strip().isdigit():
        return min(int(after), 300)
    return RETRY_WAITS[attempt]


def published(name, version, sleep=time.sleep, urlopen=urllib.request.urlopen):
    req = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}/{version}", headers={"User-Agent": UA}
    )
    attempt = 0
    while True:
        try:
            with urlopen(req, timeout=30) as r:
                v = json.load(r)["version"]
            break
        except (urllib.error.URLError, TimeoutError, ConnectionError) as err:
            wait = retry_wait(err, attempt)
            if wait is None:
                raise
            what = err.code if isinstance(err, urllib.error.HTTPError) else err
            print(f"crates.io: {what} for {name} {version}; asking again in {wait}s",
                  file=sys.stderr)
            sleep(wait)
            attempt += 1
    return dt.datetime.fromisoformat(v["created_at"].replace("Z", "+00:00")), v["yanked"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--days", type=int, default=14)
    ap.add_argument("--lock", default="Cargo.lock")
    ap.add_argument("--only-changed", metavar="REF")
    args = ap.parse_args()

    with open(args.lock) as f:
        pkgs = external_packages(f.read())
    if args.only_changed:
        old = subprocess.run(
            ["git", "show", f"{args.only_changed}:{args.lock}"], capture_output=True, text=True
        )
        if old.returncode == 0:
            pkgs -= external_packages(old.stdout)

    bad = []
    for name, version, source in sorted(p for p in pkgs if p[2] != CRATES_IO):
        if git_allowed(source):
            print(f"{name:40} {version:16}  pinned git commit, allowlisted repository")
            continue
        print(f"{name:40} {version:16}  NOT FROM CRATES.IO ({source}): no publish date to check")
        bad.append(f"{name} {version} ({source})")
    pkgs = {(name, version) for name, version, source in pkgs if source == CRATES_IO}

    now = dt.datetime.now(dt.timezone.utc)
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

    print(f"\nchecked {len(pkgs)} crates.io crates; {len(bad)} problem(s)")
    if bad:
        print("downgrade with: cargo update -p NAME --precise VERSION")
        sys.exit(1)


if __name__ == "__main__":
    main()
