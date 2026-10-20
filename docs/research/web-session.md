# Method Budget in a browser: the window-free session

Status: phases 1, 1b, 2 and 3 built (decisions.md, entries 70 to 73): the page runs in a
browser tab against method-session compiled to WebAssembly, the charts of accounts load
only when picked, and the website has a demo at `/try/` (docs/WEBSITE.md, "The demo").
This note is the plan and the contract a partner's web app and our own demo integrate
against; "Running it in a browser" below is how to build, serve and embed it.

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
method-web       wasm-bindgen exports + the page's invoke shim            wasm32 only
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

### What moved in phase 1b

Every command module's logic and its tests, module by module (decisions.md, entry 71). Each
module in method-session has a "Commands" section of typed functions (one per desktop
command, naming the undo step as the desktop does); its desktop file in method-app keeps a
re-export (`pub use method_session::<module>::*`), the Tauri commands as one call each, the
dialogs and the files; and `src/host/<module>.rs` dispatches its commands by name.

| Module | What moved | What stays on the desktop |
|---|---|---|
| `versions`, `history` (save a step as a version) | all | nothing |
| `notes`, `rowstyle`, `presets`, `apply`, `rename` | all | nothing |
| `quickadd`, `find`, `moverows`, `renumber`, `setup_lists`, `explain`, `overtime` | all | nothing |
| `clipboard` | all: it is the app's own row clipboard (the page puts text on the system clipboard itself); the host keeps its own `ClipboardState` | nothing |
| `tabs` | new tab, switch, move, close (the closing rules), the list, split view and its panes | the Window menu, `tabs.json`, autosave and recovery copies on close |
| `compare`, `diffmerge` | the comparison, What Changed, the merge; picked budgets kept by token | the Open dialog, matching a picked file to an open tab, reading the saved file for What Changed |
| `export`, `reports` | the workbook and its files as bytes; report presets | the Save dialog and atomic writes; printing (`print_report`, `pdf_mac.rs`) |
| `templates`, `import_setup`, `coa_import`, `sheet_import`, `export_import`, `open_any`, `setup_sheets`, `actuals` | the reviews, previews and imports on bytes, the bundled charts, every token store (`ImportSources`, `SheetSources`, `CoaSources`, `ExportSources`, `ActualsSources`) | the Open dialog, reading files (and a file open in a tab), Open Recent, a file handed over by the system |
| `sharing` | the copy for sharing, the editable copy, storing a returned copy, sub-budgets | the Save dialog and the write |
| `packs` | the review, importing into a budget, the items a pack can be made from, building a pack | the Packs list and folder, the library's side |
| `library` | the library's view, the review both ways, copying in and out, folder changes | which file is the library (`library.json`), reading and writing it, refusing changes while it is open as a tab |

No module was too tangled to move. No crate had to be gated for wasm32: every import and
export reads and writes bytes through method-format's own code.

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
- The start window (`start.rs`: the folder listing, pins, New Budget into a folder), the
  setup library's file (`library.rs`), the Packs folder (`packs.rs`).
- In every moved module: the Open and Save dialogs, reading and writing files, and the
  wiring tests that read `app.rs`, `build.rs` and the capabilities.

Desktop commands with no host form (55 of the desktop's 226): the windows
(`tools_window`, `close_tools_window`, `setup_window`, `close_setup_window`), the start window
(`start_*`, `new_budget_*`), saving and the files (`save_budget`, `open_recent`,
`open_pending`, `resolve_external_change`), autosave, recovery and backups
(`autosave_setting`, `set_autosave`, `flush_autosave`, `recovery_list`, `open_recovered`,
`discard_recovered`, `backup_list`, `restore_backup`, `reveal_backups`), the updater
(`update_status`, `update_restart`, `whats_new`, `release_notes`), printing
(`print_report`), folder sharing (`share_*`), the library's file (`library_*`) and the Packs
folder (`packs_list`, `packs_reveal`, `pack_review`, `pack_import_library`). A browser host
saves with `save_mthd`, starts a budget with `new_tab` or `chart_template_new`, and keeps
budgets, a library and packs in its own storage.

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
- **Typed functions behind it**: `method_session::commands::*` (phase 1) and each moved
  module's "Commands" section (phase 1b) take a `&mut Session` (or the tabs) and typed
  arguments. The desktop's Tauri commands are one call into these, so both hosts name undo
  steps and validate alike.
- **Files in.** Where the desktop shows an Open dialog (`open_budget`, `template_new`,
  `import_setup_pick`, `sheet_import_pick`, `coa_import_pick`, `export_import_pick`,
  `setup_sheet_pick`, `actuals_pick`, `merge_pick_file`, `pack_import_pick`,
  `schedule_days_pick`), the host's
  command of the same name takes the picked file as `file: {name, bytes}` beside the
  desktop's arguments, and replies as the desktop does (a review with its token, so the
  page's next calls work unchanged). `compare_pick_files` takes `files: [{name, bytes}]`, as
  its dialog picks several. The name is only shown; no path ever crosses.
- **Files out.** Where the desktop shows a Save dialog and writes (`template_save`,
  `share_copy_save`, `export_budget`, `pack_export`, `setup_sheet_export`), the host's
  command replies with the desktop's reply as after a save (a file's `path` is its name) and,
  beside it, `downloads: [{name, bytes}]` for the host to offer (`Downloads`).
