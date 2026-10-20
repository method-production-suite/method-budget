//! The start window's projects and thumbnails, without files (roadmap 3.19,
//! phase 2; decisions.md, entry 91; design: docs/research/new-project-window.md,
//! sections 4 and 5). The desktop (method-app's src/start.rs) reads the
//! folders and the files; this decides what each thing is, how a project's
//! files are arranged, and what a budget's thumbnail shows.
//!
//! - **Thumbnails** are a budget's topsheet as bars: one per category, scaled
//!   to its share of the categories' total, made from the totals the start
//!   window already reads. No picture is drawn from the screen or kept in the
//!   budget file; the desktop keeps the bars with each file's summary in its
//!   own data folder, read again only when the file changes.
//! - **A project is a folder.** Its budgets are grouped: each budget, then
//!   the sub-budgets and copies made from it (matched by title, as a returned
//!   copy finds its original), then whatever came from a budget that isn't
//!   in the folder, setup packs, schedule days files and actuals exports.

use std::cmp::Ordering;

use method_core::{Budget, ProvenanceKind, Report};
use serde::{Deserialize, Serialize};

/// Most bars a thumbnail has; more categories share the last bar.
pub const THUMB_BARS: usize = 12;

/// One bar of a budget's thumbnail.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThumbBar {
    /// The category's number and name ("21 Camera"), or how many categories
    /// share the last bar ("8 more").
    pub label: String,
    /// Its share of the categories' total, in thousandths (0 to 1000).
    pub share: u16,
}

/// A budget's thumbnail: a bar per category, in topsheet order, scaled to
/// its share of what the categories come to (a category below zero counts
/// as nothing). With more than [`THUMB_BARS`] categories, the largest ones
/// keep their own bar, in their places, and the rest share the last one.
/// Empty when no category has a total above zero.
pub fn thumbnail(report: &Report) -> Vec<ThumbBar> {
    let cats: Vec<(String, i64)> = report
        .categories
        .iter()
        .map(|c| {
            let label = match c.name.trim() {
                "" => c.code.clone(),
                n => format!("{} {n}", c.code),
            };
            (label, c.total.max(0))
        })
        .collect();
    let sum: i128 = cats.iter().map(|(_, t)| i128::from(*t)).sum();
    if sum <= 0 {
        return Vec::new();
    }
    // Which categories keep a bar of their own.
    let own: Vec<bool> = if cats.len() <= THUMB_BARS {
        vec![true; cats.len()]
    } else {
        let mut by_size: Vec<usize> = (0..cats.len()).collect();
        by_size.sort_by(|a, b| match cats[*b].1.cmp(&cats[*a].1) {
            Ordering::Equal => a.cmp(b),
            o => o,
        });
        let mut own = vec![false; cats.len()];
        for i in by_size.into_iter().take(THUMB_BARS - 1) {
            own[i] = true;
        }
        own
    };
    let share = |t: i128| -> u16 {
        // Rounded to the nearest thousandth; t <= sum, so at most 1000.
        u16::try_from((t * 1000 + sum / 2) / sum).unwrap_or(1000)
    };
    let mut bars: Vec<ThumbBar> = cats
        .iter()
        .zip(&own)
        .filter(|(_, own)| **own)
        .map(|((label, t), _)| ThumbBar {
            label: label.clone(),
            share: share(i128::from(*t)),
        })
        .collect();
    let rest: Vec<i64> = cats
        .iter()
        .zip(&own)
        .filter(|(_, own)| !**own)
        .map(|((_, t), _)| *t)
        .collect();
    if !rest.is_empty() {
        bars.push(ThumbBar {
            label: format!("{} more", rest.len()),
            share: share(rest.iter().map(|t| i128::from(*t)).sum()),
        });
    }
    bars
}

// ---------------------------------------------------------------------------
// Projects

/// What a file in a project folder is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// A budget of its own.
    Budget,
    /// A filtered copy of a budget (File > New Sub-Budget, or from a row).
    SubBudget,
    /// A copy saved for sharing (opens read-only).
    SharedCopy,
    /// An editable copy of a shared copy.
    EditableCopy,
    /// A setup pack.
    SetupPack,
    /// A schedule's days, written by Method Schedule.
    Schedule,
    /// An accounting export in an `actuals` folder.
    Actuals,
}

impl ItemKind {
    /// Whether the start window opens it as a budget.
    pub fn is_budget_file(self) -> bool {
        !matches!(self, ItemKind::Schedule | ItemKind::Actuals)
    }

