//! `method report`: a printed report without the app, from a saved report
//! preset (or the default setup), as HTML, plain text, CSV or PDF.

use std::fmt::Write as _;
use std::path::Path;

use method_core::{recalc, RecalcOptions, ReportSection, ReportSpec};

use crate::cli::{load, not_the_input, write_atomic, CliError, Flags, Outcome};
use crate::render::{self, Context};

pub const USAGE: &str =
    "  method report <file.mthd> [--preset NAME] [--sections LIST] [--format html|txt|csv|pdf]
                [-o <path>] [--date TEXT] [--bom]
      Draw a report from a saved preset (default: the budget's default preset,
      else the topsheet and the detail). --sections replaces the preset's
      sections (comma-separated: topsheet,accounts,detail,fringes,globals,
      groups,notes,info,chart,units,sets,locations,currencies). The format
      comes from --format, else the extension of -o, else txt. Without -o (or
      with -o -) the report goes to stdout (not pdf). --date sets the printed
      date and time instead of the clock. --bom starts CSV with a byte order mark.
  method report <file.mthd> --list-presets
      List the budget's report presets, the default marked with *.
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Html,
    Txt,
    Csv,
    Pdf,
}

impl Kind {
    fn parse(s: &str) -> Option<Kind> {
        match s.trim().to_ascii_lowercase().as_str() {
            "html" | "htm" => Some(Kind::Html),
            "txt" | "text" => Some(Kind::Txt),
            "csv" => Some(Kind::Csv),
            "pdf" => Some(Kind::Pdf),
            _ => None,
        }
    }
}

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(
        args,
        &["--preset", "--sections", "--format", "--out|-o", "--date"],
        &["--list-presets", "--bom"],
        USAGE,
    )?;
    let input = &f.exactly(1, "the budget file", USAGE)?[0];
    let b = load(input)?;

    if f.has("--list-presets") {
        let mut out = String::new();
        for p in &b.report_presets {
            let mark = if p.default { "*" } else { " " };
            let _ = writeln!(out, "{mark} {}", p.name);
        }
        return Ok(out);
    }

    let mut spec = match f.value("--preset") {
        Some(name) => {
            let key = name.trim().to_lowercase();
            let found = b
                .report_presets
                .iter()
                .find(|p| p.name == name)
                .or_else(|| {
                    b.report_presets
                        .iter()
                        .find(|p| p.name.trim().to_lowercase() == key)
                });
            match found {
                Some(p) => p.spec.clone(),
                None => {
                    let names: Vec<String> = b
                        .report_presets
                        .iter()
                        .map(|p| format!("\"{}\"", p.name))
                        .collect();
                    let have = if names.is_empty() {
                        "the budget has no report presets".to_string()
                    } else {
                        format!("presets: {}", names.join(", "))
                    };
                    return Err(CliError::failed(format!(
                        "{input}: no report preset called \"{name}\" ({have})"
                    )));
                }
            }
        }
        None => b
            .report_presets
            .iter()
            .find(|p| p.default)
            .map(|p| p.spec.clone())
            .unwrap_or_else(ReportSpec::default),
    };
    if let Some(list) = f.value("--sections") {
        let mut sections = Vec::new();
        for name in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let s = ReportSection::from_name(name).ok_or_else(|| {
                CliError::usage(format!("unknown report section `{name}`"), USAGE)
            })?;
            if !sections.contains(&s) {
                sections.push(s);
            }
        }
        spec.sections = sections;
    }

    let out = f.value("--out").filter(|o| *o != "-");
    let kind = match (f.value("--format"), out) {
        (Some(name), _) => Kind::parse(name).ok_or_else(|| {
            CliError::usage(
                format!("unknown report format `{name}`: use html, txt, csv or pdf"),
                USAGE,
            )
        })?,
        (None, Some(path)) => Path::new(path)
            .extension()
            .and_then(|e| Kind::parse(&e.to_string_lossy()))
            .unwrap_or(Kind::Txt),
        (None, None) => Kind::Txt,
    };
    if kind == Kind::Pdf && out.is_none() {
        return Err(CliError::usage("a PDF report needs -o <path>", USAGE));
    }

    let r = recalc(&b, &RecalcOptions::default())
        .map_err(|e| CliError::failed(format!("{input}: {e}")))?;
    let (date, time) = match f.value("--date") {
        Some(d) => match d.split_once(' ') {
            Some((date, time)) => (date.to_string(), time.trim().to_string()),
            None => (d.to_string(), String::new()),
        },
        None => render::now_utc(),
    };
    let ctx = Context {
        file_name: Path::new(input)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        date,
        time,
    };
    let doc = render::build(&b, &r, &spec, &ctx)
        .map_err(|e| CliError::failed(format!("{input}: {e}")))?;
    let bytes = match kind {
        Kind::Html => render::html::to_html(&doc).into_bytes(),
        Kind::Txt => render::text::to_txt(&doc).into_bytes(),
        Kind::Csv => render::text::to_csv(&doc, f.has("--bom")).into_bytes(),
        Kind::Pdf => render::pdf::to_pdf(&doc),
    };
    match out {
        Some(path) => {
            not_the_input(input, Path::new(path), "reported")?;
            write_atomic(Path::new(path), &bytes)?;
            Ok(format!("wrote {path}\n"))
        }
        // Every format but PDF is text.
        None => Ok(String::from_utf8_lossy(&bytes).into_owned()),
    }
}
