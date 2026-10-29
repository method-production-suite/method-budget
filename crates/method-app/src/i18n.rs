//! The app's words, from the page's tables (docs/APP.md, "Languages";
//! decisions.md, entry 94).
//!
//! Each language is a table in `ui/i18n/<code>.js`: one object of key to
//! words, strict JSON after `export default`, so the page imports it as a
//! module (`ui/js/i18n.js`, under the page's script-src 'self') and Rust
//! reads the same file here, built in with `include_str!`. The menu bar
//! (src/menu.rs) takes its words from it.
//!
//! A value is a string, or for a count an object of plural forms (`one`,
//! `other`...). `{name}` in the words is filled in by [`tr_with`]. Only
//! English exists, so Rust always uses it; the page picks the system's
//! language from its tables, with English for anything missing.
//!
//! Words that arrive from the engine as data (an edit refused, a number
//! that doesn't read) go through [`engine`]: method-session makes each from
//! a stable code and its parameters (`method_session::messages`), and the
//! table's entry for the code is the same English, so the person sees it in
//! their language, or as the engine wrote it when the table has no entry.
//!
//! Undo labels go through [`label`] the same way. A label is stored (in the
//! history, in a shared budget's change log, read by other computers and
//! older versions), so it stays the engine's English; only what this
//! computer shows is translated: the History tab, the Activity tab, a
//! field's past values, the changes that just arrived and the changes a
//! restarted shared budget left out.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

/// The English table, the fallback for every key.
pub const EN: &str = include_str!("../ui/i18n/en.js");

/// The plural forms a value may have (Unicode's plural categories).
pub const PLURAL_FORMS: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// A table's entries, from its file's text: the JSON object after the line
/// that starts `export default`, with an optional `;` after it.
pub fn parse_table(src: &str) -> Result<BTreeMap<String, Value>, String> {
    const START: &str = "export default";
    let at = if src.starts_with(START) {
        START.len()
    } else {
        src.find(&format!("\n{START}"))
            .ok_or_else(|| format!("no line starting `{START}` in the table"))?
            + START.len()
            + 1
    };
    let body = src[at..].trim();
    let body = body.strip_suffix(';').unwrap_or(body).trim_end();
    serde_json::from_str(body).map_err(|e| format!("the table isn't a JSON object: {e}"))
}

fn english() -> &'static BTreeMap<String, Value> {
    static TABLE: OnceLock<BTreeMap<String, Value>> = OnceLock::new();
    TABLE.get_or_init(|| parse_table(EN).expect("ui/i18n/en.js is a JSON object (tested)"))
}

/// The words for `key`: for a plural entry, the `one` form when `count` is
/// 1 and the `other` form otherwise (English's rule). `None` when the table
/// has no such entry.
fn lookup(key: &str, count: Option<&str>) -> Option<String> {
    match english().get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Object(forms) => {
            let form = if count == Some("1") && forms.contains_key("one") {
                "one"
            } else {
                "other"
            };
            forms.get(form).and_then(Value::as_str).map(str::to_string)
        }
        _ => None,
    }
}

/// The words for `key` (the `other` form of a plural entry). A missing key
/// gives the key itself; the tests keep that from happening.
pub fn tr(key: &str) -> String {
    lookup(key, None).unwrap_or_else(|| {
        debug_assert!(false, "i18n: {key} is not in ui/i18n/en.js");
        key.to_string()
    })
}

/// [`tr`] with `{name}` filled in from `params`, in one pass (a value
/// holding `{other}` is left as it is).
pub fn tr_with(key: &str, params: &[(&str, &str)]) -> String {
    fill(&tr(key), params)
}

/// The words for `key`, a count's entry, in the form for `count`, with
/// `{count}` and `params` filled in.
pub fn tr_count(key: &str, count: usize, params: &[(&str, &str)]) -> String {
    let n = count.to_string();
    let words = lookup(key, Some(&n)).unwrap_or_else(|| {
        debug_assert!(false, "i18n: {key} is not in ui/i18n/en.js");
        key.to_string()
    });
    let mut all = vec![("count", n.as_str())];
    all.extend_from_slice(params);
    fill(&words, &all)
}

/// An engine message in the person's language: when `text` is one of the
/// engine's messages (`method_session::messages::recognize`), the table's
/// words for its code with its parameters filled in, a noun, reason or
/// count phrase inside it translated the same way; otherwise, or when the
/// table has no entry for the code, `text` as it is. A summary of several
/// messages joined by a space ("Merged 2 comments as notes. 1 was in the
/// budget already.") is translated sentence by sentence. In English the
/// result is `text`.
pub fn engine(text: &str) -> String {
    translated(text, false).unwrap_or_else(|| text.to_string())
}

/// `text` translated when it is a message, or messages joined by a space
/// (each sentence ending ". "); `None` when some part isn't one. `nested`:
/// `text` is a parameter's value, which may also be a count phrase or a
/// list of them joined by ", ".
fn translated(text: &str, nested: bool) -> Option<String> {
    if let Some(t) = one_message(text, nested) {
        return Some(t);
    }
    for (at, _) in text.match_indices(". ") {
        let (head, tail) = (&text[..=at], &text[at + 2..]);
        if let (Some(h), Some(t)) = (one_message(head, nested), translated(tail, nested)) {
            return Some(format!("{h} {t}"));
        }
    }
    if nested && text.contains(", ") {
        let parts: Option<Vec<String>> = text.split(", ").map(|p| one_message(p, true)).collect();
        return parts.map(|p| p.join(", "));
    }
    None
}

