//! Charts of accounts on their own: categories, accounts and production
//! totals, with the account-number pattern they follow, and no lines.
//!
//! A [`Chart`] is what a chart-of-accounts import reads (from a spreadsheet
//! in `method_format::import::coa`, or from another budget with
//! [`Chart::from_budget`]) and what the bundled chart templates are built
//! from. It becomes either a new budget with that chart and nothing else
//! ([`Chart::to_budget`]) or is applied to an existing budget ([`apply`]):
//! missing categories and accounts are added, existing ones renamed by
//! number, and nothing with lines is ever removed.
//!
//! Only numbers, names and levels are kept (CLEAN-ROOM.md): a chart carries
//! no descriptions, fringes, globals or rates.

use std::collections::{BTreeMap, BTreeSet};

use crate::coding::{AccountCoding, MAX_WIDTH, SEPARATORS};
use crate::model::{Account, Budget, Category, ProductionTotal, TopsheetRow};
use crate::renumber::{carry_account, carry_category, carry_category_part};

/// One account of a chart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartAccount {
    pub code: String,
    pub name: String,
}

/// One category of a chart and its accounts, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartCategory {
    pub code: String,
    pub name: String,
    pub accounts: Vec<ChartAccount>,
}

/// A topsheet row of a chart: a category, or a production total (a level)
/// that subtotals the categories since the previous one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChartRow {
    Category(ChartCategory),
    Level(String),
}

/// A chart of accounts: its rows in topsheet order and the pattern its
/// numbers are written in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chart {
    pub coding: AccountCoding,
    pub rows: Vec<ChartRow>,
}

impl Chart {
    pub fn categories(&self) -> impl Iterator<Item = &ChartCategory> {
        self.rows.iter().filter_map(|r| match r {
            ChartRow::Category(c) => Some(c),
            ChartRow::Level(_) => None,
        })
    }

    pub fn category_count(&self) -> usize {
        self.categories().count()
    }

    pub fn account_count(&self) -> usize {
        self.categories().map(|c| c.accounts.len()).sum()
    }

