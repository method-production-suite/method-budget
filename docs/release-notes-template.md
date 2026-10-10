# Writing release notes

Every version has notes in `docs/releases/<version>.md`. They are shown in three places,
so they are written once, for the people who budget films:

- **In the app.** After an update, Method Budget shows "What's New in <version>" once,
  and Help > Release Notes shows it again. The app reads the notes as plain text (no
  markup is drawn, links show their words only).
- **On GitHub.** The release workflow uses the file as the release's description.
- **On the website.** The newest notes fill the "What's new" box (`scripts/site.py`).

`cargo xtask bump` starts a new file from the skeleton at the end of this page.
`cargo xtask notes-check` says whether it is ready to publish. Release steps:
[RELEASING.md](RELEASING.md).

## Voice

- Write for someone budgeting a film, not for someone reading the code. Say what they can
  do now, or what works better, in their words: "Restart to update keeps your tabs", not
  "persisted session state across relaunch".
- Plain sentences. No em dashes (a colon, a comma or a period instead). No exclamation
  marks. No internal names: no crate, module, function or file names, no commit hashes,
  no roadmap numbers.
- Examples use invented budgets only. Never a real production, a real person or a real
  figure: the notes become a public web page.
- Claim only what is built. If `docs/features.toml` or `docs/ROADMAP.md` doesn't back a
  claim, leave it out.
- Most commits earn no bullet. Refactors, tests, tooling and documentation don't, unless
  a user would notice.

## Shape

The reader in the app understands exactly this shape (anything else is refused by
`cargo xtask notes-check`, so nothing a reader was meant to see is dropped):

1. `# Method Budget <version>`, the title. It ends with the version.
2. One headline paragraph: one or two sentences on what this release means.
3. Sections, each `## ` and a heading named for what changed for the reader ("Updates
   come to you", not "Updater"). Common closing sections: `## Fixed` and
   `## Good to know` (caveats, changed defaults, anything to do after updating).
4. In each section, bullets. A bullet may start with a bold claim, `**Like this.**`,
   followed by a sentence or two. A long bullet wraps onto indented lines.

Inline `code` and `**bold**` are allowed; they show as plain words in the app. Links show
their text only. HTML comments are never shown anywhere, so they are fine for notes to
the next writer.

## Length

A minor release: a headline, two to four sections, up to about a dozen bullets. A patch
release: a headline and a `## Fixed` section is often all it needs. The whole file stays
well under 60,000 characters (a GitHub release has a limit).

## The skeleton

`cargo xtask bump` copies the block below into the new file, with the version filled in.
Every `{{...}}` is a placeholder: `notes-check` refuses the file until none is left.

<!-- skeleton:start -->
```markdown
# Method Budget {{version}}

{{One or two sentences: what this release means for someone budgeting a film.}}

## {{A heading named for what changed for the reader}}

- **{{The claim, in the reader's words.}}** {{What it means, and where to find it.}}

## Fixed

- **{{What works now.}}** {{What used to go wrong, in a few words.}}

## Good to know

- {{A caveat, a changed default, or something to do after updating. Delete this section if there is none.}}
```
<!-- skeleton:end -->
