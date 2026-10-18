//! File > Import Setup From…: setup items picked from another budget file
//! (roadmap 1.20; decisions.md, entries 15, 23 and 29).
//!
//! - **The source.** Rust shows the Open dialog and reads the file (a file
//!   open in a tab is read from the tab, so its unsaved changes count). Paths
//!   never come from the page; it names the source by a token afterwards.
//! - **The review.** Every setup item in the source is listed by kind with
//!   how it stands here: new, the same, or different under the same name.
//!   Each item also lists what it needs (a fringe's flat unit and the globals
//!   in its rate, the globals a global names, a charge's excluded groups and
//!   currency, a credit's groups and excluded fringes, a tag preset's tags,
//!   what a line block's rows use, a report preset's groups), transitively,
//!   so the page can tick them along.
//! - **Importing** brings the picked items and everything they need. New
//!   items are added; items that differ keep this budget's version
//!   (decision 12) unless the user picks the imported one or "add it under a
//!   new name", which the imported items that refer to it then use. One undo
//!   step. The same dependency walk, conflict rule, renaming and definitions
//!   in words as pasting rows (src/clipboard.rs), which this reuses.
//!
//! Showing the Open dialog and reading the file are the host's (the
//! desktop's src/import_setup.rs, which reads a file open in a tab from the
//! tab); this reviews and imports the budget it hands in.

use std::sync::{Arc, Mutex, MutexGuard};

use method_core::Budget;
use serde::Serialize;

use crate::edit::EditResult;
use crate::session::Session;
use crate::view::View;

// The review and the import themselves are plain functions on budgets in
// method_format::setup::import, shared with the command line (`method-budget pack`).
pub use method_format::setup::import::{
    all_items, closure, count, describe, import, item_ref, preview, ImportItem, ImportPreview,
    Imported, ItemRef, Resolution, What,
};

// ---------------------------------------------------------------------------
// The source, kept by token between the review and the import

struct Source {
    token: u64,
    label: String,
    budget: Arc<Budget>,
}

/// The last budget picked to import from (the app's state; one for every
/// budget).
#[derive(Default)]
pub struct ImportSources {
    inner: Mutex<(u64, Option<Source>)>,
}

impl ImportSources {
    fn lock(&self) -> MutexGuard<'_, (u64, Option<Source>)> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Keeps `budget` and returns its token.
    pub fn keep(&self, label: String, budget: Budget) -> u64 {
        let mut g = self.lock();
        g.0 += 1;
        let token = g.0;
        g.1 = Some(Source {
            token,
            label,
            budget: Arc::new(budget),
        });
        token
    }

    pub fn get(&self, token: u64) -> Option<(String, Arc<Budget>)> {
        let g = self.lock();
        g.1.as_ref()
            .filter(|s| s.token == token)
            .map(|s| (s.label.clone(), Arc::clone(&s.budget)))
    }
}

/// What `import_setup` did: the view, and a sentence saying so.
#[derive(Debug, Serialize)]
pub struct ImportDone {
    pub view: View,
    pub message: String,
    #[serde(flatten)]
    pub counts: Imported,
}

impl ImportDone {
    pub fn new(view: View, counts: Imported) -> ImportDone {
        ImportDone {
            view,
            message: counts.message(),
            counts,
        }
    }
}

/// The label a picked file's source goes by: its name without the
/// extension.
pub fn source_label(file: &str) -> String {
    std::path::Path::new(file)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.to_string())
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/import_setup.rs and the host's
// src/host/import_setup.rs call these)

/// The review of importing setup from `source` (named `label`, read from
/// `from`, a file shown by its name or path) into budget `s`, keeping the
/// source by the review's token.
pub fn import_setup_pick(
    s: &Session,
    sources: &ImportSources,
    label: String,
    source: Budget,
    from: &str,
) -> ImportPreview {
    let p = preview(s.budget(), &source, 0, &label, from);
    let token = sources.keep(label, source);
    ImportPreview { token, ..p }
}

