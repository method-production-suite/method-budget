//! `method-budget schedule-days`: a schedule's day counts set a budget's
//! globals, as the app's File > Import Schedule Days… does (roadmap 3.8;
//! the logic is method-session's `schedule_days`). A dry run unless
//! `--apply`.

use std::fmt::Write as _;
use std::path::Path;

use method_format::serde_json::{self, json, Value};
use method_session::schedule_days::{self as sd, FigureRow, Review};

use crate::cli::{load, save, CliError, Flags, Outcome};

pub const USAGE: &str =
    "  method-budget schedule-days <budget.mthd> <days.json> [--pick KEY[=GLOBAL]]...
                [--skip KEY]... [--apply] [--json]
      Set globals from a schedule's day counts (the days file Method Schedule
      writes for a budget). Lists every figure the file counts with the global
      it would set, that global's value now and the new one. Shoot days in all
      go to SHOOT_DAYS; any other figure only when picked: --pick KEY, KEY as
      listed (total, unit:NAME, set:NAME, location:NAME, week:YYYY-MM-DD,
      element:ID), =GLOBAL to name the global (else its default name).
      Globals an earlier import set are picked again; --skip KEY leaves one
      out. Prep and wrap are never touched. Nothing is written unless --apply,
      which saves the budget atomically. --json prints the review as JSON.
      Exit 3 when a pick is refused.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &["--pick", "--skip"], &["--apply", "--json"], USAGE)?;
    let p = f.exactly(2, "the budget file and the schedule days file", USAGE)?;
    let (path, days_path) = (&p[0], &p[1]);
    let mut b = load(path)?;
    let bytes = std::fs::read(days_path).map_err(|e| format!("{days_path}: {e}"))?;
    let file_name = Path::new(days_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let days = sd::read(&file_name, &bytes).map_err(CliError::failed)?;

    let add: Vec<(String, Option<String>)> =
        f.values("--pick").into_iter().map(split_pick).collect();
    let skip: Vec<String> = f.values("--skip").into_iter().map(str::to_string).collect();
    let picks = sd::picks_with(&b, &days, &add, &skip).map_err(CliError::rejected)?;
    let mut review = sd::review(&b, &days, Some(&picks));
    review.file = file_name;
    if !review.problems.is_empty() {
        return Err(CliError::rejected(format!(
            "{days_path}: {}; nothing was written",
            review.problems.join("; ")
        )));
    }

    let apply = f.has("--apply");
    let mut message = None;
    if apply {
        let done = sd::apply(&mut b, &days, Some(&picks)).map_err(CliError::rejected)?;
        method_core::recalc(&b, &method_core::RecalcOptions::default()).map_err(|e| {
            CliError::rejected(format!(
                "{path}: after setting the globals the budget doesn't recalculate: {e}; nothing was written"
            ))
        })?;
        save(path, &b)?;
        message = Some(done.message());
    }

    if f.has("--json") {
        let mut v = serde_json::to_value(&review).map_err(|e| e.to_string())?;
        if let Value::Object(m) = &mut v {
            m.remove("token");
            m.insert(
                "written".into(),
                if apply { json!(path) } else { Value::Null },
            );
            m.insert("message".into(), json!(message));
        }
        let mut s = serde_json::to_string_pretty(&v).unwrap_or_default();
        s.push('\n');
        return Ok(s);
    }
    let mut out = describe(&review);
    match message {
        Some(m) => {
            let _ = writeln!(out, "{m}");
            let _ = writeln!(out, "wrote {path}");
        }
        None => out.push_str("dry run: nothing written (pass --apply to set the globals)\n"),
    }
    Ok(out)
}

/// `KEY=GLOBAL` or `KEY`; the global after the last `=`.
fn split_pick(v: &str) -> (String, Option<String>) {
    match v.rsplit_once('=') {
        Some((k, g)) if !k.is_empty() => (k.to_string(), Some(g.trim().to_string())),
        _ => (v.to_string(), None),
    }
}

/// The review in words: the file, what would be set, then what wasn't
/// picked with its key.
pub fn describe(r: &Review) -> String {
    let mut s = String::new();
    let where_ = [r.board.trim(), r.sub_board.trim()]
        .iter()
        .filter(|x| !x.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let _ = write!(
        s,
        "{}: {}{}, {} shoot day{}",
        r.file,
        r.schedule,
        if where_.is_empty() {
            String::new()
        } else {
            format!(" ({where_})")
        },
        r.shoot_days,
        if r.shoot_days == 1 { "" } else { "s" }
    );
    match &r.end {
        Some(end) if end != &r.start => {
            let _ = write!(s, ", {} to {end}", r.start);
        }
        _ if r.shoot_days > 0 => {
            let _ = write!(s, ", {}", r.start);
        }
        _ => {}
    }
    s.push('\n');
    for n in &r.notes {
        let _ = writeln!(s, "note: {n}");
    }
    if !r.changed.is_empty() {
        let _ = writeln!(
            s,
            "the schedule changed since these globals were set from it: {}",
            r.changed.join(", ")
        );
    }
    let picked: Vec<&FigureRow> = r.figures.iter().filter(|f| f.picked).collect();
    if picked.is_empty() {
        s.push_str("nothing picked\n");
    } else {
        s.push_str("globals:\n");
        for f in picked {
            let was = match (&f.current, f.action) {
                (None, _) => "new".to_string(),
                (Some(_), "same") => "already set".to_string(),
                (Some(c), _) => format!("was {c}"),
            };
            let _ = writeln!(
                s,
                "  {} = {} ({was}{})  {}  [{}]",
                f.global,
                f.new,
                if f.changed {
                    ", changed in the schedule"
                } else {
                    ""
                },
                f.label,
                f.key
            );
        }
    }
    let rest: Vec<&FigureRow> = r.figures.iter().filter(|f| !f.picked).collect();
    if !rest.is_empty() {
        s.push_str("not picked (--pick KEY[=GLOBAL] to set one):\n");
        for f in rest {
            let _ = writeln!(
                s,
                "  {}  {}: {} day{}  -> {}{}",
                f.key,
                f.label,
                f.days,
                if f.days == 1 { "" } else { "s" },
                f.global,
                match &f.current {
                    Some(c) => format!(" (now {c})"),
                    None => String::new(),
                }
            );
        }
    }
    let _ = writeln!(
        s,
        "{} new, {} changed, {} already set",
        r.adds, r.updates, r.same
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_split_at_the_last_equals() {
        assert_eq!(split_pick("total"), ("total".into(), None));
        assert_eq!(
            split_pick("location:A=B=CAMP"),
            ("location:A=B".into(), Some("CAMP".into()))
        );
        assert_eq!(split_pick("=X"), ("=X".into(), None));
    }
}
