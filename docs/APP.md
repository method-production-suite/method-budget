# The desktop app (`crates/fwb-app`)

Method Budget's editor is a local desktop app built with [Tauri 2](https://tauri.app):
the Rust engine runs natively inside the app, and the interface is plain HTML, CSS and
JavaScript drawn in the operating system's own web view. Budgets are `.mthd` files on your
disk ([FORMAT.md](FORMAT.md); older `.fwb` files open and save the same way). There is no server and no account, and the app
goes online only to check for updates of itself, which can be turned off ("Updates" below).
Why Tauri: [decisions.md](decisions.md), entry 14.

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

**Budget tabs.** Several budgets can be open at once, one per tab in the strip under the
toolbar (decisions.md, entries 19 and 25). A tab shows the file's name without its extension, or
the budget's title before it has a file; a dot means unsaved changes; the tab in front,
and the one under the pointer, show a close button (×). Its tooltip has the title, the
full path and whether the file changed outside the app. Click a tab to bring it to the
front: the grid, breadcrumb, tools panel (docked or in its window), formula bar and footer
all switch to that budget, and each tab comes back where it was left (level, category,
account, cursor, scroll, and the footer's This session and Last change). Each budget has
its own undo, autosave, recovery copy and backups; the row clipboard is shared. New
(Cmd+N) and New Tab (Cmd+T) open a starter budget in a new tab, and so does the **+**
button at the end of the strip. Open, Open Recent, a recovered budget and a `.mthd`
double-clicked in the Finder open in a new tab; a file that is open already just has its
tab brought forward, and an untouched Untitled budget alone in the window gives its tab up
to the file. Close Tab (Cmd+W), the × or a middle-click closes a tab: with Autosave on its
changes go into its file first, and whatever is still unsaved asks **Save**, **Close
Without Saving** or **Cancel**. Cmd+W on the last tab closes the window. Drag a tab (or
Option+Shift+Left and Right on a focused tab) to move it. Ctrl+Tab and Ctrl+Shift+Tab, or
Shift+Cmd+] and Shift+Cmd+[, go to the next and previous tab; Cmd+1 and Cmd+2 to the first
and second (Cmd+3 to Cmd+9 are the Budget Setup tabs); Left, Right, Home and End move between tabs
while one has the keyboard, and Escape goes back to the grid. When the tabs don't fit, the
strip scrolls sideways (the mouse wheel too), with arrow buttons at its ends. The Window
menu lists every open budget, the one in front ticked. The window title names the budget in
front. The next start reopens the files that were open as tabs, in order, with the same one
in front; budgets that never had a file come back through their recovery copies (below).

**Split view.** View > Split Right (Shift+Cmd+\\; Cmd+\\ is the Credits tab) splits the
window into two panes side by side (decisions.md, entry 40; behavior note 04). Each pane
has its own tab strip, breadcrumb, level buttons, grid and footer totals. The new pane at
the right shows the budget in front too, at the same level and account, so the same budget
can be in both panes: two views of one budget, one undo history, an edit in either showing
in both. One pane is the **target**, the one last clicked or focused: it has an accent line
along its tab strip and its tab in front in bold (the other pane's tab in front is in plain
weight). The toolbar (Save, Undo, Versions...), the menus, every key, the tools panel
(docked or in its window), the formula bar and the window title act on the target. New
and Open put the budget in the target pane. Ctrl+Tab, Cmd+1 and Cmd+2 step through the
target pane's tabs. Window > Move Tab to Other Pane, **Move to Other Pane** on a tab's
right-click menu (Shift+F10 or the menu key on a focused tab), or dragging a tab onto the
other pane's strip moves it there, where it comes to the front where it was left; a pane's
only tab can't move. Closing a tab the other pane also shows only takes it from that pane;
a pane whose last tab closes goes away. Drag the divider between the panes (or focus it
and use Left and Right, Shift for bigger steps; Enter or a double-click splits evenly); its
place is remembered for you. View > Close Split (also on a tab's menu, which offers Split
Right when the window isn't split) puts every tab back in one strip, the first pane's tabs
then the second's, with the target's budget in front; each budget comes back where it was
last seen. With the window split already, Split Right goes to the other pane. The split,
which files are in which pane and the target are reopened at the next start.

The window has three levels, with a breadcrumb above the grid:

1. **Topsheet**: categories with their totals, production totals (the first one ends
   above-the-line), then the summary rows: above and below-the-line totals, charges, the
   grand total, credits and the net total (Budget Setup > Settings > Numbers on screen can leave
   out the above and below-the-line rows and the net total).
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
Which tags is a budget setting saved in the file (Budget Setup > Settings > New lines): fringes
by default, and optionally groups, set, location and currency. Subtotals, adjustments and
separators copy nothing.

**Notes and the formula bar.** Edit > Add Note (Shift+Cmd+Space, or Add note… in the
right-click menu) adds a dated note to what the cursor is on: the line on an account's
lines, the account on the account list, the category on the topsheet. A small dialog
opens next to it (it says how many notes the item has already): Add (or Cmd+Enter) keeps
it, Escape cancels. The Notes tab (below) shows, edits, deletes and ticks off notes. The bar above the grid shows the selected cell as typed: a formula
with its global names, a number unformatted, or a read-only cell's value.

The commands live in `src/quickadd.rs` (notes use `add_note` in `src/notes.rs`):

| Command | Arguments | Returns |
|---|---|---|
| `set_cell_creating` | `rowId`, `field`, `value`, `create` (`{kind, name, value}`: kind `unit`, `set`, `location`, `currency` or `global`; `value` is a global's formula or a currency's rate, 1 when left out) | View |
| `set_new_line_defaults` | `defaults` (`{fringes, groups, set, location, currency}`, each true or false) | View |

`set_cell_creating` is refused, with nothing changed, when the item already exists, the
column doesn't hold that kind of item (units go in the unit columns, globals in amount, X,
rate and 4X), the name or value isn't valid, or the row can't take the value.

**Formulas and values.** A number cell holding a formula (`SHOOT_DAYS / 5`, `2 * 3`) shows
its value, in the formula color, with the formula on hover and in the formula bar; editing
the cell edits the formula. A global can be set to **show by name** (its details in the
Globals tab): cells whose formula uses it show the formula instead. A global's own
**decimals** set how its value shows, in the Globals tab and in the cells whose formula
names it first. **View > Show or Hide Formulas in Selection** (Option+Cmd+F; on unless you
turn it off, remembered for you) makes every cell of a selected range (Shift+arrows,
Select All, several rows) show its formula while it stays selected, then its value again.
Plain numbers show as typed. Rates follow the budget's rate decimals (Budget Setup > Settings >
Numbers on screen): a cutoff splits rates by size into two bands, each shown as typed or
with 0 to 8 decimals (rounded half away from zero, padded with zeros), and a rate whose
formula names a global with its own decimals shows those unless that switch is off. All of
it is display only: values, totals, copy and paste and exports use the exact numbers.

**Hidden factors.** When X, 4X or the currency column is hidden and a line's value there
changes its total (an X or 4X other than blank or 1, by value for a formula; a currency
other than the base one), the line's # cell gets a small red dot, and its tooltip names
what the hidden column holds (`X is 2, Currency is EUR`). Show the column (or set it back)
and the dot goes. The unit columns are labels and never change a total, so hiding them
never marks a line. The dot is on screen only: reports don't print it.

**Totals, zero accounts, zoom** (View menu, remembered for you, not in the budget). **Totals
With or Without Cents** switches every total in the grid and the footer between cents and
whole amounts (rounded half away from zero; the exact cents stay underneath). **Show or Hide
Zero Accounts** leaves out the categories on the topsheet and the accounts in a category
that total zero (unless they have a problem cell or were different when the baseline was
locked); the row under the cursor stays, so a new account doesn't vanish as it is made.
**Zoom In**, **Zoom Out** and **Actual Size** (Option+Cmd+=, Option+Cmd+minus,
Option+Cmd+0; Cmd+= is Insert Subtotal) size the grid's text, rows and columns from 70% to
150% in steps of 10%.

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

**Original and variance columns.** When the budget has a baseline (Tools, Budget,
Baseline: Lock), every level shows **Orig** (the total when the baseline was locked) and
**Var** (how far it has moved since) just before Total: categories, the grand and net
totals on the topsheet; accounts and the category total in an account list; each line
and adjustment, and the account total, in the lines. A line added since shows `new`. The
column menu hides or shows them, on the lines (with the other columns) and on the topsheet
and account list (its button sits in the Acct # header there, and only while there is a
baseline); the choice is one for all three levels. Without a baseline they never show.
The account total's Orig is the topsheet's account total, so with fringes posted by
account it sits on the "Total with fringes" row.

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
past it (Insert > Separator, or the Insert separator button). A row with notes has a
small mark in the corner of its description, and an account or category with notes of its
own has one on its name; the tooltip shows the first few notes. The Fringes and Groups
columns show an item's short code instead of its name when it has one (`P&H, PAYROLL`),
with the full names in the tooltip; editing or copying the cell gives the names. Lines that share a
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
Insert production total, Rename, Delete; Create charge… and Create credit… (the Charges or
Credits tab with the new item's name box focused, ready to type); and Add or apply a charge,
Add or apply a credit (the Charges and Credits tabs). All three levels have Move up and Move down. A right-click on a total row offers only the inserts, and
one inside the cell editor gets the system's own text menu. Paste from the menu goes
through the web view's paste command; where the web view asks first, or refuses, the key
(Cmd+V) always works.

**Moving rows.** Option+Up and Option+Down (Alt on Windows and Linux; Edit > Move Up and
Move Down; Move up and Move down in the right-click menu) move the selected rows one place:
lines within their account, accounts within their category, categories and production
totals on the topsheet. Any selection works, contiguous or not: each selected row steps past
the nearest row that isn't selected, the selected rows keep their order, rows already at the
top or bottom stay there and the rest close up behind them. The moved rows stay selected
and the cursor stays on its row, so pressing again keeps going. Presses of the same rows
less than a second apart are one undo step (holding the key is one step, and a run that
ends where it started leaves none). **Fringe ranges stay whole**: a range whose lines are
all selected moves as it is, with the subtotal and separator rows between its lines; a
selected line from a range whose other lines stay behind leaves the range, and the lines
left behind are stepped over as one block, so no line ever lands inside a range it isn't
in. Moving a subtotal or adjustment row changes which lines it adds up, as it would by
hand. Moving lines between accounts is cut and paste (Edit > Cut Rows, Paste Rows).
Dragging rows by the # column isn't built.

| Command | Arguments | Returns |
|---|---|---|
| `move_rows` | `rowIds` (all in one account), `by` (`-1` up, `1` down) | View, with `moved` (the moved rows' ids in their new order: the selection plus any subtotal or separator rows inside a range that moved whole) and `changed` |
| `move_accounts` | `codes` (all in one category), `by` | View, with `moved` (their new places in the category) and `changed` |
| `move_topsheet_rows` | `indexes` (topsheet places), `by` | View, with `moved` (their new places) and `changed` |

All three are in `src/moverows.rs` and work through `Session::edit_merging`, which joins
quick repeats of one edit into one undo step.

Styles and group colors are these commands, in `src/rowstyle.rs` (notes are under
"Notes" in the tools panel, below):

| Command | Arguments | Returns |
|---|---|---|
| `set_row_style` | `rowIds`, and any of `bold`, `italic`, `indent` (0 to 4), `indentBy` (-4 to 4: each row moves that many steps, kept within 0 to 4; not with `indent`) and `color` (`#rrggbb`, or `null` to clear); fields left out are left alone | View |
| `set_group_color` | `name`, `color` (`#rrggbb` or `null`) | View |
| `update_group` | `name` (an existing group), `group` (`{ description, in_budget_total, short_code? }`), `color` (`#rrggbb` or `null`): all as one undo step | View |

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
stacked in a window called Tools. The first pane starts with **Fringes, Groups, Globals,
Units, Presets and Find**, the second with **Sets, Locations, Currency, Notes,
Library** (the setup library, below) **and History** (below). The tabs are for applying
items to the selected lines and for browsing; every setup table's full editor, and the
budget's charges, credits, info, settings and baseline, are in the **Budget Setup** window
(below). Drag a tab along its strip to
reorder it, or onto the other pane's strip to move it there (a bar shows where it will
land); from the keyboard, Option+Shift+Left and Right move the focused tab along its pane
and Option+Shift+Up and Down move it to the first or second pane. A pane whose tabs have
all moved away keeps an empty strip to drag one back to. The arrangement and each pane's
tab are remembered for you (in the web view's local storage, not in the budget; a layout
saved with the old Budget tab simply loses it). View > Tools Panel lists the tabs (without
keys): each shows its tab wherever it is now and puts focus on it. The setup keys (Cmd+3
Fringes, Cmd+G Globals...) open Budget Setup instead.

Each tag list (Fringes, Groups, Sets, Locations, Currency) shows the item's name, its key
figure (a fringe's rate, a currency's rate; else its description), the selection's
checkbox and dollars, its line count and its dollars across the budget, with a filter,
Show, Applied only and Clear above it, and a **Setup** button that opens the list's table
in Budget Setup. An item's right-click menu has **Edit in Budget Setup** (the table, with
that item focused). Double-click still opens the item's small editor next to it. The
Globals tab keeps each global's formula editable in place, with renaming, deleting and
adding, and leaves its details form to Budget Setup; the Units tab shows each unit's
plural and hours as text. Both have a Setup button and the right-click item too.

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
fringe dollars from percentage fringes and from flat fringes, their total, and the effective
percent.

A click on an item's **checkbox** (or Space when it has focus) puts it on every selected
line that can take it, or takes it off them all when every one already has it. A click on
the item's **name** only selects it (highlighted, with focus), and a **double-click on the
name**, or Enter or F2, opens its editor next to it; Delete (or Backspace) asks, next to the
item, before deleting it. Up and Down move through the list. Sets, locations and currencies
are one per line (round checkboxes): picking one replaces each line's old value, and Clear
takes it off (for a currency: back to the base currency). On the fringe and group lists,
**Clear** takes every fringe (or every group) off the selected lines at once, names left on
lines from an edited file included. **Replace tags…** under the selection's totals opens a
form like a tag preset's editor: a box per fringe and group and a pick for the set, location
and currency, starting from what every selected line already shares; Replace makes the
selected lines' tags exactly those (an empty part clears it), as a tag preset's replace does.
Fringes and currencies go on lines; groups, sets and locations on lines and adjustments;
other rows are skipped.
**Make range** turns the selected lines into one fringe range with a number new in that
account; they must be next to each other in one account, with at least two lines
(unselected subtotal rows in between are fine, since they never break a range). **Remove
range** takes the selected lines out of their ranges. Each list has a filter box (it also
matches descriptions and short codes) and an "Applied only" switch. Every click is one undo
step.

**Use counts, sorting and order** (every setup list: Fringes, Groups, Sets, Locations,
Currency, Globals and Units). Each item shows how many lines use it across the budget, as
a link: a click shows the Find tab searching for its uses (a global: its lines' formulas).
The lists with dollars show the item's total across the budget beside it (a fringe: its
fringe dollars; a group, set or location: its lines' totals with fringes, a group out of
the total counted as if it were in; a currency: its lines' totals in the base currency);
globals and units show the count only. The tooltip on a count also says when other setup
uses the item (a global used by another global or a fringe rate, a unit by a flat fringe,
a group by a credit or a charge). **Show** (All, Used or Unused) narrows the list; used
means lines or other setup name it. The **Name** head cycles the list's sort on each
click: A to Z, Z to A, then back to the budget's own order. The budget's order is saved in
the file and followed by the Apply lists, the cell type-ahead and printed setup tables: drag
an item by the dotted handle at its left, or press Option+Shift+Up or Down on it, to move
it (quick key presses are one undo step). Moving is off while the list is sorted or
filtered (text, Show or Applied only), so items that aren't shown never move. The counts
come from Rust a moment after each change (`setup_usage`), not with every view.

**Deleting something in use** says how many lines use it and what deleting does to them:
a fringe, group, set or location comes off them ("Remove from 3 lines and delete"); a
currency's lines go back to the base currency ("Move 3 lines and delete"; charges and the
totals currency must still not use it); a unit is blanked in every unit column that names
it; a global's formulas keep its value as a number. Cancel leaves everything as it was.
Each is one undo step.

