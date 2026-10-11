//! Spreadsheet import against files made elsewhere and our own exports:
//!
//! - raw DEFLATE streams made by zlib (Python's standard library) at
//!   several levels and strategies decode to the bytes the manifest says;
//! - workbooks written by hand with Python's zipfile read as expected, and
//!   damaged or hostile ones are refused (tests/fixtures/make_fixtures.py
//!   makes them all);
//! - a budget exported to Excel, CSV and tab-delimited text and imported back
//!   with the guessed mapping has exactly the same line totals and grand
//!   total.

use std::path::PathBuf;

use method_core::sample::{budget as sample_budget, SampleSpec};
use method_core::{recalc, Budget, Cents, FringePosting, RecalcOptions, RowKind};
use method_format::export::{csv, workbook, xlsx};
use method_format::import::mapping::{detect_header, guess, Field, Mapping};
use method_format::import::merge::{self, Options};
use method_format::import::{
    inflate::inflate, read_file, xlsxread::read_xlsx, FileKind, Table, Value,
};

fn fixture(path: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(path);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn zlib_streams_decode_exactly() {
    let manifest = String::from_utf8(fixture("inflate/manifest.txt")).unwrap();
    let mut checked = 0;
    for line in manifest
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let (name, len, crc) = (
            parts[0],
            parts[1].parse::<usize>().unwrap(),
            u32::from_str_radix(parts[2], 16).unwrap(),
        );
        let z = fixture(&format!("inflate/{name}.deflate"));
        let out = inflate(&z, 64 << 20).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(out.len(), len, "{name}");
        assert_eq!(method_format::export::zip::crc32(&out), crc, "{name}");
        // One byte short of the right size is refused, not cut.
        if len > 0 {
            assert!(inflate(&z, len - 1).is_err(), "{name}");
        }
        // Every truncation is an error.
        for cut in [0, z.len() / 3, z.len() / 2, z.len().saturating_sub(1)] {
            if cut < z.len() {
                assert!(inflate(&z[..cut], 64 << 20).is_err(), "{name} cut at {cut}");
            }
        }
        checked += 1;
    }
    assert!(checked >= 25, "only {checked} fixtures");
}

fn texts(t: &Table) -> Vec<Vec<String>> {
    t.rows
        .iter()
        .map(|r| r.iter().map(|v| v.text().to_string()).collect())
        .collect()
}

#[test]
fn hand_made_workbooks_read() {
    for name in ["basic.xlsx", "descriptors.xlsx", "comment.xlsx"] {
        let tables =
            read_xlsx(&fixture(&format!("xlsx/{name}"))).unwrap_or_else(|e| panic!("{name}: {e}"));
        let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Crew & Cast", "Hidden", "Prefixed"], "{name}");
        assert_eq!(
            tables.iter().map(|t| t.hidden).collect::<Vec<_>>(),
            [false, true, false]
        );
        let s = &tables[0];
        assert_eq!(
            s.rows[0],
            ["Account", "Description", "Rate (USD)", "Amount", "Total"]
                .map(|t| Value::Text(t.into()))
                .to_vec(),
            "rich text runs joined, inline strings read"
        );
        assert_eq!(
            s.rows[1],
            vec![
                Value::Number("1100".into()),
                Value::Text("Caf\u{e9} & crew <day> \u{1f3ac}".into()),
                Value::Number("1234.5".into()),
                Value::Number("1.5E-3".into()),
                Value::Number("1.85175".into()),
            ]
        );
        assert!(s.rows[2].is_empty(), "row 3 has no cells");
        assert_eq!(
            s.rows[3],
            vec![
                Value::Text("11-01".into()),
                Value::Text("Line\rbreak".into()),
                Value::Number("0.30000000000000004".into()),
                Value::Bool(true),
                Value::Text("#DIV/0!".into()),
            ],
            "cells without references follow the one before"
        );
        assert_eq!(
            texts(s)[4],
            ["a<b"],
            "CDATA; an empty string and an empty cell trimmed"
        );
        assert_eq!(s.rows.len(), 5, "empty rows at the end trimmed");
        assert_eq!(tables[1].rows, vec![vec![Value::Number("42".into())]]);
        let p = &tables[2];
        assert_eq!(p.rows[0][1], Value::Text("Strict".into()));
        assert_eq!(p.rows[0][26], Value::Number("-7.25".into()));
    }
}

#[test]
fn hostile_or_damaged_workbooks_are_refused() {
    let cases = [
        ("doctype.xlsx", "DTD"),
        ("entity.xlsx", "entity"),
        ("badxml.xlsx", "closes"),
        ("badcrc.xlsx", "checksum"),
        ("notzip.xlsx", "ZIP"),
    ];
    for (name, says) in cases {
        let e = read_file(&fixture(&format!("xlsx/{name}")), name).unwrap_err();
        assert!(e.0.contains(says), "{name}: {e}");
    }
}

// ---------------------------------------------------------------------------
// Round trip through our own exports

