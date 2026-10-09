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
cargo build --release -p fwb-app --example perf && target/release/examples/perf
                                            # timings on a 5,000-row sample budget (no window)
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
2. **Accounts** in one category with each account's fringes and total. When fringes post
   by category, the category's fringe account is a bold row before the category total;
   when they post by account, the Fringes column is part of each account's total.
3. **Detail lines** in one account: #, description, amount, unit, X, rate, currency,
   fringes, fringe percent, fringe dollars, groups and the line total. More columns (the
   units next to X, rate and 4X, 4X itself, set and location) are in the column menu.

Enter or a double-click goes down a level; Escape or the breadcrumb goes back up. Each
breadcrumb segment below the budget has a small button (▾) listing its siblings, so you can
jump straight to another category or another account in the category; Down arrow on the
button opens the list, then arrows, a typed number or name, Enter, and Escape. On the
detail level, the ‹ › buttons beside the account step to the previous or next account,
across categories (Cmd+[ and Cmd+]). On the detail grid, typing in a cell starts editing
it, Enter and Tab commit and move on, Escape cancels. Formulas can use globals
(`SHOOT_DAYS / 5`). A cell that can't be calculated is kept as typed, outlined in red with
the reason on hover, and the banner says what is wrong; the rest of the budget still
totals, counting that cell as blank.

**Detail grid columns.** The button in the grid's top-left corner (Shift+Tab from the
first cell reaches it) opens the column menu: X, the three unit columns, currency, 4X,
fringes, fringe percent, fringe dollars, groups, set and location can each be shown or
hidden, and the choice is remembered for you (in the web view's local storage, not in the
budget). Fringe % is the line's fringe dollars over its total, blank when the total is 0.
Columns are sized to the window: when there isn't room for every column at its usual
width, they all narrow toward a minimum (numbers keep room for their digits; long text is
cut with an ellipsis, in full on hover) before the grid scrolls sideways, and while it
scrolls, # and Description stay at the left and Total at the right. At 1280 pixels wide
with the side panel open, the default columns fit without scrolling.

**Row styles.** Bold, italic and indented lines (up to 4 levels) show as such. A line's text
takes its own color, else the color of its first group (by name) that has one; lines a
group keeps out of the total are greyed and struck through. A separator row is drawn as a
rule: it can be selected and deleted, has nothing to edit, and Enter after an edit moves
past it (Insert > Separator, or the Insert separator button). A row with a note has a
small mark in the corner of its description; the note is its tooltip. Lines that share a
fringe range have a bracket in the # column. The printed report shows bold, italic, indent
and separators too.

The side panel edits the budget's setup tables: globals (with their current values),
fringes, groups (with the "in budget total" switch), units and currencies. Drag its left
edge to make it wider or narrower (or focus the edge and use the arrow keys; double-click
for the standard width); the width is remembered. Option+Cmd+S hides and shows it.

The footer shows the above-the-line, below-the-line, grand and net totals. Drilled down,
it adds the current account's total and its category's total, each labeled with code and
name. It shows the totals of the selected detail rows (Shift+arrows or Shift+click to
select a range, Cmd/Ctrl+click to add rows), and at the right two change counters: **This
session** is the net total now less the net total when the budget was opened or created,
and **Last change** is how much the most recent edit moved the net total (0 for an edit
that moves no money; an undo shows the reverse). Both start at 0 when a budget is opened
or created.

### Apply panel

The side panel opens on **Apply**: the half of setup that works on the selected lines
rather than on the tables. It lists every fringe, group, set, location and currency with a
checkbox that reflects the grid's selection: checked when every selected line has it, a
dash when some do (with how many), empty when none do. On the topsheet and the account
list, the selection is every line under the selected categories or accounts. Next to each
item are its dollars within the selection: a fringe's own dollars, and for groups, sets,
locations and currencies the selected lines' totals (a group that is out of the budget
total says so instead). The top of the panel and the foot of the fringe list show the
selection's line total, fringe total and effective fringe percent (fringe dollars over
line totals).

Clicking an item puts it on every selected line that can take it, or takes it off them all
when every one already has it. Sets, locations and currencies are one per line (round
checkboxes): picking one replaces each line's old value, and Clear takes it off (for a
currency: back to the base currency). Fringes and currencies go on lines; groups, sets and
locations on lines and adjustments; other rows are skipped. **Make range** turns the
selected lines into one fringe range with a number new in that account; they must be next
to each other in one account, with at least two lines (unselected subtotal rows in between
are fine, since they never break a range). **Remove range** takes the selected lines out of
their ranges. Each list has a filter box, an "Applied only" switch and an Edit link to its
setup tab. Every click is one undo step.

The commands live in `src/apply.rs`:

