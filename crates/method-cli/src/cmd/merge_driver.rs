//! `method-budget merge-driver`: `method-budget merge` as a git merge driver (docs/MERGE.md,
//! "With git"). git runs it as `method-budget merge-driver %O %A %B %P`: the base,
//! ours (which the result replaces) and theirs, all temporary files, and the
//! file's path in the repository for messages.

use method_core::merge::merge_with;

use super::merge::{default_side, finish};
use crate::cli::{load, save, CliError, Flags, Outcome};

pub const USAGE: &str = "  method-budget merge-driver <base> <ours> <theirs> [<path>] [--theirs]
      For git (`%O %A %B %P`): merges into <ours> in place, conflicts settled
      for ours (or --theirs) and listed; exit 1 when there were any, so git
      marks the file conflicted. Set up in docs/MERGE.md.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &[], &["--ours", "--theirs"], USAGE)?;
    let files = f.positional();
    if !(3..=4).contains(&files.len()) {
        return Err(CliError::usage(
            if files.len() < 3 {
                "missing the base, ours and theirs files"
            } else {
                "too many arguments"
            },
            USAGE,
        ));
    }
    let side = default_side(&f, USAGE)?;
    let shown = files.get(3).unwrap_or(&files[1]);
    // git passes an empty base when both branches added the file.
    if std::fs::metadata(&files[0]).is_ok_and(|m| m.len() == 0) {
        return Err(CliError::failed(format!(
            "{shown}: no common base (both branches added it); merge it in the app"
        )));
    }
    let named = |e: CliError| CliError::new(e.code, format!("{shown}: {}", e.message));
    let base = load(&files[0]).map_err(named)?;
    let ours = load(&files[1]).map_err(named)?;
    let theirs = load(&files[2]).map_err(named)?;
    let m = merge_with(&base, &ours, &theirs, side, &Default::default());
    save(&files[1], &m.budget)?;
    finish(shown, &m, side)
}
