//! Person cutoffs across accounts (roadmap 3.11; decisions.md, entry 79).
//! Every name and figure is invented.

mod common;

use std::collections::BTreeMap;

use common::*;
use method_core::calc::cache::RecalcCache;
use method_core::calc::persons::{person_key, person_totals};
use method_core::explain::{Explainer, Target};
use method_core::sample::{self, SampleSpec};
use method_core::*;

const DANA: &str = "Dana Reyes";

fn suta(cutoff: Option<Cents>, rate: &str) -> Fringe {
    Fringe {
        description: "Unemployment".into(),
        short_code: None,
        rate: rate.into(),
        kind: FringeKind::Percent { cutoff },
    }
}

/// A line of `pay` (whole units) carrying `fringes`, paying `person`.
fn paid(id: u64, desc: &str, pay: &str, fringes: &[&str], person: Option<&str>) -> Detail {
    Detail {
        fringes: set(fringes),
        person: person.map(String::from),
        ..Detail::line(id, desc, "1", "Allow", pay)
    }
}

/// One person paid in three accounts of two categories: prep 3,000.00,
/// shoot 5,000.00 and wrap 2,000.00, all with SUTA (10%, cutoff 7,000.00),
/// beside another crew member with her own 4,000.00 line.
fn dana(person: Option<&str>) -> Budget {
    let mut b = Budget::new("Field Station", "USD");
    b.arithmetic = Arithmetic::Exact;
    b.fringes.insert("SUTA".into(), suta(Some(700_000), "10"));
    b.fringes.insert("FICA".into(), suta(None, "7.65"));
    b.topsheet = vec![
        category(
            "2000",
            "Production",
            vec![
                account(
                    "2100",
                    "Art",
                    vec![
                        paid(1, "Prep", "3000", &["SUTA", "FICA"], person),
                        paid(2, "Swing", "4000", &["SUTA"], Some("Lee Park")),
                    ],
                ),
                account(
                    "2200",
                    "Camera",
                    vec![paid(3, "Shoot", "5000", &["SUTA", "FICA"], person)],
                ),
            ],
        ),
        category(
            "3000",
            "Post",
            vec![account(
                "3100",
                "Editorial",
                vec![paid(4, "Wrap", "2000", &["SUTA"], person)],
            )],
        ),
    ];
    b
}

fn row_fringes(r: &Report) -> BTreeMap<RowId, BTreeMap<String, Cents>> {
    r.categories
        .iter()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.rows)
        .map(|row| (row.id, row.fringes.clone()))
        .collect()
}

/// Recalculated three ways (plain, cached, Explain's leaves) that must agree.
fn checked(b: &Budget) -> Report {
    let opts = RecalcOptions::default();
    let r = recalc(b, &opts).unwrap();
    assert_eq!(RecalcCache::new().recalc(b, &opts).unwrap(), r);
    let ex = Explainer::new(b, &opts).unwrap();
    for t in ex.targets() {
        let e = ex.explain(&t).unwrap();
        assert_eq!(e.root.leaf_sum(), Some(i128::from(e.reported)), "{t:?}");
        assert_eq!(e.reported, ex.reported(&t).unwrap(), "{t:?}");
    }
    r
}

#[test]
fn one_cutoff_for_a_person_paid_in_three_accounts() {
    let r = checked(&dana(Some(DANA)));
    let f = row_fringes(&r);
    // Pay 10,000.00 past the 7,000.00 cutoff: 700.00, split by pay.
    assert_eq!(f[&1]["SUTA"], 21_000);
    assert_eq!(f[&3]["SUTA"], 35_000);
    assert_eq!(f[&4]["SUTA"], 14_000);
    // No cutoff on FICA: each line its own 7.65%.
    assert_eq!(f[&1]["FICA"], 22_950);
    assert_eq!(f[&3]["FICA"], 38_250);
    // Lee is a person of her own, under the cutoff.
    assert_eq!(f[&2]["SUTA"], 40_000);
    assert_eq!(r.fringes["SUTA"].total, 70_000 + 40_000);
    // The account and category totals carry the person's pieces.
    assert_eq!(r.categories[1].accounts[0].fringe_total, 14_000);
    assert_eq!(
        r.categories[0].fringe_total,
        21_000 + 22_950 + 40_000 + 35_000 + 38_250
    );
}

