//! `method set`: change one value in a budget from the command line.

use crate::cli::{CliError, Flags, Outcome};
use crate::script::{self, Op, RowRef, Word, LINE_FIELDS};

pub const USAGE: &str = "  method set <file.mthd> <path> <value> [--dry-run]
      Change one value and save atomically. Paths: title, compact_versions,
      arithmetic (mmb or exact), trailing_level_fringes (keep or drop),
      group_rules.ungrouped_lines, group_rules.conflict and
      group_rules.fringes_in_totals (include or exclude),
      global.NAME, fringe.NAME.rate, fringe.NAME.cutoff, fringe.NAME.basis
      (amount, amount_counts or quantity), fringe.NAME.max_hours_per_day,
      line.ROW.FIELD (ROW is #ID, ACCOUNT:N or ACCOUNT:*; FIELD as in `set
      line`), charge.NAME.applied.
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
        "compact_versions" if rest.is_empty() => {
            Op::CompactVersions(script::flag(&v).map_err(bad)?)
        }
        "arithmetic" if rest.is_empty() => Op::Arithmetic(script::arithmetic(value).map_err(bad)?),
        "trailing_level_fringes" if rest.is_empty() => {
            Op::TrailingLevelFringes(script::trailing(value).map_err(bad)?)
        }
        "global" if !rest.is_empty() => Op::Global(rest.to_string(), value.to_string()),
        "fringe" => match middle(rest, path)? {
            (name, "rate") => Op::FringeRate(name.to_string(), value.to_string()),
            (name, field @ ("cutoff" | "basis" | "max_hours_per_day")) => {
                // The script parser handles amounts, hours, bases and
                // `none` (or "").
                let line = format!(
                    "set fringe {} {field} {}",
                    quote(name),
                    if v.is_none() {
                        "none".into()
                    } else {
                        quote(value)
                    }
                );
                match script::parse(&line).map_err(CliError::rejected)? {
                    Some(
                        op @ (Op::FringeCutoff(..) | Op::FringeBasis(..) | Op::FringeMaxHours(..)),
                    ) => op,
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
            (name, "applied") => Op::Charge(name.to_string(), script::flag(&v).map_err(bad)?),
            (_, other) => return Err(bad(format!("a charge has no field `{other}`"))),
        },
        "group_rules" if !rest.is_empty() => script::group_rule(rest, value).map_err(bad)?,
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
        for clear in ["", "none"] {
            assert_eq!(
                op_for("fringe.FICA.cutoff", clear).unwrap(),
                Op::FringeCutoff("FICA".into(), None)
            );
        }
        assert_eq!(
            op_for("fringe.MEALS.basis", "amount_counts").unwrap(),
            Op::FringeBasis("MEALS".into(), method_core::FlatBasis::AmountCounts)
        );
        assert_eq!(
            op_for("fringe.PEN.max_hours_per_day", "12.5").unwrap(),
            Op::FringeMaxHours("PEN".into(), Some("12.5".parse().unwrap()))
        );
        for clear in ["", "none"] {
            assert_eq!(
                op_for("fringe.PEN.max_hours_per_day", clear).unwrap(),
                Op::FringeMaxHours("PEN".into(), None)
            );
        }
        assert!(op_for("fringe.MEALS.basis", "none").is_err());
        assert!(op_for("fringe.MEALS.basis", "hours").is_err());
        assert_eq!(
            op_for("compact_versions", "on").unwrap(),
            Op::CompactVersions(true)
        );
        for (path, value) in [("compact_versions", "maybe"), ("charge.C.applied", "2")] {
            let e = op_for(path, value).unwrap_err();
            assert_eq!(e.code, crate::cli::exit::USAGE);
            assert!(e
                .message
                .starts_with(&format!("`{value}` is not true or false")));
        }
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
        assert_eq!(
            op_for("arithmetic", "exact").unwrap(),
            Op::Arithmetic(method_core::Arithmetic::Exact)
        );
        assert_eq!(
            op_for("arithmetic", "mmb").unwrap(),
            Op::Arithmetic(method_core::Arithmetic::Mmb)
        );
        assert!(op_for("arithmetic", "float").is_err());
        assert_eq!(
            op_for("trailing_level_fringes", "drop").unwrap(),
            Op::TrailingLevelFringes(method_core::TrailingFringes::Drop)
        );
        assert!(op_for("trailing_level_fringes", "lose").is_err());
        assert_eq!(
            op_for("group_rules.ungrouped_lines", "exclude").unwrap(),
            Op::GroupRule("ungrouped_lines", method_core::Inclusion::Exclude)
        );
        assert_eq!(
            op_for("group_rules.conflict", "include").unwrap(),
            Op::GroupRule("conflict", method_core::Inclusion::Include)
        );
        assert!(op_for("group_rules.conflict", "maybe").is_err());
        assert!(op_for("group_rules.nope", "include").is_err());
        assert!(op_for("group_rules", "include").is_err());
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
