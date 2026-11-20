//! The working's fixed words and the number formatting for them.

use super::*;

// ---------------------------------------------------------------------------
// The working's fixed words (decisions.md, entry 94, twelfth pass)

/// A fixed piece of the working as a message code and its values, so a
/// front end can show it in the person's language. `label` and `text`
/// keep the English.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub code: &'static str,
    /// Each `{name}` in the code's words and its value: a number as the
    /// working writes it, or the person's own text (a description, a
    /// fringe's name, an account), never translated.
    pub params: Vec<(&'static str, String)>,
}

/// One of the working's fixed sentences: its code and its English, with a
/// form for one when it counts something (picked by its `count`).
/// method-session's message catalog has the same codes and words (tested
/// there), and the app's table translates them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wording {
    pub code: &'static str,
    pub one: Option<&'static str>,
    pub other: &'static str,
}

const fn w(code: &'static str, english: &'static str) -> Wording {
    Wording {
        code,
        one: None,
        other: english,
    }
}

const fn plural(code: &'static str, one: &'static str, other: &'static str) -> Wording {
    Wording {
        code,
        one: Some(one),
        other,
    }
}

/// The codes of the working's words all start with this.
pub const WORDING_PREFIX: &str = "engine.working.";

/// Every fixed sentence of the working, by code (sorted, tested).
#[rustfmt::skip]
pub const WORDING: &[Wording] = &[
    w("engine.working.a_flat_fee", "a flat fee"),
    w("engine.working.account_fringes", "fringes earned by the account's lines"),
    w("engine.working.account_total", "account total"),
    w("engine.working.adjustment_base", "the lines above, back to the previous adjustment (subtotal rows don't restart it)"),
    w("engine.working.adjustment_base_doubles", "the lines above, back to the previous adjustment (subtotal rows don't restart it), added up in doubles"),
    w("engine.working.after_last_level_neither", "categories after the last production total are neither above nor below the line"),
    w("engine.working.applied_credit", "an applied credit"),
    w("engine.working.at_least_minimum", "the rows' net spend {spend} is at least the minimum of {minimum}"),
    w("engine.working.at_most", "at most {cap}: {capped}"),
    w("engine.working.atl_btl_is", "above-the-line plus below-the-line"),
    w("engine.working.atl_is", "the first production level, when a production total closes it"),
    w("engine.working.below_minimum", "the rows' net spend {spend} is below the minimum of {minimum}, so the credit is 0"),
    w("engine.working.blank_counts_as_0", "blank, counts as 0"),
    w("engine.working.btl_is", "every later production level a production total closes"),
    w("engine.working.cap_on_spend", "the spend the percent applies to is at most {cap}: {spend}"),
    w("engine.working.category_fringe_account", "the category's fringe account (fringe posting: by category)"),
    w("engine.working.category_fringes", "fringes earned in the category"),
    w("engine.working.category_total", "category total"),
    w("engine.working.charge_base", "every category with its fringes, wherever they post, plus the budget's fringe row: the grand total before charges (charges never apply to each other)"),
    w("engine.working.charge_percent_of", "{percent}% of ({base} - {excluded}) = {exact}"),
    w("engine.working.counted_lines_tagged", "counted lines tagged {group}, with their fringes"),
    w("engine.working.currency", "{amount} {currency} x {rate} {base} per {currency} = {exact} {base}"),
    w("engine.working.currency_unrounded", "{amount} {currency} (not rounded yet) x {rate} {base} per {currency} = {exact} {base}"),
    w("engine.working.cutoff_factor", "the budget multiplies every percent cutoff by {factor}: {typed} x {factor} = {cutoff}"),
    w("engine.working.cutoff_used_by_lines_above", "{fringe} stops at {cutoff} per earner, and the lines above in the range used it all, so this line takes nothing"),
    w("engine.working.discount", "less {percent}%: {capped} x {keep} = {exact}"),
    w("engine.working.earns_none", "this line earns none"),
    w("engine.working.earns_nothing_from_it", "this line earns nothing from it"),
    w("engine.working.every_applied_credit", "every applied credit"),
    w("engine.working.every_line_of", "every line of {account}"),
    w("engine.working.fee_converted", "{amount} {currency} x {rate} {base} per {currency} = {exact}"),
    w("engine.working.fee_less", "less {fee}: {amount}"),
    w("engine.working.fee_less_floor", "less {fee}, but never below zero: {taken} taken off, {amount}"),
    w("engine.working.flat_cutoff_unused", "{cutoff} is kept on {fringe} but not used: cutoffs apply to percent fringes only"),
    w("engine.working.flat_once_per_line", "flat, once per line"),
    w("engine.working.flat_per_unit_amount", "flat, per {unit}, amount only"),
    w("engine.working.flat_per_unit_counts", "flat, per {unit}, amount plus counts in X and 4X"),
    w("engine.working.flat_per_unit_quantity", "flat, per {unit}, full quantity"),
    w("engine.working.fringe_kind_flat", "a flat fringe, over every counted line"),
    w("engine.working.fringe_kind_percent", "a percent fringe, no cutoff, over every counted line"),
    w("engine.working.fringe_kind_percent_cutoff", "a percent fringe with a {cutoff} cutoff per earner, over every counted line"),
    w("engine.working.fringe_kind_percent_factor", "a percent fringe with a {cutoff} cutoff per earner ({typed} x the budget's cutoff factor {factor}), over every counted line"),
    w("engine.working.fringe_on_account_lines", "{fringe} on the account's lines"),
    w("engine.working.fringe_range", "range {range}: lines {lines} earn as one person, so they share one cutoff, used up in line order"),
    w("engine.working.fringes_by_line", "fringes earned by line {line}"),
    w("engine.working.fringes_on_top", "earned on top of this total, shown in Fringe $ (not in it)"),
    w("engine.working.grand_total_is", "above and below-the-line, the categories after the last production total, budget fringes and charges"),
    w("engine.working.gross_account", "account {account} with its fringes, wherever fringes post"),
    w("engine.working.gross_category", "category {category} with its fringes, wherever fringes post"),
    w("engine.working.gross_no_account", "account {account} is not in the budget, so 0"),
    w("engine.working.gross_no_category", "category {category} is not in the budget, so 0"),
    w("engine.working.label.adjustment", "Adjustment"),
    w("engine.working.label.after_last_level", "Categories after the last production total"),
    w("engine.working.label.amount", "Amount"),
    w("engine.working.label.atl", "Total above-the-line"),
    w("engine.working.label.atl_btl", "Total above and below-the-line"),
    w("engine.working.label.base", "Base"),
    w("engine.working.label.brokers_fee", "Broker's fee"),
    w("engine.working.label.btl", "Total below-the-line"),
    w("engine.working.label.cap_on_spend", "Cap on spend"),
    w("engine.working.label.charge", "Charge"),
    w("engine.working.label.count", "Count"),
    w("engine.working.label.credit", "Credit"),
    w("engine.working.label.credit_cap", "Credit cap"),
    w("engine.working.label.credits", "Credits"),
    w("engine.working.label.currency", "Currency"),
    w("engine.working.label.cutoff", "Cutoff"),
    w("engine.working.label.cutoff_factor", "Cutoff factor"),
    w("engine.working.label.cutoff_reached", "Cutoff reached"),
    w("engine.working.label.cutoff_used_up", "Cutoff used up"),
    w("engine.working.label.discount", "Discount"),
    w("engine.working.label.earnings", "Earnings"),
    w("engine.working.label.excluded", "Excluded"),
    w("engine.working.label.excluded_amount", "Excluded amount"),
    w("engine.working.label.excluded_fringes", "Excluded fringes"),
    w("engine.working.label.fee", "Fee"),
    w("engine.working.label.fringe", "Fringe"),
    w("engine.working.label.fringe_range", "Fringe range"),
    w("engine.working.label.fringes", "Fringes"),
    w("engine.working.label.fringes_earned_here", "Fringes earned here"),
    w("engine.working.label.grand_total", "Grand total"),
    w("engine.working.label.gross", "Gross"),
    w("engine.working.label.left_out", "Left out"),
    w("engine.working.label.less_credits", "Less credits"),
    w("engine.working.label.line", "Line"),
    w("engine.working.label.line_n", "Line {line}"),
    w("engine.working.label.lines", "Lines"),
    w("engine.working.label.locations", "Locations"),
    w("engine.working.label.minimum_spend", "Minimum spend"),
    w("engine.working.label.net_spend", "Net spend"),
    w("engine.working.label.net_total", "Net total"),
    w("engine.working.label.no_fringes", "No fringes"),
    w("engine.working.label.no_production_totals", "No production totals"),
    w("engine.working.label.not_counted_here", "Not counted here"),
    w("engine.working.label.not_rounded_yet", "Not rounded yet"),
    w("engine.working.label.numbered_fringes", "{number} Fringes"),
    w("engine.working.label.person", "Person"),
    w("engine.working.label.quantity", "Quantity"),
    w("engine.working.label.rate", "Rate"),
    w("engine.working.label.rate_source", "Rate source"),
    w("engine.working.label.rounding", "Rounding"),
    w("engine.working.label.row_n", "Row {row}"),
    w("engine.working.label.rows_added", "Rows added"),
    w("engine.working.label.share", "Share"),
    w("engine.working.label.shared_cap", "Shared cap"),
    w("engine.working.label.subtotal", "Subtotal"),
    w("engine.working.label.total_fringes", "Total fringes"),
    w("engine.working.label.where_fringes_post", "Where fringes post"),
    w("engine.working.label.x", "X"),
    w("engine.working.label.x4", "4X"),
    w("engine.working.left_out_adjustment_ungrouped", "the adjustment row has no group and the group rule leaves out lines with no group, so it shows but counts as 0; counted, it would be {total}"),
    w("engine.working.left_out_by_report", "the report leaves out group {groups}; counted, it would be {total}"),
    w("engine.working.left_out_group_off", "group {groups} has In budget total off, so the line shows but counts as 0; counted, it would be {total}"),
    w("engine.working.left_out_group_off_conflict", "group {groups} has In budget total off (it is also in a counted group, and the group rule leaves out such conflicts), so the line shows but counts as 0; counted, it would be {total}"),
    w("engine.working.left_out_groups_off", "groups {groups} have In budget total off, so the line shows but counts as 0; counted, it would be {total}"),
    w("engine.working.left_out_groups_off_conflict", "groups {groups} have In budget total off (it is also in a counted group, and the group rule leaves out such conflicts), so the line shows but counts as 0; counted, it would be {total}"),
    w("engine.working.left_out_no_fringes", "the line doesn't count, so it earns no fringes"),
    w("engine.working.left_out_not_shown", "the report shows only some groups, and this line has none of them; counted, it would be {total}"),
    w("engine.working.left_out_ungrouped", "the line has no group and the group rule leaves out lines with no group, so it shows but counts as 0; counted, it would be {total}"),
    w("engine.working.level_adds", "the categories since the previous production total"),
    w("engine.working.lines_tagged", "lines tagged {group} plus their fringes"),
    w("engine.working.net_total_is", "grand total less credits"),
    w("engine.working.no_production_totals", "nothing is above or below the line until a production total closes the first level"),
    w("engine.working.not_in_total_account", "not in this total: fringes post to each account's total"),
    w("engine.working.not_in_total_budget", "not in this total: fringes post to the budget's one Total Fringes row"),
    w("engine.working.not_in_total_category", "not in this total: fringes post to each category's fringe account"),
    w("engine.working.not_in_total_level", "not in this total: fringes post above each production total"),
    w("engine.working.only_lines_at", "only lines at {locations} count, with their fringes"),
    w("engine.working.per_earner_left_not_reached", "{cutoff} per earner; {left} of it left after the lines above in the range, not reached"),
    w("engine.working.per_earner_not_reached", "{cutoff} per earner, not reached"),
    w("engine.working.per_person_not_reached", "{cutoff} per person, not reached: {person} is paid {pay} with {fringe}, so each line earns its own percent"),
    w("engine.working.percent_of", "{percent}% of {base} = {exact}"),
    w("engine.working.percent_of_budget", "{percent}% of the budget"),
    w("engine.working.percent_of_capped", "{percent}% of {base} = {exact}, capped at {cap}: {credit}"),
    w("engine.working.percent_of_rate", "{percent}% of {base} = {exact} (rate {formula})"),
    w("engine.working.percent_with_cutoff", "{percent}% with a {cutoff} cutoff"),
    w("engine.working.percent_with_cutoff_per_person", "{percent}% with a {cutoff} cutoff per person"),
    plural("engine.working.person_paid_account", "{person} is paid on {count} line in account {accounts}, so they share one cutoff", "{person} is paid on {count} lines in account {accounts}, so they share one cutoff"),
    plural("engine.working.person_paid_account_no_cutoff", "{person} is paid on {count} line in account {accounts}; {fringe} has no cutoff, so each line earns its own percent", "{person} is paid on {count} lines in account {accounts}; {fringe} has no cutoff, so each line earns its own percent"),
    plural("engine.working.person_paid_accounts", "{person} is paid on {count} line in accounts {accounts}, so they share one cutoff", "{person} is paid on {count} lines in accounts {accounts}, so they share one cutoff"),
    plural("engine.working.person_paid_accounts_no_cutoff", "{person} is paid on {count} line in accounts {accounts}; {fringe} has no cutoff, so each line earns its own percent", "{person} is paid on {count} lines in accounts {accounts}; {fringe} has no cutoff, so each line earns its own percent"),
    w("engine.working.person_past_cutoff", "{person} is paid {pay} with {fringe}, past the {cutoff} cutoff, so {person} earns {percent}% of {cutoff} = {exact}"),
    w("engine.working.person_past_cutoff_rate", "{person} is paid {pay} with {fringe}, past the {cutoff} cutoff, so {person} earns {percent}% of {cutoff} = {exact} (rate {formula})"),
    w("engine.working.posted_above_level", "posted above the production total (fringe posting: by production level)"),
    w("engine.working.posted_to_account", "posted to the account (fringe posting: by account)"),
    w("engine.working.posting_account", "fringes post to each account's total (a budget setting)"),
    w("engine.working.posting_budget", "fringes post to the budget's one Total Fringes row (a budget setting)"),
    w("engine.working.posting_category", "fringes post to each category's fringe account (a budget setting)"),
    w("engine.working.posting_level", "fringes post above each production total (a budget setting)"),
    w("engine.working.rounded_once", "rows are added, capped and discounted before the one rounding"),
    w("engine.working.rounds_double", "{amount} rounds to {rounded} (spreadsheet rounding: half up on the binary value)"),
    w("engine.working.rounds_double_held", "{amount} is {held} as a binary double, so it rounds to {rounded} (spreadsheet rounding: half up on the binary value)"),
    w("engine.working.rounds_exact", "{amount} rounds half away from zero to {rounded}"),
    w("engine.working.row_adjustment", "{code} line {line}, adjustment"),
    w("engine.working.row_line", "{code} line {line}, line"),
    w("engine.working.row_line_left_out", "{code} line {line}, line, left out"),
    w("engine.working.share", "this line's part of {person}'s {amount}, by pay: {amount} x {total} / {pay}, {floor} rounded down"),
    w("engine.working.share_extra_cent", "this line's part of {person}'s {amount}, by pay: {amount} x {total} / {pay}, {floor} rounded down, plus one of the cents left over (shares rounded down; the left-over cents go to the largest remainders, then the lowest row id)"),
    w("engine.working.shared_cap_missing", "{name} is not in the budget's shared caps, so it holds nothing down"),
    w("engine.working.shared_cap_share", "{name} caps this credit and {others} at {cap} together; its share, in proportion to {capped}, is {share}"),
    w("engine.working.shared_cap_within", "{name} caps the credits sharing it at {cap} together, which they are within"),
    w("engine.working.stops_per_earner", "{fringe} stops at {cutoff} per earner"),
    w("engine.working.stops_per_earner_left", "{fringe} stops at {cutoff} per earner; {left} of it left after the lines above in the range"),
    w("engine.working.stops_per_person", "{fringe} stops at {cutoff} per person, so nothing is earned"),
    w("engine.working.subtotal_adds", "the lines and adjustments since the previous subtotal row"),
    w("engine.working.taken_off_base", "taken off the base"),
    w("engine.working.taken_off_fee", "taken off the fee (typed amounts, and the lines of excluded groups, accounts and categories with their fringes)"),
    w("engine.working.this_lines_total", "this line's total"),
    w("engine.working.title.account_fringes", "Account {account}: fringes"),
    w("engine.working.title.account_lines", "Account {account}: lines"),
    w("engine.working.title.account_total", "Account {account}: total"),
    w("engine.working.title.after_last_level_fringes", "Categories after the last production total: fringes"),
    w("engine.working.title.after_last_level_total", "Categories after the last production total: total"),
    w("engine.working.title.category_fringes", "Category {category}: fringes"),
    w("engine.working.title.category_total", "Category {category}: total"),
    w("engine.working.title.charge", "Charge {name}"),
    w("engine.working.title.credit", "Credit {name}"),
    w("engine.working.title.fringe_total", "Fringe {fringe}: total"),
    w("engine.working.title.level_fringes", "{level}: fringes"),
    w("engine.working.title.level_total", "{level}: total"),
    w("engine.working.title.line_described_fringe", "{code} line {line} ({description}): {fringe}"),
    w("engine.working.title.line_described_fringes", "{code} line {line} ({description}): fringes"),
    w("engine.working.title.line_described_total", "{code} line {line} ({description}): total"),
    w("engine.working.title.line_fringe", "{code} line {line}: {fringe}"),
    w("engine.working.title.line_fringes", "{code} line {line}: fringes"),
    w("engine.working.title.line_total", "{code} line {line}: total"),
    w("engine.working.total_fringes_is", "every fringe in the budget, outside above and below-the-line (fringe posting: by budget)"),
    w("engine.working.trailing_fringes_dropped", "no production total follows these categories, so their fringes post nowhere and count in no total (trailing level fringes: drop)"),
    w("engine.working.typed_in", "typed in"),
    w("engine.working.whole_units_half_away", "the budget rounds account totals to whole units: the lines come to {lines}, which rounds to the nearest whole unit, a half away from zero to {whole}"),
    w("engine.working.whole_units_half_up", "the budget rounds account totals to whole units: the lines come to {lines}, which rounds to the nearest whole unit, a half up to {whole}"),
];

