# Installing Method Budget

Method Budget is a local desktop app ([APP.md](APP.md)). This page covers building
it from source, installing it on a Mac, the one-time Gatekeeper step for builds that are
not notarized, and what a signed, notarized release needs.

## Download a release

Pushing a version tag (`v0.1.0` and so on) runs `.github/workflows/release.yml`, which
builds every platform and drafts a GitHub Release with the files attached:

| Platform | Files | Status |
|---|---|---|
| macOS 11 or later, Intel and Apple silicon (one universal app) | `Method-Budget-macOS.dmg`, and the `.app` as `Method-Budget-macOS.app.zip` | signed and notarized once the release secrets are set (until then ad-hoc signed) |
| Windows 10/11 (x64) | `Method-Budget-windows-x64.msi` and the NSIS `Method-Budget-windows-x64-setup.exe` | unsigned, untested |
| Linux (x64) | `Method-Budget-linux-amd64.deb` and `Method-Budget-linux-x86_64.AppImage` | untested |

Releases are universal again from 0.6: one app with both Intel and Apple silicon code.
0.3 to 0.5 were for Apple silicon only, so an Intel Mac stayed on 0.2; Help > Check for
Updates on that copy now finds 0.6. A copy of 0.3 to 0.5 on an Intel Mac (built from
source) says "There's no update for this computer yet" until it reads 0.6's manifest.

The draft is published by hand once it has been checked ([RELEASING.md](RELEASING.md)).
Until the macOS build is signed with a Developer ID and notarized, the first launch needs
the Gatekeeper step below; the release's description says when that applies.

## Build from source (macOS)

