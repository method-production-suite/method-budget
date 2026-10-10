//! Release notes: `docs/releases/<version>.md`, in the shape
//! `docs/release-notes-template.md` describes, read into plain data.
//!
//! ```text
//! # Method Budget 0.2.0
//!
//! One headline paragraph (it may wrap over several lines).
//!
//! ## A heading named for what changed for the reader
//!
//! - **A claim in the reader's words.** The explanation, which may wrap
//!   onto indented lines.
//! - A plain bullet, with no bold claim.
//!
//! ## Good to know
//!
//! - A caveat.
//! ```
//!
//! The app has no markdown library and shows the notes with `textContent`,
//! so the inline markup the template uses is reduced to its words here:
//! `**bold**` and `` `code` `` keep their text, a link keeps its text and
//! drops its address, and an HTML comment disappears. Nothing in the result
//! is markup. A line this reader doesn't understand is an error, never
//! dropped silently: words a reader was meant to see must not vanish.
//!
//! [`check_for_release`] adds the rules a release must meet on top of
//! parsing: the version in the title, no em dashes, no leftover template
//! placeholders, a sane size.

/// A release's notes, as plain text pieces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notes {
    /// The `# ` title, as written (`Method Budget 0.2.0`).
    pub title: String,
    /// The paragraph before the first section.
    pub headline: String,
    pub sections: Vec<Section>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub items: Vec<Item>,
}

/// One bullet: an optional bold claim and the rest of the sentence(s).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// The bold claim at the start of the bullet, or empty.
    pub lead: String,
    pub body: String,
}

/// Where template placeholders start; [`check_for_release`] refuses a file
/// that still has one.
pub const PLACEHOLDER: &str = "{{";
/// The largest notes file a release accepts (a GitHub release body tops out
/// near 125,000 characters; this leaves room for what the workflow adds).
pub const MAX_BYTES: usize = 60_000;

const EM_DASH: char = '\u{2014}';

