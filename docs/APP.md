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

Enter or a double-click goes down a level; Escape or the breadcrumb goes back up. On the
topsheet and the account list, a double-click on the number or the description edits it in
place instead (Enter keeps it, Escape cancels, one undo step), so double-click the total, or
press Enter, to open the row. Each
breadcrumb segment below the budget has a small button (▾) listing its siblings, so you can
jump straight to another category or another account in the category; Down arrow on the
button opens the list, then arrows, a typed number or name, Enter, and Escape. On the
detail level, the ‹ › buttons beside the account step to the previous or next account,
across categories (Cmd+Right and Cmd+Left, or Cmd+] and Cmd+[). On the detail grid,
typing in a cell starts editing it, Enter and Tab commit and move on, Escape cancels.
Formulas can use globals (`SHOOT_DAYS / 5`). A cell that can't be calculated is kept as
typed, outlined in red with the reason on hover, and the banner says what is wrong; the
rest of the budget still totals, counting that cell as blank.

**Quick entry.** Typing in a unit, currency, set or location cell opens a short list of
matching names under the cell (names that start with what you typed first, then names
that contain it; capitals don't matter), with "Add unit ..." (or set, location, currency)
at the end when no name matches exactly. In amount, X, rate and 4X cells the list offers
the globals whose names start with the name at the caret, anywhere in the formula, and
"Create global ..." when no global has that name. Fringe and group cells complete the
item at the caret in their comma list, leaving out names already in it. Up and Down move
the highlight; Enter or Tab puts the highlighted name in and then finishes the edit as
usual (Tab moves right); Escape closes the list, and a second Escape cancels the edit; a
click puts the name in and keeps editing. Committing a unit, set, location or currency the
budget doesn't have adds it to setup with the edit, as one undo step (a new currency
starts at an exchange rate of 1, and a message says to set its rate); a name typed in
other capitals is written the way setup spells it. "Create global" asks for the global's
value in a small box next to the cell, then adds the global and sets the cell as one undo
step; Cancel goes back to the cell with the formula as typed. A formula that names an
unknown global without creating it is kept and flagged, as before.

**New lines.** An inserted line starts with an empty description (the cell shows a hint)
and copies tags from the nearest line above it, or below it when there is none above.
Which tags is a budget setting saved in the file (Budget > Settings > New lines): fringes
by default, and optionally groups, set, location and currency. Subtotals, adjustments and
separators copy nothing.

**Notes and the formula bar.** Edit > Note (Shift+Cmd+Space) opens a small editor for the
line's note next to its description: Save (or Cmd+Enter) keeps it, an empty note removes
it, Escape cancels. The bar above the grid shows the selected cell as typed: a formula
with its global names, a number unformatted, or a read-only cell's value.

The commands live in `src/quickadd.rs` (notes use `set_row_note` in `src/rowstyle.rs`):

| Command | Arguments | Returns |
|---|---|---|
| `set_cell_creating` | `rowId`, `field`, `value`, `create` (`{kind, name, value}`: kind `unit`, `set`, `location`, `currency` or `global`; `value` is a global's formula or a currency's rate, 1 when left out) | View |
| `set_new_line_defaults` | `defaults` (`{fringes, groups, set, location, currency}`, each true or false) | View |

`set_cell_creating` is refused, with nothing changed, when the item already exists, the
column doesn't hold that kind of item (units go in the unit columns, globals in amount, X,
rate and 4X), the name or value isn't valid, or the row can't take the value.

