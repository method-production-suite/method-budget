#!/usr/bin/env python3
"""Every git dependency in the locks is a commit on its repository's main branch.

deny.toml allows one git source, the suite's method-libs. GitHub serves a fork's commits
through the parent repository's address too, so a pinned commit that only exists in a fork
would still build. This asks GitHub's compare API whether each pinned commit is on main
(main is ahead of it or the same).

  python3 scripts/check-git-deps.py [--lock Cargo.lock ...]

GITHUB_TOKEN, when set, is sent so the API's rate limit is the token's, not the address's.
"""

import argparse
import json
import os
import re
import sys
import urllib.request

SOURCE = re.compile(r'^source = "git\+https://github\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)\?[^#"]*#([0-9a-f]{40})"$')


def pins(lock):
    """(owner, repo, sha) for every git source in `lock`."""
    out = set()
    with open(lock, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line.startswith('source = "git+'):
                m = SOURCE.match(line)
                if not m:
                    sys.exit(f"{lock}: a git source this check can't read: {line}")
                out.add(m.groups())
    return sorted(out)


def on_main(owner, repo, sha):
    """GitHub's compare status of main against `sha` ("behind" or "identical" is on main)."""
    url = f"https://api.github.com/repos/{owner}/{repo}/compare/main...{sha}"
    req = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json", "User-Agent": "method-check-git-deps"})
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)["status"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--lock", action="append", default=None)
    args = ap.parse_args()
    bad = 0
    for lock in args.lock or ["Cargo.lock", "fuzz/Cargo.lock"]:
        for owner, repo, sha in pins(lock):
            status = on_main(owner, repo, sha)
            ok = status in ("behind", "identical")
            print(f"{lock}: {owner}/{repo} {sha[:12]}: {status}{'' if ok else ' (not on main)'}")
            bad += not ok
    if bad:
        sys.exit(f"check-git-deps: {bad} pinned commit(s) not on their repository's main")
    print("check-git-deps: every git dependency is on its repository's main")


if __name__ == "__main__":
    main()
