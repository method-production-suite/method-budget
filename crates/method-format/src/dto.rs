//! File-side types: the exact shape of the JSON, kept separate from the engine
//! model so `method-core` stays dependency-free and the format can evolve on its own.
//!
//! Every conversion destructures the core type completely, so adding a field to
//! the model is a compile error here until the format handles it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use method_core::actuals::{Actuals, EtcMethod, ImportBatch, Transaction};
use method_core::coding::AccountCoding;
use method_core::order::SetupOrder;
use method_core::pack::{ItemSource, PackInfo};
use method_core::{
    Account, Arithmetic, Baseline, Budget, Category, Cents, Charge, ChargeKind, Credit, CreditCap,
    CreditRow, CreditSource, Currency, CutoffFactor, Dec, Detail, DisplayPrefs, FlatBasis, Formula,
    Fringe, FringeKind, FringePosting, FringeRow, GlobalInfo, Group, GroupRules, Inclusion,
    InfoValue, Location, NewLineDefaults, Note, ProductionTotal, RowId, RowKind, SetDef,
    TopsheetRow, TrailingFringes, UnitDef, Version,
};
use method_core::{
    Band, BandField, BidField, BidLayout, BidLine, BidPick, FontSize, ReportColumn, ReportPreset,
    ReportSection, ReportSpec,
};
use method_core::{CurrencyFormat, Separators};
use method_core::{LinePreset, SetupItems, TagPreset};
use method_core::{Provenance, ProvenanceKind};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

mod accounts;
mod actuals;
mod charges;
mod meta;
mod presets;
mod rates;
mod reports;
mod settings;

