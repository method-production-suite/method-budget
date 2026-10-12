//! Seeded random round trips over every field and enum variant.
//!
//! Uses the same small xorshift generator as method-core's property tests (copied,
//! since test crates cannot share code). Each failure prints its seed.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use method_core::coding::AccountCoding;
use method_core::*;

const CASES: u64 = 600;

/// xorshift64*: tiny, deterministic, good enough for test data.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }

    /// Plain names plus strings that stress JSON escaping and whitespace.
    fn text(&mut self) -> String {
        self.pick(&[
            "",
            " ",
            "  padded  ",
            "A",
            "Key grip",
            "SHOOT_DAYS * 2",
            "quote \" and back \\ slash",
            "tab\tnewline\nreturn\r",
            "nul \u{0} bell \u{7} del \u{7f}",
            "caf\u{e9} \u{65e5}\u{672c} \u{2603}",
            "\u{1F3AC} clapper",
            "</script>",
            "1100",
            "\u{2028}line sep",
        ])
        .to_string()
    }

    fn opt_text(&mut self) -> Option<String> {
        self.chance(50).then(|| self.text())
    }

    fn texts(&mut self) -> BTreeSet<String> {
        (0..self.below(4)).map(|_| self.text()).collect()
    }

    fn cents(&mut self) -> Cents {
        match self.below(6) {
            0 => 0,
            1 => i64::MIN,
            2 => i64::MAX,
            3 => -(self.below(1_000_000) as i64),
            _ => self.next() as i64,
        }
    }

    fn dec(&mut self) -> Dec {
        match self.below(8) {
            0 => Dec::ZERO,
            1 => Dec::from_int(i64::MAX),
            2 => Dec::from_int(i64::MIN),
            3 => "-0.000000001".parse().unwrap(),
            _ => {
                let sign = if self.chance(30) { "-" } else { "" };
                let int = self.next() % 10u64.pow(self.below(13) as u32 + 1);
                let places = self.below(10) as usize;
                let frac = self.next() % 10u64.pow(places as u32);
                let s = if places == 0 {
                    format!("{sign}{int}")
                } else {
                    format!("{sign}{int}.{frac:0places$}")
                };
                s.parse().unwrap()
            }
        }
    }

    fn formula(&mut self) -> Formula {
        match self.below(4) {
            0 => Formula::default(),
            1 => self.dec().into(),
            _ => self.text().into(),
        }
    }

    fn map<V>(&mut self, mut f: impl FnMut(&mut Rng) -> V) -> BTreeMap<String, V> {
        (0..self.below(4)).map(|_| (self.text(), f(self))).collect()
    }
}

fn gen_detail(rng: &mut Rng, id: RowId) -> Detail {
    Detail {
        id,
        kind: match rng.below(4) {
            0 => RowKind::Line,
            1 => RowKind::Subtotal,
            2 => RowKind::Separator,
            _ => RowKind::Adjustment,
        },
        description: rng.text(),
        amount: rng.formula(),
        unit: rng.text(),
        x: rng.formula(),
        unit2: rng.text(),
        rate: rng.formula(),
        unit3: rng.text(),
        x4: rng.formula(),
        unit4: rng.text(),
        currency: rng.opt_text(),
        fringes: rng.texts(),
        groups: rng.texts(),
        location: rng.opt_text(),
        set: rng.opt_text(),
        fringe_range: match rng.below(4) {
            0 => Some(rng.next() as u32),
            1 => Some(u32::MAX),
            _ => None,
        },
        bold: rng.chance(30),
        italic: rng.chance(30),
        indent: match rng.below(4) {
            0 => u8::MAX,
            1 => rng.below(5) as u8,
            _ => 0,
        },
        color: rng
            .chance(30)
            .then(|| rng.pick(&["#a0b1c2", "", "red"]).into()),
        notes: gen_notes(rng),
    }
}

