#!/usr/bin/env bash
#
# scripts/hooks/require-release-notes.sh
#
# Claude Code PreToolUse hook body for Bash tool calls. Wiring: docs/WEBSITE.md.
#
#
# THIS IS AN ERGONOMICS GUARD, NOT A SECURITY BOUNDARY. It only runs when
# Claude Code is about to execute a Bash command through this hook; a tag
# typed directly into a terminal, created by a different tool, or pushed
# from another machine entirely is never intercepted. Whatever gates a
# release for real lives in .github/workflows/ (ci.yml and release.yml),
# which run no matter how the tag got there. Forms it knowingly does NOT
# catch: `git -C .
# tag`, double-space spellings, `git update-ref refs/tags/...`,
# `git push --mirror`, and `gh api` calls that create a tag or release
# directly.
#
# Purpose: block creating or pushing a version tag (`git tag`, tag-moving
# `git push` forms, `gh release create`) when docs/releases/<version>.md
# is not committed at HEAD. Exit 2 blocks the pending tool call and shows
# stderr to the model; any other exit (this script always uses 0) lets it
# proceed.
#
# Recovery is guaranteed for the forms this script recognizes: `git tag
# -d`/`--delete`, `git push --delete`/`-d`, and pushing a tag deletion
# (`:refs/tags/...`, when the same push sends no other tag), checked before
# anything else that could block. A
# guard that blocks the fix is worse than no guard, because it fires
# exactly when someone is already undoing a mistake. This is a guarantee
# about recognized forms, not literally every string that deletes a tag --
# an unrecognized spelling fails closed like anything else this guard
# cannot resolve (see the version-extraction step below).
#
# When the command was cleanly extracted by jq, matching is done against
# actual shell SEGMENTS of the command, not the whole string and not only
# its first token. A segment boundary is any of `;`, `&`, `|` (which also
# covers `&&` and `||`) or an embedded newline; each segment is then
# stripped of a leading environment-variable assignment (`FOO=bar `) or a
# small set of shell keywords (`do`, `then`, `else`, `elif`, `while`,
# `until`, `if`) before being checked, so `cd /tmp && git tag --list`,
# `GIT_PAGER=cat git tag --list`, and `for x in 1; do git tag --list;
# done` are all recognized as the same read-only listing that
# `git tag --list` alone already was. Whether the command counts as
# COMPOUND at all is still decided by the ORIGINAL whole-string test
# below (a shell metacharacter present AND more than one `git
# tag`/`git push`/`gh release create` substring anywhere in the string),
# specifically because that cruder test also catches an invocation
# hidden inside `$(...)` or backticks that segment-splitting alone would
# never see (segment-splitting does not descend into either). "Compound"
# means BOTH a shell metacharacter is present AND more than one
# `git tag`/`git push`/`gh release create` invocation appears -- a plain
# metacharacter alone (`git tag --list | wc -l`) is not enough to trip
# it. On a compound command, the only exemption available is "every
# invocation present is a recognized recovery form" (e.g. `git tag -d v1
# && git push origin :refs/tags/v1`, the standard two-step undo) --
# outnumbering a real create with deletes elsewhere in the same line does
# not exempt it. On a non-compound command with exactly one real
# (segment-anchored) invocation, that segment is checked against both the
# recovery forms and the read-only tag-listing allowlist (`git tag`,
# `--list`, `-l`, `--contains`, `--points-at`, `--sort`, `--merged`,
# `--no-merged`, `--format`) -- but the listing allowlist is ALSO gated on
# naming no version at all, because no legitimate read-only form does,
# while a real `-a`/`-m` create can have "--list" or "--format" appear
# only incidentally, inside its message.
#
# A command whose tag-invocation-shaped text (e.g. the literal substring
# "git tag") never appears in command position in any top-level segment
# -- `grep -rn "git tag" docs/`, `echo "run git tag to list"` -- is not
# actually touching a tag at all and is waved through immediately,
# UNLESS the command also contains `$(...)` or a backtick, in which case
# a real invocation could be hiding out of segment-splitting's sight and
# this guard deliberately does not rule that out; it falls through to
# the version-extraction step below instead, which scans the raw string
# regardless of shell structure.
#
# When jq could not be used (missing, or the JSON did not parse), `cmd`
# is the raw, unparsed stdin payload, which does not start with `git` --
# it starts with `{`. The segment-anchored checks above would then never
# match, which would make recovery fail in exactly the degraded mode the
# raw fallback exists to serve. So in that mode ONLY, recovery reverts to
# the original bare-substring test instead: never blocking recovery
# outranks the precision segment-matching buys elsewhere.
#
# `git push` is treated as tag-touching both for the explicit bulk forms
# (`--tags`, `--follow-tags`, `refs/tags/`) AND whenever the command also
# names a vX.Y.Z token, because `git push --atomic origin main v0.5.0` --
# pushing a single tag by name, no flag at all -- is the exact command the
# release flow is designed to print.
#
# Version extraction is strict semver with an optional prerelease
# (`^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$` against the tag with the
# leading `v` stripped), so a prerelease like v0.5.0-rc1 requires
# docs/releases/0.5.0-rc1.md, not docs/releases/0.5.0.md. Every
# vX.Y.Z(-prerelease)? token in the command is checked, not just the
# first, so `git push --tags origin v0.5.0 v99.0.0` blocks regardless of
# which order the tags are listed in.
#
# The pending command arrives as model-generated JSON on stdin, at
# .tool_input.command. That string is ONLY EVER matched against with
# plain substring/regex checks -- it is never eval'd, never passed to
# `bash -c`, and never allowed to undergo command substitution. Doing
# otherwise would let a crafted command execute before this guard, or the
# user's own permission prompt, ever sees it.

