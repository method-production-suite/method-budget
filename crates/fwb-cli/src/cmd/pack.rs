//! `method pack`: setup packs from the command line (docs/CLI.md, "Setup
//! packs"; decisions.md, entry 50). `export` writes a pack of a budget's
//! picked setup items and everything they need; `import` brings a pack's
//! items into a budget with Import Setup's rules (the budget keeps its own
//! version of what differs unless `--theirs`) and stamps them with the
//! pack's title, version and effective date; `show` prints a pack.

use std::path::Path;

use fwb_format::setup::import::{self, ItemRef, Resolution, What};
use fwb_format::setup::pack::{self, PackDetails};

use crate::cli::{load, not_the_input, save, write_atomic, CliError, Flags, Outcome};

pub const USAGE: &str =
    "  method pack export <budget.mthd> -o <pack.mthd> [--kinds fringes,globals]
        [--items NAME,NAME] [--item NAME] [--title T] [--description D] [--author A]
        [--pack-version V] [--effective YYYY-MM-DD] [--region R] [--notes N]
        [--url URL]... [--created DATE]
      Write a setup pack: the budget's setup items of --kinds (all kinds when
      left out), or just --items (`kind:Name` names the kind), with everything
      they need (a fringe's globals and unit, a charge's groups, a tag
      preset's tags). The title defaults to the budget's.
  method pack import <pack.mthd> <budget.mthd> [--ours | --theirs]
        [--kinds K,K] [--items NAME,NAME] [--item NAME] [--dry-run]
      Import a pack's items (all, or the picked ones) and what they need into
      the budget, and save it atomically. Where the budget has an item of the
      same name set up differently it keeps its own (--ours, the default), or
      takes the pack's (--theirs). Imported items are stamped with the pack's
      title, version and effective date. --dry-run lists what would happen.
  method pack show <pack.mthd>
      Print a pack's details, its items and where each came from.
";

const VALUES: &[&str] = &[
    "--out|-o",
    "--kinds",
    "--items",
    "--item",
    "--title",
    "--description",
    "--author",
    "--pack-version",
    "--effective",
    "--region",
    "--notes",
    "--url",
    "--created",
];
const SWITCHES: &[&str] = &["--ours", "--theirs", "--dry-run"];

pub fn run(args: &[String]) -> Outcome {
    let Some((action, rest)) = args.split_first() else {
        return Err(CliError::usage("pack needs export, import or show", USAGE));
    };
    let f = Flags::parse(rest, VALUES, SWITCHES, USAGE)?;
    match action.as_str() {
        "export" => export(&f),
        "import" => import_into(&f),
        "show" => show(&f),
        other => Err(CliError::usage(
            format!("unknown pack action `{other}`: use export, import or show"),
            USAGE,
        )),
    }
}

/// `--kinds` as kinds (refused when one isn't a pack kind).
fn kinds(f: &Flags) -> Result<Vec<What>, CliError> {
    let mut out = Vec::new();
    for word in f.values("--kinds").iter().flat_map(|v| v.split(',')) {
        let word = word.trim();
        if word.is_empty() {
            continue;
        }
        let w = pack::parse_kind(word).ok_or_else(|| {
            CliError::usage(
                format!("unknown kind `{word}` (globals, fringes, groups, units, sets, locations, currencies, charges, credits, tag-presets, line-blocks, report-presets, ot-patterns)"),
                USAGE,
            )
        })?;
        if !out.contains(&w) {
            out.push(w);
        }
    }
    Ok(out)
}

/// `--items A,B` (split on commas) and `--item NAME` (taken whole).
fn names(f: &Flags) -> Vec<String> {
    let mut out: Vec<String> = f
        .values("--items")
        .iter()
        .flat_map(|v| v.split(','))
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    out.extend(f.values("--item").iter().map(|n| n.trim().to_string()));
    out
}

/// Today's date (UTC) as `YYYY-MM-DD`: when a pack was made, unless
/// `--created` says.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// "3 globals, 2 fringes": what a list of items holds, by kind.
fn by_kind(items: &[(What, String)]) -> String {
    let mut parts = Vec::new();
    for w in What::ALL {
        let n = items.iter().filter(|(k, _)| *k == w).count();
        if n > 0 {
            let word = w.key().replace('_', " ");
            parts.push(if n == 1 {
                format!("1 {word}")
            } else {
                format!("{n} {word}s")
            });
        }
    }
    parts.join(", ")
}

fn export(f: &Flags) -> Outcome {
    let p = f.exactly(1, "the budget file", USAGE)?;
    let input = &p[0];
    let out = f
        .value("--out")
        .ok_or_else(|| CliError::usage("pack export needs -o <pack.mthd>", USAGE))?;
    let out_path = Path::new(out);
    if out_path.exists() {
        not_the_input(input, out_path, "packed")?;
    }
    let src = load(input)?;
    let picks = pack::picks_for(&src, &kinds(f)?, &names(f)).map_err(CliError::rejected)?;
    if picks.is_empty() {
        return Err(CliError::rejected(
            "nothing to pack: the budget has no setup items of those kinds",
        ));
    }
    let details = PackDetails {
        title: f
            .value("--title")
            .map(str::to_string)
            .unwrap_or_else(|| src.title.clone()),
        description: f.value("--description").unwrap_or_default().to_string(),
        author: f.value("--author").unwrap_or_default().to_string(),
        version: f.value("--pack-version").unwrap_or_default().to_string(),
        effective: f.value("--effective").unwrap_or_default().to_string(),
        region: f.value("--region").unwrap_or_default().to_string(),
        notes: f.value("--notes").unwrap_or_default().to_string(),
        urls: f.values("--url").iter().map(|u| u.to_string()).collect(),
    };
    let created = f.value("--created").map_or_else(today, str::to_string);
    let (built, done) = pack::build(&src, &picks, &details, &created, env!("CARGO_PKG_VERSION"))
        .map_err(CliError::rejected)?;
    write_atomic(out_path, fwb_format::to_string(&built).as_bytes())?;
    let items: Vec<(What, String)> = done
        .written
        .iter()
        .map(|(w, _, n)| (*w, n.clone()))
        .collect();
    Ok(format!(
        "Wrote the setup pack {out}: {} ({}).\n",
        import::count(items.len(), "item", "items"),
        by_kind(&items)
    ))
}

fn import_into(f: &Flags) -> Outcome {
    let p = f.exactly(2, "the pack and the budget", USAGE)?;
    let (pack_path, budget_path) = (&p[0], &p[1]);
    if f.has("--ours") && f.has("--theirs") {
        return Err(CliError::usage(
            "choose --ours or --theirs, not both",
            USAGE,
        ));
    }
    if std::fs::canonicalize(pack_path)
        .ok()
        .is_some_and(|a| std::fs::canonicalize(budget_path).ok() == Some(a))
    {
        return Err(CliError::failed(
            "the pack and the budget are the same file",
        ));
    }
    let src = load(pack_path)?;
    let mut b = load(budget_path)?;
    let picks: Vec<ItemRef> =
        pack::picks_for(&src, &kinds(f)?, &names(f)).map_err(CliError::rejected)?;
    if picks.is_empty() {
        return Err(CliError::rejected(
            "nothing to import: the pack has no such items",
        ));
    }
    let review = import::preview(&b, &src, 0, pack_path, pack_path);
    let wanted = import::closure(
        &src,
        picks
            .iter()
            .filter_map(|p| What::parse(&p.kind).ok().map(|w| (w, p.name.clone()))),
    );
    let choice = if f.has("--theirs") {
        "imported"
    } else {
        "keep"
    };
    let resolutions: Vec<Resolution> = review
        .items
        .iter()
        .filter(|it| it.status == "differs")
        .filter(|it| {
            What::parse(it.kind)
                .ok()
                .is_some_and(|w| wanted.contains(&(w, it.name.clone())))
        })
        .map(|it| Resolution {
            kind: it.kind.to_string(),
            name: it.name.clone(),
            choice: choice.to_string(),
        })
        .collect();
    let mut text = String::new();
    if f.has("--dry-run") {
        for it in &review.items {
            let Ok(w) = What::parse(it.kind) else {
                continue;
            };
            if !wanted.contains(&(w, it.name.clone())) {
                continue;
            }
            let what = match it.status {
                "new" => "add".to_string(),
                "same" => "same".to_string(),
                _ if choice == "imported" => "replace (theirs)".to_string(),
                _ => "keep (ours)".to_string(),
            };
            text.push_str(&format!(
                "{what:<16} {} {}\n",
                it.kind.replace('_', " "),
                it.name
            ));
        }
    }
    let done = pack::import_pack(&mut b, &src, &picks, &resolutions).map_err(CliError::rejected)?;
    if f.has("--dry-run") {
        text.push_str(&format!("Dry run: {} Nothing written.\n", done.message()));
        return Ok(text);
    }
    if done.added > 0 || done.replaced > 0 {
        save(budget_path, &b)?;
    }
    text.push_str(&done.message());
    text.push('\n');
    Ok(text)
}

fn show(f: &Flags) -> Outcome {
    let p = f.exactly(1, "the pack file", USAGE)?;
    let src = load(&p[0])?;
    let s = pack::summary(&src);
    let mut text = String::new();
    if s.is_pack {
        for line in pack::describe(&s) {
            text.push_str(&line);
            text.push('\n');
        }
    } else {
        text.push_str(&format!(
            "{} is not a setup pack (an ordinary budget can be imported from too).\n",
            p[0]
        ));
    }
    let items: Vec<(What, String)> = import::all_items(&src)
        .into_iter()
        .filter(|(w, _)| pack::pack_kind(*w))
        .collect();
    text.push_str(&format!(
        "\n{} ({})\n",
        import::count(items.len(), "item", "items"),
        by_kind(&items)
    ));
    for (w, name) in &items {
        text.push_str(&format!(
            "  {} {}: {}\n",
            w.key().replace('_', " "),
            name,
            import::describe(&src, *w, name)
        ));
        if let Some(src_text) = s
            .sources
            .iter()
            .find(|x| x.kind == w.key() && x.name == *name)
        {
            text.push_str(&format!("      {}\n", src_text.text));
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::exit;
    use fwb_core::{starter, Fringe, FringeKind};

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("method-pack-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn today_is_a_date() {
        let t = today();
        assert_eq!(t.len(), 10);
        assert!(fwb_core::pack::check_date(&t).is_ok(), "{t}");
    }

    #[test]
    fn export_then_import_round_trip() {
        let d = dir("round");
        let budget = d.join("Show.mthd").to_string_lossy().into_owned();
        let mut b = starter::budget();
        b.globals.insert("PENSION_RATE".into(), "8.5".into());
        b.fringes.insert(
            "PENSION".into(),
            Fringe {
                description: "Pension".into(),
                short_code: None,
                rate: "PENSION_RATE".into(),
                kind: FringeKind::Percent { cutoff: None },
            },
        );
        save(&budget, &b).unwrap();
        let packf = d.join("Rates.mthd").to_string_lossy().into_owned();
        let out = run(&s(&[
            "export",
            &budget,
            "-o",
            &packf,
            "--items",
            "fringe:PENSION",
            "--title",
            "Union Rates",
            "--pack-version",
            "2026.1",
            "--effective",
            "2026-01-01",
            "--url",
            "https://example.org/scale",
            "--created",
            "2026-10-10",
        ]))
        .unwrap();
        assert!(out.contains("2 items (1 global, 1 fringe)"), "{out}");
        let p = load(&packf).unwrap();
        assert!(fwb_core::pack::is_pack(&p));
        assert!(
            p.globals.contains_key("PENSION_RATE"),
            "the fringe's global"
        );

        let shown = run(&s(&["show", &packf])).unwrap();
        assert!(shown.starts_with("Union Rates (2026.1)\n"), "{shown}");
        assert!(shown.contains("Effective: 2026-01-01"));

        // Into a budget whose PENSION_RATE differs: ours by default.
        let target = d.join("Other.mthd").to_string_lossy().into_owned();
        let mut t = starter::budget();
        t.globals.insert("PENSION_RATE".into(), "9".into());
        save(&target, &t).unwrap();
        let dry = run(&s(&["import", &packf, &target, "--dry-run"])).unwrap();
        assert!(
            dry.contains("keep (ours)") && dry.contains("Nothing written"),
            "{dry}"
        );
        assert_eq!(load(&target).unwrap(), t, "a dry run writes nothing");
        let out = run(&s(&["import", &packf, &target])).unwrap();
        assert!(
            out.contains("1 item added") && out.contains("1 item kept"),
            "{out}"
        );
        let after = load(&target).unwrap();
        assert_eq!(after.globals["PENSION_RATE"].0, "9");
        let stamp = after.setup_sources.get("fringe", "PENSION").unwrap();
        assert_eq!(stamp.pack, "Union Rates");
        assert_eq!(stamp.effective, "2026-01-01");
        // Theirs.
        let out = run(&s(&["import", &packf, &target, "--theirs"])).unwrap();
        assert!(out.contains("1 item replaced"), "{out}");
        assert_eq!(load(&target).unwrap().globals["PENSION_RATE"].0, "8.5");

        // Mistakes.
        let e = run(&s(&["import", &packf, &target, "--ours", "--theirs"])).unwrap_err();
        assert_eq!(e.code, exit::USAGE);
        let e = run(&s(&[
            "export", &budget, "-o", &packf, "--kinds", "settings",
        ]))
        .unwrap_err();
        assert_eq!(e.code, exit::USAGE);
        let e = run(&s(&["export", &budget, "-o", &packf, "--items", "NOPE"])).unwrap_err();
        assert_eq!(e.code, exit::REJECTED);
        let e = run(&s(&["export", &budget, "-o", &budget])).unwrap_err();
        assert!(e.message.contains("budget being packed"), "{e}");
        let e = run(&s(&[
            "export",
            &budget,
            "-o",
            &packf,
            "--effective",
            "soon",
        ]))
        .unwrap_err();
        assert_eq!(e.code, exit::REJECTED);
        assert!(run(&s(&["frobnicate"])).is_err());
        assert!(run(&[]).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
