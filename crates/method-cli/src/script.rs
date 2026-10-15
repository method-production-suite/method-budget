//! The edit language behind `method set` and `method apply` (docs/CLI.md, "Edit
//! scripts"): one command per line, applied in order to a copy of the
//! budget. Every command must succeed and the result must recalculate, or
//! nothing is written.
//!
//! ```text
//! # comments start a line with #
//! set title "Feature, draft 3"
//! set compact_versions true
//! set arithmetic exact
//! set round_account_totals true
//! set trailing_level_fringes drop
//! set cutoff_factor 1.5
//! set fringe_row account 9900
//! set fringe_row description "Payroll fringes"
//! set fringe_row position 4
//! set group_rules ungrouped_lines exclude
//! set global SHOOT_DAYS 32
//! set fringe FICA rate 7.65
//! set fringe FICA cutoff 168600.00
//! set fringe MEALS basis amount_counts
//! set fringe PENSION max_hours_per_day 12
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
use std::str::FromStr;

use method_core::rename::{rename, SetupKind};
use method_core::{
    recalc, Arithmetic, Budget, Cents, CutoffFactor, Dec, Detail, FlatBasis, Formula, FringeKind,
    Inclusion, RecalcOptions, Report, RowId, TrailingFringes,
};
use method_session::edit::{self, parse_money_as, typed, EditError, FringeRowInput, SubCents};
use method_session::rowstyle::check_color;

use crate::cli::{money, CliError, Outcome};

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
    pub fn is_none(&self) -> bool {
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
    /// What a flat fringe counts on a line (decisions.md, entry 54).
    FringeBasis(String, FlatBasis),
    /// A flat fringe's most hours a day; `None` for no cap.
    FringeMaxHours(String, Option<Dec>),
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
    /// How totals are worked out (docs/FORMAT.md, "Budget").
    Arithmetic(Arithmetic),
    /// Round account totals to whole units (docs/FORMAT.md, "Account
    /// totals").
    RoundAccountTotals(bool),
    /// Under production-level posting, keep or drop the fringes of
    /// categories after the last production total (docs/FORMAT.md, "Budget").
    TrailingLevelFringes(TrailingFringes),
    /// What every percent fringe's cutoff is multiplied by (docs/FORMAT.md,
    /// "Budget").
    CutoffFactor(CutoffFactor),
    /// One field of the budget's fringe row (`account`, `description` or
    /// `position`; [`FRINGE_ROW_FIELDS`]), `None` to clear it
    /// (docs/FORMAT.md, "Fringe row").
    FringeRow(&'static str, Option<String>),
    /// One of the group rules ([`GROUP_RULES`]; docs/FORMAT.md, "Group
    /// rules").
    GroupRule(&'static str, Inclusion),
}

/// The budget's fringe row fields, by the names files and commands use.
pub const FRINGE_ROW_FIELDS: [&str; 3] = ["account", "description", "position"];

/// A fringe row field by name, with its value (`none` or blank clears it).
pub fn fringe_row(field: &str, value: &Word) -> Result<Op, String> {
    let name = FRINGE_ROW_FIELDS
        .into_iter()
        .find(|f| *f == field.trim())
        .ok_or_else(|| {
            format!(
                "`{field}` is not a fringe row field: use {}",
                FRINGE_ROW_FIELDS.join(", ")
            )
        })?;
    let v = (!value.is_none()).then(|| value.text.trim().to_string());
    if name == "position" {
        if let Some(p) = &v {
            p.parse::<usize>().map_err(|_| {
                format!("`{p}` is not a topsheet row: use a number from 0, or none")
            })?;
        }
    }
    Ok(Op::FringeRow(name, v))
}

/// A cutoff factor by name (`1.0`, `1.5`, `2.0`, `2.5` or `3.0`).
pub fn cutoff_factor(s: &str) -> Result<CutoffFactor, String> {
    CutoffFactor::parse(s)
        .ok_or_else(|| format!("`{s}` is not a cutoff factor: use 1.0, 1.5, 2.0, 2.5 or 3.0"))
}

/// The group rules by the names files and commands use.
pub const GROUP_RULES: [&str; 3] = ["ungrouped_lines", "conflict", "fringes_in_totals"];

/// A group rule by name, with its value (`include` or `exclude`).
pub fn group_rule(rule: &str, value: &str) -> Result<Op, String> {
    let name = GROUP_RULES
        .into_iter()
        .find(|r| *r == rule.trim())
        .ok_or_else(|| {
            format!(
                "`{rule}` is not a group rule: use {}",
                GROUP_RULES.join(", ")
            )
        })?;
    let v = Inclusion::parse(value)
        .ok_or_else(|| format!("`{value}` is not a group rule value: use include or exclude"))?;
    Ok(Op::GroupRule(name, v))
}

fn group_rule_slot<'a>(b: &'a mut Budget, rule: &str) -> &'a mut Inclusion {
    let r = &mut b.group_rules;
    match rule {
        "ungrouped_lines" => &mut r.ungrouped_lines,
        "conflict" => &mut r.conflict,
        _ => &mut r.fringes_in_totals,
    }
}

