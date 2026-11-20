use super::*;
use crate::session::SessionFiles;
use method_core::{FringePosting, InfoValue};

fn spec(title: &str) -> NewBudgetSpec {
    NewBudgetSpec {
        title: title.into(),
        kind: "feature".into(),
        ..NewBudgetSpec::default()
    }
}

fn info(b: &Budget, name: &str) -> Option<InfoValue> {
    b.info.get(name).cloned()
}

#[test]
fn the_form_fills_title_info_globals_currency_and_settings() {
    let s = NewBudgetSpec {
        title: "  Big Show ".into(),
        kind: "tv_season".into(),
        season: "2".into(),
        episode: "ignored for a season".into(),
        episodes: "10".into(),
        prep_start: "2027-01-04".into(),
        shoot_start: "2027-02-01".into(),
        shoot_end: "".into(),
        wrap_end: "2027-04-30".into(),
        prep_weeks: "6".into(),
        shoot_days: "1,200".into(),
        wrap_weeks: "2.5".into(),
        country: "Canada".into(),
        region: "British Columbia".into(),
        city: "Vancouver".into(),
        currency: "cad".into(),
        start: "starter".into(),
        fringe_posting: "category".into(),
        arithmetic: "exact".into(),
        ..NewBudgetSpec::default()
    };
    let (b, msg) = build(&s, None, &[]).unwrap();
    assert_eq!(b.title, "Big Show");
    assert_eq!(b.base_currency, "CAD");
    assert_eq!(
        info(&b, "Production type"),
        Some(InfoValue::Text("TV season".into()))
    );
    assert_eq!(info(&b, "Season"), Some(InfoValue::Text("2".into())));
    assert_eq!(info(&b, "Episode"), None, "a season has no episode number");
    assert_eq!(
        info(&b, "Episodes"),
        Some(InfoValue::Number(Dec::from_int(10)))
    );
    assert_eq!(
        info(&b, "Prep start"),
        Some(InfoValue::Date("2027-01-04".into()))
    );
    assert_eq!(info(&b, "Shoot end"), None, "blank is left out");
    assert_eq!(info(&b, "City"), Some(InfoValue::Text("Vancouver".into())));
    assert_eq!(b.globals.get("PREP_WEEKS").map(|f| f.0.as_str()), Some("6"));
    assert_eq!(
        b.globals.get("SHOOT_DAYS").map(|f| f.0.as_str()),
        Some("1200")
    );
    assert_eq!(
        b.globals.get("WRAP_WEEKS").map(|f| f.0.as_str()),
        Some("2.5")
    );
    // The starter's own formula follows the new shoot days.
    assert_eq!(
        b.globals.get("SHOOT_WEEKS").map(|f| f.0.as_str()),
        Some("SHOOT_DAYS / 5")
    );
    assert!(matches!(b.fringe_posting, FringePosting::Category { .. }));
    assert_eq!(b.arithmetic, method_core::Arithmetic::Exact);
    assert!(msg.starts_with("New budget \"Big Show\" with "), "{msg}");
    assert!(msg.contains("lines"), "the starter has lines: {msg}");
    // It recalculates with the new globals.
    assert!(method_core::recalc(&b, &Default::default()).is_ok());
}

#[test]
fn a_chart_an_empty_budget_and_a_file_start_without_lines() {
    let chart = NewBudgetSpec {
        start: "chart".into(),
        chart: "feature".into(),
        ..spec("Chart")
    };
    let (b, msg) = build(&chart, None, &[]).unwrap();
    assert!(b.categories().count() > 5);
    assert!(b
        .categories()
        .flat_map(|c| &c.accounts)
        .all(|a| a.details.is_empty()));
    assert_eq!(b.title, "Chart");
    assert!(!msg.contains("line"), "{msg}");
    // A type alone never adds globals.
    assert!(!b.globals.contains_key("SHOOT_DAYS"));

    let blank = NewBudgetSpec {
        start: "blank".into(),
        currency: "EUR".into(),
        ..spec("Empty")
    };
    let (b, _) = build(&blank, None, &[]).unwrap();
    assert_eq!(b.categories().count(), 0);
    assert_eq!(b.base_currency, "EUR");
    assert_eq!(
        b.fringe_posting,
        FringePosting::Budget,
        "new-budget settings"
    );

    let mut from = starter::budget();
    from.title = "Old show".into();
    from.info
        .insert("Director".into(), InfoValue::Text("Someone".into()));
    let file = NewBudgetSpec {
        start: "file".into(),
        ..spec("From file")
    };
    let (b, _) = build(&file, Some(&from), &[]).unwrap();
    assert_eq!(b.title, "From file");
    assert_eq!(info(&b, "Director"), None, "a template's info is left out");
    assert_eq!(b.fringes, from.fringes, "its setup is kept");
    assert!(
        build(&file, None, &[]).is_err(),
        "the file has to be picked"
    );
}

