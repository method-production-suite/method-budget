//! How each model type is spread over an entity's fields and sets
//! (docs/research/collaboration.md, 3.5), and put back together.
//!
//! Encoding needs a [`Binder`] to turn names into ids; decoding needs a
//! [`Resolver`] to turn ids back into the names they have now. Decoders
//! are total: a missing or oddly typed field reads as its default, so a
//! log written by a newer build still projects.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::formula;
use super::ids::Uid;
use super::op::{Field, Kind, Member, Value};
use super::state::Entity;
use crate::actuals::{BatchId, EtcMethod, ImportBatch, Transaction};
use crate::model::{
    Account, Baseline, Budget, Category, Charge, ChargeKind, Credit, CreditCap, CreditRow,
    CreditSource, Currency, Detail, DisplayPrefs, FlatBasis, Formula, Fringe, FringeKind,
    GlobalInfo, Group, InfoValue, Location, Note, ProductionTotal, RowId, RowKind, SetDef, UnitDef,
    Version,
};
use crate::{Arithmetic, CurrencyFormat, CutoffFactor, Dec, Separators, TrailingFringes};

/// An entity's fields and sets as one record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    pub fields: BTreeMap<Field, Value>,
    pub sets: BTreeMap<Field, BTreeSet<Member>>,
}

impl Record {
    fn put(&mut self, f: Field, v: Value) -> &mut Record {
        self.fields.insert(f, v);
        self
    }

    fn tags(&mut self, f: Field, members: BTreeSet<Member>) -> &mut Record {
        self.sets.insert(f, members);
        self
    }

    /// The record's fields and sets as an entity holds them now.
    pub fn of(e: &Entity) -> Record {
        Record {
            fields: e
                .fields
                .iter()
                .map(|(f, r)| (*f, r.value().clone()))
                .collect(),
            sets: e
                .sets
                .iter()
                .map(|(f, s)| (*f, s.members().cloned().collect()))
                .collect(),
        }
    }

    /// Everything but the name: what "the same item" means for two items
    /// with one name.
    pub fn without_name(&self) -> Record {
        let mut r = self.clone();
        r.fields.remove(&Field::Name);
        r
    }
}

/// Names to ids, for encoding.
pub trait Binder {
    /// A named entity (setup items, info fields, charges, credits).
    fn named(&self, kind: Kind, name: &str) -> Option<Uid>;
    fn account(&self, code: &str) -> Option<Uid>;
    fn category(&self, code: &str) -> Option<Uid>;
    fn row(&self, id: RowId) -> Option<Uid>;
    fn batch(&self, id: BatchId) -> Option<Uid>;
}

/// Ids to the names they have now, for decoding.
pub trait Resolver {
    /// The name, code or id (as text) the entity shows with.
    fn name(&self, uid: Uid) -> Option<String>;
    fn row_id(&self, uid: Uid) -> Option<RowId>;
    fn batch_id(&self, uid: Uid) -> Option<BatchId>;
}

// ---------------------------------------------------------------------------
// Encoding helpers

fn named_or_text(b: &dyn Binder, kind: Kind, name: &str) -> Value {
    if name.is_empty() {
        return Value::text("");
    }
    match b.named(kind, name) {
        Some(u) => Value::Ref(u),
        None => Value::text(name),
    }
}

fn opt_named(b: &dyn Binder, kind: Kind, name: Option<&str>) -> Value {
    match name {
        None => Value::Null,
        Some(n) => named_or_text(b, kind, n),
    }
}

fn member(b: &dyn Binder, kind: Kind, name: &str) -> Member {
    match b.named(kind, name) {
        Some(u) => Member::Ref(u),
        None => Member::Text(name.to_string()),
    }
}

fn members(b: &dyn Binder, kind: Kind, names: &BTreeSet<String>) -> BTreeSet<Member> {
    names.iter().map(|n| member(b, kind, n)).collect()
}

fn bound(b: &dyn Binder, f: &Formula) -> Value {
    Value::Formula(formula::bind(&f.0, &|n| b.named(Kind::Global, n)))
}

// ---------------------------------------------------------------------------
// Decoding helpers

fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_text).unwrap_or_default().to_string()
}

fn opt_text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_text).map(str::to_string)
}

fn boolean(v: Option<&Value>) -> bool {
    v.and_then(Value::as_bool).unwrap_or(false)
}

fn int(v: Option<&Value>) -> i64 {
    v.and_then(Value::as_int).unwrap_or(0)
}

