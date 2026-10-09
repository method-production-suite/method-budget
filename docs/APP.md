# The desktop app (`crates/fwb-app`)

Film Wizard Budgeting's editor is a local desktop app built with [Tauri 2](https://tauri.app):
the Rust engine runs natively inside the app, and the interface is plain HTML, CSS and
JavaScript drawn in the operating system's own web view. Budgets are `.fwb` files on your
disk ([FORMAT.md](FORMAT.md)). There is no server, no account, and the app never connects
to the network. Why Tauri: [decisions.md](decisions.md), entry 14.

## Run and build

You need Rust (stable) and, for bundles, the Tauri CLI (`cargo install tauri-cli --version 2.11.4 --locked`).
There is no JavaScript toolchain: no npm, no bundler, no packages.

```bash
cargo run -p fwb-app                        # run a debug build
cargo tauri build --debug --no-bundle       # debug binary in target/debug/fwb-app
cargo tauri build                           # release installers (.app/.dmg, .msi, .deb/.AppImage)
cargo test -p fwb-app                       # command-layer tests, no window needed
```

Run the `cargo tauri` commands from `crates/fwb-app` (or pass `--config crates/fwb-app/tauri.conf.json`).
The UI files in `crates/fwb-app/ui/` are embedded into the binary at compile time, so after
editing them, run `cargo run -p fwb-app` again to see the change.

**Linux** needs the WebKitGTK development packages to build:

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
  libxdo-dev libssl-dev build-essential pkg-config libgtk-3-dev
