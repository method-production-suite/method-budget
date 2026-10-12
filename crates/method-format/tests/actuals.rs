//! Actuals in the file format, imported from spreadsheets (synthetic CSV,
//! tab-delimited and Excel files made here, never real figures) and in the
//! spreadsheet export.

use std::collections::BTreeMap;
use std::sync::Arc;

use method_core::actuals::{
    add_batch, cost_report_of, set_etc, Actuals, EtcMethod, ImportBatch, Transaction,
};
use method_core::{recalc, starter, Budget, RecalcOptions};
use method_format::export::{
    self, workbook, xlsx, Cell, Sheet, Workbook, ACTUALS_SHEETS, SHEET_NAMES,
};
use method_format::import::actuals::{self as ia, CodeChoice, Field, Mapping, Options};
use method_format::import::{read_file, FileKind};

fn with_actuals() -> Budget {
    let mut b = starter::budget_with_separator('-');
    add_batch(
        &mut b,
        "september.csv",
        "2026-10-09T14:30:00-07:00",
        vec![
            Transaction {
                date: "2026-09-01".into(),
                account: "10-01".into(),
                line: Some(1),
                tags: ["PO".to_string()].into(),
                vendor: "Writer".into(),
                description: "First draft".into(),
                reference: "INV-1".into(),
                amount: 2_500_000,
                ..Transaction::default()
            },
            Transaction {
                account: "77-01".into(),
                amount: -1_234,
                ..Transaction::default()
            },
        ],
    )
    .unwrap();
    set_etc(
        &mut b,
        "11-01",
        Some(EtcMethod::PercentComplete("37.5".parse().unwrap())),
    )
    .unwrap();
    set_etc(&mut b, "12-01", Some(EtcMethod::Manual(10_000))).unwrap();
    b
}

#[test]
fn actuals_round_trip_and_are_written_as_documented() {
    let b = with_actuals();
    let text = method_format::to_string(&b);
    assert_eq!(method_format::from_str(&text).unwrap(), b);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let a = &v["budget"]["actuals"];
    assert_eq!(
        a["batches"][0],
        serde_json::json!({"id": 1, "file": "september.csv", "imported": "2026-10-09T14:30:00-07:00", "rows": 2})
    );
    assert_eq!(
        a["transactions"][0],
        serde_json::json!({
            "id": 1, "date": "2026-09-01", "account": "10-01", "line": 1, "tags": ["PO"],
            "vendor": "Writer", "description": "First draft", "reference": "INV-1",
            "amount": 2_500_000, "batch": 1
        })
    );
    // Defaults are left out; the amount is always written.
    assert_eq!(
        a["transactions"][1],
        serde_json::json!({"id": 2, "account": "77-01", "amount": -1234, "batch": 1})
    );
    assert_eq!(
        a["etc"]["11-01"],
        serde_json::json!({"kind": "percent_complete", "percent": "37.5"})
    );
    assert_eq!(
        a["etc"]["12-01"],
        serde_json::json!({"kind": "manual", "amount": 10000})
    );
    // The key sits after the baseline, before the display settings.
    let keys: Vec<&String> = v["budget"].as_object().unwrap().keys().collect();
    assert!(keys.contains(&&"actuals".to_string()));
    let at = text.find("\"actuals\"").unwrap();
    assert!(text.find("\"charges\"").unwrap() < at);
    // A budget without actuals has no key at all.
    assert!(
        !method_format::to_string(&starter::budget_with_separator('-')).contains("\"actuals\"")
    );
}

