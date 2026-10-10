//! Find and replace: a typed search over one budget, and replacing what was
//! found in the places the user checked.
//!
//! A search looks for one kind of thing (its scope) at a time:
//!
//! - text: a line's description or note, any formula's text (amount, X,
//!   rate, 4X), or an account's name or number, with match case and whole
//!   word options;
//! - a global used in formulas, matched as a whole token the way the formula
//!   reader splits a formula, so `DAYS` never matches inside `SHOOT_DAYS`;
//! - a setup item on lines: a fringe, group, set, location, unit (any of the
//!   four unit columns) or currency (the base currency finds the lines left
//!   blank, which use it), matched by its exact name.
//!
//! Results come in budget order, one per place: a cell of a detail row, or an
//! account's name or number. [`search`] only reads, so it can run after every
//! edit; on a 5,000-row budget it takes a few milliseconds.
//!
//! [`replace_in`] changes only the places it is given (the checked results),
//! and only if every one of them still matches the search: otherwise it
//! changes nothing and says so. Text is replaced inside the field; a global
//! only as a token inside formulas (by another global, a number or a formula,
//! or by its current value when it is removed, as deleting a global does,
//! decisions.md entry 12); an item is swapped for another existing one, or
//! taken off. Run through `Session::edit` (as the command does), a replace is
//! one undo step. Renaming a setup item is a separate thing and isn't here.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use fwb_core::expr::{is_ident, Expr};
use fwb_core::{Budget, Dec, Detail, Formula, RowId, RowKind, TopsheetRow, MAX_NOTE_CHARS};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{edit_budget, AppState, DocId};
use crate::edit::{details, row_gone, rows_by_id, EditResult};
use crate::view::View;

/// Most results sent back at once; `count` still says how many there are.
pub const MAX_HITS: usize = 1_000;

/// Longest text shown for a result (a description, a formula, a note), in
/// characters.
pub const PREVIEW_CHARS: usize = 160;

const FORMULA_FIELDS: [&str; 4] = ["amount", "x", "rate", "x4"];
const UNIT_FIELDS: [&str; 4] = ["unit", "unit2", "unit3", "unit4"];

/// What a search looks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Description,
    Formula,
    Global,
    Fringe,
    Group,
    Set,
    Location,
    Unit,
    Currency,
    Note,
    Account,
}

impl Scope {
    pub fn parse(s: &str) -> EditResult<Scope> {
        Ok(match s {
            "description" => Scope::Description,
            "formula" => Scope::Formula,
            "global" => Scope::Global,
            "fringe" => Scope::Fringe,
            "group" => Scope::Group,
            "set" => Scope::Set,
            "location" => Scope::Location,
            "unit" => Scope::Unit,
            "currency" => Scope::Currency,
            "note" => Scope::Note,
            "account" => Scope::Account,
            other => return Err(format!("can't search for `{other}`")),
        })
    }

    /// The row fields this scope looks in, in the grid's order.
    fn fields(self) -> &'static [&'static str] {
        match self {
            Scope::Description => &["description"],
            Scope::Formula | Scope::Global => &FORMULA_FIELDS,
            Scope::Fringe => &["fringes"],
            Scope::Group => &["groups"],
            Scope::Set => &["set"],
            Scope::Location => &["location"],
            Scope::Unit => &UNIT_FIELDS,
            Scope::Currency => &["currency"],
            Scope::Note => &["note"],
            Scope::Account => &["code", "name"],
        }
    }

    fn what(self) -> &'static str {
        match self {
            Scope::Description => "description",
            Scope::Formula => "formula",
            Scope::Global => "global",
            Scope::Fringe => "fringe",
            Scope::Group => "group",
            Scope::Set => "set",
            Scope::Location => "location",
            Scope::Unit => "unit",
            Scope::Currency => "currency",
            Scope::Note => "note",
            Scope::Account => "account",
        }
    }
}

/// A search, as the Find panel sends it.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct FindQuery {
    /// `description`, `formula`, `global`, `fringe`, `group`, `set`,
    /// `location`, `unit`, `currency`, `note` or `account`.
    pub scope: String,
    /// The text, or the global's or item's name.
    #[serde(default)]
    pub text: String,
    /// Text scopes only: capitals must match.
    #[serde(default)]
    pub match_case: bool,
    /// Text scopes only: the text must not run on into letters, digits or `_`
    /// on either side.
    #[serde(default)]
    pub whole_word: bool,
}

/// One place a search matched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Hit {
    /// Where the account is: its topsheet index and index in its category.
    pub ti: usize,
    pub ai: usize,
    pub category: String,
    pub category_name: String,
    pub account: String,
    pub account_name: String,
    /// The detail row, or `None` for an account's own name or number.
    pub row_id: Option<RowId>,
    /// The row's place in its account, from 1.
    pub line: Option<usize>,
    /// The row's description, cut to [`PREVIEW_CHARS`].
    pub description: String,
    /// The matching field: a row field as the grid names its column
    /// (`description`, `amount`, `x`, `rate`, `x4`, `unit` to `unit4`,
    /// `fringes`, `groups`, `set`, `location`, `currency`, `note`), or `code`
    /// or `name` of an account.
    pub field: &'static str,
    /// What the field holds, cut to [`PREVIEW_CHARS`].
    pub text: String,
}

/// What a search found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Found {
    /// How many places match.
    pub count: usize,
    /// The first [`MAX_HITS`] of them, in budget order.
    pub hits: Vec<Hit>,
    /// Some were left out.
    pub truncated: bool,
}

/// A place to replace in: a result's `row_id` (or `account`, for an
/// account's name) and `field`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Target {
    #[serde(default)]
    pub row_id: Option<RowId>,
    #[serde(default)]
    pub account: Option<String>,
    pub field: String,
}

// ---------------------------------------------------------------------------
// Matching text

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Plain text matching with the panel's options.
struct Matcher {
    text: String,
    needle: Vec<char>,
    fold: bool,
    whole: bool,
}

