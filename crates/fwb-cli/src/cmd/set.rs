//! `method set`: change one value in a budget from the command line.

use crate::cli::{CliError, Flags, Outcome};
use crate::script::{self, Op, RowRef, Word, LINE_FIELDS};

pub const USAGE: &str = "  method set <file.mthd> <path> <value> [--dry-run]
      Change one value and save atomically. Paths: title, compact_versions,
      global.NAME, fringe.NAME.rate, fringe.NAME.cutoff, line.ROW.FIELD (ROW is
      #ID, ACCOUNT:N or ACCOUNT:*; FIELD as in `set line`), charge.NAME.applied.
      `none` or \"\" clears an optional value. --dry-run prints what would
      change and writes nothing. Exit 3 when the edit is refused.
";

/// `prefix.MIDDLE.last` split at the first and last dots, so a name or an
/// account code with dots in it (`11.00`) stays whole.
fn middle<'a>(rest: &'a str, path: &str) -> Result<(&'a str, &'a str), CliError> {
    rest.rsplit_once('.')
        .filter(|(m, l)| !m.is_empty() && !l.is_empty())
        .ok_or_else(|| CliError::usage(format!("`{path}` needs a name and a field"), USAGE))
}

/// The edit command a path and value mean.
pub fn op_for(path: &str, value: &str) -> Result<Op, CliError> {
    let v = Word::plain(value);
    let (head, rest) = path.split_once('.').unwrap_or((path, ""));
    let bad = |what: String| CliError::usage(what, USAGE);
    Ok(match head {
        "title" if rest.is_empty() => Op::Title(value.to_string()),
        "compact_versions" if rest.is_empty() => match value {
            "true" | "yes" | "on" => Op::CompactVersions(true),
            "false" | "no" | "off" => Op::CompactVersions(false),
            _ => return Err(bad(format!("`{value}` is not true or false"))),
        },
        "global" if !rest.is_empty() => Op::Global(rest.to_string(), value.to_string()),
        "fringe" => match middle(rest, path)? {
            (name, "rate") => Op::FringeRate(name.to_string(), value.to_string()),
            (name, "cutoff") => {
                // The script parser handles amounts and `none`.
                let line = format!(
                    "set fringe {} cutoff {}",
                    quote(name),
                    if v.text.is_empty() {
                        "none".into()
                    } else {
                        quote(value)
                    }
                );
                match script::parse(&line).map_err(CliError::rejected)? {
                    Some(op @ Op::FringeCutoff(..)) => op,
                    _ => return Err(bad(format!("`{path}` is not a fringe field"))),
                }
            }
            (_, other) => return Err(bad(format!("a fringe has no field `{other}`"))),
        },
        "line" => {
            let (row, field) = middle(rest, path)?;
            if !LINE_FIELDS.contains(&field) {
                return Err(bad(format!(
                    "a line has no field `{field}`: use one of {}",
                    LINE_FIELDS.join(", ")
                )));
            }
            let row = RowRef::parse(row).map_err(&bad)?;
            Op::Line(row, field.to_string(), v)
        }
        "charge" => match middle(rest, path)? {
            (name, "applied") => {
                let on = match value {
                    "true" | "yes" | "on" => true,
                    "false" | "no" | "off" => false,
                    _ => return Err(bad(format!("`{value}` is not true or false"))),
                };
                Op::Charge(name.to_string(), on)
            }
            (_, other) => return Err(bad(format!("a charge has no field `{other}`"))),
        },
        _ => return Err(bad(format!("`{path}` is not a path `method set` knows"))),
    })
}

/// `s` as a script word.
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &[], &["--dry-run"], USAGE)?;
    let p = f.exactly(3, "the budget file, a path and a value", USAGE)?;
    let op = op_for(&p[1], &p[2])?;
    script::run(&p[0], &[(format!("set {}", p[1]), op)], f.has("--dry-run"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(op_for("title", "X").unwrap(), Op::Title("X".into()));
        assert_eq!(
            op_for("global.DAYS", "30").unwrap(),
            Op::Global("DAYS".into(), "30".into())
        );
        assert_eq!(
            op_for("fringe.My.Fringe.rate", "5").unwrap(),
            Op::FringeRate("My.Fringe".into(), "5".into())
        );
        assert_eq!(
            op_for("fringe.FICA.cutoff", "1,000.50").unwrap(),
            Op::FringeCutoff("FICA".into(), Some(100_050))
        );
        assert_eq!(
            op_for("fringe.FICA.cutoff", "").unwrap(),
            Op::FringeCutoff("FICA".into(), None)
        );
        assert_eq!(
            op_for("line.11.00:2.rate", "900").unwrap(),
            Op::Line(
                RowRef::Nth("11.00".into(), 2),
                "rate".into(),
                Word::plain("900")
            )
        );
        assert_eq!(
            op_for("charge.Contingency.applied", "no").unwrap(),
            Op::Charge("Contingency".into(), false)
        );
        for bad in [
            "nope",
            "title.x",
            "line.#1",
            "line.#1.colour",
            "fringe.F.kind",
            "charge.C.x",
        ] {
            assert!(op_for(bad, "1").is_err(), "{bad}");
        }
    }
}
