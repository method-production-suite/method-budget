#!/usr/bin/env bash
# Inspects a built macOS bundle without opening it: architecture, minimum macOS,
# the .fwb document type, and the code signature.
# Usage: scripts/check-macos-bundle.sh [path/to/Film Wizard Budgeting.app]
set -euo pipefail

app="${1:-target/release/bundle/macos/Film Wizard Budgeting.app}"
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

echo "== .fwb document type"
if /usr/libexec/PlistBuddy -c 'Print :CFBundleDocumentTypes:0:CFBundleTypeExtensions' "$plist" | grep -qx '[[:space:]]*fwb'; then
  echo "CFBundleDocumentTypes declares .fwb"
else
  echo "MISSING: CFBundleDocumentTypes does not declare .fwb" >&2
  exit 1
fi
/usr/libexec/PlistBuddy -c 'Print :UTExportedTypeDeclarations:0:UTTypeIdentifier' "$plist"

echo "== Signature"
codesign -dv "$app" 2>&1
codesign --verify --deep --strict "$app" && echo "codesign --verify: OK"
# Gatekeeper's verdict: "rejected" is expected for ad-hoc builds, "accepted" once
# signed with a Developer ID and notarized.
spctl -a -vv "$app" 2>&1 || true
