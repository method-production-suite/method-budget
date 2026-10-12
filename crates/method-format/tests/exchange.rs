//! Importing whole budgets from another program's budget exports (roadmap
//! 1.16), against reduced copies of real exports (tests/fixtures/exchange:
//! the structure and figures of exports from the 2026-10-10 and 2026-10-11
//! parity runs, every name made up). Four budgets come in every layout
//! (`<name>.json`, `-xmladv.xml`, `-xmlbasic.xml`, `-excel.xlsx`,
//! `-tab.txt`), renamed alike so the layouts stay twins.
//!
//! Each fixture imports and its totals, worked out by our engine, match the
//! export's own figures line by line, account by account, category by
//! category and in total; what had to be worked out (posting mode,
//! cutoffs, ranges, exchange rates, unit hours) comes out as the run log
//! recorded it; and each layout imports as its JSON twin does, but for what
//! the layout leaves out.

use std::path::PathBuf;

use method_core::{
    recalc, Budget, Cents, FringeKind, FringePosting, InfoValue, RecalcOptions, RowKind,
    TopsheetRow, TrailingFringes,
};
use method_format::import::exchange::{
    self, build, detect, json, reconcile, BuildOptions, ExportFormat, Imported, Level, NoteKind,
    Reconciliation,
};

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/exchange")
        .join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn import(name: &str) -> (Imported, Reconciliation) {
    exchange::import(&fixture(name), name, None).unwrap_or_else(|e| panic!("{name}: {e}"))
}

const FIXTURES: [&str; 25] = [
    "adjustments.json",
    "by-account.json",
    "by-budget-credits.json",
    "by-budget-credits-excel.xlsx",
    "by-budget-credits-xmladv.xml",
    "by-budget-credits-xmlbasic.xml",
    "by-category-charges.json",
    "by-category-charges-excel.xlsx",
    "by-category-charges-tab.txt",
    "by-category-charges-xmladv.xml",
    "by-category-charges-xmlbasic.xml",
    "by-category-levels.json",
    "by-level.json",
    "crew-ranges.json",
    "cutoffs-ranges.json",
    "cutoffs-ranges-excel.xlsx",
    "cutoffs-ranges-xmladv.xml",
    "flat-units.json",
    "globals.json",
    "globals-excel.xlsx",
    "globals-xmladv.xml",
    "globals-xmlbasic.xml",
    "originals.json",
    "sets-locations.json",
    "totals-currency.json",
];

fn lines<'a>(b: &'a Budget, account: &str) -> Vec<&'a method_core::Detail> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .find(|a| a.code == account)
        .unwrap_or_else(|| panic!("no account {account}"))
        .details
        .iter()
        .collect()
}

fn cutoff(b: &Budget, fringe: &str) -> Option<Cents> {
    b.fringes[fringe].kind.cutoff()
}

fn has_note(im: &Imported, kind: NoteKind, text: &str) -> bool {
    im.notes
        .iter()
        .any(|n| n.kind == kind && n.text.contains(text))
}

#[test]
fn every_fixture_comes_to_the_exports_figures() {
    for name in FIXTURES {
        let (im, rec) = import(name);
        assert!(
            rec.matches(),
            "{name}: {:?} {:#?}",
            rec.calc_error,
            rec.differences
        );
        assert!(rec.tallies[&Level::Line].compared > 0, "{name}");
        if rec.places == 2 {
            assert_eq!(rec.grand, Some(rec.export_grand), "{name}");
            assert_eq!(rec.net, Some(rec.export_net), "{name}");
        }
        // The budget saves and reads back as it is.
        let text = method_format::to_string(&im.budget);
        let back = method_format::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back, im.budget, "{name}");
        assert!(recalc(&back, &RecalcOptions::default()).is_ok(), "{name}");
    }
}