#[test]
fn without_a_person_each_line_keeps_its_own_cutoff() {
    let r = checked(&dana(None));
    let f = row_fringes(&r);
    assert_eq!(
        (f[&1]["SUTA"], f[&3]["SUTA"], f[&4]["SUTA"]),
        (30_000, 50_000, 20_000)
    );
}

#[test]
fn a_blank_person_is_no_person() {
    let plain = recalc(&dana(None), &RecalcOptions::default()).unwrap();
    let blank = recalc(&dana(Some("  ")), &RecalcOptions::default()).unwrap();
    assert_eq!(plain, blank);
    // Spaces around a name don't make another person.
    let mut b = dana(Some(DANA));
    b.categories_mut().next().unwrap().accounts[0].details[0].person = Some(" Dana Reyes ".into());
    assert_eq!(
        row_fringes(&recalc(&b, &RecalcOptions::default()).unwrap()),
        row_fringes(&recalc(&dana(Some(DANA)), &RecalcOptions::default()).unwrap())
    );
}

#[test]
fn a_leftover_cent_goes_to_the_lowest_row_id_wherever_it_sits() {
    // 12.5% of a 2,000.00 cutoff is 250.00 over three equal lines: 83.33
    // each and one cent left over.
    let mut b = dana(Some(DANA));
    b.fringes.insert("SUTA".into(), suta(Some(200_000), "12.5"));
    for d in b
        .categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
    {
        if d.person.is_some() {
            d.rate = "1000".into();
        }
    }
    let f = row_fringes(&checked(&b));
    assert_eq!(
        (f[&1]["SUTA"], f[&3]["SUTA"], f[&4]["SUTA"]),
        (8_334, 8_333, 8_333)
    );
    // Moving Wrap's account to the top changes nothing.
    let post = b.topsheet.pop().unwrap();
    b.topsheet.insert(0, post);
    assert_eq!(row_fringes(&checked(&b)), f);
}

#[test]
fn a_person_takes_a_line_out_of_its_fringe_range() {
    let mut b = dana(Some(DANA));
    let art = &mut b.categories_mut().next().unwrap().accounts[0];
    // Prep and Swing in one range, but Prep is Dana's.
    art.details[0].fringe_range = Some(1);
    art.details[1].fringe_range = Some(1);
    art.details[1].person = None;
    art.details[1].rate = "9000".into();
    let f = row_fringes(&checked(&b));
    // Dana's pieces as before; Swing alone in its range, capped at 7,000.00.
    assert_eq!(
        (f[&1]["SUTA"], f[&3]["SUTA"], f[&4]["SUTA"]),
        (21_000, 35_000, 14_000)
    );
    assert_eq!(f[&2]["SUTA"], 70_000);
}

#[test]
fn a_suppressed_line_leaves_the_person() {
    let mut b = dana(Some(DANA));
    b.groups.insert("HOLD".into(), group(false));
    b.categories_mut().nth(1).unwrap().accounts[0].details[0]
        .groups
        .insert("HOLD".into());
    let f = row_fringes(&checked(&b));
    // Pay is now 8,000.00: 700.00 split 3,000 / 5,000.
    assert_eq!((f[&1]["SUTA"], f[&3]["SUTA"]), (26_250, 43_750));
    assert!(f[&4].is_empty());
    // As if the wrap line were deleted.
    let mut deleted = dana(Some(DANA));
    deleted.categories_mut().nth(1).unwrap().accounts[0]
        .details
        .clear();
    let g = row_fringes(&recalc(&deleted, &RecalcOptions::default()).unwrap());
    assert_eq!((g[&1]["SUTA"], g[&3]["SUTA"]), (26_250, 43_750));
}

#[test]
fn the_cutoff_factor_applies_to_a_person_cutoff() {
    let mut b = dana(Some(DANA));
    b.cutoff_factor = CutoffFactor::OneAndAHalf;
    let f = row_fringes(&checked(&b));
    // 10,000.00 is within 10,500.00: each line its own 10%.
    assert_eq!(
        (f[&1]["SUTA"], f[&3]["SUTA"], f[&4]["SUTA"]),
        (30_000, 50_000, 20_000)
    );
}

#[test]
fn movie_magic_arithmetic_splits_the_same_way() {
    let mut b = dana(Some(DANA));
    b.arithmetic = Arithmetic::Mmb;
    let f = row_fringes(&checked(&b));
    assert_eq!(
        (f[&1]["SUTA"], f[&3]["SUTA"], f[&4]["SUTA"]),
        (21_000, 35_000, 14_000)
    );
}