**Short codes.** A fringe or group can have a short code (`FICA`, `NQ`): set it in the
item's editor (Short code) or with **Short code…** in its right-click menu; empty removes
it. Codes are unique within fringes or groups (ignoring case), never another item's name,
without commas and up to 16 characters. The list shows the code beside the name, the grid
shows it in the Fringes and Groups columns, and typing a code in those cells offers the
item in the type-ahead (choosing it puts the name in). Lines always refer to items by
name, so renaming an item keeps its code and nothing else changes.

**Editing an item where it is listed.** The editor holds a fringe's kind, rate, cutoff
(percent) or what it is charged per (flat), description and short code; a group's
description, short code, "counts in total" switch and line color (No color or one of the eight colors; a color set some
other way is kept unless another is picked); a set's description; a location's
description, city, region and country; a currency's rate, symbol and description. Enter
saves, Escape or Cancel closes it, a refused value keeps it open with the message, and
saving with nothing changed does nothing. Each save is one command, so one undo step (a
group's three fields go together through `update_group`). The editor's **Delete…** asks
before deleting, as the Delete key does. The row at the bottom of every list (**New
fringe…**, **New group…**, and so on) opens the same kind of form for a new item, name
first; a name already in use, or with a comma, is refused there.

**Renaming an item everywhere.** The editor's first field is the item's name (a currency's
code): a new name renames it, and every line, formula and setting that names it follows,
as one undo step (another change saved with it is a second step), and a message says how
many places changed. A right-click on a list item offers Edit…, **Rename everywhere…** and
Delete…. A global's or unit's name in its tab does the same: double-click it, or Enter or
F2 on it, or right-click it. What follows a rename: a global in every formula (other
globals, fringe rates, Amount, X, Rate and 4X), only as that whole name and with the rest
of each formula as typed; a fringe on lines and in credits' excluded fringes; a group on
lines, in charges' excluded groups, the credits based on it, and report presets' group
filters, keeping its color; a unit in all four unit columns and flat fringes; a set or
location on lines; a currency on lines, flat charges and the totals currency. Tag presets
and line blocks follow too (a block's saved copies of setup are filed under the new name).
Renaming the base currency (Budget Setup > Settings) works the same way. The new name has to be
free: not another item, and not a name something already uses without an item behind it
(a unit typed on lines, a global a formula names), so a rename never merges two names and
renaming back gives the identical budget. Saved versions keep their own names.

| Command | Arguments | Returns |
|---|---|---|
| `rename_setup_item` | `kind` (`global`, `fringe`, `group`, `unit`, `set`, `location` or `currency`), `from`, `to` | View, with `name` (the new name) and `places` (how many places were rewritten) |

**Renumbering accounts** (roadmap 2.20, decisions.md entry 39). Applying a new pattern in
Budget Setup > Settings > Account numbers first works out what every category, account and
category fringe account number would become. When nothing would change, the pattern is
simply applied. Otherwise a preview opens under the form: a table of numbers now and
after (unchanged ones greyed, ones that don't fit in red), any conflicts (a number that
doesn't fit, two numbers that would become one) and warnings (a fringe account that would
share a number with an account), then **Renumber accounts to the new pattern** (off while
there are conflicts), **Change the pattern only** (numbers keep their spelling) or
**Cancel** (Escape). Renumbering is one undo step: the pattern, every category and
account number, and what names them (charges' excluded categories and accounts and their
account, credits' accounts, report presets' left-out categories and accounts, the
baseline's keys, so Orig and Var still match). A number is split on its separator (or by
the current widths when it has none); each part is padded with zeros or loses leading
zeros to fit its new width, never cut. Saved versions keep their own numbers. If the
category or account on screen was renumbered, the grid goes back to the topsheet.

| Command | Arguments | Returns |
|---|---|---|
| `renumber_preview` | `category_width`, `account_width`, `sub_width`, `set_width`, `separator` (as `set_account_coding`) | `{categories, accounts, fringe_accounts, conflicts, warnings, changes}`: each change `{old, new}` (`new` empty when it doesn't fit), conflicts and warnings as sentences |
| `renumber_accounts` | the same | View (refused, nothing changed, on any conflict) |

The command is in `src/rename.rs`; the engine does the work (`fwb_core::rename`).

**Globals and Units** are edited in place, as before: a global's formula (with its current
value beside it), a unit's plural, hours and description; Enter or leaving the box saves,
Escape puts the old text back, and the × asks before deleting. Under each global, a
**Details** line (it shows what is set) opens its description, unit (from the Units tab),
decimals (as calculated, or 0 to 8), folder and **Show by name** switch; each change is
one undo step and none changes a value. Renaming a unit renames it on globals too. **New global…** and **New
unit…** at the bottom add one.

**Notes.** Lines, accounts and categories each keep a list of dated notes. The Notes tab
shows the notes of what the grid's cursor is on: on an account's lines, the cursor's line
and then the account; on the account list, the cursor's account and its category; on the
topsheet, the cursor's category. Each note shows when it was written (and edited) and by
whom, with **Edit** (Save or Cmd+Enter keeps it, Cancel or Escape puts the text back,
saving empty text deletes the note) and **Delete** (asks first). The box under them adds a
note (Add, or Cmd+Enter). Below, **All notes** is a review checklist: every note in the
budget in budget order (a category's own, then each account's own and its lines'), with
where it is, its date and author and the start of its text; the box beside each ticks it
off as reviewed (and back), **Open only** hides the ticked ones, and a click goes to it
(a line's account with the line selected, the account list on an account, the topsheet on
a category). **Your name on new notes** at the bottom signs the notes you add; it is kept
in this window's local storage, and blank signs nothing. Every change is one undo step.

**Budget.** Charges, credits, budget info, settings (currencies, fringe posting, account
numbers, what new lines copy, numbers on screen) and the baseline, as in the setup tabs
before. **Charges**: two switches per charge, saved in the budget, add to its topsheet label
(screen and printed): the percent (percent charges, "Contingency 10%") and the amount left
out ("(excluding 25,000.00)"). "Left out of this charge" lists every group with a box and
the group's own amount in the charge's base as the totals stand now (its counted lines, with
their fringes unless fringes post by budget), the typed amounts, categories and accounts,
and the total left out (each line once). While a percent, a flat amount or a left-out amount
is being typed, the charge's total in its heading is figured from the typed text before
Enter commits it (`preview_charge`, about a tenth of a second after the last key); a value
that isn't a number yet dims the old total. **Credits**: each row shows its spend, the
fringes left out of it, the net spend the percent applies to, the credit (rounded for
display; its tooltip has the exact figure, since rows add up before rounding), and with a
cap, how far the row is under or over it (spend for a cap on the spend, credit for a cap on
the credit).

| Command | Arguments | Returns |
|---|---|---|
| `preview_charge` | `index` (`null` for a new charge), `charge` (as `set_charge`) | `{name, base, excluded, amount}`, figured on the current totals; nothing changes |
| `clear_from_rows` | `rowIds`, `field` (`fringes` or `groups`) | View |
| `replace_tags_on_rows` | `rowIds`, `tags` (`{fringes, groups, set, location, currency}`: names the budget defines; `null` values and empty lists clear; the base currency is none) | View |

`set_charge` takes `label_percent` and `label_excluded` (left out: the charge keeps its own
setting). A credit row's report entry has `cap_variance` (unrounded currency units like
`credit`; positive under the cap, negative over it, `null` with no cap). **Numbers on screen** (saved in the budget, one undo step per change): the rate
cutoff and decimals below and at or above it, whether a rate's global decimals win, and
whether the topsheet shows the above- and below-the-line summary rows and the net total.

| Command | Arguments | Returns |
|---|---|---|
| `set_global_info` | `name`, `info` (`{description, unit, decimals, show_by_name, folder}`: `unit` empty or one of the budget's units, `decimals` `null` or 0 to 8; text trimmed, description up to 500 characters, folder up to 100 on one line) | View |
| `set_display_prefs` | `prefs` (`{rate_cutoff, rate_decimals_below, rate_decimals_above, rate_global_decimals, summary_rows, net_total}`: the cutoff `null`, blank or a number above 0, decimals `null` or 0 to 8) | View |

Both are in `src/display.rs`; a refused value changes nothing. The per-user choices (totals'
cents, zero accounts, zoom, formulas in a selection) never reach Rust.

**Presets** (View > Tools Panel > Presets, and the grid's right-click menu). Both kinds are
kept in the budget, so they travel with the file; versions don't carry them and restoring
a version keeps the current ones.

- **Tag presets** bundle a line's tags: fringes, groups, set, location and currency, any of
  them empty. **Save tags as preset…** (in the tab, or Apply preset > in the right-click
  menu) takes the cursor line's tags, with a box per tag to leave one out, under a name
  (unique ignoring case, up to 80 characters). Each preset is listed with what it holds and
  two buttons for the selection (the selected lines, or every line under the selected
  accounts or categories, as for the lists): **Add** puts the preset's fringes and groups on
  next to the lines' own, and its set, location and currency only on lines that have none
  (a line in the base currency has none); it never takes anything off or changes a value a
  line already has. **Replace** makes the lines' fringes, groups, set, location and currency
  exactly the preset's, so a part the preset leaves empty is cleared. Fringes and currencies
  go on lines, groups, sets and locations on lines and adjustments, as with the lists. Each
  use is one undo step; a preset that names something the budget no longer has (a deleted
  fringe, say) is refused, naming it, and nothing changes. F2 or a double-click on the
  name edits a preset: its name, a box for every fringe and group, and the set, location
  and currency (a name the budget no longer has is shown so it can be unticked).
- **Line blocks** keep copies of rows. **Save lines as block…** takes the selected lines,
  with boxes for what to keep: amount, X, rate and 4X; units; tags (fringes, groups, set,
  location, currency and fringe ranges); notes. A box starts unticked when none of the
  lines use it. Descriptions, row kinds and text styles always come along. **Insert** puts
  copies above the cursor line (at the end of the account when the cursor is on no line):
  new row ids, fringe ranges numbered new in the account, one undo step, the cursor on the
  first new line. A block also keeps the definitions of the setup items its rows use (with
  what those use: a fringe's flat unit, the globals in a formula); when the budget no longer
  has one, the paste review ("Copy and paste rows", below) lists it first and Insert adds it back with the rows,
  or Cancel leaves everything as it was. An item set up differently now keeps the budget's
  own version, without asking. F2 renames a block.

Keys on a focused preset or block: Enter adds (a block: inserts), Shift+Enter replaces, F2
edits, Delete asks and deletes, Up and Down move through the list, Option+Shift+Up and Down
move it in its list. The right-click menu has **Apply preset >** (each preset, with Add tags
and Replace tags) at every level, and **Insert line block >** on detail lines, each ending
with the save item and a way to this tab.

**The tools window.** Popped out, the panel is a window of its own (title Tools) that you
can put anywhere, a second screen included; it opens where it was last, at the size it was
(on a screen that is gone, centered). It works on the same budget: a change made in either
window shows in the other at once, the panel follows the grid's selection in the main
window, and a click on a noted line moves the main window's grid. Its header's Dock at right
and Dock at bottom put the panel back; closing the window (its close button, or Close Window
while it is in front) docks it at the right. Option+Cmd+S hides and shows it. Closing the
main window closes it too. Keys in the tools window: text fields keep their own editing keys
(Cmd+Z undoes typing there); the setup keys (Cmd+3...) open Budget Setup; Close Window docks it;
everything else (Save, Undo, Print, grid commands) goes to the main window, as the menu bar
does while the tools window is in front.

**The Budget Setup window** (decisions.md, entry 42). One window, titled Budget Setup, with
a tab per table: **Fringes, Groups, Globals, Units, Sets, Locations, Currency, Charges,
Credits, Info, Settings and Baseline**. Each tab is the full editor: the tag tables list
every column of an item's definition (a fringe's description, rate, unit and cutoff, a
group's color and whether it counts in the total, a location's city, region and country, a
currency's rate and symbol) with its line count (a click shows them in the tools panel's
Find tab) and its dollars across the budget; double-click, Enter or F2 edits an item in
its form, Delete deletes it, the row at the bottom adds one, and the right-click menu
renames it everywhere, sets a short code or saves it to the library. The fringe table ends
with the budget's percent and flat fringe dollars. Globals and Units are their full tables
(a global's details included); Charges, Credits, Info, Settings (renumbering accounts
included) and Baseline are the forms the tools panel's Budget tab used to hold. When the
window is at least 860 pixels wide, the setup library sits beside each table, on that
table's kind.

Open it with the toolbar's **Setup** button, View > Budget Setup, or a setup key (Cmd+3
Fringes, Cmd+4 Groups, Cmd+G Globals, Cmd+5 Locations, Cmd+6 Currency, Cmd+7 Units, Cmd+8
Sets, Cmd+9 Settings, Cmd+\\ Credits, Cmd+/ Charges), from any window; it opens on the tab
it showed last, or comes to the front on the tab asked for. The topsheet's right-click
**Create charge…** and **Create credit…** open it on that tab with the new item's name box
focused, and **Set up fringes…** on a line's menu opens it on Fringes. It works on the
budget in front (in split view, the target pane's) and follows when another tab comes to
the front; a change made in it shows in the other windows at once, and the other way
round. It opens where it was last, at the size it was. Escape closes it (unless a form, a
menu or a field being typed in has it), as do Close Window and Close Tab; the main window's
grid gets focus back. Keys in it: text fields keep their own editing keys, Left and Right
move along its tab strip, the setup keys show their tab, and everything else (Save, Undo,
Print) goes to the main window.

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

The tools window and the Notes tab use these commands, in `src/tools.rs` (the Budget Setup
window's two are in `src/setupwin.rs`):

| Command | Arguments | Returns |
|---|---|---|
| `tools_window` | `show` (true shows it, making it if needed, and brings it to the front; false hides it), `geometry` (`{x, y, width, height}` in logical pixels, or `null`: where it opens the first time) | nothing |
| `close_tools_window` | none | nothing |
| `setup_window` | `show`, `geometry` (as `tools_window`) | whether the Budget Setup window was already there (its page is listening; a new one says `setup-ready` once it is) |
| `close_setup_window` | none | nothing |
| `notes_list` | none | Every note in budget order: `[{ owner, id, kind, ti, ai, row_id, category, category_name, account, account_name, line, description, note, more, created, edited, author, done }]`; `kind` is `row`, `account` or `category` (`ai`, `row_id` and `line` are `null` when they don't apply); `note` is cut to 280 characters (`more` says so) |

Notes, in `src/notes.rs` (both windows may call them; each is one undo step and returns the
View; a refusal changes nothing). `owner` is `{ "row": id }`, `{ "account": "11-01" }` or
`{ "category": "11" }`; times are ISO 8601 text from the page (Rust never reads the clock
for them):

| Command | Arguments | Returns |
|---|---|---|
| `add_note` | `owner`, `text` (not blank, up to 10,000 characters), `created`, `author` (optional, up to 100 characters; blank is none) | View |
| `update_note` | `owner`, `id`, `text` (blank deletes the note; the same text changes nothing), `edited` | View |
| `delete_note` | `owner`, `id` | View |
| `set_note_done` | `owner`, `id`, `done` | View |

The setup lists' counts, order and short codes, in `src/setup_lists.rs` (both windows):

| Command | Arguments | Returns |
|---|---|---|
| `setup_usage` | none | `{ table: { name: { lines, other, total } } }` for `units`, `globals`, `fringes`, `groups`, `sets`, `locations` and `currencies`: lines that name the item, other setup that does, and its dollars in cents (`null` for globals and units, and when totals can't be worked out) |
| `set_setup_order` | `kind` (`fringe` or `fringes`, and so on), `names` (first to last; names the table doesn't have are skipped, the rest follow in name order) | View |
| `move_setup_item` | `kind`, `name`, `by` (negative is up) | View (quick repeats on one item are one undo step) |
| `set_short_code` | `kind` (`fringe` or `group`), `name`, `code` (blank clears) | View |

`delete_unit` and `delete_currency` take `clearLines` (default false): blank the unit on
the lines that name it, or put the currency's lines back in the base currency, before
deleting, as one step. `set_fringe`'s `fringe` and `set_group`'s and `update_group`'s
`group` take `short_code` (left out keeps the item's own; `""` clears it).

The Presets tab and the right-click submenus (`ui/js/presets.js`) use these, in
`src/presets.rs`; the main window and the tools window may both call them. Every one
that changes the budget is one undo step and returns the View; a refusal changes nothing.

| Command | Arguments | Returns |
|---|---|---|
| `save_tag_preset` | `index` (`null`: a new one at the end), `preset` (the file's shape, docs/FORMAT.md "Tag preset") | View |
| `tag_preset_from_line` | `rowId`, `name`, `parts` (`{fringes, groups, set, location, currency}` booleans, each true when left out) | View |
| `apply_tag_preset` | `index`, `mode` (`add` or `replace`), `rowIds`, `accounts`, `categories` (as for `apply_summary`; rows, accounts and categories must exist) | View |
| `save_line_preset` | `name`, `rowIds`, `keep` (`{formulas, units, tags, notes}` booleans, each true when left out) | View |
| `line_preset_preview` | `index` | the paste review's preview, listing only the setup items the block would add back (`review` false when there are none) |
| `insert_line_preset` | `index`, `account`, `beforeRow` (`null`: the account's end) | View, with `created_row` the first new row |
| `rename_tool_preset` | `which` (`tags` or `lines`), `index`, `name` | View |
| `delete_tool_preset` | `which`, `index` | View |
| `move_tool_preset` | `which`, `index`, `to` | View |

The lean view lists line blocks without their rows: `budget.line_presets` is
`[{ name, count }]` (the rows and their setup stay in Rust until a block is inserted).
Tag presets are in the view in the file's shape.

### History

The **History** tab (second tools pane; View > Tools Panel > History) lists every change to
the target budget that Undo can take back, oldest first (decisions.md, entry 40). Each step
is named by the command that made it, the way the grid names things: "Edit rate on 21-01
line 3", "Paste 12 rows", "Fill down 5 cells in 20-01", "Rename fringe FICA to Payroll",
"Delete 2 lines from 20-01", "Move 3 lines down", "Save version Approved", with the time it
was made (and the date, when it isn't today). The first row, **Start**, is the budget as it
was opened or created (or, past the 200 steps kept, the oldest state still kept). The
current step is highlighted; steps after it, the ones Redo would bring back, are fainter.
Click a step (or Enter on it) to undo or redo to just after it, as one change. **Version…**
on a step (shown on the current step and the one under the pointer or with focus; Right
from a step reaches it) asks for a name and saves the budget as it was at that step as a
version (File > Versions…) without changing the budget as it is now; the save is one undo
step, as any version save, so it drops the steps Redo would have brought back. Up and Down
move between steps, Home and End to the ends. In the tools window, a click or a version
save runs in the main window, as Undo does.

### Explain this number

**Explain This Number…** (first in the grid's right-click menu on any total, Edit >
Explain This Number…, or Cmd+E on the cursor cell) shows the tools panel's **Explain** tab
(first pane, after Find) with how that total is worked out (decisions.md, entry 43).
It works on every money cell: a line's total (its fringes from the Fringe $ or Fringe %
column), a subtotal row, an account's total, lines or fringes, a category and its fringe
account, a production level and the fringes posted above it, the summary rows (above and
below the line, total fringes, each charge, the grand total, each credit, the net total).
Total rows that the cursor can't land on are reached by right-clicking them; the Orig and
Var columns aren't explained (they are the baseline's numbers).

The tab shows the total's name and amount, then a tree of steps that add up to it exactly:
a sum adds the steps under it, a value is worked out from the plain-words lines under it,
and notes only explain. A line reads "Amount: SHOOT_DAYS + PREP_DAYS = 40 (PREP_DAYS = 10,
SHOOT_DAYS = 30) Days", "Line: 120 x 450 = 54,000.00", the currency conversion and every
rounding ("6,125.819 rounds half away from zero to 6,125.82"); a line left out says which
group or filter left it out and what it would be. Each fringe on a line shows percent or
flat (with the hours conversion), the cutoff and how much of it each line of a range used,
and its largest-remainder share; a charge its base, each excluded account with the rule
that caught it and the typed amounts; a credit each row's gross, excluded fringes, net,
percent and caps, the overall cap, the discount and the one rounding. Where fringes post
(the budget setting) is said wherever it decides what a total holds. "less" marks a step
taken off (credits under the net total).

A click on a step goes to its line, account or category; the arrow opens or closes it.
Keys: Up and Down move, Right opens (or goes into), Left closes (or goes up), Enter goes to
the step's place, Space opens or closes, Home and End. A big total arrives with its top
steps; deeper ones (and a note that points at another total, like a line's fringes or a
charge's base) are fetched when opened, so a grand total of thousands of lines sends a few
dozen steps. The explanation is worked out on demand only (never on an edit), from the
budget the totals on screen came from, and asked for again when the budget changes,
keeping what was open. With cells that have problems it says the totals, and it, count
them as blank. From a terminal, `method explain file.mthd <total>` prints the same tree (see
docs/CLI.md).

### Overtime helper

The **Overtime** tab (second pane, after History; also Edit > Overtime Helper… and the
line's right-click menu) is a small calculator for a labor line (decisions.md, entry
43). Every number is yours; no union or guild rules are built in. Type the
straight-time rate an hour, the hours worked a day, the straight-time hours, then the
overtime bands ("then 4 hours at 1.5x", the last one "then the rest at 2x"; Add band,
Remove), the sixth and seventh day multipliers (blank: a regular day; on such a day every
hour is paid at least that multiple, a band above it still wins) and the days worked a
week. As you type it shows a day (hours at each multiple and their pay, the day's pay, its
straight-time equivalent hours, the average an hour) and a week, every amount exact.

**Write into line** (the cursor line, detail level) puts the day's or the week's pay (Per
day or Per week) into the line **as the rate**, exactly, so the line rounds once as every
line does, or **as split lines**: one line per multiple (straight time, then each
overtime rate), each keeping the line's amount, units, X, 4X and tags with that part of
the pay as its rate, the first keeping the line's id and notes, all in one fringe range
(the line's own, else a new one) so percent fringe cutoffs still treat them as one person.
Either way it is one undo step. **Pattern** picks a saved pattern to fill the form;
**Save as…** saves the form's straight time, bands and day multipliers in the budget by
name, **Update** saves into the picked one and **Delete** removes it (one undo step each).
Patterns are tools, not content, like tag presets: versions don't carry them.

### Find and replace

Edit > Find (Cmd+F) shows the tools panel's **Find** tab, wherever the panel is, with focus
in its search box; Edit > Find and Replace (Shift+Cmd+F) shows it in replace mode (the
**Replace…** switch in the tab does the same). A search looks for one kind of thing, picked
in the menu above the box:

- **Description**, **Formula text** (Amount, X, Rate and 4X as typed), **Notes** and
  **Account name or number**: text, with **Match case** and **Whole word** (the text must
  not run on into letters, digits or `_`).
- **Global in formulas**: a global used in Amount, X, Rate or 4X, matched as a whole name
  the way formulas are read, so `DAYS` never matches inside `SHOOT_DAYS` or `DAYS2`.
- **Fringe**, **Group**, **Set**, **Location**, **Unit** (any of the four unit columns, each
  its own result) and **Currency** on lines, by name. The base currency finds the lines
  left blank, which use it.

For globals and setup items the box offers the names as you type (Up and Down, Enter or
Tab takes one, Escape closes the list; Enter on part of a name takes the first one offered),
and capitals don't matter. Text searches run as you type. Results are grouped by category
and account, with the account's count; each shows the line number and the column that
matched, the line's description and what the cell holds. A click on a result opens its
account with that cell selected (an account's name or number: the category's account list,
on that account). Enter and Shift+Enter in the search box go to the next and previous
result, keeping focus in the box; F3 and Shift+F3 do the same from anywhere (Cmd+G is the
Globals tab), opening the Find tab when there is nothing to find yet. The count says how
many places match; at most 1,000 are listed (the count says so), and the list is searched
again after every change to the budget, so places that no longer match drop out.

**Replacing.** In replace mode every result has a checkbox, all checked to begin with, with
one per account and **All results** above (a dash when some are checked). **Replace**
changes the checked results only, all as one undo step, and says how many places changed
(`4 updated`, until the budget changes again). Text is replaced inside the field (the
matches only, in every note on the line; a note left empty is removed; notes on accounts and categories are not searched). A global is replaced, only as that name, by
another global, a number or a formula (a formula goes in parentheses inside a longer
formula: `(WEEKS * 5) / DAYS`; a cell that is only the global takes it as typed). A fringe,
group, set, location, unit or currency is replaced by another one that exists (the Replace
box offers them). **Remove** takes the item off the checked lines (a currency goes back to
the base currency), or puts a global's current value in its place, as deleting a global
does (decisions.md, entry 12). Account numbers are found but not replaced (they change in
the account list). A replace is refused, with nothing changed and the reason shown, when a
checked place no longer matches (the budget changed meanwhile), the new global or item
doesn't exist, the formula can't be read, or nothing would change. Replacing swaps one item
for another in the places checked; to give the global or item itself a new name everywhere,
the replace panel has **Rename everywhere…** for the one being searched for (see Renaming an
item everywhere, above), and the search follows the new name.

Popped out, the Find tab works in the tools window: its results move the main window's
grid, and Cmd+F and F3 in the main window go to it.

The commands live in `src/find.rs`:

| Command | Arguments | Returns |
|---|---|---|
| `find` | `query` (`{ scope, text, match_case, whole_word }`; scope `description`, `formula`, `global`, `fringe`, `group`, `set`, `location`, `unit`, `currency`, `note` or `account`) | `{ count, hits, truncated }`: `hits` are `[{ ti, ai, category, category_name, account, account_name, row_id, line, description, field, text }]` in budget order, at most 1,000; `row_id` is `null` for an account's own `code` or `name`; `description` and `text` are cut to 160 characters |
| `replace` | `query` (as above), `targets` (`[{ row_id, account, field }]`, the checked results), `with` (the replacement; `null` removes) | View, with `updated` (how many places changed) |

A search only reads and takes well under a millisecond on the 5,000-row sample budget
(`examples/perf.rs` prints it); `find` is answered from the budget as it is, so it never
waits for totals.

### Keyboard and menus

Everything has a key, and the menu bar (File, Edit, Insert, Format, Go, View, Window, Help) lists the same
commands with the same keys. Help > Keyboard Shortcuts (Shift+Cmd+/ or Ctrl+Shift+/) shows the full
list for your platform. The list lives in one place, `ui/js/shortcuts.js`; the menu bar
(`src/menu.rs`) uses the same ids and keys, and a Rust test fails if they drift apart.

| Action | macOS | Windows and Linux |
|---|---|---|
| New budget (in a new tab), New tab, Open (in a new tab) | Cmd+N, Cmd+T, Cmd+O | Ctrl+N, Ctrl+T, Ctrl+O |
| Save, Save As | Cmd+S, Shift+Cmd+S | Ctrl+S, Ctrl+Shift+S |
| Print (the report setup, with its preview, presets and Save as PDF), Print Topsheet | Cmd+P, Shift+Cmd+P | Ctrl+P, Ctrl+Shift+P |
| Close the budget's tab (the window, on the last tab), Close the window, Quit | Cmd+W, Shift+Cmd+W, Cmd+Q | Ctrl+W, Ctrl+Shift+W, Ctrl+Q |
| Next, previous budget tab | Shift+Cmd+], Shift+Cmd+[ (or Ctrl+Tab, Ctrl+Shift+Tab) | Ctrl+Shift+], Ctrl+Shift+[ (or Ctrl+Tab, Ctrl+Shift+Tab) |
| First, second budget tab | Cmd+1, Cmd+2 | Ctrl+1, Ctrl+2 |
| On a budget tab: the previous or next tab; move the tab | Left, Right (also Home, End); Option+Shift+Left, Right | Left, Right (also Home, End); Alt+Shift+Left, Right |
| On a budget tab: its menu (close, move to the other pane, split) | Shift+F10 or the menu key (or a right-click) | Shift+F10 or the menu key (or a right-click) |
| Split Right (split already: go to the other pane) | Shift+Cmd+\\ | Ctrl+Shift+\\ |
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
| Add a note to the line, account or category | Shift+Cmd+Space | Ctrl+Shift+Space |
| Bold, italic on or off for the selected lines | Cmd+B, Shift+Cmd+I | Ctrl+B, Ctrl+Shift+I |
| Indent the selected lines more, less | Option+Cmd+Right, Option+Cmd+Left | Ctrl+Alt+Right, Ctrl+Alt+Left |
| Open the right-click menu at the cursor cell | Shift+F10 (or Control-click, or the menu key) | Shift+F10 (or the menu key) |
| Make the selected lines one fringe range; take them out of their range | Cmd+R; Shift+Cmd+R | Ctrl+R; Ctrl+Shift+R |
| Explain the total under the cursor (the Explain tab) | Cmd+E | Ctrl+E |
| Move the selected lines, accounts or topsheet rows up, down one place | Option+Up, Option+Down | Alt+Up, Alt+Down |
| Up a level; down a level (also Enter) | Cmd+Up; Cmd+Down | Ctrl+Up; Ctrl+Down |
| Go to the topsheet | Shift+Cmd+Up | Ctrl+Shift+Up |
| Next, previous account (across categories, wrapping; also the ‹ › buttons; not while editing a cell, where they move the caret) | Cmd+Right, Cmd+Left (or Cmd+], Cmd+[) | Ctrl+Right, Ctrl+Left (or Ctrl+], Ctrl+[) |
| In a type-ahead list: move, take the name, close it | Up, Down; Enter or Tab; Escape | Up, Down; Enter or Tab; Escape |
| Open a breadcrumb jump list or the column menu (on its button) | Down or Enter | Down or Enter |
| The column menu button | Shift+Tab from the first cell | Shift+Tab from the first cell |
| Show or hide the tools panel (docked or its window) | Option+Cmd+S | Ctrl+Alt+S |
| Selected cells show formulas instead of values, or not | Option+Cmd+F | Ctrl+Alt+F |
| Zoom the grid in, out, actual size | Option+Cmd+=, Option+Cmd+minus, Option+Cmd+0 | Ctrl+Alt+=, Ctrl+Alt+minus, Ctrl+Alt+0 |
| Resize the docked tools panel (on its edge; Shift for bigger steps): wider or narrower at the right, taller or shorter at the bottom | Left, Right; Up, Down | Left, Right; Up, Down |
| Budget Setup: Globals, Fringes, Groups, Locations | Cmd+G, Cmd+3, Cmd+4, Cmd+5 | Ctrl+G, Ctrl+3, Ctrl+4, Ctrl+5 |
| Budget Setup: Currency, Units, Sets, Settings | Cmd+6, Cmd+7, Cmd+8, Cmd+9 | Ctrl+6, Ctrl+7, Ctrl+8, Ctrl+9 |
| Budget Setup: Credits, Charges | Cmd+\\, Cmd+/ | Ctrl+\\, Ctrl+/ |
| In Budget Setup: close it (focus goes back to the grid) | Escape, Cmd+W | Escape, Ctrl+W |
| On a Budget Setup tab: the previous or next tab (also Home, End) | Left, Right | Left, Right |
| On a tools tab: the previous or next tab in its pane (also Home, End) | Left, Right | Left, Right |
| On a tools tab: move it along its pane; to the first or second pane | Option+Shift+Left, Right; Option+Shift+Up, Down | Alt+Shift+Left, Right; Alt+Shift+Up, Down |
| On a list item: apply it to the selection; edit it; delete it; move up or down | Space; Enter or F2; Delete; Up, Down | Space; Enter or F2; Delete; Up, Down |
| On a global's or unit's name: rename it everywhere | Enter or F2 (or double-click) | Enter or F2 (or double-click) |
| In the Notes tab: add or save a note; put back the saved text | Cmd+Enter; Escape | Ctrl+Enter; Escape |
| In a setup list: move the item up or down in the budget's order | Option+Shift+Up / Down | Alt+Shift+Up / Down |
| Find (the tools panel's Find tab); Find and Replace | Cmd+F; Shift+Cmd+F | Ctrl+F; Ctrl+Shift+F |
| Next, previous find result (from anywhere); in the Find box | F3, Shift+F3; Enter, Shift+Enter | F3, Shift+F3; Enter, Shift+Enter |
| Keyboard shortcuts | Shift+Cmd+/ | Ctrl+Shift+/ |

The row keys (Cmd+I insert, Cmd+D delete, Cmd+= subtotal, Cmd+minus adjustment, Cmd+R
fringe range, Shift+Cmd+Space note) and the setup keys (Cmd+G globals, Cmd+3 to Cmd+9,
Cmd+\\ credits, Cmd+/ charges) follow the habits of people coming from other budgeting
tools, so the shortcut sheet moved to Shift+Cmd+/ and Cmd+1 and Cmd+2 have no setup tab;
fill down is on Shift+Cmd+D instead of the spreadsheet Cmd+D. Insert > Account, Category and Production Total have no key; they go to the level they
belong to first and open the same dialog as the buttons. Insert > Separator has no key
either; it adds a separator row below the cursor on the detail grid. Budget Setup and its
tabs are in View > Budget Setup (Info and Baseline without a key); the tools panel's own
tabs, without keys, and where the panel goes are in View > Tools Panel. Cmd+I inserts a line, so italic is Shift+Cmd+I;
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
The budget tab keys (next, previous, first, second) are grid commands that also work from a
text field outside the grid, since they mean nothing there, and Ctrl+Tab passes through the
cell editor and the type-ahead list (the edit is committed first). Close Tab from the menu
closes the tools window when that is in front, as Close Window does.

Closing the window or quitting asks once about every budget with unsaved changes: with
Autosave on, changes go into their files first, so the question lists only what is left
(untitled budgets, Autosave off, a file changed outside the app). It is a native dialog
naming each budget, with **Save All** (the default: each budget into its file, an untitled
one through its own Save dialog; a cancelled dialog or a failed save stops it and nothing
closes), **Discard All** (no recovery copies either) and **Cancel**. With one budget it
reads Save, Discard, Cancel.

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
the page, so a paste reaches any budget open in the app: rows copied in one tab paste into
another, with the review against that budget's setup. The clipboard text is the key: a paste whose text isn't the rows' text (any copy
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

### Starting from another budget

**Import Setup From…** (File menu) brings setup items from another budget file into the
budget in front (decisions.md, entry 29). Rust shows the Open dialog and reads the file; a
file that is open in a tab is read from the tab, so its unsaved changes count, and the
budget in front can't import from itself. A review then lists every item in that budget,
grouped by kind: globals, fringes, groups, units, sets, locations, currencies, charges,
credits, tag presets, line blocks, report presets, and the budget settings (account
numbering, fringe posting, new-line defaults). Each says how it stands here: **New**,
**Same here**, or **Different here** with both definitions in words. Tick what to bring;
a kind's own box ticks all of it. What a ticked item needs comes with it and is shown
ticked and locked, saying what it comes with: a fringe's flat unit and the globals in its
rate, the globals a global names (however deep), a charge's excluded groups and currency,
a credit's groups and excluded fringes, a tag preset's tags, what a line block's rows use,
a report preset's groups. For each item that differs and is coming in, the choice is the
paste review's (decision 12): **Keep this budget's version** (the default), **Use the
imported version** (it replaces this budget's, for everything that uses it), or **Add the
imported one as** a new name (`OT (2)`, globals `DAYS_2`), which the imported items that
refer to it then use. Currencies and settings can't take a new name; a setting that
differs comes in only with "Use the imported version". A replaced charge, credit or preset
keeps its place in the list, and a replaced report preset stays the default if it was; an
added report preset is never the default. A currency that is this budget's base currency
isn't imported (flat charges in the other budget's base currency count in this one's, and
the review says so). Import is one undo step and says how many items were added,
replaced and kept. Nothing about lines, accounts or categories is imported (copy and paste
rows does that).

**New From Template…** (File menu) starts a new budget in a new tab from any budget file:
its categories, accounts and production totals, its setup (globals, fringes, groups,
units, sets, locations, currencies, charges, credits, tag, line and report presets) and its
settings (account numbering, fringe posting, new-line defaults, base and totals
currencies). Its detail lines, budget info, baseline, versions and line notes are left
out; "Keep the detail lines" keeps the lines (without their notes). The new budget is
called Untitled and has no file, so the first Save asks where; the budget it came from is
not touched (an open one is read from its tab). **Save as Template…** writes the budget in
front to a new file, stripped the same way and titled after the file, without changing the
budget or its own file; it won't write over a file open in a tab. A template is an
ordinary `.mthd`: there is no template format and no template folder.

| Command | Arguments | Returns |
|---|---|---|
| `import_setup_pick` | none (shows the Open dialog) | the review, or `null` when cancelled |
| `import_setup` | `token` (from the review), `picks` (`[{kind, name}]`, settings with an empty name), `resolutions` (`[{kind, name, choice}]`, choice `keep`, `imported` or `rename`) | `{view, message, added, replaced, kept}` |
| `template_new` | `keepLines` (shows the Open dialog) | View of the new tab, or `null` when cancelled |
| `template_save` | `keepLines` (shows the Save dialog) | `{name, path}`, or `null` when cancelled |

The review has `token`, `source` (the file's or tab's name), `path`, `notes` and `items`
(`[{kind, name, status, imported, here, rename_to, needs}]`: kind `global`, `fringe`,
`group`, `unit`, `set`, `location`, `currency`, `charge`, `credit`, `tag_preset`,
`line_preset`, `report_preset`, `account_coding`, `fringe_posting` or `new_line_defaults`;
status `new`, `same` or `differs`; `needs` every item it brings, however deep). Only the
last file picked is kept; `import_setup` is refused, nothing changed, when another file was
picked since, a pick isn't in that budget, a choice is unknown, or the import would pass a
budget's limit on presets.

### Sharing copies and sub-budgets

**Save a Copy for Sharing…** (File menu; roadmap 2.21, decisions.md entry 39) writes a
snapshot for someone else: the budget as it is, or one of its saved versions (with the
current presets and display settings). Tick what to leave out: notes (on lines, accounts,
categories and in line blocks), budget info fields, saved versions, the original totals,
and setup items nothing uses (removed until none is left; what tag and report presets
name stays, so no total moves). An optional note goes with it. Rust's Save dialog
suggests `<title> - <version> - <date>.mthd` in the budget's folder; a file open in a tab is
refused. The copy carries a `provenance` (docs/FORMAT.md): shared copy, the source title,
the version's name, when it was saved, the app version and the note. The budget and its
own file don't change.

**A shared copy opens read-only.** A banner under the breadcrumb says where it came from
(and its note), and every edit is refused with one message (Rust refuses it in the
session, whatever sends it); printing, exporting, finding and comparing work as usual.
**Make Editable Copy** opens the same budget in a new tab with no file, stamped as an
editable copy. When the budget a shared or editable copy came from is open (matched by
title, trimmed and ignoring case: the copy carries no path), the first time the copy comes
to the front the app asks: **Compare With Original** (the compare screen, with the
original as the base and the copy ticked) or **Store as a Version…** (the copy's content
saved as a new version of the original, named in a small form, its note saying when the
copy was made; one undo step in the original), or Not Now. The shared copy's banner keeps
both buttons while the original is open.

**New Sub-Budget…** (File menu; roadmap 2.22) opens a filtered copy of the budget in front
in a new tab: tick categories (all their accounts come along, ticked and locked) and
single accounts; nothing ticked keeps every category. Then groups, sets and locations: a
line is kept when it has any ticked value of each kind that has a tick, or "Lines with no
group" (set, location) for lines without one. Subtotal and separator rows stay while
their account keeps a line. Options: leave out accounts with no lines left, take the
fringes off every line (the fringe table stays; totals change), leave out notes. Versions
and the baseline stay behind. The sub-budget is titled after its parent ("Big Show (sub-budget)") and has no
file, like a template, and is stamped with its parent's title and a line saying what it keeps. Totals
are worked out normally: there is no fringe lock yet.

| Command | Arguments | Returns |
|---|---|---|
| `share_copy_save` | `version` (id or `null`), `leaveOut` (`{notes, info, versions, baseline, unused_setup}`), `note`, `saved` (the page's local time, ISO 8601); shows the Save dialog | `{name, path}`, or `null` when cancelled |
| `share_copy_editable` | none | View of the new tab |
| `share_copy_original` | none | `{doc, label}` of the open budget the copy came from, or `null` |
| `share_copy_store` | `original` (its doc id), `name`, `created` | View of the original (one undo step there) |
| `sub_budget_new` | `spec` (`{categories, accounts, groups, sets, locations, skip_empty_accounts, drop_fringes, drop_notes}`, each tag kind `{values, include_untagged}`), `saved` | View of the new tab |

### Setup library

Your own library of setup items to reuse across budgets (roadmap 2.15, decisions.md entries
15 and 37): the local-first equivalent of a cloud setup library. **The library is a budget
file**: our ordinary `.mthd` format with setup and no lines, plus folders per kind
(`setup_folders`, FORMAT.md). It starts empty (no units, fringes or anything else: we ship
no library data) and opens in the app like any other budget, so it can live in a synced
folder or a git repository of your choosing.

**The Library tab** (in the tools panel's second pane by default; draggable like any tab;
View > Tools Panel > Library). With no library yet it explains itself and offers **Create
Library…** (Rust asks for a folder and makes `My Method Budget Library.mthd` there, or uses
the one already there, under its current or its pre-rename name) and **Use Existing
Library…** (any budget file, in Rust's Open dialog).
The choice is remembered for you in `library.json` in the app's data folder; **Change…**
and **New…** switch libraries at any time. Then the tab shows the library's name, a kind
picker (globals, fringes, groups, units, sets, locations, currencies, charges, credits, tag
presets, line blocks, report presets, each with its count), a folder picker (All folders,
Unfiled, then the kind's folders) and a filter on names and definitions. Each item shows
its definition in words (its tooltip adds what it brings along), a tick box, and its folder
when All folders is shown.

- **Add to This Budget…** copies the ticked items (or the item double-clicked, pressed Enter
  on, or right-clicked) into the budget in front, after Import Setup's review: what each
  item needs comes along (a fringe's unit and its rate's globals, a global's globals, a
  charge's groups...), each item says **New**, **Same here** or **Different here**, and an
  item that differs keeps this budget's version unless you pick **Use the library's
  version** or **Add the library's as** a new name. One undo step. The three budget settings
  are never library items.
- **Save to Library…** (at the foot of the tab, for any of this budget's setup items, ticked
  in the review, several at once) and **Save to Library…** in a setup list's right-click
  menu (Fringes, Groups, Globals, Units, Sets, Locations, Currency: that item and what it
  needs, ticked) copy the other way, after the same review turned round: an item the
  library has set up differently keeps **the library's version** by default, or **Replace
  it with this budget's**, or **Add this budget's as** a new name. New items land in no
  folder. The library file is written at once (not an undo step of the budget).
- **Folders**: New Folder…, Rename Folder… and Delete Folder… (the folder shown; its items
  stay, unfiled), and **Move To…** (or Move To in an item's right-click menu) for the ticked
  items. Folder names are unique per kind ignoring case, up to 60 characters.

**No live link.** Every copy is one-time: changing an item in the library later doesn't
change budgets it was copied into, or the other way round. **Writes** read the library
file, change it and write it back through a temporary file renamed into place. **Open as a
tab**: when the library file is also open as a budget tab, the Library tab reads it from
there (its unsaved changes count) and adding from it works, but every change to it from the
Library tab (saving to it, folders, moving) is refused with a message saying to make the
change in that tab or close it first, so one file never has two writers; adding into the
library's own tab from the Library tab is refused too. A library that has been moved or
can't be read says so, with Change… and New… to fix it.

| Command | Arguments | Returns |
|---|---|---|
| `library_view` | none | the library (below) |
| `library_create` | none (shows Rust's folder dialog) | `{library, message}`, or `null` when cancelled |
| `library_choose` | none (shows Rust's Open dialog) | `{library, message}`, or `null` when cancelled |
| `library_add_preview` | `picks` (`[{kind, name}]`) | Import Setup's review (`token`, `source`, `items`, `notes`) plus `picked` |
| `library_add` | `token`, `picks`, `resolutions` (as `import_setup`) | `{view, message}`, one undo step |
| `library_save_preview` | `picks` (`[{kind, name}]`, or `null` for every item) | the review plus `picked` |
| `library_save` | `token`, `picks`, `resolutions` | `{library, message}` |
| `library_folder` | `kind`, `action` (`create`, `rename`, `delete`), `name`, `to` | `{library, message}` |
| `library_move` | `kind`, `names`, `folder` (`null`: unfiled) | `{library, message}` |

The library is `{configured, name, location, problem, in_tab, kinds}`, `kinds` being
`[{kind, folders, items: [{name, folder, definition, needs}]}]`. A review's token names
the library as the review saw it; adding or saving with it is refused ("The library changed
since") once the library has been written since. The Library tab works in the tools window
too (its commands are in `capabilities/tools.json`); the dialogs are Rust's, parented to
the window that asked.

### Versions and comparing

**Versions** (File > Versions…, or Versions in the toolbar) are named copies of the
budget kept inside its file (decisions.md, entries 17 and 27). Save Version takes a name
(one line, up to 200 characters; it suggests `Version N`) and an optional note, and stamps
the page's local time. The list, newest first, shows each version's name, when it was
saved and its note, with **Compare** (the compare screen with just that version),
**Restore…**, **Rename** (in place: Enter saves, Escape cancels) and **Delete…**. Restore
and Delete ask on the version's own row. Restoring saves the budget as it is first, as
`Before restoring "<name>"`, then puts the version in its place; every one of these is one
undo step, so Undo takes back a whole restore (and its automatic version) at once.

**Compare** (File > Compare Budgets…, or Compare in the toolbar) puts the budget in front
side by side with 1 to 15 others. At the left, tick what to compare with: open budgets
(tabs), versions of this budget, or files (Add Files… shows the Open dialog; a file that is
open in a tab is added as that tab, so its unsaved changes count). The letters A, B, C...
follow the order they were ticked. The comparison lists categories (▸ opens one to its
accounts, and an account to its lines), each production total, above and below the line,
all fringes, charges (each charge on its own line under their total, matched by name), the
grand total, credits (the same) and the net total. Columns: each budget's
total, its difference from this one and that as a percent of this one (Differences), and,
ticked in the header, **Sum** and **Average** of the other budgets, **Variance** (their
sum minus this one) and **Variance %** (of the average), as the reference product reads a
comparison. A row missing from some budgets says `not in B, C`, and its missing figures say
`none`. Lines match as the same line across versions of one budget, by description
otherwise. A budget with problem cells is compared as its own grid totals it (the cells
count as blank) and its header is marked. A click on a figure goes to that row in that
budget when it is open in a tab; a click on a row's name goes to it in this budget (or the
first open one that has it). Opening the comparison again brings back the same budgets.
Column choices are kept per user. **Print…** and **Save as PDF…** lay the comparison out
as shown (the columns ticked, the rows open) on landscape pages, with a legend of which
budget each letter is, through the report module's `printHtml`.

| Command | Arguments | Returns |
|---|---|---|
| `save_version` | `name`, `note` (optional), `created` (ISO 8601 local time) | View |
| `restore_version` | `id`, `created` | View |
| `rename_version` | `id`, `name` | View |
| `delete_version` | `id` | View |
| `compare_budgets` | `sources` (`[{kind: "tab", doc} \| {kind: "version", id} \| {kind: "file", token}]`, 1 to 15) | the comparison |
| `compare_detail` | `sources`, `code` (an account) | one account's lines compared |
| `compare_pick_files` | none (shows the Open dialog) | `[{source, label, path}]`, empty when cancelled |

The comparison has `sources` (`{kind, label, doc, version, token, detail, partial}`, this
budget first), `categories` (each with `accounts`), `levels`, `charges` and `credits`
(`{name, line}` each, matched by name, trimmed and ignoring case) and `totals`; every figure is
a `line`: `values` (cents, `null` where a budget lacks it), `diffs` (`{amount, percent}`
against this budget, percents as text with two decimals) and `agg` (`{sum, average,
variance, percent}`). Categories, accounts and lines carry where they are in each budget
(`at`, `ids`). The selection is refused, nothing compared, when it is empty, has more than
15, names a budget twice or this budget itself, or names a closed tab, a deleted version or
a file no longer at hand.

### What changed and merging

**File > What Changed…** lists every change from another budget to the one in front, in
plain words (roadmap 3.1; docs/MERGE.md; decisions.md, entry 44). At the left, pick what to
compare with: this budget's **file as last saved** (with autosave on that is a few
seconds old; a version or another file says more), one of its **versions**, another
**open budget**, or a **file** (Choose a File… shows the Open dialog; a file open in a tab
comes back as that tab). At the right: the topsheet totals that moved (before, after,
change), then every change grouped under Budget (settings), each setup table, Topsheet and
account by account (`21-01 line 3: rate 1,200.00 -> 1,350.00 (+1,500.00 to 21-01)`), then
the categories and accounts whose totals moved. Rows match by id, accounts and categories
by number (a renumbered one is found by what it holds), setup items by name. A click on a
change goes there in this budget: a row's cell, an account in its category's list, a
category on the topsheet, a setup tab in Budget Setup, the Versions dialog; a row that went
goes to its account. Budgets with problem cells are compared as their grids total them and
say so. The choice is kept per budget for the session.

**File > Merge Changes From…** merges a returned copy into the budget in front (roadmap
3.2). At the left, pick the **returned copy** (an open budget or a file) and the **base**,
the budget both started from: **From the copy** finds the version of this budget the copy
was shared from (its provenance names it), worked out as it was sent, so notes, info,
the baseline or unused setup the copy left out aren't read as deleted; or pick a version,
an open budget or a file (the copy as it was sent, when it was shared from the budget as it
stood). At the right: every **conflict** (the same field changed both ways, something
deleted on one side and changed on the other, an account or category deleted on one side
and changed on the other, both sides reordering one list) with what it was and **Ours**
and **Theirs** to pick, ours to begin with; then what the merge brings into this budget,
grouped like What Changed, and the topsheet totals it moves. Each pick works the merge out
again. **Merge** applies it as one undo step (`Merge changes from <copy>`), worked out on
the budget as it is then; it is refused on a read-only shared copy like any edit. Rows
both sides added with one id are kept as two (the copy's gets a new id) and the preview
says so; a merged budget that can't be totalled though both sides can says why.

| Command | Arguments | Returns |
|---|---|---|
| `what_changed` | `against` (`{kind: "saved"} \| {kind: "version", id} \| {kind: "tab", doc} \| {kind: "file", token}`) | `{from, to, changes, accounts, categories, totals, money_known}` |
| `merge_preview` | `copy` (`{kind: "tab", doc} \| {kind: "file", token}`), `base` (`{kind: "auto"} \| {kind: "version", id} \| {kind: "tab", doc} \| {kind: "file", token}`), `picks` (`{key: "ours" \| "theirs"}`) | `{copy, base, needs_base, as_sent, conflicts, changes, totals, renumbered, problem}` |
| `merge_apply` | `copy`, `base`, `picks` | View |
| `merge_pick_file` | `purpose` (`"diff"`, `"copy"` or `"base"`: the dialog's title) | `{source, label, path}`, `null` when cancelled |

A change is `{kind, group, group_kind, subject, text, money, fields, go}`; `go` is where a
click goes (`{kind: "account" | "category" | "setup" | "versions", ti, ai, category,
account, row_id, field, tab}`). A conflict is `{key, group, what, base, ours, theirs,
taken, text}`. Files are read in Rust and named by token, kept with the compare screen's
(the last 32). `needs_base` says why no base was found (the copy has no provenance, came
from another budget, was saved from the budget as it stood, or its version is gone) and
the page asks for one.

### Printing and reports

**File > Print… (Cmd+P)** opens the report setup: the choices on the left, the report on
the right exactly as it will print, page by page, redrawn as the choices change. Above it:
first, previous, next and last page buttons around the page shown (type a number to go
there; it follows scrolling), the page count, and a Go to section list. **File > Print Topsheet… (Shift+Cmd+P)**
still prints the topsheet straight away, with the standard layout.

- **Sections, in print order**: topsheet, account level (each category's accounts with
  fringes and totals), detail level (each category starts a page), fringes (type, rate,
  cutoff or unit, how many lines use it, what it adds up to), globals (formula, value, how
  many lines use it), groups with totals (in the budget total or not, lines, lines total,
  fringes, total), notes list, budget information, chart of accounts (every category and
  account code with its name, account and row counts, production totals where they fall),
  units (plural, description, hours, rows using it), sets and locations with totals (rows,
  lines total, fringes, total of the lines in the budget total) and currencies (the base
  first, then each with its symbol and rate, rows in it). A checkbox prints a section, the
  arrows move it.
- **Categories and accounts**: a tree with a checkbox for every category (its accounts
  under the ▸) and an All box. It decides the account level, the detail level and the
  notes list; the topsheet always lists every category, so its totals add up.
- **Leave out**: accounts with no lines, accounts that total zero, lines not in the budget
  total, the notes under detail lines, and (when the budget has groups) every line not in
  the checked groups. Totals stay the budget's own; an account with rows left out says
  how many under its totals, and a category with nothing left to print is skipped.
- **Topsheet columns**: Fringes (what each category's lines earn, wherever they post; a
  production total sums its categories) and Page its detail starts on (filled in from the
  laid-out pages; blank for a category whose detail doesn't print).
- **Detail columns**: the grid's columns (amount, unit, X and its unit, rate and its unit,
  currency, 4X and its unit, fringes, fringe %, fringe $, groups, set, location) plus
  Original and Variance when the budget has a baseline (those print on the topsheet and
  account level too). #, description and total always print. Standard columns puts the
  usual set back. Money columns keep their width; the others give way to the description
  when the page is full.
- **Layout**: portrait or landscape (Letter), type size small, medium or large, shading
  (alternate lines tinted, totals darker), lines in their group colors (a line's own
  color first, as in the grid; otherwise black), lines between columns, and the detail
  level as one line per account (code, name, total, original and variance; the
  categories then run on instead of each starting a page, and line notes don't print).
- **Header and footer**: three slots each (left, center, right), each nothing, the budget
  title, the section, the date and time printed, the date, the time, the file name, Page
  X of Y, a budget information field (Prep weeks, Shoot days, ...), a global's value
  (`SHOOT_DAYS: 25`; renaming the global follows) or text you type. Each band can print
  on the first page only (later pages then use the room). The standard ones: title,
  section and date printed above, the page number below.

**Notes print.** A line with notes gets a small number after its description on the detail
pages, and its notes follow the account's totals with the same number, each with its date
and author (cut at 700 characters together there; printed whole in the notes list). The
notes list prints every note on the chosen categories and accounts: each category's own
notes, each account's own, then its lines' (with line number, line and the same number
when the detail pages print it), each with its date, author and whether it is reviewed.
**Leave out > Notes ticked off as reviewed** (`skip_done_notes`) leaves the reviewed ones
out of both. Printed setup tables (fringes, globals, groups, sets, locations, currencies)
follow the budget's own order.

**Presets.** The setup can be kept in the budget as a named preset (the Preset list at the
top): Save Preset saves it (in place, so typing a new name and saving renames it), Save as
New keeps a copy under a new name, Delete removes it (Edit > Undo brings it back), and
Default marks the one Print opens with (one at a time). Presets hold choices, never
figures: a report is always drawn from the budget as it is. Each change is one undo step
and marks the budget edited. Versions don't carry presets, and restoring a version keeps
the current ones.

**Print… and Save as PDF….** Print… opens the system print sheet with the orientation
already set. Save as PDF… asks where to save and writes the PDF straight there, with no
print sheet. Both print exactly the pages in the preview: the report is laid out by the app
into Letter-sized pages, each with its own header, footer and page number, because the
system web view repeats no table headings and prints no page numbers by itself (decisions
entry 28).

| Command | Arguments | Returns |
|---|---|---|
| `save_report_preset` | `index` (the preset to replace, or `null` to add one), `preset` (FORMAT.md, "Report preset") | View |
| `delete_report_preset` | `index` | View |
| `print_report` | `pdf` (true: into a PDF file chosen in a Save dialog), `landscape`, `title` (the job's name and the suggested file name) | `{supported, done, file}`: `supported` false where the app prints with `window.print()` instead (not macOS); `done` false when cancelled; `file` the PDF's file name |

For other screens, `ui/js/report.js` lays out and prints any table the same way:
`renderHtml(container, table, opts)` and `printHtml(table, opts)` (`opts`: `title`,
`section`, `landscape`, `fontSize`, `header`, `footer`, `pdf`) split a table's rows over
pages, repeating its header row on each, with the standard header and footer.
`renderReport(container, view, spec)` and `printReport(view, spec, {pdf})` take a report
spec (a preset's fields) or the older `"topsheet"`, `"detail"` or `"both"`.

### Exporting spreadsheets

**File > Export** has three items, each asking where to save with a native Save dialog
(in the budget's folder, named after its file or, untitled, its title):

- **Excel Workbook…** writes one `.xlsx` with a sheet per table: Topsheet, Accounts,
  Detail, Globals, Fringes, Groups, Units, Currencies, Sets and Locations.
- **CSV Files…** writes the same tables as one CSV file each, named from the name chosen:
  `Pilot.csv` gives `Pilot - Topsheet.csv`, `Pilot - Accounts.csv` and so on, beside it.
  They start with a UTF-8 byte order mark, so Excel reads accented letters.
- **Tab-Delimited Detail…** writes the Detail table alone as tab-delimited text (`.txt`,
  no byte order mark), the shape accounting and payroll importers take.

What the tables hold:

- **Topsheet**: type, code, description and total for each category in topsheet order,
  posted fringe rows, production totals, the above-, below- and above-and-below-the-line
  totals, budget-wide fringes, each applied charge (with its account), the grand total,
  each applied credit (as a negative), the net total, and the totals in the totals currency
  when one is set.
- **Accounts**: category code and name, account code and name, details total, fringe total
  and total for every account; a category's fringe account (fringes posted by category)
  follows its accounts. The Total column adds up to the categories' totals.
- **Detail**: one row per row of every account (lines, subtotals, adjustments and
  separators, with the kind), in budget order: category, account and its name, line
  number, description, the amount and rate as typed and their values, unit, X, the rate
  unit, currency, 4X and their units, the running subtotal on subtotal rows, the line total
  in the base currency, its fringes and the fringe dollars it earned, groups, set,
  location, note, whether it counts toward the total (`Yes`/`No`), its row id and its
  fringe range number (lines with the same number in an account are one range; added
  last so positional importers keep their columns). An adjustment's rate is the amount it
  is a percentage of.
- **Setup tables**: globals with their formulas and values; fringes with type, rate (as
  typed and its value), cutoff or unit, lines and total; groups with their switch and
  totals (every tagged line, and what counts toward the budget); units with hours;
  currencies with their rate in the base currency; sets and locations with their totals.

Figures are the budget's own, unfiltered: every group as the budget sets it. Money is
exact: integer cents written as a decimal (`1234.56`), never through a float; in the
workbook it is a number with a `#,##0.00;(#,##0.00)` format (negatives in parentheses, as
the app shows them), decimals are numbers as the engine holds them, and text is always a
string, never a formula. Each sheet's header row is bold and stays in place when
scrolling, and column widths follow the content. CSV follows RFC 4180 (commas, CRLF,
quotes only when needed); tab-delimited text never quotes, so tabs and line breaks inside a
field become spaces. In both, text a spreadsheet would run as a formula (starting with `=`,
`+`, `-` or `@`) gets a leading `'` unless it is a plain number.

A budget with a cell the engine can't evaluate is not exported (the message names the
first problem): the grid counts such cells as blank, and an export must carry the real
figures. Each file is written atomically, and an export never replaces a budget that is
open in a tab. The same tables come from the command line:
`method export <file.mthd> --format xlsx|csv|tsv --out <path> [--bom]`.

| Command | Arguments | Returns |
|---|---|---|
| `export_budget` | `format` (`"xlsx"`, `"csv"` or `"tsv"`; shows the Save dialog) | `{done, files}`: `done` false when cancelled; `files` the names written |

### Importing spreadsheets

**File > Import Spreadsheet…** adds detail lines from an Excel workbook (`.xlsx`), a CSV
file or tab-delimited text (`.tsv`, `.txt`): any department's sheet, an old budget kept in
Excel, or our own export. Rust shows the Open dialog and reads the file; then a dialog maps
the sheet's columns to budget fields and shows what the import will do before anything
changes:

- **Sheet and header row.** A workbook's sheets are listed (hidden ones say so); the dialog
  starts on the sheet whose column titles name the most fields (our own export's Detail
  sheet). The header row is found among the first 30 rows (the row whose titles best name
  fields, else the first row of text over a row of numbers) and can be changed, or set to
  none; the list shows the top 12 rows to pick from.
- **Columns.** Category number and name, account number and name, row kind, description,
  amount, unit, X, rate, currency, 4X, fringes (names separated by commas), fringe range,
  groups, set, location, note, and a line total that is only compared, never imported.
  Each starts as guessed from the titles (`Acct #`, `Qty`, `Unit Price`, `Ext. Total`...;
  our own export maps itself completely) and each select shows the column's letter, title
  and first value.
- **Saved mappings.** Save Mapping keeps the header row, columns and options under a name,
  per user (in the web view's storage, like other per-user choices). A sheet whose header
  row has the same titles (ignoring case, spaces and punctuation) starts with the newest
  saved mapping for them, so a sheet laid out the same way next week is one click.
- **Options.** Add accounts and categories this budget doesn't have (off: their rows are
  skipped, and listed); add fringes, groups, units, sets, locations and currencies it
  doesn't have (off: they are left off the lines, and listed; new fringes start at 0%,
  new currencies at a rate of 1, and the preview says so); the account for rows without
  one (the account on screen to start with); and where to import: **this budget**, as one
  undo step, or **a new budget tab** that starts with this budget's categories, accounts,
  setup and settings and no lines (as New From Template makes it), with the import as its
  first undo step, so it starts with unsaved changes.
- **Preview.** How many lines go into how many accounts, new accounts and categories,
  skipped rows, the grand total now and after, then the first 25 lines as they will come
  in with what each comes to; skipped rows and rows with something left out (each with its
  row number and why), new accounts and categories, and setup items added or left off are
  listed below. Import is disabled while nothing would be imported.
- **Check against the sheet's totals.** With a line total column mapped, each imported
  line's total (base currency, after import, as the budget calculates it; a subtotal row's
  running total) is compared with the sheet's: how many lines were compared, how many
  differ, the two sums and their difference, and the first 50 differing rows. Differing
  rows are tinted in the preview table.

How rows are read (`fwb_format::import::merge`): rows after the header row; rows empty in
every mapped column are passed over. Account numbers are read with the budget's account
numbering (`1105`, `11-5` and `11.05` are all `11-05` under the default pattern) and
matched anywhere in the budget; a number that doesn't fit the pattern (too long, too many
parts) is refused rather than cut short. An empty account cell takes the account above it.
A row with an account or category and nothing for a line only sets the account for the
rows below (a heading), as does a row with an account and only a description when the
next row leaves the account blank (the description then names a new account). New
accounts and categories are numbered by the pattern (a new account's category is the
category column's, else the category part of its number) and placed after the last one
numbered below them. Lines go at the end of their account, in sheet order, with new row
ids; fringe range numbers get numbers new in the account. Amount, X, rate and 4X are read
exactly (no floats): number cells as written, rounded to the engine's nine places; text
with currency signs, thousands separators, `(5)` for -5 and `10%` for 0.1 (a semicolon
CSV uses decimal commas, `1.234,56`); a formula is kept when its globals exist, otherwise
the cell is left blank and the row says so. Names (fringes, groups, units, sets,
locations, currencies) match exactly, else ignoring case when only one does; a unit also
matches its plural (`Days` is `Day`); the base currency is no currency. Row kinds are
line, subtotal, adjustment and separator; subtotal and separator rows keep no amounts and
an adjustment keeps no rate (it is always the lines above it). A note becomes one note on
the line.

Reading the files (`fwb_format::import`, plain Rust, no crates): CSV per RFC 4180 with
quoted fields, CRLF, LF or CR, a UTF-8 byte order mark dropped, UTF-16 with a byte order
mark (Excel's Unicode Text), Windows-1252 when the bytes aren't UTF-8, and the delimiter
(comma, tab or semicolon) taken from whichever splits the first lines most consistently;
the `'` our export puts before formula-like text is taken off again. Workbooks: our own
ZIP reader (stored and deflated entries, our own DEFLATE decoder), the parts found through
the package's relationships, shared strings (rich text joined, phonetic guides left out),
inline strings, numbers as written, booleans, formula results, errors as text, cells and
rows without references, Excel's `_xHHHH_` escapes, and the Strict variant's namespaces.
Dates come in as serial numbers (styles aren't read). An older `.xls` workbook is refused
with a message to save it as `.xlsx` or CSV. Exporting a budget and importing the Detail
sheet back (Excel, CSV or tab-delimited) into the same chart without lines gives exactly
the same line totals and grand total (tested).

| Command | Arguments | Returns |
|---|---|---|
| `sheet_import_pick` | none (shows the Open dialog) | `{token, file, kind, sheets: [{name, hidden, rows, columns, truncated}], sheet, fields}`, or `null` when cancelled; `file` is the name only |
| `sheet_import_preview` | `token`, `request` (`{sheet, header_row, mapping: {field: column}, detect_header, guess, create_accounts, create_setup, default_account, target}`, `target` `"new"` for a new tab) | the preview: `{header_row, signature, mapping, columns, raw, lines, line_count, skipped, skipped_count, warnings, warning_count, headings, new_categories, new_accounts, matched_accounts, added, dropped, notes, reconciliation, grand_total_before, grand_total_after, calc_error, truncated}`; changes nothing |
| `sheet_import` | `token`, `request` | `{view, message}`: into the budget as one undo step, or a new tab in front |

### Actuals and the cost report

**File > Import Actuals…** brings in actual costs from an accounting system's export (an
Excel workbook, CSV or tab-delimited file; decisions.md, entry 45). Rust shows the Open
dialog and reads the file with the same readers as Import Spreadsheet; the dialog (built
like Import Spreadsheet's) picks the sheet and header row (both found automatically) and a
column for each field: date, account number, amount (or debit and credit: the amount is
debit minus credit), vendor, description, reference (invoice, PO or check number) and
tags, each guessed from the titles (`Trans Date`, `GL Account`, `Payee`, `Memo`,
`Invoice #`...). Options: how slash dates read (found from the file when a date can only
be read one way, else month first), "costs are negative numbers in this file" (flip every
sign), "empty account cells take the account above" (for reports that name each account
once), skip rows matching a transaction already in the budget (on), skip rows repeating an
earlier row of the file (off). The preview shows the first 25 transactions, the actual to
date now and after, then the **review**: every account number the chart doesn't have (and
the rows with none), with its rows and amount, each to keep unassigned (the default: the
number stays on the transactions, so adding that account later picks them up), skip, or
put in one of the chart's accounts; the possible duplicates (by date, account, amount and
description: of a transaction the budget has, skipped by default, or of an earlier row,
imported by default); skipped rows (no amount, a zero amount, a total row with no date)
and rows with something left out (an unreadable date is left blank). Import is one undo
step and one **import batch** the Actuals tab can take back out. Mappings are saved per
user like Import Spreadsheet's (by the header row's titles); the review is per budget and
never saved.

How rows are read (`fwb_format::import::actuals`): amounts exactly (no floats; currency
signs, thousands separators, `(5)` for -5), rounded once to the cent; dates from workbook
serial numbers (the 1900 system), ISO `2026-09-30`, `2026/09/30`, `20260930`, `9/30/2026`
or `30/9/2026` by the date order, `30.9.2026` (day first), and month names (`Sep 30,
2026`, `30-Sep-26`), two-digit years 00 to 29 as 2000 to 2029; account numbers through the
budget's numbering (`1101` is `11-01`) and matched to its accounts and, when fringes post
by category, its category fringe accounts.

**The cost columns.** A budget with actuals gets Actual, ETC (estimate to complete), EAC
(estimate at completion), EAC Var (budget minus EAC) and % Spent on the topsheet (every
category, production total, posted fringes, the summary rows, charges, "Unassigned
actuals" and the grand total) and the account list (every account, the category fringe
account and the category total), before Total like Orig and Var; the lines get Actual (the
transactions assigned to each line), and an account's total row its whole actual. They
show by default once there are actuals and are hidden or shown one by one from the column
menu. An over-budget row's EAC Var is in the warning color. The figures are the engine's
(`fwb_core::actuals::cost_report`): an account's ETC is budget minus actual (never past
zero) unless the Actuals tab sets a typed amount or a percent complete (EAC = actual /
percent, rounded once to the cent); categories, levels and the totals add up their
parts exactly; budget items outside the chart (fringes posted by budget or level, charges)
keep their whole budget to complete; credits aren't part of it.

**The Actuals tab** (tools panel, second pane; View > Tools Panel > Actuals): the number of
transactions, the actual to date and how far over or under the estimate at completion is;
for the account at the grid's cursor (on the account list, or the account on screen), its
budget, actual, ETC, EAC and variance and its estimate-to-complete method (budget minus
actual, manual amount or percent complete; Set is one undo step); the transactions,
filtered by account (or the unassigned ones), vendor, text and dates (the first 500 by
date, with the count and total of every match); tick transactions to **Edit…** (one: date,
account, the line it pays for, vendor, description, reference, tags, amount), **Move to…**
another account or unassigned, or **Delete**, each one undo step (a double-click edits
one); and **Imports**, each file imported with how many of its transactions are left and
**Remove** to take the whole import out (one undo step).

**Printing and exporting.** Print's setup dialog has a **Cost report** section: every
category's accounts with budget, baseline (when the budget has one), actual, ETC, EAC,
variance and percent spent, the category totals, production totals, the summary rows,
unassigned actuals and the grand total, accounts left out of the report left out of the
listing but never out of the totals; over-budget variances print bold. Landscape fits it
best. File > Export adds two sheets (CSV files) for a budget with actuals: **Cost Report**
(the same rows, with the ETC method) and **Actuals** (every transaction with its account's
name, the line it pays for and the file it came from).

Actuals are kept in the budget file (docs/FORMAT.md, "Actuals"), are not budget content
(versions don't carry them; restoring a version keeps them), follow their accounts when
accounts are renumbered, are left out of templates and sub-budgets, and Save a Copy for
Sharing can leave them out.

| Command | Arguments | Returns |
|---|---|---|
| `actuals_pick` | none (shows the Open dialog) | `{token, file, kind, sheets, sheet, fields}`, or `null` when cancelled; `file` is the name only |
| `actuals_preview` | `token`, `request` (`{sheet, header_row, mapping: {field: column}, detect_header, guess, date_order, negate, carry_account, codes: {key: {choice, account}}, skip_existing, skip_repeats}`) | `{header_row, signature, mapping, columns, raw, transactions, count, total, skipped, skipped_count, warnings, warning_count, headings, unmatched, duplicates, duplicate_count, duplicates_skipped, date_order, date_order_found, accounts, unassigned, chart, actual_before, actual_after, truncated}`; changes nothing |
| `actuals_import` | `token`, `request`, `imported` (the page's local time) | `{view, message}`: one undo step, one import batch |
| `actuals_list` | `filter` (`{account, unassigned, vendor, text, from, to, batch}`) | `{transactions, count, total, all, unassigned, batches, chart}`; changes nothing |
| `actuals_update` | `ids`, `edit` (`{date, account, line, clear_line, tags, vendor, description, reference, amount}`, each optional; `amount` typed text) | View (one undo step) |
| `actuals_delete` | `ids` | View (one undo step) |
| `actuals_remove_batch` | `batch` | View (one undo step) |
| `actuals_set_etc` | `account`, `method` (`budget_minus_actual`, `manual`, `percent_complete`), `value` | View (one undo step) |
| `actuals_lines` | `account` | `[{id, n, description}]`: the account's costed lines |

Every view of a budget with actuals carries `cost`: the cost figures (`{budget, baseline,
actual, count, etc, eac, variance, percent, over}`) of every category and its accounts and
fringe account (by position, as `report` lists them, with each account's method), every
level, the summary rows, the charges, the unassigned actuals (with their account numbers)
and the total, plus the line sums of the account on screen (every line's in the full
view). The view's `budget` never carries the transactions. The only pass over every
transaction (summing them by account) is cached by the session until the actuals or the
account pattern change, so an edit to a line costs one pass over the accounts: on this
Mac, a one-cell edit with its view in the 5,000-row sample takes 4.3 ms with 20,000
transactions, as without (release build; the view grows about 21 KB for its 80 accounts).

## Saving and recovery

**Autosave** is on unless you turn it off: the Autosave switch in the toolbar next to Save
As, or File > Autosave (one setting for the app, kept in its data folder). With it on, a
budget that has a file is saved into that file 3 seconds after you stop changing it (at
most 30 seconds after the first unsaved change while you keep going), and right away when
the app loses focus (moving between the main window and the tools window doesn't count),
when you close its tab, and when you close the window or quit. Every open budget (every
tab) is timed and saved on its own, in front or not. The
unsaved-changes mark clears after each autosave. Undo and Redo keep working across
autosaves (an undo is a change like any other and is saved in turn). A budget never saved
(Untitled) can't be saved into a file: it gets a recovery copy instead (below), and the
first Save asks where, as before. If an autosave into the file fails (the folder is gone,
the disk is full), the app says so once and keeps a recovery copy.

**File names.** New budgets, Save As, shared copies, templates and sub-budgets suggest
`.mthd`, and a name typed without an extension gets `.mthd`. A budget opened from an older
`.fwb` file keeps saving and autosaving to that same file; only Save As (or a copy) gives it
a new name, and a `.fwb` name typed on purpose is kept. Every dialog that opens a budget
accepts both.

**Changes made outside the app.** Because autosave writes the file often, the app never
writes over a version it hasn't seen. When it opens or writes a budget file it notes the
file's size, modification time and a hash of its content; before every autosave and every
Save into that file, and whenever the window gets focus back, it compares, for every open
budget. A budget in a tab that isn't in front has its tab brought forward before the
question, one budget at a time. If another
program changed the file meanwhile (a sync service such as Dropbox or iCloud, another
computer, another app), nothing is written, autosave into that file pauses (a recovery copy
keeps the unsaved work safe), and a dialog, "This budget was changed outside Method Budget",
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
macOS and Linux), one per open budget, each a budget file (named `.fwb`, an internal detail) with a small
`.json` beside it (the
original path, title and time). A copy is deleted when the budget is saved, when undo
takes it back to the saved state, when you close its tab or the window (or quit) and choose
not to save, and when an untouched Untitled budget gives its tab up to a file. Quitting
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
app's data folder: `Backups/<hash of the file's path>/<name> <date time> UTC.mthd` (backups
taken before the rename end in `.fwb` and are listed too), with a
`source.json` naming the file. An explicit Save always makes one (unless the file is
unchanged since the last backup); autosaves make one on the first write in a session and
then at most every 10 minutes, so the backups span real time rather than the last few
seconds of typing. The last 10 per file are kept. **File > Revert to Backup…** lists the
open budget's backups, newest first; **Restore** puts that version in place of the budget
as one edit (Undo brings the current one back; autosave or Save then writes it), and
**Show Backups Folder** opens the folder in Finder, Explorer or the file manager. We chose
the data folder over a `film.mthd~` file beside the budget: nothing appears in shared
project folders, there is room for more than one version, and the dialog finds them.

**Saves are atomic.** Every write (a save, an autosave, a recovery copy, a backup, the
settings and recent lists) goes to a temporary file in the same folder, is flushed to disk,
then renamed over the target (and the folder flushed, on macOS and Linux). A budget file
keeps the permissions of the file it replaces. A failed write leaves the old file whole.

**Reopening tabs.** The files open as tabs, in tab order, and which one is in front are kept
in the data folder's `tabs.json`, rewritten whenever a tab opens, closes, moves, comes to the
front or is saved under a new name. Split, it also keeps which of those files are in which
pane, each pane's budget in front and the target pane (by place in the list; a pane left
with no file means one pane). The next start opens them again (files that are gone or
can't be read are left out) before the window asks for its first view; with none, it starts
with an Untitled budget as before. Budgets that never had a file aren't listed: their
unsaved work comes back as a recovered budget. A recovered budget whose file was reopened
untouched takes that tab's place.

**File > Open Recent** lists the last 10 budgets opened or saved (newest first, each once;
two files with one name show their folder), kept in the data folder's `recent.json`. Files
that no longer exist drop off when the menu is refreshed (every open and save). Clear Menu
empties it. macOS's own system-wide recent documents list is not updated.

**Opening from the system.** Double-clicking a `.mthd` (or older `.fwb`) file, Open With, or dropping one on
the Dock icon opens it in a tab (its own tab, when it is open already): on macOS the app
receives an open event, on Windows and Linux the path arrives on the command line. Nothing
is asked: no budget is replaced. When several files arrive at once, the first one opens. On macOS a
second double-click goes to the running app. On Windows and Linux there is no
single-instance handling: a second launch with a path starts a second copy of the app with
that file (each copy has its own recovery copies and doesn't offer the other's).

The app's data folder is the platform's local app data folder for
`com.methodbudget.desktop`: `~/Library/Application Support/...` on macOS,
`%LOCALAPPDATA%\...` on Windows, `~/.local/share/...` on Linux.

**Moving over from the old identifier** (src/migrate.rs; decisions.md, entry 46). The data
folder is named after the app's identifier, which changed with the rename from
`io.github.ssandweiss.film-wizard-budgeting`. On every start, before the window opens,
the app checks: when the new folder has none of its state (`settings.json`,
`recent.json`, `tabs.json`, `library.json`, `Recovery/`, `Backups/` or the
`migrated-from.json` marker) and the old folder exists, the old folder is **copied** (never
moved, so an old copy of the app still works) into a temporary folder beside the new one,
which is then renamed into place, so an interrupted copy leaves nothing half done. Lock
files and web view caches stay behind; a file that can't be read is skipped and named on
stderr. If the web view made the new folder first (Windows and Linux keep its storage
there), the copied entries move in one by one, never over anything already there. On
macOS the web view's storage (display choices and saved spreadsheet mappings) lives in
`~/Library/WebKit/<identifier>` instead and is copied the same way when the new one doesn't
exist yet. `migrated-from.json` records where the copy came from and when; the check runs
again from Tauri's setup with the folder Tauri reports, which does nothing once copied.

| Command | Arguments | Returns |
|---|---|---|
| `autosave_setting`, `set_autosave` | `on` (set) | the setting |
| `flush_autosave` | `doc` | View (autosaved now, if anything was unsaved) |
| `open_pending` | none | View of a file the system asked to open (in a tab), or `null` |
| `open_recent` | `index` (its place in Open Recent) | View (in a tab), or `null` |
| `recovery_list` | none | `[{ key, path, name, title, saved_at_ms }]`, newest first |
| `open_recovered`, `discard_recovered` | `key` | View (in a tab); the remaining list |
| `backup_list` | `doc` | `[{ id, saved_at_ms, bytes }]` for that budget's file |
| `restore_backup` | `doc`, `id` | View |
| `reveal_backups` | `doc` | nothing (opens the folder) |
| `resolve_external_change` | `doc`, `choice`: `keep` or `load` | View |

`doc` is optional everywhere (the budget in front when left out; below). Rust tells the page
about autosave with events: `file-state` (`{ doc, rev, file, tabs }` after a save the page
didn't ask for: autosave, or Save All on the way out; the page redraws the tabs and updates
the title if `doc` is in front and `rev` is still current), `autosave-changed`,
`autosave-error`, `open-request` (a file from the system; the page finishes any cell edit,
then calls `open_pending`) and `external-change` (`{ doc, name }`: that budget's file
changed outside the app; the page brings its tab forward and shows the dialog above).

## Updates

The app keeps itself up to date (decisions.md, entry 48; how releases are made:
[RELEASING.md](RELEASING.md)).

- **Checking.** About two minutes after the app starts, then every six hours, it asks the
  update endpoint (`plugins.updater.endpoints` in `tauri.conf.json`, the one place the
  address is written) whether a newer version exists. Help > Check for Updates Automatically
  (a check item, on unless turned off, kept in `updates.json` in the app's data folder)
  stops the timed checks; Help > Check for Updates… checks at once.
- **Quietly.** A newer version downloads in the background. The updater plugin checks the
  download's signature against the public key the app was built with
  (`plugins.updater.pubkey`) before it is kept; anything else is thrown away. Then the
  toolbar shows **Update ready: Restart to update**. An automatic check that finds nothing
  or can't reach the endpoint says nothing at all; only Help > Check for Updates answers, in
  the same place for a few seconds ("Method Budget is up to date." or "Couldn't check for
  updates"). Nothing appears as a dialog while you work.
- **Restart to update** saves first: every open budget's unsaved changes go into its file
  by Autosave's own rules (so a file changed outside the app is never written over: that
  budget's question comes up instead), the open tabs and split view are remembered, and any
  budget that still can't be saved (untitled, Autosave off, a file changed outside the app)
  gets the same Save All, Discard All or Cancel question as quitting. Cancel leaves the
  update ready. Then the update is installed and the app starts again with its tabs, split
  view and tools panel as they were (the files reopen as at any start: src/tabs.rs).
- **What's New** for the new version opens once after an update, after any start-up
  question (recovered budgets, a file to open). Help > Release Notes shows this version's
  notes at any time. The notes are `docs/releases/<version>.md`, copied into the app when it
  is built (build.rs), read by `method_release::notes` into plain text pieces, and drawn with
  DOM calls and `textContent` (ui/js/updates.js), never as markup.
- **No key, no checks.** Until the release key exists, `tauri.conf.json` has a placeholder
  public key; a build with it never checks (no timer, no network), and Help > Check for
  Updates says "Couldn't check for updates". While the repository is private the endpoint
  answers "not found", which is a failed check like any other.

The page's side (main window only): `update_status` (the indicator's state:
`{ phase, version, manual, said, automatic }`, `phase` being idle, checking, downloading,
ready or installing; `manual` when the user asked; `said` a one-time answer: `current`,
`failed` or `install-failed`), `update_restart`, `whats_new` (the notes once after an
update, else null) and `release_notes`. Rust sends `update-status` events with the same
shape. Check for Updates and the automatic switch are answered in Rust (src/updater.rs).

## Architecture

```
crates/fwb-app/
  src/session.rs     one open budget: Budget, file path, dirty flag, undo/redo patches
                     (each step labeled and timed), the account on screen, the recalc
                     cache, autosave and recovery state
  src/docs.rs        every open budget (one Session per tab) in tab order, the one in
                     front, split view's panes and target, and the lock the commands go
                     through
  src/tabs.rs        the tab and split view commands, the Window menu's list of budgets,
                     remembering the open files (and panes) for the next start
  src/history.rs     the History tab: step labels, the list, jumps, a step as a version
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
  src/import_setup.rs  Import Setup From: the review of another budget's setup items
                     (status, what each needs), importing the picks as one undo step
  src/templates.rs   New From Template and Save as Template: a budget stripped to its
                     chart, setup and settings
  src/library.rs     the personal setup library: which file it is (library.json), its
                     view, copying items in and out through Import Setup's review, its
                     folders, atomic writes, refusing changes while it is open as a tab
  src/sheet_import.rs  Import Spreadsheet: the picked file kept by token, the mapping
                     dialog's preview and the import (fwb_format::import does the work)
  src/actuals.rs     Import Actuals, the Actuals tab's commands and the cost figures
                     every view of a budget with actuals carries (fwb_core::actuals and
                     fwb_format::import::actuals do the work); the rollup cache
  src/menu.rs        native menu bar, built from the same table as ui/js/shortcuts.js
  src/tools.rs       the tools panel's own window: opening, hiding and placing it, telling
                     the other window when the budget changed, the Notes tab's list
  src/setupwin.rs    the Budget Setup window: opening, placing and closing it
  src/notes.rs       dated notes on lines, accounts and categories, and the budget-wide
                     list the Notes tab shows as a review checklist
  src/setup_lists.rs the setup lists' use counts and totals, their order, short codes
  src/find.rs        find and replace: typed search over a budget, replacing in the
                     checked results as one undo step
  src/moverows.rs    moving lines, accounts and topsheet rows one place, keeping fringe
                     ranges whole; quick presses join one undo step
  src/rename.rs      renaming a setup item everywhere (fwb_core::rename does the work)
  src/renumber.rs    renumbering accounts to a new pattern: the preview and the one-step
                     renumbering (fwb_core::renumber does the work)
  src/sharing.rs     copies for sharing (Save dialog, provenance, read-only, editable
                     copies, returned copies stored as versions) and sub-budgets
  src/versions.rs    the Versions dialog's commands: save, restore, rename, delete, each
                     one undo step, with names, notes and times checked
  src/compare.rs     the compare screen's commands: budgets copied out of tabs, versions
                     and picked files, compared after the lock is released
  src/diffmerge.rs   What Changed and Merge Changes From: budgets copied out, totalled
                     as their grids do, diffed or merged after the lock (fwb_core::diff
                     and fwb_core::merge do the work); the merge is one undo step
  src/reports.rs     report presets saved in the budget (one undo step each), and
                     printing a report to the print sheet or a PDF file
  src/pdf_mac.rs     macOS: AppKit's print operation on the web view, into a PDF file
                     with no panel or through the print sheet (Objective-C runtime calls)
  src/export.rs      File > Export: the budget's tables (fwb_format::export) as an Excel
                     workbook, CSV files or tab-delimited detail, through a Save dialog
  src/safety.rs      saving safety as plain files: atomic writes, recovery copies,
                     backups, settings, the recent list (tested with temporary folders)
  src/safety_app.rs  its Tauri side: autosave timer, recovery and backup commands, Open
                     Recent and Autosave menu items, files opened by the system, opening
                     budgets in tabs and closing them, reopening the tabs at the start
  src/updater.rs     in-app updates (the plugin driven from Rust: timed and asked-for
                     checks, background download, Restart to update after saving every
                     budget), What's New once after an update, Help > Release Notes
  src/app.rs         Tauri glue: commands, file dialogs, the close and quit guard (one
                     question about every unsaved tab), menu events
  examples/perf.rs   timings on a large sample budget (fwb_core::sample), and canned
                     replies for a UI harness (perf dump DIR)
  ui/                index.html, styles.css, setup.css, js/*.js (ES modules, no build step)
  capabilities/      what each window's web view may call (main.json, tools.json,
                     setup.json)
```

**Rust owns the budgets.** The app state (`src/docs.rs`) holds every open budget, one
`Session` each, in tab order with an id per document and one of them in front, behind one
mutex. The web view never holds a real budget; it sends a command (`set_detail_field`,
`add_detail`, `set_global`, `undo`, ...) and gets back a `View`. Every command works on the
budget in front unless its optional `doc` argument names another; each window passes the
budget it is showing as `doc` with every command (`api.js`), so a command sent just as
another tab comes to the front still lands on the budget the user was looking at. A `doc`
that has been closed is refused with a message, nothing changed.

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
  index in its category), `code`, `details` (its rows, file-format JSON), `rows` (their
  totals, as the report's rows would be, or `null` without totals) and `values` (per row,
  the values of its formula cells, `{amount, x, rate, x4}` with blank, plain-number and
  failing cells left out, or `null`; empty without totals; `src/display.rs`).
- `error`, `partial`, `issues` (problems with globals, fringes, charges and the like, plus
  the cells of `account`), `issue_count` (all problems) and `issue_accounts`
  (`[ti, ai]` of every account with a problem cell, for the red outlines up the levels).
- `variance`: original vs current totals when there is a baseline: `accounts`,
  `categories`, `grand_total`, `net_total`, `rows` for `account`'s rows only, and
  `rows_changed` (rows that moved, anywhere).
- `file` (`name`, `path`, `dirty`), `can_undo`, `can_redo`, and `created_row` after an insert.
- `doc`: which open budget this is, and `tabs`: every tab in order (`id`, `label`, `title`,
  `name`, `path`, `dirty`, `conflict`, `active`). A page draws only the view of the budget
  in front; a reply about another one (a background tab saved or closed) only redraws the
  tabs.
- `panes`: the window's panes (`split`, `target`, and per pane its `tabs` as ids in its
  order and its `front`): one pane with every tab, or split view's two.

| Command | Arguments | Returns |
|---|---|---|
| `focus_account` | `category`, `ti`, `account`, `ai` (all `null`: no account) | View with that account |
| `get_full_view` | none | everything: the whole budget, every row's totals, every problem |
| `new_tab` | none | View of a starter budget in a new tab, in front |
| `switch_tab` | `doc` | View of that budget, now in front |
| `move_tab` | `doc`, `to` (its place afterwards) | the tabs |
| `close_tab` | `doc`, `discard` | `{closed, unsaved, last, view}`: closed (view: the budget now in front); or not, because of unsaved changes (ask, then again with `discard`, or save first) or because it is the last tab (close the window) |
| `list_tabs` | none | the tabs |
| `switch_tab` (split) | `doc`, `pane` | View of that budget, in front in that pane, which becomes the target |
| `close_tab` (split) | `doc`, `discard`, `pane` | as above; when the other pane shows the budget too, only this pane's tab goes |
| `split_view` | none | View: a second pane showing the budget in front too, the target |
| `close_split` | none | View: one pane again, the target's budget in front |
| `set_target_pane` | `pane`, `category`, `ti`, `account`, `ai` | View of that pane's budget in front, with that account on screen |
| `move_tab_in_pane` | `pane`, `doc`, `to` | View (the budget in front) |
| `move_to_pane` | `doc`, `from`, `to` (or `null`: the end) | View: in front in the other pane, the target |
| `pane_view` | `doc`, `category`, `ti`, `account`, `ai` | View of that budget at that account; changes nothing, tells no other window |
| `history_list` | `doc` | `{steps: [{label, at, done}], current, trimmed}`: every undo step oldest first, `at` in milliseconds since 1970, `current` how many are applied |
| `history_jump` | `doc`, `to` | View after undoing or redoing to `to` steps applied |
| `history_save_version` | `doc`, `step`, `name`, `note`, `created` | View: the budget as it was at `step` saved as a version (one undo step) |

Every budget command also takes `doc` (above). The account on screen is per budget, so
coming back to a tab brings its account's rows with the view. In split view it follows
the target pane: `set_target_pane` sets it, and the other pane draws its own account with
`pane_view` (only when it shows the same budget at another account), so the view stays
lean and an edit costs one more view at most.

Every accepted edit names its undo step (`Session::edit(label, f)`; `edit_budget` and
`edit_budget_as` in `app.rs` for the commands): the label says what the command did, the
way the grid names things, and goes with the step through undo and redo, with the wall
clock time it was made. `set_cells` takes `action` (`paste`, `fill`, `cut`, `clear`) for
its label; anything else is an edit. `Session::budget_at` reads the budget at any step
without changing it (the steps are walked on the budget and walked back).

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
right for `method check` but not for an editor. `diagnose::lenient_recalc` runs the engine;
if it fails, it checks every global, fringe rate and detail cell with the engine's own
rules, flags each bad one (`issues`, by row id and field), and recalculates a copy with
those cells blank. The UI shows the engine's message in a banner, outlines the bad cells,
and shows totals marked as partial. The budget keeps what the user typed.

**Selection totals** come from `fwb_core::selection_totals` through the
`selection_totals` command.

**The UI** is a handful of ES modules with no dependencies: `main.js` (state, navigation,
shortcuts, and where each tab was left), `tabs.js` (the tab strip: switching, closing,
dragging, scrolling), `grid.js` (a generic keyboard grid used at all three levels), `levels.js`
(turns a View into rows and columns), `footer.js`,
`modal.js`, `format.js` (money and decimal display), `api.js` (the command queue),
`dom.js`, `util.js` (small shared helpers: plurals, error text), `panelsize.js` (the
tools panel resizer and saved window sizes), `shortcuts.js` (the one list of shortcuts, shared with the Rust menu and checked
by a test), `help.js` (the shortcut sheet), `topsheet-setup.js` (Budget Setup's
charges, credits, info, settings and baseline), `report.js` with `print.css` (printable
reports, laid out into pages) and `reportsetup.js` with `reportsetup.css` (File > Print's
setup dialog and its preview), `menu.js` (the small popup menus: breadcrumb jump lists and the column menu, and
the right-click menu with its submenus), `context.js` (what the right-click menu offers at
each level, opened by the mouse, Shift+F10 or the menu key), `rowformat.js` (bold, italic,
indent, text color and the color palette; `context.css` draws the menu, the palette and
the tools panel's small editors),
`autocomplete.js` (the type-ahead list on a cell editor, a listbox the editor drives with
`aria-activedescendant`), `quickentry.js` (wires it to the grid, sends edits that add
setup items, the note editor, the fringe range keys and the formula bar),
`clipboard.js` (copy, cut and paste of whole rows, and the paste review),
`versions.js` (the Versions dialog), `compare.js` with `compare.css` (the compare screen;
`compare.css` also sizes the Orig and Var columns),
`diffmerge.js` with `diffmerge.css` (What Changed and Merge Changes From, in the compare
screen's frame),
`sharing.js` with `sharing.css` (Save a Copy for Sharing, the shared copy's banner and the
returned-copy question, New Sub-Budget; `sharing.css` also draws `renumber.js`'s preview in
Budget Setup > Settings > Account numbers),
`prefs.js` (per-user display choices in local storage: visible columns, the tools panel's
place, size and tabs), `display.js` with `display.css` (how numbers show: formula values,
rate and global decimals, totals with or without cents, hidden-factor marks, zero rows,
zoom and the formula view, and those View menu commands), `display-settings.js` (Budget
Setup > Settings > Numbers on screen and each global's details) and `sheet.js` (styles worked out at run time, below). `boot.js`
starts `main.js`, `toolswin.js` in the tools window (the page loaded with `?panel=1`), or
`setupwin.js` in the Budget Setup window (`?panel=setup`; its tabs are listed in
`setuptabs.js`, and `setupwin.css` lays it out, matching only `body.setup-window`).
The tools panel is `tools.js` (panes, tabs, dragging, the header) with `apply.js` (the
fringe, group, set, location and currency lists, which Budget Setup draws as setup tables),
`setup.js` (globals and units, compact in the panel and in full in Budget Setup, and the
helpers the setup tabs in `topsheet-setup.js` share), `setuplist.js` (what every setup
list shares: counts, Show, the name sort, dragging and moving items, the delete question),
`order.js` (a table's order and short codes, also used by the grid, the type-ahead and
reports), `notes.js` (the Notes tab), `find.js` (the Find tab), `history.js` (the
History tab) and `actuals.js` (the Actuals tab; `actualsimport.js` is File > Import
Actuals…, `actuals.css` styles both and the grid's cost columns); `tools.css` lays it out. Commands run one at a time, in order.

**Split view** (`pane.js`): each pane is a `Pane` holding where it is (budget, level,
category, account, the footer's counters, each tab's memory), its grid and its tab strip
(`tabs.js` makes one per pane, `createTabStrip`). `main.js`'s `state` and `grid` read the
pane in use: the target, except while another pane is drawn (`withPane`), so every
command, key and menu item reaches the target as before split view. A view is drawn in the
pane Rust says is the target (or the pane that asked for it); the other pane follows its
budget (the same view, or its own `pane_view`). Split, the first pane's strip and footer
move into a column beside the second pane's, and the one formula bar moves to the target;
closing the split puts them back where `index.html` has them.

**Two windows.** Popped out, the panel runs in a second web view on the same page and the
same Rust budgets, and follows the budget in front in the main window. After any command
whose view is news (a new view `rev`: the budget changed, or another tab came to the front,
since revs are never shared between budgets), Rust sends `view-changed` (`{doc, rev}`) to
every window but the one that sent the command, and that window fetches a fresh view: the
main window its own budget, the tools window the one in front (`get_view` with no `doc`).
The tools window's commands name the budget it drew, and a reply about a budget no longer in
front makes it fetch again; the selection it hears carries the budget too. A change made in
the tools window counts in the main window's Last change like any other edit. The
pages also talk directly, each listening only under its own label (`main` or `tools`) so a
window never hears events meant for the other: the main window sends `selection-changed`
(level, row keys, cursor, account) and `tools-state` (that and the tab layout, when the
tools window says `tools-ready`); the tools window sends `tools-position` (a dock button),
`tools-layout` (tabs moved; the main window keeps the preference), `tools-jump` (a noted
line), `tools-history` (the History tab: undo or redo to a step, or save one as a version,
for the budget it drew) and `menu` (keys and menu items for the main window). Split, the
tools window follows the target pane's budget, which is the one in front in Rust. Rust sends `tools-geometry`
(where the tools window is, remembered by the main window) and `tools-closed` (it closed:
the panel docks back). A menu item chosen while the tools window is in front goes to it
first (`src/menu.rs`): it keeps Undo, Redo and Select All for its own text fields and the
tab items, and passes the rest on.

**The Budget Setup window** is a third web view on the same page (`?panel=setup`), built the
same way (`src/setupwin.rs`, `setupwin.js`): it fetches the view with no `doc`, names the
budget it drew in every command, and hears `view-changed` under its own label (`setup`),
so it follows the budget in front and redraws after edits made elsewhere. The main window
sends it `setup-state` (the tab to show and an item or control to focus: right away when
`setup_window` says the window was already there, else once the page says `setup-ready`);
it sends the main window `setup-tab` (the tab it shows, remembered with its place and size
from Rust's `setup-geometry`), `setup-find` (a line count was clicked: the panel's Find tab
shows the item's uses, in the tools window when the panel is popped out, through
`tools-find`), `setup-close` (Escape) and `menu`; the tools window's Setup buttons send
`tools-setup` to the main window, which opens it. Rust sends `setup-closed` when it goes,
and brings the main window forward, which puts focus in its grid. A menu item chosen while
it is in front goes to it first, like the tools window's.

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

- **No network unless checking for updates.** No telemetry, no remote content, no
  accounts. The only network access is the updater, in Rust (src/updater.rs): an HTTPS
  request for the update manifest at the endpoint in `tauri.conf.json` and, when there is a
  newer version, its archive. The updater's HTTP client is https-only and follows redirects
  only to the endpoint's host (plus GitHub's two download hosts, `objects.githubusercontent.com`
  and `release-assets.githubusercontent.com`, when the endpoint is on github.com); a
  manifest pointing anywhere else is refused before downloading. An archive is installed
  only if its minisign signature verifies against the public key built into the app; the
  plugin's insecure-transport, invalid-certificate and downgrade options stay off (a test
  checks), and a build with the placeholder key never checks at all. Automatic checks can be
  turned off. The web view itself still has no network: the CSP limits every
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
  The guard is a plugin hook, so it covers every web view, the tools and Budget Setup
  windows included; those windows are made in Rust with a fixed page (`index.html?panel=1`,
  `index.html?panel=setup`), never a URL from the page.
- **Least privilege for the web view.** `capabilities/main.json` grants the main window
  only the app's own budget commands, the tab commands among them (and sending events to the
  tools and Budget Setup windows), and four update commands of ours (`update_status`,
  `update_restart`, `whats_new`, `release_notes`). No window is granted the updater
  plugin's own commands (`updater:*`): the plugin is driven from Rust only, and a test
  checks that no capability names it.
  `capabilities/tools.json` grants the tools window, by its label, only what the panel
  calls: `get_view`, the selection summary and the apply edits, the setup tables as its
  compact tabs edit them, notes and the notes list, the setup lists' counts, order and short codes,
  find and replace, the setup library's commands (its own folder and Open dialogs among
  them, shown by Rust), the History tab's list of undo steps (`history_list`), the
  Explain tab's `explain_number`, the Overtime tab's `ot_calculate`, `ot_apply`,
  `ot_save_pattern` and `ot_delete_pattern`, the Actuals tab's `actuals_list`,
  `actuals_update`, `actuals_delete`, `actuals_remove_batch`, `actuals_set_etc` and
  `actuals_lines` (importing is the main window's File menu), and listening for and
  sending events. It
  can't open, save, print, undo (or jump in the history: the History tab asks the main
  window), use the clipboard or recovery, open, close, switch or move budget tabs, split
  or merge the panes, or open or close windows;
  its keys for those go to the main window as menu commands, which the main window runs as
  if chosen from its own menu bar. A Rust test checks that every command the panel's
  scripts name is granted and nothing else is.
  `capabilities/setup.json` grants the Budget Setup window, by its label, only what its
  tabs call: `get_view`, the setup tables (editing, adding, deleting, renaming, order,
  short codes, in-use counts, a global's details and the numbers-on-screen settings),
  charges, credits, info, settings with renumbering, the baseline, and the setup library's
  commands, and listening for and sending events. Nothing on the selected lines (its
  tables have no selection, and a test checks that setup mode never asks for one), no
  notes, find, history, files, undo, tabs or windows. A test in `src/setupwin.rs` compares
  the grant with what its scripts name, both ways. `build.rs` lists those commands in an app manifest,
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
  list, a recovery key or a backup's file name, each checked against what is there, and an
  open budget by its document id, which is only ever a number checked against the open
  tabs. The files reopened at the start come from the app's own `tabs.json`. A report's
  PDF is written where the native Save dialog shown by Rust says; the page can only
  suggest a file name. Exports (`export_budget`) are the same: the page names only the
  format, Rust shows the Save dialog, forces the format's extension, writes atomically and
  refuses a path that is a budget open in a tab. Files to
  compare with are picked in the same native dialog, read in Rust and named by the page
  only by a token Rust gave out (the last 32 picked are kept); versions by their id. The
  same goes for the budget to import setup from (the last one picked) and for templates:
  New From Template and Save as Template show the native dialogs from Rust and only take
  "keep the lines" from the page. Import Spreadsheet is the same: Rust shows the Open
  dialog and reads the file; the page gets its name (never its path) and names it by a
  token afterwards (only the last file picked is kept). Import Actuals is the same, with
  its own token (src/actuals.rs). The setup library is picked or
  made in Rust's Open and folder dialogs and remembered in the app's own `library.json`;
  the page names it never (only its items, folders and a review's token), and every write
  to it is atomic.
  Save a Copy for Sharing is the same: Rust shows the Save dialog (the page can only
  suggest what to leave out and a note) and refuses a budget open in a tab. A shared copy
  carries no path: its provenance names its source by title only, so nothing about the
  sender's disk travels with it, and a returned copy is matched to an open budget by that
  title, in Rust. Read-only is enforced in Rust (`Session::edit` refuses every change to a
  budget stamped as a shared copy), not by the page hiding controls; the stamp is a
  request in an open file format, not a lock.
- **Imported spreadsheets are untrusted** and are read by our own code with limits: a file
  over 50 MB is refused before it is read in full; a ZIP gives out at most 200 MB of
  decompressed data in all, an entry is decoded into at most the size its directory
  declares (more or less, or a CRC that doesn't match, is an error) and an entry declaring
  more than 1,000 times its compressed size (beyond 1 MB) is refused unread; the DEFLATE
  decoder bounds-checks every read, checks every code table and stops at its output cap
  (tested with zlib's streams, randomly built streams, every truncation, bit flips, noise
  and bombs); encrypted entries, ZIP64 and split archives are refused. The XML tokenizer
  refuses any DTD (`<!DOCTYPE`) and any entity but the five predefined ones and character
  references, so nothing is ever fetched or expanded, and limits nesting to 128 levels and
  64 attributes per element. A sheet is read to at most 100,000 rows, 256 columns, 2
  million cells (counting the blanks a row is padded with) and 32,767 characters a cell,
  and a workbook to 128 MB of cell text (a shared string is copied into each cell using
  it); the dialog says when a sheet was cut short. Imported text goes
  into the budget as data (descriptions, names, notes), never as markup, and a cell is kept
  as a formula only when the engine parses it and its globals exist.
  Show Backups Folder runs the system's `open`, `explorer` or `xdg-open` from Rust on a
  folder Rust chose.
- **Budget text is never markup.** The UI builds the page with DOM calls and puts every
  string from a budget in with `textContent` or input values, never `innerHTML`.
  `freezePrototype` is on.
- **Hostile files** can't crash the engine: formulas have length and nesting limits and
  globals are evaluated without recursion (see the engine's hardening tests). A file that
  can't be read shows the reader's message with its line, column and JSON path.
- **Native printing code.** `src/pdf_mac.rs` is the app's only `unsafe` code: a handful of
  Objective-C runtime calls (`objc_msgSend` with an exact function type per call, one small
  delegate class registered once) to run AppKit's print operation on the main window's web
  view, because no crate already in the build offers printing to a file. It never takes a
  pointer or a selector from the page; it runs on the main thread inside Tauri's
  `with_webview`, and the PDF's path comes from the Save dialog.

## Dependencies

Tauri crates are pinned to exact versions and, like every crate in `Cargo.lock`, must be at
least 14 days old when added (`python3 scripts/check-crate-ages.py --only-changed HEAD`).
`tauri` uses `default-features = false` with only `wry`, `common-controls-v6`, `x11` and
`dbus` (no asset compression, no runtime ACL changes). `tauri-plugin-updater` uses
`default-features = false` with only `rustls-tls`: no native TLS, no system proxy lookup,
and no readers for the older archive formats (the Windows zip and the Linux tar; macOS
always reads its `.app.tar.gz`). `reqwest` is named
directly only for the updater's redirect policy; it was already in the build through the
plugin, at the same version. `method-release` (the workspace's own release plumbing:
versions, release notes, the updater key's shape) has no dependencies.

## Not yet

Split view is two panes side by side only (no vertical split, no third pane), with one
tools panel for the target (the reference docks tools under each pane). It needs a live
check: Shift+Cmd+\\ and the View and Window menu items reaching the page in the system web
views, dragging a tab between the strips with a real pointer, the tools window following
the target, and the panes coming back at the next start. The History tab needs a live
check in the tools window and with a long history.

Moving rows by drag, editing fringe ranges in the grid, import from other tools' own formats (spreadsheets import, see
Importing spreadsheets), single-instance handling on Windows and Linux,
the macOS system recent documents list, a mark on copied rows, and the total change a
paste will cause in its review. Printing, the Windows/Linux menu keys, Copy Rows from
the menu bar reaching the system clipboard, and the tools window (placement on a second
screen, keys while it is in front, closing with the main window) still need a live check.
The two tool panes split their height evenly; there is no divider to drag between them yet.
Budget tabs need a live check too: Ctrl+Tab and Shift+Cmd+] reaching the page in the
system web views, the Window menu's list, the close question (Save All with an untitled
budget), reopening the tabs at the next start, and the tools window following the tabs.
Versions and the compare screen need a live check of the Add Files… dialog (several files
at once), of going to a compared row in another tab, and of printing a wide comparison.
What Changed and Merge Changes From need a live check in the app: the menu items, the
Open dialogs, a click on a change landing on the row's cell, a setup tab or the Versions
dialog, and a merge's undo step (their logic is tested in Rust, the dialogs in a browser
harness). Merging doesn't offer itself from a returned copy's banner yet.
Find needs a live check of its keys in the system web views (Cmd+F and Shift+Cmd+F through
the menu, F3 reaching the page on a Mac whose function keys control the system) and of the
Find tab in the tools window.
The setup library needs a live check of its folder and Open dialogs (also from the tools
window), the Library tab in the tools window and at the bottom, and Save to Library in the
setup lists' right-click menus (its logic is tested in Rust, the tab in a browser harness).
Import Setup From, New From Template and Save as Template need a live check of their
native dialogs and of the review with a large library budget (the review's logic and
dialog are tested in Rust and in a browser harness).
Reports need a live check in the app: Print… and Save as PDF… through the native print
operation (checked here only in a hidden test web view: each page box is one sheet,
portrait and landscape, with backgrounds and colors), the print sheet's orientation, and
the setup dialog's keys. Paper is Letter only for now (A4 and Legal would be a layout
choice), and Windows and Linux print with `window.print()` and the page's own `@page` size and zero margins
(untested there; Save as PDF is the print dialog's PDF choice on those systems).
The Presets tab and its right-click submenus need a live check in the app (keys and the
review in the tools window included; the logic is tested in Rust and the tab, forms,
review and menus in a browser harness). Presets follow a rename of a setup item, but
deleting a fringe, group, set or location leaves its name in tag presets (applying one
then says what is missing). Line blocks are reordered from the keyboard only.
Import Actuals and the Actuals tab need a live check: the Open dialog, real exports from
accounting systems (checked here with synthetic CSV, tab-delimited and workbook files and
the dialog, tab, grid columns and Cost report preview in a browser harness against the
real Rust code), the tab in the tools window. The cost columns sit before Total like Orig
and Var, so with the tools panel docked at 1440 pixels the topsheet scrolls sideways to
Total; hide columns from the column menu. Not yet: entering a transaction by hand, an
ETC override for a charge, purchase orders and commitments, actuals by period.
Import Spreadsheet needs a live check: the Open dialog, a real workbook saved by Excel and
Numbers and CSV files from Excel on Windows (checked here with workbooks written by hand
with Python's zipfile and our own exports, and the dialog in a browser harness against the
real Rust preview and import). It reads values, not formulas or styles (a date comes in as
its serial number, a number formatted with leading zeros loses them), maps one column per
field, and keeps a saved mapping only in the web view's storage; 1.16 (the reference's own
exports) will add readers for its layouts on top of it.
Exports need a live check: File > Export's Save dialogs, and the workbook opened in Excel
and Numbers (checked here by reading it back: ZIP CRCs, every part well-formed with
`xmllint` and Python's parser, content types and relationships complete, every cell's
type, style and value matching the CSV and tab-delimited files of the same budget).
Display settings need a live check in the app: Option+Cmd+F, Option+Cmd+= and
Option+Cmd+minus (and Option+Cmd+0) reaching the menu in the system web views, the View
menu's toggles, the zoomed grid's scrolling on a long account, and the Globals details and
Numbers on screen forms in the tools window (logic tested in Rust and in a browser harness).
Report pages keep their own layout: they don't follow the screen's rate decimals, totals'
cents or hidden summary rows yet.
Moving rows and renaming need a live check: Option+Up and Option+Down reaching the page in
the system web views (Alt+Up and Alt+Down on Windows and Linux), the Edit menu's Move Up and
Move Down, and the rename forms and right-click menus in the tools window (all checked in a
browser harness against the real Rust session).
Renumbering accounts, Save a Copy for Sharing, the read-only banner and New Sub-Budget need
a live check in the app: the Save dialog's suggested name, opening a shared copy from the
Finder, the returned-copy question when the original is open, and the renumber preview in
the Budget Setup window (logic tested in Rust, the dialogs, banner and preview in a browser
harness).
The Budget Setup window needs a live check in the app: the window opening, coming forward
and remembering its place and size (a second screen included), the setup keys and Close
Window reaching it from the menu bar while it is in front, Escape closing it with focus back
in the grid, closing with the main window, and the library's dialogs from it (checked in a
browser harness against the real Rust session: every tab, editing there and the other
windows refreshing, following a new tab, the keys and Escape, Create charge and Create
credit, the Setup buttons and right-click item, the renumber preview, and the panel's
compact tabs and saved-layout migration).
