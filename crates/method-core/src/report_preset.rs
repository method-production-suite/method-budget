//! Saved report setups ("report presets"), stored in the budget.
//!
//! A preset is a name and a [`ReportSpec`]: which sections print and in what
//! order, which categories and accounts, what to leave out, the detail
//! columns, the page layout, and the header and footer. It holds no figures:
//! a report is always drawn from the budget as it is now, so a preset never
//! goes stale. Presets are layout, not budget content, so they are not part
//! of a [`crate::BudgetSnapshot`]: versions don't carry them, and restoring a
//! version keeps the current ones.
//!
//! Display data only: nothing here ever affects a total.

use std::collections::BTreeSet;

/// Longest preset name, in characters.
pub const MAX_PRESET_NAME_CHARS: usize = 80;

/// Most presets one budget keeps.
pub const MAX_PRESETS: usize = 100;

/// Longest custom text in a header or footer slot, in characters.
pub const MAX_BAND_TEXT_CHARS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportPreset {
    pub name: String,
    /// The preset a new report starts from. At most one preset in a budget
    /// is the default (the application keeps it that way).
    pub default: bool,
    pub spec: ReportSpec,
}

/// What a report contains and how it is laid out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportSpec {
    /// Sections in print order. Each appears at most once.
    pub sections: Vec<ReportSection>,
    /// Categories (by code) left out of the account and detail sections.
    /// The topsheet always lists every category, so its totals add up.
    pub excluded_categories: BTreeSet<String>,
    /// Accounts (by code) left out of the account and detail sections.
    pub excluded_accounts: BTreeSet<String>,
    /// Leave out accounts with no rows.
    pub skip_empty_accounts: bool,
    /// Leave out accounts whose total is zero.
    pub skip_zero_accounts: bool,
    /// Leave out detail lines that don't count (a group on them is out of
    /// the budget total).
    pub skip_suppressed_lines: bool,
    /// Leave out the notes printed under detail lines (the notes section
    /// still lists them when it is included).
    pub skip_notes: bool,
    /// Leave out notes ticked off as reviewed, under lines and in the
    /// notes section.
    pub skip_done_notes: bool,
    /// When not empty, detail pages print only the lines tagged with at
    /// least one of these groups.
    pub only_groups: BTreeSet<String>,
    /// Optional detail columns in print order (`#`, description and total
    /// always print). `None` means the standard set.
    pub columns: Option<Vec<ReportColumn>>,
    pub landscape: bool,
    pub font_size: FontSize,
    /// Alternate rows tinted, total rows darker.
    pub shading: bool,
    /// Lines print in their own or their groups' colors (otherwise black).
    pub group_colors: bool,
    /// Thin rules between the columns of every table.
    pub vertical_rules: bool,
    /// Detail pages print each account as one line (its code, name and
    /// totals) instead of its rows.
    pub collapse_details: bool,
    /// The topsheet gets a column with each category's fringes.
    pub topsheet_fringes: bool,
    /// The topsheet gets a column with the page each category's detail
    /// starts on.
    pub topsheet_pages: bool,
    /// `None` means the standard header (title, section, date printed).
    pub header: Option<Band>,
    /// `None` means the standard footer (page number at the right).
    pub footer: Option<Band>,
}

impl Default for ReportSpec {
    /// The topsheet and the detail, everything included, standard columns,
    /// portrait, medium type, no shading, black and white.
    fn default() -> ReportSpec {
        ReportSpec {
            sections: vec![ReportSection::Topsheet, ReportSection::Detail],
            excluded_categories: BTreeSet::new(),
            excluded_accounts: BTreeSet::new(),
            skip_empty_accounts: false,
            skip_zero_accounts: false,
            skip_suppressed_lines: false,
            skip_notes: false,
            skip_done_notes: false,
            only_groups: BTreeSet::new(),
            columns: None,
            landscape: false,
            font_size: FontSize::Medium,
            shading: false,
            group_colors: false,
            vertical_rules: false,
            collapse_details: false,
            topsheet_fringes: false,
            topsheet_pages: false,
            header: None,
            footer: None,
        }
    }
}