#[test]
fn fringes_by_budget_with_credits() {
    // ZTEST10: grand 8,800.00, credits 3,349.53, net 5,450.47.
    let (im, rec) = import("by-budget-credits.json");
    let b = &im.budget;
    assert_eq!(b.fringe_posting, FringePosting::Budget);
    assert_eq!(rec.export_grand, 880_000);
    assert_eq!(rec.export_net, 545_047);
    assert_eq!(rec.net, Some(545_047));
    assert_eq!(b.credits.len(), 6);
    assert_eq!(rec.tallies[&Level::Credit].compared, 6);
    assert!(matches!(
        &b.fringes["QFDAY"].kind,
        FringeKind::Flat { unit: Some(u), .. } if u == "Day"
    ));
    assert_eq!(b.fringes["QFDAY"].rate.0, "25");
    assert_eq!(b.fringes["QP10N"].rate.0, "10");
    assert!(b.groups.contains_key("QCRED") && b.groups.contains_key("QCRED2"));
    // Empty entry rows are passed over: three lines, no blank ones.
    assert_eq!(im.counts.lines, 3);
    assert!(has_note(
        &im,
        NoteKind::Lost,
        "Credits come in as fixed amounts"
    ));
    assert!(has_note(&im, NoteKind::Lost, "Production totals"));
}

#[test]
fn fringes_by_category_with_charges() {
    // ZTEST09: the program shows a grand total of 41,389.91, but the export
    // writes the 100.005 charge as 100.00, so its own figures come to
    // 41,389.90, and so does the import.
    let (im, rec) = import("by-category-charges.json");
    let b = &im.budget;
    assert_eq!(
        b.fringe_posting,
        FringePosting::Category {
            suffix: "99".into()
        }
    );
    assert_eq!(rec.export_grand, 4_138_990);
    assert_eq!(rec.grand, Some(4_138_990));
    assert_eq!(b.charges.len(), 9);
    assert_eq!(rec.tallies[&Level::FringeAccount].compared, 3);
    // The fringe accounts are worked out, not imported as accounts.
    assert!(b
        .categories()
        .flat_map(|c| &c.accounts)
        .all(|a| !a.code.ends_with("99")));
    assert_eq!(b.account_coding.category_width, 2);
    assert_eq!(b.account_coding.separator, None);
    assert!(has_note(&im, NoteKind::Check, "QPCT5 is 5%"));
}

#[test]
fn fringes_by_category_with_a_trailing_category() {
    // ZTEST06 posting by category: grand 34,125.00.
    let (im, rec) = import("by-category-levels.json");
    assert!(matches!(
        im.budget.fringe_posting,
        FringePosting::Category { .. }
    ));
    assert_eq!(rec.grand, Some(3_412_500));
}

#[test]
fn fringes_by_account() {
    // ZTEST06 V06-1: a Total Fringes row closes each account; 34,125.00.
    let (im, rec) = import("by-account.json");
    assert_eq!(im.budget.fringe_posting, FringePosting::Account);
    assert_eq!(rec.grand, Some(3_412_500));
    assert!(rec.tallies[&Level::FringeRow].compared >= 6);
    assert!(im
        .budget
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .all(|d| d.description != "Total Fringes"));
}

#[test]
fn fringes_by_production_level() {
    // ZTEST06 V06-2: a Total Fringes row before each production total, and
    // the trailing category's fringes dropped: grand 34,020.00.
    let (im, rec) = import("by-level.json");
    let b = &im.budget;
    assert_eq!(b.fringe_posting, FringePosting::ProductionLevel);
    assert_eq!(b.trailing_level_fringes, TrailingFringes::Drop);
    let levels = b
        .topsheet
        .iter()
        .filter(|r| matches!(r, TopsheetRow::ProductionTotal(_)))
        .count();
    assert_eq!(levels, 3);
    assert_eq!(rec.grand, Some(3_402_000));
    let r = recalc(b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.total_atl, 1_650_000);
}