You need Rust (stable, via [rustup](https://rustup.rs)) and the Tauri CLI, pinned:

```bash
cargo install tauri-cli --version 2.11.4 --locked
```

Then, from the repository root:

```bash
cd crates/method-app
cargo tauri build --bundles app          # just the .app (quickest)
cargo tauri build --bundles app,dmg      # the .app and a .dmg installer
```

The app lands in `target/release/bundle/macos/Method Budget.app` (at the repository
root, not inside `crates/method-app`), and the disk image in `target/release/bundle/dmg/`.
Making the `.dmg` briefly opens and arranges a Finder window; that is normal.

That build runs on your Mac's own architecture. Releases are universal, built the way the
release workflow does it (both targets, joined with `lipo`):

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo tauri build --bundles app,dmg --target universal-apple-darwin
# lands in target/universal-apple-darwin/release/bundle/
```

To check a built bundle without opening it (architecture, minimum macOS, the `.mthd`
document type and the signature):

```bash
scripts/check-macos-bundle.sh
```

### Install to /Applications

Drag `Method Budget.app` into Applications in Finder (from the `.dmg` window, or
from `target/release/bundle/macos/`), or in Terminal:

```bash
ditto "target/release/bundle/macos/Method Budget.app" "/Applications/Method Budget.app"
```

A released copy updates itself (once releases carry the updater key: [RELEASING.md](RELEASING.md)):
when a new version is ready the toolbar says "Update ready: Restart to update", and restarting saves your budgets first and reopens your tabs
(Help > Check for Updates Automatically turns the checks off; [APP.md](APP.md), "Updates").
A copy built from source doesn't check; to update it, quit the app and replace it the same
way. Your budgets are `.mthd` files wherever you saved them; installing, updating or
removing the app never touches them.

The app registers the `.mthd` file type ("Method Budget"), so
Finder shows those files as belonging to it and offers it under Open With; double-clicking one opens it in the
app (asking first if the open budget has unsaved changes). It also offers itself under
Open With, never as the default, for the budget exports it reads (`.json`, `.xml`, `.xlsx`,
`.tsv`): opening one there converts it to a new budget, after a line-by-line check, and
leaves the export as it was (the build's `tauri.macos.conf.json`; the Windows and Linux
installers register `.mthd` only).

### First launch: "unidentified developer"

A build that is not signed with a Developer ID and notarized by Apple (every build from
source, and the current release files) is blocked by Gatekeeper the first time. A build you
made yourself on the same Mac usually opens without this; a downloaded one will not. Do this
once per installed copy:

1. Double-click the app. macOS says it can't verify the developer; click **Done** (or
   **Cancel**), not Move to Trash.
2. Open **System Settings > Privacy & Security**, scroll to **Security**, and click
   **Open Anyway** next to the message about Method Budget. Confirm with your
   password or Touch ID, then click **Open**.

On macOS 14 and earlier, right-clicking (or Control-clicking) the app in Finder and choosing
**Open**, then **Open** again in the dialog, does the same thing. macOS 15 and later removed
that shortcut, so use System Settings there.

After that, the app opens normally. Each new version you download needs the step again.

## Windows and Linux (untested)

The same command builds the native installers on each platform; none of them has been tested
yet.

```bash
cd crates/method-app
cargo tauri build --bundles msi,nsis       # Windows: target/release/bundle/msi/ and nsis/
cargo tauri build --bundles deb,appimage   # Linux:   target/release/bundle/deb/ and appimage/
```

Windows needs the WebView2 runtime (built into Windows 11 and current Windows 10). Linux needs
the WebKitGTK development packages listed in [APP.md](APP.md). Unsigned Windows installers
show a SmartScreen warning: click **More info**, then **Run anyway**.

## Signed and notarized macOS releases

Signing with a Developer ID and notarizing removes the Gatekeeper step. It needs an Apple
Developer Program membership. Nothing below is stored in this repository: every secret
lives in the maintainers' password manager (1Password) and is read at build time with
`op read` or `op run` and `op://` references.

What is needed:

| What | Where it comes from | Tauri reads it from |
|---|---|---|
| **Developer ID Application** certificate (not "Apple Development", not "Mac App Store") | developer.apple.com > Certificates, or Xcode > Settings > Accounts > Manage Certificates | the login keychain, named by `APPLE_SIGNING_IDENTITY` |
| Signing identity name, like `Developer ID Application: Name (TEAMID)` | `security find-identity -v -p codesigning` | `APPLE_SIGNING_IDENTITY` |
| Team ID (10 characters) | developer.apple.com > Account > Membership details | `APPLE_TEAM_ID` |
| Notarization credentials, **either** an Apple ID with an app-specific password ... | appleid.apple.com > Sign-In and Security > App-Specific Passwords | `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` |
| ... **or** an App Store Connect API key (preferred for CI; role Developer) | App Store Connect > Users and Access > Integrations > Team Keys: key ID, issuer ID and the `.p8` file (downloadable once) | `APPLE_API_KEY` (key ID), `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH` (path to the `.p8`) |
| For CI only: the certificate exported as a `.p12` with a password, base64-encoded | Keychain Access > export the certificate with its private key; `base64 -i cert.p12` | `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` |

`tauri.conf.json` sets `bundle.macOS.signingIdentity` to `-` (ad-hoc). `APPLE_SIGNING_IDENTITY`
overrides it, and when the notarization variables are present the build also notarizes and
staples the app. A local signed build looks like this (the `op://` paths are examples; use
the real item names):

```bash
cd crates/method-app
export APPLE_SIGNING_IDENTITY="$(op read 'op://<team vault>/<item>/signing identity')"
export APPLE_TEAM_ID="$(op read 'op://<team vault>/<item>/team id')"
export APPLE_API_KEY="$(op read 'op://<team vault>/<item>/key id')"
export APPLE_API_ISSUER="$(op read 'op://<team vault>/<item>/issuer id')"
op read --out-file "$TMPDIR/AuthKey.p8" 'op://<team vault>/<item>/AuthKey.p8'
export APPLE_API_KEY_PATH="$TMPDIR/AuthKey.p8"
cargo tauri build --bundles app,dmg --target aarch64-apple-darwin
rm -f "$TMPDIR/AuthKey.p8"
```

Afterwards `scripts/check-macos-bundle.sh` should show a `TeamIdentifier` and Gatekeeper
`accepted` (source: Notarized Developer ID).

The release workflow signs, notarizes, staples and verifies the Mac build itself once its
secrets are set (an App Store Connect API key for notarizing, the certificate as a `.p12`);
the secrets, the commands that set them and the updater key: [RELEASING.md](RELEASING.md).
