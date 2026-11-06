# The Linux AppImage opening to an empty window

Reported on a current Arch-based system against Budget and Schedule: the AppImage prints an EGL
error such as `Could not create surfaceless EGL display: EGL_BAD_ALLOC. Aborting...`.

## The cause

Budget's AppImages up to 0.8.1 carry `libwayland-client.so.0` from the Ubuntu 22.04 machine that
built them, and the launcher puts the AppImage's libraries first. The computer's own Mesa then
loads that old copy, which lacks symbols current Mesa needs. WebKit can't make an EGL display and
its web process aborts. The main program stays up, so the person sees the menu bar over an empty
window.

The Tauri bundler stopped including the library when it updated linuxdeploy (bundler 2.10.0, which
arrived with CLI 2.12.0). Budget pinned CLI 2.11.4; it now pins 2.12.1, as Schedule does.

## Checked (2026-11-06)

An Arch Linux container (x86_64 under emulation; Mesa 26.2.6, wayland 1.26.0, software rendering,
X11 under Xvfb), running the AppImage's contents through its own `AppRun` for 40 seconds, with a
screenshot of each run:

| AppImage | `libwayland-client` inside | Result |
|---|---|---|
| 0.8.1 | yes | `EGL_BAD_PARAMETER`, web process gone, menu bar over an empty window |
| 0.8.1 with `LD_PRELOAD` of the system's copy | yes, overridden | No error, the whole app draws |

Not checked on real hardware, where the error differs, or by running the `.AppImage` file itself
under FUSE.

## Keeping it fixed

`release.yml` fails if `libwayland-client` is inside the AppImage.

## For anyone on 0.8.1 or older

Install the `.deb`, or use the computer's own library:

    LD_PRELOAD=/usr/lib/libwayland-client.so.0 ./Method-Budget-linux-x86_64.AppImage

The path is `/usr/lib64/` on some distributions and `/usr/lib/x86_64-linux-gnu/` on Debian-based
ones.
