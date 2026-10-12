//! What a change is made of: entities of a kind, their fields (registers),
//! their tag sets and their place in a list, and the operations that write
//! them (docs/research/collaboration.md, 3.4 and 3.5).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::clock::{ChangeId, Frontier, Stamp};
use super::ids::{ActorId, Uid};
use super::seq::{Node, PosId};
use crate::coding::AccountCoding;
use crate::model::{
    Baseline, BudgetSnapshot, DisplayPrefs, FringePosting, FringeRow, GroupRules, InfoValue,
    NewLineDefaults,
};
use crate::Dec;

/// What an entity is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// The budget itself ([`Uid::ROOT`]): settings and tools are its
    /// fields; the topsheet, charges, credits, versions and actuals are
    /// its lists.
    Root,
    Category,
    ProductionTotal,
    Account,
    Row,
    Note,
    Info,
    Currency,
    Global,
    Unit,
    Fringe,
    Group,
    Set,
    Location,
    Charge,
    ExcludedAmount,
    Credit,
    CreditRow,
    Version,
    Transaction,
    Batch,
    /// An estimate-to-complete override on one account.
    Etc,
}

impl Kind {
    pub const ALL: [Kind; 22] = [
        Kind::Root,
        Kind::Category,
        Kind::ProductionTotal,
        Kind::Account,
        Kind::Row,
        Kind::Note,
        Kind::Info,
        Kind::Currency,
        Kind::Global,
        Kind::Unit,
        Kind::Fringe,
        Kind::Group,
        Kind::Set,
        Kind::Location,
        Kind::Charge,
        Kind::ExcludedAmount,
        Kind::Credit,
        Kind::CreditRow,
        Kind::Version,
        Kind::Transaction,
        Kind::Batch,
        Kind::Etc,
    ];

    /// The name files use.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Root => "root",
            Kind::Category => "category",
            Kind::ProductionTotal => "production_total",
            Kind::Account => "account",
            Kind::Row => "row",
            Kind::Note => "note",
            Kind::Info => "info",
            Kind::Currency => "currency",
            Kind::Global => "global",
            Kind::Unit => "unit",
            Kind::Fringe => "fringe",
            Kind::Group => "group",
            Kind::Set => "set",
            Kind::Location => "location",
            Kind::Charge => "charge",
            Kind::ExcludedAmount => "excluded_amount",
            Kind::Credit => "credit",
            Kind::CreditRow => "credit_row",
            Kind::Version => "version",
            Kind::Transaction => "transaction",
            Kind::Batch => "batch",
            Kind::Etc => "etc",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == s)
    }

    /// The list entities of this kind live in, or `None` for kinds kept
    /// in a map by name (setup items, info, ETC overrides) and the root.
    pub fn list(self) -> Option<ListName> {
        Some(match self {
            Kind::Category | Kind::ProductionTotal => ListName::Topsheet,
            Kind::Account => ListName::Accounts,
            Kind::Row => ListName::Rows,
            Kind::Note => ListName::Notes,
            Kind::Charge => ListName::Charges,
            Kind::ExcludedAmount => ListName::ExcludedAmounts,
            Kind::Credit => ListName::Credits,
            Kind::CreditRow => ListName::CreditRows,
            Kind::Version => ListName::Versions,
            Kind::Transaction => ListName::Transactions,
            Kind::Batch => ListName::Batches,
            Kind::Root
            | Kind::Info
            | Kind::Currency
            | Kind::Global
            | Kind::Unit
            | Kind::Fringe
            | Kind::Group
            | Kind::Set
            | Kind::Location
            | Kind::Etc => return None,
        })
    }

    /// Kinds whose entities are told apart by a name (their `name` field
    /// must be unique among live entities of the kind).
    pub fn is_named(self) -> bool {
        matches!(
            self,
            Kind::Info
                | Kind::Currency
                | Kind::Global
                | Kind::Unit
                | Kind::Fringe
                | Kind::Group
                | Kind::Set
                | Kind::Location
                | Kind::Charge
                | Kind::Credit
        )
    }

    /// In sentences: "fringe", "account"...
    pub fn word(self) -> &'static str {
        match self {
            Kind::Root => "budget",
            Kind::Category => "category",
            Kind::ProductionTotal => "production total",
            Kind::Account => "account",
            Kind::Row => "line",
            Kind::Note => "note",
            Kind::Info => "info field",
            Kind::Currency => "currency",
            Kind::Global => "global",
            Kind::Unit => "unit",
            Kind::Fringe => "fringe",
            Kind::Group => "group",
            Kind::Set => "set",
            Kind::Location => "location",
            Kind::Charge => "charge",
            Kind::ExcludedAmount => "excluded amount",
            Kind::Credit => "credit",
            Kind::CreditRow => "credit row",
            Kind::Version => "version",
            Kind::Transaction => "transaction",
            Kind::Batch => "import",
            Kind::Etc => "estimate to complete",
        }
    }
}