/// One message in the table's words, or `None` when `text` isn't one.
fn one_message(text: &str, nested: bool) -> Option<String> {
    use method_session::messages::{recognize, recognize_nested, NESTED};
    let m = if nested {
        recognize_nested(text)
    } else {
        recognize(text)
    }?;
    let count = m
        .params
        .iter()
        .find(|(n, _)| *n == "count")
        .map(|(_, v)| v.as_str());
    let words = lookup(m.code, count)?;
    let params: Vec<(&str, String)> = m
        .params
        .iter()
        .map(|(n, v)| {
            (
                *n,
                if NESTED.contains(n) {
                    translated(v, true).unwrap_or_else(|| v.clone())
                } else {
                    v.clone()
                },
            )
        })
        .collect();
    Some(fill(&words, &params))
}

/// An undo label in the person's language: when `text` is one of the
/// engine's labels (`method_session::messages::recognize_label`), the
/// table's words for its code with its parameters filled in (a noun or a
/// rows phrase inside it translated too, the names the person typed kept
/// as they are); otherwise `text` as it is. In English the result is `text`.
pub fn label(text: &str) -> String {
    stored(text, method_session::messages::recognize_label)
}

/// A name the engine gave something it saved (the automatic version before
/// a restore) in the person's language, the name inside it kept as it is;
/// any other name as it is. In English the result is `text`.
pub fn name(text: &str) -> String {
    stored(text, method_session::messages::recognize_name)
}

/// Stored English (a label, a name) in the person's language, when
/// `recognize` knows it.
fn stored(
    text: &str,
    recognize: fn(&str) -> Option<method_session::messages::Recognized>,
) -> String {
    use method_session::messages::NESTED;
    let Some(m) = recognize(text) else {
        return text.to_string();
    };
    let count = m
        .params
        .iter()
        .find(|(n, _)| *n == "count")
        .map(|(_, v)| v.as_str());
    let Some(words) = lookup(m.code, count) else {
        return text.to_string();
    };
    let params: Vec<(&str, String)> = m
        .params
        .iter()
        .map(|(n, v)| {
            let v = if NESTED.contains(n) {
                translated(v, true).unwrap_or_else(|| v.clone())
            } else {
                v.clone()
            };
            (*n, v)
        })
        .collect();
    fill(&words, &params)
}

/// A phrase of the engine's put into a sentence (a role in words, a count
/// phrase) in the person's language; `text` as it is when it isn't one.
pub fn phrase(text: &str) -> String {
    translated(text, true).unwrap_or_else(|| text.to_string())
}

/// [`engine`] over an error, for a command's result.
pub fn engine_err<T>(r: Result<T, String>) -> Result<T, String> {
    r.map_err(|e| engine(&e))
}

/// [`engine`] over a command's error and over the engine's words in what
/// it returns ([`Localize`]).
pub fn engine_dto<T: Localize>(r: Result<T, String>) -> Result<T, String> {
    engine_err(r).map(|mut t| {
        t.localize();
        t
    })
}

/// What a command returns with engine words in it (a summary, a reason, a
/// review's notes), put through [`engine`]. Only the fields the engine
/// writes: the budget's own text (names, notes, descriptions) is left as
/// it is.
pub trait Localize {
    fn localize(&mut self);
}

impl Localize for String {
    fn localize(&mut self) {
        *self = engine(self);
    }
}

impl<T: Localize> Localize for Option<T> {
    fn localize(&mut self) {
        if let Some(t) = self {
            t.localize();
        }
    }
}

impl<T: Localize> Localize for Vec<T> {
    fn localize(&mut self) {
        for t in self {
            t.localize();
        }
    }
}

impl Localize for method_session::apply::ApplySummary {
    fn localize(&mut self) {
        self.range_blocked.localize();
    }
}

impl Localize for method_session::review::ReviewList {
    fn localize(&mut self) {
        for e in &mut self.entries {
            e.why_not.localize();
        }
    }
}

impl Localize for method_session::schedule_days::Review {
    fn localize(&mut self) {
        self.notes.localize();
        self.problems.localize();
        for f in &mut self.figures {
            f.problem.localize();
            f.note.localize();
        }
    }
}

impl Localize for method_session::sheet_import::PreviewDto {
    fn localize(&mut self) {
        self.notes.localize();
    }
}

impl Localize for method_session::clipboard::Preview {
    fn localize(&mut self) {
        self.notes.localize();
    }
}

impl Localize for method_session::diffmerge::WhatChangedDto {
    fn localize(&mut self) {
        self.from.label.localize();
        self.to.label.localize();
    }
}

impl Localize for method_session::diffmerge::MergeDto {
    fn localize(&mut self) {
        self.copy.label.localize();
        if let Some(b) = &mut self.base {
            b.label.localize();
        }
        self.needs_base.localize();
    }
}

/// The History tab's steps: their labels.
impl Localize for method_session::session::History {
    fn localize(&mut self) {
        for s in &mut self.steps {
            s.label = label(&s.label);
        }
    }
}

