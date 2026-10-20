#!/usr/bin/env python3
# The Method Production Suite's shared crate-age check (SUITE-STANDARDS.md section 6).
# This file lives in method-libs, and every suite repository copies it VERBATIM into its
# own scripts/ folder. Change it here first, then copy it out; never edit a copy in place.
# Each repository keeps its own scripts/crate-age-exceptions.txt and
# scripts/crate-age-git-allow.txt beside it.
"""Supply-chain cooldown check: fail if any crates.io package in Cargo.lock is too new.

Usage: python3 scripts/check-crate-ages.py [--days 14] [--lock Cargo.lock]
                                          [--only-changed REF] [--release]
                                          [--exceptions FILE] [--git-allow FILE]

Reads each locked crates.io version's publish time ("pubtime") and yanked flag from
crates.io's sparse index (index.crates.io, the CDN cargo itself reads; one file per crate
holds every version), so even a whole-lock check makes one request per crate and no API
calls. A version the index gives no pubtime is dated by its file on static.crates.io
(Last-Modified is never earlier than the publish date, so an old file date proves the
version old), and only then by the crates.io API, one request a second. Every request
retries a rate limit (429), a server error (5xx) or a network failure, honouring
Retry-After, waiting 5, 15, 30, 60, 120, 300 and 300 seconds before giving up.

A locked version younger than the cooldown fails, with the `cargo update --precise`
command that moves it back to the newest caret-compatible, unyanked version old enough.
A yanked version always fails.

A package from anywhere else (a git repository, another registry) has no publish date
to check, so it fails too: fail closed. The one exception is a git repository listed in
scripts/crate-age-git-allow.txt, and only when Cargo.toml pins it with rev = the full
40-character commit (so the lock reads git+URL?rev=SHA#SHA), because a pinned commit
cannot change under us and the repository was reviewed when it was added to the list.

scripts/crate-age-exceptions.txt lists exact versions allowed under the cooldown, one
"NAME VERSION  # reason" per line, of two kinds:
  - security: the reason names a RUSTSEC id; a young version that fixes an advisory.
  - age-out: anything else; a version that cannot be pinned back, with the date it turns
    14 days old, so main stays green while it ages.
--release (the release workflow, before any build or signing step) honours only the
security entries, so no release ships while an age-out entry is still young. Once a
listed version is old enough, or no longer locked, the check warns so its line is removed.

With --only-changed REF (pull requests), only packages added or bumped since that git
ref are checked. Standard library only, so it runs anywhere Python 3.11+ does.
"""

import argparse
import datetime as dt
import email.utils
import json
import pathlib
import re
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request

UA = "method-production-suite crate-age check (https://github.com/method-production-suite/method-libs)"

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
HERE = pathlib.Path(__file__).resolve().parent
EXCEPTIONS = HERE / "crate-age-exceptions.txt"
GIT_ALLOW = HERE / "crate-age-git-allow.txt"
# The manifest's own pin, not only the lock's: the lock writes the full commit after "#"
# whatever Cargo.toml asked for (a branch, a tag), so require the query to be
# rev=<that same 40-character commit>.
PINNED = re.compile(r"^git\+(https://[^?#]+)\?rev=([0-9a-f]{40})#\2$")


# ---------------------------------------------------------------------------------------
# Network: every request goes through fetch(), which retries.

# Rate limited (429) or briefly down (5xx): wait and ask again, longer each time, as
# crates.io's Retry-After says when it says. A full check makes hundreds of requests, and
# other repositories' checks share the limit, so one 429 must not fail the run.
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


def fetch(url, method="GET", sleep=time.sleep, urlopen=urllib.request.urlopen):
    """(body text, headers) of a request, retried per retry_wait(). Anything not worth
    retrying (a 404, say) raises at once."""
    req = urllib.request.Request(url, method=method, headers={"User-Agent": UA})
    attempt = 0
    while True:
        try:
            with urlopen(req, timeout=30) as r:
                return r.read().decode(), r.headers
        except (urllib.error.URLError, TimeoutError, ConnectionError) as err:
            wait = retry_wait(err, attempt)
            if wait is None:
                raise
            what = err.code if isinstance(err, urllib.error.HTTPError) else (str(err) or type(err).__name__)
            print(f"{url}: {what}; asking again in {wait}s", file=sys.stderr)
            sleep(wait)
            attempt += 1


def parse_time(text):
    return dt.datetime.fromisoformat(text.replace("Z", "+00:00"))


def index_path(name):
    """A crate's file in crates.io's sparse index (the layout cargo reads)."""
    n = name.lower()
    if len(n) <= 2:
        return f"{len(n)}/{n}"
    if len(n) == 3:
        return f"3/{n[0]}/{n}"
    return f"{n[:2]}/{n[2:4]}/{n}"


