//! Charges and credits.

use super::*;

// ---------------------------------------------------------------------------
// Charges and credits

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileExcludedAmount {
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
pub(super) enum FileCharge {
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
pub(super) struct FileCredit {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account: Option<String>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    applied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cap: Option<Cents>,
    #[serde(default = "zero", skip_serializing_if = "DecStr::is_zero")]
    discount_percent: DecStr,
    /// Fringes left out of every group and category row's spend.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    excluded_fringes: BTreeSet<String>,
    /// Locations the spend has to be at (decisions.md, entry 80). Left out
    /// when empty, so a credit without them is written as before.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    locations: BTreeSet<String>,
    /// The qualified spend below which the credit is 0 (decisions.md,
    /// entry 88). Left out when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    minimum_spend: Option<Cents>,
    /// A flat fee taken off after the discount. Left out when 0.
    #[serde(default, skip_serializing_if = "is_zero_cents")]
    broker_fee: Cents,
    /// The name of a cap in the budget's `shared_caps`. Left out when none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shared_cap: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rows: Vec<FileCreditRow>,
}

fn is_zero_cents(c: &Cents) -> bool {
    *c == 0
}

/// A cap several credits share (decisions.md, entry 88).
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileSharedCap {
    pub(crate) amount: Cents,
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
            excluded_fringes,
            locations,
            minimum_spend,
            broker_fee,
            shared_cap,
            rows,
        } = c;
        FileCredit {
            name: name.clone(),
            account: account.clone(),
            applied: *applied,
            cap: *cap,
            discount_percent: DecStr(*discount_percent),
            excluded_fringes: excluded_fringes.clone(),
            locations: locations.clone(),
            minimum_spend: *minimum_spend,
            broker_fee: *broker_fee,
            shared_cap: shared_cap.clone(),
            rows: rows.iter().map(FileCreditRow::from).collect(),
        }
    }
}

impl From<FileCredit> for Credit {
    fn from(f: FileCredit) -> Credit {
        // Older files kept the list on each group or category row: the
        // credit takes the union of them (FORMAT.md, changelog).
        let mut excluded_fringes = f.excluded_fringes;
        for r in &f.rows {
            match &r.source {
                FileCreditSource::Flat { .. } | FileCreditSource::Account { .. } => {}
                FileCreditSource::Group {
                    excluded_fringes: old,
                    ..
                }
                | FileCreditSource::Category {
                    excluded_fringes: old,
                    ..
                } => excluded_fringes.extend(old.iter().cloned()),
            }
        }
        Credit {
            name: f.name,
            account: f.account,
            applied: f.applied,
            cap: f.cap,
            discount_percent: f.discount_percent.0,
            excluded_fringes,
            locations: f.locations,
            minimum_spend: f.minimum_spend,
            broker_fee: f.broker_fee,
            shared_cap: f.shared_cap,
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
        /// Read only, from older files: the credit takes the union of its
        /// rows' lists. Never written.
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        excluded_fringes: BTreeSet<String>,
    },
    Category {
        category: String,
        /// As for `Group`: read from older files only.
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        excluded_fringes: BTreeSet<String>,
    },
    /// Version 1, additive (decisions.md, entry 61): never had a list of
    /// its own.
    Account {
        account: String,
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
                CreditSource::Group { group } => FileCreditSource::Group {
                    group: group.clone(),
                    excluded_fringes: BTreeSet::new(),
                },
                CreditSource::Category { category } => FileCreditSource::Category {
                    category: category.clone(),
                    excluded_fringes: BTreeSet::new(),
                },
                CreditSource::Account { account } => FileCreditSource::Account {
                    account: account.clone(),
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
                FileCreditSource::Group { group, .. } => CreditSource::Group { group },
                FileCreditSource::Category { category, .. } => CreditSource::Category { category },
                FileCreditSource::Account { account } => CreditSource::Account { account },
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
