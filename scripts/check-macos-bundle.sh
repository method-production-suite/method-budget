#!/usr/bin/env bash
# Inspects a built macOS bundle without opening it: architecture, minimum macOS,
# the document types, and the code signature.
# Usage: scripts/check-macos-bundle.sh [path/to/Method Budget.app]
set -euo pipefail

built="${CARGO_TARGET_DIR:-target}"
app="${1:-$built/release/bundle/macos/Method Budget.app}"
if [ ! -d "$app" ]; then
  echo "No bundle at: $app (build it with: cd crates/method-app && cargo tauri build --bundles app)" >&2
  exit 1
fi

plist="$app/Contents/Info.plist"
exe="$app/Contents/MacOS/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist")"

echo "== Bundle"
du -sh "$app"
file "$exe"
# Releases are universal (docs/INSTALL.md): a release bundle holds exactly Intel and
# Apple silicon code. A plain local build has only this Mac's, which is fine for its own use.
archs="$(lipo -archs "$exe")"
echo "Architectures: $archs"
if [ "$archs" != "x86_64 arm64" ]; then
  echo "NOTE: not universal; a release bundle must be (build with --target universal-apple-darwin)" >&2
fi
for key in CFBundleIdentifier CFBundleShortVersionString LSMinimumSystemVersion LSApplicationCategoryType NSHumanReadableCopyright; do
  printf '%s: %s\n' "$key" "$(/usr/libexec/PlistBuddy -c "Print :$key" "$plist")"
done

echo "== Document type (.mthd)"
if /usr/libexec/PlistBuddy -c "Print :CFBundleDocumentTypes:0:CFBundleTypeExtensions" "$plist" | grep -qx "[[:space:]]*mthd"; then
  echo "CFBundleDocumentTypes declares .mthd"
else
  echo "MISSING: CFBundleDocumentTypes:0 does not declare .mthd" >&2
  exit 1
fi
/usr/libexec/PlistBuddy -c "Print :UTExportedTypeDeclarations:0:UTTypeIdentifier" "$plist"
# The budget exports offered under Open With (rank Alternate; tauri.macos.conf.json).
/usr/libexec/PlistBuddy -c "Print :CFBundleDocumentTypes" "$plist" \
  | grep -E "CFBundleTypeName|LSHandlerRank|CFBundleTypeRole" || true

echo "== Signature"
codesign -dv "$app" 2>&1
codesign --verify --deep --strict "$app" && echo "codesign --verify: OK"
# Gatekeeper's verdict: "rejected" is expected for ad-hoc builds, "accepted" once
# signed with a Developer ID and notarized.
spctl -a -vv "$app" 2>&1 || true
