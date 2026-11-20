//! The imported budget's totals, worked out by our engine, against the
//! export's own figures: every line, account, category, fringe row,
//! charge and credit, the grand total and the net total.
//!
//! Figures in another currency than the base are compared the way the
//! export shows them: each line converted and rounded, each fringe's total
//! converted and rounded, then added up.

use std::collections::{BTreeMap, HashMap};

use method_core::{
    recalc, AccountTotals, CategoryTotals, Cents, FringePosting, RecalcOptions, Report, RowId,
    RowTotals,
};

use super::build::{at_places, cents, Imported};
use crate::setup::copy::money;

/// What a compared figure is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Line,
    Account,
    AccountFringes,
    FringeAccount,
    Category,
    CategoryFringes,
    FringeRow,
    Charge,
    Credit,
    GrandTotal,
    NetTotal,
}

impl Level {
    pub const ALL: [Level; 11] = [
        Level::Line,
        Level::Account,
        Level::AccountFringes,
        Level::FringeAccount,
        Level::Category,
        Level::CategoryFringes,
        Level::FringeRow,
        Level::Charge,
        Level::Credit,
        Level::GrandTotal,
        Level::NetTotal,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Level::Line => "line",
            Level::Account => "account",
            Level::AccountFringes => "account_fringes",
            Level::FringeAccount => "fringe_account",
            Level::Category => "category",
            Level::CategoryFringes => "category_fringes",
            Level::FringeRow => "fringe_row",
            Level::Charge => "charge",
            Level::Credit => "credit",
            Level::GrandTotal => "grand_total",
            Level::NetTotal => "net_total",
        }
    }

    /// One of them, in words.
    pub fn singular(self) -> &'static str {
        match self {
            Level::Line => "line",
            Level::Account => "account total",
            Level::AccountFringes => "account's fringes",
            Level::FringeAccount => "fringe account",
            Level::Category => "category total",
            Level::CategoryFringes => "category's fringes",
            Level::FringeRow => "Total Fringes row",
            Level::Charge => "charge",
            Level::Credit => "credit",
            Level::GrandTotal => "grand total",
            Level::NetTotal => "net total",
        }
    }

    /// Plural words for a count of them.
    pub fn label(self) -> &'static str {
        match self {
            Level::Line => "lines",
            Level::Account => "account totals",
            Level::AccountFringes => "account fringes",
            Level::FringeAccount => "fringe accounts",
            Level::Category => "category totals",
            Level::CategoryFringes => "category fringes",
            Level::FringeRow => "Total Fringes rows",
            Level::Charge => "charges",
            Level::Credit => "credits",
            Level::GrandTotal => "grand total",
            Level::NetTotal => "net total",
        }
    }
}

/// How many figures of one kind were compared, and how many differ.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub compared: usize,
    pub different: usize,
}

/// One figure that differs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    pub level: Level,
    /// What it is, in words (`1101 Z star: Grip`).
    pub what: String,
    /// The line's id, for a line.
    pub row: Option<RowId>,
    pub export: Cents,
    pub ours: Cents,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reconciliation {
    /// The currency the figures are in.
    pub currency: String,
    pub tallies: BTreeMap<Level, Tally>,
    /// Every differing figure, lines first.
    pub differences: Vec<Difference>,
    pub export_grand: Cents,
    pub export_net: Cents,
    /// Ours, when the budget recalculates.
    pub grand: Option<Cents>,
    pub net: Option<Cents>,
    /// Why the budget doesn't recalculate, when it doesn't.
    pub calc_error: Option<String>,
    /// Decimal places the figures are compared at (0: whole units).
    pub places: u8,
}

impl Reconciliation {
    /// How many figures were compared.
    pub fn compared(&self) -> usize {
        self.tallies.values().map(|t| t.compared).sum()
    }

    /// Whether every figure matches.
    pub fn matches(&self) -> bool {
        self.calc_error.is_none() && self.differences.is_empty()
    }

    fn check(
        &mut self,
        level: Level,
        what: impl FnOnce() -> String,
        row: Option<RowId>,
        export: Cents,
        ours: Cents,
    ) {
        let t = self.tallies.entry(level).or_default();
        t.compared += 1;
        if at_places(export, self.places) != at_places(ours, self.places) {
            t.different += 1;
            self.differences.push(Difference {
                level,
                what: what(),
                row,
                export,
                ours,
            });
        }
    }
}

