//! `method coa`: charts of accounts. `method coa import` reads a chart from
//! a spreadsheet or another budget file and writes it as a new template (or
//! applies it to a copy of a budget); `method coa list` lists the bundled
//! chart templates (`method new --template` starts from one).

use std::fmt::Write as _;
use std::path::Path;

use fwb_core::coa::{apply, ApplyOptions};
use fwb_format::import::coa::{build_template, read_source, CoaMappingFile};

use crate::cli::{load, save, CliError, Flags, Outcome};

pub const USAGE: &str =
    "  method coa import <chart.xlsx|.csv|.txt|.mthd> -o <out.mthd> [--sheet N|NAME]
                [--mapping <mapping.json>] [--title TEXT] [--currency CODE]
                [--into <budget.mthd>] [--remove-unlisted] [--force] [--dry-run]
      Read a chart of accounts (category and account numbers, names and
      levels) and write it as a new budget with no lines: a template. The
      header row, columns and numbering pattern are found unless --mapping
      names them (docs/CLI.md). --into applies the chart to a copy of that
      budget instead: missing categories and accounts are added and existing
      ones renamed by number; --remove-unlisted also removes accounts the
      chart doesn't list when they are empty (never one with lines).
  method coa list
      List the bundled chart-of-accounts templates.
";

/// How many skipped rows and warnings are listed.
const LISTED: usize = 20;

pub fn run(args: &[String]) -> Outcome {
    match args.first().map(String::as_str) {
        Some("import") => import(&args[1..]),
        Some("list") => {
            Flags::parse(&args[1..], &[], &[], USAGE)?.exactly(0, "", USAGE)?;
            Ok(list())
        }
        Some(other) => Err(CliError::usage(
            format!("unknown coa command `{other}`"),
            USAGE,
        )),
        None => Err(CliError::usage("coa needs import or list", USAGE)),
    }
}

fn list() -> String {
    let mut out = String::new();
    for group in fwb_format::charts::GROUPS {
        let charts: Vec<_> = fwb_format::charts::all()
            .iter()
            .filter(|c| c.group == group)
            .collect();
        if charts.is_empty() {
            continue;
        }
        let _ = writeln!(out, "{group}");
        for c in charts {
            let _ = writeln!(out, "  {:<22} {}: {}", c.id, c.name, c.description);
        }
    }
    out
}

