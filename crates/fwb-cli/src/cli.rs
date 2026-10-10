//! What every subcommand shares: errors that carry an exit code, a small flag
//! parser, and reading and writing budget files.
//!
//! A subcommand is a module under `src/cmd/` with
//! `pub const USAGE: &str` (its lines of the help text) and
//! `pub fn run(args: &[String]) -> Outcome`, where `args` are the words after
//! the subcommand's name. What `run` returns in `Ok` is printed to stdout as
//! is; an `Err` prints `fwb: <message>` to stderr and exits with its code.

use std::fmt;
use std::path::Path;

use fwb_core::{Budget, Cents};

/// What a subcommand returns: text for stdout, or an error with its exit code.
pub type Outcome = Result<String, CliError>;

/// Exit codes shared by every subcommand (docs/CLI.md, "Exit codes").
pub mod exit {
    /// It worked.
    pub const OK: u8 = 0;
    /// Something failed: a file couldn't be read, parsed or written, or the
    /// budget has a calculation problem.
    pub const FAILED: u8 = 1;
    /// The command line itself is wrong: unknown command or flag, missing
    /// argument.
    pub const USAGE: u8 = 2;
    /// An edit (`fwb set`, `fwb apply`) was refused: a bad script line, a name
    /// that doesn't exist, or a result that doesn't recalculate. Nothing was
    /// written.
    pub const REJECTED: u8 = 3;
}

/// An error and the exit code it ends the program with. A blank message
/// prints nothing (for a command that already wrote what it had to say).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliError {
    pub code: u8,
    pub message: String,
}

impl CliError {
    pub fn new(code: u8, message: impl Into<String>) -> CliError {
        CliError {
            code,
            message: message.into(),
        }
    }

    pub fn failed(message: impl Into<String>) -> CliError {
        CliError::new(exit::FAILED, message)
    }

    /// A usage error: the message, a blank line, then `usage`.
    pub fn usage(message: impl Into<String>, usage: &str) -> CliError {
        let message = message.into();
        let text = if message.is_empty() {
            format!("usage:\n{usage}")
        } else {
            format!("{message}\n\nusage:\n{usage}")
        };
        CliError::new(exit::USAGE, text.trim_end().to_string())
    }

    pub fn rejected(message: impl Into<String>) -> CliError {
        CliError::new(exit::REJECTED, message)
    }
}

impl From<String> for CliError {
    fn from(message: String) -> CliError {
        CliError::failed(message)
    }
}