macro_rules! names {
    ($ty:ident { $($variant:ident = $name:literal),+ $(,)? }) => {
        impl $ty {
            /// Every value, in a stable order.
            pub const ALL: &'static [$ty] = &[$($ty::$variant),+];

            /// The name the file format and the UI use.
            pub fn name(self) -> &'static str {
                match self {
                    $($ty::$variant => $name),+
                }
            }

            /// The value with this name.
            pub fn from_name(s: &str) -> Option<$ty> {
                match s {
                    $($name => Some($ty::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReportSection {
    Topsheet,
    /// Each category's accounts with their totals.
    Accounts,
    /// Every account's lines.
    Detail,
    /// The fringes table with what each one adds up to.
    Fringes,
    Globals,
    /// The groups with their totals.
    Groups,
    /// Every note in the budget, by account and line.
    Notes,
    /// Budget information: title, currencies, info fields.
    Info,
    /// Every category and account code with its name, no figures.
    Chart,
    /// The units table.
    Units,
    /// The sets with their totals.
    Sets,
    /// The locations with their totals.
    Locations,
    /// The currencies with their rates.
    Currencies,
    /// Budget, baseline, actual to date, estimate to complete, estimate at
    /// completion, variance and percent spent per account, category and
    /// the topsheet (see [`crate::actuals`]).
    CostReport,
}

names!(ReportSection {
    Topsheet = "topsheet",
    Accounts = "accounts",
    Detail = "detail",
    Fringes = "fringes",
    Globals = "globals",
    Groups = "groups",
    Notes = "notes",
    Info = "info",
    Chart = "chart",
    Units = "units",
    Sets = "sets",
    Locations = "locations",
    Currencies = "currencies",
    CostReport = "cost_report",
});

/// An optional column of the detail pages (the grid's columns, plus the
/// baseline's original and variance).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReportColumn {
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
    FringePercent,
    FringeTotal,
    Groups,
    Set,
    Location,
    Original,
    Variance,
}

names!(ReportColumn {
    Amount = "amount",
    Unit = "unit",
    X = "x",
    Unit2 = "unit2",
    Rate = "rate",
    Unit3 = "unit3",
    Currency = "currency",
    X4 = "x4",
    Unit4 = "unit4",
    Fringes = "fringes",
    FringePercent = "fringe_pct",
    FringeTotal = "fringe_total",
    Groups = "groups",
    Set = "set",
    Location = "location",
    Original = "original",
    Variance = "variance",
});

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontSize {
    Small,
    #[default]
    Medium,
    Large,
}

names!(FontSize {
    Small = "small",
    Medium = "medium",
    Large = "large",
});

/// A header or footer: three slots across the page, each empty or one field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Band {
    pub left: Option<BandField>,
    pub center: Option<BandField>,
    pub right: Option<BandField>,
    /// Printed on the first page only; later pages have none.
    pub first_page_only: bool,
}

/// What a header or footer slot shows, worked out when the report is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BandField {
    /// The budget's title.
    Title,
    /// The section on the page (topsheet, a category's detail, ...).
    Section,
    /// The date and time the report was drawn.
    Printed,
    /// The date the report was drawn.
    Date,
    /// The time of day the report was drawn.
    Time,
    /// The budget's file name.
    File,
    /// "Page 3 of 12".
    Page,
    /// A budget information field, by label (blank when there is none).
    Info(String),
    /// A global's value, by name (blank when there is none).
    Global(String),
    /// Text as typed.
    Text(String),
}

/// Why a preset can't be saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresetError {
    BlankName,
    NameTooLong,
    TextTooLong,
    RepeatedSection(ReportSection),
    RepeatedColumn(ReportColumn),
}

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PresetError::BlankName => f.write_str("a report preset needs a name"),
            PresetError::NameTooLong => write!(
                f,
                "a report preset's name can be at most {MAX_PRESET_NAME_CHARS} characters"
            ),
            PresetError::TextTooLong => write!(
                f,
                "header and footer text can be at most {MAX_BAND_TEXT_CHARS} characters"
            ),
            PresetError::RepeatedSection(s) => {
                write!(f, "the section \"{}\" is listed twice", s.name())
            }
            PresetError::RepeatedColumn(c) => {
                write!(f, "the column \"{}\" is listed twice", c.name())
            }
        }
    }
}