- **Bytes are base64.** In both directions a file's `bytes` is one standard, padded base64
  string (`btoa`/`atob` or `Uint8Array.fromBase64` in a browser; `method_session::host::
  b64_encode` and `b64_decode` in Rust). Decided before phase 2 so the shim never sends
  number arrays.
- **What the host is**: `host_info` returns `{kind, commands_version, read_only}`: `"web"`
  from `Host`, `"desktop"` from the desktop's command of the same name. The page asks it at
  start (`ui/js/host.js`) and, on `"web"`, hides what only the desktop does (below).
- **Read-only**: `Host::set_read_only(true)` shows every budget read-only, those open and
  any opened later (a partner's viewer). Every edit is refused through
  `Session::check_writable` with "This budget is open read-only here, so it can't be
  changed.", and undo, redo and history jumps do nothing; views, `save_mthd` and the review
  commands still work. `host_info` reports it.
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

Commands in version 1 (187: 60 from phase 1, 113 from phase 1b, 14 added since):

| Group | Commands |
|---|---|
| Views | `get_view`, `get_full_view`, `focus_account`, `selection_totals`, `preview_charge`, `incentives` |
| History | `undo`, `redo`, `history_list`, `history_jump`, `history_save_version` |
| Structure | `set_title`, `add_category`, `add_production_total`, `update_topsheet_row`, `delete_topsheet_row`, `add_account`, `update_account`, `delete_account` |
| Detail rows | `set_detail_field`, `set_rows_field`, `add_detail`, `delete_details`, `set_cells` |
| Setup tables | `set_global`, `delete_global`, `set_global_info`, `set_display_prefs`, `set_fringe`, `delete_fringe`, `set_group`, `set_group_rule`, `delete_group`, `set_unit`, `delete_unit`, `set_currency`, `delete_currency` |
| Settings | `set_arithmetic`, `set_round_account_totals`, `set_cutoff_factor`, `set_fringe_row`, `set_trailing_level_fringes`, `set_fringe_posting`, `set_account_coding`, `set_totals_currency` |
| Topsheet setup | `set_charge`, `delete_charge`, `move_charge`, `set_credit`, `delete_credit`, `move_credit`, `set_set`, `delete_set`, `set_location`, `delete_location`, `set_info`, `delete_info`, `lock_baseline`, `set_baseline_frozen`, `clear_baseline` |
| Versions | `save_version`, `restore_version`, `rename_version`, `delete_version` |
| Tabs and split view | `new_tab`, `switch_tab`, `move_tab`, `close_tab`, `list_tabs`, `split_view`, `close_split`, `set_target_pane`, `move_tab_in_pane`, `move_to_pane`, `pane_view` |
| Rows | `set_cell_creating`, `set_new_line_defaults`, `move_rows`, `move_accounts`, `move_topsheet_rows`, `set_row_style`, `set_group_color`, `update_group` |
| Row clipboard | `clip_copy`, `clip_cut`, `clip_preview`, `clip_paste`, `clip_forget` |
| Apply panel | `apply_summary`, `apply_to_rows`, `assign_to_rows`, `clear_from_rows`, `replace_tags_on_rows`, `make_fringe_range`, `clear_fringe_range` |
| Presets | `save_tag_preset`, `tag_preset_from_line`, `apply_tag_preset`, `save_line_preset`, `line_preset_preview`, `insert_line_preset`, `rename_tool_preset`, `delete_tool_preset`, `move_tool_preset` |
| Notes | `add_note`, `update_note`, `delete_note`, `set_note_done`, `notes_list` |
| Find, rename, renumber | `find`, `replace`, `rename_setup_item`, `renumber_preview`, `renumber_accounts` |
| Setup lists | `setup_usage`, `set_setup_order`, `move_setup_item`, `set_short_code` |
| Explain, overtime | `explain_number`, `ot_calculate`, `ot_apply`, `ot_save_pattern`, `ot_delete_pattern` |
| Compare, What Changed, merge | `compare_budgets`, `compare_detail`, `compare_pick_files`, `what_changed`, `merge_preview`, `merge_apply`, `merge_pick_file` (What Changed against the saved file is refused: the host keeps no file; pick it as a file) |
| Export, reports | `export_budget`, `save_report_preset`, `delete_report_preset`, `bid_sheet` |
| Templates, charts | `template_new`, `template_save`, `chart_templates`, `chart_template_new` |
| Imports | `open_budget`, `import_setup_pick`, `import_setup`, `sheet_import_pick`, `sheet_import_preview`, `sheet_import`, `coa_import_pick`, `coa_import_preview`, `coa_import`, `export_import_pick`, `export_import_preview`, `export_import`, `setup_sheet_export`, `setup_sheet_pick`, `setup_sheet_guess`, `setup_sheet_review`, `schedule_days_pick`, `schedule_days_review`, `schedule_days_apply` (decisions.md, entry 76) |
| Review files | `review_start`, `review_open`, `review_close`, `review_list`, `review_add`, `review_update`, `review_resolve`, `review_delete`, `review_restore`, `review_settle`, `review_save` (decisions.md, entry 81; `review_start` and `review_open` take the review file as `file`, `review_save` gives it back as a download) |
| Actuals | `actuals_pick`, `actuals_preview`, `actuals_import`, `actuals_list`, `actuals_update`, `actuals_delete`, `actuals_remove_batch`, `actuals_set_etc`, `actuals_lines` |
| Sharing copies | `share_copy_save`, `share_copy_editable`, `share_copy_original`, `share_copy_store`, `sub_budget_new` |
| Setup packs | `pack_items`, `pack_export`, `pack_import_pick`, `pack_import` (into and out of a budget; `source` or `into` `library` is refused) |
| The host's own | `open_mthd`, `save_mthd` |
| The host | `host_info` (the desktop has it too) |

The arguments of each are the desktop command's (docs/APP.md and the `#[tauri::command]`
functions in method-app), with the file forms above. The host's `close_tab` keeps no
autosave or recovery copy (the host saves with `save_mthd`).

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
and redo from it, lists its steps and asks it whether this person may change anything at
all (`refusal`, decisions.md entry 78; roles themselves live in `method_core::log::access`,
so a server carrying the log gets them too); folder sharing (`SharedDoc`) implements it in
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
2. **Phase 1b (done)**: every command module above, each adding its commands to `COMMANDS`
   (no version change: additions), files in and out as base64, `host_info`, and the
   read-only switch.
