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
//! Everything but the commands at the end is plain functions on budgets, so
//! it is tested without a window.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use method_core::Budget;
use serde::Serialize;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{finish, AppState, DocId};
use crate::docs::Docs;
use crate::view::View;

// The review and the import themselves are plain functions on budgets in
// method_format::setup::import, shared with the command line (`method pack`).
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

    pub(crate) fn get(&self, token: u64) -> Option<(String, Arc<Budget>)> {
        let g = self.lock();
        g.1.as_ref()
            .filter(|s| s.token == token)
            .map(|s| (s.label.clone(), Arc::clone(&s.budget)))
    }
}

/// The budget in `path` and its name: from its tab when it is open (unsaved
/// changes count), else read from the file. Refused when it is budget
/// `target` itself.
pub fn read_source(docs: &Docs, target: DocId, path: &Path) -> Result<(String, Budget), String> {
    if let Some(doc) = docs.find_file(path) {
        if doc == target {
            return Err("That is this budget. Pick another file to import from.".into());
        }
        let s = docs.get(doc).ok_or("That budget has been closed.")?;
        return Ok((s.tab_label(), s.budget().clone()));
    }
    let (budget, _) = crate::session::read_budget(path)?;
    let label = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    Ok((label, budget))
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// Shows an Open dialog for the budget to import setup from, reads it and
/// returns the review for budget `doc` (`None`: the one in front). `None`
/// when cancelled.
#[tauri::command]
pub async fn import_setup_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ImportSources>,
    doc: Option<DocId>,
) -> Result<Option<ImportPreview>, String> {
    let (target, start) = {
        let s = state.lock_doc(doc)?;
        (s.id(), s.path().and_then(|p| p.parent()).map(PathBuf::from))
    };
    let Some(path) = crate::app::pick_budget(&window, "Import setup from", start)? else {
        return Ok(None);
    };
    let (label, budget) = read_source(&state.docs(), target, &path)?;
    let s = state.lock_doc(Some(target))?;
    let p = preview(s.budget(), &budget, 0, &label, &path.display().to_string());
    let token = sources.keep(label, budget);
    Ok(Some(ImportPreview { token, ..p }))
}

/// What `import_setup` did: the view, and a sentence saying so.
#[derive(Debug, Serialize)]
pub struct ImportDone {
    pub view: View,
    pub message: String,
    #[serde(flatten)]
    pub counts: Imported,
}

/// `{token, picks, resolutions}`: imports the picked items (and what they
/// need) from the reviewed source into budget `doc`, as one undo step.
#[tauri::command]
pub fn import_setup<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ImportSources>,
    doc: Option<DocId>,
    token: u64,
    picks: Vec<ItemRef>,
    resolutions: Vec<Resolution>,
) -> Result<ImportDone, String> {
    let (_, src) = sources
        .get(token)
        .ok_or("Another file was picked since. Import again.")?;
    let mut s = state.lock_doc(doc)?;
    let label = format!(
        "Import {} of setup",
        crate::history::count(picks.len(), "item", "items")
    );
    let counts = s.edit(label, |b| import(b, &src, &picks, &resolutions))?;
    Ok(ImportDone {
        view: finish(&window, &mut s, None),
        message: counts.message(),
        counts,
    })
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
                kind: FringeKind::Flat {
                    unit: Some("Shift".into()),
                },
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
            rows: vec![method_core::CreditRow {
                description: "Labor".into(),
                source: CreditSource::Group {
                    group: "OT".into(),
                    excluded_fringes: ["KIT".to_string()].into(),
                },
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
        import(&mut b, &src, &[pick("fringe_posting", "")], &[]).unwrap();
        assert_eq!(b.fringe_posting, FringePosting::default());
        import(
            &mut b,
            &src,
            &[pick("fringe_posting", "")],
            &[res("fringe_posting", "", "imported")],
        )
        .unwrap();
        assert_eq!(b.fringe_posting, FringePosting::Account);
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

    #[test]
    fn a_source_open_in_a_tab_is_read_from_the_tab_and_never_itself() {
        let dir = crate::safety::tests::temp_dir("import-source");
        let path = dir.join("other.mthd");
        let mut other = Session::new();
        other.save_to(&path).unwrap();
        other
            .edit("Test edit", |b| crate::edit::set_global(b, "UNSAVED", "1"))
            .unwrap();
        let mut docs = Docs::default();
        let first = docs.active();
        let tab = docs.add(other);
        let (label, b) = read_source(&docs, first, &path).unwrap();
        assert_eq!(label, "other");
        assert!(b.globals.contains_key("UNSAVED"), "unsaved changes count");
        assert!(read_source(&docs, tab, &path).is_err(), "not from itself");
        docs.remove(tab).unwrap();
        let (_, b) = read_source(&docs, first, &path).unwrap();
        assert!(!b.globals.contains_key("UNSAVED"), "read from the file");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn commands_are_wired_for_the_main_window_only() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
        const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");
        for (module, c) in [
            ("import_setup", "import_setup_pick"),
            ("import_setup", "import_setup"),
            ("templates", "template_new"),
            ("templates", "template_save"),
        ] {
            assert!(
                APP_RS.contains(&format!("crate::{module}::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(
                MAIN_CAPS.contains(&allow),
                "{c} not allowed for the main window"
            );
            assert!(
                !TOOLS_CAPS.contains(&allow),
                "{c} allowed for the tools window"
            );
        }
    }
}