set -euo pipefail

# ---------------------------------------------------------------------
# Small helpers used only by the segment-anchored matching below.
# ---------------------------------------------------------------------

# Trims leading/trailing whitespace. Safe on an all-whitespace or empty
# string (returns empty).
trim() {
  local s="$1"
  s="${s#"${s%%[![:space:]]*}"}"
  s="${s%"${s##*[![:space:]]}"}"
  printf '%s' "$s"
}

# Strips a leading run of environment-variable assignments
# (`FOO=bar BAZ=qux `) and/or a small set of shell keywords (`do`,
# `then`, `else`, `elif`, `while`, `until`, `if`) from the front of a
# segment, so the invocation-anchored checks below see `git tag --list`
# whether the source was `git tag --list`, `GIT_PAGER=cat git tag
# --list`, or `do git tag --list` (the shape a `for`/`while` body takes
# once split on its own `;`).
normalize_segment() {
  local s="$1" changed=1
  while [ "$changed" -eq 1 ]; do
    changed=0
    if [[ "$s" =~ ^(do|then|else|elif|while|until|if)[[:space:]]+(.*)$ ]]; then
      s="${BASH_REMATCH[2]}"
      changed=1
      continue
    fi
    if [[ "$s" =~ ^[A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+(.*)$ ]]; then
      s="${BASH_REMATCH[1]}"
      changed=1
    fi
  done
  printf '%s' "$s"
}

# A normalized segment is a recognized recovery form: `git tag -d`,
# `git tag --delete`, pushing a tag deletion (`:refs/tags/...`, with no
# other tag pushed in the same segment), or `git push --delete`/`git push -d`.
is_recovery_form() {
  local s="$1"
  if [[ "$s" =~ ^git[[:space:]]+tag[[:space:]]+-d([[:space:]]|$) ]]; then return 0; fi
  if [[ "$s" =~ ^git[[:space:]]+tag[[:space:]]+--delete([[:space:]]|$) ]]; then return 0; fi
  if [[ "$s" == *":refs/tags/"* ]]; then
    # Only when every tag the segment pushes is a deletion: with the
    # `:refs/tags/NAME` refspecs taken out, nothing tag-shaped may remain
    # (`git push origin v9.9.9 :refs/tags/v0.0.1` creates v9.9.9).
    local rest
    rest="$(printf '%s' "$s" | sed -E 's#:refs/tags/[^[:space:]]*##g')"
    if [[ "$rest" != *"--tags"* ]] && [[ "$rest" != *"--follow-tags"* ]] && [[ "$rest" != *"refs/tags/"* ]] \
      && ! printf '%s' "$rest" | grep -qE 'v[0-9]+\.[0-9]+\.[0-9]+'; then
      return 0
    fi
  fi
  if [[ "$s" =~ ^git[[:space:]]+push[[:space:]]+(--delete|-d)([[:space:]]|$) ]]; then return 0; fi
  return 1
}

# ---------------------------------------------------------------------
# Step 1: read stdin without ever crashing, even on no input at all.
# ---------------------------------------------------------------------
if [ -t 0 ]; then
  # Nothing piped in (e.g. run by hand with no input). Nothing to block.
  exit 0
fi

raw="$(cat 2>/dev/null || true)"

if [ -z "$raw" ]; then
  exit 0
fi

# ---------------------------------------------------------------------
# Step 2: pull .tool_input.command out of the JSON with jq. Tolerate jq
# being missing or the JSON being malformed: fall back to scanning the
# raw payload itself so a broken/absent jq degrades to "match on raw
# text", never to "crash" and never to "silently allow everything".
# ---------------------------------------------------------------------
cmd=""
extracted=0
if command -v jq >/dev/null 2>&1; then
  if cmd="$(printf '%s' "$raw" | jq -r '.tool_input.command // empty' 2>/dev/null)"; then
    extracted=1
  fi
fi

if [ "$extracted" -eq 0 ]; then
  # jq missing, or the JSON did not parse: we truly cannot read the
  # command out. Fall back to the raw payload as the text to scan. If
  # that does not look tag-touching either, we exit 0 below --
  # under-blocking on a hook malfunction beats blocking unrelated work
  # (e.g. `cargo test`) just because `jq` was not on PATH.
  cmd="$raw"
fi

# ---------------------------------------------------------------------
# Step 3: cheap bail-out for anything that is not tag-touching, before
# doing any git work or deciding anything about recovery. Deliberately a
# WHOLE-STRING substring test, not segment-anchored: "git tag" appearing
# anywhere -- inside a quoted argument, inside $(...), after a `#`
# comment -- still counts here, so an invocation hidden inside command
# substitution is never waved through by this step alone. Whether it is
# REALLY an invocation gets refined below, once that is safe to decide.
# ---------------------------------------------------------------------
has_version_token=0
if printf '%s' "$cmd" | grep -qE 'v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?'; then
  has_version_token=1
fi

is_tag_touching=0
if [[ "$cmd" == *"git tag"* ]]; then
  is_tag_touching=1
elif [[ "$cmd" == *"gh release create"* ]]; then
  is_tag_touching=1
elif [[ "$cmd" == *"git push"* ]] \
  && { [[ "$cmd" == *"--tags"* ]] || [[ "$cmd" == *"--follow-tags"* ]] || [[ "$cmd" == *"refs/tags/"* ]] || [ "$has_version_token" -eq 1 ]; }; then
  is_tag_touching=1
fi

if [ "$is_tag_touching" -eq 0 ]; then
  exit 0
fi

# ---------------------------------------------------------------------
# Step 4: recovery forms and read-only tag listing are never blocked.
# ---------------------------------------------------------------------

# Compound detection: a shell metacharacter alone does not make a
# command compound -- `git tag --list | wc -l` must still be allowed.
# What matters is whether MORE THAN ONE tag-touching invocation appears
# in the string, so: a metacharacter is present AND more than one of
# `git tag` / `git push` / `gh release create` appears. This is a
# whole-string test on purpose (see the header comment): it is what
# catches an invocation hidden inside `$(...)`/backticks that the
# segment-splitting below cannot see into.
meta=0
# '$(' below is a literal 2-char pattern to match against, not a
# substitution to expand; that is the whole point of this case match.
# shellcheck disable=SC2016
case "$cmd" in
  *';'*|*'&'*|*'|'*|*'$('*|*'`'*|*$'\n'*) meta=1 ;;
esac

n_invocations="$(printf '%s' "$cmd" | grep -oE 'git tag|git push|gh release create' | wc -l)"

is_compound=0
if [ "$meta" -eq 1 ] && [ "$n_invocations" -gt 1 ]; then
  is_compound=1
fi

if [ "$extracted" -eq 1 ]; then
  # cmd is the real command text: split it into shell segments and find
  # every one that is ACTUALLY a git-tag/git-push/gh-release-create
  # invocation in command position (see the header comment for why this
  # is not a full shell parser and does not need to be). Each element of
  # touching_segments contributes at least one occurrence counted by
  # n_invocations above, so whenever is_compound is 0, this array has at
  # most one element.
  touching_segments=()
  # `tr` is translating three DISTINCT separator characters to newline,
  # not deduplicating a word list; the three-`\n` replacement set is
  # intentional, one per input character. The trailing `\n` appended by
  # the `printf` before it matters: without it, a command whose last
  # segment is the tag-touching one loses that segment entirely, because
  # `read` discards a final line with no trailing newline.
  # shellcheck disable=SC2020
  segments="$(printf '%s\n' "$cmd" | tr ';&|' '\n\n\n')"
  while IFS= read -r line; do
    seg="$(trim "$line")"
    [ -z "$seg" ] && continue
    norm="$(normalize_segment "$seg")"
    if [[ "$norm" =~ ^git[[:space:]]+tag([[:space:]]|$) ]]; then
      touching_segments+=("$norm")
    elif [[ "$norm" =~ ^gh[[:space:]]+release[[:space:]]+create([[:space:]]|$) ]]; then
      touching_segments+=("$norm")
    elif [[ "$norm" =~ ^git[[:space:]]+push([[:space:]]|$) ]]; then
      seg_has_version=0
      if printf '%s' "$norm" | grep -qE 'v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?'; then
        seg_has_version=1
      fi
      if [[ "$norm" == *"--tags"* ]] || [[ "$norm" == *"--follow-tags"* ]] || [[ "$norm" == *"refs/tags/"* ]] || [ "$seg_has_version" -eq 1 ]; then
        touching_segments+=("$norm")
      fi
    fi
  done <<< "$segments"

  if [ "${#touching_segments[@]}" -eq 0 ]; then
    # Nothing in command position is a real invocation: every occurrence
    # of tag-shaped text is inert (a grep pattern, an echoed sentence).
    # Only safe to trust when there is also no $(...)/backtick that
    # could be hiding a real invocation out of segment-splitting's
    # sight -- if there is, fall through instead; Step 5/6 below still
    # scan the raw string for a version token regardless of structure.
    has_exec_risk=0
    # '$(' below is a literal 2-char pattern to match against, not a
    # substitution to expand; same idiom as the compound-detection case
    # match above.
    # shellcheck disable=SC2016
    case "$cmd" in
      *'$('*|*'`'*) has_exec_risk=1 ;;
    esac
    if [ "$has_exec_risk" -eq 0 ]; then
      exit 0
    fi
  fi

  if [ "$is_compound" -eq 1 ]; then
    # More than one real invocation, joined by a metacharacter. The only
    # exemption available here is "every invocation present is a
    # recognized recovery form" -- a real create anywhere in the line
    # forfeits the exemption for the WHOLE command; it is not enough for
    # the create to be outnumbered by deletes.
    all_recovery=1
    if [ "${#touching_segments[@]}" -eq 0 ]; then
      all_recovery=0
    fi
    for seg in "${touching_segments[@]+"${touching_segments[@]}"}"; do
      if ! is_recovery_form "$seg"; then
        all_recovery=0
        break
      fi
    done
    if [ "$all_recovery" -eq 1 ]; then
      exit 0
    fi
  elif [ "${#touching_segments[@]}" -eq 1 ]; then
    seg="${touching_segments[0]}"
    if is_recovery_form "$seg"; then
      exit 0
    fi

    # No legitimate read-only listing form names a version, so gate this
    # on has_version_token too: a create whose MESSAGE merely mentions
    # "--list" or "--format" must still be checked, not waved through.
    if [ "$has_version_token" -eq 0 ] && [[ "$seg" =~ ^git[[:space:]]+tag([[:space:]]+.*)?$ ]]; then
      if [[ "$seg" =~ ^git[[:space:]]+tag[[:space:]]*$ ]]; then
        exit 0
      fi
      if [[ "$seg" == *"--list"* ]] || [[ "$seg" == *"--contains"* ]] || [[ "$seg" == *"--points-at"* ]] \
        || [[ "$seg" == *"--sort"* ]] || [[ "$seg" == *"--merged"* ]] || [[ "$seg" == *"--no-merged"* ]] \
        || [[ "$seg" == *"--format"* ]] || [[ "$seg" =~ [[:space:]]-l([[:space:]]|$) ]]; then
        exit 0
      fi
    fi
  fi