    pub fn level_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| matches!(r, ChartRow::Level(_)))
            .count()
    }

    /// The chart of budget `b`: its categories, accounts and production
    /// totals, in its own pattern.
    pub fn from_budget(b: &Budget) -> Chart {
        let rows = b
            .topsheet
            .iter()
            .map(|r| match r {
                TopsheetRow::Category(c) => ChartRow::Category(ChartCategory {
                    code: c.code.clone(),
                    name: c.name.clone(),
                    accounts: c
                        .accounts
                        .iter()
                        .map(|a| ChartAccount {
                            code: a.code.clone(),
                            name: a.name.clone(),
                        })
                        .collect(),
                }),
                TopsheetRow::ProductionTotal(p) => ChartRow::Level(p.name.clone()),
            })
            .collect();
        Chart {
            coding: b.account_coding,
            rows,
        }
    }

    /// The same chart with every number carried over to pattern `to` (as
    /// renumbering does: short parts padded with zeros, long ones losing
    /// leading zeros, nothing cut off). `Err` lists every number that
    /// doesn't fit and every two numbers that would become one.
    pub fn recode(&self, to: &AccountCoding) -> Result<Chart, Vec<String>> {
        if to.validate().is_err() {
            return Err(vec!["the account-number pattern isn't valid".into()]);
        }
        let mut problems = Vec::new();
        let mut seen_cat: BTreeMap<String, String> = BTreeMap::new();
        let mut seen_acct: BTreeMap<String, String> = BTreeMap::new();
        let mut rows = Vec::with_capacity(self.rows.len());
        for r in &self.rows {
            match r {
                ChartRow::Level(n) => rows.push(ChartRow::Level(n.clone())),
                ChartRow::Category(c) => {
                    let code = match carry_category(&self.coding, to, &c.code) {
                        Ok(code) => code,
                        Err(e) => {
                            problems.push(format!("category {}: {e}", c.code));
                            continue;
                        }
                    };
                    if let Some(old) = seen_cat.insert(code.clone(), c.code.clone()) {
                        problems.push(format!(
                            "categories {old} and {} would both be {code}",
                            c.code
                        ));
                    }
                    let mut accounts = Vec::with_capacity(c.accounts.len());
                    for a in &c.accounts {
                        match carry_account(&self.coding, to, &a.code) {
                            Ok(new) => {
                                if let Some(old) = seen_acct.insert(new.clone(), a.code.clone()) {
                                    problems.push(format!(
                                        "accounts {old} and {} would both be {new}",
                                        a.code
                                    ));
                                }
                                accounts.push(ChartAccount {
                                    code: new,
                                    name: a.name.clone(),
                                });
                            }
                            Err(e) => problems.push(format!("account {}: {e}", a.code)),
                        }
                    }
                    rows.push(ChartRow::Category(ChartCategory {
                        code,
                        name: c.name.clone(),
                        accounts,
                    }));
                }
            }
        }
        if problems.is_empty() {
            Ok(Chart { coding: *to, rows })
        } else {
            Err(problems)
        }
    }

    /// The chart with our default production totals added when it names
    /// none: categories are grouped by the first character of their number
    /// (`1` above the line, `2` and `3` production, `4` post, the rest
    /// other), with a total after each group. A chart whose categories all
    /// fall in one group gets none (there is nothing to tell apart).
    pub fn with_default_levels(&self) -> Chart {
        if self.level_count() > 0 {
            return self.clone();
        }
        let group = |code: &str| -> usize {
            match code.chars().next() {
                Some('0' | '1') => 0,
                Some('2' | '3') => 1,
                Some('4') => 2,
                _ => 3,
            }
        };
        let groups: BTreeSet<usize> = self.categories().map(|c| group(&c.code)).collect();
        if groups.len() < 2 {
            return self.clone();
        }
        let mut rows = Vec::new();
        let mut current: Option<usize> = None;
        for r in &self.rows {
            if let ChartRow::Category(c) = r {
                let g = group(&c.code);
                if let Some(prev) = current {
                    if prev != g {
                        rows.push(ChartRow::Level(DEFAULT_LEVELS[prev].to_string()));
                    }
                }
                current = Some(g);
            }
            rows.push(r.clone());
        }
        if let Some(prev) = current {
            rows.push(ChartRow::Level(DEFAULT_LEVELS[prev].to_string()));
        }
        Chart {
            coding: self.coding,
            rows,
        }
    }

    /// A new budget titled `title` holding this chart and nothing else: no
    /// lines, setup or charges, with the settings every new budget starts
    /// with ([`crate::starter::blank`]) but the chart's own account numbers.
    pub fn to_budget(&self, title: &str, base_currency: &str) -> Budget {
        let mut b = crate::starter::blank(title.trim(), base_currency);
        b.account_coding = self.coding;
        b.topsheet = self
            .rows
            .iter()
            .map(|r| match r {
                ChartRow::Level(n) => {
                    TopsheetRow::ProductionTotal(ProductionTotal { name: n.clone() })
                }
                ChartRow::Category(c) => TopsheetRow::Category(Category {
                    code: c.code.clone(),
                    name: c.name.clone(),
                    accounts: c
                        .accounts
                        .iter()
                        .map(|a| Account {
                            code: a.code.clone(),
                            name: a.name.clone(),
                            details: Vec::new(),
                            notes: Vec::new(),
                        })
                        .collect(),
                    notes: Vec::new(),
                }),
            })
            .collect();
        b
    }
}

/// Our default production totals' names, in [`Chart::with_default_levels`]'s
/// group order; the starter budget uses the same ones.
pub const DEFAULT_LEVELS: [&str; 4] = [
    "Total above-the-line",
    "Total production",
    "Total post",
    "Total other",
];

/// Letters and digits only, lowercase: how level headings are compared.
fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The production total a heading or total row names (`ABOVE THE LINE`,
/// `Total Below-the-Line`, `Post Production`, `BTL`), or `None` when the
/// text isn't one of the usual level names. Words such as `total`,
/// `costs`, `period` and `expenses` around the name are ignored.
pub fn level_name(text: &str) -> Option<String> {
    let mut s = squash(text);
    for prefix in ["grandtotal", "subtotal", "total"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
            break;
        }
    }
    for suffix in ["costs", "cost", "period", "expenses", "expense", "total"] {
        if let Some(rest) = s.strip_suffix(suffix) {
            if !rest.is_empty() {
                s = rest.to_string();
            }
            break;
        }
    }
    let name = match s.as_str() {
        "abovetheline" | "atl" => DEFAULT_LEVELS[0],
        "belowtheline" | "btl" => "Total below-the-line",
        "production"
        | "belowthelineproduction"
        | "btlproduction"
        | "shooting"
        | "physical"
        | "physicalproduction" => DEFAULT_LEVELS[1],
        "post"
        | "postproduction"
        | "belowthelinepost"
        | "belowthelinepostproduction"
        | "btlpost"
        | "btlpostproduction"
        | "completion" => DEFAULT_LEVELS[2],
        "other" | "belowthelineother" | "btlother" | "general" | "generalexpense" | "otherbtl" => {
            DEFAULT_LEVELS[3]
        }
        "fringes" | "fringebenefits" | "fringe" => "Total fringes",
        "pattern" | "patternbudget" => "Total pattern",
        "amortization" | "amortized" | "amortizationbudget" => "Total amortization",
        "episodic" | "perepisode" => "Total episodic",
        _ => return None,
    };
    Some(name.to_string())
}

