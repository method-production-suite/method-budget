#!/usr/bin/env bash
# Inspects a built macOS bundle without opening it: architecture, minimum macOS,
# the .mthd and .fwb document types, and the code signature.
# Usage: scripts/check-macos-bundle.sh [path/to/Method Budget.app]
set -euo pipefail

app="${1:-target/release/bundle/macos/Method Budget.app}"
if [ ! -d "$app" ]; then
  echo "No bundle at: $app (build it with: cd crates/fwb-app && cargo tauri build --bundles app)" >&2
  exit 1
fi

plist="$app/Contents/Info.plist"
exe="$app/Contents/MacOS/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist")"

echo "== Bundle"
du -sh "$app"
file "$exe"
for key in CFBundleIdentifier CFBundleShortVersionString LSMinimumSystemVersion LSApplicationCategoryType NSHumanReadableCopyright; do
  printf '%s: %s\n' "$key" "$(/usr/libexec/PlistBuddy -c "Print :$key" "$plist")"
done

echo "== Document types (.mthd, and the older .fwb)"
for i in 0 1; do
  want=$([ "$i" = 0 ] && echo mthd || echo fwb)
  if /usr/libexec/PlistBuddy -c "Print :CFBundleDocumentTypes:$i:CFBundleTypeExtensions" "$plist" | grep -qx "[[:space:]]*$want"; then
    echo "CFBundleDocumentTypes declares .$want"
  else
    echo "MISSING: CFBundleDocumentTypes:$i does not declare .$want" >&2
    exit 1
  fi
  /usr/libexec/PlistBuddy -c "Print :UTExportedTypeDeclarations:$i:UTTypeIdentifier" "$plist"
done

echo "== Signature"
codesign -dv "$app" 2>&1
codesign --verify --deep --strict "$app" && echo "codesign --verify: OK"
# Gatekeeper's verdict: "rejected" is expected for ad-hoc builds, "accepted" once
# signed with a Developer ID and notarized.
spctl -a -vv "$app" 2>&1 || true