/// Dated notes, as a row, account or category keeps them.
fn gen_notes(rng: &mut Rng) -> Vec<Note> {
    (0..rng.below(4).saturating_sub(1))
        .map(|_| Note {
            id: if rng.chance(5) {
                u64::MAX
            } else {
                rng.below(9)
            },
            text: rng.text(),
            created: rng.pick(&["2026-10-09T14:30:00Z", "", "not a time"]).into(),
            edited: rng
                .chance(30)
                .then(|| rng.pick(&["2026-10-10T09:00:00Z", ""]).into()),
            author: rng.opt_text(),
            done: rng.chance(30),
        })
        .collect()
}

/// A hand-made order for some of the budget's setup tables.
fn gen_order(rng: &mut Rng, b: &mut Budget) {
    for &kind in SetupKind::ALL {
        if !rng.chance(40) {
            continue;
        }
        let mut names = method_core::order::names(b, kind);
        for i in (1..names.len()).rev() {
            names.swap(i, rng.below(i as u64 + 1) as usize);
        }
        if rng.chance(20) {
            names.push("not an item".into());
        }
        method_core::order::set_order(b, kind, &names);
    }
}

fn gen_charge(rng: &mut Rng) -> Charge {
    Charge {
        name: rng.text(),
        account: rng.opt_text(),
        applied: rng.chance(50),
        kind: if rng.chance(50) {
            ChargeKind::Percent(rng.dec())
        } else {
            ChargeKind::Flat {
                amount: rng.cents(),
                currency: rng.opt_text(),
            }
        },
        excluded_amounts: (0..rng.below(3))
            .map(|_| (rng.text(), rng.cents()))
            .collect(),
        excluded_groups: rng.texts(),
        excluded_categories: rng.texts(),
        excluded_accounts: rng.texts(),
        label_percent: rng.chance(50),
        label_excluded: rng.chance(50),
    }
}

fn gen_credit(rng: &mut Rng) -> Credit {
    Credit {
        name: rng.text(),
        account: rng.opt_text(),
        applied: rng.chance(50),
        cap: rng.chance(50).then(|| rng.cents()),
        discount_percent: rng.dec(),
        excluded_fringes: rng.texts(),
        rows: (0..rng.below(4))
            .map(|_| CreditRow {
                description: rng.text(),
                source: match rng.below(3) {
                    0 => CreditSource::Flat(rng.cents()),
                    1 => CreditSource::Group { group: rng.text() },
                    _ => CreditSource::Category {
                        category: rng.text(),
                    },
                },
                percent: rng.dec(),
                cap: match rng.below(3) {
                    0 => CreditCap::None,
                    1 => CreditCap::QualifiedSpend(rng.cents()),
                    _ => CreditCap::Credit(rng.cents()),
                },
            })
            .collect(),
    }
}

