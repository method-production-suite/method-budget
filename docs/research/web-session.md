# Method Budget in a browser: the window-free session

Status: phase 1 built (decisions.md, entry 70). This note is the plan and the contract a
partner's web app, and later our own website demo, integrate against.

## The goal

The engine and the logic behind every screen run in a browser as WebAssembly, so a
partner's web app can host the existing page (`crates/method-app/ui/`) unchanged, and so can
a demo on our website. The owner decided we do the split ourselves. The desktop app keeps
working exactly as before: same commands, same JSON, same files, same tests.

## The crate map

```
method-core      the engine (no dependencies)                         builds for wasm32
method-format    .mthd and imports/exports on bytes (serde, serde_json) builds for wasm32
method-session   everything about open budgets that needs no window    builds for wasm32
method-app       the desktop: Tauri, dialogs, files, windows, menus     native only
method-web       (phase 2) wasm-bindgen exports + a JS invoke shim       wasm32 only
```

### What moved to method-session (phase 1)

| Module | What it is | Notes |
|---|---|---|
| `session` | one open budget: undo/redo patches, labels and times, the recalc cache, focus, the view | file I/O split off (below) |
| `docs` | every open budget as tabs, split view's panes, `AppState`, `DocGuard` | `find_file` takes the "same file" test from the host |
| `view` | `View` and `FullView`, what the page draws | unchanged |
| `edit`, `batch` | every edit as a function on `&mut Budget`; multi-cell edits | unchanged |
| `diagnose`, `report_dto` | lenient recalc; the report as the page reads it | unchanged |
| `history` | naming undo steps for the History list | saving a step as a version stays (needs `versions`) |
| `display` | global display info, the numbers-on-screen settings, cell values | the two Tauri commands stay as one-liners |
| `actuals` | the cost figures every view of a budget with actuals carries, and the rollup cache | importing and listing actuals stay (1b) |
| `safety` | the autosave rule, recovery key names, the file stamp as data | writing files, recovery and backups stay |
| `clock` | the injected clock (below) | new |
| `collab` | `SharedLog`: what a session asks of a shared budget's change log | new; the folder's `SharedDoc` implements it |
| `commands` | the typed function behind each budget command, with its undo label | new; the Tauri commands call these |
| `host` | `Host::dispatch`, `COMMANDS`, `COMMANDS_VERSION` | new |

method-app re-exports the moved modules under their old names (`crate::edit`, `crate::view`,
`crate::docs`, `crate::batch`, `crate::diagnose`, `crate::report_dto`), and its own
`session`, `history`, `display`, `actuals` and `safety` modules re-export the moved parts
beside what stays, so the rest of the app reads as before.

### What stays in method-app, and why

Desktop only, for good:

- `app.rs` (Tauri glue, the close and quit guard, dialogs), `menu.rs`, `updater.rs`,
  `tools.rs` and `setupwin.rs` (pop-out windows), `tabs.rs`'s Window menu and `tabs.json`.
- `safety.rs` and `safety_app.rs`: atomic writes, the autosave timer and its files, recovery
  copies, backups, the recent list, settings. `bookmark.rs` (macOS bookmarks), `pdf_mac.rs`.
- The file side of a session, in `src/session.rs`: `SessionFiles` (open, save, a file changed
  outside, Load theirs, starting or joining folder sharing, the sharing timer) and
  `DocsFiles` (a tab by its file). Bring the trait into scope and the calls read as before.
- `shared.rs` and `shared_app.rs`: sharing through a folder. The browser build has none of it
  (it lives only in method-app), as the partner asked: their server does sharing.
- The start window's file listing (`start.rs`), the setup library's folder (`library.rs`), the
  Packs folder (`packs.rs`).

Phase 1b, logic that belongs in method-session but still shares a file with its Tauri
commands. Each is the same mechanical split as `display.rs`: the logic and its tests move,
the command becomes one call, and a dispatch arm and a `COMMANDS` entry are added. None was
moved in phase 1 only to keep each step reviewable (about 23,000 lines in all):

| Module | What has to change first |
|---|---|
| `apply`, `quickadd`, `notes`, `find`, `moverows`, `rename`, `renumber`, `rowstyle`, `setup_lists`, `presets`, `explain`, `overtime`, `versions`, `history` (jump and save as version) | nothing: plain edits on a session behind `edit_budget` and `finish` |
| `tabs` (new tab, switch, close, move, split view) | the Window menu and `tabs.json` stay; the rest is `Docs` already |
| `clipboard` (rows with their setup) | it is the app's own row clipboard, no OS clipboard: moves as is |
| `compare`, `diffmerge` | budgets picked from files come in as text with a display name and a token |
| `templates`, `coa_import`, `import_setup` | the bundled charts are in `method_format::charts_bundled`; picked files come in as bytes |
| `sheet_import`, `export_import`, `open_any`, `setup_sheets`, `actuals` (import) | already read bytes (`method_format::import`); the token store moves, the Open dialog stays |
| `export`, `reports`, `sharing` (copies for sharing), `packs` (export) | return bytes and a suggested name; the Save dialog stays; printing stays |
| `library` | the review and the copying move; the library's own file stays |