fn sample() -> Budget {
    sample_budget(&SampleSpec {
        rows: 900,
        accounts: 30,
        ..SampleSpec::default()
    })
}

/// `b` with its chart, setup and settings but no lines (what New From
/// Template makes): where the export is imported back into.
fn without_lines(b: &Budget) -> Budget {
    let mut t = b.clone();
    for c in t.categories_mut() {
        for a in &mut c.accounts {
            a.details.clear();
        }
    }
    t.baseline = None;
    t
}

/// Every row's (kind, total, fringe total), in budget order.
fn row_totals(b: &Budget) -> Vec<(RowKind, Cents, Cents)> {
    let r = recalc(b, &RecalcOptions::default()).unwrap();
    r.categories
        .iter()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.rows)
        .map(|rr| (rr.kind, rr.total, rr.fringe_total))
        .collect()
}

fn import_back(b: &Budget, t: &Table) -> Budget {
    let header = detect_header(t);
    assert_eq!(header, Some(0));
    let m = Mapping {
        header_row: header,
        columns: guess(t, header),
    };
    for f in [
        Field::AccountCode,
        Field::Kind,
        Field::Amount,
        Field::Rate,
        Field::Fringes,
        Field::FringeRange,
        Field::Groups,
        Field::Currency,
        Field::LineTotal,
    ] {
        assert!(m.column(f).is_some(), "{f:?} not guessed");
    }
    let o = Options {
        create_accounts: true,
        create_setup: true,
        default_account: String::new(),
    };
    let mut into = without_lines(b);
    let preview = merge::preview(&into, t, &m, &o);
    assert!(
        preview.plan.skipped.is_empty(),
        "{:?}",
        &preview.plan.skipped[..3.min(preview.plan.skipped.len())]
    );
    assert!(
        preview.plan.warnings.is_empty(),
        "{:?}",
        &preview.plan.warnings[..3.min(preview.plan.warnings.len())]
    );
    assert!(preview.plan.new_accounts.is_empty() && preview.plan.new_categories.is_empty());
    let rec = preview.reconciliation.expect("a total column");
    assert_eq!(rec.mismatched, 0, "{:?}", rec.first.first());
    assert_eq!(rec.sheet_sum, rec.computed_sum);
    merge::import(&mut into, t, &m, &o).unwrap();
    into
}

fn check_round_trip(b: &Budget, back: &Budget) {
    assert_eq!(row_totals(back), row_totals(b), "line totals");
    let (r1, r2) = (
        recalc(b, &RecalcOptions::default()).unwrap(),
        recalc(back, &RecalcOptions::default()).unwrap(),
    );
    assert_eq!(r2.grand_total, r1.grand_total, "grand total");
    assert_eq!(r2.net_total, r1.net_total, "net total");
}

#[test]
fn an_exported_budget_imports_back_to_the_same_totals() {
    let mut by_account = sample();
    by_account.fringe_posting = FringePosting::Account;
    for b in [sample(), by_account] {
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let wb = workbook(&b, &r).unwrap();

        // Excel.
        let bytes = xlsx::write(&wb).unwrap();
        let book = read_file(&bytes, "Sample.xlsx").unwrap();
        assert_eq!(book.kind, FileKind::Xlsx);
        let detail = book.tables.iter().find(|t| t.name == "Detail").unwrap();
        check_round_trip(&b, &import_back(&b, detail));

        // CSV, with a byte order mark.
        let sheet = wb.sheet("Detail").unwrap();
        let bytes = csv::write(sheet, csv::Delimiter::Comma, true);
        let book = read_file(&bytes, "Sample - Detail.csv").unwrap();
        assert_eq!(book.kind, FileKind::Csv);
        check_round_trip(&b, &import_back(&b, &book.tables[0]));

        // Tab-delimited.
        let bytes = csv::write(sheet, csv::Delimiter::Tab, false);
        let book = read_file(&bytes, "Sample.txt").unwrap();
        assert_eq!(book.kind, FileKind::Tsv);
        check_round_trip(&b, &import_back(&b, &book.tables[0]));
    }
}

#[test]
fn importing_twice_doubles_the_lines_not_the_setup() {
    let b = sample();
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    let wb = workbook(&b, &r).unwrap();
    let t = &read_file(&xlsx::write(&wb).unwrap(), "s.xlsx")
        .unwrap()
        .tables[2];
    let m = Mapping {
        header_row: Some(0),
        columns: guess(t, Some(0)),
    };
    let o = Options {
        create_accounts: false,
        create_setup: false,
        default_account: String::new(),
    };
    let mut twice = b.clone();
    let (plan, _) = merge::import(&mut twice, t, &m, &o).unwrap();
    assert!(plan.added.is_empty() && plan.dropped.is_empty());
    let lines = |b: &Budget| -> usize {
        b.categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.details.len())
            .sum()
    };
    assert_eq!(lines(&twice), 2 * lines(&b));
    assert_eq!(twice.fringes, b.fringes);
    // Every row id is still unique (recalc refuses duplicates).
    recalc(&twice, &RecalcOptions::default()).unwrap();
}
