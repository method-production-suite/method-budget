//! Report presets.

use super::*;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bid: Option<FileBid>,
}

/// A bid layout (method_core::bid; docs/FORMAT.md, "Bid layout").
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileBid {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    fields: Vec<FileBidField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lines: Vec<FileBidLine>,
    #[serde(default, skip_serializing_if = "is_false")]
    show_accounts: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileBidField {
    label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    value: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FileBidLine {
    Section {
        name: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        picks: Vec<FileBidPick>,
    },
    Subtotal {
        name: String,
    },
    Percent {
        name: String,
        percent: DecStr,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        of: Vec<String>,
    },
    Flat {
        name: String,
        amount: Cents,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FileBidPick {
    Category { code: String },
    Account { code: String },
    Range { from: String, to: String },
}

impl From<&BidLayout> for FileBid {
    fn from(l: &BidLayout) -> FileBid {
        let BidLayout {
            fields,
            lines,
            show_accounts,
        } = l;
        FileBid {
            fields: fields
                .iter()
                .map(|f| FileBidField {
                    label: f.label.clone(),
                    value: f.value.clone(),
                })
                .collect(),
            lines: lines
                .iter()
                .map(|l| match l {
                    BidLine::Section { name, picks } => FileBidLine::Section {
                        name: name.clone(),
                        picks: picks
                            .iter()
                            .map(|p| match p {
                                BidPick::Category(code) => {
                                    FileBidPick::Category { code: code.clone() }
                                }
                                BidPick::Account(code) => {
                                    FileBidPick::Account { code: code.clone() }
                                }
                                BidPick::Range { from, to } => FileBidPick::Range {
                                    from: from.clone(),
                                    to: to.clone(),
                                },
                            })
                            .collect(),
                    },
                    BidLine::Subtotal { name } => FileBidLine::Subtotal { name: name.clone() },
                    BidLine::Percent { name, percent, of } => FileBidLine::Percent {
                        name: name.clone(),
                        percent: DecStr(*percent),
                        of: of.clone(),
                    },
                    BidLine::Flat { name, amount } => FileBidLine::Flat {
                        name: name.clone(),
                        amount: *amount,
                    },
                })
                .collect(),
            show_accounts: *show_accounts,
        }
    }
}

impl From<FileBid> for BidLayout {
    fn from(f: FileBid) -> BidLayout {
        BidLayout {
            fields: f
                .fields
                .into_iter()
                .map(|f| BidField {
                    label: f.label,
                    value: f.value,
                })
                .collect(),
            lines: f
                .lines
                .into_iter()
                .map(|l| match l {
                    FileBidLine::Section { name, picks } => BidLine::Section {
                        name,
                        picks: picks
                            .into_iter()
                            .map(|p| match p {
                                FileBidPick::Category { code } => BidPick::Category(code),
                                FileBidPick::Account { code } => BidPick::Account(code),
                                FileBidPick::Range { from, to } => BidPick::Range { from, to },
                            })
                            .collect(),
                    },
                    FileBidLine::Subtotal { name } => BidLine::Subtotal { name },
                    FileBidLine::Percent { name, percent, of } => BidLine::Percent {
                        name,
                        percent: percent.0,
                        of,
                    },
                    FileBidLine::Flat { name, amount } => BidLine::Flat { name, amount },
                })
                .collect(),
            show_accounts: f.show_accounts,
        }
    }
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
    Bid,
    Incentives,
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
    Locations, Currencies, CostReport, Bid, Incentives,
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
            bid,
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
            bid: bid.as_ref().map(FileBid::from),
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
                bid: f.bid.map(BidLayout::from),
            },
        }
    }
}