**Detail grid columns.** The button in the grid's top-left corner (Shift+Tab from the
first cell reaches it) opens the column menu: X, the three unit columns, currency, 4X,
fringes, fringe percent, fringe dollars, groups, set and location can each be shown or
hidden, and the choice is remembered for you (in the web view's local storage, not in the
budget). Fringe % is the line's fringe dollars over its total, blank when the total is 0.
Columns are sized to the window: when there isn't room for every column at its usual
width, they all narrow toward a minimum (numbers keep room for their digits; long text is
cut with an ellipsis, in full on hover) before the grid scrolls sideways, and while it
scrolls, # and Description stay at the left and Total at the right. At 1280 pixels wide
with the tools panel docked at the right, the default columns fit without scrolling (docked
at the bottom, the grid has the window's whole width).

**Row styles.** The Format menu (and the grid's right-click menu) styles the selected
lines: Bold (Cmd+B) and Italic (Shift+Cmd+I) go on, or off when every selected line has
them already; Indent More and Indent Less (Option+Cmd+Right and Left) move each line one
step, keeping lines at different depths apart; Text Color picks one of eight colors or No
Color; Clear Formatting takes all of it off. Each is one undo step. The eight colors read
at 4.5:1 or better on the grid in a light or a dark window: the file stores the light
value, and a dark window shows a lighter one. Bold, italic and indented lines (up to 4 levels) show as such. A line's text
takes its own color, else the color of its first group (by name) that has one; lines a
group keeps out of the total are greyed and struck through. A separator row is drawn as a
rule: it can be selected and deleted, has nothing to edit, and Enter after an edit moves
past it (Insert > Separator, or the Insert separator button). A row with a note has a
small mark in the corner of its description; the note is its tooltip. Lines that share a
fringe range have a bracket in the # column. The printed report shows bold, italic, indent
and separators too.

**Right-click menus.** A right-click on a cell (Control-click on a Mac) moves the cursor
there, keeping a selection of several rows it falls inside, and opens a menu of what can be
done to it; Shift+F10 or the keyboard's menu key opens the same menu at the cursor cell.
Up and Down move (passing over rules and dimmed items), Right or Enter opens a submenu, Left
or Escape closes one, Enter or Space chooses, a letter jumps to the next item starting with
it, Escape or Tab closes the menu, and a key with Command or Ctrl closes it and does what
it does anywhere. Each item shows its key. On detail lines: Cut, Copy, Paste; Insert line,
subtotal, adjustment, separator; Delete rows; Format (bold, italic, indent more and less,
text color, clear formatting, with check marks for what the selection has); Add or Edit
note; Make and Remove fringe range (dimmed when the selection can't); Apply group and Apply
fringe (every group or fringe, checked when every selected line has it, a dash when some
do; choosing one puts it on them all, or takes it off, as the apply panel does). On the
account list: Cut, Copy, Paste; Open account; Insert, Rename (edits the name in place) and
Delete account. On the topsheet: Cut, Copy, Paste; Open category; Insert category,
Insert production total, Rename, Delete; and Add or apply a charge, Add or apply a credit
(the Charges and Credits tabs). A right-click on a total row offers only the inserts, and
one inside the cell editor gets the system's own text menu. Paste from the menu goes
through the web view's paste command; where the web view asks first, or refuses, the key
(Cmd+V) always works.

Styles, notes and group colors are these commands, in `src/rowstyle.rs`:

| Command | Arguments | Returns |
|---|---|---|
| `set_row_style` | `rowIds`, and any of `bold`, `italic`, `indent` (0 to 4), `indentBy` (-4 to 4: each row moves that many steps, kept within 0 to 4; not with `indent`) and `color` (`#rrggbb`, or `null` to clear); fields left out are left alone | View |
| `set_row_note` | `rowId`, `note` (`null` or blank removes it) | View |
| `set_group_color` | `name`, `color` (`#rrggbb` or `null`) | View |
| `update_group` | `name` (an existing group), `group` (`{ description, in_budget_total }`), `color` (`#rrggbb` or `null`): all three as one undo step | View |

The tools panel (below) holds the budget's setup tables and puts fringes, groups, sets,
locations and currencies on the selected lines. It docks at the right (the default) or at
the bottom, or pops out into a window of its own. Option+Cmd+S hides and shows it wherever
it is.

The footer shows the above-the-line, below-the-line, grand and net totals. Drilled down,
it adds the current account's total and its category's total, each labeled with code and
name. It shows the totals of the selected detail rows (Shift+arrows or Shift+click to
select a range, Cmd/Ctrl+click to add rows), and at the right two change counters: **This
session** is the net total now less the net total when the budget was opened or created,
and **Last change** is how much the most recent edit moved the net total (0 for an edit
that moves no money; an undo shows the reverse). Both start at 0 when a budget is opened
or created.

### Tools panel

The panel is two tab panes. Docked at the right they are stacked; docked at the bottom
they sit side by side under a grid that gets the window's whole width; popped out they are
stacked in a window called Tools. The first pane starts with **Fringes, Groups, Globals and
Units**, the second with **Sets, Locations, Currency, Notes and Budget**; Budget has tabs of
its own: **Charges, Credits, Info, Settings and Baseline**. Drag a tab along its strip to
reorder it, or onto the other pane's strip to move it there (a bar shows where it will
land); from the keyboard, Option+Shift+Left and Right move the focused tab along its pane
and Option+Shift+Up and Down move it to the first or second pane. A pane whose tabs have
all moved away keeps an empty strip to drag one back to. The arrangement, each pane's tab
and Budget's tab are remembered for you (in the web view's local storage, not in the
budget). The View > Tools Tab items and their keys (Cmd+3 Fringes, Cmd+G Globals...) show
a tab wherever it is now and put focus on it.

The header shows what is selected in the grid (`3 rows selected`, `All lines in 20
Production staff`) with its line total, fringe total and effective fringe percent, and three
small buttons for where the panel goes: **Dock at right**, **Dock at bottom** and **Pop out
into a window** (also View > Tools Panel, with Show or Hide). The choice is remembered.
Docked, drag the panel's edge to make it wider (at the right) or taller (at the bottom), or
focus the edge and use the arrow keys (Shift for bigger steps); double-click it for the
standard size. Each place keeps its own size.

**The lists.** Fringes, Groups, Sets, Locations and Currency each list every item the budget
defines, with a checkbox that reflects the grid's selection: checked when every selected
line has it, a dash when some do (with how many, `2/5`), empty when none do. On the
topsheet and the account list, the selection is every line under the selected categories
or accounts. Next to each item are its dollars within the selection: a fringe's own
dollars, and for groups, sets, locations and currencies the selected lines' totals (a group
that is out of the budget total says so instead). Its tooltip has its description and what
it comes to across the whole budget. The foot of the fringe list shows the selection's
fringe total and effective percent.

