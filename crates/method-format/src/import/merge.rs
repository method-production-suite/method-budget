//! Turning a mapped table into detail lines of a budget: which rows become
//! lines in which accounts, what has to be created for them, and what the
//! lines come to compared with the sheet's own totals.
//!
//! [`plan`] works everything out against a budget without changing it;
//! [`apply`] carries a plan out (as one edit: the app wraps it in one undo
//! step); [`preview`] applies it to a copy and recalculates, for the
//! dialog's preview and the reconciliation.
//!
//! How rows are read:
//!
//! - Rows after the header row; rows with nothing in any mapped column are
//!   passed over.
//! - **Accounts.** A row's account number is read with the budget's account
//!   numbering (`11-5`, `1105` and `11.05` are all `11-05` under the
//!   default pattern) and matched to an account anywhere in the budget. An
//!   empty account cell takes the account of the row above (sheets often
//!   give it once per block), and with no account column, or none yet, the
//!   default account in the options. A number that doesn't fit the
//!   numbering (too many characters or segments) is refused, never cut
//!   short, so two accounts can't be merged by accident.
//! - **Heading rows.** A row with an account or category but nothing for a
//!   line (no kind, description, amounts, tags, note or total) only sets
//!   the account (and its name, for a new account) for the rows below.
//! - **New accounts and categories** (when the options allow) are numbered
//!   by the budget's pattern; a new account's category is the category
//!   column's, else the category part of its number. Each goes after the
//!   last one numbered below it, so a budget's order holds.
//! - **Lines** are added at the end of their account, in sheet order, with
//!   new row ids. Fringe range numbers are per account in the sheet and get
//!   numbers new in the account, so they never join a range already there.
//! - **Numbers** in Amount, X, Rate and 4X are read exactly (no floats):
//!   number cells as written, text with currency signs, thousands
//!   separators, `(5)` for -5 and `10%` for 0.1. Text that isn't a number
//!   is kept as a formula when it is one this budget can work out (its
//!   globals exist); otherwise the cell is left blank and the row says so.
//! - **Names** (fringes, groups, units, sets, locations, currencies) match
//!   the budget's exactly, else ignoring case when only one does (a unit
//!   also by its plural). A name the budget lacks is added (fringes at 0%,
//!   groups in the total, currencies at a rate of 1, sets, locations and
//!   units bare) or, when the options say skip, left off the line; either
//!   way the plan lists them.
//! - **Row kinds**: line, subtotal, adjustment or separator (`Kind`
//!   column); anything else is a line, with a warning. Subtotal and
//!   separator rows keep no amounts; an adjustment keeps no rate (its rate
//!   is always the lines above it) and no currency.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use method_core::coding::AccountCoding;
use method_core::expr::Expr;
use method_core::{
    Account, Budget, Category, Cents, Currency, Dec, Detail, Formula, Fringe, FringeKind, Group,
    Note, RecalcOptions, RowId, RowKind, SetupKind, TopsheetRow, UnitDef, MAX_NOTE_CHARS,
};

use super::mapping::{cell_list, cell_money, cell_number, Field, Mapping};
use super::{Table, Value};

/// Choices beyond the mapping.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Add accounts (and categories) the budget doesn't have; otherwise
    /// their rows are skipped.
    pub create_accounts: bool,
    /// Add fringes, groups, units, sets, locations and currencies the budget
    /// doesn't have; otherwise they are left off the lines.
    pub create_setup: bool,
    /// The account for rows without one (no account column, or none above).
    pub default_account: String,
}

/// Something about one row, by its row number in the sheet (from 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowNote {
    pub row: usize,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCategory {
    pub code: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewAccount {
    pub category: String,
    pub code: String,
    pub name: String,
}

/// A row that becomes a detail line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedLine {
    /// Its row number in the sheet (from 1).
    pub row: usize,
    /// The account it goes to, as the budget writes the number.
    pub account: String,
    /// Whether that account is new.
    pub new_account: bool,
    /// The line, its id still 0 and its fringe range the sheet's number.
    pub detail: Detail,
    /// The sheet's total for it, when a total column is mapped and has one.
    pub sheet_total: Option<Cents>,
}

/// What importing a mapped table would do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub lines: Vec<PlannedLine>,
    /// Rows that won't be imported, and why.
    pub skipped: Vec<RowNote>,
    /// Rows imported with something left out or changed.
    pub warnings: Vec<RowNote>,
    /// Rows that only name an account or category for the rows below.
    pub headings: usize,
    pub new_categories: Vec<NewCategory>,
    pub new_accounts: Vec<NewAccount>,
    /// Accounts already in the budget that get lines.
    pub matched_accounts: BTreeSet<String>,
    /// Setup items to be added, by kind.
    pub added: BTreeMap<SetupKind, BTreeSet<String>>,
    /// Names left off lines because the budget lacks them (and the options
    /// say skip), with how many rows named each.
    pub dropped: BTreeMap<SetupKind, BTreeMap<String, usize>>,
}

/// The kinds of setup item an import can add, in the order the plan lists them.
pub const SETUP_KINDS: [SetupKind; 6] = [
    SetupKind::Fringe,
    SetupKind::Group,
    SetupKind::Unit,
    SetupKind::Set,
    SetupKind::Location,
    SetupKind::Currency,
];