#[test]
fn cutoffs_and_ranges_are_worked_out() {
    // ZTEST04 with the P1 range cases: ZP10 has a 1,000.00 cutoff, ZP125
    // 2,222.22, and the ranges use them up in line order.
    let (im, rec) = import("cutoffs-ranges.json");
    let b = &im.budget;
    assert_eq!(cutoff(b, "QP10"), Some(100_000));
    assert_eq!(cutoff(b, "QP125"), Some(222_222));
    assert_eq!(cutoff(b, "QP321"), None);
    assert_eq!(rec.export_grand, 4_205_326);
    // Prep, Shoot and Wrap are one range.
    let range = lines(b, "1102")
        .iter()
        .map(|d| d.fringe_range)
        .collect::<Vec<_>>();
    assert!(
        range[0].is_some() && range.iter().all(|r| *r == range[0]),
        "{range:?}"
    );
    // Two ranges in 1202, not one.
    let r1202: Vec<_> = lines(b, "1202").iter().map(|d| d.fringe_range).collect();
    assert_eq!(r1202[0], r1202[1]);
    assert_eq!(r1202[2], r1202[3]);
    assert_ne!(r1202[0], r1202[2]);
    // Lines that earn on their own stay out of ranges.
    assert!(lines(b, "1101").iter().all(|d| d.fringe_range.is_none()));
    assert!(has_note(&im, NoteKind::Check, "QP125 2,222.22"));
}

#[test]
fn unit_hours_come_from_the_flat_fringes() {
    // ZTEST05: a shift unit of 11 hours and a half-day of 6, worked out
    // from what the flat fringes counted; grand 14,850.21.
    let (im, rec) = import("flat-units.json");
    let b = &im.budget;
    let hours = |u: &str| b.units[u].hours.map(|h| h.to_string());
    assert_eq!(hours("QSHIFT").as_deref(), Some("11"));
    assert_eq!(hours("QHALF").as_deref(), Some("6"));
    assert_eq!(hours("QZERO"), None);
    assert_eq!(hours("Day").as_deref(), Some("12"));
    assert_eq!(b.units["QSHIFT"].plural, "QSHIFTS");
    assert_eq!(rec.grand, Some(1_485_021));
}

#[test]
fn totals_in_another_currency() {
    // ZTEST08 V08-1: base USD, totals shown in EUR at 3.
    let (im, rec) = import("totals-currency.json");
    let b = &im.budget;
    assert_eq!(b.base_currency, "USD");
    assert_eq!(b.totals_currency.as_deref(), Some("EUR"));
    assert_eq!(b.currencies["EUR"].rate.to_string(), "3");
    assert_eq!(b.currencies["CAD"].rate.to_string(), "1.234565");
    assert_eq!(rec.currency, "EUR");
    assert_eq!(rec.export_grand, 4_925_340);
    // The cutoff, worked out in EUR, is 2,222.22 in USD.
    assert_eq!(cutoff(b, "QP125"), Some(222_222));
}

#[test]
fn adjustment_rows_are_recognized() {
    // ZTEST03: six adjustment rows, one of them fringed; 6,551.85.
    let (im, rec) = import("adjustments.json");
    let b = &im.budget;
    assert_eq!(im.counts.adjustments, 6);
    let adj: Vec<_> = lines(b, "1101")
        .into_iter()
        .filter(|d| d.kind == RowKind::Adjustment)
        .collect();
    assert_eq!(adj.len(), 6);
    assert_eq!(adj[0].amount.0, "0.1");
    assert!(adj[0].rate.is_blank() && adj[0].currency.is_none());
    assert!(!adj[0].fringes.is_empty());
    assert_eq!(adj[5].x.0, "2");
    assert_eq!(rec.grand, Some(655_185));
}

#[test]
fn globals_are_named_in_notes() {
    // ZTEST02: values come over; each line names the globals it used.
    let (im, rec) = import("globals.json");
    let b = &im.budget;
    assert!(b.globals.is_empty());
    let l = lines(b, "1101");
    assert!(l[0].notes[0].text.contains("QG_B"), "{:?}", l[0].notes);
    assert_eq!(l[1].amount.0, "5.666666666666667");
    assert_eq!(rec.grand, Some(8_500_163_101));
    assert!(has_note(&im, NoteKind::Lost, "16 globals"));
}

#[test]
fn sets_locations_and_groups() {
    let (im, _) = import("sets-locations.json");
    let b = &im.budget;
    assert_eq!(b.sets.keys().collect::<Vec<_>>(), ["QS1"]);
    assert_eq!(b.locations.keys().collect::<Vec<_>>(), ["QL1", "QL2"]);
    assert!(b.groups.values().all(|g| g.in_budget_total));
    let r = recalc(b, &RecalcOptions::default()).unwrap();
    // ZTEST07: S1 5,500.00 with fringes, L1 1,600.00, L2 600.00.
    let s1 = &r.sets["QS1"];
    assert_eq!(s1.details_total + s1.fringe_total, 550_000);
    let l1 = &r.locations["QL1"];
    assert_eq!(l1.details_total + l1.fringe_total, 160_000);
}

