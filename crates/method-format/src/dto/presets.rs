//! Tag presets and line blocks.

use super::*;

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
pub(crate) struct FileLinePreset {
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
