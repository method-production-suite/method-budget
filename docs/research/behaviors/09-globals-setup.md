# Behavior note 09: globals setup screen

Source: "Globals Tutorial", vendor tutorial video (published 2020; vendor tutorial, 2020/2021
era), about 2 minutes, recorded on macOS. Clean-room: behavior in our own words, read
headless from captions and extracted frames. No budget data, copied help text or images.
The video stays outside the repo.

## Behaviors observed

- **Three ways in**: a toolbar button, the Setup menu, and a keyboard shortcut. All open
  one "budget setup" window with a tab per table (locations, fringes, globals, groups,
  currency, units, sets, credits, charges, and in this macOS build a shortcuts tab).
- **Two sources side by side**: the window has a "cloud library" tab and a "budget" tab.
  The budget tab is this budget's own globals; the library holds globals the user has
  saved to their account for reuse in any later budget, so setup tables don't have to be
  copied between budgets by hand.
- **Folders**: a left rail lists categories (folders) of globals: an uncategorized folder
  plus user-made ones, and a separate vendor-supplied folder. Adding a category is an
  inline name entry in the rail. One or more selected globals can be moved into a folder
  through a "move to category" menu. In the 2021 Windows build the rail showed many
  user folders organized by topic (agreements, schedules, travel and so on).
- **Columns of a global row**: drag handle (manual order, with a "Sort: Custom" control
  above the list), a selection checkbox, a visibility (eye) toggle, an in-use count, an
  unlabeled checkbox column headed "D" (purpose not shown), name, description,
  calculation, units, a "Dec" number, and the evaluated value.
- **Adding**: an add button inserts a blank row with the name cell in edit mode.
- **Formulas reference other globals**: a global's calculation can be another global's
  name or an expression using it (for example a multiple of another global). The value
  column shows the evaluated result, two decimals.
- **A global carries a unit**: the units cell offers type-ahead over the budget's existing
  units, so a global can say "this number is in days".
- **In-use counts references from other globals**: a global referenced by two other
  globals (and by no lines) showed an in-use count of 2.
- **Uploading to the library pulls dependencies along**: uploading a global that
  references a unit (or presumably another global) warns that referenced setup items will
  be uploaded too. The library copy of the dependency is overwritten so the global keeps
  the same value when reused. The library mirrors the budget's folder structure.
- In the library tab, a formula that referenced another global was displayed with an
  internal identifier in place of the name: the library stores references by a stable id,
  not by name.
- The library tab offers the reverse action: download selected globals to the budget.

## Calculation findings

- Globals evaluate in dependency order and show their value next to the formula. This
  matches our engine (decision 5).
- A global's value displayed with two decimals regardless of the "Dec" field; what "Dec"
  controls was not demonstrated (possibly display or rounding precision). Unverified.
- No arithmetic was shown that could disagree with ours. No mismatch found.

## Shortcuts seen (macOS, 2020 build)

- Globals: Cmd+G (Ctrl+G on Windows, per the narration).
- Setup menu, in the same build: budget preferences Cmd+9, locations Cmd+5, fringes Cmd+3,
  groups Cmd+4, currency Cmd+6, units Cmd+7, sets Cmd+8, credits Cmd+backslash, charges
  Cmd+slash. Note 14's Windows build shows the same layout with Ctrl.
- Clash to decide: our app uses Cmd+/ for the shortcut sheet and Cmd+1 to Cmd+9 for setup
  tabs. Someone switching over will expect Cmd+/ to open charges and Cmd+G globals.

## Era caution

The cloud library and the "new app" framing are from 2020. Sam's 2026 notes (01, 02) show
a tools pane with the same tables, but the setup window's current layout, the "D" and
"Dec" columns, and the library workflow were not checked in 2026. Treat column details as
indicative.

## What this means for Method Budget

1. [partial] P2 **Global description and in-use count**: show where a global is used
   (lines and other globals) and allow a description. We show name, formula, value only.
2. [missing] P2 **Global unit**: an optional unit on a global (days, weeks), offered from
   the units table with type-ahead. Decide whether it pre-fills a line's unit when the
   global is typed into Amount (not shown in the video; needs Sam's 2026 check).
3. [missing] P2 **Rename a global with formula rewrite**: the reference stores references
   by id, so renames are safe there. We reference by name; a rename should rewrite every
   formula that uses it (APP.md lists renaming under "Not yet").
4. [missing] P3 **Folders and manual order for setup tables**: categories for globals
   (and fringes, groups) plus drag ordering. Our tables are alphabetical maps.
5. [missing] P3 **Personal library of setup items**: save globals, fringes, groups to a
   local library file and import into another budget, carrying dependencies (units,
   referenced globals) along. Local and file-based, not a cloud account.
6. [missing] P2 **Shortcut decision**: Cmd+G for globals and the Cmd+3/4/5... setup keys
   people already know, versus our Cmd+1 to Cmd+9 tabs and Cmd+/ help. Needs a call.