/// An account number as the budget writes it, refusing one the pattern
/// would have to cut short.
pub fn fit_account(coding: &AccountCoding, text: &str) -> Result<String, String> {
    let t = text.trim();
    let widths = [
        coding.category_width,
        coding.account_width,
        coding.sub_width,
        coding.set_width,
    ];
    let used: Vec<usize> = widths
        .iter()
        .take_while(|w| **w > 0)
        .map(|w| usize::from(*w))
        .collect();
    let explicit = t
        .chars()
        .any(|c| method_core::coding::SEPARATORS.contains(&c));
    if explicit {
        let pieces: Vec<&str> = t
            .split(|c| method_core::coding::SEPARATORS.contains(&c))
            .filter(|p| !p.is_empty())
            .collect();
        if pieces.len() > used.len() {
            return Err("has more parts than this budget's account numbers".into());
        }
        if pieces.iter().zip(&used).any(|(p, w)| p.len() > *w) {
            return Err("is longer than this budget's account numbers".into());
        }
    } else if t.len() > used.iter().sum() {
        return Err("is longer than this budget's account numbers".into());
    }
    let normalized = coding
        .normalize(t)
        .map_err(|e| format!("isn't an account number this budget can read ({e})"))?;
    Ok(normalized)
}

/// The category part of `text` as the budget writes it.
fn category_part(coding: &AccountCoding, text: &str) -> Result<String, String> {
    let t = text.trim();
    let width = usize::from(coding.category_width);
    let first = t
        .split(|c| method_core::coding::SEPARATORS.contains(&c))
        .find(|p| !p.is_empty())
        .unwrap_or("");
    let explicit = t
        .chars()
        .any(|c| method_core::coding::SEPARATORS.contains(&c));
    if (explicit && first.len() > width) || (!explicit && t.len() > width) {
        return Err("is longer than this budget's category numbers".into());
    }
    coding
        .parse(t)
        .map(|c| c.category)
        .map_err(|e| e.to_string())
}

/// A budget name (fringe, group...) for `text`: the same name, else the one
/// name equal to it ignoring case.
fn find_name<'a>(names: impl Iterator<Item = &'a String> + Clone, text: &str) -> Option<String> {
    if let Some(n) = names.clone().find(|n| n.as_str() == text) {
        return Some(n.clone());
    }
    let mut hits =
        names.filter(|n| n.eq_ignore_ascii_case(text) || n.to_lowercase() == text.to_lowercase());
    match (hits.next(), hits.next()) {
        (Some(n), None) => Some(n.clone()),
        _ => None,
    }
}

struct Planner<'a> {
    b: &'a Budget,
    t: &'a Table,
    m: &'a Mapping,
    o: &'a Options,
    coding: AccountCoding,
    /// Normalized account number -> (the account's own number, new?).
    accounts: HashMap<String, (String, bool)>,
    /// Normalized category number -> the category's own number.
    categories: HashMap<String, String>,
    plan: Plan,
}

