//! `method import`: spreadsheet rows into a budget with a column mapping
//! (decision 36), without the app.

use std::fmt::Write as _;
use std::path::Path;

use fwb_core::{recalc, Budget, RecalcOptions};
use fwb_format::import::{mapping_file, merge, read_file};

use crate::cli::{load, money, save, CliError, Flags, Outcome};

pub const USAGE: &str =
    "  method import <sheet.xlsx|.csv|.txt> -o <out.mthd> [--mapping <mapping.json>]
                [--into <budget.mthd>] [--title TEXT] [--currency CODE] [--force] [--dry-run]
      Add a sheet's rows to a budget as detail lines. --mapping names the
      sheet, header row and columns (docs/CLI.md); without it the header row is
      found and every column guessed from its title. --into imports into a
      copy of that budget (its chart and setup give the lines their meaning);
      without it the lines go into a new empty budget (use create_accounts in
      the mapping). Writes -o atomically; an existing -o is refused unless it
      is the --into budget or --force is given. --dry-run writes nothing.
";

/// How many skipped rows, warnings and mismatches are listed.
const LISTED: usize = 20;

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(
        args,
        &["--out|-o", "--mapping", "--into", "--title", "--currency"],
        &["--force", "--dry-run"],
        USAGE,
    )?;
    let sheet_path = &f.exactly(1, "the spreadsheet file", USAGE)?[0];
    let out = f
        .value("--out")
        .ok_or_else(|| CliError::usage("import needs -o <out.mthd>", USAGE))?;
    let dry_run = f.has("--dry-run");

    let mapping = match f.value("--mapping") {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?;
            mapping_file::parse(&text).map_err(|e| CliError::failed(format!("{p}: {e}")))?
        }
        None => mapping_file::parse("{}").expect("an empty mapping file is valid"),
    };
    let bytes = std::fs::read(sheet_path).map_err(|e| format!("{sheet_path}: {e}"))?;
    let file_name = Path::new(sheet_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let book = read_file(&bytes, &file_name).map_err(|e| format!("{sheet_path}: {e}"))?;
    let (table, m) = mapping
        .resolve(&book)
        .map_err(|e| CliError::failed(format!("{sheet_path}: {e}")))?;

    let mut b = match f.value("--into") {
        Some(p) => load(p)?,
        None => {
            let stem = Path::new(sheet_path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Imported".into());
            Budget::new(stem, f.value("--currency").unwrap_or("USD"))
        }
    };
    if let Some(t) = f.value("--title") {
        b.title = t.to_string();
    }

    // Refuse to replace a file by accident before doing the work.
    let out_path = Path::new(out);
    if !dry_run && out_path.exists() && !f.has("--force") {
        let same_as_into = f.value("--into").is_some_and(|p| {
            let a = std::fs::canonicalize(p).ok();
            a.is_some() && a == std::fs::canonicalize(out_path).ok()
        });
        if !same_as_into {
            return Err(CliError::failed(format!(
                "{out} already exists; pass --force to replace it"
            )));
        }
    }

    let preview = merge::preview(&b, table, &m, &mapping.options);
    let (plan, applied) = merge::import(&mut b, table, &m, &mapping.options).map_err(|e| {
        let mut msg = format!("{sheet_path}: {e}");
        for n in preview.plan.skipped.iter().take(5) {
            let _ = write!(msg, "\n  row {}: {}", n.row, n.message);
        }
        if f.value("--into").is_none() && !mapping.options.create_accounts {
            msg.push_str(
                "\n(a new budget has no accounts: pass --into <budget.mthd>, or set \
                 \"create_accounts\": true in the mapping)",
            );
        }
        CliError::failed(msg)
    })?;
    let r = recalc(&b, &RecalcOptions::default()).map_err(|e| {
        CliError::failed(format!(
            "{sheet_path}: the imported budget doesn't recalculate: {e}"
        ))
    })?;

    let mut report = String::new();
    let _ = writeln!(
        report,
        "{} lines from sheet \"{}\" into {} accounts",
        applied.row_ids.len(),
        table.name,
        plan.matched_accounts.len() + plan.new_accounts.len()
    );
    if applied.categories + applied.accounts + applied.setup > 0 {
        let _ = writeln!(
            report,
            "added {} categories, {} accounts, {} setup items",
            applied.categories, applied.accounts, applied.setup
        );
    }
    let list = |report: &mut String, title: &str, notes: &[merge::RowNote]| {
        if notes.is_empty() {
            return;
        }
        let _ = writeln!(report, "{title} ({}):", notes.len());
        for n in notes.iter().take(LISTED) {
            let _ = writeln!(report, "  row {}: {}", n.row, n.message);
        }
        if notes.len() > LISTED {
            let _ = writeln!(report, "  ... and {} more", notes.len() - LISTED);
        }
    };
    list(&mut report, "skipped rows", &plan.skipped);
    list(&mut report, "warnings", &plan.warnings);
    if let Some(rec) = &preview.reconciliation {
        let _ = writeln!(
            report,
            "line totals checked: {}, different: {} (sheet {}, budget {})",
            rec.compared,
            rec.mismatched,
            money(rec.sheet_sum),
            money(rec.computed_sum)
        );
        for mm in rec.first.iter().take(LISTED) {
            let _ = writeln!(
                report,
                "  row {} ({} {}): sheet {}, budget {}",
                mm.row,
                mm.account,
                mm.description,
                money(mm.sheet),
                money(mm.computed)
            );
        }
    }
    let _ = writeln!(report, "grand total {}", money(r.grand_total));
    if dry_run {
        report.push_str("dry run: nothing written\n");
    } else {
        save(out, &b)?;
        let _ = writeln!(report, "wrote {out}");
    }
    Ok(report)
}