| Command | Arguments | Returns |
|---|---|---|
| `apply_summary` | `rowIds`, `accounts` (account numbers), `categories` (topsheet positions) | the summary below |
| `apply_to_rows` | `rowIds`, `field` (`fringes` or `groups`), `name`, `on` | View |
| `assign_to_rows` | `rowIds`, `field` (`set`, `location` or `currency`), `value` (`null` clears) | View |
| `make_fringe_range` | `rowIds` | View |
| `clear_fringe_range` | `rowIds` | View |

The summary has `rows` (every selected row id, in budget order; the edit commands take
these), `lines` and `costed` (how many are lines, and lines or adjustments), `totals` (the
`selection_totals` result), `fringe_percent` (`"18.25"`, or `null` when the lines total 0),
`fringes`, `groups`, `sets`, `locations` and `currencies` (`{ name: { count, total } }` for
every defined item; currencies include the base currency), `in_range` (selected rows in a
fringe range) and `range_blocked` (why Make range can't run, or `null`). Edits check
everything before changing anything: an unknown name, a row that no longer exists, or a
selection with no row that takes the field is refused and nothing changes.

`setup.js` mounts `apply.js` as the panel's first section. `main.js` gives it the grid's
selection (`selection()` in the panel context: level, row keys, current category) and fires
a `fwb:selection` event on the document when the selection changes; the panel asks for a
fresh summary on that event and on every new view.

### Keyboard and menus

Everything has a key, and the menu bar (File, Edit, Insert, Go, View, Help) lists the same
commands with the same keys. Help > Keyboard Shortcuts (Cmd+/ or Ctrl+/) shows the full
list for your platform. The list lives in one place, `ui/js/shortcuts.js`; the menu bar
(`src/menu.rs`) uses the same ids and keys, and a Rust test fails if they drift apart.

| Action | macOS | Windows and Linux |
|---|---|---|
| New, Open | Cmd+N, Cmd+O | Ctrl+N, Ctrl+O |
| Save, Save As | Cmd+S, Shift+Cmd+S | Ctrl+S, Ctrl+Shift+S |
| Print the budget (topsheet and detail), the topsheet | Cmd+P, Shift+Cmd+P | Ctrl+P, Ctrl+Shift+P |
| Close the window, Quit | Cmd+W, Cmd+Q | Ctrl+W, Ctrl+Q |
| Undo, Redo (in a text field: its typing) | Cmd+Z, Shift+Cmd+Z | Ctrl+Z, Ctrl+Y or Ctrl+Shift+Z |
| Cut, Copy, Paste cells (tab-separated, like a spreadsheet) | Cmd+X, Cmd+C, Cmd+V | Ctrl+X, Ctrl+C, Ctrl+V |
| Select every row | Cmd+A | Ctrl+A |
| Fill down | Shift+Cmd+D | Ctrl+Shift+D |
| Delete the selected rows (or the category or account; Cmd+Z brings them back) | Cmd+D | Ctrl+D |
| Clear the selected cells | Backspace or Delete | Backspace or Delete |
| Move; extend the selection | Arrows; Shift+arrows | Arrows; Shift+arrows |
| Next or previous cell | Tab, Shift+Tab | Tab, Shift+Tab |
| First or last column; first or last row | Home, End; Cmd+Home, Cmd+End | Home, End; Ctrl+Home, Ctrl+End |
| Edit a cell keeping its text (or just type to replace it) | F2 or double-click | F2 or double-click |
| Finish editing and move down or up (detail lines) | Enter, Shift+Enter | Enter, Shift+Enter |
| Same value into every selected row | Cmd+Enter while editing | Ctrl+Enter while editing |
| Cancel an edit, clear the selection, or go up a level | Escape | Escape |
| Insert a line below | Cmd+I | Ctrl+I |
| Insert a subtotal, an adjustment | Cmd+=, Option+Cmd+A | Ctrl+=, Ctrl+Alt+A |
| Up a level; down a level (also Enter) | Cmd+Up; Cmd+Down | Ctrl+Up; Ctrl+Down |
| Go to the topsheet | Shift+Cmd+Up | Ctrl+Shift+Up |
| Next, previous account (across categories, wrapping; also the ‹ › buttons) | Cmd+], Cmd+[ | Ctrl+], Ctrl+[ |
| Open a breadcrumb jump list or the column menu (on its button) | Down or Enter | Down or Enter |
| The column menu button | Shift+Tab from the first cell | Shift+Tab from the first cell |
| Show or hide the setup panel | Option+Cmd+S | Ctrl+Alt+S |
| Widen or narrow the setup panel (on its edge; Shift for bigger steps) | Left, Right | Left, Right |
| Setup tab 1 to 9 (Globals ... Credits; Escape returns to the grid) | Cmd+1 ... Cmd+9 | Ctrl+1 ... Ctrl+9 |
| Keyboard shortcuts | Cmd+/ | Ctrl+/ |

The row keys (Cmd+I insert, Cmd+D delete, Cmd+= subtotal) follow the habits of people
coming from other budgeting tools; fill down is on Shift+Cmd+D instead of the spreadsheet
Cmd+D. Insert > Account, Category and Production Total have no key; they go to the level they
belong to first and open the same dialog as the buttons. Insert > Separator has no key
either; it adds a separator row below the cursor on the detail grid. Info, Settings and
Baseline tabs are in View > Setup Tab.

**Cells.** The selection is a set of rows (Shift+Up/Down, Shift-click, Cmd/Ctrl-click) across
a run of columns (Shift+Left/Right). Copy gives tab-separated text, raw formulas for
editable cells and the shown value for totals. Paste starts at the top-left selected cell
and goes down and right, skipping cells that can't be edited; one copied cell fills every
selected cell; numbers like `1,250.50` or `(300)` are cleaned up in number columns. Fill
down copies the top selected cell of each column into the selected cells below it (with one
row selected, the cell above). Paste, fill down, cut and clearing a range on the detail
grid are one command, `set_cells`, so each is one undo step and one recalculation. The
topsheet and account lists take one pasted cell at a time.

**Who answers a key.** A menu item and its key run the same function in `main.js`, by id.
On macOS the web page sees a key first and the menu's key equivalent fires only if the page
didn't handle it, so `main.js` leaves app-level keys (file commands, undo and redo, select
all, the setup panel, help) to the menu and handles grid keys itself, which stops the menu
from firing too. On Windows and Linux `main.js` handles every key and ignores a menu event
that repeats a key it handled in the last 400 ms. Cut, Copy and Paste are the stock menu
items everywhere: text fields edit natively, and the grid answers the browser's `copy`,
`cut` and `paste` events, so no clipboard permission or plugin is involved. In a text field,
Undo, Redo and Select All act on the field; grid keys that move the caret (Cmd+Up) or
delete (Cmd+Backspace) stay with the field, and other grid keys are ignored there. Grid
commands that arrive from the menu while a text field has focus are ignored too, because
where menus see keys before the page (GTK on Linux) the key was meant for the field.
Close Window and Quit from the menu are answered in Rust (both ask about unsaved changes);
their keys on Windows and Linux close the window from the page, which asks the same way.
Menu items are never disabled: Undo and Redo also serve text fields, and each command does
nothing where it doesn't apply. While a dialog is open, the menu only reaches its fields.

