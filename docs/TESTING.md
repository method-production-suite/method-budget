# Testing Method Budget

Method Budget is in alpha. It's tested against thousands of figures, but you will find rough
edges, and finding them is the point. Thank you for looking.

An hour is plenty. Pick what's closest to your own work from the list below; you don't need
to do all of it.

## Before you start

- **Install** the latest release from the
  [releases page](https://github.com/method-production-suite/method-budget/releases/latest).
  On a Mac, open `Method-Budget-macOS.dmg` and drag the app to Applications. It's signed and
  notarized, so it opens like any other app.
- **Use invented budgets only.** The app opens a sample budget, **Field Station**, the first
  time (Help > Open Sample opens it again). Make your own from New Budget or a template, or
  run `method-budget sample` for a large one. Please don't load a real show's budget, and
  never send one with a report.
- **Keep copies.** Anything you make that you care about, keep a copy of it outside the app.

## What to try

Each of these takes five to fifteen minutes.

1. **Find your way around Field Station.** Open a category, then an account, then its lines.
   Change a rate or a number of days and watch the totals move. Pick any total and choose
   Edit > Explain This Number… to see how it was worked out.
2. **Change the setup.** In Budget Setup, change a fringe rate or a global (shoot days, say)
   and check that every line that uses it follows.
3. **Start a budget of your own.** File > New… or New From Template…, with a chart of
   accounts you know. Add a few lines the way you would on a real show.
4. **Bring something in.** Try one of:
   - File > Import Spreadsheet… with a rental list or crew list you make up;
   - File > Import Budget Export… with an export from another budgeting program, if you
     have a test budget in one. Then compare the totals line by line and tell us any that
     differ.
5. **Keep versions.** File > Versions… to save one, change some figures, then File > Compare
   Budgets… to see the difference.
6. **Print and export.** File > Print… or Print Topsheet…, and File > Export as a spreadsheet
   or CSV. Check the printed figures against the screen.
7. **Use only the keyboard,** or a screen reader if you use one. Move around the grid, edit
   cells, open the menus and dialogs. Help > Keyboard Shortcuts lists the keys.
8. **Share through a folder,** if you have a second computer and a folder you both sync
   (any sync service you already use). File > Share… on one, then File > Join from Link… on
   the other, set a role, and edit on both.
9. **The command line,** if you're comfortable in a terminal: `method-budget totals` on a
   budget file, as in the [README](../README.md#command-line-quickstart).

## Telling us what you found

- In the app, **Help > Report a Bug…** opens a new issue with the app's version and your
  system already filled in. You can also open an
  [issue](https://github.com/method-production-suite/method-budget/issues) directly.
- Say what you did, what you expected and what happened instead. For a figure, give the
  number you expected and the number you got.
- A screenshot helps, of invented data only. If a budget shows the problem, make an
  invented one that does.
- A question about how something should work is a good report too. So is "I couldn't find
  how to…".

## What alpha means here

- **Mac first.** The Mac app is signed, notarized and tested by hand. The Windows and Linux
  builds are made on every release but haven't been tried by hand yet, and they don't
  update themselves yet.
- **Live changes aren't in this release.** Sharing works through a shared folder; changes
  appearing within seconds on the other computer comes later.
- **English only** for now.
- **Starter templates are still being written,** so the choice of charts of accounts is short.
- **Check numbers before they leave the room.** That's true of any new tool, and of this one
  especially while it's in alpha.

The [roadmap](ROADMAP.md) has what comes next.
