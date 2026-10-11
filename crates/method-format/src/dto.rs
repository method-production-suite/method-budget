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
    CreditRow, CreditSource, Currency, Dec, Detail, DisplayPrefs, FlatBasis, Formula, Fringe,
    FringeKind, FringePosting, GlobalInfo, Group, InfoValue, Location, NewLineDefaults, Note,
    ProductionTotal, RowId, RowKind, SetDef, TopsheetRow, TrailingFringes, UnitDef, Version,
};
use method_core::{
    Band, BandField, FontSize, ReportColumn, ReportPreset, ReportSection, ReportSpec,
};
use method_core::{LinePreset, SetupItems, TagPreset};
use method_core::{Provenance, ProvenanceKind};
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
    provenance: Option<FileProvenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_coding: Option<FileCoding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fringe_posting: Option<FilePosting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trailing_level_fringes: Option<FileTrailingFringes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    arithmetic: Option<FileArithmetic>,
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
            arithmetic,
            account_coding,
            new_line_defaults,
            groups,
            sets,
            locations,
            topsheet,
            charges,
            credits,
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
            // Movie Magic arithmetic is the default, also for files that
            // don't say.
            arithmetic: (*arithmetic != Arithmetic::default())
                .then(|| FileArithmetic::from(*arithmetic)),
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
            sets: map(sets, FileSet::from),
            locations: map(locations, FileLocation::from),
            topsheet: topsheet.iter().map(FileTopsheetRow::from).collect(),
            charges: charges.iter().map(FileCharge::from).collect(),
            credits: credits.iter().map(FileCredit::from).collect(),
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
            arithmetic: f.arithmetic.map(Arithmetic::from).unwrap_or_default(),
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
        }
    }
}

// ---------------------------------------------------------------------------
// Provenance (where a shared copy or a sub-budget came from)

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileProvenanceKind {
    SharedCopy,
    EditableCopy,
    SubBudget,
    SetupPack,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileProvenance {
    kind: FileProvenanceKind,
    source_title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_version: Option<String>,
    saved: String,
    app_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pack: Option<FilePackInfo>,
}

impl From<&Provenance> for FileProvenance {
    fn from(p: &Provenance) -> FileProvenance {
        let Provenance {
            kind,
            source_title,
            source_version,
            saved,
            app_version,
            note,
            pack,
        } = p;
        FileProvenance {
            kind: match kind {
                ProvenanceKind::SharedCopy => FileProvenanceKind::SharedCopy,
                ProvenanceKind::EditableCopy => FileProvenanceKind::EditableCopy,
                ProvenanceKind::SubBudget => FileProvenanceKind::SubBudget,
                ProvenanceKind::SetupPack => FileProvenanceKind::SetupPack,
            },
            source_title: source_title.clone(),
            source_version: source_version.clone(),
            saved: saved.clone(),
            app_version: app_version.clone(),
            note: note.clone(),
            pack: pack.as_ref().map(FilePackInfo::from),
        }
    }
}

impl From<FileProvenance> for Provenance {
    fn from(f: FileProvenance) -> Provenance {
        Provenance {
            kind: match f.kind {
                FileProvenanceKind::SharedCopy => ProvenanceKind::SharedCopy,
                FileProvenanceKind::EditableCopy => ProvenanceKind::EditableCopy,
                FileProvenanceKind::SubBudget => ProvenanceKind::SubBudget,
                FileProvenanceKind::SetupPack => ProvenanceKind::SetupPack,
            },
            source_title: f.source_title,
            source_version: f.source_version,
            saved: f.saved,
            app_version: f.app_version,
            note: f.note,
            pack: f.pack.map(PackInfo::from),
        }
    }
}

// ---------------------------------------------------------------------------
// Setup packs: a pack's details, and where setup items came from

/// A setup pack's details (`provenance.pack`); every field left out when
/// empty but the title.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilePackInfo {
    title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    effective: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    region: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    notes: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    urls: Vec<String>,
}