#[test]
fn the_form_refuses_what_it_cant_use() {
    let bad = |s: NewBudgetSpec| build(&s, None, &[]).unwrap_err();
    assert_eq!(bad(spec("  ")), "Give the budget a title.");
    assert!(bad(NewBudgetSpec {
        kind: "opera".into(),
        ..spec("X")
    })
    .contains("production type"));
    let date = bad(NewBudgetSpec {
        shoot_start: "2027-02-30".into(),
        ..spec("X")
    });
    assert!(date.starts_with("Shoot start: "), "{date}");
    assert!(date.ends_with('.') && !date.ends_with(".."), "{date}");
    assert!(bad(NewBudgetSpec {
        shoot_days: "lots".into(),
        ..spec("X")
    })
    .starts_with("Shoot days is a number"));
    assert!(bad(NewBudgetSpec {
        prep_weeks: "-1".into(),
        ..spec("X")
    })
    .contains("below 0"));
    assert!(bad(NewBudgetSpec {
        kind: "tv_season".into(),
        episodes: "0".into(),
        ..spec("X")
    })
    .starts_with("Episodes"));
    assert!(bad(NewBudgetSpec {
        start: "chart".into(),
        chart: "no-such-chart".into(),
        ..spec("X")
    })
    .contains("isn't bundled"));
    assert!(bad(NewBudgetSpec {
        fringe_posting: "sideways".into(),
        ..spec("X")
    })
    .starts_with("Fringe posting"));
    assert!(bad(NewBudgetSpec {
        city: "a\nb".into(),
        ..spec("X")
    })
    .contains("one line"));
    // A currency the starting point lists as foreign.
    let mut from = starter::budget();
    from.currencies.insert(
        "GBP".into(),
        method_core::Currency {
            description: "Pounds".into(),
            symbol: String::new(),
            rate: Dec::from_int(1),
            format: Default::default(),
        },
    );
    let e = build(
        &NewBudgetSpec {
            start: "file".into(),
            currency: "GBP".into(),
            ..spec("X")
        },
        Some(&from),
        &[],
    )
    .unwrap_err();
    assert!(e.contains("foreign currency"), "{e}");
}

