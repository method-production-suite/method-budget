# Roadmap

What to build next so a production can move its real budgets off the reference product, then reach
parity, then go past it. Local files only: no cloud, no accounts, no network beyond the app's own update checks (decisions 14 and 48).
Every cloud or sharing feature below has a local-first stand-in.

Sources are cited by behavior note number (`01` to `24` in `docs/research/behaviors/`),
`inv` (sections of the feature inventory in `docs/research/`), `ETP` (export test plan),
`APP` (the "Not yet" list in `docs/APP.md`) and `D` (entries in `docs/decisions.md`).
Notes 01 and 02 are 2026 recordings and win over the 2020/2021 tutorials (03 to 24).

Status: **[have]** built, **[partial]** some of it built (engine only, display only, or
missing pieces), **[missing]** not started. Size: **S** a day or two, **M** about a week,
**L** more than a week.

---

## Waiting on the owner

Updated 2026-10-21 (evening). Only what needs the owner's answer, hands or yes; everything else on
this page goes ahead without waiting.

### Release go-aheads

- **0.7.0 is out** (published 2026-10-21, from main without 3.18; methodbudget.com follows
  it). The next release needs the owner's yes as usual. It carries signed changes and the
  owner's key (3.18, decisions 89 and 95, merged after the tag), so its notes must say
  plainly that everyone sharing a budget with roles needs it: Method Budget 0.4.0 to 0.7.0
  refuse a shared folder at log format 3 once its owner's key is in. It also carries invite
  links (decision 97), so the notes must also say that 0.7.0 and older refuse a shared
  folder at log format 4, which it reaches with its first invite.
- **rustls update, on or after 2026-10-28**: the fix (0.23.46) leaves the 14-day crate
  cooldown then. `cargo update -p rustls` and the four RUSTSEC-2026-0346 to 0349 ignores
  taken out of `deny.toml` go in as one pull request. None of the four applies to the
  updater (deny.toml gives each reason), so nothing waits on it.
- **Crate-age exceptions end**: webpki-roots on 2026-11-02 and rustls-webpki on
  2026-11-03 reach 14 days; their lines in the age-check exceptions come out then.

### Open decisions

- **A key put in the owner's name before the owner's app does: decided by the owner
  (2026-10-20)** (3.18; D89, D95): "build 3.18 options 1 and 3". Built (decision 95; design
  docs/research/owner-key.md): (1) **Start Again**, offered only on the owner's computer
  when a key in the owner's name isn't its own, makes a new shared budget from the budget as
  it stands, signed with the owner's real key from its second change, with everyone's roles,
  and keeps the old log folder aside; members' apps say the owner started again and refuse
  edits until they reopen; (3) each computer **remembers the first owner key it sees**,
  outside the shared folder, and keeps out changes signed with any other, with **Trust the
  New Key…** for an owner who moved to a new computer. Needs a live check in the app.