#[test]
fn person_totals_show_pay_against_the_cutoff() {
    let b = dana(Some(DANA));
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    let people = person_totals(&b, &r).unwrap();
    // Keyed by the matching form of the name, shown as typed.
    assert_eq!(
        people.keys().collect::<Vec<_>>(),
        ["dana reyes", "lee park"]
    );
    let p = &people["dana reyes"];
    assert_eq!(p.name, DANA);
    assert_eq!(p.rows, [1, 3, 4]);
    assert_eq!(p.accounts, ["2100", "2200", "3100"]);
    assert_eq!(p.pay, 1_000_000);
    assert_eq!(p.fringe_total, 70_000 + 22_950 + 38_250);
    let suta = p.fringes.iter().find(|f| f.fringe == "SUTA").unwrap();
    assert_eq!(
        (suta.pay, suta.cutoff, suta.amount, suta.reached),
        (1_000_000, Some(700_000), 70_000, true)
    );
    let fica = p.fringes.iter().find(|f| f.fringe == "FICA").unwrap();
    assert_eq!(
        (fica.pay, fica.cutoff, fica.reached),
        (800_000, None, false)
    );
    assert!(person_totals(
        &dana(None),
        &recalc(&dana(None), &RecalcOptions::default()).unwrap()
    )
    .unwrap()
    .contains_key("lee park"));
}

#[test]
fn explain_shows_the_person_step() {
    let b = dana(Some(DANA));
    let ex = Explainer::new(&b, &RecalcOptions::default()).unwrap();
    let e = ex.explain(&Target::LineFringe(3, "SUTA".into())).unwrap();
    assert_eq!(e.reported, 35_000);
    let text = e.to_text(None);
    for want in [
        "Person",
        "Dana Reyes is paid on 3 lines in accounts 2100, 2200 and 3100, so they share one cutoff",
        "10,000.00",
        "7,000.00",
        "700.00",
        "5,000.00",
    ] {
        assert!(text.contains(want), "{want:?} not in:\n{text}");
    }
    // Under the cutoff, it says so.
    let lee = ex.explain(&Target::LineFringe(2, "SUTA".into())).unwrap();
    let text = lee.to_text(None);
    assert!(text.contains("not reached"), "{text}");
}

// ---------------------------------------------------------------------------
// Properties, over generated budgets

fn budgets() -> impl Iterator<Item = (u64, Budget)> {
    (1..=10u64).map(|seed| {
        let mut b = sample::budget(&SampleSpec {
            seed,
            rows: 200 + seed as usize * 31,
            accounts: 8 + seed as usize,
            categories: 3,
            fringes: 8,
            groups: 3,
            baseline: false,
        });
        if seed % 3 == 0 {
            b.arithmetic = Arithmetic::Exact;
        }
        if seed % 4 == 0 {
            b.cutoff_factor = CutoffFactor::Two;
        }
        (seed, b)
    })
}

fn without_persons(mut b: Budget) -> Budget {
    for d in b
        .categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
    {
        d.person = None;
    }
    b
}

/// With no person on any row, every total is what the engine gave before
/// persons existed; and a person on a single row alone (not in a range)
/// changes nothing, since a lone row is already one earner.
#[test]
fn no_person_and_lone_persons_change_nothing() {
    for (seed, b) in budgets() {
        let opts = RecalcOptions::default();
        let plain = recalc(&b, &opts).unwrap();
        let mut lone = b.clone();
        let mut n = 0u64;
        for d in lone
            .categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
        {
            if d.kind.is_costed() && d.fringe_range.is_none() {
                n += 1;
                d.person = Some(format!("Person {n}"));
            }
        }
        assert_eq!(recalc(&lone, &opts).unwrap(), plain, "seed {seed}");
        let mut with = b.clone();
        sample::persons(&mut with, seed);
        assert_eq!(
            recalc(&without_persons(with), &opts).unwrap(),
            plain,
            "seed {seed}"
        );
    }
}