/// Words in the working: the English, and the code and values it was made
/// from when it is fixed words (`None`: the person's own text, or not
/// catalogued).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Words {
    pub(super) text: String,
    pub(super) said: Option<Said>,
}

impl Words {
    /// `code`'s English with `params` filled in. Every `{name}` in it has a
    /// value and every value a `{name}` (checked in debug builds).
    pub(super) fn said(code: &'static str, params: Vec<(&'static str, String)>) -> Words {
        let Ok(i) = WORDING.binary_search_by(|w| w.code.cmp(code)) else {
            debug_assert!(false, "{code} is not in WORDING");
            return Words::from(code);
        };
        let w = WORDING[i];
        let one = params.iter().any(|(n, v)| *n == "count" && v == "1");
        let english = match w.one {
            Some(o) if one => o,
            _ => w.other,
        };
        let (text, used, missing) = fill(english, &params);
        debug_assert!(
            used.len() == params.len() && !missing,
            "{code}: the values {params:?} don't fit {english:?}"
        );
        Words {
            text,
            said: Some(Said { code, params }),
        }
    }
}

impl From<String> for Words {
    fn from(text: String) -> Words {
        Words { text, said: None }
    }
}

impl From<&str> for Words {
    fn from(text: &str) -> Words {
        Words::from(text.to_string())
    }
}

