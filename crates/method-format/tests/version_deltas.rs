//! Version deltas (`compact_versions`, docs/FORMAT.md "Version deltas"):
//! a file with deltas reads back to exactly the budget a file of full copies
//! does, takes far less room, and a damaged delta is a clean error.

use std::sync::Arc;

use method_core::versions::{delete_version, restore_version, save_version};
use method_core::*;
use method_format::{from_str, to_string, FormatError};
use serde_json::Value;

/// xorshift64*, as the other property tests use.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
}

fn accounts_mut(b: &mut Budget) -> Vec<&mut Account> {
    b.categories_mut()
        .flat_map(|c| c.accounts.iter_mut())
        .collect()
}

/// One random edit of the kind a day's work makes.
fn edit(b: &mut Budget, rng: &mut Rng) {
    let next_id = b.highest_row_id().unwrap_or(0) + 1;
    match rng.below(8) {
        0 => {
            let n = rng.below(1000).to_string();
            b.globals.insert(format!("G{}", rng.below(5)), Formula(n));
        }
        1 => b.title = format!("Draft {}", rng.below(100)),
        2 => {
            let mut accts = accounts_mut(b);
            if accts.is_empty() {
                return;
            }
            let k = rng.below(accts.len() as u64) as usize;
            let a = &mut accts[k];
            let at = rng.below(a.details.len() as u64 + 1) as usize;
            a.details.insert(
                at,
                Detail::line(
                    next_id,
                    "New line",
                    "1",
                    "Flat",
                    format!("{}", rng.below(5000)),
                ),
            );
        }
        3 => {
            let mut accts = accounts_mut(b);
            let with_rows: Vec<usize> = (0..accts.len())
                .filter(|&i| !accts[i].details.is_empty())
                .collect();
            if let Some(&k) = with_rows.get(rng.below(with_rows.len() as u64) as usize) {
                let a = &mut accts[k];
                let at = rng.below(a.details.len() as u64) as usize;
                a.details.remove(at);
            }
        }
        4 | 5 => {
            let mut accts = accounts_mut(b);
            let with_rows: Vec<usize> = (0..accts.len())
                .filter(|&i| !accts[i].details.is_empty())
                .collect();
            if let Some(&k) = with_rows.get(rng.below(with_rows.len() as u64) as usize) {
                let a = &mut accts[k];
                let at = rng.below(a.details.len() as u64) as usize;
                a.details[at].rate = Formula(format!("{}.{}", rng.below(900), rng.below(100)));
                if rng.below(2) == 0 {
                    a.details[at].description.push_str(" (revised)");
                }
            }
        }
        6 => {
            if let Some(TopsheetRow::Category(c)) = b.topsheet.first_mut() {
                c.name = format!("Category {}", rng.below(50));
            }
        }
        _ => {
            if let Some(c) = b.charges.first_mut() {
                c.applied = !c.applied;
            }
        }
    }
}

/// A budget with `n` versions, each saved after a few edits.
fn with_versions(mut b: Budget, n: usize, seed: u64) -> Budget {
    let mut rng = Rng(seed | 1);
    for i in 0..n {
        for _ in 0..1 + rng.below(3) {
            edit(&mut b, &mut rng);
        }
        save_version(
            &mut b,
            format!("Version {i}"),
            format!("2026-10-{:02}T10:00:00Z", 1 + i % 28),
            (i % 3 == 0).then(|| format!("note {i}")),
        );
    }
    for _ in 0..2 {
        edit(&mut b, &mut rng);
    }
    b
}

fn compact(b: &Budget) -> Budget {
    let mut c = b.clone();
    c.compact_versions = true;
    c
}

