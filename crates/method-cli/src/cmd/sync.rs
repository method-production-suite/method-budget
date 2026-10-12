//! `method sync`: merge every actor's changes in a shared folder, record
//! outside edits to the `.mthd`, and write the merged budget back as the
//! keeper does (docs/CLI.md; docs/research/collaboration.md, section 5).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use method_format::log::{self, LogDir};

use crate::cli::{exit, CliError, Flags, Outcome};
use crate::cmd::log::{now, open_shared};

pub const USAGE: &str = "  method sync <file.mthd|FOLDER> [--no-write] [--roll]
      Merge every actor's changes in the budget's change log (every
      shared budget in FOLDER), record an outside edit to the .mthd as a
      change, and write the merged budget as the .mthd. --no-write merges
      and reports only. --roll joins this computer's segment files into
      one. Exit 1 when a budget could not be merged.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &[], &["--no-write", "--roll"], USAGE)?;
    let targets = f.exactly(1, "the budget file or folder", USAGE)?;
    let target = Path::new(&targets[0]);
    let budgets = if target.is_dir() {
        shared_budgets(target)?
    } else {
        vec![target.to_path_buf()]
    };
    if budgets.is_empty() {
        return Err(CliError::failed(format!(
            "{}: no shared budget here (a .mthd with a .mthd-log folder beside it)",
            target.display()
        )));
    }
    let mut out = String::new();
    let mut failed = false;
    for path in budgets {
        match sync_one(&path, f.has("--no-write"), f.has("--roll")) {
            Ok(text) => out.push_str(&text),
            Err(e) => {
                failed = true;
                let _ = writeln!(out, "{}: {}", path.display(), e.message);
            }
        }
    }
    if failed {
        print!("{out}");
        return Err(CliError::new(exit::FAILED, ""));
    }
    Ok(out)
}

/// Every `.mthd` in `dir` with a change log beside it.
fn shared_budgets(dir: &Path) -> Result<Vec<PathBuf>, CliError> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| method_format::has_budget_extension(p) && LogDir::for_budget(p).exists())
        .collect();
    out.sort();
    Ok(out)
}

fn sync_one(path: &Path, no_write: bool, roll: bool) -> Result<String, CliError> {
    let (mut shared, report) = open_shared(path)?;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{}: {} changes by {} actors",
        path.display(),
        shared.log.len(),
        shared.log.frontier().0.len()
    );
    for (path, why) in &report.skipped {
        let _ = writeln!(s, "  skipped {}: {why}", path.display());
    }
    for (id, why) in &report.rejected {
        let _ = writeln!(s, "  refused {id}: {why}");
    }
    if report.pending > 0 {
        let _ = writeln!(
            s,
            "  {} changes are waiting for ones that haven't arrived",
            report.pending
        );
    }
    let time = now();
    if let Some(id) = log::ingest(&mut shared, &time).map_err(|e| CliError::failed(e.0))? {
        let n = shared.log.change(id).map(|c| c.ops.len()).unwrap_or(0);
        let _ = writeln!(s, "  recorded an outside edit to the file ({n} ops)");
    }
    if let Some(seg) = log::flush(&shared.log, &shared.dir).map_err(|e| CliError::failed(e.0))? {
        let _ = writeln!(s, "  wrote {}", seg.display());
    }
    if roll {
        if let Some(p) = shared
            .dir
            .roll(shared.log.actor())
            .map_err(|e| CliError::failed(e.0))?
        {
            let _ = writeln!(s, "  rolled this computer's segments into {}", p.display());
        }
    }
    let proj = if no_write {
        shared.log.project()
    } else {
        let proj = log::write_snapshot(&shared).map_err(|e| CliError::failed(e.0))?;
        let _ = writeln!(s, "  wrote the merged budget to {}", path.display());
        proj
    };
    if !proj.review.is_empty() {
        let _ = writeln!(s, "  to review:");
        for item in &proj.review {
            let _ = writeln!(s, "    {}", item.sentence);
        }
    }
    Ok(s)
}
