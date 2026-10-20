# The desktop app (`crates/method-app`)

Method Budget's editor is a local desktop app built with [Tauri 2](https://tauri.app):
the Rust engine runs natively inside the app, and the interface is plain HTML, CSS and
JavaScript drawn in the operating system's own web view. Budgets are `.mthd` files on your
disk ([FORMAT.md](FORMAT.md)). There is no server and no account, and the app
goes online only to check for updates of itself, which can be turned off ("Updates" below).
Why Tauri: [decisions.md](decisions.md), entry 14.

## Run and build

You need Rust (stable) and, for bundles, the Tauri CLI (`cargo install tauri-cli --version 2.11.4 --locked`).
There is no JavaScript toolchain: no npm, no bundler, no packages.

```bash
cargo run -p method-app                     # run a debug build
cargo tauri build --debug --no-bundle       # debug binary in target/debug/method-budget-app
cargo tauri build                           # release installers (.app/.dmg, .msi, .deb/.AppImage)
cargo test -p method-app                    # command-layer tests, no window needed
cargo build --release -p method-app --example perf && target/release/examples/perf
                                            # timings on a 5,000-row sample budget (no window)
```

Run the `cargo tauri` commands from `crates/method-app` (or pass `--config crates/method-app/tauri.conf.json`).
The UI files in `crates/method-app/ui/` are embedded into the binary at compile time, so after
editing them, run `cargo run -p method-app` again to see the change.

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
its own undo, autosave, recovery copy and backups; the row clipboard is shared. New…
(Cmd+N) and the toolbar's New open the start window (below, "The start window and New
Budget"); New Tab (Cmd+T) opens a starter budget in a new tab at once, and so does the **+**
button at the end of the strip. Open, Open Recent, a recovered budget and a `.mthd`
double-clicked in the Finder open in a new tab; a file that is open already just has its
tab brought forward, and an untouched Untitled budget alone in the window gives its tab up
to the file. **Open also reads budget exports and spreadsheets** (decisions.md, entry 66;
src/open_any.rs): its dialog allows `.mthd`, `.json`, `.xml`, `.xlsx`, `.csv`, `.tsv`,
`.txt` and `.tab`, and the file's content decides what happens. A budget (any file in our
format, `.json` included) opens in a tab; a budget export is converted: it goes to Import
Budget Export…'s review with the file picked, worded as a conversion (below, "Importing a
budget export"; entry 67), and Convert to Method Budget opens it in a new unsaved tab that
says once where it came from and is first saved beside the export as a `.mthd`; another
spreadsheet goes to Import Spreadsheet's dialog with
the file picked and "A new budget tab" chosen; anything else gets a message saying what
Open reads. The start window's Open…, a file from the system and a file dropped on the
window go the same way. Close Tab (Cmd+W), the × or a middle-click closes a tab: with Autosave on its
changes go into its file first, and whatever is still unsaved asks **Save**, **Close
Without Saving** or **Cancel**. Closing the last tab gives its place to an untouched
Untitled budget and shows the start window over it (below; with the start window's box
turned off, Cmd+W on the last tab closes the window instead). Drag a tab (or
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
   above-the-line), then the summary rows: above and below-the-line totals, the budget's
   fringe row (fringes posted by budget), charges newest first, the grand total, credits in
   reverse application order with a Sub Total when there are two or more, and the net total
   (Budget Setup > Settings > Numbers on screen can leave out the above and below-the-line
   rows and the net total). The budget's fringe row shows its account number and
   description (no number and "Total Fringes" unless Settings > Fringe totals says
   otherwise) and can be placed above any topsheet row instead; it is the same figure
   wherever it shows and never changes a total (decisions.md, entry 60). With **Show
   totals in** set, a last row gives the net total in that currency, written in its own
   separators and symbol (`49.253,40 €`; the currency's editor sets them; entry 61). The
   printed topsheet follows the same order.
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
unknown global without creating it is kept and flagged, as before. Two kinds are refused
as you commit them, with a message, and nothing changes (decisions.md, entry
59): one that divides by zero (`SHOOT_DAYS / 0`), and one that uses a global that refers
back to itself through other globals. A global's formula is refused the same way when it
divides by zero or would make globals refer to each other. A global that does anyway (from
a file) shows #ERR in the Globals table and counts as 0; `method-budget check` still names it.
When the Amt typed is exactly a global with a unit (Budget Setup > Globals, its details)
and the line's Unit is empty, the Unit fills with that unit.

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
fringes, fringe percent, full fringe percent, fringe dollars, groups, set, location and
person can each be shown or hidden, and the choice is remembered for you (in the web view's local
storage, not in the budget). Fringe % is the line's fringe dollars over its total, blank
when the total is 0. **Full fringe %** (hidden until chosen; decisions.md, entry 61) is
the line's fringes before cutoffs and ranges over its total: each percent fringe on the
whole total, rounded on its own, plus the flat fringes as earned, so a line past its
range's cutoff that earns 0.00% still shows its full rate; blank where Fringe % is.
Columns are sized to the window: when there isn't room for every column at its usual
width, they all narrow toward a minimum (numbers keep room for their digits; long text is
cut with an ellipsis, in full on hover) before the grid scrolls sideways, and while it
scrolls, # and Description stay at the left and Total at the right. At 1280 pixels wide
with the tools panel docked at the right, the default columns fit without scrolling (docked
at the bottom, the grid has the window's whole width).

**Person.** One person paid in several accounts can share each percent fringe's cutoff
across all of them (roadmap 3.11; decisions.md, entry 79). Turn on the **Person** column
from the column menu and type the person's name on each of their lines and adjustment rows
(type-ahead offers the people the budget already has; clearing the cell takes the person
off). Spacing and capitals don't make another person: "mara  lee" is Mara Lee, and is
written the way the budget already spells her. Once any line has a person the column shows
by default, even while the totals can't be worked out. A saved line block never keeps a
person. Lines with the same person,
in any account, are one earner: within the cutoff each line earns its own percent; past
it, the person earns the percent of the cutoff once, shared over their lines by pay. The
cell's tooltip is the person's running total: what they are paid, on how many lines in
which accounts, and for each fringe with a cutoff the pay against it ("SUI: 18,420.00 of a
7,000.00 cutoff, reached; 238.00 across the lines", or how much is left); a mark in the
cell's corner shows a cutoff is reached. A line with a person leaves its fringe range for
percent fringes, and Explain This Number shows the person, the cutoff and each line's
share. Budgets that never set a person work out exactly as before. Needs a live check in
the app (the column, the type-ahead and the tooltip are drawn from tested pieces, but not
yet seen in the app's web view).

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
Currencies go on lines; fringes, groups, sets and locations on lines and adjustments
(decisions.md, entry 56); other rows are skipped.
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
the total: its lines at their base, with only the fringes they earn
(decisions.md, entry 58); a currency: its lines' totals in the base currency);
globals and units show the count only. The tooltip on a count also says when other setup
uses the item (a global used by another global or a fringe rate, a unit by a flat fringe,
a group by a credit or a charge). **Show** (All, Used or Unused) narrows the list; used
means lines or other setup name it. The **Name** head cycles the list's sort on each
click: A to Z, Z to A, then back to the budget's own order (in Budget Setup every column
head sorts its column that way, and the rail's **Sort** picks the order too). The budget's order is saved in
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
(percent) or, for a flat fringe, what it is charged per, its **Basis** ("Amount only
(standard)", "Amount, plus counts in X and 4X" or "Full quantity": what a line counts,
decisions.md entry 54) and, when it is charged per hour, **Max hours per day** (blank for
no cap), then the description and short code. A flat fringe that keeps a cutoff (switched
from percent, or from a file) says so in a quiet line under the rate: it is kept but not
used, since cutoffs apply to percent fringes only, and comes back if the fringe turns
percent again. The lists' rate text adds what a flat fringe counts beyond its unit ("1.00
USD per Hour, plus counts in X and 4X, up to 12 hours a day"); a group's
description, short code, "counts in total" switch and line color (No color or one of the eight colors; a color set some
other way is kept unless another is picked); a set's description; a location's
description, city, region and country; a currency's rate, symbol and description. Enter
saves, Escape or Cancel closes it, a refused value keeps it open with the message, and
saving with nothing changed does nothing. Each save is one command, so one undo step (a
group's three fields go together through `update_group`). The editor's **Delete…** asks
before deleting, as the Delete key does. The row at the bottom of every list (**New
fringe…**, **New group…**, and so on; in Budget Setup, **New…** on the action strip) opens
the same kind of form for a new item, name first; a name already in use, or with a comma,
is refused there.

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
Renaming the base currency (Budget Setup > Currency, the band over the table) works the same way. The new name has to be
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

The command is in `src/rename.rs`; the engine does the work (`method_core::rename`).

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

**Budget.** Charges, credits, budget info, settings (fringe posting, calculation, account
numbers, what new lines copy, numbers on screen; the base and totals currency are the
Currency tab's band) and the baseline, as in the setup tabs
before. Settings > Calculation sets the rounding (Spreadsheet (binary), the default, or
Exact decimal), **Round account totals to whole units** (off by default: each account's
total is rounded to the nearest whole unit and categories, charges and the grand and net
totals add up the rounded accounts; lines and fringes keep their cents; decision 69), the
percent fringe cutoff factor (1.0, the default, 1.5, 2.0, 2.5 or 3.0:
every percent fringe's cutoff is multiplied by it) and whether the fringes of categories
after the last production total are kept or dropped; each change is one undo step. With
fringes posted by budget, Settings > Fringe totals also has the fringe row's number,
description and place (after the summary rows, or above any topsheet row); adding or
removing topsheet rows above it keeps it above the same row. A new budget posts fringes
by budget, numbers accounts category 2, account 2, set 3, and starts with a Day of 12
hours, a Week of 60 and a Month of 240 (decisions.md, entry 60). **Charges**: two switches per charge, saved in the budget, add to its topsheet label
(screen and printed): the percent (percent charges, "Contingency 10%") and the amount left
out ("(excluding 25,000.00)"). The charge's **Left out** table lists the typed amounts, the
groups (each with its own amount in the charge's base as the totals stand now: its counted
lines, with their fringes unless fringes post by budget), categories and accounts it leaves
out, and the footer has the total left out (each line once). While a percent, a flat amount
or a left-out amount is being typed, the charge's total in the footer is figured from the
typed text before Enter commits it (`preview_charge`, about a tenth of a second after the
last key); a value that isn't a number yet dims the old total. **Credits**: a row's
**Spend** is an amount typed in, a group's lines, a topsheet category ("Category: 2100
Electric": the category's total with the fringes its lines earn, wherever fringes post;
decisions.md, entry 57) or an account ("Account: 1101 Crew": its lines with the fringes
they earn, wherever fringes post; entry 61). **Fringes left out of the spend**, under the
credit's own fields, is one list per credit: it applies to every group, category and
account row (a typed amount has no fringes), so changing a row's Spend keeps it. Each row shows its spend, the
fringes left out of it, the net spend the percent applies to, the credit (rounded for
display; its tooltip has the exact figure, since rows add up before rounding), and with a
cap, how far the row is under or over it (spend for a cap on the spend, credit for a cap on
the credit). The footer adds the rows up (gross, fringes out, net, credit) and shows the
discount and the net credit taken off the grand total.

**Incentive programs** (roadmap 3.6; decisions.md, entry 80): a credit is also how an
incentive program is modeled, with every rate, cap and qualifying category, account or
group typed by the user (nothing about any real program ships). **Spend at these
locations**, under Fringes left out of the spend, limits the credit's group, category and
account rows to lines at the picked locations, with the fringes they earn; none picked
counts every line, as before. A credit that is not applied is an estimate: the rail shows
"Est." and its figure, and its footer the qualified spend and the estimate, while the
topsheet leaves it out. Under the editor, **Programs side by side** lists every credit with
where its spend has to be, its qualified spend (the tooltip has it by location), the
credit after its caps and discount, and whether it is applied, an estimate or has a
problem; **Spend by location** lists each location's lines, fringes and total, then lines
with no location (the Spend head's tooltip says it can differ from the topsheet when
account totals are rounded to whole units or fringes after the last production total are
dropped). Both come from `incentives`, fetched once per budget revision. When the budget
has problems, the figures come from the cleaned copy that counts them as blank: a note
above the tables says so, and a program with a problem of its own says "May be short",
with the problem in its tooltip and in the footer. Applying
an estimated credit takes off exactly the estimate. Renaming a location renames it in the
credits; deleting one leaves its name in a credit, which then counts nothing there
(`method-budget check` and the MCP `validate` tool warn about it).

A program's terms (decisions.md, entry 88) are a band of their own under Spend at these
locations: **Minimum spend** (below this qualified spend, the rows' net spend at its
locations before any cap, the credit pays nothing; a spend at the minimum qualifies),
**Broker's fee** (a flat amount taken off after the cap and the discount, never taking the
credit below zero), and **Shared cap** with **Shared cap amount**: credits naming the same
shared cap never come to more than its amount together. When they would, each gets a share
in proportion to what it comes to after its own cap, in whole cents that add up to the
shared cap exactly (a cent left over goes to the largest remainder, then by name), and
then takes its own discount and fee off its share. The amount belongs to the shared cap,
so changing it on one credit changes it for all; a new shared cap needs an amount, and
typing a new name keeps the amount shown. A shared cap no credit names any more goes away.
An unapplied credit is estimated as if it were applied alongside the applied ones, so
applying it takes off exactly its estimate (and may lower the others sharing its cap).
The footer of an applied credit adds "Minimum: Not met", its shared cap share and the
broker's fee when they apply. Programs side by side gains **Minimum**, **Shared cap**
(the cap's name, and the program's share when the cap holds it down) and **Fee** columns,
and says "Below minimum" for a program that pays nothing for that reason.

| Command | Arguments | Returns |
|---|---|---|
| `preview_charge` | `index` (`null` for a new charge), `charge` (as `set_charge`) | `{name, base, excluded, amount}`, figured on the current totals; nothing changes |
| `incentives` | none | `{partial, locations: [{location, known, lines, fringes, total}], programs: [{index, name, applied, locations, cap, discount_percent, minimum_spend, broker_fee, shared_cap, shared_cap_amount, amount, problem, qualified, by_location: [{location, net}], rows_total, capped, figures}]}` (`figures` has `below_minimum`, `shared`, the share in cents when a shared cap holds it down, and `fee`, the fee taken off): spend by location (`location` `null` for lines with none) and every credit as an estimate, applied or not; figured on the current totals; `partial` when those come from the cleaned copy of a budget with problems, and then a program's own problems are in its `problem` with its (possibly short) `amount`; nothing changes |
| `clear_from_rows` | `rowIds`, `field` (`fringes` or `groups`) | View |
| `replace_tags_on_rows` | `rowIds`, `tags` (`{fringes, groups, set, location, currency}`: names the budget defines; `null` values and empty lists clear; the base currency is none) | View |

`set_charge` takes `label_percent` and `label_excluded` (left out: the charge keeps its own
setting). `set_credit` takes `locations` (location names the budget has, or ones the
credit already names; empty: every line), and `minimum_spend`, `broker_fee`, `shared_cap`
and `shared_cap_amount` (money text, blank or `null` for none; a new shared cap needs an
amount, an amount sets it for every credit sharing it; decisions.md, entry 88). A credit row's report entry has `cap_variance` (unrounded currency units like
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
  exactly the preset's, so a part the preset leaves empty is cleared. Currencies go on
  lines, fringes, groups, sets and locations on lines and adjustments, as with the lists. Each
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
Credits, Rate Tables, Info, Settings and Baseline** (a tab's tooltip says what it is for). It is laid out
for density: under the tab strip, a narrow **rail** at the left (on the tabs that have a
library kind) and the tab's own area, and at the bottom a **footer** with the tab's totals
at the left and **Close** at the right.

- **The rail** starts with a **Budget** and **Library** switch. In Budget it holds the
  list tabs' **Sort** (the budget's order, by name, or the column a head sorts on) and
  **Show** (All, Used or Unused), and whatever a tab adds there. In Library it holds the
  library's name (with Change… and New…), the kind, the sort and the **folders** (All
  folders, Unfiled, then each folder, with counts; New Folder…, Rename Folder… and Delete
  Folder… under them), and the tab's area shows the library's items of that kind (below).
- **The list tabs** (Fringes, Groups, Globals, Units, Sets, Locations, Currency) are one
  table each, one item per row with every other row shaded: a drag handle, then the
  columns, most edited in place (Enter or leaving the cell saves it as one undo step,
  Escape puts the old value back). Fringes: Name, Description, Rate, Unit (Percent, Flat
  per line or Flat per a unit with hours), Cutoff (percent fringes; a flat fringe's kept
  cutoff shows muted), In use, Total. Groups: Name, Description, In budget total, Color,
  In use, Total, Active total (the dollars that count in the budget total). Globals: In
  use, Name, Description, Calculation, Units, Value, then **…** for its details
  (description, unit, decimals, folder, show by name) and × to delete it. Units: In use,
  Singular, Plural, Description, Hours, ×. Sets: Name, Description, In use, Total.
  Locations: Name, Description, City, Region, Country, In use, Total. Currency: Code,
  Description, Symbol, In use, Rate, Total; its editor also has **Separators** (`1,234.56`
  or `1.234,56`) and **Symbol after the amount**, how totals shown in that currency are
  written (decisions.md, entry 61). Every head sorts its column (A to Z, Z to A,
  then the budget's order). A click on a name selects its row (so does a cell getting
  focus); double-click, Enter or F2 on a name opens its editor (the rest of a fringe,
  such as its basis and short code, and its name: renaming everywhere; a global's or
  unit's name renames it); in the tag tables Delete deletes it and Up and Down move
  along; the right-click menu renames it everywhere, sets a short code, saves it to the
  library or deletes it (a global's has Details… too).
- **The action strip** over each table: **New…**, **Edit…** (the tag tables) or
  **Details…** (Globals), **Delete…** and **Save to Library…** for the selected row, a
  **Search** box that takes the rest of the row (Escape clears it), and **Undo** and
  **Redo** at its right end (sent to the main window, as the keys are).
- **Tab-wide settings** sit in one thin band over the table: on Groups, the three **Group
  rules** (decisions.md, entry 57), each Include or Exclude for the whole budget: **Lines
  with no group** (Exclude leaves out every line and adjustment row that has no group),
  **Lines in a counted and an uncounted group** (Exclude, the default, leaves them out) and
  **Fringes in group totals** (display only: the group dollars in the tables and the
  printed Groups section with or without their fringes); on Currency, the **Base
  currency** (renaming it renames it everywhere) and **Show totals in** (Settings only
  points here).
- **The footer** shows how many items the table lists and, on Fringes, the budget's
  fringe total and its percent and flat parts.
- **In Library** the tab's area is the library's items of the kind and folder as a table
  (a tick box, Name, Definition, Folder), with **Add to This Budget…**, **Move To…**
  and **Save to Library…** and a search over it, the Setup packs section under it, and
  the count and how many are ticked in the footer.

Charges, Credits, Info, Settings (renumbering accounts included) and Baseline are the
forms the tools panel's Budget tab used to hold, drawn in the same layout (strip, bands,
tables, footer), each with its explanation in its tab's tooltip:

- **Charges and Credits** list their items in the rail (name and amount; a click, or Up
  and Down, picks one, and Option+Shift+Up and Down move it), with the New charge or New
  credit form under the list. The strip has **New…** (the form's name box), **Move up**,
  **Move down**, **Delete…** (asking at the top of the editor) and **Save to Library…**
  for the picked item, whose editor fills the area: a band with its own fields (name,
  Applied, type and value or cap and discount, the account it posts to), a band of
  topsheet label switches (a charge) or the fringes left out of its spend (a credit),
  a band that adds to its table (Exclude amount, group, category or account; Add row),
  then the table. The footer has the count and the picked item's figures: a charge's
  **Total charge** and **Left out**; a credit's rows added up (**Gross**, **Fringes
  out**, **Net**, **Credit**), its **Discount** and **Net credit**.
- **Info** is one table (Name, Kind, Value, ×) with **New field…** (a small form: label,
  kind, value), **Delete…** for the selected row and a search on the strip, and the count
  in the footer.
- **Settings** is its groups of rows (Fringe totals, Calculation, New lines, Numbers on
  screen, Account numbers), under a strip with only Undo and Redo.
- **Baseline** has **Lock current totals**, or **Lock again…** and **Clear…**, on the
  strip, the grand and net totals' original, current and change as a table, **Freeze the
  original totals** (while it is on, Lock again… and Clear… are off and say so; edits
  still move the variance; decisions.md, entry 61), and what has changed since (lines and
  accounts) in the footer.

The layout pieces (strip, band, table, footer totals, the rail's own content) are
`ui/js/setupframe.js`, described at the top of `ui/setup.css`, for any tab to use.

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
cursor row and account) and fires `method:selection` and `method:cursor` events on the document
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

The group rules are one command, `set_group_rule` (`rule`: `ungrouped_lines`, `conflict`
or `fringes_in_totals`; `value`: `include` or `exclude`; returns the View), one undo
step each. The cutoff factor is `set_cutoff_factor` (`value`: `1.0`, `1.5`, `2.0`, `2.5`
or `3.0`), account totals in whole units `set_round_account_totals` (`value`: `true` or
`false`), and the budget's fringe row `set_fringe_row` (`row`: `{account, description,
position}`; blank text is none or the default description, `position` `null` or a
topsheet row from 0 up to the number of rows); each returns the View and is one undo
step. `set_credit`'s rows take a `source` of `{kind: "category", category}` or `{kind:
"account", account}` besides `flat` and `group`; the credit itself takes
`excluded_fringes` (one list for all its group, category and account rows).
`set_currency`'s `currency` also takes `symbol_after` (true or false) and `separators`
(`comma_point` or `point_comma`); left out, the currency keeps its own (decisions.md,
entry 61). `set_baseline_frozen` (`frozen`: true or false) freezes or unfreezes the
original totals; refused when there are none; one undo step.

`delete_unit` and `delete_currency` take `clearLines` (default false): blank the unit on
the lines that name it, or put the currency's lines back in the base currency, before
deleting, as one step. `set_fringe`'s `fringe` and `set_group`'s and `update_group`'s
`group` take `short_code` (left out keeps the item's own; `""` clears it). `set_fringe`'s
`fringe` is `{kind, rate, description, cutoff, unit, basis, max_hours_per_day, short_code}`:
`cutoff` is kept on a flat fringe too (never used there); `basis` is `amount` (blank),
`amount_counts` or `quantity`, and `max_hours_per_day` a number above 0 or blank, both
for flat fringes only (a percent fringe ignores them).

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
rounding, by the budget's arithmetic (decisions.md, entry 55: "1.005 is
1.00499999999999989341... as a binary double, so it rounds to 1.00 (spreadsheet rounding:
half up on the binary value)", or "6,125.819 rounds half away from zero to 6,125.82" for exact
arithmetic); a line left out says which
group or filter left it out and what it would be. Each fringe on a line shows percent or
flat, the cutoff and, for a range, how much of it the lines above had left (a range uses
its cutoff up in line order, each line rounded on its own; decisions.md, entry 56); a flat fringe says what the line counts and why ("ZDAY 100.00 per Day x 25 Days
(amount only; 14 Hours in X ignored) = 2,500.00", "x 1 (Allow has no hours: charged
once)"), with a Count line for the hours conversion, the hours-a-day cap or the counts
in X and 4X; a charge its base, each excluded account with the rule
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
them as blank. From a terminal, `method-budget explain file.mthd <total>` prints the same tree (see
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

### Rate tables

Rate tables are yours to keep (roadmap 3.9; decisions.md, entry 90): a named table of
classifications ("Grip, Key", "Grip, Best Boy"), each with a rate per unit (the budget's
own units: Hour, Day, Week) and, when the figures change over time, the dates each row is
in effect, with notes. Nothing comes with the app: there are no bundled rates, union or
otherwise. Tables are kept in the budget, as tools like overtime patterns (versions don't
carry them, restoring one keeps the current tables), and go between budgets the way setup
items do: Import Setup, the setup library (a **Rate tables** kind), setup packs and setup
lists in a spreadsheet (a **Rate Tables** sheet, one row per classification, period and
unit). So you can keep your own tables in your library and bring them into any budget.

**Budget Setup > Rate Tables** (View > Budget Setup > Rate Tables) lists the budget's
tables in the rail; the one picked shows as a grid: Classification, a **Per Day**
(and so on) column per unit, **From**, **To** and **Notes**, and × to remove a row. Typing
changes a draft; **Save Table** keeps it (one undo step) and **Revert** drops it. **Add
unit…** in the band adds a column for another of the budget's units, **Add
classification** a row, **New Table** a table of your own, **Delete Table…** removes one
(lines filled from it keep their rates). Two rows of one classification can't be in
effect on the same day; a row with no dates is in effect every day. The rail's Library
switch shows your setup library's rate tables, to copy in or out.

**Import from Spreadsheet…** reads a sheet you have (.xlsx, CSV or tab-delimited, Import
Spreadsheet's readers and limits) into a new table: the dialog picks the sheet, names the
table (after the sheet or file) and maps the columns, guessed from their titles: a
**Classification** column, then a column per unit (`Hourly`, `Daily`, `Weekly`, `Day`,
`Per Day`, `Day Rate`...) or a **Unit column** and a **Rate column**, with optional
**From**, **To** and **Notes**. Every row is checked and problems are listed with their row
numbers; nothing is imported while any is left. A table of the same name is replaced only
when you choose **Replace Table**. **Update Filled Lines** gives every line filled from a
table what its table says now (one undo step); lines changed by hand are left alone.

**Fill Rate from Table…** (Edit menu, or the line's right-click menu) opens a picker for the
cursor line: every classification and period of every table, each with the rate it gives
for the line's unit (**Rate per** changes the unit), and a search box. A unit the table has
no figure for is converted from the unit closest in hours, only when both units have hours
in the budget, rounded to cents (a 50.00 Hour rate is 600.00 for a 12-hour Day); a row that
can't give a rate says why. Picking one writes its figure into the line's rate once, as a
plain number, and its unit as the rate unit, as one undo step. The line remembers where the
rate came from: its Rate cell has a dotted underline and says so in its tooltip, and
**Explain This Number** on the line says "filled from the rate table ..., at ... per ...".

**A changed table never changes a line.** When a table now says something else for a line
filled from it, the line keeps its figure; its Rate cell gets a mark in the corner and its
tooltip and Explain say "the table now says ...". **Update rate to ...** (the line's
right-click menu and the picker) takes the table's figure, as one undo step. A rate typed
over by hand is yours: the line says it was changed by hand since, and nothing offers to
update it (pick a classification again to refill it). Deleting a table keeps every figure.

| Command | Arguments | Returns |
|---|---|---|
| `rate_table_save` | `table` (`{name, notes, rows: [{classification, rates: {unit: "figure"}, from, to, notes}]}`), `replacing` (a table's name, or null for a new one) | the view |
| `rate_table_delete` | `name` | the view |
| `rate_choices` | `row_id`, `unit` (null: the line's) | `{row_id, unit, units, choices: [{table, classification, period, as_of, rate, table_unit, table_rate, converted, problem, notes}], current}` |
| `rate_fill` | `row_id`, `table`, `classification`, `unit`, `as_of` | the view |
| `rate_update` | `row_ids` (empty: every line whose table changed) | the view |
| `rate_import_pick` | none (shows Rust's Open dialog) | `{token, file, sheets, index, mapping, titles, units, name, exists, table, errors}`, or `null` |
| `rate_import_read` | `token`, `index`, `mapping` (null: the guess), `name` | the same, read again |

The view's `rate_checks` (left out when empty) gives, for each line of the account on
screen filled from a table, its `state` (`current`, `changed` with `now`, `edited`,
`missing`) and `words`. The main window may call all of these; the Budget Setup window
the five its tab uses. From a terminal, `method-budget rates` (docs/CLI.md); agents have
MCP tools (docs/MCP.md).

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
| New budget or a recent one (the start window), New tab, Open (in a new tab) | Cmd+N, Cmd+T, Cmd+O | Ctrl+N, Ctrl+T, Ctrl+O |
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

### Screen readers and contrast

The keys above are the same with a screen reader on; nothing here changes them (decision
82; what was checked and what is left: docs/research/accessibility-audit.md).

- **The grid** is an ARIA grid. Focus stays on the grid itself and
  `aria-activedescendant` points at the cursor cell, so a screen reader follows the
  cursor as it moves. Each cell says what it shows (or "blank"), its column (the full
  name for a short label: "X (multiplier)") and the row's description ("45.00, Rate,
  Gaffer"), and a problem in it. Totals and other computed cells are read only; every
  cell of a selectable row says whether it is selected (the cells of a selected range
  or of picked rows are; the cursor on its own isn't). Each cell's id comes from its row
  and column, so the cursor keeps pointing at the same cell when the grid redraws or
  scrolls. Rows and cells give their
  place (row 212 of 400), also in a long grid that draws only the rows near the visible
  ones. The description column is the row header. The cell editor is named by its
  column and row.
- **Totals after an edit**: when an edit moves the net total, a screen reader hears
  "Net total ..., up ..." (or down) a moment later, politely; a run of quick edits is one
  sentence. The words come from a hidden live region (`#announce`, `ui/js/announce.js`).
  Messages (the toast) are alerts; banners and the Find count are status.
- **Dialogs** are labelled modal dialogs: the page behind is inert, Tab stays inside,
  Escape cancels and focus goes back to what opened it.
- **Contrast**: text and focus rings follow docs/BRAND.md's rules (4.5:1 for text, 3:1
  for focus rings and borders that carry meaning) in light and dark. Every focused control
  shows a 2-pixel accent ring.

`scripts/web-smoke.mjs` checks this headless in the browser build, through Chrome's
accessibility tree: every control has a name, the grid's roles and the cursor cell's name,
the announcement after an edit, and a dialog's role, its hold on focus and focus coming
back.

### The Help menu

In order: **Keyboard Shortcuts**; **User Guide** (this file on GitHub, in your browser);
**Open Sample** (below, "The sample budget");
**Release Notes** (this version's notes, below under "Updates"); **Check for Updates…** and
**Check for Updates Automatically** (also under "Updates"); **Report a Bug…**; **Licenses and
Notices**; **methodproduction.com** (the suite's website, in your browser). On Windows and
Linux **About Method Budget** comes last; on macOS it is in the app menu. About shows the
version, the website (methodbudget.com) and the licence, MIT OR Apache-2.0.

- **Report a Bug…** opens a new issue on the app's GitHub repository in your browser, with
  a short note on what to write and a last line naming the app's version and your system,
  for example `Method Budget 0.2.1 · macOS 15.6 (aarch64)`. The system's version comes
  from `sw_vers` on macOS, `ver` on Windows and `/etc/os-release` on Linux (the system's
  name alone if that fails). Nothing from a budget is ever put in it, and nothing is sent
  until you submit the issue yourself on GitHub. The body is form-encoded, kept under
  about 2,000 characters, and any message passed with it has the home folder and file
  paths taken out first (src/help.rs, ported from Method Reader).
- **Licenses and Notices** opens a dialog with the MIT License, the Apache License 2.0 and
  the third-party notices (`THIRD-PARTY-NOTICES.md`, written by `cargo xtask notices`),
  one at a time, as plain text you can scroll. The three files are built into the app from
  the repository's root; the page asks for them (`licenses`, main window only) when the
  dialog first opens, and draws them with `textContent` (ui/js/licenses.js), never as
  markup.
- **Opening links.** User Guide, Report a Bug… and methodproduction.com are answered in
  Rust (src/help.rs), never by the page, and open only those addresses: the two fixed
  ones, and the new-issue address followed by a body made of form-encoded characters
  only. Anything else is refused before a program is started. The link goes as a single
  argument to the system's opener: `open` on macOS, `rundll32 url.dll,FileProtocolHandler`
  on Windows (no shell in between) and `xdg-open` on Linux. A link that can't be opened
  does nothing (a line on standard error).

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

### The sample budget

The app comes with one invented budget, **Field Station** (SUITE-STANDARDS.md, section 8;
decisions.md, entry 84): an independent feature of about a million dollars, budgeted from
Method Schedule's schedule of the same production (fixtures/field-station/, made by
`scripts/make-field-station.py`). It uses globals set from the schedule's days, fringes
with cutoffs and a flat hourly one, a person paid in two accounts, groups (one left out of
the total), locations and sets, lines in euros, a contingency and a completion bond, a
county incentive program worked out but not applied, dated notes, frozen original totals,
an earlier draft kept as a version, and actuals for a cost report. Every name and figure
is made up.

- **At the first start.** When the app starts with nothing to open (no tabs from last
  time, no file handed over, no recovered budget offered), has never shown the sample,
  and has no budgets of yours on the recent or pinned lists, it opens Field Station in
  place of the untouched Untitled budget, instead of the start window. That happens once:
  showing the sample marks it seen (`sample_seen` in the data folder's `settings.json`),
  and a start never opens it again. A start that finds budgets of yours, or something to
  open or recover, marks it seen without showing it, so someone already at work never sees
  it unasked, even after clearing the recent list. There is no setting for it
  (SUITE-STANDARDS.md, section 8; decisions.md, entry 86).
- **Always a copy.** The sample opens as an untitled copy of the file built into the app,
  never the file itself: its tab says **Sample: Field Station**, and a quiet line above
  the grid (the converted-budget banner, closed with ×) says it is an invented sample, that
  nothing is kept unless you save it, and that New… in the File menu starts a budget of
  your own. It starts with nothing to undo and nothing unsaved, so closing it asks
  nothing, and a budget you open or make next takes its tab, as with an untouched
  Untitled budget. Saved as a file, it is yours: the mark and the line go.
- **Help > Open Sample** opens a fresh copy any time (in a new tab, or in place of an
  untouched Untitled budget alone in the window). Starting the app with `--open-sample`
  (Method Launcher's way; "Command-line options" under Saving and recovery) does the same
  at the start, instead of the start window.
- **The logic** is method-session's `sample` module (the decision and the copy, no
  window); the app bundles the file with `include_bytes!` (src/sample.rs). The browser
  build's demo at `/try/` starts with the same file (docs/WEBSITE.md, "The demo").

| Command | Arguments | Returns |
|---|---|---|
| `sample_at_launch` | none (the page asks after the files to open and the budgets to recover) | the sample's view when it was opened (always, once, after `--open-sample`), else `null` |
| `sample_open` | none | the view of a fresh copy of the sample, in front |

### The start window and New Budget

**The start window** (roadmap 3.19; decisions.md, entries 63, 65, 77 and 91; design and
later phases: docs/research/new-project-window.md) shows when the app starts with nothing to
reopen (no tabs from last time, no file handed over by the system, no recovered budgets to
offer), over the untouched Untitled budget; when the last tab closes, over the untouched
Untitled budget that takes its place; and any time from **File > New…** (Cmd+N) or the
toolbar's New. Down the left are the places: **Pinned and recent**, then each **project
folder** (below), then **Add Folder…**. Under Pinned and recent, **Pinned** budgets come
first, then **Recent** (Open Recent's list, without the pinned ones). Each row has the
budget's thumbnail (below), its title, its file name and folder (the home folder written
`~`), its net total in its base currency, when the file last changed, "Open in a tab" for
a budget that is (its unsaved changes are in the total), and "Can't find this file." or why
it can't be read. Typing searches both lists (every word must be in the
title, the file name or the folder). **Sort** beside the search box (decision 91) puts the
lists, and a project's, in another order: Last opened (as listed: pinned as pinned,
recent newest first, a project by title), Name, Last changed (newest first) or Net total
(largest first), rows without one last; a row shown under a budget stays with it. The
choice is kept for next time. A budget with a change log beside it (`X.mthd-log/`, shared
through a folder, decision 64) says "shared" and, on a line of its own, who was in it
lately and where, latest first, from each person's heartbeat in the log ("Bo in account
2101, Today 09:00; Ann, Yesterday"; at most five people). The start window only reads the
log folder; it never writes to it. Keys: Down from the search box into the list, Up and
Down, Home and End move, Enter opens (in the search box, the first match), P pins or
unpins, D duplicates, Delete or Backspace takes a budget off the list (recent and pinned;
the file is never touched), Left goes to the places and Right (from a place) back to the
list, any other letter goes back to the search box, Escape clears the search and then
closes the window. In the places, Up and Down move and Enter or Space shows that place. A
row's own buttons (Pin or Unpin, Duplicate, Show in Folder, Remove) show under the pointer
and with the keyboard on the row. **New Budget…** (Cmd+N in the window)
opens the form; **Open…** (Cmd+O) shows the Open dialog and keeps the window if it is
cancelled. "Show this window when the app starts and when the last tab closes" is kept in
this window's local storage; turned off, the window shows only from New, and Cmd+W on the
last tab closes the window. A pinned or recent budget moved or renamed in the Finder is
found again at its new place (macOS keeps a bookmark with each; one moved to the Trash
counts as gone); on Windows and Linux it shows as "Can't find this file.".
Opening or making a budget closes the window; one opened and an untouched Untitled budget
alone in the window gives its tab up to it, as Open does. A budget opened another way
while the window shows (a file from the system) closes it too. New Tab and the **+** still
make a starter budget at once.

**Thumbnails** (decision 91): each budget's row starts with a small drawing of its topsheet,
a bar per category in topsheet order, each as tall as its share of what the categories
come to (the tallest bar the full height; a category below zero counts as nothing). With
more than 12 categories the 11 largest keep their own bar, in their places, and the rest
share the last one ("8 more"). Under the pointer a bar names its category and share. The
bars are worked out when the row's total is (from the file, or from its tab when it is
open), kept with the row's summary in the app's data folder and read again only when the
file changes. Nothing is drawn from the screen, and nothing is written into the budget.

**Duplicate** (D, or the row's button; decision 91) writes the budget file again beside
itself under the first free name ("Big Show 2.mthd", then "Big Show 3.mthd"…), as it is on
disk (a tab's unsaved changes are not in it), never over another file, and opens the
duplicate in front as a saved budget listed in Open Recent; the title stays the same, so
rename it in Budget Info. A file that isn't a readable budget is refused.

**Projects** (decision 91; design section 5): a project is a folder. **Add Folder…** picks
one in the folder dialog, and New Budget's **Make a project folder** box makes one; each is
listed down the left by its name, in the order added, at most 50. Choosing a project shows
what is in the folder and in the folders directly in it (at most 300 files and folders are
looked at; past that a line says so; hidden files and change-log folders, `X.mthd-log`, are
passed over): each budget by title, with the sub-budgets and copies made from it (by their
source title, as a returned copy finds its original) indented under it and named so
("sub-budget", "shared copy", "editable copy"); then sub-budgets and copies of budgets not
in the folder, setup packs, schedule days files (a `.json` file that says it is
`method-schedule-days`) and actuals exports (`.csv`, `.tsv` or `.xlsx` in a folder named
`actuals`). Budgets open, pin and duplicate as in the other lists; a schedule or actuals
file is shown, and Enter shows it in its folder. The project's own buttons: **New Budget
Here…** (the form, saving in this folder, without a folder of its own), **Show Folder**
and **Remove from List** (the folder and its files are never touched). A folder that has
gone says "Can't find this folder." with Remove from List. Nothing is written into a
project folder except what is made there.

**New Budget** asks for a title and, all optional: the production type (Feature, TV
episode, TV season, Commercial, Documentary, Other; a season and an episode number for an
episode, a season and the number of episodes for a season), the prep start, shoot start,
shoot end and wrap end dates (`YYYY-MM-DD`), the schedule's length (prep weeks, shoot days,
wrap weeks), the country, region or state and city, the base currency (blank: USD; the last
one used is offered next time), the chart of accounts, where to save, how fringes post and
the arithmetic.

**Chart of accounts**, right under the title and type: **A chart of accounts**, **Starter
budget** (a small sample with placeholder rates), **Empty** or **A budget file** (any
budget file, picked in the Open dialog and read from its tab when it is open, as New From
Template uses one). For a chart, every bundled chart is listed by budget type (Feature,
Television, Commercial, Other; the chosen type's group first), our own charts first, then
the studios' and networks' by name (numbers, names and levels only), each with its
description and number of accounts; a search box narrows the list (Down or Enter goes into
it, arrows move). Until something is picked by hand, the type picks our own chart for it
(feature, one-hour TV, commercial, documentary). A line under the choice says how many
accounts and categories it has.

**Setup to bring in** (decision 77), under the chart: a box for **Everything in my setup
library** (with its item count, or why it can't be read; not shown until a library is
picked in the Library tab) and one for each setup pack in the Packs list (the user's Packs
folder, then the packs that come with the app), with its version, effective date, region
and item count. Each ticked source brings every setup item it has, with what each needs,
through Import Setup's import, the library first and then the packs in the list's order;
a pack's items are stamped with the pack as the Packs list stamps them. The budget
settings never come from them: they come from the form. What is ticked replaces the
starting point's items of the same name (the starter's or a chart's defaults); two ticked
sources that define the same item differently are refused with a sentence naming the item
and both, and nothing is made. The setup comes in before the details typed in the form, so
a typed Shoot days wins over a library's `SHOOT_DAYS`. The sentence after Create ends with
what came in ("Setup from your library and Union Basics: 12 items added, 1 item
replaced."). Nothing in a pack or a library is bundled rate data: packs ship empty
(decision 50) and a library holds only what the user put in it.

**Link a Schedule…** (under the schedule's length; decision 91, with roadmap 3.8) picks the
days file Method Schedule writes for a budget (`method-schedule-days`, as File > Import
Schedule Days reads it). The form's Shoot days, Shoot start and Shoot end take its
figures (they can still be changed), and a line says which schedule it is and how many
shoot days it has; **Unlink** takes it back. At Create the file's shoot days in all go to
`SHOOT_DAYS` linked to the schedule exactly as Import Schedule Days links a global (its
setup source names the figure and the schedule), so a later import finds it and says what
changed; its prep start and wrap end, when it has them, go to those budget info fields. All
of that comes in after the setup and before the form's details, so a value typed in the
form wins (the global stays linked). The other figures (units, sets, locations, weeks,
elements) are picked later in Import Schedule Days. The budget file gets no new field.

**Save in**: the folder the last new budget was saved in, else Documents, else the home
folder; **Choose Folder…** changes it. **Make a project folder for it** (decision 91; on
unless turned off, which is remembered; off for New Budget Here) makes a folder named like
the file in that folder, saves the budget in it (`Big Show/Big Show.mthd`) and lists the
folder with the projects. The file name follows the title (characters a file name can't
have become spaces) until it is typed by hand; `.mthd` is added. Enter in a text
box, or Cmd+Enter anywhere, creates; Escape goes back to the lists keeping what was typed.
Where things go:

- the title is the budget's title;
- budget info fields **Production type**, **Season**, **Episode**, **Episodes** (a
  number), **Prep start**, **Shoot start**, **Shoot end**, **Wrap end** (dates),
  **Country**, **Region** and **City**, each only when filled in;
- the schedule's length sets the globals `PREP_WEEKS`, `SHOOT_DAYS` and `WRAP_WEEKS`
  (the starter's own names, so its formulas and its `SHOOT_WEEKS` follow);
- the currency becomes the base currency (refused when the budget started from lists it as
  a foreign currency);
- fringe posting (one total for the whole budget, the new-budget default; an account at the
  end of each category; a line at the end of each account; a row above each production
  total) and the arithmetic (spreadsheet, the default, or exact decimal).

A chart or a budget file starts without lines, info, baseline, versions or notes (as New
From Template). Every value goes through the setup tabs' own checks, and a refusal is shown
in the form with nothing made. The budget is written into the folder as a new file and
opens in front as a saved budget, listed in Open Recent; a message says how many
categories, accounts and lines it has and where it was saved. An existing file is never
written over: when the name is taken nothing is made, and the form says so and offers the
first free name ("Big Show 2") in one click; with a project folder, a folder or a file of
that name in the folder counts as taken. With no folder to offer (none of the three is
there), the budget opens with no file (the first Save asks where, suggesting the title),
as one undo step, so closing it asks to save.

| Command | Arguments | Returns |
|---|---|---|
| `start_list` | none | `{pinned, recent, projects, fresh}`: each row `{token, file, title, folder, modified (ms since 1970), net_total (cents), currency, bars, kind, openable, under, shared, people, pinned, recent, open, problem}` (`bars` the thumbnail, `[{label, share}]` with `share` in thousandths; `shared` when a change log is beside it, `people` then `[{name, place, at}]`, latest first; `kind` `budget`, `sub-budget`, `shared copy`, `editable copy`, `setup pack`, `schedule` or `actuals`; `openable` false for a schedule or actuals file; `under` for something shown under the budget it came from); each project `{token, name, folder, items, more, problem}`, its rows' `file` the path inside the folder; `fresh` when only an untouched Untitled budget is open |
| `start_open` | `token` (a budget file) | View of its tab |
| `start_duplicate` | `token` | `{view, message}`: the duplicate in front |
| `start_add_project` | none (shows the folder dialog) | the lists again, or `null` when cancelled |
| `start_remove_project` | `token` (a project) | the lists again |
| `start_project_folder` | `token` (a project) | `{token, folder}` for New Budget's Save In |
| `start_pin` | `token`, `pinned` | the lists again |
| `start_forget` | `token` (off the recent list and unpinned) | the lists again |
| `start_reveal` | `token` (shown in Finder, Explorer or the file manager; a project folder is opened) | nothing |
| `new_budget_pick_template` | none (shows the Open dialog) | `{token, file, title, categories, accounts}`, or `null` when cancelled |
| `new_budget_charts` | none | every bundled chart: `[{id, name, group, description, ours, categories, accounts}]` |
| `new_budget_setup` | none | `{library, packs}`: `library` `{name, items, problem}` or `null` when none is picked; `packs` the Packs list's rows (`{token, file, place, title, version, effective, region, items, problem}`), listed again (tokens from an older listing are refused) |
| `new_budget_folder` | none | Save In's first folder: `{token, folder}` (`folder` to show, `~` for home), or `null` |
| `new_budget_pick_folder` | `current` (a folder token, or `null`; shows the folder dialog from there) | `{token, folder}`, or `null` when cancelled |
| `new_budget_create` | `spec` (`{title, kind, season, episode, episodes, prep_start, shoot_start, shoot_end, wrap_end, prep_weeks, shoot_days, wrap_weeks, country, region, city, currency, start, chart, template, fringe_posting, arithmetic, folder, file_name, library, packs, project, schedule}`; `start` is `starter`, `blank`, `chart` with `chart` an id, or `file` with `template` the picked token; `folder` a folder token or `null` for no file; `file_name` blank for the title; `library` true to bring in the setup library, `packs` the tokens of packs from `new_budget_setup`; `project` true to make a project folder in `folder`; `schedule` the token of a days file picked with `schedule_days_pick`, or `null`) | `{view, message, taken}`: `taken` (a free file name, without `.mthd`) when a file of that name is there, and then nothing was made (`view` is `null`) |

The pinned list (at most 100), each file's bookmark, the last Save In folder and the
project folders are the data folder's `start.json`. A file is read once for its row and
read again only when its modified time or size changes; the summaries (title, net total,
currency, thumbnail, kind and source title, with the modified time and size they were read
at; at most 500) are kept in `start-cache.json` (version 2; an older one is read as empty,
so every file is read once more), so a new run doesn't read every file again. A token names
a row or project of the last listing only (the picked file: the last one picked; the
folder: the last one offered or picked).

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

New From Template also lists the **bundled charts of accounts** under "Charts of accounts",
grouped Feature, Television, Commercial and Other: our own feature, low-budget feature,
one-hour and half-hour TV, commercial, music video and documentary charts, and any studio
chart built into the app from a source the owner supplied (docs/TEMPLATES.md). Picking one
opens it like a template file: a new Untitled tab with its categories, accounts and
production totals, no lines and no setup.

**Import Chart of Accounts…** (File menu) brings in any chart: from a spreadsheet (.xlsx, any
sheet), a CSV or tab-delimited file, or another budget file (its own chart). For a
spreadsheet the dialog works like Import Spreadsheet: the sheet, the header row and a
column for each field (category number, category name, account number, account name,
level), guessed from the titles and the cells; a file in the `kind,code,name,parent` layout
needs no columns. It reads flat lists, outlines, side-by-side category and account columns,
numbers and names in one cell, numbers with or without separators or with lost leading
zeros, and skips rows above the header, repeated headers and total rows; level headings
and level totals become production totals (our default ones are offered when the file
names none). The numbering pattern the numbers follow is shown and can be changed (widths
and separator). The preview lists the chart as it will come in, the rows skipped and what
to check (repeated numbers, accounts without a category row, empty categories, folded
sub-accounts). A file in the `kind,code,name,parent` layout keeps every number as written
and puts each account in the category it names. An account number listed again with
another name comes in under a new number (`10950-2`): a sentence at the top of the preview
says how many, and they are listed under **Renumbered**; the same number and name listed
twice comes in once, and accounts named `.` (unused slots) are left out, each said in a
sentence (docs/TEMPLATES.md). Then either:

- **A new budget tab with this chart and no lines** (titled Untitled, in this budget's base
  currency; the import is its first undo step, so it starts unsaved), or
- **This budget**: categories and accounts it lacks are added in number order, the ones it
  has are renamed by number, numbers are carried over to this budget's pattern (ones that
  don't fit are listed and left out), production totals are left alone. Accounts the chart
  doesn't list are kept; "Also remove empty accounts the chart doesn't list" removes those
  with no lines and no notes. An account with lines is never removed, and the preview lists
  them. One undo step.

| Command | Arguments | Returns |
|---|---|---|
| `import_setup_pick` | none (shows the Open dialog) | the review, or `null` when cancelled |
| `import_setup` | `token` (from the review), `picks` (`[{kind, name}]`, settings with an empty name), `resolutions` (`[{kind, name, choice}]`, choice `keep`, `imported` or `rename`) | `{view, message, added, replaced, kept}` |
| `template_new` | `keepLines` (shows the Open dialog) | View of the new tab, or `null` when cancelled |
| `template_save` | `keepLines` (shows the Save dialog) | `{name, path}`, or `null` when cancelled |
| `chart_templates` | none | the bundled charts `[{id, name, group, description}]` |
| `chart_template_new` | `id` | View of the new tab |
| `coa_import_pick` | none (shows the Open dialog) | `{token, file, kind, sheets, sheet, fields}` (kind `xlsx`, `csv`, `tsv` or `budget`), or `null` when cancelled |
| `coa_import_preview` | `token`, `request` (`{sheet, header_row, mapping, detect_header, guess, coding: {category, account, sub, separator} or null, sub_accounts, default_levels, target, remove_unlisted}`, `target` `"current"` to apply to the budget) | the preview: `{budget_file, kinded, header_row, mapping, columns, raw, inferred, coding, rows: [{kind, code, name}], categories, accounts, levels, skipped, warnings, totals_skipped, sub_accounts, padded, levels_from_source, default_levels_added, apply, truncated}`; changes nothing |
| `coa_import` | `token`, `request` | `{view, message}` |

The review has `token`, `source` (the file's or tab's name), `path`, `notes` and `items`
(`[{kind, name, status, imported, here, rename_to, needs}]`: kind `global`, `fringe`,
`group`, `unit`, `set`, `location`, `currency`, `charge`, `credit`, `tag_preset`,
`line_preset`, `report_preset`, `account_coding`, `fringe_posting`, `fringe_row` (the budget's
fringe row's number and description; its place stays this budget's), `cutoff_factor` or
`new_line_defaults`;
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

**New Sub-Budget from This Category** (the topsheet's context menu on a category row) and
**from This Account** (the account list's, on an account row; decision 91) make a budget
of that one row, keyboard first with Shift+F10 or the menu key. What carries over,
exactly:

- the row: the category with all its accounts, or the account under its own category
  (the category's other accounts left out), with every line, subtotal, adjustment and
  separator in them and their notes, fringes, groups, sets, locations and units;
  production total rows stay in place;
- the setup those lines use, and what that setup needs in turn: fringes, globals (a
  global another kept global names comes too), units, groups, sets, locations and
  foreign currencies; anything the lines don't use is left out, with its folder, order
  and source;
- the budget settings and details: base and totals currency, arithmetic, fringe posting
  and the fringe row, cutoff factor, account numbering, new-line defaults, group rules,
  rounding, budget info, display settings and overtime patterns.

Left out: every other category and account, charges and credits (they work on the whole
budget), tag, line and report presets, versions, the baseline and actuals. So the row's
category, account and fringe totals are the same as in the budget it came from, and its
bottom line is the row and the fringes it earns (a fringe cutoff shared with lines outside
the row is the one exception: the kept lines are counted alone). It is titled `<budget> -
<row name>` ("Big Show - Camera"), stamped as a sub-budget of its parent with a note naming
the row, and saved as its own file beside the budget's file (the first free name when
that one is taken, never over a file), opened in front as a saved budget. A budget with no
file yet, and the browser build, open it in a new tab with no file.

| Command | Arguments | Returns |
|---|---|---|
| `share_copy_save` | `version` (id or `null`), `leaveOut` (`{notes, info, versions, baseline, unused_setup}`), `note`, `saved` (the page's local time, ISO 8601); shows the Save dialog | `{name, path}`, or `null` when cancelled |
| `share_copy_editable` | none | View of the new tab |
| `share_copy_original` | none | `{doc, label}` of the open budget the copy came from, or `null` |
| `share_copy_store` | `original` (its doc id), `name`, `created` | View of the original (one undo step there) |
| `sub_budget_new` | `spec` (`{categories, accounts, groups, sets, locations, skip_empty_accounts, drop_fringes, drop_notes}`, each tag kind `{values, include_untagged}`), `saved` | View of the new tab |
| `sub_budget_from_row` | `row` (`{kind: "category" or "account", code}`), `saved` | `{view, message}`: the sub-budget in front |

### Sharing a budget

**Share…** (File menu, and the Share button at the right of the toolbar; roadmap 3.18,
decisions.md entries 62 and 64; the design is docs/research/collaboration.md) lets
several people work in one budget through a folder they already share with a sync
service. The dialog asks how people get each other's changes (through a shared folder;
a live link is listed as coming later), where the folder is (the budget's own folder, or
one chosen in Rust's folder dialog, in which case the file is moved or copied there after
a confirmation), and this person's name and color, kept for every shared budget beside
this computer's sharing id (`~/.method/person.json` and `~/.method/actor`, the same id
the command line's `method-budget log` and `method-budget sync` use, so one computer is one actor
whatever program edits the budget). Start Sharing makes `Feature.mthd-log/` beside the
file (docs/FORMAT.md, "The change log") and this computer keeps the file. A budget must be
saved first; a copy saved for sharing (read-only) can't be shared this way.

**Opening a budget that has a change log beside it joins it**: the merged state of
everyone's changes replaces what the file held, a notice says what that means (once per
budget), and from then on every edit is recorded as a change and shared. Nothing is
sent anywhere: this computer writes its own change files into its own folder under
`Feature.mthd-log/actors/`, once typing pauses for a second and a half (at least every
six seconds while it goes on), and reads the others' every second. Changes made offline
pile up in the folder and go out when the sync service is back. Rows made in a shared
budget get random ids from a range nobody else can make, so two people adding lines
apart never clash. Renames and renumbering are recorded as one write (the commands pass
hints), so everything that names the item follows.

**There is nothing to save.** Everything typed is kept; Save puts this person's changes
in the folder now, and Autosave leaves a shared budget alone. Only one computer, the
keeper (the one that started sharing, shown as "keeps the file"), rewrites the `.mthd`
from the merged state once the budget has been quiet for two seconds, so a sync service
never sees two writers on one file. The keeper also notices the file changed by another
program (an older version of the app, `method-budget set`, an agent) and records the
difference as a change by itself, labeled "Outside edit to Feature.mthd". Save As is
refused in a shared budget (File > Save a Copy for Sharing makes a copy); the close and
quit questions never ask about a shared budget whose changes are in the folder.

**The status item** in the toolbar says, in plain words, where things stand: "Synced
through the folder · 12 s ago", "Sharing 4 changes…" (not yet in the folder), "Waiting
for 2 changes that haven't arrived" (a change depends on one the sync service hasn't
brought yet), "Can't reach the shared folder", and an amber "2 to review". Clicking it
opens a panel: the folder and file, who keeps the file and when it was last written,
how many of your changes aren't shared yet, each person with where they are and when
they were last seen, and buttons to the Review and Activity tabs and the Share dialog.

**Presence.** Each computer rewrites a small heartbeat file in its own folder every
twenty seconds (and when the person moves to another category or account) with its
name, color and where it is; nobody's keystrokes leave the computer. The toolbar shows
the other people as initials in their colors (faded when their heartbeat is more than
two minutes old; hover for where they are and when), and the topsheet and account list
show initials next to the categories and accounts people are in. Folder mode shows no
cursors: heartbeats are too slow to be honest about a cell.

**Changes arriving** never move your cursor, selection or scroll. The cells (or rows,
accounts and categories) someone changed get a brief wash of that person's color and
an outline, with their name and what they did as the tooltip, and a dot at the start of
the row until the cursor lands on it; the cell being typed in is never touched, and
what is typed there wins as a normal later write. With reduce motion on, the wash is a
plain short highlight.

**Changed by two people.** Last write wins and nothing blocks; the losing value is never
lost. The cell gets an amber outline and a "!" badge; clicking it shows both values with
who wrote each and when, and Keep (a write that settles it) or Use the other value (a
new change, shared like any). The **Review** tab of the tools panel lists everything a
person should look at after other people's changes arrive, each in a sentence with Go
there and the actions that settle it: values changed by two people, an item deleted by
one person while another edited it, a line moved by two people, a deleted item kept
because something still uses it, two items with one name made apart, account numbers
that repeat or don't fit the numbering, and globals that refer back to themselves (the
engine's review list, decisions.md entry 62).

**Who changed this** (right-click a cell, or Edit > Who Changed This…) lists every value
the cell has had since the budget was shared, newest first, with who typed it, when and
the step that did it, and puts any of them back as a new change. It works on cells
people type in (a line's description, amount, unit, X, rate, 4X, currency, fringes,
groups, set and location; an account's or category's number and name; a production
total's name), not on totals. The **Activity** tab lists everyone's changes newest
first, grouped by hour, with the person's initials, what they did and where (Go there),
and a filter by person.

**Undo in a shared budget takes back only your own changes.** Each is put back field by
field where your value is still the current one; a field someone changed after you is
left as they set it, and a message says so ("Undid "Edit rate on 2300 line 2". Line 2 in
2300's rate was not put back: Jordan Lee changed it after you."). Redo is the undo of the
undo. The History tab lists your steps only; saving a version from a past step is
refused while a budget is shared (versions are saved from the budget as it is now).

**Who may change what** (decisions.md, entry 78). The person who started sharing owns
the budget. Until the owner sets what someone may change, everyone who opens the budget
edits everything, as before. In the Share dialog the owner, and anyone the owner makes a
co-owner or an admin, picks each person's role under their name: Co-owner (sets every
role, admins included, but the owner's and other co-owners'; only the owner gives it),
Edits everything and sets roles (an admin; only the owner and co-owners make or change
admins), Edits everything, Edits some categories
(a checklist of the topsheet's categories: that person adds, changes, moves within and
deletes lines and notes there and applies existing fringes and groups, and changes nothing
else), Actuals and notes, Notes only, or Read only. Set saves it as a change shared like
any other. The first role set asks first, because from then on someone without a role
reads only; the dialog names who that would be, says that everyone sharing the budget then
needs this version or later (an older version won't join it: it says the shared changes are
newer than it reads and opens the file on its own, and what is changed there isn't shared),
and offers to make someone an admin at the same time (or Not now), since only the owner's
computer, co-owners and admins can set roles. The question shows in place under the
person, not as a second dialog. Categories ticked for any role other than Edits some categories are refused
with a sentence, in the dialog and in Rust.
Under the people the dialog also says plainly that roles stop changes by accident, not
someone who edits the shared folder's files directly, and that signed changes come later. A person given a role before they ever
opened the budget is listed too. The People list shows each person's role in words; a
person who may change less than everything sees a chip in the toolbar ("Some categories",
"Actuals", "Notes only", "Read only") with the details on hover. An edit beyond the role
is refused before anything changes, with a sentence that says what the role allows and
who sets roles ("You can change lines and notes in 2000 Production only. Alex Kim sets
who may change what."); a read-only person's every edit, undo and redo is refused that
way. Every computer also checks everyone else's changes as they arrive and leaves out one
its author's role didn't allow; the status panel lists what was left out (the latest 20),
each with who, when, what it was and why in a sentence. Each change is
judged by the roles its author had seen when making it, so a change made before someone
saw their new role is kept everywhere, and a line moved out of a person's categories
while they edited it keeps their edit; every computer keeps and leaves out the same
changes. Roles are checked by the app, not yet secured: changes aren't signed, so another
program that can write in the shared folder can still write a change in someone's name,
and the folder's own sharing settings are what keeps a reader from writing at all.
The command line's `set` and `apply` and the MCP server change a shared budget the same
way, as this computer's person and within their role (docs/CLI.md, docs/MCP.md). Once roles
are set, an edit to the `.mthd` made by another program can't be said to be anyone's, so
the computer that keeps the file doesn't take it in: it keeps the edited file beside it as
"Feature (outside edit 2026-10-18 140322).mthd", writes the file again from everyone's
changes, and the status panel says so. A file put back to an older version of the budget
(a sync service's history, a backup) is behind the log, not an outside edit: it is written
again and the panel says so. Changes made by a person's command line or agent show under
their name, as "Jordan Lee (command line or agent)", and aren't listed as another person.

**Not yet** (later phases of the design): handing ownership itself on (the owner is one
computer's id; a co-owner can do everything else), invite links, signed changes and removing a
person's later changes (a removed member's work is judged by what they had seen, as
above), picking single accounts (not only whole categories) in the dialog, muted cells and
locks on what a person can't change, access requests, live sessions with cursors,
proposals, checkpoints for very long logs, and the service-by-service checks (iCloud,
Google Drive, LucidLink) the design calls for.

| Command | Arguments | Returns |
|---|---|---|
| `share_status` | none | The status item's state (`mode`, `folder`, `file`, `me`, `keeper`, `people`, `synced_secs`, `unshared`, `waiting`, `review_count`, `error`, `file_written`, `changes`, `recent`, `conflicts`, `can_set_roles`, `roles_started`, `refused`; each person also has `role`, `role_text` and `scope`), or `null` when the budget isn't shared |
| `share_setup` | none | What the Share dialog starts from: `name`, `color`, `colors`, `file`, `folder`, `blocked` (why it can't be shared yet), `picked`, `categories` (`[{code, name}]`, when this person sets roles) |
| `share_pick_folder` | none; shows Rust's folder dialog | The folder's path (kept in Rust until `share_start`), or `null` when cancelled |
| `share_start` | `use_picked`, `move_file`, `me` (`{name, color}`) | View of the budget, now shared |
| `share_set_me` | `name`, `color` | The person as kept, for every shared budget |
| `share_place` | `category`, `account` (where this person is) | nothing |
| `share_review` | none | The review list: `index`, `key`, `sentence`, `place` (`{kind, key, parent, field, text}`), `values` |
| `share_activity` | `limit` | Recent changes, newest first: `actor`, `name`, `color`, `me`, `time`, `label`, `ops`, `place` |
| `share_blame` | `kind`, `key`, `field` | Every value the field has had, oldest first: `value`, `actor`, `name`, `color`, `me`, `time`, `label`, `kept`, `index` |
| `share_use_value` | `kind`, `key`, `field`, `index` | View, with the value put back as one change |
| `share_set_role` | `actor` (64 hex characters), `role` (`co_owner`, `admin`, `editor`, `department`, `actuals`, `comment`, `read_only`), `scope` (category numbers, for `department`) | The status, with the role set as one change; refused unless this person is the owner, a co-owner or an admin who may give it |

Rust sends `share-status` (`{doc, status}`) whenever the status changes and
`share-changed` (`{doc, rev, remote: [{actor, name, color, label, time, places}]}`)
when other people's changes (or an outside edit) were taken in; the page fetches the
view and flashes the places. Everything else goes through views: a view says `shared`,
and carries a `notice` once when a command has something to say (an undo that couldn't
put everything back).

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
presets, line blocks, report presets, overtime patterns, rate tables, each with its count), a folder picker (All folders,
Unfiled, then the kind's folders) and a filter on names and definitions. Each item shows
its definition in words (its tooltip adds what it brings along), a tick box, and its folder
when All folders is shown. Budget Setup shows the same library when a tab's rail is switched
to **Library**: the name, kind, sort and folders in the rail and the items as a table (see
"The Budget Setup window" above).

- **Add to This Budget…** copies the ticked items (or the item double-clicked, pressed Enter
  on, or right-clicked) into the budget in front, after Import Setup's review: what each
  item needs comes along (a fringe's unit and its rate's globals, a global's globals, a
  charge's groups...), each item says **New**, **Same here** or **Different here**, and an
  item that differs keeps this budget's version unless you pick **Use the library's
  version** or **Add the library's as** a new name. One undo step. The budget settings
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

### Setup packs

Sets of setup items to share, keep or start from (decisions.md entry 50; the file is
described in FORMAT.md, "Setup packs"). **A setup pack is a budget file** with setup and no
lines, stamped `setup_pack` with the pack's details: a title, a description, who made it
(free text), a version, the date its figures take effect, the region or jurisdiction they
apply to, and where its figures come from (notes and links, for the pack and for any item).
It opens in the app like any budget, so a pack is edited by opening it as a tab, and New
From Template… starts a budget from one.

- **Export Setup Pack…** (File menu, and **Export Pack…** or **Export Library…** in the
  Library tab's Setup packs section, so also from the Budget Setup window's library rail)
  lists the budget's (or the library's) setup items by kind and folder: a box per kind,
  per folder and per item, and what a ticked item needs (a fringe's globals and unit, a
  charge's groups, a tag preset's tags, however deep) comes along, shown as such. Below,
  the pack's details. **Export Pack…** builds the pack in Rust (the same dependency walk as
  Import Setup, the source's folders and item sources kept, the budget settings never
  included), checks the details (a title; the date `YYYY-MM-DD`; links `http://` or
  `https://`) and shows Rust's Save dialog, in your Packs folder at first; the file is
  written through a temporary file, and a budget open in a tab is never written over.
- **Import Setup Pack…** (File menu; **Import Pack…** and **Import into Library…** in the
  Setup packs section; **Add to This Budget…** and **Add to Library…** on a listed pack)
  shows Import Setup's review with the pack's details on top and each item's source under
  it: each item **New**, **Same here** or **Different here**, keep this budget's (the
  default), use the pack's, or add the pack's under a new name; what an item needs comes
  along. Into a budget it is one undo step; into the library it is the library's own
  checked write (refused while the library is open as a tab). **Every item that comes in is
  stamped** with the pack's title, version and effective date (`setup_sources`), and its
  own notes and links come with it, so you can always see where a rate came from. A budget
  that isn't a pack can be imported this way too (nothing is stamped).
- **The Setup packs section** at the foot of the Library tab lists the packs in your
  **Packs folder** (`Packs` in the app's local data folder, made the first time it is
  listed; **Show Packs Folder** opens it, so you can drop packs in) and the packs that come
  with the app (the repository's `packs/` folder, bundled as a resource: empty but for a
  README, since rate packs come from a separate data project after review, CLEAN-ROOM.md),
  each with its title, version, effective date, region and item count. **Refresh** looks
  again.
- **Where an item came from** travels with it: Import Setup From…, the library and packs all
  carry an item's source along, and replacing an item replaces its source. Renaming an item
  everywhere keeps it. Overtime patterns are setup items now too (Import Setup, the library,
  packs and spreadsheets all carry them).

| Command | Arguments | Returns |
|---|---|---|
| `packs_list` | none | `{folder, packs: [{token, file, place, title, version, effective, region, items, problem}]}` (`place`: `yours` or `bundled`) |
| `packs_reveal` | none | opens the Packs folder in the file manager |
| `pack_items` | `source` (`budget` or `library`) | `{source, title, kinds}` as the library's `kinds` |
| `pack_export` | `source`, `picks`, `details` (`{title, description, author, version, effective, region, notes, urls}`), `created` | `{name, message}`, or `null` when cancelled |
| `pack_import_pick` | `into` (`budget` or `library`); shows Rust's Open dialog | the review, or `null` when cancelled |
| `pack_review` | `token` (from `packs_list`), `into` | the review |
| `pack_import` | `token`, `picks`, `resolutions` (as `import_setup`) | `{view, message}`, one undo step |
| `pack_import_library` | `token`, `sourceToken`, `picks`, `resolutions` | `{library, message}` |

A pack review is Import Setup's (`token`, `source`, `items`, `notes`) plus `pack`
(`{is_pack, details, source_title, created, sources: [{kind, name, text}]}`), `into` and
`source_token` (the pack, for an import into the library, whose `token` names the library
as the review saw it). The pack commands are allowed in all three windows, since the
Library tab is in each; the dialogs are Rust's and the page never names a path.

### Setup lists in a spreadsheet

**File > Export > Setup Lists…** writes the budget's setup items as tables, one row per item
(per credit row for credits) with every part of its definition and where it came from
(Effective, Source Notes, Source Links): globals, fringes, groups, units, sets, locations,
currencies, charges, credits, tag presets, overtime patterns and rate tables (a row per
classification, period and unit), whichever are ticked, as a
spreadsheet (.xlsx), a sheet per kind, or CSV files (one per kind, named `<file> - Fringes.csv`
and so on). Lists (a charge's excluded groups, a tag preset's fringes) are names joined with
commas; an overtime pattern's bands read `2 @ 1.5; rest @ 2`; a flat fringe's Basis is the
editor's label (reading also takes `amount`, `amount_counts`, `quantity` or `Full quantity`)
and its Max Hours Per Day a number, and a cutoff a flat fringe keeps is written and read
back as it is. A charge's excluded amounts
belong to one budget's lines and stay out (the message says how many charges have them);
line blocks, report presets and the budget settings are not lists.

**File > Import Setup Lists…** reads a workbook, CSV or tab-delimited file (Import
Spreadsheet's readers and limits). Each sheet gets a kind (guessed from its name, or the
file's) and a column per field, guessed from the titles (our own export maps itself; a
department sheet's `Fringe Name`, `Percentage`, `Wage Cap` and `Notes` come out right) and
changeable. **Check and Review…** reads every chosen sheet and lists every problem with its
sheet and row number (names missing or listed twice, numbers that aren't numbers, formulas
that don't parse or name a global neither the sheet nor the budget has, a basis or max
hours per day on a percent fringe, a basis it doesn't know, dates that aren't `YYYY-MM-DD`,
links that aren't web links); nothing is imported
while any is left. A clean read goes to Import Setup's review (keep, take or add under a
new name; one undo step). To fill a pack from a spreadsheet, open the pack as a tab and
import there.

| Command | Arguments | Returns |
|---|---|---|
| `setup_sheet_export` | `kinds` (empty: all), `format` (`xlsx` or `csv`); shows Rust's Save dialog | `{done, files, message}` |
| `setup_sheet_pick` | none (shows Rust's Open dialog) | `{token, file, sheets: [{index, name, rows, kind, mapping, titles}], kinds: [{kind, label, columns}]}`, or `null` |
| `setup_sheet_guess` | `token`, `index`, `kind` | the sheet with that kind's guessed `mapping` and `titles` |
| `setup_sheet_review` | `token`, `choices` (`[{index, kind, mapping: {header_row, columns}}]`) | `{errors: [{sheet, row, message}], preview}`: the problems, or Import Setup's review (its token goes to `import_setup`) |

These are the main window's only (File menu).

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
  cutoff or unit with what a flat fringe counts beyond it, how many lines use it, what it
  adds up to), globals (formula, value, how
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

- **Spreadsheet (.xlsx)…** writes one `.xlsx` with a sheet per table: Topsheet, Accounts,
  Detail, Globals, Fringes, Groups, Units, Currencies, Sets and Locations.
- **CSV Files…** writes the same tables as one CSV file each, named from the name chosen:
  `Pilot.csv` gives `Pilot - Topsheet.csv`, `Pilot - Accounts.csv` and so on, beside it.
  They start with a UTF-8 byte order mark, so spreadsheet apps read accented letters.
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
  typed and its value), cutoff or unit, lines and total, then a flat fringe's basis and max
  hours per day (added last); groups with their switch and
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
`method-budget export <file.mthd> --format xlsx|csv|tsv --out <path> [--bom]`.

| Command | Arguments | Returns |
|---|---|---|
| `export_budget` | `format` (`"xlsx"`, `"csv"` or `"tsv"`; shows the Save dialog) | `{done, files}`: `done` false when cancelled; `files` the names written |

### Importing spreadsheets

**File > Import Spreadsheet…** adds detail lines from a spreadsheet (`.xlsx`), a CSV
file or tab-delimited text (`.tsv`, `.txt`): any department's sheet, an old budget kept in
a spreadsheet, or our own export. Rust shows the Open dialog and reads the file; then a dialog maps
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

How rows are read (`method_format::import::merge`): rows after the header row; rows empty in
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

Reading the files (`method_format::import`, plain Rust, no crates): CSV per RFC 4180 with
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
sheet back (.xlsx, CSV or tab-delimited) into the same chart without lines gives exactly
the same line totals and grand total (tested).

| Command | Arguments | Returns |
|---|---|---|
| `sheet_import_pick` | none (shows the Open dialog) | `{token, file, kind, sheets: [{name, hidden, rows, columns, truncated}], sheet, fields}`, or `null` when cancelled; `file` is the name only |
| `sheet_import_preview` | `token`, `request` (`{sheet, header_row, mapping: {field: column}, detect_header, guess, create_accounts, create_setup, default_account, target}`, `target` `"new"` for a new tab) | the preview: `{header_row, signature, mapping, columns, raw, lines, line_count, skipped, skipped_count, warnings, warning_count, headings, new_categories, new_accounts, matched_accounts, added, dropped, notes, reconciliation, grand_total_before, grand_total_after, calc_error, truncated}`; changes nothing |
| `sheet_import` | `token`, `request` | `{view, message}`: into the budget as one undo step, or a new tab in front |

### Importing a budget export

**File > Import Budget Export…** (and File > Open, for a file whose content is an export)
makes a whole budget from another budgeting program's export: budget exchange JSON or XML (basic or advanced), its .xlsx spreadsheet or its
tab-delimited topsheet (docs/IMPORT.md says what each carries). Rust shows the Open dialog
(a filter per layout), reads the file, builds the budget and checks it; the review shows,
before anything changes:

- **Read as**: the layout found from the file's content, changeable (the file is read
  again).
- **What comes over**: the budget's title, its categories, accounts and lines (adjustment
  rows among them), fringes, groups, sets, locations and currencies, charges and credits.
- **Check against the export's totals**: whether every figure comes out the same (grand
  total, and net total after credits), a table of what was compared (lines, account
  totals, account fringes, fringe accounts or Total Fringes rows, category totals and
  fringes, charges, credits, the grand and net totals) with how many differ, and the
  differing figures (the first 200) with the export's value, the imported one and the
  difference.
- **Notes**: what was worked out from the export's figures and should be checked (fringe
  posting, account numbering, cutoffs and ranges, exchange rates, unit hours, a charge that
  is a round percentage), what isn't in the export or wasn't brought over (formulas and
  globals, production totals, the rules of charges and credits, suppressed lines), and the
  rest.

**Import as New Budget** opens it in a new tab, unsaved (its first undo step), like Import
Chart of Accounts' new budget; the toast says what came in. It is disabled when the budget
doesn't recalculate or has no categories. The new tab's first Save (or Save As) starts in
the export's folder with the export's name and `.mthd` ("Pilot budget.json" gives "Pilot
budget.mthd"); the Save dialog asks before replacing a file, as always.

**Converting an export** (decisions.md, entry 67). An export that arrives through File >
Open, the start window's Open…, the Finder (double-click, Open With) or a drop gets the
same review, worded as converting it: the title is "Convert budget export: <file>", the
introduction says the original file is left as it is, and the button is **Convert to
Method Budget** (the review's `convert` is true). The new tab then shows a slim banner
at the top of its budget view (every view's `converted`, kept by the session): "Converted
from <file>. The original file is left as it was. Saving keeps this budget as a Method
Budget file (.mthd)." It belongs to that tab (switching tabs hides and shows it with the
tab; each pane of a split view shows its own), stays until its close button is clicked
(or Escape is pressed on it), which is remembered for that tab for the rest of the run,
and goes by itself after the first successful Save. It is never saved into the file
(js/convertbanner.js). The first Save starts beside the export as above. Nothing is ever written in the export's layout: the `.mthd`
is the working file.

| Command | Arguments | Returns |
|---|---|---|
| `export_import_pick` | none (shows the Open dialog) | the review, or `null` when cancelled: `{token, file, format, format_label, formats, title, categories, accounts, lines, adjustments, fringes, groups, sets, locations, currencies, charges, credits, notes: [{kind, text}], currency, tallies: [{kind, label, compared, different}], differences: [{kind, what, export, imported}], difference_count, export_grand, grand, export_net, net, calc_error, matches, convert}` (`convert`: false here, true from Open) |
| `export_import_preview` | `token`, `format` (`null` to detect) | the review, read as that layout; changes nothing |
| `export_import` | `token`, `format` | `{view, message}`: the budget in a new tab in front; `message` is `null` for a conversion, whose banner comes with every view (`converted`) until saved |

### Globals from a schedule's days

**File > Import Schedule Days…** sets the budget's globals from a schedule's day counts
(roadmap 3.8; decisions.md, entry 76). The file is the one Method Schedule writes for a
budget (File > Export for Budget there, or `method-schedule budget FILE`): one sub-board's
shoot days, in all and by unit, set, location and week, and each element's days
(`format: "method-schedule-days"`, versions 0 and 1; docs/research/schedule-exchange.md).
Rust shows the Open dialog (JSON files) and reads the file; then a review shows, before
anything changes:

- **The schedule**: its title, board and sub-board, how many shoot days and their dates,
  when the file was written, and notes on anything odd in the file (a count that doesn't
  add up, a `days_hash` that doesn't match its days).
- **Figures and globals**, grouped: shoot days in all, by unit, set, location and week,
  then each category's elements. Each row has a tick (use it), the figure, its days, the
  global it would set (a field to rename it, offering the budget's globals), that global's
  value now (its formula, and what it comes to when it isn't a plain number), the new value,
  and what happens: a new global, a new value, the same value, or changed in the schedule.
- **The rule**: shoot days in all go to `SHOOT_DAYS`, ticked to start. Any other figure is
  set only when ticked, to the global named in its row: by default the figure's name in
  capitals with `_DAYS` (`COLD_SPRING_FIELD_STATION_DAYS`), or any name typed there, an
  existing global to update it (`STATION_DAYS`). Typing a name ticks the row. A default is
  never a global the budget already has: it gets `_2`, `_3`... (in the Field Station budget
  `RIDGE_DAYS` is the fire road location's days, so the RIDGE set defaults to
  `RIDGE_DAYS_2`). An element's days
  are its day out of days total. Prep and wrap globals are never touched.
- **What Set Globals does**: how many globals are new, change or already match, and
  anything refused (a name that can't be a global, two figures in one global), which keeps
  Set Globals off.

**Set Globals** applies it as one undo step ("Set globals from the schedule Field
Station"); the toast says how many globals were set. A new global gets the figure as its
description ("Shoot days at location Cold Spring field station") and the Day unit when the
budget has one. Each global set records its figure in its source (the setup source packs
use, so the file format is unchanged), with the figure's key: "Shoot days at location Cold
Spring field station, from the schedule Field Station (Shooting Order, Main) [key:
location:Cold Spring field station]"; an element's key is its id, category, board ID and
name. The next import of the same schedule and sub-board finds each global's figure by its
key (a renamed or renumbered cast member keeps its link; one whose name and number both
changed is shown unticked with a note asking to confirm; one that now matches several
elements is shown on each of them, unticked, with a note asking which, and is never
offered at 0; when two globals were set from one figure, the row says so and only the
first is offered; an older note without a key is
matched by its words, and the row says so), ticks those globals again under their own
names, and says which ones the schedule has changed since ("The schedule changed since
these globals were set from it: STATION_DAYS"). A global whose figure is no longer in the
file (a location dropped or renamed, an element cut) is listed under "Set from this
schedule before, no longer in it" at 0 days, unticked: it is set to 0 only when ticked. A global already at its value can still be set,
which only records the link. The file's `days_hash` is not stored in the budget (that would
be a format change; the roadmap's 3.8 row has the proposal).

The command line does the same with `method-budget schedule-days <budget.mthd> <days.json>
[--pick KEY[=GLOBAL]]... [--skip KEY]... [--apply] [--json]` (docs/CLI.md), and agents
with the `schedule_days` MCP tool (docs/MCP.md). In the browser build the page's file
picker takes the file.

| Command | Arguments | Returns |
|---|---|---|
| `schedule_days_pick` | none (shows the Open dialog) | the review, or `null` when cancelled: `{token, file, schedule, board, sub_board, generated, version, source_hash, days_hash, shoot_days, start, end, notes, figures: [{key, kind, name, category, label, days, global, picked, linked, current, current_value, new, action, problem, note, changed}], adds, updates, same, changed, problems, can_apply}`; `kind` is `total`, `unit`, `set`, `location`, `week`, `element` or `missing`, `action` `add`, `update` or `same` |
| `schedule_days_review` | `token`, `picks` (`[{key, global}]`, `global` empty for the default; `null` for the review's own) | the review with those picks; changes nothing |
| `schedule_days_apply` | `token`, `picks` | `{view, message}`: the globals set as one undo step |

### Review files

A reviewer marks up a budget they can't change, and the author merges the comments back
(roadmap 3.3; decisions.md, entry 81). The comments live in a small file beside the
budget, `<budget>.review.json` (docs/FORMAT.md, "Review files"); the budget file itself
never changes during a review. It is the local stand-in for shared comments: the file
travels with the copy (a copy saved for sharing, an email, a shared folder) and comes back.

**File > Open for Review…** opens a budget (Rust's Open dialog; one already open just comes
to the front) for review. It is read-only while reviewed: every edit, undo and redo is
refused with one message. A banner under the breadcrumb says so, with **Show Comments** and
**Stop Reviewing**. A review file already beside the budget is read, so a review can go on
over several sittings; otherwise a new one is started, and it is written beside the budget
after every change (atomically). If the file there changed since it was read (someone else
wrote it), it is not written over: the app says so once, the Comments tab keeps the
comments and offers **Save Review File As…** (beside it) or **Replace the File**. Stop
Reviewing writes the file first and stays open, with the reason, when it can't. A budget
never saved has no place for one and is refused, and so is a budget shared through a
folder.

**The Comments tab** (bottom tools pane, also in the tools window) while reviewing: **Comment
on** offers what the grid's cursor is on (the line, its account and category; on the
account list the account and its category; on the topsheet the category) and **The whole
budget**; the box adds the comment (Add, or Cmd+Enter), signed with the name set in the
Notes tab and dated by this computer's clock. Below, **All comments** lists every comment
in the budget's order (the whole budget's, then categories, accounts and lines as the
topsheet runs, then setup items, then any whose place is gone), each with its place (a
click goes there: the line's account with the line selected, the account list, the
topsheet, or Budget Setup on a setup item), when and by whom it was written, its status,
and **Edit** (saving empty text deletes it), **Resolve** (or **Reopen**) and **Delete**
(asks first). **To settle only** hides the merged and dismissed ones. **Save Review File
As…** writes it somewhere else (when its folder can't be written, the next view says so).
Lines, accounts and categories with comments to settle get a bar inside their number cell,
whose tooltip points to the Comments tab. Comments on setup items can be written with
`method-budget review --add` for now (below); the tab shows them and goes to them.

**File > Open Review Comments…** opens a reviewer's file on the budget in front (Rust's Open
dialog, starting beside the budget). The budget stays editable. The banner and the
Comments tab say how many comments are left to settle; when the review was written on a
budget of another title (the reviewed copy's own title or the title of the budget it was
copied from), the tab warns before anything is merged. Each comment shows at its place as
above, with:

- **Merge as Note**: the comment becomes a dated note on its line, account or category
  (decisions.md, entry 35): its text, author and time, and ticked off as reviewed when the
  reviewer resolved it. **Merge All** merges every comment that can be, at once.
- **Dismiss** sets it aside: marked in the review file, the budget untouched; **Restore**
  takes that back.

Merging is one undo step ("Merge 3 review comments as notes"); the toast says what was
merged and dismissed and what stayed open. A comment on a setup item or on the whole budget
can't become a note (those have no notes) and a comment whose place is gone (a line deleted,
an account renumbered, a fringe renamed) can't either: each says why, stays listed with
where it was, and can be dismissed. A merged comment is known by its note, so merging twice
adds nothing and Undo shows it open again. What was done goes back into the review file
(each comment's outcome, with when and by whom), so the reviewer can see it; a comment the
file marks as merged stays merged even when its note was edited or deleted since, so Merge
All never adds it twice.

The command line does the same with `method-budget review` (docs/CLI.md), and agents read
a review file with the `review_comments` MCP tool (docs/MCP.md). In the browser build Open
for Review reviews the budget in front, the page's file picker takes the review file, and
**Download Review File** saves it. Comments live only in the page until downloaded, so
while some aren't, closing or reloading the page asks first, and Stop Reviewing or Close
Review File asks before dropping them.

| Command | Arguments | Returns |
|---|---|---|
| `review_open_budget` | none (shows the Open dialog) | View of the budget, now open for review, or `null` when cancelled (desktop only) |
| `review_start` | none | View, the budget open for review (the review file beside it read when there is one) |
| `review_open` | none (shows the Open dialog) | View with the review file open on the budget, or `null` when cancelled |
| `review_close` | none | View, the review file closed (the budget editable again); on the desktop written first, and refused (still open) when it can't be |
| `review_list` | none | `{mode, file, rev, unsaved, conflict, to_settle, comments, title, matches, rows, accounts, categories, budget, entries: [{id, on, text, author, created, edited, resolved, outcome, status, found, place, written_at, can_merge, why_not, ti, ai, row_id, line, category, account}]}`; `status` is `open`, `resolved`, `merged` or `dismissed` |
| `review_add` | `on` (`{row}`, `{account}`, `{category}`, `{setup: {kind, name}}` or `"budget"`), `text`, `created`, `author` | View |
| `review_update` | `id`, `text`, `edited` | View (empty text deletes the comment) |
| `review_resolve` | `id`, `resolved` | View |
| `review_delete` | `id` | View |
| `review_restore` | `id` | View (a dismissed comment open again) |
| `review_settle` | `picks` (`[{id, action}]`, `action` `merge` or `dismiss`), `at`, `by` | `{view, message}`: the merges as one undo step |
| `review_save` | `save_as` (show the Save dialog), `force` (write over a file that changed on disk: Replace the File) | View, or `null` when cancelled |

Every view carries `review` while a review file is open: `{mode, file, rev, unsaved,
conflict, to_settle, comments, title, matches, rows, accounts, categories}` (`mode` is `reviewing` or
`reading`; `rows`, `accounts` and `categories` are the places with comments to settle).

### Actuals and the cost report

**File > Import Actuals…** brings in actual costs from an accounting system's export (a
spreadsheet (.xlsx), CSV or tab-delimited file; decisions.md, entry 45). Rust shows the Open
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

How rows are read (`method_format::import::actuals`): amounts exactly (no floats; currency
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
(`method_core::actuals::cost_report`): an account's ETC is budget minus actual (never past
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
`.mthd`, and a name typed without an extension gets `.mthd`. Every dialog that opens a
budget accepts `.mthd` files.

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
macOS and Linux), one per open budget, each a budget file (`.mthd`) with a small
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
app's data folder: `Backups/<hash of the file's path>/<name> <date time> UTC.mthd`, with a
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
A new budget saved by New Budget is linked into place from its temporary file instead, so
it can never replace a file that is there (renamed after checking again where the folder
can't hold links).

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
moved or renamed in the Finder are followed to their new place (each is kept with a macOS
bookmark), and files that no longer exist drop off when the menu is refreshed (every open
and save). Clear Menu
empties it; the start window's Remove takes one file off. macOS's own system-wide recent
documents list is not updated.

`recent.json` has the suite's shape, which Method Launcher reads (SUITE-STANDARDS.md,
section 2; decisions.md, entry 85): a JSON array, newest first, one entry per file, each
with its absolute `path` and `opened`, the time it was last opened or saved (RFC 3339 in
UTC, to the second), plus a `bookmark` (hex) when the system made one:

```json
[
  {"path": "/Users/me/Films/Field.mthd", "opened": "2026-10-18T09:31:00Z", "bookmark": "626f6f6b..."},
  {"path": "/Users/me/Films/Short.mthd", "opened": "2026-10-17T16:02:45Z"}
]
```

Keys the app doesn't know are ignored, and an entry without `opened` gets the time it was
read. A list in the earlier shape (`{"files": [...], "bookmarks": {path: hex}}`) is still
read, bookmarks and all; its files get the time it was read as their `opened`, and the
next change to the list writes the new shape. A file that can't be read as either shape
is an empty list.

**Opening from the system.** Double-clicking a `.mthd` file, Open With, or dropping one on
the Dock icon or the window opens it in a tab (its own tab, when it is open already): on
macOS the app receives an open event, on Windows and Linux the path arrives on the command
line, and a drop on the window is Tauri's drop event, handled in Rust. A budget export or
spreadsheet handed over this way goes where Open sends it (above, entry 66). On macOS the
bundle also offers the app for `.json`, `.xml`, `.xlsx` and `.tsv` files as a second choice
under Open With (role Viewer, rank Alternate, on the system's own types: public.json,
public.xml, org.openxmlformats.spreadsheetml.sheet, public.tab-separated-values-text;
`crates/method-app/tauri.macos.conf.json`, entry 67), never as their default app; `.txt`
is left out (every text and source file is plain text). Windows and Linux declare `.mthd`
only, so there the app is offered for other files through the system's own "choose
another app". Nothing
is asked: no budget is replaced. When several files arrive at once, the first one opens. On macOS a
second double-click goes to the running app. On Windows and Linux there is no
single-instance handling: a second launch with a path starts a second copy of the app with
that file (each copy has its own recovery copies and doesn't offer the other's).

**Command-line options.** The window binary, `method-budget-app`, takes these besides a
file to open (SUITE-STANDARDS.md, section 2; Method Launcher starts the app with them;
src/launch.rs):

| Option | Does |
|---|---|
| `--check-for-updates` | checks for an update at once, as Help > Check for Updates… does, with its answer in the toolbar. The check starts when the window has finished starting (when the page first asks where an update is), so the answer is seen. |
| `--open-sample` | opens the Field Station sample, as Help > Open Sample does, in place of the start window. It opens once, whether or not the sample was shown before, and the first-start sample doesn't open a second copy. When the start shows a question first (budgets to recover, or the review of an export handed over with it), it doesn't open; Help > Open Sample is there. |

Options count anywhere on the line and can be given with a file, which opens first; any
other option (one this version doesn't know, or macOS's `-psn_...`) is skipped and never
taken for a file. They are read when the app starts: there is no single-instance
hand-off, so on macOS a second launch while the app runs brings the running app forward
without them, and on Windows and Linux it starts a second copy with its own options.

The app's data folder is the platform's local app data folder for
`com.methodbudget.desktop`: `~/Library/Application Support/...` on macOS,
`%LOCALAPPDATA%\...` on Windows, `~/.local/share/...` on Linux. Method Launcher reads
`recent.json` there.

| Command | Arguments | Returns |
|---|---|---|
| `autosave_setting`, `set_autosave` | `on` (set) | the setting |
| `flush_autosave` | `doc` | View (autosaved now, if anything was unsaved) |
| `open_pending` | none | what Open returns for a file the system asked to open or one dropped on the window (below), or `null` |
| `open_budget` | none (shows the Open dialog) | `{kind: "budget", view}` (in a tab), `{kind: "export", review}` (`export_import_pick`'s review, with `convert` true), `{kind: "sheet", picked}` (`sheet_import_pick`'s result), or `null` when cancelled; anything else is an error saying what Open reads |
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

The app keeps itself up to date (decisions.md, entries 48 and 83; how releases are made:
[RELEASING.md](RELEASING.md)). The work underneath is the suite's shared updater,
`method-update` from method-production-suite/method-libs, pinned to a full commit in
`crates/method-app/Cargo.toml`; src/updater.rs keeps Method Budget's menu items, commands
and page.

- **Checking.** About two minutes after the app starts, then every six hours, it asks the
  update endpoint (`plugins.updater.endpoints` in `tauri.conf.json`, the one place the
  address is written; build.rs bakes it into the app) whether a newer version exists. Help >
  Check for Updates Automatically (a check item, on unless turned off, kept in
  `updates.json` in the app's data folder, `{ "automatic": ..., "seen": ... }`) stops the
  timed checks; Help > Check for Updates… checks at once, and so does starting the app
  with `--check-for-updates` (Method Launcher's way), once the window has started. A check gives up after 30
  seconds, a download after 20 minutes.
- **Quietly.** A newer version downloads in the background. Its signature is checked
  against the public key the app was built with (`plugins.updater.pubkey`, baked in the
  same way) before it is kept, and the signature must name the version the manifest
  announces; anything else is thrown away. Then the toolbar shows **Update ready: Restart
  to update**. An automatic check that finds nothing or can't reach the endpoint says
  nothing at all; only Help > Check for Updates answers, in the same place for a few
  seconds ("Method Budget is up to date." or "Couldn't check for updates"). Releases are
  universal (Intel and Apple silicon) from 0.6, and the manifest lists the one archive for
  both Macs. Where the manifest has no build for the computer (method-update's
  `UpdateError::Platform`: not a Mac, or an Intel Mac reading a manifest from 0.3 to 0.5)
  the answer is "There's no update for this computer yet. This version stays as it is." and
  the installed version keeps working. Nothing appears as a dialog while you work.
- **Only the update's hosts.** Every request and every redirect goes over https to the
  endpoint's host, or to GitHub's two download hosts when the endpoint is on github.com
  (`transport::host_allowed`); a manifest that points the download anywhere else fails the
  check.
- **Restart to update** saves first: every open budget's unsaved changes go into its file
  by Autosave's own rules (so a file changed outside the app is never written over: that
  budget's question comes up instead), the open tabs and split view are remembered, and any
  budget that still can't be saved (untitled, Autosave off, a file changed outside the app)
  gets the same Save All, Discard All or Cancel question as quitting. Cancel leaves the
  update ready. Then `Method Budget.app` is swapped for the update in place (staged and
  backed up beside it, on the same volume; an /Applications the user can't write asks for
  an administrator through the system's own prompt), and the app starts again with its
  tabs, split view and tools panel as they were (the files reopen as at any start:
  src/tabs.rs). A failed install leaves the update ready and the app running. At the next
  start, a bundle an interrupted swap left hidden is put back.
- **What's New** for the new version opens once after an update, after any start-up
  question (recovered budgets, a file to open). Help > Release Notes shows this version's
  notes at any time. The notes are `docs/releases/<version>.md`, copied into the app when it
  is built (build.rs), read by `method_release::notes` into plain text pieces, and drawn with
  DOM calls and `textContent` (ui/js/updates.js), never as markup.
- **No key, no checks.** With the placeholder public key, a build never checks (no timer,
  no network), and Help > Check for Updates says "Couldn't check for updates".

The page's side (main window only): `update_status` (the indicator's state:
`{ phase, version, manual, said, automatic }`, `phase` being idle, checking, downloading,
ready or installing; `manual` when the user asked; `said` a one-time answer: `current`,
`failed`, `no-build` or `install-failed`), `update_restart`, `whats_new` (the notes once after an
update, else null) and `release_notes`. Rust sends `update-status` events with the same
shape. Check for Updates and the automatic switch are answered in Rust (src/updater.rs).
method-update's own `failed` and `read-only` phases never reach the page: a failure is said
once and the indicator goes back to idle, as it always has.

## Architecture

The budget logic that needs no window is its own crate, `method-session`, which also builds
for WebAssembly so a browser can run the same screens (decisions.md, entries 70 and 71;
docs/research/web-session.md). method-app re-exports what moved, under the same names: each
of its command modules is the Tauri commands (one call each into method-session), the
dialogs and the files.

```
crates/method-session/   no window, no file system, no threads, no clock of its own
  src/session.rs     one open budget: Budget, where it lives, dirty flag, undo/redo
                     patches (each step labeled and timed), the account on screen, the
                     recalc cache, autosave and recovery state (as data), read-only
  src/docs.rs        every open budget (one Session per tab) in tab order, the one in
                     front, split view's panes and target, and the lock the commands go
                     through
  src/edit.rs        every edit as a function on &mut Budget (validates, then applies)
  src/batch.rs       multi-cell edits (paste, fill down) applied as one undo step
  src/diagnose.rs    lenient recalc: flags every bad cell instead of stopping at the first
  src/report_dto.rs  serde shapes for method_core::Report (the engine stays serde-free)
  src/view.rs        what the UI draws after each command (lean), and the full view for printing
  src/history.rs     step labels for the History list; a step saved as a version
  src/display.rs     global display info, numbers-on-screen settings, formula cell values
  src/actuals.rs     the cost figures every view of a budget with actuals carries, the
                     rollup cache, Import Actuals (kept by token) and the Actuals tab
  src/safety.rs      the autosave rule, recovery key names, the file's stamp (as data)
  src/clock.rs       the clock the host hands in (the desktop: the `std-clock` feature)
  src/collab.rs      SharedLog: what a session asks of a shared budget's change log
  src/commands.rs    the typed function behind each phase 1 budget command, with its
                     undo label (the other modules have their own Commands sections)
  src/tabs.rs        new tab, switch, move, close (the closing rules), split view
  src/versions.rs    the Versions dialog: save, restore, rename, delete, each one undo
                     step, with names, notes and times checked
  src/notes.rs       dated notes on lines, accounts and categories, the Notes tab's list
  src/rowstyle.rs    row styles and group colors
  src/apply.rs       the apply panel: tag the selected rows, fringe ranges, selection summary
  src/presets.rs     tag presets and line blocks: saving, applying, inserting, arranging
  src/clipboard.rs   copy and paste whole rows, accounts and categories with the setup
                     they use: the app's own row clipboard, the paste review, the paste
  src/quickadd.rs    quick entry: add a unit, set, location, currency or global from a
                     cell with the edit (one undo step); the new-line defaults setting
  src/find.rs        find and replace: typed search, replacing in the checked results
  src/moverows.rs    moving lines, accounts and topsheet rows one place, keeping fringe
                     ranges whole; quick presses join one undo step
  src/rename.rs      renaming a setup item everywhere (method_core::rename does the work)
  src/renumber.rs    renumbering accounts to a new pattern: preview and the one step
  src/setup_lists.rs the setup lists' use counts and totals, their order, short codes
  src/explain.rs     Explain This Number
  src/overtime.rs    the Overtime tab: preview, writing into a line, saved patterns
  src/compare.rs     the compare screen: budgets from tabs, versions and picked files
                     (kept by token)
  src/diffmerge.rs   What Changed and Merge Changes From (method_core::diff and
                     method_core::merge do the work); the merge is one undo step
  src/export.rs      File > Export: the budget's tables as a spreadsheet (.xlsx), CSV files or
                     tab-delimited detail, as bytes named as the Save dialog suggests
  src/reports.rs     report presets saved in the budget (one undo step each)
  src/templates.rs   New From Template and Save as Template: a budget stripped to its
                     chart, setup and settings; safe file names
  src/import_setup.rs  Import Setup From: the review of another budget's setup items and
                     the import as one undo step (the source kept by token)
  src/coa_import.rs  Import Chart of Accounts (preview, new tab or apply as one undo
                     step) and the bundled chart templates
  src/sheet_import.rs  Import Spreadsheet: the picked file kept by token, the mapping
                     dialog's preview and the import (method_format::import does the work)
  src/export_import.rs  Import Budget Export: the review and the new tab
  src/schedule_days.rs  Import Schedule Days: a schedule's figures, the global each sets,
                     the review and applying it as one undo step (the file kept by token)
  src/review.rs      review files: reviewing (the budget read-only), the comments, where
                     each one is, merging them as notes in one undo step, dismissing
  src/open_any.rs    Open's routing by content: a budget, a budget export, a spreadsheet
  src/setup_sheets.rs  setup lists as spreadsheets: export by kind, import with a column
                     mapping per sheet, then Import Setup's review
  src/sharing.rs     copies for sharing (provenance, read-only, editable copies, returned
                     copies stored as versions) and sub-budgets
  src/packs.rs       setup packs: the review, importing into a budget, building one
  src/library.rs     the setup library's view, the review both ways, copying, folders
  src/host.rs        Host::dispatch(command, args): the commands by name, as JSON, with
                     COMMANDS and COMMANDS_VERSION, files in and out (base64), host_info,
                     the read-only switch
  src/host/*.rs      each moved module's commands as the host dispatches them

crates/method-web/       the page in a browser: Host as WebAssembly (wasm32 only)
  src/web.rs         the JS API (wasm-bindgen): new Host(), dispatch, openMthd, saveMthd,
                     setReadOnly, hostInfo, commands(); the browser's clock
  js/tauri-shim.js   window.__TAURI__ for the page: invoke over dispatch, the events, file
                     pickers for Open dialogs, downloads for Save dialogs, Budget Setup in
                     a frame. `cargo xtask web` builds target/web (docs/research/web-session.md)

crates/method-app/
  src/session.rs     the desktop's side of a session: SessionFiles (open, save, a file
                     changed outside, Load theirs, folder sharing) and DocsFiles
  src/tabs.rs        the tab commands, the Window menu's list of budgets, remembering the
                     open files (and panes) for the next start
  src/history.rs, src/display.rs, src/versions.rs, src/notes.rs, src/rowstyle.rs,
  src/apply.rs, src/presets.rs, src/clipboard.rs, src/quickadd.rs, src/find.rs,
  src/moverows.rs, src/rename.rs, src/renumber.rs, src/setup_lists.rs, src/explain.rs,
  src/overtime.rs, src/reports.rs
                     the Tauri commands, one call each into method-session (printing a
                     report to the print sheet or a PDF file stays in reports.rs)
  src/compare.rs, src/diffmerge.rs
                     the Open dialogs, reading picked files, a picked file open in a tab,
                     the saved file for What Changed
  src/export.rs, src/templates.rs, src/sharing.rs, src/setup_sheets.rs
                     the Save dialogs and atomic writes
  src/import_setup.rs, src/coa_import.rs, src/sheet_import.rs, src/export_import.rs,
  src/open_any.rs, src/actuals.rs, src/schedule_days.rs
                     the Open dialogs and reading the files (a file open in a tab is read
                     from the tab)
  src/review.rs      review files: Open for Review, Open Review Comments, the review file
                     read from and written beside the budget after every change
  src/library.rs     which file is the library (library.json), reading and writing it
                     atomically, refusing changes while it is open as a tab
  src/packs.rs       the Packs list (the user's Packs folder and the bundled packs), the
                     pack dialogs, importing into the library
  src/menu.rs        native menu bar, built from the same table as ui/js/shortcuts.js
  src/tools.rs       the tools panel's own window: opening, hiding and placing it, telling
                     the other window when the budget changed, the Notes tab's list
  src/setupwin.rs    the Budget Setup window: opening, placing and closing it
  src/start.rs       the start window: the folder listing, pins, New Budget into a folder
  src/shared.rs      a budget shared through a folder, inside its session: every edit
                     recorded in the change log, random row ids, the cached projection,
                     the timer pass (changes in and out, the keeper's file, heartbeats),
                     per-person undo, the status, review, blame and activity the page shows
  src/shared_app.rs  its Tauri side: File > Share, the Share dialog's commands, the
                     folder dialog, joining on open, the timer hook and the events
  src/pdf_mac.rs     macOS: AppKit's print operation on the web view, into a PDF file
                     with no panel or through the print sheet (Objective-C runtime calls)
  src/safety.rs      saving safety as plain files: atomic writes, recovery copies,
                     backups, settings, the recent list (tested with temporary folders)
  src/safety_app.rs  its Tauri side: autosave timer, recovery and backup commands, Open
                     Recent and Autosave menu items, files opened by the system, opening
                     budgets in tabs and closing them, reopening the tabs at the start
  src/updater.rs     in-app updates (the plugin driven from Rust: timed and asked-for
                     checks, background download, Restart to update after saving every
                     budget), What's New once after an update, Help > Release Notes
  src/help.rs        the Help menu's links (User Guide, Report a Bug, methodproduction.com:
                     a fixed allowlist, opened with the system's opener) and the texts
                     for Licenses and Notices
  src/app.rs         Tauri glue: the phase 1 commands (one call each into
                     method_session::commands), host_info, file dialogs, the close and
                     quit guard (one question about every unsaved tab), menu events
  examples/perf.rs   timings on a large sample budget (method_core::sample), and canned
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
- `budget`: the budget's outline as file-format JSON (via `method_format`, so the UI reads the
  documented format): setup tables, categories and accounts, charges, credits, info. Each
  account has `lines` (its row count) instead of `details`; the baseline has no `rows`;
  `versions` lists each version's `id`, `name`, `created` and `note`, not its content.
- `report`: every total down to the account from `method_core::recalc`, with money as integer
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
- `persons`, only when a line has a person and there are totals (decisions.md, entry 79):
  each person by the matching form of the name (trimmed, single spaces, lower case) with
  `name` (as shown), `lines`, `accounts`, `pay`, `fringe_total` and per percent fringe
  `fringe`, `cutoff`, `pay`, `amount` and `reached`, for the Person column; and
  `has_persons`, `true` whenever a line has a person (left out otherwise).
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
| `host_info` | none | `{kind, commands_version, read_only}`: `"desktop"`, the command contract's version (docs/research/web-session.md), `false`; a browser host says `"web"`. The main window and the Budget Setup window ask it at start (`host.js`) and, in a browser, hide what only the desktop does |
| `new_tab` | none | View of a starter budget in a new tab, in front |
| `switch_tab` | `doc` | View of that budget, now in front |
| `move_tab` | `doc`, `to` (its place afterwards) | the tabs |
| `close_tab` | `doc`, `discard`, `toStart` | `{closed, unsaved, last, view}`: closed (view: the budget now in front); or not, because of unsaved changes (ask, then again with `discard`, or save first) or because it is the last tab (close the window). With `toStart`, the last tab closes too and an untouched Untitled budget takes its place (the page then shows the start window) |
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

Every accepted edit names its undo step (`Session::edit(label, f)`; the labels are in
method-session, in `src/commands.rs` and each module's Commands section, so the desktop and
a browser host name steps alike): the label says what the command did, the
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
keeps a `method_core::calc::cache::RecalcCache`: an account's rows depend only on the account
and on the globals, fringes, currencies, units and groups, so after an edit only the
accounts whose rows changed are evaluated again (every account when one of those tables
changed); topsheet, charges, credits and breakdowns are always worked out in full. The
result is exactly `recalc`'s (tested over random edit sequences, and against the engine as
it was before these optimizations). On this Mac, a one-cell edit in a 5,000-row budget
costs about 5 ms in Rust from command to JSON.

**Calculation errors never lose an edit.** The engine stops at the first problem, which is
right for `method-budget check` but not for an editor. `diagnose::lenient_recalc` runs the engine;
if it fails, it checks every global, fringe rate and detail cell with the engine's own
rules, flags each bad one (`issues`, by row id and field), and recalculates a copy with
those cells blank. The UI shows the engine's message in a banner, outlines the bad cells,
and shows totals marked as partial. The budget keeps what the user typed.

**Selection totals** come from `method_core::selection_totals` through the
`selection_totals` command.

**The UI** is a handful of ES modules with no dependencies: `main.js` (state, navigation,
shortcuts, and where each tab was left), `tabs.js` (the tab strip: switching, closing,
dragging, scrolling), `grid.js` (a generic keyboard grid used at all three levels), `levels.js`
(turns a View into rows and columns), `footer.js`, `announce.js` (polite words for screen readers),
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
Budget Setup > Settings > Account numbers), `share.js` with `shared.css` (sharing a
budget through a folder: the Share dialog, the status item and its panel, presence,
the wash on cells other people changed, conflict badges and Who Changed This; `review.js`
and `activity.js` are the Review and Activity tabs),
`prefs.js` (per-user display choices in local storage: visible columns, the tools panel's
place, size and tabs), `display.js` with `display.css` (how numbers show: formula values,
rate and global decimals, totals with or without cents, hidden-factor marks, zero rows,
zoom and the formula view, and those View menu commands), `display-settings.js` (Budget
Setup > Settings > Numbers on screen and each global's details), `sheet.js` (styles worked out at run time, below) and `host.js` (which host the page runs in, from `host_info`: in a browser, method-web, the page has no menu bar and answers the app's keys itself, a File button stands in for the File menu, New makes a new tab, Save downloads the budget, and autosave, Share, the updater, the start window, the tools window and the setup library are hidden; docs/research/web-session.md). `boot.js`
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
  charges, credits, info, settings with renumbering, the baseline, the rate tables
  (`rate_table_save`, `rate_table_delete`, `rate_update`, and `rate_import_pick` and
  `rate_import_read`, whose Open dialog is Rust's), and the setup library's commands, and
  listening for and sending events. Nothing on the selected lines (its
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
  "keep the lines" from the page. The start window gets each pinned and recent budget's
  file name and folder to show, and names a row only by a token from the last listing
  (`start.json` and `recent.json` hold the paths); New Budget's budget file is picked in
  Rust's Open dialog and named by its token, and Save In's folder is picked in Rust's
  folder dialog (or offered from `start.json` or the system's Documents folder) and named
  by its token; the page sends only a file name, checked in Rust, which never writes over
  a file. Show in Folder runs the system's `open -R`,
  `explorer /select,` or `xdg-open` on a listed file. Import Spreadsheet is the same: Rust shows the Open
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

Tauri crates are pinned to exact versions and move as one set: Dependabot proposes them in a
`tauri` group of their own, and that pull request also moves the Tauri CLI pin in
`scripts/install-tauri-cli.sh`, by hand. Like every crate in
`Cargo.lock`, they must be at least 14 days old when added (`python3 scripts/check-crate-ages.py --only-changed HEAD`).
CI checks what a pull request adds, and the whole lock on every push to main and weekly
(crate-ages.yml), so a young crate that slipped in is caught too.
`tauri` uses `default-features = false` with only `wry`, `common-controls-v6`, `x11` and
`dbus` (no asset compression, no runtime ACL changes). The updater is `method-update`, from
the suite's method-libs repository, pinned to a full commit (`rev = "<40 hex>"`): the
crate-age check passes it only because that repository is on
`scripts/crate-age-git-allow.txt` and the pin is a full commit, and `cargo deny` allows it as
a git source. It brings HTTPS through ureq and rustls with the webpki root list (no native
TLS, no system proxy lookup, no async runtime), minisign-verify, and the gzip and tar
readers for the `.app.tar.gz`. The release tooling (crates/xtask) adds `minisign`, the crate
the Tauri CLI signs with, for `sign-update`; it never ships in the app. `method-release` (the workspace's own release plumbing:
versions, release notes, the updater key's shape) has no dependencies.
`method-session` depends only on the engine, the format, `serde` and `serde_json`, and must
keep building for `wasm32-unknown-unknown` (`cargo xtask wasm-check`, in CI); its
`std-clock` feature, which the app turns on, is the only thing in it that reads the system
clock.

## Not yet

Split view is two panes side by side only (no vertical split, no third pane), with one
tools panel for the target (the reference docks tools under each pane). It needs a live
check: Shift+Cmd+\\ and the View and Window menu items reaching the page in the system web
views, dragging a tab between the strips with a real pointer, the tools window following
the target, and the panes coming back at the next start. The History tab needs a live
check in the tools window and with a long history.

The start window's projects, thumbnails and Duplicate, and New Sub-Budget from This
Category or Account (decisions.md, entry 91), need a live check in the app: Add Folder…'s
dialog, a project made by New Budget appearing down the left, the bars' look in light and
dark, D and the arrows between the places and the list, Link a Schedule…'s dialog, a shared
budget's people, and the sub-budget's file appearing beside its parent. Their logic and file
work are tested in Rust, the page in a headless browser harness, and the row sub-budget's
menu item in the browser smoke. Rates at creation wait on 3.9; created dates and hiding
archived projects are later.

The sample budget (decisions.md, entry 84) needs a live check in the app: the first start
on a fresh data folder opening it with its Sample tab and line, a second start not opening
it, and Help > Open Sample.

Screen readers (decisions.md, entry 82) need a live check with VoiceOver, and with
Narrator or NVDA on Windows: the grid's cells, the announcement after an edit and the
dialogs. The footer's other totals aren't announced, and an empty grid shows focus only
by its border.

Rate tables (decisions.md, entry 90) need a live check in the app: the Rate Tables tab
(typing a table, Save Table and Revert, Add unit…, the rail and its Library switch), the
import dialog with a real workbook and Rust's Open dialog, the Fill Rate from Table picker,
the Rate cell's mark and tooltip, and Update rate from the right-click menu. Bundled public
rate data waits on the owner's answer about which public sources may ship.

The Person column (decisions.md, entry 79) needs a live check: typing a name, the
type-ahead of people, the running-total tooltip and the cutoff mark. The spreadsheet
export and import have no Person column yet.

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
Setup packs and setup lists need a live check of their Save and Open dialogs, Show Packs
Folder, the bundled packs folder in a built app, and the Setup packs section in the tools
and Budget Setup windows (their logic is tested in Rust, the dialogs in a browser
harness). Not yet: editing an item's source notes in the app (a pack's maker sets them
through a spreadsheet's Source columns or by editing the pack), clearing an item's stamp
when its figure is edited by hand, and showing an item's source in the setup tables (the
review, the spreadsheet export and `method-budget pack show` show it).
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
Incentive programs need a live check in the app: the Spend at these locations chips, the
program's terms band, the estimate in the rail and footer, and the Programs side by side
and Spend by location tables (checked here offscreen in the browser build in headless
Chrome, against the real engine). Not yet: an unapplied credit's rows grid shows its row
figures (the footer and the tables have them), a what-if that moves lines between
locations, a printed per-location report, and a preset of program fields to fill in.
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
field, and keeps a saved mapping only in the web view's storage.
Import Budget Export needs a live check: the Open dialog's filters and the review in the
app (checked here with the parity runs' exports in every layout through `method-budget
import-export`, and the review in a browser harness with a canned reply, Import calling
`export_import` with the token and layout).
Import Schedule Days needs a live check in the app: the Open dialog and the menu item
(checked here with the Field Station files through `method-budget schedule-days`, and the
review, renaming, ticking, a refused name and Set Globals in the browser build against the
real Rust code).
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
Roles in a shared budget need a live check in the app: the Share dialog's role menus and
category checklist, the first-role question, the toolbar chip and the refusal toasts in a
second person's window (logic tested in Rust through two sessions on one folder; the
script passes `node --check`).
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
Review files need a live check in the app: File > Open for Review… and Open Review
Comments… with their Open dialogs, the review banner, the Comments tab docked and in the
tools window (adding on the cursor's line, account and category, editing, resolving,
Merge All and its undo step, Dismiss and Restore), the number-cell bars, Save Review File
As…, and a review file written beside a budget in a folder that can't be written (logic
tested in Rust, the file round trip through `method-budget review`).