/// How the export would show base-currency figures.
struct Shown<'a> {
    r: &'a Report,
    converted: bool,
}

impl Shown<'_> {
    fn one(&self, c: Cents) -> Cents {
        if self.converted {
            self.r.in_totals_currency(c).unwrap_or(c)
        } else {
            c
        }
    }

    /// Fringes by name, each converted on its own, added up.
    fn fringes(&self, by_name: &BTreeMap<String, Cents>) -> Cents {
        by_name.values().map(|c| self.one(*c)).sum()
    }

    fn row_fringes(rows: &[&RowTotals]) -> BTreeMap<String, Cents> {
        let mut m: BTreeMap<String, Cents> = BTreeMap::new();
        for r in rows {
            for (k, v) in &r.fringes {
                *m.entry(k.clone()).or_default() += v;
            }
        }
        m
    }

    fn account_lines(&self, a: &AccountTotals) -> Cents {
        if self.converted {
            a.rows.iter().map(|r| self.one(r.total)).sum()
        } else {
            a.details_total
        }
    }

    fn account_fringes(&self, a: &AccountTotals) -> Cents {
        if self.converted {
            self.fringes(&Self::row_fringes(&a.rows.iter().collect::<Vec<_>>()))
        } else {
            a.fringe_total
        }
    }

    fn account_total(&self, a: &AccountTotals, by_account: bool) -> Cents {
        if !self.converted {
            return a.total;
        }
        self.account_lines(a)
            + if by_account {
                self.account_fringes(a)
            } else {
                0
            }
    }

    fn category_fringes(&self, c: &CategoryTotals) -> Cents {
        if self.converted {
            let rows: Vec<&RowTotals> = c.accounts.iter().flat_map(|a| &a.rows).collect();
            self.fringes(&Self::row_fringes(&rows))
        } else {
            c.fringe_total
        }
    }

    fn fringe_account(&self, c: &CategoryTotals) -> Cents {
        match &c.fringe_account {
            Some(_) if self.converted => self.category_fringes(c),
            Some((_, amount)) => *amount,
            None => 0,
        }
    }

    fn category_total(&self, c: &CategoryTotals, posting: &FringePosting) -> Cents {
        if !self.converted {
            return c.total;
        }
        let by_account = matches!(posting, FringePosting::Account);
        c.accounts
            .iter()
            .map(|a| self.account_total(a, by_account))
            .sum::<Cents>()
            + self.fringe_account(c)
    }
}