impl From<&String> for Words {
    fn from(text: &String) -> Words {
        Words::from(text.clone())
    }
}

/// `template` with each `{name}` replaced by its value in one pass, as
/// method-session's `messages::fill` does, the names that were used, and
/// whether a name had no value.
pub(super) fn fill(
    template: &str,
    params: &[(&'static str, String)],
) -> (String, BTreeSet<&'static str>, bool) {
    let mut out = String::with_capacity(template.len());
    let mut used = BTreeSet::new();
    let mut missing = false;
    let mut rest = template;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let name = after.find('}').map(|end| &after[..end]).filter(|n| {
            !n.is_empty()
                && n.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        });
        match name.and_then(|n| params.iter().find(|(p, _)| *p == n)) {
            Some((p, v)) => {
                out.push_str(v);
                used.insert(*p);
                rest = &after[p.len() + 1..];
            }
            None => {
                missing |= name.is_some();
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    (out, used, missing)
}

// ---------------------------------------------------------------------------
// Number formatting for the plain words

/// `123456` cents as `1,234.56`, negatives with a minus sign.
pub fn money(c: Cents) -> String {
    crate::CurrencyFormat::default().number(c)
}

/// A decimal amount of money: at least two places, more when it has them.
pub(crate) fn dmoney(d: impl std::fmt::Display) -> String {
    let s = d.to_string();
    let (sign, s) = match s.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", s.as_str()),
    };
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    let frac = if frac.len() < 2 {
        format!("{frac:0<2}")
    } else {
        frac.to_string()
    };
    format!("{sign}{}.{frac}", group(int))
}

/// A plain decimal, thousands grouped.
pub(super) fn num(d: Dec) -> String {
    let s = d.to_string();
    let (sign, s) = match s.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", s.as_str()),
    };
    match s.split_once('.') {
        Some((i, f)) => format!("{sign}{}.{f}", group(i)),
        None => format!("{sign}{}", group(s)),
    }
}

pub(super) fn group(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// How `m` rounded to `rounded`, by the budget's arithmetic, when rounding
/// changed it: "6,125.819 rounds half away from zero to 6,125.82", or for
/// Movie Magic arithmetic "1.005 is 1.00499999999999989341... as a binary
/// double, so it rounds to 1.00 (Movie Magic rounding: half up on the
/// double)".
pub(super) fn rounding(m: impl Into<Money>, rounded: Cents) -> Option<Step> {
    let m = m.into();
    (!m.is_cents(rounded)).then(|| {
        let text = match m {
            Money::Exact(_) => say!(
                "engine.working.rounds_exact",
                amount = dmoney(m),
                rounded = money(rounded)
            ),
            Money::F64(v) => {
                let held = float_digits(v);
                if held == float_text(v) {
                    say!(
                        "engine.working.rounds_double",
                        amount = dmoney(m),
                        rounded = money(rounded)
                    )
                } else {
                    say!(
                        "engine.working.rounds_double_held",
                        amount = dmoney(m),
                        held = dmoney(held),
                        rounded = money(rounded)
                    )
                }
            }
        };
        Step::note(say!("engine.working.label.rounding"), text)
    })
}