/// A budget with no versions.
fn gen_content(rng: &mut Rng) -> Budget {
    let mut b = Budget::new(rng.text(), rng.text());
    b.totals_currency = rng.opt_text();
    b.info = rng.map(|r| match r.below(3) {
        0 => InfoValue::Text(r.text()),
        1 => InfoValue::Number(r.dec()),
        _ => InfoValue::Date(r.pick(&["2027-01-31", "", "not a date"]).into()),
    });
    b.currencies = rng.map(|r| Currency {
        description: r.text(),
        symbol: r.text(),
        rate: r.dec(),
    });
    b.globals = rng.map(Rng::formula);
    // Info for some of the globals (only non-default info is kept in a file).
    let names: Vec<String> = b.globals.keys().cloned().collect();
    for name in names {
        if !rng.chance(50) {
            continue;
        }
        let g = GlobalInfo {
            description: rng.text(),
            unit: rng.text(),
            decimals: rng.chance(50).then(|| rng.below(9) as u8),
            show_by_name: rng.chance(50),
            folder: rng.text(),
        };
        if !g.is_default() {
            b.global_info.insert(name, g);
        }
    }
    if rng.chance(70) {
        b.units = rng.map(|r| UnitDef {
            plural: r.text(),
            description: r.text(),
            hours: r.chance(50).then(|| r.dec()),
        });
    }
    b.fringes = rng.map(|r| Fringe {
        short_code: r.opt_text(),
        description: r.text(),
        rate: r.formula(),
        kind: if r.chance(50) {
            FringeKind::Percent {
                cutoff: r.chance(50).then(|| r.cents()),
            }
        } else {
            FringeKind::Flat {
                unit: r.opt_text(),
                basis: FlatBasis::ALL[r.below(3) as usize],
                max_hours_per_day: r.chance(40).then(|| r.dec()),
                cutoff: r.chance(20).then(|| r.cents()),
            }
        },
    });
    b.fringe_posting = match rng.below(5) {
        0 => FringePosting::Budget,
        1 => FringePosting::ProductionLevel,
        2 => FringePosting::Account,
        3 => FringePosting::Category { suffix: rng.text() },
        _ => FringePosting::default(),
    };
    if rng.chance(60) {
        b.account_coding = AccountCoding {
            category_width: rng.below(256) as u8,
            account_width: rng.below(7) as u8,
            sub_width: rng.below(7) as u8,
            set_width: rng.below(3) as u8,
            separator: match rng.below(6) {
                0 => None,
                1 => Some('"'),
                2 => Some('\u{2603}'),
                3 => Some('\n'),
                _ => Some(
                    rng.pick(&[" ", "-", ".", ",", "/", ":", ";"])
                        .chars()
                        .next()
                        .unwrap(),
                ),
            },
        };
    }
    if rng.chance(50) {
        b.new_line_defaults = NewLineDefaults {
            fringes: rng.chance(50),
            groups: rng.chance(50),
            set: rng.chance(50),
            location: rng.chance(50),
            currency: rng.chance(50),
        };
    }
    if rng.chance(30) {
        b.arithmetic = Arithmetic::Exact;
    }
    if rng.chance(30) {
        b.trailing_level_fringes = TrailingFringes::Drop;
    }
    if rng.chance(40) {
        let pick = |r: &mut Rng| {
            if r.chance(50) {
                Inclusion::Include
            } else {
                Inclusion::Exclude
            }
        };
        b.group_rules = GroupRules {
            ungrouped_lines: pick(rng),
            conflict: pick(rng),
            fringes_in_totals: pick(rng),
        };
    }
    b.groups = rng.map(|r| Group {
        short_code: r.opt_text(),
        description: r.text(),
        in_budget_total: r.chance(50),
        color: r.chance(40).then(|| r.pick(&["#00ff7f", ""]).into()),
    });
    b.sets = rng.map(|r| SetDef {
        description: r.text(),
    });
    b.locations = rng.map(|r| Location {
        description: r.text(),
        city: r.text(),
        region: r.text(),
        country: r.text(),
    });

    let mut id = rng.below(1_000);
    for _ in 0..rng.below(6) {
        if rng.chance(25) {
            b.topsheet.push(level(&rng.text()));
            continue;
        }
        let accounts = (0..rng.below(4))
            .map(|_| {
                let details = (0..rng.below(6))
                    .map(|_| {
                        id += 1 + rng.below(3);
                        let row_id = if rng.chance(3) { u64::MAX } else { id };
                        gen_detail(rng, row_id)
                    })
                    .collect();
                let mut a = account(&rng.text(), &rng.text(), details);
                a.notes = gen_notes(rng);
                a
            })
            .collect();
        let mut row = category(&rng.text(), &rng.text(), accounts);
        if let TopsheetRow::Category(c) = &mut row {
            c.notes = gen_notes(rng);
        }
        b.topsheet.push(row);
    }
    b.charges = (0..rng.below(4)).map(|_| gen_charge(rng)).collect();
    b.credits = (0..rng.below(3)).map(|_| gen_credit(rng)).collect();
    if rng.chance(60) {
        b.baseline = Some(Baseline {
            rows: (0..rng.below(5))
                .map(|_| (rng.next(), rng.cents()))
                .collect(),
            accounts: rng.map(Rng::cents),
            categories: rng.map(Rng::cents),
            grand_total: rng.cents(),
            net_total: rng.cents(),
        });
    }
    b
}

