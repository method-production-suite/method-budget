//! Baseline and actuals.

use super::*;

// ---------------------------------------------------------------------------
// Baseline

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileBaseline {
    /// Keyed by row id; JSON object keys are strings, so ids are written as `"17"`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    rows: BTreeMap<RowId, Cents>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    accounts: BTreeMap<String, Cents>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    categories: BTreeMap<String, Cents>,
    grand_total: Cents,
    net_total: Cents,
    /// Version 1, additive (decisions.md, entry 61): left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    frozen: bool,
}

impl From<&Baseline> for FileBaseline {
    fn from(b: &Baseline) -> FileBaseline {
        let Baseline {
            rows,
            accounts,
            categories,
            grand_total,
            net_total,
            frozen,
        } = b;
        FileBaseline {
            rows: rows.clone(),
            accounts: accounts.clone(),
            categories: categories.clone(),
            grand_total: *grand_total,
            net_total: *net_total,
            frozen: *frozen,
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
            frozen: f.frozen,
        }
    }
}

// ---------------------------------------------------------------------------
// Actuals (transactions from accounting, estimates to complete;
// method_core::actuals)

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileActuals {
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