impl Planner<'_> {
    fn cell(&self, r: usize, f: Field) -> &Value {
        match self.m.column(f) {
            Some(c) => self.t.cell(r, c),
            None => &Value::Empty,
        }
    }

    fn text(&self, r: usize, f: Field) -> String {
        self.cell(r, f).text().trim().to_string()
    }

    fn warn(&mut self, row: usize, message: String) {
        self.plan.warnings.push(RowNote { row, message });
    }

    /// The account a line in `row` goes to, planning a new account (and
    /// category) when allowed. `Err` says why the row is skipped.
    fn account_for(
        &mut self,
        code_text: &str,
        name: &str,
        cat_text: &str,
        cat_name: &str,
    ) -> Result<(String, bool), String> {
        // A number already in the budget, as written or as the pattern
        // reads it (a chart's own numbers may not follow the pattern).
        if let Some(found) = self.accounts.get(&self.coding.key(code_text)).cloned() {
            if !found.1 {
                self.plan.matched_accounts.insert(found.0.clone());
            }
            return Ok(found);
        }
        let code = fit_account(&self.coding, code_text)
            .map_err(|e| format!("account number \"{code_text}\" {e}"))?;
        if let Some(found) = self.accounts.get(&code) {
            if !found.1 {
                self.plan.matched_accounts.insert(found.0.clone());
            }
            return Ok(found.clone());
        }
        if !self.o.create_accounts {
            return Err(format!("account {code} isn't in this budget"));
        }
        let cat = if cat_text.is_empty() {
            self.coding
                .parse(&code)
                .map(|c| c.category)
                .map_err(|e| e.to_string())?
        } else if self
            .categories
            .contains_key(&self.coding.category_key(cat_text))
        {
            self.coding.category_key(cat_text)
        } else {
            category_part(&self.coding, cat_text)
                .map_err(|e| format!("category number \"{cat_text}\" {e}"))?
        };
        if cat.is_empty() {
            return Err(format!("account {code} has no category number"));
        }
        let cat_code = match self.categories.get(&cat) {
            Some(c) => c.clone(),
            None => {
                self.plan.new_categories.push(NewCategory {
                    code: cat.clone(),
                    name: cat_name.to_string(),
                });
                self.categories.insert(cat.clone(), cat.clone());
                cat
            }
        };
        self.plan.new_accounts.push(NewAccount {
            category: cat_code,
            code: code.clone(),
            name: name.to_string(),
        });
        self.accounts.insert(code.clone(), (code.clone(), true));
        Ok((code, true))
    }

    /// A setup name for `text`: the budget's, one this plan adds, or `None`
    /// (left off the line, noted) when the options say skip.
    fn setup_name(&mut self, kind: SetupKind, text: &str) -> Option<String> {
        let b = self.b;
        let existing = match kind {
            SetupKind::Fringe => find_name(b.fringes.keys(), text),
            SetupKind::Group => find_name(b.groups.keys(), text),
            SetupKind::Set => find_name(b.sets.keys(), text),
            SetupKind::Location => find_name(b.locations.keys(), text),
            SetupKind::Unit => find_name(b.units.keys(), text).or_else(|| {
                b.units
                    .iter()
                    .find(|(_, u)| {
                        !u.plural.trim().is_empty() && u.plural.trim().eq_ignore_ascii_case(text)
                    })
                    .map(|(n, _)| n.clone())
            }),
            SetupKind::Currency => find_name(b.currencies.keys(), text),
            SetupKind::Global => None,
        };
        if existing.is_some() {
            return existing;
        }
        if let Some(n) = self
            .plan
            .added
            .get(&kind)
            .and_then(|s| find_name(s.iter(), text))
        {
            return Some(n);
        }
        if self.o.create_setup {
            self.plan
                .added
                .entry(kind)
                .or_default()
                .insert(text.to_string());
            Some(text.to_string())
        } else {
            *self
                .plan
                .dropped
                .entry(kind)
                .or_default()
                .entry(text.to_string())
                .or_default() += 1;
            None
        }
    }

    /// Amount, X, Rate or 4X: a number, a formula this budget can work out,
    /// or blank (with a warning).
    fn formula(&mut self, r: usize, f: Field) -> Formula {
        let v = self.cell(r, f).clone();
        match cell_number(&v, self.t.decimal_comma) {
            Ok(None) => Formula::default(),
            Ok(Some(d)) => Formula(d.to_string()),
            Err(_) => {
                let raw = v.text().trim();
                let src = raw.strip_prefix('=').unwrap_or(raw).trim();
                let ok = Expr::parse(src).is_ok_and(|e| {
                    let mut vars = BTreeSet::new();
                    e.vars(&mut vars);
                    vars.iter().all(|n| self.b.globals.contains_key(n))
                });
                if ok {
                    Formula(src.to_string())
                } else {
                    self.warn(
                        r + 1,
                        format!(
                            "{} \"{raw}\" isn't a number or a formula this budget can work out; left blank",
                            f.name()
                        ),
                    );
                    Formula::default()
                }
            }
        }
    }

    fn row(&mut self, r: usize, carry: &mut Carry) {
        let row_no = r + 1;
        let mapped_empty = self
            .m
            .columns
            .values()
            .all(|&c| self.t.cell(r, c).is_empty());
        if mapped_empty {
            return;
        }
        // What this row says about its account and category, carried down.
        let cat_text = self.text(r, Field::CategoryCode);
        let cat_name = self.text(r, Field::CategoryName);
        if !cat_text.is_empty() {
            carry.category = cat_text;
            carry.category_name = cat_name;
        } else if !cat_name.is_empty() {
            carry.category_name = cat_name;
        }
        let acct_text = self.text(r, Field::AccountCode);
        let acct_name = self.text(r, Field::AccountName);
        if !acct_text.is_empty() {
            carry.account = acct_text;
            carry.account_name = acct_name;
        } else if !acct_name.is_empty() {
            carry.account_name = acct_name;
        }
        // Everything a line can have but a description.
        const VALUE_FIELDS: [Field; 14] = [
            Field::Kind,
            Field::Amount,
            Field::Unit,
            Field::X,
            Field::Rate,
            Field::Currency,
            Field::X4,
            Field::Fringes,
            Field::FringeRange,
            Field::Groups,
            Field::Set,
            Field::Location,
            Field::Note,
            Field::LineTotal,
        ];
        let no_values = VALUE_FIELDS.iter().all(|&f| self.cell(r, f).is_empty());
        let description = self.text(r, Field::Description);
        // A heading: an account (or category) and nothing for a line; or an
        // account with only a description over rows that leave the account
        // blank (the description is then the account's name).
        let heading = no_values
            && (description.is_empty()
                || (acct_given(self, r) && self.next_row_leaves_account_blank(r)));
        if heading {
            if !description.is_empty() && carry.account_name.is_empty() {
                carry.account_name = description;
            }
            self.plan.headings += 1;
            return;
        }
        let code_text = if carry.account.is_empty() {
            self.o.default_account.trim().to_string()
        } else {
            carry.account.clone()
        };
        if code_text.is_empty() {
            self.plan.skipped.push(RowNote {
                row: row_no,
                message: "has no account number".into(),
            });
            return;
        }
        let (cat, cat_name, name) = (
            carry.category.clone(),
            carry.category_name.clone(),
            carry.account_name.clone(),
        );
        let (account, new_account) = match self.account_for(&code_text, &name, &cat, &cat_name) {
            Ok(a) => a,
            Err(why) => {
                self.plan.skipped.push(RowNote {
                    row: row_no,
                    message: why,
                });
                return;
            }
        };

        let kind_text = self.text(r, Field::Kind);
        let kind = match kind_text.to_ascii_lowercase().as_str() {
            "" | "line" | "detail" => RowKind::Line,
            "subtotal" | "sub total" | "sub-total" | "sub" => RowKind::Subtotal,
            "adjustment" | "adj" => RowKind::Adjustment,
            "separator" | "sep" | "rule" => RowKind::Separator,
            _ => {
                self.warn(
                    row_no,
                    format!("row kind \"{kind_text}\" isn't line, subtotal, adjustment or separator; imported as a line"),
                );
                RowKind::Line
            }
        };
        let mut d = Detail::line(0, self.text(r, Field::Description), "", "", "");
        d.kind = kind;
        d.amount = self.formula(r, Field::Amount);
        d.x = self.formula(r, Field::X);
        d.rate = self.formula(r, Field::Rate);
        d.x4 = self.formula(r, Field::X4);
        let unit = self.text(r, Field::Unit);
        if !unit.is_empty() {
            d.unit = self.setup_name(SetupKind::Unit, &unit).unwrap_or_default();
        }
        let currency = self.text(r, Field::Currency);
        if !currency.is_empty() && !currency.eq_ignore_ascii_case(&self.b.base_currency) {
            d.currency = self.setup_name(SetupKind::Currency, &currency);
        }
        for name in cell_list(self.cell(r, Field::Fringes)) {
            if let Some(n) = self.setup_name(SetupKind::Fringe, &name) {
                d.fringes.insert(n);
            }
        }
        for name in cell_list(self.cell(r, Field::Groups)) {
            if let Some(n) = self.setup_name(SetupKind::Group, &name) {
                d.groups.insert(n);
            }
        }
        let set = self.text(r, Field::Set);
        if !set.is_empty() {
            d.set = self.setup_name(SetupKind::Set, &set);
        }
        let location = self.text(r, Field::Location);
        if !location.is_empty() {
            d.location = self.setup_name(SetupKind::Location, &location);
        }
        let note = self.cell(r, Field::Note).text().trim().to_string();
        if !note.is_empty() {
            let text: String = note.chars().take(MAX_NOTE_CHARS).collect();
            d.notes = vec![Note::plain(1, text)];
        }
        let range = self.cell(r, Field::FringeRange).clone();
        match cell_number(&range, false) {
            Ok(None) => {}
            Ok(Some(n)) => match n.to_string().parse::<u32>() {
                Ok(k) if k >= 1 => d.fringe_range = Some(k),
                _ => self.warn(
                    row_no,
                    format!(
                        "fringe range \"{}\" isn't a whole number from 1; left out",
                        range.text()
                    ),
                ),
            },
            Err(_) => self.warn(
                row_no,
                format!(
                    "fringe range \"{}\" isn't a whole number from 1; left out",
                    range.text()
                ),
            ),
        }
        // What each kind of row keeps.
        match kind {
            RowKind::Line => {}
            RowKind::Adjustment => {
                d.rate = Formula::default();
                d.currency = None;
            }
            RowKind::Subtotal | RowKind::Separator => {
                d.amount = Formula::default();
                d.x = Formula::default();
                d.rate = Formula::default();
                d.x4 = Formula::default();
                d.unit.clear();
                d.currency = None;
                d.fringes.clear();
                d.fringe_range = None;
                if kind == RowKind::Separator {
                    d.description.clear();
                }
            }
        }
        let total = self.cell(r, Field::LineTotal).clone();
        let sheet_total = match cell_money(&total, self.t.decimal_comma) {
            Ok(t) => t,
            Err(_) => {
                self.warn(
                    row_no,
                    format!(
                        "line total \"{}\" isn't a number; not compared",
                        total.text()
                    ),
                );
                None
            }
        };
        self.plan.lines.push(PlannedLine {
            row: row_no,
            account,
            new_account,
            detail: d,
            sheet_total,
        });
    }
}

