//! `method-budget review`: a review file's comments on a budget, at their
//! places; adding one; and merging them back as notes or dismissing them,
//! as the app's Comments tab does (roadmap 3.3; the logic is
//! method-session's `review`). Merging is a dry run unless `--apply`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use method_format::serde_json;
use method_session::review::{self as rv, Pick, PickAction, ReviewList, Status};
use method_session::Session;

use crate::cli::{load, save, write_atomic, CliError, Flags, Outcome};

pub const USAGE: &str = "  method-budget review <budget.mthd> [<review.json>] [--json]
  method-budget review <budget.mthd> [<review.json>] --add ON --text TEXT
                [--author NAME] [--at TIME]
  method-budget review <budget.mthd> [<review.json>] [--merge ID|all]...
                [--dismiss ID]... [--by NAME] [--at TIME] [--apply] [--json]
      A review file's comments on a budget (<budget>.review.json beside it
      unless named), each at its place, with whether it can become a note.
      --add writes a comment on ON: row:ID, account:NUMBER (or the number),
      category:CODE, fringe:NAME (or global, group, unit, set, location,
      currency, charge, credit) or budget; the review file is made when there
      is none. --merge puts comments into the budget as dated notes (all: every
      one that can be) and --dismiss sets them aside; nothing is written unless
      --apply, which saves the budget and the review file atomically. A comment
      whose place is gone, or on a setup item or the whole budget, stays open.
      --at is the time written (default: now, UTC). Exit 3 when refused.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(
        args,
        &[
            "--add",
            "--text",
            "--author",
            "--at",
            "--merge",
            "--dismiss",
            "--by",
        ],
        &["--apply", "--json"],
        USAGE,
    )?;
    let p = f.positional();
    if p.is_empty() || p.len() > 2 {
        return Err(CliError::usage(
            "give the budget file, and the review file when it isn't beside it",
            USAGE,
        ));
    }
    let path = &p[0];
    let review_path = match p.get(1) {
        Some(r) => PathBuf::from(r),
        None => Path::new(path).with_file_name(rv::file_name_for(
            &Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )),
    };
    let review_name = review_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let b = load(path)?;
    let at = f.value("--at").map_or_else(now_iso, str::to_string);
    let mut s = Session::with_budget(b, Some(PathBuf::from(path)));

    if let Some(on) = f.value("--add") {
        let text = f
            .value("--text")
            .ok_or_else(|| CliError::usage("--add needs --text", USAGE))?;
        let existing = if review_path.is_file() {
            Some(read(&review_path, &review_name)?)
        } else {
            None
        };
        let on = rv::parse_anchor(s.budget(), on).map_err(CliError::rejected)?;
        rv::start_reviewing(&mut s, existing, review_name.clone(), None)
            .map_err(CliError::rejected)?;
        let id = rv::add(&mut s, on, text, &at, f.value("--author")).map_err(CliError::rejected)?;
        let out = rv::text(&s).map_err(CliError::failed)?;
        write_atomic(&review_path, out.as_bytes())?;
        return Ok(format!(
            "added comment {id}\nwrote {}\n",
            review_path.display()
        ));
    }

    let review = read(&review_path, &review_name)?;
    rv::open_for_author(&mut s, review, review_name, None).map_err(CliError::rejected)?;
    let merge = f.values("--merge");
    let dismiss = f.values("--dismiss");
    let mut message = None;
    let mut picks = Vec::new();
    if !merge.is_empty() || !dismiss.is_empty() {
        let list = rv::list(&s).map_err(CliError::failed)?;
        for m in &merge {
            if m.eq_ignore_ascii_case("all") {
                picks.extend(
                    list.entries
                        .iter()
                        .filter(|e| e.can_merge && e.status.to_settle())
                        .map(|e| Pick {
                            id: e.id,
                            action: PickAction::Merge,
                        }),
                );
            } else {
                picks.push(Pick {
                    id: id_of(m)?,
                    action: PickAction::Merge,
                });
            }
        }
        for d in &dismiss {
            picks.push(Pick {
                id: id_of(d)?,
                action: PickAction::Dismiss,
            });
        }
        let done = rv::settle(&mut s, &picks, &at, f.value("--by")).map_err(CliError::rejected)?;
        if f.has("--apply") {
            save(path, s.budget())?;
            let out = rv::text(&s).map_err(CliError::failed)?;
            write_atomic(&review_path, out.as_bytes())?;
        }
        message = Some(done.message);
    }

    let list = rv::list(&s).map_err(CliError::failed)?;
    if f.has("--json") {
        let mut v = serde_json::to_value(&list).map_err(|e| e.to_string())?;
        if let serde_json::Value::Object(m) = &mut v {
            for k in ["rev", "unsaved", "rows", "accounts", "categories", "mode"] {
                m.remove(k);
            }
            m.insert("message".into(), serde_json::json!(message));
        }
        let mut out = serde_json::to_string_pretty(&v).unwrap_or_default();
        out.push('\n');
        return Ok(out);
    }
    let mut out = describe(&list);
    if let Some(m) = message {
        let _ = writeln!(out, "{m}");
        if f.has("--apply") {
            let _ = writeln!(out, "wrote {path} and {}", review_path.display());
        } else {
            out.push_str(
                "dry run: nothing written (pass --apply to write the budget and the review file)\n",
            );
        }
    }
    Ok(out)
}

fn read(path: &Path, name: &str) -> Result<method_session::review::Review, CliError> {
    let bytes =
        std::fs::read(path).map_err(|e| CliError::failed(format!("{}: {e}", path.display())))?;
    rv::read(name, &bytes).map_err(CliError::failed)
}

fn id_of(s: &str) -> Result<u64, CliError> {
    s.trim()
        .parse()
        .map_err(|_| CliError::rejected(format!("\"{s}\" isn't a comment id (or all)")))
}

/// The time now in UTC, as ISO 8601 to the minute.
fn now_iso() -> String {
    let (date, time) = crate::render::now_utc();
    format!("{date}T{}:00Z", time.trim_end_matches(" UTC"))
}

/// The list in words: each comment at its place, then the counts.
pub fn describe(l: &ReviewList) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{}: {} on \"{}\"{}",
        l.brief.file,
        plural(l.brief.comments, "comment", "comments"),
        l.budget.title,
        if l.brief.matches {
            ""
        } else {
            " (a budget with another title)"
        }
    );
    for e in &l.entries {
        let status = match e.status {
            Status::Open => "open",
            Status::Resolved => "resolved",
            Status::Merged => "merged",
            Status::Dismissed => "dismissed",
        };
        let who = [e.author.trim(), e.created.trim()]
            .iter()
            .filter(|x| !x.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            s,
            "  {}. {}{}  [{status}]{}",
            e.id,
            if e.found { "" } else { "gone: " },
            e.place,
            if who.is_empty() {
                String::new()
            } else {
                format!("  ({who})")
            }
        );
        for line in e.text.lines() {
            let _ = writeln!(s, "      {line}");
        }
        if let Some(w) = &e.why_not {
            let _ = writeln!(s, "      ({w})");
        }
    }
    let _ = writeln!(s, "{} to settle", l.brief.to_settle);
    s
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
