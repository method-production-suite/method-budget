//! Spreadsheet export against whole budgets: every table lines up with the
//! budget and its report, and every format reads back to the same fields.

use std::path::Path;

use fwb_core::decimal::{mul_cents, Exact};
use fwb_core::sample::{budget as sample_budget, SampleSpec};
use fwb_core::{recalc, starter, Budget, Cents, Dec, FringePosting, RecalcOptions, Report};
use fwb_format::export::{
    self, csv, money_text, render, workbook, xlsx, Cell, Format, Options, Sheet, Workbook,
    SHEET_NAMES,
};

fn sample() -> Budget {
    sample_budget(&SampleSpec {
        rows: 800,
        accounts: 30,
        ..SampleSpec::default()
    })
}

fn report(b: &Budget) -> Report {
    recalc(b, &RecalcOptions::default()).unwrap()
}

fn col(s: &Sheet, title: &str) -> usize {
    s.columns
        .iter()
        .position(|c| c == title)
        .unwrap_or_else(|| panic!("{} has no column {title}", s.name))
}

fn money_sum(s: &Sheet, title: &str) -> Cents {
    let i = col(s, title);
    s.rows
        .iter()
        .map(|r| match &r[i] {
            Cell::Money(c) => *c,
            _ => 0,
        })
        .sum()
}

fn budgets() -> Vec<Budget> {
    let mut by_account = sample();
    by_account.fringe_posting = FringePosting::Account;
    let mut by_budget = sample();
    by_budget.fringe_posting = FringePosting::Budget;
    vec![sample(), by_account, by_budget, starter::budget()]
}

#[test]
fn tables_line_up_with_the_budget() {
    for b in budgets() {
        let r = report(&b);
        let wb = workbook(&b, &r).unwrap();
        let names: Vec<&str> = wb.sheets.iter().map(|s| s.name).collect();
        assert_eq!(names, SHEET_NAMES);
        for s in &wb.sheets {
            for row in &s.rows {
                assert_eq!(row.len(), s.columns.len(), "{} row width", s.name);
            }
        }

        // Detail: one row per detail row, in order, adding up to the report.
        let detail = wb.sheet("Detail").unwrap();
        let details: Vec<_> = b
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .collect();
        assert_eq!(detail.rows.len(), details.len());
        let id = col(detail, "Row ID");
        for (row, d) in detail.rows.iter().zip(&details) {
            assert_eq!(row[id], Cell::Int(d.id));
        }
        let details_total: Cents = r.categories.iter().map(|c| c.details_total).sum();
        let fringe_total: Cents = r.categories.iter().map(|c| c.fringe_total).sum();
        assert_eq!(money_sum(detail, "Total"), details_total);
        assert_eq!(money_sum(detail, "Fringe Total"), fringe_total);

        // Accounts: every account (plus category fringe accounts), adding up
        // to the categories' totals.
        let accounts = wb.sheet("Accounts").unwrap();
        let n_accounts: usize = r.categories.iter().map(|c| c.accounts.len()).sum();
        let n_fringe: usize = r
            .categories
            .iter()
            .filter(|c| c.fringe_account.is_some())
            .count();
        assert_eq!(accounts.rows.len(), n_accounts + n_fringe);
        let cat_total: Cents = r.categories.iter().map(|c| c.total).sum();
        assert_eq!(money_sum(accounts, "Total"), cat_total);
        assert_eq!(money_sum(accounts, "Details Total"), details_total);

        // Topsheet: every category once, then the bottom lines.
        let top = wb.sheet("Topsheet").unwrap();
        let kinds: Vec<String> = top
            .rows
            .iter()
            .map(|r| match &r[0] {
                Cell::Text(s) => s.clone(),
                _ => String::new(),
            })
            .collect();
        let count = |k: &str| kinds.iter().filter(|x| *x == k).count();
        assert_eq!(count("Category"), r.categories.len());
        assert_eq!(count("Charge"), r.charges.len());
        assert_eq!(count("Credit"), r.credits.len());
        let label = |want: &str| {
            top.rows
                .iter()
                .find(|row| row[2] == Cell::text(want))
                .map(|row| row[3].clone())
        };
        assert_eq!(label("Grand total"), Some(Cell::Money(r.grand_total)));
        assert_eq!(label("Net total"), Some(Cell::Money(r.net_total)));
        assert_eq!(
            label("Total above-the-line"),
            Some(Cell::Money(r.total_atl))
        );
        let cats: Cents = top
            .rows
            .iter()
            .filter(|row| row[0] == Cell::text("Category"))
            .map(|row| match row[3] {
                Cell::Money(c) => c,
                _ => 0,
            })
            .sum();
        assert_eq!(cats, cat_total);

        // Setup tables: one row per entry.
        assert_eq!(wb.sheet("Globals").unwrap().rows.len(), b.globals.len());
        assert_eq!(wb.sheet("Fringes").unwrap().rows.len(), b.fringes.len());
        assert_eq!(wb.sheet("Groups").unwrap().rows.len(), b.groups.len());
        assert_eq!(wb.sheet("Units").unwrap().rows.len(), b.units.len());
        assert_eq!(
            wb.sheet("Currencies").unwrap().rows.len(),
            b.currencies.len()
        );
        assert!(wb.sheet("Sets").unwrap().rows.len() >= b.sets.len());
        assert!(wb.sheet("Locations").unwrap().rows.len() >= b.locations.len());
        let fringes = wb.sheet("Fringes").unwrap();
        let fringe_dollars: Cents = r.fringes.values().map(|f| f.total).sum();
        assert_eq!(money_sum(fringes, "Total"), fringe_dollars);
    }
}