#[test]
fn readers_take_every_method_and_refuse_actuals_in_versions() {
    let doc = |actuals: &str| {
        format!(
            r#"{{"format": "method-budget", "version": 1, "budget": {{"title": "T", "base_currency": "USD", "actuals": {actuals}}}}}"#
        )
    };
    let b = method_format::from_str(&doc(
        r#"{"etc": {"10-01": {"kind": "budget_minus_actual"}, "11-01": {"kind": "manual", "amount": 5}}, "transactions": [{"id": 3, "amount": 7}]}"#,
    ))
    .unwrap();
    assert_eq!(
        b.actuals.etc.get("10-01"),
        Some(&EtcMethod::BudgetMinusActual)
    );
    assert_eq!(b.actuals.etc.get("11-01"), Some(&EtcMethod::Manual(5)));
    assert_eq!(b.actuals.transactions[0].amount, 7);
    // Strict: unknown fields, a missing amount, a float, a number for a percent.
    for bad in [
        r#"{"transactions": [{"id": 1, "amount": 1, "colour": "red"}]}"#,
        r#"{"transactions": [{"id": 1}]}"#,
        r#"{"transactions": [{"id": 1, "amount": 1.5}]}"#,
        r#"{"etc": {"10-01": {"kind": "percent_complete", "percent": 40}}}"#,
        r#"{"etc": {"10-01": {"kind": "guess"}}}"#,
        r#"{"batches": [{"id": 1}]}"#,
    ] {
        assert!(method_format::from_str(&doc(bad)).is_err(), "{bad}");
    }
    let version = r#"{"format": "method-budget", "version": 1, "budget": {"title": "T", "base_currency": "USD",
        "versions": [{"id": 1, "name": "v", "created": "", "budget": {"title": "T", "base_currency": "USD",
        "actuals": {"transactions": [{"id": 1, "amount": 1}]}}}]}}"#;
    let e = method_format::from_str(version).unwrap_err().to_string();
    assert!(e.contains("cannot have actuals"), "{e}");
    // Reading the default spellings back gives the same budget.
    let mut a = Actuals::default();
    a.batches.push(ImportBatch {
        id: 1,
        file: String::new(),
        imported: String::new(),
        rows: 0,
    });
    let mut b = starter::budget_with_separator('-');
    b.actuals = Arc::new(a);
    let text = method_format::to_string(&b);
    assert_eq!(method_format::from_str(&text).unwrap(), b);
}

// ---------------------------------------------------------------------------
// Import from files made here

const CSV: &str = "Ledger export\r\n\r\nTrans Date,GL Account,Payee,Memo,Invoice #,Amount\r\n\
9/1/2026,1001,Writer,First draft,INV-1,\"$2,500.00\"\r\n\
9/2/2026,10-01,Writer,Polish,INV-2,(100.00)\r\n\
9/3/2026,9999,Mystery,Unknown,,50\r\n\
,,,Total,,2450\r\n";

fn mapping_for(t: &method_format::import::Table) -> Mapping {
    let header_row = ia::detect_header(t);
    Mapping {
        header_row,
        columns: ia::guess(t, header_row),
    }
}

#[test]
fn csv_files_import_with_a_guessed_mapping() {
    let book = read_file(CSV.as_bytes(), "ledger.csv").unwrap();
    assert_eq!(book.kind, FileKind::Csv);
    let t = &book.tables[0];
    let m = mapping_for(t);
    assert_eq!(m.header_row, Some(2));
    assert_eq!(m.column(Field::Amount), Some(5));
    let mut b = starter::budget_with_separator('-');
    let p = ia::plan(&b, t, &m, &Options::default());
    assert_eq!(p.transactions.len(), 3);
    assert_eq!(p.total, 250_000 - 10_000 + 5_000);
    assert_eq!(p.unmatched.len(), 1);
    assert_eq!(p.skipped.len(), 1, "the total row");
    // Map the unknown number, then import: one batch, exact to the cent.
    let o = Options {
        codes: BTreeMap::from([("99-99".to_string(), CodeChoice::Map("11-01".into()))]),
        ..Options::default()
    };
    let (p, batch) = ia::import(&mut b, t, &m, &o, "ledger.csv", "2026-10-09T12:00:00Z").unwrap();
    assert_eq!((p.transactions.len(), batch), (3, 1));
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    let c = cost_report_of(&b, &r).unwrap();
    assert_eq!(c.total.actual, 245_000);
    assert_eq!(c.categories[0].accounts[0].figures.actual, 240_000);
    assert_eq!(c.unassigned.actual, 0);
    // Tab-delimited text reads the same way.
    let tsv = CSV.replace(',', "\t").replace("\"$2\t500.00\"", "2500");
    let book = read_file(tsv.as_bytes(), "ledger.txt").unwrap();
    assert_eq!(book.kind, FileKind::Tsv);
    let t = &book.tables[0];
    let p = ia::plan(
        &starter::budget_with_separator('-'),
        t,
        &mapping_for(t),
        &Options::default(),
    );
    assert_eq!(p.total, 245_000);
}

