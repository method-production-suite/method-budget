//! `method-budget log`: start or inspect a budget's change log (docs/CLI.md;
//! docs/FORMAT.md, "The change log").

use std::fmt::Write as _;
use std::path::Path;

use method_core::log::{ActorId, Change, Ids, Op};
use method_format::log::{self, LogDir, Shared};

use crate::cli::{json_string, CliError, Flags, Outcome};

pub const USAGE: &str = "  method-budget log <file.mthd> [--init] [--last N] [--json]
      The budget's change log (<file.mthd>-log/ beside it): who has
      changed it, the last N changes (10), what is waiting or refused,
      and what to review. --init starts the log from the file as it is.
      --json prints one JSON object.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &["--last"], &["--init", "--json"], USAGE)?;
    let files = f.exactly(1, "the budget file", USAGE)?;
    let path = Path::new(&files[0]);
    let last: usize = match f.value("--last") {
        Some(n) => n
            .parse()
            .map_err(|_| CliError::usage("--last takes a number", USAGE))?,
        None => 10,
    };
    if f.has("--init") {
        let actor = actor_id()?;
        let shared =
            log::init(path, actor, Ids::from_os(), &now()).map_err(|e| CliError::failed(e.0))?;
        log::flush(&shared.log, &shared.dir).map_err(|e| CliError::failed(e.0))?;
        return Ok(format!(
            "Started the change log at {} (budget id {}, actor {}).\n",
            shared.dir.root().display(),
            &shared.info.budget_id[..16],
            actor.short()
        ));
    }
    let dir = LogDir::for_budget(path);
    if !dir.exists() {
        return Err(CliError::failed(format!(
            "{} has no change log (start one with --init)",
            path.display()
        )));
    }
    let actor = actor_id().unwrap_or(ActorId::NONE);
    let (shared, report) =
        log::open(path, actor, Ids::from_os()).map_err(|e| CliError::failed(e.0))?;
    if f.has("--json") {
        Ok(json(&shared, &report, last))
    } else {
        Ok(text(&shared, &report, last))
    }
}

fn text(shared: &Shared, report: &log::LoadReport, last: usize) -> String {
    let mut s = String::new();
    let proj = shared.log.project();
    let _ = writeln!(
        s,
        "Budget {} ({})",
        shared.info.file,
        &shared.info.budget_id[..16]
    );
    let _ = writeln!(s, "Log: {}", shared.dir.root().display());
    let _ = writeln!(
        s,
        "{} changes by {} actors; frontier {}",
        shared.log.len(),
        shared.log.frontier().0.len(),
        shared.log.frontier().digest()
    );
    let _ = writeln!(s, "{}", signed_line(shared));
    let _ = writeln!(s);
    let _ = writeln!(s, "Actors");
    for (actor, seq) in &shared.log.frontier().0 {
        let latest = shared.log.changes().filter(|c| c.actor == *actor).last();
        let me = if *actor == shared.log.actor() {
            " (this computer)"
        } else {
            ""
        };
        match latest {
            Some(c) => {
                let _ = writeln!(
                    s,
                    "  {}  {seq} changes, last {} {:?}{me}",
                    actor.short(),
                    c.time,
                    c.label
                );
            }
            None => {
                let _ = writeln!(s, "  {}  {seq} changes{me}", actor.short());
            }
        }
    }
    if report.pending > 0 || !report.rejected.is_empty() || !report.skipped.is_empty() {
        let _ = writeln!(s);
        if report.pending > 0 {
            let _ = writeln!(
                s,
                "{} changes are waiting for ones that haven't arrived.",
                report.pending
            );
        }
        for (id, why) in &report.rejected {
            let _ = writeln!(s, "Refused {id}: {why}");
        }
        for (path, why) in &report.skipped {
            let _ = writeln!(s, "Skipped {}: {why}", path.display());
        }
    }
    if !proj.review.is_empty() {
        let _ = writeln!(s);
        let _ = writeln!(s, "To review");
        for item in &proj.review {
            let _ = writeln!(s, "  {}", item.sentence);
        }
    }
    let changes: Vec<&Change> = shared.log.changes().collect();
    let shown = &changes[changes.len().saturating_sub(last)..];
    if !shown.is_empty() {
        let _ = writeln!(s);
        let _ = writeln!(s, "Last {} changes", shown.len());
        for c in shown {
            let _ = writeln!(
                s,
                "  {}/{}  {}  {}  ({})",
                c.actor.short(),
                c.seq,
                c.time,
                c.label,
                ops_summary(c)
            );
        }
    }
    s
}

/// Whether changes are signed (decisions.md, entry 89), in a line.
fn signed_line(shared: &Shared) -> String {
    let keys = shared.log.state().keys().len();
    if shared.log.signing_started() {
        format!(
            "Changes are signed; {keys} {} a key in the log.",
            if keys == 1 {
                "actor has"
            } else {
                "actors have"
            }
        )
    } else {
        "Changes aren't signed (they are from the owner's first role on).".into()
    }
}

