#!/usr/bin/env bash
# Render site/og-card.html to site/og.png at 1200x630, the size social link previews
# expect. Re-run after editing og-card.html, then commit og.png with it.
#
# Uses headless Chrome (nothing else on a stock Mac draws HTML faithfully). It runs with
# a throwaway profile, so it never touches, or hands off to, a Chrome you have open, and
# it opens no window. Set CHROME to use another Chromium-based browser.
#
set -euo pipefail
cd "$(dirname "$0")/.."
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
[ -x "$CHROME" ] || { echo "make-og: no Chrome at $CHROME (set CHROME=...)" >&2; exit 1; }
profile="$(mktemp -d)"
out="$profile/og.png"
pid=""
cleanup() { [ -n "$pid" ] && { kill "$pid" 2>/dev/null || true; }; rm -rf "$profile"; }
trap cleanup EXIT
# Recent Chrome can write the screenshot and then keep running, so run it in the
# background, wait for the file, and stop it ourselves (up to 30 seconds).
"$CHROME" --headless --disable-gpu --hide-scrollbars --no-first-run \
  --no-default-browser-check --user-data-dir="$profile" \
  --screenshot="$out" --window-size=1200,630 \
  "file://$PWD/site/og-card.html" >/dev/null 2>&1 &
pid=$!
for _ in $(seq 60); do
  if [ -s "$out" ] && ! kill -0 "$pid" 2>/dev/null; then break; fi
  if [ -s "$out" ]; then sleep 1; break; fi
  sleep 0.5
done
[ -s "$out" ] || { echo "make-og: Chrome wrote no picture" >&2; exit 1; }
mv "$out" site/og.png
if command -v sips >/dev/null 2>&1; then
  sips -g pixelWidth -g pixelHeight site/og.png
else
  echo "wrote site/og.png"
fi