Closing the window or quitting with unsaved changes asks first.

## Architecture

```
crates/fwb-app/
  src/session.rs     one open budget: Budget, file path, dirty flag, undo/redo patches,
                     the account on screen, the recalc cache
  src/edit.rs        every edit as a function on &mut Budget (validates, then applies)
  src/diagnose.rs    lenient recalc: flags every bad cell instead of stopping at the first
  src/report_dto.rs  serde shapes for fwb_core::Report (the engine stays serde-free)
  src/view.rs        what the UI draws after each command (lean), and the full view for printing
  src/batch.rs       multi-cell edits (paste, fill down) applied as one undo step
  src/apply.rs       the apply panel: tag the selected rows, fringe ranges, selection summary
  src/menu.rs        native menu bar, built from the same table as ui/js/shortcuts.js
  src/app.rs         Tauri glue: commands, file dialogs, close and quit guard, menu events
  examples/perf.rs   timings on a large sample budget (fwb_core::sample), and canned
                     replies for a UI harness (perf dump DIR)
  ui/                index.html, styles.css, setup.css, js/*.js (ES modules, no build step)
  capabilities/      what the web view may call
```

**Rust owns the budget.** The app state is one `Session` behind a mutex. The web view never
holds the real budget; it sends a command (`set_detail_field`, `add_detail`, `set_global`,
`undo`, ...) and gets back a `View`.

**What crosses the bridge.** Budgets run to thousands of lines, so a view carries what the
screen needs, not the whole budget (decisions.md, entry 18). Its size depends on the
account on screen, not on the budget: about 35 KB plus half a kilobyte per row of that
account.

- `rev`: changes whenever the budget does (edit, undo, redo, a new or opened budget).
- `budget`: the budget's outline as file-format JSON (via `fwb_format`, so the UI reads the
  documented format): setup tables, categories and accounts, charges, credits, info. Each
  account has `lines` (its row count) instead of `details`; the baseline has no `rows`;
  `versions` lists each version's `id`, `name`, `created` and `note`, not its content.
- `report`: every total down to the account from `fwb_core::recalc`, with money as integer
  cents and decimals as strings, never floats. Accounts have no `rows` here.