/// "3 ops: 1 create, 2 set".
fn ops_summary(c: &Change) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for op in &c.ops {
        let name = op.name();
        match counts.iter_mut().find(|(n, _)| *n == name) {
            Some((_, n)) => *n += 1,
            None => counts.push((name, 1)),
        }
    }
    let parts: Vec<String> = counts.iter().map(|(n, k)| format!("{k} {n}")).collect();
    format!("{} ops: {}", c.ops.len(), parts.join(", "))
}

fn json(shared: &Shared, report: &log::LoadReport, last: usize) -> String {
    let proj = shared.log.project();
    let actors: Vec<String> = shared
        .log
        .frontier()
        .0
        .iter()
        .map(|(a, seq)| {
            format!(
                "{{\"actor\": {}, \"changes\": {seq}, \"this_computer\": {}, \"key\": {}}}",
                json_string(&a.hex()),
                *a == shared.log.actor(),
                shared
                    .log
                    .key_of(*a)
                    .map_or_else(|| "null".to_string(), |k| json_string(&k.hex()))
            )
        })
        .collect();
    let review: Vec<String> = proj
        .review
        .iter()
        .map(|r| {
            format!(
                "{{\"kind\": {}, \"sentence\": {}}}",
                json_string(r.review.key()),
                json_string(&r.sentence)
            )
        })
        .collect();
    let changes: Vec<&Change> = shared.log.changes().collect();
    let shown: Vec<String> = changes[changes.len().saturating_sub(last)..]
        .iter()
        .map(|c| {
            format!(
                "{{\"actor\": {}, \"seq\": {}, \"lamport\": {}, \"time\": {}, \"label\": {}, \"ops\": {}}}",
                json_string(&c.actor.hex()),
                c.seq,
                c.lamport,
                json_string(&c.time),
                json_string(&c.label),
                c.ops.len()
            )
        })
        .collect();
    let created = shared
        .log
        .changes()
        .map(|c| {
            c.ops
                .iter()
                .filter(|op| matches!(op, Op::Create { .. }))
                .count()
        })
        .sum::<usize>();
    format!(
        "{{\"file\": {}, \"budget_id\": {}, \"log\": {}, \"changes\": {}, \"entities_created\": {created}, \"frontier\": {}, \"signed\": {}, \"actors\": [{}], \"pending\": {}, \"rejected\": {}, \"skipped\": {}, \"review\": [{}], \"last\": [{}]}}\n",
        json_string(&shared.info.file),
        json_string(&shared.info.budget_id),
        json_string(&shared.dir.root().display().to_string()),
        shared.log.len(),
        json_string(&shared.log.frontier().digest()),
        shared.log.signing_started(),
        actors.join(", "),
        report.pending,
        report.rejected.len(),
        report.skipped.len(),
        review.join(", "),
        shown.join(", ")
    )
}

/// The time now as ISO 8601 UTC, to the second.
pub fn now() -> String {
    let (date, time) = crate::render::now_utc();
    format!("{date}T{}:00Z", time.trim_end_matches(" UTC"))
}

/// This computer's actor id, from `$METHOD_ACTOR_FILE` or
/// `~/.method/actor` (64 hex characters), made the first time it is
/// needed and kept readable by its owner only.
pub fn actor_id() -> Result<ActorId, CliError> {
    let path = match std::env::var_os("METHOD_ACTOR_FILE") {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let home = std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .ok_or_else(|| {
                    CliError::failed(
                        "no home folder to keep the actor id in (set METHOD_ACTOR_FILE)",
                    )
                })?;
            Path::new(&home).join(".method").join("actor")
        }
    };
    if let Ok(text) = std::fs::read_to_string(&path) {
        return ActorId::parse(&text)
            .ok_or_else(|| CliError::failed(format!("{}: not an actor id", path.display())));
    }
    let actor = ActorId::random(&mut Ids::from_os());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, format!("{}\n", actor.hex()))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(actor)
}

/// Opens the log of `path` as this computer, for commands that change it.
/// Opens a shared budget to write to it as a program of this computer's
/// person: an actor of its own, held while the returned lock lives, so the
/// app and other programs never share its sequence of changes.
pub fn open_shared_as_tool(
    path: &Path,
) -> Result<(Shared, log::LoadReport, log::ToolLock), CliError> {
    let person = actor_id()?;
    let file = log::actor_id_path().ok_or_else(|| {
        CliError::failed("no home folder to keep the actor id in (set METHOD_ACTOR_FILE)")
    })?;
    log::open_as_tool(path, &file, person).map_err(|e| CliError::failed(e.0))
}