/// An ordered list an entity can be in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ListName {
    /// Categories and production totals, under the root.
    Topsheet,
    /// Accounts, under a category.
    Accounts,
    /// Detail rows, under an account.
    Rows,
    /// Notes, under a category, an account or a row.
    Notes,
    /// Charges, under the root.
    Charges,
    /// A charge's excluded amounts.
    ExcludedAmounts,
    /// Credits, under the root.
    Credits,
    /// A credit's rows.
    CreditRows,
    /// Saved versions, under the root.
    Versions,
    /// Actuals transactions, under the root.
    Transactions,
    /// Actuals import batches, under the root.
    Batches,
}

impl ListName {
    pub const ALL: [ListName; 11] = [
        ListName::Topsheet,
        ListName::Accounts,
        ListName::Rows,
        ListName::Notes,
        ListName::Charges,
        ListName::ExcludedAmounts,
        ListName::Credits,
        ListName::CreditRows,
        ListName::Versions,
        ListName::Transactions,
        ListName::Batches,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ListName::Topsheet => "topsheet",
            ListName::Accounts => "accounts",
            ListName::Rows => "rows",
            ListName::Notes => "notes",
            ListName::Charges => "charges",
            ListName::ExcludedAmounts => "excluded_amounts",
            ListName::Credits => "credits",
            ListName::CreditRows => "credit_rows",
            ListName::Versions => "versions",
            ListName::Transactions => "transactions",
            ListName::Batches => "batches",
        }
    }

    pub fn parse(s: &str) -> Option<ListName> {
        ListName::ALL.into_iter().find(|l| l.name() == s)
    }

    /// Which kinds of entity own a list of this name.
    pub fn owners(self) -> &'static [Kind] {
        match self {
            ListName::Topsheet
            | ListName::Charges
            | ListName::Credits
            | ListName::Versions
            | ListName::Transactions
            | ListName::Batches => &[Kind::Root],
            ListName::Accounts => &[Kind::Category],
            ListName::Rows => &[Kind::Account],
            ListName::Notes => &[Kind::Category, Kind::Account, Kind::Row],
            ListName::ExcludedAmounts => &[Kind::Charge],
            ListName::CreditRows => &[Kind::Credit],
        }
    }
}

/// One list: its owner and its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ListKey {
    pub parent: Uid,
    pub name: ListName,
}

impl ListKey {
    pub fn new(parent: Uid, name: ListName) -> ListKey {
        ListKey { parent, name }
    }

    pub fn root(name: ListName) -> ListKey {
        ListKey {
            parent: Uid::ROOT,
            name,
        }
    }
}

/// A field of an entity: one last-writer-wins register, or (for the tag
/// sets) one add-wins set. Which fields an entity has depends on its kind;
/// the same field name can hold different value types on different kinds
/// (`rate` is a formula on a row and a decimal on a currency).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    // Shared
    Name,
    Code,
    Description,
    Id,
    Kind,
    Note,
    Text,
    Created,
    Edited,
    Author,
    Done,
    Amount,
    Unit,
    Rate,
    Currency,
    Account,
    Percent,
    Cap,
    Color,
    ShortCode,
    // Rows
    X,
    Unit2,
    Unit3,
    X4,
    Unit4,
    Location,
    Set,
    FringeRange,
    Bold,
    Italic,
    Indent,
    Fringes,
    Groups,
    // Setup items
    Symbol,
    Plural,
    Hours,
    Formula,
    Decimals,
    ShowByName,
    Folder,
    Cutoff,
    Basis,
    MaxHoursPerDay,
    InBudgetTotal,
    City,
    Region,
    Country,
    Value,
    // Charges and credits
    Applied,
    LabelPercent,
    LabelExcluded,
    ExcludedGroups,
    ExcludedCategories,
    ExcludedAccounts,
    ExcludedFringes,
    DiscountPercent,
    Source,
    Group,
    Category,
    CapKind,
    // Versions
    Content,
    // Actuals
    Date,
    Line,
    Vendor,
    Reference,
    Batch,
    Tags,
    File,
    Imported,
    Rows,
    Method,
    // The budget's settings and tools (fields of the root)
    Title,
    BaseCurrency,
    TotalsCurrency,
    FringePosting,
    TrailingLevelFringes,
    CutoffFactor,
    FringeRow,
    Arithmetic,
    AccountCoding,
    NewLineDefaults,
    GroupRules,
    Display,
    CompactVersions,
    ReportPresets,
    TagPresets,
    LinePresets,
    SetupOrder,
    SetupFolders,
    SetupSources,
    OtPatterns,
    Baseline,
}