#[test]
fn detail_values_reproduce_the_engine() {
    let b = sample();
    let r = report(&b);
    let wb = workbook(&b, &r).unwrap();
    let detail = wb.sheet("Detail").unwrap();
    let rows: Vec<_> = r
        .categories
        .iter()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.rows)
        .collect();
    let [kind, amount, x, rate, x4, total] =
        ["Kind", "Amount", "X", "Rate", "4X", "Total"].map(|t| col(detail, t));
    let num = |c: &Cell, blank: Dec| match c {
        Cell::Number(d) => *d,
        Cell::Empty => blank,
        other => panic!("not a number: {other:?}"),
    };
    let (mut lines, mut adjustments) = (0, 0);
    for (row, rr) in detail.rows.iter().zip(rows) {
        assert_eq!(row[total], {
            if rr.kind.is_costed() {
                Cell::Money(rr.total)
            } else {
                Cell::Empty
            }
        });
        let q = num(&row[amount], Dec::ZERO)
            .checked_mul(num(&row[x], Dec::ONE))
            .and_then(|q| q.checked_mul(num(&row[x4], Dec::ONE)));
        if row[kind] == Cell::text("Line") {
            assert_eq!(q, Some(rr.quantity));
            let local = Exact::dec(rr.quantity).checked_mul(num(&row[rate], Dec::ZERO));
            if rr.included {
                assert_eq!(local.and_then(Exact::to_cents), Some(rr.local_total));
            }
            lines += 1;
        } else if row[kind] == Cell::text("Adjustment") && rr.included {
            assert_eq!(q, Some(rr.quantity));
            let Cell::Money(base) = row[rate] else {
                panic!("adjustment rate is money")
            };
            assert_eq!(mul_cents(base, rr.quantity), Some(rr.total));
            adjustments += 1;
        }
    }
    assert!(
        lines > 100 && adjustments > 0,
        "{lines} lines, {adjustments} adjustments"
    );
}

/// RFC 4180 fields, record by record.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            ',' => record.push(std::mem::take(&mut field)),
            '\r' => {
                assert_eq!(chars.next(), Some('\n'), "CR without LF");
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            c => field.push(c),
        }
    }
    assert!(
        field.is_empty() && record.is_empty(),
        "last record not ended"
    );
    records
}

