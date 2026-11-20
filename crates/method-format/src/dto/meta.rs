//! Provenance, setup packs, global info and display, setup order, notes and versions.

use super::*;

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
pub(super) struct FileProvenance {
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
pub(crate) struct FileItemSource {
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
pub(super) struct FileGlobalInfo {
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
pub(crate) struct FileDisplay {
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
pub(crate) struct FileSetupOrder {
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
pub(super) struct FileNote {
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

pub(super) fn file_notes(notes: &[Note]) -> Vec<FileNote> {
    notes.iter().map(FileNote::from).collect()
}

/// A row's notes as read: an older file's single `note` (a string) first,
/// as an undated note, then the `notes` list. The carried-over note gets the
/// id after the list's highest, so ids stay unique.
pub(super) fn read_notes(legacy: Option<String>, notes: Vec<FileNote>) -> Vec<Note> {
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
        if !b.rate_tables.is_empty() {
            return Err(de::Error::custom(
                "a version's budget cannot have rate tables",
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
