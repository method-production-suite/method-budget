//! File-side types: the exact shape of the JSON, kept separate from the engine
//! model so `fwb-core` stays dependency-free and the format can evolve on its own.
//!
//! Every conversion destructures the core type completely, so adding a field to
//! the model is a compile error here until the format handles it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use fwb_core::coding::AccountCoding;
use fwb_core::{
    Account, Baseline, Budget, Category, Cents, Charge, ChargeKind, Credit, CreditCap, CreditRow,
    CreditSource, Currency, Dec, Detail, Formula, Fringe, FringeKind, FringePosting, Group,
    InfoValue, Location, NewLineDefaults, ProductionTotal, RowId, RowKind, SetDef, TopsheetRow,
    UnitDef, Version,
};
use fwb_core::{Band, BandField, FontSize, ReportColumn, ReportPreset, ReportSection, ReportSpec};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ---------------------------------------------------------------------------
// Scalars

/// A [`Dec`] written as a JSON string in canonical form (`"7.65"`, `"-3"`).
/// Never a JSON number, so no reader ever sees a float.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DecStr(pub Dec);

impl DecStr {
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl Serialize for DecStr {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DecStr {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<DecStr, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = DecStr;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a decimal number written as a string, like \"7.65\"")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<DecStr, E> {
                v.parse()
                    .map(DecStr)
                    .map_err(|e| E::custom(format!("invalid decimal {v:?}: {e}")))
            }
        }
        d.deserialize_str(V)
    }
}

/// An account-coding separator: one character, or `""` for none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SepStr(pub Option<char>);

impl Serialize for SepStr {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut buf = [0u8; 4];
        s.serialize_str(match self.0 {
            Some(c) => c.encode_utf8(&mut buf),
            None => "",
        })
    }
}

impl<'de> Deserialize<'de> for SepStr {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<SepStr, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = SepStr;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a one-character string, or \"\" for no separator")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<SepStr, E> {
                let mut chars = v.chars();
                match (chars.next(), chars.next()) {
                    (None, _) => Ok(SepStr(None)),
                    (Some(c), None) => Ok(SepStr(Some(c))),
                    _ => Err(E::custom(format!(
                        "invalid separator {v:?}: use one character, or \"\" for none"
                    ))),
                }
            }
        }
        d.deserialize_str(V)
    }
}

fn is_true(b: &bool) -> bool {
    *b
}

