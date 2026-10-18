//! New Budget's setup step (roadmap 3.19, phase 2; decisions.md, entry 77;
//! design: docs/research/new-project-window.md, section 6): everything in
//! the personal setup library and any setup packs ticked in the form, brought
//! into the budget being made.
//!
//! - **Import Setup's rules, every item.** Each ticked source brings every
//!   setup item it has (never the budget settings, which belong to a
//!   production and are set in the form), with what each needs, through the
//!   same import as the Library tab and the Packs list
//!   (method_format::setup), so a pack's items are stamped with the pack as
//!   they are there and an item's source comes along.
//! - **What is ticked wins over the starting point.** The starter's or a
//!   chart's items are defaults nobody chose for this production; an item
//!   the ticked setup has under the same name replaces it.
//! - **Two ticked sources that disagree are refused**, whatever their order
//!   and whether or not one of them matches the starting point. Nothing
//!   decides between them quietly: the sentence names the item and both sources, and
//!   the other one can be brought in later from the Library tab or the Packs
//!   list, item by item.
//! - **All or nothing.** The budget changes only when every source came in.
//!
//! Which file is the library and where packs are kept are the desktop's
//! (method-app's src/start.rs reads them and hands the budgets in).

use std::collections::BTreeMap;

use method_core::Budget;
use method_format::setup::import::{self, all_items, item_ref, Resolution, What};
use method_format::setup::pack::{self, pack_kind};
use serde::Serialize;

/// One source of setup ticked in New Budget: what it is called in
/// sentences ("your library", a pack's title) and its budget.
#[derive(Clone, Debug)]
pub struct Ticked {
    pub label: String,
    pub budget: Budget,
}

/// What the setup step brought in, counted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SetupBrought {
    /// The sources, as ticked.
    pub sources: Vec<String>,
    /// Items the budget didn't have.
    pub added: usize,
    /// Items of the starting point replaced by the ticked setup's version.
    pub replaced: usize,
}

impl SetupBrought {
    /// The sentence saying so, or `None` when nothing was ticked.
    pub fn sentence(&self) -> Option<String> {
        if self.sources.is_empty() {
            return None;
        }
        let from = and_list(&self.sources);
        if self.added == 0 && self.replaced == 0 {
            return Some(format!(
                "Setup from {from}: the budget already had all of it."
            ));
        }
        let mut parts = Vec::new();
        if self.added > 0 {
            parts.push(format!(
                "{} added",
                import::count(self.added, "item", "items")
            ));
        }
        if self.replaced > 0 {
            parts.push(format!(
                "{} replaced",
                import::count(self.replaced, "item", "items")
            ));
        }
        Some(format!("Setup from {from}: {}.", parts.join(", ")))
    }
}

/// "a", "a and b", "a, b and c".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// An item's key for telling sources apart: presets are named ignoring case
/// and surrounding spaces, as the import names them.
fn key(w: What, name: &str) -> (What, String) {
    match w {
        What::TagPreset | What::LinePreset | What::ReportPreset => (w, name.trim().to_lowercase()),
        _ => (w, name.to_string()),
    }
}

/// An item's kind in words: "fringe", "tag preset".
fn kind_words(w: What) -> String {
    w.key().replace('_', " ")
}

