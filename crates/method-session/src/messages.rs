//! Engine messages with stable codes (decisions.md, entry 94, fifth pass;
//! docs/APP.md, "Languages").
//!
//! A message the engine gives a person (an edit refused, a value that
//! doesn't read) is made at its source from a code and its parameters with
//! [`msg`]: `msg("engine.edit.account_gone", &[("code", &code)])`. The
//! English for each code is in [`MESSAGES`], so the command line, the MCP
//! server and the browser build show the same English as before: the error
//! is still that text, and nothing here reads a table.
//!
//! The desktop app shows the words in the person's language: it hands the
//! text to [`recognize`], which gives back the code and the parameters the
//! English was made from, and looks the code up in `ui/i18n/en.js` (whose
//! entry for each code is this English, tested), falling back to the
//! English. A parameter named in [`NESTED`] is a message itself (a noun
//! such as `engine.what.fringe`, or a reason) and is recognized in turn.
//!
//! Codes are `engine.<area>.<name>`, letters, digits, dots and underscores;
//! parameters are `{name}`. A message about a count is plural: an English
//! form for one and one for more, picked by its `count` parameter.

use std::fmt::Display;

/// One message: its code and its English, with a second form when it
/// counts something (`one` for a count of 1, `other` for the rest).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    pub code: &'static str,
    /// The English for a count of 1, when the message counts something.
    pub one: Option<&'static str>,
    /// The English (for a count other than 1, when it counts something).
    pub other: &'static str,
}

const fn m(code: &'static str, english: &'static str) -> Message {
    Message {
        code,
        one: None,
        other: english,
    }
}

const fn plural(code: &'static str, one: &'static str, other: &'static str) -> Message {
    Message {
        code,
        one: Some(one),
        other,
    }
}

/// Parameters whose value is a message too: a noun the engine names
/// (`engine.what.*`) or a reason given after a colon. Others are data.
pub const NESTED: &[&str] = &["what", "why"];

