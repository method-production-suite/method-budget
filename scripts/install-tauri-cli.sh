#!/usr/bin/env bash
# Installs the Tauri CLI (`cargo tauri`) from its prebuilt release on GitHub instead of
# compiling it (`cargo install tauri-cli` took 9 of a release's 18 macOS minutes). The
# version and each file's SHA-256 are pinned here: a download that doesn't match is
# refused. Used by .github/workflows/release.yml and nightly.yml.
#
# To move to a new version: take the digests GitHub lists for the tauri-cli-vX.Y.Z
# release's assets (`gh release view tauri-cli-vX.Y.Z -R tauri-apps/tauri --json assets`),
# check the release is at least 14 days old (SUITE-STANDARDS.md section 6), and change
# VERSION and the three lines below together.
#
# The Linux build needs glibc 2.35 (Ubuntu 22.04, the release's Linux runner).
set -euo pipefail

VERSION=2.12.1

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) asset=cargo-tauri-aarch64-apple-darwin.zip
    sha=ee35ee364a0c626df7852d9a5bc0c5394b6440772e9864cfb9b11dcb8d33f527 ;;
  Linux-x86_64) asset=cargo-tauri-x86_64-unknown-linux-gnu.tgz
    sha=04ef3a9a2ed7b7dc479bef08ab40deee80065c99fc778f79ebce775eb72bbeff ;;
  MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64) asset=cargo-tauri-x86_64-pc-windows-msvc.zip
    sha=d3919ebe0bf7013dd98293862784dd37b8f76db2d650e13a3ec11c5dd799c519 ;;
  *) echo "install-tauri-cli: no pinned build for $(uname -s) $(uname -m)" >&2; exit 1 ;;
esac

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
url="https://github.com/tauri-apps/tauri/releases/download/tauri-cli-v$VERSION/$asset"
curl --proto '=https' --tlsv1.2 -fsSL --retry 3 -o "$work/$asset" "$url"

if command -v sha256sum >/dev/null; then
  got="$(sha256sum "$work/$asset" | cut -d' ' -f1)"
else
  got="$(shasum -a 256 "$work/$asset" | cut -d' ' -f1)"
fi
if [ "$got" != "$sha" ]; then
  echo "install-tauri-cli: $asset has SHA-256 $got, expected $sha" >&2
  exit 1
fi

mkdir -p "$work/out"
case "$asset" in
  *.tgz) tar -xzf "$work/$asset" -C "$work/out" ;;
  *.zip) unzip -q "$work/$asset" -d "$work/out" ;;
esac

bin="${CARGO_HOME:-$HOME/.cargo}/bin"
mkdir -p "$bin"
for exe in cargo-tauri cargo-tauri.exe; do
  if [ -f "$work/out/$exe" ]; then
    install -m 0755 "$work/out/$exe" "$bin/$exe"
  fi
done
cargo tauri --version