impl From<&PackInfo> for FilePackInfo {
    fn from(p: &PackInfo) -> FilePackInfo {
        let PackInfo {
            title,
            description,
            author,
            version,
            effective,
            region,
            notes,
            urls,
        } = p;
        FilePackInfo {
            title: title.clone(),
            description: description.clone(),
            author: author.clone(),
            version: version.clone(),
            effective: effective.clone(),
            region: region.clone(),
            notes: notes.clone(),
            urls: urls.clone(),
        }
    }
}

impl From<FilePackInfo> for PackInfo {
    fn from(f: FilePackInfo) -> PackInfo {
        PackInfo {
            title: f.title,
            description: f.description,
            author: f.author,
            version: f.version,
            effective: f.effective,
            region: f.region,
            notes: f.notes,
            urls: f.urls,
        }
    }
}

/// Where one setup item came from (`setup_sources`); every field left out
/// when empty.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileItemSource {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pack: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pack_version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    effective: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    notes: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    urls: Vec<String>,
}

impl From<&ItemSource> for FileItemSource {
    fn from(s: &ItemSource) -> FileItemSource {
        let ItemSource {
            pack,
            pack_version,
            effective,
            notes,
            urls,
        } = s;
        FileItemSource {
            pack: pack.clone(),
            pack_version: pack_version.clone(),
            effective: effective.clone(),
            notes: notes.clone(),
            urls: urls.clone(),
        }
    }
}

impl From<FileItemSource> for ItemSource {
    fn from(f: FileItemSource) -> ItemSource {
        ItemSource {
            pack: f.pack,
            pack_version: f.pack_version,
            effective: f.effective,
            notes: f.notes,
            urls: f.urls,
        }
    }
}

// ---------------------------------------------------------------------------
// Global info and display settings (display only; never change a value)

/// Decimal places for display: an integer from 0 to
/// [`method_core::MAX_DISPLAY_DECIMALS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Places(u8);

impl Serialize for Places {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for Places {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Places, D::Error> {
        let n = u8::deserialize(d)?;
        if n > method_core::MAX_DISPLAY_DECIMALS {
            return Err(de::Error::custom(format!(
                "decimal places must be 0 to {}, not {n}",
                method_core::MAX_DISPLAY_DECIMALS
            )));
        }
        Ok(Places(n))
    }
}

/// What a global says about itself besides its formula. Each field is left
/// out at its default; a global with nothing but defaults has no entry.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileGlobalInfo {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    decimals: Option<Places>,
    #[serde(default, skip_serializing_if = "is_false")]
    show_by_name: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    folder: String,
}

impl From<&GlobalInfo> for FileGlobalInfo {
    fn from(g: &GlobalInfo) -> FileGlobalInfo {
        let GlobalInfo {
            description,
            unit,
            decimals,
            show_by_name,
            folder,
        } = g;
        FileGlobalInfo {
            description: description.clone(),
            unit: unit.clone(),
            decimals: decimals.map(Places),
            show_by_name: *show_by_name,
            folder: folder.clone(),
        }
    }
}

impl From<FileGlobalInfo> for GlobalInfo {
    fn from(f: FileGlobalInfo) -> GlobalInfo {
        GlobalInfo {
            description: f.description,
            unit: f.unit,
            decimals: f.decimals.map(|p| p.0),
            show_by_name: f.show_by_name,
            folder: f.folder,
        }
    }
}

/// How numbers show on screen. Written only when some setting differs from
/// the default; each field is left out at its default.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDisplay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rate_cutoff: Option<DecStr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rate_decimals_below: Option<Places>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rate_decimals_above: Option<Places>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    rate_global_decimals: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    summary_rows: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    net_total: bool,
}

