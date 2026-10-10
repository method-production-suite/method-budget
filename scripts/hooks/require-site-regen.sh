#!/usr/bin/env bash
#
# scripts/hooks/require-site-regen.sh
#
# Claude Code PreToolUse hook body for Bash tool calls. Wiring: docs/WEBSITE.md.
#
# Purpose: block a `git commit` that would commit docs/features.toml or a release note
# (docs/releases/*.md) without the README.md and site/index.html that
# `python3 scripts/site.py` generates from them. It blocks when either
#
#   1. a staged change touches docs/features.toml, docs/releases/*.md, README.md or
#      site/index.html, and `python3 scripts/site.py --check` says the working tree is
#      out of date; or
#   2. docs/features.toml or a release note is staged while README.md or site/index.html
#      has changes that are NOT staged (regenerated, but left out of the commit).
#
# Like require-release-notes.sh, THIS IS AN ERGONOMICS GUARD, NOT A SECURITY BOUNDARY.
# A commit typed in a terminal is never seen here. The real check is the `site` job in
# .github/workflows/ci.yml, which runs `python3 scripts/site.py --check` on every push.
#
# It decides from the repository's state, not from parsing the command: the command is
# only searched for the text `git commit`. So a false positive (a command that merely
# mentions `git commit`) is blocked only when the tree really is out of date, which is
# worth fixing anyway. The command text is never eval'd or run.
#
# Exit 2 blocks the pending tool call and shows stderr to the model; exit 0 lets it run.
# Anything this script cannot work out (no git, no python3, not in the repository)
# lets the command through: CI still checks.

set -euo pipefail

if [ -t 0 ]; then
  exit 0
fi
raw="$(cat 2>/dev/null || true)"
[ -z "$raw" ] && exit 0

cmd="$raw"
if command -v jq >/dev/null 2>&1; then
  if parsed="$(printf '%s' "$raw" | jq -r '.tool_input.command // empty' 2>/dev/null)"; then
    cmd="$parsed"
  fi
fi

case "$cmd" in
  *"git commit"*) ;;
  *) exit 0 ;;
esac

root="$(git rev-parse --show-toplevel 2>/dev/null)" || exit 0
cd "$root"
[ -f scripts/site.py ] || exit 0
command -v python3 >/dev/null 2>&1 || exit 0

staged="$(git diff --cached --name-only 2>/dev/null || true)"
# `git commit -a` / `--all` stages tracked changes as it commits; count those too.
if [[ "$cmd" =~ git[[:space:]]+commit([[:space:]].*)?[[:space:]](-a|--all|-[A-Za-z]*a[A-Za-z]*)([[:space:]]|$) ]]; then
  staged="$staged"$'\n'"$(git diff --name-only 2>/dev/null || true)"
fi

source_staged=0
touches_site=0
while IFS= read -r f; do
  case "$f" in
    docs/features.toml|docs/releases/*.md) source_staged=1; touches_site=1 ;;
    README.md|site/index.html) touches_site=1 ;;
  esac
done <<< "$staged"

[ "$touches_site" -eq 1 ] || exit 0

if ! check_out="$(python3 scripts/site.py --check 2>&1)"; then
  cat >&2 <<EOF
require-site-regen: blocked -- README.md or site/index.html is out of date with
docs/features.toml or the newest release note:

$check_out

Run \`python3 scripts/site.py\`, stage README.md and site/index.html with the rest,
then commit again. (This is an ergonomics guard; CI runs the same check.)
EOF
  exit 2
fi

if [ "$source_staged" -eq 1 ]; then
  unstaged="$(git diff --name-only -- README.md site/index.html 2>/dev/null || true)"
  if [ -n "$unstaged" ] && ! [[ "$cmd" =~ git[[:space:]]+commit([[:space:]].*)?[[:space:]](-a|--all|-[A-Za-z]*a[A-Za-z]*)([[:space:]]|$) ]]; then
    cat >&2 <<EOF
require-site-regen: blocked -- the feature list or a release note is staged, but the
regenerated file(s) below are not:

$unstaged

Stage them too (git add README.md site/index.html), then commit again.
EOF
    exit 2
  fi
fi

exit 0
