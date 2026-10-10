# Behavior note 19: adding, showing and editing notes

Source: vendor tutorial video "Adding Notes" (published around Mar 2020 to Apr 2021;
vendor tutorial, 2020/2021 era), about 1 minute, Windows. Clean-room: behavior in our own
words; no copied text or images. The video stays outside the repo.

## Creating a note

- Notes attach to a detail line or an account. The narration pitches them for flagging
  items to review and for keeping supporting information.
- Right-click a line, then "add note", or press Ctrl+Shift+Space (Cmd+Shift+Space on Mac).
- The tools pane switches to its Notes tab. It shows the account (code and name), then the
  line (line number and description), with an empty text box, cancel, save note, and a
  new-note button.
- Saving turns the note into a yellow card stamped with the author and the date and time.
  Each card has edit, delete and reply actions. Reply suggests threaded comments.
- More than one note can sit on the same line: "new note" adds another card under it.

## Showing notes

- A line with a note gets a small note icon in the grid (it appeared at the right end of
  the description cell in the video; the narration places it near the total).
- Clicking the icon opens the Notes tab on that line's notes for a quick read.
- The Notes tab lists notes grouped by account and line, with expand/collapse controls.

## Printing

- Not shown in this video. Note 15 shows a "notes report" as an optional section of a
  printed report.

## Shortcuts seen

- Add note: Ctrl+Shift+Space (Windows), Cmd+Shift+Space (Mac). Same as Sam's 2026
  recording (note 01).
- The Windows right-click menu in this era also showed: cut, copy and paste row, insert row
  Ctrl+I, delete row on Delete, insert subtotal adjustment, insert subtotal, include in an
  existing credit, exclude from an existing charge, add to an x-linked group, make a fringe
  range Ctrl+R, remove a fringe range Ctrl+Shift+R.

## Era caution

- 2020 Windows build. The author stamp implies user identity from the vendor's account
  system; our app has no accounts, so "author" would come from a local name setting or be
  left out. Note 01 confirms add-note and the Notes tab still exist in 2026.

## What this means for Method Budget

1. [partial] P1 **Note entry**: we store one plain-text note per detail row and show a
   corner mark with the note as a tooltip. Add a visible editor (side panel or popover)
   reached by Cmd/Ctrl+Shift+Space and the right-click menu.
2. [missing] P2 **Several dated notes per row**, each with a timestamp and an optional
   author name from a local setting; edit and delete each. Needs a format change (list of
   notes instead of one string), additive in version 1.
3. [missing] P2 **Notes on accounts** (and categories), not just detail rows.
4. [missing] P2 **Notes list**: every note in the budget grouped by account and line, with
   jump-to. Doubles as a review checklist.
5. [missing] P2 **Notes report** as a printable section (see note 15).
6. [missing] P3 **Replies (threads)**. Low value for a local, single-user app; skip unless
   shared files become common.
7. [partial] P3 **Click the note mark to open the note** (today it is hover only).