- **Signed changes in shared budgets: decided by the owner (2026-10-20)** (3.18; D89):
  "Method Budget may merge 3.18 signed changes with the shared log format 3", then "prepare
  Budget 0.7.0 from main without 3.18". Built: signed changes (each person's key pair,
  every change signed, removal by cutoff) use the `ed25519-dalek` crate (3.0.0; pure Rust,
  BSD-3-Clause, already on `deny.toml`'s licence list), with `sha2` held at 0.11.0 for the
  14-day check; both the crate-age check and `cargo deny` pass. Signing raises the change
  log's format to 3 once the owner's key is in, so Method Budget 0.4.0 to 0.7.0 refuse a
  signed budget rather than take in unsigned changes. On main since 2026-10-21; they ship in
  the release after 0.7.0 (above). Later (the owner): key rotation, and publishing a key
  from the command line.

- **Invite links for shared budgets: decided by the owner (2026-10-21)** (3.18; D97;
  design docs/research/collaboration.md 9.1): "Method Budget invite links use option (a),
  File > Join from Link, now." Built (decision 97): whoever may give a role makes an invite
  in the Share dialog (role, categories for Edits some categories, 1, 7 or 30 days), a
  signed change in the log holding only the public key of the secret in the link; the link
  (`methodbudget:join?v=1&...`) is shown once to copy and send any way; File > Join from
  Link… checks it, picks the budget's `.mthd` in the newcomer's synced folder in Rust's own
  dialog, joins as this computer and records a signed claim, checked by every computer:
  single use (settled the same everywhere), expiry by the claim's own clock, Cancel as a
  cutoff, and each invite sealed by a granter's app as soon as it sees it used or past its
  date (after MReview on #92), with a claim's clock checked against what it saw. No new dependency. Raises the shared log to format 4. Needs a live check in the
  app. Later: clickable `methodbudget://` links (Tauri's deep-link plugin, single-instance
  handling, a static join page), if people ask; `method-budget invite` and `join`.

- **Which languages first** (3.14; D82, D94): the framework is built and English is
  the only language (docs/APP.md, "Languages"): one string table read by the page and
  the menu bar, with plural forms and an English fallback, and most of the main areas
  moved into it. Adding a language is a table: `ui/i18n/<code>.js` with the same keys,
  imported in `ui/js/i18n.js`. **Recommend: pick one or two languages where the users
  are**, so the remaining passes (docs/research/accessibility-audit.md, "Localization")
  are checked against real translations, with room for longer words and, if one is
  picked, right to left. Not built: no language but English, waiting on the choice.

- **Charge rows on the topsheet** (decision 13 below; D60): the reference may put charges
  after the last production total, before the summary rows; ours puts them after. No
  parity run has confirmed the reference's place. **Recommend: match it once a run
  confirms it.** Not built: ours is unchanged.
- **Fringe % on a line that earns none** (D61): ours leaves the cell blank on a suppressed
  line, a line without fringes and an adjustment row without fringes; the reference writes
  0.00%. Display only. **Recommend: match it (0.00%)**, by the match-by-default rule. Not
  built.
- **Changing the base currency** (D61, parity run R2-03): the reference keeps every rate
  as typed and reads blank-currency lines as the new base, so amounts move; ours renames
  the base, so nothing moves. **Recommend: keep ours** (matching it changes figures
  without an edit); the reference's way could be an option later. Built on the
  recommendation.
- **Separator with a label** (decision 6 below): **Recommend: a bare rule**; bold title
  lines do the labeling. Built on the recommendation (today's separator).
- **How a person's cutoff is shared** (3.11; D79): past the cutoff, a person paid in
  several accounts earns the percent of the cutoff once, split over their lines by pay.
  The other way would use the cutoff up in line order across accounts, as a fringe range
  does within one account. The reference product has no person, so no parity run can
  decide it. **Recommend: by pay**, so moving lines or accounts never moves a cent. Built
  on the recommendation.
- **Which person names are one person** (3.11; D79): names match with the ends trimmed,
  inner runs of spaces as one and case ignored ("Mara Lee", "mara  lee " and "MARA LEE"
  are one person), shown as the budget first spells them. The other way is exact
  matching, where a stray space or capital makes a second person. **Recommend: loose
  matching**, so a typo can't charge a wage base twice. Built on the recommendation.
- **Where a project folder comes from** (3.19; D91): the design said a project is also
  the folder a new budget is saved in, which would make Documents a project. Built
  instead: New Budget's Save In has "Make a project folder for it" (on by default) that
  makes a folder named like the budget and saves it inside; only that folder, and folders
  added with Add Folder…, are projects. **Recommend: keep it**, so a loose save never adds
  a folder nobody chose. Built on the recommendation; reversible (the box's default, or
  listing every Save In folder).
- **Built on the recommendation, never confirmed by the owner** (any can be reversed from
  the digest): renames rewrite every reference (decision 7; D32), new-line defaults match
  the reference as per-budget switches (decision 10; 1.4, D60), dated notes on lines,
  accounts and categories (decision 11; D35), the paste review with the target winning
  (decision 12; D23; the review doesn't show the total change yet), tag presets in the
  budget, Import Setup and a library file (decision 15; D29, D31, D37), and in New Budget a
  ticked library or pack item replaces the starting point's item of the same name (D77;
  Import Setup elsewhere keeps the budget's own).
- **Review files, built on the recommendation** (D81; roadmap 3.3): a reviewer's comments
  live in their own file beside the budget, never in the `.mthd`; dismissing a comment
  changes only that file, so Restore takes it back, not Undo (merging is one undo step);
  comments on setup items and the whole budget stay in the file, since those have no notes
  (giving them notes is a format change). **Recommend: keep these** until reviewers ask for
  more; any can be reversed.
- **Roles in a shared budget, built on the recommendation** (3.18; D78, D89): roles shipped
  before signatures and are now secured by them (D89); until the owner sets a first role everyone edits everything, and after it
  someone without a role reads only (the design's 9.1); an editor changes everything but
  roles and locking the originals; a department editor changes only lines and notes in
  its categories. **Recommend: keep all four**; any can be reversed from the digest.
- **What an older app does with a budget that has roles** (3.18; D78): **decided by the
  owner (2026-10-18)**, as recommended, and built: the first role raises the change log's
  format to 2, so Method Budget 0.3.0 refuses the log as newer than it reads and opens the
  file on its own with that notice; anything saved there is kept aside by the computer that
  keeps the file, not taken in. The first-role question warns of it.
- **The owner is one computer** (3.18; D78): **decided by the owner (2026-10-18)**, as
  recommended, and built: the first-role question asks the owner to name an admin (it can
  be skipped), and the owner can make anyone a co-owner, who sets every role but the
  owner's and other co-owners', admins included.
- **Command line and MCP writes to a shared budget** (3.18; D78): **decided by the owner
  (2026-10-18)**, as recommended, and built: they act as this computer's person and follow
  roles, recording through the change log; with no person on the computer they are refused
  once roles are set; other commands that rewrite a shared file whole are refused; an
  outside edit after roles is kept aside.
- **Incentive programs** (3.6; D80): a program is a credit with an optional list of
  locations, and a credit that isn't applied is shown as an estimate the topsheet leaves
  out; deleting a location leaves its name in the credits limited to it, so they count
  nothing there instead of widening to every location. **Recommend: keep both** (no second
  item type to keep in step; no silent jump in an estimate). Built on the recommendation.
- **Incentive terms** (3.6; D88): a minimum spend is compared with the qualified spend
  after the locations and before any cap; a cap shared by programs comes after each
  program's own cap and is shared in proportion, to the cent (a leftover cent to the
  largest remainder, then by name); an unapplied program is estimated as if applied
  alongside the applied ones, so applying it takes off its estimate. **Recommend: keep
  all three.** Built on the recommendation.
- **Incentive what-ifs and the per-location report** (3.6; D93): the printed report lists
  what each program counts at each location but prints a program's credit once, not split
  by location (a share per location would be a new figure no program rule gives); the
  spreadsheet (.xlsx) and CSV exports gain an Incentives table only for a budget with
  credits, so other exports are unchanged; the What if… panel picks a category or an
  account (less the lines unticked), and applying a what-if that moves nothing is refused
  rather than adding an empty undo step. **Recommend: keep all three.** Built on the
  recommendation.

### Tests that need the owner's hands or hardware

**Deferred (the owner, 2026-10-20):** "defer windows tests indefinitely. We can do linux ones
eventually." Windows hands-on tests are off this list: the Windows build is still built and
tested automatically on every release and in the weekly run, and its release notes keep
saying it isn't yet tested by a person. Deferred from here: the Windows half of the live
check (2.25: printing, menu keys, file association, and installing over 0.2.x without an
old `method-budget.exe` left beside `method-budget-app.exe`, D74) and Narrator or NVDA with
the screen-reader check.

- [ ] **The sample budget at the first start** (3.20; D84, D86; on 0.6.0, **ready now**): quit the app, move its data
      folder aside (on a Mac `~/Library/Application Support/com.methodbudget.desktop`),
      and start it: Field Station should open in the tab
      "Sample: Field Station" with the line above the grid, instead of the start window.
      Close it (nothing asked), quit and start again: the start window, not the sample.
      Help > Open Sample opens a fresh copy, and the Help menu has no switch for the
      sample (D86). Save the sample under a name: the tab loses "Sample" and the
      line goes. Put the data folder back afterwards.
- [ ] **The launcher's flags** (3.21; D85; on 0.6.0, **ready now**): quit the app, then from Terminal run
      `open -a "Method Budget" --args --open-sample`: Field Station opens, not the start
      window. Quit, then `open -a "Method Budget" --args --check-for-updates`: the toolbar
      says "Checking for updates…", then the answer. Open a budget and quit: the data
      folder's `recent.json` is a list with `path` and `opened` for it.

- [ ] **Signed changes and Remove person** (3.18; D89; on the next build after this
      branch merges): on two computers sharing a test copy through a synced folder, set a
      first role for the second person in File > Share: the dialog says changes are signed,
      and within a minute "Signs changes" shows under both people. Edit on each and check the
      Activity tab marks the changes "signed". Then Remove… the second person while they
      edit offline: once they are back online, their offline edit is left out on both
      computers, the status panel says "It was made after its author was removed.", and
      their app refuses further edits. Set a role for them again and check a new edit
      arrives.
- [ ] **Invite links** (3.18; D97; on the next build after this branch merges; about ten
      minutes): on the first computer, File > Share…, Invite someone as Edits some
      categories (tick one), 7 days, Create invite link, Copy, and paste the link into a
      note. On the second computer (with the synced folder, not yet in the budget), File >
      Join from Link…, paste, Continue, type a name, Choose the Budget…, pick the `.mthd`
      in the synced folder: it opens and says "You joined: Edits ...". Edit a line in that
      category (it arrives on the first) and try one outside (refused in words). On the
      first, the Share dialog lists the invite as used by that name. Paste the same link
      again on a third session or computer: "already used". Make another invite, Cancel
      Invite, and try it: "cancelled".

- [ ] **Engine checks in the reference product** (1.17; needs a licensed copy): run what is
      still open in "Engine checks to run" below (16, and the rest of 14: Amt starting at
      1 and one ready line), the charge position above, and the "built but not checked"
      list in docs/research/parity-tracker.md, section 2. The export test plan
      (docs/research/export-test-plan.md) has the steps; exports go in `imports/`, never
      committed.
- [ ] **The Intel code in the window, on this Mac** (D87; **ready now**, about a
      minute): install 0.6.0, select Method Budget in Applications, File > Get Info,
      tick "Open using Rosetta", open it and open Help > Open Sample: Field Station should
      show its totals. Untick the box afterwards.
- [ ] **An Intel Mac** (D87; needs an Intel Mac; **ready now**): install
      0.6.0 from the release's `.dmg` and open the Field Station sample; and on an Intel Mac
      that has 0.2.x, Help > Check for Updates should find 0.6.0, download it and restart
      into it.
- [ ] **Linux live check** (2.25; **eventually**, the owner 2026-10-20): install from the
      release's `.deb` or AppImage (0.7.0, **ready now**); print the topsheet and detail;
      menu keys (Alt+Up and Alt+Down, Ctrl+F, the setup keys); double-click a `.mthd` to
      open it.
- [ ] **Rate tables in the app** (3.9; D90; about five minutes): open the Field Station
      sample, Budget Setup > Rate Tables, New Table: type two invented classifications with
      a Per Day figure (Add unit… for Week), Save Table, then Revert after a change. Import
      from Spreadsheet… with a small sheet of your own (a Classification column and Daily,
      Weekly columns): the columns should be guessed, a bad row listed with its row number.
      On a crew line, Edit > Fill Rate from Table…: pick one; the Rate cell gets a dotted
      underline and its tooltip and Explain say where it came from. Change that figure in
      the table and save: the cell gets a corner mark and keeps its figure; right-click,
      Update rate to … takes the new one. Undo each step.
- [ ] **Help menu on a Mac**: User Guide opens docs/APP.md; Report a Bug opens a new issue
      with the version and system filled in; Licenses and Notices opens and scrolls;
      methodproduction.com opens; About shows "Licensed under MIT OR Apache-2.0".
- [ ] **The app's live checks**: the dialogs, keys and tools-window checks listed in
      docs/APP.md, "Not yet".
- [ ] **A screen reader in the app** (3.14; D82): with VoiceOver on (Cmd+F5), open the
      Field Station sample (`method-budget sample` writes it); click into the topsheet
      grid and arrow through it (each move should be spoken, with the cell's value, column
      and row, "read only" on totals and never "selected"); Shift+arrow to select (it
      should say selected); drill into an account and edit a rate (after a moment it
      should say "Net total ..., up ..."); Cmd+I to add a line; open Rename Budget and
      press Escape (it should say the dialog's name, then come back to the button). Note
      anything said twice, missed or confusing.
- [ ] **An update through method-update** (D83; an Apple silicon Mac; **ready now**: 0.4.0
      and 0.5.0 are both signed method-update releases): install 0.4.0 from its release
      page, then in the installed app: Help > Check for Updates finds and downloads it and
      "Update ready: Restart to update" appears; Restart to update saves, swaps the app in
      /Applications (and asks for an administrator when the folder isn't writable), starts
      the new version with its tabs back, and What's New shows once. Also: the release
      workflow's log shows "signed ... for X.Y.Z" and `latest.json` was written; Help >
      Check for Updates Automatically stays as it was set before the update; an older
      copy (0.3.0, the plugin) updates to the first method-update release.
- [ ] **Recording wishlist** (below): short clips from the reference product, most useful
      first: 1 (real keystrokes), 2 (reports), 8 (saving and opening), 6 (find and
      replace), 4 (versions and history).

### Legal line before building

- **3.9 Rate tables**: the machinery is built (D90, on the owner's direction of
  2026-10-20): tables the user keeps, imported from their own spreadsheets or typed in,
  filling a line's rate, kept in the library and packs. No rate data of any kind ships.
  Only the data waits: a yes on which public union rate sources may be shipped and under
  what licence (docs/research/rate-data-plan.md, section 4, "The ethical and legal line").
- **3.13 Commercial bid layouts**: whether the industry bid form's layout may be
  reproduced. The machinery is built (D92, on the owner's instruction of 2026-10-20):
  users make their own bid layouts, and the app ships only a plain example of its own.
  Only a ready-made layout of that form waits on the answer; nothing of it is built.

---

## Where we are

As of 2026-10-21. Release 0.7.0 is out (2026-10-21): incentive programs complete, rate
tables and bid layouts of your own, the start window finished, on one universal Mac app for
Intel and Apple silicon (decision 87), plus Windows and Linux builds that are included but
not yet tested by hand. The engine covers the v1
calculation model and works figures out as the reference product does by default
(decisions 55 to 61 and 69; exact decimals are a per-budget setting), property-tested.
Budgets save as our own documented, diff-friendly `.mthd` JSON. The Mac app has all of
Phase 1 and Phase 2 but two items: the engine checks that need the reference product (1.17)
and the Windows and Linux live check (2.25). That includes autosave with recovery copies and
backups, recent files, budget tabs and split view, find and replace, versions and compare,
reports with a live preview and PDF, imports from spreadsheets and from other programs'
exports, and a signed, notarized Mac build that updates itself through the suite's
method-update (decision 83). Eighteen of Phase 3's twenty-two items are built, incentive
modeling (3.6), the start window (3.19) and formulas walked without recursion (3.22) among
them; localization (3.14) and working together on one budget (3.18) are partway (3.18's
folder mode, roles, signed changes with removal by cutoff, Start Again and the remembered
owner key are built, decisions 89 and 95, on main for the next release; invites, muted
cells, locks and live sessions are left); rate tables (3.9) and bid layouts (3.13) are partial:
the machinery is built (decisions 90 and 92, tables and bid layouts of your own), and only
bundled public rate data and a ready-made industry bid form wait on the legal line above. Around the app: the `method-budget` command line, an MCP server for coding
agents (3.17), a browser build for the website's demo, the Field Station sample opened once
at the first start (3.20), and Method Launcher's two command-line options and recent list
(3.21). Every release is rehearsed on main first, tests all three systems before anything is
signed, and runs nothing but prebuilt programs once a signing key is in reach. Many built
items still need a live check in the app (docs/APP.md, "Not yet").

---

## Decisions needed from the owner

**Decided or built** (history in docs/decisions.md): 1 compare math, both (D19; built D27,
D33); 2 setup keys, the reference's, shortcut sheet on Shift+Cmd+/ (D19; built 1.7);
3 group rules, matched as three budget settings (D57); 4 group full total (D58); 5
suppressed lines in subtotal rows, the reference drops them as ours does (parity run
ZTEST07, D57); 8 more than one budget, tabs in one window (D19; built D25, split view
D40); 9 autosave into the file, on by default (D19; built D20); 14 who signs, the
organization's Developer ID (D48). Built on the recommendation, open to reversal: 7
renames (D32), 10 new-line defaults (1.4, D60), 11 notes (D35), 12 paste review (D23),
15 presets and library (D29, D31, D37). Still open: 6 and 13 below, and the two D61
items under Waiting on the owner.

6. **Separator with a label.** Today a separator is a bare rule with nothing to edit (D16).
   Options: bare rule; rule with optional centered text. **Recommend: bare rule**; bold
   title lines already do the labeling job. Cheap to add later.
13. **Charge row position on the topsheet.** Reference: after the last production total,
    before the ATL/BTL summary rows; ours: after them (23). Math is identical. **Recommend:
    match the reference**, since printed topsheets get read side by side. D60 kept ours
    until a parity run confirms the reference's place.

---

## Phase 1: Switch-over ready

What the owner needs to do real work every day. Ordered by priority.

| # | Item | Notes | Status | Size | Depends |
|---|---|---|---|---|---|
| 1.1 | **Never lose work**: autosave into the file (default on, toggle), recovery copy for untitled budgets, rolling backups, restore after a crash, recent files list, open a `.mthd` by double-clicking it in the Finder | 07, inv 15, APP | [have] | M | decision 9 |
| 1.2 | **Type-ahead in cells**: units, currencies, sets, locations by prefix; global names in Amt, X, Rate, 4X; Enter or Tab accepts | 02, 07, 09, inv 3, 5 | [have] | M | none |
| 1.3 | **Add on the fly**: typing an unknown unit, currency, set, location or global offers "add it", then applies the edit, one undo step (today an unknown currency is refused) | 02, 07, inv 3, 7 | [have] | S | 1.2 |
| 1.4 | **New-line defaults**: copy fringes (and optionally groups, set, location, currency) from the line above, Amt 1, empty account shows one ready line; per-budget switches in the file | 02, 03, 17, inv 2 | [have] | S | decision 10 |
| 1.5 | **Right-click menus** at each level: cut, copy, paste, insert, delete, subtotal, adjustment, note, text style, make or remove range; topsheet adds production total, charge, credit | 01, 02, 19, 20, 23 | [have] | M | none |
| 1.6 | **Style and note editing in the app**: bold, italic, indent, line color, group color, and a notes editor (side panel, Shift+Cmd+Space, click the note mark) for lines; engine and commands already exist | 01, 19, D16 | [have] | M | 1.5, decision 11 |
| 1.7 | **Shortcut parity pass**: setup keys, Cmd+Left/Right sibling steps on the account list too (stepping categories), range, adjustment, note and bold keys, visible prev/next buttons on the deepest crumb | 01, 02, 03, 09, 14, 17, 19, 20 | [have] | S | decision 2 |
| 1.8 | **Copy and paste rows with their tags**: our own clipboard payload (plus TSV for spreadsheets) carrying fringes, groups, set, location, currency, notes, style and ranges; "paste as new rows" above the cursor; accounts and categories with their lines | 20, inv 18 | [have] | L | 1.9 for cross-budget |
| 1.9 | **Budget tabs in one window** (the owner's call), each tab its own undo; copying between tabs is normal clipboard work; paste conflict review; split view later. Built (decision 25): a tab per budget with its own undo, autosave, recovery and backups; open, new, close with the unsaved question, reorder by drag, Ctrl+Tab, Shift+Cmd+] and [, Cmd+1 and Cmd+2, a Window menu, the tools panel and its window following the tab in front, one question about every unsaved tab on quit, the tabs reopened at the next start. Split view is 2.26; the keys and the close question need a live check | 04, 20, inv 16 | [have] | M | decisions 8, 12 |
| 1.10 | **Move rows**: move up or down by key, cut and paste rows as rows, keep fringe ranges whole. Built (decision 32): Option+Up and Option+Down, Edit > Move Up and Move Down and the right-click menu move the selected lines, accounts or topsheet rows one place, any selection, order kept, quick presses one undo step; a fully selected range moves whole, a line leaving a range never splits it; cut and paste rows is 1.8. Dragging by the # column isn't built; the keys need a live check | APP, inv 18 | [have] | S | 1.8 |
| 1.11 | **Find and replace**: typed search (go to account number, description, fringe, global, group, set, location, unit, currency, notes), results by account with the matching column, jump to each, replace or remove in checked results only, token-exact for globals, one undo step. Built (decision 26): the tools panel's Find tab (Cmd+F, Shift+Cmd+F, F3), every scope above plus formula text, match case and whole word, name suggestions, a global replaced by a global, number or formula, Remove freezing a global's value; renaming a setup item everywhere is not part of it; the keys and the tools window need a live check | 18, inv 5, 17 | [have] | L | none |
| 1.12 | **Versions screen**: save a version, list, rename, add a note, restore (reversible), delete | 01, D17, inv 14 | [have] | M | none |
| 1.13 | **Original and variance columns in the grid** at topsheet and account levels (data already in every view; reports already print it) | inv 14, 17, APP | [have] | S | none |
| 1.14 | **Compare screen**: pick versions of this budget or other `.mthd` files, compare grid on topsheet and account levels (base stays editable), drill to detail lines (our edge), print it | 21, 04, 22, D17 | [have] | L | 1.12, decision 1 |
| 1.15 | **Report sections and PDF**: choose topsheet, category summary, account detail; a tree of categories and accounts; include or leave out suppressed lines and empty accounts; portrait or landscape; page numbers, date and file name in the footer; save straight to PDF | 07, 15, inv 20 | [have] | L | none |
| 1.16 | **Import from the reference product's exports**: all five, JSON, XML (basic and advanced), Excel and tab-delimited, with a reconciliation against the export's own totals, figure by figure. Built (decision 59, docs/IMPORT.md): one reader of the layout and one builder for all five; categories, accounts, lines, adjustment rows, units, currencies, fringes, groups, sets, locations, budget information and the originals come over; fringe posting, cutoffs and ranges, exchange rates, unit hours and (for the workbook) fringe rates are worked out from the figures and listed to check; formulas and globals, production totals (but under production-level posting), suppressed lines and the rules of charges and credits can't come over and are listed; `method-budget import-export`, the `import_budget_export` MCP tool and File > Import Budget Export… with the review before importing. Every parity-run export comes to its own figures but one shown in pounds (0.01 to 0.03). The app's dialog needs a live check | inv 21, ETP | [have] | L | none |
| 1.17 | **Engine fixes from the export test**: currency direction, cutoff factor, flat fringe rules, adjustment rows, charge base, rounding points, whatever the totals disagree on. Done from the parity runs: D54 to D61 and D69 (engine checks 1 to 13 and 15). Left: engine checks 14 and 16 and the parity tracker's "built but not checked" list, which need a licensed copy | ETP, inv Q2 to Q11 | [partial] | M | 1.16 samples |
| 1.18 | **Rename setup items** with every reference rewritten (globals token-exact in formulas, fringes, groups, units, sets, locations, currencies). Built (decision 32): the editor's name field, Rename everywhere in the lists' right-click menu, a global's or unit's name, Find's replace panel and the base currency setting; one undo step, says how many places changed; charges, credits, report, tag and line presets follow; the new name must be free (no merging), so renaming back gives the identical budget (property-tested) | 09, APP | [have] | M | decision 7 |
| 1.19 | **Tag presets**: save a line's tags as a named bundle, apply to a selection with add or replace, in the Apply panel, stored in the budget. Built (decision 31): a Presets tab in the tools panel (after Units) and Apply preset > in the right-click menu; Save tags as preset… from the cursor line (a box per tag), edit, rename, delete, reorder (keyboard); Add (fringes and groups joined, set, location and currency only where a line has none) or Replace (the five tags become exactly the preset's) on lines or everything under selected accounts or categories, one undo step, a preset naming a deleted item refused. Needs a live check in the app | 05, inv 13 | [have] | M | decision 15 |
| 1.20 | **Start a budget from another**: "import setup from another .mthd" (pick items, dependencies come along) and "new from template" (a budget with its chart and setup, no lines). Built (decision 29): File > Import Setup From… (every kind of setup item, the presets and the three budget settings, each new, the same or different here, what an item needs ticked along with it, the paste review's keep, imported or new-name choices, one undo step); File > New From Template… (any `.mthd`, opened in a new untitled tab without its lines, info, baseline, versions or notes, "keep the detail lines" optional) and Save as Template… (the same stripping, into a new file). Bundled chart-of-accounts templates are 3.16; the native dialogs need a live check | 24, 07, inv 12 | [have] | M | decision 15 |
| 1.21 | **Signed and notarized Mac build**, no Gatekeeper step. Built as a pipeline (decision 48): the release workflow signs the app and disk image with the Developer ID, notarizes them with an App Store Connect API key, staples and checks them with `codesign` and `spctl`, each step skipped cleanly without its secrets (docs/RELEASING.md). The first signed release needs the secrets set | INSTALL.md, RELEASING.md | [have] | S | decision 14 |

Already in place and worth keeping as-is: drill and climb keys and cursor memory (03),
breadcrumb jump lists (01, 03), Apply panel with tri-state boxes, dollars and ranges (01,
06, 13), column menu with pinned # and Total (01, 10), footer context totals and change
counters (01), percent and flat charges with exclusions matched to the cent (11, 23),
live recalc on every setup change (07). A test that toggling a group twice returns every
total to the cent exists in spirit (D4 property tests); keep it explicit (02).

---

### Phase 1 addition (the owner, 2026-10-09)

| # | Item | Source | Status | Size | Depends |
|---|---|---|---|---|---|
| 1.22 | **Tools panel rebuild**: two stacked tab panes (Apply and Tables merged; checkbox applies, double-click edits), drag tabs between panes, dock right (default) / bottom / pop-out window, remembered. Built (decision 24). The Budget tab and every table's full editor moved to a Budget Setup window (toolbar Setup, View > Budget Setup, the setup keys), and the panel's tabs became compact apply lists (decision 42); the pop-out window and Budget Setup need a live check | the owner's mock review, decision 21 | [have] | L | inline editing (in progress) |
| 1.23 | **Updates inside the app**, one version everywhere, release notes. Built (decision 48): background checks (after start, then every six hours; Help > Check for Updates Automatically turns them off) and download, signature checked against the app's own key, "Update ready: Restart to update" in the toolbar, every budget saved and the tabs and split view back after the restart, What's New once after an update and Help > Release Notes; `cargo xtask` checks and bumps the version and drafts notes facts; the release workflow writes the update manifest. Off until the updater key exists and the repository is public; needs a live check of a real update | RELEASING.md, APP.md | [have] | M | 1.21 |

## Phase 2: Parity and polish

| # | Item | Notes | Status | Size | Depends |
|---|---|---|---|---|---|
| 2.1 | **Spreadsheet export**: CSV and Excel of topsheet, accounts and detail with tags; a tab-delimited flavor for accounting tools. Built (decision 30): File > Export (Excel Workbook, CSV Files, Tab-Delimited Detail) and `method-budget export`; one workbook with Topsheet, Accounts, Detail and the setup tables (globals, fringes, groups, units, currencies, sets, locations), money exact from cents, header rows bold and frozen; written without new crates (our own ZIP and SpreadsheetML); opening the workbook in Excel and Numbers needs a live check | inv 21 | [have] | M | none |
| 2.2 | **Spreadsheet import with column mapping** (any department's sheet, not just the reference product's). Built (decision 36): File > Import Spreadsheet… reads .xlsx, CSV and tab-delimited files with our own ZIP, DEFLATE and XML readers (no crates, hostile-file limits); the dialog picks the sheet and header row (both found automatically), a column per field (guessed from the titles), whether to add missing accounts, categories and setup items, the account for rows without one, and this budget (one undo step) or a new tab with its chart and setup; the preview shows the first lines with their totals, skipped rows and why, and a reconciliation against the sheet's line totals; mappings are saved per user and come back for a sheet with the same titles; our own export imports back to the same totals (tested). Needs a live check with real Excel and Numbers files | inv 24 | [have] | M | none |
| 2.3 | **Report layout options**: font size, vertical rules, shading, group colors or black and white, collapse details to one line per account. Built (decisions 28 and 33): type size small, medium or large, shading, group colors, lines between columns, and the detail level as one line per account, all kept in the preset. Point sizes (the reference's 5 to 10 pt) are three steps here | 15, inv 20 | [have] | M | 1.15 |
| 2.4 | **Report templates in the budget**: named, re-rendered from live data, with header and footer zones and live fields (page, dates, time, file, budget info, globals), first-page-only switch. Built (decisions 28 and 33): named presets in the `.mthd` (save, save as new, rename, delete, default), drawn from the budget as it is; three slots per band, each one field (title, section, date and time, date, time, file, page, an info field, a global, text); a first-page-only switch per band. Not built: text and fields mixed in one slot, created and modified dates (the budget keeps none), one header design shared by several presets | 15, inv 20 | [have] | L | 1.15 |
| 2.5 | **Extra report sections**: setup tables, budget info, chart of accounts, group totals, notes report; topsheet fringe column and "detail starts on page N". Built (decisions 28 and 33): fringes, globals, groups with totals, units, sets and locations with totals, currencies, chart of accounts, budget information and the notes list as sections; optional Fringes and Page columns on the topsheet. Charges and credits as their own setup tables are not a section yet (the topsheet lists them) | 15, 19, inv 20 | [have] | M | 1.15 |
| 2.6 | **Live report preview** with page navigation and a section jump list. Built (decisions 28 and 33): the setup dialog's preview is the printed pages, redrawn as choices change, with first, previous, next and last, a page number to type, the page count and a Go to section list | 15 | [have] | M | 2.4 |
| 2.7 | **Compare extras**: optional Sum and Average columns, each production-total row, charges and credits per item, legend of which file is which. Built (decisions 27 and 33): Sum, Average, Variance and Variance % columns, each production total, each charge and credit under its total (matched by name), the column headers naming each letter's budget and the printed comparison's legend, printing and PDF | 21 | [have] | M | 1.14 |
| 2.8 | **Notes, fuller**: several dated notes per line, notes on accounts and categories, a notes list with jump-to (a review checklist). Built (decision 35): dated notes (author from a local name setting, edited time, no replies) on lines, accounts and categories, older files' single note read as one undated note; the Notes tab's cards with add, edit and delete for the cursor's line, account or category; All notes as a checklist (tick off as reviewed, Open only, jump to a line, account or category); Shift+Cmd+Space adds a note at every level; reports print every note dated, with a switch to leave reviewed ones out. Needs a live check in the app | 19, inv 16 | [have] | M | 1.6 |
| 2.9 | **In-use counts and totals in setup tables** (units, globals, fringes, groups, sets, locations), used/unused filter, warn before deleting something in use, click to find uses. Built (decision 35): every setup list (currencies too) shows each item's line count and, where it means something, its dollars across the budget; Show All, Used or Unused; a click on a count runs Find for its uses; deleting something in use says how many lines and offers to take it off them (currency lines back to the base, unit columns blanked) or Cancel. Needs a live check in the app | 06, 09, 12, 14, 18, 24, inv 3 | [have] | M | 1.11 |
| 2.10 | **Globals, fuller**: description, unit, display decimals, "show by name" in cells, formula view while a range is selected. Built (decision 34): formula cells show their value, a global's details (description, unit, decimals, show by name, folder) in the Globals tab, View > Show or Hide Formulas in Selection (Option+Cmd+F). Folders don't group the list yet. Needs a live check in the app | 02, 09, 17, inv 5 | [have] | M | none |
| 2.11 | **Short codes (ID) for fringes and groups**, shown in the grid's tag columns. Built (decision 35): an optional short code per fringe and group (unique ignoring case, not another item's name), set in the item's editor or its right-click menu, shown in the list, in the grid's Fringes and Groups columns (names in the tooltip) and matched by the type-ahead; display only, so renames are unaffected. Needs a live check in the app | 12, 14, inv 4, 6 | [have] | S | none |
| 2.12 | **Custom order for setup tables** (drag, saved in the file, followed by reports and the Apply panel), click-to-sort headers that cycle back to custom, no dragging while filtered. Built (decision 35): a hand-made order per table in the file (`setup_order`, left out when alphabetical), set by dragging an item's handle or Option+Shift+Up and Down; followed by every setup list, the type-ahead and printed setup tables; the Name head cycles A to Z, Z to A and back; no moving while sorted or filtered. Folders are not built. Needs a live check in the app | 12, 16, inv 12 | [have] | M | none |
| 2.13 | **Apply panel extras**: clear all per list, replace all tags, per-group line count across the budget, fringe table footer (percent, flat, total). Built (decision 38): Clear on the fringe and group lists, Replace tags… under the selection's totals (a tag-preset-style form, starting from what every selected line shares), the fringe list's foot split into percent, flat and total; the per-group line count across the budget was already there (the Lines column, decision 35). Needs a live check in the app | 05, 06, 11, 12, 13 | [have] | S | none |
| 2.14 | **Line-block presets**: save lines (with a checklist of fields) and insert copies at the cursor, one undo step. Built (decision 31): Save lines as block… (keep amounts, units, tags, notes; descriptions, kinds and styles always), Insert above the cursor line from the Presets tab or Insert line block > in the right-click menu, new ids and fringe ranges, the block's own copy of its setup offered back through the paste review when the budget lost an item. Needs a live check in the app | 05, inv 13 | [have] | M | 1.19 |
| 2.15 | **Personal setup library**: an open library file in a folder the user picks, folders per kind, copy in and out with dependencies and a conflict review, no live link. Built (decision 37): the library is a `.mthd` with setup and no lines (`My Method Budget Library.mthd` made in a folder picked in Rust's dialog, or any budget file; remembered per user; switchable), starting empty; a Library tab in the tools panel (second pane, draggable) browses it by kind and folder with a filter and each item's definition; Add to This Budget and Save to Library (the tab, or a setup list's right-click menu) run Import Setup's review both ways (dependencies along; the target keeps its version unless the user picks the other or a new name), adding being one undo step; folders made, renamed, deleted and items moved, kept in the file as `setup_folders`; written atomically; while the library is open as a tab it is read from there and changes from the Library tab are refused. Needs a live check of the dialogs and the tab in the app | 07, 09, 11, 24, inv 12 | [have] | L | 1.20, decision 15 |
| 2.16 | **Charges and credits polish**: percent and excluded amount in the topsheet label, exclusion grid with live group totals, preview that includes the cell being typed, credit rows showing gross, excluded fringes, net and variance to cap, "create charge" from the topsheet menu. Built (decision 38): two label switches per charge (saved in the budget, screen and printed topsheet), every group with a box and its amount in the base, a live total from the typed text (`preview_charge`), credit row figures with the cap variance, Create charge… and Create credit… on the topsheet's right-click menu. Needs a live check in the app | 11, 23, inv 9, 10 | [have] | M | 1.5 |
| 2.17 | **Two tools at once and dock at the bottom**, so the grid gets full width beside a setup list (done with 1.22) | 01, 04, 06, inv 18 | [have] | M | none |
| 2.18 | **Display preferences**: rate decimals with a cutoff band, totals decimals, hide zero accounts, show or hide summary rows and the net total, grid zoom. Built (decision 34): rate band and summary rows in Budget > Settings (saved in the budget); totals' cents, zero accounts and zoom in the View menu (per user). Screen only: reports keep their own options. Needs a live check in the app | 17, inv 1, 18, 22 | [have] | M | none |
| 2.19 | **Hidden-factor marker**: flag a line whose hidden X, 4X, unit or currency column changes its total. Built (decision 34): a dot in the # cell naming the hidden X, 4X or currency; unit columns never change a total, so they never mark. Needs a live check in the app | inv 2, 23 | [have] | S | none |
| 2.20 | **Renumber accounts when the coding pattern changes**. Built (decision 39): Apply in Settings > Account numbers previews old and new numbers (categories, accounts, fringe accounts) and offers to renumber, one undo step, rewriting charges, credits, report presets and baseline keys; conflicts refuse. Needs a live check in the app | APP, inv 11 | [have] | M | none |
| 2.21 | **Share-ready copies**: save a snapshot named by title, version and date, optionally without notes or private info, stamped with where it came from; open read-only; offer to compare a returned copy or store it as a version. Built (decision 39): File > Save a Copy for Sharing…, `provenance` in the file, read-only banner with Make Editable Copy, Compare With Original and Store as a Version. Needs a live check in the app | 22, inv 16 | [have] | M | 1.14 |
| 2.22 | **Sub-budgets**: a filtered copy by categories, accounts and tags, optionally without fringes or notes. Built (decision 39): File > New Sub-Budget…, groups, sets and locations OR within a kind and AND across kinds, new untitled tab stamped with its parent; no fringe lock yet. Needs a live check in the app | 04, inv 14 | [have] | M | none |
| 2.23 | **History list**: every undo step listed, jump back, or branch it into a version. Built (decision 40): every undo step named by its command ("Edit rate on 21-01 line 3", "Paste 12 rows") with its time; the History tab (bottom tools pane) lists them oldest first, a click undoes or redoes to a step, "Version…" saves a step as a named version without changing the budget. Needs a live check (the tab in the tools window, long histories) | inv 15 | [have] | M | 1.12 |
| 2.24 | **Group rules as budget settings**. Built (decision 57): `group_rules` with the reference's three rules (lines with no group, a line in a counted and an uncounted group, fringes in group totals), each written only when not the reference's default; Budget Setup's Groups table, `method-budget set` and scripts set them | 14, 24 | [have] | M | decision 3 |
| 2.25 | **Windows and Linux live check**: printing, menu keys, installers, file association. Windows hands-on is deferred indefinitely (the owner, 2026-10-20; built and tested automatically on every release); Linux eventually | APP | [partial] | M | none |
| 2.26 | **Split view**: two budget tabs side by side in one window, each pane with its own tab strip, breadcrumb and grid; one pane is the target (dot and bold tab) and the toolbar and tools act on it; move a tab to the other pane; exit merges the panes. Built (decision 40): View > Split Right (Shift+Cmd+\\) and Close Split; each pane its own tabs, breadcrumb, grid and footer; the target (accent on its strip, its tab bold) is what the toolbar, menus, keys, tools panel and formula bar act on; one budget can show in both panes; Move Tab to Other Pane (Window menu, a tab's right-click menu) or drag a tab onto the other strip; a resizable divider; the panes kept in tabs.json. Needs a live check (the keys and menus in the system web views, the tools window following the target) | 04 | [have] | M | 1.9 |

---

## Phase 3: Beyond the reference

Where competitors and the pain points (inv "Gaps and pain points", inv 24) say we can win.

| # | Item | Why | Status | Size | Depends |
|---|---|---|---|---|---|
| 3.1 | **Budget diff**: `method-budget diff` and an in-app "what changed" between two files or versions, by row id and account code, in plain words with money deltas. Built (decision 44; docs/MERGE.md): `method_core::diff` matches rows by id, accounts and categories by number (renumbered ones by what they hold), setup items, charges and credits by name, and lists every change with its fields before and after and a sentence (`21-01 line 3: rate 1,200.00 -> 1,350.00 (+1,500.00 to 21-01)`), plus the account, category and topsheet money that moved; `method-budget diff a b [--json] [--exit-code]`; File > What Changed… against the saved file, a version, an open budget or a file, grouped by account, a click going to the change. Needs a live check in the app | no real collaboration in the reference; our files are text | [have] | M | 1.14 |
| 3.2 | **Three-way merge**: `method-budget merge` plus a git merge driver, merging edits to different lines automatically and listing real conflicts for a person to pick; turns "send a copy" forks into merges. Built (decision 44; docs/MERGE.md): `method_core::merge` merges different rows, fields and setup items on their own (tags name by name, notes note by note), lists same-field clashes, deleted-versus-changed rows, items, accounts and categories and both-side reorders as conflicts (ours by default, a pick per conflict), keeps rows both sides added with one id as two; property-tested; `method-budget merge base ours theirs -o out [--theirs]` (exit 1 with conflicts) and `method-budget merge-driver %O %A %B %P` for git; File > Merge Changes From… with the base found from a shared copy's provenance (worked out as it was sent), conflicts picked per item, one undo step. Needs a live check in the app | the reference only sends divergent copies | [have] | L | 3.1 |
| 3.3 | **Review files**: a reviewer marks up a read-only copy, comments land in a small sidecar file the author can merge back. First slice built (decision 81; docs/APP.md, "Review files"; docs/FORMAT.md, "Review files"): `<budget>.review.json` beside the budget (its own versioned format; the `.mthd` unchanged), comments anchored by row id, account number, category code, setup item or the whole budget, with author, time, text, resolved and the author's outcome; File > Open for Review… (the budget read-only, the file written after every change) and the Comments tab (add on the cursor's line, account or category or the whole budget, edit, resolve, delete); File > Open Review Comments… on the author's budget (each comment at its place, bars on the number cells, merge as dated notes in one undo step, Merge All, dismiss and restore, a comment whose place is gone listed with where it was); `method-budget review` (list, `--add`, `--merge`, `--dismiss`, `--apply`), the `review_comments` MCP tool, the browser build. Left: writing comments on setup items from the app, replies, several reviewers' files at once, notes on setup items (a format change). Needs a live check in the app | local stand-in for shared comments | [have] | M | 2.21 |
| 3.4 | **Version deltas** so many versions don't bloat the file. Built (decision 41): a budget-level `compact_versions` switch (off by default, so every other file is unchanged and older readers still read it) writes each version as a JSON Patch against the next version (the newest against the budget), keeping a full copy where a delta isn't smaller; reading rebuilds full snapshots either way. Ten versions of the 5,000-row sample: 2.0 MB instead of 26 MB (the budget alone is 2.0 MB). `method-budget set f.mthd compact_versions true` turns it on; no app switch yet | D17 | [have] | M | none |
| 3.5 | **"Explain this number"**: trace any total down to lines, fringes, cutoffs, ranges and rounding steps. Built (decision 43): `method_core::explain` works any total out again from the budget as a tree of steps whose leaves add up to it exactly (tested on every total of the 5,000-line sample, the hand-checked fixtures, every posting mode and filter): lines with globals by name and value, currency, every rounding, fringes per line with cutoffs, ranges and largest-remainder shares, posting, left-out lines and why, charge bases and exclusions, credit gross, excluded, net and caps. In the app: right-click a total (any level, total rows too) > Explain This Number…, or Cmd+E on the cursor cell, opens the Explain tab (collapsible tree, deep steps fetched when opened, a click goes to the line); `method-budget explain file.mthd <total>` on the command line. Needs a live check in the app | opaque fringe math and rounding complaints | [have] | M | none |
| 3.6 | **Incentive modeling**: per-location qualified spend, several programs side by side, caps and broker discounts, what-if by moving lines between locations; programs authored by the user from public rules, never bundled vendor data. Built so far (decision 80): a program is a credit, and a credit's optional `locations` limit its group, category and account rows to lines at those locations with their fringes (left out when empty, so other files are unchanged byte for byte); every credit, applied or not, gets an estimate from the topsheet's own code (caps, the discount taken off after the cap, one rounding), with its net spend by location, and the spend at each location; Explain, diff, merge, rename, setup import, setup sheets and the change log follow the field; property-tested (no programs change nothing, caps never exceeded, order independent, spend adds up by location). In the app: Budget Setup's Credits tab has Spend at these locations, an estimate for an unapplied credit in the rail and footer, and Programs side by side and Spend by location tables; `method-budget incentives [--json]` and the MCP `incentives` tool. Then (decision 88): an optional minimum spend (compared with the qualified spend after the locations, before any cap; below it the program pays nothing and says so), a flat broker's fee taken off after the cap and discount, never below zero, and caps shared by programs (a named amount in the budget; own cap first, then shared in proportion to the cent, remainder by largest remainder then name; an unapplied program estimated alongside the applied ones), through Explain, diff, merge, setup import and sheets, the change log, `incentives`, MCP `set_credit` and the Credits tab's terms band and Programs side by side; additive to format version 1, left out when unset. Then (decision 93): a what-if that moves lines (rows, or an account's or category's lines, optionally only those at one location now) to another location on a copy of the budget and shows every program's estimate before and after, the difference, the credits taken off, the unapplied estimates and the net total before and after, from the same code as the topsheet, and applies as one undo step (property-tested: applying then estimating gives the what-if's "after", the order of the lines changes nothing, an empty what-if changes nothing); an unapplied credit's rows table shows its row figures marked as an estimate; and a printed incentives-by-location report (report section `incentives`: each location's spend and the programs counting it, every program's terms, totals that reconcile with the topsheet's credits) in the preview, PDF, `method-budget report` (text, HTML, CSV, PDF) and an Incentives table in the spreadsheet (.xlsx) and CSV exports of a budget with credits. In the app: the Credits tab's What if… panel; `method-budget incentives what-if` and the MCP `incentives_what_if` tool. Needs a live check in the app | locations and credits exist; no one makes this easy | [have] | L | 2.15 |
| 3.7 | **Actuals and cost reports**: import actuals (CSV), budget vs actual vs estimate to complete by account. Built (decision 45): actuals kept in the budget (`actuals`: transactions, import batches, per-account ETC methods; not content, so versions don't carry them); File > Import Actuals… reads CSV, tab-delimited and Excel exports with a column mapping (date, account, amount or debit and credit, vendor, description, reference, tags; guessed, saved per user), dates of every common shape, account numbers through the budget's numbering, a review of numbers the chart lacks (keep unassigned, skip, map) and of duplicates (date, account, amount, description), one undo step and one removable import; `method_core::actuals` works out actual, ETC (budget minus actual, manual, percent complete), EAC, variance and percent spent per account, category fringe account, category, level and the topsheet exactly to the cent, with an unassigned bucket (property-tested); the grid's Actual, ETC, EAC, EAC Var and % Spent columns on the topsheet and account list (Actual on lines), over-budget marked; the Actuals tab (filter, edit, reassign, delete, remove an import, set an account's ETC); a Cost report print section and Cost Report and Actuals export sheets. The rollup over the transactions is cached, so an edit costs the same with 20,000 transactions. Needs a live check with real accounting exports | top competitor gap | [have] | L | 2.2 |
| 3.8 | **Schedule-linked quantities**: read a schedule's day counts from an open export and drive globals. Agreed input: the `method-schedule-days` file written by Method Schedule (`method-schedule budget FILE`, spec in that repo's docs/EXCHANGE.md); a performer's days default to the day out of days total, a missing element counts 0 with a warning. Built (decision 76; docs/APP.md, "Globals from a schedule's days"): the reader for versions 0 and 1 (`method_format::import::schedule`, spec copy docs/research/schedule-exchange.md); File > Import Schedule Days… with a review of every figure (shoot days in all, by unit, set, location and week, each element's days) and the global it would set, its value now and the new one, applied as one undo step (logic in method-session's `schedule_days`, so the browser build has it too); `method-budget schedule-days` (dry run unless `--apply`) and the `schedule_days` MCP tool. The rule: shoot days in all to `SHOOT_DAYS`; any other figure only when picked, to a global the person names or `<NAME>_DAYS`; prep and wrap untouched. Each global set keeps its figure and the figure's key (an element's id, category, board ID and name) in its setup source (no format change), so the next import finds it by key, picks it again, says what the schedule changed and offers a figure gone from the file at 0, unticked. Proposed next, a format change (additive, so a new optional field): a budget `schedule_link` holding the file's `schedule`, `board`, `sub_board`, `days_hash` and `source_hash` as last applied, and per global the figure's kind and key, with an element's (category, board ID, name) and id as agreed, so a budget can say "the schedule changed" from the hash alone, find a renamed element by id (asking before relinking), and offer the element counts apart (work, hold, travel). Also later: the shoot dates to budget info from the review, and reading several sub-boards' files at once | competitor feature; the reference only imports static blocks | [have] | L | 2.2 |
| 3.9 | **Rate tables**: user-kept tables from public union rates; pick a classification to fill the rate (flag the legal line before shipping any data). Built (decision 90): the machinery, with no bundled data of any kind. A budget keeps rate tables (classifications with a rate per unit, optional dates in effect, notes) as setup tools, through Import Setup, the setup library, setup packs and setup sheets; Budget Setup's Rate Tables tab edits them and imports one from the user's spreadsheet with a column mapping (a column per unit, or a unit and a rate column); Fill Rate from Table… on a line writes the figure once (converting only through units with hours) and remembers its source, Explain says where it came from, and a changed table offers Update rate without changing any figure; `method-budget rates` and MCP tools. Not yet: bundled public rate data (waits on the owner's legal-line answer). Needs a live check in the app | competitor add-on | [partial] | L | 2.15 |
| 3.10 | **Overtime helper** for labor lines. Built (decision 43): the tools panel's Overtime tab (also Edit > Overtime Helper… and the line's right-click menu) on the cursor line: straight-time rate, hours a day, straight-time hours, overtime bands, sixth and seventh day multipliers, days a week; shows a day and a week exactly (pay per multiplier, straight-time equivalent hours, average an hour) and writes the day's or week's pay into the line as its rate or as one line per multiplier in one fringe range, one undo step. Patterns saved by name in the budget (`ot_patterns`, tools, not content). No union rules shipped. Needs a live check in the app | competitor feature | [have] | M | none |
| 3.11 | **Person-level cutoffs across accounts**, opt-in, for one person paid in several accounts. Built (decision 79): a line or adjustment row's optional `person` (free text; FORMAT.md, additive, left out when unset) makes every counted row with that person, in any account, one earner for each percent fringe: within the cutoff each row earns its own percent; past it the person earns the percent of the cutoff once, split over the rows by pay in whole cents (largest remainder, ties to the lower row id), so the order of lines and accounts never matters; a person row leaves its fringe range. Budgets without a person are unchanged (tested). Explain shows the person, the cutoff and each line's share; the lines' optional Person column (shown once a line has one) has type-ahead and the person's pay against each cutoff as its tooltip; `set line ROW person NAME` and the MCP `update_lines` set it. Property-tested (no double counting, order-independent, excluding equals deleting). Needs a live check of the column in the app | inv 23 quirk | [have] | M | engine check 1 |
| 3.12 | **Scriptable command line**: batch edits, headless PDF and spreadsheet output for pipelines. Built (decision 41, docs/CLI.md): `method-budget report` (a saved preset as HTML, text, CSV or a plain Courier PDF from our own small writer), `method-budget export --xlsx/--csv/--tsv`, `method-budget totals [--json]`, `method-budget set` and `method-budget apply` (a line-oriented edit script: globals, fringe rates and cutoffs, line fields by row id or account and line, tags, renames, charges; atomic, all or nothing, `--dry-run` with a diff), `method-budget import` with a JSON column mapping; documented exit codes. The app's typeset PDF stays Mac-app only | open format advantage | [have] | M | 2.1, 1.15 |
| 3.13 | **Commercial bid layouts**. Machinery built (decision 92): a bid layout is part of a report preset that the user makes: header fields with their own labels, sections that take accounts by category, account or a range of account numbers, running subtotals, and markups that are a percent of chosen sections or a flat amount; the Bid section prints it in the live preview, the PDF and the command line's text, HTML, CSV and PDF. Every account counts once (the first section that takes it, else Unassigned, which is shown); the budget's own charges and credits carry through, so the bid total is the budget's net total plus the bid's markups (property-tested). The app ships one plain example layout of its own. Additive format field (`bid`), byte-identical when unused. Not yet: a ready-made layout of the industry bid form, waiting on the owner's legal-line answer; needs a live check of the Bid layout group in the app | competitor feature; legal line on form layouts | [partial] | L | 2.4 |
| 3.14 | **Localization and accessibility pass**. Accessibility, first slice built (decision 82; audit and what is left: docs/research/accessibility-audit.md; docs/APP.md, "Screen readers and contrast"): the budget grid is an ARIA grid that follows the cursor with `aria-activedescendant`, with row and column places, read-only and selected cells, and a cursor cell named by its value, column and row; an edit that moves the net total is announced politely; every small dialog is a labelled modal dialog; secondary text, the note amber and the popup menu's focus pass contrast in light and dark. The browser build's smoke test checks it headless through Chrome's accessibility tree. Localization, framework and English built (decision 94; docs/APP.md, "Languages"): one string table, `ui/i18n/en.js`, read by the page (`t(key, params)` with names filled in, plural forms from `Intl.PluralRules`, English for a missing key, the system's language picked) and by the menu bar from Rust; the start window and New Budget, the Help menu and About, the shortcut list, the menu bar, Budget Setup's list, Globals, Units and Rate Tables tabs, Find and Replace, the main window's dialogs and the status bar converted in the first pass, and sharing, the Activity tab, Budget Setup's Charges, Credits, Info, Settings and Baseline tabs, the printed reports and the report setup in the second, and the tools panel's frame, Notes, History, Versions, the Library, Review, Compare, the tab strips, split view, the right-click menu, util.js's shared phrases and the toolbar in the third, and the rest of the page in the fourth: the tools panel's other tabs, the import dialogs, What Changed and Merge, sharing's banners, the grid's levels, quick entry and row colors, backups and the small scripts (about 3,850 entries then); a test fails on a new inline string in any page script, a script it doesn't list, a missing key or an unused entry. Engine messages with stable codes, first slice (decision 94, fifth pass): every refusal and validation error of the edits is a code with parameters in method-session's catalog (`messages.rs`; the command line, MCP and browser print the same English), and the app shows the table's words for it; the native dialogs' titles, file-type names and buttons read the table through `tr()` (about 4,060 entries then). Second slice (decision 94, sixth pass): Find and Replace, presets, apply, pasting, moving rows, setup lists, tabs, the read-only and review refusals, "That step is no longer in the history.", Review and its summary, What Changed's and Merge's refusals and labels, actuals, the imports with their summaries and notes, and Explain's refusal have codes too (362 in the catalog), method-core's reasons for a formula's value, a number and a numbering pattern are catalogued as `engine.core.*` without method-core reading a table, and sharing's refusals and the window titles read the table (about 4,370 entries). Left: a run with real screen readers, the footer's other totals, focus going back to a redrawn opener, an empty grid's focus, forced colors and reduced motion; for localization, the undo labels, Explain's steps, What Changed's change and Merge's conflict sentences, method-core's formula parse and calculation errors, method-format's import problems, the Activity tab's reasons and the method-session modules not yet converted (notes, versions, the Library, packs, rates, overtime, Compare, reports) are shown as they come, room for longer words and a right-to-left check, a language override, and the languages themselves, waiting on the owner's choice (the audit lists it all) | reference is English only | [partial] | M | none |
| 3.15 | **Setup packs**: import and export sets of setup items (globals, groups, fringes, units, sets, locations, currencies, charges, credits, tag presets, overtime patterns) as shareable packs with their sources, and setup lists as spreadsheets. Built (decision 50): a pack is a `.mthd` with setup and no lines, provenance `setup_pack` with a title, description, author, version, effective date, region, notes and links; File > Export Setup Pack… and the Library tab's Setup packs section pick items by kind and folder with their dependencies; File > Import Setup Pack… and the listed packs (your Packs folder and the bundled `packs/`, empty but for a README) run Import Setup's review into a budget (one undo step) or the library, stamping what comes in (`setup_sources`); File > Export > Setup Lists… and Import Setup Lists… (a sheet per kind, checked row by row); `method-budget pack export`, `import` and `show`. Rate packs come from a separate data project after review. Needs a live check of the dialogs and the bundled folder | shareable setup, 2026-10-10; 3.9 | [have] | L | 1.20, 2.15 |
| 3.16 | **Charts of accounts**: bundled chart templates and importing any chart. Built (decision 51; docs/TEMPLATES.md): File > Import Chart of Accounts… and `method-budget coa import` read a chart from any sheet of a workbook, CSV, tab-delimited text or another budget (flat lists, outlines, side-by-side columns, number and name in one cell, separators or none, lost leading zeros, header, total and level rows, the `kind,code,name,parent` layout), infer the numbering pattern (changeable) and preview with duplicates and gaps; into a new budget with no lines, or applied to this one (adds, renames by number, never removes an account with lines, one undo step). Seven charts written by us and the studio charts built from the owner's exports ship in New From Template under Charts of accounts and in `method-budget new --template`; the sources sit in `imports/coa-sources/` (never committed) and are built by `scripts/build-coa-templates.py`. Needs a live check of the dialogs in the app | top request: start from the studio's chart | [have] | L | 2.2, 1.20 |
| 3.17 | **Agents can work on budgets**: a Model Context Protocol server so Claude Code and other coding agents open, read, edit, check and report on budgets with typed tools. Built (decision 52; docs/MCP.md): `method-budget mcp [--root DIR]... [--read-only]` over stdio, both protocol eras (the `initialize` handshake from 2024-11-05 to 2025-11-25, and 2026-07-28 per-request metadata); 43 tools: open and summarize, topsheet, accounts, lines with what they work out to, globals, setup tables, charges and credits, totals, explain, find, validate, diff, merge, add, update, delete, move and tag lines, setup add, change, rename everywhere and delete with a use check, charges and credits, categories, production totals and accounts, export and reports, spreadsheet import, new budgets from the starter or a template, copies and undo; every path inside a root (symlinks out, hidden folders and wrong extensions refused), atomic writes, a change refused when the file changed on disk since the server read it, shared copies read-only, input limits; an example agent skill in docs/agents/. A live bridge to the running app (edits through the session, in History and undo) is designed in docs/MCP.md, not built | agents should push the buttons too | [have] | L | 3.12 |
| 3.18 | **Working together on one budget**: several people in the same budget, changes merging on their own; first through a shared folder you already use (Dropbox, iCloud Drive, Google Drive, LucidLink), then live sessions with presence and cursors over an encrypted peer-to-peer link; roles (edit everything, some categories, actuals, comment, read only), who-changed-this, per-person undo. Design: docs/research/collaboration.md. Built so far (decisions 62, 64, 78, 89, 96 and 97): the change log under the budget and `method-budget log` and `method-budget sync`; in the app, folder mode end to end: File > Share, every edit recorded and shared through the folder, the keeper's file, the status item, presence, flashes, conflict badges, Review and Activity tabs, Who Changed This, per-person undo; roles (edit everything, some categories, actuals, notes, read only, admins, co-owners) set in the Share dialog, checked on every change where it was made, refused in plain words; key pairs and signed changes (decision 89): each computer's key kept in the app's data folder, every change signed from the owner's first role on and checked by every computer, changes with a missing or wrong signature left out and listed, Remove person with a cutoff (later changes from them left out everywhere, a role given again brings them back), the command line and MCP signing with the person's key, the change log at format 3; the department-role interface and access requests (decision 96): a department editor's categories and accounts outside their scope muted with a lock, typing in one saying so with "Ask <owner> for access", requests written into the asker's heartbeat file (nothing sent anywhere) and shown to whoever may give that role in the status panel and as a toolbar badge with Allow, Allow those categories only (a signed role change) and Decline (remembered on that computer). invite links (decision 97): Invite someone in the Share dialog makes a signed, single-use, expiring invite and shows its link once, File > Join from Link… takes it up from the newcomer's synced copy of the folder, Cancel works as a cutoff, the change log at format 4. Next: live sessions; clickable `methodbudget://` links if people ask; later, grants that expire; key rotation and publishing a key from the command line come later (the owner, 2026-10-20). Also later (review): settle a contested change number without refolding the whole log, so planting many forgeries can't make a large shared budget slow to open. Roles, signing, Remove, muted cells, access requests and invite links need a live check in the app | the reference only sends copies | [partial] | L | 3.1, 3.2 |
| 3.19 | **New project window**: start a budget from one window: title and production details, chart of accounts or template, schedule length, currency and union or location rate setup, then open it ready to fill. Design: docs/research/new-project-window.md. Phase 1 built (decision 63): a start window at launch when nothing is open and from File > New… (Cmd+N) with pinned and recent budgets (title, net total, last changed, folder), search, open, pin, show in folder and remove, all from the keyboard; New Budget with title, type (season, episode, episode count), dates, schedule length as globals, location, base currency, start from (starter, empty, bundled charts, a budget file), fringe posting and arithmetic, in a new tab as one undo step. Phase 2 (decision 65): the start window when the last tab closes, a chart-of-accounts step listing every bundled chart by budget type with its description and account count, Save In (a folder and file name, never over a file, opened as a saved tab), pinned and recent following files moved in the Finder (macOS bookmarks), net totals kept between runs. Setup step (decision 77): everything in the setup library and any setup packs ticked in the form come in before the form's details; built on the recommendation that what is ticked replaces the starting point's same-named items (the owner can reverse it), and two ticked sources that disagree are refused. The rest (decision 91): projects as folders down the start window's left (Add Folder…, or New Budget's "Make a project folder", built on the recommendation below), each read two levels deep with its budgets, the sub-budgets and copies made from them, setup packs, schedule days files and actuals exports; a thumbnail per budget (a bar per category from the topsheet's totals, kept in the app's data folder, never in the file); Duplicate (beside the file, never over one, opened as a saved tab); New Sub-Budget from This Category or Account (the row's accounts, lines and only the setup they use, totals the same as the row, beside the parent's file); Link a Schedule… at New Budget (shoot days to `SHOOT_DAYS` linked as Import Schedule Days links it, the dates to budget info); shared budgets marked with who was in them lately; sorting by last opened, name, last changed or net total. Rates from 3.9 when it lands; created dates and archived projects (design phase 4) not built. Needs a live check in the app | getting started fast | [have] | M | 1.20, 3.9, 3.16 |
| 3.20 | **The sample budget**: Field Station, an invented feature of about a million dollars made from Method Schedule's schedule of the same production, showing the whole feature set, opened as an untitled copy marked Sample at the first start and from Help > Open Sample (SUITE-STANDARDS.md, section 8). Built (decision 84; docs/APP.md, "The sample budget"): the budget from Method Schedule's days file through `schedule-days --apply` (scripts/make-field-station.py, checked byte for byte in CI); the first-start rule and the copy in method-session's `sample`; Help > Open Sample; shown once at the first start, with no setting (decision 86, per the suite standard); the browser demo and its smoke test on the same file; the pictures retaken from it. Left: a live check in the app; copying the days file again when Method Schedule's PR #29 merges | something real to look at from the first minute | [have] | M | 3.8, 3.19 |
| 3.21 | **Method Launcher contract**: the suite's launcher can start the app and read its recent files (SUITE-STANDARDS.md, section 2). Built (decision 85; docs/APP.md, "Command-line options"): `method-budget-app --check-for-updates` (as Help > Check for Updates…) and `--open-sample` (as Help > Open Sample, in place of the start window), read at the start, other options skipped; `recent.json` as the suite's array, newest first, `path`, `opened` (RFC 3339, UTC) and our optional `bookmark`, the old shape still read and rewritten. Left: a live check of the flags in the app | one launcher for the whole suite | [have] | S | 3.20 |
| 3.22 | **Formulas walked without recursion**: a formula may hold 2,048 terms (`MAX_FORMULA_LEN`), and `a + b + c ...` parses into a tree as deep. Built: every walk over a formula keeps its own stack on the heap instead of recursing (crates/method-core/src/expr.rs): evaluating in decimals and in doubles, the names it uses, renaming and substituting, printing, and hand-written clone, compare, debug print and drop (drop takes each node's children out into a list first); the parser reads nesting with a stack too. Same parse, values, errors (the leftmost failing part, as before) and printed text, checked against the old recursive code kept in the tests as a reference; no format change. A test runs every walk over the deepest formulas on a 64 KiB thread, and method-core's debug tests now pass with 64 KiB test threads (they needed more than 1 MiB), so the release test step no longer sets 8 MiB | found by the release rehearsal | [have] | M | 3.21 |

Skipped on purpose: accounts and sign-in, cloud sync, a send-to-user inbox, live currency
rates (no network), vendor template or rate data, other products' native file formats.

---

## Engine checks to run

Semantic questions only a licensed copy can settle. Most are covered by the export test
(ETP); the quick test is for settling one on its own in a few minutes. The parity runs of
2026-10-10 to 2026-10-12 answered most of them (docs/research/parity-tracker.md); still
open: 14 in part and 16.

| # | Question | Our rule today | Quick test | In ETP |
|---|---|---|---|---|
| 1 | Is a percent cutoff per fringe range? | **Answered (D56, ZTEST04)**: per earner, a line or a range, as ours; ours adds an opt-in person across accounts (3.11, D79), which the reference doesn't have, so there is nothing to check | Three lines of 1,000 each, a 12.5% fringe with a 2,000 cutoff. Fringe total with them bracketed into a range: ours says 250. Unbracketed: 375 | yes (7201) |
| 2 | Do detail subtotal and total rows drop suppressed lines? | **Answered (ZTEST07, D57)**: they drop out, as ours | Subtotal row under two lines, suppress one line's group, read the subtotal row and the account total row | no |
| 3 | Does toggling a group off and on drift the total? | **Answered (V07-1b)**: no drift, as ours | In a real budget note the net total, suppress a group, restore it, read the net total and "last change" again | no |
| 4 | Currency direction | **Answered (CU-01)**: line x rate = base, as ours | One line of 100 with currency EUR at rate 3. Total 300 means ours; 33.33 means the other way | yes (7301, V3) |
| 5 | What does the percentage cutoff factor do? | **Answered (D60)**: it multiplies every percent cutoff; built as `cutoff_factor` | Export test variation V2 (factor 2.0) and read the driver line's fringe against the table | yes (V2) |
| 6 | Flat fringes: once per line or per range; do they multiply on non-time units? | **Answered 2026-10-11 (D54)**: per line; the Amount only, converted through unit hours; X and 4X never count; once when either unit has no hours. Built, with an optional basis (amount plus counts, or the full quantity) and most hours a day | Export test grip and driver kit lines | yes |
| 7 | Adjustment rows: can they be fringed, do they include fringes above, do they restart at a subtotal? | **Answered (ZTEST07, D56)**: they take the lines above without their fringes and don't restart at a subtotal (550.00 is 10% of 5,500.00 across one); an adjustment row can carry fringes (built) | Export test account 7102 plus screenshot 19 | yes |
| 8 | Charge base and stacking | **Answered (D56, V09-2)**: the base is everything before charges; charges never stack | Export test ZPCTCHG and ZFLATCHG, and V1 for post-by-budget | yes |
| 9 | Line with one "out" group and one "in" group | **Answered (D57, GR-03)**: out by default; the conflict rule is a budget setting | Export test Option B lens; or tag one line with both and read its total | yes |
| 10 | Do suppressed lines inside a range still count toward the cutoff? | **Answered (D56)**: no, as ours | Bracket three lines that cross a cutoff, suppress the middle one, read the fringe on the other two | no |
| 11 | Where does rounding happen? | **Answered (D55)**: once per line in the reference's arithmetic, now our default | Export test Polish line (1,296.315): 1,296.32 or 1,296.31 | yes |
| 12 | Group "total" column after suppression | **Answered 2026-10-11 (D58)**: base of every tagged line plus only the fringes earned (ZOFF 5,000.00 total, 0.00 active). Built | Open the groups setup in 2026 and read total vs active total for a suppressed group | yes |
| 13 | Does a global's unit do anything? | **Answered (D60)**: it fills an empty Unit when typed into Amt; built | Give a global the unit "week", type it into a line's Amt, see whether the line's unit fills in | no |
| 14 | 2026 new-line and preference defaults | **Partly answered (D60)**: new-budget settings match; Amt starting at 1 and one ready line not compared yet | Screenshot budget preferences in a fresh blank budget (ETP step 1 does this) | yes |
| 15 | Fringe percent shown on a suppressed line | **Answered (D61, R2-05)**: 0.00, as ours earns 0; the blank cell is an open display call | Turn on the Agg column, suppress a fringed line, read its Agg | no |
| 16 | Next account at the end of a category | wraps across categories | On the last account of a category, press the next-account step | no |

---

## Recording wishlist

Short clips (1 to 3 minutes, the Z test budget is fine, no real figures needed) that fill
gaps the 2020/2021 tutorials leave or that need 2026 confirmation. In priority order.

1. **Two minutes of real keystrokes** with an on-screen key display: moving, inserting,
   setup keys, ranges, notes. Confirms the keys built for decision 2 and item 1.7.
2. **Reports in 2026**: open report setup, pick sections and accounts, change orientation,
   add a page number, export a PDF. Shapes 1.15 and 2.3 to 2.6.
3. **Compare in 2026**: link one budget, then two, and hover or read the variance and
   percent columns on a row whose numbers are easy. Confirms the columns built for
   decision 1.
4. **Versions and history**: make a version, look at the history list, revert a step, branch
   one into a version. Shapes 1.12 and 2.23.
5. **Copy rows between two budgets** where the target has a same-named fringe with a
   different rate and lacks a group the rows use. Checks the paste review built for
   decision 12 and fills note 20's gaps.
6. **Find and replace a global and a fringe** in 2026, including the remove action and what
   a cell holds afterwards. Shapes 1.11.
7. **Suppression detail**: suppress a group with a subtotal row under the lines, then open
   the groups setup to show total and active total. Engine checks 2 and 12 are answered;
   a clip would still confirm them on screen.
8. **Saving and opening**: the autosave switch, closing with unsaved work, saving a local
   file, opening a recent budget. Shapes 1.1 and decision 9.