/// The production total for a level heading of any wording: one of the
/// usual names when it is one ([`level_name`]), else `Total <heading>` (a
/// heading in capitals is written in sentence case).
pub fn total_for_heading(heading: &str) -> String {
    if let Some(n) = level_name(heading) {
        return n;
    }
    let h = heading.trim();
    let shouty = h.chars().any(|c| c.is_alphabetic()) && !h.chars().any(|c| c.is_lowercase());
    let h = if shouty {
        h.to_lowercase()
    } else {
        h.to_string()
    };
    match h
        .strip_prefix("total ")
        .or_else(|| h.strip_prefix("Total "))
    {
        Some(rest) => format!("Total {rest}"),
        None => format!("Total {h}"),
    }
}

// ---------------------------------------------------------------------------
// Inferring the pattern

fn is_separator(c: char) -> bool {
    SEPARATORS.contains(&c)
}

fn pieces(code: &str) -> Vec<&str> {
    code.split(is_separator).filter(|p| !p.is_empty()).collect()
}

/// The smallest category width `w` for numbers that all have the same
/// length and no separator (`1100`, `1101`, `1200`): the numbers that are
/// all zeros after `w` characters are the categories, and every other number
/// starts with one of their first `w` characters. `None` when no width
/// works (a flat list of accounts with no category rows).
pub fn zero_suffix_width(codes: &[&str]) -> Option<usize> {
    let len = codes.iter().map(|c| c.len()).max()?;
    (1..len).find(|&w| {
        let is_cat = |c: &str| c.len() > w && c[w..].bytes().all(|b| b == b'0');
        let cats: BTreeSet<&str> = codes
            .iter()
            .filter(|c| is_cat(c))
            .map(|c| &c[..w])
            .collect();
        let rest: Vec<&str> = codes.iter().filter(|c| !is_cat(c)).copied().collect();
        !cats.is_empty() && covered(&cats, &rest, w)
    })
}

/// Whether at least half of `accounts` start with one of the category
/// prefixes `cats` (of `w` characters): a stray account or two whose
/// category row is missing doesn't stop the categories being found.
fn covered(cats: &BTreeSet<&str>, accounts: &[&str], w: usize) -> bool {
    let n = accounts
        .iter()
        .filter(|a| a.len() > w && cats.contains(&a[..w]))
        .count();
    n * 2 >= accounts.len()
}