3. **Phase 2, `method-web` (done)**: the crate, the shim and `cargo xtask web`; the page
   runs in a browser with a few changes guarded by `host_info` (below, "Running it in a
   browser"). Differences from this plan: `dispatch` is synchronous (it returns the reply's
   JSON; the shim's `invoke` is the promise), the clock is installed by the first `new
   Host()` rather than a separate call, and `saveMthd` names a `.mthd` file itself.
4. **Phase 3, the website demo (done)**: the charts of accounts out of the module (below,
   "The charts in a browser"), and `cargo xtask web --demo`: the page plus `method-web` as
   static files under `site/try/`, an invented sample budget, a note about running in the
   browser, a strict Content-Security-Policy; nothing kept beyond the tab (docs/WEBSITE.md,
   "The demo"; decisions.md, entry 73).
5. **CI (done)**: `cargo xtask wasm-check` checks the session crates for wasm32 and builds
   method-web for it; then `cargo xtask web` and `node scripts/web-smoke.mjs` (below,
   "The smoke test"), with the wasm-bindgen command line cached. The Pages workflow builds
   the demo the same way and runs the same test before it deploys.

## Running it in a browser (phase 2)

### Building

```
rustup target add wasm32-unknown-unknown                    # once
cargo install wasm-bindgen-cli --version 0.2.128 --locked    # once: Cargo.lock's wasm-bindgen
cargo xtask web                                              # builds target/web/
python3 -m http.server -d target/web 8000                    # then open http://localhost:8000/
```

`cargo xtask web` (crates/xtask/src/web.rs) builds `method-web` for wasm32-unknown-unknown
with the `web-release` profile (the root Cargo.toml: `opt-level = "s"`, LTO, one codegen
unit; the desktop's profiles are untouched), runs the `wasm-bindgen` command line
(`--target web`, no TypeScript, name and producers sections removed; it refuses a command
line whose version isn't Cargo.lock's), and fills `target/web/` (`--out <dir>` for another
folder) with:

```
index.html, *.css, js/          the page, copied from crates/method-app/ui; index.html loads
                                method-web/tauri-shim.js before js/boot.js (the only change)
method-web/tauri-shim.js        the page's window.__TAURI__ over the Host
method-web/tauri-shim.css       the Budget Setup frame's styles (a file, for a strict CSP)
method-web/method_web.js        wasm-bindgen's JS, an ES module
method-web/method_web_bg.wasm   about 5.2 MB, 1.4 MB gzipped (11.5 MB and 2 MB with the
                                charts compiled in)
charts/index.json               the bundled charts of accounts: each one's id, name, group,
charts/<id>.mthd                description, categories and accounts; 92 files, 6.3 MB,
                                each fetched only when it is needed
```

`--demo` adds the website's demo (docs/WEBSITE.md, "The demo"): `demo/`, `sample/` and a
Content-Security-Policy in `index.html`.

No npm and no bundler. `cargo build -p method-web --target wasm32-unknown-unknown --release`
builds it too (larger: about 20 MB before wasm-bindgen). On the desktop the crate is a name
helper only, so `cargo build` and `cargo test --workspace` are as before.

The bundled charts were in the module twice: `charts_bundled.rs` declared them as a
`const`, which copies its strings at each use; it is a `static` now (and
`scripts/build-coa-templates.py` writes it so), which took 6 MB off the module and nothing
off the desktop's behaviour.

### The charts in a browser

The bundled chart-of-accounts templates (`templates/charts/`, 6.2 MB of text) are compiled
in only with method-format's `bundled-charts` feature. It is on by default, and
method-session's feature of the same name (also on by default) turns it on, so the desktop
and the command line are unchanged. method-web depends on both with default features off,
so the module has none (`method_format::charts::all()` is empty there).

Instead `cargo xtask web` writes them beside the page, from the same list the desktop
compiles in: `charts/index.json` and `charts/<id>.mthd`. The host supplies them to the
session itself (`method_session::coa_import::HostCharts`, kept by the Host):

- `Host::set_charts(list)` (JS `host.setCharts(json)`): the charts `chart_templates` lists
  after any bundled ones. The shim fetches `charts/index.json` the first time the page asks
  for `chart_templates` (New From Template) and hands it over; the list's extra fields
  (`categories`, `accounts`) are ignored by the Host and there for a fuller list later.
- `Host::add_chart(id, text)` (JS `host.addChart(id, text)`): one chart's text, refused if
  it isn't a budget file. The shim fetches `charts/<id>.mthd` when `chart_template_new` is
  called for a chart in the list, then dispatches it.
- `chart_template_new` takes a bundled chart first, then the host's; a chart the host lists
  but hasn't handed over is refused with "<name> hasn't loaded yet. Check the connection
  and pick it again." A failed fetch is refused with the shim's own message.

No command's shape changed (`COMMANDS_VERSION` stays 1): `chart_templates` replies with
the same `{id, name, group, description}` objects and `chart_template_new` takes the same
`{id}`. Nothing else in a browser needs the charts: the start window's New Budget (which
lists them with their sizes) is the desktop's.

### What a partner serves

Every file in `target/web/`, as static files from one origin, with `.wasm` served as
`application/wasm` (any current web server does), `charts/` included (without it New From
Template lists no charts). The page needs nothing else: no server calls beyond fetching
those files, no cookies; budgets live in the page until the person saves (downloads) them. Its
address can say:

- `?open=<url>`: start with the `.mthd` file at that address (same origin, or one that
  allows it) instead of a new starter budget. The name shown is the URL's last part.
- `?readonly=1`: every budget read-only (`Host.setReadOnly(true)`).

To keep budgets in the partner's own storage, call the Host from the page
(`window.__METHOD_WEB__.ready` is a promise of the `Host`) or build their own page around
the JS API below.

### The JS API (crates/method-web/src/web.rs)

```js
import init, { Host, commands } from './method-web/method_web.js';
await init({ module_or_path: './method-web/method_web_bg.wasm' });
const host = new Host();                        // one untitled starter budget
const reply = JSON.parse(host.dispatch('set_title', JSON.stringify({ title: 'Pilot' })));
host.openMthd('Pilot.mthd', text);              // a budget in a new tab in front: its view (JSON)
const { name, text: saved } = host.saveMthd();  // the budget in front (or saveMthd(doc, name))
host.setReadOnly(true);                         // a viewer: every edit refused
JSON.parse(host.hostInfo());                    // {kind: "web", commands_version: 1, read_only}
commands();                                     // every command dispatch runs (COMMANDS)
host.setCharts(indexJson);                      // the chart list (charts/index.json)
host.addChart('feature', text);                 // one chart's text, once it is picked
```

- `dispatch(command, argsJson)` is `Host::dispatch`: the desktop's command names and
  arguments, its reply as JSON. A refusal throws an `Error` whose `message` is the text the
  page shows (`Error::message()`); nothing changed.
- `saveMthd(doc?, name?)` returns `{name, text}` and marks the budget saved; `name` is a
  `.mthd` file name (an opened export or a `.json` budget saves as `.mthd`).
- The first `new Host()` installs the browser's clock: `Date.now()` for the time of each
  undo step, held so it never goes back for joining quick repeats, and unique names seeded
  from `crypto.getRandomValues`. Nothing else in the session needs randomness.
- A panic in the engine stops the module (WebAssembly traps); the shim then says to reload
  the page. None is known.

### The shim (crates/method-web/js/tauri-shim.js)

A classic script, loaded before the page's modules, that defines the part of Tauri the page
uses:

- **`core.invoke(cmd, args)`** runs `dispatch` and resolves with the reply, or rejects with
  the message, as Tauri does. Where the desktop shows an Open dialog (`open_budget`,
  `template_new`, `import_setup_pick`, `sheet_import_pick`, `coa_import_pick`,
  `export_import_pick`, `setup_sheet_pick`, `actuals_pick`, `merge_pick_file`,
  `pack_import_pick`, `schedule_days_pick`, `compare_pick_files`) it shows the browser's file picker with the
  same file types and adds `file` (or `files`) as `{name, bytes}` in base64; cancelling
  resolves `null`, as the desktop's dialog does. A reply's `downloads` are offered as
  browser downloads and the rest goes to the page.
- **`save_budget`** downloads the budget as a `.mthd` file (`saveMthd`), asking a name with
  the browser's prompt for Save As or a budget never saved, and replies with the view.
- **The charts of accounts** are fetched from `charts/` beside the page: the list when
  `chart_templates` is first asked, a chart's file when `chart_template_new` names it
  (above, "The charts in a browser").
- **Desktop-only commands** are refused with "That is only in the Method Budget app, not in
  the browser."; the few the page asks at start-up are answered as nothing there
  (`autosave_setting` false, `open_pending` null, `recovery_list` empty, `whats_new` null),
  and `print_report` says it isn't supported, so the page prints with the browser's own
  print dialog.
- **`event.listen`, `emitTo`, `emit`**: one bus for the page and the Budget Setup window.
  After a command whose reply is a view with a new `rev`, every other window hears
  `view-changed`, as the desktop's `tools::changed` does. The page's title follows the
  budget in front, and leaving the page asks first while a budget has unsaved changes.
- **Budget Setup** (`setup_window`) shows the same page with `?panel=setup` in a frame over
  the page; it shares the main page's Host and bus, and `close_setup_window` removes it
  and tells the page (`setup-closed`). The tools panel has no window of its own in a
  browser: it docks at the right or the bottom.

### What the page does differently in a browser

All of it behind `host_info` saying `"web"` (`ui/js/host.js`), so the desktop is unchanged:

- No menu bar: the page answers the app's keys itself on macOS too (`pageRunsAppKeys` in
  `shortcuts.js`; elsewhere it already did), and a **File** button in the toolbar opens
  the File menu's items a browser can run (Open, Save, imports and exports, Versions,
  Compare, What Changed, Merge, Print, Keyboard Shortcuts). The other menus' commands are
  on their keys, the toolbar and the right-click menus. Keys the browser keeps for itself
  (Cmd or Ctrl with N, T, W in most browsers) don't reach the page.
- New (button and key) makes a new tab: there is no start window. Closing the last tab
  leaves a new Untitled budget.
- Hidden: the Autosave switch, Share (folder sharing), the update indicator, the tools
  panel's Pop Out button, the setup library (the tools panel's Library tab, the Budget
  Setup rail's Budget and Library switch, Save to Library in the setup lists' strips).
- The Budget Setup window may ask `host_info` too (its capability allows it on the
  desktop, where the answer changes nothing).

### What isn't in the browser

Folder sharing, autosave, recovery copies and backups, Open Recent and files handed over by
the system, the start window, the updater and release notes, printing to PDF through the
system (the browser's print dialog can), pop-out windows, the setup library and the Packs
folder (packs import into and export from a budget), and Load theirs (no file changes
outside). Save a Copy for Sharing, templates, exports and setup packs download their files.
Save to Library in the right-click menus still shows and is refused with the plain message.

### Checked in a browser (2026-10-12)

`cargo xtask web`, served with `python3 -m http.server`, driven in a Chromium-based browser:
the starter budget's topsheet draws; opening a category and an account; editing a rate
(totals and the footer follow); undo and redo by button and by key (Cmd+Z, Cmd+Shift+Z);
Budget Setup in its frame, editing a fringe rate there (the page behind updates through
`view-changed`), closing it by Close and by Escape; Save (a `.mthd` download, the edited
mark cleared); Open of a `.mthd` (a new tab) and of a budget export fixture
(`crates/method-format/tests/fixtures/exchange/by-account.json`: the review, then Convert, a
new tab with the export's totals); Open cancelled (nothing happens); `?open=`; File >
Export > Excel Workbook (an `.xlsx` download); Compare with an added file; Versions (saved
with the browser's time); `?readonly=1` (an edit refused with the plain message); a new
tab and closing both tabs; Print Topsheet (the browser's print). The file picker and the
downloads were driven by a test harness in the page (a picked file put into the input,
downloads caught), as a browser automation tool can't use the system's file dialog.

### Checked in a browser: the demo (2026-10-12)

`cargo xtask web --demo --out site/try`, the whole site served with `python3 -m
http.server`, in a Chromium-based browser: the home page's new button and nav link open
`/try/`, which opens the sample budget (`?open=` added by demo.js) under the note;
drilling into an account and editing a rate (the footer's totals follow); Save and Save As
download the `.mthd` (Save As falls back to the suggested name where the browser has no
`prompt`); Open of another `.mthd` (a new tab); New From Template lists the charts after
fetching only `charts/index.json`, and picking one fetches only that chart's file and
opens it; Budget Setup in its frame, styled and without the note; hiding the note; dark
mode; no Content-Security-Policy refusals. At phone width (375 px) the note says the app
works best on a computer: the grid is mostly covered by the tools panel, and the home
page has no sideways scroll.

### The smoke test

`node scripts/web-smoke.mjs [dir]` (CI, after `cargo xtask web`; the Pages workflow, after
`--demo`): no npm packages. It serves the folder with Node's own http server, starts the
runner's headless Chrome with `--remote-debugging-pipe` and speaks the DevTools protocol
over that pipe (so it needs no WebSocket client), opens the sample budget with `?open=`,
and checks that the topsheet draws with its grand total, that an edit (a fringe's rate,
through the shim's `invoke` as Budget Setup sends it) changes the footer's grand total to
the engine's, that the chart list and one chart are fetched only when asked for and the
chart opens as a new budget, and that the page logs no errors (Content-Security-Policy
refusals included). `CHROME=<path>` picks the browser; it takes about two seconds.

## What a partner integrates against

- `method_session::Host` (or, in a browser, `method-web`'s JS API above) with `dispatch`,
  the `COMMANDS` list and `COMMANDS_VERSION`; this note's command table; docs/APP.md for each
  command's reply (the `View`).
- `.mthd` text in and out (`open_mthd`, `save_mthd`); docs/FORMAT.md for the file. Schedules
  and anything else they store are theirs.
- `method_session::clock::install` before the first budget, in a browser.
- Sharing is their server's (one editor at a time, others read-only with updates); nothing
  of our folder sharing is in the browser build. A viewer can open the latest `.mthd` with
  `open_mthd` again as it changes; `Host::set_read_only(true)` keeps the viewer's page from
  changing its copy.
- Files in and out as base64 (`file`, `files`, `downloads`; above).
- Open for later: a setup library and packs kept in the partner's storage (the library is a
  file on the desktop, so its commands have no host form yet).
- Clean room both ways: the partner sends no code to this repository, and nothing derived
  from another program's files or code enters it (CLEAN-ROOM.md).
