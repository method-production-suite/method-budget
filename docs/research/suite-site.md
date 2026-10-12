# The Method Production suite and its websites

Status: plan, 2026-10-12. A brief for whoever builds the suite site (likely a separate
repository and session). Nothing here is built yet.

## The shape

Three sites, one family:

| Site | Domain | What it is |
|---|---|---|
| Method Production | methodproduction.com | The suite's home: one page introducing the apps and how they connect, linking out to each |
| Method Budget | methodbudget.com | This repository's `site/`: the full product site for the budgeting app |
| Method Schedule | its own domain | The full product site for the scheduling app |

Each app keeps its own site, repository, releases and download links. The suite site sells
the pair and the hand-off between them; it does not duplicate either product page.

## The suite home page

1. **Hero.** The suite's name, one line on what it is (scheduling and budgeting for film
   and TV that run on your own computer and talk to each other), and two cards, one per
   app, each with its accent, a one-line pitch, a screenshot and a link to its site.
2. **How they connect.** The schedule feeds the budget: shoot days, prep and wrap weeks,
   cast and location days arrive as the budget's globals (`SHOOT_DAYS`, `PREP_WEEKS`,
   `WRAP_WEEKS` are already what the budget's New Budget form writes). Show it as a small
   diagram, not paragraphs. Only claim what is built; mark the rest as coming.
3. **What they share.** Local files you own, no account or cloud required, open file
   formats, a command line, working together through a shared folder.
4. **Download strip.** One row per app, each linking to that app's latest release.
5. **Footer.** Links to each app's site and repository. No company or personal names.

## The strip on each product site

Every product site gets a slim bar above its own header:

```
Part of Method Production   ·   Method Schedule   ·   Method Budget (current, not a link)
```

Plain text links, muted, one line, collapsing to "Part of Method Production" on a phone.
For this site it goes at the top of `site/index.html`, `site/releases/index.html` and
`site/404.html`, styled in `site/assets/site.css`. Add it only once methodproduction.com
is live, so it never links to a parked page.

## One look

- **Shared tokens.** Today the colors, fonts and shadows live in `site/assets/tokens.css`
  (explained in docs/BRAND.md). Lift the neutral parts (paper, ink, rules, type scale,
  radii, shadow) into one shared tokens file that every site copies in at the same
  version; each site then sets only its accent.
- **Accents.** Budget keeps its teal (`--accent: #0E7468`, the app's own). Schedule picks
  its own accent from the same lightness and contrast checks (docs/BRAND.md lists them).
  The suite site uses the neutral ink as its accent and shows each app's color only on
  that app's card.
- **Marks.** Each app has its mark and wordmark (`site/assets/brand/` here). The suite
  gets a wordmark in the same type, no new symbol needed at first.
- **Same structure, same rules:** a static site, no build step or a tiny Python generator
  like `scripts/site.py`, GitHub Pages, no trackers, no cookies, no external scripts, fonts
  and images served from the site itself, light and dark mode, readable at phone width.

## Rules that carry over

- No company, studio or personal names anywhere public: not in pages, alt text, file
  names, metadata or commit authors of the site repository.
- Never name the incumbent products. Say "the industry-standard tools" if a comparison is
  needed at all.
- Screenshots use invented productions only (`scripts/make-showcase-budgets.py` makes the
  budget ones).
- Each claim must be backed by what is built; anything planned says so.

## Order of work

1. Point methodproduction.com at GitHub Pages (same records as methodbudget.com: four A,
   four AAAA, `www` CNAME, the account-level TXT verification).
2. Build the suite page in its own repository, with the shared tokens file.
3. Add the strip to this site and to the schedule's site, pointing at the live suite.
4. Later: one combined download page, and a "works with" section on each product site.