- `account`: the account on screen in full, or `null`: `ti` and `ai` (its topsheet index and
  index in its category), `code`, `details` (its rows, file-format JSON) and `rows` (their
  totals, as the report's rows would be, or `null` without totals).
- `error`, `partial`, `issues` (problems with globals, fringes, charges and the like, plus
  the cells of `account`), `issue_count` (all problems) and `issue_accounts`
  (`[ti, ai]` of every account with a problem cell, for the red outlines up the levels).
- `variance`: original vs current totals when there is a baseline: `accounts`,
  `categories`, `grand_total`, `net_total`, `rows` for `account`'s rows only, and
  `rows_changed` (rows that moved, anywhere).
- `file` (`name`, `path`, `dirty`), `can_undo`, `can_redo`, and `created_row` after an insert.

| Command | Arguments | Returns |
|---|---|---|
| `focus_account` | `category`, `ti`, `account`, `ai` (all `null`: no account) | View with that account |
| `get_full_view` | none | everything: the whole budget, every row's totals, every problem |

`focus_account` sets the account whose rows every later view carries; the UI calls it when
it opens an account (and with nothing when it leaves one). Rust finds the account by code
first, as the UI does, so an edit that moves it doesn't lose it. Printing calls
`get_full_view`, the view as it was before (`budget` in full, `report` with `rows`, all
`issues`, `variance` with every row).

Every edit runs on a copy of the budget; if it is rejected (an unknown row, a duplicate
account number, a bad global name) nothing changes and the message comes back as the
command's error. An accepted edit puts what it changed on the undo stack (200 steps): the
old rows of the accounts it touched (just the changed rows when none were added or
removed), the rest of the budget only when that changed, the whole budget only when
categories or accounts were added, removed or moved. The dirty flag compares a revision
number with the one last saved, so undoing back to the saved state clears it.

**Recalculation at scale.** Totals are worked out once per change and cached. The session
keeps a `fwb_core::calc::cache::RecalcCache`: an account's rows depend only on the account
and on the globals, fringes, currencies, units and groups, so after an edit only the
accounts whose rows changed are evaluated again (every account when one of those tables
changed); topsheet, charges, credits and breakdowns are always worked out in full. The
result is exactly `recalc`'s (tested over random edit sequences, and against the engine as
it was before these optimizations). On this Mac, a one-cell edit in a 5,000-row budget
costs about 5 ms in Rust from command to JSON.

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
`modal.js`, `format.js` (money and decimal display), `api.js` (the command queue),
`dom.js`, `shortcuts.js` (the one list of shortcuts, shared with the Rust menu and checked
by a test), `help.js` (the shortcut sheet), `topsheet-setup.js` (charges, credits, sets,
locations, info, settings and baseline tabs), `report.js` with `print.css` (printable
reports), `menu.js` (the small popup menus: breadcrumb jump lists and the column menu),
`prefs.js` (per-user display choices in local storage: visible columns, panel width) and
`sheet.js` (styles worked out at run time, below). The side panel opens on Apply (`apply.js`, the selected lines) and groups the
setup tabs into Tables and Budget. Commands run one at a
time, in order.

The grid redraws by patching: a row whose content is unchanged keeps its element, so an
edit touches the rows it changed, and scroll position, focus and an open cell editor stay
put. Only a change of columns (another level, a column shown or hidden) builds the table
afresh. A grid of more than 150 rows (a big account) draws only the rows near the visible
ones, 40 beyond each edge, with spacer rows (sized by a generated rule) standing in for the
rest, so a redraw lays out about a hundred rows however long the account is; scrolling,
the keyboard and the cell editor bring rows in as needed. Scrolling the cursor into view
waits for the next frame, so one layout serves a whole redraw.

## Security model

The app opens files the user picks and nothing else, and the web view is treated as
untrusted (a budget file is data from anywhere).

- **No network.** No updater, no telemetry, no remote content. The CSP limits every
  resource to the app's own bundled files and Tauri's IPC:
  `default-src 'self'; script-src 'self'; style-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`.
  No inline scripts or styles, no CDN. Styles that depend on the window or the budget
  (squeezed column widths, the panel's width, line and group colors) go into constructed
  stylesheets (`CSSStyleSheet` and `document.adoptedStyleSheets`, in `ui/js/sheet.js`),
  never style attributes or `element.style`. Every value in those rules is a number or a
  checked `#hex` color, and a color reaches a row as a generated class (`tr.tc-2a6fdb`).
  Where constructed stylesheets aren't supported the fixed widths in `styles.css` apply,
  the panel keeps its standard width and lines show without color.
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

Moving rows by drag, editing fringe ranges in the grid, original/variance columns in the
grid (the data is in `view.variance`; reports already print it), renaming setup entries (add
the new one, then delete the old), renumbering accounts when the coding pattern changes,
import from other tools, find and replace, and opening a `.fwb` file by double-clicking it
in the file manager. Printing and the Windows/Linux menu keys still need a live check.