impl Localize for method_session::rates::RateChoices {
    fn localize(&mut self) {
        for c in &mut self.choices {
            c.problem.localize();
        }
    }
}

/// The Library tab: why the library couldn't be read.
impl Localize for method_session::library::LibraryView {
    fn localize(&mut self) {
        self.problem.localize();
    }
}

impl Localize for method_session::library::LibraryDone {
    fn localize(&mut self) {
        self.library.localize();
        self.message.localize();
    }
}

impl Localize for crate::view::View {
    fn localize(&mut self) {
        if let Some(c) = &mut self.cost {
            c.error.localize();
        }
        version_names(&mut self.budget);
    }
}

/// The Versions dialog's list in a view's `budget`: the automatic
/// version's name.
fn version_names(budget: &mut serde_json::Value) {
    let versions = budget
        .get_mut("versions")
        .and_then(serde_json::Value::as_array_mut);
    for v in versions.into_iter().flatten() {
        if let Some(n) = v.get("name").and_then(serde_json::Value::as_str) {
            v["name"] = name(n).into();
        }
    }
}

fn fill<V: AsRef<str>>(words: &str, params: &[(&str, V)]) -> String {
    method_session::messages::fill(words, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_session::messages::{msg, MESSAGES};
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    /// The page's scripts whose words are all in the tables: a new inline
    /// string in one of them fails [`converted_scripts_have_no_inline_words`].
    /// Since the fourth pass that is every script (the last few have no words
    /// of their own), and a new script has to be added here.
    const CONVERTED: &[&str] = &[
        "i18n.js",
        "shortcuts.js",
        "help.js",
        "licenses.js",
        "updates.js",
        "start.js",
        "startlist.js",
        "setupwin.js",
        "setuptabs.js",
        "setup.js",
        "setuplist.js",
        "setuplists.js",
        "setupbands.js",
        "setupframe.js",
        "apply.js",
        "rates.js",
        "find.js",
        "main.js",
        "modal.js",
        "footer.js",
        "share.js",
        "activity.js",
        "topsheet-setup.js",
        "report.js",
        "reportsetup.js",
        "util.js",
        "tools.js",
        "toolswin.js",
        "notes.js",
        "history.js",
        "versions.js",
        "library.js",
        "review.js",
        "compare.js",
        "tabs.js",
        "pane.js",
        "context.js",
        "packs.js",
        "presets.js",
        "explain.js",
        "overtime.js",
        "actuals.js",
        "actualsimport.js",
        "diffmerge.js",
        "importsetup.js",
        "sheetimport.js",
        "coaimport.js",
        "exchangeimport.js",
        "levels.js",
        "quickentry.js",
        "rowformat.js",
        "rename.js",
        "renumber.js",
        "safety.js",
        "display-settings.js",
        "convertbanner.js",
        "clipboard.js",
        "sharing.js",
        "comments.js",
        "scheduledays.js",
        "ratepick.js",
        "autocomplete.js",
        "display.js",
        "order.js",
        "panelsize.js",
        "api.js",
        "menu.js",
        "announce.js",
        "boot.js",
        "dom.js",
        "format.js",
        "grid.js",
        "host.js",
        "prefs.js",
        "sheet.js",
    ];

    /// Literals that aren't words in a language (a domain name, a file
    /// extension), allowed inline in the converted scripts. Literals with no
    /// letters at all (symbols, numbers, `${...}` alone) are always allowed.
    const NOT_WORDS: &[&str] = &["methodproduction.com"];

    /// Where a literal shows to the user: a property or call the page's
    /// helpers put on screen (dom.js's el(), modal.js's dialogs, menus).
    const SHOWN: &[&str] = &[
        "text:",
        "title:",
        "label:",
        "placeholder:",
        "message:",
        "submitText:",
        "confirmText:",
        "hint:",
        "'aria-label':",
        "toast(",
        ".textContent =",
        ".title =",
        ".placeholder =",
        "setAttribute('aria-label',",
    ];

    fn ui() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("ui")
    }

    fn scripts() -> Vec<(String, String)> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(ui().join("js")).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_some_and(|x| x == "js") {
                let name = p.file_name().unwrap().to_string_lossy().into_owned();
                out.push((name, std::fs::read_to_string(&p).unwrap()));
            }
        }
        out.sort();
        out
    }

    fn is_ident(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_' || c == '$'
    }

    /// The string literal starting at `s` (its first char a quote), without
    /// the quotes; `${...}` kept as written. None when `s` isn't one.
    fn literal(s: &str) -> Option<&str> {
        let q = s.chars().next()?;
        if !matches!(q, '\'' | '"' | '`') {
            return None;
        }
        let bytes = s.as_bytes();
        let mut i = 1;
        let mut depth = 0;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c == '\\' {
                i += 2;
                continue;
            }
            if q == '`' && c == '$' && bytes.get(i + 1) == Some(&b'{') {
                depth += 1;
                i += 2;
                continue;
            }
            if depth > 0 && c == '}' {
                depth -= 1;
            } else if depth == 0 && c == q {
                return Some(&s[1..i]);
            }
            i += 1;
        }
        None
    }

    /// A literal's text outside its `${...}` parts and escapes (`\n`, `\u2019`).
    fn outside_interpolation(lit: &str) -> String {
        let mut out = String::new();
        let mut depth = 0;
        let mut chars = lit.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if chars.next() == Some('u') {
                    for _ in 0..4 {
                        chars.next();
                    }
                }
            } else if c == '$' && chars.peek() == Some(&'{') {
                chars.next();
                depth += 1;
            } else if depth > 0 && c == '}' {
                depth -= 1;
            } else if depth == 0 {
                out.push(c);
            }
        }
        out
    }

    /// Inline words in a script: (line number, literal) for each literal
    /// shown to the user that has letters outside `${...}`.
    fn inline_words(js: &str) -> Vec<(usize, String)> {
        let mut found = Vec::new();
        for (n, line) in js.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                continue;
            }
            for needle in SHOWN {
                let mut from = 0;
                while let Some(at) = line[from..].find(needle) {
                    let start = from + at;
                    from = start + needle.len();
                    let word_start = needle.chars().next().is_some_and(is_ident);
                    if word_start && line[..start].chars().next_back().is_some_and(is_ident) {
                        continue; // subtitle:, ariaLabel:...
                    }
                    let Some(lit) = literal(line[from..].trim_start()) else {
                        continue;
                    };
                    let words = outside_interpolation(lit);
                    if words.chars().any(|c| c.is_alphabetic()) && !NOT_WORDS.contains(&lit) {
                        found.push((n + 1, lit.to_string()));
                    }
                }
            }
        }
        found
    }

    /// The keys a script looks up: every `t('key'` (a literal key, so this
    /// test can see it), and the places t( takes something else.
    fn keys_in_script(js: &str) -> (BTreeSet<String>, Vec<usize>) {
        let mut keys = BTreeSet::new();
        let mut dynamic = Vec::new();
        for (n, line) in js.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            let mut from = 0;
            while let Some(at) = line[from..].find("t(") {
                let start = from + at;
                from = start + 2;
                // A call starts an expression: after a space, a bracket or an
                // operator, not inside a name (`split(`), after a dot or in a
                // regular expression (`/^t(`).
                if line[..start]
                    .chars()
                    .next_back()
                    .is_some_and(|c| !(c.is_whitespace() || "([{,:=?&|+>!;".contains(c)))
                {
                    continue;
                }
                match literal(&line[from..]) {
                    Some(k) if !line[from..].starts_with('`') => {
                        keys.insert(k.to_string());
                    }
                    _ => dynamic.push(n + 1),
                }
            }
        }
        (keys, dynamic)
    }

    /// An attribute's value in a tag's text (`name="value"`).
    fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
        let mut from = 0;
        while let Some(at) = tag[from..].find(&format!("{name}=\"")) {
            let start = from + at;
            from = start + name.len() + 2;
            if tag[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace())
            {
                let end = tag[from..].find('"')?;
                return Some(&tag[from..from + end]);
            }
        }
        None
    }

    /// index.html's words, from its <body>: (key, English written there, where)
    /// for each `data-i18n` text and `data-i18n-aria-label` / `data-i18n-title`
    /// attribute that i18n.js's fillPage() fills, and the words written with no
    /// key beside them.
    fn html_words() -> (Vec<(String, String, String)>, Vec<String>) {
        let html = std::fs::read_to_string(ui().join("index.html")).unwrap();
        let body = &html[html.find("<body").expect("index.html has a <body>")..];
        let mut keyed = Vec::new();
        let mut loose = Vec::new();
        let mut open: Option<&str> = None; // the last opening tag, for the text after it
        let mut rest = body;
        while let Some(lt) = rest.find('<') {
            let text = rest[..lt].trim();
            if text.chars().any(char::is_alphabetic) {
                match open.and_then(|tag| attr(tag, "data-i18n")) {
                    Some(key) => {
                        keyed.push((key.to_string(), text.to_string(), format!("text {text:?}")))
                    }
                    None => loose.push(format!("text {text:?}")),
                }
            }
            let gt = rest[lt..].find('>').expect("a tag ends") + lt;
            let tag = &rest[lt + 1..gt];
            open = (!tag.starts_with('/')).then_some(tag);
            for name in ["aria-label", "title"] {
                if let Some(words) = attr(tag, name) {
                    match attr(tag, &format!("data-i18n-{name}")) {
                        Some(key) => keyed.push((
                            key.to_string(),
                            words.to_string(),
                            format!("{name} {words:?}"),
                        )),
                        None => loose.push(format!("{name} {words:?}")),
                    }
                }
            }
            rest = &rest[gt + 1..];
        }
        (keyed, loose)
    }

    /// The keys Rust looks up: every `tr("key"`, `tr_with("key"` and
    /// `tr_count("key"` in src/,
    /// each menu item's `menu.<id>`, and every engine message's code
    /// (looked up by [`engine`]).
    fn keys_in_rust() -> BTreeSet<String> {
        let mut keys: BTreeSet<String> = crate::menu::item_ids()
            .into_iter()
            .map(|id| format!("menu.{id}"))
            .chain(MESSAGES.iter().map(|m| m.code.to_string()))
            .collect();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for e in std::fs::read_dir(src).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_none_or(|x| x != "rs") || p.ends_with("i18n.rs") {
                continue;
            }
            let rs = std::fs::read_to_string(&p).unwrap();
            // The key may be on the next line, as rustfmt breaks a long call.
            for needle in ["tr(", "tr_with(", "tr_count("] {
                let mut from = 0;
                while let Some(at) = rs[from..].find(needle) {
                    let start = from + at;
                    from = start + needle.len();
                    if rs[..start].chars().next_back().is_some_and(is_ident) {
                        continue;
                    }
                    let Some(rest) = rs[from..].trim_start().strip_prefix('"') else {
                        continue;
                    };
                    let end = rest.find('"').unwrap();
                    keys.insert(rest[..end].to_string());
                }
            }
        }
        keys
    }

    #[test]
    fn the_english_table_is_well_formed() {
        let table = parse_table(EN).unwrap();
        assert!(table.len() > 100, "only {} entries", table.len());
        for (k, v) in &table {
            assert!(
                !k.is_empty()
                    && k.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_'),
                "key {k:?}: letters, digits, dots and underscores only"
            );
            match v {
                Value::String(s) => assert!(!s.trim().is_empty(), "{k} is empty"),
                Value::Object(forms) => {
                    assert!(forms.contains_key("other"), "{k} has no `other` form");
                    for (form, words) in forms {
                        assert!(PLURAL_FORMS.contains(&form.as_str()), "{k}: {form}");
                        assert!(words.is_string(), "{k}.{form} isn't words");
                    }
                }
                _ => panic!("{k} is neither words nor plural forms"),
            }
        }
        // One entry per line, so a repeated key (which JSON lets the last
        // one win) can't hide.
        let mut seen = BTreeSet::new();
        let mut lines = 0;
        for line in EN.lines() {
            let Some(rest) = line.strip_prefix("  \"") else {
                continue;
            };
            let key = &rest[..rest.find('"').unwrap()];
            assert!(seen.insert(key.to_string()), "{key} is in the table twice");
            lines += 1;
        }
        assert_eq!(lines, table.len(), "every entry on a line of its own");
    }

    #[test]
    fn the_house_style_holds_in_the_words() {
        // Plain voice: no em dashes, no exclamation marks.
        for (k, v) in parse_table(EN).unwrap() {
            let all: Vec<String> = match v {
                Value::String(s) => vec![s],
                Value::Object(f) => f
                    .values()
                    .filter_map(|w| w.as_str().map(String::from))
                    .collect(),
                _ => vec![],
            };
            for w in all {
                assert!(!w.contains('\u{2014}'), "{k}: an em dash");
                assert!(!w.contains('!'), "{k}: an exclamation mark");
            }
        }
    }

    #[test]
    fn every_key_used_is_in_the_table_and_every_entry_is_used() {
        let table = parse_table(EN).unwrap();
        let mut used = keys_in_rust();
        for (name, js) in scripts() {
            if name == "i18n.js" {
                continue;
            }
            let (keys, dynamic) = keys_in_script(&js);
            assert!(
                dynamic.is_empty(),
                "{name}: t( takes a literal key, so the tests can see it (lines {dynamic:?})"
            );
            for k in keys {
                assert!(
                    table.contains_key(&k),
                    "{name} uses {k}, which isn't in ui/i18n/en.js"
                );
                used.insert(k);
            }
        }
        for (k, _, _) in html_words().0 {
            assert!(
                table.contains_key(&k),
                "index.html uses {k}, which isn't in ui/i18n/en.js"
            );
            used.insert(k);
        }
        for k in keys_in_rust() {
            assert!(
                table.contains_key(&k),
                "Rust uses {k}, which isn't in ui/i18n/en.js"
            );
        }
        let unused: Vec<&String> = table.keys().filter(|k| !used.contains(*k)).collect();
        assert!(unused.is_empty(), "entries nothing uses: {unused:?}");
    }

    #[test]
    fn converted_scripts_have_no_inline_words() {
        let all: BTreeSet<String> = scripts().into_iter().map(|(n, _)| n).collect();
        let unlisted: Vec<&String> = all
            .iter()
            .filter(|n| !CONVERTED.contains(&n.as_str()))
            .collect();
        assert!(
            unlisted.is_empty(),
            "scripts not in CONVERTED: {unlisted:?}"
        );
        let mut problems = Vec::new();
        for name in CONVERTED {
            assert!(all.contains(*name), "{name} isn't in ui/js");
            let js = std::fs::read_to_string(ui().join("js").join(name)).unwrap();
            for (line, lit) in inline_words(&js) {
                problems.push(format!("{name}:{line}: {lit:?}"));
            }
        }
        assert!(
            problems.is_empty(),
            "words written inline instead of through t() (add them to ui/i18n/en.js):\n  {}",
            problems.join("\n  ")
        );
    }

    #[test]
    fn index_html_words_have_keys_and_match_the_table() {
        let table = parse_table(EN).unwrap();
        let (keyed, loose) = html_words();
        assert!(
            keyed.len() > 10,
            "only {} keyed words in index.html",
            keyed.len()
        );
        assert!(
            loose.is_empty(),
            "index.html words with no data-i18n key beside them: {loose:?}"
        );
        for (key, words, place) in keyed {
            assert_eq!(
                table.get(&key).and_then(Value::as_str),
                Some(words.as_str()),
                "index.html's {place} and ui/i18n/en.js's {key} differ"
            );
        }
    }

    #[test]
    fn the_scan_finds_inline_words_and_allows_the_rest() {
        let js = "el('b', { text: 'Save' });\n\
                  el('b', { title: `Open ${name}` });\n\
                  el('b', { attrs: { 'aria-label': \"Close\" } });\n\
                  toast('Saved.');\n\
                  node.textContent = 'Done';\n\
                  el('b', { text: t('find.next') });\n\
                  el('b', { text: '×', title: `${a}\\n${b}` });\n\
                  el('b', { subtitle: 'Not shown' });\n\
                  // el('b', { text: 'In a comment' });\n\
                  el('a', { text: 'methodproduction.com' });";
        let found: Vec<String> = inline_words(js).into_iter().map(|(_, l)| l).collect();
        assert_eq!(found, ["Save", "Open ${name}", "Close", "Saved.", "Done"]);
    }

    #[test]
    fn keys_are_read_from_literal_lookups_only() {
        let (keys, dynamic) = keys_in_script(
            "t('a.b'); x.t('no'); const s = t(\"c.d\", { count }); t(`e.${f}`); foot('x'); t(key); /^t(1)/;",
        );
        assert_eq!(keys.into_iter().collect::<Vec<_>>(), ["a.b", "c.d"]);
        assert_eq!(dynamic, [1, 1]);
    }

    #[test]
    fn rust_lookups_fill_in_names() {
        let table = parse_table(EN).unwrap();
        let (key, words) = table
            .iter()
            .find_map(|(k, v)| v.as_str().filter(|s| s.contains("{name}")).map(|s| (k, s)))
            .expect("an entry with {name}");
        assert_eq!(tr(key), words);
        assert_eq!(tr_with(key, &[("name", "X")]), words.replace("{name}", "X"));
        assert!(parse_table("export default { \"a\": \"b\" };").is_ok());
        assert!(parse_table("{}").is_err());
    }

    /// The table's entry for each engine message is the engine's English,
    /// so what the person reads doesn't change.
    #[test]
    fn engine_messages_are_in_the_table_as_the_engine_writes_them() {
        let table = parse_table(EN).unwrap();
        for m in MESSAGES {
            let entry = table.get(m.code);
            match (m.one, entry) {
                (None, Some(Value::String(s))) => assert_eq!(s, m.other, "{}", m.code),
                (Some(one), Some(Value::Object(forms))) => {
                    assert_eq!(
                        forms.get("one").and_then(Value::as_str),
                        Some(one),
                        "{}",
                        m.code
                    );
                    assert_eq!(
                        forms.get("other").and_then(Value::as_str),
                        Some(m.other),
                        "{}",
                        m.code
                    );
                }
                (_, e) => panic!("{}: the table has {e:?}", m.code),
            }
        }
    }

    #[test]
    fn engine_messages_come_through_the_table_unchanged_in_english() {
        let samples = [
            msg("engine.edit.account_gone", &[("code", &"1100")]),
            msg(
                "engine.edit.cant_be_negative",
                &[("what", &msg("engine.what.row_cap", &[("row", &3)]))],
            ),
            msg(
                "engine.edit.lines_use_currency",
                &[("count", &1), ("code", &"EUR")],
            ),
            msg(
                "engine.edit.lines_use_currency",
                &[("count", &4), ("code", &"EUR")],
            ),
            msg(
                "engine.edit.formula_refused",
                &[
                    ("what", &"Amt"),
                    ("value", &"1/0"),
                    ("why", &"it divides by zero"),
                ],
            ),
            "Not an engine message.".to_string(),
        ];
        for text in samples {
            assert_eq!(engine(&text), text);
        }
        assert_eq!(engine_err::<()>(Err("x".into())), Err("x".to_string()));
    }

    /// Summaries made of several messages (sentences joined by a space, a
    /// list of count phrases) are found piece by piece, and come back
    /// unchanged in English; text that is only partly the engine's is left
    /// as it is.
    #[test]
    fn summaries_are_translated_piece_by_piece() {
        let both = msg(
            "engine.review.settled_both",
            &[
                ("merged", &msg("engine.phrase.comments", &[("count", &2)])),
                (
                    "dismissed",
                    &msg("engine.phrase.comments", &[("count", &1)]),
                ),
            ],
        );
        let settled = format!(
            "{both} {}",
            msg("engine.review.settled_already", &[("count", &1)])
        );
        assert_eq!(
            settled,
            "Merged 2 comments as notes and dismissed 1 comment. 1 was in the budget already."
        );
        assert!(translated(&settled, false).is_some());
        let added = [
            msg("engine.phrase.new_accounts", &[("count", &1)]),
            msg("engine.phrase.new_setup_items", &[("count", &3)]),
        ]
        .join(", ");
        let imported = msg(
            "engine.import.imported_adding",
            &[
                ("what", &msg("engine.phrase.lines", &[("count", &4)])),
                ("accounts", &msg("engine.phrase.accounts", &[("count", &2)])),
                ("added", &added),
            ],
        );
        assert_eq!(
            imported,
            "Imported 4 lines into 2 accounts (1 new account, 3 new setup items)."
        );
        assert!(translated(&imported, false).is_some());
        assert!(translated("3 accounts", true).is_some());
        assert!(translated("3 accounts", false).is_none());
        let half = "Nothing changed. Something else.".to_string();
        assert!(translated(&half, false).is_none());
        for text in [settled, imported, half] {
            assert_eq!(engine(&text), text);
        }
        assert_eq!(engine("3 accounts"), "3 accounts");
    }

    /// Undo labels (seventh pass): made from the catalog, they come back
    /// through the table unchanged in English, labels nested inside them
    /// included; the person's own words and labels cut short stay as they
    /// are; and an error message is never taken for a label.
    #[test]
    fn undo_labels_are_shown_through_the_table() {
        use method_session::history::{
            cell_label, cells_label, delete_rows_label, row_place, rows_field_label, rows_phrase,
        };
        let session = method_session::session::Session::new();
        let b = session.budget();
        let account = b
            .categories()
            .flat_map(|c| &c.accounts)
            .find(|a| a.details.len() >= 2)
            .expect("the starter has an account with two lines");
        let ids: Vec<_> = account.details.iter().map(|d| d.id).take(2).collect();
        let (code, _) = row_place(b, ids[0]).unwrap();
        let made = [
            cell_label(b, ids[1], "rate"),
            rows_field_label(b, &ids, "x4"),
            cells_label(b, &[(ids[0], "rate", ""), (ids[1], "rate", "")], None),
            delete_rows_label(b, &ids[..1]),
            msg(
                "engine.undo.add_fringe_to_rows",
                &[("name", &"FICA"), ("rows", &rows_phrase(b, &ids))],
            ),
            msg(
                "engine.undo.save_version",
                &[("name", &"Edit rate on 21-01 line 3")],
            ),
            msg("engine.undo.move_accounts_up", &[("count", &1)]),
        ];
        assert_eq!(made[0], format!("Edit rate on {code} line 2"));
        assert_eq!(made[1], format!("Set 4X on 2 lines in {code}"));
        assert_eq!(made[2], format!("Clear 2 cells in {code}"));
        assert_eq!(made[3], format!("Delete line 1 of {code}"));
        assert_eq!(made[4], format!("Add fringe FICA to 2 lines in {code}"));
        for text in &made {
            assert!(
                method_session::messages::recognize_label(text).is_some(),
                "{text:?} is a label"
            );
            assert_eq!(&label(text), text);
        }
        for text in [
            "Rename fringe x\u{2026}",
            "My own words",
            "account 1100 no longer exists",
        ] {
            assert_eq!(label(text), text);
        }
        // Every label's code is in the table (so a language can word it).
        let table = parse_table(EN).unwrap();
        for m in MESSAGES
            .iter()
            .filter(|m| m.code.starts_with("engine.undo."))
        {
            assert!(table.contains_key(m.code), "{}", m.code);
        }
        let mut h = method_session::session::History {
            steps: vec![method_session::session::HistoryStep {
                label: made[0].clone(),
                at: 1,
                done: true,
            }],
            current: 1,
            trimmed: false,
        };
        h.localize();
        assert_eq!(h.steps[0].label, made[0]);
    }

    /// Sharing's labels (a role given, a person removed, an invite, a value
    /// put back): stored in the change log as the catalog's English, each
    /// recognized as its own label, the role inside it as a phrase, and
    /// shown back unchanged in English.
    #[test]
    fn the_automatic_version_name_is_shown_through_the_table() {
        let auto = method_core::versions::before_restore_name("Approved");
        let m = method_session::messages::recognize_name(&auto).expect("a name");
        assert_eq!(m.code, "engine.name.before_restoring");
        assert_eq!(name(&auto), auto);
        // A name the person typed, even one shaped like a label, is theirs.
        for typed in ["Approved", "Removed Morgan", "Before the shoot"] {
            assert!(method_session::messages::recognize_name(typed).is_none());
            assert_eq!(name(typed), typed);
        }
        let mut budget = serde_json::json!({ "versions": [
            { "id": 1, "name": "Approved" },
            { "id": 2, "name": auto },
        ] });
        version_names(&mut budget);
        assert_eq!(budget["versions"][0]["name"], "Approved");
        assert_eq!(budget["versions"][1]["name"], auto);
    }

    /// The session modules converted in the eleventh pass (notes,
    /// versions, the Library, packs, rates, Compare, reports): their
    /// messages are recognized as their codes, a list of count phrases
    /// inside one too, and come back unchanged in English.
    #[test]
    fn session_module_messages_are_shown_through_the_table() {
        use method_session::messages::{recognize, recognize_nested, word};
        let changes = [
            msg("engine.phrase.items_added", &[("count", &2)]),
            msg("engine.phrase.items_kept_library", &[("count", &1)]),
        ]
        .join(", ");
        let made = [
            (
                msg("engine.library.saved", &[("changes", &changes)]),
                "engine.library.saved",
            ),
            (
                msg(
                    "engine.library.folder_deleted_items",
                    &[("name", &"Crew"), ("count", &3)],
                ),
                "engine.library.folder_deleted_items",
            ),
            (
                msg(
                    "engine.packs.imported_from",
                    &[("title", &"Field Station"), ("changes", &changes)],
                ),
                "engine.packs.imported_from",
            ),
            (
                msg("engine.notes.too_long", &[("len", &5001), ("max", &5000)]),
                "engine.notes.too_long",
            ),
            (
                word("engine.versions.needs_name"),
                "engine.versions.needs_name",
            ),
            (
                msg(
                    "engine.rates.not_a_number",
                    &[
                        ("row", &2),
                        ("classification", &"Grip, Key"),
                        ("unit", &"Day"),
                        ("value", &"lots"),
                    ],
                ),
                "engine.rates.not_a_number",
            ),
            (
                msg("engine.compare.too_many", &[("max", &15)]),
                "engine.compare.too_many",
            ),
            (
                msg(
                    "engine.presets.gone",
                    &[("what", &word("engine.what.report_preset"))],
                ),
                "engine.presets.gone",
            ),
            (
                msg("engine.reports.cant_save", &[("why", &"the name is empty")]),
                "engine.reports.cant_save",
            ),
        ];
        assert_eq!(
            made[0].0,
            "Saved to the library: 2 items added, 1 item kept as the library had it."
        );
        assert_eq!(
            made[1].0,
            "Deleted the folder Crew; its 3 items are unfiled now."
        );
        assert_eq!(made[7].0, "That report preset no longer exists.");
        for (text, code) in &made {
            let m = recognize(text).unwrap_or_else(|| panic!("{text:?} is a message"));
            assert_eq!(m.code, *code, "{text:?}");
            assert_eq!(&engine(text), text);
        }
        let m = recognize(&made[0].0).unwrap();
        assert_eq!(m.params, [("changes", changes.clone())]);
        for part in changes.split(", ") {
            assert!(
                recognize_nested(part).is_some_and(|r| r.code.starts_with("engine.phrase.items_")),
                "{part:?} is a count phrase"
            );
        }
        assert_eq!(
            engine_err::<()>(Err(word("engine.rates.line_gone"))),
            Err("That line is no longer in the budget.".to_string())
        );
    }

    #[test]
    fn sharing_labels_are_shown_through_the_table() {
        let dept = msg("engine.role.department", &[("scope", &"2000 Production")]);
        let made = [
            (
                msg(
                    "engine.undo.set_role",
                    &[("name", &"Morgan"), ("role", &dept)],
                ),
                "engine.undo.set_role",
            ),
            (
                msg(
                    "engine.undo.invite",
                    &[("role", &msg("engine.role.editor", &[]))],
                ),
                "engine.undo.invite",
            ),
            (
                msg(
                    "engine.undo.cancel_invite",
                    &[("role", &msg("engine.role.co_owner", &[]))],
                ),
                "engine.undo.cancel_invite",
            ),
            (
                msg(
                    "engine.undo.join_from_invite",
                    &[("role", &msg("engine.role.read_only", &[]))],
                ),
                "engine.undo.join_from_invite",
            ),
            (
                msg("engine.undo.remove_person", &[("name", &"Morgan")]),
                "engine.undo.remove_person",
            ),
            (
                msg(
                    "engine.undo.put_back_field",
                    &[("field", &"rate"), ("place", &"line 3 in 2400")],
                ),
                "engine.undo.put_back_field",
            ),
            (
                msg("engine.undo.put_back_field_budget", &[("field", &"title")]),
                "engine.undo.put_back_field_budget",
            ),
        ];
        assert_eq!(made[0].0, "Morgan: Edits 2000 Production");
        for (text, code) in &made {
            let m = method_session::messages::recognize_label(text)
                .unwrap_or_else(|| panic!("{text:?} is a label"));
            assert_eq!(m.code, *code, "{text:?}");
            if let Some((_, role)) = m.params.iter().find(|(n, _)| *n == "role") {
                assert!(
                    method_session::messages::recognize_nested(role).is_some(),
                    "{role:?} is a role phrase"
                );
            }
            assert_eq!(&label(text), text);
        }
        assert_eq!(phrase(&dept), dept);
        let table = parse_table(EN).unwrap();
        for m in MESSAGES
            .iter()
            .filter(|m| m.code.starts_with("engine.phrase.role_"))
        {
            assert!(table.contains_key(m.code), "{}", m.code);
        }
    }

    /// Dialog words method-session also writes (for the browser build) are
    /// the table's words for them.
    #[test]
    fn dialog_words_match_the_engine() {
        use method_format::export::Format;
        use method_session::export::dialog_text;
        for f in [Format::Xlsx, Format::Csv, Format::Tsv] {
            let (title, filter, _) = dialog_text(f, "x");
            assert_eq!(
                crate::export::dialog_words(f),
                (title.into(), filter.into())
            );
        }
        for (purpose, key) in [
            ("diff", "dialog.merge_pick_diff"),
            ("copy", "dialog.merge_pick_copy"),
            ("base", "dialog.merge_pick_base"),
        ] {
            assert_eq!(
                method_session::diffmerge::pick_title(purpose).unwrap(),
                tr(key)
            );
        }
        assert_eq!(
            method_session::open_any::ALL_FILTER,
            tr("dialog.filter_all_openable")
        );
        assert_eq!(
            tr_count("dialog.unsaved_many", 2, &[("list", "a\nb")]),
            "2 budgets have unsaved changes:\n\na\nb\n\nSave them before closing? If you don't, they will be lost."
        );
    }
}