def parse_index(text):
    """{version: {"pubtime": datetime or None, "yanked": bool}} from a sparse-index file
    (one JSON object per line)."""
    out = {}
    for line in text.splitlines():
        if not line.strip():
            continue
        e = json.loads(line)
        when = e.get("pubtime")
        out[e["vers"]] = {"pubtime": parse_time(when) if when else None,
                          "yanked": bool(e.get("yanked"))}
    return out


class Registry:
    """crates.io, read through fetch(). sleep and urlopen are parameters so tests can
    pass fakes and never touch the network."""

    def __init__(self, sleep=time.sleep, urlopen=urllib.request.urlopen):
        self.sleep = sleep
        self.urlopen = urlopen
        self.indexes = {}
        self.api_calls = 0

    def get(self, url, method="GET"):
        return fetch(url, method=method, sleep=self.sleep, urlopen=self.urlopen)

    def index(self, name):
        if name not in self.indexes:
            text, _ = self.get(f"https://index.crates.io/{index_path(name)}")
            self.indexes[name] = parse_index(text)
        return self.indexes[name]

    def stored_at(self, name, version):
        """When static.crates.io last wrote the .crate file (its Last-Modified), or None.
        Never earlier than the publish date (files are sometimes rewritten later, never
        before), so a file this shows as old enough is old enough."""
        try:
            _, headers = self.get(
                f"https://static.crates.io/crates/{name}/{name}-{version}.crate", method="HEAD")
        except urllib.error.HTTPError:
            return None
        stamp = headers.get("Last-Modified") if headers else None
        return email.utils.parsedate_to_datetime(stamp) if stamp else None

    def api_published(self, name, version):
        """(publish time, yanked) from the crates.io API, one request a second as its
        crawler policy asks."""
        if self.api_calls:
            self.sleep(1)
        self.api_calls += 1
        text, _ = self.get(f"https://crates.io/api/v1/crates/{name}/{version}")
        v = json.loads(text)["version"]
        return parse_time(v["created_at"]), bool(v["yanked"])

    def published(self, name, version, cutoff):
        """(publish time or None, yanked). None means only "on or before cutoff", proven
        by the file date; the exact time was not needed."""
        e = self.index(name).get(version)
        if e and e["pubtime"]:
            return e["pubtime"], e["yanked"]
        if e:
            stored = self.stored_at(name, version)
            if stored and stored <= cutoff:
                return None, e["yanked"]
        return self.api_published(name, version)


# ---------------------------------------------------------------------------------------
# Lock files, exceptions and the git allowlist.


def external_packages(lock_text):
    """(name, version, source) of every locked package that isn't a workspace member."""
    lock = tomllib.loads(lock_text)
    return {
        (p["name"], p["version"], p["source"]) for p in lock.get("package", []) if p.get("source")
    }


def read_exceptions(path):
    """{(name, version): "security" or "age-out"}. A line whose reason names a RUSTSEC
    advisory is a security entry; any other is an age-out entry."""
    path = pathlib.Path(path)
    if not path.exists():
        return {}
    out = {}
    for line in path.read_text().splitlines():
        entry, _, reason = line.partition("#")
        words = entry.split()
        if len(words) == 2:
            out[(words[0], words[1])] = "security" if "RUSTSEC-" in reason else "age-out"
    return out


def read_git_allow(path):
    """The set of repository URLs in the git allowlist."""
    path = pathlib.Path(path)
    if not path.exists():
        return set()
    return {line.split("#", 1)[0].strip() for line in path.read_text().splitlines()} - {""}


def git_allowed(source, allowed):
    """True for a git source from an allowlisted repository, pinned to a full commit."""
    m = PINNED.match(source)
    return bool(m) and m.group(1) in allowed


# ---------------------------------------------------------------------------------------
# The --precise suggestion.


def semver(version):
    """(major, minor, patch) of a release (build metadata such as `+spec-1.1.0` ignored),
    or None for a pre-release."""
    m = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:\+[0-9A-Za-z.-]+)?", version)
    return tuple(int(x) for x in m.groups()) if m else None


def compatible(a, b):
    """Cargo's caret rule: the same leftmost non-zero component."""
    if a[0] != b[0]:
        return False
    return a[0] != 0 or (a[1] == b[1] and (a[1] != 0 or a[2] == b[2]))