/// Tag presets and line blocks (top level only: versions never hold them).
fn gen_tool_presets(rng: &mut Rng, b: &mut Budget) {
    b.tag_presets = (0..rng.below(3))
        .map(|_| TagPreset {
            name: rng.text(),
            fringes: rng.texts(),
            groups: rng.texts(),
            set: rng.opt_text(),
            location: rng.opt_text(),
            currency: rng.opt_text(),
        })
        .collect();
    b.line_presets = (0..rng.below(3))
        .map(|_| {
            // Block rows have no id of their own: 0 in memory.
            let rows = (0..rng.below(4)).map(|_| gen_detail(rng, 0)).collect();
            let setup = if rng.chance(30) {
                SetupItems::default()
            } else {
                let from = gen_content(rng);
                SetupItems {
                    fringes: from.fringes,
                    groups: from.groups,
                    globals: from.globals,
                    units: if rng.chance(50) {
                        from.units
                    } else {
                        BTreeMap::new()
                    },
                    sets: from.sets,
                    locations: from.locations,
                    currencies: from.currencies,
                }
            };
            LinePreset {
                name: rng.text(),
                rows,
                setup,
            }
        })
        .collect();
}

/// Display settings (top level only: versions never hold them).
fn gen_display(rng: &mut Rng) -> DisplayPrefs {
    if rng.chance(40) {
        return DisplayPrefs::default();
    }
    DisplayPrefs {
        rate_cutoff: rng.chance(50).then(|| rng.dec()),
        rate_decimals_below: rng.chance(50).then(|| rng.below(9) as u8),
        rate_decimals_above: rng.chance(50).then(|| rng.below(9) as u8),
        rate_global_decimals: rng.chance(50),
        summary_rows: rng.chance(50),
        net_total: rng.chance(50),
    }
}

/// Actuals: transactions with every optional field, imports and every
/// estimate-to-complete method.
fn gen_actuals(rng: &mut Rng) -> method_core::actuals::Actuals {
    use method_core::actuals::{Actuals, EtcMethod, ImportBatch, Transaction};
    let mut a = Actuals::default();
    for i in 0..rng.below(4) {
        a.transactions.push(std::sync::Arc::new(Transaction {
            id: if rng.chance(5) { u64::MAX } else { i },
            date: rng.pick(&["", "2026-09-30", "not a date"]).into(),
            account: rng.text(),
            line: rng.chance(50).then(|| rng.below(100)),
            tags: rng.texts(),
            vendor: rng.text(),
            description: rng.text(),
            reference: rng.text(),
            amount: rng.cents(),
            batch: rng.chance(60).then(|| rng.below(4)),
        }));
    }
    for i in 0..rng.below(3) {
        a.batches.push(ImportBatch {
            id: i,
            file: rng.text(),
            imported: rng.pick(&["", "2026-10-09T14:30:00-07:00"]).into(),
            rows: rng.below(1000),
        });
    }
    for _ in 0..rng.below(4) {
        let m = match rng.below(3) {
            0 => EtcMethod::BudgetMinusActual,
            1 => EtcMethod::Manual(rng.cents()),
            _ => EtcMethod::PercentComplete(rng.dec()),
        };
        a.etc.insert(rng.text(), m);
    }
    a
}

/// A budget whose versions hold other random content.
fn gen_budget(rng: &mut Rng) -> Budget {
    let mut b = gen_content(rng);
    gen_tool_presets(rng, &mut b);
    b.display = gen_display(rng);
    gen_order(rng, &mut b);
    if rng.chance(40) {
        b.actuals = std::sync::Arc::new(gen_actuals(rng));
    }
    for _ in 0..rng.below(3) {
        let (content, _) = gen_content(rng).into_parts();
        b.versions.push(Version {
            id: if rng.chance(5) {
                u64::MAX
            } else {
                rng.below(100)
            },
            name: rng.text(),
            created: rng.pick(&["2026-10-09T14:30:00Z", "", "not a time"]).into(),
            note: rng.opt_text(),
            budget: std::sync::Arc::new(content),
        });
    }
    b
}

#[test]
fn random_budgets_round_trip_exactly() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        let text = method_format::to_string(&b);
        let back =
            method_format::from_str(&text).unwrap_or_else(|e| panic!("seed {seed}: {e}\n{text}"));
        assert_eq!(back, b, "seed {seed}");
        assert_eq!(method_format::to_string(&back), text, "seed {seed}");
    }
}