    /// The plain word the start window shows.
    pub fn word(self) -> &'static str {
        match self {
            ItemKind::Budget => "budget",
            ItemKind::SubBudget => "sub-budget",
            ItemKind::SharedCopy => "shared copy",
            ItemKind::EditableCopy => "editable copy",
            ItemKind::SetupPack => "setup pack",
            ItemKind::Schedule => "schedule",
            ItemKind::Actuals => "actuals",
        }
    }
}

/// What budget `b` is, from its provenance, and the title of the budget it
/// came from (for a sub-budget or a copy).
pub fn kind_of(b: &Budget) -> (ItemKind, Option<String>) {
    let Some(p) = &b.provenance else {
        return (ItemKind::Budget, None);
    };
    let kind = match p.kind {
        ProvenanceKind::SubBudget => ItemKind::SubBudget,
        ProvenanceKind::SharedCopy => ItemKind::SharedCopy,
        ProvenanceKind::EditableCopy => ItemKind::EditableCopy,
        ProvenanceKind::SetupPack => return (ItemKind::SetupPack, None),
    };
    let source = p.source_title.trim();
    (kind, (!source.is_empty()).then(|| source.to_string()))
}

/// Whether the start of a `.json` file (`head`, its first few kilobytes)
/// says it is a schedule's days file (`"format": "method-schedule-days"`,
/// docs/research/schedule-exchange.md).
pub fn looks_like_schedule_days(head: &str) -> bool {
    let Some(at) = head.find("\"format\"") else {
        return false;
    };
    let rest = head[at + "\"format\"".len()..].trim_start();
    rest.strip_prefix(':')
        .map(str::trim_start)
        .is_some_and(|v| v.starts_with("\"method-schedule-days\""))
}

/// Whether a file at `rel` (its path inside the project folder, as its
/// parts) is an actuals export: a `.csv`, `.tsv` or `.xlsx` file in a folder
/// named `actuals` (any case).
pub fn is_actuals_file(rel: &[&str]) -> bool {
    let Some((name, dirs)) = rel.split_last() else {
        return false;
    };
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    matches!(ext.as_deref(), Some("csv" | "tsv" | "xlsx"))
        && dirs.iter().any(|d| d.eq_ignore_ascii_case("actuals"))
}

/// One thing found in a project folder, for [`arrange`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub kind: ItemKind,
    /// The budget's title, else the file's name.
    pub title: String,
    /// For a sub-budget or a copy: the title of the budget it came from.
    pub source: Option<String>,
    /// Its path inside the folder, for the order of things with one title.
    pub file: String,
}

/// Where a found thing goes in a project's list: its index in what was
/// found, and whether it is shown under the budget above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub index: usize,
    pub under: bool,
}

fn same_title(a: &str, b: &str) -> bool {
    let a = a.trim();
    !a.is_empty() && a.to_lowercase() == b.trim().to_lowercase()
}

fn by_title(a: &Found, b: &Found) -> Ordering {
    a.title
        .to_lowercase()
        .cmp(&b.title.to_lowercase())
        .then_with(|| a.file.cmp(&b.file))
}

