# Behavior note 15: report setup, templates, sections and print options

Source: vendor tutorial video "Create and Manage Reports" (published around Mar 2020 to
Apr 2021; vendor tutorial, 2020/2021 era), about 3.5 minutes, macOS. Clean-room: behavior in
our own words from the narration and frames; no report layouts, help text or images copied.
The video stays outside the repo.

## Getting there

- A reports menu in the top bar has a "report setup" entry, a quick-reports section, and a
  budget comparison entry (greyed out when there is nothing to compare).
- Report setup opens a separate window titled for the budget. It shows a live, paginated
  preview of the printed report, not a dialog of checkboxes.

## Templates

- Reports are built from templates stored with the budget. A dropdown at the top left lists
  the templates for this budget and has a "new report template" entry. A new one starts as
  an untitled template; clicking its name makes it editable in place (renamed in the video).
- Each template keeps its own column choices, widths, layout options, header/footer and
  included sections. Templates are re-rendered from current budget data every time, so a
  saved template never goes stale.
- A delete-template action sits beside the layout options.
- The narration stresses the point of templates: report layout is separate from the
  editing grid. Hiding columns or resizing them in a report never touches the budget view.

## Columns on the preview

- Each table in the preview has a kebab menu in its header row listing optional columns.
  On the topsheet these were: a page number column (which page of the printout each
  category's detail starts on), fringes, original (baseline) and variance. Toggling one
  re-renders the preview immediately.
- Column widths are dragged directly on the preview header and stick to the template.

## Layout options (toolbar above the preview)

- Vertical lines between columns: on or off.
- Font size (8 pt was the default shown).
- Orientation: portrait or landscape.
- Report shading: on or off (alternating row tint and darker tint on total rows).
- Group color: a choice that starts at black and white; the other option prints lines in
  their group colors.

## Header and footer editor

- A "Header" button on the preview opens a header/footer editor with Header and Footer tabs.
- Three text zones: one full-width zone on top and a left and right zone under it.
- A row of draggable tags drops live fields into any zone: copyright, page number, today's
  date, current time, budget file name, budget creation date and last-modified date. Plain
  text can be typed around the tags.
- A scope dropdown (first page vs. later pages) and a "header on first page only" switch.
- A left rail lists saved header/footer templates ("my templates", with a default one) and
  can add more, so one header design can be reused across report templates. "Apply to
  report" closes the editor and the preview shows the result.

## Included sections

- A two-pane picker. Left: available sections as a checklist with a search box. Right:
  sections in the report, in print order, reorderable by drag.
- Available sections: every category (expandable to its accounts) with a checkbox each; a
  dropdown on the account list that controls suppressed lines (set to exclude them); the
  topsheet; category and detail; and additional reports: chart of accounts, budget
  information, notes report, and setup screen reports (fringes, globals, groups,
  locations, sets, currencies, units), each its own checkbox.
- Setup reports print the setup tables as tables: fringes with description, ID, rate,
  unit, cutoff, in-use count and total; globals with in-use count, a flag column,
  name, description, calculation, units, decimals and value.

## Navigating the preview

- A section dropdown jumps straight to a section of the report (topsheet, fringes,
  globals, and so on).
- Page navigation: first, previous, numbered pages with an ellipsis, last, next. Scrolling
  also works. Page count updates as sections are added.
- Footer buttons: close, export PDF, print (straight to the printer).

## Shortcuts seen

- None for reports in this video.

## Era caution

- This is the 2020/2021 report window. The quick-reports list was empty in the video, so
  what it holds now is unknown. The 2026 notes have not covered reports yet; check the
  current product before matching any specific option.

## What this means for Method Budget

1. [partial] P1 **Report templates saved in the budget**: we print topsheet, detail or
   both with a fixed layout. Add named templates (columns, options, sections, header and
   footer) stored in the `.mthd`, re-rendered from live data.
2. [missing] P1 **Section picker**: choose categories and accounts to include, toggle
   suppressed lines, reorder sections, plus extra reports.
3. [missing] P1 **Export to PDF** as a direct action (today we rely on the system print
   dialog). Check whether the print dialog's save-as-PDF is enough on all three platforms.
4. [missing] P2 **Layout options**: orientation, font size, vertical rules, shading on or
   off, group colors on or off.
5. [missing] P2 **Header/footer with live fields**: page number, dates, time, file name,
   created and modified dates, first-page-only switch. We print a running header today.
6. [partial] P2 **Optional report columns**: original and variance print already; add
   fringe and a "detail starts on page N" column on the topsheet.
7. [missing] P2 **Setup and notes reports**: fringes, globals, groups, units, currencies,
   sets, locations, budget info, chart of accounts, notes.
8. [missing] P3 **Live preview with page navigation and a section jump list**, so layout
   changes are seen before printing.
9. [missing] P3 **Reusable header/footer designs** across templates.
