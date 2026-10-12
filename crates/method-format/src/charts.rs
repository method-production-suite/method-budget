//! The chart-of-accounts templates bundled with the app and the command
//! line: `templates/charts/*.mthd` in the repository, each built from a
//! source by `scripts/build-coa-templates.py` and listed in
//! `templates/charts/charts.json` (docs/TEMPLATES.md). The list itself,
//! `charts_bundled.rs`, is written by that script; a test checks it agrees
//! with the manifest.
//!
//! A template is an ordinary budget file holding a chart (categories,
//! accounts, production totals) and nothing else.
//!
//! The texts are compiled in only with the `bundled-charts` feature (on by
//! default: the desktop and the command line). Without it, as in the
//! browser build, [`all`] is empty and a host supplies a chart's text itself
//! when it is picked (method-session's `coa_import::HostCharts`).

use method_core::Budget;

/// One bundled chart template.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bundled {
    /// Short name used by `method new --template` (`feature`, `tv-one-hour`).
    pub id: &'static str,
    /// The name shown in New From Template.
    pub name: &'static str,
    /// `Feature`, `Television`, `Commercial` or `Other`.
    pub group: &'static str,
    /// One line about it (what kind of production, whose chart it follows).
    pub description: &'static str,
    /// The template file's text.
    pub text: &'static str,
}

#[cfg(feature = "bundled-charts")]
mod bundled {
    use super::Bundled;
    include!("charts_bundled.rs");
}
#[cfg(feature = "bundled-charts")]
use bundled::BUNDLED;

/// No charts compiled in (the browser build).
#[cfg(not(feature = "bundled-charts"))]
static BUNDLED: &[Bundled] = &[];

/// The groups New From Template shows the charts under, in order.
pub const GROUPS: [&str; 4] = ["Feature", "Television", "Commercial", "Other"];

/// Every bundled chart, in the manifest's order.
pub fn all() -> &'static [Bundled] {
    BUNDLED
}

/// The bundled chart whose id or name is `key`, ignoring case.
pub fn find(key: &str) -> Option<&'static Bundled> {
    let key = key.trim();
    BUNDLED
        .iter()
        .find(|c| c.id.eq_ignore_ascii_case(key) || c.name.eq_ignore_ascii_case(key))
}

impl Bundled {
    /// The template as a budget.
    pub fn budget(&self) -> Result<Budget, crate::FormatError> {
        crate::from_str(self.text)
    }
}