/// A workbook written with our own export writer: dates as serial numbers,
/// amounts as number cells.
fn workbook_bytes() -> Vec<u8> {
    let wb = Workbook {
        title: "Ledger".into(),
        sheets: vec![
            Sheet {
                name: "Notes",
                columns: vec!["Prepared by".into()],
                rows: vec![vec![Cell::text("Accounting")]],
            },
            Sheet {
                name: "Detail",
                columns: [
                    "Date",
                    "Account",
                    "Vendor",
                    "Description",
                    "Debit",
                    "Credit",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect(),
                rows: vec![
                    vec![
                        Cell::Int(46_266),
                        Cell::text("12-01"),
                        Cell::text("Director"),
                        Cell::text("Prep week"),
                        Cell::Money(1_000_050),
                        Cell::Empty,
                    ],
                    vec![
                        Cell::Int(46_267),
                        Cell::text("12-01"),
                        Cell::text("Director"),
                        Cell::text("Overpayment back"),
                        Cell::Empty,
                        Cell::Money(50),
                    ],
                ],
            },
        ],
    };
    xlsx::write(&wb).unwrap()
}

#[test]
fn workbooks_import_with_serial_dates_and_debits() {
    let book = read_file(&workbook_bytes(), "ledger.xlsx").unwrap();
    assert_eq!(book.kind, FileKind::Xlsx);
    let t = &book.tables[1];
    let m = mapping_for(t);
    assert_eq!(m.header_row, Some(0));
    assert_eq!(m.column(Field::Debit), Some(4));
    assert_eq!(m.column(Field::Credit), Some(5));
    let p = ia::plan(
        &starter::budget_with_separator('-'),
        t,
        &m,
        &Options::default(),
    );
    let got: Vec<(&str, i64)> = p
        .transactions
        .iter()
        .map(|x| (x.transaction.date.as_str(), x.transaction.amount))
        .collect();
    assert_eq!(got, [("2026-09-01", 1_000_050), ("2026-09-02", -50)]);
    assert!(p.unmatched.is_empty());
}

// ---------------------------------------------------------------------------
// Export

#[test]
fn exports_add_the_cost_report_and_transactions() {
    let b = with_actuals();
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    let wb = workbook(&b, &r).unwrap();
    let names: Vec<&str> = wb.sheets.iter().map(|s| s.name).collect();
    let mut want: Vec<&str> = SHEET_NAMES.to_vec();
    want.extend(ACTUALS_SHEETS);
    assert_eq!(names, want);
    let cost = wb.sheet("Cost Report").unwrap();
    let c = cost_report_of(&b, &r).unwrap();
    let grand = cost.rows.last().unwrap();
    assert_eq!(grand[2], Cell::text("Grand total"));
    assert_eq!(grand[3], Cell::Money(r.grand_total));
    assert_eq!(grand[5], Cell::Money(c.total.actual));
    assert_eq!(grand[9], Cell::Money(c.total.eac));
    assert_eq!(grand[10], Cell::Money(c.total.variance));
    // Account rows' actuals add up to the total less what is unassigned.
    let actual_col = 5;
    let accounts: i64 = cost
        .rows
        .iter()
        .filter(|r| matches!(&r[0], Cell::Text(k) if k == "Account" || k == "Fringe Account"))
        .map(|r| match r[actual_col] {
            Cell::Money(m) => m,
            _ => 0,
        })
        .sum();
    assert_eq!(accounts, c.total.actual - c.unassigned.actual);
    assert!(cost.rows.iter().any(|r| r[0] == Cell::text("Unassigned")));
    assert!(cost
        .rows
        .iter()
        .any(|r| r[8] == Cell::text("37.5% complete")));
    let tx = wb.sheet("Actuals").unwrap();
    assert_eq!(tx.rows.len(), 2);
    assert_eq!(tx.rows[0][2], Cell::text("Writing"), "the account's name");
    assert_eq!(
        tx.rows[0][8],
        Cell::text("Screenplay"),
        "the line it pays for"
    );
    assert_eq!(tx.rows[0][9], Cell::text("september.csv"));
    assert_eq!(tx.rows[1][2], Cell::Empty, "not in the chart");
    // Every format carries the sheets; CSV gets one file each.
    let files = export::render(
        &wb,
        export::Format::Csv,
        std::path::Path::new("/x/Film.csv"),
        export::Options::default(),
    )
    .unwrap();
    assert!(files
        .iter()
        .any(|f| f.path.ends_with("Film - Cost Report.csv")));
    assert!(files.iter().any(|f| f.path.ends_with("Film - Actuals.csv")));
    let bytes = xlsx::write(&wb).unwrap();
    let back = read_file(&bytes, "Film.xlsx").unwrap();
    assert!(back.tables.iter().any(|t| t.name == "Cost Report"));
    // A budget without actuals exports as before.
    let plain = starter::budget_with_separator('-');
    let wb = workbook(&plain, &recalc(&plain, &RecalcOptions::default()).unwrap()).unwrap();
    assert_eq!(wb.sheets.len(), SHEET_NAMES.len());
}
