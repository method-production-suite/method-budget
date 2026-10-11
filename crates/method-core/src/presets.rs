//! Tag presets and line blocks, stored in the budget (decision 15: they
//! travel with it).
//!
//! - A [`TagPreset`] is a named bundle of a line's tags: fringes, groups, a
//!   set, a location and a currency. Any part may be empty. The application
//!   puts it on selected lines in one of two ways (src/presets.rs in the app):
//!   adding (fringes and groups joined with the line's own; set, location and
//!   currency filled in only where the line has none) or replacing (the
//!   line's five tags become exactly the preset's, empty parts clearing).
//! - A [`LinePreset`] is a named block of rows saved from a budget, with the
//!   definitions of the setup items those rows use, so a block whose items
//!   have since been deleted can offer to add them back when it is inserted.
//!
//! Like report presets, these are tools, not budget content: they are not
//! part of a [`crate::BudgetSnapshot`], versions don't carry them, and
//! restoring a version keeps the current ones. Nothing here ever affects a
//! total until a preset is applied or a block inserted.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Currency, Detail, Formula, Fringe, Group, Location, SetDef, UnitDef};

/// Longest tag preset or line block name, in characters.
pub const MAX_TOOL_PRESET_NAME_CHARS: usize = 80;

/// Most tag presets one budget keeps.
pub const MAX_TAG_PRESETS: usize = 200;

/// Most line blocks one budget keeps.
pub const MAX_LINE_PRESETS: usize = 100;

/// Most rows one line block holds.
pub const MAX_BLOCK_ROWS: usize = 500;

/// A named bundle of tags to put on lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TagPreset {
    pub name: String,
    /// Fringe names (from [`crate::Budget::fringes`]).
    pub fringes: BTreeSet<String>,
    /// Group names.
    pub groups: BTreeSet<String>,
    pub set: Option<String>,
    pub location: Option<String>,
    /// `None` means the base currency, as on a line.
    pub currency: Option<String>,
}

impl TagPreset {
    /// Whether it carries nothing at all (every part empty).
    pub fn is_empty(&self) -> bool {
        self.fringes.is_empty()
            && self.groups.is_empty()
            && self.set.is_none()
            && self.location.is_none()
            && self.currency.is_none()
    }
}

/// A named block of rows to insert copies of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinePreset {
    pub name: String,
    /// The rows in order, every field as saved (the fields left out when it
    /// was saved are blank). Row ids mean nothing here and are 0: inserted
    /// copies get new ids. Fringe range numbers are kept as they were, and
    /// renumbered for the account a copy lands in.
    pub rows: Vec<Detail>,
    /// The setup items the rows use, as they were defined when the block was
    /// saved (with what those items use in turn: a fringe's flat unit, the
    /// globals in a formula). Used only to add back an item the budget no
    /// longer has; a budget's own definition always wins.
    pub setup: SetupItems,
}

/// Setup items by kind and name: the budget's own tables, holding only some
/// items.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetupItems {
    pub fringes: BTreeMap<String, Fringe>,
    pub groups: BTreeMap<String, Group>,
    pub globals: BTreeMap<String, Formula>,
    pub units: BTreeMap<String, UnitDef>,
    pub sets: BTreeMap<String, SetDef>,
    pub locations: BTreeMap<String, Location>,
    pub currencies: BTreeMap<String, Currency>,
}

impl SetupItems {
    pub fn is_empty(&self) -> bool {
        let SetupItems {
            fringes,
            groups,
            globals,
            units,
            sets,
            locations,
            currencies,
        } = self;
        fringes.is_empty()
            && groups.is_empty()
            && globals.is_empty()
            && units.is_empty()
            && sets.is_empty()
            && locations.is_empty()
            && currencies.is_empty()
    }
}
