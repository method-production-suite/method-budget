//! `method totals`: the budget's totals for scripts and pipelines.

use std::fmt::Write as _;

use method_core::{recalc, Budget, Cents, Report};
use method_format::export::money_text;

use crate::cli::{excluding, json_string, load, CliError, Flags, Outcome};

pub const USAGE: &str = "  method totals <file.mthd> [--json] [--exclude GROUP]...
      Print the totals, one `name<TAB>amount` line each (exact decimals), with
      the categories as `category<TAB>code<TAB>amount`. --json prints one JSON
      object with every amount in integer cents and the accounts too.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &["--exclude"], &["--json"], USAGE)?;
    let path = &f.exactly(1, "the budget file", USAGE)?[0];
    let opts = excluding(&f);
    let b = load(path)?;
    let r = recalc(&b, &opts).map_err(|e| CliError::failed(format!("{path}: {e}")))?;
    Ok(if f.has("--json") {
        json(&b, &r)
    } else {
        text(&r)
    })
}

/// The headline totals, in the order they print.
fn headline(r: &Report) -> Vec<(&'static str, Cents)> {
    vec![
        ("total_atl", r.total_atl),
        ("total_btl", r.total_btl),
        ("atl_btl", r.atl_btl),
        ("budget_fringes", r.budget_fringes),
        ("charges_total", r.charges.iter().map(|c| c.amount).sum()),
        ("grand_total", r.grand_total),
        ("credits_total", r.credits_total),
        ("net_total", r.net_total),
    ]
}

/// Tab-separated text: categories first, then the headline totals.
pub fn text(r: &Report) -> String {
    let mut out = String::new();
    for c in &r.categories {
        let _ = writeln!(out, "category\t{}\t{}", c.code, money_text(c.total));
    }
    for (name, amount) in headline(r) {
        let _ = writeln!(out, "{name}\t{}", money_text(amount));
    }
    out
}

/// One JSON object (keys in a fixed order, money as integer cents, as in the
/// file format).
pub fn json(b: &Budget, r: &Report) -> String {
    let mut o = String::from("{\n");
    let _ = writeln!(o, "  \"title\": {},", json_string(&b.title));
    let _ = writeln!(o, "  \"base_currency\": {},", json_string(&r.base_currency));
    let _ = writeln!(
        o,
        "  \"totals_currency\": {},",
        json_string(&r.totals_currency)
    );
    o.push_str("  \"categories\": [");
    for (i, c) in r.categories.iter().enumerate() {
        o.push_str(if i == 0 { "\n" } else { ",\n" });
        let _ = write!(
            o,
            "    {{\"code\": {}, \"name\": {}, \"total\": {}, \"fringe_total\": {}, \"accounts\": [",
            json_string(&c.code),
            json_string(&c.name),
            c.total,
            c.fringe_total
        );
        for (j, a) in c.accounts.iter().enumerate() {
            if j > 0 {
                o.push_str(", ");
            }
            let _ = write!(
                o,
                "{{\"code\": {}, \"name\": {}, \"total\": {}}}",
                json_string(&a.code),
                json_string(&a.name),
                a.total
            );
        }
        o.push_str("]}");
    }
    o.push_str(if r.categories.is_empty() {
        "],\n"
    } else {
        "\n  ],\n"
    });
    let named = |o: &mut String, key: &str, items: Vec<(&str, Cents)>| {
        let _ = write!(o, "  \"{key}\": [");
        for (i, (name, amount)) in items.iter().enumerate() {
            if i > 0 {
                o.push_str(", ");
            }
            let _ = write!(
                o,
                "{{\"name\": {}, \"amount\": {amount}}}",
                json_string(name)
            );
        }
        o.push_str("],\n");
    };
    named(
        &mut o,
        "levels",
        r.levels
            .iter()
            .filter(|l| !l.name.is_empty())
            .map(|l| (l.name.as_str(), l.total))
            .collect(),
    );
    named(
        &mut o,
        "charges",
        r.charges
            .iter()
            .map(|c| (c.name.as_str(), c.amount))
            .collect(),
    );
    named(
        &mut o,
        "credits",
        r.credits
            .iter()
            .map(|c| (c.name.as_str(), c.amount))
            .collect(),
    );
    let lines = headline(r);
    for (name, amount) in &lines {
        let _ = writeln!(o, "  \"{name}\": {amount},");
    }
    match r.in_totals_currency(r.net_total) {
        Some(n) => {
            let _ = writeln!(o, "  \"net_total_in_totals_currency\": {n}");
        }
        None => o.push_str("  \"net_total_in_totals_currency\": null\n"),
    }
    o.push_str("}\n");
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::{starter, RecalcOptions};

    #[test]
    fn text_and_json_agree() {
        let b = starter::budget();
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let t = text(&r);
        let net = format!("net_total\t{}\n", money_text(r.net_total));
        assert!(t.ends_with(&net), "{t}");
        let j = json(&b, &r);
        assert!(
            j.contains(&format!("\"net_total\": {},", r.net_total)),
            "{j}"
        );
        assert!(balanced(&j), "{j}");
    }

    /// Brackets and quotes balance (the CLI has no JSON parser to check with).
    pub fn balanced(s: &str) -> bool {
        let mut depth = 0i32;
        let (mut in_str, mut esc) = (false, false);
        for c in s.chars() {
            if in_str {
                match (esc, c) {
                    (true, _) => esc = false,
                    (false, '\\') => esc = true,
                    (false, '"') => in_str = false,
                    _ => {}
                }
                continue;
            }
            match c {
                '"' => in_str = true,
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                _ => {}
            }
            if depth < 0 {
                return false;
            }
        }
        depth == 0 && !in_str
    }
}
