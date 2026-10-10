//! Sub-budgets (roadmap 2.22): a filtered copy of a budget, as an
//! independent budget with no live link to its parent.
//!
//! # What is kept
//!
//! 1. **The chart.** Categories and accounts picked: an account is kept when
//!    its category is picked or it is picked itself (so picking a category
//!    and one account elsewhere keeps both). Nothing picked keeps the whole
//!    chart. Production totals are always kept, in place.
//! 2. **The lines**, by tags. Each tag kind (groups, sets, locations) is a
//!    [`TagFilter`]: a line passes when it carries any of the picked values
//!    (OR within a kind), or carries none of that kind and lines without one
//!    are asked for. A line must pass every kind that has a filter (AND
//!    across kinds); a kind with nothing picked passes every line. So a line
//!    carrying both a picked and an unpicked group is kept. Filters apply to
//!    costed rows (lines and adjustments); subtotal and separator rows stay
//!    when their account keeps at least one costed row.
//! 3. Optionally, accounts left with no rows are dropped, and then
//!    categories left with no accounts.
//! 4. Optionally, every fringe comes off every line (the fringe table stays,
//!    so they can be put back), and every note on lines, accounts and
//!    categories goes.
//!
//! Setup tables, settings, budget info, charges, credits, presets and display
//! settings come along as they are; references to categories and accounts
//! left out (a charge's exclusions, a report preset's) stay and simply name
//! nothing. The saved versions and the baseline are left out: they describe
//! the whole parent, so their totals would not line up with the copy.
//!
//! Totals are recalculated normally: percent charges and fringe cutoffs see
//! only what was kept, and a fringe range missing some of its lines caps
//! what is left of it. There is no fringe lock (keeping the parent's fringe
//! dollars) in this first version.

use std::collections::BTreeSet;
use std::fmt;

use crate::model::{Budget, Detail, TopsheetRow};

/// Which values of one tag kind a line must carry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TagFilter {
    /// Values picked; a line with any of them passes.
    pub values: BTreeSet<String>,
    /// Lines with no value of this kind pass too.
    pub include_untagged: bool,
}

impl TagFilter {
    /// Whether this kind filters at all.
    pub fn is_active(&self) -> bool {
        !self.values.is_empty() || self.include_untagged
    }

    fn passes<'a>(&self, mut tags: impl Iterator<Item = &'a String>) -> bool {
        if !self.is_active() {
            return true;
        }
        let mut any = false;
        let hit = tags.any(|t| {
            any = true;
            self.values.contains(t)
        });
        hit || (!any && self.include_untagged)
    }
}

/// What goes into a sub-budget.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubBudgetSpec {
    /// Category codes picked (every account in them is kept).
    pub categories: BTreeSet<String>,
    /// Account codes picked.
    pub accounts: BTreeSet<String>,
    pub groups: TagFilter,
    pub sets: TagFilter,
    pub locations: TagFilter,
    /// Leave out accounts with no rows left (and categories left empty).
    pub skip_empty_accounts: bool,
    /// Take every fringe off every line.
    pub drop_fringes: bool,
    /// Leave out every note.
    pub drop_notes: bool,
}

/// Why no sub-budget was made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubBudgetError {
    /// A picked category isn't in the budget.
    NoCategory(String),
    /// A picked account isn't in the budget.
    NoAccount(String),
    /// A picked group, set or location isn't in the budget.
    NoTag { kind: &'static str, name: String },
    /// The choices leave no category at all.
    NothingLeft,
}

impl fmt::Display for SubBudgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubBudgetError::NoCategory(c) => write!(f, "there is no category {c}"),
            SubBudgetError::NoAccount(a) => write!(f, "there is no account {a}"),
            SubBudgetError::NoTag { kind, name } => write!(f, "there is no {kind} `{name}`"),
            SubBudgetError::NothingLeft => {
                f.write_str("nothing in the budget matches those choices")
            }
        }
    }
}

impl std::error::Error for SubBudgetError {}

fn line_passes(d: &Detail, spec: &SubBudgetSpec) -> bool {
    spec.groups.passes(d.groups.iter())
        && spec.sets.passes(d.set.iter())
        && spec.locations.passes(d.location.iter())
}