/// The account-number pattern a chart's numbers follow, from the category
/// numbers (`11`, `1100`, `11-00`), account numbers (`1101`, `11-01`) and
/// sub-account numbers (`110101`, `11-01-01`) read from a file. Numbers
/// with a separator set it (the commonest one) and give each part's width;
/// numbers without one are cut where the category numbers end, a category
/// written `1100` over accounts `1101` counting as category `11`.
pub fn infer_coding(
    categories: &[&str],
    accounts: &[&str],
    subs: &[&str],
) -> Result<AccountCoding, String> {
    let mut counts: BTreeMap<char, usize> = BTreeMap::new();
    for code in accounts.iter().chain(subs).chain(categories) {
        for c in code.chars().filter(|c| is_separator(*c)) {
            *counts.entry(c).or_default() += 1;
        }
    }
    // The commonest separator; ties go to the one listed first.
    let separator = SEPARATORS
        .iter()
        .copied()
        .filter(|c| counts.contains_key(c))
        .max_by_key(|c| {
            (
                counts[c],
                std::cmp::Reverse(SEPARATORS.iter().position(|s| s == c)),
            )
        });
    let has_sep = |c: &&str| c.chars().any(is_separator);
    let max_len = |it: &mut dyn Iterator<Item = usize>| it.max().unwrap_or(0);

    let (cw, aw, sw) =
        if separator.is_some() && (accounts.iter().any(has_sep) || subs.iter().any(has_sep)) {
            let leaves: Vec<&str> = accounts.iter().chain(subs).copied().collect();
            let cw = max_len(
                &mut leaves
                    .iter()
                    .map(|c| pieces(c).first().map_or(0, |p| p.len())),
            );
            let aw = max_len(
                &mut leaves
                    .iter()
                    .map(|c| pieces(c).get(1).map_or(0, |p| p.len())),
            );
            let sw = max_len(
                &mut leaves
                    .iter()
                    .map(|c| pieces(c).get(2).map_or(0, |p| p.len())),
            );
            (cw, aw, sw)
        } else if separator.is_some() && accounts.is_empty() && subs.is_empty() {
            let cw = max_len(
                &mut categories
                    .iter()
                    .map(|c| pieces(c).first().map_or(0, |p| p.len())),
            );
            (cw, 2, 0)
        } else {
            let alen = max_len(&mut accounts.iter().map(|c| c.len()));
            let cw = if categories.is_empty() {
                match alen {
                    0 => 2,
                    1..=3 => 1,
                    _ => 2,
                }
            } else if accounts.is_empty() {
                max_len(&mut categories.iter().map(|c| c.len()))
            } else {
                let cmax = categories.iter().map(|c| c.len()).max().unwrap_or(0);
                (1..=cmax.min(alen.saturating_sub(1)))
                    .find(|&w| {
                        let zeros = |c: &&str| c.len() >= w && c[w..].bytes().all(|b| b == b'0');
                        let prefixes: BTreeSet<&str> =
                            categories.iter().map(|c| &c[..w.min(c.len())]).collect();
                        categories.iter().all(zeros) && covered(&prefixes, accounts, w)
                    })
                    .unwrap_or_else(|| {
                        categories
                            .iter()
                            .map(|c| c.len())
                            .min()
                            .unwrap_or(2)
                            .min(alen.saturating_sub(1).max(1))
                    })
            };
            let aw = if accounts.is_empty() {
                2
            } else {
                alen.saturating_sub(cw)
            };
            let slen = max_len(&mut subs.iter().map(|c| c.len()));
            let sw = if subs.is_empty() {
                0
            } else {
                slen.saturating_sub(cw + aw)
            };
            (cw, aw, sw)
        };
    let fits = |w: usize| w <= usize::from(MAX_WIDTH);
    if cw == 0 {
        return Err("no category part could be found in the numbers".into());
    }
    if !fits(cw) || !fits(aw) || !fits(sw) {
        return Err(format!(
            "the numbers' parts are longer than {MAX_WIDTH} characters (category {cw}, account {aw}{})",
            if sw > 0 { format!(", sub-account {sw}") } else { String::new() }
        ));
    }
    if !accounts.is_empty() && aw == 0 {
        return Err("the account numbers are no longer than the category numbers".into());
    }
    if !subs.is_empty() && sw == 0 {
        return Err("the sub-account numbers are no longer than the account numbers".into());
    }
    let coding = AccountCoding {
        category_width: cw as u8,
        account_width: aw.max(1) as u8,
        sub_width: sw as u8,
        set_width: 0,
        separator,
    };
    coding.validate().map_err(|e| e.to_string())?;
    Ok(coding)
}

/// The pattern for a chart whose numbers are kept exactly as written and
/// whose accounts are already placed in their categories (one read with
/// its rows' kinds named): nothing depends on the pattern but typing new
/// numbers and matching, so it is the one most of the numbers follow.
///
/// `accounts` are each account's number and its category's number. Every
/// pattern of up to [`MAX_WIDTH`] + [`MAX_WIDTH`] characters, with no
/// separator or one the account numbers use, is scored by how many account
/// numbers it writes exactly as they are (`1101` under 2 + 2, not `11` +
/// `1` padded) with the category part their category names (`1100` and
/// `11-00` name `11`). The best score wins; on a tie, the pattern
/// [`infer_coding`] gives for all the numbers, then the one under which
/// more category numbers name a category part, then the shorter category
/// part. Never fails: a chart with no usable numbers gets the default.
pub fn infer_coding_lenient(
    categories: &[&str],
    accounts: &[(&str, &str)],
    subs: &[&str],
) -> AccountCoding {
    let codes: Vec<&str> = accounts.iter().map(|(a, _)| *a).collect();
    let inferred = infer_coding(categories, &codes, subs).ok();
    let mut seps: Vec<Option<char>> = vec![None];
    for c in SEPARATORS {
        if codes.iter().any(|a| a.contains(c)) {
            seps.push(Some(c));
        }
    }
    let mut candidates: Vec<AccountCoding> = inferred.into_iter().collect();
    for separator in seps {
        for category_width in 1..=MAX_WIDTH {
            for account_width in 1..=MAX_WIDTH {
                candidates.push(AccountCoding {
                    category_width,
                    account_width,
                    sub_width: 0,
                    set_width: 0,
                    separator,
                });
            }
        }
    }
    let score = |k: &AccountCoding| {
        let follows = accounts
            .iter()
            .filter(|(a, cat)| {
                k.read(a).is_some_and(|c| {
                    !c.account.is_empty()
                        && k.format(&c) == a.trim()
                        && (cat.is_empty() || k.category_key(cat) == c.category)
                })
            })
            .count();
        let cats = categories
            .iter()
            .filter(|c| k.category_key(c) != c.trim())
            .count();
        (
            follows,
            Some(*k) == inferred,
            cats,
            std::cmp::Reverse(k.category_width),
        )
    };
    // `rev`: the first of equals wins.
    candidates
        .into_iter()
        .rev()
        .max_by_key(|k| score(k))
        .unwrap_or_default()
}