/// Whether row `r` gives an account number of its own.
fn acct_given(p: &Planner<'_>, r: usize) -> bool {
    !p.cell(r, Field::AccountCode).is_empty()
}

impl Planner<'_> {
    /// Whether the next row with anything mapped in it has an empty account
    /// cell (so it takes this row's account).
    fn next_row_leaves_account_blank(&self, r: usize) -> bool {
        let Some(col) = self.m.column(Field::AccountCode) else {
            return false;
        };
        ((r + 1)..self.t.rows.len())
            .find(|&n| {
                self.m
                    .columns
                    .values()
                    .any(|&c| !self.t.cell(n, c).is_empty())
            })
            .is_some_and(|n| self.t.cell(n, col).is_empty())
    }
}

/// The account and category the rows above named.
#[derive(Default)]
struct Carry {
    category: String,
    category_name: String,
    account: String,
    account_name: String,
}

/// The category number as matching compares it
/// ([`AccountCoding::category_key`]: never cut short).
fn category_key(coding: &AccountCoding, code: &str) -> String {
    coding.category_key(code)
}

/// What importing `t` with mapping `m` into `b` would do. `b` is not
/// changed.
pub fn plan(b: &Budget, t: &Table, m: &Mapping, o: &Options) -> Plan {
    let coding = b.account_coding;
    let mut accounts = HashMap::new();
    let mut categories = HashMap::new();
    for c in b.categories() {
        categories
            .entry(category_key(&coding, &c.code))
            .or_insert_with(|| c.code.clone());
        for a in &c.accounts {
            let key = coding.key(&a.code);
            accounts
                .entry(key)
                .or_insert_with(|| (a.code.clone(), false));
        }
    }
    let mut p = Planner {
        b,
        t,
        m,
        o,
        coding,
        accounts,
        categories,
        plan: Plan::default(),
    };
    let mut carry = Carry::default();
    for r in m.first_data_row()..t.rows.len() {
        p.row(r, &mut carry);
    }
    p.plan
}

/// What [`apply`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// The new lines' ids, in the plan's order.
    pub row_ids: Vec<RowId>,
    pub categories: usize,
    pub accounts: usize,
    /// Setup items added.
    pub setup: usize,
}