fn check(b: &Budget, spec: &SubBudgetSpec) -> Result<(), SubBudgetError> {
    let cats: BTreeSet<&String> = b.categories().map(|c| &c.code).collect();
    if let Some(c) = spec.categories.iter().find(|c| !cats.contains(c)) {
        return Err(SubBudgetError::NoCategory(c.clone()));
    }
    let accts: BTreeSet<&String> = b
        .categories()
        .flat_map(|c| &c.accounts)
        .map(|a| &a.code)
        .collect();
    if let Some(a) = spec.accounts.iter().find(|a| !accts.contains(a)) {
        return Err(SubBudgetError::NoAccount(a.clone()));
    }
    let tag_check = |kind: &'static str, f: &TagFilter, known: &dyn Fn(&str) -> bool| match f
        .values
        .iter()
        .find(|v| !known(v))
    {
        Some(name) => Err(SubBudgetError::NoTag {
            kind,
            name: name.clone(),
        }),
        None => Ok(()),
    };
    tag_check("group", &spec.groups, &|n| b.groups.contains_key(n))?;
    tag_check("set", &spec.sets, &|n| b.sets.contains_key(n))?;
    tag_check("location", &spec.locations, &|n| {
        b.locations.contains_key(n)
    })
}

/// The sub-budget of `parent` that `spec` describes, titled and stamped as
/// the parent (the caller names it). See the module docs for the rules.
pub fn sub_budget(parent: &Budget, spec: &SubBudgetSpec) -> Result<Budget, SubBudgetError> {
    check(parent, spec)?;
    let mut b = parent.clone();
    b.versions.clear();
    b.baseline = None;
    // The parent's actuals describe the whole production, like its baseline.
    b.actuals = Default::default();
    let whole_chart = spec.categories.is_empty() && spec.accounts.is_empty();
    let filtering_lines =
        spec.groups.is_active() || spec.sets.is_active() || spec.locations.is_active();

    b.topsheet.retain_mut(|row| {
        let TopsheetRow::Category(cat) = row else {
            return true;
        };
        let cat_picked = whole_chart || spec.categories.contains(&cat.code);
        cat.accounts
            .retain(|a| cat_picked || spec.accounts.contains(&a.code));
        if cat.accounts.is_empty() && !cat_picked {
            return false;
        }
        for a in &mut cat.accounts {
            if filtering_lines {
                a.details
                    .retain(|d| !d.kind.is_costed() || line_passes(d, spec));
                if !a.details.iter().any(|d| d.kind.is_costed()) {
                    a.details.clear();
                }
            }
            if spec.drop_fringes {
                for d in &mut a.details {
                    d.fringes.clear();
                }
            }
            if spec.drop_notes {
                a.notes.clear();
                for d in &mut a.details {
                    d.notes.clear();
                }
            }
        }
        if spec.skip_empty_accounts {
            cat.accounts.retain(|a| !a.details.is_empty());
            if cat.accounts.is_empty() {
                return false;
            }
        }
        if spec.drop_notes {
            cat.notes.clear();
        }
        true
    });
    if b.categories().next().is_none() {
        return Err(SubBudgetError::NothingLeft);
    }
    Ok(b)
}