/// Every engine message, by code (sorted, tested).
pub const MESSAGES: &[Message] = &[
    m(
        "engine.edit.account_exists",
        "account {code} already exists",
    ),
    m(
        "engine.edit.account_gone",
        "account {code} no longer exists",
    ),
    m(
        "engine.edit.account_not_in_category",
        "account {code} is not in category {category}",
    ),
    m(
        "engine.edit.account_number_empty",
        "account number is empty",
    ),
    m("engine.edit.amount_sign_after", "put the sign before the $"),
    m(
        "engine.edit.amount_sign_twice",
        "use a minus sign or parentheses, not both",
    ),
    m("engine.edit.base_currency", "{code} is the base currency"),
    m(
        "engine.edit.base_currency_listed",
        "{code} is in the currency list; delete it there first",
    ),
    m("engine.edit.cant_be_negative", "{what} can't be negative"),
    m("engine.edit.cant_move", "can't move the {what} there"),
    m(
        "engine.edit.category_exists",
        "category {code} already exists",
    ),
    m(
        "engine.edit.category_gone",
        "category {code} no longer exists",
    ),
    m(
        "engine.edit.category_number_empty",
        "category number is empty",
    ),
    m(
        "engine.edit.charge_exists",
        "there is already a charge named {name}",
    ),
    m(
        "engine.edit.charge_in_currency",
        "a charge is in {code}; change it first",
    ),
    m("engine.edit.charge_name_empty", "charge name is empty"),
    m(
        "engine.edit.credit_exists",
        "there is already a credit named {name}",
    ),
    m("engine.edit.credit_name_empty", "credit name is empty"),
    m(
        "engine.edit.credit_no_account",
        "there is no account `{code}`",
    ),
    m(
        "engine.edit.credit_no_category",
        "there is no category `{code}`",
    ),
    m("engine.edit.credit_no_group", "there is no group `{name}`"),
    m(
        "engine.edit.currency_gone",
        "currency {code} no longer exists",
    ),
    m(
        "engine.edit.discount_over_100",
        "the discount can't be more than 100%",
    ),
    m(
        "engine.edit.exchange_rate_zero",
        "an exchange rate must be more than 0",
    ),
    m("engine.edit.field_gone", "field {name} no longer exists"),
    m("engine.edit.field_needs_label", "the field needs a label"),
    m(
        "engine.edit.flat_charge_needs_amount",
        "a flat charge needs an amount",
    ),
    m(
        "engine.edit.formula_refused",
        "{what} can't be {value}: {why}",
    ),
    m("engine.edit.fringe_gone", "fringe {name} no longer exists"),
    m(
        "engine.edit.fringe_row_account_char",
        "the fringe row's account number can only use letters, digits and separators, not {char}",
    ),
    m(
        "engine.edit.fringe_row_description_long",
        "the fringe row's description is longer than {max} characters",
    ),
    m(
        "engine.edit.fringe_suffix_char",
        "the suffix can only use letters and digits, not {char}",
    ),
    m(
        "engine.edit.fringe_suffix_empty",
        "enter the fringe account suffix, like 99",
    ),
    m("engine.edit.global_gone", "global {name} no longer exists"),
    m(
        "engine.edit.global_name",
        "`{name}` can't be a global name: use letters, digits and _, with at least one letter or _",
    ),
    m(
        "engine.edit.global_needs_value",
        "global {name} needs a value",
    ),
    m(
        "engine.edit.global_refused",
        "global {name} can't be {formula}: {why}",
    ),
    m("engine.edit.group_gone", "group {name} no longer exists"),
    m(
        "engine.edit.group_used_by_credit",
        "group {name} is used by the credit {credit}; change the credit first",
    ),
    m("engine.edit.hours_negative", "hours can't be negative"),
    m("engine.edit.item_gone", "{what} {name} no longer exists"),
    plural(
        "engine.edit.lines_use_currency",
        "{count} line uses {code}; change them first",
        "{count} lines use {code}; change them first",
    ),
    m(
        "engine.edit.location_gone",
        "location {name} no longer exists",
    ),
    m(
        "engine.edit.lock_needs_fix",
        "fix this first, then lock the totals: {why}",
    ),
    m(
        "engine.edit.max_hours_zero",
        "max hours per day must be more than 0",
    ),
    m(
        "engine.edit.name_comma",
        "{what} names can't contain commas",
    ),
    m("engine.edit.name_empty", "{what} name is empty"),
    m("engine.edit.needs_value", "{what} needs a value"),
    m("engine.edit.negative_cutoff", "a cutoff can't be negative"),
    m(
        "engine.edit.no_baseline_to_clear",
        "there are no original totals to clear",
    ),
    m(
        "engine.edit.no_baseline_to_freeze",
        "there are no original totals to freeze",
    ),
    m(
        "engine.edit.no_exchange_rate",
        "there is no exchange rate for {code}; add it first",
    ),
    m(
        "engine.edit.no_exchange_rate_listed",
        "there is no exchange rate for {code}; add it under Currencies first",
    ),
    m(
        "engine.edit.no_row_uses_column",
        "none of the selected rows use this column",
    ),
    m("engine.edit.no_short_code", "a {what} has no short code"),
    m("engine.edit.no_such", "there is no {what} {name}"),
    m(
        "engine.edit.not_a_date",
        "`{text}` is not a date; write it as YYYY-MM-DD, like 2027-03-01",
    ),
    m(
        "engine.edit.not_a_number",
        "{what} `{text}` is not a number: {why}",
    ),
    m(
        "engine.edit.not_an_amount",
        "`{text}` is not an amount: {why}",
    ),
    m(
        "engine.edit.not_binary_exact",
        "`{value}` is not binary or exact",
    ),
    m(
        "engine.edit.not_cutoff_factor",
        "`{value}` is not a cutoff factor: use 1.0, 1.5, 2.0, 2.5 or 3.0",
    ),
    m(
        "engine.edit.not_group_rule",
        "`{value}` is not a group rule",
    ),
    m(
        "engine.edit.not_include_exclude",
        "`{value}` is not include or exclude",
    ),
    m("engine.edit.not_keep_drop", "`{value}` is not keep or drop"),
    m(
        "engine.edit.past_cents",
        "`{text}` has more than two decimal places",
    ),
    m(
        "engine.edit.production_total_name_empty",
        "production total name is empty",
    ),
    m("engine.edit.row_gone", "row {id} no longer exists"),
    m(
        "engine.edit.row_needs_account",
        "row {row} needs an account",
    ),
    m(
        "engine.edit.row_needs_category",
        "row {row} needs a category",
    ),
    m("engine.edit.row_needs_group", "row {row} needs a group"),
    m(
        "engine.edit.row_not_in_account",
        "row {id} is not in account {code}",
    ),
    m(
        "engine.edit.rows_dont_use_column",
        "{what} don't use this column",
    ),
    m("engine.edit.rows_gone", "those rows no longer exist"),
    m(
        "engine.edit.separator_one_char",
        "the separator is one character, or none",
    ),
    m(
        "engine.edit.separators",
        "separators should be 1,234.56 or 1.234,56, not `{text}`",
    ),
    m("engine.edit.set_gone", "set {name} no longer exists"),
    m(
        "engine.edit.shared_cap_needs_amount",
        "the shared cap {name} is new, so it needs an amount",
    ),
    m(
        "engine.edit.shared_cap_needs_name",
        "a shared cap amount needs a shared cap name",
    ),
    m(
        "engine.edit.short_code_comma",
        "short codes can't contain commas",
    ),
    m(
        "engine.edit.short_code_is_name",
        "{code} is the name of another {what}",
    ),
    m(
        "engine.edit.short_code_long",
        "that short code is {count} characters; short codes can be at most {max}",
    ),
    m(
        "engine.edit.short_code_taken",
        "{code} is already the short code of the {what} {name}",
    ),
    m("engine.edit.that_item_gone", "that {what} no longer exists"),
    m("engine.edit.too_large", "`{text}` is too large"),
    m(
        "engine.edit.too_many_cells",
        "that's {count} cells; at most {max} can change at once",
    ),
    m(
        "engine.edit.topsheet_row_gone",
        "that topsheet row no longer exists",
    ),
    m(
        "engine.edit.totals_frozen",
        "the original totals are frozen; unfreeze them first",
    ),
    m(
        "engine.edit.totals_in_currency",
        "totals are shown in {code}; change that first",
    ),
    m(
        "engine.edit.totals_too_large",
        "the totals are too large to keep",
    ),
    m("engine.edit.unit_gone", "unit {name} no longer exists"),
    m(
        "engine.edit.unknown_charge_kind",
        "unknown charge kind `{kind}`",
    ),
    m("engine.edit.unknown_field", "unknown field `{field}`"),
    m(
        "engine.edit.unknown_field_kind",
        "unknown field kind `{kind}`",
    ),
    m(
        "engine.edit.unknown_flat_basis",
        "unknown flat fringe basis `{basis}`",
    ),
    m(
        "engine.edit.unknown_fringe_kind",
        "unknown fringe kind `{kind}`",
    ),
    m(
        "engine.edit.unknown_fringe_posting",
        "unknown fringe posting `{mode}`",
    ),
    m("engine.edit.unknown_row_kind", "unknown row kind `{kind}`"),
    m("engine.what.account", "account"),
    m("engine.what.adjustment_rows", "adjustment rows"),
    m("engine.what.amt", "Amt"),
    m("engine.what.an_excluded_amount", "an excluded amount"),
    m("engine.what.category", "category"),
    m("engine.what.charge", "charge"),
    m("engine.what.credit", "credit"),
    m("engine.what.currency", "currency"),
    m("engine.what.fringe", "fringe"),
    m("engine.what.global", "global"),
    m("engine.what.group", "group"),
    m("engine.what.hours", "hours"),
    m("engine.what.lines", "lines"),
    m("engine.what.location", "location"),
    m("engine.what.max_hours_per_day", "max hours per day"),
    m("engine.what.rate", "rate"),
    m("engine.what.rate_column", "Rate"),
    m("engine.what.row_amount", "row {row}'s amount"),
    m("engine.what.row_cap", "row {row}'s cap"),
    m("engine.what.row_percent", "row {row}'s percent"),
    m("engine.what.separator_rows", "separator rows"),
    m("engine.what.set", "set"),
    m("engine.what.subtotal_rows", "subtotal rows"),
    m("engine.what.the_amount", "the amount"),
    m("engine.what.the_brokers_fee", "the broker's fee"),
    m("engine.what.the_cap", "the cap"),
    m("engine.what.the_discount", "the discount"),
    m("engine.what.the_minimum_spend", "the minimum spend"),
    m("engine.what.the_percent", "the percent"),
    m("engine.what.the_shared_cap", "the shared cap"),
    m("engine.what.this_cell", "This cell"),
    m("engine.what.unit", "unit"),
    m("engine.what.x", "X"),
    m("engine.what.x4", "4X"),
];