/// Compares `im`'s budget, recalculated, with the export's figures.
pub fn reconcile(im: &Imported) -> Reconciliation {
    let b = &im.budget;
    let links = &im.links;
    let mut out = Reconciliation {
        currency: im.currency.clone(),
        places: im.places,
        ..Reconciliation::default()
    };
    let export_grand: Cents = links
        .categories
        .iter()
        .map(|c| cents(c.total))
        .sum::<Cents>()
        + links.level_fringes.iter().map(|v| cents(*v)).sum::<Cents>()
        + links.budget_fringes.map_or(0, cents)
        + links.charges.iter().map(|(_, v)| cents(*v)).sum::<Cents>();
    out.export_grand = export_grand;
    out.export_net = export_grand + links.credits.iter().map(|(_, v)| cents(*v)).sum::<Cents>();
    let r = match recalc(b, &RecalcOptions::default()) {
        Ok(r) => r,
        Err(e) => {
            out.calc_error = Some(e.to_string());
            return out;
        }
    };
    let shown = Shown {
        r: &r,
        converted: b
            .totals_currency
            .as_ref()
            .is_some_and(|c| *c != b.base_currency),
    };

    let rows: HashMap<RowId, &RowTotals> = r
        .categories
        .iter()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.rows)
        .map(|row| (row.id, row))
        .collect();
    for l in &links.lines {
        let ours = rows.get(&l.id).map_or(0, |row| shown.one(row.total));
        out.check(
            Level::Line,
            || {
                let d = l.description.trim();
                format!(
                    "{}: {}",
                    l.account,
                    if d.is_empty() { "(no description)" } else { d }
                )
            },
            Some(l.id),
            cents(l.total),
            ours,
        );
    }

    let by_account = matches!(b.fringe_posting, FringePosting::Account);
    let accounts: HashMap<&str, &AccountTotals> = r
        .categories
        .iter()
        .flat_map(|c| &c.accounts)
        .map(|a| (a.code.as_str(), a))
        .collect();
    for a in &links.accounts {
        let ours = accounts.get(a.code.as_str());
        let label = || format!("{} {}", a.code, a.name.trim());
        out.check(
            Level::Account,
            label,
            None,
            cents(a.total),
            ours.map_or(0, |t| shown.account_total(t, by_account)),
        );
        out.check(
            Level::AccountFringes,
            label,
            None,
            cents(a.fringe),
            ours.map_or(0, |t| shown.account_fringes(t)),
        );
    }
    for (code, total) in &links.account_fringe_rows {
        let ours = accounts
            .get(code.as_str())
            .map_or(0, |t| shown.account_fringes(t));
        out.check(
            Level::FringeRow,
            || format!("{code} Total Fringes"),
            None,
            cents(*total),
            ours,
        );
    }

    let categories: HashMap<&str, &CategoryTotals> =
        r.categories.iter().map(|c| (c.code.as_str(), c)).collect();
    for c in &links.categories {
        let ours = categories.get(c.code.as_str());
        let label = || format!("{} {}", c.code, c.name.trim());
        out.check(
            Level::Category,
            label,
            None,
            cents(c.total),
            ours.map_or(0, |t| shown.category_total(t, &b.fringe_posting)),
        );
        out.check(
            Level::CategoryFringes,
            label,
            None,
            cents(c.fringe),
            ours.map_or(0, |t| shown.category_fringes(t)),
        );
    }
    for (cat, number, total) in &links.fringe_accounts {
        let ours = categories
            .get(cat.as_str())
            .map_or(0, |t| shown.fringe_account(t));
        out.check(
            Level::FringeAccount,
            || format!("{number} Total Fringes"),
            None,
            cents(*total),
            ours,
        );
    }

    // Fringe rows on the topsheet.
    if !links.level_fringes.is_empty() {
        let closed: Vec<_> = r.levels.iter().filter(|l| !l.name.is_empty()).collect();
        for (i, total) in links.level_fringes.iter().enumerate() {
            let ours = closed.get(i).map_or(0, |l| {
                if shown.converted {
                    let rows: Vec<&RowTotals> = r.categories[l.start..l.end]
                        .iter()
                        .flat_map(|c| &c.accounts)
                        .flat_map(|a| &a.rows)
                        .collect();
                    shown.fringes(&Shown::row_fringes(&rows))
                } else {
                    l.posted_fringes
                }
            });
            out.check(
                Level::FringeRow,
                || format!("Total Fringes before production total {}", i + 1),
                None,
                cents(*total),
                ours,
            );
        }
    }
    if let Some(total) = links.budget_fringes {
        let ours = if shown.converted {
            let rows: Vec<&RowTotals> = r
                .categories
                .iter()
                .flat_map(|c| &c.accounts)
                .flat_map(|a| &a.rows)
                .collect();
            shown.fringes(&Shown::row_fringes(&rows))
        } else {
            r.budget_fringes
        };
        out.check(
            Level::FringeRow,
            || "Total Fringes".to_string(),
            None,
            cents(total),
            ours,
        );
    }
    for (i, (name, total)) in links.charges.iter().enumerate() {
        let ours = r.charges.get(i).map_or(0, |c| shown.one(c.amount));
        out.check(Level::Charge, || name.clone(), None, cents(*total), ours);
    }
    for (i, (name, total)) in links.credits.iter().enumerate() {
        let ours = r.credits.get(i).map_or(0, |c| -shown.one(c.amount));
        out.check(Level::Credit, || name.clone(), None, cents(*total), ours);
    }

    let grand = if shown.converted {
        let cats: Cents = r
            .categories
            .iter()
            .map(|c| shown.category_total(c, &b.fringe_posting))
            .sum();
        let posted: Cents = if links.budget_fringes.is_some() || !links.level_fringes.is_empty() {
            let rows: Vec<&RowTotals> = r
                .categories
                .iter()
                .flat_map(|c| &c.accounts)
                .flat_map(|a| &a.rows)
                .collect();
            shown.fringes(&Shown::row_fringes(&rows))
        } else {
            0
        };
        cats + posted + r.charges.iter().map(|c| shown.one(c.amount)).sum::<Cents>()
    } else {
        r.grand_total
    };
    let credits: Cents = r.credits.iter().map(|c| shown.one(c.amount)).sum();
    let net = if shown.converted {
        grand - credits
    } else {
        r.net_total
    };
    out.grand = Some(grand);
    out.net = Some(net);
    out.check(
        Level::GrandTotal,
        || "Grand total".to_string(),
        None,
        export_grand,
        grand,
    );
    out.check(
        Level::NetTotal,
        || "Net total".to_string(),
        None,
        out.export_net,
        net,
    );
    out
}

