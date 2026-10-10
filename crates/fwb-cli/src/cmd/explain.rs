//! `method explain`: how any total of a budget is worked out, step by step
//! (roadmap 3.5; the engine's `fwb_core::explain`).

use fwb_core::explain::{Explainer, Target};

use crate::cli::{excluding, load, CliError, Flags, Outcome};

pub const USAGE: &str = "  method explain <file.mthd> <total> [--depth N] [--exclude GROUP]...
      Explain how a total is worked out, as an indented tree whose leaves add
      up to it: lines (amount x X x 4X x rate, globals, currency, rounding),
      fringes (cutoffs, ranges, shares), posting, left-out lines, charges and
      credits. <total> is an account (`21-01`, or `21-01/lines`,
      `21-01/fringes`), a category (`21`, `21/fringes`), a line (`21-01:3`
      for line 3, or `#ID`), `level:NAME`, `charge:NAME`, `credit:NAME`,
      `fringe:NAME`, or atl, btl, atl+btl, fringes, credits, grand
      (or topsheet), net. --depth N prints N levels.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &["--depth", "--exclude"], &[], USAGE)?;
    let pos = f.exactly(2, "the budget file and the total to explain", USAGE)?;
    let (path, what) = (&pos[0], &pos[1]);
    let depth = match f.value("--depth") {
        Some(d) => Some(
            d.parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| CliError::usage("--depth takes a whole number from 1", USAGE))?,
        ),
        None => None,
    };
    let opts = excluding(&f);
    let b = load(path)?;
    let ex = Explainer::new(&b, &opts).map_err(|e| CliError::failed(format!("{path}: {e}")))?;
    let target = Target::parse(&b, ex.report(), what).map_err(|e| CliError::usage(e.0, USAGE))?;
    let e = ex
        .explain(&target)
        .map_err(|e| CliError::failed(format!("{path}: {e}")))?;
    Ok(e.to_text(depth))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{exit, save};

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn explains_totals_of_a_file() {
        let dir = std::env::temp_dir().join(format!("fwb-cli-explain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Film.fwb").to_string_lossy().into_owned();
        save(&file, &fwb_core::starter::budget()).unwrap();

        let out = run(&args(&[&file, "grand", "--depth", "2"])).unwrap();
        assert!(out.starts_with("Grand total: "), "{out}");
        assert!(out.contains("- Total above and below-the-line"), "{out}");
        let net = run(&args(&[&file, "net"])).unwrap();
        assert!(net.starts_with("Net total: "), "{net}");

        let b = load(&file).unwrap();
        let acct = b.categories().flat_map(|c| &c.accounts).next().unwrap();
        let out = run(&args(&[&file, &acct.code])).unwrap();
        assert!(out.starts_with(&format!("Account {}", acct.code)), "{out}");

        let e = run(&args(&[&file, "nothing-here"])).unwrap_err();
        assert_eq!(e.code, exit::USAGE);
        assert!(e.message.contains("doesn't name a total"), "{}", e.message);
        assert_eq!(run(&args(&[&file])).unwrap_err().code, exit::USAGE);
        assert_eq!(
            run(&args(&[&file, "grand", "--depth", "0"]))
                .unwrap_err()
                .code,
            exit::USAGE
        );
        let missing = dir.join("none.fwb").to_string_lossy().into_owned();
        assert_eq!(
            run(&args(&[&missing, "grand"])).unwrap_err().code,
            exit::FAILED
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