impl Field {
    pub const ALL: [Field; 92] = [
        Field::Name,
        Field::Code,
        Field::Description,
        Field::Id,
        Field::Kind,
        Field::Note,
        Field::Text,
        Field::Created,
        Field::Edited,
        Field::Author,
        Field::Done,
        Field::Amount,
        Field::Unit,
        Field::Rate,
        Field::Currency,
        Field::Account,
        Field::Percent,
        Field::Cap,
        Field::Color,
        Field::ShortCode,
        Field::X,
        Field::Unit2,
        Field::Unit3,
        Field::X4,
        Field::Unit4,
        Field::Location,
        Field::Set,
        Field::FringeRange,
        Field::Bold,
        Field::Italic,
        Field::Indent,
        Field::Fringes,
        Field::Groups,
        Field::Symbol,
        Field::Plural,
        Field::Hours,
        Field::Formula,
        Field::Decimals,
        Field::ShowByName,
        Field::Folder,
        Field::Cutoff,
        Field::Basis,
        Field::MaxHoursPerDay,
        Field::InBudgetTotal,
        Field::City,
        Field::Region,
        Field::Country,
        Field::Value,
        Field::Applied,
        Field::LabelPercent,
        Field::LabelExcluded,
        Field::ExcludedGroups,
        Field::ExcludedCategories,
        Field::ExcludedAccounts,
        Field::ExcludedFringes,
        Field::DiscountPercent,
        Field::Source,
        Field::Group,
        Field::Category,
        Field::CapKind,
        Field::Content,
        Field::Date,
        Field::Line,
        Field::Vendor,
        Field::Reference,
        Field::Batch,
        Field::Tags,
        Field::File,
        Field::Imported,
        Field::Rows,
        Field::Method,
        Field::Title,
        Field::BaseCurrency,
        Field::TotalsCurrency,
        Field::FringePosting,
        Field::TrailingLevelFringes,
        Field::CutoffFactor,
        Field::FringeRow,
        Field::Arithmetic,
        Field::AccountCoding,
        Field::NewLineDefaults,
        Field::GroupRules,
        Field::Display,
        Field::CompactVersions,
        Field::ReportPresets,
        Field::TagPresets,
        Field::LinePresets,
        Field::SetupOrder,
        Field::SetupFolders,
        Field::SetupSources,
        Field::OtPatterns,
        Field::Baseline,
    ];

