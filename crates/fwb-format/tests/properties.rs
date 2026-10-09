//! Seeded random round trips over every field and enum variant.
//!
//! Uses the same small xorshift generator as fwb-core's property tests (copied,
//! since test crates cannot share code). Each failure prints its seed.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use fwb_core::coding::AccountCoding;
use fwb_core::*;

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
        note: rng.opt_text(),
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
    }
}

fn gen_credit(rng: &mut Rng) -> Credit {
    Credit {
        name: rng.text(),
        account: rng.opt_text(),
        applied: rng.chance(50),
        cap: rng.chance(50).then(|| rng.cents()),
        discount_percent: rng.dec(),
        rows: (0..rng.below(4))
            .map(|_| CreditRow {
                description: rng.text(),
                source: if rng.chance(50) {
                    CreditSource::Flat(rng.cents())
                } else {
                    CreditSource::Group {
                        group: rng.text(),
                        excluded_fringes: rng.texts(),
                    }
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

fn gen_budget(rng: &mut Rng) -> Budget {
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
    if rng.chance(70) {
        b.units = rng.map(|r| UnitDef {
            plural: r.text(),
            description: r.text(),
            hours: r.chance(50).then(|| r.dec()),
        });
    }
    b.fringes = rng.map(|r| Fringe {
        description: r.text(),
        rate: r.formula(),
        kind: if r.chance(50) {
            FringeKind::Percent {
                cutoff: r.chance(50).then(|| r.cents()),
            }
        } else {
            FringeKind::Flat { unit: r.opt_text() }
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
    b.groups = rng.map(|r| Group {
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
                account(&rng.text(), &rng.text(), details)
            })
            .collect();
        b.topsheet
            .push(category(&rng.text(), &rng.text(), accounts));
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

#[test]
fn random_budgets_round_trip_exactly() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        let text = fwb_format::to_string(&b);
        let back =
            fwb_format::from_str(&text).unwrap_or_else(|e| panic!("seed {seed}: {e}\n{text}"));
        assert_eq!(back, b, "seed {seed}");
        assert_eq!(fwb_format::to_string(&back), text, "seed {seed}");
    }
}

#[test]
fn output_is_deterministic() {
    for seed in 1..=CASES / 4 {
        let a = fwb_format::to_string(&gen_budget(&mut Rng(seed)));
        let b = fwb_format::to_string(&gen_budget(&mut Rng(seed)));
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
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        group_colors += b.groups.values().filter(|g| g.color.is_some()).count();
        row_colors += b
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .filter(|d| d.color.is_some())
            .count();
        all.push_str(&fwb_format::to_string(&b));
    }
    // `color` is a field of both groups and detail rows: check each.
    assert!(group_colors > 0 && row_colors > 0);
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
        "info",
        "currencies",
        "units",
        "globals",
        "fringes",
        "groups",
        "sets",
        "locations",
        "topsheet",
        "charges",
        "credits",
        "baseline",
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
    ];
    for f in fields {
        assert!(
            all.contains(&format!("\"{f}\":")),
            "field {f} never written"
        );
    }
}
