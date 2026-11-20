//! Topsheet, accounts and detail rows.

use super::*;

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
pub(super) struct FileTopsheetRow {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    person: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rate_source: Option<FileRateSource>,
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
            person,
            rate_source,
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
            person: person.clone(),
            rate_source: rate_source.clone().map(FileRateSource),
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
            person: f.person,
            rate_source: f.rate_source.map(|s| s.0),
            bold: f.bold,
            italic: f.italic,
            indent: f.indent,
            color: f.color,
            notes: read_notes(f.note, f.notes),
        }
    }
}

/// A row in a line block: a [detail row](FileDetail) without an `id` (the
/// copies inserted get new ones). Converted through [`FileDetail`], so the
/// two always agree field for field.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileBlockRow {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    person: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rate_source: Option<FileRateSource>,
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
            person,
            rate_source,
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
            person,
            rate_source,
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
            person,
            rate_source,
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
            person,
            rate_source,
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