impl Matcher {
    fn new(q: &FindQuery) -> Option<Matcher> {
        if q.text.is_empty() {
            return None;
        }
        Some(Matcher {
            text: q.text.clone(),
            needle: q.text.chars().collect(),
            fold: !q.match_case,
            whole: q.whole_word,
        })
    }

    /// Where the needle ends if it starts at byte `at`.
    fn match_at(&self, hay: &str, at: usize) -> Option<usize> {
        let mut it = hay[at..].char_indices();
        for &n in &self.needle {
            let (_, h) = it.next()?;
            if h != n && !(self.fold && h.to_lowercase().eq(n.to_lowercase())) {
                return None;
            }
        }
        Some(at + it.next().map_or(hay.len() - at, |(i, _)| i))
    }

    fn bounded(&self, hay: &str, start: usize, end: usize) -> bool {
        if !self.whole {
            return true;
        }
        let first = self.needle.first().copied().is_some_and(is_word);
        let last = self.needle.last().copied().is_some_and(is_word);
        let before = hay[..start].chars().next_back().is_some_and(is_word);
        let after = hay[end..].chars().next().is_some_and(is_word);
        !((first && before) || (last && after))
    }

    /// Byte ranges of every match in `hay`, left to right, not overlapping.
    fn ranges(&self, hay: &str) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < hay.len() {
            let start = if self.fold {
                at
            } else {
                match hay[at..].find(&self.text) {
                    Some(i) => at + i,
                    None => break,
                }
            };
            if let Some(end) = self.match_at(hay, start) {
                if self.bounded(hay, start, end) {
                    out.push((start, end));
                    at = end;
                    continue;
                }
            }
            at = start + hay[start..].chars().next().map_or(1, char::len_utf8);
        }
        out
    }

    fn is_match(&self, hay: &str) -> bool {
        !hay.is_empty() && !self.ranges(hay).is_empty()
    }
}

/// `hay` with each range replaced by `with`.
fn splice(hay: &str, ranges: &[(usize, usize)], with: &str) -> String {
    let mut out = String::with_capacity(hay.len() + ranges.len() * with.len());
    let mut from = 0;
    for &(s, e) in ranges {
        out.push_str(&hay[from..s]);
        out.push_str(with);
        from = e;
    }
    out.push_str(&hay[from..]);
    out
}

/// Byte ranges of the global-name tokens equal to `name` in a formula, split
/// the way the formula reader splits it: a run of digits and dots is a
/// number, a letter or `_` starts a name that runs on through letters, digits
/// and `_`. So `DAYS` matches in `DAYS * 2` but not in `SHOOT_DAYS` or
/// `DAYS2`.
pub fn global_tokens(src: &str, name: &str) -> Vec<(usize, usize)> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_digit() || c == b'.' {
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            if &src[start..i] == name {
                out.push((start, i));
            }
        } else {
            i += 1;
        }
    }
    out
}

fn cut(s: &str) -> String {
    match s.char_indices().nth(PREVIEW_CHARS) {
        Some((i, _)) => s[..i].to_string(),
        None => s.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Row fields by name

fn formula<'a>(d: &'a Detail, field: &str) -> Option<&'a Formula> {
    Some(match field {
        "amount" => &d.amount,
        "x" => &d.x,
        "rate" => &d.rate,
        "x4" => &d.x4,
        _ => return None,
    })
}

fn formula_mut<'a>(d: &'a mut Detail, field: &str) -> Option<&'a mut Formula> {
    Some(match field {
        "amount" => &mut d.amount,
        "x" => &mut d.x,
        "rate" => &mut d.rate,
        "x4" => &mut d.x4,
        _ => return None,
    })
}

fn unit_mut<'a>(d: &'a mut Detail, field: &str) -> Option<&'a mut String> {
    Some(match field {
        "unit" => &mut d.unit,
        "unit2" => &mut d.unit2,
        "unit3" => &mut d.unit3,
        "unit4" => &mut d.unit4,
        _ => return None,
    })
}

fn unit<'a>(d: &'a Detail, field: &str) -> Option<&'a str> {
    Some(match field {
        "unit" => &d.unit,
        "unit2" => &d.unit2,
        "unit3" => &d.unit3,
        "unit4" => &d.unit4,
        _ => return None,
    })
}