/// A fresh setup item of `kind` named `name`, added to `b`.
fn add_setup(b: &mut Budget, kind: SetupKind, name: &str) {
    let n = name.to_string();
    match kind {
        SetupKind::Fringe => {
            b.fringes.entry(n).or_insert_with(|| Fringe {
                description: String::new(),
                short_code: None,
                rate: Formula::from("0"),
                kind: FringeKind::Percent { cutoff: None },
            });
        }
        SetupKind::Group => {
            b.groups.entry(n).or_insert_with(|| Group {
                description: String::new(),
                short_code: None,
                in_budget_total: true,
                color: None,
            });
        }
        SetupKind::Unit => {
            b.units.entry(n).or_insert_with(|| UnitDef {
                plural: String::new(),
                description: String::new(),
                hours: None,
            });
        }
        SetupKind::Set => {
            b.sets.entry(n).or_default();
        }
        SetupKind::Location => {
            b.locations.entry(n).or_default();
        }
        SetupKind::Currency => {
            b.currencies.entry(n).or_insert_with(|| Currency {
                description: String::new(),
                symbol: String::new(),
                rate: Dec::ONE,
                format: Default::default(),
            });
        }
        SetupKind::Global => {}
    }
}

/// Carries out `plan` (made by [`plan`] against this same budget): adds the
/// setup items, categories and accounts it lists, then the lines.
pub fn apply(b: &mut Budget, plan: &Plan) -> Result<Applied, String> {
    let mut out = Applied::default();
    for (kind, names) in &plan.added {
        for n in names {
            add_setup(b, *kind, n);
            out.setup += 1;
        }
    }
    let coding = b.account_coding;
    for nc in &plan.new_categories {
        let key = category_key(&coding, &nc.code);
        if b.categories()
            .any(|c| category_key(&coding, &c.code) == key)
        {
            continue;
        }
        // After the last category numbered below it, else before the first.
        let at = b
            .topsheet
            .iter()
            .rposition(
                |r| matches!(r, TopsheetRow::Category(c) if category_key(&coding, &c.code) < key),
            )
            .map(|i| i + 1)
            .or_else(|| {
                b.topsheet
                    .iter()
                    .position(|r| matches!(r, TopsheetRow::Category(_)))
            })
            .unwrap_or(b.topsheet.len());
        b.topsheet.insert(
            at,
            TopsheetRow::Category(Category {
                code: nc.code.clone(),
                name: nc.name.clone(),
                accounts: Vec::new(),
                notes: Vec::new(),
            }),
        );
        out.categories += 1;
    }
    for na in &plan.new_accounts {
        let key = category_key(&coding, &na.category);
        let cat = b
            .categories_mut()
            .find(|c| category_key(&coding, &c.code) == key)
            .ok_or_else(|| format!("category {} is missing", na.category))?;
        if cat.accounts.iter().any(|a| a.code == na.code) {
            continue;
        }
        let at = cat
            .accounts
            .iter()
            .rposition(|a| coding.key(&a.code) < na.code)
            .map_or(0, |i| i + 1);
        cat.accounts.insert(
            at,
            Account {
                code: na.code.clone(),
                name: na.name.clone(),
                details: Vec::new(),
                notes: Vec::new(),
            },
        );
        out.accounts += 1;
    }
    let mut next_id = b.highest_row_id().unwrap_or(0);
    // Per account: the range numbers in use, and the sheet's numbers mapped.
    let mut ranges: HashMap<String, (BTreeSet<u32>, BTreeMap<u32, u32>)> = HashMap::new();
    for line in &plan.lines {
        let acct = b
            .categories_mut()
            .flat_map(|c| c.accounts.iter_mut())
            .find(|a| a.code == line.account)
            .ok_or_else(|| format!("account {} is missing", line.account))?;
        next_id = next_id.checked_add(1).ok_or("too many rows")?;
        let mut d = line.detail.clone();
        d.id = next_id;
        if let Some(n) = d.fringe_range {
            let (used, map) = ranges.entry(line.account.clone()).or_insert_with(|| {
                (
                    acct.details.iter().filter_map(|d| d.fringe_range).collect(),
                    BTreeMap::new(),
                )
            });
            let m = match map.get(&n) {
                Some(&m) => m,
                None => {
                    let m = (1..=u32::MAX)
                        .find(|k| !used.contains(k))
                        .ok_or("too many fringe ranges")?;
                    used.insert(m);
                    map.insert(n, m);
                    m
                }
            };
            d.fringe_range = Some(m);
        }
        acct.details.push(d);
        out.row_ids.push(next_id);
    }
    Ok(out)
}

/// One line whose total differs from the sheet's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    pub row: usize,
    pub account: String,
    pub description: String,
    pub sheet: Cents,
    pub computed: Cents,
}

/// The imported lines' totals against the sheet's total column.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reconciliation {
    /// Lines with a total in the sheet.
    pub compared: usize,
    /// How many of those differ.
    pub mismatched: usize,
    /// The sheet's totals, and what the lines come to, over the compared lines.
    pub sheet_sum: Cents,
    pub computed_sum: Cents,
    /// The first differing lines, in sheet order.
    pub first: Vec<Mismatch>,
}

/// How many differing lines a reconciliation lists.
pub const MISMATCHES_LISTED: usize = 50;