    /// The name files use.
    pub fn name(self) -> &'static str {
        match self {
            Field::Name => "name",
            Field::Code => "code",
            Field::Description => "description",
            Field::Id => "id",
            Field::Kind => "kind",
            Field::Note => "note",
            Field::Text => "text",
            Field::Created => "created",
            Field::Edited => "edited",
            Field::Author => "author",
            Field::Done => "done",
            Field::Amount => "amount",
            Field::Unit => "unit",
            Field::Rate => "rate",
            Field::Currency => "currency",
            Field::Account => "account",
            Field::Percent => "percent",
            Field::Cap => "cap",
            Field::Color => "color",
            Field::ShortCode => "short_code",
            Field::X => "x",
            Field::Unit2 => "unit2",
            Field::Unit3 => "unit3",
            Field::X4 => "x4",
            Field::Unit4 => "unit4",
            Field::Location => "location",
            Field::Set => "set",
            Field::FringeRange => "fringe_range",
            Field::Bold => "bold",
            Field::Italic => "italic",
            Field::Indent => "indent",
            Field::Fringes => "fringes",
            Field::Groups => "groups",
            Field::Symbol => "symbol",
            Field::Plural => "plural",
            Field::Hours => "hours",
            Field::Formula => "formula",
            Field::Decimals => "decimals",
            Field::ShowByName => "show_by_name",
            Field::Folder => "folder",
            Field::Cutoff => "cutoff",
            Field::Basis => "basis",
            Field::MaxHoursPerDay => "max_hours_per_day",
            Field::InBudgetTotal => "in_budget_total",
            Field::City => "city",
            Field::Region => "region",
            Field::Country => "country",
            Field::Value => "value",
            Field::Applied => "applied",
            Field::LabelPercent => "label_percent",
            Field::LabelExcluded => "label_excluded",
            Field::ExcludedGroups => "excluded_groups",
            Field::ExcludedCategories => "excluded_categories",
            Field::ExcludedAccounts => "excluded_accounts",
            Field::ExcludedFringes => "excluded_fringes",
            Field::DiscountPercent => "discount_percent",
            Field::Source => "source",
            Field::Group => "group",
            Field::Category => "category",
            Field::CapKind => "cap_kind",
            Field::Content => "content",
            Field::Date => "date",
            Field::Line => "line",
            Field::Vendor => "vendor",
            Field::Reference => "reference",
            Field::Batch => "batch",
            Field::Tags => "tags",
            Field::File => "file",
            Field::Imported => "imported",
            Field::Rows => "rows",
            Field::Method => "method",
            Field::Title => "title",
            Field::BaseCurrency => "base_currency",
            Field::TotalsCurrency => "totals_currency",
            Field::FringePosting => "fringe_posting",
            Field::TrailingLevelFringes => "trailing_level_fringes",
            Field::CutoffFactor => "cutoff_factor",
            Field::FringeRow => "fringe_row",
            Field::Arithmetic => "arithmetic",
            Field::AccountCoding => "account_coding",
            Field::NewLineDefaults => "new_line_defaults",
            Field::GroupRules => "group_rules",
            Field::Display => "display",
            Field::CompactVersions => "compact_versions",
            Field::ReportPresets => "report_presets",
            Field::TagPresets => "tag_presets",
            Field::LinePresets => "line_presets",
            Field::SetupOrder => "setup_order",
            Field::SetupFolders => "setup_folders",
            Field::SetupSources => "setup_sources",
            Field::OtPatterns => "ot_patterns",
            Field::Baseline => "baseline",
        }
    }

    pub fn parse(s: &str) -> Option<Field> {
        Field::ALL.into_iter().find(|f| f.name() == s)
    }

    /// Whether this field is an add-wins set rather than a register.
    pub fn is_set(self) -> bool {
        matches!(
            self,
            Field::Fringes
                | Field::Groups
                | Field::ExcludedGroups
                | Field::ExcludedCategories
                | Field::ExcludedAccounts
                | Field::ExcludedFringes
                | Field::Tags
        )
    }
}

/// What a register holds. Scalars, references to other entities by id,
/// bound formulas, and the compound settings and tools the design keeps
/// as one value each (docs/research/collaboration.md, 3.5), held as their
/// model types so the file format writes them as it writes the `.mthd`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Text(String),
    Dec(Dec),
    /// Another entity: a row's fringe, a transaction's account...
    Ref(Uid),
    /// A formula with every global name replaced by `{g:<uid>}`
    /// (see [`super::formula`]).
    Formula(String),
    Info(InfoValue),
    Coding(AccountCoding),
    Posting(FringePosting),
    FringeRow(FringeRow),
    NewLineDefaults(NewLineDefaults),
    GroupRules(GroupRules),
    Display(DisplayPrefs),
    Baseline(Baseline),
    /// A version's content, as the file stores a version.
    Snapshot(Arc<BudgetSnapshot>),
    ReportPresets(Vec<crate::report_preset::ReportPreset>),
    TagPresets(Vec<crate::presets::TagPreset>),
    LinePresets(Vec<crate::presets::LinePreset>),
    OtPatterns(Vec<crate::overtime::OtPattern>),
    SetupOrder(crate::order::SetupOrder),
    SetupFolders(crate::folders::SetupFolders),
    SetupSources(crate::pack::SetupSources),
}

impl Value {
    pub fn text(s: impl Into<String>) -> Value {
        Value::Text(s.into())
    }

    /// `Some(text)` as text, `None` as null.
    pub fn opt_text(s: Option<&str>) -> Value {
        s.map_or(Value::Null, Value::text)
    }

    pub fn opt_int(n: Option<i64>) -> Value {
        n.map_or(Value::Null, Value::Int)
    }