fn field(c: &Cell) -> String {
    match c {
        Cell::Empty => String::new(),
        Cell::Text(s) => csv::neutralize(s),
        Cell::Money(m) => money_text(*m),
        Cell::Number(d) => d.to_string(),
        Cell::Int(n) => n.to_string(),
    }
}

fn with_notes() -> Budget {
    let mut b = sample();
    // Text that needs quoting, escaping and neutralizing.
    let d = &mut b
        .categories_mut()
        .next()
        .unwrap()
        .accounts
        .first_mut()
        .unwrap()
        .details[0];
    d.description = "=HYPERLINK(\"x\") , \"quoted\"".into();
    d.notes = vec![fwb_core::Note::plain(
        1,
        "first line\r\nsecond, line\tTabbed \u{e9}",
    )];
    b
}

#[test]
fn csv_files_read_back_field_for_field() {
    let b = with_notes();
    let wb = workbook(&b, &report(&b)).unwrap();
    let files = render(
        &wb,
        Format::Csv,
        Path::new("/out/Film.csv"),
        Options { bom: true },
    )
    .unwrap();
    assert_eq!(files.len(), SHEET_NAMES.len());
    for (f, s) in files.iter().zip(&wb.sheets) {
        assert_eq!(
            f.path,
            Path::new(&format!("/out/Film - {}.csv", s.name)).to_path_buf()
        );
        let text = std::str::from_utf8(&f.bytes).unwrap();
        let text = text.strip_prefix('\u{feff}').expect("BOM first");
        let records = parse_csv(text);
        assert_eq!(records.len(), s.rows.len() + 1, "{}", s.name);
        assert_eq!(records[0], s.columns);
        for (rec, row) in records[1..].iter().zip(&s.rows) {
            let want: Vec<String> = row.iter().map(field).collect();
            assert_eq!(rec, &want);
        }
    }
}

#[test]
fn tab_delimited_is_the_detail_one_line_per_row() {
    let b = with_notes();
    let wb = workbook(&b, &report(&b)).unwrap();
    let files = render(
        &wb,
        Format::Tsv,
        Path::new("/out/Film.txt"),
        Options::default(),
    )
    .unwrap();
    assert_eq!(files.len(), 1);
    let text = String::from_utf8(files[0].bytes.clone()).unwrap();
    assert!(!text.starts_with('\u{feff}'));
    let detail = wb.sheet("Detail").unwrap();
    let lines: Vec<&str> = text.split("\r\n").collect();
    assert_eq!(lines.last(), Some(&""));
    assert_eq!(lines.len() - 1, detail.rows.len() + 1);
    assert!(!text.replace("\r\n", "").contains(['\r', '\n']));
    for line in &lines[..lines.len() - 1] {
        assert_eq!(line.split('\t').count(), detail.columns.len());
    }
    assert!(lines[1].contains("first line second, line Tabbed \u{e9}"));
    assert!(lines[1].contains("'=HYPERLINK(\"x\") , \"quoted\""));
}

#[test]
fn workbook_is_a_valid_package_with_every_row() {
    let b = with_notes();
    let wb: Workbook = workbook(&b, &report(&b)).unwrap();
    let files = render(
        &wb,
        Format::Xlsx,
        Path::new("/out/Film.xlsx"),
        Options::default(),
    )
    .unwrap();
    assert_eq!(files.len(), 1);
    let bytes = &files[0].bytes;
    xlsx::verify(bytes, wb.sheets.len()).unwrap();
    let parts = export::zip::read_entries(bytes).unwrap();
    for (i, s) in wb.sheets.iter().enumerate() {
        let name = format!("xl/worksheets/sheet{}.xml", i + 1);
        let xml = parts.iter().find(|p| p.name == name).unwrap();
        let xml = std::str::from_utf8(&xml.data).unwrap();
        assert_eq!(xml.matches("<row ").count(), s.rows.len() + 1, "{}", s.name);
    }
}

#[test]
fn a_stale_report_is_refused() {
    let b = sample();
    let r = report(&starter::budget());
    assert!(workbook(&b, &r).is_err());
    assert!(export::workbook_of(&b).is_ok());
}