A click on an item's **checkbox** (or Space when it has focus) puts it on every selected
line that can take it, or takes it off them all when every one already has it. A click on
the item's **name** only selects it (highlighted, with focus), and a **double-click on the
name**, or Enter or F2, opens its editor next to it; Delete (or Backspace) asks, next to the
item, before deleting it. Up and Down move through the list. Sets, locations and currencies
are one per line (round checkboxes): picking one replaces each line's old value, and Clear
takes it off (for a currency: back to the base currency). Fringes and currencies go on
lines; groups, sets and locations on lines and adjustments; other rows are skipped.
**Make range** turns the selected lines into one fringe range with a number new in that
account; they must be next to each other in one account, with at least two lines
(unselected subtotal rows in between are fine, since they never break a range). **Remove
range** takes the selected lines out of their ranges. Each list has a filter box and an
"Applied only" switch. Every click is one undo step.

**Editing an item where it is listed.** The editor holds a fringe's kind, rate, cutoff
(percent) or what it is charged per (flat) and description; a group's description, "counts
in total" switch and line color (No color or one of the eight colors; a color set some
other way is kept unless another is picked); a set's description; a location's
description, city, region and country; a currency's rate, symbol and description. Enter
saves, Escape or Cancel closes it, a refused value keeps it open with the message, and
saving with nothing changed does nothing. Each save is one command, so one undo step (a
group's three fields go together through `update_group`). The editor's **Delete…** asks
before deleting, as the Delete key does. The row at the bottom of every list (**New
fringe…**, **New group…**, and so on) opens the same kind of form for a new item, name
first; a name already in use, or with a comma, is refused there.

**Globals and Units** are edited in place, as before: a global's formula (with its current
value beside it), a unit's plural, hours and description; Enter or leaving the box saves,
Escape puts the old text back, and the × asks before deleting. **New global…** and **New
unit…** at the bottom add one.

**Notes.** The Notes tab shows the note on the line under the grid's cursor, in a box you
can type in: Save (or Cmd+Enter) keeps it, Revert (or Escape) puts back what is saved, and
an empty note is removed. It is the same note as Edit > Note. Below it, **Noted lines**
lists every line in the budget with a note (account, line number, description and the start
of the note); a click opens that line's account with the line selected.

**Budget.** Charges, credits, budget info, settings (currencies, fringe posting, account
numbers, what new lines copy) and the baseline, as in the setup tabs before.

**The tools window.** Popped out, the panel is a window of its own (title Tools) that you
can put anywhere, a second screen included; it opens where it was last, at the size it was
(on a screen that is gone, centered). It works on the same budget: a change made in either
window shows in the other at once, the panel follows the grid's selection in the main
window, and a click on a noted line moves the main window's grid. Its header's Dock at right
and Dock at bottom put the panel back; closing the window (its close button, or Close Window
while it is in front) docks it at the right. Option+Cmd+S hides and shows it. Closing the
main window closes it too. Keys in the tools window: text fields keep their own editing keys
(Cmd+Z undoes typing there); the tab keys (Cmd+3...) show a tab in it; Close Window docks it;
everything else (Save, Undo, Print, grid commands) goes to the main window, as the menu bar
does while the tools window is in front.

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

`tools.js` draws the panel and mounts `apply.js`'s lists and header. `main.js` gives it the
grid's selection (`selection()` in the panel context: level, row keys, current category,
cursor row and account) and fires `fwb:selection` and `fwb:cursor` events on the document
when the selection or the cursor changes; the lists ask for a fresh summary on those and on
every new view, and the Notes tab redraws its line.

The tools window and the Notes tab use these commands, in `src/tools.rs`:

| Command | Arguments | Returns |
|---|---|---|
| `tools_window` | `show` (true shows it, making it if needed, and brings it to the front; false hides it), `geometry` (`{x, y, width, height}` in logical pixels, or `null`: where it opens the first time) | nothing |
| `close_tools_window` | none | nothing |
| `notes_list` | none | `[{ row_id, ti, ai, category, account, account_name, line, description, note, more }]`, every line with a note in budget order; `note` is cut to 280 characters (`more` says so) |