fn need<'a>(w: &'a [Word], n: usize, form: &str) -> Result<&'a [Word], String> {
    match w.len() {
        k if k == n => Ok(w),
        k if k < n => Err(format!("missing a value: {form}")),
        _ => Err(format!("too many words (quote values with spaces): {form}")),
    }
}

/// Money as the app reads it, but never rounded: a script says exactly
/// what it means.
fn parse_money(s: &str) -> Result<Cents, String> {
    parse_money_as(s, SubCents::Refuse)
}

/// A trailing level fringes setting by name (`keep` or `drop`).
pub fn trailing(s: &str) -> Result<TrailingFringes, String> {
    TrailingFringes::parse(s)
        .ok_or_else(|| format!("`{s}` is not a trailing level fringes setting: use keep or drop"))
}

/// An arithmetic by name (`binary` or `exact`; `mmb` still reads as `binary`).
pub fn arithmetic(s: &str) -> Result<Arithmetic, String> {
    Arithmetic::parse(s).ok_or_else(|| format!("`{s}` is not an arithmetic: use binary or exact"))
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
        ["set", "arithmetic"] => {
            let w = need(&w, 3, "set arithmetic binary|exact")?;
            Op::Arithmetic(arithmetic(&w[2].text)?)
        }
        ["set", "round_account_totals"] => {
            let w = need(&w, 3, "set round_account_totals true|false")?;
            Op::RoundAccountTotals(flag(&w[2])?)
        }
        ["set", "trailing_level_fringes"] => {
            let w = need(&w, 3, "set trailing_level_fringes keep|drop")?;
            Op::TrailingLevelFringes(trailing(&w[2].text)?)
        }
        ["set", "cutoff_factor"] => {
            let w = need(&w, 3, "set cutoff_factor 1.0|1.5|2.0|2.5|3.0")?;
            Op::CutoffFactor(cutoff_factor(&w[2].text)?)
        }
        ["set", "fringe_row"] => {
            let w = need(&w, 4, "set fringe_row account|description|position VALUE")?;
            fringe_row(&w[2].text, &w[3])?
        }
        ["set", "group_rules"] => {
            let w = need(
                &w,
                4,
                "set group_rules ungrouped_lines|conflict|fringes_in_totals include|exclude",
            )?;
            group_rule(&w[2].text, &w[3].text)?
        }
        ["set", "global"] => {
            let w = need(&w, 4, "set global NAME FORMULA")?;
            Op::Global(w[2].text.clone(), w[3].text.clone())
        }
        ["set", "fringe"] => {
            let form = "set fringe NAME rate FORMULA | set fringe NAME cutoff AMOUNT|none | \
                        set fringe NAME basis amount|amount_counts|quantity | \
                        set fringe NAME max_hours_per_day HOURS|none";
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
                "basis" => Op::FringeBasis(
                    w[2].text.clone(),
                    FlatBasis::parse(&w[4].text).ok_or_else(|| {
                        format!(
                            "`{}` is not a basis: use amount, amount_counts or quantity",
                            w[4].text
                        )
                    })?,
                ),
                "max_hours_per_day" => {
                    let hours = if w[4].is_none() {
                        None
                    } else {
                        let h = Dec::from_str(w[4].text.trim())
                            .map_err(|_| format!("`{}` is not a number of hours", w[4].text))?;
                        if h <= Dec::ZERO {
                            return Err("max_hours_per_day must be more than 0".into());
                        }
                        Some(h)
                    };
                    Op::FringeMaxHours(w[2].text.clone(), hours)
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

fn account<'a>(b: &'a Budget, code: &str) -> Result<&'a method_core::Account, String> {
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

/// `true`/`yes`/`on` or `false`/`no`/`off`.
pub fn flag(w: &Word) -> Result<bool, String> {
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
            let new = if v.is_none() {
                None
            } else {
                Some(check_color(&text)?)
            };
            let old = opt_shown(&d.color);
            d.color = new;
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
                .filter(|n| *n <= method_core::MAX_INDENT)
                .ok_or_else(|| format!("`{text}` is not an indent from 0 to 4"))?;
            let old = d.indent.to_string();
            d.indent = n;
            (old, n.to_string())
        }
        other => return Err(format!("a line has no field `{other}`")),
    })
}