impl From<&DisplayPrefs> for FileDisplay {
    fn from(d: &DisplayPrefs) -> FileDisplay {
        let DisplayPrefs {
            rate_cutoff,
            rate_decimals_below,
            rate_decimals_above,
            rate_global_decimals,
            summary_rows,
            net_total,
        } = d;
        FileDisplay {
            rate_cutoff: rate_cutoff.map(DecStr),
            rate_decimals_below: rate_decimals_below.map(Places),
            rate_decimals_above: rate_decimals_above.map(Places),
            rate_global_decimals: *rate_global_decimals,
            summary_rows: *summary_rows,
            net_total: *net_total,
        }
    }
}

impl From<FileDisplay> for DisplayPrefs {
    fn from(f: FileDisplay) -> DisplayPrefs {
        DisplayPrefs {
            rate_cutoff: f.rate_cutoff.map(|d| d.0),
            rate_decimals_below: f.rate_decimals_below.map(|p| p.0),
            rate_decimals_above: f.rate_decimals_above.map(|p| p.0),
            rate_global_decimals: f.rate_global_decimals,
            summary_rows: f.summary_rows,
            net_total: f.net_total,
        }
    }
}

// ---------------------------------------------------------------------------
// Setup order (display only)

/// Hand-made order of the setup tables, first to last. Each list is left
/// out when its table is in byte order; the object when all are.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileSetupOrder {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    units: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    globals: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    fringes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    locations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    currencies: Vec<String>,
}

impl From<SetupOrder> for FileSetupOrder {
    fn from(o: SetupOrder) -> FileSetupOrder {
        let SetupOrder {
            units,
            globals,
            fringes,
            groups,
            sets,
            locations,
            currencies,
        } = o;
        FileSetupOrder {
            units,
            globals,
            fringes,
            groups,
            sets,
            locations,
            currencies,
        }
    }
}

impl From<FileSetupOrder> for SetupOrder {
    fn from(f: FileSetupOrder) -> SetupOrder {
        SetupOrder {
            units: f.units,
            globals: f.globals,
            fringes: f.fringes,
            groups: f.groups,
            sets: f.sets,
            locations: f.locations,
            currencies: f.currencies,
        }
    }
}

// ---------------------------------------------------------------------------
// Notes

/// A dated note on a row, an account or a category.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileNote {
    id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    created: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    edited: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    done: bool,
    text: String,
}

impl From<&Note> for FileNote {
    fn from(n: &Note) -> FileNote {
        let Note {
            id,
            text,
            created,
            edited,
            author,
            done,
        } = n;
        FileNote {
            id: *id,
            created: created.clone(),
            edited: edited.clone(),
            author: author.clone(),
            done: *done,
            text: text.clone(),
        }
    }
}

impl From<FileNote> for Note {
    fn from(f: FileNote) -> Note {
        Note {
            id: f.id,
            text: f.text,
            created: f.created,
            edited: f.edited,
            author: f.author,
            done: f.done,
        }
    }
}

fn file_notes(notes: &[Note]) -> Vec<FileNote> {
    notes.iter().map(FileNote::from).collect()
}

/// A row's notes as read: an older file's single `note` (a string) first,
/// as an undated note, then the `notes` list. The carried-over note gets the
/// id after the list's highest, so ids stay unique.
fn read_notes(legacy: Option<String>, notes: Vec<FileNote>) -> Vec<Note> {
    let notes: Vec<Note> = notes.into_iter().map(Note::from).collect();
    match legacy {
        None => notes,
        Some(text) => {
            let mut out = vec![Note::plain(method_core::next_note_id(&notes), text)];
            out.extend(notes);
            out
        }
    }
}

// ---------------------------------------------------------------------------
// Versions

/// A named copy of the budget's content: a full `budget`, or a `delta`
/// against another budget object (crate::delta). Exactly one of the two.
#[derive(Serialize, Deserialize)]
#[serde(try_from = "RawVersion")]
pub(crate) struct FileVersion {
    pub id: u64,
    pub name: String,
    pub created: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<FileSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<crate::delta::FileDelta>,
}