#[test]
fn deltas_read_back_exactly_like_full_copies() {
    let full = with_versions(starter::budget(), 6, 7);
    let packed = compact(&full);
    let text = to_string(&packed);
    assert!(text.contains("\"delta\""), "{text}");
    assert!(text.contains("\"compact_versions\": true"));
    let back = from_str(&text).unwrap();
    assert_eq!(back, packed);
    // Same versions as a file of full copies gives.
    let plain = from_str(&to_string(&full)).unwrap();
    assert_eq!(back.versions, plain.versions);
    assert_eq!(back.versions.len(), 6);
    // Writing again gives the same bytes.
    assert_eq!(to_string(&back), text);
    // Turning it off writes full copies again.
    let mut off = back.clone();
    off.compact_versions = false;
    assert_eq!(to_string(&off), to_string(&full));
}

#[test]
fn files_without_the_setting_are_unchanged() {
    let full = with_versions(starter::budget(), 3, 11);
    let text = to_string(&full);
    assert!(!text.contains("\"delta\""));
    assert!(!text.contains("compact_versions"));
    // On with no versions: only the setting itself is added.
    let mut none = starter::budget();
    none.compact_versions = true;
    let t = to_string(&none);
    assert_eq!(
        t.replace(",\n    \"compact_versions\": true", ""),
        to_string(&starter::budget())
    );
    assert_eq!(from_str(&t).unwrap(), none);
}

#[test]
fn ten_versions_of_a_big_budget_take_far_less_room() {
    let b = method_core::sample::budget(&method_core::sample::SampleSpec::default());
    let full = with_versions(b, 10, 3);
    let full_len = to_string(&full).len();
    let packed = compact(&full);
    let text = to_string(&packed);
    let one_copy = to_string(&Budget {
        versions: Vec::new(),
        ..full.clone()
    })
    .len();
    eprintln!(
        "one copy {one_copy} bytes, 10 versions in full {full_len}, as deltas {}",
        text.len()
    );
    // Full copies: about 11 budgets. Deltas: about one, plus small patches.
    assert!(full_len > one_copy * 10, "{full_len} vs {one_copy}");
    assert!(
        text.len() < one_copy * 2,
        "compact {} vs one copy {one_copy} (full {full_len})",
        text.len()
    );
    assert_eq!(from_str(&text).unwrap(), packed);
}

#[test]
fn a_snapshot_is_shared_and_complete() {
    let full = with_versions(starter::budget(), 4, 5);
    let back = from_str(&to_string(&compact(&full))).unwrap();
    for (a, b) in back.versions.iter().zip(&full.versions) {
        assert_eq!(*a.budget, *b.budget);
        // Each version owns a full snapshot.
        assert_eq!(Arc::strong_count(&a.budget), 1);
    }
}

/// The compact file of a small budget with versions, as JSON.
fn compact_json() -> Value {
    let full = with_versions(starter::budget(), 3, 9);
    serde_json::from_str(&to_string(&compact(&full))).unwrap()
}

fn read(v: &Value) -> Result<Budget, FormatError> {
    from_str(&serde_json::to_string_pretty(v).unwrap())
}

fn delta_error(v: &Value) -> String {
    match read(v) {
        Err(e @ FormatError::VersionDelta { .. }) => e.to_string(),
        other => panic!("expected a delta error, got {other:?}"),
    }
}

fn versions(v: &mut Value) -> &mut Vec<Value> {
    v["budget"]["versions"].as_array_mut().unwrap()
}

fn patch(v: &mut Value, i: usize) -> &mut Vec<Value> {
    v["budget"]["versions"][i]["delta"]["patch"]
        .as_array_mut()
        .unwrap()
}