def pin_to(index, locked, cutoff):
    """The newest unyanked version in a crate's index, caret-compatible with and older than
    the locked one, published on or before the cutoff; or None. A version the index gives
    no pubtime is skipped: the suggestion names only versions it can prove old enough."""
    want = semver(locked)
    if not want:
        return None
    ok = []
    for v, e in index.items():
        have = semver(v)
        if (have and have < want and compatible(have, want) and not e["yanked"]
                and e["pubtime"] and e["pubtime"] <= cutoff):
            ok.append((have, v))
    return max(ok)[1] if ok else None


# ---------------------------------------------------------------------------------------
# The check.


def check(pkgs, registry, *, now, days=14, exceptions=None, git_allow=frozenset(),
          release=False, full=True, out=print):
    """Check (name, version, source) packages. Returns (problems, pins, warnings): the
    failures, the `cargo update --precise` commands that fix the young ones, and the
    exception lines to remove. full=False (an --only-changed run) sees only some of the
    lock, so it cannot say an exception is no longer locked."""
    exceptions = exceptions or {}
    cutoff = now - dt.timedelta(days=days)
    bad, pins, warnings = [], [], []

    for name, version, source in sorted(p for p in pkgs if p[2] != CRATES_IO):
        if git_allowed(source, git_allow):
            out(f"{name:40} {version:16}  pinned git commit, allowlisted repository")
            continue
        out(f"{name:40} {version:16}  NOT FROM CRATES.IO ({source}): no publish date to check")
        bad.append(f"{name} {version} ({source})")

    crates = sorted({(n, v) for n, v, s in pkgs if s == CRATES_IO})
    for name, version in crates:
        when, yanked = registry.published(name, version, cutoff)
        young = when is not None and when > cutoff
        age = f"{(now - when).days:5} days" if when else "   old enough"
        kind = exceptions.get((name, version))
        flag = ""
        if young and kind == "security":
            flag = "  young, a security exception"
        elif young and kind == "age-out" and not release:
            flag = "  young, an age-out exception (no release until it is old enough)"
        elif young:
            flag = f"  TOO NEW (< {days} days)"
            if kind == "age-out":
                flag += ", an age-out exception, which a release does not honour"
            target = pin_to(registry.index(name), version, cutoff)
            if target:
                flag += f", pin to {target}"
                pins.append(f"cargo update -p {name}@{version} --precise {target}")
            else:
                flag += ", no older compatible version"
            bad.append(f"{name} {version}")
        elif kind:
            warnings.append(f"{name} {version} is {days}+ days old now")
        if yanked:
            flag += "  YANKED"
            bad.append(f"{name} {version} (yanked)")
        out(f"{name:40} {version:16} {age}{flag}")

    if full:
        locked = set(crates)
        for name, version in sorted(set(exceptions) - locked):
            warnings.append(f"{name} {version} is not in the lock")
    return bad, pins, warnings


def main(argv=None, registry=None, now=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    ap.add_argument("--days", type=int, default=14)
    ap.add_argument("--lock", default="Cargo.lock")
    ap.add_argument("--only-changed", metavar="REF",
                    help="check only packages added or bumped since this git ref")
    ap.add_argument("--release", action="store_true",
                    help="for a release: honour only security exceptions, not age-out ones")
    ap.add_argument("--exceptions", default=str(EXCEPTIONS), metavar="FILE")
    ap.add_argument("--git-allow", default=str(GIT_ALLOW), metavar="FILE")
    args = ap.parse_args(argv)

    with open(args.lock) as f:
        pkgs = external_packages(f.read())
    if args.only_changed:
        old = subprocess.run(
            ["git", "show", f"{args.only_changed}:{args.lock}"], capture_output=True, text=True
        )
        # No lock at that ref (or a bad ref): check everything, fail closed.
        if old.returncode == 0:
            pkgs -= external_packages(old.stdout)

    exceptions = read_exceptions(args.exceptions)
    bad, pins, warnings = check(
        pkgs,
        registry or Registry(),
        now=now or dt.datetime.now(dt.timezone.utc),
        days=args.days,
        exceptions=exceptions,
        git_allow=read_git_allow(args.git_allow),
        release=args.release,
        full=not args.only_changed,
    )

    crates = sum(1 for p in pkgs if p[2] == CRATES_IO)
    print(f"\nchecked {crates} crates.io crates; {len(bad)} problem(s)")
    # Not a failure: a listed version that has aged past the cooldown, or left the lock,
    # only needs its line removed. A warning, so the list stays honest.
    for w in warnings:
        print(f"::warning::{w}: remove its line from {pathlib.Path(args.exceptions).name}")
    if bad:
        if pins:
            print("move back with:")
            for pin in pins:
                print(f"  {pin}")
        print("a young version that cannot move back may be listed in "
              f"{pathlib.Path(args.exceptions).name} with its age-out date "
              "(or its RUSTSEC id, for a security fix)")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
