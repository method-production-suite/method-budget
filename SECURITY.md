# Security

## Reporting a problem

Please report security problems privately, through GitHub's private vulnerability
reporting on this repository (the **Security** tab, **Report a vulnerability**), not in a
public issue. Say what you found, how to reproduce it, and which version (Help > About, or
`method --version`). Fixes ship in the next release; the latest release is the supported
one.

## What the app does with your data and the network

Method Budget is a local desktop app. Budgets are files on your own disk; there is no
server, no account, no telemetry and no cloud.

**No network unless checking for updates.** The only time the app goes online is to check
for a new version of itself, and to download it when there is one:

- A couple of minutes after the app starts, then every six hours, it fetches a small
  manifest (`latest.json`) from the project's GitHub Releases, and, when a newer version is
  listed, its download. Turn this off with Help > Check for Updates Automatically; Help >
  Check for Updates then checks only when asked.
- The request carries nothing about you or your budgets: it is a plain HTTPS download, with
  the updater's name and version as its user agent.
- Only HTTPS, and only the update endpoint's host (and GitHub's two download hosts), even
  across redirects. The interface itself (the web view) still has no network access at all.
- An update is installed only if its archive is signed by Method Budget's own updater key:
  the app checks the signature against the public key it was built with, before anything
  is kept. A tampered or unsigned download is thrown away.
- Nothing is installed while you work. The app says "Update ready: Restart to update";
  choosing it saves every open budget first and reopens your tabs afterwards.
- A copy built from source without the release key never checks at all.

The `method` command line never uses the network.

## How the app is built to resist hostile files

A budget file is data from anywhere, so the app treats it that way: the interface never
turns budget text into markup, file paths never come from the interface, imported
spreadsheets are read by our own size-limited readers, and the web view has the narrowest
permissions Tauri allows. The full security model: [docs/APP.md](docs/APP.md), "Security
model". How releases are signed and what the update keys are: [docs/RELEASING.md](docs/RELEASING.md).
