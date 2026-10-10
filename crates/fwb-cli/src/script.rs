//! The edit language behind `method set` and `method apply` (docs/CLI.md, "Edit
//! scripts"): one command per line, applied in order to a copy of the
//! budget. Every command must succeed and the result must recalculate, or
//! nothing is written.
//!
//! ```text
//! # comments start a line with #
//! set title "Feature, draft 3"
//! set compact_versions true
//! set global SHOOT_DAYS 32
//! set fringe FICA rate 7.65
//! set fringe FICA cutoff 168600.00
//! set line 21-01:2 rate 900
//! set line #17 description "Director of photography"
//! add tag 21-01:* fringe FICA
//! remove tag #17 group Night
//! rename location "Stage 4" "Stage 5"
//! apply charge Contingency
//! remove charge Insurance
//! ```

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;
use std::str::FromStr;

use fwb_core::rename::{rename, valid_name, SetupKind};
use fwb_core::{
    recalc, Budget, Cents, Dec, Detail, Formula, FringeKind, RecalcOptions, Report, RowId,
};

use crate::cli::{money, write_atomic, CliError, Outcome};

/// One word of a script line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    /// Written in double quotes (so `"none"` is the text, not the keyword).
    pub quoted: bool,
}

impl Word {
    pub fn plain(text: impl Into<String>) -> Word {
        Word {
            text: text.into(),
            quoted: false,
        }
    }

    /// The `none` keyword, or an empty value: "clear this".
    fn is_none(&self) -> bool {
        self.text.is_empty() || (!self.quoted && self.text == "none")
    }
}

/// Splits a line into words: runs of non-spaces, or text in double quotes
/// where `\"` is a quote and `\\` a backslash.
pub fn words(line: &str) -> Result<Vec<Word>, String> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '"' {
            chars.next();
            let mut text = String::new();
            loop {
                match chars.next() {
                    None => return Err("a quoted value is missing its closing quote".into()),
                    Some('"') => break,
                    Some('\\') => match chars.next() {
                        Some(e @ ('"' | '\\')) => text.push(e),
                        Some(e) => {
                            text.push('\\');
                            text.push(e);
                        }
                        None => return Err("a quoted value is missing its closing quote".into()),
                    },
                    Some(ch) => text.push(ch),
                }
            }
            if chars.peek().is_some_and(|c| !c.is_whitespace()) {
                return Err("put a space after a closing quote".into());
            }
            out.push(Word { text, quoted: true });
        } else {
            let mut text = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() {
                    break;
                }
                if ch == '"' {
                    return Err("a quote can only start a word".into());
                }
                text.push(ch);
                chars.next();
            }
            out.push(Word::plain(text));
        }
    }
    Ok(out)
}

/// Which rows a command acts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowRef {
    /// `#17`: the row with id 17.
    Id(RowId),
    /// `21-01:3`: the third row of account 21-01 (counting every row).
    Nth(String, usize),
    /// `21-01:*`: every line and adjustment row of account 21-01.
    All(String),
}

impl RowRef {
    pub fn parse(s: &str) -> Result<RowRef, String> {
        if let Some(id) = s.strip_prefix('#') {
            return id
                .parse()
                .map(RowRef::Id)
                .map_err(|_| format!("`{s}` is not a row id (use #17)"));
        }
        let bad = || format!("`{s}` is not a row: use #ID, ACCOUNT:N or ACCOUNT:*");
        let (account, n) = s.rsplit_once(':').ok_or_else(bad)?;
        if account.is_empty() {
            return Err(bad());
        }
        if n == "*" {
            return Ok(RowRef::All(account.to_string()));
        }
        match n.parse::<usize>() {
            Ok(n) if n >= 1 => Ok(RowRef::Nth(account.to_string(), n)),
            _ => Err(bad()),
        }
    }
}

impl std::fmt::Display for RowRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RowRef::Id(id) => write!(f, "#{id}"),
            RowRef::Nth(a, n) => write!(f, "{a}:{n}"),
            RowRef::All(a) => write!(f, "{a}:*"),
        }
    }
}

