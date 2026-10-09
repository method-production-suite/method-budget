//! Baseline/variance, display currency and whole-budget edits.

mod common;

use common::*;
use fwb_core::*;

fn run(b: &Budget) -> Report {
    recalc(b, &RecalcOptions::default()).unwrap()
}

#[test]
fn deleting_a_global_keeps_every_total() {
    let mut b = sample();
    b.fringes.get_mut("PH").unwrap().rate = "PREP_DAYS * 5".into();
    let before = run(&b);

    assert_eq!(delete_global(&mut b, "PREP_DAYS").unwrap(), Some(d("10")));
    assert!(!b.globals.contains_key("PREP_DAYS"));
    let grip = &b.categories().nth(1).unwrap().accounts[0].details[0];
    assert_eq!(grip.amount.0, "SHOOT_DAYS + 10");
    assert_eq!(b.fringes["PH"].rate.0, "10 * 5");

    let after = run(&b);
    assert_eq!(after.categories, before.categories);
    assert_eq!(after.net_total, before.net_total);

    assert_eq!(delete_global(&mut b, "NOPE").unwrap(), None);
}

#[test]
fn deleting_a_global_used_by_another_global() {
    let mut b = sample();
    let before = run(&b);
    assert_eq!(delete_global(&mut b, "SHOOT_DAYS").unwrap(), Some(d("30")));
    assert_eq!(b.globals["PREP_DAYS"].0, "30 / 3");
    assert_eq!(run(&b).net_total, before.net_total);
}

#[test]
fn variance_against_a_baseline() {
    let mut b = sample();
    b.baseline = Some(Baseline::capture(&run(&b)).unwrap());

    // Writer gets a raise, the editor line is deleted, a new line appears.
    let cats: Vec<&mut Category> = b.categories_mut().collect();
    let [atl, _, post] = <[_; 3]>::try_from(cats).ok().unwrap();
    atl.accounts[0].details[0].rate = "160000".into();
    post.accounts[0].details[0] = Detail::line(50, "Assistant editor", 10, "Week", 1_500);

    let r = run(&b);
    let v = variance(b.baseline.as_ref().unwrap(), &r).unwrap();

    let writer = v.rows[&1];
    assert_eq!(
        (writer.original, writer.current),
        (Some(15_000_000), Some(16_000_000))
    );
    assert_eq!(writer.variance(), Some(1_000_000));
    assert_eq!(
        v.rows[&8],
        Change {
            original: Some(3_000_000),
            current: None
        }
    );
    assert_eq!(
        v.rows[&50],
        Change {
            original: None,
            current: Some(1_500_000)
        }
    );
    assert_eq!(v.rows[&8].variance(), Some(-3_000_000));
    // Subtotal rows aren't costs, so they're not compared.
    assert!(!v.rows.contains_key(&4));
    assert_eq!(v.accounts["5100"].variance(), Some(-1_500_000));
    assert_eq!(
        v.grand_total.variance(),
        Some(r.grand_total - b.baseline.as_ref().unwrap().grand_total)
    );
}

#[test]
fn totals_currency() {
    let mut b = sample();
    let r = run(&b);
    assert_eq!(r.totals_currency, "USD");
    assert_eq!(r.in_totals_currency(r.net_total), Some(r.net_total));

    b.totals_currency = Some("CAD".into());
    let r = run(&b);
    assert_eq!(r.totals_currency, "CAD");
    // $1.00 is 1 / 0.73 = 1.369863... CAD.
    assert_eq!(r.in_totals_currency(100), Some(137));

    b.totals_currency = Some("EUR".into());
    let e = recalc(&b, &RecalcOptions::default()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::UnknownCurrency("EUR".into()));
}

#[test]
fn budget_info_is_plain_data() {
    let mut b = sample();
    b.info
        .insert("Shoot days".into(), InfoValue::Number(d("30")));
    b.info
        .insert("Start".into(), InfoValue::Date("2027-03-01".into()));
    assert_eq!(run(&b).net_total, run(&sample()).net_total);
}
