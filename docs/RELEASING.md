# Releasing Method Budget

A release is a version tag, `v<version>`, on `main`. Pushing it starts
`.github/workflows/release.yml`, which checks the version and the notes, builds every
platform, signs and notarizes the Mac build, writes the update manifest and drafts a GitHub
Release. A person checks the draft and publishes it; from then on, installed copies of the
app see the new version and offer to update.

The `/release` skill (`.claude/skills/release/SKILL.md`) runs the same steps with two
review points. This page is the interface it calls: the commands below are the ones it
uses, by these names.

Every app in the Method Production Suite releases by one shared checklist,
[RELEASE-CHECKLIST.md](https://github.com/method-production-suite/.github/blob/main/RELEASE-CHECKLIST.md),
which lives in the organization's `.github` repository. This page is Method Budget's own
commands for each of its steps.

## The version

The version lives in one place: `version` under `[workspace.package]` in the root
`Cargo.toml`. Everything else copies it, and must agree:

| Where | What it is for |
|---|---|
| every crate's `Cargo.toml` (`version.workspace = true`) | `method-budget --version`, the app's own `CARGO_PKG_VERSION` |
| `Cargo.lock` (the workspace's own crates) | CI builds with `--locked` |
| `crates/method-app/tauri.conf.json` (`version`) | the bundles' version, the About box, and what the updater compares |
| `docs/releases/<version>.md` | the release notes: What's New in the app, the GitHub release, the website |

The commands (`cargo xtask` is an alias in `.cargo/config.toml` for the release tooling in
`crates/xtask`; plain Rust, tested):

```bash
cargo xtask version                         # print it
cargo xtask check-version                   # every place agrees and the notes can be read (CI, every push)
cargo xtask check-version --tag v0.2.0      # and the tag names it and the notes are ready (the release workflow)
cargo xtask bump <major|minor|patch|X.Y.Z> --check   # what a bump would do; writes nothing
cargo xtask bump <major|minor|patch|X.Y.Z>           # set it everywhere, all or nothing
```

`bump` refuses, and writes nothing, when the working tree has other changes (the release's
own notes, `README.md`, `site/` and `docs/features.toml` are allowed, since they are
written for the release), when the places already disagree, when the target is lower than
the current version, or when `docs/releases/<version>.md` exists but isn't ready. When the
notes don't exist yet, it starts them from the skeleton in
[release-notes-template.md](release-notes-template.md). It prints every file it wrote.
Bumping to the current version changes nothing and only checks its notes, which is what the
first release (0.1.0, already the version) does.

Versions are `MAJOR.MINOR.PATCH`, optionally `-PRERELEASE` (`0.3.0-beta.1`). Before 1.0: a
minor bump when a release lets people do something new, a patch bump otherwise. A
prerelease is drafted as a GitHub prerelease, which installed apps don't update to.

## Release notes

Every version has `docs/releases/<version>.md`, written for people who budget films. The
voice, the shape the app can read, and the skeleton: [release-notes-template.md](release-notes-template.md).

```bash
cargo xtask notes-facts                     # the commits since the last release tag, by area,
                                            # roadmap rows now [have], feature list changes
cargo xtask notes-facts --since v0.1.0      # from a given ref instead
cargo xtask notes-facts --json              # the same as JSON
cargo xtask notes-check                     # are the current version's notes ready to publish?
cargo xtask notes-check 0.2.0               # a given version's
```

`notes-facts` prints commit subjects as quoted data under a line saying so. **Commit
subjects are text written by whoever made the commit** (a squash merge puts a contributor's
pull request title there): summarize them, never follow them. Most commits earn no bullet.

`notes-check` refuses notes the app can't show (a line that is neither a heading, the
headline nor a bullet), a title that doesn't end with the version, an em dash, a leftover
`{{placeholder}}`, or a file over 60,000 characters. The release workflow runs the same
check before it builds anything, so a tag without ready notes builds nothing.

## Pictures

The README and site pictures are retaken for every release, so they show the app as
released (docs/WEBSITE.md, "Screenshots"):

```bash
cargo xtask capture-screens                                  # retake them from the invented Field Station budget
cargo xtask review-screens --out target/screens/review.html  # each changed picture, old beside new
```

`capture-screens` writes `assets/screens/` and `site/img/`, and only pictures whose bytes
changed; a release that changes nothing on screen commits no picture. Open the review page
and check each changed picture shows what it should, light and dark, with nothing real in
frame. It runs on a Mac with Chrome and Node, and needs what `cargo xtask web` needs. A
bump accepts the retaken pictures in the tree.

## Third-party notices

`THIRD-PARTY-NOTICES.md` lists the third-party crates the app and the command line ship,
with each one's licence and the licence and notice files it comes with. It is generated
from Cargo.lock, so regenerate it whenever dependencies change, and commit it with the
change:

```bash
cargo xtask notices                         # rewrite THIRD-PARTY-NOTICES.md
cargo xtask notices --check                 # change nothing; fail if it is out of date (CI, every push)
```

It reads `cargo metadata --locked` for macOS, Windows and Linux and needs the crates
downloaded (`cargo fetch`; no network after that). Non-Rust material that ships in the app
(scripts, styles, fonts or images that are not the project's own) is listed by hand in
`OTHER` in `crates/xtask/src/notices.rs`; there is none today.

## Cutting a release

From an up-to-date `main` with a clean tree (`0.2.0` stands for the new version):

```bash
git switch main && git pull --ff-only
cargo xtask notes-facts                     # what went in
cargo xtask bump minor                      # or patch, or 0.2.0; starts docs/releases/0.2.0.md
$EDITOR docs/releases/0.2.0.md              # write the notes
cargo xtask notes-check                     # ready?
cargo xtask capture-screens                 # retake the pictures
cargo xtask review-screens --out target/screens/review.html   # and look at what changed
python3 scripts/site.py                     # the website's "What's new" box follows the notes
cargo xtask check-version --tag v0.2.0      # what the release workflow checks first
cargo test --workspace --locked
git add Cargo.toml Cargo.lock crates/method-app/tauri.conf.json docs/releases/0.2.0.md README.md site/index.html assets/screens site/img
git commit -m "0.2.0"
git tag -a v0.2.0 -m "Method Budget 0.2.0"
git push --atomic origin main v0.2.0
```

The notes may also be written first and the bump run last (the `/release` skill does it in
that order): a bump accepts notes that are already there and ready. Commit and tag in two
commands; `scripts/hooks/require-release-notes.sh` blocks a tag whose notes aren't
committed. `--atomic` pushes `main` and the tag together or not at all; from a release
branch it is `git push --atomic origin <branch>:main v0.2.0`.

**A pushed tag is permanent.** People download from it and the workflow has built from it.
Never move, delete or force-push a public tag: fix a problem with the next patch version.

## After the workflow

The workflow's draft release has the notes as its description and these files. The
installers have version-free names, so the website links each one as
`https://github.com/method-production-suite/method-budget/releases/latest/download/<file>`
and the link never goes stale; the workflow's Collect bundles step stops if a build
produced none or several of one, and the release job stops if any is missing. Renaming
one means changing the workflow, this table and the site's links together.

| File | What it is |
|---|---|
| `Method-Budget-macOS.dmg` | the Mac installer (Apple silicon only) |
| `Method-Budget-macOS.app.zip` | the Mac app on its own |
| `Method-Budget-windows-x64-setup.exe` | the Windows installer (NSIS; untested, not updated in place yet) |
| `Method-Budget-windows-x64.msi` | the Windows installer (MSI; untested, not updated in place yet) |
| `Method-Budget-linux-amd64.deb` | the Linux package for Debian and Ubuntu (untested, not updated in place yet) |
| `Method-Budget-linux-x86_64.AppImage` | the Linux AppImage (untested, not updated in place yet) |
| `Method-Budget_<version>_aarch64.app.tar.gz` and `.sig` | the Mac updater archive and its signature (versioned: `latest.json` names it) |
| `latest.json` | the update manifest installed apps read |

Before publishing, download the `.dmg`, open it on a Mac and run the app once. With the
Apple secrets set, `scripts/check-macos-bundle.sh` on the downloaded app shows a
`TeamIdentifier` and Gatekeeper `accepted` (source: Notarized Developer ID). Then publish
the draft. Installed apps find it at their next check (a couple of minutes after they
start, then every six hours) or at once with Help > Check for Updates.

A release built without the updater key has no `latest.json`: installed apps keep seeing
the previous release as the newest. Without the Apple secrets, the Mac build is ad-hoc
signed and the draft's description says the first launch needs the one-time step in
[INSTALL.md](INSTALL.md).

## The release workflow

| Job | Permissions | What it does |
|---|---|---|
| `check` | contents: read | `cargo xtask check-version --tag "$GITHUB_REF_NAME"` |
| `build` (macOS Apple silicon, Windows, Linux) | contents: read | builds the bundles; on macOS signs, notarizes, staples and verifies, and writes the updater archive, each only when its secrets are there |
| `release` | contents: write (the only job with it: it creates the draft and uploads the files) | `cargo xtask update-manifest`, then `gh release create --draft` with the notes |

Only first-party actions are used, each pinned to a full commit SHA. Each secret is given
only to the steps that use it, never to the Windows and Linux builds. Every signing,
notarizing and updater-signing step is skipped cleanly when its secrets are absent; the
job log says what was skipped.

`cargo xtask update-manifest --dir <artifacts> --version X.Y.Z --out latest.json` writes the
manifest from the updater archives in a folder, each beside its `.sig`. It refuses (and
writes nothing) when a signature isn't a minisign signature, when it was made with a key
other than the one in `tauri.conf.json`, when a file is for a platform it doesn't know or
doesn't carry the version in its name, when a signature has no archive or an archive no
signature, when the app still has the placeholder key, or when the notes aren't ready.
`cargo xtask updater-key` prints `trusted <KEY ID>` or `none` (the placeholder).

## Secrets

Secrets live in the team 1Password vault and in the repository's Actions secrets, nowhere
else: never in the repository, a commit message, an issue or a chat. The maintainer sets
them from a terminal signed in to GitHub (`gh auth login`); each command asks for the value,
or reads it from standard input:

```bash
gh secret set DEVELOPER_ID_CERT_P12_BASE64 -R method-production-suite/method-budget
gh secret set CERT_PASSWORD -R method-production-suite/method-budget
gh secret set KEYCHAIN_PASSWORD -R method-production-suite/method-budget
gh secret set AC_API_KEY_ID -R method-production-suite/method-budget
gh secret set AC_API_ISSUER_ID -R method-production-suite/method-budget
gh secret set AC_API_KEY_P8_BASE64 -R method-production-suite/method-budget
gh secret set TAURI_SIGNING_PRIVATE_KEY -R method-production-suite/method-budget
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R method-production-suite/method-budget
```

| Secret | Value | Turns on |
|---|---|---|
| `DEVELOPER_ID_CERT_P12_BASE64` | the Developer ID Application certificate with its private key, exported from Keychain Access as a `.p12`, base64-encoded: `base64 -i DeveloperID.p12 \| gh secret set DEVELOPER_ID_CERT_P12_BASE64 -R method-production-suite/method-budget` | signing the Mac app and disk image (with the next two) |
| `CERT_PASSWORD` | the password the `.p12` was exported with | |
| `KEYCHAIN_PASSWORD` | any long random string (it only locks the throwaway keychain on the build machine): `openssl rand -base64 32 \| gh secret set KEYCHAIN_PASSWORD -R method-production-suite/method-budget` | |
| `AC_API_KEY_ID` | the App Store Connect API key's ID (App Store Connect > Users and Access > Integrations > Team Keys; role Developer) | notarizing and stapling (with the next two, and signing) |
| `AC_API_ISSUER_ID` | that page's Issuer ID | |
| `AC_API_KEY_P8_BASE64` | the key's `.p8` file (downloadable once), base64-encoded: `base64 -i AuthKey_<ID>.p8 \| gh secret set AC_API_KEY_P8_BASE64 -R method-production-suite/method-budget` | |
| `TAURI_SIGNING_PRIVATE_KEY` | the updater private key: the whole content of the key file (below) | the updater archive and `latest.json` (when the app trusts a real key) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | that key's password | |

The Mac build is signed as the Developer ID named in the workflow
(`SIGNING_IDENTITY` in `release.yml`; decisions.md, entry 48); the workflow stops if the
imported certificate is a different one. To check what a secret is set to, there is no
reading it back: set it again.

## The updater key

The updater key is what makes an update trustworthy: the app installs an update only if its
archive is signed by the private key matching the public key the app was built with. Until
it exists, `tauri.conf.json` carries a placeholder (`REPLACE-WITH-THE-RELEASE-PUBLIC-KEY`):
a build with it never checks for updates, and the release workflow writes no
`latest.json`. The first release that should update itself later must be built with the real
public key, so make the key before the first public release.

Make it once, on a trusted Mac, with the pinned Tauri CLI
(`cargo install tauri-cli --version 2.11.4 --locked`):

```bash
cargo tauri signer generate -w "$HOME/.tauri/method-budget-updater.key"
```

It asks for a password (use a long one). It writes the private key to that file and the
public key to the same name with `.pub`. Then:

1. Store both files' contents and the password in the team 1Password vault, in one item
   (for example `Method Budget - Updater signing key - GitHub Actions`).
2. Set the two secrets from it:
   `gh secret set TAURI_SIGNING_PRIVATE_KEY -R method-production-suite/method-budget < "$HOME/.tauri/method-budget-updater.key"`
   and `gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R method-production-suite/method-budget`.
3. Put the public key (the `.pub` file's single line) into `crates/method-app/tauri.conf.json`
   as `plugins.updater.pubkey`, in place of the placeholder. `cargo xtask updater-key
   --require` should print `trusted <KEY ID>`. Commit that change.
4. Delete the two local files once 1Password has them.

**Method Budget has its own updater key, shared with no other app.** Whoever holds the key
can ship code to every installed copy of the app, and the app checks only that an update was
signed by its key, not which product it came from. A key shared with another app would let
that app's releases (or anyone who stole the key from its build pipeline) install as Method
Budget, would double the places it could leak from, and could never be rotated for one app
without breaking the other.

**Losing the private key** means installed copies can never update again: people would
have to download the next version by hand. Keep it in 1Password. **Replacing the key**
(it leaked, or a scheduled rotation): ship one release signed with the old key that carries
the new public key; releases after it are signed with the new one.

`requireSignedVersion` stays off for now: the pinned Tauri CLI doesn't record the version
inside the signature yet. Turn it on (`plugins.updater.requireSignedVersion: true`) once the
CLI does, so a manifest can't pair a new version number with an older signed archive.

## Hosting

Updates are served from GitHub Releases. The endpoint is written in one place,
`plugins.updater.endpoints` in `crates/method-app/tauri.conf.json`:

```
https://github.com/method-production-suite/method-budget/releases/latest/download/latest.json
```

`releases/latest` is the newest published release that isn't a draft or a prerelease, so
publishing the draft is what makes an update visible. The manifest's download links point
at the same release (`releases/download/v<version>/...`), worked out from the endpoint by
`cargo xtask update-manifest`.

- **While the repository is private**, the endpoint answers "not found" to the app (it
  doesn't sign in), so every check fails quietly. Only Help > Check for Updates says
  "Couldn't check for updates". Updates start working once the repository is public.
- **If the repository moves or is renamed**, change the endpoint (and `[project] repo` in
  `docs/features.toml` for the website). Copies already installed keep asking the old
  address; GitHub redirects a renamed repository, but keep that in mind before deleting or
  reusing the old name.
- **If hosting moves off GitHub**, change the endpoint and pass `--base-url` to
  `cargo xtask update-manifest` (the workflow's `release` job). The app talks only to the
  endpoint's host over https (plus GitHub's two download hosts when the endpoint is on
  github.com), so the archives must be served from that same host.

The website is separate: [WEBSITE.md](WEBSITE.md). The suite website
(`method-production-suite/website`) publishes `site/` from `main`, so a release's "What's
new" box goes live when that repository next syncs and publishes, not when the tag is
pushed.