### Keyboard and menus

Everything has a key, and the menu bar (File, Edit, Insert, Format, Go, View, Help) lists the same
commands with the same keys. Help > Keyboard Shortcuts (Shift+Cmd+/ or Ctrl+Shift+/) shows the full
list for your platform. The list lives in one place, `ui/js/shortcuts.js`; the menu bar
(`src/menu.rs`) uses the same ids and keys, and a Rust test fails if they drift apart.

| Action | macOS | Windows and Linux |
|---|---|---|
| New, Open | Cmd+N, Cmd+O | Ctrl+N, Ctrl+O |
| Save, Save As | Cmd+S, Shift+Cmd+S | Ctrl+S, Ctrl+Shift+S |
| Print the budget (topsheet and detail), the topsheet | Cmd+P, Shift+Cmd+P | Ctrl+P, Ctrl+Shift+P |
| Close the window, Quit | Cmd+W, Cmd+Q | Ctrl+W, Ctrl+Q |
| Undo, Redo (in a text field: its typing) | Cmd+Z, Shift+Cmd+Z | Ctrl+Z, Ctrl+Y or Ctrl+Shift+Z |
| Cut, Copy, Paste cells (tab-separated, like a spreadsheet; with whole rows selected, the rows with everything on them, below) | Cmd+X, Cmd+C, Cmd+V | Ctrl+X, Ctrl+C, Ctrl+V |
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
| Insert a subtotal, an adjustment | Cmd+=, Cmd+minus (or Option+Cmd+A) | Ctrl+=, Ctrl+minus (or Ctrl+Alt+A) |
| Add or edit the line's note | Shift+Cmd+Space | Ctrl+Shift+Space |
| Bold, italic on or off for the selected lines | Cmd+B, Shift+Cmd+I | Ctrl+B, Ctrl+Shift+I |
| Indent the selected lines more, less | Option+Cmd+Right, Option+Cmd+Left | Ctrl+Alt+Right, Ctrl+Alt+Left |
| Open the right-click menu at the cursor cell | Shift+F10 (or Control-click, or the menu key) | Shift+F10 (or the menu key) |
| Make the selected lines one fringe range; take them out of their range | Cmd+R; Shift+Cmd+R | Ctrl+R; Ctrl+Shift+R |
| Up a level; down a level (also Enter) | Cmd+Up; Cmd+Down | Ctrl+Up; Ctrl+Down |
| Go to the topsheet | Shift+Cmd+Up | Ctrl+Shift+Up |
| Next, previous account (across categories, wrapping; also the ‹ › buttons; not while editing a cell, where they move the caret) | Cmd+Right, Cmd+Left (or Cmd+], Cmd+[) | Ctrl+Right, Ctrl+Left (or Ctrl+], Ctrl+[) |
| In a type-ahead list: move, take the name, close it | Up, Down; Enter or Tab; Escape | Up, Down; Enter or Tab; Escape |
| Open a breadcrumb jump list or the column menu (on its button) | Down or Enter | Down or Enter |
| The column menu button | Shift+Tab from the first cell | Shift+Tab from the first cell |
| Show or hide the tools panel (docked or its window) | Option+Cmd+S | Ctrl+Alt+S |
| Resize the docked tools panel (on its edge; Shift for bigger steps): wider or narrower at the right, taller or shorter at the bottom | Left, Right; Up, Down | Left, Right; Up, Down |
| Tools: Globals, Fringes, Groups, Locations (Escape returns to the grid) | Cmd+G, Cmd+3, Cmd+4, Cmd+5 | Ctrl+G, Ctrl+3, Ctrl+4, Ctrl+5 |
| Tools: Currency, Units, Sets, Budget > Settings | Cmd+6, Cmd+7, Cmd+8, Cmd+9 | Ctrl+6, Ctrl+7, Ctrl+8, Ctrl+9 |
| Tools: Budget > Credits, Budget > Charges | Cmd+\\, Cmd+/ | Ctrl+\\, Ctrl+/ |
| On a tools tab: the previous or next tab in its pane (also Home, End) | Left, Right | Left, Right |
| On a tools tab: move it along its pane; to the first or second pane | Option+Shift+Left, Right; Option+Shift+Up, Down | Alt+Shift+Left, Right; Alt+Shift+Up, Down |
| On a list item: apply it to the selection; edit it; delete it; move up or down | Space; Enter or F2; Delete; Up, Down | Space; Enter or F2; Delete; Up, Down |
| In the Notes tab: save the note; put back the saved note | Cmd+Enter; Escape | Ctrl+Enter; Escape |
| Keyboard shortcuts | Shift+Cmd+/ | Ctrl+Shift+/ |