## The command contract

One documented entry point:

```rust
let host = method_session::Host::new();               // one untitled starter budget
let reply: serde_json::Value = host.dispatch("set_title", json!({"title": "Feature"}))?;
```

- **Names and shapes are the desktop's.** `dispatch(command, args)` takes the command's name
  and the arguments object exactly as the page's `invoke` sends it to Tauri (`api.js`), and
  gives back the JSON the desktop command returns: for an edit, the `View` (docs/APP.md, "What
  crosses the bridge"). Arguments are camelCase as Tauri reads them (`rowId`, `accountCode`,
  `clearLines`); `set_account_coding` alone takes snake_case (`category_width`), as on the
  desktop. Unknown arguments are ignored (the page adds `doc` to every call).
- **`doc`** names the open budget a command acts on; missing or `null` means the one in front.
  A closed budget is refused with "That budget has been closed.", nothing changed.
- **Errors** (`method_session::Error`): `UnknownCommand`, `BadArguments` (the JSON didn't have
  the command's shape) and `Refused` (the command's own message, which the page shows; the
  budget is unchanged). `Error::message()` is the text a desktop command would reject with.
- **Typed functions behind it**: `method_session::commands::*` take a `&mut Session` and typed
  arguments. The desktop's Tauri commands are one call into these (`app.rs`, `display.rs`),
  so both hosts name undo steps and validate alike.
- **Budgets in and out as `.mthd` text**, the host's own two commands (no desktop command has
  these names): `open_mthd {name, text}` opens a budget in a new tab in front (an untouched
  Untitled gives its tab up) and returns its view; `save_mthd {doc, name?}` returns `{text,
  view}` and marks the budget saved. The partner keeps `.mthd` files in their own storage;
  where a budget lives is the host's business. Typed: `Host::open_mthd`, `Host::save_mthd`.
- **Versioning**: `COMMANDS_VERSION` (now 1) goes up when a command in `COMMANDS` is removed
  or renamed, or its arguments or reply change in a way an existing caller would notice.
  Adding a command or an optional argument doesn't change it. `COMMANDS` lists every command
  `dispatch` runs; a test checks each is dispatched, and another (in method-app) that each is
  a desktop command of the same name.

Commands in version 1:

| Group | Commands |
|---|---|
| Views | `get_view`, `get_full_view`, `focus_account`, `selection_totals`, `preview_charge` |
| History | `undo`, `redo`, `history_list`, `history_jump` |
| Structure | `set_title`, `add_category`, `add_production_total`, `update_topsheet_row`, `delete_topsheet_row`, `add_account`, `update_account`, `delete_account` |
| Detail rows | `set_detail_field`, `set_rows_field`, `add_detail`, `delete_details`, `set_cells` |
| Setup tables | `set_global`, `delete_global`, `set_global_info`, `set_display_prefs`, `set_fringe`, `delete_fringe`, `set_group`, `set_group_rule`, `delete_group`, `set_unit`, `delete_unit`, `set_currency`, `delete_currency` |
| Settings | `set_arithmetic`, `set_round_account_totals`, `set_cutoff_factor`, `set_fringe_row`, `set_trailing_level_fringes`, `set_fringe_posting`, `set_account_coding`, `set_totals_currency` |
| Topsheet setup | `set_charge`, `delete_charge`, `move_charge`, `set_credit`, `delete_credit`, `move_credit`, `set_set`, `delete_set`, `set_location`, `delete_location`, `set_info`, `delete_info`, `lock_baseline`, `set_baseline_frozen`, `clear_baseline` |
| The host's own | `open_mthd`, `save_mthd` |

The arguments of each are the desktop command's (docs/APP.md and the `#[tauri::command]`
functions in method-app). Every phase 1b module adds its commands under their desktop names.

## The seams

**The clock** (`method_session::clock`). A session needs the time for three things: when
each undo step was made (the History list), quick repeats joining one undo step and
autosave's quiet period (both on a clock that never goes back), and recovery key names
(unique across running copies). The host installs a `Clock` (`wall_ms`, `mono_ms`,
`unique`) once with `clock::install`, before the first session. The desktop turns on the
`std-clock` feature, which makes the system clock the default, the same calls as before.
Without it the clock reads 0 (steps undated, quick repeats always join) until the host
installs one; a browser installs `Date.now()`, `performance.now()` and a counter.

**Files.** method-session never opens a file. The desktop shows the dialog, reads or writes
the file, and hands the session what it needs: `Session::mark_saved(path, stamp)` after a
save, `Session::adopt_file_version(budget, stamp)` for Load theirs, `set_disk_stamp` after
reading. A path here is only a name (the title bar and tab label use its file name), so a
browser passes the name it shows. The page still never sees a path on the desktop: the token
design is untouched, since the dialogs and tokens all stay in method-app.