/// What fringe `name` of `b` comes to on one line paid exactly `pay`, with
/// no cutoff: one earner's worth of the fringe at its cutoff.
fn one_earner_at(b: &Budget, name: &str, pay: Cents) -> Cents {
    let mut one = b.clone();
    one.cutoff_factor = CutoffFactor::One;
    if let Some(f) = one.fringes.get_mut(name) {
        f.kind = FringeKind::Percent { cutoff: None };
    }
    let rate = format!("{}.{:02}", pay / 100, pay % 100);
    one.topsheet = vec![category(
        "9000",
        "Check",
        vec![account(
            "9100",
            "Check",
            vec![paid(1, "At the cutoff", &rate, &[name], None)],
        )],
    )];
    let r = recalc(&one, &RecalcOptions::default()).unwrap();
    assert_eq!(r.categories[0].accounts[0].rows[0].total, pay);
    r.categories[0].accounts[0].rows[0].fringes[name]
}

/// A person's percent fringe never counts the cutoff twice: across all of
/// the person's counted rows, it is exactly the percent of the cutoff
/// (rounded once) when their pay is past it, and each row's own percent
/// otherwise. Without credits, never more than with a cutoff per line (give
/// or take each line's rounding).
#[test]
fn a_person_cutoff_never_double_counts() {
    let mut checked_past = 0;
    for (seed, mut b) in budgets() {
        sample::persons(&mut b, seed);
        let opts = RecalcOptions::default();
        let r = recalc(&b, &opts).unwrap();
        let none = recalc(&without_persons(b.clone()), &opts).unwrap();
        let people = person_totals(&b, &r).unwrap();
        for p in people.values() {
            for f in &p.fringes {
                if let Some(c) = f.cutoff.filter(|&c| f.pay > c && c > 0) {
                    checked_past += 1;
                    assert_eq!(
                        f.amount,
                        one_earner_at(&b, &f.fringe, c),
                        "seed {seed} {} {}",
                        p.name,
                        f.fringe
                    );
                }
            }
            // Without credits, never more than with every line on its own
            // cutoff, give or take each line's own rounding (under a cent a
            // line). With a credit it can be more: see
            // `with_a_credit_a_person_can_earn_more_than_lines_alone`.
            let own_rows: Vec<&RowTotals> = p
                .rows
                .iter()
                .filter_map(|id| {
                    none.categories
                        .iter()
                        .flat_map(|c| &c.accounts)
                        .flat_map(|a| &a.rows)
                        .find(|row| row.id == *id)
                })
                .collect();
            if own_rows.iter().all(|row| row.total >= 0) {
                let own: Cents = own_rows.iter().map(|row| row.fringe_total).sum();
                let n = own_rows.len() as Cents;
                assert!(p.fringe_total <= own + n, "seed {seed} {}", p.name);
            }
        }
        // Every fringe dollar is on exactly one row.
        let rows: Cents = r
            .categories
            .iter()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.rows)
            .map(|row| row.fringe_total)
            .sum();
        let by_fringe: Cents = r.fringes.values().map(|f| f.total).sum();
        assert_eq!(rows, by_fringe, "seed {seed}");
    }
    assert!(checked_past > 0, "no person went past a cutoff");
}

/// Where a person's lines sit doesn't matter: moving categories, moving
/// accounts between categories and reversing accounts gives every row the
/// same fringes and the budget the same grand total.
#[test]
fn a_person_cutoff_does_not_depend_on_order() {
    for (seed, mut b) in budgets() {
        sample::persons(&mut b, seed);
        let opts = RecalcOptions::default();
        let r = recalc(&b, &opts).unwrap();
        let want = row_fringes(&r);

        let mut moved = b.clone();
        moved.topsheet.reverse();
        let mut cats: Vec<&mut Category> = moved.categories_mut().collect();
        for c in cats.iter_mut() {
            c.accounts.reverse();
        }
        // The first account of the first category moves to the last one.
        if cats.len() > 1 && !cats[0].accounts.is_empty() {
            let a = cats[0].accounts.remove(0);
            if let Some(last) = cats.last_mut() {
                last.accounts.push(a);
            }
        }
        let m = recalc(&moved, &opts).unwrap();
        assert_eq!(row_fringes(&m), want, "seed {seed}");
        assert_eq!(m.fringes, r.fringes, "seed {seed}");
    }
}