/// The detail row fields `set line` can change.
pub const LINE_FIELDS: &[&str] = &[
    "description",
    "amount",
    "x",
    "rate",
    "x4",
    "unit",
    "unit2",
    "unit3",
    "unit4",
    "currency",
    "set",
    "location",
    "fringe_range",
    "bold",
    "italic",
    "indent",
    "color",
];

/// One parsed command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Title(String),
    Global(String, String),
    FringeRate(String, String),
    FringeCutoff(String, Option<Cents>),
    Line(RowRef, String, Word),
    Tag {
        add: bool,
        rows: RowRef,
        group: bool,
        name: String,
    },
    Rename(SetupKind, String, String),
    Charge(String, bool),
    /// Save versions as deltas (docs/FORMAT.md, "Version deltas").
    CompactVersions(bool),
}

fn need<'a>(w: &'a [Word], n: usize, form: &str) -> Result<&'a [Word], String> {
    match w.len() {
        k if k == n => Ok(w),
        k if k < n => Err(format!("missing a value: {form}")),
        _ => Err(format!("too many words (quote values with spaces): {form}")),
    }
}

fn parse_money(s: &str) -> Result<Cents, String> {
    let clean: String = s.chars().filter(|c| *c != ',').collect();
    let d = Dec::from_str(&clean).map_err(|_| format!("`{s}` is not an amount"))?;
    let c = d.to_cents().ok_or_else(|| format!("`{s}` is too large"))?;
    if Dec::from_cents(c) != d {
        return Err(format!("`{s}` has more than two decimal places"));
    }
    Ok(c)
}

/// Parses one line. `Ok(None)` for a blank line or a comment.
pub fn parse(line: &str) -> Result<Option<Op>, String> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let w = words(line)?;
    let head: Vec<&str> = w.iter().take(2).map(|x| x.text.as_str()).collect();
    let op = match head.as_slice() {
        ["set", "title"] => {
            let w = need(&w, 3, "set title TEXT")?;
            Op::Title(w[2].text.clone())
        }
        ["set", "compact_versions"] => {
            let w = need(&w, 3, "set compact_versions true|false")?;
            Op::CompactVersions(flag(&w[2])?)
        }
        ["set", "global"] => {
            let w = need(&w, 4, "set global NAME FORMULA")?;
            Op::Global(w[2].text.clone(), w[3].text.clone())
        }
        ["set", "fringe"] => {
            let form = "set fringe NAME rate FORMULA | set fringe NAME cutoff AMOUNT|none";
            let w = need(&w, 5, form)?;
            match w[3].text.as_str() {
                "rate" => Op::FringeRate(w[2].text.clone(), w[4].text.clone()),
                "cutoff" => {
                    let cutoff = if w[4].is_none() {
                        None
                    } else {
                        Some(parse_money(&w[4].text)?)
                    };
                    Op::FringeCutoff(w[2].text.clone(), cutoff)
                }
                other => return Err(format!("a fringe has no field `{other}`: {form}")),
            }
        }
        ["set", "line"] => {
            let w = need(&w, 5, "set line ROW FIELD VALUE")?;
            let field = w[3].text.clone();
            if !LINE_FIELDS.contains(&field.as_str()) {
                return Err(format!(
                    "a line has no field `{field}`: use one of {}",
                    LINE_FIELDS.join(", ")
                ));
            }
            Op::Line(RowRef::parse(&w[2].text)?, field, w[4].clone())
        }
        [verb @ ("add" | "remove"), "tag"] => {
            let form = format!("{verb} tag ROW fringe|group NAME");
            let w = need(&w, 5, &form)?;
            let group = match w[3].text.as_str() {
                "fringe" => false,
                "group" => true,
                other => return Err(format!("`{other}` is not a tag kind: {form}")),
            };
            Op::Tag {
                add: *verb == "add",
                rows: RowRef::parse(&w[2].text)?,
                group,
                name: w[4].text.clone(),
            }
        }
        ["rename", kind] => {
            let form = "rename global|fringe|group|unit|set|location|currency OLD NEW";
            let k = SetupKind::from_name(kind)
                .ok_or_else(|| format!("`{kind}` is not something to rename: {form}"))?;
            let w = need(&w, 4, form)?;
            Op::Rename(k, w[2].text.clone(), w[3].text.clone())
        }
        [verb @ ("apply" | "remove" | "unapply"), "charge"] => {
            let w = need(&w, 3, &format!("{verb} charge NAME"))?;
            Op::Charge(w[2].text.clone(), *verb == "apply")
        }
        _ => {
            return Err(format!(
                "unknown command `{}`: use set, add tag, remove tag, rename, apply charge \
                 or remove charge",
                head.join(" ")
            ))
        }
    };
    Ok(Some(op))
}