/// A plan with what it comes to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preview {
    pub plan: Plan,
    /// What each planned line comes to once imported (base currency, its
    /// running total for a subtotal row), when the budget recalculates.
    pub totals: Vec<Option<Cents>>,
    /// Present when a line total column is mapped.
    pub reconciliation: Option<Reconciliation>,
    pub grand_total_before: Option<Cents>,
    pub grand_total_after: Option<Cents>,
    /// Why the totals couldn't be worked out, if they couldn't.
    pub calc_error: Option<String>,
}

/// Plans the import, applies it to a copy of `b` and recalculates, for the
/// dialog: the plan, each line's total, the reconciliation and the grand
/// total before and after.
pub fn preview(b: &Budget, t: &Table, m: &Mapping, o: &Options) -> Preview {
    let plan = plan(b, t, m, o);
    let mut out = Preview {
        grand_total_before: method_core::recalc(b, &RecalcOptions::default())
            .ok()
            .map(|r| r.grand_total),
        ..Preview::default()
    };
    let mut copy = b.clone();
    let applied = match apply(&mut copy, &plan) {
        Ok(a) => a,
        Err(e) => {
            out.calc_error = Some(e);
            out.totals = vec![None; plan.lines.len()];
            out.plan = plan;
            return out;
        }
    };
    match method_core::recalc(&copy, &RecalcOptions::default()) {
        Ok(report) => {
            out.grand_total_after = Some(report.grand_total);
            let mut by_id: HashMap<RowId, Cents> = HashMap::new();
            for rr in report
                .categories
                .iter()
                .flat_map(|c| &c.accounts)
                .flat_map(|a| &a.rows)
            {
                let v = if rr.kind == RowKind::Subtotal {
                    rr.subtotal.unwrap_or(0)
                } else {
                    rr.total
                };
                by_id.insert(rr.id, v);
            }
            out.totals = applied
                .row_ids
                .iter()
                .map(|id| by_id.get(id).copied())
                .collect();
            if m.column(Field::LineTotal).is_some() {
                let mut rec = Reconciliation::default();
                for (line, total) in plan.lines.iter().zip(&out.totals) {
                    let (Some(sheet), Some(computed)) = (line.sheet_total, *total) else {
                        continue;
                    };
                    rec.compared += 1;
                    rec.sheet_sum = rec.sheet_sum.saturating_add(sheet);
                    rec.computed_sum = rec.computed_sum.saturating_add(computed);
                    if sheet != computed {
                        rec.mismatched += 1;
                        if rec.first.len() < MISMATCHES_LISTED {
                            rec.first.push(Mismatch {
                                row: line.row,
                                account: line.account.clone(),
                                description: line.detail.description.clone(),
                                sheet,
                                computed,
                            });
                        }
                    }
                }
                out.reconciliation = Some(rec);
            }
        }
        Err(e) => {
            out.calc_error = Some(e.to_string());
            out.totals = vec![None; plan.lines.len()];
        }
    }
    out.plan = plan;
    out
}