/// The reconciliation in words: one line per kind of figure, then the
/// differences (all of them, or the first `limit`).
pub fn summary_lines(rec: &Reconciliation, limit: Option<usize>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(e) = &rec.calc_error {
        out.push(format!("The imported budget doesn't recalculate: {e}"));
        return out;
    }
    let cur = &rec.currency;
    out.push(format!(
        "Grand total: export {} {cur}, imported {} {cur}{}",
        money(rec.export_grand),
        rec.grand.map_or("?".into(), money),
        if rec.grand.map(|g| at_places(g, rec.places))
            == Some(at_places(rec.export_grand, rec.places))
        {
            " (same)"
        } else {
            ""
        }
    ));
    if rec.export_net != rec.export_grand || rec.net != rec.grand {
        out.push(format!(
            "Net total: export {} {cur}, imported {} {cur}{}",
            money(rec.export_net),
            rec.net.map_or("?".into(), money),
            if rec.net.map(|g| at_places(g, rec.places))
                == Some(at_places(rec.export_net, rec.places))
            {
                " (same)"
            } else {
                ""
            }
        ));
    }
    for level in Level::ALL {
        let Some(t) = rec.tallies.get(&level) else {
            continue;
        };
        if matches!(level, Level::GrandTotal | Level::NetTotal) {
            continue;
        }
        out.push(format!(
            "{} {}: {}",
            t.compared,
            if t.compared == 1 {
                level.singular()
            } else {
                level.label()
            },
            match (t.compared, t.different) {
                (1, 0) => "the same".to_string(),
                (_, 0) => "all the same".to_string(),
                (1, _) => "different".to_string(),
                (_, d) => format!("{d} different"),
            }
        ));
    }
    let n = rec.differences.len();
    let shown = limit.map_or(n, |l| l.min(n));
    for d in &rec.differences[..shown] {
        out.push(format!(
            "  {}{}: export {}, imported {} ({})",
            match d.level {
                Level::Line => "line ",
                Level::Account => "account ",
                Level::AccountFringes => "account fringes ",
                Level::FringeAccount => "fringe account ",
                Level::Category => "category ",
                Level::CategoryFringes => "category fringes ",
                Level::FringeRow => "fringe row ",
                Level::Charge => "charge ",
                Level::Credit => "credit ",
                Level::GrandTotal | Level::NetTotal => "",
            },
            d.what,
            money(d.export),
            money(d.ours),
            signed(d.ours - d.export)
        ));
    }
    if shown < n {
        out.push(format!("  ... and {} more differences", n - shown));
    }
    out
}

fn signed(c: Cents) -> String {
    if c > 0 {
        format!("+{}", money(c))
    } else {
        money(c)
    }
}

#[cfg(test)]
mod tests {
    use super::super::build::{LineLink, Links};
    use super::*;

    /// A fixture export (invented figures), read and built but not yet
    /// reconciled.
    fn imported(name: &str) -> Imported {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/exchange")
            .join(name);
        let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        super::super::import(&bytes, name, None)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .0
    }

    fn empty(places: u8) -> Reconciliation {
        Reconciliation {
            currency: "USD".into(),
            places,
            ..Reconciliation::default()
        }
    }

    #[test]
    fn every_level_has_its_own_key() {
        let keys: std::collections::BTreeSet<&str> = Level::ALL.iter().map(|l| l.key()).collect();
        assert_eq!(keys.len(), Level::ALL.len());
    }