/// A number split into the parts of `coding`, each padded to its width, or
/// why it doesn't fit (a part too long, too many parts, or with no
/// separator more characters than the pattern holds). Nothing is cut off.
pub fn split_code(coding: &AccountCoding, code: &str) -> Result<Vec<String>, String> {
    let code = code.trim();
    if code.is_empty() {
        return Err("it is empty".into());
    }
    if let Some(c) = code
        .chars()
        .find(|c| !c.is_ascii_alphanumeric() && !is_separator(*c))
    {
        return Err(format!("it has the character {c:?}"));
    }
    let widths: Vec<usize> = [
        coding.category_width,
        coding.account_width,
        coding.sub_width,
        coding.set_width,
    ]
    .into_iter()
    .filter(|w| *w > 0)
    .map(usize::from)
    .collect();
    let raw: Vec<String> = if code.chars().any(is_separator) {
        pieces(code).into_iter().map(str::to_string).collect()
    } else {
        let mut rest = code;
        let mut out = Vec::new();
        for &w in &widths {
            if rest.is_empty() {
                break;
            }
            let (head, tail) = rest.split_at(rest.len().min(w));
            out.push(head.to_string());
            rest = tail;
        }
        if !rest.is_empty() {
            return Err(format!(
                "it is longer than the pattern's {} characters",
                widths.iter().sum::<usize>()
            ));
        }
        out
    };
    if raw.len() > widths.len() {
        return Err(format!(
            "it has {} parts but the pattern has {}",
            raw.len(),
            widths.len()
        ));
    }
    raw.iter()
        .zip(&widths)
        .map(|(p, &w)| {
            if p.len() > w {
                Err(format!("its part {p} is longer than {w} characters"))
            } else {
                Ok(format!("{p:0>w$}"))
            }
        })
        .collect()
}

/// Parts joined with `coding`'s separator.
pub fn join_code(coding: &AccountCoding, parts: &[String]) -> String {
    let sep = coding.separator.map(String::from).unwrap_or_default();
    parts.join(&sep)
}

// ---------------------------------------------------------------------------
// Applying a chart to a budget

/// How [`apply`] treats the budget's accounts that the chart doesn't list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplyOptions {
    /// Remove accounts the chart doesn't list when they have no lines and
    /// no notes, and categories it doesn't list once they are empty. An
    /// account with lines is never removed.
    pub remove_unlisted: bool,
}

/// A number and its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    pub code: String,
    pub name: String,
}

/// A category or account the chart renames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Renamed {
    pub code: String,
    pub old: String,
    pub new: String,
}

/// What [`apply`] did (or, on a copy, would do).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub added_categories: Vec<Named>,
    pub added_accounts: Vec<Named>,
    pub renamed_categories: Vec<Renamed>,
    pub renamed_accounts: Vec<Renamed>,
    /// Accounts already there under the same number and name.
    pub unchanged_accounts: usize,
    /// Accounts the chart doesn't list that have lines (or notes): kept.
    pub kept: Vec<Named>,
    /// Accounts the chart doesn't list with nothing in them: removed with
    /// [`ApplyOptions::remove_unlisted`], else kept.
    pub unlisted_empty: Vec<Named>,
    /// Accounts and categories removed.
    pub removed_accounts: Vec<Named>,
    pub removed_categories: Vec<Named>,
    /// Chart numbers that don't fit the budget's pattern, left out.
    pub not_fitting: Vec<String>,
}

impl ApplyReport {
    pub fn changes_anything(&self) -> bool {
        !(self.added_categories.is_empty()
            && self.added_accounts.is_empty()
            && self.renamed_categories.is_empty()
            && self.renamed_accounts.is_empty()
            && self.removed_accounts.is_empty()
            && self.removed_categories.is_empty())
    }
}

/// Where a new category goes: after the last category numbered below it,
/// else before the first category (production totals keep their places).
fn category_slot(b: &Budget, code: &str) -> usize {
    let mut after = None;
    let mut first = None;
    for (i, r) in b.topsheet.iter().enumerate() {
        if let TopsheetRow::Category(c) = r {
            first.get_or_insert(i);
            if c.code.as_str() < code {
                after = Some(i);
            }
        }
    }
    match (after, first) {
        (Some(i), _) => i + 1,
        (None, Some(i)) => i,
        (None, None) => b.topsheet.len(),
    }
}