/// The rows `r` names, as ids (one or more), or why there are none.
fn resolve(b: &Budget, r: &RowRef) -> Result<Vec<RowId>, String> {
    match r {
        RowRef::Id(id) => {
            let found = b
                .categories()
                .flat_map(|c| &c.accounts)
                .flat_map(|a| &a.details)
                .any(|d| d.id == *id);
            if found {
                Ok(vec![*id])
            } else {
                Err(format!("there is no row #{id}"))
            }
        }
        RowRef::Nth(code, n) => {
            let a = account(b, code)?;
            a.details
                .get(n - 1)
                .map(|d| vec![d.id])
                .ok_or_else(|| format!("account {code} has {} rows, not {n}", a.details.len()))
        }
        RowRef::All(code) => {
            let a = account(b, code)?;
            let ids: Vec<RowId> = a
                .details
                .iter()
                .filter(|d| d.kind.is_costed())
                .map(|d| d.id)
                .collect();
            if ids.is_empty() {
                Err(format!("account {code} has no lines"))
            } else {
                Ok(ids)
            }
        }
    }
}

fn account<'a>(b: &'a Budget, code: &str) -> Result<&'a fwb_core::Account, String> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .find(|a| a.code == code)
        .ok_or_else(|| format!("there is no account {code}"))
}

/// The row with this id and where it is (`21-01:2`), for messages.
fn row_mut(b: &mut Budget, id: RowId) -> Option<(String, &mut Detail)> {
    for c in b.categories_mut() {
        for a in &mut c.accounts {
            let code = a.code.clone();
            for (i, d) in a.details.iter_mut().enumerate() {
                if d.id == id {
                    return Some((format!("{code}:{} (#{id})", i + 1), d));
                }
            }
        }
    }
    None
}

fn shown(s: &str) -> String {
    format!("\"{s}\"")
}

fn opt_shown(s: &Option<String>) -> String {
    s.as_deref().map_or_else(|| "none".into(), shown)
}

fn flag(w: &Word) -> Result<bool, String> {
    match w.text.as_str() {
        "true" | "yes" | "on" => Ok(true),
        "false" | "no" | "off" => Ok(false),
        other => Err(format!("`{other}` is not true or false")),
    }
}

