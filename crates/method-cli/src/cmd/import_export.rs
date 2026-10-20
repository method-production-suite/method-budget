//! `method-budget import-export`: a whole budget from another budgeting program's
//! open export (budget exchange JSON or XML, its Excel or tab-delimited
//! export), checked against the export's own figures (roadmap 1.16).

use std::fmt::Write as _;
use std::path::Path;

use method_format::import::exchange::{
    self, summary_lines, ExportFormat, Imported, Level, NoteKind, Reconciliation,
};
use method_format::serde_json::{json, Value};

use crate::cli::{save, CliError, Flags, Outcome};

pub const USAGE: &str =
    "  method-budget import-export <export-file> [-o <out.mthd>] [--format F] [--title TEXT]
                [--report] [--json] [--force]
      Make a budget from another program's budget export (budget exchange
      JSON or XML, or its .xlsx spreadsheet or tab-delimited export; --format json, xml,
      xml-advanced, xlsx or tab when the file doesn't say) and compare the
      budget's totals with the export's own, line by line. Prints what came
      over, what was worked out and what couldn't come over, then every
      figure that differs (--report lists them all, not the first 20).
      Writes -o atomically (refused when it exists unless --force); without
      -o nothing is written. --json prints the same as JSON.
";

/// How many differences are listed without --report.
const LISTED: usize = 20;

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(
        args,
        &["--out|-o", "--format", "--title"],
        &["--report", "--json", "--force"],
        USAGE,
    )?;
    let path = &f.exactly(1, "the export file", USAGE)?[0];
    let format = match f.value("--format") {
        Some(v) => Some(ExportFormat::parse(v).ok_or_else(|| {
            CliError::usage(
                format!("--format must be json, xml, xml-advanced, xlsx or tab, not `{v}`"),
                USAGE,
            )
        })?),
        None => None,
    };
    let out = f.value("--out");
    if let Some(o) = out {
        if Path::new(o).exists() && !f.has("--force") {
            return Err(CliError::failed(format!(
                "{o} already exists; pass --force to replace it"
            )));
        }
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let file_name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (mut im, mut rec) = exchange::import(&bytes, &file_name, format)
        .map_err(|e| CliError::failed(format!("{path}: {e}")))?;
    if let Some(t) = f.value("--title") {
        im.budget.title = t.to_string();
        rec = exchange::reconcile(&im);
    }
    if let Some(o) = out {
        save(o, &im.budget)?;
    }
    if f.has("--json") {
        let mut v = to_json(
            &im,
            &rec,
            if f.has("--report") {
                None
            } else {
                Some(LISTED)
            },
        );
        if let (Some(o), Value::Object(m)) = (out, &mut v) {
            m.insert("written".into(), json!(o));
        }
        let mut s = method_format::serde_json::to_string_pretty(&v).unwrap_or_default();
        s.push('\n');
        return Ok(s);
    }
    let mut report = describe(&im, &file_name);
    report.push_str(&format!(
        "Compared with the export's figures ({}):\n",
        rec.currency
    ));
    let limit = if f.has("--report") {
        None
    } else {
        Some(LISTED)
    };
    for l in summary_lines(&rec, limit) {
        let _ = writeln!(report, "  {l}");
    }
    match out {
        Some(o) => {
            let _ = writeln!(report, "wrote {o}");
        }
        None => report.push_str("nothing written (give -o <out.mthd> to write the budget)\n"),
    }
    Ok(report)
}

/// What came over and the notes, in words.
pub fn describe(im: &Imported, file_name: &str) -> String {
    let c = &im.counts;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{file_name} ({}): \"{}\", {} categories, {} accounts, {} lines{}{}{}",
        im.format.label(),
        im.budget.title,
        c.categories,
        c.accounts,
        c.lines,
        match c.adjustments {
            0 => String::new(),
            1 => " (1 adjustment row)".into(),
            n => format!(" ({n} adjustment rows)"),
        },
        match c.charges {
            0 => String::new(),
            1 => ", 1 charge".into(),
            n => format!(", {n} charges"),
        },
        match c.credits {
            0 => String::new(),
            1 => ", 1 credit".into(),
            n => format!(", {n} credits"),
        },
    );
    let _ = writeln!(
        s,
        "setup: {} fringes, {} groups, {} sets, {} locations, {} currencies",
        c.fringes, c.groups, c.sets, c.locations, c.currencies
    );
    for kind in [NoteKind::Check, NoteKind::Lost, NoteKind::Info] {
        for n in im.notes.iter().filter(|n| n.kind == kind) {
            let _ = writeln!(s, "{}: {}", kind.label().to_lowercase(), n.text);
        }
    }
    s
}

/// The import and its reconciliation as JSON (the MCP tool gives the same):
/// counts, notes, tallies per kind of figure, and the differences (all, or
/// the first `limit`), money in cents.
pub fn to_json(im: &Imported, rec: &Reconciliation, limit: Option<usize>) -> Value {
    let c = &im.counts;
    let n = rec.differences.len();
    let listed = limit.map_or(n, |l| l.min(n));
    json!({
        "format": im.format.key(),
        "title": im.budget.title,
        "counts": {
            "categories": c.categories,
            "accounts": c.accounts,
            "lines": c.lines,
            "adjustments": c.adjustments,
            "blank_rows_skipped": c.blank_rows,
            "fringes": c.fringes,
            "groups": c.groups,
            "sets": c.sets,
            "locations": c.locations,
            "currencies": c.currencies,
            "charges": c.charges,
            "credits": c.credits,
            "lines_with_globals": c.lines_with_globals,
        },
        "notes": im.notes.iter().map(|n| json!({"kind": n.kind.key(), "text": n.text})).collect::<Vec<_>>(),
        "reconciliation": {
            "currency": rec.currency,
            "matches": rec.matches(),
            "calc_error": rec.calc_error,
            "export_grand_cents": rec.export_grand,
            "grand_cents": rec.grand,
            "export_net_cents": rec.export_net,
            "net_cents": rec.net,
            "tallies": Level::ALL.iter().filter_map(|l| rec.tallies.get(l).map(|t| json!({
                "kind": l.key(),
                "compared": t.compared,
                "different": t.different,
            }))).collect::<Vec<_>>(),
            "differences": rec.differences[..listed].iter().map(|d| json!({
                "kind": d.level.key(),
                "what": d.what,
                "export_cents": d.export,
                "imported_cents": d.ours,
            })).collect::<Vec<_>>(),
            "differences_not_listed": n - listed,
        },
    })
}