**Sharing.** A session records edits through `collab::SharedLog`, takes this person's undo
and redo from it and lists its steps; folder sharing (`SharedDoc`) implements it in
method-app. The change log's types and logic (`method_core::log`: `Log`, `Change`,
`Projection`, recording, undo) and its line and segment codecs (`method_format::log`'s
`change_to_json`, `change_from_json`, `segment_bytes`, `read_segment`) are already free of
file I/O and build for wasm32, so a server could carry our log later by implementing
`SharedLog` over its own transport. Two things to settle then: `method_format::log`'s folder
code (`LogDir`, `init`, `open`, presence) would go behind a feature, and `Ids::from_os` reads
the process id, which panics in a browser, so a browser seeds `Ids::seeded` from
`crypto.getRandomValues`.

## WebAssembly constraints found

Checked with `cargo xtask wasm-check` (`cargo check --locked -p method-core -p method-format
-p method-session --target wasm32-unknown-unknown`, no features), run in CI.

- **Dependencies**: method-session needs only method-core (no dependencies), method-format,
  serde and serde_json. There is no zip, xlsx, PDF, time, rand, uuid or getrandom crate in
  its tree: method-format reads and writes workbooks with its own ZIP, DEFLATE and XML code,
  and the engine's ids come from its own xoshiro generator. So no `getrandom` `js` feature
  or similar is needed. Phase 2 needs `wasm-bindgen` and `js-sys`; both are already in
  Cargo.lock through Tauri (0.2.128 and 0.3.105), so no newer version has to be taken.
- **Clocks**: `std::time::Instant::now` and `SystemTime::now` panic on
  wasm32-unknown-unknown. Hence the injected clock; nothing in method-session calls them
  without the `std-clock` feature.
- **Files**: `std::fs` compiles on wasm32-unknown-unknown and fails at run time. Nothing in
  method-session calls it. method-format still has file code (the change log's folder, test
  helpers) that compiles but is never reached from a session.
- **Threads**: none. `Mutex`, `OnceLock` and the atomics compile and work single-threaded.
- **Process ids**: `std::process::id` panics; only the `std-clock` clock and
  `Ids::from_os` (sharing, desktop only) use it.
- **Printing** is the desktop's (AppKit on macOS); a browser prints the report page itself.

## The phases

1. **Phase 1 (done)**: method-session with the modules above and their tests, the injected
   clock, `dispatch` for the commands listed, the desktop's commands calling into it, the
   wasm check in CI.
2. **Phase 1b**: the modules in the table above, one or a few at a time, each adding its
   commands to `COMMANDS` (no version change: additions).
3. **Phase 2, `method-web`**: a crate with `crate-type = ["cdylib"]` and `wasm-bindgen`
   exports: `new Host()`, `host.dispatch(command, argsJson) -> Promise<replyJson>`,
   `host.openMthd(name, text)`, `host.saveMthd(doc, name)`, `installClock()` (wiring
   `Date.now`, `performance.now` and a counter). A small JS shim defines
   `window.__TAURI__.core.invoke` over `dispatch` (rejecting with `Error::message()`) and a
   minimal `__TAURI__.event` (the page listens for `view-changed`, `autosaved` and the like;
   in a browser most never fire), so `ui/` runs unchanged. Desktop-only items (File > Open,
   Save, Print to PDF, the updater, pop-out windows) are hidden by the page when the shim
   says it is a browser host.
4. **Phase 3, the website demo**: the page plus `method-web` as static files, a sample
   budget, nothing kept beyond the tab (or local storage).
5. **CI**: `cargo xtask wasm-check` now; phase 2 adds `wasm-pack build` (or `cargo build
   --target wasm32-unknown-unknown` with `wasm-bindgen-cli`) and a headless smoke test of the
   shim.

## What a partner integrates against

- `method_session::Host` (or, in a browser, phase 2's `method-web` exports) with `dispatch`,
  the `COMMANDS` list and `COMMANDS_VERSION`; this note's command table; docs/APP.md for each
  command's reply (the `View`).
- `.mthd` text in and out (`open_mthd`, `save_mthd`); docs/FORMAT.md for the file. Schedules
  and anything else they store are theirs.
- `method_session::clock::install` before the first budget, in a browser.
- Sharing is their server's (one editor at a time, others read-only with updates); nothing
  of our folder sharing is in the browser build. A viewer can open the latest `.mthd` with
  `open_mthd` again as it changes. Open question: a host-level read-only switch (every edit
  refused, as for a copy saved for sharing) so the viewer's page can't change its copy; it
  would be one flag on `Host` checked by `Session::check_writable`.
- Clean room both ways: the partner sends no code to this repository, and nothing derived
  from another program's files or code enters it (CLEAN-ROOM.md).