The row keys (Cmd+I insert, Cmd+D delete, Cmd+= subtotal, Cmd+minus adjustment, Cmd+R
fringe range, Shift+Cmd+Space note) and the setup keys (Cmd+G globals, Cmd+3 to Cmd+9,
Cmd+\\ credits, Cmd+/ charges) follow the habits of people coming from other budgeting
tools, so the shortcut sheet moved to Shift+Cmd+/ and Cmd+1 and Cmd+2 have no setup tab;
fill down is on Shift+Cmd+D instead of the spreadsheet Cmd+D. Insert > Account, Category and Production Total have no key; they go to the level they
belong to first and open the same dialog as the buttons. Insert > Separator has no key
either; it adds a separator row below the cursor on the detail grid. The Notes, Info and
Baseline tabs are in View > Tools Tab; where the panel goes is in View > Tools Panel. Cmd+I inserts a line, so italic is Shift+Cmd+I;
Cmd+[ and Cmd+] step accounts, so indenting is Option+Cmd+Right and Left. Format > Text
Color and Clear Formatting have no key.

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
all, the tools panel, help) to the menu and handles grid keys itself, which stops the menu
from firing too. On Windows and Linux `main.js` handles every key and ignores a menu event
that repeats a key it handled in the last 400 ms. Cut, Copy and Paste are the stock menu
items everywhere: text fields edit natively, and the grid answers the browser's `copy`,
`cut` and `paste` events, so no clipboard permission or plugin is involved. In a text field,
Undo, Redo and Select All act on the field; grid keys that move the caret (Cmd+Up,
Cmd+Left, Cmd+Right) or delete (Cmd+Backspace) stay with the field, and other grid keys are ignored there. Grid
commands that arrive from the menu while a text field has focus are ignored too, because
where menus see keys before the page (GTK on Linux) the key was meant for the field.
Close Window and Quit from the menu are answered in Rust (both ask about unsaved changes);
their keys on Windows and Linux close the window from the page, which asks the same way.
Menu items are never disabled: Undo and Redo also serve text fields, and each command does
nothing where it doesn't apply. While a dialog is open, the menu only reaches its fields.

Closing the window or quitting with unsaved changes asks first (with Autosave on, they
are saved into the file instead, below).

### Copy and paste rows