impl std::error::Error for PresetError {}

impl ReportPreset {
    /// Checks what the application requires of a preset it saves: a name of
    /// 1 to [`MAX_PRESET_NAME_CHARS`] characters (not just spaces), sections
    /// and columns each listed once, header and footer text within
    /// [`MAX_BAND_TEXT_CHARS`]. The file reader is more lenient and keeps
    /// whatever a file holds.
    pub fn validate(&self) -> Result<(), PresetError> {
        if self.name.trim().is_empty() {
            return Err(PresetError::BlankName);
        }
        if self.name.chars().count() > MAX_PRESET_NAME_CHARS {
            return Err(PresetError::NameTooLong);
        }
        let s = &self.spec;
        let mut seen = BTreeSet::new();
        for &sec in &s.sections {
            if !seen.insert(sec) {
                return Err(PresetError::RepeatedSection(sec));
            }
        }
        let mut seen = BTreeSet::new();
        for &col in s.columns.iter().flatten() {
            if !seen.insert(col) {
                return Err(PresetError::RepeatedColumn(col));
            }
        }
        for band in [&s.header, &s.footer].into_iter().flatten() {
            for field in [&band.left, &band.center, &band.right]
                .into_iter()
                .flatten()
            {
                if let BandField::Text(t) | BandField::Info(t) | BandField::Global(t) = field {
                    if t.chars().count() > MAX_BAND_TEXT_CHARS {
                        return Err(PresetError::TextTooLong);
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for &s in ReportSection::ALL {
            assert_eq!(ReportSection::from_name(s.name()), Some(s));
        }
        for &c in ReportColumn::ALL {
            assert_eq!(ReportColumn::from_name(c.name()), Some(c));
        }
        for &f in FontSize::ALL {
            assert_eq!(FontSize::from_name(f.name()), Some(f));
        }
        assert_eq!(ReportSection::from_name("Topsheet"), None);
    }

    #[test]
    fn validate() {
        let ok = ReportPreset {
            name: "Studio".into(),
            default: false,
            spec: ReportSpec::default(),
        };
        assert_eq!(ok.validate(), Ok(()));
        let mut p = ok.clone();
        p.name = "  ".into();
        assert_eq!(p.validate(), Err(PresetError::BlankName));
        p.name = "x".repeat(MAX_PRESET_NAME_CHARS + 1);
        assert_eq!(p.validate(), Err(PresetError::NameTooLong));
        let mut p = ok.clone();
        p.spec.sections = vec![ReportSection::Detail, ReportSection::Detail];
        assert_eq!(
            p.validate(),
            Err(PresetError::RepeatedSection(ReportSection::Detail))
        );
        let mut p = ok.clone();
        p.spec.columns = Some(vec![ReportColumn::Rate, ReportColumn::Rate]);
        assert_eq!(
            p.validate(),
            Err(PresetError::RepeatedColumn(ReportColumn::Rate))
        );
        let mut p = ok;
        p.spec.footer = Some(Band {
            left: Some(BandField::Text("y".repeat(MAX_BAND_TEXT_CHARS + 1))),
            ..Band::default()
        });
        assert_eq!(p.validate(), Err(PresetError::TextTooLong));
        let mut p = ok_global();
        assert_eq!(p.validate(), Ok(()));
        p.spec.header.as_mut().unwrap().center =
            Some(BandField::Global("g".repeat(MAX_BAND_TEXT_CHARS + 1)));
        assert_eq!(p.validate(), Err(PresetError::TextTooLong));
    }

    fn ok_global() -> ReportPreset {
        let mut spec = ReportSpec {
            sections: ReportSection::ALL.to_vec(),
            ..ReportSpec::default()
        };
        spec.header = Some(Band {
            center: Some(BandField::Global("WEEKS".into())),
            first_page_only: true,
            ..Band::default()
        });
        ReportPreset {
            name: "Everything".into(),
            default: false,
            spec,
        }
    }
}