/// Removes HTML comments (`<!-- ... -->`), including ones spanning lines.
fn strip_comments(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        let end = after
            .find("-->")
            .ok_or("an HTML comment (<!--) is never closed")?;
        rest = &after[end + 3..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The words of one line of inline markup: links keep their text, bold and
/// code marks go, runs of spaces become one.
pub fn plain(value: &str) -> String {
    let mut s = String::with_capacity(value.len());
    let mut rest = value;
    // [text](address) -> text
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let link = after.find("](").and_then(|mid| {
            after[mid + 2..]
                .find(')')
                .map(|close| (mid, mid + 2 + close))
        });
        match link {
            Some((mid, close)) if !after[..mid].contains('[') => {
                s.push_str(&rest[..open]);
                s.push_str(&after[..mid]);
                rest = &after[close + 1..];
            }
            _ => {
                s.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    s.push_str(rest);
    let s = s.replace("**", "").replace('`', "");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Splits `**Claim.** Rest` into its claim and the rest.
fn split_lead(bullet: &str) -> (String, String) {
    if let Some(after) = bullet.strip_prefix("**") {
        if let Some(end) = after.find("**") {
            return (plain(&after[..end]), after[end + 2..].trim().to_string());
        }
    }
    (String::new(), bullet.to_string())
}

/// Reads a notes file. Errors name the line and what is wrong with it.
pub fn parse(markdown: &str) -> Result<Notes, String> {
    let text = strip_comments(&markdown.replace("\r\n", "\n"))?;
    let mut title: Option<String> = None;
    let mut headline: Vec<String> = Vec::new();
    let mut headline_done = false;
    let mut sections: Vec<Section> = Vec::new();
    // The bullet being read: its first line and its continuations.
    let mut open: Option<Vec<String>> = None;

    fn close(open: &mut Option<Vec<String>>, sections: &mut [Section]) {
        if let (Some(lines), Some(section)) = (open.take(), sections.last_mut()) {
            let (lead, body) = split_lead(&lines.join(" "));
            section.items.push(Item {
                lead,
                body: plain(&body),
            });
        }
    }

    for (n, raw) in text.lines().enumerate() {
        let line_no = n + 1;
        let line = raw.trim_end();
        if line.trim().is_empty() {
            close(&mut open, &mut sections);
            if !headline.is_empty() {
                headline_done = true;
            }
            continue;
        }
        if let Some(t) = line.strip_prefix("# ") {
            if title.is_some() || !sections.is_empty() || !headline.is_empty() {
                return Err(format!("line {line_no}: a second \"# \" title"));
            }
            title = Some(plain(t));
            continue;
        }
        if let Some(t) = line.strip_prefix("## ") {
            close(&mut open, &mut sections);
            if title.is_none() {
                return Err(format!("line {line_no}: a section before the \"# \" title"));
            }
            sections.push(Section {
                title: plain(t),
                items: Vec::new(),
            });
            headline_done = true;
            continue;
        }
        if line.starts_with('#') {
            return Err(format!(
                "line {line_no}: only \"# \" (the title) and \"## \" (sections) headings are understood"
            ));
        }
        if let Some(b) = line.strip_prefix("- ") {
            if sections.is_empty() {
                return Err(format!(
                    "line {line_no}: a bullet before the first \"## \" section"
                ));
            }
            close(&mut open, &mut sections);
            open = Some(vec![b.trim().to_string()]);
            continue;
        }
        if raw.starts_with(' ') || raw.starts_with('\t') {
            if let Some(lines) = open.as_mut() {
                lines.push(line.trim().to_string());
                continue;
            }
        }
        if sections.is_empty() {
            if title.is_none() {
                return Err(format!("line {line_no}: text before the \"# \" title"));
            }
            if headline_done {
                return Err(format!(
                    "line {line_no}: a second paragraph before the first \"## \" section (the headline is one paragraph)"
                ));
            }
            headline.push(line.trim().to_string());
            continue;
        }
        let section = &sections.last().map(|s| s.title.clone()).unwrap_or_default();
        return Err(format!(
            "line {line_no}: a line under \"{section}\" that is neither a bullet (\"- \") nor an indented continuation of one"
        ));
    }
    close(&mut open, &mut sections);

    let title = title.ok_or("no \"# \" title")?;
    if headline.is_empty() {
        return Err("no headline paragraph after the title".into());
    }
    if sections.is_empty() {
        return Err("no \"## \" sections".into());
    }
    for s in &sections {
        if s.items.is_empty() {
            return Err(format!("the section \"{}\" has no bullets", s.title));
        }
    }
    Ok(Notes {
        title,
        headline: plain(&headline.join(" ")),
        sections,
    })
}

/// The notes as plain text: the headline, then each section's title and
/// bullets. What the update manifest carries.
pub fn to_plain_text(notes: &Notes) -> String {
    let mut out = notes.headline.clone();
    for s in &notes.sections {
        out.push_str("\n\n");
        out.push_str(&s.title);
        for item in &s.items {
            out.push_str("\n- ");
            if !item.lead.is_empty() {
                out.push_str(&item.lead);
                if !item.body.is_empty() {
                    out.push(' ');
                }
            }
            out.push_str(&item.body);
        }
    }
    out
}

/// What a release's notes must also be, beyond readable: titled with
/// `version`, free of em dashes and template placeholders, and not too big.
/// Every problem is reported, not just the first.
pub fn check_for_release(version: &str, markdown: &str) -> Result<Notes, Vec<String>> {
    let mut problems = Vec::new();
    if markdown.len() > MAX_BYTES {
        problems.push(format!(
            "it is {} bytes; release notes must stay under {MAX_BYTES}",
            markdown.len()
        ));
    }
    for (n, line) in markdown.lines().enumerate() {
        if line.contains(EM_DASH) {
            problems.push(format!(
                "line {}: an em dash (use a colon, a comma or a period)",
                n + 1
            ));
        }
        if line.contains(PLACEHOLDER) {
            problems.push(format!(
                "line {}: a template placeholder ({PLACEHOLDER} ... }}}}) is still there",
                n + 1
            ));
        }
    }
    let notes = match parse(markdown) {
        Ok(n) => Some(n),
        Err(e) => {
            problems.push(e);
            None
        }
    };
    if let Some(n) = &notes {
        let last = n.title.rsplit(' ').next().unwrap_or("");
        if last != version {
            problems.push(format!(
                "the title \"{}\" should end with the version, {version}",
                n.title
            ));
        }
    }
    match notes {
        Some(n) if problems.is_empty() => Ok(n),
        _ => Err(problems),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "# Method Budget 0.2.0

Updates now arrive inside the app, and the totals strip
reads better.

## Updates come to you

- **The app tells you when a new version is ready.** It downloads in the
  background and asks for nothing until you choose Restart to Update.
- **What's New after each update.** Help > Release Notes shows it again.

## Good to know

- A restart is still needed: your budgets and tabs come back as they were.
<!-- a note to whoever writes the next release, never shown -->
";

    #[test]
    fn reads_the_template_shape() {
        let n = parse(GOOD).unwrap();
        assert_eq!(n.title, "Method Budget 0.2.0");
        assert_eq!(
            n.headline,
            "Updates now arrive inside the app, and the totals strip reads better."
        );
        assert_eq!(n.sections.len(), 2);
        assert_eq!(n.sections[0].title, "Updates come to you");
        assert_eq!(
            n.sections[0].items[0].lead,
            "The app tells you when a new version is ready."
        );
        assert_eq!(
            n.sections[0].items[0].body,
            "It downloads in the background and asks for nothing until you choose Restart to Update."
        );
        assert_eq!(n.sections[1].items[0].lead, "");
        assert!(n.sections[1].items[0]
            .body
            .starts_with("A restart is still needed"));
        assert!(check_for_release("0.2.0", GOOD).is_ok());
    }

    #[test]
    fn inline_markup_becomes_words() {
        assert_eq!(
            plain("See [the guide](https://example.com/x) now"),
            "See the guide now"
        );
        assert_eq!(
            plain("Run `method check` **today**"),
            "Run method check today"
        );
        assert_eq!(plain("a [bracket] stays"), "a [bracket] stays");
        assert_eq!(
            plain("<b>tags</b> stay as text"),
            "<b>tags</b> stay as text"
        );
    }

    #[test]
    fn refuses_what_it_cannot_show() {
        let cases = [
            ("no title\n\n## S\n\n- x\n", "before the"),
            ("# T 1\n\n## S\n\n- x\n", "no headline"),
            ("# T 1\n\nHead.\n", "no \"## \" sections"),
            ("# T 1\n\nHead.\n\n## Empty\n", "no bullets"),
            (
                "# T 1\n\nHead.\n\nSecond.\n\n## S\n\n- x\n",
                "second paragraph",
            ),
            ("# T 1\n\nHead.\n\n## S\n\nloose line\n", "neither a bullet"),
            ("# T 1\n\nHead.\n\n### Deep\n\n- x\n", "only \"# \""),
            ("# T 1\n\nHead.\n\n## S\n\n- x\n<!-- open\n", "never closed"),
            ("# T 1\n# T 2\n\nHead.\n\n## S\n\n- x\n", "second \"# \""),
        ];
        for (text, needle) in cases {
            let e = parse(text).unwrap_err();
            assert!(e.contains(needle), "{text:?}: {e}");
        }
    }

    #[test]
    fn a_release_needs_its_version_and_no_leftovers() {
        let e = check_for_release("0.3.0", GOOD).unwrap_err();
        assert!(e[0].contains("should end with the version, 0.3.0"), "{e:?}");
        let dashed = GOOD.replace("background and", "background \u{2014} and");
        let e = check_for_release("0.2.0", &dashed).unwrap_err();
        assert!(e.iter().any(|p| p.contains("em dash")), "{e:?}");
        let draft = GOOD.replace("reads better.", "{{One sentence: the theme.}}");
        let e = check_for_release("0.2.0", &draft).unwrap_err();
        assert!(e.iter().any(|p| p.contains("placeholder")), "{e:?}");
        // A draft still parses: the app can show it in a development build.
        assert!(parse(&draft).is_ok());
    }

    #[test]
    fn plain_text_for_the_manifest() {
        let text = to_plain_text(&parse(GOOD).unwrap());
        assert!(text.starts_with("Updates now arrive inside the app"));
        assert!(text.contains("\n\nUpdates come to you\n- The app tells you"));
        assert!(text.contains("\n- A restart is still needed"));
        assert!(!text.contains("**"));
        assert!(!text.contains("never shown"));
    }
}