else
  # jq was missing, or the JSON did not parse: cmd is the raw, unparsed
  # payload, which may be wrapped in JSON punctuation the segment-based
  # checks above were never meant to see through (it starts with `{`,
  # not `git`). Recovery must never break just because jq is absent, so
  # fall back to the original bare-substring test here -- deliberately
  # NOT extended to the read-only listing allowlist: a false block on a
  # listing command in this already-degraded mode is an acceptable cost,
  # a false PASS on recovery is not.
  if [[ "$cmd" == *"tag -d"* ]] || [[ "$cmd" == *"tag --delete"* ]] || [[ "$cmd" == *":refs/tags/"* ]] \
    || [[ "$cmd" == *"push --delete"* ]] || [[ "$cmd" == *"push -d"* ]]; then
    exit 0
  fi
fi

# ---------------------------------------------------------------------
# Step 5: extract every vX.Y.Z(-prerelease)? token, deduplicated. Strict
# match, leading v stripped.
# ---------------------------------------------------------------------
versions=""
if matches="$(printf '%s' "$cmd" | grep -oE 'v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?' | sort -u)"; then
  versions="$matches"
fi

if [ -z "$versions" ]; then
  truncated="$cmd"
  if [ "${#truncated}" -gt 200 ]; then
    truncated="${truncated:0:200}...[truncated]"
  fi
  cat >&2 <<EOF
