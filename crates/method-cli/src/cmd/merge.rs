//! `method-budget merge`: three-way merge of budgets (`method_core::merge`;
//! docs/MERGE.md). `method-budget merge-driver` (src/cmd/merge_driver.rs) is the same
//! merge for git.

use std::fmt::Write as _;
use std::path::Path;

use method_core::merge::{merge_with, Merged, Side};

use crate::cli::{exit, load, not_the_input, save, CliError, Flags, Outcome};

pub const USAGE: &str =
    "  method-budget merge <base.mthd> <ours.mthd> <theirs.mthd> -o <out.mthd> [--ours|--theirs]
      Three-way merge: both budgets' changes since their common base.
      Changes to different rows, fields and setup items merge on their own;
      the same field changed both ways, or something deleted on one side and
      changed on the other, is a conflict, settled for ours (or --theirs)
      and listed. The merged budget is written either way; exit 1 when
      conflicts were settled that way.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &["--out|-o"], &["--ours", "--theirs"], USAGE)?;
    let files = f.exactly(3, "the base, ours and theirs files", USAGE)?;
    let out = f
        .value("--out")
        .ok_or_else(|| CliError::usage("merge needs -o <out.mthd>", USAGE))?;
    let side = default_side(&f, USAGE)?;
    let (base, ours, theirs) = (load(&files[0])?, load(&files[1])?, load(&files[2])?);
    // Never over one of the budgets being merged (merge-driver's job, not this one's).
    for input in files {
        not_the_input(input, Path::new(out), "merged")?;
    }
    let m = merge_with(&base, &ours, &theirs, side, &Default::default());
    save(out, &m.budget)?;
    finish(out, &m, side)
}

/// `--ours` (the default) or `--theirs`; both is a usage error.
pub fn default_side(f: &Flags, usage: &str) -> Result<Side, CliError> {
    match (f.has("--ours"), f.has("--theirs")) {
        (true, true) => Err(CliError::usage("give --ours or --theirs, not both", usage)),
        (_, true) => Ok(Side::Theirs),
        _ => Ok(Side::Ours),
    }
}

/// The report: what was written, rows renumbered, a problem, every
/// conflict and the side taken. Conflicts left: printed, and exit 1.
pub fn finish(out: &str, m: &Merged, side: Side) -> Outcome {
    let text = report(out, m);
    if m.conflicts.is_empty() {
        return Ok(text);
    }
    print!("{text}");
    let n = m.conflicts.len();
    Err(CliError::new(
        exit::FAILED,
        format!(
            "{n} conflict{} settled for {}; check them in {out}",
            if n == 1 { "" } else { "s" },
            side.key()
        ),
    ))
}

pub fn report(out: &str, m: &Merged) -> String {
    let mut s = format!("wrote {out}\n");
    if m.renumbered > 0 {
        let _ = writeln!(
            s,
            "{} added on both sides with the same ids: theirs got new ids",
            if m.renumbered == 1 {
                "1 row or version".to_string()
            } else {
                format!("{} rows or versions", m.renumbered)
            }
        );
    }
    if let Some(p) = &m.problem {
        let _ = writeln!(s, "the merged budget has a problem: {p}");
    }
    if !m.conflicts.is_empty() {
        let n = m.conflicts.len();
        let _ = writeln!(s, "{n} conflict{}:", if n == 1 { "" } else { "s" });
        for c in &m.conflicts {
            let _ = writeln!(s, "  {} (took {})", c.text, c.taken.key());
        }
    }
    s
}
