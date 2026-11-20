//! Budget settings.

use super::*;

// ---------------------------------------------------------------------------
// Budget settings

/// Written only when it differs from the default; then every field is present.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileCoding {
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
pub(crate) struct FileNewLineDefaults {
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

/// The budget's percent fringe cutoff factor as text, `"1.5"`: one of
/// `1.0`, `1.5`, `2.0`, `2.5` and `3.0` (`"2"` reads as `2.0`). Left out at
/// 1.0.
pub(crate) struct FileCutoffFactor(pub(super) CutoffFactor);

impl Serialize for FileCutoffFactor {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0.name())
    }
}

impl<'de> Deserialize<'de> for FileCutoffFactor {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<FileCutoffFactor, D::Error> {
        let text = String::deserialize(d)?;
        CutoffFactor::parse(&text)
            .map(FileCutoffFactor)
            .ok_or_else(|| {
                de::Error::custom(format!(
                    "cutoff_factor {text:?} is not one of \"1.0\", \"1.5\", \"2.0\", \"2.5\", \"3.0\""
                ))
            })
    }
}

/// The budget's one fringe row (fringes posted by budget): its account
/// number, description and place. Left out when all are the defaults.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileFringeRow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    position: Option<usize>,
}

impl From<&FringeRow> for FileFringeRow {
    fn from(r: &FringeRow) -> FileFringeRow {
        let FringeRow {
            account,
            description,
            position,
        } = r;
        FileFringeRow {
            account: account.clone(),
            description: description.clone(),
            position: *position,
        }
    }
}

impl From<FileFringeRow> for FringeRow {
    fn from(f: FileFringeRow) -> FringeRow {
        let blank = |s: Option<String>| s.filter(|s| !s.trim().is_empty());
        FringeRow {
            account: blank(f.account),
            description: blank(f.description),
            position: f.position,
        }
    }
}

/// Under production-level posting, what happens to the fringes of
/// categories after the last production total: `"keep"` (the default, left
/// out) or `"drop"`.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FileTrailingFringes {
    Keep,
    Drop,
}

impl From<TrailingFringes> for FileTrailingFringes {
    fn from(t: TrailingFringes) -> FileTrailingFringes {
        match t {
            TrailingFringes::Keep => FileTrailingFringes::Keep,
            TrailingFringes::Drop => FileTrailingFringes::Drop,
        }
    }
}

impl From<FileTrailingFringes> for TrailingFringes {
    fn from(t: FileTrailingFringes) -> TrailingFringes {
        match t {
            FileTrailingFringes::Keep => TrailingFringes::Keep,
            FileTrailingFringes::Drop => TrailingFringes::Drop,
        }
    }
}

/// How a budget works out its totals: `"binary"` (the default, left out) or
/// `"exact"`. `"mmb"`, the earlier name of `"binary"`, is still read.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FileArithmetic {
    #[serde(rename = "binary", alias = "mmb")]
    Mmb,
    Exact,
}

impl From<Arithmetic> for FileArithmetic {
    fn from(a: Arithmetic) -> FileArithmetic {
        match a {
            Arithmetic::Mmb => FileArithmetic::Mmb,
            Arithmetic::Exact => FileArithmetic::Exact,
        }
    }
}

impl From<FileArithmetic> for Arithmetic {
    fn from(a: FileArithmetic) -> Arithmetic {
        match a {
            FileArithmetic::Mmb => Arithmetic::Mmb,
            FileArithmetic::Exact => Arithmetic::Exact,
        }
    }
}

/// One of Movie Magic's group rules: `"include"` or `"exclude"`.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileInclusion {
    Include,
    Exclude,
}

impl From<Inclusion> for FileInclusion {
    fn from(i: Inclusion) -> FileInclusion {
        match i {
            Inclusion::Include => FileInclusion::Include,
            Inclusion::Exclude => FileInclusion::Exclude,
        }
    }
}

impl From<FileInclusion> for Inclusion {
    fn from(i: FileInclusion) -> Inclusion {
        match i {
            FileInclusion::Include => Inclusion::Include,
            FileInclusion::Exclude => Inclusion::Exclude,
        }
    }
}

/// The budget-wide group rules (decisions.md, entry 57). The whole object
/// is left out when every rule is Movie Magic's default, and so is each
/// rule that is.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileGroupRules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ungrouped_lines: Option<FileInclusion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    conflict: Option<FileInclusion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fringes_in_totals: Option<FileInclusion>,
}

impl From<GroupRules> for FileGroupRules {
    fn from(r: GroupRules) -> FileGroupRules {
        let d = GroupRules::default();
        let write = |v: Inclusion, def: Inclusion| (v != def).then(|| FileInclusion::from(v));
        FileGroupRules {
            ungrouped_lines: write(r.ungrouped_lines, d.ungrouped_lines),
            conflict: write(r.conflict, d.conflict),
            fringes_in_totals: write(r.fringes_in_totals, d.fringes_in_totals),
        }
    }
}