```

macOS and Windows use the system web view (WebKit and WebView2) and need nothing extra.

## Using it

The window has three levels, with a breadcrumb above the grid:

1. **Topsheet**: categories with their totals, production totals (the first one ends
   above-the-line), then the summary rows: above and below-the-line totals, charges, the
   grand total, credits and the net total.
2. **Accounts** in one category, with the category's fringe account when fringes post by
   category.
3. **Detail lines** in one account: description, amount, unit, X, rate, currency, 4X, the
   line total, fringes, fringe dollars, groups, set and location.

Enter or a double-click goes down a level; Escape or the breadcrumb goes back up. On the
detail grid, typing in a cell starts editing it, Enter and Tab commit and move on, Escape
cancels. Formulas can use globals (`SHOOT_DAYS / 5`). A cell that can't be calculated is
kept as typed, outlined in red with the reason on hover, and the banner says what is wrong;
the rest of the budget still totals, counting that cell as blank.

The side panel edits the budget's setup tables: globals (with their current values),
fringes, groups (with the "in budget total" switch), units and currencies. The footer
shows the above-the-line, below-the-line, grand and net totals, and the totals of the
selected detail rows (Shift+arrows or Shift+click to select a range, Cmd/Ctrl+click to add
rows).

| Action | macOS | Windows and Linux |
|---|---|---|
| New, Open | Cmd+N, Cmd+O | Ctrl+N, Ctrl+O |
| Save, Save As | Cmd+S, Shift+Cmd+S | Ctrl+S, Ctrl+Shift+S |
| Undo, Redo | Cmd+Z, Shift+Cmd+Z | Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y |
| Edit cell (keep text) | F2 or double-click | F2 or double-click |
| Same value into every selected row | Cmd+Enter while editing | Ctrl+Enter while editing |
| Insert line below | Cmd+Enter | Ctrl+Enter |
| Insert subtotal / adjustment | Option+Cmd+T / Option+Cmd+A | Ctrl+Alt+T / Ctrl+Alt+A |
| Delete selected rows (or the category or account) | Cmd+Backspace | Ctrl+Backspace |
| Clear a cell | Backspace or Delete | Backspace or Delete |
| Select all rows | Cmd+A | Ctrl+A |

Closing the window or quitting with unsaved changes asks first.

## Architecture

```
crates/fwb-app/
  src/session.rs     one open budget: Budget, file path, dirty flag, undo/redo snapshots
  src/edit.rs        every edit as a function on &mut Budget (validates, then applies)
  src/diagnose.rs    lenient recalc: flags every bad cell instead of stopping at the first
  src/report_dto.rs  serde shapes for fwb_core::Report (the engine stays serde-free)
  src/view.rs        what the UI draws after each command
  src/app.rs         Tauri glue: commands, file dialogs, close and quit guard, menu
  ui/                index.html, styles.css, setup.css, js/*.js (ES modules, no build step)
  capabilities/      what the web view may call
```

**Rust owns the budget.** The app state is one `Session` behind a mutex. The web view never
holds the real budget; it sends a command (`set_detail_field`, `add_detail`, `set_global`,
`undo`, ...) and gets back a complete `View`:

- `budget`: the budget as file-format JSON (the same text a `.fwb` file holds, via
  `fwb_format`), so the UI reads exactly the documented format.
- `report`: every total from `fwb_core::recalc`, with money as integer cents and decimals
  as strings, never floats.
- `error`, `issues`, `partial`: calculation problems (see below).
- `file` (`name`, `path`, `dirty`), `can_undo`, `can_redo`, and `created_row` after an insert.

Every edit runs on a copy of the budget; if it is rejected (an unknown row, a duplicate
account number, a bad global name) nothing changes and the message comes back as the
command's error. An accepted edit pushes the old budget onto the undo stack (200 steps).
The dirty flag compares a revision number with the one last saved, so undoing back to the
saved state clears it.

**Calculation errors never lose an edit.** The engine stops at the first problem, which is
right for `fwb check` but not for an editor. `diagnose::lenient_recalc` runs the engine;
if it fails, it checks every global, fringe rate and detail cell with the engine's own
rules, flags each bad one (`issues`, by row id and field), and recalculates a copy with
those cells blank. The UI shows the engine's message in a banner, outlines the bad cells,
and shows totals marked as partial. The budget keeps what the user typed.

**Selection totals** come from `fwb_core::selection_totals` through the
`selection_totals` command.

**The UI** is a handful of ES modules with no dependencies: `main.js` (state, navigation,
shortcuts), `grid.js` (a generic keyboard grid used at all three levels), `levels.js`
(turns a View into rows and columns), `setup.js` (the side panel), `footer.js`,
`modal.js`, `format.js` (money and decimal display), `api.js` (the command queue) and
`dom.js`. Commands run one at a time, in order.

## Security model

The app opens files the user picks and nothing else, and the web view is treated as
untrusted (a budget file is data from anywhere).

- **No network.** No updater, no telemetry, no remote content. The CSP limits every
  resource to the app's own bundled files and Tauri's IPC:
  `default-src 'self'; script-src 'self'; style-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`.
  No inline scripts or styles, no CDN.
- **Navigation is locked** to the bundled UI (`tauri://localhost`, or `http://tauri.localhost`
  on Windows) by a small navigation guard, so a link or dropped URL can't load another page.
- **Least privilege for the web view.** `capabilities/main.json` grants the main window
  only the app's own budget commands. `build.rs` lists those commands in an app manifest,
  which turns on Tauri's access control for them: anything not listed is refused. The web
  view gets no file system, shell, HTTP or dialog permissions; there is no fs or shell
  plugin registered. (`tauri-plugin-dialog` depends on the fs plugin crate as a library,
  but it is never registered, so none of its commands exist.)
- **Paths never come from the web view.** Open and Save show the native file dialog from
  Rust and read or write the chosen file there. JavaScript can ask to open or save, but it
  can't name a path. Saves write a temporary file next to the target and rename it into
  place, so a failed save never leaves half a file.
- **Budget text is never markup.** The UI builds the page with DOM calls and puts every
  string from a budget in with `textContent` or input values, never `innerHTML`.
  `freezePrototype` is on.
- **Hostile files** can't crash the engine: formulas have length and nesting limits and
  globals are evaluated without recursion (see the engine's hardening tests). A file that
  can't be read shows the reader's message with its line, column and JSON path.

## Dependencies

Tauri crates are pinned to exact versions and, like every crate in `Cargo.lock`, must be at
least 14 days old when added (`python3 scripts/check-crate-ages.py --only-changed HEAD`).
`tauri` uses `default-features = false` with only `wry`, `common-controls-v6`, `x11` and
`dbus` (no asset compression, no runtime ACL changes).

## Not yet

Copy and paste of rows and cells, moving rows, fringe ranges, sets and locations tables,
charges and credits editors, renaming setup entries (add the new one, then delete the old),
budget preferences (account coding, fringe posting), import from other tools, reports and
printing, and opening a `.fwb` file by double-clicking it in the file manager.