/// A version as read, before checking it has a budget or a delta.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVersion {
    id: u64,
    name: String,
    created: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    budget: Option<FileSnapshot>,
    #[serde(default)]
    delta: Option<crate::delta::FileDelta>,
}

impl TryFrom<RawVersion> for FileVersion {
    type Error = &'static str;
    fn try_from(r: RawVersion) -> Result<FileVersion, &'static str> {
        match (&r.budget, &r.delta) {
            (Some(_), Some(_)) => Err("a version has either a budget or a delta, not both"),
            (None, None) => Err("missing field `budget` (or `delta`)"),
            _ => Ok(FileVersion {
                id: r.id,
                name: r.name,
                created: r.created,
                note: r.note,
                budget: r.budget,
                delta: r.delta,
            }),
        }
    }
}

/// A version's budget: the same object as the top-level budget, except that
/// it may not have `versions` of its own.
pub(crate) struct FileSnapshot(pub FileBudget);

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
        if b.compact_versions {
            return Err(de::Error::custom(
                "a version's budget cannot have compact_versions",
            ));
        }
        if !b.report_presets.is_empty() {
            return Err(de::Error::custom(
                "a version's budget cannot have report presets",
            ));
        }
        if !b.tag_presets.is_empty() || !b.line_presets.is_empty() {
            return Err(de::Error::custom(
                "a version's budget cannot have tag presets or line blocks",
            ));
        }
        if !b.ot_patterns.is_empty() {
            return Err(de::Error::custom(
                "a version's budget cannot have overtime patterns",
            ));
        }
        if b.setup_order.is_some() {
            return Err(de::Error::custom(
                "a version's budget cannot have a setup order",
            ));
        }
        if !b.setup_folders.is_empty() {
            return Err(de::Error::custom(
                "a version's budget cannot have setup folders",
            ));
        }
        if !b.setup_sources.is_empty() {
            return Err(de::Error::custom(
                "a version's budget cannot have setup sources",
            ));
        }
        if b.provenance.is_some() {
            return Err(de::Error::custom(
                "a version's budget cannot have a provenance",
            ));
        }
        if b.display.is_some() {
            return Err(de::Error::custom(
                "a version's budget cannot have display settings",
            ));
        }
        if b.actuals.is_some() {
            return Err(de::Error::custom("a version's budget cannot have actuals"));
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
            budget: Some(FileSnapshot(FileBudget::from(&budget.to_budget()))),
            delta: None,
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

/// Under production-level posting, what happens to the fringes of
/// categories after the last production total: `"keep"` (the default, left
/// out) or `"drop"`.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileTrailingFringes {
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

/// How a budget works out its totals: `"mmb"` (the default, left out) or
/// `"exact"`.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileArithmetic {
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
enum FileBasis {
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
struct FileGroup {
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
    notes: Vec<FileNote>,
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
    notes: Option<Vec<FileNote>>,
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
                notes: r.notes.unwrap_or_default(),
                accounts: r.accounts.unwrap_or_default(),
            }),
            TopsheetKind::ProductionTotal => {
                if r.code.is_some() {
                    return Err("a production_total row has no `code`".into());
                }
                if r.accounts.is_some() {
                    return Err("a production_total row has no `accounts`".into());
                }
                if r.notes.is_some() {
                    return Err("a production_total row has no `notes`".into());
                }
                Ok(FileTopsheetRow {
                    kind: r.kind,
                    code: None,
                    name: r.name,
                    notes: Vec::new(),
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
                notes,
            }) => FileTopsheetRow {
                kind: TopsheetKind::Category,
                code: Some(code.clone()),
                name: name.clone(),
                notes: file_notes(notes),
                accounts: accounts.iter().map(FileAccount::from).collect(),
            },
            TopsheetRow::ProductionTotal(ProductionTotal { name }) => FileTopsheetRow {
                kind: TopsheetKind::ProductionTotal,
                code: None,
                name: name.clone(),
                notes: Vec::new(),
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
                notes: f.notes.into_iter().map(Note::from).collect(),
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
    notes: Vec<FileNote>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    details: Vec<FileDetail>,
}

impl From<&Account> for FileAccount {
    fn from(a: &Account) -> FileAccount {
        let Account {
            code,
            name,
            details,
            notes,
        } = a;
        FileAccount {
            code: code.clone(),
            name: name.clone(),
            notes: file_notes(notes),
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
            notes: f.notes.into_iter().map(Note::from).collect(),
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
    /// Read only: older files kept one undated note per row, as a string.
    /// It reads as the first of the row's `notes`; never written.
    #[serde(default, skip_serializing)]
    note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    notes: Vec<FileNote>,
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
            notes,
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
            note: None,
            notes: file_notes(notes),
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
            notes: read_notes(f.note, f.notes),
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
    label_percent: bool,
    label_excluded: bool,
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
        #[serde(default, skip_serializing_if = "is_false")]
        label_percent: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        label_excluded: bool,
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
        #[serde(default, skip_serializing_if = "is_false")]
        label_percent: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        label_excluded: bool,
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
            label_percent,
            label_excluded,
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
            label_percent: *label_percent,
            label_excluded: *label_excluded,
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
                label_percent: c.label_percent,
                label_excluded: c.label_excluded,
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
                label_percent: c.label_percent,
                label_excluded: c.label_excluded,
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
                label_percent,
                label_excluded,
            } => (
                ChargeCommon {
                    name,
                    account,
                    applied,
                    excluded_amounts,
                    excluded_groups,
                    excluded_categories,
                    excluded_accounts,
                    label_percent,
                    label_excluded,
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
                label_percent,
                label_excluded,
            } => (
                ChargeCommon {
                    name,
                    account,
                    applied,
                    excluded_amounts,
                    excluded_groups,
                    excluded_categories,
                    excluded_accounts,
                    label_percent,
                    label_excluded,
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
            label_percent: c.label_percent,
            label_excluded: c.label_excluded,
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
// Actuals (transactions from accounting, estimates to complete;
// method_core::actuals)

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileActuals {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    batches: Vec<FileBatch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    transactions: Vec<FileTransaction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    etc: BTreeMap<String, FileEtc>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileBatch {
    id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    imported: String,
    rows: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileTransaction {
    id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    account: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line: Option<RowId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    tags: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    vendor: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    reference: String,
    amount: Cents,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    batch: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FileEtc {
    BudgetMinusActual {},
    Manual { amount: Cents },
    PercentComplete { percent: DecStr },
}

impl From<&Actuals> for FileActuals {
    fn from(a: &Actuals) -> FileActuals {
        let Actuals {
            transactions,
            batches,
            etc,
        } = a;
        FileActuals {
            batches: batches
                .iter()
                .map(|b| {
                    let ImportBatch {
                        id,
                        file,
                        imported,
                        rows,
                    } = b;
                    FileBatch {
                        id: *id,
                        file: file.clone(),
                        imported: imported.clone(),
                        rows: *rows,
                    }
                })
                .collect(),
            transactions: transactions
                .iter()
                .map(|t| {
                    let Transaction {
                        id,
                        date,
                        account,
                        line,
                        tags,
                        vendor,
                        description,
                        reference,
                        amount,
                        batch,
                    } = &**t;
                    FileTransaction {
                        id: *id,
                        date: date.clone(),
                        account: account.clone(),
                        line: *line,
                        tags: tags.clone(),
                        vendor: vendor.clone(),
                        description: description.clone(),
                        reference: reference.clone(),
                        amount: *amount,
                        batch: *batch,
                    }
                })
                .collect(),
            etc: map(etc, |m| match m {
                EtcMethod::BudgetMinusActual => FileEtc::BudgetMinusActual {},
                EtcMethod::Manual(amount) => FileEtc::Manual { amount: *amount },
                EtcMethod::PercentComplete(p) => FileEtc::PercentComplete {
                    percent: DecStr(*p),
                },
            }),
        }
    }
}

impl From<FileActuals> for Actuals {
    fn from(f: FileActuals) -> Actuals {
        Actuals {
            batches: f
                .batches
                .into_iter()
                .map(|b| ImportBatch {
                    id: b.id,
                    file: b.file,
                    imported: b.imported,
                    rows: b.rows,
                })
                .collect(),
            transactions: f
                .transactions
                .into_iter()
                .map(|t| {
                    std::sync::Arc::new(Transaction {
                        id: t.id,
                        date: t.date,
                        account: t.account,
                        line: t.line,
                        tags: t.tags,
                        vendor: t.vendor,
                        description: t.description,
                        reference: t.reference,
                        amount: t.amount,
                        batch: t.batch,
                    })
                })
                .collect(),
            etc: unmap(f.etc, |m| match m {
                FileEtc::BudgetMinusActual {} => EtcMethod::BudgetMinusActual,
                FileEtc::Manual { amount } => EtcMethod::Manual(amount),
                FileEtc::PercentComplete { percent } => EtcMethod::PercentComplete(percent.0),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Report presets (saved report setups; method_core::report_preset)

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
    #[serde(default, skip_serializing_if = "is_false")]
    skip_done_notes: bool,
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
    #[serde(default, skip_serializing_if = "is_false")]
    vertical_rules: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    collapse_details: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    topsheet_fringes: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    topsheet_pages: bool,
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
    Chart,
    Units,
    Sets,
    Locations,
    Currencies,
    CostReport,
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
    #[serde(default, skip_serializing_if = "is_false")]
    first_page_only: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FileBandField {
    Title {},
    Section {},
    Printed {},
    Date {},
    Time {},
    File {},
    Page {},
    Info { label: String },
    Global { name: String },
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
    Topsheet, Accounts, Detail, Fringes, Globals, Groups, Notes, Info, Chart, Units, Sets,
    Locations, Currencies, CostReport,
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
            BandField::Date => FileBandField::Date {},
            BandField::Time => FileBandField::Time {},
            BandField::File => FileBandField::File {},
            BandField::Page => FileBandField::Page {},
            BandField::Info(label) => FileBandField::Info {
                label: label.clone(),
            },
            BandField::Global(name) => FileBandField::Global { name: name.clone() },
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
            FileBandField::Date {} => BandField::Date,
            FileBandField::Time {} => BandField::Time,
            FileBandField::File {} => BandField::File,
            FileBandField::Page {} => BandField::Page,
            FileBandField::Info { label } => BandField::Info(label),
            FileBandField::Global { name } => BandField::Global(name),
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
            first_page_only,
        } = b;
        FileBand {
            left: left.as_ref().map(FileBandField::from),
            center: center.as_ref().map(FileBandField::from),
            right: right.as_ref().map(FileBandField::from),
            first_page_only: *first_page_only,
        }
    }
}

impl From<FileBand> for Band {
    fn from(f: FileBand) -> Band {
        Band {
            left: f.left.map(BandField::from),
            center: f.center.map(BandField::from),
            right: f.right.map(BandField::from),
            first_page_only: f.first_page_only,
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
            skip_done_notes,
            only_groups,
            columns,
            landscape,
            font_size,
            shading,
            group_colors,
            vertical_rules,
            collapse_details,
            topsheet_fringes,
            topsheet_pages,
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
            skip_done_notes: *skip_done_notes,
            only_groups: only_groups.clone(),
            columns: columns
                .as_ref()
                .map(|c| c.iter().map(|&x| x.into()).collect()),
            landscape: *landscape,
            font_size: (*font_size).into(),
            shading: *shading,
            group_colors: *group_colors,
            vertical_rules: *vertical_rules,
            collapse_details: *collapse_details,
            topsheet_fringes: *topsheet_fringes,
            topsheet_pages: *topsheet_pages,
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
                skip_done_notes: f.skip_done_notes,
                only_groups: f.only_groups,
                columns: f.columns.map(|c| c.into_iter().map(Into::into).collect()),
                landscape: f.landscape,
                font_size: f.font_size.into(),
                shading: f.shading,
                group_colors: f.group_colors,
                vertical_rules: f.vertical_rules,
                collapse_details: f.collapse_details,
                topsheet_fringes: f.topsheet_fringes,
                topsheet_pages: f.topsheet_pages,
                header: f.header.map(Band::from),
                footer: f.footer.map(Band::from),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Tag presets and line blocks (method_core::presets)

/// A saved bundle of line tags. Every part is left out when empty.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileTagPreset {
    name: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    fringes: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    groups: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    set: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    currency: Option<String>,
}

impl From<&TagPreset> for FileTagPreset {
    fn from(p: &TagPreset) -> FileTagPreset {
        let TagPreset {
            name,
            fringes,
            groups,
            set,
            location,
            currency,
        } = p;
        FileTagPreset {
            name: name.clone(),
            fringes: fringes.clone(),
            groups: groups.clone(),
            set: set.clone(),
            location: location.clone(),
            currency: currency.clone(),
        }
    }
}

impl From<FileTagPreset> for TagPreset {
    fn from(f: FileTagPreset) -> TagPreset {
        TagPreset {
            name: f.name,
            fringes: f.fringes,
            groups: f.groups,
            set: f.set,
            location: f.location,
            currency: f.currency,
        }
    }
}

/// A saved block of rows, with the setup items they use.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileLinePreset {
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rows: Vec<FileBlockRow>,
    #[serde(default, skip_serializing_if = "FileSetupItems::is_empty")]
    setup: FileSetupItems,
}

impl From<&LinePreset> for FileLinePreset {
    fn from(p: &LinePreset) -> FileLinePreset {
        let LinePreset { name, rows, setup } = p;
        FileLinePreset {
            name: name.clone(),
            rows: rows.iter().map(FileBlockRow::from).collect(),
            setup: FileSetupItems::from(setup),
        }
    }
}

impl From<FileLinePreset> for LinePreset {
    fn from(f: FileLinePreset) -> LinePreset {
        LinePreset {
            name: f.name,
            rows: f.rows.into_iter().map(Detail::from).collect(),
            setup: SetupItems::from(f.setup),
        }
    }
}

/// Setup items in the shapes the budget's own tables use. Each table is
/// left out when empty.
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileSetupItems {
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
}

impl FileSetupItems {
    fn is_empty(&self) -> bool {
        self.currencies.is_empty()
            && self.units.is_empty()
            && self.globals.is_empty()
            && self.fringes.is_empty()
            && self.groups.is_empty()
            && self.sets.is_empty()
            && self.locations.is_empty()
    }
}

impl From<&SetupItems> for FileSetupItems {
    fn from(s: &SetupItems) -> FileSetupItems {
        let SetupItems {
            fringes,
            groups,
            globals,
            units,
            sets,
            locations,
            currencies,
        } = s;
        FileSetupItems {
            currencies: map(currencies, FileCurrency::from),
            units: map(units, FileUnit::from),
            globals: map(globals, |f| f.0.clone()),
            fringes: map(fringes, FileFringe::from),
            groups: map(groups, FileGroup::from),
            sets: map(sets, FileSet::from),
            locations: map(locations, FileLocation::from),
        }
    }
}

impl From<FileSetupItems> for SetupItems {
    fn from(f: FileSetupItems) -> SetupItems {
        SetupItems {
            fringes: unmap(f.fringes, Fringe::from),
            groups: unmap(f.groups, Group::from),
            globals: unmap(f.globals, Formula),
            units: unmap(f.units, UnitDef::from),
            sets: unmap(f.sets, SetDef::from),
            locations: unmap(f.locations, Location::from),
            currencies: unmap(f.currencies, Currency::from),
        }
    }
}

/// A row in a line block: a [detail row](FileDetail) without an `id` (the
/// copies inserted get new ones). Converted through [`FileDetail`], so the
/// two always agree field for field.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileBlockRow {
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
    /// Read only, as on a [`FileDetail`].
    #[serde(default, skip_serializing)]
    note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    notes: Vec<FileNote>,
}

impl From<&Detail> for FileBlockRow {
    fn from(d: &Detail) -> FileBlockRow {
        let FileDetail {
            id: _,
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
            fringe_range,
            fringes,
            groups,
            set,
            location,
            bold,
            italic,
            indent,
            color,
            note: _,
            notes,
        } = FileDetail::from(d);
        FileBlockRow {
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
            fringe_range,
            fringes,
            groups,
            set,
            location,
            bold,
            italic,
            indent,
            color,
            note: None,
            notes,
        }
    }
}

impl From<FileBlockRow> for Detail {
    /// The row with id 0: a block's rows have none.
    fn from(r: FileBlockRow) -> Detail {
        let FileBlockRow {
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
            fringe_range,
            fringes,
            groups,
            set,
            location,
            bold,
            italic,
            indent,
            color,
            note,
            notes,
        } = r;
        Detail::from(FileDetail {
            id: 0,
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
            fringe_range,
            fringes,
            groups,
            set,
            location,
            bold,
            italic,
            indent,
            color,
            note,
            notes,
        })
    }
}

// ---------------------------------------------------------------------------
// Overtime patterns (method_core::overtime)

/// One overtime band: its multiplier, and how many hours it covers (left
/// out on the last band, which covers the rest).
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileOtBand {
    multiplier: DecStr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hours: Option<DecStr>,
}

/// A saved overtime pattern as written.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileOtPatternRaw {
    name: String,
    straight_hours: DecStr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bands: Vec<FileOtBand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sixth_day: Option<DecStr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seventh_day: Option<DecStr>,
}

/// A saved overtime pattern, checked when read (the same rules as saving
/// one in the app).
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "FileOtPatternRaw", into = "FileOtPatternRaw")]
pub(crate) struct FileOtPattern(pub(crate) method_core::overtime::OtPattern);

impl From<FileOtPattern> for FileOtPatternRaw {
    fn from(p: FileOtPattern) -> FileOtPatternRaw {
        let method_core::overtime::OtPattern {
            name,
            straight_hours,
            bands,
            sixth_day,
            seventh_day,
        } = p.0;
        FileOtPatternRaw {
            name,
            straight_hours: DecStr(straight_hours),
            bands: bands
                .into_iter()
                .map(|b| FileOtBand {
                    multiplier: DecStr(b.multiplier),
                    hours: b.hours.map(DecStr),
                })
                .collect(),
            sixth_day: sixth_day.map(DecStr),
            seventh_day: seventh_day.map(DecStr),
        }
    }
}

impl TryFrom<FileOtPatternRaw> for FileOtPattern {
    type Error = String;

    fn try_from(f: FileOtPatternRaw) -> Result<FileOtPattern, String> {
        let p = method_core::overtime::OtPattern {
            name: f.name,
            straight_hours: f.straight_hours.0,
            bands: f
                .bands
                .into_iter()
                .map(|b| method_core::overtime::OtBand {
                    multiplier: b.multiplier.0,
                    hours: b.hours.map(|h| h.0),
                })
                .collect(),
            sixth_day: f.sixth_day.map(|d| d.0),
            seventh_day: f.seventh_day.map(|d| d.0),
        };
        p.validate_all()
            .map_err(|e| format!("overtime pattern {:?}: {e}", p.name))?;
        Ok(FileOtPattern(p))
    }
}