impl From<FileGroupRules> for GroupRules {
    fn from(f: FileGroupRules) -> GroupRules {
        let d = GroupRules::default();
        let read = |v: Option<FileInclusion>, def: Inclusion| v.map_or(def, Inclusion::from);
        GroupRules {
            ungrouped_lines: read(f.ungrouped_lines, d.ungrouped_lines),
            conflict: read(f.conflict, d.conflict),
            fringes_in_totals: read(f.fringes_in_totals, d.fringes_in_totals),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum FilePosting {
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
pub(crate) enum FileInfo {
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
pub(super) struct FileCurrency {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    symbol: String,
    rate: DecStr,
    /// Version 1, additive (decisions.md, entry 61): left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    symbol_after: bool,
    /// `comma_point` (`1,234.56`, left out) or `point_comma` (`1.234,56`).
    #[serde(
        default,
        skip_serializing_if = "is_comma_point",
        with = "separators_name"
    )]
    separators: Separators,
}

fn is_comma_point(s: &Separators) -> bool {
    *s == Separators::CommaPoint
}

/// A currency's separators by name (`comma_point`, `point_comma`).
mod separators_name {
    use method_core::Separators;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(s: &Separators, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(s.name())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Separators, D::Error> {
        let text = String::deserialize(de)?;
        match text.as_str() {
            "comma_point" => Ok(Separators::CommaPoint),
            "point_comma" => Ok(Separators::PointComma),
            other => Err(serde::de::Error::custom(format!(
                "unknown separators `{other}` (comma_point or point_comma)"
            ))),
        }
    }
}

impl From<&Currency> for FileCurrency {
    fn from(c: &Currency) -> FileCurrency {
        let Currency {
            description,
            symbol,
            rate,
            format,
        } = c;
        FileCurrency {
            description: description.clone(),
            symbol: symbol.clone(),
            rate: DecStr(*rate),
            symbol_after: format.symbol_after,
            separators: format.separators,
        }
    }
}

impl From<FileCurrency> for Currency {
    fn from(f: FileCurrency) -> Currency {
        Currency {
            description: f.description,
            symbol: f.symbol,
            rate: f.rate.0,
            format: CurrencyFormat {
                symbol_after: f.symbol_after,
                separators: f.separators,
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileUnit {
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
pub(super) enum FileFringe {
    Percent {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        description: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        short_code: Option<String>,
        rate: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cutoff: Option<Cents>,
    },
    Flat {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        description: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        short_code: Option<String>,
        rate: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
        #[serde(default, skip_serializing_if = "FileBasis::is_amount")]
        basis: FileBasis,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_hours_per_day: Option<DecStr>,
        /// Kept, never used: cutoffs apply to percent fringes only.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cutoff: Option<Cents>,
    },
}

/// What a flat fringe counts on a line (docs/FORMAT.md, fringe).
#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FileBasis {
    #[default]
    Amount,
    AmountCounts,
    Quantity,
}

impl FileBasis {
    fn is_amount(&self) -> bool {
        *self == FileBasis::Amount
    }
}

impl From<FlatBasis> for FileBasis {
    fn from(b: FlatBasis) -> FileBasis {
        match b {
            FlatBasis::Amount => FileBasis::Amount,
            FlatBasis::AmountCounts => FileBasis::AmountCounts,
            FlatBasis::Quantity => FileBasis::Quantity,
        }
    }
}

impl From<FileBasis> for FlatBasis {
    fn from(b: FileBasis) -> FlatBasis {
        match b {
            FileBasis::Amount => FlatBasis::Amount,
            FileBasis::AmountCounts => FlatBasis::AmountCounts,
            FileBasis::Quantity => FlatBasis::Quantity,
        }
    }
}

impl From<&Fringe> for FileFringe {
    fn from(f: &Fringe) -> FileFringe {
        let Fringe {
            description,
            short_code,
            rate,
            kind,
        } = f;
        let (description, short_code, rate) =
            (description.clone(), short_code.clone(), rate.0.clone());
        match kind {
            FringeKind::Percent { cutoff } => FileFringe::Percent {
                description,
                short_code,
                rate,
                cutoff: *cutoff,
            },
            FringeKind::Flat {
                unit,
                basis,
                max_hours_per_day,
                cutoff,
            } => FileFringe::Flat {
                description,
                short_code,
                rate,
                unit: unit.clone(),
                basis: (*basis).into(),
                max_hours_per_day: max_hours_per_day.map(DecStr),
                cutoff: *cutoff,
            },
        }
    }
}

impl From<FileFringe> for Fringe {
    fn from(f: FileFringe) -> Fringe {
        match f {
            FileFringe::Percent {
                description,
                short_code,
                rate,
                cutoff,
            } => Fringe {
                description,
                short_code,
                rate: Formula(rate),
                kind: FringeKind::Percent { cutoff },
            },
            FileFringe::Flat {
                description,
                short_code,
                rate,
                unit,
                basis,
                max_hours_per_day,
                cutoff,
            } => Fringe {
                description,
                short_code,
                rate: Formula(rate),
                kind: FringeKind::Flat {
                    unit,
                    basis: basis.into(),
                    max_hours_per_day: max_hours_per_day.map(|h| h.0),
                    cutoff,
                },
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileGroup {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    short_code: Option<String>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    in_budget_total: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
}

impl From<&Group> for FileGroup {
    fn from(g: &Group) -> FileGroup {
        let Group {
            description,
            short_code,
            in_budget_total,
            color,
        } = g;
        FileGroup {
            description: description.clone(),
            short_code: short_code.clone(),
            in_budget_total: *in_budget_total,
            color: color.clone(),
        }
    }
}

impl From<FileGroup> for Group {
    fn from(f: FileGroup) -> Group {
        Group {
            description: f.description,
            short_code: f.short_code,
            in_budget_total: f.in_budget_total,
            color: f.color,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileSet {
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
pub(super) struct FileLocation {
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