/// Imports the picked items (and what they need) from the reviewed source
/// with `token` into budget `s`, as one undo step. What came in.
pub fn import_setup(
    s: &mut Session,
    sources: &ImportSources,
    token: u64,
    picks: &[ItemRef],
    resolutions: &[Resolution],
) -> EditResult<Imported> {
    let (_, src) = sources
        .get(token)
        .ok_or("Another file was picked since. Import again.")?;
    let label = format!(
        "Import {} of setup",
        crate::history::count(picks.len(), "item", "items")
    );
    s.edit(label, |b| import(b, &src, picks, resolutions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use method_core::{
        starter, Charge, ChargeKind, Credit, CreditCap, CreditSource, Currency, Dec, Fringe,
        FringeKind, FringePosting, Group, ReportPreset, ReportSpec, UnitDef,
    };
    use std::collections::BTreeSet;

    /// A source with every kind of item, some referring to others.
    fn source() -> Budget {
        let mut b = starter::budget();
        b.title = "Source".into();
        for (n, f) in [("DAYS", "20"), ("WEEKS", "DAYS / 5"), ("FICA_RATE", "7.65")] {
            b.globals.insert(n.into(), f.into());
        }
        b.units.insert(
            "Shift".into(),
            UnitDef {
                plural: "Shifts".into(),
                description: String::new(),
                hours: Some(Dec::from_int(12)),
            },
        );
        b.fringes.insert(
            "FICA".into(),
            Fringe {
                short_code: None,
                description: "Social security".into(),
                rate: "FICA_RATE".into(),
                kind: FringeKind::Percent { cutoff: None },
            },
        );
        b.fringes.insert(
            "KIT".into(),
            Fringe {
                short_code: None,
                description: String::new(),
                rate: "WEEKS * 10".into(),
                kind: FringeKind::flat(Some("Shift".into())),
            },
        );
        b.groups.insert(
            "OT".into(),
            Group {
                short_code: None,
                description: "Overtime".into(),
                in_budget_total: true,
                color: None,
            },
        );
        b.currencies.insert(
            "EUR".into(),
            Currency {
                description: "Euro".into(),
                symbol: "€".into(),
                rate: "1.1".parse().unwrap(),
                format: Default::default(),
            },
        );
        b.charges.push(Charge {
            name: "Fee".into(),
            account: None,
            applied: true,
            kind: ChargeKind::Flat {
                amount: 500_000,
                currency: Some("EUR".into()),
            },
            excluded_amounts: Vec::new(),
            excluded_groups: ["OT".to_string()].into(),
            excluded_categories: BTreeSet::new(),
            excluded_accounts: BTreeSet::new(),
            label_percent: false,
            label_excluded: false,
        });
        b.credits.push(Credit {
            name: "State".into(),
            account: None,
            applied: true,
            cap: None,
            discount_percent: Dec::ZERO,
            excluded_fringes: ["KIT".to_string()].into(),
            locations: BTreeSet::new(),
            rows: vec![method_core::CreditRow {
                description: "Labor".into(),
                source: CreditSource::Group { group: "OT".into() },
                percent: Dec::from_int(25),
                cap: CreditCap::None,
            }],
        });
        b.report_presets.push(ReportPreset {
            name: "Crew".into(),
            default: true,
            spec: ReportSpec {
                only_groups: ["OT".to_string()].into(),
                footer: Some(method_core::Band {
                    left: Some(method_core::BandField::Global("WEEKS".into())),
                    ..method_core::Band::default()
                }),
                ..ReportSpec::default()
            },
        });
        b.fringe_posting = FringePosting::Account;
        b
    }

    fn pick(kind: &str, name: &str) -> ItemRef {
        ItemRef {
            kind: kind.into(),
            name: name.into(),
        }
    }

    fn res(kind: &str, name: &str, choice: &str) -> Resolution {
        Resolution {
            kind: kind.into(),
            name: name.into(),
            choice: choice.into(),
        }
    }

    fn item<'a>(p: &'a ImportPreview, kind: &str, name: &str) -> &'a ImportItem {
        p.items
            .iter()
            .find(|i| i.kind == kind && i.name == name)
            .unwrap_or_else(|| panic!("{kind} {name} not listed"))
    }

    #[test]
    fn the_review_lists_every_item_with_its_status_and_needs() {
        let src = source();
        let mut here = starter::budget();
        here.globals.insert("DAYS".into(), "30".into());
        let p = preview(&here, &src, 7, "Source", "/x/Source.mthd");
        assert_eq!(p.token, 7);
        // Globals first, settings last, as the review lists them.
        assert_eq!(p.items.first().unwrap().kind, "global");
        assert_eq!(p.items.last().unwrap().kind, "new_line_defaults");
        let days = item(&p, "global", "DAYS");
        assert_eq!(days.status, "differs");
        assert_eq!(days.here.as_deref(), Some("= 30"));
        assert_eq!(days.imported, "= 20");
        assert_eq!(days.rename_to.as_deref(), Some("DAYS_2"));
        assert_eq!(item(&p, "fringe", "PAYROLL").status, "same");
        let kit = item(&p, "fringe", "KIT");
        assert_eq!(kit.status, "new");
        assert_eq!(
            kit.needs,
            [
                pick("global", "DAYS"),
                pick("global", "WEEKS"),
                pick("unit", "Shift")
            ],
            "a flat fringe needs its unit and its rate's globals, however deep"
        );
        let credit = item(&p, "credit", "State");
        assert!(credit.needs.contains(&pick("fringe", "KIT")));
        assert!(credit.needs.contains(&pick("unit", "Shift")));
        assert!(credit.needs.contains(&pick("group", "OT")));
        assert_eq!(
            item(&p, "charge", "Fee").needs,
            [pick("group", "OT"), pick("currency", "EUR")]
        );
        assert_eq!(
            item(&p, "report_preset", "Crew").needs,
            [
                pick("global", "DAYS"),
                pick("global", "WEEKS"),
                pick("group", "OT")
            ],
            "a footer showing a global needs it, and what its formula names"
        );
        assert_eq!(item(&p, "fringe_posting", "").status, "differs");
        assert_eq!(item(&p, "fringe_posting", "").rename_to, None);
        assert_eq!(item(&p, "account_coding", "").status, "same");
        assert!(p.notes.is_empty());
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["items"][0]["kind"], "global");
    }

    #[test]
    fn import_brings_dependencies_and_is_one_undo_step() {
        let src = source();
        let mut s = Session::new();
        let before = s.budget().clone();
        let done = s
            .edit("Test edit", |b| {
                import(b, &src, &[pick("credit", "State")], &[])
            })
            .unwrap();
        let b = s.budget();
        assert_eq!(b.credits.len(), 1);
        assert!(
            b.fringes.contains_key("KIT"),
            "the credit's excluded fringe"
        );
        assert!(b.units.contains_key("Shift"), "the fringe's unit");
        assert!(b.groups.contains_key("OT"), "the credit's group");
        assert_eq!(b.globals.get("WEEKS").unwrap().0, "DAYS / 5");
        assert_eq!(b.globals.get("DAYS").unwrap().0, "20");
        assert!(!b.fringes.contains_key("FICA"), "not picked, not needed");
        assert!(b.charges.iter().all(|c| c.name != "Fee"));
        assert_eq!(done.added, 6, "{done:?}"); // credit, KIT, Shift, OT, DAYS, WEEKS
        assert_eq!(done.replaced, 0);
        assert!(s.undo(), "one undo step");
        assert_eq!(s.budget(), &before);
        assert!(!s.can_undo());
    }

    fn conflicted() -> Budget {
        let mut here = starter::budget();
        here.globals.insert("DAYS".into(), "30".into());
        here.groups.insert(
            "OT".into(),
            Group {
                short_code: None,
                description: "Other".into(),
                in_budget_total: false,
                color: None,
            },
        );
        here
    }

    #[test]
    fn conflicts_keep_this_budgets_version_by_default() {
        let src = source();
        let mut b = conflicted();
        let done = import(
            &mut b,
            &src,
            &[pick("fringe", "KIT"), pick("charge", "Fee")],
            &[],
        )
        .unwrap();
        assert_eq!(b.globals["DAYS"].0, "30", "kept");
        assert!(!b.groups["OT"].in_budget_total, "kept");
        assert_eq!(b.fringes["KIT"].rate.0, "WEEKS * 10");
        assert_eq!(done.kept, 2);
        let fee = b.charges.iter().find(|c| c.name == "Fee").unwrap();
        assert!(fee.excluded_groups.contains("OT"));
        assert!(b.currencies.contains_key("EUR"));
    }

    #[test]
    fn conflicts_can_use_the_imported_version_or_a_new_name() {
        let src = source();
        let mut b = conflicted();
        import(
            &mut b,
            &src,
            &[
                pick("fringe", "KIT"),
                pick("charge", "Fee"),
                pick("report_preset", "crew"),
            ],
            &[
                res("global", "DAYS", "imported"),
                res("group", "OT", "rename"),
            ],
        )
        .unwrap();
        assert_eq!(b.globals["DAYS"].0, "20", "imported replaces");
        assert!(!b.groups["OT"].in_budget_total, "this budget's OT stays");
        assert!(
            b.groups["OT (2)"].in_budget_total,
            "the imported one, renamed"
        );
        let fee = b.charges.iter().find(|c| c.name == "Fee").unwrap();
        assert!(fee.excluded_groups.contains("OT (2)"), "uses the new name");
        let crew = b.report_presets.iter().find(|p| p.name == "Crew").unwrap();
        assert!(crew.spec.only_groups.contains("OT (2)"));
        assert!(!crew.default, "an added preset is never the default");

        // A renamed global is renamed in what the import brings.
        let mut b = conflicted();
        import(
            &mut b,
            &src,
            &[pick("fringe", "KIT")],
            &[res("global", "DAYS", "rename")],
        )
        .unwrap();
        assert_eq!(b.globals["DAYS"].0, "30");
        assert_eq!(b.globals["DAYS_2"].0, "20");
        assert_eq!(b.globals["WEEKS"].0, "DAYS_2 / 5");

        // Settings: keep by default, or the imported one.
        let mut b = conflicted();
        let before = b.fringe_posting.clone();
        import(&mut b, &src, &[pick("fringe_posting", "")], &[]).unwrap();
        assert_eq!(b.fringe_posting, before);
        import(
            &mut b,
            &src,
            &[pick("fringe_posting", "")],
            &[res("fringe_posting", "", "imported")],
        )
        .unwrap();
        assert_eq!(b.fringe_posting, FringePosting::Account);
    }

    /// The cutoff factor and the budget's fringe row are budget settings
    /// like the others (decisions.md, entry 61): listed, kept unless the
    /// imported one is picked, and the fringe row's place stays this
    /// budget's (it is a row of one topsheet).
    #[test]
    fn the_cutoff_factor_and_fringe_row_import_as_settings() {
        let mut src = source();
        src.cutoff_factor = method_core::CutoffFactor::OneAndAHalf;
        src.fringe_row = method_core::FringeRow {
            account: Some("9999".into()),
            description: Some("All fringes".into()),
            position: Some(0),
        };
        let mut here = starter::budget();
        here.fringe_row.position = Some(3);
        let p = preview(&here, &src, 1, "Source", "/x/Source.mthd");
        let factor = item(&p, "cutoff_factor", "");
        assert_eq!(factor.status, "differs");
        assert_eq!(factor.imported, "every percent fringe cutoff times 1.5");
        assert_eq!(
            factor.here.as_deref(),
            Some("percent fringe cutoffs as typed (1.0)")
        );
        let row = item(&p, "fringe_row", "");
        assert_eq!(row.status, "differs");
        assert_eq!(row.imported, "the budget's fringe row is 9999 All fringes");
        assert_eq!(factor.rename_to, None);

        // Kept by default.
        let mut b = here.clone();
        import(
            &mut b,
            &src,
            &[pick("cutoff_factor", ""), pick("fringe_row", "")],
            &[],
        )
        .unwrap();
        assert_eq!(b, here);
        // The imported ones: the number and description come, the place stays.
        let done = import(
            &mut b,
            &src,
            &[pick("cutoff_factor", ""), pick("fringe_row", "")],
            &[
                res("cutoff_factor", "", "imported"),
                res("fringe_row", "", "imported"),
            ],
        )
        .unwrap();
        assert_eq!(done.replaced, 2);
        assert_eq!(b.cutoff_factor, method_core::CutoffFactor::OneAndAHalf);
        assert_eq!(b.fringe_row.account.as_deref(), Some("9999"));
        assert_eq!(b.fringe_row.description(), "All fringes");
        assert_eq!(b.fringe_row.position, Some(3));
        // Only the place differs now, so the fringe row is the same.
        let p = preview(&b, &src, 2, "Source", "/x/Source.mthd");
        assert_eq!(item(&p, "fringe_row", "").status, "same");
        assert_eq!(item(&p, "cutoff_factor", "").status, "same");
        // A setup pack never holds them, as it holds no budget setting.
        assert!(!method_format::setup::pack::pack_kind(What::CutoffFactor));
        assert!(!method_format::setup::pack::pack_kind(What::FringeRow));
    }

    #[test]
    fn replacing_a_charge_or_preset_keeps_its_place_and_default() {
        let src = source();
        let mut b = starter::budget();
        b.charges.insert(
            0,
            Charge {
                name: "Fee".into(),
                ..b.charges[0].clone()
            },
        );
        b.report_presets.push(ReportPreset {
            name: "CREW".into(),
            default: true,
            spec: ReportSpec::default(),
        });
        let done = import(
            &mut b,
            &src,
            &[pick("charge", "Fee"), pick("report_preset", "Crew")],
            &[
                res("charge", "Fee", "imported"),
                res("report_preset", "Crew", "imported"),
            ],
        )
        .unwrap();
        assert_eq!(b.charges[0].name, "Fee");
        assert!(matches!(b.charges[0].kind, ChargeKind::Flat { .. }));
        assert_eq!(b.report_presets.len(), 1);
        assert_eq!(b.report_presets[0].name, "CREW");
        assert!(b.report_presets[0].default);
        assert!(b.report_presets[0].spec.only_groups.contains("OT"));
        assert_eq!(done.replaced, 2);
    }

    #[test]
    fn tag_presets_and_line_blocks_bring_their_tags_under_new_names() {
        let mut src = source();
        src.tag_presets.push(method_core::TagPreset {
            name: "Crew OT".into(),
            fringes: ["FICA".to_string()].into(),
            groups: ["OT".to_string()].into(),
            ..method_core::TagPreset::default()
        });
        let mut row = method_core::Detail::line(0, "Grip", "DAYS", "Shift", "400");
        row.groups = ["OT".to_string()].into();
        let mut setup = method_core::SetupItems::default();
        setup.groups.insert("OT".into(), src.groups["OT"].clone());
        src.line_presets.push(method_core::LinePreset {
            name: "Grips".into(),
            rows: vec![row],
            setup,
        });
        let p = preview(&starter::budget(), &src, 1, "Source", "");
        assert_eq!(
            item(&p, "tag_preset", "Crew OT").needs,
            [
                pick("global", "FICA_RATE"),
                pick("fringe", "FICA"),
                pick("group", "OT")
            ]
        );
        assert_eq!(
            item(&p, "line_preset", "Grips").needs,
            [
                pick("global", "DAYS"),
                pick("group", "OT"),
                pick("unit", "Shift")
            ]
        );

        let mut b = conflicted();
        import(
            &mut b,
            &src,
            &[pick("tag_preset", "crew ot"), pick("line_preset", "Grips")],
            &[res("group", "OT", "rename")],
        )
        .unwrap();
        assert_eq!(b.tag_presets[0].name, "Crew OT");
        assert!(b.tag_presets[0].groups.contains("OT (2)"));
        assert!(b.fringes.contains_key("FICA"));
        let block = &b.line_presets[0];
        assert!(block.rows[0].groups.contains("OT (2)"));
        assert!(block.setup.groups.contains_key("OT (2)"));
        assert!(!block.setup.groups.contains_key("OT"));
    }

    #[test]
    fn bad_picks_and_choices_change_nothing() {
        let src = source();
        let mut b = conflicted();
        let before = b.clone();
        assert!(import(&mut b, &src, &[pick("fringe", "NOPE")], &[]).is_err());
        assert!(import(&mut b, &src, &[pick("bogus", "x")], &[]).is_err());
        assert!(import(
            &mut b,
            &src,
            &[pick("fringe", "KIT")],
            &[res("global", "DAYS", "maybe")]
        )
        .is_err());
        assert!(import(
            &mut b,
            &src,
            &[pick("fringe_posting", "")],
            &[res("fringe_posting", "", "rename")]
        )
        .is_err());
        assert_eq!(b, before);
    }

    #[test]
    fn the_base_currency_is_not_imported_and_flat_charges_in_it_count_as_base() {
        let mut src = source();
        src.base_currency = "CAD".into();
        let mut b = starter::budget();
        b.base_currency = "EUR".into();
        let p = preview(&b, &src, 1, "Source", "");
        assert!(p
            .items
            .iter()
            .all(|i| !(i.kind == "currency" && i.name == "EUR")));
        assert_eq!(p.notes.len(), 1);
        assert!(!item(&p, "charge", "Fee")
            .needs
            .contains(&pick("currency", "EUR")));
        import(&mut b, &src, &[pick("charge", "Fee")], &[]).unwrap();
        assert!(!b.currencies.contains_key("EUR"));
        let fee = b.charges.iter().find(|c| c.name == "Fee").unwrap();
        assert_eq!(
            fee.kind,
            ChargeKind::Flat {
                amount: 500_000,
                currency: None
            }
        );
    }
}