#[test]
fn damaged_deltas_are_clean_errors() {
    let base = compact_json();
    let last = versions(&mut base.clone()).len() - 1;

    // A path that isn't in the base.
    let mut v = base.clone();
    patch(&mut v, last)[0]["path"] = Value::from("/topsheet/999/name");
    let e = delta_error(&v);
    assert!(e.contains(&format!("budget.versions[{last}]")), "{e}");
    assert!(e.contains("patch operation 0"), "{e}");

    // A result that isn't a budget.
    let mut v = base.clone();
    patch(&mut v, last).push(serde_json::json!({"op": "replace", "path": "/title", "value": 5}));
    assert!(delta_error(&v).contains("not valid"));

    // A result with something a version can't have.
    let mut v = base.clone();
    patch(&mut v, 0)
        .push(serde_json::json!({"op": "add", "path": "/display", "value": {"net_total": false}}));
    assert!(delta_error(&v).contains("not valid"));

    // The newest version can't lean on a next one.
    let mut v = base.clone();
    v["budget"]["versions"][last]["delta"]["base"] = Value::from("next");
    assert!(delta_error(&v).contains("must have the budget as its base"));

    // A remove with a value, an add without one.
    let mut v = base.clone();
    patch(&mut v, last).push(serde_json::json!({"op": "remove", "path": "/title", "value": 1}));
    assert!(delta_error(&v).contains("takes no value"));
    let mut v = base.clone();
    patch(&mut v, last).push(serde_json::json!({"op": "add", "path": "/title"}));
    assert!(delta_error(&v).contains("needs a value"));

    // Malformed shapes are ordinary format errors with a position.
    for (field, value) in [
        ("op", Value::from("move")),
        ("extra", Value::from(1)),
        ("path", Value::from(3)),
    ] {
        let mut v = base.clone();
        patch(&mut v, last)[0][field] = value;
        match read(&v) {
            Err(FormatError::Invalid { path, .. }) => {
                assert!(path.starts_with("budget.versions"), "{path}")
            }
            other => panic!("{field}: {other:?}"),
        }
    }
    let mut v = base.clone();
    v["budget"]["versions"][last]["budget"] =
        serde_json::json!({"title": "x", "base_currency": "USD"});
    let e = read(&v).unwrap_err().to_string();
    assert!(e.contains("not both"), "{e}");
    let mut v = base.clone();
    versions(&mut v)[last]
        .as_object_mut()
        .unwrap()
        .remove("delta");
    let e = read(&v).unwrap_err().to_string();
    assert!(e.contains("missing field `budget`"), "{e}");
    let mut v = base;
    v["budget"]["versions"][last]["delta"]["base"] = Value::from("previous");
    assert!(matches!(read(&v), Err(FormatError::Invalid { .. })));
}

#[test]
fn deltas_against_the_next_version_chain() {
    let v = compact_json();
    let versions = v["budget"]["versions"].as_array().unwrap();
    let bases: Vec<&str> = versions
        .iter()
        .map(|x| x["delta"]["base"].as_str().unwrap_or("full"))
        .collect();
    assert_eq!(bases.last(), Some(&"budget"));
    assert!(
        bases[..bases.len() - 1].iter().all(|b| *b == "next"),
        "{bases:?}"
    );
}

#[test]
fn property_random_histories_round_trip() {
    for seed in 1..=120u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut b = if seed % 3 == 0 {
            Budget::new("Empty", "USD")
        } else {
            starter::budget()
        };
        for step in 0..rng.below(12) {
            for _ in 0..rng.below(4) {
                edit(&mut b, &mut rng);
            }
            match rng.below(6) {
                0 if !b.versions.is_empty() => {
                    let id = b.versions[rng.below(b.versions.len() as u64) as usize].id;
                    restore_version(&mut b, id, "2026-10-09T12:00:00Z").unwrap();
                }
                1 if !b.versions.is_empty() => {
                    let id = b.versions[rng.below(b.versions.len() as u64) as usize].id;
                    delete_version(&mut b, id).unwrap();
                }
                _ => {
                    save_version(&mut b, format!("v{step}"), "2026-10-09T12:00:00Z", None);
                }
            }
        }
        b.compact_versions = true;
        let text = to_string(&b);
        let back = from_str(&text).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        assert_eq!(back, b, "seed {seed}");
        assert_eq!(to_string(&back), text, "seed {seed}");
        b.compact_versions = false;
        let full = from_str(&to_string(&b)).unwrap();
        assert_eq!(full.versions, back.versions, "seed {seed}");
    }
}