fn import(args: &[String]) -> Outcome {
    let f = Flags::parse(
        args,
        &[
            "--out|-o",
            "--sheet",
            "--mapping",
            "--title",
            "--currency",
            "--into",
        ],
        &["--force", "--dry-run", "--remove-unlisted"],
        USAGE,
    )?;
    let src = &f.exactly(1, "the chart file", USAGE)?[0];
    let out = f
        .value("--out")
        .ok_or_else(|| CliError::usage("coa import needs -o <out.mthd>", USAGE))?;
    let dry_run = f.has("--dry-run");

    let mut mapping = match f.value("--mapping") {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?;
            CoaMappingFile::parse(&text).map_err(|e| CliError::failed(format!("{p}: {e}")))?
        }
        None => CoaMappingFile::default(),
    };
    if let Some(s) = f.value("--sheet") {
        match s.parse::<usize>() {
            Ok(0) => return Err(CliError::usage("sheets count from 1", USAGE)),
            Ok(n) => {
                mapping.sheet = Some(n - 1);
                mapping.sheet_name = None;
            }
            Err(_) => mapping.sheet_name = Some(s.to_string()),
        }
    }
    let bytes = std::fs::read(src).map_err(|e| format!("{src}: {e}"))?;
    let file_name = Path::new(src)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = Path::new(src)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Chart".into());

    let out_path = Path::new(out);
    if !dry_run && out_path.exists() && !f.has("--force") {
        let same_as_into = f.value("--into").is_some_and(|p| {
            let a = std::fs::canonicalize(p).ok();
            a.is_some() && a == std::fs::canonicalize(out_path).ok()
        });
        if !same_as_into {
            return Err(CliError::failed(format!(
                "{out} already exists; pass --force to replace it"
            )));
        }
    }

    let mut report = String::new();
    let (budget, source) = match f.value("--into") {
        Some(into) => {
            let source = read_source(&bytes, &file_name, Some(&mapping))
                .map_err(|e| CliError::failed(format!("{src}: {e}")))?;
            let mut b = load(into)?;
            if let Some(t) = f.value("--title") {
                b.title = t.to_string();
            }
            let opts = ApplyOptions {
                remove_unlisted: f.has("--remove-unlisted"),
            };
            let rep = apply(&mut b, &source.chart, &opts)
                .map_err(|e| CliError::failed(format!("{src}: {e}")))?;
            let _ = writeln!(
                report,
                "applied to {into}: added {} categories and {} accounts, renamed {} categories and {} accounts",
                rep.added_categories.len(),
                rep.added_accounts.len(),
                rep.renamed_categories.len(),
                rep.renamed_accounts.len()
            );
            if !rep.removed_accounts.is_empty() || !rep.removed_categories.is_empty() {
                let _ = writeln!(
                    report,
                    "removed {} empty accounts and {} empty categories the chart doesn't list",
                    rep.removed_accounts.len(),
                    rep.removed_categories.len()
                );
            }
            if !rep.kept.is_empty() {
                let _ = writeln!(
                    report,
                    "kept {} accounts the chart doesn't list (they have lines or notes):",
                    rep.kept.len()
                );
                for a in rep.kept.iter().take(LISTED) {
                    let _ = writeln!(report, "  {} {}", a.code, a.name);
                }
            }
            for n in rep.not_fitting.iter().take(LISTED) {
                let _ = writeln!(report, "left out (doesn't fit the budget's numbering): {n}");
            }
            (b, source)
        }
        None => {
            let title = f.value("--title").unwrap_or(&stem);
            build_template(
                &bytes,
                &file_name,
                Some(&mapping),
                title,
                f.value("--currency").unwrap_or("USD"),
            )
            .map_err(|e| CliError::failed(format!("{src}: {e}")))?
        }
    };
    fwb_core::recalc(&budget, &fwb_core::RecalcOptions::default())
        .map_err(|e| CliError::failed(format!("{src}: the result doesn't recalculate: {e}")))?;

    let chart = &source.chart;
    let k = chart.coding;
    let pattern = {
        let mut widths = vec![k.category_width.to_string(), k.account_width.to_string()];
        if k.sub_width > 0 {
            widths.push(k.sub_width.to_string());
        }
        let sep = match k.separator {
            None => "no separator".to_string(),
            Some(' ') => "space".to_string(),
            Some(c) => format!("\"{c}\""),
        };
        format!("{} ({sep})", widths.join(" + "))
    };
    let mut head = format!(
        "{} categories, {} accounts, {} production totals; numbers {pattern}",
        chart.category_count(),
        chart.account_count(),
        chart.level_count(),
    );
    if let Some(s) = &source.sheet {
        let _ = write!(head, "; sheet \"{s}\"");
    }
    let _ = writeln!(head);
    report.insert_str(0, &head);
    if let Some(r) = &source.read {
        if r.default_levels_added {
            let _ = writeln!(
                report,
                "the file names no levels: our default production totals were added"
            );
        }
        if r.totals_skipped > 0 {
            let _ = writeln!(report, "{} total rows skipped", r.totals_skipped);
        }
        for (what, notes) in [("skipped", &r.skipped), ("check", &r.warnings)] {
            for n in notes.iter().take(LISTED) {
                if n.row == 0 {
                    let _ = writeln!(report, "{what}: {}", n.message);
                } else {
                    let _ = writeln!(report, "{what}: row {}: {}", n.row, n.message);
                }
            }
            if notes.len() > LISTED {
                let _ = writeln!(report, "{what}: and {} more", notes.len() - LISTED);
            }
        }
    }
    if dry_run {
        let _ = writeln!(report, "dry run: nothing written");
    } else {
        save(out, &budget)?;
        let _ = writeln!(report, "wrote {out}");
    }
    Ok(report)
}