/// Sets `field` of row `d` from `v`, checked against the budget's setup
/// tables (`setup` is the budget before the change). Returns the old and new
/// values as shown in messages.
fn set_field(
    setup: &Budget,
    d: &mut Detail,
    field: &str,
    v: &Word,
) -> Result<(String, String), String> {
    let text = v.text.clone();
    let formula = |f: &mut Formula| {
        let old = shown(&f.0);
        f.0 = text.clone();
        (old, shown(&text))
    };
    let unit = |u: &mut String| -> Result<(String, String), String> {
        if !text.is_empty() && !setup.units.contains_key(&text) {
            return Err(format!("there is no unit {text}"));
        }
        let old = shown(u);
        *u = text.clone();
        Ok((old, shown(&text)))
    };
    let named = |o: &mut Option<String>, exists: bool, what: &str| {
        let new = if v.is_none() {
            None
        } else {
            Some(text.clone())
        };
        if new.is_some() && !exists {
            return Err(format!("there is no {what} {text}"));
        }
        let old = opt_shown(o);
        *o = new;
        Ok((old, opt_shown(o)))
    };
    Ok(match field {
        "description" => {
            let old = shown(&d.description);
            d.description = text.clone();
            (old, shown(&text))
        }
        "amount" => formula(&mut d.amount),
        "x" => formula(&mut d.x),
        "rate" => formula(&mut d.rate),
        "x4" => formula(&mut d.x4),
        "unit" => unit(&mut d.unit)?,
        "unit2" => unit(&mut d.unit2)?,
        "unit3" => unit(&mut d.unit3)?,
        "unit4" => unit(&mut d.unit4)?,
        "currency" => {
            let exists = text == setup.base_currency || setup.currencies.contains_key(&text);
            named(&mut d.currency, exists, "currency")?
        }
        "set" => named(&mut d.set, setup.sets.contains_key(&text), "set")?,
        "location" => named(
            &mut d.location,
            setup.locations.contains_key(&text),
            "location",
        )?,
        "color" => {
            let ok = text.len() == 7
                && text.starts_with('#')
                && text[1..].bytes().all(|b| b.is_ascii_hexdigit());
            if !v.is_none() && !ok {
                return Err(format!("`{text}` is not a color: use #rrggbb"));
            }
            let lower = text.to_ascii_lowercase();
            let old = opt_shown(&d.color);
            d.color = (!v.is_none()).then_some(lower);
            (old, opt_shown(&d.color))
        }
        "fringe_range" => {
            let old = d.fringe_range.map_or("none".into(), |n| n.to_string());
            d.fringe_range = if v.is_none() {
                None
            } else {
                Some(
                    text.parse()
                        .map_err(|_| format!("`{text}` is not a fringe range number"))?,
                )
            };
            (old, d.fringe_range.map_or("none".into(), |n| n.to_string()))
        }
        "bold" => {
            let old = d.bold.to_string();
            d.bold = flag(v)?;
            (old, d.bold.to_string())
        }
        "italic" => {
            let old = d.italic.to_string();
            d.italic = flag(v)?;
            (old, d.italic.to_string())
        }
        "indent" => {
            let n: u8 = text
                .parse()
                .ok()
                .filter(|n| *n <= fwb_core::MAX_INDENT)
                .ok_or_else(|| format!("`{text}` is not an indent from 0 to 4"))?;
            let old = d.indent.to_string();
            d.indent = n;
            (old, n.to_string())
        }
        other => return Err(format!("a line has no field `{other}`")),
    })
}

