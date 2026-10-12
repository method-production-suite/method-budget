# Method Budget in a browser: the window-free session

Status: phases 1 and 1b built (decisions.md, entries 70 and 71). This note is the plan and
the contract a partner's web app, and later our own website demo, integrate against.

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
  `setup_sheet_pick`, `actuals_pick`, `merge_pick_file`, `pack_import_pick`), the host's
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
  from `Host`, `"desktop"` from the desktop's command of the same name. The page will use it
  in phase 2 to hide what only the desktop does; it doesn't yet.
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

Commands in version 1 (173: 60 from phase 1, 113 from phase 1b):

| Group | Commands |
|---|---|
| Views | `get_view`, `get_full_view`, `focus_account`, `selection_totals`, `preview_charge` |
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
| Export, reports | `export_budget`, `save_report_preset`, `delete_report_preset` |
| Templates, charts | `template_new`, `template_save`, `chart_templates`, `chart_template_new` |
| Imports | `open_budget`, `import_setup_pick`, `import_setup`, `sheet_import_pick`, `sheet_import_preview`, `sheet_import`, `coa_import_pick`, `coa_import_preview`, `coa_import`, `export_import_pick`, `export_import_preview`, `export_import`, `setup_sheet_export`, `setup_sheet_pick`, `setup_sheet_guess`, `setup_sheet_review` |
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
2. **Phase 1b (done)**: every command module above, each adding its commands to `COMMANDS`
   (no version change: additions), files in and out as base64, `host_info`, and the
   read-only switch.
3. **Phase 2, `method-web`**: a crate with `crate-type = ["cdylib"]` and `wasm-bindgen`
   exports: `new Host()`, `host.dispatch(command, argsJson) -> Promise<replyJson>`,
   `host.openMthd(name, text)`, `host.saveMthd(doc, name)`, `installClock()` (wiring
   `Date.now`, `performance.now` and a counter). A small JS shim defines
   `window.__TAURI__.core.invoke` over `dispatch` (rejecting with `Error::message()`) and a
   minimal `__TAURI__.event` (the page listens for `view-changed`, `autosaved` and the like;
   in a browser most never fire), so `ui/` runs unchanged. For a command with a file form
   the shim shows the browser's file picker and adds `file`, and offers a reply's
   `downloads` and hands the rest to the page. Desktop-only items (Save, Open Recent, Print
   to PDF, the updater, pop-out windows, the library's file, folder sharing) are hidden by
   the page when `host_info` says `web`.
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
  `open_mthd` again as it changes; `Host::set_read_only(true)` keeps the viewer's page from
  changing its copy.
- Files in and out as base64 (`file`, `files`, `downloads`; above).
- Open for later: a setup library and packs kept in the partner's storage (the library is a
  file on the desktop, so its commands have no host form yet).
- Clean room both ways: the partner sends no code to this repository, and nothing derived
  from another program's files or code enters it (CLEAN-ROOM.md).