#[test]
fn output_is_deterministic() {
    for seed in 1..=CASES / 4 {
        let a = method_format::to_string(&gen_budget(&mut Rng(seed)));
        let b = method_format::to_string(&gen_budget(&mut Rng(seed)));
        assert_eq!(a, b, "seed {seed}");
        assert!(a.ends_with("\n") && !a.contains("\r\n"), "seed {seed}");
    }
}

/// Guards the generator itself: across all seeds, every variant and every
/// optional field shows up in some file.
#[test]
fn generator_covers_every_variant_and_field() {
    let mut all = String::new();
    let (mut group_colors, mut row_colors) = (0, 0);
    // Tag preset parts and line blocks share field names with the budget:
    // count them on their own.
    let mut tag_parts = [0usize; 5];
    let (mut block_rows, mut block_setups, mut block_units) = (0, 0, 0);
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        for p in &b.tag_presets {
            for (n, has) in tag_parts.iter_mut().zip([
                !p.fringes.is_empty(),
                !p.groups.is_empty(),
                p.set.is_some(),
                p.location.is_some(),
                p.currency.is_some(),
            ]) {
                *n += usize::from(has);
            }
        }
        for p in &b.line_presets {
            block_rows += p.rows.len();
            block_setups += usize::from(!p.setup.is_empty());
            block_units += usize::from(!p.setup.units.is_empty());
        }
        group_colors += b.groups.values().filter(|g| g.color.is_some()).count();
        row_colors += b
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .filter(|d| d.color.is_some())
            .count();
        all.push_str(&method_format::to_string(&b));
    }
    // `color` is a field of both groups and detail rows: check each.
    assert!(group_colors > 0 && row_colors > 0);
    assert!(tag_parts.iter().all(|&n| n > 0), "{tag_parts:?}");
    assert!(block_rows > 0 && block_setups > 0 && block_units > 0);
    let kinds = [
        "text",
        "number",
        "date",
        "percent",
        "flat",
        "budget",
        "production_level",
        "category",
        "account",
        "production_total",
        "subtotal",
        "adjustment",
        "separator",
        "group",
        "qualified_spend",
        "credit",
        "budget_minus_actual",
        "manual",
        "percent_complete",
    ];
    for k in kinds {
        assert!(
            all.contains(&format!("\"kind\": \"{k}\"")),
            "kind {k} never generated"
        );
    }
    let fields = [
        "totals_currency",
        "account_coding",
        "fringe_posting",
        "trailing_level_fringes",
        "arithmetic",
        "group_rules",
        "ungrouped_lines",
        "conflict",
        "fringes_in_totals",
        "new_line_defaults",
        "info",
        "currencies",
        "units",
        "globals",
        "global_info",
        "decimals",
        "show_by_name",
        "folder",
        "display",
        "rate_cutoff",
        "rate_decimals_below",
        "rate_decimals_above",
        "rate_global_decimals",
        "summary_rows",
        "fringes",
        "groups",
        "sets",
        "locations",
        "topsheet",
        "charges",
        "credits",
        "category",
        "baseline",
        "tag_presets",
        "line_presets",
        "setup",
        "versions",
        "created",
        "plural",
        "hours",
        "symbol",
        "cutoff",
        "unit",
        "suffix",
        "in_budget_total",
        "city",
        "region",
        "country",
        "accounts",
        "details",
        "amount",
        "x",
        "unit2",
        "rate",
        "unit3",
        "x4",
        "unit4",
        "currency",
        "fringe_range",
        "set",
        "location",
        "bold",
        "italic",
        "indent",
        "color",
        "note",
        "notes",
        "text",
        "edited",
        "author",
        "done",
        "short_code",
        "setup_order",
        "applied",
        "excluded_amounts",
        "excluded_groups",
        "excluded_categories",
        "excluded_accounts",
        "cap",
        "discount_percent",
        "rows",
        "source",
        "excluded_fringes",
        "grand_total",
        "net_total",
        "separator",
        "actuals",
        "transactions",
        "batches",
        "etc",
        "file",
        "imported",
        "line",
        "tags",
        "vendor",
        "reference",
        "batch",
        "percent",
    ];
    for f in fields {
        assert!(
            all.contains(&format!("\"{f}\":")),
            "field {f} never written"
        );
    }
}
