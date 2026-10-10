#!/usr/bin/env bash
# Tests for the two Claude Code hook scripts beside this file, run in throwaway git
# repositories (nothing in this checkout is touched). Needs bash, git, jq and python3.
#
#     bash scripts/hooks/test-hooks.sh
#
# Exit 0 when every case gives the expected exit code; 1 otherwise. CI runs it in the
# `site` job of .github/workflows/ci.yml.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fails=0

payload() { jq -cn --arg c "$1" '{tool_name:"Bash",tool_input:{command:$c}}'; }

expect() { # expect <want-exit> <hook> <command> <label>
  local want="$1" hook="$2" cmd="$3" label="$4" got=0
  payload "$cmd" | "$here/$hook" >/dev/null 2>"$work/err" || got=$?
  if [ "$got" -eq "$want" ]; then
    printf 'ok    %s\n' "$label"
  else
    printf 'FAIL  %s (want exit %s, got %s)\n' "$label" "$want" "$got"
    sed 's/^/      /' "$work/err"
    fails=$((fails + 1))
  fi
}

gitq() { git -c user.name=test -c user.email=test@example.invalid -c commit.gpgsign=false "$@" >/dev/null; }

# ------------------------------------------------------------ require-release-notes
r1="$work/notes"
mkdir -p "$r1/docs/releases"
cd "$r1"
gitq init -q
echo "# Method Budget 0.1.0" > docs/releases/0.1.0.md
gitq add docs/releases/0.1.0.md
gitq commit -q -m "0.1.0"
echo "# Method Budget 0.2.0" > docs/releases/0.2.0.md   # on disk, not committed

H=require-release-notes.sh
expect 0 $H 'echo hello' 'unrelated command passes'
expect 0 $H 'git tag -a v0.1.0 -m "Method Budget 0.1.0"' 'tag with committed notes passes'
expect 2 $H 'git tag -a v0.2.0 -m "Method Budget 0.2.0"' 'tag with notes only on disk is blocked'
expect 2 $H 'git tag -a v0.3.0 -m x' 'tag with no notes is blocked'
expect 2 $H 'git commit -m 0.2.0 && git tag -a v0.2.0 -m x' 'commit and tag in one command is blocked'
expect 0 $H 'git tag -d v0.3.0' 'deleting a tag passes'
expect 0 $H 'git push origin :refs/tags/v0.3.0' 'pushing a tag deletion passes'
expect 0 $H 'git tag -d v0.3.0 && git push origin :refs/tags/v0.3.0' 'the two-step undo passes'
expect 2 $H 'git push origin v0.3.0 :refs/tags/v0.0.1' 'a tag pushed beside a deletion is blocked'
expect 2 $H 'git push --tags origin :refs/tags/v0.0.1' 'push --tags beside a deletion is blocked'
expect 0 $H 'git tag --list' 'listing tags passes'
expect 2 $H 'git push --atomic origin main v0.3.0' 'pushing an unreleased tag is blocked'
expect 0 $H 'git push --atomic origin main v0.1.0' 'pushing a released tag passes'
expect 2 $H 'git push --tags' 'push --tags with no version is blocked'
expect 0 $H 'grep -rn "git tag" docs/' 'a command that only mentions git tag passes'

# ------------------------------------------------------------ require-site-regen
r2="$work/site"
mkdir -p "$r2/docs" "$r2/scripts" "$r2/site"
cp "$repo/scripts/site.py" "$r2/scripts/"
cp "$repo/docs/features.toml" "$repo/docs/ROADMAP.md" "$repo/docs/decisions.md" "$r2/docs/"
cp "$repo/README.md" "$r2/"
cp "$repo/site/index.html" "$r2/site/"
cd "$r2"
gitq init -q
python3 scripts/site.py >/dev/null
gitq add -A
gitq commit -q -m base

H=require-site-regen.sh
expect 0 $H 'git commit -m "nothing staged"' 'commit with nothing relevant staged passes'
expect 0 $H 'cargo test' 'non-commit command passes'

python3 - <<'EOF'
from pathlib import Path
p = Path("docs/features.toml")
p.write_text(p.read_text().replace(
    'title = "Currencies"', 'title = "Currencies everywhere"'))
EOF
gitq add docs/features.toml
expect 2 $H 'git commit -m "edit features"' 'features staged without regenerating is blocked'
python3 scripts/site.py >/dev/null
expect 2 $H 'git commit -m "edit features"' 'regenerated but README and site not staged is blocked'
expect 0 $H 'git commit -am "edit features"' 'commit -a takes the regenerated files along'
gitq add README.md site/index.html
expect 0 $H 'git commit -m "edit features"' 'regenerated and staged passes'
gitq commit -q -m "edit features"

mkdir -p docs/releases
printf '# Method Budget 0.1.0\n\nFirst release.\n\n## New\n\n- **A thing.** It works.\n' > docs/releases/0.1.0.md
gitq add docs/releases/0.1.0.md
expect 2 $H 'git commit -m "notes"' 'release note staged without regenerating is blocked'
python3 scripts/site.py >/dev/null
gitq add site/index.html
expect 0 $H 'git commit -m "notes"' 'release note with regenerated site passes'

if [ "$fails" -ne 0 ]; then
  echo "$fails hook test(s) failed"
  exit 1
fi
echo "all hook tests passed"
