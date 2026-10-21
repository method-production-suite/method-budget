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

/// The words for `key` (the `other` form of a plural entry). A missing key
/// gives the key itself; the tests keep that from happening.
pub fn tr(key: &str) -> String {
    match english().get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(forms)) => forms
            .get("other")
            .and_then(Value::as_str)
            .unwrap_or(key)
            .to_string(),
        _ => {
            debug_assert!(false, "i18n: {key} is not in ui/i18n/en.js");
            key.to_string()
        }
    }
}

/// [`tr`] with `{name}` filled in from `params`.
pub fn tr_with(key: &str, params: &[(&str, &str)]) -> String {
    let mut out = tr(key);
    for (name, value) in params {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    /// The page's scripts whose words are all in the tables: a new inline
    /// string in one of them fails [`converted_scripts_have_no_inline_words`].
    /// The rest are converted in later passes (docs/research/accessibility-audit.md,
    /// "Localization").
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

    /// The keys Rust looks up: every `tr("key"` and `tr_with("key"` in src/,
    /// and each menu item's `menu.<id>`.
    fn keys_in_rust() -> BTreeSet<String> {
        let mut keys: BTreeSet<String> = crate::menu::item_ids()
            .into_iter()
            .map(|id| format!("menu.{id}"))
            .collect();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for e in std::fs::read_dir(src).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_none_or(|x| x != "rs") || p.ends_with("i18n.rs") {
                continue;
            }
            let rs = std::fs::read_to_string(&p).unwrap();
            for needle in ["tr(\"", "tr_with(\""] {
                let mut from = 0;
                while let Some(at) = rs[from..].find(needle) {
                    let start = from + at;
                    from = start + needle.len();
                    if rs[..start].chars().next_back().is_some_and(is_ident) {
                        continue;
                    }
                    let end = rs[from..].find('"').unwrap();
                    keys.insert(rs[from..from + end].to_string());
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
}