    pub fn opt_dec(d: Option<Dec>) -> Value {
        d.map_or(Value::Null, Value::Dec)
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_dec(&self) -> Option<Dec> {
        match self {
            Value::Dec(d) => Some(*d),
            _ => None,
        }
    }

    pub fn as_ref(&self) -> Option<Uid> {
        match self {
            Value::Ref(u) => Some(*u),
            _ => None,
        }
    }

    /// A short description for sentences: the text, the number, or the
    /// kind of value.
    pub fn brief(&self) -> String {
        match self {
            Value::Null => "nothing".into(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Text(s) => s.clone(),
            Value::Dec(d) => d.to_string(),
            Value::Ref(u) => format!("#{}", &u.hex()[..8]),
            Value::Formula(f) => f.clone(),
            Value::Info(_) => "an info value".into(),
            Value::Coding(_) => "account numbering".into(),
            Value::Posting(_) => "fringe posting".into(),
            Value::FringeRow(_) => "the fringe row".into(),
            Value::NewLineDefaults(_) => "new-line defaults".into(),
            Value::GroupRules(_) => "group rules".into(),
            Value::Display(_) => "display settings".into(),
            Value::Baseline(_) => "the locked originals".into(),
            Value::Snapshot(_) => "a version's content".into(),
            Value::ReportPresets(_) => "report presets".into(),
            Value::TagPresets(_) => "tag presets".into(),
            Value::LinePresets(_) => "line blocks".into(),
            Value::OtPatterns(_) => "overtime patterns".into(),
            Value::SetupOrder(_) => "setup order".into(),
            Value::SetupFolders(_) => "setup folders".into(),
            Value::SetupSources(_) => "setup sources".into(),
        }
    }
}

/// A member of an add-wins set: another entity (a row's fringe) or free
/// text (a transaction's tag, a name that resolves to nothing).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Member {
    Ref(Uid),
    Text(String),
}

/// Where an entity is put: which list, at which position, and (for a
/// position made by this op) where the position's node hangs. `node` is
/// `None` when the op points an entity back at a position that already
/// exists, as an undo does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub list: ListKey,
    pub pos: PosId,
    pub node: Option<Node>,
}

/// One operation. Every op names its entity by [`Uid`]; [`Uid::ROOT`] is
/// the budget's own settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// A new entity with its initial fields and tag sets, and its place
    /// when its kind lives in a list.
    Create {
        entity: Uid,
        kind: Kind,
        place: Option<Placement>,
        fields: BTreeMap<Field, Value>,
        sets: BTreeMap<Field, BTreeSet<Member>>,
    },
    /// One field, last writer wins.
    Set {
        entity: Uid,
        field: Field,
        value: Value,
    },
    /// Where the entity is, last writer wins.
    Move { entity: Uid, place: Placement },
    /// Tombstone on, last writer wins.
    Delete { entity: Uid },
    /// Tombstone off.
    Restore { entity: Uid },
    /// Add to a tag set; adds win over concurrent removes.
    Add {
        entity: Uid,
        field: Field,
        member: Member,
    },
    /// Remove from a tag set: only removes the adds (`seen`) the remover
    /// had seen.
    Remove {
        entity: Uid,
        field: Field,
        member: Member,
        seen: Vec<Stamp>,
    },
}

impl Op {
    /// The entity the op is about.
    pub fn entity(&self) -> Uid {
        match self {
            Op::Create { entity, .. }
            | Op::Set { entity, .. }
            | Op::Move { entity, .. }
            | Op::Delete { entity }
            | Op::Restore { entity }
            | Op::Add { entity, .. }
            | Op::Remove { entity, .. } => *entity,
        }
    }

    /// The op's name in files and sentences.
    pub fn name(&self) -> &'static str {
        match self {
            Op::Create { .. } => "create",
            Op::Set { .. } => "set",
            Op::Move { .. } => "move",
            Op::Delete { .. } => "delete",
            Op::Restore { .. } => "restore",
            Op::Add { .. } => "add",
            Op::Remove { .. } => "remove",
        }
    }
}

/// One undo step by one person: the unit of the log (docs/research/
/// collaboration.md, 3.4). Signed in phase 3; until then `sig` is empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub actor: ActorId,
    /// Per actor, from 1, no gaps.
    pub seq: u64,
    /// The last seq the author had seen from each actor.
    pub deps: Frontier,
    /// 1 + the highest Lamport clock the author had seen.
    pub lamport: u64,
    /// The author's wall clock, ISO 8601, for display only.
    pub time: String,
    /// What the person did: "Edit rate on 21-01 line 3".
    pub label: String,
    pub ops: Vec<Op>,
}

impl Change {
    pub fn id(&self) -> ChangeId {
        ChangeId {
            actor: self.actor,
            seq: self.seq,
        }
    }

    /// The stamp of op `index` of this change.
    pub fn stamp(&self, index: usize) -> Stamp {
        Stamp {
            lamport: self.lamport,
            actor: self.actor,
            seq: self.seq,
            index: index as u32,
        }
    }
}
