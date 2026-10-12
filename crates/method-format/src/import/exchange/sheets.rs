//! The budget export's Excel workbook and its tab-delimited topsheet.
//!
//! **The workbook** has four sheets:
//!
//! - *Budget Metadata*: a label and a value per row (`Base Currency`,
//!   `Conv Currency`, counts, then the budget information fields).
//! - *Topsheet*: `Account` (the category number), `Category Description`,
//!   `Fringe`, `Original`, `Total`, `Variance`; rows with no number are the
//!   entry row, the Total Fringes rows, charges and credits (figures
//!   unrounded). No row says which is a fringe row: it is the one named
//!   Total Fringes.
//! - *Categories*: the accounts with the same columns, every category's in
//!   turn, each closed by its empty entry row (and, posting fringes by
//!   category, its fringe account after it, named Total Fringes). No column
//!   names the category, so the rows are placed in that order.
//! - *Account Details*: `Account` (the number), `Agg` and `Agg Sum` (the
//!   fringe percentages as fractions), `Fringes`, `Groups` and `Globals
//!   Used` (comma-separated names), `Location`, `Set`, `Description`,
//!   `Amount`, `Unit`, `X`, `Unit 2`, `Currency`, `Rate`, `Unit 3`, `4X`,
//!   `Unit 4`, `Subtotal`. The entry rows of the empty accounts have no
//!   number.
//!
//! It has no fringe figures (no rates, no kinds): the builder works them
//! out from the lines.
//!
//! **The tab-delimited export** is the topsheet and the accounts' totals
//! only, in whole units: five columns (category number and name, account
//! number and name, total).
//!
//! Both are made into the tree the JSON reader takes.

use super::json::{from_tree, J};
use super::model::{Export, ExportFormat};
use crate::import::csvread::{read_csv, Delimiter};
use crate::import::xlsxread::read_xlsx;
use crate::import::{ReadError, Table};

fn sheet<'a>(tables: &'a [Table], name: &str) -> Option<&'a Table> {
    tables
        .iter()
        .find(|t| t.name.trim().eq_ignore_ascii_case(name))
}

fn cell(t: &Table, r: usize, c: Option<usize>) -> String {
    c.map(|c| t.cell(r, c).text().trim().to_string())
        .unwrap_or_default()
}

/// The column titled `title` in row 1 (any case).
fn column(t: &Table, title: &str) -> Option<usize> {
    (0..t.width()).find(|&c| t.cell(0, c).text().trim().eq_ignore_ascii_case(title))
}

/// Whether `bytes` are the export's workbook: a ZIP with the four sheets'
/// telltale ones.
pub fn looks_like_workbook(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"PK") {
        return false;
    }
    read_xlsx(bytes)
        .is_ok_and(|t| sheet(&t, "Account Details").is_some() && sheet(&t, "Topsheet").is_some())
}

/// A date cell (a serial number in the workbook) as ISO 8601; text as it is.
fn date_text(t: &Table, v: &str) -> String {
    match v.parse::<f64>() {
        Ok(n) if n.is_finite() && n > 0.0 => {
            let serial = n.floor() as i64;
            let d = if t.date1904 {
                crate::import::actuals::serial_date_1904(serial)
            } else {
                crate::import::actuals::serial_date(serial)
            };
            d.unwrap_or_else(|| v.to_string())
        }
        _ => v.to_string(),
    }
}

/// A fraction (`0.125`) as the percentage text the JSON layout writes.
fn percent_text(v: &str) -> String {
    match v.trim().parse::<f64>() {
        Ok(f) if f.is_finite() => format!("{}%", f * 100.0),
        _ => String::new(),
    }
}

fn list(v: &str) -> J {
    J::Arr(
        v.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(J::s)
            .collect(),
    )
}

fn is_total_fringes(name: &str) -> bool {
    name.trim().eq_ignore_ascii_case("Total Fringes")
}