/// A one-line description of what `spec` keeps, for the sub-budget's
/// provenance note: `Categories 10, 20; groups Night; without fringes`.
pub fn describe(spec: &SubBudgetSpec) -> String {
    fn list(set: &BTreeSet<String>) -> String {
        set.iter().cloned().collect::<Vec<_>>().join(", ")
    }
    let mut parts = Vec::new();
    if !spec.categories.is_empty() {
        parts.push(format!("categories {}", list(&spec.categories)));
    }
    if !spec.accounts.is_empty() {
        parts.push(format!("accounts {}", list(&spec.accounts)));
    }
    if parts.is_empty() {
        parts.push("every category".to_string());
    }
    for (kind, f) in [
        ("groups", &spec.groups),
        ("sets", &spec.sets),
        ("locations", &spec.locations),
    ] {
        if !f.is_active() {
            continue;
        }
        let mut what = list(&f.values);
        if f.include_untagged {
            let none = format!("lines without {kind}");
            what = if what.is_empty() {
                none
            } else {
                format!("{what} or {none}")
            };
        }
        parts.push(format!("{kind} {what}"));
    }
    if spec.skip_empty_accounts {
        parts.push("without empty accounts".into());
    }
    if spec.drop_fringes {
        parts.push("without fringes".into());
    }
    if spec.drop_notes {
        parts.push("without notes".into());
    }
    let mut s = parts.join("; ");
    if let Some(first) = s.get(..1) {
        s = first.to_uppercase() + &s[1..];
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Group, Location, Note, SetDef};
    use crate::{recalc, starter, RecalcOptions};

    fn set(xs: &[&str]) -> BTreeSet<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    fn lines(b: &Budget) -> Vec<&Detail> {
        b.categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .collect()
    }

    fn account_codes(b: &Budget) -> Vec<String> {
        b.categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.code.clone())
            .collect()
    }

    /// The starter budget with some tags to filter by.
    fn tagged() -> Budget {
        let mut b = starter::budget();
        b.groups.insert(
            "Night".into(),
            Group {
                description: String::new(),
                short_code: None,
                in_budget_total: true,
                color: None,
            },
        );
        b.groups.insert(
            "Day".into(),
            Group {
                description: String::new(),
                short_code: None,
                in_budget_total: true,
                color: None,
            },
        );
        b.sets.insert("Ext".into(), SetDef::default());
        b.locations.insert("LA".into(), Location::default());
        let mut n = 0;
        for c in b.categories_mut() {
            for a in &mut c.accounts {
                for d in &mut a.details {
                    if !d.kind.is_costed() {
                        continue;
                    }
                    n += 1;
                    match n % 4 {
                        0 => {
                            d.groups = set(&["Night"]);
                            d.set = Some("Ext".into());
                        }
                        1 => {
                            d.groups = set(&["Night", "Day"]);
                            d.location = Some("LA".into());
                        }
                        2 => d.groups = set(&["Day"]),
                        _ => {}
                    }
                }
            }
        }
        b
    }

    #[test]
    fn nothing_picked_keeps_everything_but_versions_and_baseline() {
        let mut b = tagged();
        crate::save_version(&mut b, "v1", "2026-10-09T10:00:00Z", None);
        b.baseline = Some(crate::Baseline::default());
        let s = sub_budget(&b, &SubBudgetSpec::default()).unwrap();
        assert!(s.versions.is_empty() && s.baseline.is_none());
        assert_eq!(s.topsheet, b.topsheet);
        assert_eq!(s.charges, b.charges);
        assert_eq!(
            recalc(&s, &RecalcOptions::default()).unwrap().grand_total,
            recalc(&b, &RecalcOptions::default()).unwrap().grand_total
        );
    }

    #[test]
    fn categories_and_accounts_are_a_union() {
        let b = tagged();
        let cats: Vec<&crate::Category> = b.categories().collect();
        let first = cats[0].code.clone();
        let other = cats[1].accounts[0].code.clone();
        let spec = SubBudgetSpec {
            categories: set(&[&first]),
            accounts: set(&[&other]),
            ..SubBudgetSpec::default()
        };
        let s = sub_budget(&b, &spec).unwrap();
        let mut want: Vec<String> = cats[0].accounts.iter().map(|a| a.code.clone()).collect();
        want.push(other);
        assert_eq!(account_codes(&s), want);
        assert_eq!(s.categories().count(), 2);
        // Production totals stay.
        let pts = |b: &Budget| {
            b.topsheet
                .iter()
                .filter(|r| matches!(r, TopsheetRow::ProductionTotal(_)))
                .count()
        };
        assert_eq!(pts(&s), pts(&b));
    }

    #[test]
    fn or_within_a_kind_and_across_kinds() {
        let b = tagged();
        // Night: every line tagged Night, including Night+Day.
        let night = SubBudgetSpec {
            groups: TagFilter {
                values: set(&["Night"]),
                include_untagged: false,
            },
            ..SubBudgetSpec::default()
        };
        let s = sub_budget(&b, &night).unwrap();
        let ls = lines(&s);
        assert!(ls
            .iter()
            .filter(|d| d.kind.is_costed())
            .all(|d| d.groups.contains("Night")));
        assert!(
            ls.iter().any(|d| d.groups.len() == 2),
            "a line with a picked and an unpicked group is kept"
        );
        // Night AND set Ext: only the n%4==0 lines.
        let both = SubBudgetSpec {
            sets: TagFilter {
                values: set(&["Ext"]),
                include_untagged: false,
            },
            ..night.clone()
        };
        let s2 = sub_budget(&b, &both).unwrap();
        assert!(lines(&s2)
            .iter()
            .filter(|d| d.kind.is_costed())
            .all(|d| d.groups == set(&["Night"]) && d.set.as_deref() == Some("Ext")));
        assert!(lines(&s2).len() < ls.len());
        // Groups Day or no group at all.
        let day_or_none = SubBudgetSpec {
            groups: TagFilter {
                values: set(&["Day"]),
                include_untagged: true,
            },
            ..SubBudgetSpec::default()
        };
        let s3 = sub_budget(&b, &day_or_none).unwrap();
        assert!(lines(&s3)
            .iter()
            .filter(|d| d.kind.is_costed())
            .all(|d| d.groups.is_empty() || d.groups.contains("Day")));
        assert!(lines(&s3)
            .iter()
            .any(|d| d.kind.is_costed() && d.groups.is_empty()));
    }

    #[test]
    fn empty_accounts_fringes_and_notes() {
        let mut b = tagged();
        b.categories_mut().next().unwrap().notes = vec![Note::plain(1, "cat")];
        let spec = SubBudgetSpec {
            locations: TagFilter {
                values: set(&["LA"]),
                include_untagged: false,
            },
            ..SubBudgetSpec::default()
        };
        let kept_empty = sub_budget(&b, &spec).unwrap();
        assert_eq!(
            account_codes(&kept_empty),
            account_codes(&b),
            "empty accounts stay by default"
        );
        let s = sub_budget(
            &b,
            &SubBudgetSpec {
                skip_empty_accounts: true,
                drop_fringes: true,
                drop_notes: true,
                ..spec.clone()
            },
        )
        .unwrap();
        assert!(s
            .categories()
            .all(|c| !c.accounts.is_empty() && c.notes.is_empty()));
        assert!(s
            .categories()
            .flat_map(|c| &c.accounts)
            .all(|a| !a.details.is_empty()));
        assert!(lines(&s)
            .iter()
            .all(|d| d.fringes.is_empty() && d.notes.is_empty()));
        assert_eq!(s.fringes, b.fringes, "the fringe table stays");
        // Dropping fringes lowers the total, and it still recalculates.
        let with = recalc(&sub_budget(&b, &spec).unwrap(), &RecalcOptions::default()).unwrap();
        let without = recalc(&s, &RecalcOptions::default()).unwrap();
        assert!(without.grand_total < with.grand_total);
    }

    #[test]
    fn unknown_picks_and_nothing_left_are_refused() {
        let b = tagged();
        let e = |spec: SubBudgetSpec| sub_budget(&b, &spec).unwrap_err();
        assert_eq!(
            e(SubBudgetSpec {
                categories: set(&["XX"]),
                ..Default::default()
            }),
            SubBudgetError::NoCategory("XX".into())
        );
        assert_eq!(
            e(SubBudgetSpec {
                accounts: set(&["XX-01"]),
                ..Default::default()
            }),
            SubBudgetError::NoAccount("XX-01".into())
        );
        assert!(matches!(
            e(SubBudgetSpec {
                sets: TagFilter {
                    values: set(&["Nope"]),
                    include_untagged: false
                },
                ..Default::default()
            }),
            SubBudgetError::NoTag { kind: "set", .. }
        ));
        // A location no line has, with empty accounts left out: nothing left.
        let mut none = b.clone();
        none.locations.insert("Nowhere".into(), Location::default());
        let r = sub_budget(
            &none,
            &SubBudgetSpec {
                locations: TagFilter {
                    values: set(&["Nowhere"]),
                    include_untagged: false,
                },
                skip_empty_accounts: true,
                ..Default::default()
            },
        );
        assert_eq!(r.unwrap_err(), SubBudgetError::NothingLeft);
    }

    #[test]
    fn descriptions() {
        assert_eq!(describe(&SubBudgetSpec::default()), "Every category");
        let spec = SubBudgetSpec {
            categories: set(&["10", "20"]),
            groups: TagFilter {
                values: set(&["Night"]),
                include_untagged: true,
            },
            drop_fringes: true,
            ..Default::default()
        };
        assert_eq!(
            describe(&spec),
            "Categories 10, 20; groups Night or lines without groups; without fringes"
        );
    }
}