fn opt_int(v: Option<&Value>) -> Option<i64> {
    v.and_then(Value::as_int)
}

fn dec(v: Option<&Value>) -> Dec {
    v.and_then(Value::as_dec).unwrap_or(Dec::ZERO)
}

fn opt_dec(v: Option<&Value>) -> Option<Dec> {
    v.and_then(Value::as_dec)
}

/// A reference or free text, as text; nothing for null or an id that
/// resolves to nothing.
fn ref_text(r: &dyn Resolver, v: Option<&Value>) -> String {
    match v {
        Some(Value::Ref(u)) => r.name(*u).unwrap_or_default(),
        Some(Value::Text(s)) => s.clone(),
        _ => String::new(),
    }
}

fn ref_opt_text(r: &dyn Resolver, v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::Ref(u)) => r.name(*u),
        Some(Value::Text(s)) => Some(s.clone()),
        _ => None,
    }
}

fn unbound(r: &dyn Resolver, v: Option<&Value>) -> Formula {
    match v {
        Some(Value::Formula(f)) => Formula(formula::unbind(f, &|u| r.name(u))),
        Some(Value::Text(s)) => Formula(s.clone()),
        _ => Formula::default(),
    }
}

fn names(r: &dyn Resolver, e: &Entity, f: Field) -> BTreeSet<String> {
    e.set_members(f)
        .into_iter()
        .filter_map(|m| match m {
            Member::Ref(u) => r.name(*u),
            Member::Text(s) => Some(s.clone()),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The budget's own fields

pub fn encode_root(b: &Budget) -> Record {
    let mut r = Record::default();
    r.put(Field::Title, Value::text(&b.title))
        .put(Field::BaseCurrency, Value::text(&b.base_currency))
        .put(
            Field::TotalsCurrency,
            Value::opt_text(b.totals_currency.as_deref()),
        )
        .put(
            Field::FringePosting,
            Value::Posting(b.fringe_posting.clone()),
        )
        .put(
            Field::TrailingLevelFringes,
            Value::text(b.trailing_level_fringes.name()),
        )
        .put(Field::CutoffFactor, Value::text(b.cutoff_factor.name()))
        .put(Field::FringeRow, Value::FringeRow(b.fringe_row.clone()))
        .put(Field::Arithmetic, Value::text(b.arithmetic.name()))
        .put(
            Field::RoundAccountTotals,
            Value::Bool(b.round_account_totals),
        )
        .put(Field::AccountCoding, Value::Coding(b.account_coding))
        .put(
            Field::NewLineDefaults,
            Value::NewLineDefaults(b.new_line_defaults),
        )
        .put(Field::GroupRules, Value::GroupRules(b.group_rules))
        .put(Field::Display, Value::Display(b.display.clone()))
        .put(Field::CompactVersions, Value::Bool(b.compact_versions))
        .put(
            Field::ReportPresets,
            Value::ReportPresets(b.report_presets.clone()),
        )
        .put(Field::TagPresets, Value::TagPresets(b.tag_presets.clone()))
        .put(
            Field::LinePresets,
            Value::LinePresets(b.line_presets.clone()),
        )
        .put(Field::SetupOrder, Value::SetupOrder(b.setup_order.clone()))
        .put(
            Field::SetupFolders,
            Value::SetupFolders(b.setup_folders.clone()),
        )
        .put(
            Field::SetupSources,
            Value::SetupSources(b.setup_sources.clone()),
        )
        .put(Field::OtPatterns, Value::OtPatterns(b.ot_patterns.clone()))
        .put(
            Field::Baseline,
            match &b.baseline {
                Some(base) => Value::Baseline(base.clone()),
                None => Value::Null,
            },
        );
    r
}

/// Fills the budget's settings and tools from the root entity. Everything
/// structural (setup maps, topsheet, charges...) is left as it is.
pub fn decode_root(e: &Entity, b: &mut Budget) {
    let g = |f: Field| e.get(f);
    b.title = text(g(Field::Title));
    b.base_currency = text(g(Field::BaseCurrency));
    b.totals_currency = opt_text(g(Field::TotalsCurrency));
    if let Some(Value::Posting(p)) = g(Field::FringePosting) {
        b.fringe_posting = p.clone();
    }
    b.trailing_level_fringes =
        TrailingFringes::parse(&text(g(Field::TrailingLevelFringes))).unwrap_or_default();
    b.cutoff_factor = CutoffFactor::parse(&text(g(Field::CutoffFactor))).unwrap_or_default();
    if let Some(Value::FringeRow(f)) = g(Field::FringeRow) {
        b.fringe_row = f.clone();
    }
    b.arithmetic = Arithmetic::parse(&text(g(Field::Arithmetic))).unwrap_or_default();
    b.round_account_totals = boolean(g(Field::RoundAccountTotals));
    if let Some(Value::Coding(c)) = g(Field::AccountCoding) {
        b.account_coding = *c;
    }
    if let Some(Value::NewLineDefaults(n)) = g(Field::NewLineDefaults) {
        b.new_line_defaults = *n;
    }
    if let Some(Value::GroupRules(r)) = g(Field::GroupRules) {
        b.group_rules = *r;
    }
    b.display = match g(Field::Display) {
        Some(Value::Display(d)) => d.clone(),
        _ => DisplayPrefs::default(),
    };
    b.compact_versions = boolean(g(Field::CompactVersions));
    b.report_presets = match g(Field::ReportPresets) {
        Some(Value::ReportPresets(p)) => p.clone(),
        _ => Vec::new(),
    };
    b.tag_presets = match g(Field::TagPresets) {
        Some(Value::TagPresets(p)) => p.clone(),
        _ => Vec::new(),
    };
    b.line_presets = match g(Field::LinePresets) {
        Some(Value::LinePresets(p)) => p.clone(),
        _ => Vec::new(),
    };
    b.setup_order = match g(Field::SetupOrder) {
        Some(Value::SetupOrder(o)) => o.clone(),
        _ => Default::default(),
    };
    b.setup_folders = match g(Field::SetupFolders) {
        Some(Value::SetupFolders(f)) => f.clone(),
        _ => Default::default(),
    };
    b.setup_sources = match g(Field::SetupSources) {
        Some(Value::SetupSources(s)) => s.clone(),
        _ => Default::default(),
    };
    b.ot_patterns = match g(Field::OtPatterns) {
        Some(Value::OtPatterns(p)) => p.clone(),
        _ => Vec::new(),
    };
    b.baseline = match g(Field::Baseline) {
        Some(Value::Baseline(base)) => Some(base.clone()),
        _ => None,
    };
}

// ---------------------------------------------------------------------------
// Topsheet

pub fn encode_category(c: &Category) -> Record {
    let mut r = Record::default();
    r.put(Field::Code, Value::text(&c.code))
        .put(Field::Name, Value::text(&c.name));
    r
}

pub fn decode_category(e: &Entity) -> Category {
    Category {
        code: text(e.get(Field::Code)),
        name: text(e.get(Field::Name)),
        accounts: Vec::new(),
        notes: Vec::new(),
    }
}

pub fn encode_production_total(p: &ProductionTotal) -> Record {
    let mut r = Record::default();
    r.put(Field::Name, Value::text(&p.name));
    r
}

pub fn decode_production_total(e: &Entity) -> ProductionTotal {
    ProductionTotal {
        name: text(e.get(Field::Name)),
    }
}

pub fn encode_account(a: &Account) -> Record {
    let mut r = Record::default();
    r.put(Field::Code, Value::text(&a.code))
        .put(Field::Name, Value::text(&a.name));
    r
}

pub fn decode_account(e: &Entity) -> Account {
    Account {
        code: text(e.get(Field::Code)),
        name: text(e.get(Field::Name)),
        details: Vec::new(),
        notes: Vec::new(),
    }
}

fn row_kind_name(k: RowKind) -> &'static str {
    match k {
        RowKind::Line => "line",
        RowKind::Subtotal => "subtotal",
        RowKind::Adjustment => "adjustment",
        RowKind::Separator => "separator",
    }
}

fn row_kind(s: &str) -> RowKind {
    match s {
        "subtotal" => RowKind::Subtotal,
        "adjustment" => RowKind::Adjustment,
        "separator" => RowKind::Separator,
        _ => RowKind::Line,
    }
}

pub fn encode_row(d: &Detail, b: &dyn Binder) -> Record {
    let mut r = Record::default();
    r.put(Field::Id, Value::Int(d.id as i64))
        .put(Field::Kind, Value::text(row_kind_name(d.kind)))
        .put(Field::Description, Value::text(&d.description))
        .put(Field::Amount, bound(b, &d.amount))
        .put(Field::Unit, named_or_text(b, Kind::Unit, &d.unit))
        .put(Field::X, bound(b, &d.x))
        .put(Field::Unit2, named_or_text(b, Kind::Unit, &d.unit2))
        .put(Field::Rate, bound(b, &d.rate))
        .put(Field::Unit3, named_or_text(b, Kind::Unit, &d.unit3))
        .put(Field::X4, bound(b, &d.x4))
        .put(Field::Unit4, named_or_text(b, Kind::Unit, &d.unit4))
        .put(
            Field::Currency,
            opt_named(b, Kind::Currency, d.currency.as_deref()),
        )
        .put(
            Field::Location,
            opt_named(b, Kind::Location, d.location.as_deref()),
        )
        .put(Field::Set, opt_named(b, Kind::Set, d.set.as_deref()))
        .put(
            Field::FringeRange,
            Value::opt_int(d.fringe_range.map(i64::from)),
        )
        .put(Field::Bold, Value::Bool(d.bold))
        .put(Field::Italic, Value::Bool(d.italic))
        .put(Field::Indent, Value::Int(i64::from(d.indent)))
        .put(Field::Color, Value::opt_text(d.color.as_deref()))
        .tags(Field::Fringes, members(b, Kind::Fringe, &d.fringes))
        .tags(Field::Groups, members(b, Kind::Group, &d.groups));
    // Only when set, as the `.mthd` writes it (decisions.md, entry 79): a
    // log of a budget that never uses it stays readable by an app without
    // the field. Clearing it is a write of nothing (see `Capture::diff`).
    if let Some(p) = &d.person {
        r.put(Field::Person, Value::text(p));
    }
    r
}

pub fn decode_row(e: &Entity, r: &dyn Resolver, id: RowId) -> Detail {
    let g = |f: Field| e.get(f);
    Detail {
        id,
        kind: row_kind(&text(g(Field::Kind))),
        description: text(g(Field::Description)),
        amount: unbound(r, g(Field::Amount)),
        unit: ref_text(r, g(Field::Unit)),
        x: unbound(r, g(Field::X)),
        unit2: ref_text(r, g(Field::Unit2)),
        rate: unbound(r, g(Field::Rate)),
        unit3: ref_text(r, g(Field::Unit3)),
        x4: unbound(r, g(Field::X4)),
        unit4: ref_text(r, g(Field::Unit4)),
        currency: ref_opt_text(r, g(Field::Currency)),
        fringes: names(r, e, Field::Fringes),
        groups: names(r, e, Field::Groups),
        location: ref_opt_text(r, g(Field::Location)),
        set: ref_opt_text(r, g(Field::Set)),
        fringe_range: opt_int(g(Field::FringeRange)).and_then(|n| u32::try_from(n).ok()),
        person: opt_text(g(Field::Person)),
        bold: boolean(g(Field::Bold)),
        italic: boolean(g(Field::Italic)),
        indent: u8::try_from(int(g(Field::Indent))).unwrap_or(0),
        color: opt_text(g(Field::Color)),
        notes: Vec::new(),
    }
}

pub fn encode_note(n: &Note) -> Record {
    let mut r = Record::default();
    r.put(Field::Id, Value::Int(n.id as i64))
        .put(Field::Text, Value::text(&n.text))
        .put(Field::Created, Value::text(&n.created))
        .put(Field::Edited, Value::opt_text(n.edited.as_deref()))
        .put(Field::Author, Value::opt_text(n.author.as_deref()))
        .put(Field::Done, Value::Bool(n.done));
    r
}

pub fn decode_note(e: &Entity, id: u64) -> Note {
    let g = |f: Field| e.get(f);
    Note {
        id,
        text: text(g(Field::Text)),
        created: text(g(Field::Created)),
        edited: opt_text(g(Field::Edited)),
        author: opt_text(g(Field::Author)),
        done: boolean(g(Field::Done)),
    }
}

// ---------------------------------------------------------------------------
// Setup items

pub fn encode_info(name: &str, v: &InfoValue) -> Record {
    let mut r = Record::default();
    r.put(Field::Name, Value::text(name))
        .put(Field::Value, Value::Info(v.clone()));
    r
}

pub fn decode_info(e: &Entity) -> InfoValue {
    match e.get(Field::Value) {
        Some(Value::Info(v)) => v.clone(),
        _ => InfoValue::Text(String::new()),
    }
}

pub fn encode_currency(code: &str, c: &Currency) -> Record {
    let mut r = Record::default();
    r.put(Field::Name, Value::text(code))
        .put(Field::Description, Value::text(&c.description))
        .put(Field::Symbol, Value::text(&c.symbol))
        .put(Field::Rate, Value::Dec(c.rate));
    // How amounts in it are written (decisions.md, entry 61). Always
    // written, so going back to the default is a write too; a log from
    // before reads as the default.
    r.put(Field::SymbolAfter, Value::Bool(c.format.symbol_after))
        .put(Field::Separators, Value::text(c.format.separators.name()));
    r
}

pub fn decode_currency(e: &Entity) -> Currency {
    Currency {
        description: text(e.get(Field::Description)),
        symbol: text(e.get(Field::Symbol)),
        rate: dec(e.get(Field::Rate)),
        format: CurrencyFormat {
            symbol_after: boolean(e.get(Field::SymbolAfter)),
            separators: Separators::parse(&text(e.get(Field::Separators))).unwrap_or_default(),
        },
    }
}

pub fn encode_global(name: &str, f: &Formula, info: &GlobalInfo, b: &dyn Binder) -> Record {
    let mut r = Record::default();
    r.put(Field::Name, Value::text(name))
        .put(Field::Formula, bound(b, f))
        .put(Field::Description, Value::text(&info.description))
        .put(Field::Unit, named_or_text(b, Kind::Unit, &info.unit))
        .put(
            Field::Decimals,
            Value::opt_int(info.decimals.map(i64::from)),
        )
        .put(Field::ShowByName, Value::Bool(info.show_by_name))
        .put(Field::Folder, Value::text(&info.folder));
    r
}

pub fn decode_global(e: &Entity, r: &dyn Resolver) -> (Formula, GlobalInfo) {
    let g = |f: Field| e.get(f);
    (
        unbound(r, g(Field::Formula)),
        GlobalInfo {
            description: text(g(Field::Description)),
            unit: ref_text(r, g(Field::Unit)),
            decimals: opt_int(g(Field::Decimals)).and_then(|n| u8::try_from(n).ok()),
            show_by_name: boolean(g(Field::ShowByName)),
            folder: text(g(Field::Folder)),
        },
    )
}

pub fn encode_unit(name: &str, u: &UnitDef) -> Record {
    let mut r = Record::default();
    r.put(Field::Name, Value::text(name))
        .put(Field::Plural, Value::text(&u.plural))
        .put(Field::Description, Value::text(&u.description))
        .put(Field::Hours, Value::opt_dec(u.hours));
    r
}

pub fn decode_unit(e: &Entity) -> UnitDef {
    UnitDef {
        plural: text(e.get(Field::Plural)),
        description: text(e.get(Field::Description)),
        hours: opt_dec(e.get(Field::Hours)),
    }
}

pub fn encode_fringe(name: &str, f: &Fringe, b: &dyn Binder) -> Record {
    let mut r = Record::default();
    r.put(Field::Name, Value::text(name))
        .put(Field::Description, Value::text(&f.description))
        .put(Field::ShortCode, Value::opt_text(f.short_code.as_deref()))
        .put(Field::Rate, bound(b, &f.rate))
        .put(Field::Cutoff, Value::opt_int(f.kind.cutoff()));
    match &f.kind {
        FringeKind::Percent { .. } => {
            r.put(Field::Kind, Value::text("percent"));
        }
        FringeKind::Flat {
            unit,
            basis,
            max_hours_per_day,
            ..
        } => {
            r.put(Field::Kind, Value::text("flat"))
                .put(Field::Unit, opt_named(b, Kind::Unit, unit.as_deref()))
                .put(Field::Basis, Value::text(basis.name()))
                .put(Field::MaxHoursPerDay, Value::opt_dec(*max_hours_per_day));
        }
    }
    r
}

pub fn decode_fringe(e: &Entity, r: &dyn Resolver) -> Fringe {
    let g = |f: Field| e.get(f);
    let cutoff = opt_int(g(Field::Cutoff));
    let kind = if text(g(Field::Kind)) == "flat" {
        FringeKind::Flat {
            unit: ref_opt_text(r, g(Field::Unit)),
            basis: FlatBasis::parse(&text(g(Field::Basis))).unwrap_or_default(),
            max_hours_per_day: opt_dec(g(Field::MaxHoursPerDay)),
            cutoff,
        }
    } else {
        FringeKind::Percent { cutoff }
    };
    Fringe {
        description: text(g(Field::Description)),
        short_code: opt_text(g(Field::ShortCode)),
        rate: unbound(r, g(Field::Rate)),
        kind,
    }
}

pub fn encode_group(name: &str, gr: &Group) -> Record {
    let mut r = Record::default();
    r.put(Field::Name, Value::text(name))
        .put(Field::Description, Value::text(&gr.description))
        .put(Field::ShortCode, Value::opt_text(gr.short_code.as_deref()))
        .put(Field::InBudgetTotal, Value::Bool(gr.in_budget_total))
        .put(Field::Color, Value::opt_text(gr.color.as_deref()));
    r
}

pub fn decode_group(e: &Entity) -> Group {
    Group {
        description: text(e.get(Field::Description)),
        short_code: opt_text(e.get(Field::ShortCode)),
        in_budget_total: boolean(e.get(Field::InBudgetTotal)),
        color: opt_text(e.get(Field::Color)),
    }
}

pub fn encode_set(name: &str, s: &SetDef) -> Record {
    let mut r = Record::default();
    r.put(Field::Name, Value::text(name))
        .put(Field::Description, Value::text(&s.description));
    r
}

pub fn decode_set(e: &Entity) -> SetDef {
    SetDef {
        description: text(e.get(Field::Description)),
    }
}

pub fn encode_location(name: &str, l: &Location) -> Record {
    let mut r = Record::default();
    r.put(Field::Name, Value::text(name))
        .put(Field::Description, Value::text(&l.description))
        .put(Field::City, Value::text(&l.city))
        .put(Field::Region, Value::text(&l.region))
        .put(Field::Country, Value::text(&l.country));
    r
}

pub fn decode_location(e: &Entity) -> Location {
    Location {
        description: text(e.get(Field::Description)),
        city: text(e.get(Field::City)),
        region: text(e.get(Field::Region)),
        country: text(e.get(Field::Country)),
    }
}

// ---------------------------------------------------------------------------
// Charges and credits

pub fn encode_charge(c: &Charge, b: &dyn Binder) -> Record {
    let mut r = Record::default();
    r.put(Field::Name, Value::text(&c.name))
        .put(Field::Account, Value::opt_text(c.account.as_deref()))
        .put(Field::Applied, Value::Bool(c.applied))
        .put(Field::LabelPercent, Value::Bool(c.label_percent))
        .put(Field::LabelExcluded, Value::Bool(c.label_excluded))
        .tags(
            Field::ExcludedGroups,
            members(b, Kind::Group, &c.excluded_groups),
        )
        .tags(
            Field::ExcludedCategories,
            c.excluded_categories
                .iter()
                .map(|code| match b.category(code) {
                    Some(u) => Member::Ref(u),
                    None => Member::Text(code.clone()),
                })
                .collect(),
        )
        .tags(
            Field::ExcludedAccounts,
            c.excluded_accounts
                .iter()
                .map(|code| match b.account(code) {
                    Some(u) => Member::Ref(u),
                    None => Member::Text(code.clone()),
                })
                .collect(),
        );
    match &c.kind {
        ChargeKind::Percent(p) => {
            r.put(Field::Kind, Value::text("percent"))
                .put(Field::Percent, Value::Dec(*p));
        }
        ChargeKind::Flat { amount, currency } => {
            r.put(Field::Kind, Value::text("flat"))
                .put(Field::Amount, Value::Int(*amount))
                .put(
                    Field::Currency,
                    opt_named(b, Kind::Currency, currency.as_deref()),
                );
        }
    }
    r
}

pub fn decode_charge(e: &Entity, r: &dyn Resolver) -> Charge {
    let g = |f: Field| e.get(f);
    let kind = if text(g(Field::Kind)) == "flat" {
        ChargeKind::Flat {
            amount: int(g(Field::Amount)),
            currency: ref_opt_text(r, g(Field::Currency)),
        }
    } else {
        ChargeKind::Percent(dec(g(Field::Percent)))
    };
    Charge {
        name: text(g(Field::Name)),
        account: opt_text(g(Field::Account)),
        applied: boolean(g(Field::Applied)),
        kind,
        excluded_amounts: Vec::new(),
        excluded_groups: names(r, e, Field::ExcludedGroups),
        excluded_categories: names(r, e, Field::ExcludedCategories),
        excluded_accounts: names(r, e, Field::ExcludedAccounts),
        label_percent: boolean(g(Field::LabelPercent)),
        label_excluded: boolean(g(Field::LabelExcluded)),
    }
}

pub fn encode_excluded_amount(description: &str, amount: i64) -> Record {
    let mut r = Record::default();
    r.put(Field::Description, Value::text(description))
        .put(Field::Amount, Value::Int(amount));
    r
}

pub fn decode_excluded_amount(e: &Entity) -> (String, i64) {
    (text(e.get(Field::Description)), int(e.get(Field::Amount)))
}

pub fn encode_credit(c: &Credit, b: &dyn Binder) -> Record {
    let mut r = Record::default();
    r.put(Field::Name, Value::text(&c.name))
        .put(Field::Account, Value::opt_text(c.account.as_deref()))
        .put(Field::Applied, Value::Bool(c.applied))
        .put(Field::Cap, Value::opt_int(c.cap))
        .put(Field::DiscountPercent, Value::Dec(c.discount_percent))
        .tags(
            Field::ExcludedFringes,
            members(b, Kind::Fringe, &c.excluded_fringes),
        );
    r
}

pub fn decode_credit(e: &Entity, r: &dyn Resolver) -> Credit {
    let g = |f: Field| e.get(f);
    Credit {
        name: text(g(Field::Name)),
        account: opt_text(g(Field::Account)),
        applied: boolean(g(Field::Applied)),
        cap: opt_int(g(Field::Cap)),
        discount_percent: dec(g(Field::DiscountPercent)),
        excluded_fringes: names(r, e, Field::ExcludedFringes),
        rows: Vec::new(),
    }
}

pub fn encode_credit_row(row: &CreditRow, b: &dyn Binder) -> Record {
    let mut r = Record::default();
    r.put(Field::Description, Value::text(&row.description))
        .put(Field::Percent, Value::Dec(row.percent));
    match &row.source {
        CreditSource::Flat(c) => {
            r.put(Field::Source, Value::text("flat"))
                .put(Field::Amount, Value::Int(*c));
        }
        CreditSource::Group { group } => {
            r.put(Field::Source, Value::text("group"))
                .put(Field::Group, named_or_text(b, Kind::Group, group));
        }
        CreditSource::Category { category } => {
            r.put(Field::Source, Value::text("category")).put(
                Field::Category,
                match b.category(category) {
                    Some(u) => Value::Ref(u),
                    None => Value::text(category),
                },
            );
        }
        // An account row (decisions.md, entry 61).
        CreditSource::Account { account } => {
            r.put(Field::Source, Value::text("account")).put(
                Field::Account,
                match b.account(account) {
                    Some(u) => Value::Ref(u),
                    None => Value::text(account),
                },
            );
        }
    }
    match row.cap {
        CreditCap::None => {
            r.put(Field::CapKind, Value::text("none"));
        }
        CreditCap::QualifiedSpend(c) => {
            r.put(Field::CapKind, Value::text("qualified_spend"))
                .put(Field::Cap, Value::Int(c));
        }
        CreditCap::Credit(c) => {
            r.put(Field::CapKind, Value::text("credit"))
                .put(Field::Cap, Value::Int(c));
        }
    }
    r
}

pub fn decode_credit_row(e: &Entity, r: &dyn Resolver) -> CreditRow {
    let g = |f: Field| e.get(f);
    let source = match text(g(Field::Source)).as_str() {
        "group" => CreditSource::Group {
            group: ref_text(r, g(Field::Group)),
        },
        "category" => CreditSource::Category {
            category: ref_text(r, g(Field::Category)),
        },
        "account" => CreditSource::Account {
            account: ref_text(r, g(Field::Account)),
        },
        _ => CreditSource::Flat(int(g(Field::Amount))),
    };
    let cap = match text(g(Field::CapKind)).as_str() {
        "qualified_spend" => CreditCap::QualifiedSpend(int(g(Field::Cap))),
        "credit" => CreditCap::Credit(int(g(Field::Cap))),
        _ => CreditCap::None,
    };
    CreditRow {
        description: text(g(Field::Description)),
        source,
        percent: dec(g(Field::Percent)),
        cap,
    }
}

// ---------------------------------------------------------------------------
// Versions

pub fn encode_version(v: &Version) -> Record {
    let mut r = Record::default();
    r.put(Field::Id, Value::Int(v.id as i64))
        .put(Field::Name, Value::text(&v.name))
        .put(Field::Created, Value::text(&v.created))
        .put(Field::Note, Value::opt_text(v.note.as_deref()))
        .put(Field::Content, Value::Snapshot(Arc::clone(&v.budget)));
    r
}

pub fn decode_version(e: &Entity, id: u64) -> Version {
    let g = |f: Field| e.get(f);
    Version {
        id,
        name: text(g(Field::Name)),
        created: text(g(Field::Created)),
        note: opt_text(g(Field::Note)),
        budget: match g(Field::Content) {
            Some(Value::Snapshot(s)) => Arc::clone(s),
            _ => Arc::new(Budget::new("", "").snapshot()),
        },
    }
}

// ---------------------------------------------------------------------------
// Actuals

pub fn encode_transaction(t: &Transaction, b: &dyn Binder) -> Record {
    let mut r = Record::default();
    r.put(Field::Id, Value::Int(t.id as i64))
        .put(Field::Date, Value::text(&t.date))
        .put(
            Field::Account,
            match b.account(&t.account) {
                Some(u) => Value::Ref(u),
                None => Value::text(&t.account),
            },
        )
        .put(
            Field::Line,
            match t.line.and_then(|id| b.row(id)) {
                Some(u) => Value::Ref(u),
                None => Value::opt_int(t.line.map(|id| id as i64)),
            },
        )
        .put(Field::Vendor, Value::text(&t.vendor))
        .put(Field::Description, Value::text(&t.description))
        .put(Field::Reference, Value::text(&t.reference))
        .put(Field::Amount, Value::Int(t.amount))
        .put(
            Field::Batch,
            match t.batch.and_then(|id| b.batch(id)) {
                Some(u) => Value::Ref(u),
                None => Value::opt_int(t.batch.map(|id| id as i64)),
            },
        )
        .tags(
            Field::Tags,
            t.tags.iter().map(|s| Member::Text(s.clone())).collect(),
        );
    r
}

pub fn decode_transaction(e: &Entity, r: &dyn Resolver, id: u64) -> Transaction {
    let g = |f: Field| e.get(f);
    Transaction {
        id,
        date: text(g(Field::Date)),
        account: ref_text(r, g(Field::Account)),
        line: match g(Field::Line) {
            Some(Value::Ref(u)) => r.row_id(*u),
            Some(Value::Int(n)) => u64::try_from(*n).ok(),
            _ => None,
        },
        tags: e
            .set_members(Field::Tags)
            .into_iter()
            .filter_map(|m| match m {
                Member::Text(s) => Some(s.clone()),
                Member::Ref(_) => None,
            })
            .collect(),
        vendor: text(g(Field::Vendor)),
        description: text(g(Field::Description)),
        reference: text(g(Field::Reference)),
        amount: int(g(Field::Amount)),
        batch: match g(Field::Batch) {
            Some(Value::Ref(u)) => r.batch_id(*u),
            Some(Value::Int(n)) => u64::try_from(*n).ok(),
            _ => None,
        },
    }
}

pub fn encode_batch(b: &ImportBatch) -> Record {
    let mut r = Record::default();
    r.put(Field::Id, Value::Int(b.id as i64))
        .put(Field::File, Value::text(&b.file))
        .put(Field::Imported, Value::text(&b.imported))
        .put(Field::Rows, Value::Int(b.rows as i64));
    r
}

pub fn decode_batch(e: &Entity, id: u64) -> ImportBatch {
    let g = |f: Field| e.get(f);
    ImportBatch {
        id,
        file: text(g(Field::File)),
        imported: text(g(Field::Imported)),
        rows: u64::try_from(int(g(Field::Rows))).unwrap_or(0),
    }
}

pub fn encode_etc(code: &str, m: EtcMethod, b: &dyn Binder) -> Record {
    let mut r = Record::default();
    r.put(
        Field::Account,
        match b.account(code) {
            Some(u) => Value::Ref(u),
            None => Value::text(code),
        },
    )
    .put(Field::Method, Value::text(m.name()));
    match m {
        EtcMethod::BudgetMinusActual => {}
        EtcMethod::Manual(c) => {
            r.put(Field::Amount, Value::Int(c));
        }
        EtcMethod::PercentComplete(p) => {
            r.put(Field::Percent, Value::Dec(p));
        }
    }
    r
}

pub fn decode_etc(e: &Entity, r: &dyn Resolver) -> (String, EtcMethod) {
    let g = |f: Field| e.get(f);
    let method = match text(g(Field::Method)).as_str() {
        "manual" => EtcMethod::Manual(int(g(Field::Amount))),
        "percent_complete" => EtcMethod::PercentComplete(dec(g(Field::Percent))),
        _ => EtcMethod::BudgetMinusActual,
    };
    (ref_text(r, g(Field::Account)), method)
}

/// The record's `Id` field as an unsigned id, if it has one.
pub fn record_id(e: &Entity) -> Option<u64> {
    e.get(Field::Id)
        .and_then(Value::as_int)
        .and_then(|n| u64::try_from(n).ok())
}

/// The baseline as the file keeps it, for the one `baseline` register.
pub fn baseline_value(b: Option<&Baseline>) -> Value {
    b.map_or(Value::Null, |base| Value::Baseline(base.clone()))
}