/// Sets global `name` to `formula` with the app's edit, which refuses a
/// formula that divides by zero or makes globals refer back to themselves
/// (decisions.md, entry 60). The line for the summary.
pub fn set_global(b: &mut Budget, name: &str, formula: &str) -> Result<Vec<String>, EditError> {
    let name = name.trim();
    let old = b.globals.get(name).map(|f| shown(&f.0));
    typed::set_global(b, name, formula)?;
    let new = shown(formula.trim());
    Ok(vec![match old {
        Some(old) => format!("global {name} {old} -> {new}"),
        None => format!("global {name} added as {new}"),
    }])
}

/// Applies one command to `b`. Returns a line per change made, for the
/// summary. On an error `b` may be partly changed; callers work on a copy.
pub fn apply(b: &mut Budget, op: &Op) -> Result<Vec<String>, String> {
    let mut log = Vec::new();
    match op {
        Op::Title(t) => {
            let old = shown(&b.title);
            edit::set_title(b, t)?;
            log.push(format!("title {old} -> {}", shown(&b.title)));
        }
        Op::Global(name, f) => log.extend(set_global(b, name, f)?),
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
        Op::FringeCutoff(name, new) => {
            let fr = b
                .fringes
                .get_mut(name)
                .ok_or_else(|| format!("there is no fringe {name}"))?;
            let c = match &mut fr.kind {
                FringeKind::Percent { cutoff } => cutoff,
                // A flat fringe may keep one from before (never used): it
                // can be cleared, not set.
                FringeKind::Flat { cutoff, .. } if new.is_none() => cutoff,
                FringeKind::Flat { .. } => {
                    return Err(format!(
                        "fringe {name} is flat and has no cutoff: cutoffs apply to percent fringes only"
                    ))
                }
            };
            let show = |c: &Option<Cents>| c.map_or("none".into(), money);
            log.push(format!("fringe {name} cutoff {} -> {}", show(c), show(new)));
            *c = *new;
        }
        Op::FringeBasis(name, new) => {
            let fr = b
                .fringes
                .get_mut(name)
                .ok_or_else(|| format!("there is no fringe {name}"))?;
            let FringeKind::Flat { basis, .. } = &mut fr.kind else {
                return Err(format!(
                    "fringe {name} is a percent fringe: only a flat fringe has a basis"
                ));
            };
            log.push(format!(
                "fringe {name} basis {} -> {}",
                basis.name(),
                new.name()
            ));
            *basis = *new;
        }
        Op::FringeMaxHours(name, new) => {
            let fr = b
                .fringes
                .get_mut(name)
                .ok_or_else(|| format!("there is no fringe {name}"))?;
            let FringeKind::Flat {
                max_hours_per_day, ..
            } = &mut fr.kind
            else {
                return Err(format!(
                    "fringe {name} is a percent fringe: only a flat fringe has max hours per day"
                ));
            };
            let show = |h: &Option<Dec>| h.map_or("none".into(), |h| h.to_string());
            log.push(format!(
                "fringe {name} max_hours_per_day {} -> {}",
                show(max_hours_per_day),
                show(new)
            ));
            *max_hours_per_day = *new;
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
        Op::Arithmetic(a) => {
            log.push(format!(
                "arithmetic {} -> {}",
                b.arithmetic.name(),
                a.name()
            ));
            b.arithmetic = *a;
        }
        Op::RoundAccountTotals(on) => {
            log.push(format!(
                "round_account_totals {} -> {on}",
                b.round_account_totals
            ));
            b.round_account_totals = *on;
        }
        Op::TrailingLevelFringes(t) => {
            log.push(format!(
                "trailing_level_fringes {} -> {}",
                b.trailing_level_fringes.name(),
                t.name()
            ));
            b.trailing_level_fringes = *t;
        }
        Op::CutoffFactor(f) => {
            log.push(format!(
                "cutoff_factor {} -> {}",
                b.cutoff_factor.name(),
                f.name()
            ));
            b.cutoff_factor = *f;
        }
        Op::FringeRow(field, v) => {
            let r = &b.fringe_row;
            let mut row = FringeRowInput {
                account: r.account.clone(),
                description: r.description.clone(),
                position: r.position,
            };
            let shown = |v: &Option<String>| v.clone().unwrap_or_else(|| "none".into());
            match *field {
                "account" => {
                    log.push(format!(
                        "fringe_row account {} -> {}",
                        shown(&r.account),
                        shown(v)
                    ));
                    row.account = v.clone();
                }
                "description" => {
                    log.push(format!(
                        "fringe_row description {} -> {}",
                        shown(&r.description),
                        shown(v)
                    ));
                    row.description = v.clone();
                }
                _ => {
                    let at = match v {
                        Some(p) => Some(
                            p.parse::<usize>()
                                .map_err(|_| format!("`{p}` is not a topsheet row"))?,
                        ),
                        None => None,
                    };
                    // Said with the bounds; the app's own message is for a
                    // row that went away while its settings were open.
                    if at.is_some_and(|p| p > b.topsheet.len()) {
                        return Err(format!(
                            "the topsheet has {} rows, so the fringe row can be above row 0 to {}",
                            b.topsheet.len(),
                            b.topsheet.len()
                        ));
                    }
                    let old = r.position.map_or("none".into(), |p| p.to_string());
                    let new = at.map_or("none".into(), |p| p.to_string());
                    log.push(format!("fringe_row position {old} -> {new}"));
                    row.position = at;
                }
            }
            edit::set_fringe_row(b, &row)?;
        }
        Op::GroupRule(rule, v) => {
            let slot = group_rule_slot(b, rule);
            log.push(format!(
                "group_rules {rule} {} -> {}",
                slot.name(),
                v.name()
            ));
            *slot = *v;
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
        let d = method_core::diff::diff_with(&before, Some(&r_before), &after, Some(&r_after));
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
        crate::cli::save(path, &after)?;
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
            "set arithmetic exact",
            "set arithmetic binary",
            "set arithmetic mmb",
            "set round_account_totals true",
            "set round_account_totals off",
            "set trailing_level_fringes drop",
            "set trailing_level_fringes keep",
            "set cutoff_factor 1.5",
            "set cutoff_factor 3",
            "set fringe_row account 9900",
            "set fringe_row description \"Payroll fringes\"",
            "set fringe_row position 4",
            "set fringe_row position none",
            "set group_rules ungrouped_lines exclude",
            "set group_rules conflict include",
            "set group_rules fringes_in_totals exclude",
            "set global DAYS 30",
            "set fringe FICA rate 7.65",
            "set fringe FICA cutoff 168,600.00",
            "set fringe FICA cutoff none",
            "set fringe MEALS basis amount_counts",
            "set fringe MEALS basis quantity",
            "set fringe PEN max_hours_per_day 12",
            "set fringe PEN max_hours_per_day none",
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
            ("set fringe MEALS basis hours", "not a basis"),
            ("set fringe PEN max_hours_per_day 0", "more than 0"),
            (
                "set fringe PEN max_hours_per_day lots",
                "not a number of hours",
            ),
            ("set line #1 colour red", "no field `colour`"),
            ("add tag #1 set A", "not a tag kind"),
            ("rename account A B", "not something to rename"),
            ("delete everything", "unknown command"),
            ("set group_rules ungrouped exclude", "not a group rule"),
            ("set group_rules conflict maybe", "include or exclude"),
            ("set group_rules conflict", "missing a value"),
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