fn yes() -> bool {
    true
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

fn map<'a, K: Clone + Ord, A, B>(m: &'a BTreeMap<K, A>, f: impl Fn(&'a A) -> B) -> BTreeMap<K, B> {
    m.iter().map(|(k, v)| (k.clone(), f(v))).collect()
}

fn unmap<K: Ord, A, B>(m: BTreeMap<K, A>, f: impl Fn(A) -> B) -> BTreeMap<K, B> {
    m.into_iter().map(|(k, v)| (k, f(v))).collect()
}

// ---------------------------------------------------------------------------
// Document

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileDoc {
    pub format: String,
    pub version: u64,
    pub budget: FileBudget,
}

/// Loose first pass: just enough to check `format` and `version` before the
/// strict parse, so a newer file gets a version error instead of field errors.
/// Only accepts a JSON object (a derived struct would also take an array).
pub(crate) struct Header {
    pub format: Option<serde_json::Value>,
    pub version: Option<serde_json::Value>,
}

impl<'de> Deserialize<'de> for Header {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Header, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Header;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object with \"format\", \"version\" and \"budget\"")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut m: A) -> Result<Header, A::Error> {
                let mut h = Header {
                    format: None,
                    version: None,
                };
                while let Some(key) = m.next_key::<String>()? {
                    match key.as_str() {
                        "format" => h.format = m.next_value()?,
                        "version" => h.version = m.next_value()?,
                        _ => {
                            m.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(h)
            }
        }
        d.deserialize_map(V)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileBudget {
    title: String,
    base_currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    totals_currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_coding: Option<FileCoding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fringe_posting: Option<FilePosting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    new_line_defaults: Option<FileNewLineDefaults>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    info: BTreeMap<String, FileInfo>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    currencies: BTreeMap<String, FileCurrency>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    units: BTreeMap<String, FileUnit>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    globals: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    fringes: BTreeMap<String, FileFringe>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    groups: BTreeMap<String, FileGroup>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    sets: BTreeMap<String, FileSet>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    locations: BTreeMap<String, FileLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    topsheet: Vec<FileTopsheetRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    charges: Vec<FileCharge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    credits: Vec<FileCredit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    baseline: Option<FileBaseline>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    report_presets: Vec<FileReportPreset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    versions: Vec<FileVersion>,
}

impl From<&Budget> for FileBudget {
    fn from(b: &Budget) -> FileBudget {
        let Budget {
            title,
            base_currency,
            totals_currency,
            info,
            currencies,
            globals,
            units,
            fringes,
            fringe_posting,
            account_coding,
            new_line_defaults,
            groups,
            sets,
            locations,
            topsheet,
            charges,
            credits,
            baseline,
            versions,
            report_presets,
        } = b;
        FileBudget {
            title: title.clone(),
            base_currency: base_currency.clone(),
            totals_currency: totals_currency.clone(),
            account_coding: (*account_coding != AccountCoding::default())
                .then(|| FileCoding::from(account_coding)),
            fringe_posting: (*fringe_posting != FringePosting::default())
                .then(|| FilePosting::from(fringe_posting)),
            new_line_defaults: (*new_line_defaults != NewLineDefaults::default())
                .then(|| FileNewLineDefaults::from(new_line_defaults)),
            info: map(info, FileInfo::from),
            currencies: map(currencies, FileCurrency::from),
            units: map(units, FileUnit::from),
            globals: map(globals, |f| f.0.clone()),
            fringes: map(fringes, FileFringe::from),
            groups: map(groups, FileGroup::from),
            sets: map(sets, FileSet::from),
            locations: map(locations, FileLocation::from),
            topsheet: topsheet.iter().map(FileTopsheetRow::from).collect(),
            charges: charges.iter().map(FileCharge::from).collect(),
            credits: credits.iter().map(FileCredit::from).collect(),
            baseline: baseline.as_ref().map(FileBaseline::from),
            report_presets: report_presets.iter().map(FileReportPreset::from).collect(),
            versions: versions.iter().map(FileVersion::from).collect(),
        }
    }
}

impl From<FileBudget> for Budget {
    fn from(f: FileBudget) -> Budget {
        Budget {
            title: f.title,
            base_currency: f.base_currency,
            totals_currency: f.totals_currency,
            info: unmap(f.info, InfoValue::from),
            currencies: unmap(f.currencies, Currency::from),
            globals: unmap(f.globals, Formula),
            units: unmap(f.units, UnitDef::from),
            fringes: unmap(f.fringes, Fringe::from),
            fringe_posting: f
                .fringe_posting
                .map(FringePosting::from)
                .unwrap_or_default(),
            account_coding: f
                .account_coding
                .map(AccountCoding::from)
                .unwrap_or_default(),
            new_line_defaults: f
                .new_line_defaults
                .map(NewLineDefaults::from)
                .unwrap_or_default(),
            groups: unmap(f.groups, Group::from),
            sets: unmap(f.sets, SetDef::from),
            locations: unmap(f.locations, Location::from),
            topsheet: f.topsheet.into_iter().map(TopsheetRow::from).collect(),
            charges: f.charges.into_iter().map(Charge::from).collect(),
            credits: f.credits.into_iter().map(Credit::from).collect(),
            baseline: f.baseline.map(Baseline::from),
            versions: f.versions.into_iter().map(Version::from).collect(),
            report_presets: f
                .report_presets
                .into_iter()
                .map(ReportPreset::from)
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Versions

/// A named copy of the budget's content.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileVersion {
    id: u64,
    name: String,
    created: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    budget: FileSnapshot,
}

/// A version's budget: the same object as the top-level budget, except that
/// it may not have `versions` of its own.
struct FileSnapshot(FileBudget);

impl Serialize for FileSnapshot {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de> Deserialize<'de> for FileSnapshot {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<FileSnapshot, D::Error> {
        let b = FileBudget::deserialize(d)?;
        if !b.versions.is_empty() {
            return Err(de::Error::custom(
                "a version's budget cannot have versions of its own",
            ));
        }
        if !b.report_presets.is_empty() {
            return Err(de::Error::custom(
                "a version's budget cannot have report presets",
            ));
        }
        Ok(FileSnapshot(b))
    }
}

impl From<&Version> for FileVersion {
    fn from(v: &Version) -> FileVersion {
        let Version {
            id,
            name,
            created,
            note,
            budget,
        } = v;
        FileVersion {
            id: *id,
            name: name.clone(),
            created: created.clone(),
            note: note.clone(),
            // A snapshot has no versions, so this never writes nested ones.
            budget: FileSnapshot(FileBudget::from(&budget.to_budget())),
        }
    }
}

impl From<FileVersion> for Version {
    fn from(f: FileVersion) -> Version {
        // Nested versions were rejected while reading, so none are dropped here.
        let (content, _) = Budget::from(f.budget.0).into_parts();
        Version {
            id: f.id,
            name: f.name,
            created: f.created,
            note: f.note,
            budget: std::sync::Arc::new(content),
        }
    }
}

// ---------------------------------------------------------------------------
// Budget settings

/// Written only when it differs from the default; then every field is present.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileCoding {
    category_width: u8,
    account_width: u8,
    sub_width: u8,
    set_width: u8,
    separator: SepStr,
}

impl From<&AccountCoding> for FileCoding {
    fn from(c: &AccountCoding) -> FileCoding {
        let AccountCoding {
            category_width,
            account_width,
            sub_width,
            set_width,
            separator,
        } = *c;
        FileCoding {
            category_width,
            account_width,
            sub_width,
            set_width,
            separator: SepStr(separator),
        }
    }
}

impl From<FileCoding> for AccountCoding {
    fn from(f: FileCoding) -> AccountCoding {
        AccountCoding {
            category_width: f.category_width,
            account_width: f.account_width,
            sub_width: f.sub_width,
            set_width: f.set_width,
            separator: f.separator.0,
        }
    }
}

/// What an inserted line copies from its neighbor. Written only when it
/// differs from the default; then every field is present.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileNewLineDefaults {
    fringes: bool,
    groups: bool,
    set: bool,
    location: bool,
    currency: bool,
}

impl From<&NewLineDefaults> for FileNewLineDefaults {
    fn from(d: &NewLineDefaults) -> FileNewLineDefaults {
        let NewLineDefaults {
            fringes,
            groups,
            set,
            location,
            currency,
        } = *d;
        FileNewLineDefaults {
            fringes,
            groups,
            set,
            location,
            currency,
        }
    }
}

impl From<FileNewLineDefaults> for NewLineDefaults {
    fn from(f: FileNewLineDefaults) -> NewLineDefaults {
        NewLineDefaults {
            fringes: f.fringes,
            groups: f.groups,
            set: f.set,
            location: f.location,
            currency: f.currency,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FilePosting {
    Budget {},
    ProductionLevel {},
    Category { suffix: String },
    Account {},
}

impl From<&FringePosting> for FilePosting {
    fn from(p: &FringePosting) -> FilePosting {
        match p {
            FringePosting::Budget => FilePosting::Budget {},
            FringePosting::ProductionLevel => FilePosting::ProductionLevel {},
            FringePosting::Category { suffix } => FilePosting::Category {
                suffix: suffix.clone(),
            },
            FringePosting::Account => FilePosting::Account {},
        }
    }
}

impl From<FilePosting> for FringePosting {
    fn from(p: FilePosting) -> FringePosting {
        match p {
            FilePosting::Budget {} => FringePosting::Budget,
            FilePosting::ProductionLevel {} => FringePosting::ProductionLevel,
            FilePosting::Category { suffix } => FringePosting::Category { suffix },
            FilePosting::Account {} => FringePosting::Account,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FileInfo {
    Text { value: String },
    Number { value: DecStr },
    Date { value: String },
}

impl From<&InfoValue> for FileInfo {
    fn from(v: &InfoValue) -> FileInfo {
        match v {
            InfoValue::Text(s) => FileInfo::Text { value: s.clone() },
            InfoValue::Number(d) => FileInfo::Number { value: DecStr(*d) },
            InfoValue::Date(s) => FileInfo::Date { value: s.clone() },
        }
    }
}

impl From<FileInfo> for InfoValue {
    fn from(v: FileInfo) -> InfoValue {
        match v {
            FileInfo::Text { value } => InfoValue::Text(value),
            FileInfo::Number { value } => InfoValue::Number(value.0),
            FileInfo::Date { value } => InfoValue::Date(value),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileCurrency {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    symbol: String,
    rate: DecStr,
}

impl From<&Currency> for FileCurrency {
    fn from(c: &Currency) -> FileCurrency {
        let Currency {
            description,
            symbol,
            rate,
        } = c;
        FileCurrency {
            description: description.clone(),
            symbol: symbol.clone(),
            rate: DecStr(*rate),
        }
    }
}

impl From<FileCurrency> for Currency {
    fn from(f: FileCurrency) -> Currency {
        Currency {
            description: f.description,
            symbol: f.symbol,
            rate: f.rate.0,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileUnit {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    plural: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hours: Option<DecStr>,
}

impl From<&UnitDef> for FileUnit {
    fn from(u: &UnitDef) -> FileUnit {
        let UnitDef {
            plural,
            description,
            hours,
        } = u;
        FileUnit {
            plural: plural.clone(),
            description: description.clone(),
            hours: hours.map(DecStr),
        }
    }
}

impl From<FileUnit> for UnitDef {
    fn from(f: FileUnit) -> UnitDef {
        UnitDef {
            plural: f.plural,
            description: f.description,
            hours: f.hours.map(|h| h.0),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FileFringe {
    Percent {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        description: String,
        rate: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cutoff: Option<Cents>,
    },
    Flat {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        description: String,
        rate: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
    },
}

impl From<&Fringe> for FileFringe {
    fn from(f: &Fringe) -> FileFringe {
        let Fringe {
            description,
            rate,
            kind,
        } = f;
        let (description, rate) = (description.clone(), rate.0.clone());
        match kind {
            FringeKind::Percent { cutoff } => FileFringe::Percent {
                description,
                rate,
                cutoff: *cutoff,
            },
            FringeKind::Flat { unit } => FileFringe::Flat {
                description,
                rate,
                unit: unit.clone(),
            },
        }
    }
}

impl From<FileFringe> for Fringe {
    fn from(f: FileFringe) -> Fringe {
        match f {
            FileFringe::Percent {
                description,
                rate,
                cutoff,
            } => Fringe {
                description,
                rate: Formula(rate),
                kind: FringeKind::Percent { cutoff },
            },
            FileFringe::Flat {
                description,
                rate,
                unit,
            } => Fringe {
                description,
                rate: Formula(rate),
                kind: FringeKind::Flat { unit },
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileGroup {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    in_budget_total: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
}

impl From<&Group> for FileGroup {
    fn from(g: &Group) -> FileGroup {
        let Group {
            description,
            in_budget_total,
            color,
        } = g;
        FileGroup {
            description: description.clone(),
            in_budget_total: *in_budget_total,
            color: color.clone(),
        }
    }
}

impl From<FileGroup> for Group {
    fn from(f: FileGroup) -> Group {
        Group {
            description: f.description,
            in_budget_total: f.in_budget_total,
            color: f.color,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileSet {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
}

impl From<&SetDef> for FileSet {
    fn from(s: &SetDef) -> FileSet {
        let SetDef { description } = s;
        FileSet {
            description: description.clone(),
        }
    }
}

impl From<FileSet> for SetDef {
    fn from(f: FileSet) -> SetDef {
        SetDef {
            description: f.description,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileLocation {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    city: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    region: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    country: String,
}

impl From<&Location> for FileLocation {
    fn from(l: &Location) -> FileLocation {
        let Location {
            description,
            city,
            region,
            country,
        } = l;
        FileLocation {
            description: description.clone(),
            city: city.clone(),
            region: region.clone(),
            country: country.clone(),
        }
    }
}

impl From<FileLocation> for Location {
    fn from(f: FileLocation) -> Location {
        Location {
            description: f.description,
            city: f.city,
            region: f.region,
            country: f.country,
        }
    }
}

// ---------------------------------------------------------------------------
// Topsheet, accounts, detail rows

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TopsheetKind {
    #[default]
    Category,
    ProductionTotal,
}

/// A topsheet row. Not a serde-tagged enum: those buffer the whole object before
/// parsing it, which would report an error deep inside a category's details at the
/// end of the category. As a plain struct, field errors keep their exact position;
/// only the kind/field combination is checked afterwards (in `try_from`).
#[derive(Serialize, Deserialize)]
#[serde(try_from = "RawTopsheetRow")]
struct FileTopsheetRow {
    kind: TopsheetKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    accounts: Vec<FileAccount>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTopsheetRow {
    kind: TopsheetKind,
    #[serde(default)]
    code: Option<String>,
    name: String,
    #[serde(default)]
    accounts: Option<Vec<FileAccount>>,
}

impl TryFrom<RawTopsheetRow> for FileTopsheetRow {
    type Error = String;

    fn try_from(r: RawTopsheetRow) -> Result<FileTopsheetRow, String> {
        match r.kind {
            TopsheetKind::Category => Ok(FileTopsheetRow {
                kind: r.kind,
                code: Some(r.code.ok_or("missing field `code` on a category row")?),
                name: r.name,
                accounts: r.accounts.unwrap_or_default(),
            }),
            TopsheetKind::ProductionTotal => {
                if r.code.is_some() {
                    return Err("a production_total row has no `code`".into());
                }
                if r.accounts.is_some() {
                    return Err("a production_total row has no `accounts`".into());
                }
                Ok(FileTopsheetRow {
                    kind: r.kind,
                    code: None,
                    name: r.name,
                    accounts: Vec::new(),
                })
            }
        }
    }
}

impl From<&TopsheetRow> for FileTopsheetRow {
    fn from(r: &TopsheetRow) -> FileTopsheetRow {
        match r {
            TopsheetRow::Category(Category {
                code,
                name,
                accounts,
            }) => FileTopsheetRow {
                kind: TopsheetKind::Category,
                code: Some(code.clone()),
                name: name.clone(),
                accounts: accounts.iter().map(FileAccount::from).collect(),
            },
            TopsheetRow::ProductionTotal(ProductionTotal { name }) => FileTopsheetRow {
                kind: TopsheetKind::ProductionTotal,
                code: None,
                name: name.clone(),
                accounts: Vec::new(),
            },
        }
    }
}

impl From<FileTopsheetRow> for TopsheetRow {
    fn from(f: FileTopsheetRow) -> TopsheetRow {
        match f.kind {
            TopsheetKind::Category => TopsheetRow::Category(Category {
                code: f.code.unwrap_or_default(),
                name: f.name,
                accounts: f.accounts.into_iter().map(Account::from).collect(),
            }),
            TopsheetKind::ProductionTotal => {
                TopsheetRow::ProductionTotal(ProductionTotal { name: f.name })
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileAccount {
    code: String,
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    details: Vec<FileDetail>,
}

impl From<&Account> for FileAccount {
    fn from(a: &Account) -> FileAccount {
        let Account {
            code,
            name,
            details,
        } = a;
        FileAccount {
            code: code.clone(),
            name: name.clone(),
            details: details.iter().map(FileDetail::from).collect(),
        }
    }
}

impl From<FileAccount> for Account {
    fn from(f: FileAccount) -> Account {
        Account {
            code: f.code,
            name: f.name,
            details: f.details.into_iter().map(Detail::from).collect(),
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileRowKind {
    #[default]
    Line,
    Subtotal,
    Adjustment,
    Separator,
}

impl FileRowKind {
    fn is_line(&self) -> bool {
        *self == FileRowKind::Line
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDetail {
    id: RowId,
    #[serde(default, skip_serializing_if = "FileRowKind::is_line")]
    kind: FileRowKind,
    description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    amount: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    unit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    x: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    unit2: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    rate: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    unit3: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    x4: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    unit4: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fringe_range: Option<u32>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    fringes: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    groups: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    set: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    location: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    italic: bool,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    indent: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

impl From<&Detail> for FileDetail {
    fn from(d: &Detail) -> FileDetail {
        let Detail {
            id,
            kind,
            description,
            amount,
            unit,
            x,
            unit2,
            rate,
            unit3,
            x4,
            unit4,
            currency,
            fringes,
            groups,
            location,
            set,
            fringe_range,
            bold,
            italic,
            indent,
            color,
            note,
        } = d;
        FileDetail {
            id: *id,
            kind: match kind {
                RowKind::Line => FileRowKind::Line,
                RowKind::Subtotal => FileRowKind::Subtotal,
                RowKind::Adjustment => FileRowKind::Adjustment,
                RowKind::Separator => FileRowKind::Separator,
            },
            description: description.clone(),
            amount: amount.0.clone(),
            unit: unit.clone(),
            x: x.0.clone(),
            unit2: unit2.clone(),
            rate: rate.0.clone(),
            unit3: unit3.clone(),
            x4: x4.0.clone(),
            unit4: unit4.clone(),
            currency: currency.clone(),
            fringe_range: *fringe_range,
            fringes: fringes.clone(),
            groups: groups.clone(),
            set: set.clone(),
            location: location.clone(),
            bold: *bold,
            italic: *italic,
            indent: *indent,
            color: color.clone(),
            note: note.clone(),
        }
    }
}

impl From<FileDetail> for Detail {
    fn from(f: FileDetail) -> Detail {
        Detail {
            id: f.id,
            kind: match f.kind {
                FileRowKind::Line => RowKind::Line,
                FileRowKind::Subtotal => RowKind::Subtotal,
                FileRowKind::Adjustment => RowKind::Adjustment,
                FileRowKind::Separator => RowKind::Separator,
            },
            description: f.description,
            amount: Formula(f.amount),
            unit: f.unit,
            x: Formula(f.x),
            unit2: f.unit2,
            rate: Formula(f.rate),
            unit3: f.unit3,
            x4: Formula(f.x4),
            unit4: f.unit4,
            currency: f.currency,
            fringes: f.fringes,
            groups: f.groups,
            location: f.location,
            set: f.set,
            fringe_range: f.fringe_range,
            bold: f.bold,
            italic: f.italic,
            indent: f.indent,
            color: f.color,
            note: f.note,
        }
    }
}

// ---------------------------------------------------------------------------
// Charges and credits

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileExcludedAmount {
    description: String,
    amount: Cents,
}

/// Fields every charge has, whatever its kind.
struct ChargeCommon {
    name: String,
    account: Option<String>,
    applied: bool,
    excluded_amounts: Vec<FileExcludedAmount>,
    excluded_groups: BTreeSet<String>,
    excluded_categories: BTreeSet<String>,
    excluded_accounts: BTreeSet<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FileCharge {
    Percent {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        applied: bool,
        percent: DecStr,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        excluded_amounts: Vec<FileExcludedAmount>,
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        excluded_groups: BTreeSet<String>,
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        excluded_categories: BTreeSet<String>,
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        excluded_accounts: BTreeSet<String>,
    },
    Flat {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        applied: bool,
        amount: Cents,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        currency: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        excluded_amounts: Vec<FileExcludedAmount>,
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        excluded_groups: BTreeSet<String>,
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        excluded_categories: BTreeSet<String>,
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        excluded_accounts: BTreeSet<String>,
    },
}

impl From<&Charge> for FileCharge {
    fn from(c: &Charge) -> FileCharge {
        let Charge {
            name,
            account,
            applied,
            kind,
            excluded_amounts,
            excluded_groups,
            excluded_categories,
            excluded_accounts,
        } = c;
        let c = ChargeCommon {
            name: name.clone(),
            account: account.clone(),
            applied: *applied,
            excluded_amounts: excluded_amounts
                .iter()
                .map(|(description, amount)| FileExcludedAmount {
                    description: description.clone(),
                    amount: *amount,
                })
                .collect(),
            excluded_groups: excluded_groups.clone(),
            excluded_categories: excluded_categories.clone(),
            excluded_accounts: excluded_accounts.clone(),
        };
        match kind {
            ChargeKind::Percent(p) => FileCharge::Percent {
                name: c.name,
                account: c.account,
                applied: c.applied,
                percent: DecStr(*p),
                excluded_amounts: c.excluded_amounts,
                excluded_groups: c.excluded_groups,
                excluded_categories: c.excluded_categories,
                excluded_accounts: c.excluded_accounts,
            },
            ChargeKind::Flat { amount, currency } => FileCharge::Flat {
                name: c.name,
                account: c.account,
                applied: c.applied,
                amount: *amount,
                currency: currency.clone(),
                excluded_amounts: c.excluded_amounts,
                excluded_groups: c.excluded_groups,
                excluded_categories: c.excluded_categories,
                excluded_accounts: c.excluded_accounts,
            },
        }
    }
}

impl From<FileCharge> for Charge {
    fn from(f: FileCharge) -> Charge {
        let (c, kind) = match f {
            FileCharge::Percent {
                name,
                account,
                applied,
                percent,
                excluded_amounts,
                excluded_groups,
                excluded_categories,
                excluded_accounts,
            } => (
                ChargeCommon {
                    name,
                    account,
                    applied,
                    excluded_amounts,
                    excluded_groups,
                    excluded_categories,
                    excluded_accounts,
                },
                ChargeKind::Percent(percent.0),
            ),
            FileCharge::Flat {
                name,
                account,
                applied,
                amount,
                currency,
                excluded_amounts,
                excluded_groups,
                excluded_categories,
                excluded_accounts,
            } => (
                ChargeCommon {
                    name,
                    account,
                    applied,
                    excluded_amounts,
                    excluded_groups,
                    excluded_categories,
                    excluded_accounts,
                },
                ChargeKind::Flat { amount, currency },
            ),
        };
        Charge {
            name: c.name,
            account: c.account,
            applied: c.applied,
            kind,
            excluded_amounts: c
                .excluded_amounts
                .into_iter()
                .map(|e| (e.description, e.amount))
                .collect(),
            excluded_groups: c.excluded_groups,
            excluded_categories: c.excluded_categories,
            excluded_accounts: c.excluded_accounts,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileCredit {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account: Option<String>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    applied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cap: Option<Cents>,
    #[serde(default = "zero", skip_serializing_if = "DecStr::is_zero")]
    discount_percent: DecStr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rows: Vec<FileCreditRow>,
}

fn zero() -> DecStr {
    DecStr(Dec::ZERO)
}

impl From<&Credit> for FileCredit {
    fn from(c: &Credit) -> FileCredit {
        let Credit {
            name,
            account,
            applied,
            cap,
            discount_percent,
            rows,
        } = c;
        FileCredit {
            name: name.clone(),
            account: account.clone(),
            applied: *applied,
            cap: *cap,
            discount_percent: DecStr(*discount_percent),
            rows: rows.iter().map(FileCreditRow::from).collect(),
        }
    }
}

impl From<FileCredit> for Credit {
    fn from(f: FileCredit) -> Credit {
        Credit {
            name: f.name,
            account: f.account,
            applied: f.applied,
            cap: f.cap,
            discount_percent: f.discount_percent.0,
            rows: f.rows.into_iter().map(CreditRow::from).collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileCreditRow {
    description: String,
    source: FileCreditSource,
    percent: DecStr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cap: Option<FileCreditCap>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FileCreditSource {
    Flat {
        amount: Cents,
    },
    Group {
        group: String,
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        excluded_fringes: BTreeSet<String>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FileCreditCap {
    QualifiedSpend { amount: Cents },
    Credit { amount: Cents },
}

impl From<&CreditRow> for FileCreditRow {
    fn from(r: &CreditRow) -> FileCreditRow {
        let CreditRow {
            description,
            source,
            percent,
            cap,
        } = r;
        FileCreditRow {
            description: description.clone(),
            source: match source {
                CreditSource::Flat(amount) => FileCreditSource::Flat { amount: *amount },
                CreditSource::Group {
                    group,
                    excluded_fringes,
                } => FileCreditSource::Group {
                    group: group.clone(),
                    excluded_fringes: excluded_fringes.clone(),
                },
            },
            percent: DecStr(*percent),
            cap: match *cap {
                CreditCap::None => None,
                CreditCap::QualifiedSpend(amount) => Some(FileCreditCap::QualifiedSpend { amount }),
                CreditCap::Credit(amount) => Some(FileCreditCap::Credit { amount }),
            },
        }
    }
}

impl From<FileCreditRow> for CreditRow {
    fn from(f: FileCreditRow) -> CreditRow {
        CreditRow {
            description: f.description,
            source: match f.source {
                FileCreditSource::Flat { amount } => CreditSource::Flat(amount),
                FileCreditSource::Group {
                    group,
                    excluded_fringes,
                } => CreditSource::Group {
                    group,
                    excluded_fringes,
                },
            },
            percent: f.percent.0,
            cap: match f.cap {
                None => CreditCap::None,
                Some(FileCreditCap::QualifiedSpend { amount }) => CreditCap::QualifiedSpend(amount),
                Some(FileCreditCap::Credit { amount }) => CreditCap::Credit(amount),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Baseline

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileBaseline {
    /// Keyed by row id; JSON object keys are strings, so ids are written as `"17"`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    rows: BTreeMap<RowId, Cents>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    accounts: BTreeMap<String, Cents>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    categories: BTreeMap<String, Cents>,
    grand_total: Cents,
    net_total: Cents,
}

impl From<&Baseline> for FileBaseline {
    fn from(b: &Baseline) -> FileBaseline {
        let Baseline {
            rows,
            accounts,
            categories,
            grand_total,
            net_total,
        } = b;
        FileBaseline {
            rows: rows.clone(),
            accounts: accounts.clone(),
            categories: categories.clone(),
            grand_total: *grand_total,
            net_total: *net_total,
        }
    }
}

impl From<FileBaseline> for Baseline {
    fn from(f: FileBaseline) -> Baseline {
        Baseline {
            rows: f.rows,
            accounts: f.accounts,
            categories: f.categories,
            grand_total: f.grand_total,
            net_total: f.net_total,
        }
    }
}

// ---------------------------------------------------------------------------
// Report presets (saved report setups; fwb_core::report_preset)

/// A saved report setup: a name, the default flag and the spec's fields, all
/// in one object.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileReportPreset {
    name: String,
    #[serde(default, skip_serializing_if = "is_false")]
    default: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sections: Vec<FileSection>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    excluded_categories: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    excluded_accounts: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    skip_empty_accounts: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    skip_zero_accounts: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    skip_suppressed_lines: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    skip_notes: bool,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    only_groups: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    columns: Option<Vec<FileColumn>>,
    #[serde(default, skip_serializing_if = "is_false")]
    landscape: bool,
    #[serde(default, skip_serializing_if = "FileFontSize::is_medium")]
    font_size: FileFontSize,
    #[serde(default, skip_serializing_if = "is_false")]
    shading: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    group_colors: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    header: Option<FileBand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    footer: Option<FileBand>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileSection {
    Topsheet,
    Accounts,
    Detail,
    Fringes,
    Globals,
    Groups,
    Notes,
    Info,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileColumn {
    Amount,
    Unit,
    X,
    Unit2,
    Rate,
    Unit3,
    Currency,
    X4,
    Unit4,
    Fringes,
    #[serde(rename = "fringe_pct")]
    FringePercent,
    FringeTotal,
    Groups,
    Set,
    Location,
    Original,
    Variance,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileFontSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl FileFontSize {
    fn is_medium(&self) -> bool {
        *self == FileFontSize::Medium
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileBand {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    left: Option<FileBandField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    center: Option<FileBandField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    right: Option<FileBandField>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FileBandField {
    Title {},
    Section {},
    Printed {},
    File {},
    Page {},
    Info { label: String },
    Text { text: String },
}

macro_rules! same_variants {
    ($a:ident <-> $b:ident { $($v:ident),+ $(,)? }) => {
        impl From<$a> for $b {
            fn from(x: $a) -> $b {
                match x {
                    $($a::$v => $b::$v),+
                }
            }
        }
        impl From<$b> for $a {
            fn from(x: $b) -> $a {
                match x {
                    $($b::$v => $a::$v),+
                }
            }
        }
    };
}

same_variants!(ReportSection <-> FileSection {
    Topsheet, Accounts, Detail, Fringes, Globals, Groups, Notes, Info,
});
same_variants!(ReportColumn <-> FileColumn {
    Amount, Unit, X, Unit2, Rate, Unit3, Currency, X4, Unit4, Fringes, FringePercent,
    FringeTotal, Groups, Set, Location, Original, Variance,
});
same_variants!(FontSize <-> FileFontSize { Small, Medium, Large });

impl From<&BandField> for FileBandField {
    fn from(f: &BandField) -> FileBandField {
        match f {
            BandField::Title => FileBandField::Title {},
            BandField::Section => FileBandField::Section {},
            BandField::Printed => FileBandField::Printed {},
            BandField::File => FileBandField::File {},
            BandField::Page => FileBandField::Page {},
            BandField::Info(label) => FileBandField::Info {
                label: label.clone(),
            },
            BandField::Text(text) => FileBandField::Text { text: text.clone() },
        }
    }
}

impl From<FileBandField> for BandField {
    fn from(f: FileBandField) -> BandField {
        match f {
            FileBandField::Title {} => BandField::Title,
            FileBandField::Section {} => BandField::Section,
            FileBandField::Printed {} => BandField::Printed,
            FileBandField::File {} => BandField::File,
            FileBandField::Page {} => BandField::Page,
            FileBandField::Info { label } => BandField::Info(label),
            FileBandField::Text { text } => BandField::Text(text),
        }
    }
}

impl From<&Band> for FileBand {
    fn from(b: &Band) -> FileBand {
        let Band {
            left,
            center,
            right,
        } = b;
        FileBand {
            left: left.as_ref().map(FileBandField::from),
            center: center.as_ref().map(FileBandField::from),
            right: right.as_ref().map(FileBandField::from),
        }
    }
}

impl From<FileBand> for Band {
    fn from(f: FileBand) -> Band {
        Band {
            left: f.left.map(BandField::from),
            center: f.center.map(BandField::from),
            right: f.right.map(BandField::from),
        }
    }
}

impl From<&ReportPreset> for FileReportPreset {
    fn from(p: &ReportPreset) -> FileReportPreset {
        let ReportPreset {
            name,
            default,
            spec,
        } = p;
        let ReportSpec {
            sections,
            excluded_categories,
            excluded_accounts,
            skip_empty_accounts,
            skip_zero_accounts,
            skip_suppressed_lines,
            skip_notes,
            only_groups,
            columns,
            landscape,
            font_size,
            shading,
            group_colors,
            header,
            footer,
        } = spec;
        FileReportPreset {
            name: name.clone(),
            default: *default,
            sections: sections.iter().map(|&x| x.into()).collect(),
            excluded_categories: excluded_categories.clone(),
            excluded_accounts: excluded_accounts.clone(),
            skip_empty_accounts: *skip_empty_accounts,
            skip_zero_accounts: *skip_zero_accounts,
            skip_suppressed_lines: *skip_suppressed_lines,
            skip_notes: *skip_notes,
            only_groups: only_groups.clone(),
            columns: columns
                .as_ref()
                .map(|c| c.iter().map(|&x| x.into()).collect()),
            landscape: *landscape,
            font_size: (*font_size).into(),
            shading: *shading,
            group_colors: *group_colors,
            header: header.as_ref().map(FileBand::from),
            footer: footer.as_ref().map(FileBand::from),
        }
    }
}

impl From<FileReportPreset> for ReportPreset {
    fn from(f: FileReportPreset) -> ReportPreset {
        ReportPreset {
            name: f.name,
            default: f.default,
            spec: ReportSpec {
                sections: f.sections.into_iter().map(Into::into).collect(),
                excluded_categories: f.excluded_categories,
                excluded_accounts: f.excluded_accounts,
                skip_empty_accounts: f.skip_empty_accounts,
                skip_zero_accounts: f.skip_zero_accounts,
                skip_suppressed_lines: f.skip_suppressed_lines,
                skip_notes: f.skip_notes,
                only_groups: f.only_groups,
                columns: f.columns.map(|c| c.into_iter().map(Into::into).collect()),
                landscape: f.landscape,
                font_size: f.font_size.into(),
                shading: f.shading,
                group_colors: f.group_colors,
                header: f.header.map(Band::from),
                footer: f.footer.map(Band::from),
            },
        }
    }
}
