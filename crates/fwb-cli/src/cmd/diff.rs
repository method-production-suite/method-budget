//! `method diff`: what changed from one budget to another, in plain words
//! (`fwb_core::diff`; docs/MERGE.md).

use std::fmt::Write as _;

use fwb_core::diff::{diff, BudgetDiff, MoneyLine, Place};

use crate::cli::{exit, json_string, load, CliError, Flags, Outcome};

pub const USAGE: &str = "  method diff <old.mthd> <new.mthd> [--json] [--exit-code]
      What changed from the first budget to the second, in plain words
      (rows by id, accounts by number), with the money that moved per
      account, category and topsheet line. --json prints one JSON object
      (money in integer cents). --exit-code exits 1 when anything changed.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &[], &["--json", "--exit-code"], USAGE)?;
    let files = f.exactly(2, "the two budget files", USAGE)?;
    let (a, b) = (load(&files[0])?, load(&files[1])?);
    let d = diff(&a, &b);
    let text = if f.has("--json") {
        json(&files[0], &files[1], &d)
    } else {
        d.to_text()
    };
    if f.has("--exit-code") && !d.is_empty() {
        // The diff is the output; the exit code says it isn't empty.
        print!("{text}");
        return Err(CliError::new(exit::FAILED, ""));
    }
    Ok(text)
}

fn opt_cents(c: Option<i64>) -> String {
    c.map_or("null".into(), |v| v.to_string())
}

fn opt_str(s: Option<&str>) -> String {
    s.map_or("null".into(), json_string)
}

fn place_json(p: &Place) -> String {
    match p {
        Place::Budget => "{\"kind\": \"budget\"}".into(),
        Place::Setup { table, name } => format!(
            "{{\"kind\": \"setup\", \"table\": {}, \"name\": {}}}",
            json_string(table.singular()),
            json_string(name)
        ),
        Place::Topsheet { code, name, .. } => format!(
            "{{\"kind\": \"topsheet\", \"code\": {}, \"name\": {}}}",
            opt_str(code.as_deref()),
            json_string(name)
        ),
        Place::Account {
            category,
            code,
            name,
            row,
            line,
            ..
        } => format!(
            "{{\"kind\": \"account\", \"category\": {}, \"code\": {}, \"name\": {}, \"row\": {}, \"line\": {}}}",
            json_string(category),
            json_string(code),
            json_string(name),
            row.map_or("null".into(), |r| r.to_string()),
            line.map_or("null".into(), |l| l.to_string())
        ),
        Place::Versions => "{\"kind\": \"versions\"}".into(),
    }
}

fn money_json(lines: &[MoneyLine]) -> String {
    if lines.is_empty() {
        return "[]".into();
    }
    let items: Vec<String> = lines
        .iter()
        .map(|l| {
            format!(
                "    {{\"label\": {}, \"before\": {}, \"after\": {}, \"delta\": {}}}",
                json_string(&l.label),
                opt_cents(l.before),
                opt_cents(l.after),
                l.delta
            )
        })
        .collect();
    format!("[\n{}\n  ]", items.join(",\n"))
}

/// The diff as one JSON object.
pub fn json(from: &str, to: &str, d: &BudgetDiff) -> String {
    let mut o = String::from("{\n");
    let _ = writeln!(o, "  \"from\": {},", json_string(from));
    let _ = writeln!(o, "  \"to\": {},", json_string(to));
    let _ = writeln!(o, "  \"money_known\": {},", d.money_known);
    let changes: Vec<String> = d
        .items
        .iter()
        .map(|i| {
            let fields: Vec<String> = i
                .fields
                .iter()
                .map(|f| {
                    format!(
                        "{{\"field\": {}, \"label\": {}, \"before\": {}, \"after\": {}}}",
                        json_string(f.field),
                        json_string(f.label),
                        json_string(&f.before),
                        json_string(&f.after)
                    )
                })
                .collect();
            format!(
                "    {{\"kind\": {}, \"group\": {}, \"place\": {}, \"subject\": {}, \"text\": {}, \"money\": {}, \"fields\": [{}]}}",
                json_string(i.kind.key()),
                json_string(&i.place.group()),
                place_json(&i.place),
                json_string(&i.subject),
                json_string(&i.text),
                opt_cents(i.money),
                fields.join(", ")
            )
        })
        .collect();
    if changes.is_empty() {
        o.push_str("  \"changes\": [],\n");
    } else {
        let _ = writeln!(o, "  \"changes\": [\n{}\n  ],", changes.join(",\n"));
    }
    let _ = writeln!(o, "  \"accounts\": {},", money_json(&d.accounts));
    let _ = writeln!(o, "  \"categories\": {},", money_json(&d.categories));
    let _ = writeln!(o, "  \"totals\": {}", money_json(&d.totals));
    o.push_str("}\n");
    o
}