#[cfg(all(test, feature = "bundled-charts"))]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[derive(serde::Deserialize)]
    struct Manifest {
        charts: Vec<Entry>,
    }

    #[derive(serde::Deserialize)]
    #[allow(dead_code)]
    struct Entry {
        id: String,
        name: String,
        group: String,
        description: String,
        source: String,
        #[serde(default)]
        mapping: Option<String>,
        #[serde(default)]
        currency: Option<String>,
    }

    fn manifest() -> Manifest {
        let text = std::fs::read_to_string(repo().join("templates/charts/charts.json")).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn the_bundled_list_agrees_with_the_manifest() {
        let m = manifest();
        let ids: Vec<&str> = m.charts.iter().map(|e| e.id.as_str()).collect();
        let bundled: Vec<&str> = BUNDLED.iter().map(|c| c.id).collect();
        assert_eq!(ids, bundled, "run scripts/build-coa-templates.py");
        for (e, c) in m.charts.iter().zip(BUNDLED) {
            assert_eq!(
                (e.name.as_str(), e.group.as_str(), e.description.as_str()),
                (c.name, c.group, c.description)
            );
            assert!(GROUPS.contains(&c.group), "{} is in an unknown group", c.id);
            let on_disk =
                std::fs::read_to_string(repo().join(format!("templates/charts/{}.mthd", c.id)))
                    .unwrap();
            assert_eq!(on_disk, c.text);
        }
    }

    /// Every template opens, holds a chart and nothing else, and
    /// recalculates. Categories with no accounts are allowed (studio charts
    /// have section headers and departments with no default accounts) and
    /// total 0. Account numbers are kept as the chart writes them, so they
    /// needn't follow the template's pattern, but no two accounts may be
    /// the same account to anything that matches by number (actuals,
    /// comparison, lookups).
    #[test]
    fn every_bundled_template_opens_and_recalculates() {
        assert!(!BUNDLED.is_empty());
        for c in BUNDLED {
            let b = c.budget().unwrap_or_else(|e| panic!("{}: {e}", c.id));
            assert_eq!(b.title, c.name, "{}", c.id);
            assert!(b.categories().count() > 3, "{}", c.id);
            assert!(
                b.categories().any(|cat| !cat.accounts.is_empty()),
                "{}",
                c.id
            );
            assert!(b
                .categories()
                .flat_map(|c| &c.accounts)
                .all(|a| a.details.is_empty()));
            assert!(b.fringes.is_empty() && b.globals.is_empty() && b.charges.is_empty());
            let r = method_core::recalc(&b, &method_core::RecalcOptions::default()).unwrap();
            assert_eq!(r.grand_total, 0);
            assert!(r.categories.iter().all(|c| c.total == 0), "{}", c.id);
            // One number, one account, however the pattern reads it.
            let mut keys = std::collections::BTreeSet::new();
            for a in b.categories().flat_map(|c| &c.accounts) {
                assert!(
                    keys.insert(b.account_coding.key(&a.code)),
                    "{}: account {} is another account's number as the pattern reads it",
                    c.id,
                    a.code
                );
            }
            let chart = method_core::actuals::chart(&b);
            assert_eq!(
                chart.values().filter(|e| !e.fringe).count(),
                keys.len(),
                "{}",
                c.id
            );
            // Each category's fringe account (fringes post by category in a
            // new budget) has a number of its own.
            if let method_core::FringePosting::Category { suffix } = &b.fringe_posting {
                let fringes: std::collections::BTreeSet<String> = b
                    .categories()
                    .map(|cat| {
                        let n = b.account_coding.category_fringe_account(&cat.code, suffix);
                        b.account_coding.key(&n)
                    })
                    .collect();
                assert_eq!(fringes.len(), b.categories().count(), "{}", c.id);
            }
            // Compares with itself and gives a cost report.
            let cmp = method_core::comparison::compare(&[("a".into(), &b), ("b".into(), &b)])
                .unwrap_or_else(|e| panic!("{}: {e}", c.id));
            let compared = cmp
                .categories
                .iter()
                .flat_map(|c| &c.accounts)
                .filter(|a| !a.fringe_account)
                .count();
            assert_eq!(compared, keys.len(), "{}", c.id);
            let roll = method_core::actuals::rollup(&b.actuals, &b.account_coding).unwrap();
            assert!(
                method_core::actuals::cost_report(&b, &r, &roll).is_some(),
                "{}",
                c.id
            );
            // Written as it reads.
            assert_eq!(crate::to_string(&b), c.text, "{}", c.id);
        }
        assert_eq!(find("FEATURE").map(|c| c.id), Some("feature"));
        assert!(find("nope").is_none());
    }

    /// Rebuilding each template whose source is in the repository gives
    /// the same bytes (the studio sources stay outside it, CLEAN-ROOM.md).
    #[test]
    fn templates_rebuild_byte_for_byte() {
        let mut checked = 0;
        for e in manifest().charts {
            let src = repo().join(&e.source);
            if !src.exists() {
                continue;
            }
            let bytes = std::fs::read(&src).unwrap();
            let mapping = e.mapping.as_ref().map(|p| {
                let text = std::fs::read_to_string(repo().join(p)).unwrap();
                crate::import::coa::CoaMappingFile::parse(&text).unwrap()
            });
            let name = src.file_name().unwrap().to_string_lossy().into_owned();
            let (b, _) = crate::import::coa::build_template(
                &bytes,
                &name,
                mapping.as_ref(),
                &e.name,
                e.currency.as_deref().unwrap_or("USD"),
            )
            .unwrap();
            let built = crate::to_string(&b);
            let committed =
                std::fs::read_to_string(repo().join(format!("templates/charts/{}.mthd", e.id)))
                    .unwrap();
            assert_eq!(
                built, committed,
                "{} is out of date: run scripts/build-coa-templates.py",
                e.id
            );
            checked += 1;
        }
        assert!(checked > 0);
    }
}