/// Plans and applies in one go: what the app runs, inside one undo step.
pub fn import(
    b: &mut Budget,
    t: &Table,
    m: &Mapping,
    o: &Options,
) -> Result<(Plan, Applied), String> {
    let p = plan(b, t, m, o);
    if p.lines.is_empty() {
        return Err("No rows to import: map the columns, or check the skipped rows.".into());
    }
    let applied = apply(b, &p)?;
    Ok((p, applied))
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::starter;

    fn table(rows: &[&[&str]]) -> Table {
        let mut t = Table::default();
        for (r, row) in rows.iter().enumerate() {
            for (c, s) in row.iter().enumerate() {
                if !s.is_empty() {
                    t.put(r, c, Value::Text(s.to_string()));
                }
            }
        }
        t
    }

    fn mapping(fields: &[Field]) -> Mapping {
        Mapping {
            header_row: Some(0),
            columns: fields.iter().enumerate().map(|(i, f)| (*f, i)).collect(),
        }
    }

    fn opts(create: bool) -> Options {
        Options {
            create_accounts: create,
            create_setup: create,
            default_account: String::new(),
        }
    }

    fn account<'a>(b: &'a Budget, code: &str) -> &'a Account {
        b.categories()
            .flat_map(|c| &c.accounts)
            .find(|a| a.code == code)
            .unwrap_or_else(|| panic!("no account {code}"))
    }

    #[test]
    fn lines_go_to_matching_accounts_with_carried_accounts() {
        let b = starter::budget_with_separator('-');
        let t = table(&[
            &["Acct", "Description", "Qty", "Rate", "Total"],
            &["10-01", "Rewrite", "1", "$2,500.00", "2500"],
            &["", "Polish", "2", "500", "1000"],
            &["1101", "Associate producer", "10", "800", "8000"],
        ]);
        use Field::*;
        let m = mapping(&[AccountCode, Description, Amount, Rate, LineTotal]);
        let p = preview(&b, &t, &m, &opts(false));
        assert!(p.plan.skipped.is_empty(), "{:?}", p.plan.skipped);
        assert_eq!(p.plan.lines.len(), 3);
        assert_eq!(p.plan.lines[1].account, "10-01", "carried down");
        assert_eq!(
            p.plan.lines[2].account, "11-01",
            "1101 read with the numbering"
        );
        assert_eq!(p.totals, vec![Some(250_000), Some(100_000), Some(800_000)]);
        let rec = p.reconciliation.unwrap();
        assert_eq!((rec.compared, rec.mismatched), (3, 0));
        assert_eq!(rec.sheet_sum, rec.computed_sum);

        let mut b2 = b.clone();
        let before = account(&b, "10-01").details.len();
        let (_, applied) = import(&mut b2, &t, &m, &opts(false)).unwrap();
        let a = account(&b2, "10-01");
        assert_eq!(a.details.len(), before + 2, "appended");
        assert_eq!(a.details[before].description, "Rewrite");
        assert_eq!(a.details[before].rate.0, "2500");
        assert_eq!(applied.row_ids.len(), 3);
        let max_before = b.highest_row_id().unwrap();
        assert!(applied.row_ids.iter().all(|&id| id > max_before));
    }

    #[test]
    fn reconciliation_lists_mismatches() {
        let b = starter::budget_with_separator('-');
        let t = table(&[
            &["Acct", "Description", "Qty", "Rate", "Total"],
            &["10-01", "Right", "2", "10", "20"],
            &["10-01", "Wrong", "3", "10", "31.50"],
            &["10-01", "No total", "1", "5", ""],
        ]);
        use Field::*;
        let p = preview(
            &b,
            &t,
            &mapping(&[AccountCode, Description, Amount, Rate, LineTotal]),
            &opts(false),
        );
        let rec = p.reconciliation.unwrap();
        assert_eq!(rec.compared, 2);
        assert_eq!(rec.mismatched, 1);
        assert_eq!(rec.sheet_sum - rec.computed_sum, 150);
        assert_eq!(rec.first[0].row, 3);
        assert_eq!((rec.first[0].sheet, rec.first[0].computed), (3150, 3000));
        // Without a total column, no reconciliation.
        let p = preview(
            &b,
            &t,
            &mapping(&[AccountCode, Description, Amount, Rate]),
            &opts(false),
        );
        assert!(p.reconciliation.is_none());
    }

    #[test]
    fn missing_accounts_and_setup_are_created_or_skipped() {
        let b = starter::budget_with_separator('-');
        let t = table(&[
            &[
                "Cat",
                "Cat name",
                "Acct",
                "Acct name",
                "Description",
                "Qty",
                "Rate",
                "Fringes",
                "Groups",
                "Unit",
                "Cur",
                "Set",
            ],
            &[
                "",
                "",
                "45-02",
                "Grip rentals",
                "Dolly",
                "5",
                "300",
                "payroll, WC",
                "Tax credit",
                "days",
                "CAD",
                "Stage",
            ],
            &[
                "", "", "", "", "Track", "5", "100", "", "", "Week", "usd", "",
            ],
            &["", "", "45-02", "", "", "", "", "", "", "", "", ""],
            &[
                "77", "Post", "77-03", "Color", "Grade", "1", "9000", "", "", "", "", "",
            ],
        ]);
        use Field::*;
        let m = mapping(&[
            CategoryCode,
            CategoryName,
            AccountCode,
            AccountName,
            Description,
            Amount,
            Rate,
            Fringes,
            Groups,
            Unit,
            Currency,
            Set,
        ]);

        // Skip: the rows for unknown accounts are skipped, nothing added.
        let p = plan(&b, &t, &m, &opts(false));
        assert_eq!(p.lines.len(), 0);
        assert_eq!(p.skipped.len(), 3);
        assert!(p.skipped[0].message.contains("45-02 isn't in this budget"));
        assert_eq!(p.headings, 1);

        // Create: a new account in a new category, the setup added.
        let p = plan(&b, &t, &m, &opts(true));
        assert!(p.skipped.is_empty(), "{:?}", p.skipped);
        assert_eq!(p.lines.len(), 3);
        assert_eq!(
            p.new_categories,
            vec![
                NewCategory {
                    code: "45".into(),
                    name: String::new()
                },
                NewCategory {
                    code: "77".into(),
                    name: "Post".into()
                }
            ]
        );
        assert_eq!(p.new_accounts.len(), 2);
        assert_eq!(p.new_accounts[0].name, "Grip rentals");
        // PAYROLL exists (case ignored); WC and the rest are new.
        assert_eq!(
            p.added[&SetupKind::Fringe],
            BTreeSet::from(["WC".to_string()])
        );
        assert_eq!(
            p.lines[0].detail.fringes,
            BTreeSet::from(["PAYROLL".to_string(), "WC".to_string()])
        );
        // A plural unit finds its unit; the base currency is no currency.
        assert_eq!(p.lines[0].detail.unit, "Day");
        assert_eq!(p.lines[1].detail.unit, "Week");
        assert_eq!(p.lines[1].detail.currency, None);
        assert_eq!(
            p.added[&SetupKind::Currency],
            BTreeSet::from(["CAD".to_string()])
        );

        let mut b2 = b.clone();
        let (_, applied) = import(&mut b2, &t, &m, &opts(true)).unwrap();
        assert_eq!((applied.categories, applied.accounts), (2, 2));
        assert_eq!(account(&b2, "45-02").details.len(), 2);
        assert!(b2.fringes.contains_key("WC") && b2.groups.contains_key("Tax credit"));
        assert_eq!(b2.currencies["CAD"].rate, Dec::ONE);
        // Categories in number order: 45 after the last below it.
        let codes: Vec<&str> = b2.categories().map(|c| c.code.as_str()).collect();
        let mut sorted = codes.clone();
        sorted.sort();
        assert_eq!(codes, sorted);
        method_core::recalc(&b2, &RecalcOptions::default()).unwrap();

        // Skip setup only: accounts made, unknown tags left off and listed.
        let o = Options {
            create_accounts: true,
            create_setup: false,
            default_account: String::new(),
        };
        let p = plan(&b, &t, &m, &o);
        assert_eq!(
            p.lines[0].detail.fringes,
            BTreeSet::from(["PAYROLL".to_string()])
        );
        assert!(p.lines[0].detail.groups.is_empty());
        assert_eq!(p.lines[0].detail.currency, None);
        assert_eq!(p.dropped[&SetupKind::Fringe]["WC"], 1);
        assert_eq!(p.dropped[&SetupKind::Set]["Stage"], 1);
    }

    #[test]
    fn numbers_formulas_and_kinds() {
        let mut b = starter::budget_with_separator('-');
        b.globals.insert("DAYS".into(), "20".into());
        let t = table(&[
            &[
                "Acct",
                "Kind",
                "Description",
                "Qty",
                "Rate",
                "Range",
                "Note",
            ],
            &[
                "10-01",
                "",
                "Writer",
                "=DAYS",
                "(1,000)",
                "3",
                "From the old budget",
            ],
            &["10-01", "line", "Bad", "TBD", "UNKNOWN * 2", "x", ""],
            &["10-01", "Subtotal", "Sub", "4", "5", "", ""],
            &["10-01", "Adjustment", "Contingency", "10%", "999", "", ""],
            &["10-01", "Separator", "ignored", "", "", "", ""],
            &["10-01", "Mystery", "Odd", "1", "1", "3", ""],
        ]);
        use Field::*;
        let m = mapping(&[
            AccountCode,
            Kind,
            Description,
            Amount,
            Rate,
            FringeRange,
            Note,
        ]);
        let p = plan(&b, &t, &m, &opts(false));
        let l = &p.lines;
        assert_eq!(l[0].detail.amount.0, "DAYS");
        assert_eq!(l[0].detail.rate.0, "-1000");
        assert_eq!(l[0].detail.fringe_range, Some(3));
        assert_eq!(l[0].detail.notes[0].text, "From the old budget");
        assert!(l[1].detail.amount.is_blank() && l[1].detail.rate.is_blank());
        assert_eq!(l[2].detail.kind, RowKind::Subtotal);
        assert!(l[2].detail.amount.is_blank());
        assert_eq!(l[3].detail.kind, RowKind::Adjustment);
        assert_eq!(l[3].detail.amount.0, "0.1");
        assert!(l[3].detail.rate.is_blank());
        assert_eq!(l[4].detail.kind, RowKind::Separator);
        assert!(l[4].detail.description.is_empty());
        assert_eq!(l[5].detail.kind, RowKind::Line);
        let rows: Vec<usize> = p.warnings.iter().map(|w| w.row).collect();
        assert_eq!(rows, [3, 3, 3, 7], "{:?}", p.warnings);

        // Ranges get numbers new in the account.
        let mut b2 = b.clone();
        let acct = b2.categories_mut().next().unwrap();
        acct.accounts[0].details[0].fringe_range = Some(1);
        let (_, applied) = import(&mut b2, &t, &m, &opts(false)).unwrap();
        let a = account(&b2, "10-01");
        let ranges: Vec<Option<u32>> = applied
            .row_ids
            .iter()
            .map(|id| a.details.iter().find(|d| d.id == *id).unwrap().fringe_range)
            .collect();
        assert_eq!(ranges, [Some(2), None, None, None, None, Some(2)]);
    }

    #[test]
    fn account_headings_name_new_accounts() {
        let b = starter::budget_with_separator('-');
        let t = table(&[
            &["Acct", "Description", "Qty", "Rate"],
            &["55-02", "Camera package", "", ""],
            &["", "Body", "2", "450"],
            &["55-03", "Title only line", "", ""],
            &["55-03", "Another", "1", "5"],
            &["", "", "", ""],
            &["55-04", "Last row title", "", ""],
        ]);
        use Field::*;
        let p = plan(
            &b,
            &t,
            &mapping(&[AccountCode, Description, Amount, Rate]),
            &opts(true),
        );
        assert_eq!(p.headings, 1);
        assert_eq!(p.new_accounts[0].code, "55-02");
        assert_eq!(p.new_accounts[0].name, "Camera package");
        let descs: Vec<&str> = p
            .lines
            .iter()
            .map(|l| l.detail.description.as_str())
            .collect();
        assert_eq!(
            descs,
            ["Body", "Title only line", "Another", "Last row title"]
        );
    }

    #[test]
    fn default_account_and_bad_numbers() {
        let b = starter::budget_with_separator('-');
        let t = table(&[
            &["Acct", "Description", "Rate"],
            &["", "Coffee", "40"],
            &["1-2-3", "Too many parts", "1"],
            &["100100", "Too long", "1"],
        ]);
        use Field::*;
        let m = mapping(&[AccountCode, Description, Rate]);
        let p = plan(&b, &t, &m, &opts(true));
        assert_eq!(p.skipped[0].message, "has no account number");
        assert!(p.skipped[1].message.contains("1-2-3"), "{:?}", p.skipped);
        assert!(p.skipped[2].message.contains("longer"), "{:?}", p.skipped);
        let o = Options {
            default_account: "10-01".into(),
            ..opts(false)
        };
        let p = plan(&b, &t, &m, &o);
        assert_eq!(p.lines.len(), 1);
        assert_eq!(p.lines[0].account, "10-01");
        // Nothing to import is an error, and changes nothing.
        let mut b2 = b.clone();
        assert!(import(
            &mut b2,
            &table(&[&["Acct"]]),
            &mapping(&[AccountCode]),
            &opts(true)
        )
        .is_err());
        assert_eq!(b2, b);
    }
}