impl From<&str> for CliError {
    fn from(message: &str) -> CliError {
        CliError::failed(message)
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// A subcommand's words, split into positional arguments, flags with a value
/// and switches.
///
/// Flags are named with their aliases as `"--out|-o"`; the first name is the
/// one to ask for. A value follows as the next word or after `=`
/// (`--out=x`). `--` ends the flags. A word that looks like a negative number
/// (`-5`, `-0.25`) is a positional argument, not a flag, so values such as
/// `fwb set f.fwb global X -5` need no quoting.
#[derive(Clone, Debug, Default)]
pub struct Flags {
    positional: Vec<String>,
    values: Vec<(&'static str, String)>,
    switches: Vec<&'static str>,
}

/// The canonical name of `word` among `names` (`"--out|-o"` entries).
fn lookup(names: &[&'static str], word: &str) -> Option<&'static str> {
    names.iter().find_map(|spec| {
        spec.split('|')
            .any(|n| n == word)
            .then(|| spec.split('|').next().unwrap_or(spec))
    })
}

fn looks_negative_number(word: &str) -> bool {
    let rest = &word[1..];
    !rest.is_empty()
        && rest
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '.')
}

impl Flags {
    /// Parses `args`. `values` are the flags that take a value, `switches` the
    /// ones that don't; anything else starting with `-` is a usage error that
    /// shows `usage`.
    pub fn parse(
        args: &[String],
        values: &[&'static str],
        switches: &[&'static str],
        usage: &str,
    ) -> Result<Flags, CliError> {
        let mut out = Flags::default();
        let mut it = args.iter();
        while let Some(word) = it.next() {
            if word == "--" {
                out.positional.extend(it.by_ref().cloned());
                break;
            }
            if !word.starts_with('-') || word == "-" || looks_negative_number(word) {
                out.positional.push(word.clone());
                continue;
            }
            let (name, inline) = match word.split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
                _ => (word.as_str(), None),
            };
            if let Some(canon) = lookup(values, name) {
                let v = match inline {
                    Some(v) => v,
                    None => it
                        .next()
                        .cloned()
                        .ok_or_else(|| CliError::usage(format!("{name} needs a value"), usage))?,
                };
                out.values.push((canon, v));
            } else if let Some(canon) = lookup(switches, name) {
                if inline.is_some() {
                    return Err(CliError::usage(format!("{name} takes no value"), usage));
                }
                out.switches.push(canon);
            } else {
                return Err(CliError::usage(
                    format!("unexpected argument `{word}`"),
                    usage,
                ));
            }
        }
        Ok(out)
    }

    /// The positional arguments, in order.
    pub fn positional(&self) -> &[String] {
        &self.positional
    }

    /// Exactly `n` positional arguments, or a usage error naming `what`.
    pub fn exactly(&self, n: usize, what: &str, usage: &str) -> Result<&[String], CliError> {
        match self.positional().len() {
            k if k == n => Ok(self.positional()),
            k if k < n => Err(CliError::usage(format!("missing {what}"), usage)),
            _ => Err(CliError::usage(
                format!("unexpected argument `{}`", self.positional[n]),
                usage,
            )),
        }
    }

    /// The flag's value (the last one when given more than once).
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .rev()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }

    /// Every value the flag was given, in order.
    pub fn values(&self, name: &str) -> Vec<&str> {
        self.values
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// Whether the switch was given.
    pub fn has(&self, name: &str) -> bool {
        self.switches.contains(&name)
    }
}

/// Reads and parses a budget file.
pub fn load(path: &str) -> Result<Budget, CliError> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    fwb_format::from_str(&text).map_err(|e| CliError::failed(format!("{path}: {e}")))
}

/// Writes `b` to `path` in the native format, atomically.
pub fn save(path: &str, b: &Budget) -> Result<(), CliError> {
    write_atomic(Path::new(path), fwb_format::to_string(b).as_bytes())
}

/// Writes `bytes` to `path` through a temporary file in the same folder and a
/// rename, so a reader never sees half a file and a failed write leaves the
/// old file as it was.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let shown = path.display();
    let name = path
        .file_name()
        .ok_or_else(|| CliError::failed(format!("{shown}: not a file name")))?;
    let tmp = path.with_file_name(format!(
        ".{}.fwb-tmp-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let result = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(CliError::failed(format!("{shown}: {e}")));
    }
    Ok(())
}

/// Refuses to write `out` when it is the file `input` (the same file under
/// another name included).
pub fn not_the_input(input: &str, out: &Path, what: &str) -> Result<(), CliError> {
    let input_real = std::fs::canonicalize(input).map_err(|e| format!("{input}: {e}"))?;
    if std::fs::canonicalize(out).is_ok_and(|p| p == input_real) {
        return Err(CliError::failed(format!(
            "{}: that is the budget being {what}; choose another output",
            out.display()
        )));
    }
    Ok(())
}

/// `1234567` cents as `12,345.67`, negatives in parentheses.
pub fn money(c: Cents) -> String {
    let a = c.unsigned_abs();
    let digits = (a / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let s = format!("{grouped}.{:02}", a % 100);
    if c < 0 {
        format!("({s})")
    } else {
        s
    }
}

/// `text` as a JSON string literal, quotes included.
pub fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &[&str]) -> Vec<String> {
        s.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn money_formats() {
        assert_eq!(money(0), "0.00");
        assert_eq!(money(5), "0.05");
        assert_eq!(money(123_456_789), "1,234,567.89");
        assert_eq!(money(-100_000), "(1,000.00)");
        assert_eq!(money(99_999), "999.99");
    }

    #[test]
    fn flags_parse() {
        let f = Flags::parse(
            &words(&[
                "a.fwb",
                "-o",
                "x",
                "--json",
                "--exclude=G",
                "-5",
                "--exclude",
                "H",
            ]),
            &["--out|-o", "--exclude"],
            &["--json"],
            "u",
        )
        .unwrap();
        assert_eq!(f.positional(), ["a.fwb", "-5"]);
        assert_eq!(f.value("--out"), Some("x"));
        assert_eq!(f.values("--exclude"), ["G", "H"]);
        assert!(f.has("--json"));
        assert!(!f.has("--dry-run"));
        let f = Flags::parse(&words(&["--", "--json"]), &[], &["--json"], "u").unwrap();
        assert_eq!(f.positional(), ["--json"]);

        let e = Flags::parse(&words(&["--nope"]), &[], &[], "the usage").unwrap_err();
        assert_eq!(e.code, exit::USAGE);
        assert!(e.message.contains("unexpected argument `--nope`"));
        assert!(e.message.contains("the usage"));
        let e = Flags::parse(&words(&["-o"]), &["--out|-o"], &[], "u").unwrap_err();
        assert!(e.message.contains("-o needs a value"), "{}", e.message);
        let e = Flags::parse(&words(&["--json=1"]), &[], &["--json"], "u").unwrap_err();
        assert!(e.message.contains("takes no value"));
    }

    #[test]
    fn exactly_counts_positionals() {
        let f = Flags::parse(&words(&["a", "b"]), &[], &[], "u").unwrap();
        assert!(f.exactly(2, "x", "u").is_ok());
        assert!(f
            .exactly(3, "the value", "u")
            .unwrap_err()
            .message
            .contains("missing the value"));
        assert!(f
            .exactly(1, "x", "u")
            .unwrap_err()
            .message
            .contains("unexpected argument `b`"));
    }

    #[test]
    fn json_strings_escape() {
        assert_eq!(json_string("a\"b\\c\nd\u{1}"), "\"a\\\"b\\\\c\\nd\\u0001\"");
        assert_eq!(json_string("café"), "\"café\"");
    }

    #[test]
    fn atomic_write_replaces() {
        let dir = std::env::temp_dir().join(format!("fwb-cli-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.txt");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        assert!(write_atomic(&dir.join("missing").join("x"), b"z").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