/// Reads the export's Excel workbook.
pub fn read_workbook(bytes: &[u8]) -> Result<Export, ReadError> {
    let tables = read_xlsx(bytes)?;
    let need = |name: &str| {
        sheet(&tables, name).ok_or_else(|| {
            ReadError::new(format!(
                "this workbook isn't a budget export (it has no {name} sheet)"
            ))
        })
    };
    let top = need("Topsheet")?;
    let accts = need("Categories")?;
    let det = need("Account Details")?;

    // Metadata: every label, spaces taken out, as both budget data and
    // information fields.
    let mut fields: Vec<(String, J)> = Vec::new();
    if let Some(m) = sheet(&tables, "Budget Metadata") {
        for r in 0..m.rows.len() {
            let label: String = m
                .cell(r, 0)
                .text()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            if label.is_empty() {
                continue;
            }
            let mut v = m.cell(r, 1).text().trim().to_string();
            if label.ends_with("ProductionDate") {
                v = date_text(m, &v);
            }
            fields.push((label, J::Str(v)));
        }
    }
    let meta = J::obj(vec![
        ("budgetdata", J::Obj(fields.clone())),
        ("defaultfields", J::Obj(fields)),
    ]);

    // The topsheet.
    let tc = |t: &str| column(top, t);
    let (t_num, t_name, t_fr, t_orig, t_tot) = (
        tc("Account"),
        tc("Category Description"),
        tc("Fringe"),
        tc("Original"),
        tc("Total"),
    );
    let mut categories = Vec::new();
    // Rows that take accounts: categories and the entry row.
    let mut account_holders: Vec<usize> = Vec::new();
    for r in 1..top.rows.len() {
        let number = cell(top, r, t_num);
        let name = cell(top, r, t_name);
        let id = categories.len();
        let mut f = vec![
            ("cID", J::Str(id.to_string())),
            ("cNumber", J::Str(number.clone())),
            ("cDescription", J::Str(name.clone())),
            ("cOriginal", J::Str(cell(top, r, t_orig))),
            ("cTotal", J::Str(cell(top, r, t_tot))),
        ];
        if !number.is_empty() || name.is_empty() {
            f.push(("cFringe", J::Str(cell(top, r, t_fr))));
            account_holders.push(id);
        } else {
            f.push(("cFringe", J::Null));
            if is_total_fringes(&name) {
                f.push(("fringeDetail", J::Arr(Vec::new())));
            }
        }
        categories.push(J::obj(f));
    }

    // The accounts, placed in their categories in order.
    let ac = |t: &str| column(accts, t);
    let (a_num, a_name, a_fr, a_orig, a_tot) = (
        ac("Account"),
        ac("Account Description"),
        ac("Fringe"),
        ac("Original"),
        ac("Total"),
    );
    let mut accounts = Vec::new();
    let mut numbers: Vec<String> = Vec::new();
    let (mut g, mut after_blank) = (0usize, false);
    let mut unplaced = 0usize;
    for r in 1..accts.rows.len() {
        let number = cell(accts, r, a_num);
        let name = cell(accts, r, a_name);
        let blank = number.is_empty() && name.is_empty();
        let fringe_account = !number.is_empty() && is_total_fringes(&name);
        let holder = if blank {
            if after_blank {
                g += 1;
            }
            after_blank = true;
            g
        } else if fringe_account && after_blank {
            after_blank = false;
            g += 1;
            g - 1
        } else {
            if after_blank {
                g += 1;
                after_blank = false;
            }
            g
        };
        let Some(&cat) = account_holders.get(holder) else {
            unplaced += 1;
            continue;
        };
        let id = accounts.len();
        let mut f = vec![
            ("categoryID", J::Str(cat.to_string())),
            ("aID", J::Str(id.to_string())),
            ("aNumber", J::Str(number.clone())),
            ("aDescription", J::Str(name)),
            ("aOriginal", J::Str(cell(accts, r, a_orig))),
            ("aTotal", J::Str(cell(accts, r, a_tot))),
        ];
        if fringe_account {
            f.push(("aFringe", J::Null));
            f.push(("fringeDetail", J::Arr(Vec::new())));
        } else {
            f.push(("aFringe", J::Str(cell(accts, r, a_fr))));
        }
        accounts.push(J::obj(f));
        numbers.push(number);
    }

    // The detail rows, by account number.
    let dc = |t: &str| column(det, t);
    let cols = [
        ("dDescription", dc("Description")),
        ("dUnit", dc("Unit")),
        ("dUnit2", dc("Unit 2")),
        ("dCurrency", dc("Currency")),
        ("dUnit3", dc("Unit 3")),
        ("dUnit4", dc("Unit 4")),
        ("dLocation", dc("Location")),
        ("dSet", dc("Set")),
        ("dAmount", dc("Amount")),
        ("dX", dc("X")),
        ("dRate", dc("Rate")),
        ("dFourthMlt", dc("4X")),
        ("dSubtotal", dc("Subtotal")),
    ];
    let (d_acct, d_agg, d_sum, d_fr, d_gr, d_gl) = (
        dc("Account"),
        dc("Agg"),
        dc("Agg Sum"),
        dc("Fringes"),
        dc("Groups"),
        dc("Globals Used"),
    );
    let mut details = Vec::new();
    let mut orphans = 0usize;
    for r in 1..det.rows.len() {
        let number = cell(det, r, d_acct);
        if number.is_empty() {
            // An empty account's entry row.
            continue;
        }
        let Some(id) = numbers.iter().position(|n| *n == number) else {
            orphans += 1;
            continue;
        };
        let mut f: Vec<(&str, J)> = vec![("accountID", J::Str(id.to_string()))];
        for (key, c) in cols {
            f.push((key, J::Str(cell(det, r, c))));
        }
        f.push(("dAggPercent", J::Str(percent_text(&cell(det, r, d_agg)))));
        f.push(("dAggSumPercent", J::Str(percent_text(&cell(det, r, d_sum)))));
        f.push(("dFringes", list(&cell(det, r, d_fr))));
        f.push(("dGroups", list(&cell(det, r, d_gr))));
        f.push(("dGlobalsUsed", list(&cell(det, r, d_gl))));
        // A Total Fringes row (fringes posted by account): no amounts.
        if is_total_fringes(&cell(det, r, cols[0].1)) && cell(det, r, cols[8].1).is_empty() {
            f.push(("fringeDetail", J::Arr(Vec::new())));
        }
        details.push(J::obj(f));
    }

    let root = J::obj(vec![
        ("metadata", meta),
        ("categories", J::Arr(categories)),
        ("accounts", J::Arr(accounts)),
        ("details", J::Arr(details)),
    ]);
    let mut x = from_tree(&root, ExportFormat::Excel, 2)?;
    x.fringe_figures = false;
    if unplaced > 0 {
        x.notes.push(format!(
            "Rows of the Categories sheet that couldn't be placed in a category, left out: {unplaced}."
        ));
    }
    if orphans > 0 {
        x.notes.push(format!(
            "Rows of the Account Details sheet naming an account the workbook doesn't list, left out: {orphans}."
        ));
    }
    Ok(x)
}