#[test]
fn originals_become_the_baseline() {
    // ZTEST12 S5b: originals 3,500 / 3,700 / fringes 100, grand 7,300.
    let (im, _) = import("originals.json");
    let bl = im.budget.baseline.as_ref().expect("a baseline");
    assert_eq!(bl.categories["1100"], 350_000);
    assert_eq!(bl.categories["2100"], 370_000);
    assert_eq!(bl.accounts["1101"], 350_000);
    assert_eq!(bl.grand_total, 730_000);
}

const SMALL: &str = r#"{"metadata": {"budgetdata": {"BudgetName": "Small", "BaseCurrency": "CAD", "ConvCurrency": "CAD", "FringeCount": 0},
  "defaultfields": {"Title": "Night Shift", "Season": "2", "StageShootDays": "12", "StartProductionDate": "2027-03-01", "Status": "Draft"},
  "customfields": [{"name": "Network", "value": "North"}]},
 "categories": [
  {"cID": 0, "cNumber": "10-00", "cDescription": "Story", "cFringe": 0, "cOriginal": 0, "cTotal": 1234.5, "cVariance": 1234.5},
  {"cID": 1, "cNumber": "", "cDescription": "", "cFringe": 0, "cOriginal": 0, "cTotal": 0, "cVariance": 0},
  {"cID": 2, "cNumber": "", "cDescription": "Total Fringes", "cFringe": null, "cOriginal": 0, "cTotal": 0, "cVariance": 0, "fringeDetail": []}],
 "accounts": [
  {"categoryID": 0, "aID": 0, "aNumber": "10-01", "aDescription": "Writers", "aFringe": 0, "aOriginal": 0, "aTotal": 1234.5, "aVariance": 1234.5},
  {"categoryID": 0, "aID": 1, "aNumber": "10-01", "aDescription": "Writers again", "aFringe": 0, "aOriginal": 0, "aTotal": 0, "aVariance": 0}],
 "details": [
  {"accountID": 0, "dDescription": "Draft", "dAmount": 1, "dUnit": "", "dX": 1, "dUnit2": "", "dCurrency": "", "dRate": 1234.5, "dUnit3": "", "dUnit4": "", "dSubtotal": 1234.5, "dFourthMlt": 1, "dFringes": [], "dGroups": [], "dGlobalsUsed": [], "dLocation": "", "dSet": "", "dAggPercent": "", "dAggSumPercent": ""},
  {"accountID": 0, "dDescription": "", "dAmount": 1, "dUnit": "", "dX": 1, "dUnit2": "", "dCurrency": "", "dRate": 0, "dUnit3": "", "dUnit4": "", "dSubtotal": 0, "dFourthMlt": 1, "dFringes": [], "dGroups": [], "dGlobalsUsed": [], "dLocation": "", "dSet": "", "dAggPercent": "", "dAggSumPercent": ""},
  {"accountID": 0, "dDescription": "Polish", "dAmount": 1, "dUnit": "", "dX": 1, "dUnit2": "", "dCurrency": "", "dRate": 0, "dUnit3": "", "dUnit4": "", "dSubtotal": 0, "dFourthMlt": 1, "dFringes": ["NEWFR"], "dGroups": [], "dGlobalsUsed": [], "dLocation": "", "dSet": "", "dAggPercent": "", "dAggSumPercent": ""},
  {"accountID": 0, "dDescription": "", "dAmount": 1, "dUnit": "", "dX": 1, "dUnit2": "", "dCurrency": "", "dRate": 0, "dUnit3": "", "dUnit4": "", "dSubtotal": 0, "dFourthMlt": 1, "dFringes": [], "dGroups": [], "dGlobalsUsed": [], "dLocation": "", "dSet": "", "dAggPercent": "", "dAggSumPercent": ""},
  {"accountID": 7, "dDescription": "Lost", "dAmount": 1, "dUnit": "", "dX": 1, "dUnit2": "", "dCurrency": "", "dRate": 5, "dUnit3": "", "dUnit4": "", "dSubtotal": 5, "dFourthMlt": 1, "dFringes": [], "dGroups": [], "dGlobalsUsed": [], "dLocation": "", "dSet": "", "dAggPercent": "", "dAggSumPercent": ""}]
}"#;