/// Applies one command to `b`. Returns a line per change made, for the
/// summary. On an error `b` may be partly changed; callers work on a copy.
pub fn apply(b: &mut Budget, op: &Op) -> Result<Vec<String>, String> {
    let mut log = Vec::new();
    match op {
        Op::Title(t) => {
            log.push(format!("title {} -> {}", shown(&b.title), shown(t)));
            b.title = t.clone();
        }
        Op::Global(name, f) => {
            if !valid_name(SetupKind::Global, name) {
                return Err(format!(
                    "`{name}` can't be a global name: use letters, digits and _"
                ));
            }
            match b.globals.insert(name.clone(), Formula(f.clone())) {
                Some(old) => log.push(format!("global {name} {} -> {}", shown(&old.0), shown(f))),
                None => log.push(format!("global {name} added as {}", shown(f))),
            }
        }
        Op::FringeRate(name, f) => {
            let fr = b
                .fringes
                .get_mut(name)
                .ok_or_else(|| format!("there is no fringe {name}"))?;
            log.push(format!(
                "fringe {name} rate {} -> {}",
                shown(&fr.rate.0),
                shown(f)
            ));
            fr.rate = Formula(f.clone());
        }
        Op::FringeCutoff(name, cutoff) => {
            let fr = b
                .fringes
                .get_mut(name)
                .ok_or_else(|| format!("there is no fringe {name}"))?;
            let FringeKind::Percent { cutoff: c } = &mut fr.kind else {
                return Err(format!("fringe {name} is flat and has no cutoff"));
            };
            let show = |c: &Option<Cents>| c.map_or("none".into(), money);
            log.push(format!(
                "fringe {name} cutoff {} -> {}",
                show(c),
                show(cutoff)
            ));
            *c = *cutoff;
        }
        Op::Line(rows, field, value) => {
            let setup = b.clone();
            for id in resolve(b, rows)? {
                let (at, d) = row_mut(b, id).expect("resolved rows exist");
                let (old, new) = set_field(&setup, d, field, value)?;
                log.push(format!("{at} {field} {old} -> {new}"));
            }
        }
        Op::Tag {
            add,
            rows,
            group,
            name,
        } => {
            let (what, exists) = if *group {
                ("group", b.groups.contains_key(name))
            } else {
                ("fringe", b.fringes.contains_key(name))
            };
            if *add && !exists {
                return Err(format!("there is no {what} {name}"));
            }
            for id in resolve(b, rows)? {
                let (at, d) = row_mut(b, id).expect("resolved rows exist");
                let set: &mut BTreeSet<String> = if *group {
                    &mut d.groups
                } else {
                    &mut d.fringes
                };
                let changed = if *add {
                    set.insert(name.clone())
                } else {
                    set.remove(name)
                };
                if changed {
                    let verb = if *add { "added" } else { "removed" };
                    log.push(format!("{at} {what} {name} {verb}"));
                }
            }
            if log.is_empty() {
                let state = if *add { "already has" } else { "doesn't have" };
                log.push(format!("{rows}: {state} {what} {name}, nothing to do"));
            }
        }
        Op::Rename(kind, from, to) => {
            let n = rename(b, *kind, from, to).map_err(|e| e.to_string())?;
            let s = if n == 1 { "" } else { "s" };
            log.push(format!(
                "{} {} renamed {}, {n} reference{s} rewritten",
                kind.name(),
                shown(from),
                shown(to.trim())
            ));
        }
        Op::CompactVersions(on) => {
            log.push(format!("compact_versions {} -> {on}", b.compact_versions));
            b.compact_versions = *on;
        }
        Op::Charge(name, applied) => {
            let c = b
                .charges
                .iter_mut()
                .find(|c| c.name == *name)
                .ok_or_else(|| format!("there is no charge {name}"))?;
            if c.applied == *applied {
                let state = if *applied {
                    "already applied"
                } else {
                    "already not applied"
                };
                log.push(format!("charge {name} {state}"));
            } else {
                c.applied = *applied;
                let state = if *applied {
                    "applied"
                } else {
                    "no longer applied"
                };
                log.push(format!("charge {name} {state}"));
            }
        }
    }
    Ok(log)
}

/// What an edit changed, in totals: the grand and net totals and every
/// category whose total moved.
pub fn totals_summary(before: &Report, after: &Report) -> String {
    let mut out = String::new();
    let mut line = |label: &str, a: Cents, z: Cents| {
        let delta = z.saturating_sub(a);
        let sign = if delta > 0 { "+" } else { "" };
        let _ = writeln!(
            out,
            "  {label:<36}{:>18} -> {:>18}  ({sign}{})",
            money(a),
            money(z),
            money(delta)
        );
    };
    for c in &after.categories {
        let old = before
            .categories
            .iter()
            .find(|o| o.code == c.code)
            .map_or(0, |o| o.total);
        if old != c.total {
            line(&format!("{} {}", c.code, c.name), old, c.total);
        }
    }
    for c in &before.categories {
        if !after.categories.iter().any(|n| n.code == c.code) {
            line(&format!("{} {}", c.code, c.name), c.total, 0);
        }
    }
    line("Grand total", before.grand_total, after.grand_total);
    line("Net total", before.net_total, after.net_total);
    out
}