#[test]
fn a_new_budget_takes_the_place_of_an_untouched_untitled_one() {
    let dir = safety::tests::temp_dir("start-new");
    let safety = Safety::new(Some(&dir));
    let state = AppState::with(Docs::default());
    let first = state.docs().active();
    let (b, _) = build(&spec("Big Show"), None, &[]).unwrap();
    let id = open_new(&state, &safety, b).unwrap();
    {
        let docs = state.docs();
        assert_eq!(docs.len(), 1, "the untouched Untitled gave its tab up");
        assert_ne!(id, first);
        let s = docs.get(id).unwrap();
        assert!(s.path().is_none(), "no file: the first Save asks where");
        assert!(s.is_dirty(), "closing it asks to save");
        assert_eq!(s.budget().title, "Big Show");
    }
    // With a budget in use, the next one gets its own tab.
    let (b, _) = build(&spec("Pilot"), None, &[]).unwrap();
    let second = open_new(&state, &safety, b).unwrap();
    let docs = state.docs();
    assert_eq!(docs.len(), 2);
    assert_eq!(docs.active(), second, "in front");
    drop(docs);
    // One undo step takes the form's work back.
    let mut s = state.lock_doc(Some(second)).unwrap();
    assert!(s.undo());
    assert_eq!(s.budget().categories().count(), 0);
    drop(s);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_lists_show_pinned_then_recent_with_totals_and_problems() {
    let dir = safety::tests::temp_dir("start-list");
    let a = dir.join("a.mthd");
    let b = dir.join("b.mthd");
    let gone = dir.join("gone.mthd");
    let junk = dir.join("junk.mthd");
    let mut s = Session::with_budget(starter::budget(), None);
    s.edit("Test edit", |x| edit::set_title(x, "Alpha"))
        .unwrap();
    s.save_to(&a).unwrap();
    Session::with_budget(starter::blank("Beta", "EUR"), None)
        .save_to(&b)
        .unwrap();
    std::fs::write(&junk, "not a budget").unwrap();
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap();
    let (a, b, junk) = (canon(&a), canon(&b), canon(&junk));
    let gone = canon(&dir).join(gone.file_name().unwrap());

    let start = StartState::load(Some(dir.join(START_FILE)));
    start.set_pinned(&b, true).unwrap();
    let state = AppState::with(Docs::default());
    let recent = vec![a.clone(), b.clone(), gone.clone(), junk.clone()];
    let v = list(&state, &start, &recent);
    assert!(v.fresh, "only an untouched Untitled budget is open");
    assert_eq!(v.pinned.len(), 1);
    assert_eq!(v.pinned[0].title, "Beta");
    assert_eq!(v.pinned[0].currency, "EUR");
    assert_eq!(v.pinned[0].net_total, Some(0));
    assert!(v.pinned[0].pinned && v.pinned[0].recent);
    let files: Vec<&str> = v.recent.iter().map(|e| e.file.as_str()).collect();
    assert_eq!(
        files,
        ["a.mthd", "gone.mthd", "junk.mthd"],
        "pinned ones once"
    );
    let alpha = &v.recent[0];
    assert_eq!(alpha.title, "Alpha");
    let want = crate::diagnose::lenient_recalc(&starter::budget())
        .report
        .unwrap()
        .net_total;
    assert_eq!(alpha.net_total, Some(want));
    assert!(alpha.modified.is_some());
    assert!(!alpha.open);
    assert_eq!(
        v.recent[1].problem.as_deref(),
        Some("Can't find this file.")
    );
    assert!(v.recent[2].problem.is_some(), "not a budget");

    // Tokens name the rows of the last listing only.
    assert_eq!(start.listed(alpha.token).unwrap(), a);
    let again = list(&state, &start, &recent);
    assert!(start.listed(alpha.token).is_err());
    assert_eq!(start.listed(again.recent[0].token).unwrap(), a);

    // A budget open in a tab is summed from the tab, unsaved changes too.
    let safety = Safety::new(Some(&dir));
    let id = crate::safety_app::open_path(&state, &safety, &a).unwrap();
    state
        .lock_doc(Some(id))
        .unwrap()
        .edit("Test edit", |x| edit::set_title(x, "Alpha (edited)"))
        .unwrap();
    let v = list(&state, &start, &recent);
    assert!(!v.fresh, "a file is open");
    assert!(v.recent[0].open);
    assert_eq!(v.recent[0].title, "Alpha (edited)");

    // Remove takes it off the recent list; the file stays.
    assert_eq!(safety.recent_paths(), std::slice::from_ref(&a));
    safety.forget_recent(&a);
    assert!(safety.recent_paths().is_empty());
    assert!(a.is_file());

    // The pinned list is kept, at the top, and unpinned again.
    start.set_pinned(&a, true).unwrap();
    let kept = StartState::load(Some(dir.join(START_FILE)));
    assert_eq!(kept.pinned(), [a.clone(), b.clone()]);
    kept.set_pinned(&b, false).unwrap();
    assert_eq!(StartState::load(Some(dir.join(START_FILE))).pinned(), [a]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn folders_under_home_are_written_from_tilde() {
    let sep = std::path::MAIN_SEPARATOR;
    let home = PathBuf::from(format!("{sep}Users{sep}riley"));
    let under = home.join("Films").join("Big Show");
    assert_eq!(
        folder_text(Some(&under), Some(&home)),
        format!("~{sep}Films{sep}Big Show")
    );
    assert_eq!(folder_text(Some(&home), Some(&home)), "~");
    let other = PathBuf::from(format!("{sep}Volumes{sep}Shared"));
    assert_eq!(
        folder_text(Some(&other), Some(&home)),
        other.display().to_string()
    );
    assert_eq!(folder_text(None, Some(&home)), "");
}

#[test]
fn a_picked_template_is_named_by_its_token() {
    let start = StartState::load(None);
    assert!(start.picked(Some(1)).is_err());
    let t = start.keep_picked(starter::budget());
    assert!(start.picked(Some(t)).is_ok());
    assert!(start.picked(Some(t + 1)).is_err());
    assert!(start.picked(None).is_err());
    let newer = start.keep_picked(starter::blank("B", "USD"));
    assert!(start.picked(Some(t)).is_err(), "only the last one is kept");
    assert_eq!(start.picked(Some(newer)).unwrap().title, "B");
}

#[test]
fn every_bundled_chart_is_listed_with_its_size() {
    let charts = chart_choices();
    assert_eq!(charts.len(), method_format::charts::all().len());
    assert!(charts.iter().all(|c| c.accounts > 0 && c.categories > 0));
    assert!(charts
        .iter()
        .all(|c| method_format::charts::GROUPS.contains(&c.group)));
    // The charts each production type starts from are our own.
    for id in ["feature", "tv-one-hour", "commercial", "documentary"] {
        let c = charts.iter().find(|c| c.id == id).unwrap();
        assert!(c.ours, "{id}");
    }
    assert!(charts.iter().any(|c| !c.ours), "studio charts too");
}

#[test]
fn file_names_come_from_the_title_or_what_was_typed() {
    assert_eq!(file_stem_for("", "Big Show").unwrap(), "Big Show");
    assert_eq!(file_stem_for("  ", "A/B: C").unwrap(), "A B  C");
    assert_eq!(file_stem_for("Pilot v3.MTHD", "X").unwrap(), "Pilot v3");
    assert_eq!(file_stem_for("Pilot.mthd ", "X").unwrap(), "Pilot");
    assert!(file_stem_for("a/b", "X")
        .unwrap_err()
        .contains("can't have"));
    assert!(file_stem_for(".hidden", "X").unwrap_err().contains("dot"));
    assert!(file_stem_for(&"x".repeat(201), "X").is_err());
}

#[test]
fn save_in_falls_back_from_the_last_folder_to_documents_to_home() {
    let dir = safety::tests::temp_dir("start-folder");
    let gone = dir.join("gone");
    let docs = dir.join("Documents");
    std::fs::create_dir(&docs).unwrap();
    assert_eq!(
        default_folder(Some(dir.clone()), Some(docs.clone()), None),
        Some(dir.clone())
    );
    assert_eq!(
        default_folder(Some(gone.clone()), Some(docs.clone()), Some(dir.clone())),
        Some(docs)
    );
    assert_eq!(
        default_folder(Some(gone.clone()), None, Some(dir.clone())),
        Some(dir.clone())
    );
    assert_eq!(default_folder(Some(gone), None, None), None);
    // The folder is named by its token, the last one given only.
    let start = StartState::load(None);
    let t = start.keep_folder(dir.clone());
    assert_eq!(start.folder(t).unwrap(), dir);
    assert!(start.folder(t + 1).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn create_saves_into_the_folder_and_never_over_a_file() {
    let dir = std::fs::canonicalize(safety::tests::temp_dir("start-save")).unwrap();
    let safety = Safety::new(Some(&dir.join("data")));
    let state = AppState::with(Docs::default());
    let (b, _) = build(&spec("Big Show"), None, &[]).unwrap();
    let made = save_new(&state, &safety, &b, &dir, "Big Show").unwrap();
    let path = dir.join("Big Show.mthd");
    let Made::Opened(id, at) = made else {
        panic!("not saved: {made:?}")
    };
    assert_eq!(at, path);
    {
        let docs = state.docs();
        assert_eq!(docs.len(), 1, "in place of the untouched Untitled");
        let s = docs.get(id).unwrap();
        assert_eq!(s.path(), Some(path.as_path()));
        assert!(!s.is_dirty(), "a saved budget");
        assert_eq!(s.budget().title, "Big Show");
    }
    assert_eq!(safety.recent_paths(), std::slice::from_ref(&path));
    let text = std::fs::read_to_string(&path).unwrap();

    // The same name again: nothing written, a free name offered.
    let (other, _) = build(&spec("Other"), None, &[]).unwrap();
    assert_eq!(
        save_new(&state, &safety, &other, &dir, "Big Show").unwrap(),
        Made::Taken("Big Show 2".into())
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "untouched");
    assert_eq!(state.docs().len(), 1);
    std::fs::write(dir.join("Big Show 2.mthd"), "x").unwrap();
    assert_eq!(free_stem(&dir, "Big Show"), "Big Show 3");
    // No temporary files are left behind.
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(names.is_empty(), "{names:?}");
    // A folder that has gone is refused.
    assert!(save_new(&state, &safety, &other, &dir.join("gone"), "X").is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn summaries_are_kept_between_runs_until_the_file_changes() {
    let dir = std::fs::canonicalize(safety::tests::temp_dir("start-cache")).unwrap();
    let a = dir.join("a.mthd");
    Session::with_budget(starter::blank("Alpha", "USD"), None)
        .save_to(&a)
        .unwrap();
    let state = AppState::with(Docs::default());
    let start = StartState::load(Some(dir.join(START_FILE)));
    let v = list(&state, &start, std::slice::from_ref(&a));
    assert_eq!(v.recent[0].title, "Alpha");
    assert!(dir.join(CACHE_FILE).is_file(), "kept for the next start");

    // The next run takes the kept summary without reading the file:
    // a kept title that differs from the file's shows it was not read.
    let kept = std::fs::read_to_string(dir.join(CACHE_FILE)).unwrap();
    std::fs::write(
        dir.join(CACHE_FILE),
        kept.replace("\"Alpha\"", "\"From the cache\""),
    )
    .unwrap();
    let start = StartState::load(Some(dir.join(START_FILE)));
    let v = list(&state, &start, std::slice::from_ref(&a));
    assert_eq!(v.recent[0].title, "From the cache");

    // A changed file (another size) is read again.
    Session::with_budget(starter::blank("Alpha, longer title", "USD"), None)
        .save_to(&a)
        .unwrap();
    let v = list(&state, &start, std::slice::from_ref(&a));
    assert_eq!(v.recent[0].title, "Alpha, longer title");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_pinned_file_moved_in_the_finder_is_followed() {
    let dir = std::fs::canonicalize(safety::tests::temp_dir("start-moved")).unwrap();
    let a = dir.join("a.mthd");
    Session::with_budget(starter::blank("Alpha", "USD"), None)
        .save_to(&a)
        .unwrap();
    let start = StartState::load(Some(dir.join(START_FILE)));
    start.set_pinned(&a, true).unwrap();
    let b = dir.join("renamed.mthd");
    std::fs::rename(&a, &b).unwrap();
    let state = AppState::with(Docs::default());
    let v = list(&state, &start, &[]);
    if cfg!(target_os = "macos") {
        assert_eq!(v.pinned[0].file, "renamed.mthd");
        assert_eq!(v.pinned[0].title, "Alpha");
        // The new place is kept.
        let again = StartState::load(Some(dir.join(START_FILE)));
        assert_eq!(again.pinned(), [b]);
    } else {
        assert_eq!(
            v.pinned[0].problem.as_deref(),
            Some("Can't find this file.")
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_last_tab_closes_into_an_untouched_untitled_budget() {
    let dir = safety::tests::temp_dir("start-last");
    let safety = Safety::new(Some(&dir));
    let state = AppState::with(Docs::default());
    let id = state.docs().active();
    state
        .lock_doc(Some(id))
        .unwrap()
        .edit("Test edit", |b| edit::set_title(b, "Changed"))
        .unwrap();
    use crate::safety_app::{close_last_for_start, Closed};
    assert_eq!(
        close_last_for_start(&state, &safety, id, false).unwrap(),
        Closed::Unsaved
    );
    assert_eq!(
        close_last_for_start(&state, &safety, id, true).unwrap(),
        Closed::Closed
    );
    let docs = state.docs();
    assert_eq!(docs.len(), 1);
    assert!(docs.lone_untouched().is_some(), "as at launch");
    assert!(docs.get(id).is_none(), "the old budget is gone");
    drop(docs);
    // With other tabs open, it is an ordinary close.
    let second = crate::safety_app::new_tab(&state);
    assert_eq!(
        close_last_for_start(&state, &safety, second, false).unwrap(),
        Closed::Closed
    );
    assert_eq!(state.docs().len(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A library with a global the starter has too and one it hasn't.
fn a_library() -> Budget {
    let mut lib = crate::library::new_library("USD");
    lib.globals.insert("SHOOT_DAYS".into(), "40".into());
    lib.globals.insert("KIT_WEEKS".into(), "10".into());
    lib
}

#[test]
fn ticked_setup_comes_in_before_what_the_form_says() {
    let s = NewBudgetSpec {
        shoot_days: "30".into(),
        ..spec("Big Show")
    };
    let ticked = [Ticked {
        label: "your library".into(),
        budget: a_library(),
    }];
    let (b, msg) = build(&s, None, &ticked).unwrap();
    assert_eq!(b.globals["KIT_WEEKS"].0, "10");
    assert_eq!(b.globals["SHOOT_DAYS"].0, "30", "the form wins");
    assert!(
        msg.ends_with(" Setup from your library: 1 item added, 1 item replaced."),
        "{msg}"
    );
    // Settings typed in the form are untouched by the library.
    let (b, _) = build(
        &NewBudgetSpec {
            fringe_posting: "account".into(),
            ..spec("Big Show")
        },
        None,
        &ticked,
    )
    .unwrap();
    assert_eq!(b.fringe_posting, FringePosting::Account);
}

#[test]
fn the_setup_step_lists_the_library_and_packs_and_reads_what_is_ticked() {
    let dir = std::fs::canonicalize(safety::tests::temp_dir("start-setup")).unwrap();
    let docs = Docs::default();
    let library = Library::new(Some(&dir.join("data")));
    let packs = Packs::new(Some(dir.join("Packs")), None);
    let none = NewBudgetSpec {
        library: true,
        ..spec("X")
    };
    // No library yet: none offered, and ticking it is refused.
    let c = setup_choices(&docs, &library, &packs);
    assert_eq!(c.library, None);
    assert!(c.packs.is_empty());
    assert!(ticked_setup(&docs, &library, &packs, &none)
        .unwrap_err()
        .contains("no setup library"));

    // A library and a pack.
    let lib_path = dir.join(crate::library::LIBRARY_FILE_NAME);
    crate::library::write(&lib_path, &a_library()).unwrap();
    library.set_path(lib_path.clone()).unwrap();
    let mut items = crate::library::new_library("USD");
    items.globals.insert("PENSION_RATE".into(), "5".into());
    let picks = vec![crate::import_setup::ItemRef {
        kind: "global".into(),
        name: "PENSION_RATE".into(),
    }];
    let details = method_format::setup::pack::PackDetails {
        title: "Union Basics".into(),
        ..Default::default()
    };
    let (pack, _) = crate::packs::pack_build(&items, &picks, &details, "2026-10-18").unwrap();
    crate::library::write(&dir.join("Packs").join("union.mthd"), &pack).unwrap();

    let c = setup_choices(&docs, &library, &packs);
    let lib = c.library.unwrap();
    assert_eq!(lib.name, "My Method Budget Library");
    assert_eq!(lib.items, 2);
    assert_eq!(lib.problem, None);
    assert_eq!(c.packs.len(), 1);
    assert_eq!(c.packs[0].title, "Union Basics");

    let s = NewBudgetSpec {
        library: true,
        packs: vec![c.packs[0].token, c.packs[0].token],
        ..spec("Big Show")
    };
    let t = ticked_setup(&docs, &library, &packs, &s).unwrap();
    let labels: Vec<&str> = t.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(
        labels,
        ["your library", "Union Basics"],
        "each once, library first"
    );
    let (b, msg) = build(&s, None, &t).unwrap();
    assert_eq!(b.globals["PENSION_RATE"].0, "5");
    assert!(
        msg.contains("Setup from your library and Union Basics"),
        "{msg}"
    );

    // A token from an older listing is refused.
    setup_choices(&docs, &library, &packs);
    assert!(ticked_setup(&docs, &library, &packs, &s).is_err());

    // A library that has gone says so.
    std::fs::remove_file(&lib_path).unwrap();
    let c = setup_choices(&docs, &library, &packs);
    assert!(c.library.unwrap().problem.is_some());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The names of the files and folders in `dir`, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn new_budget_can_make_a_project_folder_with_the_budget_in_it() {
    let dir = std::fs::canonicalize(safety::tests::temp_dir("start-project")).unwrap();
    let data = dir.join("data");
    let safety = Safety::new(Some(&data));
    let start = StartState::load(Some(data.join(START_FILE)));
    let state = AppState::with(Docs::default());
    let (b, _) = build(&spec("Big Show"), None, &[]).unwrap();
    let made = save_new_project(&state, &safety, &start, &b, &dir, "Big Show").unwrap();
    let project = dir.join("Big Show");
    let path = project.join("Big Show.mthd");
    assert_eq!(made, Made::Opened(state.docs().active(), path.clone()));
    assert_eq!(
        names_in(&project),
        ["Big Show.mthd"],
        "the budget, nothing else"
    );
    {
        let docs = state.docs();
        let s = docs.get(docs.active()).unwrap();
        assert_eq!(s.path(), Some(path.as_path()));
        assert!(!s.is_dirty(), "a saved budget");
    }
    // The folder is listed with the projects, and kept between runs.
    assert_eq!(start.projects(), std::slice::from_ref(&project));
    let again = StartState::load(Some(data.join(START_FILE)));
    assert_eq!(again.projects(), std::slice::from_ref(&project));

    // The same name again makes nothing and offers a free one, also
    // when only a loose file of that name is there.
    let (other, _) = build(&spec("Other"), None, &[]).unwrap();
    assert_eq!(
        save_new_project(&state, &safety, &start, &other, &dir, "Big Show").unwrap(),
        Made::Taken("Big Show 2".into())
    );
    std::fs::write(dir.join("Pilot.mthd"), "x").unwrap();
    std::fs::create_dir(dir.join("Pilot 2")).unwrap();
    assert_eq!(
        save_new_project(&state, &safety, &start, &other, &dir, "Pilot").unwrap(),
        Made::Taken("Pilot 3".into())
    );
    assert_eq!(names_in(&project), ["Big Show.mthd"], "untouched");
    assert_eq!(start.projects().len(), 1);
    // A folder that has gone is refused, and nothing is left behind.
    assert!(save_new_project(&state, &safety, &start, &other, &dir.join("gone"), "X").is_err());
    assert!(!dir.join("gone").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_duplicate_is_a_new_file_beside_it_never_over_one() {
    let dir = std::fs::canonicalize(safety::tests::temp_dir("start-dup")).unwrap();
    let safety = Safety::new(Some(&dir.join("data")));
    let state = AppState::with(Docs::default());
    let a = dir.join("Big Show.mthd");
    let mut s = Session::with_budget(starter::budget(), None);
    s.edit("Test edit", |x| edit::set_title(x, "Big Show"))
        .unwrap();
    s.save_to(&a).unwrap();
    let text = std::fs::read(&a).unwrap();
    // "Big Show 2" is taken by something else: it is left alone.
    std::fs::write(dir.join("Big Show 2.mthd"), "someone else's").unwrap();

    let (id, to) = duplicate(&state, &safety, &a).unwrap();
    assert_eq!(to, dir.join("Big Show 3.mthd"));
    assert_eq!(std::fs::read(&to).unwrap(), text, "the same budget");
    assert_eq!(std::fs::read(&a).unwrap(), text, "the original untouched");
    assert_eq!(
        std::fs::read_to_string(dir.join("Big Show 2.mthd")).unwrap(),
        "someone else's"
    );
    {
        let docs = state.docs();
        let s = docs.get(id).unwrap();
        assert_eq!(docs.active(), id, "in front");
        assert_eq!(s.path(), Some(to.as_path()));
        assert!(!s.is_dirty(), "opened as a saved budget");
        assert_eq!(s.budget().title, "Big Show");
    }
    assert!(safety.recent_paths().contains(&to), "listed in Open Recent");
    // Again: the next free name.
    let (_, next) = duplicate(&state, &safety, &a).unwrap();
    assert_eq!(next, dir.join("Big Show 4.mthd"));
    // Only a readable budget file is duplicated.
    std::fs::write(dir.join("junk.mthd"), "not a budget").unwrap();
    assert!(duplicate(&state, &safety, &dir.join("junk.mthd")).is_err());
    std::fs::write(dir.join("notes.txt"), "x").unwrap();
    assert!(duplicate(&state, &safety, &dir.join("notes.txt")).is_err());
    assert!(duplicate(&state, &safety, &dir.join("gone.mthd")).is_err());
    let names = names_in(&dir);
    assert!(!names.iter().any(|n| n.ends_with(".tmp")), "{names:?}");
    assert!(!names.contains(&"junk 2.mthd".to_string()));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn save_new_free_takes_the_next_free_name() {
    let dir = std::fs::canonicalize(safety::tests::temp_dir("start-free")).unwrap();
    let safety = Safety::new(Some(&dir.join("data")));
    let state = AppState::with(Docs::default());
    std::fs::write(dir.join("Big Show - Camera.mthd"), "kept").unwrap();
    let (b, _) = build(&spec("Big Show - Camera"), None, &[]).unwrap();
    let (_, path) = save_new_free(&state, &safety, &b, &dir, "Big Show - Camera").unwrap();
    assert_eq!(path, dir.join("Big Show - Camera 2.mthd"));
    assert_eq!(
        std::fs::read_to_string(dir.join("Big Show - Camera.mthd")).unwrap(),
        "kept"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn thumbnails_are_kept_with_the_summary_until_the_file_changes() {
    let dir = std::fs::canonicalize(safety::tests::temp_dir("start-thumb")).unwrap();
    let a = dir.join("a.mthd");
    Session::with_budget(starter::budget(), None)
        .save_to(&a)
        .unwrap();
    let state = AppState::with(Docs::default());
    let start = StartState::load(Some(dir.join(START_FILE)));
    let v = list(&state, &start, std::slice::from_ref(&a));
    let report = crate::diagnose::lenient_recalc(&starter::budget())
        .report
        .unwrap();
    let bars = projects::thumbnail(&report);
    assert!(!bars.is_empty());
    assert_eq!(v.recent[0].bars, bars, "drawn from the topsheet's totals");
    assert_eq!(v.recent[0].kind, "budget");
    // Kept in the app's data folder, keyed by the file, never in the file.
    let kept = std::fs::read_to_string(dir.join(CACHE_FILE)).unwrap();
    assert!(kept.contains("\"bars\""), "{kept}");
    assert!(kept.contains(&format!("\"version\": {CACHE_VERSION}")));
    assert!(!std::fs::read_to_string(&a).unwrap().contains("\"bars\""));

    // The next run takes the kept bars without reading the file: kept
    // bars that differ from the file's show it was not read.
    let marked = kept.replacen("\"10 Story and rights\"", "\"From the cache\"", 1);
    assert_ne!(marked, kept);
    std::fs::write(dir.join(CACHE_FILE), &marked).unwrap();
    let start = StartState::load(Some(dir.join(START_FILE)));
    let v = list(&state, &start, std::slice::from_ref(&a));
    assert_eq!(v.recent[0].bars[0].label, "From the cache");

    // A changed file is read again: one category now.
    let mut one = starter::budget();
    let keep_first = one.categories().next().unwrap().code.clone();
    one.topsheet.retain(|r| match r {
        method_core::TopsheetRow::Category(c) => c.code == keep_first,
        _ => false,
    });
    Session::with_budget(one, None).save_to(&a).unwrap();
    let v = list(&state, &start, std::slice::from_ref(&a));
    assert_eq!(v.recent[0].bars.len(), 1);
    assert_eq!(v.recent[0].bars[0].share, 1000);

    // A cache from before thumbnails (no version) is read as empty.
    std::fs::write(
        dir.join(CACHE_FILE),
        marked.replace(&format!("\"version\": {CACHE_VERSION},"), ""),
    )
    .unwrap();
    let start = StartState::load(Some(dir.join(START_FILE)));
    let v = list(&state, &start, std::slice::from_ref(&a));
    assert_eq!(v.recent[0].bars.len(), 1, "read from the file");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn projects_show_their_budgets_by_folder() {
    let dir = std::fs::canonicalize(safety::tests::temp_dir("start-projects")).unwrap();
    let data = dir.join("data");
    let project = dir.join("Big Show");
    std::fs::create_dir_all(project.join("actuals")).unwrap();
    std::fs::create_dir_all(project.join("Big Show.mthd-log")).unwrap();
    std::fs::create_dir_all(project.join("versions")).unwrap();
    let mut parent = starter::budget();
    parent.title = "Big Show".into();
    Session::with_budget(parent.clone(), None)
        .save_to(&project.join("Big Show.mthd"))
        .unwrap();
    let row = method_session::sharing::RowInput {
        kind: "category".into(),
        code: "21".into(),
    };
    let sub =
        method_session::sharing::make_row_sub_budget(&parent, &row, "2026-10-20T10:00:00-07:00")
            .unwrap();
    Session::with_budget(sub, None)
        .save_to(&project.join("versions").join("Camera.mthd"))
        .unwrap();
    Session::with_budget(starter::blank("Pilot", "USD"), None)
        .save_to(&project.join("Pilot.mthd"))
        .unwrap();
    std::fs::write(
        project.join("Big Show.days.json"),
        "{\"format\": \"method-schedule-days\", \"version\": 1}",
    )
    .unwrap();
    std::fs::write(project.join("other.json"), "{\"format\": \"x\"}").unwrap();
    std::fs::write(project.join("actuals").join("2026-09 GL.csv"), "a,b").unwrap();
    std::fs::write(project.join("notes.txt"), "x").unwrap();
    std::fs::write(project.join(".hidden.mthd"), "x").unwrap();
    std::fs::write(project.join("Big Show.mthd-log").join("x.mthd"), "x").unwrap();

    let start = StartState::load(Some(data.join(START_FILE)));
    start.add_project(&project).unwrap();
    start.add_project(&project).unwrap();
    start.add_project(&dir.join("gone")).unwrap();
    let state = AppState::with(Docs::default());
    let v = list(&state, &start, &[]);
    assert_eq!(v.projects.len(), 2, "each folder once");
    let p = &v.projects[0];
    assert_eq!(p.name, "Big Show");
    assert!(p.problem.is_none() && !p.more);
    let sep = std::path::MAIN_SEPARATOR;
    let rows: Vec<(String, &str, bool, bool)> = p
        .items
        .iter()
        .map(|e| (e.file.clone(), e.kind.as_str(), e.under, e.openable))
        .collect();
    assert_eq!(
        rows,
        [
            ("Big Show.mthd".to_string(), "budget", false, true),
            (
                format!("versions{sep}Camera.mthd"),
                "sub-budget",
                true,
                true
            ),
            ("Pilot.mthd".to_string(), "budget", false, true),
            ("Big Show.days.json".to_string(), "schedule", false, false),
            (
                format!("actuals{sep}2026-09 GL.csv"),
                "actuals",
                false,
                false
            ),
        ]
    );
    assert_eq!(p.items[1].title, "Big Show - Camera");
    assert!(p.items[0].net_total.is_some() && !p.items[0].bars.is_empty());
    assert_eq!(
        v.projects[1].problem.as_deref(),
        Some("Can't find this folder.")
    );
    // Tokens name the folder and its files; only budgets open.
    assert_eq!(start.listed(p.token).unwrap(), project);
    assert_eq!(
        start.listed(p.items[0].token).unwrap(),
        project.join("Big Show.mthd")
    );
    assert!(is_budget_path(&start.listed(p.items[0].token).unwrap()));
    assert!(!is_budget_path(&start.listed(p.items[3].token).unwrap()));

    // Removing a project takes it off the list; the folder stays.
    start.remove_project(&project).unwrap();
    assert_eq!(start.projects(), [dir.join("gone")]);
    assert!(project.join("Big Show.mthd").is_file());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_big_project_folder_is_read_up_to_the_limit() {
    let dir = std::fs::canonicalize(safety::tests::temp_dir("start-big")).unwrap();
    for n in 0..SCAN_LIMIT + 5 {
        std::fs::write(dir.join(format!("{n:04}.txt")), "x").unwrap();
    }
    let (files, more) = scan(&dir);
    assert!(more);
    assert_eq!(files.len(), SCAN_LIMIT);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_project_folder_is_shown_itself() {
    let dir = std::fs::canonicalize(safety::tests::temp_dir("start-reveal")).unwrap();
    let cmd = reveal_command(&dir).unwrap();
    let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
    assert_eq!(args, [dir.as_os_str()]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_shared_budget_says_so_with_who_was_in_it() {
    use method_core::log::ActorId;
    use method_format::log::{LogDir, Presence};
    let dir = std::fs::canonicalize(safety::tests::temp_dir("start-shared")).unwrap();
    let a = dir.join("Big Show.mthd");
    let b = dir.join("Pilot.mthd");
    for p in [&a, &b] {
        Session::with_budget(starter::budget(), None)
            .save_to(p)
            .unwrap();
    }
    let log = LogDir::for_budget(&a);
    std::fs::create_dir_all(log.root()).unwrap();
    std::fs::write(log.root().join("budget.json"), "{}").unwrap();
    for (n, name, acct, at) in [
        (1u8, "Ann", None, "2026-10-19T10:00:00Z"),
        (2, "Bo", Some("2101"), "2026-10-20T09:00:00Z"),
    ] {
        let p = Presence {
            name: name.into(),
            account: acct.map(str::to_string),
            at: at.into(),
            ..Presence::default()
        };
        log.write_presence(ActorId([n; 32]), &p).unwrap();
    }
    let state = AppState::with(Docs::default());
    let start = StartState::load(Some(dir.join(START_FILE)));
    let v = list(&state, &start, &[a.clone(), b.clone()]);
    assert!(v.recent[0].shared);
    let people: Vec<(&str, &str)> = v.recent[0]
        .people
        .iter()
        .map(|p| (p.name.as_str(), p.place.as_str()))
        .collect();
    assert_eq!(people, [("Bo", "account 2101"), ("Ann", "")]);
    assert!(!v.recent[1].shared && v.recent[1].people.is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_linked_schedule_comes_in_before_what_the_form_says() {
    let days = method_session::schedule_days::read(
        "every-field-days.json",
        include_bytes!("../../../method-format/tests/fixtures/schedule/every-field-days.json"),
    )
    .unwrap();
    let (b, msg) = build_with(&spec("Harbor"), None, &[], Some(&days)).unwrap();
    assert_eq!(b.globals["SHOOT_DAYS"].0, "6");
    assert_eq!(
        info(&b, "Wrap end"),
        Some(InfoValue::Date("2027-03-26".into()))
    );
    assert!(msg.ends_with("linked for later imports."), "{msg}");
    // What is typed in the form wins.
    let typed = NewBudgetSpec {
        shoot_days: "30".into(),
        shoot_start: "2027-02-22".into(),
        ..spec("Harbor")
    };
    let (b, _) = build_with(&typed, None, &[], Some(&days)).unwrap();
    assert_eq!(b.globals["SHOOT_DAYS"].0, "30");
    assert_eq!(
        info(&b, "Shoot start"),
        Some(InfoValue::Date("2027-02-22".into()))
    );
    assert!(
        b.setup_sources.get("global", "SHOOT_DAYS").is_some(),
        "still linked"
    );
}

#[test]
fn projects_duplicate_and_row_sub_budgets_are_wired_for_the_main_window() {
    for (cmd, perm) in [
        ("start_duplicate", "allow-start-duplicate"),
        ("start_add_project", "allow-start-add-project"),
        ("start_remove_project", "allow-start-remove-project"),
        ("start_project_folder", "allow-start-project-folder"),
        ("sub_budget_from_row", "allow-sub-budget-from-row"),
    ] {
        assert!(
            include_str!("../app.rs").contains(&format!("::{cmd},")),
            "{cmd}"
        );
        assert!(
            include_str!("../../build.rs").contains(&format!("\"{cmd}\",")),
            "{cmd}"
        );
        assert!(
            include_str!("../../capabilities/main.json").contains(&format!("\"{perm}\"")),
            "{perm}"
        );
    }
}

#[test]
fn the_setup_step_is_wired_for_the_main_window() {
    assert!(include_str!("../app.rs").contains("crate::start::new_budget_setup,"));
    assert!(include_str!("../../build.rs").contains("\"new_budget_setup\","));
    assert!(include_str!("../../capabilities/main.json").contains("\"allow-new-budget-setup\""));
}