fn joined(names: &BTreeSet<String>) -> String {
    names
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a row's field holds, as text.
fn field_text(d: &Detail, field: &str) -> String {
    match field {
        "description" => d.description.clone(),
        "fringes" => joined(&d.fringes),
        "groups" => joined(&d.groups),
        "set" => d.set.clone().unwrap_or_default(),
        "location" => d.location.clone().unwrap_or_default(),
        "currency" => d.currency.clone().unwrap_or_default(),
        "note" => fwb_core::notes_text(&d.notes),
        f => formula(d, f)
            .map(|x| x.0.clone())
            .or_else(|| unit(d, f).map(String::from))
            .unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------
// The search

/// What a search looks for, ready to test fields with.
enum Want {
    Text(Matcher),
    Token(String),
    Name(String),
}

struct Finder {
    scope: Scope,
    want: Want,
    base: String,
}

impl Finder {
    /// `None` when there is nothing to look for yet (no text typed).
    fn new(b: &Budget, q: &FindQuery) -> EditResult<Option<Finder>> {
        let scope = Scope::parse(&q.scope)?;
        let want = match scope {
            Scope::Description | Scope::Formula | Scope::Note | Scope::Account => {
                match Matcher::new(q) {
                    Some(m) => Want::Text(m),
                    None => return Ok(None),
                }
            }
            Scope::Global => {
                let name = q.text.trim();
                if name.is_empty() {
                    return Ok(None);
                }
                if !is_ident(name) {
                    return Err(format!(
                        "`{name}` isn't a global's name: names are letters, digits and _"
                    ));
                }
                Want::Token(name.to_string())
            }
            _ => {
                let name = q.text.trim();
                if name.is_empty() {
                    return Ok(None);
                }
                Want::Name(name.to_string())
            }
        };
        Ok(Some(Finder {
            scope,
            want,
            base: b.base_currency.clone(),
        }))
    }

    /// Whether field `field` of row `d` matches.
    fn row_matches(&self, d: &Detail, field: &str) -> bool {
        match (&self.want, self.scope) {
            (Want::Text(m), Scope::Description) => m.is_match(&d.description),
            (Want::Text(m), Scope::Note) => d.notes.iter().any(|n| m.is_match(&n.text)),
            (Want::Text(m), Scope::Formula) => formula(d, field).is_some_and(|f| m.is_match(&f.0)),
            (Want::Token(name), _) => {
                formula(d, field).is_some_and(|f| !global_tokens(&f.0, name).is_empty())
            }
            (Want::Name(n), Scope::Fringe) => d.fringes.contains(n),
            (Want::Name(n), Scope::Group) => d.groups.contains(n),
            (Want::Name(n), Scope::Set) => d.set.as_ref() == Some(n),
            (Want::Name(n), Scope::Location) => d.location.as_ref() == Some(n),
            (Want::Name(n), Scope::Unit) => unit(d, field) == Some(n.as_str()),
            (Want::Name(n), Scope::Currency) => match &d.currency {
                Some(c) => c == n,
                // A blank currency on a line is the base currency.
                None => d.kind == RowKind::Line && *n == self.base,
            },
            _ => false,
        }
    }

    fn account_matches(&self, code: &str, name: &str, field: &str) -> bool {
        let Want::Text(m) = &self.want else {
            return false;
        };
        match field {
            "code" => m.is_match(code),
            "name" => m.is_match(name),
            _ => false,
        }
    }
}

/// Everything in `b` that matches `q`, in budget order. An empty search
/// finds nothing; an unknown scope or a global name that can't be one is
/// refused.
pub fn search(b: &Budget, q: &FindQuery) -> EditResult<Found> {
    let mut found = Found {
        count: 0,
        hits: Vec::new(),
        truncated: false,
    };
    let Some(finder) = Finder::new(b, q)? else {
        return Ok(found);
    };
    let fields = finder.scope.fields();
    for (ti, row) in b.topsheet.iter().enumerate() {
        let TopsheetRow::Category(cat) = row else {
            continue;
        };
        for (ai, acct) in cat.accounts.iter().enumerate() {
            let hit = |row_id, line, description: &str, field, text: &str| Hit {
                ti,
                ai,
                category: cat.code.clone(),
                category_name: cat.name.clone(),
                account: acct.code.clone(),
                account_name: acct.name.clone(),
                row_id,
                line,
                description: cut(description),
                field,
                text: cut(text),
            };
            if finder.scope == Scope::Account {
                for &field in fields {
                    if finder.account_matches(&acct.code, &acct.name, field) {
                        found.count += 1;
                        if found.hits.len() < MAX_HITS {
                            let text = if field == "code" {
                                &acct.code
                            } else {
                                &acct.name
                            };
                            found.hits.push(hit(None, None, "", field, text));
                        }
                    }
                }
                continue;
            }
            for (i, d) in acct.details.iter().enumerate() {
                for &field in fields {
                    if !finder.row_matches(d, field) {
                        continue;
                    }
                    found.count += 1;
                    if found.hits.len() < MAX_HITS {
                        let text = field_text(d, field);
                        found
                            .hits
                            .push(hit(Some(d.id), Some(i + 1), &d.description, field, &text));
                    }
                }
            }
        }
    }
    found.truncated = found.count > found.hits.len();
    Ok(found)
}

// ---------------------------------------------------------------------------
// Replacing

/// What goes in place of what was found.
enum With {
    /// Text scopes: the replacement text (may be empty).
    Text(String),
    /// A global: the text that takes the token's place, and the same text
    /// for a formula that is only the token (no parentheses needed).
    Token { wrapped: String, bare: String },
    /// An item: another one's name, or `None` to take it off.
    Name(Option<String>),
}

/// A number as formula text, in parentheses when negative so it reads the
/// same next to an operator.
fn number_text(v: Dec) -> String {
    if v < Dec::ZERO {
        format!("({v})")
    } else {
        v.to_string()
    }
}

/// Global `name`'s current value, worked out from the globals alone.
fn global_value(b: &Budget, name: &str) -> EditResult<Dec> {
    let mut only = Budget::new(String::new(), b.base_currency.clone());
    only.globals = b.globals.clone();
    match fwb_core::delete_global(&mut only, name) {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Err(format!(
            "there is no global {name}, so it has no value to put in its place"
        )),
        Err(e) => Err(format!(
            "global {name} can't be worked out ({e}), so its value can't take its place"
        )),
    }
}

impl Finder {
    fn replacement(&self, b: &Budget, with: Option<&str>) -> EditResult<With> {
        match &self.want {
            Want::Text(_) => Ok(With::Text(with.unwrap_or_default().to_string())),
            Want::Token(name) => {
                let Some(w) = with.map(str::trim) else {
                    let v = global_value(b, name)?;
                    return Ok(With::Token {
                        wrapped: number_text(v),
                        bare: v.to_string(),
                    });
                };
                if w.is_empty() {
                    return Err("type a global, a number or a formula to put in its place".into());
                }
                if w == name {
                    return Err(format!("{name} would be replaced by itself"));
                }
                let e = Expr::parse(w).map_err(|e| format!("`{w}` isn't a formula: {e}"))?;
                let mut vars = BTreeSet::new();
                e.vars(&mut vars);
                if let Some(v) = vars.iter().find(|v| !b.globals.contains_key(*v)) {
                    return Err(format!(
                        "there is no global {v}; add it under Globals first"
                    ));
                }
                let simple = is_ident(w) || w.bytes().all(|c| c.is_ascii_digit() || c == b'.');
                Ok(With::Token {
                    wrapped: if simple {
                        w.to_string()
                    } else {
                        format!("({w})")
                    },
                    bare: w.to_string(),
                })
            }
            Want::Name(old) => {
                let Some(w) = with else {
                    return Ok(With::Name(None));
                };
                let w = w.trim();
                if w.is_empty() {
                    return Err(format!(
                        "pick the {} to put in its place, or remove it",
                        self.scope.what()
                    ));
                }
                if w == old {
                    return Err(format!("{old} would be replaced by itself"));
                }
                let known = match self.scope {
                    Scope::Fringe => b.fringes.contains_key(w),
                    Scope::Group => b.groups.contains_key(w),
                    Scope::Set => b.sets.contains_key(w),
                    Scope::Location => b.locations.contains_key(w),
                    Scope::Unit => b.units.contains_key(w),
                    Scope::Currency => w == b.base_currency || b.currencies.contains_key(w),
                    _ => false,
                };
                if !known {
                    return Err(format!(
                        "there is no {} `{w}`; add it in the tools panel first",
                        self.scope.what()
                    ));
                }
                Ok(With::Name(Some(w.to_string())))
            }
        }
    }

    /// Replaces in field `field` of `d`, which matches. Returns whether it
    /// changed.
    fn change(&self, d: &mut Detail, field: &str, with: &With) -> EditResult<bool> {
        let before = d.clone();
        match (&self.want, with) {
            (Want::Text(m), With::Text(w)) => match self.scope {
                Scope::Description => {
                    d.description = splice(&d.description, &m.ranges(&d.description), w);
                }
                Scope::Note => {
                    // In every note on the row; a note left blank goes.
                    for n in &mut d.notes {
                        let new = splice(&n.text, &m.ranges(&n.text), w);
                        let len = new.chars().count();
                        if len > MAX_NOTE_CHARS {
                            return Err(format!(
                                "a note would be {len} characters; notes can be at most {MAX_NOTE_CHARS}"
                            ));
                        }
                        n.text = new;
                    }
                    d.notes.retain(|n| !n.text.trim().is_empty());
                }
                _ => {
                    let f = formula_mut(d, field).ok_or("not a formula")?;
                    let new = splice(&f.0, &m.ranges(&f.0), w);
                    *f = Formula(new.trim().to_string());
                }
            },
            (Want::Token(name), With::Token { wrapped, bare }) => {
                let f = formula_mut(d, field).ok_or("not a formula")?;
                let new = if f.0.trim() == name {
                    bare.clone()
                } else {
                    splice(&f.0, &global_tokens(&f.0, name), wrapped)
                };
                *f = Formula(new);
            }
            (Want::Name(old), With::Name(new)) => {
                let new = new.as_deref();
                match self.scope {
                    Scope::Fringe | Scope::Group => {
                        let (names, takes) = if self.scope == Scope::Fringe {
                            (&mut d.fringes, d.kind == RowKind::Line)
                        } else {
                            (&mut d.groups, d.kind.is_costed())
                        };
                        names.remove(old);
                        if let Some(n) = new.filter(|_| takes) {
                            names.insert(n.to_string());
                        }
                    }
                    Scope::Set => d.set = new.map(String::from),
                    Scope::Location => d.location = new.map(String::from),
                    Scope::Unit => {
                        *unit_mut(d, field).ok_or("not a unit column")? =
                            new.unwrap_or_default().to_string();
                    }
                    Scope::Currency => {
                        d.currency = new.filter(|n| *n != self.base).map(String::from);
                    }
                    _ => return Err("nothing to replace there".into()),
                }
            }
            _ => return Err("nothing to replace there".into()),
        }
        Ok(*d != before)
    }
}

const STALE: &str =
    "some of the checked results no longer match (the budget changed); search again";

/// Replaces what `q` finds in each of `targets` (results of [`search`] with
/// the same query) by `with`. `with` is the replacement text, global, number,
/// formula or item name; `None` removes: a global's current value goes in its
/// place, an item is taken off (a currency: back to the base currency). Every
/// target must still match, and a new global or item must exist, or nothing
/// changes. Returns how many places changed.
pub fn replace_in(
    b: &mut Budget,
    q: &FindQuery,
    targets: &[Target],
    with: Option<&str>,
) -> EditResult<usize> {
    let finder = Finder::new(b, q)?.ok_or("type what to find first")?;
    if targets.is_empty() {
        return Err("check one or more results first".into());
    }
    let with = finder.replacement(b, with)?;
    let fields = finder.scope.fields();

    // Work out every change on copies first: nothing changes unless all can.
    let want: BTreeSet<RowId> = targets.iter().filter_map(|t| t.row_id).collect();
    let mut index: HashMap<RowId, &Detail> = HashMap::with_capacity(want.len());
    for d in details(b).filter(|d| want.contains(&d.id)) {
        index.entry(d.id).or_insert(d);
    }
    let mut rows: BTreeMap<RowId, Detail> = BTreeMap::new();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut updated = 0;
    for t in targets {
        if !seen.insert((t.row_id, t.account.as_deref(), t.field.as_str())) {
            continue;
        }
        if !fields.contains(&t.field.as_str()) {
            return Err(format!(
                "a {} search doesn't look in `{}`",
                finder.scope.what(),
                t.field
            ));
        }
        if finder.scope == Scope::Account {
            let code = t.account.as_deref().ok_or("which account?")?;
            if t.field == "code" {
                return Err(
                    "account numbers aren't replaced here; change them in the account list".into(),
                );
            }
            let acct = b
                .categories()
                .flat_map(|c| &c.accounts)
                .find(|a| a.code == code)
                .ok_or_else(|| format!("account {code} no longer exists"))?;
            let current = names.get(code).unwrap_or(&acct.name);
            if !finder.account_matches(&acct.code, current, "name") {
                return Err(STALE.into());
            }
            let (Want::Text(m), With::Text(w)) = (&finder.want, &with) else {
                return Err("nothing to replace there".into());
            };
            let new = splice(current, &m.ranges(current), w).trim().to_string();
            if new != *current {
                updated += 1;
                names.insert(code.to_string(), new);
            }
            continue;
        }
        let id = t.row_id.ok_or("which row?")?;
        let row = rows.entry(id);
        let copy = match row {
            std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::btree_map::Entry::Vacant(e) => {
                let orig = index.get(&id).ok_or_else(|| row_gone(id))?;
                e.insert((*orig).clone())
            }
        };
        if !finder.row_matches(copy, &t.field) {
            return Err(STALE.into());
        }
        if finder.change(copy, &t.field, &with)? {
            updated += 1;
        }
    }
    if updated == 0 {
        return Err("nothing would change: the replacement is what is there already".into());
    }

    // Then apply them.
    let ids: BTreeSet<RowId> = rows.keys().copied().collect();
    let mut slots = rows_by_id(b, &ids);
    for (id, d) in rows {
        if let Some(slot) = slots.get_mut(&id) {
            **slot = d;
        }
    }
    drop(slots);
    for acct in b.categories_mut().flat_map(|c| &mut c.accounts) {
        if let Some(name) = names.get(&acct.code) {
            acct.name.clone_from(name);
        }
    }
    Ok(updated)
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json and
// tools.json: the Find tab is part of the tools panel)

/// The places in budget `doc` that match `query`.
#[tauri::command]
pub fn find(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    query: FindQuery,
) -> Result<Found, String> {
    let s = state.lock_doc(doc)?;
    search(s.budget(), &query)
}

/// The view after a replace, and how many places changed.
#[derive(Clone, Debug, Serialize)]
pub struct Replaced {
    #[serde(flatten)]
    pub view: View,
    pub updated: usize,
}

/// Replaces in the checked results (`targets`) as one undo step; `with`
/// `null` removes. See [`replace_in`].
#[tauri::command]
pub fn replace<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    query: FindQuery,
    targets: Vec<Target>,
    with: Option<String>,
) -> Result<Replaced, String> {
    let mut updated = 0;
    let view = edit_budget(&window, &state, doc, |b| {
        updated = replace_in(b, &query, &targets, with.as_deref())?;
        Ok(())
    })?;
    Ok(Replaced { view, updated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit;
    use crate::session::Session;
    use fwb_core::sample::{self, SampleSpec};
    use fwb_core::{Account, Category, Currency, Fringe, FringeKind, Group, Location, SetDef};

    fn q(scope: &str, text: &str) -> FindQuery {
        FindQuery {
            scope: scope.into(),
            text: text.into(),
            match_case: false,
            whole_word: false,
        }
    }

    fn targets(found: &Found) -> Vec<Target> {
        found
            .hits
            .iter()
            .map(|h| Target {
                row_id: h.row_id,
                account: h.row_id.is_none().then(|| h.account.clone()),
                field: h.field.to_string(),
            })
            .collect()
    }

    fn row(b: &Budget, id: RowId) -> Detail {
        details(b).find(|d| d.id == id).cloned().unwrap()
    }

    /// Two categories with three accounts, globals with names inside each
    /// other's, and lines carrying every kind of tag.
    fn budget() -> Budget {
        let mut b = Budget::new("Find test", "USD");
        for (n, f) in [
            ("DAYS", "5"),
            ("SHOOT_DAYS", "20"),
            ("DAYS2", "7"),
            ("WEEKS", "4"),
            ("NEG", "0 - 3"),
        ] {
            b.globals.insert(n.into(), Formula::from(f));
        }
        for f in ["FICA", "PENSION"] {
            b.fringes.insert(
                f.into(),
                Fringe {
                    short_code: None,
                    description: String::new(),
                    rate: "5".into(),
                    kind: FringeKind::Percent { cutoff: None },
                },
            );
        }
        for g in ["Crew", "Cast"] {
            b.groups.insert(
                g.into(),
                Group {
                    short_code: None,
                    description: String::new(),
                    in_budget_total: true,
                    color: None,
                },
            );
        }
        for s in ["Stage A", "Stage B"] {
            b.sets.insert(s.into(), SetDef::default());
        }
        b.locations.insert("Rome".into(), Location::default());
        b.locations.insert("Paris".into(), Location::default());
        b.currencies.insert(
            "EUR".into(),
            Currency {
                description: String::new(),
                symbol: "€".into(),
                rate: "1.1".parse().unwrap(),
            },
        );

        let mut l1 = Detail::line(1, "Camera operator", "DAYS * 2", "Day", "500");
        l1.fringes = ["FICA".to_string()].into();
        l1.groups = ["Crew".to_string()].into();
        l1.set = Some("Stage A".into());
        l1.location = Some("Rome".into());
        l1.notes = vec![fwb_core::Note::plain(1, "Ask about the camera rental")];
        let mut l2 = Detail::line(2, "Camera assistant", "SHOOT_DAYS + DAYS2", "Day", "300");
        l2.fringes = ["FICA".to_string(), "PENSION".to_string()].into();
        l2.currency = Some("EUR".into());
        l2.unit2 = "Day".into();
        let mut l3 = Detail::line(3, "Cameraman's kit", "(DAYS+1)/DAYS", "Week", "WEEKS * 10");
        l3.groups = ["Cast".to_string()].into();
        let sub = Detail::subtotal(4, "Camera subtotal");
        let mut l5 = Detail::line(5, "Gaffer", "10", "Day", "DAYS");
        l5.fringes = ["FICA".to_string()].into();
        let adj = Detail::adjustment(6, "Contingency camera", "0.1");

        b.topsheet = vec![
            TopsheetRow::Category(Category {
                notes: Vec::new(),
                code: "20".into(),
                name: "Production".into(),
                accounts: vec![
                    Account {
                        notes: Vec::new(),
                        code: "20-01".into(),
                        name: "Camera".into(),
                        details: vec![l1, l2, l3, sub],
                    },
                    Account {
                        notes: Vec::new(),
                        code: "20-02".into(),
                        name: "Electric".into(),
                        details: vec![l5, adj],
                    },
                ],
            }),
            TopsheetRow::ProductionTotal(fwb_core::ProductionTotal {
                name: "Total".into(),
            }),
            TopsheetRow::Category(Category {
                notes: Vec::new(),
                code: "30".into(),
                name: "Post".into(),
                accounts: vec![Account {
                    notes: Vec::new(),
                    code: "30-01".into(),
                    name: "Camera tests".into(),
                    details: Vec::new(),
                }],
            }),
        ];
        b
    }

    fn places(found: &Found) -> Vec<(Option<RowId>, &str)> {
        found.hits.iter().map(|h| (h.row_id, h.field)).collect()
    }

    #[test]
    fn text_search_with_match_case_and_whole_word() {
        let b = budget();
        let f = search(&b, &q("description", "camera")).unwrap();
        assert_eq!(f.count, 5);
        assert_eq!(
            places(&f),
            [
                (Some(1), "description"),
                (Some(2), "description"),
                (Some(3), "description"),
                (Some(4), "description"),
                (Some(6), "description"),
            ]
        );
        let h = &f.hits[0];
        assert_eq!((h.ti, h.ai, h.line), (0, 0, Some(1)));
        assert_eq!((h.category.as_str(), h.account.as_str()), ("20", "20-01"));
        assert_eq!(h.account_name, "Camera");
        assert_eq!(f.hits[4].ai, 1);
        assert_eq!(f.hits[4].line, Some(2));

        let mut cased = q("description", "camera");
        cased.match_case = true;
        assert_eq!(
            search(&b, &cased).unwrap().count,
            1,
            "only `Contingency camera`"
        );

        let mut whole = q("description", "camera");
        whole.whole_word = true;
        // "Cameraman's" runs on into letters; the rest are whole words.
        assert_eq!(search(&b, &whole).unwrap().count, 4);

        // Accounts by name or number; notes.
        let a = search(&b, &q("account", "camera")).unwrap();
        assert_eq!(a.count, 2);
        assert!(a
            .hits
            .iter()
            .all(|h| h.row_id.is_none() && h.field == "name"));
        assert_eq!(search(&b, &q("account", "20-0")).unwrap().count, 2);
        assert_eq!(
            places(&search(&b, &q("note", "RENTAL")).unwrap()),
            [(Some(1), "note")]
        );

        // Nothing typed finds nothing; a bad scope is refused.
        assert_eq!(search(&b, &q("description", "")).unwrap().count, 0);
        assert!(search(&b, &q("nonsense", "x")).is_err());
    }

    #[test]
    fn case_folding_and_whole_words_beyond_ascii() {
        let m = Matcher::new(&q("description", "ÉTÉ")).unwrap();
        assert_eq!(m.ranges("un été chaud, l'été"), [(3, 8), (18, 23)]);
        let mut wq = q("description", "a");
        wq.whole_word = true;
        let w = Matcher::new(&wq).unwrap();
        assert_eq!(w.ranges("a ab a_ (a)"), [(0, 1), (9, 10)]);
        // Overlaps are not counted twice, and replacing keeps the rest.
        let o = Matcher::new(&q("description", "aa")).unwrap();
        assert_eq!(o.ranges("aaaa"), [(0, 2), (2, 4)]);
        assert_eq!(splice("one two one", &[(0, 3), (8, 11)], "1"), "1 two 1");
    }

    #[test]
    fn globals_match_as_whole_tokens_only() {
        assert_eq!(global_tokens("DAYS * 2", "DAYS"), [(0, 4)]);
        assert!(global_tokens("SHOOT_DAYS + DAYS2", "DAYS").is_empty());
        assert_eq!(global_tokens("(DAYS+1)/DAYS", "DAYS"), [(1, 5), (9, 13)]);
        // A number running into letters lexes as a number then a name.
        assert_eq!(global_tokens("2DAYS", "DAYS"), [(1, 5)]);
        assert!(global_tokens("1.5", "DAYS").is_empty());

        let b = budget();
        let f = search(&b, &q("global", "DAYS")).unwrap();
        assert_eq!(
            places(&f),
            [(Some(1), "amount"), (Some(3), "amount"), (Some(5), "rate")]
        );
        assert_eq!(f.hits[1].text, "(DAYS+1)/DAYS");
        assert!(search(&b, &q("global", "NOT A NAME")).is_err());

        // Plain formula text does match inside other names.
        let t = search(&b, &q("formula", "DAYS")).unwrap();
        assert_eq!(t.count, 4);
    }

    #[test]
    fn replacing_a_global_touches_only_its_tokens() {
        let mut b = budget();
        let query = q("global", "DAYS");
        let found = search(&b, &query).unwrap();
        let n = replace_in(&mut b, &query, &targets(&found), Some("WEEKS")).unwrap();
        assert_eq!(n, 3);
        assert_eq!(row(&b, 1).amount.0, "WEEKS * 2");
        assert_eq!(row(&b, 2).amount.0, "SHOOT_DAYS + DAYS2");
        assert_eq!(row(&b, 3).amount.0, "(WEEKS+1)/WEEKS");
        assert_eq!(row(&b, 5).rate.0, "WEEKS");
        assert_eq!(
            search(&b, &query).unwrap().count,
            0,
            "replaced places drop out"
        );

        // A formula goes in parentheses, a number as it is.
        let mut b = budget();
        let found = search(&b, &query).unwrap();
        replace_in(&mut b, &query, &targets(&found)[..1], Some("WEEKS * 5")).unwrap();
        assert_eq!(row(&b, 1).amount.0, "(WEEKS * 5) * 2");
        replace_in(&mut b, &query, &targets(&found)[1..2], Some("7")).unwrap();
        assert_eq!(row(&b, 3).amount.0, "(7+1)/7");
        // Removing puts its current value in, like deleting a global.
        replace_in(&mut b, &query, &targets(&found)[2..], None).unwrap();
        assert_eq!(row(&b, 5).rate.0, "5");
        // A cell that is only the global takes a formula without parentheses.
        let mut b = budget();
        let found = search(&b, &query).unwrap();
        replace_in(&mut b, &query, &targets(&found)[2..], Some("WEEKS * 5")).unwrap();
        assert_eq!(row(&b, 5).rate.0, "WEEKS * 5");
        let neg = q("global", "NEG");
        let mut b2 = budget();
        row_formula(&mut b2, 5, "rate", "NEG * 2");
        let found = search(&b2, &neg).unwrap();
        replace_in(&mut b2, &neg, &targets(&found), None).unwrap();
        assert_eq!(row(&b2, 5).rate.0, "(-3) * 2");
        let mut b3 = budget();
        row_formula(&mut b3, 5, "rate", " NEG ");
        let found = search(&b3, &neg).unwrap();
        replace_in(&mut b3, &neg, &targets(&found), None).unwrap();
        assert_eq!(row(&b3, 5).rate.0, "-3");

        // Refused, with nothing changed: an unknown global, a bad formula, itself.
        let b = budget();
        for with in ["NOPE", "WEEKS * NOPE", "WEEKS *", "DAYS", " "] {
            let mut c = b.clone();
            let found = search(&c, &query).unwrap();
            assert!(
                replace_in(&mut c, &query, &targets(&found), Some(with)).is_err(),
                "{with}"
            );
            assert_eq!(c, b);
        }
    }

    fn row_formula(b: &mut Budget, id: RowId, field: &str, f: &str) {
        let d = b
            .categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
            .find(|d| d.id == id)
            .unwrap();
        *formula_mut(d, field).unwrap() = Formula::from(f);
    }

    #[test]
    fn items_are_swapped_or_taken_off() {
        let mut b = budget();
        // A fringe for another, on the checked lines only.
        let fq = q("fringe", "FICA");
        let found = search(&b, &fq).unwrap();
        assert_eq!(
            places(&found),
            [
                (Some(1), "fringes"),
                (Some(2), "fringes"),
                (Some(5), "fringes")
            ]
        );
        assert_eq!(found.hits[1].text, "FICA, PENSION");
        let n = replace_in(&mut b, &fq, &targets(&found)[..2], Some("PENSION")).unwrap();
        assert_eq!(n, 2);
        assert_eq!(joined(&row(&b, 1).fringes), "PENSION");
        assert_eq!(joined(&row(&b, 2).fringes), "PENSION");
        assert_eq!(joined(&row(&b, 5).fringes), "FICA", "unchecked: left alone");
        // Removing.
        let found = search(&b, &fq).unwrap();
        replace_in(&mut b, &fq, &targets(&found), None).unwrap();
        assert!(row(&b, 5).fringes.is_empty());

        let gq = q("group", "Crew");
        let found = search(&b, &gq).unwrap();
        replace_in(&mut b, &gq, &targets(&found), Some("Cast")).unwrap();
        assert_eq!(joined(&row(&b, 1).groups), "Cast");

        let sq = q("set", "Stage A");
        let found = search(&b, &sq).unwrap();
        replace_in(&mut b, &sq, &targets(&found), Some("Stage B")).unwrap();
        assert_eq!(row(&b, 1).set.as_deref(), Some("Stage B"));

        let lq = q("location", "Rome");
        let found = search(&b, &lq).unwrap();
        replace_in(&mut b, &lq, &targets(&found), None).unwrap();
        assert_eq!(row(&b, 1).location, None);

        // Units: any of the four columns, each its own place.
        let uq = q("unit", "Day");
        let found = search(&b, &uq).unwrap();
        assert_eq!(
            places(&found),
            [
                (Some(1), "unit"),
                (Some(2), "unit"),
                (Some(2), "unit2"),
                (Some(5), "unit")
            ]
        );
        replace_in(&mut b, &uq, &targets(&found)[1..3], Some("Week")).unwrap();
        assert_eq!(
            (row(&b, 2).unit.as_str(), row(&b, 2).unit2.as_str()),
            ("Week", "Week")
        );
        assert_eq!(row(&b, 1).unit, "Day");

        // Currencies: the base currency finds the blank lines (not other rows).
        let base = search(&b, &q("currency", "USD")).unwrap();
        assert_eq!(
            base.count, 3,
            "lines 1, 3 and 5; not the subtotal or the adjustment"
        );
        let cq = q("currency", "EUR");
        let found = search(&b, &cq).unwrap();
        replace_in(&mut b, &cq, &targets(&found), Some("USD")).unwrap();
        assert_eq!(
            row(&b, 2).currency,
            None,
            "the base currency is stored as blank"
        );
        let found = search(&b, &q("currency", "USD")).unwrap();
        replace_in(
            &mut b,
            &q("currency", "USD"),
            &targets(&found)[..1],
            Some("EUR"),
        )
        .unwrap();
        assert_eq!(row(&b, 1).currency.as_deref(), Some("EUR"));

        // An item that doesn't exist, or the same one, is refused.
        let before = b.clone();
        let found = search(&b, &q("group", "Cast")).unwrap();
        for with in ["Nope", "Cast", ""] {
            assert!(replace_in(&mut b, &q("group", "Cast"), &targets(&found), Some(with)).is_err());
        }
        assert_eq!(b, before);
    }

    #[test]
    fn text_is_replaced_inside_fields() {
        let mut b = budget();
        let mut dq = q("description", "camera");
        dq.whole_word = true;
        let found = search(&b, &dq).unwrap();
        replace_in(&mut b, &dq, &targets(&found), Some("Lens")).unwrap();
        assert_eq!(row(&b, 1).description, "Lens operator");
        assert_eq!(row(&b, 3).description, "Cameraman's kit");
        assert_eq!(row(&b, 6).description, "Contingency Lens");

        let fq = q("formula", "days");
        let found = search(&b, &fq).unwrap();
        replace_in(&mut b, &fq, &targets(&found)[..1], Some("WEEKS")).unwrap();
        assert_eq!(row(&b, 1).amount.0, "WEEKS * 2");

        // An emptied note goes away.
        let nq = q("note", "Ask about the camera rental");
        let found = search(&b, &nq).unwrap();
        replace_in(&mut b, &nq, &targets(&found), Some("")).unwrap();
        assert!(row(&b, 1).notes.is_empty());

        // Account names, not numbers.
        let aq = q("account", "Camera");
        let found = search(&b, &aq).unwrap();
        replace_in(&mut b, &aq, &targets(&found), Some("Lens")).unwrap();
        let names: Vec<&str> = b
            .categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(names, ["Lens", "Electric", "Lens tests"]);
        let cq = q("account", "20-01");
        let found = search(&b, &cq).unwrap();
        assert!(replace_in(&mut b, &cq, &targets(&found), Some("x")).is_err());
    }

    #[test]
    fn a_replace_is_one_undo_step() {
        let mut s = Session::with_budget(budget(), None);
        let original = s.budget().clone();
        let query = q("description", "camera");
        let found = search(s.budget(), &query).unwrap();
        let n = s
            .edit(|b| replace_in(b, &query, &targets(&found), Some("Lens")))
            .unwrap();
        assert_eq!(n, 5);
        assert!(s.can_undo());
        assert_ne!(s.budget(), &original);
        assert!(s.undo());
        assert_eq!(s.budget(), &original, "one undo brings every place back");
        assert!(!s.can_undo());
        assert!(s.redo());
        assert_eq!(search(s.budget(), &query).unwrap().count, 0);
    }

    #[test]
    fn a_stale_or_bad_target_changes_nothing() {
        let mut s = Session::with_budget(budget(), None);
        let query = q("fringe", "FICA");
        let found = search(s.budget(), &query).unwrap();
        // Line 5 loses FICA after the search.
        s.edit(|b| edit::set_detail_field(b, 5, edit::Field::Fringes, ""))
            .unwrap();
        let before = s.budget().clone();
        let undo_depth = s.can_undo();
        let err = s
            .edit(|b| replace_in(b, &query, &targets(&found), Some("PENSION")))
            .unwrap_err();
        assert!(err.contains("no longer match"), "{err}");
        assert_eq!(s.budget(), &before);
        assert_eq!(s.can_undo(), undo_depth);

        // Called directly, it refuses before touching anything too.
        let mut b = before.clone();
        assert!(replace_in(&mut b, &query, &targets(&found), Some("PENSION")).is_err());
        assert_eq!(b, before);

        // A row that is gone, a field the scope doesn't look in, no targets.
        let gone = vec![
            targets(&found)[0].clone(),
            Target {
                row_id: Some(999),
                account: None,
                field: "fringes".into(),
            },
        ];
        assert!(replace_in(&mut b, &query, &gone, Some("PENSION"))
            .unwrap_err()
            .contains("no longer exists"));
        let wrong = vec![Target {
            row_id: Some(1),
            account: None,
            field: "groups".into(),
        }];
        assert!(replace_in(&mut b, &query, &wrong, Some("PENSION")).is_err());
        assert!(replace_in(&mut b, &query, &[], Some("PENSION")).is_err());
        assert_eq!(b, before);
    }

    #[test]
    fn results_are_capped_but_counted() {
        let b = sample::budget(&SampleSpec::default());
        let f = search(&b, &q("fringe", "FR00")).unwrap();
        assert!(f.count > MAX_HITS, "{}", f.count);
        assert_eq!(f.hits.len(), MAX_HITS);
        assert!(f.truncated);
        // Budget order.
        let order: Vec<(usize, usize, Option<usize>)> =
            f.hits.iter().map(|h| (h.ti, h.ai, h.line)).collect();
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(order, sorted);
    }

    /// Searching a 5,000-row budget stays quick (well under 20 ms in a
    /// release build; the bound is loose so a debug build on a slow machine
    /// passes too).
    #[test]
    fn searching_a_big_budget_is_quick() {
        let b = sample::budget(&SampleSpec::default());
        assert!(details(&b).count() >= 5_000);
        let mut cased = q("description", "Operator");
        cased.match_case = true;
        let mut whole = q("description", "kit");
        whole.whole_word = true;
        let queries = [
            q("description", "operator"),
            cased,
            whole,
            q("formula", "weeks"),
            q("global", "SHOOT_DAYS"),
            q("fringe", "FR01"),
            q("group", "G03"),
            q("set", "Set 04"),
            q("location", "Location 2"),
            q("unit", "Day"),
            q("currency", "USD"),
            q("note", "x"),
            q("account", "camera"),
        ];
        for query in &queries {
            // The best of a few runs, so a busy machine doesn't fail it.
            let best = (0..5)
                .map(|_| {
                    let t = std::time::Instant::now();
                    let found = search(&b, query).unwrap();
                    std::hint::black_box(found);
                    t.elapsed()
                })
                .min()
                .unwrap();
            assert!(
                best < std::time::Duration::from_millis(250),
                "{} search took {best:?}",
                query.scope
            );
        }
        assert!(search(&b, &q("global", "SHOOT_DAYS")).unwrap().count > 0);
    }

    #[test]
    fn a_replace_on_the_big_budget_recalculates_cleanly() {
        let mut s = Session::with_budget(sample::budget(&SampleSpec::default()), None);
        let query = q("global", "SHOOT_DAYS");
        let found = search(s.budget(), &query).unwrap();
        assert!(found.count > 0);
        let n = s
            .edit(|b| replace_in(b, &query, &targets(&found), Some("SHOOT_WEEKS * 5")))
            .unwrap();
        assert_eq!(n, found.hits.len());
        let v = serde_json::to_value(s.view_with(None)).unwrap();
        assert!(v["error"].is_null(), "{}", v["error"]);
    }

    #[test]
    fn the_replace_reply_is_a_view_with_a_count() {
        let mut s = Session::with_budget(budget(), None);
        let out = Replaced {
            view: s.view_with(None),
            updated: 3,
        };
        let v = serde_json::to_value(out).unwrap();
        assert!(v.get("budget").is_some(), "the page redraws from it");
        assert_eq!(v["updated"], 3);
    }

    const APP_RS: &str = include_str!("app.rs");
    const BUILD_RS: &str = include_str!("../build.rs");
    const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
    const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");

    #[test]
    fn find_commands_are_wired_for_both_windows() {
        for c in ["find", "replace"] {
            assert!(
                APP_RS.contains(&format!("crate::find::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{c}\"");
            assert!(
                MAIN_CAPS.contains(&allow),
                "{c} not allowed for the main window"
            );
            assert!(
                TOOLS_CAPS.contains(&allow),
                "{c} not allowed for the tools window"
            );
        }
    }
}