/// A project's list: each budget (by title), with the sub-budgets and
/// copies whose source is its title under it (sub-budgets first, then
/// copies, each by title); then sub-budgets and copies of budgets not in
/// the folder; then setup packs, schedules and actuals, each by title.
pub fn arrange(found: &[Found]) -> Vec<Placed> {
    let mut idx: Vec<usize> = (0..found.len()).collect();
    idx.sort_by(|a, b| {
        found[*a]
            .kind
            .cmp(&found[*b].kind)
            .then_with(|| by_title(&found[*a], &found[*b]))
    });
    let budgets: Vec<usize> = idx
        .iter()
        .copied()
        .filter(|i| found[*i].kind == ItemKind::Budget)
        .collect();
    let derived = |k: ItemKind| {
        matches!(
            k,
            ItemKind::SubBudget | ItemKind::SharedCopy | ItemKind::EditableCopy
        )
    };
    let mut out = Vec::with_capacity(found.len());
    let mut placed = vec![false; found.len()];
    for &b in &budgets {
        out.push(Placed {
            index: b,
            under: false,
        });
        placed[b] = true;
        for &i in &idx {
            let f = &found[i];
            if !placed[i]
                && derived(f.kind)
                && f.source
                    .as_deref()
                    .is_some_and(|s| same_title(s, &found[b].title))
            {
                out.push(Placed {
                    index: i,
                    under: true,
                });
                placed[i] = true;
            }
        }
    }
    for &i in &idx {
        if !placed[i] {
            out.push(Placed {
                index: i,
                under: false,
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Shared budgets

/// Most people a shared budget's row names.
pub const PEOPLE_SHOWN: usize = 5;

/// One person's last heartbeat in a shared budget (the change log's
/// `presence.json`, decisions.md entries 62 and 64), as the desktop reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Heartbeat {
    pub name: String,
    pub category: Option<String>,
    pub account: Option<String>,
    /// When it was written, ISO 8601 UTC.
    pub at: String,
}

/// Someone who was in a shared budget, for its row in the start window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    /// Where they were: "account 2101", "category 21", or empty.
    pub place: String,
    /// When, ISO 8601 UTC, as written.
    pub at: String,
}

/// Who was in a shared budget, latest first: one row per name (trimmed,
/// ignoring case; their latest heartbeat), nameless heartbeats left out, at
/// most [`PEOPLE_SHOWN`]. Times compare as written (ISO 8601 UTC sorts as
/// text).
pub fn people(beats: impl IntoIterator<Item = Heartbeat>) -> Vec<Person> {
    let mut all: Vec<Heartbeat> = beats
        .into_iter()
        .filter(|h| !h.name.trim().is_empty())
        .collect();
    all.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.name.cmp(&b.name)));
    let mut seen = std::collections::BTreeSet::new();
    all.into_iter()
        .filter(|h| seen.insert(h.name.trim().to_lowercase()))
        .take(PEOPLE_SHOWN)
        .map(|h| Person {
            name: h.name.trim().to_string(),
            place: match (h.account, h.category) {
                (Some(a), _) if !a.trim().is_empty() => format!("account {}", a.trim()),
                (_, Some(c)) if !c.trim().is_empty() => format!("category {}", c.trim()),
                _ => String::new(),
            },
            at: h.at,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::{recalc, starter, RecalcOptions};

    fn report(b: &Budget) -> Report {
        recalc(b, &RecalcOptions::default()).unwrap()
    }

    #[test]
    fn a_thumbnail_is_a_bar_per_category_by_its_share() {
        let r = report(&starter::budget());
        let bars = thumbnail(&r);
        assert_eq!(bars.len(), r.categories.len());
        assert_eq!(bars[0].label, "10 Story and rights");
        let sum: i64 = r.categories.iter().map(|c| c.total).sum();
        for (bar, c) in bars.iter().zip(&r.categories) {
            let want = (i128::from(c.total) * 1000 + i128::from(sum) / 2) / i128::from(sum);
            assert_eq!(i128::from(bar.share), want, "{}", bar.label);
        }
        let total: i32 = bars.iter().map(|b| i32::from(b.share)).sum();
        assert!((995..=1005).contains(&total), "{total}");
        // Nothing to draw for a budget with no totals.
        assert!(thumbnail(&report(&starter::blank("Empty", "USD"))).is_empty());
    }

    #[test]
    fn many_categories_keep_the_largest_and_share_the_last_bar() {
        let mut r = report(&starter::budget());
        let template = r.categories[0].clone();
        r.categories = (1..=20)
            .map(|n| {
                let mut c = template.clone();
                c.code = format!("{n:02}");
                c.name = String::new();
                c.total = if n == 5 { -100 } else { i64::from(n) * 100 };
                c
            })
            .collect();
        let bars = thumbnail(&r);
        assert_eq!(bars.len(), THUMB_BARS);
        // The 11 largest (10 to 20, not 5, which is below zero) in their places.
        let own: Vec<&str> = bars[..THUMB_BARS - 1]
            .iter()
            .map(|b| b.label.as_str())
            .collect();
        let want: Vec<String> = (10..=20).map(|n| format!("{n:02}")).collect();
        assert_eq!(own, want);
        assert_eq!(bars[THUMB_BARS - 1].label, "9 more");
        let sum: i64 = (1..=20).filter(|n| *n != 5).map(|n| n * 100).sum();
        let rest: i64 = (1..=9).filter(|n| *n != 5).map(|n| n * 100).sum();
        assert_eq!(
            i64::from(bars[THUMB_BARS - 1].share),
            (rest * 1000 + sum / 2) / sum
        );
    }

    #[test]
    fn what_a_budget_is_comes_from_its_provenance() {
        let mut b = starter::budget();
        assert_eq!(kind_of(&b), (ItemKind::Budget, None));
        b.title = "Big Show".into();
        let row = crate::sharing::RowInput {
            kind: "category".into(),
            code: "21".into(),
        };
        let sub =
            crate::sharing::make_row_sub_budget(&b, &row, "2026-10-20T10:00:00-07:00").unwrap();
        assert_eq!(
            kind_of(&sub),
            (ItemKind::SubBudget, Some("Big Show".into()))
        );
        assert!(ItemKind::Budget.is_budget_file());
        assert!(!ItemKind::Schedule.is_budget_file());
        assert_eq!(ItemKind::SubBudget.word(), "sub-budget");
    }

    #[test]
    fn schedule_days_and_actuals_are_recognized() {
        assert!(looks_like_schedule_days(
            "{\n  \"format\" : \"method-schedule-days\",\n  \"version\": 1"
        ));
        assert!(!looks_like_schedule_days(
            "{\"format\": \"something-else\"}"
        ));
        assert!(!looks_like_schedule_days(
            "{\"title\": \"method-schedule-days\"}"
        ));
        assert!(is_actuals_file(&["actuals", "2026-09 GL.csv"]));
        assert!(is_actuals_file(&["Actuals", "ledger.XLSX"]));
        assert!(!is_actuals_file(&["2026-09 GL.csv"]));
        assert!(!is_actuals_file(&["actuals", "notes.txt"]));
    }

    fn found(kind: ItemKind, title: &str, source: Option<&str>) -> Found {
        Found {
            kind,
            title: title.into(),
            source: source.map(str::to_string),
            file: format!("{title}.mthd"),
        }
    }

    #[test]
    fn a_project_lists_each_budget_with_what_came_from_it() {
        let f = vec![
            found(ItemKind::Schedule, "Big Show.days.json", None),
            found(ItemKind::SharedCopy, "Big Show", Some("big show")),
            found(ItemKind::SubBudget, "Big Show - Camera", Some("Big Show")),
            found(ItemKind::Budget, "Pilot", None),
            found(ItemKind::SubBudget, "Elsewhere - Cast", Some("Elsewhere")),
            found(ItemKind::Budget, "Big Show", None),
            found(ItemKind::Actuals, "2026-09 GL.csv", None),
            found(ItemKind::SetupPack, "Union rates", None),
        ];
        let order: Vec<(&str, ItemKind, bool)> = arrange(&f)
            .into_iter()
            .map(|p| (f[p.index].title.as_str(), f[p.index].kind, p.under))
            .collect();
        assert_eq!(
            order,
            [
                ("Big Show", ItemKind::Budget, false),
                ("Big Show - Camera", ItemKind::SubBudget, true),
                ("Big Show", ItemKind::SharedCopy, true),
                ("Pilot", ItemKind::Budget, false),
                ("Elsewhere - Cast", ItemKind::SubBudget, false),
                ("Union rates", ItemKind::SetupPack, false),
                ("Big Show.days.json", ItemKind::Schedule, false),
                ("2026-09 GL.csv", ItemKind::Actuals, false),
            ]
        );
        assert!(arrange(&[]).is_empty());
    }

    #[test]
    fn a_shared_budgets_people_latest_first_once_each() {
        let beat = |name: &str, cat: Option<&str>, acct: Option<&str>, at: &str| Heartbeat {
            name: name.into(),
            category: cat.map(str::to_string),
            account: acct.map(str::to_string),
            at: at.into(),
        };
        let p = people([
            beat("Ann", Some("21"), None, "2026-10-19T10:00:00Z"),
            beat("Bo", Some("21"), Some("2101"), "2026-10-20T09:00:00Z"),
            beat(" ann ", None, None, "2026-10-20T08:00:00Z"),
            beat("", Some("10"), None, "2026-10-20T10:00:00Z"),
        ]);
        let rows: Vec<(&str, &str, &str)> = p
            .iter()
            .map(|x| (x.name.as_str(), x.place.as_str(), x.at.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("Bo", "account 2101", "2026-10-20T09:00:00Z"),
                ("ann", "", "2026-10-20T08:00:00Z"),
            ]
        );
        let many = (0..9).map(|n| {
            beat(
                &format!("P{n}"),
                None,
                None,
                &format!("2026-10-20T0{n}:00:00Z"),
            )
        });
        let p = people(many);
        assert_eq!(p.len(), PEOPLE_SHOWN);
        assert_eq!(p[0].name, "P8");
    }
}