use accounts::{FileBlockRow, FileTopsheetRow};
use actuals::FileActuals;
pub(crate) use actuals::FileBaseline;
pub(crate) use charges::FileSharedCap;
use charges::{FileCharge, FileCredit};
use meta::{file_notes, read_notes, FileGlobalInfo, FileNote, FileProvenance};
pub(crate) use meta::{FileDisplay, FileItemSource, FileSetupOrder, FileSnapshot, FileVersion};
pub(crate) use presets::{FileLinePreset, FileTagPreset};
pub(crate) use rates::{FileOtPattern, FileRateSource, FileRateTable};
pub(crate) use reports::{FileBid, FileReportPreset};
use settings::{
    FileArithmetic, FileCurrency, FileFringe, FileGroup, FileLocation, FileSet,
    FileTrailingFringes, FileUnit,
};
pub(crate) use settings::{
    FileCoding, FileCutoffFactor, FileFringeRow, FileGroupRules, FileInfo, FileNewLineDefaults,
    FilePosting,
};

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
    provenance: Option<FileProvenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_coding: Option<FileCoding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fringe_posting: Option<FilePosting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trailing_level_fringes: Option<FileTrailingFringes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cutoff_factor: Option<FileCutoffFactor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fringe_row: Option<FileFringeRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    arithmetic: Option<FileArithmetic>,
    #[serde(default, skip_serializing_if = "is_false")]
    round_account_totals: bool,
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
    global_info: BTreeMap<String, FileGlobalInfo>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    fringes: BTreeMap<String, FileFringe>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    groups: BTreeMap<String, FileGroup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group_rules: Option<FileGroupRules>,
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
    /// Caps credits share, by name (decisions.md, entry 88). Left out when
    /// there are none, so other files are unchanged.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    shared_caps: BTreeMap<String, FileSharedCap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    baseline: Option<FileBaseline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    actuals: Option<FileActuals>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display: Option<FileDisplay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    setup_order: Option<FileSetupOrder>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    setup_folders: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    setup_sources: BTreeMap<String, BTreeMap<String, FileItemSource>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    report_presets: Vec<FileReportPreset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tag_presets: Vec<FileTagPreset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    line_presets: Vec<FileLinePreset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ot_patterns: Vec<FileOtPattern>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rate_tables: Vec<FileRateTable>,
    #[serde(default, skip_serializing_if = "is_false")]
    compact_versions: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) versions: Vec<FileVersion>,
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
            global_info,
            units,
            fringes,
            fringe_posting,
            trailing_level_fringes,
            cutoff_factor,
            fringe_row,
            arithmetic,
            round_account_totals,
            account_coding,
            new_line_defaults,
            groups,
            group_rules,
            sets,
            locations,
            topsheet,
            charges,
            credits,
            shared_caps,
            baseline,
            display,
            versions,
            compact_versions,
            report_presets,
            tag_presets,
            line_presets,
            ot_patterns,
            setup_order: _,
            setup_folders: _,
            setup_sources: _,
            provenance,
            actuals,
            rate_tables,
        } = b;
        FileBudget {
            provenance: provenance.as_ref().map(FileProvenance::from),
            actuals: (!actuals.is_empty()).then(|| FileActuals::from(&**actuals)),
            title: title.clone(),
            base_currency: base_currency.clone(),
            totals_currency: totals_currency.clone(),
            account_coding: (*account_coding != AccountCoding::default())
                .then(|| FileCoding::from(account_coding)),
            fringe_posting: (*fringe_posting != FringePosting::default())
                .then(|| FilePosting::from(fringe_posting)),
            // Keeping them is the default, also for files that don't say.
            trailing_level_fringes: (*trailing_level_fringes != TrailingFringes::default())
                .then(|| FileTrailingFringes::from(*trailing_level_fringes)),
            // 1.0 is the default, also for files that don't say.
            cutoff_factor: (*cutoff_factor != CutoffFactor::One)
                .then_some(FileCutoffFactor(*cutoff_factor)),
            fringe_row: (!fringe_row.is_default()).then(|| FileFringeRow::from(fringe_row)),
            // Movie Magic arithmetic is the default, also for files that
            // don't say.
            arithmetic: (*arithmetic != Arithmetic::default())
                .then(|| FileArithmetic::from(*arithmetic)),
            // Off is the default, also for files that don't say.
            round_account_totals: *round_account_totals,
            new_line_defaults: (*new_line_defaults != NewLineDefaults::default())
                .then(|| FileNewLineDefaults::from(new_line_defaults)),
            info: map(info, FileInfo::from),
            currencies: map(currencies, FileCurrency::from),
            units: map(units, FileUnit::from),
            globals: map(globals, |f| f.0.clone()),
            // Only for globals that exist, and only what isn't the default.
            global_info: global_info
                .iter()
                .filter(|(n, g)| globals.contains_key(*n) && !g.is_default())
                .map(|(n, g)| (n.clone(), FileGlobalInfo::from(g)))
                .collect(),
            fringes: map(fringes, FileFringe::from),
            groups: map(groups, FileGroup::from),
            // Movie Magic's rules are the default, also for files that
            // don't say.
            group_rules: (*group_rules != GroupRules::default())
                .then(|| FileGroupRules::from(*group_rules)),
            sets: map(sets, FileSet::from),
            locations: map(locations, FileLocation::from),
            topsheet: topsheet.iter().map(FileTopsheetRow::from).collect(),
            charges: charges.iter().map(FileCharge::from).collect(),
            credits: credits.iter().map(FileCredit::from).collect(),
            shared_caps: map(shared_caps, |a| FileSharedCap { amount: *a }),
            baseline: baseline.as_ref().map(FileBaseline::from),
            display: (*display != DisplayPrefs::default()).then(|| FileDisplay::from(display)),
            // Only names that exist, and nothing when a table is in byte order.
            setup_order: Some(method_core::order::normalized_all(b))
                .filter(|o| !o.is_empty())
                .map(FileSetupOrder::from),
            // Only names that exist, each once; left out when no kind has a folder.
            setup_folders: method_core::folders::normalized(b).0,
            // Only items that exist and sources that say something.
            setup_sources: method_core::pack::normalized(b)
                .0
                .iter()
                .map(|(k, m)| (k.clone(), map(m, FileItemSource::from)))
                .collect(),
            report_presets: report_presets.iter().map(FileReportPreset::from).collect(),
            tag_presets: tag_presets.iter().map(FileTagPreset::from).collect(),
            line_presets: line_presets.iter().map(FileLinePreset::from).collect(),
            ot_patterns: ot_patterns.iter().cloned().map(FileOtPattern).collect(),
            rate_tables: rate_tables.iter().cloned().map(FileRateTable).collect(),
            compact_versions: *compact_versions,
            // Full copies; `to_string` turns them into deltas when
            // `compact_versions` is on (crate::delta).
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
            // An entry for a name that isn't a global is ignored.
            global_info: f
                .global_info
                .into_iter()
                .filter(|(n, _)| f.globals.contains_key(n))
                .map(|(n, g)| (n, GlobalInfo::from(g)))
                .filter(|(_, g)| !g.is_default())
                .collect(),
            globals: unmap(f.globals, Formula),
            units: unmap(f.units, UnitDef::from),
            fringes: unmap(f.fringes, Fringe::from),
            fringe_posting: f
                .fringe_posting
                .map(FringePosting::from)
                .unwrap_or_default(),
            trailing_level_fringes: f
                .trailing_level_fringes
                .map(TrailingFringes::from)
                .unwrap_or_default(),
            cutoff_factor: f.cutoff_factor.map(|c| c.0).unwrap_or_default(),
            fringe_row: f.fringe_row.map(FringeRow::from).unwrap_or_default(),
            arithmetic: f.arithmetic.map(Arithmetic::from).unwrap_or_default(),
            round_account_totals: f.round_account_totals,
            account_coding: f
                .account_coding
                .map(AccountCoding::from)
                .unwrap_or_default(),
            new_line_defaults: f
                .new_line_defaults
                .map(NewLineDefaults::from)
                .unwrap_or_default(),
            groups: unmap(f.groups, Group::from),
            group_rules: f.group_rules.map(GroupRules::from).unwrap_or_default(),
            sets: unmap(f.sets, SetDef::from),
            locations: unmap(f.locations, Location::from),
            topsheet: f.topsheet.into_iter().map(TopsheetRow::from).collect(),
            charges: f.charges.into_iter().map(Charge::from).collect(),
            credits: f.credits.into_iter().map(Credit::from).collect(),
            shared_caps: unmap(f.shared_caps, |c| c.amount),
            baseline: f.baseline.map(Baseline::from),
            display: f.display.map(DisplayPrefs::from).unwrap_or_default(),
            setup_order: f.setup_order.map(SetupOrder::from).unwrap_or_default(),
            setup_folders: method_core::folders::SetupFolders(f.setup_folders),
            setup_sources: method_core::pack::SetupSources(
                f.setup_sources
                    .into_iter()
                    .map(|(k, m)| (k, unmap(m, ItemSource::from)))
                    .collect(),
            ),
            provenance: f.provenance.map(Provenance::from),
            actuals: std::sync::Arc::new(f.actuals.map(Actuals::from).unwrap_or_default()),
            compact_versions: f.compact_versions,
            // Rebuilt by `crate::delta::read_versions` (`from_str` takes them
            // out first); a version's own budget never has any.
            versions: Vec::new(),
            report_presets: f
                .report_presets
                .into_iter()
                .map(ReportPreset::from)
                .collect(),
            tag_presets: f.tag_presets.into_iter().map(TagPreset::from).collect(),
            line_presets: f.line_presets.into_iter().map(LinePreset::from).collect(),
            ot_patterns: f.ot_patterns.into_iter().map(|p| p.0).collect(),
            rate_tables: f.rate_tables.into_iter().map(|t| t.0).collect(),
        }
    }
}