/// The message with this code.
pub fn find(code: &str) -> Option<&'static Message> {
    MESSAGES
        .binary_search_by(|m| m.code.cmp(code))
        .ok()
        .map(|i| &MESSAGES[i])
}

/// `code`'s English with `params` filled in: the form for one when its
/// `count` is 1. An unknown code gives the code itself (the tests keep
/// that from happening).
pub fn msg(code: &str, params: &[(&str, &dyn Display)]) -> String {
    let Some(m) = find(code) else {
        debug_assert!(false, "{code} is not in method_session::messages::MESSAGES");
        return code.to_string();
    };
    let params: Vec<(&str, String)> = params.iter().map(|(n, v)| (*n, v.to_string())).collect();
    let count_is_one = params.iter().any(|(n, v)| *n == "count" && v == "1");
    let english = match m.one {
        Some(one) if count_is_one => one,
        _ => m.other,
    };
    fill(english, &params)
}

/// A noun or other message with no parameters, as text.
pub fn word(code: &str) -> String {
    msg(code, &[])
}

/// `template` with each `{name}` replaced by its value, in one pass (a
/// value that holds `{other}` stays as it is). A name with no value stays
/// written as `{name}`.
pub fn fill<V: AsRef<str>>(template: &str, params: &[(&str, V)]) -> String {
    let mut out = String::with_capacity(template.len());
    for piece in pieces(template) {
        match piece {
            Piece::Text(t) => out.push_str(t),
            Piece::Param(name) => match params.iter().find(|(n, _)| *n == name) {
                Some((_, v)) => out.push_str(v.as_ref()),
                None => {
                    out.push('{');
                    out.push_str(name);
                    out.push('}');
                }
            },
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Piece<'a> {
    Text(&'a str),
    Param(&'a str),
}

/// A template's text and `{name}` parameters, in order. A brace that
/// doesn't open a parameter name is text.
fn pieces(template: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut text_from = 0;
    let mut i = 0;
    let bytes = template.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let rest = &template[i + 1..];
            if let Some(end) = rest.find('}') {
                let name = &rest[..end];
                if !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                {
                    if text_from < i {
                        out.push(Piece::Text(&template[text_from..i]));
                    }
                    out.push(Piece::Param(name));
                    i += end + 2;
                    text_from = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    if text_from < template.len() {
        out.push(Piece::Text(&template[text_from..]));
    }
    out
}

/// What [`recognize`] found: the code a message was made from and its
/// parameters' values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recognized {
    pub code: &'static str,
    pub params: Vec<(&'static str, String)>,
}

/// The code and parameters `text` was made from by [`msg`], when it is one
/// of the engine's messages: the message whose English it matches, the one
/// with the most fixed words when several do (`a cutoff can't be negative`
/// over `{what} can't be negative`). Filling the code's English with the
/// parameters gives `text` back exactly.
pub fn recognize(text: &str) -> Option<Recognized> {
    let mut best: Option<(usize, Recognized)> = None;
    for m in MESSAGES {
        for english in m.one.into_iter().chain([m.other]) {
            let pieces = pieces(english);
            let fixed: usize = pieces
                .iter()
                .map(|p| match p {
                    Piece::Text(t) => t.len(),
                    Piece::Param(_) => 0,
                })
                .sum();
            if best.as_ref().is_some_and(|(f, _)| *f >= fixed) {
                continue;
            }
            let mut params = Vec::new();
            if match_pieces(&pieces, text, &mut params) {
                let params = params
                    .into_iter()
                    .map(|(n, v)| (n, v.to_string()))
                    .collect();
                best = Some((
                    fixed,
                    Recognized {
                        code: m.code,
                        params,
                    },
                ));
            }
        }
    }
    best.map(|(_, r)| r)
}

/// Whether `text` is `pieces` with some value for each parameter, trying
/// every place the text after a parameter could start. The values found
/// are pushed onto `params`.
fn match_pieces<'p, 't>(
    pieces: &[Piece<'p>],
    text: &'t str,
    params: &mut Vec<(&'p str, &'t str)>,
) -> bool {
    match pieces.split_first() {
        None => text.is_empty(),
        Some((Piece::Text(t), rest)) => {
            text.starts_with(t) && match_pieces(rest, &text[t.len()..], params)
        }
        Some((Piece::Param(name), rest)) => {
            let ends: Vec<usize> = match rest.first() {
                None => vec![text.len()],
                Some(Piece::Text(next)) => text.match_indices(next).map(|(i, _)| i).collect(),
                // Two parameters side by side can't be told apart (tested
                // not to happen).
                Some(Piece::Param(_)) => return false,
            };
            for end in ends {
                let mark = params.len();
                params.push((name, &text[..end]));
                if match_pieces(rest, &text[end..], params) {
                    return true;
                }
                params.truncate(mark);
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    #[test]
    fn the_catalog_is_sorted_and_well_formed() {
        for pair in MESSAGES.windows(2) {
            assert!(
                pair[0].code < pair[1].code,
                "{} is out of order",
                pair[1].code
            );
        }
        for m in MESSAGES {
            assert!(
                m.code.starts_with("engine.")
                    && m.code
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_'),
                "{}: engine.<area>.<name>",
                m.code
            );
            for english in m.one.into_iter().chain([m.other]) {
                assert!(!english.contains('\u{2014}'), "{}: an em dash", m.code);
                assert!(!english.contains('!'), "{}: an exclamation mark", m.code);
                let ps = pieces(english);
                assert!(
                    !matches!(ps.as_slice(), [Piece::Param(_)]),
                    "{}: a parameter alone",
                    m.code
                );
                for w in ps.windows(2) {
                    assert!(
                        !matches!(w, [Piece::Param(_), Piece::Param(_)]),
                        "{}: two parameters side by side",
                        m.code
                    );
                }
            }
            if m.one.is_some() {
                assert!(
                    m.other.contains("{count}"),
                    "{}: a plural counts {{count}}",
                    m.code
                );
            }
        }
    }

    /// Every message, made with sample values, is recognized as itself.
    #[test]
    fn each_message_is_recognized_with_its_parameters() {
        for m in MESSAGES {
            for (english, count) in m.one.map(|o| (o, "1")).into_iter().chain([(m.other, "7")]) {
                let names: Vec<&str> = pieces(english)
                    .into_iter()
                    .filter_map(|p| match p {
                        Piece::Param(n) => Some(n),
                        Piece::Text(_) => None,
                    })
                    .collect();
                let values: Vec<(&str, String)> = names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| {
                        let v = if *n == "count" {
                            count.to_string()
                        } else {
                            format!("Qz{i}")
                        };
                        (*n, v)
                    })
                    .collect();
                let refs: Vec<(&str, &dyn Display)> = values
                    .iter()
                    .map(|(n, v)| (*n, v as &dyn Display))
                    .collect();
                let text = msg(m.code, &refs);
                assert_eq!(text, fill(english, &values));
                let r = recognize(&text).unwrap_or_else(|| panic!("{text:?} isn't recognized"));
                assert_eq!(r.code, m.code, "{text:?}");
                let got: Vec<(&str, String)> = r.params.into_iter().collect();
                assert_eq!(got, values, "{text:?}");
            }
        }
    }

    #[test]
    fn the_more_specific_message_wins_and_nouns_nest() {
        let r = recognize("a cutoff can't be negative").unwrap();
        assert_eq!(r.code, "engine.edit.negative_cutoff");
        let r = recognize("row 3's cap can't be negative").unwrap();
        assert_eq!(r.code, "engine.edit.cant_be_negative");
        assert_eq!(r.params, [("what", "row 3's cap".to_string())]);
        let r = recognize(&r.params[0].1).unwrap();
        assert_eq!(r.code, "engine.what.row_cap");
        let r = recognize("there is no group `Crew`").unwrap();
        assert_eq!(r.code, "engine.edit.credit_no_group");
        let r = recognize("account 1100 no longer exists").unwrap();
        assert_eq!(r.code, "engine.edit.account_gone");
        assert_eq!(recognize("Something else entirely."), None);
        assert_eq!(
            msg(
                "engine.edit.lines_use_currency",
                &[("count", &1), ("code", &"EUR")]
            ),
            "1 line uses EUR; change them first"
        );
        assert_eq!(
            fill("{a} and {b}", &[("a", "{b}"), ("b", "x")]),
            "{b} and x",
            "filled in one pass"
        );
    }

    /// Every `"engine.…"` code written in the crate's sources is in the
    /// catalog, and every message in the catalog is written somewhere.
    #[test]
    fn every_code_used_is_in_the_catalog_and_every_message_is_used() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut used = BTreeSet::new();
        let mut dirs = vec![src];
        while let Some(dir) = dirs.pop() {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    dirs.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs") || p.ends_with("messages.rs") {
                    continue;
                }
                let rs = std::fs::read_to_string(&p).unwrap();
                for (at, _) in rs.match_indices("\"engine.") {
                    let rest = &rs[at + 1..];
                    let end = rest.find('"').unwrap();
                    used.insert(rest[..end].to_string());
                }
            }
        }
        for code in &used {
            assert!(find(code).is_some(), "{code} is used but not in MESSAGES");
        }
        let unused: Vec<&str> = MESSAGES
            .iter()
            .map(|m| m.code)
            .filter(|c| !used.contains(*c))
            .collect();
        assert!(unused.is_empty(), "messages nothing makes: {unused:?}");
    }
}