/// Whether `bytes` look like the tab-delimited topsheet export: every
/// line five fields, the last a number.
pub fn looks_like_tab(bytes: &[u8]) -> bool {
    let Ok(read) = read_csv(bytes, "", Some(Delimiter::Tab)) else {
        return false;
    };
    let t = &read.table;
    read.delimiter == Delimiter::Tab
        && !t.rows.is_empty()
        && t.rows.iter().filter(|r| !r.is_empty()).all(|r| {
            r.len() <= 5
                && r.last()
                    .is_some_and(|v| v.text().trim().parse::<f64>().is_ok())
        })
        && t.rows.iter().any(|r| r.len() == 5)
}

/// Reads the tab-delimited topsheet export: categories, their accounts
/// with one line holding each account's total, charges and credits.
pub fn read_tab(bytes: &[u8]) -> Result<Export, ReadError> {
    let read = read_csv(bytes, "export", Some(Delimiter::Tab))?;
    let t = &read.table;
    let mut categories = Vec::new();
    let mut accounts = Vec::new();
    let mut details = Vec::new();
    let mut current: Option<usize> = None;
    for r in 0..t.rows.len() {
        let f: Vec<String> = (0..5)
            .map(|c| t.cell(r, c).text().trim().to_string())
            .collect();
        if f.iter().all(String::is_empty) {
            continue;
        }
        let total = f[4].clone();
        if !f[0].is_empty() {
            let id = categories.len();
            categories.push(J::obj(vec![
                ("cID", J::Str(id.to_string())),
                ("cNumber", J::Str(f[0].clone())),
                ("cDescription", J::Str(f[1].clone())),
                ("cFringe", J::s("0")),
                ("cOriginal", J::s("0")),
                ("cTotal", J::Str(total)),
            ]));
            current = Some(id);
        } else if !f[1].is_empty() {
            // A charge, credit or Total Fringes row: a figure only.
            categories.push(J::obj(vec![
                ("cID", J::Str(categories.len().to_string())),
                ("cNumber", J::s("")),
                ("cDescription", J::Str(f[1].clone())),
                ("cFringe", J::Null),
                ("cOriginal", J::s("0")),
                ("cTotal", J::Str(total)),
            ]));
        } else if let Some(cat) = current {
            let zero = total.parse::<f64>().is_ok_and(|v| v == 0.0);
            if (f[2].is_empty() && f[3].is_empty()) || (is_total_fringes(&f[3]) && zero) {
                // An entry row, or an empty fringe account.
                continue;
            }
            let id = accounts.len();
            accounts.push(J::obj(vec![
                ("categoryID", J::Str(cat.to_string())),
                ("aID", J::Str(id.to_string())),
                ("aNumber", J::Str(f[2].clone())),
                ("aDescription", J::Str(f[3].clone())),
                ("aFringe", J::s("0")),
                ("aOriginal", J::s("0")),
                ("aTotal", J::Str(total.clone())),
            ]));
            if !zero {
                details.push(J::obj(vec![
                    ("accountID", J::Str(id.to_string())),
                    ("dDescription", J::s("Account total from the export")),
                    ("dAmount", J::s("1")),
                    ("dX", J::s("1")),
                    ("dRate", J::Str(total.clone())),
                    ("dFourthMlt", J::s("1")),
                    ("dSubtotal", J::Str(total)),
                    ("dFringes", J::Arr(Vec::new())),
                    ("dGroups", J::Arr(Vec::new())),
                    ("dGlobalsUsed", J::Arr(Vec::new())),
                ]));
            }
        }
    }
    if categories.is_empty() {
        return Err(ReadError::new(
            "this tab-delimited file isn't a budget export (it has no categories)",
        ));
    }
    let root = J::obj(vec![
        ("categories", J::Arr(categories)),
        ("accounts", J::Arr(accounts)),
        ("details", J::Arr(details)),
    ]);
    let mut x = from_tree(&root, ExportFormat::Tab, 0)?;
    x.totals_only = true;
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::exchange::model::TopRow;

    #[test]
    fn the_tab_topsheet_reads_as_totals() {
        let text = "1100\tStory\t\t\t1500\n\t\t1101\tRights\t1000\n\t\t\t\t0\n\t\t1199\tTotal Fringes\t500\n\t\t\t\t0\n\t\t99\tTotal Fringes\t0\n\tFee\t\t\t75\n\tRebate\t\t\t-20\n";
        assert!(looks_like_tab(text.as_bytes()));
        assert!(!looks_like_tab(b"Account\tRate\n1100\t5\n"));
        let x = read_tab(text.as_bytes()).unwrap();
        assert!(x.totals_only);
        assert_eq!(x.places, 0);
        let TopRow::Category(c) = &x.rows[0] else {
            panic!()
        };
        assert_eq!(c.accounts.len(), 2);
        assert_eq!(c.accounts[1].number, "1199");
        assert_eq!(c.accounts[1].details[0].subtotal, 500.0);
        assert!(matches!(x.rows[1], TopRow::Charge(_)));
        assert!(matches!(x.rows[2], TopRow::Credit(_)));
    }

    #[test]
    fn percentages_and_lists() {
        assert_eq!(percent_text("0.125"), "12.5%");
        assert_eq!(percent_text(""), "");
        assert_eq!(list("A, B ,"), J::Arr(vec![J::s("A"), J::s("B")]));
    }
}