/// Excluding a group still equals deleting its lines, persons and all.
#[test]
fn excluding_a_group_equals_deleting_its_lines_with_persons() {
    for (seed, mut b) in budgets() {
        sample::persons(&mut b, seed);
        let Some(g) = b.groups.keys().next().cloned() else {
            continue;
        };
        let opts = RecalcOptions {
            exclude_groups: [g.clone()].into(),
            only_groups: None,
        };
        let excluded = recalc(&b, &opts).unwrap();
        let mut deleted = b.clone();
        for a in deleted.categories_mut().flat_map(|c| &mut c.accounts) {
            a.details.retain(|d| !d.groups.contains(&g));
        }
        let gone = recalc(&deleted, &RecalcOptions::default()).unwrap();
        assert_eq!(excluded.grand_total, gone.grand_total, "seed {seed}");
        let kept: BTreeMap<RowId, BTreeMap<String, Cents>> = row_fringes(&excluded)
            .into_iter()
            .filter(|(_, f)| !f.is_empty())
            .collect();
        let left: BTreeMap<RowId, BTreeMap<String, Cents>> = row_fringes(&gone)
            .into_iter()
            .filter(|(_, f)| !f.is_empty())
            .collect();
        assert_eq!(kept, left, "seed {seed}");
    }
}

/// Explain's leaves add up to every total of budgets with persons.
#[test]
fn explain_adds_up_with_persons() {
    for (seed, mut b) in budgets().take(4) {
        sample::persons(&mut b, seed);
        let opts = RecalcOptions::default();
        let ex = Explainer::new(&b, &opts).unwrap();
        for t in ex.targets() {
            let e = ex.explain(&t).unwrap();
            assert_eq!(
                e.root.leaf_sum(),
                Some(i128::from(e.reported)),
                "seed {seed} {t:?}"
            );
        }
    }
}

/// Names match with the ends trimmed, inner runs of spaces as one and case
/// ignored (decisions.md, entry 79): "Mara Lee", "mara  lee " and "MARA LEE"
/// are one person, shown as the first row in the budget spells it.
#[test]
fn names_match_ignoring_case_and_spacing() {
    assert_eq!(person_key("  Mara \t Lee "), "mara lee");
    assert_eq!(person_key("MARA LEE"), person_key("mara  lee "));
    let mut b = dana(None);
    for (id, name) in [(1, "Mara Lee"), (3, "mara  lee "), (4, "MARA LEE")] {
        for d in b
            .categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
            .filter(|d| d.id == id)
        {
            d.person = Some(name.into());
        }
    }
    let r = checked(&b);
    let f = row_fringes(&r);
    assert_eq!(
        (f[&1]["SUTA"], f[&3]["SUTA"], f[&4]["SUTA"]),
        (21_000, 35_000, 14_000)
    );
    let people = person_totals(&b, &r).unwrap();
    let p = &people["mara lee"];
    assert_eq!(
        (p.name.as_str(), p.rows.as_slice()),
        ("Mara Lee", &[1, 3, 4][..])
    );
    let ex = Explainer::new(&b, &RecalcOptions::default()).unwrap();
    let text = ex
        .explain(&Target::LineFringe(4, "SUTA".into()))
        .unwrap()
        .to_text(None);
    assert!(text.contains("Mara Lee is paid on 3 lines"), "{text}");
}

/// The "never more than per line" guarantee needs rows of pay, not credits:
/// 10% with a 7,000.00 cutoff on 10,000.00 and a -5,000.00 credit is
/// 700.00 - 500.00 = 200.00 line by line, but the person is paid 5,000.00,
/// within the cutoff, so earns 1,000.00 - 500.00 = 500.00.
#[test]
fn with_a_credit_a_person_can_earn_more_than_lines_alone() {
    let mut b = dana(None);
    fn lines(b: &mut Budget) -> Vec<&mut Detail> {
        b.categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
            .filter(|d| d.id == 1 || d.id == 3)
            .collect()
    }
    for (d, pay) in lines(&mut b).into_iter().zip(["10000", "-5000"]) {
        d.rate = pay.into();
        d.fringes = set(&["SUTA"]);
    }
    let alone: Cents = [1, 3]
        .iter()
        .map(|id| row_fringes(&checked(&b))[id]["SUTA"])
        .sum();
    assert_eq!(alone, 20_000);
    for d in lines(&mut b) {
        d.person = Some(DANA.into());
    }
    let f = row_fringes(&checked(&b));
    assert_eq!((f[&1]["SUTA"], f[&3]["SUTA"]), (100_000, -50_000));
}