#[test]
fn information_fields_titles_and_odd_rows() {
    assert_eq!(detect(SMALL.as_bytes(), "x.txt"), Some(ExportFormat::Json));
    let x = json::read(SMALL.as_bytes()).unwrap();
    let im = build(
        &x,
        &BuildOptions {
            title: "file".into(),
        },
    );
    let rec = reconcile(&im);
    assert!(rec.matches(), "{:#?}", rec.differences);
    let b = &im.budget;
    assert_eq!(b.title, "Night Shift");
    assert_eq!(b.base_currency, "CAD");
    assert_eq!(b.info["Season"], InfoValue::Text("2".into()));
    assert!(matches!(b.info["Stage shoot days"], InfoValue::Number(_)));
    assert_eq!(
        b.info["Start of production"],
        InfoValue::Date("2027-03-01".into())
    );
    assert_eq!(b.info["Network"], InfoValue::Text("North".into()));
    assert_eq!(b.account_coding.separator, Some('-'));
    // A blank row between lines stays (as an empty line); the one at the
    // end is the entry row and goes.
    let l = lines(b, "10-01");
    assert_eq!(l.len(), 3);
    assert!(l[1].description.is_empty() && l[1].amount.is_blank());
    // A repeated account number gets a new one; a fringe with no figures
    // comes in at 0%; a detail of an unknown account is left out, and said.
    assert!(b
        .categories()
        .flat_map(|c| &c.accounts)
        .any(|a| a.code == "10-01-2"));
    assert_eq!(b.fringes["NEWFR"].rate.0, "0");
    assert!(has_note(&im, NoteKind::Lost, "NEWFR"));
    assert!(has_note(
        &im,
        NoteKind::Lost,
        "naming an account the export doesn't list, left out: 1"
    ));
    assert!(has_note(&im, NoteKind::Check, "used twice"));
}

#[test]
fn files_that_arent_exports_are_refused() {
    let e = exchange::import(b"{\"a\": 1}", "x.json", None).unwrap_err();
    assert!(e.0.contains("categories"), "{e}");
    let e = exchange::import(b"Account\tRate\n", "x.txt", None).unwrap_err();
    assert!(e.0.contains("budget export"), "{e}");
    let e = exchange::import(b"{\"categories\": [", "x.json", None).unwrap_err();
    assert!(e.0.contains("JSON"), "{e}");
    let big = vec![b' '; method_format::import::MAX_FILE_BYTES + 1];
    assert!(exchange::import(&big, "x.json", None).is_err());
}

/// A budget with the title every import gives it, so twins compare.
fn titled(mut b: Budget) -> Budget {
    b.title = "Twin".into();
    b
}

