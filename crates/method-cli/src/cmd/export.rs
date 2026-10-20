//! `method-budget export`: the evaluated budget as an Excel workbook, CSV files or
//! tab-delimited text (decision 30).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use method_format::export::{self, Format, Options};

use crate::cli::{load, not_the_input, write_atomic, CliError, Flags, Outcome};

pub const USAGE: &str = "  method-budget export <file.mthd> --xlsx|--csv|--tsv <path> [--bom]
  method-budget export <file.mthd> --format xlsx|csv|tsv --out <path> [--bom]
      Export the evaluated budget: xlsx is one workbook (Topsheet, Accounts,
      Detail and the setup tables); csv is one file per table, named
      `<stem> - <Table>.csv` from the path (a folder: the budget's file name);
      tsv is the Detail table as tab-delimited text for accounting tools.
      --bom starts CSV and TSV files with a UTF-8 byte order mark (for spreadsheet apps that need it).
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(
        args,
        &["--format", "--out|-o", "--xlsx", "--csv", "--tsv"],
        &["--bom"],
        USAGE,
    )?;
    let input = &f.exactly(1, "the budget file", USAGE)?[0];

    // `--xlsx PATH` is `--format xlsx --out PATH`, and so on.
    let mut chosen: Vec<(Format, &str)> = Vec::new();
    for (flag, format) in [
        ("--xlsx", Format::Xlsx),
        ("--csv", Format::Csv),
        ("--tsv", Format::Tsv),
    ] {
        if let Some(p) = f.value(flag) {
            chosen.push((format, p));
        }
    }
    if let Some(name) = f.value("--format") {
        let format = Format::parse(name).ok_or_else(|| {
            CliError::usage(
                format!("unknown export format `{name}`: use xlsx, csv or tsv"),
                USAGE,
            )
        })?;
        let out = f
            .value("--out")
            .ok_or_else(|| CliError::usage("export needs --out", USAGE))?;
        chosen.push((format, out));
    } else if f.value("--out").is_some() {
        return Err(CliError::usage("export needs --format", USAGE));
    }
    let (format, out) = match chosen.as_slice() {
        [one] => *one,
        [] => {
            return Err(CliError::usage(
                "export needs --xlsx, --csv or --tsv (or --format)",
                USAGE,
            ))
        }
        _ => return Err(CliError::usage("export takes one format at a time", USAGE)),
    };
    let opts = Options {
        bom: f.has("--bom"),
    };

    let mut out = PathBuf::from(out);
    if out.is_dir() {
        // A folder: name the export after the budget's file.
        let stem = Path::new(input)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Budget".into());
        out = out.join(format!("{stem}.{}", format.extension()));
    }

    let b = load(input)?;
    let wb = export::workbook_of(&b).map_err(|e| CliError::failed(format!("{input}: {e}")))?;
    let files =
        export::render(&wb, format, &out, opts).map_err(|e| CliError::failed(e.to_string()))?;
    // Never write over the budget itself.
    for file in &files {
        not_the_input(input, &file.path, "exported")?;
    }
    let mut report = String::new();
    for file in &files {
        write_atomic(&file.path, &file.bytes)?;
        let _ = writeln!(report, "wrote {}", file.path.display());
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{exit, save};
    use method_core::starter;

    #[test]
    fn export_writes_each_format() {
        let dir = std::env::temp_dir().join(format!("method-cli-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("Film.mthd");
        let input_s = input.to_string_lossy().into_owned();
        save(&input_s, &starter::budget()).unwrap();
        let args = |rest: &[&str]| -> Vec<String> {
            [input_s.as_str()]
                .iter()
                .chain(rest)
                .map(|s| s.to_string())
                .collect()
        };
        let dir_s = dir.to_string_lossy().into_owned();

        // A folder: named after the budget's file.
        let out = run(&args(&["--format", "xlsx", "--out", &dir_s])).unwrap();
        assert!(out.contains("Film.xlsx"), "{out}");
        let bytes = std::fs::read(dir.join("Film.xlsx")).unwrap();
        method_format::export::xlsx::verify(&bytes, 10).unwrap();

        let csv_out = dir.join("Out.csv").to_string_lossy().into_owned();
        let out = run(&args(&["--bom", "--csv", &csv_out])).unwrap();
        assert_eq!(out.lines().count(), 10, "{out}");
        let top = std::fs::read(dir.join("Out - Topsheet.csv")).unwrap();
        assert!(top.starts_with(b"\xEF\xBB\xBFType,Code,Description,Total\r\n"));

        let tsv_out = dir.join("Detail.txt").to_string_lossy().into_owned();
        run(&args(&["--format", "tsv", "--out", &tsv_out])).unwrap();
        let tsv = std::fs::read_to_string(dir.join("Detail.txt")).unwrap();
        assert!(tsv.starts_with("Category\tAccount\tAccount Name\tLine\t"));

        let x_out = dir.join("Short.xlsx").to_string_lossy().into_owned();
        run(&args(&["--xlsx", &x_out])).unwrap();
        assert!(dir.join("Short.xlsx").exists());

        // Never over the budget itself; flags are checked.
        let e = run(&args(&["--format", "xlsx", "--out", &input_s])).unwrap_err();
        assert!(e.message.contains("budget being exported"), "{e}");
        assert_eq!(e.code, exit::FAILED);
        assert!(load(&input_s).is_ok());
        let e = run(&args(&["--format", "xls", "--out", &tsv_out])).unwrap_err();
        assert!(e.message.contains("unknown export format"));
        assert_eq!(e.code, exit::USAGE);
        assert!(run(&args(&["--out", &tsv_out]))
            .unwrap_err()
            .message
            .contains("needs --format"));
        assert!(run(&args(&["--format"]))
            .unwrap_err()
            .message
            .contains("needs a value"));
        assert!(run(&args(&["--xlsx", &x_out, "--csv", &csv_out]))
            .unwrap_err()
            .message
            .contains("one format at a time"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