/// Brings every setup item of each `ticked` source into `b`, in order, as
/// the module says. Refused (and `b` left as it was) when two sources
/// disagree about an item or an import refuses.
pub fn bring_in(b: &mut Budget, ticked: &[Ticked]) -> Result<SetupBrought, String> {
    let mut out = b.clone();
    let mut done = SetupBrought::default();
    // Every item an earlier source has, written or already the same here:
    // which source. After it came in, the budget holds that source's version
    // of each, so a later source that differs disagrees with it, in
    // whichever order they were ticked.
    let mut from: BTreeMap<(What, String), &str> = BTreeMap::new();
    for t in ticked {
        let src = &t.budget;
        let picks: Vec<_> = all_items(src)
            .into_iter()
            .filter(|(w, _)| pack_kind(*w))
            .collect();
        done.sources.push(t.label.clone());
        if picks.is_empty() {
            continue;
        }
        let review = import::preview(&out, src, 0, &t.label, "");
        let mut resolutions = Vec::new();
        for it in review.items.iter().filter(|it| it.status == "differs") {
            let w = What::parse(it.kind)?;
            if !pack_kind(w) {
                continue;
            }
            if let Some(earlier) = from.get(&key(w, &it.name)) {
                return Err(format!(
                    "The {} \u{201c}{}\u{201d} is in both {earlier} and {}, and they differ. Untick one of them; you can bring it in later from the Library tab or the Packs list, where you choose item by item.",
                    kind_words(w),
                    it.name,
                    t.label
                ));
            }
            resolutions.push(Resolution {
                kind: it.kind.to_string(),
                name: it.name.clone(),
                choice: "imported".into(),
            });
        }
        let refs: Vec<_> = picks.iter().map(item_ref).collect();
        let c = pack::import_pack(&mut out, src, &refs, &resolutions)
            .map_err(|e| format!("Setup from {}: {}.", t.label, e.trim_end_matches('.')))?;
        done.added += c.added;
        done.replaced += c.replaced;
        for (w, name) in &picks {
            from.entry(key(*w, name)).or_insert(t.label.as_str());
        }
    }
    *b = out;
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::{starter, Fringe, FringeKind, FringePosting};
    use method_format::setup::pack::PackDetails;

    fn fringe(rate: &str) -> Fringe {
        Fringe {
            short_code: None,
            description: String::new(),
            rate: rate.into(),
            kind: FringeKind::Percent { cutoff: None },
        }
    }

    /// A library: setup and no lines, with no units.
    fn library() -> Budget {
        let mut b = Budget::new("My Method Budget Library", "USD");
        b.units.clear();
        b.globals.insert("KIT_WEEKS".into(), "10".into());
        b.fringes.insert("KIT".into(), fringe("KIT_WEEKS * 2"));
        b
    }

    /// A setup pack of every item in `items`, titled `title`.
    fn pack_of(title: &str, items: &Budget) -> Budget {
        let picks: Vec<_> = all_items(items)
            .into_iter()
            .filter(|(w, _)| pack_kind(*w))
            .map(|k| item_ref(&k))
            .collect();
        let details = PackDetails {
            title: title.into(),
            version: "1".into(),
            effective: "2027-01-01".into(),
            ..PackDetails::default()
        };
        pack::build(items, &picks, &details, "2026-10-18", "0.0.0")
            .unwrap()
            .0
    }

    fn ticked(label: &str, budget: Budget) -> Ticked {
        Ticked {
            label: label.into(),
            budget,
        }
    }

    #[test]
    fn nothing_ticked_changes_nothing() {
        let mut b = starter::budget();
        let before = b.clone();
        let done = bring_in(&mut b, &[]).unwrap();
        assert_eq!(b, before);
        assert_eq!(done.sentence(), None);
    }

    #[test]
    fn the_library_and_packs_come_in_whole_and_win_over_the_starting_point() {
        let mut b = starter::budget();
        let fica = b
            .fringes
            .keys()
            .next()
            .cloned()
            .expect("the starter has fringes");
        let mut items = Budget::new("Union items", "USD");
        items.units.clear();
        items.fringes.insert(fica.clone(), fringe("99"));
        items
            .fringes
            .insert("PENSION".into(), fringe("PENSION_RATE"));
        items.globals.insert("PENSION_RATE".into(), "5".into());
        let union = pack_of("Union Basics", &items);

        let done = bring_in(
            &mut b,
            &[
                ticked("your library", library()),
                ticked("Union Basics", union),
            ],
        )
        .unwrap();
        // The library's fringe and the global it needs.
        assert_eq!(b.fringes["KIT"].rate.0, "KIT_WEEKS * 2");
        assert!(b.globals.contains_key("KIT_WEEKS"));
        // The pack's, the starter's same-named fringe replaced.
        assert_eq!(b.fringes[&fica].rate.0, "99");
        assert_eq!(b.fringes["PENSION"].rate.0, "PENSION_RATE");
        assert_eq!(done.added, 4);
        assert_eq!(done.replaced, 1);
        assert_eq!(
            done.sentence().unwrap(),
            "Setup from your library and Union Basics: 4 items added, 1 item replaced."
        );
        // The pack's items are stamped with it; the library's aren't.
        let s = b.setup_sources.get("fringe", "PENSION").unwrap();
        assert_eq!(s.pack, "Union Basics");
        assert_eq!(s.effective, "2027-01-01");
        assert!(b.setup_sources.get("fringe", "KIT").is_none());
    }

    #[test]
    fn the_budget_settings_never_come() {
        let mut b = starter::budget();
        let mut lib = library();
        lib.fringe_posting = FringePosting::Account;
        let posting = b.fringe_posting.clone();
        bring_in(&mut b, &[ticked("your library", lib)]).unwrap();
        assert_eq!(b.fringe_posting, posting);
    }

    #[test]
    fn two_ticked_sources_that_disagree_are_refused_and_nothing_changes() {
        let mut b = starter::budget();
        let before = b.clone();
        let mut other = library();
        other.globals.insert("KIT_WEEKS".into(), "12".into());
        let e = bring_in(
            &mut b,
            &[
                ticked("your library", library()),
                ticked("Kit Rental", pack_of("Kit Rental", &other)),
            ],
        )
        .unwrap_err();
        assert!(
            e.contains("\u{201c}KIT_WEEKS\u{201d} is in both your library and Kit Rental"),
            "{e}"
        );
        assert_eq!(b, before, "all or nothing");

        // The same items in both are fine, and count once.
        let mut b = starter::budget();
        let done = bring_in(
            &mut b,
            &[
                ticked("your library", library()),
                ticked("Kit Rental", pack_of("Kit Rental", &library())),
            ],
        )
        .unwrap();
        assert_eq!((done.added, done.replaced), (2, 0));
    }

    #[test]
    fn a_source_with_nothing_new_says_so() {
        let mut b = starter::budget();
        bring_in(&mut b, &[ticked("your library", library())]).unwrap();
        let done = bring_in(&mut b, &[ticked("your library", library())]).unwrap();
        assert_eq!(
            done.sentence().unwrap(),
            "Setup from your library: the budget already had all of it."
        );
        let mut empty = Budget::new("Empty", "USD");
        empty.units.clear();
        let done = bring_in(&mut b, &[ticked("Empty", empty)]).unwrap();
        assert_eq!(done.sources, vec!["Empty".to_string()]);
    }

    #[test]
    fn lists_are_said_in_words() {
        let s = |v: &[&str]| and_list(&v.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        assert_eq!(s(&[]), "");
        assert_eq!(s(&["a"]), "a");
        assert_eq!(s(&["a", "b"]), "a and b");
        assert_eq!(s(&["a", "b", "c"]), "a, b and c");
    }

    /// A source holding the starter's own first fringe as the starter has
    /// it, and one holding it with another rate.
    fn same_and_other() -> (Budget, Budget, String) {
        let b = starter::budget();
        let (name, f) = b
            .fringes
            .iter()
            .next()
            .map(|(n, f)| (n.clone(), f.clone()))
            .expect("the starter has fringes");
        let mut same = Budget::new("Same", "USD");
        same.units.clear();
        same.fringes.insert(name.clone(), f);
        let mut other = Budget::new("Other", "USD");
        other.units.clear();
        other.fringes.insert(name.clone(), fringe("99"));
        (same, other, name)
    }

    /// Refused, naming both, and `b` untouched.
    fn refused(ticked: &[Ticked], name: &str, first: &str, second: &str) {
        let mut b = starter::budget();
        let before = b.clone();
        let e = bring_in(&mut b, ticked).unwrap_err();
        assert!(
            e.contains(&format!(
                "\u{201c}{name}\u{201d} is in both {first} and {second}"
            )),
            "{e}"
        );
        assert_eq!(b, before, "all or nothing");
    }

    #[test]
    fn disagreeing_sources_are_refused_in_either_order_even_when_one_matches_the_start() {
        let (same, other, name) = same_and_other();
        // Library (as the starter has it) then a pack (another rate).
        refused(
            &[
                ticked("your library", same.clone()),
                ticked("Union Basics", pack_of("Union Basics", &other)),
            ],
            &name,
            "your library",
            "Union Basics",
        );
        // The pack first, then the library.
        refused(
            &[
                ticked("Union Basics", pack_of("Union Basics", &other)),
                ticked("your library", same.clone()),
            ],
            &name,
            "Union Basics",
            "your library",
        );
        // Two packs, both orders.
        refused(
            &[
                ticked("Pack A", pack_of("Pack A", &same)),
                ticked("Pack B", pack_of("Pack B", &other)),
            ],
            &name,
            "Pack A",
            "Pack B",
        );
        refused(
            &[
                ticked("Pack B", pack_of("Pack B", &other)),
                ticked("Pack A", pack_of("Pack A", &same)),
            ],
            &name,
            "Pack B",
            "Pack A",
        );
        // Agreeing sources, matching the start or not, are fine in any order.
        for t in [
            [
                ticked("your library", same.clone()),
                ticked("Pack A", pack_of("Pack A", &same)),
            ],
            [
                ticked("Pack B", pack_of("Pack B", &other)),
                ticked("your library", other.clone()),
            ],
        ] {
            let mut b = starter::budget();
            bring_in(&mut b, &t).unwrap();
        }
    }
}