    #[test]
    fn an_export_with_nothing_in_it_matches_an_empty_budget() {
        let mut im = imported("by-budget-credits.json");
        im.budget = method_core::starter::blank("Field Station", "USD");
        im.links = Links::default();
        let rec = reconcile(&im);
        assert!(rec.matches(), "{:?}", rec.differences);
        assert_eq!((rec.export_grand, rec.export_net), (0, 0));
        assert_eq!((rec.grand, rec.net), (Some(0), Some(0)));
        // Only the grand and net totals are compared.
        assert_eq!(rec.compared(), 2);
        assert_eq!(rec.tallies[&Level::GrandTotal].compared, 1);
    }

    #[test]
    fn a_line_figure_that_differs_is_named_with_its_row() {
        let mut im = imported("by-budget-credits.json");
        assert!(reconcile(&im).matches());
        let line = &mut im.links.lines[0];
        let row = line.id;
        line.total += 1.0;
        let rec = reconcile(&im);
        assert!(!rec.matches());
        let lines: Vec<&Difference> = rec
            .differences
            .iter()
            .filter(|d| d.level == Level::Line)
            .collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].row, Some(row));
        assert_eq!(lines[0].export - lines[0].ours, 100);
        assert_eq!(rec.tallies[&Level::Line].different, 1);
        // The export's grand total adds up its categories, which didn't
        // change, so it still matches.
        assert_eq!(rec.grand, Some(rec.export_grand));
    }

    #[test]
    fn a_line_the_budget_lacks_counts_as_nothing() {
        let mut im = imported("by-budget-credits.json");
        let before = reconcile(&im).tallies[&Level::Line].compared;
        im.links.lines.push(LineLink {
            id: u64::MAX,
            account: "9999".into(),
            description: "  ".into(),
            total: 12.5,
        });
        let rec = reconcile(&im);
        assert_eq!(rec.tallies[&Level::Line].compared, before + 1);
        let d = rec.differences.last().unwrap();
        assert_eq!((d.export, d.ours), (1_250, 0));
        assert_eq!(d.what, "9999: (no description)");
    }

    #[test]
    fn whole_units_compare_rounded() {
        let mut rec = empty(0);
        // 10.49 and 10.00 are both 10; 10.50 rounds up to 11.
        rec.check(Level::Line, || "a".into(), None, 1_049, 1_000);
        rec.check(Level::Line, || "b".into(), None, 1_050, 1_000);
        rec.check(Level::Line, || "c".into(), None, -1_049, -1_000);
        assert_eq!(
            rec.tallies[&Level::Line],
            Tally {
                compared: 3,
                different: 1
            }
        );
        assert_eq!(rec.differences[0].what, "b");
        // At two places every cent counts.
        let mut rec = empty(2);
        rec.check(Level::Line, || "a".into(), None, 1_001, 1_000);
        assert_eq!(rec.differences.len(), 1);
    }

    #[test]
    fn a_budget_that_does_not_recalculate_is_said_in_one_line() {
        let rec = Reconciliation {
            calc_error: Some("a formula refers to itself".into()),
            ..empty(2)
        };
        assert!(!rec.matches());
        assert_eq!(
            summary_lines(&rec, None),
            ["The imported budget doesn't recalculate: a formula refers to itself"]
        );
    }

    #[test]
    fn the_summary_counts_kinds_and_stops_at_the_limit() {
        let mut rec = empty(2);
        rec.export_grand = 10_000;
        rec.export_net = 10_000;
        rec.grand = Some(10_000);
        rec.net = Some(10_000);
        rec.check(Level::Account, || "1100 Story".into(), None, 500, 500);
        for (i, ours) in [600, 700, 800].into_iter().enumerate() {
            rec.check(Level::Line, || format!("1100: line {i}"), None, 500, ours);
        }
        let lines = summary_lines(&rec, Some(2));
        assert_eq!(
            lines,
            [
                "Grand total: export 100.00 USD, imported 100.00 USD (same)",
                "3 lines: 3 different",
                "1 account total: the same",
                "  line 1100: line 0: export 5.00, imported 6.00 (+1.00)",
                "  line 1100: line 1: export 5.00, imported 7.00 (+2.00)",
                "  ... and 1 more differences",
            ]
        );
        // A net total that differs from the grand gets its own line.
        rec.export_net = 9_000;
        rec.net = Some(9_100);
        let lines = summary_lines(&rec, Some(0));
        assert_eq!(lines[1], "Net total: export 90.00 USD, imported 91.00 USD");
        assert_eq!(lines.last().unwrap(), "  ... and 3 more differences");
    }
}