/// Every detail row's content, by account, without the notes.
fn rows(b: &Budget) -> Vec<(String, Vec<String>)> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .map(|a| {
            (
                a.code.clone(),
                a.details
                    .iter()
                    .map(|d| {
                        format!(
                            "{:?} {} {}|{}|{}|{}|{}|{:?} {:?} {:?}",
                            d.kind,
                            d.description,
                            d.amount.0,
                            d.x.0,
                            d.rate.0,
                            d.x4.0,
                            d.unit,
                            d.currency,
                            d.fringes,
                            d.fringe_range
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn every_layout_reads_as_its_json_twin() {
    for stem in [
        "by-budget-credits",
        "by-category-charges",
        "cutoffs-ranges",
        "globals",
    ] {
        let json = titled(import(&format!("{stem}.json")).0.budget);
        let layouts = [
            ("xmladv.xml", ExportFormat::XmlAdvanced),
            ("excel.xlsx", ExportFormat::Excel),
            ("xmlbasic.xml", ExportFormat::XmlBasic),
            ("tab.txt", ExportFormat::Tab),
        ];
        for (suffix, format) in layouts {
            let name = format!("{stem}-{suffix}");
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/exchange")
                .join(&name);
            if !p.exists() {
                continue;
            }
            let bytes = fixture(&name);
            assert_eq!(detect(&bytes, &name), Some(format), "{name}");
            let (im, rec) = import(&name);
            assert!(rec.matches(), "{name}: {:#?}", rec.differences);
            let mut b = titled(im.budget);
            match format {
                // Everything the JSON has.
                ExportFormat::XmlAdvanced => assert_eq!(b, json, "{name}"),
                ExportFormat::Excel => {
                    // No fringe figures: rates from the lines, a cutoff
                    // that gives the same totals (within a few cents of
                    // the JSON's); and the charges unrounded.
                    for (n, f) in &json.fringes {
                        let mine = &b.fringes[n];
                        assert_eq!(mine.rate, f.rate, "{name} {n}");
                        if let (Some(a), Some(c)) = (mine.kind.cutoff(), f.kind.cutoff()) {
                            assert!((a - c).abs() <= 5, "{name} {n}: {a} {c}");
                            b.fringes.insert(n.clone(), f.clone());
                        }
                    }
                    for (mine, theirs) in b.charges.iter_mut().zip(&json.charges) {
                        if mine.name == "QHALF" {
                            // 100.005: the workbook keeps it, the JSON
                            // writes 100.00, the program shows 100.01.
                            assert_eq!(
                                mine.kind,
                                method_core::ChargeKind::Flat {
                                    amount: 10_001,
                                    currency: None
                                }
                            );
                            *mine = theirs.clone();
                        }
                    }
                    assert_eq!(b, json, "{name}");
                }
                // No groups, globals or currencies; whole units; fringes
                // put back on the lines from their percentages.
                ExportFormat::XmlBasic => {
                    assert!(b.groups.is_empty(), "{name}");
                    assert_eq!(rows(&b).len(), rows(&json).len(), "{name}");
                    let strip = |v: Vec<(String, Vec<String>)>| v;
                    assert_eq!(strip(rows(&b)), strip(rows(&json)), "{name}");
                    assert_eq!(b.fringes, json.fringes, "{name}");
                    assert_eq!(b.fringe_posting, json.fringe_posting, "{name}");
                }
                // Totals only: the accounts' totals, to the unit.
                ExportFormat::Tab => {
                    assert!(im
                        .links
                        .lines
                        .iter()
                        .all(|l| l.description.contains("total")));
                    assert_eq!(rec.export_grand, 4_139_000);
                }
                ExportFormat::Json => unreachable!(),
            }
        }
    }
}

/// An invented crew budget with the shapes a real export (a feature's
/// budget, 2026-10-12) brought in wrong: a 0-day premium inside a person's
/// lines, people whose lines carry flat fringes, two people in one account
/// (the second earning less than the full rate on their own cutoff), a
/// fitting that takes a shoot just over the cutoff (invisible in the lines'
/// two-place percentages), a stand-in line of Amount 0 with a flat fringe,
/// a line in `%`, and a charge and a credit with account numbers
/// (decisions.md, entry 67).
#[test]
fn crew_ranges_and_percent_units() {
    let (im, rec) = import("crew-ranges.json");
    assert!(rec.matches(), "{:#?}", rec.differences);
    let b = &im.budget;
    assert_eq!(cutoff(b, "QFUTA"), Some(700_000));
    assert_eq!(cutoff(b, "QSUI"), Some(1_170_000));
    assert_eq!(cutoff(b, "QFICA"), None);
    let ranges = |acct: &str| -> Vec<Option<u32>> {
        lines(b, acct).iter().map(|d| d.fringe_range).collect()
    };
    let one = |v: &[Option<u32>]| v.iter().all(|r| r.is_some() && *r == v[0]);
    // The 0-day premium doesn't split the person's lines, with percent
    // fringes only or flat ones as well.
    assert!(one(&ranges("2101")), "{:?}", ranges("2101"));
    assert!(one(&ranges("2102")), "{:?}", ranges("2102"));
    // Two people: the first's three lines are one range; the second's shoot
    // earns less only by its own cutoff, so it stands alone.
    let pair = ranges("2103");
    assert!(pair[1].is_some() && pair[1] == pair[2] && pair[2] == pair[3]);
    assert_eq!(pair[5], None);
    // The fitting and shoot that only the account's fringes tell apart.
    let fit = ranges("2104");
    assert!(fit[0].is_some() && fit[0] == fit[1], "{fit:?}");
    assert_eq!(fit[2..], [None, None]);
    // 5,250.00 x 3.05% is 160.125 in doubles: 160.13 (the account's
    // fringes match only so).
    let r = recalc(b, &RecalcOptions::default()).unwrap();
    let premium = &r.categories[0].accounts[0].rows[0];
    assert_eq!(premium.fringes["QWC"], 16_013);
    // The stand-in line of Amount 0 and no unit counts no flat fringe.
    let standin = &r.categories[0].accounts[4].rows[0];
    assert_eq!(standin.fringes.get("QPOST").copied().unwrap_or(0), 0);
    // 1.5 % of 5,500,000 is 82,500.00.
    let cover = lines(b, "2201")[0];
    assert_eq!(cover.amount.0, "1.5/100");
    assert_eq!(cover.unit, "");
    assert!(cover.notes.iter().any(|n| n.text.contains("1.5 %")));
    assert!(has_note(&im, NoteKind::Check, "1 line in a percent unit"));
    // A numbered row with no fringe figure and no accounts is a charge or
    // a credit posted to that number, not an empty category.
    assert_eq!(b.categories().count(), 2);
    assert_eq!(b.charges[0].account.as_deref(), Some("9800-00"));
    assert_eq!(b.credits[0].account.as_deref(), Some("9900-01"));
    assert_eq!(rec.export_net, 18_914_318);
}

/// An export whose account totals are their lines rounded to whole units:
/// the import says so, and the cents are the only difference.
#[test]
fn account_totals_in_whole_units_are_pointed_out() {
    let (im, rec) = import("whole-units.json");
    assert!(has_note(
        &im,
        NoteKind::Check,
        "rounds account totals, which this program doesn't, so 1 account"
    ));
    assert_eq!(rec.tallies[&Level::Line].different, 0);
    assert_eq!(rec.tallies[&Level::AccountFringes].different, 0);
    assert_eq!(rec.tallies[&Level::Account].different, 1);
    assert_eq!(
        rec.differences[0].ours - rec.differences[0].export,
        25,
        "{:#?}",
        rec.differences
    );
}

/// The real exports of the parity runs, when the environment points at
/// them (`METHOD_EXPORT_SAMPLES=<dir>:<dir>`, such as
/// imports/mmb-parity/2026-10-10/exports and 2026-10-11/exports; they are
/// not in the repository). Each comes to its own figures but the one shown
/// in pounds, where the program converts its adjustment row in pounds.
#[test]
fn local_sample_exports_reconcile() {
    let Ok(dirs) = std::env::var("METHOD_EXPORT_SAMPLES") else {
        return;
    };
    let mut files = Vec::new();
    for dir in dirs.split(':') {
        let mut todo = vec![PathBuf::from(dir)];
        while let Some(d) = todo.pop() {
            for f in std::fs::read_dir(&d).unwrap() {
                let f = f.unwrap().path();
                if f.is_dir() {
                    todo.push(f);
                } else if f.extension().is_some_and(|e| {
                    ["json", "xml", "xlsx", "txt"].contains(&e.to_str().unwrap_or(""))
                }) {
                    files.push(f);
                }
            }
        }
    }
    files.sort();
    assert!(!files.is_empty(), "no exports under {dirs}");
    let mut figures = std::collections::BTreeMap::new();
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        let (_, rec) = exchange::import(&std::fs::read(f).unwrap(), &name, None).unwrap();
        if name != "ZTEST08-V08-2.json" {
            assert!(rec.matches(), "{name}: {:#?}", rec.differences);
        }
        figures.insert(name, (rec.grand, rec.net));
    }
    assert_eq!(
        figures["ZTEST10-main-bybudget.json"].1,
        Some(545_047),
        "ZTEST10 net"
    );
    assert_eq!(figures["ZTEST09-main-full.json"].0, Some(4_138_990));
    assert_eq!(figures["ZTEST06-main.json"].0, Some(3_412_500));
}