/// Runs `ops` (each with the label errors name it by, such as
/// `script.txt line 4`) against the budget at `path`, then writes the result
/// atomically, or with `dry_run` only says what would change. Exit code 3
/// when a command fails or the result doesn't recalculate; nothing is
/// written then.
pub fn run(path: &str, ops: &[(String, Op)], dry_run: bool) -> Outcome {
    let before = crate::cli::load(path)?;
    let mut after = before.clone();
    let mut log = Vec::new();
    for (label, op) in ops {
        let lines =
            apply(&mut after, op).map_err(|e| CliError::rejected(format!("{label}: {e}")))?;
        log.extend(lines);
    }
    let opts = RecalcOptions::default();
    let r_before = recalc(&before, &opts).map_err(|e| CliError::failed(format!("{path}: {e}")))?;
    let r_after = recalc(&after, &opts).map_err(|e| {
        CliError::rejected(format!(
            "{path}: after the edits the budget doesn't recalculate: {e}; nothing was written"
        ))
    })?;
    let mut out = String::new();
    for l in &log {
        let _ = writeln!(out, "{l}");
    }
    let changed = after != before;
    if !changed {
        out.push_str("no changes\n");
    } else if dry_run {
        // What would change, in the same words `method diff` uses.
        let d = fwb_core::diff::diff_with(&before, Some(&r_before), &after, Some(&r_after));
        out.push('\n');
        out.push_str(&d.to_text());
        out.push('\n');
    } else {
        out.push_str("totals:\n");
        out.push_str(&totals_summary(&r_before, &r_after));
    }
    if dry_run {
        out.push_str("dry run: nothing written\n");
    } else if changed {
        write_atomic(Path::new(path), fwb_format::to_string(&after).as_bytes())?;
        let _ = writeln!(out, "wrote {path}");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<String> {
        words(s).unwrap().into_iter().map(|w| w.text).collect()
    }

    #[test]
    fn splits_words() {
        assert_eq!(
            w("set  line #3 rate 900"),
            ["set", "line", "#3", "rate", "900"]
        );
        assert_eq!(
            w(r#"set title "A \"B\" \\ C""#),
            ["set", "title", r#"A "B" \ C"#]
        );
        assert_eq!(w(r#"x "" y"#), ["x", "", "y"]);
        assert!(words(r#"x "open"#).is_err());
        assert!(words(r#"x a"b"#).is_err());
        assert!(words(r#"x "a"b"#).is_err());
        assert!(words(r#"x "none""#).unwrap()[1].quoted);
    }

    #[test]
    fn row_refs() {
        assert_eq!(RowRef::parse("#17"), Ok(RowRef::Id(17)));
        assert_eq!(RowRef::parse("21-01:3"), Ok(RowRef::Nth("21-01".into(), 3)));
        assert_eq!(RowRef::parse("21.01:*"), Ok(RowRef::All("21.01".into())));
        assert!(RowRef::parse("21-01:0").is_err());
        assert!(RowRef::parse("21-01").is_err());
        assert!(RowRef::parse(":3").is_err());
        assert!(RowRef::parse("#x").is_err());
    }

    #[test]
    fn parses_every_command() {
        let ok = [
            "set title \"New title\"",
            "set compact_versions true",
            "set global DAYS 30",
            "set fringe FICA rate 7.65",
            "set fringe FICA cutoff 168,600.00",
            "set fringe FICA cutoff none",
            "set line #1 rate \"WEEKS * 5\"",
            "add tag 10-01:* fringe FICA",
            "remove tag #1 group Night",
            "rename unit Week Wk",
            "apply charge Contingency",
            "remove charge Contingency",
            "   # a comment",
            "",
        ];
        for line in ok {
            assert!(parse(line).is_ok(), "{line}: {:?}", parse(line));
        }
        assert_eq!(parse("# x").unwrap(), None);
        let bad = [
            ("set global DAYS", "missing a value"),
            ("set global DAYS 3 0", "too many words"),
            ("set fringe FICA kind flat", "no field `kind`"),
            ("set fringe FICA cutoff 1.005", "two decimal places"),
            ("set line #1 colour red", "no field `colour`"),
            ("add tag #1 set A", "not a tag kind"),
            ("rename account A B", "not something to rename"),
            ("delete everything", "unknown command"),
        ];
        for (line, want) in bad {
            let e = parse(line).unwrap_err();
            assert!(e.contains(want), "{line}: {e}");
        }
    }

    #[test]
    fn money_values() {
        assert_eq!(parse_money("1,500.25"), Ok(150_025));
        assert_eq!(parse_money("7"), Ok(700));
        assert!(parse_money("abc").is_err());
    }
}