**Whole rows.** When the selection is whole rows (Select All, or Shift+arrows across every
column), Copy and Cut work on rows instead of cells: on the detail grid the lines, on the
account list the accounts with all their lines, on the topsheet the categories with all
their accounts and lines (production totals are left out, and a cut leaves them). The
clipboard still gets the rows' cells as tab-separated text for spreadsheets, and the app
keeps the rows themselves: every field (formulas as typed, units, X, rate, currency, 4X,
fringes, groups, set, location, fringe range, bold, italic, indent, color, note, row kind)
and the definition of every setup item they use: fringes, groups, units, sets, locations,
currencies, and the globals their formulas name, with what those use in turn (a fringe's
flat unit and the globals in its rate, the globals in a global's formula, however deep).
Edit > Copy Rows, Cut Rows and Paste Rows (also on the right-click menu) do the same
whatever columns are selected; with a single cell selected, Copy Rows copies its row.

**Paste** with copied rows on the clipboard puts them in as new rows: lines above the
selected line (at the end of an empty account), accounts above the selected account in the
category on screen, categories above the selected topsheet row. Pasted lines get new ids;
pasted fringe ranges get numbers new in the account, so they never join a range already
there; pasted accounts and categories keep their numbers when free, else count on (an
account copied as `11-05` goes into category 20 as `20-05`, or `20-06` when that is taken,
wrapping round to `20-01`); a pasted category's accounts move under its new number. A line
in the base currency of the budget it came from counts in this budget's base currency. The
paste, with any setup it adds or changes, is one undo step; a cut deletes as one step and
its paste is another. Anything else on the clipboard (cells copied since, here or in another
app) pastes as cells, as before. Lines go only into an account, accounts only into a
category's account list and categories only onto the topsheet; elsewhere a message says
where to go.

**The review.** Before rows land, each setup item they use is checked against this
budget: missing here (it will be added), the same here (nothing to do), or set up
differently under the same name. With nothing missing or different (a paste within one
budget, usually) the rows go straight in. Otherwise a dialog lists each difference with
both definitions in words, and what will be added. For each difference the choice is
**Keep this budget's version** (the default, decision 12 in ROADMAP.md: the pasted lines
then follow this budget's rate or cutoff, so they may total differently), **Use the copied
version everywhere here** (it replaces this budget's, for every line that uses it), or
**Add the copied one as** a new name (`FICA (2)`; globals `DAYS_2`, so formulas can still
use them), which the pasted lines, and any copied fringe or global that names it, then use.
A currency can't take a new name. Paste applies everything as one undo step; Cancel,
Escape or a click outside changes nothing. Items that only a kept item would have needed
(say a unit used by a copied fringe whose own version is kept here) are still added.

**One clipboard for the app.** The copied rows live in Rust (`src/clipboard.rs`), not in
the page, so a paste reaches any budget open in the app (budgets opening as tabs will
share it). The clipboard text is the key: a paste whose text isn't the rows' text (any copy
since, here or elsewhere, replaced it) is a paste of cells, and the copied rows are
forgotten; copying cells forgets them too. No custom clipboard formats are used (WebKit
doesn't keep them reliably), and nothing extra goes into the text, so spreadsheets get
exactly the cells. If the page can't put the text on the system clipboard (Copy Rows from
the menu bar in a web view that refuses), the rows are still kept and Paste Rows pastes
them; Cmd+V then pastes whatever the clipboard holds.

| Command | Arguments | Returns |
|---|---|---|
| `clip_copy` | `pick` (`{what: "rows", rowIds}`, `{what: "accounts", accounts}` or `{what: "categories", categories}`, topsheet positions), `text` (what went on the clipboard), `onClipboard` | `{token, what, count, lines}` |
| `clip_cut` | the same | View (the copy, then the delete as one undo step) |
| `clip_preview` | `text` (what Cmd+V found; `null` for Paste Rows) | the review, or `null` when that text isn't the copied rows' |
| `clip_paste` | `token` (from the review), `target` (`{account, beforeRow}`, `{category, beforeAccount}` or `{at}`), `resolutions` (`[{kind, name, choice}]`, choice `keep`, `copied` or `rename`) | View (`created_row`: the first pasted line) |
| `clip_forget` | none | nothing |

The review has `what`, `count`, `lines`, `source` (the title of the budget the rows came
from), `items` (`[{kind, name, status, copied, here, rename_to}]`: status `add` or
`differs`, definitions in words), `same` (items that match), `notes` (sentences, such as a
different base currency) and `review` (whether to show it). `clip_paste` is refused, with
nothing changed, when another copy replaced the reviewed one, the target is gone, or a
choice is unknown.

## Saving and recovery

**Autosave** is on unless you turn it off: the Autosave switch in the toolbar next to Save
As, or File > Autosave (one setting for the app, kept in its data folder). With it on, a
budget that has a file is saved into that file 3 seconds after you stop changing it (at
most 30 seconds after the first unsaved change while you keep going), and right away when
the app loses focus (moving between the main window and the tools window doesn't count),
when you close or quit, and before New, Open or Open Recent. The
unsaved-changes mark clears after each autosave. Undo and Redo keep working across
autosaves (an undo is a change like any other and is saved in turn). A budget never saved
(Untitled) can't be saved into a file: it gets a recovery copy instead (below), and the
first Save asks where, as before. If an autosave into the file fails (the folder is gone,
the disk is full), the app says so once and keeps a recovery copy.

**Changes made outside the app.** Because autosave writes the file often, the app never
writes over a version it hasn't seen. When it opens or writes a budget file it notes the
file's size, modification time and a hash of its content; before every autosave and every
Save into that file, and whenever the window gets focus back, it compares. If another
program changed the file meanwhile (a sync service such as Dropbox or iCloud, another
computer, another app), nothing is written, autosave into that file pauses (a recovery copy
keeps the unsaved work safe), and a dialog, "This budget was changed outside Film Wizard",
offers:

- **Keep mine**: the version on disk is copied to Backups, then this budget is saved over
  it and autosave carries on.
- **Load theirs**: this budget is copied to Backups, then the version on disk replaces it,
  as one edit (Undo brings this one back, unsaved).
- **Save mine as a copy…**: Save As; the file on disk is left as it is and the budget
  continues in the new file.
- **Decide later**: autosave stays paused; the question comes back with the next Save (or
  a new change on disk).

If the backup that comes first can't be made, nothing changes. A file that was only
touched (same content, new time) is not a change, and neither is a file that was moved or
deleted (the next save puts it back). Save As to another file never asks.

**Autosave off** leaves the file alone until you save, and keeps a recovery copy of the
unsaved changes on the same timing.

**Recovery copies** are the safety net for a crash, a forced quit or a power cut. They live
in the app's data folder (`Recovery/`, readable only by you: 0700 folder, 0600 files on
macOS and Linux), one per open budget, each a `.fwb` with a small `.json` beside it (the
original path, title and time). A copy is deleted when the budget is saved, when undo
takes it back to the saved state, when you close or quit and choose not to save, and when
another budget replaces it (New, Open) after you agreed to lose the changes. Quitting
without the guard (logging out, the system quitting the app) keeps the changes: into the
file with Autosave on, else into a recovery copy.

On the next start, if copies were left behind, a **Recovered budgets** dialog lists each
one (budget title, original file or "Untitled (never saved)", when it was autosaved) with
**Open Recovered** and **Discard** (a second click confirms; discarding can't be undone).
Later keeps them all for the next start. A recovered budget opens with unsaved changes,
pointing at its original file; the first Save into that file asks before replacing it, and
autosave won't write into it until then. Each running copy of the app holds a lock on a
small file in `Recovery/`, so a second copy of the app never offers the first one's live
copies as recovered.

**Backups.** Before a save replaces a budget file, the version on disk is copied to the
app's data folder: `Backups/<hash of the file's path>/<name> <date time> UTC.fwb`, with a
`source.json` naming the file. An explicit Save always makes one (unless the file is
unchanged since the last backup); autosaves make one on the first write in a session and
then at most every 10 minutes, so the backups span real time rather than the last few
seconds of typing. The last 10 per file are kept. **File > Revert to Backup…** lists the
open budget's backups, newest first; **Restore** puts that version in place of the budget
as one edit (Undo brings the current one back; autosave or Save then writes it), and
**Show Backups Folder** opens the folder in Finder, Explorer or the file manager. We chose
the data folder over a `film.fwb~` file beside the budget: nothing appears in shared
project folders, there is room for more than one version, and the dialog finds them.

**Saves are atomic.** Every write (a save, an autosave, a recovery copy, a backup, the
settings and recent lists) goes to a temporary file in the same folder, is flushed to disk,
then renamed over the target (and the folder flushed, on macOS and Linux). A budget file
keeps the permissions of the file it replaces. A failed write leaves the old file whole.

**File > Open Recent** lists the last 10 budgets opened or saved (newest first, each once;
two files with one name show their folder), kept in the data folder's `recent.json`. Files
that no longer exist drop off when the menu is refreshed (every open and save). Clear Menu
empties it. macOS's own system-wide recent documents list is not updated.

**Opening from the system.** Double-clicking a `.fwb` file, Open With, or dropping one on
the Dock icon opens it: on macOS the app receives an open event, on Windows and Linux the
path arrives on the command line. If the app is running with unsaved changes, it asks
first, as Open does. When several files arrive at once, the first one opens. On macOS a
second double-click goes to the running app. On Windows and Linux there is no
single-instance handling: a second launch with a path starts a second copy of the app with
that file (each copy has its own recovery copies and doesn't offer the other's).

The app's data folder is the platform's local app data folder for
`io.github.ssandweiss.film-wizard-budgeting`: `~/Library/Application Support/...` on macOS,
`%LOCALAPPDATA%\...` on Windows, `~/.local/share/...` on Linux.

| Command | Arguments | Returns |
|---|---|---|
| `autosave_setting`, `set_autosave` | `on` (set) | the setting |
| `flush_autosave` | none | View (autosaved now, if anything was unsaved) |
| `open_pending` | none | View of a file the system asked to open, or `null` |
| `open_recent` | `index` (its place in Open Recent) | View, or `null` |
| `recovery_list` | none | `[{ key, path, name, title, saved_at_ms }]`, newest first |
| `open_recovered`, `discard_recovered` | `key` | View; the remaining list |
| `backup_list` | none | `[{ id, saved_at_ms, bytes }]` for the open budget's file |
| `restore_backup` | `id` | View |
| `reveal_backups` | none | nothing (opens the folder) |
| `resolve_external_change` | `choice`: `keep` or `load` | View |

Rust tells the page about autosave with events: `file-state` (`{ rev, file }` after an
autosave into the file; the page updates the title if `rev` is still current),
`autosave-changed`, `autosave-error`, `open-request` (a file from the system; the page
asks about unsaved changes, then calls `open_pending`) and `external-change` (the file
changed outside the app; the page shows the dialog above).

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
  src/quickadd.rs    quick entry: add a unit, set, location, currency or global from a
                     cell with the edit (one undo step); the new-line defaults setting
  src/clipboard.rs   copy and paste whole rows, accounts and categories with the setup
                     they use: the app's one row clipboard, the paste review, the paste
  src/menu.rs        native menu bar, built from the same table as ui/js/shortcuts.js
  src/tools.rs       the tools panel's own window: opening, hiding and placing it, telling
                     the other window when the budget changed, the Notes tab's list
  src/safety.rs      saving safety as plain files: atomic writes, recovery copies,
                     backups, settings, the recent list (tested with temporary folders)
  src/safety_app.rs  its Tauri side: autosave timer, recovery and backup commands, Open
                     Recent and Autosave menu items, files opened by the system
  src/app.rs         Tauri glue: commands, file dialogs, close and quit guard, menu events
  examples/perf.rs   timings on a large sample budget (fwb_core::sample), and canned
                     replies for a UI harness (perf dump DIR)
  ui/                index.html, styles.css, setup.css, js/*.js (ES modules, no build step)
  capabilities/      what each window's web view may call (main.json, tools.json)
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
(turns a View into rows and columns), `footer.js`,
`modal.js`, `format.js` (money and decimal display), `api.js` (the command queue),
`dom.js`, `shortcuts.js` (the one list of shortcuts, shared with the Rust menu and checked
by a test), `help.js` (the shortcut sheet), `topsheet-setup.js` (the Budget tab's
charges, credits, info, settings and baseline), `report.js` with `print.css` (printable
reports), `menu.js` (the small popup menus: breadcrumb jump lists and the column menu, and
the right-click menu with its submenus), `context.js` (what the right-click menu offers at
each level, opened by the mouse, Shift+F10 or the menu key), `rowformat.js` (bold, italic,
indent, text color and the color palette; `context.css` draws the menu, the palette and
the tools panel's small editors),
`autocomplete.js` (the type-ahead list on a cell editor, a listbox the editor drives with
`aria-activedescendant`), `quickentry.js` (wires it to the grid, sends edits that add
setup items, the note editor, the fringe range keys and the formula bar),
`clipboard.js` (copy, cut and paste of whole rows, and the paste review),
`prefs.js` (per-user display choices in local storage: visible columns, the tools panel's
place, size and tabs) and `sheet.js` (styles worked out at run time, below). `boot.js`
starts `main.js`, or `toolswin.js` in the tools window (the page loaded with `?panel=1`).
The tools panel is `tools.js` (panes, tabs, dragging, the header) with `apply.js` (the
fringe, group, set, location and currency lists), `setup.js` (globals and units, and the
helpers the Budget tabs in `topsheet-setup.js` share) and `notes.js` (the Notes tab);
`tools.css` lays it out. Commands run one at a time, in order.

**Two windows.** Popped out, the panel runs in a second web view on the same page and the
same Rust session; every command works on the one budget either way. After any command
that changes the budget (a new view `rev`), Rust sends `view-changed` to every window but
the one that sent the command, and that window fetches a fresh view (`get_view`); a change
made in the tools window counts in the main window's Last change like any other edit. The
pages also talk directly, each listening only under its own label (`main` or `tools`) so a
window never hears events meant for the other: the main window sends `selection-changed`
(level, row keys, cursor, account) and `tools-state` (that and the tab layout, when the
tools window says `tools-ready`); the tools window sends `tools-position` (a dock button),
`tools-layout` (tabs moved; the main window keeps the preference), `tools-jump` (a noted
line) and `menu` (keys and menu items for the main window). Rust sends `tools-geometry`
(where the tools window is, remembered by the main window) and `tools-closed` (it closed:
the panel docks back). A menu item chosen while the tools window is in front goes to it
first (`src/menu.rs`): it keeps Undo, Redo and Select All for its own text fields and the
tab items, and passes the rest on.

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
  The guard is a plugin hook, so it covers every web view, the tools window included; that
  window is made in Rust with a fixed page (`index.html?panel=1`), never a URL from the page.
- **Least privilege for the web view.** `capabilities/main.json` grants the main window
  only the app's own budget commands (and sending events to the tools window).
  `capabilities/tools.json` grants the tools window, by its label, only what the panel
  calls: `get_view`, the selection summary and the apply edits, the setup tables, the
  Budget tabs, a line's note and the notes list, and listening for and sending events. It
  can't open, save, print, undo, use the clipboard or recovery, or open or close windows;
  its keys for those go to the main window as menu commands, which the main window runs as
  if chosen from its own menu bar. A Rust test checks that every command the panel's
  scripts name is granted and nothing else is. `build.rs` lists those commands in an app manifest,
  which turns on Tauri's access control for them: anything not listed is refused. The web
  view gets no file system, shell, HTTP or dialog permissions; there is no fs or shell
  plugin registered. (`tauri-plugin-dialog` depends on the fs plugin crate as a library,
  but it is never registered, so none of its commands exist.)
- **Paths never come from the web view.** Open and Save show the native file dialog from
  Rust and read or write the chosen file there. JavaScript can ask to open or save, but it
  can't name a path. Saves write a temporary file next to the target, flush it and rename
  it into place, so a failed save never leaves half a file. Other paths come from the
  operating system (open events, the command line) or the app's own data folder (Open
  Recent, recovery copies, backups); the web view names those only by their place in the
  list, a recovery key or a backup's file name, each checked against what is there.
  Show Backups Folder runs the system's `open`, `explorer` or `xdg-open` from Rust on a
  folder Rust chose.
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
import from other tools, find and replace, single-instance handling on Windows and Linux,
the macOS system recent documents list, a mark on copied rows, and the total change a
paste will cause in its review. Printing, the Windows/Linux menu keys, Copy Rows from
the menu bar reaching the system clipboard, and the tools window (placement on a second
screen, keys while it is in front, closing with the main window) still need a live check.
The two tool panes split their height evenly; there is no divider to drag between them yet.