/// Applies `chart` to `b`: categories and accounts it lists that `b`
/// lacks are added (in number order among what is there), the ones `b`
/// has are renamed to the chart's names, and what the chart doesn't list
/// is kept (or, with [`ApplyOptions::remove_unlisted`], removed when it
/// holds nothing). The chart's numbers are carried over to `b`'s pattern
/// first; ones that don't fit are left out and listed. Production totals
/// are left as `b` has them. Never touches a line.
pub fn apply(b: &mut Budget, chart: &Chart, opts: &ApplyOptions) -> Result<ApplyReport, String> {
    if chart.category_count() == 0 {
        return Err("the chart has no categories".into());
    }
    let mut report = ApplyReport::default();
    let to = b.account_coding;
    // The chart's numbers in the budget's pattern.
    let mut cats: Vec<(String, String, Vec<Named>)> = Vec::new();
    for c in chart.categories() {
        let code = match carry_category_part(&chart.coding, &to, &c.code) {
            Ok(code) => code,
            Err(e) => {
                report.not_fitting.push(format!("category {}: {e}", c.code));
                continue;
            }
        };
        let mut accounts = Vec::new();
        for a in &c.accounts {
            match carry_account(&chart.coding, &to, &a.code) {
                Ok(new) => accounts.push(Named {
                    code: new,
                    name: a.name.clone(),
                }),
                Err(e) => report.not_fitting.push(format!("account {}: {e}", a.code)),
            }
        }
        cats.push((code, c.name.clone(), accounts));
    }
    // Numbers are compared as the budget's pattern reads them, never cut
    // short: category `11` is the budget's `1100`, account `1101` its `11-01`.
    let listed_accounts: BTreeSet<String> = cats
        .iter()
        .flat_map(|(_, _, a)| a.iter().map(|a| to.key(&a.code)))
        .collect();
    let listed_categories: BTreeSet<String> =
        cats.iter().map(|(c, _, _)| to.category_key(c)).collect();

    for (code, name, accounts) in &cats {
        let key = to.category_key(code);
        let at = b
            .categories()
            .position(|c| &c.code == code)
            .or_else(|| b.categories().position(|c| to.category_key(&c.code) == key));
        let existing = at.and_then(|i| b.categories_mut().nth(i));
        // The budget's own spelling of the category, for its accounts below.
        let code = &existing
            .as_ref()
            .map_or_else(|| code.clone(), |c| c.code.clone());
        match existing {
            Some(c) => {
                if !name.trim().is_empty() && c.name != *name {
                    report.renamed_categories.push(Renamed {
                        code: code.clone(),
                        old: c.name.clone(),
                        new: name.clone(),
                    });
                    c.name = name.clone();
                }
            }
            None => {
                let at = category_slot(b, code);
                b.topsheet.insert(
                    at,
                    TopsheetRow::Category(Category {
                        code: code.clone(),
                        name: name.clone(),
                        accounts: Vec::new(),
                        notes: Vec::new(),
                    }),
                );
                report.added_categories.push(Named {
                    code: code.clone(),
                    name: name.clone(),
                });
            }
        }
        for a in accounts {
            let exact = b
                .categories()
                .flat_map(|c| &c.accounts)
                .any(|x| x.code == a.code);
            let akey = to.key(&a.code);
            let existing = b
                .categories_mut()
                .flat_map(|c| c.accounts.iter_mut())
                .find(|x| {
                    if exact {
                        x.code == a.code
                    } else {
                        to.key(&x.code) == akey
                    }
                });
            match existing {
                Some(x) => {
                    if !a.name.trim().is_empty() && x.name != a.name {
                        report.renamed_accounts.push(Renamed {
                            code: a.code.clone(),
                            old: x.name.clone(),
                            new: a.name.clone(),
                        });
                        x.name = a.name.clone();
                    } else {
                        report.unchanged_accounts += 1;
                    }
                }
                None => {
                    let cat = b
                        .categories_mut()
                        .find(|c| &c.code == code)
                        .expect("added above");
                    let at = cat
                        .accounts
                        .iter()
                        .rposition(|x| x.code < a.code)
                        .map_or(0, |i| i + 1);
                    cat.accounts.insert(
                        at,
                        Account {
                            code: a.code.clone(),
                            name: a.name.clone(),
                            details: Vec::new(),
                            notes: Vec::new(),
                        },
                    );
                    report.added_accounts.push(a.clone());
                }
            }
        }
    }

    // What the chart doesn't list.
    for c in b.categories_mut() {
        let mut keep = Vec::with_capacity(c.accounts.len());
        for a in std::mem::take(&mut c.accounts) {
            if listed_accounts.contains(&to.key(&a.code)) {
                keep.push(a);
                continue;
            }
            let named = Named {
                code: a.code.clone(),
                name: a.name.clone(),
            };
            if !a.details.is_empty() || !a.notes.is_empty() {
                report.kept.push(named);
                keep.push(a);
            } else {
                report.unlisted_empty.push(named.clone());
                if opts.remove_unlisted {
                    report.removed_accounts.push(named);
                } else {
                    keep.push(a);
                }
            }
        }
        c.accounts = keep;
    }
    if opts.remove_unlisted {
        b.topsheet.retain(|r| match r {
            TopsheetRow::Category(c)
                if !listed_categories.contains(&to.category_key(&c.code))
                    && c.accounts.is_empty()
                    && c.notes.is_empty() =>
            {
                report.removed_categories.push(Named {
                    code: c.code.clone(),
                    name: c.name.clone(),
                });
                false
            }
            _ => true,
        });
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Detail;
    use crate::{recalc, starter, RecalcOptions};

    fn chart(rows: &[(&str, &str)]) -> Chart {
        // `("10", "Story")` is a category, `("10-01", "Rights")` an account,
        // `("", "Total x")` a level.
        let mut out: Vec<ChartRow> = Vec::new();
        for (code, name) in rows {
            if code.is_empty() {
                out.push(ChartRow::Level(name.to_string()));
            } else if !code.contains('-') {
                out.push(ChartRow::Category(ChartCategory {
                    code: code.to_string(),
                    name: name.to_string(),
                    accounts: Vec::new(),
                }));
            } else if let Some(ChartRow::Category(c)) = out
                .iter_mut()
                .rev()
                .find(|r| matches!(r, ChartRow::Category(_)))
            {
                c.accounts.push(ChartAccount {
                    code: code.to_string(),
                    name: name.to_string(),
                });
            }
        }
        Chart {
            coding: AccountCoding::default(),
            rows: out,
        }
    }

    #[test]
    fn infers_patterns_from_numbers() {
        let c = |cats: &[&str], accts: &[&str], subs: &[&str]| {
            let k = infer_coding(cats, accts, subs).unwrap();
            (k.category_width, k.account_width, k.sub_width, k.separator)
        };
        assert_eq!(
            c(&["11", "12"], &["11-01", "12-05"], &[]),
            (2, 2, 0, Some('-'))
        );
        assert_eq!(
            c(&["1100", "1200"], &["1101", "1205"], &[]),
            (2, 2, 0, None)
        );
        assert_eq!(c(&["11", "12"], &["1101", "1205"], &[]), (2, 2, 0, None));
        assert_eq!(c(&[], &["1101", "1205"], &[]), (2, 2, 0, None));
        assert_eq!(
            c(&["011"], &["011.001"], &["011.001.01"]),
            (3, 3, 2, Some('.'))
        );
        assert_eq!(
            c(&["1000", "2000"], &["1001", "2050"], &[]),
            (1, 3, 0, None)
        );
        assert_eq!(c(&["11"], &["1101"], &["110101"]), (2, 2, 2, None));
        assert!(infer_coding(&["1"], &["1234567890"], &[]).is_err());
    }

    #[test]
    fn zero_suffix_finds_category_rows() {
        assert_eq!(
            zero_suffix_width(&["1100", "1101", "1200", "1201"]),
            Some(2)
        );
        assert_eq!(zero_suffix_width(&["1101", "1102"]), None);
    }

    #[test]
    fn split_code_never_cuts() {
        let k = AccountCoding::default();
        assert_eq!(split_code(&k, "11-5").unwrap(), vec!["11", "05"]);
        assert_eq!(split_code(&k, "1105").unwrap(), vec!["11", "05"]);
        assert!(split_code(&k, "110501").is_err());
        assert!(split_code(&k, "11-001").is_err());
        assert!(split_code(&k, "11-01-01").is_err());
    }

    #[test]
    fn level_names_are_recognized() {
        assert_eq!(
            level_name("ABOVE THE LINE").as_deref(),
            Some("Total above-the-line")
        );
        assert_eq!(
            level_name("Total Above-the-Line").as_deref(),
            Some("Total above-the-line")
        );
        assert_eq!(level_name("Post Production").as_deref(), Some("Total post"));
        assert_eq!(level_name("BTL").as_deref(), Some("Total below-the-line"));
        assert_eq!(level_name("Other costs").as_deref(), Some("Total other"));
        assert_eq!(level_name("Story and rights"), None);
        assert_eq!(total_for_heading("EPISODIC"), "Total episodic");
        assert_eq!(total_for_heading("DEVELOPMENT"), "Total development");
    }

    #[test]
    fn default_levels_follow_the_first_digit() {
        let c = chart(&[
            ("10", "Story"),
            ("11", "Producers"),
            ("20", "Staff"),
            ("45", "Editorial"),
            ("60", "Insurance"),
        ]);
        let d = c.with_default_levels();
        let names: Vec<String> = d
            .rows
            .iter()
            .map(|r| match r {
                ChartRow::Level(n) => n.clone(),
                ChartRow::Category(c) => c.code.clone(),
            })
            .collect();
        assert_eq!(
            names,
            [
                "10",
                "11",
                "Total above-the-line",
                "20",
                "Total production",
                "45",
                "Total post",
                "60",
                "Total other"
            ]
        );
        // One group: nothing to tell apart.
        let one = chart(&[("10", "A"), ("11", "B")]);
        assert_eq!(one.with_default_levels(), one);
    }

    #[test]
    fn a_chart_becomes_a_budget_and_back() {
        let c = chart(&[
            ("10", "Story"),
            ("10-01", "Rights"),
            ("", "Total above-the-line"),
            ("20", "Staff"),
            ("20-01", "Producer"),
        ]);
        let b = c.to_budget("Feature", "USD");
        assert_eq!(Chart::from_budget(&b), c);
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        assert_eq!(r.grand_total, 0);
    }

    #[test]
    fn recode_carries_numbers_and_finds_clashes() {
        let c = chart(&[("10", "Story"), ("10-01", "Rights")]);
        let to = AccountCoding {
            category_width: 3,
            account_width: 3,
            sub_width: 0,
            set_width: 0,
            separator: None,
        };
        let r = c.recode(&to).unwrap();
        assert_eq!(r.categories().next().unwrap().accounts[0].code, "010001");
        let narrow = AccountCoding {
            category_width: 1,
            ..AccountCoding::default()
        };
        assert!(c.recode(&narrow).is_err());
    }

    #[test]
    fn applying_adds_renames_and_never_removes_lines() {
        let mut b = starter::budget_with_separator('-');
        let lines_before: usize = b
            .categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.details.len())
            .sum();
        // An empty account the chart won't list.
        b.categories_mut().next().unwrap().accounts.push(Account {
            code: "10-50".into(),
            name: "Spare".into(),
            details: Vec::new(),
            notes: Vec::new(),
        });
        let c = chart(&[
            ("10", "Story and rights"),
            ("10-01", "Screenplay"),
            ("10-02", "Rights purchase"),
            ("14", "Stunts"),
            ("14-01", "Stunt coordinator"),
            ("20", "Production staff"),
        ]);
        let mut keep = b.clone();
        let rep = apply(&mut keep, &c, &ApplyOptions::default()).unwrap();
        assert_eq!(rep.added_categories.len(), 1);
        assert_eq!(
            rep.added_accounts
                .iter()
                .map(|a| a.code.as_str())
                .collect::<Vec<_>>(),
            ["10-02", "14-01"]
        );
        assert_eq!(
            rep.renamed_accounts.len(),
            1,
            "10-01 Writing becomes Screenplay"
        );
        assert!(rep.removed_accounts.is_empty());
        assert!(
            rep.kept.iter().any(|a| a.code == "11-01"),
            "accounts with lines are kept"
        );
        assert_eq!(rep.unlisted_empty.len(), 1);
        // 14 goes after 13, before the above-the-line total.
        let pos = |b: &Budget, code: &str| {
            b.topsheet
                .iter()
                .position(|r| matches!(r, TopsheetRow::Category(c) if c.code == code))
                .unwrap()
        };
        assert_eq!(pos(&keep, "14"), pos(&keep, "13") + 1);

        let mut removing = b.clone();
        let rep = apply(
            &mut removing,
            &c,
            &ApplyOptions {
                remove_unlisted: true,
            },
        )
        .unwrap();
        assert_eq!(rep.removed_accounts.len(), 1);
        assert_eq!(rep.removed_accounts[0].code, "10-50");
        let lines_after: usize = removing
            .categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.details.len())
            .sum();
        assert_eq!(lines_after, lines_before, "no line is ever removed");
        assert_eq!(
            recalc(&removing, &RecalcOptions::default())
                .unwrap()
                .grand_total,
            recalc(&b, &RecalcOptions::default()).unwrap().grand_total
        );
    }

    #[test]
    fn applying_carries_numbers_into_the_budget_pattern() {
        let mut b = Budget::new("x", "USD");
        b.account_coding.separator = None;
        let mut c = chart(&[("11", "Producers"), ("11-01", "Producer")]);
        c.coding = AccountCoding::default();
        apply(&mut b, &c, &ApplyOptions::default()).unwrap();
        assert_eq!(b.categories().next().unwrap().accounts[0].code, "1101");
        // A category with lines stays even when unlisted and asked to remove.
        b.categories_mut().next().unwrap().accounts[0]
            .details
            .push(Detail::line(1, "x", "1", "Flat", "5"));
        let other = chart(&[("20", "Staff")]);
        let rep = apply(
            &mut b,
            &other,
            &ApplyOptions {
                remove_unlisted: true,
            },
        )
        .unwrap();
        assert!(rep.removed_categories.is_empty());
        assert_eq!(rep.kept.len(), 1);
    }
}
