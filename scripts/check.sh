#!/usr/bin/env bash
# The full local check: everything a pull request has to pass before `gh pr ready`
# (SUITE-STANDARDS.md section 6). Pull requests on GitHub run only the dependency checks
# (crate ages and cargo deny); the weekly build (.github/workflows/weekly.yml) runs this on
# Linux and the tests on every system. Paste its last line under "Tested locally".
#
#   scripts/check.sh            everything
#   scripts/check.sh --quick    without the browser build and its smoke test
set -euo pipefail
cd "$(dirname "$0")/.."

quick=no
[ "${1:-}" = --quick ] && quick=yes

step() { printf '\n== %s\n' "$*"; }

# Every checkout of a repository on this machine may share one build folder
# (CARGO_TARGET_DIR); cargo says where it is.
built="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"

step "format";               cargo fmt --all -- --check
step "clippy";               cargo clippy --workspace --all-targets --locked -- -D warnings
step "tests"
tests="$(cargo test --workspace --locked --no-fail-fast 2>&1 | tee /dev/stderr | grep '^test result')"
passed="$(printf '%s\n' "$tests" | awk '{s+=$4} END {print s}')"
failed="$(printf '%s\n' "$tests" | awk '{f+=$6} END {print f}')"
[ "$failed" = 0 ] || { echo "check.sh: $failed tests failed" >&2; exit 1; }
step "Field Station";        cargo build --locked -p method-cli
METHOD="$built/debug/method-budget" python3 scripts/make-field-station.py --check
step "third-party notices";  cargo xtask notices --check
step "version and notes";    cargo xtask check-version
step "site and features";    python3 scripts/site.py --check
step "brand files";          python3 scripts/make-brand.py --check
step "hook scripts";         bash scripts/hooks/test-hooks.sh
step "WebAssembly";          cargo xtask wasm-check
if [ "$quick" = no ]; then
  step "the page in a browser"
  cargo xtask web
  node scripts/web-smoke.mjs "$built/web"
fi

echo
echo "check.sh: all checks passed ($passed tests passed, 0 failed$([ "$quick" = yes ] && echo ', browser skipped'))"