require-release-notes: blocked -- this command touches version tags but
no vX.Y.Z version could be found in it:

  $truncated

Failing closed: this guard cannot tell whether release notes exist for
an unresolved version, so it blocks rather than risk a tag shipping
without docs/releases/<version>.md. This is an ergonomics guard, not a
security boundary.

Run /release to create the tag and its release notes together.

Recovery is not blocked by this: "git tag -d ...", "git tag --delete
...", "git push --delete ..."/"git push -d ...", and pushing a tag
deletion (":refs/tags/...") are always allowed, and so is read-only tag
listing ("git tag", "git tag --list", ...). An unrecognized spelling of
any of those still fails closed like anything else this guard cannot
resolve.
EOF
  exit 2
fi

# ---------------------------------------------------------------------
# Step 6: check git, not the filesystem, for EVERY version found. A file
# present in the working tree but not committed would otherwise pass
# while the tag ships without it; checking only the first version found
# would let a second, unreleased version ride along depending on token
# order.
# ---------------------------------------------------------------------
missing=""
while IFS= read -r v; do
  [ -z "$v" ] && continue
  ver="${v#v}"
  notes_path="docs/releases/${ver}.md"
  if ! git cat-file -e "HEAD:${notes_path}" 2>/dev/null; then
    if [ -z "$missing" ]; then
      missing="$notes_path"
    else
      missing="${missing}, ${notes_path}"
    fi
  fi
done <<< "$versions"

if [ -z "$missing" ]; then
  exit 0
fi

cat >&2 <<EOF
require-release-notes: blocked -- release notes are not committed at
HEAD for: ${missing}

Tagging or pushing a version without release notes ships a version
nobody can read the changelog for. This is an ergonomics guard, not a
security boundary.

Run /release to write the missing notes and create the tag together.

Recovery is not blocked by this: "git tag -d ...", "git tag --delete
...", "git push --delete ..."/"git push -d ...", and pushing a tag
deletion (":refs/tags/...") are always allowed, and so is read-only tag
listing ("git tag", "git tag --list", ...). An unrecognized spelling of
any of those still fails closed like anything else this guard cannot
resolve.
EOF
exit 2
