#!/usr/bin/env bash
# Turn raw app captures into the website's screenshot files.
#
#   scripts/make-screenshots.sh RAW_DIR
#
# RAW_DIR holds 2x (retina) window captures named <name>.png, taken from the real app
# with the invented budgets from scripts/make-showcase-budgets.py (docs/WEBSITE.md,
# "Screenshots", says how). For each one this writes into site/assets/shots/, at half
# the capture's size (a 2880x1800 capture of a 1440x900 window becomes 1440x900, twice the
# width the gallery shows it at):
#
#   <name>.webp   what browsers load
#   <name>.png    256 colors, the fallback for a browser without WebP
#
# Needs cwebp, ffmpeg and sips (macOS).
set -euo pipefail
cd "$(dirname "$0")/.."
raw="${1:?usage: scripts/make-screenshots.sh RAW_DIR}"
out=site/assets/shots
mkdir -p "$out"
for src in "$raw"/*.png; do
  name="$(basename "$src" .png)"
  w=$(sips -g pixelWidth "$src" | awk '/pixelWidth/ {print $2}')
  half=$((w / 2))
  cwebp -quiet -q 82 -m 6 -resize "$half" 0 "$src" -o "$out/$name.webp"
  ffmpeg -loglevel error -y -i "$src" -filter_complex \
    "scale=${half}:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=256:stats_mode=full[p];[b][p]paletteuse=dither=none" \
    "$out/$name.png"
done
ls -l "$out"
echo "total bytes: $(cat "$out"/* | wc -c | tr -d ' ')"
